//! Per-vehicle specification table for every tracked vehicle the game drives (2026-10-28): mass,
//! engine, gearbox, road wheels and suspension, with the source of every number.
//!
//! Tags follow docs/PHYSICAL_TRUTH_AUDIT.md: a figure with a source names it (read 2026-10-28);
//! **[E]** marks an engineering estimate, with its derivation beside it. The suspension figures of
//! WWII tanks are rarely published: where a spring rate or wheel travel could not be found, the
//! rate is derived from a static deflection chosen for a named ride frequency
//! (`f = sqrt(g / d) / 2 pi`), and the damping from a named damping ratio. The ledger row
//! "Vehicle dynamics: per-vehicle spec" lists each estimate.
//!
//! Sources:
//! - **[DW-Tiger]** de.wikipedia "Panzerkampfwagen VI Tiger", data table and running-gear text.
//! - **[EW-Tiger]** en.wikipedia "Tiger I", infobox and suspension text.
//! - **[T1I]** tiger1.info, "Torsion bars" (<https://tiger1.info/EN/Torsion-bars.html>).
//! - **[SPEC]** docs/TIGER_I_SPEC.md (the player Tiger's existing project spec).
//! - **[DW-T34]** de.wikipedia "T-34", data table (76 mm version).
//! - **[RW-T34]** ru.wikipedia "Т-34", ТТХ table.
//! - **[RW-SU100]**, **[RW-KV2]**, **[RW-IS2]**, **[RW-ISU152]**, **[RW-IS3]**: ru.wikipedia
//!   "СУ-100", "КВ-2", "ИС-2", "ИСУ-152", "ИС-3", ТТХ tables and running-gear text.
//!
//! The Tiger II and Panzer IV exist only as editor showcase names (`tank_editor_scene`); nothing in
//! the game drives them, so they have no rows here.
//!
//! In-game geometry: the rendered meshes are larger than the historical vehicles (the Tiger hull
//! mesh is 7.0 m long against 6.316 m [EW-Tiger]; the T-34's footprint is about 1.2 times the
//! real hull). Road-wheel stations are therefore laid out on each mesh's measured track run
//! (the Tiger: `tank_support`'s profile) or on the AI footprint's declared contact span
//! (`vehicle_ground`), so wheels sit where the drawn tracks are. Masses, powers, gears, spring
//! rates and damping are the historical (or estimated) values; ride frequencies come out of them.

use crate::tank::EnemyKind;

pub const G: f32 = 9.81;
/// Shear modulus of spring steel (Pa) [E: handbook value for carbon and alloy steels, 77-81 GPa].
pub const STEEL_SHEAR_PA: f32 = 79.3e9;
/// 1 PS in watts.
pub const PS_W: f32 = 735.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    TigerI,
    T34_76,
    Su100,
    Kv2,
    Is2,
    Isu152,
    Is3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuspensionKind {
    /// Tiger I: eight torsion-bar swing arms per side carrying interleaved (Schachtel) wheels;
    /// hydraulic shock absorbers on the first and last arms.
    InterleavedTorsionBar,
    /// T-34 / SU-100: Christie coil springs in the hull sides, five large wheels, no shock
    /// absorbers ("Schraubenfedern (Christie-Laufwerk) ohne Stoßdämpfer" [DW-T34]).
    ChristieCoil,
    /// KV / IS family: individual torsion bars, six small wheels per side.
    TorsionBar,
}

