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
        self.cigarettes.draw_or_inspect();self.personal_primary_guard=true;self.consume_primary_press();
        true
    }
    pub fn toggle_hand_flashlight(&mut self)->bool {
        if self.hand_torch_on {self.hand_torch_on=false;self.personal_primary_guard=true;return true;}
        if !self.personal_hands_available() {return false;}
        self.cigarettes.stow();self.compass_equipped=false;self.holster_mounted_sidearm();
        if self.mode==Mode::Tank&&self.station==Station::Gunner {self.jump_station(Station::Loader);}
        self.hand_torch_on=true;self.personal_primary_guard=true;self.consume_primary_press();true
    }
    pub fn stow_personal_hand_item(&mut self)->bool {
        let active=self.cigarettes.stow()||self.hand_torch_on;
        self.hand_torch_on=false;
        if active {self.personal_primary_guard=true;self.consume_primary_press();}active
    }
    pub(super) fn route_personal_hand_input(&mut self,input:&mut TankInput,dt:f32) {
        let primary=input.fire2||input.mg_coax;
        if !primary {self.personal_primary_guard=false;}
        if self.personal_hand_item_active()&&!self.personal_hands_remain_available() {self.stow_personal_hand_item();}
        // A cigarette that burns out frees the hand mid-hold; that held press belonged to the
        // cigarette and must not become a weapon trigger on the same frame.
        if self.cigarettes.advance(dt,primary,input.aim)==crate::cigarette::Action::BurnedOut&&primary {
            self.personal_primary_guard=true;
        }
        if self.personal_hand_item_active()||self.personal_primary_guard {
            mask_hand_input(input,self.lan_local_role);
        }
    }
    pub fn cigarette_snapshot(&self)->serde_json::Value {
        let v=self.cigarettes.view();
        serde_json::json!({"equipped":v.equipped,"pack_open":v.pack_open,"pack_count":v.pack_count,
            "reserve_count":v.reserve_count,"cigarette_present":v.cigarette_present,"lit":v.lit,
            "burn_remaining":v.burn_remaining,"lighter_raise":v.lighter_raise,"lighter_flame":v.lighter_flame,
            "puff_phase":v.puff_phase,"exhale":v.exhale,"drawn":self.cigarettes.drawn(),
            "consumed":self.cigarettes.consumed(),"puffs":self.cigarettes.puffs(),"lights":self.cigarettes.lit_count(),
            "total_unconsumed":self.cigarettes.total_unconsumed(),"flashlight_equipped":self.hand_torch_on,
            "effects":self.cigarette_fx.receipt(),"named_audio":self.audio.as_ref().map(|audio|audio.named_effect_snapshot()),"source_props_loaded":self.hand_props.is_some(),
            "collected_world_packs":self.pickups.iter().filter(|p|p.kind==PickupKind::Cigarettes&&p.collected).count(),
            "scope":"Finite physical inventory and actual simulation timers;20 per pack,240s idle burn and5x extra inhale burn are authored estimates"})
    }
    pub(super) fn update_personal_hand_presentation(&mut self,dt:f32,r:&mut Renderer) {
        let view=self.cigarettes.view();
        let visible=self.personal_hands_remain_available()&&r.optic_mask==0.0;
        self.hand_prop_anchors=self.hand_props.as_mut().map_or_else(
            crate::hand_prop_models::Anchors::default,|props|props.update(r,view,visible));
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
