//! Wounds with a time course, the vehicle's first-aid kit, and the hands-on procedures that
//! treat them.
//!
//! One `Casualty` per crewman. Hits produce wounds (bleeding, fracture, burns, unconsciousness)
//! with a course: untreated bleeding lowers the man's blood volume, which lowers his
//! performance, then takes him under, then kills him; an unconscious man left on his back
//! loses his airway. Treatment is a `Procedure`: a short sequence of hands-on steps done at
//! the man's position in the game's object-interaction register (grab, place, hold, wrap with
//! a cadence, turn, watch). Each procedure has its own steps, consumes its own items from the
//! finite `AidBox`, can be improvised when the kit is out, and comes out as well as it was
//! done: a rushed or interrupted job gives a degraded result, the wrong order of care lets a
//! faster bleed win.
//!
//! Every number here is a design approximation of field-medicine rules of thumb, not a
//! physiological solver; the named limits are listed in `docs/PHYSICAL_TRUTH_AUDIT.md` and
//! `docs/CREW_SOLO.md`. The model is deterministic and free of randomness so the course of a
//! wound and the outcome of a procedure are testable to the second.

/// Adult blood volume the model starts from (litres).
pub const BLOOD_VOLUME_L: f32 = 5.0;
/// Fraction of blood lost at which the man loses consciousness (class III haemorrhage).
pub const UNCONSCIOUS_LOSS: f32 = 0.33;
/// Fraction of blood lost that is fatal without transfusion (class IV).
pub const FATAL_LOSS: f32 = 0.42;
/// Performance starts to fall past this loss fraction (class II).
pub const DEGRADE_LOSS: f32 = 0.15;
/// An unconscious man on his back with nobody minding his airway: seconds until it closes.
pub const AIRWAY_FATAL_S: f32 = 240.0;

/// Where on the body a wound sits. Only what the treatment rules need to distinguish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Head,
    /// Trunk and pelvis: bleeding is partly internal, a dressing helps less and no tourniquet
    /// applies; a penetrating chest wound wants sealing.
    Torso,
    Arm,
    Leg,
}

impl Region {
    /// Peak external/internal bleeding rate, litres per minute, for a severity-1.0 wound. A
    /// severed femoral artery can bleed out in a few minutes; a trunk wound bleeds mostly inside.
    fn peak_rate_l_per_min(self) -> f32 {
        match self {
            Region::Head => 0.45,
            Region::Torso => 0.70,
            Region::Arm => 0.90,
            Region::Leg => 1.20,
        }
    }

    pub fn is_limb(self) -> bool {
        matches!(self, Region::Arm | Region::Leg)
    }

    pub fn label(self) -> &'static str {
        match self {
            Region::Head => "head",
            Region::Torso => "body",
            Region::Arm => "arm",
            Region::Leg => "leg",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WoundKind {
    Bleeding,
    Fracture,
    Burn,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wound {
    pub kind: WoundKind,
    pub region: Region,
    /// 0..1. For bleeding this scales the rate; for a fracture/burn the performance penalty.
    pub severity: f32,
    pub dressed: bool,
    pub tourniquet: bool,
    pub splinted: bool,
    /// The chest wound's hole is covered airtight.
    pub sealed: bool,
    /// The burn was cooled before it was dressed.
    pub cooled: bool,
    /// How well the dressing, tourniquet or splint was put on, 0..1 (1 = a textbook job).
    pub care_quality: f32,
    /// Seconds since the wound was taken. Minor bleeding clots on its own; major bleeding
    /// does not.
    pub age_s: f32,
    /// Seconds since the dressing went on. Under a pressure dressing a bleed clots over about
    /// ten minutes.
    pub dressed_age_s: f32,
}

impl Wound {
    pub fn new(kind: WoundKind, region: Region, severity: f32) -> Self {
        Self {
            kind,
            region,
            severity: severity.clamp(0.0, 1.0),
            dressed: false,
            tourniquet: false,
            splinted: false,
            sealed: false,
            cooled: false,
            care_quality: 1.0,
            age_s: 0.0,
            dressed_age_s: 0.0,
        }
    }

    /// A penetrating trunk wound that wants sealing.
    pub fn is_chest_wound(&self) -> bool {
        self.kind == WoundKind::Bleeding && self.region == Region::Torso && self.severity >= 0.5
    }

    /// Current bleeding rate in litres per second. Zero for non-bleeding wounds.
    pub fn bleed_rate_l_per_s(&self) -> f32 {
        if self.kind != WoundKind::Bleeding {
            return 0.0;
        }
        let peak = self.region.peak_rate_l_per_min() / 60.0;
        let mut rate = peak * self.severity.powf(1.5);
        if self.tourniquet {
            // A well-tightened tourniquet stops the limb; a loose one only slows it.
            let leak = 0.5 * (1.0 - self.care_quality);
            if leak <= 0.0 {
                return 0.0;
            }
            return rate * leak;
        }
        // Natural clotting: a minor wound tapers over about five minutes; a severe one does not.
        if self.severity < 0.30 {
            rate *= (-self.age_s / 300.0).exp();
        }
        if self.sealed && self.region == Region::Torso {
            rate *= 0.4 + 0.3 * (1.0 - self.care_quality);
        }
        if self.dressed {
            // A pressure dressing controls an external bleed well; it cannot reach an internal
            // trunk bleed, and only slows a head wound. A loose wrap does less. Under it the
            // wound clots over about ten minutes (the trunk bleed, being inside, over thirty).
            let base = match self.region {
                Region::Torso => 0.55,
                Region::Head => 0.35,
                Region::Arm | Region::Leg => 0.15,
            };
            rate *= base + (1.0 - base) * 0.6 * (1.0 - self.care_quality);
            let clot_s = if self.region == Region::Torso { 1800.0 } else { 600.0 };
            rate *= (-self.dressed_age_s / clot_s).exp();
        }
        rate
    }

    /// Whether this wound still calls for a treatment, and which.
    pub fn indicated(&self) -> Option<Treatment> {
        self.procedure().map(Procedure::treatment)
    }

    /// The procedure this wound wants next, if any.
    pub fn procedure(&self) -> Option<Procedure> {
        match self.kind {
            WoundKind::Bleeding => {
                if self.region.is_limb() && self.severity >= 0.75 && !self.tourniquet {
                    Some(Procedure::Tourniquet)
                } else if self.is_chest_wound() && !self.sealed {
                    Some(Procedure::ChestSeal)
                } else if !self.dressed && self.bleed_rate_l_per_s() > 0.0 {
                    Some(Procedure::PressureDressing)
                } else {
                    None
                }
            }
            WoundKind::Fracture => (!self.splinted).then_some(Procedure::Splint),
            WoundKind::Burn => (!self.dressed && self.severity >= 0.2).then_some(Procedure::BurnCare),
        }
    }
}

/// The kinds of care, as the receipts and the older instant path name them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Treatment {
    /// Verbandpäckchen: a field dressing with its pressure pad.
    Dressing,
    /// Esmarch rubber tourniquet on a limb.
    Tourniquet,
    /// Morphine for pain. It does nothing for the bleed and makes the man drowsy.
    Morphine,
    /// Wire ladder splint on a fracture.
    Splint,
}

impl Treatment {
    pub const ALL: [Treatment; 4] = [
        Treatment::Dressing,
        Treatment::Tourniquet,
        Treatment::Morphine,
        Treatment::Splint,
    ];

    /// Real seconds the player spends at the man's position with the full procedure.
    pub fn duration_s(self) -> f32 {
        self.procedure().nominal_seconds()
    }

    pub fn procedure(self) -> Procedure {
        match self {
            Treatment::Dressing => Procedure::PressureDressing,
            Treatment::Tourniquet => Procedure::Tourniquet,
            Treatment::Morphine => Procedure::Morphine,
            Treatment::Splint => Procedure::Splint,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Treatment::Dressing => "DRESSING",
            Treatment::Tourniquet => "TOURNIQUET",
            Treatment::Morphine => "MORPHINE",
            Treatment::Splint => "SPLINT",
        }
    }

    pub fn item_label(self) -> &'static str {
        match self {
            Treatment::Dressing => "Verbandpäckchen",
            Treatment::Tourniquet => "Esmarch tourniquet",
            Treatment::Morphine => "morphine ampoule",
            Treatment::Splint => "ladder splint",
        }
    }
}

// ── Procedures ───────────────────────────────────────────────────────────────────────────────

/// One hands-on treatment, done at the man.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Procedure {
    /// Limb arterial bleed: place above the wound, turn the windlass, secure, note the time.
    Tourniquet,
    /// A bleeder: cut the cloth away, pack, wrap with a cadence, tie off.
    PressureDressing,
    /// A fracture: draw the limb straight, lay the splint, bind, knot.
    Splint,
    /// A burn: cut the charred cloth away, cool from the canteen, lay the burn dressing, wrap.
    BurnCare,
    /// A sucking chest wound: open the tunic, the waxed wrapper over the hole, hold as he
    /// breathes out, strap.
    ChestSeal,
    /// An unconscious man: tilt the head and clear the mouth, roll him onto his side, watch
    /// him breathe.
    Airway,
    /// Morphine: dose into the thigh, mark him with the hour.
    Morphine,
}

