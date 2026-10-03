//! Authored sound library. Originals are catalogued separately; runtime opens only imported,
//! relative playback paths. Decoding runs on a worker, never on the device callback.
use anyhow::{bail, ensure, Context as _, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub const RATE: u32 = 44_100;
pub const MAX_CLIP_BYTES: usize = 64 * 1024 * 1024;
const ROLES: &[&str] = &[
    "engine_hiss",
    "blizzard",
    "night",
    "day",
    "cabin",
    "scene",
    "explore",
    "driving",
    "threat",
    "combat",
    "aftermath",
    "stinger",
    "music",
    "intro",
    "effect",
];

#[derive(Clone, Debug)]
pub struct Cue {
    pub id: String,
    pub path: PathBuf,
    pub role: String,
    pub role_aliases: Vec<String>,
    /// Role -> OR alternatives; each alternative is an AND of required context tags.
    pub role_contexts: HashMap<String, Vec<Vec<String>>>,
    pub tags: Vec<String>,
    pub contexts: Vec<String>,
    pub gain: f32,
    pub crossfade_s: f32,
    pub equal_gain_seam: bool,
    pub fade_in_s: Option<f32>,
    pub fade_out_s: Option<f32>,
    pub looping: bool,
    pub bpm: Option<f32>,
    pub beats_per_bar: u32,
    pub cooldown_s: f32,
}
impl Cue {
    pub fn music(&self) -> bool {
        matches!(
            self.role.as_str(),
            "explore"
                | "driving"
                | "threat"
                | "combat"
                | "aftermath"
                | "stinger"
                | "music"
                | "intro"
        )
    }
}

#[derive(Clone, Debug)]
pub struct Zone {
    pub id: String,
    pub act_id: u8,
    pub center: [f32; 2],
    pub radius: f32,
    pub fade: f32,
    pub priority: i64,
    pub rack: Option<String>,
}
#[derive(Default)]
pub struct Library {
    pub cues: HashMap<String, Cue>,
    pub zones: Vec<Zone>,
    pub events: HashMap<String, Vec<String>>,
    pub errors: Vec<String>,
}
fn number(v: &Value, key: &str, fallback: f32, lo: f32, hi: f32) -> f32 {
    v[key]
        .as_f64()
        .map(|x| x as f32)
        .filter(|x| x.is_finite())
        .unwrap_or(fallback)
        .clamp(lo, hi)
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn relative(path: &str) -> Result<&Path> {
    let p = Path::new(path);
    ensure!(
        !p.as_os_str().is_empty() && p.components().all(|c| matches!(c, Component::Normal(_))),
        "playback path must stay inside library: {path}"
    );
    Ok(p)
}
impl Library {
    pub fn load() -> Result<Self> {
        let root = std::env::var_os("PANZER_SOUND_LIBRARY")
            .map(PathBuf::from)
            .unwrap_or(crate::resource::resolve("assets/audio/library")?);
        let manifest: Value = serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
        let config = match std::fs::read(root.join("soundscape.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
            Err(e) => return Err(e.into()),
        };
        Self::from_json(&root, &manifest, &config)
    }
    pub fn from_json(root: &Path, manifest: &Value, config: &Value) -> Result<Self> {
        ensure!(
            manifest["version"].as_u64() == Some(1),
            "unsupported sound library version"
        );
        let mut out = Self::default();
        let mut seen = HashSet::new();
        for asset in manifest["assets"]
            .as_array()
            .context("assets array missing")?
        {
            let Some(id) = asset["id"].as_str() else {
                continue;
            };
            let binding = config["cues"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|v| v["asset"].as_str() == Some(id));
            if !binding
                .and_then(|v| v["enabled"].as_bool())
                .unwrap_or(asset["default_enabled"].as_bool().unwrap_or(false))
            {
                continue;
            }
            // The catalogue is already deduplicated; reject accidental duplicate identities too.
            if !seen.insert(id.to_owned()) {
                out.errors.push(format!("duplicate cue {id}"));
                continue;
            }
            let result = (|| -> Result<Cue> {
                let path = relative(
                    asset["playback_path"]
                        .as_str()
                        .context("missing imported playback_path")?,
                )?;
                let b = binding.unwrap_or(&Value::Null);
                let role = b["role"]
                    .as_str()
                    .or(asset["role"].as_str())
                    .unwrap_or("effect")
                    .to_owned();
                ensure!(ROLES.contains(&role.as_str()), "unknown role {role}");
                let mut role_aliases = if b["role_aliases"].is_array() {
                    strings(&b["role_aliases"])
                } else {
                    strings(&asset["role_aliases"])
                };
                role_aliases.push(role.clone());
                role_aliases.sort();
                role_aliases.dedup();
                ensure!(
                    role_aliases.iter().all(|r| ROLES.contains(&r.as_str())),
                    "unknown alias role"
                );
                let contexts = if b["contexts"].is_array() {
                    strings(&b["contexts"])
                } else {
                    strings(&asset["contexts"])
                };
                let mut role_contexts = HashMap::new();
                for source in [&asset["role_contexts"], &b["role_contexts"]] {
                    if let Some(mapping) = source.as_object() {
                        for (alias, alternatives) in mapping {
                            ensure!(
                                ROLES.contains(&alias.as_str()),
                                "unknown context role {alias}"
                            );
                            let alternatives = alternatives
                                .as_array()
                                .context("role_contexts value must be an array of AND clauses")?;
                            let mut parsed = Vec::new();
                            for clause in alternatives {
                                let values = clause
                                    .as_array()
                                    .context("role_contexts clause must be an array")?;
                                ensure!(
                                    values.iter().all(|v| v.as_str().is_some()),
                                    "role_contexts tags must be strings"
                                );
                                let mut tags = strings(clause);
                                tags.sort();
                                tags.dedup();
                                if !parsed.contains(&tags) {
                                    parsed.push(tags);
                                }
                            }
                            role_contexts.insert(alias.clone(), parsed);
                        }
                    }
                }
                if b["contexts"].is_array() && b["role_contexts"].get(role.as_str()).is_none() {
                    role_contexts.insert(role.clone(), vec![contexts.clone()]);
                }
                let db = number(&asset["gain"], "suggested_db", 0.0, -60.0, 0.0)
                    + number(b, "gain_db", 0.0, -60.0, 12.0);
                let bpm = b["bpm"]
                    .as_f64()
                    .or(asset["bpm"].as_f64())
                    .map(|x| x as f32)
                    .filter(|x| x.is_finite() && (30.0..=300.0).contains(x));
                // Explicit authored repeat works for effects/stingers too. Terminal Death
                // playback enforces its own finite presentation without corrupting this cue.
                let looping = b["loop"]
                    .as_bool()
                    .or_else(|| asset["explicit_filename_loop"].as_bool().filter(|v| *v))
                    .or(asset["loop"]["enabled"].as_bool())
                    .unwrap_or(!matches!(role.as_str(), "effect" | "stinger" | "intro" | "engine_hiss"));
                let fade = |key: &str| {
                    b[key]
                        .as_f64()
                        .filter(|x| x.is_finite())
                        .map(|x| (x as f32).clamp(0.0, 30.0))
                };
                Ok(Cue {
                    id: id.into(),
                    path: root.join(path),
                    role,
                    role_aliases,
                    role_contexts,
                    tags: if b["tags"].is_array() {
                        strings(&b["tags"])
                    } else {
                        strings(&asset["tags"])
                    },
                    contexts,
                    gain: 10f32.powf(db / 20.0),
                    crossfade_s: number(
                        b,
                        "crossfade_s",
                        number(&asset["loop"], "crossfade_s", 2.0, 0.0, 8.0),
                        0.0,
                        8.0,
                    ),
                    equal_gain_seam: b["crossfade_curve"]
                        .as_str()
                        .or(asset["loop"]["crossfade_curve"].as_str())
                        == Some("equal_gain"),
                    fade_in_s: fade("fade_in_s"),
                    fade_out_s: fade("fade_out_s"),
                    looping,
                    bpm,
                    beats_per_bar: b["beats_per_bar"].as_u64().unwrap_or(4).clamp(1, 16) as u32,
                    cooldown_s: number(b, "cooldown_s", 3.0, 0.05, 3600.0),
                })
            })();
            match result {
                Ok(cue) => {
                    out.cues.insert(id.into(), cue);
                }
                Err(e) => out.errors.push(format!("{id}: {e}")),
            }
        }
        for z in config["locations"].as_array().into_iter().flatten() {
            if let (Some(id), Some(x), Some(y)) = (
                z["id"].as_str(),
                z["center"][0].as_f64(),
                z["center"][1].as_f64(),
            ) {
                if x.is_finite() && y.is_finite() {
                    out.zones.push(Zone {
                        id: id.into(),
                        act_id: z["act_id"].as_u64().unwrap_or(1).min(255) as u8,
                        center: [x as f32, y as f32],
                        radius: number(z, "radius_m", 20.0, 0.1, 10000.0),
                        fade: number(z, "fade_m", 12.0, 0.1, 1000.0),
                        priority: z["priority"].as_i64().unwrap_or(0),
                        rack: z["rack"].as_str().map(str::to_owned),
                    });
                }
            }
        }
        out.zones.sort_by_key(|z| std::cmp::Reverse(z.priority));
        if let Some(events) = config["events"].as_object() {
            for (name, ids) in events {
                out.events.insert(
                    name.clone(),
                    strings(ids)
                        .into_iter()
                        .filter(|id| out.cues.contains_key(id))
                        .collect(),
                );
            }
        }
        Ok(out)
    }
    pub fn pool(&self, role: &str, tag: &str) -> Vec<String> {
        let active: Vec<_> = tag.split('|').collect();
        let eligible: Vec<_> = self
            .cues
            .values()
            .filter_map(|c| {
                if !(c.role == role || c.role_aliases.iter().any(|r| r == role))
                    || (role == "scene"
                        && !c
                            .tags
                            .iter()
                            .any(|t| active.first() == Some(&t.as_str()) || t == "*"))
                {
                    return None;
                }
                let specificity = if let Some(alternatives) = c.role_contexts.get(role) {
                    alternatives
                        .iter()
                        .filter(|tags| tags.iter().all(|t| active.contains(&t.as_str())))
                        .map(Vec::len)
                        .max()
                } else {
                    c.contexts
                        .iter()
                        .all(|t| active.contains(&t.as_str()))
                        .then_some(c.contexts.len())
                };
                specificity.map(|s| (c, s))
            })
            .collect();
        let specificity = eligible.iter().map(|(_, s)| *s).max().unwrap_or(0);
        let mut ids: Vec<_> = eligible
            .into_iter()
            .filter(|(_, s)| *s == specificity)
            .map(|(c, _)| c.id.clone())
            .collect();
        ids.sort();
        ids
    }
    pub fn location(&self, act: u8, position: [f32; 2]) -> Option<(String, f32)> {
        self.zones.iter().filter(|z| z.act_id == act).find_map(|z| {
            let d = (position[0] - z.center[0]).hypot(position[1] - z.center[1]);
            let gain = ((z.radius + z.fade - d) / z.fade).clamp(0.0, 1.0);
            (gain > 0.0).then(|| (z.id.clone(), gain))
        })
    }
    pub fn scene_environment(
        &self,
        id: &str,
        act: u8,
        position: [f32; 2],
    ) -> (f32, Option<String>) {
        self.zones
            .iter()
            .find(|z| z.id == id && z.act_id == act)
            .map_or((0.0, None), |z| {
                let d = (position[0] - z.center[0]).hypot(position[1] - z.center[1]);
                (
                    ((z.radius + z.fade - d) / z.fade).clamp(0.0, 1.0),
                    z.rack.clone(),
                )
            })
    }
}

#[derive(Clone)]
pub struct Pcm {
    pub samples: Arc<[f32]>,
}
impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }
}

/// Imported stereo PCM16 at44.1kHz. Strict, bounded WAV reader avoids optional decoder
/// features and rejects unsupported formats rather than playing bytes as noise.
pub fn decode(path: &Path) -> Result<Pcm> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    ensure!(
        file.metadata()?.len() <= MAX_CLIP_BYTES as u64 / 2 + 4096,
        "cue exceeds64MiB decoded budget; import a shorter loop"
    );
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(MAX_CLIP_BYTES as u64 / 2 + 4097)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        "not PCM WAV"
    );
    let u16le = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32le = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    let mut pos = 12;
    let mut format = None;
    let mut data = None;
    while pos + 8 <= bytes.len() {
        let n = u32le(pos + 4) as usize;
        let start = pos + 8;
        ensure!(n <= bytes.len() - start, "truncated WAV chunk");
        match &bytes[pos..pos + 4] {
            b"fmt " if n >= 16 => {
                format = Some((
                    u16le(start),
                    u16le(start + 2),
                    u32le(start + 4),
                    u16le(start + 14),
                ))
            }
            b"data" => data = Some(start..start + n),
            _ => {}
        }
        pos = start + n + (n & 1);
    }
    ensure!(
        format == Some((1, 2, RATE, 16)),
        "import cue as stereo44100Hz PCM16 WAV"
    );
    let d = data.context("WAV has no data")?;
    ensure!(
        d.len() % 4 == 0 && d.len() >= 16 && d.len() * 2 <= MAX_CLIP_BYTES,
        "invalid PCM data size"
    );
    let samples: Vec<f32> = bytes[d]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect();
    if samples.iter().any(|x| !x.is_finite()) {
        bail!("nonfinite PCM")
    }
    Ok(Pcm {
        samples: samples.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_traversal_or_unimported_assets() {
        let m = json!({"version":1,"assets":[{"id":"bad","default_enabled":true,"playback_path":"../outside.wav","role":"night"},{"id":"pending","default_enabled":true,"role":"night"}]});
        let lib = Library::from_json(Path::new("."), &m, &json!({})).unwrap();
        assert!(lib.cues.is_empty());
        assert_eq!(lib.errors.len(), 2);
    }
    #[test]
    fn explicit_scene_zones_and_pending_files() {
        let m = json!({"version":1,"assets":[{"id":"coffee","default_enabled":false,"role":"effect","playback_path":"clips/coffee.wav"}]});
        let c = json!({"cues":[{"asset":"coffee","enabled":true,"role":"scene","tags":["coffee_shop"]}],"locations":[{"id":"coffee_shop","center":[10,20],"radius_m":5,"fade_m":10}]});
        let lib = Library::from_json(Path::new("."), &m, &c).unwrap();
        assert_eq!(lib.pool("scene", "coffee_shop"), vec!["coffee"]);
        assert!(lib.pool("scene", "airport").is_empty());
        assert_eq!(lib.location(1, [10.0, 20.0]).unwrap().1, 1.0);
        assert_eq!(lib.location(1, [20.0, 20.0]).unwrap().1, 0.5);
        assert!(lib.location(1, [26.0, 20.0]).is_none());
        assert!(lib.location(2, [10.0, 20.0]).is_none());
    }

    #[test]
    fn role_alias_uses_requested_role_contexts_without_duplicating_content() {
        let m = json!({"version":1,"assets":[
            {"id":"shared","default_enabled":true,"playback_path":"clips/shared.wav","role":"intro",
             "role_aliases":["intro","threat","threat"],
             "role_contexts":{"intro":[[]],"threat":[["winter"],["spring","night"]]}},
            {"id":"general","default_enabled":true,"playback_path":"clips/general.wav","role":"threat"}
        ]});
        let lib = Library::from_json(Path::new("."), &m, &json!({})).unwrap();
        assert!(lib.errors.is_empty());
        assert_eq!(lib.cues.len(), 2);
        assert!(lib.cues["shared"].music());
        assert_eq!(lib.pool("intro", "map|spring|day"), vec!["shared"]);
        assert_eq!(lib.pool("threat", "map|winter|day"), vec!["shared"]);
        assert_eq!(lib.pool("threat", "map|spring|night"), vec!["shared"]);
        assert_eq!(lib.pool("threat", "map|spring|day"), vec!["general"]);
    }

    #[test]
    fn role_context_override_matches_or_alternatives_and_specificity() {
        let m = json!({"version":1,"assets":[
            {"id":"seasonal","default_enabled":true,"playback_path":"clips/a.wav","role":"day",
             "role_contexts":{"day":[["winter"],["spring"]]}},
            {"id":"night","default_enabled":true,"playback_path":"clips/b.wav","role":"day",
             "contexts":["winter","night"]}
        ]});
        let config = json!({"cues":[{"asset":"seasonal","role_contexts":{"day":[["spring"],["winter","inside"]]}}]});
        let lib = Library::from_json(Path::new("."), &m, &config).unwrap();
        assert_eq!(lib.pool("day", "map|spring|exterior"), vec!["seasonal"]);
        assert_eq!(
            lib.pool("day", "map|winter|day|exterior"),
            Vec::<String>::new()
        );
        assert_eq!(lib.pool("day", "map|winter|night|exterior"), vec!["night"]);
        assert_eq!(lib.pool("day", "map|winter|day|inside"), vec!["seasonal"]);
    }

    #[test]
    fn explicit_loop_metadata_and_binding_override_one_shot_defaults() {
        let m = json!({"version":1,"assets":[
            {"id":"effect","default_enabled":true,"playback_path":"clips/a.wav","role":"effect","explicit_filename_loop":true,"loop":{"enabled":false}},
            {"id":"stinger","default_enabled":true,"playback_path":"clips/b.wav","role":"stinger"},
            {"id":"intro","default_enabled":true,"playback_path":"clips/c.wav","role":"intro"},
            {"id":"night","default_enabled":true,"playback_path":"clips/d.wav","role":"night"}
        ]});
        let lib = Library::from_json(Path::new("."), &m, &json!({})).unwrap();
        assert!(lib.cues["effect"].looping);
        assert!(!lib.cues["stinger"].looping);
        assert!(!lib.cues["intro"].looping);
        assert!(lib.cues["night"].looping);
        let bound = Library::from_json(
            Path::new("."),
            &m,
            &json!({"cues":[{"asset":"stinger","loop":true},{"asset":"effect","loop":false}]}),
        )
        .unwrap();
        assert!(bound.cues["stinger"].looping);
        assert!(!bound.cues["effect"].looping);
    }

    #[test]
    fn malformed_context_clause_cannot_become_an_unconditional_pool_member() {
        let m = json!({"version":1,"assets":[{"id":"bad","default_enabled":true,"playback_path":"clips/a.wav",
            "role":"night","role_contexts":{"night":[[42]]}}]});
        let lib = Library::from_json(Path::new("."), &m, &json!({})).unwrap();
        assert_eq!(lib.errors.len(), 1);
        assert!(lib.pool("night", "map|winter|night").is_empty());
    }

    #[test]
    fn heavy_blizzard_is_a_separate_physical_bed_without_impossible_context_gate() {
        let m = json!({"version":1,"assets":[{"id":"storm","default_enabled":true,
            "playback_path":"clips/storm.wav","role":"blizzard","contexts":[],"loop":{"enabled":true}}]});
        let lib = Library::from_json(Path::new("."), &m, &json!({})).unwrap();
        assert!(lib.errors.is_empty());
        assert!(!lib.cues["storm"].music());
        assert!(lib.cues["storm"].looping);
        assert_eq!(lib.pool("blizzard", "map|winter|night|inside"), vec!["storm"]);
        for role in ["night", "day", "explore", "music", "combat"] {
            assert!(lib.pool(role, "map|winter|night|inside").is_empty());
        }
    }

    #[test]
    fn engine_hiss_is_physical_and_repeat_is_explicit() {
        let m = json!({"version":1,"assets":[
            {"id":"cooldown","default_enabled":true,"playback_path":"clips/a.wav","role":"engine_hiss","loop":{"enabled":false}},
            {"id":"sustain","default_enabled":true,"playback_path":"clips/b.wav","role":"engine_hiss","explicit_filename_loop":true}
        ]});
        let lib = Library::from_json(Path::new("."), &m, &json!({})).unwrap();
        assert!(lib.errors.is_empty());
        assert!(!lib.cues["cooldown"].music());
        assert!(!lib.cues["cooldown"].looping);
        assert!(lib.cues["sustain"].looping);
        assert_eq!(lib.pool("engine_hiss", "map|winter|night|inside"), vec!["cooldown", "sustain"]);
        for role in ["effect", "night", "day", "cabin", "explore", "music", "combat"] {
            assert!(lib.pool(role, "map|winter|night|inside").is_empty());
        }
    }
}
