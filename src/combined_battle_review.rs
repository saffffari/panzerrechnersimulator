//! A staged deployment, followed by one uninterrupted, mortal Game simulation. This is a
//! recording adapter, not another combat model. No casualties, firing results or reinforcements
//! are injected. The existing roster has faction semantics, not free-for-all semantics.
use super::*;
use crate::infantry_combat::{Combatant, CoverSet, Team};
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    time::Instant,
};

const HZ: u32 = 120;
const FPS: u32 = 24;
const STEP: f32 = 1.0 / HZ as f32;
const CUT_FRAMES: u64 = FPS as u64 * 6;

fn audio_frames(tick: u64) -> usize {
    let rate = u64::from(crate::acoustics::SAMPLE_RATE);
    (((tick + 1) * rate / u64::from(HZ)) - tick * rate / u64::from(HZ)) as usize
}

fn load_policy(path: &str) -> anyhow::Result<(crate::ai::Mlp, Value)> {
    let path = crate::resource::resolve(path)?;
    let bytes = std::fs::read(&path)?;
    let net = crate::ai::Mlp::load(path.to_str().context("non-UTF8 policy path")?)?;
    ensure!(
        net.w.iter().all(|x| x.is_finite()),
        "nonfinite policy weights"
    );
    ensure!(
        bytes.len() == 24 + crate::ai::GENOME * 4,
        "unexpected policy artifact length"
    );
    let genome_id = u32::from_le_bytes(bytes[16..20].try_into()?);
    let fitness = f32::from_le_bytes(bytes[20..24].try_into()?);
    Ok((
        net,
        json!({"path":path,"sha256":crate::audio::sha256_hex(&bytes),
        "bytes":bytes.len(),"format":"KSL1","shape":[crate::ai::OBS,crate::ai::HID,crate::ai::ACT],
        "genome_id":genome_id,"stored_fitness":fitness,"fallback":false}),
    ))
}

/// A bounded deployment search, using the same conservative source-derived cover index as
/// infantry navigation. It does not replace vehicle contact once the battle starts.
fn deployment(g: &Game, wanted: Vec2, radius: f32, used: &[(Vec3, f32)]) -> anyhow::Result<Vec3> {
    let cover = g.world_cover(Vec::new());
    for ring in 0..=12 {
        for side in 0..if ring == 0 { 1 } else { 16 } {
            let angle = side as f32 * std::f32::consts::TAU / 16.0;
            let p = wanted + Vec2::new(angle.cos(), angle.sin()) * ring as f32 * 1.5;
            let y = g.hf.height_at(p.x, p.y);
            let at = Vec3::new(p.x, y, p.y);
            if !at.is_finite() || used.iter().any(|(q, r)| at.distance(*q) < radius + r + 0.8) {
                continue;
            }
            let supported = [-radius, radius].into_iter().all(|x| {
                [-radius, radius]
                    .into_iter()
                    .all(|z| (g.hf.height_at(p.x + x, p.y + z) - y).abs() < 0.65)
            });
            if supported
                && !cover.occupies(
                    at,
                    radius,
                    y + 0.08,
                    y + if radius > 1.0 { 3.2 } else { 1.8 },
                )
            {
                return Ok(at);
            }
        }
    }
    anyhow::bail!("no clear supported deployment within 18m of {wanted:?}")
}

