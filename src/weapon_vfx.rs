//! Bounded raster-only weapon presentation. No collision bodies or damage decisions live here.
//! Runtime small arms and cannon trails sample authoritative projectile states. The legacy
//! endpoint-flight helper remains available only to isolated presentation diagnostics.
use super::*;

pub(super) const PAGES: usize = 9;
const FLIGHTS: usize = 160;
const BURSTS: usize = 48;
const GRAINS: usize = 3072;
const FRAME_GRAINS: usize = 24_576;
const CANNON_DRAWS: usize = 128;
pub(super) const PAGE_CAPACITY: usize = FRAME_GRAINS + FLIGHTS * 2 + CANNON_DRAWS * 2;
const COLORS: [[u8; 3]; PAGES] = [
    [255, 226, 96], [255, 181, 48], [255, 146, 28], [255, 249, 215],
    [255, 202, 62], [255, 191, 52], [142, 143, 136], [76, 79, 82],
    [228, 231, 234], // pale propellant aerosol, separate from blast soot and ground dust
];
const EMISSION: [f32; PAGES] = [4.0, 4.5, 5.0, 9.0, 4.2, 5.2, 0.0, 0.0, 0.0];

/// Authored discharge kinematics: forward momentum dominates early, then the plume
/// entrains a little wind. The barrel direction is captured at discharge, not at render time.
fn propellant_offset(axis: Vec3, ray: Vec3, age: f32, radius: f32, spread: f32, wind: Vec2) -> Vec3 {
    let axis = if axis.length_squared() > 1e-8 { axis.normalize() } else { Vec3::Z };
    let rotation = Quat::from_rotation_arc(Vec3::Z, axis);
    let expansion = 1.0 - (-age / 0.32).exp();
    let jet = 4.8 * (1.0 - (-age / 0.62).exp());
    let width = (0.22 + 0.78 * age.sqrt()) * expansion;
    let local = Vec3::new(ray.x * width, ray.y * width,
        jet * (0.55 + 0.45 * spread) + ray.z.abs() * expansion * 0.8);
    let wind_time = (age - 1.2 * (1.0 - (-age / 1.2).exp())).max(0.0);
    rotation * local * radius + Vec3::new(wind.x,0.0,wind.y) * wind_time * 0.12
        + Vec3::Y * age * age * 0.10
}

/// Artistic energy classes, shared by both factions. Rifle, MG and tank-gun tiers become
/// differ in size and amber warmth; this is presentation, not historical tracer composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Weapon { Pistol, Rifle, MachineGun, Cannon(u16), Rocket }
impl Weapon {
    fn page(self) -> usize {
        match self { Self::Pistol => 0, Self::Rifle => 4, Self::MachineGun => 1,
            Self::Cannon(mm) if mm < 80 => 2, Self::Cannon(_) | Self::Rocket => 5 }
    }
    fn width(self) -> f32 {
        match self { Self::Pistol => 0.025, Self::Rifle | Self::MachineGun => 0.035,
            Self::Cannon(mm) => (mm as f32 * 0.0015).clamp(0.07, 0.23), Self::Rocket => 0.12 }
    }
    fn length(self) -> f32 {
        match self { Self::Pistol => 1.6, Self::Rifle | Self::MachineGun => 3.4,
            Self::Cannon(mm) => (mm as f32 * 0.10).clamp(4.5, 13.0), Self::Rocket => 1.4 }
    }
    fn speed(self) -> f32 {
        match self { Self::Pistol => 350.0, Self::Rifle | Self::MachineGun => 760.0,
            Self::Cannon(_) => 800.0, Self::Rocket => 45.0 }
    }
    fn muzzle_size(self) -> f32 {
        match self { Self::Pistol => 0.10, Self::Rifle | Self::MachineGun => 0.18,
            Self::Cannon(mm) => (mm as f32 * 0.015).clamp(0.65, 2.6), Self::Rocket => 0.9 }
    }
}

#[derive(Clone, Copy)]
struct Flight { from: Vec3, to: Vec3, age: f32, life: f32, weapon: Weapon, armor_normal: Option<Vec3> }
#[derive(Clone, Copy)]
enum BurstKind { Muzzle, Blast, Impact, Dust, Ricochet(Vec3) }
#[derive(Clone, Copy)]
struct Burst { pos: Vec3, dir: Vec3, age: f32, life: f32, radius: f32,
    seed: u32, kind: BurstKind, weapon: Weapon }

