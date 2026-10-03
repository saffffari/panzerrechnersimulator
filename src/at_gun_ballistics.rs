//! Kinetic contacts on the gun's actual rendered components. No crew-radius damage is applied.
//! Steel uses the existing RHA solver as a declared surrogate; the coarse gun has no authored
//! steel grade or internal mechanism. Tyre-only penetration remains unsupported (terminal).
use super::*;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub(super) struct GunContact {
    pub index: usize,
    pub part: u8,
    pub normal: Vec3,
    pub chord_m: f32,
    pub steel: bool,
}
struct SourcePart {
    mesh: CpuMesh,
    upper: bool,
    steel: bool,
}
fn source_parts() -> &'static [SourcePart] {
    static PARTS: OnceLock<Vec<SourcePart>> = OnceLock::new();
    PARTS.get_or_init(|| {
        at_guns::lower_parts()
            .into_iter()
            .enumerate()
            .map(|(i, mesh)| SourcePart {
                mesh,
                upper: false,
                steel: i >= 2,
            })
            .chain(at_guns::upper_parts().into_iter().map(|mesh| SourcePart {
                mesh,
                upper: true,
                steel: true,
            }))
            .collect()
    })
}
fn triangle_t(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let q = origin - a;
    let u = q.dot(p) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let r = q.cross(e1);
    let v = direction.dot(r) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(e2.dot(r) / det)
}
fn contains(mesh: &CpuMesh, p: Vec3) -> bool {
    // Every source component is one closed convex box, including its authored trail transform.
    // Face planes come from its actual flat normals/vertices, not an encompassing gun proxy.
    mesh.indices.chunks_exact(3).all(|tri| {
        let v = &mesh.vertices[tri[0] as usize];
        Vec3::from_array(v.normal).dot(p - Vec3::from_array(v.pos)) <= 1e-4
    })
}

pub(super) fn source_contact(
    index: usize,
    gun: &at_guns::AtGun,
    start: Vec3,
    end: Vec3,
    passed: Option<(usize, u8)>,
) -> Option<ProjectileContact> {
    let length = start.distance(end);
    if length <= 1e-7 {
        return None;
    }
    let direction = (end - start) / length;
    let mut best: Option<ProjectileContact> = None;
    for (part, source) in source_parts().iter().enumerate() {
        let model = if source.upper {
            gun.visible_upper()
        } else {
            gun.carriage()
        };
        let inverse = model.inverse();
        let a = inverse.transform_point3(start);
        let d = inverse.transform_vector3(direction);
        if passed == Some((index, part as u8)) && contains(&source.mesh, a) {
            continue;
        }
        if ray_aabb_near(a, d, source.mesh.min, source.mesh.max, length, 0.0).is_none() {
            continue;
        }
        let mut entry = f32::INFINITY;
        let mut exit = f32::INFINITY;
        let mut normal = Vec3::ZERO;
        for tri in source.mesh.indices.chunks_exact(3) {
            let vs = [
                &source.mesh.vertices[tri[0] as usize],
                &source.mesh.vertices[tri[1] as usize],
                &source.mesh.vertices[tri[2] as usize],
            ];
            let n = Vec3::from_array(vs[0].normal);
            let Some(t) = triangle_t(
                a,
                d,
                Vec3::from_array(vs[0].pos),
                Vec3::from_array(vs[1].pos),
                Vec3::from_array(vs[2].pos),
            ) else {
                continue;
            };
            if t < -1e-6 {
                continue;
            }
            if n.dot(d) < -1e-6 && t < entry {
                entry = t.max(0.0);
                normal = n;
            }
            if n.dot(d) > 1e-6 && t < exit {
                exit = t;
            }
        }
        if entry > length || !exit.is_finite() || exit <= entry {
            continue;
        }
        let hit = ProjectileContact {
            t_m: entry,
            pos: start + direction * entry,
            kind: ProjectileContactKind::AtGun(GunContact {
                index,
                part: part as u8,
                normal: model.transform_vector3(normal).normalize(),
                chord_m: exit - entry,
                steel: source.steel,
            }),
        };
        if best.is_none_or(|old| entry < old.t_m) {
            best = Some(hit);
        }
    }
    best
}

