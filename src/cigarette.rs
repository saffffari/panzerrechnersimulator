//! Finite hand-item state, independent of rendering. One authored starting packet has20 cigarettes.
//! Burn duration and puff timing are gameplay estimates; no healing/stamina effect is applied.
const PACK_SIZE:u16=20;
const MAX_CIGARETTES:u16=400;
const BURN_SECONDS:f64=240.0;
const INHALE_SECONDS:f64=0.85;
const EXHALE_SECONDS:f64=1.65;
const PUFF_SECONDS:f64=INHALE_SECONDS+EXHALE_SECONDS;

#[derive(Clone,Copy,Debug,Default,PartialEq)]
pub struct View {
    pub pack_open:bool,pub equipped:bool,pub pack_count:u16,pub reserve_count:u16,
    pub cigarette_present:bool,pub lit:bool,pub burn_remaining:f32,
    pub lighter_raise:f32,pub lighter_flame:bool,pub puff_phase:f32,pub exhale:f32,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Action {#[default] None,Draw,Light,Puff,BurnedOut}
#[derive(Clone,Debug)]
pub struct State {
    pack:u16,reserve:u16,held_burn_s:Option<f64>,lit:bool,equipped:bool,pack_open:bool,
    lighter_raise:f64,lighter_held:bool,puff_s:f64,pack_reveal_s:f64,previous_primary:bool,wait_for_release:bool,
    consumed:u32,drawn:u32,lit_count:u32,puffs:u32,
}
impl Default for State {fn default()->Self {Self{pack:PACK_SIZE,reserve:0,held_burn_s:None,
    lit:false,equipped:false,pack_open:false,lighter_raise:0.0,lighter_held:false,puff_s:0.0,pack_reveal_s:0.0,
    previous_primary:false,wait_for_release:false,consumed:0,drawn:0,lit_count:0,puffs:0}}}
impl State {
    pub fn active(&self)->bool {self.equipped}
    pub fn view(&self)->View {
        let elapsed=(PUFF_SECONDS-self.puff_s).max(0.0);
        let inhaling=self.puff_s>EXHALE_SECONDS;
        let exhale=if self.puff_s>0.0&&!inhaling {
            let p=((elapsed-INHALE_SECONDS)/EXHALE_SECONDS).clamp(0.0,1.0);
            (std::f64::consts::PI*p).sin() as f32
        }else{0.0};
        View{pack_open:self.pack_open,equipped:self.equipped,pack_count:self.pack,reserve_count:self.reserve,
            cigarette_present:self.held_burn_s.is_some(),lit:self.lit,
            burn_remaining:self.held_burn_s.map_or(0.0,|v|(v/BURN_SECONDS) as f32),
            lighter_raise:self.lighter_raise as f32,
            lighter_flame:self.equipped&&!self.pack_open&&self.lighter_held&&self.lighter_raise>=0.70,
            puff_phase:if inhaling {(std::f64::consts::PI*elapsed/INHALE_SECONDS).sin() as f32}else{0.0},exhale}
    }
    /// C draws exactly one cigarette, briefly revealing the packet. With a held cigarette C
    /// instead toggles inspection; it never discards one or conjures a new cigarette.
    pub fn draw_or_inspect(&mut self)->Action {
        if self.equipped&&self.held_burn_s.is_some() {
            self.pack_open=!self.pack_open;self.pack_reveal_s=0.0;self.lighter_held=false;
            self.wait_for_release=true;return Action::None;
        }
        if self.pack==0 {let transfer=self.reserve.min(PACK_SIZE);self.pack=transfer;self.reserve-=transfer;}
        self.equipped=true;self.pack_open=true;self.pack_reveal_s=0.85;
        self.lighter_held=false;self.puff_s=0.0;self.wait_for_release=true;
        if self.held_burn_s.is_none()&&self.pack>0 {
            self.pack-=1;self.held_burn_s=Some(BURN_SECONDS);self.drawn=self.drawn.saturating_add(1);Action::Draw
        }else{Action::None}
    }
    /// A lit end is extinguished before pocketing. The remaining physical cigarette is retained.
    pub fn stow(&mut self)->bool {
        let active=self.equipped;self.equipped=false;self.pack_open=false;self.lit=false;
        self.lighter_held=false;self.puff_s=0.0;self.pack_reveal_s=0.0;self.wait_for_release|=self.previous_primary;active
    }
    pub fn accept_packet(&mut self)->bool {
        if self.total_unconsumed()+u32::from(PACK_SIZE)>u32::from(MAX_CIGARETTES) {return false;}
        self.reserve+=PACK_SIZE;true
    }
    pub fn total_unconsumed(&self)->u32 {u32::from(self.pack+self.reserve)+u32::from(self.held_burn_s.is_some())}
    pub fn consumed(&self)->u32 {self.consumed}
    pub fn drawn(&self)->u32 {self.drawn}
    pub fn lit_count(&self)->u32 {self.lit_count}
    pub fn puffs(&self)->u32 {self.puffs}
    /// Called once per actual simulation step. RMB alone never lights the cigarette.
    pub fn advance(&mut self,dt:f32,primary:bool,lighter:bool)->Action {
        if !dt.is_finite()||dt<0.0 {return Action::None;}
        let dt=f64::from(dt);
        if self.pack_reveal_s>0.0 {
            self.pack_reveal_s=(self.pack_reveal_s-dt).max(0.0);
            if self.pack_reveal_s==0.0 {self.pack_open=false;}
        }
        if !primary {self.wait_for_release=false;}
        let pressed=primary&&!self.previous_primary&&!self.wait_for_release;
        self.previous_primary=primary;
        self.lighter_held=self.equipped&&!self.pack_open&&lighter;
        let target=if self.lighter_held {1.0}else{0.0};
        self.lighter_raise+=(target-self.lighter_raise).clamp(-dt*7.0,dt*5.0);
        let active_inhale=dt.min((self.puff_s-EXHALE_SECONDS).max(0.0));
        self.puff_s=(self.puff_s-dt).max(0.0);
        let mut action=Action::None;
        if self.lit {if let Some(burn)=self.held_burn_s.as_mut() {
            *burn-=dt+active_inhale*5.0;
            if *burn<=0.0 {self.held_burn_s=None;self.lit=false;self.puff_s=0.0;
                self.consumed=self.consumed.saturating_add(1);action=Action::BurnedOut;}
        }}
        if !self.equipped||!pressed {return action;}
        if self.pack_open {return action;}
        if self.held_burn_s.is_some() {
            if lighter&&!self.lit&&self.lighter_raise>=0.70 {
                self.lit=true;self.lit_count=self.lit_count.saturating_add(1);action=Action::Light;
            }else if !lighter&&self.lit&&self.puff_s<=0.0 {
                self.puff_s=PUFF_SECONDS;self.puffs=self.puffs.saturating_add(1);action=Action::Puff;
            }
        }
        action
    }
}
#[cfg(test)]mod tests {
    use super::*;
    fn draw()->State {let mut s=State::default();assert_eq!(s.draw_or_inspect(),Action::Draw);
        s.advance(0.9,false,false);s}
    fn light()->State {let mut s=draw();s.advance(0.2,false,true);
        assert_eq!(s.advance(0.01,true,true),Action::Light);s.advance(0.01,false,false);s}
    #[test]fn finite_draw_conserves_pack_and_never_repeats_held_click(){let mut s=draw();
        assert_eq!(s.view().pack_count,19);assert_eq!(s.total_unconsumed(),20);
        for _ in 0..100 {s.advance(0.01,true,false);}assert_eq!(s.drawn(),1);assert_eq!(s.total_unconsumed(),20);}
    #[test]fn lighter_alone_never_lights_and_puff_requires_lit_cigarette(){let mut s=draw();
        s.advance(1.0,false,true);assert!(s.view().lighter_flame);assert!(!s.view().lit);
        assert_eq!(s.advance(0.01,true,false),Action::None);s.advance(0.01,false,true);
        s.advance(0.2,false,true);assert_eq!(s.advance(0.01,true,true),Action::Light);
        s.advance(0.1,false,false);assert_eq!(s.advance(0.01,true,false),Action::Puff);
        assert_eq!(s.advance(0.01,true,false),Action::None);s.advance(1.0,false,false);assert!(s.view().exhale>0.0);}
    #[test]fn stow_extinguishes_without_destroying_or_restoring_a_cigarette(){let mut s=light();
        s.advance(8.0,false,false);let left=s.view().burn_remaining;assert!(s.stow());
        s.advance(30.0,false,false);assert!(!s.view().lit);assert_eq!(s.view().burn_remaining,left);
        s.draw_or_inspect();s.advance(0.9,false,false);
        assert_eq!(s.drawn(),1);assert_eq!(s.total_unconsumed(),20);}
    #[test]fn burn_consumes_all_elapsed_time_at_30_60_144_hz_and_hitches(){let mut result=Vec::new();
        for hz in [30,60,144] {let mut s=light();s.advance(0.01,true,false);
            for _ in 0..hz*120 {s.advance(1.0/hz as f32,false,false);}result.push(s.view().burn_remaining);}
        assert!((result[0]-result[2]).abs()<1e-5);let mut h=light();h.advance(0.01,true,false);
        h.advance(120.0,false,false);assert!((h.view().burn_remaining-result[0]).abs()<1e-5);
        h.advance(400.0,false,false);assert!(!h.view().cigarette_present);assert_eq!(h.consumed(),1);
        assert_eq!(h.total_unconsumed()+h.consumed(),20);}
    #[test]fn pickup_capacity_never_silently_loses_a_packet(){let mut s=State::default();
        for _ in 0..19 {assert!(s.accept_packet());}assert!(!s.accept_packet());assert_eq!(s.total_unconsumed(),400);}
    #[test]fn pickup_capacity_counts_the_cigarette_already_in_hand(){let mut s=State::default();
        s.draw_or_inspect();s.pack=0;s.reserve=380;
        assert_eq!(s.total_unconsumed(),381);assert!(!s.accept_packet());
        assert_eq!(s.total_unconsumed(),381);}
    #[test]fn opening_pack_while_trigger_held_waits_for_release(){let mut s=State::default();
        s.draw_or_inspect();s.advance(0.3,true,false);assert_eq!(s.drawn(),1);
        s.advance(0.1,false,false);s.advance(0.1,true,false);assert_eq!(s.drawn(),1);}
}
