//! Portable two-level visibility over the original source triangles. Geometry is registered once;
//! changing instances refits only the TLAS and uploads transforms, never world-expanded triangles.
//! Both levels use forward escape links, so GPU traversal has no finite-stack overflow case.
//! Payloads are two `array<u32>` storage buffers. Offsets below are WORDS, not bytes.
//! The CPU query reads those same payloads. No renderer binding or shipping integration is implied.

use crate::gltf_load::{CpuImage, CpuMesh};
use glam::{Mat4, Vec2, Vec3};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const SHADER: &str = include_str!("../shaders/scene_visibility.wgsl");
const MAGIC: u32 = 0x5356_4953;
const HEADER: usize = 8;
const GEOMETRY_HEADER: usize = 16;
const NODE_WORDS: usize = 12;
const VERTEX_WORDS: usize = 8;
const INSTANCE_WORDS: usize = 32;
const LEAF_SIZE: usize = 4;
/// Same source-vertex deformation height as stylized_mesh.wgsl SOLID_GRASS_BEGIN.
pub const GRASS_TIP_HEIGHT_M: f32 = 0.48;
fn bend_point(point: Vec3, tip: Vec3) -> Vec3 {
    let u = (point.y / GRASS_TIP_HEIGHT_M).clamp(0., 1.);
    point + tip * u * u
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GeometryId(pub u32);

/// Alpha uses source level-zero nearest/repeat coverage (cutoff 0.5). Ray-footprint/mip coverage
/// and vertex deformation must be supplied consistently by the renderer; neither is invented here.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialClass {
    Opaque = 0,
    Dielectric = 1,
    AlphaTest = 2,
    Unresolved = 3,
    Mirror = 4,
    ThinLens = 5,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisibilityInstance {
    pub id: u32,
    pub geometry: GeometryId,
    pub model: Mat4,
    pub material: MaterialClass,
    pub material_id: u32,
    /// Actual world X/Z tip displacement from the solid-grass vertex shader; zero otherwise.
    pub bend_world: Vec2,
    /// Source finite ideal-lens center in world metres and positive focal length; zero otherwise.
    pub lens_center_focal: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibilityError(pub &'static str);
impl std::fmt::Display for VisibilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for VisibilityError {}
type Result<T> = std::result::Result<T, VisibilityError>;
fn require(ok: bool, message: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(VisibilityError(message))
    }
}
fn index(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| VisibilityError("payload exceeds u32 word addressing"))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}
fn up(x: f32) -> f32 {
    if x == 0.0 {
        f32::from_bits(1)
    } else {
        f32::from_bits(if x > 0.0 {
            x.to_bits() + 1
        } else {
            x.to_bits() - 1
        })
    }
}
fn down(x: f32) -> f32 {
    -up(-x)
}
impl Bounds {
    fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }
    fn include(&mut self, b: Self) {
        self.min = self.min.min(b.min);
        self.max = self.max.max(b.max);
    }
    fn triangle(v: [Vec3; 3]) -> Self {
        let lo = v[0].min(v[1]).min(v[2]);
        let hi = v[0].max(v[1]).max(v[2]);
        Self {
            min: Vec3::new(down(lo.x), down(lo.y), down(lo.z)),
            max: Vec3::new(up(hi.x), up(hi.y), up(hi.z)),
        }
    }
    fn center(self) -> Vec3 {
        ((self.min.as_dvec3() + self.max.as_dvec3()) * 0.5).as_vec3()
    }
    fn bent(self, tip: Vec3) -> Result<Self> {
        if tip == Vec3::ZERO {
            return Ok(self);
        }
        let a = tip * (self.min.y / GRASS_TIP_HEIGHT_M).clamp(0., 1.).powi(2);
        let b = tip * (self.max.y / GRASS_TIP_HEIGHT_M).clamp(0., 1.).powi(2);
        let lo = self.min + a.min(b);
        let hi = self.max + a.max(b);
        let error =
            (self.min.abs().max(self.max.abs()) + a.abs().max(b.abs())) * (8. * f32::EPSILON);
        let lo = lo - error;
        let hi = hi + error;
        require(
            lo.is_finite() && hi.is_finite(),
            "grass bend bounds overflow",
        )?;
        Ok(Self {
            min: Vec3::new(down(lo.x), down(lo.y), down(lo.z)),
            max: Vec3::new(up(hi.x), up(hi.y), up(hi.z)),
        })
    }
    fn transformed(self, model: Mat4) -> Result<Self> {
        let matrix = model.as_dmat4();
        let mut result = Self::empty();
        for bits in 0..8 {
            let p = Vec3::new(
                if bits & 1 == 0 {
                    self.min.x
                } else {
                    self.max.x
                },
                if bits & 2 == 0 {
                    self.min.y
                } else {
                    self.max.y
                },
                if bits & 4 == 0 {
                    self.min.z
                } else {
                    self.max.z
                },
            );
            let world = matrix.transform_point3(p.as_dvec3());
            // Error bound for the f32 multiply/add sequence in the drawing transform. It is
            // scale/coordinate dependent, not a fixed geometric inflation or simplified hull.
            let abs = model.to_cols_array_2d();
            let error = Vec3::new(
                abs[0][0].abs() * p.x.abs()
                    + abs[1][0].abs() * p.y.abs()
                    + abs[2][0].abs() * p.z.abs()
                    + abs[3][0].abs(),
                abs[0][1].abs() * p.x.abs()
                    + abs[1][1].abs() * p.y.abs()
                    + abs[2][1].abs() * p.z.abs()
                    + abs[3][1].abs(),
                abs[0][2].abs() * p.x.abs()
                    + abs[1][2].abs() * p.y.abs()
                    + abs[2][2].abs() * p.z.abs()
                    + abs[3][2].abs(),
            ) * (8.0 * f32::EPSILON);
            let lo = world.as_vec3() - error;
            let hi = world.as_vec3() + error;
            require(
                lo.is_finite() && hi.is_finite(),
                "nonfinite transformed bounds",
            )?;
            result.include(Self {
                min: Vec3::new(down(lo.x), down(lo.y), down(lo.z)),
                max: Vec3::new(up(hi.x), up(hi.y), up(hi.z)),
            });
        }
        require(
            result.min.is_finite() && result.max.is_finite(),
            "bounds exceed finite f32",
        )?;
        Ok(result)
    }
}

#[derive(Clone, Copy)]
struct Primitive {
    bounds: Bounds,
    source: usize,
}
#[derive(Clone, Copy)]
struct Node {
    bounds: Bounds,
    escape: usize,
    first: usize,
    count: usize,
}
fn build(primitives: &mut [Primitive], base: usize, nodes: &mut Vec<Node>) {
    let at = nodes.len();
    let mut bounds = Bounds::empty();
    for p in primitives.iter() {
        bounds.include(p.bounds);
    }
    nodes.push(Node {
        bounds,
        escape: 0,
        first: base,
        count: 0,
    });
    if primitives.len() <= LEAF_SIZE {
        nodes[at].count = primitives.len();
    } else {
        let extent = bounds.max - bounds.min;
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        let middle = primitives.len() / 2;
        primitives.select_nth_unstable_by(middle, |a, b| {
            a.bounds.center()[axis]
                .total_cmp(&b.bounds.center()[axis])
                .then(a.source.cmp(&b.source))
        });
        let (left, right) = primitives.split_at_mut(middle);
        build(left, base, nodes);
        build(right, base + middle, nodes);
    }
    nodes[at].escape = nodes.len();
}
fn pack_nodes(nodes: &[Node], offset: usize, words: &mut Vec<u32>) -> Result<()> {
    for n in nodes {
        words.extend([
            n.bounds.min.x.to_bits(),
            n.bounds.min.y.to_bits(),
            n.bounds.min.z.to_bits(),
            index(offset + n.escape * NODE_WORDS)?,
            n.bounds.max.x.to_bits(),
            n.bounds.max.y.to_bits(),
            n.bounds.max.z.to_bits(),
            index(n.first)?,
            index(n.count)?,
            0,
            0,
            0,
        ]);
    }
    Ok(())
}

