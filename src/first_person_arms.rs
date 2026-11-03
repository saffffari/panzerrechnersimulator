//! The player's own body seen from his own eyes: the reviewed AdrielCZ Panzertruppe (CC-BY-4.0,
//! black Panzer uniform, the same source as the rest of the crew) with source-weighted local
//! arms/body. Same bounded two-link IK as mechanism crew arms; no weapon/gameplay authority.
//!
//! Placement rule (2026-11-02): the camera sits at the source's own `Eyes_N1` joint. Earlier the
//! nisu infantry body was hung from its neck joint 8 cm under the camera, so a downward look started
//! inside the collar and shoulders: 26 % of the lower view hemisphere met body surfaces closer than
//! 15 cm, drawn double-sided as the screen-filling "orange pointy" shards of the owner's report.
//!
//! Named visibility rule, OWNER_HEAD_HIDDEN: the eyes are inside the head, so the one source object
//! that is the head (`PanzerTruppe_Head_N1`: skull, face and neck skin, every vertex weighted wholly
//! to Neck/Head/Eyes) is not drawn for the camera that owns the body. Nothing is shrunk or moved to
//! hide it; every other camera (the commander witness, third-person and LAN views) draws the
//! whole man.
use super::*;
use crate::gltf_load::Vertex;
const ROOT: &str = "assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4";
/// The player's own identity is the commander's (`crew_identity_index(Commander)` = face01).
/// The face is never drawn for the owner; the witness shows it.
const OWN_FACE: &str = "face01";
const INTERVAL: f32 = 1.0 / 30.0;
/// The source rig's pose names: seated posts (gunner, driver, radio operator) and standing posts
/// (commander in the cupola, loader on the turret floor, the dismounted walker).
const POSES: [&str; 2] = ["seated", "standing"];
const SEATED: usize = 0;
const STANDING: usize = 1;
struct Bone {
    name: String,
    parent: Option<usize>,
    base: Mat4,
}
struct Rig {
    bones: Vec<Bone>,
    weights: Vec<Vec<(usize, f32)>>,
    vertices: Vec<Vertex>,
    arms: [[usize; 3]; 2],
    /// Mitten finger chain (Fingers_1, Fingers_2, Fingers_3) and thumb chain per side.
    fingers: [[usize; 3]; 2],
    thumbs: [[usize; 3]; 2],
    arm_map: Vec<usize>,
    page_indices: [Vec<u32>; 2],
    page_ownership: [Vec<[f32; 11]>; 2],
    /// Source triangles withheld from the owner's view by OWNER_HEAD_HIDDEN.
    head_triangles: usize,
    /// The source `Eyes_N1` joint in the body's local frame: where the owner's camera is.
    eyes: Vec3,
    neck: usize,
    palm_offsets: [Vec3; 2],
    source_hand_directions: [Vec3; 2],
}
/// The crank knob of a source handwheel, in the wheel's own frame: the centroid of the rim
/// vertices that stand furthest out along the shaft (`sign` picks the operator's side).
fn handwheel_knob(path: &str, axis: usize, sign: f32) -> anyhow::Result<Vec3> {
    let wheel = crate::gltf_load::load_glb(path)?;
    let points = wheel.vertices.iter().map(|v| Vec3::from_array(v.pos)).collect::<Vec<_>>();
    let radial = |p: Vec3| { let mut q = p; q[axis] = 0.0; q.length() };
    let out = points.iter().filter(|p| radial(**p) > 0.06).map(|p| p[axis] * sign).fold(f32::MIN, f32::max);
    let knob = points.iter().copied().filter(|p| radial(*p) > 0.06 && p[axis] * sign > out - 0.035).collect::<Vec<_>>();
    anyhow::ensure!(!knob.is_empty(), "handwheel knob absent in {path}");
    Ok(knob.iter().copied().sum::<Vec3>() / knob.len() as f32)
}
pub(super) struct FirstPerson {
    rigs: [Rig; 2],
    /// Crank knobs of the gunner's elevation handwheel (shaft X) and the commander's auxiliary
    /// traverse handwheel (shaft -Z), each in its wheel's frame.
    knobs: [Vec3; 2],
    last_turret_yaw: std::cell::Cell<f32>,
    slots: [[usize; 2]; 2],
    last_sample: std::cell::Cell<f32>,
    last_grips: std::cell::Cell<(usize, [Option<Mat4>; 2])>,
    local_visibility: std::cell::Cell<[[bool; 2]; 2]>,
    desired_hand: std::cell::Cell<Option<(Mat4, Mat4)>>,
    last_error: std::cell::Cell<f32>,
    last_mask: std::cell::Cell<(usize, u16)>,
    witness_slot: usize,
    witness_indices: Vec<u32>,
    witness_ownership: Vec<[f32; 11]>,
    witness_mask: std::cell::Cell<u16>,
}
fn compact(source: &CpuMesh, triangles: Vec<u32>) -> (CpuMesh, Vec<usize>) {
    let mut map = std::collections::HashMap::new();
    let mut originals = Vec::new();
    let mut mesh = source.clone();
    mesh.vertices.clear();
    mesh.indices = triangles
        .into_iter()
        .map(|i| {
            *map.entry(i).or_insert_with(|| {
                let result = mesh.vertices.len() as u32;
                mesh.vertices.push(source.vertices[i as usize]);
                originals.push(i as usize);
                result
            })
        })
        .collect();
    (mesh, originals)
}
fn static_glb(pose: &str) -> String {
    format!("{ROOT}/baked/german_panzertruppe_{OWN_FACE}_{pose}_static.glb")
}
impl Rig {
    fn descendant(&self, mut i: usize, parent: usize) -> bool {
        for _ in 0..self.bones.len() {
            if i == parent {
                return true;
            }
            let Some(next) = self.bones[i].parent else {
                return false;
            };
            i = next;
        }
        false
    }
    fn joint(&self, i: usize) -> Vec3 {
        self.bones[i].base.transform_point3(Vec3::ZERO)
    }
    fn load(pose: usize) -> anyhow::Result<(Self, [CpuMesh; 2])> {
        let name = POSES[pose];
        let doc: serde_json::Value = serde_json::from_str(&crate::resource::read_to_string(
            format!("{ROOT}/animation/{name}_arm_bindings.v1.json"),
        )?)?;
        anyhow::ensure!(doc["schema"] == "panzer.source-crew-arm-bindings.v1");
        let source = crate::gltf_load::load_glb(&static_glb(name))?;
        let bones = doc["bones"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| Bone {
                name: b["name"].as_str().unwrap().into(),
                parent: b["parent"].as_u64().map(|i| i as usize),
                base: Mat4::from_cols_array(&std::array::from_fn(|i| {
                    b["matrix"][i].as_f64().unwrap() as f32
                })),
            })
            .collect::<Vec<_>>();
        let weights = doc["weights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                w.as_array()
                    .unwrap()
                    .iter()
                    .map(|a| {
                        (
                            a[0].as_u64().unwrap() as usize,
                            a[1].as_f64().unwrap() as f32,
                        )
                    })
                    .collect()
            })
            .collect::<Vec<Vec<_>>>();
        anyhow::ensure!(bones.len() == 39 && weights.len() == source.vertices.len(),
            "first-person crew bindings do not match {}", static_glb(name));
        // Every influence names a joint of this skeleton and every vertex's weights sum to one.
        // A joint index past the skeleton, or a vertex left unweighted, would drag source
        // triangles to the body origin when skinned.
        anyhow::ensure!(weights.iter().all(|w| !w.is_empty()
            && w.iter().all(|&(j, v)| j < bones.len() && v.is_finite() && v >= 0.0)
            && (w.iter().map(|w| w.1).sum::<f32>() - 1.0).abs() < 1e-4),
            "first-person crew weights reference a missing joint or do not sum to one");
        let index = |name: &str| {
            bones
                .iter()
                .position(|b| b.name == name)
                .ok_or_else(|| anyhow::anyhow!("missing source crew joint {name}"))
        };
        let arms = [
            [index("L_Arm_N1")?, index("L_Elbow_N1")?, index("L_Hand_N1")?],
            [index("R_Arm_N1")?, index("R_Elbow_N1")?, index("R_Hand_N1")?],
        ];
        let fingers = [
            [index("L_Fingers_1_N1")?, index("L_Fingers_2_N1")?, index("L_Fingers_3_N1")?],
            [index("R_Fingers_1_N1")?, index("R_Fingers_2_N1")?, index("R_Fingers_3_N1")?],
        ];
        let thumbs = [
            [index("L_Thumb_1_N1")?, index("L_Thumb_2_N1")?, index("L_Thumb_3_N1")?],
            [index("R_Thumb_1_N1")?, index("R_Thumb_2_N1")?, index("R_Thumb_3_N1")?],
        ];
        let neck = index("Neck_N1")?;
        let eyes = bones[index("Eyes_N1")?].base.transform_point3(Vec3::ZERO);
        let mut rig = Self {
            bones,
            weights,
            vertices: source.vertices.clone(),
            arms,
            fingers,
            thumbs,
            arm_map: Vec::new(),
            page_indices: Default::default(),
            page_ownership: Default::default(),
            head_triangles: 0,
            eyes,
            neck,
            palm_offsets: [Vec3::ZERO; 2],
            source_hand_directions: [Vec3::ZERO; 2],
        };
        for side in 0..2 {
            rig.source_hand_directions[side] =
                rig.joint(rig.fingers[side][0]) - rig.joint(rig.arms[side][2]);
            let wrist = rig.joint(rig.arms[side][2]);
            let hand = rig.arms[side][2];
            let points = rig
                .vertices
                .iter()
                .zip(&rig.weights)
                .filter(|(_, w)| {
                    w.iter().filter(|&&(b, _)| rig.descendant(b, hand)).map(|&(_, v)| v).sum::<f32>() > 0.65
                })
                .map(|(v, _)| Vec3::from(v.pos))
                .collect::<Vec<_>>();
            anyhow::ensure!(!points.is_empty(), "source palm missing");
            rig.palm_offsets[side] =
                points.iter().copied().sum::<Vec3>() / points.len() as f32 - wrist;
        }
        // Any vertex an arm joint moves is skinned. The former rule (one influence above 0.35)
        // left spread-weight palm and thumb triangles (0.31/0.34/0.34 over wrist, hand and thumb)
        // on the static body page, so the source hands stayed hanging at the thighs while the
        // skinned hands held the pistol. Shoulder-weighted vertices on the same page simply stay
        // put under the skin, so seam triangles stretch with the arm instead of tearing.
        let arm_vertex = |i: usize| {
            rig.weights[i]
                .iter()
                .any(|&(b, w)| w > 0.0 && rig.arms.iter().any(|a| rig.descendant(b, a[0])))
        };
        // OWNER_HEAD_HIDDEN: the head object's vertices are weighted wholly to Neck/Head/Eyes; no
        // uniform, arm or hand vertex carries any neck weight (`owner_head_rule_is_exactly_the_
        // source_head_object`).
        let head_vertex = |i: usize| {
            rig.weights[i]
                .iter()
                .filter(|&&(b, _)| rig.descendant(b, neck))
                .map(|&(_, w)| w)
                .sum::<f32>()
                > 0.5
        };
        let triangles = source.indices.chunks_exact(3);
        let head_triangles = triangles.clone().filter(|t| t.iter().any(|&i| head_vertex(i as usize))).count();
        let arms_idx = triangles
            .clone()
            .filter(|t| t.iter().any(|&i| arm_vertex(i as usize)))
            .flatten()
            .copied()
            .collect();
        let body_idx = triangles
            .filter(|t| {
                !t.iter().any(|&i| head_vertex(i as usize))
                    && !t.iter().any(|&i| arm_vertex(i as usize))
            })
            .flatten()
            .copied()
            .collect();
        let (arm_mesh, arm_map) = compact(&source, arms_idx);
        let (body_mesh, body_map) = compact(&source, body_idx);
        anyhow::ensure!(
            !arm_mesh.indices.is_empty() && arm_mesh.vertices.len() <= 4096,
            "source compact arms invalid"
        );
        rig.page_indices = [arm_mesh.indices.clone(), body_mesh.indices.clone()];
        rig.page_ownership = [&arm_map, &body_map].map(|map| {
            map.iter()
                .map(|i| {
                    let mut w = [0.0; 11];
                    for &(b, v) in &rig.weights[*i] {
                        w[living_crew_wounds::bone_part(&rig.bones[b].name)] += v;
                    }
                    w
                })
                .collect()
        });
        rig.arm_map = arm_map;
        rig.head_triangles = head_triangles;
        Ok((rig, [arm_mesh, body_mesh]))
    }
    /// Re-skin the arm page. `body` places the source in the world and may carry a uniform scale
    /// (the dismounted walker's stature); `grips` are world hand frames.
    fn pose(&self, body: Mat4, grips: [Option<Mat4>; 2]) -> (Vec<Vertex>, f32) {
        let inverse = body.inverse();
        let (body_scale, body_rotation, _) = body.to_scale_rotation_translation();
        let scale = body_scale.x;
        let mut matrices = self.bones.iter().map(|b| b.base).collect::<Vec<_>>();
        let mut error: f32 = 0.0;
        for side in 0..2 {
            let Some(grip) = grips[side] else { continue };
            let arm = self.arms[side];
            let points = arm.map(|i| self.joint(i));
            let delta = Quat::from_rotation_arc(
                self.source_hand_directions[side].normalize(),
                Vec3::new(0.0, -0.89, -0.455).normalize(),
            );
            let (_, grip_rotation, grip_position) = grip.to_scale_rotation_translation();
            let hand_rotation = grip_rotation * delta;
            let wrist_world = grip_position - hand_rotation * (self.palm_offsets[side] * scale);
            let target = inverse.transform_point3(wrist_world);
            let solved = crew_arm_animation::arm_chain(
                points[0],
                points[1],
                points[2],
                target,
                Vec3::new(if side == 0 { 1.0 } else { -1.0 }, -0.45, -0.25),
            );
            let first = Mat4::from_translation(points[0])
                * Mat4::from_quat(Quat::from_rotation_arc(
                    (points[1] - points[0]).normalize(),
                    (solved[1] - points[0]).normalize(),
                ))
                * Mat4::from_translation(-points[0]);
            for (i, m) in matrices.iter_mut().enumerate() {
                if self.descendant(i, arm[0]) {
                    *m = first * *m;
                }
            }
            let moved = first.transform_point3(points[2]);
            let second = Mat4::from_translation(solved[1])
                * Mat4::from_quat(Quat::from_rotation_arc(
                    (moved - solved[1]).normalize(),
                    (solved[2] - solved[1]).normalize(),
                ))
                * Mat4::from_translation(-solved[1]);
            for (i, m) in matrices.iter_mut().enumerate() {
                if self.descendant(i, arm[1]) {
                    *m = second * *m;
                }
            }
            let before = matrices[arm[2]];
            let rotation = body_rotation.inverse() * hand_rotation;
            // Source FBX bones retain their imported 0.0089 scale. Dropping it here would expand
            // every weighted hand vertex about 114x after base.inverse(). Extract rotation from
            // the decomposed source matrix, never a scaled Mat4.
            let (source_scale, source_rotation, _) =
                self.bones[arm[2]].base.to_scale_rotation_translation();
            let after = Mat4::from_scale_rotation_translation(
                source_scale,
                rotation * source_rotation,
                solved[2],
            );
            let hand_delta = after * before.inverse();
            for (i, m) in matrices.iter_mut().enumerate() {
                if self.descendant(i, arm[2]) {
                    *m = hand_delta * *m;
                }
            }
            let actual = body.transform_point3(solved[2]) + hand_rotation * (self.palm_offsets[side] * scale);
            error = error.max(actual.distance(grip_position));
            // Close the source mitten finger and thumb about the grip (no replacement hands or
            // texture): each finger joint turns toward the thumb tip, the thumb toward the finger.
            let at = |m: &[Mat4], i: usize| m[i].transform_point3(Vec3::ZERO);
            for (chain, other, angle) in [
                (self.fingers[side], self.thumbs[side][2], 0.70f32),
                (self.thumbs[side], self.fingers[side][2], 0.30),
            ] {
                for k in 0..2 {
                    let pivot = at(&matrices, chain[k]);
                    let along = (at(&matrices, chain[k + 1]) - pivot).normalize_or_zero();
                    let toward = at(&matrices, other) - pivot;
                    let axis = along.cross(toward - along * toward.dot(along)).normalize_or_zero();
                    if axis.length_squared() < 0.5 { continue; }
                    let curl = Mat4::from_translation(pivot)
                        * Mat4::from_axis_angle(axis, angle)
                        * Mat4::from_translation(-pivot);
                    for (i, m) in matrices.iter_mut().enumerate() {
                        if self.descendant(i, chain[k]) {
                            *m = curl * *m;
                        }
                    }
                }
            }
        }
        let deltas = matrices
            .iter()
            .zip(&self.bones)
            .map(|(m, b)| *m * b.base.inverse())
            .collect::<Vec<_>>();
        let vertices = self
            .arm_map
            .iter()
            .map(|&i| {
                let mut v = self.vertices[i];
                let mut p = Vec3::ZERO;
                let mut n = Vec3::ZERO;
                let mut t = Vec3::ZERO;
                for &(joint, w) in &self.weights[i] {
                    p += deltas[joint].transform_point3(Vec3::from(v.pos)) * w;
                    n += deltas[joint].transform_vector3(Vec3::from(v.normal)) * w;
                    t += deltas[joint].transform_vector3(Vec3::new(
                        v.tangent[0],
                        v.tangent[1],
                        v.tangent[2],
                    )) * w;
                }
                v.pos = p.to_array();
                v.normal = n.normalize_or_zero().to_array();
                let t = t.normalize_or_zero();
                v.tangent = [t.x, t.y, t.z, v.tangent[3]];
                v
            })
            .collect();
        (vertices, error)
    }
}
impl Rig {
    /// Posed arm-page triangles against their source: (longest posed edge in metres, worst
    /// posed/source edge-length ratio, number of edges stretched past 1.5x their source).
    fn arm_page_stretch(&self, posed: &[Vertex]) -> (f32, f32, usize) {
        let (mut longest, mut ratio, mut stretched) = (0.0f32, 0.0f32, 0usize);
        for t in self.page_indices[0].chunks_exact(3) {
            for k in 0..3 {
                let (a, b) = (t[k] as usize, t[(k + 1) % 3] as usize);
                let posed_len = Vec3::from(posed[a].pos).distance(Vec3::from(posed[b].pos));
                let source_len = Vec3::from(self.vertices[self.arm_map[a]].pos)
                    .distance(Vec3::from(self.vertices[self.arm_map[b]].pos));
                longest = longest.max(posed_len);
                let r = posed_len / source_len.max(1.0e-4);
                ratio = ratio.max(r);
                if r > 1.5 && posed_len > 0.02 { stretched += 1; }
            }
        }
        (longest, ratio, stretched)
    }
}
/// Which source pose the player's own body takes: standing for the dismounted walker and the two
/// standing turret posts (the cupola commander and the loader, as the third-person crew stand
/// there), seated for the gunner, driver and radio operator.
fn first_person_pose(dismounted: bool, station: Station) -> usize {
    if dismounted || matches!(station, Station::Commander | Station::Loader) { STANDING } else { SEATED }
}
/// The dismounted walker's stature: the uniform scale that puts the standing source's eyes at the
/// simulated walker's eye height `foot::EYE` (1.70 m; the source is normalized to 1.53 m overall).
fn walker_stature_scale(rig: &Rig) -> f32 {
    crate::foot::EYE / rig.eyes.y
}
/// NECK_FLEXION_EYE_TRAVEL: looking down bends the neck, which carries the eyes forward over
/// the chest. The neck takes `NECK_FLEXION_SHARE` of the view's depression, up to the cervical
/// flexion range `NECK_FLEXION_MAX`; the head turns about the source `Neck_N1` joint. Only the
/// eyes' forward travel is applied (the body moves back under the camera by it, 4.4 cm at most in
/// source units): the game owns the eye height, so the drop that real flexion also brings is not
/// modelled. Without it the source eyes sit straight over the flat chest front of this low-poly
/// jacket and a downward look meets only the collar shelf.
const NECK_FLEXION_SHARE: f32 = 0.6;
const NECK_FLEXION_MAX: f32 = 0.80;
impl Rig {
    fn eye_with_neck_flexion(&self, depression: f32) -> Vec3 {
        let neck = self.joint(self.neck);
        let flexion = (NECK_FLEXION_SHARE * depression.max(0.0)).min(NECK_FLEXION_MAX);
        let flexed = neck + Quat::from_rotation_x(flexion) * (self.eyes - neck);
        self.eyes + Vec3::Z * (flexed.z - self.eyes.z)
    }
}
fn first_person_grips_changed(old:[Option<Mat4>;2],new:[Option<Mat4>;2])->bool {
    old.into_iter().zip(new).any(|(a,b)|match(a,b) {
        (None,None)=>false,
        (Some(a),Some(b))=>!a.abs_diff_eq(b,1e-6),
        _=>true,
    })
}