/// One vehicle's row. Lengths in metres, masses in kg, power in W, speeds in m/s unless named.
#[derive(Debug)]
pub struct VehicleSpec {
    pub model: Model,
    pub name: &'static str,
    pub mass_kg: f32,
    pub engine_w: f32,
    pub rated_rpm: f32,
    pub idle_rpm: f32,
    /// Road speed in each forward gear at rated revolutions (km/h).
    pub forward_kmh: &'static [f32],
    pub reverse_kmh: &'static [f32],
    /// Documented maximum road speed (m/s): the reference the gear table is checked against.
    pub road_speed: f32,
    /// Neutral-to-full-steer yaw rate the steering gear reaches at speed (rad/s) [E].
    pub max_yaw_rate: f32,
    pub suspension: SuspensionKind,
    /// Road-wheel stations per side (swing arms / spring units).
    pub stations: usize,
    pub wheel_diameter_m: f32,
    /// Historical track centre distance and shoe width.
    pub track_gauge_m: f32,
    pub track_width_m: f32,
    /// Track centre half-distance the drivetrain's steering uses: the Tiger's measured mesh
    /// (`tank_support::TIGER_TRACK_X`, which its player drive has always used), else half the
    /// historical gauge.
    pub steer_half_gauge_m: f32,
    /// Historical ground contact length.
    pub contact_length_m: f32,
    /// Overall hull length, width and height (for the inertia estimate).
    pub hull_lwh_m: [f32; 3],
    pub ground_clearance_m: f32,
    /// Height of the centre of gravity above the track bottoms [E unless noted].
    pub cg_height_m: f32,
    /// Share of the combat mass that is unsprung (tracks, wheels, arms) [E].
    pub unsprung_share: f32,
    /// Spring rate at each wheel station (N/m); `None` derives it from `static_deflection_m`.
    pub station_rate_n_m: Option<fn(usize, usize) -> f32>,
    /// Static deflection of a station under its share of the sprung weight [E unless derived].
    pub static_deflection_m: f32,
    /// Wheel travel from static to the bump stop, and from static to full droop [E].
    pub jounce_m: f32,
    pub droop_m: f32,
    /// Damping ratio of every station from arm bearings, spring friction and track [E], and of
    /// the stations with hydraulic shock absorbers (`damper_stations`).
    pub friction_damping: f32,
    pub shock_damping: f32,
    pub damper_stations: &'static [usize],
    /// Obstacle limits: step (wall), trench, gradient.
    pub step_m: f32,
    pub trench_m: f32,
    pub gradient_deg: f32,
}

impl VehicleSpec {
    pub fn sprung_mass(&self) -> f32 {
        self.mass_kg * (1.0 - self.unsprung_share)
    }

    /// Static load on one station of the 2 x `stations`, the sprung weight shared evenly [E: the
    /// centre of gravity over the middle of the contact; real hulls are a little nose- or
    /// tail-heavy].
    pub fn station_load_n(&self) -> f32 {
        self.sprung_mass() * G / (2 * self.stations) as f32
    }

    /// Wheel rate of station `i` (0 = rearmost) of `n`.
    pub fn station_rate(&self, i: usize) -> f32 {
        match self.station_rate_n_m {
            Some(rate) => rate(i, self.stations),
            None => self.station_load_n() / self.static_deflection_m,
        }
    }

    /// Undamped heave frequency (Hz) of the sprung hull on all its stations.
    pub fn heave_hz(&self) -> f32 {
        let k: f32 = (0..self.stations).map(|i| 2.0 * self.station_rate(i)).sum();
        (k / self.sprung_mass()).sqrt() / std::f32::consts::TAU
    }

    /// Pitch and roll moments of inertia of the sprung hull about its centre of gravity
    /// [E: a uniform box of the hull's length, width and height; the real mass sits lower and
    /// more central, so these are upper estimates and the hull pitches a little slower than the
    /// real one].
    pub fn pitch_inertia(&self) -> f32 {
        let [l, _, h] = self.hull_lwh_m;
        self.sprung_mass() * (l * l + h * h) / 12.0
    }
    pub fn roll_inertia(&self) -> f32 {
        let [_, w, h] = self.hull_lwh_m;
        self.sprung_mass() * (w * w + h * h) / 12.0
    }

    pub fn wheel_radius(&self) -> f32 {
        0.5 * self.wheel_diameter_m
    }

    /// Mean ground pressure (kg/cm^2) of the historical contact patch, for checking the row.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn ground_pressure_kg_cm2(&self) -> f32 {
        self.mass_kg / (2.0 * self.track_width_m * 100.0 * self.contact_length_m * 100.0)
    }
}