fn stage(
    g: &mut Game,
    seed: u32,
    t34: &crate::ai::Mlp,
    tiger: &crate::ai::Mlp,
    tiger_source: &str,
) -> anyhow::Result<()> {
    g.reset_match(seed, 180.0); // Before the film only; never called during combat.
    ensure!(
        g.enemies.len() == 6 && g.allies.len() == 2,
        "combined battle needs six enemy and two ally rigs"
    );
    ensure!(
        g.soldiers.len() == 28 && g.soviet_soldier_count == 24,
        "unexpected infantry roster"
    );
    g.god = false;
    g.brawl = false; // Parity brawl disables mixed tank/infantry targeting; it is not this scenario.
    g.targets_only = false;
    g.set_playtest_return_fire(true);
    g.enemy_policy = Some(t34.clone());
    let mut used = Vec::new();
    let p = deployment(g, Vec2::new(0.0, -58.0), 4.5, &used)?;
    g.player = Tank::new(p.x, p.z, 0.0, &g.hf, tank::TIGER_PARTS);
    g.spawn_xz = (p.x, p.z);
    used.push((p, 4.5));
    g.enable_platoon();
    for i in 0..2 {
        let p = deployment(
            g,
            Vec2::new(if i == 0 { -30.0 } else { 30.0 }, -55.0),
            4.5,
            &used,
        )?;
        g.allies[i].tank = Tank::new(p.x, p.z, 0.0, &g.hf, tank::TIGER_PARTS);
        used.push((p, 4.5));
    }
    let platoon = g.platoon.as_mut().context("platoon activation failed")?;
    platoon.policy = Some(tiger.clone());
    platoon.policy_source = tiger_source.into();
    platoon.placed = true; // Initial positions above replace only the ordinary opening formation.
    for wingman in &mut platoon.wingmen {
        wingman.standing = platoon::Standing::Advance(Vec2::new(0.0, 55.0));
    }
    for i in 0..6 {
        let p = deployment(
            g,
            Vec2::new((i % 3) as f32 * 31.0 - 31.0, 50.0 + (i / 3) as f32 * 23.0),
            4.0,
            &used,
        )?;
        let direction = g.player.pos - p;
        let mut enemy = Tank::new(
            p.x,
            p.z,
            direction.x.atan2(direction.z),
            &g.hf,
            tank::T34_PARTS,
        );
        enemy.kind = EnemyKind::T34;
        enemy.reload = ENEMY_RELOAD * (i as f32 + 0.5) / 6.0;
        g.enemies[i] = enemy;
        g.enemy_active[i] = true;
        used.push((p, 4.0));
    }
    // Clear initial artificial contact from reset/setup; everyone must now see/hear the enemy.
    g.stealth = stealth::Stealth::new(g.enemies.len());
    g.mixed_combat.enabled = true;
    for i in 0..g.soldiers.len() {
        let german = i >= g.soviet_soldier_count;
        let k = if german {
            i - g.soviet_soldier_count
        } else {
            i
        };
        let wanted = if german {
            Vec2::new(k as f32 * 6.0 - 9.0, -16.0)
        } else {
            Vec2::new((k % 6) as f32 * 7.0 - 17.5, 15.0 + (k / 6) as f32 * 6.0)
        };
        let p = deployment(g, wanted, 0.45, &used)?;
        let yaw = if german { std::f32::consts::PI } else { 0.0 }; // infantry forward is -Z
        let goal = Vec3::new(p.x, g.hf.height_at(p.x, -18.0), -18.0);
        let old = &g.soldiers[i];
        g.soldiers[i] = Soldier::new(
            p,
            yaw,
            [goal; 4],
            seed.wrapping_add(i as u32),
            old.body_slot,
            old.rifle_slot,
        );
        g.mixed_combat.actors[i] = Combatant::new(
            i as u32,
            if german { Team::German } else { Team::Soviet },
            p,
            yaw,
        );
        used.push((p, 0.45));
    }
    g.mixed_combat.squads = (0..24)
        .step_by(6)
        .map(|first| infantry_squad::Squad {
            members: (first..first + 6).collect(),
        })
        .chain(std::iter::once(infantry_squad::Squad {
            members: (24..28).collect(),
        }))
        .collect();
    g.enable_singleplayer_crew();
    Ok(())
}

