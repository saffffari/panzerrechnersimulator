//! Persistent, spatial armor damage for every armored vehicle (the player's Tiger, wingmen,
//! enemy tanks and the wrecks they leave). See docs/ARMOR_SCARS.md.
//!
//! A scar is deliberately compact and quantized: it can live in campaign checkpoints and LAN
//! snapshots without serializing renderer state.  Ballistics asks the field for the plate at the
//! actual hit point; repeated fire into a crater, open penetration or field patch therefore sees a
//! different thickness.  Rendering and repair consume the same records: the raymarch backend turns
//! each one into a displaced crater, a cut hole with its bore wall, petals or a plug, and the
//! back-face spall scab (`renderer_scars.rs`, `shaders/rm_scars.wgsl`).
//!
//! The terminal geometry fields (`aperture_mm`, `back_radius_mm`, `petals`, `petal_mm`, `plug`)
//! are filled today by `terminal_geometry`, an empirical rule from the ballistic terminal state.
//! They are plain data on purpose: a later plate solver (voxel or particle) can write measured
//! values into the same record and every consumer follows without change.

use glam::Vec3;

use crate::ballistics::{Facing, ImpactEvent, Plate, ShellKind, Terminal};

/// Wounds kept per vehicle. Hundreds: a machine-gun burst leaves dozens of pocks, and a Tiger that
/// survived a day's fighting carried that many marks. Eviction drops the least significant pock or
/// dent first, never an open hole or a patch while a lesser mark remains.
pub const MAX_ARMOR_SCARS: usize = 512;

/// Packed size of one scar (`ArmorScar::pack`).
pub const PACKED_SCAR_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ScarKind {
    Dent = 0,
    Hole = 1,
    Patched = 2,
    /// A bullet or fragment strike: paint gone, a millimetre-scale splash crater, no resistance
    /// change (small arms are inert against tank armor in the ballistic model).
    Pock = 3,
}

impl ScarKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Dent),
            1 => Some(Self::Hole),
            2 => Some(Self::Patched),
            3 => Some(Self::Pock),
            _ => None,
        }
    }
}

/// What struck the plate, as far as the scar's shape is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ShotClass {
    /// Full-calibre armour-piercing (capped or not).
    Ap = 0,
    /// Sub-calibre hard core in a soft carrier.
    Apcr = 1,
    He = 2,
    Heat = 3,
    /// Rifle-calibre bullet.
    Bullet = 4,
}

impl ShotClass {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Ap),
            1 => Some(Self::Apcr),
            2 => Some(Self::He),
            3 => Some(Self::Heat),
            4 => Some(Self::Bullet),
            _ => None,
        }
    }

    pub fn of(kind: ShellKind, caliber_mm: f32) -> Self {
        if caliber_mm < 20.0 {
            return Self::Bullet;
        }
        match kind {
            ShellKind::Apcbc | ShellKind::Ap => Self::Ap,
            ShellKind::Apcr => Self::Apcr,
            ShellKind::He => Self::He,
            ShellKind::Heat => Self::Heat,
        }
    }
}

/// One localized wound. Position is millimetres in hull-local or turret-local space (selected by
/// `facing`); lengths are millimetres unless the name says otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmorScar {
    pub facing: Facing,
    pub local_mm: [i16; 3],
    /// Wound radius: crater plus the zone of plastically displaced metal and stripped paint.
    pub radius_mm: u16,
    /// Ballistic resistance lost at the centre of the wound (what `plate_at` subtracts).
    pub loss_mm: u16,
    pub kind: ScarKind,
    pub class: ShotClass,
    /// Outward plate normal at the strike, quantized to 1/127.
    pub normal: [i8; 3],
    /// Shell travel direction at the strike, quantized to 1/127.
    pub dir: [i8; 3],
    /// Crater depth below the original surface, tenths of a millimetre.
    pub depth_dmm: u16,
    /// Plate thickness at the strike (bore length of a hole).
    pub plate_mm: u16,
    /// Open hole radius across the shot (0 for a dent or pock). An oblique hole is elongated along
    /// `dir` by 1/cos of the obliquity.
    pub aperture_mm: u16,
    /// Exit-face scab / spall crater radius (0 when the back face is untouched).
    pub back_radius_mm: u16,
    /// Petals torn back on the exit face (0: a plug was punched or the bore is clean).
    pub petals: u8,
    pub petal_mm: u8,
    /// A plug was punched out of the plate and lies behind it.
    pub plug: bool,
}

impl ArmorScar {
    pub fn local_position(self) -> Vec3 {
        Vec3::new(
            self.local_mm[0] as f32,
            self.local_mm[1] as f32,
            self.local_mm[2] as f32,
        ) * 0.001
    }

