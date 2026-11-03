//! Slow-motion film of the round as a deformable body (`shot`), drawn on the CPU: each strike
//! from the calibrated event, the round's cells at their solved positions in two views (outside,
//! and cut along its axis to show the cap, the filler, the core and the cavity it cuts), the plate
//! as its section through the shot line with the voxels the round cut away. A review drawing, not
//! the game's renderer; cells are drawn as their own blocks, glowing by their solved temperature.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use glam::Vec3;
use image::{Rgb, RgbImage};
use serde_json::json;

use super::section::heat_colour;
use super::shot::{self, Shot, Zone};
use crate::ballistics::{self, ArmorMaterial, Plate, ShellSpec};

const W: u32 = 1280;
const H: u32 = 720;
const PANEL: u32 = 640;
const FPS: u32 = 30;
const HOLD_FRAMES: usize = 45;

/// One filmed strike: the round, its range (striking speed from the ballistic law), the plate and
/// the angle off its normal.
pub struct Case {
    pub label: &'static str,
    pub shell: &'static ShellSpec,
    pub range_m: f32,
    pub plate_mm: f32,
    pub angle_deg: f32,
}

pub fn cases() -> Vec<Case> {
    use ballistics::{BR_350B, BR_354P, BR_365, BR_471, PZGR_39};
    vec![
        Case { label: "122 mm sharp APHE square on 100 mm: through, nose intact", shell: &BR_471, range_m: 300.0, plate_mm: 100.0, angle_deg: 0.0 },
        Case { label: "122 mm at 72 degrees: gouge, ricochet, tumble", shell: &BR_471, range_m: 500.0, plate_mm: 100.0, angle_deg: 72.0 },
        Case { label: "122 mm at 55 degrees on 100 mm", shell: &BR_471, range_m: 800.0, plate_mm: 100.0, angle_deg: 55.0 },
        Case { label: "76 mm blunt APHE on 100 mm front: defeated", shell: &BR_350B, range_m: 300.0, plate_mm: 100.0, angle_deg: 10.0 },
        Case { label: "76 mm tungsten core at 30 degrees on 100 mm", shell: &BR_354P, range_m: 200.0, plate_mm: 100.0, angle_deg: 30.0 },
        Case { label: "85 mm blunt APHE on 80 mm side at 25 degrees", shell: &BR_365, range_m: 500.0, plate_mm: 80.0, angle_deg: 25.0 },
        Case { label: "88 mm capped APCBC at 30 degrees on 100 mm", shell: &PZGR_39, range_m: 500.0, plate_mm: 100.0, angle_deg: 30.0 },
        Case { label: "85 mm at 65 degrees on 80 mm: ricochet", shell: &BR_365, range_m: 800.0, plate_mm: 80.0, angle_deg: 65.0 },
    ]
}

fn zone_colour(z: Zone) -> [u8; 3] {
    match z {
        Zone::Nose => [150, 158, 172],
        Zone::Body => [122, 126, 136],
        Zone::Base => [100, 100, 104],
        Zone::Cap => [176, 150, 112],
        Zone::Windshield => [196, 196, 200],
        Zone::Filler => [214, 190, 92],
        Zone::Core => [66, 66, 78],
        Zone::Carrier => [140, 140, 150],
    }
}

struct Text<'a> {
    font: FontRef<'a>,
}

impl Text<'_> {
    fn draw(&self, img: &mut RgbImage, x: f32, y: f32, px: f32, s: &str, c: [u8; 3]) {
        let sf = self.font.as_scaled(PxScale::from(px));
        let mut pen = x;
        for ch in s.chars() {
            let id = self.font.glyph_id(ch);
            let glyph = id.with_scale_and_position(px, ab_glyph::point(pen, y + sf.ascent()));
            pen += sf.h_advance(id);
            if let Some(outline) = self.font.outline_glyph(glyph) {
                let b = outline.px_bounds();
                outline.draw(|gx, gy, v| {
                    let (px_, py_) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                    if v > 0.4 && px_ >= 0 && py_ >= 0 && (px_ as u32) < img.width() && (py_ as u32) < img.height() {
                        img.put_pixel(px_ as u32, py_ as u32, Rgb(c));
                    }
                });
            }
        }
    }
}

/// The view: plate-frame z (into the plate) to the right, x up, seen along y turned 28 degrees so
/// the plate's face shows.
struct View {
    right: Vec3,
    up: Vec3,
    depth: Vec3,
    centre: Vec3,
    ppm: f32,
    x0: i32,
}

