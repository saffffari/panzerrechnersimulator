//! Offscreen proof of the actual held book in the source Tiger. No alternate lighting or HUD book.
//! The optional clip records the same physical leaf animation as normal play. Existing outputs
//! are never overwritten; the caller supplies a fresh captures/ prefix.

use crate::{
    game::{Game, Station, TankInput},
    tank_editor_scene::{TankAssetId, TankEditorScene},
};
use anyhow::{ensure, Context};
use serde_json::json;
use std::{
    io::Write,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
};

struct Clip {
    child: Child,
    input: Option<ChildStdin>,
}
impl Clip {
    fn new(path: &str, width: u32, height: u32) -> anyhow::Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-n",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &format!("{width}x{height}"),
                "-framerate",
                "24",
                "-i",
                "-",
                "-an",
                "-vf",
                "scale=1280:720:flags=neighbor,format=yuv420p",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "18",
                "-movflags",
                "+faststart",
                path,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("start manual page-turn encoder")?;
        let input = Some(
            child
                .stdin
                .take()
                .context("manual encoder input unavailable")?,
        );
        Ok(Self { child, input })
    }
    fn frame(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.input
            .as_mut()
            .context("manual encoder closed")?
            .write_all(bytes)?;
        Ok(())
    }
    fn finish(mut self) -> anyhow::Result<()> {
        self.input.take();
        ensure!(self.child.wait()?.success(), "manual encoder failed");
        Ok(())
    }
}
impl Drop for Clip {
    fn drop(&mut self) {
        self.input.take();
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn settle(
    r: &mut crate::renderer::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    lamps: bool,
) -> anyhow::Result<serde_json::Value> {
    // Frozen scene time is not frozen eye time. Use the project's ordinary headless-photo
    // convergence path so a transient, unlit first bounce cannot establish the meter forever.
    let warmup = crate::converge_headless_screenshot(r, device, queue, 24)?;
    ensure!(warmup.stable, "manual review eye did not converge");
    Ok(json!({"transport_frames":warmup.transport_frames,
        "exposure_frames":warmup.exposure_frames,"measured_exposure":warmup.measured_exposure,
        "cabin_lamps_on":lamps,"photometry":r.debug_frame_photometry(device, queue)?.to_json(),
        "lighting":r.debug_manual_lighting(device, queue)?}))
}

pub fn run(prefix: &str, video: bool) -> anyhow::Result<()> {
    let prefix = crate::capture_target(prefix);
    let mut outputs = (0..crate::repair_manual::SPREAD_COUNT)
        .map(|i| format!("{prefix}_spread_{i:02}.png"))
        .collect::<Vec<_>>();
    outputs.extend([
        format!("{prefix}_ink_00.png"),
        format!("{prefix}_ink_01.png"),
        format!("{prefix}_albedo_00.png"),
        format!("{prefix}_albedo_01.png"),
        format!("{prefix}_zoom_left.png"),
        format!("{prefix}_zoom_right.png"),
        format!("{prefix}_loupe.png"),
        format!("{prefix}_turn_quarter.png"),
        format!("{prefix}_turn_half.png"),
        format!("{prefix}_turn_three_quarters.png"),
        format!("{prefix}_lamps_on.png"),
        format!("{prefix}_lamps_off.png"),
        format!("{prefix}.json"),
    ]);
    if video {
        outputs.push(format!("{prefix}_page_turn.mp4"));
    }
    ensure!(
        outputs.iter().all(|p| !Path::new(p).exists()),
        "manual review prefix already exists; choose a fresh prefix"
    );
    if let Some(parent) = Path::new(&prefix).parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Audio must be offline before constructing Game: muting after construction is too late
    // for unattended captures on hosts without an interactive audio device.
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) =
        Game::build_tank_editor(&device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?;
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    g.set_cabin_lights(true);
    g.jump_station(Station::Loader);
    for _ in 0..3 {
        g.update(&TankInput::default(), 1. / 60., &mut r);
    }
    ensure!(
        g.set_repair_manual_held(true),
        "manual unavailable at source Tiger loader station"
    );
    let mut receipts = Vec::new();
    let mut first_page_luminance = None;
    let mut revisited_page_luminance = None;
    for spread in 0..crate::repair_manual::SPREAD_COUNT {
        g.repair_manual.restore_state(spread, None)?;
        ensure!(
            g.set_repair_manual_held(true),
            "manual unavailable during spread review"
        );
        g.update(&TankInput::default(), 0., &mut r);
        g.prepare_repair_manual(&mut r, &device, &queue, None);
        r.ui_reset();
        r.reset_history();
        let measured = settle(&mut r, &device, &queue, g.cabin_lights_on())?;
        if spread == 0 {
            first_page_luminance = measured["photometry"]["hdr_mean_cd_m2"].as_f64();
        }
        let path = format!("{prefix}_spread_{spread:02}.png");
        r.render_to_png(&device, &queue, &path, 1280, 720)?;
        if spread < 2 {
            r.debug_dump_manual_ink(&device,&queue,&format!("{prefix}_ink_{spread:02}.png"))?;
            r.debug_dump_manual_albedo(&device,&queue,&format!("{prefix}_albedo_{spread:02}.png"))?;
        }
        receipts.push(
            json!({"spread":spread,"pages":g.repair_manual.page_bindings(),"file":path,
            "fov_y_deg":r.camera.fov_y.to_degrees(),"zoom":g.repair_manual.zoom(),"measurement":measured}),
        );
    }
    g.repair_manual.restore_state(2, Some(2))?;
    g.set_repair_manual_held(true);
    g.repair_manual.zoom_by(8.);
    for (label, x) in [("left", 0.25), ("right", 0.75)] {
        g.repair_manual.focus(glam::Vec2::new(x, 0.45));
        g.prepare_repair_manual(&mut r, &device, &queue, None);
        r.ui_reset();
        r.reset_history();
        let measured = settle(&mut r, &device, &queue, g.cabin_lights_on())?;
        r.render_to_png(
            &device,
            &queue,
            &format!("{prefix}_zoom_{label}.png"),
            1280,
            720,
        )?;
        receipts.push(json!({"view":format!("zoom_{label}"),"measurement":measured}));
    }
    // The loupe over the right page: the lens pass from the lens itself (its own view and
    // history), then the reading frame seats that picture in the lens's circle (mask mode 5),
    // as the window does each frame while the right button is held.
    g.repair_manual.restore_state(2, Some(2))?;
    g.set_repair_manual_held(true);
    g.repair_manual.loupe_wanted = true;
    let pointer = glam::Vec2::new(0.33, 0.40);
    g.prepare_repair_manual(&mut r, &device, &queue, Some(pointer));
    let loupe = g.repair_manual.loupe.context("the loupe did not stand over the page")?;
    let saved = (r.camera, r.optic_mask);
    r.set_active_view(crate::renderer::OPTIC_EYE_VIEW);
    r.camera.pos = loupe.eye;
    r.camera.set_forward(loupe.forward);
    r.camera.fov_y = loupe.fov_y;
    r.camera.znear = 0.004;
    r.optic_mask = 0.0;
    r.set_visible(g.loupe_slot(), false);
    r.reset_history();
    // The lens view's eye adapts over its first frames as the optics' do; the window runs the
    // lens pass every frame, so the still runs it a dozen times before reading the glass.
    for _ in 0..12 {
        r.render_optic_snapshot(&device, &queue);
    }
    r.set_visible(g.loupe_slot(), true);
    r.camera = saved.0;
    r.set_active_view(0);
    r.optic_mask = 5.0;
    r.loupe = [loupe.centre_uv.x, loupe.centre_uv.y, loupe.radius_uv, loupe.sample_scale];
    r.ui_reset();
    r.reset_history();
    let measured = settle(&mut r, &device, &queue, g.cabin_lights_on())?;
    r.render_to_png(&device, &queue, &format!("{prefix}_loupe.png"), 1280, 720)?;
    r.optic_mask = saved.1;
    r.loupe = [0.0; 4];
    g.repair_manual.loupe_wanted = false;
    receipts.push(json!({"view":"loupe","measurement":measured,"lens":{"eye":loupe.eye.to_array(),
        "fov_y_deg":loupe.fov_y.to_degrees(),"centre_uv":loupe.centre_uv.to_array(),"radius_uv":loupe.radius_uv,"sample_scale":loupe.sample_scale,
        "power":crate::repair_manual::LOUPE_POWER,"diameter_m":crate::repair_manual::LOUPE_DIAMETER}}));
    g.repair_manual = crate::repair_manual::ManualState::default();
    g.set_repair_manual_held(true);
    ensure!(g.repair_manual.turn(1), "manual leaf did not start");
    for label in ["quarter", "half", "three_quarters"] {
        g.repair_manual.tick(0.12);
        g.prepare_repair_manual(&mut r, &device, &queue, None);
        r.ui_reset();
        r.reset_history();
        let measured = settle(&mut r, &device, &queue, g.cabin_lights_on())?;
        r.render_to_png(
            &device,
            &queue,
            &format!("{prefix}_turn_{label}.png"),
            1280,
            720,
        )?;
        receipts.push(json!({"view":format!("turn_{label}"),"measurement":measured}));
    }
    g.repair_manual = crate::repair_manual::ManualState::default();
    g.set_repair_manual_held(true);
    for (name, on) in [("on", true), ("off", false)] {
        // Exercise the actual cabin switch, without adding a lamp, modifying its power or
        // changing the paper material. Both cases retain normal measured eye adaptation.
        g.set_cabin_lights(on);
        g.update(&TankInput::default(), 0., &mut r);
        g.prepare_repair_manual(&mut r, &device, &queue, None);
        r.ui_reset();
        r.reset_history();
        let measured = settle(&mut r, &device, &queue, g.cabin_lights_on())?;
        if on {
            revisited_page_luminance = measured["photometry"]["hdr_mean_cd_m2"].as_f64();
        }
        r.render_to_png(
            &device,
            &queue,
            &format!("{prefix}_lamps_{name}.png"),
            1280,
            720,
        )?;
        receipts.push(json!({"view":format!("lamps_{name}"),"measurement":measured}));
    }
    g.set_cabin_lights(true);
    if video {
        g.repair_manual = crate::repair_manual::ManualState::default();
        g.set_repair_manual_held(true);
        g.update(&TankInput::default(), 0., &mut r);
        g.prepare_repair_manual(&mut r, &device, &queue, None);
        r.ui_reset();
        r.reset_history();
        settle(&mut r, &device, &queue, g.cabin_lights_on())?;
        let mut clip = Clip::new(&format!("{prefix}_page_turn.mp4"), r.iw, r.ih)?;
        for frame in 0..96 {
            if frame == 18 {
                g.repair_manual.turn(1);
            }
            if frame == 60 {
                g.repair_manual.turn(-1);
            }
            g.update(&TankInput::default(), 1. / 24., &mut r);
            g.prepare_repair_manual(&mut r, &device, &queue, None);
            r.ui_reset();
            clip.frame(&r.render_to_rgba(&device, &queue, r.iw, r.ih)?)?;
        }
        clip.finish()?;
    }
    // Same camera, source lights, original page and ordinary history reset. The scene's first
    // frame must not seed open-sky illumination through an empty/stale TLAS. A factor-two
    // tolerance allows stochastic transport noise; the diagnosed failure differed by ~45x.
    let first_l = first_page_luminance.context("first-page photometry absent")?;
    let revisited_l = revisited_page_luminance.context("revisited-page photometry absent")?;
    let startup_ratio = first_l.max(revisited_l) / first_l.min(revisited_l).max(1e-12);
    let startup_consistent = first_l.is_finite() && revisited_l.is_finite()
        && first_l > 0.0 && revisited_l > 0.0 && startup_ratio <= 2.0;
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_vec_pretty(&json!({
            "schema":"panzer.physical-repair-manual-review.v2","adapter":adapter.get_info().name,
            "scope":"Actual source Tiger loader station and runtime physical book; staged page selection and pointer-independent flips. Existing cabin lamps are switched on for reading and compared honestly with switched-off lamps. Standard headless transport and measured-eye convergence, no alternate light power, emissive paper or fixed exposure. Human readability and native-input playtest remain required.",
            "diagram_provenance":"Authored monochrome explanatory schematics of the supported game interactions; not traced source CAD, engineering drawings, or a historical manual facsimile.",
            "spreads":receipts,"video":video,"outputs":outputs,
            "checks":{"book_opened":true,"all_spreads_reached":true,"forward_leaf_started":true,
                "first_and_revisited_page_lighting_consistent":startup_consistent},
            "startup_lighting":{"first_page_mean_cd_m2":first_l,
                "revisited_page_mean_cd_m2":revisited_l,"ratio":startup_ratio,
                "maximum_ratio":2.0},
        }))?,
    )?;
    ensure!(startup_consistent,"first and revisited page lighting differ by {startup_ratio:.2}x; see {prefix}.json");
    println!("[repair-manual] wrote physical page review {prefix}.json");
    Ok(())
}

/// One display-resolution still of the paper overlays over the converged cabin.
struct OverlayStill<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: &'a str,
    size: (u32, u32),
    receipts: Vec<serde_json::Value>,
}

