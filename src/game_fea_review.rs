//! `fea-play PREFIX`: the layered damage model in play on the GPU (docs/FEA_DAMAGE_MODEL_20261102.md,
//! "Stage 3 in play"). The armor-scars review's scene (Canyon v2 winter workshop, act 1, the Tiger
//! alone with its racks empty): real rounds through the ordinary flight sweep and terminal resolver
//! at the player's Tiger, then the loader's hands, the field repair and an interior burst. Stills
//! and a JSON receipt with a PASS/FAIL line per check. Observer cameras outside; the loader's own
//! camera inside. Disclosed harness actions: the station and on-foot placement are set directly,
//! the toolbox is put in hand, a lid is opened by setting its state, and the long cooling wait
//! runs the ordinary update at 30 Hz. No image grading. Requires KESSEL_RENDERER=raymarch.
use super::*;
use crate::{act_manifest, celestial_sky, ephemeris, graphics, world};
use anyhow::Context;
use serde_json::json;

const TIGER_AT: Vec2 = Vec2::new(-450.0, -410.0);
/// The longest cooling wait before the hands beat gives up (game seconds).
const COOL_WAIT_MAX_S: f32 = 900.0;

struct Rig<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: String,
    cw: u32,
    ch: u32,
    stills: Vec<serde_json::Value>,
    checks: Vec<serde_json::Value>,
}

impl Rig<'_> {
    fn check(&mut self, name: &str, pass: bool, detail: String) -> bool {
        println!("[fea-play] {} {name}: {detail}", if pass { "PASS" } else { "FAIL" });
        self.checks.push(json!({"check": name, "pass": pass, "detail": detail}));
        pass
    }

    /// One still: an explicit observer camera outside, or the game's own camera inside.
    fn still(&mut self, g: &Game, r: &mut Renderer, name: &str, note: &str,
        view: Option<(Vec3, Vec3, f32)>) -> anyhow::Result<()> {
        g.drive_armor_scars(r);
        if let Some((eye, at, fov)) = view {
            r.camera.pos = eye;
            r.camera.set_forward(at - eye);
            r.camera.fov_y = fov.to_radians();
            r.optic_mask = 0.0;
            r.reticle = false;
            r.interior_light = 0.0;
            r.interior_haze = 0.0;
            r.weather_shelter = 0.0;
            r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
        }
        r.reset_history();
        let settled = crate::converge_headless_screenshot(r, self.device, self.queue, 16)?;
        let path = format!("{}_{name}.png", self.prefix);
        r.render_to_png(self.device, self.queue, &path, self.cw, self.ch)?;
        println!("[fea-play] {path}: {note}");
        self.stills.push(json!({"name": name, "path": path, "note": note, "eye": r.camera.pos.to_array(),
            "forward": r.camera.forward().to_array(), "exposure": settled.measured_exposure,
            "hover_label": g.hover_label}));
        Ok(())
    }
}

fn step(g: &mut Game, r: &mut Renderer, frames: u32, input: &TankInput) {
    for _ in 0..frames {
        g.update(input, 1.0 / 60.0, r);
    }
}

fn press() -> TankInput {
    TankInput { mg_coax: true, trigger: true, fire2: true, ..TankInput::default() }
}

fn lodged_json(round: &game_fea::LodgedRound) -> serde_json::Value {
    let body = game_fea::penetrator_body(round.shell);
    json!({"id": round.id, "shell": round.shell.name, "in_struck_plate": round.in_struck_plate,
        "component": round.component, "depth_mm": round.depth_m * 1000.0, "removal": format!("{:?}", round.extraction.removal),
        "pull_kn": round.extraction.force_n / 1000.0, "drawn_length_mm": body.length_m * 1000.0,
        "drawn_diameter_mm": body.radius_m * 2000.0, "tip_local": round.tip_local.to_array()})
}

/// Resting steel the lattice made: (slot, world position, temperature K).
fn resting_solved(g: &Game) -> Vec<(usize, Vec3, f32)> {
    g.hot_fragments.iter().enumerate()
        .filter(|(_, f)| f.active && f.settled && f.temp_k > 0.0)
        .map(|(i, f)| (i, blood_parent_model(&g.player, f.frame).transform_point3(f.pos), f.temp_k))
        .collect()
}

