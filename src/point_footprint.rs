//! Point footprints (the owner, 2026-11-03: "never use a star particle shape"). A point-like
//! particle (a hot chip, a spark, an ember, a tracer's head, a snow grain) is one centred square on
//! the internal grid, and nothing after it spreads a bright pixel into a cross: the concussed eye's
//! defocus is a blur circle (crush.wgsl `defocus_disc`), mirrored here.

use glam::{IVec2, Vec2};
use std::collections::HashMap;

/// crush.wgsl `defocus_disc`: each pixel within reach weighted by its share inside the blur
/// circle, as offsets (pixels) and normalised weights.
fn defocus_taps(r_px: f32) -> Vec<(Vec2, f32)> {
    let reach = r_px.ceil() as i32;
    let mut taps = Vec::new();
    for y in -reach..=reach {
        for x in -reach..=reach {
            let o = Vec2::new(x as f32, y as f32);
            let w = (r_px + 0.5 - o.length()).clamp(0.0, 1.0);
            if w > 0.0 { taps.push((o, w)); }
        }
    }
    let total: f32 = taps.iter().map(|t| t.1).sum();
    taps.into_iter().map(|(o, w)| (o, w / total)).collect()
}

/// The concussion's blur radius, pixels, at strength `c` (crush.wgsl `fs`).
fn defocus_radius(c: f32) -> f32 { 2.0 + 3.5 * c }

/// The pixels a nearest-sampled tap set reads from a pixel's centre, with their weights: how one
/// bright pixel spreads into its neighbours.
fn footprint(taps: &[(Vec2, f32)]) -> HashMap<IVec2, f32> {
    let mut out = HashMap::new();
    for &(o, w) in taps {
        let cell = IVec2::new((0.5 + o.x).floor() as i32, (0.5 + o.y).floor() as i32);
        *out.entry(cell).or_insert(0.0) += w;
    }
    out
}

/// The share of a footprint's weight on the two axes through its centre, over the share of the
/// disc's pixels that lie on them: 1 for an even disc, far above it for a cross or a star.
fn axial_bias(foot: &HashMap<IVec2, f32>, r_px: f32) -> f32 {
    let total: f32 = foot.values().sum();
    let axial: f32 = foot.iter().filter(|(c, _)| c.x == 0 || c.y == 0).map(|(_, w)| *w).sum();
    let reach = r_px.floor() as i32;
    let (mut inside, mut on_axes) = (0, 0);
    for y in -reach..=reach {
        for x in -reach..=reach {
            if ((x * x + y * y) as f32) <= r_px * r_px {
                inside += 1;
                if x == 0 || y == 0 { on_axes += 1; }
            }
        }
    }
    (axial / total) / (on_axes as f32 / inside as f32)
}

#[test]
fn the_concussed_eye_spreads_a_point_into_a_disc_not_a_cross() {
    for c in [0.0f32, 0.05, 0.2, 0.5, 0.8, 1.0] {
        let r = defocus_radius(c);
        let taps = defocus_taps(r);
        assert!(taps.iter().all(|(o, _)| o.length() <= r + 0.5), "a tap outside the blur circle at r {r}");
        let foot = footprint(&taps);
        // The diagonal neighbours as fully lit as the axial ones, every one of them.
        let axial = foot[&IVec2::new(1, 0)];
        for d in [IVec2::new(1, 1), IVec2::new(-1, 1), IVec2::new(1, -1), IVec2::new(-1, -1)] {
            let w = foot.get(&d).copied().unwrap_or(0.0);
            assert!(w >= 0.95 * axial, "diagonal {d} at {w} against axial {axial} at r {r}");
        }
        let bias = axial_bias(&foot, r);
        assert!(bias < 1.25, "the axes take {bias:.2} times their share at r {r}");
    }
    // The former kernel (the centre 0.40 and four axial taps 0.15 at 1 + 4.5 c pixels) fails it.
    for c in [0.33f32, 0.66, 1.0] {
        let d = 1.0 + 4.5 * c;
        let old = [(Vec2::ZERO, 0.4), (Vec2::new(d, 0.0), 0.15), (Vec2::new(-d, 0.0), 0.15),
            (Vec2::new(0.0, d), 0.15), (Vec2::new(0.0, -d), 0.15)];
        assert!(axial_bias(&footprint(&old), d) > 2.0, "the test must catch the old cross at c {c}");
    }
}

#[test]
fn crush_draws_the_mirrored_disc() {
    let crush = include_str!("../shaders/crush.wgsl");
    for needle in ["let reach = i32(ceil(r_px));", "let w = clamp(r_px + 0.5 - length(o), 0.0, 1.0);",
        "textureSampleLevel(hdr, samp, uv + o * texel, 0.0).rgb * w", "defocus_disc(focus_uv, texel, 2.0 + 3.5 * concussion)"] {
        assert!(crush.contains(needle), "crush.wgsl lacks {needle}");
    }
    assert!(!crush.contains("vec2(radius.x, 0.0)"), "the five-tap cross is back");
}

#[test]
fn every_point_particle_is_one_pixel_square() {
    let unit = "array<vec2<f32>,6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0))";
    for (name, source) in [
        ("rm_points.wgsl", include_str!("../shaders/rm_points.wgsl")),
        ("rm_sparks.wgsl", include_str!("../shaders/rm_sparks.wgsl")),
        ("weapon_particles.wgsl", include_str!("../shaders/weapon_particles.wgsl")),
        ("pixel_particles.wgsl", include_str!("../shaders/pixel_particles.wgsl")),
        ("stylized_particles.wgsl", include_str!("../shaders/stylized_particles.wgsl")),
    ] {
        let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        let quads = compact.matches("array<vec2<f32>,6>(").count();
        assert!(quads > 0, "{name} draws no point quad");
        assert_eq!(compact.matches(unit).count(), quads, "{name}: a point quad that is not the one-pixel square");
    }
}