impl Procedure {
    pub const ALL: [Procedure; 7] = [
        Procedure::Tourniquet,
        Procedure::PressureDressing,
        Procedure::Splint,
        Procedure::BurnCare,
        Procedure::ChestSeal,
        Procedure::Airway,
        Procedure::Morphine,
    ];

    pub fn treatment(self) -> Treatment {
        match self {
            Procedure::Tourniquet => Treatment::Tourniquet,
            Procedure::PressureDressing | Procedure::BurnCare | Procedure::ChestSeal => Treatment::Dressing,
            Procedure::Splint => Treatment::Splint,
            Procedure::Airway | Procedure::Morphine => Treatment::Morphine,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Procedure::Tourniquet => "TOURNIQUET",
            Procedure::PressureDressing => "PRESSURE DRESSING",
            Procedure::Splint => "SPLINT",
            Procedure::BurnCare => "BURN CARE",
            Procedure::ChestSeal => "CHEST SEAL",
            Procedure::Airway => "AIRWAY",
            Procedure::Morphine => "MORPHINE",
        }
    }

    /// Urgency rank for triage, lowest first.
    fn rank(self) -> u8 {
        match self {
            Procedure::Airway => 0,
            Procedure::Tourniquet => 1,
            Procedure::ChestSeal => 2,
            Procedure::PressureDressing => 3,
            Procedure::BurnCare => 4,
            Procedure::Splint => 5,
            Procedure::Morphine => 6,
        }
    }

    /// The steps, with the issued kit or improvised.
    pub fn steps(self, improvised: bool) -> &'static [Step] {
        use Item as I;
        use StepKind as K;
        match (self, improvised) {
            (Procedure::Tourniquet, false) => &[
                Step { kind: K::Press, label: "TOURNIQUET - PLACE IT ABOVE THE WOUND [E]", consumes: Some(I::Tourniquet) },
                Step { kind: K::Turn(3), label: "TOURNIQUET - TURN THE WINDLASS [LMB, THREE TURNS]", consumes: None },
                Step { kind: K::Hold(2.0), label: "TOURNIQUET - SECURE THE WINDLASS, NOTE THE HOUR [HOLD LMB]", consumes: None },
            ],
            (Procedure::Tourniquet, true) => &[
                Step { kind: K::Press, label: "NO TOURNIQUET - BELT AND WRENCH ABOVE THE WOUND [E]", consumes: None },
                Step { kind: K::Turn(5), label: "WRENCH - TWIST IT TIGHT [LMB, FIVE TURNS]", consumes: None },
                Step { kind: K::Hold(2.5), label: "WRENCH - TIE IT OFF, NOTE THE HOUR [HOLD LMB]", consumes: None },
            ],
            (Procedure::PressureDressing, false) => &[
                Step { kind: K::Hold(2.0), label: "DRESSING - CUT THE CLOTH AWAY [HOLD LMB]", consumes: None },
                Step { kind: K::Press, label: "DRESSING - PACK THE WOUND [E]", consumes: Some(I::Dressing) },
                Step { kind: K::Wrap(6), label: "DRESSING - WRAP IT, STEADY [MOUSE LEFT AND RIGHT]", consumes: None },
                Step { kind: K::Hold(1.5), label: "DRESSING - TIE IT OFF [HOLD LMB]", consumes: None },
            ],
            (Procedure::PressureDressing, true) => &[
                Step { kind: K::Hold(3.0), label: "NO DRESSINGS - TEAR A SLEEVE, PRESS IT ON [HOLD LMB]", consumes: None },
                Step { kind: K::Wrap(6), label: "SLEEVE - BIND IT TIGHT [MOUSE LEFT AND RIGHT]", consumes: None },
                Step { kind: K::Hold(1.5), label: "SLEEVE - KNOT [HOLD LMB]", consumes: None },
            ],
            (Procedure::Splint, false) => &[
                Step { kind: K::Hold(3.0), label: "FRACTURE - DRAW THE LIMB STRAIGHT [HOLD LMB]", consumes: None },
                Step { kind: K::Press, label: "SPLINT - LAY THE LADDER SPLINT ALONG IT [E]", consumes: Some(I::Splint) },
                Step { kind: K::Wrap(4), label: "SPLINT - BIND IT [MOUSE LEFT AND RIGHT]", consumes: Some(I::Triangular) },
                Step { kind: K::Hold(1.0), label: "SPLINT - KNOT [HOLD LMB]", consumes: None },
            ],
            (Procedure::Splint, true) => &[
                Step { kind: K::Hold(3.0), label: "FRACTURE - DRAW THE LIMB STRAIGHT [HOLD LMB]", consumes: None },
                Step { kind: K::Press, label: "NO SPLINT - THE CLEANING-ROD SECTION ALONG IT [E]", consumes: None },
                Step { kind: K::Wrap(5), label: "ROD - BIND IT WITH THE TRIANGULAR BANDAGE [MOUSE LEFT AND RIGHT]", consumes: Some(I::Triangular) },
                Step { kind: K::Hold(1.0), label: "ROD - KNOT [HOLD LMB]", consumes: None },
            ],
            (Procedure::BurnCare, false) => &[
                Step { kind: K::Hold(2.0), label: "BURN - CUT THE CHARRED CLOTH AWAY [HOLD LMB]", consumes: None },
                Step { kind: K::Press, label: "BURN - POUR THE CANTEEN OVER IT [E]", consumes: Some(I::Canteen) },
                Step { kind: K::Press, label: "BURN - LAY THE BURN DRESSING ON [E]", consumes: Some(I::BurnDressing) },
                Step { kind: K::Wrap(4), label: "BURN - WRAP IT LOOSELY [MOUSE LEFT AND RIGHT]", consumes: None },
            ],
            (Procedure::BurnCare, true) => &[
                Step { kind: K::Hold(2.0), label: "BURN - CUT THE CHARRED CLOTH AWAY [HOLD LMB]", consumes: None },
                Step { kind: K::Press, label: "NO BURN DRESSING - A PLAIN DRESSING ON IT [E]", consumes: Some(I::Dressing) },
                Step { kind: K::Wrap(4), label: "BURN - WRAP IT LOOSELY [MOUSE LEFT AND RIGHT]", consumes: None },
            ],
            (Procedure::ChestSeal, _) => &[
                Step { kind: K::Hold(1.5), label: "CHEST WOUND - OPEN THE TUNIC [HOLD LMB]", consumes: None },
                Step { kind: K::Press, label: "CHEST WOUND - THE WAXED WRAPPER OVER THE HOLE [E]", consumes: Some(I::Dressing) },
                Step { kind: K::Hold(3.0), label: "CHEST WOUND - HOLD IT SEALED AS HE BREATHES OUT [HOLD LMB]", consumes: None },
                Step { kind: K::Wrap(3), label: "CHEST WOUND - STRAP IT [MOUSE LEFT AND RIGHT]", consumes: None },
            ],
            (Procedure::Airway, _) => &[
                Step { kind: K::Hold(1.5), label: "OUT COLD - TILT THE HEAD BACK, CLEAR THE MOUTH [HOLD LMB]", consumes: None },
                Step { kind: K::Press, label: "OUT COLD - ROLL HIM ONTO HIS SIDE [E]", consumes: None },
                Step { kind: K::Watch(4.0), label: "OUT COLD - WATCH HIM BREATHE [HANDS OFF]", consumes: None },
            ],
            (Procedure::Morphine, _) => &[
                Step { kind: K::Press, label: "MORPHINE - DOSE INTO THE THIGH [E]", consumes: Some(I::Morphine) },
                Step { kind: K::Hold(1.5), label: "MORPHINE - MARK HIM: M AND THE HOUR [HOLD LMB]", consumes: None },
            ],
        }
    }

    /// Seconds a practised hand needs, kit issued, no interruptions.
    pub fn nominal_seconds(self) -> f32 {
        self.steps(false)
            .iter()
            .map(|step| match step.kind {
                StepKind::Hold(s) | StepKind::Watch(s) => s,
                StepKind::Press => 0.5,
                StepKind::Wrap(n) => n as f32 * 0.7,
                StepKind::Turn(n) => n as f32 * 0.8,
            })
            .sum()
    }
}

