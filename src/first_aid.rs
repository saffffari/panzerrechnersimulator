//! Wounds with a time course, and the vehicle's first-aid box.
//!
//! One `Casualty` per crewman. Hits produce wounds (bleeding, fracture, burn, unconsciousness);
//! untreated bleeding lowers the man's blood volume, which lowers his performance, then takes
//! him under, then kills him. Treatment is applied by the player from the finite `AidBox`.
//!
//! Every number here is a design approximation of field-medicine rules of thumb, not a
//! physiological solver; the named limits are listed in `docs/PHYSICAL_TRUTH_AUDIT.md` and
//! `docs/CREW_SOLO.md`. The model is deterministic and free of randomness so the course of a
//! wound is testable to the second.

/// Adult blood volume the model starts from (litres).
pub const BLOOD_VOLUME_L: f32 = 5.0;
/// Fraction of blood lost at which the man loses consciousness (class III haemorrhage).
pub const UNCONSCIOUS_LOSS: f32 = 0.33;
/// Fraction of blood lost that is fatal without transfusion (class IV).
pub const FATAL_LOSS: f32 = 0.42;
/// Performance starts to fall past this loss fraction (class II).
pub const DEGRADE_LOSS: f32 = 0.15;

/// Where on the body a wound sits. Only what the treatment rules need to distinguish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Head,
    /// Trunk and pelvis: bleeding is partly internal, a dressing helps less and no tourniquet
    /// applies.
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
            age_s: 0.0,
            dressed_age_s: 0.0,
        }
    }

    /// Current bleeding rate in litres per second. Zero for non-bleeding wounds.
    pub fn bleed_rate_l_per_s(&self) -> f32 {
        if self.kind != WoundKind::Bleeding {
            return 0.0;
        }
        if self.tourniquet {
            return 0.0;
        }
        let peak = self.region.peak_rate_l_per_min() / 60.0;
        let mut rate = peak * self.severity.powf(1.5);
        // Natural clotting: a minor wound tapers over about five minutes; a severe one does not.
        if self.severity < 0.30 {
            rate *= (-self.age_s / 300.0).exp();
        }
        if self.dressed {
            // A pressure dressing controls an external bleed well; it cannot reach an internal
            // trunk bleed, and only slows a head wound. Under it the wound clots over about ten
            // minutes (the trunk bleed, being inside, over thirty).
            rate *= match self.region {
                Region::Torso => 0.55,
                Region::Head => 0.35,
                Region::Arm | Region::Leg => 0.15,
            };
            let clot_s = if self.region == Region::Torso { 1800.0 } else { 600.0 };
            rate *= (-self.dressed_age_s / clot_s).exp();
        }
        rate
    }

    /// Whether this wound still calls for a treatment, and which.
    pub fn indicated(&self) -> Option<Treatment> {
        match self.kind {
            WoundKind::Bleeding => {
                if self.region.is_limb() && self.severity >= 0.75 && !self.tourniquet {
                    Some(Treatment::Tourniquet)
                } else if !self.dressed && self.bleed_rate_l_per_s() > 0.0 {
                    Some(Treatment::Dressing)
                } else {
                    None
                }
            }
            WoundKind::Fracture => (!self.splinted).then_some(Treatment::Splint),
            WoundKind::Burn => (!self.dressed && self.severity >= 0.2).then_some(Treatment::Dressing),
        }
    }
}

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

    /// Real seconds the player spends at the man's position, hands busy.
    pub fn duration_s(self) -> f32 {
        match self {
            Treatment::Dressing => 20.0,
            Treatment::Tourniquet => 12.0,
            Treatment::Morphine => 8.0,
            Treatment::Splint => 25.0,
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

/// The vehicle's first-aid box (Kfz-Verbandkasten). Counts are the supply; nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AidBox {
    pub dressings: u8,
    pub tourniquets: u8,
    pub morphine: u8,
    pub splints: u8,
}

impl AidBox {
    /// A full box as issued.
    pub const FULL: AidBox = AidBox {
        dressings: 6,
        tourniquets: 2,
        morphine: 3,
        splints: 2,
    };

    pub fn count(&self, treatment: Treatment) -> u8 {
        match treatment {
            Treatment::Dressing => self.dressings,
            Treatment::Tourniquet => self.tourniquets,
            Treatment::Morphine => self.morphine,
            Treatment::Splint => self.splints,
        }
    }

    fn take(&mut self, treatment: Treatment) -> bool {
        let slot = match treatment {
            Treatment::Dressing => &mut self.dressings,
            Treatment::Tourniquet => &mut self.tourniquets,
            Treatment::Morphine => &mut self.morphine,
            Treatment::Splint => &mut self.splints,
        };
        if *slot == 0 {
            return false;
        }
        *slot -= 1;
        true
    }

    /// Resupply to the issued contents (the same sources that refill ammunition and fuel).
    pub fn restock(&mut self) {
        *self = Self::FULL;
    }

    pub fn is_empty(&self) -> bool {
        self.dressings == 0 && self.tourniquets == 0 && self.morphine == 0 && self.splints == 0
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
    pub dead: bool,
    pub cause: Option<&'static str>,
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
            dead: false,
            cause: None,
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
                WoundKind::Burn => w.severity * 0.8,
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
                        0.70
                    } else {
                        0.45
                    }
                }
                WoundKind::Burn => 1.0 - 0.5 * w.severity,
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

    /// The treatment the man needs most, if any.
    pub fn indicated(&self) -> Option<Treatment> {
        if self.dead {
            return None;
        }
        let mut best: Option<Treatment> = None;
        for w in &self.wounds {
            if let Some(t) = w.indicated() {
                let rank = |t: Treatment| match t {
                    Treatment::Tourniquet => 0,
                    Treatment::Dressing => 1,
                    Treatment::Splint => 2,
                    Treatment::Morphine => 3,
                };
                if best.is_none_or(|b| rank(t) < rank(b)) {
                    best = Some(t);
                }
            }
        }
        if best.is_none() && self.pain() >= 0.3 && self.morphine < 0.3 {
            best = Some(Treatment::Morphine);
        }
        best
    }

    /// Apply a treatment from the box. Supply is consumed only when the treatment applies.
    pub fn treat(&mut self, treatment: Treatment, aid: &mut AidBox) -> Result<(), Refusal> {
        if self.dead {
            return Err(Refusal::Dead);
        }
        let target = match treatment {
            Treatment::Morphine => {
                if self.since_morphine_s < 3600.0 {
                    return Err(Refusal::MorphineLimit);
                }
                if self.wounds.is_empty() {
                    return Err(Refusal::NotIndicated);
                }
                None
            }
            Treatment::Tourniquet => self
                .wounds
                .iter()
                .position(|w| w.kind == WoundKind::Bleeding && w.region.is_limb() && !w.tourniquet)
                .map(Some)
                .ok_or(Refusal::NotIndicated)?,
            Treatment::Dressing => {
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
            Treatment::Splint => self
                .wounds
                .iter()
                .position(|w| w.kind == WoundKind::Fracture && !w.splinted)
                .map(Some)
                .ok_or(Refusal::NotIndicated)?,
        };
        if !aid.take(treatment) {
            return Err(Refusal::OutOfSupply);
        }
        match (treatment, target) {
            (Treatment::Morphine, _) => {
                self.morphine += 1.0;
                self.since_morphine_s = 0.0;
            }
            (Treatment::Tourniquet, Some(i)) => self.wounds[i].tourniquet = true,
            (Treatment::Dressing, Some(i)) => self.wounds[i].dressed = true,
            (Treatment::Splint, Some(i)) => self.wounds[i].splinted = true,
            _ => unreachable!("treatment target resolved above"),
        }
        Ok(())
    }

    /// Advance the time course by `dt` seconds.
    pub fn advance(&mut self, dt: f32) -> Vec<Event> {
        let mut events = Vec::new();
        if self.dead || !(dt > 0.0) {
            return events;
        }
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
        }
        if self.unconscious_s > 0.0 {
            self.unconscious_s -= dt;
            if self.unconscious_s <= 0.0 {
                if loss >= UNCONSCIOUS_LOSS {
                    self.unconscious_s = 10.0;
                } else {
                    self.unconscious_s = 0.0;
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
    fn trunk_bleed_is_only_partly_controlled_by_a_dressing() {
        let mut aid = AidBox::FULL;
        let mut c = Casualty::default();
        c.add_wound(Wound::new(WoundKind::Bleeding, Region::Torso, 0.8));
        assert_eq!(c.treat(Treatment::Tourniquet, &mut aid), Err(Refusal::NotIndicated));
        let before = c.total_bleed_rate_l_per_s();
        c.treat(Treatment::Dressing, &mut aid).unwrap();
        let after = c.total_bleed_rate_l_per_s();
        assert!(after > before * 0.4 && after < before * 0.7);
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
        let events = run(&mut c, 50.0);
        assert!(events.contains(&Event::Woke));
        assert!(c.is_conscious());
        assert!(c.performance() <= 0.85, "concussed on waking");
        assert_eq!(c.state_label(), "bleeding");
    }

    #[test]
    fn fire_exposure_accumulates_burns() {
        let mut c = Casualty::default();
        for _ in 0..300 {
            c.expose_to_fire(0.02, 0.1);
        }
        let burn = c.wounds.iter().find(|w| w.kind == WoundKind::Burn).unwrap();
        assert!((burn.severity - 0.6).abs() < 1.0e-3);
        assert!(c.performance() < 0.75);
        assert_eq!(c.indicated(), Some(Treatment::Dressing));
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
    }
}
