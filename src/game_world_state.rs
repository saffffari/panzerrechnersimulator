//! Complete-world owner adapters. These operate only on a validated fresh candidate Game.
//! GPU/source caches remain from the exact fingerprinted build; typed mutable state is restored.
use super::*;
use anyhow::{ensure,Context,Result};
use crate::expedition_world_save::{OwnerBlock,StateFields};

macro_rules! fields_owner {
    ($save:ident,$load:ident,$key:literal; $($field:ident),* $(,)?) => {
        impl Game {
            pub(super) fn $save(&self)->Result<StateFields> {Ok(crate::capture_world_fields!(self; $($field),*))}
            pub(super) fn $load(&mut self,fields:StateFields)->Result<()> {
                crate::restore_world_fields!(self,fields; $($field),*);Ok(())
            }
        }
    };
}

fields_owner!(capture_physical,restore_physical,"game.physical";
    hand_torch_on, cabin_lights, personal_primary_guard, cigarette_pick, cigarette_left_was_down, cigarette_butt_pending, next_cigarette_butt,
    ally_shot_tick, finale_town_integrity, civilian_column_alive, ally_attrition, ally_damage_tick, enemy_active, enemy_skill,
    enemy_track_age, darkness, authored_weather, enemy_concealed, enemy_gun_shots, standalone_fuel_liters, campaign_channel,
    track_snow_previous_model, next_mg_tracer, next_mg_impact, mg_timer, mg_round, mg_flash_t, mg_flash_pos,
    mg_heat, mg_overheated, mg_hot, gun_shots, engine_death_cued, next_blast, next_spark,
    next_snow, kill_tick, gunner_optic, commander_outside, hatch_open, hatch_t, driver_visor_lean,
    driver_look, radio_look, last_access_hatch, hover_roof_hatch, hover_hatch, hover_lever, rounds_spent,
    walk_pos, loader_crouch_t, interior_haze, weather_baseline, weather_wind_scale, bore_smoke_pending, breech_gas_t,
    chase_cam, force_buttoned, force_driver, head_yaw, head_pitch, binocular_raise, interior_light,
    next_projectile_id, projectile_accumulator_s, next_e_fx, ai_tick, armor_tick, armor_clock_s, concussion_t,
    concussion_dur, engine_ear_laf_dba, hit_camera_kick, pressure_reset_pending, pressure_arrivals_delivered, pressure_last_arrival_s, act_complete,
    sights_mag, sight_steady, god, grab, lay_yaw, lay_pitch, traverse_jam_offset,
    traverse_jam_phase, traverse_jam_active, drop_physics_frame, slot_taken, carry_pos, carry_vel, carry_dir,
    carry_rotation, carry_angular_vel, carried_casing, casing_pull, drive_cmd, steer_cmd, turret_audio_prev_yaw,
    engine_on, engine_crank_t, start_noise_t, prev_ignition, marks, mark_dwell, ext_equipped,
    compass_equipped, compass_raise, map_held, primary_pickup_press, ram_ready, prev_enter, canyon_opening_fielded,
    canyon_weather_seed, weather_storm_now, auto_ram, lever_open, lever_pulling, slot_nature_he, rtt_refresh_cursor,
    rtt_updates_per_frame, rtt_priority_enabled, rtt_refresh_tick, rtt_last_refresh, defer_rtt_to_render, defer_presentation_rebuild, cupola_closed_hint_t,
    foam_emit_t, ext_want_spray, next_hot_fragment, hot_fragment_accumulator_s, hot_fragment_collision_count, hot_fragment_audio_cooldown, hot_fragment_audio_dings_remaining,
    hot_fragment_whizz_cooldown, hot_fragment_audio_whizzes_remaining, carrying_track_kit, equipment_eye, equipment_look, track_pull, track_hits,
    hammer_t, repair_hammer_t, bail_presses, ambient_t, ambient_tick, soundscape_combat_t, night,
    scatter_loot_sockets, scatter_cover_first, crush_rumble, crush_phase, crush_fx_t, crush_was, tank_fall_vy,
    tank_obstacle_was, vehicle_contact_was, vehicle_contact_release_t, prev_lmb, prev_primary, he_loaded, demo_struct,
    lan_ai_mask, couch_crew_active, couch_local_peer, couch_commander_yaw, couch_commander_pitch, couch_binocular_raise, canyon_paper,
    mission_flare_slots, harness_futile, fire, fire_active, wreck_deck_anchors, player_surface_fire_started, fire_t,
    fire_ammo_ids, fire_origin, crew_ko_t, crew_ko_world_dir, force_crew_ko, force_crew_ko_dir, replay_cue,
    switch_origin_hull, switch_near_restore, switch_t, switch_dur, prev_space, ram, eject_t,
    ext_t, repair_t, coop_reload_clock, lan_post_mask, next_mg_case, brass_frames, brass_steps,
    brass_dt, brass_sound_credit, ear_pos, ear_forward, next_casing, driver_hatch, panzerfaust_rack_count,
    panzerfaust_source, panzerfaust_claimed, luger_impact_t, luger_impact_pos, held_item, soviet_soldier_count, cover,
    next_noise, next_inf_fx, stat_hits_on_player, stat_pens_on_player, enemy_struck, engine_listen_s, stat_small_arms_hits_on_foot,
    stat_hits_by_player, stat_pens_by_player, stat_kills_by_player, campaign_telemetry_enabled, campaign_foot_dagger_enabled, campaign_foot_dagger_tick, base_occluders,
    spawn_xz, enemy_mg_cd, brawl, targets_only, playtest_return_fire, abandoned, pending_hulk,
    prev_jump, blocked_hatch_exit, death_cause, roof_hatches, driver_shutters, cupola_focus, hover_station,
    carry_mesh, toolbox_rest, track_repair_phase, station, switch_from, activity, breech,
    cartridge_primers, hatch_edges, mode, climb, mission, mission_objectives, compound,
);