/// Something a procedure takes from the kit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Dressing,
    Tourniquet,
    Splint,
    Morphine,
    BurnDressing,
    Triangular,
    Canteen,
}

/// What the hands do in one step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StepKind {
    /// Hold the button for this many seconds; letting go early loses the hold.
    Hold(f32),
    /// One press.
    Press,
    /// This many wraps: alternate strokes left and right, in a cadence.
    Wrap(u8),
    /// This many turns of the windlass: presses, each given its time.
    Turn(u8),
    /// Hands off for this many seconds, watching him.
    Watch(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub kind: StepKind,
    pub label: &'static str,
    /// Taken from the kit when this step is done.
    pub consumes: Option<Item>,
}

/// The player's hands this tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hands {
    /// The button is down.
    pub hold: bool,
    /// The button went down this tick.
    pub press: bool,
    /// Sideways hand motion (mouse x), positive right.
    pub stroke: f32,
    pub dt: f32,
}

/// A procedure in progress on one man.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcedureRun {
    pub procedure: Procedure,
    /// Index of the wound it treats (none for airway and morphine).
    pub wound: Option<usize>,
    pub improvised: bool,
    pub step: usize,
    /// Seconds into a hold or watch.
    pub progress: f32,
    /// Wraps or turns done in the current step.
    pub count: u8,
    /// Sign of the last wrap stroke, so wraps alternate.
    last_stroke_sign: i8,
    /// Seconds since the last wrap or turn.
    since_count_s: f32,
    /// Quality of the work so far, 0..1.
    pub quality: f32,
    pub interruptions: u8,
    pub elapsed_s: f32,
    /// A cooling step that could not be done (no canteen) is skipped and noted.
    pub skipped_cooling: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Working,
    /// The current step is done; the next begins (the step's item has been taken).
    StepDone,
    /// The procedure is complete; apply it with `Casualty::finish_procedure`.
    Finished,
}

/// Wrap/turn cadence window: a stroke faster than this is fumbling, slower is dawdling.
const CADENCE_MIN_S: f32 = 0.25;
const CADENCE_MAX_S: f32 = 1.8;
const STROKE_THRESHOLD: f32 = 18.0;

impl ProcedureRun {
    pub fn new(procedure: Procedure, wound: Option<usize>, improvised: bool) -> Self {
        Self {
            procedure,
            wound,
            improvised,
            step: 0,
            progress: 0.0,
            count: 0,
            last_stroke_sign: 0,
            since_count_s: 10.0,
            quality: 1.0,
            interruptions: 0,
            elapsed_s: 0.0,
            skipped_cooling: false,
        }
    }

    pub fn steps(&self) -> &'static [Step] {
        self.procedure.steps(self.improvised)
    }

    pub fn current(&self) -> Option<&'static Step> {
        self.steps().get(self.step)
    }

    pub fn is_finished(&self) -> bool {
        self.step >= self.steps().len()
    }

    /// The hands jerked: a hit on the tank, a man shoved. The step's work so far is lost, a
    /// wrap or turn comes undone, and the job will be a little worse for it.
    pub fn interrupt(&mut self) {
        self.progress = 0.0;
        self.count = self.count.saturating_sub(2);
        self.quality = (self.quality - 0.15).max(0.0);
        self.interruptions += 1;
    }

    /// Advance by one tick of the hands. Returns what happened; the caller takes the step's
    /// item from the kit on `StepDone`/`Finished` (see `AidBox::take_for`).
    pub fn advance(&mut self, hands: Hands) -> Progress {
        let Some(step) = self.current() else { return Progress::Finished; };
        let dt = hands.dt.max(0.0);
        self.elapsed_s += dt;
        self.since_count_s += dt;
        let done = match step.kind {
            StepKind::Hold(seconds) => {
                if hands.hold {
                    self.progress += dt;
                } else if self.progress > 0.0 {
                    // Let go early: the work slips.
                    self.progress = 0.0;
                    self.quality = (self.quality - 0.05).max(0.0);
                }
                self.progress >= seconds
            }
            StepKind::Press => hands.press,
            StepKind::Wrap(wraps) => {
                let sign = if hands.stroke > STROKE_THRESHOLD {
                    1
                } else if hands.stroke < -STROKE_THRESHOLD {
                    -1
                } else {
                    0
                };
                if sign != 0 && sign != self.last_stroke_sign {
                    if self.last_stroke_sign != 0 {
                        // A wrap is one full stroke the other way; its timing is the cadence,
                        // judged from the second wrap on (the first sets the rhythm).
                        if self.count > 0 && (self.since_count_s < CADENCE_MIN_S || self.since_count_s > CADENCE_MAX_S) {
                            self.quality = (self.quality - 0.08).max(0.0);
                        }
                        self.count += 1;
                    }
                    // The first stroke starts the cadence clock; every wrap restarts it.
                    self.since_count_s = 0.0;
                    self.last_stroke_sign = sign;
                }
                self.count >= wraps
            }
            StepKind::Turn(turns) => {
                if hands.press {
                    if self.since_count_s >= 0.5 {
                        self.count += 1;
                        self.since_count_s = 0.0;
                    } else {
                        // Mashing the windlass does not tighten it.
                        self.quality = (self.quality - 0.03).max(0.0);
                    }
                }
                self.count >= turns
            }
            StepKind::Watch(seconds) => {
                if hands.hold {
                    self.progress = 0.0;
                } else {
                    self.progress += dt;
                }
                self.progress >= seconds
            }
        };
        if !done {
            return Progress::Working;
        }
        self.step += 1;
        self.progress = 0.0;
        self.count = 0;
        self.last_stroke_sign = 0;
        self.since_count_s = 10.0;
        if self.is_finished() {
            Progress::Finished
        } else {
            Progress::StepDone
        }
    }

    /// Skip the current step (a canteen that is not there): the care is poorer for it.
    pub fn skip_step(&mut self) {
        if self.current().is_some() {
            self.step += 1;
            self.progress = 0.0;
            self.count = 0;
            self.skipped_cooling = true;
            self.quality = (self.quality - 0.2).max(0.0);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Nothing in the box of that kind.
    OutOfSupply,
    /// No wound this treatment applies to.
    NotIndicated,
    /// The man is dead.
    Dead,
    /// A second morphine dose within the hour would stop his breathing.
    MorphineLimit,
}

/// The vehicle's first-aid kit (Kfz-Verbandkasten) and what the crew carries. Counts are the
/// supply; nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AidBox {
    pub dressings: u8,
    pub tourniquets: u8,
    pub morphine: u8,
    pub splints: u8,
    pub burn_dressings: u8,
    pub triangular: u8,
    /// Water for cooling a burn: the crew's canteens.
    pub canteen: u8,
    pub scissors: bool,
}

