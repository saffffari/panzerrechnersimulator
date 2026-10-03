//! Bounded raster-only weapon presentation. No collision bodies or damage decisions live here.
//! Runtime small arms and cannon trails sample authoritative projectile states. The legacy
//! endpoint-flight helper remains available only to isolated presentation diagnostics.
use super::*;
use crate::renderer::renderer_weapon_particles::{Record as GpuWeaponRecord, math as gpu_weapon};

const MATERIAL_PAGES: usize = 9;
const TRACER_BASE: usize = MATERIAL_PAGES;
pub(super) const PAGES: usize = MATERIAL_PAGES + 6;
const FLIGHTS: usize = 160;
const BULLET_SEGMENTS:usize=4096;
const BULLET_EXPOSURE_S:f32=1.0/120.0;
const MAX_BULLET_EXPOSURE_S:f32=1.0/30.0;
const SMALL_ARM_EVENTS:usize=512;
const BURSTS: usize = 48;
const GRAINS: usize = 3072;
const FRAME_GRAINS: usize = 24_576;
const CANNON_DRAWS: usize = 128;
// Fine exterior incandescent grains: density comes from the number of sparks, not a wide
// luminous cuboid per particle. These authored raster streaks do not change impact physics.
const CANNON_RICOCHET_SPARKS: usize = 384;
const SMALL_ARM_RICOCHET_SPARKS: usize = 72;
const BLAST_SPARKS: usize = 384;
const IMPACT_SPARKS: usize = 192;
const SPARK_EXPOSURE_S: f32 = 0.008;
const SPARK_MAX_LENGTH_M: f32 = 0.32;
const ARMOR_FLASH_OFFSET_M: f32 = 0.03;
const ARMOR_RICOCHET_OFFSET_M: f32 = 0.008;
pub(super) const PAGE_CAPACITY: usize = FRAME_GRAINS + FLIGHTS * 2 + CANNON_DRAWS * 2;
const COLORS: [[u8; 3]; MATERIAL_PAGES] = [
    [255, 226, 96], [255, 181, 48], [255, 146, 28], [255, 249, 215],
    [255, 202, 62], [255, 191, 52], [142, 143, 136], [76, 79, 82],
    [228, 231, 234], // pale propellant aerosol, separate from blast soot and ground dust
];
const EMISSION: [f32; MATERIAL_PAGES] = [4.0, 4.5, 5.0, 9.0, 4.2, 5.2, 0.0, 0.0, 0.0];

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
enum BurstKind { Muzzle, Blast, InteriorBlast, PressureVent(Vec3), FullCookoff(Vec3), Ground {normal:Vec3,budget:gpu_weapon::GroundBudget}, Impact, ArmorImpact(Vec3), PenetrationSmoke(Vec3), HotRim {normal:Vec3,heat:gpu_weapon::RimHeat}, Dust, Ricochet(Vec3), BulletArmor { normal:Vec3, energy_j:f32, cosine:f32 } }
impl BurstKind {
    fn emits_smoke(self) -> bool { !matches!(self, Self::Ricochet(_) | Self::BulletArmor{..} | Self::Ground{..} | Self::HotRim{..}) }
}
#[derive(Clone,Copy)]
struct SurfaceAnchor {vehicle:ProjectileVehicle,part:ArmorPart,point:Vec3,normal:Vec3,tangent:Vec3}
impl SurfaceAnchor {
    fn new(vehicle:ProjectileVehicle,part:ArmorPart,model:Mat4,point:Vec3,normal:Vec3)->Self {
        let tangent=Quat::from_rotation_arc(Vec3::Z,normal)*Vec3::X;
        Self {vehicle,part,point:model.inverse().transform_point3(point),
            normal:model.transpose().transform_vector3(normal).normalize(),
            tangent:model.inverse().transform_vector3(tangent).normalize()}
    }
    fn pose(self,model:Mat4)->(Vec3,Vec3,Vec3) {
        let normal=model.inverse().transpose().transform_vector3(self.normal).normalize();
        let tangent=model.transform_vector3(self.tangent).normalize();
        (model.transform_point3(self.point)+normal*0.002,normal,tangent)
    }
}
#[derive(Clone, Copy)]
struct Burst { pos: Vec3, dir: Vec3, age: f32, life: f32, radius: f32,
    seed: u32, kind: BurstKind, weapon: Weapon, born_s:Option<f32>,anchor:Option<SurfaceAnchor> }

/// Cosmetic armor grains cannot cross the source plate. Test the complete oriented cuboid,
/// including its speed streak, rather than only its center. Returning grains disappear on
/// contact; this source-plane approximation does not simulate secondary hull collisions.
fn armor_grain_is_air_side(burst: Burst, pos: Vec3, size: Vec3, rotation: Quat) -> bool {
    let (normal, offset) = match burst.kind {
        BurstKind::Ground{normal,..} => (normal,0.01),
        BurstKind::ArmorImpact(normal) | BurstKind::PressureVent(normal) | BurstKind::FullCookoff(normal) => (normal, ARMOR_FLASH_OFFSET_M),
        BurstKind::Ricochet(normal) | BurstKind::BulletArmor {normal,..} => (normal, ARMOR_RICOCHET_OFFSET_M),
        _ => return true,
    };
    let half_support = (rotation.conjugate() * normal).abs().dot(size) * 0.5;
    (pos - burst.pos).dot(normal) + offset >= half_support
}

/// Read-only history of actual swept substeps. A rendered segment never extends beyond a
/// resolved contact. Fixed shutter duration is independent of output frame rate.
#[derive(Clone, Copy, Debug)]
struct BulletSegment { id:u64, start:Vec3, end:Vec3, at:f32, until:f32 }
impl BulletSegment {
    fn exposed(self,now:f32)->Option<(Vec3,Vec3)> {
        self.exposed_for(now,BULLET_EXPOSURE_S)
    }
    fn exposed_for(self,now:f32,exposure:f32)->Option<(Vec3,Vec3)> {
        let lo=self.at.max(now-exposure);let hi=self.until.min(now);
        if hi<=lo || self.until<=self.at {return None;}
        let a=(lo-self.at)/(self.until-self.at);let b=(hi-self.at)/(self.until-self.at);
        Some((self.start.lerp(self.end,a),self.start.lerp(self.end,b)))
    }
}
#[derive(Default)]
struct SmallArmsReview {
    discharges:u64,tracer_selected:u64,armor_contacts:u64,player_armor_contacts:u64,spark_bursts:u64,
    tracer_segments_submitted:u64,tracer_frames:u64,omitted_segments:u64,
    structural_armor_calls:u64,structural_armor_mixer_calls:u64,structural_armor_silenced_calls:u64,
    events:std::collections::VecDeque<serde_json::Value>,
}
impl SmallArmsReview {
    fn event(&mut self,event:serde_json::Value) {
        if self.events.len()==SMALL_ARM_EVENTS {self.events.pop_front();}
        self.events.push_back(event);
    }
}

