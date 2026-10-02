//! Dry small-arms source reports. Muzzle report, ballistic shock and terminal contact remain
//! separate events. These authored timbres do not choose their level, propagation or timing.
use std::{cell::Cell, sync::Arc};
use super::Sfx;

const SR: f32 = crate::acoustics::SAMPLE_RATE as f32;
const VARIANTS: usize = 12;
const COMBAT_VARIANTS: usize = 4;
const COMBAT: [Sfx; 8] = [Sfx::EnemyGun,Sfx::ImpactGround,Sfx::ImpactTank,Sfx::KillExplosion,
    Sfx::TurretPop,Sfx::TurretRip,Sfx::CookOff,Sfx::PenetrationHit];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Report { Rifle, MachineGun }

fn identity(sfx: Sfx) -> Option<Report> {
    match sfx { Sfx::RifleShot => Some(Report::Rifle), Sfx::TankMgShot => Some(Report::MachineGun), _ => None }
}

fn random(seed: u32) -> f32 {
    let mut x = seed.wrapping_add(0x9e37_79b9);
    x = (x ^ (x >> 16)).wrapping_mul(0x85eb_ca6b);
    x = (x ^ (x >> 13)).wrapping_mul(0xc2b2_ae35);
    ((x ^ (x >> 16)) >> 8) as f32 / 16_777_216.0
}

/// Select the beginning of the verified report, excluding leading silence and its long tail.
/// Early room reflections can remain in a crop; this is explicitly not de-reverberation.
fn recording_onset(recording: &[f32]) -> Option<usize> {
    let peak = recording.iter().copied().filter(|x| x.is_finite())
        .map(f32::abs).fold(0.0f32, f32::max);
    if peak < 1.0e-6 { return None; }
    recording.iter().position(|x| x.is_finite() && x.abs() >= peak * 0.04)
        .map(|i| i.saturating_sub((SR * 0.0003) as usize))
}

/// A single causal pressure pulse and damped broad resonances, with optional early owned source.
/// There is no sustained white-noise bed, 90Hz cannon-like oscillator or built-in room tail.
pub(crate) fn dry_report(kind: Report, seed: u32, recording: Option<&[f32]>) -> Vec<f32> {
    let mg = kind == Report::MachineGun;
    let duration = if mg { 0.090 } else { 0.145 };
    let mut mono = vec![0.0; (duration * SR) as usize];
    let detune = 0.97 + 0.06 * random(seed);
    let pulse_width = (if mg { 0.0010 } else { 0.00135 }) * (0.90 + 0.20 * random(seed ^ 73));
    let modes = if mg { [510.0, 1180.0, 2540.0] } else { [395.0, 970.0, 2140.0] };
    for (i, out) in mono.iter_mut().enumerate() {
        let t = i as f32 / SR;
        // Smooth zero-onset bipolar pressure shape: its short rarefaction is part of the muzzle
        // report, not a duplicate in-flight N-wave. Modal phases begin at the same causal onset.
        let u = t / pulse_width;
        let pressure = 2.6 * u * (1.0 - 0.55 * u) * (-u).exp();
        let mut body = 0.0;
        for (j, hz) in modes.iter().enumerate() {
            let tau = (if mg { 0.009 } else { 0.016 }) / (1.0 + j as f32 * 0.75);
            let hz = hz * detune * (0.985 + 0.03 * random(seed ^ (j as u32 + 1) * 977));
            body += (std::f32::consts::TAU * hz * t).sin() * (-t / tau).exp()
                * (1.0 - (-t / 0.00018).exp()) * [0.22, 0.16, 0.11][j];
        }
        *out = pressure + body;
    }
    if let Some((source, start)) = recording.and_then(|r| recording_onset(r).map(|s| (r, s))) {
        let crop_s = if mg { 0.060 } else { 0.110 };
        let rate = detune * if mg { 1.025 } else { 1.0 };
        let crop = (crop_s * SR) as usize;
        let source_peak = source[start..(start + crop + 256).min(source.len())]
            .iter().map(|x| x.abs()).fold(1.0e-6f32, f32::max);
        for (i, out) in mono.iter_mut().take(crop).enumerate() {
            let p = start as f32 + i as f32 * rate;
            let j = p as usize;
            if j + 1 >= source.len() { break; }
            let f = p - j as f32;
            let sample = source[j] * (1.0 - f) + source[j + 1] * f;
            let t = i as f32 / SR;
            let fade = ((crop_s - t) / (crop_s * 0.30)).clamp(0.0, 1.0);
            let attack = (t / 0.00020).clamp(0.0, 1.0);
            *out = *out * 0.20 + sample / source_peak * 0.80 * fade * fade * attack;
        }
        // Do not let the synthesized component jump back up at the recording crop boundary.
        for out in &mut mono[crop..] { *out *= 0.20; }
    }
    // Source shaping only. One live propagation/reverb path and calibrated ear gain follow.
    // Two DC-blocking poles remove rumble from old recordings without creating a bass shelf.
    let alpha = (-std::f32::consts::TAU * 155.0 / SR).exp();
    let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for out in &mut mono {
        let a = alpha * (y1 + *out - x1); x1 = *out; y1 = a;
        let b = alpha * (y2 + a - x2); x2 = a; y2 = b;
        *out = if b.is_finite() { b } else { 0.0 };
    }
    let fade = (0.008 * SR) as usize;
    let n = mono.len();
    for (i, x) in mono[n-fade..].iter_mut().enumerate() { *x *= 1.0 - i as f32 / (fade-1) as f32; }
    let peak = mono.iter().map(|x| x.abs()).fold(1.0e-6f32, f32::max);
    for x in &mut mono { *x *= 0.85 / peak; }
    mono
}

