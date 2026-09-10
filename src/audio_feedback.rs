//! Event-bound review foley. Recorded replacements use exact game-private assets;
//! mechanism fallback voices keep a clean checkout audible without those assets.
use super::{Audio, Clip, Sfx};
use rodio::Source;
use std::{collections::HashMap, sync::Arc};

const SR: f32 = crate::acoustics::SAMPLE_RATE as f32;
const SELECTION_MANIFEST: &str = include_str!("../catalog/audio_feedback_20260905.json");
const PRIVATE_PARTITION: &str = "feedback_20260905";

/// Recognize only this independently pinned bank inside the older v1 root. The returned paths
/// extend a filesystem closure, never the v1 cue list. No prefix or untrusted receipt is ignored.
pub(super) fn verified_private_partition_files(
    bank_root: &std::path::Path,
) -> Result<std::collections::HashSet<String>, String> {
    verify_private_partition_against(bank_root, SELECTION_MANIFEST.as_bytes())
}

fn verify_private_partition_against(
    bank_root: &std::path::Path,
    manifest_bytes: &[u8],
) -> Result<std::collections::HashSet<String>, String> {
    use std::{collections::HashSet, fs};
    let partition = bank_root.join(PRIVATE_PARTITION);
    let metadata = match fs::symlink_metadata(&partition) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(error) => return Err(format!("cannot inspect feedback partition: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("feedback partition must be a directory without symlinks".into());
    }
    // Enumerate before reading: symlink files/directories and special entries are rejected too.
    let actual = super::collect_private_bank_files(&partition)?;
    let receipt = fs::read(partition.join(super::PIZZA_AUDIO_RECEIPT))
        .map_err(|error| format!("cannot read feedback partition receipt: {error}"))?;
    if receipt != manifest_bytes {
        return Err("feedback partition receipt differs from compiled selection".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(manifest_bytes)
        .map_err(|error| format!("invalid compiled feedback selection: {error}"))?;
    if manifest["schema"] != "kessel.audio-feedback" || manifest["version"] != 1
        || manifest["owner_private"] != true || manifest["license"] != "pizzadoggy-local-purchase"
    {
        return Err("unsupported compiled feedback partition policy".into());
    }
    let entries = manifest["entries"].as_array().filter(|entries| !entries.is_empty())
        .ok_or("feedback partition selection has no entries")?;
    let mut expected = HashSet::from([super::PIZZA_AUDIO_RECEIPT.to_owned()]);
    let mut keys = HashSet::new();
    let mut sources = Vec::new();
    for entry in entries {
        let key = entry["key"].as_str().filter(|key| super::safe_private_id(key))
            .ok_or("invalid feedback partition key")?;
        if !keys.insert(key) { return Err("duplicate feedback partition key".into()); }
        let digest = entry["sha256"].as_str().filter(|digest| super::is_lower_sha256(digest))
            .ok_or("invalid feedback partition source hash")?;
        let bytes = entry["bytes"].as_u64().filter(|bytes| *bytes > 0)
            .ok_or("invalid feedback partition source size")?;
        let file = format!("{digest}.ogg");
        let logical = format!("{}/{}/{PRIVATE_PARTITION}/{file}", super::SOUNDS, super::PIZZA_AUDIO_BANK_DIR);
        if entry["path"].as_str() != Some(logical.as_str()) {
            return Err("feedback partition source path differs from pinned bank path".into());
        }
        expected.insert(file.clone());
        sources.push((file, bytes, digest));
    }
    if actual != expected {
        return Err("feedback partition closure mismatch".into());
    }
    for (file, bytes, digest) in sources {
        let encoded = fs::read(partition.join(file))
            .map_err(|error| format!("cannot read feedback partition source: {error}"))?;
        if encoded.len() as u64 != bytes || super::sha256_hex(&encoded) != digest {
            return Err("feedback partition source bytes do not match compiled selection".into());
        }
    }
    Ok(expected.into_iter().map(|file| format!("{PRIVATE_PARTITION}/{file}")).collect())
}

#[derive(Default)]
pub(super) struct FeedbackBank(HashMap<String, Arc<[f32]>>, bool);

impl FeedbackBank {
    pub(super) fn load() -> Self {
        // The owner's explicit request makes this the normal local gameplay path. An explicit
        // zero still supports the public procedural fallback and controlled comparisons.
        if std::env::var("KESSEL_ENABLE_PIZZA_AUDIO").is_ok_and(|v| v == "0") {
            return Self::default();
        }
        let selection: serde_json::Value = serde_json::from_str(SELECTION_MANIFEST)
            .expect("checked audio selection JSON");
        let mut bank = HashMap::new();
        for entry in selection["entries"].as_array().expect("audio entries") {
            let path = entry["path"].as_str().expect("audio path");
            let Ok(bytes) = crate::resource::read(path) else { continue };
            let hash = super::sha256_hex(&bytes);
            if bytes.len() as u64 != entry["bytes"].as_u64().unwrap_or(0)
                || Some(hash.as_str()) != entry["sha256"].as_str()
            {
                eprintln!("[audio] feedback recording rejected: {path}");
                continue;
            }
            let Some(clip) = Clip::from_encoded_bytes(path, bytes) else { continue };
            let source = clip.world_mono();
            let value = |key: &str| entry[key].as_f64().expect("audio recipe number") as f32;
            let start = (value("start") * SR) as usize;
            let end = ((value("end") * SR) as usize).min(source.len());
            if end <= start { continue }
            let pitch = value("pitch").clamp(0.5, 2.0);
            let frames = ((end - start) as f32 / pitch) as usize;
            let mut mono: Vec<f32> = (0..frames).map(|i| {
                let p = start as f32 + i as f32 * pitch;
                let j = p as usize;
                let f = p - j as f32;
                source[j] * (1.0 - f) + source[(j + 1).min(end - 1)] * f
            }).collect();
            tone(&mut mono, value("lowpass_hz"), value("bass_gain"), value("gain"));
            // Short onset/tail ramps avoid splice clicks while retaining a contact's attack.
            let fade = (SR * 0.002) as usize;
            let len = mono.len();
            for (i, x) in mono.iter_mut().enumerate() {
                *x *= (i as f32 / fade as f32).min(1.0)
                    * ((len - 1 - i) as f32 / (fade * 4) as f32).min(1.0);
            }
            bank.insert(entry["key"].as_str().unwrap().to_owned(), Arc::from(mono));
        }
        eprintln!("[audio] {} gameplay feedback recordings loaded; video audition pending", bank.len());
        Self(bank, true)
    }

    fn get(&self, key: &str) -> Option<Arc<[f32]>> { self.0.get(key).cloned() }

    pub(super) fn contains(&self, key: &str) -> bool { self.0.contains_key(key) }

    /// Report the bank that passed verification at load, never current filesystem/env state.
    /// Recordings are identified by content hash and cue key; source paths stay out of status UI.
    pub(super) fn metadata(&self) -> serde_json::Value {
        let selection: serde_json::Value = serde_json::from_str(SELECTION_MANIFEST)
            .expect("checked audio selection JSON");
        let configured = selection["entries"].as_array().expect("audio entries");
        let mut loaded = Vec::new();
        let mut not_loaded = Vec::new();
        for entry in configured {
            let key = entry["key"].as_str().expect("audio key");
            if let Some(mono) = self.0.get(key) {
                loaded.push(serde_json::json!({
                    "key": key,
                    "source_sha256": entry["sha256"],
                    "source_bytes": entry["bytes"],
                    "source_verified_at_load": true,
                    "prepared_mono_frames": mono.len(),
                    "prepared_sample_rate": crate::acoustics::SAMPLE_RATE,
                    "prepared_pcm_fnv1a64": format!("{:016x}", super::buffer_checksum(mono)),
                }));
            } else {
                not_loaded.push(key);
            }
        }
        serde_json::json!({
            "schema": selection["schema"],
            "version": selection["version"],
            "selection_manifest_sha256": super::sha256_hex(SELECTION_MANIFEST.as_bytes()),
            "credit": "Pizza Doggy",
            "license_id": selection["license"],
            "owner_private": selection["owner_private"],
            "review": selection["review"],
            "enabled_at_load": self.1,
            "status": if !self.1 { "disabled" }
                else if loaded.is_empty() { "unavailable" }
                else if loaded.len() < configured.len() { "partial" }
                else { "loaded" },
            "configured_count": configured.len(),
            "loaded_count": loaded.len(),
            "loaded": loaded,
            "not_loaded_keys": not_loaded,
        })
    }
}

/// Two-pole top-end rolloff and a low shelf. No peak normalization: requested
/// level changes must survive, and the ordinary output mixer owns headroom.
pub(super) fn tone(mono: &mut [f32], cutoff: f32, bass: f32, gain: f32) {
    let a = 1.0 - (-std::f32::consts::TAU * cutoff / SR).exp();
    let b = 1.0 - (-std::f32::consts::TAU * 180.0 / SR).exp();
    let (mut low, mut low2, mut body) = (0.0, 0.0, 0.0);
    for x in mono {
        low += a * (*x - low);
        low2 += a * (low - low2);
        body += b * (low2 - body);
        *x = (low2 + (bass - 1.0) * body) * gain;
    }
}

pub(super) fn replacement_key(sfx: Sfx) -> Option<&'static str> {
    Some(match sfx {
        Sfx::LugerShot => "luger_shot",
        Sfx::RifleShot => "rifle_shot",
        Sfx::HammerStrike => "hammer_strike",
        Sfx::PlayerGunInt1 | Sfx::PlayerGunInt2 => "gun_interior",
        Sfx::PlayerGunExt1 | Sfx::PlayerGunExt2 | Sfx::PlayerGunExt3 | Sfx::PlayerGunExt4 => "gun_exterior",
        _ => return None,
    })
}

#[derive(Default)]
pub(super) struct Mechanisms {
    hatches: [Option<super::Sink>; 4], // cupola, two roof lids, and opposed Driver visor shields
    starter: Option<super::Sink>,
    jam: Option<super::Sink>,
    mg_trigger_held: [bool; 2],
}

impl Mechanisms {
    pub(super) fn set_paused(&self, paused: bool) {
        for sink in self.hatches.iter().flatten().chain(self.starter.iter()).chain(self.jam.iter()) {
            if paused { sink.pause(); } else { sink.play(); }
        }
    }

    /// These long-lived mechanism sinks are separate from the transient/engine/MG pools. Keep
    /// their existing phase and source while the same listener concussion gain changes around them.
    pub(super) fn set_master_volume(&mut self, gain: f32) {
        for sink in self.hatches.iter().flatten() { sink.set_volume(0.75 * gain); }
        if let Some(sink)=&self.starter { sink.set_volume(0.75 * gain); }
        if let Some(sink)=&self.jam { sink.set_volume(0.60 * gain); }
    }
}

fn failed_trigger_edge(held: &mut bool, pulled: bool, can_fire: bool) -> bool {
    let clicked = pulled && !*held && !can_fire;
    *held = pulled;
    clicked
}

fn stereo(mono: &[f32]) -> Vec<f32> { mono.iter().flat_map(|v| [*v, *v]).collect() }

/// Authored mechanism approximation where the pack has no identified matching recording.
/// Starter is a geared motor under cyclic compression, with no electrical zap or sub-bass boom.
fn fallback(key: &str) -> Arc<[f32]> {
    let duration = match key { "starter" => 1.8, "hatch_move" | "traverse_jam" => 1.0, _ => 0.22 };
    let mut mono = Vec::with_capacity((duration * SR) as usize);
    let mut phase = 0.0;
    for i in 0..(duration * SR) as usize {
        let t = i as f32 / SR;
        let x = match key {
            "starter" => {
                let crank = 7.0 * t + 0.5 * t * t;
                let load = 0.72 + 0.28 * (std::f32::consts::TAU * crank).cos();
                phase += std::f32::consts::TAU * (240.0 + 70.0 * t) * load / SR;
                0.12 * load * (phase.sin() + 0.32 * (phase * 3.0).sin())
                    * (t / 0.06).min(1.0) * ((duration - t) / 0.025).min(1.0)
            }
            "hatch_move" | "traverse_jam" => {
                let ratchet = (std::f32::consts::TAU * 19.0 * t).sin().max(0.0).powi(12);
                0.10 * ratchet * ((std::f32::consts::TAU * 270.0 * t).sin()
                    + 0.4 * (std::f32::consts::TAU * 611.0 * t).sin())
            }
            _ => {
                let f = if key == "trigger" { 630.0 } else { 135.0 };
                0.25 * (-t / 0.035).exp() * ((std::f32::consts::TAU * f * t).sin()
                    + 0.35 * (std::f32::consts::TAU * f * 2.71 * t).sin())
            }
        };
        mono.push(x);
    }
    Arc::from(mono)
}

impl Audio {
    /// One failed-trigger contact per deliberate pull. A working MG keeps its existing burst;
    /// a held disabled/overheated trigger must neither chatter nor repeatedly play dry clicks.
    pub fn update_mg_trigger(&self, pulled: bool, can_fire: bool, coax: bool) {
        let index = usize::from(coax);
        let clicked = {
            let mut mechanisms = self.mechanisms.borrow_mut();
            mechanisms.mg_trigger_held[1 - index] = false;
            failed_trigger_edge(&mut mechanisms.mg_trigger_held[index], pulled, can_fire)
        };
        if clicked { self.play_control("trigger", 0.8); }
    }

    pub(super) fn feedback_source(&self, key: &str) -> Option<Arc<[f32]>> {
        self.feedback_bank.get(key)
    }

    pub fn play_control(&self, key: &str, volume: f32) {
        let mono = self.feedback_bank.get(key).unwrap_or_else(|| fallback(key));
        self.play_stereo(stereo(&mono), volume);
    }

    pub(super) fn play_feedback_sfx(&self, sfx: Sfx, volume: f32) -> bool {
        let Some(mono) = replacement_key(sfx).and_then(|k| self.feedback_bank.get(k)) else { return false };
        self.play_stereo(stereo(&mono), volume);
        true
    }

    /// Called with measured animation phases, including reversals and remote hatch motion.
    pub fn update_hatch_sound(&self, index: usize, before: f32, after: f32) {
        let moving = (after - before).abs() > 1e-7;
        let at_stop = after <= 0.0 || after >= 1.0;
        let mut mechanisms = self.mechanisms.borrow_mut();
        let Some(slot) = mechanisms.hatches.get_mut(index) else { return };
        if moving && !at_stop && slot.is_none() {
            let mono = self.feedback_bank.get("hatch_move").unwrap_or_else(|| fallback("hatch_move"));
            if let Ok(sink) = self.create_sink() {
                sink.append(Clip::from_stereo_f32(stereo(&mono)).source().repeat_infinite());
                sink.set_volume(0.75 * self.master_volume);
                *slot = Some(sink);
            }
        }
        if !moving || at_stop {
            if let Some(sink) = slot.take() { sink.stop(); }
        }
        drop(mechanisms);
        if moving && at_stop { self.play_control("hatch_stop", if after == 0.0 { 0.95 } else { 0.8 }); }
    }

    pub fn set_starter(&self, cranking: bool) {
        let mut mechanisms = self.mechanisms.borrow_mut();
        if cranking && mechanisms.starter.is_none() {
            if let Ok(sink) = self.create_sink() {
                sink.append(Clip::from_stereo_f32(stereo(&fallback("starter"))).source());
                sink.set_volume(0.75 * self.master_volume);
                mechanisms.starter = Some(sink);
            }
        } else if !cranking {
            if let Some(sink) = mechanisms.starter.take() { sink.stop(); }
        }
    }

    pub fn set_traverse_jam(&self, straining: bool) {
        let mut mechanisms = self.mechanisms.borrow_mut();
        if straining && mechanisms.jam.is_none() {
            if let Ok(sink) = self.create_sink() {
                sink.append(Clip::from_stereo_f32(stereo(&fallback("traverse_jam"))).source().repeat_infinite());
                sink.set_volume(0.6 * self.master_volume);
                mechanisms.jam = Some(sink);
            }
        } else if !straining {
            if let Some(sink) = mechanisms.jam.take() { sink.stop(); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_mg_clicks_once_per_pull_without_changing_working_bursts() {
        let mut held = false;
        assert!(!failed_trigger_edge(&mut held, true, true));
        // A module failure while held does not pretend the finger pulled the trigger again.
        assert!(!failed_trigger_edge(&mut held, true, false));
        assert!(!failed_trigger_edge(&mut held, false, false));
        assert!(failed_trigger_edge(&mut held, true, false));
        for _ in 0..180 { assert!(!failed_trigger_edge(&mut held, true, false)); }
        assert!(!failed_trigger_edge(&mut held, false, false));
        assert!(failed_trigger_edge(&mut held, true, false));
        assert!(!failed_trigger_edge(&mut held, false, true));
        assert!(!failed_trigger_edge(&mut held, true, true));
    }

    struct PartitionFixture {
        root: std::path::PathBuf,
        manifest: Vec<u8>,
        filename: String,
        source: Vec<u8>,
    }

    impl PartitionFixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_nanos();
            let root = std::env::temp_dir().join(format!("kessel-feedback-partition-{}-{stamp}-{}",
                std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            std::fs::create_dir(&root).unwrap();
            let partition = root.join(PRIVATE_PARTITION);
            std::fs::create_dir(&partition).unwrap();
            let source = b"OggS independently pinned test bytes".to_vec();
            let hash = super::super::sha256_hex(&source);
            let filename = format!("{hash}.ogg");
            let manifest = serde_json::to_vec(&serde_json::json!({
                "schema": "kessel.audio-feedback", "version": 1,
                "owner_private": true, "license": "pizzadoggy-local-purchase",
                "entries": [{ "key": "trigger", "sha256": hash, "bytes": source.len(),
                    "path": format!("assets/sounds/pizza_private/{PRIVATE_PARTITION}/{filename}") }],
            })).unwrap();
            std::fs::write(partition.join(&filename), &source).unwrap();
            std::fs::write(partition.join("BANK-RECEIPT.json"), &manifest).unwrap();
            Self { root, manifest, filename, source }
        }

        fn partition(&self) -> std::path::PathBuf { self.root.join(PRIVATE_PARTITION) }
        fn verify(&self) -> Result<std::collections::HashSet<String>, String> {
            verify_private_partition_against(&self.root, &self.manifest)
        }
    }

    impl Drop for PartitionFixture {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
    }

    #[test]
    fn verified_partition_accepts_exact_files_and_remains_optional() {
        let fixture = PartitionFixture::new();
        let files = fixture.verify().unwrap();
        assert_eq!(files, std::collections::HashSet::from([
            format!("{PRIVATE_PARTITION}/BANK-RECEIPT.json"),
            format!("{PRIVATE_PARTITION}/{}", fixture.filename),
        ]));
        std::fs::remove_dir_all(fixture.partition()).unwrap();
        assert!(fixture.verify().unwrap().is_empty());
    }

    #[test]
    fn verified_partition_rejects_extra_nested_and_missing_files() {
        let fixture = PartitionFixture::new();
        let nested = fixture.partition().join("unreviewed");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("extra.ogg"), b"OggSextra").unwrap();
        assert!(fixture.verify().unwrap_err().contains("closure mismatch"));
        std::fs::remove_dir_all(nested).unwrap();
        std::fs::remove_file(fixture.partition().join(&fixture.filename)).unwrap();
        assert!(fixture.verify().unwrap_err().contains("closure mismatch"));
    }

    #[test]
    fn verified_partition_rejects_tampered_source_size_hash_and_receipt() {
        let fixture = PartitionFixture::new();
        let source = fixture.partition().join(&fixture.filename);
        let mut changed = fixture.source.clone();
        changed[0] ^= 1; // Same length still has to pass SHA-256, not merely a size check.
        std::fs::write(&source, &changed).unwrap();
        assert!(fixture.verify().unwrap_err().contains("source bytes"));
        std::fs::write(&source, &fixture.source[..fixture.source.len()-1]).unwrap();
        assert!(fixture.verify().unwrap_err().contains("source bytes"));
        std::fs::write(&source, &fixture.source).unwrap();
        let mut receipt = fixture.manifest.clone();
        receipt.push(b'\n');
        std::fs::write(fixture.partition().join("BANK-RECEIPT.json"), receipt).unwrap();
        assert!(fixture.verify().unwrap_err().contains("receipt differs"));
    }

    #[test]
    fn verified_partition_never_accepts_manifest_paths_outside_its_pinned_root() {
        let mut fixture = PartitionFixture::new();
        let mut manifest: serde_json::Value = serde_json::from_slice(&fixture.manifest).unwrap();
        manifest["entries"][0]["path"] = serde_json::json!("../unreviewed.ogg");
        fixture.manifest = serde_json::to_vec(&manifest).unwrap();
        std::fs::write(fixture.partition().join("BANK-RECEIPT.json"), &fixture.manifest).unwrap();
        assert!(fixture.verify().unwrap_err().contains("pinned bank path"));
    }

    #[test]
    #[cfg(unix)]
    fn verified_partition_rejects_file_directory_and_receipt_symlinks() {
        use std::{fs, os::unix::fs::symlink};
        let fixture = PartitionFixture::new();
        let outside = fixture.root.join("same-bytes.ogg");
        fs::write(&outside, &fixture.source).unwrap();
        let source = fixture.partition().join(&fixture.filename);
        fs::remove_file(&source).unwrap();
        symlink(&outside, &source).unwrap();
        assert!(fixture.verify().unwrap_err().contains("symlink"));
        fs::remove_file(&source).unwrap();
        fs::write(&source, &fixture.source).unwrap();
        let receipt = fixture.partition().join("BANK-RECEIPT.json");
        let outside_receipt = fixture.root.join("same-receipt.json");
        fs::write(&outside_receipt, &fixture.manifest).unwrap();
        fs::remove_file(&receipt).unwrap();
        symlink(outside_receipt, &receipt).unwrap();
        assert!(fixture.verify().unwrap_err().contains("symlink"));
        fs::remove_file(&receipt).unwrap();
        fs::write(&receipt, &fixture.manifest).unwrap();
        let saved = fixture.root.join("saved");
        fs::rename(fixture.partition(), &saved).unwrap();
        symlink(saved, fixture.partition()).unwrap();
        assert!(fixture.verify().unwrap_err().contains("symlink"));
    }

    #[test]
    fn feedback_metadata_reports_disabled_missing_partial_and_loaded_without_source_paths() {
        let disabled = FeedbackBank::default().metadata();
        assert_eq!(disabled["status"], "disabled");
        assert_eq!(disabled["loaded_count"], 0);
        let missing = FeedbackBank(HashMap::new(), true).metadata();
        assert_eq!(missing["status"], "unavailable");
        assert_eq!(missing["enabled_at_load"], true);

        // Report fixtures stand in for sources that have passed the existing load-time checks.
        // No filesystem or process environment changes are required to test reporting branches.
        let mono: Arc<[f32]> = Arc::from(vec![0.25, -0.5, 0.125]);
        let mut bank = FeedbackBank(HashMap::from([("rifle_shot".to_owned(), mono.clone())]), true);
        let partial = bank.metadata();
        assert_eq!(partial["status"], "partial");
        assert_eq!(partial["loaded_count"], 1);
        assert_eq!(partial["loaded"][0]["key"], "rifle_shot");
        assert_eq!(partial["loaded"][0]["source_sha256"],
            "8f537a47c60ea37b5a8c8d726cc4e0520b7d48424173049e9b39a5da81958d9d");
        assert_eq!(partial["loaded"][0]["prepared_pcm_fnv1a64"],
            format!("{:016x}", super::super::buffer_checksum(&mono)));
        assert_eq!(partial["selection_manifest_sha256"],
            super::super::sha256_hex(SELECTION_MANIFEST.as_bytes()));
        let selection: serde_json::Value = serde_json::from_str(SELECTION_MANIFEST).unwrap();
        for entry in selection["entries"].as_array().unwrap() {
            bank.0.insert(entry["key"].as_str().unwrap().to_owned(), mono.clone());
        }
        let loaded = bank.metadata();
        assert_eq!(loaded["status"], "loaded");
        assert_eq!(loaded["loaded_count"], loaded["configured_count"]);
        assert_eq!(loaded["not_loaded_keys"], serde_json::json!([]));
        assert_eq!(&*mono, &[0.25, -0.5, 0.125]);
        assert_eq!(loaded, bank.metadata(), "reporting is deterministic and read-only");
        assert!(!loaded.to_string().contains("source_assets/"));
        assert!(!loaded.to_string().contains(".ogg"));
    }

    #[test]
    fn bank_metadata_reports_recorded_rifle_bypass_and_preserved_tank_mg_layer() {
        use super::super::{metadata, PizzaLayer};
        let mono: Arc<[f32]> = Arc::from(vec![0.25, -0.5]);
        let bank = FeedbackBank(HashMap::from([("rifle_shot".to_owned(), mono.clone())]), true);
        let mut layers = vec![Vec::new(); Sfx::ALL.len()];
        layers[Sfx::RifleShot.index()].push(PizzaLayer {
            clip: Clip::from_stereo_f32(vec![0.25, 0.25, -0.5, -0.5]),
            world_mono: mono, gain: 0.12,
        });
        let sources = metadata::LoadedSources { v1_requested: true, ..Default::default() };
        let report = metadata::bank_metadata(&bank, &sources, &layers);
        let routes = report["sfx_routes"].as_array().unwrap();
        let rifle = routes.iter().find(|r| r["sfx"] == "rifle_shot").unwrap();
        let tank_mg = routes.iter().find(|r| r["sfx"] == "tank_mg_shot").unwrap();
        assert_eq!(rifle["direct_source"], "verified_feedback_recording");
        assert_eq!(rifle["world_source"], "verified_feedback_recording");
        assert_eq!(rifle["v1_layer_variants_available"], 1);
        assert_eq!(rifle["v1_layer_variants_active"], 0);
        assert_eq!(tank_mg["world_source"], "procedural");
        assert_eq!(tank_mg["v1_layer_variants_active"], 1);
        assert_eq!(report["v1_layers"]["verified_loaded_count"], 1);
        assert!(metadata::startup_summary(&report).contains("1 verified feedback recordings (partial)"));
    }

    #[test]
    fn muffling_preserves_body_and_reduces_high_band() {
        let rms = |freq: f32| {
            let mut x: Vec<f32> = (0..44100).map(|i| (std::f32::consts::TAU * freq * i as f32 / SR).sin()).collect();
            tone(&mut x, 1800.0, 1.0, 1.0);
            (x[4096..].iter().map(|v| v*v).sum::<f32>() / (x.len()-4096) as f32).sqrt()
        };
        assert!(rms(100.0) > 0.69);
        assert!(rms(6000.0) < 0.09);
    }

    #[test]
    fn active_mechanisms_follow_listener_gain_without_restarting_their_motion() {
        let mut audio=Audio::new_capture();
        audio.update_hatch_sound(0,0.0,0.1);
        audio.set_starter(true);audio.set_traverse_jam(true);
        let created=audio.capture_metadata().unwrap()["sinks_created"].clone();
        let before=audio.render_capture_frames(735).unwrap();
        assert!(before.iter().any(|sample|sample.abs()>1e-5));
        for master in [0.25,0.0,1.0] {
            audio.set_master_volume(master);
            let mechanisms=audio.mechanisms.borrow();
            assert_eq!(mechanisms.hatches[0].as_ref().unwrap().volume(),0.75*master);
            assert_eq!(mechanisms.starter.as_ref().unwrap().volume(),0.75*master);
            assert_eq!(mechanisms.jam.as_ref().unwrap().volume(),0.60*master);
            drop(mechanisms);
            assert_eq!(audio.capture_metadata().unwrap()["sinks_created"],created,
                "listener gain must not recreate a hatch/crank or replay its attack");
        }
        audio.update_hatch_sound(0,0.1,0.1);audio.set_starter(false);audio.set_traverse_jam(false);
        let mechanisms=audio.mechanisms.borrow();
        assert!(mechanisms.hatches[0].is_none() && mechanisms.starter.is_none() && mechanisms.jam.is_none());
    }

    #[test]
    fn starter_stops_with_cancel_and_hatch_stops_only_on_arrival() {
        let audio = Audio::new_capture();
        audio.set_starter(true);
        assert!(audio.mechanisms.borrow().starter.is_some());
        audio.set_starter(false);
        assert!(audio.mechanisms.borrow().starter.is_none());
        audio.update_hatch_sound(0, 0.0, 0.1);
        assert!(audio.mechanisms.borrow().hatches[0].is_some());
        audio.update_hatch_sound(0, 0.1, 0.05); // reversing must not invent a hard stop
        assert!(audio.mechanisms.borrow().hatches[0].is_some());
        audio.update_hatch_sound(0, 0.05, 0.0);
        assert!(audio.mechanisms.borrow().hatches[0].is_none());
        let pcm = audio.render_capture_frames(11025).unwrap();
        assert!(pcm.iter().all(|v| v.is_finite()));
        assert!(pcm.iter().any(|v| v.abs() > 0.01));
    }
}
