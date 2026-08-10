//! ballistics.rs — the SIMULATION-GRADE gunnery engine. Replaces the hand-authored pen-probability
//! tables (game.rs `*_pen_chance`) with physics: real shells (mass, muzzle velocity, drag, filler)
//! flying with velocity decay + gravity drop, striking real armor PLATES (thickness, slope, material)
//! at a computed obliquity, resolved by a calibrated DeMarre penetration law with slope-overmatch,
//! cap normalization and APCR slope penalty, then post-armor: ballistic-limit residual velocity
//! (Lambert–Jonas) and spall energy. One resolution emits ONE `ImpactEvent` — the shared event that
//! drives the visual FX, the parametric audio (acoustics.rs) and the damage model (game.rs spall).
//!
//! Design notes (docs/ACOUSTICS.md §1, docs/BUILD_ORDER.md Phase 2):
//! - Historical fidelity is CALIBRATED, not simulated ab initio: each shell carries published anchor
//!   points (muzzle velocity + penetration at a reference range/angle); the DeMarre velocity exponent
//!   (~1.43) extrapolates between them. Numbers here are first-pass from standard references
//!   (Bird & Livingston-class figures); the research pass refines them as data lands.
//! - Deterministic: NO `rand`. The only stochastic element (the marginal-penetration zone) hashes an
//!   event seed, so a given shot is reproducible — the AI-training substrate depends on this.
//! - The emergent behavior the design demands falls OUT of the physics (tests assert it):
//!   a T-34's 76mm bounces off a Tiger's front at any range; the 88 kills a T-34 frontally past
//!   1500 m (slope overmatch); APCR wins up close, loses at range and hates sloped plate.

#![allow(dead_code)] // wired into game.rs combat in the integration slice; tests exercise it today.

use glam::Vec3;

// ── Shells ────────────────────────────────────────────────────────────────────────────────────────

/// Terminal-effect class of a shell. Drives penetration law, normalization and post-armor behavior.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShellKind {
    /// Armor-piercing capped, ballistic cap, HE filler + base fuze (PzGr.39, BR-471B…). The killer:
    /// penetrates then DETONATES inside.
    Apcbc,
    /// Uncapped AP / APBC (Soviet BR-350A/B family): shatters on face-hardened plate at high velocity.
    Ap,
    /// Sub-caliber tungsten (PzGr.40, BR-354P): very fast, sheds velocity quickly, poor vs slope,
    /// no filler — post-armor effect is the slug alone.
    Apcr,
    /// High explosive: kills by blast + spall shock, not penetration. The KV-2/ISU-152 argument.
    He,
}

/// One shell type: identity + the physical/calibration constants the solver needs.
#[derive(Clone, Copy, Debug)]
pub struct ShellSpec {
    pub name: &'static str,
    pub kind: ShellKind,
    pub caliber_mm: f32,
    pub mass_kg: f32,
    pub muzzle_velocity: f32, // m/s
    /// HE filler mass (g) — drives the post-penetration burst (APCBC) or the blast (HE).
    pub filler_g: f32,
    /// Exponential velocity-decay constant k (1/m): v(x) = v0·e^(−kx). Fitted per shell from
    /// published striking-velocity/penetration falloff; heavier+pointier = smaller k.
    pub drag_k: f32,
    /// Calibration anchor: penetration (mm RHA) at `ref_range_m` against a plate at 30° from
    /// vertical (the German/most-published test convention).
    pub pen_ref_mm: f32,
    pub ref_range_m: f32,
}

/// DeMarre velocity exponent: pen ∝ v^DEMARRE_N (≈1.4–1.43 in the classical fits).
const DEMARRE_N: f32 = 1.43;

impl ShellSpec {
    /// Striking velocity after `range` metres of flat-fire flight.
    pub fn velocity_at(&self, range: f32) -> f32 {
        self.muzzle_velocity * (-self.drag_k * range.max(0.0)).exp()
    }

    /// Time of flight to `range` (closed form for v = v0·e^(−kx):  t = (e^{kx} − 1)/(k·v0)).
    pub fn time_of_flight(&self, range: f32) -> f32 {
        let k = self.drag_k;
        if k < 1e-9 {
            return range / self.muzzle_velocity;
        }
        ((k * range).exp_m1()) / (k * self.muzzle_velocity)
    }

    /// Gravity drop (m) over `range` under flat-fire assumption (drop = ½gt²) — what the gunner's
    /// elevation must hold over. Also the arc the tracer should visibly fly.
    pub fn drop_at(&self, range: f32) -> f32 {
        let t = self.time_of_flight(range);
        0.5 * 9.81 * t * t
    }

    /// Base flat-plate (0° obliquity) penetration at striking velocity `v`, mm RHA. Calibrated off
    /// the 30°-convention anchor: converted to 0° via the same slope law the solver applies.
    fn pen_flat_at_v(&self, v: f32) -> f32 {
        // Anchor: pen_ref at 30° at ref_range → flat pen at that velocity.
        let v_ref = self.velocity_at(self.ref_range_m);
        let anchor_flat = self.pen_ref_mm * slope_multiplier(self, 30.0, self.pen_ref_mm);
        anchor_flat * (v / v_ref).max(0.0).powf(DEMARRE_N)
    }
}

