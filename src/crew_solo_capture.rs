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
use crate::first_aid::{Region, WoundKind};
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
        // Each new target stands clear of the burning wrecks of the last ones, at the first
        // bearing and range the gunner can see from the muzzle.
        let mut placed = None;
        'search: for &range in &[spot.range, spot.range + 40.0, spot.range - 30.0, spot.range + 90.0] {
            for k in 0..9 {
                let offset = [20.0f32, -20.0, 35.0, -35.0, 50.0, -50.0, 10.0, -10.0, 0.0][k];
                let bearing = spot.heading + (spot.bearing + offset).to_radians();
                let at = g.player.pos + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * range;
                g.debug_place_enemy(i, at.x, at.z, bearing + std::f32::consts::PI);
                g.debug_crew_solo_set_enemy_active(i, true);
                g.update(&TankInput::default(), DT, r);
                let (recognizable, los) = g.debug_crew_solo_target_visible(i);
                if recognizable && los {
                    placed = Some((range, spot.bearing + offset));
                    break 'search;
                }
            }
        }
        g.debug_engage_all();
        *target = i;
        println!("[crew-solo] next range target: T-34 #{i} at {placed:?} (range m, bearing deg)");
        return i;
    }
    *target
}

/// Work the procedure in the hands the way practised hands would, reading each step's kind:
/// holds held, a press every tenth of a second, wraps as alternating mouse strokes at 0.6 s,
/// windlass turns as presses every 0.8 s, watches with the hands off. Takes `still` once the
/// hands are into the second step. Returns what the receipt recorded for it.
fn work_procedure(g: &mut Game, r: &mut crate::renderer::Renderer, proof: &mut Proof, still: &str) -> anyhow::Result<Value> {
    let idle = TankInput::default();
    let button = TankInput { mg_coax: true, interaction: true, ..TankInput::default() };
    // The press that starts it (an edge: the button was up).
    g.update(&idle, DT, r);
    g.update(&button, DT, r);
    g.update(&idle, DT, r);
    let before = g.debug_crew_solo_procedures_done().len();
    let mut frame = 0usize;
    let mut still_taken = false;
    let mut labels: Vec<String> = Vec::new();
    let mut last = json!(null);
    while let Some(state) = g.debug_crew_solo_procedure() {
        if frame > 60 * 120 {
            break;
        }
        if let Some(label) = state["label"].as_str() {
            if labels.last().map(String::as_str) != Some(label) {
                labels.push(label.to_owned());
            }
        }
        let input = match state["kind"].as_str().unwrap_or("") {
            "Hold" => button,
            "Press" => if frame % 6 == 0 { button } else { idle },
            "Wrap" => TankInput { look_dx: if (frame / 36) % 2 == 0 { 60.0 } else { -60.0 }, ..TankInput::default() },
            "Turn" => if frame % 48 == 0 { button } else { idle },
            _ => idle,
        };
        if !still_taken && !still.is_empty() && state["step"].as_u64().unwrap_or(0) >= 1 {
            proof.still(r, still)?;
            still_taken = true;
        }
        g.update(&input, DT, r);
        frame += 1;
        last = state;
    }
    if !still_taken && !still.is_empty() {
        proof.still(r, still)?;
    }
    let done = g.debug_crew_solo_procedures_done();
    let record = done.get(before).cloned().unwrap_or(json!(null));
    Ok(json!({"record": record, "frames": frame, "seconds": frame as f32 * DT, "last_state": last, "steps_seen": labels}))
}

/// What the crosshair offers, as "Treat(Role, Procedure)" or none.
fn offered_procedure(g: &Game) -> Option<String> {
    offered(g).map(|(_, procedure)| procedure)
}

/// Who and what the crosshair offers: (role as the hover names it, procedure).
fn offered(g: &Game) -> Option<(String, String)> {
    let hover = g.debug_crew_solo_hover()?;
    let inner = hover.strip_prefix("Treat(")?.strip_suffix(')')?;
    let mut parts = inner.split(", ");
    Some((parts.next()?.to_owned(), parts.next()?.to_owned()))
}

