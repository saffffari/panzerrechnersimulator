//! AI battle lab + cinematic recorder.
//!
//! The normal campaign deliberately owns six expensive, fully simulated enemy rigs. This module
//! turns those six slots into a repeatable all-T-34 gauntlet, evolves a separate Tiger driver over
//! the same tiny MLP architecture, and records the representative fight through the real renderer.
//! It does not weaken the physical simulation: policies propose drive, steer and trigger intent;
//! crew movement, reload, travel lock, gun lay, armor and ballistics remain authoritative.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{ensure, Context};
use glam::Vec3;

use crate::ai;
use crate::game::{Game, Station, TankInput};
use crate::renderer::Renderer;

const DT: f32 = 1.0 / 60.0;
const TIGER_CREW_CYCLE: f32 = 6.5;
const TIGER_OPENING_ACQUISITION: f32 = 3.5;
const GAUNTLET_RING_M: f32 = 205.0;
const MAX_T34S: usize = 6;

#[derive(Clone, Copy, Debug)]
struct BattleOutcome {
    seed: u32,
    t34_count: usize,
    tiger_alive: bool,
    t34_survivors: usize,
    tiger_hp: f32,
    seconds: f32,
    hits_on_tiger: u32,
    pens_on_tiger: u32,
}

impl BattleOutcome {
    fn t34s_destroyed(self) -> usize {
        self.t34_count.saturating_sub(self.t34_survivors)
    }

    fn t34_win(self) -> bool {
        !self.tiger_alive
    }
}

struct TigerPilot<'a> {
    brain: &'a ai::Mlp,
    crew_cd: f32,
    prev_reload: f32,
}

impl<'a> TigerPilot<'a> {
    fn new(brain: &'a ai::Mlp) -> Self {
        Self {
            brain,
            crew_cd: TIGER_OPENING_ACQUISITION,
            prev_reload: 0.0,
        }
    }

    /// One policy/crew tick. The MLP never receives direct turret or reload controls: it can decide
    /// where to move and when it wants a shot, while this embodied auto-crew must physically change
    /// stations and service the same breech state machine as the player.
    fn input(&mut self, game: &mut Game, step: usize) -> TankInput {
        if game.player.reload > self.prev_reload + 0.2 {
            self.crew_cd = TIGER_CREW_CYCLE;
        }
        self.prev_reload = game.player.reload;
        self.crew_cd = (self.crew_cd - DT).max(0.0);

        if !game.player.alive {
            return TankInput::default();
        }
        if game.is_fire_active() {
            return TankInput {
                extinguish: true,
                ..Default::default()
            };
        }
        if !game.breech_loaded() {
            game.request_station(Station::Loader);
            game.auto_service_breech(step % 2 == 0, DT);
            return TankInput::default();
        }

        let action = self.brain.forward(&game.tiger_executor_obs());
        game.request_station(Station::Gunner);
        game.aim_at_enemy();
        let trigger = game.station_ready(Station::Gunner)
            && self.crew_cd <= 0.0
            && action[2] > 0.0
            && game.auto_can_fire();
        TankInput {
            drive: action[0],
            steer: action[1],
            // Main-gun input is LMB (`mg_coax`) in embodied play. `fire` is Space/climb now.
            mg_coax: trigger,
            ..Default::default()
        }
    }
}

fn stage(
    game: &mut Game,
    t34_brain: &ai::Mlp,
    t34_count: usize,
    seed: u32,
    ring_m: f32,
) -> Vec<usize> {
    game.god = false;
    game.brawl = false;
    game.targets_only = false;
    let active = game.stage_t34_gauntlet(t34_count, seed, ring_m);
    game.enemy_policy = Some(t34_brain.clone());
    active
}

fn simulate_battle(
    game: &mut Game,
    renderer: &mut Renderer,
    t34_brain: &ai::Mlp,
    tiger_brain: &ai::Mlp,
    t34_count: usize,
    seed: u32,
    max_secs: f32,
) -> BattleOutcome {
    let active = stage(game, t34_brain, t34_count, seed, GAUNTLET_RING_M);
    let mut pilot = TigerPilot::new(tiger_brain);
    let steps = (max_secs / DT).ceil() as usize;
    let mut elapsed = 0.0;

    for step in 0..steps {
        game.debug_engage_all();
        let input = pilot.input(game, step);
        game.update(&input, DT, renderer);
        elapsed = (step + 1) as f32 * DT;
        if !game.player.alive || active.iter().all(|&i| !game.enemies[i].alive) {
            break;
        }
    }

    let outcome = BattleOutcome {
        seed,
        t34_count,
        tiger_alive: game.player.alive,
        t34_survivors: active.iter().filter(|&&i| game.enemies[i].alive).count(),
        tiger_hp: game.player.hp.max(0.0),
        seconds: elapsed,
        hits_on_tiger: game.stat_hits_on_player,
        pens_on_tiger: game.stat_pens_on_player,
    };
    game.enemy_policy = None;
    outcome
}

