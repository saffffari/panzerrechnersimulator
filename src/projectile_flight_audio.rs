//! Cannon wake audio and read-only capture telemetry from authoritative swept flight segments.
//! The whistle is an authored source timbre; its existence, duration, motion and terminal stop
//! follow the projectile solver. Reports, the physical N-wave, and impacts keep separate voices.

use super::{AuthoritativeProjectile, Game, ProjectileStep, Renderer, Vec3};
use crate::shell_flight_audio::{Params, Voice};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct FlightSegment {
    pub id: u64,
    /// Absolute simulation emission time on the renderer clock, before propagation delay.
    pub at_s: f32,
    pub age_s: f32,
    pub from: Vec3,
    pub to: Vec3,
    pub velocity: Vec3,
    pub duration_s: f32,
    pub caliber_mm: f32,
    pub mass_kg: f32,
    pub explosive_fraction: f32,
    pub seed: u32,
    pub first: bool,
    pub last: bool,
}

impl FlightSegment {
    fn from_step(
        projectile: &AuthoritativeProjectile,
        step: ProjectileStep,
        end: Vec3,
        step_dt: f32,
        at_s: f32,
    ) -> Option<Self> {
        if projectile.small_arm.is_some() || projectile.shell.caliber_mm < 20.0 {
            return None;
        }
        let whole = step.start.distance(step.end);
        let fraction = if whole > 1e-6 {
            (step.start.distance(end) / whole).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let duration = step_dt * fraction;
        if !duration.is_finite() || duration < 0.0 {
            return None;
        }
        Some(FlightSegment {
            id: projectile.id,
            at_s,
            age_s: projectile.age_s,
            from: step.start,
            to: end,
            velocity: projectile.velocity.lerp(step.velocity, fraction * 0.5),
            duration_s: duration,
            caliber_mm: projectile.shell.caliber_mm,
            mass_kg: projectile.shell.mass_kg,
            explosive_fraction: projectile.shell.filler_g * 0.001
                / projectile.shell.mass_kg.max(0.01),
            seed: projectile.seed,
            first: projectile.age_s <= 1e-6,
            last: false,
        })
    }

    /// Snapshot a listener only after actual geometry has clipped the swept flight segment.
    pub fn params(&self, listener: Vec3, forward: Vec3) -> Params {
        Params {
            caliber_mm: self.caliber_mm,
            mass_kg: self.mass_kg,
            speed_mps: self.velocity.length(),
            explosive_fraction: self.explosive_fraction,
            duration_s: self.duration_s,
            from_bearing_deg: super::bearing_deg_from(forward, self.from - listener),
            to_bearing_deg: super::bearing_deg_from(forward, self.to - listener),
            radial_velocity_start_mps: self
                .velocity
                .dot((self.from - listener).normalize_or_zero()),
            radial_velocity_end_mps: self.velocity.dot((self.to - listener).normalize_or_zero()),
            seed: self.seed,
        }
    }
}

struct Stream {
    source: Voice,
    sink: Option<rodio::Sink>,
}

#[derive(Default)]
pub(super) struct Runtime {
    pub(super) segments: Vec<FlightSegment>,
    streams: HashMap<u64, Stream>,
    /// Finished shells still have emitted sound in transit. Keep controls until finite playback
    /// ends so pause, reset and Game drop affect them just as they affect active sources.
    tails: Vec<rodio::Sink>,
}

impl Runtime {
    pub(super) fn set_paused(&self, paused: bool) {
        for sink in self
            .streams
            .values()
            .filter_map(|stream| stream.sink.as_ref())
            .chain(self.tails.iter())
        {
            if paused {
                sink.pause();
            } else {
                sink.play();
            }
        }
    }

    fn finish_source(&mut self, id: u64) {
        if let Some(finished) = self.streams.remove(&id) {
            if let Some(sink) = finished.sink {
                self.tails.push(sink);
            }
        }
    }
}

impl Game {
    /// Swept cannon flight since the beginning of the most recent `update`. This remains present
    /// with audio disabled, so a capture can listen from its own real observer position.
    pub fn shell_flight_segments(&self) -> &[FlightSegment] {
        &self.projectile_flight_audio.segments
    }

    pub(super) fn record_shell_flight_segment(
        &mut self,
        projectile: &AuthoritativeProjectile,
        step: ProjectileStep,
        end: Vec3,
        step_dt: f32,
        at_s: f32,
    ) {
        if let Some(segment) = FlightSegment::from_step(projectile, step, end, step_dt, at_s) {
            self.projectile_flight_audio.segments.push(segment);
        }
    }

    pub(super) fn finish_shell_flight_step(
        &mut self,
        from: usize,
        active: &[AuthoritativeProjectile],
    ) {
        for segment in &mut self.projectile_flight_audio.segments[from..] {
            segment.last = !active.iter().any(|projectile| projectile.id == segment.id);
        }
    }