fields_owner!(capture_tanks,restore_tanks,"vehicles.tanks";
    player, enemies, allies, campaign_hulks, wrecks, armor_scars, player_powertrain,
    vehicle_finish_seeds, snow_cover, surface_wear,
    exhaust, modules, exterior, head_sway, tank_rest, player_surface_burn, running_gear_motion, gun_elevation_motion,
);

fields_owner!(capture_personal,restore_personal,"personal.runtime";
    foot, pickups, banked, cigarettes, cigarette_butts, hearing, player_burn,
    pressure_arrivals, walk_sounds,
    hand_map_view, repair_manual, cupola_pistol, cigarette_fx, hand_prop_anchors,
);

fields_owner!(capture_projectiles,restore_projectiles,"combat.projectiles";
    projectiles, projectile_scratch, fx, e_shots, blasts, sparks, snow_puffs,
    scheduled, last_fatal, hot_fragment_collisions, hot_fragment_audio_contacts,
    mg_tracers, mg_impacts, casings, mg_cases, panzerfaust_shot, luger_casing_fx,
);

fields_owner!(capture_infantry,restore_infantry,"combat.infantry";
    soldiers, mixed_combat, stealth, noise, enemy_policy, tiger_foot_policy, soviet_foot_policy,
    enemy_crew, infantry_casualties, at_guns, winter_encounter,
);

fields_owner!(capture_environment,restore_environment,"world.environment";
    occluders, story_cover, scatter_heavy_absorbed, destructibles, chunks, surface_blood, interior_fire_lights, world_fire_lights,
);

