use super::*;

#[test]
fn finite_case_hits_retained_hull_triangles_at_translated_rotated_pose() {
    let (parts, _, _, _, _, _) = tiger_hatches::load_player_tiger().unwrap();
    let wall = parts[0].indices.chunks_exact(3).filter_map(|ids| {
        let tri = [0,1,2].map(|i| Vec3::from(parts[0].vertices[ids[i] as usize].pos));
        let normal = (tri[1]-tri[0]).cross(tri[2]-tri[0]);
        let center = (tri[0]+tri[1]+tri[2])/3.0;
        (normal.normalize_or_zero().x.abs()>0.8 && center.y>0.75 && center.y<2.5)
            .then_some((normal.length(), tri))
    }).max_by(|a,b|a.0.total_cmp(&b.0)).expect("authored hull side triangle").1;
    let source = surface_fire::CabinFire::new(&[(0,&parts[0])]);
    let mut models = [Mat4::IDENTITY; surface_fire::FRAMES];
    models[0] = Mat4::from_translation(Vec3::new(17.0,2.0,-12.0))*Mat4::from_rotation_y(0.63);
    let tri = wall.map(|p|models[0].transform_point3(p));
    let normal = (tri[1]-tri[0]).cross(tri[2]-tri[0]).normalize();
    let point = (tri[0]+tri[1]+tri[2])/3.0;
    let mut body = shell_collision::Body {
        position: point + normal * (SPENT_CASE_HALF_LENGTH+0.18),
        rotation: Quat::from_rotation_arc(Vec3::Z,normal),
        velocity: -normal*60.0, angular_velocity: Vec3::ZERO,
        half: Vec3::new(SPENT_CASE_RADIUS,SPENT_CASE_RADIUS,SPENT_CASE_HALF_LENGTH), mass: 7.0,
    };
    let (lo,hi) = body.query_bounds(0.04,None);
    let triangles = source.source_contacts(&models).world_triangles(lo,hi);
    assert!(!triangles.is_empty());
    let report = body.advance(0.04,&triangles,None,0.0);
    assert!(report.impact_speed>0.0);
    assert!((body.position-point).dot(normal)>=SPENT_CASE_HALF_LENGTH-0.003,
        "full case crossed the retained hull wall: {body:?}");
}

#[test]
fn finite_round_uses_actual_open_and_closed_moving_breech_block() {
    let mesh = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_part_breechcover.glb").unwrap();
    let source = surface_fire::CabinFire::new(&[(12,&mesh)]);
    let gun = Mat4::from_translation(Vec3::new(12.0,3.0,-5.0))*Mat4::from_rotation_y(0.4)
        *Mat4::from_rotation_x(-0.1);
    let chamber = chambered_round_model(gun);
    let (_,rotation,seat)=chamber.to_scale_rotation_translation();
    let start=chamber.transform_point3(crate::interior_rig::ROUND_READY-crate::interior_rig::ROUND_SEAT);
    for opening in [0.0,1.0] {
        let mut models=[Mat4::IDENTITY;surface_fire::FRAMES];
        models[12]=breech_cover_model(gun,opening);
        let radius=crate::procedural_ordnance::LIVE_MAX_DIAMETER_M*0.5;
        let mut body=shell_collision::Body { position:start,rotation,velocity:Vec3::ZERO,
            angular_velocity:Vec3::ZERO,half:Vec3::new(radius,radius,crate::procedural_ordnance::LIVE_LENGTH_M*0.5),mass:15.0 };
        let margin=Vec3::splat(body.half.length()+0.01);
        let triangles=source.source_contacts(&models).world_triangles(start.min(seat)-margin,start.max(seat)+margin);
        body.constrain_pose(seat,rotation,&triangles);
        if opening==0.0 {
            assert!(body.position.distance(seat)>0.2,"closed source block must stop the round");
        } else {
            assert!(body.position.distance(seat)<0.003,"actual open block must clear finite round: {body:?}");
        }
    }
}
