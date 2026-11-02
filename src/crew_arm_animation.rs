//! Bounded source-weighted skinning of the reviewed seated crew. The mechanical state owns every
//! hand target; this module cannot load a cartridge, traverse a gun, or fire a weapon.
use super::*;
use crate::gltf_load::Vertex;
#[path = "crew_balance.rs"]
mod balance;
#[path="crew_hatch_posture.rs"]
mod hatch_posture;

const INTERVAL: f32 = 1.0 / 30.0;
// Idle balance remains sampled. A changed mechanical grasp must not wait for that clock:
// the cartridge/control and these same skinned vertices are both contact authority.
fn mechanical_targets_changed(previous:[Option<Vec3>;2],current:[Option<Vec3>;2])->bool {
    previous.into_iter().zip(current).any(|(a,b)|match (a,b) {
        (None,None)=>false,
        (Some(a),Some(b))=>a.distance_squared(b)>1e-12,
        _=>true,
    })
}
fn hand_contact_errors(arms:[[Vec3;3];2],targets:[Option<Vec3>;2],model:Mat4)->Vec<f32> {
    targets.into_iter().enumerate().filter_map(|(side,target)|target.map(|target|
        model.transform_point3(target).distance(model.transform_point3(arms[side][2])))).collect()
}
const BINDINGS: &str = include_str!("../assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/animation/seated_arm_bindings.v1.json");
fn around(p:Vec3,q:Quat)->Mat4 {Mat4::from_translation(p)*Mat4::from_quat(q)*Mat4::from_translation(-p)}