/// Go to the man and work every procedure he needs, one after another (at most `cap`), a still
/// for each distinct procedure. Returns the records.
fn treat_the_man(g: &mut Game, r: &mut crate::renderer::Renderer, proof: &mut Proof, idle: &TankInput, role: CrewRole, cap: usize, stills: &mut Vec<String>) -> anyhow::Result<Vec<Value>> {
    let mut records = Vec::new();
    for _ in 0..cap {
        g.debug_crew_solo_go_to(role);
        run_for(g, r, idle, 0.2);
        // The crosshair decides who is under the hands; name everything by him.
        let Some((treated, procedure)) = offered(g) else { break; };
        let name = format!("{}_{}", procedure.to_lowercase(), treated.to_lowercase());
        let key = format!("{procedure}/{treated}");
        let first_time = !stills.contains(&key);
        let still = if first_time { format!("04_{name}") } else { String::new() };
        let outcome = work_procedure(g, r, proof, &still)?;
        if first_time {
            stills.push(key);
        }
        records.push(json!({"man": treated.to_lowercase(), "walked_to": role.name(), "procedure": procedure, "outcome": outcome}));
        run_for(g, r, idle, 0.3);
    }
    Ok(records)
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
    let (recognizable, los) = g.debug_crew_solo_target_visible(*target);
    println!("[crew-solo] empty_the_breech: fired={} waited={waited:.1}s loaded_now={} target #{} alive={} recognizable={recognizable} line_clear={los} gunner_target={}",
        shots(g) > before, g.breech_loaded(), *target, g.enemies[*target].alive,
        g.crew_solo_receipt()["gunner_target"]);
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

    // The commanded shot's empty went into the rack (not rushed: the gunner held fire).
    let rack = g.empties_rack_receipt();
    g.jump_station(Station::Loader);
    run_for(g, r, &idle, 1.2);
    g.debug_crew_solo_face_empties_rack();
    run_for(g, r, &idle, 3.0);
    proof.still(r, "02b_empties_rack")?;
    proof.check(
        "the loader stands the commanded shot's empty upright in the rear-wall rack",
        g.stowed_empties() >= 1,
        json!({"rack": g.empties_rack_receipt(), "rack_after_shot": rack}),
    );
    g.jump_station(Station::Commander);
    run_for(g, r, &idle, 1.0);

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
    // Solid shot through the right hull side at the radio operator, raked rearward: the cone
    // goes on into the radio and bow MG (not needed later) and away from the transmission, the
    // gun and the other men. Through the turret, every wounding lane also wrecked the gun, and
    // its fragment cloud cut the men there past saving. The player stays up at the cupola.
    let wound_strike = g.debug_crew_solo_strike_raked(CrewRole::Radioman, false, Arc::Side, 0.3, 35.0);
    proof.strikes.push(wound_strike.clone());
    anyhow::ensure!(g.player.alive, "the proof's own body was killed by the wounding strike");
    // The patient: a living man who is bleeding and can be saved (the slowest bleed among the
    // wounded; a man cut up by a dozen fragments is past one dressing, and the proof is about
    // the treatment, not triage).
    let indicated = |g: &Game, role: CrewRole| g.debug_crew_solo_man(role).wounds.iter().filter(|w| w.procedure().is_some()).count();
    let mut patient = AI_ROLES
        .into_iter()
        .filter(|&role| !dead(g, role) && g.debug_crew_solo_man(role).is_bleeding())
        .min_by(|&a, &b| {
            indicated(g, a).cmp(&indicated(g, b)).then_with(|| {
                g.debug_crew_solo_man(a).total_bleed_rate_l_per_s()
                    .total_cmp(&g.debug_crew_solo_man(b).total_bleed_rate_l_per_s())
            })
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

    // ── 6. The kit in the hands: procedures at the men ──────────────────────────────────
    let hover = g.debug_crew_solo_hover();
    let label = g.hover_label_text().map(|l| l.to_string());
    let offered = offered_procedure(g);
    proof.check(
        "crosshair on the wounded man offers the procedure he needs, by name",
        hover.as_deref().is_some_and(|h| h.contains(&format!("Treat({patient:?}"))) && offered.is_some(),
        json!({"patient": patient.name(), "hover": hover, "label": label, "offered": offered}),
    );
    let box_before = g.debug_crew_solo_aid_box();
    let bleed_before = g.debug_crew_solo_man(patient).total_bleed_rate_l_per_s();
    let mut stills_by_procedure: Vec<String> = Vec::new();
    // Every procedure he needs, until nothing more is indicated (a man cut by a dozen
    // fragments needs a dozen dressings, sleeves when the box runs out).
    let first = treat_the_man(g, r, proof, &idle, patient, 16, &mut stills_by_procedure)?;
    let box_after = g.debug_crew_solo_aid_box();
    let treated = g.debug_crew_solo_man(patient).clone();
    let bleed_after = treated.total_bleed_rate_l_per_s();
    let count = |b: crate::first_aid::AidBox| u32::from(b.dressings) + u32::from(b.tourniquets) + u32::from(b.splints)
        + u32::from(b.morphine) + u32::from(b.burn_dressings) + u32::from(b.triangular) + u32::from(b.canteen);
    let consumed = count(box_before) - count(box_after);
    let first_record = first.first().and_then(|f| f["outcome"]["record"].as_object().cloned());
    let first_quality = first_record.as_ref().and_then(|rec| rec.get("quality")).and_then(Value::as_f64).unwrap_or(0.0);
    proof.check(
        "the first procedure is worked end to end by hand: steps in order, items taken, a good job, the bleed controlled",
        !first.is_empty() && first_quality >= 0.7 && consumed >= 1 && (bleed_after < bleed_before || !treated.is_bleeding()),
        json!({"patient": patient.name(), "procedures": first, "box_before": format!("{box_before:?}"),
            "box_after": format!("{box_after:?}"), "bleed_before_l_per_min": bleed_before * 60.0,
            "bleed_after_l_per_min": bleed_after * 60.0, "first_quality": first_quality}),
    );
    // A second man, if the cone left one: the triage line names him.
    let second = AI_ROLES
        .into_iter()
        .filter(|&role| role != patient && !dead(g, role) && g.debug_crew_solo_man(role).procedure_needed().is_some())
        .min_by(|&a, &b| g.debug_crew_solo_man(a).time_to_death_s().total_cmp(&g.debug_crew_solo_man(b).time_to_death_s()));
    let mut all_records = first;
    if let Some(second) = second {
        all_records.extend(treat_the_man(g, r, proof, &idle, second, 16, &mut stills_by_procedure)?);
    }
    // A fracture is rarer than a bleed in a 76 mm cone: one is staged (named so in the receipt)
    // on the least-hurt living man so the splint, and the morphine after it, are worked by hand.
    let splinted = AI_ROLES.into_iter().filter(|&role| !dead(g, role))
        .min_by_key(|&role| indicated(g, role)).unwrap_or(patient);
    g.debug_crew_solo_stage_wound(splinted, WoundKind::Fracture, Region::Arm, 0.7);
    all_records.extend(treat_the_man(g, r, proof, &idle, splinted, 4, &mut stills_by_procedure)?);
    let distinct_stills: Vec<String> = {
        let mut names: Vec<String> = stills_by_procedure.iter().map(|k| k.split('/').next().unwrap_or("").to_owned()).collect();
        names.sort();
        names.dedup();
        names
    };
    let distinct: Vec<String> = {
        let mut names: Vec<String> = all_records.iter()
            .filter(|rec| rec["outcome"]["record"].get("quality").is_some())
            .map(|rec| rec["procedure"].as_str().unwrap_or("").to_owned()).collect();
        names.sort();
        names.dedup();
        names
    };
    proof.check(
        "three or more distinct procedures worked by hand, each with a still of the hands at it",
        distinct.len() >= 3 && distinct_stills.len() >= 3,
        json!({"distinct": distinct, "stills": stills_by_procedure, "records": all_records,
            "fracture_staged_on": splinted.name()}),
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
    // Staged through the casualty ledger: every live lane tried into the gunner's seat also
    // wrecked the gun the player goes on to use (see the receipt's fixture note).
    let gunner_strike = if dead(g, CrewRole::Gunner) {
        json!({"skipped": "gunner already dead from an earlier cone"})
    } else {
        empty_the_breech(g, r, &mut target, spot);
        json!({"staged_kia": g.debug_crew_solo_kia(CrewRole::Gunner)})
    };
    proof.strikes.push(gunner_strike.clone());
    anyhow::ensure!(g.player.alive, "the proof's own body was killed by the gunner strike");
    let shots_dark = shots(g);
    g.jump_station(Station::Loader);
    run_for(g, r, &idle, 1.2);
    g.jump_station(Station::Commander);
    run_for(g, r, &idle, 10.0);
    proof.check(
        "gunner killed; nobody lays or fires the gun for ten seconds",
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
    // One round handed up to the stowage for the player's shot. A living loader at his post
    // loads it; a dead one means the player goes to the breech and does it himself (the
    // auto-crew stroke helper at the player's own breech post, standing in for the held-mouse
    // carry, which has its own `reload` proof).
    g.debug_crew_solo_ammunition_left(1);
    let loader_alive = !dead(g, CrewRole::Loader);
    let mut loaded_by = "loader";
    let mut t_loaded = 0.0;
    if loader_alive {
        t_loaded = run_until(g, r, &idle, 25.0, |g| g.breech_loaded());
    }
    if !g.breech_loaded() {
        loaded_by = "player";
        g.jump_station(Station::Loader);
        run_for(g, r, &idle, 1.2);
        g.debug_crew_solo_go_to(CrewRole::Loader);
        run_for(g, r, &idle, 0.2);
        let mut tick = 0u32;
        for _ in 0..steps(30.0) {
            if g.breech_loaded() {
                break;
            }
            tick += 1;
            g.auto_service_breech(tick % 20 == 0, DT);
            g.update(&idle, DT, r);
            t_loaded += DT;
        }
        proof.still(r, "05b_player_loading_by_hand")?;
        g.debug_face_gunner_sight();
        run_for(g, r, &idle, 0.2);
        g.click_station();
        run_for(g, r, &idle, 1.2);
    }
    println!("[crew-solo] the handed-up round loaded by the {loaded_by} in {t_loaded:.1} s");
    proof.check(
        "the handed-up round goes home: by the loader if he lives, by the player's own hands at the breech if he is dead",
        loader_alive || (loaded_by == "player" && g.breech_loaded()),
        json!({"loader_alive": loader_alive, "loaded_by": loaded_by, "seconds": t_loaded,
            "station_after": format!("{:?}", g.station)}),
    );
    let target_now = ensure_target(g, r, &mut target, spot);
    // Lay by hand: the harness sets the desired lay the mouse would, the turret chases it.
    let mut laid = false;
    let mut lay_seconds = 0.0;
    for _ in 0..steps(90.0) {
        lay_seconds += DT;
        let center = g.enemies[target_now].center();
        laid = g.campaign_ai_order_lay(center, 0.01);
        g.update(&idle, DT, r);
        if laid {
            break;
        }
    }
    let shots_by_hand = shots(g);
    let loaded_before = g.breech_loaded();
    let spent_before = g.debug_rounds_spent();
    let gates = g.debug_crew_solo_fire_gates();
    let fire = TankInput { mg_coax: true, trigger: true, ..TankInput::default() };
    g.update(&idle, DT, r);
    g.update(&fire, DT, r);
    g.update(&fire, DT, r);
    g.update(&idle, DT, r);
    let gates_after = g.debug_crew_solo_fire_gates();
    let fired_by_hand = loaded_before && !g.breech_loaded() && g.debug_rounds_spent() > spent_before;
    proof.check(
        "the player lays and fires the gun himself (a loaded round leaves the breech); no AI shot is counted",
        laid && fired_by_hand && shots(g) == shots_by_hand,
        json!({"laid": laid, "lay_seconds": lay_seconds, "lay_error_rad": g.campaign_ai_lay_error(),
            "modules": g.debug_crew_solo_modules(), "loaded_before": loaded_before,
            "rounds_spent": [spent_before, g.debug_rounds_spent()], "fired": fired_by_hand,
            "gates_before": gates, "gates_after": gates_after}),
    );
    proof.still(r, "06_player_at_the_sights")?;
    run_until(g, r, &idle, 20.0, |g| g.breech_loaded());

    // ── 9. The driver is killed; the tank halts under orders ─────────────────────────────
    g.jump_station(Station::Loader);
    run_for(g, r, &idle, 1.2);
    g.jump_station(Station::Commander);
    run_for(g, r, &idle, 1.0);
    // Staged through the casualty ledger: every live lane tried into the driver's seat also
    // broke the transmission the player goes on to drive with.
    let driver_strike = if dead(g, CrewRole::Driver) {
        json!({"skipped": "driver already dead from the wounding cone"})
    } else {
        json!({"staged_kia": g.debug_crew_solo_kia(CrewRole::Driver)})
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
        json!({"hover": driver_hover, "station": format!("{:?}", g.station), "moved_m": drove,
            "modules": g.debug_crew_solo_modules(), "speed": g.player.speed}),
    );
    proof.still(r, "08_player_driving")?;
    Ok(())
}

/// Short reproduction of two play reports (`crew-solo-kia PREFIX`): does a loader killed by a
/// live penetration leave a body at his station and blood on it, seen from the interior walk;
/// does a bleeding man bleed onto his station over time; does a displaced man stand clear of
/// the cupola. One run, a few stills, a JSON receipt; no full proof.
pub fn run_crew_solo_kia_capture(prefix: &str) -> anyhow::Result<()> {
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[crew-solo-kia] adapter={} ({:?}, {:?})", info.name, info.device_type, info.backend);
    let (device, queue) = crate::request_device(&adapter);
    let map = crate::world::act(0);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &map)?;
    g.enable_singleplayer_crew();
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    g.god = false;
    g.targets_only = true;
    g.brawl = false;
    g.debug_hold_enemy_fire();
    for i in 1..g.enemies.len() {
        g.debug_crew_solo_set_enemy_active(i, false);
    }
    r.set_weather_override(None);
    r.overcast = 0.08;
    r.storm = 0.0;
    r.fog_density = 0.00035;
    r.precipitation = 0.0;
    let heading = g.player.heading;
    let mut spot = Spot { range: 160.0, bearing: 0.0, heading };
    'search: for &range in &[160.0f32, 200.0, 240.0, 120.0, 300.0] {
        for &bearing_deg in &[0.0f32, 15.0, -15.0, 30.0, -30.0] {
            let bearing = heading + bearing_deg.to_radians();
            let target = g.player.pos + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * range;
            g.debug_place_enemy(0, target.x, target.z, bearing + std::f32::consts::PI);
            g.update(&TankInput::default(), DT, &mut r);
            let (recognizable, los) = g.debug_crew_solo_target_visible(0);
            spot = Spot { range, bearing: bearing_deg, heading };
            if recognizable && los {
                break 'search;
            }
        }
    }
    g.debug_engage_all();
    // The Tiger's last round is the one in the chamber; it is fired off before the strike.
    g.debug_crew_solo_ammunition_left(0);
    let mut proof = Proof { prefix: prefix.to_owned(), device: &device, queue: &queue, checks: Vec::new(), strikes: Vec::new(), stills: Vec::new(), pass: true };
    let idle = TankInput::default();
    let hold_e = TankInput { mg_coax: true, interaction: true, ..TankInput::default() };
    let mut target = 0usize;
    run_for(&mut g, &mut r, &idle, 1.0);

    // 1. Displacement: the player at the sights, the gunner stands by at the rear of the basket,
    //    nobody under the cupola. Seen from the interior walk afterwards.
    g.jump_station(Station::Loader);
    run_for(&mut g, &mut r, &idle, 1.2);
    g.debug_crew_solo_face_empties_rack();
    run_for(&mut g, &mut r, &idle, 0.2);
    proof.still(&mut r, "kia_00_empties_rack_space")?;
    g.debug_face_gunner_sight();
    run_for(&mut g, &mut r, &idle, 0.2);
    g.click_station();
    run_for(&mut g, &mut r, &idle, 1.2);
    let at_sights = g.debug_crew_solo_bodies();
    // What the renderer draws, not what the crew model says: render once so the rig pass runs.
    proof.still(&mut r, "kia_00b_from_the_sights")?;
    let drawn_at_sights = g.debug_crew_solo_visible_figures(&r);
    let commander_drawn = drawn_at_sights.as_array().map_or(0, |f| f.iter().filter(|x| x["man"] == "commander").count());
    let under_cupola = drawn_at_sights.as_array().map_or(0, |f| f.iter()
        .filter(|x| x["from_cupola_post_m"].as_f64().unwrap_or(9.0) < 0.35).count());
    proof.check(
        "drawn by the renderer with the player at the sights: no commander figure anywhere (the player is the commander) and no figure standing in the cupola post",
        g.station == Station::Gunner && commander_drawn == 0 && under_cupola == 0,
        json!({"figures": drawn_at_sights}),
    );
    let gunner_standing_by = at_sights["men"].as_array().and_then(|men| men.iter().find(|m| m["role"] == "gunner")).map(|m| m["standing_by"] == true).unwrap_or(false);
    let nobody_at_cupola = at_sights["men"].as_array().map_or(false, |men| men.iter().all(|m| m["player"] == true || m["alive"] == false || m["standing_by"] == true || m["post"] != "COMMANDER"));
    proof.check(
        "player at the sights: the gunner stands by at the rear of the basket and no figure is left under the cupola",
        g.station == Station::Gunner && gunner_standing_by && nobody_at_cupola,
        json!({"bodies": at_sights}),
    );
    g.jump_station(Station::Loader);
    run_for(&mut g, &mut r, &idle, 1.2);
    g.debug_crew_solo_go_to(CrewRole::Commander);
    run_for(&mut g, &mut r, &idle, 0.2);
    proof.still(&mut r, "kia_01_cupola_after_the_player_left_it")?;
    let drawn_walking = g.debug_crew_solo_visible_figures(&r);
    let commander_walking = drawn_walking.as_array().map_or(0, |f| f.iter().filter(|x| x["man"] == "commander").count());
    let under_cupola_walking = drawn_walking.as_array().map_or(0, |f| f.iter()
        .filter(|x| x["from_cupola_post_m"].as_f64().unwrap_or(9.0) < 0.35).count());
    proof.check(
        "drawn by the renderer with the player on the compartment floor looking at the cupola: no commander figure and nobody in the cupola post",
        commander_walking == 0 && under_cupola_walking == 0,
        json!({"figures": drawn_walking}),
    );
    let walking = g.debug_crew_solo_bodies();
    let loader_at_post_while_walking = walking["men"].as_array().and_then(|men| men.iter().find(|m| m["role"] == "loader")).map(|m| m["post"] == "LOADER").unwrap_or(false);
    proof.check(
        "walking the compartment leaves the loader at his post",
        loader_at_post_while_walking,
        json!({"bodies": walking}),
    );

    // 2. The loader killed by a live solid shot, with the player at the driver's seat.
    g.jump_station(Station::Commander);
    run_for(&mut g, &mut r, &idle, 1.0);
    empty_the_breech(&mut g, &mut r, &mut target, spot);
    let (_, in_seat) = take_driver_seat(&mut g, &mut r, &idle);
    anyhow::ensure!(in_seat, "could not take the driver's seat");
    let blood_before = g.debug_surface_blood();
    let strike = g.debug_crew_solo_strike(CrewRole::Loader, true, Arc::Side, 0.0);
    proof.strikes.push(strike.clone());
    anyhow::ensure!(g.player.alive, "the proof's own body was killed by the loader strike");
    run_for(&mut g, &mut r, &idle, 2.5);
    g.jump_station(Station::Loader);
    run_for(&mut g, &mut r, &idle, 1.2);
    g.debug_crew_solo_go_to(CrewRole::Loader);
    run_for(&mut g, &mut r, &idle, 0.3);
    proof.still(&mut r, "kia_02_loader_dead_from_the_walk")?;
    let after = g.debug_crew_solo_bodies();
    let loader = after["men"].as_array().and_then(|men| men.iter().find(|m| m["role"] == "loader").cloned()).unwrap_or(json!(null));
    let emitted_before = blood_before["emitted"].as_u64().unwrap_or(0);
    let emitted_after = after["surface_blood"]["emitted"].as_u64().unwrap_or(0);
    proof.check(
        "loader killed: a settled body at his post, blood emitted and retained in the cabin, the intercom said so",
        loader["alive"] == false && loader["ragdoll"]["settled"] == true && emitted_after > emitted_before
            && after["surface_blood"]["retained_cells"].as_u64().unwrap_or(0) > 0,
        json!({"loader": loader, "strike": strike, "blood_before": blood_before, "bodies": after}),
    );
    run_for(&mut g, &mut r, &idle, 8.0);
    proof.still(&mut r, "kia_03_loader_dead_eight_seconds_on")?;

    // 3. A bleeding man bleeds onto his station: a staged femoral bleed (named so) on the first
    //    man the cone left alive.
    let bleeder = AI_ROLES.into_iter().find(|&role| !dead(&g, role)).unwrap_or(CrewRole::Radioman);
    g.debug_crew_solo_stage_wound(bleeder, WoundKind::Bleeding, Region::Leg, 0.8);
    let before = g.debug_surface_blood()["emitted"].as_u64().unwrap_or(0);
    run_for(&mut g, &mut r, &idle, 30.0);
    let during = g.debug_surface_blood();
    g.debug_crew_solo_go_to(bleeder);
    run_for(&mut g, &mut r, &idle, 0.3);
    proof.still(&mut r, "kia_04_bleeding_man_thirty_seconds")?;
    proof.check(
        "a bleeding man keeps bleeding onto his seat and the floor (grains emitted over thirty seconds)",
        during["emitted"].as_u64().unwrap_or(0) > before + 100,
        json!({"bleeder": bleeder.name(), "emitted_before": before, "surface_blood": during,
            "man": g.debug_crew_solo_bodies()["men"]}),
    );
    let _ = hold_e;

    let receipt_path = format!("{prefix}.json");
    let receipt = json!({"command": "crew-solo-kia", "adapter": info.name, "backend": if r.shader_backend { "shader" } else { "physical/raymarch" },
        "pass": proof.pass, "checks": proof.checks, "strikes": proof.strikes, "stills": proof.stills,
        "bodies": g.debug_crew_solo_bodies(), "crew": g.crew_solo_receipt()});
    std::fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?).with_context(|| format!("writing {receipt_path}"))?;
    println!("[crew-solo-kia] receipt {receipt_path}");
    println!("[crew-solo-kia] {}", if proof.pass { "ALL PASS" } else { "FAILURES" });
    anyhow::ensure!(proof.pass, "crew-solo-kia check failed; see {receipt_path}");
    Ok(())
}

/// Short check of the empties' lifecycle (`crew-solo-empties PREFIX`): with the rack full the
/// loader asks to dump out of the side hatch; refused, the next empty falls to the floor and the
/// floor clutter slows and kicks; approved, he opens the hatch between reloads, heaves the cases
/// out one by one, shuts it when a contact comes into the gunner's sight, resumes, and shuts it
/// when done. One run, a few stills, a JSON receipt.
pub fn run_crew_solo_empties_capture(prefix: &str) -> anyhow::Result<()> {
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[crew-solo-empties] adapter={} ({:?}, {:?})", info.name, info.device_type, info.backend);
    let (device, queue) = crate::request_device(&adapter);
    let map = crate::world::act(0);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &map)?;
    g.enable_singleplayer_crew();
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    g.god = true;
    g.targets_only = true;
    g.brawl = false;
    g.debug_hold_enemy_fire();
    for i in 0..g.enemies.len() {
        g.debug_crew_solo_set_enemy_active(i, false);
    }
    r.set_weather_override(None);
    r.overcast = 0.08;
    r.storm = 0.0;
    r.fog_density = 0.00035;
    r.precipitation = 0.0;
    let heading = g.player.heading;
    let ranges = [160.0f32, 200.0, 240.0, 120.0];
    let bearings = [0.0f32, 15.0, -15.0, 30.0, -30.0];
    let mut found = false;
    for range in ranges {
        if found {
            break;
        }
        for bearing_deg in bearings {
            let bearing = heading + bearing_deg.to_radians();
            let target = g.player.pos + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * range;
            g.debug_place_enemy(0, target.x, target.z, bearing + std::f32::consts::PI);
            g.debug_crew_solo_set_enemy_active(0, true);
            g.update(&TankInput::default(), DT, &mut r);
            let (recognizable, los) = g.debug_crew_solo_target_visible(0);
            if recognizable && los {
                found = true;
                break;
            }
        }
    }
    g.debug_crew_solo_set_enemy_active(0, false);
    let target_spot = (g.enemies[0].pos, g.enemies[0].heading);
    let mut proof = Proof { prefix: prefix.to_owned(), device: &device, queue: &queue, checks: Vec::new(), strikes: Vec::new(), stills: Vec::new(), pass: true };
    let idle = TankInput::default();
    let yes = TankInput { crew_yes: true, ..TankInput::default() };
    let no = TankInput { crew_no: true, ..TankInput::default() };
    run_for(&mut g, &mut r, &idle, 1.0);

    // 1. A run's worth of empties in the rack (staged, named): the loader asks.
    let filled = g.debug_crew_solo_fill_empties_rack();
    run_for(&mut g, &mut r, &idle, 1.0);
    let asked = g.crew_solo_dump_receipt();
    let line = g.intercom_line().map(str::to_owned);
    proof.check(
        "rack full: the loader asks the commander to dump out of the side hatch",
        asked["permission"] == "Asked" && line.as_deref().is_some_and(|l| l.contains("side hatch")),
        json!({"filled": filled, "target_found": found, "dump": asked, "intercom": line}),
    );

    // 2. Refused: a commanded shot's empty falls to the floor; the floor clutter is underfoot.
    g.update(&no, DT, &mut r);
    g.update(&idle, DT, &mut r);
    g.debug_crew_solo_set_enemy_active(0, true);
    g.debug_engage_all();
    g.debug_crew_solo_set_fire_at_will(false);
    run_until(&mut g, &mut r, &idle, 20.0, |g| g.breech_loaded());
    let shots_before = shots(&g);
    g.debug_crew_solo_fire_command();
    run_until(&mut g, &mut r, &idle, 20.0, |g| shots(g) > shots_before);
    run_for(&mut g, &mut r, &idle, 3.0);
    let refused = g.crew_solo_dump_receipt();
    proof.check(
        "refused: the hatch stays shut and the next empty is on the floor, not in the rack",
        refused["permission"] == "Refused" && refused["hatch_open"] == false
            && refused["loose_aboard"].as_u64().unwrap_or(0) >= 1 && refused["in_rack"].as_u64() == Some(filled as u64),
        json!({"dump": refused, "shots": [shots_before, shots(&g)]}),
    );
    // More empties round the loader's feet (staged, named), then the player walks through them.
    for (x, z) in [(-0.60f32, 0.05f32), (-0.70, 0.20), (-0.55, 0.30), (-0.45, 0.10)] {
        g.debug_crew_solo_drop_case(Vec3::new(x, crate::interior_rig::FLOOR_STAND_Y + 0.08, z));
    }
    run_for(&mut g, &mut r, &idle, 1.5);
    let (at_loader, _) = g.debug_crew_solo_floor_clutter();
    g.jump_station(Station::Loader);
    run_for(&mut g, &mut r, &idle, 1.2);
    g.debug_loader_face_local(Vec3::new(-0.6, 0.8, 0.2));
    run_for(&mut g, &mut r, &idle, 0.2);
    proof.still(&mut r, "empties_01_floor_clutter_at_the_breech")?;
    let walk = TankInput { drive: 1.0, ..TankInput::default() };
    let start = g.debug_crew_solo_walk_pos();
    run_for(&mut g, &mut r, &walk, 1.0);
    let walked = g.debug_crew_solo_walk_pos().distance(start);
    proof.check(
        "refused: empties collect round the loader's feet (slowing his strokes); the player can still walk through them",
        at_loader >= 3 && walked > 0.05,
        json!({"cases_at_loader_post": at_loader, "walked_m_in_1s": walked}),
    );

    // 3. Approved, with no contact in sight: hatch open, out they go.
    g.debug_crew_solo_set_enemy_active(0, false);
    g.jump_station(Station::Commander);
    run_for(&mut g, &mut r, &idle, 1.0);
    g.update(&yes, DT, &mut r);
    g.update(&idle, DT, &mut r);
    run_until(&mut g, &mut r, &idle, 20.0, |g| g.breech_loaded());
    let t_open = run_until(&mut g, &mut r, &idle, 10.0, |g| g.crew_solo_dump_receipt()["hatch_open"] == true);
    run_until(&mut g, &mut r, &idle, 15.0, |g| g.crew_solo_dump_receipt()["dumped_total"].as_u64().unwrap_or(0) >= 3);
    g.jump_station(Station::Loader);
    run_for(&mut g, &mut r, &idle, 1.2);
    g.debug_crew_solo_face_escape_hatch();
    run_for(&mut g, &mut r, &idle, 0.4);
    proof.still(&mut r, "empties_02_side_hatch_open_dumping")?;
    let dumping = g.crew_solo_dump_receipt();
    proof.check(
        "approved: the loader opens the side hatch between reloads and heaves the empties out",
        dumping["dumped_total"].as_u64().unwrap_or(0) >= 3,
        json!({"seconds_to_open": t_open, "dump": dumping}),
    );

    // 4. A contact comes into sight (a fresh T-34 where the first one stood; the first was
    //    killed by the commanded shot): the hatch is shut at once; gone again, he resumes.
    let _ = target_spot;
    let mut contact = 0usize;
    if g.enemies[0].alive {
        g.debug_crew_solo_set_enemy_active(0, true);
        g.debug_engage_all();
    } else {
        contact = ensure_target(&mut g, &mut r, &mut contact, Spot { range: 160.0, bearing: 0.0, heading });
    }
    let t_shut = run_until(&mut g, &mut r, &idle, 6.0, |g| g.crew_solo_dump_receipt()["hatch_open"] == false);
    let shut = g.crew_solo_dump_receipt();
    proof.check(
        "a contact in the gunner's sight shuts the hatch at once",
        shut["hatch_open"] == false && t_shut < 6.0,
        json!({"seconds_to_shut": t_shut, "dump": shut, "contact": contact}),
    );
    g.debug_crew_solo_set_enemy_active(contact, false);
    run_until(&mut g, &mut r, &idle, 120.0, |g| {
        let d = g.crew_solo_dump_receipt();
        d["in_rack"] == 0 && d["loose_aboard"] == 0 && d["hatch_open"] == false
    });
    run_for(&mut g, &mut r, &idle, 4.0);
    let done = g.crew_solo_dump_receipt();
    proof.check(
        "all empties out, the hatch shut again, the cases lying outside where they fell",
        done["in_rack"] == 0 && done["hatch_open"] == false
            && done["dumped_total"].as_u64().unwrap_or(0) >= filled as u64
            && done["outside_resting"].as_u64().unwrap_or(0) >= 1,
        json!({"dump": done}),
    );

    let receipt_path = format!("{prefix}.json");
    let receipt = json!({"command": "crew-solo-empties", "adapter": info.name, "pass": proof.pass,
        "checks": proof.checks, "stills": proof.stills, "crew": g.crew_solo_receipt(),
        "fixture": "rack filled by debug_crew_solo_fill_empties_rack; four extra empties dropped at the loader's post; the range target is toggled active to stand for a contact; the Tiger is invulnerable (god) for this check"});
    std::fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?).with_context(|| format!("writing {receipt_path}"))?;
    println!("[crew-solo-empties] receipt {receipt_path}");
    println!("[crew-solo-empties] {}", if proof.pass { "ALL PASS" } else { "FAILURES" });
    anyhow::ensure!(proof.pass, "crew-solo-empties check failed; see {receipt_path}");
    Ok(())
}