/// The pen-ray BUDGET (docs/DESTRUCTION.md §3): flat-plate penetration at `range_m`, mm RHA.
/// This is what wall transits spend from — each material's `ap_cost_mm` subtracts, and whatever
/// remains fights the terminal plate. KE shells only (HE never rides the pen ray).
pub fn budget_mm(shell: &'static ShellSpec, range_m: f32) -> f32 {
    shell.pen_flat_at_v(shell.velocity_at(range_m))
}

// ── Armor ─────────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArmorMaterial {
    /// Rolled homogeneous armor — the baseline (quality 1.0).
    Rha,
    /// Soviet cast turrets etc — slightly softer than rolled (quality ~0.94).
    Cast,
    /// German face-hardened plate (early-war): brutal vs uncapped AP (shatter), normal vs capped.
    FaceHardened,
    /// Late-war German plate of degraded quality (Tiger II hulls): quality ~0.92.
    LateGerman,
}

impl ArmorMaterial {
    fn quality(self) -> f32 {
        match self {
            ArmorMaterial::Rha => 1.0,
            ArmorMaterial::Cast => 0.94,
            ArmorMaterial::FaceHardened => 1.0, // vs capped; the shatter check handles uncapped AP
            ArmorMaterial::LateGerman => 0.92,
        }
    }
}

/// One armor plate: thickness, constructive slope (degrees from VERTICAL — 0 = upright wall,
/// 60 = heavily raked like the T-34 glacis), material.
#[derive(Clone, Copy, Debug)]
pub struct Plate {
    pub name: &'static str,
    pub thickness_mm: f32,
    pub slope_deg: f32,
    pub material: ArmorMaterial,
}

/// Which face of the vehicle got hit — the game maps its hit geometry (Arc × entry height) here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Facing {
    HullFront,
    HullSide,
    HullRear,
    TurretFront,
    TurretSide,
    TurretRear,
}

/// A vehicle's armor scheme. Data first-pass from standard references; refined by the research pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArmorScheme {
    TigerI,
    TigerII,
    PantherD,
    PzIII,
    T34_76,
    T34_85,
    Kv1,
    Kv2,
    Is2,
    Is3,
    Su100,
    Isu152,
}