pub(super) struct Effects {
    flights: [Option<Flight>; FLIGHTS], bursts: [Option<Burst>; BURSTS],
    next_flight: usize, next_burst: usize, sequence: u32,
    pixel_world_scale: f32,
    act_id: Option<u8>,
    wind: Vec2,
    mats: [Vec<Mat4>; PAGES],
}
impl Default for Effects {
    fn default() -> Self { Self {
        flights: [None; FLIGHTS], bursts: [None; BURSTS], next_flight: 0, next_burst: 0,
        sequence: 0, pixel_world_scale: 0.0036, act_id: None, wind: Vec2::ZERO,
        mats: std::array::from_fn(|_| Vec::with_capacity(PAGE_CAPACITY)),
    } }
}
fn random(seed: u32) -> f32 {
    let mut s = seed ^ (seed >> 16);
    s = s.wrapping_mul(0x7feb352d); s ^= s >> 15;
    s = s.wrapping_mul(0x846ca68b); s ^= s >> 16;
    (s >> 8) as f32 / 16777216.0
}
fn direction(seed: u32) -> Vec3 {
    let z = random(seed) * 2.0 - 1.0;
    let a = random(seed.wrapping_add(1)) * std::f32::consts::TAU;
    let r = (1.0 - z * z).sqrt();
    Vec3::new(r * a.cos(), z, r * a.sin())
}
impl Effects {
    fn burst(&mut self, pos: Vec3, dir: Vec3, radius: f32, life: f32,
        kind: BurstKind, weapon: Weapon) {
        if !pos.is_finite() || !dir.is_finite() || !radius.is_finite() { return; }
        self.sequence = self.sequence.wrapping_add(1);
        // Prefer expired slots. Under sustained load replace the oldest cosmetic emitter; never
        // grow a vector or borrow a physical body slot to accommodate another burst.
        let k = (0..BURSTS).map(|n| (self.next_burst + n) % BURSTS)
            .find(|&i| self.bursts[i].is_none()).unwrap_or(self.next_burst);
        self.bursts[k] = Some(Burst { pos, dir: dir.normalize_or_zero(), age: 0.0,
            life, radius: radius.clamp(0.02, 9.0), seed: self.sequence, kind, weapon });
        self.next_burst = (k + 1) % BURSTS;
    }
    pub(super) fn muzzle(&mut self, pos: Vec3, dir: Vec3, weapon: Weapon) {
        let life = match weapon { Weapon::Pistol => 0.65, Weapon::Rifle | Weapon::MachineGun => 0.95, _ => 3.4 };
        self.burst(pos, dir, weapon.muzzle_size(), life, BurstKind::Muzzle, weapon);
    }
    pub(super) fn blast(&mut self, pos: Vec3, radius: f32) {
        self.burst(pos, Vec3::Y, radius, 6.0, BurstKind::Blast, Weapon::Cannon(88));
    }
    pub(super) fn impact(&mut self, pos: Vec3, dir: Vec3, radius: f32, hot: bool, weapon: Weapon) {
        self.burst(pos, -dir, radius, 1.3,
            if hot { BurstKind::Impact } else { BurstKind::Dust }, weapon);
    }
    pub(super) fn armor_sparks(&mut self, point:Vec3, incoming:Vec3, normal:Vec3, weapon:Weapon) {
        let incoming=incoming.normalize_or_zero();
        let normal=normal.normalize_or_zero();
        let reflected=incoming-normal*(2.0*incoming.dot(normal));
        if matches!(weapon,Weapon::Cannon(_) | Weapon::Rocket) {
            self.burst(point+normal*0.03,normal,0.75,1.2,BurstKind::Impact,weapon);
        }
        self.burst(point+normal*0.008,reflected,0.24,if matches!(weapon,Weapon::Cannon(_) | Weapon::Rocket) {1.4}else{0.65},BurstKind::Ricochet(normal),weapon);
    }
    pub(super) fn small_arm(&mut self, from: Vec3, to: Vec3, weapon: Weapon) {
        self.small_arm_contact(from,to,weapon,None);
    }
    pub(super) fn small_arm_contact(&mut self, from: Vec3, to: Vec3, weapon: Weapon, armor_normal: Option<Vec3>) {
        if !from.is_finite() || !to.is_finite() || from.distance_squared(to) < 1e-6 { return; }
        let distance = from.distance(to);
        // Very close hits still get two visible frames. Never extend a streak through its endpoint.
        let life = (distance / weapon.speed()).clamp(0.055, 1.5);
        let armor_normal = armor_normal.filter(|n| n.is_finite() && n.length_squared()>0.5).map(Vec3::normalize);
        self.flights[self.next_flight] = Some(Flight { from, to, age: 0.0, life, weapon, armor_normal });
        self.next_flight = (self.next_flight + 1) % FLIGHTS;
        self.muzzle(from, (to-from).normalize(), weapon);
    }
    fn push(&mut self, page: usize, size: Vec3, pos: Vec3, rot: Quat) {
        if self.mats[page].len() < PAGE_CAPACITY && pos.is_finite() && size.is_finite() {
            self.mats[page].push(Mat4::from_scale_rotation_translation(size, rot, pos));
        }
    }
    /// A short luminous tail behind a moving head, clipped at the muzzle and terminal endpoint.
    fn streak(&mut self, head: Vec3, dir: Vec3, travelled: f32, weapon: Weapon, eye: Vec3) {
        let length = weapon.length().min(travelled.max(0.0));
        if length < 0.015 || dir.length_squared() < 0.5 { return; }
        // A one-native-pixel floor preserves a continuous beam at low raster resolutions. This is a
        // bounded visual width, not an enlarged projectile hitbox or distance-adaptive screen res.
        let width = weapon.width().max((head.distance(eye) * self.pixel_world_scale * 1.12).min(0.65));
        let center = head - dir * length * 0.5;
        let rot = Quat::from_rotation_arc(Vec3::Z, dir);
        self.push(weapon.page(), Vec3::new(width, width, length), center, rot);
        self.push(3, Vec3::new(width * 0.32, width * 0.32, length * 0.78), center, rot);
    }
    fn prepare(&mut self, dt: f32, eye: Vec3) {
        for page in &mut self.mats { page.clear(); }
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.10) } else { 0.0 };
        for i in 0..FLIGHTS {
            if let Some(mut flight) = self.flights[i] {
                flight.age += dt;
                if flight.age >= flight.life {
                    self.flights[i] = None;
                    let incoming = (flight.to-flight.from).normalize();
                    if let Some(normal) = flight.armor_normal {
                        let reflected = incoming - normal * (2.0*incoming.dot(normal));
                        self.burst(flight.to+normal*0.008,reflected,0.24,0.65,
                            BurstKind::Ricochet(normal),flight.weapon);
                    } else {
                        self.impact(flight.to,incoming,
                            if flight.weapon == Weapon::Pistol { 0.12 } else { 0.20 }, false, flight.weapon);
                    }
                    continue;
                }
                let fraction = flight.age / flight.life;
                let delta = flight.to - flight.from;
                self.streak(flight.from + delta * fraction, delta.normalize(),
                    delta.length() * fraction, flight.weapon, eye);
                self.flights[i] = Some(flight);
            }
        }
        // Spend the visual budget nearest-first so a far barrage cannot starve the player's own
        // muzzle. Fixed stack storage and analytic grains keep allocation and physics out of it.
        let mut order: [usize; BURSTS] = std::array::from_fn(|i| i);
        order.sort_unstable_by(|&a, &b| {
            let distance = |i: usize| self.bursts[i].map_or(f32::INFINITY,
                |burst| burst.pos.distance_squared(eye));
            distance(a).total_cmp(&distance(b))
        });
        let mut remaining = FRAME_GRAINS;
        for i in order {
            if let Some(mut burst) = self.bursts[i] {
                burst.age += dt;
                if burst.age >= burst.life { self.bursts[i] = None; continue; }
                let count = self.draw_burst(burst, eye, remaining);
                remaining -= count;
                self.bursts[i] = Some(burst);
            }
        }
    }
    fn draw_burst(&mut self, b: Burst, eye: Vec3, remaining: usize) -> usize {
        if let BurstKind::Ricochet(normal) = b.kind {
            let cannon=matches!(b.weapon,Weapon::Cannon(_) | Weapon::Rocket);
            let count = remaining.min(if cannon {192}else{40});
            for j in 0..count {
                let seed=b.seed.wrapping_mul(3948571).wrapping_add(j as u32*977);
                let rnd=random(seed);
                if b.age > if cannon {0.55+rnd*0.85}else{0.24 + rnd*0.40} { continue; }
                let mut direction=(b.dir*0.75+direction(seed.wrapping_add(19))*0.30+normal*0.25).normalize_or_zero();
                if direction.dot(normal)<0.05 { direction=(direction+normal*(0.06-direction.dot(normal))).normalize_or_zero(); }
                let speed=if cannon {14.0+rnd*30.0}else{5.0+rnd*15.0};
                let mut offset=direction*speed*b.age-Vec3::Y*(4.905*b.age*b.age);
                if offset.dot(normal)<0.003 { offset+=normal*(0.003-offset.dot(normal)); }
                let width=(if cannon {0.025+rnd*0.035}else{0.008+rnd*0.012})*(1.0-b.age/b.life).max(0.15);
                let page=if b.age<0.055 {3} else if b.age<0.16 {4} else {2};
                self.push(page,Vec3::new(width,width,(speed*0.020).clamp(0.04,0.80)),b.pos+offset,
                    Quat::from_rotation_arc(Vec3::Z,direction));
            }
            return count;
        }
        // Long-range incandescent ejecta: separate from the expanding flash and the slow soot.
        // Cosmetic trajectories only. Authoritative shell/armor damage is untouched.
        let mut spark_count=0;
        if matches!(b.kind,BurstKind::Blast | BurstKind::Impact) {
            let n=remaining.min(if matches!(b.kind,BurstKind::Blast){192}else{96});
            spark_count=n;
            for j in 0..n {
                let seed=b.seed.wrapping_mul(9348571).wrapping_add(j as u32*977);
                let rnd=random(seed); let life=0.45+rnd*1.4;
                if b.age>=life {continue;}
                let mut dir=direction(seed.wrapping_add(17));dir.y=dir.y.abs()*0.75+0.15;
                let velocity=dir.normalize()*(10.0+rnd*28.0);
                let age=b.age;let pos=b.pos+velocity*age-Vec3::Y*(4.905*age*age);
                let v=velocity-Vec3::Y*(9.81*age);let width=(0.02+rnd*0.04)*(1.0-age/life).sqrt();
                let page=if age<0.12 {3}else if age<0.55 {4}else{2};
                self.push(page,Vec3::new(width,width,v.length()*0.022),pos,Quat::from_rotation_arc(Vec3::Z,v.normalize()));
            }
        }
        let muzzle = matches!(b.kind, BurstKind::Muzzle);
        let blast = matches!(b.kind, BurstKind::Blast);
        let hot = !matches!(b.kind, BurstKind::Dust);
        let distance = eye.distance(b.pos).max(0.1);
        let detail = if distance < b.radius * 18.0 { GRAINS }
            else if distance < b.radius * 65.0 { 1024 } else { 256 };
        let desired = if blast { detail } else if muzzle && b.radius > 0.6 { detail.min(2048) }
            else if b.radius > 0.4 { detail.min(384) } else { 48 };
        let count = desired.min(remaining.saturating_sub(spark_count));
        // Coarsen distant silhouettes only as grain count decreases; close fire is composed of
        // thousands of fine voxels, while its bounded, less detailed far LOD remains recognizable.
        let grain_scale = (GRAINS as f32 / desired as f32).sqrt().clamp(1.0, 3.5);
        let flash_life = if muzzle { 0.11 } else if blast { 0.28 } else { 0.24 };
        let age = b.age;
        for j in 0..count {
            let seed = b.seed.wrapping_mul(3948571).wrapping_add(j as u32 * 977);
            let ray = direction(seed);
            let rnd = random(seed.wrapping_add(11));
            let delayed = if blast { (j % 3) as f32 * 0.065 } else { 0.0 };
            let t = age - delayed;
            if t < 0.0 { continue; }
            let f = (t / (b.life - delayed)).clamp(0.0, 1.0);
            let fade = (1.0 - f).sqrt();
            let radial = if muzzle {
                let rot = Quat::from_rotation_arc(Vec3::Z,
                    if b.dir.length_squared() > 0.5 { b.dir } else { Vec3::Z });
                rot * Vec3::new(ray.x * 0.34, ray.y * 0.34, 0.22 + ray.z.abs())
            } else { Vec3::new(ray.x, ray.y.abs() * 0.95, ray.z) };
            let expansion = if blast { 1.0 - (-t * 4.5).exp() } else { 1.0 - (-t * 5.0).exp() };
            let flame = hot && t < flash_life && j % 3 != 0;
            let wind = self.act_id.map(|act| wind_at(act,b.pos)).unwrap_or(Vec2::ZERO);
            let mut pos = b.pos + radial * b.radius * expansion * (0.25 + rnd * 0.9);
            if muzzle && !flame {
                pos = b.pos + propellant_offset(b.dir, ray, t, b.radius, rnd, wind);
            } else {
                pos += Vec3::new(wind.x,0.0,wind.y) * t * if blast {0.55} else {0.35};
                pos.y += (0.15 + rnd * 0.42) * t * b.radius;
            }
            let page = if flame {
                let phase = t / flash_life;
                if phase < 0.20 && j % 5 == 0 { 3 }
                else if muzzle { b.weapon.page() }
                else if phase < 0.48 { 4 } else { 2 }
            } else if muzzle { 8 } else if blast && j % 5 != 0 { 7 } else { 6 };
            // Separate small grains, not one scaled cube. The three delayed lobes expand and cool
            // into matte rising soot; a per-grain stagger dissolves the cloud without alpha sheets.
            if !flame { continue; } // Matter is integrated by the shared smoke volume pass.
            let size = b.radius * grain_scale * if flame { 0.019 + rnd * 0.025 }
                else { (0.016 + rnd * 0.025) * (0.65 + expansion * 0.8)
                    * if muzzle { 1.7 } else { 1.0 } } * fade;
            self.push(page, Vec3::splat(size.max(0.008)), pos,
                Quat::from_rotation_y(rnd * 1.57 + t * 0.18));
        }
        count+spark_count
    }
}

