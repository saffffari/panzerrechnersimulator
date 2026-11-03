//! Drawing the casualties' voxel bodies (`crew_tissue`) in the solo corpse renderer, and moving
//! the grains that come away from them. A dead man with a solved body is drawn as his cells, each
//! cell its own closed cube (one instanced page per colour), never as the figure's triangles or the
//! skeleton page; only cells with a face open to air or to a wound are submitted. Cells near a joint
//! are drawn between the two segments' poses so a bent joint stays closed (`TissueModel::blend`).
//! The pieces that came away fly, break into single grains on a hard landing, and come to rest on
//! the tank's surfaces and on grains already lying there. A man's steel helmet is no cells: it is
//! a rigid body (`crew_helmet_body`), drawn on his head until it comes away. Every page is
//! allocated here at build, before any casualty, so the retained draw layout that saves record
//! never changes shape.
use super::*;
use super::crew_helmet_body::{HelmetBody, HelmetShape};
use super::crew_tissue::{self, CasualtyTissue, ChunkBody, Strike, TissueModel};

const APPEARANCES: usize = 5;
const PARTS: usize = crew_physics::CREW_BODY_PART_COUNT;
/// Cubes per colour page; more are dropped with one report (a named bound).
const CUBES_PER_LABEL: usize = 8_192;

/// Review only (`fea-range`'s camera section, world normal and distance): a body's inside cells
/// within a cell and a half of that plane are drawn too, so a section through a body shows its
/// cells cut solid rather than the inside of its drawn shell. None everywhere else: the game draws
/// the cells open to air or to a wound only.
static REVIEW_SECTION: std::sync::RwLock<Option<glam::Vec4>> = std::sync::RwLock::new(None);

pub(crate) fn set_review_section(plane: Option<glam::Vec4>) {
    if let Ok(mut section) = REVIEW_SECTION.write() {
        *section = plane;
    }
}

pub(crate) fn review_section_set() -> bool {
    review_section().is_some()
}

fn review_section() -> Option<glam::Vec4> {
    REVIEW_SECTION.read().ok().and_then(|section| *section)
}
/// Cells in pieces kept at once; past it the oldest pieces at rest go (a named bound; 1 cm
/// anatomical cells are eight to a 2 cm cell).
const CHUNK_CELLS: usize = 32_768;
/// Muscle albedo (sRGB) and roughness: two shades of dark red, authored game-level colours (not
/// measured), wet enough to take a highlight.
const MUSCLE: [[u8; 3]; 2] = [[118, 26, 22], [96, 18, 16]];
const MUSCLE_ROUGHNESS: f32 = 0.42;
/// Grains at rest are found by a hash of this size (m).
const REST_HASH_M: f32 = 0.04;

struct Actor {
    peer: u32,
    tissue: CasualtyTissue,
    /// (tissue revision, face) the cached cells were listed for, and the cells drawn by part.
    cache: Option<(u32, usize, [Vec<(u32, u8)>; PARTS])>,
    /// His helmet once it came away from his head (on it while `None`).
    helmet: Option<HelmetBody>,
}

/// What `TissueDraw` simulates (its drawing caches aside): each struck body's cells and bonds and
/// loose helmet, and the pieces in flight or at rest.
#[derive(Clone)]
pub(super) struct TissueState {
    pub actors: Vec<(u32, std::sync::Arc<CasualtyTissue>, Option<HelmetBody>)>,
    pub pieces: Vec<ChunkBody>,
}

/// A body's solve started on a worker (`TissueDraw::begin_strike`), finished in order by
/// `TissueDraw::finish_strike`.
pub(super) struct StrikeJob {
    pub(super) peer: u32,
    strikes: usize,
    helmet_impulse: Vec3,
    helmet_at: Vec3,
    started: std::time::Instant,
    handle: std::thread::JoinHandle<(CasualtyTissue, crew_tissue::Solve)>,
}

impl StrikeJob {
    pub(super) fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
}

pub(super) struct TissueDraw {
    model: std::sync::Arc<TissueModel>,
    helmet: HelmetShape,
    cube_pages: [usize; crew_tissue::LABELS],
    /// Skin colour label per face and cell.
    skin_labels: Vec<Vec<u8>>,
    actors: Vec<Actor>,
    pieces: Vec<ChunkBody>,
    cube_lists: [Vec<Mat4>; crew_tissue::LABELS],
    overflow_reported: bool,
}

fn texel(image: &crate::gltf_load::CpuImage, uv: Vec2) -> [u8; 3] {
    let x = ((uv.x.rem_euclid(1.0) * image.width as f32) as u32).min(image.width - 1);
    let y = ((uv.y.rem_euclid(1.0) * image.height as f32) as u32).min(image.height - 1);
    let at = ((y * image.width + x) * 4) as usize;
    [image.rgba[at], image.rgba[at + 1], image.rgba[at + 2]]
}

