//! Chunk-grid walls + ballistic debris (docs/DESTRUCTION.md §6, slice 4).
//!
//! Every CHUNKABLE destructible (a thin, axis-aligned standing wall — the city walls and the
//! demo wall; rotated GLB shells keep the plain swap-collapse) pre-allocates a brick-course grid
//! of jittered chunk meshes at build, hidden behind the intact mesh. The first hole ACTIVATES the
//! set: intact swaps out, chunks swap in, and from then on damage is spatial —
//!   - an AP transit removes chunks in a CYLINDER around the shot line (a hole you can see),
//!   - an HE burst removes chunks in a SPHERE and flings them radially,
//!   - a chunk with no support below (its under-neighbour died) drops after a hashed beat, so
//!     shooting out a wall's base CASCADES the courses above it down,
//!   - a full collapse (HP zero — shells or the track-crush grind) flings whatever still stands.
//! Dead chunks fly a dumb ballistic arc (gravity, hashed tumble), bounce once off the
//! heightfield, and FREEZE forever where they settle — persistent rubble, no physics crate, no
//! chunk-vs-chunk collision (invisible through the PS1 crush anyway). Deterministic: all jitter
//! is counter-hashed off the wall's seed (no rand).

use glam::{Mat4, Quat, Vec3};

use crate::gltf_load::CpuMesh;
use crate::renderer::Renderer;
use crate::terrain::Heightfield;

const CHUNK_TARGET_W: f32 = 1.05; // m — nominal brick-course chunk width
const CHUNK_TARGET_H: f32 = 0.85; // m — nominal course height
const MAX_COLS: usize = 8;
const MAX_ROWS: usize = 6;
const HOLE_R: f32 = 0.62; // m — AP hole radius through the grid
const SUPPORT_DELAY: f32 = 0.08; // s — base beat before an unsupported chunk lets go
const FREEZE_SPEED: f32 = 1.1; // m/s — a bounced chunk slower than this settles for good

fn hash01(n: u32) -> f32 {
    let h = n.wrapping_mul(2654435761) >> 8;
    (h & 0xffff) as f32 / 65535.0
}

#[derive(Clone, Copy)]
enum ChunkState {
    Hidden,       // set not yet activated — the intact wall still draws
    Solid,        // standing masonry (drawn at its base transform)
    Falling(f32), // lost support; lets go when the timer runs out
    Flying {
        vel: Vec3,
        axis: Vec3,
        spin: f32,
        ang: f32,
        bounced: bool,
    },
    Frozen, // settled rubble — drawn at its final transform forever
}

struct Chunk {
    slot: usize,  // renderer mesh slot (pre-allocated, hidden at build)
    base: Vec3,   // BUILD-time center — the box mesh's verts are baked here; never changes
    center: Vec3, // CURRENT center (displaced while flying / settled)
    half: Vec3,
    col: usize,
    row: usize,
    state: ChunkState,
    seed: u32,
}

struct WallSet {
    di: usize, // destructible index this set shadows
    intact_slot: usize,
    cols: usize,
    chunks: Vec<Chunk>,
    active: bool,
}

#[derive(Default)]
pub struct ChunkWalls {
    sets: Vec<WallSet>,
    /// destructible index → set index (sparse; not every destructible is chunkable).
    by_di: std::collections::HashMap<usize, usize>,
}

/// Is this wall AABB the thin axis-aligned slab the chunk grid can honestly fill? Rotated GLB
/// shells present fat diagonal AABBs and keep the plain swap-collapse instead.
pub fn chunkable(min: Vec3, max: Vec3) -> bool {
    let (wx, wz) = (max.x - min.x, max.z - min.z);
    wx.min(wz) < 1.7 && wx.max(wz) > 1.6 && (max.y - min.y) > 1.6
}

