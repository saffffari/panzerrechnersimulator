//! Original CC0 red-band crew identity with source-weighted local arms/body.
//! Same bounded two-link IK as mechanism crew arms; no weapon/gameplay authority.
use super::*;
use crate::gltf_load::Vertex;
const ROOT: &str = "assets/flavor/Characters/german-ww2-soldier-cc0";
const INTERVAL: f32 = 1.0 / 30.0;
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
    arm_map: Vec<usize>,
    page_indices: [Vec<u32>; 2],
    page_ownership: [Vec<[f32; 11]>; 2],
    neck: Vec3,
    palm_offsets: [Vec3; 2],
    source_hand_directions: [Vec3; 2],
}
pub(super) struct FirstPerson {
    rigs: [Rig; 2],
    slots: [[usize; 2]; 2],
    last_sample: std::cell::Cell<f32>,
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
    fn load(pose: &str) -> anyhow::Result<(Self, [CpuMesh; 2])> {
        let doc: serde_json::Value = serde_json::from_str(&crate::resource::read_to_string(
            format!("{ROOT}/animation/{pose}_first_person_bindings.v1.json"),
        )?)?;
        anyhow::ensure!(doc["schema"] == "panzer.source-original-first-person-bindings.v1");
        let source = crate::gltf_load::load_glb(&format!(
            "{ROOT}/baked/german_ww2_soldier_{pose}_static.glb"
        ))?;
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
        anyhow::ensure!(bones.len() == 49 && weights.len() == source.vertices.len());
        let index = |name: &str| {
            bones
                .iter()
                .position(|b| b.name == name)
                .ok_or_else(|| anyhow::anyhow!("missing original crew joint {name}"))
        };
        let arms = [
            [index("LeftArm")?, index("LeftForeArm")?, index("LeftHand")?],
            [
                index("RightArm")?,
                index("RightForeArm")?,
                index("RightHand")?,
            ],
        ];
        let neck = bones[index("Neck")?].base.transform_point3(Vec3::ZERO);
        let directions = [
            bones[index("LeftHandMiddle1")?]
                .base
                .transform_point3(Vec3::ZERO)
                - bones[arms[0][2]].base.transform_point3(Vec3::ZERO),
            bones[index("RightHandMiddle1")?]
                .base
                .transform_point3(Vec3::ZERO)
                - bones[arms[1][2]].base.transform_point3(Vec3::ZERO),
        ];
        let mut rig = Self {
            bones,
            weights,
            vertices: source.vertices.clone(),
            arms,
            arm_map: Vec::new(),
            page_indices: Default::default(),
            page_ownership: Default::default(),
            neck,
            palm_offsets: [Vec3::ZERO; 2],
            source_hand_directions: directions,
        };
        for side in 0..2 {
            let wrist = rig.bones[rig.arms[side][2]]
                .base
                .transform_point3(Vec3::ZERO);
            let points = rig
                .vertices
                .iter()
                .zip(&rig.weights)
                .filter(|(_, w)| w.iter().any(|&(b, v)| b == rig.arms[side][2] && v > 0.65))
                .map(|(v, _)| Vec3::from(v.pos))
                .collect::<Vec<_>>();
            anyhow::ensure!(!points.is_empty(), "original source palm missing");
            rig.palm_offsets[side] =
                points.iter().copied().sum::<Vec3>() / points.len() as f32 - wrist;
        }
        let arm_vertex = |i: usize| {
            rig.weights[i]
                .iter()
                .any(|&(b, w)| w > 0.35 && rig.arms.iter().any(|a| rig.descendant(b, a[0])))
        };
        let head = index_for(&rig.bones, "Neck");
        let near_head = |i: usize| {
            rig.weights[i]
                .iter()
                .any(|&(b, w)| w > 0.25 && rig.descendant(b, head))
        };
        let arms_idx = source
            .indices
            .chunks_exact(3)
            .filter(|t| t.iter().all(|&i| arm_vertex(i as usize)))
            .flatten()
            .copied()
            .collect();
        let body_idx = source
            .indices
            .chunks_exact(3)
            .filter(|t| {
                !t.iter().any(|&i| near_head(i as usize))
                    && !t.iter().all(|&i| arm_vertex(i as usize))
            })
            .flatten()
            .copied()
            .collect();
        let (arm_mesh, arm_map) = compact(&source, arms_idx);
        let (body_mesh, body_map) = compact(&source, body_idx);
        anyhow::ensure!(
            !arm_mesh.indices.is_empty() && arm_mesh.vertices.len() <= 4096,
            "original compact arms invalid"
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
        Ok((rig, [arm_mesh, body_mesh]))
    }
    fn pose(&self, body: Mat4, grips: [Option<Mat4>; 2]) -> (Vec<Vertex>, f32) {
        let inverse = body.inverse();
        let mut matrices = self.bones.iter().map(|b| b.base).collect::<Vec<_>>();
        let mut error: f32 = 0.0;
        for side in 0..2 {
            let Some(grip) = grips[side] else { continue };
            let arm = self.arms[side];
            let points = arm.map(|i| self.bones[i].base.transform_point3(Vec3::ZERO));
            let delta = Quat::from_rotation_arc(
                self.source_hand_directions[side].normalize(),
                Vec3::new(0.0, -0.89, -0.455).normalize(),
            );
            let hand_rotation = Quat::from_mat4(&grip) * delta;
            let wrist_world =
                grip.transform_point3(Vec3::ZERO) - hand_rotation * self.palm_offsets[side];
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
            let rotation = Quat::from_mat4(&inverse) * hand_rotation;
            // Original FBX bones retain their imported 0.00879023 scale. Dropping it
            // here expands every weighted hand/finger about 114x after base.inverse().
            // Extract rotation from the decomposed source matrix, never a scaled Mat4.
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
            let actual = body.transform_point3(solved[2]) + hand_rotation * self.palm_offsets[side];
            error = error.max(actual.distance(grip.transform_point3(Vec3::ZERO)));
            // Curl source finger bones around the unchanged grip. No replacement hands/texture.
            let prefix = if side == 0 { "LeftHand" } else { "RightHand" };
            for suffix in ["Index1", "Index2", "Middle1", "Middle2", "Thumb1"] {
                let name = format!("{prefix}{suffix}");
                let Some(joint) = self.bones.iter().position(|b| b.name == name) else {
                    continue;
                };
                let pivot = matrices[joint].transform_point3(Vec3::ZERO);
                let axis = matrices[joint].transform_vector3(Vec3::X).normalize();
                let curl = Mat4::from_translation(pivot)
                    * Mat4::from_axis_angle(axis, if suffix == "Thumb1" { 0.45 } else { 0.65 })
                    * Mat4::from_translation(-pivot);
                for (i, m) in matrices.iter_mut().enumerate() {
                    if self.descendant(i, joint) {
                        *m = curl * *m;
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
fn index_for(bones: &[Bone], name: &str) -> usize {
    bones.iter().position(|b| b.name == name).unwrap()
}
impl FirstPerson {
    pub(super) fn append(scene: &mut Vec<CpuMesh>) -> anyhow::Result<Self> {
        let (seat, seat_mesh) = Rig::load("seated")?;
        let (stand, stand_mesh) = Rig::load("relaxed")?;
        let witness = crate::gltf_load::load_glb(&format!(
            "{ROOT}/baked/german_ww2_soldier_relaxed_static.glb"
        ))?;
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
            slots,
            last_sample: std::cell::Cell::new(-100.0),
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
        let pose = usize::from(game.mode == Mode::Foot || game.climb.active);
        let rig = &self.rigs[pose];
        let missing = game
            .singleplayer_embodied_role()
            .map(|s| {
                game.living_wounds
                    .missing(singleplayer_crew::casualty_identity(s).identity)
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
        // Body belongs to the physical operating post. The composite gun optic and port
        // focus move the render eye independently; translating the chest with that remote
        // eye places real source geometry across the optic entrance.
        let eye_offset = Vec3::new(0.0, 0.080, 0.045);
        // Climb feet follow the same validated physical eye/perch path as finish_climb.
        // Grounded foot body stays at actual feet, rather than chasing camera look/bob.
        let origin = if game.climb.active {
            r.camera.pos - Vec3::Y * crate::foot::EYE
        } else if game.mode == Mode::Foot {
            // A crouching eye drops below the standing source shoulders; lower the same body
            // with it instead of leaving the camera inside the torso.
            game.foot.pos
                - Vec3::Y * (crate::foot::EYE - crate::foot::EYE_CROUCH) * game.foot.crouch_t
        } else {
            let physical_eye = if game.switch_t > 0.0 {
                r.camera.pos
            } else {
                game.station_eye(game.station)
            };
            physical_eye - rotation * (rig.neck + eye_offset)
        };
        let body = Mat4::from_rotation_translation(rotation, origin);
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
            let p = game
                .player
                .turret_model()
                .transform_point3(crate::interior_rig::WHEEL1_POS + Vec3::X * 0.13);
            grips[1] = Some(Mat4::from_translation(p));
        }
        if r.time - self.last_sample.get() >= INTERVAL || dt == 0.0 {
            let (vertices, error) = rig.pose(body, grips);
            r.upload_pose_vertices(self.slots[pose][0], &vertices);
            self.last_sample.set(r.time);
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
    pub(super) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({"source":"nisu CC0 original red-band LowpolySoldier","available":true,"sample_cap_hz":30,"hand_error_m":self.last_error.get(),"compact_arm_vertices":self.rigs.iter().map(|r|r.arm_map.len()).collect::<Vec<_>>()})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pistol_ik_preserves_imported_hand_scale_and_bounded_source_arm_geometry() {
        if !crate::resource::exists(format!("{ROOT}/baked/german_ww2_soldier_seated_static.glb")) {
            return;
        }
        for pose in ["seated", "relaxed"] {
            let (rig, _) = Rig::load(pose).unwrap();
            for side in 0..2 {
                let (scale, rotation, position) = rig.bones[rig.arms[side][2]]
                    .base
                    .to_scale_rotation_translation();
                assert!(scale.min_element() > 0.008 && scale.max_element() < 0.010);
                let reconstructed =
                    Mat4::from_scale_rotation_translation(scale, rotation, position);
                for (a, b) in reconstructed
                    .to_cols_array()
                    .iter()
                    .zip(rig.bones[rig.arms[side][2]].base.to_cols_array())
                {
                    assert!((a - b).abs() < 1e-5);
                }
            }
            let eye = rig.neck + Vec3::new(0.0, 0.080, 0.045);
            for pitch in [0.0_f32, 0.20, -0.35] {
                let forward = Quat::from_rotation_x(-pitch) * Vec3::Z;
                let rotation = crate::foot::held_weapon_rotation(forward, Vec3::Y);
                let gun = Mat4::from_rotation_translation(
                    rotation,
                    p08::aimed_origin(eye, forward, rotation),
                );
                let grips = [
                    Some(gun * Mat4::from_translation(Vec3::new(0.018, -0.043, -0.056))),
                    Some(gun * Mat4::from_translation(Vec3::new(0.0, -0.047, -0.062))),
                ];
                let (vertices, _) = rig.pose(Mat4::IDENTITY, grips);
                for vertex in vertices {
                    let p = Vec3::from_array(vertex.pos);
                    assert!(p.is_finite());
                    assert!(
                        p.distance(eye) < 1.25,
                        "{pose} pistol source arm expanded: {p:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn original_arm_pages_retain_source_texture_and_skin_weights() {
        if !crate::resource::exists(format!("{ROOT}/baked/german_ww2_soldier_seated_static.glb")) {
            return;
        }
        for pose in ["seated", "relaxed"] {
            let (rig, meshes) = Rig::load(pose).unwrap();
            assert!(meshes[0].texture.is_some());
            assert!(rig.arm_map.len() < 4096);
            assert_eq!(rig.bones.len(), 49);
            let (vertices, error) = rig.pose(Mat4::IDENTITY, [None, None]);
            assert_eq!(error, 0.0);
            for (vertex, &source) in vertices.iter().zip(&rig.arm_map) {
                assert!(
                    Vec3::from(vertex.pos).distance(Vec3::from(rig.vertices[source].pos)) < 1e-5
                );
            }
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