fn fitness(outcome: BattleOutcome, max_secs: f32) -> f32 {
    let kills = outcome.t34s_destroyed() as f32;
    let sweep = outcome.t34_survivors == 0;
    let speed = if sweep {
        1.0 - (outcome.seconds / max_secs).clamp(0.0, 1.0)
    } else {
        0.0
    };
    kills * 115.0
        + if outcome.tiger_alive { 100.0 } else { -80.0 }
        + outcome.tiger_hp * 0.7
        + if sweep { 170.0 } else { 0.0 }
        + speed * 90.0
        - outcome.pens_on_tiger as f32 * 2.0
}

fn training_seed(generation: u32, i: usize) -> u32 {
    let mut x = 0x71CE_1201u32
        ^ generation
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(i as u32 * 7_919);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^ (x >> 13)
}

/// Evolve the AI-controlled Tiger against the currently evolved T-34 policy.
pub fn run_train_tiger(
    generations: u32,
    population: usize,
    match_secs: f32,
    minutes: f32,
    t34_weights: &str,
    tag: &str,
    sigma: f32,
) -> anyhow::Result<()> {
    ensure!(generations > 0, "train-tiger needs at least one generation");
    ensure!(population >= 4, "train-tiger population must be at least 4");
    ensure!(
        match_secs >= 10.0,
        "train-tiger matches must be at least 10 seconds"
    );
    ensure!(sigma > 0.0, "train-tiger mutation sigma must be positive");

    let t34_brain = ai::Mlp::load(t34_weights)
        .with_context(|| format!("loading evolved T-34 policy {t34_weights}"))?;
    let instance = super::create_instance();
    let adapter = super::request_adapter(&instance, None);
    println!(
        "[train-tiger] adapter: {} (simulation only)",
        adapter.get_info().name
    );
    let (device, queue) = super::request_device(&adapter);
    let (mut renderer, mut game) = Game::build_act(&device, &queue, false, &crate::world::act(0))?;
    game.mute_audio();

    let artifact = format!("assets/ai/weights_{tag}.bin");
    let checkpoint_dir = format!("assets/ai/ckpt_{tag}");
    std::fs::create_dir_all(&checkpoint_dir)?;

    let doctrine = ai::Mlp::load(&artifact).unwrap_or_else(|_| ai::Mlp::tiger_doctrine_seed());
    println!(
        "[train-tiger] T-34 policy: {t34_weights}\n\
         [train-tiger] Tiger seed: {}\n\
         [train-tiger] artifact: {artifact}",
        if std::path::Path::new(&artifact).exists() {
            "existing champion"
        } else {
            "doctrine ancestor"
        }
    );

    const TRAIN_COUNTS: [usize; 3] = [2, 4, 6];
    const EXAM_SEEDS: [u32; 6] = [
        0xA11C_0001,
        0xA11C_0003,
        0xA11C_0007,
        0xA11C_000B,
        0xA11C_0011,
        0xA11C_001D,
    ];
    let eval = |game: &mut Game,
                renderer: &mut Renderer,
                tiger: &ai::Mlp,
                generation: u32,
                exam: bool|
     -> f32 {
        let mut total = 0.0;
        let mut n = 0usize;
        for (ci, &count) in TRAIN_COUNTS.iter().enumerate() {
            let seeds: Vec<u32> = if exam {
                EXAM_SEEDS[ci * 2..ci * 2 + 2].to_vec()
            } else {
                vec![
                    training_seed(generation / 6, ci * 2),
                    training_seed(generation / 6, ci * 2 + 1),
                ]
            };
            for seed in seeds {
                total += fitness(
                    simulate_battle(game, renderer, &t34_brain, tiger, count, seed, match_secs),
                    match_secs,
                );
                n += 1;
            }
        }
        total / n as f32
    };

    let deadline = (minutes > 0.0).then(|| std::time::Duration::from_secs_f32(minutes * 60.0));
    let started = Instant::now();
    let elite_n = (population / 4).max(2);
    let mut shipping_weights = doctrine.w.clone();
    let mut parents: Vec<Vec<f32>> = vec![shipping_weights.clone()];
    let checkpoint_next = std::fs::read_dir(&checkpoint_dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            name.strip_prefix("gen_")
                .and_then(|rest| rest.get(..4))
                .and_then(|digits| digits.parse::<u32>().ok())
        })
        .max()
        .map_or(0, |generation| generation + 1);
    let history_path = format!("{checkpoint_dir}/training.csv");
    let mut history = std::fs::read_to_string(&history_path).unwrap_or_else(|_| {
        String::from("generation,classroom_best,classroom_mean,exam_best,elapsed_minutes\n")
    });
    let history_next = history
        .lines()
        .skip(1)
        .filter_map(|line| line.split(',').next()?.parse::<u32>().ok())
        .max()
        .map_or(0, |generation| generation + 1);
    let generation_offset = checkpoint_next.max(history_next);
    // A resumed session must not overwrite a proven champion merely because its local
    // `best_exam` started at -infinity. Grade the loaded artifact before admitting any child.
    let mut best_exam = eval(&mut game, &mut renderer, &doctrine, 0, true);
    let mut best_generation: Option<u32> = None;
    println!(
        "[train-tiger] incumbent exam: {best_exam:.1}; new generations start at {generation_offset}"
    );

    for local_generation in 0..generations {
        let generation = generation_offset + local_generation;
        if deadline.is_some_and(|d| started.elapsed() >= d) {
            println!("[train-tiger] wall-clock cap reached before generation {generation}");
            break;
        }

        let mut scored = Vec::with_capacity(population);
        for child in 0..population {
            let weights = if child == 0 {
                // Permanent held-out-exam elite. Classroom luck can breed, but it can never evict
                // the only policy we have actually proved generalizes.
                shipping_weights.clone()
            } else if child + 2 >= population {
                ai::Mlp::seeded(training_seed(generation, child + 1000), 0.45).w
            } else {
                let parent = &parents[child % parents.len()];
                ai::mutate(
                    parent,
                    training_seed(generation, child),
                    sigma * (0.985f32).powi(generation as i32),
                )
            };
            let net = ai::Mlp::new(weights.clone());
            let score = eval(&mut game, &mut renderer, &net, generation, false);
            scored.push((weights, score));
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mean = scored.iter().map(|x| x.1).sum::<f32>() / scored.len() as f32;
        let classroom = scored[0].1;
        let champion = ai::Mlp::new(scored[0].0.clone());
        let exam = eval(&mut game, &mut renderer, &champion, generation, true);
        let elapsed_min = started.elapsed().as_secs_f32() / 60.0;
        println!(
            "[train-tiger] gen {generation:>4}: classroom {classroom:>7.1}  \
             mean {mean:>7.1}  exam {exam:>7.1}  ({elapsed_min:.1} min)"
        );
        history.push_str(&format!(
            "{generation},{classroom:.3},{mean:.3},{exam:.3},{elapsed_min:.3}\n"
        ));

        if exam > best_exam {
            best_exam = exam;
            best_generation = Some(generation);
            shipping_weights = champion.w.clone();
            champion.save(&artifact, generation, exam)?;
            let checkpoint = format!(
                "{checkpoint_dir}/gen_{generation:04}_class_{classroom:.0}_exam_{exam:.0}.bin"
            );
            champion.save(&checkpoint, generation, exam)?;
            println!("[train-tiger] ── EXAM RECORD: {exam:.1} → {checkpoint}");
        }
        let mut next_parents = vec![shipping_weights.clone()];
        for (weights, _) in scored.into_iter() {
            if next_parents.len() >= elite_n {
                break;
            }
            if weights != shipping_weights {
                next_parents.push(weights);
            }
        }
        parents = next_parents;
    }

    std::fs::write(&history_path, history)?;
    let reloaded = ai::Mlp::load(&artifact)?;
    let replay = eval(
        &mut game,
        &mut renderer,
        &reloaded,
        best_generation.unwrap_or(0),
        true,
    );
    let champion_label = best_generation
        .map(|generation| format!("generation {generation}"))
        .unwrap_or_else(|| "the incumbent entering this session".to_string());
    println!(
        "[train-tiger] RESULT: champion {champion_label}, exam {best_exam:.2}, \
         reload {replay:.2} ({})",
        if (best_exam - replay).abs() < 0.001 {
            "bit-stable"
        } else {
            "seed-window differs"
        }
    );
    Ok(())
}