impl View {
    fn new(centre: Vec3, span_m: f32, x0: i32) -> Self {
        let yaw = 28f32.to_radians();
        Self {
            right: Vec3::new(0.0, yaw.sin(), yaw.cos()),
            up: Vec3::new(-1.0, 0.0, 0.0),
            depth: Vec3::new(0.0, yaw.cos(), -yaw.sin()),
            centre,
            ppm: PANEL as f32 / span_m,
            x0,
        }
    }
    fn project(&self, p: Vec3) -> (f32, f32, f32) {
        let d = p - self.centre;
        (self.x0 as f32 + PANEL as f32 * 0.5 + d.dot(self.right) * self.ppm, H as f32 * 0.55 - d.dot(self.up) * self.ppm, d.dot(self.depth))
    }
}

fn square(img: &mut RgbImage, cx: f32, cy: f32, half: f32, c: [u8; 3], edge: [u8; 3], clip: (i32, i32)) {
    let (x0, y0, x1, y1) = ((cx - half).round() as i32, (cy - half).round() as i32, (cx + half).round() as i32, (cy + half).round() as i32);
    for y in y0.max(0)..y1.min(H as i32) {
        for x in x0.max(clip.0)..x1.min(clip.1) {
            let border = half >= 2.0 && (x == x0 || y == y0 || x == x1 - 1 || y == y1 - 1);
            img.put_pixel(x as u32, y as u32, Rgb(if border { edge } else { c }));
        }
    }
}

fn shade(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * k).clamp(0.0, 255.0) as u8)
}

#[allow(clippy::too_many_arguments)]
fn draw_panel(img: &mut RgbImage, shot: &Shot, frame: &shot::Frame, view: &View, section: bool, thickness_m: f32) {
    let clip = (view.x0, view.x0 + PANEL as i32);
    let h = shot.h;
    let half = 0.5 * h * view.ppm;
    // The plate's section through the shot line (voxels at y = 0), less what the round cut away.
    let gone: std::collections::HashSet<(i32, i32, i32)> = frame.gone.iter().copied().collect();
    let nz = (thickness_m / h).ceil() as i32;
    let reach = (PANEL as f32 / view.ppm / h) as i32 + 4;
    let ci = (view.centre.x / h).floor() as i32;
    for k in 0..nz {
        for i in ci - reach..ci + reach {
            let key = (i, 0, k);
            let p = Vec3::new((i as f32 + 0.5) * h, 0.5 * h, (k as f32 + 0.5) * h);
            let (sx, sy, _) = view.project(p);
            if gone.contains(&key) {
                square(img, sx, sy, half, [34, 30, 30], [34, 30, 30], clip);
            } else {
                let tone = if section { 92 } else { 78 };
                square(img, sx, sy, half, [tone, tone, tone + 6], [tone - 12, tone - 12, tone - 6], clip);
            }
        }
    }
    // The round's cells, far to near.
    let mut order: Vec<(f32, usize)> = (0..shot.len())
        .filter(|&c| !section || frame.pos[c].y >= -0.25 * h)
        .map(|c| (view.project(frame.pos[c]).2, c))
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, c) in order {
        let (sx, sy, _) = view.project(frame.pos[c]);
        let base = zone_colour(shot.zone[c]);
        let lit = heat_colour(frame.temp[c], base);
        let dmg = frame.damage[c] as f32 / 255.0;
        let col = shade(lit, 1.0 - 0.55 * dmg);
        square(img, sx, sy, half.max(1.0), col, shade(col, 0.62), clip);
    }
}