#[cfg(test)]
mod grip_cadence_tests {
    use super::*;
    #[test]
    fn moving_or_released_first_person_grips_cannot_wait_for_idle_skin_tick() {
        let old=[Some(Mat4::IDENTITY),None];
        assert!(!first_person_grips_changed(old,old));
        assert!(first_person_grips_changed(old,[Some(Mat4::from_translation(Vec3::X*0.001)),None]));
        assert!(first_person_grips_changed(old,[Some(Mat4::from_rotation_z(0.01)),None]));
        assert!(first_person_grips_changed(old,[None,None]));
        // The comparison is source-relative: a rigid world translation of both operator
        // and mechanism leaves the local grip unchanged, so does not reskin idle hands.
        let world=Mat4::from_rotation_translation(Quat::from_rotation_y(0.4),Vec3::new(20.,1.,30.));
        let recovered=world.inverse()*(world*Mat4::IDENTITY);
        assert!(!first_person_grips_changed(old,[Some(recovered),None]));
    }
}

impl FirstPerson {
    pub(super) fn append(scene: &mut Vec<CpuMesh>) -> anyhow::Result<Self> {
        let (seat, seat_mesh) = Rig::load(SEATED)?;
        let (stand, stand_mesh) = Rig::load(STANDING)?;
        // The same man seen by any other camera: the whole standing source, head included.
        let witness = crate::gltf_load::load_glb(&static_glb(POSES[STANDING]))?;
        let witness_indices = witness.indices.clone();
        let witness_ownership = stand
            .weights
            .iter()
            .map(|weights| {
                let mut w = [0.0; 11];
                for &(b, v) in weights {
                    w[living_crew_wounds::bone_part(&stand.bones[b].name)] += v;
                }
                w
            })
            .collect();
        let witness_slot = scene.len();
        scene.push(witness);
        let slots = [seat_mesh, stand_mesh].map(|meshes| {
            meshes.map(|m| {
                let slot = scene.len();
                scene.push(m);
                slot
            })
        });
        Ok(Self {
            rigs: [seat, stand],
            witness_slot,
            witness_indices,
            witness_ownership,
            witness_mask: std::cell::Cell::new(u16::MAX),
            knobs: [
                handwheel_knob("assets/tanks/tiger/baked/tiger_part_wheel1.glb", 0, 1.0)?,
                handwheel_knob("assets/tanks/tiger/baked/tiger_part_wheel2.glb", 2, -1.0)?,
            ],
            last_turret_yaw: std::cell::Cell::new(f32::NAN),
            slots,
            last_sample: std::cell::Cell::new(-100.0),
            last_grips: std::cell::Cell::new((usize::MAX, [None; 2])),
            local_visibility: std::cell::Cell::new([[false; 2]; 2]),
            desired_hand: std::cell::Cell::new(None),
            last_error: std::cell::Cell::new(0.0),
            last_mask: std::cell::Cell::new((usize::MAX, 0)),
        })
    }
    pub(super) fn mark(&self, r: &mut Renderer) {
        r.mark_raster_only(self.witness_slot);
        r.set_visible(self.witness_slot, false);
        for slot in self.slots.into_iter().flatten() {
            r.mark_raster_only(slot);
            r.mark_temporal_dynamic(slot);
            r.set_visible(slot, false);
        }
    }
    pub(super) fn clear(&self, r: &mut Renderer) {
        self.local_visibility.set([[false; 2]; 2]);
        r.set_visible(self.witness_slot, false);
        for slot in self.slots.into_iter().flatten() {
            r.set_visible(slot, false);
        }
        self.desired_hand.set(None);
    }
    pub(super) fn hide_witness(&self, r: &mut Renderer) {
        r.set_visible(self.witness_slot, false);
    }
    pub(super) fn draw_commander_witness(
        &self,
        model: Mat4,
        missing: [bool; 11],
        r: &mut Renderer,
    ) {
        let bits = missing
            .iter()
            .enumerate()
            .fold(0u16, |a, (i, b)| a | ((*b as u16) << i));
        if self.witness_mask.get() != bits {
            let indices = living_crew_wounds::mask_indices(
                &self.witness_indices,
                &self.witness_ownership,
                missing,
            );
            if r.upload_pose_indices(self.witness_slot, &indices) {
                self.witness_mask.set(bits);
            }
        }
        r.set_model(self.witness_slot, model);
        r.set_visible(self.witness_slot, true);
    }
    /// Review harness: where the drawn hands' grips are in the world, if the hands are posed.
    pub(super) fn debug_grip_points(&self, r: &Renderer) -> [Option<Vec3>; 2] {
        let (_, grips) = self.last_grips.get();
        let Some(model) = self.debug_drawn_model(r) else { return [None, None] };
        grips.map(|g| g.map(|m| model.transform_point3(m.w_axis.truncate())))
    }
    /// Review harness: the world model of the drawn local body, if one is drawn.
    pub(super) fn debug_drawn_model(&self, r: &Renderer) -> Option<Mat4> {
        let visible = self.local_visibility.get();
        (0..2).find(|&p| visible[p][1]).and_then(|p| r.debug_mesh_state(self.slots[p][1])).map(|(_, m)| m)
    }
    /// Review harness: the local arm/body page slots (seated, standing) and the witness slot.
    pub(super) fn debug_slots(&self) -> (Vec<usize>, usize) {
        (self.slots.into_iter().flatten().collect(), self.witness_slot)
    }
    pub(super) fn hold_pistol(&self, gun: Mat4, magazine: Mat4) {
        self.desired_hand.set(Some((gun, magazine)));
    }
    /// The local body must not appear in the gunner's exterior optic image. On the shader
    /// backend it is excluded from that one raster and stays a caster everywhere, so nothing is
    /// returned to restore and no shadow or visibility cache sees a change. Other backends keep
    /// the visibility toggle, restored by the caller.
    pub(super) fn hide_for_optic(&self, r: &mut Renderer) -> Vec<(usize, bool)> {
        if r.shader_backend {
            for slot in self.slots.into_iter().flatten() { r.set_optic_hidden(slot, true); }
            return Vec::new();
        }
        self.hide_all(r)
    }
    /// Hide the local body outright (review captures); returns the visibility to restore.
    pub(super) fn hide_all(&self, r: &mut Renderer) -> Vec<(usize, bool)> {
        let visible = self.local_visibility.get();
        self.slots
            .iter()
            .enumerate()
            .flat_map(|(pose, slots)| {
                slots
                    .iter()
                    .enumerate()
                    .map(move |(page, &slot)| (slot, visible[pose][page]))
            })
            .inspect(|(slot, _)| r.set_visible(*slot, false))
            .collect()
    }
    pub(super) fn draw(&self, game: &Game, dt: f32, r: &mut Renderer) {
        self.local_visibility.set([[false; 2]; 2]);
        for slot in self.slots.into_iter().flatten() {
            r.set_visible(slot, false);
        }
        // On foot the walker's own life decides; the abandoned Tiger may already be a wreck.
        let alive = if game.mode == Mode::Foot { game.foot.hp > 0.0 } else { game.player.alive };
        if !game.singleplayer_crew_active || !alive {
            return;
        }
        let transition = game.climb.active || game.switch_t > 0.0;
        let pose = first_person_pose(game.mode == Mode::Foot || game.climb.active, game.station);
        let rig = &self.rigs[pose];
        let missing = game
            .singleplayer_embodied_role()
            .map(|s| {
                game.living_wounds
                    .missing(game.singleplayer_casualty_identity(s).peer_id)
            })
            .unwrap_or([false; 11]);
        let bits = missing
            .iter()
            .enumerate()
            .fold(0u16, |a, (i, b)| a | ((*b as u16) << i));
        if self.last_mask.get() != (pose, bits) {
            for page in 0..2 {
                let indices = living_crew_wounds::mask_indices(
                    &rig.page_indices[page],
                    &rig.page_ownership[page],
                    missing,
                );
                r.upload_pose_indices(self.slots[pose][page], &indices);
            }
            self.last_mask.set((pose, bits));
        }

        // `Quat::from_rotation_y(a) * Z` is (sin a, 0, cos a): the source body's +Z front then
        // matches `Tank::forward` at the vehicle heading with no half-turn. The on-foot yaw is
        // the camera convention (forward (sin y, 0, -cos y)) and keeps its existing half-turn.
        // The former shared +PI seated every crew body facing the turret rear, putting the back
        // of the torso between the commander's eye and the forward instruments.
        let heading = if game.mode == Mode::Foot {
            // Foot/camera forward is (sin y, 0, -cos y) = from_rotation_y(PI - y) * Z. The
            // former `y + PI` mirrored X, so the body counter-rotated as the walker turned.
            std::f32::consts::PI - game.foot.yaw
        } else {
            match game.station {
                // Hull posts do not ride the turret ring.
                Station::Driver | Station::Radioman => game.player.heading,
                // The cupola and loader posts look all round; the operator turns with his view
                // rather than leaving a fixed torso in front of a rearward eye.
                Station::Commander | Station::Loader if !transition => {
                    let look = r.camera.forward();
                    if look.x.abs() + look.z.abs() > 1.0e-4 {
                        look.x.atan2(look.z)
                    } else {
                        game.player.heading + game.player.turret_yaw
                    }
                }
                _ => game.player.heading + game.player.turret_yaw,
            }
        };
        let rotation = Quat::from_rotation_y(heading);
        // The camera is the source man's own eyes (`Eyes_N1`); the body hangs from that point.
        // Crew posts use the source at its reviewed 1.53 m Tiger-interior size. Out of the tank
        // the walker's simulated eye is `foot::EYE`, so the dismounted body is the same source
        // scaled uniformly to that stature (`walker_stature_scale`), feet on his ground contact.
        let dismounted = game.mode == Mode::Foot || game.climb.active;
        let scale = if dismounted { walker_stature_scale(rig) } else { 1.0 };
        let eye = if game.climb.active {
            // Climb feet follow the same validated physical eye/perch path as finish_climb.
            r.camera.pos
        } else if game.mode == Mode::Foot {
            // Grounded foot body stays at the actual feet and stance, rather than chasing camera
            // look/bob. A crouching eye lowers the whole standing source with it (no crouch pose).
            game.foot.eye()
        } else if game.switch_t > 0.0 {
            r.camera.pos
        } else {
            // Body belongs to the physical operating post. The composite gun optic and port
            // focus move the render eye independently; translating the chest with that remote
            // eye places real source geometry across the optic entrance.
            game.station_eye(game.station)
        };
        let depression = (-r.camera.forward().y).clamp(0.0, 1.0).asin();
        let origin = eye - rotation * (rig.eye_with_neck_flexion(depression) * scale);
        let body = Mat4::from_scale_rotation_translation(Vec3::splat(scale), rotation, origin);
        let mut grips = [None, None];
        if transition {
            // Keep bounded authored neutral limbs attached to the moving body; mechanism
            // targets belong to a seated operating post and must not drag hands through armor.
        } else if let Some((gun, magazine)) = self.desired_hand.get() {
            grips[1] = Some(gun * Mat4::from_translation(Vec3::new(0.0, -0.047, -0.062)));
            grips[0] = Some(if game.foot.reload_t > 0.0 {
                magazine * Mat4::from_translation(Vec3::new(-0.010, -0.078, -0.073))
            } else {
                gun * Mat4::from_translation(Vec3::new(0.018, -0.043, -0.056))
            });
        } else if game.mode == Mode::Tank && game.station == Station::Driver {
            grips = game
                .driver_controls
                .hand_targets(game.player.hull_model())
                .map(|p| p.map(Mat4::from_translation));
        } else if game.carry_mesh.is_some()
            || game.carried_casing.is_some()
            || game.casing_pull > 0.0
        {
            grips = game
                .crew_loader_hand_targets()
                .map(|p| p.map(Mat4::from_translation));
        } else if game.mode == Mode::Tank && game.station == Station::Gunner {
            // The right hand on the elevation handwheel's crank knob, which turns with the gun
            // (the same wheel transform the renderer draws).
            let wheel = game.player.turret_model()
                * Mat4::from_translation(crate::interior_rig::WHEEL1_POS)
                * Mat4::from_axis_angle(Vec3::X, game.player.gun_pitch * 40.0);
            grips[1] = Some(Mat4::from_translation(wheel.transform_point3(self.knobs[0])));
        } else if game.mode == Mode::Tank && game.station == Station::Commander
            && !game.commander_outside && game.binocular_raise <= 0.0
            && (game.player.turret_yaw - self.last_turret_yaw.get()).abs() < 1.0e-4 {
            // Buttoned at the cupola with the turret at rest, the right hand rests on the crank
            // knob of the auxiliary traverse handwheel beside him. Under power traverse that wheel
            // spins about 10 turns a second (595 per turret turn), so the hand lets go.
            let wheel = game.player.turret_model()
                * Mat4::from_translation(crate::interior_rig::WHEEL2_POS)
                * Mat4::from_axis_angle(crate::interior_rig::COMMANDER_TRAVERSE_WHEEL_AXIS,
                    game.player.turret_yaw * crate::interior_rig::COMMANDER_TRAVERSE_TURNS_PER_TURRET_REV);
            grips[1] = Some(Mat4::from_translation(wheel.transform_point3(self.knobs[1])));
        }
        self.last_turret_yaw.set(game.player.turret_yaw);
        // Tools and cartridges move every simulation step. An idle skin cadence must not
        // leave the actual hands behind them; compare in the same body's local frame so
        // rigid vehicle/world motion alone does not request a new skin solve.
        let local_grips = grips.map(|grip| grip.map(|m| body.inverse() * m));
        let (old_pose, old_grips) = self.last_grips.get();
        if r.time - self.last_sample.get() >= INTERVAL || dt == 0.0
            || old_pose != pose || first_person_grips_changed(old_grips, local_grips) {
            let (vertices, error) = rig.pose(body, grips);
            r.upload_pose_vertices(self.slots[pose][0], &vertices);
            self.last_sample.set(r.time);
            self.last_grips.set((pose, local_grips));
            self.last_error.set(error);
        }
        r.set_model(self.slots[pose][0], body);
        r.set_model(self.slots[pose][1], body);
        r.set_visible(self.slots[pose][0], true);
        r.set_visible(self.slots[pose][1], true);
        let mut visible = [[false; 2]; 2];
        visible[pose] = [true; 2];
        self.local_visibility.set(visible);
    }
    /// Review measurement of the arm page as last skinned: the longest posed triangle edge and
    /// the worst posed/source edge ratio, re-solved from the stored source-relative grips.
    pub(super) fn audit(&self, _game: &Game, _r: &Renderer) -> serde_json::Value {
        let (pose, grips) = self.last_grips.get();
        if pose > 1 { return serde_json::json!({"drawn": false}); }
        let rig = &self.rigs[pose];
        let (vertices, error) = rig.pose(Mat4::IDENTITY, grips);
        let (longest, ratio, stretched) = rig.arm_page_stretch(&vertices);
        serde_json::json!({"drawn": self.local_visibility.get()[pose][0], "pose": POSES[pose],
            "grips": grips.map(|g| g.map(|m| m.w_axis.truncate().to_array())),
            "longest_edge_m": longest, "worst_edge_ratio": ratio, "edges_over_1_5x": stretched, "hand_error_m": error})
    }
    pub(super) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({"source":"AdrielCZ CC-BY-4.0 Panzertruppe face01 (black Panzer uniform)","owner_head_hidden":"OWNER_HEAD_HIDDEN","available":true,"idle_sample_hz":30,"mechanical_grasps":"refresh on changed source-relative grip","hand_error_m":self.last_error.get(),"compact_arm_vertices":self.rigs.iter().map(|r|r.arm_map.len()).collect::<Vec<_>>()})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn available() -> bool {
        crate::resource::exists(static_glb(POSES[SEATED]))
            && crate::resource::exists(static_glb(POSES[STANDING]))
    }
    fn hit(o: Vec3, d: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
        let (e1, e2) = (b - a, c - a);
        let p = d.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-9 { return None; }
        let s = o - a;
        let u = s.dot(p) / det;
        if !(0.0..=1.0).contains(&u) { return None; }
        let q = s.cross(e1);
        let v = d.dot(q) / det;
        if v < 0.0 || u + v > 1.0 { return None; }
        let t = e2.dot(q) / det;
        (t > 0.0).then_some(t)
    }
    /// The world grips the game hands to the rig for a drawn pistol at `pitch`, with the body
    /// placed exactly as `FirstPerson::draw` places it for the standing walker.
    fn pistol_grips(rig: &Rig, pitch: f32) -> (Mat4, Vec3, [Option<Mat4>; 2]) {
        let scale = walker_stature_scale(rig);
        let eye = Vec3::new(0.0, crate::foot::EYE, 0.0);
        let body = Mat4::from_scale_rotation_translation(Vec3::splat(scale), Quat::IDENTITY, eye - rig.eyes * scale);
        let forward = Quat::from_rotation_x(-pitch) * Vec3::Z;
        let rotation = crate::foot::held_weapon_rotation(forward, Vec3::Y);
        let gun = Mat4::from_rotation_translation(rotation, p08::aimed_origin(eye, forward, rotation));
        (body, eye, [
            Some(gun * Mat4::from_translation(Vec3::new(0.018, -0.043, -0.056))),
            Some(gun * Mat4::from_translation(Vec3::new(0.0, -0.047, -0.062))),
        ])
    }

