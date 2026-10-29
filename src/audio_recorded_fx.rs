//! Recorded effects from the owner's licensed libraries (Rust & Blood SFX Library v1.1d, Echoes
//! Audio Super Kit v2.0.1, ROT Horror Audio Bundle v1.8; owner-directed, 2026-10-28). The files
//! are owner-private: copied under the git-ignored `assets/sounds` with the names below, never
//! committed (`assets/sounds/LIBRARY_20261028_SOURCES.txt`, local, maps each to its source).
//!
//! Each family is a set of variants used in turn without repeating back to back, prepared once:
//! decoded to the acoustics rate, the leading silence trimmed to 2 ms before the onset, and where
//! the family says so resampled (`pitch` < 1 lowers and lengthens, to give a rifle-sized part the
//! weight of an 88 mm one) or reversed. Level is not set here: the game places each at its 1 m
//! reference (`Fx::ref_dba_1m`, estimates [E]) propagated from where it happens, through the
//! same hull, headset and room stages as every other world or cabin source.
use super::{Audio, Clip};
use std::{cell::{Cell, RefCell}, collections::HashMap, sync::Arc};

/// Surfaces with recorded footsteps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum StepSurface { Concrete, Dirt, Grass, Gravel, Metal, Snow, Wood }

/// Struck or broken material classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Material { Body, Concrete, Glass, Metal, Wood }

/// What a body struck (wound contact layer).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BodyContact { Metal, Wood, Soft, Heavy }

/// One family of recorded variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Fx {
    /// Equipment dropped or set down (tools, weapons, binoculars, kit).
    EquipDrop,
    /// Equipment picked up.
    ItemPickup,
    /// Gear raised, drawn, holstered, put on or taken off.
    ItemEquip,
    /// The main gun's electric trigger and firing mechanism, pulled without a shot.
    TriggerDry,
    /// The Luger's toggle pulled up and back to chamber a round.
    LugerRack,
    /// A round or case drawn out of the 88's breech.
    BreechExtract,
    /// The round rammed home; its last strike is the seating (`Prepared::strike_s`).
    BreechRam,
    /// The loader's lever pull and the breech block closing.
    BreechClose,
    /// A live 88 mm round lifted out of the rack or off the floor.
    ShellPickup,
    /// A 12.7–25 mm round striking the hull and stopped.
    LightCalibreHit,
    /// A cabin switch, button or knob.
    SwitchButton,
    MaterialImpact(Material),
    MaterialBreak(Material),
    /// Wound layers by mechanism and severity.
    WoundBluntLight, WoundBluntModerate, WoundBluntHeavy,
    WoundPenetrating, WoundSharpLight, WoundSharpHeavy, WoundCritical,
    WoundLacerationLight, WoundLacerationHeavy,
    /// A body knocked without a wound (a bump, a fall).
    BodyKnock,
    /// What a body struck.
    BodyContact(BodyContact),
    /// A small fragment pinging off steel near the crew.
    FragmentPing,
    MedkitUse, BagOpen, BagClose, PaperPage, CrateOpen, CrateClose,
    Footstep(StepSurface),
}

macro_rules! files { ($($f:literal),* $(,)?) => { &[$(concat!("assets/sounds/", $f, ".ogg")),*] } }

impl Fx {
    pub(crate) fn name(self) -> String { format!("{self:?}") }

