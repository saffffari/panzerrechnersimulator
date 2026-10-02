//! Minimal glTF/GLB loader → CPU mesh (positions/normals/uvs/tangents/indices) + optional PBR
//! metallic-roughness material (base color, metallic-roughness, normal — all RGBA8, embedded PNG
//! decoded to raw texels). Bakes node world transforms so a prop lands where authored. The PSX
//! packs and the freshly-baked Tiger both ship self-contained `.glb` (textures embedded), which is
//! exactly what this wants.
//!
//! Meshes without a real PBR material (older bakes like the T-34, or GLBs with only a constant
//! `baseColorFactor`) still come out of here with sane non-PBR defaults — see `CpuMesh::basic` /
//! the `metallic_factor`/`roughness_factor` fallback in `walk` — so the renderer can treat every
//! mesh uniformly (see `renderer::upload_material`'s fallback textures).

use anyhow::Context;
use glam::{Mat3, Mat4, Vec3};
use std::sync::Arc;

#[path = "gltf_material_channels.rs"]
mod material_channels;

#[derive(Clone)]
pub struct CpuImage {
    pub width: u32,
    pub height: u32,
    /// Immutable decoded texels shared by mesh pages and pose variants. `CpuMesh::clone` is used
    /// heavily for anatomical draw ranges; keeping those bytes behind an `Arc` prevents every page
    /// from allocating another 4 MiB crew atlas. Call `rgba_mut` only before a mesh is shared.
    pub rgba: Arc<Vec<u8>>,
}

impl CpuImage {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba: Arc::new(rgba),
        }
    }

    pub fn rgba_mut(&mut self) -> &mut Vec<u8> {
        Arc::make_mut(&mut self.rgba)
    }
}

#[derive(Clone)]
pub struct CpuMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub texture: Option<CpuImage>, // base color (existing)
    pub metallic_roughness_texture: Option<CpuImage>, // glTF convention: G = roughness, B = metallic
    pub normal_texture: Option<CpuImage>,             // tangent-space normal map
    /// Multiplies the metallic-roughness texture's B/G channels (glTF spec: texture × factor).
    /// Imported single-material meshes retain authored scalar factors even without an MR texture.
    /// Merged constant-material pages encode each primitive's factors in a parallel nearest atlas.
    /// Procedural meshes use the explicit defaults in `basic`; mixed textured pages still require
    /// the separate source-material table migration documented in the material audit.
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub min: Vec3,
    pub max: Vec3,
}

/// Non-PBR default: fully rough dielectric, no metal — matches the old flat-Lambert look for
/// meshes that only ever had a flat tint (procs, terrain, legacy tank bakes).
const DEFAULT_METALLIC: f32 = 0.0;
const DEFAULT_ROUGHNESS: f32 = 0.9;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4], // xyz = tangent, w = bitangent handedness sign
}

impl CpuMesh {
    /// Build a mesh with no PBR maps — used by the procedural (box) and terrain meshes that only
    /// ever carry a flat vertex-independent tint texture.
    pub fn basic(
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        texture: Option<CpuImage>,
        min: Vec3,
        max: Vec3,
    ) -> Self {
        Self {
            vertices,
            indices,
            texture,
            metallic_roughness_texture: None,
            normal_texture: None,
            metallic_factor: DEFAULT_METALLIC,
            roughness_factor: DEFAULT_ROUGHNESS,
            min,
            max,
        }
    }
}

#[allow(dead_code)] // scene-framing helpers
impl CpuMesh {
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn radius(&self) -> f32 {
        (self.max - self.min).length() * 0.5
    }