fn gauntlet_seed(i: usize) -> u32 {
    const FIXED: [u32; 12] = [
        0x34A0_0001,
        0x34A0_0002,
        0x34A0_0003,
        0x34A0_0005,
        0x34A0_0008,
        0x34A0_000D,
        0x34A0_0015,
        0x34A0_0022,
        0x34A0_0037,
        0x34A0_0059,
        0x34A0_0090,
        0x34A0_00E9,
    ];
    FIXED
        .get(i)
        .copied()
        .unwrap_or_else(|| training_seed(0x34A0, i))
}

#[derive(Debug)]
struct GauntletReport {
    outcomes: Vec<BattleOutcome>,
    majority_threshold: Option<usize>,
}

fn evaluate_gauntlet(
    game: &mut Game,
    renderer: &mut Renderer,
    t34_brain: &ai::Mlp,
    tiger_brain: &ai::Mlp,
    seed_count: usize,
    max_secs: f32,
) -> GauntletReport {
    let mut outcomes = Vec::with_capacity(MAX_T34S * seed_count);
    let mut majority_threshold = None;
    println!("[gauntlet] count | T-34 wins | Tiger wins | mean T-34 survivors | mean time");
    for count in 1..=MAX_T34S {
        let mut t34_wins = 0usize;
        let mut survivor_sum = 0usize;
        let mut time_sum = 0.0;
        for i in 0..seed_count {
            let outcome = simulate_battle(
                game,
                renderer,
                t34_brain,
                tiger_brain,
                count,
                gauntlet_seed(i),
                max_secs,
            );
            t34_wins += outcome.t34_win() as usize;
            survivor_sum += outcome.t34_survivors;
            time_sum += outcome.seconds;
            outcomes.push(outcome);
        }
        let tiger_wins = seed_count - t34_wins;
        let mean_survivors = survivor_sum as f32 / seed_count as f32;
        let mean_time = time_sum / seed_count as f32;
        println!(
            "[gauntlet]   {count:>2}  | {t34_wins:>3}/{seed_count:<3}   | \
             {tiger_wins:>3}/{seed_count:<3}    | {mean_survivors:>7.2}              | {mean_time:>6.1}s"
        );
        if majority_threshold.is_none() && t34_wins * 2 >= seed_count {
            majority_threshold = Some(count);
        }
    }
    GauntletReport {
        outcomes,
        majority_threshold,
    }
}

