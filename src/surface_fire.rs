//! Source-surface fire and deposited foam. Geometry is copied from the same CPU meshes
//! submitted to the renderer; finite segment contacts never use the retired cabin cylinder/floor.
//! Suppression is an authored coverage/heat model, not a calibrated combustion or fluid solver.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec3};
use std::collections::HashMap;

pub const GRAIN_M: f32 = 0.008;
pub const EMISSION_RATE: f32 = 2400.0;
pub const MAX_AIRBORNE: usize = 4096;
pub const MAX_DEPOSITS: usize = 65536;
pub const FRAMES: usize = 12; //8: laid telescope;9/10: Driver shields;11: source visor handwheel
/// Maximum retained contact patches, shared with the renderer's global cell budget.
pub const MAX_PATCHES: usize = 32;
const SPREAD_INTERVAL: f32 = 0.12;
const SPREAD_ATTEMPTS: usize = 8;
const SPREAD_DISTANCE: f32 = 0.18;
const SURFACE_STEP: f32 = 0.02;
const SURFACE_LIFT: f32 = 0.004;
const FIREBREAK_COVERAGE: f32 = 0.25;
const AMMO_SCAN_INTERVAL: f32 = 0.10;
const AMMO_VISIBILITY_BUDGET: usize = 64;
const BIN: f32 = 0.20;
const COATING_BIN: f32 = 0.125;
const PATCH_RADIUS: f32 = 0.12;
const MAX_LAYERS: u8 = 4;
const RESIDUE_FADE_START_S: f64 = 60.0;
const RESIDUE_LIFETIME_S: f64 = 120.0;

pub const SPRAY_HALF_ANGLE_RAD: f32 = 0.20;

/// Deterministic circular nozzle cone. Direction remains the actual aim vector; this sampler
/// never looks up a fire or contact point. Independent avalanche streams avoid line-like jitter.
pub fn spray_velocity(direction: Vec3, sample: u32) -> Vec3 {
    let forward = direction.normalize_or_zero();
    if !forward.is_finite() || forward.length_squared() < 0.5 {
        return Vec3::ZERO;
    }
    let hash = |mut n: u32| {
        n = (n ^ (n >> 16)).wrapping_mul(0x7feb_352d);
        n = (n ^ (n >> 15)).wrapping_mul(0x846c_a68b);
        n ^ (n >> 16)
    };
    let unit = |salt| (hash(sample ^ salt) >> 8) as f32 / 16_777_216.0;
    let radius = unit(0x19d2_051d).sqrt() * SPRAY_HALF_ANGLE_RAD.tan();
    let angle = unit(0xb5a6_379f) * std::f32::consts::TAU;
    let tangent = (if forward.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    })
    .cross(forward)
    .normalize();
    let bitangent = forward.cross(tangent);
    let ray = (forward + (tangent * angle.cos() + bitangent * angle.sin()) * radius).normalize();
    ray * (4.1 + unit(0x5fd7_287b))
}