pub(super) struct SourceBank {
    reports: [Vec<Arc<[f32]>>; 2],
    counters: [Cell<usize>; 2],
    recorded: bool,
    combat: Vec<(Sfx, Vec<Arc<[f32]>>, Cell<usize>)>,
}

impl SourceBank {
    pub(super) fn new(recording: Option<&[f32]>) -> Self {
        let recorded = recording.and_then(recording_onset).is_some();
        Self {
            reports: [Report::Rifle, Report::MachineGun].map(|kind| (0..VARIANTS)
                .map(|i| Arc::from(dry_report(kind, 3701 + i as u32 * 7919, recording))).collect()),
            counters: [Cell::new(0), Cell::new(0)], recorded,
            combat:COMBAT.into_iter().map(|sfx|(sfx,(0..COMBAT_VARIANTS)
                .map(|i|Arc::from(super::stereo_source_to_mono(
                    &crate::acoustics::synth_procedural_dry_variant(sfx.procedural(),i as u32))))
                .collect(),Cell::new(0))).collect(),
        }
    }
    pub(super) fn next(&self, sfx: Sfx) -> Option<Arc<[f32]>> {
        if let Some((_,voices,index))=self.combat.iter().find(|(key,_,_)|*key==sfx) {
            let at=index.get(); index.set((at+1)%voices.len());
            return Some(voices[at].clone());
        }
        let index = match identity(sfx)? { Report::Rifle => 0, Report::MachineGun => 1 };
        let next = self.counters[index].get();
        self.counters[index].set((next + 1) % VARIANTS);
        Some(self.reports[index][next].clone())
    }
    pub(super) fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "schema":"kessel.small-arms-source", "version":1,
            "source":if self.recorded {"verified_rifle_early_crop_and_pressure_synthesis"} else {"pressure_and_modal_synthesis"},
            "verified_recording_key":if self.recorded {Some("rifle_shot")} else {None},
            "recording_crop_seconds":[0.110,0.060], "dry_report_seconds":[0.145,0.090],
            "variants_per_family":VARIANTS,"order":["rifle_shot","tank_mg_shot"],
            "prepared_pcm_fnv1a64":self.reports.iter().map(|bank| bank.iter()
                .map(|s|format!("{:016x}",super::buffer_checksum(s))).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "retained_early_reflections_possible":self.recorded,
            "embedded_reverb":false,"live_environment_passes":1,"white_noise_bed":false,
            "calibration":"existing physical emission and ear calibration; timbres are authored estimates",
            "generic_combat_variants":self.combat.iter().map(|(sfx,voices,_)|serde_json::json!({
                "sfx":sfx.slug(),"variants":voices.len(),"source":"seeded_dry_procedural",
                "prepared_pcm_fnv1a64":voices.iter().map(|v|format!("{:016x}",super::buffer_checksum(v))).collect::<Vec<_>>(),
                "time_stretch":false,"embedded_reverb":false,
            })).collect::<Vec<_>>(),
        })
    }
}

