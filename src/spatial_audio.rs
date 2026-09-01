//! Editor-authored spatial sound emitters.
//!
//! The editor owns this small, renderer-free schema and stores it as the top-level
//! `sound_emitters` array in `manifest_edits.json`. Runtime audio can consume the same validated
//! records without treating sounds as mesh placements or campaign marker operations.

use std::path::{Component, Path};

use glam::Vec3;

pub const EDITS_KEY: &str = "sound_emitters";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackPolicy {
    /// Start on entry, update attenuation while inside, stop on exit.
    Loop,
    /// Start only the first time this playthrough enters the radius.
    Once,
    /// Start on each outside-to-inside transition.
    Reenter,
}

impl PlaybackPolicy {
    pub const ALL: [Self; 3] = [Self::Loop, Self::Once, Self::Reenter];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Loop => "loop",
            Self::Once => "once",
            Self::Reenter => "reenter",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Loop => "LOOP WHILE INSIDE",
            Self::Once => "ONCE PER PLAYTHROUGH",
            Self::Reenter => "REPLAY ON RE-ENTRY",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "loop" => Some(Self::Loop),
            "once" => Some(Self::Once),
            "reenter" => Some(Self::Reenter),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Loop => Self::Once,
            Self::Once => Self::Reenter,
            Self::Reenter => Self::Loop,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundEmitter {
    pub id: String,
    pub asset: String,
    pub position: Vec3,
    /// Linear gain. Values above one are permitted for deliberately quiet source recordings.
    pub volume: f32,
    pub radius_m: f32,
    /// Zero means use the whole source clip. Positive values cap a one-shot or loop segment.
    pub duration_s: f32,
    pub playback: PlaybackPolicy,
    /// Forward-compatible fields loaded from the record and written back unchanged. The editor
    /// owns the core transform/playback settings but must not erase later runtime metadata.
    pub extra: serde_json::Map<String, serde_json::Value>,
    /// Resolved by the editor/runtime environment after parsing. It is diagnostic state, not part
    /// of the portable JSON record.
    pub available: bool,
    /// Editor-only tombstone; deleted records are never serialized.
    pub deleted: bool,
}

impl SoundEmitter {
    pub fn new(id: String, asset: String, position: Vec3) -> anyhow::Result<Self> {
        let emitter = Self {
            id,
            asset,
            position,
            volume: 0.8,
            radius_m: 25.0,
            duration_s: 0.0,
            playback: PlaybackPolicy::Reenter,
            extra: serde_json::Map::new(),
            available: true,
            deleted: false,
        };
        emitter.validate()?;
        Ok(emitter)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.id.trim().is_empty()
                && self
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
            "sound emitter id must be non-empty and contain only letters, digits, '-' or '_'"
        );
        anyhow::ensure!(
            safe_ogg_asset(&self.asset),
            "unsafe or unsupported sound asset"
        );
        anyhow::ensure!(
            self.position.is_finite(),
            "sound emitter position must be finite"
        );
        anyhow::ensure!(
            self.volume.is_finite() && (0.0..=4.0).contains(&self.volume),
            "sound emitter volume must be within 0..=4"
        );
        anyhow::ensure!(
            self.radius_m.is_finite() && (0.5..=2_000.0).contains(&self.radius_m),
            "sound emitter radius_m must be within 0.5..=2000"
        );
        anyhow::ensure!(
            self.duration_s.is_finite() && (0.0..=3_600.0).contains(&self.duration_s),
            "sound emitter duration_s must be within 0..=3600"
        );
        Ok(())
    }

    pub fn to_json(&self) -> serde_json::Value {
        let known = serde_json::json!({
            "id": self.id,
            "asset": self.asset,
            "position_m": [self.position.x, self.position.y, self.position.z],
            "volume": self.volume,
            "radius_m": self.radius_m,
            "duration_s": self.duration_s,
            "playback": self.playback.as_str(),
        });
        let mut object = self.extra.clone();
        object.extend(known.as_object().unwrap().clone());
        serde_json::Value::Object(object)
    }

