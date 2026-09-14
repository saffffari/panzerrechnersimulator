use super::*;

#[test]
fn dropped_ammunition_runtime_is_bounded_in_source_racks_and_breech() {
    use std::hint::black_box;
    use std::time::Instant;
    let (parts, lids, holder, _, sight, _, _, _) = tiger_hatches::load_player_tiger().unwrap();
    let cover = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_part_breechcover.glb").unwrap();
    let source = surface_fire::CabinFire::new(&[
        (0, &parts[0]), (1, &parts[1]), (2, &parts[2]), (3, &lids[0]), (4, &lids[1]),
        (5, &holder), (8, &sight.parts[0]), (12, &cover), (14, &lids[2]), (15, &lids[3]),
    ]);
    let hf = Heightfield { res: 2, spacing: 2000.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0 };
    let tiger = Tank::new(0.0, 0.0, 0.0, &hf, tank::TIGER_PARTS);
    let hull = tiger.hull_model();
    let turret = tiger.turret_model();
    let mut models = [hull; surface_fire::FRAMES];
    models[1] = turret; models[2] = tiger.gun_model(); models[4] = turret;
    models[8] = tiger.gun_model(); models[12] = breech_cover_model(tiger.gun_model(), 0.0);
    models[15] = turret;
    let mut poses = Vec::new();
    for slot in [0, 8, 20, 41] {
        let (frame, position, rotation) = crate::interior_rig::SHELL_SLOTS[slot];
        let parent = if frame == crate::interior_rig::Frame::Hull { hull } else { turret };
        poses.push(parent * Mat4::from_rotation_translation(Quat::from_array(rotation), position));
    }
    // A closed breech and a deliberately buried release exercise the overlap recovery path.
    poses.push(chambered_round_model(tiger.gun_model()));
    poses.push(models[12] * Mat4::from_translation((cover.min + cover.max) * 0.5));
    let mut bodies = Vec::new();
    for (index, pose) in poses.into_iter().enumerate() {
        let (_, rotation, position) = pose.to_scale_rotation_translation();
        for spent in [false, true] {
            let half = if spent { Vec3::new(SPENT_CASE_RADIUS, SPENT_CASE_RADIUS, SPENT_CASE_HALF_LENGTH) }
                else { Vec3::new(crate::procedural_ordnance::LIVE_MAX_DIAMETER_M * 0.5,
                    crate::procedural_ordnance::LIVE_MAX_DIAMETER_M * 0.5,
                    crate::procedural_ordnance::LIVE_LENGTH_M * 0.5) };
            bodies.push((shell_collision::Body { position, rotation, half,
                mass: if spent { 7.0 } else { 15.0 },
                velocity: if index % 2 == 0 { Vec3::ZERO } else { Vec3::new(10000.0, -10000.0, 5000.0) },
                angular_velocity: Vec3::new(5000.0, -4000.0, 6000.0) },
                shell_collision::DropRest::default()));
        }
    }
    let contacts = source.source_contacts(&models);
    let signature = shell_collision::scene_signature(&models);
    let mut times = Vec::new();
    let mut limited = 0;
    let mut most_tests = 0;
    let mut most_queries = 0;
    for frame in 0..120 {
        // A 20-second hitch must never generate 20 seconds of catch-up work.
        let dt = if frame == 0 { 20.0 } else if frame % 20 == 0 { 0.1 } else { 1.0 / 60.0 };
        let mut budget = shell_collision::DropBudget::default();
        let start = Instant::now();
        // Mirror the shipping pool's rotating starting body so deferred drops get turns.
        for offset in 0..bodies.len() {
            let index = (frame + offset) % bodies.len();
            let (body, rest) = &mut bodies[index];
            let report = body.advance_dropped(dt, &contacts, signature, rest, &mut budget, 0.12);
            limited += usize::from(report.budget_limited);
            assert!(body.position.is_finite() && body.rotation.is_finite()
                && body.velocity.is_finite() && body.angular_velocity.is_finite());
            assert!(body.velocity.length() < 12.4, "release momentum escaped its cap at frame {frame}: {body:?}");
            assert!(body.angular_velocity.length() < 12.1, "release spin escaped its cap at frame {frame}: {body:?}");
        }
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        most_tests = most_tests.max(16_384 - budget.tests_left);
        most_queries = most_queries.max(96 - budget.queries_left);
        black_box(&bodies);
    }
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    times.sort_by(f64::total_cmp);
    eprintln!("[dropped-ammunition-cpu] 12 source bodies/120 frames: mean={mean:.3}ms p95={:.3}ms max={:.3}ms max_tests={most_tests} max_queries={most_queries} deferred={limited}",
        times[114], times[119]);
    assert!(most_tests <= 16_384 && most_queries <= 96);
}

