//! Sound calibration check (`impact-mix-review`): the key events rendered device-free through the
//! live `Audio` graph at their physical level at the ear, each measured as the player receives it
//! (short-term A-weighted LAF,max and sample peak, dBFS), and the dB relations the calibration
//! promises asserted: the own gun near full scale, the running bed ~15 dB (LAF) below it, head-out
//! louder than buttoned, distance and the hull taking level off an enemy gun, hits ordered by their
//! physical level, every voice where the reproduction curve puts it, and nothing above -1 dBFS.
//!
//! Each event renders alone in a fresh capture graph, so its measurement is its own. Levels at the
//! ear come from the same laws the game uses (`sound_levels`, `sound_propagation`); the WAVs are
//! the capture graph's output at unity export gain.

use crate::acoustics as a;
use crate::audio::{Audio, EarBus, EngineTelemetry, Sfx};
use crate::reproduction::{laf_max_dbfs, peak_dbfs, tone_map_dbfs, LIMITER_CEILING};
use crate::sound_levels as levels;
use crate::sound_propagation::{self as prop, Atmosphere, EarLevel, Emission, Enclosure, Path};
use serde_json::{json, Value};
use std::path::Path as FsPath;

const HZ: usize = a::SAMPLE_RATE as usize;
const TICKS: usize = 60;
const FRAMES_PER_TICK: usize = HZ / TICKS;

/// One event as rendered and measured.
#[derive(Clone, Debug)]
pub struct Measured {
    pub name: &'static str,
    pub description: &'static str,
    /// Physical level at the ear (after enclosure and headset): peak dB and LAF dB(A).
    pub ear_lpk_db: f32,
    pub ear_laf_dba: f32,
    /// Where the reproduction curve puts the voice (dBFS(A) short-term).
    pub expected_laf_dbfs: f32,
    /// Measured on the capture output.
    pub laf_dbfs: f32,
    pub peak_dbfs: f32,
    /// Deepest master limiter gain reduction during the render (dB, <= 0).
    pub limiter_db: f32,
    pub samples: Vec<f32>,
}

/// One asserted relation.
#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub value_db: f32,
    pub range_db: (f32, f32),
    pub pass: bool,
}

fn render(seconds: f32, mut tick: impl FnMut(&Audio, usize)) -> (Vec<f32>, f32) {
    let audio = Audio::new_capture();
    let mut samples = Vec::with_capacity((seconds * HZ as f32) as usize * 2);
    for t in 0..(seconds * TICKS as f32).round() as usize {
        tick(&audio, t);
        samples.extend(audio.render_capture_frames(FRAMES_PER_TICK).expect("device-free capture"));
    }
    let limiter = audio.master_telemetry()["deepest_gain_reduction_db"].as_f64().unwrap_or(0.0) as f32;
    (samples, limiter)
}

fn measured(name: &'static str, description: &'static str, ear: (f32, f32), window_s: (f32, f32),
    (samples, limiter_db): (Vec<f32>, f32)) -> Measured
{
    let a = ((window_s.0 * HZ as f32) as usize * 2).min(samples.len());
    let b = ((window_s.1 * HZ as f32) as usize * 2).min(samples.len()).max(a);
    let window = &samples[a..b];
    Measured {
        name,
        description,
        ear_lpk_db: ear.0,
        ear_laf_dba: ear.1,
        expected_laf_dbfs: tone_map_dbfs(ear.1),
        laf_dbfs: laf_max_dbfs(window),
        peak_dbfs: peak_dbfs(window),
        limiter_db,
        samples,
    }
}

fn own_gun(level: &levels::SourceLevel) -> EarLevel {
    prop::at_ear(level.lpk_db, level.laf_db, level.spectrum, level.a_duration_ms, true)
}

/// A one-shot at the ear on the first tick.
fn one_shot(play: impl Fn(&Audio)) -> (Vec<f32>, f32) {
    render(2.0, |audio, t| if t == 0 { play(audio) })
}

