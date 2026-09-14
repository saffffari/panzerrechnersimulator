//! AI battle lab + cinematic recorder.
//!
//! The normal campaign deliberately owns six expensive, fully simulated enemy rigs. This module
//! turns those six slots into a repeatable all-T-34 gauntlet, evolves a separate Tiger driver over
//! the same tiny MLP architecture, and records the representative fight through the real renderer.
//! It does not weaken the physical simulation: policies propose drive, steer and trigger intent;
//! crew movement, reload, travel lock, gun lay, armor and ballistics remain authoritative.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{ensure, Context};
use glam::{Vec2, Vec3};

use crate::acoustics::{self, ImpactSignature, ProceduralSound};
use crate::ai;
use crate::game::{Game, ShotSequenceCursor, Station, TankInput};
use crate::lan::{CrewFrame, LanRole, RoleInput, F_EXTINGUISH, F_FIRE_MAIN, F_SERVICE_BREECH};
use crate::renderer::Renderer;

#[path = "campaign_capture.rs"]
mod campaign_capture;
use campaign_capture::FatalCaptureLatch;

#[path = "crew_battle_capture.rs"]
mod crew_battle_capture;
pub use crew_battle_capture::run_crew_battle_capture;

const DT: f32 = 1.0 / 60.0;
const TIGER_CREW_CYCLE: f32 = 6.5;
const TIGER_OPENING_ACQUISITION: f32 = 3.5;
const GAUNTLET_RING_M: f32 = 205.0;
const MAX_T34S: usize = 6;

// Real Act-I procedural village: the block centres sit around (-30, 110). This point is four metres
// north of the (-45,95) street crossing, keeping the Tiger in the clear north/south lane while also
// missing the streetlamp planted at the exact junction.
const CAMPAIGN_VILLAGE_STAGING: Vec2 = Vec2::new(-45.0, 99.0);
const SHOWREEL_COMMANDER_PEER: u32 = 0xCA01;
const SHOWREEL_DRIVER_PEER: u32 = 0xCA02;
const SHOWREEL_GUNNER_PEER: u32 = 0xCA03;
const SHOWREEL_LOADER_PEER: u32 = 0xCA04;

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

pub(crate) struct TigerPilot<'a> {
    brain: &'a ai::Mlp,
    crew_cd: f32,
    prev_reload: f32,
}

impl<'a> TigerPilot<'a> {
    pub(crate) fn new(brain: &'a ai::Mlp) -> Self {
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
        self.input_with_dt(game, step, DT)
    }