fn mesh_fingerprint(mesh: &CpuMesh, width: u32, height: u32, alpha: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut bytes = |bytes: &[u8]| {
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    bytes(&(mesh.vertices.len() as u64).to_le_bytes());
    bytes(&(mesh.indices.len() as u64).to_le_bytes());
    for v in &mesh.vertices {
        for x in v.pos.iter().chain(&v.uv) {
            bytes(&x.to_bits().to_le_bytes());
        }
    }
    for i in &mesh.indices {
        bytes(&i.to_le_bytes());
    }
    bytes(&width.to_le_bytes());
    bytes(&height.to_le_bytes());
    bytes(alpha);
    hash
}

#[derive(Clone)]
struct Geometry {
    bounds: Bounds,
    triangles: usize,
    deformable: bool,
    source_texture: Option<CpuImage>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateStats {
    pub rebuilt_tlas: bool,
    pub changed_instances: usize,
    pub instance_count: usize,
    pub tlas_nodes: usize,
}

pub struct SceneVisibility {
    geometry_words: Vec<u32>,
    instance_words: Vec<u32>,
    geometries: BTreeMap<GeometryId, Geometry>,
    geometry_cache: BTreeMap<u64, Vec<GeometryId>>,
    geometry_writes: VecDeque<(u64, std::ops::Range<usize>)>,
    instances_geometry_revision: u64,
    instances: Vec<VisibilityInstance>,
    instance_bounds: Vec<Bounds>,
    ordered: Vec<usize>,
    nodes: Vec<Node>,
    geometry_revision: u64,
    instance_revision: u64,
}
impl Default for SceneVisibility {
    fn default() -> Self {
        Self::new()
    }
}
impl SceneVisibility {
    pub fn new() -> Self {
        Self {
            geometry_words: vec![0; 4],
            instance_words: vec![MAGIC, 1, HEADER as u32, 0, HEADER as u32, 0, 0, 0],
            geometries: BTreeMap::new(),
            geometry_cache: BTreeMap::new(),
            geometry_writes: VecDeque::new(),
            instances_geometry_revision: 0,
            instances: Vec::new(),
            instance_bounds: Vec::new(),
            ordered: Vec::new(),
            nodes: Vec::new(),
            geometry_revision: 0,
            instance_revision: 0,
        }
    }
    pub fn geometry_words(&self) -> &[u32] {
        &self.geometry_words
    }
    pub fn instance_words(&self) -> &[u32] {
        &self.instance_words
    }
    pub fn geometry_revision(&self) -> u64 {
        self.geometry_revision
    }
    pub fn instance_revision(&self) -> u64 {
        self.instance_revision
    }
    pub fn geometry_count(&self) -> usize {
        self.geometries.len()
    }
    pub fn unique_triangle_count(&self) -> usize {
        self.geometries.values().map(|g| g.triangles).sum()
    }

    /// Register exact source positions, indices and UVs. Caller reuses the ID for every placement
    /// of this mesh. Invalid geometry fails the update; it is never dropped from visibility.
    pub fn register_mesh(&mut self, mesh: &CpuMesh) -> Result<GeometryId> {
        self.register_mesh_inner(mesh, false)
    }
    /// A private BLAS for one deforming actor. Never shared through content dedup; renderer clones
    /// must request a private ID before their first vertex deformation (copy on write).
    pub fn register_deformable_mesh(&mut self, mesh: &CpuMesh) -> Result<GeometryId> {
        self.register_mesh_inner(mesh, true)
    }
    fn register_mesh_inner(&mut self, mesh: &CpuMesh, deformable: bool) -> Result<GeometryId> {
        require(
            !mesh.indices.is_empty() && mesh.indices.len() % 3 == 0,
            "mesh needs complete triangles",
        )?;
        require(
            mesh.vertices
                .iter()
                .all(|v| v.pos.iter().chain(&v.uv).all(|x| x.is_finite())),
            "nonfinite source vertex/UV",
        )?;
        require(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.vertices.len()),
            "source index out of range",
        )?;
        let (width, height, alpha_bytes) = if let Some(texture) = &mesh.texture {
            let texels = (texture.width as usize)
                .checked_mul(texture.height as usize)
                .ok_or(VisibilityError("alpha texture size overflow"))?;
            index(texels)?; // shader alpha texel addressing is u32, not only its word offset
            require(
                texels > 0
                    && texture.rgba.len()
                        == texels
                            .checked_mul(4)
                            .ok_or(VisibilityError("alpha size overflow"))?,
                "invalid source alpha texture",
            )?;
            (
                texture.width,
                texture.height,
                texture
                    .rgba
                    .chunks_exact(4)
                    .map(|p| p[3])
                    .collect::<Vec<_>>(),
            )
        } else {
            (1, 1, vec![255])
        };
        let fingerprint = mesh_fingerprint(mesh, width, height, &alpha_bytes);
        if !deformable {
            if let Some(candidates) = self.geometry_cache.get(&fingerprint) {
                for &id in candidates {
                    if self.matches_source(id, mesh, width, height, &alpha_bytes) {
                        return Ok(id);
                    }
                }
            }
        }
        let mut primitives = Vec::with_capacity(mesh.indices.len() / 3);
        for (source, t) in mesh.indices.chunks_exact(3).enumerate() {
            let bounds = Bounds::triangle(std::array::from_fn(|k| {
                Vec3::from_array(mesh.vertices[t[k] as usize].pos)
            }));
            require(
                bounds.min.is_finite() && bounds.max.is_finite(),
                "source bounds exceed finite f32",
            )?;
            primitives.push(Primitive { bounds, source });
        }
        let mut nodes = Vec::new();
        build(&mut primitives, 0, &mut nodes);
        let start = self.geometry_words.len();
        let id = GeometryId(index(start)?);
        let root = start + GEOMETRY_HEADER;
        let vertices = root + nodes.len() * NODE_WORDS;
        let triangles = vertices + mesh.vertices.len() * VERTEX_WORDS;
        let alpha = triangles + primitives.len() * 4;
        index(alpha + alpha_bytes.len().div_ceil(4))?;
        let mut payload = vec![
            index(root)?,
            index(nodes.len())?,
            index(vertices)?,
            index(mesh.vertices.len())?,
            index(triangles)?,
            index(primitives.len())?,
            index(alpha)?,
            width,
            height,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        pack_nodes(&nodes, root, &mut payload)?;
        for v in &mesh.vertices {
            payload.extend([
                v.pos[0].to_bits(),
                v.pos[1].to_bits(),
                v.pos[2].to_bits(),
                0,
                v.uv[0].to_bits(),
                v.uv[1].to_bits(),
                0,
                0,
            ]);
        }
        for p in &primitives {
            let t = &mesh.indices[p.source * 3..p.source * 3 + 3];
            payload.extend([t[0], t[1], t[2], index(p.source)?]);
        }
        for p in alpha_bytes.chunks(4) {
            let mut bytes = [255; 4];
            bytes[..p.len()].copy_from_slice(p);
            payload.push(u32::from_le_bytes(bytes));
        }
        self.geometry_words.extend(payload);
        self.geometries.insert(
            id,
            Geometry {
                bounds: nodes[0].bounds,
                triangles: primitives.len(),
                deformable,
                source_texture: mesh.texture.clone(),
            },
        );
        if !deformable {
            self.geometry_cache.entry(fingerprint).or_default().push(id);
        }
        self.record_geometry_write(start..self.geometry_words.len());
        Ok(id)
    }

    fn record_geometry_write(&mut self, range: std::ops::Range<usize>) {
        self.geometry_revision += 1;
        self.geometry_writes
            .push_back((self.geometry_revision, range));
        // A bounded journal supports several GPU consumers. Lagging consumers upload the full
        // payload; recent per-frame pose updates upload only the changed BLAS nodes/vertices.
        if self.geometry_writes.len() > 4096 {
            self.geometry_writes.pop_front();
        }
    }
    pub fn geometry_upload_ranges(&self, since: Option<u64>) -> Vec<std::ops::Range<usize>> {
        let Some(since) = since else {
            return vec![0..self.geometry_words.len()];
        };
        if since >= self.geometry_revision {
            return Vec::new();
        }
        if self
            .geometry_writes
            .front()
            .is_none_or(|(revision, _)| since < revision - 1)
        {
            return vec![0..self.geometry_words.len()];
        }
        let mut ranges: Vec<_> = self
            .geometry_writes
            .iter()
            .filter(|(revision, _)| *revision > since)
            .map(|(_, r)| r.clone())
            .collect();
        ranges.sort_unstable_by_key(|r| r.start);
        let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
        for range in ranges {
            if let Some(last) = merged.last_mut() {
                if range.start <= last.end {
                    last.end = last.end.max(range.end);
                    continue;
                }
            }
            merged.push(range);
        }
        merged
    }
    /// Refit positions of a private deforming mesh, preserving source topology, UV and alpha.
    /// Geometry memory stays bounded, and the next instance update refits TLAS bounds even when
    /// instance transforms did not change. Returns false for an unchanged vertex snapshot.
    pub fn refit_mesh(&mut self, id: GeometryId, mesh: &CpuMesh) -> Result<bool> {
        let geometry = self
            .geometries
            .get(&id)
            .ok_or(VisibilityError("unregistered deformable geometry"))?;
        require(
            geometry.deformable,
            "cannot deform shared immutable geometry; register a private mesh first",
        )?;
        let g = id.0 as usize;
        let w = &self.geometry_words;
        let root = w[g] as usize;
        let count = w[g + 1] as usize;
        let vertices = w[g + 2] as usize;
        let triangles = w[g + 4] as usize;
        require(
            w[g + 3] as usize == mesh.vertices.len() && w[g + 5] as usize * 3 == mesh.indices.len(),
            "deformation changed source topology",
        )?;
        require(
            mesh.vertices
                .iter()
                .all(|v| v.pos.iter().all(|x| x.is_finite())),
            "nonfinite deformed source vertex",
        )?;
        for i in 0..geometry.triangles {
            let t = triangles + i * 4;
            let source = w[t + 3] as usize * 3;
            require(
                w[t..t + 3] == mesh.indices[source..source + 3],
                "deformation changed source indices",
            )?;
        }
        for (i, v) in mesh.vertices.iter().enumerate() {
            require(
                (0..2).all(|k| w[vertices + i * 8 + 4 + k] == v.uv[k].to_bits()),
                "deformation changed source UV coverage",
            )?;
        }
        let same_alpha = match (&geometry.source_texture, &mesh.texture) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.width == b.width
                    && a.height == b.height
                    && (std::sync::Arc::ptr_eq(&a.rgba, &b.rgba)
                        || (a.rgba.len() == b.rgba.len()
                            && a.rgba
                                .chunks_exact(4)
                                .zip(b.rgba.chunks_exact(4))
                                .all(|(a, b)| a[3] == b[3])))
            }
            _ => false,
        };
        require(same_alpha, "deformation changed source alpha coverage")?;
        if mesh
            .vertices
            .iter()
            .enumerate()
            .all(|(i, v)| (0..3).all(|k| w[vertices + i * 8 + k] == v.pos[k].to_bits()))
        {
            return Ok(false);
        }
        let mut update = w[root..triangles].to_vec();
        let mut bounds = vec![Bounds::empty(); count];
        for node in (0..count).rev() {
            let offset = root + node * NODE_WORDS;
            let first = w[offset + 7] as usize;
            let size = w[offset + 8] as usize;
            let mut b = Bounds::empty();
            if size > 0 {
                for i in first..first + size {
                    let t = triangles + i * 4;
                    let p = std::array::from_fn(|k| {
                        Vec3::from_array(mesh.vertices[w[t + k] as usize].pos)
                    });
                    b.include(Bounds::triangle(p));
                }
            } else {
                let left = node + 1;
                let right = (w[root + left * NODE_WORDS + 3] as usize - root) / NODE_WORDS;
                b.include(bounds[left]);
                b.include(bounds[right]);
            }
            require(
                b.min.is_finite() && b.max.is_finite(),
                "deformed bounds exceed finite f32",
            )?;
            bounds[node] = b;
            let dest = node * NODE_WORDS;
            for k in 0..3 {
                update[dest + k] = b.min[k].to_bits();
                update[dest + 4 + k] = b.max[k].to_bits();
            }
        }
        for (i, v) in mesh.vertices.iter().enumerate() {
            for k in 0..3 {
                update[vertices - root + i * VERTEX_WORDS + k] = v.pos[k].to_bits();
            }
        }
        self.geometry_words[root..triangles].copy_from_slice(&update);
        self.geometries.get_mut(&id).unwrap().bounds = bounds[0];
        self.record_geometry_write(root..triangles);
        Ok(true)
    }

    // Hashes only narrow the candidates. Compare every source position/index/UV/alpha bit before
    // reusing geometry, so a hash collision never aliases different triangles or alpha coverage.
    fn matches_source(
        &self,
        id: GeometryId,
        mesh: &CpuMesh,
        width: u32,
        height: u32,
        alpha: &[u8],
    ) -> bool {
        let w = &self.geometry_words;
        let g = id.0 as usize;
        if w[g + 3] as usize != mesh.vertices.len()
            || w[g + 5] as usize != mesh.indices.len() / 3
            || w[g + 7] != width
            || w[g + 8] != height
        {
            return false;
        }
        for (i, v) in mesh.vertices.iter().enumerate() {
            let p = w[g + 2] as usize + i * VERTEX_WORDS;
            if v.pos
                .iter()
                .enumerate()
                .any(|(k, x)| w[p + k] != x.to_bits())
                || v.uv
                    .iter()
                    .enumerate()
                    .any(|(k, x)| w[p + 4 + k] != x.to_bits())
            {
                return false;
            }
        }
        for i in 0..w[g + 5] as usize {
            let p = w[g + 4] as usize + i * 4;
            let source = w[p + 3] as usize * 3;
            if w[p..p + 3] != mesh.indices[source..source + 3] {
                return false;
            }
        }
        alpha
            .iter()
            .enumerate()
            .all(|(i, &a)| ((w[w[g + 6] as usize + i / 4] >> ((i % 4) * 8)) & 255) == a as u32)
    }

    /// Active visible instances only, including terrain. Stable instance order/topology refits
    /// bounds in O(instances + TLAS nodes); unchanged snapshots produce no new upload revision.
    /// Failure is transactional: the previous valid payload remains intact and the caller must
    /// surface the error, not keep presenting stale geometry as a successful update.
    pub fn update_instances(&mut self, active: &[VisibilityInstance]) -> Result<UpdateStats> {
        if active == self.instances && self.instances_geometry_revision == self.geometry_revision {
            return Ok(UpdateStats {
                instance_count: active.len(),
                tlas_nodes: self.nodes.len(),
                ..Default::default()
            });
        }
        let mut seen = BTreeSet::new();
        let mut bounds = Vec::with_capacity(active.len());
        let mut inverses = Vec::with_capacity(active.len());
        let mut changed = 0;
        for (i, instance) in active.iter().enumerate() {
            require(seen.insert(instance.id), "duplicate live instance id")?;
            let geometry = self
                .geometries
                .get(&instance.geometry)
                .ok_or(VisibilityError("unregistered geometry id"))?;
            let m = instance.model;
            require(
                m.is_finite()
                    && m.x_axis.w == 0.0
                    && m.y_axis.w == 0.0
                    && m.z_axis.w == 0.0
                    && m.w_axis.w == 1.0,
                "instance transform must be finite affine",
            )?;
            let determinant = m.determinant();
            let inverse = m.inverse();
            require(
                determinant.is_finite() && determinant != 0.0 && inverse.is_finite(),
                "noninvertible instance transform",
            )?;
            require(
                instance.bend_world.is_finite(),
                "nonfinite world grass bend",
            )?;
            let tip = inverse.transform_vector3(Vec3::new(
                instance.bend_world.x,
                0.,
                instance.bend_world.y,
            ));
            require(tip.is_finite(), "local grass bend overflow")?;
            require(instance.lens_center_focal.iter().all(|v|v.is_finite())
                && (instance.material != MaterialClass::ThinLens || instance.lens_center_focal[3] > 0.001),
                "invalid source thin-lens metadata")?;
            bounds.push(geometry.bounds.bent(tip)?.transformed(m)?);
            inverses.push((inverse, determinant.signum(), tip));
            if self.instances.get(i) != Some(instance) {
                changed += 1;
            }
        }
        let rebuild = active.len() != self.instances.len()
            || active
                .iter()
                .zip(&self.instances)
                .any(|(a, b)| a.id != b.id || a.geometry != b.geometry);
        let (mut nodes, ordered) = if rebuild {
            let mut primitives: Vec<_> = bounds
                .iter()
                .copied()
                .enumerate()
                .map(|(source, bounds)| Primitive { bounds, source })
                .collect();
            let mut nodes = Vec::new();
            if !primitives.is_empty() {
                build(&mut primitives, 0, &mut nodes);
            }
            (
                nodes,
                primitives.iter().map(|p| p.source).collect::<Vec<_>>(),
            )
        } else {
            (self.nodes.clone(), self.ordered.clone())
        };
        if !rebuild {
            for k in (0..nodes.len()).rev() {
                let mut b = Bounds::empty();
                if nodes[k].count > 0 {
                    for &i in &ordered[nodes[k].first..nodes[k].first + nodes[k].count] {
                        b.include(bounds[i]);
                    }
                } else {
                    let left = k + 1;
                    let right = nodes[left].escape;
                    b.include(nodes[left].bounds);
                    b.include(nodes[right].bounds);
                }
                nodes[k].bounds = b;
            }
        }
        let records = HEADER + nodes.len() * NODE_WORDS;
        index(records + active.len() * INSTANCE_WORDS)?;
        let mut words = vec![
            MAGIC,
            1,
            HEADER as u32,
            index(nodes.len())?,
            index(records)?,
            index(active.len())?,
            0,
            0,
        ];
        pack_nodes(&nodes, HEADER, &mut words)?;
        for &i in &ordered {
            let instance = &active[i];
            let (inverse, sign, tip) = inverses[i];
            words.extend(inverse.to_cols_array().map(f32::to_bits));
            words.extend([
                instance.geometry.0,
                instance.id,
                instance.material as u32,
                instance.material_id,
                sign.to_bits(),
                tip.x.to_bits(),
                tip.y.to_bits(),
                tip.z.to_bits(),
            ]);
            words.extend(instance.lens_center_focal.map(f32::to_bits));
            words.extend([0;4]);
        }
        // Each TLAS node has a conservative optical-candidate flag in its spare word9.
        // This selects the exact any-opaque query without scanning every pane for every ray.
        words[6]=0x4f50544b; // OPTK: optical descendant metadata is valid.
        let mut optical=vec![false;nodes.len()];
        for k in (0..nodes.len()).rev() {
            optical[k]=if nodes[k].count>0 {
                ordered[nodes[k].first..nodes[k].first+nodes[k].count].iter().any(|&i|
                    !matches!(active[i].material,MaterialClass::Opaque|MaterialClass::AlphaTest))
            } else {optical[k+1] || optical[nodes[k+1].escape]};
            words[HEADER+k*NODE_WORDS+9]=u32::from(optical[k]);
        }
        words[7]=index(active.iter().filter(|i|!matches!(i.material,MaterialClass::Opaque|MaterialClass::AlphaTest)).count())?;
        self.instances = active.to_vec();
        self.instance_bounds = bounds;
        self.ordered = ordered;
        self.nodes = nodes;
        self.instance_words = words;
        self.instance_revision += 1;
        self.instances_geometry_revision = self.geometry_revision;
        Ok(UpdateStats {
            rebuilt_tlas: rebuild,
            changed_instances: changed,
            instance_count: active.len(),
            tlas_nodes: self.nodes.len(),
        })
    }
}

