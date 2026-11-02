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
        g.prepare_repair_manual(&mut r, &device, &queue);
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
        g.prepare_repair_manual(&mut r, &device, &queue);
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
    g.repair_manual = crate::repair_manual::ManualState::default();
    g.set_repair_manual_held(true);
    ensure!(g.repair_manual.turn(1), "manual leaf did not start");
    for label in ["quarter", "half", "three_quarters"] {
        g.repair_manual.tick(0.12);
        g.prepare_repair_manual(&mut r, &device, &queue);
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
        g.prepare_repair_manual(&mut r, &device, &queue);
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
        g.prepare_repair_manual(&mut r, &device, &queue);
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
            g.prepare_repair_manual(&mut r, &device, &queue);
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
