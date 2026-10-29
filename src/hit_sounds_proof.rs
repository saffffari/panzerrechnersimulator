//! `hit-sounds <outdir>`: CPU-only A/B of the tank-hit recordings (`audio::hit_recordings`)
//! against the voices they replace, rendered through the live capture graph at their calibrated
//! levels. Penetration: a T-34 holed at 150 m heard head-out (old: the `ImpactTank` +
//! `PenetrationHit` world voices; new: a long close explosion), and the own Tiger holed with the
//! listener buttoned (old: the synthesized steel strike; new: the recording through the hull and
//! the steel cabin). Non-penetrating: a light, a medium and a hard strike, each on the own hull
//! (buttoned) and on a T-34 at 100 m (head-out), old and new. Writes one WAV per case, three
//! reels and a JSON receipt with the hardness, corner, level shift and measured levels.
use super::*;
use crate::audio::{Audio, EarBus, HitFamily, Sfx};
use crate::ballistics::{self as b, ArmorScheme, Facing, ImpactEvent, Terminal};
use crate::sound_propagation::{self as prop, Atmosphere, Enclosure, Path};

const SR: usize = crate::acoustics::SAMPLE_RATE as usize;
const GAP_S: f32 = 0.6;

struct Case {
    name: String,
    samples: Vec<f32>,
    meta: serde_json::Value,
}

fn render(seconds: f32, play: impl FnOnce(&Audio) -> serde_json::Value) -> anyhow::Result<(Vec<f32>, serde_json::Value)> {
    let audio = Audio::new_capture();
    let meta = play(&audio);
    let pcm = audio.render_capture_frames((seconds * SR as f32) as usize)
        .ok_or_else(|| anyhow::anyhow!("capture graph not active"))?;
    Ok((pcm, meta))
}

fn levels(samples: &[f32]) -> serde_json::Value {
    let mono: Vec<f32> = samples.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect();
    let energy = |v: &[f32]| v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
    let total = energy(&mono).max(1e-30);
    let above_300 = energy(&crate::audio::hit_high_pass(&mono, 300.0)) / total;
    serde_json::json!({
        "laf_max_dbfs": crate::reproduction::laf_max_dbfs(samples),
        "peak_dbfs": crate::reproduction::peak_dbfs(samples),
        "energy_share_above_300_hz": above_300,
        "finite": samples.iter().all(|v| v.is_finite()),
    })
}

/// An impact on `facing` of `scheme` by `shell` at `range_m` and horizontal obliquity, forced to a
/// non-penetrating terminal if the solve went through (the A/B wants the stopped voice).
fn strike(shell: &'static b::ShellSpec, range_m: f32, scheme: ArmorScheme, facing: Facing, obliquity: f32,
    stopped_as: Terminal) -> ImpactEvent
{
    let mut event = b::resolve(shell, range_m, scheme.plate(facing), obliquity, 7);
    if matches!(event.terminal, Terminal::Penetration) { event.terminal = stopped_as; }
    event
}

fn own_hull_ear(event: &ImpactEvent, penetrated: bool, station_m: f32) -> prop::EarLevel {
    let fill = event.shell.filler_g / 1000.0;
    let he = matches!(event.shell.kind, b::ShellKind::He | b::ShellKind::Heat);
    let s = crate::sound_levels::own_armour_strike(event.shell.mass_kg, event.striking_v,
        event.impact_angle_deg, station_m, penetrated, false, fill, he);
    prop::at_ear(s.lpk_db, s.laf_db, s.spectrum, s.a_duration_ms, true)
}

fn world_ear(sfx: Sfx, emission: prop::Emission, distance_m: f32) -> prop::EarLevel {
    let path = Path { distance_m, atmosphere: Atmosphere::WINTER, occluded: false, enclosure: Enclosure::HeadOut, headset: false };
    presented_world_ear(sfx, prop::propagate(&emission, &path), distance_m)
}

fn voice_meta(v: &crate::audio::HitVoice, hardness: Option<f32>) -> serde_json::Value {
    serde_json::json!({"file": v.file, "variant": v.variant, "cutoff_hz": v.cutoff_hz,
        "level_shift_db": v.level_shift_db, "hardness": hardness})
}