    /// Transform every vertex by `m` (placement onto the world / terrain). Recomputes bounds.
    pub fn transformed(mut self, m: Mat4) -> Self {
        let nm = Mat3::from_mat4(m).inverse().transpose();
        let lm = Mat3::from_mat4(m); // tangents transform with the linear part directly, not N^-T
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for v in &mut self.vertices {
            let p = m.transform_point3(Vec3::from(v.pos));
            v.pos = p.to_array();
            v.normal = (nm * Vec3::from(v.normal)).normalize_or_zero().to_array();
            let t =
                (lm * Vec3::from([v.tangent[0], v.tangent[1], v.tangent[2]])).normalize_or_zero();
            v.tangent = [t.x, t.y, t.z, v.tangent[3]];
            min = min.min(p);
            max = max.max(p);
        }
        self.min = min;
        self.max = max;
        self
    }
}

pub fn load_glb(path: &str) -> anyhow::Result<CpuMesh> {
    Ok(load_glb_partitioned_roots(path, &[])?.remove(0))
}

/// Selects source primitives by their glTF material name. An empty `include` keeps every
/// material not named in `exclude`. Used by the building kit to take, for example, a window wall
/// without its glazing, or a gate leaf without its wall, from one reviewed Pizza GLB.
#[derive(Clone, Copy, Debug, Default)]
pub struct PrimitiveFilter {
    pub include: &'static [&'static str],
    pub exclude: &'static [&'static str],
}

impl PrimitiveFilter {
    pub fn keeps(&self, material: Option<&str>) -> bool {
        let name = material.unwrap_or("");
        (self.include.is_empty() || self.include.contains(&name)) && !self.exclude.contains(&name)
    }
}

/// Load a GLB keeping only the primitives `filter` accepts. Geometry, UVs and the first kept
/// primitive's texture are exactly what `load_glb` would produce for those primitives; nothing is
/// resampled or re-authored. Fails if the filter keeps no geometry.
pub fn load_glb_filtered(path: &str, filter: PrimitiveFilter) -> anyhow::Result<CpuMesh> {
    let bytes = crate::resource::read(path)?;
    let (doc, buffers, images) = gltf::import_slice(&bytes)
        .with_context(|| format!("import filtered glTF resource {path:?}"))?;
    let mut page = MeshAccum::new();
    page.mats.primitive_filter = Some(filter);
    if let Some(scene) = doc.default_scene().or_else(|| doc.scenes().next()) {
        for node in scene.nodes() {
            walk(
                &node,
                Mat4::IDENTITY,
                &buffers,
                &images,
                &mut page.vertices,
                &mut page.indices,
                &mut page.mats,
                &mut page.min,
                &mut page.max,
                true,
            );
        }
    }
    page.finish(path)
}

/// Explicit runtime finish for selected constant-material source nodes. Ordinary imports retain
/// the asset verbatim; callers choose this only when authoring a separate gameplay paint finish.
#[derive(Clone, Copy)]
pub(crate) struct MaterialOverride {
    /// Stable finish identity, separate from source glTF materials and other runtime finishes.
    pub swatch: u16,
    pub base: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}
pub(crate) type MaterialOverrideFn = fn(&str, Option<&str>) -> Option<MaterialOverride>;

pub(crate) fn load_glb_with_material_override(path: &str, finish: MaterialOverrideFn) -> anyhow::Result<CpuMesh> {
    Ok(load_glb_partitioned_roots_with_material_override(path, &[], Some(finish))?.remove(0))
}

/// Split complete named scene-root assemblies from a GLB while retaining their authored world
/// transforms. Page zero contains the remainder; subsequent pages follow `moving_roots` order.
/// This partitions existing triangles without cutting, duplicating or reauthoring geometry. Only
/// scene roots are accepted, so a missing/changed rig hierarchy fails instead of leaving a second
/// static copy behind. The material behavior is identical to the ordinary single-mesh loader.
pub fn load_glb_partitioned_roots(
    path: &str,
    moving_roots: &[&str],
) -> anyhow::Result<Vec<CpuMesh>> {
    load_glb_partitioned_roots_with_material_override(path, moving_roots, None)
}

pub(crate) fn load_glb_partitioned_roots_with_material_override(
    path: &str,
    moving_roots: &[&str],
    finish: Option<MaterialOverrideFn>,
) -> anyhow::Result<Vec<CpuMesh>> {
    load_glb_partitioned_roots_with_source_addition(path,moving_roots,finish,None)
}

