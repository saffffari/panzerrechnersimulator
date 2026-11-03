//! Low cloud and mist banks inside the playable volume (2026-11-02).
//!
//! The cloud deck (`physical_environment.rs`, the sky cache) lives kilometres up; nothing cloud-like
//! drifted through the space the player drives in. A map can now author a `low_clouds` block in its
//! `manifest_edits.json`: a seeded set of banks, each a volume of liquid water drops 30-300 m
//! across whose base sits from the ground up to about 150 m. Absent block = no banks, so existing
//! maps are unchanged.
//!
//! What the renderer computes (raymarch backend, `shaders/rm_common.wgsl` `rm_low_*`):
//!
//! * **Extinction from the liquid water content.** For drops much larger than the wavelength the
//!   extinction efficiency tends to 2 (the extinction paradox; Hansen & Travis 1974, Space Sci.
//!   Rev. 16), so a population of effective radius r_e and liquid water content W has
//!   sigma_ext = 3 W / (2 rho_w r_e) (Stephens 1978, J. Atmos. Sci. 35; Hu & Stamnes 1993).
//!   W = 0.2 g/m^3 and r_e = 8 um give 0.0375 /m (visibility 3.9/sigma about 100 m, a fog).
//!   Typical values: radiation fog W 0.05-0.2 g/m^3, r_e 4-10 um; stratus/stratocumulus W 0.1-0.5
//!   g/m^3, r_e 6-12 um (Gultepe et al. 2007, Pure Appl. Geophys. 164, "Fog research: a review").
//! * **Mie scattering of water drops**: single-scattering albedo 0.9999 in the visible (water
//!   barely absorbs there), asymmetry g 0.85 (Hansen & Travis 1974, cloud drops in the visible),
//!   as a Henyey-Greenstein lobe through the backend's shared dense-medium phase.
//! * **One transport**: the banks are a density field in the world medium. Inside the froxel grid
//!   they are lit like every other medium; every query of the key beam (`rm_sun_visibility`:
//!   surfaces, tanks, the medium, glass, points) multiplies in the banks' transmittance toward the
//!   key body, marched through the same field, so they shadow the ground, the tanks and
//!   themselves. Those depths (toward the key body and straight up) and the banks' extinction are
//!   filled each frame into a grid round the eye (`shaders/rm_low_clouds.wgsl`, 176 x 64 x 176
//!   texels over 704 m, 4 m across; half its slabs a frame while it holds still) that the froxel
//!   pass and the shade sample; outside it they march the field. Beyond the froxel grid's 320 m
//!   the far segment marches each bank the view ray crosses.
//! * **Multiple scattering, named approximation**: a bank's diffusely transmitted light is the
//!   two-stream estimate for a conservative, non-absorbing layer, T = 1 / (1 + 3/4 (1 - g) tau)
//!   (Bohren 1987, Am. J. Phys. 55, "Multiple scattering of light and some of its observable
//!   consequences"): skylight under/inside a bank is the sky times T of the optical depth straight
//!   up, and the key beam's diffused share (T - exp(-tau) along the beam) is added as isotropic
//!   radiance. Not a radiative-transfer solve: no angular structure, no inter-bank exchange.
//! * **Shape**: an ellipsoid per bank, flattened below its centre (a base), its edge broken by a
//!   two-scale value noise that churns slowly in the bank's own frame. Authored shape, not a
//!   cloud-physics (condensation/entrainment) model.
//! * **Drift**: each bank moves with the renderer's world wind (`weather_wind_world` times the
//!   weather's gust), integrated frame by frame, scaled by the block's `drift` factor; the banks
//!   wrap through the authored area and fade at its edge (no bank pops in view of a camera
//!   inside the area unless it sits at the edge). The base follows the mean ground under the
//!   bank's footprint, so hills poke into a bank and valleys hold one.
//!
//! The gameplay simulation does not read the banks: AI perception and the player's optics agree
//! only through what is rendered (named in docs/PHYSICAL_TRUTH_AUDIT.md). `optical_depth` below is
//! the CPU twin of the shader field's envelope for the tests and a future perception hookup.