impl Game {
    pub(super) fn export_game_physical_world(&self,r:&Renderer)->Result<OwnerBlock> {
        let mut fields=self.capture_physical()?;
        self.tiger_refuel.state.validate(self.pickups.len())?;fields.put("tiger_refuel",&self.tiger_refuel.state)?;
        fields.put("first_person",&self.first_person.as_ref().map(|p|p.export_world_state()).transpose()?)?;
        fields.put("renderer_camera",&r.camera)?;fields.put("renderer_time",&r.time)?;
        fields.put("renderer_night",&r.night)?;fields.put("renderer_world_physics",&r.export_world_physics()?)?;
        fields.put("renderer_source_journal",&r.export_actor_source_journal())?;
        fields.put("enemy_source_slots",&[self.slots.t34.clone(),self.slots.su100.clone(),self.slots.kv2.clone(),self.slots.isu152.clone(),self.slots.is2.clone(),self.slots.is3.clone()])?;
        fields.put("enemy_flame_slots",&self.slots.flame)?;
        fields.put("renderer_meshes",&(0..r.mesh_count()).map(|i|r.debug_mesh_state(i).unwrap()).collect::<Vec<_>>())?;
        fields.put("loose",&self.loose.as_slice())?;fields.put("panzerfaust_rack_taken",&self.panzerfaust_rack_taken)?;
        fields.put("inf_flash_t",&self.inf_flash_t)?;fields.put("inf_flash_pos",&self.inf_flash_pos)?;
        OwnerBlock::encode("game.physical",3,&fields)
    }
    pub(super) fn import_game_physical_world(&mut self,r:&mut Renderer,block:&OwnerBlock)->Result<()> {
        let mut fields:StateFields=block.decode("game.physical",3)?;
        let fuel:tiger_refuel::State=fields.take("tiger_refuel")?;fuel.validate(self.pickups.len())?;
        let arms:Option<StateFields>=fields.take("first_person")?;
        ensure!(arms.is_some()==self.first_person.is_some(),"first person source presence mismatch");
        if let Some(arms)=arms {self.first_person.as_ref().unwrap().import_world_state(arms)?;}
        let camera:crate::camera::Camera=fields.take("renderer_camera")?;
        let time:f32=fields.take("renderer_time")?;let night:f32=fields.take("renderer_night")?;
        let meshes:Vec<(bool,Mat4)>=fields.take("renderer_meshes")?;
        ensure!(meshes.len()==r.mesh_count(),"renderer source layout mismatch: saved {} slots, rebuilt {}",meshes.len(),r.mesh_count());
        ensure!(meshes.iter().all(|(_,m)|m.is_finite()),"nonfinite retained renderer pose");
        let journal:Vec<(usize,usize)>=fields.take("renderer_source_journal")?;
        ensure!(journal==r.export_actor_source_journal(),"renderer source journal was not reconstructed");
        let rigs:[Vec<[usize;3]>;6]=fields.take("enemy_source_slots")?;
        let flames:Vec<usize>=fields.take("enemy_flame_slots")?;
        ensure!(rigs.iter().all(|rig|rig.len()==flames.len()&&rig.iter().flatten().all(|slot|*slot<meshes.len()))&&flames.iter().all(|slot|*slot<meshes.len()),"invalid retained enemy source slots");
        [self.slots.t34,self.slots.su100,self.slots.kv2,self.slots.isu152,self.slots.is2,self.slots.is3]=rigs;
        self.slots.flame=flames;
        ensure!(camera.pos.is_finite()&&time.is_finite()&&time>=0.0&&night.is_finite(),"invalid camera/world clock");
        self.loose=fields.take::<Vec<LooseShell>>("loose")?.try_into().map_err(|_|anyhow::anyhow!("loose round capacity mismatch"))?;
        self.panzerfaust_rack_taken=fields.take("panzerfaust_rack_taken")?;
        self.inf_flash_t=fields.take("inf_flash_t")?;self.inf_flash_pos=fields.take("inf_flash_pos")?;
        r.import_world_physics(fields.take("renderer_world_physics")?)?;
        self.restore_physical(fields)?;self.tiger_refuel.state=fuel;
        r.camera=camera;r.time=time;r.night=night;
        for (i,(visible,model)) in meshes.into_iter().enumerate(){r.set_model(i,model);r.set_visible(i,visible);}
        Ok(())
    }
    pub(super) fn export_tanks_world(&self)->Result<OwnerBlock> {
        let mut fields=self.capture_tanks()?;
        let bodies=self.wrecks.iter().map(|w|w.t_body.as_ref().map(wreck_turret::Body::export_world_state).transpose()).collect::<Result<Vec<_>>>()?;
        fields.put("detached_turret_bodies",&bodies)?;
        fields.put("track_snow",&self.track_snow.export_world_state()?)?;
        OwnerBlock::encode("vehicles.tanks",1,&fields)
    }
    pub(super) fn import_tanks_world(&mut self,block:&OwnerBlock)->Result<()> {
        let mut fields:StateFields=block.decode("vehicles.tanks",1)?;
        let bodies:Vec<Option<StateFields>>=fields.take("detached_turret_bodies")?;
        self.track_snow.import_world_state(fields.take("track_snow")?)?;
        let enemy_count=self.slots.t34.len();self.restore_tanks(fields)?;
        ensure!(self.enemies.len()==enemy_count&&self.wrecks.len()==enemy_count&&bodies.len()==enemy_count,"vehicle/source slot count mismatch");
        for (index,body) in bodies.into_iter().enumerate() {
            self.wrecks[index].t_body=body.map(|b|wreck_turret::Body::restore_world_state(self.wreck_turret_sources[wreck_kind_index(self.enemies[index].kind)].clone(),b)).transpose()?;
        }
        ensure!(self.player.pos.is_finite()&&self.enemies.iter().all(|e|e.pos.is_finite()),"nonfinite saved tank pose");Ok(())
    }
    pub(super) fn export_personal_world(&self)->Result<OwnerBlock> {
        let mut fields=self.capture_personal()?;fields.put("hand_props",&self.hand_props.as_ref().map(|p|p.export_world_state()).transpose()?)?;
        OwnerBlock::encode("personal.runtime",1,&fields)
    }
    pub(super) fn import_personal_world(&mut self,block:&OwnerBlock)->Result<()> {
        let mut fields:StateFields=block.decode("personal.runtime",1)?;let props:Option<StateFields>=fields.take("hand_props")?;
        ensure!(props.is_some()==self.hand_props.is_some(),"hand prop source presence mismatch");
        if let Some(props)=props {self.hand_props.as_mut().unwrap().import_world_state(props)?;}
        self.restore_personal(fields)
    }
    pub(super) fn export_projectiles_world(&self)->Result<OwnerBlock> {
        let mut fields=self.capture_projectiles()?;fields.put("hot_fragments",&self.hot_fragments.as_slice())?;
        fields.put("weapon_vfx",&self.weapon_vfx.export_world_state()?)?;
        OwnerBlock::encode("combat.projectiles",1,&fields)
    }
    pub(super) fn import_projectiles_world(&mut self,block:&OwnerBlock)->Result<()> {
        let mut fields:StateFields=block.decode("combat.projectiles",1)?;
        self.weapon_vfx.import_world_state(fields.take("weapon_vfx")?)?;
        self.hot_fragments=fields.take::<Vec<HotFragment>>("hot_fragments")?.try_into().map_err(|_|anyhow::anyhow!("fragment capacity mismatch"))?;
        self.restore_projectiles(fields)?;
        let mut ids=std::collections::BTreeSet::new();
        ensure!(self.projectiles.iter().all(|p|ids.insert(p.id)&&p.id<self.next_projectile_id&&p.pos.is_finite()&&p.velocity.is_finite()),"invalid in-flight projectile identity/pose");Ok(())
    }
    pub(super) fn export_infantry_world(&self)->Result<OwnerBlock> {OwnerBlock::encode("combat.infantry",1,&self.capture_infantry()?)}
    pub(super) fn import_infantry_world(&mut self,block:&OwnerBlock)->Result<()> {self.restore_infantry(block.decode("combat.infantry",1)?)}
    pub(super) fn export_environment_world(&self)->Result<OwnerBlock> {
        let mut fields=self.capture_environment()?;
        fields.put("terrain_detail",&self.hf.detail.as_ref().map(|d|d.export_world_state()).transpose()?)?;
        fields.put("cabin_fire",&self.cabin_fire.export_world_state()?)?;
        fields.put("ammo_fire",&self.ammo_fire)?;
        fields.put("scene_environment",&self.scene_environment.export_world_state()?)?;
        fields.put("acoustics",&self.acoustics.export_world_state()?)?;
        fields.put("grass",&self.grass.as_ref().map(|g|g.export_world_state()).transpose()?)?;
        fields.put("trees",&self.trees.export_world_state()?)?;fields.put("scatter",&self.scatter.export_world_state()?)?;
        fields.put("buildings",&self.buildings.export_world_state()?)?;fields.put("world_doors",&self.world_doors.export_world_state()?)?;
        fields.put("camp_people",&self.camp_people.as_ref().map(|c|c.export_world_state()).transpose()?)?;
        OwnerBlock::encode("world.environment",2,&fields)
    }
    pub(super) fn import_environment_world(&mut self,r:&mut Renderer,block:&OwnerBlock)->Result<()> {
        let mut fields:StateFields=if block.version==1 {
            ensure!(self.hf.detail.is_none(),"legacy world.environment1 omitted physical terrain damage; cannot resume a Detail terrain without inventing lost crater/rut state");
            block.decode("world.environment",1)?
        }else{block.decode("world.environment",2)?};
        if block.version==2 {
            let terrain:Option<OwnerBlock>=fields.take("terrain_detail")?;
            ensure!(terrain.is_some()==self.hf.detail.is_some(),"saved physical terrain detail presence differs");
            if let Some(terrain)=terrain {let detail=self.hf.detail.as_ref().unwrap();
                ensure!(r.terrain_detail.as_ref().is_some_and(|render|std::sync::Arc::ptr_eq(render,detail)),"renderer and physics do not share the restored terrain detail");
                detail.import_world_state(&terrain)?;r.invalidate_terrain_detail();
            }
        }
        self.cabin_fire.import_world_state(fields.take("cabin_fire")?)?;self.ammo_fire=fields.take("ammo_fire")?;
        self.scene_environment.import_world_state(fields.take("scene_environment")?)?;
        self.acoustics.import_world_state(fields.take("acoustics")?)?;
        let grass:Option<StateFields>=fields.take("grass")?;
        ensure!(grass.is_some()==self.grass.is_some(),"grass source presence mismatch");
        if let Some(grass)=grass {self.grass.as_mut().unwrap().import_world_state(grass)?;}
        self.trees.import_world_state(fields.take("trees")?)?;self.scatter.import_world_state(fields.take("scatter")?)?;
        let hf=&self.hf;self.buildings.import_world_state(fields.take("buildings")?,&|p|hf.height_at(p.x,p.y))?;
        self.world_doors.import_world_state(fields.take("world_doors")?)?;
        let camp:Option<StateFields>=fields.take("camp_people")?;
        ensure!(camp.is_some()==self.camp_people.is_some(),"camp population source layout mismatch");
        if let Some(camp)=camp {self.camp_people.as_mut().unwrap().import_world_state(camp)?;}
        self.restore_environment(fields)?;
        self.trees.present_world_state(r);self.scatter.present_world_state(r);self.world_doors.upload(r);
        if let Some(camp)=&self.camp_people {camp.present_world_state(r)?;}
        Ok(())
    }
}

