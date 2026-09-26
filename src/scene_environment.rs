//! Authored world sources. Positions stay in world space; only the receiver moves.
//! These ordinary Audio sinks also enter the offline sample-clock capture graph. The sound
//! adapter models direct stereo attenuation and a bounded obstruction low-pass, not a measured
//! room impulse response. Renderer point sources use the same visibility as gameplay lights.

use super::*;
use crate::environment_authoring::{AuthoredLight, LightKind};
use crate::spatial_audio::{self, EmitterAction, EmitterRuntimeState, SoundEmitter};
use rodio::Source;
use std::collections::HashMap;
use std::io::BufReader;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[path = "scene_environment_readout.rs"]
mod readout;

const MAX_VOICES: usize = 16;
const MAX_CLIP_FRAMES: usize = 2_000_000;
const MAX_CACHE_FRAMES: usize = 4_000_000; // 16 MiB of decoded mono f32, independent of map size.

#[derive(Default)]
struct Parameters {
    left: AtomicU32,
    right: AtomicU32,
    lowpass: AtomicU32,
}

impl Parameters {
    fn set(&self, gains: [f32; 2], lowpass: f32) {
        self.left.store(gains[0].to_bits(), Ordering::Relaxed);
        self.right.store(gains[1].to_bits(), Ordering::Relaxed);
        self.lowpass.store(lowpass.to_bits(), Ordering::Relaxed);
    }
}

struct Clip {
    mono: Vec<f32>,
    rate: u32,
}

struct PositionedSource {
    clip: Arc<Clip>,
    parameters: Arc<Parameters>,
    frame: usize,
    end: usize,
    looping: bool,
    right: bool,
    filtered: f32,
    gains: [f32; 2],
}

impl Iterator for PositionedSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if !self.right {
            if self.frame >= self.end {
                if !self.looping {
                    return None;
                }
                self.frame = 0;
            }
            let target = [
                f32::from_bits(self.parameters.left.load(Ordering::Relaxed)),
                f32::from_bits(self.parameters.right.load(Ordering::Relaxed)),
            ];
            // Smooth receiver motion over ~5 ms; do not bake pan into the loop recording.
            let smooth = 1.0 - (-1.0 / (0.005 * self.clip.rate as f32)).exp();
            for (gain, target) in self.gains.iter_mut().zip(target) {
                *gain += (target - *gain) * smooth;
            }
            let alpha =
                f32::from_bits(self.parameters.lowpass.load(Ordering::Relaxed)).clamp(0.001, 1.0);
            let sample = self.clip.mono[self.frame];
            self.filtered += alpha * (sample - self.filtered);
            self.frame += 1;
            self.right = true;
            Some(self.filtered * self.gains[0])
        } else {
            self.right = false;
            Some(self.filtered * self.gains[1])
        }
    }
}

impl Source for PositionedSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        self.clip.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        if self.looping {
            None
        } else {
            Some(Duration::from_secs_f64(
                self.end as f64 / self.clip.rate as f64,
            ))
        }
    }
}

struct Voice {
    sink: rodio::Sink,
    parameters: Arc<Parameters>,
    rate: u32,
}

struct Emitter {
    source: SoundEmitter,
    trigger: EmitterRuntimeState,
    voice: Option<Voice>,
    failed: bool,
}

#[derive(Default)]
pub(super) struct SceneEnvironment {
    emitters: Vec<Emitter>,
    lights: Vec<AuthoredLight>,
    pub(super) lighting: crate::environment_authoring::LightingTuning,
    clips: HashMap<String, Arc<Clip>>,
}

fn receiver_gains(source: Vec3, listener: Vec3, right: Vec3, gain: f32) -> [f32; 2] {
    let pan = (source - listener)
        .normalize_or_zero()
        .dot(right.normalize_or_zero())
        .clamp(-1.0, 1.0);
    [
        gain * ((1.0 - pan) * 0.5).sqrt(),
        gain * ((1.0 + pan) * 0.5).sqrt(),
    ]
}

impl SceneEnvironment {
    pub(super) fn load(
        manifest: Option<&crate::act_manifest::ActManifest>,
    ) -> anyhow::Result<Self> {
        let Some(manifest) = manifest else {
            return Ok(Self::default());
        };
        let sources = manifest.sound_emitters()?;
        let lights = manifest.authored_lights()?;
        anyhow::ensure!(
            sources.len() <= 256 && lights.len() <= 256,
            "authored source budget exceeds 256"
        );
        anyhow::ensure!(
            lights.iter().all(|light| light.kind == LightKind::Point),
            "scene runtime supports point lights; spot/area need an implemented source model"
        );
        let emitters = sources
            .into_iter()
            .map(|source| Emitter {
                source,
                trigger: EmitterRuntimeState::default(),
                voice: None,
                failed: false,
            })
            .collect();
        Ok(Self {
            emitters,
            lights,
            lighting: crate::environment_authoring::LightingTuning::from_edits(manifest.edits())?,
            clips: HashMap::new(),
        })
    }