use glam::{Vec2, Vec3};
use std::sync::Mutex;

/// Top-level key in a revision's `manifest_edits.json`.
pub const KEY: &str = "low_clouds";
/// The most banks the frame carries (rm_common.wgsl `RM_LOW_BANKS`).
pub const MAX_BANKS: usize = 32;
/// Single-scattering albedo of water drops in the visible (Hansen & Travis 1974).
pub const DROP_ALBEDO: f32 = 0.9999;
/// Asymmetry parameter of cloud and fog drops in the visible (Hansen & Travis 1974).
pub const DROP_ASYMMETRY: f32 = 0.85;
/// Density of liquid water, kg/m^3.
const RHO_WATER: f32 = 1000.0;

/// Extinction coefficient (/m) of a drop population: sigma = 3 W / (2 rho_w r_e), extinction
/// efficiency 2 (geometric-optics limit). `lwc_g_m3` in g/m^3, `radius_um` in micrometres.
pub fn extinction_per_m(lwc_g_m3: f32, radius_um: f32) -> f32 {
    let w = lwc_g_m3.max(0.0) * 1.0e-3; // kg/m^3
    let r = radius_um.max(0.5) * 1.0e-6; // m
    1.5 * w / (RHO_WATER * r)
}

/// The authored block. Ranges are `[min, max]`; each bank draws its value from the range with a
/// counter-seeded hash.
#[derive(Clone, Debug, PartialEq)]
pub struct LowCloudSpec {
    pub seed: u32,
    pub count: u32,
    /// World rectangle `[min_x, min_z, max_x, max_z]` the banks drift through; `None` = the map's
    /// terrain extent less a 5 % margin.
    pub area: Option<[f32; 4]>,
    /// Height of a bank's base above the mean ground under it, m.
    pub base_m: [f32; 2],
    /// Horizontal extent (diameter along its long axis), m.
    pub size_m: [f32; 2],
    /// Vertical extent, m.
    pub depth_m: [f32; 2],
    /// Liquid water content at the bank's core, g/m^3.
    pub lwc_g_m3: [f32; 2],
    /// Effective drop radius, um.
    pub droplet_radius_um: f32,
    /// Multiplier on the world wind for the drift (1 = moves with the air).
    pub drift: f32,
}

impl Default for LowCloudSpec {
    fn default() -> Self {
        Self {
            seed: 1,
            count: 0,
            area: None,
            base_m: [0.0, 60.0],
            size_m: [40.0, 220.0],
            depth_m: [15.0, 70.0],
            lwc_g_m3: [0.08, 0.30],
            droplet_radius_um: 8.0,
            drift: 1.0,
        }
    }
}

fn range(value: Option<&serde_json::Value>, fallback: [f32; 2], name: &str) -> anyhow::Result<[f32; 2]> {
    let Some(value) = value else { return Ok(fallback) };
    if let Some(x) = value.as_f64() {
        return Ok([x as f32, x as f32]);
    }
    let items = value.as_array().ok_or_else(|| anyhow::anyhow!("low_clouds.{name} must be a number or [min, max]"))?;
    anyhow::ensure!(items.len() == 2, "low_clouds.{name} must be [min, max]");
    let a = items[0].as_f64().ok_or_else(|| anyhow::anyhow!("low_clouds.{name}[0] must be a number"))? as f32;
    let b = items[1].as_f64().ok_or_else(|| anyhow::anyhow!("low_clouds.{name}[1] must be a number"))? as f32;
    Ok([a, b])
}

impl LowCloudSpec {
    /// Read the block from a parsed `manifest_edits.json`; absent (or `enabled: false`, or a count
    /// of zero) means no banks.
    pub fn from_edits(edits: Option<&serde_json::Value>) -> anyhow::Result<Option<Self>> {
        let Some(block) = edits.and_then(|edits| edits.get(KEY)) else { return Ok(None) };
        Self::from_json(block).map(|spec| spec.filter(|s| s.count > 0))
    }

