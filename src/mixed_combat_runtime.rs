//! Game adapter for the shared infantry combat model. Visual meshes remain owned by Game;
//! this module keeps per-actor physical state and validated policies between simulation frames.
use super::*;
use crate::{anti_infantry_ai as soft_ai, infantry_combat as combat};
use combat::{Combatant, Cover, Team, Weapon};
pub(super) const GERMAN_COUNT: usize = 4;
const PLAYER_ACTOR_ID: u32 = u32::MAX;

/// Keep allies available to the shared observation, but query terrain, cover and physical doors
/// only for enemies the same sensor could admit. Rejected enemies cannot affect observe's target,
/// memory or ally selection, so this changes work rather than perception or policy cadence.
fn sensed_actors(
    actor: &Combatant,
    snapshot: &[Combatant],
    mut world_los: impl FnMut(Vec3, Vec3) -> bool,
) -> Vec<Combatant> {
    snapshot.iter().filter(|other| {
        other.team == actor.team
            || (combat::sight_candidate(actor, other) && world_los(actor.eye(), other.eye()))
    }).cloned().collect()
}

fn permitted_infantry_action(mut action: FootAction, return_fire: bool) -> FootAction {
    if !return_fire {
        action.forward = 0.0;
        action.strafe = 0.0;
        action.turn = 0.0;
        action.fire = false;
    }
    action
}