    fn clip(&mut self, asset: &str) -> anyhow::Result<Arc<Clip>> {
        if let Some(clip) = self.clips.get(asset) {
            return Ok(Arc::clone(clip));
        }
        let file = crate::resource::open(asset)?;
        let decoder = rodio::Decoder::new(BufReader::new(file))?;
        let channels = decoder.channels() as usize;
        let rate = decoder.sample_rate();
        anyhow::ensure!(
            channels > 0 && channels <= 8 && rate > 0,
            "unsupported authored audio layout"
        );
        let mut mono = Vec::new();
        let mut sum = 0.0;
        for (index, sample) in decoder.take((MAX_CLIP_FRAMES + 1) * channels).enumerate() {
            sum += sample as f32 / 32768.0;
            if index % channels == channels - 1 {
                mono.push(sum / channels as f32);
                sum = 0.0;
            }
        }
        anyhow::ensure!(
            !mono.is_empty() && mono.len() <= MAX_CLIP_FRAMES,
            "authored audio exceeds bounded clip capacity; author a shorter loop segment"
        );
        // Release inactive decoded clips before admitting a new source. Live Arc voices retain
        // their recording until they stop; no source is silently truncated to fit the budget.
        if self
            .clips
            .values()
            .map(|clip| clip.mono.len())
            .sum::<usize>()
            + mono.len()
            > MAX_CACHE_FRAMES
        {
            self.clips.retain(|_, clip| Arc::strong_count(clip) > 1);
        }
        anyhow::ensure!(
            self.clips
                .values()
                .map(|clip| clip.mono.len())
                .sum::<usize>()
                + mono.len()
                <= MAX_CACHE_FRAMES,
            "authored audio cache budget exhausted"
        );
        let clip = Arc::new(Clip { mono, rate });
        self.clips.insert(asset.to_owned(), Arc::clone(&clip));
        Ok(clip)
    }

    pub(super) fn clear_voices(&mut self) {
        for emitter in &mut self.emitters {
            emitter.voice = None; // Sink Drop stops the old live/capture graph.
            emitter.trigger = EmitterRuntimeState::default();
        }
    }

    pub(super) fn set_paused(&self, paused: bool) {
        for voice in self.emitters.iter().filter_map(|emitter| emitter.voice.as_ref()) {
            if paused { voice.sink.pause(); } else { voice.sink.play(); }
        }
    }