/// The Tiger's engine and tracks buttoned up for a telemetry state: (telemetry, level at the ear).
fn engine_state(speed_norm: f32, throttle: f32, load: f32) -> (EngineTelemetry, f32) {
    let telemetry = EngineTelemetry::automatic(speed_norm, throttle, load, true);
    let interior = crate::audio::tiger_engine_interior_dba(telemetry);
    (telemetry, interior - prop::headset_loss_dba(levels::SpectrumClass::Engine))
}

/// The T-34's F-34 76 mm report at `range_m`, 10° off its muzzle axis (it is firing at the
/// listener), through `enclosure`.
fn enemy_gun(range_m: f32, enclosure: Enclosure) -> EarLevel {
    let shell = &crate::ballistics::BR_350B;
    let charge = levels::propellant_charge_kg(shell.caliber_mm, shell.mass_kg, shell.muzzle_velocity);
    let braked = levels::gun_is_braked(shell.caliber_mm, shell.muzzle_velocity);
    let emission = Emission::gun(charge, braked, 90.0)
        .with_directivity(levels::muzzle_directivity_db(10.0, braked));
    prop::propagate(&emission, &Path { distance_m: range_m, atmosphere: Atmosphere::WINTER,
        occluded: false, enclosure, headset: true })
}

/// Render and measure every key event.
pub fn measure_events() -> Vec<Measured> {
    let mut out = Vec::new();
    // The own 88 at the ear, the owner-verified recordings (SL §1 rows 1–3).
    for (name, description, level, sfx) in [
        ("own_gun_buttoned", "own 8.8 cm KwK 36 fired, buttoned up (163 dB / 139 dB(A) at the ear)",
            &levels::OWN_88_BUTTONED, Sfx::PlayerGunInt1),
        ("own_gun_hatch_open", "own 88 fired, a hatch open, head inside (168 / 142)",
            &levels::OWN_88_HATCH_OPEN, Sfx::PlayerGunInt1),
        ("own_gun_head_out", "own 88 fired, commander head-out behind the brake (182 / 152)",
            &levels::OWN_88_HEAD_OUT, Sfx::PlayerGunExt1),
    ] {
        let ear = own_gun(level);
        let render = one_shot(|audio| audio.play_at_ear(sfx, ear.laf_db));
        out.push(measured(name, description, (ear.lpk_db, ear.laf_db), (0.0, 1.5), render));
    }
    // The running bed: the Maybach and tracks buttoned up (SL §7: 93 / 110 / 115 dB(A)).
    for (name, description, state) in [
        ("engine_idle_buttoned", "Maybach HL 210 idling, buttoned, through the headset", (0.0, 0.0, 0.0)),
        ("engine_cross_country_buttoned", "Tiger at ~20 km/h cross-country, buttoned, headset",
            (0.46, 0.7, 0.6)),
        ("engine_road_buttoned", "Tiger at ~38 km/h on a road, buttoned, headset", (0.88, 0.9, 0.5)),
    ] {
        let (telemetry, ear) = engine_state(state.0, state.1, state.2);
        let render = render(4.0, |audio, _| audio.set_engine_at_ear(telemetry, ear, 1.0 / 60.0));
        out.push(measured(name, description, (f32::NAN, ear), (2.5, 4.0), render));
    }
    // An enemy T-34's gun at a distance.
    for (name, description, range, enclosure, reverb) in [
        ("enemy_gun_500m_head_out", "T-34 76 mm firing at the Tiger from 500 m, heard head-out",
            500.0, Enclosure::HeadOut, a::ReverbParams::OPEN_SNOW),
        ("enemy_gun_1000m_head_out", "the same gun at 1000 m, head-out", 1000.0, Enclosure::HeadOut,
            a::ReverbParams::OPEN_SNOW),
        ("enemy_gun_500m_buttoned", "the same gun at 500 m, buttoned up", 500.0, Enclosure::Buttoned,
            a::ReverbParams::STEEL_INTERIOR),
    ] {
        let ear = enemy_gun(range, enclosure);
        let report = a::synth_gun_report(76.2, 0.0, 0.0, 0x7635_0500);
        let mono: Vec<f32> = report.chunks_exact(2)
            .map(|f| (f[0] + f[1]) * std::f32::consts::FRAC_1_SQRT_2).collect();
        let render = one_shot(|audio| audio.play_world_mono_at_ear(&mono, &ear, 20.0, reverb));
        out.push(measured(name, description, (ear.lpk_db, ear.laf_db), (0.0, 2.0), render));
    }
    // Hits on the Tiger (SL §5), the live damage rack at each strike's level at the ear.
    for (name, description, signature, seed) in [
        ("hit_distant_frontal_ricochet", "76 mm ricochet off the glacis, range-bled",
            a::ImpactSignature::DistantFrontalRicochet, 0x7600_1001),
        ("hit_close_side_penetration", "76 mm penetration of the side beside the crew station",
            a::ImpactSignature::CloseT34SidePenetration, 0x7600_1003),
        ("hit_heavy_122mm_penetration", "122 mm penetration with a filler burst inside",
            a::ImpactSignature::HeavyObliquePenetration, 0x1220_1004),
    ] {
        let ear = a::impact_signature_ear_level(signature, seed);
        let voice = a::synth_impact_signature(signature, seed).main;
        let render = one_shot(|audio| audio.play_stereo_at_ear(voice.clone(), ear.laf_db, EarBus::Impact));
        out.push(measured(name, description, (ear.lpk_db, ear.laf_db), (0.0, 1.8), render));
    }
    // The own MG34 burst inside (SL §1 row 10) and a 76 mm round cracking past 10 m from a head-out
    // commander (SL §3).
    {
        let level = &levels::MG34_INSIDE;
        let ear = prop::at_ear(level.lpk_db, level.laf_db, level.spectrum, level.a_duration_ms, true);
        let render = render(3.0, |audio, _| audio.set_mg_at_ear(true, true, ear.laf_db, 1.0 / 60.0));
        out.push(measured("mg34_burst_buttoned", "coaxial MG34 burst fired inside, headset",
            (ear.lpk_db, ear.laf_db), (1.5, 3.0), render));
    }
    {
        let c = Atmosphere::WINTER.speed_of_sound();
        let speed = 650.0;
        let (lpk, period) = levels::whitham_n_wave(speed, c, 0.0762, levels::projectile_length_m(76.2),
            10.0).expect("supersonic");
        let mach = speed / c;
        let path = Path { distance_m: 10.0 * mach / (mach * mach - 1.0).sqrt(),
            atmosphere: Atmosphere::WINTER, occluded: false, enclosure: Enclosure::HeadOut,
            headset: true };
        let ear = prop::propagate(&Emission::crack(lpk, period), &path);
        let wave = a::synth_n_wave(period);
        let render = one_shot(|audio| audio.play_world_mono_at_ear(&wave, &ear, 300.0,
            a::ReverbParams::OPEN_SNOW));
        out.push(measured("crack_76mm_10m_head_out", "76 mm round passing 10 m from a head-out ear",
            (ear.lpk_db, ear.laf_db), (0.0, 1.0), render));
    }
    // Tinnitus of 10 dB SL, the ear model's percept (rendered relative to the player's threshold).
    {
        let render = render(8.0, |audio, t| if t == 0 {
            audio.set_ear_state(&crate::hearing::EarRender {
                shift_db: [[0.0; crate::hearing::EAR_BANDS]; 2], reflex_db: 0.0,
                conductive_db: [0.0; 2], tinnitus: Some((10.0, 3_800.0)) });
        });
        let mut m = measured("tinnitus_10db_sl", "tinnitus 10 dB SL after acoustic trauma",
            (f32::NAN, f32::NAN), (6.0, 8.0), render);
        m.expected_laf_dbfs = crate::audio::tinnitus_dbfs(10.0);
        out.push(m);
    }
    out
}

