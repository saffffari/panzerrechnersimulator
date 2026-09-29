//! Offscreen physical-paper readability review; the studio avoids constructing an unrelated map.
use super::*;

#[test]
#[ignore = "requires GPU and Tiger source assets; KESSEL_MAP_REVIEW_PREFIX stores physical-paper views"]
fn physical_map_overview_and_zoom_preserve_ink_and_depth() -> anyhow::Result<()> {
    use crate::game::{Game,Station,TankInput};
    use crate::tank_editor_scene::{TankAssetId,TankEditorScene};
    let prefix=std::env::var("KESSEL_MAP_REVIEW_PREFIX")?;
    for suffix in ["_overview.png","_you_zoom.png","_goal_zoom.png",".json"] {
        anyhow::ensure!(!std::path::Path::new(&format!("{prefix}{suffix}")).exists(),"use a fresh map review prefix");
    }
    let instance=crate::create_instance();
    let adapter=crate::request_adapter(&instance,None);
    let (device,queue)=crate::request_device(&adapter);
    let (mut renderer,mut game)=Game::build_tank_editor(&device,&queue,&TankEditorScene::new(TankAssetId::TigerI))?;
    game.mute_audio();
    game.jump_station(Station::Loader);
    for _ in 0..3 {game.update(&TankInput::default(),1.0/60.0,&mut renderer);}
    // Print the actual winter Act I survey onto the real held sheet. The map's pose/lighting and
    // depth are the runtime path; only these pencil strokes/pin and player-at-spawn are staged.
    let act=crate::world::act(1);
    let half=act.half_extent_m.max(1.0);
    let paper_uv=|p:Vec2|Vec2::new(0.50+p.x/half*0.39,0.51+p.y/half*0.38);
    let player=act.spawn;
    let mut pencil=Vec::new();
    for i in 0..24 {pencil.push((act.id,Vec2::new(0.55+i as f32*0.003,0.63+(i as f32*0.3).sin()*0.02)));}
    let pins=vec![(act.id,Vec2::new(0.63,0.59))];
    let mut views=Vec::new();
    assert!(game.set_map_held(true));
    for (name,focus,zoom) in [("overview",paper_uv(player),1.0),("you_zoom",paper_uv(player),MAX_ZOOM),
        ("goal_zoom",paper_uv(act.goal),MAX_ZOOM)] {
        game.hand_map_view.zoom=zoom;
        game.focus_held_map(focus);
        game.update(&TankInput::default(),0.0,&mut renderer);
        assert!(game.prepare_held_map(&mut renderer,true));
        assert_eq!(renderer.optic_mask,0.0);
        renderer.ui_reset();
        crate::ui::draw_held_map_print(&mut renderer,&act,&[],&pencil,&pins,None,0,focus,0.0,false,player);
        renderer.render_held_map_texture(&device,&queue,game.held_map_slot());
        renderer.ui_reset();
        renderer.reset_history();
        renderer.render_to_png_hud(&device,&queue,&format!("{prefix}_{name}.png"),1280,720)?;
        let m=model(&renderer.camera,game.hand_map_view);
        let target=m.transform_point3(super::paper_point(focus.x,focus.y));
        let clip=renderer.camera.view_proj()*target.extend(1.0);
        anyhow::ensure!(clip.w>renderer.camera.znear && (clip.x/clip.w).abs()<1.0
            && (clip.y/clip.w).abs()<1.0,"focused map marker must remain visible");
        views.push(serde_json::json!({"view":name,"zoom":zoom,"focus_paper_uv":focus.to_array(),
            "focus_screen_ndc":[clip.x/clip.w,clip.y/clip.w],"eye_world_m":renderer.camera.pos.to_array(),
            "fov_y_deg":renderer.camera.fov_y.to_degrees()}));
    }
    std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "schema":"panzer.held-map-reading-review.v1","adapter":adapter.get_info().name,
        "scope":"Actual lit, depth-tested held paper in the isolated source Tiger Loader view. Printed survey is world::act(1); player-at-spawn, one pin and pencil line are disclosed review fixtures. Native mouse/controller input remains a live playtest check.",
        "paper_size_m":[WIDTH,HEIGHT],"zoom_range":[1.0,MAX_ZOOM],"views":views,
        "controls":"Wheel or +/-; controller D-pad up/down. Move pencil to read another part. Existing LMB drawing and RMB pins retain paper UV coordinates.",
    }))?)?;
    Ok(())
}
