//! Continuation plumbing for one surviving source-armor deflection. No speculative target,
//! damage multiplication, lifetime reset or permanent source-vehicle exemption is introduced.
use super::*;
use crate::ballistics::ricochet;

impl AuthoritativeProjectile {
    pub(super) fn ignores_firing_vehicle(&self)->bool {
        self.small_arm.is_some() || !self.muzzle_cleared
    }
    pub(super) fn continue_ricochet(&mut self,point:Vec3,response:ricochet::Response) {
        self.pos=ricochet::separated_origin(point,response.outward);
        self.velocity=response.velocity;self.direction=response.velocity.normalize();
        self.retained_speed=response.velocity.length()/self.shell.velocity_at(self.distance_m).max(1e-6);
        self.flight_epoch_s=self.age_s;self.crosswind_velocity=Vec3::ZERO;
        self.ricochet_count+=1;
        // Contact elsewhere proves departure from the launcher's muzzle. A returning shell can
        // now hit the firing hull just like any other hull; no team or shooter immunity persists.
        self.muzzle_cleared=true;
        // These transit exemptions belong to an incoming segment, never a reflected trajectory.
        self.ignored_structure=None;self.ignored_tree=None;self.passed_at_gun_part=None;
    }
    pub(super) fn bounded_flight_dt(&self,available:f32,limit:f32)->f32 {
        let remaining=(limit-self.distance_m).max(0.0);
        let speed=self.shell.velocity_at(self.distance_m)*self.retained_speed;
        let k=self.shell.drag_k;
        let to_range=if speed<=1e-6 {0.0}else if k<1e-9 {remaining/speed}
            else {(k*remaining).exp_m1()/(k*speed)};
        available.min(to_range).min((ricochet::MAX_FLIGHT_SECONDS-self.age_s).max(0.0))
    }
}