fn enemy_native_he(kind: EnemyKind) -> Option<&'static crate::ballistics::NativeHeProfile> {
    use crate::ballistics::{native_soviet_he, SovietGun};
    native_soviet_he(match kind {
        EnemyKind::T34 => SovietGun::F34,
        EnemyKind::Su100 => SovietGun::D10S,
        EnemyKind::Kv2 => SovietGun::M10T,
        EnemyKind::Isu152 => SovietGun::Ml20S,
        EnemyKind::Is2 | EnemyKind::Is3 => SovietGun::D25T,
    })
}
#[derive(Default)]
pub(super) struct TankWeapons {
    heat: f32,
    overheated: bool,
    timer: f32,
    round: u32,
    he_ammo: u32,
    mg_ammo: u32,
    target: Option<(u32, Vec3)>,
    aim: Option<Vec3>,
    laid: bool,
    requested_mg: bool,
    requested_he: bool,
    he_available: bool,
}
pub(super) struct MixedCombatRuntime {
    pub enabled: bool,
    pub actors: Vec<Combatant>,
    german_policy: Option<FootPolicy>,
    soviet_policy: Option<FootPolicy>,
    tank_policy: Option<soft_ai::Policy>,
    pub tank_weapons: Vec<TankWeapons>,
    tick: u64,
    decision_clock: f32,
    actions: Vec<FootAction>,
    rifle_shots: [u64; 2],
    damage: [f32; 2],
    friendly_damage: [f32; 2],
    mg_shots: u64,
    he_shots: u64,
    he_impacts: u64,
    he_damage: f32,
    auxiliary_he_ids: std::collections::BTreeSet<u64>,
    primary_he_impacts: u64,
    primary_he_damage: f32,
    mg_damage: f32,
    policy_directory: Option<String>,
}
impl MixedCombatRuntime {
    fn record_he_contact(&mut self, id: u64, shooter: ProjectileShooter) -> bool {
        let auxiliary = self.auxiliary_he_ids.remove(&id);
        if matches!(shooter, ProjectileShooter::Enemy(_)) {
            if auxiliary {
                self.he_impacts += 1;
            } else {
                self.primary_he_impacts += 1;
            }
        }
        auxiliary
    }
    pub fn new(soldiers: &[Soldier]) -> Self {
        // Explicit evaluation never goes through the legacy OnceLock checkpoint cache.
        let dir = std::env::var("KESSEL_COMBAT_WEIGHTS_DIR")
            .ok()
            .or_else(|| std::env::var("KESSEL_COMBAT_POLICY_DIR").ok());
        let load = |role: PolicyRole, name: &str| {
            let path = dir
                .as_ref()
                .map(|d| std::path::Path::new(d).join(name))
                .unwrap_or_else(|| role.checkpoint_path().into());
            match FootPolicy::load(path.to_string_lossy().as_ref(), role) {
                Ok((p, m)) => {
                    println!(
                        "[mixed-combat] {} loaded generation {} from {}",
                        role.label(),
                        m.generation,
                        path.display()
                    );
                    Some(p)
                }
                Err(e) => {
                    eprintln!(
                        "[mixed-combat] {} uses combat doctrine; checkpoint {} unavailable: {e}",
                        role.label(),
                        path.display()
                    );
                    None
                }
            }
        };
        let tank_policy = dir.as_ref().and_then(|d| {
            let path = std::path::Path::new(d).join("tank_soft_targets.json");
            match soft_ai::Policy::load(&path) {
                Ok(p) => Some(p),
                Err(e) => {
                    eprintln!("[mixed-combat] tank auxiliary uses doctrine: {e}");
                    None
                }
            }
        });
        Self {
            enabled: std::env::var("KESSEL_MIXED_COMBAT").as_deref() != Ok("0"),
            actors: soldiers
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    Combatant::new(
                        i as u32,
                        if i < N_SOLDIERS {
                            Team::Soviet
                        } else {
                            Team::German
                        },
                        s.pos,
                        s.yaw,
                    )
                })
                .collect(),
            german_policy: load(PolicyRole::GermanRifle, "german_rifle.ksl2"),
            soviet_policy: load(PolicyRole::SovietRifle, "soviet_rifle.ksl2"),
            tank_policy,
            tank_weapons: (0..N_ENEMIES)
                .map(|_| TankWeapons {
                    he_ammo: 8,
                    mg_ammo: 300,
                    ..Default::default()
                })
                .collect(),
            tick: 0,
            decision_clock: 0.0,
            actions: vec![FootAction::default(); soldiers.len()],
            rifle_shots: [0; 2],
            damage: [0.0; 2],
            friendly_damage: [0.0; 2],
            mg_shots: 0,
            he_shots: 0,
            he_impacts: 0,
            he_damage: 0.0,
            auxiliary_he_ids: Default::default(),
            primary_he_impacts: 0,
            primary_he_damage: 0.0,
            mg_damage: 0.0,
            policy_directory: dir,
        }
    }
}
/// The existing CC0 asset has its own German uniform and helmet. Its static relaxed pose is
/// intentionally kept distinct from the Soviet pose library. It remains the standing/corpse
/// fallback; living Germans use the independently validated articulated pool when available.
pub(super) fn german_infantry_mesh() -> CpuMesh {
    let path = "assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_ww2_soldier_relaxed_static.glb";
    match crate::gltf_load::load_glb(path) {
        Ok(mesh) => {
            let height = mesh.max.y - mesh.min.y;
            if height.is_finite() && height > 0.1 {
                let center = mesh.center();
                let floor = Vec3::new(center.x, mesh.min.y, center.z);
                return mesh.transformed(
                    Mat4::from_scale(Vec3::splat(1.75 / height)) * Mat4::from_translation(-floor),
                );
            }
            eprintln!("[mixed-combat] invalid German model bounds; using grey silhouette");
        }
        Err(error) => eprintln!("[mixed-combat] German model unavailable: {error}"),
    }
    soviet_infantry_mesh([83, 89, 85])
}
impl Game {
    fn mixed_cover(&self, ignored_enemy: Option<usize>) -> Vec<Cover> {
        let mut cover: Vec<Cover> = self
            .destructibles
            .iter()
            .filter(|s| s.alive)
            .map(|s| Cover {
                min: s.min,
                max: s.max,
            })
            .collect();
        cover.extend(
            self.story_cover
                .iter()
                .map(|&(min, max)| Cover { min, max }),
        );
        // Conservative world AABBs derived from measured articulated armor bounds. They are
        // visibility/collision proxies, not replacement render geometry or penetrable material.
        let mut push_tank = |tank: &Tank, scheme| {
            for part in [ArmorPart::Hull, ArmorPart::Turret] {
                if let Some((lo, hi)) = rig_armor_bounds(scheme, part) {
                    let model = if part == ArmorPart::Hull {
                        tank.hull_model()
                    } else {
                        tank.turret_model()
                    };
                    let mut min = Vec3::splat(f32::INFINITY);
                    let mut max = Vec3::splat(f32::NEG_INFINITY);
                    for x in [lo.x, hi.x] {
                        for y in [lo.y, hi.y] {
                            for z in [lo.z, hi.z] {
                                let p = model.transform_point3(Vec3::new(x, y, z));
                                min = min.min(p);
                                max = max.max(p);
                            }
                        }
                    }
                    cover.push(Cover { min, max });
                }
            }
        };
        push_tank(&self.player, crate::ballistics::ArmorScheme::TigerI);
        for (i, tank) in self.enemies.iter().enumerate() {
            if Some(i) != ignored_enemy && self.enemy_active.get(i).copied().unwrap_or(false) {
                push_tank(tank, enemy_armor(tank.kind));
            }
        }
        for ally in &self.allies {
            push_tank(&ally.tank, crate::ballistics::ArmorScheme::TigerI);
        }
        cover
    }
    fn mixed_sync_actors(&mut self) {
        for (actor, soldier) in self.mixed_combat.actors.iter_mut().zip(&self.soldiers) {
            if !soldier.alive {
                actor.hp = 0.0;
                continue;
            }
            if !actor.alive() || actor.pos.distance(soldier.pos) > 1.0 {
                let mut fresh = Combatant::new(actor.id, actor.team, soldier.pos, soldier.yaw);
                fresh.cooldown = 0.5;
                *actor = fresh;
            }
            actor.pos = soldier.pos;
            actor.yaw = soldier.yaw;
            // Loading is transactional: absent derivative geometry cannot grant an invisible
            // crouch. Renderer-free training uses the complete shipped capability contract.
            actor.pose_support = if actor.team == Team::German && self.german_pose_batches.is_some() {
                combat::PoseSupport::GermanArticulatedV1
            } else {
                combat::PoseSupport::StandingOnly
            };
        }
    }
    fn mixed_player_actor(&self) -> Option<Combatant> {
        if !matches!(self.mode, Mode::Foot) || self.foot.hp <= 0.0 {
            return None;
        }
        let mut actor = Combatant::new(PLAYER_ACTOR_ID, Team::German, self.foot.pos, self.foot.yaw);
        actor.pose_support = combat::PoseSupport::InstantCrouch;
        actor.hp = self.foot.hp;
        actor.crouched = self.foot.crouch_t > 0.5;
        Some(actor)
    }
    fn mixed_launch_small_arm(&mut self, shot: combat::Shot) {
        self.launch_small_arm(shot, combat::small_arms_damage(shot.weapon,false), true, None);
    }
    pub(super) fn record_small_arm_damage(&mut self, shot:combat::Shot, damage:f32, friendly:bool) {
        let team = if shot.team == Team::German { 0 } else { 1 };
        if friendly { self.mixed_combat.friendly_damage[team] += damage; }
        else {
            self.mixed_combat.damage[team] += damage;
            if shot.weapon == Weapon::MachineGun && shot.team == Team::Soviet {
                self.mixed_combat.mg_damage += damage;
            }
        }
    }
    pub(super) fn update_mixed_infantry(&mut self, dt: f32, r: &mut Renderer) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        for event in &mut self.noise {
            event.ttl = (event.ttl - dt).max(0.0);
        }
        self.reanchor_infantry_escorts(dt);
        self.mixed_combat.decision_clock -= dt;
        let decide = self.mixed_combat.decision_clock <= 0.0;
        if decide {
            self.mixed_combat.decision_clock = 0.2;
        } // same 5 Hz policy cadence as training