/// A small deterministic k-means palette (sRGB) over the skin cells' colours.
fn palette(samples: &[[u8; 3]], k: usize) -> Vec<[f32; 3]> {
    let mut sorted: Vec<[f32; 3]> = samples.iter().map(|c| c.map(f32::from)).collect();
    sorted.sort_by(|a, b| (a[0] + a[1] + a[2]).total_cmp(&(b[0] + b[1] + b[2])));
    if sorted.is_empty() {
        return vec![[128.0; 3]; k];
    }
    let mut centres: Vec<[f32; 3]> = (0..k).map(|i| sorted[(i * 2 + 1) * sorted.len() / (2 * k)]).collect();
    for _ in 0..12 {
        let mut sums = vec![([0.0f32; 3], 0usize); k];
        for c in &sorted {
            let n = nearest(&centres, *c);
            for a in 0..3 {
                sums[n].0[a] += c[a];
            }
            sums[n].1 += 1;
        }
        for (centre, (sum, count)) in centres.iter_mut().zip(sums) {
            if count > 0 {
                *centre = sum.map(|s| s / count as f32);
            }
        }
    }
    centres
}

fn nearest(centres: &[[f32; 3]], c: [f32; 3]) -> usize {
    (0..centres.len()).min_by(|&a, &b| {
        let d = |i: usize| (0..3).map(|n| (centres[i][n] - c[n]).powi(2)).sum::<f32>();
        d(a).total_cmp(&d(b))
    }).unwrap_or(0)
}

/// A unit cube of one colour: neighbouring cells of one colour blend into one surface (the owner,
/// 2026-11-02: no outlines).
fn cell_cube(colour: [u8; 3], roughness: f32) -> CpuMesh {
    let mut mesh = box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), colour);
    mesh.roughness_factor = roughness;
    mesh.metallic_factor = 0.0;
    mesh
}

fn hash01(n: u32) -> f32 {
    let h = n.wrapping_mul(2_654_435_761) ^ (n >> 15).wrapping_mul(2_246_822_519);
    (h >> 8) as f32 / (1u32 << 24) as f32
}

impl TissueDraw {
    /// `figures` are the five relaxed faces (one body, five atlases); `bones` the fitted skeleton
    /// pages, voxelised onto the same grid; `helmet` the steel helmet (figure frame), a rigid body.
    pub(super) fn allocate(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer, figures: &[CpuMesh],
        bones: &[Option<CpuMesh>; PARTS], helmet: &CpuMesh) -> anyhow::Result<Self> {
        anyhow::ensure!(figures.len() == APPEARANCES, "five relaxed faces expected");
        let start = std::time::Instant::now();
        let model = std::sync::Arc::new(crew_tissue::build_tissue(&figures[0], bones));
        let cells_ms = start.elapsed().as_secs_f32() * 1000.0;
        let atlases: Vec<&crate::gltf_load::CpuImage> = figures.iter()
            .map(|f| f.texture.as_ref().ok_or_else(|| anyhow::anyhow!("relaxed face without atlas")))
            .collect::<anyhow::Result<_>>()?;
        // The skin cells' colours, from each face's atlas at the cell's nearest skin point.
        let skin = |c: &&crew_tissue::Cell| c.outer();
        let samples: Vec<[u8; 3]> = atlases.iter()
            .flat_map(|atlas| model.cells.iter().filter(skin).map(|c| texel(atlas, c.uv)))
            .collect();
        let colours = palette(&samples, crew_tissue::SKIN_LABELS as usize);
        let skin_labels: Vec<Vec<u8>> = atlases.iter().map(|atlas|
            model.cells.iter().map(|c| nearest(&colours, texel(atlas, c.uv).map(f32::from)) as u8 + 1).collect()).collect();
        // Bone: the fitted skeleton's own albedo, averaged over its pages' vertices.
        let bone_colour = {
            let mut sum = [0.0f32; 3];
            let mut count = 0.0f32;
            for page in bones.iter().flatten() {
                if let Some(image) = &page.texture {
                    for v in &page.vertices {
                        let c = texel(image, Vec2::from(v.uv));
                        for a in 0..3 {
                            sum[a] += f32::from(c[a]);
                        }
                        count += 1.0;
                    }
                }
            }
            if count > 0.0 { sum.map(|s| (s / count) as u8) } else { [178, 171, 145] }
        };
        let bone_roughness = bones.iter().flatten().next().map_or(0.72, |page| page.roughness_factor);
        // A page per colour: the figure's, muscle's two, bone's, and each other anatomical tissue's
        // table colour (wet tissue's roughness; authored), only for the tissues the body holds.
        let mut cube_pages = [usize::MAX; crew_tissue::LABELS];
        for (index, page) in cube_pages.iter_mut().enumerate() {
            let label = index as u8 + 1;
            let (colour, roughness) = if label <= crew_tissue::SKIN_LABELS {
                (colours[index].map(|c| c.round().clamp(0.0, 255.0) as u8), figures[0].roughness_factor)
            } else if label == crew_tissue::BONE_LABEL {
                (bone_colour, bone_roughness)
            } else if label < crew_tissue::BONE_LABEL {
                (MUSCLE[(label - crew_tissue::MUSCLE_LABELS[0]) as usize], MUSCLE_ROUGHNESS)
            } else {
                match model.tissue_colours.iter().find(|(id, _)| *id == label - crew_tissue::BONE_LABEL) {
                    Some(&(_, colour)) => (colour, MUSCLE_ROUGHNESS),
                    None => continue,
                }
            };
            let prototype = r.register_instanced_prototype(device, queue, &cell_cube(colour, roughness));
            *page = r.add_moving_instanced_pool(device, prototype, CUBES_PER_LABEL)
                .ok_or_else(|| anyhow::anyhow!("tissue cube page"))?;
        }
        let count = |t| model.cells.iter().filter(|c| c.tissue == t).count();
        println!("[crew] voxel bodies: {} cells of {:.0} mm (skin-class {}, muscle-class {}, bone {}, fluid {}; {}) in {cells_ms:.0} ms; {} pages; the helmet a rigid body",
            model.cell_count(), model.cell * 1000.0, count(crew_tissue::Tissue::Skin), count(crew_tissue::Tissue::Muscle),
            count(crew_tissue::Tissue::Bone), count(crew_tissue::Tissue::Fluid),
            model.anatomy.as_deref().map_or("the figure and its fitted skeleton".to_string(), |a| format!("anatomy {a}")),
            cube_pages.iter().filter(|p| **p != usize::MAX).count());
        Ok(Self { model, helmet: HelmetShape::new(helmet), cube_pages, skin_labels, actors: Vec::new(), pieces: Vec::new(),
            cube_lists: std::array::from_fn(|_| Vec::new()), overflow_reported: false })
    }

