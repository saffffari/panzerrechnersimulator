//! The gun's fumes in the Tiger's crew compartment (2026-11-03; the owner: a light haze leaving the
//! breech as the spent case is pulled, the case venting for about three seconds wherever it lies,
//! many shots stacking, the haze drawn out slowly by the fan up top where the light comes in).
//!
//! A two-zone model of the compartment's air, the zone method of enclosure smoke models: fresh fume
//! is hot and buoyant, so it rises into an upper layer under the turret roof, from which the roof
//! ventilator draws. The air that replaces what the fan draws comes in low and rises through the
//! interface; a slower exchange mixes the two layers. Amounts are litres of undiluted fume at the
//! compartment's temperature; a concentration's visible extinction is `FUME_EXTINCTION_PER_M` times
//! its undiluted share.
//!
//! Sources: the breech's blowback when the block opens on a fired case, drawn as the breech puff
//! (`breech_extinction`; rm_medium.wgsl `rm_gas_density`) and handed to the upper layer as the puff
//! spreads; the spent case venting from its mouth (`Vent`), drawn as a buoyant plume (rm_medium.wgsl
//! `rm_case_plume`) and fed to the upper layer while it is aboard. Sinks: the ventilator while it
//! runs, the hatches that stand open, the hull's leakage.
//!
//! The Tiger's own measurement (Foreign Military Studies P-002, "Tank ventilation", via
//! kamenn.eu): with the roof ventilator drawing, hatches shut and the empties left inside, carbon
//! monoxide stood at 0.05-0.1 % after each shot and fell to practically nothing one to two minutes
//! after firing stopped; lidding each case as it came out improved it (the cases keep venting).
//! The powder gas is about half carbon monoxide. Tests hold this model to both figures.

use glam::{Mat4, Vec2, Vec3};

/// Free air in the crew compartments with the turret, m³ (`fea_damage`'s named estimate).
pub const CREW_AIR_M3: f32 = crate::fea_damage::compartment::TIGER_CREW_AIR_M3;
/// The upper layer's lower edge, turret frame units: a little above the breech (the trunnion at
/// 1.845, the breech's top near 1.93) and 0.23-0.30 below the roof (named estimate).
pub const LAYER_Y: f32 = 2.00;
/// Half the interface's thickness, turret frame units (named estimate: a mixed band ~15 cm deep).
pub const LAYER_HALF: f32 = 0.08;
/// The upper layer's share of the free air: the source compartments' air above `LAYER_Y` over all
/// of it (`cabin_medium::Medium::volume_split`; test `the_upper_share_is_the_source_compartments`).
pub const UPPER_SHARE: f32 = 0.088;

/// The turret ventilator's rated flow: 12 m³ of air a minute (the Tiger's turret manual, via
/// tiger1.info, "First kind of ventilator fan").
pub const FAN_M3_S: f32 = 12.0 / 60.0;
/// The ventilator's intake, turret frame: the underside of the source's fan housing (Turret_0118,
/// `interior_rig::TURRET_FAN_UNDERSIDE_Y`) at the dome lamp's mount under it (`lamp_mounts`).
pub const FAN_INTAKE: Vec3 = Vec3::new(-0.012_648_94, crate::interior_rig::TURRET_FAN_UNDERSIDE_Y, -0.002_224_71);
/// The intake's radius, m: the sink flow is held at its speed there nearer in (named estimate).
pub const FAN_RADIUS_M: f32 = 0.10;
/// Exchange between the layers, m³/s (named estimate: the crew's movement, the gun's recoil and the
/// cooling of the fume stir about a fifth of the upper layer's air a second).
pub const MIX_M3_S: f32 = 0.05;
/// The buttoned hull's leakage, m³/s: half an air change an hour (named estimate).
pub const LEAK_M3_S: f32 = CREW_AIR_M3 * 0.5 / 3600.0;
/// An open hatch's exchange with the outside air, m³/s (named estimate: the buoyant exchange through
/// a 0.5 m round opening, (C_d A / 3) sqrt(g D Δρ/ρ) with C_d 0.6, A 0.2 m² and Δρ/ρ 0.15 for warm
/// cabin air under winter air, is 0.035 m³/s; rounded up for the wind over the hatch).
pub const HATCH_M3_S: f32 = 0.05;