    #[test]
    fn first_person_skin_names_only_skeleton_joints_with_unit_weight() {
        if !available() { return; }
        for pose in [SEATED, STANDING] {
            let (rig, meshes) = Rig::load(pose).unwrap();
            assert_eq!(rig.bones.len(), 39);
            assert_eq!(rig.weights.len(), rig.vertices.len());
            for (i, w) in rig.weights.iter().enumerate() {
                assert!(w.iter().all(|&(j, _)| j < rig.bones.len()), "vertex {i} names a joint past the skeleton");
                let sum: f32 = w.iter().map(|w| w.1).sum();
                assert!((sum - 1.0).abs() < 1e-4, "vertex {i} weights sum to {sum}");
            }
            for page in &meshes {
                assert!(page.indices.iter().all(|&i| (i as usize) < page.vertices.len()));
            }
            // The static body page holds no vertex an arm joint moves: such a vertex would stay
            // behind at its source position when the arm is posed.
            let arm_moved = |v: &Vertex| rig.vertices.iter().zip(&rig.weights)
                .filter(|(s, _)| s.pos == v.pos)
                .any(|(_, w)| w.iter().any(|&(b, x)| x > 0.0 && rig.arms.iter().any(|a| rig.descendant(b, a[0]))));
            let body_used: std::collections::BTreeSet<u32> = meshes[1].indices.iter().copied().collect();
            assert!(body_used.iter().all(|&i| !arm_moved(&meshes[1].vertices[i as usize])),
                "{}: an arm-weighted vertex is drawn on the static body page", POSES[pose]);
            assert!(meshes[0].texture.is_some() && meshes[1].texture.is_some());
        }
    }

