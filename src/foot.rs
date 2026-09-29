//! ABSITZEN ("dismount") — the on-foot dismounted soldier: a first-person controller, supply
//! pickups, and Luger-pistol FX state. Pure game-logic (no wgpu): the soldier walks the terrain
//! heightfield, mouse-looks by accumulating yaw/pitch directly (never through `set_forward`, which
//! can't accumulate and gimbal-locks at ±90°), and the camera + held-weapon transforms are driven
//! from this state each frame in `game.rs`.

use glam::{Mat3, Mat4, Quat, Vec3};

use crate::tank::Tank;
use crate::terrain::Heightfield;

/// Full standing stature (six feet); the eyes remain below the crown.
pub const STANDING_HEIGHT: f32 = 1.8288;
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
pub const JUMP_V: f32 = 7.1; // ~1.8 m rise: ground -> Tiger hull deck -> turret roof
pub const GRAVITY: f32 = 14.0;
/// Mouse look sensitivity — matches the commander head-look (game.rs).
pub const SENS: f32 = 0.0022;
pub const BODY_RADIUS: f32 = 0.28;
const STEP_HEIGHT: f32 = 0.30;

/// Authored weapons point along +Z and keep their sights along +Y. A shortest-arc
/// forward rotation leaves roll unconstrained when looking toward camera -Z.
pub fn held_weapon_rotation(forward: Vec3, up: Vec3) -> Quat {
    let forward = forward.normalize();
    let left = up.cross(forward).normalize();
    let up = forward.cross(left).normalize();
    Quat::from_mat3(&Mat3::from_cols(left, up, forward))
}

/// Stand beside the cupola on source roof geometry, with space for the upright body.
/// The nearby alternatives avoid an open lid; failure leaves the player aboard.
pub fn dismount_roof_position(
    turret: Mat4,
    mut contact: impl FnMut(Vec3, Vec3) -> Option<(Vec3, Vec3)>,
) -> Option<Vec3> {
    for (x, z) in [(-0.38, -0.72), (-0.40, -0.85)] {
        let from = turret.transform_point3(Vec3::new(x, 3.8, z));
        let to = turret.transform_point3(Vec3::new(x, 2.1, z));
        let Some((feet, normal)) = contact(from, to) else { continue; };
        if normal.y < 0.75 { continue; }
        let clear = [Vec3::ZERO, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z]
            .into_iter()
            .all(|offset| {
                let base = feet + offset * BODY_RADIUS;
                contact(base + Vec3::Y * 0.05, base + Vec3::Y * STANDING_HEIGHT).is_none()
            });
        if clear { return Some(feet); }
    }
    None
}

/// Enter only by descending through the open cupola aperture. Being near a tank or clicking
/// from ground level never qualifies; the feet must actually cross the rim from above.
pub fn cupola_entry_crossing(previous: Vec3, current: Vec3, centre: Vec3, fully_open: bool) -> bool {
    hatch_entry_crossing(previous, current, Vec3::new(centre.x, 2.58, centre.z), Vec3::Y, 0.25, fully_open)
}

/// Crossing a real hatch plane from its exterior side. The accepted movement must be supplied
/// after solid-body collision; this predicate never cancels a roof or lets a body through a lid.
pub fn hatch_entry_crossing(previous: Vec3, current: Vec3, center: Vec3, normal: Vec3,
    radius: f32, fully_open: bool) -> bool
{
    let before = (previous - center).dot(normal);
    let after = (current - center).dot(normal);
    if !fully_open || before < 0.0 || after >= 0.0 || after >= before { return false; }
    let crossing = previous.lerp(current, before / (before - after));
    let offset = crossing - center;
    (offset - normal * offset.dot(normal)).length_squared() < radius * radius
}

/// Releasing crouch may raise the body only through clear space. Five vertical probes cover
/// the head/shoulders so a centreline under a beam edge cannot let the skull pass through it.
pub fn standing_headroom(feet: Vec3, current_height: f32, standing_height: f32, radius: f32,
    mut blocked: impl FnMut(Vec3, Vec3) -> bool) -> bool
{
    if current_height >= standing_height - 1e-4 { return true; }
    [Vec3::ZERO, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z].into_iter().all(|offset| {
        let base = feet + offset * radius;
        !blocked(base + Vec3::Y * (current_height - 0.01),
            base + Vec3::Y * (standing_height + 0.015))
    })
}

/// The supply classes a scavenger picks up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickupKind {
    Ammo,
    Fuel,
    Medical,
    /// The treasure — gated behind the deep patrols in the city ring.
    Cigarettes,
}

