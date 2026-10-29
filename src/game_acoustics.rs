//! The game's acoustic world: the scene the ray tracer sees (the Tiger's own source triangles,
//! the terrain, buildings and wrecks), a cache of traced reflected fields, foley played from world
//! positions through the ordinary propagation and binaural path, and the continuous sounds of the
//! weather, the machinery and the ground under the tracks.
//!
//! Tracing spends a fixed number of rays per simulation update (`rays_per_update`, by graphics
//! preset), so a capture is deterministic and no frame stalls on a whole trace. Fields are traced
//! per listener cell (0.3 m in the hull's frame aboard, 4 m in the world outside), per hatch state
//! and turret bucket, and per source bucket: the listener's own body, a cell of the cabin, the
//! plate facing an outside source (four quadrants), the open cupola, or an outside source by
//! direction (eight sectors) and range (four classes). Until a field is traced, the nearest traced
//! field of the same kind stands in, else the parametric tail (`Room::Parametric`).

use super::*;
use crate::acoustics::foley::{CabinCue, Cue, EnvCue, ExtCue, Gait, Ground as FootGround, VehicleCue};
use crate::acoustics::raytrace::{self as rt, AcousticScene, Hit, Material, StereoIr, TraceJob, TraceSettings, VoxelSolid};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// Articulated cabin frames that are machinery rather than armour (`fire_surface_models`: visual
/// gun, turret disc, gun, breech cover, gun lever).
const MACHINERY_FRAMES: [usize; 5] = [2, 7, 8, 12, 13];
const CACHE_CAPACITY: usize = 160;
/// Room constant of the fighting compartment, m² [E]: from the traced cabin's T20 of 0.16–0.19 s
/// (`tests::the_real_cabin_rings_and_an_open_cupola_lets_it_out`) with an estimated 10 m³ of air
/// and 35 m² of surface: A = 0.161 V / T ≈ 9 m², R = A / (1 − A/S) ≈ 12 m².
pub(super) const CABIN_ROOM_CONSTANT_M2: f32 = 12.0;

/// Direct share of a firewall-radiated sound at `r` m in the cabin (Q = 2).
pub(super) fn engine_direct_share(r: f32) -> f32 {
    let direct = 2.0 / (4.0 * std::f32::consts::PI * r.max(0.5).powi(2));
    direct / (direct + 4.0 / CABIN_ROOM_CONSTANT_M2)
}
const INSIDE_CELL_M: f32 = 0.3;
const OUTSIDE_CELL_M: f32 = 4.0;

/// Acoustic voxel cell, m: 5 cm, the wavelength at 6.9 kHz.
pub(super) const CABIN_VOXEL_M: f32 = 0.05;

/// The Tiger's articulated source frames, each voxelized once in its own frame
/// (`VoxelSolid::from_triangles` of the triangles `CabinFire` holds for that frame).
pub(super) struct CabinVoxels {
    pub frames: Vec<(usize, VoxelSolid)>,
    pub triangles: usize,
    pub build_ms: f64,
}

impl CabinVoxels {
    pub fn build(cabin: &surface_fire::CabinFire) -> Self {
        let start = std::time::Instant::now();
        let far = Mat4::from_translation(Vec3::splat(1.0e6));
        let mut frames = Vec::new();
        let mut triangles = 0;
        for f in 0..surface_fire::FRAMES {
            let mut models = [far; surface_fire::FRAMES];
            models[f] = Mat4::IDENTITY;
            let contacts = cabin.source_contacts(&models);
            let tris = contacts.world_triangles(Vec3::splat(-8.0), Vec3::splat(8.0));
            triangles += tris.len();
            if let Some(solid) = VoxelSolid::from_triangles(&tris, CABIN_VOXEL_M) {
                frames.push((f, solid));
            }
        }
        Self { frames, triangles, build_ms: start.elapsed().as_secs_f64() * 1e3 }
    }

    /// The same voxels from the meshes `CabinFire::new` is given, built on a worker thread so
    /// the ~0.6 s never lands in a simulation update. Only vertex positions and indices are
    /// copied on the caller's thread. The worker keeps exactly the triangles `build` gets back
    /// from `world_triangles` with identity frames: the non-degenerate ones (as `CabinFire`
    /// keeps them) whose bounds meet the ±8 m box. The occupancy grid does not depend on their
    /// order. `None` when the traced rooms are off (`KESSEL_ACOUSTICS=0`) or no thread starts.
    pub fn spawn(meshes: &[(usize, &crate::gltf_load::CpuMesh)]) -> Option<std::thread::JoinHandle<CabinVoxels>> {
        if !acoustics_enabled() { return None; }
        let copies: Vec<(usize, Vec<[f32; 3]>, Vec<u32>)> = meshes.iter()
            .map(|(frame, mesh)| (*frame, mesh.vertices.iter().map(|v| v.pos).collect(), mesh.indices.clone()))
            .collect();
        std::thread::Builder::new().name("acoustic-voxels".into())
            .spawn(move || Self::from_copies(&copies)).ok()
    }

    fn from_copies(copies: &[(usize, Vec<[f32; 3]>, Vec<u32>)]) -> Self {
        let start = std::time::Instant::now();
        let reach = Vec3::splat(8.0);
        let mut per_frame: Vec<Vec<[Vec3; 3]>> = vec![Vec::new(); surface_fire::FRAMES];
        for (frame, positions, indices) in copies {
            for ids in indices.chunks_exact(3) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[ids[i] as usize]));
                if (b - a).cross(c - a).length_squared() < 1e-16 { continue; }
                let (lo, hi) = (a.min(b).min(c), a.max(b).max(c));
                if lo.cmpgt(reach).any() || hi.cmplt(-reach).any() { continue; }
                per_frame[*frame].push([a, b, c]);
            }
        }
        let mut frames = Vec::new();
        let mut triangles = 0;
        for (f, tris) in per_frame.iter().enumerate() {
            triangles += tris.len();
            if let Some(solid) = VoxelSolid::from_triangles(tris, CABIN_VOXEL_M) {
                frames.push((f, solid));
            }
        }
        Self { frames, triangles, build_ms: start.elapsed().as_secs_f64() * 1e3 }
    }

    /// Nearest voxel hit over every frame placed by `models` (scene frame).
    pub fn hit(&self, models: &[Mat4; surface_fire::FRAMES], inverses: &[Mat4; surface_fire::FRAMES],
        origin: Vec3, dir: Vec3, max_distance: f32) -> Option<Hit>
    {
        let mut best: Option<Hit> = None;
        for (f, solid) in &self.frames {
            let (o, d) = (inverses[*f].transform_point3(origin), inverses[*f].transform_vector3(dir));
            let limit = best.map_or(max_distance, |b| b.distance);
            if let Some((t, n)) = solid.hit(o, d, limit) {
                if best.is_none_or(|b| t < b.distance) {
                    let material = if MACHINERY_FRAMES.contains(f) { Material::Machinery } else { Material::Armor };
                    best = Some(Hit { distance: t, normal: models[*f].transform_vector3(n).normalize_or_zero(), material });
                }
            }
        }
        best
    }
}

/// The scene's masonry boxes in vertical columns of a uniform x/z grid, so a ray tests only the
/// boxes in the columns it crosses, nearest column first, and stops once a hit lies before the
/// next column. It returns exactly the linear scan's first entry (ties go to the earlier box).
/// Canyon v2 has ~10,000 boxes within 150 m; scanning all of them for each of a cabin ray's up to
/// 80 queries was 98 % of the trace time and made 1.5 s simulation updates in contact (2026-10-28).
pub(super) struct BoxGrid {
    boxes: Vec<(Vec3, Vec3)>,
    origin: Vec2,
    dims: [i32; 2],
    /// Column `c` holds `items[start[c]..start[c + 1]]`.
    start: Vec<u32>,
    items: Vec<u32>,
}

impl BoxGrid {
    const CELL_M: f32 = 8.0;