    fn files(self) -> &'static [&'static str] {
        use BodyContact as C; use Material as M; use StepSurface as S;
        match self {
            Fx::EquipDrop => files!("equip_drop_01", "equip_drop_02", "equip_drop_03", "equip_drop_04", "equip_drop_05"),
            Fx::ItemPickup => files!("item_pickup_01"),
            Fx::ItemEquip => files!("item_equip_01", "item_equip_02", "item_equip_03", "item_equip_04"),
            Fx::TriggerDry => files!("gun_trigger_dry_01"),
            Fx::LugerRack => files!("luger_rack_01"),
            Fx::BreechExtract => files!("breech_extract_01"),
            Fx::BreechRam => files!("breech_ram_01", "breech_ram_02"),
            Fx::BreechClose => files!("breech_close_01", "breech_close_02", "breech_close_03"),
            Fx::ShellPickup => files!("shell_pickup_01", "shell_pickup_02"),
            Fx::LightCalibreHit => files!("tank_hit_light_calibre_01", "tank_hit_light_calibre_02", "tank_hit_light_calibre_03",
                "tank_hit_light_calibre_04", "tank_hit_light_calibre_05"),
            Fx::SwitchButton => files!("switch_button_01", "switch_button_02", "switch_button_03", "switch_button_04"),
            Fx::MaterialImpact(M::Body) => files!("material_impact_body_01", "material_impact_body_02"),
            Fx::MaterialImpact(M::Concrete) => files!("material_impact_concrete_01", "material_impact_concrete_02", "material_impact_concrete_03"),
            Fx::MaterialImpact(M::Glass) => files!("material_impact_glass_01", "material_impact_glass_02", "material_impact_glass_03",
                "material_impact_glass_04", "material_impact_glass_05"),
            Fx::MaterialImpact(M::Metal) => files!("material_impact_metal_01", "material_impact_metal_02", "material_impact_metal_03", "material_impact_metal_04"),
            Fx::MaterialImpact(M::Wood) => files!("material_impact_wood_01", "material_impact_wood_02"),
            Fx::MaterialBreak(M::Concrete) => files!("material_break_concrete_01", "material_break_concrete_02"),
            Fx::MaterialBreak(M::Metal) => files!("material_break_metal_01"),
            Fx::MaterialBreak(M::Wood) => files!("material_break_wood_01"),
            // Glass and bodies have no separate break recordings: their impacts are the break.
            Fx::MaterialBreak(M::Glass) => Fx::MaterialImpact(M::Glass).files(),
            Fx::MaterialBreak(M::Body) => Fx::MaterialImpact(M::Body).files(),
            Fx::WoundBluntLight => files!("wound_blunt_bloody_light_01", "wound_blunt_bloody_light_02"),
            Fx::WoundBluntModerate => files!("wound_blunt_bloody_moderate_01"),
            Fx::WoundBluntHeavy => files!("wound_blunt_bloody_heavy_01"),
            Fx::WoundPenetrating => files!("wound_penetrating_01", "wound_penetrating_02"),
            Fx::WoundSharpLight => files!("wound_sharp_light_01"),
            Fx::WoundSharpHeavy => files!("wound_sharp_heavy_01"),
            Fx::WoundCritical => files!("wound_critical_01", "wound_critical_02"),
            Fx::WoundLacerationLight => files!("wound_laceration_light_01", "wound_laceration_body_01"),
            Fx::WoundLacerationHeavy => files!("wound_laceration_heavy_01", "wound_laceration_heavy_02", "wound_laceration_heavy_03"),
            Fx::BodyKnock => files!("body_knock_01", "body_knock_02", "body_knock_03", "body_knock_04"),
            Fx::BodyContact(C::Metal) => files!("body_contact_metal_01"),
            Fx::BodyContact(C::Wood) => files!("body_contact_wood_01"),
            Fx::BodyContact(C::Soft) => files!("body_contact_soft_01"),
            Fx::BodyContact(C::Heavy) => files!("body_contact_heavy_01"),
            Fx::FragmentPing => files!("fragment_ping_steel_01", "fragment_ping_steel_02", "fragment_ping_steel_03"),
            Fx::MedkitUse => files!("medkit_use_01"),
            Fx::BagOpen => files!("bag_open_01"),
            Fx::BagClose => files!("bag_close_01"),
            Fx::PaperPage => files!("paper_page_01"),
            Fx::CrateOpen => files!("crate_open_01"),
            Fx::CrateClose => files!("crate_close_01"),
            Fx::Footstep(S::Concrete) => files!("footstep_concrete_01", "footstep_concrete_02", "footstep_concrete_03", "footstep_concrete_04"),
            Fx::Footstep(S::Dirt) => files!("footstep_dirt_01", "footstep_dirt_02", "footstep_dirt_03", "footstep_dirt_04",
                "footstep_dirt_05", "footstep_dirt_06", "footstep_dirt_07", "footstep_dirt_08"),
            Fx::Footstep(S::Grass) => files!("footstep_grass_01", "footstep_grass_02", "footstep_grass_03", "footstep_grass_04", "footstep_grass_05"),
            Fx::Footstep(S::Gravel) => files!("footstep_gravel_01", "footstep_gravel_02", "footstep_gravel_03", "footstep_gravel_04", "footstep_gravel_05"),
            Fx::Footstep(S::Metal) => files!("footstep_metal_01", "footstep_metal_02", "footstep_metal_03", "footstep_metal_04",
                "footstep_metal_05", "footstep_metal_06", "footstep_metal_07"),
            Fx::Footstep(S::Snow) => files!("footstep_snow_01", "footstep_snow_02", "footstep_snow_03", "footstep_snow_04",
                "footstep_snow_05", "footstep_snow_06", "footstep_snow_07", "footstep_snow_08", "footstep_snow_09",
                "footstep_snow_10", "footstep_snow_11"),
            Fx::Footstep(S::Wood) => files!("footstep_wood_01", "footstep_wood_02", "footstep_wood_03", "footstep_wood_04",
                "footstep_wood_05", "footstep_wood_06", "footstep_wood_07", "footstep_wood_08", "footstep_wood_09",
                "footstep_wood_10", "footstep_wood_11", "footstep_wood_12", "footstep_wood_13", "footstep_wood_14",
                "footstep_wood_15", "footstep_wood_16", "footstep_wood_17", "footstep_wood_18", "footstep_wood_19",
                "footstep_wood_20"),
        }
    }

    /// Resampling factor applied once at load (< 1 lowers pitch and lengthens) [E]: the breech
    /// parts are rifle- and shotgun-sized recordings given the mass of an 88 mm round and a
    /// 400 kg breech block (a mode scales with 1/size: 0.6 ≈ a part 1.7× the size).
    fn pitch(self) -> f32 {
        match self {
            Fx::BreechClose => 0.62,
            Fx::BreechRam => 0.85,
            Fx::BreechExtract => 0.80,
            _ => 1.0,
        }
    }

    /// A-weighted level at 1 m of the event as recorded here [E] (the caller adds the event's own
    /// offset: drop height, speed, energy, effort). Anchored to the foley catalogue's levels for
    /// the same classes of contact (`acoustics::foley::Cue::ref_dba_1m`).
    pub(crate) fn ref_dba_1m(self) -> f32 {
        use StepSurface as S;
        match self {
            Fx::EquipDrop => 74.0,
            Fx::ItemPickup | Fx::ItemEquip => 58.0,
            Fx::TriggerDry => 66.0,
            Fx::LugerRack => 72.0,
            Fx::BreechExtract => 80.0,
            Fx::BreechRam => 86.0,
            Fx::BreechClose => 94.0,
            Fx::ShellPickup => 66.0,
            Fx::LightCalibreHit => 110.0,
            Fx::SwitchButton => 52.0,
            Fx::MaterialImpact(_) => 84.0,
            Fx::MaterialBreak(_) => 90.0,
            Fx::WoundBluntLight | Fx::WoundSharpLight | Fx::WoundLacerationLight | Fx::WoundPenetrating => 66.0,
            Fx::WoundBluntModerate | Fx::WoundSharpHeavy => 70.0,
            Fx::WoundBluntHeavy | Fx::WoundCritical | Fx::WoundLacerationHeavy => 74.0,
            Fx::BodyKnock => 64.0,
            Fx::BodyContact(_) => 72.0,
            Fx::FragmentPing => 76.0,
            Fx::MedkitUse | Fx::BagOpen | Fx::BagClose => 56.0,
            Fx::PaperPage => 48.0,
            Fx::CrateOpen | Fx::CrateClose => 70.0,
            Fx::Footstep(s) => match s {
                S::Snow => 60.0, S::Dirt => 62.0, S::Grass => 58.0, S::Gravel => 68.0,
                S::Concrete => 70.0, S::Wood => 72.0, S::Metal => 80.0,
            },
        }
    }
}

