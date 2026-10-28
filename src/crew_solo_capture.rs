//! Headless proof of solo play with a living crew (`crew-solo PREFIX`).
//!
//! One scripted run on the act-0 arena: the crew works its posts under the commander's orders;
//! a real penetration wounds a man and the player goes to him and treats the wound; a second
//! kills the gunner, the gun goes dark, the player drags the body off the seat and takes the
//! sights himself; a third kills the driver, the tank halts under orders until the player
//! climbs into the driver's seat. Stills go to `captures/`; a JSON receipt records every
//! check, the solver's attempts and the crew's state, also when the run aborts. PASS/FAIL per
//! check; the command fails on any FAIL.
//!
//! The solver decides who a cone finds. The proof therefore treats whichever man the wounding
//! strike leaves alive and bleeding, and clears a dead driver's body before it needs his seat.
use crate::first_aid::Region;
use crate::game::{Arc, CrewRole, Game, Station, TankInput};
use anyhow::Context;
use glam::Vec3;
use serde_json::{json, Value};

const DT: f32 = 1.0 / 60.0;
const AI_ROLES: [CrewRole; 4] = [CrewRole::Gunner, CrewRole::Loader, CrewRole::Driver, CrewRole::Radioman];

fn steps(seconds: f32) -> usize {
    (seconds / DT).round().max(1.0) as usize
}

struct Proof<'a> {
    prefix: String,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    checks: Vec<Value>,
    strikes: Vec<Value>,
    stills: Vec<String>,
    pass: bool,
}

impl Proof<'_> {
    fn check(&mut self, label: &str, ok: bool, detail: Value) {
        println!("[crew-solo] {label} — {}", if ok { "PASS" } else { "FAIL" });
        self.checks.push(json!({"check": label, "pass": ok, "detail": detail}));
        if !ok {
            self.pass = false;
        }
    }

    fn still(&mut self, r: &mut crate::renderer::Renderer, name: &str) -> anyhow::Result<()> {
        let path = format!("{}_{name}.png", self.prefix);
        r.render_to_png(self.device, self.queue, &path, r.iw * 3, r.ih * 3)?;
        println!("[crew-solo] wrote {path}");
        self.stills.push(path);
        Ok(())
    }
}

fn man(g: &Game, role: CrewRole) -> Value {
    let m = g.debug_crew_solo_man(role);
    json!({"role": role.name(), "state": m.state_label(), "performance": m.performance(),
        "blood_loss": m.blood_loss_fraction(), "bleed_l_per_min": m.total_bleed_rate_l_per_s() * 60.0,
        "wounds": m.wounds.len(), "dead": m.dead, "conscious": m.is_conscious()})
}

fn crew(g: &Game) -> Value {
    json!(AI_ROLES.iter().map(|&role| man(g, role)).collect::<Vec<_>>())
}

fn run_for(g: &mut Game, r: &mut crate::renderer::Renderer, input: &TankInput, seconds: f32) {
    for _ in 0..steps(seconds) {
        g.update(input, DT, r);
    }
}

/// Step with `input` until `done` holds or `max_s` elapses; returns the seconds spent.
fn run_until(
    g: &mut Game,
    r: &mut crate::renderer::Renderer,
    input: &TankInput,
    max_s: f32,
    mut done: impl FnMut(&Game) -> bool,
) -> f32 {
    let mut t = 0.0;
    while t < max_s {
        if done(g) {
            return t;
        }
        g.update(input, DT, r);
        t += DT;
    }
    t
}

fn shots(g: &Game) -> u64 {
    g.crew_solo_receipt()["shots_by_gunner"].as_u64().unwrap_or(0)
}

fn dead(g: &Game, role: CrewRole) -> bool {
    g.debug_crew_solo_man(role).dead
}

/// Where the range targets are parked, as the placement search found it.
#[derive(Clone, Copy)]
struct Spot {
    range: f32,
    bearing: f32,
    heading: f32,
}