impl AidBox {
    /// A full box as issued.
    pub const FULL: AidBox = AidBox {
        dressings: 6,
        tourniquets: 2,
        morphine: 3,
        splints: 2,
        burn_dressings: 2,
        triangular: 3,
        canteen: 2,
        scissors: true,
    };
    /// Scavenged dressings do not pile up without limit.
    const MAX_DRESSINGS: u8 = 10;

    pub fn count(&self, treatment: Treatment) -> u8 {
        match treatment {
            Treatment::Dressing => self.dressings,
            Treatment::Tourniquet => self.tourniquets,
            Treatment::Morphine => self.morphine,
            Treatment::Splint => self.splints,
        }
    }

    pub fn has(&self, item: Item) -> bool {
        match item {
            Item::Dressing => self.dressings > 0,
            Item::Tourniquet => self.tourniquets > 0,
            Item::Splint => self.splints > 0,
            Item::Morphine => self.morphine > 0,
            Item::BurnDressing => self.burn_dressings > 0,
            Item::Triangular => self.triangular > 0,
            Item::Canteen => self.canteen > 0,
        }
    }

    pub fn take_item(&mut self, item: Item) -> bool {
        let slot = match item {
            Item::Dressing => &mut self.dressings,
            Item::Tourniquet => &mut self.tourniquets,
            Item::Splint => &mut self.splints,
            Item::Morphine => &mut self.morphine,
            Item::BurnDressing => &mut self.burn_dressings,
            Item::Triangular => &mut self.triangular,
            Item::Canteen => &mut self.canteen,
        };
        if *slot == 0 {
            return false;
        }
        *slot -= 1;
        true
    }

    fn take(&mut self, treatment: Treatment) -> bool {
        self.take_item(match treatment {
            Treatment::Dressing => Item::Dressing,
            Treatment::Tourniquet => Item::Tourniquet,
            Treatment::Morphine => Item::Morphine,
            Treatment::Splint => Item::Splint,
        })
    }

    /// Whether a procedure can be started, and whether it must be improvised.
    /// `None`: not possible at all (nothing to improvise with).
    pub fn can_start(&self, procedure: Procedure) -> Option<bool> {
        match procedure {
            Procedure::Tourniquet => Some(self.tourniquets == 0),
            Procedure::PressureDressing => Some(self.dressings == 0),
            Procedure::Splint => {
                if self.splints > 0 && self.triangular > 0 {
                    Some(false)
                } else if self.triangular > 0 {
                    Some(true)
                } else {
                    None
                }
            }
            Procedure::BurnCare => {
                if self.burn_dressings > 0 {
                    Some(false)
                } else if self.dressings > 0 {
                    Some(true)
                } else {
                    None
                }
            }
            Procedure::ChestSeal => (self.dressings > 0).then_some(false),
            Procedure::Airway => Some(false),
            Procedure::Morphine => (self.morphine > 0).then_some(false),
        }
    }

    /// Take a finished step's item. A canteen that is dry is not an error (the step is skipped
    /// beforehand); anything else missing means the kit changed mid-procedure.
    pub fn take_for(&mut self, step: &Step) -> bool {
        match step.consumes {
            None => true,
            Some(item) => self.take_item(item),
        }
    }

    /// Resupply to the issued contents (the same sources that refill ammunition and fuel).
    pub fn restock(&mut self) {
        *self = Self::FULL;
    }

    /// A dead soldier's own dressing packet, taken from the body.
    pub fn scavenge_dressing(&mut self) -> bool {
        if self.dressings >= Self::MAX_DRESSINGS {
            return false;
        }
        self.dressings += 1;
        true
    }

    pub fn is_empty(&self) -> bool {
        self.dressings == 0
            && self.tourniquets == 0
            && self.morphine == 0
            && self.splints == 0
            && self.burn_dressings == 0
            && self.triangular == 0
    }
}

impl Default for AidBox {
    fn default() -> Self {
        Self::FULL
    }
}

/// Something that changed in a man's state during `advance`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    LostConsciousness,
    Woke,
    Died,
}

/// One man's physiological state.
#[derive(Clone, Debug, PartialEq)]
pub struct Casualty {
    pub blood_l: f32,
    pub wounds: Vec<Wound>,
    /// Seconds of unconsciousness left (blast, head wound, blood loss, morphine).
    pub unconscious_s: f32,
    /// Concussion after waking: a performance cap that recovers over ten minutes.
    pub concussion: f32,
    /// Morphine in effect, 0..; decays over about three hours.
    pub morphine: f32,
    /// Seconds since the last morphine dose; a second dose within the hour is refused.
    pub since_morphine_s: f32,
    /// Somebody has tilted his head and put him on his side.
    pub airway_clear: bool,
    /// Seconds unconscious on his back with nobody minding his airway.
    pub airway_s: f32,
    /// The hour the tourniquet went on, written on him (elapsed seconds of the man's record).
    pub tourniquet_time_s: Option<f32>,
    /// The M and the hour written on his forehead.
    pub marked: bool,
    pub dead: bool,
    pub cause: Option<&'static str>,
    /// Seconds this record has run.
    pub clock_s: f32,
}

impl Default for Casualty {
    fn default() -> Self {
        Self {
            blood_l: BLOOD_VOLUME_L,
            wounds: Vec::new(),
            unconscious_s: 0.0,
            concussion: 0.0,
            morphine: 0.0,
            since_morphine_s: f32::INFINITY,
            airway_clear: false,
            airway_s: 0.0,
            tourniquet_time_s: None,
            marked: false,
            dead: false,
            cause: None,
            clock_s: 0.0,
        }
    }
}

impl Casualty {
    pub fn blood_loss_fraction(&self) -> f32 {
        (1.0 - self.blood_l / BLOOD_VOLUME_L).clamp(0.0, 1.0)
    }

    pub fn is_bleeding(&self) -> bool {
        self.total_bleed_rate_l_per_s() > 1.0e-5
    }

    pub fn total_bleed_rate_l_per_s(&self) -> f32 {
        self.wounds.iter().map(Wound::bleed_rate_l_per_s).sum()
    }

    pub fn is_wounded(&self) -> bool {
        !self.wounds.is_empty() || self.blood_loss_fraction() > 0.02
    }

    pub fn is_conscious(&self) -> bool {
        !self.dead && self.unconscious_s <= 0.0
    }

