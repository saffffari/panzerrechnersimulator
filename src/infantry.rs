//! ABSITZEN enemy infantry — Russian soldiers that patrol set routes with a vision cone and a
//! detection meter, turn to face a contact (the diegetic tell), raise the alarm when fully detected,
//! and fire (lethal both ways). Pure game-logic (no wgpu), same discipline as `foot.rs`: `game.rs`
//! grounds each soldier on the heightfield and drives its two mesh slots (body + rifle) per frame.
//!
//! Placeholder capsule/box meshes now; real PSX rigs drop into the same `body_slot`/`rifle_slot`
//! later — the seam is a mesh slot + a per-frame transform.

use glam::{Vec2, Vec3};

use crate::game::ray_terrain;
use crate::terrain::Heightfield;

/// Eye height of a standing soldier (m).
pub const EYE_H: f32 = 1.6;

const VIEW_RANGE: f32 = 45.0; // m — clean standing sight range (scaled by player visibility)
const HALF_FOV_COS: f32 = 0.6428; // cos(50°) — half the vision cone
const GAIN_RATE: f32 = 0.9; // detection accrual scalar (full standing LOS ≈ 1.6 s)
const DECAY_RATE: f32 = 0.35; // detection bleed-off with no contact (~1 s behind cover resets Suspicious)
const HEAR_RATE: f32 = 0.28; // continuous "hears you moving nearby" trickle
const PATROL_SPEED: f32 = 1.4; // m/s walking the route
const INVEST_SPEED: f32 = 1.1; // m/s easing toward a suspicious point
const HUNT_SPEED: f32 = 1.7; // m/s advancing on the last-seen point while alarmed
const NEAR_RANGE: f32 = 8.0; // m — inside this, range falloff is 1.0
const FAR_VIS: f32 = 0.15; // range falloff at the edge of VIEW_RANGE
const AIM_WINDUP: f32 = 0.5; // s — shoulders the rifle before the first shot (the 2nd tell)
const INF_RELOAD: f32 = 1.6; // s — between shots
const LOUD_NOISE: f32 = 60.0; // event strength above this = a gunshot
const GUNSHOT_STARTLE: f32 = 50.0; // m — inside this a gunshot forces Alarmed (wider radius: drawn)

const SUSPICIOUS_ON: f32 = 0.4;
const ALARMED_ON: f32 = 1.0;
const UNAWARE_OFF: f32 = 0.15;

/// A discrete loud event (running footfalls, and especially a Luger shot) that soldiers sample.
/// Lives in the `Game.noise` ring; decays by `ttl`.
#[derive(Clone, Copy, Default)]
pub struct NoiseEvent {
    pub pos: Vec3,
    pub strength: f32, // audible radius in metres
    pub ttl: f32,
}

/// What a soldier will do this frame, from most to least aware.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Alert {
    Unaware,
    Suspicious,
    Alarmed,
}

impl Alert {
    pub fn name(self) -> &'static str {
        match self {
            Alert::Unaware => "UNAWARE",
            Alert::Suspicious => "SUSPICIOUS",
            Alert::Alarmed => "ALARMED",
        }
    }
}

/// One in-flight infantry shot, resolved in `game.rs` against the foot player.
pub struct InfantryShot {
    pub from: Vec3,
    pub dir: Vec3,
    pub range: f32,
}

pub struct Soldier {
    pub pos: Vec3,
    pub yaw: f32,         // forward = (sin, 0, -cos), same convention as the foot player
    pub route: [Vec3; 4], // patrol waypoints (a repeated waypoint = an idle scanning sentry)
    pub leg: usize,
    pub alert: Alert,
    pub detect: f32,  // 0..1 detection meter (NO on-screen bar — diegetic only)
    pub invest: Vec3, // last-seen / last-heard point
    pub has_invest: bool,
    pub alive: bool,
    /// Luger-hit stagger: while > 0 the man is down on a knee — doesn't move, doesn't shoot.
    /// He ALWAYS gets back up (the Luger kills nobody; the game never says so — by design).
    pub flinch_t: f32,
    pub fire_cd: f32,
    pub aim_windup: f32,
    pub tick: u32,          // per-soldier deterministic hash counter
    pub alarm_signal: bool, // set the frame it enters Alarmed; game.rs consumes it to cascade
    pub body_slot: usize,
    pub rifle_slot: usize,
}