fn write_gauntlet_report(
    outdir: &str,
    report: &GauntletReport,
    t34_weights: &str,
    tiger_weights: &str,
    seed_count: usize,
    max_secs: f32,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    let mut csv =
        String::from("seed,t34_count,tiger_alive,t34_survivors,tiger_hp,seconds,hits,pens\n");
    for o in &report.outcomes {
        csv.push_str(&format!(
            "{:#010x},{},{},{},{:.2},{:.3},{},{}\n",
            o.seed,
            o.t34_count,
            o.tiger_alive,
            o.t34_survivors,
            o.tiger_hp,
            o.seconds,
            o.hits_on_tiger,
            o.pens_on_tiger
        ));
    }
    std::fs::write(format!("{outdir}/gauntlet.csv"), csv)?;

    let threshold = report
        .majority_threshold
        .map(|n| format!("{n} T-34s"))
        .unwrap_or_else(|| "not reached with six T-34s".to_string());
    let summary = format!(
        "KESSEL AI GAUNTLET\n\
         T-34 policy: {t34_weights}\n\
         Tiger policy: {tiger_weights}\n\
         seeds per force size: {seed_count}\n\
         match cap: {max_secs:.1} s\n\
         empirical majority threshold: {threshold}\n"
    );
    std::fs::write(format!("{outdir}/summary.txt"), summary)?;
    Ok(())
}

/// Measure 1–6 evolved T-34s against the evolved Tiger on fixed seeds.
pub fn run_gauntlet(
    outdir: &str,
    t34_weights: &str,
    tiger_weights: &str,
    seed_count: usize,
    max_secs: f32,
    act: u8,
) -> anyhow::Result<()> {
    ensure!(seed_count > 0, "ai-gauntlet needs at least one seed");
    let t34_brain =
        ai::Mlp::load(t34_weights).with_context(|| format!("loading T-34 policy {t34_weights}"))?;
    let tiger_brain = ai::Mlp::load(tiger_weights)
        .with_context(|| format!("loading Tiger policy {tiger_weights}"))?;
    let instance = super::create_instance();
    let adapter = super::request_adapter(&instance, None);
    println!(
        "[gauntlet] adapter: {} — act {act} ({})",
        adapter.get_info().name,
        crate::world::act(act).name
    );
    let (device, queue) = super::request_device(&adapter);
    let (mut renderer, mut game) =
        Game::build_act(&device, &queue, false, &crate::world::act(act))?;
    game.mute_audio();
    let report = evaluate_gauntlet(
        &mut game,
        &mut renderer,
        &t34_brain,
        &tiger_brain,
        seed_count,
        max_secs,
    );
    write_gauntlet_report(
        outdir,
        &report,
        t34_weights,
        tiger_weights,
        seed_count,
        max_secs,
    )?;
    println!(
        "[gauntlet] empirical majority threshold: {}",
        report
            .majority_threshold
            .map(|n| format!("{n} T-34s"))
            .unwrap_or_else(|| "not reached at six".to_string())
    );
    println!("[gauntlet] raw results: {outdir}/gauntlet.csv");
    Ok(())
}