/// Wheel rate of a torsion-bar swing arm: the bar's torsional stiffness `G pi d^4 / (32 L)` over
/// the arm length squared (the wheel moves on a circle; small-angle rate).
pub fn torsion_bar_wheel_rate(diameter_m: f32, length_m: f32, arm_m: f32) -> f32 {
    let polar = std::f32::consts::PI * diameter_m.powi(4) / 32.0;
    STEEL_SHEAR_PA * polar / length_m / (arm_m * arm_m)
}

/// Tiger I swing-arm length, pivot to wheel axle [E: about half the 0.8 m wheel's diameter plus
/// the hub; drawings show the arm slightly longer than the wheel radius].
pub const TIGER_ARM_M: f32 = 0.41;
/// Tiger I torsion bars [T1I]: 55 mm, the first and last bar of each side 58 mm, 1890 mm long.
pub const TIGER_BAR_D_M: f32 = 0.055;
pub const TIGER_END_BAR_D_M: f32 = 0.058;
pub const TIGER_BAR_L_M: f32 = 1.89;

fn tiger_station_rate(i: usize, n: usize) -> f32 {
    let d = if i == 0 || i + 1 == n { TIGER_END_BAR_D_M } else { TIGER_BAR_D_M };
    torsion_bar_wheel_rate(d, TIGER_BAR_L_M, TIGER_ARM_M)
}

/// Static deflection for a target ride frequency `hz`: `g / (2 pi f)^2`.
pub const fn deflection_for_hz(hz: f32) -> f32 {
    let w = 2.0 * std::f32::consts::PI * hz;
    G / (w * w)
}

pub static TIGER_I: VehicleSpec = VehicleSpec {
    model: Model::TigerI,
    name: "Tiger I",
    // 57.25 t, 650 PS (478 kW) at 3000 rpm [SPEC]; 650 PS / 478 kW and 57 t also [DW-Tiger].
    mass_kg: crate::tank_support::TIGER_MASS_KG,
    engine_w: crate::tank_support::TIGER_ENGINE_W,
    rated_rpm: 3000.0,
    idle_rpm: 800.0,
    // 8 forward / 4 reverse [DW-Tiger, EW-Tiger]; the road speeds per gear are the existing
    // powertrain's documented table (`powertrain`), 45 km/h top [DW-Tiger: "Straße 45 km/h"].
    forward_kmh: &[2.8, 4.2, 6.1, 9.1, 14.0, 20.8, 30.1, 45.0],
    reverse_kmh: &[2.8, 4.4, 7.0, 11.0],
    road_speed: 45.0 / 3.6,
    // The existing player model's 0.7 rad/s [E] (powertrain, unchanged).
    max_yaw_rate: 0.7,
    suspension: SuspensionKind::InterleavedTorsionBar,
    // Eight swing arms per side [T1I, EW-Tiger], 800 mm wheels [EW-Tiger].
    stations: 8,
    wheel_diameter_m: 0.80,
    // 2.82 m centres [E: the commonly quoted figure, not checked this session]; 725 mm combat
    // track [EW-Tiger] (de: "72 cm").
    track_gauge_m: 2.82,
    track_width_m: 0.725,
    steer_half_gauge_m: crate::tank_support::TIGER_TRACK_X,
    // [E] 3.605 m, the commonly quoted figure (not checked this session).
    contact_length_m: 3.605,
    // 6.316 m long, 3.56 m wide [EW-Tiger]; 3.0 m high [E: commonly quoted 2.93-3.0 m].
    hull_lwh_m: [6.316, 3.56, 3.0],
    ground_clearance_m: crate::tank_support::TIGER_GROUND_CLEARANCE_M,
    cg_height_m: crate::tank_support::TIGER_CG_HEIGHT_M,
    // [E] tracks about 2 x 3 t plus 48 wheels and 16 arms; 15 % of the combat mass.
    unsprung_share: 0.15,
    // Derived from the bars [T1I] and the estimated arm: 224 kN/m (275 kN/m at the 58 mm ends),
    // a static deflection of 0.13 m and a 1.37 Hz heave (tests::tiger_rates_come_from_the_bars).
    station_rate_n_m: Some(tiger_station_rate),
    static_deflection_m: 0.0,
    // [E] 0.15 m to the bump stop, 0.10 m of droop: about 0.25 m of total wheel travel, in the
    // range of contemporary torsion-bar heavies.
    jounce_m: 0.15,
    droop_m: 0.10,
    // [E] friction of arm bearings and the steel-rimmed interleaved wheels on the track: 0.06;
    // the hydraulic shock absorbers on the first and last arms [E: the brief and the commonly
    // cited Jentz & Doyle description; not checked this session] at 0.5 of their station's
    // critical damping, which damps pitch most (the end stations have the longest lever).
    friction_damping: 0.06,
    shock_damping: 0.5,
    damper_stations: &[0, 7],
    step_m: crate::tank_support::TIGER_STEP_M,
    trench_m: crate::tank_support::TIGER_TRENCH_M,
    gradient_deg: crate::tank_support::TIGER_GRADIENT_DEG,
};

