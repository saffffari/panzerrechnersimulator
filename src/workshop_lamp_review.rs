//! Additional live phase for the existing workshop harness; all positioning is disclosed.
use super::*;

fn face_walk(g:&mut Game,r:&mut Renderer,target:Vec3) {
    let mut wanted=r.camera;wanted.set_forward(target-wanted.pos);
    let yaw=(wanted.yaw-r.camera.yaw+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
    let input=TankInput{look_dx:yaw/0.0022,look_dy:-(wanted.pitch-r.camera.pitch)/0.0022,..Default::default()};
    g.update(&input,DT,r);g.update(&TankInput::default(),DT,r);
}

pub(super) fn run_lamp(g:&mut Game,r:&mut Renderer,checks:&mut Vec<Value>,stages:&mut Vec<Value>,
    evidence:&mut Value,mut photo:impl FnMut(&mut Renderer,usize)->Result<()>)->Result<()> {
    let before_ids=ids(g);let before_ledger=ledger_ids(g);
    let person=g.expedition_state().context("no expedition")?.controlled_person.context("no keeper")?;
    let item=ItemId(799_901);
    let root=g.player.hull_model();let at=root.transform_point3(Vec3::new(3.0,0.0,0.0));
    let ground=Vec3::new(at.x,g.hf.height_at(at.x,at.z),at.z);
    g.spawn_workshop_reward(item,PartKind::WorkLamp.key(),ground)?;
    g.update(&TankInput::default(),DT,r);
    let bank=g.workshop.state.parts.iter().find(|p|p.item==item).context("lamp fixture absent")?.bank;
    let center=g.workshop.models[bank].transform_point3(g.workshop.banks[bank].center);
    stage_foot(g,r,ground+root.transform_vector3(Vec3::X)*0.6,center);
    let point=pick_point(g,bank).context("lamp source not reachable")?;
    g.dbg_look_foot_at(point);g.update(&TankInput::default(),DT,r);
    g.update(&TankInput{trigger:true,fire2:true,..Default::default()},DT,r);
    g.update(&TankInput::default(),DT,r);
    check(checks,"ordinary source pickup holds one finite lamp",g.workshop.state.held().is_some_and(|p|
        p.item==item&&matches!(p.location,PartLocation::Hands{person:owner} if owner==person)))?;
    evidence["lamp_fixture"]=json!({"item":item,"bank":bank,"ground_m":ground.to_array(),
        "staging":"One finite source lamp commissioned; existing keeper's grounded pickup and return to commander are staged. Interior exit and subsequent approach use ordinary station and walking controls."});
    g.mode=Mode::Tank;g.chase_cam=false;
    // This bracket is on the opposite wall, beyond the1.65m retainer reach from the cupola.
    // Leave the seat and approach through the existing walk clamp; never enlarge reach or
    // move the mount/collision to make a seated witness pass.
    g.jump_station(Station::Loader);
    for _ in 0..240 {g.update(&TankInput::default(),DT,r);if g.switch_t<=0.0 {break;}}
    check(checks,"keeper leaves cupola through ordinary interior station action",g.station==Station::Loader&&g.switch_t<=0.0)?;
    let walk_start=g.walk_pos;let walk_goal=Vec3::new(-0.30,walk_start.y,-0.50);let mut walk_ticks=0;
    while walk_ticks<240&&Vec3::new(g.walk_pos.x-walk_goal.x,0.0,g.walk_pos.z-walk_goal.z).length()>0.04 {
        let eye_local=g.player.turret_model().inverse().transform_point3(r.camera.pos);
        let point=g.player.turret_model().transform_point3(Vec3::new(walk_goal.x,eye_local.y,walk_goal.z));
        face_walk(g,r,point);g.update(&TankInput{drive:1.0,..Default::default()},DT,r);walk_ticks+=1;
    }
    g.update(&TankInput::default(),DT,r);
    evidence["lamp_interior_approach"]=json!({"start_turret_m":walk_start.to_array(),
        "end_turret_m":g.walk_pos.to_array(),"ordinary_walk_ticks":walk_ticks,
        "staged_interior_translation":false});
    check(checks,"keeper physically approaches lamp mount through ordinary walking",g.walk_pos.distance(walk_start)>0.25)?;
    let target=g.workshop.mount_point(15,g.player.hull_model(),g.player.turret_model(),g.sight_model());
    face_walk(g,r,target);
    evidence["lamp_mount_reach"]=json!({"eye_m":r.camera.pos.to_array(),"target_m":target.to_array(),
        "distance_m":r.camera.pos.distance(target),"source_path_clear":g.workshop_clear_to(r.camera.pos,target)});
    check(checks,"lamp bracket reachable from actual interior walk eye",g.workshop_mount_target(r.camera.pos,r.camera.forward(),Some(PartKind::WorkLamp))==Some(15))?;
    operate(g,r,15,item,Operation::Insert,stages,checks)?;
    check(checks,"unsecured lamp cannot emit light",g.workshop_light().is_none())?;
    operate(g,r,15,item,Operation::Secure,stages,checks)?;
    check(checks,"secured lamp still requires function check",g.workshop_light().is_none())?;
    operate(g,r,15,item,Operation::Test,stages,checks)?;
    check(checks,"tested lamp uses actual source lens emitter",g.workshop_light().is_some_and(|(light,lens)|
        lens==g.workshop.banks[bank].slots[1]&&light.pos.distance(g.workshop.models[bank]
            .transform_point3(Vec3::new(0.0,0.0,0.035)))<1e-5))?;
    photo(r,7)?;
    let before=g.workshop.state.part_at(15).unwrap().energy_joules;
    for _ in 0..120 {g.update(&TankInput::default(),DT,r);}
    let after=g.workshop.state.part_at(15).unwrap().energy_joules;
    check(checks,"actual lit lamp consumes only its finite battery",before>after&&(before-after-6.0).abs()<0.3)?;
    // The switch belongs to the physical lamp body; choose a clear real source triangle.
    let mut switch=None;let eye=r.camera.pos;
    for mesh in &g.workshop.banks[bank].meshes {for t in mesh.indices.chunks_exact(3) {
        let p=t.iter().map(|i|g.workshop.models[bank].transform_point3(Vec3::from(mesh.vertices[*i as usize].pos))).sum::<Vec3>()/3.0;
        let look=(p-eye).normalize_or_zero();
        if g.workshop.ray_part(eye,look,1.85).is_some_and(|(hit,d)|hit==bank&&g.workshop_clear_to(eye,eye+look*d)) {switch=Some(p);break;}
    }if switch.is_some(){break;}}
    let switch=switch.context("installed lamp switch surface unreachable")?;face_walk(g,r,switch);
    g.update(&TankInput{trigger:true,fire2:true,..Default::default()},DT,r);
    g.update(&TankInput::default(),DT,r);
    check(checks,"ordinary lamp body press switches emitter off",g.workshop_light().is_none()&&!g.workshop.state.mounts[15].lamp_on)?;
    let off=g.workshop.state.part_at(15).unwrap().energy_joules;
    for _ in 0..120 {g.update(&TankInput::default(),DT,r);}
    check(checks,"off lamp preserves remaining battery exactly",g.workshop.state.part_at(15).unwrap().energy_joules.to_bits()==off.to_bits())?;
    photo(r,8)?;
    let mut expected=before_ids;expected.push(item.0);expected.sort_unstable();
    let mut ledger=before_ledger;ledger.push(item.0);ledger.sort_unstable();
    check(checks,"lamp fitting conserves all other component identities",ids(g)==expected&&ledger_ids(g)==ledger)?;
    check(checks,"fitted lamp and battery survive typed owner save",owner_roundtrip(g)?)?;
    evidence["lamp_battery"]=json!({"before_lit_test_j":before,"after_two_seconds_j":after,"retained_off_j":off});Ok(())
}