impl ChunkWalls {
    /// Build-time: generate the chunk grid for destructible `di`, pushing one hidden box mesh per
    /// chunk into `scene` (must run BEFORE the tank/FX tail — same rule as the rubble heaps).
    pub fn register(
        &mut self,
        di: usize,
        intact_slot: usize,
        min: Vec3,
        max: Vec3,
        seed: u32,
        scene: &mut Vec<CpuMesh>,
    ) {
        let (wx, wz) = (max.x - min.x, max.z - min.z);
        let along_x = wx >= wz; // long horizontal axis carries the courses
        let long = if along_x { wx } else { wz };
        let height = max.y - min.y;
        let cols = ((long / CHUNK_TARGET_W).round() as usize).clamp(2, MAX_COLS);
        let rows = ((height / CHUNK_TARGET_H).round() as usize).clamp(2, MAX_ROWS);
        let (cw, ch) = (long / cols as f32, height / rows as f32);
        let mut chunks = Vec::with_capacity(cols * rows);
        for row in 0..rows {
            for col in 0..cols {
                let s = seed ^ ((row as u32) << 9) ^ ((col as u32) << 3) ^ 0xC4A8_0000;
                // Brick-course offset: odd rows shift half a chunk (clamped to the wall).
                let stagger = if row % 2 == 1 { 0.5 * cw } else { 0.0 };
                let a0 = (col as f32 + 0.5) * cw + stagger;
                let a = a0.min(long - 0.5 * cw).max(0.5 * cw);
                let center = if along_x {
                    Vec3::new(
                        min.x + a,
                        min.y + (row as f32 + 0.5) * ch,
                        (min.z + max.z) * 0.5,
                    )
                } else {
                    Vec3::new(
                        (min.x + max.x) * 0.5,
                        min.y + (row as f32 + 0.5) * ch,
                        min.z + a,
                    )
                };
                let jw = 0.88 + 0.24 * hash01(s);
                let jh = 0.88 + 0.24 * hash01(s.wrapping_add(7));
                let half = if along_x {
                    Vec3::new(0.5 * cw * jw, 0.5 * ch * jh, 0.5 * wz.min(1.7) * 0.92)
                } else {
                    Vec3::new(0.5 * wx.min(1.7) * 0.92, 0.5 * ch * jh, 0.5 * cw * jw)
                };
                // Cold masonry, tint-jittered per chunk so the grid reads as coursed brick.
                let t = 0.85 + 0.3 * hash01(s.wrapping_add(13));
                let tint = [(108.0 * t) as u8, (98.0 * t) as u8, (86.0 * t) as u8];
                let slot = scene.len();
                scene.push(crate::prims::box_mesh(center - half, center + half, tint));
                chunks.push(Chunk {
                    slot,
                    base: center,
                    center,
                    half,
                    col,
                    row,
                    state: ChunkState::Hidden,
                    seed: s,
                });
            }
        }
        self.by_di.insert(di, self.sets.len());
        self.sets.push(WallSet {
            di,
            intact_slot,
            cols,
            chunks,
            active: false,
        });
    }

    pub fn chunk_count(&self) -> usize {
        self.sets.iter().map(|s| s.chunks.len()).sum()
    }

    fn activate(set: &mut WallSet, r: &mut Renderer) {
        if set.active {
            return;
        }
        set.active = true;
        r.set_visible(set.intact_slot, false);
        for c in &mut set.chunks {
            if matches!(c.state, ChunkState::Hidden) {
                c.state = ChunkState::Solid;
                r.set_visible(c.slot, true);
            }
        }
    }

    /// An AP round punched through destructible `di` at `entry` along `dir`: open a visible hole —
    /// kill the chunks within `HOLE_R` of the shot line and send them flying with the round.
    pub fn hole(&mut self, di: usize, entry: Vec3, dir: Vec3, r: &mut Renderer) {
        let Some(&si) = self.by_di.get(&di) else {
            return;
        };
        let set = &mut self.sets[si];
        Self::activate(set, r);
        for c in &mut set.chunks {
            if !matches!(c.state, ChunkState::Solid) {
                continue;
            }
            // Distance from chunk center to the shot line.
            let to = c.center - entry;
            let d_line = (to - dir * to.dot(dir)).length();
            if d_line < HOLE_R + c.half.length() * 0.35 {
                let s = c.seed;
                let spread = Vec3::new(hash01(s) - 0.5, hash01(s ^ 5) * 0.7, hash01(s ^ 9) - 0.5);
                c.state = ChunkState::Flying {
                    vel: dir * (7.0 + 4.0 * hash01(s ^ 3)) + spread * 3.0,
                    axis: Vec3::new(
                        hash01(s ^ 11) - 0.5,
                        hash01(s ^ 13) - 0.5,
                        hash01(s ^ 17) - 0.5,
                    )
                    .normalize_or_zero(),
                    spin: 3.0 + 7.0 * hash01(s ^ 19),
                    ang: 0.0,
                    bounced: false,
                };
            }
        }
    }