/// Add one source root omitted from an older bake. It shares the remainder's full material
/// accumulation, so restored geometry retains its own source material instead of borrowing UVs.
/// A corrected future bake containing that root needs no addition and never duplicates it.
pub(crate) fn load_glb_partitioned_roots_with_source_addition(
    path:&str, moving_roots:&[&str], finish:Option<MaterialOverrideFn>,
    addition:Option<(&str,&str)>,
)->anyhow::Result<Vec<CpuMesh>> {
    // Import from bytes so a source path can resolve to the cooker's stable `models/<id>.glb`
    // filename inside an app bundle. Runtime-catalog GLBs are required to be self-contained.
    let resolved = crate::resource::resolve(path)?;
    let bytes = crate::resource::read(path)?;
    let (doc, buffers, images) = gltf::import_slice(&bytes)
        .with_context(|| format!("import glTF resource {path:?} resolved to {resolved:?}"))?;
    let mut pages: Vec<_> = (0..=moving_roots.len()).map(|_| MeshAccum::new()).collect();
    for page in &mut pages { page.mats.material_override = finish; }
    let mut found = vec![false; moving_roots.len()];

    // Walk the default scene's node tree with accumulated transforms.
    let scene = doc.default_scene().or_else(|| doc.scenes().next());
    if let Some(scene) = scene {
        for node in scene.nodes() {
            let selected = moving_roots
                .iter()
                .position(|name| node.name() == Some(*name));
            if let Some(index) = selected {
                anyhow::ensure!(
                    !found[index],
                    "duplicate scene-root {:?} in {path}",
                    moving_roots[index]
                );
                found[index] = true;
            }
            let page = &mut pages[selected.map_or(0, |index| index + 1)];
            walk(
                &node,
                Mat4::IDENTITY,
                &buffers,
                &images,
                &mut page.vertices,
                &mut page.indices,
                &mut page.mats,
                &mut page.min,
                &mut page.max,
                true,
            );
        }
    }

    if let Some((extra_path,root_name))=addition {
        if !doc.nodes().any(|node|node.name()==Some(root_name)) {
            let extra_bytes=crate::resource::read(extra_path)?;
            let (extra,extra_buffers,extra_images)=gltf::import_slice(&extra_bytes)
                .with_context(||format!("import omitted source root {root_name:?} from {extra_path}"))?;
            let scene=extra.default_scene().or_else(||extra.scenes().next())
                .context("source addition has no scene")?;
            let roots:Vec<_>=scene.nodes().collect();
            anyhow::ensure!(roots.len()==1 && roots[0].name()==Some(root_name),
                "source addition must contain exactly the registered root {root_name}");
            // This bounded addition is a constant-material fitting. A textured addition needs
            // a separate material page, not the existing mixed-texture first-material fallback.
            anyhow::ensure!(extra.materials().all(|m|m.pbr_metallic_roughness().base_color_texture().is_none()
                &&m.pbr_metallic_roughness().metallic_roughness_texture().is_none()&&m.normal_texture().is_none()),
                "textured source addition requires its own material page");
            let page=&mut pages[0];let start=page.mats.source_primitives.len();
            walk(&roots[0],Mat4::IDENTITY,&extra_buffers,&extra_images,&mut page.vertices,
                &mut page.indices,&mut page.mats,&mut page.min,&mut page.max,true);
            let offset=doc.materials().count();
            for primitive in &mut page.mats.source_primitives[start..] {
                // Source material indices are local to each document. Paint swatch identities
                // occupy the top of usize and intentionally remain shared across both documents.
                if let Some(id)=primitive.material {
                    if id<extra.materials().count() {primitive.material=Some(offset+id);}
                }
            }
        }
    }

    for (index, present) in found.into_iter().enumerate() {
        anyhow::ensure!(
            present,
            "missing scene-root {:?} in {path}",
            moving_roots[index]
        );
    }
    pages.into_iter().map(|page| page.finish(path)).collect()
}