#[derive(Clone, Copy)]
enum Shot {
    Establish,
    TigerShoulder,
    T34Chase,
    Crossfire,
    GroundOrbit,
    Tactical,
    EventClose,
}

struct CameraDirector {
    shot_index: usize,
    shot_age: f32,
    pos: Vec3,
    target: Vec3,
    fov: f32,
    initialized: bool,
    previous_alive: usize,
    previous_tiger_hp: f32,
    event_point: Option<Vec3>,
}

impl CameraDirector {
    fn new(game: &Game, active: &[usize]) -> Self {
        Self {
            shot_index: 0,
            shot_age: 99.0,
            pos: Vec3::ZERO,
            target: game.player.center(),
            fov: 48f32.to_radians(),
            initialized: false,
            previous_alive: active.iter().filter(|&&i| game.enemies[i].alive).count(),
            previous_tiger_hp: game.player.hp,
            event_point: None,
        }
    }

    fn live_indices(game: &Game, active: &[usize]) -> Vec<usize> {
        active
            .iter()
            .copied()
            .filter(|&i| game.enemies[i].alive)
            .collect()
    }

    fn focus_and_spread(game: &Game, active: &[usize]) -> (Vec3, f32) {
        let mut points = vec![game.player.center()];
        points.extend(
            Self::live_indices(game, active)
                .into_iter()
                .map(|i| game.enemies[i].center()),
        );
        let focus = points.iter().copied().sum::<Vec3>() / points.len() as f32;
        let spread = points
            .iter()
            .map(|p| p.distance(focus))
            .fold(0.0f32, f32::max);
        (focus, spread)
    }

    fn nearest_t34(game: &Game, active: &[usize]) -> Option<usize> {
        active
            .iter()
            .copied()
            .filter(|&i| game.enemies[i].alive)
            .min_by(|&a, &b| {
                game.enemies[a]
                    .center()
                    .distance_squared(game.player.center())
                    .total_cmp(
                        &game.enemies[b]
                            .center()
                            .distance_squared(game.player.center()),
                    )
            })
    }

    fn shot(&self) -> Shot {
        if self.event_point.is_some() && self.shot_age < 2.7 {
            return Shot::EventClose;
        }
        match self.shot_index % 6 {
            0 => Shot::Establish,
            1 => Shot::TigerShoulder,
            2 => Shot::T34Chase,
            3 => Shot::Crossfire,
            4 => Shot::GroundOrbit,
            _ => Shot::Tactical,
        }
    }

