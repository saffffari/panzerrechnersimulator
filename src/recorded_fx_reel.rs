//! `recorded-fx-reel <outdir>`: CPU-only renders of the owner's library recordings
//! (`audio::recorded_fx`) through the live capture graph at their 1 m reference levels, placed as
//! the game places them (`propagate` from the event's distance, the hull and headset where the
//! listener is buttoned, the bearing, the room). Three reels: every family once (handling, breech,
//! trigger, switches, kit, wounds, footsteps by surface); a tank hit per component class (the
//! strike's recording with its material layer: optics glass, running gear, a broken track, the
//! radio's spark, a crewman's wound); and a tank crushing through mixed debris (wood, metal,
//! glass, a wall) under the per-update voice cap. A JSON receipt lists each segment.
use super::*;
use crate::audio::{Audio, HitFamily, RecordedBodyContact as C, RecordedFx as Fx, RecordedMaterial as M, RecordedStepSurface as S};
use crate::sound_propagation::{self as prop, Atmosphere, Enclosure, Path};

const SR: usize = crate::acoustics::SAMPLE_RATE as usize;
const TICKS: usize = 60;

struct Event { t: f32, fx: Fx, offset_db: f32, distance_m: f32, bearing: f32, enclosure: Enclosure, label: String }

fn emission(laf: f32) -> prop::Emission {
    prop::Emission { lpk_db: laf + 22.0, laf_db: laf, ref_m: 1.0, law: crate::sound_levels::DistanceLaw::Spherical,
        class: crate::sound_levels::SpectrumClass::Mechanical,
        spectrum: prop::class_spectrum(crate::sound_levels::SpectrumClass::Mechanical), a_duration_ms: 1.0, tnt_kg: 0.0, directivity_db: 0.0 }
}

fn reverb(enclosure: Enclosure) -> crate::acoustics::ReverbParams {
    if matches!(enclosure, Enclosure::OnFoot) && false { crate::acoustics::ReverbParams::OPEN_SNOW }
    else if matches!(enclosure, Enclosure::OnFoot | Enclosure::HeadOut) { crate::acoustics::ReverbParams::OPEN_SNOW }
    else { crate::acoustics::ReverbParams::STEEL_INTERIOR }
}

/// Render `events` (and optional tank-hit voices at times) into one stereo buffer.
fn render(events: &[Event], hits: &[(f32, HitFamily, f32, f32)], seconds: f32) -> anyhow::Result<(Vec<f32>, Vec<serde_json::Value>)> {
    let audio = Audio::new_capture();
    let mut log = Vec::new();
    let mut pcm = Vec::new();
    let mut seed = 1u32;
    for tick in 0..(seconds * TICKS as f32) as usize {
        let t0 = tick as f32 / TICKS as f32;
        let t1 = (tick + 1) as f32 / TICKS as f32;
        audio.recorded_fx_begin_update();
        for (t, family, hit_laf, bearing) in hits.iter().filter(|h| h.0 >= t0 && h.0 < t1) {
            seed = seed.wrapping_add(7);
            if let Some(v) = audio.hit_voice(*family, seed) {
                // Own hull, buttoned: the calibrated strike level at the ear, the steel cabin.
                let ear = prop::at_ear(hit_laf + 20.0, hit_laf + v.level_shift_db, crate::sound_levels::SpectrumClass::LowFrequencyBlast, 5.0, true);
                let stereo = audio.render_world_mono_for_ear(&v.mono, &ear, *bearing,
                    crate::audio::Room::Parametric(crate::acoustics::ReverbParams::STEEL_INTERIOR));
                audio.play_stereo_at_ear(stereo, ear.laf_db, crate::audio::EarBus::Impact);
                log.push(serde_json::json!({"t": t, "hit": v.file, "variant": v.variant}));
            }
        }
        for e in events.iter().filter(|e| e.t >= t0 && e.t < t1) {
            seed = seed.wrapping_add(13);
            match audio.recorded_fx(e.fx, seed) {
                Some(p) => {
                    let path = Path { distance_m: e.distance_m, atmosphere: Atmosphere::WINTER, occluded: false,
                        enclosure: if matches!(e.enclosure, Enclosure::HeadOut) { Enclosure::OnFoot } else if e.distance_m < 3.0 && !matches!(e.enclosure, Enclosure::OnFoot) { Enclosure::OnFoot } else { e.enclosure },
                        headset: !matches!(e.enclosure, Enclosure::OnFoot) };
                    let ear = prop::propagate(&emission(e.fx.ref_dba_1m() + e.offset_db), &path);
                    audio.play_world_mono_in_room(&p.mono, &ear, e.bearing, crate::audio::Room::Parametric(reverb(e.enclosure)));
                    log.push(serde_json::json!({"t": e.t, "label": e.label, "fx": e.fx.name(), "file": p.file, "variant": p.variant,
                        "level_1m_dba": e.fx.ref_dba_1m() + e.offset_db, "distance_m": e.distance_m, "ear_laf_dba": ear.laf_db}));
                }
                None => log.push(serde_json::json!({"t": e.t, "label": e.label, "fx": e.fx.name(), "dropped": "not installed or voice cap"})),
            }
        }
        pcm.extend(audio.render_capture_frames(SR / TICKS).ok_or_else(|| anyhow::anyhow!("capture"))?);
    }
    Ok((pcm, log))
}