    pub fn from_json(block: &serde_json::Value) -> anyhow::Result<Option<Self>> {
        anyhow::ensure!(block.is_object(), "low_clouds must be an object");
        if block.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
            return Ok(None);
        }
        let d = Self::default();
        let num = |name: &str, fallback: f32| -> anyhow::Result<f32> {
            match block.get(name) {
                None => Ok(fallback),
                Some(v) => v.as_f64().map(|x| x as f32).ok_or_else(|| anyhow::anyhow!("low_clouds.{name} must be a number")),
            }
        };
        let area = match block.get("area") {
            None | Some(serde_json::Value::Null) => None,
            Some(v) => {
                let items = v.as_array().ok_or_else(|| anyhow::anyhow!("low_clouds.area must be [min_x, min_z, max_x, max_z]"))?;
                anyhow::ensure!(items.len() == 4, "low_clouds.area must be [min_x, min_z, max_x, max_z]");
                let mut a = [0.0; 4];
                for (slot, item) in a.iter_mut().zip(items) {
                    *slot = item.as_f64().ok_or_else(|| anyhow::anyhow!("low_clouds.area holds a non-number"))? as f32;
                }
                Some(a)
            }
        };
        let spec = Self {
            seed: block.get("seed").and_then(serde_json::Value::as_u64).unwrap_or(d.seed as u64) as u32,
            count: block.get("count").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32,
            area,
            base_m: range(block.get("base_m"), d.base_m, "base_m")?,
            size_m: range(block.get("size_m"), d.size_m, "size_m")?,
            depth_m: range(block.get("depth_m"), d.depth_m, "depth_m")?,
            lwc_g_m3: range(block.get("lwc_g_m3"), d.lwc_g_m3, "lwc_g_m3")?,
            droplet_radius_um: num("droplet_radius_um", d.droplet_radius_um)?,
            drift: num("drift", d.drift)?,
        };
        spec.validate()?;
        Ok(Some(spec))
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.count as usize <= MAX_BANKS, "low_clouds.count {} exceeds {MAX_BANKS}", self.count);
        let ordered = |r: [f32; 2], lo: f32, hi: f32, name: &str| -> anyhow::Result<()> {
            anyhow::ensure!(r[0].is_finite() && r[1].is_finite() && r[0] <= r[1] && r[0] >= lo && r[1] <= hi,
                "low_clouds.{name} {:?} must be ordered within {lo}..{hi}", r);
            Ok(())
        };
        ordered(self.base_m, -50.0, 600.0, "base_m")?;
        ordered(self.size_m, 5.0, 1500.0, "size_m")?;
        ordered(self.depth_m, 2.0, 600.0, "depth_m")?;
        ordered(self.lwc_g_m3, 0.0, 3.0, "lwc_g_m3")?;
        anyhow::ensure!(self.droplet_radius_um.is_finite() && (1.0..=50.0).contains(&self.droplet_radius_um),
            "low_clouds.droplet_radius_um must lie in 1..50");
        anyhow::ensure!(self.drift.is_finite() && (0.0..=5.0).contains(&self.drift), "low_clouds.drift must lie in 0..5");
        if let Some(a) = self.area {
            anyhow::ensure!(a.iter().all(|v| v.is_finite()) && a[2] > a[0] + 10.0 && a[3] > a[1] + 10.0,
                "low_clouds.area must be [min_x, min_z, max_x, max_z] with positive size");
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn to_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "seed": self.seed, "count": self.count,
            "base_m": self.base_m, "size_m": self.size_m, "depth_m": self.depth_m,
            "lwc_g_m3": self.lwc_g_m3, "droplet_radius_um": self.droplet_radius_um, "drift": self.drift,
        });
        if let Some(area) = self.area { v["area"] = serde_json::json!(area); }
        v
    }
}

