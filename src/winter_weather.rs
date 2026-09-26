//! Seeded, continuous weather fronts shared by snow, visibility, wind and the audio bed.
//! These are art-directed weather cycles, not a meteorological forecast.

#[derive(Clone, Copy, Debug)]
pub struct WeatherFrame {
    pub storm: f32,
    pub fog_density: f32,
    pub wind_scale: f32,
}

fn hash(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(747796405).wrapping_add(2891336453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277803737);
    ((x >> 22) ^ x) as f32 / u32::MAX as f32
}

/// Each front takes 80–120 seconds to arrive with zero slope at its endpoints. The initial
/// authored weather is retained for ten seconds, then eases into a lull before a heavier front.
pub fn sample(seconds: f32, seed: u32, base_storm: f32, base_fog: f32) -> WeatherFrame {
    let base = if base_storm.is_finite() { base_storm.clamp(0.0, 1.0) } else { 0.66 };
    let fog = if base_fog.is_finite() { base_fog.clamp(0.0, 0.25) } else { 0.0018 };
    let time = if seconds.is_finite() { (seconds - 10.0).max(0.0) } else { 0.0 };
    let span = 80.0 + hash(seed) * 40.0;
    let leg = (time / span).floor() as u32;
    let target = |index: u32| {
        if index == 0 { return base; }
        let variation = hash(seed.wrapping_add(index.wrapping_mul(0x9e3779b9)));
        if index % 2 == 1 { 0.12 + variation * 0.12 } else { 0.90 + variation * 0.10 }
    };
    let t = (time / span).fract();
    let smooth = t * t * (3.0 - 2.0 * t);
    let storm = target(leg) + (target(leg.saturating_add(1)) - target(leg)) * smooth;
    // Return the exact authored baseline at t=0; apply a bounded shared weather excursion.
    let fog_density = (fog + 0.0045 * (storm * storm - base * base)).clamp(0.00035, 0.025);
    let wind_scale = (0.30 + 1.25 * storm) / (0.30 + 1.25 * base);
    WeatherFrame { storm, fog_density, wind_scale }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "GPU, selected winter manifest and fresh capture prefix required"]
    fn winter_fronts_drive_actual_snow_visibility_and_world_render() -> anyhow::Result<()> {
        use crate::{act_manifest, game::{Game, TankInput}, world};
        let prefix = std::env::var("KESSEL_WINTER_REVIEW_PREFIX")?;
        let path = std::path::PathBuf::from(std::env::var("KESSEL_PLAYTEST_MANIFEST")?);
        for tag in ["lull", "blizzard"] {
            anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_{tag}.png")).exists());
        }
        let manifest = act_manifest::ActManifest::load(&path)?;
        let lighting = crate::environment_authoring::LightingTuning::from_edits(manifest.edits())?;
        act_manifest::select_runtime_manifest(Some(path))?;
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
        g.mute_audio();
        g.debug_banish_enemies();
        let frames: Vec<_> = (10..360).map(|t| (t as f32,
            sample(t as f32, g.act.seed, lighting.storm, lighting.fog_density))).collect();
        let low = frames.iter().min_by(|a,b| a.1.storm.total_cmp(&b.1.storm)).unwrap();
        let high = frames.iter().max_by(|a,b| a.1.storm.total_cmp(&b.1.storm)).unwrap();
        let mut receipt = Vec::new();
        for (tag, &(seconds, expected)) in [("lull", low), ("blizzard", high)] {
            r.time = seconds;
            g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
            assert!((r.storm - expected.storm).abs() < 0.0001);
            assert!((r.fog_density - expected.fog_density).abs() < 0.000001);
            assert!((r.precipitation - expected.storm / 0.98).abs() < 0.025);
            // One common fixed view of the authored courtyard. Only the weather phase changes.
            r.camera.pos = glam::Vec3::new(1903.0,8.0,193.0);
            r.camera.set_forward(glam::Vec3::new(1880.0,7.0,167.0)-r.camera.pos);
            r.camera.fov_y = 60f32.to_radians(); r.reticle = false; r.optic_mask = 0.0;
            r.interior_light = 0.0; r.interior_haze = 0.0; r.weather_shelter = 0.0;
            r.set_interior_atmosphere(0.0,0.0,glam::Vec3::ZERO);
            // Populate camera-local vegetation before both renders, rather than leaving the
            // first capture with vegetation centered on the player's distant spawn.
            g.prepare_world_survey(&mut r, 0.0);
            r.reset_history();
            r.render_to_png(&device,&queue,&format!("{prefix}_{tag}.png"),r.iw*2,r.ih*2)?;
            receipt.push(serde_json::json!({"phase":tag,"staged_time_s":seconds,
                "storm":r.storm,"snow":r.precipitation,"fog_density":r.fog_density,
                "wind_scale":expected.wind_scale,"internal_resolution":[r.iw,r.ih],
                "vegetation":g.world_survey_grass_stats()}));
        }
        std::fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"Actual saved Play world and ordinary weather update; staged clock phases, common exterior camera",
            "adapter":adapter.get_info().name,"frames":receipt,
        }))?)?;
        act_manifest::select_runtime_manifest(None)?;
        Ok(())
    }

    #[test]
    fn weather_preserves_authored_start_and_moves_snow_fog_wind_together() {
        let first = sample(0.0, 1942, 0.66, 0.0018);
        assert_eq!(first.storm, 0.66);
        assert_eq!(first.fog_density, 0.0018);
        assert_eq!(first.wind_scale, 1.0);
        let frames: Vec<_> = (0..600).map(|t| sample(t as f32, 1942, 0.66, 0.0018)).collect();
        let low = frames.iter().min_by(|a,b| a.storm.total_cmp(&b.storm)).unwrap();
        let high = frames.iter().max_by(|a,b| a.storm.total_cmp(&b.storm)).unwrap();
        assert!(low.storm < 0.25 && high.storm > 0.89);
        assert!(high.fog_density > low.fog_density * 3.0);
        assert!(high.wind_scale > low.wind_scale * 2.0);
        for pair in frames.windows(2) { assert!((pair[1].storm - pair[0].storm).abs() < 0.025); }
    }

    #[test]
    fn weather_is_deterministic_across_frame_rates_and_finite_at_boundaries() {
        for seed in [0, 1942, u32::MAX] {
            for t in [0.0, 10.0, 89.0, 210.0, 3500.0, f32::NAN] {
                let frame = sample(t, seed, 0.66, 0.0018);
                assert!(frame.storm.is_finite() && (0.0..=1.0).contains(&frame.storm));
                assert!(frame.wind_scale.is_finite() && frame.wind_scale > 0.0);
                assert!(frame.fog_density.is_finite());
            }
        }
        assert_eq!(sample(42.0, 1, 0.66, 0.0018).storm, sample(42.0, 1, 0.66, 0.0018).storm);
    }
}