pub(super) struct Effects {
    flights: [Option<Flight>; FLIGHTS], bursts: [Option<Burst>; BURSTS],
    next_flight: usize, next_burst: usize, sequence: u32,
    replaced_bursts:u64,
    pixel_world_scale: f32,
    gpu_analytic: bool,
    gpu_tracers: Vec<GpuWeaponRecord>,
    bullet_exposure_s:f32,
    wind: Vec2,
    now_s:f32,
    small_arm_rounds:std::collections::BTreeMap<(u32,u8,u8),u32>,
    bullet_segments:std::collections::VecDeque<BulletSegment>,
    bullet_times:std::collections::BTreeMap<u64,f32>,
    last_bullet_contact_s:Option<f32>,
    small_arms_review:SmallArmsReview,
    mats: [Vec<Mat4>; PAGES],
    particle_ids: [Vec<[u32;4]>; PAGES],
}
impl Default for Effects {
    fn default() -> Self { Self {
        flights: [None; FLIGHTS], bursts: [None; BURSTS], next_flight: 0, next_burst: 0,
        sequence: 0, replaced_bursts:0, pixel_world_scale: 0.0036, gpu_analytic:false,
        gpu_tracers:Vec::with_capacity(BULLET_SEGMENTS), bullet_exposure_s:BULLET_EXPOSURE_S, wind: Vec2::ZERO,
        now_s:0.0,small_arm_rounds:Default::default(),bullet_segments:Default::default(),
        bullet_times:Default::default(),last_bullet_contact_s:None,small_arms_review:Default::default(),
        mats: std::array::from_fn(|_| Vec::with_capacity(PAGE_CAPACITY)),
        particle_ids: std::array::from_fn(|_| Vec::with_capacity(PAGE_CAPACITY)),
    } }
}
/// Authored incandescent ejecta budget, driven by actual normal-impact energy. These tiny
/// cosmetic grains are not additional damaging fragments or calibrated thermal radiation.
fn bullet_spark_count(energy_j:f32,cosine:f32)->usize {
    ((energy_j.max(0.0)*(0.2+0.8*cosine.clamp(0.0,1.0)) / 45.0).sqrt()*8.0)
        .round().clamp(1.0,96.0) as usize
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
            .find(|&i| self.bursts[i].is_none()).unwrap_or_else(|| {
                // A sustained MG stream must not erase a sixteen-second cookoff plume before
                // its ballistic apex. Reuse ordinary/muzzle emitters before catastrophe slots.
                (0..BURSTS).map(|n|(self.next_burst+n)%BURSTS).find(|&i| {
                    !matches!(self.bursts[i].unwrap().kind,BurstKind::FullCookoff(_)|BurstKind::PressureVent(_))
                }).unwrap_or(self.next_burst)
            });
        if self.bursts[k].is_some(){self.replaced_bursts+=1;}
        self.bursts[k] = Some(Burst { pos, dir: dir.normalize_or_zero(), age: 0.0,
            life, radius: radius.clamp(0.02, 9.0), seed: self.sequence, kind, weapon, born_s:None,anchor:None });
        self.next_burst = (k + 1) % BURSTS;
    }
    pub(super) fn muzzle(&mut self, pos: Vec3, dir: Vec3, weapon: Weapon) {
        let life = match weapon { Weapon::Pistol => 0.65, Weapon::Rifle | Weapon::MachineGun => 0.95, _ => 3.4 };
        self.burst(pos, dir, weapon.muzzle_size(), life, BurstKind::Muzzle, weapon);
    }
    pub(super) fn blast(&mut self, pos: Vec3, radius: f32) {
        self.burst(pos, Vec3::Y, radius, 6.0, BurstKind::Blast, Weapon::Cannon(88));
    }
    /// Cabin fragmentation already comes from the authoritative solver. Keep the same ignition
    /// flash and smoke without adding uncollided exterior ejecta through the compartment.
    pub(super) fn interior_blast(&mut self, pos: Vec3, radius: f32) {
        self.burst(pos, Vec3::Y, radius, 6.0, BurstKind::InteriorBlast, Weapon::Cannon(88));
    }
    /// Authored pressure jet only at an actual recorded breach or separated turret-ring opening.
    /// It does not create an opening, damage or acoustic event by itself.
    pub(super) fn uses_native_ground(&self)->bool {self.gpu_analytic}
    pub(super) fn ground_impact(&mut self,id:u64,at:f32,point:Vec3,velocity:Vec3,mass:f32,caliber_mm:f32,normal:Vec3,snow:f32) {
        let Some(budget)=gpu_weapon::ground_budget(mass,velocity,caliber_mm,snow)else{return;};
        if !point.is_finite() || !normal.is_finite() || normal.length_squared()<0.5{return;}
        let n=normal.normalize();
        self.burst(point+n*0.01,velocity.normalize_or_zero(),0.2,3.0,BurstKind::Ground{normal:n,budget},Weapon::Cannon(caliber_mm as u16));
        let k=(self.next_burst+BURSTS-1)%BURSTS;
        if let Some(b)=self.bursts[k].as_mut(){b.born_s=at.is_finite().then_some(at);}
        self.small_arms_review.event(serde_json::json!({"event":"ground_ejecta_source","projectile_id":id,
            "at_s":at,"position":point.to_array(),"normal":n.to_array(),"residual_velocity_mps":velocity.to_array(),
            "projectile_mass_kg":mass,"caliber_mm":caliber_mm,"residual_energy_j":budget.residual_j,
            "ejecta_energy_cap_j":budget.ejecta_j,"estimated_ejecta_mass_kg":budget.mass_kg,
            "estimated_material_density_kg_m3":budget.density,"snow_fraction":budget.snow,
            "requested_grains":budget.base_count*gpu_weapon::SPARK_MULTIPLIER,"height_cap_m":gpu_weapon::GROUND_APEX_TARGET_M}));
    }
    pub(super) fn pressure_vent(&mut self,point:Vec3,normal:Vec3) {
        if !point.is_finite() || !normal.is_finite() || normal.length_squared()<0.5{return;}
        let n=normal.normalize();self.burst(point+n*ARMOR_FLASH_OFFSET_M,n,0.20,1.90,BurstKind::PressureVent(n),Weapon::Cannon(88));
    }
    /// Full-ammunition event only: visual ejecta with a304.8m maximum vertical ballistic rise.
    /// Authoritative turret and damaging fragments remain in their independent physical solvers.
    pub(super) fn full_cookoff_plume(&mut self,point:Vec3,normal:Vec3) {
        if !point.is_finite() || !normal.is_finite() || normal.length_squared()<0.5{return;}
        let n=normal.normalize();self.burst(point+n*ARMOR_FLASH_OFFSET_M,n,2.0,18.0,BurstKind::FullCookoff(n),Weapon::Cannon(88));
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
            self.burst(point+normal*ARMOR_FLASH_OFFSET_M,normal,0.75,1.2,BurstKind::ArmorImpact(normal),weapon);
        }
        self.burst(point+normal*ARMOR_RICOCHET_OFFSET_M,reflected,0.24,if matches!(weapon,Weapon::Cannon(_) | Weapon::Rocket) {1.4}else{0.65},BurstKind::Ricochet(normal),weapon);
    }
    fn stamp_newest(&mut self,at:f32,anchor:Option<SurfaceAnchor>) {
        if let Some(b)=self.bursts[(self.next_burst+BURSTS-1)%BURSTS].as_mut(){
            b.born_s=at.is_finite().then_some(at);b.anchor=anchor;
        }
    }
    pub(super) fn steel_perforation_spray(&mut self,point:Vec3,incoming:Vec3,normal:Vec3,
        deposited_j:f32,caliber_mm:f32,at:f32) {
        let incident=incoming.normalize_or_zero();let mut normal=normal.normalize_or_zero();
        if normal.length_squared()<0.5 || incident.length_squared()<0.5{return;}
        if incident.dot(normal)>0.0 {normal=-normal;}
        let weapon=Weapon::Cannon(caliber_mm as u16);
        self.burst(point+normal*ARMOR_RICOCHET_OFFSET_M,normal,0.02,0.45,
            BurstKind::BulletArmor{normal,energy_j:deposited_j.max(0.0)*0.006,
                cosine:(-incident.dot(normal)).clamp(0.0,1.0)},weapon);self.stamp_newest(at,None);
        self.burst(point,normal,(caliber_mm*0.00065).clamp(0.025,0.15),0.9,
            BurstKind::PenetrationSmoke(normal),weapon);self.stamp_newest(at,None);
    }
    /// Consume the resolved terminal classification. Chemical filler behind armor never
    /// becomes a free exterior fireball here; interior detonation owns its separate route.
    fn armor_terminal(&mut self,id:u64,at:f32,point:Vec3,incoming:Vec3,normal:Vec3,
        event:crate::ballistics::ImpactEvent,anchor:SurfaceAnchor,outgoing:Option<Vec3>) {
        use crate::ballistics::Terminal;
        let incident=incoming.normalize_or_zero();let mut normal=normal.normalize_or_zero();
        if !point.is_finite() || !incoming.is_finite() || normal.length_squared()<0.5 || incident.length_squared()<0.5{return;}
        if incident.dot(normal)>0.0 {normal=-normal;}
        let weapon=Weapon::Cannon(event.shell.caliber_mm as u16);
        let kinetic=0.5*event.shell.mass_kg*incoming.length_squared();
        let residual=if matches!(event.terminal,Terminal::Penetration|Terminal::Ricochet) {0.5*event.shell.mass_kg*event.residual_v.powi(2)}else{0.0};
        let deposited=(kinetic-residual.min(kinetic)).max(0.0);
        // The solver also names an armed HE/HEAT charge's thin-plate breach "Penetration".
        // That actual exterior chemical event is distinct from kinetic AP/APCBC perforation.
        let exterior_detonation=event.shell.filler_g>0.0 && (event.terminal==Terminal::HeBlast
            || event.terminal==Terminal::Penetration && matches!(event.shell.kind,
                crate::ballistics::ShellKind::He|crate::ballistics::ShellKind::Heat));
        if exterior_detonation {
            self.blast(point+normal*0.08,(event.shell.filler_g/1000.0).cbrt()*3.0);
            self.stamp_newest(at,None);
        }
        match event.terminal {
            Terminal::Penetration|Terminal::Shatter if !exterior_detonation=>{
                // Only a small portion of plate work drives this cosmetic exterior incandescent
                // dust. These do not duplicate authoritative interior damaging fragments.
                self.burst(point+normal*ARMOR_RICOCHET_OFFSET_M,normal,0.02,0.45,
                    BurstKind::BulletArmor{normal,energy_j:deposited*0.006,cosine:(-incident.dot(normal)).clamp(0.0,1.0)},weapon);
                self.stamp_newest(at,None);
                self.burst(point,normal,(event.shell.caliber_mm*0.00065).clamp(0.025,0.15),0.9,
                    BurstKind::PenetrationSmoke(normal),weapon);self.stamp_newest(at,None);
                if event.terminal==Terminal::Penetration {
                    if let Some(heat)=gpu_weapon::rim_heat(event.shell.caliber_mm,event.plate.thickness_mm,deposited) {
                        self.burst(point+normal*0.002,Quat::from_rotation_arc(Vec3::Z,normal)*Vec3::X,heat.radius,6.0,
                            BurstKind::HotRim{normal,heat},weapon);self.stamp_newest(at,Some(anchor));
                    }
                }
            },
            Terminal::Ricochet=>{
                let reflected=outgoing.expect("resolved ricochet carries actual outgoing velocity").normalize_or_zero();
                self.burst(point+normal*ARMOR_RICOCHET_OFFSET_M,reflected,0.24,1.4,BurstKind::Ricochet(normal),weapon);
                self.stamp_newest(at,None);
            },
            Terminal::Bounce=>{
                self.burst(point+normal*ARMOR_RICOCHET_OFFSET_M,normal,0.02,0.45,
                    BurstKind::BulletArmor{normal,energy_j:deposited*0.006,cosine:(-incident.dot(normal)).clamp(0.0,1.0)},weapon);
                self.stamp_newest(at,None);
            },
            _=>{},
        }
        self.small_arms_review.event(serde_json::json!({"event":"armor_terminal_presentation","projectile_id":id,
            "at_s":at,"terminal":format!("{:?}",event.terminal),"position":point.to_array(),"normal":normal.to_array(),
            "actual_kinetic_j":kinetic,"deposited_kinetic_j":deposited,"filler_g":event.shell.filler_g,
            "exterior_detonation":exterior_detonation,
            "outgoing_ricochet_projectile":outgoing.is_some(),"outgoing_velocity_mps":outgoing.map(|v|v.to_array()),
            "retained_kinetic_j":residual,"ricochet_response_model":if outgoing.is_some(){Some("authored grazing inelastic point-mass response; not measured restitution")}else{None}}));
    }
    pub(super) fn next_small_arm_round(&mut self,shot:crate::infantry_combat::Shot)->u32 {
        use crate::infantry_combat::{Team,Weapon as W};
        let key=(shot.shooter,if shot.team==Team::German {0}else{1},
            match shot.weapon {W::P08=>0,W::Rifle=>1,W::MachineGun=>2});
        let round=self.small_arm_rounds.entry(key).or_default();
        *round=round.wrapping_add(1).max(1);*round
    }
    pub(super) fn record_small_arm_discharge(&mut self,p:&AuthoritativeProjectile) {
        let Some(b)=p.small_arm else {return;};
        let r=&mut self.small_arms_review;r.discharges+=1;r.tracer_selected+=u64::from(b.tracer);
        r.event(serde_json::json!({"event":"discharge","projectile_id":p.id,"shooter":b.shot.shooter,
            "team":if b.shot.team==crate::infantry_combat::Team::German {"German"}else{"Soviet"},
            "profile":p.shell.name,"round":b.round,"tracer_selected":b.tracer,
            "origin":p.origin.to_array(),"muzzle_velocity_mps":p.shell.muzzle_velocity,
            "caliber_mm":p.shell.caliber_mm,"mass_kg":p.shell.mass_kg,"drag_k_per_m":p.shell.drag_k}));
    }
    pub(super) fn record_bullet_segment(&mut self,id:u64,tracer:bool,start:Vec3,end:Vec3,
        at:f32,until:f32,distance:f32,end_distance:f32) {
        if !start.is_finite() || !end.is_finite() || !at.is_finite() || !until.is_finite(){return;}
        self.bullet_times.insert(id,until);
        // Authored visible composition range, not a collision/range alteration. No ball round glow.
        if !tracer || distance>=800.0 || until<=at {return;}
        let f=if end_distance>800.0 {(800.0-distance)/(end_distance-distance)}else{1.0};
        if self.bullet_segments.len()==BULLET_SEGMENTS {
            self.bullet_segments.pop_front();self.small_arms_review.omitted_segments+=1;
        }
        self.bullet_segments.push_back(BulletSegment{id,start,end:start.lerp(end,f),at,until:at+(until-at)*f});
    }
    pub(super) fn record_small_arm_contact(&mut self,p:&AuthoritativeProjectile,c:ProjectileContact) {
        let at=self.bullet_times.remove(&p.id).unwrap_or(self.now_s);
        self.last_bullet_contact_s=Some(at);
        let vehicle=if let ProjectileContactKind::Vehicle(a)=c.kind {
            match a.vehicle {
                ProjectileVehicle::Player=>{self.small_arms_review.player_armor_contacts+=1;Some("Player".to_owned())},
                ProjectileVehicle::Enemy(i)=>Some(format!("Enemy({i})")),
                ProjectileVehicle::Ally(i)=>Some(format!("Ally({i})")),
            }
        }else{None};
        let (kind,normal)=match c.kind {
            ProjectileContactKind::Vehicle(a)=>("armor",Some(a.world_normal.to_array())),
            ProjectileContactKind::RangeLimit=>("range_retirement",None),
            ProjectileContactKind::Foot|ProjectileContactKind::ExposedCrew(..)|ProjectileContactKind::Infantry(_)=>("body",None),
            _=>("world",None),
        };
        if kind=="armor" {self.small_arms_review.armor_contacts+=1;}
        self.small_arms_review.event(serde_json::json!({"event":"contact","projectile_id":p.id,
            "kind":kind,"vehicle":vehicle,"world_time_s":at,"flight_time_s":p.age_s,"distance_m":p.distance_m,
            "position":c.pos.to_array(),"normal":normal,"velocity_mps":p.velocity.length(),
            "energy_j":0.5*p.shell.mass_kg*p.velocity.length_squared()}));
    }
    /// Direct same-hull audio bypasses the delayed world queue. Record the actual call here,
    /// without pretending it is an acoustic wave arrival or isolated audible contribution.
    pub(super) fn record_small_arm_structural_sound(&mut self, id:u64, point:Vec3, at:f32,
        mixer_present:bool, listener_silenced:Option<bool>, listener_peer:Option<u32>,
        listener_peer_kia:Option<bool>) {
        let r=&mut self.small_arms_review;
        r.structural_armor_calls+=1;
        r.structural_armor_mixer_calls+=u64::from(mixer_present);
        r.structural_armor_silenced_calls+=u64::from(listener_silenced==Some(true));
        r.event(serde_json::json!({"event":"structural_armor_sound_submission",
            "projectile_id":id,"position":point.to_array(),"submitted_at_s":at,
            "contact_at_s":self.last_bullet_contact_s,
            "source_route":"same_vehicle_structure_borne_at_ear",
            "mixer_present":mixer_present,"listener_silenced":listener_silenced,
            "listener_peer_id":listener_peer,"listener_peer_kia":listener_peer_kia,
            "scope":"Actual playback-method call; no acoustic wavefront or isolated audibility assertion"}));
    }
    pub(super) fn small_arms_receipt(&self)->serde_json::Value {
        let r=&self.small_arms_review;
        serde_json::json!({"schema":"small-arms-presentation.v1","discharges":r.discharges,
            "tracer_selected":r.tracer_selected,"armor_contacts":r.armor_contacts,
            "player_armor_contacts":r.player_armor_contacts,"armor_spark_bursts":r.spark_bursts,
            "structural_armor_sound_calls":r.structural_armor_calls,
            "structural_armor_mixer_calls":r.structural_armor_mixer_calls,
            "structural_armor_silenced_listener_calls":r.structural_armor_silenced_calls,
            "tracer_segments_submitted":r.tracer_segments_submitted,"tracer_frames_submitted":r.tracer_frames,
            "omitted_tracer_segments":r.omitted_segments,"events":r.events,"event_capacity":SMALL_ARM_EVENTS,
            "tracer_policy":"MG: every fifth round per emitter; rifle/P08 ball unlit; shared family drag estimate",
            "burst_pool_replacements":self.replaced_bursts,"tracer_exposure_s":self.bullet_exposure_s,"tracer_exposure_max_s":MAX_BULLET_EXPOSURE_S,"visibility":"submitted geometry only; no pixel visibility assertion"})
    }
    pub(super) fn bullet_armor_sparks(&mut self,id:u64,point:Vec3,incoming:Vec3,normal:Vec3,mass:f32,weapon:Weapon) {
        if !incoming.is_finite() || !normal.is_finite() || !mass.is_finite() || mass<=0.0 {return;}
        let normal=normal.normalize_or_zero();let incident=incoming.normalize_or_zero();
        if normal.length_squared()<0.5 || incident.length_squared()<0.5 {return;}
        // Source triangle orientation may be two-sided. Orient its exterior side against arrival.
        let normal=if incident.dot(normal)>0.0 {-normal}else{normal};
        let cosine=(-incident.dot(normal)).clamp(0.0,1.0);
        let energy_j=0.5*mass*incoming.length_squared();
        if energy_j<1.0 {return;}
        let reflected=incident-normal*(2.0*incident.dot(normal));
        self.burst(point+normal*ARMOR_RICOCHET_OFFSET_M,reflected,0.02,0.45,
            BurstKind::BulletArmor{normal,energy_j,cosine},weapon);
        let k=(self.next_burst+BURSTS-1)%BURSTS;
        if let Some(b)=self.bursts[k].as_mut() {b.born_s=self.last_bullet_contact_s;}
        self.small_arms_review.spark_bursts+=1;
        self.small_arms_review.event(serde_json::json!({"event":"armor_spark_burst","projectile_id":id,
            "position":point.to_array(),"normal":normal.to_array(),"energy_j":energy_j,"incidence_cosine":cosine,
            "requested_sparks":bullet_spark_count(energy_j,cosine)*gpu_weapon::SPARK_MULTIPLIER as usize,
            "count_scope":"requested raster analytic grains; renderer receipt records submission/omission"}));
    }
    fn draw_bullet_segments(&mut self,eye:Vec3) {
        self.gpu_tracers.clear();
        let now=self.now_s;
        self.bullet_segments.retain(|s|s.until>now-self.bullet_exposure_s);
        let visible:Vec<_>=self.bullet_segments.iter().filter_map(|s|s.exposed_for(now,self.bullet_exposure_s).map(|(a,b)|(s.id,a,b))).collect();
        if !visible.is_empty(){self.small_arms_review.tracer_frames+=1;}
        for (id,a,b) in visible {
            let delta=b-a;let length=delta.length();if length<1e-5 {continue;}
            let center=(a+b)*0.5;
            if self.gpu_analytic {
                self.gpu_tracers.push(GpuWeaponRecord::tracer(a,b));
            } else {
                // Legacy physical backend fallback; native raster uses per-view projected strips.
                let width=(center.distance(eye)*self.pixel_world_scale).max(0.002).min(0.65);
                self.push(TRACER_BASE+1,Vec3::new(width,width,length),center,
                    Quat::from_rotation_arc(Vec3::Z,delta/length));
            }
            self.small_arms_review.tracer_segments_submitted+=1;
            self.small_arms_review.event(serde_json::json!({"event":"tracer_segment_submitted",
                "projectile_id":id,"at_s":now,"start":a.to_array(),"end":b.to_array()}));
        }
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
        // Continuous tracer pages do not use the fine-grain renderer. Keep parallel shape for
        // all pages anyway; emitted cosmetic parcels always call push_particle with an identity.
        self.push_particle(page, size, pos, rot, [0;4]);
    }
    fn push_particle(&mut self, page: usize, size: Vec3, pos: Vec3, rot: Quat, id:[u32;4]) {
        if self.mats[page].len() < PAGE_CAPACITY && pos.is_finite() && size.is_finite() {
            self.mats[page].push(Mat4::from_scale_rotation_translation(size, rot, pos));
            self.particle_ids[page].push(id);
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
        self.push(TRACER_BASE + weapon.page(), Vec3::new(width, width, length), center, rot);
        self.push(TRACER_BASE + 3, Vec3::new(width * 0.32, width * 0.32, length * 0.78), center, rot);
    }
    fn prepare(&mut self, dt: f32, eye: Vec3) {
        for page in &mut self.mats { page.clear(); }
        for page in &mut self.particle_ids { page.clear(); }
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        self.now_s+=dt;
        for i in 0..FLIGHTS {
            if let Some(mut flight) = self.flights[i] {
                flight.age += dt;
                if flight.age >= flight.life {
                    let impact_delay=(flight.life-(flight.age-dt)).clamp(0.0,dt);
                    let sequence_before_impact=self.sequence;
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
                    if self.sequence != sequence_before_impact {
                        let newest=(self.next_burst+BURSTS-1)%BURSTS;
                        // Bursts advance below by dt; this contact existed only after impact.
                        // Credit the time before arrival so partitioning a frame does not age it.
                        if let Some(burst)=self.bursts[newest].as_mut() { burst.age-=impact_delay; }
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
                if burst.born_s.is_some_and(|born|self.now_s<born) { continue; }
                burst.age = burst.born_s.map_or(burst.age+dt,|born|(self.now_s-born).max(0.0));
                if burst.age >= burst.life { self.bursts[i] = None; continue; }
                let count = self.draw_burst(burst, eye, remaining);
                remaining -= count;
                self.bursts[i] = Some(burst);
            }
        }
    }
    fn gpu_bursts(&self)->Vec<GpuWeaponRecord> {
        self.bursts.iter().flatten().filter_map(|&b|gpu_burst_record(b)).collect()
    }
    fn draw_burst(&mut self, mut b: Burst, eye: Vec3, remaining: usize) -> usize {
        // Only exterior blast presentation expands; source damage and cabin ignition do not.
        if matches!(b.kind,BurstKind::Blast) {b.radius*=gpu_weapon::EXTERIOR_RADIUS_MULTIPLIER;}
        if matches!(b.kind,BurstKind::PressureVent(_)|BurstKind::FullCookoff(_)|BurstKind::Ground{..}|BurstKind::PenetrationSmoke(_)|BurstKind::HotRim{..}) {return 0;}
        if self.gpu_analytic && matches!(b.kind,BurstKind::BulletArmor{..}|BurstKind::Ricochet(_)) {return 0;}
        if let BurstKind::BulletArmor{normal,energy_j,cosine}=b.kind {
            if energy_j<0.01 {return 0;}
            let count=bullet_spark_count(energy_j,cosine).min(remaining);
            for j in 0..count {
                let seed=b.seed.wrapping_mul(3948571).wrapping_add(j as u32*977);
                let rnd=random(seed);let life=0.08+0.22*rnd;
                if b.age>=life {continue;}
                let spray=direction(seed.wrapping_add(19));
                let outward=spray-normal*(2.0*spray.dot(normal).min(0.0));
                let dir=(b.dir*(1.0-cosine)*0.8+outward*0.55+normal*0.25).normalize_or_zero();
                let speed=(energy_j/3200.0).sqrt().min(2.0)*(4.0+18.0*rnd)*(0.4+0.6*cosine.sqrt());
                let age=b.age;let center=b.pos+dir*speed*age-Vec3::Y*4.905*age*age;
                let width=(0.0015+0.002*rnd)*(1.0-age/life).sqrt();
                let size=Vec3::new(width,width,(speed*0.002).clamp(width,0.04));
                let rotation=Quat::from_rotation_arc(Vec3::Z,dir);
                if armor_grain_is_air_side(b,center,size,rotation) {
                    let page=if age<0.02 {3}else if age<0.075 {4}else{2};
                    self.push_particle(page,size,center,rotation,[0x5746_0004,b.seed,j as u32,0]);
                }
            }
            return count;
        }
        if let BurstKind::Ricochet(normal) = b.kind {
            let cannon=matches!(b.weapon,Weapon::Cannon(_) | Weapon::Rocket);
            let count = remaining.min(if cannon {CANNON_RICOCHET_SPARKS}else{SMALL_ARM_RICOCHET_SPARKS});
            for j in 0..count {
                let seed=b.seed.wrapping_mul(3948571).wrapping_add(j as u32*977);
                let rnd=random(seed);
                if b.age > if cannon {0.55+rnd*0.85}else{0.24 + rnd*0.40} { continue; }
                let mut direction=(b.dir*0.75+direction(seed.wrapping_add(19))*0.30+normal*0.25).normalize_or_zero();
                if direction.dot(normal)<0.05 { direction=(direction+normal*(0.06-direction.dot(normal))).normalize_or_zero(); }
                let speed=if cannon {14.0+rnd*30.0}else{5.0+rnd*15.0};
                let offset=direction*speed*b.age-Vec3::Y*(4.905*b.age*b.age);
                let width=(if cannon {0.008+rnd*0.010}else{0.003+rnd*0.005})*(1.0-b.age/b.life).max(0.15);
                let page=if b.age<0.055 {3} else if b.age<0.16 {4} else {2};
                let size=Vec3::new(width,width,(speed*SPARK_EXPOSURE_S).clamp(0.025,SPARK_MAX_LENGTH_M));
                let rotation=Quat::from_rotation_arc(Vec3::Z,direction);
                if armor_grain_is_air_side(b,b.pos+offset,size,rotation) {
                    self.push_particle(page,size,b.pos+offset,rotation,[0x5746_0001,b.seed,j as u32,0]);
                }
            }
            return count;
        }
        // Long-range incandescent ejecta: separate from the expanding flash and the slow soot.
        // Cosmetic trajectories only. Authoritative shell/armor damage is untouched.
        let mut spark_count=0;
        if !self.gpu_analytic && matches!(b.kind,BurstKind::Blast | BurstKind::Impact | BurstKind::ArmorImpact(_)) {
            let n=remaining.min(if matches!(b.kind,BurstKind::Blast){BLAST_SPARKS}else{IMPACT_SPARKS});
            spark_count=n;
            for j in 0..n {
                let seed=b.seed.wrapping_mul(9348571).wrapping_add(j as u32*977);
                let rnd=random(seed); let life=0.45+rnd*1.4;
                if b.age>=life {continue;}
                let mut dir=direction(seed.wrapping_add(17));dir.y=dir.y.abs()*0.75+0.15;
                if let BurstKind::ArmorImpact(normal)=b.kind {
                    // The same outward hemisphere follows the actual struck plate, including
                    // vertical/sloped armor, instead of ejecting through it in world-up space.
                    dir=Quat::from_rotation_arc(Vec3::Y,normal)*dir;
                }
                let velocity=dir.normalize()*(10.0+rnd*28.0);
                let age=b.age;let pos=b.pos+velocity*age-Vec3::Y*(4.905*age*age);
                let v=velocity-Vec3::Y*(9.81*age);let width=(0.006+rnd*0.012)*(1.0-age/life).sqrt();
                let page=if age<0.12 {3}else if age<0.55 {4}else{2};
                let size=Vec3::new(width,width,(v.length()*SPARK_EXPOSURE_S).min(SPARK_MAX_LENGTH_M));
                let rotation=Quat::from_rotation_arc(Vec3::Z,v.normalize());
                if armor_grain_is_air_side(b,pos,size,rotation) {
                    self.push_particle(page,size,pos,rotation,[0x5746_0002,b.seed,j as u32,0]);
                }
            }
        }
        let muzzle = matches!(b.kind, BurstKind::Muzzle);
        let blast = matches!(b.kind, BurstKind::Blast | BurstKind::InteriorBlast);
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
            } else {
                let radial=Vec3::new(ray.x, ray.y.abs() * 0.95, ray.z);
                if let BurstKind::ArmorImpact(normal)=b.kind {
                    Quat::from_rotation_arc(Vec3::Y,normal)*radial
                } else { radial }
            };
            let expansion = if blast { 1.0 - (-t * 4.5).exp() } else { 1.0 - (-t * 5.0).exp() };
            let flame = hot && t < flash_life && j % 3 != 0;
            let wind = self.wind;
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
            let size=Vec3::splat(size.max(0.008));
            let rotation=Quat::from_rotation_y(rnd * 1.57 + t * 0.18);
            if armor_grain_is_air_side(b,pos,size,rotation) {
                self.push_particle(page,size,pos,rotation,[0x5746_0003,b.seed,j as u32,0]);
            }
        }
        count+spark_count
    }
}