impl ArmorScheme {
    pub fn plate(self, f: Facing) -> Plate {
        use ArmorMaterial::*;
        use ArmorScheme::*;
        let p = |name, t, s, m| Plate {
            name,
            thickness_mm: t,
            slope_deg: s,
            material: m,
        };
        match (self, f) {
            // Tiger I (Ausf. E): the famous vertical slab. Face-hardened early plates.
            (TigerI, Facing::HullFront) => p("Tiger glacis/driver plate", 100.0, 9.0, Rha), // homogeneous rolled (~265-310 BHN), NOT face-hardened
            (TigerI, Facing::HullSide) => p("Tiger hull side", 80.0, 0.0, Rha),
            (TigerI, Facing::HullRear) => p("Tiger hull rear", 80.0, 9.0, Rha),
            (TigerI, Facing::TurretFront) => p("Tiger mantlet", 120.0, 0.0, Rha),
            (TigerI, Facing::TurretSide) => p("Tiger turret side", 80.0, 0.0, Rha),
            (TigerI, Facing::TurretRear) => p("Tiger turret rear", 80.0, 0.0, Rha),
            // Tiger II (Henschel) — the King the player is handed in Act 3.
            (TigerII, Facing::HullFront) => p("Königstiger glacis", 150.0, 50.0, LateGerman),
            (TigerII, Facing::HullSide) => p("Königstiger hull side", 80.0, 25.0, LateGerman),
            (TigerII, Facing::HullRear) => p("Königstiger hull rear", 80.0, 30.0, LateGerman),
            (TigerII, Facing::TurretFront) => {
                p("Königstiger turret front", 180.0, 10.0, LateGerman)
            }
            (TigerII, Facing::TurretSide) => p("Königstiger turret side", 80.0, 21.0, LateGerman),
            (TigerII, Facing::TurretRear) => p("Königstiger turret rear", 80.0, 18.0, LateGerman),
            // Panther D
            (PantherD, Facing::HullFront) => p("Panther glacis", 80.0, 55.0, Rha),
            (PantherD, Facing::HullSide) => p("Panther hull side", 40.0, 40.0, Rha),
            (PantherD, Facing::HullRear) => p("Panther hull rear", 40.0, 30.0, Rha),
            (PantherD, Facing::TurretFront) => p("Panther mantlet", 100.0, 0.0, Rha),
            (PantherD, Facing::TurretSide) => p("Panther turret side", 45.0, 25.0, Rha),
            (PantherD, Facing::TurretRear) => p("Panther turret rear", 45.0, 25.0, Rha),
            // Panzer III (early Ausf.) — Act-1 wreck dressing / early enemies if ever needed.
            (PzIII, Facing::HullFront) => p("Pz III hull front", 50.0, 10.0, FaceHardened), // 1942 uparmored driver plate
            (PzIII, Facing::HullSide) => p("Pz III hull side", 30.0, 0.0, Rha),
            (PzIII, Facing::HullRear) => p("Pz III hull rear", 20.0, 10.0, Rha),
            (PzIII, Facing::TurretFront) => p("Pz III turret front", 57.0, 15.0, FaceHardened),
            (PzIII, Facing::TurretSide) => p("Pz III turret side", 30.0, 25.0, Rha),
            (PzIII, Facing::TurretRear) => p("Pz III turret rear", 30.0, 25.0, Rha),
            // T-34/76: THE sloped-armor argument. 45 mm everywhere, but the glacis at 60°.
            (T34_76, Facing::HullFront) => p("T-34 glacis", 45.0, 60.0, Rha),
            (T34_76, Facing::HullSide) => p("T-34 hull side", 40.0, 40.0, Rha), // 40 mm sponson side (45 is the vertical lower hull)
            (T34_76, Facing::HullRear) => p("T-34 hull rear", 45.0, 47.0, Rha),
            (T34_76, Facing::TurretFront) => p("T-34/76 turret front", 52.0, 30.0, Cast),
            (T34_76, Facing::TurretSide) => p("T-34/76 turret side", 52.0, 30.0, Cast),
            (T34_76, Facing::TurretRear) => p("T-34/76 turret rear", 45.0, 30.0, Cast),
            // T-34/85: same hull, big new turret.
            (T34_85, Facing::HullFront) => p("T-34-85 glacis", 45.0, 60.0, Rha),
            (T34_85, Facing::HullSide) => p("T-34-85 hull side", 40.0, 40.0, Rha),
            (T34_85, Facing::HullRear) => p("T-34-85 hull rear", 45.0, 47.0, Rha),
            (T34_85, Facing::TurretFront) => p("T-34-85 turret front", 90.0, 15.0, Cast),
            (T34_85, Facing::TurretSide) => p("T-34-85 turret side", 75.0, 20.0, Cast),
            (T34_85, Facing::TurretRear) => p("T-34-85 turret rear", 52.0, 10.0, Cast),
            // KV-1 (ZiS-5, 1941): the heavy slab the early 88 was dragged out for.
            (Kv1, Facing::HullFront) => p("KV-1 hull front", 75.0, 30.0, Rha),
            (Kv1, Facing::HullSide) => p("KV-1 hull side", 75.0, 0.0, Rha),
            (Kv1, Facing::HullRear) => p("KV-1 hull rear", 70.0, 15.0, Rha),
            (Kv1, Facing::TurretFront) => p("KV-1 turret front", 95.0, 20.0, Cast), // ZiS-5 cast turret walls 95-110
            (Kv1, Facing::TurretSide) => p("KV-1 turret side", 95.0, 15.0, Cast),
            (Kv1, Facing::TurretRear) => p("KV-1 turret rear", 70.0, 15.0, Cast),
            // KV-2: KV-1 hull under the monstrous box turret. The Act-1 boss.
            (Kv2, Facing::HullFront) => p("KV-2 hull front", 75.0, 30.0, Rha),
            (Kv2, Facing::HullSide) => p("KV-2 hull side", 75.0, 0.0, Rha),
            (Kv2, Facing::HullRear) => p("KV-2 hull rear", 70.0, 15.0, Rha),
            (Kv2, Facing::TurretFront) => p("KV-2 turret front", 75.0, 0.0, Rha),
            (Kv2, Facing::TurretSide) => p("KV-2 turret side", 75.0, 0.0, Rha),
            (Kv2, Facing::TurretRear) => p("KV-2 turret rear", 75.0, 0.0, Rha),
            // IS-2 (m1944 straightened glacis).
            (Is2, Facing::HullFront) => p("IS-2 glacis", 120.0, 60.0, Cast), // m.1944 cast glacis (UZTM welded 90-100 Rha variant exists)
            (Is2, Facing::HullSide) => p("IS-2 hull side", 90.0, 0.0, Rha),
            (Is2, Facing::HullRear) => p("IS-2 hull rear", 60.0, 41.0, Rha),
            (Is2, Facing::TurretFront) => p("IS-2 turret front", 100.0, 20.0, Cast),
            (Is2, Facing::TurretSide) => p("IS-2 turret side", 90.0, 20.0, Cast),
            (Is2, Facing::TurretRear) => p("IS-2 turret rear", 90.0, 25.0, Cast),
            // IS-3: the pike nose — modeled as heavy plate at compound extreme slope.
            (Is3, Facing::HullFront) => p("IS-3 pike nose", 110.0, 66.0, Rha), // pike plates ALSO rotated ~43° in plan → head-on compound obliquity ≈66°
            (Is3, Facing::HullSide) => p("IS-3 hull side", 90.0, 30.0, Rha),
            (Is3, Facing::HullRear) => p("IS-3 hull rear", 60.0, 41.0, Rha),
            (Is3, Facing::TurretFront) => p("IS-3 turret front", 180.0, 35.0, Cast),
            (Is3, Facing::TurretSide) => p("IS-3 turret side", 110.0, 40.0, Cast),
            (Is3, Facing::TurretRear) => p("IS-3 turret rear", 90.0, 30.0, Cast),
            // SU-100 casemate.
            (Su100, Facing::HullFront) => p("SU-100 casemate front", 75.0, 50.0, Rha),
            (Su100, Facing::HullSide) => p("SU-100 side", 45.0, 20.0, Rha),
            (Su100, Facing::HullRear) => p("SU-100 rear", 45.0, 20.0, Rha),
            (Su100, Facing::TurretFront) => p("SU-100 mantlet", 110.0, 0.0, Cast),
            (Su100, Facing::TurretSide) => p("SU-100 side (casemate)", 45.0, 20.0, Rha),
            (Su100, Facing::TurretRear) => p("SU-100 rear (casemate)", 45.0, 20.0, Rha),
            // ISU-152 casemate — the Act-2 boss.
            (Isu152, Facing::HullFront) => p("ISU-152 casemate front", 90.0, 30.0, Rha),
            (Isu152, Facing::HullSide) => p("ISU-152 side", 75.0, 15.0, Rha),
            (Isu152, Facing::HullRear) => p("ISU-152 rear", 60.0, 0.0, Rha),
            (Isu152, Facing::TurretFront) => p("ISU-152 mantlet", 100.0, 0.0, Cast),
            (Isu152, Facing::TurretSide) => p("ISU-152 side (casemate)", 75.0, 15.0, Rha),
            (Isu152, Facing::TurretRear) => p("ISU-152 rear (casemate)", 60.0, 0.0, Rha),
        }
    }
}

