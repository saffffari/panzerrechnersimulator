//! Live small arms share the host's swept 240 Hz flight clock with tank shells. Damage and
//! particles are emitted at contact, never at trigger time. Offline policy training retains its
//! renderer-free hitscan approximation; this adapter owns the live game's flight delay.
use super::*;
use crate::infantry_combat::{Shot, Team, Weapon};

const PLAYER: u32 = u32::MAX;
const TANK_BASE: u32 = 10_000;

// Retain the existing game's 350 / 760 m/s presentation speeds. Drag is provisional gameplay
// tuning, not a historical ammunition certification. Zero penetration prevents these profiles
// from ever being mistaken for calibrated AP ammunition; terminal small-arms routing also exits
// before the cannon/DeMarre path, including for trees and structures.
const PISTOL: crate::ballistics::ShellSpec = crate::ballistics::ShellSpec {
    name: "gameplay pistol flight", kind: crate::ballistics::ShellKind::Ap,
    caliber_mm: 9.0, mass_kg: 0.008, muzzle_velocity: 350.0, filler_g: 0.0,
    drag_k: 0.0018, pen_ref_mm: 0.0, ref_range_m: 0.0,
};
const RIFLE: crate::ballistics::ShellSpec = crate::ballistics::ShellSpec {
    name: "gameplay rifle/MG flight", caliber_mm: 7.92, mass_kg: 0.012,
    muzzle_velocity: 760.0, drag_k: 0.0012, ..PISTOL
};

#[derive(Clone, Copy, Debug)]
pub(super) struct Payload {
    pub shot: Shot,
    pub damage: f32,
    pub mixed: bool,
    pub telemetry: Option<&'static str>,
}
impl Payload {
    pub fn visual_weapon(self) -> weapon_vfx::Weapon {
        match self.shot.weapon {
            Weapon::P08 => weapon_vfx::Weapon::Pistol,
            Weapon::Rifle => weapon_vfx::Weapon::Rifle,
            Weapon::MachineGun => weapon_vfx::Weapon::MachineGun,
        }
    }
    pub fn ignores_infantry(self, index: usize) -> bool { self.shot.shooter == index as u32 }
    pub fn is_player(self) -> bool { self.shot.shooter == PLAYER }
}

impl Game {
    pub(super) fn launch_small_arm(&mut self, shot: Shot, damage: f32, mixed: bool,
        telemetry: Option<&'static str>) {
        if !shot.origin.is_finite() || !shot.direction.is_finite()
            || shot.direction.length_squared() < 0.5 || !shot.range.is_finite() || shot.range <= 0.0 {
            return;
        }
        let payload = Payload { shot, damage, mixed, telemetry };
        let shell = if shot.weapon == Weapon::P08 { &PISTOL } else { &RIFLE };
        let shooter = if payload.is_player() { ProjectileShooter::Player }
            else if (TANK_BASE..TANK_BASE+N_ENEMIES as u32).contains(&shot.shooter) {
                ProjectileShooter::Enemy((shot.shooter-TANK_BASE) as usize)
            } else { ProjectileShooter::Enemy(usize::MAX) };
        let id = self.next_projectile_id;
        self.next_projectile_id = id.wrapping_add(1).max(1);
        let mut flight = AuthoritativeProjectile::new(id, shooter, shell, shot.origin,
            shot.direction, id as u32);
        flight.small_arm = Some(payload);
        self.projectiles.push(flight);
        self.weapon_vfx.muzzle(shot.origin, shot.direction, payload.visual_weapon());
        if payload.is_player() { self.winter_report_discharge(shot.origin,
            if shot.weapon == Weapon::P08 { 240.0 } else { 520.0 }); }
    }

