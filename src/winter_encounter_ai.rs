//! Local winter patrols, organised as the Soviet tank platoons of winter 1942–43. Perception
//! remains in `stealth`; this layer only consumes confirmed observations, radio fixes and
//! uncertain gunshot reports. Hidden player poses are deliberately absent from its input.
//!
//! Organisation (sources in docs/WINTER_PLATOONS_20260928.md): a platoon was three T-34s and only
//! the leader's tank carried a transceiver. Wingmen watched his tank and signals, followed him in
//! column 30–50 m apart, fought in line abreast with him in the centre and fired on the target he
//! chose. A platoon that lost its leader held and fought from the halt, then joined another
//! platoon's leader; with none left, its tanks wandered. Speeds, delays and ranges beyond those
//! are encounter tuning, not historical claims.
use glam::{Vec2, Vec3};

use super::stealth::PLATOON_SIZE;
use super::street_nav::{Leg, StreetNav, CARROT_M, REPLAN_OFF_M};
use super::tank_doctrine;

/// Enemy slots the encounter may field: two platoons in stealth's contiguous platoon slots.
pub(super) const SLOTS: usize = 2 * PLATOON_SIZE;
/// Tanks fielded per platoon, leader first: a full platoon and an attrited one of two, the
/// late-January company remnant. Gameplay knob: [3, 0] fields a lone platoon, [3, 3] a full
/// two-platoon company.
pub(super) const PLATOON_TANKS: [usize; 2] = [3, 2];
/// Column and line interval (doctrine: 30–50 m).
pub(super) const SPACING_M: f32 = 40.0;
/// A platoon that lost its leader holds and fights from the halt this long before joining
/// another platoon's leader.
pub(super) const REGROUP_S: f32 = 30.0;
/// ... and joins only a leader this close; with none in reach it fights on alone (a straggler).
pub(super) const JOIN_RANGE_M: f32 = 600.0;
/// A leader's contact report reaches the other platoon leader after the same hand-off time a
/// spotter needs inside its own platoon.
const RELAY_S: f32 = super::stealth::FIVE_SEC_RULE;
/// How long a relayed report is remembered once its sender no longer reports contact.
const RELAY_MEMORY_S: f32 = 60.0;
const SOUND_MEMORY_S: f32 = 150.0;
const TRAIL_STEP_M: f32 = 2.0;
const TRAIL_POINTS: usize = 128;
/// Beats walked by each platoon, halting at every corner to observe. They lie north of the
/// courtyard, beyond the fair-weather foot envelope of the house and the spawn, and clear of the
/// anti-tank guns' sites (`live_playtest`).
const ROUTES: [[Vec2; 4]; 2] = [
    [Vec2::new(1700.0, -210.0), Vec2::new(1820.0, -160.0),
     Vec2::new(1990.0, -170.0), Vec2::new(1880.0, -250.0)],
    [Vec2::new(1960.0, -360.0), Vec2::new(2100.0, -330.0),
     Vec2::new(2120.0, -230.0), Vec2::new(1990.0, -250.0)],
];

pub(super) fn slot(platoon: usize, place: usize) -> usize { platoon * PLATOON_SIZE + place }
pub(super) fn platoon_of(slot: usize) -> usize { slot / PLATOON_SIZE }
/// Whether the encounter fields a tank in this slot.
pub(super) fn fielded(i: usize) -> bool {
    i < SLOTS && i % PLATOON_SIZE < PLATOON_TANKS[platoon_of(i)].min(PLATOON_SIZE)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase { Patrol, Investigate, Search, Track }
impl Phase {
    pub fn name(self) -> &'static str {
        match self { Self::Patrol => "patrol", Self::Investigate => "investigate_sound",
            Self::Search => "search_last_seen", Self::Track => "visual_contact" }
    }
}

/// Command within the platoon. Only a leader has a radio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Role {
    /// Carries the transceiver; chooses the platoon's movement and target.
    Leader,
    /// Watches `leader`'s tank from formation `place` (1, 2, …): follows his track, copies his
    /// halts and fires on his target.
    Wingman { leader: usize, place: usize },
    /// Lost its leader: holds and fights from the halt on what it sees itself.
    Leaderless { waited: f32 },
    /// No leader left to join: wanders its own beat on its own senses.
    Straggler,
    /// Slot not fielded.
    Absent,
}
impl Role {
    pub fn name(self) -> &'static str {
        match self { Self::Leader => "leader", Self::Wingman { .. } => "wingman",
            Self::Leaderless { .. } => "leaderless", Self::Straggler => "straggler",
            Self::Absent => "absent" }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Observation {
    /// Some only when the shipping meter AND this frame's actual visibility qualify.
    pub visible: Option<Vec3>,
    /// Existing platoon memory. Never a caller-supplied current hidden position.
    pub radio_fix: Option<Vec3>,
    pub on_foot: bool,
    /// The armoured target's hull heading as its silhouette shows it; Some only with `visible`.
    pub heading: Option<f32>,
    /// Its gun is laid on this tank (the crew looks down the barrel); only with `visible`.
    pub gun_on_me: bool,
    /// A round from the target struck this tank, or burst beside it, since the last orders.
    pub struck: bool,
    /// This tank's own main-gun discharges so far, and whether its gun is loaded.
    pub shots: u32,
    pub loaded: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Order {
    pub phase: Phase,
    pub drive: f32,
    pub steer: f32,
    pub aim: Vec3,
    pub fire: bool,
}

/// A fielded, living hull as the encounter sees it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Hull {
    pub pos: Vec3,
    pub heading: f32,
    /// Signed hull speed along its heading, m/s.
    pub speed: f32,
}

#[derive(Clone, Copy, Debug)]
struct Crew {
    role: Role,
    waypoint: usize,
    dwell: f32,
    heard: Option<Vec3>,
    heard_t: f32,
    phase: Phase,
    visible_now: bool,
    observed_fix: Option<Vec3>,
    observed_t: f32,
    /// Fighting an armoured target (docs/ENEMY_TANK_AI.md).
    assault: Assault,
    /// The armour fix this crew acts on this frame; a wingman reads its leader's.
    known_fix: Option<Vec3>,
    /// Seconds halted at (or in view of, or with no street to) a reported area.
    linger_s: f32,
}

/// Doctrine against armour: tuning knobs, not historical claims (docs/ENEMY_TANK_AI.md).
/// Throttle while bounding between firing posts and while closing on a remembered fix.
const BOUND_SPEED: f32 = 0.62;
const CLOSE_SPEED: f32 = 0.5;
/// Shots from one post before the crew relocates (shoot, then move).
const SHOTS_PER_POST: u32 = 2;
/// Least time between two choices of post.
const DECIDE_S: f32 = 2.0;
/// A post held this long without the target in sight is given up for a closer one.
const POST_HOLD_S: f32 = 14.0;
/// Posts a crew found no street to are remembered and avoided (this many, this close).
const REFUSED_POSTS: usize = 4;
const REFUSED_RADIUS_M: f32 = 25.0;
/// Reversing out of the line of fire after a hit or under the target's gun.
const WITHDRAW_S: f32 = 4.5;
/// Ordered to drive but not moving for this long: the tank is pinned; it backs off and re-plans.
const STUCK_S: f32 = 3.0;
/// Bearings round the target at which firing posts are sampled.
const CANDIDATE_BEARINGS: usize = 24;
/// The target's frontal arc (half angle) where its armour defeats the 76 mm and its gun bears.
const FRONT_ARC_RAD: f32 = 0.61;
/// In contact with armour a platoon follows it this far from its beat.
const ASSAULT_LEASH_M: f32 = 1400.0;
/// Within this range of the target, with the target in the platoon's sight, at least one tank of
/// a platoon stays at its post covering the others' bounds.
const OVERWATCH_RANGE_M: f32 = 800.0;
/// Eye heights above the ground for a post's sight probes: the commander's cupola, and the hull
/// roof (a post from which the hull roof cannot see the target is hull-down).
const TURRET_EYE_M: f32 = 2.3;
const HULL_EYE_M: f32 = 1.1;

#[derive(Clone, Copy, Debug, Default)]
struct Assault {
    active: bool,
    /// The firing position the crew is making for or holding.
    post: Option<Vec2>,
    /// The fix the post was chosen against.
    against: Vec2,
    arrived: bool,
    at_post_s: f32,
    since_choice_s: f32,
    shots_seen: u32,
    shots_at_post: u32,
    withdraw_s: f32,
    stuck_s: f32,
    halt_s: f32,
    /// Posts pushed closer without regaining sight.
    press: i32,
    /// The target's hull heading when last seen.
    heading: Option<f32>,
    last_drive: f32,
    /// Posts no street led to, newest first.
    refused: [Option<Vec2>; REFUSED_POSTS],
}

/// The last report a leader passed on the company net.
#[derive(Clone, Copy, Debug)]
struct Relay {
    fix: Vec3,
    from: usize,
    held_s: f32,
    silent_s: f32,
}

pub(super) struct WinterEncounter {
    /// Each platoon's beat: a closed loop of corners.
    routes: Vec<Vec<Vec2>>,
    crews: Vec<Crew>,
    /// Each platoon leader's recent track, newest first; wingmen drive along it.
    trails: Vec<Vec<Vec2>>,
    relay: Option<Relay>,
    pub elapsed: f32,
    pub storm: f32,
    pub sounds_heard: u32,
    pub confirmed_sightings: u32,
    pub flashes_seen: u32,
    /// In a street district (a manifest revision's `street_encounter`): the measured streets the
    /// tanks drive along. `None` on open ground (the courtyard), where they drive straight.
    streets: Option<Streets>,
}

/// Can an eye at this point see a hull standing at this ground position (the caller's sight test
/// through the real terrain and cover)? Used only in the streets, to halt where a reported area
/// comes into view.
pub(super) type SightTest<'a> = &'a dyn Fn(Vec3, Vec2) -> bool;
/// Ground height at a point (the caller's terrain), for sight probes from candidate posts.
pub(super) type Ground<'a> = &'a dyn Fn(Vec2) -> f32;

/// Throttle closing on a report or a fix outside armour contact.
const INVESTIGATE_SPEED: f32 = 0.4;
/// A crew halted at a reported area, in view of it, or with no street to it, gives the report up
/// after this long instead of waiting out the sound memory.
const INVESTIGATE_LINGER_S: f32 = 20.0;
/// Leaders investigating a report in the streets halt once the reported area is in view this close.
pub(super) const STREET_VIEW_HALT_M: f32 = 170.0;
/// ... and otherwise close on it along the streets no nearer than this.
const STREET_CLOSE_M: f32 = 45.0;
/// Height of a T-34 commander's eye above the hull's centre.
const EYE_ABOVE_CENTRE_M: f32 = 1.0;

/// The street district's ground: the measured drivable streets, each tank's current street route,
/// and each leader's progress on the report it is closing on.
pub(super) struct Streets {
    nav: StreetNav,
    legs: Vec<Option<Leg>>,
    /// The fix a leader is closing on and whether it has reached its flank position yet.
    closing: Vec<Option<(Vec2, bool)>>,
}

impl Streets {
    pub fn new(nav: StreetNav) -> Self {
        Self { nav, legs: vec![None; SLOTS], closing: vec![None; SLOTS] }
    }