    pub(super) fn reset(&mut self) {
        self.actors.clear();
        self.pieces.clear();
    }

    /// The struck bodies' simulated state, for a review checkpoint (`fea-range`). A body whose
    /// cells and bonds are unchanged since `previous` shares that checkpoint's copy.
    pub(super) fn state(&self, previous: Option<&TissueState>) -> TissueState {
        let actors = self.actors.iter().map(|a| {
            let shared = previous.and_then(|p| p.actors.iter().find(|(peer, _, _)| *peer == a.peer))
                .map(|(_, tissue, _)| tissue)
                .filter(|t| t.revision == a.tissue.revision && t.present == a.tissue.present && t.broken == a.tissue.broken);
            let tissue = shared.cloned().unwrap_or_else(|| std::sync::Arc::new(a.tissue.clone()));
            (a.peer, tissue, a.helmet.clone())
        }).collect();
        TissueState { actors, pieces: self.pieces.clone() }
    }

    /// Put the bodies back as `state` holds them; the drawing's cell lists are rebuilt.
    pub(super) fn restore(&mut self, state: &TissueState) {
        self.actors = state.actors.iter().map(|(peer, tissue, helmet)|
            Actor { peer: *peer, tissue: (**tissue).clone(), cache: None, helmet: helmet.clone() }).collect();
        self.pieces = state.pieces.clone();
    }

    /// Solve the strikes that killed the ragdoll's man and throw what came away: the share of
    /// each part's soft cells that remain. His helmet, while on his head, meets the strikes first;
    /// it comes away when its momentum against the head tears the strap or his head is mostly gone.
    pub(super) fn strike(&mut self, ragdoll: &CrewRagdoll, strikes: &[Strike]) -> [f32; PARTS] {
        let job = self.begin_strike(ragdoll, strikes);
        self.finish_strike(job, ragdoll)
    }