    pub fn radius_m(self) -> f32 {
        self.radius_mm as f32 * 0.001
    }

    pub fn depth_m(self) -> f32 {
        self.depth_dmm as f32 * 0.0001
    }

    pub fn aperture_m(self) -> f32 {
        self.aperture_mm as f32 * 0.001
    }

    pub fn plate_m(self) -> f32 {
        self.plate_mm as f32 * 0.001
    }

    pub fn local_normal(self) -> Vec3 {
        dequantize_dir(self.normal).unwrap_or_else(|| facing_axis(self.facing, self.local_position()))
    }

    pub fn local_dir(self) -> Vec3 {
        dequantize_dir(self.dir).unwrap_or(-self.local_normal())
    }

    /// Cosine of the obliquity between the shell path and the plate normal.
    pub fn obliquity_cos(self) -> f32 {
        (-self.local_dir()).dot(self.local_normal()).clamp(0.05, 1.0)
    }

    pub fn is_turret(self) -> bool {
        matches!(
            self.facing,
            Facing::TurretFront | Facing::TurretSide | Facing::TurretRear
        )
    }

    pub fn is_open(self) -> bool {
        self.kind == ScarKind::Hole
    }

    pub fn pack(self) -> [u8; PACKED_SCAR_BYTES] {
        let mut b = [0u8; PACKED_SCAR_BYTES];
        b[0] = facing_code(self.facing);
        b[1] = self.kind as u8;
        b[2] = self.class as u8;
        b[3] = self.petals;
        for (i, v) in self.local_mm.iter().enumerate() {
            b[4 + i * 2..6 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        b[10..12].copy_from_slice(&self.radius_mm.to_le_bytes());
        b[12..14].copy_from_slice(&self.loss_mm.to_le_bytes());
        b[14..16].copy_from_slice(&self.depth_dmm.to_le_bytes());
        b[16..18].copy_from_slice(&self.aperture_mm.to_le_bytes());
        b[18..20].copy_from_slice(&self.plate_mm.to_le_bytes());
        b[20..22].copy_from_slice(&self.back_radius_mm.to_le_bytes());
        b[22] = self.petal_mm;
        b[23] = self.plug as u8;
        for i in 0..3 {
            b[24 + i] = self.normal[i] as u8;
            b[27 + i] = self.dir[i] as u8;
        }
        b
    }

    pub fn unpack(b: &[u8]) -> Option<Self> {
        if b.len() < PACKED_SCAR_BYTES || b[23] > 1 || b[30] != 0 || b[31] != 0 {
            return None;
        }
        let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        let scar = Self {
            facing: facing_from_code(b[0])?,
            kind: ScarKind::from_u8(b[1])?,
            class: ShotClass::from_u8(b[2])?,
            petals: b[3],
            local_mm: [0, 1, 2].map(|i| i16::from_le_bytes([b[4 + i * 2], b[5 + i * 2]])),
            radius_mm: u16_at(10),
            loss_mm: u16_at(12),
            depth_dmm: u16_at(14),
            aperture_mm: u16_at(16),
            plate_mm: u16_at(18),
            back_radius_mm: u16_at(20),
            petal_mm: b[22],
            plug: b[23] == 1,
            normal: [b[24] as i8, b[25] as i8, b[26] as i8],
            dir: [b[27] as i8, b[28] as i8, b[29] as i8],
        };
        valid_scar(&scar).then_some(scar)
    }

    /// A v1 record (campaign checkpoint version 1, LAN protocol 7): centimetre position, no
    /// geometry. The missing fields are rebuilt from the kind and the facing's nominal axis.
    pub fn from_v1(facing: Facing, local_cm: [i16; 3], radius_mm: u16, loss_mm: u16, kind: ScarKind) -> Self {
        let local = Vec3::new(local_cm[0] as f32, local_cm[1] as f32, local_cm[2] as f32) * 0.01;
        let normal = facing_axis(facing, local);
        let hole = matches!(kind, ScarKind::Hole | ScarKind::Patched);
        let plate = if hole { (loss_mm as f32 / 0.94).round().max(1.0) } else { 80.0 };
        Self {
            facing,
            local_mm: quantize_mm(local),
            radius_mm,
            loss_mm,
            kind,
            class: ShotClass::Ap,
            normal: quantize_dir(normal),
            dir: quantize_dir(-normal),
            depth_dmm: if hole { 0 } else { loss_mm.saturating_mul(10) },
            plate_mm: plate as u16,
            aperture_mm: if hole { ((radius_mm as f32 * 0.45) as u16).max(4) } else { 0 },
            back_radius_mm: if hole { radius_mm } else { 0 },
            petals: 0,
            petal_mm: 0,
            plug: false,
        }
    }
}

pub fn facing_code(facing: Facing) -> u8 {
    match facing {
        Facing::HullFront => 0,
        Facing::HullSide => 1,
        Facing::HullRear => 2,
        Facing::TurretFront => 3,
        Facing::TurretSide => 4,
        Facing::TurretRear => 5,
    }
}

pub fn facing_from_code(code: u8) -> Option<Facing> {
    match code {
        0 => Some(Facing::HullFront),
        1 => Some(Facing::HullSide),
        2 => Some(Facing::HullRear),
        3 => Some(Facing::TurretFront),
        4 => Some(Facing::TurretSide),
        5 => Some(Facing::TurretRear),
        _ => None,
    }
}

/// The facing's nominal outward axis, for records that carry no measured normal.
pub fn facing_axis(facing: Facing, local: Vec3) -> Vec3 {
    match facing {
        Facing::HullFront | Facing::TurretFront => Vec3::Z,
        Facing::HullRear | Facing::TurretRear => -Vec3::Z,
        Facing::HullSide | Facing::TurretSide => {
            if local.x < 0.0 { -Vec3::X } else { Vec3::X }
        }
    }
}

/// Terminal geometry derived from the ballistic outcome. Empirical rules, named in
/// docs/PHYSICAL_TRUTH_AUDIT.md ("Armor scar terminal geometry"):
/// * A full-calibre AP hole is ~1.05 calibres across the shot and 1/cos(obliquity) along it.
/// * Plate thicker than ~0.6 calibre fails by ductile hole growth and plugging: a plug of about
///   the calibre is pushed out and the back face scabs to ~1.6-2.4 calibres.
/// * Thinner plate petals: 4-6 petals of ~0.35-0.6 calibre, bent along the residual path.
/// * A sub-calibre core leaves a hole of the core (~0.37 calibre) with the carrier's splash
///   around it; the back face plugs cleanly.
/// * HEAT leaves a narrow jet hole (~0.25 charge diameter) with a wide scorched entrance.
/// * HE does not hole armor it fails to defeat; it dishes and scorches the face.
pub struct TerminalGeometry {
    pub aperture_m: f32,
    pub back_radius_m: f32,
    pub petals: u8,
    pub petal_m: f32,
    pub plug: bool,
}

pub fn terminal_geometry(class: ShotClass, caliber_mm: f32, plate_mm: f32, obliquity_cos: f32,
    residual_v: f32, seed: u32) -> TerminalGeometry {
    let d = caliber_mm.max(5.0) * 0.001;
    let t = plate_mm.max(1.0) * 0.001;
    let jitter = (seed % 1000) as f32 / 1000.0;
    let (aperture, core) = match class {
        ShotClass::Apcr => (0.37 * d * 0.5 * 1.08, 0.37 * d),
        ShotClass::Heat => (0.25 * d * 0.5, 0.25 * d),
        ShotClass::Bullet => (0.5 * d, d),
        _ => (0.5 * d * 1.05, d),
    };
    let thin = t < 0.6 * core * obliquity_cos.max(0.3);
    let back = match class {
        ShotClass::Heat => (1.2 + 0.6 * jitter) * core,
        _ if thin => (0.9 + 0.2 * jitter) * core,
        // A faster residual tears a wider scab off the back face.
        _ => (0.8 + 0.4 * jitter + (residual_v / 2500.0).clamp(0.0, 0.4)) * core,
    };
    TerminalGeometry {
        aperture_m: aperture,
        back_radius_m: back.max(aperture * 1.15),
        petals: if thin { 4 + (seed % 3) as u8 } else { 0 },
        petal_m: if thin { (0.35 + 0.25 * jitter) * core } else { 0.0 },
        plug: !thin && !matches!(class, ShotClass::Heat),
    }
}

#[derive(Clone, Debug, Default)]
pub struct ArmorField {
    scars: Vec<ArmorScar>,
    /// Simulation time each scar was last struck. Not persisted: a loaded scar is cold. Drives the
    /// cooling rim and the fresh-steel sheen.
    struck_s: Vec<f32>,
    /// Bumped on every change, so renderers can skip re-uploading an unchanged field.
    revision: u32,
}

impl PartialEq for ArmorField {
    fn eq(&self, other: &Self) -> bool {
        self.scars == other.scars
    }
}
impl Eq for ArmorField {}

impl ArmorField {
    pub fn from_scars(mut scars: Vec<ArmorScar>) -> Self {
        scars.retain(valid_scar);
        scars.truncate(MAX_ARMOR_SCARS);
        let struck_s = vec![f32::NEG_INFINITY; scars.len()];
        Self { scars, struck_s, revision: 1 }
    }

    pub fn scars(&self) -> &[ArmorScar] {
        &self.scars
    }

    /// When scar `index` was last struck, or `NEG_INFINITY` for a loaded (cold) record.
    pub fn struck_at(&self, index: usize) -> f32 {
        self.struck_s.get(index).copied().unwrap_or(f32::NEG_INFINITY)
    }

    pub fn revision(&self) -> u32 {
        self.revision
    }

    pub fn is_empty(&self) -> bool {
        self.scars.is_empty()
    }

    /// Apply the strongest nearby wound with a soft radial falloff. Multiple tiny dents do not
    /// magically add up across unrelated points, while a shot placed into the same hole sees the
    /// open path. A field patch restores useful splinter/weather protection but never factory armor.
    pub fn plate_at(&self, base: Plate, facing: Facing, local_hit: Vec3) -> Plate {
        let mut worst_loss = 0.0f32;
        for scar in self.scars.iter().filter(|scar| scar.facing == facing && scar.kind != ScarKind::Pock) {
            let radius = scar.radius_m().max(0.02);
            let distance = local_hit.distance(scar.local_position());
            if distance >= radius {
                continue;
            }
            let falloff = 1.0 - smoothstep(0.35, 1.0, distance / radius);
            worst_loss = worst_loss.max(scar.loss_mm as f32 * falloff);
        }
        Plate {
            thickness_mm: (base.thickness_mm - worst_loss).max(4.0),
            ..base
        }
    }

    /// Record without a measured surface (scripted harness hits): the facing's nominal axis is the
    /// normal and the shell is taken to arrive along it.
    pub fn record(&mut self, facing: Facing, local_hit: Vec3, event: &ImpactEvent) -> ArmorScar {
        let normal = facing_axis(facing, local_hit);
        self.record_at(facing, local_hit, normal, -normal, event, 0.0)
    }

    /// Record the permanent plate response after the ballistic result has been decided.
    /// `local_normal` is the struck surface's outward normal and `local_dir` the shell's travel
    /// direction, both in the scar's frame (hull or turret local); `now_s` stamps the fresh strike.
    pub fn record_at(&mut self, facing: Facing, local_hit: Vec3, local_normal: Vec3, local_dir: Vec3,
        event: &ImpactEvent, now_s: f32) -> ArmorScar {
        let escaped_v2=if event.terminal==Terminal::Ricochet {event.residual_v.powi(2)}else{0.0};
        let energy_mj = 0.5 * event.shell.mass_kg * (event.striking_v.powi(2)-escaped_v2).max(0.0) / 1_000_000.0;
        let normal_cos = event.impact_angle_deg.to_radians().cos().clamp(0.12, 1.0);
        let class = ShotClass::of(event.shell.kind, event.shell.caliber_mm);
        let (kind, radius_m, loss_mm) = match event.terminal {
            Terminal::Penetration => (
                ScarKind::Hole,
                (event.shell.caliber_mm * 0.001 * (0.85 + 0.35 * event.behind_armor))
                    .clamp(0.07, 0.24),
                event.plate.thickness_mm * 0.94,
            ),
            Terminal::HeBlast => (
                ScarKind::Dent,
                (event.shell.caliber_mm * 0.0022).clamp(0.12, 0.42),
                (energy_mj.sqrt() * 5.0 * normal_cos).clamp(2.0, event.plate.thickness_mm * 0.28),
            ),
            Terminal::Shatter => (
                ScarKind::Dent,
                (event.shell.caliber_mm * 0.0015).clamp(0.06, 0.22),
                (energy_mj.sqrt() * 3.2 * normal_cos).clamp(1.0, event.plate.thickness_mm * 0.18),
            ),
            Terminal::Bounce | Terminal::Ricochet => (
                ScarKind::Dent,
                (event.shell.caliber_mm * 0.0012).clamp(0.05, 0.18),
                (energy_mj.sqrt() * 2.0 * normal_cos).clamp(0.5, event.plate.thickness_mm * 0.12),
            ),
        };
        let normal = local_normal.try_normalize().unwrap_or_else(|| facing_axis(facing, local_hit));
        let dir = local_dir.try_normalize().unwrap_or(-normal);
        let obliquity_cos = (-dir).dot(normal).clamp(0.05, 1.0);
        let seed = position_seed(local_hit);
        let hole = kind == ScarKind::Hole;
        let geometry = terminal_geometry(class, event.shell.caliber_mm, event.plate.thickness_mm,
            obliquity_cos, event.residual_v, seed);
        // The crater: the resistance model's loss is the cavity the strike leaves. HE dishes the
        // plate over its wider radius with the same loss; it is not cut deeper. A hole's entrance
        // crater is a quarter of the plate before the bore.
        let depth_m = if hole { event.plate.thickness_mm * 0.001 * 0.25 } else { loss_mm * 0.001 };
        let scar = ArmorScar {
            facing,
            local_mm: quantize_mm(local_hit),
            radius_mm: (radius_m * 1000.0).round() as u16,
            loss_mm: loss_mm.round().clamp(1.0, u16::MAX as f32) as u16,
            kind,
            class,
            normal: quantize_dir(normal),
            dir: quantize_dir(dir),
            depth_dmm: (depth_m * 10_000.0).round().clamp(1.0, u16::MAX as f32) as u16,
            plate_mm: event.plate.thickness_mm.round().clamp(1.0, u16::MAX as f32) as u16,
            aperture_mm: if hole { (geometry.aperture_m * 1000.0).round().max(4.0) as u16 } else { 0 },
            back_radius_mm: if hole { (geometry.back_radius_m * 1000.0).round() as u16 } else { 0 },
            petals: if hole { geometry.petals } else { 0 },
            petal_mm: if hole { (geometry.petal_m * 1000.0).round().min(255.0) as u8 } else { 0 },
            plug: hole && geometry.plug,
        };
        self.insert(scar, now_s)
    }

    /// A rifle-calibre bullet (or a fragment) on armor: a splash crater and stripped paint with no
    /// resistance change. `energy_j` is the strike's kinetic energy.
    #[allow(clippy::too_many_arguments)]
    pub fn record_pock(&mut self, facing: Facing, local_hit: Vec3, local_normal: Vec3, local_dir: Vec3,
        caliber_mm: f32, energy_j: f32, plate_mm: f32, now_s: f32) -> ArmorScar {
        let normal = local_normal.try_normalize().unwrap_or_else(|| facing_axis(facing, local_hit));
        let dir = local_dir.try_normalize().unwrap_or(-normal);
        let cosine = (-dir).dot(normal).clamp(0.05, 1.0);
        // A 7.92 mm ball round (~3.6 kJ) leaves a splash crater of ~1-2 mm in RHA; the paint
        // flakes off over about 2.5 calibres.
        let depth_mm = (0.03 * (energy_j * cosine).max(0.0).sqrt()).clamp(0.2, 3.0);
        let scar = ArmorScar {
            facing,
            local_mm: quantize_mm(local_hit),
            radius_mm: (caliber_mm * 1.25).round().clamp(5.0, 30.0) as u16,
            loss_mm: 0,
            kind: ScarKind::Pock,
            class: ShotClass::Bullet,
            normal: quantize_dir(normal),
            dir: quantize_dir(dir),
            depth_dmm: (depth_mm * 10.0).round() as u16,
            plate_mm: plate_mm.round().clamp(1.0, u16::MAX as f32) as u16,
            aperture_mm: 0,
            back_radius_mm: 0,
            petals: 0,
            petal_mm: 0,
            plug: false,
        };
        self.insert(scar, now_s)
    }

    fn insert(&mut self, scar: ArmorScar, now_s: f32) -> ArmorScar {
        self.revision = self.revision.wrapping_add(1);
        self.struck_s.resize(self.scars.len(), f32::NEG_INFINITY);
        let local_hit = scar.local_position();
        let radius_m = scar.radius_m();
        if scar.kind != ScarKind::Pock {
            // Hits inside the same physical wound update it instead of manufacturing decal spam.
            if let Some((index, existing)) = self.scars.iter_mut().enumerate().find(|(_, old)| {
                old.kind != ScarKind::Pock
                    && old.facing == scar.facing
                    && old.local_position().distance(local_hit) <= old.radius_m().max(radius_m) * 0.55
            }) {
                existing.radius_mm = existing.radius_mm.max(scar.radius_mm);
                existing.loss_mm = existing.loss_mm.max(scar.loss_mm);
                existing.depth_dmm = existing.depth_dmm.max(scar.depth_dmm);
                if scar.kind == ScarKind::Hole {
                    let was_open = existing.kind == ScarKind::Hole;
                    existing.kind = ScarKind::Hole;
                    existing.aperture_mm = existing.aperture_mm.max(scar.aperture_mm);
                    existing.back_radius_mm = existing.back_radius_mm.max(scar.back_radius_mm);
                    if !was_open {
                        existing.class = scar.class;
                        existing.dir = scar.dir;
                        existing.petals = scar.petals;
                        existing.petal_mm = scar.petal_mm;
                        existing.plug = scar.plug;
                        existing.plate_mm = scar.plate_mm;
                    }
                }
                self.struck_s[index] = now_s;
                return *existing;
            }
            // A crater swallows the pocks under it.
            let mut index = 0;
            while index < self.scars.len() {
                let old = self.scars[index];
                if old.kind == ScarKind::Pock && old.facing == scar.facing
                    && old.local_position().distance(local_hit) < radius_m * 0.8 {
                    self.scars.remove(index);
                    self.struck_s.remove(index);
                } else {
                    index += 1;
                }
            }
        } else if let Some(index) = self.scars.iter().position(|old| {
            old.facing == scar.facing
                && if old.kind == ScarKind::Pock {
                    old.local_position().distance(local_hit) < 0.004
                } else {
                    old.local_position().distance(local_hit) < old.radius_m() * 0.6
                }
        }) {
            // A bullet into an existing crater or onto the same pock adds nothing visible.
            self.struck_s[index] = now_s;
            return self.scars[index];
        }
        if self.scars.len() >= MAX_ARMOR_SCARS {
            let evict = self.eviction_candidate();
            self.scars.remove(evict);
            self.struck_s.remove(evict);
        }
        self.scars.push(scar);
        self.struck_s.push(now_s);
        scar
    }

    /// The least significant mark: pocks before dents before holes and patches, then the
    /// smallest crater volume, then the oldest.
    fn eviction_candidate(&self) -> usize {
        let rank = |scar: &ArmorScar| match scar.kind {
            ScarKind::Pock => 0u8,
            ScarKind::Dent => 1,
            ScarKind::Hole | ScarKind::Patched => 2,
        };
        let volume = |scar: &ArmorScar| scar.depth_dmm as f32 * (scar.radius_mm as f32).powi(2);
        let mut best = 0;
        for (index, scar) in self.scars.iter().enumerate().skip(1) {
            let old = &self.scars[best];
            if rank(scar) < rank(old) || (rank(scar) == rank(old) && volume(scar) < volume(old)) {
                best = index;
            }
        }
        best
    }

    /// Patch the nearest open penetration. The patch cuts the wound's loss to 45% of the original
    /// plate loss: enough to stop weather, fragments and many light rounds, explicitly not a magical
    /// restoration of homogeneous armor.
    pub fn nearest_hole(&self, local_pos: Vec3, turret: bool, reach_m: f32) -> Option<usize> {
        self.scars
            .iter()
            .enumerate()
            .filter(|(_, scar)| scar.kind == ScarKind::Hole && scar.is_turret() == turret)
            .map(|(index, scar)| (index, scar.local_position().distance(local_pos)))
            .filter(|(_, distance)| *distance <= reach_m)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }

    pub fn patch(&mut self, index: usize) -> bool {
        let Some(scar) = self.scars.get_mut(index) else {
            return false;
        };
        if scar.kind != ScarKind::Hole {
            return false;
        }
        scar.kind = ScarKind::Patched;
        scar.loss_mm = ((scar.loss_mm as f32) * 0.45).round().max(1.0) as u16;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// A field as bytes (`PACKED_SCAR_BYTES` per scar), for checkpoints.
    pub fn pack_all(scars: &[ArmorScar]) -> Vec<u8> {
        scars.iter().flat_map(|scar| scar.pack()).collect()
    }

    pub fn unpack_all(bytes: &[u8]) -> Option<Vec<ArmorScar>> {
        if bytes.len() % PACKED_SCAR_BYTES != 0 || bytes.len() / PACKED_SCAR_BYTES > MAX_ARMOR_SCARS {
            return None;
        }
        bytes.chunks_exact(PACKED_SCAR_BYTES).map(ArmorScar::unpack).collect()
    }

    /// Apply one window of a field streamed in pieces (LAN): the host holds `total` scars and
    /// `start` is the first index this window carries. Scars past `total` are dropped; an entry
    /// that did not change keeps its strike time.
    pub fn apply_window(&mut self, total: usize, start: usize, window: &[ArmorScar]) {
        let total = total.min(MAX_ARMOR_SCARS);
        let mut changed = self.scars.len() > total;
        self.scars.truncate(total);
        self.struck_s.truncate(total);
        for (offset, scar) in window.iter().enumerate() {
            let index = start + offset;
            if index >= total {
                break;
            }
            if index < self.scars.len() {
                if self.scars[index] != *scar {
                    self.scars[index] = *scar;
                    changed = true;
                }
                continue;
            }
            // Earlier indices arrive in a later window; hold their places with this record.
            while self.scars.len() <= index {
                self.scars.push(*scar);
                self.struck_s.push(f32::NEG_INFINITY);
            }
            changed = true;
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

pub fn quantize_mm(value: Vec3) -> [i16; 3] {
    value
        .to_array()
        .map(|v| (v * 1000.0).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16)
}

pub fn quantize_dir(value: Vec3) -> [i8; 3] {
    let v = value.normalize_or_zero();
    v.to_array().map(|c| (c * 127.0).round().clamp(-127.0, 127.0) as i8)
}

fn dequantize_dir(value: [i8; 3]) -> Option<Vec3> {
    Vec3::new(value[0] as f32, value[1] as f32, value[2] as f32).try_normalize()
}

/// A stable per-wound seed from its position (renderer noise, petal count).
pub fn position_seed(local: Vec3) -> u32 {
    let q = quantize_mm(local);
    let mut h = 0x811C_9DC5u32;
    for v in q {
        for byte in v.to_le_bytes() {
            h ^= byte as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
    }
    h
}

pub fn valid_scar(scar: &ArmorScar) -> bool {
    let sized = scar.radius_mm >= 4 && scar.plate_mm > 0;
    let lossy = if scar.kind == ScarKind::Pock { scar.loss_mm == 0 } else { scar.loss_mm > 0 };
    let open = matches!(scar.kind, ScarKind::Hole | ScarKind::Patched);
    sized && lossy && (!open || scar.aperture_mm > 0) && scar.petals <= 8
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let x = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ballistics::{ArmorMaterial, ShellSpec};

    static TEST_SHELL: ShellSpec = ShellSpec {
        name: "test",
        kind: ShellKind::Apcbc,
        caliber_mm: 88.0,
        mass_kg: 10.2,
        muzzle_velocity: 900.0,
        filler_g: 60.0,
        drag_k: 0.0,
        pen_ref_mm: 200.0,
        ref_range_m: 0.0,
    };

    fn event(terminal: Terminal) -> ImpactEvent {
        ImpactEvent {
            shell: &TEST_SHELL,
            terminal,
            striking_v: 800.0,
            impact_angle_deg: 10.0,
            plate: Plate {
                name: "plate",
                thickness_mm: 100.0,
                slope_deg: 0.0,
                material: ArmorMaterial::Rha,
            },
            effective_mm: 100.0,
            pen_mm: 180.0,
            residual_v: 430.0,
            behind_armor: 1.0,
            flight_time: 0.2,
            range_m: 150.0,
        }
    }

    #[test]
    fn penetration_creates_a_local_repeat_hit_weakness() {
        let mut field = ArmorField::default();
        let p = Vec3::new(0.3, 1.0, 2.9);
        field.record(Facing::HullFront, p, &event(Terminal::Penetration));
        let base = event(Terminal::Penetration).plate;
        assert!(field.plate_at(base, Facing::HullFront, p).thickness_mm < 10.0);
        assert_eq!(
            field
                .plate_at(base, Facing::HullFront, p + Vec3::X * 1.0)
                .thickness_mm,
            100.0
        );
    }

    #[test]
    fn patch_restores_some_but_not_factory_resistance() {
        let mut field = ArmorField::default();
        let p = Vec3::new(-0.5, 1.2, 3.0);
        field.record(Facing::HullFront, p, &event(Terminal::Penetration));
        let open = field
            .plate_at(event(Terminal::Penetration).plate, Facing::HullFront, p)
            .thickness_mm;
        let index = field.nearest_hole(p, false, 0.5).unwrap();
        assert!(field.patch(index));
        let patched = field
            .plate_at(event(Terminal::Penetration).plate, Facing::HullFront, p)
            .thickness_mm;
        assert!(patched > open);
        assert!(patched < 70.0);
        assert_eq!(field.scars()[0].kind, ScarKind::Patched);
    }

    #[test]
    fn nearby_impacts_merge_and_capacity_is_bounded() {
        let mut field = ArmorField::default();
        field.record(Facing::HullSide, Vec3::ZERO, &event(Terminal::Bounce));
        field.record(
            Facing::HullSide,
            Vec3::new(0.01, 0.0, 0.0),
            &event(Terminal::Penetration),
        );
        assert_eq!(field.scars().len(), 1);
        assert_eq!(field.scars()[0].kind, ScarKind::Hole);
        for i in 0..(MAX_ARMOR_SCARS + 40) {
            field.record(
                Facing::HullFront,
                Vec3::new((i % 40) as f32 * 0.5, (i / 40) as f32 * 0.5, 0.0),
                &event(Terminal::Bounce),
            );
        }
        assert_eq!(field.scars().len(), MAX_ARMOR_SCARS);
        // Eviction keeps the open hole while lesser dents remain.
        assert!(field.scars().iter().any(|scar| scar.kind == ScarKind::Hole));
    }

    #[test]
    fn pocks_are_inert_and_swallowed_by_a_crater() {
        let mut field = ArmorField::default();
        let base = event(Terminal::Bounce).plate;
        for i in 0..20 {
            field.record_pock(Facing::HullSide, Vec3::new(1.8, 1.0 + i as f32 * 0.01, 0.0), Vec3::X,
                -Vec3::X, 7.92, 3600.0, 80.0, 0.0);
        }
        assert!(field.scars().len() > 10);
        assert!(field.scars().iter().all(|scar| scar.kind == ScarKind::Pock && scar.depth_dmm >= 2));
        assert_eq!(field.plate_at(base, Facing::HullSide, Vec3::new(1.8, 1.05, 0.0)).thickness_mm, 100.0);
        field.record_at(Facing::HullSide, Vec3::new(1.8, 1.1, 0.0), Vec3::X, -Vec3::X,
            &event(Terminal::Bounce), 1.0);
        let pocks = field.scars().iter().filter(|scar| scar.kind == ScarKind::Pock).count();
        assert!(pocks < 20, "the crater replaced the pocks it covers");
        assert_eq!(field.struck_at(field.scars().len() - 1), 1.0);
    }

    #[test]
    fn hole_geometry_follows_calibre_obliquity_and_plate() {
        let mut field = ArmorField::default();
        // 88 mm AP through 80 mm at normal: a plug, no petals, aperture ~ half a calibre.
        let normal = Vec3::X;
        let mut e = event(Terminal::Penetration);
        e.plate.thickness_mm = 80.0;
        let scar = field.record_at(Facing::HullSide, Vec3::new(1.8, 1.2, 0.0), normal, -normal, &e, 0.0);
        assert!(scar.plug && scar.petals == 0);
        assert!((44..=48).contains(&scar.aperture_mm), "{}", scar.aperture_mm);
        assert!(scar.back_radius_mm > scar.aperture_mm);
        // The same round through 25 mm roof plate petals.
        let mut thin = event(Terminal::Penetration);
        thin.plate.thickness_mm = 25.0;
        let roof = field.record_at(Facing::HullFront, Vec3::new(0.0, 1.9, 1.0), Vec3::Y, -Vec3::Y, &thin, 0.0);
        assert!(!roof.plug && roof.petals >= 4 && roof.petal_mm > 20);
        // An oblique dir is stored and gives the obliquity back.
        let dir = Vec3::new(-0.5, -0.866, 0.0);
        let oblique = field.record_at(Facing::HullFront, Vec3::new(0.0, 1.9, -1.0), Vec3::Y, dir, &thin, 0.0);
        assert!((oblique.obliquity_cos() - 0.866).abs() < 0.02);
    }

    #[test]
    fn pack_round_trip_and_v1_upgrade() {
        let mut field = ArmorField::default();
        field.record_at(Facing::TurretFront, Vec3::new(0.1, 0.4, 1.2), Vec3::Z, -Vec3::Z,
            &event(Terminal::Penetration), 2.0);
        field.record_pock(Facing::HullRear, Vec3::new(-0.2, 1.0, -3.0), -Vec3::Z, Vec3::Z, 7.62, 3000.0, 80.0, 2.0);
        let bytes = ArmorField::pack_all(field.scars());
        assert_eq!(bytes.len(), 2 * PACKED_SCAR_BYTES);
        assert_eq!(ArmorField::unpack_all(&bytes).unwrap(), field.scars());
        let mut corrupt = bytes.clone();
        corrupt[1] = 9;
        assert!(ArmorField::unpack_all(&corrupt).is_none());
        let v1 = ArmorScar::from_v1(Facing::HullSide, [-30, 120, -90], 80, 35, ScarKind::Hole);
        assert_eq!(v1.local_mm, [-300, 1200, -900]);
        assert!(v1.local_normal().x < -0.99 && v1.aperture_mm > 0);
        assert!(valid_scar(&v1));
    }

    #[test]
    fn streamed_windows_rebuild_the_field() {
        let mut host = ArmorField::default();
        for i in 0..40 {
            host.record(Facing::HullSide, Vec3::new(1.8, 0.5 + i as f32 * 0.3, 0.0), &event(Terminal::Bounce));
        }
        let mut client = ArmorField::default();
        for start in (0..host.scars().len()).step_by(16).rev() {
            let end = (start + 16).min(host.scars().len());
            client.apply_window(host.scars().len(), start, &host.scars()[start..end]);
        }
        assert_eq!(client, host);
    }
}