/// The current range target, or the next one rolled up to the spot when it has been killed.
/// The 88 kills a T-34 in one round often enough that the proof keeps a queue of them.
fn ensure_target(g: &mut Game, r: &mut crate::renderer::Renderer, target: &mut usize, spot: Spot) -> usize {
    if g.enemies[*target].alive {
        return *target;
    }
    for i in 0..g.enemies.len() {
        if i == *target || !g.enemies[i].alive {
            continue;
        }
        let bearing = spot.heading + spot.bearing.to_radians();
        let at = g.player.pos + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * spot.range;
        g.debug_place_enemy(i, at.x, at.z, bearing + std::f32::consts::PI);
        g.debug_crew_solo_set_enemy_active(i, true);
        g.debug_engage_all();
        g.update(&TankInput::default(), DT, r);
        *target = i;
        println!("[crew-solo] next range target: T-34 #{i}");
        return i;
    }
    *target
}

/// Walk to a dead man's body and hold E until it is dragged aside. Returns the hover seen.
fn clear_body(g: &mut Game, r: &mut crate::renderer::Renderer, idle: &TankInput, hold_e: &TankInput, role: CrewRole) -> Option<String> {
    if g.station != Station::Loader {
        g.jump_station(Station::Loader);
        run_for(g, r, idle, 1.2);
    }
    g.debug_crew_solo_go_to(role);
    run_for(g, r, idle, 0.2);
    let hover = g.debug_crew_solo_hover();
    if hover.is_none() {
        let diag = g.debug_crew_solo_hover_diag(r.camera.pos, r.camera.forward());
        println!("[crew-solo] no body hover for {}: {diag}", role.name());
    }
    run_for(g, r, hold_e, crate::game::CREW_SOLO_CLEAR_BODY_S + 0.5);
    run_for(g, r, idle, 0.2);
    hover
}

/// Fire the loaded round at the range target on the commander's order so the breech holds a
/// spent case when a strike comes: a live round in the chamber is the Tiger's own catastrophe,
/// not the crew proof. Leaves the standing order where it was.
fn empty_the_breech(g: &mut Game, r: &mut crate::renderer::Renderer, target: &mut usize, spot: Spot) {
    let idle = TankInput::default();
    ensure_target(g, r, target, spot);
    let fire_at_will = g.crew_solo_receipt()["fire_at_will"] == true;
    g.debug_crew_solo_set_fire_at_will(false);
    run_until(g, r, &idle, 20.0, |g| g.breech_loaded());
    let before = shots(g);
    g.debug_crew_solo_fire_command();
    let waited = run_until(g, r, &idle, 8.0, |g| !g.breech_loaded());
    g.update(&idle, DT, r);
    g.debug_crew_solo_set_fire_at_will(fire_at_will);
    println!("[crew-solo] empty_the_breech: fired={} waited={waited:.1}s loaded_now={} target_alive={}",
        shots(g) > before, g.breech_loaded(), g.enemies[*target].alive);
}

/// From the cupola or the sights down to the basket floor, then into the driver's seat through
/// its anchor. Returns what the hover said and whether the seat was taken.
fn take_driver_seat(g: &mut Game, r: &mut crate::renderer::Renderer, idle: &TankInput) -> (Option<String>, bool) {
    if g.station != Station::Loader {
        g.jump_station(Station::Loader);
        run_for(g, r, idle, 1.2);
    }
    g.debug_crew_solo_face_driver_seat();
    run_for(g, r, idle, 0.2);
    let hover = g.hover_label_text().map(|l| l.to_string());
    if hover.as_deref() != Some("DRIVER'S SEAT [click]") {
        let diag = g.debug_crew_solo_hover_diag(r.camera.pos, r.camera.forward());
        println!("[crew-solo] driver's seat not offered: {diag}");
    }
    let took = g.click_station();
    run_for(g, r, idle, 1.5);
    (hover, took && g.station == Station::Driver)
}