pub(super) fn allocate(device: &wgpu::Device, queue: &wgpu::Queue,
    renderer: &mut Renderer) -> [usize; PAGES] {
    std::array::from_fn(|i| {
        // Matte smoke grains use a small faceted lobe so their silhouettes do not read as stacks
        // of solid debris cubes. Fire and luminous tracer pages retain the pixel-like box shape.
        let mesh = if i >= 6 { crate::prims::faceted_sphere_mesh(COLORS[i]).transformed(Mat4::from_scale(Vec3::splat(0.5))) }
            else { box_mesh(Vec3::splat(-0.5),Vec3::splat(0.5),COLORS[i]) };
        let slot = renderer.add_dynamic_instanced_pool(device, queue,
            &mesh, PAGE_CAPACITY)
            .expect("bounded weapon particle page");
        // Smoke uses the ordinary matte sun/ambient response. The sustained cabin soot material
        // intentionally receives only weak hemisphere light; reusing it here crushed exterior
        // gun smoke into a black solid-looking blob. Neither smoke page emits or joins physics.
        renderer.set_batch_flags(slot, if i >= 6 { 0.0 } else { 2.0 });
        assert!(renderer.set_dynamic_instanced_emissive(slot, EMISSION[i]));
        slot
    })
}
impl Game {
    pub(super) fn drive_weapon_vfx(&mut self, dt: f32, r: &mut Renderer) {
        self.weapon_vfx.wind=r.weather_wind_world;
        self.weapon_vfx.act_id = Some(self.act.id);
        self.weapon_vfx.pixel_world_scale = 2.0 * (r.camera.fov_y * 0.5).tan() / r.ih.max(1) as f32;
        self.weapon_vfx.prepare(dt, r.camera.pos);
        let mut smoke: Vec<_>=self.weapon_vfx.bursts.iter().flatten().copied().collect();
        smoke.sort_by(|a,b|a.pos.distance_squared(r.camera.pos).total_cmp(&b.pos.distance_squared(r.camera.pos)));
        for b in smoke {
            if matches!(b.kind,BurstKind::Ricochet(_)){continue;}
            let age=b.age;let f=(age/b.life).clamp(0.0,1.0);
            let wind=self.weapon_vfx.wind;
            let muzzle=matches!(b.kind,BurstKind::Muzzle);
            let radius=if muzzle {b.radius*(0.22+age.sqrt()*0.70)}else{b.radius*(0.25+0.60*age.sqrt())};
            let center=if muzzle {b.pos+propellant_offset(b.dir,Vec3::ZERO,age,b.radius,0.5,wind)}
                else {b.pos+Vec3::new(wind.x*age*0.55,age*b.radius*0.32,wind.y*age*0.55)};
            let density=(1.0-f).powi(2)*(if muzzle {0.16}else{0.45})*smoothstep(0.0,0.08,age);
            for lobe in 0..3 {
                let phase=b.seed as f32*0.17+lobe as f32*2.1;
                let offset=Vec3::new(phase.sin(),0.3*phase.cos(),phase.cos())*radius*0.30;
                r.add_smoke_volume(center+offset,radius*0.76,density,age*0.8+phase,if muzzle {0.55}else{0.08});
            }
        }
        let (mut cannon_draws,mut bullet_draws)=(0,0);
        for shot in &self.projectiles {
            let weapon=if let Some(bullet)=shot.small_arm {
                if bullet_draws>=FLIGHTS { continue; }
                bullet_draws+=1;
                bullet.visual_weapon()
            } else {
                if cannon_draws>=CANNON_DRAWS { continue; }
                cannon_draws+=1;
                Weapon::Cannon(shot.shell.caliber_mm as u16)
            };
            self.weapon_vfx.streak(shot.pos,shot.velocity.normalize_or_zero(),shot.distance_m,weapon,r.camera.pos);
        }
        if self.panzerfaust_shot.active {
            let shot = &self.panzerfaust_shot;
            self.weapon_vfx.streak(shot.pos, shot.vel.normalize_or_zero(), shot.distance,
                Weapon::Rocket, r.camera.pos);
        }
        for (i, page) in self.slots.weapon_vfx.iter().copied().enumerate() {
            assert!(r.set_dynamic_instanced_mats(page, &self.weapon_vfx.mats[i]));
        }
        // Retain scene indices for old capture tools, but retire oversized primitive presentation.
        for slot in [self.slots.muzzle, self.slots.tracer, self.slots.impact,
            self.slots.mg_flash, self.slots.luger_impact].into_iter()
            .chain(self.slots.blast).chain(self.slots.e_tracer).chain(self.slots.e_impact)
            .chain(self.slots.mg_tracer).chain(self.slots.mg_impact)
            .chain(self.slots.inf_flash).chain(self.slots.luger_flash).chain(self.slots.panzerfaust_flash) {
            r.set_visible(slot, false);
        }
    }
    pub(super) fn clear_weapon_vfx_render(&self, r: &mut Renderer) {
        for page in self.slots.weapon_vfx { r.set_dynamic_instanced_mats(page, &[]); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn propellant_keeps_barrel_momentum_before_wind_entrainment() {
        for axis in [Vec3::Z, Vec3::X, -Vec3::Z] {
            let calm = propellant_offset(axis, Vec3::Y, 0.3, 1.0, 0.5, Vec2::ZERO);
            let gust = propellant_offset(axis, Vec3::Y, 0.3, 1.0, 0.5, Vec2::splat(8.0));
            assert!(calm.dot(axis) > 1.3);
            assert!((gust-calm).length() < 0.06);
            let late = propellant_offset(axis, Vec3::Y, 2.0, 1.0, 0.5, Vec2::splat(8.0));
            assert!(late.dot(axis) > 1.5);
        }
        assert!(propellant_offset(Vec3::ZERO,Vec3::Y,0.3,1.0,0.5,Vec2::ZERO).is_finite());
        let mut fx = Effects::default();
        fx.muzzle(Vec3::ZERO, Vec3::Z, Weapon::Cannon(88));
        for _ in 0..24 { fx.prepare(0.1, Vec3::ZERO); }
        assert!(fx.bursts.iter().flatten().any(|b|matches!(b.kind,BurstKind::Muzzle)), "propellant volume persists after the flash");
        assert!(fx.mats[..8].iter().all(Vec::is_empty));
        assert_eq!(EMISSION[8], 0.0);
    }
    #[test]
    fn armor_contact_emits_reflected_sparks_only_after_tracer_arrival() {
        let mut fx=Effects::default();
        let normal=Vec3::NEG_Z;
        fx.small_arm_contact(Vec3::ZERO,Vec3::Z*10.0,Weapon::MachineGun,Some(normal));
        fx.prepare(0.02,Vec3::ZERO);
        assert!(!fx.bursts.iter().flatten().any(|b|matches!(b.kind,BurstKind::Ricochet(_))));
        fx.prepare(0.05,Vec3::ZERO);
        let impact=fx.bursts.iter().flatten().find(|b|matches!(b.kind,BurstKind::Ricochet(_))).unwrap();
        assert!(impact.dir.dot(normal)>0.99);
        assert!(fx.mats[4].iter().all(|m|m.w_axis.z<10.0));
    }

    #[test]
    fn explosion_smoke_follows_world_wind_and_outlives_flash() {
        let mut still=Effects::default();let mut windy=Effects::default();windy.act_id=Some(1);
        let origin=Vec3::new(1880.0,6.0,165.0);
        still.blast(origin,3.0);windy.blast(origin,3.0);
        for _ in 0..40 {still.prepare(0.1,origin);windy.prepare(0.1,origin);}
        assert!(windy.bursts.iter().flatten().any(|b|matches!(b.kind,BurstKind::Blast)));
        assert!(windy.mats[6..].iter().all(Vec::is_empty), "smoke must not submit opaque matter meshes");
        assert!(windy.mats[..6].iter().all(Vec::is_empty));
    }
    #[test]
    #[ignore = "GPU source-Tiger effect review; run alone with KESSEL_RENDERER=shader"]
    fn weapon_vfx_gpu_source_tiger_review() -> anyhow::Result<()> {
        use crate::tank_editor_scene::{TankEditorScene, TankAssetId};
        let prefix = std::env::var("KESSEL_WEAPON_VFX_REVIEW_PREFIX")
            .unwrap_or_else(|_| "captures/astra/weapon_vfx_20260913_v1".into());
        anyhow::ensure!(!std::path::Path::new(&format!("{prefix}.json")).exists(), "choose a fresh review prefix");
        std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut r, mut g) = Game::build_tank_editor(&device, &queue,
            &TankEditorScene::new(TankAssetId::TigerI))?;
        anyhow::ensure!(r.shader_backend, "review requires shader backend");
        g.set_defer_rtt_to_render(true);
        for _ in 0..3 { g.update(&TankInput::default(), 1.0/60.0, &mut r); }
        let eye = Vec3::new(10.0, 4.5, -7.0);
        let target = Vec3::new(0.0, 2.0, 5.0);
        let set_camera = |r: &mut Renderer| {
            r.camera.pos=eye; r.camera.set_forward(target-eye);
            r.camera.fov_y=65f32.to_radians();r.reticle=false;r.optic_mask=0.0;
            r.interior_light=0.0;r.interior_haze=0.0;r.weather_shelter=0.0;
            r.set_interior_atmosphere(0.0,0.0,Vec3::ZERO);
        };
        set_camera(&mut r);
        for _ in 0..4 { r.render_to_rgba(&device,&queue,r.iw,r.ih)?; }
        let mut frames = Vec::new();
        let capture = |label: &str, g: &mut Game, r: &mut Renderer,
            frames: &mut Vec<serde_json::Value>| -> anyhow::Result<()> {
            set_camera(r); r.clear_smoke(); g.drive_weapon_vfx(0.0,r);
            let path=format!("{prefix}_{label}.png");
            r.ui_reset();r.draw_text("STAGED WEAPON VFX / SOURCE TIGER / RASTER ONLY",0.5,0.025,0.018,
                [1.0,0.95,0.8,1.0],crate::ui::Align::Center);
            r.render_to_png_hud(&device,&queue,&path,r.iw*3,r.ih*3)?;
            frames.push(serde_json::json!({"label":label,"path":path,
                "instances_per_page":g.weapon_vfx.mats.iter().map(Vec::len).collect::<Vec<_>>() }));
            Ok(())
        };
        if std::env::var_os("KESSEL_WEAPON_VFX_PALETTE_ONLY").is_some() {
            // Render-only comparison of the exact production streak material/geometry path.
            // No projectile or terminal event is fabricated by this diagnostic color chart.
            let palette_eye=Vec3::new(14.0,4.0,-9.0);
            r.camera.pos=palette_eye;r.camera.set_forward(Vec3::Z);
            r.camera.fov_y=45f32.to_radians();
            g.weapon_vfx=Effects::default();
            g.weapon_vfx.pixel_world_scale=2.0*(r.camera.fov_y*0.5).tan()/r.ih as f32;
            let tiers=[("PISTOL / YELLOW",Weapon::Pistol),("RIFLE / AMBER",Weapon::Rifle),
                ("MG / ORANGE",Weapon::MachineGun),("CANNON <80 MM / AMBER",Weapon::Cannon(76)),
                ("CANNON 88+ / ROCKET / GOLD",Weapon::Cannon(88))];
            r.ui_reset();
            r.draw_text("WEAPON ENERGY PALETTE / STAGED RASTER COMPARISON",0.5,0.025,0.018,
                [1.0,0.95,0.8,1.0],crate::ui::Align::Center);
            let mut tier_receipts=Vec::new();
            for (i,(label,weapon)) in tiers.into_iter().enumerate() {
                let y=6.4-i as f32*1.2;
                g.weapon_vfx.streak(Vec3::new(18.0,y,4.0),Vec3::X,20.0,weapon,palette_eye);
                let label_y=0.5-(y-4.0)/(26.0*(r.camera.fov_y*0.5).tan());
                r.draw_text(label,0.02,label_y-0.012,0.017,[1.0,0.95,0.8,1.0],crate::ui::Align::Left);
                tier_receipts.push(serde_json::json!({"label":label,"page":weapon.page(),
                    "rgb":COLORS[weapon.page()],"emission":EMISSION[weapon.page()]}));
            }
            for (i,page) in g.slots.weapon_vfx.into_iter().enumerate() {
                assert!(r.set_dynamic_instanced_mats(page,&g.weapon_vfx.mats[i]));
            }
            let path=format!("{prefix}_palette.png");
            r.render_to_png_hud(&device,&queue,&path,r.iw*3,r.ih*3)?;
            std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&serde_json::json!({
                "scope":"Staged color comparison using production streak geometry/materials. No shots or damage simulated.",
                "adapter":adapter.get_info().name,"shader_backend":r.shader_backend,
                "native_resolution":[r.iw,r.ih],"draw_pages":PAGES,"path":path,"tiers":tier_receipts,
            }))?)?;
            return Ok(());
        }
        if std::env::var_os("KESSEL_WEAPON_VFX_FILM").is_some() {
            use std::{io::Write,process::{Command,Stdio}};
            let path=format!("{prefix}.mp4");
            let mut encoder=Command::new("ffmpeg").args(["-v","error","-nostdin","-y","-f","rawvideo","-pixel_format","rgba","-video_size","1280x720","-framerate","30","-i","pipe:0","-an","-c:v","libx264","-preset","veryfast","-crf","18","-pix_fmt","yuv420p","-movflags","+faststart",&path]).stdin(Stdio::piped()).spawn()?;
            g.fire(&mut r);
            for frame in 0..180 {
                for _ in 0..2 {g.update(&TankInput::default(),1.0/60.0,&mut r);}
                if frame==24 {g.weapon_vfx.armor_sparks(Vec3::new(0.0,1.5,7.0),Vec3::Z,Vec3::NEG_Z,Weapon::Cannon(88));}
                if frame==48 {g.spawn_blast(Vec3::new(0.0,0.8,7.0),7.0,20.0,0.8);}
                set_camera(&mut r);r.clear_smoke();g.drive_weapon_vfx(0.0,&mut r);r.ui_reset();
                r.draw_text("STAGED EFFECT REVIEW / SOURCE TIGER / NO DAMAGE OUTCOME",0.5,0.025,0.016,[1.0,0.95,0.8,1.0],crate::ui::Align::Center);
                encoder.stdin.as_mut().unwrap().write_all(&r.render_to_rgba(&device,&queue,1280,720)?)?;
            }
            drop(encoder.stdin.take());anyhow::ensure!(encoder.wait()?.success(),"effect review encoder failed");
            std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&serde_json::json!({"scope":"Native production effects: real discharge, staged armor sparks and staged blast beside source Tiger for scale. Cosmetic timing; no battle outcome. No audio.","video":path,"seconds":6,"frames":180,"fps":30,"adapter":adapter.get_info().name}))?)?;
            return Ok(());
        }
        // The real main-gun discharge path, with the ordinary stable-ID ballistic flight.
        g.fire(&mut r);
        g.update(&TankInput::default(),1.0/60.0,&mut r);
        capture("cannon_flight",&mut g,&mut r,&mut frames)?;
        for _ in 0..29 {g.update(&TankInput::default(),1.0/60.0,&mut r);}
        capture("barrel_smoke",&mut g,&mut r,&mut frames)?;
        g.weapon_vfx=Effects::default();g.projectiles.clear();
        // Same authored game explosion emitter used by real kills and cookoffs. No tank damage is
        // forced by this presentation review; the receipt explicitly records this staged burst.
        g.spawn_blast(Vec3::new(0.0,0.8,7.0),7.0,20.0,0.8);
        for _ in 0..12 {g.weapon_vfx.prepare(1.0/60.0,eye);}
        capture("explosion",&mut g,&mut r,&mut frames)?;
        for _ in 0..27 {g.weapon_vfx.prepare(1.0/60.0,eye);}
        capture("explosion_ejecta",&mut g,&mut r,&mut frames)?;
        for _ in 0..72 {g.weapon_vfx.prepare(1.0/60.0,eye);}
        capture("explosion_soot",&mut g,&mut r,&mut frames)?;
        g.weapon_vfx=Effects::default();
        let direction=Vec3::new(0.0,-0.05,1.0).normalize();
        let pistol=Vec3::new(3.8,1.5,0.0);
        g.fire_luger(pistol,direction,Vec3::X,Vec3::Y,pistol);
        g.resolve_infantry_shot(0,crate::infantry::InfantryShot {
            from:Vec3::new(-3.8,1.5,0.0),dir:direction,range:45.0,
        });
        g.fire_mg_round(true);
        g.advance_projectiles(0.035,&mut r);
        g.weapon_vfx.prepare(0.035,eye);
        capture("small_arms",&mut g,&mut r,&mut frames)?;
        g.weapon_vfx=Effects::default();g.projectiles.clear();
        g.weapon_vfx.armor_sparks(Vec3::new(2.0,1.5,0.0),Vec3::new(-1.0,-0.1,0.5),Vec3::X,Weapon::Cannon(88));
        g.weapon_vfx.prepare(0.06,eye);
        capture("armor_sparks",&mut g,&mut r,&mut frames)?;
        std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"Source Tiger workshop diagnostic. Real main-gun/Luger/infantry/MG discharge hooks; separately staged cosmetic kill/cookoff emitter, no forced kill outcome. No audio audition.",
            "adapter":adapter.get_info().name,"shader_backend":r.shader_backend,
            "native_resolution":[r.iw,r.ih],"draw_pages":PAGES,"grain_frame_cap":FRAME_GRAINS,
            "burst_pool":BURSTS,"near_explosion_grains":GRAINS,"small_arm_flight_pool":FLIGHTS,
            "frames":frames,
        }))?)?;
        Ok(())
    }
    #[test]
    fn moving_small_arm_streak_never_crosses_muzzle_or_resolved_endpoint() {
        let mut fx = Effects::default();
        fx.small_arm(Vec3::ZERO, Vec3::Z * 8.0, Weapon::Pistol);
        let mut previous = 0.0;
        for _ in 0..4 {
            fx.prepare(0.01, Vec3::ZERO);
            let m = fx.mats[0][0];
            let (size, _, center) = m.to_scale_rotation_translation();
            assert!(center.z > previous);
            assert!(center.z - size.z * 0.5 >= -1e-5);
            assert!(center.z + size.z * 0.5 <= 8.00001);
            previous = center.z;
        }
        fx.prepare(0.10, Vec3::ZERO);
        assert!(fx.flights.iter().all(Option::is_none));
    }
    #[test]
    fn sustained_fire_and_explosions_have_fixed_memory_and_draw_work() {
        let mut fx = Effects::default();
        for _ in 0..2000 {
            fx.small_arm(Vec3::ZERO, Vec3::Z * 500.0, Weapon::MachineGun);
            fx.blast(Vec3::X * 3.0, 7.0);
        }
        let capacities: Vec<_> = fx.mats.iter().map(Vec::capacity).collect();
        for _ in 0..420 {
            fx.prepare(1.0 / 60.0, Vec3::Y);
            assert!(fx.mats.iter().all(|p| p.len() <= PAGE_CAPACITY));
            assert!(fx.mats.iter().map(Vec::len).sum::<usize>() <= FRAME_GRAINS + FLIGHTS * 2);
            assert!(fx.mats.iter().flatten().all(|m| m.is_finite()));
        }
        assert!(fx.mats.iter().all(Vec::is_empty));
        assert_eq!(capacities, fx.mats.iter().map(Vec::capacity).collect::<Vec<_>>());
    }
    #[test]
    fn explosions_expand_as_many_small_grains_then_cool_into_nonemissive_smoke() {
        let mut fx = Effects::default(); fx.blast(Vec3::ZERO, 5.0);
        fx.prepare(0.10, Vec3::ZERO);
        assert!(fx.mats[..6].iter().map(Vec::len).sum::<usize>() > 20);
        assert!(fx.mats.iter().flatten().all(|m| m.to_scale_rotation_translation().0.max_element() < 1.3));
        for _ in 0..20 { fx.prepare(0.10, Vec3::ZERO); }
        assert_eq!(fx.mats[..6].iter().map(Vec::len).sum::<usize>(), 0);
        assert!(fx.bursts.iter().flatten().any(|b|matches!(b.kind,BurstKind::Blast)));
        assert!(fx.mats[6..].iter().all(Vec::is_empty));
        assert_eq!(EMISSION[7], 0.0);
    }
    #[test]
    fn weapon_colors_follow_energy_not_faction_and_invalid_inputs_stay_finite() {
        assert_eq!(Weapon::Pistol.page(), 0);
        assert_eq!(Weapon::Rifle.page(), 4);
        assert_eq!(Weapon::MachineGun.page(), 1);
        assert_eq!(Weapon::Cannon(76).page(), 2);
        assert_eq!(Weapon::Cannon(88).page(), 5);
        assert_eq!(Weapon::Cannon(152).page(), 5);
        assert_eq!(Weapon::Rocket.page(), 5);
        let tiers=[Weapon::Pistol,Weapon::Rifle,Weapon::MachineGun,Weapon::Cannon(76),Weapon::Cannon(88)];
        let red_bias=tiers.map(|weapon| {
            let [r,g,b]=COLORS[weapon.page()];
            r as f32/(g as f32+b as f32)
        });
        assert!(red_bias.iter().all(|bias|*bias<1.6), "hot effects stay amber rather than saturated red");
        let mut fx = Effects::default();
        fx.small_arm(Vec3::NAN, Vec3::ZERO, Weapon::Pistol);
        fx.blast(Vec3::ZERO, f32::NAN);
        fx.prepare(f32::NAN, Vec3::ZERO);
        assert!(fx.mats.iter().all(Vec::is_empty));
    }
    #[test]
    fn small_arm_native_pixel_floor_prevents_broken_subpixel_dashes() {
        let mut fx=Effects::default();
        let eye=Vec3::ZERO;
        let head=Vec3::new(20.0,0.0,20.0);
        fx.streak(head,Vec3::Z,10.0,Weapon::Pistol,eye);
        let width=fx.mats[0][0].to_scale_rotation_translation().0.x;
        assert!(width / (head.distance(eye)*fx.pixel_world_scale) >= 1.1);
        assert!(width <= 0.65);
        assert_eq!(EMISSION[6],0.0); assert_eq!(EMISSION[7],0.0);
    }
}
