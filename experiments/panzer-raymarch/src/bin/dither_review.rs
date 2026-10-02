//! Frozen native-pixel display review, not another scene render or tone-map pass.
//!
//! Usage: dither_review plain.png output_prefix [--algorithm all|bayer4|bayer8|blue-noise|floyd-steinberg]
//!        [--pair-penalty 0.12]
//!
//! Input is an already tone-mapped sRGB PNG. Decode once to display-linear RGB, then use the
//! exact palette constants and exhaustive OKLab pair search from ../shaders/crush.wgsl. The
//! first three methods change only the threshold field. The fourth uses serpentine Floyd–
//! Steinberg error diffusion in linear light, with the same pair quantizer at threshold 0.5.
//! No source, lighting, exposure, contrast, gamma or resolution is changed.
//!
//! Blue noise is a real 64x64 void-and-cluster rank array: a deterministic random 10% seed,
//! toroidal Gaussian density (sigma=1.5 px), cluster/void relaxation, then three ranking phases.
//! A pseudorandom generator supplies only the initial seed; it is NOT the finished threshold.
//! Reference: R. Ulichney, The Void-and-Cluster Method for Dither Array Generation (1993),
//! https://cv.ulichney.com/papers/1993-void-cluster.pdf and Filter Design (1994),
//! https://cv.ulichney.com/papers/1994-filter-design.pdf .

use anyhow::{bail, ensure, Context, Result};
use glam::Vec3;
use image::{ImageEncoder, Rgba, RgbaImage};
use std::{collections::HashMap, fs::OpenOptions, io::Write, path::PathBuf, time::Instant};

const PAL: [[f32; 3]; 16] = [
    [0.000, 0.000, 0.000],
    [0.114, 0.169, 0.325],
    [0.494, 0.145, 0.325],
    [0.000, 0.529, 0.318],
    [0.671, 0.322, 0.212],
    [0.373, 0.341, 0.310],
    [0.761, 0.765, 0.780],
    [1.000, 0.945, 0.910],
    [1.000, 0.000, 0.302],
    [1.000, 0.639, 0.000],
    [1.000, 0.925, 0.153],
    [0.000, 0.894, 0.212],
    [0.161, 0.678, 1.000],
    [0.514, 0.463, 0.612],
    [1.000, 0.467, 0.659],
    [1.000, 0.800, 0.667],
];
const BLUE_SIDE: usize = 64;
const BLUE_SEED: u64 = 0x7f4a_7c15_2026_1001;
const BLUE_SIGMA: f64 = 1.5;