    /// Pain, 0..1, from untreated wounds; morphine removes it.
    pub fn pain(&self) -> f32 {
        if self.morphine >= 0.3 {
            return 0.0;
        }
        self.wounds
            .iter()
            .map(|w| match w.kind {
                WoundKind::Fracture => {
                    if w.splinted {
                        w.severity * 0.4
                    } else {
                        w.severity
                    }
                }
                WoundKind::Burn => {
                    if w.cooled {
                        w.severity * 0.4
                    } else {
                        w.severity * 0.8
                    }
                }
                WoundKind::Bleeding => w.severity * 0.5,
            })
            .sum::<f32>()
            .min(1.0)
    }

    /// How well the man can work his station, 0..1. One is a fit man.
    pub fn performance(&self) -> f32 {
        if !self.is_conscious() {
            return 0.0;
        }
        let loss = self.blood_loss_fraction();
        let blood = if loss < DEGRADE_LOSS {
            1.0
        } else if loss < UNCONSCIOUS_LOSS {
            // Class II to III: shaky, then failing.
            1.0 - 0.75 * (loss - DEGRADE_LOSS) / (UNCONSCIOUS_LOSS - DEGRADE_LOSS)
        } else {
            0.25
        };
        let mut perf = blood;
        for w in &self.wounds {
            let factor = match w.kind {
                WoundKind::Fracture => {
                    if w.splinted {
                        0.45 + 0.25 * w.care_quality
                    } else {
                        0.45
                    }
                }
                WoundKind::Burn => {
                    if w.cooled {
                        1.0 - 0.3 * w.severity
                    } else {
                        1.0 - 0.5 * w.severity
                    }
                }
                WoundKind::Bleeding => 1.0,
            };
            perf *= factor;
        }
        perf *= 1.0 - 0.35 * self.pain();
        if self.morphine >= 0.3 {
            perf *= 0.85; // drowsy
        }
        if self.concussion > 0.0 {
            perf = perf.min(1.0 - 0.3 * self.concussion);
        }
        perf.clamp(0.0, 1.0)
    }

    /// Able to do his job at all.
    pub fn able(&self) -> bool {
        self.performance() > 0.05
    }

    /// Record a new wound. A head wound or heavy shock also knocks the man out.
    pub fn add_wound(&mut self, wound: Wound) {
        if self.dead {
            return;
        }
        if wound.region == Region::Head && wound.severity >= 0.35 {
            self.knock_out(20.0 + 60.0 * wound.severity);
        }
        self.wounds.push(wound);
    }

    pub fn knock_out(&mut self, seconds: f32) {
        if self.dead {
            return;
        }
        self.unconscious_s = self.unconscious_s.max(seconds);
        self.airway_clear = false;
    }

    /// Burn exposure accumulates while the man is in a burning compartment.
    pub fn expose_to_fire(&mut self, severity_per_s: f32, dt: f32) {
        if self.dead || severity_per_s <= 0.0 {
            return;
        }
        let add = severity_per_s * dt;
        match self
            .wounds
            .iter_mut()
            .find(|w| w.kind == WoundKind::Burn)
        {
            Some(burn) => burn.severity = (burn.severity + add).min(1.0),
            None => self.wounds.push(Wound::new(WoundKind::Burn, Region::Torso, add)),
        }
    }

    /// The procedure the man needs most, and the wound it is for.
    pub fn procedure_needed(&self) -> Option<(Procedure, Option<usize>)> {
        if self.dead {
            return None;
        }
        if !self.is_conscious() && !self.airway_clear {
            return Some((Procedure::Airway, None));
        }
        let mut best: Option<(Procedure, usize)> = None;
        for (i, w) in self.wounds.iter().enumerate() {
            if let Some(p) = w.procedure() {
                if best.is_none_or(|(b, _)| p.rank() < b.rank()) {
                    best = Some((p, i));
                }
            }
        }
        if let Some((p, i)) = best {
            return Some((p, Some(i)));
        }
        if self.pain() >= 0.3 && self.morphine < 0.3 && self.since_morphine_s >= 3600.0 {
            return Some((Procedure::Morphine, None));
        }
        None
    }

    /// The treatment the man needs most, if any (the older name for the receipts).
    pub fn indicated(&self) -> Option<Treatment> {
        self.procedure_needed().map(|(p, _)| p.treatment())
    }

    /// Begin a procedure on this man with what the kit has. `None` when it cannot be started.
    pub fn begin_procedure(&self, procedure: Procedure, wound: Option<usize>, aid: &AidBox) -> Option<ProcedureRun> {
        if self.dead {
            return None;
        }
        if procedure == Procedure::Morphine && self.since_morphine_s < 3600.0 {
            return None;
        }
        let improvised = aid.can_start(procedure)?;
        Some(ProcedureRun::new(procedure, wound, improvised))
    }

    /// Apply a finished procedure at its quality.
    pub fn finish_procedure(&mut self, run: &ProcedureRun) -> Result<(), Refusal> {
        if self.dead {
            return Err(Refusal::Dead);
        }
        let quality = if run.improvised { run.quality.min(0.6) } else { run.quality };
        let clock = self.clock_s;
        match run.procedure {
            Procedure::Morphine => {
                if self.since_morphine_s < 3600.0 {
                    return Err(Refusal::MorphineLimit);
                }
                self.morphine += 1.0;
                self.since_morphine_s = 0.0;
                self.marked = true;
                Ok(())
            }
            Procedure::Airway => {
                self.airway_clear = true;
                self.airway_s = 0.0;
                // On his side with a clear airway he comes round sooner.
                self.unconscious_s *= 0.5;
                Ok(())
            }
            procedure => {
                let index = run
                    .wound
                    .filter(|&i| i < self.wounds.len())
                    .ok_or(Refusal::NotIndicated)?;
                let w = &mut self.wounds[index];
                match procedure {
                    Procedure::Tourniquet => {
                        if !w.region.is_limb() || w.kind != WoundKind::Bleeding {
                            return Err(Refusal::NotIndicated);
                        }
                        w.tourniquet = true;
                        w.care_quality = quality;
                        self.tourniquet_time_s = Some(clock);
                    }
                    Procedure::PressureDressing => {
                        if w.kind != WoundKind::Bleeding || w.dressed {
                            return Err(Refusal::NotIndicated);
                        }
                        w.dressed = true;
                        w.dressed_age_s = 0.0;
                        w.care_quality = quality;
                    }
                    Procedure::Splint => {
                        if w.kind != WoundKind::Fracture || w.splinted {
                            return Err(Refusal::NotIndicated);
                        }
                        w.splinted = true;
                        w.care_quality = quality;
                    }
                    Procedure::BurnCare => {
                        if w.kind != WoundKind::Burn || w.dressed {
                            return Err(Refusal::NotIndicated);
                        }
                        w.dressed = true;
                        w.cooled = !run.skipped_cooling && !run.improvised;
                        w.care_quality = quality;
                    }
                    Procedure::ChestSeal => {
                        if !w.is_chest_wound() || w.sealed {
                            return Err(Refusal::NotIndicated);
                        }
                        w.sealed = true;
                        w.care_quality = quality;
                    }
                    Procedure::Airway | Procedure::Morphine => unreachable!(),
                }
                Ok(())
            }
        }
    }