/// Cross-owner references must be admitted against the retained arrays, never fresh defaults.
fn validate_theatre_sources(t:&expedition_theatre_game::TheatreRuntime,soldiers:usize,meshes:usize,pickups:usize)->Result<()> {
    ensure!(t.sites.iter().all(|site|site.infantry.iter().all(|id|*id<soldiers)),"retained theatre infantry identity out of bounds");
    let sentries:std::collections::BTreeSet<usize>=t.sites.iter().flat_map(|site|site.infantry.iter().copied()).collect();
    if t.initialized {
        ensure!(t.sites.iter().all(|site|site.infantry.len()==2)&&sentries.len()==soldiers&&soldiers==t.sites.len()*2,
            "initialized theatre must retain its exact finite sentry population");
    } else {ensure!(sentries.is_empty()&&soldiers==0,"uninitialized theatre contains infantry state");}
    ensure!(t.water_mesh<meshes&&t.crossing_meshes.iter().all(|slot|*slot<meshes),"retained theatre surface source out of bounds");
    ensure!(t.fuel_pickups.iter().all(|(_,index)|*index<pickups),"retained fuel pickup identity out of bounds");
    for ledger in &t.relays {
        ensure!(ledger.paper_slot<meshes&&ledger.position.is_finite()&&t.sites.iter().any(|site|site.id==ledger.site),"invalid retained relay source/position");
    }
    let mut ids=std::collections::BTreeSet::new();
    for cart in &t.ammo_carts {
        ensure!(ids.insert(cart.id)&&cart.position.is_finite()&&cart.heading.is_finite()&&cart.spin.is_finite(),"invalid retained ammunition cart identity/pose");
        ensure!(cart.body<meshes&&cart.wheels.iter().chain(&cart.rounds).all(|slot|*slot<meshes)&&t.sites.iter().any(|site|site.id==cart.site),"retained ammunition cart source out of bounds");
    }
    Ok(())
}