impl OverlayStill<'_> {
    fn take(&mut self, r: &mut crate::renderer::Renderer, g: &mut Game, name: &str, settle_frames: u32) -> anyhow::Result<()> {
        g.update(&TankInput::default(), 0., r);
        g.prepare_repair_manual(r, self.device, self.queue, None);
        g.prepare_order_receipt(r, self.device, self.queue);
        r.ui_reset();
        if settle_frames > 0 {
            let warmup = crate::converge_headless_screenshot(r, self.device, self.queue, settle_frames)?;
            ensure!(warmup.stable, "overlay review eye did not converge");
        }
        // The overlays are queued as the play frame queues them, then composited over the
        // crushed frame at the display's resolution.
        r.ui_reset();
        let light = g.reading_light();
        if g.repair_manual.held {
            crate::repair_manual::draw_manual_overlay(r, &g.repair_manual, light);
        }
        let rise = g.order_sheet_rise();
        if rise > 0.0 && g.order_sheet_in_hand() {
            if let Some([_, hand]) = r.order_sheet_images() {
                crate::renderer::draw_order_sheet_overlay(r, hand, rise, light);
            }
        }
        let path = format!("{}_overlay_{name}.png", self.prefix);
        r.render_to_png_hud(self.device, self.queue, &path, self.size.0, self.size.1)?;
        self.receipts.push(json!({"still": name, "file": path, "spread": g.repair_manual.spread(),
            "zoom": g.repair_manual.zoom(), "turning": g.repair_manual.turning(), "leaf": g.repair_manual.overlay_leaf(),
            "reading_light": light, "cabin_lamps_on": g.cabin_lights_on(), "sheet_rise": rise,
            "layout_px": crate::repair_manual::overlay_layout((r.iw, r.ih), &g.repair_manual).px,
            "grid": [r.iw, r.ih]}));
        Ok(())
    }
}

