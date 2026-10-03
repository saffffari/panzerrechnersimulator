//! Pure soundscape selection state. Gameplay supplies facts; this module performs no I/O,
//! playback, wall-clock reads, location inference, or environmental classification.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

const COMBAT_RELEASE_S: f64 = 60.0;
const MIN_COMBAT_PASSAGE_S: f64 = 90.0;
const AFTERMATH_S: f64 = 90.0;
const MIN_MODE_HOLD_S: f64 = 20.0;
const LOCATION_DEBOUNCE_S: f64 = 2.0;
const THREAT_ENTER: f64 = 0.45;
const THREAT_LEAVE: f64 = 0.25;
const DRIVE_ENTER_MPS: f64 = 1.8;
const DRIVE_LEAVE_MPS: f64 = 0.65;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Explore,
    Driving,
    Threat,
    Combat,
    Aftermath,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Driving => "driving",
            Self::Threat => "threat",
            Self::Combat => "combat",
            Self::Aftermath => "aftermath",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "explore" => Self::Explore,
            "driving" => Self::Driving,
            "threat" => Self::Threat,
            "combat" => Self::Combat,
            "aftermath" => Self::Aftermath,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Context {
    pub night: bool,
    /// Caller-owned night identity. The director neither increments nor interprets it.
    pub night_id: i64,
    pub speed_mps: f32,
    pub threat: f32,
    pub combat: bool,
    /// Explicit tag, e.g. `coffee_shop`, `playground`, or `airport`. No name-based inference.
    pub location: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub mode: Mode,
    pub night: bool,
    pub night_id: i64,
    pub location: String,
    pub intensity: f32,
}

#[derive(Clone, Debug, Default)]
struct Bag {
    members: Vec<String>,
    remaining: Vec<String>,
    last: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Director {
    seed: u64,
    rng_state: u64,
    bags: BTreeMap<String, Bag>,
    plan: Plan,
    mode_age_s: f64,
    speed_mps: f64,
    threat: f64,
    driving: bool,
    threatened: bool,
    combat_release_s: f64,
    aftermath_s: f64,
    recent_combat: bool,
    location_initialized: bool,
    pending_location: Option<String>,
    pending_location_s: f64,
}

impl Director {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            rng_state: seed,
            bags: BTreeMap::new(),
            plan: Plan {
                mode: Mode::Explore,
                night: false,
                night_id: 0,
                location: String::new(),
                intensity: 0.1,
            },
            mode_age_s: 0.0,
            speed_mps: 0.0,
            threat: 0.0,
            driving: false,
            threatened: false,
            combat_release_s: 0.0,
            aftermath_s: 0.0,
            recent_combat: false,
            location_initialized: false,
            pending_location: None,
            pending_location_s: 0.0,
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Consume simulated seconds. A zero, negative, or nonfinite dt freezes the whole director,
    /// including pending location and context changes. Night identity updates without debounce
    /// on a running update. The caller caches a selected night track for the whole night.
    pub fn update(&mut self, dt: f32, context: &Context) -> Plan {
        if !dt.is_finite() || dt <= 0.0 {
            return self.plan.clone();
        }
        let dt = f64::from(dt);
        let speed = if context.speed_mps.is_finite() {
            f64::from(context.speed_mps).abs()
        } else {
            0.0
        };
        let threat = if context.threat.is_finite() {
            f64::from(context.threat.clamp(0.0, 1.0))
        } else {
            0.0
        };
        self.speed_mps = relax(self.speed_mps, speed, dt, 1.2);
        self.threat = relax(
            self.threat,
            threat,
            dt,
            if threat > self.threat { 0.4 } else { 1.5 },
        );
        self.driving = if self.driving {
            self.speed_mps > DRIVE_LEAVE_MPS
        } else {
            self.speed_mps >= DRIVE_ENTER_MPS
        };
        self.threatened = if self.threatened {
            self.threat > THREAT_LEAVE
        } else {
            self.threat >= THREAT_ENTER
        };
        self.plan.night = context.night;
        self.plan.night_id = context.night_id;
        self.update_location(dt, context.location.trim());
        self.update_mode(dt, context.combat);
        let target = match self.plan.mode {
            Mode::Explore => 0.1 + 0.15 * self.threat,
            Mode::Driving => 0.25 + 0.25 * (self.speed_mps / 8.0).clamp(0.0, 1.0),
            Mode::Threat => 0.5 + 0.35 * self.threat,
            Mode::Combat => 1.0,
            Mode::Aftermath => 0.2 + 0.4 * (self.aftermath_s / AFTERMATH_S),
        };
        let current = f64::from(self.plan.intensity);
        self.plan.intensity = relax(
            current,
            target,
            dt,
            if target > current { 3.0 } else { 12.0 },
        )
        .clamp(0.0, 1.0) as f32;
        self.plan.clone()
    }

