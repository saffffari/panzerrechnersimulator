//! Finite hand-item state, independent of rendering. One authored starting packet has20 cigarettes.
//! Burn duration and toke timing are gameplay estimates. A toke is held: the tip brightens while
//! the trigger is down, the release exhales a cloud sized by how long it was held, and holding
//! too long ends in a cough. Each inhaled second is a dose the game turns into recovery and a
//! short-lived steadier, sharper eye (see `Game::route_personal_hand_input`).
const PACK_SIZE:u16=20;
const MAX_CIGARETTES:u16=400;
const BURN_SECONDS:f64=240.0;
const INHALE_SECONDS:f64=0.85;
const EXHALE_SECONDS:f64=1.65;
const PUFF_SECONDS:f64=INHALE_SECONDS+EXHALE_SECONDS;
/// A toke held this long ends in a cough; the lungs give out entirely at the maximum.
const COUGH_TOKE_SECONDS:f64=4.5;
const MAX_TOKE_SECONDS:f64=6.0;
const COUGH_SECONDS:f64=2.2;
/// Extra burn while inhaling, as a multiple of the idle rate.
const INHALE_BURN_MULTIPLE:f64=5.0;
/// Seconds of inhaling one whole cigarette allows (idle burn ignored).
pub const TOKE_SECONDS_PER_CIGARETTE:f32=(BURN_SECONDS/(1.0+INHALE_BURN_MULTIPLE)) as f32;
/// The eye's boost rises with each inhaled second and fades over this long after the last toke.
const BOOST_PER_TOKE_SECOND:f64=0.30;
const BOOST_FADE_SECONDS:f64=50.0;

