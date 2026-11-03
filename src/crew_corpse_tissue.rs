//! Drawing the casualties' voxel bodies (`crew_tissue`) in the solo corpse renderer. A dead man
//! with a solved body is drawn wholly as his cells (equal runs merged into boxes, one instanced
//! page per colour) instead of the figure's triangles and the skeleton page; the pieces that came
//! away are drawn the same way where they fly and lie. Every page is allocated here at build,
//! before any casualty, so the retained draw layout that saves record never changes shape.
use super::*;
use super::crew_tissue::{self, CasualtyTissue, ChunkBody, Strike, TissueModel, VoxelBox};

const APPEARANCES: usize = 5;
const PARTS: usize = crew_physics::CREW_BODY_PART_COUNT;
/// Boxes per colour page; more are dropped with one report (a named bound).
const BOXES_PER_LABEL: usize = 6_144;
/// Cells in pieces kept at once; past it the oldest pieces at rest go (a named bound).
const CHUNK_CELLS: usize = 4_096;
/// Muscle albedo (sRGB) and roughness: two shades of dark red, authored game-level colours (not
/// measured), wet enough to take a highlight.
const MUSCLE: [[u8; 3]; 2] = [[118, 26, 22], [96, 18, 16]];
const MUSCLE_ROUGHNESS: f32 = 0.42;

struct Actor {
    peer: u32,
    tissue: CasualtyTissue,
    /// (tissue revision, face) the cached boxes were made for.
    cache: [Option<(u32, usize, Vec<VoxelBox>)>; PARTS],
}

struct Piece {
    body: ChunkBody,
    /// (face, boxes about the piece's centre, figure frame).
    boxes: Option<(usize, Vec<VoxelBox>)>,
}

