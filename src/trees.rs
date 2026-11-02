//! Destructible trees (docs/DESTRUCTION.md §5, slice 2). Released trees use actual sampled
//! source contacts, gravity and rigid-body torque; no fracture or compliant branch solver. Small firs snap under the tracks or any shell;
//! big pines fall to a shell or a heavy tank's sustained shove (the Tiger pushes them over at a
//! crawl; lighter vehicles are blocked). Source-surface terrain contact ends the fall, not a
//! preset terminal angle. A fallen trunk stays down forever as a low obstacle the
//! tank simply drives over.
//!
//! Trees live in the renderer's INSTANCED path (they arrive by the thousands from worldgen /
//! the act manifests), so this subsystem owns the CPU-side transform lists for every tree batch
//! and rewrites a batch's instance buffer only on the frames a tree in it is actually moving —
//! the same device-clone + `set_instanced_mats` pattern the grass system already uses. All
//! randomness is counter-hashed off the tree index (no rand crate).

use glam::{Mat4, Quat, Vec3};
use std::path::Path;
use std::sync::Arc;

use crate::gltf_load::CpuMesh;
use crate::renderer::{InstancedPrototype, Renderer};

#[path = "tree_review.rs"]
mod review;
#[path = "tree_ground_support.rs"]
mod ground_support;

/// Trunk contact radius scales with the instance's footprint; height classifies small fir vs
/// big pine (the crush tiers differ — DESTRUCTION.md §2).
const BIG_TREE_HEIGHT: f32 = 6.5; // m — world height at or above this = BigTree (tier-3 crush)
const TRUNK_R: f32 = 0.30; // m — base trunk contact/ray radius at scale 1
const LOS_TARGET_SLOP: f32 = 0.25; // do not self-mask an endpoint touching its own nearby trunk

/// Whether a manifest model is a live standing tree that belongs in trunk collision/destruction.
///
/// Inspect only the file stem: Pizza stores stones, ferns and grass below a directory named
/// `Trees, Rocks & Nature`, so matching the full path silently turned all of that dressing into
/// thousands of physics trees. Explicit non-trunk tokens also keep `tree_log` and `tree_stump`
/// visual-only even though their stems begin with `tree`.
pub(crate) fn is_live_tree_asset(path: &str) -> bool {
    let Some(stem) = Path::new(path).file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    let stem = stem.to_ascii_lowercase();
    let tokens: Vec<_> = stem
        .split(|character: char| character == '_' || character == '-' || character.is_whitespace())
        .filter(|token| !token.is_empty())
        .collect();
    if tokens
        .iter()
        .any(|token| matches!(*token, "log" | "stump" | "grass" | "fern" | "stone"))
    {
        return false;
    }
    tokens
        .first()
        .is_some_and(|token| matches!(*token, "pine" | "tree" | "birch" | "spruce" | "fir"))
}