fn find<'a>(events: &'a [Measured], name: &str) -> &'a Measured {
    events.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("event {name}"))
}

/// The dB relations the calibration promises.
pub fn checks(events: &[Measured]) -> Vec<Check> {
    let mut out = Vec::new();
    let mut check = |name: String, value: f32, range: (f32, f32)| {
        out.push(Check { pass: value.is_finite() && value >= range.0 && value <= range.1,
            name, value_db: value, range_db: range });
    };
    let e = |name: &str| find(events, name);
    // The own gun buttoned reaches near full scale (verified recording, crest 15.7 dB).
    check("own gun buttoned: sample peak near full scale (dBFS)".into(),
        e("own_gun_buttoned").peak_dbfs, (-3.0, -0.9));
    // The running bed ~15 dB (LAF) below it.
    check("own gun buttoned LAF over the cross-country bed (dB)".into(),
        e("own_gun_buttoned").laf_dbfs - e("engine_cross_country_buttoned").laf_dbfs, (12.0, 18.0));
    // Buttoning up does not make the own gun louder: head-out and hatch-open are louder.
    check("own gun head-out over buttoned, LAF (dB; the limiter holds it below the curve's 6.5)".into(),
        e("own_gun_head_out").laf_dbfs - e("own_gun_buttoned").laf_dbfs, (1.0, 7.0));
    check("own gun hatch open over buttoned, LAF (dB)".into(),
        e("own_gun_hatch_open").laf_dbfs - e("own_gun_buttoned").laf_dbfs, (0.3, 3.5));
    // The engine law (SL §7: 93 idle, 115 road): road over idle as the curve maps it.
    let road = e("engine_road_buttoned").laf_dbfs - e("engine_idle_buttoned").laf_dbfs;
    check("engine road over idle, measured minus the curve's difference (dB)".into(),
        road - (e("engine_road_buttoned").expected_laf_dbfs - e("engine_idle_buttoned").expected_laf_dbfs),
        (-1.5, 1.5));
    check("engine road over idle at the ear, physical (dB(A))".into(),
        e("engine_road_buttoned").ear_laf_dba - e("engine_idle_buttoned").ear_laf_dba, (18.0, 27.0));
    // Distance and the hull take level off an enemy gun.
    let far = e("enemy_gun_500m_head_out").laf_dbfs - e("enemy_gun_1000m_head_out").laf_dbfs;
    let curve = e("enemy_gun_500m_head_out").expected_laf_dbfs
        - e("enemy_gun_1000m_head_out").expected_laf_dbfs;
    check("enemy gun 500 m over 1000 m head-out, measured minus the curve's difference (dB)".into(),
        far - curve, (-1.5, 1.5));
    check("enemy gun 500 m head-out over buttoned, LAF (dB)".into(),
        e("enemy_gun_500m_head_out").laf_dbfs - e("enemy_gun_500m_buttoned").laf_dbfs, (8.0, 45.0));
    // Hits ordered by their physical level at the ear.
    check("122 mm penetration over the distant ricochet, LAF (dB)".into(),
        e("hit_heavy_122mm_penetration").laf_dbfs - e("hit_distant_frontal_ricochet").laf_dbfs,
        (3.0, 40.0));
    check("close 76 mm penetration over the distant ricochet, LAF (dB)".into(),
        e("hit_close_side_penetration").laf_dbfs - e("hit_distant_frontal_ricochet").laf_dbfs,
        (0.5, 40.0));
    // Every voice where the reproduction curve puts it, unless the master limiter acted on it.
    for m in events {
        if m.limiter_db < -0.5 || !m.expected_laf_dbfs.is_finite() {
            continue;
        }
        check(format!("{}: measured LAF minus the curve (dB)", m.name),
            m.laf_dbfs - m.expected_laf_dbfs, (-1.5, 1.5));
    }
    // Nothing above -1 dBFS.
    let ceiling = 20.0 * LIMITER_CEILING.log10();
    for m in events {
        check(format!("{}: sample peak under the -1 dBFS ceiling (dB of headroom)", m.name),
            ceiling - m.peak_dbfs, (-0.01, 200.0));
    }
    out
}

