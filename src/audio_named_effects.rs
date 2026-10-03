//! Explicit user-named diegetic effects. Prepared once, finite, physical World/Impact routes.
//! No score event queue, automatic gain from musical cues, source stacking, or callback decode.
use super::Audio;
use std::{cell::RefCell,collections::HashMap,sync::Arc};
const EVENTS:[&str;8]=["cigarette_pickup","hammer_pickup","trigger_pull_not_loaded",
    "jammed_gun_elevation","jammed_traverse","fire_igniting","impact_stopped","light_round_impact"];
const MAX_MONO_BYTES:usize=8*1024*1024;
const MAX_SECONDS:usize=12;
const MAX_VARIANTS:usize=4;
struct Variant {id:String,mono:Arc<[f32]>,character:Option<Arc<[f32]>>}
#[derive(Default)]
pub(super) struct Bank {
    sources:HashMap<String,Vec<Variant>>,
    cursors:RefCell<HashMap<String,u64>>,
    bytes:usize,
    rejected:Vec<String>,
}
fn valid_role(cue:&crate::sound_library::Cue)->bool {
    cue.role=="effect"&&!cue.looping
        && !cue.role_aliases.iter().any(|r|matches!(r.as_str(),"music"|"combat"|"explore"|"driving"|"night"|"day"))
}
impl Bank {
    pub(super) fn load()->Self {
        let mut bank=Self::default();
        if std::env::var("KESSEL_ENABLE_PIZZA_AUDIO").is_ok_and(|v|v=="0") {return bank;}
        let Ok(library)=crate::sound_library::Library::load() else {return bank;};
        for event in EVENTS {
            for id in library.events.get(event).into_iter().flatten().take(MAX_VARIANTS) {
                let Some(cue)=library.cues.get(id).filter(|cue|valid_role(cue)) else {continue;};
                let limit=(MAX_SECONDS*crate::acoustics::SAMPLE_RATE as usize*4+4096) as u64;
                if std::fs::metadata(&cue.path).map_or(true,|m|m.len()>limit) {
                    bank.rejected.push(format!("{event}:{id}:encoded_budget"));continue;
                }
                let Ok(pcm)=crate::sound_library::decode(&cue.path) else {
                    bank.rejected.push(format!("{event}:{id}:decode"));continue;
                };
                // Collapse stereo once to an actual positioned point source. Source tails remain
                // untouched; callers choose DRY for this unverified recorded room response.
                let mono:Vec<f32>=pcm.samples.chunks_exact(2).map(|f|(f[0]+f[1])*0.5).collect();
                if mono.len()>MAX_SECONDS*crate::acoustics::SAMPLE_RATE as usize
                    ||!bank.admit(event,id,mono) {
                    bank.rejected.push(format!("{event}:{id}:pcm_budget_or_silence"));
                }
            }
        }
        bank
    }
    fn admit(&mut self,event:&str,id:&str,mono:Vec<f32>)->bool {
        if !EVENTS.contains(&event)||mono.is_empty()||mono.iter().any(|v|!v.is_finite())
            ||mono.iter().all(|v|v.abs()<1e-9)||self.bytes+mono.len()*4
                +contact_policy(event).map_or(0,|(seconds,_)|(seconds*crate::acoustics::SAMPLE_RATE as f32) as usize*4)>MAX_MONO_BYTES
            ||self.sources.get(event).is_some_and(|v|v.len()>=MAX_VARIANTS) {return false;}
        self.bytes+=mono.len()*4;
        let character=contact_policy(event).map(|(seconds,_)|prepare_character(&mono,seconds).into());
        self.bytes+=character.as_ref().map_or(0,|pcm:&Arc<[f32]>|pcm.len()*4);
        self.sources.entry(event.into()).or_default().push(Variant{id:id.into(),mono:mono.into(),character});true
    }
    fn next_variant(&self,event:&str)->Option<&Variant> {
        let variants=self.sources.get(event)?;
        let mut cursors=self.cursors.borrow_mut();let ordinal=cursors.entry(event.into()).or_default();
        let source=&variants[*ordinal as usize%variants.len()];
        *ordinal=ordinal.wrapping_add(1);Some(source)
    }
    pub(super) fn next(&self,event:&str)->Option<Arc<[f32]>> {
        self.next_variant(event).map(|source|Arc::clone(&source.mono))
    }
    fn character(&self,event:&str)->Option<Arc<[f32]>> {
        self.next_variant(event)?.character.clone()
    }
    pub(super) fn metadata(&self)->serde_json::Value {
        serde_json::json!({"scope":"explicit finite named effects; actual gameplay events only",
            "pcm_bytes":self.bytes,"budget_bytes":MAX_MONO_BYTES,"max_clip_seconds":MAX_SECONDS,
            "events":self.sources.iter().map(|(key,variants)|serde_json::json!({"event":key,
                "variants":variants.iter().map(|v|serde_json::json!({"id":v.id,"frames":v.mono.len(),"character_frames":v.character.as_ref().map(|p|p.len()),"character_policy":contact_policy(key)})).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),"source_selections":*self.cursors.borrow(),"rejected":self.rejected,
            "gain":"existing physical emission/ear calibration, not soundscape cue gain",
            "embedded_tail":"controls retain recorded tail without another rack; combat character excerpts stay inside existing physical voice",
            "contact_mix":"low peak-relative authored character; one composite voice with original integrated source energy, support and onset"})
    }
}
/// Small source-character windows, not complete replacement blasts or full recorded rooms.
/// Ratios bound character pressure to the existing physical voice's peak before energy matching.
fn stopped_terminal(terminal:crate::ballistics::Terminal)->bool {
    matches!(terminal,crate::ballistics::Terminal::Bounce|crate::ballistics::Terminal::Ricochet|crate::ballistics::Terminal::Shatter)
}
fn contact_policy(name:&str)->Option<(f32,f32)> {Some(match name {
    "fire_igniting"=>(0.40,0.18),"impact_stopped"=>(0.45,0.12),
    "light_round_impact"=>(0.14,0.22),_=>return None,
})}
fn prepare_character(source:&[f32],seconds:f32)->Vec<f32> {
    let peak=source.iter().fold(0.0f32,|a,b|a.max(b.abs()));
    let start=source.iter().position(|x|x.abs()>peak*0.005).unwrap_or(0).saturating_sub(16);
    let count=((seconds*crate::acoustics::SAMPLE_RATE as f32) as usize).min(source.len().saturating_sub(start));
    let mut out=source[start..start+count].to_vec();
    // Remove sub-audible/DC drift from authored synth effects; preserve their supplied pitch.
    let a=(-std::f32::consts::TAU*25.0/crate::acoustics::SAMPLE_RATE as f32).exp();
    let (mut old_x,mut old_y)=(0.0,0.0);
    let fade=(0.012*crate::acoustics::SAMPLE_RATE as f32) as usize;
    for (i,x) in out.iter_mut().enumerate() {
        let y=a*(old_y+*x-old_x);old_x=*x;old_y=y;
        *x=y*(i as f32/16.0).min(1.0)*((count-1-i) as f32/fade as f32).min(1.0);
    }
    out
}
/// One composite source, no second sound event. No sample begins before the original voice's
/// onset and no sample extends past its support; its integrated stereo energy is retained.
fn blend_character(base:&mut[f32],character:&[f32],bearing:f32,ratio:f32)->bool {
    if base.len()<4||character.is_empty()||!bearing.is_finite()
        ||base.iter().chain(character).any(|v|!v.is_finite()) {return false;}
    let energy=base.iter().map(|v|f64::from(*v).powi(2)).sum::<f64>();
    let peak=base.iter().fold(0.0f32,|a,b|a.max(b.abs()));
    let cp=character.iter().fold(0.0f32,|a,b|a.max(b.abs()));
    if energy<1e-18||cp<1e-9{return false;}
    let start=base.chunks_exact(2).position(|f|f[0].abs().max(f[1].abs())>peak*0.0005).unwrap_or(0);
    let positioned=crate::acoustics::spatialize_mono(character,bearing,0.0);
    let gain=ratio.clamp(0.0,0.25)*peak/cp;
    let available=base.len()-start*2;
    let cut=positioned.len()>available;
    let fade=(0.012*crate::acoustics::SAMPLE_RATE as f32) as usize*2;
    for (i,(dst,src)) in base[start*2..].iter_mut().zip(positioned).enumerate() {
        let tail=if cut {((available-1-i) as f32/fade as f32).min(1.0)}else{1.0};
        *dst+=src*gain*tail;
    }
    let combined=base.iter().map(|v|f64::from(*v).powi(2)).sum::<f64>();
    if combined>1e-18 {let keep=(energy/combined).sqrt() as f32;for x in base {*x*=keep;}}
    true
}
impl Audio {
    pub(crate) fn named_effect_mono(&self,name:&str)->Option<Arc<[f32]>> {self.named_effects.next(name)}
    pub fn named_effect_snapshot(&self)->serde_json::Value {self.named_effects.metadata()}
    /// Already-at-ear control: headset transfer and bearing apply normally, without a second
    /// authored room rack over an unverified recorded tail. Return false to select the fallback.
    pub(crate) fn play_named_control(&self,name:&str,level:f32,headset:bool,bearing:f32)->bool {
        let Some(mono)=self.named_effect_mono(name) else {return false;};
        let ear=crate::sound_propagation::at_ear(level+15.0,level,
            crate::sound_levels::SpectrumClass::Mechanical,1.0,headset);
        self.play_world_mono_at_ear(&mono,&ear,bearing,crate::acoustics::ReverbParams::DIRECT);true
    }
    /// Calibrated gameplay keeps its physical voice and adds only the explicitly named character.
    pub(crate) fn blend_named_ignition(&self,base:&mut[f32])->bool {
        self.blend_named_contact("fire_igniting",base,0.0)
    }
    pub(crate) fn blend_named_light_armor(&self,base:&mut[f32],bearing:f32)->bool {
        self.blend_named_contact("light_round_impact",base,bearing)
    }
    pub(crate) fn blend_named_stopped_armor(&self,base:&mut[f32],bearing:f32,
        terminal:crate::ballistics::Terminal)->bool {
        if !stopped_terminal(terminal) {return false;}
        self.blend_named_contact("impact_stopped",base,bearing)
    }
    fn blend_named_contact(&self,event:&str,base:&mut[f32],bearing:f32)->bool {
        let Some((_,ratio))=contact_policy(event) else {return false;};
        let Some(character)=self.named_effects.character(event) else {return false;};
        blend_character(base,&character,bearing,ratio)
    }
    pub(crate) fn named_stopped_armor_world(&self,sfx:super::Sfx,
        terminal:crate::ballistics::Terminal)->Option<Vec<f32>> {
        if !stopped_terminal(terminal) {return None;}
        if !self.named_effects.sources.contains_key("impact_stopped") {return None;}
        let mono=self.world_source(sfx)?;
        let mut source=crate::acoustics::spatialize_mono(&mono,0.0,0.0);
        self.blend_named_stopped_armor(&mut source,0.0,terminal).then_some(source)
    }
    /// Preserve the existing positioned wavefront caller by returning only its dry source.
    pub(crate) fn named_effect_stereo(&self,name:&str)->Option<Vec<f32>> {
        self.named_effect_mono(name).map(|m|crate::acoustics::spatialize_mono(&m,0.0,0.0))
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn only_explicit_finite_events_are_admitted_and_missing_stays_fallback(){
        let mut b=Bank::default();assert!(!b.admit("music","a",vec![0.1;16]));
        assert!(!b.admit("hammer_pickup","a",vec![f32::NAN;16]));
        assert!(!b.admit("hammer_pickup","a",vec![0.0;16]));assert!(b.next("hammer_pickup").is_none());
        assert!(b.admit("hammer_pickup","a",vec![0.1;16]));assert!(b.admit("hammer_pickup","b",vec![0.2;16]));
        for expected in [0.1,0.2,0.1,0.2] {let s=b.next("hammer_pickup").unwrap();assert_eq!(s.len(),16);assert_eq!(s[0],expected);}
    }
    #[test]fn contact_character_keeps_energy_onset_support_and_finite_tail(){
        let mut base=vec![0.0f32;4000];
        for frame in 100..1900 {let v=((frame-100) as f32*0.13).sin()*0.4;
            base[frame*2]=v;base[frame*2+1]=v*0.8;}
        let original=base.clone();let energy=|v:&[f32]|v.iter().map(|x|f64::from(*x).powi(2)).sum::<f64>();
        let prepared=prepare_character(&vec![0.3;18000],0.45);
        assert!(blend_character(&mut base,&prepared,45.0,0.22));
        assert_eq!(base.len(),original.len());assert!(base[..200].iter().all(|v|*v==0.0));
        assert_eq!(*base.last().unwrap(),0.0);assert!(base.iter().all(|v|v.is_finite()));
        assert!((energy(&base)/energy(&original)-1.0).abs()<1e-6);
        assert!(base.iter().zip(&original).any(|(a,b)|(a-b).abs()>1e-5));
    }
    #[test]fn silent_or_invalid_physical_source_never_invents_a_contact(){
        let mut silence=vec![0.0;100];let copy=silence.clone();
        assert!(!blend_character(&mut silence,&[0.1;100],0.0,0.2));assert_eq!(silence,copy);
        let mut source=vec![0.1;100];let copy=source.clone();
        assert!(!blend_character(&mut source,&[f32::NAN;20],0.0,0.2));assert_eq!(source,copy);
        assert!(contact_policy("surprise_contact").is_none());assert!(contact_policy("error").is_none());
        assert!(contact_policy("light_impact").is_none());
    }
    #[test]fn stopped_character_never_admits_penetration_or_he_blast(){
        use crate::ballistics::Terminal;
        assert!(stopped_terminal(Terminal::Bounce));assert!(stopped_terminal(Terminal::Ricochet));assert!(stopped_terminal(Terminal::Shatter));
        assert!(!stopped_terminal(Terminal::Penetration));assert!(!stopped_terminal(Terminal::HeBlast));
    }
    #[test]fn contact_excerpts_are_finite_and_do_not_keep_the_full_recorded_room(){
        for name in ["fire_igniting","impact_stopped","light_round_impact"] {
            let mut bank=Bank::default();let source=vec![0.1;44100*10];
            assert!(bank.admit(name,"fixture",source));let v=bank.next_variant(name).unwrap();
            let excerpt=v.character.as_ref().unwrap();
            assert!(excerpt.len()<=19845);assert_eq!(excerpt[0],0.0);assert_eq!(*excerpt.last().unwrap(),0.0);
            assert_eq!(v.mono.len(),441000);assert!(bank.bytes<MAX_MONO_BYTES);
        }
    }
    #[test]fn source_cache_and_per_event_variants_have_hard_caps(){
        let mut b=Bank::default();for i in 0..MAX_VARIANTS {assert!(b.admit("hammer_pickup",&i.to_string(),vec![0.1;16]));}
        assert!(!b.admit("hammer_pickup","overflow",vec![0.1;16]));
        assert!(!b.admit("cigarette_pickup","overflow",vec![0.1;MAX_MONO_BYTES/4]));assert!(b.bytes<MAX_MONO_BYTES);
    }
}
