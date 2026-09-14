//! Bounded raster-only weapon presentation. No collision bodies or damage decisions live here.
//! Small arms retain their authoritative hitscan; the visible flight is cosmetic. Cannon trails
//! are sampled directly from live projectile states, including deflections and retirement.
use super::*;

pub(super) const PAGES: usize = 8;
const FLIGHTS: usize = 160;
const BURSTS: usize = 48;
const GRAINS: usize = 3072;
const FRAME_GRAINS: usize = 24_576;
const CANNON_DRAWS: usize = 128;
pub(super) const PAGE_CAPACITY: usize = FRAME_GRAINS + FLIGHTS * 2 + CANNON_DRAWS * 2;
const COLORS: [[u8; 3]; PAGES] = [
    [255, 211, 46], [255, 132, 24], [255, 64, 12], [255, 241, 185],
    [255, 171, 36], [255, 28, 8], [142, 143, 136], [49, 52, 54],
];
const EMISSION: [f32; PAGES] = [4.0, 4.5, 5.0, 9.0, 4.2, 5.2, 0.0, 0.0];

/// Artistic energy classes, shared by both factions. Rifle, MG and tank-gun tiers become
/// progressively redder; this is a readability cue, not historical tracer composition.
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
struct Flight { from: Vec3, to: Vec3, age: f32, life: f32, weapon: Weapon }
#[derive(Clone, Copy)]
enum BurstKind { Muzzle, Blast, Impact, Dust }
#[derive(Clone, Copy)]
struct Burst { pos: Vec3, dir: Vec3, age: f32, life: f32, radius: f32,
    seed: u32, kind: BurstKind, weapon: Weapon }

pub(super) struct Effects {
    flights: [Option<Flight>; FLIGHTS], bursts: [Option<Burst>; BURSTS],
    next_flight: usize, next_burst: usize, sequence: u32,
    pixel_world_scale: f32,
    mats: [Vec<Mat4>; PAGES],
}
impl Default for Effects {
    fn default() -> Self { Self {
        flights: [None; FLIGHTS], bursts: [None; BURSTS], next_flight: 0, next_burst: 0,
        sequence: 0, pixel_world_scale: 0.0036,
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
        self.burst(pos, dir, weapon.muzzle_size(), 2.1, BurstKind::Muzzle, weapon);
    }
    pub(super) fn blast(&mut self, pos: Vec3, radius: f32) {
        self.burst(pos, Vec3::Y, radius, 3.3, BurstKind::Blast, Weapon::Cannon(88));
    }
    pub(super) fn impact(&mut self, pos: Vec3, dir: Vec3, radius: f32, hot: bool, weapon: Weapon) {
        self.burst(pos, -dir, radius, 1.3,
            if hot { BurstKind::Impact } else { BurstKind::Dust }, weapon);
    }
    pub(super) fn small_arm(&mut self, from: Vec3, to: Vec3, weapon: Weapon) {
        if !from.is_finite() || !to.is_finite() || from.distance_squared(to) < 1e-6 { return; }
        let distance = from.distance(to);
        // Very close hits still get two visible frames. Never extend a streak through its endpoint.
        let life = (distance / weapon.speed()).clamp(0.055, 1.5);
        self.flights[self.next_flight] = Some(Flight { from, to, age: 0.0, life, weapon });
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
                    self.impact(flight.to, (flight.to-flight.from).normalize(),
                        if flight.weapon == Weapon::Pistol { 0.12 } else { 0.20 }, false, flight.weapon);
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
        let muzzle = matches!(b.kind, BurstKind::Muzzle);
        let blast = matches!(b.kind, BurstKind::Blast);
        let hot = !matches!(b.kind, BurstKind::Dust);
        let distance = eye.distance(b.pos).max(0.1);
        let detail = if distance < b.radius * 18.0 { GRAINS }
            else if distance < b.radius * 65.0 { 1024 } else { 256 };
        let desired = if blast { detail } else if muzzle && b.radius > 0.6 { detail.min(768) }
            else if b.radius > 0.4 { detail.min(384) } else { 48 };
        let count = desired.min(remaining);
        // Coarsen distant silhouettes only as grain count decreases; close fire is composed of
        // thousands of fine voxels, while its bounded, less detailed far LOD remains recognizable.
        let grain_scale = (GRAINS as f32 / desired as f32).sqrt().clamp(1.0, 3.5);
        let flash_life = if muzzle { 0.11 } else if blast { 0.85 } else { 0.24 };
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
            let mut pos = b.pos + radial * b.radius * expansion * (0.25 + rnd * 0.9);
            pos += Vec3::new(0.19 * t, (0.15 + rnd * 0.42) * t, -0.10 * t) * b.radius;
            let flame = hot && t < flash_life && j % 3 != 0;
            let page = if flame {
                let phase = t / flash_life;
                if phase < 0.20 && j % 5 == 0 { 3 }
                else if muzzle { b.weapon.page() }
                else if phase < 0.48 { 4 } else { 5 }
            } else if blast { 7 } else { 6 };
            // Separate small grains, not one scaled cube. The three delayed lobes expand and cool
            // into matte rising soot; a per-grain stagger dissolves the cloud without alpha sheets.
            if !flame && f > 0.58 + rnd * 0.38 { continue; }
            let size = b.radius * grain_scale * if flame { 0.019 + rnd * 0.025 }
                else { (0.021 + rnd * 0.034) * (0.65 + expansion * 0.8) } * fade;
            self.push(page, Vec3::splat(size.max(0.008)), pos,
                Quat::from_rotation_y(rnd * 1.57 + t * 0.18));
        }
        count
    }
}

pub(super) fn allocate(device: &wgpu::Device, queue: &wgpu::Queue,
    renderer: &mut Renderer) -> [usize; PAGES] {
    std::array::from_fn(|i| {
        let slot = renderer.add_dynamic_instanced_pool(device, queue,
            &box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), COLORS[i]), PAGE_CAPACITY)
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
        self.weapon_vfx.pixel_world_scale = 2.0 * (r.camera.fov_y * 0.5).tan() / r.ih.max(1) as f32;
        self.weapon_vfx.prepare(dt, r.camera.pos);
        for shot in self.projectiles.iter().take(CANNON_DRAWS) {
            self.weapon_vfx.streak(shot.pos, shot.velocity.normalize_or_zero(), shot.distance_m,
                Weapon::Cannon(shot.shell.caliber_mm as u16), r.camera.pos);
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
            set_camera(r); g.drive_weapon_vfx(0.0,r);
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
                ("MG / ORANGE",Weapon::MachineGun),("CANNON <80 MM / RED-ORANGE",Weapon::Cannon(76)),
                ("CANNON 88+ / ROCKET / HOT RED",Weapon::Cannon(88))];
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
        g.weapon_vfx.prepare(0.035,eye);
        capture("small_arms",&mut g,&mut r,&mut frames)?;
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
        for _ in 0..240 {
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
        for _ in 0..10 { fx.prepare(0.10, Vec3::ZERO); }
        assert_eq!(fx.mats[..6].iter().map(Vec::len).sum::<usize>(), 0);
        assert!(fx.mats[7].len() > 20);
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
        assert!(red_bias.windows(2).all(|pair|pair[1]>pair[0]));
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
