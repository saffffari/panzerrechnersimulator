//! Controlled terminal-impact seam for repeatable interior review films.
//!
//! This stages the incoming state at an explicitly selected turret OBB through the common part
//! contact helper and terminal resolver, using the catalogue's range velocity law. The unfiltered
//! whole-vehicle contact is reported too: its hull envelope includes empty upper corners that can
//! incorrectly precede the turret. This is an isolated-part bench, not an external flight replay.
//! No casualty filtering, repeated strikes, selected kill outcome or hatch override occurs here.

use super::*;

#[derive(Clone, Debug)]
pub struct ReviewArmorContact {
    pub part: &'static str,
    pub facing: &'static str,
    pub world_m: [f32; 3],
    pub part_local_m: [f32; 3],
    pub world_normal: [f32; 3],
    pub distance_along_probe_m: f32,
}

impl ReviewArmorContact {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "part": self.part, "facing": self.facing, "world_m": self.world_m,
            "part_local_m": self.part_local_m, "world_normal": self.world_normal,
            "distance_along_probe_m": self.distance_along_probe_m,
        })
    }

    fn from_contact(contact: ProjectileContact) -> Self {
        let ProjectileContactKind::Vehicle(armor) = contact.kind else {
            unreachable!("armor-part probe returns a vehicle contact");
        };
        use crate::ballistics::Facing;
        Self {
            part: match armor.part {
                ArmorPart::Hull => "Hull",
                ArmorPart::Turret => "Turret",
            },
            facing: match armor.facing {
                Facing::HullFront => "HullFront",
                Facing::HullSide => "HullSide",
                Facing::HullRear => "HullRear",
                Facing::TurretFront => "TurretFront",
                Facing::TurretSide => "TurretSide",
                Facing::TurretRear => "TurretRear",
            },
            world_m: contact.pos.to_array(),
            part_local_m: armor.local_point.to_array(),
            world_normal: armor.world_normal.to_array(),
            distance_along_probe_m: contact.t_m,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReviewImpactContact {
    pub collision_scope: &'static str,
    pub selected: ReviewArmorContact,
    pub unfiltered_vehicle: Option<ReviewArmorContact>,
    pub target_turret_local_m: [f32; 3],
    pub direction_world: [f32; 3],
    pub projectile_seed: u32,
    pub armor_input_seed: u32,
}

impl ReviewImpactContact {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "collision_scope": self.collision_scope,
            "selected": self.selected.to_json(),
            "unfiltered_vehicle": self.unfiltered_vehicle.as_ref().map(ReviewArmorContact::to_json),
            "target_turret_local_m": self.target_turret_local_m,
            "direction_world": self.direction_world,
            "projectile_seed": self.projectile_seed,
            "armor_input_seed": self.armor_input_seed,
        })
    }
}

/// Choose a side on the requested turret independently of caliber or consequences. This changes
/// no global collider bounds and does not search poses or seeds for a casualty.
fn review_turret_side_contact(
    tank: &Tank,
    target_local: Vec3,
    physical_left: bool,
) -> anyhow::Result<(ProjectileContact, Option<ProjectileContact>, Vec3)> {
    let tm = tank.turret_model();
    let target = tm.transform_point3(target_local);
    let outward = tm
        .transform_vector3(if physical_left { Vec3::X } else { -Vec3::X })
        .normalize();
    let start = target + outward * 8.0;
    let end = target - outward * 8.0;
    let scheme = crate::ballistics::ArmorScheme::TigerI;
    let contact = swept_vehicle_armor_part_contact(
        tank,
        ProjectileVehicle::Player,
        scheme,
        ArmorPart::Turret,
        start,
        end,
    )
    .ok_or_else(|| anyhow::anyhow!("review line missed the selected turret armor part"))?;
    anyhow::ensure!(
        matches!(
            contact.kind,
            ProjectileContactKind::Vehicle(VehicleArmorContact {
                part: ArmorPart::Turret,
                facing: crate::ballistics::Facing::TurretSide,
                ..
            })
        ),
        "review line did not reach a turret-side plate"
    );
    let unfiltered =
        swept_vehicle_armor_contact(tank, ProjectileVehicle::Player, scheme, start, end);
    Ok((contact, unfiltered, -outward))
}

impl Game {
    /// Compatibility wrapper: an isolated turret-side strike. New captures should use the metadata
    /// method below so the selected part and the unfiltered coarse-vehicle contact are recorded.
    pub fn debug_review_side_impact(
        &mut self,
        r: &mut Renderer,
        shell: &'static crate::ballistics::ShellSpec,
        range_m: f32,
        physical_left: bool,
        seed: u32,
    ) -> anyhow::Result<Vec3> {
        let report =
            self.debug_review_turret_side_impact(r, shell, range_m, physical_left, seed)?;
        Ok(Vec3::from(report.selected.world_m))
    }

