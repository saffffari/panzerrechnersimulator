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
    let (parts, lids, _, _, _, _, _) = tiger_hatches::load_player_tiger().unwrap();
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
