//! Deterministic adversarial combat evolution. This executes shared world/body/weapon rules;
//! fitness comes from casualties, damage, survival and ammunition, never expert action agreement.
//! CPU workers own independent renderer-free episodes. Validation selects candidates; a distinct
//! sealed test seed set is evaluated only at run end. Candidate files never replace shipped AI.
use crate::{
    anti_infantry_ai as tank_ai,
    foot_ai::{self, FootAction, FootPolicy, PolicyRole},
    infantry_combat as combat,
};
use combat::{Combatant, Cover, Shot, Team, Weapon};
use glam::{Mat4, Vec3};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
#[path = "combat_evaluation.rs"]
mod checkpoint_evaluation;
pub use checkpoint_evaluation::{evaluate_checkpoints, EvaluationConfig, EvaluationSeedSet};
#[path = "combat_bootstrap.rs"]
mod bootstrap;
pub use bootstrap::{bootstrap_combat, BootstrapConfig};
#[derive(Clone)]
pub struct Policies {
    pub german: Option<FootPolicy>,
    pub soviet: Option<FootPolicy>,
    pub tank: Option<tank_ai::Policy>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track {
    German,
    Soviet,
    Tank,
}
impl Track {
    pub fn label(self) -> &'static str {
        match self {
            Self::German => "german",
            Self::Soviet => "soviet",
            Self::Tank => "tank",
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub damage: [f32; 2],
    pub friendly_damage: [f32; 2],
    pub kills: [u32; 2],
    pub rifle_shots: [u32; 2],
    pub survivors: [u32; 2],
    pub winner: Option<Team>,
    pub seconds: f32,
    pub mg_shots: u32,
    pub he_shots: u32,
    pub tank_damage: f32,
    pub tank_friendly_damage: f32,
    pub tank_kills: u32,
    pub mg_damage: f32,
    pub he_damage: f32,
    pub he_detonations: u32,
}
impl Metrics {
    pub fn score(self, track: Track) -> f32 {
        let i = if track == Track::Soviet { 1 } else { 0 };
        let j = 1 - i;
        if track == Track::Tank {
            return self.tank_damage + 80.0 * self.tank_kills as f32
                - 2.5 * self.tank_friendly_damage
                - 0.08 * self.mg_shots as f32
                - 6.0 * self.he_shots as f32;
        }
        let win = match self.winner {
            Some(Team::German) if i == 0 => 1.0,
            Some(Team::Soviet) if i == 1 => 1.0,
            Some(_) => -1.0,
            None => 0.0,
        };
        self.damage[i] * 0.8 + self.kills[i] as f32 * 80.0
            - self.damage[j] * 0.45
            - self.friendly_damage[i] * 2.5
            + self.survivors[i] as f32 * 15.0
            + win * 200.0
            - self.rifle_shots[i] as f32 * 0.20
    }
    pub fn json(self) -> serde_json::Value {
        serde_json::json!({"damage":self.damage,"friendly_damage":self.friendly_damage,"kills":self.kills,
        "rifle_shots":self.rifle_shots,"survivors":self.survivors,"winner":self.winner.map(|t|if t==Team::German{"german"}else{"soviet"}),
        "seconds":self.seconds,"mg_shots":self.mg_shots,"he_shots":self.he_shots,"tank_damage":self.tank_damage,"tank_friendly_damage":self.tank_friendly_damage,"tank_kills":self.tank_kills,"mg_damage":self.mg_damage,"he_damage":self.he_damage,"he_detonations":self.he_detonations})
    }
}
fn team_index(t: Team) -> usize {
    if t == Team::German {
        0
    } else {
        1
    }
}
#[derive(Clone, Debug)]
struct TrainingTank {
    pos: Vec3,
    yaw: f32,
    gun_yaw: f32, // relative to hull, as in runtime
    gun_pitch: f32,
    gun: crate::ballistics::SovietGun,
    speed: f32,
    heat: f32,
    overheated: bool,
    mg_timer: f32,
    he_reload: f32,
    mg_ammo: u32,
    he_ammo: u32,
}
#[derive(Clone, Debug)]
struct Projectile {
    pos: Vec3,
    direction: Vec3,
    shell: &'static crate::ballistics::ShellSpec,
    distance_m: f32,
    age: f32,
}
impl TrainingTank {
    fn rig(&self) -> tank_ai::GunRig {
        match self.gun {
            crate::ballistics::SovietGun::F34 => tank_ai::F34_RIG,
            crate::ballistics::SovietGun::M10T => tank_ai::M10T_RIG,
            _ => unreachable!("training selects only the declared native profiles"),
        }
    }
    fn shell(&self) -> &'static crate::ballistics::ShellSpec {
        crate::ballistics::native_soviet_he(self.gun).unwrap().shell
    }
    fn turret(&self) -> Mat4 {
        self.rig().turret(self.pos, self.yaw, self.gun_yaw)
    }
    fn gun_model(&self) -> Mat4 {
        self.rig()
            .gun(self.pos, self.yaw, self.gun_yaw, self.gun_pitch)
    }
    fn muzzle(&self) -> (Vec3, Vec3) {
        let g = self.gun_model();
        (
            g.transform_point3(Vec3::Z * self.rig().barrel_length),
            g.transform_vector3(Vec3::Z).normalize(),
        )
    }
    fn center(&self) -> Vec3 {
        self.pos
            + Vec3::Y
                * match self.gun {
                    crate::ballistics::SovietGun::F34 => 1.4,
                    _ => 1.65,
                }
    }
}
#[derive(Clone, Debug)]
pub struct Episode {
    pub actors: Vec<Combatant>,
    pub cover: Vec<Cover>,
    pub metrics: Metrics,
    seed: u64,
    tick: u64,
    tank: Option<TrainingTank>,
    shells: Vec<Projectile>,
    actions: Vec<FootAction>,
    objectives: [Vec3; 2],
}
impl Episode {
    pub fn new(seed: u64, with_tank: bool) -> Self {
        let mut actors = Vec::new();
        let angle = (tank_ai::unit(seed ^ 0x52) - 0.5) * 0.7;
        let rotation = glam::Quat::from_rotation_y(angle);
        let separation = 22.0 + tank_ai::unit(seed ^ 0x41) * 14.0;
        for team in [Team::German, Team::Soviet] {
            for i in 0..4 {
                let side = if team == Team::German { -1.0 } else { 1.0 };
                let id = team_index(team) as u32 * 4 + i;
                let spacing = if with_tank && team == Team::German {
                    0.9 + tank_ai::unit(seed ^ 0x911) * 4.1
                } else {
                    5.0
                };
                let x = (i as f32 - 1.5) * spacing + (tank_ai::unit(seed ^ id as u64) - 0.5) * 3.0;
                let pos = rotation * Vec3::new(x, 0.0, side * separation * 0.5);
                let yaw = angle
                    + if team == Team::German {
                        std::f32::consts::PI
                    } else {
                        0.0
                    };
                actors.push(Combatant::new(id, team, pos, yaw));
            }
        }
        let mut cover = Vec::new();
        // Different lateral gaps and offsets each episode; no actor spawns within the centre cover.
        for i in 0..(seed % 4) {
            let x = (i as f32 - 1.0) * 8.0 + (tank_ai::unit(seed ^ i ^ 0x841) - 0.5) * 3.0;
            let z = (tank_ai::unit(seed ^ i ^ 0x913) - 0.5) * 8.0;
            let half = Vec3::new(1.1, 0.65 + tank_ai::unit(seed ^ i ^ 0x71) * 0.7, 0.55);
            cover.push(Cover {
                min: Vec3::new(x, 0.0, z) - Vec3::new(half.x, 0.0, half.z),
                max: Vec3::new(x, half.y * 2.0, z) + Vec3::new(half.x, 0.0, half.z),
            });
        }
        Self {
            actors,
            cover,
            metrics: Metrics::default(),
            seed,
            tick: 0,
            actions: vec![FootAction::default(); 8],
            shells: Vec::new(),
            objectives: [
                rotation * Vec3::new(0.0, 0.0, 8.0),
                rotation * Vec3::new(0.0, 0.0, -8.0),
            ],
            tank: with_tank.then(|| TrainingTank {
                pos: rotation * Vec3::new(-13.0, 0.0, 27.0),
                yaw: angle + std::f32::consts::PI,
                gun_yaw: 0.0,
                gun_pitch: 0.0,
                gun: if seed & 1 == 0 {
                    crate::ballistics::SovietGun::F34
                } else {
                    crate::ballistics::SovietGun::M10T
                },
                speed: 0.0,
                heat: 0.0,
                overheated: false,
                mg_timer: 0.0,
                he_reload: 0.0,
                mg_ammo: 300,
                he_ammo: 8,
            }),
        }
    }
    pub fn finished(&self) -> bool {
        self.metrics.winner.is_some()
    }
    fn record_hit(&mut self, team: Team, hit: combat::Hit, tank: bool, he: bool) {
        let i = team_index(team);
        if hit.friendly {
            self.metrics.friendly_damage[i] += hit.damage;
            if tank {
                self.metrics.tank_friendly_damage += hit.damage;
            }
        } else {
            self.metrics.damage[i] += hit.damage;
            self.metrics.kills[i] += u32::from(hit.killed);
            if tank {
                self.metrics.tank_damage += hit.damage;
                if he {
                    self.metrics.he_damage += hit.damage;
                } else {
                    self.metrics.mg_damage += hit.damage;
                }
                self.metrics.tank_kills += u32::from(hit.killed);
            }
        }
    }
    pub fn step(&mut self, policies: &Policies, dt: f32) {
        self.step_observed(policies, dt, |_| {});
    }