pub(super) struct TissueDraw {
    model: std::sync::Arc<TissueModel>,
    box_pages: [usize; crew_tissue::LABELS],
    /// Skin colour label per face and cell.
    skin_labels: Vec<Vec<u8>>,
    actors: Vec<Actor>,
    pieces: Vec<Piece>,
    box_lists: [Vec<Mat4>; crew_tissue::LABELS],
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

impl TissueDraw {
    /// `figures` are the five relaxed faces (one body, five atlases); `bones` the fitted skeleton
    /// pages and `helmet` the steel helmet (figure frame), voxelised onto the same grid.
    pub(super) fn allocate(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer, figures: &[CpuMesh],
        bones: &[Option<CpuMesh>; PARTS], helmet: &CpuMesh) -> anyhow::Result<Self> {
        anyhow::ensure!(figures.len() == APPEARANCES, "five relaxed faces expected");
        let start = std::time::Instant::now();
        let model = std::sync::Arc::new(crew_tissue::build_tissue(&figures[0], bones, Some(helmet)));
        let cells_ms = start.elapsed().as_secs_f32() * 1000.0;
        let atlases: Vec<&crate::gltf_load::CpuImage> = figures.iter()
            .map(|f| f.texture.as_ref().ok_or_else(|| anyhow::anyhow!("relaxed face without atlas")))
            .collect::<anyhow::Result<_>>()?;
        // The skin cells' colours, from each face's atlas at the cell's nearest skin point.
        let skin = |c: &&crew_tissue::Cell| c.tissue == crew_tissue::Tissue::Skin;
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
        // The helmet's paint: its own atlas averaged over its vertices.
        let steel_colour = helmet.texture.as_ref().map_or([72, 76, 66], |image| {
            let sum = helmet.vertices.iter().map(|v| texel(image, Vec2::from(v.uv)).map(f32::from))
                .fold([0.0f32; 3], |a, c| [a[0] + c[0], a[1] + c[1], a[2] + c[2]]);
            sum.map(|s| (s / helmet.vertices.len().max(1) as f32) as u8)
        });
        let mut box_pages = [0usize; crew_tissue::LABELS];
        for (index, page) in box_pages.iter_mut().enumerate() {
            let label = index as u8 + 1;
            let (colour, roughness) = if label <= crew_tissue::SKIN_LABELS {
                (colours[index].map(|c| c.round().clamp(0.0, 255.0) as u8), figures[0].roughness_factor)
            } else if label == crew_tissue::BONE_LABEL {
                (bone_colour, bone_roughness)
            } else if label == crew_tissue::STEEL_LABEL {
                (steel_colour, helmet.roughness_factor)
            } else {
                (MUSCLE[(label - crew_tissue::MUSCLE_LABELS[0]) as usize], MUSCLE_ROUGHNESS)
            };
            let mut mesh = box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), colour);
            mesh.roughness_factor = roughness;
            mesh.metallic_factor = 0.0;
            let prototype = r.register_instanced_prototype(device, queue, &mesh);
            *page = r.add_moving_instanced_pool(device, prototype, BOXES_PER_LABEL)
                .ok_or_else(|| anyhow::anyhow!("tissue box page"))?;
        }
        let count = |t| model.cells.iter().filter(|c| c.tissue == t).count();
        println!("[crew] voxel bodies: {} cells of {:.0} mm (skin {}, muscle {}, bone {} from the skeleton's triangles, helmet {}) in {cells_ms:.0} ms; {} pages",
            model.cell_count(), crew_tissue::CELL_M * 1000.0, count(crew_tissue::Tissue::Skin), count(crew_tissue::Tissue::Muscle),
            count(crew_tissue::Tissue::Bone), count(crew_tissue::Tissue::Steel), box_pages.len());
        Ok(Self { model, box_pages, skin_labels, actors: Vec::new(), pieces: Vec::new(),
            box_lists: std::array::from_fn(|_| Vec::new()), overflow_reported: false })
    }

    pub(super) fn reset(&mut self) {
        self.actors.clear();
        self.pieces.clear();
    }

    /// Solve the strikes that killed the ragdoll's man and throw what came away: the share of
    /// each part's soft cells that remain.
    pub(super) fn strike(&mut self, ragdoll: &CrewRagdoll, strikes: &[Strike]) -> [f32; PARTS] {
        let model = self.model.clone();
        let peer = ragdoll.casualty.peer_id;
        let index = match self.actors.iter().position(|a| a.peer == peer) {
            Some(index) => index,
            None => {
                self.actors.push(Actor { peer, tissue: CasualtyTissue::new(&model), cache: std::array::from_fn(|_| None) });
                self.actors.len() - 1
            }
        };
        let actor = &mut self.actors[index];
        let started = std::time::Instant::now();
        let solve = actor.tissue.solve(&model, strikes);
        let torn: usize = solve.chunks.iter().map(|c| c.cells.len()).sum();
        let bone: usize = solve.chunks.iter().flat_map(|c| c.cells.iter())
            .filter(|&&c| model.cells[c as usize].tissue == crew_tissue::Tissue::Bone).count();
        println!("[crew] body of actor {peer} solved on its cells: {} strikes, {} bonds broken, {torn} cells torn away ({bone} bone) in {} pieces, {} fragments stopped in him, {} steps in {:.0} ms",
            strikes.len(), solve.bonds_broken, solve.chunks.len(), solve.lodged, solve.steps, started.elapsed().as_secs_f32() * 1000.0);
        let shares = std::array::from_fn(|i| actor.tissue.soft_share(&model, crew_physics::CrewBodyPart::ALL[i]));
        for body in crew_tissue::chunk_bodies(&model, ragdoll, solve.chunks) {
            self.pieces.push(Piece { body, boxes: None });
        }
        // Bounded: the oldest pieces at rest go first, then the oldest in flight.
        let mut cells: usize = self.pieces.iter().map(|p| p.body.cells.len()).sum();
        while cells > CHUNK_CELLS && !self.pieces.is_empty() {
            let oldest = self.pieces.iter().position(|p| p.body.settled).unwrap_or(0);
            cells -= self.pieces[oldest].body.cells.len();
            self.pieces.remove(oldest);
        }
        shares
    }

    pub(super) fn has_body(&self, peer: u32) -> bool {
        self.actors.iter().any(|a| a.peer == peer)
    }

    pub(super) fn pieces_mut(&mut self) -> impl Iterator<Item = &mut ChunkBody> {
        self.pieces.iter_mut().map(|p| &mut p.body)
    }

    pub(super) fn begin_frame(&mut self) {
        for list in &mut self.box_lists {
            list.clear();
        }
    }

    fn push(&mut self, model: Mat4, b: &VoxelBox) {
        let list = &mut self.box_lists[b.label as usize - 1];
        if list.len() < BOXES_PER_LABEL {
            list.push(model * Mat4::from_scale_rotation_translation(b.size, Quat::IDENTITY, b.centre));
        } else if !self.overflow_reported {
            self.overflow_reported = true;
            eprintln!("[crew] voxel bodies: more than {BOXES_PER_LABEL} boxes of one colour in view; the rest are not drawn");
        }
    }

    /// Draw one part of a solved body with `model` (the part's own pose, or its anchor's once it
    /// left the articulated simulation).
    pub(super) fn add_part(&mut self, peer: u32, face: usize, part: crew_physics::CrewBodyPart, model: Mat4) {
        let Some(index) = self.actors.iter().position(|a| a.peer == peer) else { return };
        let face = face.min(APPEARANCES - 1);
        let i = part.index();
        let revision = self.actors[index].tissue.revision;
        if !matches!(&self.actors[index].cache[i], Some((rev, f, _)) if *rev == revision && *f == face) {
            let tissue = &self.actors[index].tissue;
            let labels = &self.skin_labels[face];
            let cells: Vec<([u16; 3], u8)> = self.model.part_cells[i].iter().filter(|&&c| tissue.present[c as usize])
                .map(|&c| (self.model.cells[c as usize].ijk, crew_tissue::label(&self.model.cells[c as usize], labels[c as usize])))
                .collect();
            let boxes = crew_tissue::merge_boxes(&self.model, &cells);
            self.actors[index].cache[i] = Some((revision, face, boxes));
        }
        let boxes = self.actors[index].cache[i].as_ref().map(|(_, _, b)| b.clone()).unwrap_or_default();
        for b in &boxes {
            self.push(model, b);
        }
    }

    /// Draw every piece where it is (`face_of` gives each actor's face).
    pub(super) fn add_pieces(&mut self, player: &Tank, face_of: &dyn Fn(u32) -> usize) {
        for n in 0..self.pieces.len() {
            let face = face_of(self.pieces[n].body.actor).min(APPEARANCES - 1);
            if !matches!(&self.pieces[n].boxes, Some((f, _)) if *f == face) {
                let labels = &self.skin_labels[face];
                let cells: Vec<([u16; 3], u8)> = self.pieces[n].body.cells.iter()
                    .map(|&c| (self.model.cells[c as usize].ijk, crew_tissue::label(&self.model.cells[c as usize], labels[c as usize])))
                    .collect();
                self.pieces[n].boxes = Some((face, crew_tissue::merge_boxes(&self.model, &cells)));
            }
            let model = blood_parent_model(player, self.pieces[n].body.frame) * self.pieces[n].body.local_model();
            let boxes = self.pieces[n].boxes.as_ref().map(|(_, b)| b.clone()).unwrap_or_default();
            for b in &boxes {
                self.push(model, b);
            }
        }
    }

    pub(super) fn end_frame(&self, r: &mut Renderer) {
        for (page, list) in self.box_pages.iter().zip(&self.box_lists) {
            r.set_moving_instanced_mats(*page, list);
        }
    }

    pub(super) fn hide(&self, r: &mut Renderer) {
        for &page in &self.box_pages {
            r.set_moving_instanced_mats(page, &[]);
        }
    }

    pub(super) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "cells": self.model.cell_count(), "cell_m": crew_tissue::CELL_M,
            "bodies": self.actors.iter().map(|a| serde_json::json!({
                "actor_id": a.peer,
                "cells_gone": crew_physics::CrewBodyPart::ALL.iter().map(|p|
                    self.model.part_cells[p.index()].iter().filter(|&&c| !a.tissue.present[c as usize]).count()).collect::<Vec<_>>(),
                "bonds_broken": a.tissue.broken.iter().filter(|b| **b).count(),
            })).collect::<Vec<_>>(),
            "pieces": self.pieces.len(),
            "pieces_at_rest": self.pieces.iter().filter(|p| p.body.settled).count(),
            "piece_cells": self.pieces.iter().map(|p| p.body.cells.len()).sum::<usize>(),
            "boxes_drawn_last_frame": self.box_lists.iter().map(Vec::len).collect::<Vec<_>>(),
        })
    }
}
