//! Debug-only gates for presentation shortcuts that have no simulated physical cause
//! (CLAUDE.md principle 2 rule 5). Each is off in both shipping presets: in a release build the
//! environment is never read, and in a debug build a gate opens only on an explicit opt-in.
//! `tests::every_gate_is_off_in_both_shipping_presets` is the required regression.
//!
//! - `KESSEL_DEBUG_SHELL_SHRIEK=1`: the scripted outgoing-shell "shriek" heard head-out. A firer
//!   behind a Mach 2+ round hears the muzzle blast, not a receding shriek (the round's shock never
//!   reaches him); removed pending a simulated projectile acoustic.
//! - `KESSEL_DEBUG_AMBIENT_BOOMS=1`: random booms on the horizon every 6–20 s with no simulated
//!   source; removed pending simulated off-map indirect fire (artillery, mortars).
//! - `KESSEL_DEBUG_HEARING_TIME_SCALE=<factor>`: time compression of hearing recovery. Real
//!   recovery (16–72 h) is the default; this multiplies the ear model's clock for tests of the
//!   recovery path and is named as time compression wherever it is reported.
//! - `KESSEL_CAMERA_EXPOSURE=<exposure>`: a fixed camera exposure in place of the crewman's eye
//!   (`observer.rs`), for photographic A/B stills only. The shipping display is always the eye.

use crate::graphics::GraphicsPreset;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugGates {
    pub shell_shriek: bool,
    pub ambient_booms: bool,
    /// Multiplier on the ear model's recovery clock (1 = real time).
    pub hearing_time_scale: f32,
    /// A fixed camera exposure replacing the eye (debug stills only).
    pub fixed_camera_exposure: Option<f32>,
}

impl DebugGates {
    /// Every gate closed: what both shipping presets run.
    pub const SHIPPING: Self = Self { shell_shriek: false, ambient_booms: false, hearing_time_scale: 1.0,
        fixed_camera_exposure: None };

    /// Resolve the gates. `debug_build` is `cfg!(debug_assertions)`; the preset does not open any
    /// gate — it is an argument so the regression can prove both presets closed.
    pub fn resolve(debug_build: bool, _preset: GraphicsPreset, env: impl Fn(&str) -> Option<String>)
        -> Self
    {
        if !debug_build {
            return Self::SHIPPING;
        }
        let opted = |key: &str| env(key).is_some_and(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on")
        });
        let scale = env("KESSEL_DEBUG_HEARING_TIME_SCALE")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v >= 1.0)
            .map_or(1.0, |v| v.min(1.0e6));
        let fixed_camera_exposure = env("KESSEL_CAMERA_EXPOSURE")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && (0.01..=1.0e7).contains(v));
        Self {
            shell_shriek: opted("KESSEL_DEBUG_SHELL_SHRIEK"),
            ambient_booms: opted("KESSEL_DEBUG_AMBIENT_BOOMS"),
            hearing_time_scale: scale,
            fixed_camera_exposure,
        }
    }

    /// The gates of this process (read once).
    pub fn active() -> Self {
        static ACTIVE: std::sync::OnceLock<DebugGates> = std::sync::OnceLock::new();
        *ACTIVE.get_or_init(|| Self::resolve(cfg!(debug_assertions),
            crate::graphics::active_preset(), |key| std::env::var(key).ok()))
    }

    pub fn is_shipping(&self) -> bool {
        *self == Self::SHIPPING
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gate_is_off_in_both_shipping_presets() {
        let everything_on = |key: &str| match key {
            "KESSEL_DEBUG_HEARING_TIME_SCALE" => Some("3600".to_string()),
            "KESSEL_CAMERA_EXPOSURE" => Some("320".to_string()),
            _ => Some("1".to_string()),
        };
        let nothing = |_: &str| None;
        for preset in [GraphicsPreset::Laptop, GraphicsPreset::High] {
            // A release build never reads the environment: every gate stays closed.
            assert!(DebugGates::resolve(false, preset, everything_on).is_shipping(), "{preset:?}");
            // A debug build without an explicit opt-in is closed too.
            assert!(DebugGates::resolve(true, preset, nothing).is_shipping(), "{preset:?}");
            // Only an explicit opt-in in a debug build opens them.
            let open = DebugGates::resolve(true, preset, everything_on);
            assert!(open.shell_shriek && open.ambient_booms && open.hearing_time_scale == 3600.0);
            assert_eq!(open.fixed_camera_exposure, Some(320.0));
        }
        // Nonsense never opens a gate or speeds time backwards.
        let junk = |key: &str| match key {
            "KESSEL_DEBUG_HEARING_TIME_SCALE" => Some("-5".to_string()),
            "KESSEL_CAMERA_EXPOSURE" => Some("NaN".to_string()),
            _ => Some("enabled".to_string()),
        };
        assert!(DebugGates::resolve(true, GraphicsPreset::High, junk).is_shipping());
        // The shipping constant is what the live code uses when nothing is set in this process.
        assert!(DebugGates::SHIPPING.hearing_time_scale == 1.0);
    }
}