// ── The penetration law ───────────────────────────────────────────────────────────────────────────

/// Cap normalization: capped rounds "bite" and rotate a few degrees INTO the plate; uncapped AP
/// normalizes less; APCR digs in worst of all (it skids).
fn normalization_deg(kind: ShellKind) -> f32 {
    match kind {
        ShellKind::Apcbc => 5.0,
        ShellKind::Ap => 3.0,
        ShellKind::Apcr => 1.5,
        ShellKind::He => 0.0,
    }
}

/// Slope-effect exponent by shell type: how savagely obliquity multiplies effective thickness.
/// APCR suffers most on slope; full-caliber capped shot the least.
fn slope_exponent(kind: ShellKind) -> f32 {
    match kind {
        ShellKind::Apcbc => 1.30,
        ShellKind::Ap => 1.40,
        ShellKind::Apcr => 1.75,
        ShellKind::He => 1.0,
    }
}

/// The LOS-thickness multiplier for a plate at `angle_deg` obliquity being struck by `shell`,
/// including the OVERMATCH correction: a shell much wider than the plate (d ≫ t) blunts the slope
/// advantage (the classic 88-vs-T-34-glacis effect); an undermatching shell suffers extra.
fn slope_multiplier(shell: &ShellSpec, angle_deg: f32, plate_mm: f32) -> f32 {
    let a = (angle_deg - normalization_deg(shell.kind))
        .max(0.0)
        .min(80.0);
    let cos = a.to_radians().cos().max(0.17);
    // Overmatch factor: t/d < 1 (shell wider than plate) shrinks the slope exponent toward LOS-only;
    // t/d > 1 grows it mildly. Exponent shape per the empirical overmatch literature.
    let t_over_d = (plate_mm / shell.caliber_mm).clamp(0.2, 3.0);
    let n = slope_exponent(shell.kind) * t_over_d.powf(0.4);
    (1.0 / cos).powf(n.max(0.85))
}

/// Effective thickness (mm RHA-equivalent) presented by `plate` to `shell` when the shot arrives
/// with an extra `horiz_obliquity_deg` off the plate's horizontal normal (compound angle).
pub fn effective_thickness(shell: &ShellSpec, plate: &Plate, horiz_obliquity_deg: f32) -> f32 {
    // Compound impact angle: cos(total) = cos(vertical slope)·cos(horizontal approach).
    let cv = plate.slope_deg.to_radians().cos();
    let ch = horiz_obliquity_deg.clamp(0.0, 75.0).to_radians().cos();
    let total = (cv * ch).clamp(0.05, 1.0).acos().to_degrees();
    plate.thickness_mm
        * plate.material.quality()
        * slope_multiplier(shell, total, plate.thickness_mm)
}

// ── The shared event ──────────────────────────────────────────────────────────────────────────────

/// How the armor answered the shell. One variant per physically distinct outcome — the FX, audio and
/// damage layers each branch off this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Terminal {
    /// Clean ricochet / defeat: the full CLANNNGG, no interior effect.
    Bounce,
    /// The shell SHATTERED on face-hardened plate (uncapped AP, high velocity): a duller crunch.
    Shatter,
    /// Through. Residual fragments + (if filler) the interior burst.
    Penetration,
    /// HE detonation on the plate: blast + interior spall shock without a hole.
    HeBlast,
}