fn event_json(m: &Measured) -> Value {
    let finite = |x: f32| if x.is_finite() { json!((x * 100.0).round() / 100.0) } else { Value::Null };
    json!({"event": m.name, "description": m.description,
        "ear_lpk_db": finite(m.ear_lpk_db), "ear_laf_dba": finite(m.ear_laf_dba),
        "expected_laf_dbfs": finite(m.expected_laf_dbfs), "laf_dbfs": finite(m.laf_dbfs),
        "peak_dbfs": finite(m.peak_dbfs), "limiter_db": finite(m.limiter_db)})
}

/// Render every event, write one WAV per event and a receipt, and fail if a relation fails.
/// Refuses to overwrite a prior run.
pub fn run(prefix: &str) -> anyhow::Result<Value> {
    let receipt_path = format!("{prefix}_receipt.json");
    anyhow::ensure!(!FsPath::new(&receipt_path).exists(), "refusing to overwrite {receipt_path}");
    if let Some(parent) = FsPath::new(prefix).parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let events = measure_events();
    for m in &events {
        let wav = format!("{prefix}_{}.wav", m.name);
        anyhow::ensure!(!FsPath::new(&wav).exists(), "refusing to overwrite {wav}");
        anyhow::ensure!(m.samples.iter().all(|x| x.is_finite()), "nonfinite sample in {}", m.name);
        a::write_wav_stereo(&wav, &m.samples, a::SAMPLE_RATE)?;
    }
    let results = checks(&events);
    let failed: Vec<&Check> = results.iter().filter(|c| !c.pass).collect();
    let receipt = json!({
        "schema": "kessel.sound-calibration-check.v1",
        "sample_rate": HZ, "channels": 2, "control_rate_hz": TICKS, "export_gain": 1.0,
        "scope": "Each key event rendered alone through the live device-free Audio graph at its \
            physical level at the ear (sound_levels, sound_propagation), measured on the output: \
            LAF,max (IEC 61672 A-weighting, fast) and sample peak in dBFS. Ear levels are the \
            calibration's, not recordings of a Tiger.",
        "events": events.iter().map(event_json).collect::<Vec<_>>(),
        "checks": results.iter().map(|c| json!({"check": c.name,
            "value_db": (c.value_db * 100.0).round() / 100.0, "range_db": [c.range_db.0, c.range_db.1],
            "pass": c.pass})).collect::<Vec<_>>(),
        "pass": failed.is_empty(),
    });
    std::fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    anyhow::ensure!(failed.is_empty(), "calibration check failed: {:?}",
        failed.iter().map(|c| format!("{} = {:.2} not in {:?}", c.name, c.value_db, c.range_db))
            .collect::<Vec<_>>());
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_calibration_relation_holds_through_the_live_graph() {
        let events = measure_events();
        for m in &events {
            println!("{:32} ear {:>6.1} dB pk {:>6.1} dB(A) | curve {:>6.1} | LAF {:>6.1} dBFS \
                peak {:>6.1} dBFS | limiter {:>5.1} dB", m.name, m.ear_lpk_db, m.ear_laf_dba,
                m.expected_laf_dbfs, m.laf_dbfs, m.peak_dbfs, m.limiter_db);
        }
        let failed: Vec<String> = checks(&events).into_iter().filter(|c| !c.pass)
            .map(|c| format!("{} = {:.2} not in {:?}", c.name, c.value_db, c.range_db)).collect();
        assert!(failed.is_empty(), "{failed:#?}");
    }
}

