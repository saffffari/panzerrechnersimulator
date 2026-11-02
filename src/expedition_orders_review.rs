//! `expedition-orders-review PREFIX`: the authored orders of The March played through on the
//! retained theatre (docs/MISSIONS.md, "The March"). Order 10 is accepted with the launcher's
//! staged forward start, its three beats are driven by disclosed fixtures (the site's guards
//! marked dead, the Tiger stood on the site for its hold) and the net's lines are checked in
//! order. Stills: the written order held up at the turn, the held map with the order's marks, the
//! secured site (the camp stills are by daylight on foot; the turn and the yard from the
//! commander's seat). This is not a human playtest and makes no ballistic or driving claim: only the
//! mission state machine and the theatre's own order rules resolve the outcome.
use crate::{game::{Game, TankInput}, renderer::Renderer, ui};
use anyhow::{ensure, Context};
use glam::{Vec2, Vec3};
use serde_json::{json, Value};
use std::path::Path;

const SITE: u64 = 10;
const CW: u32 = 1280;
const CH: u32 = 720;

fn still(device: &wgpu::Device, queue: &wgpu::Queue, g: &mut Game, r: &mut Renderer, path: &str) -> anyhow::Result<()> {
    // The strips' ink is painted by the window's frame; a headless still paints it here.
    g.prepare_order_receipt(r, device, queue);
    r.reset_history();
    crate::converge_headless_screenshot(r, device, queue, 16)?;
    r.ui_reset();
    if let Some(overlay) = g.mission_overlay() { ui::draw_mission_overlay(r, &overlay, g.map_held); }
    r.render_to_png_hud(device, queue, path, CW, CH)?;
    println!("[orders-review] {path}");
    Ok(())
}
fn step(g: &mut Game, r: &mut Renderer, seconds: f32) {
    let steps = (seconds * 10.0).ceil() as usize;
    for _ in 0..steps { g.update(&TankInput::default(), 0.1, r); }
}
fn radio(g: &Game) -> Vec<String> {
    g.mission_report().and_then(|m| m["radio"].as_array().map(|lines| lines.iter()
        .map(|l| format!("{}: {}", l["from"].as_str().unwrap_or(""), l["text"].as_str().unwrap_or(""))).collect())).unwrap_or_default()
}
fn beat_status(g: &Game, k: usize) -> String {
    g.mission_report().and_then(|m| m["beats"][k]["status"].as_str().map(str::to_owned)).unwrap_or_default()
}