pub static T34_76: VehicleSpec = VehicleSpec {
    model: Model::T34_76,
    name: "T-34/76",
    // 26.3 t, 500 PS at 1800 rpm, 4 forward + 1 reverse, 55 km/h [DW-T34]; ru gives 26.5-30.9 t
    // and 54 km/h [RW-T34]. The 1942 tank in the game is taken at 28.5 t, mid-range of [RW-T34].
    mass_kg: 28_500.0,
    engine_w: 500.0 * PS_W,
    rated_rpm: 1800.0,
    // [E] V-2 idle.
    idle_rpm: 600.0,
    // [E] gear speeds: a 7.5:1 four-speed tractor box spread geometrically from 7.2 km/h to the
    // documented 54 km/h top; reverse [E] at about first gear.
    forward_kmh: &[7.2, 14.1, 27.6, 54.0],
    reverse_kmh: &[7.0],
    road_speed: 54.0 / 3.6,
    // [E] clutch-and-brake steering: slower, cruder turns than the Tiger's regenerative gear.
    max_yaw_rate: 0.6,
    suspension: SuspensionKind::ChristieCoil,
    // Five wheels per side [DW-T34]; 830 mm wheels [RW-SU100, the same running gear].
    stations: 5,
    wheel_diameter_m: 0.83,
    // [E] 2.45 m centres (commonly quoted; not checked this session); 550 mm track [RW-T34].
    track_gauge_m: 2.45,
    track_width_m: 0.55,
    steer_half_gauge_m: 2.45 / 2.0,
    // [E] from the mean ground pressure: 26.3 t / (2 x 55 cm x 0.64 kg/cm^2) [DW-T34] = 3.74 m.
    contact_length_m: 3.74,
    // [E] 5.92 x 3.0 x 2.4 m, the commonly quoted hull dimensions.
    hull_lwh_m: [5.92, 3.0, 2.4],
    ground_clearance_m: 0.40, // [RW-T34] (de: 38 cm)
    // [E] 0.45 of the overall height (the Tiger's 1.30 m is 0.43 of its 3.0 m).
    cg_height_m: 1.08,
    unsprung_share: 0.14, // [E]
    station_rate_n_m: None,
    // [E] long-travel coil springs: a 1.25 Hz heave, static deflection 0.159 m.
    static_deflection_m: deflection_for_hz(1.25),
    jounce_m: 0.14, // [E] Christie units have long travel (the M1930's 0.35 m total)
    droop_m: 0.09,  // [E]
    // No shock absorbers [DW-T34]: friction of the spring guides and arm bearings only [E].
    friction_damping: 0.08,
    shock_damping: 0.0,
    damper_stations: &[],
    // Wall 0.75 m, trench 3.4 m, 36° [RW-T34].
    step_m: 0.75,
    trench_m: 3.4,
    gradient_deg: 36.0,
};