    /// Start a body's solve on a worker: his helmet meets the strikes first, then his cells' own
    /// state goes to the worker until `finish_strike` takes it back (no other strike on him may
    /// come between: the caller finishes his earlier job first).
    pub(super) fn begin_strike(&mut self, ragdoll: &CrewRagdoll, strikes: &[Strike]) -> StrikeJob {
        let peer = ragdoll.casualty.peer_id;
        let index = match self.actors.iter().position(|a| a.peer == peer) {
            Some(index) => index,
            None => {
                self.actors.push(Actor { peer, tissue: CasualtyTissue::new(&self.model), cache: None, helmet: None });
                self.actors.len() - 1
            }
        };
        let actor = &self.actors[index];
        let head_segment = ragdoll.segments[crew_physics::CrewBodyPart::Head.index()];
        let (strikes, helmet_impulse, helmet_at) = if actor.helmet.is_none() {
            self.helmet.through_shell(strikes, head_segment.rot.conjugate() * head_segment.vel)
        } else {
            (strikes.to_vec(), Vec3::ZERO, self.helmet.com)
        };
        let mut tissue = actor.tissue.clone();
        let model = self.model.clone();
        let count = strikes.len();
        let handle = std::thread::spawn(move || {
            let solve = tissue.solve(&model, &strikes);
            (tissue, solve)
        });
        StrikeJob { peer, strikes: count, helmet_impulse, helmet_at, started: std::time::Instant::now(), handle }
    }

    /// Take a body's solve back (waiting for it if it is still running) and throw what came away.
    pub(super) fn finish_strike(&mut self, job: StrikeJob, ragdoll: &CrewRagdoll) -> [f32; PARTS] {
        let model = self.model.clone();
        let peer = job.peer;
        let (tissue, solve) = job.handle.join().expect("a body's tissue solve");
        let Some(actor) = self.actors.iter_mut().find(|a| a.peer == peer) else { return [1.0; PARTS] };
        actor.tissue = tissue;
        let head = crew_physics::CrewBodyPart::Head;
        let torn: usize = solve.chunks.iter().map(|c| c.cells.len()).sum();
        let bone: usize = solve.chunks.iter().flat_map(|c| c.cells.iter())
            .filter(|&&c| model.cells[c as usize].tissue == crew_tissue::Tissue::Bone).count();
        let single = solve.chunks.iter().filter(|c| c.cells.len() == 1).count();
        let largest = solve.chunks.iter().map(|c| c.cells.len()).max().unwrap_or(0);
        let fastest = solve.chunks.iter().map(|c| c.velocity.length()).fold(0.0f32, f32::max);
        println!("[crew] body of actor {peer} solved on its cells: {} strikes, {} bonds broken, {} crushed in the track, {torn} cells torn away ({bone} bone) in {} pieces ({single} single, largest {largest}, fastest {fastest:.0} m/s), {} fragments stopped in him, {} steps in {:.0} ms",
            job.strikes, solve.bonds_broken, solve.crushed, solve.chunks.len(), solve.lodged, solve.steps, job.started.elapsed().as_secs_f32() * 1000.0);
        let shares: [f32; PARTS] = std::array::from_fn(|i| actor.tissue.soft_share(&model, crew_physics::CrewBodyPart::ALL[i]));
        if actor.helmet.is_none() && (HelmetBody::strap_tears(job.helmet_impulse) || shares[head.index()] < crew_tissue::KEEP_SOFT_SHARE) {
            let body = HelmetBody::release(&self.helmet, ragdoll, job.helmet_impulse, job.helmet_at);
            println!("[crew] helmet of actor {peer} came away at {:.1} m/s ({:.2} N·s against the head, head soft share {:.2})",
                body.vel.length(), job.helmet_impulse.length(), shares[head.index()]);
            actor.helmet = Some(body);
        }
        self.pieces.extend(crew_tissue::chunk_bodies(&model, ragdoll, solve.chunks));
        self.bound_pieces();
        shares
    }

    /// The oldest pieces at rest go first, then the oldest in flight, past `CHUNK_CELLS`.
    fn bound_pieces(&mut self) {
        let mut cells: usize = self.pieces.iter().map(|p| p.cells.len()).sum();
        while cells > CHUNK_CELLS && !self.pieces.is_empty() {
            let oldest = self.pieces.iter().position(|p| p.settled).unwrap_or(0);
            cells -= self.pieces[oldest].cells.len();
            self.pieces.remove(oldest);
        }
    }

    pub(super) fn has_body(&self, peer: u32) -> bool {
        self.actors.iter().any(|a| a.peer == peer)
    }

    /// Where a helmet that came away from its man is now (world); `None` while it is on his head.
    pub(super) fn loose_helmet_model(&self, peer: u32, player: &Tank) -> Option<Mat4> {
        let helmet = self.actors.iter().find(|a| a.peer == peer)?.helmet.as_ref()?;
        Some(blood_parent_model(player, helmet.frame) * helmet.local_model(&self.helmet))
    }

