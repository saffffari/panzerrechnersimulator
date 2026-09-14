//! Bounded source-weighted skinning of the reviewed seated crew. The mechanical state owns every
//! hand target; this module cannot load a cartridge, traverse a gun, or fire a weapon.
use super::*;
use crate::gltf_load::Vertex;

const INTERVAL: f32 = 1.0 / 30.0;
const BINDINGS: &str = include_str!("../assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/animation/seated_arm_bindings.v1.json");

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
    elevation_grip: Vec3,
}
#[derive(Clone)]
struct Pose {
    vertices: Vec<Vertex>,
    arms: [[Vec3; 3]; 2],
    targets: [Option<Vec3>; 2],
    active: bool,
}
pub(super) struct CrewArmAnimation {
    rig: Option<Rig>,
    poses: [Option<Pose>; 5],
    elapsed: f32,
    samples: u64,
    loader_preview: bool,
}

fn arm_post_enabled(post: crate::lan::LanRole, loader_preview: bool) -> bool {
    matches!(post, crate::lan::LanRole::Gunner | crate::lan::LanRole::Driver) || (post == crate::lan::LanRole::Loader && loader_preview)
}

/// Keep original weighted vertices/materials but draw only upper-arm/forearm/hand triangles.
/// The local head and torso can never enter the camera when looking down or behind.
pub(super) fn driver_arm_meshes() -> anyhow::Result<[CpuMesh;5]> {
    let rig=Rig::load()?;
    let is_arm=|index:usize| rig.weights[index].iter().any(|&(bone,weight)|
        weight>0.4 && rig.arms.iter().any(|arm|rig.descendant(bone,arm[0])));
    let mut meshes=Vec::new();
    for path in CREW_SEATED_GLBS {
        let mut mesh=crate::gltf_load::load_glb(path)?;
        anyhow::ensure!(mesh.vertices.len()==rig.vertices.len(),"driver arm source topology changed");
        mesh.indices=mesh.indices.chunks_exact(3).filter(|tri|tri.iter().all(|&i|is_arm(i as usize)))
            .flatten().copied().collect();
        anyhow::ensure!(!mesh.indices.is_empty(),"source arms absent");
        meshes.push(mesh);
    }
    Ok(meshes.try_into().ok().unwrap())
}

/// A closed two-link chain keeps its authored lengths even for unreachable hand requests. The
/// elbow pole keeps the elbow out of the chest and supplies a stable plane near full extension.
fn arm_chain(shoulder: Vec3, elbow: Vec3, hand: Vec3, target: Vec3, pole: Vec3) -> [Vec3; 3] {
    let upper = shoulder.distance(elbow);
    let lower = elbow.distance(hand);
    if !target.is_finite() || upper < 1e-5 || lower < 1e-5 {
        return [shoulder, elbow, hand];
    }
    let ray = (target - shoulder).normalize_or_zero();
    if ray.length_squared() < 0.5 {
        return [shoulder, elbow, hand];
    }
    let distance = target
        .distance(shoulder)
        .clamp((upper - lower).abs() + 0.001, upper + lower - 0.001);
    let along = (upper * upper - lower * lower + distance * distance) / (2.0 * distance);
    let height = (upper * upper - along * along).max(0.0).sqrt();
    let mut side = (pole - ray * pole.dot(ray)).normalize_or_zero();
    if side.length_squared() < 0.5 {
        side = ray.cross(Vec3::Z).normalize_or_zero();
    }
    if side.length_squared() < 0.5 {
        side = ray.cross(Vec3::X).normalize_or_zero();
    }
    [
        shoulder,
        shoulder + ray * along + side * height,
        shoulder + ray * distance,
    ]
}