impl Game {
    pub(super) fn update_projectile_muzzle_clearance(&self,p:&mut AuthoritativeProjectile) {
        if p.small_arm.is_some() || p.muzzle_cleared{return;}
        let margin=ricochet::SEPARATION_M;
        let fallback=|tank:&Tank,scheme|[ArmorPart::Hull,ArmorPart::Turret].into_iter().all(|part| {
            rig_armor_bounds(scheme,part).is_none_or(|(lo,hi)| {
                let model=if part==ArmorPart::Hull {tank.hull_model()}else{tank.turret_model()};
                let local=model.inverse().transform_point3(p.pos);
                local.cmplt(lo-Vec3::splat(margin)).any() || local.cmpgt(hi+Vec3::splat(margin)).any()
            })
        });
        p.muzzle_cleared=match p.shooter {
            ProjectileShooter::Player=>self.projectile_player_armor.outside_source_bounds(p.pos,&self.fire_surface_models(),margin),
            ProjectileShooter::Enemy(index)=>self.enemies.get(index).is_none_or(|tank| {
                if tank.kind==EnemyKind::T34 {
                    let mut models=[Mat4::IDENTITY;surface_fire::FRAMES];
                    models[0]=tank.hull_model();models[1]=tank.turret_model();
                    self.projectile_enemy_armor.outside_source_bounds(p.pos,&models,margin)
                }else{fallback(tank,enemy_armor(tank.kind))}
            }),
            ProjectileShooter::Ally(index)=>self.allies.get(index).is_none_or(|ally|
                fallback(&ally.tank,crate::ballistics::ArmorScheme::TigerI)),
            ProjectileShooter::AtGun(_)=>true,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn round()->AuthoritativeProjectile {
        AuthoritativeProjectile::new(917,ProjectileShooter::Player,&crate::ballistics::PZGR_39,
            Vec3::new(0.0,2.0,0.0),Vec3::Z,31)
    }
    fn reflected(p:&AuthoritativeProjectile)->ricochet::Response {
        let normal=Vec3::new(1.0,0.0,-0.1).normalize();
        let mut event=crate::ballistics::resolve(p.shell,p.distance_m,
            crate::ballistics::ArmorScheme::TigerI.plate(crate::ballistics::Facing::HullSide),85.0,3);
        event.terminal=crate::ballistics::Terminal::Bounce;
        ricochet::response(&event,p.velocity,normal,p.ricochet_count).unwrap()
    }
    #[test]
    fn ricochet_keeps_identity_age_range_and_restarts_only_force_epoch() {
        let mut p=round();p.commit_step(p.swept_step(0.4));
        let (id,seed,age,range,origin)=(p.id,p.seed,p.age_s,p.distance_m,p.origin);
        p.ignored_tree=Some(4);p.ignored_structure=Some(7);p.passed_at_gun_part=Some((2,1));
        let response=reflected(&p);p.continue_ricochet(p.pos,response);
        assert_eq!((p.id,p.seed,p.age_s,p.distance_m,p.origin),(id,seed,age,range,origin));
        assert_eq!(p.ricochet_count,1);assert!(p.muzzle_cleared);
        assert!(p.ignored_tree.is_none() && p.ignored_structure.is_none() && p.passed_at_gun_part.is_none());
        assert!(p.swept_step(0.0).velocity.distance(response.velocity)<0.0001,
            "launch-age gravity must not be applied twice at deflection");
        let start=p.pos;let after=p.swept_step(PROJECTILE_FIXED_DT);
        assert!((after.end-start).dot(response.outward)>0.0);
        assert!((after.age_s-age-PROJECTILE_FIXED_DT).abs()<1e-6);
    }
    #[test]
    fn continuation_consumes_same_tick_remainder_and_retains_range_lifetime_limits() {
        let mut p=round();let whole=p.swept_step(PROJECTILE_FIXED_DT);
        let fraction=0.35;let point=whole.start.lerp(whole.end,fraction);
        p.commit_fraction(whole,fraction,point);let response=reflected(&p);
        p.continue_ricochet(point,response);
        p.commit_step(p.swept_step(PROJECTILE_FIXED_DT*(1.0-fraction)));
        assert!((p.age_s-PROJECTILE_FIXED_DT).abs()<1e-7);
        let range=p.distance_m+0.35;
        let dt=p.bounded_flight_dt(PROJECTILE_FIXED_DT,range);
        p.commit_step(p.swept_step(dt));
        assert!((p.distance_m-range).abs()<1e-4);assert!(p.range_expired(range));
        p.age_s=ricochet::MAX_FLIGHT_SECONDS;assert!(p.range_expired(1500.0));
        assert_eq!(p.bounded_flight_dt(PROJECTILE_FIXED_DT,1500.0),0.0);
    }
    #[test]
    fn retained_speed_cannot_regain_clean_penetration_at_a_second_plate() {
        let shell=&crate::ballistics::PZGR_39;
        let plate=crate::ballistics::ArmorScheme::TigerI.plate(crate::ballistics::Facing::HullFront);
        let original=crate::ballistics::resolve_with_budget_loss_at_speed(shell,100.0,plate,0.0,1,0.0,0.0,750.0);
        let slow=crate::ballistics::resolve_with_budget_loss_at_speed(shell,100.0,plate,0.0,1,0.0,0.0,250.0);
        assert_eq!(slow.striking_v,250.0);assert!(slow.pen_mm<original.pen_mm);
        assert_ne!(slow.terminal,crate::ballistics::Terminal::Penetration);
    }
    #[test]
    fn post_deflection_flight_is_frame_partition_independent_and_source_immunity_ends() {
        let mut start=round();assert!(start.ignores_firing_vehicle());
        start.commit_step(start.swept_step(0.3));let response=reflected(&start);
        start.continue_ricochet(start.pos,response);assert!(!start.ignores_firing_vehicle());
        let mut results=Vec::new();
        for hz in [30,60,144] {
            let mut p=start.clone();let mut accumulator=0.0_f64;
            for _ in 0..hz {
                accumulator+=1.0/hz as f64;
                while accumulator+1e-8>=PROJECTILE_FIXED_DT as f64 {
                    accumulator-=PROJECTILE_FIXED_DT as f64;
                    p.commit_step(p.swept_step(PROJECTILE_FIXED_DT));
                }
            }
            results.push(p);
        }
        for p in &results[1..] {
            assert!(p.pos.distance(results[0].pos)<0.0001);
            assert!(p.velocity.distance(results[0].velocity)<0.0001);
            assert_eq!(p.age_s,results[0].age_s);assert_eq!(p.distance_m,results[0].distance_m);
        }
    }
}