/// Own-hull strike, new voice: as `Game::play_own_hull_hit_recording` with the parametric steel
/// interior (the traced room needs the game's scene).
fn own_new(audio: &Audio, family: HitFamily, seed: u32, event: &ImpactEvent, penetrated: bool, station_m: f32,
    bearing: f32) -> serde_json::Value
{
    let Some(v) = audio.hit_voice(family, seed) else { return serde_json::json!({"missing": true}) };
    let ear = super::game_hit_sounds::own_hull_hit_ear(own_hull_ear(event, penetrated, station_m),
        v.level_shift_db);
    let stereo = audio.render_world_mono_for_ear(&v.mono, &ear, bearing,
        crate::audio::Room::Parametric(crate::acoustics::ReverbParams::STEEL_INTERIOR));
    audio.play_stereo_at_ear(stereo, ear.laf_db, EarBus::Impact);
    let hardness = match family { HitFamily::Stopped { hardness } => Some(hardness), _ => None };
    let mut meta = voice_meta(&v, hardness);
    meta["ear_laf_dba"] = ear.laf_db.into();
    meta
}

/// Own-hull strike, old voice: the synthesized steel strike at the same calibrated level.
fn own_old(audio: &Audio, event: &ImpactEvent, penetrated: bool, station_m: f32, bearing: f32) -> serde_json::Value {
    let mut params = crate::acoustics::params_from_event(event, Facing::TurretSide, bearing, station_m, 11);
    params.penetrated = penetrated;
    let mut sound = crate::acoustics::synth_damage_impact_with_pressure_delay(params,
        crate::acoustics::DamageSoundContext::default(), station_m / 331.0);
    audio.blend_named_stopped_armor(&mut sound.main, bearing, event.terminal);
    let ear = own_hull_ear(event, penetrated, station_m);
    audio.play_stereo_at_ear(sound.main, ear.laf_db, EarBus::Impact);
    serde_json::json!({"voice": "synth_damage_impact_with_pressure_delay", "ear_laf_dba": ear.laf_db})
}