struct Bone {
    name: String,
    parent: Option<usize>,
    base: Mat4,
}
struct Rig {
    indices: Vec<u32>,
    bones: Vec<Bone>,
    weights: Vec<Vec<(usize, f32)>>,
    vertices: Vec<Vertex>,
    arms: [[usize; 3]; 2],
    elevation_grip: Vec3,
    legs: [[usize;3];2],
    spine: usize,
    head: usize,
}
#[derive(Clone,serde::Serialize,serde::Deserialize)]
struct Pose {
    #[serde(with="saved_vertices")]
    vertices: Vec<Vertex>,
    arms: [[Vec3; 3]; 2],
    targets: [Option<Vec3>; 2],
    active: bool,
    deltas: Vec<Mat4>,
    legs: [[Vec3;3];2],
    standing: bool,
}
pub(super) struct CrewArmAnimation {
    rig: Option<Rig>,
    poses: [Option<Pose>; 5],
    elapsed: f32,
    samples: u64,
    loader_preview: bool,
    standing_rig: Option<Rig>,
    balance: [balance::Balance;5],
    balance_actors: [Option<u32>;5],
    previous_shots: Option<u32>,
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

pub(super) fn wounded_seated_indices(identity:usize,missing:[bool;11])->anyhow::Result<Vec<u32>> {
    wounded_pose_indices(identity,missing,false)
}
pub(super) fn wounded_pose_indices(identity:usize,missing:[bool;11],standing:bool)->anyhow::Result<Vec<u32>> {
    let rig=if standing{Rig::load_standing()?}else{Rig::load()?};
    let source=crate::gltf_load::load_glb(if standing{CREW_STANDING_GLBS[identity]}else{CREW_SEATED_GLBS[identity]})?;
    anyhow::ensure!(source.vertices.len()==rig.weights.len());
    let ownership=rig.weights.iter().map(|weights|{let mut w=[0.0;11];for &(b,v) in weights{w[living_crew_wounds::bone_part(&rig.bones[b].name)]+=v;}w}).collect::<Vec<_>>();
    Ok(living_crew_wounds::mask_indices(&source.indices,&ownership,missing))
}
/// A closed two-link chain keeps its authored lengths even for unreachable hand requests. The
/// elbow pole keeps the elbow out of the chest and supplies a stable plane near full extension.
pub(super) fn arm_chain(shoulder: Vec3, elbow: Vec3, hand: Vec3, target: Vec3, pole: Vec3) -> [Vec3; 3] {
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
        Self::load_pose(BINDINGS,CREW_SEATED_GLBS[0])
    }
    fn load_standing()->anyhow::Result<Self> {
        // Through the asset root (KESSEL_ASSET_ROOT), like every other source file: a checkout-
        // relative read found nothing from a worktree and silently dropped the standing bindings.
        let data=crate::resource::read_to_string("assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/animation/standing_arm_bindings.v1.json")?;
        Self::load_pose(&data,CREW_STANDING_GLBS[0])
    }
    fn load_pose(bindings:&str,path:&str)->anyhow::Result<Self> {
        let doc: serde_json::Value = serde_json::from_str(bindings)?;
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
        let mesh = crate::gltf_load::load_glb(path)?;
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
        let legs=[[index("L_Leg_N1")?,index("L_Knee_N1")?,index("L_Ankle_N1")?],
            [index("R_Leg_N1")?,index("R_Knee_N1")?,index("R_Ankle_N1")?]];
        let spine=index("Spine_1_N1")?;let head=index("Head_N1")?;
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
            indices:mesh.indices,
            bones,
            weights,
            vertices: mesh.vertices,
            arms,
            elevation_grip,
            legs,spine,head,
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
        self.body_pose(targets,Vec3::ZERO,Quat::IDENTITY,false)
    }
    fn branch(&self,matrices:&mut[Mat4],root:usize,transform:Mat4) {
        for (i,m) in matrices.iter_mut().enumerate() {if self.descendant(i,root) {*m=transform**m;}}
    }
    fn solve(&self,matrices:&mut[Mat4],chain:[usize;3],target:Vec3,pole:Vec3)->[Vec3;3] {
        let [a,b,c]=chain.map(|i|matrices[i].w_axis.truncate());
        let solved=arm_chain(a,b,c,target,pole);
        let first=around(a,Quat::from_rotation_arc((b-a).normalize_or_zero(),(solved[1]-a).normalize_or_zero()));
        self.branch(matrices,chain[0],first);
        let current=matrices[chain[2]].w_axis.truncate();
        let second=around(solved[1],Quat::from_rotation_arc((current-solved[1]).normalize_or_zero(),(solved[2]-solved[1]).normalize_or_zero()));
        self.branch(matrices,chain[1],second);solved
    }
    /// Search bounded waist flex, then intersect the exact arm spheres with a vertical
    /// pelvis translation. Boots stay planted; original links never stretch. The two actual
    /// brass contacts may trade hands when the loader turns around the cartridge.
    fn contact_posture(&self,targets:[Vec3;2])->Option<(f32,f32,bool)> {
        let max_crouch=self.legs.iter().map(|leg| {
            let [hip,knee,ankle]=leg.map(|i|self.bones[i].base.w_axis.truncate());
            hip.y-ankle.y-(hip.distance(knee)-knee.distance(ankle)).abs()-0.08-0.025
        }).fold(f32::INFINITY,f32::min);
        let spine=self.bones[self.spine].base.w_axis.truncate();
        for step in 0..=10 {
            let lean=step as f32*0.05;
            let turn=Quat::from_rotation_x(lean);
            for swapped in [false,true] {
                let targets=if swapped {[targets[1],targets[0]]}else{targets};
                let mut low=0.0_f32;let mut high=max_crouch;
                for (arm,target) in self.neutral_arms().into_iter().zip(targets) {
                    let shoulder=spine+turn*(arm[0]-spine)-Vec3::Y*0.025;
                    let reach=arm[0].distance(arm[1])+arm[1].distance(arm[2])-0.012;
                    let horizontal=Vec2::new(shoulder.x-target.x,shoulder.z-target.z).length_squared();
                    if horizontal>=reach*reach {high=-1.0;break;}
                    let vertical=(reach*reach-horizontal).sqrt();
                    low=low.max(shoulder.y-target.y-vertical);
                    high=high.min(shoulder.y-target.y+vertical);
                }
                if low<=high&&low.is_finite() {return Some((low.max(0.0),lean,swapped));}
            }
        }
        None
    }
    fn body_pose(&self, targets:[Option<Vec3>;2],sway:Vec3,head_upright:Quat,standing:bool)->Pose {
        self.body_pose_with_gait(targets,sway,head_upright,standing,None,0.0)
    }
    fn body_pose_with_gait(&self, targets:[Option<Vec3>;2],sway:Vec3,head_upright:Quat,standing:bool,gait:Option<(f32,Vec3)>,work_lean:f32)->Pose {
        let mut matrices = self.bones.iter().map(|b| b.base).collect::<Vec<_>>();
        let mut legs=self.legs.map(|leg|leg.map(|i|matrices[i].w_axis.truncate()));
        if standing {
            // Boots keep their exact source support position. The pelvis lowers and the
            // original-length thighs/shins bend to absorb vertical chassis acceleration.
            let shift=Vec3::new(sway.x*0.25,(-0.025+sway.y*0.65).min(-0.008),sway.z*0.25);
            for matrix in &mut matrices {*matrix=Mat4::from_translation(shift)**matrix;}
            for side in 0..2 {
                let chain=self.legs[side];let mut target=legs[side][2];
                if let Some((phase,direction))=gait {
                    let phase=(phase+side as f32*0.5).rem_euclid(1.0);
                    let (z,y)=if phase<0.6 {(0.15-phase*0.5,0.0)}else{
                        let p=(phase-0.6)/0.4;(-0.15+0.3*p*p*(3.0-2.0*p),(std::f32::consts::PI*p).sin()*0.045)};
                    target+=direction*z+Vec3::Y*y;
                }
                legs[side]=self.solve(&mut matrices,chain,target,Vec3::Z);
                let foot=chain[2];let at=matrices[foot].w_axis.truncate();
                let original=self.bones[foot].base.to_scale_rotation_translation().1;
                let current=matrices[foot].to_scale_rotation_translation().1;
                self.branch(&mut matrices,foot,around(at,original*current.inverse()));
            }
        }
        let spine=matrices[self.spine].w_axis.truncate();
        let lean=Quat::from_rotation_z((-sway.x/0.45).clamp(-0.15,0.15))
            *Quat::from_rotation_x((sway.z/0.45).clamp(-0.15,0.15)+work_lean);
        self.branch(&mut matrices,self.spine,around(spine,lean));
        let head=matrices[self.head].w_axis.truncate();
        self.branch(&mut matrices,self.head,around(head,head_upright*lean.inverse()));
        let mut arms=self.arms.map(|arm|arm.map(|i|matrices[i].w_axis.truncate()));
        for side in 0..2 {
            let Some(target) = targets[side] else {
                continue;
            };
            let pole = Vec3::new(if side == 0 { 1.0 } else { -1.0 }, -0.45, -0.25);
            arms[side]=self.solve(&mut matrices,self.arms[side],target,pole);
        }
        self.skin_pose(matrices,targets,standing)
    }
    fn blend_bones(&self,a:&[Mat4],b:&[Mat4],t:f32)->Vec<Mat4> {
        let mut out=Vec::with_capacity(self.bones.len());
        for i in 0..self.bones.len() {
            let parent=self.bones[i].parent;
            let la=parent.map_or(a[i],|p|a[p].inverse()*a[i]);
            let lb=parent.map_or(b[i],|p|b[p].inverse()*b[i]);
            let (sa,qa,pa)=la.to_scale_rotation_translation();let (sb,qb,pb)=lb.to_scale_rotation_translation();
            let local=Mat4::from_scale_rotation_translation(sa.lerp(sb,t),qa.normalize().slerp(qb.normalize(),t),pa.lerp(pb,t));
            out.push(parent.map_or(local,|p|out[p]*local));
        }
        out
    }
    fn transfer_pose(&self,seated:&Rig,spec:crew_assignment::TransferPose,initial:&[Mat4])->Pose {
        let standing=self.body_pose_with_gait([None,None],Vec3::new(0.0,-spec.crouch/0.65,0.0),Quat::IDENTITY,true,
            spec.walking.then_some((spec.travel/0.5,spec.direction)),0.0);
        let stand_matrices=standing.deltas.iter().zip(&self.bones).map(|(d,b)|*d*b.base).collect::<Vec<_>>();
        let seat_matrices=seated.bones.iter().map(|b|b.base).collect::<Vec<_>>();
        let target=self.blend_bones(&seat_matrices,&stand_matrices,spec.standing.clamp(0.0,1.0));
        let matrices=if initial.len()==self.bones.len()&&spec.relax<1.0 {self.blend_bones(initial,&target,spec.relax.clamp(0.0,1.0))}else{target};
        let mut pose=self.skin_pose(matrices,[None,None],true);pose.active=true;pose
    }
    fn skin_pose(&self,matrices:Vec<Mat4>,targets:[Option<Vec3>;2],standing:bool)->Pose {
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
            arms:self.arms.map(|a|a.map(|i|matrices[i].w_axis.truncate())),
            targets,
            active: targets.iter().any(Option::is_some),
            deltas,legs:self.legs.map(|a|a.map(|i|matrices[i].w_axis.truncate())),standing,
        }
    }
}