/// WGSL declarations use exactly two read-only storage bindings. Include once per entry module.
pub fn shader_source(group: u32, geometry_binding: u32, instance_binding: u32) -> String {
    SHADER
        .replace("// @environment-optical-path", include_str!("../shaders/scene_environment_visibility.wgsl"))
        .replace("SV_GROUP", &group.to_string())
        .replace("SV_GEOMETRY_BINDING", &geometry_binding.to_string())
        .replace("SV_INSTANCE_BINDING", &instance_binding.to_string())
}

#[derive(Clone, Copy, Debug)]
pub struct VisibilityRay {
    pub origin: Vec3,
    pub direction: Vec3,
    pub t_min: f32,
    pub t_max: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisibilityHit {
    pub t: f32,
    pub instance_id: u32,
    pub geometry: GeometryId,
    pub primitive_id: u32,
    pub triangle_record: u32,
    pub material: MaterialClass,
    pub material_id: u32,
    pub barycentrics: Vec2,
    pub uv: Vec2,
    pub normal: Vec3,
    pub front_face: bool,
}
fn f(words: &[u32], i: usize) -> f32 {
    f32::from_bits(words[i])
}
fn v3(words: &[u32], i: usize) -> Vec3 {
    Vec3::new(f(words, i), f(words, i + 1), f(words, i + 2))
}
// The two source triangles share an exact edge and the same oriented source plane. A ray
// leaving that plane cannot encounter the adjacent triangle as a separate optical interface.
// No position-distance tolerance merges disjoint panes or perpendicular side/edge faces.
fn coplanar_source_neighbors(a:[Vec3;3],b:[Vec3;3])->bool {
    if a.iter().filter(|v|b.contains(v)).count()<2 {return false;}
    let an=(a[1]-a[0]).cross(a[2]-a[0]);let bn=(b[1]-b[0]).cross(b[2]-b[0]);
    let scale_a=an.abs().max_element();let scale_b=bn.abs().max_element();
    scale_a>0.0 && scale_b>0.0 && an/scale_a==bn/scale_b
}
fn interval(ray: VisibilityRay, bounds: Bounds, upper: f32) -> bool {
    let mut lo = ray.t_min;
    let mut hi = upper;
    for k in 0..3 {
        let d = ray.direction[k];
        if d == 0.0 {
            if ray.origin[k] < bounds.min[k] || ray.origin[k] > bounds.max[k] {
                return false;
            }
        } else {
            let a = (bounds.min[k] - ray.origin[k]) / d;
            let b = (bounds.max[k] - ray.origin[k]) / d;
            lo = lo.max(a.min(b));
            hi = hi.min(a.max(b));
            if lo > hi {
                return false;
            }
        }
    }
    true
}
// Watertight projected-edge test: no backface culling, thickness, or small-triangle cutoff.
fn triangle(ray: VisibilityRay, p: [Vec3; 3], upper: f32) -> Result<Option<(f32, Vec2)>> {
    let d = ray.direction;
    let a = d.abs();
    let z = if a.x >= a.y && a.x >= a.z {
        0
    } else if a.y >= a.z {
        1
    } else {
        2
    };
    if d[z] == 0.0 {
        return Err(VisibilityError("zero transformed ray direction"));
    }
    let mut x = (z + 1) % 3;
    let mut y = (x + 1) % 3;
    if d[z] < 0.0 {
        std::mem::swap(&mut x, &mut y);
    }
    let s = Vec3::new(d[x] / d[z], d[y] / d[z], 1.0 / d[z]);
    let p = p.map(|v| v - ray.origin);
    let q = p.map(|v| Vec3::new(v[x] - s.x * v[z], v[y] - s.y * v[z], v[z] * s.z));
    // Match the GPU's canonical shared-edge arithmetic under contraction.
    let edge=|a:Vec3,b:Vec3| {
        let swap=a.x>b.x || (a.x==b.x && a.y>b.y);
        let (first,second)=if swap {(b,a)}else{(a,b)};
        let determinant=first.x*second.y-first.y*second.x;
        if swap {-determinant}else{determinant}
    };
    let e=Vec3::new(edge(q[1],q[2]),edge(q[2],q[0]),edge(q[0],q[1]));
    require(e.is_finite(), "triangle edge arithmetic overflow")?;
    if e.min_element() < 0.0 && e.max_element() > 0.0 {
        return Ok(None);
    }
    let det = e.x + e.y + e.z;
    require(det.is_finite(), "triangle determinant overflow")?;
    if det == 0.0 {
        return Ok(None);
    }
    let t = (e.x * q[0].z + e.y * q[1].z + e.z * q[2].z) / det;
    require(t.is_finite(), "triangle distance arithmetic overflow")?;
    Ok((t >= ray.t_min && t <= upper).then(|| (t, Vec2::new(e.y, e.z) / det)))
}

impl SceneVisibility {
    pub fn trace(&self, ray: VisibilityRay) -> Result<Option<VisibilityHit>> {
        self.trace_continuation(ray, None)
    }

