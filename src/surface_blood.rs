//! Bounded physical droplets and connected, source-supported contact film.
//! Wound events own emission quantities. This module only transports those grains and spends each
//! real contact's fixed film-area budget beside that contact; it never creates an injury event.
use super::surface_fire::{Contact, FRAMES};
use glam::{Mat4, Vec3};
use std::collections::{HashMap, HashSet, VecDeque};

pub const GRAIN_M: f32 = 0.004;
pub const CELL_M: f32 = 0.005;
pub const MAX_GRAINS: usize = 8192;
pub const MAX_CELLS: usize = 65_536;
const MAX_FRONTIER: usize = MAX_CELLS * 4;
const FILM_CELLS_PER_CONTACT: u32 = 9;
const MAX_CELLS_PER_STEP: usize = 384;
const FILM_CELLS_PER_SECOND: f32 = 12_000.0;
const SURFACE_OFFSET: f32 = 0.00035;

/// The live implementation must use the existing source triangle bins. Sweeps are world-space;
/// support spans are local to one explicitly named source frame, and require continuous triangle
/// coverage rather than endpoint-only tests. Batch methods amortize frame inverses and scratch.
pub trait SourceQuery {
    fn sweep_batch(&self, paths: &[[Vec3; 2]], models: &[Mat4; FRAMES]) -> Vec<Option<Contact>>;
    fn supports_batch(&self, frame: usize, paths: &[[Vec3; 2]]) -> Vec<bool>;
    fn supports_cell(&self, frame: usize, corners: &[Vec3; 4]) -> bool;
}

