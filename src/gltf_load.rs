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

#[derive(Clone)]
pub struct CpuImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone)]
pub struct CpuMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub texture: Option<CpuImage>, // base color (existing)
    pub metallic_roughness_texture: Option<CpuImage>, // glTF convention: G = roughness, B = metallic
    pub normal_texture: Option<CpuImage>,             // tangent-space normal map
    /// Multiplies the metallic-roughness texture's B/G channels (glTF spec: texture × factor).
    /// For meshes with NO metallic-roughness texture this is overridden to a sane non-PBR default
    /// (fully rough dielectric) rather than trusting a legacy material's possibly-nonsensical
    /// factor (e.g. the T-34 bake authors roughness/metallic factors of 0 with no texture at all —
    /// clearly meant for a different renderer, not "mirror finish").
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
    // Import from bytes so a source path can resolve to the cooker's stable `models/<id>.glb`
    // filename inside an app bundle. Runtime-catalog GLBs are required to be self-contained.
    let resolved = crate::resource::resolve(path)?;
    let bytes = crate::resource::read(path)?;
    let (doc, buffers, images) = gltf::import_slice(&bytes)
        .with_context(|| format!("import glTF resource {path:?} resolved to {resolved:?}"))?;
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut mats = MaterialAccum::new();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);

    // Walk the default scene's node tree with accumulated transforms.
    let scene = doc.default_scene().or_else(|| doc.scenes().next());
    if let Some(scene) = scene {
        for node in scene.nodes() {
            walk(
                &node,
                Mat4::IDENTITY,
                &buffers,
                &images,
                &mut vertices,
                &mut indices,
                &mut mats,
                &mut min,
                &mut max,
            );
        }
    }

    if vertices.is_empty() {
        anyhow::bail!("no geometry in {path}");
    }
    Ok(CpuMesh {
        vertices,
        indices,
        texture: mats.texture,
        metallic_roughness_texture: mats.mr_texture,
        normal_texture: mats.normal_texture,
        metallic_factor: mats.metallic_factor,
        roughness_factor: mats.roughness_factor,
        min,
        max,
    })
}

/// First-material-wins accumulator (single-material-per-mesh simplification, same as the original
/// base-color-only loader — our props/tank parts are one material each).
#[derive(Default)]
struct MaterialAccum {
    texture: Option<CpuImage>,
    mr_texture: Option<CpuImage>,
    normal_texture: Option<CpuImage>,
    metallic_factor: f32,
    roughness_factor: f32,
    mr_seen: bool,
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
) {
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent * local;
    let normal_mat = Mat3::from_mat4(world).inverse().transpose();
    let tangent_mat = Mat3::from_mat4(world);

    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
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

    for child in node.children() {
        walk(
            &child, world, buffers, images, vertices, indices, mats, min, max,
        );
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
    CpuImage {
        width: 1,
        height: 1,
        rgba: vec![rgb[0], rgb[1], rgb[2], 255],
    }
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
        _ => {
            // Fallback: flat magenta so a missing/odd format is obvious, not a crash.
            rgba.resize((w * h * 4) as usize, 0);
            for p in rgba.chunks_exact_mut(4) {
                p.copy_from_slice(&[255, 0, 255, 255]);
            }
        }
    }
    CpuImage {
        width: w,
        height: h,
        rgba,
    }
}