    pub(super) fn finish_small_arm(&mut self, projectile: &AuthoritativeProjectile,
        contact: ProjectileContact, payload: Payload, camera: &crate::camera::Camera) {
        let hit_player = matches!(contact.kind, ProjectileContactKind::Foot);
        let hp_before = self.foot.hp;
        let weapon = payload.visual_weapon();
        let mut damage = 0.0;
        let mut friendly = false;
        match contact.kind {
            ProjectileContactKind::Vehicle(armor) => {
                self.weapon_vfx.armor_sparks(contact.pos, projectile.velocity, armor.world_normal, weapon);
                let on_own_armor = armor.vehicle == ProjectileVehicle::Player;
                let structure_borne = on_own_armor && self.mode == Mode::Tank;
                if on_own_armor || projectile.id % MG_SOUND_EVERY as u64 == 0 {
                    use crate::acoustics::{HullPanel, ImpactParams, ShellClass};
                    use crate::ballistics::Facing;
                    let (bearing, distance) = impact_listener_geometry(camera, contact.pos);
                    let panel = if armor.world_normal.y.abs() > 0.7 { HullPanel::Roof } else {
                        match armor.facing {
                            Facing::HullFront => HullPanel::GlacisFront,
                            Facing::HullSide => HullPanel::HullSide,
                            Facing::HullRear => HullPanel::HullRear,
                            Facing::TurretFront => HullPanel::TurretFront,
                            Facing::TurretSide => HullPanel::TurretSide,
                            Facing::TurretRear => HullPanel::TurretRear,
                        }
                    };
                    // Audio-only panel damping reference; roof/belly have no shell-armor claim.
                    let thickness = if on_own_armor && panel != HullPanel::Roof {
                        crate::ballistics::ArmorScheme::TigerI.plate(armor.facing).thickness_mm
                    } else { 0.0 };
                    let cosine = (-projectile.velocity.normalize_or_zero())
                        .dot(armor.world_normal).abs().clamp(0.02, 1.0);
                    let sound = crate::acoustics::synth_mg_armor_hit(ImpactParams {
                        shell: ShellClass::Mg, caliber_mm: projectile.shell.caliber_mm,
                        mass_kg: projectile.shell.mass_kg, velocity: projectile.velocity.length(),
                        plate_thickness_mm: thickness, effective_mm: thickness / cosine,
                        impact_angle_deg: cosine.acos().to_degrees(), residual_v: 0.0,
                        behind_armor: 0.0, panel, penetrated: false,
                        bearing_deg: bearing, station_dist_m: if structure_borne { distance } else { 1.0 },
                        seed: projectile.seed,
                    });
                    if structure_borne {
                        // Structure-borne arrival is effectively immediate inside the struck hull.
                        // SL §5: 120-140 dB per hit beside the plate, from the bullet's energy,
                        // its incidence and the listener's distance from the struck point.
                        let energy = 0.5 * projectile.shell.mass_kg
                            * projectile.velocity.length_squared();
                        let lpk = crate::sound_levels::armour_strike_lpk_db(energy,
                            cosine.acos().to_degrees(), distance);
                        let ear = self.own_vehicle_ear_level(lpk,
                            lpk - crate::sound_levels::LPK_MINUS_LAE_REVERBERANT_DB
                                + crate::sound_levels::LAF_MINUS_LAE_DB,
                            crate::sound_levels::SpectrumClass::ArmourRing, 0.5);
                        self.hear_own_vehicle_impulse(&ear, bearing);
                        if let Some(audio) = &self.audio {
                            audio.play_stereo_at_ear(sound, ear.laf_db, crate::audio::EarBus::World);
                        }
                    } else {
                        self.schedule_emission(crate::audio::Sfx::ImpactTank, contact.pos,
                            crate::sound_propagation::Emission::from_level(
                                &crate::sound_levels::BULLET_ON_STEEL_1M),
                            None, Some(sound));
                    }
                }
                // All small-arms profiles are inert against tank armor: no AP resolution, HP chip,
                // spall, or cabin fire. Roof/belly blocking below needs no invented plate values.
            }
            ProjectileContactKind::Foot => {
                if !self.god {
                    damage = payload.damage.min(self.foot.hp.max(0.0));
                    self.foot.hp = (self.foot.hp-damage).max(0.0);
                }
                friendly = payload.shot.team == Team::German;
                self.weapon_vfx.impact(contact.pos, projectile.velocity.normalize_or_zero(), 0.10, false, weapon);
            }
            ProjectileContactKind::Infantry(index) => {
                if let Some(actor) = self.mixed_combat.actors.get_mut(index) {
                    damage = payload.damage.min(actor.hp.max(0.0));
                    friendly = actor.team == payload.shot.team;
                    actor.hp = (actor.hp-damage).max(0.0);
                    actor.suppression = 1.0;
                    if actor.hp <= 0.0 { self.kill_soldier(index, "ballistic small arms"); }
                } else { self.kill_soldier(index, "ballistic small arms"); }
                self.weapon_vfx.impact(contact.pos, projectile.velocity.normalize_or_zero(), 0.10, false, weapon);
            }
            ProjectileContactKind::RangeLimit => {} // Retirement never invents an impact in air.
            _ => {
                self.weapon_vfx.impact(contact.pos, projectile.velocity.normalize_or_zero(), 0.20, false, weapon);
                if projectile.id % MG_SOUND_EVERY as u64 == 0 {
                    let mono = crate::acoustics::synth_small_arm_ground_hit(
                        projectile.velocity.length(), projectile.shell.caliber_mm, projectile.seed);
                    // ScheduledSound accepts stereo buffers and resolves the actual arrival pan.
                    // Equal-power centering makes its later mono reduction recover this dry voice.
                    let stereo = crate::acoustics::spatialize_mono(&mono, 0.0, 0.0);
                    self.schedule_emission(crate::audio::Sfx::ImpactGround, contact.pos,
                        crate::sound_propagation::Emission::from_level(
                            &crate::sound_levels::BULLET_INTO_GROUND_1M),
                        None, Some(stereo));
                }
            }
        }
        if payload.mixed { self.record_small_arm_damage(payload.shot, damage, friendly); }
        if let Some(weapon) = payload.telemetry.filter(|_| self.campaign_telemetry_enabled) {
            let target = self.foot.eye();
            self.campaign_pedestrian_shots.push(crate::campaign_ai::PedestrianShotEvent {
                time_s: 0.0, weapon,
                shooter: Vec2::new(payload.shot.origin.x, payload.shot.origin.z),
                target: Vec2::new(target.x, target.z), distance_m: projectile.distance_m,
                target_crouched: self.foot.crouch_t > 0.5, target_visible: true,
                hit: hit_player, killed: hp_before > 0.0 && self.foot.hp <= 0.0,
            });
        }
    }
}