    /// The outgoing straight ray cannot meet its previous planar source triangle again. Exclude
    /// that one primitive rather than a metric slab that can erase a real adjacent glass exit.
    /// Other triangles of the same instance remain present, including arbitrarily close faces.
    pub(crate) fn trace_continuation(&self, ray: VisibilityRay,
        previous: Option<VisibilityHit>) -> Result<Option<VisibilityHit>> {
        require(
            ray.origin.is_finite()
                && ray.direction.is_finite()
                && ray.direction.abs().max_element() > 0.0
                && ray.t_min.is_finite()
                && ray.t_max.is_finite()
                && ray.t_min >= 0.0
                && ray.t_max >= ray.t_min,
            "invalid visibility ray",
        )?;
        let words = &self.instance_words;
        let geom = &self.geometry_words;
        let mut closest = None;
        let mut upper = ray.t_max;
        let mut node = HEADER;
        let end = HEADER + words[3] as usize * NODE_WORDS;
        let mut iterations = 0;
        while node < end {
            iterations += 1;
            require(
                iterations <= words[3] as usize,
                "TLAS traversal did not terminate",
            )?;
            let escape = words[node + 3] as usize;
            require(escape > node && escape <= end, "invalid TLAS escape")?;
            let bounds = Bounds {
                min: v3(words, node),
                max: v3(words, node + 4),
            };
            if !interval(ray, bounds, upper) {
                node = escape;
                continue;
            }
            let count = words[node + 8] as usize;
            if count == 0 {
                node += NODE_WORDS;
                continue;
            }
            for item in words[node + 7] as usize..words[node + 7] as usize + count {
                let record = words[4] as usize + item * INSTANCE_WORDS;
                require(
                    record + INSTANCE_WORDS <= words.len(),
                    "invalid TLAS instance",
                )?;
                let inverse = Mat4::from_cols_array(&std::array::from_fn(|k| f(words, record + k)));
                let tip = v3(words, record + 21);
                let local = VisibilityRay {
                    origin: inverse.transform_point3(ray.origin),
                    direction: inverse.transform_vector3(ray.direction),
                    ..ray
                };
                require(
                    local.origin.is_finite() && local.direction.is_finite(),
                    "ray transform overflow",
                )?;
                let g = words[record + 16] as usize;
                require(g + GEOMETRY_HEADER <= geom.len(), "invalid geometry header")?;
                let mut b = geom[g] as usize;
                let b_end = b + geom[g + 1] as usize * NODE_WORDS;
                let mut visits = 0;
                while b < b_end {
                    visits += 1;
                    require(
                        visits <= geom[g + 1] as usize,
                        "BLAS traversal did not terminate",
                    )?;
                    let escape = geom[b + 3] as usize;
                    require(escape > b && escape <= b_end, "invalid BLAS escape")?;
                    if !interval(
                        local,
                        Bounds {
                            min: v3(geom, b),
                            max: v3(geom, b + 4),
                        }
                        .bent(tip)?,
                        upper,
                    ) {
                        b = escape;
                        continue;
                    }
                    let count = geom[b + 8] as usize;
                    if count == 0 {
                        b += NODE_WORDS;
                        continue;
                    }
                    for primitive in geom[b + 7] as usize..geom[b + 7] as usize + count {
                        let tri = geom[g + 4] as usize + primitive * 4;
                        if previous.is_some_and(|last| last.instance_id == words[record + 17] && last.primitive_id == geom[tri + 3]) { continue; }
                        let vertices: [usize; 3] = std::array::from_fn(|k| {
                            geom[g + 2] as usize + geom[tri + k] as usize * VERTEX_WORDS
                        });
                        let points = vertices.map(|v| bend_point(v3(geom, v), tip));
                        if let Some(last)=previous.filter(|last|last.instance_id==words[record+17] && last.geometry.0==g as u32) {
                            let prior=std::array::from_fn(|k|bend_point(v3(geom,geom[g+2] as usize+geom[last.triangle_record as usize+k] as usize*VERTEX_WORDS),tip));
                            if coplanar_source_neighbors(points,prior) {continue;}
                        }
                        let Some((t, bary)) = triangle(local, points, upper)? else {
                            continue;
                        };
                        if previous.is_some() && t == 0.0 { continue; }
                        let uv = vertices.map(|v| Vec2::new(f(geom, v + 4), f(geom, v + 5)));
                        let uv = uv[0] * (1.0 - bary.x - bary.y) + uv[1] * bary.x + uv[2] * bary.y;
                        let material = match words[record + 18] {
                            0 => MaterialClass::Opaque,
                            1 => MaterialClass::Dielectric,
                            2 => MaterialClass::AlphaTest,
                            4 => MaterialClass::Mirror,
                            5 => MaterialClass::ThinLens,
                            _ => MaterialClass::Unresolved,
                        };
                        if material == MaterialClass::AlphaTest {
                            let width = geom[g + 7] as usize;
                            let height = geom[g + 8] as usize;
                            let x = ((uv.x - uv.x.floor()) * width as f32) as usize;
                            let y = ((uv.y - uv.y.floor()) * height as f32) as usize;
                            let pixel = y.min(height - 1) * width + x.min(width - 1);
                            let alpha =
                                (geom[geom[g + 6] as usize + pixel / 4] >> ((pixel % 4) * 8)) & 255;
                            if alpha < 128 {
                                continue;
                            }
                        }
                        let local_normal = (points[1] - points[0]).cross(points[2] - points[0]);
                        let normal = inverse.transpose().transform_vector3(local_normal)
                            * f(words, record + 20);
                        require(
                            normal.is_finite() && normal.abs().max_element() > 0.0,
                            "triangle normal arithmetic overflow",
                        )?;
                        let normal = (normal / normal.abs().max_element()).normalize();
                        upper = t;
                        closest = Some(VisibilityHit {
                            t,
                            instance_id: words[record + 17],
                            geometry: GeometryId(g as u32),
                            primitive_id: geom[tri + 3],
                            triangle_record: tri as u32,
                            material,
                            material_id: words[record + 19],
                            barycentrics: bary,
                            uv,
                            normal,
                            front_face: ray.direction.dot(normal) < 0.0,
                        });
                    }
                    b = escape;
                }
            }
            node = escape;
        }
        Ok(closest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf_load::{CpuImage, Vertex};
    fn mesh(points: &[[f32; 3]], indices: &[u32]) -> CpuMesh {
        CpuMesh::basic(
            points
                .iter()
                .map(|&pos| Vertex {
                    pos,
                    normal: [0., 0., 1.],
                    uv: [0.25, 0.25],
                    tangent: [1., 0., 0., 1.],
                })
                .collect(),
            indices.to_vec(),
            None,
            Vec3::ZERO,
            Vec3::ZERO,
        )
    }
    fn tri() -> CpuMesh {
        mesh(&[[-1., -1., 0.], [1., -1., 0.], [0., 1., 0.]], &[0, 1, 2])
    }
    fn instance(id: u32, geometry: GeometryId, model: Mat4) -> VisibilityInstance {
        VisibilityInstance {
            bend_world: glam::Vec2::ZERO,
            lens_center_focal: [0.0; 4],
            id,
            geometry,
            model,
            material: MaterialClass::Opaque,
            material_id: 0,
        }
    }
    fn ray(origin: Vec3, direction: Vec3) -> VisibilityRay {
        VisibilityRay {
            origin,
            direction,
            t_min: 0.,
            t_max: 1000.,
        }
    }
    #[test]
    fn exact_source_triangle_is_not_its_bounds_and_blocks_both_sides() {
        let mut s = SceneVisibility::new();
        let g = s.register_mesh(&tri()).unwrap();
        s.update_instances(&[instance(7, g, Mat4::IDENTITY)])
            .unwrap();
        let front = s
            .trace(ray(Vec3::new(0., 0., 2.), -Vec3::Z))
            .unwrap()
            .unwrap();
        assert_eq!(front.t, 2.);
        assert!(front.front_face);
        assert_eq!(front.normal, Vec3::Z);
        let back = s
            .trace(ray(Vec3::new(0., 0., -2.), Vec3::Z))
            .unwrap()
            .unwrap();
        assert!(!back.front_face);
        assert!(s
            .trace(ray(Vec3::new(0.9, 0.9, 2.), -Vec3::Z))
            .unwrap()
            .is_none());
        assert!(s
            .trace(ray(Vec3::new(0., 0., 2.), Vec3::X))
            .unwrap()
            .is_none());
        // Shared vertex/edge and an open terrain-style triangle remain occluding.
        assert!(s
            .trace(ray(Vec3::new(-1., -1., 2.), -Vec3::Z))
            .unwrap()
            .is_some());
    }
    #[test]
    fn reusable_blas_refits_instances_without_expanding_source_triangles() {
        let mut s = SceneVisibility::new();
        let g = s.register_mesh(&tri()).unwrap();
        let geometry = s.geometry_words().to_vec();
        let mut active: Vec<_> = (0..10000)
            .map(|i| {
                instance(
                    i,
                    g,
                    Mat4::from_translation(Vec3::new(i as f32 * 4., 0., 0.)),
                )
            })
            .collect();
        assert!(s.update_instances(&active).unwrap().rebuilt_tlas);
        assert_eq!(s.unique_triangle_count(), 1);
        assert_eq!(s.geometry_words(), geometry);
        let revision = s.instance_revision();
        assert_eq!(s.update_instances(&active).unwrap().changed_instances, 0);
        assert_eq!(s.instance_revision(), revision);
        active[0].model = Mat4::from_translation(Vec3::new(0., 0., -4.));
        let update = s.update_instances(&active).unwrap();
        assert!(!update.rebuilt_tlas);
        assert_eq!(update.changed_instances, 1);
        assert_eq!(
            s.trace(ray(Vec3::new(0., 0., 2.), -Vec3::Z))
                .unwrap()
                .unwrap()
                .t,
            6.
        );
        active.remove(0);
        assert!(s.update_instances(&active).unwrap().rebuilt_tlas);
        assert!(s
            .trace(ray(Vec3::new(0., 0., 2.), -Vec3::Z))
            .unwrap()
            .is_none());
        s.update_instances(&[]).unwrap();
        assert!(s.trace(ray(Vec3::ZERO, Vec3::Z)).unwrap().is_none());
    }
    #[test]
    fn inverse_ray_preserves_distance_normals_and_source_geometry_under_live_transforms() {
        let mut s = SceneVisibility::new();
        let g = s.register_mesh(&tri()).unwrap();
        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(-2., 0.5, 3.),
            glam::Quat::from_rotation_y(0.61),
            Vec3::new(7., 2., -9.),
        );
        s.update_instances(&[instance(2, g, model)]).unwrap();
        let center = model.transform_point3(Vec3::ZERO);
        let normal = (model.transform_point3(Vec3::X) - center)
            .cross(model.transform_point3(Vec3::Y) - center)
            .normalize();
        let hit = s
            .trace(ray(center + normal * 7., -normal))
            .unwrap()
            .unwrap();
        assert!((hit.t - 7.).abs() < 2e-5);
        assert!(hit.normal.dot(normal) > 0.99999);
        assert!(hit.front_face);
        for p in tri().vertices {
            let p = model.transform_point3(Vec3::from_array(p.pos));
            let b = s.instance_bounds[0];
            assert!(p.cmpge(b.min).all() && p.cmple(b.max).all());
        }
    }
    #[test]
    fn alpha_coverage_uses_source_texels_and_keeps_geometry_behind_holes() {
        let mut source = tri();
        source.texture = Some(CpuImage::new(
            2,
            1,
            vec![255, 255, 255, 0, 255, 255, 255, 255],
        ));
        let mut s = SceneVisibility::new();
        let g = s.register_mesh(&source).unwrap();
        let back = s.register_mesh(&tri()).unwrap();
        let mut a = instance(1, g, Mat4::IDENTITY);
        a.material = MaterialClass::AlphaTest;
        s.update_instances(&[a, instance(2, back, Mat4::from_translation(-Vec3::Z))])
            .unwrap();
        assert_eq!(
            s.trace(ray(Vec3::Z, -Vec3::Z))
                .unwrap()
                .unwrap()
                .instance_id,
            2
        );
        for v in &mut source.vertices {
            v.uv = [-0.25, 2.25];
        }
        let covered = s.register_mesh(&source).unwrap();
        a.geometry = covered;
        s.update_instances(&[a, instance(2, back, Mat4::from_translation(-Vec3::Z))])
            .unwrap();
        assert_eq!(
            s.trace(ray(Vec3::Z, -Vec3::Z))
                .unwrap()
                .unwrap()
                .instance_id,
            1
        );
    }
    #[test]
    fn invalid_inputs_and_corrupt_escape_fail_explicitly() {
        let mut s = SceneVisibility::new();
        let g = s.register_mesh(&tri()).unwrap();
        let a = instance(3, g, Mat4::IDENTITY);
        s.update_instances(&[a]).unwrap();
        let before = s.instance_words().to_vec();
        let mut invalid = a;
        invalid.model = Mat4::from_scale(Vec3::ZERO);
        assert!(s.update_instances(&[invalid]).is_err());
        assert_eq!(s.instance_words(), before);
        assert!(s.update_instances(&[a, a]).is_err());
        let mut invalidmesh = tri();
        invalidmesh.indices[0] = 999;
        assert!(s.register_mesh(&invalidmesh).is_err());
        assert!(s.trace(ray(Vec3::ZERO, Vec3::ZERO)).is_err());
        s.instance_words[HEADER + 3] = HEADER as u32;
        assert!(s
            .trace(ray(Vec3::Z, -Vec3::Z))
            .unwrap_err()
            .0
            .contains("escape"));
    }
    #[test]
    fn deterministic_payload_matches_brute_force_original_transformed_triangles() {
        let mut s = SceneVisibility::new();
        let mut second = SceneVisibility::new();
        let mesh = mesh(
            &[[-1., -1., 0.], [1., -1., 0.], [0., 1., 0.], [0., 0., 1.]],
            &[0, 1, 2, 0, 3, 1, 1, 3, 2, 2, 3, 0],
        );
        let g = s.register_mesh(&mesh).unwrap();
        assert_eq!(g, second.register_mesh(&mesh).unwrap());
        let active: Vec<_> = (0..90)
            .map(|i| {
                instance(
                    i,
                    g,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(if i % 2 == 0 { -0.7 } else { 1.2 }, 0.8, 1.3),
                        glam::Quat::from_rotation_y(i as f32 * 0.73),
                        Vec3::new(
                            (i % 9) as f32 * 2.1 - 8.,
                            (i / 9) as f32 * 2.1 - 9.,
                            (i % 4) as f32 * 0.9,
                        ),
                    ),
                )
            })
            .collect();
        s.update_instances(&active).unwrap();
        second.update_instances(&active).unwrap();
        assert_eq!(s.geometry_words(), second.geometry_words());
        assert_eq!(s.instance_words(), second.instance_words());
        let mut seed = 41u32;
        let mut random = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        let mut hits = 0;
        for _ in 0..1000 {
            let r = ray(
                Vec3::new(random() * 22. - 11., random() * 22. - 11., 15.),
                Vec3::new((random() - 0.5) * 0.1, (random() - 0.5) * 0.1, -1.).normalize(),
            );
            let mut brute = f64::INFINITY;
            for a in &active {
                for tri in mesh.indices.chunks_exact(3) {
                    let p: [glam::DVec3; 3] = std::array::from_fn(|k| {
                        a.model.as_dmat4().transform_point3(
                            Vec3::from_array(mesh.vertices[tri[k] as usize].pos).as_dvec3(),
                        )
                    });
                    let o = r.origin.as_dvec3();
                    let d = r.direction.as_dvec3();
                    let e1 = p[1] - p[0];
                    let e2 = p[2] - p[0];
                    let h = d.cross(e2);
                    let det = e1.dot(h);
                    if det.abs() < 1e-14 {
                        continue;
                    }
                    let q = o - p[0];
                    let u = q.dot(h) / det;
                    let v = d.dot(q.cross(e1)) / det;
                    let t = e2.dot(q.cross(e1)) / det;
                    if u >= 0. && v >= 0. && u + v <= 1. && t >= 0. && t <= r.t_max as f64 {
                        brute = brute.min(t);
                    }
                }
            }
            let hit = s.trace(r).unwrap();
            assert_eq!(hit.is_some(), brute.is_finite());
            if let Some(hit) = hit {
                assert!(
                    (hit.t as f64 - brute).abs() < 0.0001,
                    "{} != {brute}",
                    hit.t
                );
                hits += 1;
            }
        }
        assert!(hits > 200);
    }
    #[test]
    fn actual_solid_grass_bend_matches_deformed_source_and_conservative_bounds() {
        let source = mesh(&[[-0.1, 0., 0.], [0.1, 0., 0.], [0., 0.48, 0.]], &[0, 1, 2]);
        let mut scene = SceneVisibility::new();
        let g = scene.register_mesh(&source).unwrap();
        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(0.8, 1.3, 0.7),
            glam::Quat::from_rotation_y(0.37),
            Vec3::new(3., 0., -2.),
        );
        let mut placement = instance(1, g, model);
        placement.bend_world = Vec2::new(2., 0.3);
        scene.update_instances(&[placement]).unwrap();
        let tip = model.inverse().transform_vector3(Vec3::new(2., 0., 0.3));
        let mut bent = source.clone();
        for v in &mut bent.vertices {
            v.pos = bend_point(Vec3::from_array(v.pos), tip).to_array();
        }
        let mut direct = SceneVisibility::new();
        let b = direct.register_mesh(&bent).unwrap();
        direct.update_instances(&[instance(1, b, model)]).unwrap();
        for u in [0.1, 0.4, 0.8, 0.98] {
            let a = model.transform_point3(Vec3::from_array(bent.vertices[0].pos));
            let b = model.transform_point3(Vec3::from_array(bent.vertices[1].pos));
            let c = model.transform_point3(Vec3::from_array(bent.vertices[2].pos));
            let p = (a + b) * 0.5 * (1. - u) + c * u;
            let n = (b - a).cross(c - a).normalize();
            let r = ray(p + n * 2., -n);
            let actual = scene.trace(r).unwrap().unwrap();
            let expected = direct.trace(r).unwrap().unwrap();
            assert!((actual.t - expected.t).abs() < 1e-5);
            assert!(actual.normal.abs_diff_eq(expected.normal, 1e-5));
        }
        let revision = scene.geometry_revision();
        placement.bend_world = Vec2::new(-2., -0.3);
        scene.update_instances(&[placement]).unwrap();
        assert_eq!(
            scene.geometry_revision(),
            revision,
            "wind changes placements, not shared source BLAS storage"
        );
        assert!(include_str!("../shaders/stylized_mesh.wgsl")
            .contains("let u = clamp(pos.y / 0.48, 0.0, 1.0)"));
    }
    #[test]
    fn private_deformation_refits_bounds_and_partial_uploads_without_growing_geometry() {
        let mut source = tri();
        let mut scene = SceneVisibility::new();
        let shared = scene.register_mesh(&source).unwrap();
        let private = scene.register_deformable_mesh(&source).unwrap();
        assert_ne!(private, shared);
        let placement = instance(1, private, Mat4::IDENTITY);
        scene.update_instances(&[placement]).unwrap();
        let words = scene.geometry_words().len();
        let revision = scene.geometry_revision();
        let instance_revision = scene.instance_revision();
        assert!(!scene.refit_mesh(private, &source).unwrap());
        assert_eq!(scene.geometry_revision(), revision);
        assert!(scene.refit_mesh(shared, &source).is_err());
        for v in &mut source.vertices {
            v.pos[2] = -3.;
        }
        assert!(scene.refit_mesh(private, &source).unwrap());
        let ranges = scene.geometry_upload_ranges(Some(revision));
        assert_eq!(ranges.len(), 1);
        assert!(ranges[0].start > shared.0 as usize);
        assert!(
            ranges[0].end < words,
            "alpha and index storage are not reuploaded for position-only poses"
        );
        assert!(
            scene
                .update_instances(&[placement])
                .unwrap()
                .changed_instances
                == 0
        );
        assert!(scene.instance_revision() > instance_revision);
        assert_eq!(scene.trace(ray(Vec3::Z, -Vec3::Z)).unwrap().unwrap().t, 4.);
        for frame in 0..100 {
            for v in &mut source.vertices {
                v.pos[2] = -(frame as f32) * 0.1;
            }
            scene.refit_mesh(private, &source).unwrap();
            scene.update_instances(&[placement]).unwrap();
        }
        assert_eq!(scene.geometry_words().len(), words);
        assert_eq!(scene.geometry_count(), 2);
        source.vertices[0].uv[0] += 0.1;
        assert!(scene.refit_mesh(private, &source).is_err());
    }
    #[test]
    fn deformed_multi_leaf_blas_matches_fresh_build_and_journal_is_bounded() {
        let mut points = Vec::new();
        let mut indices = Vec::new();
        for i in 0..17 {
            for p in [[-1., -1., 0.], [1., -1., 0.], [0., 1., 0.]] {
                points.push([p[0], p[1], i as f32 * 0.2]);
                indices.push(indices.len() as u32);
            }
        }
        let mut source = mesh(&points, &indices);
        let mut scene = SceneVisibility::new();
        let g = scene.register_deformable_mesh(&source).unwrap();
        let placement = instance(1, g, Mat4::IDENTITY);
        scene.update_instances(&[placement]).unwrap();
        for (i, v) in source.vertices.iter_mut().enumerate() {
            v.pos[2] = ((i / 3) as f32 * 1.7).sin() * 2.;
        }
        scene.refit_mesh(g, &source).unwrap();
        scene.update_instances(&[placement]).unwrap();
        let mut fresh = SceneVisibility::new();
        let f = fresh.register_mesh(&source).unwrap();
        fresh
            .update_instances(&[instance(1, f, Mat4::IDENTITY)])
            .unwrap();
        assert_eq!(
            scene
                .trace(ray(Vec3::Z * 10., -Vec3::Z))
                .unwrap()
                .unwrap()
                .t,
            fresh
                .trace(ray(Vec3::Z * 10., -Vec3::Z))
                .unwrap()
                .unwrap()
                .t
        );
        for _ in 0..4100 {
            scene.record_geometry_write(4..8);
        }
        assert_eq!(scene.geometry_writes.len(), 4096);
        assert_eq!(
            scene.geometry_upload_ranges(Some(0)),
            vec![0..scene.geometry_words().len()]
        );
    }
    #[test]
    fn registration_deduplicates_exact_content_and_checks_hash_collisions() {
        let mut source = tri();
        let mut scene = SceneVisibility::new();
        let first = scene.register_mesh(&source).unwrap();
        let words = scene.geometry_words().len();
        let revision = scene.geometry_revision();
        assert_eq!(scene.register_mesh(&source).unwrap(), first);
        assert_eq!(scene.geometry_words().len(), words);
        assert_eq!(scene.geometry_revision(), revision);
        // Normal/color changes do not alter visibility. UV, position, index order and alpha do.
        source.vertices[0].normal = [1., 0., 0.];
        assert_eq!(scene.register_mesh(&source).unwrap(), first);
        source.vertices[0].pos[0] -= 0.1;
        let hash = mesh_fingerprint(&source, 1, 1, &[255]);
        scene.geometry_cache.insert(hash, vec![first]);
        let moved = scene.register_mesh(&source).unwrap();
        assert_ne!(
            moved, first,
            "exact comparison must defeat deliberately collided hash bucket"
        );
        source.texture = Some(CpuImage::new(1, 1, vec![0, 0, 0, 255]));
        assert_eq!(scene.register_mesh(&source).unwrap(), moved);
        source.texture = Some(CpuImage::new(1, 1, vec![0, 0, 0, 0]));
        assert_ne!(scene.register_mesh(&source).unwrap(), moved);
        assert_eq!(scene.geometry_count(), 3);
    }
    #[test]
    fn division_free_shader_alpha_product_guard_matches_u64_arithmetic() {
        fn guard(a: u32, b: u32) -> bool {
            let a0 = a & 65535;
            let a1 = a >> 16;
            let b0 = b & 65535;
            let b1 = b >> 16;
            let p00 = a0 * b0;
            let p10 = a1 * b0;
            let p01 = a0 * b1;
            a1 * b1 != 0
                || (p10 >> 16) != 0
                || (p01 >> 16) != 0
                || ((p00 >> 16) + (p10 & 65535) + (p01 & 65535)) > 65535
        }
        for a in [0, 1, 2, 65535, 65536, 65537, u32::MAX] {
            for b in [0, 1, 2, 65535, 65536, 65537, u32::MAX] {
                assert_eq!(guard(a, b), a as u64 * b as u64 > u32::MAX as u64);
            }
        }
        let mut seed = 93u32;
        for _ in 0..10000 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let a = seed;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let b = seed;
            assert_eq!(guard(a, b), a as u64 * b as u64 > u32::MAX as u64);
        }
    }
    #[test]
    fn shader_parses_and_validates_with_two_portable_storage_bindings() {
        let source = shader_source(0, 0, 1);
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        assert_eq!(
            module
                .global_variables
                .iter()
                .filter(|(_, v)| matches!(v.space, wgpu::naga::AddressSpace::Storage { .. }))
                .count(),
            2
        );
    }
}