/// One prepared variant.
pub(crate) struct Prepared {
    pub mono: Arc<[f32]>,
    pub file: &'static str,
    pub variant: usize,
    /// Seconds from the start to the clip's last strong transient (the ram's seating strike).
    pub strike_s: f32,
}

struct Family {
    clips: Vec<Prepared>,
    last: Cell<Option<usize>>,
    played: Cell<u32>,
}

const SR: f32 = crate::acoustics::SAMPLE_RATE as f32;

/// Leading silence trimmed to 2 ms before the first sample within 40 dB of the peak.
fn trim_head(mono: &[f32]) -> Vec<f32> {
    let peak = mono.iter().fold(0.0f32, |a, b| a.max(b.abs()));
    if peak <= 0.0 { return Vec::new(); }
    let onset = mono.iter().position(|x| x.abs() > peak * 0.01).unwrap_or(0);
    let start = onset.saturating_sub((0.002 * SR) as usize);
    let mut out = mono[start..].to_vec();
    let ramp = ((0.001 * SR) as usize).min(out.len());
    for (i, x) in out.iter_mut().take(ramp).enumerate() { *x *= i as f32 / ramp as f32; }
    out
}

/// Linear-interpolation resample by `factor` (< 1 lowers pitch and lengthens).
fn resample(mono: &[f32], factor: f32) -> Vec<f32> {
    if (factor - 1.0).abs() < 1e-4 || mono.len() < 2 { return mono.to_vec(); }
    let n = ((mono.len() - 1) as f32 / factor) as usize;
    (0..n).map(|i| {
        let t = i as f32 * factor;
        let j = t as usize;
        let f = t - j as f32;
        mono[j] * (1.0 - f) + mono.get(j + 1).copied().unwrap_or(0.0) * f
    }).collect()
}