/// The bore's gas that leaves by the breech when the block opens, litres at the compartment's
/// temperature (named estimate: the 8.8 cm L/56 bore, 4.93 m of 88 mm, 30 L, and a 4 L chamber, full
/// of gas at ambient pressure near 1,000 K is about 10 L at 293 K; half of it leaves rearward).
pub const BREECH_FUME_L: f32 = 5.0;
/// The spent case's gas, litres at the compartment's temperature (named estimate: its 5.5 L cavity of
/// hot gas, about 1.6 L at 293 K, and the first seconds of its residue's outgassing).
pub const CASE_FUME_L: f32 = 3.0;
/// The powder gas's carbon monoxide share (P-002: "about 50 percent").
#[cfg_attr(not(test), allow(dead_code))]
pub const POWDER_GAS_CO_SHARE: f32 = 0.5;
/// The visible extinction of undiluted gun fume, /m (named estimate; no measured smoke yield was
/// found). Its smoke is the flash reducer's potassium salts and the charge's residue: taking 0.5 % of
/// the Pzgr 39's 2.5 kg charge (JH.Dv. 481/60, via Wikipedia) as smoke-forming, 12.5 g a shot, and
/// the compartment's share as its 8 L of the charge's ~2,200 L of gas at 293 K (0.36 %), about 45 mg
/// a shot comes inside; at a fine salt smoke's ~3 m²/g that is 0.135 m² of cross-section in 8 L.
/// A case mouth's 8 cm of it is nearly opaque; one shot spread through the compartment, 0.014 /m.
pub const FUME_EXTINCTION_PER_M: f32 = 17.0;

/// The case's venting e-folding time, s: the owner's "about 3 s" (95 % gone by then). A buoyant
/// exchange through the 81 mm mouth, (C_d / 3) d^2.5 sqrt(g Δρ/ρ), is about a litre a second for
/// gas at a few hundred degrees: the same order (named estimate).
pub const CASE_VENT_TAU_S: f32 = 1.0;
/// When a vent ends and its last few percent join the upper layer at once, s.
pub const CASE_VENT_S: f32 = 3.5;
/// The case mouth's inner radius, m (`procedural_ordnance::spent_case_mesh`).
pub const CASE_MOUTH_RADIUS_M: f32 = crate::procedural_ordnance::CASE_MOUTH_INNER_RADIUS_M;
/// A pure plume's entrainment coefficient (Morton, Taylor & Turner 1956; 0.08-0.12 measured): its
/// half-width grows by 1.2 alpha per metre of rise. rm_medium.wgsl `rm_case_plume` uses the same.
#[cfg_attr(not(test), allow(dead_code))]
pub const PLUME_ALPHA: f32 = 0.1;
/// The plume's rise speed, m/s, against which the fan's sink flow tilts it (named estimate).
pub const PLUME_RISE_M_S: f32 = 0.3;
/// The most cases venting at once (a vent lasts 3.5 s; the fastest reload takes longer).
pub const MAX_VENTS: usize = 4;

/// The breech puff's fume joins the upper layer with this e-folding time as the puff spreads, s
/// (named estimate: 4 % left when the drawn puff ends at `BREECH_PUFF_S`).
pub const BREECH_MIX_TAU_S: f32 = 0.7;
/// How long the breech puff is drawn, s (game `BREECH_GAS_DUR`; rm_medium.wgsl `rm_gas_density`).
pub const BREECH_PUFF_S: f32 = 2.2;
/// The drawn puff's semi-axes along and across its axis, m, as rm_medium.wgsl `rm_gas_density`
/// grows them: (0.16 + 0.16 age) and (0.09 + 0.10 age).
pub fn breech_puff_axes(age: f32) -> (f32, f32) { (0.16 + 0.16 * age, 0.09 + 0.10 * age) }
/// The puff profile's moment, the integral over the unit ball's radius of s² (1 - smoothstep(0.18,
/// 1, s)): its volume-weighted fill is 4 pi a r² times this (test `the_puff_moment_is_its_profile`).
pub const PUFF_PROFILE_MOMENT: f32 = 0.088_35;
/// rm_medium.wgsl's puff density to extinction (`rm_puff_extinction`, the shared smoke coefficient).
pub const PUFF_DENSITY_TO_EXTINCTION: f32 = 5.2;

/// The shader's lanes: layers (lower and upper extinction /m, the interface's turret-frame height and
/// half thickness); the fan's intake (world) and the vents drawn; per vent its mouth (world) and
/// plume strength (extinction /m at the mouth), then its drift per metre of rise (x, z), its reach up
/// to the layer (y) and the mouth's radius.
pub const LANES: usize = 2 + 2 * MAX_VENTS;