    fn update(&mut self, game: &Game, active: &[usize], renderer: &mut Renderer, dt: f32) {
        self.shot_age += dt;
        let live = Self::live_indices(game, active);
        let alive = live.len();
        let mut cut = false;

        if alive < self.previous_alive {
            let dead = active
                .iter()
                .copied()
                .find(|&i| !game.enemies[i].alive)
                .map(|i| game.enemies[i].center());
            self.event_point = dead;
            self.shot_age = 0.0;
            self.shot_index += 1;
            cut = true;
        } else if game.player.hp + 1.0 < self.previous_tiger_hp {
            self.event_point = Some(game.player.center());
            self.shot_age = 0.0;
            self.shot_index += 1;
            cut = true;
        } else if self.shot_age > if self.shot_index == 0 { 3.4 } else { 5.5 } {
            self.event_point = None;
            self.shot_age = 0.0;
            self.shot_index += 1;
            cut = true;
        }
        self.previous_alive = alive;
        self.previous_tiger_hp = game.player.hp;

        let (focus, spread) = Self::focus_and_spread(game, active);
        let tiger = game.player.center();
        let nearest = Self::nearest_t34(game, active);
        let actor = nearest.map(|i| game.enemies[i].center()).unwrap_or(tiger);
        let line = (tiger - actor).normalize_or_zero();
        let side = Vec3::new(line.z, 0.0, -line.x).normalize_or_zero();

        let (mut desired_pos, desired_target, desired_fov) = match self.shot() {
            Shot::Establish => {
                let dist = (spread * 0.82 + 32.0).clamp(62.0, 120.0);
                (
                    focus + Vec3::new(-0.62, 0.48, -0.62).normalize() * dist,
                    focus,
                    44f32.to_radians(),
                )
            }
            Shot::TigerShoulder => {
                let f = game.player.forward();
                let r = game.player.right();
                (
                    tiger - f * 17.0 + r * 7.0 + Vec3::Y * 5.2,
                    tiger + (actor - tiger) * 0.38 + Vec3::Y * 0.8,
                    42f32.to_radians(),
                )
            }
            Shot::T34Chase => {
                if let Some(i) = nearest {
                    let e = &game.enemies[i];
                    (
                        e.center() - e.forward() * 14.0 - e.right() * 3.5 + Vec3::Y * 4.0,
                        e.center() + (tiger - e.center()) * 0.32 + Vec3::Y * 0.8,
                        45f32.to_radians(),
                    )
                } else {
                    (
                        tiger + Vec3::new(-15.0, 5.0, -15.0),
                        tiger,
                        44f32.to_radians(),
                    )
                }
            }
            Shot::Crossfire => {
                let mid = (tiger + actor) * 0.5;
                let offset = (tiger.distance(actor) * 0.20 + 20.0).clamp(24.0, 48.0);
                (
                    mid + side * offset + Vec3::Y * 6.0,
                    mid + Vec3::Y,
                    39f32.to_radians(),
                )
            }
            Shot::GroundOrbit => {
                let a = self.shot_index as f32 * 1.37 + self.shot_age * 0.12;
                (
                    focus + Vec3::new(a.sin() * 34.0, 8.0, a.cos() * 34.0),
                    focus + Vec3::Y,
                    46f32.to_radians(),
                )
            }
            Shot::Tactical => (
                focus + Vec3::new(-32.0, 54.0, -28.0),
                focus,
                52f32.to_radians(),
            ),
            Shot::EventClose => {
                let p = self.event_point.unwrap_or(focus);
                let away = (p - focus).normalize_or_zero();
                (
                    p + side * 17.0 - away * 8.0 + Vec3::Y * 4.5,
                    p + Vec3::Y * 0.8,
                    37f32.to_radians(),
                )
            }
        };

        // No subterranean dolly shots when the fight crosses a rise.
        desired_pos.y = desired_pos
            .y
            .max(game.hf.height_at(desired_pos.x, desired_pos.z) + 2.4);
        if !self.initialized || cut {
            self.pos = desired_pos;
            self.target = desired_target;
            self.fov = desired_fov;
            self.initialized = true;
        } else {
            let follow = 1.0 - (-dt * 3.2).exp();
            let lens = 1.0 - (-dt * 2.0).exp();
            self.pos = self.pos.lerp(desired_pos, follow);
            self.target = self.target.lerp(desired_target, follow);
            self.fov += (desired_fov - self.fov) * lens;
        }
        self.pos.y = self
            .pos
            .y
            .max(game.hf.height_at(self.pos.x, self.pos.z) + 2.2);
        renderer.camera.pos = self.pos;
        renderer.camera.set_forward(self.target - self.pos);
        renderer.camera.fov_y = self.fov;
        // The AI crew is physically posted at the gunner/loader stations, but this camera is not.
        // Strip every first-person-only layer after `Game::update` has populated the renderer.
        renderer.reticle = false;
        renderer.optic_mask = 0.0;
        renderer.optic_blast = 0.0;
        renderer.crew_ko = 0.0;
        renderer.interior_haze = 0.0;
        renderer.interior_light = 0.0;
    }
}

fn representative<'a>(
    report: &'a GauntletReport,
    count: usize,
    max_secs: f32,
) -> &'a BattleOutcome {
    let candidates: Vec<&BattleOutcome> = report
        .outcomes
        .iter()
        .filter(|o| o.t34_count == count)
        .collect();
    candidates
        .iter()
        .copied()
        .filter(|o| o.t34_win())
        // Prefer a resolved, dramatic mid-length T-34 victory rather than the shortest lucky hit.
        .min_by(|a, b| {
            (a.seconds - max_secs.min(55.0) * 0.72)
                .abs()
                .total_cmp(&(b.seconds - max_secs.min(55.0) * 0.72).abs())
        })
        .or_else(|| {
            candidates.iter().copied().max_by(|a, b| {
                a.t34s_destroyed()
                    .cmp(&b.t34s_destroyed())
                    .then_with(|| a.seconds.total_cmp(&b.seconds))
            })
        })
        .expect("gauntlet always has one result for each count")
}

fn spawn_encoder(
    output: &str,
    iw: u32,
    ih: u32,
    fps: u32,
    output_width: u32,
    output_height: u32,
) -> anyhow::Result<std::process::Child> {
    Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &format!("{iw}x{ih}"),
            "-framerate",
            &fps.to_string(),
            "-i",
            "-",
            "-an",
            "-vf",
            &format!("scale={output_width}:{output_height}:flags=neighbor,format=yuv420p"),
            "-c:v",
            "libx264",
            "-preset",
            "slow",
            "-crf",
            "14",
            "-movflags",
            "+faststart",
            output,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("starting ffmpeg (is it installed?)")
}