    fn step_observed(&mut self, policies: &Policies, dt: f32,
        mut record: impl FnMut(foot_ai::FootObservation)) {
        let snapshot = self.actors.clone();
        let mut shots = Vec::new();
        for i in 0..self.actors.len() {
            if !self.actors[i].alive() {
                continue;
            }
            let team = self.actors[i].team;
            let obs = combat::observe(
                &mut self.actors[i],
                &snapshot,
                &self.cover,
                self.objectives[team_index(team)],
                dt,
            );
            if self.tick % 4 == 0 {
                record(obs);
                let policy = if team == Team::German {
                    policies.german.as_ref()
                } else {
                    policies.soviet.as_ref()
                };
                self.actions[i] = policy
                    .and_then(|p| p.decide(&obs).ok())
                    .unwrap_or_else(|| foot_ai::micro_expert(&obs));
            }
            if let Some(shot) = combat::apply_action(
                &mut self.actors[i],
                &obs,
                self.actions[i],
                &self.cover,
                dt,
                self.seed ^ self.tick,
            ) {
                shots.push(shot);
            }
        }
        // All actors decide from the same starting snapshot; damage is resolved after decisions,
        // so a low actor index cannot prevent an already-fired opposing bullet from existing.
        for shot in shots {
            self.metrics.rifle_shots[team_index(shot.team)] += 1;
            let hit = combat::resolve_hitscan(shot, &mut self.actors, &self.cover);
            self.record_hit(shot.team, hit, false, false);
        }
        self.step_tank(policies.tank.as_ref(), dt);
        self.metrics.seconds += dt;
        self.tick += 1;
        self.metrics.survivors = [0; 2];
        for a in &self.actors {
            if a.alive() {
                self.metrics.survivors[team_index(a.team)] += 1;
            }
        }
        self.metrics.winner = match self.metrics.survivors {
            [0, 0] => None,
            [0, _] => Some(Team::Soviet),
            [_, 0] => Some(Team::German),
            _ => None,
        };
    }
    fn step_tank(&mut self, policy: Option<&tank_ai::Policy>, dt: f32) {
        let Some(mut tank) = self.tank.take() else {
            return;
        };
        let sensing_origin = tank.center();
        let gun_yaw = tank.yaw + tank.gun_yaw;
        let target = self
            .actors
            .iter()
            .filter(|a| {
                a.alive()
                    && a.team == Team::German
                    && a.pos.distance(tank.pos) < 150.0
                    && tank_ai::relative_target(sensing_origin, gun_yaw, a.eye()).0 > 0.342
                    && combat::clear_segment(sensing_origin, a.eye(), &self.cover)
            })
            .min_by(|a, b| {
                a.pos
                    .distance_squared(tank.pos)
                    .total_cmp(&b.pos.distance_squared(tank.pos))
            })
            .cloned();
        tank.he_reload = (tank.he_reload - dt).max(0.0);
        let mut action = tank_ai::Action::default();
        let mut aim_point = None;
        if let Some(target) = target {
            let (forward, right, range) =
                tank_ai::relative_target(sensing_origin, gun_yaw, target.eye());
            let nearby = self
                .actors
                .iter()
                .filter(|a| {
                    a.alive()
                        && a.team == Team::German
                        && a.pos.distance(target.pos) < 7.0
                        && combat::clear_segment(sensing_origin, a.eye(), &self.cover)
                })
                .count() as u32;
            let friendly = self
                .actors
                .iter()
                .any(|a| a.alive() && a.team == Team::Soviet && a.pos.distance(target.pos) < 8.0);
            let mut clearance = [1.0; 8];
            for (i, c) in clearance.iter_mut().enumerate() {
                let yaw = tank.yaw + i as f32 * std::f32::consts::FRAC_PI_4;
                *c = combat::first_cover(
                    tank.pos + Vec3::Y,
                    Vec3::new(yaw.sin(), 0.0, yaw.cos()),
                    10.0,
                    &self.cover,
                )
                .unwrap_or(10.0)
                    / 10.0;
            }
            let obs = tank_ai::Observation {
                target_forward: forward,
                target_right: right,
                range_m: range,
                visible: true,
                nearby_hostiles: nearby,
                friendly_blast_risk: friendly,
                mg_heat: tank.heat,
                he_reload: tank.he_reload,
                mg_ammo_fraction: tank.mg_ammo as f32 / 300.0,
                he_ammo_fraction: tank.he_ammo as f32 / 8.0,
                target_velocity: [
                    target
                        .velocity
                        .dot(Vec3::new(gun_yaw.sin(), 0.0, gun_yaw.cos())),
                    target
                        .velocity
                        .dot(Vec3::new(gun_yaw.cos(), 0.0, -gun_yaw.sin())),
                ],
                clearance,
                hull_gun_angle: tank.gun_yaw,
                speed: tank.speed,
            };
            action = policy
                .map(|p| p.decide(obs))
                .unwrap_or_else(|| tank_ai::Policy::doctrine().decide(obs));
            aim_point = Some(target.pos + target.torso_offset() * (0.9 / 1.15));
        }
        tank.yaw += action.steer * 0.7 * dt;
        tank.gun_yaw += action.traverse * 0.80 * dt;
        tank.speed += (action.drive * 12.0 - tank.speed).clamp(-6.0 * dt, 6.0 * dt);
        let next = tank.pos + Vec3::new(tank.yaw.sin(), 0.0, tank.yaw.cos()) * tank.speed * dt;
        if self.cover.iter().all(|c| {
            !(next.x > c.min.x - 1.5
                && next.x < c.max.x + 1.5
                && next.z > c.min.z - 2.5
                && next.z < c.max.z + 2.5)
        }) {
            tank.pos = next;
        }
        let want_he = action.high_explosive && tank.he_reload <= 0.0 && tank.he_ammo > 0;
        let aim = aim_point.map(|body| {
            if want_he {
                let ground = Vec3::new(body.x, 0.02, body.z);
                let muzzle = tank.muzzle().0;
                let horizontal = (ground.x - muzzle.x).hypot(ground.z - muzzle.z);
                ground + Vec3::Y * tank.shell().drop_at(horizontal)
            } else {
                body
            }
        });
        if let Some(aim) = aim {
            let local = tank.turret().inverse().transform_point3(aim) - tank.rig().trunnion;
            tank.gun_pitch = tank_ai::slew_elevation(tank.gun_pitch, local, dt);
        }
        let firing =
            action.machine_gun && !tank.overheated && tank.mg_ammo > 0 && aim_point.is_some();
        let rounds = combat::cycle_machine_gun(&mut tank.mg_timer, &mut tank.mg_ammo, firing, dt);
        if firing {
            tank.heat = (tank.heat + combat::MG_HEAT_GAIN * dt).min(1.0);
            if tank.heat >= 1.0 {
                tank.overheated = true;
            }
            for _ in 0..rounds {
                self.metrics.mg_shots += 1;
                let g = tank.gun_model();
                let muzzle =
                    g.transform_point3(Vec3::new(-0.22, -0.16, tank.rig().barrel_length * 0.72));
                let direction = tank_ai::machine_gun_direction(
                    g.transform_vector3(Vec3::Z).normalize(),
                    self.seed ^ self.tick,
                );
                let hit = combat::resolve_hitscan(
                    Shot {
                        shooter: 1000,
                        team: Team::Soviet,
                        origin: muzzle,
                        direction,
                        range: combat::MG_RANGE,
                        weapon: Weapon::MachineGun,
                    },
                    &mut self.actors,
                    &self.cover,
                );
                self.record_hit(Team::Soviet, hit, true, false);
            }
        } else {
            tank.heat = (tank.heat - combat::MG_HEAT_LOSS * dt).max(0.0);
            if tank.heat <= combat::MG_RESUME {
                tank.overheated = false;
            }
        }
        if want_he {
            if let Some(aim) = aim {
                let (muzzle, direction) = tank.muzzle();
                if tank_ai::gun_is_laid(muzzle, direction, aim) {
                    self.shells.push(Projectile {
                        pos: muzzle,
                        direction,
                        shell: tank.shell(),
                        distance_m: 0.0,
                        age: 0.0,
                    });
                    tank.he_reload = tank_ai::HE_RELOAD_SECONDS;
                    tank.he_ammo -= 1;
                    self.metrics.he_shots += 1;
                }
            }
        }
        self.tank = Some(tank);
        let mut blasts = Vec::new();
        let mut retained = Vec::new();
        for mut shell in self.shells.drain(..) {
            let old = shell.pos;
            // The authoritative runtime law: analytic exponential forward decay plus gravity.
            let step = shell.shell.flight_step(shell.distance_m, shell.age, dt);
            let delta = shell.direction * step.forward_m + Vec3::Y * step.gravity_m;
            let len = delta.length();
            let direction = delta.normalize_or_zero();
            let hit = combat::first_cover(old, direction, len, &self.cover);
            let ground = if delta.y < 0.0 && old.y + delta.y <= 0.0 {
                Some(-old.y / direction.y)
            } else {
                None
            };
            let body = self
                .actors
                .iter()
                .filter(|a| a.alive())
                .filter_map(|actor| {
                    let half = Vec3::new(0.25, 0.0, 0.25);
                    combat::ray_box(
                        old,
                        direction,
                        Cover {
                            min: actor.pos - half,
                            max: actor.pos + half + Vec3::Y * actor.body_height(),
                        },
                        len,
                    )
                })
                .min_by(f32::total_cmp);
            let contact = hit
                .into_iter()
                .chain(ground)
                .chain(body)
                .min_by(f32::total_cmp);
            if let Some(t) = contact {
                blasts.push((
                    old + direction * t - direction * 0.003 + Vec3::Y * 0.003,
                    shell.shell.filler_g / 1000.0,
                ));
            } else {
                shell.pos += delta;
                shell.distance_m += step.forward_m;
                shell.age += dt;
                if shell.age < 8.0 {
                    retained.push(shell);
                }
            }
        }
        self.shells = retained;
        for (point, filler_kg) in blasts {
            self.metrics.he_detonations += 1;
            let hits = combat::blast(
                point,
                filler_kg,
                Team::Soviet,
                &mut self.actors,
                &self.cover,
            );
            for hit in hits {
                self.record_hit(Team::Soviet, hit, true, true);
            }
        }
    }
}
pub fn run_episode(seed: u64, seconds: f32, policies: &Policies, with_tank: bool) -> Metrics {
    let mut ep = Episode::new(seed, with_tank);
    let steps = (seconds / 0.05).ceil() as usize;
    for _ in 0..steps {
        ep.step(policies, 0.05);
        if ep.finished() {
            break;
        }
    }
    ep.metrics
}

#[cfg(test)]
mod gun_parity_tests {
    use super::*;
    #[test]
    fn native_tank_profiles_slew_before_firing_real_he_and_damage_german_targets() {
        for seed in [0, 1] {
            let mut ep = Episode::new(seed, true);
            ep.cover.clear();
            ep.actors = vec![Combatant::new(1, Team::German, Vec3::Z * 50.0, 0.0)];
            let tank = ep.tank.as_mut().unwrap();
            tank.pos = Vec3::ZERO;
            tank.yaw = 0.0;
            tank.gun_yaw = 0.0;
            tank.gun_pitch = 0.20;
            let expected_caliber = tank.shell().caliber_mm;
            assert_eq!(expected_caliber, if seed == 0 { 76.2 } else { 152.4 });
            let mut weights = vec![0.0; tank_ai::WEIGHTS];
            weights[tank_ai::WEIGHTS - tank_ai::ACT + 4] = 5.0;
            let policy = tank_ai::Policy::new(weights).unwrap();
            ep.step_tank(Some(&policy), 0.05);
            assert_eq!(ep.metrics.he_shots, 0, "elevation must not snap to target");
            assert!((ep.tank.as_ref().unwrap().gun_pitch - 0.18).abs() < 1e-6);
            for _ in 0..160 {
                ep.step_tank(Some(&policy), 0.05);
                assert!(ep
                    .shells
                    .iter()
                    .all(|s| s.shell.caliber_mm == expected_caliber));
            }
            assert!(ep.metrics.he_shots > 0 && ep.metrics.he_detonations > 0);
            assert!(ep.metrics.he_damage > 0.0);
            assert_eq!(ep.metrics.mg_shots, 0);
            assert_eq!(ep.metrics.damage[0], 0.0);
            assert!(
                ep.metrics.damage[1] > 0.0,
                "Soviet tank damage is attributed to Soviet"
            );
        }
    }
}
#[derive(Clone, Debug)]
pub struct Config {
    pub output: PathBuf,
    pub initial: Option<PathBuf>,
    pub generations: u32,
    pub population: usize,
    pub episodes: usize,
    pub seconds: f32,
    pub workers: usize,
    pub minutes: f32,
    pub seed: u64,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.generations > 0 && self.population >= 4 && self.episodes >= 2,
            "need generations>0, population>=4, episodes>=2"
        );
        anyhow::ensure!(
            self.workers >= 1 && self.workers <= 12,
            "workers must be 1..12"
        );
        anyhow::ensure!(
            self.seconds.is_finite() && (5.0..=180.0).contains(&self.seconds),
            "episode seconds must be 5..180"
        );
        anyhow::ensure!(
            self.minutes.is_finite() && self.minutes > 0.0,
            "positive wall time required"
        );
        anyhow::ensure!(
            !self.output.as_os_str().is_empty() && !self.output.starts_with("assets"),
            "candidate output must be separate from assets"
        );
        Ok(())
    }
}
fn seed_policy(role: PolicyRole) -> FootPolicy {
    if let Ok((p, _)) = FootPolicy::load(role.checkpoint_path(), role) {
        return p;
    }
    if role == PolicyRole::GermanRifle {
        if let Ok((p, _)) = FootPolicy::load(
            PolicyRole::SovietRifle.checkpoint_path(),
            PolicyRole::SovietRifle,
        ) {
            let mut w = p.weights().to_vec();
            for j in 0..foot_ai::HIDDEN_DIM {
                w.swap(j * foot_ai::OBS_DIM, j * foot_ai::OBS_DIM + 1);
            }
            return FootPolicy::new(role, w).unwrap();
        }
    }
    // This is only a declared initialization. Genuine combat evolution follows and is evaluated.
    foot_ai::train_micro_curriculum(
        role,
        foot_ai::MicroTrainerConfig {
            epochs: 80,
            ..Default::default()
        },
    )
    .unwrap()
    .policy
}
fn mutate(base: &Policies, track: Track, seed: u64, sigma: f32) -> Policies {
    let mut p = base.clone();
    match track {
        Track::German => p.german = Some(p.german.as_ref().unwrap().mutated(seed, sigma)),
        Track::Soviet => p.soviet = Some(p.soviet.as_ref().unwrap().mutated(seed, sigma)),
        Track::Tank => p.tank = Some(p.tank.as_ref().unwrap().mutated(seed, sigma)),
    }
    p
}
fn evaluate(
    p: &Policies,
    track: Track,
    seeds: &[u64],
    seconds: f32,
    opponents: &[Policies],
) -> (f32, Vec<Metrics>) {
    let mut metrics = Vec::new();
    for (i, &seed) in seeds.iter().enumerate() {
        let opponent = &opponents[i % opponents.len()];
        let mut paired = p.clone();
        match track {
            Track::German => paired.soviet = opponent.soviet.clone(),
            Track::Soviet => paired.german = opponent.german.clone(),
            Track::Tank => {
                paired.soviet = opponent.soviet.clone();
                paired.german = opponent.german.clone();
            }
        }
        metrics.push(run_episode(seed, seconds, &paired, track == Track::Tank));
    }
    let score = metrics.iter().map(|m| m.score(track)).sum::<f32>() / metrics.len() as f32;
    (score, metrics)
}
fn save_candidate(
    dir: &Path,
    p: &Policies,
    generation: u32,
    scores: [f32; 3],
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, policy, score) in [
        ("german_rifle.ksl2", p.german.as_ref().unwrap(), scores[0]),
        ("soviet_rifle.ksl2", p.soviet.as_ref().unwrap(), scores[1]),
    ] {
        let path = dir.join(name);
        let temp = path.with_extension("tmp");
        policy.save(temp.to_str().unwrap(), generation, score)?;
        let (check, _) = FootPolicy::load(temp.to_str().unwrap(), policy.role())?;
        anyhow::ensure!(
            check.weights() == policy.weights(),
            "checkpoint reload mismatch"
        );
        std::fs::rename(temp, path)?;
    }
    p.tank
        .as_ref()
        .unwrap()
        .save(&dir.join("tank_soft_targets.json"), generation, scores[2])?;
    Ok(())
}
fn source_hashes() -> serde_json::Value {
    let entries = [
        ("foot_ai", include_bytes!("foot_ai.rs").as_slice()),
        ("combat", include_bytes!("infantry_combat.rs").as_slice()),
        (
            "tank_policy",
            include_bytes!("anti_infantry_ai.rs").as_slice(),
        ),
        ("trainer", include_bytes!("combat_training.rs").as_slice()),
        ("bootstrap", include_bytes!("combat_bootstrap.rs").as_slice()),
        (
            "evaluation",
            include_bytes!("combat_evaluation.rs").as_slice(),
        ),
        ("ballistics", include_bytes!("ballistics.rs").as_slice()),
    ];
    let mut map = serde_json::Map::new();
    for (name, bytes) in entries {
        let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ *b as u64).wrapping_mul(0x100000001b3)
        });
        map.insert(
            name.into(),
            serde_json::Value::String(format!("fnv1a64:{hash:016x}")),
        );
    }
    map.into()
}
pub fn train(config: Config) -> anyhow::Result<serde_json::Value> {
    use std::io::Write;
    config.validate()?;
    anyhow::ensure!(
        !config.output.exists(),
        "training output already exists; preserve it and choose a new directory"
    );
    let initial_hashes=config.initial.as_deref().map(checkpoint_evaluation::checkpoint_hashes).transpose()?;
    let baseline = if let Some(directory) = &config.initial {
        checkpoint_evaluation::load_checkpoints(directory)?
    } else { Policies {
        german: Some(seed_policy(PolicyRole::GermanRifle)),
        soviet: Some(seed_policy(PolicyRole::SovietRifle)),
        tank: Some(tank_ai::Policy::doctrine()),
    }};
    if let Some(directory)=&config.initial {
        anyhow::ensure!(Some(checkpoint_evaluation::checkpoint_hashes(directory)?)==initial_hashes,
            "initial checkpoints changed during load");
    }
    std::fs::create_dir_all(&config.output)?;
    let initialization_receipt=if let Some(path)=config.initial.as_ref()
        .and_then(|d|d.parent()).map(|p|p.join("bootstrap.json")).filter(|p|p.is_file()) {
        let bytes=std::fs::read(&path)?;
        let hash=bytes.iter().fold(0xcbf29ce484222325u64,|h,b|(h^*b as u64).wrapping_mul(0x100000001b3));
        std::fs::write(config.output.join("initialization-bootstrap.json"),&bytes)?;
        Some(serde_json::json!({"source":path,"copied":"initialization-bootstrap.json",
            "bytes":bytes.len(),"fnv1a64":format!("{hash:016x}")}))
    } else {None};
    let expert = Policies {
        german: None,
        soviet: None,
        tank: None,
    };
    let evaluation_opponents = vec![expert.clone(), baseline.clone()];
    let mut champions = baseline.clone();
    let mut archive = vec![baseline.clone(), expert];
    let mut scores = [0.0; 3];
    let validation: Vec<u64> = (0..config.episodes.max(64))
        .map(|i| tank_ai::hash(config.seed ^ 0x56414c4944415445 ^ i as u64))
        .collect();
    let test_seeds: Vec<u64> = (0..128)
        .map(|i| tank_ai::hash(config.seed ^ 0x5345414c45445453 ^ i))
        .collect();
    for (i, track) in [Track::German, Track::Soviet, Track::Tank]
        .into_iter()
        .enumerate()
    {
        scores[i] = evaluate(
            &baseline,
            track,
            &validation,
            config.seconds,
            &evaluation_opponents,
        )
        .0;
    }
    save_candidate(&config.output.join("initial"), &baseline, 0, scores)?;
    std::fs::write(
        config.output.join("run.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema":"kessel.combat_training_run.v1",
        "source_hashes": source_hashes(),"seed":config.seed,"generations":config.generations,"population":config.population,"episodes":config.episodes,"seconds":config.seconds,"workers":config.workers,
        "minutes":config.minutes,"roles":["german_rifle","soviet_rifle","tank_soft_targets"],"backend":"parallel_cpu_shared_combat",
        "validation_seeds":validation,"sealed_test_seeds":test_seeds,"training_seed_namespace":"0x545241494e494e47","initial_directory":config.initial,"initial_checkpoint_hashes":initial_hashes,"initialization_receipt":initialization_receipt,"initialization":if config.initial.is_some(){"explicit role-checked checkpoint warm start; see initial_directory for separate initialization provenance"}else{"existing KSL2; faction-swapped German rifle warm start; explicit tank doctrine"},
        "limitations":["flat procedural cover arena; runtime terrain regression remains required","exterior HE radius law is a game abstraction","native F34 OF350 candidate and legacy M10T OF530 profiles alternate by seed; provisional velocity/drag remain","physical elevation slew, muzzle and 3D gate shared; hull collision and cover remain simplified","no tank armor opponents in soft-target track"]}),
        )?,
    )?;
    let mut log = std::fs::File::create(config.output.join("training.jsonl"))?;
    let start = Instant::now();
    let mut generations = 0;
    for generation in 0..config.generations {
        if start.elapsed().as_secs_f32() >= config.minutes * 60.0 {
            break;
        }
        let track = [Track::German, Track::Soviet, Track::Tank][generation as usize % 3];
        let index = generation as usize % 3;
        let seeds: Vec<u64> = (0..config.episodes)
            .map(|i| {
                tank_ai::hash(
                    config.seed ^ 0x545241494e494e47 ^ ((generation as u64) << 24) ^ i as u64,
                )
            })
            .collect();
        let mut opponents = vec![champions.clone()];
        opponents.push(archive[(generation as usize / 3) % archive.len()].clone());
        let population: Vec<Policies> = (0..config.population)
            .map(|child| {
                if child == 0 {
                    champions.clone()
                } else {
                    let sigma = [0.008, 0.018, 0.04, 0.08][child % 4];
                    mutate(
                        &champions,
                        track,
                        tank_ai::hash(config.seed ^ ((generation as u64) << 32) ^ child as u64),
                        sigma,
                    )
                }
            })
            .collect();
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for worker in 0..config.workers {
                let population = &population;
                let seeds = &seeds;
                let opponents = &opponents;
                handles.push(scope.spawn(move || {
                    (worker..population.len())
                        .step_by(config.workers)
                        .map(|i| {
                            (
                                i,
                                evaluate(&population[i], track, seeds, config.seconds, opponents).0,
                            )
                        })
                        .collect::<Vec<_>>()
                }));
            }
            handles
                .into_iter()
                .flat_map(|h| h.join().expect("combat worker panicked"))
                .collect::<Vec<_>>()
        });
        let (winner, training_score) = results
            .iter()
            .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .copied()
            .unwrap();
        let candidate = &population[winner];
        let (validation_score, metrics) = evaluate(
            candidate,
            track,
            &validation,
            config.seconds,
            &evaluation_opponents,
        );
        if validation_score > scores[index] {
            match track {
                Track::German => champions.german = candidate.german.clone(),
                Track::Soviet => champions.soviet = candidate.soviet.clone(),
                Track::Tank => champions.tank = candidate.tank.clone(),
            }
            scores[index] = validation_score;
            if scores.iter().all(|x| x.is_finite()) {
                save_candidate(
                    &config.output.join(format!("checkpoint_{generation:06}")),
                    &champions,
                    generation,
                    scores,
                )?;
                save_candidate(
                    &config.output.join("candidate"),
                    &champions,
                    generation,
                    scores,
                )?;
            }
        }
        if generation % 12 == 11 {
            archive.push(champions.clone());
            if archive.len() > 10 {
                archive.remove(2);
            }
        }
        let row = serde_json::json!({"generation":generation,"track":track.label(),"train_score":training_score,"validation_score":validation_score,
            "best_validation":scores.iter().map(|v|if v.is_finite(){Some(*v)}else{None}).collect::<Vec<_>>(),"elapsed_seconds":start.elapsed().as_secs_f32(),"validation_metrics":metrics.iter().map(|m|m.json()).collect::<Vec<_>>()});
        writeln!(log, "{row}")?;
        log.flush()?;
        println!("[combat-train] gen={generation} track={} train={training_score:.2} validation={validation_score:.2} elapsed={:.1}s",track.label(),start.elapsed().as_secs_f32());
        generations = generation + 1;
    }
    save_candidate(
        &config.output.join("candidate"),
        &champions,
        generations,
        scores,
    )?;
    let mut exam = Vec::new();
    for track in [Track::German, Track::Soviet, Track::Tank] {
        let (score, metrics) = evaluate(
            &champions,
            track,
            &test_seeds,
            config.seconds,
            &evaluation_opponents,
        );
        let (baseline_score, baseline_metrics) = evaluate(
            &baseline,
            track,
            &test_seeds,
            config.seconds,
            &evaluation_opponents,
        );
        exam.push(serde_json::json!({"track":track.label(),"candidate_score":score,"baseline_score":baseline_score,"candidate":metrics.iter().map(|m|m.json()).collect::<Vec<_>>(),"baseline":baseline_metrics.iter().map(|m|m.json()).collect::<Vec<_>>() }));
    }
    let report = serde_json::json!({"schema":"kessel.combat_training_result.v1","generations":generations,"elapsed_seconds":start.elapsed().as_secs_f32(),
        "promoted_to_shipping":false,"candidate_directory":config.output.join("candidate"),"sealed_test":exam});
    std::fs::write(
        config.output.join("evaluation.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