    #[test]
    fn owner_head_rule_is_exactly_the_source_head_object() {
        if !available() { return; }
        for pose in [SEATED, STANDING] {
            let (rig, meshes) = Rig::load(pose).unwrap();
            let parts = crate::gltf_load::load_glb_partitioned_roots(
                &static_glb(POSES[pose]), &["PanzerTruppe_Head_N1_Static"]).unwrap();
            assert_eq!(parts.len(), 2);
            assert_eq!(rig.head_triangles, parts[1].indices.len() / 3, "the hidden set is not the head object");
            assert_eq!(meshes[0].indices.len() + meshes[1].indices.len(), parts[0].indices.len(),
                "every non-head source triangle is drawn on the arm or body page");
            // The camera point lies inside the hidden head object's bounds.
            let (lo, hi) = parts[1].vertices.iter().map(|v| Vec3::from(v.pos))
                .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
            assert!(rig.eyes.cmpge(lo).all() && rig.eyes.cmple(hi).all(), "eyes {:?} outside head {lo:?}..{hi:?}", rig.eyes);
        }
    }

    /// The owner's report (2026-11-02): looking down inside the Tiger filled the screen with
    /// large dark and orange shards. The camera hung 8 cm above the nisu body's neck joint, inside
    /// its collar and shoulders: its nearest drawn vertex was 7 cm from the eye and 207 of 800
    /// lower-hemisphere rays met body surfaces within 15 cm. From the source's own eyes (neck
    /// flexion included), with its head hidden, every look down the 64-degree view must meet the
    /// body no nearer than 10 cm, and a look at the feet must actually show the body.
    #[test]
    fn own_eyes_look_down_onto_the_body_not_from_inside_it() {
        if !available() { return; }
        for pose in [SEATED, STANDING] {
            let (rig, meshes) = Rig::load(pose).unwrap();
            let arms = rig.pose(Mat4::IDENTITY, [None, None]).0;
            let triangles = [(&arms, &meshes[0].indices), (&meshes[1].vertices, &meshes[1].indices)]
                .into_iter()
                .flat_map(|(v, idx)| idx.chunks_exact(3).map(|t| [0, 1, 2].map(|k| Vec3::from(v[t[k] as usize].pos))).collect::<Vec<_>>())
                .collect::<Vec<[Vec3; 3]>>();
            for depression in [0.0f32, 0.785, 1.2, 1.45] {
                let eye = rig.eye_with_neck_flexion(depression);
                let forward = Vec3::new(0.0, -depression.sin(), depression.cos());
                let right = forward.cross(Vec3::Y).normalize();
                let up = right.cross(forward);
                let half = (64f32.to_radians() * 0.5).tan();
                let (mut rays, mut on_body, mut nearest) = (0, 0, f32::MAX);
                for j in 0..36 {
                    for i in 0..64 {
                        let (sx, sy) = (i as f32 / 63.0 * 2.0 - 1.0, j as f32 / 35.0 * 2.0 - 1.0);
                        let d = (forward + right * sx * half * 16.0 / 9.0 + up * sy * half).normalize();
                        rays += 1;
                        let t = triangles.iter().filter_map(|&t| hit(eye, d, t)).fold(f32::MAX, f32::min);
                        if t < f32::MAX { on_body += 1; nearest = nearest.min(t); }
                    }
                }
                eprintln!("{} looking {depression:.2} rad down: eye {eye:?}, body fills {on_body}/{rays} of the view, nearest {nearest:.3} m",
                    POSES[pose]);
                assert!(nearest > 0.10, "{} at {depression}: drawn body {nearest:.3} m from the eye", POSES[pose]);
                if depression > 1.4 {
                    assert!(on_body * 8 > rays, "{}: looking at the feet no longer shows the body", POSES[pose]);
                }
            }
        }
    }

