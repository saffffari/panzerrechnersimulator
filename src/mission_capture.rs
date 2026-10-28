//! `mission-captures PREFIX [SEED]`: headless stills of the mission's guidance on the Canyon v2
//! winter run (docs/MISSIONS.md): the written order over the world, the held map with its marks
//! and bearing, each in-world cue (the white and the red star, the smoke column, the alerted
//! yard) and the net's lines. Observer cameras for the cues, the crew's own eye for the rest.
//! Cues are staged with the disclosed fixtures (`debug_mission_*`); nothing is drawn that the
//! run does not emit itself. A JSON receipt names every still and the mission's state.
use crate::{
    act_manifest,
    game::{Game, Station, TankInput},
    graphics,
    mission::FlareColor,
    renderer::Renderer,
    ui, world,
};
use anyhow::Context;
use glam::{Vec2, Vec3};
use std::fs;

const CW: u32 = 1280;
const CH: u32 = 720;

struct Rig<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: String,
    shots: Vec<serde_json::Value>,
}

impl Rig<'_> {
    fn observe(&self, r: &mut Renderer, eye: Vec3, target: Vec3, fov_deg: f32) {
        r.camera.pos = eye;
        r.camera.set_forward(target - eye);
        r.camera.fov_y = fov_deg.to_radians();
        r.optic_mask = 0.0;
        r.reticle = false;
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.weather_shelter = 0.0;
        r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    }
    fn still(&mut self, g: &mut Game, r: &mut Renderer, name: &str, note: &str, hud: Option<&crate::mission::Overlay>) -> anyhow::Result<()> {
        r.reset_history();
        let settled = crate::converge_headless_screenshot(r, self.device, self.queue, 16)?;
        let path = format!("{}_{name}.png", self.prefix);
        if let Some(overlay) = hud {
            r.ui_reset();
            ui::draw_mission_overlay(r, overlay);
            r.render_to_png_hud(self.device, self.queue, &path, CW, CH)?;
        } else {
            r.render_to_png(self.device, self.queue, &path, CW, CH)?;
        }
        println!("[mission-captures] {path}: {note}");
        self.shots.push(serde_json::json!({
            "name": name, "path": path, "note": note, "eye": r.camera.pos.to_array(),
            "exposure": settled.measured_exposure, "clock_utc": r.sky_clock.now().to_iso(),
            "storm": r.storm, "fog_density_m_inv": r.fog_density,
            "mission_beat": g.mission_current_beat(), "flares": g.mission_flare_positions().iter().map(|p| p.to_array()).collect::<Vec<_>>(),
        }));
        Ok(())
    }
}

fn step(g: &mut Game, r: &mut Renderer, frames: u32) {
    for _ in 0..frames { g.update(&TankInput::default(), 1.0 / 60.0, r); }
}

fn ground(g: &Game, p: Vec2) -> Vec3 { Vec3::new(p.x, g.hf.height_at(p.x, p.y), p.y) }