    /// Advance the pieces by `dt`: gravity; the vehicle's own surfaces (`FragmentWorld`) and the
    /// grains already at rest, each met by a sphere of the piece's bounding radius about its
    /// centre, and the vehicle's surfaces also by the piece's outermost cells (the one furthest
    /// along each of 14 directions, each swept with its turn and its half edge ahead; named: not
    /// every cell), with no rotational response to a contact (named). A piece
    /// that lands harder than its own bonds hold (half its mass times the square of its speed
    /// into the surface, against the breaking energy of its intact bonds) breaks into single
    /// grains, each leaving with the piece's motion at its own place. A piece rests on a face
    /// turned up (or on grains) when slow or past its bounces; 12 m from its frame's origin it has
    /// left the vehicle and stops being moved.
    pub(super) fn integrate(&mut self, world: &FragmentWorld, player: &Tank, dt: f32) {
        if !(dt > 0.0) {
            return;
        }
        for helmet in self.actors.iter_mut().filter_map(|a| a.helmet.as_mut()).filter(|h| !h.settled) {
            let frame = helmet.frame;
            let gravity = blood_parent_model(player, frame).inverse().transform_vector3(Vec3::new(0.0, -9.81, 0.0));
            helmet.advance(&self.helmet, dt, gravity, &|from, to| world.first_surface(frame, from, to));
        }
        if self.pieces.iter().all(|p| p.settled) {
            return;
        }
        let model = self.model.clone();
        let key = |frame: BloodFrame, p: Vec3| {
            let g = (p / REST_HASH_M).floor().as_ivec3();
            (matches!(frame, BloodFrame::Turret) as u8, [g.x, g.y, g.z])
        };
        let mut resting: std::collections::HashMap<(u8, [i32; 3]), Vec<(Vec3, f32)>> = std::collections::HashMap::new();
        for piece in self.pieces.iter().filter(|p| p.settled) {
            let m = piece.local_model();
            for &c in &piece.cells {
                let p = m.transform_point3(model.cells[c as usize].at);
                resting.entry(key(piece.frame, p)).or_default().push((p, 0.5 * model.cell * piece.scale));
            }
        }
        let mut born: Vec<ChunkBody> = Vec::new();
        let mut broken: Vec<usize> = Vec::new();
        for (n, piece) in self.pieces.iter_mut().enumerate().filter(|(_, p)| !p.settled) {
            let parent_inverse = blood_parent_model(player, piece.frame).inverse();
            let gravity = parent_inverse.transform_vector3(Vec3::new(0.0, -9.81, 0.0));
            let up = parent_inverse.transform_vector3(Vec3::Y).normalize_or_zero();
            let half_cell = 0.5 * model.cell * piece.scale;
            let outer = outer_cells(&model, piece);
            let steps = (piece.vel.length() * dt / 0.03).ceil().clamp(1.0, 48.0) as usize;
            let h = dt / steps as f32;
            for _ in 0..steps {
                piece.vel += gravity * h;
                let travel = piece.vel * h;
                let length = travel.length();
                let ahead = travel + travel.normalize_or_zero() * piece.radius;
                let mut contact = world.first_surface(piece.frame, piece.pos, piece.pos + ahead)
                    .map(|(fraction, normal)| (fraction * ahead.length(), normal));
                // The outer cells, from this pose to the next (its turn included), each with its
                // half edge ahead; a hit is put as the centre's distance (the sphere's convention).
                let turned = (Quat::from_scaled_axis(piece.spin * h) * piece.rot).normalize();
                for &offset in &outer {
                    let from = piece.pos + piece.rot * offset;
                    let to = piece.pos + travel + turned * offset;
                    let path = to - from;
                    let span = path.length();
                    if span <= 1.0e-7 {
                        continue;
                    }
                    let end = to + path / span * half_cell;
                    if let Some((fraction, normal)) = world.first_surface(piece.frame, from, end) {
                        let reach = fraction * (span + half_cell) - half_cell;
                        let distance = reach / span * length + piece.radius;
                        if contact.is_none_or(|(d, _)| distance < d) {
                            let normal = if normal.dot(path) > 0.0 { -normal } else { normal };
                            contact = Some((distance, normal));
                        }
                    }
                }
                // Grains at rest along the way: the nearest whose sphere the piece's would enter.
                if length > 0.0 {
                    let dir = travel / length;
                    let samples = (length / REST_HASH_M).ceil() as usize + 1;
                    let mut seen = std::collections::HashSet::new();
                    for k in 0..=samples {
                        let p = piece.pos + dir * (length * k as f32 / samples as f32);
                        let g = key(piece.frame, p);
                        for dz in -1..=1 {
                            for dy in -1..=1 {
                                for dx in -1..=1 {
                                    let cellkey = (g.0, [g.1[0] + dx, g.1[1] + dy, g.1[2] + dz]);
                                    if !seen.insert(cellkey) {
                                        continue;
                                    }
                                    for &(q, r) in resting.get(&cellkey).into_iter().flatten() {
                                        let reach = piece.radius + r;
                                        let to = q - piece.pos;
                                        let along = to.dot(dir);
                                        let off2 = to.length_squared() - along * along;
                                        if along < 0.0 || off2 > reach * reach {
                                            continue;
                                        }
                                        let at = along - (reach * reach - off2).sqrt();
                                        if at <= length && contact.is_none_or(|(d, _)| at < d) {
                                            let hit = piece.pos + dir * at.max(0.0);
                                            contact = Some((at.max(0.0), (hit - q).normalize_or_zero()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                match contact {
                    Some((distance, normal)) if normal != Vec3::ZERO => {
                        let dir = travel.normalize_or_zero();
                        piece.pos += dir * (distance - piece.radius).max(0.0).min(length);
                        let vn = piece.vel.dot(normal);
                        if vn < 0.0 {
                            let mass: f32 = piece.cells.iter().map(|&c| model.mass(c as usize)).sum();
                            piece.vel = (piece.vel - normal * vn) * crew_tissue::PIECE_TANGENT_KEEP
                                - normal * (vn * crew_tissue::PIECE_RESTITUTION);
                            if piece.cells.len() > 1
                                && 0.5 * mass * vn * vn > piece.cohesion_j * crew_tissue::tune().grain * crew_tissue::tissue_tuning().landing_breakup {
                                broken.push(n);
                                break;
                            }
                        }
                        piece.spin *= 0.5;
                        piece.bounces = piece.bounces.saturating_add(1);
                        if normal.dot(up) > 0.5
                            && (piece.vel.length() < crew_tissue::PIECE_REST_MPS || piece.bounces >= crew_tissue::PIECE_BOUNCES) {
                            piece.vel = Vec3::ZERO;
                            piece.spin = Vec3::ZERO;
                            piece.settled = true;
                            break;
                        }
                    }
                    _ => piece.pos += travel,
                }
                piece.rot = (Quat::from_scaled_axis(piece.spin * h) * piece.rot).normalize();
            }
            if piece.pos.length() > 12.0 {
                piece.settled = true;
            }
            if broken.last() == Some(&n) {
                // Into single grains: each at its own place with the piece's motion there.
                let m = piece.local_model();
                for &c in &piece.cells {
                    let at = model.cells[c as usize].at;
                    let pos = m.transform_point3(at);
                    let seed = c.wrapping_mul(0x9E37_79B9) ^ piece.actor;
                    let scatter = Vec3::new(hash01(seed) - 0.5, hash01(seed ^ 0x51) - 0.5, hash01(seed ^ 0xA3) - 0.5) * 0.6;
                    born.push(ChunkBody {
                        actor: piece.actor,
                        frame: piece.frame,
                        cells: vec![c],
                        centre: at,
                        scale: piece.scale,
                        pos,
                        rot: piece.rot,
                        vel: piece.vel + piece.spin.cross(pos - piece.pos) + scatter,
                        spin: scatter * 20.0,
                        radius: 0.5 * model.cell * 3f32.sqrt() * piece.scale,
                        cohesion_j: 0.0,
                        bounces: piece.bounces,
                        settled: false,
                    });
                }
            }
        }
        for &n in broken.iter().rev() {
            self.pieces.remove(n);
        }
        self.pieces.extend(born);
        self.bound_pieces();
    }

    pub(super) fn begin_frame(&mut self) {
        for list in &mut self.cube_lists {
            list.clear();
        }
    }

    fn push(&mut self, label: u8, transform: Mat4) {
        let list = &mut self.cube_lists[label as usize - 1];
        if list.len() < CUBES_PER_LABEL {
            list.push(transform);
        } else if !self.overflow_reported {
            self.overflow_reported = true;
            eprintln!("[crew] voxel bodies: more than {CUBES_PER_LABEL} cubes of one colour in view; the rest are not drawn");
        }
    }

    /// Draw a solved body: `models` are its parts' poses (each part's own, or its anchor's once
    /// it left the articulated simulation); `joined[i]` whether part i is still joined to its
    /// anatomical parent (cells are drawn blended only across a joint that holds).
    pub(super) fn add_body(&mut self, peer: u32, face: usize, models: &[Mat4; PARTS], joined: &[bool; PARTS]) {
        let Some(index) = self.actors.iter().position(|a| a.peer == peer) else { return };
        let face = face.min(APPEARANCES - 1);
        let revision = self.actors[index].tissue.revision;
        if !matches!(&self.actors[index].cache, Some((rev, f, _)) if *rev == revision && *f == face) {
            let tissue = &self.actors[index].tissue;
            let labels = &self.skin_labels[face];
            let lists: [Vec<(u32, u8)>; PARTS] = std::array::from_fn(|i| self.model.part_cells[i].iter()
                .filter(|&&c| tissue.present[c as usize] && self.model.exposed(&tissue.present, c as usize))
                .map(|&c| (c, crew_tissue::label(&self.model.cells[c as usize], labels[c as usize])))
                .collect());
            self.actors[index].cache = Some((revision, face, lists));
        }
        let poses: [(Vec3, Quat, Vec3); PARTS] = std::array::from_fn(|i| models[i].to_scale_rotation_translation());
        let model = self.model.clone();
        let Some((_, _, lists)) = self.actors[index].cache.take() else { return };
        for (i, cells) in lists.iter().enumerate() {
            let (scale, rot, _) = poses[i];
            for &(c, label) in cells {
                let at = model.cells[c as usize].at;
                let mut p = models[i].transform_point3(at);
                let mut r = rot;
                let part = model.cells[c as usize].part;
                if let Some((other, w)) = model.blend[c as usize] {
                    // The joint between them belongs to the child: blend only while it holds.
                    let child = if crew_anatomical_parent(part) == Some(other) { part } else { other };
                    if joined[child.index()] {
                        p = p.lerp(models[other.index()].transform_point3(at), w);
                        r = rot.slerp(poses[other.index()].1, w);
                    }
                }
                self.push(label, Mat4::from_scale_rotation_translation(Vec3::splat(model.cell * scale.x), r, p));
            }
        }
        if let Some(plane) = review_section() {
            let n = plane.truncate();
            let tissue = &self.actors[index].tissue;
            let mut inside = Vec::new();
            for (i, cells) in model.part_cells.iter().enumerate() {
                let (scale, rot, _) = poses[i];
                for &c in cells {
                    let c = c as usize;
                    if !tissue.present[c] || model.exposed(&tissue.present, c) {
                        continue;
                    }
                    let p = models[i].transform_point3(model.cells[c].at);
                    if (n.dot(p) - plane.w).abs() <= 1.5 * model.cell * scale.x {
                        inside.push((crew_tissue::label(&model.cells[c], self.skin_labels[face][c]),
                            Mat4::from_scale_rotation_translation(Vec3::splat(model.cell * scale.x), rot, p)));
                    }
                }
            }
            for (label, m) in inside {
                self.push(label, m);
            }
        }
        self.actors[index].cache = Some((revision, face, lists));
    }

    /// Draw every piece where it is (`face_of` gives each actor's face).
    pub(super) fn add_pieces(&mut self, player: &Tank, face_of: &dyn Fn(u32) -> usize) {
        let model = self.model.clone();
        let pieces = std::mem::take(&mut self.pieces);
        for piece in &pieces {
            let face = face_of(piece.actor).min(APPEARANCES - 1);
            let m = blood_parent_model(player, piece.frame) * piece.local_model();
            for &c in &piece.cells {
                let cell = &model.cells[c as usize];
                let label = crew_tissue::label(cell, self.skin_labels[face][c as usize]);
                self.push(label, m * Mat4::from_scale_rotation_translation(Vec3::splat(model.cell), Quat::IDENTITY, cell.at));
            }
        }
        self.pieces = pieces;
    }

    pub(super) fn end_frame(&self, r: &mut Renderer) {
        // Debug only (KESSEL_TISSUE_DEBUG=hide): no cells drawn, so what is not cells (the
        // helmet) can be checked where it sits. Never set by a shipping preset.
        let hide = std::env::var("KESSEL_TISSUE_DEBUG").as_deref() == Ok("hide");
        for (page, list) in self.cube_pages.iter().zip(&self.cube_lists).filter(|(p, _)| **p != usize::MAX) {
            r.set_moving_instanced_mats(*page, if hide { &[] } else { list });
        }
    }

    pub(super) fn hide(&self, r: &mut Renderer) {
        for &page in self.cube_pages.iter().filter(|p| **p != usize::MAX) {
            r.set_moving_instanced_mats(page, &[]);
        }
    }

    pub(super) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "cells": self.model.cell_count(), "cell_m": self.model.cell, "anatomy": self.model.anatomy,
            "bodies": self.actors.iter().map(|a| serde_json::json!({
                "actor_id": a.peer,
                "cells_gone": crew_physics::CrewBodyPart::ALL.iter().map(|p|
                    self.model.part_cells[p.index()].iter().filter(|&&c| !a.tissue.present[c as usize]).count()).collect::<Vec<_>>(),
                "bonds_broken": a.tissue.broken.iter().filter(|b| **b).count(),
                "helmet": a.helmet.as_ref().map_or(serde_json::json!("on the head"), |h| serde_json::json!({
                    "frame_position": h.pos.to_array(), "speed_mps": h.vel.length(), "at_rest": h.settled})),
            })).collect::<Vec<_>>(),
            "pieces": self.pieces.len(),
            "grains": self.pieces.iter().filter(|p| p.cells.len() == 1).count(),
            "pieces_at_rest": self.pieces.iter().filter(|p| p.settled).count(),
            "piece_cells": self.pieces.iter().map(|p| p.cells.len()).sum::<usize>(),
            "cubes_drawn_last_frame": self.cube_lists.iter().map(Vec::len).collect::<Vec<_>>(),
        })
    }
}

/// A piece's outermost cells (offsets from its centre in its own turn, scaled): for each of the 6
/// axes and 8 diagonals the cell furthest along it, duplicates dropped. A single grain has none
/// (its sphere is its cell).
fn outer_cells(model: &crew_tissue::TissueModel, piece: &crew_tissue::ChunkBody) -> Vec<Vec3> {
    if piece.cells.len() < 2 {
        return Vec::new();
    }
    let mut dirs = vec![Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
    for i in 0..8 {
        dirs.push(Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 },
            if i & 4 == 0 { -1.0 } else { 1.0 }));
    }
    let mut chosen: Vec<u32> = Vec::new();
    for dir in dirs {
        let best = piece.cells.iter().copied()
            .max_by(|&a, &b| model.cells[a as usize].at.dot(dir).total_cmp(&model.cells[b as usize].at.dot(dir)));
        if let Some(c) = best {
            if !chosen.contains(&c) {
                chosen.push(c);
            }
        }
    }
    chosen.into_iter().map(|c| (model.cells[c as usize].at - piece.centre) * piece.scale).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 12 x 20 x 12 cm chunk flying past a 2 cm post that its centre's path misses but its side
    /// meets: it stops at the post, and no cell ends up beyond it.
    #[test]
    fn a_piece_whose_side_meets_a_post_stops_at_it() {
        let figure = crate::prims::box_mesh(Vec3::new(-0.06, 1.0, -0.06), Vec3::new(0.06, 1.2, 0.06), [1, 1, 1]);
        let model = std::sync::Arc::new(crew_tissue::build_procedural(&figure, &std::array::from_fn(|_| None)));
        let cells: Vec<u32> = (0..model.cells.len() as u32).collect();
        let centre = Vec3::new(0.0, 1.1, 0.0);
        let piece = ChunkBody { actor: 1, frame: BloodFrame::Turret, cells, centre, scale: 1.0,
            pos: Vec3::new(0.0, 2.0, 0.0), rot: Quat::IDENTITY, vel: Vec3::new(0.0, 0.0, 4.0), spin: Vec3::ZERO,
            radius: 0.13, cohesion_j: 1.0e9, bounces: 0, settled: false };
        let post_z = 0.40;
        let post = crate::prims::box_mesh(Vec3::new(0.035, 1.0, post_z), Vec3::new(0.055, 3.0, post_z + 0.02), [90; 3]);
        let away = crate::prims::box_mesh(Vec3::new(5.0, 0.0, 5.0), Vec3::new(5.1, 0.1, 5.1), [90; 3]);
        let hf = crate::terrain::Heightfield { res: 2, spacing: 1000.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0,
            openings: Default::default(), detail: None };
        let tank = Tank::new(0.0, 0.0, 0.0, &hf, crate::tank::TIGER_PARTS);
        let mut models = [Mat4::IDENTITY; surface_fire::FRAMES];
        models[0] = tank.hull_model();
        models[1] = tank.turret_model();
        let run = |with_post: bool| {
            let source = if with_post {
                surface_fire::CabinFire::new(&[(1, &post)])
            } else {
                surface_fire::CabinFire::new(&[(1, &away)])
            };
            let world = FragmentWorld::new(&source, &models, &tank,
                crate::fea_damage::components::Articulation::rest(tank.parts.turret_pivot));
            let mut draw = TissueDraw { model: model.clone(), helmet: HelmetShape::new(&figure),
                cube_pages: [usize::MAX; crew_tissue::LABELS], skin_labels: Vec::new(), actors: Vec::new(),
                pieces: vec![piece.clone()], cube_lists: std::array::from_fn(|_| Vec::new()), overflow_reported: false };
            for _ in 0..15 {
                draw.integrate(&world, &tank, 1.0 / 60.0);
            }
            let p = &draw.pieces[0];
            let m = p.local_model();
            p.cells.iter().map(|&c| m.transform_point3(model.cells[c as usize].at).z).fold(f32::MIN, f32::max)
        };
        // Its centre's path passes the post 3.5 cm clear: with no post it flies on.
        assert!(run(false) > post_z + 0.3);
        let furthest = run(true);
        assert!(furthest < post_z + 0.5 * model.cell, "a cell went beyond the post: {furthest}");
    }
}