/// Seconds to the last strong transient: the last 5 ms frame whose energy rises 12 dB over the
/// mean of the 20 ms before it, within 30 dB of the clip's loudest frame.
pub(crate) fn last_strike_s(mono: &[f32]) -> f32 {
    let hop = (0.005 * SR) as usize;
    let frames: Vec<f32> = mono.chunks(hop).map(|c| c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).collect();
    let loudest = frames.iter().cloned().fold(0.0f32, f32::max);
    let mut at = 0;
    for i in 4..frames.len() {
        let before = frames[i - 4..i].iter().sum::<f32>() / 4.0;
        if frames[i] > loudest * 1e-3 && frames[i] > before * 15.85 { at = i; }
    }
    at as f32 * hop as f32 / SR
}

impl Family {
    fn load(fx: Fx) -> Self {
        let mut clips = Vec::new();
        for (i, path) in fx.files().iter().enumerate() {
            let Some(mono) = crate::resource::read(path).ok()
                .and_then(|bytes| Clip::from_encoded_bytes(path, bytes)).map(|clip| clip.world_mono()) else { continue };
            let mono = resample(&trim_head(&mono), fx.pitch());
            if mono.len() < 64 || !mono.iter().all(|v| v.is_finite()) { continue; }
            let strike_s = last_strike_s(&mono);
            clips.push(Prepared { mono: Arc::from(mono), file: path, variant: i + 1, strike_s });
        }
        Self { clips, last: Cell::new(None), played: Cell::new(0) }
    }

    fn pick(&self, seed: u32) -> Option<&Prepared> {
        let n = self.clips.len();
        if n == 0 { return None; }
        let hash = seed.wrapping_mul(0x9E37_79B1).rotate_left(13) ^ 0x27D4_EB2F;
        let mut at = (hash as usize) % n;
        if let Some(last) = self.last.get().filter(|_| n > 1) {
            at = (hash as usize) % (n - 1);
            if at >= last { at += 1; }
        }
        self.last.set(Some(at));
        self.played.set(self.played.get().wrapping_add(1));
        self.clips.get(at)
    }
}

/// Every family, loaded on first use (a family's files decode in a few milliseconds).
#[derive(Default)]
pub(super) struct RecordedBank {
    enabled: bool,
    families: RefCell<HashMap<Fx, Family>>,
    /// Voices started this simulation update, and the cap (`begin_update`).
    voices_this_update: Cell<u32>,
    capped: Cell<u32>,
}

/// At most this many recorded effect voices start in one simulation update; later requests in the
/// same update are dropped (a crush through a debris field, a spall burst).
pub(crate) const MAX_VOICES_PER_UPDATE: u32 = 6;

impl RecordedBank {
    /// Off with `KESSEL_RECORDED_FX=0`; unit tests keep the voices these replace unless a test
    /// asks for the bank (`=1`).
    pub(super) fn load() -> Self {
        let wanted = std::env::var("KESSEL_RECORDED_FX").ok();
        let enabled = !(wanted.as_deref() == Some("0") || (cfg!(test) && wanted.as_deref() != Some("1")));
        Self { enabled, ..Default::default() }
    }

    fn with<R>(&self, fx: Fx, f: impl FnOnce(&Family) -> R) -> Option<R> {
        if !self.enabled { return None; }
        let mut families = self.families.borrow_mut();
        let family = families.entry(fx).or_insert_with(|| Family::load(fx));
        if family.clips.is_empty() { return None; }
        Some(f(family))
    }
}

impl Audio {
    /// Whether `fx` has installed recordings (the caller keeps its previous voice where not).
    pub(crate) fn recorded_fx_installed(&self, fx: Fx) -> bool {
        self.recorded_fx.with(fx, |_| ()).is_some()
    }

