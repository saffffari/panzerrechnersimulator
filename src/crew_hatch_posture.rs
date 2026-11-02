//! Source-joint hatch posture; original weighted vertices and bone lengths.
//! Original weighted vertices, original link lengths, grounded ankles and raised forearms.
use super::*;
impl Rig {
    pub(super) fn hatch_posture(&self,base:&[Mat4],amount:f32,standing:f32)->Pose {
        let amount=amount.clamp(0.,1.);let mut matrices=base.to_vec();
        if amount<=0.0{return self.skin_pose(matrices,[None,None],standing>0.5);}
        let shift=Vec3::Y*(-0.18*amount*standing);
        for m in &mut matrices{*m=Mat4::from_translation(shift)**m;}
        for chain in self.legs {
            let foot=chain[2];let ankle=base[foot].w_axis.truncate();
            self.solve(&mut matrices,chain,ankle,Vec3::Z);
            let at=matrices[foot].w_axis.truncate();let original=base[foot].to_scale_rotation_translation().1;
            let current=matrices[foot].to_scale_rotation_translation().1;
            self.branch(&mut matrices,foot,around(at,original*current.inverse()));
        }
        let pivot=matrices[self.spine].w_axis.truncate();
        self.branch(&mut matrices,self.spine,around(pivot,Quat::from_rotation_z(0.40*amount)));
        for (side,chain) in self.arms.into_iter().enumerate(){
            let [a,b,c]=chain.map(|i|matrices[i].w_axis.truncate());
            let desired=Vec3::new(if side==0{0.10}else{-0.10},1.8-0.18*amount*standing,0.05);
            let reach=a.distance(b)+b.distance(c)-0.012;
            let raised=a+(desired-a).normalize_or_zero()*reach;
            // Blend original hand location toward a reachable overhead location. The pole lies
            // forward of the chest, avoiding the wide lateral elbows of the idle pose.
            self.solve(&mut matrices,chain,c.lerp(raised,amount),Vec3::Z);
        }
        self.skin_pose(matrices,[None,None],standing>0.5)
    }
}
#[cfg(test)]mod tests {use super::*;
 #[test]#[ignore="requires private standing rig"]
 fn hatch_posture_keeps_original_limb_lengths_and_ankle_support(){let rig=Rig::load_standing().unwrap();
  for n in 0..=10 {let pose=rig.hatch_posture(&rig.bones.iter().map(|b|b.base).collect::<Vec<_>>(),n as f32/10.,1.);for (chains,positions) in [(rig.arms,pose.arms),(rig.legs,pose.legs)]{
   for (chain,p) in chains.into_iter().zip(positions){let original=chain.map(|i|rig.bones[i].base.w_axis.truncate());for k in 0..2{assert!((p[k].distance(p[k+1])-original[k].distance(original[k+1])).abs()<1e-4);}}}
   for (chain,p) in rig.legs.into_iter().zip(pose.legs){assert!(p[2].distance(rig.bones[chain[2]].base.w_axis.truncate())<1e-4);}
   assert_eq!(pose.vertices.len(),rig.vertices.len());for (a,b) in pose.vertices.iter().zip(&rig.vertices){assert_eq!(a.uv,b.uv);}
  }
 }
}