fn scenario(g: &mut Game, r: &mut crate::renderer::Renderer, proof: &mut Proof, spot: Spot) -> anyhow::Result<()> {
    let mut target = 0usize;
    let idle = TankInput::default();
    let hold_e = TankInput { mg_coax: true, interaction: true, ..TankInput::default() };
    let forward = TankInput { drive: 1.0, ..TankInput::default() };

    // ── 1. The crew at its posts, seen from the cupola ───────────────────────────────────
    run_for(g, r, &idle, 1.0);
    let receipt = g.crew_solo_receipt();
    let alive = receipt["men"].as_array().map_or(0, |men| men.iter().filter(|m| m["alive"] == true).count());
    let (recognizable, los) = g.debug_crew_solo_target_visible(0);
    proof.check(
        "five men aboard; gunner, driver and loader working their own posts; player at the cupola",
        alive == 5
            && g.station == Station::Commander
            && g.debug_crew_solo_working(CrewRole::Gunner)
            && g.debug_crew_solo_working(CrewRole::Driver)
            && g.debug_crew_solo_working(CrewRole::Loader),
        json!({"alive": alive, "station": format!("{:?}", g.station),
            "target_range_m": g.enemies[0].pos.distance(g.player.pos),
            "target_recognizable": recognizable, "target_line_clear": los}),
    );
    g.debug_face_gunner_sight();
    run_for(g, r, &idle, 0.1);
    proof.still(r, "01_commander_view_of_crew")?;

    // ── 2. Standing order: the gunner lays on the contact and fires; the loader reloads ──
    let shots_before = shots(g);
    let t_fire = run_until(g, r, &idle, 25.0, |g| shots(g) > shots_before);
    let (yaw_err, pitch_err) = g.campaign_ai_lay_error();
    proof.check(
        "gunner lays on the T-34 and fires under fire-at-will",
        shots(g) > shots_before && yaw_err.abs() < 0.05,
        json!({"seconds_to_shot": t_fire, "lay_error_rad": [yaw_err, pitch_err],
            "turret_yaw": g.player.turret_yaw, "engine_on": g.engine_on,
            "gunner_target": g.crew_solo_receipt()["gunner_target"]}),
    );
    let t_reload = run_until(g, r, &idle, 20.0, |g| g.breech_loaded());
    proof.check(
        "loader serves the breech back to loaded at a human rate (3-12 s)",
        g.breech_loaded() && (3.0..=12.0).contains(&t_reload),
        json!({"reload_seconds": t_reload, "loads_by_loader": g.crew_solo_receipt()["loads_by_loader"]}),
    );
    proof.still(r, "02_after_first_shot")?;

    // ── 3. Hold fire, then the commander's fire command ──────────────────────────────────
    ensure_target(g, r, &mut target, spot);
    g.debug_crew_solo_set_fire_at_will(false);
    let shots_hold = shots(g);
    run_for(g, r, &idle, 8.0);
    let held = shots(g) == shots_hold;
    g.debug_crew_solo_fire_command();
    let t_cmd = run_until(g, r, &idle, 6.0, |g| shots(g) > shots_hold);
    proof.check(
        "hold-fire holds the gunner; the commander's fire command releases one shot",
        held && shots(g) == shots_hold + 1,
        json!({"held_for_8s": held, "seconds_to_commanded_shot": t_cmd}),
    );
    g.debug_crew_solo_set_fire_at_will(true);
    run_until(g, r, &idle, 20.0, |g| g.breech_loaded());

    // ── 4. The driver follows the commander's orders ─────────────────────────────────────
    let start = g.player.pos;
    run_for(g, r, &forward, 5.0);
    let moved = g.player.pos.distance(start);
    run_for(g, r, &idle, 4.0);
    proof.check(
        "driver moves the tank on the commander's forward order and halts on none",
        moved > 3.0 && g.player.speed.abs() < 0.3,
        json!({"moved_m": moved, "speed_after_halt": g.player.speed}),
    );

    // ── 5. A real penetration wounds the crew ────────────────────────────────────────────
    // Through the hull side at the driver's seat, with the player at the cupola well away
    // from the cone, right after a commanded shot so the breech holds a spent case. (The
    // loader's post is the stowage side of the turret: every wounding lane tried there
    // found live rounds first and the Tiger went up.) Whoever the cone leaves alive and
    // bleeding is the patient.
    empty_the_breech(g, r, &mut target, spot);
    let wound_strike = g.debug_crew_solo_strike(CrewRole::Driver, false, Arc::Side, 0.0);
    proof.strikes.push(wound_strike.clone());
    anyhow::ensure!(g.player.alive, "the proof's own body was killed by the wounding strike");
    // The patient: a living man who is bleeding and can be saved (the slowest bleed among the
    // wounded; a man cut up by a dozen fragments is past one dressing, and the proof is about
    // the treatment, not triage).
    let mut patient = AI_ROLES
        .into_iter()
        .filter(|&role| !dead(g, role) && g.debug_crew_solo_man(role).is_wounded())
        .min_by(|&a, &b| {
            let rate = |role: CrewRole| {
                let m = g.debug_crew_solo_man(role);
                if m.is_bleeding() { m.total_bleed_rate_l_per_s() } else { f32::INFINITY }
            };
            rate(a).total_cmp(&rate(b))
        });
    let mut injected = false;
    if patient.is_none() {
        // The solver's rounds found nobody alive to wound (named in the receipt): put a wound
        // in the first living man so the first-aid proof still runs on a real post.
        if let Some(role) = AI_ROLES.into_iter().find(|&role| !dead(g, role)) {
            g.debug_crew_solo_wound(role, Region::Leg, 0.55);
            patient = Some(role);
            injected = true;
        }
    }
    let Some(patient) = patient else {
        anyhow::bail!("no living crewman left to treat");
    };
    proof.check(
        "the live penetration leaves at least one crewman wounded and alive to treat",
        !injected,
        json!({"patient": patient.name(), "crew": crew(g), "strike": wound_strike,
            "injected_fallback": injected, "player_alive": g.player.alive}),
    );
    run_for(g, r, &idle, 1.5);
    let bleed_before = g.debug_crew_solo_man(patient).total_bleed_rate_l_per_s();
    let perf_wounded = g.debug_crew_solo_man(patient).performance();
    // Down from the cupola to the basket floor, over to the man.
    g.jump_station(Station::Loader);
    run_for(g, r, &idle, 1.2);
    g.debug_crew_solo_go_to(patient);
    run_for(g, r, &idle, 0.2);
    proof.still(r, "03_crewman_hit")?;

    // ── 6. The player goes to him and treats the wound ───────────────────────────────────
    let hover = g.debug_crew_solo_hover();
    let label = g.hover_label_text().map(|l| l.to_string());
    let expect = format!("Treat({patient:?}");
    proof.check(
        "crosshair on the wounded man offers the indicated treatment",
        hover.as_deref().is_some_and(|h| h.contains(&expect)),
        json!({"patient": patient.name(), "hover": hover, "label": label}),
    );
    let box_before = g.debug_crew_solo_aid_box();
    let indicated = g.debug_crew_solo_man(patient).indicated();
    let duration = indicated.map_or(20.0, |t| t.duration_s());
    run_for(g, r, &hold_e, duration * 0.5);
    let mid_progress = g.debug_crew_solo_action_progress();
    proof.still(r, "04_treating_crewman")?;
    run_for(g, r, &hold_e, duration * 0.5 + 0.5);
    run_for(g, r, &idle, 0.2);
    let box_after = g.debug_crew_solo_aid_box();
    let treated = g.debug_crew_solo_man(patient).clone();
    let bleed_after = treated.total_bleed_rate_l_per_s();
    let count = |b: crate::first_aid::AidBox| u32::from(b.dressings) + u32::from(b.tourniquets) + u32::from(b.splints) + u32::from(b.morphine);
    let consumed = count(box_before) - count(box_after);
    proof.check(
        "holding E at the man for the treatment's real seconds consumes one item and controls the bleed",
        consumed == 1 && (bleed_after < bleed_before || !treated.is_bleeding()),
        json!({"patient": patient.name(), "treatment": indicated.map(|t| t.label()), "seconds": duration,
            "progress_at_half": mid_progress, "box_before": format!("{box_before:?}"),
            "box_after": format!("{box_after:?}"), "bleed_before_l_per_min": bleed_before * 60.0,
            "bleed_after_l_per_min": bleed_after * 60.0}),
    );
    // Back to the cupola: the treated man is at his post and works, degraded.
    g.jump_station(Station::Commander);
    run_for(g, r, &idle, 60.0);
    let treated = g.debug_crew_solo_man(patient).clone();
    proof.check(
        "a minute later the treated man is alive, at his post, working degraded",
        !treated.dead && g.debug_crew_solo_working(patient) && treated.performance() < 1.0,
        json!({"patient": man(g, patient), "performance_when_wounded": perf_wounded, "crew": crew(g)}),
    );
    run_until(g, r, &idle, 20.0, |g| g.breech_loaded());

    // ── 7. The gunner is killed; the gun goes dark ───────────────────────────────────────
    // The player takes the driver's seat for this one: the hull front is clear of a turret
    // side cone. A dead driver's body has to come off the seat first.
    let mut cleared_driver_first = None;
    if dead(g, CrewRole::Driver) && !g.debug_crew_solo_body_cleared(CrewRole::Driver) {
        cleared_driver_first = clear_body(g, r, &idle, &hold_e, CrewRole::Driver);
    }
    let (seat_hover, in_seat) = take_driver_seat(g, r, &idle);
    proof.check(
        "the driver's seat is reachable from the interior walk and the move costs the clamber",
        in_seat,
        json!({"hover": seat_hover, "station": format!("{:?}", g.station),
            "driver_body_cleared_first": cleared_driver_first}),
    );
    anyhow::ensure!(in_seat, "could not take the driver's seat before the gunner strike");
    let gunner_strike = if dead(g, CrewRole::Gunner) {
        json!({"skipped": "gunner already dead from an earlier cone"})
    } else {
        empty_the_breech(g, r, &mut target, spot);
        g.debug_crew_solo_strike(CrewRole::Gunner, true, Arc::Side, 0.0)
    };
    proof.strikes.push(gunner_strike.clone());
    anyhow::ensure!(g.player.alive, "the proof's own body was killed by the gunner strike");
    let shots_dark = shots(g);
    g.jump_station(Station::Loader);
    run_for(g, r, &idle, 1.2);
    g.jump_station(Station::Commander);
    run_for(g, r, &idle, 10.0);
    proof.check(
        "gunner killed by the live solver; nobody lays or fires the gun for ten seconds",
        dead(g, CrewRole::Gunner) && !g.debug_crew_solo_working(CrewRole::Gunner) && shots(g) == shots_dark,
        json!({"strike": gunner_strike, "crew": crew(g)}),
    );
    g.debug_face_gunner_sight();
    run_for(g, r, &idle, 0.1);
    proof.still(r, "05_gunner_dead_from_cupola")?;

    // ── 8. The body blocks the seat until dragged off; then the player takes the gun ──────
    g.jump_station(Station::Loader);
    run_for(g, r, &idle, 1.2);
    g.debug_face_gunner_sight();
    run_for(g, r, &idle, 0.2);
    let blocked_label = g.hover_label_text().map(|l| l.to_string());
    let refused = !g.click_station() || g.station != Station::Gunner;
    run_for(g, r, &idle, 1.0);
    proof.check(
        "the dead gunner in the seat refuses the sights",
        refused && g.station == Station::Loader && !g.debug_crew_solo_body_cleared(CrewRole::Gunner),
        json!({"label": blocked_label, "station": format!("{:?}", g.station)}),
    );
    let drag_hover = clear_body(g, r, &idle, &hold_e, CrewRole::Gunner);
    proof.check(
        "holding E at the body drags it off the seat",
        drag_hover.as_deref().is_some_and(|h| h.contains("ClearBody(Gunner")) && g.debug_crew_solo_body_cleared(CrewRole::Gunner),
        json!({"hover": drag_hover}),
    );
    g.debug_face_gunner_sight();
    run_for(g, r, &idle, 0.2);
    let took_sights = g.click_station();
    run_for(g, r, &idle, 1.0);
    proof.check(
        "the player takes the sights once the body is clear",
        took_sights && g.station == Station::Gunner,
        json!({"station": format!("{:?}", g.station)}),
    );
    // One round handed up to the stowage for the player's shot; the loader at his post loads it.
    g.debug_crew_solo_ammunition_left(1);
    let t_loaded = run_until(g, r, &idle, 25.0, |g| g.breech_loaded());
    println!("[crew-solo] the loader served the handed-up round in {t_loaded:.1} s");
    let target_now = ensure_target(g, r, &mut target, spot);
    // Lay by hand: the harness sets the desired lay the mouse would, the turret chases it.
    let mut laid = false;
    for _ in 0..steps(20.0) {
        let center = g.enemies[target_now].center();
        laid = g.campaign_ai_order_lay(center, 0.01);
        g.update(&idle, DT, r);
        if laid {
            break;
        }
    }
    let shots_by_hand = shots(g);
    let fire = TankInput { mg_coax: true, trigger: true, ..TankInput::default() };
    g.update(&fire, DT, r);
    g.update(&idle, DT, r);
    let fired_by_hand = !g.breech_loaded();
    proof.check(
        "the player lays and fires the gun himself; no AI shot is counted",
        laid && fired_by_hand && shots(g) == shots_by_hand,
        json!({"laid": laid, "fired": fired_by_hand}),
    );
    proof.still(r, "06_player_at_the_sights")?;
    run_until(g, r, &idle, 20.0, |g| g.breech_loaded());

    // ── 9. The driver is killed; the tank halts under orders ─────────────────────────────
    g.jump_station(Station::Loader);
    run_for(g, r, &idle, 1.2);
    g.jump_station(Station::Commander);
    run_for(g, r, &idle, 1.0);
    let driver_strike = if dead(g, CrewRole::Driver) {
        json!({"skipped": "driver already dead from the wounding cone"})
    } else {
        empty_the_breech(g, r, &mut target, spot);
        g.debug_crew_solo_strike(CrewRole::Driver, true, Arc::Side, 0.0)
    };
    proof.strikes.push(driver_strike.clone());
    anyhow::ensure!(g.player.alive, "the proof's own body was killed by the driver strike");
    let start = g.player.pos;
    run_for(g, r, &forward, 6.0);
    let crept = g.player.pos.distance(start);
    proof.check(
        "driver dead; the tank does not move on the commander's order",
        dead(g, CrewRole::Driver) && !g.debug_crew_solo_working(CrewRole::Driver) && crept < 1.0,
        json!({"strike": driver_strike, "moved_m": crept, "crew": crew(g)}),
    );
    proof.still(r, "07_driver_dead_tank_halted")?;

    // ── 10. The player drags the driver off the seat and drives ──────────────────────────
    let driver_hover = if g.debug_crew_solo_body_cleared(CrewRole::Driver) {
        Some("already cleared before the gunner strike".to_owned())
    } else {
        clear_body(g, r, &idle, &hold_e, CrewRole::Driver)
    };
    let (_, in_seat) = take_driver_seat(g, r, &idle);
    let start = g.player.pos;
    run_for(g, r, &forward, 6.0);
    let drove = g.player.pos.distance(start);
    proof.check(
        "the player drags the driver's body off, takes the seat, and drives the tank himself",
        driver_hover.as_deref().is_some_and(|h| h.contains("ClearBody(Driver") || h.contains("already cleared"))
            && g.debug_crew_solo_body_cleared(CrewRole::Driver)
            && in_seat && drove > 3.0,
        json!({"hover": driver_hover, "station": format!("{:?}", g.station), "moved_m": drove}),
    );
    proof.still(r, "08_player_driving")?;
    Ok(())
}