    pub fn new(boxes: Vec<(Vec3, Vec3)>) -> Self {
        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for (min, max) in &boxes {
            lo = lo.min(Vec2::new(min.x, min.z));
            hi = hi.max(Vec2::new(max.x, max.z));
        }
        if boxes.is_empty() {
            lo = Vec2::ZERO;
            hi = Vec2::ZERO;
        }
        let dims = [((hi.x - lo.x) / Self::CELL_M).floor() as i32 + 1, ((hi.y - lo.y) / Self::CELL_M).floor() as i32 + 1];
        let mut grid = Self { boxes, origin: lo, dims, start: Vec::new(), items: Vec::new() };
        let cells = (dims[0] * dims[1]) as usize;
        let mut start = vec![0u32; cells + 1];
        for (min, max) in &grid.boxes {
            let (a, b) = grid.span(*min, *max);
            for z in a[1]..=b[1] {
                for x in a[0]..=b[0] { start[(z * dims[0] + x) as usize + 1] += 1; }
            }
        }
        for c in 0..cells { start[c + 1] += start[c]; }
        let mut fill = start.clone();
        let mut items = vec![0u32; start[cells] as usize];
        for (i, (min, max)) in grid.boxes.iter().enumerate() {
            let (a, b) = grid.span(*min, *max);
            for z in a[1]..=b[1] {
                for x in a[0]..=b[0] {
                    let c = (z * dims[0] + x) as usize;
                    items[fill[c] as usize] = i as u32;
                    fill[c] += 1;
                }
            }
        }
        grid.start = start;
        grid.items = items;
        grid
    }

    fn cell_of(&self, x: f32, z: f32) -> [i32; 2] {
        [(((x - self.origin.x) / Self::CELL_M).floor() as i32).clamp(0, self.dims[0] - 1),
         (((z - self.origin.y) / Self::CELL_M).floor() as i32).clamp(0, self.dims[1] - 1)]
    }

    fn span(&self, min: Vec3, max: Vec3) -> ([i32; 2], [i32; 2]) {
        (self.cell_of(min.x, min.z), self.cell_of(max.x, max.z))
    }

    /// The nearest box entry along `o + d t` with `t < limit` (`rt::ray_box_entry`), as the
    /// linear scan over every box finds it.
    pub fn first_entry(&self, o: Vec3, d: Vec3, limit: f32) -> Option<(f32, Vec3)> {
        if self.boxes.is_empty() { return None; }
        // The ray's span over the grid's x/z rectangle (the columns are unbounded in y).
        let size = Vec2::new(self.dims[0] as f32, self.dims[1] as f32) * Self::CELL_M;
        let (oxz, dxz) = (Vec2::new(o.x, o.z), Vec2::new(d.x, d.z));
        let (mut t0, mut t1) = (0.0f32, limit);
        for axis in 0..2 {
            let (lo, hi) = (self.origin[axis], self.origin[axis] + size[axis]);
            if dxz[axis].abs() < 1.0e-12 {
                if oxz[axis] < lo || oxz[axis] > hi { return None; }
            } else {
                let (mut a, mut b) = ((lo - oxz[axis]) / dxz[axis], (hi - oxz[axis]) / dxz[axis]);
                if a > b { std::mem::swap(&mut a, &mut b); }
                t0 = t0.max(a);
                t1 = t1.min(b);
            }
        }
        if t0 > t1 { return None; }
        let p = o + d * t0;
        let mut c = self.cell_of(p.x, p.z);
        let step = [if d.x > 0.0 { 1 } else { -1 }, if d.z > 0.0 { 1 } else { -1 }];
        let mut t_max = [f32::INFINITY; 2];
        let mut t_delta = [f32::INFINITY; 2];
        for axis in 0..2 {
            if dxz[axis].abs() > 1.0e-12 {
                let edge = self.origin[axis] + (c[axis] + i32::from(step[axis] > 0)) as f32 * Self::CELL_M;
                t_max[axis] = (edge - oxz[axis]) / dxz[axis];
                t_delta[axis] = Self::CELL_M / dxz[axis].abs();
            }
        }
        let mut best: Option<(f32, Vec3, u32)> = None;
        loop {
            let cell = (c[1] * self.dims[0] + c[0]) as usize;
            for &i in &self.items[self.start[cell] as usize..self.start[cell + 1] as usize] {
                let (min, max) = self.boxes[i as usize];
                if let Some((t, n)) = rt::ray_box_entry(o, d, min, max) {
                    let better = match best {
                        None => t < limit,
                        Some((bt, _, bi)) => t < bt || (t == bt && i < bi),
                    };
                    if better { best = Some((t, n, i)); }
                }
            }
            // Leave this column; a hit before its far edge cannot be beaten further on.
            let exit = t_max[0].min(t_max[1]);
            if exit > t1 || best.is_some_and(|(bt, ..)| bt + 1.0e-4 < exit) { break; }
            let axis = if t_max[0] < t_max[1] { 0 } else { 1 };
            c[axis] += step[axis];
            if c[axis] < 0 || c[axis] >= self.dims[axis] { break; }
            t_max[axis] += t_delta[axis];
        }
        best.map(|(t, n, _)| (t, n))
    }
}

/// The scene in one frame of reference: the hull's (aboard) or the world's (outside).
pub(super) struct GameScene<'a> {
    voxels: Option<&'a CabinVoxels>,
    models: [Mat4; surface_fire::FRAMES],
    inverses: [Mat4; surface_fire::FRAMES],
    hf: &'a Heightfield,
    boxes: BoxGrid,
    ground: Material,
    /// Scene frame → world.
    to_world: Mat4,
    to_local: Mat4,
}