    /// The instant path: a textbook job with the issued kit, items taken. Tests and the receipts
    /// use it; play goes through `begin_procedure`/`ProcedureRun`/`finish_procedure`.
    pub fn treat(&mut self, treatment: Treatment, aid: &mut AidBox) -> Result<(), Refusal> {
        if self.dead {
            return Err(Refusal::Dead);
        }
        let procedure = treatment.procedure();
        let wound = match procedure {
            Procedure::Morphine => {
                if self.since_morphine_s < 3600.0 {
                    return Err(Refusal::MorphineLimit);
                }
                if self.wounds.is_empty() {
                    return Err(Refusal::NotIndicated);
                }
                None
            }
            Procedure::Tourniquet => Some(
                self.wounds
                    .iter()
                    .position(|w| w.kind == WoundKind::Bleeding && w.region.is_limb() && !w.tourniquet)
                    .ok_or(Refusal::NotIndicated)?,
            ),
            Procedure::PressureDressing => {
                // The worst undressed bleed first, then an undressed burn.
                let mut best: Option<(usize, f32)> = None;
                for (i, w) in self.wounds.iter().enumerate() {
                    if w.dressed {
                        continue;
                    }
                    let weight = match w.kind {
                        WoundKind::Bleeding => 10.0 + w.bleed_rate_l_per_s() * 1000.0,
                        WoundKind::Burn => w.severity,
                        WoundKind::Fracture => continue,
                    };
                    if best.is_none_or(|(_, b)| weight > b) {
                        best = Some((i, weight));
                    }
                }
                Some(best.ok_or(Refusal::NotIndicated)?.0)
            }
            Procedure::Splint => Some(
                self.wounds
                    .iter()
                    .position(|w| w.kind == WoundKind::Fracture && !w.splinted)
                    .ok_or(Refusal::NotIndicated)?,
            ),
            Procedure::BurnCare | Procedure::ChestSeal | Procedure::Airway => unreachable!(),
        };
        if !aid.take(treatment) {
            return Err(Refusal::OutOfSupply);
        }
        // A plain dressing on a burn is burn care without cooling.
        let procedure = match (procedure, wound) {
            (Procedure::PressureDressing, Some(i)) if self.wounds[i].kind == WoundKind::Burn => Procedure::BurnCare,
            (p, _) => p,
        };
        let mut run = ProcedureRun::new(procedure, wound, false);
        run.step = run.steps().len();
        if procedure == Procedure::BurnCare {
            run.skipped_cooling = true;
            run.quality = 1.0;
        }
        self.finish_procedure(&run)
    }

    /// Advance the time course by `dt` seconds.
    pub fn advance(&mut self, dt: f32) -> Vec<Event> {
        let mut events = Vec::new();
        if self.dead || !(dt > 0.0) {
            return events;
        }
        self.clock_s += dt;
        for w in &mut self.wounds {
            w.age_s += dt;
            if w.dressed {
                w.dressed_age_s += dt;
            }
        }
        let rate = self.total_bleed_rate_l_per_s();
        self.blood_l = (self.blood_l - rate * dt).max(0.0);
        self.morphine = (self.morphine - dt / 10_800.0).max(0.0);
        self.since_morphine_s += dt;
        self.concussion = (self.concussion - dt / 600.0).max(0.0);

        let loss = self.blood_loss_fraction();
        let was_out = self.unconscious_s > 0.0;
        if loss >= FATAL_LOSS {
            self.dead = true;
            self.unconscious_s = 0.0;
            self.cause = Some("bled out");
            events.push(Event::Died);
            return events;
        }
        if loss >= UNCONSCIOUS_LOSS && !was_out {
            // Out until the loss is treated; checked again every ten seconds.
            self.unconscious_s = 10.0;
            self.airway_clear = false;
        }
        if self.unconscious_s > 0.0 {
            if !self.airway_clear {
                self.airway_s += dt;
                if self.airway_s >= AIRWAY_FATAL_S {
                    self.dead = true;
                    self.unconscious_s = 0.0;
                    self.cause = Some("airway");
                    events.push(Event::Died);
                    return events;
                }
            }
            self.unconscious_s -= dt;
            if self.unconscious_s <= 0.0 {
                if loss >= UNCONSCIOUS_LOSS {
                    self.unconscious_s = 10.0;
                } else {
                    self.unconscious_s = 0.0;
                    self.airway_s = 0.0;
                    self.concussion = self.concussion.max(0.6);
                    events.push(Event::Woke);
                }
            }
        }
        if !was_out && self.unconscious_s > 0.0 {
            events.push(Event::LostConsciousness);
        }
        events
    }

    /// Seconds until this man dies of what he has now, untreated; infinite if he is stable.
    pub fn time_to_death_s(&self) -> f32 {
        if self.dead {
            return 0.0;
        }
        let mut t = f32::INFINITY;
        let rate = self.total_bleed_rate_l_per_s();
        if rate > 1.0e-6 {
            let fatal_l = BLOOD_VOLUME_L * (1.0 - FATAL_LOSS);
            t = t.min(((self.blood_l - fatal_l) / rate).max(0.0));
        }
        if !self.is_conscious() && !self.airway_clear {
            t = t.min((AIRWAY_FATAL_S - self.airway_s).max(0.0));
        }
        t
    }

    /// Short state word for intercom lines and receipts.
    pub fn state_label(&self) -> &'static str {
        if self.dead {
            "dead"
        } else if !self.is_conscious() {
            "out"
        } else if self.is_bleeding() {
            "bleeding"
        } else if self.is_wounded() {
            "wounded"
        } else {
            "fit"
        }
    }
}

