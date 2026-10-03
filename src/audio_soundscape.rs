//! Adaptive sample-clock transport for owned ambience, scene beds and score.
//! The game chooses intent, a bounded worker decodes imports, and ordinary Audio sinks
//! feed the shared ear/master graph. No decoding or filesystem access on the audio callback.
use super::{Audio, EarBus};
use crate::audio_soundscape_fx::{Rack, RackConfig};
use crate::soundscape_program::{ScoreProgram, SolarEdges};
use crate::soundscape_riser::{Directive as RiserDirective, RiserPlanner};
use crate::{
    sound_library::{self as library, Cue, Library, Pcm, RATE},
    soundscape_director::{Context, Director, Plan},
};
use rodio::{Sink, Source};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug)]
pub struct LibraryEmitter { pub gain: f32, pub bearing_deg: f32, pub occlusion: f32 }

/// Listener facts supplied by the actual game, not inferred from audio filenames.
pub struct SoundscapeInput {
    pub context: Context,
    pub act: u8,
    pub position: [f32; 2],
    pub shelter: f32,
    pub storm: f32,
    /// Shared local optical whiteout severity, independent of the listener audio enclosure.
    pub whiteout: f32,
    pub engine_hiss: Option<LibraryEmitter>,
    pub inside: bool,
    pub solar_altitude_deg: f32,
    pub season: &'static str,
}
const MAX_VOICES: usize = 12;
const CACHE_BYTES: usize = 256 * 1024 * 1024;
const MAX_REQUESTS: usize = 4;
fn put(a: &AtomicU32, x: f32) {
    a.store(x.to_bits(), Ordering::Relaxed)
}
fn get(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

struct Control {
    gain: AtomicU32,
    master: AtomicU32,
    occlusion: AtomicU32,
    bearing: AtomicU32,
    fade_s: AtomicU32,
    retire: AtomicBool,
    retire_at: AtomicU64,
    frames: AtomicU64,
    finished: AtomicBool,
    peak_at: AtomicU64,
}
impl Control {
    fn new(gain: f32, fade: f32) -> Self {
        Self {
            gain: AtomicU32::new(gain.to_bits()),
            master: AtomicU32::new(1.0f32.to_bits()),
            occlusion: AtomicU32::new(0),
            bearing: AtomicU32::new(0),
            fade_s: AtomicU32::new(fade.to_bits()),
            retire: AtomicBool::new(false),
            retire_at: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            peak_at: AtomicU64::new(u64::MAX),
        }
    }
    fn fade_out(&self, seconds: f32) {
        self.fade_out_after(seconds, 0);
    }
    fn fade_out_after(&self, seconds: f32, delay: u64) {
        put(&self.fade_s, seconds.max(0.02));
        self.retire_at.store(
            self.frames.load(Ordering::Relaxed) + delay,
            Ordering::Relaxed,
        );
        self.retire.store(true, Ordering::Relaxed);
    }
}

struct BedSource {
    pcm: Pcm,
    control: Arc<Control>,
    cursor: usize,
    channel: usize,
    frame: [f32; 2],
    seam: usize,
    equal_gain_seam: bool,
    natural_fade_frames: usize,
    looping: bool,
    gain: f32,
    target: f32,
    step: f32,
    remaining: u64,
    elapsed: u64,
    rack: Rack,
    end_tail: u32,
    tail_frames: u32,
    birth_fade: f32,
    master_gain: f32,
    tail_only_frames: usize,
    spatial: bool,
    lateral: f32,
    aligned_peak: Option<AlignedPeak>,
}
/// Normal-rate excerpt with short crossfaded phase corrections. This never stretches
/// pitch, delays a projectile, or generates a physical collision sound.
struct AlignedPeak {
    marker: usize,
    lead: u64,
    applied_at: u64,
    previous_cursor: Option<(usize, usize)>,
    started: bool,
}
impl BedSource {
    fn new(pcm: Pcm, cue: &Cue, control: Arc<Control>, rack: RackConfig) -> Self {
        // Rhythmic loops keep their exact original duration; no beat-shortening overlap.
        let seam = if cue.bpm.is_some() {
            0
        } else {
            ((cue.crossfade_s * RATE as f32) as usize).min(pcm.frames() / 3)
        };
        let birth_fade = get(&control.fade_s);
        let tail_frames = if rack.wet > 0.0 {
            ((rack.decay_s.clamp(0.05, 10.0) * 1.5 + 0.25) * RATE as f32) as u32
        } else {
            128
        };
        Self {
            pcm,
            control,
            cursor: 0,
            channel: 0,
            frame: [0.0; 2],
            seam,
            equal_gain_seam: cue.equal_gain_seam,
            natural_fade_frames: (cue.fade_out_s.unwrap_or(0.005) * RATE as f32) as usize,
            looping: cue.looping,
            gain: 0.0,
            target: -1.0,
            step: 0.0,
            remaining: 0,
            elapsed: 0,
            rack: Rack::new(rack),
            end_tail: 0,
            tail_frames,
            birth_fade,
            master_gain: 1.0,
            tail_only_frames: 0,
            spatial: false,
            lateral: 0.0,
            aligned_peak: None,
        }
    }
    fn spatial(mut self, enabled: bool) -> Self { self.spatial = enabled; self }
    fn with_aligned_peak(mut self, marker_s: f32, peak_in_s: f32, lead_s: f32) -> Self {
        let marker = (marker_s * RATE as f32).round() as usize;
        let peak_at = (peak_in_s.max(0.0) * RATE as f32).round() as u64;
        let lead = (lead_s.max(0.0) * RATE as f32).round() as u64;
        self.control.peak_at.store(peak_at, Ordering::Relaxed);
        self.cursor = marker.saturating_sub(lead.min(peak_at) as usize);
        self.aligned_peak = Some(AlignedPeak {marker, lead, applied_at:peak_at,
            previous_cursor:None, started:false});
        self
    }
    fn with_tail_reverb(mut self, seconds: f32) -> Self {
        if !self.looping && seconds.is_finite() {
            self.tail_only_frames = (seconds.clamp(0.0, 30.0) * RATE as f32) as usize;
        }
        self
    }
    fn render_frame(&mut self) -> Option<[f32; 2]> {
        let retiring = self.control.retire.load(Ordering::Relaxed)
            && self.elapsed >= self.control.retire_at.load(Ordering::Relaxed);
        if let Some(alignment) = &mut self.aligned_peak {
            let requested = self.control.peak_at.load(Ordering::Relaxed);
            let remaining = requested.saturating_sub(self.elapsed);
            if !alignment.started && remaining > alignment.lead && !retiring {
                self.elapsed += 1;
                self.control.frames.store(self.elapsed, Ordering::Relaxed);
                return Some([0.0; 2]);
            }
            let changed = requested != alignment.applied_at; // planner already debounces forecasts
            // Ignore late corrections after the peak has already decayed; replaying it
            // would turn one shell into two dramatic accents.
            if !alignment.started || (changed && self.cursor < alignment.marker) {
                let desired = alignment.marker.saturating_sub(remaining as usize)
                    + self.elapsed.saturating_sub(requested) as usize;
                if alignment.started {
                    alignment.previous_cursor = Some((self.cursor, 0));
                }
                self.cursor = desired.min(self.pcm.frames());
                alignment.applied_at = requested;
                alignment.started = true;
            }
        }
        if self.elapsed % 32 == 0 {
            let target = if retiring {
                0.0
            } else {
                get(&self.control.gain).clamp(0.0, 4.0)
            };
            if target.to_bits() != self.target.to_bits() {
                let first_target = self.target < 0.0;
                self.target = target;
                let fade = if self.elapsed == 0 || first_target {
                    self.birth_fade
                } else {
                    get(&self.control.fade_s)
                };
                self.remaining = (fade.clamp(1.0 / RATE as f32, 30.0) * RATE as f32) as u64;
                self.step = (target - self.gain) / self.remaining.max(1) as f32;
            }
            self.rack.set_occlusion(get(&self.control.occlusion));
        }
        if self.remaining > 0 {
            self.gain += self.step;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.gain = self.target;
            }
        }
        if retiring && self.remaining == 0 && self.gain <= 0.000001 {
            self.control.finished.store(true, Ordering::Relaxed);
            return None;
        }
        let n = self.pcm.frames();
        let mut frame = [0.0; 2];
        if self.cursor < n {
            for (c, x) in frame.iter_mut().enumerate() {
                *x = self.pcm.samples[self.cursor * 2 + c];
            }
            if let Some(alignment) = &mut self.aligned_peak {
                if let Some((old, step)) = alignment.previous_cursor {
                    let duration = RATE as usize / 100; // 10ms, no pitch shift.
                    let t = (step + 1) as f32 / duration as f32;
                    for ch in 0..2 {
                        let previous = self.pcm.samples.get(old * 2 + ch).copied().unwrap_or(0.0);
                        frame[ch] = previous * (1.0 - t) + frame[ch] * t;
                    }
                    alignment.previous_cursor = if step + 1 < duration {
                        Some((old + 1, step + 1))
                    } else { None };
                }
            }
            if self.looping && self.seam > 0 && self.cursor >= n - self.seam {
                let head = self.cursor - (n - self.seam);
                let t = head as f32 / self.seam as f32;
                let (s, c) = if self.equal_gain_seam {
                    (t, 1.0 - t)
                } else {
                    (t * std::f32::consts::FRAC_PI_2).sin_cos()
                };
                for ch in 0..2 {
                    frame[ch] = frame[ch] * c + self.pcm.samples[head * 2 + ch] * s;
                }
            } else if self.looping && self.seam == 0 {
                // A 2ms edge envelope avoids discontinuities without changing bar length.
                let edge = 88usize.min(n / 4).max(1);
                let gain = (self.cursor.min(n - 1 - self.cursor) as f32 / edge as f32).min(1.0);
                frame[0] *= gain;
                frame[1] *= gain;
            }
            if !self.looping && self.natural_fade_frames > 0 {
                let tail_gain =
                    ((n - 1 - self.cursor) as f32 / self.natural_fade_frames as f32).min(1.0);
                frame[0] *= tail_gain;
                frame[1] *= tail_gain;
            }
            self.cursor += 1;
            if self.looping && self.cursor >= n {
                self.cursor = self.seam;
            }
        } else {
            self.end_tail += 1;
            if self.end_tail > self.tail_frames {
                self.control.finished.store(true, Ordering::Relaxed);
                return None;
            }
        }
        let processed = self.rack.process(frame);
        let blend = if self.tail_only_frames == 0 { 1.0 } else {
            let t = 1.0 - n.saturating_sub(self.cursor) as f32 / self.tail_only_frames as f32;
            let t = t.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let mut frame = [frame[0] + (processed[0] - frame[0]) * blend,
            frame[1] + (processed[1] - frame[1]) * blend];
        if self.spatial {
            let bearing = get(&self.control.bearing);
            let target = if bearing.is_finite() { bearing.to_radians().sin() } else { 0.0 };
            self.lateral += (target - self.lateral) * 0.004524;
            let angle = (self.lateral + 1.0) * std::f32::consts::FRAC_PI_4;
            let mono = (frame[0] + frame[1]) * 0.5;
            frame = [mono * angle.cos(), mono * angle.sin()];
        }
        self.elapsed += 1;
        self.control.frames.store(self.elapsed, Ordering::Relaxed);
        self.master_gain +=
            (get(&self.control.master).clamp(0.0, 4.0) - self.master_gain) * 0.004524;
        Some([
            frame[0] * self.gain * self.master_gain,
            frame[1] * self.gain * self.master_gain,
        ])
    }
}
impl Iterator for BedSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.channel == 0 {
            self.frame = self.render_frame()?;
        }
        let out = self.frame[self.channel];
        self.channel ^= 1;
        Some(out)
    }
}
impl Source for BedSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