pub(crate) fn run(outdir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    let probe = Audio::new_capture();
    let bank = probe.hit_recordings_snapshot();
    anyhow::ensure!(probe.hit_voice(HitFamily::Penetration, 1).is_some(),
        "the tank-hit recordings are not installed (assets/sounds/tank_hit_*.ogg under KESSEL_ASSET_ROOT)");
    drop(probe);

    let mut cases: Vec<Case> = Vec::new();
    let mut push = |name: &str, seconds: f32, play: &dyn Fn(&Audio) -> serde_json::Value| -> anyhow::Result<()> {
        let (samples, meta) = render(seconds, |a| play(a))?;
        let mut meta = meta;
        meta["measured"] = levels(&samples);
        cases.push(Case { name: name.into(), samples, meta });
        Ok(())
    };

    // ── Penetration ──
    let pen = b::resolve(&b::PZGR_39, 150.0, ArmorScheme::T34_76.plate(Facing::HullSide), 10.0, 3);
    push("penetration_t34_150m_head_out_old", 4.6, &|a| {
        for sfx in [Sfx::ImpactTank, Sfx::PenetrationHit] {
            let ear = world_ear(sfx, sound_emission_for(sfx), 150.0);
            a.play_world_at_ear(sfx, &ear, 40.0, crate::acoustics::ReverbParams::OPEN_SNOW);
        }
        serde_json::json!({"voices": ["ImpactTank", "PenetrationHit"], "terminal": format!("{:?}", pen.terminal)})
    })?;
    push("penetration_t34_150m_head_out_new", 4.6, &|a| {
        let Some(v) = a.hit_voice(HitFamily::Penetration, 5) else { return serde_json::json!({"missing": true}) };
        let ear = world_ear(Sfx::PenetrationHit, sound_emission_for(Sfx::PenetrationHit).shifted(v.level_shift_db), 150.0);
        a.play_world_mono_in_room(&v.mono, &ear, 40.0, crate::audio::Room::Parametric(crate::acoustics::ReverbParams::OPEN_SNOW));
        voice_meta(&v, None)
    })?;
    let own_pen = b::resolve(&b::BR_365, 400.0, ArmorScheme::TigerI.plate(Facing::TurretSide), 0.0, 3);
    push("penetration_own_tiger_buttoned_old", 4.6, &|a| own_old(a, &own_pen, true, 1.2, 270.0))?;
    push("penetration_own_tiger_buttoned_new", 4.6, &|a| own_new(a, HitFamily::Penetration, 5, &own_pen, true, 1.2, 270.0))?;

    // ── Non-penetrating: light, medium, hard ──
    let strikes = [
        ("light", strike(&b::BR_240, 1200.0, ArmorScheme::TigerI, Facing::HullFront, 60.0, Terminal::Ricochet)),
        ("medium", strike(&b::BR_350B, 700.0, ArmorScheme::TigerI, Facing::TurretSide, 30.0, Terminal::Bounce)),
        ("hard", strike(&b::BR_365, 300.0, ArmorScheme::TigerI, Facing::HullFront, 0.0, Terminal::Bounce)),
    ];
    let mut hardness_by = Vec::new();
    for (label, event) in &strikes {
        let family = super::game_hit_sounds::hit_family(event, false);
        let HitFamily::Stopped { hardness } = family else { anyhow::bail!("{label} should be stopped") };
        hardness_by.push((label.to_string(), hardness, crate::audio::hit_hardness_of(event)));
        let describe = serde_json::json!({"shell": event.shell.name, "striking_v": event.striking_v,
            "obliquity_deg": event.impact_angle_deg, "pen_mm": event.pen_mm, "effective_mm": event.effective_mm,
            "terminal": format!("{:?}", event.terminal)});
        // One recording for all three, so only the filter and level differ.
        push(&format!("nonpen_{label}_own_tiger_buttoned_new"), 3.4, &|a| {
            let mut m = own_new(a, family, 9, event, false, 1.2, 90.0);
            m["strike"] = describe.clone(); m
        })?;
        push(&format!("nonpen_{label}_own_tiger_buttoned_old"), 3.4, &|a| own_old(a, event, false, 1.2, 90.0))?;
        push(&format!("nonpen_{label}_t34_100m_head_out_new"), 3.4, &|a| {
            let Some(v) = a.hit_voice(family, 9) else { return serde_json::json!({"missing": true}) };
            let ear = world_ear(Sfx::ArmorClang1, sound_emission_for(Sfx::ArmorClang1).shifted(v.level_shift_db), 100.0);
            a.play_world_mono_in_room(&v.mono, &ear, 320.0, crate::audio::Room::Parametric(crate::acoustics::ReverbParams::OPEN_SNOW));
            let mut m = voice_meta(&v, Some(hardness));
            m["strike"] = describe.clone(); m
        })?;
        push(&format!("nonpen_{label}_t34_100m_head_out_old"), 3.4, &|a| {
            let ear = world_ear(Sfx::ArmorClang1, sound_emission_for(Sfx::ArmorClang1), 100.0);
            a.play_world_at_ear(Sfx::ArmorClang1, &ear, 320.0, crate::acoustics::ReverbParams::OPEN_SNOW);
            serde_json::json!({"voice": "ArmorClang1"})
        })?;
    }

    // ── WAVs, reels, receipt ──
    for c in &cases {
        crate::acoustics::write_wav_stereo(format!("{outdir}/hit_{}.wav", c.name), &c.samples, SR as u32)?;
    }
    let reel = |names: &[&str]| -> Vec<f32> {
        let gap = vec![0.0f32; (GAP_S * SR as f32) as usize * 2];
        let mut out = Vec::new();
        for n in names {
            if let Some(c) = cases.iter().find(|c| c.name == *n) { out.extend(&c.samples); out.extend(&gap); }
        }
        out
    };
    let reels = [
        ("hit_reel_penetration_old_vs_new", vec!["penetration_t34_150m_head_out_old", "penetration_t34_150m_head_out_new",
            "penetration_own_tiger_buttoned_old", "penetration_own_tiger_buttoned_new"]),
        ("hit_reel_nonpen_light_medium_hard_new", vec!["nonpen_light_own_tiger_buttoned_new", "nonpen_medium_own_tiger_buttoned_new",
            "nonpen_hard_own_tiger_buttoned_new", "nonpen_light_t34_100m_head_out_new", "nonpen_medium_t34_100m_head_out_new",
            "nonpen_hard_t34_100m_head_out_new"]),
        ("hit_reel_nonpen_light_medium_hard_old", vec!["nonpen_light_own_tiger_buttoned_old", "nonpen_medium_own_tiger_buttoned_old",
            "nonpen_hard_own_tiger_buttoned_old", "nonpen_light_t34_100m_head_out_old", "nonpen_medium_t34_100m_head_out_old",
            "nonpen_hard_t34_100m_head_out_old"]),
    ];
    for (name, names) in &reels {
        crate::acoustics::write_wav_stereo(format!("{outdir}/{name}.wav"), &reel(names), SR as u32)?;
    }

    let meta = |n: &str| cases.iter().find(|c| c.name == n).map(|c| c.meta.clone()).unwrap_or_default();
    let mut checks = Vec::new();
    let mut check = |name: &str, pass: bool, detail: serde_json::Value| {
        checks.push(serde_json::json!({"check": name, "pass": pass, "detail": detail}));
    };
    for c in &cases {
        let m = &c.meta["measured"];
        let audible = m["laf_max_dbfs"].as_f64().is_some_and(|v| v > -60.0);
        let safe = m["peak_dbfs"].as_f64().is_some_and(|v| v <= 0.0) && m["finite"].as_bool() == Some(true);
        check(&format!("{} audible, finite, no clip", c.name), audible && safe, m.clone());
    }
    let corner = |l: &str, where_: &str| meta(&format!("nonpen_{l}_{where_}_new"))["cutoff_hz"].as_f64().unwrap_or(f64::NAN);
    let shift = |l: &str, where_: &str| meta(&format!("nonpen_{l}_{where_}_new"))["level_shift_db"].as_f64().unwrap_or(f64::NAN);
    let share = |l: &str, where_: &str| meta(&format!("nonpen_{l}_{where_}_new"))["measured"]["energy_share_above_300_hz"].as_f64().unwrap_or(f64::NAN);
    for w in ["own_tiger_buttoned", "t34_100m_head_out"] {
        check(&format!("{w}: corner falls light > medium > hard"),
            corner("light", w) > corner("medium", w) && corner("medium", w) > corner("hard", w),
            serde_json::json!([corner("light", w), corner("medium", w), corner("hard", w)]));
        check(&format!("{w}: level shift rises light <= medium <= hard <= 0"),
            shift("light", w) <= shift("medium", w) && shift("medium", w) <= shift("hard", w) && shift("hard", w) <= 0.0,
            serde_json::json!([shift("light", w), shift("medium", w), shift("hard", w)]));
        check(&format!("{w}: the light strike is thinner (more energy above 300 Hz) than the hard"),
            share("light", w) > share("hard", w), serde_json::json!([share("light", w), share("medium", w), share("hard", w)]));
    }
    let pen_variant = meta("penetration_own_tiger_buttoned_new")["variant"].clone();
    check("penetration voices come from the long family", meta("penetration_t34_150m_head_out_new")["file"]
        .as_str().is_some_and(|f| f.contains("tank_hit_penetration_")), pen_variant);
    let pass = checks.iter().all(|c| c["pass"].as_bool() == Some(true));
    let receipt = serde_json::json!({
        "what": "tank-hit recordings A/B (old voices vs owner's Rust & Blood recordings), live capture graph, CPU only",
        "bank": bank,
        "hardness": hardness_by.iter().map(|(l, h, h2)| serde_json::json!({"strike": l, "hardness": h, "hardness_of": h2})).collect::<Vec<_>>(),
        "cases": cases.iter().map(|c| serde_json::json!({"name": c.name, "wav": format!("hit_{}.wav", c.name), "meta": c.meta})).collect::<Vec<_>>(),
        "reels": reels.iter().map(|(n, names)| serde_json::json!({"wav": format!("{n}.wav"), "order": names})).collect::<Vec<_>>(),
        "checks": checks,
        "pass": pass,
    });
    std::fs::write(format!("{outdir}/hit_sounds_receipt.json"), serde_json::to_string_pretty(&receipt)?)?;
    for c in receipt["checks"].as_array().into_iter().flatten() {
        println!("[hit-sounds] {} {}", if c["pass"].as_bool() == Some(true) { "PASS" } else { "FAIL" }, c["check"].as_str().unwrap_or(""));
    }
    anyhow::ensure!(pass, "hit-sounds checks failed; see {outdir}/hit_sounds_receipt.json");
    println!("[hit-sounds] PASS: {} cases; {outdir}/hit_sounds_receipt.json", cases.len());
    Ok(())
}
