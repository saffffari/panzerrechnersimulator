//! Cross sections of the layered damage model, drawn on the CPU (no wgpu device): the voxel
//! lattice around a strike, and the vehicle cut by the plane of the shot with its components,
//! the round, fragments and the systems knocked out. docs/FEA_DAMAGE_MODEL_20261102.md, "Stage 5".

use glam::Vec3;
use image::{Rgb, RgbImage};

use super::lattice::Lattice;
use super::materials::AMBIENT_K;

/// Incandescence ramp by temperature: steel grey below visible glow (Draper point ~798 K), then
/// dull red through orange and yellow to white. A display ramp, not a radiance solve (the game's
/// glow uses the Planck radiance at the same temperature).
pub fn heat_colour(kelvin: f32, base: [u8; 3]) -> [u8; 3] {
    let stops: [(f32, [f32; 3]); 6] = [
        (798.0, [0.35, 0.05, 0.02]),
        (950.0, [0.70, 0.10, 0.03]),
        (1100.0, [0.95, 0.30, 0.05]),
        (1300.0, [1.00, 0.60, 0.15]),
        (1500.0, [1.00, 0.85, 0.45]),
        (1800.0, [1.00, 1.00, 0.90]),
    ];
    if kelvin < stops[0].0 {
        // Below glow: tint the base toward warm grey with temperature (temper colours).
        let f = ((kelvin - AMBIENT_K) / (stops[0].0 - AMBIENT_K)).clamp(0.0, 1.0);
        let warm = [180.0, 120.0, 80.0];
        return [0, 1, 2].map(|i| (base[i] as f32 * (1.0 - 0.6 * f) + warm[i] * 0.6 * f) as u8);
    }
    let mut c = stops[stops.len() - 1].1;
    for w in stops.windows(2) {
        if kelvin < w[1].0 {
            let f = (kelvin - w[0].0) / (w[1].0 - w[0].0);
            c = [0, 1, 2].map(|i| w[0].1[i] + (w[1].1[i] - w[0].1[i]) * f);
            break;
        }
    }
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
}

fn put(img: &mut RgbImage, x: i32, y: i32, size: i32, c: [u8; 3]) {
    for dy in 0..size.max(1) {
        for dx in 0..size.max(1) {
            let (px, py) = (x + dx, y + dy);
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                img.put_pixel(px as u32, py as u32, Rgb(c));
            }
        }
    }
}

/// Two panels: left, the section through the shot plane (plate-frame x across, z down into the
/// plate) of every particle within one cell of y = 0, coloured by temperature, darkened by broken
/// bonds, detached clusters outlined in blue; right, the back face seen from behind, coloured by
/// how far each particle moved out of the plate.
pub fn lattice_png(l: &Lattice, path: &std::path::Path, pixels_per_cell: u32) -> anyhow::Result<()> {
    let s = pixels_per_cell.max(2) as f32;
    let [nx, ny, nz] = l.dims;
    let h = l.h;
    // Room for petals and fragments that left the plate.
    let margin_cells = 6.0 * l.impact.projectile.radius_m / h + 6.0;
    let w = ((nx as f32 + 2.0 * margin_cells) * s) as u32;
    let sec_h = ((nz as f32 + 2.0 * margin_cells) * s) as u32;
    let back_h = (ny as f32 * s) as u32;
    let mut img = RgbImage::from_pixel(w * 2 + 8, sec_h.max(back_h), Rgb([18, 18, 22]));
    let (ids, plate) = l.connectivity();
    let x0 = l.origin.x - 0.5 * h - margin_cells * h;
    let z0 = -margin_cells * h;
    let to_px = |x: f32| ((x - x0) / h * s) as i32;
    let to_pz = |z: f32| ((z - z0) / h * s) as i32;
    // Plate outline (rest).
    for x in 0..w as i32 {
        for z in [to_pz(0.0), to_pz(nz as f32 * h)] {
            put(&mut img, x, z, 1, [60, 60, 70]);
        }
    }
    let base = [150, 150, 155];
    for p in 0..l.n {
        let pos = l.position(p);
        if pos.y.abs() > 0.75 * h {
            continue;
        }
        let damage = l.broken_fraction(p);
        let mut c = heat_colour(l.temp[p], base);
        c = c.map(|v| (v as f32 * (1.0 - 0.6 * damage)) as u8);
        if ids[p] != plate {
            c = [80, 140, 255];
        }
        put(&mut img, to_px(pos.x), to_pz(pos.z), s as i32 - 1, c);
    }
    // The projectile: its axis and body outline at its final tip position.
    let a = l.impact.projectile.radius_m;
    let tip = l.dir * l.tip_s;
    for i in 0..400 {
        let u = i as f32 / 400.0 * 0.6;
        let p = tip - l.dir * u;
        let side = Vec3::new(l.dir.z, 0.0, -l.dir.x);
        let r = if u < l.impact.projectile.nose_m {
            let f = 1.0 - (l.impact.projectile.nose_m - u) / l.impact.projectile.nose_m;
            a * (1.0 - (1.0 - f) * (1.0 - f)).max(0.0).sqrt()
        } else {
            a
        };
        for sgn in [-1.0f32, 1.0] {
            let q = p + side * r * sgn;
            put(&mut img, to_px(q.x), to_pz(q.z), 2, [255, 230, 90]);
        }
    }
    // Back face from behind, at the particles' present positions (x across, y up the image).
    let ox = w as i32 + 8;
    let y0 = l.origin.y - 0.5 * h;
    for j in 0..ny {
        for i in 0..nx {
            let p = l.index(i, j, nz - 1);
            let rest = l.rest_position(p);
            let pos = l.position(p);
            let out = (pos.z - rest.z) / (4.0 * h);
            let c = if ids[p] != plate {
                [80, 140, 255]
            } else if out > 0.0 {
                let f = out.clamp(0.0, 1.0);
                [(120.0 + 135.0 * f) as u8, (120.0 - 60.0 * f) as u8, (120.0 - 80.0 * f) as u8]
            } else {
                heat_colour(l.temp[p], [120, 120, 125])
            };
            let x = ox + ((pos.x - x0 - margin_cells * h) / h * s) as i32;
            let y = ((pos.y - y0) / h * s) as i32;
            if x >= ox {
                put(&mut img, x, y, s as i32 - 1, c);
            }
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    img.save(path)?;
    Ok(())
}