    /// Campaign/headless variant of the embodied pilot tick. The showreel remains locked to its
    /// cinematic 60 Hz clock through [`Self::input`], while mission rollouts can state their own
    /// fixed step without silently changing station or reload timing.
    pub(crate) fn input_with_dt(&mut self, game: &mut Game, step: usize, dt: f32) -> TankInput {
        if game.player.reload > self.prev_reload + 0.2 {
            self.crew_cd = TIGER_CREW_CYCLE;
        }
        self.prev_reload = game.player.reload;
        self.crew_cd = (self.crew_cd - dt).max(0.0);

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
            game.auto_service_breech(step % 2 == 0, dt);
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

    /// Four-body version used by the campaign reel. Controls travel through the same couch station
    /// authority as real split-screen play, so a KIA peer immediately loses that post and cannot be
    /// resurrected by the film camera. Stable peer IDs let the penetration model attach casualties,
    /// ragdolls and cubic blood to the bodies visible in the shot.
    fn campaign_input(&mut self, game: &mut Game, dt: f32) -> TankInput {
        if game.player.reload > self.prev_reload + 0.2 {
            self.crew_cd = TIGER_CREW_CYCLE;
        }
        self.prev_reload = game.player.reload;
        self.crew_cd = (self.crew_cd - dt).max(0.0);

        let first_couch_tick = game.player.alive && game.couch_living_crew() == 0;
        let role_alive = |role| first_couch_tick || game.couch_role_occupant(role).is_some();

        let mut drive = 0.0;
        let mut steer = 0.0;
        let mut fire_main = false;
        let mut service_breech = false;
        let mut extinguish = false;
        if game.player.alive {
            if game.is_fire_active() {
                extinguish = role_alive(LanRole::Commander);
            } else {
                let action = self.brain.forward(&game.tiger_executor_obs());
                if role_alive(LanRole::Driver) {
                    drive = action[0];
                    steer = action[1];
                }
                if game.breech_loaded() && role_alive(LanRole::Gunner) {
                    game.aim_at_enemy();
                    fire_main = self.crew_cd <= 0.0 && action[2] > 0.0 && game.auto_can_fire();
                } else if !game.breech_loaded() && role_alive(LanRole::Loader) {
                    service_breech = true;
                }
            }
        }

        TankInput {
            coop: Some(campaign_crew_frame(
                drive,
                steer,
                fire_main,
                service_breech,
                extinguish,
            )),
            // This is a single-process authoritative couch crew, not a LAN prediction client.
            coop_host: true,
            couch_crew: true,
            ..TankInput::default()
        }
    }
}

fn campaign_crew_frame(
    drive: f32,
    steer: f32,
    fire_main: bool,
    service_breech: bool,
    extinguish: bool,
) -> CrewFrame {
    let mut frame = CrewFrame::default();
    for (peer, role) in [
        (SHOWREEL_COMMANDER_PEER, LanRole::Commander),
        (SHOWREEL_DRIVER_PEER, LanRole::Driver),
        (SHOWREEL_GUNNER_PEER, LanRole::Gunner),
        (SHOWREEL_LOADER_PEER, LanRole::Loader),
    ] {
        frame.roster[role as usize] = peer;
        frame.inputs[role as usize] = RoleInput {
            post: role,
            ..RoleInput::default()
        };
    }
    frame.inputs[LanRole::Driver as usize].drive = drive;
    frame.inputs[LanRole::Driver as usize].steer = steer;
    if fire_main {
        frame.inputs[LanRole::Gunner as usize].flags |= F_FIRE_MAIN;
    }
    if service_breech {
        frame.inputs[LanRole::Loader as usize].flags |= F_SERVICE_BREECH;
    }
    if extinguish {
        frame.inputs[LanRole::Commander as usize].flags |= F_EXTINGUISH;
    }
    frame
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

/// A filmed fight belongs at the resolved act Spawn marker. `Game::build_act` has already replaced
/// the compiled metadata with the validated manifest coordinates by the time this is called. Keep
/// Act 0 on its historical laboratory translation so threshold/replay films remain comparable.
fn take_staging_center(game: &Game) -> Vec2 {
    if game.act.id == 0 {
        Vec2::new(210.0, -210.0)
    } else {
        game.act.spawn
    }
}

fn stage_take(
    game: &mut Game,
    t34_brain: &ai::Mlp,
    t34_count: usize,
    seed: u32,
    ring_m: f32,
) -> Vec<usize> {
    game.god = false;
    game.brawl = false;
    game.targets_only = false;
    let center = take_staging_center(game);
    let active = game.stage_showreel_t34_gauntlet_at(t34_count, seed, ring_m, center);
    if game.act.id != 0 {
        let to_goal = game.act.goal - game.act.spawn;
        game.player.heading = to_goal.x.atan2(to_goal.y);
    }
    game.enemy_policy = Some(t34_brain.clone());
    active
}

fn stage_campaign_wave(game: &mut Game, t34_brain: &ai::Mlp, spec: CampaignWaveSpec) -> Vec<usize> {
    game.god = false;
    game.brawl = false;
    game.targets_only = false;
    let active = game.stage_showreel_t34_gauntlet_at(
        spec.count,
        spec.seed,
        spec.ring_m,
        CAMPAIGN_VILLAGE_STAGING,
    );
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
    // Evolution consumes simulation state only; no render graph follows Game::update here.
    // Deferring RTT therefore suppresses the otherwise invisible periscope scene submission.
    game.set_defer_rtt_to_render(true);

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
    // The CSV gauntlet is simulation-only. Keep the visual showreel paths unchanged, but avoid
    // spending one complete offscreen render on a periscope tile for every gauntlet tick.
    game.set_defer_rtt_to_render(true);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shot {
    Establish,
    CommanderInterior,
    TigerShoulder,
    LoaderInterior,
    T34Chase,
    DriverInterior,
    Crossfire,
    GroundOrbit,
    Tactical,
    EventClose,
}

impl Shot {
    fn interior_role(self) -> Option<LanRole> {
        match self {
            Shot::CommanderInterior => Some(LanRole::Commander),
            Shot::LoaderInterior => Some(LanRole::Loader),
            Shot::DriverInterior => Some(LanRole::Driver),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Shot::Establish => "exterior-establish",
            Shot::CommanderInterior => "interior-commander",
            Shot::TigerShoulder => "exterior-tiger-shoulder",
            Shot::LoaderInterior => "interior-loader",
            Shot::T34Chase => "exterior-t34-chase",
            Shot::DriverInterior => "interior-driver",
            Shot::Crossfire => "exterior-crossfire",
            Shot::GroundOrbit => "exterior-ground-orbit",
            Shot::Tactical => "exterior-tactical",
            Shot::EventClose => "exterior-event-close",
        }
    }

    fn duration(self) -> f32 {
        match self {
            Shot::Establish => 3.6,
            Shot::CommanderInterior | Shot::LoaderInterior | Shot::DriverInterior => 3.2,
            Shot::EventClose => 1.6,
            _ => 4.8,
        }
    }
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
    event_age: f32,
}

/// Renderer state authored by `Game::build_act` through the shipping campaign atmosphere path.
/// Camera direction and shelter may change during a take; these physical world/camera parameters
/// may not. Keeping a fingerprint makes a cinematic regression fail instead of silently becoming
/// a different weather setup from the level the player actually receives.
#[derive(Clone, Copy, Debug)]
struct TakeAtmosphere {
    fog_density: f32,
    precipitation: f32,
    sun_dir: Vec3,
    autoexp: bool,
    exposure_override: Option<f32>,
    master_retro: f32,
    night: f32,
    twilight: f32,
    overcast: f32,
    storm: f32,
    atmosphere_mood: f32,
}

impl TakeAtmosphere {
    fn capture(renderer: &Renderer) -> Self {
        Self {
            fog_density: renderer.fog_density,
            precipitation: renderer.precipitation,
            sun_dir: renderer.sun_dir,
            autoexp: renderer.autoexp,
            exposure_override: renderer.exposure_override,
            master_retro: renderer.master_retro,
            night: renderer.night,
            twilight: renderer.twilight,
            overcast: renderer.overcast,
            storm: renderer.storm,
            atmosphere_mood: renderer.atmosphere_mood,
        }
    }

    fn matches(self, renderer: &Renderer) -> bool {
        self.fog_density.to_bits() == renderer.fog_density.to_bits()
            && self.precipitation.to_bits() == renderer.precipitation.to_bits()
            && self.sun_dir == renderer.sun_dir
            && self.autoexp == renderer.autoexp
            && self.exposure_override == renderer.exposure_override
            && self.master_retro.to_bits() == renderer.master_retro.to_bits()
            && self.night.to_bits() == renderer.night.to_bits()
            && self.twilight.to_bits() == renderer.twilight.to_bits()
            && self.overcast.to_bits() == renderer.overcast.to_bits()
            && self.storm.to_bits() == renderer.storm.to_bits()
            && self.atmosphere_mood.to_bits() == renderer.atmosphere_mood.to_bits()
    }
}

impl CameraDirector {
    fn new(game: &Game, active: &[usize]) -> Self {
        Self {
            shot_index: 0,
            shot_age: 0.0,
            pos: Vec3::ZERO,
            target: game.player.center(),
            fov: 48f32.to_radians(),
            initialized: false,
            previous_alive: active.iter().filter(|&&i| game.enemies[i].alive).count(),
            previous_tiger_hp: game.player.hp,
            event_point: None,
            event_age: 0.0,
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
        if self.event_point.is_some() {
            return Shot::EventClose;
        }
        match self.shot_index % 9 {
            0 => Shot::Establish,
            1 => Shot::CommanderInterior,
            2 => Shot::TigerShoulder,
            3 => Shot::LoaderInterior,
            4 => Shot::T34Chase,
            5 => Shot::DriverInterior,
            6 => Shot::Crossfire,
            7 => Shot::GroundOrbit,
            _ => Shot::Tactical,
        }
    }

    fn is_interior(&self) -> bool {
        self.shot().interior_role().is_some()
    }

    fn shot_label(&self) -> &'static str {
        self.shot().label()
    }

    fn update(&mut self, game: &Game, active: &[usize], renderer: &mut Renderer, dt: f32) {
        self.shot_age += dt;
        if self.event_point.is_some() {
            self.event_age += dt;
        }
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
            self.event_age = 0.0;
            cut = true;
        } else if game.player.hp + 1.0 < self.previous_tiger_hp {
            self.event_point = Some(game.player.center());
            self.event_age = 0.0;
            cut = true;
        } else if self.event_point.is_some() && self.event_age >= 1.6 {
            self.event_point = None;
            self.event_age = 0.0;
            cut = true;
        }
        if self.shot_age > self.shot().duration() {
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

        if let Some(role) = self.shot().interior_role() {
            let (mut eye, mut forward, fov, mask, reticle) = game.coop_view_camera(role);
            // The play cameras are authored for task visibility. Give the commander film cut a
            // slight downward bias so the other crew and the fighting compartment share the frame.
            if role == LanRole::Commander {
                eye -= Vec3::Y * 0.10;
                forward = (forward - Vec3::Y * 0.22).normalize_or_zero();
            }
            renderer.camera.pos = eye;
            renderer.camera.set_forward(forward);
            renderer.camera.fov_y = fov;
            renderer.reticle = reticle;
            renderer.optic_mask = mask;
            renderer.optic_blast = 0.0;
            renderer.weather_shelter = 1.0;
            game.configure_crew_render_view(role, false, renderer);
            game.drive_showreel_crew_rigs_for_view(role, renderer);
            game.apply_cinematic_camera_impulses(renderer);
            // Force the next exterior view to cut rather than dolly out through the armor.
            self.initialized = false;
            return;
        }

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
                    tiger - f * 14.0 + r * 6.0 + Vec3::Y * 4.2,
                    tiger + f * 2.4 + Vec3::Y * 0.9,
                    46f32.to_radians(),
                )
            }
            Shot::T34Chase => {
                if let Some(i) = nearest {
                    let e = &game.enemies[i];
                    (
                        e.center() - e.forward() * 12.0 - e.right() * 4.0 + Vec3::Y * 3.8,
                        e.center() + e.forward() * 2.0 + Vec3::Y * 0.8,
                        48f32.to_radians(),
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
            Shot::CommanderInterior | Shot::LoaderInterior | Shot::DriverInterior => unreachable!(),
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
        renderer.rt_mix = 1.0;
        renderer.weather_shelter = 0.0;
        renderer.set_interior_atmosphere_enabled(false);
        game.hide_couch_crew_rigs(renderer);
        game.apply_cinematic_camera_impulses(renderer);
    }
}

// ── Campaign-wave film ──────────────────────────────────────────────────────────────────────────

/// A deliberately small observational shot vocabulary for the campaign-wave film. Ordinary hits
/// never trigger a cut. The one exception is an authoritative Tiger-destroyed transition, which is
/// latched so a between-frame loss is witnessed from the established full-turret composition:
/// breech centred, commander under the cupola, gunner left and loader right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CampaignShot {
    VillageWide,
    TurretWide,
    HullCrewWide,
    DeathWide,
}

impl CampaignShot {
    fn label(self) -> &'static str {
        match self {
            Self::VillageWide => "springbrunnen-wide",
            Self::TurretWide => "interior-full-turret",
            Self::HullCrewWide => "interior-driver-witness",
            Self::DeathWide => "springbrunnen-wreck",
        }
    }

    fn interior(self) -> bool {
        matches!(self, Self::TurretWide | Self::HullCrewWide)
    }
}

#[derive(Clone, Copy, Debug)]
struct CampaignWaveSpec {
    count: usize,
    seed: u32,
    ring_m: f32,
}

// The first two attacks establish the escalation quickly. The six-tank climax uses the audited
// generation-101 contact-solver seed; the take contract below still rejects it unless all three
// waves actually start and the authoritative four-person couch crew is destroyed in wave three.
const CAMPAIGN_WAVES: [CampaignWaveSpec; 3] = [
    CampaignWaveSpec {
        count: 1,
        seed: 0x34A0_0003,
        ring_m: GAUNTLET_RING_M,
    },
    CampaignWaveSpec {
        count: 2,
        seed: 0x34A0_0008,
        ring_m: GAUNTLET_RING_M,
    },
    CampaignWaveSpec {
        count: 6,
        seed: 0x34A0_0002,
        ring_m: GAUNTLET_RING_M,
    },
];

const CAMPAIGN_WAVE_CLEAR_HOLD: f32 = 1.75;
const CAMPAIGN_WAVE_BREATHER: f32 = 3.25;
const CAMPAIGN_DEATH_HOLD: f32 = 4.5;
const CAMPAIGN_POSTROLL: f32 = 7.0;
const CAMPAIGN_FINAL_OPENING_INTERIOR_END: f32 = 16.0;
const CAMPAIGN_FINAL_FATAL_INTERIOR_START: f32 = 30.0;
const CAMPAIGN_DISTINCT_INTERIOR_IMPACT_GAP: f32 = 1.25;
const CAMPAIGN_FATAL_INTERIOR_HOLD: f32 = 2.0;
const CAMPAIGN_FINAL_CROSSFIRE_M: f32 = 96.0;
const CAMPAIGN_RETRO: f32 = 0.12;
const SHOWREEL_FOG_DENSITY_ENV: &str = "KESSEL_SHOWREEL_FOG_DENSITY";
const SHOWREEL_RETRO_ENV: &str = "KESSEL_SHOWREEL_RETRO";
const SHOWREEL_FINAL_SEED_ENV: &str = "KESSEL_SHOWREEL_FINAL_SEED";
const SHOWREEL_FINAL_LAYOUT_ENV: &str = "KESSEL_SHOWREEL_FINAL_LAYOUT";
const SHOWREEL_FINAL_GEOMETRY_ENV: &str = "KESSEL_SHOWREEL_FINAL_GEOMETRY_M";
const ACT1_STREET_X: [f32; 4] = [-75.0, -45.0, -15.0, 15.0];
const ACT1_STREET_Z: [f32; 4] = [65.0, 95.0, 125.0, 155.0];

fn parse_showreel_scalar(name: &str, raw: &str, minimum: f32, maximum: f32) -> anyhow::Result<f32> {
    let value = raw
        .trim()
        .parse::<f32>()
        .with_context(|| format!("invalid {name}={raw:?}"))?;
    ensure!(
        value.is_finite() && value >= minimum && value <= maximum,
        "invalid {name}={raw:?}; expected a finite value in {minimum}..={maximum}"
    );
    Ok(value)
}

fn configured_showreel_scalar(
    name: &str,
    default: f32,
    minimum: f32,
    maximum: f32,
) -> anyhow::Result<f32> {
    match std::env::var(name) {
        Ok(raw) => parse_showreel_scalar(name, &raw, minimum, maximum),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => anyhow::bail!("{name} is not valid Unicode"),
    }
}

fn parse_showreel_seed(name: &str, raw: &str) -> anyhow::Result<u32> {
    let raw = raw.trim();
    let parsed = if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else {
        raw.parse::<u32>()
    };
    parsed.with_context(|| format!("invalid {name}={raw:?}; expected a u32 or 0x-prefixed hex u32"))
}

fn configured_showreel_seed(default: u32) -> anyhow::Result<u32> {
    match std::env::var(SHOWREEL_FINAL_SEED_ENV) {
        Ok(raw) => parse_showreel_seed(SHOWREEL_FINAL_SEED_ENV, &raw),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("{SHOWREEL_FINAL_SEED_ENV} is not valid Unicode")
        }
    }
}

fn configured_showreel_layout(default: CampaignFinalLayout) -> anyhow::Result<CampaignFinalLayout> {
    match std::env::var(SHOWREEL_FINAL_LAYOUT_ENV) {
        Ok(raw) => CampaignFinalLayout::parse(raw.trim()),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("{SHOWREEL_FINAL_LAYOUT_ENV} is not valid Unicode")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CampaignFinalLayout {
    StreetCrossfire,
    Ring,
}

impl CampaignFinalLayout {
    fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "crossfire" | "street-crossfire" => Ok(Self::StreetCrossfire),
            "ring" => Ok(Self::Ring),
            _ => {
                anyhow::bail!("campaign final layout must be 'crossfire' or 'ring', got {value:?}")
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::StreetCrossfire => "street-crossfire",
            Self::Ring => "ring",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CampaignWaveConfig {
    final_seed: u32,
    final_layout: CampaignFinalLayout,
    final_geometry_m: f32,
}

impl Default for CampaignWaveConfig {
    fn default() -> Self {
        Self {
            final_seed: CAMPAIGN_WAVES[CAMPAIGN_WAVES.len() - 1].seed,
            final_layout: CampaignFinalLayout::StreetCrossfire,
            final_geometry_m: CAMPAIGN_FINAL_CROSSFIRE_M,
        }
    }
}

impl CampaignWaveConfig {
    fn wave_spec(self, wave: usize) -> CampaignWaveSpec {
        let mut spec = CAMPAIGN_WAVES[wave.min(CAMPAIGN_WAVES.len() - 1)];
        if wave == CAMPAIGN_WAVES.len() - 1 {
            spec.seed = self.final_seed;
            spec.ring_m = self.final_geometry_m;
        }
        spec
    }

    fn from_environment() -> anyhow::Result<Self> {
        let defaults = Self::default();
        Ok(Self {
            final_seed: configured_showreel_seed(defaults.final_seed)?,
            final_layout: configured_showreel_layout(defaults.final_layout)?,
            final_geometry_m: configured_showreel_scalar(
                SHOWREEL_FINAL_GEOMETRY_ENV,
                defaults.final_geometry_m,
                35.0,
                320.0,
            )?,
        })
    }
}

fn nearest_street(value: f32, streets: &[f32]) -> f32 {
    streets
        .iter()
        .copied()
        .min_by(|a, b| (value - *a).abs().total_cmp(&(value - *b).abs()))
        .unwrap_or(value)
}

fn campaign_geometry_hash(seed: u32, index: usize) -> f32 {
    let mut x = seed
        .wrapping_add(index as u32 * 7_919)
        .wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    ((x ^ (x >> 13)) & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

fn campaign_crossfire_positions(
    anchor: Vec2,
    forward: Vec2,
    seed: u32,
    geometry_m: f32,
) -> Vec<Vec2> {
    let forward = forward.normalize_or_zero();
    let forward = if forward.length_squared() > 0.5 {
        forward
    } else {
        Vec2::Y
    };
    let right = Vec2::new(forward.y, -forward.x);
    let diagonal_sign = if seed & 1 == 0 { 1.0 } else { -1.0 };
    let directions = [
        forward,
        -forward,
        right,
        -right,
        (forward + right * diagonal_sign).normalize(),
        (-forward + right * diagonal_sign).normalize(),
    ];
    directions
        .into_iter()
        .enumerate()
        .map(|(index, direction)| {
            let radius = geometry_m * (0.92 + campaign_geometry_hash(seed, index) * 0.16);
            let mut point = anchor + direction * radius;
            // A primarily east/west approach uses a horizontal street; north/south uses a vertical
            // one. Only the cross-axis is snapped, retaining the requested armor arc and distance.
            if direction.x.abs() > direction.y.abs() {
                point.y = nearest_street(point.y, &ACT1_STREET_Z);
            } else {
                point.x = nearest_street(point.x, &ACT1_STREET_X);
            }
            point
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CampaignWavePhase {
    Combat,
    Breather,
    Finished,
    FinalCleared,
}

struct CampaignWaveSequence {
    wave: usize,
    active: Vec<usize>,
    phase: CampaignWavePhase,
    phase_age: f32,
    clear_age: f32,
    total_age: f32,
    starts: Vec<f32>,
    ends: Vec<Option<f32>>,
    death_wave: Option<usize>,
    config: CampaignWaveConfig,
}

impl CampaignWaveSequence {
    fn new(active: Vec<usize>) -> Self {
        Self::with_config(active, CampaignWaveConfig::default())
    }

    fn with_config(active: Vec<usize>, config: CampaignWaveConfig) -> Self {
        Self {
            wave: 0,
            active,
            phase: CampaignWavePhase::Combat,
            phase_age: 0.0,
            clear_age: 0.0,
            total_age: 0.0,
            starts: vec![0.0],
            ends: vec![None],
            death_wave: None,
            config,
        }
    }

    fn current_spec(&self) -> CampaignWaveSpec {
        self.config.wave_spec(self.wave)
    }

    fn combat_shot(&self) -> CampaignShot {
        if self.wave == 0 {
            CampaignShot::VillageWide
        } else if self.wave == 1 || self.phase_age < CAMPAIGN_FINAL_OPENING_INTERIOR_END {
            CampaignShot::TurretWide
        } else if self.wave == CAMPAIGN_WAVES.len() - 1
            && self.phase_age >= CAMPAIGN_FINAL_FATAL_INTERIOR_START
        {
            // Return to the established full-turret tableau well before the likely lethal window.
            // The earlier breather already proves the Driver; the climax belongs on the angle that
            // holds Commander, Gunner and Loader together around the recoiling breech.
            CampaignShot::TurretWide
        } else {
            CampaignShot::VillageWide
        }
    }

    /// Advance wave bookkeeping after the authoritative game tick. Returns true on the one frame a
    /// new wave is mustered, allowing event telemetry to rebase its per-slot alive ledger.
    fn advance(&mut self, game: &mut Game, dt: f32) -> bool {
        self.total_age += dt;
        self.phase_age += dt;
        if !game.player.alive && self.phase != CampaignWavePhase::Finished {
            if let Some(end) = self.ends.get_mut(self.wave) {
                *end = Some(self.total_age);
            }
            self.phase = CampaignWavePhase::Finished;
            self.phase_age = 0.0;
            self.death_wave = Some(self.wave);
            return false;
        }

        match self.phase {
            CampaignWavePhase::Combat => {
                let cleared = self.active.iter().all(|&i| !game.enemies[i].alive);
                if cleared {
                    self.clear_age += dt;
                    if self.clear_age >= CAMPAIGN_WAVE_CLEAR_HOLD {
                        if let Some(end) = self.ends.get_mut(self.wave) {
                            *end = Some(self.total_age);
                        }
                        self.phase_age = 0.0;
                        self.clear_age = 0.0;
                        if self.wave + 1 < CAMPAIGN_WAVES.len() {
                            self.phase = CampaignWavePhase::Breather;
                        } else {
                            self.phase = CampaignWavePhase::FinalCleared;
                        }
                    }
                } else {
                    self.clear_age = 0.0;
                }
            }
            CampaignWavePhase::Breather if self.phase_age >= CAMPAIGN_WAVE_BREATHER => {
                self.wave += 1;
                let spec = self.current_spec();
                self.active = if self.wave == CAMPAIGN_WAVES.len() - 1
                    && self.config.final_layout == CampaignFinalLayout::StreetCrossfire
                {
                    let center = Vec2::new(game.player.pos.x, game.player.pos.z);
                    let forward = game.player.forward();
                    let positions = campaign_crossfire_positions(
                        center,
                        Vec2::new(forward.x, forward.z),
                        spec.seed,
                        spec.ring_m,
                    );
                    game.stage_showreel_t34_crossfire_wave(&positions, spec.seed)
                } else {
                    game.stage_showreel_t34_wave(spec.count, spec.seed, spec.ring_m)
                };
                self.phase = CampaignWavePhase::Combat;
                self.phase_age = 0.0;
                self.clear_age = 0.0;
                self.starts.push(self.total_age);
                self.ends.push(None);
                return true;
            }
            CampaignWavePhase::Breather
            | CampaignWavePhase::Finished
            | CampaignWavePhase::FinalCleared => {}
        }
        false
    }

    fn finished_postroll(&self) -> bool {
        self.phase == CampaignWavePhase::Finished && self.phase_age >= CAMPAIGN_POSTROLL
    }

    fn completed_required_escalation(&self) -> bool {
        self.phase == CampaignWavePhase::Finished
            && self.death_wave == Some(CAMPAIGN_WAVES.len() - 1)
            && self.starts.len() == CAMPAIGN_WAVES.len()
            && self.ends.len() == CAMPAIGN_WAVES.len()
    }
}

struct CampaignCameraDirector {
    shot: CampaignShot,
    last_combat_shot: CampaignShot,
    death_hold_shot: Option<CampaignShot>,
    pos: Vec3,
    target: Vec3,
    fov: f32,
    initialized: bool,
    interior_impact_hold: f32,
}

impl CampaignCameraDirector {
    fn new() -> Self {
        Self {
            shot: CampaignShot::VillageWide,
            last_combat_shot: CampaignShot::VillageWide,
            death_hold_shot: None,
            pos: Vec3::ZERO,
            target: Vec3::ZERO,
            fov: 45f32.to_radians(),
            initialized: false,
            interior_impact_hold: 0.0,
        }
    }

    fn desired_shot(
        &mut self,
        waves: &CampaignWaveSequence,
        force_fatal_full_turret: bool,
    ) -> CampaignShot {
        // This is armed only by the authoritative living-to-destroyed simulation edge. Keep the
        // established full-turret witness angle until the encoder has actually received the
        // required number of interior frames; a 60 Hz event cannot fall between 24 Hz samples.
        if force_fatal_full_turret {
            return CampaignShot::TurretWide;
        }
        // This latch is armed only when a round lands while the camera is already inside. It never
        // cuts *to* an impact, but it prevents the phase clock from cutting away before fragments,
        // illumination and the crew/camera reaction have had at least two seconds to decay.
        if self.shot.interior()
            && self.interior_impact_hold > 0.0
            && waves.phase != CampaignWavePhase::Finished
        {
            return self.shot;
        }
        match waves.phase {
            CampaignWavePhase::Finished => {
                let held = *self.death_hold_shot.get_or_insert(self.last_combat_shot);
                if waves.phase_age < CAMPAIGN_DEATH_HOLD {
                    held
                } else {
                    CampaignShot::DeathWide
                }
            }
            CampaignWavePhase::Breather => {
                if waves.wave == 0 {
                    CampaignShot::HullCrewWide
                } else {
                    CampaignShot::VillageWide
                }
            }
            CampaignWavePhase::FinalCleared => CampaignShot::VillageWide,
            CampaignWavePhase::Combat => waves.combat_shot(),
        }
    }

    fn note_interior_impact(&mut self, hits: u32) {
        if hits > 0 && self.shot.interior() {
            self.interior_impact_hold = self.interior_impact_hold.max(2.25);
        }
    }

    fn note_fatal_transition(&mut self, destroyed_this_tick: bool) {
        if destroyed_this_tick {
            // Presentation changes only after the real simulation has destroyed the Tiger. Combat,
            // penetration and casualty state remain untouched.
            self.death_hold_shot = Some(CampaignShot::TurretWide);
        }
    }

    fn update(
        &mut self,
        game: &Game,
        waves: &CampaignWaveSequence,
        renderer: &mut Renderer,
        dt: f32,
        force_fatal_full_turret: bool,
    ) {
        self.interior_impact_hold = (self.interior_impact_hold - dt).max(0.0);
        let requested = self.desired_shot(waves, force_fatal_full_turret);
        if waves.phase != CampaignWavePhase::Finished {
            self.last_combat_shot = requested;
            if !force_fatal_full_turret {
                self.death_hold_shot = None;
            }
        }
        let cut = requested != self.shot;
        self.shot = requested;

        if self.shot.interior() {
            let (parent, eye_local, target_local, fov) = match self.shot {
                CampaignShot::TurretWide => (
                    game.player.turret_model(),
                    Vec3::new(-0.45, 2.34, -0.84),
                    Vec3::new(0.00, 1.45, 0.18),
                    80f32.to_radians(),
                ),
                // Free lens at the unused bow-radio side, looking across the transmission tunnel at
                // the Driver's complete seated figure. This complements rather than replaces the
                // required turret tableau, proving all four human bodies across the interior views.
                CampaignShot::HullCrewWide => (
                    game.player.hull_model(),
                    Vec3::new(-0.62, 1.48, 0.62),
                    Vec3::new(0.46, 1.18, 1.48),
                    72f32.to_radians(),
                ),
                _ => unreachable!(),
            };
            renderer.camera.pos = parent.transform_point3(eye_local);
            let target = parent.transform_point3(target_local);
            renderer.camera.set_forward(target - renderer.camera.pos);
            renderer.camera.fov_y = fov;
            renderer.optic_mask = 0.0;
            renderer.optic_blast = 0.0;
            renderer.reticle = false;
            renderer.weather_shelter = 1.0;
            renderer.precipitation = 0.0;
            renderer.set_active_view(4);
            game.configure_crew_render_view(LanRole::Loader, false, renderer);
            game.drive_showreel_all_crew_rigs(renderer);
            // Keep the same measured-HDR eye for both sides of the armor. A cut into this dark
            // compartment now adapts over time; the small snow-bright apertures remain brilliant
            // because they no longer select an authored per-shot exposure.
            renderer.exposure_override = None;
            game.apply_cinematic_camera_impulses(renderer);
            // A violent kick may move either witness camera, but never through the low steel roof.
            let mut kicked = parent.inverse().transform_point3(renderer.camera.pos);
            let roof_y = if self.shot == CampaignShot::TurretWide {
                2.34
            } else {
                1.82
            };
            if kicked.y > roof_y {
                kicked.y = roof_y;
                renderer.camera.pos = parent.transform_point3(kicked);
            }
            self.initialized = false;
            if cut {
                renderer.reset_history();
            }
            return;
        }

        let tiger = game.player.center();
        let (mut desired_pos, desired_target, desired_fov) = match self.shot {
            // Camera sits southwest and looks northeast across the Springbrunnen district. At 69 m
            // it remains inside the new dense Act-I fog instead of flattening the village to haze.
            CampaignShot::VillageWide => (
                tiger + Vec3::new(-52.0, 11.5, -44.0),
                tiger + Vec3::new(10.0, 1.1, 10.0),
                43f32.to_radians(),
            ),
            CampaignShot::DeathWide => (
                tiger + Vec3::new(-29.0, 7.5, -25.0),
                tiger + Vec3::Y * 1.0,
                46f32.to_radians(),
            ),
            CampaignShot::TurretWide | CampaignShot::HullCrewWide => unreachable!(),
        };
        desired_pos.y = desired_pos
            .y
            .max(game.hf.height_at(desired_pos.x, desired_pos.z) + 2.4);

        if !self.initialized || cut {
            self.pos = desired_pos;
            self.target = desired_target;
            self.fov = desired_fov;
            self.initialized = true;
            renderer.reset_history();
        } else {
            let follow = 1.0 - (-dt * 1.65).exp();
            let lens = 1.0 - (-dt * 1.25).exp();
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
        renderer.reticle = false;
        renderer.optic_mask = 0.0;
        renderer.optic_blast = 0.0;
        renderer.crew_ko = 0.0;
        renderer.interior_haze = 0.0;
        renderer.interior_light = 0.0;
        renderer.rt_mix = 1.0;
        renderer.weather_shelter = 0.0;
        renderer.precipitation = 1.0;
        renderer.set_active_view(0);
        renderer.set_interior_atmosphere_enabled(false);
        game.hide_couch_crew_rigs(renderer);
        renderer.exposure_override = None;
        game.apply_cinematic_camera_impulses(renderer);
    }

    fn is_interior(&self) -> bool {
        self.shot.interior()
    }

    fn shot_label(&self) -> &'static str {
        self.shot.label()
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

#[derive(Clone, Copy, Debug)]
enum CombatAudioCue {
    TigerGun { interior: bool },
    T34Gun { range_m: f32 },
    TigerImpact { penetrated: bool },
    T34Destroyed,
    TigerDestroyed,
}

#[derive(Clone, Copy, Debug)]
struct CombatAudioEvent {
    at: f32,
    bearing_deg: f32,
    seed: u32,
    cue: CombatAudioCue,
}

/// Drain every authoritative discharge sequence observed since the prior film tick. This is immune
/// to recoil-window duration and gives tests an exact event count without involving audio synthesis.
fn collect_shot_sequence_events<T>(
    cursor: &mut ShotSequenceCursor,
    sequence: u32,
    events: &mut Vec<T>,
    mut build: impl FnMut(u32) -> T,
) -> usize {
    let Some(advance) = cursor.observe(sequence) else {
        return 0;
    };
    let count = advance.count as usize;
    events.extend(advance.iter().map(&mut build));
    count
}

#[derive(Debug, Default)]
struct CampaignTakeAudit {
    interior_hit_count: u32,
    interior_penetration_count: u32,
    distinct_interior_impact_times: Vec<f32>,
    fatal_capture: FatalCaptureLatch,
}

impl CampaignTakeAudit {
    fn note_impact(&mut self, now: f32, interior: bool, hits: u32, penetrations: u32) {
        if !interior || hits == 0 {
            return;
        }
        self.interior_hit_count = self.interior_hit_count.saturating_add(hits);
        self.interior_penetration_count =
            self.interior_penetration_count.saturating_add(penetrations);
        let distinct = self
            .distinct_interior_impact_times
            .last()
            .map_or(true, |last| {
                now - *last + f32::EPSILON >= CAMPAIGN_DISTINCT_INTERIOR_IMPACT_GAP
            });
        if distinct {
            self.distinct_interior_impact_times.push(now);
        }
    }

    fn note_authoritative_tiger_state(&mut self, now: f32, was_alive: bool, alive: bool) -> bool {
        self.fatal_capture
            .observe_authoritative_state(now, was_alive, alive)
    }

    fn note_rendered_camera_frame(&mut self, now: f32, interior: bool, frame_seconds: f32) {
        self.fatal_capture
            .note_rendered_camera_frame(now, interior, frame_seconds);
    }

    fn requires_fatal_full_turret_camera(&self) -> bool {
        self.fatal_capture
            .requires_full_turret_camera(CAMPAIGN_FATAL_INTERIOR_HOLD)
    }

    fn has_required_interior_impacts(&self) -> bool {
        self.distinct_interior_impact_times.len() >= 2
    }

    fn has_required_fatal_hold(&self) -> bool {
        self.fatal_capture
            .has_required_hold(CAMPAIGN_FATAL_INTERIOR_HOLD, 1.0 / 48.0)
    }
}

fn camera_bearing_deg(renderer: &Renderer, source: Vec3) -> f32 {
    let to = (source - renderer.camera.pos).normalize_or_zero();
    to.dot(renderer.camera.right())
        .atan2(to.dot(renderer.camera.forward()))
        .to_degrees()
}

fn mix_stereo_at(dst: &mut [f32], src: &[f32], at_frames: usize, gain: f32) {
    let start = at_frames.saturating_mul(2);
    if start >= dst.len() {
        return;
    }
    let count = src.len().min(dst.len() - start);
    for i in 0..count {
        dst[start + i] += src[i] * gain;
    }
}

fn synth_combat_event(event: CombatAudioEvent) -> Vec<f32> {
    // Tiger armor uses the exact live-game body voice. In particular, do not call the full offline
    // damage rack here: its separately-authored fragment flyby/contact layer previously merged into
    // a white-noise-like impact wash. Bearing is set before synthesis, so it is spatialized once.
    if let CombatAudioCue::TigerImpact { penetrated } = event.cue {
        let signature = if penetrated {
            ImpactSignature::CloseT34SidePenetration
        } else {
            ImpactSignature::LongRangeT34Marginal
        };
        let (mut params, context) = acoustics::impact_signature_params(signature, event.seed);
        params.bearing_deg = event.bearing_deg;
        return acoustics::synth_damage_impact_body(params, context).main;
    }

    let mut clip = match event.cue {
        CombatAudioCue::TigerGun { interior } => acoustics::synth_gun_punch(interior, event.seed),
        CombatAudioCue::T34Gun { range_m } => {
            acoustics::synth_gun_report(76.2, range_m, event.bearing_deg, event.seed)
        }
        CombatAudioCue::TigerImpact { .. } => unreachable!("Tiger impact returned above"),
        CombatAudioCue::T34Destroyed => {
            acoustics::synth_procedural_dry(ProceduralSound::KillExplosion)
        }
        CombatAudioCue::TigerDestroyed => acoustics::synth_procedural_dry(ProceduralSound::CookOff),
    };
    if !matches!(event.cue, CombatAudioCue::T34Gun { .. }) {
        acoustics::apply_head_shadow(&mut clip, event.bearing_deg);
    }
    clip
}

fn build_showreel_soundtrack(
    duration: f32,
    fps: u32,
    interior_frames: &[bool],
    events: &[CombatAudioEvent],
) -> Vec<f32> {
    let sample_frames = (duration * acoustics::SAMPLE_RATE as f32).ceil() as usize;
    let mut out = vec![0.0f32; sample_frames.saturating_mul(2)];
    let engine_exterior = acoustics::synth_procedural_dry(ProceduralSound::EngineLoop);
    let ambience_exterior = acoustics::synth_procedural_dry(ProceduralSound::AmbienceExterior);
    let mut engine_interior = engine_exterior.clone();
    acoustics::interior_muffle(&mut engine_interior, 0.80);
    let ambience_interior = acoustics::synth_procedural_dry(ProceduralSound::AmbienceInterior);
    let loop_sample = |clip: &[f32], frame: usize, channel: usize| -> f32 {
        if clip.len() < 2 {
            return 0.0;
        }
        let clip_frames = clip.len() / 2;
        clip[(frame % clip_frames) * 2 + channel]
    };

    // Crossfade the acoustic perspective at camera cuts. The engine is the exact mechanical
    // procedural loop used by the runtime fallback; only its hull damping changes inside.
    let mut interior_mix = 0.0f32;
    for frame in 0..sample_frames {
        let video_frame = ((frame as f32 / acoustics::SAMPLE_RATE as f32) * fps as f32) as usize;
        let target = if interior_frames.get(video_frame).copied().unwrap_or(false) {
            1.0
        } else {
            0.0
        };
        interior_mix += (target - interior_mix) * 0.0018;
        for channel in 0..2 {
            let engine_out = loop_sample(&engine_exterior, frame, channel) * 0.20;
            let engine_in = loop_sample(&engine_interior, frame, channel) * 0.31;
            let wind = loop_sample(&ambience_exterior, frame, channel) * 0.105;
            let cabin = loop_sample(&ambience_interior, frame, channel) * 0.13;
            out[frame * 2 + channel] =
                (engine_out + wind) * (1.0 - interior_mix) + (engine_in + cabin) * interior_mix;
        }
    }

    for event in events {
        let gain = match event.cue {
            CombatAudioCue::TigerGun { .. } => 0.92,
            CombatAudioCue::T34Gun { .. } => 0.72,
            CombatAudioCue::TigerImpact { penetrated: true } => 0.88,
            CombatAudioCue::TigerImpact { penetrated: false } => 0.68,
            CombatAudioCue::T34Destroyed => 0.82,
            CombatAudioCue::TigerDestroyed => 0.95,
        };
        let clip = synth_combat_event(*event);
        let at = (event.at.max(0.0) * acoustics::SAMPLE_RATE as f32) as usize;
        mix_stereo_at(&mut out, &clip, at, gain);
    }
    acoustics::soft_limit_in_place(&mut out, 0.96);
    out
}

fn mux_showreel(picture: &str, soundtrack: &str, output: &str) -> anyhow::Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-y",
            "-i",
            picture,
            "-i",
            soundtrack,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "256k",
            "-shortest",
            "-movflags",
            "+faststart",
            output,
        ])
        .status()?;
    ensure!(status.success(), "ffmpeg could not mux showreel audio");
    Ok(())
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
    let atmosphere = TakeAtmosphere::capture(renderer);
    let staging_center = take_staging_center(game);
    let staging_source = if game.act.id == 0 {
        "legacy Act-0 laboratory center"
    } else {
        "resolved manifest Spawn marker"
    };
    let active = stage_take(game, t34_brain, count, seed, GAUNTLET_RING_M);
    let staging_heading_deg = game.player.heading.to_degrees();
    renderer.reticle = false;
    let mut pilot = TigerPilot::new(tiger_brain);
    let mut director = CameraDirector::new(game, &active);

    const FPS: u32 = 24;
    let four_k = std::env::var("KESSEL_SHOWREEL_4K")
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let (ow, oh) = if four_k { (3840, 2160) } else { (1920, 1080) };
    let stem = format!("ai_take_act{}_{}t34_seed_{seed:08x}", game.act.id, count);
    let picture = format!("{outdir}/{stem}_picture.mp4");
    let soundtrack = format!("{outdir}/{stem}_soundtrack.wav");
    let video = format!("{outdir}/{stem}_showreel.mp4");
    let poster = format!("{outdir}/{stem}_poster.png");
    let mut encoder = spawn_encoder(&picture, renderer.iw, renderer.ih, FPS, ow, oh)?;
    let mut stdin = encoder.stdin.take().context("ffmpeg stdin unavailable")?;

    let max_steps = (max_secs / DT).ceil() as usize;
    let mut next_frame = 0.0f32;
    let mut frames = 0u32;
    let mut battle_over_at: Option<f32> = None;
    let mut interior_frames = Vec::new();
    let mut camera_frames: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut audio_events = Vec::new();
    let mut tiger_shot_cursor = ShotSequenceCursor::at(game.main_gun_shot_sequence());
    let mut enemy_shot_cursors = (0..game.enemies.len())
        .map(|i| ShotSequenceCursor::at(game.enemy_main_gun_shot_sequence(i)))
        .collect::<Vec<_>>();
    let mut previous_alive: Vec<bool> = game.enemies.iter().map(|enemy| enemy.alive).collect();
    let mut previous_tiger_alive = game.player.alive;
    let mut previous_hits = game.stat_hits_on_player;
    let mut previous_pens = game.stat_pens_on_player;
    let started = Instant::now();
    for step in 0..max_steps {
        game.debug_engage_all();
        let input = pilot.input(game, step);
        game.update(&input, DT, renderer);
        game.clear_crew_ko();
        director.update(game, &active, renderer, DT);
        ensure!(
            atmosphere.matches(renderer),
            "ai-take camera mutated the shipping Act-{} atmosphere",
            game.act.id
        );
        let now = (step + 1) as f32 * DT;

        let tiger_interior = director.is_interior();
        collect_shot_sequence_events(
            &mut tiger_shot_cursor,
            game.main_gun_shot_sequence(),
            &mut audio_events,
            |shot_sequence| CombatAudioEvent {
                at: now,
                bearing_deg: 0.0,
                seed: seed ^ 0x8800_0000 ^ shot_sequence,
                cue: CombatAudioCue::TigerGun {
                    interior: tiger_interior,
                },
            },
        );
        for &i in &active {
            let enemy = &game.enemies[i];
            let range_m = enemy.center().distance(renderer.camera.pos);
            let bearing_deg = camera_bearing_deg(renderer, enemy.center());
            collect_shot_sequence_events(
                &mut enemy_shot_cursors[i],
                game.enemy_main_gun_shot_sequence(i),
                &mut audio_events,
                |shot_sequence| CombatAudioEvent {
                    at: now + range_m / 343.0,
                    bearing_deg,
                    seed: seed ^ 0x3400_0000 ^ ((i as u32) << 16) ^ shot_sequence,
                    cue: CombatAudioCue::T34Gun { range_m },
                },
            );
            if previous_alive[i] && !enemy.alive {
                audio_events.push(CombatAudioEvent {
                    at: now,
                    bearing_deg: camera_bearing_deg(renderer, enemy.center()),
                    seed: seed ^ 0xDEAD_3400 ^ i as u32,
                    cue: CombatAudioCue::T34Destroyed,
                });
            }
            previous_alive[i] = enemy.alive;
        }
        if previous_tiger_alive && !game.player.alive {
            audio_events.push(CombatAudioEvent {
                at: now,
                bearing_deg: 0.0,
                seed: seed ^ 0xDEAD_8800,
                cue: CombatAudioCue::TigerDestroyed,
            });
        }
        previous_tiger_alive = game.player.alive;
        let hit_delta = game.stat_hits_on_player.saturating_sub(previous_hits);
        let pen_delta = game.stat_pens_on_player.saturating_sub(previous_pens);
        if hit_delta > 0 {
            let source = active
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
                .map(|i| game.enemies[i].center())
                .unwrap_or(game.player.center() + game.player.forward());
            for hit in 0..hit_delta {
                audio_events.push(CombatAudioEvent {
                    at: now + hit as f32 * 0.035,
                    bearing_deg: camera_bearing_deg(renderer, source),
                    seed: seed ^ 0x1A10_0000 ^ game.stat_hits_on_player ^ hit,
                    cue: CombatAudioCue::TigerImpact {
                        penetrated: hit < pen_delta,
                    },
                });
            }
        }
        previous_hits = game.stat_hits_on_player;
        previous_pens = game.stat_pens_on_player;

        if now + DT * 0.5 >= next_frame {
            let rgba = renderer.render_to_rgba(device, queue, renderer.iw, renderer.ih)?;
            stdin
                .write_all(&rgba)
                .context("streaming frame to ffmpeg")?;
            frames += 1;
            interior_frames.push(director.is_interior());
            *camera_frames.entry(director.shot_label()).or_default() += 1;
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
    ensure!(status.success(), "ffmpeg failed while encoding {picture}");
    let duration = frames as f32 / FPS as f32;
    let audio = build_showreel_soundtrack(duration, FPS, &interior_frames, &audio_events);
    acoustics::write_wav_stereo(&soundtrack, &audio, acoustics::SAMPLE_RATE)?;
    mux_showreel(&picture, &soundtrack, &video)?;
    // The first close exterior pass still has every combatant moving and catches the opening
    // crossfire. Late frames can land on an already-empty kill-event point after the final sweep.
    extract_poster(&video, &poster, (duration * 0.15).clamp(4.0, 12.0))?;

    let survivors = active.iter().filter(|&&i| game.enemies[i].alive).count();
    let interior_count = interior_frames.iter().filter(|&&inside| inside).count() as u32;
    let exterior_count = frames.saturating_sub(interior_count);
    let camera_mix = camera_frames
        .iter()
        .map(|(label, count)| format!("{label}:{:.1}s", *count as f32 / FPS as f32))
        .collect::<Vec<_>>()
        .join(", ");
    let result = format!(
        "KESSEL AI SHOWREEL TAKE\n\
         video: {video}\n\
         picture master: {picture}\n\
         soundtrack: {soundtrack}\n\
         resolution: {ow}x{oh}\n\
         frame rate: {FPS} fps\n\
         act: {} ({})\n\
         staging pose: ({:.2}, {:.2}) m, heading {:.2} deg — {}\n\
         shipping atmosphere: fog {:.5}, precipitation {:.2}, overcast {:.2}, storm {:.2}, mood {:.2}\n\
         shipping camera: auto exposure {}, override {:?}, retro {:.2}\n\
         shipping day phase: night {:.2}, twilight {:.2}, sun ({:.3}, {:.3}, {:.3})\n\
         T-34 count: {count}\n\
         seed: {seed:#010x}\n\
         Tiger: {}\n\
         T-34 survivors: {survivors}\n\
         duration: {duration:.2} s\n\
         exterior cameras: {:.2} s\n\
         interior cameras: {:.2} s\n\
         camera mix: {camera_mix}\n\
         audio events: {}\n\
         T-34 policy: {t34_weights}\n\
         Tiger policy: {tiger_weights}\n",
        game.act.id,
        game.act.name,
        staging_center.x,
        staging_center.y,
        staging_heading_deg,
        staging_source,
        atmosphere.fog_density,
        atmosphere.precipitation,
        atmosphere.overcast,
        atmosphere.storm,
        atmosphere.atmosphere_mood,
        atmosphere.autoexp,
        atmosphere.exposure_override,
        atmosphere.master_retro,
        atmosphere.night,
        atmosphere.twilight,
        atmosphere.sun_dir.x,
        atmosphere.sun_dir.y,
        atmosphere.sun_dir.z,
        if game.player.alive {
            "survived"
        } else {
            "destroyed"
        },
        exterior_count as f32 / FPS as f32,
        interior_count as f32 / FPS as f32,
        audio_events.len(),
    );
    std::fs::write(format!("{outdir}/{stem}_take.txt"), result)?;
    println!("[showreel] wrote {video} ({ow}x{oh}, {FPS} fps, CRF 14) + {poster}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_campaign_wave_take(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    game: &mut Game,
    t34_brain: &ai::Mlp,
    tiger_brain: &ai::Mlp,
    outdir: &str,
    max_secs: f32,
    t34_weights: &str,
    tiger_weights: &str,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    let config = CampaignWaveConfig::from_environment()?;
    let opening = config.wave_spec(0);
    let active = stage_campaign_wave(game, t34_brain, opening);
    let mut waves = CampaignWaveSequence::with_config(active, config);

    // The defaults keep the real Act-I weather and camera response. Scoped overrides exist only so
    // renderer A/B captures can vary declared physical coefficients without changing gameplay or
    // silently injecting light in post.
    let act_fog_density =
        configured_showreel_scalar(SHOWREEL_FOG_DENSITY_ENV, renderer.fog_density, 0.0, 0.02)?;
    let showreel_retro = configured_showreel_scalar(SHOWREEL_RETRO_ENV, CAMPAIGN_RETRO, 0.0, 1.0)?;
    renderer.fog_density = act_fog_density;
    renderer.autoexp = true;
    renderer.exposure_override = None;
    renderer.master_retro = showreel_retro;
    renderer.reticle = false;
    let mut pilot = TigerPilot::new(tiger_brain);
    let mut director = CampaignCameraDirector::new();

    const FPS: u32 = 24;
    let preview = std::env::var("KESSEL_SHOWREEL_PREVIEW")
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let (ow, oh) = if preview { (1280, 720) } else { (1920, 1080) };
    let geometry_tag = format!("{:.1}", config.final_geometry_m).replace('.', "p");
    let stem = format!(
        "campaign_wave_{}_seed_{:08x}_{}m",
        config.final_layout.label(),
        config.final_seed,
        geometry_tag
    );
    let picture = format!("{outdir}/{stem}_picture.mp4");
    let soundtrack = format!("{outdir}/{stem}_soundtrack.wav");
    let video = format!("{outdir}/{stem}_reel.mp4");
    let poster = format!("{outdir}/{stem}_poster.png");
    let mut encoder = spawn_encoder(&picture, renderer.iw, renderer.ih, FPS, ow, oh)?;
    let mut stdin = encoder.stdin.take().context("ffmpeg stdin unavailable")?;

    let max_steps = (max_secs / DT).ceil() as usize;
    let mut next_frame = 0.0f32;
    let mut frames = 0u32;
    let mut interior_frames = Vec::new();
    let mut camera_frames: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut audio_events = Vec::new();
    let mut tiger_shot_cursor = ShotSequenceCursor::at(game.main_gun_shot_sequence());
    let mut enemy_shot_cursors = (0..game.enemies.len())
        .map(|i| ShotSequenceCursor::at(game.enemy_main_gun_shot_sequence(i)))
        .collect::<Vec<_>>();
    let mut previous_alive: Vec<bool> = game.enemies.iter().map(|enemy| enemy.alive).collect();
    let mut previous_tiger_alive = game.player.alive;
    let mut previous_hits = game.stat_hits_on_player;
    let mut previous_pens = game.stat_pens_on_player;
    let mut audit = CampaignTakeAudit::default();
    let started = Instant::now();

    for step in 0..max_steps {
        game.debug_engage_all();
        let input = pilot.campaign_input(game, DT);
        game.update(&input, DT, renderer);
        let now = (step + 1) as f32 * DT;
        let tiger_destroyed_this_tick =
            audit.note_authoritative_tiger_state(now, previous_tiger_alive, game.player.alive);
        let staged_new_wave = waves.advance(game, DT);
        if staged_new_wave {
            previous_alive = game.enemies.iter().map(|enemy| enemy.alive).collect();
        }
        let hit_delta = game.stat_hits_on_player.saturating_sub(previous_hits);
        let pen_delta = game.stat_pens_on_player.saturating_sub(previous_pens);
        director.note_interior_impact(hit_delta);
        director.note_fatal_transition(tiger_destroyed_this_tick);
        director.update(
            game,
            &waves,
            renderer,
            DT,
            audit.requires_fatal_full_turret_camera(),
        );
        audit.note_impact(now, director.is_interior(), hit_delta, pen_delta);

        let tiger_interior = director.is_interior();
        collect_shot_sequence_events(
            &mut tiger_shot_cursor,
            game.main_gun_shot_sequence(),
            &mut audio_events,
            |shot_sequence| CombatAudioEvent {
                at: now,
                bearing_deg: 0.0,
                seed: opening.seed ^ 0x8800_0000 ^ shot_sequence,
                cue: CombatAudioCue::TigerGun {
                    interior: tiger_interior,
                },
            },
        );
        for &i in &waves.active {
            let enemy = &game.enemies[i];
            let range_m = enemy.center().distance(renderer.camera.pos);
            let bearing_deg = camera_bearing_deg(renderer, enemy.center());
            collect_shot_sequence_events(
                &mut enemy_shot_cursors[i],
                game.enemy_main_gun_shot_sequence(i),
                &mut audio_events,
                |shot_sequence| CombatAudioEvent {
                    at: now + range_m / 343.0,
                    bearing_deg,
                    seed: waves.current_spec().seed
                        ^ 0x3400_0000
                        ^ ((i as u32) << 16)
                        ^ shot_sequence,
                    cue: CombatAudioCue::T34Gun { range_m },
                },
            );
            if previous_alive.get(i).copied().unwrap_or(false) && !enemy.alive {
                audio_events.push(CombatAudioEvent {
                    at: now,
                    bearing_deg: camera_bearing_deg(renderer, enemy.center()),
                    seed: waves.current_spec().seed ^ 0xDEAD_3400 ^ i as u32,
                    cue: CombatAudioCue::T34Destroyed,
                });
            }
            if let Some(alive) = previous_alive.get_mut(i) {
                *alive = enemy.alive;
            }
        }
        if previous_tiger_alive && !game.player.alive {
            audio_events.push(CombatAudioEvent {
                at: now,
                bearing_deg: 0.0,
                seed: waves.current_spec().seed ^ 0xDEAD_8800,
                cue: CombatAudioCue::TigerDestroyed,
            });
        }
        previous_tiger_alive = game.player.alive;

        if hit_delta > 0 {
            let source = waves
                .active
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
                .map(|i| game.enemies[i].center())
                .unwrap_or(game.player.center() + game.player.forward());
            for hit in 0..hit_delta {
                audio_events.push(CombatAudioEvent {
                    at: now + hit as f32 * 0.035,
                    bearing_deg: camera_bearing_deg(renderer, source),
                    seed: waves.current_spec().seed ^ 0x1A10_0000 ^ game.stat_hits_on_player ^ hit,
                    cue: CombatAudioCue::TigerImpact {
                        penetrated: hit < pen_delta,
                    },
                });
            }
        }
        previous_hits = game.stat_hits_on_player;
        previous_pens = game.stat_pens_on_player;

        if now + DT * 0.5 >= next_frame {
            let rgba = renderer.render_to_rgba(device, queue, renderer.iw, renderer.ih)?;
            stdin
                .write_all(&rgba)
                .context("streaming campaign-wave frame to ffmpeg")?;
            audit.note_rendered_camera_frame(now, director.is_interior(), 1.0 / FPS as f32);
            frames += 1;
            interior_frames.push(director.is_interior());
            *camera_frames.entry(director.shot_label()).or_default() += 1;
            next_frame += 1.0 / FPS as f32;
            if frames % (FPS * 5) == 0 {
                println!(
                    "[campaign-wave] encoded {:>5.1}s / {:>5.1}s — wave {} ({} T-34), {:?}  (wall {:.1}s)",
                    frames as f32 / FPS as f32,
                    now,
                    waves.wave + 1,
                    waves.current_spec().count,
                    waves.phase,
                    started.elapsed().as_secs_f32()
                );
            }
        }

        if waves.finished_postroll()
            || (waves.phase == CampaignWavePhase::FinalCleared && waves.phase_age >= 2.0)
        {
            break;
        }
    }
    drop(stdin);
    let status = encoder.wait()?;
    ensure!(status.success(), "ffmpeg failed while encoding {picture}");
    let duration = frames as f32 / FPS as f32;
    let audio = build_showreel_soundtrack(duration, FPS, &interior_frames, &audio_events);
    acoustics::write_wav_stereo(&soundtrack, &audio, acoustics::SAMPLE_RATE)?;
    mux_showreel(&picture, &soundtrack, &video)?;
    extract_poster(&video, &poster, duration.min(8.0).max(1.0))?;

    let interior_count = interior_frames.iter().filter(|&&inside| inside).count() as u32;
    let exterior_count = frames.saturating_sub(interior_count);
    let camera_mix = camera_frames
        .iter()
        .map(|(label, count)| format!("{label}:{:.1}s", *count as f32 / FPS as f32))
        .collect::<Vec<_>>()
        .join(", ");
    let wave_timeline = (0..CAMPAIGN_WAVES.len())
        .filter_map(|index| {
            let spec = waves.config.wave_spec(index);
            let start = waves.starts.get(index).copied()?;
            let end = waves
                .ends
                .get(index)
                .and_then(|end| *end)
                .unwrap_or(duration);
            Some(format!(
                "wave {}: {} T-34, seed {:#010x}, {:.2}-{:.2}s",
                index + 1,
                spec.count,
                spec.seed,
                start,
                end
            ))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let interior_impact_timeline = audit
        .distinct_interior_impact_times
        .iter()
        .map(|at| format!("{at:.2}s"))
        .collect::<Vec<_>>()
        .join(", ");
    let fatal_at = audit
        .fatal_capture
        .death_at()
        .map_or_else(|| "none".to_owned(), |at| format!("{at:.2}s"));
    let fatal_interior_at = audit
        .fatal_capture
        .first_interior_frame_at()
        .map_or_else(|| "none".to_owned(), |at| format!("{at:.2}s"));
    let escalation_ok = waves.completed_required_escalation();
    let impacts_ok = audit.has_required_interior_impacts();
    let fatal_hold_ok = audit.has_required_fatal_hold();
    let accepted = escalation_ok && impacts_ok && fatal_hold_ok && !game.player.alive;
    let result = format!(
        "KESSEL ACT-I CAMPAIGN WAVE REEL\n\
         video: {video}\n\
         picture master: {picture}\n\
         soundtrack: {soundtrack}\n\
         audio source: deterministic spatial film mix (runtime audio sink muted; not a live capture)\n\
         location: Act I procedural village street crossing ({:.0}, {:.0})\n\
         resolution: {ow}x{oh}\n\
         frame rate: {FPS} fps\n\
         duration: {duration:.2} s\n\
         Tiger: {}\n\
         authoritative couch crew: {} living / 4 at end\n\
         final seed: {:#010x}\n\
         final layout: {} at {:.1} m\n\
         death wave: {}\n\
         Act-I fog density: {act_fog_density:.5}\n\
         camera exposure: measured HDR eye adaptation (continuous across camera cuts)\n\
         retro response: {showreel_retro:.3}\n\
         exterior cameras: {:.2} s\n\
         interior cameras: {:.2} s\n\
         interior hits held on camera: {}\n\
         interior penetrations held on camera: {}\n\
         distinct interior impact timestamps (>=1.25 s apart): {} [{}]\n\
         fatal event timestamp: {fatal_at}\n\
         first encoded fatal interior frame: {fatal_interior_at}\n\
         fatal interior hold: {:.2} s\n\
         camera mix: {camera_mix}\n\
         audio events: {}\n\
         acceptance: {}\n\
         T-34 policy: {t34_weights}\n\
         Tiger policy: {tiger_weights}\n\
         {wave_timeline}\n",
        CAMPAIGN_VILLAGE_STAGING.x,
        CAMPAIGN_VILLAGE_STAGING.y,
        if game.player.alive {
            "survived"
        } else {
            "destroyed"
        },
        game.couch_living_crew(),
        waves.config.final_seed,
        waves.config.final_layout.label(),
        waves.config.final_geometry_m,
        waves
            .death_wave
            .map_or_else(|| "none".to_owned(), |wave| (wave + 1).to_string()),
        exterior_count as f32 / FPS as f32,
        interior_count as f32 / FPS as f32,
        audit.interior_hit_count,
        audit.interior_penetration_count,
        audit.distinct_interior_impact_times.len(),
        interior_impact_timeline,
        audit.fatal_capture.maximum_interior_hold(),
        audio_events.len(),
        if accepted { "PASS" } else { "REJECT" },
    );
    std::fs::write(format!("{outdir}/{stem}_take.txt"), result)?;

    ensure!(
        escalation_ok && !game.player.alive,
        "campaign-wave take did not start 1→2→6 and destroy the Tiger in wave 3 (starts: {}, death wave: {:?}); rejected take is in {outdir}",
        waves.starts.len(),
        waves.death_wave,
    );
    ensure!(
        impacts_ok,
        "campaign-wave take held only {} distinct interior impact timestamp(s), requiring two at least {CAMPAIGN_DISTINCT_INTERIOR_IMPACT_GAP:.2}s apart",
        audit.distinct_interior_impact_times.len(),
    );
    ensure!(
        fatal_hold_ok,
        "fatal event was not held continuously on an interior camera for {CAMPAIGN_FATAL_INTERIOR_HOLD:.2}s (death {:?}, max hold {:.2}s)",
        audit.fatal_capture.death_at(),
        audit.fatal_capture.maximum_interior_hold(),
    );
    println!(
        "[campaign-wave] wrote {video} ({ow}x{oh}, {FPS} fps) — {} distinct interior impacts / {:.2}s fatal hold",
        audit.distinct_interior_impact_times.len(),
        audit.fatal_capture.maximum_interior_hold(),
    );
    Ok(())
}

#[derive(Clone, Debug)]
struct CampaignPreflightReport {
    config: CampaignWaveConfig,
    seconds: f32,
    starts: Vec<f32>,
    death_wave: Option<usize>,
    tiger_alive: bool,
    living_crew: usize,
    hits_by_wave: [u32; 3],
    pens_by_wave: [u32; 3],
    t34_survivors: usize,
    final_start_tiger: Option<Vec2>,
    final_phase: CampaignWavePhase,
}

impl CampaignPreflightReport {
    fn accepted(&self) -> bool {
        self.starts.len() == CAMPAIGN_WAVES.len()
            && self.death_wave == Some(CAMPAIGN_WAVES.len() - 1)
            && !self.tiger_alive
            && self.pens_by_wave[CAMPAIGN_WAVES.len() - 1] >= 2
    }

    fn total_hits(&self) -> u32 {
        self.hits_by_wave.iter().sum()
    }

    fn total_pens(&self) -> u32 {
        self.pens_by_wave.iter().sum()
    }
}

fn simulate_campaign_wave_preflight(
    game: &mut Game,
    renderer: &mut Renderer,
    t34_brain: &ai::Mlp,
    tiger_brain: &ai::Mlp,
    config: CampaignWaveConfig,
    max_secs: f32,
) -> CampaignPreflightReport {
    let opening = config.wave_spec(0);
    let active = stage_campaign_wave(game, t34_brain, opening);
    let mut waves = CampaignWaveSequence::with_config(active, config);
    let mut pilot = TigerPilot::new(tiger_brain);
    let mut hits_by_wave = [0u32; CAMPAIGN_WAVES.len()];
    let mut pens_by_wave = [0u32; CAMPAIGN_WAVES.len()];
    let mut previous_hits = game.stat_hits_on_player;
    let mut previous_pens = game.stat_pens_on_player;
    let mut final_start_tiger = None;

    for _step in 0..(max_secs / DT).ceil() as usize {
        let combat_wave = waves.wave.min(CAMPAIGN_WAVES.len() - 1);
        game.debug_engage_all();
        let input = pilot.campaign_input(game, DT);
        game.update(&input, DT, renderer);
        hits_by_wave[combat_wave] = hits_by_wave[combat_wave]
            .saturating_add(game.stat_hits_on_player.saturating_sub(previous_hits));
        pens_by_wave[combat_wave] = pens_by_wave[combat_wave]
            .saturating_add(game.stat_pens_on_player.saturating_sub(previous_pens));
        previous_hits = game.stat_hits_on_player;
        previous_pens = game.stat_pens_on_player;

        if waves.advance(game, DT) && waves.wave == CAMPAIGN_WAVES.len() - 1 {
            final_start_tiger = Some(Vec2::new(game.player.pos.x, game.player.pos.z));
        }
        if matches!(
            waves.phase,
            CampaignWavePhase::Finished | CampaignWavePhase::FinalCleared
        ) {
            break;
        }
    }

    let t34_survivors = waves
        .active
        .iter()
        .filter(|&&index| game.enemies[index].alive)
        .count();
    let report = CampaignPreflightReport {
        config,
        seconds: waves.total_age,
        starts: waves.starts.clone(),
        death_wave: waves.death_wave,
        tiger_alive: game.player.alive,
        living_crew: game.couch_living_crew(),
        hits_by_wave,
        pens_by_wave,
        t34_survivors,
        final_start_tiger,
        final_phase: waves.phase,
    };
    game.enemy_policy = None;
    report
}

fn print_campaign_preflight(report: &CampaignPreflightReport) {
    println!(
        "[campaign-preflight] {} seed={:#010x} geometry={:.1}m — {} in {:.2}s (crew {}/4, T-34 survivors {}, phase {:?})",
        report.config.final_layout.label(),
        report.config.final_seed,
        report.config.final_geometry_m,
        if report.tiger_alive {
            "Tiger survived"
        } else {
            "Tiger destroyed"
        },
        report.seconds,
        report.living_crew,
        report.t34_survivors,
        report.final_phase,
    );
    println!(
        "[campaign-preflight] starts={:?}, death_wave={}, hits={} {:?}, pens={} {:?}, final_start={}",
        report.starts,
        report
            .death_wave
            .map_or_else(|| "none".to_owned(), |wave| (wave + 1).to_string()),
        report.total_hits(),
        report.hits_by_wave,
        report.total_pens(),
        report.pens_by_wave,
        report.final_start_tiger.map_or_else(
            || "none".to_owned(),
            |point| format!("({:.1},{:.1})", point.x, point.y)
        ),
    );
    println!(
        "[campaign-preflight] {} — no video frames or render submissions",
        if report.accepted() { "PASS" } else { "REJECT" }
    );
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
        "[showreel] direct take: {} — Act {act} ({}) manifest spawn — {count} T-34s, seed {seed:#010x}",
        adapter.get_info().name,
        crate::world::act(act).name,
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

/// Run the exact four-body 1→2→6 campaign sequence without encoding or submitting a render frame.
/// The final seed/layout/radius are explicit diagnostic inputs; everything after muster remains the
/// shipping enemy AI, vehicle collision, armor, penetration, spall and couch-casualty simulation.
pub fn run_ai_campaign_wave_preflight(
    t34_weights: &str,
    tiger_weights: &str,
    max_secs: f32,
    final_seed: u32,
    final_layout: &str,
    final_geometry_m: f32,
) -> anyhow::Result<()> {
    ensure!(
        max_secs >= 30.0,
        "campaign-wave preflight needs at least 30 seconds"
    );
    ensure!(
        (35.0..=320.0).contains(&final_geometry_m),
        "campaign final geometry must be 35..=320 m"
    );
    let config = CampaignWaveConfig {
        final_seed,
        final_layout: CampaignFinalLayout::parse(final_layout)?,
        final_geometry_m,
    };
    let t34_brain =
        ai::Mlp::load(t34_weights).with_context(|| format!("loading T-34 policy {t34_weights}"))?;
    let tiger_brain = ai::Mlp::load(tiger_weights)
        .with_context(|| format!("loading Tiger policy {tiger_weights}"))?;
    let instance = super::create_instance();
    let adapter = super::request_adapter(&instance, None);
    println!(
        "[campaign-preflight] adapter: {} — Act I / 1→2→6, render disabled",
        adapter.get_info().name
    );
    let (device, queue) = super::request_device(&adapter);
    let (mut renderer, mut game) = Game::build_act(&device, &queue, false, &crate::world::act(1))?;
    game.mute_audio();
    game.set_defer_rtt_to_render(true);
    let report = simulate_campaign_wave_preflight(
        &mut game,
        &mut renderer,
        &t34_brain,
        &tiger_brain,
        config,
        max_secs,
    );
    print_campaign_preflight(&report);
    ensure!(
        report.accepted(),
        "campaign preflight rejected: need all 1→2→6 starts, Tiger death in wave 3, and at least two wave-3 penetrations"
    );
    Ok(())
}

/// Record the authored Act-I Springbrunnen escalation: one T-34, then two, then the deterministic
/// six-tank generation-101 kill wave. This is intentionally a separate path so the analytical
/// `ai-showreel` and direct `ai-take` contracts remain byte-for-byte compatible with existing tools.
pub fn run_ai_campaign_wave_take(
    outdir: &str,
    t34_weights: &str,
    tiger_weights: &str,
    max_secs: f32,
) -> anyhow::Result<()> {
    ensure!(
        max_secs >= 30.0,
        "campaign-wave take needs at least 30 seconds"
    );
    let t34_brain =
        ai::Mlp::load(t34_weights).with_context(|| format!("loading T-34 policy {t34_weights}"))?;
    let tiger_brain = ai::Mlp::load(tiger_weights)
        .with_context(|| format!("loading Tiger policy {tiger_weights}"))?;
    let instance = super::create_instance();
    let adapter = super::request_adapter(&instance, None);
    println!(
        "[campaign-wave] adapter: {} — Act I / Springbrunnen — 1→2→6 T-34",
        adapter.get_info().name
    );
    let (device, queue) = super::request_device(&adapter);
    let (mut renderer, mut game) = Game::build_act(&device, &queue, false, &crate::world::act(1))?;
    game.mute_audio();
    record_campaign_wave_take(
        &device,
        &queue,
        &mut renderer,
        &mut game,
        &t34_brain,
        &tiger_brain,
        outdir,
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
    fn campaign_wave_climax_uses_the_diagnostic_crossfire_defaults() {
        let config = CampaignWaveConfig::default();
        let climax = config.wave_spec(2);
        assert_eq!(climax.count, 6);
        assert_eq!(climax.seed, 0x34A0_0002);
        assert_eq!(climax.ring_m, CAMPAIGN_FINAL_CROSSFIRE_M);
        assert_eq!(config.final_layout, CampaignFinalLayout::StreetCrossfire);
        assert_eq!(config.wave_spec(0).ring_m, GAUNTLET_RING_M);
        assert_eq!(config.wave_spec(1).ring_m, GAUNTLET_RING_M);
    }

    #[test]
    fn showreel_renderer_controls_accept_only_finite_bounded_scalars() {
        assert_eq!(
            parse_showreel_scalar(SHOWREEL_FOG_DENSITY_ENV, "0.0015", 0.0, 0.02)
                .unwrap()
                .to_bits(),
            0.0015f32.to_bits()
        );
        assert!(parse_showreel_scalar(SHOWREEL_FOG_DENSITY_ENV, "-0.1", 0.0, 0.02).is_err());
        assert!(parse_showreel_scalar(SHOWREEL_RETRO_ENV, "NaN", 0.0, 1.0).is_err());
        assert!(parse_showreel_scalar(SHOWREEL_RETRO_ENV, "1.01", 0.0, 1.0).is_err());
        assert_eq!(
            parse_showreel_seed(SHOWREEL_FINAL_SEED_ENV, "0x34a00005").unwrap(),
            0x34a0_0005
        );
        assert_eq!(
            parse_showreel_seed(SHOWREEL_FINAL_SEED_ENV, "883949573").unwrap(),
            0x34b0_0005
        );
        assert!(parse_showreel_seed(SHOWREEL_FINAL_SEED_ENV, "not-a-seed").is_err());
        assert_eq!(
            CampaignFinalLayout::parse("ring").unwrap(),
            CampaignFinalLayout::Ring
        );
        assert!(CampaignFinalLayout::parse("ambush").is_err());
        assert_eq!(
            parse_showreel_scalar(SHOWREEL_FINAL_GEOMETRY_ENV, "118", 35.0, 320.0)
                .unwrap()
                .to_bits(),
            118.0f32.to_bits()
        );
        assert!(parse_showreel_scalar(SHOWREEL_FINAL_GEOMETRY_ENV, "321", 35.0, 320.0).is_err());
    }

    #[test]
    fn campaign_wave_is_back_inside_before_the_audited_fatal_event() {
        let mut waves = CampaignWaveSequence::new(Vec::new());
        waves.wave = CAMPAIGN_WAVES.len() - 1;

        waves.phase_age = CAMPAIGN_FINAL_FATAL_INTERIOR_START - 0.01;
        assert_eq!(waves.combat_shot(), CampaignShot::VillageWide);
        waves.phase_age = CAMPAIGN_FINAL_FATAL_INTERIOR_START;
        assert_eq!(waves.combat_shot(), CampaignShot::TurretWide);
        waves.phase_age = 37.58;
        assert_eq!(waves.combat_shot(), CampaignShot::TurretWide);

        let mut director = CampaignCameraDirector::new();
        director.shot = CampaignShot::TurretWide;
        director.note_interior_impact(1);
        assert!(director.interior_impact_hold >= 2.25);
        assert_eq!(
            director.desired_shot(&waves, false),
            CampaignShot::TurretWide
        );

        // The chosen alternate fight kills the Tiger before the old 30 s final-wave cut. A real
        // living-to-destroyed edge must therefore override an exterior combat shot and latch the
        // full-turret witness angle; this changes only the camera, never gameplay state.
        director.shot = CampaignShot::VillageWide;
        director.note_fatal_transition(true);
        assert_eq!(
            director.desired_shot(&waves, true),
            CampaignShot::TurretWide
        );
        assert_eq!(director.death_hold_shot, Some(CampaignShot::TurretWide));
    }

    #[test]
    fn campaign_camera_never_authors_exposure_per_shot() {
        let source = include_str!("showreel.rs");
        let director = source
            .split("impl CampaignCameraDirector")
            .nth(1)
            .expect("camera director implementation")
            .split("fn representative")
            .next()
            .expect("camera director body");
        assert!(!director.contains("exposure_override = Some"));
        assert!(director.matches("exposure_override = None").count() >= 2);
    }

    #[test]
    fn final_crossfire_occupies_streets_across_front_sides_and_rear() {
        let anchor = CAMPAIGN_VILLAGE_STAGING;
        let forward = Vec2::Y;
        let right = Vec2::X;
        let positions =
            campaign_crossfire_positions(anchor, forward, 0x34A0_0002, CAMPAIGN_FINAL_CROSSFIRE_M);
        assert_eq!(positions.len(), 6);
        let mut front = 0;
        let mut rear = 0;
        let mut left = 0;
        let mut right_count = 0;
        for point in positions {
            let on_vertical = ACT1_STREET_X.contains(&point.x);
            let on_horizontal = ACT1_STREET_Z.contains(&point.y);
            assert!(
                on_vertical || on_horizontal,
                "{point:?} must sit on a street lane"
            );
            let offset = (point - anchor).normalize();
            front += usize::from(offset.dot(forward) > 0.55);
            rear += usize::from(offset.dot(forward) < -0.55);
            left += usize::from(offset.dot(right) < -0.55);
            right_count += usize::from(offset.dot(right) > 0.55);
        }
        assert!(front >= 1 && rear >= 1);
        assert!(left >= 1 && right_count >= 1);
    }

    #[test]
    fn preflight_acceptance_requires_two_final_wave_pens() {
        let mut report = CampaignPreflightReport {
            config: CampaignWaveConfig::default(),
            seconds: 72.0,
            starts: vec![0.0, 12.0, 28.0],
            death_wave: Some(2),
            tiger_alive: false,
            living_crew: 0,
            hits_by_wave: [1, 2, 8],
            pens_by_wave: [0, 0, 2],
            t34_survivors: 4,
            final_start_tiger: Some(CAMPAIGN_VILLAGE_STAGING),
            final_phase: CampaignWavePhase::Finished,
        };
        assert!(report.accepted());
        report.pens_by_wave[2] = 1;
        assert!(!report.accepted());
    }

    #[test]
    fn campaign_wave_stages_on_the_dense_act_one_street_crossing() {
        let procedural_village_center = Vec2::new(-30.0, 110.0);
        assert!(CAMPAIGN_VILLAGE_STAGING.distance(procedural_village_center) < 25.0);
        assert!(CAMPAIGN_VILLAGE_STAGING.distance(Vec2::new(210.0, -210.0)) > 300.0);
    }

    #[test]
    fn campaign_frame_has_four_stable_bodies_at_the_correct_posts() {
        let frame = campaign_crew_frame(0.6, -0.3, true, true, true);
        assert_eq!(frame.human_count(), 4);
        for role in [
            LanRole::Commander,
            LanRole::Driver,
            LanRole::Gunner,
            LanRole::Loader,
        ] {
            assert!(frame.occupied(role));
            assert_eq!(frame.input(role).post, role);
        }
        assert!(!frame.occupied(LanRole::Radio));
        assert_eq!(frame.input(LanRole::Driver).drive, 0.6);
        assert_eq!(frame.input(LanRole::Driver).steer, -0.3);
        assert!(frame.input(LanRole::Gunner).flag(F_FIRE_MAIN));
        assert!(frame.input(LanRole::Loader).flag(F_SERVICE_BREECH));
        assert!(frame.input(LanRole::Commander).flag(F_EXTINGUISH));
    }

    #[test]
    fn campaign_acceptance_requires_wave_three_death_after_all_starts() {
        let mut waves = CampaignWaveSequence::new(Vec::new());
        waves.wave = CAMPAIGN_WAVES.len() - 1;
        waves.starts = vec![0.0, 10.0, 20.0];
        waves.ends = vec![Some(8.0), Some(18.0), Some(32.0)];
        waves.phase = CampaignWavePhase::Finished;
        waves.death_wave = Some(CAMPAIGN_WAVES.len() - 1);
        assert!(waves.completed_required_escalation());

        waves.death_wave = Some(1);
        assert!(!waves.completed_required_escalation());
        waves.death_wave = Some(CAMPAIGN_WAVES.len() - 1);
        waves.starts.pop();
        assert!(!waves.completed_required_escalation());
    }

    #[test]
    fn campaign_audit_requires_separated_impacts_and_two_second_fatal_hold() {
        let mut audit = CampaignTakeAudit::default();
        audit.note_impact(4.0, true, 3, 1);
        audit.note_impact(4.9, true, 1, 1);
        assert!(!audit.has_required_interior_impacts());
        audit.note_impact(5.25, true, 1, 0);
        assert!(audit.has_required_interior_impacts());
        assert_eq!(audit.distinct_interior_impact_times, vec![4.0, 5.25]);
        assert_eq!(audit.interior_hit_count, 5);
        assert_eq!(audit.interior_penetration_count, 2);

        assert!(audit.note_authoritative_tiger_state(9.0, true, false));
        assert!(audit.requires_fatal_full_turret_camera());
        for frame in 0..48 {
            audit.note_rendered_camera_frame(9.0 + frame as f32 / 24.0, true, 1.0 / 24.0);
        }
        assert!(audit.has_required_fatal_hold());
        assert!(!audit.requires_fatal_full_turret_camera());
        assert_eq!(audit.fatal_capture.death_at(), Some(9.0));

        let mut exterior_death = CampaignTakeAudit::default();
        assert!(exterior_death.note_authoritative_tiger_state(9.0, true, false));
        exterior_death.note_rendered_camera_frame(9.0, false, 1.0 / 24.0);
        for frame in 0..47 {
            exterior_death.note_rendered_camera_frame(
                9.0 + (frame + 1) as f32 / 24.0,
                true,
                1.0 / 24.0,
            );
        }
        assert!(!exterior_death.has_required_fatal_hold());
        assert!(exterior_death.requires_fatal_full_turret_camera());
    }

    #[test]
    fn shot_audio_collects_every_authoritative_sequence_exactly_once() {
        let mut cursor = ShotSequenceCursor::at(40);
        let mut events = Vec::new();
        assert_eq!(
            collect_shot_sequence_events(&mut cursor, 43, &mut events, |sequence| sequence),
            3
        );
        assert_eq!(events, vec![41, 42, 43]);
        assert_eq!(
            collect_shot_sequence_events(&mut cursor, 43, &mut events, |sequence| sequence),
            0
        );
        assert_eq!(
            events.len(),
            3,
            "a repeated film tick must not duplicate reports"
        );

        let mut wrapping = ShotSequenceCursor::at(u32::MAX);
        let mut wrapped_events = Vec::new();
        assert_eq!(
            collect_shot_sequence_events(&mut wrapping, 0, &mut wrapped_events, |sequence| {
                sequence
            },),
            1
        );
        assert_eq!(wrapped_events, vec![0]);
    }

    #[test]
    fn showreel_tiger_impact_is_exactly_the_runtime_body_voice() {
        let event = CombatAudioEvent {
            at: 2.0,
            bearing_deg: -73.0,
            seed: 0xA8D1_0091,
            cue: CombatAudioCue::TigerImpact { penetrated: true },
        };
        let actual = synth_combat_event(event);
        let (mut params, context) = acoustics::impact_signature_params(
            ImpactSignature::CloseT34SidePenetration,
            event.seed,
        );
        params.bearing_deg = event.bearing_deg;
        let runtime = acoustics::synth_damage_impact_body(params, context);
        let offline_full = acoustics::synth_damage_event(params, context);
        assert_eq!(
            actual, runtime.main,
            "headless film diverged from live Tiger-impact synthesis"
        );
        assert_ne!(
            actual, offline_full.main,
            "film baked the separately-authored fragment layer into the armor impact"
        );
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