impl Game {
    pub fn expedition_world_recipe(&self)->Result<crate::expedition_world_save::WorldRecipe> {
        let state=self.expedition_state().context("no live expedition")?;
        let root=self.support_camp.as_ref().context("no physical support camp")?.placement.root;
        let path=crate::act_manifest::path_for_act(self.act.id).to_string_lossy().replace('\\',"/");
        crate::expedition_world_save::WorldRecipe::capture(path,state.run_seed,self.act.id,root)
    }
    /// Snapshot the complete owner set. The frontend must never substitute this for an older
    /// coordinator checkpoint. Import is deliberately limited to a newly built candidate Game.
    pub fn export_expedition_world(&mut self,r:&Renderer,world:crate::expedition_world_save::WorldRecipe)
        ->Result<crate::expedition_world_save::WorldSave> {
        self.validate_expedition_world_mode()?;self.sync_expedition_now()?;
        let runtime=self.expedition_runtime.as_ref().context("no live expedition")?;
        runtime.state.validate()?;
        if let Some(theatre)=&self.theatre {theatre.validate(self.enemies.len())?;theatre.validate_coordinator(&runtime.state)?;validate_theatre_sources(theatre,self.soldiers.len(),r.mesh_count(),self.pickups.len())?;}
        world.verify_sources()?;
        ensure!(world.act_id==self.act.id,"world recipe act differs from running battlefield");
        ensure!(self.support_camp.as_ref().is_some_and(|camp|camp.placement.root.to_cols_array()==world.camp_root),"world recipe does not describe current camp root");
        let owners=vec![self.export_game_physical_world(r)?,self.export_tanks_world()?,
            self.export_crew_world_state()?,self.export_transport_world_state()?,
            self.export_workshop_world_state()?,
            OwnerBlock::encode("world.progression",3,&self.theatre)?,
            self.export_environment_world()?,self.export_projectiles_world()?,
            self.export_infantry_world()?,self.export_expedition_runtime_world()?,self.export_personal_world()?];
        crate::expedition_world_save::WorldSave::new(world,runtime.state.simulation_seconds,owners)
    }
    /// Shared candidate constructor used by the window Resume path and its headless witness.
    /// The caller keeps the active world alive until this has fully validated all owners.
    pub fn build_expedition_resume_candidate(device:&wgpu::Device,queue:&wgpu::Queue,
        save:&crate::expedition_world_save::WorldSave,with_menu:bool)->Result<(Renderer,Game)> {
        save.verify_for_resume()?;
        let prior=crate::act_manifest::selected_runtime_manifest().map(|(_,path)|path);
        let result=(||->Result<(Renderer,Game)>{
            ensure!(crate::act_manifest::select_runtime_manifest(Some(save.world.manifest_path.clone().into()))?==Some(save.world.act_id),"saved manifest act differs");
            let (mut r,mut g)=Game::build_expedition_act(device,queue,false,&crate::world::act(save.world.act_id))?;
            g.set_audio_paused(true);g.set_defer_rtt_to_render(true);
            g.enable_expedition(save.world.run_seed,g.expedition_initial_camp_position()?)?;
            if with_menu {r.load_menu_meshes(device,queue,&tank::build_tiger());}
            g.import_expedition_world(&mut r,save)?;
            g.present_restored_expedition_print(&mut r,device,queue,save.frontend.as_ref());
            Ok((r,g))
        })();
        if result.is_err(){let _=crate::act_manifest::select_runtime_manifest(prior);}
        result
    }
    /// Recreate derived ink/material bindings after import, without moving the saved paper,
    /// turning a page, replaying a hand action or advancing any simulation owner.
    pub(super) fn present_restored_expedition_print(&self,r:&mut Renderer,device:&wgpu::Device,
        queue:&wgpu::Queue,frontend:Option<&crate::expedition_world_save::FrontendState>) {
        if self.map_held {
            r.ui_reset();
            if !self.draw_expedition_map(r) {
                let pencil=frontend.map_or(&[][..],|f|f.pencil_marks.as_slice());
                let pins=frontend.map_or(&[][..],|f|f.map_pins.as_slice());
                let cursor=frontend.map_or(Vec2::splat(0.5),|f|f.map_cursor);
                let direction=r.camera.forward();let heading=crate::world::heading_of(Vec2::new(direction.x,direction.z));
                crate::ui::draw_held_map_print(r,&self.act,&self.marks,pencil,pins,None,0,cursor,
                    heading,false,self.map_world_position(),self.mission_map_guidance().as_ref());
                self.draw_campaign_building_map(r);
            }
            r.render_held_map_texture(device,queue,self.slots.held_map);
        }
        if self.repair_manual.held {
            r.render_repair_manual_pages(device,queue,self.slots.repair_manual,&self.repair_manual);
        }
        r.ui_reset();
    }
    /// Restore ONLY into a disposable Game built from save.world. The frontend retains its
    /// current Game until this returns success. No simulation tick or dispatch runs here.
    pub fn import_expedition_world(&mut self,r:&mut Renderer,save:&crate::expedition_world_save::WorldSave)->Result<()> {
        save.verify_for_resume()?;
        ensure!(save.world.act_id==self.act.id,"resume source act mismatch");
        self.validate_expedition_world_mode()?;
        let theatre:Option<Box<expedition_theatre_game::TheatreRuntime>>=save.owner("world.progression")?.decode("world.progression",3)?;
        let mut retained_physical:StateFields=save.owner("game.physical")?.decode("game.physical",3)?;
        let retained_meshes:Vec<(bool,Mat4)>=retained_physical.take("renderer_meshes")?;
        let mut retained_infantry:StateFields=save.owner("combat.infantry")?.decode("combat.infantry",1)?;
        let retained_soldiers:Vec<Soldier>=retained_infantry.take("soldiers")?;
        let mut retained_personal:StateFields=save.owner("personal.runtime")?.decode("personal.runtime",1)?;
        let retained_pickups:Vec<Pickup>=retained_personal.take("pickups")?;
        if let Some(t)=&theatre {validate_theatre_sources(t,retained_soldiers.len(),retained_meshes.len(),retained_pickups.len())?;}
        let mut tanks:StateFields=save.owner("vehicles.tanks")?.decode("vehicles.tanks",1)?;
        let saved_player:Tank=tanks.take("player")?;
        let saved_enemies:Vec<Tank>=tanks.take("enemies")?;
        if let Some(t)=&theatre {t.validate(saved_enemies.len())?;}
        ensure!(theatre.is_some()==self.theatre.is_some(),"theatre source presence mismatch");
        // Allocate the same immutable source pages as the original initial theatre. All fresh
        // coordinator/world values are replaced below before the candidate can be presented.
        if theatre.as_ref().is_some_and(|t|t.initialized) && self.theatre.as_ref().is_some_and(|t|!t.initialized) {
            self.enable_expedition_theatre(r)?;
        }
        ensure!(saved_enemies.len()>=self.enemies.len()&&(saved_enemies.len()-self.enemies.len())%stealth::PLATOON_SIZE==0,"invalid dynamic vehicle source count");
        // Flares and later reinforcements may interleave. Replaying only tank allocations would
        // assign their meshes to the wrong identities; replay the actual immutable-source journal.
        let mut source_fields:StateFields=save.owner("game.physical")?.decode("game.physical",3)?;
        let journal:Vec<(usize,usize)>=source_fields.take("renderer_source_journal")?;
        let rigs:[Vec<[usize;3]>;6]=source_fields.take("enemy_source_slots")?;
        let flames:Vec<usize>=source_fields.take("enemy_flame_slots")?;
        ensure!(rigs.iter().all(|rig|rig.len()==saved_enemies.len())&&flames.len()==saved_enemies.len(),"retained vehicle source identities differ from vehicle owner");
        for (saved,current) in rigs.iter().zip([&self.slots.t34,&self.slots.su100,&self.slots.kv2,&self.slots.isu152,&self.slots.is2,&self.slots.is3]) {
            ensure!(saved.starts_with(current),"retained vehicle source prefix differs");
        }
        ensure!(flames.starts_with(&self.slots.flame),"retained flame source prefix differs");
        r.restore_actor_source_journal(&journal)?;
        self.prepare_retained_crew_sources(r,save.owner("crew.runtime")?)?;
        self.relocate_camp_geometry(Mat4::from_cols_array(&save.world.camp_root),r)?;
        ensure!(self.support_camp.as_ref().is_some_and(|camp|camp.placement.root.to_cols_array()==save.world.camp_root),"candidate camp must be relocated to saved root before import");
        self.import_game_physical_world(r,save.owner("game.physical")?)?;
        self.import_tanks_world(save.owner("vehicles.tanks")?)?;
        self.import_personal_world(save.owner("personal.runtime")?)?;
        self.import_expedition_runtime_world(save.owner("expedition.runtime")?)?;
        self.import_crew_world_state(save.owner("crew.runtime")?,saved_player.hull_model())?;
        self.import_transport_world_state(save.owner("transport.recovery")?)?;
        self.import_projectiles_world(save.owner("combat.projectiles")?)?;
        self.import_infantry_world(save.owner("combat.infantry")?)?;
        self.import_environment_world(r,save.owner("world.environment")?)?;
        self.import_workshop_world_state(save.owner("workshop.runtime")?)?;
        self.theatre=theatre;
        self.validate_expedition_world_mode()?;
        let restored_state=&self.expedition_runtime.as_ref().context("missing restored expedition")?.state;
        restored_state.validate()?;
        if let Some(t)=&self.theatre {t.validate_coordinator(restored_state)?;}
        ensure!(self.expedition_runtime.as_ref().unwrap().state.simulation_seconds.to_bits()==save.simulation_seconds.to_bits(),"world/expedition clock mismatch");
        self.present_restored_crew_world_state(r);self.present_restored_transport_world_state(r)?;
        if let Some(props)=&mut self.hand_props {props.present_world_state(r)?;}
        if let Some(arms)=&self.first_person {arms.present_world_state(self,r);}
        self.present_restored_buildings(r)?;
        self.enemy_crew.draw(r);self.present_workshop_world(r);self.present_tiger_refueling(r);
        self.scar_geometry_cache.borrow_mut().clear();
        self.drive_armor_scars(r);self.drive_lights(r);
        // Owner-specific source deformation is now restored. Reapply the exact retained draw
        // lists last; no zero-time physics solver or emitter generation is permitted here.
        let mut presentation:StateFields=save.owner("game.physical")?.decode("game.physical",3)?;
        r.import_world_physics(presentation.take("renderer_world_physics")?)?;
        r.camera=presentation.take("renderer_camera")?;r.time=presentation.take("renderer_time")?;r.night=presentation.take("renderer_night")?;
        for (i,(visible,model)) in presentation.take::<Vec<(bool,Mat4)>>("renderer_meshes")?.into_iter().enumerate(){r.set_model(i,model);r.set_visible(i,visible);}
        self.weather_view=r.weather_visibility();self.weather_view.set_cabin_pose(self.player.hull_model(),self.player.turret_model());
        self.projectile_cover=projectile_cover::Index::build(&self.story_cover,&self.destructibles);
        self.story_cover_shared=std::sync::Arc::new(self.story_cover.clone());
        self.world_index=std::sync::Arc::new(world_index::WorldIndex::build(&self.destructibles,&self.occluders[..self.base_occluders],&self.story_cover,&self.buildings));
        Ok(())
    }
    fn validate_expedition_world_mode(&self)->Result<()> {
        ensure!(self.expedition_runtime.is_some()&&self.singleplayer_crew_active,"complete world saves require a live solo expedition");
        ensure!(self.lan_local_role.is_none()&&!self.couch_crew_active,"network/couch sessions cannot use solo world saves");
        ensure!(self.campaign.is_none()&&self.canyon_run.is_none()&&self.winter_mission.is_none()&&self.platoon.is_none()&&self.tank_editor_scene.is_none(),"legacy campaign/tutorial/director cannot masquerade as complete expedition");
        ensure!(self.death_replay.is_none(),"finish the presentation-only death replay before saving");
        Ok(())
    }
    /// Rebuild only draw bindings from exact saved structural state. Calling the legacy quiet
    /// reconciliation here would quantize proxy damage and settle the saved pile a second time.
    fn present_restored_buildings(&mut self,r:&mut Renderer)->Result<()> {
        for instance in 0..self.buildings.len() {
            let building=self.buildings.get(instance);
            {
                for update in building.render_snapshot()? {
                    let intact=update.state==crate::building_blueprint::ModuleRenderState::Intact;
                    for binding in building.bindings(update.placement_index) {
                        r.set_ref_visible(binding.intact,intact);
                        if let Some(rubble)=binding.rubble {r.set_ref_visible(rubble,!intact);}
                    }
                }
            }
            for module in 0..building.layout().placements().len() {
                if building.module_intact(module) {continue;}
                let mound=self.buildings.rubble_mound(instance,module);
                for binding in building.bindings(module) {if let Some(rubble)=binding.rubble {
                    r.set_ref_visible(rubble,mound.is_some());
                    if let (Some(mound),crate::renderer::SceneRef::Instance(handle))=(mound,rubble) {r.set_instance_matrix(handle,mound.transform);}
                }}
            }
            self.sync_tableau_attachments(instance,r);
        }
        Ok(())
    }
}