fn hash(n: u32) -> u32 {
    let mut s = n.wrapping_mul(2654435761);
    s ^= s >> 16;
    s = s.wrapping_mul(0x7feb352d);
    s ^= s >> 15;
    s = s.wrapping_mul(0x846ca68b);
    s ^ (s >> 16)
}
fn hash01(n: u32) -> f32 { (hash(n) >> 8) as f32 / 16_777_216.0 }
fn lerp(r: [f32; 2], t: f32) -> f32 { r[0] + (r[1] - r[0]) * t }

/// One bank, in its anchor frame (before drift).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bank {
    pub anchor: Vec2,
    pub base_m: f32,
    /// Semi-axes: along, up, across (m).
    pub radii: Vec3,
    pub yaw: f32,
    /// Extinction at the core, /m.
    pub sigma: f32,
    pub seed: u32,
}

#[derive(Debug)]
struct Drift {
    offset: Vec2,
    last_time: Option<f32>,
}

/// A map's banks plus the coarse ground they ride over. Built once per map; the renderer packs
/// it into the frame uniform each frame (`pack`).
#[derive(Debug)]
pub struct LowCloudField {
    pub spec: LowCloudSpec,
    pub banks: Vec<Bank>,
    /// `[min_x, min_z, max_x, max_z]`.
    pub area: [f32; 4],
    ground: Vec<f32>,
    ground_n: usize,
    drift: Mutex<Drift>,
}

const GROUND_N: usize = 96;

impl LowCloudField {
    /// Generate the banks of `spec` over `extent` (`[min_x, min_z, max_x, max_z]`, the terrain's),
    /// sampling `height` for the ground they follow.
    pub fn new(spec: LowCloudSpec, extent: [f32; 4], height: impl Fn(f32, f32) -> f32) -> Self {
        let area = spec.area.unwrap_or_else(|| {
            let mx = (extent[2] - extent[0]) * 0.05;
            let mz = (extent[3] - extent[1]) * 0.05;
            [extent[0] + mx, extent[1] + mz, extent[2] - mx, extent[3] - mz]
        });
        let mut ground = Vec::with_capacity(GROUND_N * GROUND_N);
        for j in 0..GROUND_N {
            for i in 0..GROUND_N {
                let x = area[0] + (area[2] - area[0]) * (i as f32 + 0.5) / GROUND_N as f32;
                let z = area[1] + (area[3] - area[1]) * (j as f32 + 0.5) / GROUND_N as f32;
                let h = height(x, z);
                ground.push(if h.is_finite() { h } else { 0.0 });
            }
        }
        let banks = (0..spec.count.min(MAX_BANKS as u32))
            .map(|i| {
                let k = spec.seed.wrapping_mul(7919).wrapping_add(i.wrapping_mul(101));
                let anchor = Vec2::new(lerp([area[0], area[2]], hash01(k)), lerp([area[1], area[3]], hash01(k + 1)));
                let size = lerp(spec.size_m, hash01(k + 2));
                let aspect = 0.45 + 0.55 * hash01(k + 3);
                let depth = lerp(spec.depth_m, hash01(k + 4)).min(size);
                let lwc = lerp(spec.lwc_g_m3, hash01(k + 5));
                Bank {
                    anchor,
                    base_m: lerp(spec.base_m, hash01(k + 6)),
                    radii: Vec3::new(size * 0.5, depth * 0.5, size * 0.5 * aspect),
                    yaw: hash01(k + 7) * std::f32::consts::TAU,
                    sigma: extinction_per_m(lwc, spec.droplet_radius_um),
                    seed: hash(k + 8) & 0xFFFF,
                }
            })
            .collect();
        Self { spec, banks, area, ground, ground_n: GROUND_N, drift: Mutex::new(Drift { offset: Vec2::ZERO, last_time: None }) }
    }