    /// No posed arm-page triangle may grow beyond its source by more than a bounded amount:
    /// exploded or mis-skinned triangles (a joint index into the wrong skeleton, unnormalised
    /// weights, a scaled hand matrix) show up as edges metres long.
    #[test]
    fn posed_first_person_arms_keep_source_sized_triangles() {
        if !available() { return; }
        for pose in [SEATED, STANDING] {
            let (rig, _) = Rig::load(pose).unwrap();
            let source = rig.arm_page_stretch(&rig.pose(Mat4::IDENTITY, [None, None]).0);
            assert!((source.1 - 1.0).abs() < 1e-3, "unposed arms differ from the source");
            let mut cases = Vec::new();
            for pitch in [0.4f32, 0.0, -0.4, -0.785, -1.2, -1.45] {
                cases.push(pistol_grips(&rig, pitch));
            }
            for (x, y, z) in [(0.3f32, 0.9, 0.4), (-0.3, 0.9, 0.4), (0.2, 1.1, 0.5), (0.0, 0.7, 0.3), (0.4, 1.3, 0.3), (-0.2, 0.5, 0.6), (0.0, 2.5, 3.0)] {
                let p = Mat4::from_translation(Vec3::new(x, y, z));
                cases.push((Mat4::IDENTITY, rig.eyes, [Some(p), None]));
                cases.push((Mat4::IDENTITY, rig.eyes, [None, Some(p)]));
            }
            for (body, eye, grips) in cases {
                let local = grips.map(|g| g.map(|m| body.inverse() * m));
                let (vertices, _) = rig.pose(Mat4::IDENTITY, local);
                let (longest, _, _) = rig.arm_page_stretch(&vertices);
                assert!(longest < source.0 * 1.6 + 0.02,
                    "{} posed arm edge {longest:.3} m against source {:.3} m", POSES[pose], source.0);
                let local_eye = body.inverse().transform_point3(eye);
                for v in &vertices {
                    let p = Vec3::from(v.pos);
                    assert!(p.is_finite() && p.distance(local_eye) < 1.25, "{} arm vertex {p:?} left the body", POSES[pose]);
                }
            }
        }
    }

