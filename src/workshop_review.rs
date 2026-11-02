//! Live source service and damaged replacement review. Retainer work and pickups use ordinary
//! LMB, putdown uses the same X action. A source-contact strike, one finite ground spare and
//! the current person's inspection poses are openly staged; travel/combat are not proved here.
use super::*;
use serde_json::{json,Value};
use std::{fs::OpenOptions,io::Write,path::Path};
#[path="workshop_lamp_review.rs"]
mod lamp;

const DT:f32=1.0/60.0;
fn ids(g:&Game)->Vec<u64> {
    let mut ids=g.workshop.state.parts.iter().map(|p|p.item.0).collect::<Vec<_>>();
    ids.sort_unstable();ids
}
fn ledger_ids(g:&Game)->Vec<u64> {
    let mut ids=g.expedition_state().map(|s|s.items.iter().map(|p|p.id.0).collect::<Vec<_>>()).unwrap_or_default();
    ids.sort_unstable();ids
}
fn face(g:&mut Game,r:&mut Renderer,target:Vec3) {
    g.dbg_cupola_view(0.0,0.0);g.update(&TankInput::default(),DT,r);
    let base=r.camera.yaw;let mut wanted=r.camera;wanted.set_forward(target-wanted.pos);
    let yaw=(wanted.yaw-base+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
    g.dbg_cupola_view(yaw,(wanted.pitch+0.10).clamp(-1.2,1.0));g.update(&TankInput::default(),DT,r);
}
fn check(checks:&mut Vec<Value>,name:&str,passed:bool)->Result<()> {
    checks.push(json!({"name":name,"passed":passed}));ensure!(passed,"workshop check failed: {name}");Ok(())
}
fn reached(g:&Game,mount:usize,item:ItemId,operation:Operation)->bool {
    let s=&g.workshop.state;let socket=s.mounts[mount];
    match operation {
        Operation::Release=>socket.fastening==0.0&&s.part_at(mount).is_some_and(|p|p.item==item),
        Operation::Withdraw=>s.held().is_some_and(|p|p.item==item)&&s.part_at(mount).is_none(),
        Operation::Insert=>s.part_at(mount).is_some_and(|p|p.item==item)&&!socket.tested&&socket.fastening==0.0,
        Operation::Secure=>s.part_at(mount).is_some_and(|p|p.item==item)&&!socket.tested&&socket.fastening==1.0,
        Operation::Test=>s.usable(mount)&&s.part_at(mount).is_some_and(|p|p.item==item),
    }
}
fn operate(g:&mut Game,r:&mut Renderer,mount:usize,item:ItemId,operation:Operation,
    stages:&mut Vec<Value>,checks:&mut Vec<Value>)->Result<()> {
    g.update(&TankInput::default(),DT,r);
    let down=TankInput{trigger:true,fire2:true,..Default::default()};
    let mut ticks=0;
    while ticks<240&&!reached(g,mount,item,operation) {g.update(&down,DT,r);ticks+=1;}
    stages.push(json!({"operation":operation,"ticks":ticks,"seconds":ticks as f32*DT,
        "eye_m":r.camera.pos.to_array(),"look":r.camera.forward().to_array(),
        "hover":g.hover_label,"state":g.workshop.state}));
    check(checks,&format!("{operation:?} reached through ordinary input"),reached(g,mount,item,operation))?;
    let after=g.workshop.state.clone();
    for _ in 0..12 {g.update(&down,DT,r);}
    let mut observed=g.workshop.state.clone();
    // A tested light keeps using real battery energy while the button remains held. That
    // autonomous use is not permission to advance the next fastening/handling operation.
    for p in &mut observed.parts {if let Some(old)=after.parts.iter().find(|old|old.item==p.item) {p.energy_joules=old.energy_joules;}}
    check(checks,&format!("{operation:?} waits for button release"),observed==after)?;
    g.update(&TankInput::default(),DT,r);Ok(())
}
fn owner_roundtrip(g:&Game)->Result<bool> {
    let owner=g.export_workshop_world_state()?;
    let mut fields:crate::expedition_world_save::StateFields=owner.decode("workshop.runtime",4)?;
    let state:WorkshopSnapshot=fields.take("state")?;state.validate()?;
    let down:bool=fields.take("primary_down")?;let guard:bool=fields.take("primary_guard")?;
    let tool:Option<usize>=fields.take("tool_mount")?;let time:f32=fields.take("tool_remaining")?;
    let models:Vec<[f32;16]>=fields.take("models")?;
    let cooling:super::cooling::thermal::CoolingState=fields.take("cooling")?;cooling.validate()?;
    let cooling_step:super::cooling::thermal::CoolingStep=fields.take("cooling_step")?;
    let rotor_angles:[f32;8]=fields.take("rotor_angles")?;
    let engine_access:super::engine::EngineAccessState=fields.take("engine_access")?;engine_access.validate()?;
    let rack_inspection:super::rack::RackInspectionState=fields.take("rack_inspection")?;rack_inspection.validate()?;fields.finish()?;
    Ok(state==g.workshop.state&&down==g.workshop.primary_down&&guard==g.workshop.primary_guard
        &&tool==g.workshop.tool_mount&&time.to_bits()==g.workshop.tool_remaining.to_bits()
        &&models==g.workshop.models.iter().map(Mat4::to_cols_array).collect::<Vec<_>>()
        &&cooling==g.workshop.cooling&&rotor_angles==g.workshop.rotor_angles&&engine_access==g.workshop.engine.state&&rack_inspection==g.workshop.rack_inspection
        &&bincode::serialize(&cooling_step)?==bincode::serialize(&g.workshop.cooling_step)?)
}
/// A millimetre-scale segment across an actual housing triangle; this is a collision
/// witness, not a fabricated aperture or a change to the contact cache.
fn source_witness(g:&Game,bank:usize)->Option<(Vec3,Vec3,usize,usize)> {
    let models=g.fire_surface_models();let model=models[FIRST_FRAME+bank];
    for (mesh_index,mesh) in g.workshop.banks[bank].meshes.iter().enumerate() {
        for (triangle,t) in mesh.indices.chunks_exact(3).enumerate() {
            let p=t.iter().map(|i|model.transform_point3(Vec3::from_array(mesh.vertices[*i as usize].pos))).collect::<Vec<_>>();
            let normal=(p[1]-p[0]).cross(p[2]-p[0]).normalize_or_zero();
            if normal.length_squared()<0.9 {continue;}
            let center=(p[0]+p[1]+p[2])/3.0;let a=center+normal*0.003;let b=center-normal*0.003;
            if g.cabin_fire.sweep(a,b,&models).is_some_and(|hit|hit.frame==FIRST_FRAME+bank)
                &&g.cabin_fire.sweep_except_frame(a,b,&models,Some(FIRST_FRAME+bank)).is_none() {
                return Some((a,b,mesh_index,triangle));
            }
        }
    }None
}
fn contact_frame(g:&Game,a:Vec3,b:Vec3)->Option<usize> {
    g.cabin_fire.sweep(a,b,&g.fire_surface_models()).map(|hit|hit.frame)
}
fn stage_foot(g:&mut Game,r:&mut Renderer,point:Vec3,target:Vec3) {
    g.mode=Mode::Foot;g.dbg_place_foot(point.x,point.z);
    g.foot.vel_y=0.0;g.foot.airborne=false;g.foot.crouch_t=0.0;
    g.dbg_look_foot_at(target);g.update(&TankInput::default(),DT,r);
    g.dbg_look_foot_at(target);g.update(&TankInput::default(),DT,r);
}
/// Every failed candidate leaves the same object in the hands. No source/support override.
fn ground_damaged_part(g:&mut Game,r:&mut Renderer,item:ItemId,attempts:&mut Vec<Value>)->Result<()> {
    let hull=g.player.hull_model();
    for (x,z) in [(5.0,0.0),(-5.0,0.0),(5.0,-3.0),(-5.0,-3.0),(0.0,-7.0),(0.0,7.0)] {
        let feet=hull.transform_point3(Vec3::new(x,0.0,z));
        let mut target=feet+hull.transform_vector3(Vec3::Z)*0.45;
        target.y=g.hf.height_at(target.x,target.z)+0.02;
        stage_foot(g,r,feet,target);
        let placed=g.put_down_workshop_part();
        attempts.push(json!({"staged_feet_m":g.foot.pos.to_array(),"eye_m":r.camera.pos.to_array(),
            "look":r.camera.forward().to_array(),"x_action_accepted":placed}));
        if placed {g.update(&TankInput::default(),DT,r);return Ok(());}
        ensure!(g.workshop.state.held().is_some_and(|p|p.item==item),"rejected placement lost held identity");
    }
    anyhow::bail!("no source-supported putdown reachable at disclosed foot candidates")
}
fn pick_point(g:&Game,bank:usize)->Option<Vec3> {
    let eye=g.foot.eye();let model=g.workshop.models[bank];
    let item=g.workshop.state.parts.iter().find(|p|p.bank==bank)?.item;
    for mesh in &g.workshop.banks[bank].meshes {for t in mesh.indices.chunks_exact(3) {
        let point=t.iter().map(|&i|model.transform_point3(Vec3::from(mesh.vertices[i as usize].pos)))
            .sum::<Vec3>()/3.0;
        let look=(point-eye).normalize_or_zero();
        if g.workshop_cargo_offer(eye,look).is_some_and(|offer|offer.item==item) {return Some(point);}
    }}None
}

pub fn run(requested:&str)->Result<()> {
    let prefix=crate::capture_target(requested);let receipt=format!("{prefix}.json");
    let paths=["installed","removed","seated","verified","damaged","damaged_ground","replacement","lamp_on","lamp_off"]
        .map(|phase|format!("{prefix}_{phase}.png"));
    ensure!(!Path::new(&receipt).exists()&&paths.iter().all(|p|!Path::new(p).exists()),
        "workshop review prefix exists; use a new prefix");
    if let Some(parent)=Path::new(&prefix).parent() {std::fs::create_dir_all(parent)?;}
    let mut checks=Vec::new();let mut stages=Vec::new();let mut captures=Vec::new();
    let mut evidence=json!({});
    let result=(||->Result<()> {
        std::env::set_var("KESSEL_AUDIO_CAPTURE","1");
        crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
        let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);
        let(device,queue)=crate::request_device(&adapter);
        let(mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
        g.mute_audio();g.enable_expedition(1944,g.expedition_initial_camp_position()?)?;
        check(&mut checks,"current theatre manifest and finite rewards installed",g.enable_expedition_theatre(&mut r)?)?;
        evidence["theatre"]=json!(g.expedition_theatre_receipt());
        let person=g.expedition_state().context("expedition absent")?.controlled_person.context("no controlled person")?;
        check(&mut checks,"existing current person occupies commander station",
            g.singleplayer_person_id(CrewRole::Commander)==Some(person)&&g.controlled_soldier_alive())?;
        g.mode=Mode::Tank;g.chase_cam=false;g.dbg_cupola_view(0.0,0.0);
        for _ in 0..4 {g.update(&TankInput::default(),DT,&mut r);}
        let native_ids=ids(&g);let initial_ledger=ledger_ids(&g);
        let mut attempts=Vec::new();let mut selected=None;
        for mount in 0..7 {
            let point=g.workshop.mount_point(mount,g.player.hull_model(),g.player.turret_model(),g.sight_model());
            face(&mut g,&mut r,point);
            let target=g.workshop_mount_target(r.camera.pos,r.camera.forward(),None);
            attempts.push(json!({"mount":mount,"target":target,"eye_m":r.camera.pos.to_array(),"point_m":point.to_array()}));
            if target==Some(mount)&&g.workshop.state.usable(mount) {selected=Some(mount);break;}
        }
        evidence["reach_attempts"]=json!(attempts);
        let mount=selected.context("no healthy native vision retainer reachable from actual commander eye")?;
        let part=g.workshop.state.part_at(mount).context("selected native part absent")?.clone();
        let(a,b,mesh,triangle)=source_witness(&g,part.bank).context("no actual source collision witness")?;
        evidence["selected"]=json!({"mount":mount,"item":part.item,"bank":part.bank,"person":person,
            "source_witness":{"mesh":mesh,"triangle":triangle,"from_m":a.to_array(),"to_m":b.to_array(),
                "installed_frame":contact_frame(&g,a,b)},"initial_ids":native_ids,"initial_ledger_ids":initial_ledger,
            "camp":g.installed_camp_receipt()});
        let photo=|r:&mut Renderer,index:usize,captures:&mut Vec<Value>|->Result<()> {
            crate::simulation_uploads::drain(&device,&queue)?;r.reset_history();
            let convergence=crate::converge_headless_screenshot(r,&device,&queue,24)?;
            r.render_to_png(&device,&queue,&paths[index],1280,720)?;
            captures.push(json!({"path":paths[index],"eye_m":r.camera.pos.to_array(),
                "forward":r.camera.forward().to_array(),"exposure_converged":convergence.stable}));
            ensure!(convergence.stable,"workshop review exposure did not converge");Ok(())
        };
        photo(&mut r,0,&mut captures)?;
        operate(&mut g,&mut r,mount,part.item,Operation::Release,&mut stages,&mut checks)?;
        check(&mut checks,"released retainer disables optical authority",!g.workshop.state.usable(mount))?;
        operate(&mut g,&mut r,mount,part.item,Operation::Withdraw,&mut stages,&mut checks)?;
        photo(&mut r,1,&mut captures)?;
        check(&mut checks,"same healthy component is in current person's hands",g.workshop.state.held().is_some_and(|p|
            p.item==part.item&&p.condition==part.condition&&matches!(p.location,PartLocation::Hands{person:owner} if owner==person)))?;
        let removed_contact=contact_frame(&g,a,b);evidence["removed_witness_frame"]=json!(removed_contact);
        check(&mut checks,"removed source leaves no hidden collision at isolated old triangle",removed_contact.is_none())?;
        check(&mut checks,"removal conserves all part and expedition item IDs",ids(&g)==native_ids&&ledger_ids(&g)==initial_ledger)?;
        check(&mut checks,"held state survives typed owner serialization",owner_roundtrip(&g)?)?;
        operate(&mut g,&mut r,mount,part.item,Operation::Insert,&mut stages,&mut checks)?;
        photo(&mut r,2,&mut captures)?;
        check(&mut checks,"unsecured inserted component has no optical authority",!g.workshop.state.usable(mount))?;
        operate(&mut g,&mut r,mount,part.item,Operation::Secure,&mut stages,&mut checks)?;
        check(&mut checks,"secured component still requires physical check",!g.workshop.state.usable(mount))?;
        operate(&mut g,&mut r,mount,part.item,Operation::Test,&mut stages,&mut checks)?;
        photo(&mut r,3,&mut captures)?;
        check(&mut checks,"tested same component restores optical authority",g.workshop.state.usable(mount)
            &&g.workshop.state.part_at(mount).is_some_and(|p|p.item==part.item&&p.condition==part.condition))?;
        let final_contact=contact_frame(&g,a,b);evidence["verified_witness_frame"]=json!(final_contact);
        check(&mut checks,"installed source collision returns to original triangle",final_contact==Some(FIRST_FRAME+part.bank))?;
        check(&mut checks,"service creates no component or inventory item",ids(&g)==native_ids&&ledger_ids(&g)==initial_ledger)?;
        check(&mut checks,"current person remains alive and unchanged",g.controlled_soldier_alive()
            &&g.expedition_state().is_some_and(|s|s.controlled_person==Some(person)))?;
        check(&mut checks,"verified state survives typed owner serialization",owner_roundtrip(&g)?)?;
        evidence["healthy_service_final"]=g.workshop_receipt();

        // Staged damage uses an actual source-triangle sweep and the live equipment strike
        // dispatcher. No flying shell, material fracture simulation or enemy attack is claimed.
        let frame=contact_frame(&g,a,b).context("source strike no longer contacts installed part")?;
        check(&mut checks,"staged strike contacts the existing source housing",frame==FIRST_FRAME+part.bank)?;
        let strike_energy_j=5_000.0;let zone=g.workshop_strike_zone(frame,strike_energy_j);
        evidence["staged_strike"]=json!({"frame":frame,"energy_j":strike_energy_j,
            "from_m":a.to_array(),"to_m":b.to_array(),"zone":format!("{zone:?}")});
        check(&mut checks,"live source strike dispatcher owns damaged component",zone.is_some())?;
        g.update(&TankInput::default(),DT,&mut r);
        check(&mut checks,"struck part loses condition and optical authority",
            g.workshop.state.part_at(mount).is_some_and(|p|p.item==part.item&&p.condition==0.0)
                &&!g.workshop.state.usable(mount))?;
        photo(&mut r,4,&mut captures)?;
        operate(&mut g,&mut r,mount,part.item,Operation::Release,&mut stages,&mut checks)?;
        operate(&mut g,&mut r,mount,part.item,Operation::Withdraw,&mut stages,&mut checks)?;
        let mut placements=Vec::new();let placed=ground_damaged_part(&mut g,&mut r,part.item,&mut placements);
        evidence["staged_putdown_attempts"]=json!(placements);placed?;
        check(&mut checks,"X putdown retains damaged original on supported source or ground",
            g.workshop.state.parts.iter().any(|p|p.item==part.item&&p.condition==0.0
                &&matches!(p.location,PartLocation::Ground{..}|PartLocation::VehicleRest{..}))&&g.workshop.state.held().is_none())?;
        check(&mut checks,"removal and putdown conserve original item set",ids(&g)==native_ids&&ledger_ids(&g)==initial_ledger)?;
        let original=g.workshop.state.parts.iter().find(|p|p.item==part.item).unwrap();
        let ground_center=g.workshop.models[original.bank].transform_point3(g.workshop.banks[original.bank].center);
        let offset=g.player.hull_model().transform_vector3(Vec3::X)*1.05;
        stage_foot(&mut g,&mut r,ground_center+offset,ground_center);
        photo(&mut r,5,&mut captures)?;

        // One explicitly commissioned spare from the prebuilt source pool, not a stock refill.
        // This isolates handling from kilometres of staged travel to a mission-site reward.
        let spare=ItemId(799_900);
        check(&mut checks,"review spare identity did not previously exist",!ledger_ids(&g).contains(&spare.0))?;
        let forward=Vec3::new(r.camera.forward().x,0.0,r.camera.forward().z).normalize_or_zero();
        let mut ground=g.foot.pos+r.camera.right()*0.85+forward*0.65;
        ground.y=g.hf.height_at(ground.x,ground.z);
        g.spawn_workshop_reward(spare,PartKind::VisionBlock.key(),ground)?;
        g.update(&TankInput::default(),DT,&mut r);
        let replacement=g.workshop.state.parts.iter().find(|p|p.item==spare).context("review spare absent")?.clone();
        evidence["finite_review_spare"]=json!({"item":spare,"bank":replacement.bank,"kind":replacement.kind,
            "ground_m":ground.to_array(),"source_pool_commissioned_once":true,"condition":replacement.condition});
        let mut expected_ids=native_ids.clone();expected_ids.push(spare.0);expected_ids.sort_unstable();
        let mut expected_ledger=initial_ledger.clone();expected_ledger.push(spare.0);expected_ledger.sort_unstable();
        check(&mut checks,"fixture adds exactly one finite source part",ids(&g)==expected_ids&&ledger_ids(&g)==expected_ledger)?;
        let center=g.workshop.models[replacement.bank].transform_point3(g.workshop.banks[replacement.bank].center);
        stage_foot(&mut g,&mut r,ground-forward*0.45,center);
        let point=pick_point(&g,replacement.bank).context("no reachable real triangle of review spare")?;
        g.dbg_look_foot_at(point);g.update(&TankInput::default(),DT,&mut r);
        check(&mut checks,"spare pickup is reached through its actual source triangles",
            g.workshop_cargo_offer(r.camera.pos,r.camera.forward()).is_some_and(|offer|offer.item==spare))?;
        g.update(&TankInput{trigger:true,fire2:true,..Default::default()},DT,&mut r);
        g.update(&TankInput::default(),DT,&mut r);
        check(&mut checks,"ordinary primary input lifts the finite spare into same person's hands",
            g.workshop.state.held().is_some_and(|p|p.item==spare&&p.condition==1.0
                &&matches!(p.location,PartLocation::Hands{person:owner} if owner==person)))?;
        check(&mut checks,"damaged ground part and held spare serialize together",owner_roundtrip(&g)?)?;
        g.mode=Mode::Tank;g.chase_cam=false;
        let point=g.workshop.mount_point(mount,g.player.hull_model(),g.player.turret_model(),g.sight_model());
        face(&mut g,&mut r,point);
        operate(&mut g,&mut r,mount,spare,Operation::Insert,&mut stages,&mut checks)?;
        check(&mut checks,"replacement does not restore optics before securing and checking",!g.workshop.state.usable(mount))?;
        operate(&mut g,&mut r,mount,spare,Operation::Secure,&mut stages,&mut checks)?;
        operate(&mut g,&mut r,mount,spare,Operation::Test,&mut stages,&mut checks)?;
        photo(&mut r,6,&mut captures)?;
        check(&mut checks,"tested finite replacement restores actual optical authority",g.workshop.state.usable(mount)
            &&g.workshop.state.part_at(mount).is_some_and(|p|p.item==spare&&p.condition==1.0))?;
        let replaced_contact=contact_frame(&g,a,b);evidence["replacement_witness_frame"]=json!(replaced_contact);
        check(&mut checks,"replacement source occupies the original optical housing witness",
            replaced_contact==Some(FIRST_FRAME+replacement.bank))?;
        check(&mut checks,"original remains damaged and separate after replacement",
            g.workshop.state.parts.iter().any(|p|p.item==part.item&&p.condition==0.0
                &&matches!(p.location,PartLocation::Ground{..}|PartLocation::VehicleRest{..})))?;
        check(&mut checks,"replacement consumes no additional item and duplicates none",ids(&g)==expected_ids&&ledger_ids(&g)==expected_ledger)?;
        check(&mut checks,"replacement owner survives typed serialization",owner_roundtrip(&g)?)?;
        check(&mut checks,"review preserves the living controlled person",g.controlled_soldier_alive()
            &&g.expedition_state().is_some_and(|s|s.controlled_person==Some(person)))?;
        lamp::run_lamp(&mut g,&mut r,&mut checks,&mut stages,&mut evidence,|r,index|photo(r,index,&mut captures))?;
        g.workshop.state.validate()?;evidence["final"]=g.workshop_receipt();Ok(())
    })();
    let passed=result.is_ok()&&checks.iter().all(|c|c["passed"]==true);
    let report=json!({"schema":"panzer.workshop-review.v3","passed":passed,
        "error":result.as_ref().err().map(|e|format!("{e:#}")),"checks":checks,"stages":stages,
        "captures":captures,"evidence":evidence,"simulation_hz":60,
        "staging":"Existing living commander's seated gaze and grounded outside poses are staged, including return to the seat with held parts. After healthy service, one source-triangle contact dispatches a5000J binary equipment-damage fixture. One finite vision block and one finite lamp are explicitly commissioned from prebuilt source pools on nearby terrain. Vehicle, crew health, environment lighting, fuel and existing item stock are unchanged; no repair progress or completion is injected. The installed lamp changes lighting through its actual battery emitter.",
        "limitations":["Source-contact damage fixture, not an enemy encounter or a simulated flying projectile.",
            "Optical authority is the live workshop usable gate consumed by the renderer, not a pixel-transmission assertion.",
            "Typed workshop-owner decode only; not whole-world restart or disk restore.",
            "Tools, track work, cargo and powertrain are outside this review.",
            "Outside pickup/return poses are staged; dismount and re-entry are not proved. Lamp approach alone uses ordinary interior station exit and walking.",
            "Nine stills, no animation or elapsed real-time video claim."]});
    let mut output=OpenOptions::new().write(true).create_new(true).open(&receipt)?;
    output.write_all(&serde_json::to_vec_pretty(&report)?)?;
    println!("[workshop/review] {receipt}: {}",if passed{"PASS"}else{"FAIL"});
    result?;ensure!(passed,"workshop review failed; see {receipt}");Ok(())
}
