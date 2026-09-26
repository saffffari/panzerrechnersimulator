//! Local winter patrol orders. Perception remains in `stealth`; this layer only consumes
//! confirmed observations, radio fixes and uncertain gunshot reports. Hidden player poses are
//! deliberately absent from its input. Values are encounter tuning, not historical claims.
use glam::{Vec2, Vec3};

pub(super) const CONTACTS: usize = 3;
const SOUND_MEMORY_S: f32 = 150.0;
const PATROL: [[Vec2; 4]; CONTACTS] = [
    [Vec2::new(1770.0, -140.0), Vec2::new(1815.0, -95.0),
     Vec2::new(1840.0, -75.0), Vec2::new(1760.0, -90.0)],
    [Vec2::new(2065.0, -150.0), Vec2::new(2040.0, -60.0),
     Vec2::new(2100.0, -35.0), Vec2::new(2130.0, -125.0)],
    [Vec2::new(1870.0, -360.0), Vec2::new(1950.0, -345.0),
     Vec2::new(1950.0, -255.0), Vec2::new(1870.0, -265.0)],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase { Patrol, Investigate, Search, Track }
impl Phase {
    pub fn name(self) -> &'static str {
        match self { Self::Patrol => "patrol", Self::Investigate => "investigate_sound",
            Self::Search => "search_last_seen", Self::Track => "visual_contact" }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Observation {
    /// Some only when the shipping meter AND this frame's actual visibility qualify.
    pub visible: Option<Vec3>,
    /// Existing platoon memory. Never a caller-supplied current hidden position.
    pub radio_fix: Option<Vec3>,
    pub on_foot: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Order {
    pub phase: Phase,
    pub drive: f32,
    pub steer: f32,
    pub aim: Vec3,
    pub fire: bool,
}

#[derive(Clone, Copy, Debug)]
struct Patrol {
    waypoint: usize,
    dwell: f32,
    heard: Option<Vec3>,
    heard_t: f32,
    phase: Phase,
    visible_now: bool,
    observed_fix: Option<Vec3>,
    observed_t: f32,
}

pub(super) struct WinterEncounter {
    routes: [[Vec2; 4]; CONTACTS],
    crews: [Patrol; CONTACTS],
    pub elapsed: f32,
    pub storm: f32,
    pub sounds_heard: u32,
    pub confirmed_sightings: u32,
}

pub(super) fn visibility_radius(existing: f32, foot: bool, storm: f32) -> f32 {
    // Snow/fog silhouettes disappear before the fair-weather stealth radius. Retain its
    // crouch, engine, night and movement reductions by taking the smaller envelope.
    existing.min(if foot { 205.0 - storm.clamp(0.0, 1.0) * 50.0 }
        else { 360.0 - storm.clamp(0.0, 1.0) * 120.0 })
}

impl WinterEncounter {
    pub fn new() -> Self {
        Self { routes: PATROL, crews: std::array::from_fn(|i| Patrol {
            waypoint: 1, dwell: 8.0 + i as f32 * 8.0, heard: None, heard_t: 0.0,
            phase: Phase::Patrol, visible_now: false, observed_fix: None, observed_t: 0.0,
        }), elapsed: 0.0, storm: 0.66, sounds_heard: 0, confirmed_sightings: 0 }
    }
    pub fn route(&self, i: usize) -> [Vec2; 4] { self.routes[i] }
    pub fn set_route(&mut self, i: usize, route: [Vec2; 4]) { self.routes[i] = route; }
    pub fn visible(&self, i: usize) -> bool { self.crews.get(i).is_some_and(|p| p.visible_now) }
    pub fn set_visible(&mut self, i: usize, value: bool) {
        if let Some(crew) = self.crews.get_mut(i) { crew.visible_now = value; }
    }
    pub fn phase(&self, i: usize) -> Phase { self.crews[i].phase }
    pub fn tick(&mut self, dt: f32) { self.elapsed += dt.max(0.0); }

    /// A real hostile muzzle report, delivered once at discharge. Obstruction attenuates
    /// hearing; sound supplies an area to investigate, never a line to fire along.
    pub fn hear(&mut self, i: usize, listener: Vec3, origin: Vec3, range: f32, obstructed: bool) {
        let Some(crew) = self.crews.get_mut(i) else { return; };
        let range = range * if obstructed { 0.62 } else { 1.0 };
        if !origin.is_finite() || listener.distance(origin) > range { return; }
        let uncertainty = [Vec2::new(11.0, -13.0), Vec2::new(-15.0, 7.0), Vec2::new(5.0, 17.0)][i];
        crew.heard = Some(Vec3::new((origin.x / 32.0).round() * 32.0 + uncertainty.x,
            (origin.y / 4.0).round() * 4.0, (origin.z / 32.0).round() * 32.0 + uncertainty.y));
        crew.heard_t = SOUND_MEMORY_S;
        self.sounds_heard = self.sounds_heard.saturating_add(1);
    }

    pub fn order(&mut self, i: usize, pos: Vec3, heading: f32, observation: Observation, dt: f32) -> Order {
        let c = &mut self.crews[i];
        let dt = dt.max(0.0);
        c.heard_t = (c.heard_t - dt).max(0.0);
        c.observed_t = (c.observed_t - dt).max(0.0);
        if c.heard_t == 0.0 { c.heard = None; }
        if c.observed_t == 0.0 { c.observed_fix = None; }
        if let Some(target) = observation.visible {
            if c.phase != Phase::Track { self.confirmed_sightings += 1; }
            c.observed_fix = Some(target);
            c.observed_t = super::stealth::SEARCH_TIMEOUT;
        }
        // A radio report uses the existing five-second handoff; a spotter can remember its
        // own observation immediately while that handoff is still pending.
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
            // using a hidden player's heading. Each crew takes a separate flank of the area.
            let radial = xz(pos - fix).normalize_or_zero();
            let tangent = Vec2::new(-radial.y, radial.x) * [-28.0, 28.0, 0.0][i];
            let goal = xz(fix) + radial * (112.0 + i as f32 * 12.0) + tangent;
            let (drive, steer) = if range < 105.0 { (if error.abs() < 0.8 { -0.12 } else { 0.0 }, (error * 1.8).clamp(-0.45, 0.45)) }
                else { navigate(xz(pos), heading, goal, 0.25, 12.0) };
            let sweep = (self.elapsed * 0.16 + i as f32 * 1.7).sin() * 24.0;
            (phase, fix + Vec3::new(radial.y * sweep, 0.0, -radial.x * sweep), drive, steer)
        } else {
            let goal = self.routes[i][c.waypoint];
            if xz(pos).distance(goal) < 6.0 {
                c.waypoint = (c.waypoint + 1) % 4;
                c.dwell = 10.0 + i as f32 * 4.0;
            }
            c.dwell = (c.dwell - dt).max(0.0);
            let (drive, steer) = if c.dwell > 0.0 { (0.0, 0.0) }
                else { navigate(xz(pos), heading, self.routes[i][c.waypoint], 0.14 - i as f32 * 0.012, 5.0) };
            let sweep = heading + (self.elapsed * 0.12 + i as f32 * 1.4).sin() * 0.9;
            (Phase::Patrol, pos + Vec3::new(sweep.sin(), 0.0, sweep.cos()) * 140.0, drive, steer)
        };
        c.phase = phase;
        // Patrol/contact area stays local. A retreating player cannot drag this encounter
        // across the act; observation can continue but movement returns toward the route.
        let home = self.routes[i][0];
        let (drive, steer) = if xz(pos).distance(home) > 650.0 {
            navigate(xz(pos), heading, home, 0.22, 8.0)
        } else { (drive, steer) };
        Order { phase, drive, steer, aim, fire: observation.visible.is_some() }
    }
}

fn xz(v: Vec3) -> Vec2 { Vec2::new(v.x, v.z) }
fn wrap(a: f32) -> f32 { (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI }
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
    fn unknown() -> Observation { Observation { visible: None, radio_fix: None, on_foot: true } }

    #[test]
    fn quiet_routes_allow_two_minutes_at_house_and_spawn_then_remain_bounded_for_ten() {
        let mut ai = WinterEncounter::new();
        let hf = crate::terrain::Heightfield { res: 2, spacing: 8000.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0 };
        let mut tanks: Vec<_> = (0..CONTACTS).map(|i| { let p = ai.route(i)[0];
            crate::tank::Tank::new(p.x,p.y,0.0,&hf,crate::tank::T34_PARTS) }).collect();
        let start: Vec<_> = tanks.iter().map(|t| t.pos).collect();
        let mut travelled = [0.0f32; CONTACTS];
        for frame in 0..600 * 20 {
            ai.tick(0.05);
            for i in 0..CONTACTS {
                if frame < 120 * 20 {
                    for foot in [Vec3::new(1850.0,1.1,250.0),Vec3::new(1880.0,1.1,165.0)] {
                        // Worst case: unobstructed, full front arc, fair-weather moving man.
                        assert_eq!(stealth::accrual_rate(1.0, tanks[i].pos.distance(foot),
                            visibility_radius(800.0 * 0.34,true,0.0),1.3),0.0);
                    }
                }
                let order = ai.order(i,tanks[i].pos,tanks[i].heading,unknown(),0.05);
                assert_eq!(order.phase,Phase::Patrol); assert!(!order.fire);
                tanks[i].update(order.drive,order.steer,0.0,0.0,0.05,&hf);
                travelled[i] = travelled[i].max(tanks[i].pos.distance(start[i]));
                assert!(tanks[i].pos.distance(start[i]) < 180.0);
            }
        }
        assert!(travelled.iter().all(|&d| d > 40.0), "patrols must move, not merely remain inert: {travelled:?}");
        assert_eq!(ai.confirmed_sightings,0); assert_eq!(ai.sounds_heard,0);
    }

    #[test]
    fn real_sound_starts_uncertain_investigation_without_authorizing_fire() {
        let mut ai = WinterEncounter::new();
        let origin = Vec3::new(1907.0,2.0,218.0);
        let p = Vec3::new(1770.0,2.0,-140.0);
        ai.hear(0,p,origin,900.0,true);
        let heard = ai.crews[0].heard.unwrap();
        assert!(heard.distance(origin) > 5.0 && heard.distance(origin) < 45.0);
        let o = ai.order(0,p,0.0,unknown(),0.05);
        assert_eq!(o.phase,Phase::Investigate); assert!(!o.fire);
        for _ in 0..40 { ai.tick(0.1); ai.order(0,p,0.0,unknown(),0.1); }
        assert_eq!(ai.crews[0].heard,Some(heard),"no hidden player pose follows the report");
        ai.order(0,p,0.0,unknown(),SOUND_MEMORY_S);
        assert_eq!(ai.phase(0),Phase::Patrol);
        ai.hear(0,p,origin,300.0,true);
        assert!(ai.crews[0].heard.is_none(),"distant obstructed pistol report is inaudible");
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
        let o = ai.order(0,p,0.0,Observation { visible:Some(target),..unknown() },0.1);
        assert!(o.fire); assert_eq!(o.phase,Phase::Track);
        let hidden = Vec3::new(1999.0,2.0,300.0);
        stealth.tick(0.1,&[0.0;3],&[true;3],hidden, |_| "test T-34");
        assert_eq!(stealth.platoons[0].last_known,target);
        let o = ai.order(0,p,0.0,Observation { radio_fix:Some(stealth.platoons[0].last_known),..unknown() },0.1);
        assert!(!o.fire); assert_eq!(o.phase,Phase::Search);
        for _ in 0..260 { stealth.tick(0.1,&[0.0;3],&[true;3],hidden, |_| "test T-34"); ai.order(0,p,0.0,unknown(),0.1); }
        assert_eq!(ai.phase(0),Phase::Patrol);
        assert_eq!(stealth.platoon_state(0),stealth::AlertState::Unaware);
    }

    #[test]
    fn confirmed_contacts_hold_standoff_and_can_return_fire_without_charging() {
        let mut ai = WinterEncounter::new();
        let target = Vec3::new(1880.0,2.0,165.0);
        for (range,expected) in [(80.0,-1),(200.0,0),(300.0,1)] {
            let p = target - Vec3::Z * range;
            let o = ai.order(0,p,0.0,Observation { visible:Some(target),on_foot:false,..unknown() },0.1);
            assert!(o.fire); assert_eq!(if o.drive == 0.0 { 0 } else { o.drive.signum() as i32 },expected);
            assert!(o.drive.abs() <= 0.2);
        }
        let o = ai.order(0,target-Vec3::Z*120.0,0.0,Observation { visible:Some(target),..unknown() },0.1);
        assert_eq!(o.drive,0.0); assert!(o.fire);
    }
}