/// Turn the loader's head onto each candidate chip in reach and take the first one the ordinary
/// hover offers; returns its slot.
fn find_pickable(g: &mut Game, r: &mut Renderer, skip: &[usize]) -> Option<usize> {
    let eye = g.station_eye(Station::Loader);
    let mut candidates: Vec<(usize, Vec3, f32)> = resting_solved(g).into_iter()
        .filter(|(i, p, _)| !skip.contains(i) && p.distance(eye) < 2.3
            && game_fea::fragment_holdable(&g.hot_fragments[*i]) && game_fea::fragment_pullable(&g.hot_fragments[*i]))
        .collect();
    candidates.sort_by(|a, b| a.1.distance(eye).total_cmp(&b.1.distance(eye)));
    for (i, p, _) in candidates.into_iter().take(40) {
        g.loader_face_world(p);
        step(g, r, 2, &TankInput::default());
        if g.hover == Some(Hotspot::Fragment(i)) {
            return Some(i);
        }
    }
    None
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(graphics::raymarch_backend_enabled(), "fea-play requires KESSEL_RENDERER=raymarch");
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_receipt.json")).exists(),
        "preserving an existing review; choose a new prefix");
    let started = std::time::Instant::now();
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3,
        stills: Vec::new(), checks: Vec::new() };
    let idle = TankInput::default();
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.god = false;
    g.debug_place_tiger(TIGER_AT, 0.0);
    g.debug_empty_ammunition();
    r.set_weather_override(Some(0.0));
    let instant = ephemeris::UtcInstant::parse_iso("1942-12-21T10:30:00Z")?;
    r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
    step(&mut g, &mut r, 60, &idle);
    let mut receipt = serde_json::Map::new();

    // ── 1. A full-calibre round lodged in the lower front plate (the cross-section case
    // front_lower_45ap: it stops 63 mm in, past its 54 mm nose).
    let hull = g.player.hull_model();
    g.debug_launch_round(&crate::ballistics::BR_240_M42, false, hull.transform_point3(Vec3::new(-0.04, 1.0, 2.0)),
        hull.transform_vector3(Vec3::new(0.0, -0.03, -1.0)), 100.0, 0x5EA1_0001);
    step(&mut g, &mut r, 6, &idle);
    let committed_before = g.fea.pending.len();
    step(&mut g, &mut r, 30, &idle);
    let lodged = g.fea.lodged.iter().find(|l| l.in_struck_plate).cloned();
    rig.check("a 45 mm AP round lodged in the lower front plate", lodged.is_some(),
        format!("{:?}", g.fea.lodged.iter().map(lodged_json).collect::<Vec<_>>()));
    rig.check("its plate solve committed 0.25 s later", committed_before >= 1 && g.fea.pending.is_empty(),
        format!("{committed_before} pending at 0.1 s, {} after 0.6 s", g.fea.pending.len()));
    receipt.insert("lodged_after_strike".into(), json!(g.fea.lodged.iter().map(lodged_json).collect::<Vec<_>>()));
    let lodged_view = |g: &Game, round: &game_fea::LodgedRound, back: f32, side: f32, up: f32| {
        let point = g.fea_lodged_point(round.id).unwrap_or(g.player.pos);
        let model = g.player.hull_model();
        let local = model.inverse().transform_point3(point);
        (model.transform_point3(local + Vec3::new(side, up, back)), point, 40.0)
    };
    if let Some(round) = &lodged {
        let view = lodged_view(&g, round, 2.4, -1.2, 0.5);
        rig.still(&g, &mut r, "lodged_front", "the 45 mm round held by the lower front plate, its base outside", Some(view))?;
        let view = lodged_view(&g, round, 0.75, -0.35, 0.18);
        rig.still(&g, &mut r, "lodged_front_close", "close: the round's exposed body standing out of its crater", Some(view))?;
    }

    // ── 2. A perforation of the turret rear plate (the armor-scars review's 76 mm APCR): the
    // lattice's clusters fly into the turret, glow, settle and cool.
    g.mode = Mode::Tank;
    g.station = Station::Loader;
    g.switch_t = 0.0;
    let turret = g.player.turret_model();
    g.debug_launch_round(&crate::ballistics::BR_354P, false, turret.transform_point3(Vec3::new(0.45, 1.85, -1.9)),
        turret.transform_vector3(Vec3::new(-0.15, -0.30, 1.0)), 200.0, 0x5EA1_0002);
    step(&mut g, &mut r, 4, &idle);
    let solved: Vec<f32> = g.hot_fragments.iter().filter(|f| f.active && f.temp_k > 0.0).map(|f| f.temp_k).collect();
    rig.check("the perforation's fragments are the lattice's solved bodies", !solved.is_empty(),
        format!("{} solved bodies, hottest {:.0} K", solved.len(), solved.iter().cloned().fold(0.0, f32::max)));
    receipt.insert("solved_fragments_at_strike".into(), json!(solved.len()));
    if let Some(hole) = g.armor_scars.scars().iter().copied()
        .find(|s| s.kind == crate::armor_damage::ScarKind::Hole && s.is_turret()) {
        let at = g.player.turret_model().transform_point3(hole.local_position() - hole.local_normal() * hole.plate_m());
        g.loader_face_world(at);
        step(&mut g, &mut r, 1, &idle);
        rig.still(&g, &mut r, "spall_hot", "the loader's eye on the turret rear: the solved chips in flight, glowing by temperature", None)?;
    }
    g.debug_extinguish_cabin();
    // Wait for the chips to come to rest and cool to a gloved hand's limit (343 K).
    let mut waited = 0.0f32;
    let mut stills_resting = false;
    while waited < COOL_WAIT_MAX_S {
        for _ in 0..30 {
            g.update(&idle, 1.0 / 30.0, &mut r);
        }
        waited += 1.0;
        g.debug_extinguish_cabin();
        let resting = resting_solved(&g);
        if !stills_resting && waited >= 4.0 && !resting.is_empty() {
            stills_resting = true;
            let centre = resting.iter().map(|c| c.1).fold(Vec3::ZERO, |a, b| a + b) / resting.len() as f32;
            g.loader_face_world(centre);
            step(&mut g, &mut r, 1, &idle);
            rig.still(&g, &mut r, "spall_resting_warm", "4 s on: the chips at rest, still on their heat pages", None)?;
        }
        let eye = g.station_eye(Station::Loader);
        if resting.iter().any(|(i, p, _)| p.distance(eye) < 2.3 && game_fea::fragment_holdable(&g.hot_fragments[*i])) && waited >= 10.0 {
            break;
        }
    }
    let resting = resting_solved(&g);
    receipt.insert("cooling_wait_s".into(), json!(waited));
    receipt.insert("resting_solved".into(), json!(resting.iter().map(|(i, p, t)| json!({"slot": i, "pos": p.to_array(),
        "kelvin": t, "size_mm": g.hot_fragments[*i].size * 1000.0, "embedded": g.hot_fragments[*i].lodged,
        "embed_mm": g.hot_fragments[*i].embed_m * 1000.0})).collect::<Vec<_>>()));
    rig.check("solved chips stay and cool", !resting.is_empty() && resting.iter().all(|c| c.2 < 798.0),
        format!("{} resting after {waited:.0} s, temperatures {:?}", resting.len(),
            resting.iter().map(|c| c.2.round()).collect::<Vec<_>>()));
    if !resting.is_empty() {
        let centre = resting.iter().map(|c| c.1).fold(Vec3::ZERO, |a, b| a + b) / resting.len() as f32;
        g.loader_face_world(centre);
        step(&mut g, &mut r, 1, &idle);
        rig.still(&g, &mut r, "spall_cold", "the chips cold on the floor (the non-emitting steel page)", None)?;
    }

    // ── 3. The loader's hands: take a chip, carry it, throw it out of the open hatch; take a
    // second and let it fall.
    let first = find_pickable(&mut g, &mut r, &[]);
    rig.check("the crosshair offers a cool chip", first.is_some(), format!("hover {:?} label {:?}", g.hover, g.hover_label));
    if let Some(i) = first {
        let label = g.hover_label;
        rig.still(&g, &mut r, "chip_hover", "the crosshair on a cold chip: the prompt", None)?;
        step(&mut g, &mut r, 1, &press());
        rig.check("left button takes it", g.fea.carried_fragment == Some(i), format!("{:?} after {label:?}", g.fea.carried_fragment));
        step(&mut g, &mut r, 20, &press());
        rig.still(&g, &mut r, "chip_in_hand", "the chip riding the hand's spring", None)?;
        // Disclosed: the loader's lid opened by state (its ordinary swing), then the eye up through it.
        g.roof_hatches.open[1] = true;
        step(&mut g, &mut r, 90, &press());
        let aperture = g.player.turret_model().transform_point3(tiger_hatches::APERTURE_CENTERS[1] + Vec3::Y * 0.9);
        g.loader_face_world(aperture);
        step(&mut g, &mut r, 12, &press());
        let thrown = g.throw_carried_casing(r.camera.forward());
        let left = !g.hot_fragments[i].active;
        rig.check("R throws it out through the open hatch", thrown && left,
            format!("throw accepted {thrown}, chip still simulated {}", g.hot_fragments[i].active));
        step(&mut g, &mut r, 5, &idle);
        let second = find_pickable(&mut g, &mut r, &[i]);
        if let Some(j) = second {
            step(&mut g, &mut r, 1, &press());
            let held = g.fea.carried_fragment == Some(j);
            step(&mut g, &mut r, 12, &press());
            step(&mut g, &mut r, 150, &idle);
            let f = g.hot_fragments[j];
            rig.check("a released chip falls and rests inside", held && f.active && f.settled && g.fea.carried_fragment.is_none(),
                format!("held {held}, active {}, settled {}, at {:?}", f.active, f.settled,
                    blood_parent_model(&g.player, f.frame).transform_point3(f.pos).to_array()));
        } else {
            rig.check("a second chip to drop", false, "none offered".into());
        }
        g.roof_hatches.open[1] = false;
    }

    // ── 4. The field repair: on foot in front of the Tiger with the toolbox, work the lodged
    // round out of the plate.
    if let Some(round) = &lodged {
        let job = game_fea::penetrator_body(round.shell);
        let point = g.fea_lodged_point(round.id).context("lodged point")?;
        let model = g.player.hull_model();
        let local = model.inverse().transform_point3(point);
        let mut stand = model.transform_point3(Vec3::new(local.x - 0.25, 0.0, local.z + 1.15));
        stand.y = g.hf.height_at(stand.x, stand.z);
        g.mode = Mode::Foot;
        g.carrying_track_kit = true; // disclosed: the toolbox put in hand
        g.foot.pos = stand;
        let seconds = g.fea_lodged_job_seconds(round.id);
        let mut held_s = 0.0f32;
        let mut prompt = None;
        while held_s < seconds + 3.0 && g.fea.lodged.iter().any(|l| l.id == round.id) {
            let eye = g.foot.eye();
            r.camera.pos = eye;
            r.camera.set_forward((point - eye).normalize_or_zero());
            g.foot.yaw = r.camera.yaw;
            g.foot.pitch = r.camera.pitch;
            g.update(&TankInput { fire2: true, trigger: true, ..TankInput::default() }, 1.0 / 60.0, &mut r);
            prompt = prompt.or(g.hover_label);
            held_s += 1.0 / 60.0;
            if (held_s - 1.0).abs() < 0.5 / 60.0 {
                rig.still(&g, &mut r, "remove_working", "on foot with the toolbox, working the round out", None)?;
            }
        }
        let removed = !g.fea.lodged.iter().any(|l| l.id == round.id);
        rig.check("the toolbox works the lodged round out", removed,
            format!("{:?} job of {seconds:.1} s, held {held_s:.1} s, prompt {prompt:?}, body {:.0} mm long",
                round.extraction.removal, job.length_m * 1000.0));
        step(&mut g, &mut r, 2, &idle);
        let view = lodged_view(&g, round, 0.75, -0.35, 0.18);
        let view = (view.0, model.transform_point3(local), view.2);
        rig.still(&g, &mut r, "lodged_removed", "the crater after the round came out", Some(view))?;
        g.carrying_track_kit = false;
        g.mode = Mode::Tank;
        g.station = Station::Loader;
        g.switch_t = 0.0;
    }

    // ── 5. A 152 mm BR-540 (660 g filler) through the hull side forward of the firewall, every
    // lid closed: the compartment's pressure against the latches.
    g.roof_hatches.open = [false; 4];
    g.hatch_open = false;
    step(&mut g, &mut r, 120, &idle);
    let receipts_before = g.fea.receipts.len();
    let hull = g.player.hull_model();
    g.debug_launch_round(&crate::ballistics::BR_540, false, hull.transform_point3(Vec3::new(1.5, 1.55, 0.6)),
        hull.transform_vector3(Vec3::new(-1.0, -0.04, 0.0)), 500.0, 0x5EA1_0003);
    step(&mut g, &mut r, 6, &idle);
    let compartment = g.fea.receipts.iter().skip(receipts_before).filter_map(|c| c.compartment.clone()).last();
    let lids = (g.roof_hatches.open, g.hatch_open);
    rig.check("an interior burst loads the lids", compartment.is_some(),
        format!("{compartment:?}; lids now {lids:?}"));
    receipt.insert("compartment".into(), json!(compartment));
    receipt.insert("lids_after_burst".into(), json!({"roof": lids.0, "cupola": lids.1}));
    step(&mut g, &mut r, 90, &idle);
    let model = g.player.hull_model();
    rig.still(&g, &mut r, "burst_lids", "after the interior burst: the hull from above the front",
        Some((model.transform_point3(Vec3::new(-4.5, 5.5, 7.5)), model.transform_point3(Vec3::new(0.0, 1.9, 0.6)), 45.0)))?;

    let passed = rig.checks.iter().filter(|c| c["pass"] == true).count();
    receipt.insert("checks".into(), json!(rig.checks));
    receipt.insert("stills".into(), json!(rig.stills));
    receipt.insert("wall_s".into(), json!(started.elapsed().as_secs_f32()));
    receipt.insert("disclosed".into(), json!("station and on-foot placement set directly; toolbox put in hand; the loader's lid opened by state; the cooling wait runs the ordinary update at 30 Hz"));
    let path = format!("{prefix}_receipt.json");
    std::fs::write(&path, serde_json::to_string_pretty(&serde_json::Value::Object(receipt))?)?;
    println!("[fea-play] {passed}/{} checks passed; receipt {path}", rig.checks.len());
    Ok(())
}