    /// An HE burst at `bp`: kill chunks of `di` within `radius`, flinging them radially outward.
    pub fn blast(&mut self, di: usize, bp: Vec3, radius: f32, r: &mut Renderer) {
        let Some(&si) = self.by_di.get(&di) else {
            return;
        };
        let set = &mut self.sets[si];
        Self::activate(set, r);
        for c in &mut set.chunks {
            if !matches!(c.state, ChunkState::Solid) {
                continue;
            }
            let d = c.center.distance(bp);
            if d < radius {
                let s = c.seed;
                let out = (c.center - bp).normalize_or_zero();
                let power = (1.0 - d / radius).max(0.15);
                c.state = ChunkState::Flying {
                    vel: (out + Vec3::Y * 0.6).normalize_or_zero() * (11.0 * power + 2.0)
                        + Vec3::new(hash01(s) - 0.5, 0.0, hash01(s ^ 5) - 0.5) * 2.0,
                    axis: Vec3::new(
                        hash01(s ^ 11) - 0.5,
                        hash01(s ^ 13) - 0.5,
                        hash01(s ^ 17) - 0.5,
                    )
                    .normalize_or_zero(),
                    spin: 4.0 + 8.0 * hash01(s ^ 19),
                    ang: 0.0,
                    bounced: false,
                };
            }
        }
    }

    /// Full structural collapse of `di` (HP zero — shellfire or the crush grind): everything still
    /// standing tumbles outward-and-down, low.
    pub fn collapse(&mut self, di: usize, r: &mut Renderer) {
        let Some(&si) = self.by_di.get(&di) else {
            return;
        };
        let set = &mut self.sets[si];
        Self::activate(set, r);
        let mid: Vec3 = {
            let n = set.chunks.len().max(1) as f32;
            set.chunks
                .iter()
                .map(|c| c.center)
                .fold(Vec3::ZERO, |a, b| a + b)
                / n
        };
        for c in &mut set.chunks {
            if !matches!(c.state, ChunkState::Solid | ChunkState::Falling(_)) {
                continue;
            }
            let s = c.seed;
            let out = Vec3::new(c.center.x - mid.x, 0.0, c.center.z - mid.z).normalize_or_zero();
            let out = if out.length_squared() < 0.5 {
                Vec3::new(hash01(s) - 0.5, 0.0, hash01(s ^ 3) - 0.5).normalize_or_zero()
            } else {
                out
            };
            c.state = ChunkState::Flying {
                vel: out * (2.0 + 3.0 * hash01(s ^ 23)) + Vec3::Y * (0.4 + 1.2 * hash01(s ^ 29)),
                axis: Vec3::new(
                    hash01(s ^ 11) - 0.5,
                    hash01(s ^ 13) - 0.5,
                    hash01(s ^ 17) - 0.5,
                )
                .normalize_or_zero(),
                spin: 2.0 + 6.0 * hash01(s ^ 19),
                ang: 0.0,
                bounced: false,
            };
        }
    }