pub static SU100: VehicleSpec = VehicleSpec {
    model: Model::Su100,
    name: "SU-100",
    // 31.6 t, 500 PS at 1800 rpm, 5-speed, 50 km/h [RW-SU100] (the table's 520 PS conflicts with
    // the text; the text is used).
    mass_kg: 31_600.0,
    engine_w: 500.0 * PS_W,
    rated_rpm: 1800.0,
    idle_rpm: 600.0,
    // [E] five speeds geometric from 6.0 km/h to 50 km/h.
    forward_kmh: &[6.0, 10.3, 17.6, 29.6, 50.0],
    reverse_kmh: &[6.0],
    road_speed: 50.0 / 3.6,
    max_yaw_rate: 0.6, // [E] as the T-34
    suspension: SuspensionKind::ChristieCoil,
    // Five 830 mm wheels per side, Christie [RW-SU100].
    stations: 5,
    wheel_diameter_m: 0.83,
    track_gauge_m: 2.45, // [E] the T-34 chassis
    track_width_m: 0.50, // 500 mm links [RW-SU100]
    steer_half_gauge_m: 2.45 / 2.0,
    // [E] 31.6 t / (2 x 50 cm x 0.80 kg/cm^2) [RW-SU100] = 3.95 m.
    contact_length_m: 3.95,
    hull_lwh_m: [6.10, 3.0, 2.245], // [E] commonly quoted hull length, width, height
    ground_clearance_m: 0.40, // [RW-SU100]
    cg_height_m: 1.01, // [E] 0.45 of the height
    unsprung_share: 0.13, // [E]
    station_rate_n_m: None,
    static_deflection_m: deflection_for_hz(1.25), // [E] as the T-34
    jounce_m: 0.14,
    droop_m: 0.09,
    friction_damping: 0.08,
    shock_damping: 0.0,
    damper_stations: &[],
    // Wall 0.73 m, trench 2.5 m, 35° [RW-SU100].
    step_m: 0.73,
    trench_m: 2.5,
    gradient_deg: 35.0,
};

pub static KV2: VehicleSpec = VehicleSpec {
    model: Model::Kv2,
    name: "KV-2",
    // 52 t; V-2K "500 л. с. (382 кВт) при 1800 об/мин" (the kW figure is used; the table's 600 PS
    // is the later rating); 5-speed tractor-type box; 34 km/h [RW-KV2].
    mass_kg: 52_000.0,
    engine_w: 382_000.0,
    rated_rpm: 1800.0,
    idle_rpm: 600.0,
    // [E] five speeds geometric from 4.5 km/h to 34 km/h.
    forward_kmh: &[4.5, 7.6, 12.8, 20.4, 34.0],
    reverse_kmh: &[4.5],
    road_speed: 34.0 / 3.6,
    max_yaw_rate: 0.45, // [E] clutch-and-brake steering on a 52 t hull
    suspension: SuspensionKind::TorsionBar,
    // Six double-rim wheels per side, torsion bars [RW-KV2]; 600 mm [E: diameter not stated].
    stations: 6,
    wheel_diameter_m: 0.60,
    track_gauge_m: 2.70, // [E]
    track_width_m: 0.70, // [RW-KV2]
    steer_half_gauge_m: 2.70 / 2.0,
    // [E] at an assumed 0.80 kg/cm^2: 52 t / (2 x 70 cm x 0.80) = 4.64 m.
    contact_length_m: 4.64,
    hull_lwh_m: [6.95, 3.32, 3.24], // [E] commonly quoted
    ground_clearance_m: 0.43, // [RW-KV2]
    cg_height_m: 1.46, // [E] 0.45 of the height (the tall turret)
    unsprung_share: 0.14,
    station_rate_n_m: None,
    // [E] torsion bars: a 1.4 Hz heave, static deflection 0.127 m.
    static_deflection_m: deflection_for_hz(1.4),
    jounce_m: 0.12,
    droop_m: 0.08,
    // "internal damping" of the bars only, no shock absorbers [RW-KV2 text]; friction 0.08 [E].
    friction_damping: 0.08,
    shock_damping: 0.0,
    damper_stations: &[],
    // Wall 0.87 m, trench 2.7 m, 36° [RW-KV2].
    step_m: 0.87,
    trench_m: 2.7,
    gradient_deg: 36.0,
};

