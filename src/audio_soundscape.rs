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
#[path = "soundscape_layers.rs"]
mod layers;
const MUSIC_LEVEL:f32=0.25;
const ATMOSPHERE_LEVEL:f32=0.33;
const MAX_VOICES: usize = 16;
const CACHE_BYTES: usize = 256 * 1024 * 1024;
const MAX_REQUESTS: usize = 4;
/// Minimum simulated dwell of one foreground passage before a programme change may replace it.
const FOREGROUND_RETENTION_S: f32 = 120.0;
/// Cabin and day beds chosen once per process, so death-and-retry does not re-roll the ambience.
/// Keyed by lane and eligible pool; a different act season or library picks independently.
static SESSION_BEDS: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
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
    hold_silent: AtomicBool,
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
            hold_silent: AtomicBool::new(false),
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
/// Normal-rate finite excerpt scheduled so its authored peak meets a musical boundary.
struct AlignedPeak {
    marker: usize,
    lead: u64,
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
        self.aligned_peak = Some(AlignedPeak {marker, lead, started:false});
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
            if !alignment.started {
                self.cursor = alignment.marker.saturating_sub(remaining as usize).min(self.pcm.frames());
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
                self.remaining = (fade.clamp(1.0 / RATE as f32, 120.0) * RATE as f32) as u64;
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
        if retiring && self.remaining == 0 && self.gain <= 0.000001
            && !self.control.hold_silent.load(Ordering::Relaxed) {
            self.control.finished.store(true, Ordering::Relaxed);
            return None;
        }
        let n = self.pcm.frames();
        let mut frame = [0.0; 2];
        if self.cursor < n {
            for (c, x) in frame.iter_mut().enumerate() {
                *x = self.pcm.samples[self.cursor * 2 + c];
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

struct PreparedDeath { cue: Cue, pcm: Pcm, rack: RackConfig, tail_only_s: f32, duration: f32 }
struct DeathScore {
    voice: Voice,
    duration: f32,
    // Observe completion on the game thread, then drain the shared output lookahead.
    finished_observed_frame: std::cell::Cell<Option<u64>>,
}
#[derive(Clone, Copy)]
struct RiserCue { peak_s: f32, lead_s: f32 }
struct MusicBridge { key:u64, id:String, role:Option<String>, old:Option<Arc<Control>> }


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
    prepared_death: Option<PreparedDeath>,
    death: Option<DeathScore>,
    completed: HashMap<String, String>,
    riser: RiserPlanner,
    riser_cues: HashMap<String, RiserCue>,
    riser_history: VecDeque<Value>,
    music_role: Option<String>,
    music_bridge: Option<MusicBridge>,
    music_delay_frames: u64,
    layers:layers::Layers,
    layer_gains:layers::Gains,
    foreground_since:f32,
    session_beds:bool,
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
            prepared_death: None,
            death: None,
            completed: HashMap::new(),
            riser: RiserPlanner::new(seed ^ 0x5249534552),
            riser_cues,
            riser_history: VecDeque::new(),
            music_role: None,
            music_bridge: None,
            music_delay_frames: 0,
            layers:layers::Layers::default(),
            layer_gains:layers::Gains::default(),
            foreground_since:0.0,
            session_beds:true,
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
    // Seeded reviews stay reproducible: they never inherit another run's bed choice.
    pub fn prepare_review(&mut self) { self.offline = true; self.session_beds = false; }
    fn select(&mut self, lane: &str, key: &str, role: &str, tag: &str) -> Option<String> {
        let mut ids = self.library.pool(role, tag);
        let score = matches!(lane, "music" | "music_fallback");
        if score {
            // One-shot hits are accents, not a foreground passage followed by minutes of silence.
            ids.retain(|id| self.library.cues.get(id).is_some_and(Cue::foreground));
        }
        // A foreground choice belongs to its programme token: context tags (driving, inside,
        // combat, night) entering or leaving the pool must not re-roll it mid passage.
        let key = if lane == "night" || score {
            key.to_owned()
        } else {
            format!("{key}:{}", ids.join(","))
        };
        if let Some((previous, id)) = self.selections.get(lane) {
            if previous == &key {
                return id.clone();
            }
        }
        if matches!(lane,"music"|"music_fallback") {
            if let Some(current)=self.voices.iter().find(|v|v.lane=="music"&&!v.retiring)
                .and_then(|v|self.library.cues.get(&v.id)) {
                let compatible:Vec<_>=ids.iter().filter(|id|self.library.cues.get(*id)
                    .is_some_and(|next|current.musically_compatible(next))).cloned().collect();
                if !compatible.is_empty() {ids=compatible;}
            }
        }
        self.completed.remove(lane);
        let session = (self.session_beds && matches!(lane, "cabin" | "day") && !ids.is_empty())
            .then(|| format!("{lane}:{}", ids.join(",")));
        let held = session.as_ref().and_then(|slot| {
            let beds = SESSION_BEDS.lock().unwrap_or_else(|e| e.into_inner());
            beds.iter().find(|(name, _)| name == slot).map(|(_, id)| id.clone())
        });
        let id = if held.is_some() {
            held
        } else if lane.starts_with("music") || lane == "intro" {
            self.score_selector.choose(role, &ids)
        } else { self.director.choose(role, &ids) };
        if let (Some(slot), Some(id)) = (session, &id) {
            let mut beds = SESSION_BEDS.lock().unwrap_or_else(|e| e.into_inner());
            if !beds.iter().any(|(name, _)| *name == slot) { beds.push((slot, id.clone())); }
        }
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
        let multiplier=if lane.starts_with("music") || lane.starts_with("score_layer")
            || id.as_ref().and_then(|id|self.library.cues.get(id)).is_some_and(Cue::music) {MUSIC_LEVEL}
            else if matches!(lane,"night"|"day"|"blizzard"|"cabin"|"scene") {ATMOSPHERE_LEVEL}
            else {1.0};
        let volume=volume*multiplier;
        let active = self
            .voices
            .iter()
            .position(|v| v.lane == lane && !v.retiring);
        if let Some(i) = active {
            if id.as_deref() == Some(&self.voices[i].id) {
                let v = &self.voices[i];
                put(&v.control.fade_s, if lane.starts_with("score_layer") {3.0}else if v.music {8.0}else{fade});
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
        let mut delay_frames = if lane=="music_pending" {self.music_delay_frames}
            else if lane.starts_with("score_layer") {self.voices.iter().find(|v|v.lane=="music"&&!v.retiring)
                .and_then(|v|self.library.cues.get(&v.id).and_then(|c|c.beat_delay_frames(v.control.frames.load(Ordering::Relaxed),RATE))).unwrap_or(0)}
            else {0};
        if let Some(i) = active {
            let v=&self.voices[i];
            let previous=self.library.cues.get(&v.id);
            let elapsed=v.control.frames.load(Ordering::Relaxed);
            let clean=lane=="music"&&!self.instant_music
                && previous.is_none_or(|prior|!prior.musically_compatible(cue));
            let release=if clean {6.0}else if self.instant_music&&lane=="music" {0.035}
                else{previous.and_then(|c|c.fade_out_s).unwrap_or(fade).max(if lane=="music" {8.0}else{fade})};
            if lane=="music"&&!self.instant_music {
                let before=if clean {(release*RATE as f32) as u64}else{0};
                delay_frames=before+previous.and_then(|c|c.beat_delay_frames(elapsed+before,RATE)).unwrap_or(0);
            }
            let v=&mut self.voices[i];v.retiring=true;
            v.control.fade_out_after(release,if clean {
                delay_frames.saturating_sub((release*RATE as f32) as u64)
            }else{delay_frames});
        }
        let control = Arc::new(Control::new(
            volume * cue.gain,
            if lane=="music_intro" {cue.fade_in_s.unwrap_or(fade).max(12.0)} else if lane=="music_pending" {0.12} else if self.instant_music && lane == "music" { 0.035 } else { cue.fade_in_s.unwrap_or(fade).max(if lane=="music" {8.0}else{fade}) },
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
        if lane=="music" {self.foreground_since=self.time;}
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
        for id in self.library.events.get("intensity_transition_riser").cloned().unwrap_or_default() {
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
        self.layer_gains=self.layers.update(dt,context.combat);
        let plan = self.director.update(dt, &context);
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
        // The night identity changes at local noon; a day bed keyed on it would swap mid-day.
        let day = self.select("day", season, "day", &tags);
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
            6.0,
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
        if !self.intro_done {
            let intro = self.select("intro", "run_start", "intro", &tags);
            // The authored opening plays through; contact and shots do not chop it.
            if intro.as_ref().is_none_or(|id| self.failed.contains(id))
                || self.completed.contains_key("music_intro") {
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
            if let Some(id)=self.eligible_music_override(choice.token,choice.role.as_deref(),&tags) {
                music=Some(id);
            }
            self.update_music(audio,dt,plan.mode,&choice,music);
        }
        // A retained foreground never takes the combat role itself; its layers do.
        let combat_passage=self.music_role.as_deref()==Some("combat")
            || (choice.role.as_deref()==Some("combat") && self.layer_gains.anchor_held
                && self.voices.iter().any(|v|v.lane=="music"&&!v.retiring));
        if mode=="combat" && combat_passage && !self.combat_accent_played {
            self.combat_accent_played=true;self.event("combat_start");
        }
        self.update_score_layers(audio,&tags);
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
    fn update_score_layers(&mut self,audio:&Audio,tags:&str) {
        let anchor=self.voices.iter().find(|v|v.lane=="music"&&!v.retiring&&v.looping)
            .and_then(|v|self.library.cues.get(&v.id)).cloned();
        for (lane,gain,rhythm) in [("score_layer_texture",self.layer_gains.texture*0.30,false),
            ("score_layer_rhythm",self.layer_gains.rhythm*0.55,true)] {
            if gain<=0.00001 {self.lane(audio,lane,None,0.0,0.0,8.0);continue;}
            let Some(anchor)=&anchor else{continue;};
            let mut ids=self.library.pool("combat",tags);
            ids.retain(|id|self.library.cues.get(id).is_some_and(|c|layers::compatible(anchor,c)
                && (if rhythm {layers::explicit_percussion(c)}else{!layers::explicit_percussion(c)}))
                && !self.voices.iter().any(|v|v.lane!=lane&&!v.retiring&&v.id==*id));
            // All concurrent layers must agree with each other as well as the foreground.
            ids.retain(|id|self.voices.iter().filter(|v|v.lane.starts_with("score_layer")&&!v.retiring&&v.lane!=lane)
                .all(|v|self.library.cues.get(&v.id).zip(self.library.cues.get(id))
                    .is_some_and(|(a,b)|layers::compatible(a,b)||layers::compatible(b,a))));
            let key=format!("{}:{}:{}",self.layers.episode,anchor.id,lane);
            let id=if let Some((old,id))=self.selections.get(lane).filter(|(old,_)|*old==key) {let _=old;id.clone()}
                else{let id=self.score_selector.choose(lane,&ids);self.selections.insert(lane.into(),(key,id.clone()));id};
            if let Some(id)=&id {self.request(id);}
            self.lane(audio,lane,id,gain,0.0,8.0);
        }
    }
    fn riser_available_lead(&self) -> f32 {
        self.library.events.get("intensity_transition_riser").into_iter().flatten()
            .filter(|id| self.cache.contains_key(*id))
            .filter_map(|id| self.riser_cues.get(id).map(|c|c.lead_s))
            .reduce(f32::min).unwrap_or(0.0)
    }
    fn start_riser(&mut self,audio:&Audio,key:u64,peak_in_s:f32,lead_s:f32)->bool {
        if self.voices.len()+2>MAX_VOICES {return false;}
        let ids:Vec<_>=self.library.events.get("intensity_transition_riser").into_iter().flatten()
            .filter(|id|self.cache.contains_key(*id)&&self.riser_cues.contains_key(*id)).cloned().collect();
        let Some(id)=self.score_selector.choose("intensity_riser",&ids) else{return false;};
        let pcm=self.cache[&id].clone();let mut cue=self.library.cues[&id].clone();
        let timing=self.riser_cues[&id];
        if timing.peak_s>=pcm.frames() as f32/RATE as f32{return false;}
        let Ok(sink)=audio.create_sink_on(EarBus::Score) else{return false;};
        cue.looping=false;
        let control=Arc::new(Control::new(cue.gain*MUSIC_LEVEL,cue.fade_in_s.unwrap_or(0.08)));
        put(&control.master,audio.master_volume);
        let (rack,tail)=self.racks.get(&id).cloned().unwrap_or_default();
        sink.append(BedSource::new(pcm,&cue,control.clone(),rack).with_tail_reverb(tail)
            .with_aligned_peak(timing.peak_s,peak_in_s,lead_s));
        self.voices.push(Voice{id,lane:format!("riser:{key}"),sink,control,gain:cue.gain,
            music:true,retiring:false,looping:false});
        true
    }
    fn cancel_music_bridge(&mut self) {
        let Some(bridge)=self.music_bridge.take() else{return;};
        for v in &mut self.voices {
            if v.lane=="music_pending"&&!v.retiring {
                v.retiring=true;
                // A source still behind its delay has no audible tail to preserve.
                if v.control.frames.load(Ordering::Relaxed)==0 {v.sink.stop();}
                else {v.control.fade_out(0.12);}
            }
            if v.lane==format!("riser:{}",bridge.key) {
                v.retiring=true;v.control.fade_out(0.3);
            }
            if bridge.old.as_ref().is_some_and(|old|Arc::ptr_eq(old,&v.control))
                && !v.control.finished.load(Ordering::Relaxed) {
                v.retiring=false;v.control.retire.store(false,Ordering::Relaxed);
                v.control.hold_silent.store(false,Ordering::Relaxed);
                put(&v.control.fade_s,0.12);
            }
        }
    }
    fn finish_music_bridge(&mut self,key:u64) {
        if self.music_bridge.as_ref().is_none_or(|b|b.key!=key){return;}
        let bridge=self.music_bridge.take().unwrap();
        if let Some(old)=bridge.old {old.hold_silent.store(false,Ordering::Relaxed);}
        for v in &mut self.voices {
            if v.lane=="music_pending"&&!v.retiring {
                v.lane="music".into();
                // The bridged passage starts its own minimum dwell now.
                self.foreground_since=self.time;
            }
            if v.lane==format!("riser:{key}")&&!v.retiring {
                put(&v.control.fade_s,0.35);put(&v.control.gain,v.gain*0.45*MUSIC_LEVEL);
            }
        }
        self.music_role=bridge.role;
    }
    fn update_music(&mut self,audio:&Audio,dt:f32,mode:crate::soundscape_director::Mode,
        choice:&crate::soundscape_program::Choice,music:Option<String>) {
        // Cancel a queued destination against the requested programme before
        // foreground retention can substitute the still-playing anchor.
        if self.music_bridge.as_ref().is_some_and(|bridge|Some(bridge.id.as_str())!=music.as_deref()) {
            self.riser.interrupt();self.cancel_music_bridge();
        }
        // Finite pieces are held too: they end naturally, never by a programme change.
        if let Some(current)=self.voices.iter().find(|v|v.lane=="music"&&!v.retiring) {
            if self.time-self.foreground_since<FOREGROUND_RETENTION_S || self.layer_gains.anchor_held {
                let id=current.id.clone();
                // A changed score role adds layers while this actual phrase continues;
                // never schedule a transition whose destination is this same source.
                self.riser.interrupt();self.cancel_music_bridge();
                self.lane(audio,"music",Some(id),choice.gain.max(0.45),0.0,8.0);
                self.instant_music=false;
                return;
            }
        }
        if self.music_bridge.as_ref().is_some_and(|bridge|Some(bridge.id.as_str())!=music.as_deref()) {
            self.riser.interrupt();self.cancel_music_bridge();
        }
        if let Some(current)=self.voices.iter().find(|v|v.lane=="music"&&!v.retiring&&Some(v.id.as_str())==music.as_deref()) {
            let id=current.id.clone();
            self.riser.interrupt();self.cancel_music_bridge();
            self.lane(audio,"music",Some(id),choice.gain,0.0,8.0);
            self.music_role=choice.role.clone();self.instant_music=false;
            return;
        }
        if let Some(id)=&music {self.request(id);}
        let available=if music.as_ref().is_some_and(|id|self.cache.contains_key(id))
            && self.voices.len()+2<=MAX_VOICES && !self.instant_music {
                self.riser_available_lead()
            }else{0.0};
        let action=if self.instant_music {self.riser.interrupt()}else{
            self.riser.update(dt,self.music_role.as_deref(),choice,
                mode==crate::soundscape_director::Mode::Combat,available)};
        if let Some(action)=action {
            self.riser_history.push_back(json!({"time_s":self.time,"action":format!("{action:?}")}));
            while self.riser_history.len()>32 {self.riser_history.pop_front();}
            match action {
                RiserDirective::Schedule{key,peak_in_s,lead_s} => {
                    let old=self.voices.iter().find(|v|v.lane=="music"&&!v.retiring);
                    let current=old.and_then(|v|self.library.cues.get(&v.id));
                    let elapsed=old.map_or(0,|v|v.control.frames.load(Ordering::Relaxed));
                    // Leave room for a clean handover into a riser with unknown key/tempo.
                    let minimum=((peak_in_s+0.35)*RATE as f32).round() as u64;
                    let aligned=(minimum+current.and_then(|c|c.beat_delay_frames(elapsed+minimum,RATE))
                        .unwrap_or(0)) as f32/RATE as f32;
                    let compatible=music.as_ref().and_then(|id|self.library.cues.get(id))
                        .zip(current).is_some_and(|(next,prior)|prior.musically_compatible(next));
                    let prior_cue=current.cloned();
                    let next_cue=music.as_ref().and_then(|id|self.library.cues.get(id)).cloned();
                    let previous=old.map(|v|v.control.clone());
                    let peak=if self.riser.align_peak(key,aligned){aligned}else{peak_in_s};
                    let mut admitted=false;
                    if self.start_riser(audio,key,peak,lead_s) {
                        let riser_voice=self.voices.last().unwrap();
                        let riser_cue=&self.library.cues[&riser_voice.id];
                        let old_overlap=prior_cue.as_ref().is_some_and(|c|c.musically_compatible(riser_cue));
                        let new_overlap=next_cue.as_ref().is_some_and(|c|riser_cue.musically_compatible(c));
                        let riser_control=riser_voice.control.clone();
                        self.music_delay_frames=(peak*RATE as f32).round() as u64;
                        admitted=self.lane(audio,"music_pending",music.clone(),choice.gain,0.0,0.12);
                        self.music_bridge=Some(MusicBridge{key,id:music.clone().unwrap(),role:choice.role.clone(),old:previous.clone()});
                        if admitted {
                            if let Some(old)=previous {
                                let fade_at=if !old_overlap {(peak-lead_s-0.3).max(0.0)}
                                    else if compatible {peak}else{(peak-0.3).max(0.0)};
                                old.hold_silent.store(true,Ordering::Relaxed);
                                old.fade_out_after(0.3,(fade_at*RATE as f32).round() as u64);
                                for v in &mut self.voices {if Arc::ptr_eq(&v.control,&old){v.retiring=true;}}
                            }
                            if !new_overlap {
                                // Keep the peak; avoid a long tonal/reverb overlap when key or
                                // tempo is unknown. The incoming track fades in over the same120ms.
                                riser_control.fade_out_after(0.12,self.music_delay_frames);
                                for v in &mut self.voices {if Arc::ptr_eq(&v.control,&riser_control){v.retiring=true;}}
                            }
                        }else{self.cancel_music_bridge();self.riser.interrupt();}
                    }else{self.riser.interrupt();}
                    let log=self.riser_history.back_mut().unwrap();
                    log["admitted"]=json!(admitted);log["aligned_peak_in_s"]=json!(peak);
                    log["harmonic_and_tempo_overlap"]=json!(compatible);
                    log["target"]=json!(music);
                }
                RiserDirective::Complete{key}=>self.finish_music_bridge(key),
                RiserDirective::Cancel{..}=>self.cancel_music_bridge(),
            }
        }
        if self.music_bridge.is_some() {
            if let Some(v)=self.voices.iter().find(|v|v.lane=="music_pending"&&!v.retiring) {
                put(&v.control.gain,choice.gain*v.gain*MUSIC_LEVEL);put(&v.control.master,audio.master_volume);
            }
            return;
        }
        let gain=if self.layer_gains.anchor_held {choice.gain.max(0.45)}else{choice.gain};
        if self.lane(audio,"music",music,gain,0.0,8.0) {
            self.music_role=choice.role.clone();self.instant_music=false;
        }
    }
    fn eligible_music_override(&mut self,token:u64,role:Option<&str>,tags:&str)->Option<String> {
        let Some((held_token,id))=&self.forced_music else {return None;};
        let mut eligible=role.map_or_else(Vec::new,|role|self.library.pool(role,tags));
        if eligible.is_empty() && role.is_some() {eligible=self.library.pool("explore",tags);}
        // A passage retained across the shot stays for this token whatever role it came from.
        let playing=self.voices.iter().any(|v|v.lane=="music"&&!v.retiring&&v.id==*id);
        if *held_token==token && (playing || eligible.contains(id)) {return Some(id.clone());}
        self.forced_music=None;
        None
    }
    fn warm_gun_successor(&mut self, mode: crate::soundscape_director::Mode) {
        let role = if mode == crate::soundscape_director::Mode::Combat { "combat" } else { "threat" };
        let mut ids = self.library.pool(role, &self.last_tags);
        if ids.is_empty() { ids = self.library.pool("explore", &self.last_tags); }
        ids.retain(|id| self.library.cues.get(id).is_some_and(Cue::foreground));
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
        let before = self.program.current_token();
        let choice = self.program.main_gun_transition(mode);
        if before == choice.token { return; }
        self.riser.interrupt();
        self.cancel_music_bridge();
        self.warm_gun_successor(mode);
        // The opening piece plays through; the raised programme follows it.
        if !self.intro_done && self.voices.iter().any(|v|v.lane=="music_intro"&&!v.retiring) { return; }
        self.intro_done = true;
        let retained=self.voices.iter().find(|v|v.lane=="music"&&!v.retiring)
            .map(|v|v.id.clone());
        if let Some(id)=retained {
            // A shot raises the programme without chopping the existing loop mid phrase.
            self.forced_music=Some((choice.token,id.clone()));
            self.completed.remove("music");
            self.lane(audio,"music",Some(id),choice.gain,0.0,4.0);
            self.instant_music=false;
            return;
        }
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
                self.music_role=choice.role.clone();
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
        json!({"layer_episode":self.layers.episode,"layer_gains":{"texture":self.layer_gains.texture,"rhythm":self.layer_gains.rhythm,"anchor_held":self.layer_gains.anchor_held},"music_level":MUSIC_LEVEL,"atmosphere_level":ATMOSPHERE_LEVEL,"seed":self.director.seed(),"director":self.director.snapshot(),
        "program":self.program.snapshot(),"riser":self.riser.snapshot(),"riser_history":self.riser_history,"prepared_death":self.prepared_death.as_ref().map(|d|json!({"id":d.cue.id,"duration_s":d.duration})),"death":self.death.as_ref().map(|d|json!({"id":d.voice.id,"duration_s":d.duration,"frames":d.voice.control.frames.load(Ordering::Relaxed),"finished":d.voice.control.finished.load(Ordering::Relaxed)})),
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
    pub fn soundscape_seed(&self) -> u64 {
        self.soundscape.borrow().as_ref().map_or(0, |s| s.director.seed())
    }
    /// Select and decode once at fatal entry, without submitting any terminal audio.
    /// Its duration preserves the memorial's existing midpoint reveal delay.
    pub fn prepare_death_score(&self) -> Option<f32> {
        if self.capture.is_none() && self.live_mixer.is_none() { return None; }
        let mut state = self.soundscape.borrow_mut();
        let s = state.get_or_insert_with(|| Runtime::new(fresh_seed()));
        if let Some(d) = &s.death { return Some(d.duration); }
        if s.death_attempted { return s.prepared_death.as_ref().map(|d| d.duration); }
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
        s.prepared_death = Some(PreparedDeath { cue, pcm, rack, tail_only_s, duration });
        Some(duration)
    }
    /// The first presented memorial-text frame submits this finite cue exactly once.
    /// Only this explicitly terminal source survives the battlefield listener/KIA cut.
    pub fn start_death_score(&self) -> Option<f32> {
        self.prepare_death_score()?;
        let mut state = self.soundscape.borrow_mut();
        let s = state.as_mut()?;
        if let Some(d) = &s.death { return Some(d.duration); }
        let PreparedDeath { cue, pcm, rack, tail_only_s, duration } = s.prepared_death.take()?;
        let control = Arc::new(Control::new(cue.gain, cue.fade_in_s.unwrap()));
        put(&control.master, self.master_volume);
        let (sink, source) = Sink::new_idle();
        self.terminal_mixer.add(source);
        sink.append(BedSource::new(pcm, &cue, Arc::clone(&control), rack).with_tail_reverb(tail_only_s));
        if s.paused { sink.pause(); }
        s.death = Some(DeathScore { duration, finished_observed_frame: std::cell::Cell::new(None),
            voice: Voice { id: cue.id, lane: "death".into(), sink, control,
                gain: cue.gain, music: true, retiring: false, looping: false } });
        Some(duration)
    }
    pub fn death_score_elapsed(&self) -> Option<f32> {
        self.soundscape.borrow().as_ref()?.death.as_ref().map(|d|
            d.voice.control.frames.load(Ordering::Relaxed) as f32 / RATE as f32)
    }
    pub fn death_score_finished(&self) -> bool {
        let state = self.soundscape.borrow();
        let Some(s) = state.as_ref() else { return true; };
        let Some(d) = s.death.as_ref() else { return s.prepared_death.is_none(); };
        if !d.voice.control.finished.load(Ordering::Relaxed) { return false; }
        let now = self.master.frames();
        let first = d.finished_observed_frame.get().unwrap_or_else(|| {
            d.finished_observed_frame.set(Some(now)); now
        });
        now.saturating_sub(first) >= crate::reproduction::MASTER_LATENCY_FRAMES as u64
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
    fn transition_fixture() -> (Audio,Runtime,crate::soundscape_program::Choice) {
        let audio=Audio::new_capture();let mut r=runtime_fixture();
        for (id,role) in [("calm","explore"),("intense","threat"),("rise","stinger")] {
            let mut c=cue();c.id=id.into();c.role=role.into();c.bpm=Some(120.0);
            c.musical_key=library::MusicalKey::parse("C minor");c.fade_in_s=Some(0.01);
            c.looping=id!="rise";r.library.cues.insert(id.into(),c);
            r.insert(id.into(),Pcm{samples:vec![0.01;RATE as usize*2*12].into()});
        }
        r.library.events.insert("intensity_transition_riser".into(),vec!["rise".into()]);
        r.riser_cues.insert("rise".into(),RiserCue{peak_s:4.0,lead_s:4.0});
        let choice=crate::soundscape_program::Choice{role:Some("threat".into()),token:2,gain:0.7};
        let seed=(1..1000).find(|seed|RiserPlanner::new(*seed)
            .update(0.05,Some("explore"),&choice,false,4.0).is_some()).unwrap();
        r.riser=RiserPlanner::new(seed);r.music_role=Some("explore".into());
        r.lane(&audio,"music",Some("calm".into()),0.5,0.0,0.01);
        // Bridge tests observe an eligible phrase after its minimum dwell, not a retained intro.
        r.time=121.0;
        (audio,r,choice)
    }
    #[test]
    fn musical_bridge_reserves_target_and_starts_it_at_riser_bar_boundary() {
        let (audio,mut r,choice)=transition_fixture();
        audio.render_capture_frames(RATE as usize/2).unwrap();
        let before=r.voices.iter().find(|v|v.lane=="music").unwrap().control.frames.load(Ordering::Relaxed);
        r.update_music(&audio,0.05,crate::soundscape_director::Mode::Explore,&choice,Some("intense".into()));
        assert!(r.music_bridge.is_some());
        let delay=r.music_delay_frames;
        assert_eq!((before+delay)%(RATE as u64*2),0,"outgoing120BPM four-beat bar");
        let pending=r.voices.iter().find(|v|v.lane=="music_pending").unwrap().control.clone();
        audio.render_capture_frames(delay as usize-1024).unwrap();
        assert_eq!(pending.frames.load(Ordering::Relaxed),0,"target waits for the sample-clock boundary");
        audio.render_capture_frames(2048).unwrap();
        assert!(pending.frames.load(Ordering::Relaxed)>0);
        r.update_music(&audio,delay as f32/RATE as f32+0.1,crate::soundscape_director::Mode::Explore,&choice,Some("intense".into()));
        assert!(r.music_bridge.is_none());assert_eq!(r.music_role.as_deref(),Some("threat"));
    }
    #[test]
    fn musical_bridge_cancels_pending_track_and_main_gun_keeps_immediate_cut() {
        let (audio,mut r,choice)=transition_fixture();
        r.update_music(&audio,0.05,crate::soundscape_director::Mode::Explore,&choice,Some("intense".into()));
        let pending=r.voices.iter().find(|v|v.lane=="music_pending").unwrap().control.clone();
        r.main_gun_fired(&audio);
        assert!(r.music_bridge.is_none());assert!(r.riser.active_key().is_none());
        audio.render_capture_frames(RATE as usize*6).unwrap();
        assert_eq!(pending.frames.load(Ordering::Relaxed),0,"canceled delayed track must never play");
        assert!(r.voices.iter().any(|v|v.lane=="music"&&!v.retiring));
    }
    #[test]
    fn unknown_key_bridge_leaves_space_between_melodic_beds() {
        let (audio,mut r,choice)=transition_fixture();
        r.library.cues.get_mut("calm").unwrap().musical_key=None;
        r.library.cues.get_mut("intense").unwrap().bpm=None;
        r.update_music(&audio,0.05,crate::soundscape_director::Mode::Explore,&choice,Some("intense".into()));
        let old=r.voices.iter().find(|v|v.id=="calm").unwrap();
        let riser=r.voices.iter().find(|v|v.id=="rise").unwrap();
        assert!(old.control.retire_at.load(Ordering::Relaxed)+RATE as u64/4<r.music_delay_frames);
        assert_eq!(riser.control.retire_at.load(Ordering::Relaxed),r.music_delay_frames);
        assert_eq!(r.riser_history.back().unwrap()["harmonic_and_tempo_overlap"],false);
    }
    #[test]
    fn canceled_bridge_restores_the_silenced_loop_without_restarting_its_phase() {
        let (audio,mut r,choice)=transition_fixture();
        r.library.cues.get_mut("calm").unwrap().musical_key=None;
        r.update_music(&audio,0.05,crate::soundscape_director::Mode::Explore,&choice,Some("intense".into()));
        let old=r.music_bridge.as_ref().unwrap().old.as_ref().unwrap().clone();
        let muted_at=old.retire_at.load(Ordering::Relaxed)+RATE as u64/2;
        audio.render_capture_frames(muted_at as usize).unwrap();r.poll();
        assert!(!old.finished.load(Ordering::Relaxed));
        let before=old.frames.load(Ordering::Relaxed);
        r.riser.interrupt();r.cancel_music_bridge();
        audio.render_capture_frames(RATE as usize/2).unwrap();
        assert!(old.frames.load(Ordering::Relaxed)>before);
        assert!(r.voices.iter().any(|v|v.id=="calm"&&!v.retiring));
        assert!(!old.retire.load(Ordering::Relaxed));
    }
    #[test]
    fn changed_target_under_same_program_token_cancels_the_queued_cue() {
        let (audio,mut r,choice)=transition_fixture();
        r.update_music(&audio,0.05,crate::soundscape_director::Mode::Explore,&choice,Some("intense".into()));
        let pending=r.voices.iter().find(|v|v.lane=="music_pending").unwrap().control.clone();
        r.update_music(&audio,0.05,crate::soundscape_director::Mode::Explore,&choice,Some("calm".into()));
        audio.render_capture_frames(RATE as usize*6).unwrap();
        assert!(r.music_bridge.is_none());assert_eq!(pending.frames.load(Ordering::Relaxed),0);
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
            musical_key: None,
            beats_per_bar: 4,
            cooldown_s: 1.0,
            source_name: String::new(),
            duration_s: 0.0,
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
    #[test]
    fn death_preparation_is_silent_and_text_start_is_once_with_full_output_drain() {
        let audio = Audio::new_capture();
        let mut r = runtime_fixture();
        r.library.events.insert("death".into(), vec!["event_a".into()]);
        audio.soundscape.replace(Some(r));
        let duration = audio.prepare_death_score().unwrap();
        assert_eq!(audio.prepare_death_score(), Some(duration));
        assert_eq!(audio.death_score_elapsed(), None);
        assert!(!audio.death_score_finished(), "prepared is not played or finished");
        audio.set_silenced(true);
        audio.set_listener_level(0.0);
        let waiting = audio.render_capture_frames(RATE as usize * 2).unwrap();
        assert!(waiting.iter().all(|x| *x == 0.0), "preparing must not submit the chord");
        assert_eq!(audio.start_death_score(), Some(duration));
        let first = audio.render_capture_frames(RATE as usize / 2).unwrap();
        assert!(first.iter().any(|x| x.abs() > 1e-5), "terminal source survives only the world gate");
        let cursor = audio.death_score_elapsed();
        assert_eq!(audio.start_death_score(), Some(duration));
        assert_eq!(audio.death_score_elapsed(), cursor, "repeated text frame must not restart");
        audio.render_capture_frames(RATE as usize).unwrap();
        assert!(!audio.death_score_finished(), "retain the shared output lookahead after source end");
        audio.render_capture_frames(crate::reproduction::MASTER_LATENCY_FRAMES).unwrap();
        assert!(audio.death_score_finished());
        assert!((audio.death_score_elapsed().unwrap() - duration).abs() < 2.0 / RATE as f32);
        assert_eq!(audio.start_death_score(), Some(duration));
        assert!(audio.render_capture_frames(256).unwrap().iter().all(|x| x.abs() < 1e-7));
    }
    #[test]
    fn gun_override_rechecks_context_even_when_program_token_stays_the_same() {
        let mut r=runtime_fixture();
        let mut c=cue();c.id="night_threat".into();c.role="threat".into();
        c.contexts=vec!["night".into(),"inside".into()];r.library.cues.insert(c.id.clone(),c);
        r.forced_music=Some((7,"night_threat".into()));
        assert_eq!(r.eligible_music_override(7,Some("threat"),"map|night|inside"),Some("night_threat".into()));
        assert_eq!(r.eligible_music_override(7,Some("threat"),"map|day|inside"),None);
        assert!(r.forced_music.is_none());
        r.forced_music=Some((7,"night_threat".into()));
        assert_eq!(r.eligible_music_override(7,Some("threat"),"map|night|exterior"),None);
        r.forced_music=Some((7,"night_threat".into()));
        assert_eq!(r.eligible_music_override(8,Some("threat"),"map|night|inside"),None);
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
    fn compatible_combat_layers_mix_without_restarting_the_foreground() {
        let audio=Audio::new_capture();let mut r=runtime_fixture();
        for (id,path,key) in [("anchor","tonal_loop.wav",Some("F# minor")),
            ("texture","tonal_texture_loop.wav",Some("F# minor")),
            ("rhythm","drum_loop_120.wav",None)] {
            let mut c=cue();c.id=id.into();c.path=path.into();c.role="combat".into();
            c.bpm=Some(120.0);c.musical_key=key.and_then(crate::sound_library::MusicalKey::parse);
            r.library.cues.insert(id.into(),c);
            r.insert(id.into(),Pcm{samples:vec![0.1;RATE as usize*2].into()});
        }
        r.lane(&audio,"music",Some("anchor".into()),0.8,0.0,0.01);
        let anchor=r.voices.iter().find(|v|v.lane=="music").unwrap().control.clone();
        r.layer_gains=r.layers.update(8.0,true);r.update_score_layers(&audio,"");
        assert!(r.voices.iter().any(|v|v.lane=="score_layer_texture"&&v.id=="texture"));
        assert!(r.voices.iter().any(|v|v.lane=="score_layer_rhythm"&&v.id=="rhythm"));
        let start=anchor.frames.load(Ordering::Relaxed);audio.render_capture_frames(4410).unwrap();
        assert!(anchor.frames.load(Ordering::Relaxed)>start);assert!(!anchor.retire.load(Ordering::Relaxed));
        let music=r.voices.iter().find(|v|v.lane=="music").unwrap();
        assert!((get(&music.control.gain)-0.2).abs()<1e-6,"score source is quarter gain");
    }
    #[test]
    fn finite_cues_and_key_conflicts_cannot_enter_loop_layers() {
        let mut anchor=cue();anchor.id="anchor".into();anchor.bpm=Some(120.0);
        anchor.musical_key=crate::sound_library::MusicalKey::parse("F# minor");
        let mut next=anchor.clone();next.id="other".into();next.looping=false;
        assert!(!layers::compatible(&anchor,&next));next.looping=true;
        next.musical_key=crate::sound_library::MusicalKey::parse("C minor");
        assert!(!layers::compatible(&anchor,&next));
        next.musical_key=None;next.path="drum_loop_120.wav".into();
        assert!(layers::compatible(&anchor,&next));next.bpm=Some(97.0);
        assert!(!layers::compatible(&anchor,&next));
    }
    #[test]
    fn long_release_is_not_truncated_to_thirty_seconds() {
        let c=cue();let control=Arc::new(Control::new(1.0,0.001));
        let mut source=BedSource::new(Pcm{samples:vec![0.1;RATE as usize*2].into()},&c,control.clone(),RackConfig::default());
        for _ in 0..1000 {source.next();}control.fade_out(90.0);
        for _ in 0..32 {source.next();}assert!(source.remaining>RATE as u64*89);
    }
    #[test]
    fn main_gun_keeps_the_existing_loop_and_leaves_environment_tracks_running() {
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
        assert_eq!(music.id,"suspense_a");
        assert_eq!(r.program.update(0.0,crate::soundscape_director::Mode::Explore).role.as_deref(),Some("threat"));
        a.render_capture_frames(4410).unwrap();
        assert!(music.control.frames.load(Ordering::Relaxed)>4000,"existing sample clock continues across the shot");
        let bed=r.voices.iter().find(|v|v.lane=="night").unwrap();
        assert!(!bed.retiring && bed.control.frames.load(Ordering::Relaxed)>bed_before);
    }
    #[test]
    fn repeated_main_gun_fire_keeps_the_playing_music_source_and_ambient_beds() {
        let a=Audio::new_capture(); let mut r=runtime_fixture();
        for id in ["suspense_a","suspense_b"] {
            let mut c=cue(); c.id=id.into(); c.role="threat".into();
            r.library.cues.insert(id.into(),c);
            r.insert(id.into(),Pcm{samples:vec![0.1; RATE as usize*4].into()});
        }
        r.update(&a,0.1,input(true,7,true,false));
        r.main_gun_fired(&a);
        let control=r.voices.iter().find(|v|v.lane=="music"&&!v.retiring).unwrap().control.clone();
        let token=r.program.current_token();
        for _ in 0..5 {
            r.update(&a,8.0,input(true,7,true,false));
            r.main_gun_fired(&a);
            assert_eq!(r.program.current_token(),token);
            assert!(r.voices.iter().any(|v|v.lane=="music"&&!v.retiring&&Arc::ptr_eq(&v.control,&control)));
        }
        assert!(r.voices.iter().any(|v|v.lane=="night"&&!v.retiring));
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
    /// Looping score cues named and stored like the imported library: hashed playback clips,
    /// authored labels only in `source_name`.
    fn cadence_fixture() -> Runtime {
        let mut r = runtime_fixture();
        let mut add = |id: &str, role: &str, source: &str, contexts: &[&str], looping: bool| {
            let mut c = cue();
            c.id = id.into();
            c.role = role.into();
            c.path = format!("clips/audio_{id}.wav").into();
            c.source_name = source.into();
            c.contexts = contexts.iter().map(|x| (*x).to_owned()).collect();
            c.looping = looping;
            c.duration_s = if looping { 8.0 } else { 1.0 };
            c.fade_in_s = Some(0.05);
            r.library.cues.insert(id.into(), c);
            let frames = if looping { RATE as usize } else { RATE as usize / 5 };
            r.insert(id.into(), Pcm { samples: vec![0.01; frames * 2].into() });
        };
        for i in 0..3 { add(&format!("explore_{i}"), "explore", "OE_Atmos_FX_01_C.wav", &[], true); }
        for i in 0..2 { add(&format!("music_{i}"), "music", "ambient_keys.wav", &[], true); }
        for i in 0..2 { add(&format!("threat_{i}"), "threat", "Disturbed_Storm_Voices.wav", &[], true); }
        add("combat_base", "combat", "combat_bass.wav", &["combat"], true);
        add("combat_inside", "combat", "combat_inside_tank.wav", &["inside", "combat"], true);
        add("combat_drums", "combat", "oe_drm110_dusted_drum_loop.wav", &[], true);
        add("driving_loop", "driving", "vibey_driving.wav", &["driving"], true);
        add("music_hit", "music", "melodic_hit.wav", &[], false);
        add("combat_hit", "combat", "kick.wav", &["combat"], false);
        add("threat_hit", "threat", "fx_hit.wav", &[], false);
        add("day_a", "day", "wind_a.wav", &[], true);
        add("day_b", "day", "wind_b.wav", &[], true);
        r
    }
    fn foreground(r: &Runtime) -> Option<String> {
        r.voices.iter().find(|v| v.lane == "music" && !v.retiring).map(|v| v.id.clone())
    }
    #[test]
    fn live_cadence_changes_the_foreground_at_most_once_per_retention_window() {
        let a = Audio::new_capture();
        let mut r = cadence_fixture();
        let dt = 0.25f32;
        let mut starts: Vec<(f32, String)> = Vec::new();
        let mut previous: Option<String> = None;
        let mut beds: Option<(Option<String>, Option<String>)> = None;
        let mut held: Option<String> = None;
        let mut layered = false;
        for tick in 0..3600u32 {
            let t = tick as f32 * dt;
            // Stop/start driving every 40 s, hatch every 15 s, the gun every 10 s for five
            // minutes, and one 0.25 s incoming-contact pulse every 5 s for two minutes.
            let firing = (200.0..500.0).contains(&t);
            let contact = (300.0..420.0).contains(&t) && tick % 20 == 0;
            if firing && tick % 40 == 0 { r.main_gun_fired(&a); }
            let mut i = input(false, 7 + (t / 450.0) as i64, (t / 15.0) as u32 % 2 == 0, contact);
            i.context.speed_mps = if (t / 40.0) as u32 % 2 == 0 { 5.0 } else { 0.0 };
            r.update(&a, dt, i);
            // A fifth of real time is enough to finish fades between the permitted changes.
            a.render_capture_frames(RATE as usize / 20).unwrap();
            let now = foreground(&r);
            if let Some(id) = &now {
                assert!(r.library.cues[id].looping, "one-shot {id} became the foreground at {t}s");
                if previous.as_ref() != Some(id) { starts.push((t, id.clone())); }
            }
            if (300.0..590.0).contains(&t) {
                // From the first contact until the 90 s hold and its release have run out.
                if held.is_none() { held = now.clone(); }
                if held.is_some() { assert_eq!(now, held, "contact swapped the foreground at {t}s"); }
                layered |= r.voices.iter().any(|v| v.lane == "score_layer_rhythm" && !v.retiring);
            }
            let chosen = (r.selections["cabin"].1.clone(), r.selections["day"].1.clone());
            assert_eq!(beds.get_or_insert_with(|| chosen.clone()), &chosen, "bed re-rolled at {t}s");
            previous = now;
        }
        assert!(starts.len() >= 2, "the programme must still move on: {starts:?}");
        for pair in starts.windows(2) {
            assert!(pair[1].0 - pair[0].0 >= FOREGROUND_RETENTION_S - dt,
                "foreground changed twice inside one retention window: {starts:?}");
        }
        assert!(held.is_some() && layered, "contact must add a rhythm layer to the held loop: {starts:?}");
        assert_eq!(r.voices.iter().filter(|v| v.lane == "cabin").count(), 1);
        assert_eq!(r.voices.iter().filter(|v| v.lane == "day").count(), 1);
    }
    #[test]
    fn hashed_playback_paths_keep_authored_percussion_and_atmosphere_labels() {
        let m = json!({"version":1,"assets":[
            {"id":"drums","default_enabled":true,"role":"combat","bpm":110,
             "source_path":"tense_combat/oe_drm110_dusted_drum_loop.wav",
             "playback_path":"clips/audio_0a1b2c3d4e5f.wav","loop":{"enabled":true},
             "format":{"duration_s":4.4}},
            {"id":"bed","default_enabled":true,"role":"explore",
             "source_path":"calm_ambiance/OE_Atmos_FX_01_C.wav",
             "playback_path":"clips/audio_9f8e7d6c5b4a.wav","loop":{"enabled":true},
             "format":{"duration_s":61.9}},
            {"id":"hit","default_enabled":true,"role":"music","source_path":"music/melodic_hit_C.wav",
             "playback_path":"clips/audio_112233445566.wav","loop":{"enabled":false},
             "format":{"duration_s":1.7}}]});
        let lib = Library::from_json(std::path::Path::new("."), &m, &json!({})).unwrap();
        assert!(lib.errors.is_empty(), "{:?}", lib.errors);
        assert_eq!(lib.cues["drums"].source_name, "oe_drm110_dusted_drum_loop.wav");
        assert!(layers::explicit_percussion(&lib.cues["drums"]));
        assert!(!layers::explicit_percussion(&lib.cues["bed"]));
        assert!(layers::compatible(&lib.cues["bed"], &lib.cues["drums"]),
            "declared atmosphere keeps its bed under percussion");
        assert!(lib.cues["bed"].foreground() && lib.cues["drums"].foreground());
        assert!(!lib.cues["hit"].foreground());
        // The same library through the real mixer admission.
        let audio = Audio::new_capture();
        let mut r = runtime_fixture();
        r.library = lib;
        for id in ["drums", "bed"] {
            r.insert(id.into(), Pcm { samples: vec![0.05; RATE as usize * 2].into() });
        }
        r.lane(&audio, "music", Some("bed".into()), 0.8, 0.0, 0.01);
        r.layer_gains = r.layers.update(8.0, true);
        r.update_score_layers(&audio, "");
        assert!(r.voices.iter().any(|v| v.lane == "score_layer_rhythm" && v.id == "drums"));
    }
    #[test]
    fn foreground_selection_skips_one_shots_and_ignores_context_flips_within_a_token() {
        let mut r = cadence_fixture();
        for token in 0..40 {
            let id = r.select("music", &format!("{token}:map"), "music", "map|winter|explore|day|inside|clear");
            assert!(id.is_some_and(|id| id != "music_hit"));
        }
        // The only driving cue needs the `driving` tag; a stop/start must not flap the choice.
        assert_eq!(r.select("music", "50:map", "driving", "map|winter|explore|day|inside|clear"), None);
        assert_eq!(r.select("music", "50:map", "driving", "map|winter|driving|day|inside|clear"), None);
        assert_eq!(r.select("music", "51:map", "driving", "map|winter|driving|day|inside|clear").as_deref(),
            Some("driving_loop"));
        assert_eq!(r.select("music", "51:map", "driving", "map|winter|explore|day|inside|clear").as_deref(),
            Some("driving_loop"));
        // Hatch and combat-tag changes move cues in and out of the combat pool.
        let combat = r.select("music", "52:map", "combat", "map|winter|combat|day|inside|clear");
        assert!(combat.as_deref().is_some_and(|id| id != "combat_hit"));
        assert_eq!(r.select("music", "52:map", "combat", "map|winter|combat|day|exterior|clear"), combat);
        assert_eq!(r.select("music", "52:map", "combat", "map|winter|aftermath|day|exterior|clear"), combat);
    }
    #[test]
    fn main_gun_keeps_an_old_loop_of_another_role_for_the_whole_token() {
        let a = Audio::new_capture();
        let mut r = cadence_fixture();
        r.update(&a, 0.25, input(false, 7, true, false));
        r.lane(&a, "music", Some("explore_0".into()), 0.5, 0.0, 0.05);
        let control = r.voices.iter().find(|v| v.lane == "music").unwrap().control.clone();
        // Well past the minimum dwell: only the shot's own retention can keep this loop.
        r.time += 500.0;
        r.main_gun_fired(&a);
        assert_eq!(r.program.update(0.0, crate::soundscape_director::Mode::Explore).role.as_deref(), Some("threat"));
        for _ in 0..80 {
            r.update(&a, 0.25, input(false, 7, true, false));
            a.render_capture_frames(RATE as usize / 20).unwrap();
            assert!(r.voices.iter().any(|v| v.lane == "music" && !v.retiring && Arc::ptr_eq(&v.control, &control)),
                "the shot replaced an explore loop");
        }
        assert!(!control.retire.load(Ordering::Relaxed));
    }
    #[test]
    fn opening_piece_survives_the_first_shot_and_first_incoming_contact() {
        let a = Audio::new_capture();
        let mut r = cadence_fixture();
        let mut c = cue();
        c.id = "opening".into(); c.role = "intro".into(); c.looping = false; c.duration_s = 40.0;
        r.library.cues.insert(c.id.clone(), c);
        r.insert("opening".into(), Pcm { samples: vec![0.01; RATE as usize * 2 * 6].into() });
        r.update(&a, 0.25, input(false, 7, true, false));
        let control = r.voices.iter().find(|v| v.lane == "music_intro").unwrap().control.clone();
        r.main_gun_fired(&a);
        for tick in 0..16 {
            r.update(&a, 0.25, input(false, 7, true, tick % 4 == 0));
            a.render_capture_frames(RATE as usize / 4).unwrap();
            assert!(!control.retire.load(Ordering::Relaxed), "opening was cut at tick {tick}");
            assert!(foreground(&r).is_none(), "a second piece started over the opening");
        }
        // It ends by itself, and only then does the programme's foreground enter.
        for _ in 0..40 {
            a.render_capture_frames(RATE as usize / 4).unwrap();
            r.update(&a, 0.25, input(false, 7, true, false));
        }
        assert!(r.intro_done && control.finished.load(Ordering::Relaxed));
        assert!(!control.retire.load(Ordering::Relaxed));
    }
    #[test]
    fn bridged_foreground_starts_its_own_minimum_dwell() {
        let (audio,mut r,choice)=transition_fixture();
        r.update_music(&audio,0.05,crate::soundscape_director::Mode::Explore,&choice,Some("intense".into()));
        assert!(r.music_bridge.is_some());
        let wait=r.music_delay_frames as f32/RATE as f32+0.1;
        r.time+=wait;
        r.update_music(&audio,wait,crate::soundscape_director::Mode::Explore,&choice,Some("intense".into()));
        assert!(r.music_bridge.is_none());
        assert_eq!(foreground(&r).as_deref(),Some("intense"));
        assert_eq!(r.foreground_since,r.time);
        // A new programme request moments later cannot replace the bridged passage.
        r.time+=5.0;
        let next=crate::soundscape_program::Choice{role:Some("explore".into()),token:3,gain:0.6};
        r.update_music(&audio,5.0,crate::soundscape_director::Mode::Explore,&next,Some("calm".into()));
        assert_eq!(foreground(&r).as_deref(),Some("intense"));
    }
    #[test]
    fn cabin_and_day_beds_survive_a_retry_and_the_day_bed_ignores_local_noon() {
        let a = Audio::new_capture();
        let build = |seed: u64| {
            let mut r = Runtime::new(seed);
            r.library = Library::default();
            for i in 0..8 {
                for role in ["cabin", "day"] {
                    let mut c = cue();
                    c.id = format!("retry_{role}_{i}"); c.role = role.into();
                    r.insert(c.id.clone(), Pcm { samples: vec![0.01; RATE as usize * 2].into() });
                    r.library.cues.insert(c.id.clone(), c);
                }
            }
            r
        };
        let mut first = build(1);
        first.update(&a, 0.25, input(false, 7, true, false));
        let beds = (first.selections["cabin"].1.clone(), first.selections["day"].1.clone());
        assert!(beds.0.is_some() && beds.1.is_some());
        // The night identity advances at local noon while the same day continues.
        first.update(&a, 0.25, input(false, 8, true, false));
        assert_eq!(first.selections["day"].1, beds.1);
        for seed in 2..12 {
            let mut retry = build(seed);
            retry.update(&a, 0.25, input(false, 7, true, false));
            assert_eq!((retry.selections["cabin"].1.clone(), retry.selections["day"].1.clone()), beds);
        }
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
