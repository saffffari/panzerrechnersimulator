//! Finite hand-item state, independent of rendering. One authored starting packet has 20
//! cigarettes, each a separate slot that is taken by hand; one of them sits inverted. Burn
//! duration and toke timing are gameplay estimates. C brings out the packet; a cigarette taken
//! from it is lit and smoked on the right mouse button, so the left button and interactions stay
//! free. A toke is held: the tip brightens, the release exhales a cloud sized by how long it was
//! held, and holding too long ends in a cough. Each toke sharpens the eye for a while, and those
//! periods stack. A lit cigarette put away goes to the mouth and keeps burning; at the filter the
//! butt is thrown away.
pub const PACK_SIZE:u16=20;
const MAX_CIGARETTES:u16=400;
/// A lit cigarette lasts this long, drawn on or not.
const BURN_SECONDS:f64=60.0;
/// A toke held this long ends in a cough; the lungs give out entirely at the maximum.
const COUGH_TOKE_SECONDS:f64=4.5;
const MAX_TOKE_SECONDS:f64=6.0;
const COUGH_SECONDS:f64=2.2;
/// Seconds of inhaling that restore one full bar: about what one cigarette allows.
pub const TOKE_SECONDS_PER_CIGARETTE:f32=20.0;
/// Each real toke sharpens the eye for this long; they stack to one cigarette's worth of tokes.
pub const BOOST_SECONDS_PER_TOKE:f64=10.0;
pub const TOKES_PER_CIGARETTE:f64=12.0;
const BOOST_MIN_TOKE_SECONDS:f64=0.5;
/// The flame must be held to the tip this long.
const LIGHT_SECONDS:f64=0.35;
const ASH_TAP_SECONDS:f64=0.7;
/// The slot whose cigarette sits tobacco-end up.
pub const INVERTED_SLOT:u8=9;
const FULL_PACK:u32=(1<<PACK_SIZE)-1;