pub(crate) fn run(outdir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    let cabin = Enclosure::OnFoot; // a source beside the listener in his own compartment
    let ev = |t: f32, fx: Fx, offset_db: f32, distance_m: f32, bearing: f32, enclosure: Enclosure, label: &str| Event {
        t, fx, offset_db, distance_m, bearing, enclosure, label: label.into() };
    // ── Reel 1: every family once, 0.9 s apart, at a handling distance.
    let mut catalogue = Vec::new();
    let families: Vec<(Fx, &str, f32)> = vec![
        (Fx::ItemPickup, "item picked up", 0.5), (Fx::EquipDrop, "equipment dropped", 1.2), (Fx::EquipDrop, "set down (-6 dB)", 1.0),
        (Fx::ItemEquip, "gear equipped / stowed", 0.5), (Fx::ShellPickup, "88 mm round lifted from the rack", 0.6),
        (Fx::BreechRam, "round rammed (strike = seat)", 0.8), (Fx::BreechClose, "lever pull, breech closes", 0.8),
        (Fx::BreechExtract, "round / case drawn out", 0.8), (Fx::TriggerDry, "main-gun trigger, no shot", 0.6),
        (Fx::LugerRack, "Luger toggle racked", 0.5), (Fx::SwitchButton, "cabin switch", 0.6),
        (Fx::BagOpen, "first-aid bag opened", 0.6), (Fx::MedkitUse, "first-aid step", 0.5), (Fx::BagClose, "bag closed", 0.6),
        (Fx::PaperPage, "order sheet / map", 0.5), (Fx::CrateOpen, "crate opened (no hook yet)", 1.0), (Fx::CrateClose, "crate closed (no hook yet)", 1.0),
        (Fx::FragmentPing, "fragment pings off steel", 1.2), (Fx::ElectricSpark, "radio shorted", 1.2),
    ];
    let mut t = 0.3;
    for (fx, label, d) in &families {
        let off = if label.contains("-6 dB") { -6.0 } else { 0.0 };
        catalogue.push(ev(t, *fx, off, *d, 30.0, cabin, label));
        t += 1.1;
    }
    for s in [S::Snow, S::Dirt, S::Grass, S::Gravel, S::Concrete, S::Wood, S::Metal] {
        for k in 0..4 { catalogue.push(ev(t + k as f32 * 0.5, Fx::Footstep(s), 0.0, 1.6, if k % 2 == 0 { 350.0 } else { 10.0 }, cabin, &format!("footsteps {s:?}"))); }
        t += 2.4;
    }
    for (fx, label) in [(Fx::WoundPenetrating, "wound: fragment, light"), (Fx::WoundSharpLight, "wound: fragment, moderate"),
        (Fx::WoundSharpHeavy, "wound: fragment, heavy"), (Fx::WoundCritical, "wound: fatal fragment"),
        (Fx::WoundBluntLight, "wound: blast, light"), (Fx::WoundBluntModerate, "wound: blast, moderate"),
        (Fx::WoundBluntHeavy, "wound: blast, heavy"), (Fx::BodyContact(C::Metal), "body thrown on steel"),
        (Fx::WoundLacerationHeavy, "laceration (sparingly)"), (Fx::BodyKnock, "a knock, no wound")] {
        catalogue.push(ev(t, fx, 0.0, 1.0, 300.0, cabin, label));
        t += 1.2;
    }
    let (pcm, log1) = render(&catalogue, &[], t + 1.0)?;
    crate::acoustics::write_wav_stereo(format!("{outdir}/recorded_fx_catalogue.wav"), &pcm, SR as u32)?;

    // ── Reel 2: a tank hit per component class (own Tiger, buttoned): strike + material layer.
    let classes: Vec<(&str, HitFamily, Option<(Fx, f32)>, Option<(Fx, f32)>)> = vec![
        ("cupola vision block smashed", HitFamily::Stopped { hardness: 0.55 }, Some((Fx::MaterialBreak(M::Glass), 0.0)), None),
        ("road wheel / running gear", HitFamily::Stopped { hardness: 0.7 }, Some((Fx::MaterialImpact(M::Metal), 0.0)), None),
        ("track broken", HitFamily::Stopped { hardness: 0.85 }, Some((Fx::MaterialBreak(M::Metal), 2.0)), None),
        ("mantlet / mount sheared", HitFamily::Stopped { hardness: 0.95 }, Some((Fx::MaterialBreak(M::Metal), 0.0)), None),
        ("penetration: radio shorted", HitFamily::Penetration, Some((Fx::ElectricSpark, 0.0)), None),
        ("penetration: crewman wounded by fragments", HitFamily::Penetration, Some((Fx::WoundSharpHeavy, 2.0)), None),
        ("penetration: crewman thrown by blast", HitFamily::Penetration, Some((Fx::WoundBluntHeavy, 2.0)), Some((Fx::BodyContact(C::Metal), -2.0))),
    ];
    let mut hits = Vec::new();
    let mut layers = Vec::new();
    let mut t2 = 0.3;
    for (label, family, a, b) in &classes {
        hits.push((t2, *family, 128.0, 300.0));
        for (k, layer) in [a, b].iter().enumerate() {
            if let Some((fx, off)) = layer { layers.push(ev(t2 + 0.02 + k as f32 * 0.03, *fx, *off, 1.2, 300.0, cabin, label)); }
        }
        t2 += 4.2;
    }
    let (pcm2, log2) = render(&layers, &hits, t2 + 0.5)?;
    crate::acoustics::write_wav_stereo(format!("{outdir}/recorded_fx_tank_hit_by_component.wav"), &pcm2, SR as u32)?;

    // ── Reel 3: the Tiger crushing through mixed debris at 3 m/s, head-out, the cap in force.
    let mut crush = Vec::new();
    let mut seed = 0x51u32;
    for i in 0..60 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let pick = seed >> 28;
        let (fx, label) = match pick % 6 { 0 | 1 => (Fx::MaterialImpact(M::Wood), "crate / plank"), 2 => (Fx::MaterialBreak(M::Wood), "fence / log snapped"),
            3 => (Fx::MaterialImpact(M::Metal), "barrel / can"), 4 => (Fx::MaterialImpact(M::Glass), "bottle / pane"), _ => (Fx::MaterialImpact(M::Concrete), "rubble / wall") };
        // Bursts: several contacts can fall in one update; the cap of six per update holds.
        let tt = 0.4 + (i / 4) as f32 * 0.35 + (i % 4) as f32 * 0.004;
        crush.push(ev(tt, fx, -4.0 + (seed >> 24 & 7) as f32, 5.0 + (seed >> 20 & 7) as f32, if i % 2 == 0 { 340.0 } else { 20.0 }, Enclosure::HeadOut, label));
    }
    let (pcm3, log3) = render(&crush, &[], 6.5)?;
    crate::acoustics::write_wav_stereo(format!("{outdir}/recorded_fx_crush_mixed_debris.wav"), &pcm3, SR as u32)?;

    let probe = Audio::new_capture();
    for fx in [Fx::BreechRam, Fx::BreechExtract] { let _ = probe.recorded_fx(fx, 1); let _ = probe.recorded_fx(fx, 2); }
    let receipt = serde_json::json!({"what": "owner library recordings through the live capture graph, CPU only",
        "catalogue": log1, "tank_hit_by_component": log2, "crush_mixed_debris": log3, "bank": probe.recorded_fx_snapshot(),
        "voice_cap_per_update": crate::audio::RECORDED_FX_VOICES_PER_UPDATE});
    std::fs::write(format!("{outdir}/recorded_fx_reel_receipt.json"), serde_json::to_string_pretty(&receipt)?)?;
    let dropped = log3.iter().filter(|e| e.get("dropped").is_some()).count();
    let missing = log1.iter().filter(|e| e.get("dropped").is_some()).count();
    println!("[recorded-fx-reel] catalogue {} events ({missing} missing), component hits {}, crush {} contacts ({dropped} over the cap)",
        log1.len(), classes.len(), log3.len());
    anyhow::ensure!(missing == 0, "{missing} catalogue families are not installed");
    Ok(())
}