/// Partition named descendant subtrees while preserving every ancestor transform. Page zero is
/// the remainder; selected pages follow `subtrees` order. Missing, duplicate and nested selections
/// fail instead of silently losing/duplicating geometry. The stricter animated-root API above is
/// unchanged: this separate path is for source assets whose foliage is parented to their trunk.
pub fn load_glb_partitioned_subtrees(path: &str, subtrees: &[&str]) -> anyhow::Result<Vec<CpuMesh>> {
    let bytes = crate::resource::read(path)?;
    let (doc, buffers, images) = gltf::import_slice(&bytes)
        .with_context(|| format!("import subtree-partitioned glTF resource {path:?}"))?;
    let mut pages: Vec<_> = (0..=subtrees.len()).map(|_| MeshAccum::new()).collect();
    let mut found = vec![false; subtrees.len()];

    #[allow(clippy::too_many_arguments)]
    fn split(
        node: gltf::Node<'_>, parent: Mat4, inherited_page: usize,
        names: &[&str], found: &mut [bool], pages: &mut [MeshAccum],
        buffers: &[gltf::buffer::Data], images: &[gltf::image::Data],
    ) -> anyhow::Result<()> {
        let selected = names.iter().position(|name| node.name() == Some(*name));
        let page_index = if let Some(index) = selected {
            anyhow::ensure!(!found[index], "duplicate selected subtree {:?}", names[index]);
            anyhow::ensure!(inherited_page == 0, "nested selected subtree {:?}", names[index]);
            found[index] = true;
            index + 1
        } else { inherited_page };
        let page = &mut pages[page_index];
        walk(&node, parent, buffers, images, &mut page.vertices, &mut page.indices,
             &mut page.mats, &mut page.min, &mut page.max, false);
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        for child in node.children() {
            split(child, world, page_index, names, found, pages, buffers, images)?;
        }
        Ok(())
    }

    if let Some(scene) = doc.default_scene().or_else(|| doc.scenes().next()) {
        for node in scene.nodes() {
            split(node, Mat4::IDENTITY, 0, subtrees, &mut found, &mut pages, &buffers, &images)?;
        }
    }
    for (index, present) in found.into_iter().enumerate() {
        anyhow::ensure!(present, "missing selected subtree {:?} in {path}", subtrees[index]);
    }
    pages.into_iter().map(|page| page.finish(path)).collect()
}

struct MeshAccum {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    mats: MaterialAccum,
    min: Vec3,
    max: Vec3,
}

impl MeshAccum {
    fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            mats: MaterialAccum::new(),
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    fn finish(self, path: &str) -> anyhow::Result<CpuMesh> {
        anyhow::ensure!(
            !self.vertices.is_empty(),
            "no geometry in a requested page of {path}"
        );
        let mut mesh = CpuMesh {
            vertices: self.vertices,
            indices: self.indices,
            texture: self.mats.texture,
            metallic_roughness_texture: self.mats.mr_texture,
            normal_texture: self.mats.normal_texture,
            metallic_factor: self.mats.metallic_factor,
            roughness_factor: self.mats.roughness_factor,
            min: self.min,
            max: self.max,
        };
        material_channels::apply(&mut mesh, &self.mats.source_primitives)?;
        Ok(mesh)
    }
}

/// Retain source assignments while walking geometry. The original image accumulator remains the
/// compatibility path for mixed textured pages pending per-material mip/texture transport.
#[derive(Default)]
struct MaterialAccum {
    texture: Option<CpuImage>,
    mr_texture: Option<CpuImage>,
    normal_texture: Option<CpuImage>,
    metallic_factor: f32,
    roughness_factor: f32,
    mr_seen: bool,
    source_primitives: Vec<material_channels::Primitive>,
    material_override: Option<MaterialOverrideFn>,
    /// Only `load_glb_filtered` sets this; every other loader keeps all primitives.
    primitive_filter: Option<PrimitiveFilter>,
}