/// THE shared impact event: produced once by the ballistic resolution; consumed by visual FX,
/// parametric audio (acoustics::ImpactParams) and the damage/spall model.
#[derive(Clone, Copy, Debug)]
pub struct ImpactEvent {
    pub shell: &'static ShellSpec,
    pub terminal: Terminal,
    /// Striking velocity (m/s) after flight.
    pub striking_v: f32,
    /// Total impact obliquity (deg from plate normal) after compounding slope + approach.
    pub impact_angle_deg: f32,
    /// The plate that was struck.
    pub plate: Plate,
    /// Effective RHA-equivalent thickness presented (mm).
    pub effective_mm: f32,
    /// The shell's penetration capability at strike (mm).
    pub pen_mm: f32,
    /// Residual velocity of penetrator fragments behind the plate (m/s); 0 unless Penetration.
    pub residual_v: f32,
    /// Post-armor lethality scalar (0..~2): scales the spall cone energy / fragment count, boosted by
    /// filler burst (APCBC) or HE shock. The damage layer maps this onto its spall resolution.
    pub behind_armor: f32,
    /// Time of flight (s) — the audio/FX layers schedule the arrival off this.
    pub flight_time: f32,
    /// Range flown (m) — for the delayed distant gun report.
    pub range_m: f32,
}

/// Deterministic [0,1) hash (project counter-hash pattern).
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

/// Resolve one shot: `shell` fired at a target `range_m` away, striking `plate` with an additional
/// horizontal approach obliquity. `seed` decides the marginal zone deterministically.
pub fn resolve(
    shell: &'static ShellSpec,
    range_m: f32,
    plate: Plate,
    horiz_obliquity_deg: f32,
    seed: u32,
) -> ImpactEvent {
    let v = shell.velocity_at(range_m);
    let eff = effective_thickness(shell, &plate, horiz_obliquity_deg);
    let cv = plate.slope_deg.to_radians().cos();
    let ch = horiz_obliquity_deg.clamp(0.0, 75.0).to_radians().cos();
    let impact_angle = (cv * ch).clamp(0.05, 1.0).acos().to_degrees();
    let flight_time = shell.time_of_flight(range_m);

    // HE: never "penetrates" by KE; it blasts. Overpressure defeats thin plate outright — roughly
    // 18·√(filler kg) mm: the KV-2's 5.1 kg OF-530 breaches ~40 mm (roofs, light hulls), never 100.
    if shell.kind == ShellKind::He {
        let blast_defeat_mm = 18.0 * (shell.filler_g / 1000.0).max(0.0).sqrt();
        let through = plate.thickness_mm < blast_defeat_mm;
        let energy = (shell.filler_g / 1000.0).sqrt() * if through { 1.4 } else { 0.9 };
        return ImpactEvent {
            shell,
            terminal: if through {
                Terminal::Penetration
            } else {
                Terminal::HeBlast
            },
            striking_v: v,
            impact_angle_deg: impact_angle,
            plate,
            effective_mm: eff,
            pen_mm: blast_defeat_mm,
            residual_v: 0.0,
            behind_armor: energy,
            flight_time,
            range_m,
        };
    }

    let pen = shell.pen_flat_at_v(v);

    // Shatter gap: uncapped AP striking face-hardened plate FAST shatters instead of biting.
    if shell.kind == ShellKind::Ap
        && plate.material == ArmorMaterial::FaceHardened
        && v > 0.85 * shell.muzzle_velocity
        && pen < eff * 1.35
    {
        return ImpactEvent {
            shell,
            terminal: Terminal::Shatter,
            striking_v: v,
            impact_angle_deg: impact_angle,
            plate,
            effective_mm: eff,
            pen_mm: pen,
            residual_v: 0.0,
            behind_armor: 0.0,
            flight_time,
            range_m,
        };
    }

    // The marginal zone: within ±8% of exact defeat, real plates and real shells scatter — hash it.
    let margin = pen / eff.max(1.0);
    let penetrated = if margin > 1.08 {
        true
    } else if margin < 0.92 {
        false
    } else {
        // linear odds across the marginal band, decided by the event seed
        let odds = (margin - 0.92) / 0.16;
        hash01(seed) < odds
    };

    if !penetrated {
        return ImpactEvent {
            shell,
            terminal: Terminal::Bounce,
            striking_v: v,
            impact_angle_deg: impact_angle,
            plate,
            effective_mm: eff,
            pen_mm: pen,
            residual_v: 0.0,
            behind_armor: 0.0,
            flight_time,
            range_m,
        };
    }

    // Lambert–Jonas residual velocity: v_r = (v² − v_bl²)^½ (p=2), with the ballistic limit v_bl the
    // velocity at which pen(v_bl) == eff. From the DeMarre law: v_bl = v·(eff/pen)^(1/N).
    let v_bl = v * (eff / pen).max(0.0).powf(1.0 / DEMARRE_N);
    let residual = (v * v - v_bl * v_bl).max(0.0).sqrt() * 0.92;

    // Behind-armor effect: KE fraction through + the filler burst (APCBC's whole argument).
    let ke_frac = (residual / shell.muzzle_velocity).clamp(0.0, 1.0);
    let filler_boost = if shell.filler_g > 10.0 {
        0.55 + (shell.filler_g / 200.0).min(1.0) * 0.45
    } else {
        0.0
    };
    let behind_armor = (0.35 + 0.65 * ke_frac)
        * (1.0 + filler_boost)
        * (shell.mass_kg / 10.0).sqrt().clamp(0.4, 2.0);

    ImpactEvent {
        shell,
        terminal: Terminal::Penetration,
        striking_v: v,
        impact_angle_deg: impact_angle,
        plate,
        effective_mm: eff,
        pen_mm: pen,
        residual_v: residual,
        behind_armor,
        flight_time,
        range_m,
    }
}