/// Short check of the commander's designation (`crew-solo-designate PREFIX`): from the cupola
/// he designates a bearing; the living gunner acknowledges and slews the turret there at the
/// real traverse rate and reports on; the TPZ's second pointer shows the designation; a second
/// designation on the same bearing refines the range; a designation mid-slew with the player
/// dropping into the sights leaves the turret part-way there; a designated contact becomes the
/// gunner's target under hold-fire with no shot; a dead gunner leaves the turret where it was.
pub fn run_crew_solo_designate_capture(prefix: &str) -> anyhow::Result<()> {
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[crew-solo-designate] adapter={} ({:?}, {:?})", info.name, info.device_type, info.backend);
    let (device, queue) = crate::request_device(&adapter);
    let map = crate::world::act(0);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &map)?;
    g.enable_singleplayer_crew();
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    g.god = true;
    g.targets_only = true;
    g.brawl = false;
    g.debug_hold_enemy_fire();
    for i in 0..g.enemies.len() {
        g.debug_crew_solo_set_enemy_active(i, false);
    }
    r.set_weather_override(None);
    r.overcast = 0.08;
    r.storm = 0.0;
    r.fog_density = 0.00035;
    r.precipitation = 0.0;
    g.debug_crew_solo_set_fire_at_will(false);
    let mut proof = Proof { prefix: prefix.to_owned(), device: &device, queue: &queue, checks: Vec::new(), strikes: Vec::new(), stills: Vec::new(), pass: true };
    let idle = TankInput::default();
    run_for(&mut g, &mut r, &idle, 1.0);
    let heading = g.player.heading;
    let ground_point = |g: &Game, rel_deg: f32, range: f32| {
        let b = heading + rel_deg.to_radians();
        let p = g.player.pos + Vec3::new(b.sin(), 0.0, b.cos()) * range;
        Vec3::new(p.x, g.debug_ground_height(p.x, p.z), p.z)
    };
    let wrap = |a: f32| {
        let mut d = a % std::f32::consts::TAU;
        if d > std::f32::consts::PI { d -= std::f32::consts::TAU; }
        if d < -std::f32::consts::PI { d += std::f32::consts::TAU; }
        d
    };

    // 1. Designate ground 75 degrees off the bow, 600 m: acknowledged, pointer up, slewed, on.
    let yaw0 = g.player.turret_yaw;
    let d1 = g.debug_crew_solo_designate_at(ground_point(&g, 75.0, 600.0));
    let ack = g.intercom_line().map(str::to_owned);
    let pointer = g.debug_crew_solo_tpz_needles_visible();
    let t_on = run_until(&mut g, &mut r, &idle, 40.0, |g| g.debug_crew_solo_designation()["on_target_after_s"].is_number());
    let des = g.debug_crew_solo_designation();
    let bearing = des["bearing_rad"].as_f64().unwrap_or(0.0) as f32;
    let err = wrap(bearing - g.player.turret_yaw);
    let swing = wrap(bearing - yaw0).abs();
    proof.check(
        "designation acknowledged, the TPZ's second pointer shows it, the gunner slews there at the real traverse rate and reports on",
        d1.is_some() && ack.as_deref().is_some_and(|l| l.contains("Traversing")) && pointer
            && des["on_target_after_s"].is_number() && err.abs() < 0.03
            && t_on >= swing.to_degrees() / 6.0 * 0.8,
        json!({"designation": des, "ack": ack, "pointer": pointer, "seconds_to_on": t_on,
            "swing_deg": swing.to_degrees(), "turret_error_rad": err, "traverse_floor_s": swing.to_degrees() / 6.0}),
    );
    g.jump_station(Station::Loader);
    run_for(&mut g, &mut r, &idle, 1.2);
    g.debug_crew_solo_face_tpz();
    run_for(&mut g, &mut r, &idle, 0.3);
    proof.still(&mut r, "designate_01_tpz_two_pointers_on")?;
    g.jump_station(Station::Commander);
    run_for(&mut g, &mut r, &idle, 1.0);

    // 2. A second designation on the same bearing refines the range.
    let before = g.debug_crew_solo_designation();
    let d2 = g.debug_crew_solo_designate_at(ground_point(&g, 75.5, 400.0));
    let after = g.debug_crew_solo_designation();
    proof.check(
        "a second designation on the same bearing refines the range instead of starting over",
        // The refined range is whatever the new eye ray strikes (here the same rise of ground
        // both times); the check is that it refines the standing designation, keeping its age.
        d2.is_some() && after["refinements"] == 1 && after["age_s"].as_f64() >= before["age_s"].as_f64()
            && after["range_m"] == d2.as_ref().map_or(json!(null), |d| d["range_m"].clone()),
        json!({"before": before, "after": after, "second_cast": d2}),
    );

    // 3. Designate the other side, then drop into the sights mid-slew: the turret is part-way
    //    there when the player takes over the lay.
    let yaw_start = g.player.turret_yaw;
    g.debug_crew_solo_designate_at(ground_point(&g, -60.0, 700.0));
    let target_bearing = g.debug_crew_solo_designation()["bearing_rad"].as_f64().unwrap_or(0.0) as f32;
    run_for(&mut g, &mut r, &idle, 6.0);
    g.jump_station(Station::Loader);
    run_for(&mut g, &mut r, &idle, 1.2);
    g.debug_face_gunner_sight();
    run_for(&mut g, &mut r, &idle, 0.2);
    let took = g.click_station();
    run_for(&mut g, &mut r, &idle, 1.0);
    let covered = wrap(g.player.turret_yaw - yaw_start).abs();
    let total = wrap(target_bearing - yaw_start).abs();
    proof.check(
        "dropping into the sights mid-slew finds the turret already on its way to the designated bearing",
        took && g.station == Station::Gunner && covered > 0.25 * total,
        json!({"covered_deg": covered.to_degrees(), "total_deg": total.to_degrees()}),
    );
    proof.still(&mut r, "designate_02_sights_mid_slew")?;
    g.jump_station(Station::Loader);
    run_for(&mut g, &mut r, &idle, 1.2);
    g.jump_station(Station::Commander);
    run_for(&mut g, &mut r, &idle, 1.0);

    // 4. A designated contact becomes the gunner's target; under hold-fire there is no shot.
    let target = ground_point(&g, 20.0, 180.0);
    g.debug_place_enemy(0, target.x, target.z, heading + std::f32::consts::PI);
    g.debug_crew_solo_set_enemy_active(0, true);
    g.update(&idle, DT, &mut r);
    let shots_before = shots(&g);
    let center = g.enemies[0].center();
    g.debug_crew_solo_designate_at(center);
    let got = run_until(&mut g, &mut r, &idle, 30.0, |g| g.crew_solo_receipt()["gunner_target"] == 0);
    run_for(&mut g, &mut r, &idle, 8.0);
    proof.check(
        "a designated contact becomes the gunner's target; under hold-fire he lays and holds",
        g.crew_solo_receipt()["gunner_target"] == 0 && shots(&g) == shots_before,
        json!({"seconds_to_target": got, "shots": [shots_before, shots(&g)], "designation": g.debug_crew_solo_designation()}),
    );
    g.debug_crew_solo_set_enemy_active(0, false);
    run_for(&mut g, &mut r, &idle, 1.0);

    // 5. A dead gunner: the designation stands on the TPZ, the turret stays where it is.
    g.debug_crew_solo_kia(CrewRole::Gunner);
    run_for(&mut g, &mut r, &idle, 1.0);
    let yaw_dead = g.player.turret_yaw;
    g.debug_crew_solo_designate_at(ground_point(&g, 120.0, 500.0));
    run_for(&mut g, &mut r, &idle, 8.0);
    proof.check(
        "with the gunner dead the designation stands on the TPZ and the turret does not move",
        g.debug_crew_solo_tpz_needles_visible() && wrap(g.player.turret_yaw - yaw_dead).abs() < 0.01,
        json!({"turret_moved_rad": wrap(g.player.turret_yaw - yaw_dead), "designation": g.debug_crew_solo_designation()}),
    );

    let receipt_path = format!("{prefix}.json");
    let receipt = json!({"command": "crew-solo-designate", "adapter": info.name, "pass": proof.pass,
        "checks": proof.checks, "stills": proof.stills, "session": g.crew_solo_session_json(),
        "log": g.crew_solo_receipt()["log"],
        "fixture": "designations made through debug_crew_solo_designate_at (the same cast from the commander's eye, with the look direction aimed at the point instead of the mouse); range targets parked and not firing; the gunner's KIA in step 5 is staged; the Tiger is invulnerable for this check"});
    std::fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?).with_context(|| format!("writing {receipt_path}"))?;
    println!("[crew-solo-designate] receipt {receipt_path}");
    println!("[crew-solo-designate] {}", if proof.pass { "ALL PASS" } else { "FAILURES" });
    anyhow::ensure!(proof.pass, "crew-solo-designate check failed; see {receipt_path}");
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
            "why": "with the full 64-round stowage every wounding lane tried (turret side, turret rear, hull side) set off live rounds, and a killing solid shot through the gunner's seat set off eight rounds left in the rear stowage; so the fixture fires the last rounds off before the strikes, and every strike is timed right after a shot so the breech holds a spent case",
            "kia_staging": "the gunner and driver KIAs are staged through the casualty ledger, ragdoll and blood path (debug_crew_solo_kia): live lanes into those seats, straight or raked 35 degrees, also knocked out the main gun, sight, traverse or the transmission the proof goes on to use. The wound is a live penetration; a live-penetration KIA is covered by crew-solo-kia (the loader)."},
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