pub fn run_crew_solo_capture(prefix: &str) -> anyhow::Result<()> {
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[crew-solo] adapter={} ({:?}, {:?})", info.name, info.device_type, info.backend);
    let (device, queue) = crate::request_device(&adapter);
    let map = crate::world::act(0);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &map)?;
    g.enable_singleplayer_crew();
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    g.god = false;
    // One parked T-34 as the range target. It neither drives nor fires (`targets_only`: the
    // range fixture): this proof is about the crew, not the duel. The arena is 958 m across
    // and a banished tank clamps to its edge in view, so the other tanks are simply dead.
    g.targets_only = true;
    g.brawl = false;
    g.debug_hold_enemy_fire();
    for i in 1..g.enemies.len() {
        g.debug_crew_solo_set_enemy_active(i, false);
        g.debug_place_enemy(i, g.player.pos.x + 400.0 + 40.0 * i as f32, g.player.pos.z - 400.0, 0.0);
    }
    // Clear weather (the screenshot path's `KESSEL_SHOT_WEATHER=clear` values): the gunner's
    // recognition of a contact is through the real medium, so the medium is stated.
    r.set_weather_override(None);
    r.overcast = 0.08;
    r.storm = 0.0;
    r.fog_density = 0.00035;
    r.precipitation = 0.0;
    // A Tiger down to its last rounds: the chambered one and two in the rearmost stowage, so
    // that by the time the strikes come the gun has fired them and no live round sits in the
    // compartment for a cone to find. With a full load every lane tried into a seated man found
    // live rounds first (see the receipt). One more round is handed up for the player's own shot.
    let rounds_left = g.debug_crew_solo_ammunition_left(2);
    // Park the target where the gunner can actually see it: the first bearing and range from
    // which the muzzle has a clear line through the arena's relief.
    let heading = g.player.heading;
    let mut placement = json!(null);
    let mut spot = Spot { range: 160.0, bearing: 0.0, heading };
    'search: for &range in &[160.0f32, 200.0, 240.0, 120.0, 300.0] {
        for &bearing_deg in &[0.0f32, 15.0, -15.0, 30.0, -30.0, 45.0, -45.0] {
            let bearing = heading + bearing_deg.to_radians();
            let target = g.player.pos + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * range;
            g.debug_place_enemy(0, target.x, target.z, bearing + std::f32::consts::PI);
            g.update(&TankInput::default(), DT, &mut r);
            let (recognizable, los) = g.debug_crew_solo_target_visible(0);
            placement = json!({"range_m": range, "bearing_deg": bearing_deg, "recognizable": recognizable, "line_clear": los});
            spot = Spot { range, bearing: bearing_deg, heading };
            if recognizable && los {
                break 'search;
            }
        }
    }
    println!("[crew-solo] target placement {placement}");
    g.debug_engage_all();

    let mut proof = Proof {
        prefix: prefix.to_owned(),
        device: &device,
        queue: &queue,
        checks: Vec::new(),
        strikes: Vec::new(),
        stills: Vec::new(),
        pass: true,
    };
    let outcome = scenario(&mut g, &mut r, &mut proof, spot);
    let aborted = outcome.as_ref().err().map(|e| format!("{e:#}"));
    let receipt_path = format!("{prefix}.json");
    let receipt = json!({
        "command": "crew-solo",
        "adapter": info.name,
        "map": "act 0 arena",
        "pass": proof.pass && aborted.is_none(),
        "aborted": aborted,
        "checks": proof.checks,
        "strikes": proof.strikes,
        "stills": proof.stills,
        "crew": g.crew_solo_receipt(),
        "fixture": {"weather": "clear override", "target": placement,
            "rounds_left_in_rear_stowage": rounds_left,
            "one_more_round_for_the_players_shot": true,
            "why": "with the full 64-round stowage every wounding lane tried (turret side, turret rear, hull side) set off live rounds, and a killing solid shot through the gunner's seat set off eight rounds left in the rear stowage; so the fixture fires the last rounds off before the strikes, and every strike is timed right after a shot so the breech holds a spent case"},
        "note": "Every strike is the live armor solve, fragment cloud and anatomy; where a wound had to be injected the check says so and fails.",
    });
    std::fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?)
        .with_context(|| format!("writing {receipt_path}"))?;
    println!("[crew-solo] receipt {receipt_path}");
    outcome?;
    println!("[crew-solo] {}", if proof.pass { "ALL PASS" } else { "FAILURES" });
    anyhow::ensure!(proof.pass, "crew-solo proof failed; see {receipt_path}");
    Ok(())
}