/// Consume the source chord using the shared kinetic law. A passed part is excluded only while
/// the shell remains inside that same closed source component. Position is never teleported to
/// its far side: the next ordinary swept query can still hit a body, ground or wall in the path.
pub(super) fn penetrate(projectile: &mut AuthoritativeProjectile, hit: GunContact) -> bool {
    use crate::ballistics::{ArmorMaterial, Plate, ShellKind, Terminal};
    if !hit.steel
        || !matches!(
            projectile.shell.kind,
            ShellKind::Ap | ShellKind::Apcbc | ShellKind::Apcr
        )
    {
        return false;
    }
    let cosine = (-projectile.velocity.normalize_or_zero())
        .dot(hit.normal)
        .abs()
        .clamp(0.001, 1.0);
    let plate = Plate {
        name: "rendered AT-gun steel (RHA surrogate)",
        thickness_mm: hit.chord_m * cosine * 1000.0,
        slope_deg: 0.0,
        material: ArmorMaterial::Rha,
    };
    let event = crate::ballistics::resolve_with_budget_loss(
        projectile.shell,
        projectile.distance_m,
        plate,
        cosine.acos().to_degrees(),
        projectile.seed,
        projectile.penalty_mm,
        projectile.disruption,
    );
    if event.terminal != Terminal::Penetration || event.residual_v <= 0.0 {
        return false;
    }
    let baseline = projectile.shell.velocity_at(projectile.distance_m);
    let retained = event.residual_v.min(baseline * projectile.retained_speed);
    projectile.retained_speed = (retained / baseline).clamp(0.0, 1.0);
    // Keep the next terminal solver's budget consistent with the same retained kinetic speed.
    let remaining =
        projectile.shell.pen_flat_at_v(retained) / (1.0 - projectile.disruption).max(0.001);
    projectile.penalty_mm = (crate::ballistics::budget_mm(projectile.shell, projectile.distance_m)
        - remaining)
        .max(projectile.penalty_mm);
    projectile.velocity = projectile.direction * retained
        - Vec3::Y * (PROJECTILE_GRAVITY_MPS2 * (projectile.age_s-projectile.flight_epoch_s))
        + projectile.crosswind_velocity;
    projectile.passed_at_gun_part = Some((hit.index, hit.part));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shot() -> AuthoritativeProjectile {
        AuthoritativeProjectile::new(
            41,
            ProjectileShooter::Player,
            &crate::ballistics::PZGR_39,
            Vec3::new(0.4, 0.90, 3.0),
            Vec3::NEG_Z,
            919,
        )
    }
    fn contact(
        projectile: &AuthoritativeProjectile,
        gun: &at_guns::AtGun,
        end: Vec3,
    ) -> ProjectileContact {
        source_contact(0, gun, projectile.pos, end, projectile.passed_at_gun_part).unwrap()
    }
    #[test]
    fn at_gun_source_contact_measures_rendered_shield_and_rejects_empty_proxy_space() {
        let gun = at_guns::AtGun::new(Vec3::ZERO, 0.0);
        let p = shot();
        let hit = contact(&p, &gun, Vec3::new(0.4, 0.9, -3.0));
        let ProjectileContactKind::AtGun(source) = hit.kind else {
            panic!()
        };
        assert_eq!(source.part, 5);
        assert!(source.steel);
        assert!((source.chord_m - 0.04).abs() < 1e-5);
        assert!(source.normal.distance(Vec3::Z) < 1e-6);
        assert!((hit.pos.z - 0.19).abs() < 1e-5);
        // The old encompassing gun AABB claimed this empty low centre as solid.
        assert!(segment_local_aabb_contact(
            Vec3::new(0.0, 0.1, 3.0),
            Vec3::new(0.0, 0.1, -0.8),
            at_guns::HIT_MIN,
            at_guns::HIT_MAX
        )
        .is_some());
        assert!(source_contact(
            0,
            &gun,
            Vec3::new(0.0, 0.1, 3.0),
            Vec3::new(0.0, 0.1, -0.8),
            None
        )
        .is_none());
        let barrel = source_contact(
            0,
            &gun,
            Vec3::new(0.0, 0.72, 3.0),
            Vec3::new(0.0, 0.72, 1.0),
            None,
        )
        .unwrap();
        assert!(
            barrel.pos.z > 1.7,
            "rendered barrel beyond the old proxy remains a physical target"
        );
    }
    #[test]
    fn at_gun_ap_continuation_keeps_speed_loss_and_hits_only_the_actual_downstream_body() {
        let gun = at_guns::AtGun::new(Vec3::ZERO, 0.0);
        let mut p = shot();
        let hit = contact(&p, &gun, Vec3::new(0.4, 0.9, -3.0));
        let ProjectileContactKind::AtGun(source) = hit.kind else {
            panic!()
        };
        p.pos = hit.pos;
        p.distance_m = 2.81;
        assert!(penetrate(&mut p, source));
        assert_eq!(
            p.pos, hit.pos,
            "penetration must not teleport past other contacts"
        );
        assert!(p.retained_speed > 0.0 && p.retained_speed < 1.0);
        let speed = p.velocity.length();
        let step = p.swept_step(0.01);
        assert!(step.velocity.length() <= speed + 0.1);
        assert!(
            step.end.distance(step.start)
                < crate::ballistics::PZGR_39.velocity_at(p.distance_m) * 0.01
        );
        assert!(source_contact(0, &gun, step.start, step.end, p.passed_at_gun_part).is_none());
        let feet = Vec3::new(0.4, 0.0, -1.5);
        let mut body = Soldier::new(feet, 0.0, [feet; 4], 17, 0, 1);
        let mut wreck = gun;
        wreck.state = at_guns::GunState::Wrecked;
        assert!(
            source_contact(0, &wreck, step.start, step.end, p.passed_at_gun_part).is_none(),
            "this source shield-to-crew path also clears the actual immediate wreck pose"
        );
        let body_hit = swept_infantry_contact(12, &body, 1.75, step.start, step.end).unwrap();
        assert!(matches!(body_hit.kind, ProjectileContactKind::Infantry(12)));
        let beside = Vec3::new(2.0, 0.0, -1.5);
        let bystander = Soldier::new(beside, 0.0, [beside; 4], 18, 2, 3);
        assert!(swept_infantry_contact(13, &bystander, 1.75, step.start, step.end).is_none());
        let mut actor = crate::infantry_combat::Combatant::new(
            12,
            crate::infantry_combat::Team::Soviet,
            feet,
            0.0,
        );
        assert!(kill_infantry_body(&mut body, Some(&mut actor)));
        assert!(!body.alive && !actor.alive());
        assert!(bystander.alive);
        // A nearer wall is selected before that body by the same shared contact ordering.
        let d = (step.end - step.start).normalize();
        let distance = ray_aabb_near(
            step.start,
            d,
            Vec3::new(0.2, 0.0, -0.45),
            Vec3::new(0.6, 1.5, -0.35),
            step.start.distance(step.end),
            0.0,
        )
        .unwrap();
        let wall = ProjectileContact {
            t_m: distance,
            pos: step.start + d * distance,
            kind: ProjectileContactKind::HardCover,
        };
        let mut nearest = None;
        consider_projectile_contact(&mut nearest, Some(body_hit), 10.0);
        consider_projectile_contact(&mut nearest, Some(wall), 10.0);
        assert!(matches!(
            nearest.unwrap().kind,
            ProjectileContactKind::HardCover
        ));
    }
    #[test]
    fn at_gun_exposed_crew_wins_nearest_contact_and_wreck_geometry_remains_solid() {
        let mut gun = at_guns::AtGun::new(Vec3::ZERO, 0.0);
        let p = shot();
        let end = Vec3::new(0.4, 0.9, -3.0);
        let feet = Vec3::new(0.4, 0.0, 1.0);
        let body = Soldier::new(feet, 0.0, [feet; 4], 17, 0, 1);
        let mut nearest = source_contact(0, &gun, p.pos, end, None);
        consider_projectile_contact(
            &mut nearest,
            swept_infantry_contact(12, &body, 1.75, p.pos, end),
            6.0,
        );
        assert!(matches!(
            nearest.unwrap().kind,
            ProjectileContactKind::Infantry(12)
        ));
        gun.state = at_guns::GunState::Wrecked;
        let model = gun.visible_upper();
        let point = model.transform_point3(Vec3::new(0.4, 0.95, 0.19));
        let normal = model.transform_vector3(Vec3::Z);
        let hit =
            source_contact(0, &gun, point + normal * 2.0, point - normal * 0.1, None).unwrap();
        assert!(hit.pos.distance(point) < 1e-4);
        assert!(!gun.working());
        assert!(!gun.step(
            Some(at_guns::Contact {
                range_m: 180.0,
                bearing: 0.0,
                visible: true,
                alerted: true
            }),
            4,
            30.0
        ));
    }
    #[test]
    fn at_gun_nonpenetrating_strike_has_no_residual_or_invented_ap_blast() {
        let mut p = shot();
        p.penalty_mm = 1000.0;
        let hit = GunContact {
            index: 0,
            part: 5,
            normal: Vec3::Z,
            chord_m: 0.04,
            steel: true,
        };
        assert!(!penetrate(&mut p, hit));
        assert!(p.passed_at_gun_part.is_none());
        assert_eq!(p.retained_speed, 1.0);
        let mut he = shot();
        he.shell = &crate::ballistics::SPRGR_88;
        assert!(!penetrate(&mut he, hit));
    }
}