/// A spent case venting from its mouth.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Vent {
    pub active: bool,
    /// The casing body it is (`Game::casings` index).
    pub key: usize,
    pub age: f32,
    /// Fume still in the case, litres.
    pub left_l: f32,
    /// Its mouth, world, once posed.
    pub mouth: Option<Vec3>,
    /// Inside the compartment: its fume joins the cabin air.
    pub aboard: bool,
}

/// The compartment's air this step.
#[derive(Clone, Copy, Debug, Default)]
pub struct Air {
    /// The roof ventilator runs.
    pub fan: bool,
    /// Hatches open on the upper layer (the cupola, the loader's, the turret's escape hatch).
    pub upper_hatches: u32,
    /// Hatches open on the lower layer (the driver's, the radio operator's).
    pub lower_hatches: u32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CabinFumes {
    /// Undiluted fume in the upper layer and in the lower, litres.
    pub upper_l: f32,
    pub lower_l: f32,
    /// The breech puff's fume not yet spread into the layer, litres, and the puff's age, s.
    breech_l: f32,
    breech_age: f32,
    vents: [Vent; MAX_VENTS],
    /// The fan's flow last step, m³/s (for the sink flow drawn).
    fan_m3_s: f32,
}

/// The fan's sink flow at `p`, m/s: a point sink in the roof draws through a hemisphere, Q / (2 pi r²),
/// held at the intake's radius nearer in (named approximation: the roof is taken as flat and open).
pub fn sink_velocity(p: Vec3, intake: Vec3, q_m3_s: f32) -> Vec3 {
    let to = intake - p;
    let d = to.length();
    if q_m3_s <= 0.0 || d < 1e-6 { return Vec3::ZERO; }
    let r = d.max(FAN_RADIUS_M);
    to / d * (q_m3_s / (std::f32::consts::TAU * r * r))
}

impl CabinFumes {
    fn volumes_m3() -> (f32, f32) {
        let upper = CREW_AIR_M3 * UPPER_SHARE;
        (upper, CREW_AIR_M3 - upper)
    }

    /// The block opens on a fired case: the bore's blowback leaves the breech as a puff.
    pub fn breech_opened(&mut self) {
        // A previous puff's remainder has spread by now.
        self.upper_l += self.breech_l;
        self.breech_l = BREECH_FUME_L;
        self.breech_age = 0.0;
    }

    /// The spent case is out of the breech as casing body `key`: it vents from its mouth.
    pub fn case_freed(&mut self, key: usize) {
        let slot = self.vents.iter().position(|v| v.active && v.key == key)
            .or_else(|| self.vents.iter().position(|v| !v.active))
            .unwrap_or_else(|| (0..MAX_VENTS).max_by(|&a, &b| self.vents[a].age.total_cmp(&self.vents[b].age)).unwrap_or(0));
        let old = self.vents[slot];
        if old.active && old.aboard { self.upper_l += old.left_l; }
        self.vents[slot] = Vent { active: true, key, age: 0.0, left_l: CASE_FUME_L, mouth: None, aboard: true };
    }

    /// Advance by `dt`. `pose` gives a venting case's mouth (world) and whether it is inside the
    /// compartment, or `None` once its body is gone.
    pub fn step(&mut self, dt: f32, air: Air, mut pose: impl FnMut(usize) -> Option<(Vec3, bool)>) {
        if !(dt > 0.0) { return; }
        let mut source = 0.0;
        if self.breech_l > 0.0 {
            let out = self.breech_l * (1.0 - (-dt / BREECH_MIX_TAU_S).exp());
            self.breech_l -= out;
            source += out;
            self.breech_age += dt;
            if self.breech_age >= BREECH_PUFF_S {
                source += self.breech_l;
                self.breech_l = 0.0;
            }
        }
        for v in self.vents.iter_mut().filter(|v| v.active) {
            match pose(v.key) {
                Some((mouth, aboard)) => { v.mouth = Some(mouth); v.aboard = aboard; }
                None => {
                    if v.aboard { source += v.left_l; }
                    *v = Vent::default();
                    continue;
                }
            }
            let out = v.left_l * (1.0 - (-dt / CASE_VENT_TAU_S).exp());
            v.left_l -= out;
            v.age += dt;
            if v.aboard { source += out; }
            if v.age >= CASE_VENT_S {
                if v.aboard { source += v.left_l; }
                *v = Vent::default();
            }
        }
        // Fresh fume is hot: it rises into the upper layer.
        self.upper_l += source;
        let (vu, vl) = Self::volumes_m3();
        self.fan_m3_s = if air.fan { FAN_M3_S } else { 0.0 };
        // What the fan and the leaks draw out high comes in low and rises through the interface.
        let through = self.fan_m3_s + LEAK_M3_S;
        let q_upper = air.upper_hatches as f32 * HATCH_M3_S;
        let q_lower = air.lower_hatches as f32 * HATCH_M3_S;
        let steps = (dt / 0.05).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            let (cu, cl) = (self.upper_l / vu, self.lower_l / vl);
            let du = through * cl - (through + q_upper) * cu + MIX_M3_S * (cl - cu);
            let dl = -(through + q_lower) * cl + MIX_M3_S * (cu - cl);
            self.upper_l = (self.upper_l + du * h).max(0.0);
            self.lower_l = (self.lower_l + dl * h).max(0.0);
        }
    }

