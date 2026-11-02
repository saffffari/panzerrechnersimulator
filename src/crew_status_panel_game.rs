//! Live boundary for the physical crew-seat instrument. The assignment coordinator supplies
//! current occupancy; stable actor/person records supply names, training and actual wounds.
use super::*;
use super::crew_status_panel::{Condition, SeatStatus};

const SEATS: [CrewRole; 5] = [CrewRole::Commander,CrewRole::Gunner,CrewRole::Loader,CrewRole::Radioman,CrewRole::Driver];
const REACH: f32 = 2.0;

impl Game {
    pub(super) fn crew_status_panel_states(&self)->[SeatStatus;5] {
        std::array::from_fn(|i| {
            let Some(actor)=self.crew_assignment_seat_actor(SEATS[i]) else {return SeatStatus::default();};
            // Cleared bodies and evacuated living casualties no longer occupy a cabin seat.
            if self.crew_solo.actor_body_cleared(actor)
                || self.couch_ragdolls.iter().any(|body|body.casualty.peer_id==actor && body.world_space) {
                return SeatStatus::default();
            }
            let Some(casualty)=self.singleplayer_actor_casualty(actor) else {return SeatStatus::default();};
            let person_id=self.singleplayer_person_for_actor(actor);
            let person=person_id.and_then(|id|self.expedition_runtime.as_ref()?.state.person(id));
            let source_role=self.singleplayer_actor_source_identity(actor)
                .map(crew_solo::role_of_post).unwrap_or(SEATS[i]);
            let (name,trained_role)=person.map(|p|(
                format!("{} {}",p.rank,p.surname),format!("{:?}",p.trained_role)))
                .unwrap_or_else(||(format!("{} crewman",source_role.name()),source_role.name().to_owned()));
            SeatStatus { actor_id:Some(actor),person_id:person_id.map(|id|id.0),name,trained_role,
                condition:Condition::from_casualty(casualty),moving:self.crew_assignment_actor_in_transit(actor) }
        })
    }

    fn crew_status_panel_can_interact(&self)->bool {
        self.singleplayer_crew_active && self.mode==Mode::Tank && self.controlled_soldier_alive()
            && self.singleplayer_embodied_role().and_then(|r|self.singleplayer_actor_id(r))
                .and_then(|a|self.singleplayer_actor_casualty(a)).is_some_and(|c|c.able())
            && !self.commander_outside && !self.gunner_optic && !self.climb.active
            && !self.map_held && !self.repair_manual.held && self.held_item.is_none()
            && !self.ext_equipped && !self.carrying_track_kit && !self.personal_hand_item_active()
    }

    /// Use this same result for both hover/click and selection-wire destination. The segment
    /// ends just before the actual lens, so its supporting wall cannot block its own control.
    pub(super) fn crew_status_panel_hover_index(&self,eye:Vec3,dir:Vec3)->Option<usize> {
        if !self.crew_status_panel_can_interact() || !eye.is_finite() || !dir.is_finite() {return None;}
        let direction=dir.normalize_or_zero();
        let (index,distance)=self.crew_status_panel.pick(self.player.turret_model(),eye,direction,REACH)?;
        let end=eye+direction*(distance-0.0005).max(0.0);
        let models=self.fire_surface_models();
        if self.cabin_fire.source_contacts(&models).sweep_batch(&[[eye,end]])[0].is_some() {return None;}
        // Retained anatomical source triangles also obstruct the hands; never click through a corpse.
        if self.crew_corpse_render.borrow().as_ref().and_then(|render|
            render.ray(&self.player,&self.couch_ragdolls,eye,direction,(distance-0.001).max(0.0),|actor|self.singleplayer_actor_source_identity(actor))).is_some() {return None;}
        Some(index)
    }

