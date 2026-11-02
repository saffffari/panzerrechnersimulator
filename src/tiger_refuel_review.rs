//! Source-interaction witness, nested under tiger_refuel. Staging is disclosed; cap poses,
//! finite transfer and restoration must come from the ordinary runtime, never fixture success.
use super::*;
use crate::expedition_world_save::{self as save, WorldSave};
use anyhow::Context;
use serde_json::{json,Value};
use std::{io::Write,path::Path};
const DT:f32=1.0/60.0;

struct Proof<'a>{prefix:String,d:&'a wgpu::Device,q:&'a wgpu::Queue,report:Value,ticks:u64}
impl Proof<'_>{
    fn flush(&self)->Result<()> {let mut f=std::fs::File::create(format!("{}.json",self.prefix))?;
        serde_json::to_writer_pretty(&mut f,&self.report)?;f.write_all(b"\n")?;f.sync_all()?;Ok(())}
    fn check(&mut self,name:&str,pass:bool,detail:Value)->Result<()> {
        self.report["checks"].as_array_mut().unwrap().push(json!({"check":name,"pass":pass,"detail":detail}));
        self.flush()?;ensure!(pass,"fuel review: {name}");Ok(())
    }
    fn tick(&mut self,g:&mut Game,r:&mut Renderer,held:bool)->Result<()> {
        g.update(&TankInput{fire2:held,..Default::default()},DT,r);self.ticks+=1;
        // Consume normal rendering queues throughout the experiment, including long cap holds.
        if self.ticks%5==0{crate::simulation_uploads::drain(self.d,self.q)?;r.ui_reset();
            let _=r.render_to_rgba_with_hud(self.d,self.q,r.iw,r.ih)?;}Ok(())
    }
    fn still(&mut self,g:&Game,r:&mut Renderer,label:&str)->Result<()> {
        let path=format!("{}_{label}.png",self.prefix);r.ui_reset();r.reset_history();
        crate::converge_headless_screenshot(r,self.d,self.q,16)?;
        r.render_to_png(self.d,self.q,&path,1280,720)?;
        self.report["images"].as_array_mut().unwrap().push(json!({"path":path,"state":g.tiger_refuel_receipt()}));self.flush()
    }
    fn click(&mut self,g:&mut Game,r:&Renderer,label:&str)->Result<()> {
        let eye=r.camera.pos;let dir=r.camera.forward();
        let models=g.fire_surface_models();let hit=g.cabin_fire.sweep(eye,eye+dir*REACH,&models);
        let hovered=g.tiger_refuel_hover(eye,dir);let accepted=g.tiger_refuel_interact(eye,dir);
        self.check(label,accepted,json!({"hover":hovered,"first_source_hit":hit.map(|h|json!({"frame":h.frame,"world_m":models[h.frame].transform_point3(h.point).to_array()})),"eye":eye.to_array(),"direction":dir.to_array()}))
    }
    fn cap(&mut self,g:&mut Game,r:&mut Renderer,index:usize,opening:bool)->Result<()> {
        let target=g.player.hull_model().transform_point3(g.tiger_refuel.center(index));look(g,r,target);
        self.click(g,r,&format!("ordinary LMB {} cap{index}",if opening{"opens"}else{"returns"}))?;
        let endpoint=if opening{1.0}else{0.0};
        for _ in 0..200 {
            if g.tiger_refuel.state.fraction[index]==endpoint{break;}
            let target=g.player.hull_model().transform_point3(g.tiger_refuel.center(index));look(g,r,target);
            self.tick(g,r,true)?;
        }
        self.check(&format!("cap{index} reaches retained {} pose",if opening{"parked"}else{"closed"}),
            g.tiger_refuel.state.fraction[index]==endpoint,g.tiger_refuel_receipt())?;
        self.tick(g,r,false)
    }
}
fn look(g:&mut Game,r:&mut Renderer,point:Vec3){let direction=(point-g.foot.eye()).normalize_or_zero();
    g.foot.yaw=direction.x.atan2(-direction.z);g.foot.pitch=direction.y.asin();
    r.camera.pos=g.foot.eye();r.camera.set_forward(direction);g.equipment_eye=r.camera.pos;g.equipment_look=direction;
}
// Aim through the actual mouth. A ray aimed at the deep bottom from this standing offset
// meets the deck lip before entering the well; the ordinary first source hit must still win.
fn neck(g:&Game)->Vec3 {g.player.hull_model().transform_point3(g.tiger_refuel.receiver)}
fn pair(g:&Game,index:usize)->(f32,f32){(g.tank_fuel_liters().unwrap(),g.pickups[index].portable.unwrap().fuel_liters())}
fn owner_diff(a:&WorldSave,b:&WorldSave)->Vec<Value>{a.owners.iter().map(|a|{let b=b.owner(&a.key).unwrap();
    json!({"owner":a.key,"pass":a.payload==b.payload,"before_bytes":a.payload.len(),"after_bytes":b.payload.len()})}).collect()}

