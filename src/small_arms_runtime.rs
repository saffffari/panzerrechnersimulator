//! Live small arms share the host's swept 240 Hz flight clock with tank shells. Damage and
//! particles are emitted at contact, never at trigger time. Offline policy training retains its
//! renderer-free hitscan approximation; this adapter owns the live game's flight delay.
use super::*;
use crate::infantry_combat::{CoverSet, Shot, Team, Weapon};

const PLAYER: u32 = u32::MAX;
const TANK_BASE: u32 = 10_000;

/// Visibility retains world/other-vehicle cover and replaces only the player's coarse armor
/// envelope with the articulated source surfaces. Movement still uses the existing occupancy.
pub(super) struct PlayerSightCover<'a> {
    pub world: &'a world_index::WorldCover,
    pub movement: &'a world_index::WorldCover,
    pub source: &'a surface_fire::CabinFire,
    pub models: &'a [Mat4;surface_fire::FRAMES],
}
impl crate::infantry_combat::CoverSet for PlayerSightCover<'_> {
    fn first_hit(&self,origin:Vec3,direction:Vec3,range:f32)->Option<f32> {
        let world=self.world.first_hit(origin,direction,range);
        let own=self.source.sweep(origin,origin+direction*range,self.models).map(|hit|hit.fraction*range);
        world.into_iter().chain(own).min_by(f32::total_cmp)
    }
    fn occupies(&self,point:Vec3,margin:f32,y0:f32,y1:f32)->bool {
        self.movement.occupies(point,margin,y0,y1)
    }
}

#[path = "small_arms_profiles.rs"]
pub(super) mod profiles;

#[derive(Clone, Copy, Debug)]
pub(super) struct Payload {
    pub emitted_at_s:f32,
    pub shot: Shot,
    pub damage: f32,
    pub tracer: bool,
    pub round: u32,
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

fn source_armor_contact(source:&surface_fire::CabinFire,models:&[Mat4;surface_fire::FRAMES],
    tank:&Tank,start:Vec3,end:Vec3)->Option<ProjectileContact> {
        let hit=source.sweep(start,end,models)?;
        let part=if matches!(hit.frame,0|3|5|9|10|11|14) {ArmorPart::Hull} else {ArmorPart::Turret};
        let pos=start.lerp(end,hit.fraction);
        let normal=models[hit.frame].transform_vector3(hit.normal).normalize_or_zero();
        let inverse=(if part==ArmorPart::Hull {tank.hull_model()} else {tank.turret_model()}).inverse();
        Some(ProjectileContact {t_m:start.distance(end)*hit.fraction,pos,
            kind:ProjectileContactKind::Vehicle(VehicleArmorContact {vehicle:ProjectileVehicle::Player,
                part,facing:facing_from_local_normal(inverse.transform_vector3(normal),part),
                local_point:inverse.transform_point3(pos),surface_plate:None,world_normal:normal})})
}

fn exposed_crew_contact(hull:Mat4,bodies:&[CouchCrewBody],start:Vec3,end:Vec3)->Option<ProjectileContact> {
        let inverse=hull.inverse();
        let a=inverse.transform_point3(start);let b=inverse.transform_point3(end);
        let mut best=None;
        for body in bodies {
            for volume in body.hit_volumes {
                if let Some(fraction)=crew_damage::swept_sphere_obb_fraction(a,b,0.0,volume) {
                    consider_projectile_contact(&mut best,Some(ProjectileContact {
                        t_m:start.distance(end)*fraction,pos:start.lerp(end,fraction),
                        kind:ProjectileContactKind::ExposedCrew(body.casualty,volume.part),
                    }),start.distance(end));
                }
            }
        }
        best
}

impl Game {
    pub fn capture_small_arms_receipt(&self)->serde_json::Value {
        let mut receipt=self.weapon_vfx.small_arms_receipt();
        receipt["listener_state"]=serde_json::json!({"couch_peer_id":self.couch_local_peer,
            "couch_peer_kia":self.couch_local_peer.map(|peer|self.couch_peer_kia(peer)),
            "ear_silenced":self.audio.as_ref().map(|audio|audio.is_silenced()),
            "hatch_open":self.hatch_open,"hatch_fraction":self.hatch_t,
            "scope":"Actual local peer casualty ledger and ear gate, not inferred from vehicle HP"});
        receipt
    }
    pub(super) fn record_small_arm_visual_segment(&mut self, projectile:&AuthoritativeProjectile,
        step:ProjectileStep, end:Vec3, dt:f32, at:f32) {
        let Some(payload)=projectile.small_arm else {return;};
        let length=step.start.distance(step.end);
        let fraction=if length>1e-8 {step.start.distance(end)/length} else {0.0};
        self.weapon_vfx.record_bullet_segment(projectile.id, payload.tracer, step.start,end,
            at,at+dt*fraction,projectile.distance_m,projectile.distance_m+step.forward_m*fraction);
    }