    #[test]
    fn pistol_ik_preserves_imported_hand_scale_and_reaches_the_grip() {
        if !available() { return; }
        for pose in [SEATED, STANDING] {
            let (rig, _) = Rig::load(pose).unwrap();
            for side in 0..2 {
                let (scale, rotation, position) = rig.bones[rig.arms[side][2]].base.to_scale_rotation_translation();
                assert!(scale.min_element() > 0.008 && scale.max_element() < 0.010);
                let rebuilt = Mat4::from_scale_rotation_translation(scale, rotation, position);
                for (a, b) in rebuilt.to_cols_array().iter().zip(rig.bones[rig.arms[side][2]].base.to_cols_array()) {
                    assert!((a - b).abs() < 1e-5);
                }
            }
        }
        // The standing walker holds the pistol at the aimed point within reach of both hands.
        let (rig, _) = Rig::load(STANDING).unwrap();
        for pitch in [0.0f32, -0.4, -0.785] {
            let (body, _, grips) = pistol_grips(&rig, pitch);
            let (_, error) = rig.pose(body, grips);
            assert!(error < 0.03, "pitch {pitch}: hand {error:.3} m off the grip");
        }
    }

    #[test]
    fn crew_posts_choose_the_pose_the_third_person_crew_takes() {
        assert_eq!(first_person_pose(true, Station::Gunner), STANDING);
        for (station, pose) in [(Station::Commander, STANDING), (Station::Loader, STANDING),
            (Station::Gunner, SEATED), (Station::Driver, SEATED), (Station::Radioman, SEATED)] {
            assert_eq!(first_person_pose(false, station), pose);
        }
    }
}
impl Game {
    /// Remote optic entrance excludes only local observer geometry; interior render restores
    /// these exact visibility values. Physical witness crew and world bodies remain present.
    pub fn hide_first_person_for_optic(&self, r: &mut Renderer) -> Vec<(usize, bool)> {
        self.first_person
            .as_ref()
            .map(|arms| arms.hide_for_optic(r))
            .unwrap_or_default()
    }
    /// Review captures: remove the local body from the view entirely.
    pub fn hide_first_person_body(&self, r: &mut Renderer) -> Vec<(usize, bool)> {
        self.first_person.as_ref().map(|arms| arms.hide_all(r)).unwrap_or_default()
    }
}

impl FirstPerson {
    pub(crate) fn export_world_state(&self)->anyhow::Result<crate::expedition_world_save::StateFields> {
        let mut f=crate::expedition_world_save::StateFields::default();
        f.put("last_sample",&self.last_sample.get())?;f.put("desired_hand",&self.desired_hand.get())?;f.put("last_error",&self.last_error.get())?;Ok(f)
    }
    pub(crate) fn import_world_state(&self,mut f:crate::expedition_world_save::StateFields)->anyhow::Result<()> {
        self.last_sample.set(f.take("last_sample")?);self.desired_hand.set(f.take("desired_hand")?);self.last_error.set(f.take("last_error")?);f.finish()?;
        self.last_mask.set((usize::MAX,u16::MAX));self.witness_mask.set(u16::MAX);
        self.last_grips.set((usize::MAX,[None;2]));Ok(())
    }
    pub(crate) fn present_world_state(&self,g:&Game,r:&mut Renderer) {
        let sample=self.last_sample.get();let error=self.last_error.get();
        self.draw(g,0.,r);self.last_sample.set(sample);self.last_error.set(error);
    }
}