pub(super) fn is_report(sfx: Sfx) -> bool { identity(sfx).is_some() }
pub(super) fn is_combat_variant(sfx:Sfx)->bool { COMBAT.contains(&sfx) }
pub(super) fn is_variant(sfx:Sfx)->bool {is_report(sfx)||is_combat_variant(sfx)}

#[cfg(test)]
mod tests {
    use super::*;
    fn energy(x: &[f32]) -> f32 { x.iter().map(|v| v*v).sum() }
    #[test]
    fn dry_reports_are_short_causal_distinct_and_have_no_sustained_noise() {
        let rifle=dry_report(Report::Rifle,73,None); let mg=dry_report(Report::MachineGun,73,None);
        for report in [&rifle,&mg] {
            assert_eq!(report[0],0.0); assert_eq!(*report.last().unwrap(),0.0);
            assert!(report.iter().all(|x|x.is_finite() && x.abs()<=0.851));
            assert!(energy(&report[(0.060*SR) as usize..]) < energy(report)*0.0005);
        }
        assert_eq!(rifle,dry_report(Report::Rifle,73,None));
        assert_ne!(rifle,dry_report(Report::Rifle,74,None));
        assert_ne!(&rifle[..mg.len()],mg.as_slice());
        assert_eq!(rifle.len(),(0.145*SR) as usize); assert_eq!(mg.len(),(0.090*SR) as usize);
    }
    #[test]
    fn recording_leading_silence_and_late_tail_do_not_delay_or_extend_report() {
        let mut recording=vec![0.0;(SR*1.5) as usize];
        let onset=(SR*0.030) as usize;
        for (i,x) in recording[onset..].iter_mut().enumerate() {
            *x=(std::f32::consts::TAU*1900.0*i as f32/SR).sin();
        }
        let mut changed=recording.clone(); changed[(SR*0.20) as usize..].fill(-0.9);
        let a=dry_report(Report::Rifle,8,Some(&recording));
        assert_eq!(a,dry_report(Report::Rifle,8,Some(&changed)));
        assert!(a[..(SR*0.005) as usize].iter().any(|x|x.abs()>0.1));
        assert!(energy(&a[(SR*0.115) as usize..]) < energy(&a)*0.0001);
    }
    #[test]
    fn report_bank_cycles_reproducibly_without_sharing_weapon_counters() {
        let bank=SourceBank::new(None); let first=bank.next(Sfx::RifleShot).unwrap();
        let mg=bank.next(Sfx::TankMgShot).unwrap();
        assert_ne!(&*first,&*bank.next(Sfx::RifleShot).unwrap());
        for _ in 2..VARIANTS { bank.next(Sfx::RifleShot); }
        assert_eq!(&*first,&*bank.next(Sfx::RifleShot).unwrap());
        assert_ne!(&*mg,&*bank.next(Sfx::TankMgShot).unwrap());
        assert!(bank.next(Sfx::HammerStrike).is_none());
    }

    #[test]
    fn generic_combat_variation_retains_onset_and_original_variant_without_resampling() {
        let bank=SourceBank::new(None);
        for sfx in COMBAT {
            let first=bank.next(sfx).unwrap();
            assert_eq!(&*first,&*super::super::procedural_world_mono(sfx));
            let start=first.iter().position(|x|x.abs()>0.00001).unwrap();
            let mut hashes=std::collections::HashSet::from([super::super::buffer_checksum(&first)]);
            for _ in 1..COMBAT_VARIANTS {
                let next=bank.next(sfx).unwrap();
                assert!(next.iter().all(|x|x.is_finite()));
                assert_eq!(next.len(),first.len());
                let onset=next.iter().position(|x|x.abs()>0.00001).unwrap();
                assert!(start.abs_diff(onset)<(SR*0.001) as usize,"{sfx:?} onset {start}/{onset}");
                assert!(hashes.insert(super::super::buffer_checksum(&next)),"{sfx:?} repeats");
            }
            assert_eq!(&*first,&*bank.next(sfx).unwrap());
        }
    }
}