    /// From a manifest's edits; `None` when the map authors no banks.
    pub fn from_edits(edits: Option<&serde_json::Value>, extent: [f32; 4], height: impl Fn(f32, f32) -> f32) -> anyhow::Result<Option<Self>> {
        Ok(LowCloudSpec::from_edits(edits)?.map(|spec| Self::new(spec, extent, height)))
    }

    fn ground_at(&self, x: f32, z: f32) -> f32 {
        let n = self.ground_n;
        let fx = ((x - self.area[0]) / (self.area[2] - self.area[0]) * n as f32 - 0.5).clamp(0.0, n as f32 - 1.0);
        let fz = ((z - self.area[1]) / (self.area[3] - self.area[1]) * n as f32 - 0.5).clamp(0.0, n as f32 - 1.0);
        let (i0, j0) = (fx.floor() as usize, fz.floor() as usize);
        let (i1, j1) = ((i0 + 1).min(n - 1), (j0 + 1).min(n - 1));
        let (tx, tz) = (fx - i0 as f32, fz - j0 as f32);
        let g = |i: usize, j: usize| self.ground[j * n + i];
        let a = g(i0, j0) + (g(i1, j0) - g(i0, j0)) * tx;
        let b = g(i0, j1) + (g(i1, j1) - g(i0, j1)) * tx;
        a + (b - a) * tz
    }

    /// Advance the drift to `time` (s) under `wind` (world m/s, x and z). Steps longer than half a
    /// second (a pause, a time cut) carry no drift.
    pub fn advance(&self, time: f32, wind: Vec2) {
        let mut d = self.drift.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(last) = d.last_time {
            let dt = time - last;
            if dt > 0.0 && dt <= 0.5 && wind.is_finite() {
                d.offset += wind * self.spec.drift * dt;
            }
        }
        d.last_time = Some(time);
        let size = Vec2::new(self.area[2] - self.area[0], self.area[3] - self.area[1]);
        d.offset = Vec2::new(d.offset.x.rem_euclid(size.x), d.offset.y.rem_euclid(size.y));
    }

    /// `KESSEL_LOW_CLOUDS_LIST=1`: print every bank where it stands now (for review cameras).
    pub fn log_banks(&self) {
        if std::env::var("KESSEL_LOW_CLOUDS_LIST").as_deref() != Ok("1") { return; }
        for (i, (centre, sigma, bank)) in self.placed().into_iter().enumerate() {
            println!("[low-clouds] bank {i}: centre [{:.0}, {:.1}, {:.0}] semi-axes [{:.0}, {:.0}, {:.0}] yaw {:.2} sigma {:.4}/m (visibility {:.0} m)",
                centre.x, centre.y, centre.z, bank.radii.x, bank.radii.y, bank.radii.z, bank.yaw, sigma, 3.912 / sigma.max(1e-6));
        }
    }

    pub fn drift_offset(&self) -> Vec2 {
        self.drift.lock().unwrap_or_else(|e| e.into_inner()).offset
    }

    /// Each bank's world centre now, its extinction (faded at the area's edge) and its geometry.
    pub fn placed(&self) -> Vec<(Vec3, f32, &Bank)> {
        let offset = self.drift_offset();
        let min = Vec2::new(self.area[0], self.area[1]);
        let size = Vec2::new(self.area[2] - self.area[0], self.area[3] - self.area[1]);
        self.banks.iter().map(|b| {
            let p = b.anchor + offset - min;
            let p = Vec2::new(p.x.rem_euclid(size.x), p.y.rem_euclid(size.y)) + min;
            // Fade over one bank radius at the area's edge, so the wrap is never seen as a jump.
            let reach = b.radii.x.max(b.radii.z);
            let edge = (p.x - self.area[0]).min(self.area[2] - p.x).min(p.y - self.area[1]).min(self.area[3] - p.y);
            let t = (edge / reach.max(1.0)).clamp(0.0, 1.0);
            let fade = t * t * (3.0 - 2.0 * t);
            // The mean ground under the footprint: centre and four points at 0.7 of the radii.
            let (s, c) = b.yaw.sin_cos();
            let along = Vec2::new(c, -s) * b.radii.x * 0.7;
            let across = Vec2::new(s, c) * b.radii.z * 0.7;
            let ground = (self.ground_at(p.x, p.y)
                + self.ground_at(p.x + along.x, p.y + along.y) + self.ground_at(p.x - along.x, p.y - along.y)
                + self.ground_at(p.x + across.x, p.y + across.y) + self.ground_at(p.x - across.x, p.y - across.y)) / 5.0;
            // The shader flattens the lower half by 1.6, so the base lies radii.y / 1.6 below centre.
            let centre = Vec3::new(p.x, ground + b.base_m + b.radii.y / 1.6, p.y);
            (centre, b.sigma * fade, b)
        }).collect()
    }