#[derive(Clone,Copy,Debug,Default,PartialEq)]
pub struct View {
    pub pack_open:bool,pub equipped:bool,pub pack_count:u16,pub reserve_count:u16,
    pub cigarette_present:bool,pub lit:bool,pub burn_remaining:f32,
    pub lighter_raise:f32,pub lighter_flame:bool,pub puff_phase:f32,pub exhale:f32,
    /// Seconds the current toke has been held (0 when not inhaling).
    pub toke_seconds:f32,
    pub coughing:bool,
    /// 0..1 steadier, sharper eye from recent tokes.
    pub vision_boost:f32,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Action {#[default] None,Draw,Light,Puff,Cough,BurnedOut}
#[derive(Clone,Debug)]
pub struct State {
    pack:u16,reserve:u16,held_burn_s:Option<f64>,lit:bool,equipped:bool,pack_open:bool,
    lighter_raise:f64,lighter_held:bool,puff_s:f64,pack_reveal_s:f64,previous_primary:bool,wait_for_release:bool,
    consumed:u32,drawn:u32,lit_count:u32,puffs:u32,
    toke_s:f64,exhale_s:f64,exhale_total:f64,last_toke_s:f64,cough_s:f64,dose_s:f64,boost:f64,
}
impl Default for State {fn default()->Self {Self{pack:PACK_SIZE,reserve:0,held_burn_s:None,
    lit:false,equipped:false,pack_open:false,lighter_raise:0.0,lighter_held:false,puff_s:0.0,pack_reveal_s:0.0,
    previous_primary:false,wait_for_release:false,consumed:0,drawn:0,lit_count:0,puffs:0,
    toke_s:0.0,exhale_s:0.0,exhale_total:0.0,last_toke_s:0.0,cough_s:0.0,dose_s:0.0,boost:0.0}}}
impl State {
    pub fn active(&self)->bool {self.equipped}
    pub fn view(&self)->View {
        let _=(PUFF_SECONDS,INHALE_SECONDS,EXHALE_SECONDS);
        // The cloud swells and thins over the exhale, scaled by how deep the toke was.
        let exhale=if self.exhale_s>0.0&&self.exhale_total>0.0 {
            let p=(1.0-self.exhale_s/self.exhale_total).clamp(0.0,1.0);
            let depth=(self.last_toke_s/COUGH_TOKE_SECONDS).clamp(0.2,1.0);
            ((std::f64::consts::PI*p).sin()*depth) as f32
        }else{0.0};
        View{pack_open:self.pack_open,equipped:self.equipped,pack_count:self.pack,reserve_count:self.reserve,
            cigarette_present:self.held_burn_s.is_some(),lit:self.lit,
            burn_remaining:self.held_burn_s.map_or(0.0,|v|(v/BURN_SECONDS) as f32),
            lighter_raise:self.lighter_raise as f32,
            lighter_flame:self.equipped&&!self.pack_open&&self.lighter_held&&self.lighter_raise>=0.70,
            // The tip brightens quickly as the draw begins and stays bright while it is held.
            puff_phase:(self.toke_s/0.45).clamp(0.0,1.0) as f32,exhale,
            toke_seconds:self.toke_s as f32,coughing:self.cough_s>0.0,vision_boost:self.boost as f32}
    }
    /// C draws exactly one cigarette, briefly revealing the packet. With a held cigarette C
    /// instead toggles inspection; it never discards one or conjures a new cigarette.
    pub fn draw_or_inspect(&mut self)->Action {
        if self.equipped&&self.held_burn_s.is_some() {
            self.pack_open=!self.pack_open;self.pack_reveal_s=0.0;self.lighter_held=false;
            self.wait_for_release=true;return Action::None;
        }
        if self.pack==0 {let transfer=self.reserve.min(PACK_SIZE);self.pack=transfer;self.reserve-=transfer;}
        // Nothing left to hold: empty hands stay empty rather than occupied by an absent item.
        if self.held_burn_s.is_none()&&self.pack==0 {return Action::None;}
        self.equipped=true;self.pack_open=true;self.pack_reveal_s=0.85;
        self.lighter_held=false;self.puff_s=0.0;self.wait_for_release=true;
        if self.held_burn_s.is_none()&&self.pack>0 {
            self.pack-=1;self.held_burn_s=Some(BURN_SECONDS);self.drawn=self.drawn.saturating_add(1);Action::Draw
        }else{Action::None}
    }
    /// A lit end is extinguished before pocketing. The remaining physical cigarette is retained.
    pub fn stow(&mut self)->bool {
        let active=self.equipped;self.equipped=false;self.pack_open=false;self.lit=false;
        self.lighter_held=false;self.puff_s=0.0;self.pack_reveal_s=0.0;self.wait_for_release|=self.previous_primary;
        self.toke_s=0.0;active
    }
    /// Inhaled seconds since the last call. The game turns them into recovery.
    pub fn take_dose(&mut self)->f32 {std::mem::take(&mut self.dose_s) as f32}
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
        self.exhale_s=(self.exhale_s-dt).max(0.0);
        self.cough_s=(self.cough_s-dt).max(0.0);
        // A toke is the trigger held on a lit cigarette with the lighter down. It cannot start
        // mid-exhale or mid-cough, and it must begin with a fresh press.
        let was_toking=self.toke_s>0.0;
        let able=self.equipped&&!self.pack_open&&self.lit&&self.held_burn_s.is_some()&&!lighter;
        let toking=able&&primary&&!self.wait_for_release&&self.toke_s<MAX_TOKE_SECONDS
            &&(was_toking||(pressed&&self.exhale_s<=0.0&&self.cough_s<=0.0));
        let active_inhale=if toking {dt.min(MAX_TOKE_SECONDS-self.toke_s)}else{0.0};
        let mut action=Action::None;
        if toking {
            self.toke_s+=active_inhale;self.dose_s+=active_inhale;
            self.boost=(self.boost+active_inhale*BOOST_PER_TOKE_SECOND).min(1.0);
        }else{
            self.boost=(self.boost-dt/BOOST_FADE_SECONDS).max(0.0);
            if was_toking {
                // Release: the breath goes out, and a draw held too long comes back as a cough.
                self.puffs=self.puffs.saturating_add(1);self.last_toke_s=self.toke_s;
                self.exhale_total=(0.7+self.toke_s*0.8).min(4.0);self.exhale_s=self.exhale_total;
                action=if self.toke_s>=COUGH_TOKE_SECONDS {self.cough_s=COUGH_SECONDS;Action::Cough}else{Action::Puff};
                self.toke_s=0.0;
            }
        }
        if self.lit {if let Some(burn)=self.held_burn_s.as_mut() {
            *burn-=dt+active_inhale*INHALE_BURN_MULTIPLE;
            if *burn<=0.0 {self.held_burn_s=None;self.lit=false;self.puff_s=0.0;self.toke_s=0.0;
                // The hand is physically empty again; it must not keep owning the controls.
                self.equipped=false;self.pack_open=false;self.pack_reveal_s=0.0;self.lighter_held=false;
                self.consumed=self.consumed.saturating_add(1);action=Action::BurnedOut;}
        }}
        if !self.equipped||!pressed {return action;}
        if self.pack_open {return action;}
        if self.held_burn_s.is_some() {
            if lighter&&!self.lit&&self.lighter_raise>=0.70 {
                self.lit=true;self.lit_count=self.lit_count.saturating_add(1);action=Action::Light;
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
        // A toke is held; the exhale and the count come with the release.
        s.advance(0.1,false,false);assert_eq!(s.advance(0.01,true,false),Action::None);
        assert_eq!(s.advance(1.0,true,false),Action::None);assert!(s.view().puff_phase>0.99);
        assert_eq!(s.advance(0.01,false,false),Action::Puff);assert_eq!(s.puffs(),1);
        s.advance(0.5,false,false);assert!(s.view().exhale>0.0);}
    #[test]fn exhale_grows_with_the_toke_and_a_long_hold_ends_in_a_cough(){
        let peak=|hold:f32| {let mut s=light();s.advance(0.01,true,false);s.advance(hold,true,false);
            let action=s.advance(0.01,false,false);let mut top=0.0f32;
            for _ in 0..400 {s.advance(0.01,false,false);top=top.max(s.view().exhale);}(action,top,s)};
        let (short_action,short,_)=peak(0.6);let (long_action,long,_)=peak(3.5);
        assert_eq!(short_action,Action::Puff);assert_eq!(long_action,Action::Puff);
        assert!(long>short*1.8,"deep toke {long} shallow {short}");
        let (cough,_,after)=peak(5.0);assert_eq!(cough,Action::Cough);
        let mut held=light();held.advance(0.01,true,false);
        // Held without limit: the lungs stop at the maximum and the release still coughs.
        let mut forced=None;let mut longest=0.0f32;
        for step in 0..2000 {longest=longest.max(held.view().toke_seconds);
            if held.advance(0.01,true,false)==Action::Cough {forced=Some(step);break;}}
        assert!(forced.is_some(),"the lungs must give out on their own");
        assert!(longest<=MAX_TOKE_SECONDS as f32+1e-3);assert!(held.view().coughing);
        // Still holding the trigger through the cough starts nothing; nor does a fresh press.
        assert_eq!(held.advance(0.5,true,false),Action::None);assert_eq!(held.view().toke_seconds,0.0);
        held.advance(0.01,false,false);
        assert_eq!(held.advance(0.01,true,false),Action::None);assert_eq!(held.view().toke_seconds,0.0);
        let _=after;}
    #[test]fn one_cigarette_carries_a_fixed_dose_and_the_boost_fades(){let mut s=light();let mut dose=0.0;
        for _ in 0..200 {
            s.advance(0.01,true,false);for _ in 0..30 {s.advance(0.1,true,false);}
            s.advance(0.01,false,false);for _ in 0..50 {s.advance(0.1,false,false);}
            dose+=s.take_dose();if !s.view().cigarette_present {break;}
        }
        assert!(!s.view().cigarette_present);
        assert!(dose>TOKE_SECONDS_PER_CIGARETTE*0.25&&dose<=TOKE_SECONDS_PER_CIGARETTE+0.1,"dose {dose}");
        let mut b=light();b.advance(0.01,true,false);b.advance(3.0,true,false);b.advance(0.01,false,false);
        let high=b.view().vision_boost;assert!(high>0.8);
        b.advance(60.0,false,false);assert_eq!(b.view().vision_boost,0.0);}
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
    #[test]fn burnout_and_an_empty_supply_leave_the_hands_free(){let mut s=light();
        assert!(s.active());assert_eq!(s.advance(400.0,false,false),Action::BurnedOut);
        assert!(!s.active(),"a consumed cigarette must not keep the hands occupied");
        assert!(!s.view().cigarette_present&&!s.view().pack_open&&!s.view().lighter_flame);
        let mut empty=State::default();empty.pack=0;empty.reserve=0;
        assert_eq!(empty.draw_or_inspect(),Action::None);
        assert!(!empty.active(),"drawing from an empty supply cannot equip nothing");
        assert_eq!(empty.total_unconsumed(),0);
        // A reserve packet still refills the pack and draws one.
        empty.reserve=PACK_SIZE;assert_eq!(empty.draw_or_inspect(),Action::Draw);
        assert!(empty.active());assert_eq!(empty.total_unconsumed(),u32::from(PACK_SIZE));}
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