    pub fn nav(&self) -> &StreetNav { &self.nav }

    /// Drive slot `i` toward `goal` along the streets: `Some((drive, steer, arrived))`, or `None`
    /// where the map knows no route to it (the caller holds or chooses another goal; a tank never
    /// drives straight at the buildings).
    fn drive_to(&mut self, i: usize, pos: Vec2, heading: f32, goal: Vec2, speed: f32, stop: f32) -> Option<(f32, f32, bool)> {
        if self.legs[i].as_ref().is_none_or(|leg| leg.goal.distance(goal) > 6.0) {
            self.legs[i] = Leg::plan(&self.nav, pos, goal);
        }
        let mut tracked = self.legs[i].as_mut().map(|leg| leg.track(pos, CARROT_M));
        if tracked.is_some_and(|(_, _, off)| off > REPLAN_OFF_M) {
            self.legs[i] = Leg::plan(&self.nav, pos, goal);
            tracked = self.legs[i].as_mut().map(|leg| leg.track(pos, CARROT_M));
        }
        let (carrot, remaining, _) = tracked?;
        if remaining <= stop { return Some((0.0, 0.0, true)); }
        let to = carrot - pos;
        let error = wrap(to.x.atan2(to.y) - heading);
        let drive = if error.abs() > 0.65 { 0.0 } else { speed * (remaining / 20.0).clamp(0.35, 1.0) };
        Some((drive, (error * 2.0).clamp(-0.65, 0.65), false))
    }
}

pub(super) fn visibility_radius(existing: f32, foot: bool, storm: f32) -> f32 {
    // Snow/fog silhouettes disappear before the fair-weather stealth radius. Retain its
    // crouch, engine, night and movement reductions by taking the smaller envelope.
    existing.min(if foot { 205.0 - storm.clamp(0.0, 1.0) * 50.0 }
        else { 360.0 - storm.clamp(0.0, 1.0) * 120.0 })
}

impl WinterEncounter {
    pub fn new() -> Self {
        Self {
            routes: ROUTES.map(|route| route.to_vec()).to_vec(),
            crews: (0..SLOTS).map(|i| Crew {
                role: if !fielded(i) { Role::Absent }
                    else if i % PLATOON_SIZE == 0 { Role::Leader }
                    else { Role::Wingman { leader: slot(platoon_of(i), 0), place: i % PLATOON_SIZE } },
                waypoint: 1, dwell: 8.0 + platoon_of(i) as f32 * 8.0, heard: None, heard_t: 0.0,
                phase: Phase::Patrol, visible_now: false, observed_fix: None, observed_t: 0.0,
                assault: Assault::default(), known_fix: None, linger_s: 0.0,
            }).collect(),
            trails: vec![Vec::new(), Vec::new()],
            relay: None,
            elapsed: 0.0, storm: 0.66, sounds_heard: 0, confirmed_sightings: 0, flashes_seen: 0,
            streets: None,
        }
    }
    /// The encounter in a street district: the platoons walk `routes` (street crossings) and drive
    /// every leg, search and return along the measured streets of `nav`.
    pub fn in_streets(routes: [Vec<Vec2>; 2], nav: StreetNav) -> Self {
        assert!(routes.iter().all(|route| route.len() >= 2), "a beat needs corners");
        Self { routes: routes.to_vec(), streets: Some(Streets::new(nav)), ..Self::new() }
    }
    /// Append a fresh platoon without recycling casualties or sharing another crew's memory.
    /// Three identity slots are retained even for an under-strength reinforcement.
    pub fn append_platoon(&mut self, route: Vec<Vec2>, count: usize) -> usize {
        assert!(route.len() >= 2 && (1..=PLATOON_SIZE).contains(&count));
        let first = self.crews.len();
        self.routes.push(route);
        self.trails.push(Vec::new());
        for place in 0..PLATOON_SIZE {
            self.crews.push(Crew {
                role: if place >= count { Role::Absent } else if place == 0 { Role::Leader }
                    else { Role::Wingman { leader: first, place } },
                waypoint: 1, dwell: 0.0, heard: None, heard_t: 0.0,
                phase: Phase::Patrol, visible_now: false, observed_fix: None, observed_t: 0.0,
                assault: Assault::default(), known_fix: None, linger_s: 0.0,
            });
        }
        if let Some(streets) = &mut self.streets {
            streets.legs.extend([None, None, None]);
            streets.closing.extend([None, None, None]);
        }
        first
    }
    pub fn slots(&self) -> usize { self.crews.len() }
    pub fn streets(&self) -> Option<&Streets> { self.streets.as_ref() }
    pub fn route(&self, platoon: usize) -> Vec<Vec2> { self.routes[platoon].clone() }
    pub fn set_route(&mut self, platoon: usize, route: Vec<Vec2>) { self.routes[platoon] = route; }
    /// Where a fielded tank starts: the leader on the beat's first corner, wingmen in column
    /// behind him along its first leg.
    pub fn start(&self, i: usize) -> (Vec2, f32) {
        let route = &self.routes[platoon_of(i)];
        let dir = (route[1] - route[0]).normalize_or_zero();
        (route[0] - dir * SPACING_M * (i % PLATOON_SIZE) as f32, dir.x.atan2(dir.y))
    }
    pub fn role(&self, i: usize) -> Role { self.crews[i].role }
    pub fn visible(&self, i: usize) -> bool { self.crews.get(i).is_some_and(|c| c.visible_now) }
    pub fn set_visible(&mut self, i: usize, value: bool) {
        if let Some(crew) = self.crews.get_mut(i) { crew.visible_now = value; }
    }
    pub fn phase(&self, i: usize) -> Phase { self.crews[i].phase }
    /// Where the crew is looking for the target: its armour fix or its own last sighting.
    pub fn cue(&self, i: usize) -> Option<Vec3> {
        self.crews.get(i).and_then(|c| c.known_fix.or(c.observed_fix))
    }
    /// Debug/capture telemetry: the crew's armour-fighting state (docs/ENEMY_TANK_AI.md).
    pub fn doctrine(&self, i: usize) -> serde_json::Value {
        let c = &self.crews[i];
        let a = &c.assault;
        serde_json::json!({"active":a.active,"fix":c.known_fix.map(|f|[f.x,f.z]),"post":a.post.map(|p|[p.x,p.y]),
            "arrived":a.arrived,"at_post_s":a.at_post_s,"shots_at_post":a.shots_at_post,"withdraw_s":a.withdraw_s,
            "press":a.press,"refused":a.refused.iter().flatten().count(),"heard":c.heard.map(|h|[h.x,h.z])})
    }
    pub fn tick(&mut self, dt: f32) { self.elapsed += dt.max(0.0); }

    /// A real hostile muzzle report, delivered once at discharge. Obstruction attenuates
    /// hearing; sound supplies an area to investigate, never a line to fire along. A wingman
    /// signals what it heard to its leader, who decides.
    pub fn hear(&mut self, i: usize, listener: Vec3, origin: Vec3, range: f32, obstructed: bool) {
        if i >= self.crews.len() { return; }
        let range = range * if obstructed { 0.62 } else { 1.0 };
        if !origin.is_finite() || listener.distance(origin) > range { return; }
        let uncertainty = [Vec2::new(11.0, -13.0), Vec2::new(-15.0, 7.0), Vec2::new(5.0, 17.0),
            Vec2::new(-9.0, -16.0), Vec2::new(14.0, 9.0), Vec2::new(-4.0, -12.0)][i % SLOTS];
        let heard = Vec3::new((origin.x / 32.0).round() * 32.0 + uncertainty.x,
            (origin.y / 4.0).round() * 4.0, (origin.z / 32.0).round() * 32.0 + uncertainty.y);
        let keeper = match self.crews[i].role { Role::Wingman { leader, .. } => leader, _ => i };
        let crew = &mut self.crews[keeper];
        crew.heard = Some(heard);
        crew.heard_t = SOUND_MEMORY_S;
        self.sounds_heard = self.sounds_heard.saturating_add(1);
    }

    /// A crew saw the target's muzzle flash at `origin`: where it was at that moment, as exact as a
    /// sighting but no track of it afterwards. A wingman signals it to its leader as well.
    pub fn saw_flash(&mut self, i: usize, origin: Vec3) {
        if i >= self.crews.len() || !origin.is_finite() { return; }
        let keeper = match self.crews[i].role { Role::Wingman { leader, .. } => Some(leader), _ => None };
        for k in std::iter::once(i).chain(keeper) {
            let c = &mut self.crews[k];
            c.observed_fix = Some(origin);
            c.observed_t = super::stealth::SEARCH_TIMEOUT;
        }
        self.flashes_seen = self.flashes_seen.saturating_add(1);
    }