/// The deliberately tiny dismounted arsenal. Number keys select directly; Q cycles.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FootWeapon {
    #[default]
    Luger,
    Panzerfaust,
}

/// A hand-portable world prop. The enum is intentionally broader than fuel so later physical loot
/// (toolboxes, map cases, ammunition tins) can reuse the same equip/drop/throw machinery without
/// another one-off inventory boolean.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PortableKind {
    JerryCan,
}

/// What is physically inside a portable prop. Contents, not the mesh slot, decide mass and use; an
/// emptied can therefore remains the exact same persistent object in the snow.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PortableContents {
    FuelLiters(f32),
    Empty,
}

pub const JERRYCAN_FUEL_LITERS: f32 = 20.0;
const JERRYCAN_FULL_MASS_KG: f32 = 19.3; // 4.5 kg can + ~20 L winter petrol
const JERRYCAN_EMPTY_MASS_KG: f32 = 4.5;
const JERRYCAN_FULL_THROW_IMPULSE: f32 = 45.0;
const JERRYCAN_EMPTY_THROW_IMPULSE: f32 = 58.0;
const PORTABLE_GRAVITY: f32 = 14.0;

/// Runtime rigid-body state for a preallocated portable mesh. Position and renderer slot stay on
/// [`Pickup`], so a campaign loot reseed can move the object without rebuilding the renderer scene.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PortableItem {
    pub kind: PortableKind,
    pub contents: PortableContents,
    pub equipped: bool,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    /// Euler angles used only to give the low-poly prop readable physical tumble.
    pub rotation: Vec3,
    pub grounded: bool,
}

impl PortableItem {
    pub fn full_jerry_can() -> Self {
        Self {
            kind: PortableKind::JerryCan,
            contents: PortableContents::FuelLiters(JERRYCAN_FUEL_LITERS),
            equipped: false,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            rotation: Vec3::ZERO,
            grounded: true,
        }
    }

    pub fn has_fuel(self) -> bool {
        self.kind == PortableKind::JerryCan
            && matches!(self.contents, PortableContents::FuelLiters(liters) if liters > 0.0)
    }

    pub fn fuel_liters(self) -> f32 {
        match self.contents {
            PortableContents::FuelLiters(liters) => liters.max(0.0),
            PortableContents::Empty => 0.0,
        }
    }

    fn fill_fraction(self) -> f32 {
        (self.fuel_liters() / JERRYCAN_FUEL_LITERS).clamp(0.0, 1.0)
    }

    pub fn mass_kg(self) -> f32 {
        JERRYCAN_EMPTY_MASS_KG
            + (JERRYCAN_FULL_MASS_KG - JERRYCAN_EMPTY_MASS_KG) * self.fill_fraction()
    }

    pub fn equip(&mut self) {
        self.equipped = true;
        self.velocity = Vec3::ZERO;
        self.angular_velocity = Vec3::ZERO;
        self.grounded = false;
    }

    /// Set an equipped item down. E is placement, never a disguised weak throw: both linear and
    /// angular impulse are exactly zero.
    pub fn put_down(&mut self) {
        self.equipped = false;
        self.velocity = Vec3::ZERO;
        self.angular_velocity = Vec3::ZERO;
        self.grounded = true;
    }

    /// Release the equipped prop with a mass-aware impulse. An empty steel shell is deliberately far
    /// more throwable than a full 20-litre can, making contents legible through motion alone.
    pub fn throw(&mut self, forward: Vec3) {
        let direction =
            Vec3::new(forward.x, forward.y.max(-0.15) + 0.14, forward.z).normalize_or_zero();
        let fill = self.fill_fraction();
        let impulse = JERRYCAN_EMPTY_THROW_IMPULSE
            + (JERRYCAN_FULL_THROW_IMPULSE - JERRYCAN_EMPTY_THROW_IMPULSE) * fill;
        self.equipped = false;
        self.velocity = direction * (impulse / self.mass_kg());
        self.angular_velocity = Vec3::new(5.4, 2.2, -3.8).lerp(Vec3::new(1.2, 0.55, -0.8), fill);
        self.grounded = false;
    }

    /// Transfer up to `requested_liters` from the physical contents. A partially accepted can keeps
    /// its exact remainder; only the final transfer turns it into a persistent empty steel shell.
    pub fn drain_fuel_liters(&mut self, requested_liters: f32) -> f32 {
        let available = self.fuel_liters();
        let transferred = requested_liters.max(0.0).min(available);
        let remaining = available - transferred;
        self.contents = if remaining > 0.01 {
            PortableContents::FuelLiters(remaining)
        } else {
            PortableContents::Empty
        };
        transferred
    }