impl Rig {
    fn load() -> anyhow::Result<Self> {
        let doc: serde_json::Value = serde_json::from_str(BINDINGS)?;
        anyhow::ensure!(doc["schema"] == "panzer.source-crew-arm-bindings.v1");
        let bones = doc["bones"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| {
                let matrix: [f32; 16] =
                    std::array::from_fn(|i| b["matrix"][i].as_f64().unwrap() as f32);
                Bone {
                    name: b["name"].as_str().unwrap().into(),
                    parent: b["parent"].as_u64().map(|i| i as usize),
                    base: Mat4::from_cols_array(&matrix),
                }
            })
            .collect::<Vec<_>>();
        anyhow::ensure!(bones.len() == 39 && bones.iter().all(|b| b.base.is_finite()));
        let weights = doc["weights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|w| {
                        (
                            w[0].as_u64().unwrap() as usize,
                            w[1].as_f64().unwrap() as f32,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mesh = crate::gltf_load::load_glb(CREW_SEATED_GLBS[0])?;
        anyhow::ensure!(weights.len() == mesh.vertices.len() && weights.len() == 914);
        anyhow::ensure!(weights.iter().all(|w| !w.is_empty()
            && w.len() <= 8
            && w.iter().all(|&(joint, weight)| joint < bones.len()
                && weight.is_finite()
                && weight >= 0.0)
            && (w.iter().map(|w| w.1).sum::<f32>() - 1.0).abs() < 1e-5));
        let index = |name: &str| {
            bones
                .iter()
                .position(|b| b.name == name)
                .ok_or_else(|| anyhow::anyhow!("missing source crew joint {name}"))
        };
        let arms = [
            [
                index("L_Arm_N1")?,
                index("L_Elbow_N1")?,
                index("L_Hand_N1")?,
            ],
            [
                index("R_Arm_N1")?,
                index("R_Elbow_N1")?,
                index("R_Hand_N1")?,
            ],
        ];
        let wheel = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_part_wheel1.glb")?;
        // Select the source grasp's outermost +X cap away from the hub. That is the source surface
        // facing the gunner, not an invented point floating beside the wheel.
        let cap_x = wheel
            .vertices
            .iter()
            .map(|v| Vec3::from_array(v.pos))
            .filter(|p| Vec2::new(p.y, p.z).length() > 0.06)
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let cap = wheel
            .vertices
            .iter()
            .map(|v| Vec3::from_array(v.pos))
            .filter(|p| Vec2::new(p.y, p.z).length() > 0.06 && p.x >= cap_x - 0.004)
            .collect::<Vec<_>>();
        anyhow::ensure!(!cap.is_empty(), "source elevation-wheel grip absent");
        let elevation_grip = cap.iter().copied().sum::<Vec3>() / (cap.len() as f32);
        Ok(Self {
            bones,
            weights,
            vertices: mesh.vertices,
            arms,
            elevation_grip,
        })
    }
    fn descendant(&self, mut bone: usize, parent: usize) -> bool {
        for _ in 0..self.bones.len() {
            if bone == parent {
                return true;
            }
            let Some(next) = self.bones[bone].parent else {
                return false;
            };
            bone = next;
        }
        false
    }
    fn neutral_arms(&self) -> [[Vec3; 3]; 2] {
        self.arms
            .map(|arm| arm.map(|i| self.bones[i].base.transform_point3(Vec3::ZERO)))
    }
    fn pose(&self, targets: [Option<Vec3>; 2]) -> Pose {
        let mut matrices = self.bones.iter().map(|b| b.base).collect::<Vec<_>>();
        let mut arms = self.neutral_arms();
        for side in 0..2 {
            let Some(target) = targets[side] else {
                continue;
            };
            let [shoulder, elbow, hand] = arms[side];
            let pole = Vec3::new(if side == 0 { 1.0 } else { -1.0 }, -0.45, -0.25);
            let solved = arm_chain(shoulder, elbow, hand, target, pole);
            let first = Mat4::from_translation(shoulder)
                * Mat4::from_quat(Quat::from_rotation_arc(
                    (elbow - shoulder).normalize(),
                    (solved[1] - shoulder).normalize(),
                ))
                * Mat4::from_translation(-shoulder);
            for (i, matrix) in matrices.iter_mut().enumerate() {
                if self.descendant(i, self.arms[side][0]) {
                    *matrix = first * *matrix;
                }
            }
            let moved_hand = first.transform_point3(hand);
            let second = Mat4::from_translation(solved[1])
                * Mat4::from_quat(Quat::from_rotation_arc(
                    (moved_hand - solved[1]).normalize(),
                    (solved[2] - solved[1]).normalize(),
                ))
                * Mat4::from_translation(-solved[1]);
            for (i, matrix) in matrices.iter_mut().enumerate() {
                if self.descendant(i, self.arms[side][1]) {
                    *matrix = second * *matrix;
                }
            }
            arms[side] = solved;
        }
        let deltas = matrices
            .iter()
            .zip(&self.bones)
            .map(|(pose, bone)| *pose * bone.base.inverse())
            .collect::<Vec<_>>();
        let vertices = self
            .vertices
            .iter()
            .zip(&self.weights)
            .map(|(source, weights)| {
                let mut vertex = *source;
                let mut p = Vec3::ZERO;
                let mut n = Vec3::ZERO;
                let mut t = Vec3::ZERO;
                for &(joint, weight) in weights {
                    p += deltas[joint].transform_point3(Vec3::from_array(source.pos)) * weight;
                    n += deltas[joint].transform_vector3(Vec3::from_array(source.normal)) * weight;
                    t += deltas[joint].transform_vector3(Vec3::new(
                        source.tangent[0],
                        source.tangent[1],
                        source.tangent[2],
                    )) * weight;
                }
                vertex.pos = p.to_array();
                vertex.normal = n.normalize_or_zero().to_array();
                let t = t.normalize_or_zero();
                vertex.tangent = [t.x, t.y, t.z, source.tangent[3]];
                vertex
            })
            .collect();
        Pose {
            vertices,
            arms,
            targets,
            active: targets.iter().any(Option::is_some),
        }
    }
}

impl CrewArmAnimation {
    pub(super) fn new() -> Self {
        let rig = match Rig::load() {
            Ok(rig) => Some(rig),
            Err(error) => {
                eprintln!("[crew] source arm animation unavailable; preserving reviewed seated pose: {error:#}");
                None
            }
        };
        Self {
            rig,
            poses: std::array::from_fn(|_| None),
            elapsed: INTERVAL,
            samples: 0,
            // Baseline witness measured 17–54cm Loader misses. Preserve the reviewed neutral
            // pose until body positioning/regripping is solved; never stretch arms to conceal it.
            loader_preview: std::env::var("KESSEL_LOADER_ARM_PREVIEW").as_deref() == Ok("1"),
        }
    }
}

impl Game {
    pub(super) fn drive_driver_first_person_arms(&self,r:&mut Renderer) {
        for slot in self.slots.driver_arms {r.set_visible(slot,false);}
        if !self.singleplayer_crew_active || self.mode!=Mode::Tank || self.station!=Station::Driver
            || self.switch_t>0.0 || self.climb.active || self.driver_visor_lean>0.01 {return;}
        let Some(viewer)=self.singleplayer_embodied_role() else {return;};
        let Some(body)=self.physical_crew_bodies(BloodFrame::Hull).into_iter()
            .find(|body|body.casualty.role==crate::lan::LanRole::Driver &&
                (couch_crew_role(body.casualty.identity).is_some_and(|role|role==viewer) ||
                 (body.casualty.identity==crate::lan::LanRole::Radio && viewer==CrewRole::Radioman))) else {return;};
        let identity=crew_identity_index(body.casualty.identity);
        if self.crew_arm_animation.poses[identity].as_ref().is_none_or(|pose|!pose.active) {return;}
        r.set_model(self.slots.driver_arms[identity],self.singleplayer_rig_model(body.casualty));
        r.set_visible(self.slots.driver_arms[identity],true);
    }
    /// Capture/QA receipt for actual mechanical contacts, not an independent animation clock.
    pub fn crew_arm_pose_receipt(&self) -> serde_json::Value {
        let actors = self.physical_crew_bodies(BloodFrame::Turret).into_iter().filter_map(|body| {
            let pose = self.crew_arm_animation.poses[crew_identity_index(body.casualty.identity)].as_ref()?;
            if !pose.active { return None; }
            let model = self.singleplayer_rig_model(body.casualty);
            let errors: Vec<_> = pose.targets.iter().enumerate().filter_map(|(side,target)| {
                target.map(|target| model.transform_point3(target)
                    .distance(model.transform_point3(pose.arms[side][2])))
            }).collect();
            Some(serde_json::json!({"identity":body.casualty.identity.label(),
                "post":body.casualty.role.label(),"hand_error_m":errors,"vertices":pose.vertices.len()}))
        }).collect::<Vec<_>>();
        serde_json::json!({"available":self.crew_arm_animation.rig.is_some(),"sample_cap_hz":30,
            "samples":self.crew_arm_animation.samples,"actors":actors,
            "loader_preview":self.crew_arm_animation.loader_preview})
    }

    pub(super) fn singleplayer_rig_model(&self, actor: CouchCrewCasualty) -> Mat4 {
        let frame = blood_frame_for_role(actor.role);
        let offset = blood_parent_model(&self.player, frame)
            .transform_vector3(self.singleplayer_body_offset(actor, frame));
        Mat4::from_translation(offset) * crew_live_model(&self.player, actor.role)
    }

    /// Exact current cartridge/case model is shared with the visible object. Hands sit on two
    /// opposite brass-side grasp points rather than following a separate canned loading clock.
    fn crew_loader_hand_targets(&self) -> [Option<Vec3>; 2] {
        let model = if self.carry_mesh.is_some() {
            Some(self.carried_round_model())
        } else if self.carried_casing.is_some() {
            Some(Mat4::from_rotation_translation(
                self.carry_rotation,
                self.carry_pos,
            ))
        } else if self.casing_pull > 0.0 && matches!(self.breech, Breech::Spent) {
            Some(spent_case_model(
                self.player.visual_gun_model(),
                self.casing_pull,
            ))
        } else {
            None
        };
        model.map_or([None, None], |m| {
            [
                Some(m.transform_point3(Vec3::new(0.052, -0.015, -0.15))),
                Some(m.transform_point3(Vec3::new(-0.052, -0.015, 0.12))),
            ]
        })
    }

    pub(super) fn advance_crew_arm_animation(&mut self, dt: f32, r: &mut Renderer) {
        if !self.singleplayer_crew_active || self.crew_arm_animation.rig.is_none() {
            return;
        }
        self.crew_arm_animation.elapsed += dt.clamp(0.0, 0.1);
        if self.crew_arm_animation.elapsed < INTERVAL {
            return;
        }
        self.crew_arm_animation.elapsed = self.crew_arm_animation.elapsed.rem_euclid(INTERVAL);
        self.crew_arm_animation.samples += 1;
        let bodies = self.physical_crew_bodies(BloodFrame::Turret);
        for body in bodies {
            let identity = crew_identity_index(body.casualty.identity);
            let post = body.casualty.role;
            let inverse = self.singleplayer_rig_model(body.casualty).inverse();
            let world_targets = if !arm_post_enabled(post, self.crew_arm_animation.loader_preview) {
                [None, None]
            } else if post == crate::lan::LanRole::Driver {
                self.driver_controls.hand_targets(self.player.hull_model())
            } else if post == crate::lan::LanRole::Loader {
                self.crew_loader_hand_targets()
            } else if post == crate::lan::LanRole::Gunner {
                let wheel = self.player.turret_model()
                    * Mat4::from_translation(crate::interior_rig::WHEEL1_POS)
                    * Mat4::from_axis_angle(Vec3::X, self.player.gun_pitch * 40.0);
                // The source wheel's grasp is on its rim, in the same rotating Y/Z plane. The
                // commander's separate traverse flywheel is not reachable from this gunner post.
                [
                    None,
                    Some(wheel.transform_point3(
                        self.crew_arm_animation.rig.as_ref().unwrap().elevation_grip,
                    )),
                ]
            } else {
                [None, None]
            };
            let targets = world_targets.map(|t| t.map(|p| inverse.transform_point3(p)));
            let active = targets.iter().any(Option::is_some);
            let previous = self.crew_arm_animation.poses[identity]
                .as_ref()
                .is_some_and(|pose| pose.active);
            if !active && !previous {
                continue;
            }
            let pose = self.crew_arm_animation.rig.as_ref().unwrap().pose(targets);
            r.upload_pose_vertices(self.slots.lan_crew[identity], &pose.vertices);
            if post == crate::lan::LanRole::Driver {r.upload_pose_vertices(self.slots.driver_arms[identity], &pose.vertices);}
            self.crew_arm_animation.poses[identity] = Some(pose);
        }
    }

    pub(super) fn pose_crew_arm_anatomy(
        &self,
        actor: CouchCrewCasualty,
        frame: BloodFrame,
        segments: &mut [crew_physics::CrewSegment; crew_physics::CREW_BODY_PART_COUNT],
    ) {
        if !self.singleplayer_crew_active || actor.role == crate::lan::LanRole::Commander {
            return;
        }
        let Some(pose) = &self.crew_arm_animation.poses[crew_identity_index(actor.identity)] else {
            return;
        };
        if !pose.active {
            return;
        }
        let model =
            blood_parent_model(&self.player, frame).inverse() * self.singleplayer_rig_model(actor);
        // The authoring source names L at +X. The existing casualty mesh partitions +X as Right;
        // preserve that established anatomical page convention so visible limbs never swap sides.
        for (side, parts) in [
            [
                crew_physics::CrewBodyPart::RightUpperArm,
                crew_physics::CrewBodyPart::RightForearm,
            ],
            [
                crew_physics::CrewBodyPart::LeftUpperArm,
                crew_physics::CrewBodyPart::LeftForearm,
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let points = pose.arms[side].map(|p| model.transform_point3(p));
            for (link, part) in parts.into_iter().enumerate() {
                let segment = &mut segments[part.index()];
                segment.pos = (points[link] + points[link + 1]) * 0.5;
                segment.rot = Quat::from_rotation_arc(
                    -Vec3::Y,
                    (points[link + 1] - points[link]).normalize(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loader_preview_is_off_by_default_without_disabling_gunner_contact() {
        use crate::lan::LanRole;
        for post in LanRole::ALL {
            assert_eq!(arm_post_enabled(post, false), matches!(post, LanRole::Gunner | LanRole::Driver));
            assert_eq!(
                arm_post_enabled(post, true),
                matches!(post, LanRole::Gunner | LanRole::Loader | LanRole::Driver)
            );
        }
    }
    #[test]
    fn arm_ik_reaches_real_target_without_changing_limb_lengths() {
        let a = Vec3::new(0.2, 1.0, 0.0);
        let b = a - Vec3::Y * 0.31;
        let c = b - Vec3::Y * 0.3;
        let target = a + Vec3::new(0.12, -0.25, 0.3);
        let result = arm_chain(a, b, c, target, Vec3::X);
        assert!(result[2].distance(target) < 1e-5);
        assert!((result[0].distance(result[1]) - 0.31).abs() < 1e-5);
        assert!((result[1].distance(result[2]) - 0.3).abs() < 1e-5);
        let clamped = arm_chain(a, b, c, a + Vec3::Z * 8.0, Vec3::X);
        assert!(clamped[2].distance(a) < 0.611);
        assert!(clamped.iter().all(|p| p.is_finite()));
    }
    #[test]
    fn driver_hands_reach_yoke_and_button_without_stretching() {
        let rig=Rig::load().unwrap();
        let mesh=driver_arm_meshes().unwrap();
        assert!(mesh[0].indices.len()>120 && mesh[0].indices.len()<2500);
        for steer in [-1.0,0.0,1.0] {
            let mut controls=driver_controls::Controls::default();
            for _ in 0..180 {controls.advance_steering(1.0/120.0,steer);controls.advance(1.0/120.0);}
            controls.request();
            for _ in 0..110 {
                controls.advance_steering(1.0/120.0,steer);controls.advance(1.0/120.0);
                let offset=Vec3::new(0.17,0.0,0.10)*controls.reach();
                let model=Mat4::from_translation(Vec3::new(0.535,0.675745,1.777748)+offset)
                    *Mat4::from_scale(Vec3::splat(0.8));
                let world=controls.hand_targets(Mat4::IDENTITY);
                let pose=rig.pose(world.map(|p|p.map(|p|model.inverse().transform_point3(p))));
                for side in 0..2 {
                    let error=model.transform_point3(pose.arms[side][2]).distance(world[side].unwrap());
                    assert!(error<0.005,"driver contact miss {error} steer{steer} reach{}",controls.reach());
                }
            }
        }
    }
    #[test]
    fn source_skin_preserves_neutral_geometry_materials_and_exact_hand_contacts() {
        let rig = Rig::load().unwrap();
        let neutral = rig.pose([None, None]);
        assert_eq!(rig.vertices.len(), 914);
        for (vertex, source) in neutral.vertices.iter().zip(&rig.vertices) {
            assert!(Vec3::from_array(vertex.pos).distance(Vec3::from_array(source.pos)) < 2e-5);
            assert_eq!(vertex.uv, source.uv);
        }
        let arms = rig.neutral_arms();
        let targets = [
            Some(arms[0][0] + Vec3::new(0.12, -0.2, 0.3)),
            Some(arms[1][0] + Vec3::new(-0.12, -0.2, 0.3)),
        ];
        let pose = rig.pose(targets);
        for side in 0..2 {
            assert!(pose.arms[side][2].distance(targets[side].unwrap()) < 1e-5);
        }
        assert!(
            pose.vertices
                .iter()
                .all(|v| Vec3::from_array(v.pos).is_finite()
                    && Vec3::from_array(v.normal).is_finite())
        );
        assert!(
            pose.vertices
                .iter()
                .zip(&neutral.vertices)
                .filter(|(a, b)| Vec3::from_array(a.pos).distance(Vec3::from_array(b.pos)) > 0.02)
                .count()
                > 100
        );
    }
}