#[test]
fn dropped_case_sleeps_on_source_shelf_and_wakes_when_the_shelf_moves() {
    let floor = crate::prims::box_mesh(Vec3::new(-2.0, -0.1, -2.0), Vec3::new(2.0, 0.0, 2.0), [80; 3]);
    let shelf = crate::prims::box_mesh(Vec3::new(-0.5, 0.8, -0.5), Vec3::new(0.5, 0.82, 0.5), [80; 3]);
    let source = surface_fire::CabinFire::new(&[(0, &floor), (1, &shelf)]);
    let mut models = [Mat4::IDENTITY; surface_fire::FRAMES];
    let mut body = shell_collision::Body { position: Vec3::new(0.0, 1.3, 0.0), rotation: Quat::IDENTITY,
        velocity: Vec3::ZERO, angular_velocity: Vec3::ZERO,
        half: Vec3::new(SPENT_CASE_RADIUS, SPENT_CASE_RADIUS, SPENT_CASE_HALF_LENGTH), mass: 7.0 };
    let mut rest = shell_collision::DropRest::default();
    for _ in 0..240 {
        body.advance_dropped(1.0 / 60.0, &source.source_contacts(&models),
            shell_collision::scene_signature(&models), &mut rest, &mut shell_collision::DropBudget::default(), 0.1);
    }
    assert!(rest.sleeping, "a settled case kept consuming contact work: {body:?}");
    assert!((body.position.y - (0.82 + SPENT_CASE_RADIUS)).abs() < 0.006);
    let mut budget = shell_collision::DropBudget::default();
    body.advance_dropped(20.0, &source.source_contacts(&models), shell_collision::scene_signature(&models),
        &mut rest, &mut budget, 0.1);
    assert_eq!((budget.tests_left, budget.queries_left), (16_384, 96), "sleeping body queried the scene");
    models[1] = Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0));
    body.advance_dropped(1.0 / 60.0, &source.source_contacts(&models), shell_collision::scene_signature(&models),
        &mut rest, &mut budget, 0.1);
    assert!(!rest.sleeping, "moving source geometry failed to wake the case");
    for _ in 0..180 {
        body.advance_dropped(1.0 / 60.0, &source.source_contacts(&models),
            shell_collision::scene_signature(&models), &mut rest, &mut shell_collision::DropBudget::default(), 0.1);
    }
    assert!((body.position.y - SPENT_CASE_RADIUS).abs() < 0.006,
        "case did not fall onto the remaining source floor: {body:?}");
    assert!(rest.sleeping);
}

/// Bounded CPU-only reproduction of picking up and turning source rack rounds. This runs the
/// actual articulated hull/turret/gun contact meshes, without opening a second game or using GPU.
#[test]
#[ignore = "manual timing comparison of held round collision against the actual Tiger source"]
fn held_shell_source_cpu_benchmark() {
    use std::time::Instant;
    use std::hint::black_box;
    let (parts, lids, holder, _, sight, _, _, _) = tiger_hatches::load_player_tiger().unwrap();
    let cover=crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_part_breechcover.glb").unwrap();
    let source=surface_fire::CabinFire::new(&[
        (0,&parts[0]),(1,&parts[1]),(2,&parts[2]),(3,&lids[0]),(4,&lids[1]),
        (5,&holder),(8,&sight.parts[0]),(12,&cover),(14,&lids[2]),(15,&lids[3]),
    ]);
    let hf=Heightfield {res:2,spacing:2000.0,heights:vec![0.0;4],min_h:0.0,max_h:0.0};
    let tiger=Tank::new(0.0,0.0,0.0,&hf,tank::TIGER_PARTS);
    let hull=tiger.hull_model();
    let turret=tiger.turret_model();
    let mut models=[hull;surface_fire::FRAMES];
    models[1]=turret; models[2]=tiger.gun_model(); models[4]=turret;
    models[8]=tiger.gun_model(); models[12]=breech_cover_model(tiger.gun_model(),1.0);
    models[15]=turret;
    let radius=crate::procedural_ordnance::LIVE_MAX_DIAMETER_M*0.5;
    for slot in [0,8,20,41] {
        let (frame,position,q)=crate::interior_rig::SHELL_SLOTS[slot];
        let parent=if frame==crate::interior_rig::Frame::Hull {hull} else {turret};
        let (_,rotation,position)=(parent*Mat4::from_rotation_translation(Quat::from_array(q),position))
            .to_scale_rotation_translation();
        let mut body=shell_collision::Body {position,rotation,velocity:Vec3::ZERO,
            angular_velocity:Vec3::ZERO,half:Vec3::new(radius,radius,
                crate::procedural_ordnance::LIVE_LENGTH_M*0.5),mass:15.0};
        let target_position=turret.transform_point3(Vec3::new(-0.45,1.7,-0.40));
        let mut times=Vec::new(); let mut query_ms=0.0; let mut total_triangles=0;
        for step in 0..48 {
            let drive=shell_collision::Carry {position:target_position,
                rotation:Quat::from_rotation_y(0.9+(step as f32*0.08).sin()*0.5),
                stiffness:CARRY_K_ROUND,damping:CARRY_D_ROUND};
            let start=Instant::now();
            let (lo,hi)=body.query_bounds(1.0/60.0,Some(drive));
            let triangles=source.source_contacts(&models).world_triangles(lo,hi);
            query_ms+=start.elapsed().as_secs_f64()*1000.0;
            total_triangles+=triangles.len();
            body.advance(1.0/60.0,&triangles,Some(drive),0.12);
            times.push(start.elapsed().as_secs_f64()*1000.0);
            black_box(body);
        }
        let mean=times.iter().sum::<f64>()/times.len() as f64;
        times.sort_by(f64::total_cmp);
        eprintln!("[held-shell-cpu] rack {slot}: mean={mean:.3}ms p95={:.3}ms max={:.3}ms query={:.3}ms triangles={} final={body:?}",
            times[45],times[47],query_ms/48.0,total_triangles/48);
    }
}

#[test]
fn finite_case_hits_retained_hull_triangles_at_translated_rotated_pose() {
    let (parts, _, _, _, _, _, _, _) = tiger_hatches::load_player_tiger().unwrap();
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
