//! Load-time reporting only. Never selects, transforms or plays an audio source.
use super::{feedback, Ambience, PizzaLayer, Sfx};

#[derive(Default)]
pub(super) struct LoadedSources {
    pub legacy_requested: bool,
    pub legacy_allowed: bool,
    pub legacy_sfx: Vec<Sfx>,
    pub legacy_ambience: Vec<Ambience>,
    pub legacy_engine: bool,
    pub legacy_mg: [bool; 2],
    pub v1_requested: bool,
}

pub(super) fn bank_metadata(
    feedback_bank: &feedback::FeedbackBank,
    sources: &LoadedSources,
    layers: &[Vec<PizzaLayer>],
) -> serde_json::Value {
    let sfx_routes: Vec<_> = Sfx::ALL.into_iter().map(|sfx| {
        let replacement = feedback::replacement_key(sfx)
            .filter(|key| feedback_bank.contains(key));
        let variants = layers.get(sfx.pizza_layer_identity().index()).map_or(0, Vec::len);
        serde_json::json!({
            "sfx": sfx.slug(),
            "feedback_key": replacement,
            "direct_source": if replacement.is_some() { "verified_feedback_recording" }
                else if sources.legacy_sfx.contains(&sfx) { "legacy_reference_recording" }
                else { "procedural" },
            // World Sfx propagation deliberately uses dry procedural sources when no feedback
            // replacement exists, even if a legacy recording loaded for direct playback.
            "world_source": if sfx==Sfx::LugerShot {
                if feedback_bank.contains("luger_shot") {"owned_generic_pistol_early_crop_and_pressure_synthesis"}
                else {"pressure_and_modal_synthesis"}
            } else if super::small_arms::is_report(sfx) {
                if feedback_bank.contains("rifle_shot") { "verified_rifle_early_crop_and_pressure_synthesis" }
                else { "pressure_and_modal_synthesis" }
            } else if super::small_arms::is_combat_variant(sfx) {"seeded_dry_procedural"}
            else if replacement.is_some() { "verified_feedback_recording" } else { "procedural" },
            "world_v1_layer_variants_active": if replacement.is_some() || super::small_arms::is_variant(sfx) {0} else {variants},
            "v1_layer_variants_available": variants,
            "v1_layer_variants_active": if replacement.is_some() { 0 } else { variants },
        })
    }).collect();
    let loop_source = |recorded: bool| if recorded { "legacy_reference_recording" } else { "procedural" };
    let loops: Vec<_> = [
        ("ambience_interior", sources.legacy_ambience.contains(&Ambience::Interior)),
        ("ambience_exterior", sources.legacy_ambience.contains(&Ambience::Exterior)),
        ("engine_loop", sources.legacy_engine),
        ("mg_loop_interior", sources.legacy_mg[0]),
        ("mg_loop_exterior", sources.legacy_mg[1]),
        ("turret_traverse_loop", false),
    ].into_iter().map(|(key, recorded)| serde_json::json!({
        "key": key, "source": loop_source(recorded),
    })).collect();
    let controls: Vec<_> = ["hatch_move", "hatch_stop", "trigger", "starter", "traverse_jam"]
        .into_iter().map(|key| serde_json::json!({
            "key": key,
            "source": if feedback_bank.contains(key) { "verified_feedback_recording" }
                else { "procedural" },
        })).collect();
    let ammunition: Vec<_> = [
        crate::acoustics::MechanicalCue::CasingExtract,
        crate::acoustics::MechanicalCue::CasingDrop,
        crate::acoustics::MechanicalCue::ShellLift,
        crate::acoustics::MechanicalCue::ShellRam,
        crate::acoustics::MechanicalCue::ShellDrop,
    ].into_iter().map(|cue| {
        let key = super::ammunition_control_key(cue);
        serde_json::json!({
            "cue": format!("{cue:?}"), "feedback_key": key,
            "source": "authored_ammunition_contact",
            "broadband_mechanical_layer": false,
        })
    }).collect();
    let layer_count: usize = layers.iter().map(Vec::len).sum();
    serde_json::json!({
        "schema": "kessel.audio-bank-status",
        "version": 1,
        "scope": "loaded Audio bank and resolved routes; scene emitters are reported by their scene manifest",
        "feedback": feedback_bank.metadata(),
        "sfx_routes": sfx_routes,
        "loops": loops,
        "controls": controls,
        "ammunition_mechanisms": ammunition,
        "legacy_references": {
            "requested_at_load": sources.legacy_requested,
            "allowed_at_load": sources.legacy_allowed,
            "loaded_base_sfx": sources.legacy_sfx.iter().map(|sfx| sfx.slug()).collect::<Vec<_>>(),
            "integrity": "decoded optional references; no source hash verification",
        },
        "v1_layers": {
            "requested_at_load": sources.v1_requested,
            "selection_id": super::PIZZA_AUDIO_SELECTION_ID,
            "selection_manifest_sha256": super::PIZZA_AUDIO_SELECTION_SHA256,
            "verified_loaded_count": layer_count,
            "status": if !sources.v1_requested { "disabled" }
                else if layer_count == 0 { "not_loaded" } else { "loaded" },
            "owner_private": true,
            "policy": "verified optional layers play only where no feedback replacement is active",
        },
        "procedural_families": [
            "unreplaced Sfx and missing-recording fallbacks",
            "MainGunRecoil, BodyImpact and finite ammunition contacts; continuous loader friction follows actual movement",
            "parametric armor impact, pressure, physical fragment contacts/flight, concussion and enemy caliber report",
        ],
        "review": "load status and routing only; subjective first-person video review is separate",
    })
}