fn decode(c: Vec3) -> Vec3 {
    let f = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    Vec3::new(f(c.x), f(c.y), f(c.z))
}
fn lab(c: Vec3) -> Vec3 {
    let l = (0.4122214708 * c.x + 0.5363325363 * c.y + 0.0514459929 * c.z)
        .max(0.0)
        .cbrt();
    let m = (0.2119034982 * c.x + 0.6806995451 * c.y + 0.1073969566 * c.z)
        .max(0.0)
        .cbrt();
    let s = (0.0883024619 * c.x + 0.2817188376 * c.y + 0.6299787005 * c.z)
        .max(0.0)
        .cbrt();
    Vec3::new(
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    )
}
#[derive(Clone, Copy, Debug)]
struct Pair {
    i: usize,
    j: usize,
    t: f32,
}
impl Pair {
    fn choose(self, threshold: f32) -> usize {
        if threshold < self.t {
            self.j
        } else {
            self.i
        }
    }
}
struct Palette {
    linear: [Vec3; 16],
    perceptual: [Vec3; 16],
    output: [Rgba<u8>; 16],
    penalty: f32,
}
impl Palette {
    fn new(penalty: f32) -> Self {
        let linear = PAL.map(|v| decode(Vec3::from_array(v)));
        Self {
            linear,
            perceptual: linear.map(lab),
            output: PAL.map(|v| {
                Rgba([
                    (v[0] * 255.0).round() as u8,
                    (v[1] * 255.0).round() as u8,
                    (v[2] * 255.0).round() as u8,
                    255,
                ])
            }),
            penalty,
        }
    }
    fn pair(&self, lin: Vec3) -> Pair {
        let target = lab(lin);
        let mut best = Pair { i: 0, j: 0, t: 0.0 };
        let mut best_err = f32::INFINITY;
        for i in 0..16 {
            for j in i..16 {
                let a = self.linear[i];
                let axis = self.linear[j] - a;
                let t = if axis.length_squared() > 1e-9 {
                    ((lin - a).dot(axis) / axis.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let err = lab(a + axis * t).distance(target)
                    + self.penalty * self.perceptual[i].distance(self.perceptual[j]);
                if err < best_err {
                    best_err = err;
                    best = Pair { i, j, t };
                }
            }
        }
        best
    }
}

/// Recursive Bayer ranks with the same orientation as the shader's explicit 8x8 table.
fn bayer_rank(mut x: usize, mut y: usize, side: usize) -> usize {
    let mut rank = 0;
    for _ in 0..side.trailing_zeros() {
        rank = rank * 4 + [0, 2, 3, 1][(y & 1) * 2 + (x & 1)];
        x >>= 1;
        y >>= 1;
    }
    rank
}
fn random(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    state.wrapping_mul(0x2545_f491_4f6c_dd1d)
}
struct Density {
    side: usize,
    occupied: Vec<bool>,
    energy: Vec<f64>,
    kernel: Vec<f64>,
}
impl Density {
    fn new(side: usize, occupied: Vec<bool>) -> Self {
        let kernel = (0..side * side)
            .map(|i| {
                let x = (i % side).min(side - i % side) as f64;
                let y = (i / side).min(side - i / side) as f64;
                (-(x * x + y * y) / (2.0 * BLUE_SIGMA * BLUE_SIGMA)).exp()
            })
            .collect();
        let mut state = Self {
            side,
            occupied: vec![false; side * side],
            energy: vec![0.0; side * side],
            kernel,
        };
        for (i, value) in occupied.into_iter().enumerate() {
            if value {
                state.set(i, true);
            }
        }
        state
    }
    fn set(&mut self, index: usize, value: bool) {
        assert_ne!(self.occupied[index], value);
        self.occupied[index] = value;
        let sign = if value { 1.0 } else { -1.0 };
        let mask = self.side - 1;
        for (i, energy) in self.energy.iter_mut().enumerate() {
            let x = (i % self.side + self.side - index % self.side) & mask;
            let y = (i / self.side + self.side - index / self.side) & mask;
            *energy += sign * self.kernel[y * self.side + x];
        }
    }
    fn cluster(&self) -> usize {
        self.extreme(true)
    }
    fn void(&self) -> usize {
        self.extreme(false)
    }
    fn extreme(&self, cluster: bool) -> usize {
        let mut result = None;
        let mut best = if cluster {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        for i in 0..self.energy.len() {
            if self.occupied[i] != cluster {
                continue;
            }
            if (cluster && self.energy[i] > best) || (!cluster && self.energy[i] < best) {
                best = self.energy[i];
                result = Some(i);
            }
        }
        result.expect("a nonempty majority/minority set in this phase")
    }
}
struct BlueNoise {
    ranks: Vec<u16>,
    relaxations: usize,
}
fn blue_noise(side: usize) -> Result<BlueNoise> {
    ensure!(
        side.is_power_of_two() && side <= 64 && side >= 8,
        "invalid blue-noise tile size"
    );
    let n = side * side;
    let seed_count = n / 10;
    let mut permutation: Vec<_> = (0..n).collect();
    let mut random_state = BLUE_SEED;
    for i in (1..n).rev() {
        let j = (random(&mut random_state) % (i + 1) as u64) as usize;
        permutation.swap(i, j);
    }
    let mut occupied = vec![false; n];
    for &i in &permutation[..seed_count] {
        occupied[i] = true;
    }
    let mut density = Density::new(side, occupied);
    let mut relaxations = 0;
    // Relax until removing the tightest cluster makes that same location the largest void.
    // The cap is an explicit failure, never silently accepting a random unfinished mask.
    loop {
        let cluster = density.cluster();
        density.set(cluster, false);
        let void = density.void();
        density.set(void, true);
        if cluster == void {
            break;
        }
        relaxations += 1;
        ensure!(
            relaxations < n * 64,
            "void-and-cluster relaxation did not converge"
        );
    }
    let prototype = density.occupied.clone();
    let mut ranks = vec![u16::MAX; n];
    // Phase I: remove successively tightest minority clusters for the low ranks.
    for rank in (0..seed_count).rev() {
        let i = density.cluster();
        density.set(i, false);
        ranks[i] = rank as u16;
    }
    // Phase II: restore the relaxed prototype and insert into largest voids up to half full.
    density = Density::new(side, prototype);
    for rank in seed_count..n / 2 {
        let i = density.void();
        density.set(i, true);
        ranks[i] = rank as u16;
    }
    // Phase III: holes are now the minority; removing their clusters gives the upper ranks.
    density = Density::new(side, density.occupied.iter().map(|v| !v).collect());
    for rank in n / 2..n {
        let i = density.cluster();
        density.set(i, false);
        ranks[i] = rank as u16;
    }
    Ok(BlueNoise { ranks, relaxations })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Algorithm {
    Bayer4,
    Bayer8,
    BlueNoise,
    FloydSteinberg,
}
impl Algorithm {
    const ALL: [Self; 4] = [
        Self::Bayer4,
        Self::Bayer8,
        Self::BlueNoise,
        Self::FloydSteinberg,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Bayer4 => "bayer4",
            Self::Bayer8 => "bayer8",
            Self::BlueNoise => "blue-noise",
            Self::FloydSteinberg => "floyd-steinberg",
        }
    }
}
fn linear_pixel(p: Rgba<u8>) -> Vec3 {
    decode(Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) / 255.0)
}
fn ordered(
    input: &RgbaImage,
    palette: &Palette,
    method: Algorithm,
    blue: Option<&BlueNoise>,
    cache: &mut HashMap<[u8; 3], Pair>,
) -> RgbaImage {
    RgbaImage::from_fn(input.width(), input.height(), |x, y| {
        let p = *input.get_pixel(x, y);
        let pair = *cache
            .entry([p[0], p[1], p[2]])
            .or_insert_with(|| palette.pair(linear_pixel(p)));
        let threshold = match method {
            Algorithm::Bayer4 => (bayer_rank(x as usize, y as usize, 4) as f32 + 0.5) / 16.0,
            Algorithm::Bayer8 => (bayer_rank(x as usize, y as usize, 8) as f32 + 0.5) / 64.0,
            Algorithm::BlueNoise => {
                (blue.unwrap().ranks[(y as usize % BLUE_SIDE) * BLUE_SIDE + x as usize % BLUE_SIDE]
                    as f32
                    + 0.5)
                    / (BLUE_SIDE * BLUE_SIDE) as f32
            }
            Algorithm::FloydSteinberg => unreachable!(),
        };
        let mut out = palette.output[pair.choose(threshold)];
        out[3] = p[3];
        out
    })
}
fn floyd_steinberg(input: &RgbaImage, palette: &Palette) -> RgbaImage {
    let w = input.width() as usize;
    let h = input.height() as usize;
    let mut out = RgbaImage::new(input.width(), input.height());
    let mut row = vec![Vec3::ZERO; w];
    let mut next = vec![Vec3::ZERO; w];
    for y in 0..h {
        let forward = y % 2 == 0;
        for step in 0..w {
            let x = if forward { step } else { w - 1 - step };
            let p = *input.get_pixel(x as u32, y as u32);
            let corrected = linear_pixel(p) + row[x];
            // Use the same palette/pair metric. Diffuse the full linear residual, including any
            // out-of-gamut correction; clamp only the quantizer's legal display input.
            let index = palette
                .pair(corrected.clamp(Vec3::ZERO, Vec3::ONE))
                .choose(0.5);
            let mut pixel = palette.output[index];
            pixel[3] = p[3];
            out.put_pixel(x as u32, y as u32, pixel);
            let error = corrected - palette.linear[index];
            let ahead = if forward {
                x.checked_add(1).filter(|v| *v < w)
            } else {
                x.checked_sub(1)
            };
            let behind = if forward {
                x.checked_sub(1)
            } else {
                x.checked_add(1).filter(|v| *v < w)
            };
            if let Some(i) = ahead {
                row[i] += error * (7.0 / 16.0);
                next[i] += error * (1.0 / 16.0);
            }
            if let Some(i) = behind {
                next[i] += error * (3.0 / 16.0);
            }
            next[x] += error * (5.0 / 16.0);
        }
        std::mem::swap(&mut row, &mut next);
        next.fill(Vec3::ZERO);
    }
    out
}
fn fingerprint(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    format!("{h:016x}")
}
fn quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn png_new(path: &PathBuf, img: &RgbaImage) -> Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    image::codecs::png::PngEncoder::new(file).write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|v| v == "--export-blue-noise") {
        ensure!(
            args.len() == 2,
            "usage: dither_review --export-blue-noise OUTPUT.bin"
        );
        let path = PathBuf::from(&args[1]);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        ensure!(!path.exists(), "refusing overwrite: {}", path.display());
        let blue = blue_noise(BLUE_SIDE)?;
        let bytes: Vec<_> = blue.ranks.iter().flat_map(|r| r.to_le_bytes()).collect();
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?
            .write_all(&bytes)?;
        eprintln!(
            "{}: 64x64 u16LE ranks0..4095, {} relaxation swaps, FNV1a64={}",
            path.display(),
            blue.relaxations,
            fingerprint(&bytes)
        );
        return Ok(());
    }
    ensure!(args.len()>=2,"usage: dither_review INPUT.png OUTPUT_PREFIX [--algorithm all|bayer4|bayer8|blue-noise|floyd-steinberg] [--pair-penalty 0.12]");
    let input_path = std::fs::canonicalize(&args[0]).context("input PNG")?;
    let mut prefix = PathBuf::from(&args[1]);
    if prefix.parent().is_none_or(|p| p.as_os_str().is_empty()) {
        prefix = PathBuf::from("captures").join(prefix);
    }
    let mut methods = Algorithm::ALL.to_vec();
    let mut penalty = 0.12;
    let mut i = 2;
    while i < args.len() {
        ensure!(i + 1 < args.len(), "missing value for {}", args[i]);
        match args[i].as_str() {
            "--algorithm" => {
                methods = if args[i + 1] == "all" {
                    Algorithm::ALL.to_vec()
                } else {
                    vec![Algorithm::ALL
                        .into_iter()
                        .find(|m| m.name() == args[i + 1])
                        .context("unknown algorithm")?]
                };
            }
            "--pair-penalty" => {
                penalty = args[i + 1].parse::<f32>()?;
                ensure!(
                    penalty.is_finite() && penalty >= 0.0,
                    "invalid pair penalty"
                );
            }
            other => bail!("unknown option {other}"),
        }
        i += 2;
    }
    if let Some(parent) = prefix.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let prefix = prefix.to_string_lossy();
    let paths: Vec<_> = methods
        .iter()
        .map(|m| PathBuf::from(format!("{prefix}_{}.png", m.name())))
        .collect();
    let receipt_path = PathBuf::from(format!("{prefix}_dither_review.json"));
    for path in paths.iter().chain(std::iter::once(&receipt_path)) {
        ensure!(!path.exists(), "refusing overwrite: {}", path.display());
    }
    let input_bytes = std::fs::read(&input_path)?;
    let input =
        image::load_from_memory_with_format(&input_bytes, image::ImageFormat::Png)?.into_rgba8();
    ensure!(input.width() > 0 && input.height() > 0, "empty input image");
    let started = Instant::now();
    let blue = if methods.contains(&Algorithm::BlueNoise) {
        Some(blue_noise(BLUE_SIDE)?)
    } else {
        None
    };
    let palette = Palette::new(penalty);
    let mut cache = HashMap::new();
    let mut outputs = Vec::new();
    for (method, path) in methods.iter().zip(paths.iter()) {
        let t = Instant::now();
        let out = if *method == Algorithm::FloydSteinberg {
            floyd_steinberg(&input, &palette)
        } else {
            ordered(&input, &palette, *method, blue.as_ref(), &mut cache)
        };
        png_new(path, &out)?;
        outputs.push(format!(
            "{{\"algorithm\":{},\"path\":{},\"rgba_fnv1a64\":{},\"elapsed_seconds\":{:.6}}}",
            quoted(method.name()),
            quoted(&std::fs::canonicalize(path)?.to_string_lossy()),
            quoted(&fingerprint(out.as_raw())),
            t.elapsed().as_secs_f64()
        ));
        eprintln!(
            "{}: {} ({:.2}s)",
            method.name(),
            path.display(),
            t.elapsed().as_secs_f64()
        );
    }
    let blue_receipt = if let Some(blue) = &blue {
        let bytes: Vec<u8> = blue.ranks.iter().flat_map(|v| v.to_le_bytes()).collect();
        format!("{{\"method\":\"three-phase void-and-cluster\",\"tile\":[64,64],\"gaussian_sigma_pixels\":1.5,\"periodic\":true,\"initial_minority_pixels\":409,\"seed\":\"0x{BLUE_SEED:016x}\",\"relaxation_swaps\":{},\"converged\":true,\"rank_le16_fnv1a64\":{},\"reference\":\"https://cv.ulichney.com/papers/1993-void-cluster.pdf\"}}",blue.relaxations,quoted(&fingerprint(&bytes)))
    } else {
        "null".to_owned()
    };
    let palette_bytes: Vec<_> = palette
        .output
        .iter()
        .map(|v| format!("[{},{},{}]", v[0], v[1], v[2]))
        .collect();
    let receipt=format!("{{\n\"schema\":\"panzer.frozen-dither-review.v1\",\n\"input\":{},\n\"input_png_fnv1a64\":{},\n\"native_extent\":[{},{}],\n\"scene_advanced\":false,\n\"tone_mapping_reapplied\":false,\n\"input_interpretation\":\"Already tone-mapped 8-bit sRGB PNG; inverse sRGB to display-linear RGB. Existing PNG quantization is retained. No resize or camera/exposure change. Alpha retained.\",\n\"palette_source\":\"experiments/panzer-raymarch/shaders/crush.wgsl PAL\",\n\"palette_rgb8\":[{}],\n\"pair_search\":\"136 unordered pairs; linear-light projected ratio; OKLab blend error plus penalty times endpoint OKLab distance, matching dither_best_pair\",\n\"pair_penalty\":{},\n\"thresholds\":\"Bayer4/Bayer8/blue-noise use identical per-input-pixel pairs with centered rank thresholds. Pattern is fixed in native screen coordinates.\",\n\"floyd_steinberg\":\"Serpentine row traversal; same pair quantizer at threshold0.5 on error-corrected input; diffuse linear RGB residual using7/16 ahead,3/16 next-row behind,5/16 below,1/16 next-row ahead. Boundary remainder discarded. Temporal stability is not established by a still.\",\n\"blue_noise\":{},\n\"outputs\":[{}],\n\"elapsed_seconds\":{:.6}\n}}\n",
        quoted(&input_path.to_string_lossy()),quoted(&fingerprint(&input_bytes)),input.width(),input.height(),palette_bytes.join(","),penalty,blue_receipt,outputs.join(",\n"),started.elapsed().as_secs_f64());
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&receipt_path)?
        .write_all(receipt.as_bytes())?;
    eprintln!("receipt: {}", receipt_path.display());
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bayer_tables_match_shader_orientation_and_centered_coverage() {
        assert_eq!(
            (0..4).map(|x| bayer_rank(x, 0, 4)).collect::<Vec<_>>(),
            vec![0, 8, 2, 10]
        );
        assert_eq!(
            (0..8).map(|x| bayer_rank(x, 0, 8)).collect::<Vec<_>>(),
            vec![0, 32, 8, 40, 2, 34, 10, 42]
        );
        for side in [4, 8] {
            let mut ranks: Vec<_> = (0..side * side)
                .map(|i| bayer_rank(i % side, i / side, side))
                .collect();
            ranks.sort_unstable();
            assert_eq!(ranks, (0..side * side).collect::<Vec<_>>());
            assert_eq!(
                ranks
                    .iter()
                    .filter(|r| (**r as f32 + 0.5) / ((side * side) as f32) < 0.5)
                    .count(),
                side * side / 2
            );
        }
    }
    fn low_frequency_power(ranks: &[u16], fraction: f64) -> f64 {
        let side = (ranks.len() as f64).sqrt() as usize;
        let mut sum = 0.0;
        for ky in -3i32..=3 {
            for kx in -3i32..=3 {
                if kx * kx + ky * ky == 0 || kx * kx + ky * ky > 9 {
                    continue;
                }
                let mut re = 0.0;
                let mut im = 0.0;
                for (i, rank) in ranks.iter().enumerate() {
                    let value = if (*rank as f64 + 0.5) / (ranks.len() as f64) < fraction {
                        1.0 - fraction
                    } else {
                        -fraction
                    };
                    let angle = std::f64::consts::TAU
                        * (kx as f64 * (i % side) as f64 + ky as f64 * (i / side) as f64)
                        / side as f64;
                    re += value * angle.cos();
                    im += value * angle.sin();
                }
                sum += re * re + im * im;
            }
        }
        sum
    }
    #[test]
    fn void_and_cluster_is_complete_deterministic_and_suppresses_low_frequencies() {
        let first = blue_noise(64).unwrap();
        let second = blue_noise(64).unwrap();
        assert_eq!(first.ranks, second.ranks);
        let mut sorted = first.ranks.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..4096).collect::<Vec<u16>>());
        let mut shuffled = sorted;
        let mut state = BLUE_SEED;
        for i in (1..shuffled.len()).rev() {
            let j = (random(&mut state) % (i + 1) as u64) as usize;
            shuffled.swap(i, j);
        }
        for fraction in [0.125, 0.5, 0.875] {
            let blue = low_frequency_power(&first.ranks, fraction);
            let white = low_frequency_power(&shuffled, fraction);
            assert!(
                blue < white * 0.20,
                "low-frequency power at {fraction}: blue={blue}, shuffled={white}"
            );
        }
    }
    #[test]
    fn algorithms_keep_native_pixels_alpha_and_palette_and_diffusion_preserves_mean() {
        let palette = Palette::new(0.12);
        let blue = blue_noise(64).unwrap();
        let input = RgbaImage::from_pixel(96, 64, Rgba([128, 128, 128, 173]));
        let mut cache = HashMap::new();
        let mut hashes = Vec::new();
        for method in Algorithm::ALL {
            let out = if method == Algorithm::FloydSteinberg {
                floyd_steinberg(&input, &palette)
            } else {
                ordered(&input, &palette, method, Some(&blue), &mut cache)
            };
            assert_eq!(out.dimensions(), input.dimensions());
            assert!(out
                .pixels()
                .all(|p| p[3] == 173 && palette.output.iter().any(|q| p.0[..3] == q.0[..3])));
            if method == Algorithm::FloydSteinberg {
                let mean = out.pixels().fold(Vec3::ZERO, |s, p| s + linear_pixel(*p))
                    / (out.width() * out.height()) as f32;
                assert!(
                    mean.distance(linear_pixel(input[(0, 0)])) < 0.01,
                    "linear mean {mean:?}"
                );
            }
            hashes.push(fingerprint(out.as_raw()));
        }
        hashes.sort();
        hashes.dedup();
        assert_eq!(
            hashes.len(),
            4,
            "four distinct threshold/diffusion patterns"
        );
    }
}
