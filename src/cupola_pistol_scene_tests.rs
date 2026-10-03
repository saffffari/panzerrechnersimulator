//! Actual Tiger update regression. Initial head-out/hatch pose is a disclosed fixture, not an
//! interaction-UX claim. Trigger, magazine accounting, source muzzle and flight are shipping paths.
use super::*;

#[test]
#[ignore = "requires GPU and Tiger assets; KESSEL_CUPOLA_PISTOL_REVIEW_PREFIX saves the aimed view"]
fn exposed_p08_uses_shared_ammo_real_flight_and_no_vehicle_trigger() -> anyhow::Result<()> {
    use crate::tank_editor_scene::{TankAssetId,TankEditorScene};
    let instance=crate::create_instance();
    let adapter=crate::request_adapter(&instance,None);
    let (device,queue)=crate::request_device(&adapter);
    let (mut renderer,mut game)=Game::build_tank_editor(&device,&queue,&TankEditorScene::new(TankAssetId::TigerI))?;
    game.mute_audio();
    game.enable_singleplayer_crew();
    game.jump_station(Station::Commander);
    let dt=1.0/60.0;
    let total=game.foot.total_p08_rounds();
    let mag=game.foot.mag;
    let main_shots=game.main_gun_shot_sequence();
    let mg_shots=game.mg_round;
    let aimed=TankInput{aim:true,..Default::default()};
    let trigger=TankInput{aim:true,fire2:true,mg_coax:true,..Default::default()};
    game.update(&trigger,dt,&mut renderer);
    assert_eq!(game.foot.mag,mag,"buttoned commander must not draw/fire the sidearm");
    assert!(!game.cupola_pistol.raised);

    game.dbg_open_hatch();
    for _ in 0..65 {game.update(&TankInput::default(),dt,&mut renderer);}
    game.commander_outside=true;
    game.head_pitch=0.20;
    game.update(&trigger,dt,&mut renderer);
    assert!(game.cupola_pistol.raised);
    assert_eq!(game.foot.mag,mag,"drawing and pressing together must not discharge");
    game.update(&aimed,dt,&mut renderer);
    let id_before=game.next_projectile_id;
    game.update(&trigger,dt,&mut renderer);
    assert_eq!(game.foot.mag,mag-1);
    assert_eq!(game.foot.total_p08_rounds(),total-1);
    assert_eq!(game.next_projectile_id,id_before+1,"exactly one physical bullet should launch");
    let bullet=game.projectiles.iter().find(|p|p.id==id_before).expect("P08 bullet must exist at muzzle").clone();
    assert_eq!(bullet.small_arm.unwrap().shot.weapon,crate::infantry_combat::Weapon::P08);
    assert_eq!(bullet.age_s,0.0,"no instant terminal hit at trigger time");
    assert_eq!(bullet.pos,bullet.origin);
    assert!(bullet.origin.distance(renderer.camera.pos)>0.25,"must launch from actual gun muzzle");
    for _ in 0..12 {game.update(&trigger,dt,&mut renderer);}
    assert_eq!(game.foot.mag,mag-1,"holding LMB must not turn P08 into an automatic weapon");
    let flown=game.projectiles.iter().find(|p|p.id==id_before).expect("fixture bullet should still be in flight");
    let flown_distance=flown.distance_m;
    let flown_age=flown.age_s;
    assert!(flown_distance>20.0 && flown_age>0.1,"projectile did not advance through shared ballistics");
    assert_eq!(game.main_gun_shot_sequence(),main_shots,"sidearm leaked into the main gun");
    assert_eq!(game.mg_round,mg_shots,"sidearm leaked into the hull MG");
    let prefix=std::env::var("KESSEL_CUPOLA_PISTOL_REVIEW_PREFIX").ok();
    if let Some(prefix)=&prefix {
        for suffix in ["_aimed.png",".json"] {
            anyhow::ensure!(!std::path::Path::new(&format!("{prefix}{suffix}")).exists(),"use a fresh pistol review prefix");
        }
        renderer.reset_history();
        renderer.render_to_png(&device,&queue,&format!("{prefix}_aimed.png"),1280,720)?;
        let local_visibility=game.hide_first_person_for_optic(&mut renderer);
        renderer.reset_history();
        renderer.render_to_png(&device,&queue,&format!("{prefix}_without_local_body.png"),1280,720)?;
        for (i,(slot,visible)) in local_visibility.iter().copied().enumerate() {renderer.set_visible(slot,visible && i%2==1);}
        renderer.reset_history();
        renderer.render_to_png(&device,&queue,&format!("{prefix}_body_only.png"),1280,720)?;
        for (i,(slot,visible)) in local_visibility.iter().copied().enumerate() {renderer.set_visible(slot,visible && i%2==0);}
        renderer.reset_history();
        renderer.render_to_png(&device,&queue,&format!("{prefix}_arms_only.png"),1280,720)?;
        for (slot,visible) in local_visibility {renderer.set_visible(slot,visible);}
        eprintln!("[clearance] eye={:?} forward={:?} receipt={}",renderer.camera.pos,renderer.camera.forward(),game.first_person.as_ref().unwrap().receipt());
    }

    // Same magazine and chamber as on foot; a tactical reload cannot create reserve rounds.
    let remaining=game.foot.total_p08_rounds();
    game.update(&TankInput{reload:true,..aimed},dt,&mut renderer);
    assert!(game.foot.reload_t>0.0);
    for _ in 0..100 {game.update(&aimed,dt,&mut renderer);}
    assert_eq!(game.foot.total_p08_rounds(),remaining);
    assert_eq!(game.foot.mag,crate::foot::MAG_CAP+1);
    assert!(game.foot.p08_chambered);

    // The mounted-pistol exemption must not skip the tank's armor like a bore-mounted MG.
    let start=game.player.center()+Vec3::Y*7.0;
    let end=game.player.center()-Vec3::Y;
    let mut down=bullet.clone();
    down.origin=start;down.pos=start;down.velocity=Vec3::NEG_Y*350.0;
    let contact=game.nearest_projectile_contact(&down,ProjectileStep{start,end,
        forward_m:start.distance(end),distance_m:start.distance(end),age_s:0.025,
        velocity:down.velocity,crosswind_velocity:Vec3::ZERO});
    assert!(matches!(contact.map(|c|c.kind),Some(ProjectileContactKind::Vehicle(v))
        if v.vehicle==ProjectileVehicle::Player),"P08 aimed at own roof must strike armor");

    game.commander_outside=false;
    game.update(&trigger,dt,&mut renderer);
    assert!(!game.cupola_pistol.raised);
    assert_eq!(game.foot.total_p08_rounds(),remaining);
    if let Some(prefix)=prefix {
        std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "schema":"panzer.cupola-p08-review.v1","adapter":adapter.get_info().name,
            "scope":"Actual isolated Tiger Game updates. Initial open hatch/head-out state staged; draw and fresh trigger, shared magazine/reload, true muzzle flight, buttoned gating and own-roof collision are asserted. Native mouse interaction remains a live playtest check.",
            "p08_rounds_spent":total-remaining,"main_gun_discharges":game.main_gun_shot_sequence()-main_shots,
            "mg_rounds":game.mg_round-mg_shots,"flight_distance_m":flown_distance,"flight_age_s":flown_age,
            "muzzle_world_m":bullet.origin.to_array(),"shared_ammo_after_reload":remaining,"own_roof_blocks_bullet":true,
        }))?)?;
    }
    Ok(())
}