pub(super) fn startup_summary(bank: &serde_json::Value) -> String {
    let direct = bank["sfx_routes"].as_array().expect("sound routes");
    let count = |kind: &str| direct.iter().filter(|route| route["direct_source"] == kind).count();
    format!(
        "[audio] initialized: {} verified feedback recordings ({}); direct Sfx routes: {} recorded replacements, {} legacy references, {} generated; {} verified v1 layers; ammunition contacts use distinct authored modes; friction follows actual loader motion",
        bank["feedback"]["loaded_count"],
        bank["feedback"]["status"].as_str().expect("feedback status"),
        count("verified_feedback_recording"), count("legacy_reference_recording"), count("procedural"),
        bank["v1_layers"]["verified_loaded_count"],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_metadata_distinguishes_legacy_direct_sources_from_generated_world_sources() {
        let feedback = feedback::FeedbackBank::default();
        let sources = LoadedSources {
            legacy_requested: true, legacy_allowed: true,
            legacy_sfx: vec![Sfx::TankMgShot],
            legacy_ambience: vec![Ambience::Interior],
            legacy_engine: true, legacy_mg: [false, true], v1_requested: false,
        };
        let bank = bank_metadata(&feedback, &sources, &vec![Vec::new(); Sfx::ALL.len()]);
        let route = bank["sfx_routes"].as_array().unwrap().iter()
            .find(|route| route["sfx"] == "tank_mg_shot").unwrap();
        assert_eq!(route["direct_source"], "legacy_reference_recording");
        assert_eq!(route["world_source"], "pressure_and_modal_synthesis");
        assert_eq!(bank["feedback"]["status"], "disabled");
        assert_eq!(bank["loops"][0]["source"], "legacy_reference_recording");
        assert_eq!(bank["loops"][3]["source"], "procedural");
        assert_eq!(bank["loops"][4]["source"], "legacy_reference_recording");
        assert!(startup_summary(&bank).contains("1 legacy references"));
        assert!(!startup_summary(&bank).contains("procedural-only"));
        assert!(!bank.to_string().contains("source_assets/"));
        assert!(!bank.to_string().contains("/home/"));
    }
}