/// Physical inputs for one homogeneous source-glass material. Distinct glass materials require
/// distinct calls/a material lookup in a future renderer integrator; mismatches return Unresolved.
#[derive(Clone, Copy, Debug)]
pub struct Dielectric {
    pub material_id: u32,
    pub ior: f32,
    pub absorption_per_m: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmissionStatus {
    Clear,
    Blocked,
    Unresolved,
}
#[derive(Clone, Copy, Debug)]
pub struct VisibilityTransmission {
    pub status: TransmissionStatus,
    /// None means unresolved geometry/material, never a physically black/clear substitute.
    pub transmission: Option<Vec3>,
    pub interfaces: u32,
    pub straight_inside_distance_m: f32,
}
fn fresnel_dielectric(cos_incident: f32, eta_i: f32, eta_t: f32) -> f32 {
    let c = cos_incident.clamp(0., 1.);
    let sin2 = (eta_i / eta_t).powi(2) * (1. - c * c);
    if sin2 >= 1. {
        return 1.;
    }
    let ct = (1. - sin2).sqrt();
    let rs = (eta_i * c - eta_t * ct) / (eta_i * c + eta_t * ct);
    let rp = (eta_t * c - eta_i * ct) / (eta_t * c + eta_i * ct);
    0.5 * (rs * rs + rp * rp)
}
impl SceneVisibility {
    /// Straight finite-segment visibility through up to 16 nonnested, paired parallel panes.
    /// Each boundary is an actual source-triangle hit; Beer attenuation uses the measured length
    /// between those hits, and each interface uses unpolarized dielectric Fresnel. This is the
    /// straight-connection approximation for thin/parallel windows: no bent internal ray, lateral
    /// displacement, rough transmission, dispersion, or internal-reflection transport is claimed.
    /// Nonparallel/open/nested panes or a segment ending inside glass explicitly return Unresolved.
    /// Only the next representable ray parameter skips an interface; no fixed thickness epsilon.
    pub fn transmission(
        &self,
        ray: VisibilityRay,
        glass: Dielectric,
    ) -> Result<VisibilityTransmission> {
        self.transmission_to_emitter(ray, glass, None)
    }
    /// A registered emissive surface is the physical connection endpoint. Intervening geometry
    /// remains occluding; no source-radius sphere clips away armor or glass.
    pub fn transmission_to_emitter(
        &self,
        ray: VisibilityRay,
        glass: Dielectric,
        target_instance: Option<u32>,
    ) -> Result<VisibilityTransmission> {
        require(
            glass.ior.is_finite()
                && glass.ior >= 1.
                && glass.absorption_per_m.is_finite()
                && glass.absorption_per_m.min_element() >= 0.,
            "invalid dielectric parameters",
        )?;
        let mut result = VisibilityTransmission {
            status: TransmissionStatus::Clear,
            transmission: Some(Vec3::ONE),
            interfaces: 0,
            straight_inside_distance_m: 0.,
        };
        let unresolved = |mut result: VisibilityTransmission| {
            result.status = TransmissionStatus::Unresolved;
            result.transmission = None;
            result
        };
        let mut segment = ray;
        let speed = ray.direction.length();
        require(
            speed.is_finite() && speed > 0.,
            "invalid transmission ray direction",
        )?;
        let direction = ray.direction / speed;
        loop {
            let Some(entry) = self.trace(segment)? else {
                return Ok(result);
            };
            if target_instance == Some(entry.instance_id) {
                return Ok(result);
            }
            if entry.material == MaterialClass::Opaque || entry.material == MaterialClass::AlphaTest
            {
                result.status = TransmissionStatus::Blocked;
                result.transmission = Some(Vec3::ZERO);
                return Ok(result);
            }
            if entry.material != MaterialClass::Dielectric
                || entry.material_id != glass.material_id
                || !entry.front_face
                || result.interfaces >= 32
            {
                return Ok(unresolved(result));
            }
            segment.t_min = up(entry.t);
            if segment.t_min > segment.t_max {
                return Ok(unresolved(result));
            }
            let Some(exit) = self.trace(segment)? else {
                return Ok(unresolved(result));
            };
            if target_instance == Some(exit.instance_id) {
                return Ok(unresolved(result));
            }
            if exit.material == MaterialClass::Opaque || exit.material == MaterialClass::AlphaTest {
                result.status = TransmissionStatus::Blocked;
                result.transmission = Some(Vec3::ZERO);
                return Ok(result);
            }
            if exit.material != MaterialClass::Dielectric
                || exit.instance_id != entry.instance_id
                || exit.material_id != entry.material_id
                || exit.front_face
                || entry.normal.dot(exit.normal) > -0.9999
            {
                return Ok(unresolved(result));
            }
            let cos_incident = (-direction.dot(entry.normal)).clamp(0., 1.);
            let cos_inside = (1. - (1. - cos_incident * cos_incident) / (glass.ior * glass.ior))
                .max(0.)
                .sqrt();
            let boundary = (1. - fresnel_dielectric(cos_incident, 1., glass.ior))
                * (1. - fresnel_dielectric(cos_inside, glass.ior, 1.));
            let distance = (exit.t - entry.t) * speed;
            let beer = (-glass.absorption_per_m * distance).exp();
            result.transmission = Some(result.transmission.unwrap() * beer * boundary);
            result.straight_inside_distance_m += distance;
            result.interfaces += 2;
            segment.t_min = up(exit.t);
            if segment.t_min > segment.t_max {
                return Ok(result);
            }
        }
    }
}

#[cfg(test)]
mod dielectric_tests {
    use super::*;
    use crate::gltf_load::{CpuMesh, Vertex};
    fn pane() -> CpuMesh {
        let points = [
            [-2., -2., 1.],
            [2., -2., 1.],
            [0., 2., 1.],
            [-2., -2., 0.98],
            [0., 2., 0.98],
            [2., -2., 0.98],
        ];
        CpuMesh::basic(
            points
                .map(|pos| Vertex {
                    pos,
                    normal: [0., 0., 1.],
                    uv: [0., 0.],
                    tangent: [1., 0., 0., 1.],
                })
                .to_vec(),
            vec![0, 1, 2, 3, 4, 5],
            None,
            Vec3::ZERO,
            Vec3::ZERO,
        )
    }
    fn glass() -> Dielectric {
        Dielectric {
            material_id: 9,
            ior: 1.52,
            absorption_per_m: Vec3::new(0.080, 0.035, 0.020),
        }
    }
    fn ray() -> VisibilityRay {
        VisibilityRay {
            origin: Vec3::new(0., 0., 2.),
            direction: -Vec3::Z,
            t_min: 0.,
            t_max: 5.,
        }
    }
    #[test]
    fn paired_source_interfaces_apply_fresnel_and_measured_beer_depth() {
        let mut scene = SceneVisibility::new();
        let geometry = scene.register_mesh(&pane()).unwrap();
        let mut a = VisibilityInstance {
            bend_world: glam::Vec2::ZERO,
            lens_center_focal: [0.0; 4],
            id: 1,
            geometry,
            model: Mat4::IDENTITY,
            material: MaterialClass::Dielectric,
            material_id: 9,
        };
        scene.update_instances(&[a]).unwrap();
        let got = scene.transmission(ray(), glass()).unwrap();
        assert_eq!(got.status, TransmissionStatus::Clear);
        assert_eq!(got.interfaces, 2);
        assert!((got.straight_inside_distance_m - 0.02).abs() < 1e-6);
        let fresnel = ((1. - 1.52f32) / (1. + 1.52)).powi(2);
        let expected = (-glass().absorption_per_m * got.straight_inside_distance_m).exp()
            * (1. - fresnel).powi(2);
        assert!(got.transmission.unwrap().abs_diff_eq(expected, 1e-6));
        assert!(expected.x < expected.y && expected.y < expected.z && expected.z < 0.92);
        let b = VisibilityInstance {
            bend_world: glam::Vec2::ZERO,
            lens_center_focal: [0.0; 4],
            id: 2,
            model: Mat4::from_translation(-Vec3::Z),
            ..a
        };
        scene.update_instances(&[a, b]).unwrap();
        let twice = scene.transmission(ray(), glass()).unwrap();
        assert_eq!(twice.interfaces, 4);
        assert!(twice
            .transmission
            .unwrap()
            .abs_diff_eq(expected * expected, 1e-6));
        a.material = MaterialClass::Opaque;
        scene.update_instances(&[a, b]).unwrap();
        assert_eq!(
            scene.transmission(ray(), glass()).unwrap().transmission,
            Some(Vec3::ZERO)
        );
    }
    #[test]
    fn exact_emitter_surface_ends_connection_without_skipping_intervening_armor() {
        let mut scene = SceneVisibility::new();
        let geometry = scene.register_mesh(&pane()).unwrap();
        let emitter = VisibilityInstance {
            id: 5,
            geometry,
            model: Mat4::IDENTITY,
            material: MaterialClass::Opaque,
            material_id: 0,
            bend_world: Vec2::ZERO,
            lens_center_focal: [0.0;4],
        };
        scene.update_instances(&[emitter]).unwrap();
        assert_eq!(
            scene.transmission(ray(), glass()).unwrap().status,
            TransmissionStatus::Blocked
        );
        assert_eq!(
            scene
                .transmission_to_emitter(ray(), glass(), Some(5))
                .unwrap()
                .transmission,
            Some(Vec3::ONE)
        );
        let armor = VisibilityInstance {
            id: 6,
            model: Mat4::from_translation(Vec3::Z * 0.25),
            ..emitter
        };
        scene.update_instances(&[emitter, armor]).unwrap();
        assert_eq!(
            scene
                .transmission_to_emitter(ray(), glass(), Some(5))
                .unwrap()
                .status,
            TransmissionStatus::Blocked
        );
    }
    #[test]
    fn unsupported_glass_never_becomes_silent_clear_or_fake_opaque() {
        let mut scene = SceneVisibility::new();
        let geometry = scene.register_mesh(&pane()).unwrap();
        let a = VisibilityInstance {
            bend_world: glam::Vec2::ZERO,
            lens_center_focal: [0.0; 4],
            id: 1,
            geometry,
            model: Mat4::IDENTITY,
            material: MaterialClass::Dielectric,
            material_id: 9,
        };
        scene.update_instances(&[a]).unwrap();
        let wrong = scene
            .transmission(
                ray(),
                Dielectric {
                    material_id: 10,
                    ..glass()
                },
            )
            .unwrap();
        assert_eq!(wrong.status, TransmissionStatus::Unresolved);
        assert_eq!(wrong.transmission, None);
        let partial = scene
            .transmission(
                VisibilityRay {
                    t_max: 1.01,
                    ..ray()
                },
                glass(),
            )
            .unwrap();
        assert_eq!(partial.transmission, None);
        let inside = scene
            .transmission(
                VisibilityRay {
                    origin: Vec3::new(0., 0., 0.99),
                    ..ray()
                },
                glass(),
            )
            .unwrap();
        assert_eq!(inside.transmission, None);
        let mut open = pane();
        open.indices.truncate(3);
        let geometry = scene.register_mesh(&open).unwrap();
        scene
            .update_instances(&[VisibilityInstance {
                bend_world: glam::Vec2::ZERO,
            lens_center_focal: [0.0; 4],
                geometry,
                ..a
            }])
            .unwrap();
        assert_eq!(
            scene.transmission(ray(), glass()).unwrap().transmission,
            None
        );
    }
}

#[path="scene_environment_visibility.rs"]
pub(crate) mod environment;