#[derive(Clone,Copy,Debug,Default,PartialEq)]
pub struct View {
    /// The packet is out in the hand, open, for a cigarette to be chosen.
    pub pack_open:bool,
    /// Something of this kit is in the hands (the packet or a cigarette).
    pub equipped:bool,
    pub pack_count:u16,pub reserve_count:u16,
    /// Which of the packet's slots still hold a cigarette (bit per slot).
    pub pack_slots:u32,
    /// A cigarette is out: in the hand or in the mouth.
    pub cigarette_present:bool,pub in_mouth:bool,pub lit:bool,pub burn_remaining:f32,
    pub lighter_raise:f32,pub lighter_flame:bool,pub puff_phase:f32,pub exhale:f32,
    /// Seconds the current toke has been held (0 when not inhaling).
    pub toke_seconds:f32,
    pub coughing:bool,
    /// 0..1 steadier, sharper eye from recent tokes, and the seconds of it left.
    pub vision_boost:f32,pub boost_seconds:f32,
    /// 0 when idle, else the 0..1 phase of the tap that knocks the ash off.
    pub ash_tap:f32,
    /// Quarters of the cigarette smoked so far, 0..1 within the current quarter.
    pub quarter_progress:f32,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Action {#[default] None,Draw,Light,Puff,Cough,BurnedOut}
#[derive(Clone,Debug)]
pub struct State {
    slots:u32,reserve:u16,held_burn_s:Option<f64>,lit:bool,equipped:bool,in_mouth:bool,pack_open:bool,
    lighter_raise:f64,lighter_held:bool,flame_s:f64,previous_toke:bool,wait_for_release:bool,
    consumed:u32,drawn:u32,lit_count:u32,puffs:u32,
    toke_s:f64,exhale_s:f64,exhale_total:f64,last_toke_s:f64,cough_s:f64,dose_s:f64,
    boost_s:f64,boost_shown:f64,ash_tap_s:f64,quarters_tapped:u8,butt_pending:bool,
}
impl Default for State {fn default()->Self {Self{slots:FULL_PACK,reserve:0,held_burn_s:None,
    lit:false,equipped:false,in_mouth:false,pack_open:false,lighter_raise:0.0,lighter_held:false,flame_s:0.0,
    previous_toke:false,wait_for_release:false,consumed:0,drawn:0,lit_count:0,puffs:0,
    toke_s:0.0,exhale_s:0.0,exhale_total:0.0,last_toke_s:0.0,cough_s:0.0,dose_s:0.0,
    boost_s:0.0,boost_shown:0.0,ash_tap_s:0.0,quarters_tapped:0,butt_pending:false}}}
impl State {
    /// The hands are occupied by the packet or a cigarette. One in the mouth leaves them free.
    pub fn active(&self)->bool {self.equipped}
    fn pack(&self)->u16 {self.slots.count_ones() as u16}
    pub fn view(&self)->View {
        // The cloud swells and thins over the exhale, scaled by how deep the toke was.
        let exhale=if self.exhale_s>0.0&&self.exhale_total>0.0 {
            let p=(1.0-self.exhale_s/self.exhale_total).clamp(0.0,1.0);
            let depth=(self.last_toke_s/COUGH_TOKE_SECONDS).clamp(0.2,1.0);
            ((std::f64::consts::PI*p).sin()*depth) as f32
        }else{0.0};
        let remaining=self.held_burn_s.map_or(0.0,|v|(v/BURN_SECONDS).clamp(0.0,1.0));
        View{pack_open:self.pack_open,equipped:self.equipped,pack_count:self.pack(),reserve_count:self.reserve,
            pack_slots:self.slots,
            cigarette_present:self.held_burn_s.is_some()&&(self.equipped||self.in_mouth),
            in_mouth:self.in_mouth,lit:self.lit,burn_remaining:remaining as f32,
            lighter_raise:self.lighter_raise as f32,
            lighter_flame:self.equipped&&!self.pack_open&&self.lighter_held&&self.lighter_raise>=0.70,
            // The tip brightens quickly as the draw begins and stays bright while it is held.
            puff_phase:(self.toke_s/0.45).clamp(0.0,1.0) as f32,exhale,
            toke_seconds:self.toke_s as f32,coughing:self.cough_s>0.0,
            vision_boost:self.boost_shown as f32,boost_seconds:self.boost_s as f32,
            ash_tap:if self.ash_tap_s>0.0 {(1.0-self.ash_tap_s/ASH_TAP_SECONDS).clamp(0.001,1.0) as f32}else{0.0},
            quarter_progress:(((1.0-remaining)*4.0).fract()) as f32}
    }
    /// C. With nothing out it brings out the whole packet, open, to choose from; again puts it
    /// away. One in the mouth comes back to the hand. With one in the hand it shows the packet.
    pub fn toggle_pack(&mut self) {
        self.lighter_held=false;self.wait_for_release=true;
        if self.in_mouth {self.in_mouth=false;self.equipped=true;self.pack_open=false;return;}
        if self.held_burn_s.is_some() {
            if self.equipped {self.pack_open=!self.pack_open;}else{self.equipped=true;self.pack_open=false;}
            return;
        }
        if self.equipped {self.equipped=false;self.pack_open=false;return;}
        if self.slots==0&&self.reserve>=PACK_SIZE {self.slots=FULL_PACK;self.reserve-=PACK_SIZE;}
        // Nothing left to hold: empty hands stay empty rather than occupied by an absent item.
        if self.slots==0 {return;}
        self.equipped=true;self.pack_open=true;
    }
    /// Take the cigarette in `slot` out of the open packet. Never conjures or discards one.
    pub fn take(&mut self,slot:u8)->Action {
        if !self.equipped||!self.pack_open||self.held_burn_s.is_some()||u16::from(slot)>=PACK_SIZE
            ||self.slots&(1<<slot)==0 {return Action::None;}
        self.slots&=!(1<<slot);self.held_burn_s=Some(BURN_SECONDS);self.pack_open=false;
        self.lit=false;self.quarters_tapped=0;self.ash_tap_s=0.0;self.flame_s=0.0;
        self.drawn=self.drawn.saturating_add(1);self.wait_for_release=true;Action::Draw
    }
    /// The hands are needed for something else. A lit cigarette goes to the mouth and burns on;
    /// an unlit one is pocketed whole. The packet is simply put away.
    pub fn stow(&mut self)->bool {
        let active=self.equipped;
        if active&&self.held_burn_s.is_some()&&self.lit {self.in_mouth=true;}
        self.equipped=false;self.pack_open=false;self.lighter_held=false;self.flame_s=0.0;
        self.wait_for_release|=self.previous_toke;self.toke_s=0.0;active
    }
    /// Inhaled seconds since the last call. The game turns them into recovery.
    pub fn take_dose(&mut self)->f32 {std::mem::take(&mut self.dose_s) as f32}
    /// A cigarette reached its filter since the last call: the butt is to be thrown down.
    pub fn take_butt(&mut self)->bool {std::mem::take(&mut self.butt_pending)}
    pub fn accept_packet(&mut self)->bool {
        if self.total_unconsumed()+u32::from(PACK_SIZE)>u32::from(MAX_CIGARETTES) {return false;}
        self.reserve+=PACK_SIZE;true
    }
    pub fn total_unconsumed(&self)->u32 {u32::from(self.pack()+self.reserve)+u32::from(self.held_burn_s.is_some())}
    pub fn consumed(&self)->u32 {self.consumed}
    pub fn drawn(&self)->u32 {self.drawn}
    pub fn lit_count(&self)->u32 {self.lit_count}
    pub fn puffs(&self)->u32 {self.puffs}
    /// Called once per actual simulation step. `toke` is the right mouse button: held on an
    /// unlit cigarette it brings the lighter up and lights it, held on a lit one it is a draw.
    pub fn advance(&mut self,dt:f32,toke:bool)->Action {
        if !dt.is_finite()||dt<0.0 {return Action::None;}
        let dt=f64::from(dt);
        if !toke {self.wait_for_release=false;}
        self.previous_toke=toke;
        let in_hand=self.equipped&&!self.pack_open&&self.held_burn_s.is_some();
        let mut action=Action::None;
        // Lighting: the flame comes up to the tip and is held there a moment.
        self.lighter_held=in_hand&&!self.lit&&toke&&!self.wait_for_release;
        let target=if self.lighter_held {1.0}else{0.0};
        self.lighter_raise+=(target-self.lighter_raise).clamp(-dt*7.0,dt*5.0);
        if self.lighter_held&&self.lighter_raise>=0.70 {
            self.flame_s+=dt;
            if self.flame_s>=LIGHT_SECONDS {
                self.lit=true;self.lit_count=self.lit_count.saturating_add(1);self.flame_s=0.0;
                self.lighter_held=false;self.wait_for_release=true;action=Action::Light;
            }
        }else if !self.lighter_held {self.flame_s=0.0;}
        self.exhale_s=(self.exhale_s-dt).max(0.0);
        self.cough_s=(self.cough_s-dt).max(0.0);
        // A toke cannot start mid-exhale or mid-cough; held through them it begins when they end.
        let was_toking=self.toke_s>0.0;
        let able=in_hand&&self.lit;
        let toking=able&&toke&&!self.wait_for_release&&self.toke_s<MAX_TOKE_SECONDS
            &&(was_toking||(self.exhale_s<=0.0&&self.cough_s<=0.0));
        if toking {
            let inhaled=dt.min(MAX_TOKE_SECONDS-self.toke_s);
            self.toke_s+=inhaled;self.dose_s+=inhaled;
        }else if was_toking {
            // Release: the breath goes out, and a draw held too long comes back as a cough.
            self.puffs=self.puffs.saturating_add(1);self.last_toke_s=self.toke_s;
            self.exhale_total=(0.7+self.toke_s*0.8).min(4.0);self.exhale_s=self.exhale_total;
            if self.toke_s>=BOOST_MIN_TOKE_SECONDS {
                self.boost_s=(self.boost_s+BOOST_SECONDS_PER_TOKE).min(BOOST_SECONDS_PER_TOKE*TOKES_PER_CIGARETTE);
            }
            action=if self.toke_s>=COUGH_TOKE_SECONDS {self.cough_s=COUGH_SECONDS;Action::Cough}else{Action::Puff};
            self.toke_s=0.0;
            // The lungs gave out with the button still down: it must be let go before another.
            if toke {self.wait_for_release=true;}
        }
        self.boost_s=(self.boost_s-dt).max(0.0);
        let boost_target=(self.boost_s/2.0).min(1.0);
        self.boost_shown+=(boost_target-self.boost_shown).clamp(-dt/1.5,dt/1.0);
        self.ash_tap_s=(self.ash_tap_s-dt).max(0.0);
        if self.lit {if let Some(burn)=self.held_burn_s.as_mut() {
            *burn-=dt;
            if *burn<=0.0 {
                // Down to the filter: the butt is thrown away and the hand or mouth is empty.
                self.held_burn_s=None;self.lit=false;self.toke_s=0.0;self.in_mouth=false;
                self.equipped=false;self.pack_open=false;self.lighter_held=false;
                self.consumed=self.consumed.saturating_add(1);self.butt_pending=true;
                action=Action::BurnedOut;
            }else{
                let quarter=((1.0-*burn/BURN_SECONDS)*4.0).floor().clamp(0.0,3.0) as u8;
                if quarter>self.quarters_tapped {
                    self.quarters_tapped=quarter;
                    // In the hand it is tapped off; in the mouth the ash just falls.
                    if self.equipped {self.ash_tap_s=ASH_TAP_SECONDS;}
                }
            }
        }}
        action
    }
}
#[cfg(test)]mod tests {
    use super::*;
    fn draw()->State {let mut s=State::default();s.toggle_pack();assert!(s.view().pack_open);
        assert_eq!(s.take(3),Action::Draw);s.advance(0.1,false);s}
    fn light()->State {let mut s=draw();let mut lit=false;
        for _ in 0..200 {if s.advance(0.01,true)==Action::Light {lit=true;break;}}
        assert!(lit);s.advance(0.3,false);s}
    fn toke(s:&mut State,seconds:f32)->Action {
        for _ in 0..(seconds/0.01).round() as usize {s.advance(0.01,true);}s.advance(0.01,false)}
    #[test]fn c_brings_out_a_full_packet_and_each_cigarette_is_taken_by_hand(){let mut s=State::default();
        s.toggle_pack();let v=s.view();
        assert!(v.equipped&&v.pack_open&&!v.cigarette_present);assert_eq!(v.pack_count,20);
        assert_eq!(v.pack_slots,FULL_PACK);
        assert_eq!(s.take(INVERTED_SLOT),Action::Draw);let v=s.view();
        assert!(v.cigarette_present&&!v.pack_open&&!v.lit);assert_eq!(v.pack_count,19);
        assert_eq!(v.pack_slots&(1<<INVERTED_SLOT),0);assert_eq!(s.total_unconsumed(),20);
        // The same slot is empty now, and a second one cannot be taken while one is held.
        s.toggle_pack();assert!(s.view().pack_open);
        assert_eq!(s.take(INVERTED_SLOT),Action::None);assert_eq!(s.take(0),Action::None);
        assert_eq!(s.drawn(),1);
        // With nothing held, C again puts the packet away.
        let mut p=State::default();p.toggle_pack();p.toggle_pack();assert!(!p.active());
        assert_eq!(p.total_unconsumed(),20);assert_eq!(p.take(0),Action::None);}
    #[test]fn the_right_button_lights_then_tokes_and_never_both_on_one_press(){let mut s=draw();
        // Held: the lighter comes up, the flame is held to the tip, and it lights.
        let mut lit_at=None;
        for step in 0..300 {let a=s.advance(0.01,true);
            if a==Action::Light {lit_at=Some(step);break;}assert!(!s.view().lit);}
        assert!(lit_at.is_some_and(|step|step>30),"lighting takes a moment: {lit_at:?}");
        // Still held: that press lit it; it is not also a toke.
        s.advance(1.0,true);assert_eq!(s.view().toke_seconds,0.0);assert_eq!(s.puffs(),0);
        s.advance(0.3,false);assert!(s.view().lighter_raise<0.01);
        // A fresh hold is a toke; the exhale and the count come with the release.
        s.advance(1.0,true);assert!(s.view().puff_phase>0.99);
        assert_eq!(s.advance(0.01,false),Action::Puff);assert_eq!(s.puffs(),1);
        s.advance(0.5,false);assert!(s.view().exhale>0.0);
        // Unlit and untouched, nothing happens.
        let mut idle=draw();idle.advance(5.0,false);assert!(!idle.view().lit);
        assert_eq!(idle.view().burn_remaining,1.0);}
    #[test]fn exhale_grows_with_the_toke_and_a_long_hold_ends_in_a_cough(){
        let peak=|hold:f32| {let mut s=light();let action=toke(&mut s,hold);let mut top=0.0f32;
            for _ in 0..400 {s.advance(0.01,false);top=top.max(s.view().exhale);}(action,top)};
        let (short_action,short)=peak(0.6);let (long_action,long)=peak(3.5);
        assert_eq!(short_action,Action::Puff);assert_eq!(long_action,Action::Puff);
        assert!(long>short*1.8,"deep toke {long} shallow {short}");
        assert_eq!(peak(5.0).0,Action::Cough);
        // Held without limit: the lungs stop at the maximum and it ends in a cough.
        let mut held=light();let mut forced=None;let mut longest=0.0f32;
        for step in 0..2000 {longest=longest.max(held.view().toke_seconds);
            if held.advance(0.01,true)==Action::Cough {forced=Some(step);break;}}
        assert!(forced.is_some(),"the lungs must give out on their own");
        assert!(longest<=MAX_TOKE_SECONDS as f32+1e-3);assert!(held.view().coughing);
        // Still holding through the cough and after it starts nothing until the button is let go.
        held.advance(5.0,true);assert_eq!(held.view().toke_seconds,0.0);
        held.advance(0.01,false);held.advance(0.5,true);assert!(held.view().toke_seconds>0.0);}
    #[test]fn each_toke_adds_ten_seconds_of_sharper_sight_up_to_one_cigarettes_worth(){let mut s=light();
        assert_eq!(s.view().vision_boost,0.0);
        toke(&mut s,1.0);assert!((s.view().boost_seconds-10.0).abs()<0.1);
        s.advance(1.5,false);assert!(s.view().vision_boost>0.95);
        // A tap is not a toke.
        let before=s.view().boost_seconds;toke(&mut s,0.2);assert!(s.view().boost_seconds<=before);
        // Stacked, and capped.
        let mut many=State::default();
        for _ in 0..40 {
            if !many.view().cigarette_present {if !many.active() {many.toggle_pack();}
                let slot=many.view().pack_slots.trailing_zeros() as u8;many.take(slot);many.advance(0.1,false);
                for _ in 0..200 {if many.advance(0.01,true)==Action::Light {break;}}many.advance(0.1,false);}
            toke(&mut many,1.0);many.advance(1.0,false);
        }
        let cap=(BOOST_SECONDS_PER_TOKE*TOKES_PER_CIGARETTE) as f32;
        assert!(many.view().boost_seconds<=cap&&many.view().boost_seconds>cap-15.0,"{}",many.view().boost_seconds);
        // It runs out.
        let mut fade=light();toke(&mut fade,1.0);fade.advance(8.0,false);assert!(fade.view().vision_boost>0.9);
        for _ in 0..40 {fade.advance(0.1,false);}assert_eq!(fade.view().vision_boost,0.0);}
    #[test]fn a_lit_cigarette_lasts_one_minute_at_any_frame_rate_and_leaves_a_butt(){let mut result=Vec::new();
        for hz in [30,60,144] {let mut s=light();
            for _ in 0..hz*30 {s.advance(1.0/hz as f32,false);}result.push(s.view().burn_remaining);}
        assert!((result[0]-result[2]).abs()<1e-3);assert!((result[1]-0.495).abs()<0.02,"{result:?}");
        let mut s=light();assert!(!s.take_butt());
        let mut out=false;for _ in 0..700 {if s.advance(0.1,false)==Action::BurnedOut {out=true;break;}}
        assert!(out);assert!(s.take_butt());assert!(!s.take_butt());
        assert!(!s.active(),"a finished cigarette must not keep the hands occupied");
        assert!(!s.view().cigarette_present);assert_eq!(s.consumed(),1);
        assert_eq!(s.total_unconsumed()+s.consumed(),20);
        // Drawing on it does not shorten the minute.
        let mut d=light();for _ in 0..6 {toke(&mut d,2.0);d.advance(2.5,false);}
        let mut plain=light();plain.advance(6.0*4.51,false);
        assert!((d.view().burn_remaining-plain.view().burn_remaining).abs()<0.02);}
    #[test]fn put_away_lit_it_goes_to_the_mouth_and_burns_on_and_c_takes_it_back(){let mut s=light();
        s.advance(8.0,false);assert!(s.stow());let v=s.view();
        assert!(v.in_mouth&&v.lit&&v.cigarette_present&&!v.equipped);assert!(!s.active());
        let left=v.burn_remaining;s.advance(10.0,false);assert!(s.view().burn_remaining<left);
        // The right button belongs to the hands again: no toke from the mouth.
        s.advance(1.0,true);assert_eq!(s.view().toke_seconds,0.0);s.advance(0.1,false);
        s.toggle_pack();let v=s.view();assert!(v.equipped&&!v.in_mouth&&!v.pack_open&&v.lit);
        // It burns out in the mouth too, and the butt is thrown.
        s.stow();let mut out=false;
        for _ in 0..700 {if s.advance(0.1,false)==Action::BurnedOut {out=true;break;}}
        assert!(out&&s.take_butt()&&!s.view().in_mouth&&!s.view().cigarette_present);
        // An unlit one is pocketed whole and comes back to the hand on C.
        let mut u=draw();assert!(u.stow());assert!(!u.view().cigarette_present&&!u.view().in_mouth);
        u.advance(30.0,false);u.toggle_pack();
        assert!(u.view().cigarette_present&&u.view().burn_remaining==1.0);assert_eq!(u.drawn(),1);}
    #[test]fn the_ash_is_tapped_off_at_each_quarter(){let mut s=light();let mut taps=0;let mut tapping=false;
        for _ in 0..590 {s.advance(0.1,false);let now=s.view().ash_tap>0.0;
            if now&&!tapping {taps+=1;}tapping=now;}
        assert_eq!(taps,3);
        // In the mouth there is no hand to tap it.
        let mut m=light();m.stow();
        for _ in 0..590 {m.advance(0.1,false);assert_eq!(m.view().ash_tap,0.0);}}
    #[test]fn an_empty_supply_leaves_the_hands_free_and_a_reserve_packet_refills(){
        let mut empty=State::default();empty.slots=0;empty.reserve=0;
        empty.toggle_pack();assert!(!empty.active(),"an empty supply cannot equip nothing");
        assert_eq!(empty.total_unconsumed(),0);
        empty.reserve=PACK_SIZE;empty.toggle_pack();
        assert!(empty.active()&&empty.view().pack_open);assert_eq!(empty.view().pack_count,20);
        assert_eq!(empty.total_unconsumed(),u32::from(PACK_SIZE));}
    #[test]fn pickup_capacity_never_silently_loses_a_packet(){let mut s=State::default();
        for _ in 0..19 {assert!(s.accept_packet());}assert!(!s.accept_packet());assert_eq!(s.total_unconsumed(),400);}
    #[test]fn pickup_capacity_counts_the_cigarette_already_in_hand(){let mut s=draw();
        s.slots=0;s.reserve=380;
        assert_eq!(s.total_unconsumed(),381);assert!(!s.accept_packet());
        assert_eq!(s.total_unconsumed(),381);}
}