    /// The layers' extinction, /m: (lower, upper).
    pub fn extinction(&self) -> (f32, f32) {
        let (vu, vl) = Self::volumes_m3();
        let k = FUME_EXTINCTION_PER_M / 1000.0;
        (self.lower_l / vl * k, self.upper_l / vu * k)
    }

    /// The compartment's mean fume extinction, /m (both layers mixed).
    pub fn mean_extinction(&self) -> f32 {
        (self.upper_l + self.lower_l) / CREW_AIR_M3 * FUME_EXTINCTION_PER_M / 1000.0
    }

    /// All the fume aboard, litres: both layers, the puff and the cases still venting inside.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn total_l(&self) -> f32 {
        self.upper_l + self.lower_l + self.breech_l
            + self.vents.iter().filter(|v| v.active && v.aboard).map(|v| v.left_l).sum::<f32>()
    }

    /// The breech puff's age, s, while it is drawn.
    pub fn breech_age(&self) -> Option<f32> {
        (self.breech_l > 0.0 && self.breech_age < BREECH_PUFF_S).then_some(self.breech_age)
    }

    /// The breech puff's extinction at its centre, /m: its unspread fume over the drawn puff's
    /// volume-weighted fill, undiluted at most.
    pub fn breech_extinction(&self) -> f32 {
        let Some(age) = self.breech_age() else { return 0.0; };
        let (a, r) = breech_puff_axes(age);
        let fill_m3 = 4.0 * std::f32::consts::PI * a * r * r * PUFF_PROFILE_MOMENT;
        FUME_EXTINCTION_PER_M * (self.breech_l / 1000.0 / fill_m3).min(1.0)
    }

    /// The breech puff's drift toward the fan by its age, m (world), from the sink flow at `at`.
    pub fn breech_drift(&self, at: Vec3, turret: Mat4) -> Vec3 {
        let age = self.breech_age().unwrap_or(0.0);
        sink_velocity(at, turret.transform_point3(FAN_INTAKE), self.fan_m3_s) * age
    }

    pub fn vents(&self) -> impl Iterator<Item = &Vent> {
        self.vents.iter().filter(|v| v.active)
    }

    /// The renderer's lanes for this pose of the turret (`LANES`).
    pub fn lanes(&self, turret: Mat4) -> [[f32; 4]; LANES] {
        let mut out = [[0.0; 4]; LANES];
        let (lower, upper) = self.extinction();
        out[0] = [lower, upper, LAYER_Y, LAYER_HALF];
        let intake = turret.transform_point3(FAN_INTAKE);
        let layer_y = turret.transform_point3(Vec3::new(FAN_INTAKE.x, LAYER_Y, FAN_INTAKE.z)).y;
        let mut drawn = 0;
        for v in self.vents() {
            let Some(mouth) = v.mouth else { continue; };
            // A pure plume's concentration goes as its source flux to the two-thirds.
            let strength = FUME_EXTINCTION_PER_M * (-2.0 * v.age / (3.0 * CASE_VENT_TAU_S)).exp();
            let drift = sink_velocity(mouth, intake, self.fan_m3_s);
            let tilt = (Vec2::new(drift.x, drift.z) / PLUME_RISE_M_S).clamp_length_max(1.0);
            let reach = (layer_y - mouth.y).clamp(0.15, 0.8);
            out[2 + 2 * drawn] = [mouth.x, mouth.y, mouth.z, strength];
            out[3 + 2 * drawn] = [tilt.x, reach, tilt.y, CASE_MOUTH_RADIUS_M];
            drawn += 1;
        }
        out[1] = [intake.x, intake.y, intake.z, drawn as f32];
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(f: &mut CabinFumes, key: usize, air: Air) {
        f.breech_opened();
        f.case_freed(key);
        run(f, 0.1, air);
    }

    fn run(f: &mut CabinFumes, secs: f32, air: Air) {
        let n = (secs / (1.0 / 60.0)).round() as usize;
        for _ in 0..n { f.step(1.0 / 60.0, air, |_| Some((Vec3::new(0.3, 1.2, -0.4), true))); }
    }

    const FAN: Air = Air { fan: true, upper_hatches: 0, lower_hatches: 0 };
    const STILL: Air = Air { fan: false, upper_hatches: 0, lower_hatches: 0 };

    #[test]
    fn a_shot_gives_the_tigers_measured_carbon_monoxide() {
        // P-002: 0.05-0.1 % after each shot (with machine-gun fire in some series); one shot's fume
        // well mixed through the free air must be of that order (a factor of two below allowed).
        let co_percent = (BREECH_FUME_L + CASE_FUME_L) * POWDER_GAS_CO_SHARE / (CREW_AIR_M3 * 1000.0) * 100.0;
        assert!((0.025..=0.1).contains(&co_percent), "{co_percent} %");
    }

    #[test]
    fn a_case_vents_for_about_three_seconds_wherever_it_lies() {
        let mut f = CabinFumes::default();
        f.case_freed(7);
        let at = |f: &CabinFumes| f.vents().next().map(|v| v.left_l);
        run(&mut f, 1.0, STILL);
        assert!(at(&f).unwrap() > CASE_FUME_L * 0.3, "still venting at 1 s");
        run(&mut f, 2.0, STILL);
        assert!(at(&f).unwrap() < CASE_FUME_L * 0.06, "nearly spent at 3 s");
        run(&mut f, 0.6, STILL);
        assert!(at(&f).is_none(), "the vent ends");
        // Out of the compartment its fume is not the cabin's; a body gone gives up its vent.
        let mut out = CabinFumes::default();
        out.case_freed(3);
        for _ in 0..60 { out.step(1.0 / 60.0, STILL, |_| Some((Vec3::ZERO, false))); }
        assert!(out.upper_l + out.lower_l < 1e-6);
        let mut gone = CabinFumes::default();
        gone.case_freed(3);
        gone.step(0.1, STILL, |_| None);
        assert!(gone.vents().next().is_none());
    }

    #[test]
    fn the_fume_is_conserved_until_the_air_carries_it_out() {
        let mut f = CabinFumes::default();
        shot(&mut f, 1, STILL);
        run(&mut f, 6.0, STILL);
        let emitted = BREECH_FUME_L + CASE_FUME_L;
        // Only the hull's leakage (half an air change an hour) has drawn any out in six seconds.
        assert!((f.total_l() - emitted).abs() < emitted * 0.01, "{} of {emitted}", f.total_l());
    }

    #[test]
    fn hot_fume_lies_under_the_roof_first() {
        let mut f = CabinFumes::default();
        shot(&mut f, 1, FAN);
        run(&mut f, 3.0, FAN);
        let (lower, upper) = f.extinction();
        assert!(upper > 3.0 * lower, "upper {upper} lower {lower}");
    }

    #[test]
    fn shots_stack_and_the_fan_clears_them_in_one_to_two_minutes() {
        let mut f = CabinFumes::default();
        shot(&mut f, 0, FAN);
        run(&mut f, 3.0, FAN);
        let first = f.mean_extinction();
        for k in 1..5 {
            run(&mut f, 5.0, FAN);
            shot(&mut f, k, FAN);
        }
        run(&mut f, 3.0, FAN);
        let peak = f.mean_extinction();
        // With the fan drawing the roof layer out, five shots stack to over twice one.
        assert!(peak > 2.0 * first, "five shots stack: {first} -> {peak}");
        // P-002: practically nothing one to two minutes after firing stopped.
        let mut t = 0.0;
        while f.mean_extinction() > 0.05 * peak && t < 600.0 { run(&mut f, 1.0, FAN); t += 1.0; }
        assert!((45.0..=150.0).contains(&t), "cleared to 5 % in {t} s");
        // Buttoned up with the engine (and so the fan) off, the fume stays.
        let mut still = CabinFumes::default();
        for k in 0..5 { shot(&mut still, k, STILL); run(&mut still, 8.0, STILL); }
        let held = still.mean_extinction();
        run(&mut still, 120.0, STILL);
        assert!(still.mean_extinction() > 0.8 * held);
    }

    #[test]
    fn an_open_hatch_clears_faster_than_the_fan_alone() {
        let air = Air { fan: true, upper_hatches: 1, lower_hatches: 0 };
        let (mut fan, mut open) = (CabinFumes::default(), CabinFumes::default());
        for k in 0..3 { shot(&mut fan, k, FAN); shot(&mut open, k, air); }
        run(&mut fan, 30.0, FAN);
        run(&mut open, 30.0, air);
        assert!(open.mean_extinction() < fan.mean_extinction());
    }

    #[test]
    fn the_breech_puff_is_light_finite_and_spreads() {
        let mut f = CabinFumes::default();
        f.breech_opened();
        let early = f.breech_extinction();
        assert!(early > 0.0 && early <= FUME_EXTINCTION_PER_M);
        run(&mut f, 1.0, FAN);
        let later = f.breech_extinction();
        assert!(later < 0.5 * early, "it thins as it spreads: {early} -> {later}");
        run(&mut f, 1.3, FAN);
        assert_eq!(f.breech_extinction(), 0.0);
        assert!(f.breech_age().is_none());
    }

    #[test]
    fn the_fan_draws_the_haze_toward_its_intake() {
        let intake = Vec3::new(0.0, 2.2, 0.0);
        let v = sink_velocity(Vec3::new(0.4, 1.9, 0.0), intake, FAN_M3_S);
        assert!(v.dot(intake - Vec3::new(0.4, 1.9, 0.0)) > 0.0);
        // 0.2 m³/s through a 0.5 m hemisphere: 0.13 m/s.
        assert!((v.length() - FAN_M3_S / (std::f32::consts::TAU * 0.25)).abs() < 1e-3);
        assert_eq!(sink_velocity(Vec3::ZERO, intake, 0.0), Vec3::ZERO);
        let mut f = CabinFumes::default();
        f.case_freed(2);
        f.step(0.1, FAN, |_| Some((Vec3::new(0.3, 1.5, 0.0), true)));
        let lanes = f.lanes(Mat4::IDENTITY);
        assert_eq!(lanes[1][3], 1.0);
        assert!(lanes[2][3] > 0.0 && lanes[2][3] <= FUME_EXTINCTION_PER_M);
        assert!(lanes[3][0] < 0.0, "the plume leans toward the intake");
        assert!((lanes[3][1] - (LAYER_Y - 1.5)).abs() < 1e-4, "it rises to the layer");
    }

    #[test]
    fn the_puff_moment_is_its_profile() {
        let smooth = |x: f32| { let t = ((x - 0.18) / 0.82).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) };
        let n = 20_000;
        let moment: f32 = (0..n).map(|i| { let s = (i as f32 + 0.5) / n as f32; s * s * (1.0 - smooth(s)) / n as f32 }).sum();
        assert!((moment - PUFF_PROFILE_MOMENT).abs() < 2e-4, "{moment}");
    }

    #[test]
    fn the_shader_draws_the_same_puff_and_plume() {
        let medium = include_str!("../shaders/rm_medium.wgsl");
        for needle in ["(0.16 + 0.16 * age)", "(0.09 + 0.10 * age)", "smoothstep(0.18, 1.0, shape)",
            "rm_gas_density(p) * 5.2", "RM_PLUME_ALPHA: f32 = 0.1;", "RM_CASE_VENTS: u32 = 4u;"] {
            assert!(medium.contains(needle), "rm_medium.wgsl lacks {needle}");
        }
        assert_eq!(MAX_VENTS, 4);
        assert_eq!(PLUME_ALPHA, 0.1);
        let common = include_str!("../shaders/rm_common.wgsl");
        assert!(common.contains(&format!("fumes: array<vec4<f32>, {LANES}>")));
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;

    /// Needs the Tiger's source GLBs (`KESSEL_ASSET_ROOT`), as the cabin medium's own tests.
    #[test]
    fn the_upper_share_is_the_source_compartments() {
        let medium = crate::renderer::cabin_medium::Medium::load().unwrap();
        let (above, total) = medium.volume_split(LAYER_Y);
        let share = above / total;
        println!("[cabin-fumes] air above {LAYER_Y}: {above:.3} of {total:.3} frame units³, share {share:.3}");
        assert!((share - UPPER_SHARE).abs() < 0.015, "measured share {share:.3}, constant {UPPER_SHARE}");
    }
}