    /// Advance support cascades and ballistic chunks; write transforms for everything that moved.
    pub fn update(&mut self, dt: f32, r: &mut Renderer, hf: &Heightfield) {
        for set in &mut self.sets {
            if !set.active {
                continue;
            }
            // ── Support pass: a Solid chunk whose under-neighbour is gone lets go after a beat.
            // (Row 0 sits on the ground and never auto-falls.) One column-index lookup per chunk.
            let cols = set.cols;
            let dead_below: Vec<bool> = set
                .chunks
                .iter()
                .map(|c| {
                    if c.row == 0 {
                        return false;
                    }
                    let below = (c.row - 1) * cols + c.col;
                    set.chunks.get(below).map_or(false, |b| {
                        !matches!(
                            b.state,
                            ChunkState::Solid | ChunkState::Hidden | ChunkState::Falling(_)
                        )
                    })
                })
                .collect();
            for (i, c) in set.chunks.iter_mut().enumerate() {
                if dead_below[i] && matches!(c.state, ChunkState::Solid) {
                    c.state = ChunkState::Falling(SUPPORT_DELAY + 0.25 * hash01(c.seed ^ 31));
                }
            }
            // ── Motion pass.
            for c in &mut set.chunks {
                match c.state {
                    ChunkState::Falling(t) => {
                        let t = t - dt;
                        if t <= 0.0 {
                            c.state = ChunkState::Flying {
                                vel: Vec3::new(0.0, -0.4, 0.0),
                                axis: Vec3::new(
                                    hash01(c.seed) - 0.5,
                                    0.2,
                                    hash01(c.seed ^ 7) - 0.5,
                                )
                                .normalize_or_zero(),
                                spin: 1.5 + 3.0 * hash01(c.seed ^ 41),
                                ang: 0.0,
                                bounced: false,
                            };
                        } else {
                            c.state = ChunkState::Falling(t);
                        }
                    }
                    ChunkState::Flying {
                        mut vel,
                        axis,
                        spin,
                        mut ang,
                        mut bounced,
                    } => {
                        vel.y -= 9.81 * dt;
                        c.center += vel * dt;
                        ang += spin * dt;
                        let ground = hf.height_at(c.center.x, c.center.z);
                        if c.center.y - c.half.y * 0.7 <= ground {
                            c.center.y = ground + c.half.y * 0.7;
                            if bounced || vel.length() < FREEZE_SPEED {
                                // Settled: sink a hair, freeze forever — persistent rubble.
                                c.center.y = ground + c.half.y * 0.45;
                                c.state = ChunkState::Frozen;
                                r.set_model(c.slot, chunk_model(c, axis, ang));
                                continue;
                            }
                            vel.y = -vel.y * 0.25;
                            vel.x *= 0.45;
                            vel.z *= 0.45;
                            bounced = true;
                        }
                        c.state = ChunkState::Flying {
                            vel,
                            axis,
                            spin,
                            ang,
                            bounced,
                        };
                        r.set_model(c.slot, chunk_model(c, axis, ang));
                    }
                    _ => {}
                }
            }
        }
    }

    /// Headless assertions: (solid, flying, frozen) chunk counts for destructible `di`.
    pub fn debug_counts(&self, di: usize) -> (usize, usize, usize) {
        let Some(&si) = self.by_di.get(&di) else {
            return (0, 0, 0);
        };
        let mut out = (0, 0, 0);
        for c in &self.sets[si].chunks {
            match c.state {
                ChunkState::Solid | ChunkState::Hidden => out.0 += 1,
                ChunkState::Flying { .. } | ChunkState::Falling(_) => out.1 += 1,
                ChunkState::Frozen => out.2 += 1,
            }
        }
        out
    }
}

/// A chunk's draw transform: the box mesh is baked in world space at `base`, so drawing at the
/// current center with tumble = translate to `center`, spin about the chunk's own pivot, and
/// unbake `base`.
fn chunk_model(c: &Chunk, axis: Vec3, ang: f32) -> Mat4 {
    Mat4::from_translation(c.center)
        * Mat4::from_quat(Quat::from_axis_angle(axis, ang))
        * Mat4::from_translation(-c.base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_walls_chunk_fat_shells_do_not() {
        assert!(
            chunkable(Vec3::new(-3.0, 0.0, -0.4), Vec3::new(3.0, 4.2, 0.4)),
            "the demo wall must be chunkable"
        );
        assert!(
            !chunkable(Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 4.0, 2.0)),
            "a fat diagonal AABB must keep swap-collapse"
        );
        assert!(
            !chunkable(Vec3::new(-3.0, 0.0, -0.4), Vec3::new(3.0, 1.0, 0.4)),
            "a knee-high wall is not worth a grid"
        );
    }
}