    /// Orders for every slot. `hulls[i]` is Some for a fielded tank that is still alive.
    pub fn orders(&mut self, hulls: &[Option<Hull>], observations: &[Observation],
        dt: f32, sees: SightTest, ground: Ground) -> Vec<Option<Order>> {
        let dt = dt.max(0.0);
        self.update_roles(hulls, dt);
        self.update_trails(hulls);
        let relayed = self.update_relay(hulls, observations, dt);
        let mut platoon_tanks = vec![0usize; self.routes.len()];
        for i in 0..self.crews.len() {
            if hulls[i].is_some() && !matches!(self.crews[i].role, Role::Absent) {
                platoon_tanks[self.command_platoon(i)] += 1;
            }
        }
        let mut platoon_sees = vec![false; self.routes.len()];
        for i in 0..self.crews.len() {
            if hulls[i].is_some() && observations[i].visible.is_some() && !observations[i].on_foot {
                platoon_sees[self.command_platoon(i)] = true;
            }
        }
        let mut out = vec![None; self.crews.len()];
        // Leaders and independent tanks decide first; wingmen then read their leader's order.
        for i in 0..self.crews.len() {
            let Some(hull) = hulls[i] else { continue; };
            let mut observation = observations[i];
            match self.crews[i].role {
                Role::Leader => {
                    let platoon = platoon_of(i);
                    observation.radio_fix = observation.radio_fix
                        .or_else(|| relayed.filter(|r| r.from != platoon).map(|r| r.fix));
                }
                // No radio and no leader to watch: only its own senses and memory.
                Role::Straggler | Role::Leaderless { .. } => observation.radio_fix = None,
                Role::Wingman { .. } | Role::Absent => continue,
            }
            if let Some((fix, visible)) = self.armour_fix(i, &observation).filter(|_| self.within_leash(i, hull)) {
                out[i] = Some(self.assault(i, hull, observation, fix, visible, hulls, &platoon_sees, &platoon_tanks, dt, sees, ground));
                continue;
            }
            self.stand_down(i);
            out[i] = match self.crews[i].role {
                Role::Leader | Role::Straggler => Some(self.lead(i, platoon_of(i), hull, observation, dt, sees)),
                _ => Some(self.hold(i, hull, observation, dt)),
            };
        }
        for i in 0..self.crews.len() {
            let (Some(hull), Role::Wingman { leader, place }) = (hulls[i], self.crews[i].role)
            else { continue; };
            let (Some(leader_hull), Some(leader_order)) = (hulls[leader], out[leader]) else { continue; };
            // A wingman watching its leader's tank fights the armour the leader is fighting.
            let observation = Observation { radio_fix: None, ..observations[i] };
            if let Some((fix, visible)) = self.armour_fix(i, &observation).filter(|_| self.within_leash(i, hull)) {
                out[i] = Some(self.assault(i, hull, observation, fix, visible, hulls, &platoon_sees, &platoon_tanks, dt, sees, ground));
                continue;
            }
            self.stand_down(i);
            out[i] = Some(self.follow(i, hull, observations[i], leader_hull, leader_order, place, dt));
        }
        out
    }

    /// In contact with armour a tank follows it up to `ASSAULT_LEASH_M` from its beat.
    fn within_leash(&self, i: usize, hull: Hull) -> bool {
        xz(hull.pos).distance(self.routes[platoon_of(i)][0]) <= ASSAULT_LEASH_M
    }

    /// The platoon whose leader this tank takes its orders from.
    fn command_platoon(&self, i: usize) -> usize {
        match self.crews[i].role { Role::Wingman { leader, .. } => platoon_of(leader), _ => platoon_of(i) }
    }

    /// The armoured target this crew knows of: in its own sight, in its own memory, by radio (a
    /// leader) or by its leader's signals (a wingman). A man on foot is not fought this way.
    fn armour_fix(&self, i: usize, observation: &Observation) -> Option<(Vec3, bool)> {
        if observation.on_foot { return None; }
        if let Some(target) = observation.visible { return Some((target, true)); }
        let c = &self.crews[i];
        c.observed_fix.or(match c.role {
            Role::Leader | Role::Straggler => observation.radio_fix,
            Role::Wingman { leader, .. } => self.crews[leader].known_fix,
            _ => None,
        }).map(|fix| (fix, false))
    }

    /// Contact with armour lost: the crew returns to its ordinary orders.
    fn stand_down(&mut self, i: usize) {
        let c = &mut self.crews[i];
        c.known_fix = None;
        if c.assault.active { c.assault = Assault::default(); }
    }

    /// Fix and flank (docs/ENEMY_TANK_AI.md). The leader holds a hull-down post and fires to fix
    /// the target; wingmen bound along the streets to posts on its flanks and rear where the
    /// crew's armour solution says a 76 mm round goes through, one tank of a platoon moving at a
    /// time while the target is in the platoon's sight. A crew shoots twice from a post, then
    /// moves; under the target's gun or after a hit it backs out of the line of fire.
    #[allow(clippy::too_many_arguments)]
    fn assault(&mut self, i: usize, hull: Hull, observation: Observation, fix: Vec3, visible: bool,
        hulls: &[Option<Hull>], platoon_sees: &[bool], platoon_tanks: &[usize], dt: f32, sees: SightTest,
        ground: Ground) -> Order {
        let platoon = self.command_platoon(i);
        let place = match self.crews[i].role { Role::Wingman { place, .. } => place, _ => 0 };
        {
            let c = &mut self.crews[i];
            remember(c, observation, dt, &mut self.confirmed_sightings);
            c.known_fix = Some(fix);
            c.heard = None;
            c.heard_t = 0.0;
        }
        let mut a = self.crews[i].assault;
        if !a.active { a = Assault { active: true, shots_seen: observation.shots, ..Assault::default() }; }
        if visible { if let Some(h) = observation.heading { a.heading = Some(h); } }
        let pos = xz(hull.pos);
        let target = xz(fix);
        // Unseen, the crew assumes the target faces it: never a hidden pose.
        let heading = a.heading.unwrap_or_else(|| { let d = pos - target; d.x.atan2(d.y) });
        let range = pos.distance(target);
        let margin = tank_doctrine::kill_margin_mm(&crate::ballistics::BR_350B, heading, target, pos);
        if observation.shots > a.shots_seen {
            a.shots_at_post += observation.shots - a.shots_seen;
            a.halt_s = 0.0;
        }
        a.shots_seen = observation.shots;
        a.since_choice_s += dt;
        if a.last_drive.abs() > 0.15 && hull.speed.abs() < 0.3 { a.stuck_s += dt; }
        else { a.stuck_s = (a.stuck_s - dt).max(0.0); }
        let to = target - pos;
        let to_target = wrap(to.x.atan2(to.y) - hull.heading);
        let face = |error: f32| if error.abs() < 0.08 { 0.0 } else { (error * 1.6).clamp(-0.55, 0.55) };

        // Reactions: a hit or the target's gun on an exposed tank sends it back out of the line
        // of fire, unless it holds a loaded killing shot; a pinned tank backs off and re-plans.
        if a.withdraw_s <= 0.0 {
            let killing_shot = visible && margin > 0.0 && observation.loaded;
            let jitter = [0.0, 0.7, 1.4][i % 3];
            if observation.struck && !killing_shot {
                a.withdraw_s = WITHDRAW_S + jitter;
            } else if visible && observation.gun_on_me && margin <= 0.0 && a.shots_at_post >= 1 {
                a.withdraw_s = WITHDRAW_S * 0.7 + jitter;
            } else if a.stuck_s > STUCK_S {
                a.withdraw_s = 2.0 + jitter;
                a.stuck_s = 0.0;
            }
            if a.withdraw_s > 0.0 {
                if debug_log() {
                    println!("[t34-ai] t={:.1} #{i} withdraws (struck {}, gun on it {}, pinned {})", self.elapsed,
                        observation.struck, observation.gun_on_me, a.stuck_s == 0.0 && !observation.struck && !observation.gun_on_me);
                }
                a.post = None;
                a.arrived = false;
                if let Some(streets) = self.streets.as_mut() { streets.legs[i] = None; }
            }
        }

        let (drive, steer) = if a.withdraw_s > 0.0 {
            a.withdraw_s -= dt;
            // Back away with the nose (the thickest plate, the gun) kept on the threat. A tank
            // pinned while reversing stops trying.
            if a.stuck_s > STUCK_S { a.withdraw_s = 0.0; a.stuck_s = 0.0; }
            (-0.6, face(to_target))
        } else {
            let lost = a.arrived && !visible && a.at_post_s > POST_HOLD_S;
            let replace = a.post.is_none()
                || a.against.distance(target) > 60.0
                || (a.arrived && a.shots_at_post >= SHOTS_PER_POST)
                || (a.arrived && visible && observation.gun_on_me && a.shots_at_post >= 1)
                || lost;
            if replace && a.since_choice_s >= DECIDE_S
                && self.may_bound(platoon, i, platoon_sees[platoon], platoon_tanks[platoon], range, hulls) {
                if lost { a.press += 1; }
                if visible { a.press = 0; }
                a.post = self.choose_post(i, place, pos, target, heading, a.press, &a.refused, hulls, sees, ground);
                if debug_log() {
                    println!("[t34-ai] t={:.1} #{i} place {place} at {pos} fix {target} range {range:.0} seen {visible} margin {margin:.0} -> post {:?}",
                        self.elapsed, a.post);
                }
                a.against = target;
                a.arrived = false;
                a.at_post_s = 0.0;
                a.shots_at_post = 0;
                a.since_choice_s = 0.0;
            }
            match a.post {
                Some(post) if !a.arrived => {
                    if visible && margin > 0.0 && observation.loaded && range < 500.0 && a.halt_s < 4.0 {
                        // A short halt for a killing shot on the way.
                        a.halt_s += dt;
                        (0.0, face(to_target))
                    } else {
                        let speed = if platoon_sees[platoon] { BOUND_SPEED } else { CLOSE_SPEED };
                        let step = match self.streets.as_mut() {
                            Some(streets) => streets.drive_to(i, pos, hull.heading, post, speed, 6.0),
                            None => Some(navigate_arrive(pos, hull.heading, post, speed, 6.0)),
                        };
                        match step {
                            Some((_, _, true)) => {
                                a.arrived = true;
                                (0.0, face(to_target))
                            }
                            Some((drive, steer, false)) => (drive, steer),
                            None => {
                                // No street leads there: remember it and choose again.
                                if debug_log() { println!("[t34-ai] t={:.1} #{i} no street to {post}", self.elapsed); }
                                a.refused.rotate_right(1);
                                a.refused[0] = Some(post);
                                a.post = None;
                                (0.0, face(to_target))
                            }
                        }
                    }
                }
                _ => {
                    a.at_post_s += dt;
                    (0.0, face(to_target))
                }
            }
        };
        a.last_drive = drive;
        self.crews[i].assault = a;
        // The leader fires to fix the target; others fire where the round can kill, when the
        // target's gun is on them, or at close range.
        let fire = visible && (margin > 0.0 || observation.gun_on_me || range < 300.0 || place == 0);
        let phase = if visible { Phase::Track } else { Phase::Search };
        let aim = if visible { fix } else {
            let radial = (pos - target).normalize_or_zero();
            let sweep = (self.elapsed * 0.16 + i as f32 * 1.7).sin() * 18.0;
            fix + Vec3::new(radial.y * sweep, 0.0, -radial.x * sweep)
        };
        self.crews[i].phase = phase;
        Order { phase, drive, steer, aim, fire }
    }

