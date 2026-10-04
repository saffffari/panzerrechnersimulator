//! Soviet 45 mm anti-tank guns (53-K, model 1937) in the winter encounter: an emplaced gun with
//! its artillery crew, fighting by the doctrine of winter 1942–43. Sources:
//! docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md.
//!
//! From December 1942 each rifle battalion had a platoon of two 45 mm guns served by specialist
//! artillerymen. They were sited for flanking fire from concealment, held fire until tanks were
//! at close range (300–400 m for a 45 mm), aimed at flanks, tracks, the sprocket, vision devices
//! and machine guns, kept their scarce tungsten-core rounds for close flank shots, and went to
//! ground when a tank closed on them, re-manning the gun after it passed. Against a Tiger the
//! ordinary round practically never penetrates (Kubinka 1943); the harm is to what it strikes.
//! Values not given by the sources are tuning and are marked so.
use super::*;

/// Gun slots: the courtyard and street encounters field two (the battalion's platoon), the CANYON
/// course one per choke (src/canyon.rs). Crews fill the soldiers after the tank escorts.
pub(super) const AT_GUNS: usize = 6;
/// Men at the gun (the full detachment was six, plus drivers and horses kept away from it).
pub(super) const CREW: usize = 4;
/// Garrison soldiers after the tank escort pairs serve the guns.
pub(super) const FIRST_CREW_SOLDIER: usize = N_ENEMIES * 2;
/// ±30°: the 53-K's 60° on-carriage traverse.
pub(super) const TRAVERSE_LIMIT: f32 = 0.5236;
/// Aimed fire at a tank, about 7 rounds a minute (tuning; 15–20 was the practical maximum).
pub(super) const AIMED_INTERVAL_S: f32 = 8.5;
/// First round after the gun is laid on a new target (tuning).
const LAY_S: f32 = 3.0;
/// Keep an already laid weapon through a short interruption in the gun commander's sight.
/// Authored crew-response tuning, not extra perception: no aiming or firing without current LOS.
const REACQUIRE_HOLD_S: f32 = 1.5;
/// Hauling the trails round to bring the gun onto a target beyond its traverse (tuning).
pub(super) const SHIFT_TRAILS_S: f32 = 10.0;
/// Doctrine: hold fire until the tank is this close.
pub(super) const OPEN_FIRE_M: f32 = 400.0;
/// So close the gun fires whatever else is happening.
pub(super) const POINT_BLANK_M: f32 = 200.0;
/// Closer than this the crew goes to ground; it re-mans once the tank is past `REMAN_M`.
pub(super) const GROUND_M: f32 = 100.0;
pub(super) const REMAN_M: f32 = 150.0;
/// The shortest time in cover before re-manning (tuning).
const GROUND_S: f32 = 20.0;
/// Tungsten-core rounds were issued one at a time; wasting them risked a tribunal.
pub(super) const APCR_ROUNDS: u8 = 2;
/// Only for flank or rear shots this close (Kubinka: the 80 mm side at 200 m, the 60 mm lower
/// side at 350 m).
pub(super) const APCR_SIDE_M: f32 = 200.0;
pub(super) const APCR_LOWER_M: f32 = 350.0;
/// The sector the gun was sited to cover (tuning).
pub(super) const SECTOR_HALF: f32 = 1.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GunState {
    /// Concealed and watching its sector.
    Hidden,
    /// Laid on the Tiger and firing.
    Engaging,
    /// The crew hauls the trails round onto a target beyond the traverse.
    ShiftingTrails,
    /// The crew in its slit trenches while a tank is on top of them.
    Down,
    /// Every man of the crew is dead.
    Silenced,
    /// The gun itself is wrecked.
    Wrecked,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AtGun {
    /// Axle centre on the ground.
    pub pos: Vec3,
    /// Lower carriage (trails) heading: the line of fire at zero traverse.
    pub heading: f32,
    /// The heading it was sited on; its sector is centred here.
    pub sited: f32,
    /// Upper carriage (shield, cradle, barrel) relative to the lower.
    pub traverse: f32,
    pub state: GunState,
    pub timer: f32,
    sight_lost_s: f32,
    pub apcr: u8,
    pub shots: u32,
    /// The crew heard the fight start: a Tiger's gun, or its own platoon's other gun.
    pub heard: bool,
    /// Riflemen who ran to the gun after its artillerymen fell (emergency service).
    pub riflemen: [Option<usize>; RIFLEMEN],
}

/// Riflemen who can take over a gun whose crew is dead, and how far they will run to it. Crewing
/// by riflemen is documented only as an emergency (lone survivors, officers), so they serve it
/// badly: wider aim and slower service (tuning).
pub(super) const RIFLEMEN: usize = 2;
const RECREW_REACH_M: f32 = 60.0;
const RIFLEMAN_DISPERSION: f32 = 1.8;

impl AtGun {
    pub fn new(pos: Vec3, heading: f32) -> Self {
        Self { pos, heading, sited: heading, traverse: 0.0, state: GunState::Hidden, timer: 0.0, sight_lost_s: 0.0,
            apcr: APCR_ROUNDS, shots: 0, heard: false, riflemen: [None; RIFLEMEN] }
    }

    /// Lower carriage frame: origin on the ground at the axle, +Z toward the line of fire.
    pub fn carriage(&self) -> Mat4 {
        Mat4::from_translation(self.pos) * Mat4::from_rotation_y(self.heading)
    }
    /// Upper carriage frame: traverses about the pivot on the axle.
    pub fn upper(&self) -> Mat4 {
        self.carriage() * Mat4::from_rotation_y(self.traverse)
    }
    pub fn muzzle(&self) -> (Vec3, Vec3) {
        let upper = self.upper();
        (upper.transform_point3(Vec3::new(0.0, BORE_Y, BARREL_END)),
            upper.transform_vector3(Vec3::Z).normalize())
    }
    /// Exact current visual pose, also used for projectile contact after a gun is wrecked.
    pub(super) fn visible_upper(&self)->Mat4 {
        if self.state==GunState::Wrecked {
            self.upper()*Mat4::from_translation(Vec3::new(0.25,-0.18,-0.2))
                *Mat4::from_rotation_z(0.35)*Mat4::from_rotation_x(0.42)
        } else {self.upper()}
    }
    pub fn working(&self) -> bool {
        !matches!(self.state, GunState::Silenced | GunState::Wrecked)
    }

    /// One frame of the crew's doctrine. `contact` is what the gun commander sees of the Tiger;
    /// `crew` how many of its men are alive. Returns true when the gun fires this frame.
    pub fn step(&mut self, contact: Option<Contact>, crew: usize, dt: f32) -> bool {
        if self.state == GunState::Wrecked { return false; }
        if crew == 0 {
            self.state = GunState::Silenced;
            return false;
        }
        if self.state == GunState::Silenced { self.state = GunState::Hidden; }
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        self.timer = (self.timer - dt).max(0.0);
        // Fewer hands: slower service (tuning).
        let hands = if crew >= 3 { 1.0 } else { 1.6 };
        let Some(c) = contact.filter(|c| c.visible) else {
            self.sight_lost_s += dt;
            if self.state == GunState::Down {
                if self.timer == 0.0 { self.state = GunState::Hidden; }
            } else if self.sight_lost_s > REACQUIRE_HOLD_S {
                self.state = GunState::Hidden;
            }
            // Reload/service can finish while concealed, but the last observed pose is frozen.
            // In particular, never use the bearing/range carried by an invisible Contact.
            return false;
        };
        self.sight_lost_s = 0.0;
        if self.state == GunState::Down {
            if self.timer == 0.0 && c.range_m > REMAN_M { self.state = GunState::Hidden; }
            return false;
        }
        if c.range_m < GROUND_M {
            self.state = GunState::Down;
            self.timer = GROUND_S;
            return false;
        }
        let in_sector = wrap(c.bearing - self.sited).abs() <= SECTOR_HALF;
        let engage = c.range_m <= POINT_BLANK_M || (c.alerted && in_sector && c.range_m <= OPEN_FIRE_M);
        if !engage {
            self.state = GunState::Hidden;
            return false;
        }
        if self.state == GunState::ShiftingTrails {
            if self.timer == 0.0 {
                self.heading = c.bearing;
                self.traverse = 0.0;
                self.state = GunState::Engaging;
                self.timer = LAY_S * hands;
            }
            return false;
        }
        let want = wrap(c.bearing - self.heading);
        if want.abs() > TRAVERSE_LIMIT {
            self.state = GunState::ShiftingTrails;
            self.timer = SHIFT_TRAILS_S * hands;
            return false;
        }
        self.traverse = want;
        if self.state != GunState::Engaging {
            self.state = GunState::Engaging;
            // Reacquisition must not shorten a reload that started before LOS was lost.
            self.timer = self.timer.max(LAY_S * hands);
            return false;
        }
        if self.timer > 0.0 { return false; }
        self.timer = AIMED_INTERVAL_S * hands;
        self.shots += 1;
        true
    }
}

/// What the gun commander sees of the Tiger this frame.
#[derive(Clone, Copy, Debug)]
pub(super) struct Contact {
    pub range_m: f32,
    /// World bearing (yaw, same convention as `Tank::heading`) from the gun to the Tiger.
    pub bearing: f32,
    /// Line of sight within the weather's reach.
    pub visible: bool,
    /// The fight has started: a shot heard or a sighting reported.
    pub alerted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Round { Ap, Apcr }

/// Round and aim point (Tiger hull-local) for one shot. `toward_gun` is the direction from the
/// Tiger to the gun in the Tiger's hull frame. Flank: the lower side over the fuel tanks (where
/// the wheels usually take it). Front: sprocket, driver's visor, sprocket, ball mount in turn.
/// Rear: the rear plate and idlers. Tungsten only for close flank or rear shots.
pub(super) fn choose_shot(toward_gun: Vec3, range_m: f32, apcr_left: u8, shot: u32) -> (Round, Vec3) {
    let d = Vec3::new(toward_gun.x, 0.0, toward_gun.z).normalize_or_zero();
    let side = if d.x >= 0.0 { 1.0 } else { -1.0 };
    let (flank, aim) = if d.x.abs() > 0.7 {
        (true, Vec3::new(side * 2.06, 0.75, -1.4))
    } else if d.z > 0.7 {
        (false, match shot % 4 {
            0 => Vec3::new(1.9, 0.7, 3.4),
            1 => Vec3::new(0.535, 1.74, 3.56),
            2 => Vec3::new(-1.9, 0.7, 3.4),
            _ => Vec3::new(-0.547, 1.78, 3.56),
        })
    } else if d.z < -0.7 {
        (true, Vec3::new(if shot % 2 == 0 { 1.4 } else { -1.4 }, 0.8, -3.56))
    } else {
        // Quartering: the track run on the side it can see.
        (false, Vec3::new(side * 2.06, 0.5, d.z.signum() * 1.5))
    };
    let tungsten = apcr_left > 0 && flank && range_m <= APCR_LOWER_M;
    (if tungsten { Round::Apcr } else { Round::Ap }, aim)
}

/// Aiming error of the crew (radians, one sigma): a steady gunner against a stationary tank, worse
/// against a moving one and short-handed. Tuning toward the doctrine's 20–25% hits at 500 m on a
/// moving tank.
pub(super) fn dispersion(tiger_speed_frac: f32, crew: usize, riflemen: bool) -> f32 {
    0.0012 * (1.0 + 1.5 * tiger_speed_frac.clamp(0.0, 1.0)) * if crew >= 3 { 1.0 } else { 1.5 }
        * if riflemen { RIFLEMAN_DISPERSION } else { 1.0 }
}

/// Whether one artilleryman is actually serving his piece: alive, not routed, and within
/// the same breech reach `at_gun_riflemen` requires of replacement riflemen.
pub(super) fn artilleryman_serving(alive: bool, fleeing: bool, distance_to_gun_m: f32) -> bool {
    alive && !fleeing && distance_to_gun_m < 3.0
}

pub(super) fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Yaw (Tank::heading convention: forward = (sin, cos)) from `from` toward `to`.
pub(super) fn bearing(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    d.x.atan2(d.z)
}

// ── The gun: a low-poly 53-K from its published dimensions (barrel 2.07 m, 46 calibres; wheel
// track about 1.2 m; shield about 1.2 m wide), in the game's flat-shaded style. Winter whitewash
// over the green is a field practice for tanks; for guns it is assumed (no order found).
const WHEEL_R: f32 = 0.40;
const BORE_Y: f32 = 0.72;
const BARREL_END: f32 = 2.07 - 0.35;

/// Collision box of shield and carriage in the upper-carriage frame (the thin barrel and trails
/// are not modelled as targets).
pub(super) const HIT_MIN: Vec3 = Vec3::new(-0.65, 0.0, -0.55);
pub(super) const HIT_MAX: Vec3 = Vec3::new(0.65, 1.2, 0.25);

fn merge(parts: Vec<CpuMesh>) -> CpuMesh {
    let mut parts = parts.into_iter();
    let mut out = parts.next().expect("at least one part");
    for part in parts {
        let base = out.vertices.len() as u32;
        out.vertices.extend(part.vertices);
        out.indices.extend(part.indices.into_iter().map(|i| i + base));
        out.min = out.min.min(part.min);
        out.max = out.max.max(part.max);
    }
    out.metallic_factor = 0.35;
    out.roughness_factor = 0.85;
    out
}

/// Wheels, axle and the split trails: the part that stays put while the gun traverses.
pub(super) fn lower_parts() -> Vec<CpuMesh> {
    const WASH: [u8; 3] = [178, 182, 176];
    const TYRE: [u8; 3] = [30, 30, 30];
    let mut parts = Vec::new();
    for x in [-0.62f32, 0.62] {
        parts.push(box_mesh(Vec3::new(x - 0.06, 0.0, -WHEEL_R), Vec3::new(x + 0.06, 2.0 * WHEEL_R, WHEEL_R), TYRE));
    }
    parts.push(box_mesh(Vec3::new(-0.62, WHEEL_R - 0.04, -0.04), Vec3::new(0.62, WHEEL_R + 0.04, 0.04), [70, 76, 64]));
    for side in [-1.0f32, 1.0] {
        let trail = box_mesh(Vec3::new(-0.06, -0.05, -2.0), Vec3::new(0.06, 0.05, 0.0), WASH)
            .transformed(Mat4::from_translation(Vec3::new(side * 0.18, WHEEL_R - 0.02, -0.1))
                * Mat4::from_rotation_y(-side * 0.21) * Mat4::from_rotation_x(-0.17));
        parts.push(trail);
    }
    parts
}
pub(super) fn lower_mesh()->CpuMesh {merge(lower_parts())}

/// Shield, cradle and barrel: the part that traverses.
pub(super) fn upper_parts() -> Vec<CpuMesh> {
    const WASH: [u8; 3] = [186, 190, 184];
    const STEEL: [u8; 3] = [66, 72, 62];
    vec![
        box_mesh(Vec3::new(-0.60, 0.30, 0.15), Vec3::new(0.60, 1.18, 0.19), WASH),
        box_mesh(Vec3::new(-0.10, BORE_Y - 0.10, -0.85), Vec3::new(0.10, BORE_Y + 0.08, 0.20), STEEL),
        box_mesh(Vec3::new(-0.035, BORE_Y - 0.035, 0.20), Vec3::new(0.035, BORE_Y + 0.035, BARREL_END), STEEL),
    ]
}
pub(super) fn upper_mesh()->CpuMesh {merge(upper_parts())}

impl Game {
    /// Site the guns (at most `AT_GUNS`) and put their crews at them.
    pub(super) fn field_at_guns(&mut self, sites: &[(Vec2, f32)]) {
        debug_assert!(sites.len() <= AT_GUNS);
        for (g, &(site, heading)) in sites.iter().enumerate().take(AT_GUNS) {
            let pos = Vec3::new(site.x, self.hf.height_at(site.x, site.y), site.y);
            self.at_guns[g] = Some(AtGun::new(pos, heading));
            let carriage = self.at_guns[g].unwrap().carriage();
            // Two beside the breech behind the shield, two at the trails with the ammunition.
            let spots = [Vec3::new(0.55, 0.0, -0.7), Vec3::new(-0.55, 0.0, -0.7),
                Vec3::new(0.95, 0.0, -1.6), Vec3::new(-0.95, 0.0, -1.8)];
            for (k, spot) in spots.into_iter().enumerate() {
                let index = FIRST_CREW_SOLDIER + g * CREW + k;
                let Some(soldier) = self.soldiers.get_mut(index) else { continue; };
                let p = carriage.transform_point3(spot);
                let p = Vec3::new(p.x, self.hf.height_at(p.x, p.z), p.z);
                soldier.escort_enemy = None;
                soldier.activate_escort([p; 4], &self.hf);
            }
        }
        println!("[at-guns] {} x 45 mm 53-K sited, crews of {CREW}", sites.len().min(AT_GUNS));
    }

    fn at_gun_artillerymen(&self, g: usize) -> usize {
        // A gunner serves the piece only while he is alive, not routed, and at the gun.
        let Some(gun) = self.at_guns[g] else { return 0; };
        (0..CREW).filter(|&k| self.soldiers.get(FIRST_CREW_SOLDIER + g * CREW + k)
            .is_some_and(|s| artilleryman_serving(s.alive, s.is_fleeing(), s.pos.distance(gun.pos)))).count()
    }

    /// Riflemen alive and at the gun (within reach of the breech).
    fn at_gun_riflemen(&self, g: usize) -> usize {
        let Some(gun) = self.at_guns[g] else { return 0; };
        gun.riflemen.iter().flatten().filter(|&&i| self.soldiers.get(i)
            .is_some_and(|s| s.alive && s.pos.distance(gun.pos) < 3.0)).count()
    }

    fn at_gun_crew(&self, g: usize) -> usize {
        self.at_gun_artillerymen(g) + self.at_gun_riflemen(g)
    }

    fn is_gun_crew(&self, index: usize) -> bool {
        (0..AT_GUNS).any(|g| self.at_guns[g].is_some_and(|gun| gun.state != GunState::Wrecked)
            && (FIRST_CREW_SOLDIER + g * CREW..FIRST_CREW_SOLDIER + (g + 1) * CREW).contains(&index))
            || self.at_guns.iter().flatten().any(|gun| gun.state != GunState::Wrecked && gun.riflemen.contains(&Some(index)))
    }

    /// The artillerymen are dead but the gun still stands: the nearest living riflemen run to it.
    fn recrew_at_gun(&mut self, g: usize) {
        let Some(gun) = self.at_guns[g] else { return; };
        if gun.state == GunState::Wrecked || self.at_gun_artillerymen(g) > 0 { return; }
        let mut riflemen = gun.riflemen;
        riflemen.iter_mut().for_each(|slot| {
            if slot.is_some_and(|i| !self.soldiers.get(i).is_some_and(|s| s.alive && !s.is_fleeing())) { *slot = None; }
        });
        for slot in 0..RIFLEMEN {
            if riflemen[slot].is_some() { continue; }
            let taken: Vec<usize> = riflemen.iter().flatten().copied().collect();
            // Only Soviet infantry re-crew a Soviet gun; allied soldiers follow in the same pool.
            let nearest = (0..self.soviet_soldier_count.min(self.soldiers.len()))
                .filter(|&i| !self.is_gun_crew(i) && !taken.contains(&i))
                .filter(|&i| self.soldiers[i].alive && !self.soldiers[i].is_fleeing() && self.soldiers[i].pos.distance(gun.pos) <= RECREW_REACH_M)
                .min_by(|&a, &b| self.soldiers[a].pos.distance(gun.pos).total_cmp(&self.soldiers[b].pos.distance(gun.pos)));
            let Some(index) = nearest else { break; };
            let spot = gun.carriage().transform_point3(Vec3::new(if slot == 0 { 0.55 } else { -0.55 }, 0.0, -0.7));
            let spot = Vec3::new(spot.x, self.hf.height_at(spot.x, spot.z), spot.z);
            let soldier = &mut self.soldiers[index];
            soldier.escort_enemy = None;
            // Walk to the gun: the route's first waypoint is where he goes.
            soldier.set_escort_route([spot; 4]);
            riflemen[slot] = Some(index);
            println!("[at-guns] rifleman #{index} runs to gun {g}");
        }
        if let Some(gun) = self.at_guns[g].as_mut() {
            gun.riflemen = riflemen;
            // A gun silenced by its dead crew can be served again.
            if gun.state == GunState::Silenced && riflemen.iter().any(Option::is_some) {
                gun.state = GunState::Hidden;
            }
        }
    }

    /// The guns watch, lay, fire and take cover. Runs with the enemy AI.
    pub(super) fn update_at_guns(&mut self, dt: f32) {
        let alerted = self.stealth.platoons.iter().any(|p| matches!(p.state, AlertState::Engaged | AlertState::Searching) || p.pending)
            || self.winter_encounter.as_ref().is_some_and(|e| e.sounds_heard > 0 || e.confirmed_sightings > 0);
        // Base target recognition envelope stays finite; local optical weather supplies contrast.
        let reach = 1000.0;
        let tiger = self.player.center();
        for g in 0..AT_GUNS {
            self.recrew_at_gun(g);
            let Some(mut gun) = self.at_guns[g] else { continue; };
            let crew = self.at_gun_crew(g);
            let eye = gun.pos + Vec3::Y * 1.1;
            let range_m = eye.distance(tiger);
            let contact = self.player.alive.then(|| Contact {
                range_m,
                bearing: bearing(gun.pos, tiger),
                visible: range_m <= reach && self.optical_los_clear(eye, tiger),
                alerted: alerted || gun.heard,
            });
            let fired = gun.step(contact, crew, dt);
            self.at_guns[g] = Some(gun);
            if fired && self.enemy_return_fire_allowed() {
                self.at_gun_fire(g, crew);
            }
        }
    }

    /// A real report within earshot tells a crew the fight has started. Obstruction attenuates it
    /// as it does for the tank crews.
    pub(super) fn at_guns_hear(&mut self, origin: Vec3, range: f32) {
        for g in 0..AT_GUNS {
            let Some(gun) = self.at_guns[g] else { continue; };
            let ear = gun.pos + Vec3::Y * 1.1;
            let reach = range * if self.los_clear(ear, origin) { 1.0 } else { 0.62 };
            if ear.distance(origin) <= reach {
                if let Some(gun) = self.at_guns[g].as_mut() { gun.heard = true; }
            }
        }
    }

    fn at_gun_fire(&mut self, g: usize, crew: usize) {
        let Some(mut gun) = self.at_guns[g] else { return; };
        let (muzzle, _) = gun.muzzle();
        let hull = self.player.hull_model();
        let toward_gun = hull.inverse().transform_vector3(gun.pos - self.player.pos);
        let range_m = muzzle.distance(self.player.center());
        let (round, aim_local) = choose_shot(toward_gun, range_m, gun.apcr, gun.shots);
        let shell = match round {
            Round::Ap => &crate::ballistics::BR_240,
            Round::Apcr => { gun.apcr -= 1; &crate::ballistics::BR_240P }
        };
        // Lead a moving Tiger by the flight time, then allow for drop.
        let aim = hull.transform_point3(aim_local)
            + self.player.forward() * self.player.speed * (range_m / shell.muzzle_velocity);
        let laid = (ballistic_aim_target(muzzle, aim, shell) - muzzle).normalize_or_zero();
        self.ai_tick = self.ai_tick.wrapping_add(1);
        let seed = self.ai_tick ^ (g as u32 + 1).wrapping_mul(0x51ED_270B) ^ gun.shots.rotate_left(9);
        let untrained = self.at_gun_artillerymen(g) == 0;
        let sigma = dispersion(self.player.speed.abs() / tank::TOP_SPEED, crew, untrained);
        let jitter = |salt: u32| tiger_damage::roll(seed, salt) * 2.0 - 1.0;
        let across = laid.cross(Vec3::Y).normalize_or_zero();
        let direction = (laid + across * jitter(1) * sigma + Vec3::Y * jitter(2) * sigma).normalize_or_zero();
        self.at_guns[g] = Some(gun);
        let id = self.spawn_authoritative_projectile(ProjectileShooter::AtGun(g), shell, muzzle, direction, seed);
        self.present_external_projectile(id, muzzle, direction);
        // The 45 mm's muzzle blast from its 0.36 kg charge (SL §1 row 6), directivity about the
        // laid axis; the near-field report is filtered by its propagation on arrival.
        let report = crate::acoustics::synth_gun_report(shell.caliber_mm, 0.0, 0.0, seed.wrapping_mul(977));
        let charge = crate::sound_levels::propellant_charge_kg(shell.caliber_mm, shell.mass_kg,
            shell.muzzle_velocity);
        let braked = crate::sound_levels::gun_is_braked(shell.caliber_mm, shell.muzzle_velocity);
        self.schedule_emission(crate::audio::Sfx::EnemyGun, muzzle,
            crate::sound_propagation::Emission::gun(charge, braked, 90.0),
            Some((direction, braked)), Some(report));
        // Its platoon's other gun hears it open fire (a 45 mm report carries well past 1 km).
        self.at_guns_hear(muzzle, 1500.0);
        println!("[at-guns] gun {g} fires {} at {:.0} m ({:?})", shell.name, range_m, round);
    }

    /// Source triangles in the same intact/wrecked models passed to the renderer. No live-gun
    /// filter: wrecks remain solid, and the ordinary nearest-contact query still compares crew.
    pub(super) fn at_gun_contact(&self,start:Vec3,end:Vec3,passed:Option<(usize,u8)>)->Option<ProjectileContact> {
        self.at_guns.iter().enumerate().filter_map(|(index,gun)|
            gun.and_then(|gun|at_gun_ballistics::source_contact(index,&gun,start,end,passed)))
            .min_by(|a,b|a.t_m.total_cmp(&b.t_m))
    }

    /// A cannon round or a close high-explosive burst wrecks the gun.
    pub(super) fn wreck_at_gun(&mut self, g: usize, cause: &str) {
        let Some(gun) = self.at_guns.get(g).copied().flatten() else { return; };
        if gun.state == GunState::Wrecked { return; }
        self.at_guns[g].as_mut().unwrap().state = GunState::Wrecked;
        // Only soldiers actually assigned to this destroyed weapon know to abandon it.
        for index in (FIRST_CREW_SOLDIER + g * CREW..FIRST_CREW_SOLDIER + (g + 1) * CREW)
            .chain(gun.riflemen.iter().flatten().copied()) {
            if let Some(soldier) = self.soldiers.get_mut(index) {
                soldier.begin_flee_from(gun.pos);
            }
        }
        println!("[at-guns] gun {g} wrecked by {cause}; surviving crew abandon position");
    }

    /// High explosive bursting this close wrecks the gun (tuning; its crew take the blast as
    /// ordinary infantry).
    pub(super) fn at_gun_blast(&mut self, center: Vec3, filler_g: f32) {
        let reach = 1.5 + (filler_g / 1000.0).sqrt() * 3.0;
        for g in 0..AT_GUNS {
            if self.at_guns[g].is_some_and(|gun| gun.state != GunState::Wrecked && (gun.pos + Vec3::Y * 0.6).distance(center) <= reach) {
                self.wreck_at_gun(g, "high explosive");
            }
        }
    }

    pub(super) fn drive_at_guns(&self, r: &mut Renderer) {
        for g in 0..AT_GUNS {
            let [lower, upper] = self.slots.at_gun[g];
            let Some(gun) = self.at_guns[g] else {
                r.set_visible(lower, false);
                r.set_visible(upper, false);
                continue;
            };
            r.set_visible(lower, true);
            r.set_visible(upper, true);
            r.set_model(lower, gun.carriage());
            // A wrecked gun's upper carriage is knocked off its pivot and down onto the trails.
            let upper_model = gun.visible_upper();
            r.set_model(upper, upper_model);
        }
    }

    /// Film fixture (disclosed in its setup): field the winter encounter, withdraw the tank
    /// platoons so only the guns act, withhold the tungsten so the clip shows the ordinary round,
    /// and put the Tiger broadside in gun A's sector `range_m` out, with the fight already started.
    pub fn debug_stage_at_gun_review(&mut self, range_m: f32, lay_on_gun: bool) -> anyhow::Result<serde_json::Value> {
        if self.at_guns[0].is_none() { self.enable_live_playtest_encounter(); }
        let gun = self.at_guns[0].ok_or_else(|| anyhow::anyhow!("the winter encounter fields no guns here"))?;
        for enemy in self.enemies.iter_mut() { enemy.alive = false; }
        for gun in self.at_guns.iter_mut().flatten() { gun.apcr = 0; gun.heard = true; }
        // A spot about `range_m` out in its sector where gun and Tiger have a clear line both ways
        // (the gun's eye to the Tiger's lower hull, the Tiger's gun to the shield); straight ahead
        // if the terrain offers none. In the streets: a street point of the gun's own field of fire.
        let eye = gun.pos + Vec3::Y * 1.1;
        let ground = |p: Vec3| Vec3::new(p.x, self.hf.height_at(p.x, p.z), p.z);
        let street = self.street_at_gun_stage(0, range_m);
        let spot = street.or_else(|| (0..9).flat_map(|a| (0..5).map(move |r| (a, r))).find_map(|(a, r)| {
            let yaw = gun.heading + [0.0, 0.12, -0.12, 0.24, -0.24, 0.36, -0.36, 0.48, -0.48][a];
            let p = ground(gun.pos + Vec3::new(yaw.sin(), 0.0, yaw.cos()) * (range_m + [0.0, -20.0, 20.0, -40.0, 40.0][r]));
            (self.los_clear(eye, p + Vec3::Y * 0.75) && self.los_clear(p + Vec3::Y * 2.4, gun.pos + Vec3::Y * 0.7))
                .then_some(p)
        })).unwrap_or_else(|| ground(gun.pos + Vec3::new(gun.heading.sin(), 0.0, gun.heading.cos()) * range_m));
        self.player.pos = spot;
        self.player.heading = bearing(spot, gun.pos) + std::f32::consts::FRAC_PI_2;
        self.player.speed = 0.0;
        if lay_on_gun {
            // The Tiger's gun laid on gun A's shield, allowing for drop as any gunner does, at its
            // own rates completed at once for the fixture.
            let shield = gun.pos + Vec3::Y * 0.7;
            let aim = ballistic_aim_target(self.player.muzzle().0, shield, &crate::ballistics::PZGR_39);
            self.player.ai_aim(aim, 1000.0);
            self.lay_yaw = self.player.turret_yaw;
            self.lay_pitch = self.player.gun_pitch;
        }
        Ok(serde_json::json!({"gun": 0, "range_m": range_m, "tank_platoons": "withdrawn",
            "tungsten": "withheld", "fight_started": true, "tiger_aspect": "broadside to the gun",
            "tiger_gun_laid_on_gun_a": lay_on_gun,
            "tiger_at": [spot.x, spot.z], "tiger_range_m": spot.distance(gun.pos),
            "staging": if street.is_some() { "a street point of gun A's field of fire (street encounter)" }
                else { "open ground in gun A's sector" },
            "gun_a": {"at": [gun.pos.x, gun.pos.z], "heading_deg": gun.heading.to_degrees()},
            "street_siting": self.street_siting.as_ref().map(|s| s.receipt.clone())}))
    }

    /// Turn the operator's head toward a gun (head rotation only).
    pub fn debug_face_at_gun(&mut self, g: usize) -> bool {
        let Some(gun) = self.at_guns.get(g).copied().flatten() else { return false; };
        let point = gun.pos + Vec3::Y * 0.8;
        let eye = self.station_eye(self.station);
        let to = (point - eye).normalize_or_zero();
        let base = self.player.turret_forward();
        self.head_yaw = wrap(to.x.atan2(-to.z) - base.x.atan2(-base.z));
        self.head_pitch = (to.y.clamp(-0.999, 0.999).asin() + 0.10).clamp(-0.5, 0.7);
        true
    }

    /// Debug/capture telemetry for the guns.
    pub fn at_gun_snapshot(&self) -> serde_json::Value {
        serde_json::json!((0..AT_GUNS).filter_map(|g| self.at_guns[g].map(|gun| serde_json::json!({
            "gun": g, "state": format!("{:?}", gun.state), "shots": gun.shots, "apcr_left": gun.apcr,
            "crew_alive": self.at_gun_artillerymen(g), "riflemen_at_gun": self.at_gun_riflemen(g),
            "position": gun.pos.to_array(),
        }))).collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(range_m: f32, bearing: f32, alerted: bool) -> Option<Contact> {
        Some(Contact { range_m, bearing, visible: true, alerted })
    }
    fn run(gun: &mut AtGun, contact: Option<Contact>, crew: usize, seconds: f32) -> u32 {
        let mut fired = 0;
        for _ in 0..(seconds / 0.05) as usize { fired += u32::from(gun.step(contact, crew, 0.05)); }
        fired
    }

    #[test]
    fn a_hidden_gun_holds_fire_until_the_fight_starts_or_the_tank_is_point_blank() {
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        assert_eq!(run(&mut gun, seen(350.0, 0.0, false), CREW, 60.0), 0, "no sudden opening at 350 m in a quiet sector");
        assert_eq!(gun.state, GunState::Hidden);
        assert!(run(&mut gun, seen(350.0, 0.0, true), CREW, 30.0) >= 3, "the fight has started: it fires");
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        assert!(run(&mut gun, seen(180.0, 0.2, false), CREW, 12.0) >= 1, "point blank it always fires");
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        assert_eq!(run(&mut gun, seen(450.0, 0.0, true), CREW, 30.0), 0, "beyond 400 m it waits");
        assert_eq!(run(&mut gun, seen(300.0, 2.2, true), CREW, 30.0), 0, "outside its sector it waits");
    }

    #[test]
    fn brief_occlusion_keeps_laying_without_tracking_or_firing_through_cover() {
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        assert!(!gun.step(seen(300.0, 0.1, true), CREW, 0.05));
        let original_heading = gun.heading;
        let original_traverse = gun.traverse;
        // An adversarial invisible contact changes hidden bearing/range every gap.
        let hidden = Some(Contact {range_m: 2.0, bearing: 2.8, visible: false, alerted: true});
        for _ in 0..30 {
            assert!(!gun.step(hidden, CREW, 0.05));
            assert_eq!(gun.heading, original_heading);
            assert_eq!(gun.traverse, original_traverse);
            let fired = gun.step(seen(300.0, 0.1, true), CREW, 0.05);
            if fired { assert_eq!(gun.shots, 1); return; }
        }
        // Float countdown tolerance: one more real visible update can cross the laying boundary.
        assert!(gun.step(seen(300.0, 0.1, true), CREW, 0.05));
        assert_eq!(gun.shots, 1);
    }

    #[test]
    fn long_occlusion_requires_relaying_and_cannot_accelerate_reload() {
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        assert_eq!(run(&mut gun, seen(300.0, 0.1, true), CREW, 3.2), 1);
        assert_eq!(run(&mut gun, None, CREW, 2.0), 0);
        assert_eq!(gun.state, GunState::Hidden);
        let reload_remaining = gun.timer;
        assert!(reload_remaining > LAY_S);
        assert!(!gun.step(seen(300.0, 0.1, true), CREW, 0.05));
        assert!((gun.timer - (reload_remaining - 0.05)).abs() < 1e-5);
        assert_eq!(run(&mut gun, seen(300.0, 0.1, true), CREW, 3.1), 0);
        assert_eq!(run(&mut gun, None, CREW, 20.0), 0);
        assert!(!gun.step(seen(300.0, 0.1, true), CREW, 0.05));
        assert!((gun.timer - LAY_S).abs() < 1e-5);
        assert_eq!(run(&mut gun, seen(300.0, 0.1, true), CREW, 2.5), 0);
        assert_eq!(run(&mut gun, seen(300.0, 0.1, true), CREW, 0.7), 1);
    }

    #[test]
    fn aimed_fire_is_about_seven_rounds_a_minute_and_slower_short_handed() {
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        let full = run(&mut gun, seen(300.0, 0.1, true), CREW, 60.0);
        assert!((6..=8).contains(&full), "{full}");
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        let short = run(&mut gun, seen(300.0, 0.1, true), 2, 60.0);
        assert!(short < full && short >= 3, "{short}");
    }

    #[test]
    fn beyond_its_traverse_the_crew_hauls_the_trails_round_before_firing() {
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        let target = seen(250.0, 0.9, true);
        assert_eq!(run(&mut gun, target, CREW, SHIFT_TRAILS_S - 0.5), 0);
        assert_eq!(gun.state, GunState::ShiftingTrails);
        assert!(run(&mut gun, target, CREW, 12.0) >= 1);
        assert!((gun.heading - 0.9).abs() < 1e-5 && gun.traverse.abs() < 1e-5);
    }

    #[test]
    fn crews_go_to_ground_when_the_tank_closes_and_re_man_after_it_passes() {
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        run(&mut gun, seen(300.0, 0.0, true), CREW, 10.0);
        assert_eq!(run(&mut gun, seen(80.0, 0.0, true), CREW, 10.0), 0);
        assert_eq!(gun.state, GunState::Down);
        assert_eq!(run(&mut gun, seen(130.0, 0.0, true), CREW, 30.0), 0, "still too close to re-man");
        assert!(run(&mut gun, seen(190.0, 0.0, true), CREW, 20.0) >= 1, "past it: back on the gun");
    }

    #[test]
    fn a_dead_crew_silences_the_gun_until_riflemen_serve_it_worse() {
        let mut gun = AtGun::new(Vec3::ZERO, 0.0);
        assert_eq!(run(&mut gun, seen(250.0, 0.0, true), 0, 30.0), 0);
        assert_eq!(gun.state, GunState::Silenced);
        // Two riflemen reach it: it fires again, slower than a full crew.
        let served = run(&mut gun, seen(250.0, 0.0, true), 2, 60.0);
        assert!((3..6).contains(&served), "{served}");
        assert!(dispersion(0.0, 2, true) > dispersion(0.0, 2, false) * 1.5);
    }

    #[test]
    fn tungsten_only_for_close_flank_or_rear_shots_and_the_aim_follows_the_aspect() {
        let flank = Vec3::X;
        let front = Vec3::Z;
        assert_eq!(choose_shot(flank, 300.0, 2, 0).0, Round::Apcr);
        assert_eq!(choose_shot(flank, 380.0, 2, 0).0, Round::Ap, "beyond 350 m the ordinary round");
        assert_eq!(choose_shot(flank, 300.0, 0, 0).0, Round::Ap, "none left");
        assert_eq!(choose_shot(front, 150.0, 2, 0).0, Round::Ap, "never wasted on the front");
        assert_eq!(choose_shot(flank, 300.0, 2, 0).1, Vec3::new(2.06, 0.75, -1.4), "the lower side facing the gun");
        let aims: Vec<_> = (0..4).map(|shot| choose_shot(front, 300.0, 2, shot).1).collect();
        assert!(aims.contains(&Vec3::new(0.535, 1.74, 3.56)) && aims.contains(&Vec3::new(1.9, 0.7, 3.4)),
            "sprocket and vision devices from the front");
        assert_eq!(choose_shot(-Vec3::Z, 150.0, 1, 0).0, Round::Apcr);
    }
}