    fn update_location(&mut self, dt: f64, location: &str) {
        if !self.location_initialized && !location.is_empty() {
            self.plan.location = location.to_owned();
            self.location_initialized = true;
            self.pending_location = None;
            self.pending_location_s = 0.0;
        } else if location == self.plan.location {
            self.pending_location = None;
            self.pending_location_s = 0.0;
        } else {
            if self.pending_location.as_deref() != Some(location) {
                self.pending_location = Some(location.to_owned());
                self.pending_location_s = 0.0;
            }
            self.pending_location_s += dt;
            if self.pending_location_s >= LOCATION_DEBOUNCE_S {
                self.plan.location = self.pending_location.take().unwrap();
                self.pending_location_s = 0.0;
            }
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.plan.mode != mode {
            self.plan.mode = mode;
            self.mode_age_s = 0.0;
        }
    }

    fn ambient_mode(&self) -> Mode {
        if self.threatened {
            Mode::Threat
        } else if self.driving {
            Mode::Driving
        } else {
            Mode::Explore
        }
    }

    fn update_mode(&mut self, dt: f64, combat: bool) {
        if combat {
            self.set_mode(Mode::Combat);
            self.mode_age_s += dt;
            self.combat_release_s = COMBAT_RELEASE_S.max(MIN_COMBAT_PASSAGE_S - self.mode_age_s);
            self.aftermath_s = 0.0;
            self.recent_combat = true;
            return;
        }
        let mut remaining = dt;
        if self.combat_release_s > 0.0 {
            let used = remaining.min(self.combat_release_s);
            self.combat_release_s -= used;
            self.mode_age_s += used;
            remaining -= used;
            if self.combat_release_s > 0.0 {
                return;
            }
            self.set_mode(Mode::Aftermath);
            self.aftermath_s = AFTERMATH_S;
        }
        if self.aftermath_s > 0.0 {
            let used = remaining.min(self.aftermath_s);
            self.aftermath_s -= used;
            self.mode_age_s += used;
            remaining -= used;
            if self.aftermath_s > 0.0 {
                return;
            }
            self.recent_combat = false;
            self.set_mode(self.ambient_mode());
        }
        self.mode_age_s += remaining;
        if self.mode_age_s >= MIN_MODE_HOLD_S {
            self.set_mode(self.ambient_mode());
        }
    }

    /// One shuffle bag per explicit pool. Each unique member plays once per cycle; the first
    /// member of a refill cannot repeat the preceding choice when alternatives exist. Input
    /// ordering is irrelevant, so callers may collect IDs from unordered catalogs safely.
    pub fn choose(&mut self, pool: &str, ids: &[String]) -> Option<String> {
        if ids.is_empty() {
            return None;
        }
        let mut members = ids.to_vec();
        members.sort();
        members.dedup();
        let bag = self.bags.entry(pool.to_owned()).or_default();
        if bag.members != members {
            bag.members = members;
            bag.remaining.clear();
        }
        if bag.remaining.is_empty() {
            bag.remaining = bag.members.clone();
            for i in (1..bag.remaining.len()).rev() {
                let j = random_index(&mut self.rng_state, i + 1);
                bag.remaining.swap(i, j);
            }
            let last_index = bag.remaining.len() - 1;
            if last_index > 0 && bag.last.as_ref() == bag.remaining.last() {
                let other = random_index(&mut self.rng_state, last_index);
                bag.remaining.swap(other, last_index);
            }
        }
        let choice = bag.remaining.pop()?;
        bag.last = Some(choice.clone());
        Some(choice)
    }

    pub fn snapshot(&self) -> Value {
        let bags: serde_json::Map<String, Value> = self
            .bags
            .iter()
            .map(|(name, bag)| {
                (
                    name.clone(),
                    json!({"members":bag.members,"remaining":bag.remaining,"last":bag.last}),
                )
            })
            .collect();
        json!({
            "schema":"panzer.soundscape-director", "version":1,
            "seed":self.seed, "rng_state":self.rng_state,
            "mode":self.plan.mode.as_str(), "night":self.plan.night,
            "night_id":self.plan.night_id, "location":self.plan.location,
            "intensity":self.plan.intensity, "mode_age_s":self.mode_age_s,
            "smoothed_speed_mps":self.speed_mps, "smoothed_threat":self.threat,
            "driving":self.driving, "threatened":self.threatened,
            "combat_release_s":self.combat_release_s, "aftermath_s":self.aftermath_s,
            "recent_combat":self.recent_combat,
            "location_initialized":self.location_initialized,
            "pending_location":self.pending_location, "pending_location_s":self.pending_location_s,
            // Keep readable decimal values and lossless continuation even when serde_json was
            // built without its optional float_roundtrip parsing feature.
            "exact_f64": {
                "mode_age_s":self.mode_age_s.to_bits(),
                "smoothed_speed_mps":self.speed_mps.to_bits(),
                "smoothed_threat":self.threat.to_bits(),
                "combat_release_s":self.combat_release_s.to_bits(),
                "aftermath_s":self.aftermath_s.to_bits(),
                "pending_location_s":self.pending_location_s.to_bits(),
            },
            "bags":bags,
        })
    }

    /// Unknown optional keys are ignored; absent state keys use safe initial values for older
    /// saves. Present fields with invalid types/ranges or impossible bag contents are rejected.
    /// A complete snapshot restores the exact continuation, including RNG and pending timers.
    pub fn restore(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        if let Some(schema) = object.get("schema") {
            if schema.as_str()? != "panzer.soundscape-director" {
                return None;
            }
        }
        if let Some(version) = object.get("version") {
            if version.as_u64()? != 1 {
                return None;
            }
        }
        let mut out = Self::new(object.get("seed")?.as_u64()?);
        out.rng_state = match object.get("rng_state") {
            Some(v) => v.as_u64()?,
            None => out.seed,
        };
        if let Some(mode) = object.get("mode") {
            out.plan.mode = Mode::parse(mode.as_str()?)?;
        }
        out.plan.night = optional_bool(value, "night", false)?;
        out.plan.night_id = match object.get("night_id") {
            Some(v) => v.as_i64()?,
            None => 0,
        };
        if let Some(location) = object.get("location") {
            out.plan.location = location.as_str()?.to_owned();
        }
        out.plan.intensity = optional_number(value, "intensity", 0.1, 0.0, 1.0)? as f32;
        out.mode_age_s = optional_number(value, "mode_age_s", 0.0, 0.0, f64::MAX)?;
        out.speed_mps =
            optional_number(value, "smoothed_speed_mps", 0.0, 0.0, f64::from(f32::MAX))?;
        out.threat = optional_number(value, "smoothed_threat", 0.0, 0.0, 1.0)?;
        out.driving = optional_bool(value, "driving", out.speed_mps >= DRIVE_ENTER_MPS)?;
        out.threatened = optional_bool(value, "threatened", out.threat >= THREAT_ENTER)?;
        out.combat_release_s = optional_number(
            value,
            "combat_release_s",
            if out.plan.mode == Mode::Combat {
                COMBAT_RELEASE_S
            } else {
                0.0
            },
            0.0,
            MIN_COMBAT_PASSAGE_S.max(COMBAT_RELEASE_S),
        )?;
        out.aftermath_s = optional_number(
            value,
            "aftermath_s",
            if out.plan.mode == Mode::Aftermath {
                AFTERMATH_S
            } else {
                0.0
            },
            0.0,
            AFTERMATH_S,
        )?;
        if (out.combat_release_s > 0.0) != (out.plan.mode == Mode::Combat)
            || (out.aftermath_s > 0.0) != (out.plan.mode == Mode::Aftermath)
        {
            return None;
        }
        out.recent_combat = optional_bool(
            value,
            "recent_combat",
            out.combat_release_s > 0.0 || out.aftermath_s > 0.0,
        )?;
        out.location_initialized =
            optional_bool(value, "location_initialized", !out.plan.location.is_empty())?;
        out.pending_location = match object.get("pending_location") {
            Some(Value::Null) | None => None,
            Some(v) => Some(v.as_str()?.to_owned()),
        };
        out.pending_location_s =
            optional_number(value, "pending_location_s", 0.0, 0.0, LOCATION_DEBOUNCE_S)?;
        if out.pending_location.is_none() && out.pending_location_s != 0.0 {
            return None;
        }
        if let Some(bags) = object.get("bags") {
            for (name, entry) in bags.as_object()? {
                let members = string_array(entry.get("members")?)?;
                let remaining = string_array(entry.get("remaining")?)?;
                let last = match entry.get("last") {
                    Some(Value::Null) | None => None,
                    Some(v) => Some(v.as_str()?.to_owned()),
                };
                let canonical: Vec<_> = members
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if canonical != members
                    || remaining.iter().collect::<BTreeSet<_>>().len() != remaining.len()
                    || remaining
                        .iter()
                        .any(|id| members.binary_search(id).is_err())
                {
                    return None;
                }
                out.bags.insert(
                    name.clone(),
                    Bag {
                        members,
                        remaining,
                        last,
                    },
                );
            }
        }
        Some(out)
    }
}

fn relax(current: f64, target: f64, dt: f64, tau: f64) -> f64 {
    current + (target - current) * (1.0 - (-dt / tau).exp())
}

// SplitMix64 with explicit state: zero is a valid seed and no platform/hash-map state leaks in.
fn random_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn random_index(state: &mut u64, count: usize) -> usize {
    let range = count as u64;
    let threshold = range.wrapping_neg() % range;
    loop {
        let value = random_u64(state);
        if value >= threshold {
            return (value % range) as usize;
        }
    }
}

fn optional_bool(value: &Value, key: &str, default: bool) -> Option<bool> {
    match value.get(key) {
        Some(v) => v.as_bool(),
        None => Some(default),
    }
}

fn optional_number(
    value: &Value,
    key: &str,
    default: f64,
    minimum: f64,
    maximum: f64,
) -> Option<f64> {
    let decimal = match value.get(key) {
        Some(v) => v.as_f64()?,
        None => default,
    };
    if !decimal.is_finite() || decimal < minimum || decimal > maximum {
        return None;
    }
    let exact = match value.get("exact_f64") {
        Some(v) => v.as_object()?.get(key),
        None => None,
    };
    let number = match exact {
        Some(bits) => f64::from_bits(bits.as_u64()?),
        None => decimal,
    };
    (number.is_finite() && number >= minimum && number <= maximum).then_some(number)
}

fn string_array(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|v| Some(v.as_str()?.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> Vec<String> {
        ["a", "b", "c", "d"].map(str::to_owned).to_vec()
    }

    fn advance(director: &mut Director, seconds: u32, context: &Context) -> Plan {
        let mut plan = director.plan.clone();
        for _ in 0..seconds * 4 {
            plan = director.update(0.25, context);
        }
        plan
    }

    #[test]
    fn explicit_seed_and_catalog_order_have_identical_choices() {
        let mut a = Director::new(0);
        let mut b = Director::new(0);
        let mut reversed = ids();
        reversed.reverse();
        for _ in 0..100 {
            assert_eq!(a.choose("night", &ids()), b.choose("night", &reversed));
        }
        assert_eq!(a.snapshot(), b.snapshot());
        assert_eq!(a.seed(), 0);
    }

    #[test]
    fn different_seeds_vary_the_sequence() {
        let mut a = Director::new(1);
        let mut b = Director::new(2);
        let left: Vec<_> = (0..32).map(|_| a.choose("night", &ids())).collect();
        let right: Vec<_> = (0..32).map(|_| b.choose("night", &ids())).collect();
        assert_ne!(left, right);
    }

    #[test]
    fn shuffle_bag_covers_each_cycle_without_adjacent_repeats() {
        for seed in 0..32 {
            let mut d = Director::new(seed);
            let mut last = None;
            for _ in 0..20 {
                let mut cycle = BTreeSet::new();
                for _ in 0..4 {
                    let choice = d.choose("ambience", &ids()).unwrap();
                    assert_ne!(Some(&choice), last.as_ref());
                    assert!(cycle.insert(choice.clone()));
                    last = Some(choice);
                }
                assert_eq!(cycle.len(), 4);
            }
        }
    }

    #[test]
    fn empty_single_duplicate_and_changed_pools_are_safe() {
        let mut d = Director::new(41);
        let untouched = d.snapshot();
        assert_eq!(d.choose("empty", &[]), None);
        assert_eq!(d.snapshot(), untouched);
        let single = vec!["one".into(), "one".into()];
        for _ in 0..3 {
            assert_eq!(d.choose("pool", &single).as_deref(), Some("one"));
        }
        let expanded = vec!["one".into(), "two".into(), "two".into()];
        assert_eq!(d.choose("pool", &expanded).as_deref(), Some("two"));
        assert_eq!(d.choose("pool", &expanded).as_deref(), Some("one"));
    }

    #[test]
    fn short_contact_retains_a_full_combat_passage_and_long_aftermath() {
        let mut d = Director::new(1);
        let combat = Context { combat: true, ..Context::default() };
        assert_eq!(d.update(0.01, &combat).mode, Mode::Combat);
        let quiet = Context::default();
        assert_eq!(advance(&mut d, 89, &quiet).mode, Mode::Combat);
        assert_eq!(advance(&mut d, 2, &quiet).mode, Mode::Aftermath);
        assert_eq!(advance(&mut d, 88, &quiet).mode, Mode::Aftermath);
        assert_eq!(advance(&mut d, 2, &quiet).mode, Mode::Explore);
        assert!(!d.recent_combat);
    }

    #[test]
    fn combat_restarts_grace_and_large_dt_consumes_all_phase_time() {
        let mut d = Director::new(3);
        let combat = Context { combat: true, ..Context::default() };
        let quiet = Context::default();
        advance(&mut d, 120, &combat);
        advance(&mut d, 50, &quiet);
        d.update(1.0, &combat);
        assert_eq!(advance(&mut d, 59, &quiet).mode, Mode::Combat);
        assert_eq!(d.update(2.0, &quiet).mode, Mode::Aftermath);
        assert_eq!(d.aftermath_s, AFTERMATH_S - 1.0);
        assert_eq!(d.update(100.0, &quiet).mode, Mode::Explore);
        assert_eq!(d.mode_age_s, 11.0);
    }

    #[test]
    fn brief_stops_and_starts_do_not_switch_the_driving_profile() {
        let mut d = Director::new(4);
        let fast = Context { speed_mps: 5.0, ..Context::default() };
        assert_eq!(advance(&mut d, 19, &fast).mode, Mode::Explore);
        assert_eq!(advance(&mut d, 1, &fast).mode, Mode::Driving);
        let quiet = Context::default();
        assert_eq!(advance(&mut d, 4, &quiet).mode, Mode::Driving);
        assert_eq!(advance(&mut d, 10, &fast).mode, Mode::Driving);
        assert_eq!(advance(&mut d, 20, &quiet).mode, Mode::Explore);
    }

    #[test]
    fn threat_hysteresis_takes_priority_over_driving() {
        let mut d = Director::new(5);
        let danger = Context {
            speed_mps: 5.0,
            threat: 0.9,
            ..Context::default()
        };
        assert_eq!(advance(&mut d, 21, &danger).mode, Mode::Threat);
        let middle = Context {
            threat: 0.35,
            ..danger.clone()
        };
        assert_eq!(advance(&mut d, 8, &middle).mode, Mode::Threat);
        let low = Context {
            threat: 0.1,
            ..danger
        };
        assert_eq!(advance(&mut d, 21, &low).mode, Mode::Driving);
        assert_eq!(advance(&mut d, 8, &middle).mode, Mode::Driving);
    }

    #[test]
    fn location_debounces_explicit_tags_and_night_identity_is_immediate() {
        let mut d = Director::new(6);
        let mut context = Context {
            location: "coffee_shop".into(),
            ..Context::default()
        };
        assert_eq!(d.update(0.1, &context).location, "coffee_shop");
        context.location = "playground".into();
        assert_eq!(d.update(1.5, &context).location, "coffee_shop");
        context.location = "airport".into();
        context.night = true;
        context.night_id = 42;
        let plan = d.update(0.5, &context);
        assert_eq!(plan.location, "coffee_shop");
        assert!(plan.night);
        assert_eq!(plan.night_id, 42);
        assert_eq!(d.update(1.5, &context).location, "airport");
        context.location = "arbitrary_authored_tag".into();
        assert_eq!(d.update(2.0, &context).location, "arbitrary_authored_tag");
    }

    #[test]
    fn pause_and_invalid_dt_freeze_every_state_and_pending_transition() {
        let mut d = Director::new(7);
        d.choose("night", &ids());
        d.update(
            0.25,
            &Context {
                combat: true,
                location: "airport".into(),
                ..Context::default()
            },
        );
        let before = d.snapshot();
        let changed = Context {
            night: true,
            night_id: 99,
            location: "playground".into(),
            ..Context::default()
        };
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let plan = d.update(dt, &changed);
            assert_eq!(plan, d.plan);
            assert_eq!(d.snapshot(), before);
        }
    }

    #[test]
    fn snapshot_json_roundtrip_preserves_exact_continuation() {
        let mut a = Director::new(u64::MAX);
        let mut context = Context {
            combat: true,
            speed_mps: 5.0,
            threat: 0.8,
            night: true,
            night_id: -3,
            location: "airport".into(),
        };
        a.update(0.25, &context);
        a.choose("night", &ids());
        a.choose("driving", &ids());
        context.combat = false;
        context.location = "coffee_shop".into();
        a.update(1.25, &context);
        let saved: Value = serde_json::from_str(&a.snapshot().to_string()).unwrap();
        let mut b = Director::restore(&saved).unwrap();
        assert_eq!(a.snapshot(), b.snapshot());
        for step in 0..100 {
            context.combat = step == 55;
            context.threat = if step < 30 { 0.8 } else { 0.1 };
            assert_eq!(a.update(0.5, &context), b.update(0.5, &context));
            let pool = if step % 2 == 0 { "night" } else { "driving" };
            assert_eq!(a.choose(pool, &ids()), b.choose(pool, &ids()));
            assert_eq!(a.snapshot(), b.snapshot());
        }
    }

    #[test]
    fn older_optional_fields_default_but_corrupt_snapshots_are_rejected() {
        let old = json!({"seed":9, "mode":"explore"});
        assert_eq!(
            Director::restore(&old).unwrap().snapshot(),
            Director::new(9).snapshot()
        );
        for bad in [
            json!({}),
            json!({"seed":9,"mode":"unknown"}),
            json!({"seed":9,"intensity":2.0}),
            json!({"seed":9,"combat_release_s":2.0}),
            json!({"seed":9,"bags":{"night":{"members":["a"],"remaining":["b"]}}}),
        ] {
            assert!(Director::restore(&bad).is_none(), "{bad}");
        }
        let legacy_combat = Director::restore(&json!({"seed":9,"mode":"combat"})).unwrap();
        assert_eq!(legacy_combat.combat_release_s, COMBAT_RELEASE_S);
    }

    #[test]
    fn nonfinite_context_signals_cannot_poison_snapshot_or_intensity() {
        let mut d = Director::new(8);
        let bad = Context {
            speed_mps: f32::INFINITY,
            threat: f32::NAN,
            ..Context::default()
        };
        let plan = d.update(1.0, &bad);
        assert!(plan.intensity.is_finite());
        assert_eq!(plan.mode, Mode::Explore);
        assert!(Director::restore(&d.snapshot()).is_some());
    }
}