impl CrewArmAnimation {
    pub(super) fn standing_available(&self)->bool {self.standing_rig.is_some()}
    /// Indexed positions last submitted to the living seated renderer. Collision consumers use
    /// the same sample, rather than predicting a different pose between the 30 Hz updates.
    pub(super) fn contact_pose(&self, identity:usize) -> Option<(u64, &[Vertex])> {
        self.poses.get(identity)?.as_ref().map(|pose|
            (self.samples, pose.vertices.as_slice()))
    }

    pub(super) fn new() -> Self {
        let rig = match Rig::load() {
            Ok(rig) => Some(rig),
            Err(error) => {
                eprintln!("[crew] source arm animation unavailable; preserving reviewed seated pose: {error:#}");
                None
            }
        };
        let standing_rig=Rig::load_standing().map_err(|error|
            eprintln!("[crew] source standing bindings unavailable: {error:#}")).ok();
        Self {
            rig,
            poses: std::array::from_fn(|_| None),
            elapsed: INTERVAL,
            samples: 0,
            // Legacy diagnostic flag retained in receipts. Standing source bindings now gate
            // normal loader skinning; unreachable contacts remain measurable, never stretched.
            loader_preview: std::env::var("KESSEL_LOADER_ARM_PREVIEW").as_deref() == Ok("1"),
            standing_rig,balance:[balance::Balance::default();5],balance_actors:[None;5],previous_shots:None,
        }
    }
}