#[derive(Clone, Copy)]
struct Subject {
    id: usize,
    team: usize,
    infantry: bool,
    pos: Vec3,
    forward: Vec3,
}
fn subjects(g: &Game) -> Vec<Subject> {
    let mut out = Vec::new();
    if g.player.alive {
        out.push(Subject {
            id: 0,
            team: 0,
            infantry: false,
            pos: g.player.center(),
            forward: g.player.forward(),
        });
    }
    for (i, a) in g.allies.iter().enumerate().filter(|(_, a)| a.tank.alive) {
        out.push(Subject {
            id: i + 1,
            team: 0,
            infantry: false,
            pos: a.tank.center(),
            forward: a.tank.forward(),
        });
    }
    for (i, e) in g
        .enemies
        .iter()
        .enumerate()
        .filter(|(i, e)| g.enemy_active[*i] && e.alive)
    {
        out.push(Subject {
            id: i + 3,
            team: 1,
            infantry: false,
            pos: e.center(),
            forward: e.forward(),
        });
    }
    for (i, a) in g
        .mixed_combat
        .actors
        .iter()
        .enumerate()
        .filter(|(_, a)| a.alive())
    {
        out.push(Subject {
            id: i + 9,
            team: usize::from(a.team == Team::Soviet),
            infantry: true,
            pos: a.pos + Vec3::Y * 0.9,
            forward: g.soldiers[i].forward(),
        });
    }
    out
}
fn counts(g: &Game) -> [usize; 4] {
    [
        usize::from(g.player.alive) + g.allies.iter().filter(|a| a.tank.alive).count(),
        g.enemies
            .iter()
            .enumerate()
            .filter(|(i, e)| g.enemy_active[*i] && e.alive)
            .count(),
        g.mixed_combat
            .actors
            .iter()
            .filter(|a| a.team == Team::German && a.alive())
            .count(),
        g.mixed_combat
            .actors
            .iter()
            .filter(|a| a.team == Team::Soviet && a.alive())
            .count(),
    ]
}
fn outcome(c: [usize; 4]) -> Option<&'static str> {
    match (c[0] + c[2] > 0, c[1] + c[3] > 0) {
        (true, false) => Some("german_combat_roster_remaining"),
        (false, true) => Some("soviet_combat_roster_remaining"),
        (false, false) => Some("both_combat_rosters_eliminated"),
        _ => None,
    }
}
fn ledger(g: &Game) -> Value {
    json!({"alive_counts_tiger_t34_german_rifle_soviet_rifle":counts(g),
        "player":{"alive":g.player.alive,"hp":g.player.hp,"position":g.player.pos.to_array(),
            "shots":g.main_gun_shot_sequence(),"crew":g.crew.iter().map(|c|json!({"role":c.role.name(),"alive":c.ok})).collect::<Vec<_>>()},
        "allies":g.allies.iter().enumerate().map(|(i,a)|json!({"id":i,"alive":a.tank.alive,
            "hp":a.tank.hp,"position":a.tank.pos.to_array(),"shots":g.platoon.as_ref().map(|p|p.wingmen[i].shots),
            "policy_s":g.platoon.as_ref().map(|p|p.wingmen[i].policy_s),
            "crew_alive":g.platoon.as_ref().map(|p|p.wingmen[i].crew_alive)})).collect::<Vec<_>>(),
        "enemies":g.enemies.iter().enumerate().map(|(i,e)|json!({"id":i,"alive":e.alive,
            "hp":e.hp,"position":e.pos.to_array(),"shots":g.enemy_main_gun_shot_sequence(i)})).collect::<Vec<_>>(),
        "infantry":g.mixed_combat_telemetry(),"solo_crew":g.crew_solo_receipt(),"loader_work":g.crew_loader_work_receipt(),
        "crew_arm_contacts":g.crew_arm_pose_receipt()})
}

fn loader_observation_point(g: &Game) -> (Vec3, &'static str) {
    if let Some([Some(left), Some(right)]) = g.crew_loader_work_targets() {
        return ((left + right) * 0.5, "actual_working_hand_targets");
    }
    (
        g.crew_solo_man_point(CrewRole::Loader),
        "actual_loader_torso",
    )
}