impl Soldier {
    pub fn new(
        pos: Vec3,
        yaw: f32,
        route: [Vec3; 4],
        seed: u32,
        body_slot: usize,
        rifle_slot: usize,
    ) -> Self {
        Self {
            pos,
            yaw,
            route,
            leg: 0,
            alert: Alert::Unaware,
            detect: 0.0,
            invest: pos,
            has_invest: false,
            alive: true,
            flinch_t: 0.0,
            fire_cd: 0.0,
            aim_windup: 0.0,
            tick: seed.wrapping_mul(2654435761),
            alarm_signal: false,
            body_slot,
            rifle_slot,
        }
    }

    /// Ground forward vector.
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    /// Externally forced awareness (alarm cascade from a neighbour or a gunshot).
    pub fn raise_alarm(&mut self, at: Vec3) {
        self.detect = self.detect.max(0.75);
        self.invest = at;
        self.has_invest = true;
        if self.alert == Alert::Unaware {
            self.alert = Alert::Suspicious;
        }
    }

    /// Advance one frame. Returns a shot if it fires this frame. `prof` = the player's
    /// `(noise_radius, visibility)`; `cover` = coarse LOS-blocking cylinders `(centre_xz, radius)`.
    pub fn update(
        &mut self,
        player_eye: Vec3,
        prof: (f32, f32),
        noise: &[NoiseEvent],
        cover: &[(Vec2, f32)],
        hf: &Heightfield,
        dt: f32,
    ) -> Option<InfantryShot> {
        if !self.alive {
            return None;
        }
        self.tick = self.tick.wrapping_add(1);
        if self.fire_cd > 0.0 {
            self.fire_cd -= dt;
        }
        self.alarm_signal = false;
        // Staggered by a pistol hit: down for a beat — no movement, no aim, no shot — then back
        // up like nothing happened. (A pistol round through winter wool kills nobody in this war.)
        if self.flinch_t > 0.0 {
            self.flinch_t -= dt;
            self.aim_windup = 0.0;
            return None;
        }

        let (noise_radius, visibility) = prof;
        let eye = self.pos + Vec3::Y * EYE_H;

        // ── Vision ────────────────────────────────────────────────────────────
        let to = player_eye - eye;
        let dist = to.length();
        let eff_range = VIEW_RANGE * visibility;
        let mut gain = 0.0;
        let mut saw = false;
        if dist > 0.05 && dist < eff_range {
            let dir = to / dist;
            let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
            let cone = self.forward().dot(flat);
            if cone > HALF_FOV_COS && !segment_blocked(eye, player_eye, hf, cover) {
                let k_cone = ((cone - HALF_FOV_COS) / (1.0 - HALF_FOV_COS)).clamp(0.0, 1.0);
                let k_range = if dist <= NEAR_RANGE {
                    1.0
                } else {
                    let t = ((dist - NEAR_RANGE) / (eff_range - NEAR_RANGE)).clamp(0.0, 1.0);
                    1.0 + (FAR_VIS - 1.0) * t
                };
                gain = k_cone * k_range * visibility;
                saw = gain > 0.0;
            }
        }

        // ── Hearing: continuous ambient loudness + discrete loud events ─────────
        let mut heard = false;
        let mut forced_alarm = false;
        if noise_radius > 0.0 {
            let d = Vec2::new(player_eye.x - self.pos.x, player_eye.z - self.pos.z).length();
            if d < noise_radius {
                self.detect += HEAR_RATE * dt;
                self.invest = Vec3::new(player_eye.x, self.pos.y, player_eye.z);
                self.has_invest = true;
                heard = true;
            }
        }
        for ev in noise {
            if ev.ttl <= 0.0 || ev.strength <= 0.0 {
                continue;
            }
            let d = Vec2::new(ev.pos.x - self.pos.x, ev.pos.z - self.pos.z).length();
            if d < ev.strength {
                let gain = (ev.strength / (d + 4.0)) * 1.2 * dt;
                // A gunshot force-alarms only the NEAR field; distant patrols within the wider
                // audible radius are DRAWN, not committed: their meter is ceilinged just under
                // Alarmed so sound alone can't flip the whole map (a Luger event is strength 120
                // over 3 s — uncapped it saturated everyone within earshot to Alarmed).
                if ev.strength >= LOUD_NOISE && d >= GUNSHOT_STARTLE {
                    self.detect = (self.detect + gain).min((ALARMED_ON - 0.1).max(self.detect));
                } else {
                    self.detect += gain;
                    if ev.strength >= LOUD_NOISE {
                        forced_alarm = true;
                    }
                }
                self.invest = ev.pos;
                self.has_invest = true;
                heard = true;
            }
        }

        if saw {
            self.detect += gain * GAIN_RATE * dt;
            self.invest = Vec3::new(player_eye.x, self.pos.y, player_eye.z);
            self.has_invest = true;
        } else if !heard {
            self.detect -= DECAY_RATE * dt;
        }
        if forced_alarm {
            self.detect = self.detect.max(ALARMED_ON);
        }
        self.detect = self.detect.clamp(0.0, 1.0);

        // ── State machine + behaviour ───────────────────────────────────────────
        let mut shot = None;
        match self.alert {
            Alert::Unaware => {
                self.patrol(dt, hf);
                if self.detect >= SUSPICIOUS_ON {
                    self.alert = Alert::Suspicious;
                }
            }
            Alert::Suspicious => {
                // Halt patrol; turn to face the contact (THE tell) and slow-walk toward it.
                let tgt = if self.has_invest {
                    self.invest
                } else {
                    player_eye
                };
                self.face(tgt, 2.2, dt);
                self.approach(tgt, INVEST_SPEED, dt, hf);
                if self.detect >= ALARMED_ON {
                    self.alert = Alert::Alarmed;
                    self.aim_windup = AIM_WINDUP;
                    self.alarm_signal = true;
                } else if self.detect < UNAWARE_OFF {
                    self.alert = Alert::Unaware;
                }
            }
            Alert::Alarmed => {
                let tgt = if saw {
                    player_eye
                } else if self.has_invest {
                    self.invest
                } else {
                    player_eye
                };
                self.face(tgt, 3.2, dt); // swing onto the target fast
                if self.aim_windup > 0.0 {
                    self.aim_windup -= dt;
                } else if saw && self.fire_cd <= 0.0 {
                    shot = Some(self.fire(player_eye));
                    self.fire_cd = INF_RELOAD;
                }
                if !saw {
                    // Lost sight: hunt toward the last-seen point, then relax.
                    self.approach(self.invest, HUNT_SPEED, dt, hf);
                    if self.detect < 0.6 {
                        self.alert = Alert::Suspicious;
                    }
                }
            }
        }

        shot
    }

