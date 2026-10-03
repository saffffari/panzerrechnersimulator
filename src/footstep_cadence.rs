//! Foot contact events follow accepted horizontal travel, never camera motion or elapsed time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Metal,
    Dirt,
    Grass,
    Snow,
}
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub variant: usize,
    pub gain: f32,
}
impl Step {
    pub fn key(self, surface: Surface) -> &'static str {
        let keys = match surface {
            Surface::Metal => ["footstep_metal_1", "footstep_metal_2", "footstep_metal_3"],
            Surface::Dirt => ["footstep_dirt_1", "footstep_dirt_2", "footstep_dirt_3"],
            Surface::Grass => ["footstep_grass_1", "footstep_grass_2", "footstep_grass_3"],
            Surface::Snow => ["footstep_snow_1", "footstep_snow_2", "footstep_snow_3"],
        };
        keys[self.variant % 3]
    }
}
#[derive(Default)]
pub struct Cadence {
    phase: f32,
    ordinal: usize,
    inside: Option<bool>,
}
impl Cadence {
    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.inside = None;
    }
    pub fn advance(
        &mut self,
        distance: f32,
        dt: f32,
        grounded: bool,
        wants_walk: bool,
        crouch: f32,
        inside: bool,
    ) -> Option<Step> {
        if !grounded
            || !wants_walk
            || !distance.is_finite()
            || !dt.is_finite()
            || dt <= 0.0
            || dt > 0.25
            || distance < 0.00001
            || distance > 6.5 * dt + 0.03
        {
            self.reset();
            return None;
        }
        if self.inside != Some(inside) {
            self.phase = 0.0;
            self.inside = Some(inside);
        }
        let speed = (distance / dt).clamp(0.0, 6.5);
        let crouch = if crouch.is_finite() {
            crouch.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let normal_stride = 0.55 + 0.19 * speed;
        let quiet_stride = 0.52 + 0.10 * speed;
        let stride = normal_stride + (quiet_stride - normal_stride) * crouch;
        self.phase += distance / stride;
        if self.phase + 1e-6 < 1.0 {
            return None;
        }
        // A stalled frame may emit one contact, never a backlog of overlapping steps.
        self.phase = (self.phase - 1.0).max(0.0).fract();
        let variant = [0, 2, 1][self.ordinal % 3];
        self.ordinal = self.ordinal.wrapping_add(1);
        Some(Step {
            variant,
            gain: (0.34 + 0.16 * (speed / 5.5).min(1.0)) * (1.0 - 0.68 * crouch),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cadence_tracks_distance_at_different_frame_rates() {
        let run = |fps: usize| {
            let mut cadence = Cadence::default();
            let mut steps = Vec::new();
            for _ in 0..fps * 10 {
                if let Some(step) =
                    cadence.advance(2.0 / fps as f32, 1.0 / fps as f32, true, true, 0.0, false)
                {
                    steps.push(step.variant);
                }
            }
            steps
        };
        assert_eq!(run(30), run(144));
        assert!((20..23).contains(&run(60).len()));
    }
    #[test]
    fn stationary_blocked_airborne_and_vehicle_motion_are_silent() {
        let mut cadence = Cadence::default();
        for _ in 0..180 {
            assert!(cadence
                .advance(0.0, 1.0 / 60.0, true, true, 0.0, false)
                .is_none());
            assert!(cadence
                .advance(0.1, 1.0 / 60.0, true, false, 0.0, true)
                .is_none());
            assert!(cadence
                .advance(0.05, 1.0 / 60.0, false, true, 0.0, false)
                .is_none());
            assert!(cadence
                .advance(5.0, 1.0 / 60.0, true, true, 0.0, false)
                .is_none());
        }
        assert!(cadence
            .advance(0.01, 1.0 / 60.0, true, true, 0.0, false)
            .is_none());
    }
    #[test]
    fn crouch_is_quieter_and_mode_changes_drop_old_stride_phase() {
        let step = |crouch: f32| {
            let mut c = Cadence::default();
            (0..120)
                .find_map(|_| c.advance(0.02, 1.0 / 60.0, true, true, crouch, true))
                .unwrap()
        };
        assert!(step(1.0).gain < step(0.0).gain * 0.4);
        let mut cadence = Cadence::default();
        for _ in 0..30 {
            cadence.advance(0.02, 1.0 / 60.0, true, true, 0.0, true);
        }
        assert!(cadence
            .advance(0.01, 1.0 / 60.0, true, true, 0.0, false)
            .is_none());
    }
    #[test]
    fn recorded_footstep_variants_load_and_render_in_the_real_offline_mixer() {
        let audio = crate::audio::Audio::new_capture();
        let metadata = audio.capture_metadata().unwrap();
        let loaded = metadata["audio_bank"]["feedback"]["loaded"]
            .as_array()
            .unwrap();
        for surface in [Surface::Metal, Surface::Dirt, Surface::Grass, Surface::Snow] {
            for variant in 0..3 {
                let key = Step {
                    variant,
                    gain: 0.45,
                }
                .key(surface);
                assert!(
                    loaded.iter().any(|row| row["key"] == key),
                    "missing verified pack recording: {key}"
                );
                audio.play_control(key, 0.45);
                let pcm = audio.render_capture_frames(44_100).unwrap();
                let peak = pcm.iter().copied().map(f32::abs).fold(0.0, f32::max);
                assert!(peak > 1.0e-5 && peak < 0.9, "{key}: peak {peak}");
                assert!(pcm.iter().all(|v| v.is_finite()));
                // Placed at its estimated physical level at the walker's ear, through the one
                // reproduction curve: a footstep is quiet (tens of dB below the tank's bed).
                let want = crate::reproduction::tone_map_dbfs(crate::audio::control_level_dba(key)
                    + crate::audio::effort_db(0.45))
                    + if surface==Surface::Metal {14.0}else{0.0};
                let got = crate::reproduction::laf_max_dbfs(&pcm);
                assert!((got - want).abs() < 0.5, "{key}: {got} vs {want} dBFS(A)");
            }
        }
    }
}
