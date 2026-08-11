//! Destructible trees (docs/DESTRUCTION.md §5, slice 2). A falling tree is ONE hinge rotation
//! about its base pivot — no fracture, no solver. Small firs snap under the tracks or any shell;
//! big pines fall to a shell or a heavy tank's sustained shove (the Tiger pushes them over at a
//! crawl; lighter vehicles are blocked). A fallen trunk stays down forever as a low obstacle the
//! tank simply drives over.
//!
//! Trees live in the renderer's INSTANCED path (they arrive by the thousands from worldgen /
//! the act manifests), so this subsystem owns the CPU-side transform lists for every tree batch
//! and rewrites a batch's instance buffer only on the frames a tree in it is actually moving —
//! the same device-clone + `set_instanced_mats` pattern the grass system already uses. All
//! randomness is counter-hashed off the tree index (no rand crate).

use glam::{Mat4, Quat, Vec3};

use crate::gltf_load::CpuMesh;
use crate::renderer::{InstancedPrototype, Renderer};

/// Trunk contact radius scales with the instance's footprint; height classifies small fir vs
/// big pine (the crush tiers differ — DESTRUCTION.md §2).
const BIG_TREE_HEIGHT: f32 = 6.5; // m — world height at or above this = BigTree (tier-3 crush)
const TRUNK_R: f32 = 0.30; // m — base trunk contact/ray radius at scale 1
const SHOVE_TIME: f32 = 2.0; // s — sustained Tiger push before a big pine gives
const FALL_END_MIN: f32 = 1.40; // rad — fall comes to rest ~80-86°, hashed per tree
const FALL_END_VAR: f32 = 0.10;

#[derive(Clone, Copy, PartialEq)]
enum State {
    Standing,
    /// Falling toward `dir` — `t` runs 0..dur, then the tree is Down.
    Falling {
        t: f32,
        dir: Vec3,
    },
    Down,
}

struct DTree {
    batch: usize, // index into self.batches (NOT the renderer index)
    inst: usize,  // instance index within that batch
    base: Mat4,   // ORIGINAL authored transform — the hinge composes off this every frame
    pos: Vec3,    // base pivot (ground contact — worldgen grounds tree origins)
    height: f32,  // world height (mesh height × instance scale)
    trunk_r: f32,
    big: bool,
    push_t: f32, // accumulated Tiger shove on a big pine
    state: State,
    seed: u32,
}

/// A world event the game routes to FX/audio (the subsystem stays deaf and mute itself).
pub enum TreeEvent {
    /// A tree just snapped/gave way at `pos` (crack + needle/snow puff).
    Snap { pos: Vec3 },
    /// A falling tree just hit the ground (`pos` = base; the thud).
    Thud { pos: Vec3 },
}

/// Outcome of driving the bow into trees this frame (game applies speed/feel).
pub struct CrushContact {
    pub speed_cap: Option<f32>, // clamp |speed| to this while pushing (None = no restriction)
    pub rumble: f32,            // crush-shudder energy to feed the camera
}

pub struct Trees {
    device: wgpu::Device,
    /// (renderer batch index, CPU-side instance transforms). Rewritten in place as trees move.
    batches: Vec<(usize, Vec<Mat4>)>,
    trees: Vec<DTree>,
    /// batches touched this frame (rebuilt once in `update`).
    dirty: Vec<bool>,
}

fn hash01(n: u32) -> f32 {
    let h = n.wrapping_mul(2654435761) >> 8;
    (h & 0xffff) as f32 / 65535.0
}

fn page_tree_records(batch: usize, mesh: &CpuMesh, mats: &[Mat4]) -> Vec<DTree> {
    let mesh_h = (mesh.max.y - mesh.min.y).max(0.1);
    mats.iter()
        .enumerate()
        .map(|(inst, model)| {
            let (scale, _, pos) = model.to_scale_rotation_translation();
            let height = mesh_h * scale.y;
            DTree {
                batch,
                inst,
                base: *model,
                pos,
                height,
                trunk_r: TRUNK_R * scale.x.max(scale.z),
                big: height >= BIG_TREE_HEIGHT,
                push_t: 0.0,
                state: State::Standing,
                seed: 0x7EE_0000 ^ ((batch as u32) << 12) ^ inst as u32,
            }
        })
        .collect()
}