    /// Drain all remaining fuel. Kept as the convenient full-can operation used by tests and any
    /// future inventory consumer that genuinely accepts the complete contents.
    pub fn drain_fuel(&mut self) -> f32 {
        self.drain_fuel_liters(f32::INFINITY)
    }

    /// Advance one portable prop against a supplied ground plane. No lifetime/retirement exists:
    /// empty and dropped cans settle but remain visible for the full scene.
    pub fn integrate(&mut self, pos: &mut Vec3, ground_y: f32, dt: f32) {
        if self.equipped {
            return;
        }
        if self.grounded && self.velocity.length_squared() < 1e-5 {
            pos.y = ground_y;
            return;
        }
        self.velocity.y -= PORTABLE_GRAVITY * dt;
        *pos += self.velocity * dt;
        self.rotation += self.angular_velocity * dt;
        if pos.y <= ground_y {
            pos.y = ground_y;
            if self.velocity.y < -0.8 {
                self.velocity.y = -self.velocity.y * 0.24;
                self.velocity.x *= 0.58;
                self.velocity.z *= 0.58;
                self.angular_velocity *= 0.72;
            } else {
                self.velocity.y = 0.0;
                let damping = (1.0 - 5.5 * dt).max(0.0);
                self.velocity.x *= damping;
                self.velocity.z *= damping;
                self.angular_velocity *= (1.0 - 4.0 * dt).max(0.0);
                if self.velocity.length_squared() < 0.0025 {
                    self.velocity = Vec3::ZERO;
                    self.angular_velocity = Vec3::ZERO;
                    self.grounded = true;
                }
            }
        }
    }
}

/// Typed supply tally — the scavenging readout (no HUD text renderer exists; a `println!` at the
/// collect site is the verification surface).
#[derive(Default, Clone, Copy)]
pub struct Supplies {
    /// World ammo-box caches. Each cache is six AP rounds when banked at the Tiger.
    pub ammo: u32,
    /// Exact authored loose 88 mm rounds recovered from storage; never multiplied as a cache.
    pub ap_88mm: u32,
    pub he_88mm: u32,
    pub fuel: u32,
    pub medical: u32,
    pub cigarettes: u32,
    pub rations: u32,
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
        self.ap_88mm += other.ap_88mm;
        self.he_88mm += other.he_88mm;
        self.fuel += other.fuel;
        self.medical += other.medical;
        self.cigarettes += other.cigarettes;
        self.rations += other.rations;
    }

    pub fn total(&self) -> u32 {
        self.ammo
            + self.ap_88mm
            + self.he_88mm
            + self.fuel
            + self.medical
            + self.cigarettes
            + self.rations
    }
}

/// A world-placed collectible. `slot` is its pre-allocated renderer mesh index (the scene is fixed
/// at `Renderer::new`, so collecting just hides the slot). Positioned once via `set_model`.
pub struct Pickup {
    pub pos: Vec3,
    pub slot: usize,
    pub kind: PickupKind,
    pub collected: bool,
    /// `Some` means this pickup is a persistent E-equipped world object rather than a proximity
    /// tally. Fuel cans are the first user; ordinary ammo/medical/cigarettes retain auto-collect.
    pub portable: Option<PortableItem>,
}

/// The dismounted player. Terrain motion is followed by source-surface support and body collision.
pub struct FootSoldier {
    pub pos: Vec3, // ground contact (feet)
    pub yaw: f32,
    pub pitch: f32,
    pub aiming: bool, // RMB — raise the selected weapon (narrow FOV + iron sight alignment)
    pub weapon: FootWeapon,
    pub flash_t: f32,    // Luger muzzle-flash timer
    pub cooldown: f32,   // pistol rate-limit
    pub prev_fire: bool, // rising-edge latch for LMB
    pub supplies: Supplies,
    pub ammo: u32, // pistol rounds IN RESERVE (loose rounds; the magazine is separate)
    pub mag: u32,  // ready ammunition total: seated magazine plus the chamber
    pub p08_chambered: bool,
    pub p08_reload_empty: bool,
    pub p08_cycle_t: f32,
    pub reload_t: f32, // >0 = mid magazine change (R); firing locked out until it seats
    /// A Panzerfaust is a disposable launcher. `ready` is the tube in hand and `spares` is the
    /// one additional complete tube carried by this crewman.
    pub panzerfaust_ready: bool,
    pub panzerfaust_spares: u8,
    pub panzerfaust_swap_t: f32,
    pub panzerfaust_flash_t: f32,
    pub recoil: f32, // 0..1 muzzle-flip kick, decays fast (drives camera + pistol pose)
    pub speed_frac: f32, // 0..1 of full sprint this frame — feeds tank crews' detection of the man
    pub vel_y: f32,  // vertical velocity (jump/gravity); 0 = grounded
    pub airborne: bool, // true between a jump and the next landing (ground snap suspended)
    pub crouch_t: f32, // 0 = stand, 1 = crouch (smoothed so the camera glides, never snaps)
    pub hp: f32,     // health on foot; 0 = KIA (drop the unbanked haul, respawn at the Tiger)
}