/// Triage: the order to work on several men, the one nearest death first; men who need no
/// care are left out. Indices into `men`.
pub fn triage(men: &[&Casualty]) -> Vec<usize> {
    let mut order: Vec<(usize, f32, u8)> = men
        .iter()
        .enumerate()
        .filter_map(|(i, man)| {
            let (procedure, _) = man.procedure_needed()?;
            Some((i, man.time_to_death_s(), procedure.rank()))
        })
        .collect();
    order.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
    order.into_iter().map(|(i, _, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(c: &mut Casualty, seconds: f32) -> Vec<Event> {
        let mut out = Vec::new();
        let dt = 0.1;
        let steps = (seconds / dt).round() as usize;
        for _ in 0..steps {
            out.extend(c.advance(dt));
        }
        out
    }

    fn hands(hold: bool, press: bool, stroke: f32, dt: f32) -> Hands {
        Hands { hold, press, stroke, dt }
    }

    /// Drive a procedure the way practised hands would: holds held, presses pressed, wraps
    /// alternated at 0.7 s, turns at 0.8 s, watches watched. Returns the seconds taken.
    fn practised(run: &mut ProcedureRun) -> f32 {
        let dt: f32 = 0.05;
        let mut t: f32 = 0.0;
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 20_000, "procedure never finished");
            let Some(step) = run.current() else { break; };
            let h = match step.kind {
                StepKind::Hold(_) => hands(true, false, 0.0, dt),
                StepKind::Press => hands(true, true, 0.0, dt),
                StepKind::Wrap(_) => {
                    let phase = ((t / 0.7) as i64) % 2;
                    hands(false, false, if phase == 0 { 40.0 } else { -40.0 }, dt)
                }
                StepKind::Turn(_) => {
                    let press = ((t * 20.0).round() as i64) % 16 == 0;
                    hands(press, press, 0.0, dt)
                }
                StepKind::Watch(_) => hands(false, false, 0.0, dt),
            };
            t += dt;
            if run.advance(h) == Progress::Finished {
                break;
            }
        }
        t
    }

    #[test]
    fn fit_man_stays_fit() {
        let mut c = Casualty::default();
        assert_eq!(c.performance(), 1.0);
        assert!(run(&mut c, 600.0).is_empty());
        assert_eq!(c.performance(), 1.0);
        assert!(!c.is_wounded());
    }

    #[test]
    fn untreated_femoral_bleed_degrades_then_kills_in_minutes() {
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Leg, 0.9));
        assert!(c.is_bleeding());
        assert!(c.time_to_death_s() < 300.0 && c.time_to_death_s() > 60.0);
        run(&mut c, 60.0);
        assert!(c.performance() < 1.0, "a minute of heavy bleeding shows");
        assert!(c.is_conscious());
        let events = run(&mut c, 240.0);
        assert!(events.contains(&Event::LostConsciousness));
        assert!(events.contains(&Event::Died), "dead within five minutes untreated");
        assert!(c.dead);
        assert_eq!(c.cause, Some("bled out"));
        assert_eq!(c.performance(), 0.0);
    }

    #[test]
    fn tourniquet_stops_a_limb_bleed_and_dressing_slows_one() {
        let mut aid = AidBox::FULL;
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Leg, 0.9));
        assert_eq!(c.indicated(), Some(Treatment::Tourniquet));
        c.treat(Treatment::Tourniquet, &mut aid).unwrap();
        assert_eq!(aid.tourniquets, 1);
        assert!(!c.is_bleeding());
        assert!(c.tourniquet_time_s.is_some(), "the hour is noted on him");
        let blood = c.blood_l;
        run(&mut c, 600.0);
        assert_eq!(c.blood_l, blood);
        assert!(!c.dead && c.is_conscious());

        let mut d = Casualty::default();
        d.add_wound(Wound::new(WoundKind::Bleeding, Region::Arm, 0.5));
        let before = d.total_bleed_rate_l_per_s();
        assert_eq!(d.indicated(), Some(Treatment::Dressing));
        d.treat(Treatment::Dressing, &mut aid).unwrap();
        assert!(d.total_bleed_rate_l_per_s() < before * 0.2);
        assert_eq!(aid.dressings, 5);
        let events = run(&mut d, 1800.0);
        assert!(!events.contains(&Event::Died), "a dressed moderate bleed is survivable");
        assert!(d.blood_loss_fraction() < DEGRADE_LOSS, "it clots under the dressing");
        assert!(d.performance() < 1.0, "treated men work degraded");
    }

    #[test]
    fn trunk_bleed_wants_a_seal_and_a_dressing_only_partly_controls_it() {
        let mut aid = AidBox::FULL;
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Torso, 0.8));
        assert_eq!(c.procedure_needed(), Some((Procedure::ChestSeal, Some(0))));
        assert_eq!(c.treat(Treatment::Tourniquet, &mut aid), Err(Refusal::NotIndicated));
        let before = c.total_bleed_rate_l_per_s();
        c.treat(Treatment::Dressing, &mut aid).unwrap();
        let after = c.total_bleed_rate_l_per_s();
        assert!(after > before * 0.4 && after < before * 0.7);
        // The seal, done properly, brings it down further.
        let mut run = c.begin_procedure(Procedure::ChestSeal, Some(0), &aid).unwrap();
        practised(&mut run);
        c.finish_procedure(&run).unwrap();
        assert!(c.total_bleed_rate_l_per_s() < after * 0.5);
    }

    #[test]
    fn minor_wound_clots_on_its_own() {
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Arm, 0.2));
        let events = run(&mut c, 1800.0);
        assert!(events.is_empty());
        assert!(c.blood_loss_fraction() < DEGRADE_LOSS);
        assert!(c.total_bleed_rate_l_per_s() < 1.0e-4);
    }

    #[test]
    fn supply_is_finite_and_only_consumed_when_applied() {
        let mut aid = AidBox {
            dressings: 1,
            tourniquets: 0,
            morphine: 1,
            splints: 0,
            burn_dressings: 0,
            triangular: 0,
            canteen: 0,
            scissors: true,
        };
        let mut c = Casualty::default();
        assert_eq!(c.treat(Treatment::Dressing, &mut aid), Err(Refusal::NotIndicated));
        assert_eq!(aid.dressings, 1, "a refused treatment costs nothing");
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Arm, 0.5));
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Leg, 0.2));
        c.treat(Treatment::Dressing, &mut aid).unwrap();
        assert_eq!(c.treat(Treatment::Dressing, &mut aid), Err(Refusal::OutOfSupply));
        assert_eq!(aid.dressings, 0);
        assert!(c.wounds[0].dressed && !c.wounds[1].dressed, "the worse bleed takes the one dressing");
        assert_eq!(c.treat(Treatment::Tourniquet, &mut aid), Err(Refusal::OutOfSupply));
        c.treat(Treatment::Morphine, &mut aid).unwrap();
        assert_eq!(c.treat(Treatment::Morphine, &mut aid), Err(Refusal::MorphineLimit));
        aid.restock();
        assert_eq!(aid, AidBox::FULL);
        assert_eq!(c.treat(Treatment::Morphine, &mut aid), Err(Refusal::MorphineLimit));
        run(&mut c, 3601.0);
        c.treat(Treatment::Morphine, &mut aid).unwrap();
        assert_eq!(aid.morphine, 2);
        // Scavenging is bounded.
        for _ in 0..20 {
            aid.scavenge_dressing();
        }
        assert_eq!(aid.dressings, AidBox::MAX_DRESSINGS);
    }

    #[test]
    fn morphine_kills_pain_but_slows_the_man_and_fracture_needs_a_splint() {
        let mut aid = AidBox::FULL;
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Fracture, Region::Arm, 0.8));
        let hurt = c.performance();
        assert!(hurt < 0.45);
        assert_eq!(c.indicated(), Some(Treatment::Splint));
        c.treat(Treatment::Splint, &mut aid).unwrap();
        let splinted = c.performance();
        assert!(splinted > hurt && splinted < 0.75);
        assert_eq!(c.indicated(), Some(Treatment::Morphine), "still in pain");
        c.treat(Treatment::Morphine, &mut aid).unwrap();
        assert_eq!(c.pain(), 0.0);
        assert!(c.marked, "the M and the hour go on his forehead");
        // Mild pain relieved, drowsiness added: a little slower than before, but working.
        assert!(c.performance() < splinted && c.performance() > 0.5, "drowsy but working");
        assert_eq!(c.indicated(), None);
        // Against severe pain the morphine is a net gain.
        let mut d = Casualty::default();
        d.add_wound(Wound::new(WoundKind::Burn, Region::Torso, 0.9));
        let in_pain = d.performance();
        d.treat(Treatment::Morphine, &mut aid).unwrap();
        assert!(d.performance() > in_pain, "severe pain relieved outweighs the drowsiness");
    }

    #[test]
    fn head_wound_knocks_out_then_wakes_concussed() {
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Head, 0.4));
        assert!(!c.is_conscious());
        assert_eq!(c.performance(), 0.0);
        assert_eq!(c.procedure_needed().map(|(p, _)| p), Some(Procedure::Airway), "first his airway");
        let events = run(&mut c, 50.0);
        assert!(events.contains(&Event::Woke));
        assert!(c.is_conscious());
        assert!(c.performance() <= 0.85, "concussed on waking");
        assert_eq!(c.state_label(), "bleeding");
    }

    #[test]
    fn unconscious_man_left_on_his_back_loses_his_airway_unless_positioned() {
        let mut c = Casualty::default();
        c.knock_out(400.0);
        assert!(c.time_to_death_s() <= AIRWAY_FATAL_S);
        let events = run(&mut c, AIRWAY_FATAL_S + 1.0);
        assert!(events.contains(&Event::Died));
        assert_eq!(c.cause, Some("airway"));

        let mut d = Casualty::default();
        d.knock_out(400.0);
        let aid = AidBox::FULL;
        let mut procedure = d.begin_procedure(Procedure::Airway, None, &aid).unwrap();
        practised(&mut procedure);
        d.finish_procedure(&procedure).unwrap();
        assert!(d.airway_clear);
        assert!(d.unconscious_s < 400.0, "on his side he comes round sooner");
        let events = run(&mut d, 400.0);
        assert!(!events.contains(&Event::Died));
        assert!(events.contains(&Event::Woke));
    }

    #[test]
    fn fire_exposure_accumulates_burns_and_burn_care_cools_them() {
        let mut c = Casualty::default();
        for _ in 0..300 {
            c.expose_to_fire(0.02, 0.1);
        }
        let burn = c.wounds.iter().find(|w| w.kind == WoundKind::Burn).unwrap();
        assert!((burn.severity - 0.6).abs() < 1.0e-3);
        assert!(c.performance() < 0.75);
        assert_eq!(c.procedure_needed(), Some((Procedure::BurnCare, Some(0))));
        let mut aid = AidBox::FULL;
        let hurting = c.performance();
        let mut procedure = c.begin_procedure(Procedure::BurnCare, Some(0), &aid).unwrap();
        for step in procedure.steps() {
            assert!(aid.take_for(step));
        }
        practised(&mut procedure);
        c.finish_procedure(&procedure).unwrap();
        assert!(c.wounds[0].cooled && c.wounds[0].dressed);
        assert!(c.performance() > hurting, "cooled and dressed he works better");
        assert_eq!(aid.canteen, 1);
        assert_eq!(aid.burn_dressings, 1);
    }

    #[test]
    fn dead_men_take_no_treatment() {
        let mut aid = AidBox::FULL;
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Leg, 1.0));
        run(&mut c, 400.0);
        assert!(c.dead);
        assert_eq!(c.treat(Treatment::Tourniquet, &mut aid), Err(Refusal::Dead));
        assert_eq!(aid, AidBox::FULL);
        assert_eq!(c.indicated(), None);
        assert!(c.begin_procedure(Procedure::Tourniquet, Some(0), &aid).is_none());
    }

    #[test]
    fn every_procedure_runs_to_the_end_with_practised_hands_in_about_its_nominal_time() {
        let aid = AidBox::FULL;
        for procedure in Procedure::ALL {
            for improvised in [false, true] {
                let mut r = ProcedureRun::new(procedure, Some(0), improvised);
                let t = practised(&mut r);
                assert!(r.is_finished(), "{procedure:?} improvised={improvised}");
                assert!(r.quality > 0.95, "{procedure:?} improvised={improvised}: practised hands keep the quality, got {}", r.quality);
                if !improvised {
                    let nominal = procedure.nominal_seconds();
                    assert!((t - nominal).abs() < nominal * 0.5 + 1.5, "{procedure:?}: {t} s vs nominal {nominal} s");
                }
            }
            assert!(aid.can_start(procedure).is_some());
        }
    }

    #[test]
    fn a_rushed_interrupted_tourniquet_leaks_and_a_practised_one_holds() {
        let aid = AidBox::FULL;
        let mut good = Casualty::default();
        good.add_wound(Wound::new(WoundKind::Bleeding, Region::Leg, 0.9));
        let mut r = good.begin_procedure(Procedure::Tourniquet, Some(0), &aid).unwrap();
        practised(&mut r);
        good.finish_procedure(&r).unwrap();
        assert_eq!(good.total_bleed_rate_l_per_s(), 0.0);

        let mut bad = Casualty::default();
        bad.add_wound(Wound::new(WoundKind::Bleeding, Region::Leg, 0.9));
        let mut r = bad.begin_procedure(Procedure::Tourniquet, Some(0), &aid).unwrap();
        // Place it, then mash the windlass with no time between turns, get hit twice, and only
        // then finish properly.
        assert_eq!(r.advance(hands(true, true, 0.0, 0.05)), Progress::StepDone);
        for _ in 0..12 {
            r.advance(hands(true, true, 0.0, 0.05));
        }
        r.interrupt();
        r.interrupt();
        practised(&mut r);
        assert!(r.quality < 0.7, "a rushed, interrupted job is a poor one: {}", r.quality);
        bad.finish_procedure(&r).unwrap();
        assert!(bad.total_bleed_rate_l_per_s() > 0.0, "a loose tourniquet leaks");
        let untreated = 1.2 / 60.0 * 0.9f32.powf(1.5);
        assert!(bad.total_bleed_rate_l_per_s() < untreated * 0.5, "but it still slows the bleed");
    }

    #[test]
    fn wrap_cadence_counts_only_alternating_strokes_and_marks_a_loose_wrap() {
        let mut r = ProcedureRun::new(Procedure::PressureDressing, Some(0), false);
        r.step = 2; // the wrap
        // Strokes all one way do not wrap.
        for _ in 0..20 {
            r.advance(hands(false, false, 50.0, 0.1));
        }
        assert_eq!(r.count, 0);
        // Alternating strokes at a good cadence count cleanly.
        for i in 0..5 {
            for _ in 0..7 {
                r.advance(hands(false, false, if i % 2 == 0 { -50.0 } else { 50.0 }, 0.1));
            }
        }
        assert_eq!(r.count, 5);
        assert!(r.quality > 0.99);
        // A stroke flicked back the other way far too fast is a loose wrap.
        r.advance(hands(false, false, 50.0, 0.05));
        assert_eq!(r.step, 3, "the sixth wrap finished the step");
        assert!(r.quality < 1.0);
    }

    #[test]
    fn improvised_kit_is_used_when_the_box_is_out_and_caps_the_result() {
        let empty = AidBox { dressings: 0, tourniquets: 0, splints: 0, triangular: 1, burn_dressings: 0, morphine: 0, canteen: 0, scissors: false };
        assert_eq!(empty.can_start(Procedure::Tourniquet), Some(true));
        assert_eq!(empty.can_start(Procedure::PressureDressing), Some(true));
        assert_eq!(empty.can_start(Procedure::Splint), Some(true));
        assert_eq!(empty.can_start(Procedure::BurnCare), None);
        assert_eq!(empty.can_start(Procedure::ChestSeal), None);
        assert_eq!(empty.can_start(Procedure::Morphine), None);
        assert_eq!(empty.can_start(Procedure::Airway), Some(false));
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Fracture, Region::Leg, 0.7));
        let mut r = c.begin_procedure(Procedure::Splint, Some(0), &empty).unwrap();
        assert!(r.improvised);
        practised(&mut r);
        c.finish_procedure(&r).unwrap();
        assert!(c.wounds[0].splinted);
        assert!(c.wounds[0].care_quality <= 0.6, "a rod and a bandage are not a ladder splint");
    }

    #[test]
    fn triage_puts_the_man_nearest_death_first() {
        let mut leg = Casualty::default();
        leg.add_wound(Wound::new(WoundKind::Bleeding, Region::Leg, 0.9));
        let mut arm = Casualty::default();
        arm.add_wound(Wound::new(WoundKind::Bleeding, Region::Arm, 0.4));
        let mut out = Casualty::default();
        out.knock_out(300.0);
        let fit = Casualty::default();
        let order = triage(&[&arm, &fit, &leg, &out]);
        assert_eq!(order[0], 2, "the femoral bleed dies first");
        assert_eq!(order.len(), 3, "the fit man is not on the list");
        assert!(order.contains(&3) && order.contains(&0));
        // Treat the arm first and the leg is lost: the order is the game.
        let mut wrong = leg.clone();
        run(&mut wrong, Procedure::PressureDressing.nominal_seconds() + 200.0);
        assert!(wrong.dead || !wrong.is_conscious());
    }
}