    pub fn from_json(value: &serde_json::Value) -> anyhow::Result<Self> {
        let mut extra = value
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("sound emitter must be a JSON object"))?;
        for key in [
            "id",
            "asset",
            "position_m",
            "volume",
            "radius_m",
            "duration_s",
            "playback",
        ] {
            extra.remove(key);
        }
        let id = value["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("sound emitter id is required"))?
            .to_owned();
        let asset = value["asset"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("sound emitter asset is required"))?
            .to_owned();
        let p = value["position_m"]
            .as_array()
            .filter(|values| values.len() == 3)
            .ok_or_else(|| anyhow::anyhow!("sound emitter position_m must have three numbers"))?;
        let number = |value: &serde_json::Value, field: &str| -> anyhow::Result<f32> {
            value
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .ok_or_else(|| anyhow::anyhow!("sound emitter {field} must be finite"))
        };
        let emitter = Self {
            id,
            asset,
            position: Vec3::new(
                number(&p[0], "position_m[0]")?,
                number(&p[1], "position_m[1]")?,
                number(&p[2], "position_m[2]")?,
            ),
            volume: number(&value["volume"], "volume")?,
            radius_m: number(&value["radius_m"], "radius_m")?,
            duration_s: number(&value["duration_s"], "duration_s")?,
            playback: value["playback"]
                .as_str()
                .and_then(PlaybackPolicy::parse)
                .ok_or_else(|| {
                    anyhow::anyhow!("sound emitter playback must be loop, once or reenter")
                })?,
            extra,
            available: true,
            deleted: false,
        };
        emitter.validate()?;
        Ok(emitter)
    }

    /// Smooth distance falloff with a quiet boundary, suitable for a runtime voice adapter.
    pub fn gain_at(&self, listener: Vec3) -> f32 {
        let normalized = (listener.distance(self.position) / self.radius_m).clamp(0.0, 1.0);
        let envelope = 1.0 - normalized * normalized;
        self.volume * envelope * envelope
    }
}

fn safe_ogg_asset(asset: &str) -> bool {
    let path = Path::new(asset);
    if path.is_absolute()
        || !asset.starts_with("assets/sounds/")
        || path.extension().and_then(|ext| ext.to_str()) != Some("ogg")
    {
        return false;
    }
    path.components().all(|component| {
        matches!(component, Component::Normal(_))
            || matches!(component, Component::CurDir) && !asset.contains("./")
    })
}

pub fn from_edits(edits: Option<&serde_json::Value>) -> anyhow::Result<Vec<SoundEmitter>> {
    let Some(edits) = edits else {
        return Ok(Vec::new());
    };
    let Some(value) = edits.get(EDITS_KEY) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{EDITS_KEY} must be an array"))?;
    let mut out = Vec::with_capacity(values.len());
    let mut ids = std::collections::HashSet::with_capacity(values.len());
    for value in values {
        let emitter = SoundEmitter::from_json(value)?;
        anyhow::ensure!(
            ids.insert(emitter.id.clone()),
            "duplicate sound emitter id {}",
            emitter.id
        );
        out.push(emitter);
    }
    Ok(out)
}

