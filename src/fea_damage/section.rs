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

// ── SVG drawing of a vehicle cross section ─────────────────────────────────────────────────────────

/// A 2-D canvas in SVG with a world-to-pixel mapping for the section plane.
pub struct Svg {
    pub width: f32,
    pub height: f32,
    body: String,
}

pub fn esc(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn rgb(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

impl Svg {
    pub fn new(width: f32, height: f32, background: [u8; 3]) -> Self {
        let mut s = Self { width, height, body: String::new() };
        s.rect(0.0, 0.0, width, height, &rgb(background), "none", 0.0, 1.0);
        s
    }
    #[allow(clippy::too_many_arguments)]
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: &str, stroke: &str, stroke_w: f32, opacity: f32) {
        self.body.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{stroke_w:.1}\" opacity=\"{opacity:.2}\"/>\n"));
    }
    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), stroke: &str, width: f32, dash: bool) {
        self.body.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{stroke}\" stroke-width=\"{width:.1}\"{}/>\n",
            a.0, a.1, b.0, b.1, if dash { " stroke-dasharray=\"6,4\"" } else { "" }));
    }
    pub fn polyline(&mut self, points: &[(f32, f32)], stroke: &str, width: f32) {
        if points.len() < 2 {
            return;
        }
        let pts: Vec<String> = points.iter().map(|p| format!("{:.1},{:.1}", p.0, p.1)).collect();
        self.body.push_str(&format!("<polyline points=\"{}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{width:.1}\"/>\n", pts.join(" ")));
    }
    pub fn polygon(&mut self, points: &[(f32, f32)], fill: &str, stroke: &str, width: f32) {
        let pts: Vec<String> = points.iter().map(|p| format!("{:.1},{:.1}", p.0, p.1)).collect();
        self.body.push_str(&format!("<polygon points=\"{}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{width:.1}\"/>\n", pts.join(" ")));
    }
    pub fn circle(&mut self, c: (f32, f32), r: f32, fill: &str, opacity: f32) {
        self.body.push_str(&format!("<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{r:.1}\" fill=\"{fill}\" opacity=\"{opacity:.2}\"/>\n", c.0, c.1));
    }
    pub fn text(&mut self, x: f32, y: f32, size: f32, fill: &str, text: &str) {
        self.body.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-family=\"DejaVu Sans, sans-serif\" font-size=\"{size:.1}\" fill=\"{fill}\">{}</text>\n",
            esc(text)));
    }
    /// A raster image (e.g. the static layer) drawn at the origin, referenced by a relative path.
    pub fn image(&mut self, href: &str, w: f32, h: f32) {
        self.body.push_str(&format!("<image href=\"{}\" x=\"0\" y=\"0\" width=\"{w:.0}\" height=\"{h:.0}\"/>\n", esc(href)));
    }
    pub fn finish(&self) -> String {
        format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\n{}</svg>\n",
            self.body, w = self.width, h = self.height)
    }
}

/// Segments where `plane` (point, unit normal) cuts the triangles of a mesh posed by `model`,
/// expressed in the plane's 2-D axes `(u, v)` through `origin`.
pub fn slice_mesh(mesh: &crate::gltf_load::CpuMesh, model: glam::Mat4, origin: Vec3, normal: Vec3,
    u: Vec3, v: Vec3, out: &mut Vec<((f32, f32), (f32, f32))>) {
    let vert = |i: u32| model.transform_point3(Vec3::from_array(mesh.vertices[i as usize].pos));
    for tri in mesh.indices.chunks_exact(3) {
        let p = [vert(tri[0]), vert(tri[1]), vert(tri[2])];
        let d = p.map(|q| (q - origin).dot(normal));
        let mut hits: Vec<Vec3> = Vec::with_capacity(2);
        for k in 0..3 {
            let (a, b) = (k, (k + 1) % 3);
            if (d[a] > 0.0) != (d[b] > 0.0) {
                let f = d[a] / (d[a] - d[b]);
                hits.push(p[a] + (p[b] - p[a]) * f);
            }
        }
        if hits.len() == 2 {
            let to2 = |q: Vec3| ((q - origin).dot(u), (q - origin).dot(v));
            out.push((to2(hits[0]), to2(hits[1])));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slicing_a_box_gives_its_outline_in_the_plane() {
        let mesh = crate::prims::box_mesh(Vec3::new(-1.0, 0.0, -2.0), Vec3::new(1.0, 2.0, 2.0), [100, 100, 100]);
        let mut segments = Vec::new();
        // The vertical plane x = 0.3: a 4 m by 2 m rectangle in (z, y).
        slice_mesh(&mesh, glam::Mat4::IDENTITY, Vec3::new(0.3, 0.0, 0.0), Vec3::X, Vec3::Z, Vec3::Y, &mut segments);
        assert!(!segments.is_empty());
        let length: f32 = segments.iter().map(|(a, b)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()).sum();
        assert!((length - 12.0).abs() < 1e-3, "outline length {length}");
        for (a, b) in &segments {
            for p in [a, b] {
                assert!(p.0 >= -2.0 - 1e-4 && p.0 <= 2.0 + 1e-4 && p.1 >= -1e-4 && p.1 <= 2.0 + 1e-4);
            }
        }
    }

    #[test]
    fn svg_text_is_escaped_and_the_document_closes() {
        let mut svg = Svg::new(100.0, 50.0, [0, 0, 0]);
        svg.text(1.0, 2.0, 10.0, "#fff", "a < b & c > d");
        let doc = svg.finish();
        assert!(doc.contains("a &lt; b &amp; c &gt; d"));
        assert!(doc.trim_end().ends_with("</svg>"));
        assert_eq!(heat_colour(AMBIENT_K, [10, 20, 30]), [10, 20, 30]);
        assert!(heat_colour(1700.0, [0, 0, 0])[2] > heat_colour(950.0, [0, 0, 0])[2]);
    }
}