fn fill(img: &mut RgbImage, x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 3]) {
    for y in y0..y1.min(H) {
        for x in x0..x1.min(W) {
            img.put_pixel(x, y, Rgb(c));
        }
    }
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    let text = Text { font: FontRef::try_from_slice(include_bytes!("../../assets/fonts/VT323-Regular.ttf"))? };
    let video = format!("{prefix}.mp4");
    let mut ffmpeg = std::process::Command::new("ffmpeg")
        .args(["-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-r", &FPS.to_string(),
            "-i", "-", "-c:v", "libx264", "-crf", "18", "-pix_fmt", "yuv420p", &video])
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    let mut stdin = ffmpeg.stdin.take().ok_or_else(|| anyhow::anyhow!("ffmpeg stdin"))?;
    let mut receipt = Vec::new();
    for (index, case) in cases().iter().enumerate() {
        let plate = Plate { name: "range plate", thickness_mm: case.plate_mm, slope_deg: 0.0, material: ArmorMaterial::Rha };
        let event = ballistics::resolve(case.shell, case.range_m, plate, case.angle_deg, 0x5407 + index as u32);
        let Some(impact) = shot::impact_of(&event, 0x5407 ^ index as u32) else { continue };
        let started = std::time::Instant::now();
        let mut solver = Shot::new(impact);
        let out = solver.solve();
        let wall_ms = started.elapsed().as_secs_f32() * 1000.0;
        println!("[shot-film] {}: {} at {:.0} m/s, {:.0} deg on {:.0} mm -> chain {:?}; solve: residual {:.2}/{:.2} kg, {} pieces, tumble {:.0} rad/s, length x{:.2} radius x{:.2}, peak {:.0} K, {} cells x {} steps in {:.0} ms",
            case.label, case.shell.name, event.striking_v, event.impact_angle_deg, case.plate_mm, event.terminal, out.residual_kg, out.mass_kg,
            out.pieces.len(), out.tumble_rad_s, out.length_ratio, out.radius_ratio, out.peak_temp_k, out.cells, out.steps, wall_ms);
        let main_cells = &out.pieces[0].cells;
        let span = (solver.length_m * 2.6 + impact.thickness_m * 1.5).max(0.45);
        let frames = &solver.frames;
        let last = frames.len() - 1;
        let mut still_saved = false;
        for f in (0..frames.len()).chain(std::iter::repeat_n(last, HOLD_FRAMES)) {
            let frame = &frames[f];
            let centre_main = main_cells.iter().map(|&c| frame.pos[c as usize]).sum::<Vec3>() / main_cells.len().max(1) as f32;
            let entry = Vec3::new(0.0, 0.0, 0.5 * impact.thickness_m);
            let centre = entry.lerp(centre_main, 0.75);
            let mut img = RgbImage::from_pixel(W, H, Rgb([16, 16, 20]));
            draw_panel(&mut img, &solver, frame, &View::new(centre, span, 0), false, impact.thickness_m);
            draw_panel(&mut img, &solver, frame, &View::new(centre, span, PANEL as i32), true, impact.thickness_m);
            fill(&mut img, PANEL - 1, 0, PANEL + 1, H, [60, 60, 66]);
            fill(&mut img, 0, 0, W, 64, [10, 10, 14]);
            text.draw(&mut img, 12.0, 4.0, 26.0, &format!("{}  |  {}  {:.0} m/s  {:.0} deg  {:.0} mm RHA  |  chain: {:?}",
                case.label, case.shell.name, event.striking_v, event.impact_angle_deg, case.plate_mm, event.terminal), [235, 235, 235]);
            let slower = 1.0 / (FPS as f32 * frames[last].t_s / last.max(1) as f32);
            text.draw(&mut img, 12.0, 32.0, 24.0, &format!("t = {:6.1} us   ({:.0}x slower than life)", frame.t_s * 1e6, slower), [180, 200, 255]);
            text.draw(&mut img, PANEL as f32 + 12.0, 32.0, 24.0, "cut along the axis", [180, 200, 255]);
            if f == last {
                fill(&mut img, 0, H - 64, W, H, [10, 10, 14]);
                text.draw(&mut img, 12.0, H as f32 - 60.0, 24.0, &format!(
                    "residual {:.2} of {:.2} kg at {:.0} m/s, {} pieces, tumble {:.0} rad/s, length x{:.2}, widest x{:.2}, bend {:.1} deg, hottest {:.0} K",
                    out.residual_kg, out.mass_kg, out.residual_velocity.length(), out.pieces.len(), out.tumble_rad_s, out.length_ratio, out.radius_ratio,
                    out.bend_deg, out.peak_temp_k), [255, 220, 150]);
                text.draw(&mut img, 12.0, H as f32 - 32.0, 22.0, &format!("{} cells of {:.1} mm, {} steps solved in {:.0} ms", out.cells, out.cell_m * 1000.0,
                    out.steps, wall_ms), [170, 170, 170]);
            }
            // A 10 cm bar.
            fill(&mut img, 16, H - 80, 16 + (0.1 * PANEL as f32 / span) as u32, H - 76, [220, 220, 220]);
            use std::io::Write;
            stdin.write_all(img.as_raw())?;
            if f == last && !still_saved {
                img.save(format!("{prefix}_{index}.png"))?;
                still_saved = true;
            }
        }
        receipt.push(json!({
            "case": case.label, "shell": case.shell.name, "range_m": case.range_m, "plate_mm": case.plate_mm,
            "striking_v": event.striking_v, "angle_deg": event.impact_angle_deg, "chain_terminal": format!("{:?}", event.terminal),
            "chain_residual_v": event.residual_v, "resistance_pa": impact.resistance_pa, "design": impact.design,
            "solve": out, "wall_ms": wall_ms,
        }));
    }
    drop(stdin);
    anyhow::ensure!(ffmpeg.wait()?.success(), "ffmpeg failed");
    std::fs::write(format!("{prefix}_receipt.json"), serde_json::to_string_pretty(&json!({ "cases": receipt }))?)?;
    println!("[shot-film] wrote {video}");
    Ok(())
}