/// GPU record preserves the existing analytic family and physical source plane. Increasing the
/// sample count changes cosmetic density, not projectile fragments, damage or light sources.
fn gpu_burst_record(b:Burst)->Option<GpuWeaponRecord> {
    let heavy=matches!(b.weapon,Weapon::Cannon(_)|Weapon::Rocket);
    let (kind,count,normal,offset,energy,cosine,scale)=match b.kind {
        BurstKind::Blast=>(gpu_weapon::BLAST,BLAST_SPARKS,Vec3::ZERO,0.0,0.0,0.0,gpu_weapon::EXTERIOR_RADIUS_MULTIPLIER),
        BurstKind::Impact=>(gpu_weapon::BLAST,IMPACT_SPARKS,Vec3::ZERO,0.0,0.0,0.0,1.0),
        BurstKind::ArmorImpact(n)=>(gpu_weapon::ARMOR_IMPACT,IMPACT_SPARKS,n,ARMOR_FLASH_OFFSET_M,0.0,0.0,1.0),
        BurstKind::Ricochet(n)=>(gpu_weapon::RICOCHET,if heavy {CANNON_RICOCHET_SPARKS}else{SMALL_ARM_RICOCHET_SPARKS},n,ARMOR_RICOCHET_OFFSET_M,0.0,0.0,1.0),
        BurstKind::BulletArmor{normal,energy_j,cosine}=>{
            if energy_j<0.01{return None;}
            (gpu_weapon::BULLET_ARMOR,bullet_spark_count(energy_j,cosine),normal,ARMOR_RICOCHET_OFFSET_M,energy_j,cosine,1.0)
        },
        BurstKind::HotRim{normal,heat}=>{
            let r=GpuWeaponRecord {origin_age:b.pos.extend(b.age).to_array(),direction_life:b.dir.extend(b.life).to_array(),
                normal_offset:normal.extend(0.002).to_array(),parameters:[heat.radius,heat.width,heat.initial_kelvin,2.5],
                meta:[b.seed,gpu_weapon::RIM_SAMPLES,gpu_weapon::RIM_SAMPLES,gpu_weapon::HOT_RIM]};
            return r.active().then_some(r);
        },
        BurstKind::Ground{normal,budget}=>{
            let r=GpuWeaponRecord {origin_age:b.pos.extend(b.age).to_array(),direction_life:b.dir.extend(b.life).to_array(),
                normal_offset:normal.extend(0.01).to_array(),parameters:[budget.ejecta_j,budget.mass_kg,budget.snow,budget.density],
                meta:[b.seed,budget.base_count*gpu_weapon::SPARK_MULTIPLIER,budget.base_count,gpu_weapon::GROUND]};
            return r.active().then_some(r);
        },
        BurstKind::PressureVent(n)=>(gpu_weapon::PRESSURE_VENT,192,n,ARMOR_FLASH_OFFSET_M,0.0,0.0,1.0),
        BurstKind::FullCookoff(n)=>(gpu_weapon::FULL_COOKOFF,768,n,ARMOR_FLASH_OFFSET_M,
            crate::sound_levels::AMMUNITION_DETONATION_TNT_KG*4_184_000.0*0.002,5.0,1.0),
        BurstKind::Muzzle|BurstKind::InteriorBlast|BurstKind::Dust|BurstKind::PenetrationSmoke(_)=>return None,
    };
    let r=GpuWeaponRecord {origin_age:b.pos.extend(b.age).to_array(),direction_life:b.dir.extend(if matches!(b.kind,BurstKind::PressureVent(_)){b.life-0.65}else{b.life}).to_array(),
        normal_offset:normal.extend(offset).to_array(),parameters:[energy,cosine,scale,if heavy {1.0}else{0.0}],
        meta:[b.seed,count as u32*gpu_weapon::SPARK_MULTIPLIER,count as u32,kind]};
    r.active().then_some(r)
}