pub fn to_json(emitters: &[SoundEmitter]) -> Vec<serde_json::Value> {
    emitters
        .iter()
        .filter(|emitter| !emitter.deleted)
        .map(SoundEmitter::to_json)
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EmitterRuntimeState {
    pub inside: bool,
    pub played_once: bool,
    pub loop_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmitterAction {
    None,
    Start { gain: f32, looping: bool },
    Update { gain: f32 },
    Stop,
}

/// Pure trigger state machine. Audio ownership only needs to translate these actions into voices.
pub fn tick(
    emitter: &SoundEmitter,
    state: &mut EmitterRuntimeState,
    listener: Vec3,
) -> EmitterAction {
    if emitter.deleted || !emitter.available {
        let stop = state.loop_active;
        *state = EmitterRuntimeState::default();
        return if stop {
            EmitterAction::Stop
        } else {
            EmitterAction::None
        };
    }
    let gain = emitter.gain_at(listener);
    // A small exit-only hysteresis band prevents listener jitter on the boundary from repeatedly
    // retriggering a one-shot or chattering a loop voice on and off.
    let threshold = emitter.radius_m * if state.inside { 1.05 } else { 1.0 };
    let inside = listener.distance_squared(emitter.position) <= threshold * threshold;
    let entered = inside && !state.inside;
    let exited = !inside && state.inside;
    state.inside = inside;
    match emitter.playback {
        PlaybackPolicy::Loop => {
            if entered {
                state.loop_active = true;
                EmitterAction::Start {
                    gain,
                    looping: true,
                }
            } else if exited && state.loop_active {
                state.loop_active = false;
                EmitterAction::Stop
            } else if inside && state.loop_active {
                EmitterAction::Update { gain }
            } else {
                EmitterAction::None
            }
        }
        PlaybackPolicy::Once if entered && !state.played_once => {
            state.played_once = true;
            EmitterAction::Start {
                gain,
                looping: false,
            }
        }
        PlaybackPolicy::Reenter if entered => EmitterAction::Start {
            gain,
            looping: false,
        },
        _ => EmitterAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitter(policy: PlaybackPolicy) -> SoundEmitter {
        SoundEmitter {
            id: "wind_field".into(),
            asset: "assets/sounds/ambience_exterior.ogg".into(),
            position: Vec3::ZERO,
            volume: 0.8,
            radius_m: 20.0,
            duration_s: 0.0,
            playback: policy,
            extra: serde_json::Map::new(),
            available: true,
            deleted: false,
        }
    }

    #[test]
    fn json_roundtrip_preserves_editor_settings() {
        let mut source = emitter(PlaybackPolicy::Reenter);
        source.extra.insert(
            "future_weather_binding".into(),
            serde_json::json!({"profile": "windward"}),
        );
        let decoded = SoundEmitter::from_json(&source.to_json()).unwrap();
        assert_eq!(decoded, source);
        assert!(decoded.gain_at(Vec3::ZERO) > decoded.gain_at(Vec3::new(10.0, 0.0, 0.0)));
        assert_eq!(decoded.gain_at(Vec3::new(20.0, 0.0, 0.0)), 0.0);
    }

    #[test]
    fn rejects_traversal_wav_and_invalid_ranges() {
        for asset in ["../escape.ogg", "assets/sounds/ref.wav", "/tmp/ref.ogg"] {
            let mut bad = emitter(PlaybackPolicy::Once);
            bad.asset = asset.into();
            assert!(bad.validate().is_err(), "accepted {asset}");
        }
        let mut bad = emitter(PlaybackPolicy::Loop);
        bad.radius_m = f32::INFINITY;
        assert!(bad.validate().is_err());
        assert!(from_edits(Some(&serde_json::json!({EDITS_KEY: {}}))).is_err());
    }

    #[test]
    fn loop_starts_updates_and_stops_at_radius() {
        let emitter = emitter(PlaybackPolicy::Loop);
        let mut state = EmitterRuntimeState::default();
        assert!(matches!(
            tick(&emitter, &mut state, Vec3::ZERO),
            EmitterAction::Start { looping: true, .. }
        ));
        assert!(matches!(
            tick(&emitter, &mut state, Vec3::new(5.0, 0.0, 0.0)),
            EmitterAction::Update { .. }
        ));
        assert_eq!(
            tick(&emitter, &mut state, Vec3::new(30.0, 0.0, 0.0)),
            EmitterAction::Stop
        );
    }

    #[test]
    fn once_and_reentry_have_distinct_revisit_behavior() {
        for (policy, second_entry_starts) in [
            (PlaybackPolicy::Once, false),
            (PlaybackPolicy::Reenter, true),
        ] {
            let emitter = emitter(policy);
            let mut state = EmitterRuntimeState::default();
            assert!(matches!(
                tick(&emitter, &mut state, Vec3::ZERO),
                EmitterAction::Start { looping: false, .. }
            ));
            tick(&emitter, &mut state, Vec3::new(30.0, 0.0, 0.0));
            let action = tick(&emitter, &mut state, Vec3::ZERO);
            assert_eq!(
                matches!(action, EmitterAction::Start { .. }),
                second_entry_starts
            );
        }
    }

    #[test]
    fn boundary_hysteresis_prevents_reentry_chatter() {
        let emitter = emitter(PlaybackPolicy::Reenter);
        let mut state = EmitterRuntimeState::default();
        assert!(matches!(
            tick(&emitter, &mut state, Vec3::new(19.9, 0.0, 0.0)),
            EmitterAction::Start { .. }
        ));
        assert_eq!(
            tick(&emitter, &mut state, Vec3::new(20.5, 0.0, 0.0)),
            EmitterAction::None
        );
        assert_eq!(
            tick(&emitter, &mut state, Vec3::new(19.9, 0.0, 0.0)),
            EmitterAction::None
        );
        tick(&emitter, &mut state, Vec3::new(22.0, 0.0, 0.0));
        assert!(matches!(
            tick(&emitter, &mut state, Vec3::new(19.9, 0.0, 0.0)),
            EmitterAction::Start { .. }
        ));
    }
}