pub static IS2: VehicleSpec = VehicleSpec {
    model: Model::Is2,
    name: "IS-2",
    // 46 t, 520 PS, 8 forward / 2 reverse (4-speed with range), 37 km/h, torsion bars, six 550 mm
    // wheels, 650 mm track, 0.42 m clearance, 0.8 kg/cm^2, wall 1 m, trench 2.5 m, 36° [RW-IS2].
    mass_kg: 46_000.0,
    engine_w: 520.0 * PS_W,
    // [E] V-2IS rated speed, not stated in [RW-IS2].
    rated_rpm: 2000.0,
    idle_rpm: 600.0,
    // [E] eight speeds geometric from 3.5 km/h to 37 km/h.
    forward_kmh: &[3.5, 4.9, 6.9, 9.6, 13.5, 18.9, 26.4, 37.0],
    reverse_kmh: &[3.5, 7.0],
    road_speed: 37.0 / 3.6,
    max_yaw_rate: 0.5, // [E] two-stage planetary steering
    suspension: SuspensionKind::TorsionBar,
    stations: 6,
    wheel_diameter_m: 0.55,
    track_gauge_m: 2.42, // [E] the ISU-152's [RW-ISU152] on the same chassis
    track_width_m: 0.65,
    steer_half_gauge_m: 2.42 / 2.0,
    // [E] 46 t / (2 x 65 cm x 0.80 kg/cm^2) = 4.42 m.
    contact_length_m: 4.42,
    hull_lwh_m: [6.77, 3.07, 2.73], // [E] commonly quoted
    ground_clearance_m: 0.42,
    cg_height_m: 1.23, // [E] 0.45 of the height
    unsprung_share: 0.14,
    station_rate_n_m: None,
    static_deflection_m: deflection_for_hz(1.4),
    jounce_m: 0.12,
    droop_m: 0.08,
    friction_damping: 0.08, // [E] no shock absorbers in the IS family ([RW-IS3] text)
    shock_damping: 0.0,
    damper_stations: &[],
    step_m: 1.0,
    trench_m: 2.5,
    gradient_deg: 36.0,
};

pub static ISU152: VehicleSpec = VehicleSpec {
    model: Model::Isu152,
    name: "ISU-152",
    // 46 t, 520 PS, 4-speed with range (8F/2R), 40 km/h, torsion bars, six wheels, 650 mm links,
    // gauge 2420 mm, clearance 470 mm, 0.82 kg/cm^2, wall 1.0 m, trench 2.5 m, 36° [RW-ISU152].
    mass_kg: 46_000.0,
    engine_w: 520.0 * PS_W,
    rated_rpm: 2000.0, // [E] as the IS-2
    idle_rpm: 600.0,
    forward_kmh: &[3.8, 5.3, 7.4, 10.4, 14.6, 20.4, 28.6, 40.0], // [E] geometric
    reverse_kmh: &[3.8, 7.5],
    road_speed: 40.0 / 3.6,
    max_yaw_rate: 0.5,
    suspension: SuspensionKind::TorsionBar,
    stations: 6,
    wheel_diameter_m: 0.55, // [E] the IS-2's [RW-IS2]
    track_gauge_m: 2.42,
    track_width_m: 0.65,
    steer_half_gauge_m: 2.42 / 2.0,
    // [E] 46 t / (2 x 65 cm x 0.82 kg/cm^2) = 4.32 m.
    contact_length_m: 4.32,
    hull_lwh_m: [6.77, 3.07, 2.48], // [E] commonly quoted
    ground_clearance_m: 0.47,
    cg_height_m: 1.12,
    unsprung_share: 0.14,
    station_rate_n_m: None,
    static_deflection_m: deflection_for_hz(1.4),
    jounce_m: 0.12,
    droop_m: 0.08,
    friction_damping: 0.08,
    shock_damping: 0.0,
    damper_stations: &[],
    step_m: 1.0,
    trench_m: 2.5,
    gradient_deg: 36.0,
};