pub fn run(requested: &str) -> anyhow::Result<()> {
    let prefix = crate::capture_target(requested);
    let receipt = format!("{prefix}.json");
    ensure!(!Path::new(&receipt).exists(), "orders review prefix exists; use a new prefix");
    if let Some(parent) = Path::new(&prefix).parent() { std::fs::create_dir_all(parent)?; }
    let mut checks: Vec<Value> = Vec::new();
    let mut check = |label: &str, pass: bool, evidence: Value| {
        println!("[orders-review] {label}: {}", if pass { "PASS" } else { "FAIL" });
        checks.push(json!({"check": label, "pass": pass, "evidence": evidence}));
    };
    let sheets = crate::expedition_theatre::mission_sheets()?;
    let spec = crate::expedition_theatre::Spec::load()?;
    check("every site has an authored three-beat order", sheets.len() == spec.sites.len() && sheets.iter().all(|m| m.beats.len() == 3),
        json!(sheets.iter().map(|m| json!({"site": m.site, "title": m.title, "beats": m.beats.iter().map(|b| &b.id).collect::<Vec<_>>()})).collect::<Vec<_>>()));

    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
    let (mut r, mut g) = Game::build_expedition_act(&device, &queue, false, &crate::world::act(1))?;
    g.mute_audio();
    g.enable_singleplayer_crew();
    let camp = g.expedition_initial_camp_position()?;
    g.enable_expedition(1944, camp)?;
    g.enable_expedition_theatre(&mut r)?;
    g.begin_expedition_at_camp(&mut r)?;
    step(&mut g, &mut r, 1.0);

    // Order 10 accepted at the camp, on foot by the orders table: the written order held up.
    g.debug_expedition_start_order(SITE, false, &mut r)?;
    let site = Vec2::from_array(spec.sites.iter().find(|s| s.id == SITE).unwrap().position);
    let hold_s = spec.sites.iter().find(|s| s.id == SITE).unwrap().hold_s;
    step(&mut g, &mut r, 0.3);
    check("the order is accepted at camp and prints on the strip, the march beat open on the net",
        g.order_strip_active() && !g.mission_order_up() && g.expedition_order_active() && beat_status(&g, 0) == "Active"
            && radio(&g).first().is_some_and(|l| l.starts_with("Stab: Elster, Stab. Order 10")),
        json!({"radio": radio(&g), "camp": [camp.x, camp.z], "print": format!("{:?}", g.order_print_phase())}));
    // Fixture: the print finished and the strip torn off into the hands for the still.
    ensure!(g.debug_tear_order_strip(), "the strip could not be torn off");
    step(&mut g, &mut r, 0.3);
    check("the torn-off strip is read in hand", g.order_receipt_held(), json!({"print": format!("{:?}", g.order_print_phase())}));
    still(&device, &queue, &mut g, &mut r, &format!("{prefix}_order_at_camp.png"))?;
    ensure!(g.stow_order_receipt(), "X did not pocket the strip");

    // The held map at camp: the order's marks (the turn, the yard) and the bearing to the yard.
    g.set_map_held(true);
    step(&mut g, &mut r, 0.5);
    r.ui_reset();
    ensure!(g.draw_expedition_map(&mut r), "the expedition map did not draw");
    r.render_held_map_texture(&device, &queue, g.held_map_slot());
    let guidance = g.mission_map_guidance().context("no map guidance")?;
    check("the map carries the open march beat's mark and a bearing to the turn",
        guidance.marks.len() == 1 && guidance.bearing.is_some() && guidance.slip.as_deref() == Some("1  THE APPROACH TO THE YARD"),
        json!({"marks": guidance.marks.len(), "bearing": guidance.bearing.map(|(_, _, deg)| deg), "slip": guidance.slip}));
    still(&device, &queue, &mut g, &mut r, &format!("{prefix}_map_at_camp.png"))?;
    g.set_map_held(false);

    // The launcher's staged start: the crewed Tiger at the turn, engine on.
    g.debug_expedition_stage_forward(&mut r)?;
    let approach = g.mission_report().map(|m| Vec2::new(m["beats"][0]["at"][0].as_f64().unwrap() as f32, m["beats"][0]["at"][1].as_f64().unwrap() as f32)).unwrap();
    let tiger = Vec2::new(g.player.pos.x, g.player.pos.z);
    check("staged start stands the Tiger at the order's turn, engine running",
        tiger.distance(approach) < 5.0 && g.engine_on && g.expedition_order_active(),
        json!({"tiger": tiger.to_array(), "approach": approach.to_array(), "site": site.to_array(), "engine_on": g.engine_on}));
    step(&mut g, &mut r, 2.0);
    let lines = radio(&g);
    check("the march beat closes at the turn; contact opens with its reported opposition",
        beat_status(&g, 0) == "Done" && beat_status(&g, 1) == "Active" && lines.len() >= 3
            && lines[1].contains("At the turn") && lines[2].contains("Reported at the yard"),
        json!({"radio": lines, "beats": [beat_status(&g, 0), beat_status(&g, 1), beat_status(&g, 2)]}));
    let guidance = g.mission_map_guidance().context("no map guidance")?;
    check("at the turn the map marks the turn as passed and bears on the yard",
        guidance.marks.len() == 2 && guidance.marks[0].3 && guidance.bearing.is_some_and(|(_, to, _)| to.distance(site) < 1.0),
        json!({"marks": guidance.marks.len(), "bearing": guidance.bearing.map(|(_, _, deg)| deg), "slip": guidance.slip}));
    still(&device, &queue, &mut g, &mut r, &format!("{prefix}_order_at_turn.png"))?;
    // The machine at the commander's left: a line keyed to the detail prints on the strip
    // (offline responder unless a key is set), the head turned to it for the still.
    let keyed = g.stab_transmit("Stab, Elster. What is my fuel and ammunition? Over.").is_some();
    step(&mut g, &mut r, 4.0);
    check("a line keyed to the detail prints on the turret's strip with an answer",
        keyed && g.order_strip_active() && g.order_print_phase() != crate::game::order_printer::Phase::Idle,
        json!({"print": format!("{:?}", g.order_print_phase())}));
    g.debug_look_at_order_machine();
    step(&mut g, &mut r, 0.2);
    still(&device, &queue, &mut g, &mut r, &format!("{prefix}_machine.png"))?;

    // Fixture: the yard's finite garrison marked dead (no shot fired here), the Tiger stood on
    // the yard for the theatre's own hold clock.
    let (guards, _sentries) = g.debug_expedition_kill_site_garrison(SITE)?;
    step(&mut g, &mut r, 0.5);
    check("contact closes when the site's guards are down (wherever they fell); secure opens",
        beat_status(&g, 1) == "Done" && beat_status(&g, 2) == "Active" && radio(&g).iter().any(|l| l.contains("Armour at the yard destroyed")),
        json!({"guards": guards, "radio": radio(&g)}));
    g.player.pos = Vec3::new(site.x, g.hf.vehicle_height_at(site.x, site.y), site.y);
    g.player.speed = 0.0;
    step(&mut g, &mut r, hold_s + 3.0);
    let report = g.mission_report().unwrap();
    let theatre = g.expedition_theatre_receipt().unwrap();
    let secured = theatre["sites"].as_array().unwrap().iter().find(|s| s["id"] == SITE).unwrap()["complete"] == true;
    check("the hold secures the yard and the order closes with its own words",
        secured && beat_status(&g, 2) == "Done" && report["finished"] == true && radio(&g).iter().any(|l| l.contains("Yard secured. Loading"))
            && !g.expedition_order_active(),
        json!({"theatre_notice": theatre["notice"], "finished": report["finished"], "radio": radio(&g)}));
    still(&device, &queue, &mut g, &mut r, &format!("{prefix}_secured.png"))?;
    g.expedition_state().unwrap().validate()?;
    check("coordinator and theatre validate after the order", true, json!({"dispatches": g.expedition_state().unwrap().dispatches.len()}));

    let pass = checks.iter().all(|c| c["pass"] == true);
    std::fs::write(&receipt, serde_json::to_vec_pretty(&json!({
        "schema": "kessel.expedition-orders-review.v1", "status": if pass { "passed" } else { "failed" },
        "scope": "Authored orders on the retained theatre; guard deaths and Tiger positions are explicit fixtures; no ballistic or driving claim.",
        "site": SITE, "checks": checks, "mission": report, "theatre": theatre,
    }))?)?;
    println!("[orders-review] receipt {receipt}");
    ensure!(pass, "one or more order checks failed");
    Ok(())
}