/// Horizontal approach obliquity of a shot direction vs a target's facing: the angle (deg) between
/// the reversed shot direction and the struck face's outward normal, in the horizontal plane.
pub fn horiz_obliquity_deg(shot_dir: Vec3, face_normal: Vec3) -> f32 {
    let d = Vec3::new(-shot_dir.x, 0.0, -shot_dir.z).normalize_or_zero();
    let n = Vec3::new(face_normal.x, 0.0, face_normal.z).normalize_or_zero();
    if d == Vec3::ZERO || n == Vec3::ZERO {
        return 0.0;
    }
    d.dot(n).clamp(-1.0, 1.0).acos().to_degrees().min(75.0)
}

// ── The shell catalogue ───────────────────────────────────────────────────────────────────────────
// First-pass anchors from the standard references; the research pass refines. All pen anchors are
// mm RHA at 30° from vertical at the stated reference range.

pub static PZGR_39: ShellSpec = ShellSpec {
    name: "8.8cm PzGr.39",
    kind: ShellKind::Apcbc,
    caliber_mm: 88.0,
    mass_kg: 10.2,
    muzzle_velocity: 773.0,
    filler_g: 59.0,
    drag_k: 1.30e-4,
    pen_ref_mm: 120.0,
    ref_range_m: 100.0,
};
pub static PZGR_40: ShellSpec = ShellSpec {
    name: "8.8cm PzGr.40",
    kind: ShellKind::Apcr,
    caliber_mm: 88.0,
    mass_kg: 7.3,
    muzzle_velocity: 930.0,
    filler_g: 0.0,
    drag_k: 1.60e-4,
    pen_ref_mm: 170.0,
    ref_range_m: 100.0,
};
pub static SPRGR_88: ShellSpec = ShellSpec {
    name: "8.8cm SprGr.",
    kind: ShellKind::He,
    caliber_mm: 88.0,
    mass_kg: 9.0,
    muzzle_velocity: 820.0,
    filler_g: 900.0,
    drag_k: 1.0e-4,
    pen_ref_mm: 20.0,
    ref_range_m: 100.0,
};
/// Tiger II / KwK 43 L/71.
pub static PZGR_39_43: ShellSpec = ShellSpec {
    name: "8.8cm PzGr.39/43",
    kind: ShellKind::Apcbc,
    caliber_mm: 88.0,
    mass_kg: 10.2,
    muzzle_velocity: 1000.0,
    filler_g: 59.0,
    drag_k: 1.55e-4,
    pen_ref_mm: 202.0,
    ref_range_m: 100.0,
};
pub static BR_350B: ShellSpec = ShellSpec {
    name: "76mm BR-350B",
    kind: ShellKind::Ap,
    caliber_mm: 76.2,
    mass_kg: 6.5,
    muzzle_velocity: 655.0,
    filler_g: 65.0,
    drag_k: 1.25e-4,
    pen_ref_mm: 83.0,
    ref_range_m: 100.0,
};
pub static BR_354P: ShellSpec = ShellSpec {
    name: "76mm BR-354P",
    kind: ShellKind::Apcr,
    caliber_mm: 76.2,
    mass_kg: 3.02,
    muzzle_velocity: 950.0,
    filler_g: 0.0,
    drag_k: 2.6e-4,
    pen_ref_mm: 90.0,
    ref_range_m: 100.0,
};
pub static BR_365: ShellSpec = ShellSpec {
    name: "85mm BR-365",
    kind: ShellKind::Apcbc,
    caliber_mm: 85.0,
    mass_kg: 9.2,
    muzzle_velocity: 792.0,
    filler_g: 164.0,
    drag_k: 1.1e-4,
    pen_ref_mm: 105.0,
    ref_range_m: 100.0,
};
pub static BR_412: ShellSpec = ShellSpec {
    name: "100mm BR-412",
    kind: ShellKind::Apcbc,
    caliber_mm: 100.0,
    mass_kg: 15.9,
    muzzle_velocity: 897.0,
    filler_g: 65.0,
    drag_k: 0.95e-4,
    pen_ref_mm: 142.0,
    ref_range_m: 100.0,
};
pub static BR_471: ShellSpec = ShellSpec {
    name: "122mm BR-471",
    kind: ShellKind::Apcbc,
    caliber_mm: 122.0,
    mass_kg: 25.0,
    muzzle_velocity: 781.0,
    filler_g: 156.0,
    drag_k: 0.90e-4,
    pen_ref_mm: 145.0,
    ref_range_m: 100.0,
};
/// KV-2's M-10T 152mm howitzer: HE is the argument.
pub static OF_530: ShellSpec = ShellSpec {
    name: "152mm OF-530",
    kind: ShellKind::He,
    caliber_mm: 152.4,
    mass_kg: 40.0,
    muzzle_velocity: 508.0,
    filler_g: 5900.0,
    drag_k: 1.3e-4,
    pen_ref_mm: 30.0,
    ref_range_m: 100.0,
};
/// ISU-152's ML-20S: the "Zveroboy" AP brick.
pub static BR_540: ShellSpec = ShellSpec {
    name: "152mm BR-540",
    kind: ShellKind::Ap,
    caliber_mm: 152.4,
    mass_kg: 48.8,
    muzzle_velocity: 600.0,
    filler_g: 660.0,
    drag_k: 1.05e-4,
    pen_ref_mm: 115.0,
    ref_range_m: 100.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(
        shell: &'static ShellSpec,
        range: f32,
        scheme: ArmorScheme,
        f: Facing,
        horiz: f32,
    ) -> ImpactEvent {
        resolve(shell, range, scheme.plate(f), horiz, 7)
    }

    #[test]
    fn tiger_front_immune_to_f34() {
        // The design's core asymmetry: the T-34's 76mm CANNOT beat the Tiger's front, even muzzle-close.
        for range in [50.0, 100.0, 300.0, 800.0] {
            let e = ev(&BR_350B, range, ArmorScheme::TigerI, Facing::HullFront, 0.0);
            assert_ne!(
                e.terminal,
                Terminal::Penetration,
                "76mm must not pen Tiger glacis at {range} m (pen {:.0} vs eff {:.0})",
                e.pen_mm,
                e.effective_mm
            );
        }
        // But the Tiger's SIDE falls to it inside ~500 m.
        let side = ev(&BR_350B, 300.0, ArmorScheme::TigerI, Facing::HullSide, 10.0);
        assert_eq!(
            side.terminal,
            Terminal::Penetration,
            "76mm should pen Tiger side at 300 m (pen {:.0} vs eff {:.0})",
            side.pen_mm,
            side.effective_mm
        );
    }

    #[test]
    fn eighty_eight_overmatches_t34_at_range() {
        // The 88 defeats the T-34 glacis far out — the slope can't save 45 mm from an 88 mm shell.
        for range in [100.0, 500.0, 1000.0, 1500.0] {
            let e = ev(&PZGR_39, range, ArmorScheme::T34_76, Facing::HullFront, 0.0);
            assert_eq!(
                e.terminal,
                Terminal::Penetration,
                "88 must pen T-34 glacis at {range} m (pen {:.0} vs eff {:.0})",
                e.pen_mm,
                e.effective_mm
            );
        }
    }

    #[test]
    fn su100_beats_tiger_front_ranged() {
        // The SU-100's 100mm defeats the Tiger front out to long range (the overwatch-sniper enemy).
        let e = ev(&BR_412, 1000.0, ArmorScheme::TigerI, Facing::HullFront, 5.0);
        assert_eq!(
            e.terminal,
            Terminal::Penetration,
            "BR-412 should pen Tiger front at 1 km (pen {:.0} vs eff {:.0})",
            e.pen_mm,
            e.effective_mm
        );
    }

    #[test]
    fn tiger_ii_glacis_shrugs_off_85() {
        let e = ev(&BR_365, 300.0, ArmorScheme::TigerII, Facing::HullFront, 0.0);
        assert_eq!(
            e.terminal,
            Terminal::Bounce,
            "85mm vs Königstiger glacis must bounce (pen {:.0} vs eff {:.0})",
            e.pen_mm,
            e.effective_mm
        );
        // While the IS-2's 122 threatens the King's flank.
        let side = ev(&BR_471, 600.0, ArmorScheme::TigerII, Facing::HullSide, 15.0);
        assert_eq!(
            side.terminal,
            Terminal::Penetration,
            "122 should pen KT side at 600 m (pen {:.0} vs eff {:.0})",
            side.pen_mm,
            side.effective_mm
        );
    }

    #[test]
    fn apcr_wins_close_loses_far_and_hates_slope() {
        // Close + flat: APCR out-penetrates APCBC.
        let close_apcr = ev(&PZGR_40, 100.0, ArmorScheme::Kv1, Facing::HullSide, 0.0);
        let close_ap = ev(&PZGR_39, 100.0, ArmorScheme::Kv1, Facing::HullSide, 0.0);
        assert!(
            close_apcr.pen_mm > close_ap.pen_mm,
            "APCR should out-pen APCBC point-blank"
        );
        // Far: APCR's advantage NARROWS with range (Jentz: 170/110 at 100/2000 vs 120/84 — the
        // ratio compresses; it does not invert on paper).
        let far_apcr = resolve(
            &PZGR_40,
            2000.0,
            ArmorScheme::Kv1.plate(Facing::HullSide),
            0.0,
            3,
        );
        let far_ap = resolve(
            &PZGR_39,
            2000.0,
            ArmorScheme::Kv1.plate(Facing::HullSide),
            0.0,
            3,
        );
        let close_ratio = close_apcr.pen_mm / close_ap.pen_mm;
        let far_ratio = far_apcr.pen_mm / far_ap.pen_mm;
        assert!(
            far_ratio < close_ratio,
            "APCR's edge must shrink with range ({close_ratio:.2} → {far_ratio:.2})"
        );
        // Slope: the T-34 glacis punishes APCR harder than APCBC (relative to their flat pens).
        let t34 = ArmorScheme::T34_76.plate(Facing::HullFront);
        let apcr_ratio = effective_thickness(&PZGR_40, &t34, 0.0) / t34.thickness_mm;
        let ap_ratio = effective_thickness(&PZGR_39, &t34, 0.0) / t34.thickness_mm;
        assert!(
            apcr_ratio > ap_ratio,
            "slope must multiply thickness harder vs APCR ({apcr_ratio:.2} vs {ap_ratio:.2})"
        );
    }

    #[test]
    fn kv2_he_is_terrifying_without_penetrating() {
        // The KV-2's 152 HE doesn't hole the Tiger — it blasts it: big behind-armor shock regardless.
        let e = ev(&OF_530, 400.0, ArmorScheme::TigerI, Facing::HullFront, 0.0);
        assert_eq!(e.terminal, Terminal::HeBlast);
        assert!(
            e.behind_armor > 1.5,
            "5.1 kg of filler should shock hard (got {:.2})",
            e.behind_armor
        );
    }

    #[test]
    fn flight_time_and_drop_grow_superlinearly() {
        let t500 = PZGR_39.time_of_flight(500.0);
        let t1500 = PZGR_39.time_of_flight(1500.0);
        assert!(
            t500 > 0.6 && t500 < 0.75,
            "88 TOF to 500 m ≈ 0.66 s (got {t500:.2})"
        );
        assert!(
            t1500 > 3.0 * t500,
            "drag makes TOF superlinear (t1500 {t1500:.2} vs 3×t500 {:.2})",
            3.0 * t500
        );
        assert!(
            PZGR_39.drop_at(1500.0) > 9.0 * PZGR_39.drop_at(500.0) * 0.9,
            "drop ~ t² — the arc the player must hold over"
        );
    }

    #[test]
    fn penetration_margin_drives_residual() {
        // Overwhelming penetration → high residual; marginal penetration → weak residual.
        let easy = ev(&PZGR_39, 100.0, ArmorScheme::T34_76, Facing::HullSide, 0.0);
        let hard = ev(
            &BR_412,
            1400.0,
            ArmorScheme::TigerI,
            Facing::HullFront,
            10.0,
        );
        assert_eq!(easy.terminal, Terminal::Penetration);
        if hard.terminal == Terminal::Penetration {
            assert!(
                easy.residual_v > hard.residual_v,
                "overwhelming pen must retain more residual ({:.0} vs {:.0})",
                easy.residual_v,
                hard.residual_v
            );
        }
        assert!(
            easy.behind_armor > 0.5,
            "APCBC filler burst should spall hard"
        );
    }

    #[test]
    fn shatter_on_face_hardened() {
        // Uncapped Soviet AP at point-blank velocity vs face-hardened plate in the marginal zone →
        // the shatter gap. (Tiger plate is homogeneous — calibrated 2026-07-01 — so the test uses an
        // explicit FH plate of the Pz III class scaled to the marginal band.)
        let fh = Plate {
            name: "test FH 80",
            thickness_mm: 80.0,
            slope_deg: 10.0,
            material: ArmorMaterial::FaceHardened,
        };
        let e = resolve(&BR_350B, 60.0, fh, 0.0, 7);
        assert!(matches!(e.terminal, Terminal::Shatter | Terminal::Bounce),
            "uncapped AP fast vs FH plate in the marginal band must not cleanly penetrate (got {:?}, pen {:.0} vs eff {:.0})", e.terminal, e.pen_mm, e.effective_mm);
    }

    #[test]
    fn obliquity_helper_sane() {
        // Head-on: 0°. 45° approach: 45°.
        assert!(horiz_obliquity_deg(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.0, -1.0)) < 0.5);
        let ob = horiz_obliquity_deg(Vec3::new(0.7071, 0.0, 0.7071), Vec3::new(0.0, 0.0, -1.0));
        assert!(
            (ob - 45.0).abs() < 1.0,
            "45° approach should read ~45° (got {ob:.1})"
        );
    }
}