    /// The vertical span the banks' optical-depth grid must hold round `centre` (x, z) within
    /// `half` metres: from 2 m under the lowest ground there to 5 m over the highest bank's top,
    /// rounded out to whole 8 m.
    pub fn vertical_span(&self, centre: Vec2, half: f32) -> (f32, f32) {
        let mut bottom = f32::INFINITY;
        for k in 0..=8 {
            for l in 0..=8 {
                let x = centre.x - half + 2.0 * half * k as f32 / 8.0;
                let z = centre.y - half + 2.0 * half * l as f32 / 8.0;
                bottom = bottom.min(self.ground_at(x, z));
            }
        }
        let top = self.placed().iter().map(|(c, _, b)| c.y + b.radii.y * ENVELOPE_REACH).fold(bottom + 20.0, f32::max);
        // Whole 8 m steps, so the span (and the grid's texels) hold still while the eye moves.
        (((bottom - 2.0) / 8.0).floor() * 8.0, ((top + 5.0) / 8.0).ceil() * 8.0)
    }

    /// The frame records (rm_common.wgsl `fr.low_banks`): per bank centre xyz + core extinction,
    /// semi-axes xyz + yaw, seed + bounding reach + 0 + 0; and the count.
    pub fn pack(&self) -> ([[f32; 4]; MAX_BANKS * 3], u32) {
        let mut out = [[0.0; 4]; MAX_BANKS * 3];
        let mut n = 0;
        for (centre, sigma, b) in self.placed() {
            if sigma <= 0.0 { continue; }
            out[n * 3] = [centre.x, centre.y, centre.z, sigma];
            out[n * 3 + 1] = [b.radii.x, b.radii.y, b.radii.z, b.yaw];
            out[n * 3 + 2] = [b.seed as f32, ENVELOPE_REACH * b.radii.max_element(), 0.0, 0.0];
            n += 1;
        }
        (out, n as u32)
    }

    /// Optical depth of the banks' envelope (no edge noise; the shader's field breaks the edge
    /// within the envelope's 1.35 reach) along a segment, by a 64-step midpoint march.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn optical_depth(&self, a: Vec3, b: Vec3) -> f32 {
        let placed = self.placed();
        let steps = 64;
        let dt = a.distance(b) / steps as f32;
        (0..steps).map(|i| {
            let p = a.lerp(b, (i as f32 + 0.5) / steps as f32);
            placed.iter().map(|(c, sigma, bank)| sigma * envelope(p - *c, bank)).sum::<f32>() * dt
        }).sum()
    }
}