/// P08 magazine capacity.
pub const MAG_CAP: u32 = 8;
/// Magazine change time (drop, seat, rack) — deliberately unhurried; this is a last-resort weapon.
pub const RELOAD_S: f32 = 1.6;
/// Time to discard a fired tube, draw the spare, remove the safety and unfold its sight.
pub const PANZERFAUST_SWAP_S: f32 = 2.7;
/// Panzerfaust 60 launch speed and bounded practical range.
pub const PANZERFAUST_V0: f32 = 45.0;
pub const PANZERFAUST_MAX_RANGE: f32 = 80.0;
/// Central PzF 60 aperture: no-drag 60 m zero at 45 m/s.
pub const PANZERFAUST_ZERO_ANGLE_DEG: f32 = 8.45;

impl Default for FootSoldier {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            aiming: false,
            weapon: FootWeapon::Luger,
            flash_t: 0.0,
            cooldown: 0.0,
            prev_fire: false,
            supplies: Supplies::default(),
            // One loaded magazine plus one spare magazine.
            ammo: MAG_CAP,
            mag: MAG_CAP,
            p08_chambered: true,
            p08_reload_empty: false,
            p08_cycle_t: 0.0,
            reload_t: 0.0,
            panzerfaust_ready: false,
            panzerfaust_spares: 0,
            panzerfaust_swap_t: 0.0,
            panzerfaust_flash_t: 0.0,
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
    /// Repack a persistent total into the seated magazine plus reserve without creating rounds.
    pub fn set_total_p08_rounds(&mut self, total: u32) {
        self.mag = total.min(MAG_CAP);
        self.ammo = total.saturating_sub(self.mag);
        self.reload_t = 0.0;
        self.p08_chambered = self.mag > 0;
        self.p08_reload_empty = false;
        self.p08_cycle_t = 0.0;
    }

    pub fn total_p08_rounds(&self) -> u32 {
        self.mag.saturating_add(self.ammo)
    }

    pub fn p08_reload_capacity(&self) -> u32 {
        MAG_CAP + u32::from(self.p08_chambered)
    }

    pub fn begin_p08_reload(&mut self) -> bool {
        if self.reload_t > 0.0 || self.ammo == 0 || self.mag >= self.p08_reload_capacity() {
            return false;
        }
        self.p08_reload_empty = !self.p08_chambered;
        self.reload_t = RELOAD_S;
        true
    }

    /// Stage crossings drive both the physical parts and their one-shot recordings. Bits are
    /// magazine out, magazine seated, toggle pull, spring close. Large timesteps cannot skip a
    /// transfer or duplicate ammunition; a tactical change never opens the loaded chamber.
    pub fn advance_p08_reload(&mut self, dt: f32) -> u8 {
        self.p08_cycle_t = (self.p08_cycle_t - dt.max(0.0)).max(0.0);
        if self.reload_t <= 0.0 { return 0; }
        let before = 1.0 - self.reload_t / RELOAD_S;
        self.reload_t = (self.reload_t - dt.max(0.0)).max(0.0);
        let after = 1.0 - self.reload_t / RELOAD_S;
        let crossed = |p: f32| before < p && after >= p;
        let mut events = 0;
        if crossed(0.10) { events |= 1; }
        if crossed(0.70) {
            let capacity = MAG_CAP + u32::from(!self.p08_reload_empty);
            let take = capacity.saturating_sub(self.mag).min(self.ammo);
            self.mag += take;
            self.ammo -= take;
            events |= 2;
        }
        if self.p08_reload_empty && crossed(0.81) { events |= 4; }
        if self.p08_reload_empty && crossed(0.93) {
            self.p08_chambered = self.mag > 0;
            events |= 8;
        }
        events
    }