impl AcousticScene for GameScene<'_> {
    fn first_hit(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        let reach = max_distance.min(400.0);
        if let Some(voxels) = self.voxels {
            best = voxels.hit(&self.models, &self.inverses, origin, dir, reach.min(30.0));
        }
        let limit = best.map_or(reach, |b| b.distance);
        let (o, d) = (self.to_world.transform_point3(origin), self.to_world.transform_vector3(dir));
        let local_normal = |n: Vec3| self.to_local.transform_vector3(n).normalize_or_zero();
        if let Some((t, n)) = self.boxes.first_entry(o, d, limit) {
            best = Some(Hit { distance: t, normal: local_normal(n), material: Material::Masonry });
        }
        // Terrain: march, then bisect the crossing; the normal from the height gradient.
        let limit = best.map_or(reach, |b| b.distance);
        let above = |t: f32| { let p = o + d * t; p.y - self.hf.height_at(p.x, p.z) };
        if above(0.0) > 0.0 {
            let mut t = 0.0f32;
            while t < limit {
                let step = if t < 10.0 { 0.5 } else { 2.0 };
                let next = (t + step).min(limit);
                if above(next) <= 0.0 {
                    let (mut lo, mut hi) = (t, next);
                    for _ in 0..8 { let mid = 0.5 * (lo + hi); if above(mid) > 0.0 { lo = mid } else { hi = mid } }
                    let p = o + d * hi;
                    let e = 0.5;
                    let n = Vec3::new(self.hf.height_at(p.x - e, p.z) - self.hf.height_at(p.x + e, p.z), 2.0 * e,
                        self.hf.height_at(p.x, p.z - e) - self.hf.height_at(p.x, p.z + e)).normalize_or_zero();
                    best = Some(Hit { distance: hi, normal: local_normal(n), material: self.ground });
                    break;
                }
                if next >= limit { break; }
                t = next;
            }
        }
        best
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SourceBucket {
    /// The listener's own hands and feet.
    Local,
    /// A cell of the cabin (hull frame, 0.5 m).
    Interior([i16; 3]),
    /// An outside source heard buttoned: the plate facing it, by hull quadrant.
    Plate(u8),
    /// An outside source heard through the open cupola.
    Hatch,
    /// Outside: direction sector (8) and range class (4).
    Outdoor(u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct FieldKey {
    inside: bool,
    cell: [i32; 3],
    yaw: i16,
    hatches: u16,
    source: SourceBucket,
}

/// Continuous-sound and event memory for the world foley.
#[derive(Default)]
pub(super) struct WorldAcoustics {
    /// The cabin's voxels, once the load-time worker has delivered them (`CabinVoxels::spawn`).
    voxels: Option<Rc<CabinVoxels>>,
    /// The worker building them; traces wait (voices keep the parametric tail) until it is done.
    pending_voxels: Option<std::thread::JoinHandle<CabinVoxels>>,
    cache: HashMap<FieldKey, Rc<StereoIr>>,
    order: VecDeque<FieldKey>,
    queue: VecDeque<(FieldKey, Vec3, Vec3, Vec3)>,
    job: Option<(FieldKey, TraceJob)>,
    pub traces_done: u32,
    pub rays_traced: u64,
    pub traced_rooms: u32,
    pub stand_in_rooms: u32,
    pub parametric_rooms: u32,
    pub foley_events: u32,
    pub recorded_events: u32,
    loops: Vec<(Cue, crate::audio::VehicleVoice, bool)>,
    active_loops: Vec<Cue>,
    pub seq: u32,
    last_gear: i8,
    last_speed: f32,
    last_airborne: bool,
    last_fall_speed: f32,
    last_station_switch: bool,
    last_engine_on: bool,
    last_hatch_open: bool,
    last_head_out: bool,
    last_ext: bool,
    last_snow_load: f32,
    engine_off_s: f32,
    timers: [f32; 4],
    hard_contact: bool,
    primed: bool,
    convolutions: u32,
    pour_s: f32,
    last_binocular: f32,
    pub(super) starved: bool,
    pub cue_counts: std::collections::BTreeMap<String, u32>,
}

fn quantize(v: Vec3, cell: f32) -> [i32; 3] {
    [(v.x / cell).floor() as i32, (v.y / cell).floor() as i32, (v.z / cell).floor() as i32]
}

fn sector(v: Vec3, sectors: u8) -> u8 {
    let a = v.x.atan2(v.z).rem_euclid(std::f32::consts::TAU);
    ((a / std::f32::consts::TAU * sectors as f32) as u8).min(sectors - 1)
}

fn bucket_dir(index: u8, sectors: u8) -> Vec3 {
    let a = (index as f32 + 0.5) / sectors as f32 * std::f32::consts::TAU;
    Vec3::new(a.sin(), 0.0, a.cos())
}

impl Game {
    /// Rays per simulation update and per trace, by preset: the presets may change ray counts,
    /// never the scene or the transport law (CLAUDE.md).
    fn acoustic_ray_budget(&self) -> (u32, u32, u32) {
        match crate::graphics::active_preset() {
            crate::graphics::GraphicsPreset::High | crate::graphics::GraphicsPreset::Ultra => (48, 384, 768),
            _ => (16, 128, 256),
        }
    }

    fn listener_aboard_inside(&self) -> bool {
        matches!(self.mode, Mode::Tank)
            && matches!(self.listener_enclosure(), crate::sound_propagation::Enclosure::Buttoned
                | crate::sound_propagation::Enclosure::HatchOpen)
    }

    /// Whether a world point lies in the Tiger's cabin air: steel on all six axis rays within
    /// 3.5 m (a sampled test against the real source triangles, not a solid Boolean).
    pub(super) fn point_in_cabin(&self, p: Vec3) -> bool {
        let local = self.player.hull_model().inverse().transform_point3(p);
        if local.x.abs() > 1.9 || !(0.3..3.2).contains(&local.y) || local.z.abs() > 3.4 {
            return false;
        }
        let models = self.fire_surface_models();
        let axes = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
        if let Some(voxels) = &self.acoustics.voxels {
            let inverses = models.map(|m| m.inverse());
            return axes.iter().all(|d| voxels.hit(&models, &inverses, p, *d, 3.5).is_some());
        }
        let contacts = self.cabin_fire.source_contacts(&models);
        let paths: Vec<[Vec3; 2]> = axes.iter().map(|d| [p, p + *d * 3.5]).collect();
        contacts.sweep_batch(&paths).iter().all(Option::is_some)
    }

    fn acoustic_scene(&self, inside: bool, listener_world: Vec3) -> (GameScene<'_>, Vec<Mat4>) {
        let hull = self.player.hull_model();
        let world_models = self.fire_surface_models();
        let to_world = if inside { hull } else { Mat4::IDENTITY };
        let to_local = to_world.inverse();
        let models: [Mat4; surface_fire::FRAMES] = world_models.map(|m| to_local * m);
        let inverses = models.map(|m| m.inverse());
        let boxes = BoxGrid::new(self.occluders.iter()
            .filter(|o| ((o.min + o.max) * 0.5).distance(listener_world) < 150.0)
            .map(|o| (o.min, o.max)).collect());
        let ground = match crate::sound_propagation::active_atmosphere().ground {
            crate::sound_propagation::Ground::Snow => Material::Snow,
            _ => Material::Ground,
        };
        (GameScene { voxels: self.acoustics.voxels.as_deref(), models, inverses, hf: &self.hf, boxes, ground,
            to_world, to_local }, models.to_vec())
    }

    fn hatch_mask(&self) -> u16 {
        u16::from(self.hatch_t > 0.2) | (self.roof_hatches.phase.iter().enumerate()
            .fold(0u16, |m, (i, p)| m | (u16::from(*p > 0.2) << (i + 1))))
    }

    /// The traced reflected field for a source at `source` heard at the current ear, with where
    /// the trace runs from. Queues the trace when it is missing.
    fn acoustic_key(&self, source: Vec3) -> (FieldKey, Vec3, Vec3, Vec3) {
        let ear = self.ear_pos;
        let inside = self.listener_aboard_inside();
        let hull = self.player.hull_model();
        let hull_inv = hull.inverse();
        let yaw = ((self.player.turret_yaw.to_degrees().rem_euclid(360.0)) / 30.0) as i16;
        if inside {
            let listener = hull_inv.transform_point3(ear);
            let forward = hull_inv.transform_vector3(self.ear_forward);
            let src_local = hull_inv.transform_point3(source);
            let (bucket, virtual_source) = if source.distance(ear) < 0.8 {
                (SourceBucket::Local, listener + Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero() * 0.3 - Vec3::Y * 0.4)
            } else if self.point_in_cabin(source) {
                let c = quantize(src_local, 0.5);
                (SourceBucket::Interior([c[0] as i16, c[1] as i16, c[2] as i16]), src_local)
            } else if self.hatch_t > 0.2 {
                let cupola = hull_inv.transform_point3(self.player.turret_model()
                    .transform_point3(tiger_periscopes::COMMANDER_EYE + Vec3::Y * 0.25));
                (SourceBucket::Hatch, cupola)
            } else {
                let q = sector(src_local - listener, 4);
                (SourceBucket::Plate(q), listener + bucket_dir(q, 4) * 0.45)
            };
            (FieldKey { inside, cell: quantize(listener, INSIDE_CELL_M), yaw, hatches: self.hatch_mask(), source: bucket },
                virtual_source, listener, forward)
        } else {
            let offset = source - ear;
            let range = offset.length();
            let class = if range < 3.0 { 0 } else if range < 25.0 { 1 } else if range < 150.0 { 2 } else { 3 };
            let q = sector(offset, 8);
            let at = [1.5f32, 12.0, 60.0, 250.0][class as usize];
            let mut virtual_source = ear + bucket_dir(q, 8) * at;
            virtual_source.y = self.hf.height_at(virtual_source.x, virtual_source.z) + 1.0;
            if class == 0 { virtual_source = source; }
            (FieldKey { inside, cell: quantize(ear, OUTSIDE_CELL_M), yaw: 0, hatches: self.hatch_mask(),
                source: SourceBucket::Outdoor(q, class) }, virtual_source, ear, self.ear_forward)
        }
    }

    /// The room a source is heard in: its traced field, a neighbouring traced field of the same
    /// kind while that is being traced, or `None` for the parametric tail.
    pub(super) fn acoustic_room_for(&mut self, source: Vec3) -> Option<Rc<StereoIr>> {
        if !acoustics_enabled() {
            self.acoustics.parametric_rooms += 1;
            return None;
        }
        // At most this many traced convolutions per update; beyond it a voice takes the
        // parametric tail (a named cost bound: ~6 ms per convolved voice in release).
        let budget = if crate::graphics::active_preset().is_desktop() { 4 } else { 2 };
        if self.acoustics.convolutions >= budget {
            self.acoustics.parametric_rooms += 1;
            return None;
        }
        let (room, traced) = self.acoustic_field_for(source);
        match (&room, traced) {
            (Some(_), true) => self.acoustics.traced_rooms += 1,
            (Some(_), false) => self.acoustics.stand_in_rooms += 1,
            (None, _) => self.acoustics.parametric_rooms += 1,
        }
        if room.is_some() { self.acoustics.convolutions += 1; }
        room
    }

    /// The field for `source` without spending the convolution budget: its own trace (true), a
    /// neighbouring stand-in (false), or none yet; queues the trace when it is missing.
    pub(super) fn acoustic_field_for(&mut self, source: Vec3) -> (Option<Rc<StereoIr>>, bool) {
        let (key, virtual_source, listener, forward) = self.acoustic_key(source);
        if let Some(ir) = self.acoustics.cache.get(&key) {
            return (Some(Rc::clone(ir)), true);
        }
        let queued = self.acoustics.job.as_ref().is_some_and(|(k, _)| *k == key)
            || self.acoustics.queue.iter().any(|(k, ..)| *k == key);
        if !queued && self.acoustics.queue.len() < 24 {
            self.acoustics.queue.push_back((key, virtual_source, listener, forward));
        }
        let cell = 1;
        let stand_in = self.acoustics.cache.iter()
            .filter(|(k, _)| k.inside == key.inside && k.source == key.source && k.hatches == key.hatches
                && (k.cell[0] - key.cell[0]).abs() <= cell + 1 && (k.cell[1] - key.cell[1]).abs() <= cell + 1
                && (k.cell[2] - key.cell[2]).abs() <= cell + 1)
            .min_by_key(|(k, _)| (k.cell[0] - key.cell[0]).abs() + (k.cell[1] - key.cell[1]).abs()
                + (k.cell[2] - key.cell[2]).abs() + 4 * i32::from(k.yaw != key.yaw))
            .map(|(_, ir)| Rc::clone(ir));
        (stand_in, false)
    }

    /// Spend this update's ray budget on the queued traces; keep the listener's own field warm.
    pub(super) fn advance_acoustic_traces(&mut self) {
        let ear = self.ear_pos;
        if ear == Vec3::ZERO || !acoustics_enabled() { return; }
        // The listener's own field (his hands, feet and voice) and, aboard, the four plates.
        let local = ear + self.ear_forward * 0.3 - Vec3::Y * 0.4;
        let _ = self.acoustic_field_for(local);
        let (per_update, cabin_rays, outdoor_rays) = self.acoustic_ray_budget();
        if self.acoustics.job.is_none() {
            if let Some((key, source, listener, forward)) = self.acoustics.queue.pop_front() {
                let atmosphere = crate::sound_propagation::active_atmosphere();
                let air = atmosphere.air_absorption_bands_db_per_m();
                let c = atmosphere.speed_of_sound();
                let seed = (key.cell[0] as u32).wrapping_mul(73_856_093) ^ (key.cell[2] as u32).wrapping_mul(19_349_663);
                let settings = if key.inside {
                    TraceSettings { rays: cabin_rays, max_order: 40, max_time_s: 0.45, receiver_radius_m: 0.25,
                        speed_m_s: c, air_db_per_m: air, seed }
                } else {
                    TraceSettings::outdoor(outdoor_rays, source.distance(listener), c, air, seed)
                };
                self.acoustics.job = Some((key, TraceJob::new(source, listener, forward, &settings)));
            }
        }
        let Some((key, mut job)) = self.acoustics.job.take() else { return };
        if self.acoustics.voxels.is_none() {
            let voxels = match self.acoustics.pending_voxels.take() {
                Some(worker) if !worker.is_finished() => {
                    // Still building off the simulation thread: hold the job, trace nothing yet.
                    self.acoustics.pending_voxels = Some(worker);
                    self.acoustics.job = Some((key, job));
                    return;
                }
                Some(worker) => worker.join().unwrap_or_else(|_| {
                    eprintln!("[acoustics] the voxel worker failed; building the cabin voxels here");
                    CabinVoxels::build(&self.cabin_fire)
                }),
                // No worker (a world made without one): build here, once.
                None => CabinVoxels::build(&self.cabin_fire),
            };
            println!("[acoustics] cabin voxels: {} frames from {} source triangles at {} m in {:.0} ms",
                voxels.frames.len(), voxels.triangles, CABIN_VOXEL_M, voxels.build_ms);
            self.acoustics.voxels = Some(Rc::new(voxels));
        }
        let done = {
            let (scene, _models) = self.acoustic_scene(key.inside, ear);
            let before = job.progress();
            let done = job.advance(&scene, per_update);
            self.acoustics.rays_traced += u64::from(job.progress() - before);
            done
        };
        if done {
            let seed = key.cell[0] as u32 ^ (key.cell[1] as u32).rotate_left(11) ^ (key.cell[2] as u32).rotate_left(22);
            let ir = Rc::new(StereoIr::synthesize(job.echogram(), seed));
            self.acoustics.cache.insert(key, ir);
            self.acoustics.order.push_back(key);
            self.acoustics.traces_done += 1;
            while self.acoustics.order.len() > CACHE_CAPACITY {
                if let Some(old) = self.acoustics.order.pop_front() { self.acoustics.cache.remove(&old); }
            }
        } else {
            self.acoustics.job = Some((key, job));
        }
    }

    /// Path from a foley source to the current ear, and whether the source shares the
    /// listener's compartment.
    pub(super) fn foley_path(&self, at: Vec3) -> crate::sound_propagation::Path {
        use crate::sound_propagation::{Enclosure, Path};
        let listener_inside = self.listener_aboard_inside();
        let source_inside = matches!(self.mode, Mode::Tank) && self.point_in_cabin(at)
            || (!matches!(self.mode, Mode::Tank) && at.distance(self.player.center()) < 4.0 && self.point_in_cabin(at));
        let enclosure = match (listener_inside, source_inside) {
            (true, true) => Enclosure::OnFoot,
            (true, false) => self.listener_enclosure(),
            (false, true) => if self.hatch_mask() == 0 { Enclosure::Buttoned } else { Enclosure::HatchOpen },
            (false, false) => if matches!(self.mode, Mode::Tank) { self.listener_enclosure() } else { Enclosure::OnFoot },
        };
        let enclosure = if matches!(enclosure, Enclosure::HeadOut) { Enclosure::OnFoot } else { enclosure };
        let occluded = !source_inside && !listener_inside && !self.los_clear(at + Vec3::Y * 0.05, self.ear_pos);
        Path { distance_m: at.distance(self.ear_pos).max(0.3), atmosphere: crate::sound_propagation::active_atmosphere(),
            occluded, enclosure, headset: self.listener_headset() }
    }

    fn foley_emission(cue: Cue, offset_db: f32) -> crate::sound_propagation::Emission {
        let laf = cue.ref_dba_1m() + offset_db.clamp(-40.0, 24.0);
        crate::sound_propagation::Emission { lpk_db: laf + 22.0, laf_db: laf, ref_m: 1.0,
            law: crate::sound_levels::DistanceLaw::Spherical, class: crate::sound_levels::SpectrumClass::Mechanical,
            spectrum: crate::sound_propagation::class_spectrum(crate::sound_levels::SpectrumClass::Mechanical),
            a_duration_ms: 1.0, tnt_kg: 0.0, directivity_db: 0.0 }
    }

    /// Play one foley event made at `at` (world). `intensity` shapes the excitation; `offset_db`
    /// moves the level from the cue's 1 m reference (the event's effort, an impact's speed).
    pub(super) fn foley(&mut self, cue: Cue, intensity: f32, offset_db: f32, at: Vec3) {
        if self.audio.is_none() || !at.is_finite() || cue.is_loop() { return; }
        self.acoustics.seq = self.acoustics.seq.wrapping_add(1);
        self.acoustics.foley_events += 1;
        *self.acoustics.cue_counts.entry(cue.name()).or_default() += 1;
        let seed = self.acoustics.seq.wrapping_mul(0x9E37_79B9) ^ (at.x.to_bits() >> 7) ^ (at.z.to_bits() << 3);
        let path = self.foley_path(at);
        let ear = crate::sound_propagation::propagate(&Self::foley_emission(cue, offset_db), &path);
        let bearing = widened_bearing_deg(bearing_deg_from(self.ear_forward, at - self.ear_pos));
        if ear.lpk_db > 125.0 { self.hear_world_impulse(&ear, bearing); }
        let room = self.acoustic_room_for(at);
        let reverb = self.surroundings_reverb(self.ear_pos, self.listener_enclosure());
        if let Some(audio) = &self.audio {
            match &room {
                Some(ir) => audio.play_foley_at_ear(cue, intensity, seed, &ear, bearing, crate::audio::Room::Traced(ir)),
                None => audio.play_foley_at_ear(cue, intensity, seed, &ear, bearing, crate::audio::Room::Parametric(reverb)),
            }
        }
    }

    /// Request a continuous foley cue this update (`id` distinguishes simultaneous sources).
    /// `rate` is the loop's playback speed: pitch and event rate together (the Aeolian tone's
    /// frequency is proportional to the wind; a track's link rate to the speed).
    pub(super) fn foley_loop(&mut self, cue: Cue, id: usize, at: Vec3, offset_db: f32, rate: f32) {
        if !at.is_finite() || !cue.is_loop() { return; }
        let path = self.foley_path(at);
        let exterior = !self.point_in_cabin(at);
        let ear = crate::sound_propagation::propagate(&Self::foley_emission(cue, offset_db), &path);
        let voice = crate::audio::VehicleVoice { id, ear_laf_dba: ear.laf_db,
            bearing_deg: widened_bearing_deg(bearing_deg_from(self.ear_forward, at - self.ear_pos)),
            lowpass_hz: crate::audio::lowpass_for_transfer(&ear.transfer_db), playback_speed: rate.clamp(0.25, 3.0) };
        self.acoustics.loops.push((cue, voice, exterior));
    }

    /// Hand this update's loop requests to the mixer; cues not requested fade out.
    pub(super) fn flush_foley_loops(&mut self, dt: f32) {
        let loops = std::mem::take(&mut self.acoustics.loops);
        let mut cues: Vec<Cue> = loops.iter().map(|(c, ..)| *c).collect();
        cues.sort_by_key(|c| c.name());
        cues.dedup();
        if let Some(audio) = &self.audio {
            for cue in &cues {
                let voices: Vec<_> = loops.iter().filter(|(c, ..)| c == cue).map(|(_, v, _)| *v).collect();
                let exterior = loops.iter().any(|(c, _, e)| c == cue && *e);
                audio.set_foley_loop(*cue, &voices, dt, exterior);
            }
            for old in &self.acoustics.active_loops {
                if !cues.contains(old) { audio.set_foley_loop(*old, &[], dt, true); }
            }
        }
        self.acoustics.active_loops = cues;
    }

    /// What is under a point on the ground: building floors, rubble, broken glass, the Tiger,
    /// or the act's snow and frozen ground.
    pub(super) fn foley_ground_at(&self, feet: Vec3) -> FootGround {
        if matches!(self.mode, Mode::Tank) { return FootGround::TurretFloor; }
        let terrain = self.hf.height_at(feet.x, feet.z);
        if feet.y > terrain + 0.6 {
            let local = self.player.hull_model().inverse().transform_point3(feet);
            if local.x.abs() < 2.0 && local.z.abs() < 4.3 && local.y > 0.8 && local.y < 3.4 {
                return FootGround::SteelDeck;
            }
            if self.buildings.walkable_surface_y(feet + Vec3::Y * 0.3, feet.y).is_some_and(|y| (y - feet.y).abs() < 0.4) {
                // Upper storeys are timber; ground floors over cellars are stone.
                return if feet.y > terrain + 2.0 { FootGround::WoodFloor } else { FootGround::StoneFloor };
            }
        }
        if self.buildings.rubble_top_from(Vec2::new(feet.x, feet.z), feet.y + 0.5)
            .is_some_and(|hit| hit.height > terrain + 0.15 && (hit.height - feet.y).abs() < 0.6) {
            return FootGround::Rubble;
        }
        let winter = matches!(self.act.season, crate::world::Season::Winter);
        match self.grass.as_ref().and_then(|g| g.vegetation_at(feet.x, feet.z)) {
            Some(true) => if winter { FootGround::FreshSnow } else { FootGround::Grass },
            Some(false) => if winter { FootGround::PackedSnow } else { FootGround::FrozenEarth },
            None => if winter { FootGround::FreshSnow } else { FootGround::Mud },
        }
    }

    /// Ground under the Tiger's tracks.
    fn track_ground(&self) -> VehicleCue {
        let c = self.player.center();
        let terrain = self.hf.height_at(c.x, c.z);
        if self.buildings.rubble_top_from(Vec2::new(c.x, c.z), self.player.pos.y + 0.5)
            .is_some_and(|hit| hit.height > terrain + 0.15) {
            return VehicleCue::TrackRubble;
        }
        if self.player.pos.y > terrain + 0.5 { return VehicleCue::TrackWood; }
        match self.act.season {
            crate::world::Season::Winter => VehicleCue::TrackSnow,
            crate::world::Season::Spring => VehicleCue::TrackGravel,
        }
    }

    /// The world's continuous sounds and the edge-triggered vehicle events, once per update.
    pub(super) fn update_world_foley(&mut self, dt: f32, r: &Renderer, turret_rate: f32) {
        if self.audio.is_none() || self.tank_editor_scene.is_some() { return; }
        self.acoustics.convolutions = 0;
        if let Some(audio) = &self.audio { audio.recorded_fx_begin_update(); }
        if !self.acoustics.primed {
            // The edges are changes from the state the game starts in, never from defaults.
            self.acoustics.primed = true;
            self.acoustics.last_gear = self.player_powertrain.gear;
            self.acoustics.last_engine_on = self.engine_on;
            self.acoustics.last_speed = self.player.speed.abs();
            self.acoustics.last_snow_load = self.snow_cover.player;
            self.acoustics.last_head_out = self.listener_enclosure() == crate::sound_propagation::Enclosure::HeadOut;
            self.acoustics.last_binocular = self.binocular_raise;
        }
        self.sim_phase.seg("world_foley.prelude");
        self.advance_acoustic_traces();
        self.sim_phase.seg("world_foley.traces");
        let hull = self.player.hull_model();
        let turret = self.player.turret_model();
        let wind = wind_at(self.act.id, r.camera.pos) * self.weather_wind_scale;
        let wind_mps = wind.length();
        let aboard = matches!(self.mode, Mode::Tank);
        let alive = self.player.alive;

        // Weather on the steel: snow grains (soft clicks) or, near freezing rain, sleet pellets.
        let precip = r.precipitation.clamp(0.0, 4.0);
        if precip > 0.02 && self.player.center().distance(self.ear_pos) < 40.0 {
            let roof = turret.transform_point3(Vec3::new(0.0, 2.55, 0.0));
            let cue = if r.storm > 0.6 { Cue::Env(EnvCue::SleetTicks) } else { Cue::Env(EnvCue::SnowTicks) };
            // Impacts per second scale with precipitation and wind-driven flux [E].
            let flux = precip * (1.0 + wind_mps / 8.0);
            self.foley_loop(cue, 0, roof, 10.0 * flux.max(0.01).log10(), (0.7 + 0.15 * flux).min(2.0));
        }
        // Aeolian tones of the antenna, the turret grab rails and, open, the hatch rim. Dipole
        // sound power grows with U^6 [E: 60 log10(U/10 m/s)]; frequency with U (0.2 U/d).
        if wind_mps > 2.5 && self.player.center().distance(self.ear_pos) < 60.0 {
            let level = 60.0 * (wind_mps / 10.0).log10();
            let pitch = wind_mps / 10.0;
            self.foley_loop(Cue::Env(EnvCue::WindAntenna), 0, hull.transform_point3(Vec3::new(-1.1, 2.9, -2.6)), level, pitch);
            self.foley_loop(Cue::Env(EnvCue::WindRail), 0, turret.transform_point3(Vec3::new(1.5, 2.2, -0.3)), level, pitch);
            if self.hatch_t > 0.2 {
                self.foley_loop(Cue::Env(EnvCue::WindHatchRim), 0,
                    turret.transform_point3(tiger_periscopes::COMMANDER_EYE + Vec3::Y * 0.35), level, pitch);
            }
        }
        // The turret ventilator runs off the generator while the engine turns [E].
        if aboard && alive && self.engine_on {
            self.foley_loop(Cue::Env(EnvCue::FanHum), 0, turret.transform_point3(Vec3::new(0.0, 2.2, 0.2)), 0.0, 1.0);
        }
        if aboard && r.interior_light > 0.05 && self.listener_aboard_inside() {
            self.foley_loop(Cue::Env(EnvCue::LampBuzz), 0, turret.transform_point3(crate::interior_rig::TURRET_DOME_LAMP_POS), 0.0, 1.0);
        }
        self.sim_phase.seg("world_foley.weather_loops");
        // Engine and exhaust cooling after shutdown: pings slowing over minutes [E].
        if self.engine_on { self.acoustics.engine_off_s = 0.0; } else { self.acoustics.engine_off_s += dt; }
        if !self.engine_on && self.acoustics.last_engine_on && std::mem::take(&mut self.acoustics.starved) {
            // Starved of fuel: the Maybach misfires and runs down (a switched-off engine plays
            // the owner's shutdown recording instead).
            let engine = hull.transform_point3(ModuleKind::Engine.station());
            self.foley(Cue::Vehicle(VehicleCue::FuelSputter), 1.0, 0.0, engine);
            self.foley(Cue::Vehicle(VehicleCue::Stall), 1.0, 0.0, engine);
        }
        self.acoustics.last_engine_on = self.engine_on;
        let ping_rate = 0.4 * (-self.acoustics.engine_off_s / 240.0).exp();
        self.acoustics.timers[0] += dt * ping_rate;
        if !self.engine_on && self.acoustics.timers[0] >= 1.0 && self.acoustics.engine_off_s < 1_200.0 {
            self.acoustics.timers[0] = 0.0;
            let k = self.acoustics.seq as f32;
            let at = hull.transform_point3(ModuleKind::Engine.station() + Vec3::new((k * 0.37).sin() * 0.6, 0.4, (k * 0.71).cos() * 0.5));
            self.foley(Cue::Env(EnvCue::HullPing), 1.0, 0.0, at);
        }
        // Wrecks cooling and settling.
        self.acoustics.timers[1] += dt * 0.25;
        if self.acoustics.timers[1] >= 1.0 {
            self.acoustics.timers[1] = 0.0;
            let ear = self.ear_pos;
            if let Some(wreck) = self.enemies.iter().filter(|e| !e.alive && e.pos.distance(ear) < 80.0)
                .min_by(|a, b| a.pos.distance(ear).total_cmp(&b.pos.distance(ear))).map(|e| e.pos + Vec3::Y * 1.5) {
                self.foley(Cue::Env(EnvCue::WreckTick), 1.0, 0.0, wreck);
            }
        }
        // Snow load sliding off the hull when the tank moves or fires.
        let load = self.snow_cover_player_load();
        if load + 0.02 < self.acoustics.last_snow_load {
            let shed = self.acoustics.last_snow_load - load;
            self.foley(Cue::Env(EnvCue::SnowSlough), (shed * 8.0).clamp(0.3, 3.0), 20.0 * (shed * 8.0).max(0.05).log10(),
                hull.transform_point3(Vec3::new(1.6, 1.6, 0.5)));
        }
        self.acoustics.last_snow_load = load;

        self.sim_phase.seg("world_foley.engine_wrecks_snow");
        // The Tiger moving: tracks on the ground they meet, braking, gear changes.
        let speed = self.player.speed.abs();
        if alive && speed > 0.4 {
            let cue = Cue::Vehicle(self.track_ground());
            let offset = 20.0 * (speed / 5.0).max(0.05).log10();
            for (side, x) in [(0usize, -1.6f32), (1, 1.6)] {
                self.foley_loop(cue, side, hull.transform_point3(Vec3::new(x, 0.6, 0.0)), offset, (speed / 5.0).clamp(0.3, 2.5));
            }
        }
        let decel = (self.acoustics.last_speed - speed) / dt.max(1.0e-3);
        if alive && speed > 1.0 && decel > 1.2 {
            self.foley_loop(Cue::Vehicle(VehicleCue::BrakeSqueal), 0, hull.transform_point3(Vec3::new(0.0, 0.9, 2.2)),
                10.0 * (decel / 3.0).max(0.1).log10(), (speed / 6.0).clamp(0.6, 1.6));
        }
        self.acoustics.last_speed = speed;
        let gear = self.player_powertrain.gear;
        if alive && gear != self.acoustics.last_gear {
            let at = hull.transform_point3(Vec3::new(0.0, 0.9, 2.0));
            self.foley(Cue::Vehicle(VehicleCue::GearShift), 1.0, 0.0, at);
        }
        self.acoustics.last_gear = gear;
        // The hand traverse with the engine off: the gunner's handwheel and the ring.
        let turning = turret_rate > 0.02;
        if aboard && alive && !self.engine_on && turning {
            self.foley_loop(Cue::Vehicle(VehicleCue::HandTraverse), 0,
                turret.transform_point3(crate::interior_rig::WHEEL1_POS), 0.0, (turret_rate * 2.0).clamp(0.4, 2.0));
        }
        // Grinding through a wall.
        if self.crush_rumble > 0.05 {
            self.foley_loop(Cue::Vehicle(VehicleCue::WallGrind), 0, hull.transform_point3(Vec3::new(0.0, 1.0, 3.2)),
                20.0 * self.crush_rumble.max(0.05).log10(), 1.0);
        }
        // Enemy T-34 tracks, separate from their engines: the cast links on the sprocket.
        let ear = self.ear_pos;
        let mut near: Vec<(usize, Vec3, f32)> = self.enemies.iter().enumerate()
            .filter(|(_, e)| e.alive && e.speed.abs() > 0.5 && e.pos.distance(ear) < 400.0)
            .map(|(i, e)| (i, e.pos + Vec3::Y * 0.8, e.speed.abs())).collect();
        near.sort_by(|a, b| a.1.distance(ear).total_cmp(&b.1.distance(ear)));
        for (i, at, v) in near.into_iter().take(4) {
            self.foley_loop(Cue::Vehicle(VehicleCue::EnemyTrackClank), i, at, 20.0 * (v / 5.0).max(0.05).log10(), (v / 5.0).clamp(0.4, 2.0));
        }
        self.sim_phase.seg("world_foley.tracks_enemies");
        // The extinguisher: the spray while the trigger is held.
        if self.ext_want_spray && !self.acoustics.last_ext {
            self.foley(Cue::Extinguisher(ExtCue::SprayStart), 1.0, 0.0, self.ear_pos + self.ear_forward * 0.5 - Vec3::Y * 0.3);
        }
        if !self.ext_want_spray && self.acoustics.last_ext {
            self.foley(Cue::Extinguisher(ExtCue::SprayEnd), 1.0, 0.0, self.ear_pos + self.ear_forward * 0.5 - Vec3::Y * 0.3);
        }
        if self.ext_want_spray {
            self.foley_loop(Cue::Extinguisher(ExtCue::Spray), 0, self.ear_pos + self.ear_forward * 0.6 - Vec3::Y * 0.3, 0.0, 1.0);
        }
        self.acoustics.last_ext = self.ext_want_spray;
        // Hatch and head: the body passing the rim.
        let head_out = self.listener_enclosure() == crate::sound_propagation::Enclosure::HeadOut;
        if head_out != self.acoustics.last_head_out && aboard {
            let cue = if head_out { CabinCue::HeadOut } else { CabinCue::HeadIn };
            self.foley(Cue::Cabin(cue), 1.0, 0.0, self.ear_pos - Vec3::Y * 0.3);
        }
        self.acoustics.last_head_out = head_out;
        // Binoculars brought up to the eyes.
        if self.binocular_raise > 0.05 && self.acoustics.last_binocular <= 0.05 {
            self.foley_hands(Cue::Handle(crate::acoustics::foley::Object::Binoculars, crate::acoustics::foley::Gesture::Grab));
        }
        self.acoustics.last_binocular = self.binocular_raise;
        // Fuel going into the tank.
        if self.acoustics.pour_s > 0.0 {
            self.acoustics.pour_s -= dt;
            let at = hull.transform_point3(Vec3::new(0.0, 1.7, -3.0));
            if self.acoustics.pour_s > 0.0 {
                self.foley_loop(Cue::Body(crate::acoustics::foley::BodyCue::FuelPour), 0, at, 0.0, 1.0);
            } else {
                self.foley(Cue::Body(crate::acoustics::foley::BodyCue::FuelCapClose), 1.0, 0.0, at);
            }
        }
        self.sim_phase.seg("world_foley.hands_misc");
        self.flush_foley_loops(dt);
        self.sim_phase.seg("world_foley.flush");
    }

    /// Where the own Maybach is heard from: the engine compartment behind the firewall. Inside
    /// the reverberant cabin only the direct share of its sound carries direction, so the lateral
    /// angle is kept by sqrt(direct / (direct + reverberant)) (Hopkinson–Stryker with the room
    /// constant `CABIN_ROOM_CONSTANT_M2`, radiation from the firewall into a half space, Q = 2) [E].
    pub(super) fn engine_heard_direction(&self, ear: Vec3, forward: Vec3) -> crate::acoustics::spatial::Direction {
        let engine = self.player.hull_model().transform_point3(ModuleKind::Engine.station());
        let d = crate::acoustics::spatial::Direction::horizontal(bearing_deg_from(forward, engine - ear));
        if self.listener_aboard_inside() {
            d.with_lateral_scaled(engine_direct_share(engine.distance(ear)).sqrt())
        } else { d }
    }

    /// The engine's level at a crew station relative to the 1.6 m reference station, inside:
    /// 10 log10 of the direct-plus-reverberant energy (Hopkinson–Stryker) [E].
    pub(super) fn engine_station_offset_db(&self, ear: Vec3) -> f32 {
        if !self.listener_aboard_inside() { return 0.0; }
        let engine = self.player.hull_model().transform_point3(ModuleKind::Engine.station());
        let energy = |r: f32| 2.0 / (4.0 * std::f32::consts::PI * r.max(0.5).powi(2)) + 4.0 / CABIN_ROOM_CONSTANT_M2;
        10.0 * (energy(engine.distance(ear)) / energy(1.6)).log10()
    }

    /// Snow lying on the player's Tiger (0..1 coverage); see `snow_cover`.
    fn snow_cover_player_load(&self) -> f32 {
        self.snow_cover.player
    }

    /// Where the listener's hands are (a held object, a lever).
    pub(super) fn hands_pos(&self) -> Vec3 {
        let flat = Vec3::new(self.ear_forward.x, 0.0, self.ear_forward.z).normalize_or_zero();
        self.ear_pos + flat * 0.35 - Vec3::Y * 0.38
    }

    /// Where his mouth is (cigarette, cough).
    pub(super) fn mouth_pos(&self) -> Vec3 {
        self.ear_pos + self.ear_forward * 0.09 - Vec3::Y * 0.10
    }

    pub(super) fn foley_hands(&mut self, cue: Cue) {
        let at = self.hands_pos();
        self.foley(cue, 1.0, 0.0, at);
    }

    pub(super) fn foley_mouth(&mut self, cue: Cue) {
        let at = self.mouth_pos();
        self.foley(cue, 1.0, 0.0, at);
    }

    /// Take-off and landing, from the foot state after this update's resolvers.
    pub(super) fn foley_airborne(&mut self, previously_airborne: bool) {
        if self.foot.airborne {
            self.acoustics.last_fall_speed = self.acoustics.last_fall_speed.max(-self.foot.vel_y);
        }
        if !previously_airborne && self.foot.airborne && self.foot.vel_y > 1.0 {
            let feet = self.foot.pos;
            let ground = self.foley_ground_at(feet);
            self.foley(Cue::Step(ground, Gait::TakeOff), 1.0, 0.0, feet);
        }
        if previously_airborne && !self.foot.airborne {
            let fall = std::mem::take(&mut self.acoustics.last_fall_speed);
            let feet = self.foot.pos;
            self.foley_landing(feet, fall);
        }
        if !self.foot.airborne { self.acoustics.last_fall_speed = 0.0; }
    }

    /// The tank's bow stopped dead against static mass this update (edge-triggered).
    pub(super) fn foley_hard_stop(&mut self, struck: bool, speed_before: f32, at: Vec3) {
        if struck && !self.acoustics.hard_contact && speed_before > 0.8 {
            self.foley(Cue::Vehicle(VehicleCue::HardStop), (speed_before / 3.0).clamp(0.3, 3.0),
                20.0 * (speed_before / 3.0).max(0.05).log10(), at);
        }
        self.acoustics.hard_contact = struck;
    }

    /// Refuelling from a can: the cap, then the pour for as long as the fuel takes [E: 2 L/s].
    pub(super) fn foley_start_pour(&mut self, liters: f32) {
        let at = self.player.hull_model().transform_point3(Vec3::new(0.0, 1.7, -3.0));
        self.foley(Cue::Body(crate::acoustics::foley::BodyCue::FuelCapOpen), 1.0, 0.0, at);
        self.acoustics.pour_s = (liters / 2.0).clamp(0.5, 12.0);
    }

    /// A body landing on its feet: after the fall, the gait by fall speed and the ground.
    pub(super) fn foley_landing(&mut self, feet: Vec3, fall_speed: f32) {
        if fall_speed < 1.2 { return; }
        let ground = self.foley_ground_at(feet);
        let gait = if fall_speed > 4.0 { Gait::LandHard } else { Gait::LandSoft };
        self.foley(Cue::Step(ground, gait), (fall_speed / 3.0).clamp(0.5, 3.0), 20.0 * (fall_speed / 3.0).max(0.1).log10(), feet);
    }

    pub(super) fn acoustics_receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "model": "acoustics::raytrace stochastic energy ray tracing per octave band against the Tiger's source triangles, terrain and cover boxes; binaural IR from the echogram; foley from acoustics::foley",
            "traces_done": self.acoustics.traces_done, "rays_traced": self.acoustics.rays_traced,
            "cached_fields": self.acoustics.cache.len(), "queued": self.acoustics.queue.len(),
            "rooms": {"traced": self.acoustics.traced_rooms, "stand_in": self.acoustics.stand_in_rooms,
                "parametric": self.acoustics.parametric_rooms},
            "foley_events": self.acoustics.foley_events, "cue_counts": self.acoustics.cue_counts,
            "active_loops": self.acoustics.active_loops.iter().map(|c| c.name()).collect::<Vec<_>>(),
        })
    }

    /// Recording/review only: a cue played at a world point through the live path.
    pub fn debug_foley(&mut self, cue: Cue, intensity: f32, at: Vec3) {
        self.foley(cue, intensity, 0.0, at);
    }

    pub fn debug_acoustics_receipt(&self) -> serde_json::Value {
        self.acoustics_receipt()
    }

    /// Recording/review only: spend rays until every queued trace is done (bounded).
    pub fn debug_finish_acoustic_traces(&mut self, max_updates: u32) {
        for _ in 0..max_updates {
            if self.acoustics.job.is_none() && self.acoustics.queue.is_empty() { break; }
            self.advance_acoustic_traces();
        }
    }
}