/// The banks a manifest's sibling `manifest_edits.json` authors, over a centred square grid of
/// `res` x `res` heights `spacing` apart (the world editor's terrain). `None` when the map authors
/// none or the file is unreadable (the error is printed; the map still opens).
pub fn for_manifest(manifest: &std::path::Path, res: usize, spacing: f32, heights: &[f32]) -> Option<std::sync::Arc<LowCloudField>> {
    let path = manifest.with_file_name("manifest_edits.json");
    if !crate::resource::is_file(&path) || res < 2 || heights.len() < res * res { return None; }
    let edits = crate::resource::read(&path).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let half = (res as f32 - 1.0) * 0.5 * spacing;
    let height = |x: f32, z: f32| {
        let gx = (x / spacing + (res as f32 - 1.0) * 0.5).clamp(0.0, res as f32 - 1.001);
        let gz = (z / spacing + (res as f32 - 1.0) * 0.5).clamp(0.0, res as f32 - 1.001);
        let (i, j) = (gx as usize, gz as usize);
        let (tx, tz) = (gx - i as f32, gz - j as f32);
        let h = |i: usize, j: usize| heights[j.min(res - 1) * res + i.min(res - 1)];
        let a = h(i, j) + (h(i + 1, j) - h(i, j)) * tx;
        let b = h(i, j + 1) + (h(i + 1, j + 1) - h(i, j + 1)) * tx;
        a + (b - a) * tz
    };
    match LowCloudField::from_edits(edits.as_ref(), [-half, -half, half, half], height) {
        Ok(Some(field)) => {
            println!("[low-clouds] {} banks from {}", field.banks.len(), path.display());
            field.log_banks();
            Some(std::sync::Arc::new(field))
        }
        Ok(None) => None,
        Err(error) => { eprintln!("[low-clouds] {}: {error:#}", path.display()); None }
    }
}

/// The bank envelope's reach in units of its semi-axes (rm_common.wgsl `RM_LOW_REACH`).
pub const ENVELOPE_REACH: f32 = 1.35;