    pub fn discharge_p08(&mut self) -> bool {
        if !self.p08_chambered || self.mag == 0 || self.reload_t > 0.0 { return false; }
        self.mag -= 1;
        self.p08_chambered = self.mag > 0;
        self.p08_cycle_t = 0.12;
        true
    }

    pub fn unfired_panzerfaust_tubes(&self) -> u8 {
        u8::from(self.panzerfaust_ready).saturating_add(self.panzerfaust_spares)
    }

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
        // Hold Shift to sprint. Crouch has priority, including when both modifiers are held.
        let speed = if crouch { CROUCH_WALK } else if run { RUN } else { WALK };
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
        if !self.airborne && self.pos.y > ground + STEP_HEIGHT {
            // An elevated support gets the final say after this terrain step. Losing a roof or
            // deck starts a real fall; it must never teleport the feet down through that vehicle.
            self.airborne = true;
            self.vel_y = 0.0;
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
        if self.panzerfaust_flash_t > 0.0 {
            self.panzerfaust_flash_t -= dt;
        }
    }

    /// Resolve one nearby articulated solid through its exact source-triangle segment query.
    /// Contacts are world-space (position, normal). Nine bounded body sweeps retain shoulder
    /// clearance; a descending foot sweep owns support, including ledges and real jumps.
    pub fn resolve_solid_motion(
        &mut self,
        previous: Vec3,
        was_airborne: bool,
        mut contact: impl FnMut(Vec3, Vec3) -> Option<(Vec3, Vec3)>,
    ) {
        let standing = self.body_height();
        let delta = Vec3::new(self.pos.x - previous.x, 0.0, self.pos.z - previous.z);
        let distance = delta.length();
        if distance > 1e-6 {
            let direction = delta / distance;
            let lateral = direction.cross(Vec3::Y) * (BODY_RADIUS * 0.85);
            let base = Vec3::new(previous.x, previous.y.max(self.pos.y), previous.z);
            let mut allowed = distance;
            for height in [0.32, standing * 0.5, standing - 0.05] {
                for side in [-1.0, 0.0, 1.0] {
                    let from = base + Vec3::Y * height + lateral * side;
                    if let Some((point, normal)) = contact(from, from + direction * (distance + BODY_RADIUS)) {
                        if normal.y.abs() < 0.7 {
                            allowed = allowed.min(((point - from).dot(direction) - BODY_RADIUS - 0.005).max(0.0));
                        }
                    }
                }
            }
            self.pos.x = previous.x + direction.x * allowed;
            self.pos.z = previous.z + direction.z * allowed;
        }
        if self.airborne && self.vel_y > 0.0 {
            let from = Vec3::new(self.pos.x, previous.y + standing, self.pos.z);
            if let Some((point, _)) = contact(from, self.pos + Vec3::Y * (standing + 0.01)) {
                self.pos.y = point.y - standing - 0.01;
                self.vel_y = 0.0;
            }
            return;
        }
        let high = previous.y + if was_airborne { 0.02 } else { STEP_HEIGHT };
        let low = self.pos.y.min(previous.y) - 0.03;
        let from = Vec3::new(self.pos.x, high, self.pos.z);
        let to = Vec3::new(self.pos.x, low, self.pos.z);
        if let Some((point, normal)) = contact(from, to) {
            if normal.y > 0.55 {
                self.pos.y = point.y;
                self.vel_y = 0.0;
                self.airborne = false;
            }
        }
    }