pub fn run(prefix: &str, seed: u64) -> anyhow::Result<()> {
    let manifest = std::path::PathBuf::from(crate::canyon::V2_MANIFEST);
    let act = act_manifest::select_runtime_manifest(Some(manifest.clone()))?
        .context("the Canyon v2 manifest did not select an act")?;
    anyhow::ensure!(act == 1, "the Canyon v2 course is an Act One map");
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    g.enable_canyon_course_seeded(seed)?;
    g.enable_singleplayer_crew();
    g.set_defer_rtt_to_render(true);
    anyhow::ensure!(g.mission_enabled(), "the Canyon v2 course has no mission sheet");
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.into(), shots: Vec::new() };
    let start = Vec2::new(g.player.pos.x, g.player.pos.z);

    // 1. The written order, held up over the world from the commander's open cupola.
    g.jump_station(Station::Commander);
    if !g.hatch_open { g.toggle_hatch(); }
    g.commander_outside = true;
    step(&mut g, &mut r, 50);
    let overlay = g.mission_overlay().context("mission overlay")?;
    anyhow::ensure!(overlay.order.is_some(), "the order sheet is up at the start");
    rig.still(&mut g, &mut r, "01_order", "the written order (three lines) held up over the ravine at the start; commander head out", Some(&overlay))?;

    // 2. The paper map: the survey with the mission's marks, the position estimate and the
    //    bearing to the open beat, in the crew's hands; then magnified on the mark. The Tiger
    //    stands 300 m up the ravine so the position ring is clear of the compass on the fold.
    step(&mut g, &mut r, 60 * 5);
    g.debug_mission_fold_order();
    g.debug_place_tiger(Vec2::new(-762.0, 470.0), 0.0);
    step(&mut g, &mut r, 6);
    anyhow::ensure!(g.set_map_held(true), "the map can be held");
    step(&mut g, &mut r, 6);
    let draw_map = |g: &mut Game, r: &mut Renderer| -> anyhow::Result<()> {
        anyhow::ensure!(g.prepare_held_map(r, true), "held map prepared");
        r.ui_reset();
        anyhow::ensure!(g.draw_canyon_map(r, &[], &[], Vec2::splat(0.5)), "the canyon survey is printed");
        r.render_held_map_texture(&device, &queue, g.held_map_slot());
        Ok(())
    };
    draw_map(&mut g, &mut r)?;
    rig.still(&mut g, &mut r, "02_map", "the held survey 300 m up the ravine: beat marks, the dead-reckoned position ring, the dashed bearing with its figures, the slip with the open beat", None)?;
    let guidance = g.mission_map_guidance().context("map guidance")?;
    let focus = guidance.bearing.map(|(_, to, _)| to).unwrap_or(start);
    let (u, v) = g.canyon_survey_uv(focus);
    g.focus_held_map(Vec2::new(u, v));
    g.zoom_held_map(3.0);
    draw_map(&mut g, &mut r)?;
    rig.still(&mut g, &mut r, "03_map_zoom", "the same sheet magnified on the open beat's mark and the bearing figures", None)?;
    g.zoom_held_map(-3.0);
    g.set_map_held(false);
    step(&mut g, &mut r, 6);

    // 3. The white star over phase line Anton (the hamlet), from the ravine floor 220 m short of it.
    g.debug_mission_open_beat(1, &mut r)?;
    let anton = g.mission_map_guidance().and_then(|gd| gd.bearing.map(|(_, to, _)| to)).context("Anton's mark")?;
    g.debug_mission_flare(anton, FlareColor::White, &mut r);
    step(&mut g, &mut r, 100);
    let eye = ground(&g, anton + (start - anton).normalize_or_zero() * 230.0) + Vec3::Y * 4.0;
    let star = g.mission_flare_positions().first().copied().unwrap_or(ground(&g, anton) + Vec3::Y * 90.0);
    rig.observe(&mut r, eye, star * 0.5 + ground(&g, anton) * 0.5 + Vec3::Y * 20.0, 62.0);
    g.prepare_world_survey(&mut r, 1.0 / 60.0);
    let overlay = g.mission_overlay().context("overlay")?;
    rig.still(&mut g, &mut r, "04_white_star", "white parachute star over the hamlet 1.7 s after launch (its light on the snow, the launch smoke), the net's line for the beat", Some(&overlay))?;
    step(&mut g, &mut r, 60 * 8);
    rig.observe(&mut r, eye, g.mission_flare_positions().first().copied().unwrap_or(star) * 0.6 + ground(&g, anton) * 0.4, 50.0);
    g.prepare_world_survey(&mut r, 1.0 / 60.0);
    rig.still(&mut g, &mut r, "05_white_star_chute", "the same star ten seconds on: under its parachute, drifting downwind, its smoke thread", None)?;

    // 4. The smoke column at the fork the gun line killed the lorry at, seen from the hamlet.
    g.debug_mission_open_beat(2, &mut r)?;
    let fork = Vec2::new(-690.0, -88.0);
    g.debug_mission_smoke(fork, 70.0, 150.0, &mut r);
    step(&mut g, &mut r, 30);
    let eye = ground(&g, Vec2::new(-747.0, 290.0)) + Vec3::Y * 5.0;
    rig.observe(&mut r, eye, ground(&g, fork) + Vec3::Y * 30.0, 58.0);
    g.prepare_world_survey(&mut r, 1.0 / 60.0);
    let overlay = g.mission_overlay().context("overlay")?;
    rig.still(&mut g, &mut r, "06_smoke_column", "the smoke column at the fork 380 m away, established 150 s, leaning downwind; the gun line's lines on the net", Some(&overlay))?;

    // 5. The factory yard: the beat opens, platoon B is alerted (Searching) and the red star
    //    stands over the yard's far edge. Observer above the saddle looking into the yard.
    g.debug_place_tiger(Vec2::new(300.0, -470.0), 0.0);
    step(&mut g, &mut r, 6);
    g.debug_mission_open_beat(3, &mut r)?;
    step(&mut g, &mut r, 60 * 5);
    let yard = Vec2::new(345.0, 190.0);
    let red = g.mission_flare_positions().first().copied();
    let eye = ground(&g, Vec2::new(250.0, 60.0)) + Vec3::Y * 7.0;
    rig.observe(&mut r, eye, red.map_or(ground(&g, yard) + Vec3::Y * 40.0, |p| p * 0.55 + ground(&g, yard) * 0.45), 58.0);
    g.prepare_world_survey(&mut r, 1.0 / 60.0);
    let overlay = g.mission_overlay().context("overlay")?;
    let alerted = g.mission_report().map(|rep| rep["radio"].clone());
    rig.still(&mut g, &mut r, "07_red_star_yard", "the red star over the factory yard's far edge 430 m off, five seconds after the beat opened; platoon B alerted to Searching (receipt); the net's line", Some(&overlay))?;

    // 6. The ravine head: the finale's white star over the goal disc, from the basin's exit.
    g.debug_place_tiger(Vec2::new(1389.0, 337.0), 90f32.to_radians());
    step(&mut g, &mut r, 6);
    g.debug_mission_open_beat(5, &mut r)?;
    step(&mut g, &mut r, 60 * 3);
    let goal = g.mission_map_guidance().and_then(|gd| gd.bearing.map(|(_, to, _)| to)).context("the head's mark")?;
    let eye = ground(&g, Vec2::new(1500.0, 300.0)) + Vec3::Y * 6.0;
    let star = g.mission_flare_positions().first().copied().unwrap_or(ground(&g, goal) + Vec3::Y * 80.0);
    rig.observe(&mut r, eye, star * 0.55 + ground(&g, goal) * 0.45, 54.0);
    g.prepare_world_survey(&mut r, 1.0 / 60.0);
    let overlay = g.mission_overlay().context("overlay")?;
    rig.still(&mut g, &mut r, "08_head_star", "the white star over the ravine head 200 m off, the finale's line", Some(&overlay))?;

    // 7. The compass with the march bearing set, from the crew's own eye (C raises it).
    g.jump_station(Station::Loader);
    anyhow::ensure!(g.toggle_compass(), "the compass can be raised");
    step(&mut g, &mut r, 45);
    rig.still(&mut g, &mut r, "09_compass", "the pocket compass raised at the loader's post: its body turned to the open beat's bearing, the needle on north", None)?;

    let receipt = serde_json::json!({
        "schema": "panzer.mission-captures.v1", "seed": seed, "manifest": manifest,
        "output": [CW, CH], "renderer": if r.shader_backend { "shader" } else { "raymarch/other" },
        "shots": rig.shots, "mission": g.mission_report(), "alerted_lines": alerted,
        "staging": "cues fired by the disclosed debug fixtures at their authored places; observer cameras for the cues; the crew's eye for the order, the map and the compass",
    });
    fs::write(format!("{prefix}_receipt.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("[mission-captures] receipt {prefix}_receipt.json");
    Ok(())
}
