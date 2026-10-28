//! CPU source-geometry regression for stepping around the open escape door onto the hull.
//! The bounded slice retains original triangles, rather than substituting a flat test floor.
use super::*;

fn side_source_slice(mesh: &crate::gltf_load::CpuMesh, model: Mat4) -> crate::gltf_load::CpuMesh {
    let low = Vec3::new(-2.35, 1.0, -2.65);
    let high = Vec3::new(-0.30, 4.3, -0.35);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for triangle in mesh.indices.chunks_exact(3) {
        let local = [0, 1, 2].map(|index| mesh.vertices[triangle[index] as usize]);
        let points = local.map(|vertex| model.transform_point3(Vec3::from(vertex.pos)));
        let triangle_low = points[0].min(points[1]).min(points[2]);
        let triangle_high = points[0].max(points[1]).max(points[2]);
        if triangle_high.cmplt(low).any() || triangle_low.cmpgt(high).any() { continue; }
        for vertex in local {
            let position = Vec3::from(vertex.pos);
            min = min.min(position);
            max = max.max(position);
            indices.push(vertices.len() as u32);
            vertices.push(vertex);
        }
    }
    assert!(!indices.is_empty());
    crate::gltf_load::CpuMesh::basic(vertices, indices, None, min, max)
}

#[test]
fn side_hatch_lands_around_the_open_source_lid_on_real_hull_deck() {
    let (parts, lids, _, _, _, _, _, _) = tiger_hatches::load_player_tiger().unwrap();
    let turret_local = Mat4::from_translation(tank::TIGER_PARTS.turret_pivot);
    let mut open = tiger_hatches::RoofHatches::default();
    open.open[3] = true;
    open.phase[3] = 1.0;
    let side_local = open.models(Mat4::IDENTITY, turret_local)[3];
    let hull = side_source_slice(&parts[0], Mat4::IDENTITY);
    let turret = side_source_slice(&parts[1], turret_local);
    let door = side_source_slice(&lids[3], side_local);
    let source = surface_fire::CabinFire::new(&[(0, &hull), (1, &turret), (15, &door)]);

    // Include the actual Act 1 heading and a translated terrain elevation. World-axis shoulder
    // probes must still find the same supporting deck without relaxing their start height.
    for yaw in [0.0, std::f32::consts::PI] {
        let hull_frame = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw),
            Vec3::new(1850.0, 5.47059, 250.0));
        let turret_frame = hull_frame * turret_local;
        let mut models = [hull_frame; surface_fire::FRAMES];
        models[1] = turret_frame;
        models[15] = hull_frame * side_local;
        let contacts = source.source_contacts(&models);
        let center = turret_frame.transform_point3(tiger_hatches::APERTURE_CENTERS[3]);
        let normal = turret_frame.transform_vector3(tiger_hatches::APERTURE_NORMALS[3]).normalize();
        let tangent = Vec3::Y.cross(normal).normalize();
        let support = |sample: Vec3| {
            contacts.sweep_batch(&[[sample + Vec3::Y * 0.65,
                Vec3::new(sample.x, 5.42, sample.z)]])[0].unwrap()
        };

        let old_hit = support(center + normal * 0.60 - tangent * 0.35);
        assert_eq!(old_hit.frame, 0);
        let old_feet = models[old_hit.frame].transform_point3(old_hit.point);
        let shoulder = hull_frame.transform_vector3(Vec3::Z * crate::foot::BODY_RADIUS);
        let blocked = contacts.sweep_batch(&[[old_feet + shoulder + Vec3::Y * 0.04,
            old_feet + shoulder + Vec3::Y * crate::foot::STANDING_HEIGHT]])[0].unwrap();
        assert_eq!(blocked.frame, 15, "the old landing put the player's leg through the open door");

        // The first production alternative and a rear-deck alternative both have real support,
        // unchanged full standing clearance, and a clear view back through the opening.
        for (outward, sideways) in [(0.45, 0.60), (0.60, -1.0)] {
            let offset = normal * outward + tangent * sideways;
            assert!(hatch_traversal::exit_offsets(normal, true).iter()
                .any(|candidate| candidate.abs_diff_eq(offset, 1e-4)),
                "the bounded production search must include the verified deck step");
            let hit = support(center + offset);
            assert_eq!(hit.frame, 0, "feet must land on hull armor, never the swinging lid");
            let feet = models[hit.frame].transform_point3(hit.point);
            let support_normal = models[hit.frame].transform_vector3(hit.normal).normalize();
            assert!(support_normal.y > 0.90);
            let local_feet = hull_frame.inverse().transform_point3(feet);
            assert!(local_feet.y > 2.03 && local_feet.y < 2.08,
                "the source deck is about 2.05 m high; terrain is not a replacement landing");
            for offset in [Vec3::ZERO, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                let base = feet + offset * crate::foot::BODY_RADIUS;
                assert!(contacts.sweep_batch(&[[base + Vec3::Y * 0.04,
                    base + Vec3::Y * crate::foot::STANDING_HEIGHT]])[0].is_none(),
                    "a verified deck step must clear the full standing body");
            }
            for eye_height in [crate::foot::EYE, crate::foot::EYE_CROUCH] {
                let eye = feet + Vec3::Y * eye_height;
                assert!(contacts.sweep_batch(&[[eye, center + normal * 0.07]])[0].is_none());
            }
            assert!(contacts.sweep_batch(&[[center + normal * 0.25,
                feet + Vec3::Y * crate::foot::EYE]])[0].is_none());
        }
    }
}