#[derive(Clone, Copy, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
enum State {
    Standing,
    /// Released toward `dir`; elapsed time is a receipt, not a prescribed fall angle.
    Falling {
        t: f32,
        dir: Vec3,
    },
    Down,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DTree {
    batch: usize, // index into self.batches (NOT the renderer index)
    inst: usize,  // instance index within that batch
    base: Mat4,   // ORIGINAL authored transform; retained source identity for saves
    pos: Vec3,    // base pivot (ground contact — worldgen grounds tree origins)
    height: f32,  // world height (mesh height × instance scale)
    trunk_r: f32,
    big: bool,
    push_work: f32, // work (J) the bow has put into bending the trunk (tank_push)
    state: State,
    seed: u32,
    /// A standing tree's sway after a pressure front: the crown's displacement (m, world xz)
    /// and its velocity, a damped spring at the tree's first mode (`pressure_front`).
    #[serde(default)]
    lean: glam::Vec2,
    #[serde(default)]
    lean_v: glam::Vec2,
}

/// A world event the game routes to FX/audio (the subsystem stays deaf and mute itself).
pub enum TreeEvent {
    /// A tree just snapped/gave way at `pos` (crack + needle/snow puff).
    Snap { pos: Vec3 },
    /// A released tree just hit the ground (`pos` is an actual source contact).
    Thud { pos: Vec3 },
}

/// Outcome of driving the bow into trees this frame (game applies speed/feel).
pub struct CrushContact {
    pub speed: Option<f32>, // the tank's signed speed after pressing on the trunks it met (None: no contact)
    pub rumble: f32,            // crush-shudder energy to feed the camera
}

pub struct Trees {
    device: wgpu::Device,
    /// (renderer batch index, CPU-side instance transforms). Rewritten in place as trees move.
    batches: Vec<(usize, Vec<Mat4>)>,
    trees: Vec<DTree>,
    /// batches touched this frame (rebuilt once in `update`).
    dirty: Vec<bool>,
    // Immutable source geometry is rebuilt with each batch, not duplicated in world saves.
    support: Vec<Arc<ground_support::Source>>,
    // Separate optional save packet preserves the existing instance/identity record layout.
    released: std::collections::BTreeMap<usize,ground_support::Released>,
    // Detail base nodes are immutable; these are its actual ordinary mutation revisions.
    // Coarse-only editable grids have no revision contract and never reuse this certificate.
    support_revision: Option<(usize,u64,u64)>,
    support_openings: Vec<crate::terrain_opening::TerrainOpening>,
}

/// The trunk's diameter (mesh units) where the bow meets it: the extent of the mesh's section by
/// the horizontal plane `y`, among the section's segments within `reach` of the tree's axis (the
/// trunk, not a low branch), the narrower of its two horizontal spans. None for a mesh with no
/// section there.
fn trunk_section_width(mesh: &CpuMesh, y: f32, reach: f32) -> Option<f32> {
    let (mut lo, mut hi) = (glam::Vec2::splat(f32::INFINITY), glam::Vec2::splat(f32::NEG_INFINITY));
    for tri in mesh.indices.chunks_exact(3) {
        let p = [0, 1, 2].map(|k| Vec3::from(mesh.vertices[tri[k] as usize].pos));
        for (a, b) in [(p[0], p[1]), (p[1], p[2]), (p[2], p[0])] {
            if (a.y - y) * (b.y - y) > 0.0 || (a.y - b.y).abs() < 1.0e-6 {
                continue;
            }
            let t = (y - a.y) / (b.y - a.y);
            let q = glam::Vec2::new(a.x + (b.x - a.x) * t, a.z + (b.z - a.z) * t);
            if q.length() <= reach {
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
    }
    (lo.x <= hi.x).then(|| (hi.x - lo.x).min(hi.y - lo.y).max(0.0))
}

fn page_tree_records(batch: usize, mesh: &CpuMesh, mats: &[Mat4]) -> Vec<DTree> {
    let mesh_h = (mesh.max.y - mesh.min.y).max(0.1);
    mats.iter()
        .enumerate()
        .map(|(inst, model)| {
            let (scale, _, pos) = model.to_scale_rotation_translation();
            let height = mesh_h * scale.y;
            let horizontal = scale.x.max(scale.z);
            // The trunk the bow meets, measured from the drawn mesh at the bow's height (the
            // former contact radius was 0.3 m times the scale whatever the model: a 0.6 m trunk on
            // a stripped sapling).
            let contact_y = mesh.min.y + crate::tank_push::BOW_CONTACT_H_M / scale.y.max(0.05);
            let trunk_d = trunk_section_width(mesh, contact_y.min(mesh.max.y - 0.01), 1.0 / horizontal.max(0.05))
                .map(|w| w * horizontal)
                .filter(|d| *d > 0.02)
                .unwrap_or(2.0 * TRUNK_R * horizontal);
            DTree {
                batch,
                inst,
                base: *model,
                pos,
                height,
                trunk_r: 0.5 * trunk_d,
                big: height >= BIG_TREE_HEIGHT,
                push_work: 0.0,
                state: State::Standing,
                seed: 0x7EE_0000 ^ ((batch as u32) << 12) ^ inst as u32,
                lean: glam::Vec2::ZERO,
                lean_v: glam::Vec2::ZERO,
            }
        })
        .collect()
}

/// Existing one-hit/blast felling supplies no transferred momentum. Use an explicit estimate:
/// five percent of the already modeled failure/bending work remains as release kinetic work.
/// Source contact initialization checks that against its actual COM-over-root energy barrier.
fn release_work_per_mass(diameter:f32,height:f32,absorbed_work_j:f32)->f32 {
    let mass=crate::tank_push::STEM_DENSITY*crate::tank_push::STEM_FORM
        *std::f32::consts::FRAC_PI_4*diameter.max(0.01).powi(2)*height.max(0.5);
    let failure=crate::tank_push::standing_tree(diameter,height).work_j;
    0.05*absorbed_work_j.max(failure)/mass
}

/// Nearest ray hit against a slice of live tree records. Keeping the geometry here (rather than in
/// a gun-specific caller) makes the same standing-trunk contract available to player rounds, enemy
/// rounds and optical LOS. Falling/down trunks intentionally leave this vertical-cylinder query as
/// soon as their hinge starts; their rendered low obstacle remains drive-over scenery.
fn ray_standing_trunks(trees: &[DTree], o: Vec3, d: Vec3, max_t: f32) -> Option<(f32, usize)> {
    let mut best: Option<(f32, usize)> = None;
    for (i, t) in trees.iter().enumerate() {
        if t.state != State::Standing {
            continue;
        }
        // 2D ray-circle in the ground plane.
        let (ox, oz) = (o.x - t.pos.x, o.z - t.pos.z);
        let (dx, dz) = (d.x, d.z);
        let a = dx * dx + dz * dz;
        let r = t.trunk_r + 0.12; // a little forgiveness — the canopy counts a bit
        let (tt, hit) = if a < 1e-8 {
            (0.0, ox * ox + oz * oz <= r * r) // straight-down ray: inside or not
        } else {
            let b = ox * dx + oz * dz;
            let c = ox * ox + oz * oz - r * r;
            let disc = b * b - a * c;
            if disc < 0.0 {
                (0.0, false)
            } else {
                ((-b - disc.sqrt()) / a, true)
            }
        };
        if !hit || tt < 0.0 || tt > max_t {
            continue;
        }
        let y = o.y + d.y * tt;
        if y < t.pos.y || y > t.pos.y + t.height {
            continue;
        }
        if best.map_or(true, |(bt, _)| tt < bt) {
            best = Some((tt, i));
        }
    }
    best
}

fn standing_trunks_block_segment(trees: &[DTree], from: Vec3, to: Vec3) -> bool {
    let delta = to - from;
    let distance = delta.length();
    if distance < 1.0e-3 {
        return false;
    }
    ray_standing_trunks(trees, from, delta / distance, distance)
        .is_some_and(|(t, _)| t < distance - LOS_TARGET_SLOP)
}

impl Trees {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            batches: Vec::new(),
            trees: Vec::new(),
            dirty: Vec::new(),
            support: Vec::new(),
            released: Default::default(),
            support_revision: None,
            support_openings: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.trees.len()
    }

    /// Register a tree-asset instance batch: uploads it to the renderer's instanced path AND
    /// records every instance as a destructible tree. Call during `Game::build` only.
    pub fn register_batch(
        &mut self,
        r: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
        mats: Vec<Mat4>,
    ) {
        if mats.is_empty() {
            return;
        }
        let prototype = r.register_instanced_prototype(device, queue, mesh);
        self.register_page(r, device, prototype, mesh, mats);
    }

    /// Register one spatial page against a prototype already uploaded by the manifest loader.
    /// Tree gameplay state remains page-local exactly as before; only immutable GPU geometry,
    /// material textures and BLAS are shared with sibling pages of the same GLB.
    pub fn register_page(
        &mut self,
        r: &mut Renderer,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        mesh: &CpuMesh,
        mats: Vec<Mat4>,
    ) {
        if mats.is_empty() {
            return;
        }
        let expected = r.instanced_stats().0; // stable index of the page appended below
        let Some(rbatch) = r.add_instanced_page(device, prototype, &mats) else {
            // A stale/invalid prototype must not leave gameplay tree records pointing at the next
            // unrelated renderer batch in release builds.
            debug_assert!(false, "tree page references an invalid renderer prototype");
            return;
        };
        debug_assert_eq!(rbatch, expected);
        let bi = self.batches.len();
        self.trees.extend(page_tree_records(bi, mesh, &mats));
        self.batches.push((rbatch, mats));
        self.dirty.push(false);
        self.support.push(Arc::new(ground_support::Source::new(mesh)));
    }

    /// Register all initial spatial pages for one tree prototype as a single packed renderer load
    /// group. Gameplay keeps an owned transform vector and independent local instance indices per
    /// page; only the GPU instance storage is shared.
    pub fn register_pages(
        &mut self,
        r: &mut Renderer,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        mesh: &CpuMesh,
        pages: &[&[Mat4]],
    ) -> bool {
        let Some(renderer_batches) = r.add_instanced_pages(device, prototype, pages) else {
            return false;
        };
        if renderer_batches.len() != pages.len() {
            return false;
        }
        let support = Arc::new(ground_support::Source::new(mesh));
        for (&rbatch, page) in renderer_batches.iter().zip(pages) {
            let bi = self.batches.len();
            self.trees.extend(page_tree_records(bi, mesh, page));
            self.batches.push((rbatch, page.to_vec()));
            self.dirty.push(false);
            self.support.push(support.clone());
        }
        true
    }

    /// Ray vs every STANDING trunk (vertical cylinder from base to height): nearest hit as
    /// (entry distance, tree index). Shared by projectile and optical-LOS paths; O(trees) like
    /// `ray_destructible`.
    pub fn ray(&self, o: Vec3, d: Vec3, max_t: f32) -> Option<(f32, usize)> {
        ray_standing_trunks(&self.trees, o, d, max_t)
    }

    /// Whether a live vertical trunk interrupts an optical segment. This intentionally shares the
    /// exact standing-tree cylinder query with projectiles; `LOS_TARGET_SLOP` only prevents cover
    /// touching the target endpoint from masking the target itself.
    pub fn blocks_segment(&self, from: Vec3, to: Vec3) -> bool {
        standing_trunks_block_segment(&self.trees, from, to)
    }

    /// A shell landed on tree `idx`: snap it along the shot direction. (Slice 3 refines this —
    /// AP will pass through a big pine with a splinter puff; today any main-gun hit fells it.)
    pub fn shot(&mut self, idx: usize, dir: Vec3, events: &mut Vec<TreeEvent>) {
        if self
            .trees
            .get(idx)
            .is_some_and(|tree| tree.state == State::Standing)
        {
            self.snap(idx, dir, events);
        }
    }

    /// Blast at `pos` with `radius`: snap every standing tree in range, falling outward.
    /// (The HE path — wired in slice 3, present now so the API is complete.)
    pub fn blast(&mut self, pos: Vec3, radius: f32, events: &mut Vec<TreeEvent>) {
        for i in 0..self.trees.len() {
            let t = &self.trees[i];
            if t.state != State::Standing {
                continue;
            }
            let to = t.pos - pos;
            if to.length_squared() < radius * radius {
                let dir = Vec3::new(to.x, 0.0, to.z).normalize_or_zero();
                let dir = if dir.length_squared() < 0.5 {
                    Vec3::Z
                } else {
                    dir
                };
                self.snap(i, dir, events);
            }
        }
    }

    /// The Tiger's bow is at `nose` heading `dir` with signed `speed` (m/s) on a slope `pitch`:
    /// every standing tree it meets resists by the weaker of its stem's bending strength and its
    /// root plate's anchorage (`tank_push::standing_tree`, the trunk measured from the drawn mesh,
    /// the height from the placed model) and falls once the bow has done the work to break or tip
    /// it; a sapling or a stripped dead tree costs the tank almost nothing, and only a big old
    /// tree (about 0.7 m across and 12 m tall) holds a crawling Tiger, never one that arrives at
    /// a few metres a second. Returns the tank's speed
    /// after the frame when a trunk was met.
    pub fn crush(
        &mut self,
        nose: Vec3,
        dir: Vec3,
        speed: f32,
        pitch: f32,
        nose_r: f32,
        dt: f32,
        events: &mut Vec<TreeEvent>,
    ) -> CrushContact {
        let mut out = CrushContact {
            speed: None,
            rumble: 0.0,
        };
        let mut v = speed;
        for i in 0..self.trees.len() {
            let t = &self.trees[i];
            if t.state != State::Standing {
                continue;
            }
            let (dx, dz) = (nose.x - t.pos.x, nose.z - t.pos.z);
            let reach = nose_r + t.trunk_r;
            if dx * dx + dz * dz > reach * reach {
                continue;
            }
            let resist = crate::tank_push::standing_tree(2.0 * t.trunk_r, t.height);
            let step = crate::tank_push::press(v, pitch, resist, t.push_work, dt);
            out.rumble=out.rumble.max(crate::tank_push::collision_rumble(v,step.speed));
            v = step.speed;
            self.trees[i].push_work += step.work_j;
            out.speed = Some(v);
            if step.gave_way {
                self.snap(i, dir, events);
            }
        }
        out
    }

    fn snap(&mut self, idx: usize, dir: Vec3, events: &mut Vec<TreeEvent>) {
        let d = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let d = if d.length_squared() < 0.5 { Vec3::Z } else { d };
        let t = &mut self.trees[idx];
        t.state = State::Falling { t: 0.0, dir: d };
        self.released.insert(idx,ground_support::Released::new(self.batches[t.batch].1[t.inst],d,0.0));
        events.push(TreeEvent::Snap { pos: t.pos });
        self.dirty[t.batch] = true;
    }

    /// Advance falls and rewrite the touched instance buffers (only moving batches pay).
    pub fn update(&mut self, dt: f32, r: &mut Renderer, terrain: &crate::terrain::Heightfield,
        events: &mut Vec<TreeEvent>) {
        let revision=terrain.detail.as_ref().map(|detail|(Arc::as_ptr(detail) as usize,
            detail.crater_revision(),detail.rut_revision()));
        let recheck_rest=revision.is_none()||revision!=self.support_revision
            ||terrain.openings.list()!=self.support_openings.as_slice();
        if recheck_rest {
            self.support_revision=revision;
            self.support_openings=terrain.openings.list().to_vec();
        }
        for i in 0..self.trees.len() {
            if self.trees[i].state==State::Down && recheck_rest {
                let tree=&mut self.trees[i];
                let motion=self.released.entry(i).or_insert_with(||ground_support::Released::new(
                    self.batches[tree.batch].1[tree.inst],Vec3::ZERO,1.0));
                if self.support[tree.batch].advance(motion,0.0,Vec3::ZERO,0.0,|x,z|terrain.height_at(x,z))
                    .is_some_and(|step|!step.sleeping) {
                    // No new release kick: gravity and the changed actual support wake it.
                    tree.state=State::Falling {t:motion.motion.age,dir:Vec3::ZERO};
                }
            }
            let State::Falling { t: ft, dir } = self.trees[i].state else {
                continue;
            };
            let nt = ft + dt.max(0.0).min(0.05);
            let tree = &mut self.trees[i];
            let release_work=release_work_per_mass(2.0*tree.trunk_r,tree.height,tree.push_work);
            // Rebuilt explicitly for pre-rigid-motion saves; new saves always carry the full
            // velocities/rotation/contact age and continue those exact ordinary solver inputs.
            let motion=self.released.entry(i).or_insert_with(||ground_support::Released::new(
                self.batches[tree.batch].1[tree.inst],dir,ft));
            let Some(step)=self.support[tree.batch].advance(motion,dt.max(0.0).min(0.05),dir,release_work,
                |x,z|terrain.height_at(x,z)) else {continue;};
            let pose=step.model;
            tree.state=if step.sleeping {State::Down}else{State::Falling {t:nt,dir}};
            if let Some((pos,_impact))=step.hit {events.push(TreeEvent::Thud {pos});}
            let (bi, ii) = (tree.batch, tree.inst);
            self.batches[bi].1[ii] = pose;
            self.dirty[bi] = true;
        }
        // Standing trees swaying after a front: a damped spring at the first mode (damping
        // ratio 0.15, the foliage's aerodynamic damping), the pose a rotation of the authored
        // transform about the base toward the crown's displacement; restored once settled.
        for i in 0..self.trees.len() {
            let tree = &mut self.trees[i];
            if tree.state != State::Standing
                || (tree.lean.length_squared() < 1e-8 && tree.lean_v.length_squared() < 1e-8) { continue; }
            let omega = std::f32::consts::TAU * crate::pressure_front::tree_sway_hz(tree.height);
            let h = dt.clamp(0.0, 0.05);
            tree.lean_v += (-omega * omega * tree.lean - 2.0 * 0.15 * omega * tree.lean_v) * h;
            tree.lean += tree.lean_v * h;
            let reach = (tree.height * 0.7).max(0.5);
            tree.lean = tree.lean.clamp_length_max(reach * 0.35);
            let settled = tree.lean.length() < 0.002 && tree.lean_v.length() < 0.01;
            let pose = if settled {
                tree.lean = glam::Vec2::ZERO; tree.lean_v = glam::Vec2::ZERO;
                tree.base
            } else {
                let angle = tree.lean.length() / reach;
                let axis = Vec3::new(tree.lean.y, 0.0, -tree.lean.x).normalize_or_zero();
                Mat4::from_translation(tree.pos) * Mat4::from_axis_angle(axis, angle)
                    * Mat4::from_translation(-tree.pos) * tree.base
            };
            let (bi, ii) = (tree.batch, tree.inst);
            self.batches[bi].1[ii] = pose;
            self.dirty[bi] = true;
        }
        for bi in 0..self.batches.len() {
            if self.dirty[bi] {
                let (rbatch, ref mats) = self.batches[bi];
                r.set_instanced_mats(&self.device, rbatch, mats);
                self.dirty[bi] = false;
            }
        }
    }

    /// The fronts' arrivals in (t0, t1] at each standing tree's crown: the crown takes the drag
    /// impulse radially from the source (`pressure_front::crown_tip_velocity_m_s`); `update`
    /// rings the sway down. A tree inside the burst's damage radius is snapped by `blast`.
    pub fn pressure_sweep(&mut self, fronts: &[crate::pressure_front::Front], t0: f32, t1: f32) -> f32 {
        let mut peak = 0.0f32;
        if fronts.is_empty() { return peak; }
        for tree in &mut self.trees {
            if tree.state != State::Standing { continue; }
            let crown = tree.pos + Vec3::Y * tree.height * 0.7;
            for front in fronts {
                let Some(arrival) = front.arrival_at(crown) else { continue };
                if arrival.at <= t0 || arrival.at > t1 { continue; }
                let v = crate::pressure_front::crown_tip_velocity_m_s(
                    front.tnt_kg, crown.distance(front.origin), tree.height, tree.trunk_r);
                let radial = glam::Vec2::new(arrival.direction.x, arrival.direction.z).normalize_or_zero();
                tree.lean_v += radial * v;
                peak = peak.max(v);
            }
        }
        peak
    }

    /// Class of tree `idx` — the pen ray charges a mature trunk more budget than a fir.
    pub fn is_big(&self, idx: usize) -> bool {
        self.trees.get(idx).map_or(false, |t| t.big)
    }

    /// Nearest STANDING tree of the requested class to `p` (headless demo teleport helper).
    pub fn debug_nearest_of(&self, p: Vec3, big: bool) -> Option<Vec3> {
        self.trees
            .iter()
            .filter(|t| t.state == State::Standing && t.big == big)
            .min_by(|a, b| {
                a.pos
                    .distance_squared(p)
                    .partial_cmp(&b.pos.distance_squared(p))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|t| t.pos)
    }

    /// Nearest standing tree to `p` (demo/teleport helper).
    pub fn debug_nearest(&self, p: Vec3) -> Option<(Vec3, bool)> {
        self.trees
            .iter()
            .filter(|t| t.state == State::Standing)
            .min_by(|a, b| {
                a.pos
                    .distance_squared(p)
                    .partial_cmp(&b.pos.distance_squared(p))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|t| (t.pos, t.big))
    }

    /// How many trees are down (headless assertions).
    pub fn debug_down_count(&self) -> usize {
        self.trees.iter().filter(|t| t.state == State::Down).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_tree_classifier_uses_the_stem_not_the_nature_directory() {
        for path in [
            "assets/pizza/Models/Trees, Rocks & Nature/pine_tree_n_1.glb",
            "assets/pizza/Models/Trees, Rocks & Nature/tree_8.glb",
            "assets/forest/Birch_A.GLB",
            "assets/forest/spruce-01.glb",
            "assets/forest/fir_03.glb",
        ] {
            assert!(is_live_tree_asset(path), "expected live tree: {path}");
        }

        for path in [
            "assets/pizza/Models/Trees, Rocks & Nature/stone_3.glb",
            "assets/pizza/Models/Trees, Rocks & Nature/fern_1.glb",
            "assets/pizza/Models/Trees, Rocks & Nature/grass_2.glb",
            "assets/pizza/Models/Trees, Rocks & Nature/tree_log_1.glb",
            "assets/pizza/Models/Trees, Rocks & Nature/tree_stump_1.glb",
            "assets/pizza/Models/Trees, Rocks & Nature/wall_hr_1.glb",
            "assets/props/treehouse.glb",
        ] {
            assert!(
                !is_live_tree_asset(path),
                "expected non-tree dressing: {path}"
            );
        }
    }

    #[test]
    fn hinge_axis_tips_the_top_toward_the_fall_direction() {
        // Rotating about up×dir must move a point ABOVE the pivot toward dir.
        let dir = Vec3::new(0.0, 0.0, 1.0);
        let axis = Vec3::Y.cross(dir).normalize();
        let top = Vec3::new(0.0, 5.0, 0.0);
        let rot = Quat::from_axis_angle(axis, 1.4);
        let moved = rot * top;
        assert!(
            moved.z > 3.0,
            "the treetop must tip toward +Z, got {moved:?}"
        );
        assert!(moved.y < top.y, "the treetop must come DOWN, got {moved:?}");
        // And the pivot itself must not move: hinge = T(p) R T(-p).
        let p = Vec3::new(7.0, 1.0, -3.0);
        let hinge = Mat4::from_translation(p) * Mat4::from_quat(rot) * Mat4::from_translation(-p);
        assert!(
            hinge.transform_point3(p).distance(p) < 1e-4,
            "base pivot must stay planted"
        );
    }

    #[test]
    fn tree_class_splits_on_height() {
        assert!(4.0 < BIG_TREE_HEIGHT, "a 4 m fir must classify small");
        assert!(9.0 > BIG_TREE_HEIGHT, "a 9 m pine must classify big");
    }

    #[test]
    fn shared_prototype_pages_keep_independent_tree_instance_ranges() {
        let mesh = CpuMesh::basic(
            Vec::new(),
            Vec::new(),
            None,
            Vec3::new(-0.5, 0.0, -0.5),
            Vec3::new(0.5, 9.0, 0.5),
        );
        let first_mats = [
            Mat4::from_translation(Vec3::new(5.0, 0.0, 7.0)),
            Mat4::from_translation(Vec3::new(9.0, 0.0, 7.0)),
        ];
        let second_mats = [Mat4::from_translation(Vec3::new(260.0, 0.0, 7.0))];
        let mut first = page_tree_records(3, &mesh, &first_mats);
        let second = page_tree_records(4, &mesh, &second_mats);

        assert_eq!((first[0].batch, first[0].inst), (3, 0));
        assert_eq!((first[1].batch, first[1].inst), (3, 1));
        assert_eq!((second[0].batch, second[0].inst), (4, 0));
        assert_ne!(first[0].seed, second[0].seed);
        first[0].state = State::Down;
        assert!(first[0].state == State::Down);
        assert!(first[1].state == State::Standing);
        assert!(second[0].state == State::Standing);
    }

    #[test]
    fn standing_trunk_blocks_rays_but_falling_and_down_trunks_do_not() {
        let mesh = CpuMesh::basic(
            Vec::new(),
            Vec::new(),
            None,
            Vec3::new(-0.5, 0.0, -0.5),
            Vec3::new(0.5, 9.0, 0.5),
        );
        let mats = [Mat4::from_translation(Vec3::new(0.0, 0.0, 10.0))];
        let mut trees = page_tree_records(0, &mesh, &mats);
        let origin = Vec3::new(0.0, 1.5, 0.0);
        let dir = Vec3::Z;

        let (distance, index) =
            ray_standing_trunks(&trees, origin, dir, 20.0).expect("standing trunk blocks the ray");
        assert_eq!(index, 0);
        assert!((9.0..10.0).contains(&distance));
        assert!(
            standing_trunks_block_segment(&trees, origin, origin + dir * 20.0),
            "the optical segment must use the same live-trunk hit as projectiles"
        );

        trees[0].state = State::Falling {
            t: 0.1,
            dir: Vec3::X,
        };
        assert!(ray_standing_trunks(&trees, origin, dir, 20.0).is_none());
        assert!(!standing_trunks_block_segment(
            &trees,
            origin,
            origin + dir * 20.0
        ));

        trees[0].state = State::Down;
        assert!(ray_standing_trunks(&trees, origin, dir, 20.0).is_none());
        assert!(!standing_trunks_block_segment(
            &trees,
            origin,
            origin + dir * 20.0
        ));
    }

    #[test]
    fn trunk_ray_respects_height_and_nearest_live_tree() {
        let mesh = CpuMesh::basic(
            Vec::new(),
            Vec::new(),
            None,
            Vec3::new(-0.5, 0.0, -0.5),
            Vec3::new(0.5, 5.0, 0.5),
        );
        let mats = [
            Mat4::from_translation(Vec3::new(0.0, 0.0, 12.0)),
            Mat4::from_translation(Vec3::new(0.0, 0.0, 7.0)),
        ];
        let trees = page_tree_records(0, &mesh, &mats);

        let hit = ray_standing_trunks(&trees, Vec3::new(0.0, 2.0, 0.0), Vec3::Z, 30.0)
            .expect("ray through trunk height should hit");
        assert_eq!(
            hit.1, 1,
            "nearest standing trunk wins for every weapon path"
        );
        assert!(
            ray_standing_trunks(&trees, Vec3::new(0.0, 6.0, 0.0), Vec3::Z, 30.0).is_none(),
            "a ray above the authored trunk height remains clear"
        );
    }
}

impl Trees {
    pub(crate) fn export_world_state(&self)->anyhow::Result<crate::expedition_world_save::StateFields> {
        let mut state=crate::capture_world_fields!(self; batches, trees, dirty);
        state.put("released_motion_v1",&self.released)?;
        Ok(state)
    }
    pub(crate) fn import_world_state(&mut self,mut state:crate::expedition_world_save::StateFields)->anyhow::Result<()> {
        let shape=self.batches.iter().map(|(i,m)|(*i,m.len())).collect::<Vec<_>>();
        let has_motion=state.keys().any(|key|key=="released_motion_v1");
        let saved_motion=if has_motion {Some(state.take::<std::collections::BTreeMap<usize,ground_support::Released>>("released_motion_v1")?)}else{None};
        crate::restore_world_fields!(self,state; batches, trees, dirty);
        anyhow::ensure!(shape==self.batches.iter().map(|(i,m)|(*i,m.len())).collect::<Vec<_>>(),"source instance layout changed");
        anyhow::ensure!(self.batches.iter().all(|(_,m)|m.iter().all(|v|v.is_finite())),"nonfinite source instance pose");
        anyhow::ensure!(self.trees.iter().all(|t|t.batch<self.batches.len()&&t.inst<self.batches[t.batch].1.len()
            &&t.base.is_finite()&&t.pos.is_finite()&&t.height.is_finite()),"invalid source tree identity");
        self.released=if let Some(motion)=saved_motion {
            anyhow::ensure!(motion.iter().all(|(i,m)|*i<self.trees.len()&&self.trees[*i].state!=State::Standing&&m.valid()),"invalid released tree motion");
            anyhow::ensure!(self.trees.iter().enumerate().all(|(i,t)|!matches!(t.state,State::Falling{..})||motion.contains_key(&i)),"falling tree motion missing");
            motion
        }else{
            // Explicit compatibility migration: old saves had only elapsed scripted hinge time
            // and the uploaded pose. Retain that visible pose; there was no velocity to restore.
            self.trees.iter().enumerate().filter_map(|(i,t)|match t.state {
                State::Falling{t:elapsed,dir}=>Some((i,ground_support::Released::new(self.batches[t.batch].1[t.inst],dir,elapsed))),
                _=>None,
            }).collect()
        };
        self.support_revision=None;
        self.support_openings.clear();
        Ok(())
    }
    pub(crate) fn present_world_state(&self,r:&mut Renderer) {
        for (batch,mats) in &self.batches {r.set_instanced_mats(&self.device,*batch,mats);}
    }
}
