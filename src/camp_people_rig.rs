//! Source-weighted camp figures. Original joints, pivots, skin weights, UVs and topology;
//! authored gait/gesture targets, not a motion-capture or biomechanical walking simulation.
use anyhow::{Context,Result,ensure};
use glam::{Mat4,Quat,Vec3};
use crate::gltf_load::{CpuMesh,Vertex};

const SOURCE:&str="assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/authoring/german_panzertruppe_rigged.glb";
pub(super) const STRIDE_M:f32=0.8;
const STANCE:f32=0.60;

struct Bone {name:String,parent:Option<usize>,base:Mat4}
pub(super) struct Rig {
    pub mesh:CpuMesh,
    bones:Vec<Bone>,
    weights:Vec<Vec<(usize,f32)>>,
    arms:[[usize;3];2],
    legs:[[usize;3];2],
    head:usize,
}

fn collect_nodes(node:gltf::Node<'_>,parent:Mat4,parents:&mut [Option<usize>],world:&mut [Mat4]) {
    let transform=parent*Mat4::from_cols_array_2d(&node.transform().matrix());
    world[node.index()]=transform;
    for child in node.children() {
        parents[child.index()]=Some(node.index());
        collect_nodes(child,transform,parents,world);
    }
}

/// Same node/primitive/vertex order as gltf_load::walk, verified against every loaded position.
fn collect_weights(node:gltf::Node<'_>,world:&[Mat4],buffers:&[gltf::buffer::Data],
    positions:&mut Vec<Vec3>,mesh_frames:&mut Vec<Mat4>,weights:&mut Vec<Vec<(usize,f32)>>)->Result<()> {
    if let Some(mesh)=node.mesh() {
        ensure!(node.skin().is_some_and(|s|s.index()==0),"camp source mesh has an unexpected skin");
        for primitive in mesh.primitives() {
            let reader=primitive.reader(|b|Some(&buffers[b.index()]));
            let p=reader.read_positions().context("source positions absent")?.collect::<Vec<_>>();
            let j0=reader.read_joints(0).context("source joints absent")?.into_u16().collect::<Vec<_>>();
            let w0=reader.read_weights(0).context("source weights absent")?.into_f32().collect::<Vec<_>>();
            let j1=reader.read_joints(1).map(|j|j.into_u16().collect::<Vec<_>>());
            let w1=reader.read_weights(1).map(|w|w.into_f32().collect::<Vec<_>>());
            ensure!(p.len()==j0.len()&&p.len()==w0.len(),"source skin accessor counts differ");
            ensure!(j1.is_some()==w1.is_some(),"incomplete secondary source skin weights");
            if let (Some(j),Some(w))=(&j1,&w1) {ensure!(j.len()==p.len()&&w.len()==p.len(),"secondary weight count differs");}
            for i in 0..p.len() {
                let mut row=Vec::with_capacity(8);
                for k in 0..4 {if w0[i][k]>0.0 {row.push((j0[i][k]as usize,w0[i][k]));}}
                if let (Some(j),Some(w))=(&j1,&w1) {
                    for k in 0..4 {if w[i][k]>0.0 {row.push((j[i][k]as usize,w[i][k]));}}
                }
                let total=row.iter().map(|(_,w)|*w).sum::<f32>();
                ensure!(!row.is_empty()&&total.is_finite()&&(total-1.0).abs()<1e-3,"invalid source skin weights");
                for (_,w) in &mut row {*w/=total;}
                weights.push(row);
                positions.push(world[node.index()].transform_point3(Vec3::from_array(p[i])));
                mesh_frames.push(world[node.index()]);
            }
        }
    }
    for child in node.children() {collect_weights(child,world,buffers,positions,mesh_frames,weights)?;}
    Ok(())
}