    /// Bounding overwatch: with the target in the platoon's sight and close, at least one tank
    /// stays at its post covering while the others move. A bound ends when the mover arrives,
    /// finds no street, is pinned or is driven back.
    fn may_bound(&self, platoon: usize, i: usize, seen: bool, tanks: usize, range: f32, hulls: &[Option<Hull>]) -> bool {
        if !seen || range > OVERWATCH_RANGE_M { return true; }
        let movers = (0..self.crews.len()).filter(|&j| j != i && hulls[j].is_some()
            && self.command_platoon(j) == platoon && {
                let a = &self.crews[j].assault;
                a.active && a.post.is_some() && !a.arrived && a.withdraw_s <= 0.0
            }).count();
        movers < tanks.saturating_sub(1).max(1)
    }

    /// The next firing post for tank `i` against a target at `target` facing `heading`: a ring of
    /// candidates on the target's flanks and rear (the leader nearer its own bearing), scored by
    /// the armour solution there, the distance to drive, exposure to the target's frontal arc on
    /// the way, spacing from the platoon's other tanks and posts, then by sight probes from the
    /// best few: the target in view from the cupola, and the hull roof below the line (hull-down).
    #[allow(clippy::too_many_arguments)]
    fn choose_post(&self, i: usize, place: usize, pos: Vec2, target: Vec2, heading: f32, press: i32,
        refused: &[Option<Vec2>], hulls: &[Option<Hull>], sees: SightTest, ground: Ground) -> Option<Vec2> {
        let own = tank_doctrine::relative_bearing(heading, target, pos);
        let side_of = |b: f32| if b >= 0.0 { 1.0 } else { -1.0 };
        let leader_side = match self.crews[i].role {
            Role::Wingman { leader, .. } => hulls.get(leader).copied().flatten()
                .map_or(side_of(own), |h| side_of(tank_doctrine::relative_bearing(heading, target, xz(h.pos)))),
            _ => side_of(own),
        };
        // Pincer: the first wingman takes the leader's other side, the second goes deeper on his;
        // the leader stays in its own sector. Preferences, not rails: in a canyon the far flank may
        // be rock, and any post a street reaches is better than none.
        let (side, ideal) = match place {
            0 => (side_of(own), own.abs().max(FRONT_ARC_RAD + 0.25)),
            p if p % 2 == 1 => (-leader_side, 1.9),
            _ => (leader_side, 2.4),
        };
        let scale = 0.8f32.powi(press.clamp(0, 6));
        let mut others: Vec<Vec2> = Vec::new();
        for (j, h) in hulls.iter().enumerate() {
            let Some(h) = h else { continue; };
            if j == i { continue; }
            others.push(xz(h.pos));
            if let Some(post) = self.crews.get(j).and_then(|c| c.assault.post) { others.push(post); }
        }
        let mut candidates: Vec<(f32, Vec2)> = Vec::new();
        for k in 0..CANDIDATE_BEARINGS {
            let bearing = -std::f32::consts::PI + (k as f32 + 0.5) * std::f32::consts::TAU / CANDIDATE_BEARINGS as f32;
            for r in [140.0f32, 200.0, 270.0, 350.0, 430.0] {
                let r = (r * scale).max(110.0);
                let a = heading + bearing;
                let raw = target + Vec2::new(a.sin(), a.cos()) * r;
                let c = match &self.streets {
                    Some(s) => match s.nav.nearest_clear(raw, 20.0).filter(|&c| s.nav.reachable(pos, c)) {
                        Some(c) => c,
                        None => continue,
                    },
                    None => raw,
                };
                let b = tank_doctrine::relative_bearing(heading, target, c);
                let margin = tank_doctrine::kill_margin_mm(&crate::ballistics::BR_350B, heading, target, c);
                let mut score = if margin > 0.0 { 40.0 + margin.min(40.0) } else { margin.max(-80.0) * 0.5 };
                if place == 0 { score *= 0.4; }
                // The crew's side of the pincer and its depth round the target.
                if side_of(b) != side { score -= 20.0; }
                score -= (b.abs() - ideal).abs() * 12.0;
                score -= pos.distance(c) / 12.0;
                score -= (target.distance(c) - 260.0).abs() / 10.0;
                // The straight way there through the target's frontal arc.
                if (1..=4).any(|k| {
                    let p = pos.lerp(c, k as f32 / 4.0);
                    p.distance(target) < 600.0 && tank_doctrine::relative_bearing(heading, target, p).abs() < FRONT_ARC_RAD
                }) { score -= 30.0; }
                if others.iter().any(|o| o.distance(c) < 40.0) { score -= 50.0; }
                if refused.iter().flatten().any(|r| r.distance(c) < REFUSED_RADIUS_M) { continue; }
                candidates.push((score, c));
            }
        }
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        candidates.truncate(6);
        candidates.into_iter().map(|(score, c)| {
            let floor = ground(c);
            let high = sees(Vec3::new(c.x, floor + TURRET_EYE_M, c.y), target);
            let low = high && sees(Vec3::new(c.x, floor + HULL_EYE_M, c.y), target);
            (score + if high { 35.0 } else { 0.0 } + if high && !low { 20.0 } else { 0.0 }, c)
        }).max_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, c)| c)
    }

    /// A leader's death leaves its wingmen leaderless; after the regroup wait they join another
    /// living leader in formation, or wander if none is left.
    fn update_roles(&mut self, hulls: &[Option<Hull>], dt: f32) {
        for i in 0..self.crews.len() {
            if hulls[i].is_none() { continue; }
            match self.crews[i].role {
                Role::Wingman { leader, .. } if hulls[leader].is_none() => {
                    self.crews[i].role = Role::Leaderless { waited: 0.0 };
                }
                Role::Leaderless { waited } => {
                    let waited = waited + dt;
                    self.crews[i].role = if waited < REGROUP_S {
                        Role::Leaderless { waited }
                    } else if let Some(leader) = (0..self.routes.len()).map(|p| slot(p, 0))
                        .filter(|&l| hulls[l].is_some() && self.crews[l].role == Role::Leader)
                        .filter(|&l| hulls[l].zip(hulls[i]).is_some_and(|(a, b)| xz(a.pos).distance(xz(b.pos)) <= JOIN_RANGE_M))
                        .min_by(|&a, &b| {
                            let d = |l: usize| xz(hulls[l].unwrap().pos).distance(xz(hulls[i].unwrap().pos));
                            d(a).total_cmp(&d(b))
                        })
                    {
                        // The lowest column place no living wingman of this leader holds. Counting
                        // wingmen instead handed out a place that a surviving wingman still
                        // occupied whenever an earlier place had been vacated.
                        let held: Vec<usize> = (0..self.crews.len()).filter(|&j| hulls[j].is_some())
                            .filter_map(|j| match self.crews[j].role {
                                Role::Wingman { leader: l, place } if l == leader => Some(place),
                                _ => None,
                            }).collect();
                        let place = (1..).find(|p| !held.contains(p)).unwrap_or(1);
                        Role::Wingman { leader, place }
                    } else {
                        Role::Straggler
                    };
                }
                _ => {}
            }
        }
    }

    fn update_trails(&mut self, hulls: &[Option<Hull>]) {
        for platoon in 0..self.routes.len() {
            let Some(leader) = hulls[slot(platoon, 0)] else { continue; };
            let here = xz(leader.pos);
            let trail = &mut self.trails[platoon];
            if trail.is_empty() {
                // Seed a straight track behind the leader so wingmen mustered in column behind
                // him have a path to hold from the first frame.
                let back = -Vec2::new(leader.heading.sin(), leader.heading.cos());
                trail.extend((0..TRAIL_POINTS).map(|k| here + back * TRAIL_STEP_M * k as f32));
            } else if trail[0].distance(here) >= TRAIL_STEP_M {
                trail.insert(0, here);
                trail.truncate(TRAIL_POINTS);
            }
        }
    }

    /// The company net: a living leader in contact reports its fix; the other living leader
    /// receives it after the hand-off time. Only leaders transmit or receive.
    fn update_relay(&mut self, hulls: &[Option<Hull>], observations: &[Observation],
        dt: f32) -> Option<Relay> {
        let reporting = (0..self.routes.len()).find_map(|platoon| {
            let leader = slot(platoon, 0);
            (hulls[leader].is_some() && self.crews[leader].role == Role::Leader).then_some(())?;
            let observation = observations[leader];
            observation.visible.or(observation.radio_fix).map(|fix| (platoon, fix))
        });
        self.relay = match (reporting, self.relay) {
            (Some((from, fix)), Some(r)) if r.from == from =>
                Some(Relay { fix, from, held_s: r.held_s + dt, silent_s: 0.0 }),
            (Some((from, fix)), _) => Some(Relay { fix, from, held_s: 0.0, silent_s: 0.0 }),
            (None, Some(r)) if r.silent_s + dt < RELAY_MEMORY_S =>
                Some(Relay { silent_s: r.silent_s + dt, ..r }),
            (None, _) => None,
        };
        // A report is only heard if the sender kept it up for the whole hand-off.
        self.relay.filter(|r| r.held_s >= RELAY_S)
    }

    /// The platoon leader's (or a straggler's) own decision: patrol, investigate, search, track.
    fn lead(&mut self, i: usize, platoon: usize, hull: Hull, observation: Observation, dt: f32,
        sees: SightTest) -> Order {
        let pos = hull.pos;
        let heading = hull.heading;
        let c = &mut self.crews[i];
        remember(c, observation, dt, &mut self.confirmed_sightings);
        let remembered = observation.radio_fix.or(c.observed_fix);
        if remembered.is_some() { c.heard = None; c.heard_t = 0.0; }
        let (phase, aim, drive, steer) = if let Some(target) = observation.visible {
            let to = xz(target - pos);
            let range = to.length();
            let (near, far) = if observation.on_foot { (100.0, 145.0) } else { (165.0, 235.0) };
            let error = wrap(to.x.atan2(to.y) - heading);
            let drive = if range < near && error.abs() < 0.8 { -0.17 }
                else if range > far && error.abs() < 0.65 { 0.20 }
                else { 0.0 };
            (Phase::Track, target, drive, (error * 1.8).clamp(-0.55, 0.55))
        } else if let Some(fix) = remembered.or(c.heard) {
            let phase = if remembered.is_some() { Phase::Search } else { Phase::Investigate };
            let to = xz(fix - pos);
            let range = to.length();
            let error = wrap(to.x.atan2(to.y) - heading);
            // Sweep from outside the reported area. Never drive through the fix or orbit
            // using a hidden player's heading. Each platoon takes a separate flank of the area.
            let radial = xz(pos - fix).normalize_or_zero();
            let tangent = Vec2::new(-radial.y, radial.x) * [-28.0, 28.0][platoon % 2];
            let goal = xz(fix) + radial * (112.0 + (platoon % 2) as f32 * 12.0) + tangent;
            let (drive, steer) = if let Some(streets) = self.streets.as_mut() {
                // In the streets: along them to the flank position outside the reported area, then
                // on toward the area itself, halting wherever it comes into view (never through it).
                let key = xz(fix);
                let (key, flanked) = streets.closing[i].filter(|(k, _)| k.distance(key) < 24.0).unwrap_or((key, false));
                if range < STREET_VIEW_HALT_M && sees(pos + Vec3::Y * EYE_ABOVE_CENTRE_M, key) {
                    streets.closing[i] = Some((key, true));
                    streets.legs[i] = None;
                    (0.0, (error * 1.8).clamp(-0.45, 0.45))
                } else if let Some((drive, steer, arrived)) = (!flanked)
                    .then(|| streets.nav.nearest_clear(goal, 40.0)).flatten()
                    .and_then(|flank| streets.drive_to(i, xz(pos), heading, flank, INVESTIGATE_SPEED, 12.0)) {
                    streets.closing[i] = Some((key, arrived));
                    (drive, steer)
                } else {
                    // No street to the flank position (or reached it): close on the area itself.
                    streets.closing[i] = Some((key, true));
                    streets.drive_to(i, xz(pos), heading, key, INVESTIGATE_SPEED, STREET_CLOSE_M)
                        .map_or((0.0, 0.0), |(drive, steer, _)| (drive, steer))
                }
            } else if range < 105.0 { (if error.abs() < 0.8 { -0.12 } else { 0.0 }, (error * 1.8).clamp(-0.45, 0.45)) }
                else { navigate(xz(pos), heading, goal, INVESTIGATE_SPEED, 12.0) };
            let sweep = (self.elapsed * 0.16 + i as f32 * 1.7).sin() * 24.0;
            (phase, fix + Vec3::new(radial.y * sweep, 0.0, -radial.x * sweep), drive, steer)
        } else {
            let route = &self.routes[platoon];
            if xz(pos).distance(route[c.waypoint]) < 6.0 {
                c.waypoint = (c.waypoint + 1) % route.len();
                c.dwell = 10.0 + (platoon % 2) as f32 * 4.0;
            }
            c.dwell = (c.dwell - dt).max(0.0);
            let corner = route[c.waypoint];
            // The leader walks slowly enough that wingmen closing up in snow can keep station.
            let (drive, steer) = if c.dwell > 0.0 { (0.0, 0.0) }
                else if let Some(streets) = self.streets.as_mut() {
                    match streets.drive_to(i, xz(pos), heading, corner, 0.12, 5.0) {
                        // The street route reports arrival at a corner it can only reach to within
                        // its own tolerance; without this the patrol halted there for good.
                        Some((_, _, true)) => {
                            c.waypoint = (c.waypoint + 1) % self.routes[platoon].len();
                            c.dwell = 10.0 + (platoon % 2) as f32 * 4.0;
                            (0.0, 0.0)
                        }
                        Some((drive, steer, false)) => (drive, steer),
                        None => {
                            // No street leads to this corner from here: walk on to the next.
                            c.waypoint = (c.waypoint + 1) % self.routes[platoon].len();
                            c.dwell = 10.0 + (platoon % 2) as f32 * 4.0;
                            (0.0, 0.0)
                        }
                    }
                }
                else { navigate(xz(pos), heading, corner, 0.12, 5.0) };
            let sweep = heading + (self.elapsed * 0.12 + i as f32 * 1.4).sin() * 0.9;
            (Phase::Patrol, pos + Vec3::new(sweep.sin(), 0.0, sweep.cos()) * 140.0, drive, steer)
        };
        c.phase = phase;
        if phase == Phase::Investigate && drive == 0.0 {
            c.linger_s += dt;
            if c.linger_s > INVESTIGATE_LINGER_S { c.heard = None; c.heard_t = 0.0; c.linger_s = 0.0; }
        } else { c.linger_s = 0.0; }
        // Patrol/contact area stays local. A retreating player cannot drag this encounter
        // across the act; observation can continue but movement returns toward the route.
        let home = self.routes[platoon][0];
        let (drive, steer) = if xz(pos).distance(home) > 650.0 {
            match self.streets.as_mut() {
                Some(streets) => streets.drive_to(i, xz(pos), heading, home, 0.22, 8.0)
                    .map_or((0.0, 0.0), |(drive, steer, _)| (drive, steer)),
                None => navigate(xz(pos), heading, home, 0.22, 8.0),
            }
        } else { (drive, steer) };
        Order { phase, drive, steer, aim, fire: observation.visible.is_some() }
    }

    /// A wingman keeps its place — in column along the leader's track, or in line abreast with
    /// the leader in the centre once he fights — and fires on the target he engages. A wingman
    /// that sees the target itself engages it from the halt.
    fn follow(&mut self, i: usize, hull: Hull, observation: Observation, leader: Hull,
        leader_order: Order, place: usize, dt: f32) -> Order {
        let c = &mut self.crews[i];
        remember(c, Observation { radio_fix: None, ..observation }, dt, &mut self.confirmed_sightings);
        let platoon = platoon_of(match c.role { Role::Wingman { leader: l, .. } => l, _ => i });
        // A street is no place for line abreast: there the platoon fights in column, each wingman
        // firing past on what it sees itself.
        let line = leader_order.phase == Phase::Track && self.streets.is_none();
        let goal = if line {
            let right = Vec2::new(leader.heading.cos(), -leader.heading.sin());
            let side = if place % 2 == 1 { ((place + 1) / 2) as f32 } else { -((place / 2) as f32) };
            xz(leader.pos) + right * SPACING_M * side
        } else {
            along_trail(&self.trails[platoon], xz(leader.pos), SPACING_M * place as f32)
        };
        let to = goal - xz(hull.pos);
        let (drive, steer) = if let Some(target) = observation.visible {
            // Fire from the halt, turning the hull toward the target only.
            let t = xz(target - hull.pos);
            (0.0, (wrap(t.x.atan2(t.y) - hull.heading) * 1.8).clamp(-0.55, 0.55))
        } else if to.length() > 8.0 {
            // Close up faster than the leader walks; ease off as the place is reached.
            match self.streets.as_mut() {
                // In the streets its place on the leader's track is reached along the streets too.
                Some(streets) => streets.drive_to(i, xz(hull.pos), hull.heading, goal,
                    (to.length() / 60.0).clamp(0.12, 0.30), 4.0)
                    .map_or((0.0, 0.0), |(drive, steer, _)| (drive, steer)),
                None => navigate(xz(hull.pos), hull.heading, goal, (to.length() / 60.0).clamp(0.12, 0.30), 8.0),
            }
        } else {
            // On station: copy the leader's halt and face his way.
            (0.0, (wrap(leader.heading - hull.heading) * 1.5).clamp(-0.45, 0.45))
        };
        let (phase, aim, fire) = if let Some(target) = observation.visible {
            (Phase::Track, target, true)
        } else if leader_order.fire {
            // The leader's tracer marks his target; the caller still requires a clear line.
            (Phase::Track, leader_order.aim, true)
        } else {
            (leader_order.phase, leader_order.aim, false)
        };
        c.phase = phase;
        Order { phase, drive, steer, aim, fire }
    }

    /// A tank that lost its leader holds its ground and fights from the halt on its own sight.
    fn hold(&mut self, i: usize, hull: Hull, observation: Observation, dt: f32) -> Order {
        let c = &mut self.crews[i];
        remember(c, Observation { radio_fix: None, ..observation }, dt, &mut self.confirmed_sightings);
        let (phase, aim, steer) = if let Some(target) = observation.visible {
            let t = xz(target - hull.pos);
            (Phase::Track, target, (wrap(t.x.atan2(t.y) - hull.heading) * 1.8).clamp(-0.55, 0.55))
        } else if let Some(fix) = c.observed_fix.or(c.heard) {
            (Phase::Search, fix, 0.0)
        } else {
            let sweep = hull.heading + (self.elapsed * 0.2 + i as f32).sin() * 1.2;
            (Phase::Patrol, hull.pos + Vec3::new(sweep.sin(), 0.0, sweep.cos()) * 140.0, 0.0)
        };
        c.phase = phase;
        Order { phase, drive: 0.0, steer, aim, fire: observation.visible.is_some() }
    }
}