pub static IS3: VehicleSpec = VehicleSpec {
    model: Model::Is3,
    name: "IS-3",
    // 49 t, 520 PS, 4-speed with reduction range, 40 km/h, individual torsion bars without shock
    // absorbers, six 550 mm double wheels, 650 mm track, 0.45 m clearance, 0.87 kg/cm^2, wall 1 m,
    // trench 2.5 m, 32° [RW-IS3].
    mass_kg: 49_000.0,
    engine_w: 520.0 * PS_W,
    rated_rpm: 2000.0, // [E]
    idle_rpm: 600.0,
    forward_kmh: &[3.8, 5.3, 7.4, 10.4, 14.6, 20.4, 28.6, 40.0], // [E] as the ISU-152
    reverse_kmh: &[3.8, 7.5],
    road_speed: 40.0 / 3.6,
    max_yaw_rate: 0.5,
    suspension: SuspensionKind::TorsionBar,
    stations: 6,
    wheel_diameter_m: 0.55,
    track_gauge_m: 2.42, // [E] the IS chassis
    track_width_m: 0.65,
    steer_half_gauge_m: 2.42 / 2.0,
    // [E] 49 t / (2 x 65 cm x 0.87 kg/cm^2) = 4.33 m.
    contact_length_m: 4.33,
    hull_lwh_m: [6.77, 3.15, 2.45], // [E] commonly quoted
    ground_clearance_m: 0.45,
    cg_height_m: 1.10,
    unsprung_share: 0.14,
    station_rate_n_m: None,
    static_deflection_m: deflection_for_hz(1.4),
    jounce_m: 0.12,
    droop_m: 0.08,
    friction_damping: 0.08,
    shock_damping: 0.0,
    damper_stations: &[],
    step_m: 1.0,
    trench_m: 2.5,
    gradient_deg: 32.0,
};

pub static ALL: [&VehicleSpec; 7] = [&TIGER_I, &T34_76, &SU100, &KV2, &IS2, &ISU152, &IS3];

pub fn for_kind(kind: EnemyKind) -> &'static VehicleSpec {
    match kind {
        EnemyKind::T34 => &T34_76,
        EnemyKind::Su100 => &SU100,
        EnemyKind::Kv2 => &KV2,
        EnemyKind::Isu152 => &ISU152,
        EnemyKind::Is2 => &IS2,
        EnemyKind::Is3 => &IS3,
    }
}