    fn fire(&mut self, player_eye: Vec3) -> InfantryShot {
        let from = self.pos + Vec3::Y * EYE_H;
        let to = player_eye - from;
        let range = to.length().max(0.01);
        let base = to / range;
        // Deterministic dispersion — generous at range so a fleeing player outruns one shooter.
        let j = |n: u32| ((((n.wrapping_mul(2654435761)) >> 8) & 0xffff) as f32 / 32768.0) - 1.0;
        let disp = 0.02 + 0.02 * (range / VIEW_RANGE).min(1.0);
        // Jitter in a basis PERPENDICULAR to the shot — world-axis jitter collapsed the spread
        // to near-zero when firing along ±X (the X component ran down the barrel).
        let mut side = base.cross(Vec3::Y);
        if side.length_squared() < 1e-6 {
            side = Vec3::X; // firing straight up/down — any horizontal works
        }
        let side = side.normalize_or_zero();
        let vert = side.cross(base);
        let dir =
            (base + side * (j(self.tick) * disp) + vert * (j(self.tick.wrapping_add(53)) * disp))
                .normalize_or_zero();
        InfantryShot {
            from,
            dir,
            range: range + 2.0,
        }
    }

    /// Ease `yaw` toward facing `target` at up to `rate` rad/s.
    fn face(&mut self, target: Vec3, rate: f32, dt: f32) {
        let dx = target.x - self.pos.x;
        let dz = target.z - self.pos.z;
        if dx * dx + dz * dz > 1e-4 {
            let want = dx.atan2(-dz); // forward = (sin, 0, -cos)
            let mut dh = want - self.yaw;
            while dh > std::f32::consts::PI {
                dh -= std::f32::consts::TAU;
            }
            while dh < -std::f32::consts::PI {
                dh += std::f32::consts::TAU;
            }
            self.yaw += dh.clamp(-rate * dt, rate * dt);
        }
    }