    /// Retained source triangles include the moving cupola lid, roof hatches and glass.
    /// Bullets use these surfaces, never the solid armor envelope across a real aperture.
    pub(super) fn small_arm_player_armor(&self, start: Vec3, end: Vec3) -> Option<ProjectileContact> {
        source_armor_contact(&self.cabin_fire,&self.fire_surface_models(),&self.player,start,end)
    }

    pub(super) fn small_arm_crew_bodies(&self) -> Vec<CouchCrewBody> {
        if self.mode!=Mode::Tank || !self.player.alive {return Vec::new();}
        // Armor/source surfaces, not a camera flag, decide whether this body is exposed to a ray.
        self.physical_crew_bodies(BloodFrame::Hull)
    }

    pub(super) fn small_arm_exposed_crew_contact(&self,start:Vec3,end:Vec3)->Option<ProjectileContact> {
        exposed_crew_contact(self.player.hull_model(),&self.small_arm_crew_bodies(),start,end)
    }

    /// A perception target is sampled from the visible body's actual pose or the first source
    /// surface toward the hull. Normal sensor FOV/range and world occlusion still admit it.
    pub(super) fn small_arm_player_aim_point(&self, observer:Vec3)->Option<Vec3> {
        if self.mode==Mode::Foot {return (self.foot.hp>0.0).then(||self.foot.eye());}
        if !self.player.alive {return None;}
        for body in self.small_arm_crew_bodies() {
            let head=body.hit_volumes.iter().find(|v|v.part==crew_physics::CrewBodyPart::Head)?;
            let point=self.player.hull_model().transform_point3(head.center);
            if self.small_arm_player_armor(observer,point).is_none() {return Some(point);}
        }
        self.small_arm_player_armor(observer,self.player.center()).map(|hit|hit.pos)
    }

    pub(super) fn launch_small_arm(&mut self, shot: Shot, damage: f32, mixed: bool,
        telemetry: Option<&'static str>) {
        if !shot.origin.is_finite() || !shot.direction.is_finite()
            || shot.direction.length_squared() < 0.5 || !shot.range.is_finite() || shot.range <= 0.0 {
            return;
        }
        let round=self.weapon_vfx.next_small_arm_round(shot);
        let payload = Payload { emitted_at_s:self.enemy_report_audit.clock_s,
            shot, damage, tracer:profiles::tracer_round(shot.weapon,round), round, mixed, telemetry };
        let shell=profiles::profile(shot.weapon,shot.team);
        let shooter = if payload.is_player() { ProjectileShooter::Player }
            else if (TANK_BASE..TANK_BASE+N_ENEMIES as u32).contains(&shot.shooter) {
                ProjectileShooter::Enemy((shot.shooter-TANK_BASE) as usize)
            } else { ProjectileShooter::Enemy(usize::MAX) };
        let id = self.next_projectile_id;
        self.next_projectile_id = id.wrapping_add(1).max(1);
        let mut flight = AuthoritativeProjectile::new(id, shooter, shell, shot.origin,
            shot.direction, id as u32);
        flight.small_arm = Some(payload);
        self.weapon_vfx.record_small_arm_discharge(&flight);
        self.projectiles.push(flight);
        self.weapon_vfx.muzzle(shot.origin, shot.direction, payload.visual_weapon());
        if payload.is_player() { self.winter_report_discharge(shot.origin,
            if shot.weapon == Weapon::P08 { 240.0 } else { 520.0 }); }
    }