/// `KESSEL_ACOUSTICS=0` turns the traced rooms off (no traces, every voice on the parametric tail
/// it takes anyway once the convolution budget is spent): the A/B switch for their cost.
fn acoustics_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KESSEL_ACOUSTICS").as_deref() != Ok("0"))
}

impl WorldAcoustics {
    /// World acoustics whose cabin voxels come from `worker` (`CabinVoxels::spawn`).
    pub(super) fn with_cabin_voxels(worker: Option<std::thread::JoinHandle<CabinVoxels>>) -> Self {
        Self { pending_voxels: worker, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worker's voxels equal the ones built through `CabinFire`'s contact query: two frames
    /// of boxes (a closed room, a slab), one with a degenerate triangle and one past ±8 m.
    #[test]
    fn worker_voxels_equal_the_contact_built_ones() {
        let mut room = crate::prims::box_mesh(Vec3::new(-1.0, 0.0, -1.5), Vec3::new(1.0, 1.8, 1.5), [90, 90, 90]);
        room.indices.extend([0, 0, 1]);
        let slab = crate::prims::box_mesh(Vec3::new(-0.4, 1.0, -0.3), Vec3::new(0.5, 1.1, 0.8), [90, 90, 90]);
        let far = crate::prims::box_mesh(Vec3::new(20.0, 0.0, 20.0), Vec3::new(21.0, 1.0, 21.0), [90, 90, 90]);
        let meshes = [(0, &room), (1, &slab), (1, &far)];
        let built = CabinVoxels::build(&surface_fire::CabinFire::new(&meshes));
        let spawned = CabinVoxels::spawn(&meshes).expect("a worker").join().expect("the worker's voxels");
        assert_eq!(built.triangles, spawned.triangles);
        assert_eq!(built.frames.len(), 2);
        assert_eq!(built.frames, spawned.frames);
    }

    /// The box grid returns exactly what the linear scan over every box returns: the same
    /// nearest entry distance and normal (ties to the earlier box), for rays from inside and
    /// outside the boxes' area, with and without a limit, vertical and horizontal.
    #[test]
    fn box_grid_matches_the_linear_scan() {
        let mut seed = 0x1234_5678u32;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32
        };
        let mut boxes = Vec::new();
        for _ in 0..4000 {
            let c = Vec3::new(rand() * 300.0 - 150.0, rand() * 6.0, rand() * 300.0 - 150.0);
            let h = Vec3::new(0.2 + rand() * 12.0, 0.5 + rand() * 6.0, 0.2 + rand() * 12.0);
            boxes.push((c - h, c + h));
        }
        // Two identical boxes straddling a column edge: the tie goes to the first.
        boxes.push((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 3.0, 0.5)));
        boxes.push((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 3.0, 0.5)));
        let linear = |o: Vec3, d: Vec3, limit: f32| {
            let mut best: Option<(f32, Vec3)> = None;
            for &(min, max) in &boxes {
                if let Some((t, n)) = rt::ray_box_entry(o, d, min, max) {
                    if t < best.map_or(limit, |b| b.0) { best = Some((t, n)); }
                }
            }
            best
        };
        let grid = BoxGrid::new(boxes.clone());
        let mut hits = 0;
        for k in 0..20_000 {
            let o = Vec3::new(rand() * 500.0 - 250.0, rand() * 10.0 - 1.0, rand() * 500.0 - 250.0);
            let d = match k % 10 {
                0 => Vec3::Y,
                1 => Vec3::NEG_Y,
                2 => Vec3::X,
                _ => Vec3::new(rand() * 2.0 - 1.0, rand() * 2.0 - 1.0, rand() * 2.0 - 1.0).normalize_or(Vec3::Z),
            };
            let limit = if k % 3 == 0 { 400.0 } else { 1.0 + rand() * 60.0 };
            let (a, b) = (linear(o, d, limit), grid.first_entry(o, d, limit));
            assert_eq!(a, b, "ray {k}: o {o} d {d} limit {limit}");
            hits += usize::from(a.is_some());
        }
        assert!(hits > 2000, "the test rays must hit boxes often ({hits})");
        assert_eq!(grid.first_entry(Vec3::new(0.0, 1.0, -5.0), Vec3::Z, 100.0),
            linear(Vec3::new(0.0, 1.0, -5.0), Vec3::Z, 100.0));
        assert!(BoxGrid::new(Vec::new()).first_entry(Vec3::ZERO, Vec3::X, 100.0).is_none());
    }

    /// The voxelized cabin alone, in the hull's frame (no terrain, no world).
    struct VoxelOnly<'a>(&'a CabinVoxels, [Mat4; surface_fire::FRAMES], [Mat4; surface_fire::FRAMES]);
    impl AcousticScene for VoxelOnly<'_> {
        fn first_hit(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<Hit> {
            self.0.hit(&self.1, &self.2, origin, dir, max_distance.min(30.0))
        }
    }

    /// The real Tiger source triangles: the closed cabin holds the sound; the open cupola lets
    /// it out. Prints the trace cost, which sets `acoustic_ray_budget`.
    #[test]
    #[ignore = "needs the baked Tiger meshes (run with KESSEL_ASSET_ROOT at a full checkout)"]
    fn the_real_cabin_rings_and_an_open_cupola_lets_it_out() -> anyhow::Result<()> {
        let (parts, _, _, _, _, _, _, _) = tiger_hatches::load_player_tiger()?;
        let lid = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hatch.glb")?;
        let source = surface_fire::CabinFire::new(&[(0, &parts[0]), (1, &parts[1]), (2, &parts[2]), (6, &lid)]);
        let hf = Heightfield { res: 2, spacing: 2000.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0,
            openings: Default::default(), detail: None };
        let tank = Tank::new(0.0, 0.0, 0.0, &hf, tank::TIGER_PARTS);
        let (hull, turret) = (tank.hull_model(), tank.turret_model());
        let inv = hull.inverse();
        let listener = inv.transform_point3(turret.transform_point3(tiger_periscopes::COMMANDER_EYE - Vec3::Y * 0.1));
        let voxels = CabinVoxels::build(&source);
        println!("voxels: {} frames from {} triangles in {:.0} ms", voxels.frames.len(), voxels.triangles, voxels.build_ms);
        let caller = std::time::Instant::now();
        let worker = CabinVoxels::spawn(&[(0, &parts[0]), (1, &parts[1]), (2, &parts[2]), (6, &lid)]).expect("a worker");
        let caller_ms = caller.elapsed().as_secs_f64() * 1e3;
        let spawned = worker.join().expect("the worker's voxels");
        println!("worker: {caller_ms:.1} ms on the caller's thread, {:.0} ms on the worker", spawned.build_ms);
        assert_eq!(spawned.triangles, voxels.triangles);
        assert!(spawned.frames == voxels.frames, "the worker's voxels differ from the contact-built ones");
        let open_air = |p: Vec3, models: &[Mat4; surface_fire::FRAMES]| {
            let inv = models.map(|m| m.inverse());
            [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z].iter()
                .all(|d| voxels.hit(models, &inv, p, *d, 0.3).is_none())
        };
        let mut probe_models = [hull; surface_fire::FRAMES];
        probe_models[1] = turret; probe_models[2] = tank.gun_model(); probe_models[6] = cupola_hatch_model(turret, 0.0);
        let probe_models = probe_models.map(|m| inv * m);
        let src = [Vec3::new(-0.5, 1.6, 0.3), Vec3::new(-0.3, 1.8, -0.2), Vec3::new(0.0, 1.5, 0.6), Vec3::new(-0.6, 1.4, -0.5)]
            .into_iter().map(|p| inv.transform_point3(turret.transform_point3(p)))
            .find(|p| open_air(*p, &probe_models)).expect("an open-air point in the turret");
        println!("listener open air: {}", open_air(listener, &probe_models));
        {
            let mut models = [hull; surface_fire::FRAMES];
            models[1] = turret; models[2] = tank.gun_model(); models[6] = cupola_hatch_model(turret, 0.0);
            let local = models.map(|m| inv * m);
            let contacts = source.source_contacts(&local);
            let tris = contacts.world_triangles(Vec3::splat(-6.0), Vec3::splat(6.0));
            println!("probe: listener {listener:?} src {src:?}, {} triangles in +-6 m", tris.len());
            for p in [listener, src] {
                let mut line = String::new();
                for d in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z] {
                    let hit = contacts.sweep_batch(&[[p, p + d * 10.0]])[0];
                    line += &format!(" {:?}:{}", d, hit.map_or("none".into(), |c| format!("{:.2}m f{}", c.fraction * 10.0, c.frame)));
                }
                println!("probe at {p:?}:{line}");
            }
        }
        let mut results = Vec::new();
        for phase in [0.0f32, 1.0] {
            let mut models = [hull; surface_fire::FRAMES];
            models[1] = turret;
            models[2] = tank.gun_model();
            models[6] = cupola_hatch_model(turret, phase);
            let local = models.map(|m| inv * m);
            let scene = VoxelOnly(&voxels, local, local.map(|m| m.inverse()));
            let settings = TraceSettings { rays: 384, max_order: 40, max_time_s: 0.45, receiver_radius_m: 0.25,
                speed_m_s: 343.0, air_db_per_m: [0.0; crate::sound_propagation::BANDS], seed: 3 };
            let start = std::time::Instant::now();
            let gram = rt::trace(&scene, src, listener, Vec3::Z, &settings);
            let ms = start.elapsed().as_secs_f64() * 1e3;
            println!("cupola phase {phase}: {:.1} ms for 384 rays ({} reflections) {}", ms, gram.reflections, gram.summary());
            results.push(gram);
        }
        let (closed, open) = (&results[0], &results[1]);
        assert!(open.escaped > closed.escaped + 0.05, "the open cupola lets rays out: {} vs {}", open.escaped, closed.escaped);
        assert!(open.reflected_db()[6] < closed.reflected_db()[6], "less steel ring with the lid open");
        assert!(closed.reflected_db()[6] > -6.0, "the closed steel cabin is reverberant: {} dB", closed.reflected_db()[6]);
        Ok(())
    }
}