impl Game {
    pub fn run_tiger_refuel_review(requested:&str)->Result<()> {
        let prefix=crate::capture_target(requested);
        for suffix in [".json",".world.bin","_01_closed.png","_02_open.png","_03_pouring.png","_04_restored.png","_05_closed.png"]{
            ensure!(!Path::new(&format!("{prefix}{suffix}")).exists(),"fuel review output exists");}
        if let Some(p)=Path::new(&prefix).parent(){std::fs::create_dir_all(p)?;}
        std::env::set_var("KESSEL_AUDIO_CAPTURE","1");
        crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
        let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);let(d,q)=crate::request_device(&adapter);
        let mut proof=Proof{prefix,d:&d,q:&q,ticks:0,report:json!({"schema":"kessel.tiger-fuel-review.v1","pass":false,"status":"building","checks":[],"images":[],
            "fixtures":["initial enemy tanks/foot soldiers disabled; ordinary campaign director remains active","existing keeper staged on actual rear deck; entry climb is not tested","one existing finite can placed on deck, never replenished","Tiger starts with535L to make room","scripted gaze follows each physical cap during hold","controlled death is tested in a disposable saved branch, then exact full-world state restored"],
            "scope":"Ordinary LMB source picking, finite caps/can/fuel, interruption, complete-world cold restore and exact continuation. No liquid CFD, historical valve procedure or human reach/animation validation."})};proof.flush()?;
        let result=(||->Result<()> {
            let spec=crate::expedition_theatre::Spec::load()?;
            let(mut r,mut g)=Game::build_expedition_act(&d,&q,false,&crate::world::act(1))?;
            g.mute_audio();g.enable_expedition(spec.seed,g.expedition_initial_camp_position()?)?;g.enable_expedition_theatre(&mut r)?;
            g.enemy_active.fill(false);for e in &mut g.enemies{e.alive=false;}for s in &mut g.soldiers{s.alive=false;}
            g.engine_on=false;g.engine_crank_t=0.0;g.player.speed=0.0;g.standalone_fuel_liters=Some(535.0);
            g.mode=Mode::Foot;g.climb.active=false;g.climb.anim=false;g.foot.vel_y=0.0;g.foot.airborne=false;g.foot.crouch_t=0.0;
            let hull=g.player.hull_model();g.foot.pos=hull.transform_point3(Vec3::new(1.48,2.13,-2.05));
            let index=g.pickups.iter().position(|p|p.portable.is_some_and(PortableItem::has_fuel)).context("existing finite fuel can missing")?;
            let original_liters=g.pickups[index].portable.unwrap().fuel_liters();
            g.pickups[index].pos=hull.transform_point3(Vec3::new(1.48,2.0381,-2.95));g.pickups[index].collected=false;
            g.pickups[index].portable.as_mut().unwrap().put_down();
            look(&mut g,&mut r,hull.transform_point3(Vec3::new(1.1462,2.0341,-1.3491)));
            proof.tick(&mut g,&mut r,false)?;proof.still(&g,&mut r,"01_closed")?;
            proof.cap(&mut g,&mut r,0,true)?;proof.cap(&mut g,&mut r,1,true)?;
            proof.still(&g,&mut r,"02_open")?;
            let at=g.pickups[index].pos+Vec3::Y*0.30;look(&mut g,&mut r,at);
            let picked=g.interact_portable_item(r.camera.forward(),r.camera.pos);
            proof.check("ordinary LMB takes existing finite can",picked&&g.held_item==Some(index)&&g.pickups[index].portable.unwrap().fuel_liters()==original_liters,json!({"pickup":index,"liters":original_liters}))?;
            let target=neck(&g);look(&mut g,&mut r,target);proof.click(&mut g,&r,"ordinary LMB starts finite pour into source neck")?;
            let before=pair(&g,index);for _ in 0..60{proof.tick(&mut g,&mut r,true)?;}
            let after=pair(&g,index);proof.check("partial pour moves only existing can fuel",after.0>before.0&&after.1<before.1&&((after.0-before.0)-(before.1-after.1)).abs()<0.0002,json!({"before":before,"after":after}))?;
            proof.still(&g,&mut r,"03_pouring")?;
            let world=g.expedition_world_recipe()?;let snapshot=g.export_expedition_world(&r,world.clone())?;
            let path=format!("{}.world.bin",proof.prefix);save::save(Path::new(&path),&snapshot)?;let loaded=save::load(Path::new(&path))?;loaded.verify_for_resume()?;
            // Continuation deliberately uses identical ordinary ticks with no intervening render
            // on either branch, preserving the exact renderer queues too.
            for _ in 0..12{g.update(&TankInput{fire2:true,..Default::default()},DT,&mut r);}
            let expected=g.export_expedition_world(&r,world.clone())?;
            drop(g);drop(r);d.poll(wgpu::PollType::wait_indefinitely())?;
            let(mut r,mut g)=Game::build_expedition_resume_candidate(&d,&q,&loaded,false)?;g.mute_audio();
            let actual=g.export_expedition_world(&r,world.clone())?;let diff=owner_diff(&snapshot,&actual);
            proof.check("cold whole-world restore retains every owner mid-pour",diff.iter().all(|v|v["pass"]==true),json!(diff))?;
            proof.still(&g,&mut r,"04_restored")?;g.import_expedition_world(&mut r,&loaded)?;
            for _ in 0..12{g.update(&TankInput{fire2:true,..Default::default()},DT,&mut r);}
            let continued=g.export_expedition_world(&r,world)?;let diff=owner_diff(&expected,&continued);
            proof.check("twelve ordinary continued ticks match every retained owner",diff.iter().all(|v|v["pass"]==true),json!(diff))?;
            proof.tick(&mut g,&mut r,false)?;let stopped=pair(&g,index);
            for _ in 0..15{proof.tick(&mut g,&mut r,false)?;}
            proof.check("released LMB stops transfer without changing finite stock",pair(&g,index)==stopped,json!({"liters":stopped}))?;
            let target=neck(&g);look(&mut g,&mut r,target);proof.click(&mut g,&r,"restart interrupted pour")?;
            let away=g.foot.eye()+Vec3::Y;look(&mut g,&mut r,away);proof.tick(&mut g,&mut r,true)?;
            proof.check("aiming away interrupts before fuel moves",pair(&g,index)==stopped&&g.tiger_refuel.state.work.is_none(),g.tiger_refuel_receipt())?;
            // This is a terminal-state test on a disposable saved branch, not a free resurrection.
            g.import_expedition_world(&mut r,&loaded)?;let death_before=pair(&g,index);g.debug_controlled_soldier_loss();proof.tick(&mut g,&mut r,true)?;
            proof.check("actual keeper death prevents continued pour",!g.controlled_soldier_alive()&&pair(&g,index)==death_before&&g.tiger_refuel.state.work.is_none(),g.tiger_refuel_receipt())?;
            // Death may allocate retained corpse source pools. Discard the whole branch and
            // rebuild a validated candidate; never import a smaller source layout in place.
            drop(g);drop(r);d.poll(wgpu::PollType::wait_indefinitely())?;
            let(mut r,mut g)=Game::build_expedition_resume_candidate(&d,&q,&loaded,false)?;g.mute_audio();
            for _ in 0..480{proof.tick(&mut g,&mut r,true)?;if g.tiger_refuel.state.work.is_none(){break;}}
            let full=pair(&g,index);proof.check("full destination stops without consuming remaining can",full.0==TIGER_FUEL_CAPACITY_L&&(original_liters-full.1-5.0).abs()<0.0002,g.tiger_refuel_receipt())?;
            let placed=g.put_down_held_portable();proof.check("X puts down remaining finite can",placed&&g.held_item.is_none()&&pair(&g,index)==full,json!({"liters":full}))?;
            proof.cap(&mut g,&mut r,1,false)?;proof.cap(&mut g,&mut r,0,false)?;proof.still(&g,&mut r,"05_closed")?;
            proof.report["final"]=g.tiger_refuel_receipt();proof.report["pass"]=json!(true);proof.report["status"]=json!("passed");proof.flush()?;Ok(())
        })();
        if let Err(e)=&result{proof.report["pass"]=json!(false);proof.report["status"]=json!("failed");proof.report["error"]=json!(format!("{e:#}"));proof.flush()?;}result
    }
}
