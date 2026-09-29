//! A short fictional winter field exercise. Progress observes ordinary world/inventory events;
//! it neither spawns enemies nor tells them where the player is.
#[cfg(test)]
#[path = "winter_route_scene_tests.rs"]
mod winter_route_scene_tests;
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use crate::ui::Align;

pub(super) const HOUSE: Vec2 = Vec2::new(1880.0, 165.0);
pub(super) const WRECK: Vec2 = Vec2::new(1907.0, 218.0);
pub(super) const WITHDRAW: Vec2 = Vec2::new(1870.0, 340.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage { Explore, Salvage, Engage, Reload, Withdraw, Complete }

pub(super) struct Mission {
    stage: Stage,
    elapsed: f32,
    house_dwell: f32,
    source_at: BTreeMap<u32, u32>,
    delivered: BTreeSet<u32>,
    shot_at_stow: u32,
    first_shot: Option<u32>,
    reloaded: bool,
    empty_cradles: Vec<usize>,
}

impl Mission {
    fn new(sources: impl Iterator<Item=u32>, empty_cradles: Vec<usize>, shot: u32) -> Self {
        Self { stage: Stage::Explore, elapsed: 0.0, house_dwell: 0.0,
            source_at: sources.map(|id| (id,id)).collect(), delivered: BTreeSet::new(),
            shot_at_stow: shot, first_shot: None, reloaded: false, empty_cradles }
    }
    /// Follow the cartridge through hand/floor/rack/chamber locations, including set-downs.
    pub(super) fn transfer(&mut self, from: u32, to: u32) {
        if from == to { return; }
        let source = self.source_at.remove(&from);
        self.source_at.remove(&to);
        if let Some(source) = source { self.source_at.insert(to, source); }
    }
    pub(super) fn stowed(&mut self, from: u32, shot: u32) {
        if let Some(&source) = self.source_at.get(&from) {
            if self.delivered.len() < 2 && self.delivered.insert(source) {
                self.shot_at_stow = shot;
            }
        }
    }
    fn observe(&mut self, dt: f32, foot: Option<Vec3>, tank: Vec3, shot: u32, loaded: bool) {
        self.elapsed += dt.max(0.0);
        if let Some(p) = foot {
            if (p.x-HOUSE.x).abs() < 4.6 && (p.z-HOUSE.y).abs() < 4.6 {
                self.house_dwell += dt.max(0.0);
            }
        }
        if self.stage == Stage::Explore && self.house_dwell >= 4.0 { self.stage = Stage::Salvage; }
        if self.stage == Stage::Salvage && self.delivered.len() >= 2 { self.stage = Stage::Engage; }
        if self.stage == Stage::Engage && shot != self.shot_at_stow {
            self.first_shot = Some(shot); self.stage = Stage::Reload;
        }
        if self.stage == Stage::Reload {
            self.reloaded |= loaded;
            if self.reloaded && self.first_shot.is_some_and(|first| shot != first) {
                self.stage = Stage::Withdraw;
            }
        }
        if self.stage == Stage::Withdraw && foot.is_none()
            && Vec2::new(tank.x,tank.z).distance(WITHDRAW) < 16.0 { self.stage = Stage::Complete; }
    }
    fn order(&self) -> String {
        match self.stage {
            Stage::Explore => "1  SEARCH THE HOUSE TO THE NORTH".into(),
            Stage::Salvage => format!("2  SALVAGE / STOW TWO AP ROUNDS  {}/2", self.delivered.len().min(2)),
            Stage::Engage => "3  OBSERVE NORTH / ENGAGE FROM GUNNER".into(),
            Stage::Reload => if self.reloaded { "4  FIRE A SECOND ROUND FROM GUNNER" } else {
                "4  CLEAR CASE / LOAD / FLICK TO RAM / CLOSE" }.into(),
            Stage::Withdraw => "5  WITHDRAW SOUTH IN THE TIGER".into(),
            Stage::Complete => "FIELD ORDER COMPLETE / FREE EXPLORATION".into(),
        }
    }
}

impl Game {
    pub(super) fn begin_winter_mission(&mut self) {
        if self.winter_mission.is_some() { return; }
        // Two actual empty AP cradles on the loader side. Pick by physical reach/clear ray,
        // exactly as placement does, rather than making the rack accept excess ammunition.
        let saved = (self.walk_pos, self.loader_crouch_t);
        let mut empty = Vec::new();
        for (walk, crouch) in [(Vec3::new(0.85,0.0,0.08),0.0),
            (Vec3::new(0.85,0.0,0.08),1.0), (Vec3::new(0.70,0.0,-0.55),0.0),
            (Vec3::new(0.70,0.0,-0.55),1.0), (Vec3::new(-0.70,0.0,-0.55),0.0),
            (Vec3::new(-0.70,0.0,-0.55),1.0)] {
            self.walk_pos = walk; self.loader_crouch_t = crouch;
            let eye = self.station_eye(Station::Loader);
            for (i,&(frame,p,q)) in crate::interior_rig::SHELL_SLOTS.iter().enumerate() {
                if empty.len() >= 2 { break; }
                if self.slot_nature_he[i] || self.slot_taken[i] { continue; }
                let parent = if frame == crate::interior_rig::Frame::Hull {
                    self.player.hull_model() } else { self.player.turret_model() };
                let point = (parent * Mat4::from_rotation_translation(Quat::from_array(q),p)).w_axis.truncate();
                self.slot_taken[i] = true;
                if self.empty_rack_under_aim(eye,(point-eye).normalize_or_zero(),false) == Some(i) {
                    empty.push(i);
                    self.cartridge_primers.consume(i as u32);
                } else { self.slot_taken[i] = false; }
            }
        }
        (self.walk_pos, self.loader_crouch_t) = saved;
        let sources = self.loose.iter().enumerate().filter(|(_,s)| s.active && s.world_space
            && !s.nature_he && Vec2::new(s.pos.x,s.pos.z).distance(WRECK) < 12.0)
            .map(|(i,_)| 0x2_0000 + i as u32);
        self.winter_mission = Some(Mission::new(sources,empty,self.rounds_spent));
        println!("[winter-mission] {:?}", self.winter_mission_snapshot());
    }
    pub(super) fn update_winter_mission(&mut self, dt: f32) {
        if !self.player.alive { return; }
        let foot = (self.mode == Mode::Foot).then_some(self.foot.pos);
        if let Some(m) = &mut self.winter_mission {
            let old = m.stage;
            m.observe(dt,foot,self.player.pos,self.rounds_spent,matches!(self.breech,Breech::Loaded));
            if m.stage != old { println!("[winter-mission] {}",m.order()); }
        }
    }
    pub fn winter_mission_snapshot(&self) -> Option<serde_json::Value> {
        self.winter_mission.as_ref().map(|m| serde_json::json!({
            "stage":format!("{:?}",m.stage),"order":m.order(),"elapsed_s":m.elapsed,
            "house_dwell_s":m.house_dwell,"distinct_stowed_rounds":m.delivered.len(),
            "empty_start_cradles":m.empty_cradles,"eligible_rounds":m.source_at.len(),
            "reloaded_after_first_shot":m.reloaded,"complete":m.stage == Stage::Complete,
            "scope":"Fictional winter courtyard field exercise; no historical battle reconstruction claim."
        }))
    }
    /// A local sketch fills the existing physical paper UV region. It reveals no enemy positions
    /// and carries no words (2026-09-28): four crosses (the start, the house, the wreck, the
    /// withdrawal point), the house's walls, a north caret and a 50 m bar.
    pub fn draw_winter_map(&self, r: &mut Renderer, pencil: &[(u8,Vec2)], pins: &[(u8,Vec2)],
        cursor: Vec2) -> bool {
        if self.winter_mission.is_none() { return false; }
        let ink=[0.17,0.16,0.13,1.0]; let faded=[0.38,0.34,0.28,1.0];
        r.fill_rect(0.17,0.34,0.66,0.59,[0.85,0.81,0.68,1.0]);
        r.draw_text("^",0.78,0.445,0.025,ink,Align::Center);
        let project=|p: Vec2| (0.49+(p.x-1880.0)/240.0*0.51,0.64+(p.y-235.0)/240.0*0.40);
        for point in [HOUSE,WRECK,Vec2::new(1850.0,250.0),WITHDRAW] {
            let(x,y)=project(point);
            r.draw_text_bold("+",x,y,0.028,ink,Align::Center);
        }
        let(x,y)=project(HOUSE);
        r.fill_rect(x-0.014,y-0.01,0.028,0.004,ink);
        r.fill_rect(x-0.014,y+0.006,0.028,0.004,ink);
        r.fill_rect(0.205,0.823,50.0/240.0*0.51,0.003,ink);
        for &(_,p) in pencil.iter().filter(|(act,_)| *act == self.act.id) {
            r.fill_rect(0.17+p.x*0.66,0.34+p.y*0.59,0.002,0.002,ink);
        }
        for &(_,p) in pins.iter().filter(|(act,_)| *act == self.act.id) {
            r.draw_text("x",0.17+p.x*0.66,0.34+p.y*0.59,0.023,[0.58,0.08,0.065,1.0],Align::Center);
        }
        r.fill_rect(0.17+cursor.x*0.66,0.34+cursor.y*0.59,0.002,0.003,faded);
        true
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn recovery_tracks_identity_through_set_downs_and_rejects_repeated_stow() {
        let mut m=Mission::new([0x20001,0x20002].into_iter(),vec![0,1],0);
        m.transfer(0x20001,0x20003); m.stowed(0x20003,0); m.transfer(0x20003,3);
        m.transfer(3,0x20004); m.stowed(0x20004,0);
        m.stowed(77,0); assert_eq!(m.delivered.len(),1);
        m.stowed(0x20002,0); assert_eq!(m.delivered.len(),2);
    }
    #[test] fn mission_requires_house_two_rounds_reload_second_shot_and_tank_withdrawal() {
        let mut m=Mission::new([1,2].into_iter(),vec![0,1],0);
        let house=Vec3::new(HOUSE.x,6.0,HOUSE.y); let end=Vec3::new(WITHDRAW.x,6.0,WITHDRAW.y);
        m.observe(10.0,None,end,5,true); assert_eq!(m.stage,Stage::Explore);
        m.observe(4.1,Some(house),end,5,true); assert_eq!(m.stage,Stage::Salvage);
        m.stowed(1,5); m.stowed(2,5); m.observe(0.1,None,Vec3::ZERO,5,true);
        assert_eq!(m.stage,Stage::Engage);
        m.observe(0.1,None,Vec3::ZERO,6,false); assert_eq!(m.stage,Stage::Reload);
        m.observe(0.1,None,Vec3::ZERO,6,true); m.observe(0.1,Some(end),end,7,false);
        assert_eq!(m.stage,Stage::Withdraw);
        m.observe(0.1,None,end,7,false); assert_eq!(m.stage,Stage::Complete);
    }
}