/// A review driver, not an alternate gunner or loader. One ordinary K edge enables the existing
/// optional autonomous gunner; his own optical perception chooses whether there is any target.
/// The driver gives a newly observed contact up to eight seconds at rest, then twelve seconds
/// moving before another halt. The installed network proposes motion between those halts.
fn commander_input(
    g: &Game,
    r: &Renderer,
    net: &crate::ai::Mlp,
    tick: u64,
    halt: &mut (f32, f32),
) -> TankInput {
    let now = tick as f32 / HZ as f32;
    if g.crew_solo.gunner_target.is_some() && now >= halt.1 {
        halt.0 = now + 8.0;
        halt.1 = now + 20.0;
    }
    let a = net.forward(&g.tiger_executor_obs());
    let (drive, steer) = if now < halt.0 {
        (0.0, 0.0)
    } else {
        g.safe_ai_tiger_drive(a[0], a[1])
    };
    let mut input = TankInput {
        drive,
        steer,
        fire_order: tick == 0,
        ..Default::default()
    };
    if g.mode == Mode::Tank
        && g.station == Station::Commander
        && g.crew_solo_role_ok(CrewRole::Commander)
    {
        let to = (loader_observation_point(g).0 - r.camera.pos).normalize_or_zero();
        let tf = g.player.turret_forward();
        let wrap = |a: f32| {
            (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        let yaw = wrap(to.x.atan2(-to.z) - tf.x.atan2(-tf.z));
        let pitch = clamp_commander_head_pitch(
            to.y.clamp(-1.0, 1.0).asin() + COMMANDER_CAMERA_DOWN_TILT,
            g.hatch_open,
            g.commander_outside,
        );
        input.look_dx = wrap(yaw - g.head_yaw).clamp(-1.4 * STEP, 1.4 * STEP) / 0.0022;
        input.look_dy = -(pitch - g.head_pitch).clamp(-1.0 * STEP, 1.0 * STEP) / 0.0022;
    }
    input
}

/// Follow one actual living subject for a six-second cut. Wide shots cover the local encounter,
/// not the entire map; low infantry shots keep a human readable at the native resolution.
fn camera(g: &Game, r: &mut Renderer, frame: u64, chosen: &mut Option<usize>) -> Value {
    let list = subjects(g);
    let cut = frame / CUT_FRAMES;
    let phase = cut % 5;
    if frame % CUT_FRAMES == 0 || !list.iter().any(|s| Some(s.id) == *chosen) {
        let desired = match phase {
            1 => Some((0, true)),
            2 => Some((1, false)),
            3 => Some((1, true)),
            4 => Some((0, false)),
            _ => None,
        };
        *chosen = list
            .iter()
            .filter(|s| desired.is_none_or(|(team, inf)| s.team == team && s.infantry == inf))
            .min_by(|a, b| {
                let nearest = |s: &Subject| {
                    list.iter()
                        .filter(|o| o.team != s.team)
                        .map(|o| o.pos.distance_squared(s.pos))
                        .fold(f32::INFINITY, f32::min)
                };
                nearest(a).total_cmp(&nearest(b))
            })
            .or_else(|| list.first())
            .map(|s| s.id);
        r.reset_history();
    }
    let s = list
        .iter()
        .find(|s| Some(s.id) == *chosen)
        .copied()
        .unwrap_or(Subject {
            id: 0,
            team: 0,
            infantry: false,
            pos: g.player.center(),
            forward: g.player.forward(),
        });
    let target = if phase == 0 {
        list.iter()
            .filter(|o| o.team != s.team)
            .min_by(|a, b| {
                a.pos
                    .distance_squared(s.pos)
                    .total_cmp(&b.pos.distance_squared(s.pos))
            })
            .map_or(s.pos, |other| {
                s.pos + (other.pos - s.pos).clamp_length_max(60.0) * 0.5
            })
    } else {
        s.pos
    };
    let (distance, height, fov) = if phase == 0 {
        (36.0, 15.0, 72.0_f32)
    } else if s.infantry {
        (5.0, 1.8, 57.0)
    } else {
        (12.0, 4.0, 62.0)
    };
    let cover = g.mixed_cover(None);
    let base = s.forward.x.atan2(s.forward.z) + 2.35;
    let mut eye = target + Vec3::Y * 20.0;
    let mut found = false;
    for step in 0..16 {
        let angle = base + step as f32 * std::f32::consts::TAU / 16.0;
        let mut p = target + Vec3::new(angle.sin() * distance, height, angle.cos() * distance);
        p.y = p.y.max(g.hf.height_at(p.x, p.z) + 1.4);
        let delta = p - target;
        if !cover.occupies(p, 0.25, p.y - 0.25, p.y + 0.25)
            && g.nearest_cover_ray(target, delta.normalize_or_zero(), delta.length())
                .is_none()
            && ray_terrain(target, delta.normalize_or_zero(), &g.hf, delta.length()).is_none()
        {
            eye = p;
            found = true;
            break;
        }
    }
    r.camera.pos = eye;
    r.camera.set_forward(target - eye);
    r.camera.fov_y = fov.to_radians();
    r.reticle = false;
    r.optic_mask = 0.0;
    r.optic_blast = 0.0;
    r.crew_ko = 0.0;
    r.interior_light = 0.0;
    r.interior_haze = 0.0;
    r.rt_mix = 1.0;
    r.weather_shelter = 0.0;
    r.set_interior_atmosphere_enabled(false);
    r.set_active_view(1);
    g.drive_singleplayer_crew_rigs_for_view(None, r);
    json!({"frame":frame,"cut":cut,"phase":phase,"subject":s.id,"infantry":s.infantry,
        "eye":eye.to_array(),"target":target.to_array(),"clear_camera_candidate":found})
}

struct Encoder {
    child: Child,
    input: Option<ChildStdin>,
}
impl Encoder {
    fn new(path: &str) -> anyhow::Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "warning",
                "-n",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                "640x360",
                "-framerate",
                "24",
                "-i",
                "-",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "16",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                path,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = Some(child.stdin.take().context("encoder input absent")?);
        Ok(Self { child, input })
    }
    fn frame(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.input
            .as_mut()
            .context("encoder closed")?
            .write_all(bytes)?;
        Ok(())
    }
    fn finish(&mut self) -> anyhow::Result<()> {
        self.input.take();
        ensure!(self.child.wait()?.success(), "video encoder failed");
        Ok(())
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        self.input.take();
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Root CLI hook: combined-battle-review PREFIX [SEED] [MAX_SECONDS] [T34_WEIGHTS] [TIGER_WEIGHTS].
/// Maximum runtime is a cap, never a promised victory; ordinary policy/physics owns the outcome.
pub fn run(
    requested: &str,
    seed: u32,
    max_seconds: f32,
    t34_weights: &str,
    tiger_weights: &str,
) -> anyhow::Result<()> {
    run_view(
        requested,
        seed,
        max_seconds,
        t34_weights,
        tiger_weights,
        false,
    )
}
/// The same combat fixture, recorded only through the normal, closed-hatch commander eye.
pub fn run_inside(
    requested: &str,
    seed: u32,
    max_seconds: f32,
    t34_weights: &str,
    tiger_weights: &str,
) -> anyhow::Result<()> {
    run_view(
        requested,
        seed,
        max_seconds,
        t34_weights,
        tiger_weights,
        true,
    )
}
fn run_view(
    requested: &str,
    seed: u32,
    max_seconds: f32,
    t34_weights: &str,
    tiger_weights: &str,
    inside: bool,
) -> anyhow::Result<()> {
    ensure!(
        max_seconds.is_finite() && (5.0..=600.0).contains(&max_seconds),
        "length must be 5..600 seconds"
    );
    let prefix = crate::capture_target(requested);
    let receipt = format!("{prefix}.json");
    let movie = format!("{prefix}.mp4");
    let picture = format!("{prefix}_picture.mp4");
    let sound = format!("{prefix}_gameplay_audio.wav");
    let poster = format!("{prefix}_poster.png");
    for path in [&receipt, &movie, &picture, &sound, &poster] {
        ensure!(!Path::new(path).exists(), "output exists: {path}");
    }
    if let Some(parent) = Path::new(&prefix).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (t34, t34_meta) = load_policy(t34_weights)?;
    let (tiger, tiger_meta) = load_policy(tiger_weights)?;
    // Process-local presentation/population selection before a single build. No persistent config
    // or authored asset is changed, and no combat setting is changed after staging.
    std::env::set_var("KESSEL_INTERNAL_RES", "640x360");
    std::env::set_var("KESSEL_FIXED_PLAYTEST_GRAPHICS", "0");
    std::env::set_var("KESSEL_PLATOON", "2");
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    std::env::set_var("KESSEL_MIXED_COMBAT", "1");
    std::env::remove_var("KESSEL_HATCH_OPEN");
    crate::graphics::set_active_preset(crate::graphics::GraphicsPreset::Ultra);
    let mut report = json!({"schema":"panzer.combined-battle-review.v1","status":"building","seed":seed,
        "view":if inside {"ordinary_commander_first_person"}else{"detached_external_cuts"},
        "player_controller":"Ordinary commander K experiment enables NPC gunner and loader. Installed Tiger network proposes driver motion; contact grants an 8s firing halt, then 12s movement. Bounded ordinary head look toward current working cartridge/case hand targets, otherwise the physical loader torso. No eye translation, station hopping, trigger injection, auto reload or casualty override.",
        "maximum_seconds":max_seconds,"weights":{"t34":t34_meta,"tiger":tiger_meta},
        "staging":"Act-0 arena; nine armour rigs and 28 source infantry; supported clear initial deployment and standing advance orders only. No historical reenactment claim.",
        "limits":["Faction battle, not free-for-all: tank knockout is not a claim that every crewman died.",
            "Two German wingmen use simplified deterministic behind-armour crew/module casualties; player Tiger uses its ordinary physical damage path.",
            "German armour policy targets armour; infantry rifles cannot finish a closed surviving tank. A stalemate is possible.",
            "Soviet movement network retains the runtime player-centred observation; gun targeting can choose a nearer wingman.",
            "External visual witness retains the embodied Tiger gameplay audio listener, including its enclosure/hearing; audio is not relocated to each camera cut."],
        "video":{"fps":FPS,"simulation_hz":HZ,"internal_pixels":[640,360],"speed":1.0,"benchmark":false},
        "no_injected_hits_or_casualties":true,"no_reinforcements_or_refills":true,
        "media":{"video":movie,"picture":picture,"audio":sound,"poster":poster},"visual_review_pending":true});
    std::fs::write(&receipt, serde_json::to_vec_pretty(&report)?)?;
    let result = (|| -> anyhow::Result<()> {
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut r, mut g) = Game::build_act(&device, &queue, false, &crate::world::act(0))?;
        ensure!(
            (r.iw, r.ih) == (640, 360),
            "internal resolution override was rejected"
        );
        stage(&mut g, seed, &t34, &tiger, tiger_weights)?;
        ensure!(
            !g.god && !g.brawl && !g.targets_only,
            "non-mortal or incompatible combat mode"
        );
        g.set_defer_rtt_to_render(true);
        g.debug_begin_crew_battle_audit();
        g.begin_audio_capture();
        report["adapter"] = json!(adapter.get_info().name);
        report["renderer"] = json!({"backend":format!("{:?}",crate::graphics::renderer_backend()),
            "preset":crate::graphics::active_preset().key()});
        report["initial"] = ledger(&g);
        report["status"] = json!("recording");
        let initial_main_shots = g.main_gun_shot_sequence();
        let mut encoder = Encoder::new(&picture)?;
        let mut halt = (0.0_f32, 0.0_f32);
        let mut audio = Vec::new();
        let mut timeline = Vec::new();
        let mut cuts = Vec::new();
        let mut subject = None;
        let mut previous = counts(&g);
        let mut elapsed_frames = 0_u64;
        let mut victory_frame = None;
        let started = Instant::now();
        for frame in 0..(max_seconds * FPS as f32).ceil() as u64 {
            for sub in 0..HZ / FPS {
                let tick = frame * u64::from(HZ / FPS) + u64::from(sub);
                g.debug_crew_battle_clock(tick + 1, (tick + 1) as f64 / f64::from(HZ));
                let input = commander_input(&g, &r, &tiger, tick, &mut halt);
                g.update(&input, STEP, &mut r);
                audio.extend(g.render_audio_capture_frames(audio_frames(tick))?);
                let current = counts(&g);
                if current != previous {
                    timeline.push(
                        json!({"tick":tick+1,"seconds":(tick+1) as f64/f64::from(HZ),
                        "before":previous,"after":current,"state":ledger(&g)}),
                    );
                    previous = current;
                }
            }
            // Restore all camera-dependent fields before the next Game tick. The detached film
            // eye never becomes an infantry target or changes the gameplay listener/controls.
            let saved = (
                r.camera,
                r.reticle,
                r.optic_mask,
                r.optic_blast,
                r.crew_ko,
                r.interior_light,
                r.interior_haze,
                r.rt_mix,
                r.weather_shelter,
            );
            if inside {
                // Same scheduling as WindowState::render_game: the panes and protected cabin
                // are one shared-shadow frame. Without this, deferred legacy panes go stale.
                r.begin_coop_shadow_frame(g.player.center());
                let shadow_ready = g.refresh_rtt_periscopes(&mut r);
                r.set_coop_shadow_reuse(shadow_ready);
                if g.protected_main_interior_view() {
                    r.camera.zfar = crate::PROTECTED_INTERIOR_FAR_M;
                }
            }
            let previous_subject = subject;
            let camera_record = if inside {
                let (look_target, target_kind) = loader_observation_point(&g);
                json!({"frame":frame,"eye":r.camera.pos.to_array(),
                "forward":r.camera.forward().to_array(),"station":format!("{:?}",g.station),
                "hatch_open":g.hatch_open,"head_out":g.commander_outside,"normal_game_camera":true,
                "head_pitch_rad":g.head_pitch,"look_target":look_target.to_array(),"look_target_kind":target_kind,
                "authored_station_eye":g.station_eye(g.station).to_array(),"near_plane_m":r.camera.znear,
                "pose_limit":"Orientation only; ordinary chassis/head sway retained, no new neck/body IK or geometry hiding.",
                "crew_arm_contacts":g.crew_arm_pose_receipt()})
            } else {
                camera(&g, &mut r, frame, &mut subject)
            };
            let rgba = r.render_to_rgba(&device, &queue, r.iw, r.ih)?;
            encoder.frame(&rgba)?;
            if inside {
                r.camera.zfar = saved.0.zfar;
                r.end_coop_shadow_frame();
            }
            if frame == 0 {
                image::save_buffer(&poster, &rgba, 640, 360, image::ColorType::Rgba8)?;
            }
            if frame % CUT_FRAMES == 0 || subject != previous_subject {
                cuts.push(camera_record);
            }
            if !inside {
                (
                    r.camera,
                    r.reticle,
                    r.optic_mask,
                    r.optic_blast,
                    r.crew_ko,
                    r.interior_light,
                    r.interior_haze,
                    r.rt_mix,
                    r.weather_shelter,
                ) = saved;
                r.set_interior_atmosphere_enabled(saved.5 > 0.5);
                r.set_active_view(0);
                g.drive_singleplayer_crew_rigs_for_view(g.singleplayer_embodied_role(), &mut r);
            }
            elapsed_frames = frame + 1;
            if outcome(counts(&g)).is_some() {
                victory_frame.get_or_insert(frame);
            } else {
                victory_frame = None;
            }
            if frame % u64::from(FPS * 5) == 0 {
                report["elapsed_seconds"] = json!(elapsed_frames as f64 / FPS as f64);
                report["latest"] = ledger(&g);
                report["timeline"] = json!(timeline);
                report["camera_cuts"] = json!(cuts);
                std::fs::write(&receipt, serde_json::to_vec_pretty(&report)?)?;
                println!(
                    "[combined-battle] {:.1}/{max_seconds:.1}s counts {:?}; wall {:.1}s",
                    elapsed_frames as f64 / FPS as f64,
                    counts(&g),
                    started.elapsed().as_secs_f32()
                );
            }
            if victory_frame.is_some_and(|first| frame - first >= u64::from(FPS * 4)) {
                break;
            }
        }
        encoder.finish()?;
        ensure!(
            audio.iter().all(|x| x.is_finite()),
            "nonfinite captured audio"
        );
        crate::acoustics::write_wav_stereo(&sound, &audio, crate::acoustics::SAMPLE_RATE)?;
        let mux = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "warning",
                "-n",
                "-i",
                &picture,
                "-i",
                &sound,
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
                "-shortest",
                &movie,
            ])
            .status()?;
        ensure!(
            mux.success(),
            "audio/video mux failed; source picture and audio retained"
        );
        report["status"] = json!("recorded");
        report["result"] = json!(outcome(counts(&g)).unwrap_or("runtime_cap_no_elimination"));
        report["elapsed_seconds"] = json!(elapsed_frames as f64 / FPS as f64);
        report["frames"] = json!(elapsed_frames);
        report["final"] = ledger(&g);
        report["timeline"] = json!(timeline);
        report["camera_cuts"] = json!(cuts);
        report["projectile_audit"] = g.debug_crew_battle_snapshot();
        report["enemy_ai"] = g.enemy_ai_summary();
        report["interior_observed"] = json!({"gunner_fire_pulses":g.crew_solo.shots_by_gunner,
            "actual_main_gun_discharges":g.main_gun_shot_sequence().wrapping_sub(initial_main_shots),
            "actual_loader_cycles":g.crew_solo.loads_by_loader,"hatch_closed":!g.hatch_open,
            "head_inside":!g.commander_outside,"final_station":format!("{:?}",g.station),
            "note":"Zero actual shots/loads is retained as zero, not advertised as proof of firing or loading."});
        report["audio"] = json!({"source":"ordinary device-free Game mixer, same simulation clock","sample_frames":audio.len()/2,
            "peak":audio.iter().map(|s|s.abs()).fold(0.0_f32,f32::max),"metadata":g.audio_capture_metadata()});
        Ok(())
    })();
    if let Err(error) = &result {
        report["status"] = json!("failed");
        report["error"] = json!(format!("{error:#}"));
    }
    std::fs::write(&receipt, serde_json::to_vec_pretty(&report)?)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combined_battle_clock_preserves_real_time_and_audio() {
        assert_eq!(HZ % FPS, 0);
        assert_eq!(
            (0..u64::from(HZ) * 600).map(audio_frames).sum::<usize>(),
            crate::acoustics::SAMPLE_RATE as usize * 600
        );
    }
    #[test]
    fn combined_battle_does_not_call_tank_knockouts_total_team_elimination() {
        assert_eq!(outcome([0, 0, 1, 1]), None);
        assert_eq!(outcome([3, 0, 0, 1]), None); // closed armour versus surviving rifleman can stalemate
        assert_eq!(
            outcome([0, 0, 0, 1]),
            Some("soviet_combat_roster_remaining")
        );
        assert_eq!(
            outcome([1, 0, 4, 0]),
            Some("german_combat_roster_remaining")
        );
        assert_eq!(
            outcome([0, 0, 0, 0]),
            Some("both_combat_rosters_eliminated")
        );
    }
}