    /// Apply mouse-look deltas (used when foot.update isn't called, e.g. mid-climb).
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * SENS;
        self.pitch = (self.pitch - dy * SENS).clamp(-1.4, 1.4);
    }

    pub fn body_height(&self) -> f32 {
        STANDING_HEIGHT + (EYE_CROUCH - EYE) * self.crouch_t
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

#[cfg(test)]
mod tests {
    #[test]
    fn cupola_entry_requires_open_lid_and_a_descent_through_actual_aperture() {
        let centre = glam::Vec3::new(0.564, 2.415, -0.423);
        let above = centre + glam::Vec3::Y * 0.6;
        let below = centre - glam::Vec3::Y * 0.1;
        assert!(super::cupola_entry_crossing(above, below, centre, true));
        assert!(!super::cupola_entry_crossing(above, below, centre, false));
        assert!(!super::cupola_entry_crossing(below, above, centre, true));
        assert!(!super::cupola_entry_crossing(above + glam::Vec3::X,
            below + glam::Vec3::X, centre, true));
        assert!(!super::cupola_entry_crossing(below, below - glam::Vec3::Y, centre, true));
    }

    #[test]
    fn jump_can_reach_hull_deck_without_bypassing_solid_contact() {
        assert!(super::JUMP_V * super::JUMP_V / (2.0 * super::GRAVITY) > 1.65);
    }

    #[test]
    fn crouch_release_waits_for_head_and_shoulder_clearance() {
        let foot = glam::Vec3::ZERO;
        assert!(super::standing_headroom(foot, 1.17, 1.83, 0.28, |_, _| false));
        assert!(!super::standing_headroom(foot, 1.17, 1.83, 0.28,
            |from, to| from.y < 1.45 && to.y > 1.45));
        assert!(!super::standing_headroom(foot, 1.17, 1.83, 0.28,
            |from, to| from.x > 0.2 && from.y < 1.45 && to.y > 1.45));
        assert!(super::standing_headroom(foot, 1.17, 1.83, 0.28,
            |from, to| from.y < 2.0 && to.y > 2.0));
    }

    use super::*;

    fn flat_ground() -> Heightfield {
        Heightfield { res: 2, spacing: 100.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0, openings: Default::default() }
    }

    #[test]
    fn hold_sprint_changes_gait_and_crouch_overrides_sprint() {
        let ground = flat_ground();
        let travel = |run, crouch, strafe| {
            let mut foot = FootSoldier { pos: Vec3::ZERO, yaw: 0.0, ..FootSoldier::default() };
            for _ in 0..60 {
                foot.update(1.0, strafe, run, crouch, false, 0.0, 0.0, 1.0 / 60.0, &ground);
            }
            foot
        };
        let walk = travel(false, false, 0.0);
        let sprint = travel(true, false, 0.0);
        let diagonal = travel(true, false, 1.0);
        let crouch = travel(true, true, 0.0);
        assert!(sprint.pos.length() > walk.pos.length() * 2.0);
        assert!((diagonal.pos.length() - sprint.pos.length()).abs() < 0.001);
        assert!(crouch.pos.length() < walk.pos.length());
        assert!(crouch.eye().y < walk.eye().y - 0.5);
        assert!(crouch.profile(true, true).0 < walk.profile(true, false).0);
        let mut released = sprint;
        let before = released.pos;
        released.update(1.0, 0.0, false, false, false, 0.0, 0.0, 0.1, &ground);
        assert!((released.pos.distance(before) - WALK * 0.1).abs() < 0.001);
    }

    // Closed source-like box: exact face intersections, not a height-only support stub.
    fn solid_box(from: Vec3, to: Vec3) -> Option<(Vec3, Vec3)> {
        let min = Vec3::new(-1.5, 0.0, -2.0);
        let max = Vec3::new(1.5, 2.46, 2.0);
        let delta = to - from;
        let mut best: Option<(f32, Vec3, Vec3)> = None;
        for axis in 0..3 {
            if delta[axis].abs() < 1e-7 { continue; }
            for edge in [min[axis], max[axis]] {
                let t = (edge - from[axis]) / delta[axis];
                if !(0.0..=1.0).contains(&t) { continue; }
                let point = from + delta * t;
                if (0..3).filter(|&a| a != axis).any(|a| point[a] < min[a] || point[a] > max[a]) { continue; }
                if best.is_none_or(|(b, _, _)| t < b) {
                    let mut normal = Vec3::ZERO;
                    normal[axis] = -delta[axis].signum();
                    best = Some((t, point, normal));
                }
            }
        }
        best.map(|(_, point, normal)| (point, normal))
    }

    fn solid_step(foot: &mut FootSoldier, drive: f32, jump: bool) {
        let previous = foot.pos;
        let was_airborne = foot.airborne;
        foot.update(drive, 0.0, false, false, jump, 0.0, 0.0, 1.0 / 120.0, &flat_ground());
        foot.resolve_solid_motion(previous, was_airborne, solid_box);
    }

    #[test]
    fn foot_weapon_sights_remain_upright_across_forward_antipode() {
        for yaw in [-3.14_f32, -1.57, -0.001, 0.0, 0.001, 1.57, 3.14] {
            for pitch in [-1.4_f32, -0.1, -0.001, 0.0, 0.001, 0.1, 1.4] {
                let forward = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos());
                let right = forward.cross(Vec3::Y).normalize();
                let up = right.cross(forward);
                let rotation = held_weapon_rotation(forward, up);
                assert!((rotation * Vec3::Z).dot(forward) > 0.99999, "muzzle {yaw}/{pitch}");
                assert!((rotation * Vec3::Y).dot(up) > 0.99999, "sights {yaw}/{pitch}");
                assert!((Mat3::from_quat(rotation).determinant() - 1.0).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn foot_dismount_selects_clear_roof_and_fails_if_no_support_exists() {
        let roof = dismount_roof_position(Mat4::IDENTITY, solid_box).unwrap();
        assert!((roof.y - 2.46).abs() < 1e-5);
        assert!(dismount_roof_position(Mat4::IDENTITY, |_, _| None).is_none());
        // A body-height obstruction over the first point forces another clear adjacent point.
        let alternative = dismount_roof_position(Mat4::IDENTITY, |from, to| {
            if from.y < 3.0 && from.x == -0.38 && (from.z + 0.72).abs() < 1e-5 {
                Some((from.lerp(to, 0.5), Vec3::NEG_Y))
            } else { solid_box(from, to) }
        }).unwrap();
        assert!(alternative.distance(roof) > 0.1);
    }

    #[test]
    fn foot_stands_on_roof_and_jumps_back_onto_source_surface() {
        let mut foot = FootSoldier::default();
        foot.pos = dismount_roof_position(Mat4::IDENTITY, solid_box).unwrap();
        for _ in 0..120 { solid_step(&mut foot, 0.0, false); }
        assert!((foot.pos.y - 2.46).abs() < 1e-5);
        assert!(!foot.airborne);
        solid_step(&mut foot, 0.0, true);
        assert!(foot.airborne && foot.pos.y > 2.46 && foot.vel_y > 0.0);
        let mut peak = foot.pos.y;
        for _ in 0..120 {
            solid_step(&mut foot, 0.0, false);
            peak = peak.max(foot.pos.y);
        }
        assert!(peak > 3.0);
        assert!(!foot.airborne);
        assert!((foot.pos.y - 2.46).abs() < 1e-5);
    }

    #[test]
    fn foot_walks_off_roof_without_teleporting_through_the_hull() {
        let mut foot = FootSoldier::default();
        foot.pos = Vec3::new(0.0, 2.46, -1.99);
        solid_step(&mut foot, 1.0, false);
        assert!(foot.pos.z < -2.0);
        assert!(foot.airborne && foot.pos.y > 2.4);
        for _ in 0..120 { solid_step(&mut foot, 1.0, false); }
        assert!(!foot.airborne && foot.pos.y.abs() < 1e-5);
    }

    #[test]
    fn foot_body_cannot_walk_through_tiger_sized_armor() {
        let mut foot = FootSoldier::default();
        foot.pos = Vec3::new(0.0, 0.0, 3.0);
        for _ in 0..120 { solid_step(&mut foot, 1.0, false); }
        assert!(foot.pos.z >= 2.0 + BODY_RADIUS);
        assert!(foot.pos.z < 2.0 + BODY_RADIUS + 0.02);
        assert_eq!(foot.pos.y, 0.0);
    }

    #[test]
    fn empty_p08_stays_open_through_mag_change_then_spring_closes() {
        let mut f=FootSoldier::default();
        for _ in 0..MAG_CAP { assert!(f.discharge_p08()); }
        assert!(!f.p08_chambered); assert_eq!(f.mag,0); assert!(!f.discharge_p08());
        let total=f.total_p08_rounds(); assert!(f.begin_p08_reload());
        assert_eq!(f.advance_p08_reload(RELOAD_S*0.71),3);
        assert_eq!(f.mag,8); assert!(!f.p08_chambered); assert!(!f.discharge_p08());
        assert_eq!(f.advance_p08_reload(RELOAD_S*0.11),4); assert!(!f.p08_chambered);
        assert_eq!(f.advance_p08_reload(RELOAD_S*0.12),8); assert!(f.p08_chambered);
        f.advance_p08_reload(RELOAD_S); assert_eq!(f.total_p08_rounds(),total);
        assert!(f.discharge_p08());
    }

    #[test]
    fn tactical_p08_change_keeps_chamber_and_never_racks_or_creates_ammo() {
        let mut f=FootSoldier::default(); assert!(f.discharge_p08());
        let total=f.total_p08_rounds();assert!(f.begin_p08_reload());
        assert!(f.p08_chambered); assert_eq!(f.advance_p08_reload(RELOAD_S*2.0),3);
        assert_eq!(f.mag,MAG_CAP+1);assert!(f.p08_chambered);
        assert_eq!(f.total_p08_rounds(),total);assert!(!f.begin_p08_reload());
        assert_eq!(f.advance_p08_reload(RELOAD_S),0);
        assert!(f.discharge_p08());
    }

    #[test]
    fn fps_starts_with_two_p08_magazines_total() {
        let foot = FootSoldier::default();
        assert_eq!(foot.mag, 8);
        assert_eq!(foot.ammo, 8);
    }

    #[test]
    fn panzerfaust_must_be_claimed_from_the_tiger() {
        let foot = FootSoldier::default();
        assert!(!foot.panzerfaust_ready);
        assert_eq!(foot.panzerfaust_spares, 0);
    }

    #[test]
    fn persistent_p08_total_never_conjures_a_fresh_magazine() {
        for total in 0..32 {
            let mut foot = FootSoldier::default();
            foot.set_total_p08_rounds(total);
            assert_eq!(foot.total_p08_rounds(), total);
        }
        let mut fired = FootSoldier::default();
        fired.mag -= 3;
        let remaining = fired.total_p08_rounds();
        let mut next_act = FootSoldier::default();
        next_act.set_total_p08_rounds(remaining);
        assert_eq!(next_act.total_p08_rounds(), 13);
    }

    #[test]
    fn panzerfaust_total_counts_only_complete_unfired_tubes() {
        let mut foot = FootSoldier::default();
        foot.panzerfaust_ready = true;
        foot.panzerfaust_spares = 1;
        assert_eq!(foot.unfired_panzerfaust_tubes(), 2);
        foot.panzerfaust_ready = false;
        assert_eq!(foot.unfired_panzerfaust_tubes(), 1);
        foot.panzerfaust_spares = 0;
        assert_eq!(foot.unfired_panzerfaust_tubes(), 0);
    }

    #[test]
    fn placing_a_portable_item_has_exactly_zero_impulse() {
        let mut can = PortableItem::full_jerry_can();
        can.equip();
        can.put_down();
        assert_eq!(can.velocity, Vec3::ZERO);
        assert_eq!(can.angular_velocity, Vec3::ZERO);
        assert!(can.grounded);
    }

    #[test]
    fn fuel_transfer_is_one_way_and_leaves_a_physical_empty_can() {
        let mut can = PortableItem::full_jerry_can();
        assert_eq!(can.drain_fuel(), JERRYCAN_FUEL_LITERS);
        assert_eq!(can.contents, PortableContents::Empty);
        assert_eq!(can.drain_fuel(), 0.0);
        assert_eq!(can.kind, PortableKind::JerryCan);
    }

    #[test]
    fn partial_fuel_transfer_preserves_the_exact_remainder() {
        let mut can = PortableItem::full_jerry_can();
        assert_eq!(can.drain_fuel_liters(5.0), 5.0);
        assert_eq!(can.contents, PortableContents::FuelLiters(15.0));
        assert_eq!(can.drain_fuel_liters(20.0), 15.0);
        assert_eq!(can.contents, PortableContents::Empty);
    }

    #[test]
    fn empty_can_has_substantially_more_throw_range_than_full_can() {
        let mut full = PortableItem::full_jerry_can();
        let mut empty = full;
        empty.drain_fuel();
        full.equip();
        empty.equip();
        full.throw(Vec3::Z);
        empty.throw(Vec3::Z);
        assert!(empty.velocity.length() > full.velocity.length() * 4.0);

        let mut full_pos = Vec3::new(0.0, 1.2, 0.0);
        let mut empty_pos = full_pos;
        for _ in 0..60 {
            full.integrate(&mut full_pos, 0.0, 1.0 / 60.0);
            empty.integrate(&mut empty_pos, 0.0, 1.0 / 60.0);
        }
        assert!(empty_pos.z > full_pos.z * 3.0);
        assert_eq!(empty.contents, PortableContents::Empty);
    }

    #[test]
    fn partial_can_mass_and_throw_speed_track_remaining_litres() {
        let mut full = PortableItem::full_jerry_can();
        let mut half = full;
        let mut empty = full;
        half.drain_fuel_liters(JERRYCAN_FUEL_LITERS * 0.5);
        empty.drain_fuel();

        assert!((full.mass_kg() - JERRYCAN_FULL_MASS_KG).abs() < 1.0e-5);
        assert!((empty.mass_kg() - JERRYCAN_EMPTY_MASS_KG).abs() < 1.0e-5);
        assert!((half.mass_kg() - 11.9).abs() < 1.0e-4);

        full.throw(Vec3::Z);
        half.throw(Vec3::Z);
        empty.throw(Vec3::Z);
        assert!(full.velocity.length() < half.velocity.length());
        assert!(half.velocity.length() < empty.velocity.length());
    }
}