impl MaterialAccum {
    fn new() -> Self {
        Self {
            metallic_factor: DEFAULT_METALLIC,
            roughness_factor: DEFAULT_ROUGHNESS,
            ..Default::default()
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk(
    node: &gltf::Node,
    parent: Mat4,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    mats: &mut MaterialAccum,
    min: &mut Vec3,
    max: &mut Vec3,
    recurse_children: bool,
) {
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent * local;
    let normal_mat = Mat3::from_mat4(world).inverse().transpose();
    let tangent_mat = Mat3::from_mat4(world);

    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
            if let Some(filter) = mats.primitive_filter {
                if !filter.keeps(prim.material().name()) {
                    continue;
                }
            }
            let reader = prim.reader(|b| Some(&buffers[b.index()]));
            let Some(pos_iter) = reader.read_positions() else {
                continue;
            };
            let positions: Vec<[f32; 3]> = pos_iter.collect();
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|n| n.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|t| t.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
            let local_indices: Vec<u32> = reader
                .read_indices()
                .map(|idx| idx.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());

            // Tangents: use the authored TANGENT attribute if present, else derive one per
            // triangle from the UV gradient (Lengyel's method), falling back to an arbitrary
            // normal-orthogonal vector for degenerate/absent UVs. Either way this never panics —
            // and for meshes with no normal map (the common case) the tangent direction doesn't
            // even matter, since a flat normal-map fallback resolves to the interpolated normal
            // regardless of the TBN basis.
            let tangents: Vec<[f32; 4]> = reader
                .read_tangents()
                .map(|t| t.collect())
                .unwrap_or_else(|| generate_tangents(&positions, &normals, &uvs, &local_indices));

            let base = vertices.len() as u32;
            for i in 0..positions.len() {
                let wp = world.transform_point3(Vec3::from(positions[i]));
                *min = min.min(wp);
                *max = max.max(wp);
                // Guard the index: a present-but-short NORMAL accessor (fewer than POSITION) would
                // otherwise panic here. Fall back to up like the absent-accessor case.
                let nraw = *normals.get(i).unwrap_or(&[0.0, 1.0, 0.0]);
                let n = (normal_mat * Vec3::from(nraw)).normalize_or_zero();
                let traw = *tangents.get(i).unwrap_or(&[1.0, 0.0, 0.0, 1.0]);
                let t = (tangent_mat * Vec3::new(traw[0], traw[1], traw[2])).normalize_or_zero();
                vertices.push(Vertex {
                    pos: wp.to_array(),
                    normal: n.to_array(),
                    uv: *uvs.get(i).unwrap_or(&[0.0, 0.0]),
                    tangent: [t.x, t.y, t.z, traw[3]],
                });
            }
            for i in &local_indices {
                indices.push(base + i);
            }

            // First material we find wins (single-material props / tank parts).
            let material = prim.material();
            let pbr = material.pbr_metallic_roughness();
            let any_textured = pbr.base_color_texture().is_some()
                || pbr.metallic_roughness_texture().is_some() || material.normal_texture().is_some();
            // Never flatten a future detailed texture/normal-map source into this constant finish.
            let finish = if any_textured { None } else {
                mats.material_override.and_then(|apply| apply(node.name().unwrap_or(""), material.name()))
            };
            mats.source_primitives.push(material_channels::Primitive {
                // A distinct swatch prevents one shared source material from painting tracks,
                // breech steel or other unselected nodes that use the same material identity.
                material: finish.map(|f| usize::MAX - usize::from(f.swatch)).or(material.index()), vertices: base as usize..vertices.len(),
                base: finish.map_or(pbr.base_color_factor(), |f| f.base),
                metallic: finish.map_or(pbr.metallic_factor(), |f| f.metallic),
                roughness: finish.map_or(pbr.roughness_factor(), |f| f.roughness),
                base_textured: pbr.base_color_texture().is_some(),
                any_textured,
            });

            if mats.texture.is_none() {
                if let Some(info) = pbr.base_color_texture() {
                    let src = info.texture().source().index();
                    if let Some(img) = images.get(src) {
                        mats.texture = Some(to_rgba8(img));
                    }
                } else {
                    // No base-color texture: bake the material's constant factor as a flat 1×1
                    // tint, same mechanism the procedural meshes already use. Matches the white
                    // (1,1,1,1) default when unspecified, so this is a no-op for untinted assets.
                    let f = pbr.base_color_factor();
                    mats.texture = Some(flat_rgba([
                        (f[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (f[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (f[2].clamp(0.0, 1.0) * 255.0) as u8,
                    ]));
                }
            }

            if !mats.mr_seen {
                if let Some(info) = pbr.metallic_roughness_texture() {
                    let src = info.texture().source().index();
                    if let Some(img) = images.get(src) {
                        mats.mr_texture = Some(to_rgba8(img));
                        mats.metallic_factor = pbr.metallic_factor();
                        mats.roughness_factor = pbr.roughness_factor();
                        mats.mr_seen = true;
                    }
                }
                // else: leave the sane non-PBR defaults from MaterialAccum::new() in place —
                // don't trust a legacy material's factor scalars when there's no actual texture.
            }

            if mats.normal_texture.is_none() {
                if let Some(info) = material.normal_texture() {
                    let src = info.texture().source().index();
                    if let Some(img) = images.get(src) {
                        mats.normal_texture = Some(to_rgba8(img));
                    }
                }
            }
        }
    }

    if recurse_children {
        for child in node.children() {
            walk(
                &child, world, buffers, images, vertices, indices, mats, min, max, true,
            );
        }
    }
}

/// Per-triangle UV-gradient tangent generation (Lengyel's method), averaged per vertex and
/// Gram-Schmidt orthogonalized against the vertex normal. Degenerate UVs (zero area / zero
/// gradient — common on untextured legacy meshes) fall back to an arbitrary vector orthogonal to
/// the normal so the result is always a valid, if arbitrary, tangent frame.
fn generate_tangents(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    let n = positions.len();
    let mut tan1 = vec![Vec3::ZERO; n];
    let mut tan2 = vec![Vec3::ZERO; n];
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= n || i1 >= n || i2 >= n {
            continue;
        }
        let p0 = Vec3::from(positions[i0]);
        let p1 = Vec3::from(positions[i1]);
        let p2 = Vec3::from(positions[i2]);
        // .get-guarded: a present-but-short UV accessor must not panic here.
        let z2 = [0.0, 0.0];
        let uv0 = *uvs.get(i0).unwrap_or(&z2);
        let uv1 = *uvs.get(i1).unwrap_or(&z2);
        let uv2 = *uvs.get(i2).unwrap_or(&z2);
        let e1 = p1 - p0;
        let e2 = p2 - p0;
        let du1 = uv1[0] - uv0[0];
        let dv1 = uv1[1] - uv0[1];
        let du2 = uv2[0] - uv0[0];
        let dv2 = uv2[1] - uv0[1];
        let denom = du1 * dv2 - du2 * dv1;
        if denom.abs() < 1e-8 {
            continue;
        }
        let r = 1.0 / denom;
        let sdir = (e1 * dv2 - e2 * dv1) * r;
        let tdir = (e2 * du1 - e1 * du2) * r;
        for &i in &[i0, i1, i2] {
            tan1[i] += sdir;
            tan2[i] += tdir;
        }
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let normal = Vec3::from(*normals.get(i).unwrap_or(&[0.0, 1.0, 0.0]));
        let raw = tan1[i];
        let mut tangent = if raw.length_squared() > 1e-12 {
            (raw - normal * normal.dot(raw)).normalize_or_zero()
        } else {
            Vec3::ZERO
        };
        if tangent.length_squared() < 1e-12 {
            let helper = if normal.x.abs() < 0.9 {
                Vec3::X
            } else {
                Vec3::Y
            };
            tangent = (helper - normal * normal.dot(helper)).normalize_or_zero();
        }
        let handedness = if normal.cross(tangent).dot(tan2[i]) < 0.0 {
            -1.0
        } else {
            1.0
        };
        out.push([tangent.x, tangent.y, tangent.z, handedness]);
    }
    out
}

fn flat_rgba(rgb: [u8; 3]) -> CpuImage {
    CpuImage::new(1, 1, vec![rgb[0], rgb[1], rgb[2], 255])
}

fn to_rgba8(img: &gltf::image::Data) -> CpuImage {
    use gltf::image::Format;
    let (w, h) = (img.width, img.height);
    let px = &img.pixels;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    match img.format {
        Format::R8G8B8A8 => rgba.extend_from_slice(px),
        Format::R8G8B8 => {
            for c in px.chunks_exact(3) {
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        Format::R8G8 => {
            for c in px.chunks_exact(2) {
                rgba.extend_from_slice(&[c[0], c[0], c[0], c[1]]);
            }
        }
        Format::R8 => {
            for &v in px {
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        Format::R16 | Format::R16G16 | Format::R16G16B16 | Format::R16G16B16A16 => {
            // The winter pine bark is a 16-bit PNG. gltf/image decode u16 samples into
            // native-endian bytes; preserve its color and alpha instead of the magenta fallback.
            let channels=match img.format {
                Format::R16=>1,Format::R16G16=>2,Format::R16G16B16=>3,_=>4,
            };
            for pixel in px.chunks_exact(channels*2) {
                let mut c=[255_u8;4];
                for i in 0..channels {
                    c[i]=((u16::from_ne_bytes([pixel[i*2],pixel[i*2+1]]) as u32+128)/257) as u8;
                }
                rgba.extend_from_slice(&match channels {
                    1=>[c[0],c[0],c[0],255],2=>[c[0],c[0],c[0],c[1]],_=>c,
                });
            }
        }
        _ => {
            // Fallback: flat magenta so a missing/odd format is obvious, not a crash.
            rgba.resize((w * h * 4) as usize, 0);
            for p in rgba.chunks_exact_mut(4) {
                p.copy_from_slice(&[255, 0, 255, 255]);
            }
        }
    }
    CpuImage::new(w, h, rgba)
}

#[cfg(test)]
mod partition_tests {
    use super::*;

    #[test]
    fn sixteen_bit_source_color_and_alpha_do_not_turn_magenta() {
        let pixels=[0_u16,32768,65535,16384].into_iter().flat_map(u16::to_ne_bytes).collect();
        let image=to_rgba8(&gltf::image::Data {pixels,width:1,height:1,format:gltf::image::Format::R16G16B16A16});
        assert_eq!(image.rgba.as_slice(),&[0,128,255,64]);
        let pixels=[32768_u16,65535].into_iter().flat_map(u16::to_ne_bytes).collect();
        let gray=to_rgba8(&gltf::image::Data {pixels,width:1,height:1,format:gltf::image::Format::R16G16});
        assert_eq!(gray.rgba.as_slice(),&[128,128,128,255]);
    }

    #[test]
    fn actual_tiger_constant_materials_match_every_source_primitive() {
        fn collect(node: gltf::Node<'_>, out: &mut Vec<(usize, f32, f32)>) {
            if let Some(mesh) = node.mesh() {
                for primitive in mesh.primitives() {
                    let material = primitive.material().pbr_metallic_roughness();
                    assert!(material.base_color_texture().is_none());
                    assert!(material.metallic_roughness_texture().is_none());
                    assert!(primitive.material().normal_texture().is_none());
                    let count = primitive.get(&gltf::Semantic::Positions).unwrap().count();
                    out.push((count, material.metallic_factor(), material.roughness_factor()));
                }
            }
            for child in node.children() { collect(child, out); }
        }
        for file in ["tiger_hull.glb", "tiger_turret.glb", "tiger_gun.glb"] {
            let path = format!("assets/tanks/tiger/baked/{file}");
            let bytes = crate::resource::read(&path).unwrap();
            let source = gltf::Gltf::from_slice(&bytes).unwrap();
            let mut expected = Vec::new();
            for node in source.default_scene().unwrap().nodes() { collect(node, &mut expected); }
            let loaded = load_glb(&path).unwrap();
            let mr = loaded.metallic_roughness_texture.as_ref().expect("parallel primitive MR atlas");
            assert_eq!(mr.height, 1, "constant source materials must never blend in generated mips");
            let mut cursor = 0;
            for (count, metallic, roughness) in expected {
                for vertex in &loaded.vertices[cursor..cursor + count] {
                    let pixel = (vertex.uv[0] * mr.width as f32).floor() as usize * 4;
                    assert!((f32::from(mr.rgba[pixel + 1]) / 255.0 - roughness).abs() <= 0.5 / 255.0 + 1e-7,
                        "{file}: source roughness mapped to a different primitive");
                    assert!((f32::from(mr.rgba[pixel + 2]) / 255.0 - metallic).abs() <= 0.5 / 255.0 + 1e-7,
                        "{file}: source metallic mapped to a different primitive");
                }
                cursor += count;
            }
            assert_eq!(cursor, loaded.vertices.len(), "all source primitives covered");
        }
    }

    #[test]
    fn scene_root_partition_conserves_triangles_transforms_and_materials() {
        // Two tiny triangles: one static, one under a translated empty lid root. This exercises
        // the same walk as a real nested hatch without making a unit test import a 123 MB tank.
        let mut json = br#"{"asset":{"version":"2.0"},"scene":0,
          "scenes":[{"nodes":[0,1]}],
          "nodes":[{"name":"body","mesh":0,"translation":[2,0,0]},
                   {"name":"lid","children":[2],"translation":[0,3,0]},
                   {"name":"handle","mesh":1,"translation":[0,0,4]}],
          "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]},
                    {"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":1}]}],
          "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1,0,0,1]}},
                       {"pbrMetallicRoughness":{"baseColorFactor":[0,0,1,1]}}],
          "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
                        "min":[0,0,0],"max":[1,1,0]},
                       {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
          "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},
                         {"buffer":0,"byteOffset":36,"byteLength":6}],
          "buffers":[{"byteLength":44}]}"#
            .to_vec();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut binary = Vec::new();
        for value in [0_f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            binary.extend_from_slice(&value.to_le_bytes());
        }
        for index in [0_u16, 1, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        binary.extend_from_slice(&[0, 0]);
        let mut glb = b"glTF".to_vec();
        for value in [
            2_u32,
            (28 + json.len() + binary.len()) as u32,
            json.len() as u32,
            0x4e4f534a,
        ] {
            glb.extend_from_slice(&value.to_le_bytes());
        }
        glb.extend_from_slice(&json);
        glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004e4942_u32.to_le_bytes());
        glb.extend_from_slice(&binary);
        let path =
            std::env::temp_dir().join(format!("kessel-root-partition-{}.glb", std::process::id()));
        std::fs::write(&path, glb).unwrap();
        let path_str = path.to_str().unwrap();
        let original = load_glb(path_str).unwrap();
        let split = load_glb_partitioned_roots(path_str, &["lid"]).unwrap();
        let missing = load_glb_partitioned_roots(path_str, &["nonexistent"]);
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            original.indices.len(),
            split.iter().map(|m| m.indices.len()).sum::<usize>()
        );
        let points = |mesh: &CpuMesh| mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>();
        assert_eq!(
            points(&original),
            split.iter().flat_map(points).collect::<Vec<_>>()
        );
        assert_eq!(split[0].min, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(split[1].min, Vec3::new(0.0, 3.0, 4.0));
        assert_eq!(
            split[0].texture.as_ref().unwrap().rgba.as_slice(),
            &[255, 0, 0, 255]
        );
        assert_eq!(
            split[1].texture.as_ref().unwrap().rgba.as_slice(),
            &[0, 0, 255, 255]
        );
        assert!(missing.is_err());
    }
}
