//! Calibrated playback: every voice here is placed by its physical level at the listener's ear
//! (`sound_levels` → `sound_propagation`) through the one reproduction curve
//! (`reproduction::tone_map_dbfs`). A waveform is normalised by its own measured short-term
//! A-weighted maximum, so no source carries a gain of its own: the level a voice plays at is
//! `tone_map(LAF at the ear) − LAF,max of the rendered waveform`.
//!
//! World voices are rendered at hear-time: the dry source passes the fitted spectral transfer of
//! its propagation (air, ground, barrier, hull, headset), the head-relative pan and head shadow,
//! and the listener's environment tail; the result is measured and placed.

use std::sync::Arc;
use std::cell::RefCell;

use super::{Audio, EarBus, Sfx};
use crate::acoustics::{self, ReverbParams};
use crate::reproduction::{self, laf_max_dbfs};
use crate::sound_propagation::{EarLevel, SpectralTransfer};

/// Where a measured waveform's LAF,max lies, so repeated renders of the same source can measure
/// a short window around it instead of the whole buffer.
#[derive(Clone, Copy, Debug)]
pub(super) struct MeasuredSource {
    /// LAF,max of the dry source played centred, dBFS(A).
    pub laf_dbfs: f32,
    /// Frame of the maximum.
    pub max_frame: usize,
}

/// Measure a mono source played centred at equal power: LAF,max and where it occurs.
pub(super) fn measure_mono(mono: &[f32]) -> MeasuredSource {
    let mut meter = reproduction::LafMeter::new(reproduction::SAMPLE_RATE);
    let g = std::f32::consts::FRAC_1_SQRT_2;
    let (mut best, mut at) = (f32::NEG_INFINITY, 0usize);
    for (frame, &x) in mono.iter().enumerate() {
        let level = meter.push(x * g, x * g);
        if level > best {
            best = level;
            at = frame;
        }
    }
    MeasuredSource { laf_dbfs: best, max_frame: at }
}

/// LAF,max of a stereo render, measured over a window around the expected maximum. The meter
/// starts 0.4 s early (3.2 fast time constants: < 0.2 dB of lost history) and runs 0.6 s past
/// the source's maximum, which covers the delay of the environment tail.
pub(super) fn measure_render_window(stereo: &[f32], max_frame: usize) -> f32 {
    let rate = reproduction::SAMPLE_RATE as usize;
    let frames = stereo.len() / 2;
    let start = max_frame.saturating_sub(rate * 2 / 5).min(frames);
    let end = (max_frame + rate * 3 / 5).min(frames).max(start);
    if end - start < rate / 4 {
        return laf_max_dbfs(stereo);
    }
    laf_max_dbfs(&stereo[start * 2..end * 2])
}

/// Physical level (dB(A) at the operator's ear) of a hand-scale control or foley key. These
/// sources are not in the research report: `sound_levels::HAND_MECHANISM`, `QUIET_HANDLING` and
/// `TIGER_STARTER` are estimates, named in the audit. Callers keep a relative effort factor
/// (their former linear volume, as 20·log10) around the class level [E, authored].
pub fn control_level_dba(key: &str) -> f32 {
    use crate::sound_levels as levels;
    match key {
        // Local boot contact on the steel turret floor, at the player's feet. Explicit
        // authored estimate; do not inherit the much quieter map-paper class.
        k if k.starts_with("footstep_metal_") => 78.0,
        "hatch_stop" | "hammer_strike" | "breech" => levels::HAND_MECHANISM.laf_db,
        // A lid rolling on its hinge squeals below its stop.
        "hatch_move" => levels::HAND_MECHANISM.laf_db - 6.0,
        "traverse_jam" => levels::TURRET_TRAVERSE_HYDRAULIC.laf_db + 4.0,
        "starter" => levels::TIGER_STARTER.laf_db,
        "map_open" | "map_close" | "compass_equip" | "compass_stow" => {
            levels::QUIET_HANDLING.laf_db - 10.0
        }
        "trigger" | "p08_mag_out" | "p08_mag_in" | "p08_toggle_pull" | "p08_toggle_close" => {
            levels::QUIET_HANDLING.laf_db + 10.0
        }
        _ => levels::QUIET_HANDLING.laf_db,
    }
}

/// The effort factor of a legacy linear volume, in dB (clamped to ±40 dB).
pub fn effort_db(volume: f32) -> f32 {
    if volume.is_finite() && volume > 0.0 { (20.0 * volume.log10()).clamp(-40.0, 12.0) } else { -200.0 }
}

/// Render a dry mono world source for the ear: the path's spectral transfer (air absorption,
/// ground, barrier, plates, hatches and headset per band), then the binaural stage
/// (`acoustics::spatial::render_through`: interaural delay, head shadow, presentation pan and the
/// direction cues the plates and open hatches let through), then the listener's environment tail.
/// The broadband level is left to the caller's placement.
pub(super) fn render_for_ear(mono: &[f32], ear: &EarLevel, bearing_deg: f32, reverb: ReverbParams)
    -> Vec<f32>
{
    render_for_ear_in(mono, ear, bearing_deg, Room::Parametric(reverb))
}

/// Seconds of each voice convolved with a traced reflected field (cost bound, named in the audit).
pub const TRACED_WET_SOURCE_S: f32 = 0.75;

/// The space a voice is heard in: the reflected field traced through the scene's geometry
/// (`acoustics::raytrace`), or a parametric late tail where no trace exists (offline tools, the
/// first moments in a new place while its trace is still being spent, the pre-rendered MG
/// receiver).
#[derive(Clone, Copy)]
pub enum Room<'a> {
    Parametric(ReverbParams),
    Traced(&'a acoustics::raytrace::StereoIr),
}