    pub(super) fn lights(&self, time: f32) -> impl Iterator<Item = PointLight> + '_ {
        self.lights
            .iter()
            .filter(|l| l.enabled && !l.deleted)
            .map(move |light| {
                let phase = light
                    .id
                    .bytes()
                    .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
                    as f32
                    * 0.001;
                PointLight {
                    pos: light.position,
                    source_radius: light.source_radius_m,
                    color: light.linear_rgb(),
                    radiant_intensity: light.radiant_intensity
                        * (1.0 - light.flicker * (0.5 + 0.5 * (time * 11.7 + phase).sin())),
                }
            })
    }

    fn update(
        &mut self,
        audio: Option<&crate::audio::Audio>,
        listener: Vec3,
        right: Vec3,
        enclosed: bool,
        blocked: impl Fn(Vec3) -> bool,
    ) {
        let Some(audio) = audio else {
            self.clear_voices();
            return;
        };
        let mut order: Vec<_> = (0..self.emitters.len()).collect();
        order.sort_by(|&a, &b| {
            self.emitters[a]
                .source
                .position
                .distance_squared(listener)
                .total_cmp(&self.emitters[b].source.position.distance_squared(listener))
        });
        let mut decoded_this_frame = false;
        let mut active = self
            .emitters
            .iter()
            .filter(|e| e.voice.as_ref().is_some_and(|v| !v.sink.empty()))
            .count();
        for index in order {
            if self.emitters[index].failed {
                continue;
            }
            let previous = self.emitters[index].trigger;
            let action = {
                let emitter = &mut self.emitters[index];
                spatial_audio::tick(&emitter.source, &mut emitter.trigger, listener)
            };
            if matches!(action, EmitterAction::Stop) {
                if self.emitters[index].voice.take().is_some() {
                    active = active.saturating_sub(1);
                }
                continue;
            }
            if let EmitterAction::Start { looping, .. } = action {
                let asset = self.emitters[index].source.asset.clone();
                if active >= MAX_VOICES || (decoded_this_frame && !self.clips.contains_key(&asset))
                {
                    self.emitters[index].trigger = previous; // Deferred admission can retry next frame.
                    continue;
                }
                decoded_this_frame |= !self.clips.contains_key(&asset);
                let result = self.clip(&asset).and_then(|clip| {
                    let parameters = Arc::new(Parameters::default());
                    parameters.set([0.0; 2], 1.0);
                    let duration = self.emitters[index].source.duration_s;
                    let end = if duration > 0.0 {
                        (duration * clip.rate as f32) as usize
                    } else {
                        clip.mono.len()
                    };
                    let end = end.clamp(1, clip.mono.len());
                    let rate = clip.rate;
                    let sink = audio.create_scene_sink()?;
                    sink.append(PositionedSource {
                        clip,
                        parameters: Arc::clone(&parameters),
                        frame: 0,
                        end,
                        looping,
                        right: false,
                        filtered: 0.0,
                        gains: [0.0; 2],
                    });
                    sink.play();
                    Ok(Voice {
                        sink,
                        parameters,
                        rate,
                    })
                });
                match result {
                    Ok(voice) => {
                        self.emitters[index].voice = Some(voice);
                        active += 1;
                    }
                    Err(error) => {
                        eprintln!("[scene audio] {asset}: {error}");
                        self.emitters[index].failed = true;
                    }
                }
            }
            let emitter = &mut self.emitters[index];
            if let Some(voice) = &emitter.voice {
                let obstructed = blocked(emitter.source.position);
                let obstruction =
                    if obstructed { 0.28 } else { 1.0 } * if enclosed { 0.35 } else { 1.0 };
                let gains = receiver_gains(
                    emitter.source.position,
                    listener,
                    right,
                    emitter.source.gain_at(listener) * audio.scene_master_gain() * obstruction,
                );
                let cutoff = if obstructed || enclosed {
                    1300.0
                } else {
                    18000.0
                };
                voice.parameters.set(
                    gains,
                    1.0 - (-std::f32::consts::TAU * cutoff / voice.rate as f32).exp(),
                );
            }
            if emitter.voice.as_ref().is_some_and(|v| v.sink.empty()) {
                emitter.voice = None;
            }
        }
    }
}

impl Game {
    pub(super) fn update_scene_environment(&mut self, r: &Renderer) {
        let mut scene = std::mem::take(&mut self.scene_environment);
        scene.update(
            self.audio.as_ref(),
            r.camera.pos,
            r.camera.right(),
            matches!(self.mode, Mode::Tank)
                && !((matches!(self.station, Station::Commander) && self.commander_outside)
                    || (matches!(self.station, Station::Driver) && self.driver_hatch)),
            |source| !self.los_clear(source, r.camera.pos),
        );
        self.scene_environment = scene;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_motion_changes_stereo_without_moving_world_source() {
        let source = Vec3::X * 10.0;
        let a = receiver_gains(source, Vec3::ZERO, Vec3::X, 1.0);
        let b = receiver_gains(source, Vec3::X * 20.0, Vec3::X, 1.0);
        assert_eq!(a, [0.0, 1.0]);
        assert_eq!(b, [1.0, 0.0]);
        let center = receiver_gains(Vec3::Z, Vec3::ZERO, Vec3::X, 1.0);
        assert!((center[0] * center[0] + center[1] * center[1] - 1.0).abs() < 1e-6);
    }
    #[test]
    fn source_loops_or_finishes_on_audio_sample_clock() {
        let clip = Arc::new(Clip {
            mono: vec![0.25, 0.5],
            rate: 48000,
        });
        let parameters = Arc::new(Parameters::default());
        parameters.set([1.0; 2], 1.0);
        let mut source = PositionedSource {
            clip,
            parameters,
            frame: 0,
            end: 2,
            looping: false,
            right: false,
            filtered: 0.0,
            gains: [1.0; 2],
        };
        assert_eq!(
            source.by_ref().collect::<Vec<_>>(),
            vec![0.25, 0.25, 0.5, 0.5]
        );
        source.looping = true;
        assert_eq!(
            source.take(4).collect::<Vec<_>>(),
            vec![0.25, 0.25, 0.5, 0.5]
        );
    }
}