impl Game {
    pub(in crate::game) fn crew_assignment_reset_pose_slots(&mut self) {
        for role in [CrewRole::Gunner,CrewRole::Loader] {
            let i=crew_identity_index(singleplayer_crew::body_post(role));
            self.crew_arm_animation.poses[i]=None;self.crew_arm_animation.balance_actors[i]=None;
        }
        self.crew_arm_animation.elapsed=INTERVAL;
    }
    pub(in crate::game) fn crew_assignment_initial_bones(&self,role:CrewRole)->Option<Vec<Mat4>> {
        let index=crew_identity_index(singleplayer_crew::body_post(role));
        let pose=self.crew_arm_animation.poses[index].as_ref()?;
        let rig=if pose.standing {self.crew_arm_animation.standing_rig.as_ref()?}else{self.crew_arm_animation.rig.as_ref()?};
        Some(pose.deltas.iter().zip(&rig.bones).map(|(d,b)|*d*b.base).collect())
    }
    pub(in crate::game) fn crew_assignment_source_vertices(&self,spec:crew_assignment::TransferPose,initial:&[Mat4])->Option<Vec<Vec3>> {
        let rig=self.crew_arm_animation.standing_rig.as_ref()?;let seated=self.crew_arm_animation.rig.as_ref()?;
        Some(rig.transfer_pose(seated,spec,initial).vertices.into_iter().map(|v|Vec3::from_array(v.pos)).collect())
    }
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
            // Compare with the mechanism NOW, not only the old targets which produced this
            // skin. Otherwise a moving shell can leave the hands while the receipt says zero.
            let current=self.crew_arm_mechanical_targets(body.casualty.role)
                .map(|target|target.map(|point|model.inverse().transform_point3(point)));
            let errors=hand_contact_errors(pose.arms,current,model);
            let solved_errors=hand_contact_errors(pose.arms,pose.targets,model);
            Some(serde_json::json!({"identity":body.casualty.identity.label(),
                "post":body.casualty.role.label(),"hand_error_m":errors,"pose_target_error_m":solved_errors,"vertices":pose.vertices.len(),
                "standing":pose.standing,"balance_offset_m":self.crew_arm_animation.balance[crew_identity_index(body.casualty.identity)].offset.to_array()}))
        }).collect::<Vec<_>>();
        serde_json::json!({"available":self.crew_arm_animation.rig.is_some(),"idle_sample_cap_hz":30,
            "contact_sampling":"changed mechanical targets and active transfers refresh on their simulation tick",
            "samples":self.crew_arm_animation.samples,"actors":actors,
            "loader_preview":self.crew_arm_animation.loader_preview,"standing_source_available":self.crew_arm_animation.standing_rig.is_some(),
            "loader_work":self.crew_loader_work_receipt()})
    }

    pub(in crate::game) fn crew_loader_contact_posture(&self,origin:Vec3,yaw:f32,targets:[Vec3;2])->Option<(f32,f32,bool)> {
        let rig=self.crew_arm_animation.standing_rig.as_ref()?;
        let inverse=(self.player.turret_model()*Mat4::from_translation(origin)
            *Mat4::from_rotation_y(yaw)*Mat4::from_scale(Vec3::splat(1.05))).inverse();
        rig.contact_posture(targets.map(|p|inverse.transform_point3(p)))
    }
    pub(super) fn crew_standing_post(&self,post:crate::lan::LanRole)->bool {
        self.crew_assignment_role_in_transit(crew_solo::role_of_post(post)) || post==crate::lan::LanRole::Commander || (post==crate::lan::LanRole::Loader && self.crew_arm_animation.standing_rig.is_some())
    }

    pub(super) fn crew_helmet_animation(&self,actor:CouchCrewCasualty)->Mat4 {
        let Some(pose)=self.crew_arm_animation.poses[crew_identity_index(actor.identity)].as_ref() else{return Mat4::IDENTITY;};
        let rig=if pose.standing {self.crew_arm_animation.standing_rig.as_ref()} else {self.crew_arm_animation.rig.as_ref()};
        rig.map_or(Mat4::IDENTITY,|rig|pose.deltas[rig.head])
    }

    pub(super) fn singleplayer_rig_model(&self, actor: CouchCrewCasualty) -> Mat4 {
        let frame = blood_frame_for_role(actor.role);
        let offset = blood_parent_model(&self.player, frame)
            .transform_vector3(self.singleplayer_body_offset(actor, frame));
        Mat4::from_translation(offset) * self.singleplayer_base_rig_model(actor)
    }
    pub(super) fn singleplayer_base_rig_model(&self,actor:CouchCrewCasualty)->Mat4 {
        if let Some(model)=self.crew_assignment_actor_model(actor.peer_id) {return model;}
        if actor.role==crate::lan::LanRole::Gunner && self.singleplayer_crew_active {
            return self.player.turret_model()*crew_assignment::gunner_seat_pose().model();
        }
        if actor.role==crate::lan::LanRole::Loader && self.crew_arm_animation.standing_rig.is_some() {
            self.crew_loader_work_model()
        }else if self.singleplayer_crew_active&&actor.peer_id>=singleplayer_crew::SOLO_ACTOR_BASE+5 {self.replacement_post_pose(crew_solo::role_of_post(actor.role))}else{crew_live_model(&self.player,actor.role)}
    }

    /// Exact current cartridge/case model is shared with the visible object. Hands sit on two
    /// opposite brass-side grasp points rather than following a separate canned loading clock.
    pub(super) fn crew_loader_hand_targets(&self) -> [Option<Vec3>; 2] {
        if let Some(targets)=self.crew_loader_work_targets() {return targets;}
        if self.crew_arm_animation.standing_available()&&self.crew_solo_working(CrewRole::Loader) {
            return [None,None];
        }
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

    fn crew_arm_mechanical_targets(&self,post:crate::lan::LanRole)->[Option<Vec3>;2] {
        let standing=self.crew_standing_post(post)&&self.crew_arm_animation.standing_rig.is_some();
        if !arm_post_enabled(post,self.crew_arm_animation.loader_preview||standing) {return [None,None];}
        match post {
            crate::lan::LanRole::Driver=>self.driver_controls.hand_targets(self.player.hull_model()),
            crate::lan::LanRole::Loader=>self.crew_loader_hand_targets(),
            crate::lan::LanRole::Gunner=> {
                let wheel=self.player.turret_model()*Mat4::from_translation(crate::interior_rig::WHEEL1_POS)
                    *Mat4::from_axis_angle(Vec3::X,self.player.gun_pitch*40.0);
                [None,self.crew_arm_animation.rig.as_ref().map(|rig|wheel.transform_point3(rig.elevation_grip))]
            },
            _=>[None,None],
        }
    }

    pub(super) fn advance_crew_arm_animation(&mut self, dt: f32, r: &mut Renderer) {
        if !self.singleplayer_crew_active || self.crew_arm_animation.rig.is_none() {
            return;
        }
        let bodies=self.physical_crew_bodies(BloodFrame::Turret);
        let fired=self.crew_arm_animation.previous_shots.is_some_and(|previous|previous!=self.rounds_spent);
        self.crew_arm_animation.previous_shots=Some(self.rounds_spent);
        let hull=self.player.hull_model();let hull_inverse=hull.inverse();
        let ride=&self.player.ride;
        let linear=Vec3::new(ride.lateral_accel,ride.heave_accel,ride.along_accel);
        let alpha=Vec3::new(-ride.pitch_accel,0.0,ride.roll_accel);
        let omega=Vec3::new(-ride.pitch_rate,0.0,ride.roll_rate);
        let shell=if self.he_loaded {&crate::ballistics::SPRGR_88}else{&crate::ballistics::PZGR_39};
        // Projectile momentum alone: gas impulse and muzzle-brake efficiency are not guessed.
        let recoil=self.player.gun_model().transform_vector3(-Vec3::Z).normalize_or_zero()
            *(shell.mass_kg*shell.muzzle_velocity/crate::vehicle_spec::TIGER_I.mass_kg);
        for body in &bodies {
            let id=crew_identity_index(body.casualty.identity);let model=self.singleplayer_rig_model(body.casualty);
            let inverse=model.inverse();
            let point=hull_inverse.transform_point3(model.transform_point3(Vec3::new(0.0,0.75,0.0)));
            let acceleration=inverse.transform_vector3(hull.transform_vector3(balance::at_point(linear,alpha,omega,point)));
            if self.crew_arm_animation.balance_actors[id]!=Some(body.casualty.peer_id) {
                self.crew_arm_animation.balance[id]=balance::Balance::default();
                self.crew_arm_animation.balance_actors[id]=Some(body.casualty.peer_id);
                self.crew_arm_animation.poses[id]=None;
                self.crew_arm_animation.elapsed=INTERVAL;
            }
            let response=&mut self.crew_arm_animation.balance[id];
            if fired {response.impulse(inverse.transform_vector3(recoil));}
            response.step(acceleration,dt);
        }
        self.crew_arm_animation.elapsed += dt.clamp(0.0, 0.1);
        let regular_sample=self.crew_arm_animation.elapsed>=INTERVAL;
        if regular_sample {
            self.crew_arm_animation.elapsed=self.crew_arm_animation.elapsed.rem_euclid(INTERVAL);
        }
        let mut sampled=false;
        for body in bodies {
            let identity = crew_identity_index(body.casualty.identity);
            let post = body.casualty.role;
            if post==crate::lan::LanRole::Commander && self.crew_arm_animation.standing_rig.is_none() {continue;}
            if let Some((spec,initial))=self.crew_assignment_actor_pose(body.casualty.peer_id) {
                if let (Some(rig),Some(seated))=(self.crew_arm_animation.standing_rig.as_ref(),self.crew_arm_animation.rig.as_ref()) {
                    let pose=rig.transfer_pose(seated,spec,initial);
                    r.upload_pose_vertices(self.slots.commander_crew[identity],&pose.vertices);
                    self.crew_arm_animation.poses[identity]=Some(pose);
                    sampled=true;
                }
                continue;
            }
            let inverse = self.singleplayer_rig_model(body.casualty).inverse();
            let standing=self.crew_standing_post(post)&&self.crew_arm_animation.standing_rig.is_some();
            let rig=if standing {self.crew_arm_animation.standing_rig.as_ref().unwrap()}else{self.crew_arm_animation.rig.as_ref().unwrap()};
            let mut world_targets=self.crew_arm_mechanical_targets(post);
            let mechanical=world_targets.map(|t|t.map(|p|inverse.transform_point3(p)));
            let changed=self.crew_arm_animation.poses[identity].as_ref().is_none_or(|old|
                old.standing!=standing || mechanical_targets_changed(old.targets,mechanical));
            if !regular_sample&&!changed {continue;}
            sampled=true;
            // During a real lurch, free hands can brace against an actual source surface.
            // Fixed mechanical grips and a held cartridge take precedence.
            if self.crew_arm_animation.balance[identity].acceleration.length()>0.7 {
                let models=self.fire_surface_models();let contacts=self.cabin_fire.source_contacts(&models);
                let model=inverse.inverse();
                for side in 0..2 {if world_targets[side].is_none() {
                    let arm=rig.neutral_arms()[side];let shoulder=model.transform_point3(arm[0]);
                    let hand=model.transform_point3(arm[2]);
                    let direction=(hand-shoulder).normalize_or_zero();
                    if let Some(contact)=contacts.sweep_batch(&[[hand-direction*0.08,hand+direction*0.20]])[0].as_ref() {
                        let target=models[contact.frame].transform_point3(contact.point);
                        let reach=(arm[0].distance(arm[1])+arm[1].distance(arm[2]))*model.x_axis.truncate().length();
                        if shoulder.distance(target)<reach-0.01 {world_targets[side]=Some(target);}
                    }
                }}
            }
            let targets = world_targets.map(|t| t.map(|p| inverse.transform_point3(p)));
            let up=inverse.transform_vector3(Vec3::Y).normalize_or_zero();
            let correction=Quat::from_rotation_arc(Vec3::Y,up);
            let (axis,angle)=correction.to_axis_angle();
            let mut upright=Quat::from_axis_angle(axis,angle.min(0.22));
            if post==crate::lan::LanRole::Loader {
                if let Some(target)=self.crew_loader_gaze() {
                    let head=rig.bones[rig.head].base.w_axis.truncate();
                    let ray=(inverse.transform_point3(target)-head).normalize_or_zero();
                    let yaw=ray.x.atan2(ray.z).clamp(-0.60,0.60);
                    let pitch=(-ray.y).atan2(Vec2::new(ray.x,ray.z).length()).clamp(-0.40,0.40);
                    upright=Quat::from_rotation_y(yaw)*Quat::from_rotation_x(pitch)*upright;
                }
            }
            let mut sway=self.crew_arm_animation.balance[identity].offset;
            if post==crate::lan::LanRole::Loader&&standing {sway.y-=self.crew_loader_work_crouch()/0.65;}
            let gait=if post==crate::lan::LanRole::Loader {let (phase,moving,direction)=self.crew_loader_gait();moving.then_some((phase,direction))}else{None};
            let lean=if post==crate::lan::LanRole::Loader {self.crew_loader_work_lean()}else{0.0};
            let mut pose=rig.body_pose_with_gait(targets,sway,upright,standing,gait,lean);
            pose.active=true;
            r.upload_pose_vertices(if standing{self.slots.commander_crew[identity]}else{self.slots.lan_crew[identity]}, &pose.vertices);
            if post == crate::lan::LanRole::Driver {r.upload_pose_vertices(self.slots.driver_arms[identity], &pose.vertices);}
            self.crew_arm_animation.poses[identity] = Some(pose);
        }
        if sampled {self.crew_arm_animation.samples+=1;}
    }

    pub(super) fn pose_crew_arm_anatomy(
        &self,
        actor: CouchCrewCasualty,
        frame: BloodFrame,
        segments: &mut [crew_physics::CrewSegment; crew_physics::CREW_BODY_PART_COUNT],
    ) {
        if !self.singleplayer_crew_active {
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
        let Some(rig)=(if pose.standing {self.crew_arm_animation.standing_rig.as_ref()}else{self.crew_arm_animation.rig.as_ref()}) else{return;};
        let inv=model.inverse();
        for (part,name) in [(crew_physics::CrewBodyPart::Pelvis,"ROOT_N1"),
            (crew_physics::CrewBodyPart::Torso,"Chest_N1"),(crew_physics::CrewBodyPart::Head,"Head_N1")] {
            if let Some(joint)=rig.bones.iter().position(|b|b.name==name) {
                let delta=model*pose.deltas[joint]*inv;
                let segment=&mut segments[part.index()];
                segment.pos=delta.transform_point3(segment.pos);
                segment.rot=(delta.to_scale_rotation_translation().1*segment.rot).normalize();
            }
        }
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
        if pose.standing {
            for (side,parts) in [[crew_physics::CrewBodyPart::RightThigh,crew_physics::CrewBodyPart::RightShin],
                [crew_physics::CrewBodyPart::LeftThigh,crew_physics::CrewBodyPart::LeftShin]].into_iter().enumerate() {
                let points=pose.legs[side].map(|p|model.transform_point3(p));
                for (link,part) in parts.into_iter().enumerate() {
                    let segment=&mut segments[part.index()];segment.pos=(points[link]+points[link+1])*0.5;
                    segment.rot=Quat::from_rotation_arc(-Vec3::Y,(points[link+1]-points[link]).normalize_or_zero());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_mechanical_contact_bypasses_idle_pose_cadence() {
        let previous=[Some(Vec3::new(0.1,0.8,0.3)),None];
        assert!(!mechanical_targets_changed(previous,previous));
        assert!(mechanical_targets_changed(previous,[Some(previous[0].unwrap()+Vec3::Z*0.003),None]));
        assert!(mechanical_targets_changed(previous,[None,None]));
        assert!(mechanical_targets_changed([None,None],previous));
    }
    #[test]
    fn contact_receipt_detects_current_mechanism_motion_hidden_by_cached_target() {
        let arms=[[Vec3::ZERO,Vec3::Y*0.3,Vec3::Y*0.6];2];
        let previous=[Some(arms[0][2]),None];
        let current=[Some(arms[0][2]+Vec3::Z*0.02),None];
        let model=Mat4::from_rotation_translation(Quat::from_rotation_y(0.7),Vec3::new(4.0,2.0,-3.0))
            *Mat4::from_scale(Vec3::splat(1.05));
        assert!(hand_contact_errors(arms,previous,model)[0]<1e-6);
        assert!((hand_contact_errors(arms,current,model)[0]-0.021).abs()<1e-6);
        assert!(hand_contact_errors(arms,[None,None],model).is_empty());
    }
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
    #[test]
    fn chassis_response_preserves_control_contacts_and_source_materials() {
        let rig=Rig::load().unwrap();let arms=rig.neutral_arms();
        let targets=arms.map(|arm|Some(arm[0]+Vec3::new(0.0,-0.35,0.25)));
        for sway in [Vec3::ZERO,Vec3::new(0.045,-0.06,-0.04),Vec3::new(-0.05,0.02,0.05)] {
            let pose=rig.body_pose(targets,sway,Quat::from_rotation_x(0.1),false);
            for side in 0..2 {
                assert!(pose.arms[side][2].distance(targets[side].unwrap())<0.005);
                for link in 0..2 {
                    assert!((pose.arms[side][link].distance(pose.arms[side][link+1])-
                        arms[side][link].distance(arms[side][link+1])).abs()<1e-5);
                }
            }
            for (original,posed) in rig.vertices.iter().zip(pose.vertices) {assert_eq!(original.uv,posed.uv);}
        }
    }
    #[test]
    #[ignore = "requires private standing rig"]
    fn low_rack_contact_uses_source_crouch_without_arm_stretch() {
        let rig=Rig::load_standing().unwrap();
        let targets=[Vec3::new(0.043,0.36,0.33),Vec3::new(-0.043,0.345,0.40)];
        let (crouch,lean,swapped)=rig.contact_posture(targets).expect("source-reachable low rack");
        let targets=if swapped {[targets[1],targets[0]]}else{targets};
        assert!(crouch>0.35,"must exercise formerly capped crouch: {crouch}");
        let pose=rig.body_pose_with_gait(targets.map(Some),Vec3::new(0.0,-crouch/0.65,0.0),Quat::IDENTITY,true,None,lean);
        for side in 0..2 {
            assert!(pose.arms[side][2].distance(targets[side])<0.005);
            let original=rig.neutral_arms()[side];
            for link in 0..2 {assert!((pose.arms[side][link].distance(pose.arms[side][link+1])-
                original[link].distance(original[link+1])).abs()<1e-5);}
        }
        assert!(rig.contact_posture([Vec3::new(2.0,0.3,0.3);2]).is_none());
    }
    #[test]
    #[ignore = "requires private standing rig"]
    fn spent_case_extraction_keeps_exact_source_two_hand_reach() {
        let rig=Rig::load_standing().unwrap();
        for hz in [30,60,120] {for pitch in [-0.1_f32,0.0,0.2] {
            let dt=1.0/hz as f32;let home=crew_solo::CrewSolo::default().loader_work.origin;let mut origin=home;
            let mut yaw=std::f32::consts::FRAC_PI_2;let mut age=0.0_f32;let mut pull=0.0_f32;
            for _ in 0..hz*3 {
                age+=dt;let proposed=(age/0.8).min(1.0);
                let gun=Mat4::from_translation(crate::tank::TIGER_PARTS.gun_trunnion)*Mat4::from_rotation_x(-pitch);
                let case=spent_case_model(gun,proposed);
                let rear=-crate::procedural_ordnance::CASE_LENGTH_M*0.5+0.04;
                let targets=[case.transform_point3(Vec3::new(0.043,0.0,rear)),
                    case.transform_point3(Vec3::new(-0.043,-0.015,rear+0.22))];
                let grip=(targets[0]+targets[1])*0.5;
                let goal=clamp_interior_walk(origin,Vec3::new(home.x,origin.y,grip.z));
                origin=clamp_interior_walk(origin,origin+(goal-origin).clamp_length_max(0.9*dt));
                let direction=grip-origin;
                let delta=(direction.x.atan2(direction.z)-yaw+std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
                yaw+=delta.clamp(-3.0*dt,3.0*dt);
                let model=Mat4::from_translation(origin)*Mat4::from_rotation_y(yaw)*Mat4::from_scale(Vec3::splat(1.05));
                let local=targets.map(|p|model.inverse().transform_point3(p));
                if let Some((crouch,lean,swapped))=rig.contact_posture(local) {
                    let local=if swapped {[local[1],local[0]]}else{local};
                    let pose=rig.body_pose_with_gait(local.map(Some),Vec3::new(0.0,-crouch/0.65,0.0),Quat::IDENTITY,true,None,lean);
                    for side in 0..2 {
                        assert!(model.transform_point3(pose.arms[side][2]).distance(model.transform_point3(local[side]))<0.005,
                            "source case contact at {hz}Hz, pitch {pitch}, pull {proposed}");
                        let original=rig.neutral_arms()[side];
                        for link in 0..2 {assert!((pose.arms[side][link].distance(pose.arms[side][link+1])-
                            original[link].distance(original[link+1])).abs()<1e-5);}
                    }
                    pull=proposed;
                }else{age=(age-dt).max(0.0);}
                if pull>=1.0 {break;}
            }
            assert!(pull>=1.0,"normal spent case stalled at {pull}, {hz}Hz, pitch {pitch}");
        }}
    }
    #[test]
    #[ignore = "requires private standing rig"]
    fn ordinary_source_racks_admit_exact_two_hand_contact() {
        let rig=Rig::load_standing().unwrap();
        for source_id in [41,47,50,53,62] {
            let index=crate::interior_rig::shell_slot_from_source_id(source_id).unwrap();
            let (_,p,q)=crate::interior_rig::SHELL_SLOTS[index];
            let model=Mat4::from_translation(-crate::tank::TIGER_PARTS.turret_pivot)
                *Mat4::from_rotation_translation(Quat::from_array(q),p);
            let center=model.w_axis.truncate();
            let origin=clamp_interior_walk(Vec3::new(-0.54,crate::interior_rig::FLOOR_STAND_Y,0.08),
                Vec3::new(center.x+0.45,crate::interior_rig::FLOOR_STAND_Y,center.z-0.12));
            let world=[model.transform_point3(Vec3::new(0.043,0.0,-0.405)),
                model.transform_point3(Vec3::new(-0.043,-0.015,-0.185))];
            let direction=(world[0]+world[1])*0.5-origin;
            let body=Mat4::from_translation(origin)*Mat4::from_rotation_y(direction.x.atan2(direction.z))
                *Mat4::from_scale(Vec3::splat(1.05));
            let targets=world.map(|p|body.inverse().transform_point3(p));
            let (crouch,lean,swapped)=rig.contact_posture(targets).unwrap_or_else(||panic!("no real rack contact: source {source_id}"));
            let targets=if swapped {[targets[1],targets[0]]}else{targets};
            let pose=rig.body_pose_with_gait(targets.map(Some),Vec3::new(0.0,-crouch/0.65,0.0),Quat::IDENTITY,true,None,lean);
            for side in 0..2 {assert!(pose.arms[side][2].distance(targets[side])<0.005,"rack {source_id}");}
        }
    }
    #[test]
    #[ignore = "requires private standing rig"]
    fn standing_gait_follows_sideways_travel_without_changing_leg_lengths() {
        let rig=Rig::load_standing().unwrap();
        let pose=rig.body_pose_with_gait([None,None],Vec3::ZERO,Quat::IDENTITY,true,Some((0.15,Vec3::X)),0.0);
        for side in 0..2 {
            let original=rig.legs[side].map(|i|rig.bones[i].base.w_axis.truncate());
            assert!((pose.legs[side][2].z-original[2].z).abs()<0.002);
            assert!((pose.legs[side][2].x-original[2].x).abs()>0.02);
        }
    }
    #[test]
    fn standing_knees_absorb_motion_without_moving_planted_source_boots() {
        if !crate::resource::exists("assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/animation/standing_arm_bindings.v1.json"){return;}
        let rig=Rig::load_standing().unwrap();
        for sway in [Vec3::ZERO,Vec3::new(0.05,-0.06,0.04)] {
            let pose=rig.body_pose([None,None],sway,Quat::IDENTITY,true);
            for side in 0..2 {
                let original=rig.legs[side].map(|i|rig.bones[i].base.w_axis.truncate());
                assert!(pose.legs[side][2].distance(original[2])<0.001);
                for link in 0..2 {assert!((pose.legs[side][link].distance(pose.legs[side][link+1])-
                    original[link].distance(original[link+1])).abs()<1e-5);}
            }
        }
    }
}

// The sampled skin is contact authority between pose updates. Keep its exact float
// bits alongside joint deltas instead of resampling a different pose during resume.
mod saved_vertices {
    use super::Vertex;use serde::{Serialize,Deserialize};
    pub fn serialize<S:serde::Serializer>(vertices:&Vec<Vertex>,s:S)->Result<S::Ok,S::Error> {
        vertices.iter().map(|v|(v.pos,v.normal,v.uv,v.tangent)).collect::<Vec<_>>().serialize(s)
    }
    pub fn deserialize<'de,D:serde::Deserializer<'de>>(d:D)->Result<Vec<Vertex>,D::Error> {
        type Saved=([f32;3],[f32;3],[f32;2],[f32;4]);
        let values=Vec::<Saved>::deserialize(d)?;
        if values.len()>1200{return Err(serde::de::Error::custom("saved source pose exceeds vertex budget"));}
        Ok(values.into_iter().map(|(pos,normal,uv,tangent)|Vertex{pos,normal,uv,tangent}).collect())
    }
}
#[derive(serde::Serialize,serde::Deserialize)]
pub(super) struct Snapshot {
    poses:[Option<Pose>;5],elapsed:f32,samples:u64,loader_preview:bool,
    balance:[balance::Balance;5],balance_actors:[Option<u32>;5],previous_shots:Option<u32>,
    seated_source:bool,standing_source:bool,
}
impl Snapshot {
    pub(super) fn validate(&self)->anyhow::Result<()> {
        anyhow::ensure!(self.elapsed.is_finite()&&self.elapsed>=0.0&&self.elapsed<1.0,"invalid crew pose clock");
        for b in &self.balance {b.validate_world_state()?;}
        for p in self.poses.iter().flatten() {
            anyhow::ensure!(!p.vertices.is_empty()&&p.vertices.len()<=1200&&p.deltas.len()==39,"saved crew topology changed");
            anyhow::ensure!(p.deltas.iter().all(|m|m.is_finite())&&p.targets.iter().flatten().all(|p|p.is_finite())
                &&p.arms.iter().flatten().chain(p.legs.iter().flatten()).all(|p|p.is_finite()),"invalid saved crew joints");
            anyhow::ensure!(p.vertices.iter().all(|v|v.pos.iter().chain(&v.normal).chain(&v.uv).chain(&v.tangent).all(|x|x.is_finite())),"invalid saved skin");
        }Ok(())
    }
}
impl CrewArmAnimation {
    pub(super) fn snapshot(&self)->Snapshot {Snapshot{poses:self.poses.clone(),elapsed:self.elapsed,samples:self.samples,
        loader_preview:self.loader_preview,balance:self.balance,balance_actors:self.balance_actors,previous_shots:self.previous_shots,
        seated_source:self.rig.is_some(),standing_source:self.standing_rig.is_some()}}
    pub(super) fn restore(s:Snapshot)->anyhow::Result<Self> {
        s.validate()?;let mut state=Self::new();
        anyhow::ensure!(state.rig.is_some()==s.seated_source&&state.standing_rig.is_some()==s.standing_source,"saved crew source availability changed");
        for pose in s.poses.iter().flatten() {
            let rig=(if pose.standing{state.standing_rig.as_ref()}else{state.rig.as_ref()})
                .ok_or_else(||anyhow::anyhow!("saved crew pose has no source rig"))?;
            anyhow::ensure!(rig.vertices.len()==pose.vertices.len()&&rig.bones.len()==pose.deltas.len(),"saved crew source topology differs");
        }
        state.poses=s.poses;state.elapsed=s.elapsed;state.samples=s.samples;state.loader_preview=s.loader_preview;
        state.balance=s.balance;state.balance_actors=s.balance_actors;state.previous_shots=s.previous_shots;Ok(state)
    }
    pub(super) fn upload_saved_poses(&self,slots:&Slots,r:&mut Renderer) {
        for (i,pose) in self.poses.iter().enumerate() {if let Some(pose)=pose {
            r.upload_pose_vertices(if pose.standing{slots.commander_crew[i]}else{slots.lan_crew[i]},&pose.vertices);
            r.upload_pose_vertices(slots.driver_arms[i],&pose.vertices);
        }}
    }
}

impl CrewArmAnimation {
    pub(in crate::game) fn transport_pose_vertices(&self,standing:f32,phase:f32,moving:bool)->Option<Vec<Vertex>> {
        let seated=self.rig.as_ref()?;let rig=self.standing_rig.as_ref()?;
        let pose=rig.body_pose_with_gait([None,None],Vec3::ZERO,Quat::IDENTITY,true,moving.then_some((phase,Vec3::Z)),0.0);
        let upright=pose.deltas.iter().zip(&rig.bones).map(|(d,b)|*d*b.base).collect::<Vec<_>>();
        let sitting=seated.bones.iter().map(|b|b.base).collect::<Vec<_>>();
        Some(rig.skin_pose(rig.blend_bones(&sitting,&upright,standing.clamp(0.,1.)),[None,None],true).vertices)
    }
}

impl CrewArmAnimation {pub(in crate::game) fn transport_indices(&self)->Option<&[u32]> {self.standing_rig.as_ref().map(|rig|rig.indices.as_slice())}}

impl CrewArmAnimation {
    pub(in crate::game) fn transport_shape(&self,person:crate::expedition::PersonId,appearance:crate::lan::LanRole,model:Mat4,standing:f32,phase:f32,moving:bool)->Option<singleplayer_crew::field_people::Shape> {
        let rig=self.standing_rig.as_ref()?;let seated=self.rig.as_ref()?;
        let upright=rig.body_pose_with_gait([None,None],Vec3::ZERO,Quat::IDENTITY,true,moving.then_some((phase,Vec3::Z)),0.);
        let matrices=rig.blend_bones(&seated.bones.iter().map(|b|b.base).collect::<Vec<_>>(),&upright.deltas.iter().zip(&rig.bones).map(|(d,b)|*d*b.base).collect::<Vec<_>>(),standing.clamp(0.,1.));
        let pose=rig.skin_pose(matrices.clone(),[None,None],true);
        let names=rig.bones.iter().map(|b|b.name.as_str()).collect::<Vec<_>>();let bases=rig.bones.iter().map(|b|b.base).collect::<Vec<_>>();
        let parts=rig.weights.iter().map(|weights|{let mut vote=[0f32;11];for &(i,w) in weights {vote[singleplayer_crew::field_people::source_part(&rig.bones[i].name)]+=w;}
            (0..11).max_by(|&a,&b|vote[a].total_cmp(&vote[b])).unwrap()}).collect();
        Some(singleplayer_crew::field_people::Shape{person,appearance,model,vertices:pose.vertices,indices:rig.indices.clone(),parts,
            segments:singleplayer_crew::field_people::source_segments(model,&names,&bases,&matrices,rig.arms,rig.legs)})
    }
}

impl CrewArmAnimation {
    fn boarding_pose(&self,standing:f32,tuck:f32,phase:f32,moving:bool)->Option<Pose>{
        let rig=self.standing_rig.as_ref()?;let seated=self.rig.as_ref()?;
        let upright=rig.body_pose_with_gait([None,None],Vec3::ZERO,Quat::IDENTITY,true,moving.then_some((phase,Vec3::Z)),0.);
        let matrices=rig.blend_bones(&seated.bones.iter().map(|b|b.base).collect::<Vec<_>>(),&upright.deltas.iter().zip(&rig.bones).map(|(d,b)|*d*b.base).collect::<Vec<_>>(),standing.clamp(0.,1.));
        Some(rig.hatch_posture(&matrices,tuck,standing))
    }
    pub(in crate::game) fn boarding_pose_vertices(&self,standing:f32,tuck:f32,phase:f32,moving:bool)->Option<Vec<Vertex>>{Some(self.boarding_pose(standing,tuck,phase,moving)?.vertices)}
    pub(in crate::game) fn boarding_shape(&self,person:crate::expedition::PersonId,appearance:crate::lan::LanRole,model:Mat4,standing:f32,tuck:f32,phase:f32,moving:bool)->Option<singleplayer_crew::field_people::Shape>{
        let rig=self.standing_rig.as_ref()?;let pose=self.boarding_pose(standing,tuck,phase,moving)?;
        let matrices=pose.deltas.iter().zip(&rig.bones).map(|(d,b)|*d*b.base).collect::<Vec<_>>();
        let names=rig.bones.iter().map(|b|b.name.as_str()).collect::<Vec<_>>();let bases=rig.bones.iter().map(|b|b.base).collect::<Vec<_>>();
        let parts=rig.weights.iter().map(|weights|{let mut vote=[0f32;11];for &(i,w) in weights{vote[singleplayer_crew::field_people::source_part(&rig.bones[i].name)]+=w;}(0..11).max_by(|&a,&b|vote[a].total_cmp(&vote[b])).unwrap()}).collect();
        Some(singleplayer_crew::field_people::Shape{person,appearance,model,vertices:pose.vertices,indices:rig.indices.clone(),parts,
            segments:singleplayer_crew::field_people::source_segments(model,&names,&bases,&matrices,rig.arms,rig.legs)})
    }
}