/// `render_for_ear` in a given room: the dry binaural path, plus the voice convolved with the
/// traced reflected field when there is one.
pub(super) fn render_for_ear_in(mono: &[f32], ear: &EarLevel, bearing_deg: f32, room: Room<'_>)
    -> Vec<f32>
{
    let mut filtered = mono.to_vec();
    SpectralTransfer::fit(&ear.transfer_db, reproduction::SAMPLE_RATE).apply_mono(&mut filtered);
    let dry = acoustics::spatial::render_through(&filtered,
        acoustics::spatial::Direction::horizontal(bearing_deg), ear.transmission, &ear.spectrum,
        acoustics::spatial::HeadModel::PRESENTED);
    match room {
        Room::Parametric(reverb) => acoustics::schroeder_reverb(&dry, reverb),
        Room::Traced(ir) => {
            // The reflected field is convolved from the voice's first `TRACED_WET_SOURCE_S`
            // (where its energy is); the dry path keeps the whole voice. A named cost bound.
            let head = filtered.len().min((TRACED_WET_SOURCE_S * reproduction::SAMPLE_RATE as f32) as usize);
            let mut wet = ir.convolve(&filtered[..head]);
            if wet.len() < dry.len() { wet.resize(dry.len(), 0.0); }
            for (w, d) in wet.iter_mut().zip(&dry) { *w += *d; }
            wet
        }
    }
}

// Preparation runs on the game thread, never the device callback. Keep exact transfer bits:
// caching must not approximate the headset/path or bypass its calibrated level.
#[derive(Default)]
struct ReceiverCache {
    entries: Vec<([u32;10], Arc<[f32]>, f32)>,
    hits:u64, misses:u64, max_prepare_us:u64,
}
thread_local! { static RECEIVER_CACHE:RefCell<ReceiverCache>=RefCell::new(ReceiverCache::default()); }

fn prepared_receiver(seed:u32,ear:&EarLevel)->(Arc<[f32]>,f32) {
    let mut key=[0u32;10]; key[0]=seed%12;
    for (target,value) in key[1..].iter_mut().zip(ear.transfer_db) {*target=value.to_bits();}
    RECEIVER_CACHE.with(|cache| {
        let mut cache=cache.borrow_mut();
        if let Some((_,pcm,level))=cache.entries.iter().find(|(found,_,_)|*found==key) {
            let found=(pcm.clone(),*level);cache.hits+=1;return found;
        }
        let start=std::time::Instant::now();
        let mono=acoustics::synth_mg_receiver_round(3701+key[0]*7919);
        let pcm:Arc<[f32]>=render_for_ear(&mono,ear,0.0,ReverbParams::DIRECT).into();
        let level=laf_max_dbfs(&pcm);
        cache.misses+=1;cache.max_prepare_us=cache.max_prepare_us.max(start.elapsed().as_micros() as u64);
        if cache.entries.len()==32 {cache.entries.remove(0);}
        cache.entries.push((key,pcm.clone(),level));(pcm,level)
    })
}

pub(super) fn preparation_metadata()->serde_json::Value {
    RECEIVER_CACHE.with(|c| {let c=c.borrow();serde_json::json!({
        "scope":"current game thread local MG receiver preparation; not device underrun telemetry",
        "receiver_cache_entries":c.entries.len(),"receiver_cache_capacity":32,
        "receiver_cache_hits":c.hits,"receiver_cache_misses":c.misses,
        "maximum_receiver_prepare_us":c.max_prepare_us,"variants":12,
        "backend_buffer_policy":"rodio 0.20 default device configuration; host quantum unchanged",
    })})
}

impl Audio {
    pub(crate) fn play_mg_receiver_at_ear(&self,seed:u32,ear:&EarLevel) {
        if !ear.laf_db.is_finite(){return;}
        let (pcm,level)=prepared_receiver(seed,ear);
        self.play_stereo_on(pcm.to_vec(),reproduction::voice_gain(ear.laf_db,level),EarBus::World);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound_levels as levels;
    use crate::sound_propagation::{propagate, Atmosphere, Emission, Enclosure, Path};

    /// CPU-only listening fixtures using the actual Audio buses, physical ear placement,
    /// hearing/reflex and master. No sound device, renderer, offline normalization or film import.
    /// A fixed2m exterior muzzle path is a declared review geometry, not an authored MG locator.
    #[test]
    fn weapon_timbre_candidates_through_live_audio_review() {
        let Ok(prefix) = std::env::var("KESSEL_WEAPON_TIMBRE_REVIEW_PREFIX") else { return; };
        use acoustics::OwnGunTimbre as G;
        use super::super::small_arms::MgTimbre as M;
        let receipt = format!("{prefix}_receipt.json");
        assert!(!std::path::Path::new(&receipt).exists(), "preserve previous review");
        if let Some(parent) = std::path::Path::new(&prefix).parent() { std::fs::create_dir_all(parent).unwrap(); }
        let rate = acoustics::SAMPLE_RATE as usize;
        let frames_per_tick = rate / 60;
        let mut rows = Vec::new();
        for profile in [G::PreviousModal, G::PressureBody, G::CompactPressure] {
            for interior in [true, false] {
                let mut audio = Audio::new_capture();
                let sfx = if interior { Sfx::PlayerGunInt1 } else { Sfx::PlayerGunExt1 };
                let mut source = acoustics::synth_gun_punch_timbre(interior,
                    if interior { 1_271 } else { 1_071 }, profile);
                // Same preparation as synth_procedural_dry and synth_procedural, then the
                // same i16 storage conversion as the generated Clip bank. Playback is float.
                acoustics::soft_limit_in_place(&mut source, 0.995);
                if interior { source = acoustics::schroeder_reverb(&source, ReverbParams::STEEL_INTERIOR); }
                acoustics::soft_limit_in_place(&mut source, 0.995);
                let samples = source.iter().map(|x| (x.clamp(-1.0,1.0)*32767.0) as i16).collect::<Vec<_>>();
                audio.clips[sfx.index()] = Some(super::super::Clip {
                    channels:2, sample_rate:acoustics::SAMPLE_RATE, samples:samples.into(),
                });
                audio.level_cache.borrow_mut().clear();
                audio.drive_cache.borrow_mut().clear();
                let level = if interior { &levels::OWN_88_BUTTONED } else { &levels::OWN_88_HEAD_OUT };
                let ear = crate::sound_propagation::at_ear(level.lpk_db,level.laf_db,
                    level.spectrum,level.a_duration_ms,true);
                let mut hearing = crate::hearing::Hearing::default();
                let mut pcm = Vec::new();
                for tick in 0..180 {
                    if tick == 15 {
                        let (onset, envelope) = audio.direct_drive(sfx).unwrap();
                        hearing.impulse_with_envelope(crate::hearing::ear_impulses(&ear,0.0), onset,Some(envelope));
                        audio.play_at_ear(sfx,ear.laf_db);
                    }
                    hearing.advance(1.0/60.0,1.0/60.0,0.0);
                    audio.set_ear_state(&hearing.render());
                    pcm.extend(audio.render_capture_frames(frames_per_tick).unwrap());
                }
                let route = if interior { "buttoned" } else { "head_out" };
                let file = format!("{prefix}_gun_{}_{route}.wav",profile.name());
                assert!(!std::path::Path::new(&file).exists());
                acoustics::write_wav_stereo(&file,&pcm,acoustics::SAMPLE_RATE).unwrap();
                rows.push(serde_json::json!({"kind":"main_gun","candidate":profile.name(),"route":route,
                    "file":file,"onset_s":0.25,"ear_laf_dba":ear.laf_db,
                    "measured_laf_dbfs":laf_max_dbfs(&pcm),"peak_dbfs":reproduction::peak_dbfs(&pcm),
                    "master":audio.master_telemetry(),"source_fn":"synth_gun_punch_timbre",
                    "playback":"candidate installed in generated Clip slot → play_at_ear → hearing → master",
                    "recoil_and_reload_included":false,"calibration_gain_added":false}));
            }
        }
        for profile in [M::Previous,M::BodyAndCrack,M::Sharper] {
            let (pcm,mut row)=render_mg_timbre_fixture(profile);
            let file=format!("{prefix}_mg_{}_buttoned.wav",profile.name());
            assert!(!std::path::Path::new(&file).exists());
            acoustics::write_wav_stereo(&file,&pcm,acoustics::SAMPLE_RATE).unwrap();
            row["file"]=serde_json::json!(file);
            rows.push(row);
        }
        let result=serde_json::json!({"schema":"panzer.weapon-timbre-candidates.v1","sample_rate_hz":acoustics::SAMPLE_RATE,
            "channels":2,"takes":rows,"subjective_acceptance":"not established",
            "working_recommendation":"pressure_body + body_and_crack; listening hypothesis only",
            "reference_audio_imported":false,"export_gain":1.0,
            "scope":"device-free real Audio graph; fixed fixture geometry, not a live firefight; no artificial bass/master gain"});
        std::fs::write(&receipt,serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        eprintln!("weapon timbre review: {receipt}");
    }

    fn render_mg_timbre_fixture(profile: super::super::small_arms::MgTimbre)
        -> (Vec<f32>, serde_json::Value)
    {
        use super::super::small_arms::{self, MgTimbre as M, Report};
        let frames_per_tick = acoustics::SAMPLE_RATE as usize / 60;
        let rounds_per_second = crate::mounted_mg::ROUNDS_PER_SECOND as f32;
        let burst_rounds = 24usize;
        let mut exterior = Emission::from_level(&levels::MG34_OUTSIDE_1M);
        exterior.laf_db = levels::MG34_OUTSIDE_1M.lae_db.unwrap() + levels::LAF_MINUS_LAE_DB;
        let atmosphere = Atmosphere::WINTER;
        let muzzle_ear = propagate(&exterior,&Path { distance_m:2.0,atmosphere,occluded:false,
            enclosure:Enclosure::Buttoned,headset:true });
        let local = &levels::MG34_INSIDE;
        let receiver_ear = crate::sound_propagation::at_ear(local.lpk_db,
            local.laf_db-10.0*rounds_per_second.log10()+levels::LAF_MINUS_LAE_DB,
            local.spectrum,local.a_duration_ms,true);
        let delay = 2.0 / atmosphere.speed_of_sound();

            let audio = Audio::new_capture();
            let recording = audio.feedback_source("rifle_shot");
            let sources = (0..12).map(|i| small_arms::dry_report_timbre(Report::MachineGun,
                3701+i*7919,recording.as_deref(),profile)).collect::<Vec<_>>();
            let mut events = Vec::new();
            for round in 0..burst_rounds {
                let at = 0.25 + round as f32/rounds_per_second;
                events.push((at,round,false));
                events.push((at+delay,round,true));
            }
            events.sort_by(|a,b| a.0.total_cmp(&b.0));
            let mut event_index = 0;
            let mut hearing = crate::hearing::Hearing::default();
            let mut pcm = Vec::new();
            for tick in 0..180 {
                let time = tick as f32/60.0;
                let dt = 1.0/60.0;
                audio.begin_simulation_audio_frame(time,dt);
                while event_index < events.len() && events[event_index].0 < time+dt {
                    let (at,round,muzzle) = events[event_index];
                    audio.set_simulation_audio_time(at);
                    let (ear,bearing) = if muzzle { (&muzzle_ear,5.0) } else { (&receiver_ear,0.0) };
                    hearing.impulse_after(crate::hearing::ear_impulses(ear,bearing),(at-time).max(0.0));
                    if muzzle {
                        audio.play_world_mono_at_ear(&sources[round%12],ear,bearing,ReverbParams::STEEL_INTERIOR);
                    } else if profile != M::Previous {
                        audio.play_mg_receiver_at_ear(round as u32,ear);
                    } else {
                        let mono=acoustics::synth_mg_receiver_round_timbre(3701+(round as u32%12)*7919,false);
                        audio.play_world_mono_at_ear(&mono,ear,0.0,ReverbParams::DIRECT);
                    }
                    event_index += 1;
                }
                audio.finish_simulation_audio_frame();
                hearing.advance(dt,dt,0.0);
                audio.set_ear_state(&hearing.render());
                pcm.extend(audio.render_capture_frames(frames_per_tick).unwrap());
            }
            assert_eq!(event_index,48,"every real round needs one receiver and one propagated muzzle report");
            let row=serde_json::json!({"kind":"machine_gun","candidate":profile.name(),"route":"buttoned",
"rounds":burst_rounds,"rounds_per_second":rounds_per_second,
                "event_count":event_index,"muzzle_path_m":2.0,"muzzle_delay_s":delay,
                "receiver_ear_laf_dba":receiver_ear.laf_db,"muzzle_ear_laf_dba":muzzle_ear.laf_db,
                "measured_laf_dbfs":laf_max_dbfs(&pcm),"peak_dbfs":reproduction::peak_dbfs(&pcm),
                "master":audio.master_telemetry(),"recorded_rifle_early_crop_present":recording.is_some(),
                "legacy_loop_used":false,"hull_headset_and_hearing_enabled":true,
                "playback":"per-round world muzzle plus local receiver → sample-clock timeline → hearing → master"});
            (pcm,row)
    }

    #[test]
    fn mg_interior_body_and_crack_survive_real_listener_path_at_fixed_levels() {
        use super::super::small_arms::MgTimbre as M;
        let (old,old_route)=render_mg_timbre_fixture(M::Previous);
        let (new,new_route)=render_mg_timbre_fixture(M::BodyAndCrack);
        // Complete FFT-bin integration of the final graph, after hull/headset transfer,
        // hearing/reflex, per-voice LAF placement and master. Sparse frequency probes alias
        // the 800-RPM burst's comb and can misreport bass/edge changes by several decibels.
        let bands=|pcm:&[f32]| -> [f64;2] {
            let n=(pcm.len()/2).next_power_of_two();
            let mut bins=vec![(0.0f64,0.0f64);n];
            let mut time_energy=0.0;
            for (dst,frame) in bins.iter_mut().zip(pcm.chunks_exact(2)) {
                dst.0=0.5*f64::from(frame[0]+frame[1]);time_energy+=dst.0*dst.0;
            }
            let mut j=0;
            for i in 1..n {
                let mut bit=n>>1;
                while j&bit!=0 {j^=bit;bit>>=1;}
                j^=bit;if i<j {bins.swap(i,j);}
            }
            let mut width=2;
            while width<=n {
                let angle=-std::f64::consts::TAU/width as f64;
                let step=(angle.cos(),angle.sin());
                for start in (0..n).step_by(width) {
                    let mut phase=(1.0,0.0);
                    for k in 0..width/2 {
                        let odd=bins[start+k+width/2];
                        let v=(odd.0*phase.0-odd.1*phase.1,odd.0*phase.1+odd.1*phase.0);
                        let u=bins[start+k];
                        bins[start+k]=(u.0+v.0,u.1+v.1);
                        bins[start+k+width/2]=(u.0-v.0,u.1-v.1);
                        phase=(phase.0*step.0-phase.1*step.1,phase.0*step.1+phase.1*step.0);
                    }
                }
                width*=2;
            }
            let frequency_energy=bins.iter().map(|(r,i)|r*r+i*i).sum::<f64>()/n as f64;
            assert!((frequency_energy/time_energy-1.0).abs()<1e-7,"FFT measurement must conserve energy");
            [(80.0,350.0),(2500.0,8000.0)].map(|(lo,hi)| {
                bins.iter().take(n/2+1).enumerate().filter_map(|(i,(r,im))| {
                    let hz=i as f64*44_100.0/n as f64;
                    (hz>=lo&&hz<hi).then_some(r*r+im*im)
                }).sum()
            })
        };
        let old_bands=bands(&old);let new_bands=bands(&new);
        let bass_db=10.0*(new_bands[0]/old_bands[0]).log10();
        let edge_db=10.0*(new_bands[1]/old_bands[1]).log10();
        eprintln!("[actual-MG-interior] 80–350Hz {bass_db:+.3}dB; 2.5–8kHz {edge_db:+.3}dB");
        assert!(bass_db>=3.0,"calibrated interior MG body gain is only {bass_db:.3}dB");
        assert!(edge_db>=3.0,"calibrated interior MG edge gain is only {edge_db:.3}dB");
        for key in ["receiver_ear_laf_dba","muzzle_ear_laf_dba","rounds","event_count","muzzle_delay_s"] {
            assert_eq!(old_route[key],new_route[key],"physical route/cadence changed: {key}");
        }
        assert_eq!(new_route["master"]["limited_frames"],0);
        assert!(new.iter().all(|x|x.is_finite()&&x.abs()<0.8));
        assert_eq!(new.len(),3*44_100*2);
    }

    #[test]
    fn local_receiver_cache_preserves_exact_transfer_and_has_no_room_tail() {
        let l=&levels::MG34_INSIDE;
        let ear=crate::sound_propagation::at_ear(l.lpk_db,l.laf_db,l.spectrum,l.a_duration_ms,true);
        let (a,level)=prepared_receiver(19,&ear);
        let (b,again)=prepared_receiver(31,&ear);
        assert!(Arc::ptr_eq(&a,&b));assert_eq!(level,again);
        let mono=acoustics::synth_mg_receiver_round(3701+(19%12)*7919);
        assert_eq!(a.len(),mono.len()*2);
        assert_eq!(&*a,render_for_ear(&mono,&ear,0.0,ReverbParams::DIRECT));
        let mut changed=ear;changed.transfer_db[3]+=0.25;
        let (c,_)=prepared_receiver(19,&changed);assert_ne!(&*a,&*c);
        assert!(a.iter().all(|v|v.is_finite()));
        assert_ne!(&*a,&*prepared_receiver(20,&ear).0);
    }

    /// Isolated source/path audit. It deliberately leaves hearing healthy to distinguish source
    /// timbre and hull transfer from the actual crew's accumulated damage in gameplay captures.
    #[test]
    fn destruction_pressure_path_review() {
        let Ok(prefix) = std::env::var("KESSEL_DESTRUCTION_PATH_REVIEW_PREFIX") else { return; };
        let mut rows = Vec::new();
        for distance_m in [2.0, 22.0] {
            for (path_name, enclosure, headset) in [
                ("same_compartment", Enclosure::OnFoot, true),
                ("through_hull", Enclosure::Buttoned, true),
                ("observer", Enclosure::OnFoot, false),
            ] {
                for (name, sfx, emission) in [
                    ("armour", Sfx::ImpactTank, Emission::from_level(&levels::EXTERIOR_ARMOUR_STRIKE_10M)),
                    ("kill", Sfx::KillExplosion, Emission::blast(levels::VEHICLE_KILL_TNT_KG)),
                    ("catastrophic", Sfx::TurretPop, Emission::blast(levels::AMMUNITION_DETONATION_TNT_KG)),
                ] {
                    let audio = Audio::new_capture();
                    let path = Path { distance_m, atmosphere: Atmosphere::WINTER, occluded: false,
                        enclosure, headset };
                    let ear = propagate(&emission, &path);
                    let reverb = if path_name == "observer" { ReverbParams::OPEN_SNOW }
                        else { ReverbParams::STEEL_INTERIOR };
                    audio.play_world_at_ear(sfx, &ear, 0.0, reverb);
                    let mut out = audio.render_capture_frames(3 * 44_100).unwrap();
                    out.drain(..reproduction::MASTER_LATENCY_FRAMES * 2);
                    assert!(out.iter().all(|v| v.is_finite()));
                    let file = format!("{prefix}_{name}_{path_name}_{distance_m:.0}m.wav");
                    assert!(!std::path::Path::new(&file).exists(), "preserve review: {file}");
                    acoustics::write_wav_stereo(&file, &out, acoustics::SAMPLE_RATE).unwrap();
                    let master = audio.master_telemetry();
                    rows.push(serde_json::json!({"source":name,"path":path_name,"distance_m":distance_m,
                        "ear_laf_dba":ear.laf_db,"ear_lpk_db":ear.lpk_db,"transfer_db":ear.transfer_db,
                        "output_laf_dbfs":laf_max_dbfs(&out),"output_peak_dbfs":reproduction::peak_dbfs(&out),
                        "master":master,"wav":file}));
                }
            }
        }
        let file = format!("{prefix}.json");
        assert!(!std::path::Path::new(&file).exists(), "preserve review: {file}");
        std::fs::write(file, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
    }

    #[test]
    #[ignore = "timing diagnostic"]
    fn print_world_render_cost() {
        let audio = Audio::new_capture();
        let mono = audio.world_source(Sfx::KillExplosion).unwrap();
        let ear = propagate(&Emission::blast(0.71), &Path { distance_m: 400.0,
            atmosphere: Atmosphere::WINTER, occluded: true, enclosure: Enclosure::Buttoned,
            headset: true });
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let rendered = render_for_ear(&mono, &ear, 40.0, ReverbParams::STEEL_INTERIOR);
            let _ = measure_render_window(&rendered, 2000);
        }
        println!("render+measure {:.2} ms per event, {} frames",
            start.elapsed().as_secs_f64() * 100.0, mono.len());
        let _ = levels::TABLE.len();
    }

    /// The own gun at the ear through the headset, as the game plays it.
    fn own_gun_ear(level: &levels::SourceLevel) -> f32 {
        propagate(&Emission::from_level(level), &Path::at_ear(true)).laf_db
    }

    #[test]
    fn historical_reproduction_reference_crest_matches_its_documented_constant() {
        let audio = Audio::new_capture();
        let Some(mono) = audio.feedback_source("gun_interior") else {
            eprintln!("verified bank absent: crest check skipped");
            return;
        };
        let stereo: Vec<f32> = mono.iter().flat_map(|v| [*v, *v]).collect();
        let crest = reproduction::peak_dbfs(&stereo) - laf_max_dbfs(&stereo);
        assert!((crest - reproduction::CREST_GUN_INTERIOR_DB).abs() < 0.05,
            "measured crest {crest} vs constant {}", reproduction::CREST_GUN_INTERIOR_DB);
    }

    #[test]
    fn own_gun_active_variants_match_the_calibrated_float_chain_and_master() {
        let frames=3*crate::acoustics::SAMPLE_RATE as usize;
        for (sounds,level) in [
            (&Sfx::PLAYER_GUNS_INT[..],&levels::OWN_88_BUTTONED),
            (&Sfx::PLAYER_GUNS_INT[..],&levels::OWN_88_HATCH_OPEN),
            (&Sfx::PLAYER_GUNS_EXT[..],&levels::OWN_88_HEAD_OUT),
        ] {
            for &sfx in sounds {
                let audio=Audio::new_capture();
                let waveform=audio.direct_waveform(sfx).expect("active gun route");
                let ear=own_gun_ear(level);
                let gain=reproduction::voice_gain(ear,laf_max_dbfs(&waveform));
                let crest=reproduction::peak_dbfs(&waveform)-laf_max_dbfs(&waveform);
                let mut placed:Vec<f32>=waveform.iter().map(|x|x*gain).collect();
                placed.resize(frames*2,0.0);
                let (expected,expected_limiting)=reproduction::master_offline(&placed);
                audio.play_at_ear(sfx,ear);
                let actual=audio.render_capture_frames(frames+reproduction::MASTER_LATENCY_FRAMES).unwrap();
                let actual=&actual[reproduction::MASTER_LATENCY_FRAMES*2..];
                let maximum_error=actual.iter().zip(&expected).map(|(a,b)|(a-b).abs()).fold(0.0f32,f32::max);
                assert!(maximum_error<2e-5,"{sfx:?}: live source differs from calibrated float/master route by {maximum_error}");
                let limiting=audio.master_telemetry()["deepest_gain_reduction_db"].as_f64().unwrap() as f32;
                assert!((limiting-expected_limiting).abs()<0.001);
                assert!(reproduction::peak_dbfs(actual)<=20.0*reproduction::LIMITER_CEILING.log10()+0.01);
                eprintln!("[active-gun-calibration] {sfx:?} ear={ear:.2} crest={crest:.3} target_LAF={:.3} measured_LAF={:.3} master={limiting:.3}dB",
                    reproduction::tone_map_dbfs(ear),laf_max_dbfs(actual));
            }
        }
    }

    #[test]
    fn the_engine_bed_lands_at_its_ear_level_at_any_revs_with_no_cap() {
        use crate::audio::EngineTelemetry;
        let level = |telemetry: EngineTelemetry, ear: f32| {
            let audio = Audio::new_capture();
            for _ in 0..240 {
                audio.set_engine_at_ear(telemetry, ear, 1.0 / 60.0);
                audio.render_capture_frames(735).unwrap();
            }
            let mut out = Vec::new();
            for _ in 0..120 {
                audio.set_engine_at_ear(telemetry, ear, 1.0 / 60.0);
                out.extend(audio.render_capture_frames(735).unwrap());
            }
            let mut meter = reproduction::LafMeter::new(reproduction::SAMPLE_RATE);
            let mut sum = 0.0f64;
            for f in out.chunks_exact(2) { sum += 10f64.powf(f64::from(meter.push(f[0], f[1])) / 10.0); }
            (10.0 * (sum / (out.len() / 2) as f64).log10()) as f32
        };
        let idle = EngineTelemetry::automatic(0.0, 0.0, 0.0, true);
        let road = EngineTelemetry::automatic(0.8, 0.9, 0.5, true);
        for telemetry in [idle, road] {
            for ear in [93.0, 110.0, 115.0] {
                let got = level(telemetry, ear);
                let want = reproduction::tone_map_dbfs(ear);
                assert!((got - want).abs() < 2.0, "rpm {} at {ear} dB(A): {got} vs {want}",
                    telemetry.rpm_norm);
            }
        }
        // The own gun sits ~15 dB (LAF) above the running bed (110 dB(A) cross-country), 12 dB
        // above the road bed (115); physically 29 and 24 dB (SL §11 rule 3).
        let gun = reproduction::tone_map_dbfs(levels::OWN_88_BUTTONED.laf_db);
        let bed = reproduction::tone_map_dbfs(levels::TIGER_INTERIOR_CROSS_COUNTRY.laf_db);
        assert!((gun - bed - 14.5).abs() < 0.01);
        let road = reproduction::tone_map_dbfs(levels::TIGER_INTERIOR_ROAD.laf_db);
        assert!((gun - road - 12.0).abs() < 0.01);
    }

    #[test]
    fn world_voices_are_placed_by_their_ear_level_not_their_waveform() {
        // Two different waveforms at the same ear level play at the same short-term level; the
        // same waveform 20 dB quieter at the ear follows the selected global reproduction curve.
        let ear_at = |laf: f32| {
            let mut ear = propagate(&Emission::from_level(&levels::GUN_76_F34_SIDE_10M),
                &Path::at_ear(false));
            ear.laf_db = laf;
            ear
        };
        let level = |sfx: Sfx, laf: f32| {
            let audio = Audio::new_capture();
            audio.play_world_at_ear(sfx, &ear_at(laf), 0.0, ReverbParams::OPEN_SNOW);
            let out = audio.render_capture_frames(3 * 44_100).unwrap();
            laf_max_dbfs(&out)
        };
        let gun = level(Sfx::EnemyGun, 100.0);
        let ground = level(Sfx::ImpactGround, 100.0);
        assert!((gun - ground).abs() < 0.5, "same ear level, same digital level: {gun} {ground}");
        assert!((gun - reproduction::tone_map_dbfs(100.0)).abs() < 0.5, "{gun}");
        let quieter = level(Sfx::EnemyGun, 80.0);
        let expected = reproduction::tone_map_dbfs(100.0) - reproduction::tone_map_dbfs(80.0);
        assert!((gun - quieter - expected).abs() < 0.5, "{gun} {quieter}, expected {expected}");
    }

    #[test]
    fn small_arm_variants_keep_calibration_and_buttoned_hull_filters_the_crack() {
        let audio = Audio::new_capture();
        let emission = Emission::from_level(&levels::RIFLE_1M_SIDE);
        let path = Path { distance_m:30.0, atmosphere:Atmosphere::WINTER, occluded:false,
            enclosure:Enclosure::OnFoot, headset:false };
        let open = propagate(&emission,&path);
        let closed = propagate(&emission,&Path { enclosure:Enclosure::Buttoned, headset:true, ..path });
        assert!(open.laf_db-closed.laf_db > 35.0);
        assert!(closed.transfer_db[7]-closed.transfer_db[2] > 25.0);
        let mut fingerprints = std::collections::HashSet::new();
        for _ in 0..12 {
            let mono=audio.world_source(Sfx::RifleShot).unwrap();
            let key=format!("world:RifleShot:{:016x}",super::super::buffer_checksum(&mono));
            assert!(fingerprints.insert(key.clone()));
            let measured=audio.measured_world_source(key,&mono);
            let rendered=render_for_ear(&mono,&open,22.0,ReverbParams::OPEN_SNOW);
            let level=measure_render_window(&rendered,measured.max_frame);
            let gain=reproduction::voice_gain(open.laf_db,level);
            let placed:Vec<_>=rendered.iter().map(|x|x*gain).collect();
            assert!((laf_max_dbfs(&placed)-reproduction::tone_map_dbfs(open.laf_db)).abs()<0.15);
        }
        assert_eq!(audio.level_cache.borrow().len(),12,"same-duration variants must not share a stale measurement");
    }
}

/// THE OWN GUN, BUTTONED UP, FITTED TO THE OWNER'S LISTENING REFERENCES (2026-10-04).
///
/// Measured with `tools/audio_reference_bands.py` on `listening_reference_main_gun_firing_inside`
/// and `listening_reference_main_gun_and_reload_perfect` (the shot's octave bands with each
/// clip's own background removed, averaged): a roar strongest at 31.5 to 63 Hz that falls about
/// 3 dB an octave, `[-0.8, -2.2, -6.7, -11.6, -11.3, -11.3, -14.3, -17.7, -22.0]` dB, and holds
/// within 4 dB of its peak for 0.7 s. Each layer's loss below is (its recording's measured
/// shape) minus (that reference shape), so the layer comes out with the reference's balance.
/// These replace the hull-and-headset path for this one sound: the reference is what is heard at
/// the ear in the cabin. OWNER-DIRECTED PRESENTATION, not a measured hull transmission.
/// The tables were then corrected once by the measured error of the rendered result
/// (`cabin_gun_capture_for_reference_measurement`), since the band filter and the tail colour it.
pub(crate) const CABIN_GUN_MAIN_LOSS_DB: crate::sound_propagation::Bands = CABIN_GUN_FIT[0];
pub(crate) const CABIN_GUN_OUTSIDE_LOSS_DB: crate::sound_propagation::Bands = CABIN_GUN_FIT[1];
pub(crate) const CABIN_GUN_INSIDE_LOSS_DB: crate::sound_propagation::Bands = CABIN_GUN_FIT[2];
const CABIN_GUN_FIT: [crate::sound_propagation::Bands; 3] = [
    [18.2, 29.4, 32.1, 36.5, 30.1, 27.0, 19.0, 4.0, 0.0],
    [13.6, 34.2, 39.7, 42.3, 34.0, 30.1, 24.0, 10.0, 0.0],
    [14.4, 28.1, 32.3, 35.7, 35.2, 31.9, 23.8, 8.2, 0.0],
];
/// The cabin holds the shot: a long tail under the reference's sustained roar.
pub(crate) const CABIN_GUN_REVERB: ReverbParams = ReverbParams { rt60_s: 1.7, wet: 0.5, damping: 0.20 };
/// The reference roar holds near its peak for this long; the recordings fall away in a third of
/// that. The hold is made of the recording itself, repeated densely over this span.
pub(crate) const CABIN_GUN_HOLD_S: f32 = 0.90;

/// Hold a recording's body for `hold_s`: many copies of it at scattered delays inside that span,
/// with scattered signs, on top of the original. The attack stays where it was; what follows is
/// the same sound sustained, as a closed steel box sustains a blast. Deterministic.
pub(crate) fn sustain_roar(mono: &[f32], hold_s: f32) -> Vec<f32> {
    const TAPS: u32 = 56;
    let span = (hold_s.max(0.0) * reproduction::SAMPLE_RATE as f32) as usize;
    if mono.is_empty() || span == 0 { return mono.to_vec(); }
    let mut out = vec![0.0_f32; mono.len() + span];
    out[..mono.len()].copy_from_slice(mono);
    // Strong enough that the held body sits just under the attack, as in the reference.
    let gain = 1.7 / (TAPS as f32).sqrt();
    for tap in 0..TAPS {
        let hash = tap.wrapping_mul(2654435761).rotate_left(13) ^ 0x9E37_79B9;
        // Later copies are a little weaker, so the hold sags gently and never swells.
        let place = (tap as f32 + 0.5 + ((hash >> 8) % 1000) as f32 / 1000.0 - 0.5) / TAPS as f32;
        let delay = ((place * span as f32) as usize).max(64);
        let sign = if hash & 1 == 0 { 1.0 } else { -1.0 };
        let weight = gain * sign * (1.0 - 0.45 * place);
        for (i, x) in mono.iter().enumerate() { out[i + delay] += x * weight; }
    }
    out
}

impl Audio {
    /// The dry world source for `sfx`: the verified recording where one replaces it, otherwise the
    /// deterministic procedural identity, with an opted-in v1 character layer mixed in at its
    /// receipt-bound share (so the layer is inside the calibrated level, not added on top).
    pub(super) fn world_source(&self, sfx: Sfx) -> Option<Arc<[f32]>> {
        if let Some(report) = self.small_arm_reports.next(sfx) { return Some(report); }
        let base = super::feedback::replacement_key(sfx)
            .and_then(|key| self.feedback_source(key))
            .or_else(|| self.world_mono.get(sfx.index()).cloned())?;
        let Some(layer) = self.next_pizza_layer(sfx) else { return Some(base) };
        let len = base.len().max(layer.world_mono.len());
        let mixed: Vec<f32> = (0..len)
            .map(|i| base.get(i).copied().unwrap_or(0.0)
                + layer.gain * layer.world_mono.get(i).copied().unwrap_or(0.0))
            .collect();
        Some(Arc::from(mixed))
    }

    fn measured_world_source(&self, key: String, mono: &[f32]) -> MeasuredSource {
        if let Some(found) = self.level_cache.borrow().get(&key) {
            return *found;
        }
        let measured = measure_mono(mono);
        self.level_cache.borrow_mut().insert(key, measured);
        measured
    }

    /// Play a world source at its physical level at the ear. `bearing_deg` is head-relative;
    /// `ear` comes from `sound_propagation::propagate` for the live path.
    pub fn play_world_at_ear(&self, sfx: Sfx, ear: &EarLevel, bearing_deg: f32, reverb: ReverbParams) {
        let Some(mono) = self.world_source(sfx) else { return };
        let key = if super::small_arms::is_variant(sfx) {
            format!("world:{sfx:?}:{:016x}", super::buffer_checksum(&mono))
        } else { format!("world:{sfx:?}:{}", mono.len()) };
        let measured = self.measured_world_source(key, &mono);
        self.place_world_render(&mono, measured, ear, bearing_deg, reverb, EarBus::World);
    }

    /// One layer of the own gun as heard buttoned up: the recording held for the reference's
    /// roar, coloured by `ear.transfer_db`, in the cabin's tail (or dry where `held` is false).
    pub(crate) fn play_cabin_gun_layer(&self, sfx: Sfx, ear: &EarLevel, bearing_deg: f32, held: bool) {
        let Some(mono) = self.world_source(sfx) else { return };
        if held {
            let roar = sustain_roar(&mono, CABIN_GUN_HOLD_S);
            self.play_world_mono_at_ear(&roar, ear, bearing_deg, CABIN_GUN_REVERB);
        } else {
            self.play_world_mono_at_ear(&mono, ear, bearing_deg, ReverbParams::DIRECT);
        }
    }

    /// Play a synthesized mono world voice (a parametric report, a small-arms contact) at its
    /// physical level at the ear.
    pub fn play_world_mono_at_ear(&self, mono: &[f32], ear: &EarLevel, bearing_deg: f32,
        reverb: ReverbParams)
    {
        self.play_world_mono_in_room(mono, ear, bearing_deg, Room::Parametric(reverb));
    }

    /// `play_world_at_ear` in a given room (a traced reflected field).
    pub fn play_world_in_room(&self, sfx: Sfx, ear: &EarLevel, bearing_deg: f32, room: Room<'_>) {
        let Some(mono) = self.world_source(sfx) else { return };
        let key = if super::small_arms::is_variant(sfx) {
            format!("world:{sfx:?}:{:016x}", super::buffer_checksum(&mono))
        } else { format!("world:{sfx:?}:{}", mono.len()) };
        let measured = self.measured_world_source(key, &mono);
        self.place_world_render_in(&mono, measured, ear, bearing_deg, room, EarBus::World);
    }

    /// `play_world_mono_at_ear` in a given room.
    pub fn play_world_mono_in_room(&self, mono: &[f32], ear: &EarLevel, bearing_deg: f32, room: Room<'_>) {
        if mono.is_empty() {
            return;
        }
        let measured = measure_mono(mono);
        self.place_world_render_in(mono, measured, ear, bearing_deg, room, EarBus::World);
    }

    fn place_world_render(&self, mono: &[f32], measured: MeasuredSource, ear: &EarLevel,
        bearing_deg: f32, reverb: ReverbParams, bus: EarBus)
    {
        self.place_world_render_in(mono, measured, ear, bearing_deg, Room::Parametric(reverb), bus);
    }

    fn place_world_render_in(&self, mono: &[f32], measured: MeasuredSource, ear: &EarLevel,
        bearing_deg: f32, room: Room<'_>, bus: EarBus)
    {
        if !ear.laf_db.is_finite() || !measured.laf_dbfs.is_finite() {
            return;
        }
        let rendered = render_for_ear_in(mono, ear, bearing_deg, room);
        let level = measure_render_window(&rendered, measured.max_frame);
        let gain = reproduction::voice_gain(ear.laf_db, level);
        self.play_stereo_on(rendered, gain, bus);
    }

    /// Play an already spatialised stereo buffer (a strike on the listener's own vehicle, a local
    /// contact) at the physical level `ear_laf_dba` on `bus`.
    pub fn play_stereo_at_ear(&self, samples: Vec<f32>, ear_laf_dba: f32, bus: EarBus) {
        if samples.is_empty() {
            return;
        }
        let gain = reproduction::voice_gain(ear_laf_dba, laf_max_dbfs(&samples));
        self.play_stereo_on(samples, gain, bus);
    }

    /// Authored close interior handling reproduction, after physical level calculation.
    /// This preserves source/propagation and passes through hearing, death fade and the shared
    /// limiter. It is not a measured historical sound-pressure adjustment.
    pub(crate) fn play_handling_at_ear(&self, samples: Vec<f32>, ear_laf_dba: f32) {
        if samples.is_empty() { return; }
        let gain = reproduction::voice_gain(ear_laf_dba, laf_max_dbfs(&samples))
            * 10.0_f32.powf(14.0 / 20.0);
        self.play_stereo_on(samples, gain, EarBus::World);
    }

    /// The direct (non-spatial) waveform `play(sfx)` starts: the verified recording where one
    /// replaces it, otherwise the loaded clip.
    pub(crate) fn direct_waveform(&self, sfx: Sfx) -> Option<Vec<f32>> {
        if let Some(mono) = super::feedback::replacement_key(sfx).and_then(|k| self.feedback_source(k)) {
            return Some(mono.iter().flat_map(|v| [*v, *v]).collect());
        }
        let clip = self.clips.get(sfx.index()).and_then(|c| c.as_ref())?;
        // Match the live sink's float conversion, channel conversion and source-rate handling.
        Some(rodio::source::UniformSourceIterator::new(clip.source_f32(),2,
            crate::acoustics::SAMPLE_RATE).collect())
    }

    /// Measured LAF,max (dBFS(A)) of `play(sfx)`'s waveform at unity gain, cached.
    pub fn direct_laf_dbfs(&self, sfx: Sfx) -> f32 {
        let key = format!("direct:{sfx:?}");
        if let Some(found) = self.level_cache.borrow().get(&key) {
            return found.laf_dbfs;
        }
        let laf = self.direct_waveform(sfx).map_or(f32::NEG_INFINITY, |w| laf_max_dbfs(&w));
        self.level_cache.borrow_mut().insert(key, MeasuredSource { laf_dbfs: laf, max_frame: 0 });
        laf
    }

    /// Where `play_at_ear(sfx)`'s sound begins (s after its first sample) and its envelope from
    /// there, cached: the impulse the voice carries reaches the ears when its sound begins (the
    /// verified gun recordings open with ~50 ms of near-silence) and the recording, as heard,
    /// drives the middle-ear reflex.
    pub fn direct_drive(&self, sfx: Sfx) -> Option<(f32, crate::hearing::DriveEnvelope)> {
        let key = format!("direct:{sfx:?}");
        if let Some(found) = self.drive_cache.borrow().get(&key) {
            return Some(found.clone());
        }
        let drive = crate::hearing::DriveEnvelope::of_voice(&self.direct_waveform(sfx)?);
        self.drive_cache.borrow_mut().insert(key, drive.clone());
        Some(drive)
    }

    /// Play `sfx` at the ear, not spatialised (the own gun, the listener's own weapons), at the
    /// physical level `ear_laf_dba`. The same waveform `play` would use, placed by calibration.
    pub fn play_at_ear(&self, sfx: Sfx, ear_laf_dba: f32) {
        let gain = reproduction::voice_gain(ear_laf_dba, self.direct_laf_dbfs(sfx));
        if gain <= 0.0 {
            return;
        }
        if self.play_feedback_sfx(sfx, gain) {
            return;
        }
        self.play_at_volume_without_layers(sfx, gain);
    }

    /// Measured LAF,max (dBFS(A)) of an interleaved stereo waveform, cached under `key`.
    pub(super) fn cached_laf_dbfs(&self, key: &str, waveform: impl FnOnce() -> Vec<f32>) -> f32 {
        if let Some(found) = self.level_cache.borrow().get(key) {
            return found.laf_dbfs;
        }
        let laf = laf_max_dbfs(&waveform());
        self.level_cache.borrow_mut()
            .insert(key.to_string(),
                MeasuredSource { laf_dbfs: laf, max_frame: 0 });
        laf
    }

    /// Linear gain placing a loaded loop/clip at `ear_laf_dba`.
    pub(super) fn clip_gain(&self, key: &str, clip: &super::Clip, ear_laf_dba: f32) -> f32 {
        let laf = self.cached_laf_dbfs(key, || {
            rodio::source::UniformSourceIterator::new(clip.source_f32(),2,
                crate::acoustics::SAMPLE_RATE).collect()
        });
        reproduction::voice_gain(ear_laf_dba, laf)
    }

    /// Diagnostics: the master stage's limiter telemetry.
    pub fn master_telemetry(&self) -> serde_json::Value {
        let mut report=self.master.snapshot();
        report["live_delivery"]=self.live_output_telemetry();
        report
    }

    /// Frames of latency the master stage adds (the limiter's lookahead).
    #[cfg(test)]
    pub fn master_latency_frames(&self) -> usize {
        reproduction::MASTER_LATENCY_FRAMES
    }
}