pub(super) fn allocate(device: &wgpu::Device, queue: &wgpu::Queue,
    renderer: &mut Renderer) -> [usize; PAGES] {
    std::array::from_fn(|i| {
        // Burst parcels contain 64 static fine grains; shell traces retain continuous geometry.
        // Parent trajectories, support, total emitting area and authoritative fragments are unchanged.
        let material = if i >= TRACER_BASE { i - TRACER_BASE } else { i };
        let mesh = if i >= TRACER_BASE {
            box_mesh(Vec3::splat(-0.5),Vec3::splat(0.5),COLORS[material])
        } else if i >= 6 { fine_vfx::smoke_mesh(COLORS[material]) }
        else { fine_vfx::flame_mesh(COLORS[material]) };
        let slot = renderer.add_dynamic_instanced_pool(device, queue,
            &mesh, PAGE_CAPACITY)
            .expect("bounded weapon particle page");
        // Smoke uses the ordinary matte sun/ambient response. The sustained cabin soot material
        // intentionally receives only weak hemisphere light; reusing it here crushed exterior
        // gun smoke into a black solid-looking blob. Neither smoke page emits or joins physics.
        renderer.set_batch_flags(slot, if (6..TRACER_BASE).contains(&i) { 0.0 } else { 2.0 });
        assert!(renderer.set_dynamic_instanced_emissive(slot, EMISSION[material]));
        if i < TRACER_BASE { renderer.set_fine_vfx_batch(slot,COLORS[material],i >= 6,64); }
        slot
    })
}
impl Game {
    fn weapon_armor_model(&self,vehicle:ProjectileVehicle,part:ArmorPart)->Option<Mat4> {
        let tank=match vehicle {ProjectileVehicle::Player=>&self.player,
            ProjectileVehicle::Enemy(i)=>self.enemies.get(i)?,ProjectileVehicle::Ally(i)=>&self.allies.get(i)?.tank};
        if part==ArmorPart::Hull {return Some(tank.hull_model());}
        if let ProjectileVehicle::Enemy(i)=vehicle {
            if let Some(body)=self.wrecks.get(i).and_then(|w|w.t_body.as_ref()) {return Some(body.model());}
        }
        Some(tank.turret_model())
    }
    pub(super) fn present_source_armor_impact(&mut self,projectile:&AuthoritativeProjectile,
        contact:ProjectileContact,armor:VehicleArmorContact,event:crate::ballistics::ImpactEvent,outgoing:Option<Vec3>) {
        let Some(model)=self.weapon_armor_model(armor.vehicle,armor.part) else{return;};
        let mut normal=armor.world_normal.normalize_or_zero();
        if projectile.velocity.dot(normal)>0.0 {normal=-normal;}
        if normal.length_squared()<0.5{return;}
        let anchor=SurfaceAnchor::new(armor.vehicle,armor.part,model,contact.pos,normal);
        self.weapon_vfx.armor_terminal(projectile.id,self.enemy_report_audit.clock_s,contact.pos,
            projectile.velocity,normal,event,anchor,outgoing);
    }
    pub(super) fn present_source_ground_impact(&mut self,projectile:&AuthoritativeProjectile,contact:ProjectileContact,r:&Renderer) {
        if matches!(contact.kind,ProjectileContactKind::Terrain) && r.analytic_weapon_particles_active() {
            let p=contact.pos;let h=0.05;
            let dx=(self.hf.height_at(p.x+h,p.z)-self.hf.height_at(p.x-h,p.z))/(2.0*h);
            let dz=(self.hf.height_at(p.x,p.z+h)-self.hf.height_at(p.x,p.z-h))/(2.0*h);
            let normal=Vec3::new(-dx,1.0,-dz).normalize();
            self.weapon_vfx.gpu_analytic=true;
            self.weapon_vfx.ground_impact(projectile.id,self.enemy_report_audit.clock_s,p,projectile.velocity,projectile.shell.mass_kg,
                projectile.shell.caliber_mm,normal,if self.act.season==crate::world::Season::Winter {1.0}else{0.0});
        }
    }
    pub(super) fn drive_weapon_vfx(&mut self, dt: f32, r: &mut Renderer) {
        // A glowing wound follows the struck source frame, including a physically detached
        // turret. Released smoke/fragments retain their independent world-space trajectories.
        let anchors:Vec<_>=self.weapon_vfx.bursts.iter().enumerate().filter_map(|(i,b)|b.and_then(|b|b.anchor.map(|a|(i,a)))).collect();
        for (i,anchor) in anchors {
            if let Some(model)=self.weapon_armor_model(anchor.vehicle,anchor.part) {
                let (pos,normal,tangent)=anchor.pose(model);
                if let Some(b)=self.weapon_vfx.bursts[i].as_mut(){
                    b.pos=pos;b.dir=tangent;
                    if let BurstKind::HotRim{heat,..}=b.kind {b.kind=BurstKind::HotRim{normal,heat};}
                }
            }
        }
        self.weapon_vfx.gpu_analytic=r.analytic_weapon_particles_active();
        self.weapon_vfx.bullet_exposure_s=if dt.is_finite() {dt.clamp(BULLET_EXPOSURE_S,MAX_BULLET_EXPOSURE_S)}else{BULLET_EXPOSURE_S};
        self.weapon_vfx.wind=r.weather_wind_world;
        self.weapon_vfx.pixel_world_scale = 2.0 * (r.camera.fov_y * 0.5).tan() / r.ih.max(1) as f32;
        self.weapon_vfx.now_s=r.time-dt;
        self.weapon_vfx.prepare(dt, r.camera.pos);
        self.weapon_vfx.draw_bullet_segments(r.camera.pos);
        if self.weapon_vfx.gpu_analytic {
            r.set_analytic_weapon_particles(&self.weapon_vfx.gpu_bursts(),&self.weapon_vfx.gpu_tracers);
        } else {r.set_analytic_weapon_particles(&[],&[]);}
        let mut smoke: Vec<_>=self.weapon_vfx.bursts.iter().flatten().copied().collect();
        smoke.sort_by(|a,b|a.pos.distance_squared(r.camera.pos).total_cmp(&b.pos.distance_squared(r.camera.pos)));
        for mut b in smoke {
            if !b.kind.emits_smoke(){continue;}
            if let BurstKind::PenetrationSmoke(normal)=b.kind {
                let radius=b.radius*(0.4+0.6*b.age.sqrt());
                let wind=Vec3::new(self.weapon_vfx.wind.x,0.0,self.weapon_vfx.wind.y);
                let tangent_wind=wind-normal*wind.dot(normal);
                let source=b.pos+normal*radius;
                r.add_smoke_sweep(source,source+normal*b.age*0.6+tangent_wind*b.age*0.20,
                    radius,0.24*(1.0-b.age/b.life).max(0.0).powi(2),
                    crate::particle_motion::seed_phase(b.seed)+b.age*0.8,0.35);
                continue;
            }
            if matches!(b.kind,BurstKind::Blast) {b.radius*=gpu_weapon::EXTERIOR_RADIUS_MULTIPLIER;}
            if let BurstKind::PressureVent(normal)=b.kind {
                let advance=(b.age*26.0).min(12.0);let fade=(1.0-b.age/b.life).max(0.0);
                r.add_smoke_sweep(b.pos,b.pos+normal*advance,0.35+advance*0.035,0.14*fade,b.age*2.0,0.20);
                continue;
            }
            if let BurstKind::FullCookoff(_)=b.kind {
                // One source-connected swept support, using the existing optical-mass Jacobian.
                // The thermal plume stays aloft as visible ejecta falls; this is an authored
                // envelope, not fluid dynamics or additional damaging fragments.
                let t=b.age.min(gpu_weapon::COOKOFF_VERTICAL_SPEED_MPS/9.81);
                let rise=(gpu_weapon::COOKOFF_VERTICAL_SPEED_MPS*t-4.905*t*t).max(0.0);
                let wind=self.weapon_vfx.wind;
                let head=b.pos+Vec3::new(wind.x*b.age*0.20,rise,wind.y*b.age*0.20);
                let radius=2.0+rise*0.025;
                let fade=(1.0-b.age/b.life).max(0.0);
                r.add_smoke_sweep(b.pos,head,radius,0.35*fade,crate::particle_motion::seed_phase(b.seed)+b.age*0.4,0.09);
                continue;
            }
            let age=b.age;let f=(age/b.life).clamp(0.0,1.0);
            let wind=self.weapon_vfx.wind;
            let muzzle=matches!(b.kind,BurstKind::Muzzle);
            let radius=if muzzle {b.radius*(0.22+age.sqrt()*0.70)}else{b.radius*(0.25+0.60*age.sqrt())};
            let center=if muzzle {b.pos+propellant_offset(b.dir,Vec3::ZERO,age,b.radius,0.5,wind)}
                else {b.pos+Vec3::new(wind.x*age*0.55,age*b.radius*0.32,wind.y*age*0.55)};
            let density=(1.0-f).powi(2)*(if muzzle {0.16}else{0.45})*smoothstep(0.0,0.08,age);
            for lobe in 0..3 {
                let phase=crate::particle_motion::seed_phase(b.seed)+lobe as f32*2.1;
                let offset=Vec3::new(phase.sin(),0.3*phase.cos(),phase.cos())*radius*0.30;
                r.add_smoke_volume(center+offset,radius*0.76,density,age*0.8+phase,if muzzle {0.55}else{0.08});
            }
        }
        let mut cannon_draws=0;
        for shot in &self.projectiles {
            // Bullet tracers use the actual swept history above; ball rounds have no luminous tail.
            if shot.small_arm.is_some() {continue;}
            if cannon_draws>=CANNON_DRAWS {continue;}
            cannon_draws+=1;
            let weapon=Weapon::Cannon(shot.shell.caliber_mm as u16);
            self.weapon_vfx.streak(shot.pos,shot.velocity.normalize_or_zero(),shot.distance_m,weapon,r.camera.pos);
        }
        if self.panzerfaust_shot.active {
            let shot = &self.panzerfaust_shot;
            self.weapon_vfx.streak(shot.pos, shot.vel.normalize_or_zero(), shot.distance,
                Weapon::Rocket, r.camera.pos);
        }
        for (i, page) in self.slots.weapon_vfx.iter().copied().enumerate() {
            assert!(r.set_dynamic_instanced_mats(page, &self.weapon_vfx.mats[i]));
            if i < TRACER_BASE {
                assert!(r.set_fine_vfx_particle_ids(page,&self.weapon_vfx.particle_ids[i]));
            }
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
        r.set_analytic_weapon_particles(&[],&[]);
        for (i,page) in self.slots.weapon_vfx.into_iter().enumerate() {
            r.set_dynamic_instanced_mats(page, &[]);
            if i < TRACER_BASE { assert!(r.set_fine_vfx_particle_ids(page,&[])); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn resolved_terminal_selects_nonpyro_perforation_and_distinct_ricochet() {
        use crate::ballistics::{ArmorScheme,Facing,Terminal};
        let point=Vec3::new(21.0,4.0,-61.0);let normal=Vec3::NEG_Z;
        let incoming=Vec3::Z*760.0;
        let mut event=crate::ballistics::resolve(&crate::ballistics::PZGR_39,100.0,
            ArmorScheme::T34_76.plate(Facing::HullSide),0.0,7);
        let anchor=SurfaceAnchor::new(ProjectileVehicle::Player,ArmorPart::Hull,Mat4::IDENTITY,point,normal);
        for terminal in [Terminal::Penetration,Terminal::Bounce,Terminal::Ricochet,Terminal::Shatter,Terminal::HeBlast] {
            event.terminal=terminal;event.residual_v=if terminal==Terminal::Penetration {300.0}else{0.0};
            let mut fx=Effects::default();fx.gpu_analytic=true;
            fx.armor_terminal(7,0.0,point,incoming,normal,event,anchor,
                (terminal==Terminal::Ricochet).then_some(Vec3::new(100.0,200.0,0.0)));fx.prepare(0.02,point);
            let has=|pred:fn(BurstKind)->bool|fx.bursts.iter().flatten().any(|b|pred(b.kind));
            assert_eq!(has(|k|matches!(k,BurstKind::Ricochet(_))),terminal==Terminal::Ricochet);
            assert_eq!(has(|k|matches!(k,BurstKind::HotRim{..})),terminal==Terminal::Penetration);
            assert_eq!(has(|k|matches!(k,BurstKind::PenetrationSmoke(_))),matches!(terminal,Terminal::Penetration|Terminal::Shatter));
            assert_eq!(has(|k|matches!(k,BurstKind::Blast)),terminal==Terminal::HeBlast);
            assert!(!has(|k|matches!(k,BurstKind::ArmorImpact(_)|BurstKind::Impact|BurstKind::InteriorBlast)));
            if terminal==Terminal::Penetration {assert!(fx.mats.iter().all(Vec::is_empty),"no generic exterior flame parcels");}
        }
        event.shell=&crate::ballistics::SPRGR_88;event.terminal=Terminal::Penetration;
        let mut fx=Effects::default();fx.armor_terminal(8,0.0,point,incoming,normal,event,anchor,None);
        assert!(fx.bursts.iter().flatten().any(|b|matches!(b.kind,BurstKind::Blast)),"real HE breach retains its chemical detonation");
        assert!(!fx.bursts.iter().flatten().any(|b|matches!(b.kind,BurstKind::HotRim{..}|BurstKind::Ricochet(_))));
    }
    #[test] fn entry_rim_follows_the_exact_source_frame_and_outward_normal() {
        let original=Mat4::from_rotation_translation(Quat::from_rotation_y(0.8),Vec3::new(10.0,4.0,-17.0));
        let local=Vec3::new(1.35,0.72,0.25);let normal=Vec3::new(0.9,0.3,0.0).normalize();
        let anchor=SurfaceAnchor::new(ProjectileVehicle::Enemy(2),ArmorPart::Turret,original,
            original.transform_point3(local),original.transform_vector3(normal));
        let moved=Mat4::from_rotation_translation(Quat::from_rotation_x(0.6)*Quat::from_rotation_y(-0.4),Vec3::new(13.0,5.0,-15.0));
        let (point,n,tangent)=anchor.pose(moved);
        assert!((point-(moved.transform_point3(local)+moved.transform_vector3(normal)*0.002)).length()<0.00001);
        assert!(n.dot(tangent).abs()<0.00001);assert!((n.length()-1.0).abs()<0.00001);
    }
    #[test]
    fn live_tracer_uses_only_travelled_segments_and_ball_is_unlit() {
        let mut e=Effects::default();
        e.record_bullet_segment(1,false,Vec3::ZERO,Vec3::Z*20.0,0.0,0.025,0.0,20.0);
        assert!(e.bullet_segments.is_empty());
        e.record_bullet_segment(2,true,Vec3::ZERO,Vec3::Z*3.0,0.0,0.004,0.0,3.0);
        e.record_bullet_segment(2,true,Vec3::Z*3.0,Vec3::new(0.1,-0.001,5.0),0.004,0.006,3.0,5.0);
        e.now_s=0.007;e.draw_bullet_segments(Vec3::Y*3.0);
        assert_eq!(e.small_arms_review.tracer_segments_submitted,2);
        for m in &e.mats[TRACER_BASE+1] {
            let (scale,rotation,center)=m.to_scale_rotation_translation();
            let a=center-rotation*Vec3::Z*scale.z*0.5;
            let b=center+rotation*Vec3::Z*scale.z*0.5;
            assert!(a.z>=-1e-5 && b.z<=5.00001,"terminal end must clip actual path");
        }
        e.now_s=0.006+BULLET_EXPOSURE_S+0.0001;
        for p in &mut e.mats {p.clear();}
        e.draw_bullet_segments(Vec3::ZERO);
        assert!(e.mats[TRACER_BASE..].iter().all(Vec::is_empty));
        assert_eq!(e.small_arms_review.spark_bursts,0,"flight never manufactures contact sparks");
    }
    #[test]
    fn tracer_selection_is_per_emitter_not_global_projectile_order() {
        use crate::infantry_combat::{Shot,Team,Weapon as W};
        let mut e=Effects::default();
        let shot=|shooter|Shot{shooter,team:Team::German,origin:Vec3::ZERO,direction:Vec3::Z,range:100.0,weapon:W::MachineGun};
        for n in 1..=10 {
            assert_eq!(e.next_small_arm_round(shot(7)),n);
            for _ in 0..3 {e.next_small_arm_round(shot(9));}
        }
        assert_eq!(e.next_small_arm_round(shot(7)),11);
    }
    #[test]
    fn structural_armor_audit_keeps_each_contact_call_and_measured_dead_listener_state() {
        let mut e=Effects::default();
        let point=Vec3::new(4.0,2.0,-8.0);
        for id in 1..=3 {
            e.last_bullet_contact_s=Some(id as f32-0.01);
            e.record_small_arm_structural_sound(id,point,id as f32,true,
                Some(id==3),Some(1),Some(id>=2));
        }
        let r=e.small_arms_receipt();
        assert_eq!(r["structural_armor_sound_calls"],3);
        assert_eq!(r["structural_armor_mixer_calls"],3);
        assert_eq!(r["structural_armor_silenced_listener_calls"],1);
        let events=r["events"].as_array().unwrap();
        assert_eq!(events.len(),3);
        for (i,event) in events.iter().enumerate() {
            assert_eq!(event["projectile_id"],i+1);
            assert_eq!(event["position"],serde_json::json!(point.to_array()));
            assert_eq!(event["source_route"],"same_vehicle_structure_borne_at_ear");
            assert!(event.get("speed_of_sound_m_s").is_none(),"a structural call is not an air wave");
        }
        assert_eq!(events[1]["listener_peer_kia"],true);
        assert_eq!(events[1]["listener_silenced"],false,"death and final fade are distinct measured states");
    }
    #[test]
    fn bullet_armor_grains_use_energy_incidence_and_stay_outside_source() {
        assert!(bullet_spark_count(3000.0,1.0)>bullet_spark_count(300.0,1.0));
        assert!(bullet_spark_count(3000.0,1.0)>bullet_spark_count(3000.0,0.05));
        let point=Vec3::new(52.0,7.0,-201.0);
        for normal in [Vec3::X,Vec3::Y,Vec3::NEG_Z,Vec3::new(0.3,0.8,-0.4).normalize()] {
            let mut e=Effects::default();e.bullet_armor_sparks(3,point,-normal*820.0,normal,0.0096,Weapon::Rifle);
            e.prepare(0.025,point+normal*2.0);
            let mut grains=0;
            for m in e.mats.iter().flatten() {
                let (size,rotation,center)=m.to_scale_rotation_translation();
                let support=(rotation.conjugate()*normal).abs().dot(size)*0.5;
                assert!((center-point).dot(normal)-support>=-0.00001);
                assert!(size.max_element()<=0.040001);grains+=1;
            }
            assert!(grains>20);assert_eq!(e.small_arms_review.spark_bursts,1);
            assert!(e.bursts.iter().flatten().all(|b|!b.kind.emits_smoke()));
        }
    }
    #[test]
    fn bullet_trails_and_contact_sparks_match_at_30_60_144_hz() {
        let run=|hz:f32| {
            let mut e=Effects::default();e.last_bullet_contact_s=Some(0.025);
            e.bullet_armor_sparks(2,Vec3::ZERO,Vec3::Z*840.0,Vec3::NEG_Z,0.0096,Weapon::Rifle);
            let mut at=0.0f32;
            while at<0.125-1e-7 {let dt=(1.0/hz).min(0.125-at);e.prepare(dt,Vec3::Y*2.0);at+=dt;}
            // Same two actual solver segments, independent of the renderer's frame partition.
            e.record_bullet_segment(3,true,Vec3::Z*80.0,Vec3::Z*83.0,0.117,0.121,80.0,83.0);
            e.record_bullet_segment(3,true,Vec3::Z*83.0,Vec3::new(0.01,-0.02,86.0),0.121,0.125,83.0,86.0);
            e.draw_bullet_segments(Vec3::ZERO);
            (e.bursts[0].unwrap().age,e.mats)
        };
        let reference=run(60.0);
        for hz in [30.0,144.0] {
            let (age,pages)=run(hz);assert!((age-reference.0).abs()<1e-6);
            for (a,b) in pages.iter().zip(&reference.1) {
                assert_eq!(a.len(),b.len());
                for (ma,mb) in a.iter().zip(b) {assert!(ma.abs_diff_eq(*mb,1e-5));}
            }
        }
    }
    fn fine_snapshot(effects:&Effects)->std::collections::BTreeMap<[u32;4],(usize,Mat4)> {
        let mut out=std::collections::BTreeMap::new();
        for page in 0..TRACER_BASE {
            assert_eq!(effects.mats[page].len(),effects.particle_ids[page].len());
            for (&id,&model) in effects.particle_ids[page].iter().zip(&effects.mats[page]) {
                assert_ne!(id,[0;4],"fine parcels must have source identity");
                assert!(out.insert(id,(page,model)).is_none(),"duplicate identity {id:?}");
            }
        }
        out
    }

    #[test]
    fn cosmetic_lifetimes_and_terminal_age_follow_elapsed_time_not_frame_partition() {
        let make=|| {let mut e=Effects::default();
            e.small_arm(Vec3::ZERO,Vec3::Z*76.0,Weapon::Rifle);e};
        let mut single=make();let mut split=make();
        single.prepare(0.25,Vec3::X*3.0);
        for _ in 0..5 {split.prepare(0.05,Vec3::X*3.0);}
        assert!(single.flights.iter().all(Option::is_none));
        assert!(split.flights.iter().all(Option::is_none));
        let snapshots=|e:&Effects|e.bursts.iter().flatten().map(|b|(b.seed,b.age)).collect::<Vec<_>>();
        let a=snapshots(&single);let b=snapshots(&split);
        assert_eq!(a.len(),b.len());
        for ((aid,age),(bid,other)) in a.into_iter().zip(b) {
            assert_eq!(aid,bid);assert!((age-other).abs()<1e-6,"{age} vs {other}");
        }
        assert!((single.bursts[1].unwrap().age-0.15).abs()<1e-6,
            "76m rifle flight arrives at0.10s; impact is0.15s old at0.25s");
        single.prepare(7.0,Vec3::ZERO);
        assert!(single.bursts.iter().all(Option::is_none));
    }

    #[test]
    fn weapon_parcels_follow_the_same_live_weather_wind_as_the_volume() {
        let make=|wind| {let mut e=Effects::default();e.wind=wind;e.blast(Vec3::ZERO,1.0);
            e.prepare(0.10,Vec3::Z*2.0);fine_snapshot(&e)};
        let calm=make(Vec2::ZERO);let windy=make(Vec2::new(12.0,-5.0));
        let mut tested=0;
        for (id,(_,model)) in calm {
            if id[0]!=0x5746_0003 {continue;}
            let Some((_,other))=windy.get(&id) else {continue;};
            let delta=other.w_axis.truncate()-model.w_axis.truncate();
            assert!(delta.x>0.0 && delta.z<0.0,"live weather drift missing: {delta:?}");
            tested+=1;
        }
        assert!(tested>100);
    }

    #[test]
    fn cosmetic_identity_survives_motion_and_cooling_into_another_material_page() {
        let mut effects=Effects::default();
        effects.blast(Vec3::ZERO,1.0);
        effects.prepare(0.03,Vec3::Z*2.0);
        let first=fine_snapshot(&effects);
        effects.prepare(0.10,Vec3::Z*2.0);
        let next=fine_snapshot(&effects);
        let moved_and_cooled=first.iter().filter(|(id,(page,model))|
            next.get(*id).is_some_and(|(next_page,next_model)|next_page!=page && next_model!=model)).count();
        assert!(moved_and_cooled>100,"actual moving sparks must retain IDs while cooling: {moved_and_cooled}");
    }

    #[test]
    fn cosmetic_identity_survives_other_emitter_retirement_and_page_compaction() {
        let mut effects=Effects::default();
        effects.muzzle(Vec3::ZERO,Vec3::Z,Weapon::Cannon(88));
        effects.muzzle(Vec3::X*2.0,Vec3::Z,Weapon::Cannon(88));
        effects.prepare(0.025,Vec3::Z*3.0);
        let first=fine_snapshot(&effects);
        let surviving_seed=effects.bursts[1].unwrap().seed;
        effects.bursts[0]=None; // unrelated emitter has retired; surviving parcel time is held
        effects.prepare(0.0,Vec3::Z*3.0);
        let next=fine_snapshot(&effects);
        let surviving:Vec<_>=first.into_iter().filter(|(id,_)|id[1]==surviving_seed).collect();
        assert!(!surviving.is_empty());
        assert_eq!(surviving.len(),next.len());
        for (id,state) in surviving { assert_eq!(next[&id],state); }
    }

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
    fn armor_sparks_and_flash_keep_full_support_outside_rotated_source_planes() {
        // Translated near the live fatal Tiger hit, including vertical, inverted and sloped armor.
        let point=Vec3::new(62.06,5.44,-280.65);
        for normal in [Vec3::X,Vec3::NEG_X,Vec3::Y,Vec3::NEG_Y,Vec3::Z,Vec3::NEG_Z,
            Vec3::new(0.65,0.72,-0.25).normalize(),Vec3::new(-0.5,-0.7,0.5).normalize()] {
            let mut fx=Effects::default();
            fx.armor_sparks(point,-normal*760.0,normal,Weapon::Cannon(76));
            for step in 0..26 {
                fx.prepare(0.05,point);
                let count=fx.mats.iter().map(Vec::len).sum::<usize>();
                if step==0 { assert!(count>500,"outward confinement must retain a dense contact spray"); }
                assert!(count<=CANNON_RICOCHET_SPARKS+IMPACT_SPARKS+384);
                for matrix in fx.mats.iter().flatten() {
                    let (size,rotation,center)=matrix.to_scale_rotation_translation();
                    let support=(rotation.conjugate()*normal).abs().dot(size)*0.5;
                    assert!((center-point).dot(normal)-support>=-0.00005,
                        "entire flash/streak must stay outside source plate: normal={normal:?}, age={}, center={center:?}, size={size:?}",
                        (step+1) as f32*0.05);
                }
            }
        }
        // Ordinary non-armor impact presentation keeps its existing world-up radial distribution.
        let mut generic=Effects::default();
        generic.impact(point,Vec3::X,0.75,true,Weapon::Cannon(76));
        generic.prepare(0.08,point);
        assert!(generic.mats.iter().flatten().any(|m|m.w_axis.x<point.x-0.1));
    }

    #[test]
    fn explosion_smoke_follows_world_wind_and_outlives_flash() {
        let mut still=Effects::default();let mut windy=Effects::default();windy.wind=Vec2::new(12.0,-5.0);
        let origin=Vec3::new(1880.0,6.0,165.0);
        still.blast(origin,3.0);windy.blast(origin,3.0);
        for _ in 0..40 {still.prepare(0.1,origin);windy.prepare(0.1,origin);}
        assert!(windy.bursts.iter().flatten().any(|b|matches!(b.kind,BurstKind::Blast)));
        assert!(windy.mats[6..].iter().all(Vec::is_empty), "smoke must not submit opaque matter meshes");
        assert!(windy.mats[..6].iter().all(Vec::is_empty));
    }
    #[test]
    fn interior_catastrophe_keeps_flash_and_smoke_without_exterior_ejecta() {
        // The distinction is semantic, never inferred from blast radius or camera location.
        for radius in [0.65 * 0.65, 3.0] {
            let mut interior = Effects::default();
            let mut exterior = Effects::default();
            interior.interior_blast(Vec3::ZERO, radius);
            exterior.blast(Vec3::ZERO, radius);
            interior.prepare(0.10, Vec3::ZERO);
            exterior.prepare(0.10, Vec3::ZERO);
            let flash: Vec<_> = interior.mats[..6].iter().flatten().copied().collect();
            assert!(flash.len() > 100, "the cabin ignition flash must remain visible");
            assert!(flash.iter().all(|m| {
                let size = m.to_scale_rotation_translation().0;
                size.max_element() - size.min_element() < 1e-5
            }), "interior flash grains must not include elongated exterior spark streaks");
            let outside: Vec<_> = exterior.mats[..6].iter().flatten().copied().collect();
            assert_eq!(outside.len(), flash.len() + BLAST_SPARKS);
            for page in 0..6 {
                let exterior_flash: Vec<_> = exterior.mats[page].iter().filter(|m| {
                    let size = m.to_scale_rotation_translation().0;
                    size.max_element() - size.min_element() < 1e-5
                }).copied().collect();
                assert_eq!(interior.mats[page].len(),exterior_flash.len());
                for (inside,outside) in interior.mats[page].iter().zip(exterior_flash) {
                    let (inside_size,_,inside_pos)=inside.to_scale_rotation_translation();
                    let (outside_size,_,outside_pos)=outside.to_scale_rotation_translation();
                    assert!((outside_pos-inside_pos*gpu_weapon::EXTERIOR_RADIUS_MULTIPLIER).length()<1e-5,
                        "only exterior flash support expands100x in volume");
                    assert!(outside_size.min_element()>inside_size.min_element());
                }
            }
            for _ in 0..4 {
                interior.prepare(0.10, Vec3::ZERO);
                exterior.prepare(0.10, Vec3::ZERO);
            }
            assert!(interior.mats.iter().all(Vec::is_empty),
                "after the flash only authoritative cabin fragments may remain luminous");
            assert!(exterior.mats[..6].iter().any(|page| !page.is_empty()),
                "exterior blast ejecta must retain their existing lifetime");
            let smoke = interior.bursts.iter().flatten().next().unwrap();
            assert!(matches!(smoke.kind, BurstKind::InteriorBlast));
            assert!(smoke.kind.emits_smoke(), "the shared smoke-volume pass must retain cabin soot");
            assert_eq!(smoke.life, 6.0);
            assert!(smoke.age < smoke.life);
        }
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
                if i < TRACER_BASE { assert!(r.set_fine_vfx_particle_ids(page,&g.weapon_vfx.particle_ids[i])); }
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
                set_camera(&mut r);r.clear_smoke();g.drive_weapon_vfx(0.0,&mut r);g.drive_dust_volumes(&mut r);r.ui_reset();
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
            let m = fx.mats[TRACER_BASE][0];
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
    fn raster_gpu_path_uploads_one_record_without_duplicate_cpu_sparks_or_tracers() {
        let mut fx=Effects::default();fx.gpu_analytic=true;
        fx.bullet_armor_sparks(9,Vec3::ZERO,Vec3::Z*820.0,Vec3::NEG_Z,0.0096,Weapon::Rifle);
        fx.prepare(0.025,Vec3::new(2.0,1.0,-3.0));
        assert!(fx.mats.iter().all(Vec::is_empty));
        let records=fx.gpu_bursts();assert_eq!(records.len(),1);
        assert_eq!(records[0].meta[1],records[0].meta[2]*1000);
        fx.record_bullet_segment(10,true,Vec3::Z,Vec3::Z*4.0,0.020,0.025,1.0,4.0);
        fx.draw_bullet_segments(Vec3::ZERO);
        assert_eq!(fx.gpu_tracers.len(),1);assert!(fx.mats[TRACER_BASE..].iter().all(Vec::is_empty));
        assert_eq!(fx.gpu_tracers[0].origin(),Vec3::Z);
        assert_eq!(&fx.gpu_tracers[0].direction_life[..3],&[0.0,0.0,4.0]);
        let mut cabin=Effects::default();cabin.gpu_analytic=true;cabin.interior_blast(Vec3::ZERO,3.0);
        cabin.prepare(0.1,Vec3::ZERO);assert!(cabin.gpu_bursts().is_empty());
        assert!(cabin.mats[..6].iter().any(|p|!p.is_empty()));
    }
    #[test]
    fn bounded_render_shutter_retains_actual_low_fps_travel_without_extending_contact() {
        let segment=BulletSegment {id:1,start:Vec3::ZERO,end:Vec3::Z*3.0,at:0.001,until:0.005};
        assert!(segment.exposed(1.0/30.0).is_none());
        let (a,b)=segment.exposed_for(1.0/30.0,MAX_BULLET_EXPOSURE_S).unwrap();
        assert_eq!(a,segment.start);assert_eq!(b,segment.end);
        assert!(segment.exposed_for(0.05,MAX_BULLET_EXPOSURE_S).is_none(),"bounded shutter never holds stale rounds indefinitely");
    }
}