impl SourceQuery for super::surface_fire::SourceContacts<'_> {
    fn sweep_batch(&self, paths: &[[Vec3; 2]], _models: &[Mat4; FRAMES]) -> Vec<Option<Contact>> {
        self.sweep_batch(paths)
    }
    fn supports_batch(&self, frame: usize, paths: &[[Vec3; 2]]) -> Vec<bool> {
        self.supports_batch(frame, paths)
    }
    fn supports_cell(&self, frame: usize, corners: &[Vec3; 4]) -> bool {
        self.supports_cell(frame, corners)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Emission {
    pub frame: usize,
    pub owner: u8,
    pub actor_id: u32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub rotation: Vec3,
    pub spin: Vec3,
    pub seed: u32,
}
#[derive(Clone, Copy, Debug)]
struct Grain {
    emission: Emission,
    age: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Plane {
    frame: u8,
    normal: [i16; 3],
    distance: i32,
}
impl Plane {
    fn from_contact(hit: Contact) -> Option<Self> {
        if hit.frame >= FRAMES
            || !hit.point.is_finite()
            || !hit.normal.is_finite()
            || hit.normal.length_squared() < 0.5
        {
            return None;
        }
        let n = hit.normal.normalize();
        let normal = (n * 16_384.0)
            .round()
            .as_ivec3()
            .to_array()
            .map(|v| v as i16);
        let normalized =
            Vec3::new(normal[0] as f32, normal[1] as f32, normal[2] as f32).normalize();
        Some(Self {
            frame: hit.frame as u8,
            normal,
            distance: (hit.point.dot(normalized) * 10_000.0).round() as i32,
        })
    }
    fn basis(self) -> (Vec3, Vec3, Vec3) {
        let n = Vec3::new(
            self.normal[0] as f32,
            self.normal[1] as f32,
            self.normal[2] as f32,
        )
        .normalize();
        let axis = if n.x.abs() < 0.8 { Vec3::X } else { Vec3::Z };
        let u = (axis - n * axis.dot(n)).normalize();
        (u, u.cross(n).normalize(), n)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Key {
    plane: Plane,
    u: i32,
    v: i32,
}
impl Key {
    fn at(hit: Contact) -> Option<Self> {
        let plane = Plane::from_contact(hit)?;
        let (u, v, _) = plane.basis();
        Some(Self {
            plane,
            u: (hit.point.dot(u) / CELL_M).floor() as i32,
            v: (hit.point.dot(v) / CELL_M).floor() as i32,
        })
    }
    fn center(self) -> Vec3 {
        let (u, v, n) = self.plane.basis();
        u * ((self.u as f32 + 0.5) * CELL_M)
            + v * ((self.v as f32 + 0.5) * CELL_M)
            + n * (self.plane.distance as f32 * 0.0001)
    }
    fn neighbors(self) -> [Self; 4] {
        [(1, 0), (0, 1), (-1, 0), (0, -1)].map(|(u, v)| Self {
            u: self.u + u,
            v: self.v + v,
            ..self
        })
    }
}
#[derive(Clone, Copy)]
struct Candidate {
    key: Key,
    from: Vec3,
}
struct Pool {
    parent: usize,
    credits: u32,
    owners: u8,
    sources: HashSet<(u32, u32)>, // actor ID and original emitted grain seed
    frontier: VecDeque<Candidate>,
    scheduled: bool,
    cells: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Deposit {
    pub frame: usize,
    pub point: Vec3,
    pub normal: Vec3,
    pub owner_mask: u8,
}
struct Cell {
    contact: Contact,
    pool: usize,
    key: Key,
}

/// A contiguous row of individually supported film cells. Combining adjacent cells cuts RT
/// instance count without making a large stamp across unhit gaps or erasing the pixel-sized edge.
#[derive(Clone, Copy, Debug)]
pub struct RenderRun {
    pub frame: usize,
    pub point: Vec3,
    pub normal: Vec3,
    pub tangent: Vec3,
    pub bitangent: Vec3,
    pub width: f32,
    pub cells: usize,
}
impl RenderRun {
    /// Unit-cube prototype: its X/Z footprint is the certified row, Y is a thin surface film.
    pub fn model(self, frames: &[Mat4; FRAMES]) -> Mat4 {
        frames[self.frame]
            * Mat4::from_cols(
                (self.tangent * self.width).extend(0.0),
                (self.normal * 0.0002).extend(0.0),
                (self.bitangent * CELL_M).extend(0.0),
                (self.point + self.normal * SURFACE_OFFSET).extend(1.0),
            )
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub emitted: u64,
    pub measured_contacts: u64,
    pub airborne: usize,
    pub retained_cells: usize,
    pub wall_cells: usize,
    pub rejected_cells: u64,
    pub capacity_rejections: u64,
    pub timed_out_airborne: u64,
    pub render_runs: usize,
}

pub struct SurfaceBlood {
    grains: Vec<Option<Grain>>,
    next_grain: usize,
    cells: Vec<Cell>,
    lookup: HashMap<Key, usize>,
    queued: HashMap<Key, usize>,
    rejected: HashSet<Key>,
    pools: Vec<Pool>,
    scheduled: VecDeque<usize>,
    work_credit: f32,
    cell_limit: usize,
    source_count: usize,
    diagnostics: Diagnostics,
    runs: Vec<RenderRun>,
    runs_dirty: bool,
}
impl Default for SurfaceBlood {
    fn default() -> Self {
        Self::new()
    }
}
impl SurfaceBlood {
    pub fn new() -> Self {
        Self {
            grains: vec![None; MAX_GRAINS],
            next_grain: 0,
            cells: Vec::new(),
            lookup: HashMap::new(),
            queued: HashMap::new(),
            rejected: HashSet::new(),
            pools: Vec::new(),
            scheduled: VecDeque::new(),
            work_credit: 0.0,
            cell_limit: MAX_CELLS,
            source_count: 0,
            diagnostics: Diagnostics::default(),
            runs: Vec::new(),
            runs_dirty: false,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    pub fn emit(&mut self, emission: Emission) -> bool {
        if emission.frame >= FRAMES
            || emission.owner >= 8
            || !emission.position.is_finite()
            || !emission.velocity.is_finite()
            || !emission.rotation.is_finite()
            || !emission.spin.is_finite()
        {
            return false;
        }
        self.grains[self.next_grain] = Some(Grain { emission, age: 0.0 });
        self.next_grain = (self.next_grain + 1) % MAX_GRAINS;
        self.diagnostics.emitted += 1;
        true
    }
    fn root(&self, mut i: usize) -> usize {
        while self.pools[i].parent != i {
            i = self.pools[i].parent;
        }
        i
    }
    fn schedule(&mut self, i: usize) {
        let i = self.root(i);
        if !self.pools[i].scheduled
            && self.pools[i].credits > 0
            && !self.pools[i].frontier.is_empty()
        {
            self.pools[i].scheduled = true;
            self.scheduled.push_back(i);
        }
    }
    fn join(&mut self, a: usize, b: usize) -> usize {
        let (mut a, mut b) = (self.root(a), self.root(b));
        if a == b {
            return a;
        }
        if self.pools[a].cells < self.pools[b].cells {
            std::mem::swap(&mut a, &mut b);
        }
        self.pools[b].parent = a;
        self.pools[a].credits = self.pools[a].credits.saturating_add(self.pools[b].credits);
        self.pools[b].credits = 0;
        self.pools[a].owners |= self.pools[b].owners;
        let sources = std::mem::take(&mut self.pools[b].sources);
        self.pools[a].sources.extend(sources);
        self.pools[a].cells += self.pools[b].cells;
        let frontier = std::mem::take(&mut self.pools[b].frontier);
        self.pools[a].frontier.extend(frontier);
        self.schedule(a);
        a
    }
    fn enqueue(&mut self, pool: usize, key: Key, from: Vec3) {
        if self.lookup.contains_key(&key)
            || self.queued.contains_key(&key)
            || self.rejected.contains(&key)
        {
            return;
        }
        if self.queued.len() + self.rejected.len() >= MAX_FRONTIER {
            self.diagnostics.capacity_rejections += 1;
            return;
        }
        let pool = self.root(pool);
        self.queued.insert(key, pool);
        self.pools[pool].frontier.push_back(Candidate { key, from });
        self.schedule(pool);
    }
    /// Only an actual sweep result may call this. Credits are an authored thin-film area quantum,
    /// not a calibrated fluid volume or a new injury quantity.
    pub fn record_contact(&mut self, hit: Contact, owner: u8, actor_id: u32, seed: u32) -> bool {
        let Some(key) = Key::at(hit) else {
            return false;
        };
        if owner >= 8 {
            return false;
        }
        self.diagnostics.measured_contacts += 1;
        if self.cells.len() >= self.cell_limit || self.source_count >= MAX_CELLS {
            self.diagnostics.capacity_rejections += 1;
            return false;
        }
        let existing = self.lookup.get(&key).map(|&c| self.cells[c].pool);
        let pool = if let Some(pool) = existing {
            self.root(pool)
        } else {
            if self.rejected.contains(&key) || self.pools.len() >= MAX_CELLS {
                self.diagnostics.capacity_rejections += 1;
                return false;
            }
            let id = self.pools.len();
            self.pools.push(Pool {
                parent: id,
                credits: 0,
                owners: 0,
                sources: HashSet::new(),
                frontier: VecDeque::new(),
                scheduled: false,
                cells: 0,
            });
            id
        };
        self.pools[pool].credits = self.pools[pool]
            .credits
            .saturating_add(FILM_CELLS_PER_CONTACT)
            .min(MAX_CELLS as u32);
        self.pools[pool].owners |= 1 << owner;
        if self.pools[pool].sources.insert((actor_id, seed)) {
            self.source_count += 1;
        }
        if !self.lookup.contains_key(&key) {
            // A not-yet-certified frontier cell cannot receive this contact's budget: that
            // would let a nearby pool spend it elsewhere before proving connection. Keep an
            // independent seed, even when another component has queued this same cell.
            self.queued.entry(key).or_insert(pool);
            self.pools[pool].frontier.push_back(Candidate {
                key,
                from: hit.point,
            });
        }
        self.schedule(pool);
        true
    }
    pub fn update(&mut self, dt: f32, models: &[Mat4; FRAMES], source: &impl SourceQuery) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        if dt <= 0.0 {
            return;
        }
        let inverses = models.map(|m| m.inverse());
        let steps = (dt / (1.0 / 60.0)).ceil().max(1.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let mut paths = Vec::new();
            let mut indices = Vec::new();
            let mut ends = Vec::new();
            for (i, slot) in self.grains.iter_mut().enumerate() {
                let Some(g) = slot else {
                    continue;
                };
                g.age += h;
                if g.age >= 9.0 {
                    self.diagnostics.timed_out_airborne += 1;
                    *slot = None;
                    continue;
                }
                let frame = g.emission.frame;
                let gravity = inverses[frame].transform_vector3(Vec3::NEG_Y * 9.8);
                let end = g.emission.position + g.emission.velocity * h + gravity * (0.5 * h * h);
                g.emission.velocity += gravity * h;
                g.emission.rotation += g.emission.spin * h;
                let from = models[frame].transform_point3(g.emission.position);
                let to = models[frame].transform_point3(end);
                let leading = to + (to - from).normalize_or_zero() * (GRAIN_M * 0.5);
                indices.push(i);
                ends.push(end);
                paths.push([from, leading]);
            }
            let hits = source.sweep_batch(&paths, models);
            assert_eq!(
                hits.len(),
                paths.len(),
                "source sweep batch must preserve input indices"
            );
            for ((index, end), hit) in indices.into_iter().zip(ends).zip(hits) {
                let emission = self.grains[index].as_ref().unwrap().emission;
                if let Some(hit) = hit {
                    self.record_contact(hit, emission.owner, emission.actor_id, emission.seed);
                    self.grains[index] = None; // absorptive contact, not repeated artificial rebound stamps
                } else {
                    self.grains[index].as_mut().unwrap().emission.position = end;
                }
            }
        }
        self.work_credit =
            (self.work_credit + dt * FILM_CELLS_PER_SECOND).min(MAX_CELLS_PER_STEP as f32);
        let work = self.work_credit.floor() as usize;
        self.work_credit -= work as f32;
        self.grow(work, models, source);
        self.rebuild_runs();
    }
    fn grow(&mut self, limit: usize, models: &[Mat4; FRAMES], source: &impl SourceQuery) {
        for _ in 0..limit {
            if self.cells.len() >= self.cell_limit {
                break;
            }
            let Some(id) = self.scheduled.pop_front() else {
                break;
            };
            if self.pools[id].parent != id {
                continue;
            }
            self.pools[id].scheduled = false;
            if self.pools[id].credits == 0 {
                continue;
            }
            let Some(candidate) = self.pools[id].frontier.pop_front() else {
                continue;
            };
            if self
                .queued
                .get(&candidate.key)
                .is_some_and(|&p| self.root(p) == id)
            {
                self.queued.remove(&candidate.key);
            }
            if let Some(&other) = self.lookup.get(&candidate.key) {
                let cell = &self.cells[other];
                let span = [[candidate.from, cell.contact.point]];
                if source.supports_batch(cell.contact.frame, &span) == [true] {
                    let root = self.join(id, cell.pool);
                    self.schedule(root);
                } else {
                    self.diagnostics.rejected_cells += 1;
                    self.schedule(id);
                }
                continue;
            }
            let key = candidate.key;
            let frame = key.plane.frame as usize;
            let (u, v, n) = key.plane.basis();
            let center = key.center();
            let world = models[frame];
            let probe = [[
                world.transform_point3(center + n * 0.003),
                world.transform_point3(center - n * 0.003),
            ]];
            let hit = source
                .sweep_batch(&probe, models)
                .into_iter()
                .next()
                .flatten();
            let Some(hit) = hit.filter(|h| {
                h.frame == frame && h.normal.dot(n) > 0.995 && (h.point - center).length() < 0.0002
            }) else {
                self.rejected.insert(key);
                self.diagnostics.rejected_cells += 1;
                self.schedule(id);
                continue;
            };
            let half = CELL_M * 0.5;
            let corners = [
                hit.point - u * half - v * half,
                hit.point + u * half - v * half,
                hit.point + u * half + v * half,
                hit.point - u * half + v * half,
            ];
            let spans = [
                [candidate.from, hit.point],
                [corners[0], corners[1]],
                [corners[1], corners[2]],
                [corners[2], corners[3]],
                [corners[3], corners[0]],
                [corners[0], corners[2]],
            ];
            let support = source.supports_batch(frame, &spans);
            if support.len() != spans.len()
                || support.iter().any(|v| !*v)
                || !source.supports_cell(frame, &corners)
            {
                self.rejected.insert(key);
                self.diagnostics.rejected_cells += 1;
                self.schedule(id);
                continue;
            }
            let mut pool = self.root(id);
            for neighbor in key.neighbors() {
                if let Some(&c) = self.lookup.get(&neighbor) {
                    pool = self.join(pool, self.cells[c].pool);
                }
            }
            self.pools[pool].credits = self.pools[pool].credits.saturating_sub(1);
            self.pools[pool].cells += 1;
            self.lookup.insert(key, self.cells.len());
            self.cells.push(Cell {
                contact: hit,
                pool,
                key,
            });
            self.runs_dirty = true;
            for neighbor in key.neighbors() {
                self.enqueue(pool, neighbor, hit.point);
            }
            self.schedule(pool);
        }
    }
    fn rebuild_runs(&mut self) {
        if !self.runs_dirty {
            return;
        }
        self.runs_dirty = false;
        self.runs.clear();
        let mut order: Vec<_> = (0..self.cells.len()).collect();
        order.sort_unstable_by_key(|&i| {
            let k = self.cells[i].key;
            (k.plane, k.v, k.u)
        });
        let mut at = 0;
        while at < order.len() {
            let first = &self.cells[order[at]];
            let mut end = at + 1;
            while end < order.len() {
                let a = &self.cells[order[end - 1]];
                let b = &self.cells[order[end]];
                if a.key.plane != b.key.plane || a.key.v != b.key.v || a.key.u + 1 != b.key.u {
                    break;
                }
                end += 1;
            }
            let count = end - at;
            let (u, v, n) = first.key.plane.basis();
            self.runs.push(RenderRun {
                frame: first.contact.frame,
                point: first.contact.point + u * (CELL_M * (count - 1) as f32 * 0.5),
                normal: n,
                tangent: u,
                bitangent: v,
                width: count as f32 * CELL_M,
                cells: count,
            });
            at = end;
        }
    }
    pub fn wet_models(&self, models: &[Mat4; FRAMES]) -> [Vec<Mat4>; 4] {
        let mut out = std::array::from_fn(|_| Vec::new());
        for (i, g) in self
            .grains
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.as_ref().map(|g| (i, g)))
        {
            let e = g.emission;
            let rotation = glam::Quat::from_euler(
                glam::EulerRot::ZYX,
                e.rotation.z,
                e.rotation.y,
                e.rotation.x,
            );
            out[i % 4].push(
                models[e.frame]
                    * Mat4::from_scale_rotation_translation(
                        Vec3::splat(GRAIN_M),
                        rotation,
                        e.position,
                    ),
            );
        }
        out
    }
    pub fn render_runs(&self) -> &[RenderRun] {
        &self.runs
    }
    pub fn deposits(&self) -> impl Iterator<Item = Deposit> + '_ {
        self.cells.iter().map(|c| Deposit {
            frame: c.contact.frame,
            point: c.contact.point,
            normal: c.contact.normal,
            owner_mask: self.pools[self.root(c.pool)].owners,
        })
    }
    /// Possible contributing actors/grains in this connected component. Unioning components
    /// retains original source IDs; geometry never creates an injury or changes wound attribution.
    pub fn contact_sources(&self, cell: usize) -> Option<&HashSet<(u32, u32)>> {
        self.cells
            .get(cell)
            .map(|c| &self.pools[self.root(c.pool)].sources)
    }
    pub fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            airborne: self.grains.iter().filter(|g| g.is_some()).count(),
            retained_cells: self.cells.len(),
            wall_cells: self
                .cells
                .iter()
                .filter(|c| c.contact.normal.y.abs() < 0.72)
                .count(),
            render_runs: self.runs.len(),
            ..self.diagnostics
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::surface_fire::CabinFire;
    use super::*;
    use crate::gltf_load::CpuMesh;
    fn floor(x0: f32, x1: f32, z0: f32, z1: f32) -> CpuMesh {
        crate::prims::box_mesh(Vec3::new(x0, -0.05, z0), Vec3::new(x1, 0.0, z1), [80; 3])
    }
    fn emission(point: Vec3, velocity: Vec3) -> Emission {
        Emission {
            frame: 0,
            owner: 2,
            actor_id: 42,
            position: point,
            velocity,
            rotation: Vec3::ZERO,
            spin: Vec3::ZERO,
            seed: 819,
        }
    }
    fn hit(source: &CabinFire, point: Vec3, n: Vec3) -> Contact {
        source
            .sweep(
                point + n * 0.002,
                point - n * 0.002,
                &[Mat4::IDENTITY; FRAMES],
            )
            .unwrap()
    }
    fn grow(blood: &mut SurfaceBlood, source: &CabinFire, steps: usize) {
        let models = [Mat4::IDENTITY; FRAMES];
        let query = source.source_contacts(&models);
        for _ in 0..steps {
            blood.update(1.0 / 60.0, &models, &query);
        }
    }
    #[test]
    fn actual_grain_contact_retains_source_frame_and_actor_while_hatch_moves() {
        let mesh = floor(-0.2, 0.2, -0.2, 0.2);
        let source = CabinFire::new(&[(7, &mesh)]);
        let mut blood = SurfaceBlood::new();
        blood.emit(emission(Vec3::new(0.02, 0.035, 0.02), Vec3::NEG_Y * 3.0));
        grow(&mut blood, &source, 2);
        assert_eq!(blood.diagnostics().emitted, 1);
        assert_eq!(blood.diagnostics().measured_contacts, 1);
        assert_eq!(blood.diagnostics().airborne, 0);
        assert_eq!(blood.diagnostics().retained_cells, 9);
        assert!(blood
            .deposits()
            .all(|d| d.frame == 7 && d.point.y.abs() < 1e-6 && d.owner_mask == (1 << 2)));
        assert_eq!(
            blood.contact_sources(0).unwrap(),
            &HashSet::from([(42, 819)])
        );
        let mut models = [Mat4::IDENTITY; FRAMES];
        models[7] = Mat4::from_rotation_translation(
            glam::Quat::from_rotation_z(0.7),
            Vec3::new(4.0, 2.0, -3.0),
        );
        for run in blood.render_runs() {
            let transform = run.model(&models);
            assert!(
                transform.determinant() > 0.0,
                "retained film must preserve face winding"
            );
            let expected = models[7].transform_point3(run.point + run.normal * SURFACE_OFFSET);
            assert!(transform.transform_point3(Vec3::ZERO).distance(expected) < 1e-5);
            assert!((transform.x_axis.truncate().length() - run.width).abs() < 1e-6);
            assert!((transform.z_axis.truncate().length() - CELL_M).abs() < 1e-6);
        }
    }
    #[test]
    fn repeated_contacts_grow_connected_film_without_moving_prior_cells_or_losing_actor_sources() {
        let mesh = floor(-0.3, 0.3, -0.3, 0.3);
        let source = CabinFire::new(&[(0, &mesh)]);
        let contact = hit(&source, Vec3::new(0.0125, 0.0, 0.0125), Vec3::Y);
        let mut blood = SurfaceBlood::new();
        blood.record_contact(contact, 0, 101, 1);
        grow(&mut blood, &source, 1);
        let original: Vec<_> = blood.deposits().map(|d| d.point).collect();
        for seed in 2..=20 {
            blood.record_contact(contact, 1, 202, seed);
        }
        grow(&mut blood, &source, 4);
        assert_eq!(blood.diagnostics().retained_cells, 20 * 9);
        assert_eq!(blood.contact_sources(0).unwrap().len(), 20);
        assert!(blood.contact_sources(0).unwrap().contains(&(101, 1)));
        assert!(blood.contact_sources(0).unwrap().contains(&(202, 20)));
        assert_eq!(
            blood
                .deposits()
                .take(original.len())
                .map(|d| d.point)
                .collect::<Vec<_>>(),
            original
        );
        assert!(blood.deposits().all(|d| d.owner_mask == 3));
        // Four-neighbor connectivity is a property of accepted cells, not proximity to an old pool.
        let first = blood.cells[0].key;
        let mut visited = HashSet::from([first]);
        let mut open = VecDeque::from([first]);
        while let Some(k) = open.pop_front() {
            for n in k.neighbors() {
                if blood.lookup.contains_key(&n) && visited.insert(n) {
                    open.push_back(n);
                }
            }
        }
        assert_eq!(visited.len(), blood.cells.len());
        assert_eq!(
            blood.render_runs().iter().map(|r| r.cells).sum::<usize>(),
            blood.cells.len()
        );
        assert!(blood.render_runs().len() < blood.cells.len() / 3);
    }
    #[test]
    fn pending_same_cell_contacts_keep_all_credits_only_after_source_connection_is_proved() {
        let mesh = floor(-0.2, 0.2, -0.2, 0.2);
        let source = CabinFire::new(&[(0, &mesh)]);
        let contact = hit(&source, Vec3::new(0.0125, 0.0, 0.0125), Vec3::Y);
        let mut blood = SurfaceBlood::new();
        for seed in 0..32 {
            assert!(blood.record_contact(contact, 0, 42, seed));
        }
        grow(&mut blood, &source, 4);
        assert_eq!(blood.diagnostics().retained_cells, 32 * 9);
        assert_eq!(blood.contact_sources(0).unwrap().len(), 32);
    }
    #[test]
    fn saturation_rejects_remote_contact_without_relocating_or_enlarging_retained_film() {
        let mesh = floor(-1.0, 1.0, -1.0, 1.0);
        let source = CabinFire::new(&[(0, &mesh)]);
        let mut blood = SurfaceBlood::new();
        blood.cell_limit = 17;
        let local = hit(&source, Vec3::new(-0.4, 0.0, 0.0), Vec3::Y);
        blood.record_contact(local, 0, 10, 1);
        blood.record_contact(local, 0, 10, 2);
        grow(&mut blood, &source, 3);
        assert_eq!(blood.cells.len(), 17);
        let before: Vec<_> = blood
            .deposits()
            .map(|d| (d.frame, d.point, d.normal))
            .collect();
        let remote = hit(&source, Vec3::new(0.4, 0.0, 0.0), Vec3::Y);
        assert!(!blood.record_contact(remote, 1, 20, 3));
        grow(&mut blood, &source, 3);
        assert_eq!(
            blood
                .deposits()
                .map(|d| (d.frame, d.point, d.normal))
                .collect::<Vec<_>>(),
            before
        );
        assert_eq!(blood.diagnostics().capacity_rejections, 1);
        assert!(!blood.contact_sources(0).unwrap().contains(&(20, 3)));
    }
    #[test]
    fn gaps_and_opposing_wall_faces_never_share_unproven_pool_area() {
        let left = floor(-0.15, -0.002, -0.15, 0.15);
        let right = floor(0.002, 0.15, -0.15, 0.15);
        let source = CabinFire::new(&[(0, &left), (0, &right)]);
        let mut blood = SurfaceBlood::new();
        for seed in 0..20 {
            blood.record_contact(
                hit(&source, Vec3::new(-0.0125, 0.0, 0.0125), Vec3::Y),
                0,
                10,
                seed,
            );
        }
        grow(&mut blood, &source, 6);
        assert!(!blood.cells.is_empty());
        assert!(blood.deposits().all(|d| d.point.x <= -0.0074));
        for seed in 0..3 {
            blood.record_contact(
                hit(&source, Vec3::new(0.0125, 0.0, 0.0125), Vec3::Y),
                1,
                20,
                seed,
            );
        }
        grow(&mut blood, &source, 4);
        for d in blood.deposits() {
            assert!(if d.point.x < 0.0 {
                d.owner_mask == 1
            } else {
                d.owner_mask == 2
            });
        }
        assert!(blood.diagnostics().rejected_cells > 0);
        let wall = crate::prims::box_mesh(
            Vec3::new(0.0, -0.2, -0.2),
            Vec3::new(0.004, 0.2, 0.2),
            [80; 3],
        );
        let source = CabinFire::new(&[(3, &wall)]);
        let mut blood = SurfaceBlood::new();
        blood.record_contact(
            hit(&source, Vec3::new(0.0, 0.02, 0.02), Vec3::NEG_X),
            0,
            10,
            1,
        );
        blood.record_contact(
            hit(&source, Vec3::new(0.004, 0.02, 0.02), Vec3::X),
            1,
            20,
            1,
        );
        grow(&mut blood, &source, 3);
        assert_eq!(blood.diagnostics().retained_cells, 18);
        assert_eq!(blood.diagnostics().wall_cells, 18);
        for d in blood.deposits() {
            assert_eq!(d.owner_mask, if d.normal.x < 0.0 { 1 } else { 2 });
        }
    }
    #[test]
    fn large_angled_source_triangles_accept_fine_cells_without_quantization_seams() {
        let rotation = Mat4::from_rotation_z(0.217);
        let n = rotation.transform_vector3(Vec3::Y);
        let mesh = floor(-2.0, 2.0, -2.0, 2.0).transformed(rotation);
        let source = CabinFire::new(&[(2, &mesh)]);
        let p = rotation.transform_point3(Vec3::new(0.8125, 0.0, 0.1125));
        let actual = hit(&source, p, n);
        let mut blood = SurfaceBlood::new();
        for seed in 0..12 {
            blood.record_contact(actual, 0, 99, seed);
        }
        grow(&mut blood, &source, 4);
        assert_eq!(blood.diagnostics().retained_cells, 12 * 9);
        assert!(blood
            .deposits()
            .all(|d| d.point.dot(n).abs() < 1e-4 && d.frame == 2));
    }
    #[test]
    fn complete_source_area_check_crosses_uv_seams_but_rejects_an_interior_hole() {
        let pieces: Vec<_> = (0..21)
            .map(|i| {
                floor(
                    -0.05 + i as f32 * 0.1 / 21.0,
                    -0.05 + (i + 1) as f32 * 0.1 / 21.0,
                    -0.05,
                    0.05,
                )
            })
            .collect();
        let refs: Vec<_> = pieces.iter().map(|m| (0, m)).collect();
        let source = CabinFire::new(&refs);
        let models = [Mat4::IDENTITY; FRAMES];
        let corners = [
            Vec3::new(-0.0025, 0.0, -0.0025),
            Vec3::new(0.0025, 0.0, -0.0025),
            Vec3::new(0.0025, 0.0, 0.0025),
            Vec3::new(-0.0025, 0.0, 0.0025),
        ];
        assert!(source.source_contacts(&models).supports_cell(0, &corners));
        assert!(
            !source.source_contacts(&models).supports_cell(1, &corners),
            "a different frame cannot lend coverage"
        );
        // A tiny enclosed missing patch misses all four edges and the cell's first diagonal.
        let pieces = [
            floor(0.0, 0.001, 0.0, 0.005),
            floor(0.0015, 0.005, 0.0, 0.005),
            floor(0.001, 0.0015, 0.0, 0.003),
            floor(0.001, 0.0015, 0.0035, 0.005),
        ];
        let refs: Vec<_> = pieces.iter().map(|m| (0, m)).collect();
        let source = CabinFire::new(&refs);
        let corners = [
            Vec3::ZERO,
            Vec3::new(0.005, 0.0, 0.0),
            Vec3::new(0.005, 0.0, 0.005),
            Vec3::new(0.0, 0.0, 0.005),
        ];
        let spans = [
            [corners[0], corners[1]],
            [corners[1], corners[2]],
            [corners[2], corners[3]],
            [corners[3], corners[0]],
            [corners[0], corners[2]],
        ];
        let query = source.source_contacts(&models);
        assert!(query.supports_batch(0, &spans).iter().all(|&x| x));
        assert!(!query.supports_cell(0, &corners));
    }
    #[test]
    fn four_millimeter_grains_preserve_input_motion_and_expiry_or_recycling_paints_nothing() {
        let source = CabinFire::new(&[]);
        let models = [Mat4::IDENTITY; FRAMES];
        let mut blood = SurfaceBlood::new();
        let e = emission(Vec3::new(0.0, 2.0, 0.0), Vec3::new(1.0, 2.0, 3.0));
        blood.emit(e);
        let before = blood.grains[0].unwrap().emission;
        assert_eq!(before.velocity, e.velocity);
        assert_eq!(before.actor_id, e.actor_id);
        assert_eq!(before.seed, e.seed);
        for m in blood.wet_models(&models).into_iter().flatten() {
            assert!((m.x_axis.truncate().length() - 0.004).abs() < 1e-7);
            assert!((m.y_axis.truncate().length() - 0.004).abs() < 1e-7);
            assert!((m.z_axis.truncate().length() - 0.004).abs() < 1e-7);
        }
        // Expiry is tested with one airborne grain; replacement then separately proves boundedness.
        for _ in 0..100 {
            blood.update(0.1, &models, &source.source_contacts(&models));
        }
        assert_eq!(blood.diagnostics().timed_out_airborne, 1);
        assert_eq!(blood.diagnostics().retained_cells, 0);
        for _ in 0..MAX_GRAINS + 1 {
            assert!(blood.emit(e));
        }
        assert_eq!(blood.diagnostics().airborne, MAX_GRAINS);
        assert_eq!(blood.diagnostics().measured_contacts, 0);
        assert!(blood.cells.is_empty());
    }
}