/// The top-down overlay book and the torn-off sheet as the player reads them: display-
/// resolution stills over the loader's lit cabin (the same source Tiger and lamps as `run`),
/// six spreads, a turn at three stages, close reading, the lamps off and the torch, the sheet
/// mid-rise and up. `KESSEL_MANUAL_3D` must be unset.
pub fn run_overlay(prefix: &str) -> anyhow::Result<()> {
    ensure!(crate::repair_manual::overlay_mode(), "KESSEL_MANUAL_3D=1 keeps the book in the scene; unset it for the overlay review");
    let prefix = crate::capture_target(prefix);
    const SPREADS: [u8; 6] = [0, 1, 2, 8, 12, 15];
    const STILLS: [&str; 8] = ["turn_quarter", "turn_half", "turn_three_quarters", "zoom", "lamps_off", "torch", "sheet_rising", "sheet"];
    let mut outputs: Vec<String> = SPREADS.iter().map(|s| format!("{prefix}_overlay_spread_{s:02}.png")).collect();
    outputs.extend(STILLS.iter().map(|n| format!("{prefix}_overlay_{n}.png")));
    outputs.push(format!("{prefix}_overlay.json"));
    ensure!(outputs.iter().all(|p| !Path::new(p).exists()), "overlay review prefix already exists; choose a fresh prefix");
    if let Some(parent) = Path::new(&prefix).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_tank_editor(&device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?;
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    g.set_cabin_lights(true);
    g.jump_station(Station::Loader);
    for _ in 0..3 {
        g.update(&TankInput::default(), 1. / 60., &mut r);
    }
    ensure!(g.set_repair_manual_held(true), "manual unavailable at source Tiger loader station");
    // The display: the owner's 3840-wide screen, the internal grid upscaled nearest as in play.
    let size = (3840u32, 2160u32);
    let mut cap = OverlayStill { device: &device, queue: &queue, prefix: &prefix, size, receipts: Vec::new() };
    for (i, spread) in SPREADS.into_iter().enumerate() {
        g.repair_manual.restore_state(spread, None)?;
        ensure!(g.set_repair_manual_held(true), "manual unavailable during spread review");
        cap.take(&mut r, &mut g, &format!("spread_{spread:02}"), if i == 0 { 24 } else { 4 })?;
    }
    // A marked spread 2 turned to 3: the leaf at a quarter, a half and three quarters.
    g.repair_manual.restore_state(2, Some(2))?;
    ensure!(g.set_repair_manual_held(true) && g.repair_manual.turn(1), "the leaf did not turn");
    for name in &STILLS[..3] {
        g.repair_manual.tick(crate::repair_manual::TURN_SECONDS * 0.25);
        cap.take(&mut r, &mut g, name, 2)?;
    }
    g.repair_manual.tick(crate::repair_manual::TURN_SECONDS);
    ensure!(!g.repair_manual.turning() && g.repair_manual.spread() == 3, "the turn did not land");
    // Close reading of the left page.
    g.repair_manual.focus(glam::Vec2::new(0.25, 0.45));
    g.repair_manual.zoom_by(8.);
    cap.take(&mut r, &mut g, "zoom", 2)?;
    g.repair_manual.zoom_by(-8.);
    // The lamps off, then the torch in hand.
    g.set_cabin_lights(false);
    cap.take(&mut r, &mut g, "lamps_off", 24)?;
    g.toggle_hand_torch();
    cap.take(&mut r, &mut g, "torch", 24)?;
    g.toggle_hand_torch();
    g.set_cabin_lights(true);
    // The torn-off sheet: printed in full at the turret's machine, torn, mid-rise, then up.
    ensure!(g.debug_hold_sample_order(), "the sample order did not tear off");
    g.debug_set_order_sheet_rise(0.5);
    cap.take(&mut r, &mut g, "sheet_rising", 24)?;
    g.debug_set_order_sheet_rise(1.0);
    cap.take(&mut r, &mut g, "sheet", 2)?;
    let receipt = json!({
        "schema": "kessel.manual-overlay-review.v1",
        "display": [size.0, size.1],
        "stills": cap.receipts,
        "scope": "The paper overlays over the source Tiger loader station: the open manual and the torn-off sheet drawn flat at the display's resolution from their retained ink, over the ordinary converged cabin (lamps on, off, and the hand torch). The overlays are not lit by the scene; their brightness is the named reading-light scalar. Human readability and a native-input playtest remain required.",
    });
    std::fs::write(format!("{prefix}_overlay.json"), serde_json::to_string_pretty(&receipt)?)?;
    println!("[manual-overlay-review] PASS — {} stills under {prefix}_overlay_*", cap.receipts.len());
    Ok(())
}