    /// One unfiltered terminal outcome against a selected turret-side OBB. This is not the
    /// whole-vehicle earliest contact: both contacts are returned so the diagnostic boundary is
    /// explicit. The normal terminal/armor/anatomy/audio resolver remains authoritative.
    pub fn debug_review_turret_side_impact(
        &mut self,
        r: &mut Renderer,
        shell: &'static crate::ballistics::ShellSpec,
        range_m: f32,
        physical_left: bool,
        seed: u32,
    ) -> anyhow::Result<ReviewImpactContact> {
        anyhow::ensure!(
            range_m.is_finite() && range_m >= 1.0,
            "invalid review range"
        );
        anyhow::ensure!(self.player.alive, "review strike requires a live tank");
        let role = if physical_left {
            crate::lan::LanRole::Gunner
        } else {
            crate::lan::LanRole::Loader
        };
        let (_, segments) = crew_pose_segments_in_frame(&self.player, role, BloodFrame::Turret);
        // Keep the exact same target line across calibers. The current contact volumes are the
        // game's authored pose-matched OBBs; these are not claimed to be anatomical ground truth.
        let target_local = segments[crew_physics::CrewBodyPart::Head.index()].pos;
        let (contact, unfiltered, direction) =
            review_turret_side_contact(&self.player, target_local, physical_left)?;
        let report = ReviewImpactContact {
            collision_scope: "isolated_turret_obb_using_common_part_contact_and_terminal_resolver",
            selected: ReviewArmorContact::from_contact(contact),
            unfiltered_vehicle: unfiltered.map(ReviewArmorContact::from_contact),
            target_turret_local_m: target_local.to_array(),
            direction_world: direction.to_array(),
            projectile_seed: seed,
            armor_input_seed: seed ^ self.armor_tick.wrapping_add(1),
        };
        let origin = contact.pos - direction * range_m;
        let mut projectile = AuthoritativeProjectile::new(
            0xA57A_0000_0000 | u64::from(seed),
            ProjectileShooter::Enemy(usize::MAX),
            shell,
            origin,
            direction,
            seed,
        );
        projectile.pos = contact.pos;
        projectile.distance_m = range_m;
        projectile.age_s = shell.time_of_flight(range_m);
        projectile.velocity = direction * shell.velocity_at(range_m);
        self.god = false;
        let hits_before = self.stat_hits_on_player;
        self.resolve_authoritative_terminal(&projectile, contact, r);
        anyhow::ensure!(
            self.stat_hits_on_player == hits_before + 1,
            "review terminal did not record exactly one strike"
        );
        anyhow::ensure!(
            self.last_player_damage_sound.is_some(),
            "review strike has no live damage/audio record"
        );
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tiger(heading: f32, turret_yaw: f32) -> Tank {
        let hf = Heightfield {
            res: 2,
            spacing: 2_000.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
        };
        let mut tank = Tank::new(13.0, -7.0, heading, &hf, tank::TIGER_PARTS);
        tank.turret_yaw = turret_yaw;
        tank
    }

    #[test]
    fn review_both_turret_sides_are_stable_under_independent_rig_rotation() {
        for heading in [0.0, 0.6] {
            for yaw in [0.0, -0.7, 1.2] {
                let tank = test_tiger(heading, yaw);
                for left in [false, true] {
                    let target = Vec3::new(if left { 0.6 } else { -0.6 }, 1.75, -0.2);
                    let (contact, _, direction) =
                        review_turret_side_contact(&tank, target, left).unwrap();
                    let ProjectileContactKind::Vehicle(armor) = contact.kind else {
                        panic!("vehicle")
                    };
                    assert_eq!(armor.part, ArmorPart::Turret);
                    assert_eq!(armor.facing, crate::ballistics::Facing::TurretSide);
                    let (min, max) =
                        rig_armor_bounds(crate::ballistics::ArmorScheme::TigerI, ArmorPart::Turret)
                            .unwrap();
                    assert!((armor.local_point.x - if left { max.x } else { min.x }).abs() < 1e-4);
                    assert!((armor.local_point.y - target.y).abs() < 1e-4);
                    assert!((armor.local_point.z - target.z).abs() < 1e-4);
                    assert!(armor.world_normal.dot(direction) < -0.9999);
                }
            }
        }
    }

    #[test]
    fn review_reports_coarse_hull_overlap_instead_of_hiding_it() {
        let tank = test_tiger(0.0, 0.0);
        for left in [false, true] {
            let target = Vec3::new(if left { 0.6 } else { -0.6 }, 1.75, -0.2);
            let (selected, unfiltered, _) =
                review_turret_side_contact(&tank, target, left).unwrap();
            let unfiltered = unfiltered.expect("known overlapping hull envelope");
            let first = ReviewArmorContact::from_contact(unfiltered);
            assert_eq!(first.part, "Hull");
            assert_eq!(first.facing, "HullSide");
            assert!(unfiltered.t_m < selected.t_m);
            assert_eq!(ReviewArmorContact::from_contact(selected).part, "Turret");
        }
    }
}
