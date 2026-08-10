//! ABSITZEN ("dismount") — the on-foot dismounted soldier: a first-person controller, supply
//! pickups, and Luger-pistol FX state. Pure game-logic (no wgpu): the soldier walks the terrain
//! heightfield, mouse-looks by accumulating yaw/pitch directly (never through `set_forward`, which
//! can't accumulate and gimbal-locks at ±90°), and the camera + held-weapon transforms are driven
//! from this state each frame in `game.rs`.

use glam::Vec3;

use crate::tank::Tank;
use crate::terrain::Heightfield;

/// Eye height above the feet (m) — a standing soldier.
pub const EYE: f32 = 1.7;
/// Eye height when crouched — the camera physically drops ~0.65 m, the diegetic "hidden" tell.
pub const EYE_CROUCH: f32 = 1.05;
/// Walk speed (m/s).
pub const WALK: f32 = 2.6;
/// Run speed (m/s, Shift held).
pub const RUN: f32 = 5.5;
/// Crouch-walk speed (m/s) — crouch caps movement and forbids running.
pub const CROUCH_WALK: f32 = 1.3;
/// Starting hit points on foot (~3 rifle rounds of 34 puts you down).
pub const FOOT_HP: f32 = 100.0;
/// Jump take-off speed (m/s) and gravity — a modest hop (clear a low wall, drop off the hull).
pub const JUMP_V: f32 = 4.2;
pub const GRAVITY: f32 = 14.0;
/// Mouse look sensitivity — matches the commander head-look (game.rs).
pub const SENS: f32 = 0.0022;

/// The supply classes a scavenger picks up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickupKind {
    Ammo,
    Fuel,
    Medical,
    /// The treasure — gated behind the deep patrols in the city ring.
    Cigarettes,
}

/// Typed supply tally — the scavenging readout (no HUD text renderer exists; a `println!` at the
/// collect site is the verification surface).
#[derive(Default, Clone, Copy)]
pub struct Supplies {
    pub ammo: u32,
    pub fuel: u32,
    pub medical: u32,
    pub cigarettes: u32,
}

impl Supplies {
    pub fn add(&mut self, k: PickupKind) {
        match k {
            PickupKind::Ammo => self.ammo += 1,
            PickupKind::Fuel => self.fuel += 1,
            PickupKind::Medical => self.medical += 1,
            PickupKind::Cigarettes => self.cigarettes += 1,
        }
    }

    /// Fold `other` into this tally (exfil banking).
    pub fn absorb(&mut self, other: &Supplies) {
        self.ammo += other.ammo;
        self.fuel += other.fuel;
        self.medical += other.medical;
        self.cigarettes += other.cigarettes;
    }

    pub fn total(&self) -> u32 {
        self.ammo + self.fuel + self.medical + self.cigarettes
    }
}

/// A world-placed collectible. `slot` is its pre-allocated renderer mesh index (the scene is fixed
/// at `Renderer::new`, so collecting just hides the slot). Positioned once via `set_model`.
pub struct Pickup {
    pub pos: Vec3,
    pub slot: usize,
    pub kind: PickupKind,
    pub collected: bool,
}

/// The dismounted player. Feet track the terrain every frame (no gravity/jump for the slice).
pub struct FootSoldier {
    pub pos: Vec3, // ground contact (feet)
    pub yaw: f32,
    pub pitch: f32,
    pub aiming: bool,    // RMB — raise the Luger (narrow FOV + reticle)
    pub flash_t: f32,    // Luger muzzle-flash timer
    pub cooldown: f32,   // pistol rate-limit
    pub prev_fire: bool, // rising-edge latch for LMB
    pub supplies: Supplies,
    pub ammo: u32, // pistol rounds IN RESERVE (loose rounds; the magazine is separate)
    pub mag: u32,  // rounds in the seated magazine (P08: 8)
    pub reload_t: f32, // >0 = mid magazine change (R); firing locked out until it seats
    pub recoil: f32, // 0..1 muzzle-flip kick, decays fast (drives camera + pistol pose)
    pub speed_frac: f32, // 0..1 of full sprint this frame — feeds tank crews' detection of the man
    pub vel_y: f32, // vertical velocity (jump/gravity); 0 = grounded
    pub airborne: bool, // true between a jump and the next landing (ground snap suspended)
    pub crouch_t: f32, // 0 = stand, 1 = crouch (smoothed so the camera glides, never snaps)
    pub hp: f32,   // health on foot; 0 = KIA (drop the unbanked haul, respawn at the Tiger)
}

/// P08 magazine capacity.
pub const MAG_CAP: u32 = 8;
/// Magazine change time (drop, seat, rack) — deliberately unhurried; this is a last-resort weapon.
pub const RELOAD_S: f32 = 1.6;