#[test]
fn open_roof_entry_recovers_below_cupola_rim_only_through_actual_clear_source_aperture() {
    let (parts, lids, holder, _, _, _, _, _) = tiger_hatches::load_player_tiger().unwrap();
    let hatch = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hatch.glb").unwrap();
    let source = surface_fire::CabinFire::new(&[
        (0, &parts[0]), (1, &parts[1]), (2, &parts[2]), (3, &lids[0]),
        (4, &lids[1]), (5, &holder), (6, &hatch), (14, &lids[2]), (15, &lids[3]),
    ]);
    let center = Vec3::new(tiger_periscopes::COMMANDER_EYE.x, 2.58,
        tiger_periscopes::COMMANDER_EYE.z);
    for yaw in [0.0, 0.7, 2.8] {
        // Translation, hull pitch and turret yaw exercise the source frame rather than world Y.
        let hull = Mat4::from_rotation_translation(
            Quat::from_rotation_y(0.37) * Quat::from_rotation_x(0.08), Vec3::new(14.0, 2.0, -23.0));
        let turret = hull * Mat4::from_translation(tank::TIGER_PARTS.turret_pivot)
            * Mat4::from_rotation_y(yaw);
        for open in [false, true] {
            let mut models = [hull; surface_fire::FRAMES];
            models[1] = turret; models[4] = turret; models[15] = turret;
            models[6] = cupola_hatch_model(turret, if open { 1.0 } else { 0.0 });
            let normal = turret.transform_vector3(Vec3::Y).normalize();
            let world_center = turret.transform_point3(center);
            let contacts = source.source_contacts(&models);
            let blocked = |from, to| contacts.sweep_batch(&[[from, to]])[0].is_some();
            let feet = turret.transform_point3(center - Vec3::Y * 0.12);
            assert!(!crate::foot::cupola_entry_crossing(center - Vec3::Y * 0.10,
                center - Vec3::Y * 0.12, tiger_periscopes::COMMANDER_EYE, open),
                "the old feet-plane crossing misses this already-below-rim fall");
            assert_eq!(crate::foot::open_roof_entry(feet, world_center, normal, 0.25,
                open, blocked), open, "yaw={yaw}, open={open}");
            assert!(!crate::foot::open_roof_entry(
                turret.transform_point3(center + Vec3::X * 0.40 - Vec3::Y * 0.12),
                world_center, normal, 0.25, open, blocked), "neighboring roof is not an aperture");
            assert!(!crate::foot::open_roof_entry(
                turret.transform_point3(center - Vec3::Y * 3.0),
                world_center, normal, 0.25, open, blocked), "underbelly blocks ground-level reconciliation");
            // Starting above a closed lid must land on its real triangles, not enter a cabin.
            let before = turret.transform_point3(center + Vec3::Y * 0.40);
            let mut foot = crate::foot::FootSoldier { pos: feet, airborne: true, vel_y: -2.0,
                ..crate::foot::FootSoldier::default() };
            foot.resolve_solid_motion(before, true, |from, to| {
                contacts.sweep_batch(&[[from, to]])[0].map(|hit|
                    (models[hit.frame].transform_point3(hit.point),
                     models[hit.frame].transform_vector3(hit.normal).normalize()))
            });
            let entered = crate::foot::open_roof_entry(foot.pos, world_center, normal,
                0.25, open, blocked);
            assert_eq!(entered, open, "source-resolved fall yaw={yaw}, open={open}, feet={:?}", foot.pos);
            if !open { assert!((foot.pos - world_center).dot(normal) >= 0.0); }
        }
    }
}


#[test]
fn ordinary_jump_reaches_source_track_and_hull_without_assisted_climb() {
    let (parts, lids, holder, _, _, _, _, _) = tiger_hatches::load_player_tiger().unwrap();
    let source = surface_fire::CabinFire::new(&[(0, &parts[0]), (1, &parts[1]),
        (2, &parts[2]), (3, &lids[0]), (4, &lids[1]), (5, &holder),
        (14, &lids[2]), (15, &lids[3])]);
    let hf = Heightfield { res: 2, spacing: 100.0, heights: vec![0.0; 4],
        min_h: 0.0, max_h: 0.0, openings: Default::default(), detail: None };
    let tiger = Tank::new(0.0, 0.0, 0.0, &hf, tank::TIGER_PARTS);
    let mut models = [tiger.hull_model(); surface_fire::FRAMES];
    models[1] = tiger.turret_model(); models[2] = tiger.gun_model();
    models[4] = models[1]; models[15] = models[1];
    let contacts = source.source_contacts(&models);
    let query = |from, to| contacts.sweep_batch(&[[from, to]])[0].map(|hit|
        (models[hit.frame].transform_point3(hit.point),
         models[hit.frame].transform_vector3(hit.normal).normalize()));
    let mut foot = crate::foot::FootSoldier { pos: Vec3::new(3.0, 0.0, 0.0),
        yaw: -std::f32::consts::FRAC_PI_2, ..crate::foot::FootSoldier::default() };
    for (phase, minimum_height) in [1.3, 2.0, 2.8].into_iter().enumerate() {
        for tick in 0..144 {
            let previous = foot.pos;
            let airborne = foot.airborne;
            foot.update(if tick < (if phase == 0 { 60 } else { 28 }) { 1.0 } else { 0.0 },
                0.0, false, false, tick == 0, 0.0, 0.0, 1.0 / 120.0, &hf);
            foot.resolve_solid_motion(previous, airborne, query);
        }
        println!("source jump phase={phase} feet={:?} grounded={}", foot.pos, !foot.airborne);
        assert!(!foot.airborne && foot.pos.y > minimum_height,
            "ordinary jump {phase} must reach source track/fender, hull, then turret: {:?}", foot.pos);
        let support = source.sweep(foot.pos + Vec3::Y * 0.02,
            foot.pos - Vec3::Y * 0.03, &models).expect("feet must remain on source geometry");
        assert_eq!(support.frame, if phase < 2 { 0 } else { 1 });
    }
}