/// Age a crew's sound and sight memories and record a fresh sighting.
fn remember(c: &mut Crew, observation: Observation, dt: f32, confirmed: &mut u32) {
    c.heard_t = (c.heard_t - dt).max(0.0);
    c.observed_t = (c.observed_t - dt).max(0.0);
    if c.heard_t == 0.0 { c.heard = None; }
    if c.observed_t == 0.0 { c.observed_fix = None; }
    if let Some(target) = observation.visible {
        if c.phase != Phase::Track { *confirmed += 1; }
        c.observed_fix = Some(target);
        c.observed_t = super::stealth::SEARCH_TIMEOUT;
    }
}

/// `KESSEL_T34_DBG=1`: print the crews' armour-fighting decisions (posts, refusals, withdrawals).
fn debug_log() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KESSEL_T34_DBG").as_deref() == Ok("1"))
}

/// The point `distance` metres back along a leader's track (newest point first).
fn along_trail(trail: &[Vec2], leader: Vec2, distance: f32) -> Vec2 {
    let mut left = distance;
    let mut previous = leader;
    for &point in trail {
        let step = previous.distance(point);
        if step >= left && step > 1e-4 {
            return previous + (point - previous) * (left / step);
        }
        left -= step;
        previous = point;
    }
    previous
}