impl Trees {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            batches: Vec::new(),
            trees: Vec::new(),
            dirty: Vec::new(),
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
        for (&rbatch, page) in renderer_batches.iter().zip(pages) {
            let bi = self.batches.len();
            self.trees.extend(page_tree_records(bi, mesh, page));
            self.batches.push((rbatch, page.to_vec()));
            self.dirty.push(false);
        }
        true
    }

    /// Ray vs every STANDING trunk (vertical cylinder from base to height): nearest hit as
    /// (entry distance, tree index). Used by the gun path; O(trees) like `ray_destructible`.
    pub fn ray(&self, o: Vec3, d: Vec3, max_t: f32) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        for (i, t) in self.trees.iter().enumerate() {
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

    /// A shell landed on tree `idx`: snap it along the shot direction. (Slice 3 refines this —
    /// AP will pass through a big pine with a splinter puff; today any main-gun hit fells it.)
    pub fn shot(&mut self, idx: usize, dir: Vec3, events: &mut Vec<TreeEvent>) {
        self.snap(idx, dir, events);
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

    /// The bow is at `nose` heading `dir` with `speed` (m/s, absolute) and vehicle `crush_power`
    /// (Tiger 3, T-34 2). Small firs snap and barely slow the tank; big pines take a sustained
    /// tier-3 shove from a power-3 vehicle and hard-block anything lighter.
    pub fn crush(
        &mut self,
        nose: Vec3,
        dir: Vec3,
        speed: f32,
        crush_power: u8,
        nose_r: f32,
        dt: f32,
        events: &mut Vec<TreeEvent>,
    ) -> CrushContact {
        let mut out = CrushContact {
            speed_cap: None,
            rumble: 0.0,
        };
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
            if !t.big {
                // Fir: snaps on contact at any real speed, ploughed forward.
                if speed > 0.5 {
                    self.snap(i, dir, events);
                    out.rumble = out.rumble.max(0.5);
                }
                continue;
            }
            if crush_power >= 3 {
                // The Tiger's shove: crawl against the trunk until it gives.
                out.speed_cap = Some(1.3);
                out.rumble = out.rumble.max(0.55);
                if speed > 0.4 {
                    self.trees[i].push_t += dt;
                    if self.trees[i].push_t >= SHOVE_TIME {
                        self.snap(i, dir, events);
                    }
                }
            } else {
                out.speed_cap = Some(0.0); // lighter vehicle: a big pine is a wall
                out.rumble = out.rumble.max(0.3);
            }
        }
        out
    }

    fn snap(&mut self, idx: usize, dir: Vec3, events: &mut Vec<TreeEvent>) {
        let d = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let d = if d.length_squared() < 0.5 { Vec3::Z } else { d };
        let t = &mut self.trees[idx];
        t.state = State::Falling { t: 0.0, dir: d };
        events.push(TreeEvent::Snap { pos: t.pos });
        self.dirty[t.batch] = true;
    }

    /// Advance falls and rewrite the touched instance buffers (only moving batches pay).
    pub fn update(&mut self, dt: f32, r: &mut Renderer, events: &mut Vec<TreeEvent>) {
        for i in 0..self.trees.len() {
            let (dur, end) = {
                let t = &self.trees[i];
                (
                    if t.big { 1.5 } else { 0.9 },
                    FALL_END_MIN + FALL_END_VAR * hash01(t.seed),
                )
            };
            let State::Falling { t: ft, dir } = self.trees[i].state else {
                continue;
            };
            let nt = ft + dt;
            let done = nt >= dur;
            let k = (nt / dur).clamp(0.0, 1.0);
            // Gravity-flavoured ease-in: slow lean, accelerating collapse.
            let ang = end * k * k * (2.0 - k);
            let tree = &mut self.trees[i];
            tree.state = if done {
                State::Down
            } else {
                State::Falling { t: nt, dir }
            };
            if done {
                events.push(TreeEvent::Thud {
                    pos: tree.pos + dir * tree.height * 0.6,
                });
            }
            let final_ang = if done { end } else { ang };
            let (bi, ii, pivot, base) = (tree.batch, tree.inst, tree.pos, tree.base);
            // Hinge about the horizontal axis through the base: axis = up × fall-dir. Composes
            // off the ORIGINAL transform every frame — the batch list only holds what's uploaded.
            let axis = Vec3::Y.cross(dir).normalize_or_zero();
            let hinge = Mat4::from_translation(pivot)
                * Mat4::from_quat(Quat::from_axis_angle(axis, final_ang))
                * Mat4::from_translation(-pivot);
            self.batches[bi].1[ii] = hinge * base;
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
}