        self.mixed_sync_actors();
        let cover = self.mixed_cover(None);
        let mut snapshot = self.mixed_combat.actors.clone();
        if let Some(player) = self.mixed_player_actor() {
            snapshot.push(player);
        }
        let mut shots = Vec::new();
        let return_fire = self.enemy_return_fire_allowed();
        for i in 0..self.mixed_combat.actors.len() {
            if !self.mixed_combat.actors[i].alive() {
                continue;
            }
            // Terrain/trees participate in sight, not just in the later bullet collision test.
            let seen = sensed_actors(&self.mixed_combat.actors[i], &snapshot,
                |from, to| self.los_clear(from, to));
            let objective = if i >= N_SOLDIERS {
                self.player.pos + self.player.forward() * 18.0
            } else {
                self.soldiers[i].route[self.soldiers[i].leg.min(3)]
            };
            let actor = &mut self.mixed_combat.actors[i];
            let obs = combat::observe(actor, &seen, &cover, objective, dt);
            let policy = if actor.team == Team::German {
                self.mixed_combat.german_policy.as_ref()
            } else {
                self.mixed_combat.soviet_policy.as_ref()
            };
            if decide {
                self.mixed_combat.actions[i] = policy
                    .and_then(|p| p.decide(&obs).ok())
                    .unwrap_or_else(|| crate::foot_ai::micro_expert(&obs));
            }
            // Practice retains physical target bodies, but NPCs neither pursue nor discharge.
            // Gate before apply_action consumes ammunition or creates shot/pose/audio events.
            let action = permitted_infantry_action(self.mixed_combat.actions[i], return_fire);
            let before = actor.pos;
            let shot =
                combat::apply_action(actor, &obs, action, &cover, dt, self.mixed_combat.tick);
            actor.pos = self.world_doors.resolve_walk(before, actor.pos, crate::foot::STANDING_HEIGHT);
            if dt > 1e-6 { actor.velocity = (actor.pos - before) / dt; }
            actor.pos.y = self.hf.height_at(actor.pos.x, actor.pos.z);
            if i < N_SOLDIERS && actor.last_seen.is_none() && actor.pos.distance(objective) < 1.5 {
                self.soldiers[i].leg = (self.soldiers[i].leg + 1) % 4;
            }
            self.soldiers[i].apply_combat_pose(
                actor.pos,
                actor.yaw,
                actor.velocity,
                actor.aiming,
                actor.crouch_stage(),
                shot.is_some(),
            );
            if let Some(shot) = shot {
                shots.push(shot);
            }
        }
        for shot in shots {
            let team = if shot.team == Team::German { 0 } else { 1 };
            self.mixed_combat.rifle_shots[team] += 1;
            self.mixed_launch_small_arm(shot);
            let k = self.next_inf_fx;
            self.next_inf_fx = (k + 1) % INF_FX;
            self.inf_flash_t[k] = 0.06;
            self.inf_flash_pos[k] = shot.origin + shot.direction * 0.4;
            self.schedule_sound_from(crate::audio::Sfx::RifleShot, shot.origin, 1.0);
        }
        self.mixed_combat.tick = self.mixed_combat.tick.wrapping_add(1);
        self.drive_infantry_visuals(dt, r);
        self.check_foot_death();
    }
    /// Each real HE terminal event applies the same exterior casualty law as training. Damage is
    /// attributed at impact; every friendly/enemy body can be affected, and live cover blocks it.
    pub(super) fn mixed_he_blast(
        &mut self,
        projectile_id: u64,
        point: Vec3,
        filler_kg: f32,
        shooter: ProjectileShooter,
    ) {
        let auxiliary = self.mixed_combat.record_he_contact(projectile_id, shooter);
        self.mixed_sync_actors();
        let origin = point + Vec3::Y * 0.025;
        let source_team = if matches!(shooter, ProjectileShooter::Enemy(_)) {
            Team::Soviet
        } else {
            Team::German
        };
        let source_index = if source_team == Team::German { 0 } else { 1 };
        let armor_and_cover = self.mixed_cover(None);
        let mut casualties = Vec::new();
        for (i, actor) in self.mixed_combat.actors.iter().enumerate() {
            if !actor.alive() {
                continue;
            }
            let target = actor.pos + actor.torso_offset() * (0.9 / 1.15);
            let delta = target - origin;
            let distance = delta.length();
            let blocked = distance > 0.001
                && self
                    .nearest_cover_ray(origin, delta / distance, distance - 0.001)
                    .is_some();
            let blocked = blocked || !combat::clear_segment(origin, target, &armor_and_cover);
            let damage = combat::he_blast_damage(distance, filler_kg, blocked).min(actor.hp);
            if damage > 0.0 {
                casualties.push((i, damage));
            }
        }
        for (i, damage) in casualties {
            if self.mixed_combat.actors[i].team != source_team {
                self.mixed_combat.damage[source_index] += damage;
                if source_team == Team::Soviet {
                    if auxiliary {
                        self.mixed_combat.he_damage += damage;
                    } else {
                        self.mixed_combat.primary_he_damage += damage;
                    }
                }
            } else {
                self.mixed_combat.friendly_damage[source_index] += damage;
            }
            self.mixed_combat.actors[i].hp = (self.mixed_combat.actors[i].hp - damage).max(0.0);
            if self.mixed_combat.actors[i].hp <= 0.0 {
                self.kill_soldier(i, "HE fragments/blast");
            }
        }
        if let Some(player) = self.mixed_player_actor() {
            let delta = player.eye() - origin;
            let distance = delta.length();
            let blocked = distance > 0.001
                && self
                    .nearest_cover_ray(origin, delta / distance, distance - 0.001)
                    .is_some();
            let blocked = blocked || !combat::clear_segment(origin, player.eye(), &armor_and_cover);
            if !self.god {
                let damage =
                    combat::he_blast_damage(distance, filler_kg, blocked).min(self.foot.hp);
                self.foot.hp = (self.foot.hp - damage).max(0.0);
                if source_team == Team::Soviet {
                    self.mixed_combat.damage[source_index] += damage;
                    if auxiliary {
                        self.mixed_combat.he_damage += damage;
                    } else {
                        self.mixed_combat.primary_he_damage += damage;
                    }
                } else {
                    self.mixed_combat.friendly_damage[source_index] += damage;
                }
            }
        }
    }
    /// Select a seen German foot target without replacing the KSL1 tank-vs-tank policy. Armored
    /// combat retains priority when the tank is already tracking the living mounted Tiger.
    pub(super) fn mixed_tank_engagement(&mut self, i: usize, dt: f32) -> bool {
        if !self.mixed_combat.enabled
            || !self.enemies[i].alive
            || self.enemies[i].kind.casemate()
            || !self.enemy_active.get(i).copied().unwrap_or(false)
            || self.brawl
        {
            return false;
        }
        let state = &mut self.mixed_combat.tank_weapons[i];
        state.target = None;
        state.aim = None;
        state.laid = false;
        state.requested_mg = false;
        state.requested_he = false;
        state.heat = (state.heat - combat::MG_HEAT_LOSS * dt).max(0.0);
        if state.heat <= combat::MG_RESUME {
            state.overheated = false;
        }
        if matches!(self.mode, Mode::Tank)
            && self.player.alive
            && self.enemy_mode(i) == AiMode::Track
        {
            return false;
        }
        let origin = self.enemies[i].center();
        let forward = self.enemies[i].turret_forward();
        let mut targets = self
            .mixed_combat
            .actors
            .iter()
            .filter(|a| a.team == Team::German && a.alive())
            .cloned()
            .collect::<Vec<_>>();
        if let Some(player) = self.mixed_player_actor() {
            targets.push(player);
        }
        targets.retain(|a| {
            let delta = a.eye() - origin;
            delta.length() < 150.0
                && forward.dot(delta.normalize_or_zero()) > 0.342
                && self.los_clear(origin, a.eye())
        });
        let Some(target) = targets
            .iter()
            .min_by(|a, b| {
                a.pos
                    .distance_squared(origin)
                    .total_cmp(&b.pos.distance_squared(origin))
            })
            .cloned()
        else {
            return false;
        };
        let delta = target.eye() - origin;
        let distance = delta.length();
        let horizontal = Vec3::new(delta.x, 0.0, delta.z).normalize_or_zero();
        let gun_right = Vec3::new(forward.z, 0.0, -forward.x);
        let he = enemy_native_he(self.enemies[i].kind).map(|profile| profile.shell);
        let state = &self.mixed_combat.tank_weapons[i];
        let nearby = targets
            .iter()
            .filter(|a| a.pos.distance(target.pos) < 7.0)
            .count() as u32;
        let friendly = self
            .mixed_combat
            .actors
            .iter()
            .any(|a| a.alive() && a.team == Team::Soviet && a.pos.distance(target.pos) < 8.0);
        let mut clearance = [1.0; 8];
        for (k, value) in clearance.iter_mut().enumerate() {
            let direction = Quat::from_rotation_y(k as f32 * std::f32::consts::FRAC_PI_4)
                * self.enemies[i].forward();
            *value = self
                .nearest_cover_ray(origin, direction, 10.0)
                .map_or(1.0, |(t, _)| t / 10.0);
        }
        let obs = soft_ai::Observation {
            target_forward: forward.dot(horizontal),
            target_right: gun_right.dot(horizontal),
            range_m: distance,
            visible: true,
            nearby_hostiles: nearby,
            friendly_blast_risk: friendly,
            mg_heat: state.heat,
            he_reload: self.enemies[i].reload,
            mg_ammo_fraction: state.mg_ammo as f32 / 300.0,
            he_ammo_fraction: if he.is_some() {
                state.he_ammo as f32 / 8.0
            } else {
                0.0
            },
            target_velocity: [target.velocity.dot(forward), target.velocity.dot(gun_right)],
            clearance,
            hull_gun_angle: self.enemies[i].turret_yaw,
            speed: self.enemies[i].speed,
        };
        let action = self
            .mixed_combat
            .tank_policy
            .as_ref()
            .map(|p| p.decide(obs))
            .unwrap_or_else(|| soft_ai::Policy::doctrine().decide(obs));
        let ground = Vec3::new(
            target.pos.x,
            self.hf.height_at(target.pos.x, target.pos.z) + 0.02,
            target.pos.z,
        );
        let avoidance: Vec<_> = std::iter::once(vehicle_motion(usize::MAX, &self.player))
            .chain(
                self.enemies
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(j, e)| vehicle_motion(j, e)),
            )
            .collect();
        let (drive, steer) = apply_vehicle_avoidance(
            vehicle_motion(i, &self.enemies[i]),
            &avoidance,
            action.drive,
            action.steer,
        );
        self.enemies[i].update(drive, steer, 0.0, 0.0, dt, &self.hf);
        if self.act.id == 2 {
            let requested = Vec2::new(self.enemies[i].pos.x, self.enemies[i].pos.z);
            let resolved = constrain_act2_channel_step(Vec2::new(origin.x, origin.z), requested);
            self.enemies[i].pos.x = resolved.x;
            self.enemies[i].pos.z = resolved.y;
            self.enemies[i].pos.y = act2_bridge_deck_height(&self.hf, resolved)
                .unwrap_or_else(|| self.hf.height_at(resolved.x, resolved.y));
        }
        let want_he = action.high_explosive
            && he.is_some()
            && self.enemies[i].reload <= 0.0
            && self.mixed_combat.tank_weapons[i].he_ammo > 0;
        let aim = if want_he {
            ballistic_aim_target(self.enemies[i].muzzle().0, ground, he.unwrap())
        } else {
            target.pos + target.torso_offset() * (0.9 / 1.15)
        };
        self.enemies[i].turret_yaw += action.traverse * 0.80 * dt;
        let gun_local_target = self.enemies[i]
            .turret_model()
            .inverse()
            .transform_point3(aim)
            - self.enemies[i].parts.gun_trunnion;
        self.enemies[i].gun_pitch =
            soft_ai::slew_elevation(self.enemies[i].gun_pitch, gun_local_target, dt);
        let (laid_origin, laid_dir) = self.enemies[i].muzzle();
        let laid = soft_ai::gun_is_laid(laid_origin, laid_dir, aim);
        let state = &mut self.mixed_combat.tank_weapons[i];
        state.target = Some((target.id, target.pos));
        state.aim = Some(aim);
        state.laid = laid;
        state.requested_mg = action.machine_gun;
        state.requested_he = action.high_explosive;
        state.he_available = he.is_some();
        let fire_mg = action.machine_gun && !state.overheated && state.mg_ammo > 0;
        let rounds = combat::cycle_machine_gun(&mut state.timer, &mut state.mg_ammo, fire_mg, dt);
        if fire_mg {
            state.heat = (state.heat + (combat::MG_HEAT_GAIN + combat::MG_HEAT_LOSS) * dt).min(1.0);
            if state.heat >= 1.0 {
                state.overheated = true;
            }
        }
        for _ in 0..rounds {
            let state = &mut self.mixed_combat.tank_weapons[i];
            state.round = state.round.wrapping_add(1);
            let n = state.round;
            let (muzzle, d) = self.enemies[i].coax_muzzle();
            let shot = combat::Shot {
                shooter: 10000 + i as u32,
                team: Team::Soviet,
                origin: muzzle,
                direction: soft_ai::machine_gun_direction(d, n as u64),
                range: MG_RANGE,
                weapon: Weapon::MachineGun,
            };
            // Exclude the firing hull from its own muzzle shadow; other armor still blocks it.
            self.mixed_launch_small_arm(shot);
            self.mixed_combat.mg_shots += 1;
            if n % MG_SOUND_EVERY == 0 {
                self.schedule_sound_from(crate::audio::Sfx::TankMgShot, muzzle, 0.65);
            }
            let k = self.next_inf_fx;
            self.next_inf_fx = (k + 1) % INF_FX;
            self.inf_flash_t[k] = 0.05;
            self.inf_flash_pos[k] = muzzle;
        }
        if want_he && laid {
            let (muzzle, direction) = self.enemies[i].muzzle();
            let shell = he.unwrap();
            self.mixed_combat.tank_weapons[i].he_ammo -= 1;
            self.mixed_combat.he_shots += 1;
            self.enemy_gun_shots[i] = self.enemy_gun_shots[i].wrapping_add(1);
            self.enemies[i].present_fired();
            let id = self.spawn_authoritative_projectile(
                ProjectileShooter::Enemy(i),
                shell,
                muzzle,
                direction,
                self.ai_tick ^ i as u32,
            );
            self.mixed_combat.auxiliary_he_ids.insert(id);
            self.present_external_projectile(id, muzzle, direction);
            self.enemies[i].reload = soft_ai::HE_RELOAD_SECONDS;
            self.schedule_sound_from(crate::audio::Sfx::EnemyGun, muzzle, VOL_ENEMY_GUN);
        }
        true
    }
    pub fn mixed_combat_telemetry(&self) -> serde_json::Value {
        let actors: Vec<_> = self
            .mixed_combat
            .actors
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id":a.id,"team":if a.team==Team::German {"german"} else {"soviet"},
                    "pos":a.pos.to_array(),"yaw":a.yaw,"hp":a.hp,"aiming":a.aiming,
                    "crouched":a.crouched,"last_seen_crouched":a.last_seen_crouched,
                    "crouch_requested":a.crouch_requested,"crouch_phase":a.crouch_phase,
                    "crouch_stage":a.crouch_stage(),"pose_support":format!("{:?}",a.pose_support),
                    "body_height_m":a.body_height(),"eye":a.eye().to_array(),
                    "last_seen_body_height_m":a.last_seen_body_height,
                    "observed_aim_point":a.last_observed_aim_point().map(|p| p.to_array()),
                    "last_seen":a.last_seen.map(|p| p.to_array()),"memory_s":a.memory_s,
                    "magazine":a.magazine,"reserve":a.reserve,"reload_s":a.reload,
                })
            })
            .collect();
        let tanks: Vec<_> = self
            .mixed_combat
            .tank_weapons
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let he = enemy_native_he(self.enemies[i].kind);
                serde_json::json!({"enemy":i,"kind":self.enemies[i].kind.name(),
                "target_id":s.target.map(|t|t.0),"target_pos":s.target.map(|t|t.1.to_array()),
                "aim_point":s.aim.map(|p| p.to_array()),"laid":s.laid,
                "requested_mg":s.requested_mg,"requested_he":s.requested_he,
                "native_he_available":s.he_available,"he_ammo":s.he_ammo,
                "native_he_shell":he.map(|p| p.shell.name),
                "native_he_provenance":he.map(|p| p.provenance),
                "native_he_projectile_primary_verified":he.map(|p| p.projectile_primary_verified),
                "native_he_velocity_primary_verified":he.map(|p| p.muzzle_velocity_primary_verified),
                "native_he_drag_calibrated":he.map(|p| p.drag_calibrated),
                "reload_s":self.enemies[i].reload,"mg_ammo":s.mg_ammo,"mg_heat":s.heat,
                "mg_overheated":s.overheated})
            })
            .collect();
        serde_json::json!({"enabled":self.mixed_combat.enabled,
            "german_alive":self.mixed_combat.actors.iter().filter(|a|a.team==Team::German&&a.alive()).count(),
            "soviet_alive":self.mixed_combat.actors.iter().filter(|a|a.team==Team::Soviet&&a.alive()).count(),
            "rifle_shots":self.mixed_combat.rifle_shots,"damage":self.mixed_combat.damage,
            "friendly_damage":self.mixed_combat.friendly_damage,
            "tank_mg_shots":self.mixed_combat.mg_shots,"tank_mg_damage":self.mixed_combat.mg_damage,
            "tank_he_shots":self.mixed_combat.he_shots,"tank_he_impacts":self.mixed_combat.he_impacts,
            "primary_tank_he_impacts":self.mixed_combat.primary_he_impacts,
            "primary_tank_he_damage":self.mixed_combat.primary_he_damage,
            "tank_he_damage":self.mixed_combat.he_damage,"policy_directory":self.mixed_combat.policy_directory,
            "explicit_override":self.mixed_combat.policy_directory.is_some(),
            "german_learned":self.mixed_combat.german_policy.is_some(),
            "soviet_learned":self.mixed_combat.soviet_policy.is_some(),
            "tank_learned":self.mixed_combat.tank_policy.is_some(),"actors":actors,"tanks":tanks})
    }
    /// Authored start pose for a live anti-infantry evaluation. No action, hit, or damage is forced.
    pub fn debug_stage_soft_target_tank(
        &mut self,
        index: usize,
        kind: EnemyKind,
        pos: Vec3,
        facing_target: Vec3,
        he_ammo: u32,
    ) -> bool {
        if index >= self.enemies.len() || he_ammo > 8 {
            return false;
        }
        let delta = facing_target - pos;
        let mut enemy = Tank::new(
            pos.x,
            pos.z,
            delta.x.atan2(delta.z),
            &self.hf,
            tank::parts_for(kind),
        );
        enemy.kind = kind;
        enemy.reload = 0.5; // first readiness occurs after recording starts
        enemy.ai_aim(facing_target, 1.0);
        self.enemies[index] = enemy;
        self.enemy_active[index] = true;
        self.enemy_concealed[index] = false;
        self.enemy_skill[index] = 42;
        self.wrecks[index] = Wreck::default();
        self.mixed_combat.tank_weapons[index] = TankWeapons {
            timer: 0.5,
            mg_ammo: 300,
            he_ammo,
            ..Default::default()
        };
        self.targets_only = false;
        self.brawl = false;
        self.mixed_combat.enabled = true;
        true
    }
    pub fn debug_stage_mixed_combat(&mut self, center: Vec3) {
        self.mixed_combat.enabled = true;
        for (i, soldier) in self.soldiers.iter_mut().enumerate() {
            soldier.activate_escort([soldier.pos; 4], &self.hf);
            let german = i >= N_SOLDIERS;
            let active = german || i < 4;
            if !active {
                soldier.kill();
                continue;
            }
            let index = if german { i - N_SOLDIERS } else { i };
            let p = center
                + Vec3::new(
                    (index as f32 - 1.5) * 3.5,
                    0.0,
                    if german { 9.0 } else { -9.0 },
                );
            soldier.pos = Vec3::new(p.x, self.hf.height_at(p.x, p.z), p.z);
            soldier.yaw = if german { 0.0 } else { std::f32::consts::PI };
            soldier.route = [soldier.pos; 4];
            soldier.escort_enemy = None;
        }
        for actor in &mut self.mixed_combat.actors {
            actor.hp = 0.0;
        }
        self.mixed_sync_actors();
        self.mixed_combat.decision_clock = 0.0;
        self.mixed_combat.actions.fill(FootAction::default());
        self.mixed_combat.rifle_shots = [0; 2];
        self.mixed_combat.damage = [0.0; 2];
        self.mixed_combat.friendly_damage = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn practice_infantry_stays_stationary_and_live_combat_restores_motion_and_fire() {
        let mut live = Combatant::new(1, Team::Soviet, Vec3::ZERO, 0.0);
        live.cooldown = 0.0;
        let target = Combatant::new(2, Team::German, Vec3::new(0.0, 0.0, -12.0), 0.0);
        let obs = combat::observe(&mut live, &[target], &[], Vec3::new(0.0, 0.0, -20.0), 0.01);
        let mut practice = live.clone();
        let action = FootAction { forward: 0.3, aim: true, fire: true, ..Default::default() };
        let original_ammo = practice.magazine;
        let held = combat::apply_action(&mut practice, &obs,
            permitted_infantry_action(action, false), &[], 0.1, 1);
        let fired = combat::apply_action(&mut live, &obs,
            permitted_infantry_action(action, true), &[], 0.1, 1);
        assert!(held.is_none());
        assert_eq!(practice.magazine, original_ammo);
        assert!(fired.is_some());
        assert_eq!(live.magazine + 1, original_ammo);
        assert_eq!(practice.pos, Vec3::ZERO);
        assert!(live.pos.distance(Vec3::ZERO) > 0.0);
        assert_eq!(practice.hp, live.hp);
    }

    #[test]
    fn sensor_prefilter_skips_irrelevant_los_without_changing_observations_or_actions() {
        let mut baseline = Combatant::new(1, Team::Soviet, Vec3::ZERO, 0.0);
        let mut filtered = baseline.clone();
        let mut snapshot = vec![
            baseline.clone(),
            Combatant::new(2, Team::German, Vec3::new(0.0, 0.0, -12.0), 0.0),
            Combatant::new(3, Team::German, Vec3::new(10.0, 0.0, -18.0), 0.0),
            Combatant::new(4, Team::German, Vec3::new(0.0, 0.0, -70.0), 0.0),
            Combatant::new(5, Team::German, Vec3::new(0.0, 0.0, 10.0), 0.0),
            Combatant::new(6, Team::German, Vec3::new(0.0, 0.0, -6.0), 0.0),
            Combatant::new(7, Team::Soviet, Vec3::new(3.0, 0.0, 1.0), 0.0),
        ];
        snapshot[5].hp = 0.0;
        let pose_observation = combat::observe(&mut snapshot[1], &[], &[], Vec3::ZERO, 0.0);
        combat::apply_action(&mut snapshot[1], &pose_observation,
            crate::foot_ai::FootAction { crouch: true, ..Default::default() }, &[], 0.2, 0);
        let policy = crate::foot_ai::FootPolicy::seeded(PolicyRole::SovietRifle, 73, 0.1);
        let goal = Vec3::new(8.0, 0.0, -30.0);
        // Observe one target, lose sight, then let its remembered pose/location expire. The other
        // eligible target stays behind world cover; allies retain their original admission rules.
        for (dt, block_all) in [(0.1, false), (1.0, true), (4.1, true)] {
            let visible = |to: Vec3| !block_all && to.x < 1.0;
            let mut baseline_calls = 0;
            let old_seen: Vec<_> = snapshot.iter().filter(|other| {
                other.team == baseline.team || {
                    baseline_calls += 1;
                    visible(other.eye())
                }
            }).cloned().collect();
            let mut filtered_calls = 0;
            let new_seen = sensed_actors(&filtered, &snapshot, |_, to| {
                filtered_calls += 1;
                visible(to)
            });
            assert_eq!(baseline_calls, 5);
            assert_eq!(filtered_calls, 2);
            let old_obs = combat::observe(&mut baseline, &old_seen, &[], goal, dt);
            let new_obs = combat::observe(&mut filtered, &new_seen, &[], goal, dt);
            assert_eq!(old_obs, new_obs);
            assert_eq!(policy.decide(&old_obs).unwrap(), policy.decide(&new_obs).unwrap());
            assert_eq!(crate::foot_ai::micro_expert(&old_obs), crate::foot_ai::micro_expert(&new_obs));
            assert_eq!(baseline.last_observed_aim_point(), filtered.last_observed_aim_point());
            assert_eq!(baseline.memory_s, filtered.memory_s);
        }
        assert_eq!(filtered.last_seen, None);
    }
    #[test]
    fn auxiliary_he_attribution_requires_the_registered_projectile_id() {
        let mut state = MixedCombatRuntime::new(&[]);
        state.auxiliary_he_ids.insert(42);
        assert!(!state.record_he_contact(41, ProjectileShooter::Enemy(0)));
        assert_eq!((state.he_impacts, state.primary_he_impacts), (0, 1));
        assert!(state.auxiliary_he_ids.contains(&42));
        assert!(state.record_he_contact(42, ProjectileShooter::Enemy(0)));
        assert_eq!((state.he_impacts, state.primary_he_impacts), (1, 1));
        assert!(!state.auxiliary_he_ids.contains(&42));
        assert!(!state.record_he_contact(43, ProjectileShooter::Player));
        assert_eq!((state.he_impacts, state.primary_he_impacts), (1, 1));
    }
    #[test]
    fn native_he_enemy_mapping_uses_its_gun_and_exposes_research_gaps() {
        for (kind, expected_caliber) in [
            (EnemyKind::T34, Some(76.2)),
            (EnemyKind::Kv2, Some(152.4)),
            (EnemyKind::Su100, None),
            (EnemyKind::Isu152, None),
            (EnemyKind::Is2, None),
            (EnemyKind::Is3, None),
        ] {
            assert_eq!(
                enemy_native_he(kind).map(|p| p.shell.caliber_mm),
                expected_caliber
            );
            if let Some(profile) = enemy_native_he(kind) {
                assert!(!std::ptr::eq(profile.shell, &crate::ballistics::SPRGR_88));
            }
        }
    }
    #[test]
    fn german_cc0_body_has_grounded_human_scale_and_its_own_texture() {
        let mesh = german_infantry_mesh();
        assert!((mesh.min.y).abs() < 0.0001);
        assert!((mesh.max.y - 1.75).abs() < 0.0001);
        assert_eq!(
            mesh.indices.len() / 3,
            5073,
            "must load the reviewed German body, not the fallback"
        );
        assert!(mesh.texture.is_some());
        assert!(mesh.max.x - mesh.min.x > 0.2 && mesh.max.x - mesh.min.x < 1.5);
    }
    #[test]
    fn appended_german_roster_has_distinct_role_and_finite_ammunition() {
        let soldiers: Vec<_> = (0..N_SOLDIERS + GERMAN_COUNT)
            .map(|i| {
                let pos = Vec3::new(i as f32 * 2.0, 0.0, 0.0);
                Soldier::new(pos, 0.0, [pos; 4], i as u32, i * 2, i * 2 + 1)
            })
            .collect();
        let state = MixedCombatRuntime::new(&soldiers);
        assert_eq!(
            state
                .actors
                .iter()
                .filter(|a| a.team == Team::Soviet)
                .count(),
            24
        );
        assert_eq!(
            state
                .actors
                .iter()
                .filter(|a| a.team == Team::German)
                .count(),
            4
        );
        assert!(state.actors[N_SOLDIERS..]
            .iter()
            .all(|a| a.team.role() == PolicyRole::GermanRifle));
        assert!(state
            .actors
            .iter()
            .all(|a| a.magazine == 5 && a.reserve == 30));
        assert!(state
            .tank_weapons
            .iter()
            .all(|a| a.mg_ammo == 300 && a.he_ammo == 8));
    }
}