    /// The next variant of `fx`, without repeating the last; `None` where not installed or when
    /// this update's voice cap is spent.
    pub(crate) fn recorded_fx(&self, fx: Fx, seed: u32) -> Option<(Arc<[f32]>, &'static str, usize, f32)> {
        let bank = &self.recorded_fx;
        if bank.voices_this_update.get() >= MAX_VOICES_PER_UPDATE {
            bank.capped.set(bank.capped.get().wrapping_add(1));
            return None;
        }
        let picked = bank.with(fx, |family| family.pick(seed).map(|p| (Arc::clone(&p.mono), p.file, p.variant, p.strike_s)))??;
        bank.voices_this_update.set(bank.voices_this_update.get() + 1);
        Some(picked)
    }

    /// Start of a simulation update: the per-update voice cap refills.
    pub(crate) fn recorded_fx_begin_update(&self) { self.recorded_fx.voices_this_update.set(0); }

    pub fn recorded_fx_snapshot(&self) -> serde_json::Value {
        let families = self.recorded_fx.families.borrow();
        let mut out = serde_json::Map::new();
        for (fx, f) in families.iter() {
            out.insert(fx.name(), serde_json::json!({"installed": f.clips.len(), "played": f.played.get(),
                "files": f.clips.iter().map(|p| serde_json::json!({"file": p.file, "seconds": p.mono.len() as f32 / SR,
                    "strike_s": p.strike_s})).collect::<Vec<_>>()}));
        }
        serde_json::json!({"enabled": self.recorded_fx.enabled, "families": out,
            "capped_requests": self.recorded_fx.capped.get(), "max_voices_per_update": MAX_VOICES_PER_UPDATE})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_names_installed_style_paths_and_a_level() {
        use BodyContact as C; use Material as M; use StepSurface as S;
        let all = [Fx::EquipDrop, Fx::ItemPickup, Fx::ItemEquip, Fx::TriggerDry, Fx::LugerRack, Fx::BreechExtract,
            Fx::BreechRam, Fx::BreechClose, Fx::ShellPickup, Fx::LightCalibreHit, Fx::SwitchButton,
            Fx::WoundBluntLight, Fx::WoundBluntModerate, Fx::WoundBluntHeavy, Fx::WoundPenetrating, Fx::WoundSharpLight,
            Fx::WoundSharpHeavy, Fx::WoundCritical, Fx::WoundLacerationLight, Fx::WoundLacerationHeavy, Fx::BodyKnock,
            Fx::FragmentPing, Fx::MedkitUse, Fx::BagOpen, Fx::BagClose, Fx::PaperPage, Fx::CrateOpen, Fx::CrateClose];
        let mut n = 0;
        let mut check = |fx: Fx| {
            assert!(!fx.files().is_empty());
            for f in fx.files() { assert!(f.starts_with("assets/sounds/") && f.ends_with(".ogg")); }
            assert!(fx.ref_dba_1m() > 40.0 && fx.ref_dba_1m() < 120.0);
            n += fx.files().len();
        };
        for fx in all { check(fx); }
        for m in [M::Body, M::Concrete, M::Glass, M::Metal, M::Wood] { check(Fx::MaterialImpact(m)); check(Fx::MaterialBreak(m)); }
        for c in [C::Metal, C::Wood, C::Soft, C::Heavy] { check(Fx::BodyContact(c)); }
        for s in [S::Concrete, S::Dirt, S::Grass, S::Gravel, S::Metal, S::Snow, S::Wood] { check(Fx::Footstep(s)); }
        assert!(n > 120);
    }

    #[test]
    fn trim_resample_and_strike() {
        let mut clip = vec![0.0f32; 4800];
        clip.extend((0..4800).map(|i| (i as f32 * 0.3).sin() * 0.05));
        clip.extend((0..2400).map(|i| (i as f32 * 0.7).sin() * (1.0 - i as f32 / 2400.0)));
        let trimmed = trim_head(&clip);
        assert!(trimmed.len() < clip.len() - 4000, "leading silence trimmed");
        let strike = last_strike_s(&trimmed);
        let expected = (4800.0 + 96.0) / SR;
        assert!((strike - expected).abs() < 0.012, "strike at {strike} s, expected ~{expected}");
        let low = resample(&trimmed, 0.5);
        assert!((low.len() as f32 / trimmed.len() as f32 - 2.0).abs() < 0.01);
    }

    #[test]
    fn variants_never_repeat_back_to_back() {
        let family = Family { clips: (0..3).map(|i| Prepared { mono: Arc::from(vec![0.1; 8]), file: "x", variant: i + 1, strike_s: 0.0 }).collect(),
            last: Cell::new(None), played: Cell::new(0) };
        let mut prev = None;
        for seed in 0..300 { let v = family.pick(seed).unwrap().variant; assert_ne!(Some(v), prev); prev = Some(v); }
    }
}
