//! Local physical cigarette/flashlight hands. Ordinary gun/station input is masked only while held.
use super::*;

fn mask_hand_input(input:&mut TankInput,local_role:Option<crate::lan::LanRole>) {
    input.fire=false;input.fire2=false;input.mg_coax=false;input.coax=false;
    input.aim=false;input.binoculars=false;input.reload=false;input.extinguish=false;input.repair=false;
    if let (Some(frame),Some(role))=(&mut input.coop,local_role) {
        frame.inputs[role as usize].flags &= !(crate::lan::F_FIRE_MAIN|crate::lan::F_FIRE_COAX
            |crate::lan::F_FIRE_HULL|crate::lan::F_SERVICE_BREECH|crate::lan::F_EXTINGUISH
            |crate::lan::F_REPAIR|crate::lan::F_BINOCULARS);
    }
}
impl Game {
    pub fn personal_hand_item_active(&self)->bool {self.cigarettes.active()||self.hand_torch_on}
    /// What takes the left button away from the world: the torch, and the open packet (the
    /// button picks a cigarette). A cigarette between the fingers takes nothing: stations,
    /// hatches, sights and every control stay within reach.
    pub fn personal_item_blocks_interaction(&self)->bool {
        self.hand_torch_on||{let v=self.cigarettes.view();v.equipped&&v.pack_open}
    }
    fn personal_hands_available(&self)->bool {self.switch_t<=0.0&&self.personal_hands_remain_available()}
    fn personal_hands_remain_available(&self)->bool {
        (if self.mode==Mode::Foot {self.foot.hp>0.0}else{self.player.alive})
            && !self.climb.active && self.activity.is_none()
            && !self.map_held && self.carry_mesh.is_none() && self.carried_casing.is_none()
            && self.held_item.is_none() && !self.carried_tools() && !self.panzerfaust_claimed
            && !self.ext_equipped
    }
    pub fn toggle_cigarette(&mut self)->bool {
        if !self.personal_hands_available()||self.hand_props.is_none() {return false;}
        // With no cigarette anywhere, C must not stow the torch or pull the gunner off his sights.
        if self.cigarettes.total_unconsumed()==0 {return false;}
        self.hand_torch_on=false;self.compass_equipped=false;self.holster_mounted_sidearm();
        if self.mode==Mode::Tank&&self.station==Station::Gunner {self.jump_station(Station::Loader);}
        let was_open=self.cigarettes.view().pack_open;
        self.cigarettes.toggle_pack();self.personal_primary_guard=true;self.consume_primary_press();
        self.foley_hands(crate::acoustics::foley::Cue::Cigarette(if was_open {crate::acoustics::foley::CigCue::PackClose} else {crate::acoustics::foley::CigCue::PackOpen}));
        true
    }
    /// Review aid for headless stills: put the cigarette kit into a named state.
    pub fn debug_cigarette(&mut self,state:&str) {
        use crate::cigarette::Action;
        if !self.cigarettes.active() {self.cigarettes.toggle_pack();}
        if state=="pack" {return;}
        self.cigarettes.take(3);self.cigarettes.advance(0.1,false);
        if state=="hand" {return;}
        for _ in 0..400 {if self.cigarettes.advance(0.01,true)==Action::Light {break;}}
        self.cigarettes.advance(0.4,false);
        match state {
            "toke" => {for _ in 0..80 {self.cigarettes.advance(0.01,true);}}
            "mouth" => {self.cigarettes.stow();}
            "boost" => {for _ in 0..3 {for _ in 0..100 {self.cigarettes.advance(0.01,true);}
                self.cigarettes.advance(0.01,false);self.cigarettes.advance(3.0,false);}}
            _ => {}
        }
    }
    pub fn toggle_hand_flashlight(&mut self)->bool {
        if self.hand_torch_on {self.hand_torch_on=false;self.personal_primary_guard=true;
            self.foley_hands(crate::acoustics::foley::Cue::Cabin(crate::acoustics::foley::CabinCue::LampSwitch));return true;}
        if !self.personal_hands_available() {return false;}
        self.cigarettes.stow();self.compass_equipped=false;self.holster_mounted_sidearm();
        if self.mode==Mode::Tank&&self.station==Station::Gunner {self.jump_station(Station::Loader);}
        self.hand_torch_on=true;self.personal_primary_guard=true;self.consume_primary_press();
        self.foley_hands(crate::acoustics::foley::Cue::Handle(crate::acoustics::foley::Object::Flashlight,crate::acoustics::foley::Gesture::Grab));
        self.foley_hands(crate::acoustics::foley::Cue::Cabin(crate::acoustics::foley::CabinCue::LampSwitch));true
    }
    pub fn stow_personal_hand_item(&mut self)->bool {
        let active=self.cigarettes.stow()||self.hand_torch_on;
        self.hand_torch_on=false;
        if active {self.personal_primary_guard=true;self.consume_primary_press();}active
    }
    pub(super) fn route_personal_hand_input(&mut self,input:&mut TankInput,dt:f32) {
        // The right button works the cigarette (light, then toke), so the left button and every
        // interaction stay free with one between the fingers. Only the open packet takes the
        // left button: it picks the cigarette the eye rests on.
        let torch_interaction=input.interaction&&input.mg_coax;
        let left=input.trigger||input.fire2||input.mg_coax;
        let right=input.aim||input.binoculars;
        if !left {self.personal_primary_guard=false;}
        // Hands needed elsewhere: a lit cigarette goes to the mouth and keeps burning.
        if self.personal_hand_item_active()&&!self.personal_hands_remain_available() {self.stow_personal_hand_item();}
        let before=self.cigarettes.view();
        if before.pack_open&&left&&!self.cigarette_left_was_down&&!self.personal_primary_guard {
            if let Some(slot)=self.cigarette_pick {
                if self.cigarettes.take(slot)==crate::cigarette::Action::Draw {
                    self.personal_primary_guard=true;self.cigarette_pick=None;
                    self.foley_hands(crate::acoustics::foley::Cue::Cigarette(crate::acoustics::foley::CigCue::DrawCigarette));
                }
            }
        }
        self.cigarette_left_was_down=left;
        // The kit's own events, voiced: the lighter, each draw, the cough, the ember dying.
        match self.cigarettes.advance(dt,right&&self.cigarettes.active()) {
            crate::cigarette::Action::Light => {
                self.foley_hands(crate::acoustics::foley::Cue::Cigarette(crate::acoustics::foley::CigCue::Flint));
                self.foley_mouth(crate::acoustics::foley::Cue::Cigarette(crate::acoustics::foley::CigCue::Flame));
            }
            crate::cigarette::Action::Puff => self.foley_mouth(crate::acoustics::foley::Cue::Cigarette(crate::acoustics::foley::CigCue::Inhale)),
            crate::cigarette::Action::Cough => self.foley_mouth(crate::acoustics::foley::Cue::Cigarette(crate::acoustics::foley::CigCue::Cough)),
            crate::cigarette::Action::BurnedOut => self.foley_mouth(crate::acoustics::foley::Cue::Cigarette(crate::acoustics::foley::CigCue::BurnOut)),
            _ => {}
        }
        if self.cigarettes.take_butt() {
            self.cigarette_butt_pending=true;
            self.foley_hands(crate::acoustics::foley::Cue::Cigarette(crate::acoustics::foley::CigCue::ButtFlick));
        }
        // Each inhaled second restores the body; one cigarette's worth of tokes is one full bar.
        let dose=self.cigarettes.take_dose();
        if dose>0.0 {
            self.foot.hp=(self.foot.hp+dose*crate::foot::FOOT_HP/crate::cigarette::TOKE_SECONDS_PER_CIGARETTE)
                .min(crate::foot::FOOT_HP);
        }
        if self.hand_torch_on||self.personal_primary_guard||before.pack_open {
            mask_hand_input(input,self.lan_local_role);
            if torch_interaction&&self.hand_torch_on {input.mg_coax=true;}
        }else if self.cigarettes.active() {
            // The right button is the cigarette's; everything else is untouched.
            input.aim=false;input.binoculars=false;
            if let (Some(frame),Some(role))=(&mut input.coop,self.lan_local_role) {
                frame.inputs[role as usize].flags&=!crate::lan::F_BINOCULARS;
            }
        }
    }
    pub fn cigarette_snapshot(&self)->serde_json::Value {
        let v=self.cigarettes.view();
        serde_json::json!({"equipped":v.equipped,"pack_open":v.pack_open,"pack_count":v.pack_count,
            "reserve_count":v.reserve_count,"cigarette_present":v.cigarette_present,"lit":v.lit,
            "burn_remaining":v.burn_remaining,"in_mouth":v.in_mouth,"boost_seconds":v.boost_seconds,
            "butts":self.cigarette_butts.iter().filter(|b|b.active).count(),"lighter_raise":v.lighter_raise,"lighter_flame":v.lighter_flame,
            "puff_phase":v.puff_phase,"exhale":v.exhale,"drawn":self.cigarettes.drawn(),
            "consumed":self.cigarettes.consumed(),"puffs":self.cigarettes.puffs(),"lights":self.cigarettes.lit_count(),
            "total_unconsumed":self.cigarettes.total_unconsumed(),"flashlight_equipped":self.hand_torch_on,
            "effects":self.cigarette_fx.receipt(),"named_audio":self.audio.as_ref().map(|audio|audio.named_effect_snapshot()),"source_props_loaded":self.hand_props.is_some(),
            "collected_world_packs":self.pickups.iter().filter(|p|p.kind==PickupKind::Cigarettes&&p.collected).count(),
            "scope":"Finite physical inventory and actual simulation timers;20 per pack and a 60s burn are authored estimates"})
    }
    pub(super) fn update_personal_hand_presentation(&mut self,dt:f32,r:&mut Renderer) {
        let view=self.cigarettes.view();
        let face=r.optic_mask==0.0;
        let visible=self.personal_hands_remain_available()&&face;
        self.hand_prop_anchors=self.hand_props.as_mut().map_or_else(
            crate::hand_prop_models::Anchors::default,|props|props.update(r,view,visible,face));
        self.cigarette_pick=if visible {self.hand_props.as_ref()
            .and_then(|props|props.pick(r.camera.pos,r.camera.forward(),view))}else{None};
        // OWNER-DIRECTED PRESENTATION: recent tokes sharpen and brighten the eye's picture.
        r.vision_boost=view.vision_boost;
        if std::mem::take(&mut self.cigarette_butt_pending) {
            self.throw_cigarette_butt(self.hand_prop_anchors.tip,r.camera.forward());
        }
        self.drive_cigarette_butts(dt,r);
        let tip=self.hand_prop_anchors.tip;
        // Classify the actual source tip in the same hull/turret cabin domain used by weather,
        // rather than guessing from the camera role or sending blizzard wind through armor.
        let weather=r.weather_visibility();
        let inside=weather.cabin.as_ref().is_some_and(|domain| {
            let span=domain.next_span(&weather.cabin_uniform,tip,Vec3::Y,0.0,0.01);
            span[0]<=0.00001&&span[1]>span[0]+0.00001
        });
        let shelter=self.building_weather_shelter_at(tip).clamp(0.0,1.0);
        let air=crate::cigarette_vfx::Air{hull_model:self.player.hull_model(),inside,
            world_velocity:Vec3::new(r.weather_wind_world.x,0.0,r.weather_wind_world.y)*(1.0-shelter),
            cabin_velocity:Vec3::ZERO};
        self.cigarette_fx.update(dt,view,&self.hand_prop_anchors,air);
        if !self.defer_presentation_rebuild {self.cigarette_fx.submit(r,&self.cigarette_fx_slots);}
    }
    fn cigarette_pickup_hover(&self,forward:Vec3,eye:Vec3)->Option<usize> {
        if self.mode!=Mode::Foot||self.climb.active||self.foot.hp<=0.0 {return None;}
        self.pickups.iter().enumerate().filter(|(_,p)|p.kind==PickupKind::Cigarettes&&!p.collected)
            .filter_map(|(i,p)| {
                let center=p.pos+Vec3::Y*0.05;let delta=center-eye;let distance=delta.length();
                let aim=delta.normalize_or_zero().dot(forward.normalize_or_zero());
                (distance<=2.1&&aim>=0.94&&self.equipment_path_clear(eye,center,0.06))
                    .then_some((i,aim-distance*0.025))
            }).max_by(|a,b|a.1.total_cmp(&b.1)).map(|(i,_)|i)
    }
    /// E collects the actual looked-at world packet once, hiding that same source-mesh slot.
    pub fn interact_cigarette_pickup(&mut self,forward:Vec3,eye:Vec3,r:&mut Renderer)->bool {
        let Some(i)=self.cigarette_pickup_hover(forward,eye)else{return false;};
        if !self.cigarettes.accept_packet() {return false;}
        self.pickups[i].collected=true;r.set_visible(self.pickups[i].slot,false);
        self.foot.supplies.cigarettes=self.foot.supplies.cigarettes.saturating_add(1); // cache statistic, not another consumable pool
        self.consume_primary_press();
        if let Some(a)=&self.audio {
            let level=crate::sound_levels::QUIET_HANDLING.laf_db+crate::audio::effort_db(0.45);
            let bearing=bearing_deg_from(forward,self.pickups[i].pos-eye);
            if !a.play_named_control("cigarette_pickup",level,self.listener_headset(),bearing) {
                a.play_at_ear(crate::audio::Sfx::Pickup,level);
            }
        }
        true
    }
    pub(super) fn update_cigarette_pickup_prompt(&mut self,r:&Renderer) {
        if self.cigarette_pickup_hover(r.camera.forward(),r.camera.pos).is_some() {
            self.hover_label=Some("CIGARETTE PACK [E]");
        }
        let view=self.cigarettes.view();
        if view.pack_open&&!view.cigarette_present {
            self.hover_label=Some(if self.cigarette_pick.is_some() {"TAKE CIGARETTE [LMB]"}
                else{"LOOK AT A CIGARETTE - C PUTS THE PACK AWAY"});
        }else if view.equipped&&view.cigarette_present&&!view.pack_open&&self.hover_label.is_none() {
            // Once the eye is already sharpened the reminder is dropped.
            if !view.lit {self.hover_label=Some("LIGHT [HOLD RMB]");}
            else if view.boost_seconds<=0.0&&view.toke_seconds<=0.0 {self.hover_label=Some("TOKE [HOLD RMB]");}
        }
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn personal_hands_mask_only_local_weapons_not_other_crew_or_movement(){
        let mut input=TankInput{fire:true,fire2:true,mg_coax:true,coax:true,aim:true,
            drive:0.6,steer:0.2,coop:Some(crate::lan::CrewFrame::default()),..Default::default()};
        let local=crate::lan::LanRole::Commander;let other=crate::lan::LanRole::Gunner;
        for post in [local,other] {input.coop.as_mut().unwrap().inputs[post as usize].flags=crate::lan::F_FIRE_MAIN;}
        mask_hand_input(&mut input,Some(local));
        assert!(!input.fire&&!input.fire2&&!input.mg_coax&&!input.coax&&!input.aim);
        assert_eq!(input.drive,0.6);assert_eq!(input.steer,0.2);
        assert!(!input.coop.unwrap().input(local).flag(crate::lan::F_FIRE_MAIN));
        assert!(input.coop.unwrap().input(other).flag(crate::lan::F_FIRE_MAIN));
    }
}