impl Default for FootSoldier {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            aiming: false,
            flash_t: 0.0,
            cooldown: 0.0,
            prev_fire: false,
            supplies: Supplies::default(),
            ammo: 16,
            mag: MAG_CAP,
            reload_t: 0.0,
            recoil: 0.0,
            speed_frac: 0.0,
            vel_y: 0.0,
            airborne: false,
            crouch_t: 0.0,
            hp: FOOT_HP,
        }
    }
}

impl FootSoldier {
    /// Place the soldier ~3.5 m to the side of the parked tank, regrounded on the terrain, facing
    /// back at the hull.
    pub fn spawn_beside(&mut self, tank: &Tank, hf: &Heightfield) {
        let p = tank.pos + tank.right() * 3.5;
        self.pos = Vec3::new(p.x, hf.height_at(p.x, p.z), p.z);
        // Face back toward the hull. Foot forward is camera-style (sin y, 0, -cos y) while the
        // tank's is (sin h, 0, +cos h) — with the spawn on hull-RIGHT, facing the tank is a
        // quarter-turn, not the half-turn the old +PI (which left you parallel to the hull) gave.
        self.yaw = tank.heading + std::f32::consts::FRAC_PI_2;
        self.pitch = 0.0;
    }

    /// Advance one frame. `fwd` = W−S, `strafe` = D−A (true strafe). Mouselook writes yaw/pitch
    /// directly from accumulated mouse deltas.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        fwd: f32,
        strafe: f32,
        run: bool,
        crouch: bool,
        jump: bool,
        look_dx: f32,
        look_dy: f32,
        dt: f32,
        hf: &Heightfield,
    ) {
        self.yaw += look_dx * SENS;
        self.pitch = (self.pitch - look_dy * SENS).clamp(-1.4, 1.4);

        // Ease the crouch stance toward its target (~6/s) so the eye glides down/up, never snaps.
        let target = if crouch { 1.0 } else { 0.0 };
        self.crouch_t += (target - self.crouch_t).clamp(-6.0 * dt, 6.0 * dt);

        let (sy, cy) = self.yaw.sin_cos();
        let f = Vec3::new(sy, 0.0, -cy); // ground-plane forward (matches Camera at pitch 0)
        let r = Vec3::new(cy, 0.0, sy); // ground-plane right
        let wish = (f * fwd + r * strafe).normalize_or_zero();
        // RUNNING is the default gait (the Last Tiger sprint — Shift now toggles binoculars);
        // crouch is the slow, quiet, low-signature stalk. WALK survives for scripted use.
        let _ = run;
        let speed = if crouch { CROUCH_WALK } else { RUN };
        self.speed_frac = if wish.length_squared() > 0.01 {
            (speed / RUN).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.pos += wish * speed * dt;
        // Vertical: a jump lofts you, gravity brings you back; the ground snap only re-engages on
        // landing (so you can hop a low wall or step off the hull deck). Can't jump mid-air or crouched.
        let ground = hf.height_at(self.pos.x, self.pos.z);
        if jump && !self.airborne && self.crouch_t < 0.2 {
            self.vel_y = JUMP_V;
            self.airborne = true;
        }
        if self.airborne {
            self.vel_y -= GRAVITY * dt;
            self.pos.y += self.vel_y * dt;
            if self.pos.y <= ground {
                self.pos.y = ground;
                self.vel_y = 0.0;
                self.airborne = false;
            }
        } else {
            self.pos.y = ground; // grounded: track the terrain
        }

        if self.cooldown > 0.0 {
            self.cooldown -= dt;
        }
        if self.flash_t > 0.0 {
            self.flash_t -= dt;
        }
    }

    /// Apply mouse-look deltas (used when foot.update isn't called, e.g. mid-climb).
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * SENS;
        self.pitch = (self.pitch - dy * SENS).clamp(-1.4, 1.4);
    }

    /// Stance-aware eye — the ONLY place camera height is read, so the whole crouch visual falls
    /// out here (game.rs sets `camera.pos = foot.eye()`).
    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::Y * (EYE + (EYE_CROUCH - EYE) * self.crouch_t)
    }

    /// The single loudness/visibility knob the detection model reads — one place couples stance to
    /// detection. Returns `(noise_radius_m, visibility)`: `visibility` scales a soldier's effective
    /// sight range (crouched = smaller target); `noise_radius` is how far this frame's movement
    /// carries (0 when still, tiny crouched, loud running).
    pub fn profile(&self, moving: bool, run: bool) -> (f32, f32) {
        let visibility = 1.0 - 0.55 * self.crouch_t; // crouched ≈ 0.45
        let noise = if !moving {
            0.0
        } else if self.crouch_t > 0.5 {
            5.0 // crouch-walk: near silent
        } else if run {
            22.0 // running is LOUD
        } else {
            12.0 // upright walk
        };
        (noise, visibility)
    }
}
