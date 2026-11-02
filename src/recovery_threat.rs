//! Enemy perception follows the living controlled person and their actual vehicle. A disabled
//! Tiger is not an invisibility switch for a keeper evacuating it or driving the carrier.
use super::*;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum Kind {Tiger,Foot,Carrier}
#[derive(Clone,Copy,Debug)]
pub(super) struct Threat {pub kind:Kind,pub position:Vec3,pub velocity:Vec3,pub speed_fraction:f32}
fn select(alive:bool,on_foot:bool,tiger:(Vec3,Vec3,f32),foot:(Vec3,f32),carrier:Option<(Vec3,Vec3,f32)>)->Option<Threat>{
    if !alive{return None;}
    let (kind,(position,velocity,speed_fraction))=if let Some(carrier)=carrier {(Kind::Carrier,carrier)}
        else if on_foot {(Kind::Foot,(foot.0,Vec3::ZERO,foot.1))}else{(Kind::Tiger,tiger)};
    (position.is_finite()&&velocity.is_finite()&&speed_fraction.is_finite())
        .then_some(Threat{kind,position,velocity,speed_fraction:speed_fraction.clamp(0.0,1.0)})
}
impl Game {
    pub(super) fn controlled_threat(&self)->Option<Threat>{
        select(self.controlled_soldier_alive(),self.winter_foot_target(),
            (self.player.center(),self.player.forward()*self.player.speed,self.player.speed.abs()/tank::TOP_SPEED),
            (self.foot.pos+Vec3::Y*(0.6+0.55*(1.0-self.foot.crouch_t)),self.foot.speed_frac),self.halftrack_threat())
    }
    pub(super) fn enemy_has_live_threat(&self)->bool{
        self.controlled_threat().is_some()||(self.platoon.is_some()&&self.allies.iter().any(|a|a.tank.alive))
    }
    pub fn recovery_threat_receipt(&self)->serde_json::Value {
        serde_json::json!({"keeper_alive":self.controlled_soldier_alive(),"tiger_operational":self.player.alive,
            "controlled_threat":self.controlled_threat().map(|t|serde_json::json!({"kind":format!("{:?}",t.kind),"position":t.position.to_array(),"velocity":t.velocity.to_array(),"speed_fraction":t.speed_fraction})),
            "enemies":self.enemies.iter().enumerate().filter(|(i,e)|e.alive&&self.enemy_active.get(*i).copied().unwrap_or(false)).map(|(i,e)|serde_json::json!({"id":i,"aim_target":self.enemy_threat_center(i).to_array(),"spotted":self.stealth.spotted[i],"meter":self.stealth.meters[i],"optical_los":self.optical_los_clear(e.center(),self.enemy_threat_center(i)),"shots":self.enemy_gun_shots[i]})).collect::<Vec<_>>(),
            "limits":"Carrier recognition uses the existing authored vehicle envelope, not a measured signature. Foot lead is not inferred from look direction. Existing LOS/weather/first-shot delays still apply."})
    }
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn dismounted_survivor_target_moves_independently_of_disabled_tiger(){
        let tank=(Vec3::new(900.0,2.0,400.0),Vec3::ZERO,0.0);let feet=Vec3::new(120.0,1.1,-30.0);
        let target=select(true,true,tank,(feet,0.4),None).unwrap();assert_eq!(target.kind,Kind::Foot);assert_eq!(target.position,feet);
        // Vehicle operability is intentionally not an input: health belongs to the real person.
        assert!(select(false,true,tank,(feet,0.4),None).is_none());
    }
    #[test]fn carrier_uses_actual_vehicle_pose_velocity_and_signature_instead_of_crouched_foot(){
        let carrier=(Vec3::new(-45.0,1.7,30.0),Vec3::Z*6.0,2.0/3.0);
        let target=select(true,true,(Vec3::ZERO,Vec3::ZERO,0.0),(Vec3::Y,0.0),Some(carrier)).unwrap();
        assert_eq!(target.kind,Kind::Carrier);assert_eq!(target.position,carrier.0);assert_eq!(target.velocity,carrier.1);assert_eq!(target.speed_fraction,carrier.2);
        assert!(select(false,true,(Vec3::ZERO,Vec3::ZERO,0.0),(Vec3::Y,0.0),Some(carrier)).is_none());
    }
    #[test]fn living_seated_keeper_is_targetable_even_if_vehicle_cannot_drive(){
        let tank=(Vec3::new(2.0,1.0,3.0),Vec3::ZERO,0.0);let target=select(true,false,tank,(Vec3::ZERO,0.0),None).unwrap();
        assert_eq!(target.kind,Kind::Tiger);assert_eq!(target.position,tank.0);
    }
}