#[cfg(test)]
mod reflex_diagnostic {
    use super::*;

    /// The own 88 buttoned through a fresh ear whose middle-ear reflex the shot itself triggers
    /// (the runtime order: impulse, then the voice, then the ear advanced per 60 Hz frame).
    #[test]
    #[ignore = "diagnostic: prints the own gun's level through its own reflex"]
    fn print_own_gun_through_its_own_reflex() {
        let ear = own_gun(&levels::OWN_88_BUTTONED);
        for with_ear in [false, true] {
            let mut hearing = crate::hearing::Hearing::default();
            let (samples, _) = render(2.0, |audio, t| {
                if t == 0 && with_ear {
                    crate::acoustics::hear_strike(&mut hearing, &ear, 0.0);
                }
                if t == 0 {
                    audio.play_at_ear(Sfx::PlayerGunInt1, ear.laf_db);
                }
                hearing.advance(1.0 / 60.0, 1.0 / 60.0, 0.0);
                if with_ear {
                    audio.set_ear_state(&hearing.render());
                }
            });
            println!("ear model {with_ear}: LAF,max {:.1} dBFS(A), peak {:.1} dBFS, reflex now {:.1} dB",
                laf_max_dbfs(&samples), peak_dbfs(&samples), hearing.reflex_db());
        }
    }
}