fn extract_poster(video: &str, poster: &str, at_secs: f32) -> anyhow::Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-ss",
            &format!("{at_secs:.2}"),
            "-i",
            video,
            "-frames:v",
            "1",
            poster,
        ])
        .status()?;
    ensure!(status.success(), "ffmpeg could not extract showreel poster");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_take(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    game: &mut Game,
    t34_brain: &ai::Mlp,
    tiger_brain: &ai::Mlp,
    outdir: &str,
    count: usize,
    seed: u32,
    max_secs: f32,
    t34_weights: &str,
    tiger_weights: &str,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    let active = stage(game, t34_brain, count, seed, GAUNTLET_RING_M);
    renderer.fog_density = 0.00085;
    // A snowfield fills most wide shots and makes adaptive metering silhouette the dark armor.
    // Pin the calibrated daylight exposure for consistent tank detail across every cut.
    renderer.autoexp = false;
    renderer.reticle = false;
    let mut pilot = TigerPilot::new(tiger_brain);
    let mut director = CameraDirector::new(game, &active);

    const FPS: u32 = 24;
    let four_k = std::env::var("KESSEL_SHOWREEL_4K")
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let (ow, oh) = if four_k { (3840, 2160) } else { (1920, 1080) };
    let video = format!("{outdir}/tiger_vs_{count}_t34_showreel.mp4");
    let poster = format!("{outdir}/tiger_vs_{count}_t34_poster.png");
    let mut encoder = spawn_encoder(&video, renderer.iw, renderer.ih, FPS, ow, oh)?;
    let mut stdin = encoder.stdin.take().context("ffmpeg stdin unavailable")?;

    let max_steps = (max_secs / DT).ceil() as usize;
    let mut next_frame = 0.0f32;
    let mut frames = 0u32;
    let mut battle_over_at: Option<f32> = None;
    let started = Instant::now();
    for step in 0..max_steps {
        game.debug_engage_all();
        let input = pilot.input(game, step);
        game.update(&input, DT, renderer);
        game.clear_crew_ko();
        director.update(game, &active, renderer, DT);
        let now = (step + 1) as f32 * DT;

        if now + DT * 0.5 >= next_frame {
            let rgba = renderer.render_to_rgba(device, queue, renderer.iw, renderer.ih)?;
            stdin
                .write_all(&rgba)
                .context("streaming frame to ffmpeg")?;
            frames += 1;
            next_frame += 1.0 / FPS as f32;
            if frames % (FPS * 5) == 0 {
                println!(
                    "[showreel] encoded {:>5.1}s / battle {:>5.1}s  (wall {:.1}s)",
                    frames as f32 / FPS as f32,
                    now,
                    started.elapsed().as_secs_f32()
                );
            }
        }

        let over = !game.player.alive || active.iter().all(|&i| !game.enemies[i].alive);
        if over {
            let ended = *battle_over_at.get_or_insert(now);
            if now - ended >= 4.0 {
                break;
            }
        }
    }
    drop(stdin);
    let status = encoder.wait()?;
    ensure!(status.success(), "ffmpeg failed while encoding {video}");
    let duration = frames as f32 / FPS as f32;
    // Late in a tipping-point battle the surviving T-34s are close enough to share a readable
    // frame with the Tiger; that makes a much stronger poster than the tactical wide shot.
    extract_poster(&video, &poster, (duration * 0.89).max(0.0))?;

    let survivors = active.iter().filter(|&&i| game.enemies[i].alive).count();
    let result = format!(
        "KESSEL AI SHOWREEL TAKE\n\
         video: {video}\n\
         resolution: {ow}x{oh}\n\
         frame rate: {FPS} fps\n\
         T-34 count: {count}\n\
         seed: {seed:#010x}\n\
         Tiger: {}\n\
         T-34 survivors: {survivors}\n\
         duration: {duration:.2} s\n\
         T-34 policy: {t34_weights}\n\
         Tiger policy: {tiger_weights}\n",
        if game.player.alive {
            "survived"
        } else {
            "destroyed"
        },
    );
    std::fs::write(format!("{outdir}/take.txt"), result)?;
    println!("[showreel] wrote {video} ({ow}x{oh}, {FPS} fps, CRF 14) + {poster}");
    Ok(())
}