fn xz(v: Vec3) -> Vec2 { Vec2::new(v.x, v.z) }
fn wrap(a: f32) -> f32 { (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI }
/// `navigate` on open ground, reporting arrival like `Streets::drive_to`.
fn navigate_arrive(pos: Vec2, heading: f32, goal: Vec2, speed: f32, stop: f32) -> (f32, f32, bool) {
    if pos.distance(goal) <= stop { return (0.0, 0.0, true); }
    let (drive, steer) = navigate(pos, heading, goal, speed, stop);
    (drive, steer, false)
}
fn navigate(pos: Vec2, heading: f32, goal: Vec2, speed: f32, stop: f32) -> (f32, f32) {
    let to = goal - pos;
    if to.length() <= stop { return (0.0, 0.0); }
    let error = wrap(to.x.atan2(to.y) - heading);
    let drive = if error.abs() > 0.65 { 0.0 } else { speed * (to.length() / 20.0).clamp(0.35, 1.0) };
    (drive, (error * 2.0).clamp(-0.65, 0.65))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::stealth;
    use crate::tank::Tank;
    use super::super::street_nav::StreetNav;

    #[test]
    fn appended_platoons_keep_independent_roles_and_can_patrol_investigate_and_search() {
        let mut ai=WinterEncounter::new();
        let old_role=ai.role(1);
        for p in 2..14 {
            let a=Vec2::new(p as f32*180.0,0.0);
            let first=ai.append_platoon(vec![a,a+Vec2::Y*240.0],3);
            assert_eq!(first,p*PLATOON_SIZE);
            assert_eq!(ai.role(first),Role::Leader);
            assert_eq!(ai.role(first+2),Role::Wingman{leader:first,place:2});
        }
        assert_eq!(ai.role(1),old_role);
        let hulls: Vec<_>=(0..ai.slots()).map(|i| {
            let(p,heading)=ai.start(i);Some(Hull{pos:Vec3::new(p.x,1.0,p.y),heading,speed:0.0})
        }).collect();
        let mut obs=vec![unknown();ai.slots()];
        assert_eq!(ai.orders(&hulls,&obs,0.1,&|_,_|true,&|_|0.0).len(),ai.slots());
        for i in 6..ai.slots() {ai.hear(i,hulls[i].unwrap().pos,hulls[i].unwrap().pos+Vec3::Z*200.0,900.0,false);}
        ai.orders(&hulls,&obs,0.1,&|_,_|true,&|_|0.0);
        for o in &mut obs {o.radio_fix=Some(Vec3::new(1200.0,1.0,300.0));}
        let orders=ai.orders(&hulls,&obs,0.1,&|_,_|false,&|_|0.0);
        assert!(orders.iter().enumerate().filter(|(i,_)|i%3==0).all(|(_,o)|o.is_some_and(|o|o.phase==Phase::Search)));
    }

    fn flat() -> crate::terrain::Heightfield {
        crate::terrain::Heightfield { res: 2, spacing: 8000.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0, openings: Default::default(), detail: None }
    }
    fn unknown() -> Observation { Observation { visible: None, radio_fix: None, on_foot: true, heading: None,
        gun_on_me: false, struck: false, shots: 0, loaded: true } }
    fn blind() -> [Observation; SLOTS] { [unknown(); SLOTS] }
    fn muster(ai: &WinterEncounter) -> Vec<Option<Tank>> {
        (0..SLOTS).map(|i| fielded(i).then(|| {
            let (p, heading) = ai.start(i);
            Tank::new(p.x, p.y, heading, &flat(), crate::tank::T34_PARTS)
        })).collect()
    }
    fn hulls(tanks: &[Option<Tank>]) -> [Option<Hull>; SLOTS] {
        std::array::from_fn(|i| tanks[i].as_ref().map(|t| Hull { pos: t.center(), heading: t.heading, speed: t.speed }))
    }
    fn step(ai: &mut WinterEncounter, tanks: &mut [Option<Tank>], observations: &[Observation; SLOTS],
        dt: f32) -> Vec<Option<Order>> {
        ai.tick(dt);
        let orders = ai.orders(&hulls(tanks), observations, dt, &|_, _| true, &|_| 0.0);
        for i in 0..SLOTS {
            if let (Some(tank), Some(order)) = (tanks[i].as_mut(), orders[i]) {
                tank.update(order.drive, order.steer, 0.0, 0.0, dt, &flat());
            }
        }
        orders
    }
    fn distance(tanks: &[Option<Tank>], a: usize, b: usize) -> f32 {
        tanks[a].as_ref().unwrap().pos.distance(tanks[b].as_ref().unwrap().pos)
    }

    #[test]
    fn the_encounter_fields_a_full_platoon_and_an_attrited_one_in_stealth_platoon_slots() {
        let fielded_slots: Vec<_> = (0..SLOTS).filter(|&i| fielded(i)).collect();
        assert_eq!(fielded_slots, vec![0, 1, 2, 3, 4]);
        let ai = WinterEncounter::new();
        assert_eq!(ai.role(0), Role::Leader);
        assert_eq!(ai.role(1), Role::Wingman { leader: 0, place: 1 });
        assert_eq!(ai.role(2), Role::Wingman { leader: 0, place: 2 });
        assert_eq!(ai.role(3), Role::Leader);
        assert_eq!(ai.role(4), Role::Wingman { leader: 3, place: 1 });
        assert_eq!(ai.role(5), Role::Absent);
        let perception = stealth::Stealth::new(SLOTS);
        for i in 0..SLOTS {
            assert_eq!(perception.platoon_index_of(i), platoon_of(i), "shared knowledge follows the platoon");
        }
        // Wingmen muster in column behind their leader, one interval apart.
        for i in [1, 2, 4] {
            let (leader, _) = ai.start(slot(platoon_of(i), 0));
            let (me, _) = ai.start(i);
            assert!((leader.distance(me) - SPACING_M * (i % PLATOON_SIZE) as f32).abs() < 1e-3);
        }
    }

    #[test]
    fn quiet_platoons_walk_their_beats_in_column_and_copy_the_leaders_halts() {
        let mut ai = WinterEncounter::new();
        let mut tanks = muster(&ai);
        let start: Vec<_> = tanks.iter().map(|t| t.as_ref().map(|t| t.pos)).collect();
        let mut travelled = [0.0f32; SLOTS];
        let mut leader_halted = [0.0f32; 2];
        let mut halts_checked = 0;
        let dt = 0.05;
        for frame in 0..600 * 20 {
            let orders = step(&mut ai, &mut tanks, &blind(), dt);
            for i in (0..SLOTS).filter(|&i| fielded(i)) {
                let tank = tanks[i].as_ref().unwrap();
                if frame < 120 * 20 {
                    for foot in [Vec3::new(1850.0,1.1,250.0),Vec3::new(1880.0,1.1,165.0)] {
                        // Worst case: unobstructed, full front arc, fair-weather moving man.
                        assert_eq!(stealth::accrual_rate(1.0, tank.pos.distance(foot),
                            visibility_radius(800.0 * 0.34,true,0.0),1.3),0.0);
                    }
                }
                let order = orders[i].unwrap();
                assert_eq!(order.phase, Phase::Patrol); assert!(!order.fire);
                travelled[i] = travelled[i].max(tank.pos.distance(start[i].unwrap()));
                assert!(tank.pos.distance(start[i].unwrap()) < 450.0, "beats stay local");
                if let Role::Wingman { leader, place } = ai.role(i) {
                    if frame > 30 * 20 {
                        assert!(distance(&tanks, i, leader) < SPACING_M * place as f32 + 45.0,
                            "wingman {i} keeps station behind its leader");
                    }
                    if leader_halted[platoon_of(leader)] > 8.0 {
                        assert!(tank.speed.abs() < 0.6, "wingman {i} copies the leader's halt");
                        halts_checked += 1;
                    }
                }
            }
            for platoon in 0..2 {
                let leader = slot(platoon, 0);
                leader_halted[platoon] = if orders[leader].unwrap().drive == 0.0 { leader_halted[platoon] + dt } else { 0.0 };
            }
        }
        assert!(halts_checked > 100, "the beats include real halts: {halts_checked}");
        assert!((0..SLOTS).filter(|&i| fielded(i)).all(|i| travelled[i] > 40.0),
            "platoons must move, not merely remain inert: {travelled:?}");
        assert_eq!(ai.confirmed_sightings,0); assert_eq!(ai.sounds_heard,0);
    }

    #[test]
    fn on_contact_with_a_man_on_foot_wingmen_form_line_abreast_and_fire_only_on_the_leaders_target() {
        let mut ai = WinterEncounter::new();
        // Platoon A mustered in column on its own beat; B stays out of this test.
        let mut tanks: Vec<Option<Tank>> = muster(&ai).into_iter().enumerate()
            .map(|(i, tank)| tank.filter(|_| i < 3)).collect();
        let (leader_start, heading) = ai.start(0);
        let ahead = Vec2::new(heading.sin(), heading.cos());
        let target = Vec3::new(leader_start.x + ahead.x * 200.0, 2.0, leader_start.y + ahead.y * 200.0);
        let mut seen = blind();
        seen[0].visible = Some(target);
        let mut orders = vec![None; SLOTS];
        for _ in 0..60 * 10 { orders = step(&mut ai, &mut tanks, &seen, 0.1); }
        let leader = tanks[0].as_ref().unwrap();
        let across = Vec3::new(ahead.y, 0.0, -ahead.x);
        let lateral = |i: usize| (tanks[i].as_ref().unwrap().pos - leader.pos).dot(across);
        assert!((lateral(1).abs() - SPACING_M).abs() < 12.0 && (lateral(2).abs() - SPACING_M).abs() < 12.0,
            "wingmen deploy an interval either side: {} {}", lateral(1), lateral(2));
        assert!(lateral(1).signum() != lateral(2).signum(), "the leader fights in the centre");
        for i in [1, 2] {
            let order = orders[i].unwrap();
            assert_eq!(order.phase, Phase::Track);
            assert!(order.fire && order.aim == target, "wingman {i} engages the leader's target");
        }
        // Once the leader stops engaging, a wingman without its own sighting holds fire.
        let orders = step(&mut ai, &mut tanks, &blind(), 0.1);
        assert!(!orders[0].unwrap().fire);
        assert!([1, 2].iter().all(|&i| !orders[i].unwrap().fire));
    }

    #[test]
    fn a_platoon_that_loses_its_leader_holds_then_joins_the_other_leader_or_wanders() {
        let mut ai = WinterEncounter::new();
        let mut tanks = muster(&ai);
        step(&mut ai, &mut tanks, &blind(), 0.1);
        tanks[0] = None; // platoon A's leader is knocked out
        let orders = step(&mut ai, &mut tanks, &blind(), 0.1);
        for i in [1, 2] {
            assert!(matches!(ai.role(i), Role::Leaderless { .. }));
            assert_eq!(orders[i].unwrap().drive, 0.0, "survivors hold their ground");
        }
        for _ in 0..(REGROUP_S * 10.0) as usize + 5 { step(&mut ai, &mut tanks, &blind(), 0.1); }
        let mut places: Vec<_> = [1, 2].iter().map(|&i| match ai.role(i) {
            Role::Wingman { leader: 3, place } => place,
            other => panic!("tank {i} should have joined platoon B's leader, is {other:?}"),
        }).collect();
        places.sort();
        assert_eq!(places, vec![2, 3], "joiners take the places behind B's own wingman");
        tanks[3] = None; // and now B's leader too
        for _ in 0..(REGROUP_S * 10.0) as usize + 5 { step(&mut ai, &mut tanks, &blind(), 0.1); }
        for i in [1, 2, 4] {
            assert_eq!(ai.role(i), Role::Straggler, "with no leader left, tank {i} wanders");
        }
    }

    #[test]
    fn only_leaders_relay_contact_between_platoons_after_the_hand_off() {
        let mut ai = WinterEncounter::new();
        let mut tanks = muster(&ai);
        let target = Vec3::new(1850.0, 2.0, -40.0);
        let mut seen = blind();
        seen[0].visible = Some(target);
        for _ in 0..30 { step(&mut ai, &mut tanks, &seen, 0.1); }
        assert_eq!(ai.phase(3), Phase::Patrol, "a report takes the hand-off time to arrive");
        for _ in 0..40 { step(&mut ai, &mut tanks, &seen, 0.1); }
        assert_eq!(ai.phase(3), Phase::Search, "platoon B's leader hears platoon A's report");
        assert_eq!(ai.phase(4), Phase::Search, "and his wingman follows him");
        // A wingman with no leader has no radio and no leader's signals to read.
        tanks[3] = None;
        for _ in 0..20 { step(&mut ai, &mut tanks, &seen, 0.1); }
        assert!(matches!(ai.role(4), Role::Leaderless { .. }));
        assert_eq!(ai.phase(4), Phase::Patrol);
    }

    fn lone_leader_orders(ai: &mut WinterEncounter, pos: Vec3, heading: f32, observation: Observation, dt: f32) -> Order {
        let mut hulls = [None; SLOTS];
        hulls[0] = Some(Hull { pos, heading, speed: 0.0 });
        let mut observations = blind();
        observations[0] = observation;
        ai.orders(&hulls, &observations, dt, &|_, _| true, &|_| 0.0)[0].unwrap()
    }

    #[test]
    fn real_sound_starts_uncertain_investigation_without_authorizing_fire() {
        let mut ai = WinterEncounter::new();
        let origin = Vec3::new(1907.0,2.0,218.0);
        let p = Vec3::new(1770.0,2.0,-140.0);
        ai.hear(0,p,origin,900.0,true);
        let heard = ai.crews[0].heard.unwrap();
        assert!(heard.distance(origin) > 5.0 && heard.distance(origin) < 45.0);
        let o = lone_leader_orders(&mut ai,p,0.0,unknown(),0.05);
        assert_eq!(o.phase,Phase::Investigate); assert!(!o.fire);
        for _ in 0..40 { ai.tick(0.1); lone_leader_orders(&mut ai,p,0.0,unknown(),0.1); }
        assert_eq!(ai.crews[0].heard,Some(heard),"no hidden player pose follows the report");
        lone_leader_orders(&mut ai,p,0.0,unknown(),SOUND_MEMORY_S);
        assert_eq!(ai.phase(0),Phase::Patrol);
        ai.hear(0,p,origin,300.0,true);
        assert!(ai.crews[0].heard.is_none(),"distant obstructed pistol report is inaudible");
        // A wingman signals what it heard to its leader, who decides.
        ai.hear(1,p,origin,900.0,false);
        assert!(ai.crews[0].heard.is_some() && ai.crews[1].heard.is_none());
    }

    #[test]
    fn cover_prevents_detection_and_lost_contact_never_tracks_hidden_pose() {
        let mut ai = WinterEncounter::new();
        let mut stealth = stealth::Stealth::new(3);
        let p = Vec3::new(1850.0,2.0,0.0);
        let target = Vec3::new(1850.0,2.0,130.0);
        let wall = stealth::Occluder { min: Vec3::new(1840.0,0.0,60.0), max: Vec3::new(1860.0,5.0,63.0) };
        assert!(stealth::los_blocked(&[wall],p,target));
        for _ in 0..80 { stealth.tick(0.1,&[0.0;3],&[true;3],target, |_| "test T-34"); }
        assert!(!stealth.spotted[0]);
        // Remove the wall: ordinary accrual acquires the target; no debug engage/fix hook.
        for _ in 0..80 { stealth.tick(0.1,&[1.0,0.0,0.0],&[true;3],target, |_| "test T-34"); }
        assert!(stealth.spotted[0]);
        let o = lone_leader_orders(&mut ai,p,0.0,Observation { visible:Some(target),..unknown() },0.1);
        assert!(o.fire); assert_eq!(o.phase,Phase::Track);
        let hidden = Vec3::new(1999.0,2.0,300.0);
        stealth.tick(0.1,&[0.0;3],&[true;3],hidden, |_| "test T-34");
        assert_eq!(stealth.platoons[0].last_known,target);
        let o = lone_leader_orders(&mut ai,p,0.0,Observation { radio_fix:Some(stealth.platoons[0].last_known),..unknown() },0.1);
        assert!(!o.fire); assert_eq!(o.phase,Phase::Search);
        for _ in 0..260 { stealth.tick(0.1,&[0.0;3],&[true;3],hidden, |_| "test T-34"); lone_leader_orders(&mut ai,p,0.0,unknown(),0.1); }
        assert_eq!(ai.phase(0),Phase::Patrol);
        assert_eq!(stealth.platoon_state(0),stealth::AlertState::Unaware);
    }

    #[test]
    fn a_man_on_foot_is_held_at_standoff_and_fired_on_without_charging() {
        let mut ai = WinterEncounter::new();
        let target = Vec3::new(1880.0,2.0,165.0);
        for (range,expected) in [(60.0,-1),(120.0,0),(170.0,1)] {
            let p = target - Vec3::Z * range;
            let o = lone_leader_orders(&mut ai,p,0.0,Observation { visible:Some(target),..unknown() },0.1);
            assert!(o.fire); assert_eq!(if o.drive == 0.0 { 0 } else { o.drive.signum() as i32 },expected);
            assert!(o.drive.abs() <= 0.2);
        }
    }

    /// Fly the encounter against a stationary Tiger on open ground; every tank sees it unless
    /// `sight` says otherwise. Shots are counted the way the game does: a fire order with a loaded
    /// gun discharges and starts a 5.5 s reload. Returns the tanks and per-frame orders.
    fn duel(ai: &mut WinterEncounter, tanks: &mut [Option<Tank>], tiger: Vec3, tiger_heading: f32,
        seconds: f32, mut each: impl FnMut(f32, &[Option<Tank>], &[Option<Order>])) {
        let dt = 0.05;
        let mut shots = [0u32; SLOTS];
        let mut reload = [0.0f32; SLOTS];
        for frame in 0..(seconds / dt) as usize {
            let mut seen = blind();
            for i in 0..SLOTS {
                if tanks[i].is_some() {
                    seen[i] = Observation { visible: Some(tiger), on_foot: false, heading: Some(tiger_heading),
                        shots: shots[i], loaded: reload[i] <= 0.0, ..unknown() };
                }
            }
            ai.tick(dt);
            let orders = ai.orders(&hulls(tanks), &seen, dt, &|_, _| true, &|_| 0.0);
            for i in 0..SLOTS {
                reload[i] = (reload[i] - dt).max(0.0);
                if let (Some(tank), Some(order)) = (tanks[i].as_mut(), orders[i]) {
                    tank.update(order.drive, order.steer, 0.0, 0.0, dt, &flat());
                    if order.fire && reload[i] <= 0.0 { shots[i] += 1; reload[i] = 5.5; }
                }
            }
            each(frame as f32 * dt, tanks, &orders);
        }
    }

    #[test]
    fn against_a_tiger_the_platoon_fixes_and_flanks_instead_of_waiting_to_be_shot() {
        let mut ai = WinterEncounter::new();
        let mut tanks: Vec<Option<Tank>> = muster(&ai).into_iter().enumerate()
            .map(|(i, tank)| tank.filter(|_| i < 3)).collect();
        // The Tiger 380 m ahead of the platoon's leader, facing it square.
        let (leader_start, heading) = ai.start(0);
        let ahead = Vec2::new(heading.sin(), heading.cos());
        let tiger_xz = leader_start + ahead * 380.0;
        let tiger = Vec3::new(tiger_xz.x, 1.0, tiger_xz.y);
        let tiger_heading = heading + std::f32::consts::PI;
        let bearing = |t: &Tank| tank_doctrine::relative_bearing(tiger_heading, tiger_xz, xz(t.pos));
        let mut flank_reached = [None::<f32>; 3];
        let mut idle = [0.0f32; 3];
        let mut all_moving = 0.0f32;
        duel(&mut ai, &mut tanks, tiger, tiger_heading, 150.0, |t, tanks, orders| {
            let mut bounding = 0;
            for i in 0..3 {
                let tank = tanks[i].as_ref().unwrap();
                let margin = tank_doctrine::kill_margin_mm(&crate::ballistics::BR_350B, tiger_heading, tiger_xz, xz(tank.pos));
                if bearing(tank).abs() > 1.05 && margin > 0.0 && flank_reached[i].is_none() { flank_reached[i] = Some(t); }
                if tank.speed.abs() < 0.5 { idle[i] += 0.05; }
                if orders[i].is_some_and(|o| o.drive > 0.2) && tank.speed > 2.0 { bounding += 1; }
            }
            if bounding == 3 { all_moving += 0.05; }
        });
        println!("flank reached {flank_reached:?}, idle {idle:?}, all three moving {all_moving:.1} s");
        for i in [1, 2] {
            let t = flank_reached[i].unwrap_or_else(|| panic!("wingman {i} never reached a killing flank post: {flank_reached:?}"));
            assert!(t < 120.0, "wingman {i} took {t:.0} s to reach the flank");
        }
        let side = |i: usize| bearing(tanks[i].as_ref().unwrap()).signum();
        assert_ne!(side(1), side(2), "the wingmen take the Tiger's two flanks (a pincer)");
        assert!(bearing(tanks[0].as_ref().unwrap()).abs() > FRONT_ARC_RAD,
            "the leader leaves the Tiger's frontal arc for its fixing post");
        assert!(idle.iter().sum::<f32>() / (3.0 * 150.0) < 0.75, "the platoon does not sit still: {idle:?}");
        assert!(all_moving < 8.0, "one tank covers while the others bound: {all_moving:.1} s");
    }

    #[test]
    fn a_struck_tank_backs_out_of_the_line_of_fire_and_one_under_the_gun_moves() {
        let setup = || {
            let ai = WinterEncounter::new();
            let tanks: Vec<Option<Tank>> = muster(&ai).into_iter().enumerate().map(|(i, t)| t.filter(|_| i == 0)).collect();
            let (start, heading) = ai.start(0);
            let tiger_xz = start + Vec2::new(heading.sin(), heading.cos()) * 260.0;
            (ai, tanks, Vec3::new(tiger_xz.x, 1.0, tiger_xz.y), heading + std::f32::consts::PI)
        };
        let seen = |tiger: Vec3, th: f32, struck: bool, shots: u32, gun_on_me: bool| {
            let mut o = blind();
            o[0] = Observation { visible: Some(tiger), on_foot: false, heading: Some(th), struck, shots, gun_on_me, ..unknown() };
            o
        };
        // Square to the Tiger's front, with no killing shot from there: struck, it reverses.
        let (mut ai, mut tanks, tiger, th) = setup();
        for _ in 0..20 { step(&mut ai, &mut tanks, &seen(tiger, th, false, 0, false), 0.05); }
        let o = step(&mut ai, &mut tanks, &seen(tiger, th, true, 0, false), 0.05)[0].unwrap();
        assert!(o.drive < 0.0, "struck, it reverses: {o:?}");
        for _ in 0..20 {
            assert!(step(&mut ai, &mut tanks, &seen(tiger, th, false, 0, false), 0.05)[0].unwrap().drive < 0.0, "and keeps backing off");
        }
        for _ in 0..200 { step(&mut ai, &mut tanks, &seen(tiger, th, false, 0, false), 0.05); }
        assert!(step(&mut ai, &mut tanks, &seen(tiger, th, false, 0, false), 0.05)[0].unwrap().drive >= 0.0, "the withdrawal ends");
        // The Tiger's gun laid on it after it fired from its post: it pulls back rather than duel.
        let (mut ai, mut tanks, tiger, th) = setup();
        for _ in 0..100 { step(&mut ai, &mut tanks, &seen(tiger, th, false, 0, false), 0.05); }
        let moved = (0..100).any(|_| step(&mut ai, &mut tanks, &seen(tiger, th, false, 1, true), 0.05)[0].unwrap().drive < 0.0);
        assert!(moved, "under the Tiger's gun after its shot, the crew pulls back");
    }

    // ── The street layout, on a synthetic district: four 120 m blocks on a 300 m square, 20 m
    // streets between and round them (x, z in [0, 20], [140, 160] and [280, 300] are street).
    fn in_block(p: Vec2) -> bool {
        let block = |v: f32| (20.0..140.0).contains(&v) || (160.0..280.0).contains(&v);
        block(p.x) && block(p.y)
    }
    fn district() -> StreetNav {
        // The T-34 probe keeps a 4.46 m half-box clear: a cell centre that close to a block is not.
        StreetNav::build(Vec2::splat(-60.0), Vec2::splat(360.0), super::super::street_nav::NAV_CELL_M, |p| {
            [(-4.5f32, -4.5f32), (-4.5, 4.5), (4.5, -4.5), (4.5, 4.5), (0.0, 0.0)].iter().all(|&(x, z)| !in_block(p + Vec2::new(x, z)))
        })
    }
    fn street_encounter() -> WinterEncounter {
        WinterEncounter::in_streets([
            vec![Vec2::new(150.0, 10.0), Vec2::new(290.0, 10.0), Vec2::new(290.0, 150.0), Vec2::new(150.0, 150.0)],
            vec![Vec2::new(10.0, 150.0), Vec2::new(10.0, 290.0), Vec2::new(150.0, 290.0), Vec2::new(150.0, 150.0)],
        ], district())
    }
    fn step_seeing(ai: &mut WinterEncounter, tanks: &mut [Option<Tank>], observations: &[Observation; SLOTS],
        dt: f32, sees: SightTest) -> Vec<Option<Order>> {
        ai.tick(dt);
        let orders = ai.orders(&hulls(tanks), observations, dt, sees, &|_| 0.0);
        for i in 0..SLOTS {
            if let (Some(tank), Some(order)) = (tanks[i].as_mut(), orders[i]) {
                tank.update(order.drive, order.steer, 0.0, 0.0, dt, &flat());
            }
        }
        orders
    }

    #[test]
    fn street_patrols_walk_every_corner_along_the_streets_in_column() {
        let mut ai = street_encounter();
        let mut tanks = muster(&ai);
        let mut visited = [[false; 4]; 2];
        let mut worst_station = 0.0f32;
        let dt = 0.05;
        for frame in 0..40 * 60 * 20 {
            step_seeing(&mut ai, &mut tanks, &blind(), dt, &|_, _| false);
            for i in (0..SLOTS).filter(|&i| fielded(i)) {
                let at = xz(tanks[i].as_ref().unwrap().pos);
                assert!(!in_block(at), "tank {i} drove into a block at {at} (frame {frame})");
                match ai.role(i) {
                    Role::Leader => for (k, corner) in ai.route(platoon_of(i)).iter().enumerate() {
                        if at.distance(*corner) < 6.0 { visited[platoon_of(i)][k] = true; }
                    },
                    Role::Wingman { leader, place } if frame > 60 * 20 => {
                        let gap = distance(&tanks, i, leader) - SPACING_M * place as f32;
                        worst_station = worst_station.max(gap);
                    }
                    _ => {}
                }
            }
            if visited.iter().flatten().all(|&v| v) { println!("all corners walked after {:.0} s", frame as f32 * dt); break; }
        }
        assert!(visited.iter().flatten().all(|&v| v), "every corner of both beats: {visited:?}");
        assert!(worst_station < 45.0, "wingmen keep their place along the streets: {worst_station:.1} m");
        assert_eq!(ai.confirmed_sightings, 0);
    }

    #[test]
    fn in_the_streets_a_report_is_closed_on_along_them_until_the_area_is_in_view() {
        let mut ai = street_encounter();
        let mut tanks: Vec<Option<Tank>> = muster(&ai).into_iter().enumerate().map(|(i, t)| t.filter(|_| i == 0)).collect();
        // Heard far across the blocks; the area can be seen only from its own two streets.
        let fix = Vec3::new(290.0, 1.0, 290.0);
        let origin = fix;
        ai.hear(0, tanks[0].as_ref().unwrap().center(), origin, 900.0, false);
        let heard = ai.crews[0].heard.unwrap();
        let sees = |eye: Vec3, at: Vec2| (eye.x - at.x).abs() < 9.0 || (eye.z - at.y).abs() < 9.0;
        let mut halted_in_view = None;
        for frame in 0..8 * 60 * 20 {
            let orders = step_seeing(&mut ai, &mut tanks, &blind(), 0.05, &sees);
            let tank = tanks[0].as_ref().unwrap();
            assert!(!in_block(xz(tank.pos)), "the leader drove into a block at {} (frame {frame})", xz(tank.pos));
            assert_eq!(orders[0].unwrap().phase, Phase::Investigate);
            assert!(!orders[0].unwrap().fire, "a report is never a target");
            let eye = tank.center() + Vec3::Y * EYE_ABOVE_CENTRE_M;
            if sees(eye, xz(heard)) && xz(tank.pos).distance(xz(heard)) < STREET_VIEW_HALT_M && tank.speed.abs() < 0.05 {
                halted_in_view = Some(xz(tank.pos));
                break;
            }
        }
        let at = halted_in_view.expect("the leader reaches a street with the reported area in view and halts");
        assert!(at.distance(xz(heard)) > 40.0, "it halts outside the area, never through it: {at}");
    }

    #[test]
    fn in_the_streets_a_platoon_flanks_a_tiger_along_the_streets_without_entering_a_block() {
        let mut ai = street_encounter();
        let mut tanks: Vec<Option<Tank>> = muster(&ai).into_iter().enumerate().map(|(i, t)| t.filter(|_| i < 3)).collect();
        for _ in 0..30 * 20 { step_seeing(&mut ai, &mut tanks, &blind(), 0.05, &|_, _| false); }
        // The Tiger in the district's centre crossing, facing the platoon's leader.
        let tiger_xz = Vec2::new(150.0, 150.0);
        let to_leader = xz(tanks[0].as_ref().unwrap().pos) - tiger_xz;
        let tiger_heading = to_leader.x.atan2(to_leader.y);
        let tiger = Vec3::new(tiger_xz.x, 1.0, tiger_xz.y);
        let mut travelled = [0.0f32; 3];
        let mut last: Vec<Vec2> = (0..3).map(|i| xz(tanks[i].as_ref().unwrap().pos)).collect();
        duel(&mut ai, &mut tanks, tiger, tiger_heading, 120.0, |_, tanks, _| {
            for i in 0..3 {
                let at = xz(tanks[i].as_ref().unwrap().pos);
                assert!(!in_block(at), "tank {i} drove into a block at {at}");
                travelled[i] += at.distance(last[i]);
                last[i] = at;
            }
        });
        println!("travelled {travelled:?}");
        assert!(travelled[1] > 60.0 && travelled[2] > 60.0, "the wingmen manoeuvre: {travelled:?}");
        let off_front = (1..3).filter(|&i| tank_doctrine::relative_bearing(tiger_heading, tiger_xz,
            xz(tanks[i].as_ref().unwrap().pos)).abs() > FRONT_ARC_RAD).count();
        assert!(off_front >= 1, "a wingman works out of the Tiger's frontal arc along the streets");
    }
}