type Cell = (i32, i32, i32);
fn cell(p: Vec3, size: f32) -> Cell {
    let a = (p / size).floor().as_ivec3();
    (a.x, a.y, a.z)
}
fn cells(lo: Vec3, hi: Vec3, size: f32) -> impl Iterator<Item = Cell> {
    let a = cell(lo, size);
    let b = cell(hi, size);
    (a.2..=b.2)
        .flat_map(move |z| (a.1..=b.1).flat_map(move |y| (a.0..=b.0).map(move |x| (x, y, z))))
}
#[derive(Clone, Copy)]
struct Triangle {
    a: Vec3,
    b: Vec3,
    c: Vec3,
}
#[derive(Default)]
struct Grid {
    triangles: Vec<Triangle>,
    bins: HashMap<Cell, Vec<usize>>,
}
impl Grid {
    fn append(&mut self, mesh: &CpuMesh) {
        for ids in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.vertices[ids[i] as usize].pos));
            if (b - a).cross(c - a).length_squared() < 1e-16 {
                continue;
            }
            let index = self.triangles.len();
            self.triangles.push(Triangle { a, b, c });
            for key in cells(
                a.min(b).min(c) - Vec3::splat(1e-5),
                a.max(b).max(c) + Vec3::splat(1e-5),
                BIN,
            ) {
                self.bins.entry(key).or_default().push(index);
            }
        }
    }
    /// Exact intervals of a segment supported by source triangle interiors. Joining these
    /// intervals proves continuity across triangulation seams; discrete probes alone skip gaps.
    fn support_intervals(&self, from: Vec3, to: Vec3, intervals: &mut Vec<(f32, f32)>) {
        let mut candidates = Vec::new();
        for key in cells(
            from.min(to) - Vec3::splat(1e-4),
            from.max(to) + Vec3::splat(1e-4),
            BIN,
        ) {
            if let Some(ids) = self.bins.get(&key) {
                candidates.extend_from_slice(ids);
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        for index in candidates {
            let Triangle { a, b, c } = self.triangles[index];
            let e0 = b - a;
            let e1 = c - a;
            let normal = e0.cross(e1).normalize();
            if normal.dot(from - a).abs() > 1e-4 || normal.dot(to - a).abs() > 1e-4 {
                continue;
            }
            let d00 = e0.dot(e0);
            let d01 = e0.dot(e1);
            let d11 = e1.dot(e1);
            let denom = d00 * d11 - d01 * d01;
            if denom.abs() < 1e-16 {
                continue;
            }
            let bary = |point: Vec3| {
                let d = point - a;
                let u = (d11 * d.dot(e0) - d01 * d.dot(e1)) / denom;
                let v = (d00 * d.dot(e1) - d01 * d.dot(e0)) / denom;
                [1.0 - u - v, u, v]
            };
            let start = bary(from);
            let end = bary(to);
            let mut lo = 0.0_f32;
            let mut hi = 1.0_f32;
            for i in 0..3 {
                let slope = end[i] - start[i];
                if slope.abs() < 1e-9 {
                    if start[i] < -1e-6 {
                        hi = -1.0;
                    }
                } else if slope > 0.0 {
                    lo = lo.max((-1e-6 - start[i]) / slope);
                } else {
                    hi = hi.min((-1e-6 - start[i]) / slope);
                }
            }
            if lo <= hi && hi >= 0.0 && lo <= 1.0 {
                intervals.push((lo.max(0.0), hi.min(1.0)));
            }
        }
    }
    fn sweep(&self, from: Vec3, to: Vec3) -> Option<(f32, Vec3)> {
        let delta = to - from;
        let mut best: Option<(f32, Vec3)> = None;
        let mut candidates = Vec::new();
        for key in cells(
            from.min(to) - Vec3::splat(1e-5),
            from.max(to) + Vec3::splat(1e-5),
            BIN,
        ) {
            if let Some(ids) = self.bins.get(&key) {
                candidates.extend_from_slice(ids);
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        for index in candidates {
            let Triangle { a, b, c } = self.triangles[index];
            let e1 = b - a;
            let e2 = c - a;
            let cross = delta.cross(e2);
            let det = e1.dot(cross);
            if det.abs() < 1e-10 {
                continue;
            }
            let t = from - a;
            let u = t.dot(cross) / det;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = t.cross(e1);
            let v = delta.dot(q) / det;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let fraction = e2.dot(q) / det;
            if (0.0..=1.0).contains(&fraction) && best.is_none_or(|(f, _)| fraction < f) {
                let mut normal = e1.cross(e2).normalize();
                if normal.dot(delta) > 0.0 {
                    normal = -normal;
                }
                best = Some((fraction, normal));
            }
        }
        best
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub frame: usize,
    pub point: Vec3,
    pub normal: Vec3,
    pub fraction: f32,
}
/// Read-only batch access to the same source triangles used by fire and foam. Retaining this
/// query for an update amortizes inverses across every wet grain and film frontier candidate.
pub struct SourceContacts<'a> {
    source: &'a CabinFire,
    inverse: [Mat4; FRAMES],
}
impl SourceContacts<'_> {
    pub fn sweep_batch(&self, paths: &[[Vec3; 2]]) -> Vec<Option<Contact>> {
        paths.iter().map(|p| self.source.sweep_inverse(p[0], p[1], &self.inverse)).collect()
    }
    pub fn supports_batch(&self, frame: usize, paths: &[[Vec3; 2]]) -> Vec<bool> {
        paths.iter().map(|p| {
            if frame >= FRAMES { return false; }
            let mut intervals = Vec::new();
            self.source.grids[frame].support_intervals(p[0], p[1], &mut intervals);
            intervals.sort_unstable_by(|a,b| a.0.total_cmp(&b.0));
            let mut end = 0.0_f32;
            for (lo, hi) in intervals {
                if lo > end + 1e-5 { return false; }
                end = end.max(hi);
                if end >= 1.0 - 1e-5 { return true; }
            }
            false
        }).collect()
    }
    /// Subtract the union of coplanar source triangles from the entire convex cell footprint.
    /// Empty remainder proves area support across UV seams; edge-only tests miss interior holes.
    /// Pathological fragment growth fails closed at a fixed scratch bound.
    pub fn supports_cell(&self, frame: usize, corners: &[Vec3; 4]) -> bool {
        if frame >= FRAMES || corners.iter().any(|p| !p.is_finite()) { return false; }
        let origin = corners[0];
        let u = (corners[1] - origin).normalize_or_zero();
        let n = (corners[1] - origin).cross(corners[3] - origin).normalize_or_zero();
        if n.length_squared() < 0.5 { return false; }
        let v = n.cross(u);
        let project = |p: Vec3| glam::Vec2::new((p-origin).dot(u), (p-origin).dot(v));
        let mut remainder = vec![corners.iter().map(|&p| project(p)).collect::<Vec<_>>()];
        let mut candidates = Vec::new();
        let lo = corners.iter().copied().fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let hi = corners.iter().copied().fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        let grid = &self.source.grids[frame];
        for key in cells(lo - Vec3::splat(1e-4), hi + Vec3::splat(1e-4), BIN) {
            if let Some(ids) = grid.bins.get(&key) { candidates.extend_from_slice(ids); }
        }
        candidates.sort_unstable();
        candidates.dedup();
        for index in candidates {
            let t = grid.triangles[index];
            let triangle_normal = (t.b-t.a).cross(t.c-t.a).normalize();
            if corners.iter().any(|p| (*p-t.a).dot(triangle_normal).abs() > 1e-4) { continue; }
            let mut tri = [project(t.a),project(t.b),project(t.c)];
            if (tri[1]-tri[0]).perp_dot(tri[2]-tri[0]) < 0.0 { tri.swap(1,2); }
            let mut uncovered = Vec::new();
            for polygon in remainder {
                let mut inside = polygon;
                for edge in 0..3 {
                    if inside.len() < 3 { break; }
                    let a = tri[edge]; let b = tri[(edge+1)%3];
                    let (kept, outside) = split_source_polygon(&inside,a,b);
                    if source_polygon_area(&outside) > 1e-12 { uncovered.push(outside); }
                    inside = kept;
                }
                if uncovered.len() > 64 { return false; }
            }
            remainder = uncovered;
            if remainder.is_empty() { return true; }
        }
        // Sub-square-micron numeric remnants may survive subtraction at shared float seams.
        remainder.iter().map(|p| source_polygon_area(p)).sum::<f32>() <= 1e-10
    }
}
fn source_polygon_area(p: &[glam::Vec2]) -> f32 {
    if p.len() < 3 { return 0.0; }
    (0..p.len()).map(|i| p[i].perp_dot(p[(i+1)%p.len()])).sum::<f32>().abs()*0.5
}
fn split_source_polygon(p: &[glam::Vec2], a: glam::Vec2, b: glam::Vec2)
    -> (Vec<glam::Vec2>,Vec<glam::Vec2>) {
    let mut inside = Vec::new(); let mut outside = Vec::new();
    if p.is_empty() { return (inside,outside); }
    for i in 0..p.len() {
        let x=p[i]; let y=p[(i+1)%p.len()];
        let dx=(b-a).perp_dot(x-a); let dy=(b-a).perp_dot(y-a);
        if dx >= 0.0 { inside.push(x); } else { outside.push(x); }
        if (dx < 0.0) != (dy < 0.0) {
            let crossing=x.lerp(y,dx/(dx-dy));
            inside.push(crossing); outside.push(crossing);
        }
    }
    (inside,outside)
}
#[derive(Clone, Copy)]
struct Grain {
    position: Vec3,
    velocity: Vec3,
    age: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Deposit {
    pub frame: usize,
    pub point: Vec3,
    pub normal: Vec3,
    pub layer: u8,
    expires_at: f64,
}
#[derive(Clone, Copy)]
struct FoamColumn {
    layers: u8,
    expires_at: f64,
}
type ColumnKey = (usize, Cell, u8);
fn column_key(frame: usize, point: Vec3, normal: Vec3) -> ColumnKey {
    let axis = normal.abs().max_element();
    let i = if normal.x.abs() == axis { 0 } else if normal.y.abs() == axis { 1 } else { 2 };
    (frame, cell(point, GRAIN_M), i as u8 * 2 + u8::from(normal[i] < 0.0))
}
fn residue_expiry(key: ColumnKey, now: f64) -> f64 {
    let mut hash = 0x811c_9dc5_u32;
    for value in [key.0 as u32, key.1.0 as u32, key.1.1 as u32, key.1.2 as u32, key.2 as u32] {
        hash = (hash ^ value).wrapping_mul(0x0100_0193);
    }
    hash = (hash ^ (hash >> 16)).wrapping_mul(0x7feb_352d);
    hash ^= hash >> 15;
    // Opaque grains thin out progressively, without changing their material/shader. Whole-second
    // expiry buckets keep rebuilding the coverage index bounded to once per simulation second.
    (now + RESIDUE_FADE_START_S + f64::from(hash % 60)).ceil().min(now + RESIDUE_LIFETIME_S)
}
#[derive(Clone, Copy, Debug, Default)]
struct Patch {
    anchor: Option<Contact>,
    intensity: f32,
    heat: f32,
    spread_direction: u8,
}
#[derive(Clone, Copy, Debug)]
pub struct FireUpdate {
    pub strength: f32,
    pub active: bool,
    pub ammo_heat: f32,
    /// Index into this update's submitted live-ammunition slice, never a retained inventory ID.
    pub hottest_ammunition: Option<usize>,
}
#[derive(Clone, Copy, Debug)]
pub struct FireSurface {
    pub position: Vec3,
    pub normal: Vec3,
    pub tangent: Vec3,
    pub bitangent: Vec3,
    pub intensity: f32,
}

pub struct CabinFire {
    grids: [Grid; FRAMES],
    airborne: Vec<Grain>,
    pub deposits: Vec<Deposit>,
    // Column identity includes the actual contacted frame, 3D surface position and face direction.
    columns: HashMap<ColumnKey, FoamColumn>,
    coating_bins: HashMap<(usize, Cell), Vec<usize>>,
    residue_cleanup: bool,
    foam_clock: f64,
    next_residue_expiry: f64,
    patches: [Patch; MAX_PATCHES],
    heat_at_ammunition: f32,
    spread_clock: f32,
    spread_cursor: usize,
    ammo_scan_clock: f32,
    ammo_exposure: f32,
    hottest_ammunition: Option<usize>,
    last_ammunition_count: usize,
}
impl CabinFire {
    pub fn new(meshes: &[(usize, &CpuMesh)]) -> Self {
        let mut grids = std::array::from_fn(|_| Grid::default());
        for &(frame, mesh) in meshes {
            grids[frame].append(mesh);
        }
        Self {
            grids,
            airborne: Vec::with_capacity(MAX_AIRBORNE),
            deposits: Vec::with_capacity(MAX_DEPOSITS),
            columns: HashMap::new(),
            coating_bins: HashMap::new(),
            residue_cleanup: false,
            foam_clock: 0.0,
            next_residue_expiry: f64::INFINITY,
            patches: [Patch::default(); MAX_PATCHES],
            heat_at_ammunition: 0.0,
            spread_clock: SPREAD_INTERVAL,
            spread_cursor: 0,
            ammo_scan_clock: 0.0,
            ammo_exposure: 0.0,
            hottest_ammunition: None,
            last_ammunition_count: 0,
        }
    }
    /// Opt-in Playtest housekeeping; campaign coating behaviour remains unchanged. Configure
    /// before the first spray. Only advance_foam's simulation delta advances this clock.
    pub fn enable_residue_cleanup(&mut self) {
        self.residue_cleanup = true;
    }
    pub fn reset(&mut self) {
        self.airborne.clear();
        self.deposits.clear();
        self.columns.clear();
        self.coating_bins.clear();
        self.foam_clock = 0.0;
        self.next_residue_expiry = f64::INFINITY;
        self.patches.fill(Patch::default());
        self.heat_at_ammunition = 0.0;
        self.spread_clock = SPREAD_INTERVAL;
        self.spread_cursor = 0;
        self.ammo_scan_clock = 0.0;
        self.ammo_exposure = 0.0;
        self.hottest_ammunition = None;
        self.last_ammunition_count = 0;
    }
    pub fn counts(&self) -> (usize, usize) {
        (
            self.airborne.len() + self.deposits.len(),
            self.airborne.len(),
        )
    }
    pub fn diagnostics(&self) -> (usize, f32, f32) {
        (
            self.deposits.len(),
            self.patches[0]
                .anchor
                .map(|a| self.coverage_at(a))
                .unwrap_or(0.0),
            self.heat_at_ammunition,
        )
    }
    fn sweep_inverse(&self, from: Vec3, to: Vec3, inverse: &[Mat4; FRAMES]) -> Option<Contact> {
        let mut best: Option<Contact> = None;
        for frame in 0..FRAMES {
            let a = inverse[frame].transform_point3(from);
            let b = inverse[frame].transform_point3(to);
            if let Some((fraction, normal)) = self.grids[frame].sweep(a, b) {
                if best.is_none_or(|old| fraction < old.fraction) {
                    best = Some(Contact {
                        frame,
                        point: a.lerp(b, fraction),
                        normal,
                        fraction,
                    });
                }
            }
        }
        best
    }
    pub fn source_contacts(&self, models: &[Mat4; FRAMES]) -> SourceContacts<'_> {
        SourceContacts { source: self, inverse: models.map(|m| m.inverse()) }
    }
    pub fn sweep(&self, from: Vec3, to: Vec3, models: &[Mat4; FRAMES]) -> Option<Contact> {
        self.sweep_inverse(from, to, &models.map(|m| m.inverse()))
    }
    /// Reuse the source triangle bins for a bounded soot candidate batch. Frame inverses are
    /// calculated once for the whole batch; this never scans every source triangle per grain.
    /// The path endpoint should include the caller's small cell-radius margin.
    pub fn clear_source_paths(
        &self,
        origin: Vec3,
        endpoints: &[Vec3],
        models: &[Mat4; FRAMES],
    ) -> Vec<bool> {
        let inverse = models.map(|model| model.inverse());
        endpoints
            .iter()
            .map(|&point| {
                origin.is_finite()
                    && point.is_finite()
                    && self.sweep_inverse(origin, point, &inverse).is_none()
            })
            .collect()
    }

    pub fn emit(&mut self, position: Vec3, velocity: Vec3) {
        if self.airborne.len() < MAX_AIRBORNE {
            self.airborne.push(Grain {
                position,
                velocity,
                age: 0.0,
            });
        }
    }
    fn deposit(&mut self, hit: Contact) {
        if self.deposits.len() >= MAX_DEPOSITS {
            return;
        }
        let key = column_key(hit.frame, hit.point, hit.normal);
        let expires_at = if self.residue_cleanup {
            residue_expiry(key, self.foam_clock)
        } else {
            f64::INFINITY
        };
        let column = self.columns.entry(key).or_insert(FoamColumn { layers: 0, expires_at });
        if column.layers >= MAX_LAYERS {
            return;
        }
        let value = Deposit {
            frame: hit.frame,
            point: hit.point,
            normal: hit.normal,
            layer: column.layers,
            expires_at: column.expires_at,
        };
        column.layers += 1;
        self.next_residue_expiry = self.next_residue_expiry.min(value.expires_at);
        // Only the first layer counts toward area coverage; extra layers build visible thickness.
        if value.layer == 0 {
            self.coating_bins
                .entry((hit.frame, cell(hit.point, COATING_BIN)))
                .or_default()
                .push(self.deposits.len());
        }
        self.deposits.push(value);
    }
    fn expire_residue(&mut self) {
        if self.foam_clock < self.next_residue_expiry {
            return;
        }
        self.deposits.retain(|deposit| deposit.expires_at > self.foam_clock);
        self.columns.retain(|_, column| column.expires_at > self.foam_clock);
        // Compaction changes deposit indices. Rebuild only the coverage lookup, never fire state:
        // disappearing residue cannot reignite a fire or leave invisible permanent protection.
        self.coating_bins.clear();
        self.next_residue_expiry = f64::INFINITY;
        for (index, deposit) in self.deposits.iter().enumerate() {
            self.next_residue_expiry = self.next_residue_expiry.min(deposit.expires_at);
            if deposit.layer == 0 {
                self.coating_bins.entry((deposit.frame, cell(deposit.point, COATING_BIN)))
                    .or_default().push(index);
            }
        }
    }
    pub fn advance_foam(&mut self, dt: f32, models: &[Mat4; FRAMES]) {
        let dt = dt.clamp(0.0, 0.05);
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.foam_clock += f64::from(dt);
        self.expire_residue();
        let inverse = models.map(|m| m.inverse());
        let mut flying = std::mem::take(&mut self.airborne);
        let mut i = 0;
        while i < flying.len() {
            let f = &mut flying[i];
            f.age += dt;
            f.velocity *= (-1.6 * dt).exp();
            f.velocity.y -= 5.5 * dt;
            let next = f.position + f.velocity * dt;
            if let Some(hit) = self.sweep_inverse(f.position, next, &inverse) {
                self.deposit(hit);
                flying.swap_remove(i);
            } else if f.age > 2.0 {
                flying.swap_remove(i);
            } else {
                f.position = next;
                i += 1;
            }
        }
        self.airborne = flying;
    }
    pub fn render_points(&self, models: &[Mat4; FRAMES]) -> [Vec<Vec3>; 2] {
        [
            self.airborne.iter().map(|p| p.position).collect(),
            self.deposits
                .iter()
                .map(|p| {
                    models[p.frame]
                        .transform_point3(p.point + p.normal * (GRAIN_M * (p.layer as f32 + 0.5)))
                })
                .collect(),
        ]
    }
    /// Compatibility ignition proposal: retain the previous first floor contact, but never
    /// install three distant preset emitters. Every further patch is discovered on source geometry.
    pub fn ensure_anchors(&mut self, models: &[Mat4; FRAMES], proposals: [Vec3; 4]) {
        if self.patches[0].anchor.is_none() {
            let p = proposals[0];
            self.patches[0].anchor = self.sweep(p, p - Vec3::Y * 2.5, models);
        }
    }
    /// Call when the caller's stable cartridge-ID mapping changes without changing slice length.
    /// Positions alone cannot distinguish a new cartridge inserted into the same rack slot.
    pub fn invalidate_ammunition_exposure(&mut self) {
        self.heat_at_ammunition = 0.0;
        self.ammo_exposure = 0.0;
        self.hottest_ammunition = None;
        self.ammo_scan_clock = 0.0;
        self.last_ammunition_count = 0;
    }
    pub fn seed_from_segment(
        &mut self,
        from: Vec3,
        to: Vec3,
        models: &[Mat4; FRAMES],
        strength: f32,
    ) -> bool {
        let Some(hit) = self.sweep(from, to, models) else {
            return false;
        };
        if self.coverage_at(hit) >= FIREBREAK_COVERAGE || !strength.is_finite() || strength <= 0.0 {
            return false;
        }
        let Some(index) = self
            .patches
            .iter()
            .position(|p| p.anchor.is_none() || p.intensity <= 0.0)
        else {
            return false;
        };
        self.patches[index] = Patch {
            anchor: Some(hit),
            intensity: strength.clamp(0.16, 1.4),
            ..Patch::default()
        };
        self.ammo_scan_clock = 0.0;
        true
    }
    pub fn patch_surface(&self, k: usize, models: &[Mat4; FRAMES]) -> Option<FireSurface> {
        let patch = self.patches.get(k)?;
        patch.anchor.map(|a| {
            let normal = models[a.frame]
                .inverse()
                .transpose()
                .transform_vector3(a.normal)
                .normalize();
            let tangent = (if normal.y.abs() < 0.9 {
                Vec3::Y.cross(normal)
            } else {
                Vec3::X.cross(normal)
            })
            .normalize();
            FireSurface {
                position: models[a.frame].transform_point3(a.point) + normal * 0.003,
                normal,
                tangent,
                bitangent: normal.cross(tangent).normalize(),
                intensity: patch.intensity,
            }
        })
    }
    pub fn patch_world(&self, k: usize, models: &[Mat4; FRAMES]) -> Option<(Vec3, f32)> {
        self.patch_surface(k, models)
            .map(|surface| (surface.position, surface.intensity))
    }
    pub fn spread_diagnostics(&self, models: &[Mat4; FRAMES]) -> (usize, usize) {
        let mut active = 0;
        let mut wall = 0;
        for i in 0..MAX_PATCHES {
            if let Some(s) = self.patch_surface(i, models) {
                if s.intensity > 0.0 {
                    active += 1;
                    if s.normal.y.abs() < 0.7 {
                        wall += 1;
                    }
                }
            }
        }
        (active, wall)
    }
    fn supports_segment(&self, from: Vec3, to: Vec3, inverse: &[Mat4; FRAMES]) -> bool {
        let mut intervals = Vec::new();
        for (frame, transform) in inverse.iter().enumerate() {
            self.grids[frame].support_intervals(
                transform.transform_point3(from),
                transform.transform_point3(to),
                &mut intervals,
            );
        }
        intervals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let mut end = 0.0_f32;
        for (lo, hi) in intervals {
            if lo > end + 1e-5 {
                return false;
            }
            end = end.max(hi);
            if end >= 1.0 - 1e-5 {
                return true;
            }
        }
        false
    }
    /// Walk a connected surface, turning up a contacted wall at a genuine floor/wall junction.
    /// Every straight section is covered by the union of actual source triangles. No radius-based
    /// proximity link can leap a crack, separated panel, opposing armor face, or coated firebreak.
    fn follow_surface(
        &self,
        initial: Contact,
        mut direction: Vec3,
        models: &[Mat4; FRAMES],
        inverse: &[Mat4; FRAMES],
    ) -> Option<Contact> {
        let mut hit = initial;
        for _ in 0..(SPREAD_DISTANCE / SURFACE_STEP).round() as usize {
            if self.coverage_at(hit) >= FIREBREAK_COVERAGE {
                return None;
            }
            let p = models[hit.frame].transform_point3(hit.point);
            let n = inverse[hit.frame]
                .transpose()
                .transform_vector3(hit.normal)
                .normalize();
            direction = (direction - n * direction.dot(n)).normalize_or_zero();
            if direction.length_squared() < 0.5 {
                return None;
            }
            let lifted = p + n * SURFACE_LIFT;
            let next = lifted + direction * SURFACE_STEP;
            if let Some(wall) = self.sweep_inverse(lifted, next, inverse) {
                let q = models[wall.frame].transform_point3(wall.point);
                let wn = inverse[wall.frame]
                    .transpose()
                    .transform_vector3(wall.normal)
                    .normalize();
                let turn = n - wn * n.dot(wn);
                let denominator = turn.dot(n);
                if denominator < 0.02 {
                    return None;
                }
                let junction = q - turn * ((q - p).dot(n) / denominator);
                if p.distance(junction) > SURFACE_STEP + 0.001
                    || q.distance(junction) > SURFACE_LIFT * 2.0
                    || !self.supports_segment(p, junction, inverse)
                    || !self.supports_segment(junction, q, inverse)
                {
                    return None;
                }
                direction = (direction - wn * direction.dot(wn)) + turn;
                hit = wall;
            } else {
                let candidate =
                    self.sweep_inverse(next, next - n * (SURFACE_LIFT * 2.0), inverse)?;
                let q = models[candidate.frame].transform_point3(candidate.point);
                let cn = inverse[candidate.frame]
                    .transpose()
                    .transform_vector3(candidate.normal)
                    .normalize();
                if cn.dot(n) < 0.995 || !self.supports_segment(p, q, inverse) {
                    return None;
                }
                hit = candidate;
            }
        }
        (self.coverage_at(hit) < FIREBREAK_COVERAGE).then_some(hit)
    }
    fn spread(&mut self, models: &[Mat4; FRAMES], inverse: &[Mat4; FRAMES]) {
        let active: Vec<usize> = self
            .patches
            .iter()
            .enumerate()
            .filter_map(|(i, p)| (p.intensity > 0.08 && p.anchor.is_some()).then_some(i))
            .collect();
        if active.is_empty() {
            return;
        }
        for _ in 0..SPREAD_ATTEMPTS {
            let Some(slot) = self.patches.iter().position(|p| p.anchor.is_none()) else {
                break;
            };
            let index = active[self.spread_cursor % active.len()];
            self.spread_cursor += 1;
            let patch = self.patches[index];
            let surface = self.patch_surface(index, models).unwrap();
            let angle = f32::from(patch.spread_direction % 8) * std::f32::consts::FRAC_PI_4;
            self.patches[index].spread_direction = patch.spread_direction.wrapping_add(1);
            let dir = surface.tangent * angle.cos() + surface.bitangent * angle.sin();
            let Some(contact) = self.follow_surface(patch.anchor.unwrap(), dir, models, inverse)
            else {
                continue;
            };
            let world = models[contact.frame].transform_point3(contact.point);
            let normal = inverse[contact.frame]
                .transpose()
                .transform_vector3(contact.normal)
                .normalize();
            let duplicate = (0..MAX_PATCHES)
                .filter_map(|k| self.patch_surface(k, models))
                .any(|other| {
                    other.position.distance(world) < 0.135 && other.normal.dot(normal) > 0.7
                });
            if duplicate {
                continue;
            }
            self.patches[slot] = Patch {
                anchor: Some(contact),
                intensity: (patch.intensity * 0.72).max(0.18),
                // Continue the parent's outward direction first; restarting every new patch at
                // angle zero biases the bounded frontier toward one compass direction.
                spread_direction: patch.spread_direction,
                ..Patch::default()
            };
        }
    }
    fn scan_ammunition(
        &mut self,
        models: &[Mat4; FRAMES],
        inverse: &[Mat4; FRAMES],
        ammunition: &[Vec3],
    ) {
        let mut candidates = Vec::new();
        for (index, round) in ammunition.iter().enumerate() {
            for k in 0..MAX_PATCHES {
                let Some(surface) = self.patch_surface(k, models) else {
                    continue;
                };
                if surface.intensity <= 0.0 {
                    continue;
                }
                let distance2 = surface.position.distance_squared(*round);
                // Finite cabin exposure range; strong nearby visible fire is dangerous in seconds.
                if distance2 <= 2.5 * 2.5 {
                    let exposure = (surface.intensity * 1.35 / (1.0 + distance2 * 2.0)).min(1.4);
                    candidates.push((exposure, index, surface.position));
                }
            }
        }
        candidates.sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let previous = self.hottest_ammunition;
        let previous_count = self.last_ammunition_count;
        self.ammo_exposure = 0.0;
        self.hottest_ammunition = None;
        for (exposure, index, source) in candidates.into_iter().take(AMMO_VISIBILITY_BUDGET) {
            if self
                .sweep_inverse(source, ammunition[index], inverse)
                .is_none()
            {
                self.ammo_exposure = exposure;
                self.hottest_ammunition = Some(index);
                break;
            }
        }
        if self.hottest_ammunition != previous || previous_count != ammunition.len() {
            self.heat_at_ammunition = 0.0;
        }
        self.last_ammunition_count = ammunition.len();
    }
    pub fn coverage_at(&self, anchor: Contact) -> f32 {
        let radius = PATCH_RADIUS;
        let mut cells_covered = 0usize;
        for key in cells(
            anchor.point - Vec3::splat(radius),
            anchor.point + Vec3::splat(radius),
            COATING_BIN,
        ) {
            if let Some(ids) = self.coating_bins.get(&(anchor.frame, key)) {
                for &i in ids {
                    let d = self.deposits[i];
                    let delta = d.point - anchor.point;
                    if d.normal.dot(anchor.normal) > 0.70
                        && delta.dot(anchor.normal).abs() < 0.025
                        && delta.length_squared() <= radius * radius
                    {
                        cells_covered += 1;
                    }
                }
            }
        }
        (cells_covered as f32 * GRAIN_M * GRAIN_M / (std::f32::consts::PI * radius * radius))
            .clamp(0.0, 1.0)
    }
    pub fn update_fire(
        &mut self,
        dt: f32,
        active: bool,
        seed_strength: f32,
        models: &[Mat4; FRAMES],
        ammunition: &[Vec3],
        venting: bool,
    ) -> FireUpdate {
        let dt = dt.clamp(0.0, 0.05);
        if !active {
            for (index, p) in self.patches.iter_mut().enumerate() {
                if index == 0 {
                    p.intensity = 0.0;
                    p.heat = 0.0;
                } else {
                    *p = Patch::default();
                }
            }
            self.heat_at_ammunition = 0.0;
            self.ammo_exposure = 0.0;
            self.hottest_ammunition = None;
            self.spread_clock = SPREAD_INTERVAL;
            self.ammo_scan_clock = 0.0;
            return FireUpdate {
                strength: 0.0,
                active: false,
                ammo_heat: 0.0,
                hottest_ammunition: None,
            };
        }
        if self.patches.iter().all(|p| p.intensity <= 0.0) && self.patches[0].anchor.is_some() {
            self.patches[0].intensity = seed_strength.max(0.16);
        }
        let current = self
            .patches
            .iter()
            .map(|p| p.intensity)
            .fold(0.0_f32, f32::max);
        if seed_strength > current + 0.01 && self.patches[0].anchor.is_some() {
            self.patches[0].intensity = seed_strength.min(1.4);
        }
        for i in 0..MAX_PATCHES {
            let Some(anchor) = self.patches[i].anchor else {
                continue;
            };
            let coating = self.coverage_at(anchor);
            let p = &mut self.patches[i];
            // Local fuel growth only. There is no heat exchange through arbitrary 3D proximity.
            if p.intensity > 0.0 {
                let growth = if venting { 0.22 } else { 0.30 };
                p.intensity =
                    (p.intensity + dt * (growth * (1.0 - coating) - coating * 1.6)).clamp(0.0, 1.4);
                p.heat = p.intensity;
                if p.intensity < 0.025 && coating > 0.35 {
                    p.intensity = 0.0;
                    p.heat = 0.0;
                }
            }
        }
        let inverse = models.map(|m| m.inverse());
        self.spread_clock -= dt;
        if self.spread_clock <= 0.0 {
            self.spread_clock = SPREAD_INTERVAL;
            self.spread(models, &inverse);
        }
        let strength = self
            .patches
            .iter()
            .map(|p| p.intensity)
            .fold(0.0_f32, f32::max);
        if ammunition.is_empty() {
            self.heat_at_ammunition = 0.0;
            self.ammo_exposure = 0.0;
            self.hottest_ammunition = None;
            self.last_ammunition_count = 0;
            self.ammo_scan_clock = 0.0;
        } else {
            self.ammo_scan_clock -= dt;
            if self.ammo_scan_clock <= 0.0 || self.last_ammunition_count != ammunition.len() {
                self.scan_ammunition(models, &inverse, ammunition);
                self.ammo_scan_clock = AMMO_SCAN_INTERVAL;
            }
            // Smoothed dimensionless exposure, not a physical ammunition temperature.
            self.heat_at_ammunition +=
                (self.ammo_exposure - self.heat_at_ammunition) * (1.0 - (-dt * 1.8).exp());
        }
        FireUpdate {
            strength,
            active: strength > 0.0,
            ammo_heat: self.heat_at_ammunition,
            hottest_ammunition: self
                .hottest_ammunition
                .filter(|index| *index < ammunition.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plane() -> CpuMesh {
        crate::prims::box_mesh(
            Vec3::new(-2.0, -0.1, -2.0),
            Vec3::new(2.0, 0.0, 2.0),
            [128, 128, 128],
        )
    }
    fn patch(f: &mut CabinFire, models: &[Mat4; FRAMES]) {
        f.ensure_anchors(models, [Vec3::new(0.0, 0.1, 0.0); 4]);
        for i in 1..MAX_PATCHES {
            f.patches[i].anchor = None;
        }
    }
    fn coat(f: &mut CabinFire, centre: Vec3) {
        for z in -15..=15 {
            for x in -15..=15 {
                f.deposit(Contact {
                    frame: 0,
                    point: centre + Vec3::new(x as f32 * GRAIN_M, 0.0, z as f32 * GRAIN_M),
                    normal: Vec3::Y,
                    fraction: 0.5,
                });
            }
        }
    }
    #[test]
    fn source_path_batches_reject_roof_and_wall_crossings_in_moving_frames() {
        let floor = plane();
        let roof = crate::prims::box_mesh(
            Vec3::new(-2.0, 0.7, -2.0),
            Vec3::new(2.0, 0.8, 2.0),
            [80; 3],
        );
        let wall =
            crate::prims::box_mesh(Vec3::new(0.5, 0.0, -2.0), Vec3::new(0.6, 0.7, 2.0), [80; 3]);
        let fire = CabinFire::new(&[(0, &floor), (1, &roof), (2, &wall)]);
        let origin = Vec3::new(0.0, 0.02, 0.0);
        let points = [
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.9, 0.4, 0.0),
            Vec3::new(0.4, 0.6, 0.0),
        ];
        let expected = vec![true, false, false, true];
        assert_eq!(
            fire.clear_source_paths(origin, &points, &[Mat4::IDENTITY; FRAMES]),
            expected
        );
        let motion = Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(0.7),
            Vec3::new(12.0, 2.0, -9.0),
        );
        assert_eq!(
            fire.clear_source_paths(
                motion.transform_point3(origin),
                &points.map(|point| motion.transform_point3(point)),
                &[motion; FRAMES]
            ),
            expected
        );
    }

    #[test]
    fn swept_source_foam_persists_and_follows_its_contacted_frame() {
        let mesh = plane();
        let mut f = CabinFire::new(&[(0, &mesh)]);
        let mut models = [Mat4::IDENTITY; FRAMES];
        f.emit(Vec3::new(0.0, 0.05, 0.0), Vec3::NEG_Y * 20.0);
        f.advance_foam(0.05, &models);
        assert_eq!(f.counts(), (1, 0));
        assert!((f.deposits[0].point.y).abs() < 1e-5);
        for _ in 0..1000 {
            f.advance_foam(0.05, &models);
        }
        assert_eq!(f.counts(), (1, 0));
        models[0] = Mat4::from_translation(Vec3::new(3.0, 2.0, 1.0));
        assert!(f.render_points(&models)[1][0].distance(Vec3::new(3.0, 2.004, 1.0)) < 1e-4);
    }
    #[test]
    fn layers_stack_without_inflating_covered_area() {
        let mesh = plane();
        let mut f = CabinFire::new(&[(0, &mesh)]);
        let h = Contact {
            frame: 0,
            point: Vec3::ZERO,
            normal: Vec3::Y,
            fraction: 0.5,
        };
        f.deposit(h);
        let coverage = f.coverage_at(h);
        for _ in 0..20 {
            f.deposit(h);
        }
        assert_eq!(f.deposits.len(), MAX_LAYERS as usize);
        assert_eq!(f.coverage_at(h), coverage);
        let points = f.render_points(&[Mat4::IDENTITY; FRAMES]);
        assert!((points[1][3].y - 0.028).abs() < 1e-6);
    }
    #[test]
    fn playtest_residue_thins_then_clears_its_render_and_suppression_by_two_minutes() {
        let mesh = plane();
        let models = [Mat4::IDENTITY; FRAMES];
        let mut fire = CabinFire::new(&[(0, &mesh)]);
        fire.enable_residue_cleanup();
        let contact = Contact { frame: 0, point: Vec3::ZERO, normal: Vec3::Y, fraction: 0.5 };
        for _ in 0..MAX_LAYERS {
            coat(&mut fire, Vec3::ZERO);
        }
        let original = fire.deposits.len();
        let coverage = fire.coverage_at(contact);
        assert!(coverage > 0.9);
        for _ in 0..1180 { fire.advance_foam(0.05, &models); }
        assert_eq!(fire.deposits.len(), original, "fresh coating lasts at least one minute");
        for _ in 0..620 { fire.advance_foam(0.05, &models); }
        assert!(fire.deposits.len() > original / 4 && fire.deposits.len() < original * 3 / 4);
        let remaining_coverage = fire.coverage_at(contact);
        assert!(remaining_coverage > 0.2 && remaining_coverage < coverage * 0.8);
        assert_eq!(fire.render_points(&models)[1].len(), fire.deposits.len());
        // Every retained column keeps its supporting base layer until the whole column clears.
        assert!(fire.columns.values().all(|column| column.layers == MAX_LAYERS));
        for _ in 0..600 { fire.advance_foam(0.05, &models); }
        assert_eq!(fire.counts(), (0, 0));
        assert!(fire.render_points(&models)[1].is_empty());
        assert_eq!(fire.coverage_at(contact), 0.0);
        assert!(fire.columns.is_empty() && fire.coating_bins.is_empty());
        fire.deposit(contact);
        assert_eq!(fire.deposits.len(), 1, "expired columns accept a new spray");
        assert_eq!(fire.deposits[0].layer, 0);
        assert!(fire.coverage_at(contact) > 0.0);
    }
    #[test]
    fn playtest_residue_clock_freezes_at_pause_and_campaign_coating_stays_unchanged() {
        let mesh = plane();
        let models = [Mat4::IDENTITY; FRAMES];
        let mut playtest = CabinFire::new(&[(0, &mesh)]);
        playtest.enable_residue_cleanup();
        let mut campaign = CabinFire::new(&[(0, &mesh)]);
        coat(&mut playtest, Vec3::ZERO);
        coat(&mut campaign, Vec3::ZERO);
        let original = playtest.deposits.len();
        for _ in 0..1180 { playtest.advance_foam(0.05, &models); }
        let clock = playtest.foam_clock;
        for _ in 0..2400 { playtest.advance_foam(0.0, &models); }
        assert_eq!(playtest.foam_clock, clock);
        assert_eq!(playtest.deposits.len(), original);
        for _ in 0..2400 { campaign.advance_foam(0.05, &models); }
        assert_eq!(campaign.deposits.len(), original);
        playtest.reset();
        assert!(playtest.residue_cleanup, "reset preserves the scene's cleanup policy");
        assert_eq!(playtest.foam_clock, 0.0);
    }
    #[test]
    fn playtest_residue_expiry_does_not_reignite_extinguished_fire_or_block_new_ignition() {
        let mesh = plane();
        let models = [Mat4::IDENTITY; FRAMES];
        let mut fire = CabinFire::new(&[(0, &mesh)]);
        fire.enable_residue_cleanup();
        patch(&mut fire, &models);
        coat(&mut fire, Vec3::ZERO);
        let mut state = FireUpdate { strength: 0.5, active: true, ammo_heat: 0.0, hottest_ammunition: None };
        for _ in 0..180 {
            state = fire.update_fire(1.0 / 60.0, state.active, state.strength, &models, &[], false);
        }
        assert!(!state.active && state.strength == 0.0);
        let from = Vec3::new(0.0, 0.1, 0.0);
        let to = Vec3::new(0.0, -0.1, 0.0);
        assert!(!fire.seed_from_segment(from, to, &models, 0.6));
        for _ in 0..2400 { fire.advance_foam(0.05, &models); }
        state = fire.update_fire(0.05, state.active, state.strength, &models, &[], false);
        assert!(!state.active && state.strength == 0.0 && state.ammo_heat == 0.0);
        assert!(fire.patches.iter().all(|patch| patch.intensity == 0.0));
        assert!(fire.seed_from_segment(from, to, &models, 0.6));
        let reignited = fire.update_fire(0.05, true, 0.6, &models, &[], false);
        assert!(reignited.active && reignited.strength > 0.6);
    }
    #[test]
    fn wrong_aim_does_not_suppress_but_surface_coverage_extinguishes() {
        let mesh = plane();
        let m = [Mat4::IDENTITY; FRAMES];
        let mut wrong = CabinFire::new(&[(0, &mesh)]);
        let mut right = CabinFire::new(&[(0, &mesh)]);
        patch(&mut wrong, &m);
        patch(&mut right, &m);
        coat(&mut wrong, Vec3::X);
        coat(&mut right, Vec3::ZERO);
        let mut a = FireUpdate {
            strength: 0.5,
            active: true,
            ammo_heat: 0.0,
            hottest_ammunition: None,
        };
        let mut b = a;
        for _ in 0..180 {
            a = wrong.update_fire(
                1.0 / 60.0,
                a.active,
                a.strength,
                &m,
                &[Vec3::new(0.1, 0.1, 0.0)],
                false,
            );
            b = right.update_fire(
                1.0 / 60.0,
                b.active,
                b.strength,
                &m,
                &[Vec3::new(0.1, 0.1, 0.0)],
                false,
            );
        }
        assert!(a.strength > 0.7 && a.ammo_heat > 0.55);
        assert!(!b.active && b.strength == 0.0 && b.ammo_heat == 0.0);
    }
    #[test]
    fn extinguished_fire_clears_exposure_without_erasing_surface_coating() {
        let mesh = plane();
        let models = [Mat4::IDENTITY; FRAMES];
        let ammunition = [Vec3::new(0.1, 0.1, 0.0)];
        let mut fire = CabinFire::new(&[(0, &mesh)]);
        patch(&mut fire, &models);
        let mut strength = 0.6;
        for _ in 0..60 {
            strength = fire
                .update_fire(1.0 / 60.0, true, strength, &models, &ammunition, false)
                .strength;
        }
        assert!(fire.diagnostics().2 > 0.2);
        let anchors: Vec<Contact> = fire
            .patches
            .iter()
            .filter_map(|patch| patch.anchor)
            .collect();
        for anchor in anchors {
            coat(&mut fire, anchor.point);
        }
        let mut active = true;
        for _ in 0..180 {
            let state = fire.update_fire(1.0 / 60.0, active, strength, &models, &ammunition, false);
            strength = state.strength;
            active = state.active;
            if !active {
                break;
            }
        }
        assert!(!active && strength == 0.0);
        assert!(fire.diagnostics().2.is_finite() && fire.diagnostics().2 >= 0.0);
        // Visibility refresh can already clear the extinguished winner on the last active tick.
        // The subsequent inactive call must remain safe and preserve the complete surface coat.
        let before = fire.diagnostics();
        let state = fire.update_fire(1.0 / 60.0, active, strength, &models, &ammunition, false);
        assert!(!state.active && state.strength == 0.0 && state.ammo_heat == 0.0);
        assert_eq!(fire.diagnostics(), (before.0, before.1, 0.0));
        assert!(
            before.0 > 0 && before.1 > 0.9,
            "the extinguishing coat must persist"
        );
        assert!(fire
            .patches
            .iter()
            .all(|patch| patch.intensity == 0.0 && patch.heat == 0.0));
    }

    #[test]
    fn live_ammunition_is_required_and_emptying_cancels_thermal_risk() {
        let mesh = plane();
        let m = [Mat4::IDENTITY; FRAMES];
        let mut f = CabinFire::new(&[(0, &mesh)]);
        patch(&mut f, &m);
        let mut a = FireUpdate {
            strength: 1.0,
            active: true,
            ammo_heat: 0.0,
            hottest_ammunition: None,
        };
        for _ in 0..120 {
            a = f.update_fire(
                1.0 / 60.0,
                true,
                a.strength,
                &m,
                &[Vec3::new(0.1, 0.1, 0.0)],
                false,
            );
        }
        assert!(a.ammo_heat > 0.55);
        let empty = f.update_fire(1.0 / 60.0, true, a.strength, &m, &[], false);
        assert_eq!(empty.ammo_heat, 0.0);
        assert!(empty.active);
    }
    fn block(min: Vec3, max: Vec3) -> CpuMesh {
        crate::prims::box_mesh(min, max, [128, 128, 128])
    }
    fn advance(
        fire: &mut CabinFire,
        seconds: f32,
        models: &[Mat4; FRAMES],
        ammunition: &[Vec3],
    ) -> FireUpdate {
        let mut state = FireUpdate {
            strength: 0.6,
            active: true,
            ammo_heat: 0.0,
            hottest_ammunition: None,
        };
        for _ in 0..(seconds * 60.0).round() as usize {
            state = fire.update_fire(
                1.0 / 60.0,
                state.active,
                state.strength,
                models,
                ammunition,
                false,
            );
        }
        state
    }
    #[test]
    fn connected_floor_reaches_vertical_wall_in_seconds_without_crossing_armor() {
        let floor = block(Vec3::new(-0.6, -0.05, -0.6), Vec3::new(0.6, 0.0, 0.6));
        let wall = block(Vec3::new(0.2, 0.0, -2.0), Vec3::new(0.25, 2.0, 2.0));
        let models = [Mat4::IDENTITY; FRAMES];
        let mut fire = CabinFire::new(&[(0, &floor), (0, &wall)]);
        assert!(fire.seed_from_segment(
            Vec3::new(0.05, 0.2, 0.0),
            Vec3::new(0.05, -0.2, 0.0),
            &models,
            0.6
        ));
        advance(&mut fire, 2.0, &models, &[]);
        let (active, vertical) = fire.spread_diagnostics(&models);
        assert!(active > 1 && active <= MAX_PATCHES);
        assert!(
            vertical > 0,
            "connected wall must ignite within two seconds"
        );
        let surfaces: Vec<_> = (0..MAX_PATCHES)
            .filter_map(|i| fire.patch_surface(i, &models))
            .collect();
        assert!(surfaces
            .iter()
            .any(|s| s.normal.x < -0.9 && s.position.y > 0.02));
        assert!(
            surfaces.iter().all(|s| s.position.x < 0.201),
            "fire cannot heat through solid armor"
        );
    }
    #[test]
    fn tiny_gap_and_detached_wall_are_not_surface_connections() {
        let left = block(Vec3::new(-0.4, -0.05, -0.4), Vec3::new(0.0, 0.0, 0.4));
        let right = block(Vec3::new(0.006, -0.05, -0.4), Vec3::new(0.4, 0.0, 0.4));
        let suspended = block(Vec3::new(-0.15, 0.012, -0.4), Vec3::new(-0.12, 0.8, 0.4));
        let models = [Mat4::IDENTITY; FRAMES];
        let mut fire = CabinFire::new(&[(0, &left), (0, &right), (1, &suspended)]);
        assert!(!fire.supports_segment(
            Vec3::new(-0.01, 0.0, 0.0),
            Vec3::new(0.015, 0.0, 0.0),
            &models
        ));
        assert!(fire.seed_from_segment(
            Vec3::new(-0.06, 0.1, 0.0),
            Vec3::new(-0.06, -0.1, 0.0),
            &models,
            0.6
        ));
        advance(&mut fire, 2.0, &models, &[]);
        assert!(fire
            .patches
            .iter()
            .filter_map(|p| p.anchor)
            .all(|p| p.point.x <= 0.0001 && p.frame == 0));
    }
    #[test]
    fn foam_firebreak_uses_the_contacted_face_and_cannot_be_jumped() {
        let floor = block(Vec3::new(-0.5, -0.05, -0.35), Vec3::new(0.5, 0.0, 0.35));
        let models = [Mat4::IDENTITY; FRAMES];
        let make = |normal: Vec3| {
            let mut f = CabinFire::new(&[(0, &floor)]);
            assert!(f.seed_from_segment(
                Vec3::new(-0.16, 0.1, 0.0),
                Vec3::new(-0.16, -0.1, 0.0),
                &models,
                0.6
            ));
            for x in 0..=20 {
                for z in -50..=50 {
                    f.deposit(Contact {
                        frame: 0,
                        point: Vec3::new(x as f32 * GRAIN_M, 0.0, z as f32 * GRAIN_M),
                        normal,
                        fraction: 0.0,
                    });
                }
            }
            f
        };
        let mut coated = make(Vec3::Y);
        let mut wrong_face = make(Vec3::NEG_Y);
        advance(&mut coated, 3.0, &models, &[]);
        advance(&mut wrong_face, 3.0, &models, &[]);
        assert!(coated
            .patches
            .iter()
            .filter_map(|p| p.anchor)
            .all(|a| a.point.x < 0.16));
        assert!(
            wrong_face
                .patches
                .iter()
                .filter_map(|p| p.anchor)
                .any(|a| a.point.x > 0.16),
            "opposite face foam must not suppress this face"
        );
        assert!(
            coated.spread_diagnostics(&models).0 > 0,
            "the uncoated side still burns"
        );
    }
    #[test]
    fn vertical_ignition_and_render_basis_follow_the_source_frame() {
        let wall = block(Vec3::new(0.0, -0.5, -0.5), Vec3::new(0.05, 0.5, 0.5));
        let mut models = [Mat4::IDENTITY; FRAMES];
        let mut fire = CabinFire::new(&[(1, &wall)]);
        assert!(fire.seed_from_segment(
            Vec3::new(-0.2, 0.0, 0.0),
            Vec3::new(0.2, 0.0, 0.0),
            &models,
            0.6
        ));
        let before = fire.patch_surface(0, &models).unwrap();
        assert!(before.normal.distance(Vec3::NEG_X) < 1e-5);
        models[1] = Mat4::from_rotation_translation(
            glam::Quat::from_rotation_z(0.7),
            Vec3::new(3.0, 2.0, 1.0),
        );
        let after = fire.patch_surface(0, &models).unwrap();
        assert!(
            after
                .position
                .distance(models[1].transform_point3(before.position))
                < 1e-5
        );
        assert!(
            after
                .normal
                .distance(models[1].transform_vector3(before.normal))
                < 1e-5
        );
        assert!(
            after.normal.dot(after.tangent).abs() < 1e-5
                && after.normal.dot(after.bitangent).abs() < 1e-5
        );
        let contact = fire.patches[0].anchor.unwrap();
        for y in -15..=15 {
            for z in -15..=15 {
                fire.deposit(Contact {
                    point: contact.point + Vec3::new(0.0, y as f32 * GRAIN_M, z as f32 * GRAIN_M),
                    ..contact
                });
            }
        }
        assert!(
            !fire.seed_from_segment(
                models[1].transform_point3(Vec3::new(-0.2, 0.0, 0.0)),
                models[1].transform_point3(Vec3::new(0.2, 0.0, 0.0)),
                &models,
                1.0
            ),
            "coated wall blocks new ignition"
        );
    }
    #[test]
    fn ammunition_exposure_requires_visibility_and_restarts_for_a_new_round() {
        let floor = plane();
        let wall = block(Vec3::new(0.2, 0.0, -2.0), Vec3::new(0.25, 2.0, 2.0));
        let models = [Mat4::IDENTITY; FRAMES];
        let make = || {
            let mut f = CabinFire::new(&[(0, &floor), (0, &wall)]);
            patch(&mut f, &models);
            f
        };
        let mut exposed = make();
        let mut occluded = make();
        let mut empty = make();
        let front = Vec3::new(0.06, 0.08, 0.0);
        let behind = Vec3::new(0.4, 0.08, 0.0);
        let warm = advance(
            &mut exposed,
            2.0,
            &models,
            &[front, Vec3::new(-1.5, 0.1, 0.0)],
        );
        let shielded = advance(&mut occluded, 2.0, &models, &[behind]);
        let no_ammo = advance(&mut empty, 2.0, &models, &[]);
        assert!(warm.ammo_heat > 0.65 && warm.hottest_ammunition == Some(0));
        assert_eq!(shielded.ammo_heat, 0.0);
        assert_eq!(shielded.hottest_ammunition, None);
        assert_eq!(no_ammo.ammo_heat, 0.0);
        assert!(no_ammo.active);
        exposed.ammo_scan_clock = 0.0;
        let switched = exposed.update_fire(
            1.0 / 60.0,
            true,
            warm.strength,
            &models,
            &[behind, Vec3::new(-1.5, 0.1, 0.0)],
            false,
        );
        assert_eq!(switched.hottest_ammunition, Some(1));
        assert!(
            switched.ammo_heat < 0.05,
            "previous cartridge's accumulated exposure cannot ignite another"
        );
    }
    #[test]
    fn circular_foam_cone_is_dense_bounded_and_tracks_the_actual_aim() {
        for axis in [Vec3::X, Vec3::Y, Vec3::NEG_Z] {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            let tangent = (if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X })
                .cross(axis)
                .normalize();
            let bitangent = axis.cross(tangent);
            let mut cells = std::collections::HashSet::new();
            let mut centre = glam::Vec2::ZERO;
            for sample in 0..8192 {
                let velocity = spray_velocity(axis, sample);
                let speed = velocity.length();
                assert!((4.099..=5.101).contains(&speed));
                assert!(velocity.normalize().dot(axis) >= SPRAY_HALF_ANGLE_RAD.cos() - 1e-5);
                let projected = velocity / velocity.dot(axis);
                let p = glam::Vec2::new(projected.dot(tangent), projected.dot(bitangent));
                min = min.min(Vec3::new(p.x, p.y, 0.0));
                max = max.max(Vec3::new(p.x, p.y, 0.0));
                centre += p;
                cells.insert(((p.x / 0.02).floor() as i32, (p.y / 0.02).floor() as i32));
                assert_eq!(velocity, spray_velocity(axis, sample));
            }
            assert!(max.x - min.x > 0.38 && max.y - min.y > 0.38);
            assert!(
                cells.len() > 300,
                "the cone must fill an area, not a correlated line"
            );
            assert!((centre / 8192.0).length() < 0.004);
        }
        assert_eq!(spray_velocity(Vec3::ZERO, 1), Vec3::ZERO);
    }
    #[test]
    fn wider_foam_covers_a_real_wall_but_spraying_away_cannot_coat_it() {
        let wall = block(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.04));
        let models = [Mat4::IDENTITY; FRAMES];
        let mut aimed = CabinFire::new(&[(0, &wall)]);
        let mut away = CabinFire::new(&[(0, &wall)]);
        let nozzle = Vec3::new(0.0, 0.8, -1.2);
        for sample in 0..3000 {
            aimed.emit(nozzle, spray_velocity(Vec3::Z, sample));
            away.emit(nozzle, spray_velocity(Vec3::X, sample));
        }
        for _ in 0..300 {
            aimed.advance_foam(1.0 / 120.0, &models);
            away.advance_foam(1.0 / 120.0, &models);
        }
        assert!(aimed.deposits.len() > 1500);
        assert!(
            away.deposits.is_empty(),
            "wider cone must not become target-seeking foam"
        );
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for deposit in &aimed.deposits {
            min = min.min(deposit.point);
            max = max.max(deposit.point);
        }
        assert!(max.x - min.x > 0.30 && max.y - min.y > 0.30);
        assert!(min.z.abs() < 1e-5 && max.z.abs() < 1e-5);
        let retained = aimed.deposits.len();
        for _ in 0..120 {
            aimed.advance_foam(1.0 / 60.0, &models);
        }
        assert_eq!(aimed.deposits.len(), retained);
    }
}
