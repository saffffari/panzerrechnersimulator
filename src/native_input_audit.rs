//! Opt-in observations of the real window input loop. Never injects or changes a game state.
use crate::{game::{Game,TankInput},renderer::Renderer};
use std::sync::{Mutex,OnceLock};
use std::time::{Duration,Instant};
fn enabled()->bool {
    static ON:OnceLock<bool>=OnceLock::new();
    *ON.get_or_init(||std::env::var("KESSEL_NATIVE_INPUT_AUDIT").as_deref()==Ok("1"))
}
pub fn key(code:&str,down:bool) {
    if enabled(){println!("[native-input-key] {}",serde_json::json!({"key":code,"down":down}));}
}
pub fn sample(g:&Game,r:&Renderer,input:&TankInput,map:bool,pads:&[crate::gamepad::PadFrame]) {
    if !enabled(){return;}
    static LAST:Mutex<Option<Instant>>=Mutex::new(None);
    let mut last=LAST.lock().unwrap();let now=Instant::now();
    if last.is_some_and(|t|now.duration_since(t)<Duration::from_millis(250)){return;}
    *last=Some(now);
    println!("[native-input-state] {}",serde_json::json!({
        "time":r.time,"station":format!("{:?}",g.station),"on_foot":g.dbg_is_foot(),
        "foot_m":g.foot_pos().to_array(),"tank_m":g.player.pos.to_array(),
        "camera_m":r.camera.pos.to_array(),"climbing":g.is_climbing(),
        "hatch_open":g.hatch_is_open(),"commander_outside":g.commander_outside,
        "map_open":map,"map_zoom":g.held_map_zoom(),
        "shots":g.main_gun_shot_sequence(),"carrying_round":g.debug_carrying(),
        "rack_rounds":g.debug_rack_left(),"tank_alive":g.player.alive,
        "input":{"drive":input.drive,"steer":input.steer,"run":input.run,"crouch":input.crouch,
            "jump":input.jump,"primary":input.mg_coax,"fire":input.fire,"aim":input.aim,
            "fire2":input.fire2,"coax":input.coax,"reload":input.reload,
            "look":[input.look_dx,input.look_dy],"coop":input.coop.is_some()},
        "pads":pads.iter().map(|p|serde_json::json!({"peer":p.peer_id,"connected":p.connected,
            "drive":p.drive,"steer":p.steer,"look":[p.look_x,p.look_y],"primary":p.primary,
            "aim":p.aim,"jump":p.jump,"crouch":p.crouch,"reload":p.reload,
            "map_pressed":p.map_pressed})).collect::<Vec<_>>(),
        "mission":g.winter_mission_snapshot()
    }));
}