struct Voice {
    id: String,
    lane: String,
    sink: Sink,
    control: Arc<Control>,
    gain: f32,
    music: bool,
    retiring: bool,
    looping: bool,
}
struct Loader {
    tx: mpsc::SyncSender<Cue>,
    rx: mpsc::Receiver<(String, Result<Pcm, String>)>,
}
impl Loader {
    fn new() -> Self {
        let (tx, requests) = mpsc::sync_channel::<Cue>(MAX_REQUESTS);
        let (results, rx) = mpsc::sync_channel(2);
        std::thread::Builder::new()
            .name("sound-library-decode".into())
            .spawn(move || {
                while let Ok(cue) = requests.recv() {
                    let result = library::decode(&cue.path).map_err(|e| format!("{e:#}"));
                    if results.send((cue.id, result)).is_err() {
                        break;
                    }
                }
            })
            .expect("start sound library loader");
        Self { tx, rx }
    }
}

struct DeathScore { voice: Voice, duration: f32 }
#[derive(Clone, Copy)]
struct RiserCue { peak_s: f32, lead_s: f32 }

pub(super) struct Runtime {
    library: Library,
    director: Director,
    loader: Loader,
    cache: HashMap<String, Pcm>,
    lru: VecDeque<String>,
    requested: HashSet<String>,
    failed: HashSet<String>,
    voices: Vec<Voice>,
    selections: HashMap<String, (String, Option<String>)>,
    cooldowns: HashMap<String, f32>,
    time: f32,
    paused: bool,
    plan: Option<Plan>,
    events: VecDeque<String>,
    event_serial: u64,
    pending_events: HashMap<String, String>,
    scene_rack: Option<RackConfig>,
    racks: HashMap<String, (RackConfig, f32)>,
    program: ScoreProgram,
    solar: SolarEdges,
    score_selector: Director,
    offline: bool,
    intro_done: bool,
    combat_accent_played: bool,
    blizzard_active: bool,
    blizzard_episode: u64,
    engine_failed: bool,
    engine_episode: u64,
    gun_successor: Option<String>,
    forced_music: Option<(u64, String)>,
    instant_music: bool,
    last_tags: String,
    death_attempted: bool,
    death: Option<DeathScore>,
    completed: HashMap<String, String>,
    riser: RiserPlanner,
    riser_cues: HashMap<String, RiserCue>,
    riser_history: VecDeque<Value>,
}
pub fn fresh_seed() -> u64 {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    std::env::var("PANZER_SOUND_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
                ^ SEQUENCE
                    .fetch_add(1, Ordering::Relaxed)
                    .wrapping_mul(0x9e3779b97f4a7c15)
        })
}
impl Runtime {
    pub fn new(seed: u64) -> Self {
        let library = Library::load().unwrap_or_else(|e| {
            eprintln!("[soundscape] library unavailable: {e:#}");
            Library::default()
        });
        for error in &library.errors {
            eprintln!("[soundscape] {error}");
        }
        let mut racks = HashMap::new();
        let mut riser_cues = HashMap::new();
        let root = std::env::var_os("PANZER_SOUND_LIBRARY")
            .map(std::path::PathBuf::from)
            .or_else(|| crate::resource::resolve("assets/audio/library").ok());
        if let Some(config) = root
            .and_then(|r| std::fs::read(r.join("soundscape.json")).ok())
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        {
            for c in config["cues"].as_array().into_iter().flatten() {
                if let Some(id) = c["asset"].as_str() {
                    if let Some(peak) = c["riser_peak_s"].as_f64().filter(|x| x.is_finite() && *x > 0.0) {
                        let lead = c["riser_lead_s"].as_f64().filter(|x|x.is_finite()).unwrap_or(4.0);
                        riser_cues.insert(id.to_owned(), RiserCue {
                            peak_s: peak as f32, lead_s: (lead as f32).clamp(1.5,4.0).min(peak as f32) });
                    }
                    let v = &c["rack"];
                    let mut rack = RackConfig::preset(v["preset"].as_str().unwrap_or("dry"));
                    let field = |key: &str, base: f32| {
                        v[key]
                            .as_f64()
                            .filter(|x| x.is_finite())
                            .map_or(base, |x| x as f32)
                    };
                    rack.wet = field("wet", rack.wet);
                    rack.decay_s = field("decay_s", rack.decay_s);
                    rack.predelay_ms = field("predelay_ms", rack.predelay_ms);
                    rack.damping_hz = field("damping_hz", rack.damping_hz);
                    rack.lowpass_hz = field("lowpass_hz", rack.lowpass_hz);
                    rack.highpass_hz = field("highpass_hz", rack.highpass_hz);
                    racks.insert(id.into(), (rack, field("tail_only_s", 0.0).clamp(0.0, 30.0)));
                }
            }
        }
        eprintln!(
            "[soundscape] {} enabled cues, {} scene zones; run seed {seed}",
            library.cues.len(),
            library.zones.len()
        );
        Self {
            library,
            director: Director::new(seed),
            loader: Loader::new(),
            cache: HashMap::new(),
            lru: VecDeque::new(),
            requested: HashSet::new(),
            failed: HashSet::new(),
            voices: Vec::new(),
            selections: HashMap::new(),
            cooldowns: HashMap::new(),
            time: 0.0,
            paused: false,
            plan: None,
            events: VecDeque::new(),
            event_serial: 0,
            pending_events: HashMap::new(),
            scene_rack: None,
            racks,
            program: ScoreProgram::new(seed ^ 0x53434f5245),
            solar: SolarEdges::default(),
            score_selector: Director::new(seed ^ 0x5049434b),
            offline: false,
            intro_done: false,
            combat_accent_played: false,
            blizzard_active: false,
            blizzard_episode: 0,
            engine_failed: false,
            engine_episode: 0,
            gun_successor: None,
            forced_music: None,
            instant_music: false,
            last_tags: String::new(),
            death_attempted: false,
            death: None,
            completed: HashMap::new(),
            riser: RiserPlanner::new(seed ^ 0x5249534552),
            riser_cues,
            riser_history: VecDeque::new(),
        }
    }
    fn insert(&mut self, id: String, pcm: Pcm) {
        self.lru.retain(|x| x != &id);
        self.lru.push_back(id.clone());
        self.cache.insert(id, pcm);
        while self
            .cache
            .values()
            .map(|p| p.samples.len() * 4)
            .sum::<usize>()
            > CACHE_BYTES
        {
            let Some(old) = self.lru.pop_front() else {
                break;
            };
            self.cache.remove(&old);
        }
    }
    fn poll(&mut self) {
        while let Ok((id, result)) = self.loader.rx.try_recv() {
            self.requested.remove(&id);
            match result {
                Ok(pcm) => self.insert(id, pcm),
                Err(e) => {
                    eprintln!("[soundscape] {id}: {e}");
                    self.failed.insert(id);
                }
            }
        }
        for v in &self.voices {
            if !v.looping
                && !v.retiring
                && v.control.finished.load(Ordering::Relaxed)
                && !v.lane.starts_with("event:")
                && !v.lane.starts_with("riser:")
            {
                self.completed.insert(v.lane.clone(), v.id.clone());
            }
        }
        self.voices
            .retain(|v| !v.control.finished.load(Ordering::Relaxed) && !v.sink.empty());
    }
    fn request(&mut self, id: &str) {
        if self.cache.contains_key(id) || self.failed.contains(id) { return; }
        if self.offline {
            self.decode_now(id);
            return;
        }
        if self.requested.contains(id) || self.requested.len() >= MAX_REQUESTS { return; }
        if let Some(cue) = self.library.cues.get(id) {
            if self.loader.tx.try_send(cue.clone()).is_ok() { self.requested.insert(id.into()); }
        }
    }
    // Caller-thread fallback also guarantees a fatal cue can begin immediately if it was evicted.
    // This never executes on the output callback; ordinary gameplay uses the bounded worker.
    fn decode_now(&mut self, id: &str) {
        if self.cache.contains_key(id) { return; }
        if let Some(cue) = self.library.cues.get(id) {
            match library::decode(&cue.path) {
                Ok(pcm) => self.insert(id.into(), pcm),
                Err(e) => { eprintln!("[soundscape] {id}: {e:#}"); self.failed.insert(id.into()); }
            }
        }
    }
    pub fn prepare_review(&mut self) { self.offline = true; }
    fn select(&mut self, lane: &str, key: &str, role: &str, tag: &str) -> Option<String> {
        let ids = self.library.pool(role, tag);
        let key = if lane == "night" {
            key.to_owned()
        } else {
            format!("{key}:{}", ids.join(","))
        };
        if let Some((previous, id)) = self.selections.get(lane) {
            if previous == &key {
                return id.clone();
            }
        }
        self.completed.remove(lane);
        let id = if lane.starts_with("music") || lane == "intro" {
            self.score_selector.choose(role, &ids)
        } else { self.director.choose(role, &ids) };
        self.selections.insert(lane.into(), (key, id.clone()));
        id
    }
    fn lane(
        &mut self,
        audio: &Audio,
        lane: &str,
        id: Option<String>,
        volume: f32,
        occlusion: f32,
        fade: f32,
    ) -> bool {
        let active = self
            .voices
            .iter()
            .position(|v| v.lane == lane && !v.retiring);
        if let Some(i) = active {
            if id.as_deref() == Some(&self.voices[i].id) {
                let v = &self.voices[i];
                put(&v.control.fade_s, 0.3);
                put(&v.control.gain, volume * v.gain);
                put(&v.control.master, audio.master_volume);
                put(&v.control.occlusion, if v.music { 0.0 } else { occlusion });
                return true;
            }
        }
        let Some(id) = id else {
            if let Some(i) = active {
                let v = &mut self.voices[i];
                v.retiring = true;
                v.control.fade_out(fade);
            }
            return true;
        };
        if self.completed.get(lane) == Some(&id) {
            return true;
        }
        self.request(&id);
        if self.voices.len() >= MAX_VOICES {
            return false;
        }
        let Some(pcm) = self.cache.get(&id).cloned() else {
            return false;
        };
        let Some(cue) = self.library.cues.get(&id) else {
            return false;
        };
        let Ok(sink) = audio.create_sink_on(if cue.music() {
            EarBus::Score
        } else {
            EarBus::World
        }) else {
            return false;
        };
        // Admission succeeded. Only now retire the outgoing bed, keeping it audible through load.
        let mut delay_frames = 0;
        if let Some(i) = active {
            let v = &self.voices[i];
            if lane == "music" && !self.instant_music {
                if let Some(bpm) = self.library.cues.get(&v.id).and_then(|c| c.bpm) {
                    let beats = self.library.cues[&v.id].beats_per_bar;
                    let bar = (RATE as f64 * 60.0 / bpm as f64 * beats as f64).round() as u64;
                    let phase = v.control.frames.load(Ordering::Relaxed) % bar.max(1);
                    delay_frames = if phase == 0 { 0 } else { bar - phase };
                }
            }
            let v = &mut self.voices[i];
            v.retiring = true;
            v.control.fade_out_after(
                if self.instant_music && lane == "music" { 0.035 } else { self.library
                    .cues
                    .get(&v.id)
                    .and_then(|c| c.fade_out_s)
                    .unwrap_or(fade) },
                delay_frames,
            );
        }
        let control = Arc::new(Control::new(
            volume * cue.gain,
            if self.instant_music && lane == "music" { 0.035 } else { cue.fade_in_s.unwrap_or(fade) },
        ));
        put(&control.master, audio.master_volume);
        put(
            &control.occlusion,
            if cue.music() { 0.0 } else { occlusion },
        );
        let (rack, tail_only_s) = self
            .racks
            .get(&id)
            .cloned()
            .or(if lane == "scene" {
                self.scene_rack.map(|rack| (rack, 0.0))
            } else {
                None
            })
            .unwrap_or_default();
        let mut playback = cue.clone();
        // The opening folder is authored as one complete introductory piece per run.
        if lane == "music_intro" { playback.looping = false; }
        sink.append(
            BedSource::new(pcm, &playback, Arc::clone(&control), rack)
                .with_tail_reverb(tail_only_s)
                .spatial(lane == "engine_hiss")
                .delay(Duration::from_secs_f64(delay_frames as f64 / RATE as f64)),
        );
        if self.paused {
            sink.pause();
        }
        self.voices.push(Voice {
            id: id.clone(),
            lane: lane.into(),
            sink,
            control,
            gain: cue.gain,
            music: cue.music(),
            retiring: false,
            looping: playback.looping,
        });
        eprintln!("[soundscape] {lane}: {id}");
        true
    }
    pub fn update(&mut self, audio: &Audio, dt: f32, input: SoundscapeInput) {
        if audio.capture.is_some() { self.offline = true; }
        self.poll();
        // Warm terminal cues before they are needed, without keeping the entire library resident.
        for id in self.library.events.get("death").cloned().unwrap_or_default() { self.request(&id); }
        if self.death_attempted { return; }
        if self.paused || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.time += dt;
        let action = self.riser.advance(dt);
        self.apply_riser(audio, action);
        for id in self.library.events.get("unnoticed_impact_riser").cloned().unwrap_or_default() {
            self.request(&id);
        }
        let SoundscapeInput {
            mut context,
            act,
            position,
            shelter,
            storm,
            whiteout,
            engine_hiss,
            inside,
            solar_altitude_deg,
            season,
        } = input;
        if let Some(event) = self.solar.update(solar_altitude_deg, context.night_id, dt) {
            self.event(event);
        }
        let zone = self.library.location(act, position);
        if let Some((name, _)) = &zone {
            context.location = name.clone();
        }
        let plan = self.director.update(dt, &context);
        let allow_fakeout = !matches!(plan.mode, crate::soundscape_director::Mode::Combat
            | crate::soundscape_director::Mode::Aftermath) && self.intro_done;
        let available = self.riser_available_lead();
        let action = self.riser.idle_fakeout(allow_fakeout, available);
        self.apply_riser(audio, action);
        let (scene_gain, scene_rack) =
            self.library
                .scene_environment(&plan.location, act, position);
        self.scene_rack = scene_rack.map(|name| RackConfig::preset(&name));
        let mode = plan.mode.as_str();
        let tags = format!(
            "{}|{}|{}|{}|{}|{}",
            plan.location,
            season,
            mode,
            if plan.night { "night" } else { "day" },
            if inside { "inside" } else { "exterior" },
            if storm > 0.5 { "blizzard" } else { "clear" }
        );
        self.last_tags = tags.clone();
        // Selection stays fixed for an entire night, including midnight and entering the tank.
        let night_id = if plan.night {
            self.select("night", &plan.night_id.to_string(), "night", &tags)
        } else {
            self.selections.get("night").and_then(|(_, id)| id.clone())
        };
        let ambient_gain = if plan.night {
            (1.0 - 0.7 * storm.clamp(0.0, 1.0)) * (1.0 - 0.83 * shelter.clamp(0.0, 1.0))
        } else {
            0.0
        };
        self.lane(audio, "night", night_id, ambient_gain, shelter, 5.0);
        let whiteout = if whiteout.is_finite() { whiteout.clamp(0.0, 1.0) } else { 0.0 };
        if !self.blizzard_active && whiteout >= 0.15 {
            self.blizzard_active = true;
            self.blizzard_episode = self.blizzard_episode.wrapping_add(1);
        } else if self.blizzard_active && whiteout <= 0.04 {
            self.blizzard_active = false;
        }
        let blizzard = if self.blizzard_active {
            self.select("blizzard", &self.blizzard_episode.to_string(), "blizzard", &tags)
        } else { None };
        self.lane(audio, "blizzard", blizzard,
            whiteout * (1.0 - 0.83 * shelter.clamp(0.0, 1.0)), shelter, 4.0);
        if engine_hiss.is_some() && !self.engine_failed { self.engine_episode = self.engine_episode.wrapping_add(1); }
        self.engine_failed = engine_hiss.is_some();
        let engine_cue = if self.engine_failed {
            self.select("engine_hiss", &self.engine_episode.to_string(), "engine_hiss", &tags)
        } else { None };
        let source = engine_hiss.unwrap_or(LibraryEmitter {gain:0.0,bearing_deg:0.0,occlusion:0.0});
        self.lane(audio, "engine_hiss", engine_cue, source.gain, source.occlusion, 0.4);
        for voice in &self.voices {
            if voice.lane == "engine_hiss" { put(&voice.control.bearing, source.bearing_deg); }
        }
        let day = self.select("day", &plan.night_id.to_string(), "day", &tags);
        self.lane(
            audio,
            "day",
            day,
            if plan.night {
                0.0
            } else {
                1.0 - 0.83 * shelter.clamp(0.0, 1.0)
            },
            shelter,
            5.0,
        );
        let cabin_pool = self.library.pool("cabin", &tags).join(",");
        let cabin = self.select("cabin", &cabin_pool, "cabin", &tags);
        self.lane(
            audio,
            "cabin",
            cabin,
            if inside { 0.85 } else { 0.0 },
            0.0,
            2.0,
        );
        let scene = self.select("scene", &plan.location, "scene", &tags);
        self.lane(
            audio,
            "scene",
            scene,
            scene_gain * (1.0 - 0.83 * shelter.clamp(0.0, 1.0)),
            if inside { shelter } else { 0.0 },
            3.0,
        );
        // One foreground programme, with rests and deliberately nonliteral state responses.
        let choice = self.program.update(dt, plan.mode);
        if mode != "combat" { self.combat_accent_played = false; }
        // A named combat-start accent is optional and follows the delayed programme draw.
        // Only incoming hostile contact can enter this factual mode; proximity never can.
        if mode == "combat" && choice.role.as_deref() == Some("combat") && !self.combat_accent_played {
            self.combat_accent_played = true;
            self.event("combat_start");
        }
        if !self.intro_done {
            let intro = self.select("intro", "run_start", "intro", &tags);
            if intro.is_none() || mode == "combat" || self.completed.contains_key("music_intro") {
                self.intro_done = true;
                self.lane(audio, "music_intro", None, 0.0, 0.0, 1.0);
            } else {
                self.lane(audio, "music_intro", intro, 0.7, 0.0, 2.0);
            }
        }
        if self.intro_done {
            let key = format!("{}:{}", choice.token, plan.location);
            let mut music = choice.role.as_deref().and_then(|role| self.select("music", &key, role, &tags));
            if music.is_none() && matches!(choice.role.as_deref(), Some("music" | "driving" | "aftermath")) {
                music = self.select("music_fallback", &key, "explore", &tags);
            }
            if let Some((token, id)) = &self.forced_music {
                if *token == choice.token { music = Some(id.clone()); }
                else { self.forced_music = None; }
            }
            if self.lane(audio, "music", music, choice.gain, 0.0, 2.0) { self.instant_music = false; }
        }
        self.warm_gun_successor(plan.mode);
        // Events are explicit authored cue bindings, bounded and rate-limited; no fake battle shots.
        let count = self.events.len();
        for _ in 0..count {
            let Some(name) = self.events.pop_front() else {
                break;
            };
            if self
                .cooldowns
                .get(&name)
                .is_some_and(|end| *end > self.time)
            {
                continue;
            }
            let ids = self.library.events.get(&name).cloned().unwrap_or_default();
            let chosen = self
                .pending_events
                .get(&name)
                .cloned()
                .or_else(|| self.director.choose(&format!("event:{name}"), &ids));
            if let Some(id) = chosen {
                self.request(&id);
                if !self.cache.contains_key(&id) {
                    if !self.failed.contains(&id) {
                        self.pending_events.insert(name.clone(), id);
                        self.events.push_back(name);
                    }
                    continue;
                }
                self.pending_events.remove(&name);
                let cooldown = self.library.cues[&id].cooldown_s;
                self.event_serial = self.event_serial.wrapping_add(1);
                if self.lane(
                    audio,
                    &format!("event:{}", self.event_serial),
                    Some(id.clone()),
                    1.0,
                    0.0,
                    0.02,
                ) {
                    self.cooldowns.insert(name, self.time + cooldown);
                } else {
                    self.pending_events.insert(name.clone(), id);
                    self.events.push_back(name);
                }
            }
        }
        self.plan = Some(plan);
    }
    fn riser_available_lead(&self) -> f32 {
        self.library.events.get("unnoticed_impact_riser").into_iter().flatten()
            .filter(|id| self.cache.contains_key(*id))
            .filter_map(|id| self.riser_cues.get(id).map(|c|c.lead_s))
            .reduce(f32::min).unwrap_or(0.0)
    }
    fn apply_riser(&mut self, audio: &Audio, action: Option<RiserDirective>) {
        let Some(action) = action else { return; };
        self.riser_history.push_back(json!({"time_s":self.time,"action":format!("{action:?}")}));
        while self.riser_history.len() > 32 { self.riser_history.pop_front(); }
        match action {
            RiserDirective::Schedule {key, peak_in_s, lead_s, ..} => {
                self.riser_history.back_mut().unwrap()["admitted"] = json!(false);
                if self.voices.len() >= MAX_VOICES { return; }
                let candidates: Vec<_> = self.library.events.get("unnoticed_impact_riser")
                    .into_iter().flatten().filter(|id|self.cache.contains_key(*id)
                        && self.riser_cues.contains_key(*id)).cloned().collect();
                let Some(id) = self.score_selector.choose("impact_riser", &candidates) else { return; };
                let pcm = self.cache[&id].clone();
                let mut cue = self.library.cues[&id].clone();
                let timing = self.riser_cues[&id];
                if timing.peak_s >= pcm.frames() as f32 / RATE as f32 { return; }
                let Ok(sink) = audio.create_sink_on(EarBus::Score) else { return; };
                cue.looping = false;
                let control = Arc::new(Control::new(cue.gain, cue.fade_in_s.unwrap_or(0.08)));
                put(&control.master, audio.master_volume);
                let (rack, tail) = self.racks.get(&id).cloned().unwrap_or_default();
                sink.append(BedSource::new(pcm,&cue,control.clone(),rack)
                    .with_tail_reverb(tail).with_aligned_peak(timing.peak_s,peak_in_s,lead_s));
                self.voices.push(Voice {id,lane:format!("riser:{key}"),sink,control,
                    gain:cue.gain,music:true,retiring:false,looping:false});
                let entry=self.riser_history.back_mut().unwrap();
                entry["admitted"]=json!(true);
                entry["source_peak_s"]=json!(timing.peak_s);
                entry["peak_at_frame"]=json!((peak_in_s*RATE as f32).round() as u64);
            }
            RiserDirective::Retime {key, peak_in_s} => {
                if let Some(v) = self.voices.iter().find(|v|v.lane==format!("riser:{key}")&&!v.retiring) {
                    let at = v.control.frames.load(Ordering::Relaxed)
                        + (peak_in_s.max(0.01)*RATE as f32).round() as u64;
                    v.control.peak_at.store(at,Ordering::Relaxed);
                }
            }
            RiserDirective::Complete {key} => {
                if let Some(v) = self.voices.iter().find(|v|v.lane==format!("riser:{key}")&&!v.retiring) {
                    // Finish a corrective crossfade just before the marker, rather than
                    // fading away its attack. This adds at most 10ms for an early contact;
                    // a peak already played is never rewound or repeated.
                    v.control.peak_at.store(v.control.frames.load(Ordering::Relaxed)
                        + u64::from(RATE/100),Ordering::Relaxed);
                    // Leave the initial accent, then give the factual strike/ring room in
                    // the mix. This changes only this score voice, never impact calibration.
                    put(&v.control.fade_s,0.12);
                    put(&v.control.gain,v.gain*0.32);
                }
            }
            RiserDirective::Cancel {key,..} => {
                for v in &mut self.voices {
                    if v.lane==format!("riser:{key}")&&!v.retiring {
                        v.retiring=true; v.control.fade_out(0.8);
                    }
                }
            }
        }
    }
    fn warm_gun_successor(&mut self, mode: crate::soundscape_director::Mode) {
        let role = if mode == crate::soundscape_director::Mode::Combat { "combat" } else { "threat" };
        let mut ids = self.library.pool(role, &self.last_tags);
        if ids.is_empty() { ids = self.library.pool("explore", &self.last_tags); }
        ids.retain(|id| !self.voices.iter().any(|v| !v.retiring
            && (v.lane == "music" || v.lane == "music_intro") && v.id == *id));
        if self.gun_successor.as_ref().is_none_or(|id| !ids.contains(id)) {
            self.gun_successor = self.score_selector.choose("main_gun_successor", &ids);
        }
        if let Some(id) = self.gun_successor.clone() { self.request(&id); }
    }
    fn main_gun_fired(&mut self, audio: &Audio) {
        if self.death_attempted || self.paused { return; }
        let mode = self.plan.as_ref().map_or(crate::soundscape_director::Mode::Explore, |p|p.mode);
        let choice = self.program.main_gun_transition(mode);
        self.warm_gun_successor(mode);
        self.intro_done = true;
        for voice in &mut self.voices {
            if voice.lane == "music" || voice.lane == "music_intro" {
                voice.retiring = true;
                voice.control.fade_out(0.035);
            }
        }
        self.completed.remove("music");
        self.completed.remove("music_intro");
        self.instant_music = true;
        if let Some(id) = self.gun_successor.take() {
            self.forced_music = Some((choice.token, id.clone()));
            // Cached successor enters immediately, without a bar wait or game-thread decode.
            // If a first-use load is still pending, the old music nevertheless cuts on the shot.
            if self.lane(audio, "music", Some(id), choice.gain, 0.0, 0.035) {
                self.instant_music = false;
            }
        }
    }
    pub fn event(&mut self, name: &str) {
        if self.events.len() < 16
            && self.library.events.contains_key(name)
            && !self.events.iter().any(|x| x == name)
        {
            self.events.push_back(name.into());
        }
    }
    pub fn set_master_volume(&mut self, volume: f32) {
        for v in &self.voices { put(&v.control.master, volume); }
        if let Some(death) = &self.death { put(&death.voice.control.master, volume); }
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        for v in self.voices.iter().chain(self.death.iter().map(|d| &d.voice)) {
            if paused {
                v.sink.pause()
            } else {
                v.sink.play()
            }
        }
    }
    pub fn snapshot(&self) -> Value {
        json!({"seed":self.director.seed(),"director":self.director.snapshot(),
        "program":self.program.snapshot(),"riser":self.riser.snapshot(),"riser_history":self.riser_history,"death":self.death.as_ref().map(|d|json!({"id":d.voice.id,"duration_s":d.duration,"frames":d.voice.control.frames.load(Ordering::Relaxed),"finished":d.voice.control.finished.load(Ordering::Relaxed)})),
        "time_s":self.time,"selections":self.selections,"mode":self.plan.as_ref().map(|p|p.mode.as_str()),
        "enabled_cues":self.library.cues.len(),"pending_loads":self.requested.len(),"failed":self.failed,
        "cache_bytes":self.cache.values().map(|p|p.samples.len()*4).sum::<usize>(),
        "voices":self.voices.iter().map(|v|json!({"id":v.id,"lane":v.lane,"frames":v.control.frames.load(Ordering::Relaxed),"retiring":v.retiring,
            "peak_at_frame":(v.control.peak_at.load(Ordering::Relaxed)!=u64::MAX).then(||v.control.peak_at.load(Ordering::Relaxed))})).collect::<Vec<_>>()})
    }
}