/// The smooth envelope of a bank at `rel` from its centre (without the shader's edge noise):
/// 1 in the core, 0 beyond the semi-axes.
#[cfg_attr(not(test), allow(dead_code))]
pub fn envelope(rel: Vec3, bank: &Bank) -> f32 {
    let (s, c) = bank.yaw.sin_cos();
    let local = Vec3::new(c * rel.x - s * rel.z, rel.y, s * rel.x + c * rel.z) / bank.radii;
    let q = Vec3::new(local.x, if local.y < 0.0 { local.y * 1.6 } else { local.y }, local.z);
    let r = q.length();
    let t = ((r - 0.45) / (1.0 - 0.45)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_clouds_extinction_from_liquid_water_matches_the_geometric_limit() {
        // 0.2 g/m^3 of 8 um drops: 3 W / (2 rho r) = 0.0375 /m.
        assert!((extinction_per_m(0.2, 8.0) - 0.0375).abs() < 1e-6);
        // Meteorological visibility (Koschmieder, 2 % contrast) about 104 m: a fog.
        assert!((3.912 / extinction_per_m(0.2, 8.0) - 104.3).abs() < 0.5);
    }

    #[test]
    fn low_clouds_absent_block_means_no_banks() {
        assert!(LowCloudSpec::from_edits(None).unwrap().is_none());
        assert!(LowCloudSpec::from_edits(Some(&serde_json::json!({"ops": []}))).unwrap().is_none());
        assert!(LowCloudSpec::from_edits(Some(&serde_json::json!({"low_clouds": {"count": 0}}))).unwrap().is_none());
        assert!(LowCloudSpec::from_edits(Some(&serde_json::json!({"low_clouds": {"count": 4, "enabled": false}}))).unwrap().is_none());
    }

    #[test]
    fn low_clouds_block_round_trips_and_rejects_nonsense() {
        let edits = serde_json::json!({"low_clouds": {"seed": 7, "count": 12, "base_m": [0, 40], "size_m": [60, 240],
            "depth_m": [20, 60], "lwc_g_m3": [0.1, 0.25], "droplet_radius_um": 7.5, "drift": 1.0, "area": [-500, -400, 500, 400]}});
        let spec = LowCloudSpec::from_edits(Some(&edits)).unwrap().unwrap();
        assert_eq!(spec.count, 12);
        assert_eq!(LowCloudSpec::from_json(&spec.to_json()).unwrap().unwrap(), spec);
        for bad in [serde_json::json!({"count": 99}), serde_json::json!({"count": 2, "size_m": [300, 30]}),
            serde_json::json!({"count": 2, "droplet_radius_um": 0.0}), serde_json::json!({"count": 2, "area": [0, 0, 1, 1]})] {
            assert!(LowCloudSpec::from_json(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn low_clouds_generation_is_deterministic_and_within_the_authored_ranges() {
        let spec = LowCloudSpec { count: 20, seed: 3, ..LowCloudSpec::default() };
        let a = LowCloudField::new(spec.clone(), [-1000.0, -1000.0, 1000.0, 1000.0], |_, _| 10.0);
        let b = LowCloudField::new(spec.clone(), [-1000.0, -1000.0, 1000.0, 1000.0], |_, _| 10.0);
        assert_eq!(a.banks, b.banks);
        for bank in &a.banks {
            assert!(bank.radii.x * 2.0 >= spec.size_m[0] - 1e-3 && bank.radii.x * 2.0 <= spec.size_m[1] + 1e-3);
            assert!(bank.base_m >= spec.base_m[0] && bank.base_m <= spec.base_m[1]);
            assert!(bank.sigma >= extinction_per_m(spec.lwc_g_m3[0], 8.0) - 1e-6);
        }
        // The base rides the ground: on flat ground at 10 m a bank's lowest envelope point is base_m up.
        for (centre, _, bank) in a.placed() {
            assert!((centre.y - bank.radii.y / 1.6 - (10.0 + bank.base_m)).abs() < 1e-3);
        }
    }

    #[test]
    fn low_clouds_drift_with_the_wind_and_wrap_inside_the_area() {
        let spec = LowCloudSpec { count: 1, seed: 9, area: Some([-100.0, -100.0, 100.0, 100.0]), ..LowCloudSpec::default() };
        let f = LowCloudField::new(spec, [-1.0e4, -1.0e4, 1.0e4, 1.0e4], |_, _| 0.0);
        let start = f.placed()[0].0;
        let mut t = 0.0;
        f.advance(t, Vec2::new(4.0, 0.0));
        for _ in 0..60 { t += 1.0 / 60.0; f.advance(t, Vec2::new(4.0, 0.0)); }
        let moved = f.placed()[0].0 - start;
        let dx = (moved.x + 200.0).rem_euclid(200.0);
        assert!((dx - 4.0).abs() < 0.05, "one second at 4 m/s moved {moved:?}");
        // A pause (a long step) carries no drift.
        f.advance(t + 30.0, Vec2::new(4.0, 0.0));
        assert!((f.placed()[0].0 - start - moved).length() < 1e-3);
        for _ in 0..3000 { t += 0.1; f.advance(t, Vec2::new(4.0, 3.0)); }
        let c = f.placed()[0].0;
        assert!(c.x >= -100.0 && c.x <= 100.0 && c.z >= -100.0 && c.z <= 100.0);
    }

    #[test]
    fn low_clouds_optical_depth_through_a_bank_core() {
        let spec = LowCloudSpec { count: 1, seed: 5, area: Some([-2000.0, -2000.0, 2000.0, 2000.0]), size_m: [200.0, 200.0],
            depth_m: [60.0, 60.0], lwc_g_m3: [0.2, 0.2], base_m: [0.0, 0.0], ..LowCloudSpec::default() };
        let f = LowCloudField::new(spec, [-1.0e4, -1.0e4, 1.0e4, 1.0e4], |_, _| 0.0);
        let (c, sigma, bank) = f.placed()[0];
        // Straight up through the core: about sigma times the envelope's effective thickness.
        let tau = f.optical_depth(c - Vec3::Y * 100.0, c + Vec3::Y * 100.0);
        assert!(tau > sigma * bank.radii.y * 0.9 && tau < sigma * bank.radii.y * 2.0, "tau {tau}");
        // Far from every bank: clear.
        assert_eq!(f.optical_depth(Vec3::new(3000.0, 0.0, 3000.0), Vec3::new(3000.0, 100.0, 3000.0)), 0.0);
    }
}