    pub(super) fn finish_small_arm(&mut self, projectile: &AuthoritativeProjectile,
        contact: ProjectileContact, payload: Payload, camera: &crate::camera::Camera) {
        let hit_player = matches!(contact.kind, ProjectileContactKind::Foot|ProjectileContactKind::ExposedCrew(..));
        self.weapon_vfx.record_small_arm_contact(projectile, contact);
        let hp_before = self.foot.hp;
        let weapon = payload.visual_weapon();
        let mut damage = 0.0;
        let mut friendly = false;
        match contact.kind {
            ProjectileContactKind::Vehicle(armor) => {
                self.weapon_vfx.bullet_armor_sparks(projectile.id, contact.pos, projectile.velocity,
                    armor.world_normal, projectile.shell.mass_kg, weapon);
                let on_own_armor = armor.vehicle == ProjectileVehicle::Player;
                let structure_borne = on_own_armor && self.mode == Mode::Tank;
                // Every resolved metal contact emits once. Misses and range retirement do not.
                // The shared master limiter, enclosure and propagation own the audible result.
                {
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
                    let mut sound = crate::acoustics::synth_mg_armor_hit(ImpactParams {
                        shell: ShellClass::Mg, caliber_mm: projectile.shell.caliber_mm,
                        mass_kg: projectile.shell.mass_kg, velocity: projectile.velocity.length(),
                        plate_thickness_mm: thickness, effective_mm: thickness / cosine,
                        impact_angle_deg: cosine.acos().to_degrees(), residual_v: 0.0,
                        behind_armor: 0.0, panel, penetrated: false,
                        bearing_deg: bearing, station_dist_m: if structure_borne { distance } else { 1.0 },
                        seed: projectile.seed,
                    });
                    if let Some(audio)=&self.audio {audio.blend_named_light_armor(&mut sound,bearing);}
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
                        self.play_stereo_and_hear_at_ear(sound, &ear, bearing,
                            crate::audio::EarBus::World);
                        let peer=self.couch_local_peer;
                        let peer_kia=peer.map(|id|self.couch_peer_kia(id));
                        self.weapon_vfx.record_small_arm_structural_sound(projectile.id,contact.pos,
                            self.enemy_report_audit.clock_s,self.audio.is_some(),
                            self.audio.as_ref().map(|audio|audio.is_silenced()),peer,peer_kia);
                    } else {
                        self.schedule_emission(crate::audio::Sfx::ImpactTank, contact.pos,
                            crate::sound_propagation::Emission::from_level(
                                &crate::sound_levels::BULLET_ON_STEEL_1M),
                            None, Some(sound));
                    }
                }
                // All small-arms profiles are inert against tank armor: no AP resolution, HP chip,
                // spall, or cabin fire. Roof/belly blocking below needs no invented plate values.
                // The strike still marks the steel: a splash crater and stripped paint that never
                // changes the plate's resistance (`ScarKind::Pock`).
                {
                    let frame = self.armor_contact_frame(&armor);
                    let to_local = frame.inverse();
                    let plate_mm = armor.surface_plate.map_or_else(|| match armor.vehicle {
                        ProjectileVehicle::Enemy(index) => enemy_armor(self.enemies[index].kind),
                        _ => crate::ballistics::ArmorScheme::TigerI,
                    }.plate(armor.facing).thickness_mm, |surface| surface.plate.thickness_mm);
                    let energy_j = 0.5 * projectile.shell.mass_kg * projectile.velocity.length_squared();
                    let normal = to_local.transform_vector3(armor.world_normal);
                    let dir = to_local.transform_vector3(projectile.velocity);
                    let caliber = projectile.shell.caliber_mm;
                    let now = self.armor_clock_s;
                    self.vehicle_scars_mut(armor.vehicle).record_pock(armor.facing, armor.local_point,
                        normal, dir, caliber, energy_j, plate_mm, now);
                }
            }
            ProjectileContactKind::Foot => {
                if !self.god {
                    damage = payload.damage.min(self.foot.hp.max(0.0));
                    self.foot.hp = (self.foot.hp-damage).max(0.0);
                    if damage > 0.0 { self.stat_small_arms_hits_on_foot = self.stat_small_arms_hits_on_foot.saturating_add(1); }
                }
                friendly = payload.shot.team == Team::German;
                self.weapon_vfx.impact(contact.pos, projectile.velocity.normalize_or_zero(), 0.10, false, weapon);
            }
            ProjectileContactKind::ExposedCrew(casualty,part) => {
                // Existing crew policy treats an actual energetic body contact as incapacitating.
                // It is not the foot avatar's HP pool and never damages another occupied post.
                let energy=0.5*projectile.shell.mass_kg*projectile.velocity.length_squared();
                let result=crew_damage::resolve_crew_damage(crew_damage::CrewDamageInput {
                    part,fragment_energy_j:energy,explosive_energy_j:0.0,blast_distance_m:0.0});
                let frame=blood_frame_for_role(casualty.role);
                let parent=blood_parent_model(&self.player,frame);
                let inverse=parent.inverse();
                let event=CrewPartDamageEvent {result,strongest_contact:Some(FragmentDamageContact {
                    fragment_id:projectile.id,target:FragmentDamageTarget::CouchCrew(casualty),body_part:Some(part),
                    local_point:inverse.transform_point3(contact.pos),local_velocity:inverse.transform_vector3(projectile.velocity),
                    speed_mps:projectile.velocity.length(),energy_j:energy,internal_blast_origin_local:None}),
                    wound_local:inverse.transform_point3(contact.pos),
                    ejection_local:inverse.transform_vector3(projectile.velocity.normalize_or_zero())};
                if !self.god && result.outcome!=crew_damage::CrewTissueOutcome::None {
                    if !crew_part_event_is_immediately_fatal(&event) {
                        if let Some(body)=self.physical_crew_bodies(frame).into_iter().find(|b|b.casualty.peer_id==casualty.peer_id) {
                            self.living_wounds.detach(&body,&[event],parent,self.player.pos.y+0.5);
                        }
                        self.living_wounds.record(casualty,&[event]);
                        self.spawn_cubic_blood(casualty,Some(contact.pos),projectile.velocity.normalize_or_zero(),
                            Vec3::ZERO,1.0,1.0,projectile.seed,0,visual_blood_request(result.blood_cube_count).min(64));
                    }else {
                    let before=self.couch_casualties.len();
                    let wiped=self.record_physical_crew_casualties(&[casualty]);
                    if self.couch_casualties.len()>before {
                        damage=100.0;
                        self.crew_ko_t=CREW_KO_DUR;
                        self.crew_ko_world_dir=projectile.velocity.normalize_or_zero();
                        self.spawn_couch_ragdoll_impact(casualty,CrewRagdollImpact {
                            hit_part:part,point_world:contact.pos,velocity_world:projectile.velocity,
                            energy_j:energy,blast_origin_world:contact.pos,explosive_energy_j:0.0,
                            behind_armor:0.0,blast_proximity_m:0.0,
                        },projectile.seed);
                    }
                    if wiped {self.player.alive=false;self.death_cause=Some(DeathCause::CrewLost);}
                    self.recompute_hp();
                    }
                }
                friendly=payload.shot.team==Team::German;
                self.weapon_vfx.impact(contact.pos,projectile.velocity.normalize_or_zero(),0.10,false,weapon);
            }
            ProjectileContactKind::Infantry(index) => {
                friendly=self.mixed_combat.actors.get(index).is_some_and(|a|a.team==payload.shot.team);
                let before=self.mixed_combat.actors.get(index).map_or(100.0,|a|a.hp);
                self.infantry_contact(index,contact.pos,projectile.velocity,projectile.shell.mass_kg,0.0,false,"ballistic small arms");
                damage=before-self.mixed_combat.actors.get(index).map_or(before,|a|a.hp);
                self.weapon_vfx.impact(contact.pos, projectile.velocity.normalize_or_zero(), 0.10, false, weapon);
            }
            ProjectileContactKind::RangeLimit => {} // Retirement never invents an impact in air.
            // A gun shield or any other steel of an anti-tank gun: sparks, as on tank armour.
            ProjectileContactKind::AtGun(hit) if hit.steel => {
                self.weapon_vfx.bullet_armor_sparks(projectile.id, contact.pos, projectile.velocity,
                    hit.normal, projectile.shell.mass_kg, weapon);
                self.schedule_emission(crate::audio::Sfx::ArmorClang1, contact.pos,
                    crate::sound_propagation::Emission::from_level(&crate::sound_levels::BULLET_ON_STEEL_1M),
                    None, None);
            }
            _ => {
                if !matches!(contact.kind,ProjectileContactKind::Terrain) || !self.weapon_vfx.uses_native_ground() {
                    self.weapon_vfx.impact(contact.pos, projectile.velocity.normalize_or_zero(), 0.20, false, weapon);
                }
                { // Every actual world contact emits once; no projectile-ID decimation.
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
        if matches!(contact.kind,ProjectileContactKind::Foot|ProjectileContactKind::ExposedCrew(..)|ProjectileContactKind::Infantry(_)) {
            let mono=crate::acoustics::synth_small_arm_body_hit(projectile.velocity.length(),
                projectile.shell.caliber_mm,projectile.seed);
            let stereo=crate::acoustics::spatialize_mono(&mono,0.0,0.0);
            self.schedule_emission(crate::audio::Sfx::ImpactGround,contact.pos,
                crate::sound_propagation::Emission::from_level(&crate::sound_levels::BULLET_ON_SOFT_BODY_1M),
                None,Some(stereo));
        }
        if payload.mixed { self.record_small_arm_damage(payload.shot, damage, friendly); }
        if let Some(weapon) = payload.telemetry.filter(|_| self.campaign_telemetry_enabled) {
            let target = if matches!(contact.kind,ProjectileContactKind::ExposedCrew(..)) {contact.pos} else {self.foot.eye()};
            self.campaign_pedestrian_shots.push(crate::campaign_ai::PedestrianShotEvent {
                time_s: 0.0, weapon,
                shooter: Vec2::new(payload.shot.origin.x, payload.shot.origin.z),
                target: Vec2::new(target.x, target.z), distance_m: projectile.distance_m,
                target_crouched: self.mode==Mode::Foot && self.foot.crouch_t > 0.5, target_visible: true,
                hit: hit_player, killed: (hp_before > 0.0 && self.foot.hp <= 0.0)
                    || (matches!(contact.kind,ProjectileContactKind::ExposedCrew(..)) && damage>0.0),
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
                local_point:a.lerp(b,fraction), surface_plate:None,world_normal:model.transform_vector3(normal).normalize_or_zero(),
            }) };
        consider_projectile_contact(&mut nearest,Some(contact),start.distance(end));
    }
    nearest
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_arm_partial_birth_tick_survives_until_actual_range_budget() {
        use crate::infantry_combat::{Shot, Team, Weapon};
        for first_dt in [0.00000048, PROJECTILE_FIXED_DT * 0.25, PROJECTILE_FIXED_DT] {
            let shell=profiles::profile(Weapon::MachineGun,Team::German);
            let mut p=AuthoritativeProjectile::new(1,ProjectileShooter::Player,shell,Vec3::ZERO,Vec3::Z,1);
            p.small_arm=Some(small_arms_runtime::Payload {emitted_at_s:0.0,
                shot:Shot {shooter:u32::MAX,team:Team::German,origin:Vec3::ZERO,
                    direction:Vec3::Z,range:120.0,weapon:Weapon::MachineGun},
                damage:20.0,tracer:true,round:1,mixed:false,telemetry:None});
            p.commit_step(p.swept_step(first_dt));
            assert!(!p.range_expired(120.0),"fractional birth must not retire a live bullet");
            for _ in 0..200 {
                if p.range_expired(120.0) {break;}
                let dt=PROJECTILE_FIXED_DT.min((shell.time_of_flight(120.0)-p.age_s).max(0.0));
                p.commit_step(p.swept_step(dt));
            }
            assert!(p.range_expired(120.0));
            assert!((p.distance_m-120.0).abs()<0.01);
            assert!((p.age_s-shell.time_of_flight(120.0)).abs()<0.00002);
        }
    }

    #[test]
    fn source_cupola_closed_armor_wins_and_open_exposed_body_is_reachable() -> anyhow::Result<()> {
        let(parts,_,_,_,_,_,_,_)=tiger_hatches::load_player_tiger()?;
        let lid=crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hatch.glb")?;
        let source=surface_fire::CabinFire::new(&[(0,&parts[0]),(1,&parts[1]),(2,&parts[2]),(6,&lid)]);
        let hf=Heightfield {res:2,spacing:2000.0,heights:vec![0.0;4],min_h:0.0,max_h:0.0,openings:Default::default(),detail:None};
        for heading in [0.0,1.17] {
            let mut tank=Tank::new(19.0,-32.0,heading,&hf,tank::TIGER_PARTS);
            tank.pitch=0.08;tank.roll=-0.05;tank.turret_yaw=0.61;
            let hull=tank.hull_model();let turret=tank.turret_model();
            let mut models=[hull;surface_fire::FRAMES];models[1]=turret;models[2]=tank.gun_model();
            models[6]=cupola_hatch_model(turret,0.0);
            let below=turret.transform_point3(tiger_periscopes::COMMANDER_EYE);
            let above=below+turret.transform_vector3(Vec3::Y)*8.0;
            let closed=source_armor_contact(&source,&models,&tank,above,below)
                .expect("actual closed cupola lid blocks the incoming path");
            assert!(closed.t_m<above.distance(below));
            models[6]=cupola_hatch_model(turret,1.0);
            assert!(source_armor_contact(&source,&models,&tank,above,below).is_none(),
                "the authored open cupola aperture must not become a solid OBB roof");
            let casualty=CouchCrewCasualty {peer_id:7,identity:crate::lan::LanRole::Commander,
                role:crate::lan::LanRole::Commander};
            let(scale,mut segments)=crew_pose_segments_in_frame(&tank,casualty.role,BloodFrame::Hull);
            let head_index=crew_physics::CrewBodyPart::Head.index();
            let exposed=tank.commander_eye()+Vec3::Y*0.52-Vec3::Y*0.06;
            let shift=hull.inverse().transform_point3(exposed)-segments[head_index].pos;
            for part in &mut segments {part.pos+=shift;}
            let body=CouchCrewBody {casualty,pos:hull.inverse().transform_point3(exposed),
                hit_volumes:crew_hit_volumes_from_segments(&segments,scale)};
            let from=exposed+turret.transform_vector3(Vec3::Z)*12.0;
            let mut nearest=source_armor_contact(&source,&models,&tank,from,exposed);
            consider_projectile_contact(&mut nearest,exposed_crew_contact(hull,&[body],from,exposed),12.1);
            assert!(matches!(nearest.unwrap().kind,ProjectileContactKind::ExposedCrew(id,_) if id==casualty));
            let contact=nearest.unwrap();
            consider_projectile_contact(&mut nearest,Some(ProjectileContact {t_m:1.0,
                pos:from+(exposed-from).normalize(),kind:ProjectileContactKind::HardCover}),12.1);
            assert!(matches!(nearest.unwrap().kind,ProjectileContactKind::HardCover));
            assert!(contact.t_m>1.0);
            assert!(exposed_crew_contact(Mat4::from_translation(Vec3::Y*4.0)*hull,&[body],from,exposed).is_none(),
                "the next swept query must use the live body pose rather than the old target point");
        }
        Ok(())
    }

    #[test]
    fn mounted_surface_target_is_only_remembered_after_real_sensor_admission() {
        use crate::infantry_combat::{self as combat,Combatant,Cover};
        let point=Vec3::new(0.0,2.9,-20.0);
        let mut target=Combatant::new(PLAYER,Team::German,point,0.0);
        target.perception_point=Some(point);
        let observer=||Combatant::new(2,Team::Soviet,Vec3::ZERO,0.0);
        let mut clear=observer();
        combat::observe(&mut clear,&[target.clone()],&[],Vec3::ZERO,1.0/60.0);
        assert_eq!(clear.last_observed_aim_point(),Some(point));
        let wall=[Cover {min:Vec3::new(-3.0,0.0,-11.0),max:Vec3::new(3.0,5.0,-10.0)}];
        let mut blocked=observer();
        combat::observe(&mut blocked,&[target.clone()],&wall,Vec3::ZERO,1.0/60.0);
        assert_eq!(blocked.last_seen,None);
        let mut behind=observer();behind.yaw=std::f32::consts::PI;
        combat::observe(&mut behind,&[target.clone()],&[],Vec3::ZERO,1.0/60.0);
        assert_eq!(behind.last_seen,None);
        let mut far=observer();target.pos.z=-80.0;target.perception_point=Some(target.pos);
        combat::observe(&mut far,&[target],&[],Vec3::ZERO,1.0/60.0);
        assert_eq!(far.last_seen,None);
        assert_eq!(combat::small_arms_damage(Weapon::Rifle,true),0.0);
        assert!(combat::small_arms_damage(Weapon::Rifle,false)>0.0);
    }

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
        // Production update advances the simulation/audio clock before ballistic sweeps.
        // This isolated fixture must advance that same clock instead of holding it at zero.
        let advance=|g:&mut Game,r:&mut Renderer,dt:f32| {
            r.time+=dt;g.enemy_report_audit.clock_s=r.time;g.advance_projectiles(dt,r);
        };
        let reset=|g:&mut Game,r:&mut Renderer| {
            r.time=0.0;g.enemy_report_audit.clock_s=0.0;
            g.projectiles.clear();g.projectile_accumulator_s=0.0;
            g.soldiers[0].alive=true;g.soldiers[0].pos=body;
            g.mixed_combat.actors[0].hp=100.0;
            g.mixed_combat.actors[0].pos=body;
        };
        reset(&mut g,&mut r);
        g.launch_small_arm(shot,100.0,false,None);
        assert!(g.soldiers[0].alive,"trigger time must not deal damage");
        advance(&mut g,&mut r,0.02);
        assert!(g.soldiers[0].alive,"bullet is still short of the target");
        advance(&mut g,&mut r,0.06);
        assert!(!g.soldiers[0].alive,"swept contact must apply terminal damage");

        reset(&mut g,&mut r);
        g.story_cover.push((Vec3::new(19.0,floor,19.995),Vec3::new(21.0,floor+2.0,20.005)));
        g.launch_small_arm(shot,100.0,false,None);
        advance(&mut g,&mut r,0.10);
        assert!(g.soldiers[0].alive,"a thin wall must block the swept bullet");
        assert!(g.projectiles.is_empty());
        g.story_cover.clear();

        reset(&mut g,&mut r);
        g.launch_small_arm(shot,100.0,false,None);
        g.soldiers[0].pos.x+=5.0;
        advance(&mut g,&mut r,0.20);
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
        let mut bullet=AuthoritativeProjectile::new(9,ProjectileShooter::Enemy(0),&profiles::GERMAN_RIFLE,
            center+Vec3::Y*10.0,Vec3::NEG_Y,9);
        bullet.small_arm=Some(Payload {emitted_at_s:0.0,shot,damage:100.0,tracer:false,round:1,mixed:false,telemetry:None});
        let alive=g.player.alive;
        let hp=g.player.hp;
        let roof=crate::ballistics::ArmorScheme::TigerI.plate(surface.facing);
        let before=g.armor_scars.plate_at(roof,surface.facing,surface.local_point).thickness_mm;
        g.finish_small_arm(&bullet,armor,bullet.small_arm.unwrap(), &r.camera);
        assert_eq!(g.player.alive,alive);
        assert_eq!(g.player.hp,hp);
        assert_eq!(g.armor_scars.plate_at(roof,surface.facing,surface.local_point).thickness_mm,before);
        assert!(g.armor_scars.scars().iter().any(|scar|scar.kind==crate::armor_damage::ScarKind::Pock),
            "the bullet left a visible pock");
        println!("[small-arms] delayed damage, thin cover, moving target, range retirement and inert roof impact PASS");
        Ok(())
    }

    #[test]
    fn bullet_drop_wind_and_elapsed_flight_are_authoritative_and_substep_stable() {
        for shell in [&profiles::PISTOL,&profiles::GERMAN_RIFLE] {
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