    /// Walk toward `target` at `speed`, grounded on the heightfield.
    fn approach(&mut self, target: Vec3, speed: f32, dt: f32, hf: &Heightfield) {
        let to = Vec3::new(target.x - self.pos.x, 0.0, target.z - self.pos.z);
        let d = to.length();
        if d > 0.4 {
            self.pos += (to / d) * speed * dt;
        }
        self.pos.y = hf.height_at(self.pos.x, self.pos.z);
    }

    /// Walk the patrol route; a route whose waypoints coincide becomes an idle scanning sentry.
    fn patrol(&mut self, dt: f32, hf: &Heightfield) {
        let wp = self.route[self.leg];
        let to = Vec3::new(wp.x - self.pos.x, 0.0, wp.z - self.pos.z);
        let d = to.length();
        if d < 1.0 {
            self.leg = (self.leg + 1) % self.route.len();
            // Idle scan (a sentry that never travels far still sweeps its gaze).
            // Wrapped phase — the raw tick starts at a hashed ~1e9 where f32 ULP >> 0.02 and
            // the sine froze; modulo keeps the argument small (6283 ticks = 20 clean periods).
            self.yaw += 0.6 * ((self.tick % 6283) as f32 * 0.02).sin() * dt;
        } else {
            self.face(wp, 2.0, dt);
            self.pos += (to / d) * PATROL_SPEED * dt;
        }
        self.pos.y = hf.height_at(self.pos.x, self.pos.z);
    }
}

/// Is the straight segment `a→b` blocked by terrain or any cover cylinder? The v1 LOS model:
/// buildings aren't in the heightfield, so coarse cylinders over the wall clusters stand in for
/// them (§6) — breaking behind a cluster actually hides you.
pub(crate) fn segment_blocked(a: Vec3, b: Vec3, hf: &Heightfield, cover: &[(Vec2, f32)]) -> bool {
    let to = b - a;
    let d = to.length();
    if d < 1e-3 {
        return false;
    }
    let dir = to / d;
    if ray_terrain(a, dir, hf, d).is_some() {
        return true;
    }
    let a2 = Vec2::new(a.x, a.z);
    let b2 = Vec2::new(b.x, b.z);
    for (c, r) in cover {
        if seg_circle_hit(a2, b2, *c, *r) {
            return true;
        }
    }
    false
}

/// Does the 2D segment `a→b` pass within `r` of `c`? (closest-point-on-segment distance test).
fn seg_circle_hit(a: Vec2, b: Vec2, c: Vec2, r: f32) -> bool {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 1e-6 {
        ((c - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t - c).length_squared() < r * r
}