/// The spec of a game `Tank`: Tigers (the player and the platoon's) carry the legacy `kind`
/// default `T34`, so a Tiger is recognised by its rig (`TIGER_PARTS`' footprint).
pub fn for_tank(kind: EnemyKind, parts: &crate::tank::Parts) -> &'static VehicleSpec {
    if parts.hull_half_extents == crate::tank::TIGER_PARTS.hull_half_extents {
        &TIGER_I
    } else {
        for_kind(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiger_rates_come_from_the_bars() {
        let k = TIGER_I.station_rate(3);
        let end = TIGER_I.station_rate(0);
        assert!((k - 224_000.0).abs() < 4_000.0, "{k}");
        assert!((end / k - (58.0f32 / 55.0).powi(4)).abs() < 1e-3);
        let d = TIGER_I.station_load_n() / k;
        assert!(d > 0.11 && d < 0.15, "static deflection {d}");
        let hz = TIGER_I.heave_hz();
        assert!(hz > 1.2 && hz < 1.6, "{hz} Hz");
    }

    #[test]
    fn every_row_is_consistent() {
        for spec in ALL {
            // The top gear at rated revolutions is the documented road speed (within 2 %).
            let top = spec.forward_kmh.last().unwrap() / 3.6;
            assert!((top - spec.road_speed).abs() / spec.road_speed < 0.02, "{}", spec.name);
            assert!(spec.forward_kmh.windows(2).all(|w| w[1] > w[0]), "{}", spec.name);
            // Heave between 1 and 1.7 Hz, the range of WWII tracked vehicles' ride [E].
            let hz = spec.heave_hz();
            assert!(hz > 1.0 && hz < 1.7, "{} {hz}", spec.name);
            // Ground pressure from the row within the 0.6-1.1 kg/cm^2 of the period's tanks.
            let p = spec.ground_pressure_kg_cm2();
            assert!(p > 0.55 && p < 1.15, "{} {p}", spec.name);
            assert!(spec.droop_m > 0.0 && spec.jounce_m > 0.0);
            assert!(spec.damper_stations.iter().all(|&i| i < spec.stations));
            // Power to weight within the period's 9-20 PS/t.
            let ps_t = spec.engine_w / PS_W / (spec.mass_kg / 1000.0);
            assert!(ps_t > 9.0 && ps_t < 20.0, "{} {ps_t}", spec.name);
        }
    }

    #[test]
    fn a_tiger_is_recognised_by_its_rig_and_soviet_kinds_by_kind() {
        assert_eq!(for_tank(EnemyKind::T34, &crate::tank::TIGER_PARTS).model, Model::TigerI);
        assert_eq!(for_tank(EnemyKind::T34, &crate::tank::T34_PARTS).model, Model::T34_76);
        assert_eq!(for_tank(EnemyKind::Is3, &crate::tank::IS3_PARTS).model, Model::Is3);
    }
    /// The receipt table for docs/VEHICLE_DYNAMICS.md (`cargo test ride_table -- --nocapture`):
    /// each row's ride frequencies on its historical contact patch, static deflection, and its
    /// standing start and stop on the level through `powertrain` and the AI driver.
    #[test]
    fn ride_table() {
        use crate::powertrain::{ai_driver, Controls, Powertrain};
        use crate::suspension::Layout;
        eprintln!("| vehicle | t | PS/t | k station kN/m | static defl. m | heave Hz | pitch Hz | roll Hz | 0-20 km/h s | 60 s speed km/h | stop from 30 km/h m |");
        for spec in ALL {
            let half = 0.5 * spec.contact_length_m;
            let layout = Layout::new(spec, (-half, half), (-half - 0.5, half + 0.5), 0.5 * spec.track_gauge_m, Vec::new());
            let kp: f32 = layout.stations.iter().map(|s| s.k * s.s * s.s).sum();
            let kr: f32 = layout.stations.iter().map(|s| s.k * s.x * s.x).sum();
            let w = layout.mass * G * spec.cg_height_m;
            let pitch_hz = ((kp - w).max(0.0) / layout.pitch_inertia).sqrt() / std::f32::consts::TAU;
            let roll_hz = ((kr - w).max(0.0) / layout.roll_inertia).sqrt() / std::f32::consts::TAU;
            let mut p = Powertrain::for_ai(spec);
            let (mut v, mut t, mut t20) = (0.0f32, 0.0f32, None);
            let controls = |d: f32, b: f32| Controls { drive: d, steer: 0.0, engine_running: true, transmission_ok: true,
                left_track: true, right_track: true, running_gear_factor: 1.0, parking_brake: false, driver_brake: Some(b) };
            while t < 60.0 {
                let (d, b) = ai_driver(spec, 1.0, v);
                v = p.advance(1.0 / 60.0, v, 0.0, controls(d, b)).speed;
                t += 1.0 / 60.0;
                if t20.is_none() && v >= 20.0 / 3.6 { t20 = Some(t); }
            }
            let top = v;
            let mut p = Powertrain::for_ai(spec);
            let mut v = 30.0f32.min(top * 3.6 * 0.99) / 3.6;
            let mut x = 0.0;
            for _ in 0..3600 {
                let (d, b) = ai_driver(spec, 0.0, v);
                let m = p.advance(1.0 / 60.0, v, 0.0, controls(d, b));
                v = m.speed;
                x += m.distance;
                if v <= 0.0 { break; }
            }
            let mean_k = layout.stations.iter().map(|s| s.k).sum::<f32>() / layout.stations.len() as f32;
            eprintln!("| {} | {:.1} | {:.1} | {:.0} | {:.3} | {:.2} | {:.2} | {:.2} | {} | {:.1} | {:.1} |",
                spec.name, spec.mass_kg / 1000.0, spec.engine_w / PS_W / (spec.mass_kg / 1000.0), mean_k / 1000.0,
                spec.station_load_n() / mean_k, spec.heave_hz(), pitch_hz, roll_hz,
                t20.map_or("-".into(), |t| format!("{t:.1}")), top * 3.6, x);
            assert!(pitch_hz > 0.4 && pitch_hz < 1.6, "{} pitch {pitch_hz}", spec.name);
        }
    }
}