    pub fn crew_status_panel_hover(&self,eye:Vec3,dir:Vec3)->Option<String> {
        if !self.crew_status_panel_can_interact() {return None;}
        let Some(index)=self.crew_status_panel_hover_index(eye,dir) else {return self.crew_panel_notice.clone();};
        let states=self.crew_status_panel_states();let state=&states[index];
        let description=if state.actor_id.is_none() {format!("{} — empty",SEATS[index].name())}
            else {format!("{} — {} · trained {}{}",SEATS[index].name(),state.name,state.trained_role,
                if state.moving {" · moving"} else {""})};
        let action=if self.crew_panel_selected.is_some() {"LMB assign here · X cancel"}
            else if matches!(state.condition,Condition::Healthy|Condition::Wounded) && !state.moving {"LMB select"}
            else if state.condition==Condition::Dead {"Clear the body before assigning this seat"}
            else {"Select a living crewman first"};
        Some(match &self.crew_panel_notice {Some(note)=>format!("{description}\n{note}\n{action}"),None=>format!("{description}\n{action}")})
    }

    pub fn crew_status_panel_click(&mut self,eye:Vec3,dir:Vec3)->bool {
        let Some(index)=self.crew_status_panel_hover_index(eye,dir) else {return false;};
        self.consume_primary_press();
        let states=self.crew_status_panel_states();let state=&states[index];
        if let Some(selected)=self.crew_panel_selected {
            if state.actor_id==Some(selected) {
                self.crew_panel_selected=None;self.crew_panel_notice=None;return true;
            }
            if self.singleplayer_actor_casualty(selected).is_none_or(|c|!c.able()) {
                self.crew_panel_selected=None;
                self.crew_panel_notice=Some("The selected crewman can no longer move to another seat. X dismiss".into());
                return true;
            }
            match self.begin_crew_assignment_actor(selected,SEATS[index]) {
                Ok(())=>{self.crew_panel_selected=None;self.crew_panel_notice=Some(format!("Moving to {}. X dismiss",SEATS[index].name()));}
                Err(error)=>self.crew_panel_notice=Some(format!("Cannot assign: {error}. X cancel")),
            }
        } else if let Some(actor)=state.actor_id {
            if state.condition==Condition::Dead {
                self.crew_panel_notice=Some("This crewman is dead; clear the body to free the seat. X dismiss".into());
            } else if state.moving {
                self.crew_panel_notice=Some("This crewman is already moving between seats. X dismiss".into());
            } else if self.singleplayer_actor_casualty(actor).is_none_or(|c|!c.able()) {
                self.crew_panel_notice=Some("This crewman cannot move without assistance. X dismiss".into());
            } else {
                self.crew_panel_selected=Some(actor);self.crew_panel_notice=None;
            }
        } else {self.crew_panel_notice=Some("Select a living crewman's lamp, then choose the destination. X dismiss".into());}
        true
    }

    /// X consumes only an active assignment selection/notice; otherwise the ordinary station
    /// exit behavior keeps its existing priority and semantics.
    pub fn cancel_crew_panel_selection(&mut self)->bool {
        let selected=self.crew_panel_selected.take().is_some();
        let notice=self.crew_panel_notice.take().is_some(); selected||notice
    }

    pub fn crew_status_panel_receipt(&self)->serde_json::Value {
        let states=self.crew_status_panel_states();
        let selected=self.crew_panel_selected.and_then(|actor|states.iter().position(|s|s.actor_id==Some(actor)));
        let power=if self.cabin_lights {1.0}else{0.0};
        let mut value=self.crew_status_panel.receipt(&states,self.crew_solo.panel_clock_s(),power,selected,None);
        value["selected_actor"]=serde_json::json!(self.crew_panel_selected);
        value["notice"]=serde_json::json!(self.crew_panel_notice);
        value["selection_source"]=serde_json::json!("current physical seat occupancy; stable actor ID");
        value
    }
    pub fn crew_status_panel_inspection_pose(&self)->(Vec3,Vec3) {
        self.crew_status_panel.inspection_pose(self.player.turret_model())
    }
}