/// OBB blocking includes roof and belly for non-penetrating bullets. Cannon plate selection stays
/// with its existing supported front/side/rear scheme; this does not invent roof armor data.
pub(super) fn vehicle_contact(tank: &Tank, vehicle: ProjectileVehicle,
    scheme: crate::ballistics::ArmorScheme, start: Vec3, end: Vec3) -> Option<ProjectileContact> {
    let mut nearest = None;
    for part in [ArmorPart::Hull, ArmorPart::Turret] {
        let Some((min,max)) = rig_armor_bounds(scheme,part) else { continue; };
        let model = if part == ArmorPart::Hull { tank.hull_model() } else { tank.turret_model() };
        let inverse = model.inverse();
        let a = inverse.transform_point3(start);
        let b = inverse.transform_point3(end);
        let Some((fraction, normal)) = segment_local_aabb_contact(a,b,min,max) else { continue; };
        let contact = ProjectileContact { t_m:start.distance(end)*fraction, pos:start.lerp(end,fraction),
            kind:ProjectileContactKind::Vehicle(VehicleArmorContact {
                vehicle, part, facing:facing_from_local_normal(normal,part),
                local_point:a.lerp(b,fraction), world_normal:model.transform_vector3(normal).normalize_or_zero(),
            }) };
        consider_projectile_contact(&mut nearest,Some(contact),start.distance(end));
    }
    nearest
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a GPU and the source Tiger assets"]
    fn small_arms_live_damage_waits_for_contact_and_respects_cover() -> anyhow::Result<()> {
        use crate::tank_editor_scene::{TankEditorScene,TankAssetId};
        let instance=crate::create_instance();
        let adapter=crate::request_adapter(&instance,None);
        let (device,queue)=crate::request_device(&adapter);
        let (mut r,mut g)=Game::build_tank_editor(&device,&queue,&TankEditorScene::new(TankAssetId::TigerI))?;
        for enemy in &mut g.enemies { enemy.alive=false; }
        for soldier in &mut g.soldiers { soldier.alive=false; }
        g.story_cover.clear();
        let floor=g.hf.height_at(20.0,40.0);
        let body=Vec3::new(20.0,floor,40.0);
        let origin=Vec3::new(20.0,floor+1.0,0.0);
        g.soldiers.push(Soldier::new(body,0.0,[body;4],0,usize::MAX,usize::MAX));
        g.mixed_combat.actors.push(crate::infantry_combat::Combatant::new(0,Team::Soviet,body,0.0));
        let shot=Shot { shooter:PLAYER,team:Team::German,origin,direction:Vec3::Z,
            range:100.0,weapon:Weapon::MachineGun };
        let reset=|g:&mut Game| {
            g.projectiles.clear();g.projectile_accumulator_s=0.0;
            g.soldiers[0].alive=true;g.soldiers[0].pos=body;
            g.mixed_combat.actors[0].hp=100.0;
            g.mixed_combat.actors[0].pos=body;
        };
        reset(&mut g);
        g.launch_small_arm(shot,100.0,false,None);
        assert!(g.soldiers[0].alive,"trigger time must not deal damage");
        g.advance_projectiles(0.02,&mut r);
        assert!(g.soldiers[0].alive,"bullet is still short of the target");
        g.advance_projectiles(0.06,&mut r);
        assert!(!g.soldiers[0].alive,"swept contact must apply terminal damage");

        reset(&mut g);
        g.story_cover.push((Vec3::new(19.0,floor,19.995),Vec3::new(21.0,floor+2.0,20.005)));
        g.launch_small_arm(shot,100.0,false,None);
        g.advance_projectiles(0.10,&mut r);
        assert!(g.soldiers[0].alive,"a thin wall must block the swept bullet");
        assert!(g.projectiles.is_empty());
        g.story_cover.clear();

        reset(&mut g);
        g.launch_small_arm(shot,100.0,false,None);
        g.soldiers[0].pos.x+=5.0;
        g.advance_projectiles(0.20,&mut r);
        assert!(g.soldiers[0].alive,"a moving target is tested at flight time");
        assert!(g.projectiles.is_empty(),"flight retires at its range bound");

        // The real Tiger hull blocks bullets from above without mapping its roof to an invented
        // front plate. The small-arm terminal branch leaves every armor/module value untouched.
        let (lo,hi)=rig_armor_bounds(crate::ballistics::ArmorScheme::TigerI,ArmorPart::Hull).unwrap();
        let center=g.player.hull_model().transform_point3((lo+hi)*0.5);
        let armor=vehicle_contact(&g.player,ProjectileVehicle::Player,
            crate::ballistics::ArmorScheme::TigerI,center+Vec3::Y*10.0,center-Vec3::Y*3.0).unwrap();
        let ProjectileContactKind::Vehicle(surface)=armor.kind else { panic!("expected armor"); };
        assert!(surface.world_normal.y>0.5);
        let mut bullet=AuthoritativeProjectile::new(9,ProjectileShooter::Enemy(0),&RIFLE,
            center+Vec3::Y*10.0,Vec3::NEG_Y,9);
        bullet.small_arm=Some(Payload {shot,damage:100.0,mixed:false,telemetry:None});
        let alive=g.player.alive;
        let hp=g.player.hp;
        let scars=g.armor_scars.clone();
        g.finish_small_arm(&bullet,armor,bullet.small_arm.unwrap(), &r.camera);
        assert_eq!(g.player.alive,alive);
        assert_eq!(g.player.hp,hp);
        assert_eq!(g.armor_scars,scars);
        println!("[small-arms] delayed damage, thin cover, moving target, range retirement and inert roof impact PASS");
        Ok(())
    }

    #[test]
    fn bullet_drop_wind_and_elapsed_flight_are_authoritative_and_substep_stable() {
        for shell in [&PISTOL,&RIFLE] {
            let initial=AuthoritativeProjectile::new(1,ProjectileShooter::Player,shell,Vec3::Y*10.0,Vec3::Z,1);
            let mut bullet=initial.clone();
            let wind=Vec3::X*4.0;
            for _ in 0..240 { bullet.commit_step(bullet.swept_step_with_wind(1.0/240.0,wind)); }
            let exact=initial.swept_step_with_wind(1.0,wind);
            assert!((bullet.pos-exact.end).length()<0.003);
            assert!((bullet.pos.y-5.095).abs()<0.003);
            assert!(bullet.pos.x>0.2 && bullet.pos.x<4.0);
            assert!(bullet.pos.z<shell.muzzle_velocity);
        }
    }
}