impl Rig {
    pub fn load()->Result<Self> {
        let mut mesh=crate::gltf_load::load_glb(SOURCE)?;
        let (doc,buffers,_)=gltf::import(SOURCE)?;
        ensure!(doc.skins().len()==1,"camp source must retain its one original skin");
        let scene=doc.default_scene().or_else(||doc.scenes().next()).context("camp source scene absent")?;
        let mut world=vec![Mat4::IDENTITY;doc.nodes().len()];
        let mut parents=vec![None;world.len()];
        for node in scene.nodes() {collect_nodes(node,Mat4::IDENTITY,&mut parents,&mut world);}
        let skin=doc.skins().next().unwrap();
        let joints=skin.joints().collect::<Vec<_>>();
        ensure!(joints.len()==39,"camp source joint count changed");
        let inverse=skin.reader(|b|Some(&buffers[b.index()])).read_inverse_bind_matrices()
            .context("source inverse binds absent")?.map(|m|Mat4::from_cols_array_2d(&m)).collect::<Vec<_>>();
        ensure!(inverse.len()==joints.len(),"source inverse-bind count changed");
        let bones=joints.iter().zip(&inverse).map(|(node,inverse)| {
            let base=world[node.index()];
            // The master exports inverse binds cancelling the source joint world frame.
            // Mesh-node transforms are handled separately below, as glTF skinning requires.
            ensure!(base.is_finite()&&inverse.is_finite()&&
                (base**inverse).to_cols_array().iter().zip(Mat4::IDENTITY.to_cols_array())
                    .all(|(a,b)|(*a-b).abs()<1e-4),"source joint bind frame changed");
            let mut ancestor=parents[node.index()];
            let parent=loop {
                let Some(index)=ancestor else {break None;};
                if let Some(joint)=joints.iter().position(|j|j.index()==index) {break Some(joint);}
                ancestor=parents[index];
            };
            Ok(Bone{name:node.name().context("source joint unnamed")?.into(),parent,base})
        }).collect::<Result<Vec<_>>>()?;
        let mut positions=Vec::new();let mut mesh_frames=Vec::new();let mut weights=Vec::new();
        for node in scene.nodes() {collect_weights(node,&world,&buffers,&mut positions,&mut mesh_frames,&mut weights)?;}
        ensure!(positions.len()==914&&mesh.vertices.len()==positions.len()&&mesh.indices.len()==564*3,
            "reviewed camp source topology changed");
        ensure!(positions.iter().zip(&mesh.vertices).all(|(p,v)|p.distance(Vec3::from_array(v.pos))<1e-5),
            "source skin order differs from rendered vertex order");
        // A skinned glTF mesh uses inverse(meshWorld)*jointWorld*inverseBind, followed by
        // meshWorld at draw. This source's jointWorld*inverseBind is identity at rest, so
        // raw POSITION is already the bind/world geometry. The static loader intentionally
        // ignores skinning and applies meshWorld; undo that exact transform here, rather than
        // shrinking the geometry a second time while keeping full-sized source bone pivots.
        mesh.min=Vec3::splat(f32::INFINITY);mesh.max=Vec3::splat(f32::NEG_INFINITY);
        for (vertex,frame) in mesh.vertices.iter_mut().zip(mesh_frames) {
            let inverse=frame.inverse();let p=inverse.transform_point3(Vec3::from_array(vertex.pos));
            vertex.pos=p.to_array();
            vertex.normal=frame.transpose().transform_vector3(Vec3::from_array(vertex.normal)).normalize_or_zero().to_array();
            let tangent=inverse.transform_vector3(Vec3::new(vertex.tangent[0],vertex.tangent[1],vertex.tangent[2])).normalize_or_zero();
            vertex.tangent=[tangent.x,tangent.y,tangent.z,vertex.tangent[3]];
            mesh.min=mesh.min.min(p);mesh.max=mesh.max.max(p);
        }
        ensure!(mesh.max.y-mesh.min.y>1.0&&mesh.max.y-mesh.min.y<2.1,"source bind body height is not human scale");
        ensure!(weights.iter().flatten().all(|(j,w)|*j<bones.len()&&w.is_finite()&&*w>=0.0),"source joint index out of range");
        let joint=|name:&str|bones.iter().position(|b|b.name==name).with_context(||format!("source joint {name} absent"));
        let arms=[[joint("L_Arm_N1")?,joint("L_Elbow_N1")?,joint("L_Hand_N1")?],
            [joint("R_Arm_N1")?,joint("R_Elbow_N1")?,joint("R_Hand_N1")?]];
        let legs=[[joint("L_Leg_N1")?,joint("L_Knee_N1")?,joint("L_Ankle_N1")?],
            [joint("R_Leg_N1")?,joint("R_Knee_N1")?,joint("R_Ankle_N1")?]];
        let head=joint("Head_N1")?;
        Ok(Self{mesh,bones,weights,arms,legs,head})
    }
    pub fn neutral(&self)->Vec<Vertex> {self.mesh.vertices.clone()}
    fn descendant(&self,mut joint:usize,root:usize)->bool {
        for _ in 0..self.bones.len() {
            if joint==root {return true;}
            let Some(parent)=self.bones[joint].parent else{return false;};joint=parent;
        }
        false
    }
    fn pivot(&self,joint:usize)->Vec3 {self.bones[joint].base.w_axis.truncate()}
    fn apply_to_branch(&self,matrices:&mut [Mat4],root:usize,transform:Mat4) {
        for (joint,matrix) in matrices.iter_mut().enumerate() {
            if self.descendant(joint,root) {*matrix=transform**matrix;}
        }
    }
    fn solve_chain(&self,matrices:&mut [Mat4],chain:[usize;3],target:Vec3,pole:Vec3) {
        let [a,b,c]=chain.map(|i|matrices[i].w_axis.truncate());
        let solved=super::crew_arm_animation::arm_chain(a,b,c,target,pole);
        let upper=around(a,Quat::from_rotation_arc((b-a).normalize_or_zero(),(solved[1]-a).normalize_or_zero()));
        self.apply_to_branch(matrices,chain[0],upper);
        let moved=matrices[chain[2]].w_axis.truncate();
        let lower=around(solved[1],Quat::from_rotation_arc((moved-solved[1]).normalize_or_zero(),(solved[2]-solved[1]).normalize_or_zero()));
        self.apply_to_branch(matrices,chain[1],lower);
    }
    /// phase advances by actual travelled distance/STRIDE_M. World +Z is the authored front.
    /// Ordinary idle lowers the source T-pose arms; neutral() is reserved for the bind proof.
    pub fn pose(&self,gait_phase:f32,moving:bool,salute:f32,working:f32)->Vec<Vertex> {self.skin(&self.pose_matrices(gait_phase,moving,salute,working))}
    fn pose_matrices(&self,gait_phase:f32,moving:bool,salute:f32,working:f32)->Vec<Mat4> {
        let phase=if gait_phase.is_finite(){gait_phase.rem_euclid(1.0)}else{0.0};
        let salute=if salute.is_finite(){salute.clamp(0.0,1.0)}else{0.0};
        let working=if working.is_finite(){working.clamp(0.0,1.0)}else{0.0};
        let mut matrices=self.bones.iter().map(|b|b.base).collect::<Vec<_>>();
        if moving {
            let [hip,knee,ankle]=self.legs[0].map(|i|self.pivot(i));
            let length=hip.distance(knee)+knee.distance(ankle);
            let travel=STRIDE_M*STANCE*0.5;
            // Shorten vertical hip-to-ground reach enough to place a real-length leg at the
            // stance extremes. No bone or mesh is lengthened to reach a requested footfall.
            let drop=(length-(length*length-travel*travel).max(0.0).sqrt()+0.006).min(length*0.15);
            for matrix in &mut matrices {*matrix=Mat4::from_translation(Vec3::NEG_Y*drop)**matrix;}
            for side in 0..2 {
                let p=(phase+side as f32*0.5).fract();
                let (z,lift)=foot_cycle(p);
                let chain=self.legs[side];let target=self.pivot(chain[2])+Vec3::new(0.0,lift,z);
                self.solve_chain(&mut matrices,chain,target,Vec3::Z);
                // Keep the source boot level while its ankle moves: no planted toe rotates
                // through the ground just because the knee flexed.
                let foot=chain[2];let at=matrices[foot].w_axis.truncate();
                let original=self.bones[foot].base.to_scale_rotation_translation().1;
                let current=matrices[foot].to_scale_rotation_translation().1;
                self.apply_to_branch(&mut matrices,foot,around(at,original*current.inverse()));
            }
        }
        for side in 0..2 {
            let chain=self.arms[side];let [shoulder,elbow,hand]=chain.map(|i|matrices[i].w_axis.truncate());
            let length=shoulder.distance(elbow)+elbow.distance(hand);let sign=if side==0 {1.0}else{-1.0};
            let swing=if moving {(phase*std::f32::consts::TAU+side as f32*std::f32::consts::PI).sin()*0.14}else{0.0};
            let mut target=shoulder+Vec3::new(sign*length*0.12,-length*0.92,0.04+swing);
            let work=shoulder+Vec3::new(-sign*length*0.15,-length*0.45,length*0.65);
            target=target.lerp(work,working);
            if side==1 {
                let head=matrices[self.head].w_axis.truncate();
                let salute_target=head+Vec3::new(-length*0.17,length*0.15,length*0.07);
                target=target.lerp(salute_target,salute);
            }
            self.solve_chain(&mut matrices,chain,target,Vec3::new(sign,-0.10,-0.35));
        }
        matrices
    }
    fn skin(&self,matrices:&[Mat4])->Vec<Vertex> {
        let deltas=matrices.iter().zip(&self.bones).map(|(m,b)|*m*b.base.inverse()).collect::<Vec<_>>();
        self.mesh.vertices.iter().zip(&self.weights).map(|(source,weights)| {
            let mut result=*source;let mut p=Vec3::ZERO;let mut n=Vec3::ZERO;let mut t=Vec3::ZERO;
            for &(joint,weight) in weights {
                p+=deltas[joint].transform_point3(Vec3::from_array(source.pos))*weight;
                n+=deltas[joint].transform_vector3(Vec3::from_array(source.normal))*weight;
                t+=deltas[joint].transform_vector3(Vec3::new(source.tangent[0],source.tangent[1],source.tangent[2]))*weight;
            }
            result.pos=p.to_array();result.normal=n.normalize_or_zero().to_array();let t=t.normalize_or_zero();
            result.tangent=[t.x,t.y,t.z,source.tangent[3]];result
        }).collect()
    }
}
fn around(p:Vec3,q:Quat)->Mat4 {Mat4::from_translation(p)*Mat4::from_quat(q)*Mat4::from_translation(-p)}
fn foot_cycle(phase:f32)->(f32,f32) {
    let end=STRIDE_M*STANCE*0.5;
    if phase<STANCE {(end-phase*STRIDE_M,0.0)} else {
        let p=(phase-STANCE)/(1.0-STANCE);let smooth=p*p*(3.0-2.0*p);
        (-end+2.0*end*smooth,(std::f32::consts::PI*p).sin()*0.09)
    }
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn stance_world_foot_does_not_slide_at_matched_root_speed() {
        for phase in [0.0,0.1,0.2,0.3,0.4,0.5] {
            let (z,y)=foot_cycle(phase);
            assert!((phase*STRIDE_M+z-STRIDE_M*STANCE*0.5).abs()<1e-6);assert_eq!(y,0.0);
        }
        assert!(foot_cycle(0.8).1>0.085);
        assert!((foot_cycle(0.999999).0-foot_cycle(0.0).0).abs()<1e-5);
    }
    #[test]fn source_bind_order_skin_and_idle_keep_material_and_ground_contract() {
        if !std::path::Path::new(SOURCE).exists() {return;}
        let rig=Rig::load().unwrap();
        let neutral=rig.skin(&rig.bones.iter().map(|b|b.base).collect::<Vec<_>>());
        for (original,posed) in rig.mesh.vertices.iter().zip(neutral) {
            assert!(Vec3::from_array(original.pos).distance(Vec3::from_array(posed.pos))<1e-5);
            assert_eq!(original.uv,posed.uv);assert_eq!(original.tangent[3],posed.tangent[3]);
        }
        for phase in [0.0,0.1,0.25,0.5,0.8,0.95] {
            let pose=rig.pose(phase,true,0.0,0.0);
            assert!(pose.iter().all(|v|Vec3::from_array(v.pos).is_finite()));
            let min=pose.iter().map(|v|v.pos[1]).fold(f32::INFINITY,f32::min);
            assert!(min>=rig.mesh.min.y-0.012,"foot below source floor by {}",rig.mesh.min.y-min);
            for (original,posed) in rig.mesh.vertices.iter().zip(pose) {assert_eq!(original.uv,posed.uv);}
        }
    }
}

impl Rig {
    pub(in crate::game) fn field_parts(&self)->Vec<usize> {
        self.weights.iter().map(|weights|{let mut vote=[0.;11];for &(joint,weight) in weights {
            vote[super::singleplayer_crew::field_people::source_part(&self.bones[joint].name)]+=weight;}
            (0..11).max_by(|&a,&b|vote[a].total_cmp(&vote[b])).unwrap()}).collect()
    }
    pub(in crate::game) fn field_segments(&self,model:Mat4,phase:f32,moving:bool,salute:f32,work:f32)->[super::crew_physics::CrewSegment;11] {
        let matrices=self.pose_matrices(phase,moving,salute,work);
        let bases=self.bones.iter().map(|b|b.base).collect::<Vec<_>>();let names=self.bones.iter().map(|b|b.name.as_str()).collect::<Vec<_>>();
        super::singleplayer_crew::field_people::source_segments(model,&names,&bases,&matrices,self.arms,self.legs)
    }
}