    pub(super) fn dispatch_shell_flight_audio(&mut self, renderer: &Renderer) {
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let listener = renderer.camera.pos;
        let forward = renderer.camera.forward();
        let enclosure = self.listener_enclosure();
        let headset = self.listener_headset();
        // Compute LOS before mutably borrowing the runtime. No visible tracer pool, target
        // assumption, hidden trajectory extension or audibility trigger creates these sources.
        let paths: Vec<_> = self
            .projectile_flight_audio
            .segments
            .iter()
            .map(|segment| {
                let center = (segment.from + segment.to) * 0.5;
                crate::sound_propagation::Path {
                    distance_m: center.distance(listener),
                    atmosphere: crate::sound_propagation::active_atmosphere(),
                    occluded: !self.los_clear(center, listener),
                    enclosure,
                    headset,
                }
            })
            .collect();
        let runtime = &mut self.projectile_flight_audio;
        runtime.tails.retain(|sink| !sink.empty());
        for (index, path) in paths.into_iter().enumerate() {
            let segment = runtime.segments[index];
            let stream = runtime.streams.entry(segment.id).or_insert_with(|| {
                let sink = audio.create_scene_sink().ok();
                if let Some(sink) = &sink {
                    sink.set_volume(audio.scene_master_gain());
                    // One transport queue per shell: begin after its first segment's range/c
                    // delay. Later blocks remain contiguous. This is explicitly a fixed-delay
                    // approximation; the conical ballistic crack has its separate exact gate.
                    let delay = (segment.from.distance(listener)
                        / crate::sound_propagation::speed_of_sound()
                        - (renderer.time - segment.at_s).max(0.0))
                    .max(0.0);
                    let frames = (delay * crate::acoustics::SAMPLE_RATE as f32).round() as usize;
                    if frames > 0 {
                        sink.append(rodio::buffer::SamplesBuffer::new(
                            2,
                            crate::acoustics::SAMPLE_RATE,
                            vec![0.0f32; frames * 2],
                        ));
                    }
                }
                Stream {
                    source: Voice::new(segment.seed),
                    sink,
                }
            });
            let pcm = stream.source.render_propagated_segment(
                &segment.params(listener, forward),
                &path,
                segment.first,
                segment.last,
            );
            if let Some(sink) = &stream.sink {
                if !pcm.is_empty() {
                    sink.append(rodio::buffer::SamplesBuffer::new(
                        2,
                        crate::acoustics::SAMPLE_RATE,
                        pcm,
                    ));
                }
            }
            if segment.last {
                // Keep the already-emitted audio through travel delay, under pause/reset control.
                runtime.finish_source(segment.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_flight_keeps_pause_control_and_reset_stops_its_finite_tail() {
        let audio = crate::audio::Audio::new_capture();
        let sink = audio.create_scene_sink().unwrap();
        sink.append(rodio::buffer::SamplesBuffer::new(
            2,
            crate::acoustics::SAMPLE_RATE,
            vec![0.1f32; crate::acoustics::SAMPLE_RATE as usize],
        ));
        let mut runtime = Runtime::default();
        runtime.streams.insert(
            1,
            Stream {
                source: Voice::new(1),
                sink: Some(sink),
            },
        );
        runtime.finish_source(1);
        assert!(runtime.streams.is_empty());
        assert_eq!(runtime.tails.len(), 1);
        runtime.set_paused(true);
        assert!(runtime.tails[0].is_paused());
        let paused = audio.render_capture_frames(4410).unwrap();
        assert!(paused[2000..].iter().all(|sample| sample.abs() < 1e-6));
        runtime.set_paused(false);
        let playing = audio.render_capture_frames(4410).unwrap();
        assert!(playing.iter().any(|sample| sample.abs() > 0.01));
        drop(runtime); // The same ownership drop used by a Game reset/retry.
        let stopped = audio.render_capture_frames(4410).unwrap();
        assert!(stopped[4000..].iter().all(|sample| sample.abs() < 1e-6));
    }

    #[test]
    fn terminal_geometry_clips_audio_duration_and_keeps_a_real_shells_parameters() {
        let shell = &crate::ballistics::PZGR_39;
        let projectile = AuthoritativeProjectile::new(
            81,
            super::super::ProjectileShooter::Player,
            shell,
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::Z,
            777,
        );
        let step = projectile.swept_step(super::super::PROJECTILE_FIXED_DT);
        let impact = step.start.lerp(step.end, 0.35);
        let segment = FlightSegment::from_step(
            &projectile,
            step,
            impact,
            super::super::PROJECTILE_FIXED_DT,
            3.0,
        )
        .unwrap();
        assert_eq!(segment.id, 81);
        assert_eq!(segment.to, impact);
        assert_eq!(segment.caliber_mm, shell.caliber_mm);
        assert_eq!(segment.mass_kg, shell.mass_kg);
        assert!(
            (segment.duration_s - super::super::PROJECTILE_FIXED_DT * 0.35).abs() < 1e-7,
            "a shell terminated inside a physics step must not whistle for the unused step"
        );
        let touch = FlightSegment::from_step(
            &projectile,
            step,
            step.start,
            super::super::PROJECTILE_FIXED_DT,
            3.0,
        )
        .unwrap();
        assert_eq!(
            touch.duration_s, 0.0,
            "zero-distance terminal event exists only to retire the source"
        );
    }

    #[test]
    fn listener_parameters_preserve_actual_flight_and_shell_characteristics() {
        let segment = FlightSegment {
            id: 9,
            at_s: 1.2,
            age_s: 0.1,
            from: Vec3::new(-4.0, 0.0, 10.0),
            to: Vec3::new(4.0, 0.0, 10.0),
            velocity: Vec3::new(800.0, 0.0, 0.0),
            duration_s: 0.01,
            caliber_mm: 88.0,
            mass_kg: 10.2,
            explosive_fraction: 0.059 / 10.2,
            seed: 42,
            first: false,
            last: true,
        };
        let params = segment.params(Vec3::ZERO, Vec3::Z);
        assert_eq!(params.duration_s, 0.01);
        assert_eq!(params.speed_mps, 800.0);
        assert_eq!(params.explosive_fraction, segment.explosive_fraction);
        assert!(params.radial_velocity_start_mps < 0.0);
        assert!(params.radial_velocity_end_mps > 0.0);
        assert!(params.from_bearing_deg.sin().is_finite());
        assert_ne!(params.from_bearing_deg, params.to_bearing_deg);
    }
}
