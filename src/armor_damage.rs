//! Persistent, spatial armor damage for the player Tiger.
//!
//! A scar is deliberately compact and quantized: it can live in campaign checkpoints and LAN
//! snapshots without serializing renderer state.  Ballistics asks the field for the plate at the
//! actual hit point; repeated fire into a crater, open penetration or field patch therefore sees a
//! different thickness.  Rendering and repair consume the same records.

use glam::Vec3;

use crate::ballistics::{Facing, ImpactEvent, Plate, Terminal};

pub const MAX_ARMOR_SCARS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ScarKind {
    Dent = 0,
    Hole = 1,
    Patched = 2,
}

impl ScarKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Dent),
            1 => Some(Self::Hole),
            2 => Some(Self::Patched),
            _ => None,
        }
    }
}

/// One localized wound. Position is centimetres in hull-local or turret-local space (selected by
/// `facing`); radius and lost resistance are millimetres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmorScar {
    pub facing: Facing,
    pub local_cm: [i16; 3],
    pub radius_mm: u16,
    pub loss_mm: u16,
    pub kind: ScarKind,
}

impl ArmorScar {
    pub fn local_position(self) -> Vec3 {
        Vec3::new(
            self.local_cm[0] as f32,
            self.local_cm[1] as f32,
            self.local_cm[2] as f32,
        ) * 0.01
    }

    pub fn radius_m(self) -> f32 {
        self.radius_mm as f32 * 0.001
    }

    pub fn is_turret(self) -> bool {
        matches!(
            self.facing,
            Facing::TurretFront | Facing::TurretSide | Facing::TurretRear
        )
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArmorField {
    scars: Vec<ArmorScar>,
}

impl ArmorField {
    pub fn from_scars(mut scars: Vec<ArmorScar>) -> Self {
        scars.truncate(MAX_ARMOR_SCARS);
        scars.retain(valid_scar);
        Self { scars }
    }

    pub fn scars(&self) -> &[ArmorScar] {
        &self.scars
    }

    /// Apply the strongest nearby wound with a soft radial falloff. Multiple tiny dents do not
    /// magically add up across unrelated points, while a shot placed into the same hole sees the
    /// open path. A field patch restores useful splinter/weather protection but never factory armor.
    pub fn plate_at(&self, base: Plate, facing: Facing, local_hit: Vec3) -> Plate {
        let mut worst_loss = 0.0f32;
        for scar in self.scars.iter().filter(|scar| scar.facing == facing) {
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

    /// Record the permanent plate response after the ballistic result has been decided.
    pub fn record(&mut self, facing: Facing, local_hit: Vec3, event: &ImpactEvent) -> ArmorScar {
        let escaped_v2=if event.terminal==Terminal::Ricochet {event.residual_v.powi(2)}else{0.0};
        let energy_mj = 0.5 * event.shell.mass_kg * (event.striking_v.powi(2)-escaped_v2).max(0.0) / 1_000_000.0;
        let normal = event.impact_angle_deg.to_radians().cos().clamp(0.12, 1.0);
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
                (energy_mj.sqrt() * 5.0 * normal).clamp(2.0, event.plate.thickness_mm * 0.28),
            ),
            Terminal::Shatter => (
                ScarKind::Dent,
                (event.shell.caliber_mm * 0.0015).clamp(0.06, 0.22),
                (energy_mj.sqrt() * 3.2 * normal).clamp(1.0, event.plate.thickness_mm * 0.18),
            ),
            Terminal::Bounce | Terminal::Ricochet => (
                ScarKind::Dent,
                (event.shell.caliber_mm * 0.0012).clamp(0.05, 0.18),
                (energy_mj.sqrt() * 2.0 * normal).clamp(0.5, event.plate.thickness_mm * 0.12),
            ),
        };
        let scar = ArmorScar {
            facing,
            local_cm: quantize_cm(local_hit),
            radius_mm: (radius_m * 1000.0).round() as u16,
            loss_mm: loss_mm.round().clamp(1.0, u16::MAX as f32) as u16,
            kind,
        };

        // Hits inside the same physical wound update it instead of manufacturing decal spam.
        if let Some(existing) = self.scars.iter_mut().find(|old| {
            old.facing == facing
                && old.local_position().distance(local_hit) <= old.radius_m().max(radius_m) * 0.55
        }) {
            existing.radius_mm = existing.radius_mm.max(scar.radius_mm);
            existing.loss_mm = existing.loss_mm.max(scar.loss_mm);
            if scar.kind == ScarKind::Hole {
                existing.kind = ScarKind::Hole;
            }
            return *existing;
        }
        if self.scars.len() == MAX_ARMOR_SCARS {
            self.scars.remove(0);
        }
        self.scars.push(scar);
        scar
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
        true
    }
}

fn quantize_cm(value: Vec3) -> [i16; 3] {
    value
        .to_array()
        .map(|v| (v * 100.0).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16)
}

fn valid_scar(scar: &ArmorScar) -> bool {
    scar.radius_mm >= 20 && scar.loss_mm > 0
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let x = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ballistics::{ArmorMaterial, ShellKind, ShellSpec};

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
        for i in 0..40 {
            field.record(
                Facing::HullFront,
                Vec3::new(i as f32, 0.0, 0.0),
                &event(Terminal::Bounce),
            );
        }
        assert_eq!(field.scars().len(), MAX_ARMOR_SCARS);
    }
}