/// Evaluate the force-size threshold, choose a representative seed, and record it with a proper
/// shot director at 24 fps / visually lossless H.264. `forced_count == None` selects the empirical
/// majority threshold (or six if the current Tiger survives every tested size).
pub fn run_ai_showreel(
    outdir: &str,
    t34_weights: &str,
    tiger_weights: &str,
    forced_count: Option<usize>,
    max_secs: f32,
    act: u8,
) -> anyhow::Result<()> {
    if let Some(n) = forced_count {
        ensure!(
            (1..=MAX_T34S).contains(&n),
            "showreel T-34 count must be 1..=6"
        );
    }
    std::fs::create_dir_all(outdir)?;
    let t34_brain =
        ai::Mlp::load(t34_weights).with_context(|| format!("loading T-34 policy {t34_weights}"))?;
    let tiger_brain = ai::Mlp::load(tiger_weights)
        .with_context(|| format!("loading Tiger policy {tiger_weights}"))?;
    let instance = super::create_instance();
    let adapter = super::request_adapter(&instance, None);
    println!(
        "[showreel] adapter: {} — act {act} ({})",
        adapter.get_info().name,
        crate::world::act(act).name
    );
    let (device, queue) = super::request_device(&adapter);
    let (mut renderer, mut game) =
        Game::build_act(&device, &queue, false, &crate::world::act(act))?;
    game.mute_audio();

    const SEEDS: usize = 7;
    let report = evaluate_gauntlet(
        &mut game,
        &mut renderer,
        &t34_brain,
        &tiger_brain,
        SEEDS,
        max_secs,
    );
    write_gauntlet_report(outdir, &report, t34_weights, tiger_weights, SEEDS, max_secs)?;
    let count = forced_count
        .or(report.majority_threshold)
        .unwrap_or(MAX_T34S);
    let selected = *representative(&report, count, max_secs);
    println!(
        "[showreel] selected {} T-34s, seed {:#010x}: Tiger {}, survivors {}, {:.1}s",
        count,
        selected.seed,
        if selected.tiger_alive {
            "survives"
        } else {
            "destroyed"
        },
        selected.t34_survivors,
        selected.seconds
    );

    record_take(
        &device,
        &queue,
        &mut renderer,
        &mut game,
        &t34_brain,
        &tiger_brain,
        outdir,
        count,
        selected.seed,
        max_secs,
        t34_weights,
        tiger_weights,
    )
}

/// Record a known force size/seed directly, without rerunning the full threshold sweep. This is the
/// building block for evolution-progress films: hold the battle constant, swap only the checkpoint.
pub fn run_ai_take(
    outdir: &str,
    t34_weights: &str,
    tiger_weights: &str,
    count: usize,
    seed: u32,
    max_secs: f32,
    act: u8,
) -> anyhow::Result<()> {
    ensure!(
        (1..=MAX_T34S).contains(&count),
        "ai-take T-34 count must be 1..=6"
    );
    let t34_brain =
        ai::Mlp::load(t34_weights).with_context(|| format!("loading T-34 policy {t34_weights}"))?;
    let tiger_brain = ai::Mlp::load(tiger_weights)
        .with_context(|| format!("loading Tiger policy {tiger_weights}"))?;
    let instance = super::create_instance();
    let adapter = super::request_adapter(&instance, None);
    println!(
        "[showreel] direct take: {} — {count} T-34s, seed {seed:#010x}",
        adapter.get_info().name
    );
    let (device, queue) = super::request_device(&adapter);
    let (mut renderer, mut game) =
        Game::build_act(&device, &queue, false, &crate::world::act(act))?;
    game.mute_audio();
    record_take(
        &device,
        &queue,
        &mut renderer,
        &mut game,
        &t34_brain,
        &tiger_brain,
        outdir,
        count,
        seed,
        max_secs,
        t34_weights,
        tiger_weights,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauntlet_seeds_are_stable_and_distinct() {
        assert_eq!(gauntlet_seed(0), 0x34A0_0001);
        assert_ne!(gauntlet_seed(7), gauntlet_seed(8));
        assert_eq!(gauntlet_seed(20), gauntlet_seed(20));
    }

    #[test]
    fn battle_fitness_rewards_a_clean_sweep() {
        let clean = BattleOutcome {
            seed: 1,
            t34_count: 4,
            tiger_alive: true,
            t34_survivors: 0,
            tiger_hp: 100.0,
            seconds: 30.0,
            hits_on_tiger: 0,
            pens_on_tiger: 0,
        };
        let loss = BattleOutcome {
            tiger_alive: false,
            t34_survivors: 2,
            tiger_hp: 0.0,
            seconds: 30.0,
            pens_on_tiger: 3,
            ..clean
        };
        assert!(fitness(clean, 90.0) > fitness(loss, 90.0) + 400.0);
    }
}