impl Audio {
    pub fn reset_soundscape(&self, seed: u64) {
        *self.soundscape.borrow_mut() = Some(Runtime::new(seed));
    }
    pub fn prepare_soundscape_review(&self, seed: u64) -> anyhow::Result<()> {
        self.reset_soundscape(seed);
        self.soundscape.borrow_mut().as_mut().unwrap().prepare_review();
        Ok(())
    }
    pub fn update_soundscape(&self, dt: f32, input: SoundscapeInput) {
        let mut state = self.soundscape.borrow_mut();
        if state.is_none() {
            *state = Some(Runtime::new(fresh_seed()));
        }
        state.as_mut().unwrap().update(self, dt, input);
    }
    pub fn soundscape_snapshot(&self) -> Value {
        self.soundscape
            .borrow()
            .as_ref()
            .map_or(Value::Null, Runtime::snapshot)
    }
    pub fn soundscape_audition(&self, id: &str) -> anyhow::Result<f32> {
        let mut state = self.soundscape.borrow_mut();
        let s = state
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("prepare soundscape first"))?;
        s.request(id);
        anyhow::ensure!(s.cache.contains_key(id), "audition cue is unavailable: {id}");
        s.lane(self, "audition", Some(id.into()), 1.0, 0.0, 0.1);
        Ok(s.cache[id].frames() as f32 / RATE as f32)
    }
    pub fn soundscape_main_gun_fired(&self) {
        if let Some(state) = self.soundscape.borrow_mut().as_mut() { state.main_gun_fired(self); }
    }
    /// These facts annotate music only. Projectile collision continues to own all impact SFX.
    pub fn soundscape_incoming_opportunity(&self, opportunity: Option<(u64,f32)>) {
        if let Some(s) = self.soundscape.borrow_mut().as_mut() {
            if s.paused || s.death_attempted { return; }
            let lead = s.riser_available_lead();
            let action = s.riser.opportunity(opportunity,lead);
            s.apply_riser(self,action);
        }
    }
    pub fn soundscape_incoming_selected(&self, key:u64) -> bool {
        self.soundscape.borrow().as_ref().is_some_and(|s|
            !s.paused && !s.death_attempted && s.riser.active_key()==Some(key)
                && s.voices.iter().any(|v|v.lane==format!("riser:{key}")&&!v.retiring))
    }
    pub fn soundscape_incoming_launched(&self, key:u64, projectile_id:u64, eta_s:Option<f32>) {
        if let Some(s) = self.soundscape.borrow_mut().as_mut() {
            if s.paused || s.death_attempted { return; }
            let action = s.riser.launched(key,projectile_id,eta_s); s.apply_riser(self,action);
        }
    }
    pub fn soundscape_incoming_flight(&self, projectile_id:u64, eta_s:Option<f32>) {
        if let Some(s) = self.soundscape.borrow_mut().as_mut() {
            if s.paused || s.death_attempted { return; }
            let action = s.riser.flight(projectile_id,eta_s); s.apply_riser(self,action);
        }
    }
    pub fn soundscape_incoming_resolved(&self, projectile_id:u64, player_tank_hit:bool) {
        if let Some(s) = self.soundscape.borrow_mut().as_mut() {
            if s.paused || s.death_attempted { return; }
            let action = s.riser.resolved(projectile_id,player_tank_hit); s.apply_riser(self,action);
        }
    }
    pub fn soundscape_seed(&self) -> u64 {
        self.soundscape.borrow().as_ref().map_or(0, |s| s.director.seed())
    }
    /// Only this explicitly terminal, finite cue survives the battlefield listener/KIA cut.
    pub fn start_death_score(&self) -> Option<f32> {
        if self.capture.is_none() && self.live_mixer.is_none() { return None; }
        let mut state = self.soundscape.borrow_mut();
        let s = state.get_or_insert_with(|| Runtime::new(fresh_seed()));
        if s.death_attempted { return s.death.as_ref().map(|d|d.duration); }
        s.death_attempted = true;
        let ids = s.library.events.get("death").cloned().unwrap_or_default();
        let id = s.score_selector.choose("death", &ids)?;
        s.decode_now(&id);
        let pcm = s.cache.get(&id)?.clone();
        let mut cue = s.library.cues.get(&id)?.clone();
        cue.looping = false;
        cue.fade_in_s = Some(cue.fade_in_s.unwrap_or(0.005).min(0.01));
        let (rack, tail_only_s) = s.racks.get(&id).cloned().unwrap_or_default();
        let duration = pcm.frames() as f32 / RATE as f32
            + if rack.wet > 0.0 { rack.decay_s.clamp(0.05, 10.0) * 1.5 + 0.25 } else { 128.0 / RATE as f32 };
        let control = Arc::new(Control::new(cue.gain, cue.fade_in_s.unwrap()));
        put(&control.master, self.master_volume);
        let (sink, source) = Sink::new_idle();
        self.terminal_mixer.add(source);
        sink.append(BedSource::new(pcm, &cue, Arc::clone(&control), rack).with_tail_reverb(tail_only_s));
        if s.paused { sink.pause(); }
        s.death = Some(DeathScore { duration, voice: Voice { id, lane: "death".into(), sink, control,
            gain:cue.gain, music:true, retiring:false, looping:false } });
        Some(duration)
    }
    pub fn death_score_elapsed(&self) -> Option<f32> {
        self.soundscape.borrow().as_ref()?.death.as_ref().map(|d|
            d.voice.control.frames.load(Ordering::Relaxed) as f32 / RATE as f32)
    }
    pub fn death_score_finished(&self) -> bool {
        self.soundscape.borrow().as_ref().and_then(|s| s.death.as_ref()).map_or(true, |d|
            d.voice.control.finished.load(Ordering::Relaxed))
    }
    pub fn play_library_event(&self, name: &str) {
        if let Some(state) = self.soundscape.borrow_mut().as_mut() {
            state.event(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn riser_source(peak_in: f32, lead: f32) -> (BedSource,Arc<Control>) {
        let mut c=cue(); c.looping=false; c.fade_out_s=Some(0.005);
        let mut samples=vec![0.01; RATE as usize*2*6];
        samples[RATE as usize*2*4]=0.9; samples[RATE as usize*2*4+1]=0.9;
        let control=Arc::new(Control::new(1.0,0.08));
        let source=BedSource::new(Pcm{samples:samples.into()},&c,control.clone(),RackConfig::default())
            .with_aligned_peak(4.0,peak_in,lead);
        (source,control)
    }
    #[test]
    fn riser_excerpt_peak_is_sample_aligned_and_keeps_natural_tail() {
        let (s,c)=riser_source(2.0,2.0);
        let samples:Vec<_>=s.collect();
        let peak=samples.chunks_exact(2).position(|x|x[0]>0.8).unwrap();
        assert_eq!(peak,RATE as usize*2);
        assert_eq!(samples.len()/2,RATE as usize*4+128);
        assert!(c.finished.load(Ordering::Relaxed));
    }
    #[test]
    fn riser_delayed_birth_and_retime_preserve_pitch_and_peak_deadline() {
        let (mut s,c)=riser_source(4.0,2.0);
        let wait:Vec<_>=s.by_ref().take(RATE as usize*2*2).collect();
        assert!(wait.iter().all(|x|*x==0.0));
        let birth:Vec<_>=s.by_ref().take(RATE as usize/50*2).collect();
        assert!(*birth.last().unwrap()<0.003,"80ms fade starts at audible onset");
        let now=c.frames.load(Ordering::Relaxed);
        c.peak_at.store(now+RATE as u64,Ordering::Relaxed);
        let tail:Vec<_>=s.collect();
        let peak=tail.chunks_exact(2).position(|x|x[0]>0.8).unwrap();
        assert_eq!(peak,RATE as usize);
        assert!(tail.iter().all(|x|x.is_finite()));
    }
    #[test]
    fn riser_cancel_during_scheduled_wait_does_not_leak_source() {
        let (mut s,c)=riser_source(5.0,2.0);
        s.by_ref().take(100).for_each(drop); c.fade_out(0.1);
        let samples:Vec<_>=s.collect();
        assert!(samples.iter().all(|x|*x==0.0));
        assert!(samples.len()<RATE as usize/2);
    }
    #[test]
    fn riser_late_contact_does_not_replay_a_finished_peak() {
        let (mut s,c)=riser_source(2.0,2.0);
        let first:Vec<_>=s.by_ref().take((RATE as f32*2.05) as usize*2).collect();
        assert_eq!(first.iter().filter(|x|**x>0.8).count(),2);
        c.peak_at.store(c.frames.load(Ordering::Relaxed),Ordering::Relaxed);
        assert!(s.all(|x|x<0.8));
    }
    #[test]
    fn riser_early_contact_correction_keeps_attack_within_ten_ms() {
        let (mut s,c)=riser_source(2.0,2.0);
        s.by_ref().take(RATE as usize*2).for_each(drop);
        c.peak_at.store(c.frames.load(Ordering::Relaxed)+u64::from(RATE/100),Ordering::Relaxed);
        let samples:Vec<_>=s.collect();
        assert_eq!(samples.chunks_exact(2).position(|x|x[0]>0.8),Some(RATE as usize/100));
    }
    #[test]
    #[ignore = "exports licensed source fixtures; set PANZER_RISER_REVIEW_PREFIX"]
    fn riser_live_transport_review() {
        let prefix=std::env::var("PANZER_RISER_REVIEW_PREFIX").expect("explicit unique capture prefix");
        let key=(1u64<<62)|1;
        let seed_for=|selected:bool| (1..10000u64).find(|seed| {
            let mut planner=RiserPlanner::new(*seed ^ 0x5249534552);
            planner.opportunity(Some((key,4.0)),4.0).is_some()==selected
        }).unwrap();
        let mut receipts=Vec::new();
        for scenario in ["hit","miss","uncued","fakeout"] {
            let file=format!("{prefix}_{scenario}.wav");
            assert!(!std::path::Path::new(&file).exists(),"preserve prior review");
            let audio=Audio::new_capture();
            let seed=seed_for(scenario!="uncued");
            audio.prepare_soundscape_review(seed).unwrap();
            {
                let mut state=audio.soundscape.borrow_mut(); let s=state.as_mut().unwrap();
                let ids=s.library.events.get("unnoticed_impact_riser").expect("riser binding").clone();
                for id in ids {s.request(&id);}
                assert!(s.riser_available_lead()>=4.0);
            }
            let mut samples=Vec::new(); let mut physical_impacts=0;
            let mut selected=false;
            for tick in 0..240 {
                let time=tick as f32*0.05;
                if tick==20 {
                    if scenario=="fakeout" {
                        let mut state=audio.soundscape.borrow_mut();let s=state.as_mut().unwrap();
                        s.riser.advance(211.0);
                        let lead=s.riser_available_lead();let action=s.riser.idle_fakeout(true,lead);
                        assert!(action.is_some());s.apply_riser(&audio,action);
                    } else {audio.soundscape_incoming_opportunity(Some((key,4.0)));}
                    selected=audio.soundscape_incoming_selected(key);
                }
                if tick==80 {audio.soundscape_incoming_launched(key,91,Some(1.0));}
                if tick==90 && scenario=="miss" {audio.soundscape_incoming_resolved(91,false);}
                if tick==100 && matches!(scenario,"hit"|"uncued") {
                    audio.soundscape_incoming_resolved(91,true);
                    // Explicit staged collision: existing physical impact path, never from planner.
                    audio.play_at_ear(super::super::Sfx::ArmorClang1,
                        crate::sound_levels::HIT_SQUARE_NEAR.laf_db);
                    physical_impacts+=1;
                }
                {
                    let mut state=audio.soundscape.borrow_mut();let s=state.as_mut().unwrap();
                    s.time=time;let action=s.riser.advance(0.05);s.apply_riser(&audio,action);s.poll();
                }
                samples.extend(audio.render_capture_frames(RATE as usize/20).unwrap());
            }
            assert_eq!(selected,matches!(scenario,"hit"|"miss"));
            assert!(samples.iter().all(|x|x.is_finite()&&x.abs()<=1.0));
            assert!(samples[samples.len()-RATE as usize..].iter().all(|x|x.abs()<1e-5));
            crate::acoustics::write_wav_stereo(&file,&samples,RATE).unwrap();
            receipts.push(json!({"scenario":scenario,"file":file,"seed":seed,"seconds":12,
                "physical_impacts":physical_impacts,"impact_s":if physical_impacts==1 {Some(5.0)}else{None},
                "selected":selected,"peak":samples.iter().map(|x|x.abs()).fold(0.0f32,f32::max),
                "state":audio.soundscape_snapshot()}));
        }
        let receipt=format!("{prefix}_receipt.json");
        assert!(!std::path::Path::new(&receipt).exists());
        std::fs::write(receipt,serde_json::to_vec_pretty(&json!({"kind":"staged context and collision fixtures through live Audio mixer; not gameplay recordings",
            "subjective_listening_accepted":false,"fixtures":receipts})).unwrap()).unwrap();
    }
    fn cue() -> Cue {
        Cue {
            id: "test".into(),
            path: Default::default(),
            role: "night".into(),
            tags: vec![],
            contexts: vec![],
            role_aliases: vec![],
            role_contexts: Default::default(),
            gain: 1.0,
            crossfade_s: 0.1,
            equal_gain_seam: false,
            fade_in_s: None,
            fade_out_s: None,
            looping: true,
            bpm: None,
            beats_per_bar: 4,
            cooldown_s: 1.0,
        }
    }
    #[test]
    fn positional_library_source_moves_between_ears_without_restarting() {
        let mut cue=cue(); cue.looping=true;
        let control=Arc::new(Control::new(1.0,0.001));
        put(&control.bearing,-90.0);
        let mut source=BedSource::new(Pcm{samples:vec![0.2;RATE as usize*2].into()},
            &cue,control.clone(),RackConfig::default()).spatial(true);
        let left:Vec<_>=source.by_ref().take(4410*2).collect();
        assert!(left[left.len()-2]>0.19 && left[left.len()-1].abs()<1e-5);
        let before=control.frames.load(Ordering::Relaxed); put(&control.bearing,90.0);
        let right:Vec<_>=source.by_ref().take(4410*2).collect();
        assert!(right[right.len()-1]>0.19 && right[right.len()-2].abs()<1e-5);
        assert!(control.frames.load(Ordering::Relaxed)>before);
    }
    #[test]
    fn selective_reverb_preserves_dry_attack_and_rings_after_finite_source() {
        let mut cue = cue(); cue.looping=false; cue.fade_out_s=Some(0.005);
        let pcm=Pcm{samples: vec![0.2; RATE as usize * 2].into()};
        let control=Arc::new(Control::new(1.0, 0.001));
        let mut source=BedSource::new(pcm,&cue,control.clone(),RackConfig::preset("large_room"))
            .with_tail_reverb(0.25);
        let samples:Vec<_>=source.by_ref().collect();
        assert!((samples[RATE as usize] - 0.2).abs()<1e-6, "attack/body stay dry");
        assert!(samples[RATE as usize*2+1000..].iter().any(|x| x.abs()>1e-5), "reverb survives sample end");
        assert!(samples.iter().all(|x|x.is_finite()));
        assert!(control.finished.load(Ordering::Relaxed));
        assert!(samples.len() < RATE as usize*2*5, "finite rack must terminate");
    }
    #[test]
    fn overlapping_seam_removes_silent_tail_and_stops_at_zero() {
        let mut data = vec![0.1; RATE as usize * 2];
        for x in &mut data[RATE as usize * 2 - 2000..] {
            *x = 0.0;
        }
        let ctl = Arc::new(Control::new(1.0, 0.02));
        let mut s = BedSource::new(
            Pcm {
                samples: data.into(),
            },
            &cue(),
            ctl.clone(),
            RackConfig::default(),
        );
        let rendered: Vec<_> = s.by_ref().take(RATE as usize * 4).collect();
        assert!(rendered[86000..92000].iter().all(|x| *x > 0.001));
        ctl.fade_out(0.02);
        let tail: Vec<_> = s.collect();
        assert!(tail.len() < 2000);
        assert!(tail.last().unwrap().abs() < 0.001);
    }
    #[test]
    fn rhythmic_period_not_shortened() {
        let mut c = cue();
        c.bpm = Some(120.0);
        let ctl = Arc::new(Control::new(1.0, 0.02));
        let mut s = BedSource::new(
            Pcm {
                samples: vec![0.1; RATE as usize * 4].into(),
            },
            &c,
            ctl,
            RackConfig::default(),
        );
        for _ in 0..RATE * 4 {
            s.next();
        }
        assert_eq!(s.cursor, 0);
        assert_eq!(s.elapsed, RATE as u64 * 2);
    }
    #[test]
    fn delayed_start_retains_birth_fade_and_master_is_independent_of_retirement() {
        let ctl = Arc::new(Control::new(1.0, 2.0));
        let mut source = BedSource::new(
            Pcm {
                samples: vec![0.1; RATE as usize * 6].into(),
            },
            &cue(),
            ctl.clone(),
            RackConfig::default(),
        );
        // Ordinary game controls update while this source is waiting for a bar boundary.
        put(&ctl.fade_s, 0.3);
        let first: Vec<_> = source.by_ref().take(RATE as usize).collect();
        assert!(
            (first.last().unwrap() - 0.025).abs() < 0.001,
            "2s birth fade was overwritten"
        );
        ctl.fade_out_after(1.0, RATE as u64);
        put(&ctl.master, 0.0);
        let muted: Vec<_> = source.by_ref().take(RATE as usize).collect();
        assert!(muted[10000..].iter().all(|x| x.abs() < 1e-6));
        assert!(
            !ctl.finished.load(Ordering::Relaxed),
            "master mute must not destroy a scheduled tail"
        );
    }
    fn runtime_fixture() -> Runtime {
        let mut r = Runtime::new(42);
        r.library = Library::default();
        for (id, role) in [
            ("night_a", "night"),
            ("night_b", "night"),
            ("cabin_a", "cabin"),
            ("event_a", "effect"),
        ] {
            let mut c = cue();
            c.id = id.into();
            c.role = role.into();
            if role == "night" {
                c.contexts = vec!["night".into()];
            }
            r.library.cues.insert(id.into(), c);
            r.insert(
                id.into(),
                Pcm {
                    samples: vec![0.1; RATE as usize * 2].into(),
                },
            );
        }
        r
    }
    fn input(night: bool, id: i64, inside: bool, combat: bool) -> SoundscapeInput {
        SoundscapeInput {
            context: Context {
                night,
                night_id: id,
                combat,
                ..Default::default()
            },
            act: 1,
            position: [0.0, 0.0],
            shelter: if inside { 0.9 } else { 0.0 },
            storm: 0.0,
            whiteout: 0.0,
            engine_hiss: None,
            inside,
            solar_altitude_deg: if night { -10.0 } else { 10.0 },
            season: "winter",
        }
    }
    #[test]
    fn actual_transport_keeps_night_and_independent_cabin_choices() {
        let a = Audio::new_capture();
        let mut r = runtime_fixture();
        r.update(&a, 0.1, input(false, 7, false, false));
        assert!(!r.selections.contains_key("night"));
        r.update(&a, 0.1, input(true, 7, false, false));
        let night = r.selections["night"].1.clone();
        let cabin = r.selections["cabin"].1.clone();
        r.update(&a, 0.1, input(true, 7, true, true));
        assert_eq!(r.selections["night"].1, night);
        assert_eq!(r.selections["cabin"].1, cabin);
        r.update(&a, 0.1, input(true, 8, true, false));
        assert_ne!(r.selections["night"].1, night);
    }
    #[test]
    fn main_gun_music_cut_is_immediate_and_leaves_environment_tracks_running() {
        let a = Audio::new_capture();
        let mut r = runtime_fixture();
        for id in ["suspense_a", "suspense_b"] {
            let mut c=cue(); c.id=id.into(); c.role="threat".into(); c.bpm=Some(120.0);
            r.library.cues.insert(id.into(), c);
            r.insert(id.into(),Pcm{samples:vec![0.1; RATE as usize*4].into()});
        }
        r.update(&a,0.1,input(true,7,true,false));
        r.lane(&a,"music",Some("suspense_a".into()),0.5,0.0,0.1);
        a.render_capture_frames(4410).unwrap();
        r.main_gun_fired(&a);
        let bed_before=r.voices.iter().find(|v|v.lane=="night").unwrap().control.frames.load(Ordering::Relaxed);
        let music=r.voices.iter().find(|v|v.lane=="music"&&!v.retiring).unwrap();
        assert_eq!(music.id,"suspense_b");
        assert_eq!(r.program.update(0.0,crate::soundscape_director::Mode::Explore).role.as_deref(),Some("threat"));
        a.render_capture_frames(4410).unwrap();
        assert!(music.control.frames.load(Ordering::Relaxed)>4000,"no waiting for the next musical bar");
        let bed=r.voices.iter().find(|v|v.lane=="night").unwrap();
        assert!(!bed.retiring && bed.control.frames.load(Ordering::Relaxed)>bed_before);
    }
    #[test]
    fn finite_engine_effect_finishes_once_and_rearms_only_after_repair() {
        let a=Audio::new_capture(); let mut r=runtime_fixture();
        let mut c=cue(); c.id="hiss".into(); c.role="engine_hiss".into(); c.looping=false;
        r.library.cues.insert(c.id.clone(),c);
        r.insert("hiss".into(),Pcm{samples:vec![0.1;RATE as usize/5].into()});
        let damaged=|| {let mut i=input(true,1,false,false);
            i.engine_hiss=Some(LibraryEmitter{gain:0.5,bearing_deg:45.0,occlusion:0.0});i};
        r.update(&a,0.1,damaged()); a.render_capture_frames(RATE as usize).unwrap();
        r.update(&a,0.1,damaged()); assert_eq!(r.completed.get("engine_hiss").map(String::as_str),Some("hiss"));
        assert!(!r.voices.iter().any(|v|v.lane=="engine_hiss"));
        r.update(&a,0.1,input(true,1,false,false));
        r.update(&a,0.1,damaged());
        assert!(r.voices.iter().any(|v|v.lane=="engine_hiss"&&!v.retiring));
    }
    #[test]
    fn full_voice_budget_defers_event_and_does_not_retire_bed() {
        let a = Audio::new_capture();
        let mut r = runtime_fixture();
        assert!(r.lane(&a, "night", Some("night_a".into()), 1.0, 0.0, 0.1));
        for i in 1..MAX_VOICES {
            assert!(r.lane(
                &a,
                &format!("occupant:{i}"),
                Some("event_a".into()),
                1.0,
                0.0,
                0.1
            ));
        }
        assert!(!r.lane(&a, "night", Some("night_b".into()), 1.0, 0.0, 0.1));
        assert!(!r.voices[0].retiring);
        r.library
            .events
            .insert("hit".into(), vec!["event_a".into()]);
        r.event("hit");
        r.update(&a, 0.1, input(true, 7, false, false));
        assert!(!r.cooldowns.contains_key("hit"));
        assert_eq!(r.pending_events["hit"], "event_a");
    }
}
