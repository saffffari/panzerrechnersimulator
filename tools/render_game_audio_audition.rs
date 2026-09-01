//! Standalone, dependency-free renderer for the layers which are generated dynamically at runtime
//! and therefore are not enumerated by `kessel soundbank`.
//!
//! Build/run from the repository root after running the normal soundbank and sfx tools:
//!   rustc --edition 2021 -O tools/render_game_audio_audition.rs -o /tmp/kessel-audition
//!   /tmp/kessel-audition artifacts/audio/game-audition-2026-08-27

// `acoustics.rs` has one bridge function whose signature names the game ballistics types. The
// offline renderer never calls it, so these shape-compatible declarations avoid pulling glam and
// the rest of the simulation into this otherwise std-only tool.
#[allow(dead_code)]
mod ballistics {
    #[derive(Clone, Copy)]
    pub enum ShellKind {
        Apcbc,
        Ap,
        Apcr,
        He,
    }
    #[derive(Clone, Copy)]
    pub enum Facing {
        HullFront,
        HullSide,
        HullRear,
        TurretFront,
        TurretSide,
        TurretRear,
    }
    #[derive(Clone, Copy)]
    pub enum Terminal {
        Penetration,
        Ricochet,
    }
    pub struct ShellSpec {
        pub kind: ShellKind,
        pub filler_g: f32,
        pub caliber_mm: f32,
        pub mass_kg: f32,
    }
    pub struct Plate {
        pub thickness_mm: f32,
    }
    pub struct ImpactEvent {
        pub shell: ShellSpec,
        pub striking_v: f32,
        pub plate: Plate,
        pub effective_mm: f32,
        pub impact_angle_deg: f32,
        pub residual_v: f32,
        pub behind_armor: f32,
        pub terminal: Terminal,
    }
}

#[path = "../src/acoustics.rs"]
mod acoustics;

use acoustics::{
    FragmentCollisionParams, FragmentMaterial, FragmentWhizzParams, ImpactSignature, MechanicalCue,
    MechanicalParams, ProceduralSound, SAMPLE_RATE,
};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MASTER_TARGET_PEAK: f32 = 0.501_187_2; // -6.0 dBFS
const MASTER_GAP_S: f32 = 0.65;
const MASTER_LEAD_S: f32 = 0.50;

#[derive(Clone)]
struct Cue {
    category: &'static str,
    name: String,
    path: PathBuf,
}

#[derive(Clone)]
struct Pcm {
    sample_rate: u32,
    channels: u16,
    samples: Vec<f32>,
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0f32, |value, sample| value.max(sample.abs()))
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
}

fn slug(cue: MechanicalCue) -> &'static str {
    match cue {
        MechanicalCue::MainGunRecoil => "main_gun_recoil",
        MechanicalCue::BreechOpen => "breech_open",
        MechanicalCue::BreechLock => "breech_lock",
        MechanicalCue::CasingExtract => "casing_extract",
        MechanicalCue::CasingDrop => "casing_drop",
        MechanicalCue::ShellLift => "shell_lift",
        MechanicalCue::ShellRam => "shell_ram",
        MechanicalCue::ShellDrop => "shell_drop",
        MechanicalCue::BodyImpact => "body_impact",
    }
}

fn material_slug(material: FragmentMaterial) -> &'static str {
    match material {
        FragmentMaterial::ArmorPlate => "armor_plate",
        FragmentMaterial::BreechBlock => "breech_block",
        FragmentMaterial::TurretFloor => "turret_floor",
        FragmentMaterial::BrassCase => "brass_case",
    }
}

fn write_layer(
    root: &Path,
    category: &'static str,
    name: &str,
    samples: Vec<f32>,
    cues: &mut Vec<Cue>,
    manifest: &mut Vec<(String, &'static str, f32, f32, f32)>,
) -> io::Result<()> {
    let dir = root.join("generated-layers").join(category);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.wav"));
    acoustics::write_wav_stereo(&path, &samples, SAMPLE_RATE)?;
    manifest.push((
        name.to_string(),
        category,
        samples.len() as f32 / 2.0 / SAMPLE_RATE as f32,
        peak(&samples),
        rms(&samples),
    ));
    cues.push(Cue {
        category,
        name: name.to_string(),
        path,
    });
    Ok(())
}

/// Render the exact procedural engine source at the playback-speed and gain targets used by
/// `Audio::set_engine_telemetry`. Linear interpolation is sufficient here because rodio likewise
/// changes source playback rate rather than regenerating the motor model.
fn loop_at(source: &[f32], playback: f32, gain: f32, duration_s: f32) -> Vec<f32> {
    let source_frames = source.len() / 2;
    let frames = (duration_s * SAMPLE_RATE as f32) as usize;
    let mut out = vec![0.0f32; frames * 2];
    for frame in 0..frames {
        let position = frame as f32 * playback;
        let base = position.floor() as usize % source_frames;
        let next = (base + 1) % source_frames;
        let fraction = position.fract();
        for channel in 0..2 {
            let a = source[base * 2 + channel];
            let b = source[next * 2 + channel];
            out[frame * 2 + channel] = (a + (b - a) * fraction) * gain;
        }
    }
    // The runtime loops indefinitely. A short review file needs a click-free editorial exit.
    let fade_frames = (0.04 * SAMPLE_RATE as f32) as usize;
    for frame in 0..fade_frames.min(frames) {
        let fade_in = frame as f32 / fade_frames as f32;
        let fade_out = (fade_frames - frame) as f32 / fade_frames as f32;
        for channel in 0..2 {
            out[frame * 2 + channel] *= fade_in;
            out[(frames - fade_frames + frame) * 2 + channel] *= fade_out;
        }
    }
    out
}

fn engine_targets(speed_norm: f32, throttle: f32, load: f32, rpm_norm: f32) -> (f32, f32) {
    // Keep these synchronized with src/audio.rs. They are copied here so the tool remains
    // standalone and does not pull rodio/device initialization into an offline render.
    const IDLE_VOLUME: f32 = 0.18;
    const MAX_VOLUME: f32 = 0.56;
    const IDLE_SPEED: f32 = 0.78;
    const MAX_SPEED: f32 = 2.42;
    let effort = (0.24 * throttle + 0.48 * load + 0.28 * speed_norm.abs()).clamp(0.0, 1.0);
    let volume = IDLE_VOLUME + (MAX_VOLUME - IDLE_VOLUME) * effort.sqrt();
    let rpm = rpm_norm.clamp(0.0, 1.0).powf(0.84);
    let playback = (IDLE_SPEED + (MAX_SPEED - IDLE_SPEED) * rpm) * (1.0 - 0.035 * load);
    (volume, playback)
}

fn turret_targets(
    angular_speed: f32,
    hydraulic_load: f32,
    engine_rpm: f32,
    engine_load: f32,
    interior: bool,
) -> (f32, f32) {
    let base = if interior { 0.22 } else { 0.095 };
    let volume = base
        * angular_speed.min(1.0).sqrt()
        * (0.72 + 0.28 * hydraulic_load)
        * (0.88 + 0.12 * engine_load);
    let playback = (0.68 + 0.30 * engine_rpm + 0.20 * angular_speed.sqrt()
        - 0.070 * hydraulic_load)
        .clamp(0.62, 1.24);
    (volume, playback)
}

fn render_generated_layers(root: &Path) -> io::Result<Vec<Cue>> {
    let mut cues = Vec::new();
    let mut manifest = Vec::new();

    for (index, cue) in MechanicalCue::ALL.into_iter().enumerate() {
        let bearing = [-18.0, 24.0, -8.0, 31.0, 48.0, -42.0, 12.0, -56.0, 67.0][index];
        let intensity = if cue == MechanicalCue::BodyImpact {
            0.72
        } else {
            0.88
        };
        write_layer(
            root,
            "interaction-crew",
            slug(cue),
            acoustics::synth_mechanical(MechanicalParams {
                cue,
                intensity,
                bearing_deg: bearing,
                seed: 0x4D45_4300 + index as u32,
            }),
            &mut cues,
            &mut manifest,
        )?;
    }

    let engine = acoustics::synth_procedural(ProceduralSound::EngineLoop);
    for (name, telemetry) in [
        ("engine_idle_600rpm", (0.0, 0.05, 0.08, 0.08)),
        ("engine_low_1000rpm", (0.14, 0.34, 0.30, 0.27)),
        ("engine_cruise_1600rpm", (0.52, 0.48, 0.38, 0.56)),
        ("engine_full_load_2500rpm", (0.90, 1.00, 0.96, 0.94)),
    ] {
        let (volume, playback) = engine_targets(telemetry.0, telemetry.1, telemetry.2, telemetry.3);
        write_layer(
            root,
            "engine",
            name,
            loop_at(&engine, playback, volume, 5.0),
            &mut cues,
            &mut manifest,
        )?;
    }
    for (name, load) in [
        ("engine_1600rpm_coast", 0.10),
        ("engine_1600rpm_heavy_load", 0.96),
    ] {
        let (volume, playback) = engine_targets(0.52, 0.52, load, 0.56);
        write_layer(
            root,
            "engine",
            name,
            loop_at(&engine, playback, volume, 4.0),
            &mut cues,
            &mut manifest,
        )?;
    }
    // Eight audible forward stages. Each new stage begins with the rev drop enforced on an upshift
    // by EngineState, then climbs progressively as road speed/load increase.
    let mut gear_sweep = Vec::new();
    for gear in 0..8 {
        let road = [0.06, 0.13, 0.22, 0.34, 0.47, 0.62, 0.77, 0.93][gear];
        let rpm = [0.78, 0.50, 0.55, 0.59, 0.63, 0.67, 0.72, 0.80][gear];
        let load = 0.48 + gear as f32 * 0.055;
        let (volume, playback) = engine_targets(road, 0.76, load, rpm);
        gear_sweep.extend(loop_at(&engine, playback, volume, 1.8));
        gear_sweep.resize(
            gear_sweep.len() + (0.12 * SAMPLE_RATE as f32) as usize * 2,
            0.0,
        );
    }
    write_layer(
        root,
        "engine",
        "engine_forward_8_gear_sweep",
        gear_sweep,
        &mut cues,
        &mut manifest,
    )?;

    let turret = acoustics::synth_procedural(ProceduralSound::TurretTraverseLoop);
    for (name, state) in [
        ("turret_interior_crawl", (0.20, 0.35, 0.30, 0.25, true)),
        ("turret_interior_lay", (0.58, 0.62, 0.58, 0.42, true)),
        ("turret_interior_full", (1.00, 0.92, 0.88, 0.70, true)),
        ("turret_exterior_full", (1.00, 0.92, 0.88, 0.70, false)),
    ] {
        let (volume, playback) = turret_targets(state.0, state.1, state.2, state.3, state.4);
        write_layer(
            root,
            "turret",
            name,
            loop_at(&turret, playback, volume, 4.0),
            &mut cues,
            &mut manifest,
        )?;
    }
    let mut traverse_sweep = Vec::new();
    for state in [
        (0.18, 0.28, 0.28, 0.20, true),
        (0.52, 0.55, 0.55, 0.40, true),
        (1.00, 0.95, 0.90, 0.72, true),
        (1.00, 0.95, 0.90, 0.72, false),
    ] {
        let (volume, playback) = turret_targets(state.0, state.1, state.2, state.3, state.4);
        traverse_sweep.extend(loop_at(&turret, playback, volume, 2.4));
        traverse_sweep.resize(
            traverse_sweep.len() + (0.15 * SAMPLE_RATE as f32) as usize * 2,
            0.0,
        );
    }
    write_layer(
        root,
        "turret",
        "turret_speed_load_perspective_sweep",
        traverse_sweep,
        &mut cues,
        &mut manifest,
    )?;

    for (index, material) in [
        FragmentMaterial::ArmorPlate,
        FragmentMaterial::BreechBlock,
        FragmentMaterial::TurretFloor,
        FragmentMaterial::BrassCase,
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("fragment_contact_{}", material_slug(material));
        write_layer(
            root,
            "fragments",
            &name,
            acoustics::synth_fragment_collision(FragmentCollisionParams {
                speed_mps: 20.0 + index as f32 * 4.0,
                material,
                bearing_deg: [-58.0, 46.0, -22.0, 71.0][index],
                distance_m: 0.65 + index as f32 * 0.18,
                seed: 0x4652_4100 + index as u32,
            }),
            &mut cues,
            &mut manifest,
        )?;
    }
    for (name, from, to, seed) in [
        ("fragment_whizz_left_to_right", -72.0, 68.0, 0x575A_0001),
        ("fragment_whizz_right_to_left", 74.0, -61.0, 0x575A_0002),
    ] {
        write_layer(
            root,
            "fragments",
            name,
            acoustics::synth_fragment_whizz(FragmentWhizzParams {
                speed_mps: 9.5,
                from_bearing_deg: from,
                to_bearing_deg: to,
                closest_distance_m: 0.28,
                seed,
            }),
            &mut cues,
            &mut manifest,
        )?;
    }
    let contacts = [
        FragmentCollisionParams {
            speed_mps: 24.0,
            material: FragmentMaterial::ArmorPlate,
            bearing_deg: -48.0,
            distance_m: 0.72,
            seed: 0xBA7C_0001,
        },
        FragmentCollisionParams {
            speed_mps: 17.0,
            material: FragmentMaterial::TurretFloor,
            bearing_deg: 39.0,
            distance_m: 1.10,
            seed: 0xBA7C_0002,
        },
    ];
    write_layer(
        root,
        "fragments",
        "fragment_two_contact_runtime_batch",
        acoustics::synth_fragment_collision_batch(&contacts),
        &mut cues,
        &mut manifest,
    )?;

    // Shipping gameplay uses the body-only form, then layers real hot-cube whizzes/collisions from
    // physics. Keep these beside the sfx tool's authored fallback composites so the distinction is
    // directly audible rather than merely documented.
    for (name, signature, seed) in [
        (
            "runtime_body_76mm_distant_frontal_ricochet",
            ImpactSignature::DistantFrontalRicochet,
            0x7600_0001,
        ),
        (
            "runtime_body_76mm_long_range_marginal",
            ImpactSignature::LongRangeT34Marginal,
            0x7600_0002,
        ),
        (
            "runtime_body_76mm_close_side_penetration",
            ImpactSignature::CloseT34SidePenetration,
            0x7600_0003,
        ),
        (
            "runtime_body_122mm_heavy_oblique_penetration",
            ImpactSignature::HeavyObliquePenetration,
            0x1220_0004,
        ),
    ] {
        let (params, context) = acoustics::impact_signature_params(signature, seed);
        write_layer(
            root,
            "impacts",
            name,
            acoustics::synth_damage_impact_body(params, context).main,
            &mut cues,
            &mut manifest,
        )?;
    }

    for (name, intensity, seed) in [
        ("hearing_trauma_moderate", 0.72, 0x5449_0001),
        ("hearing_trauma_heavy", 1.28, 0x5449_0002),
    ] {
        write_layer(
            root,
            "impacts",
            name,
            acoustics::synth_tinnitus(intensity, seed),
            &mut cues,
            &mut manifest,
        )?;
    }

    // Render the rebuilt destruction sources directly from the same dry API used by live world
    // propagation. The fifth cue is the exact catastrophic two-layer stack scheduled by gameplay,
    // making it easy to catch doubled pressure, ringing metal or an accidental noise wash.
    for (name, sound) in [
        ("destruction_dry_kill", ProceduralSound::KillExplosion),
        ("destruction_dry_turret_pop", ProceduralSound::TurretPop),
        ("destruction_dry_turret_rip", ProceduralSound::TurretRip),
        ("destruction_dry_cook_off", ProceduralSound::CookOff),
    ] {
        write_layer(
            root,
            "fire-cookoff",
            name,
            acoustics::synth_procedural_dry(sound),
            &mut cues,
            &mut manifest,
        )?;
    }
    let pop = acoustics::synth_procedural_dry(ProceduralSound::TurretPop);
    let rip = acoustics::synth_procedural_dry(ProceduralSound::TurretRip);
    let mut catastrophic = vec![0.0f32; pop.len().max(rip.len())];
    for (output, sample) in catastrophic.iter_mut().zip(pop.iter()) {
        *output += *sample;
    }
    for (output, sample) in catastrophic.iter_mut().zip(rip.iter()) {
        *output += *sample;
    }
    acoustics::soft_limit_in_place(&mut catastrophic, 0.96);
    write_layer(
        root,
        "fire-cookoff",
        "destruction_dry_catastrophic_pop_plus_rip",
        catastrophic,
        &mut cues,
        &mut manifest,
    )?;

    for (name, range, bearing, seed) in [
        ("gun_report_close_left", 90.0, -58.0, 0x4752_0001),
        ("gun_report_medium_front", 280.0, 0.0, 0x4752_0002),
        ("gun_report_distant_right", 720.0, 62.0, 0x4752_0003),
    ] {
        write_layer(
            root,
            "weapons",
            name,
            acoustics::synth_gun_report(88.0, range, bearing, seed),
            &mut cues,
            &mut manifest,
        )?;
    }

    let mut file = io::BufWriter::new(fs::File::create(
        root.join("generated-layers").join("manifest.csv"),
    )?);
    writeln!(file, "name,category,duration_seconds,peak,rms")?;
    for (name, category, duration, peak, rms) in manifest {
        writeln!(file, "{name},{category},{duration:.6},{peak:.6},{rms:.6}")?;
    }
    Ok(cues)
}

fn category_for_runtime(name: &str) -> &'static str {
    match name {
        "engine_loop" | "engine_death" => "engine",
        "ambience_interior" | "ambience_exterior" => "ambience",
        "turret_traverse_loop" => "turret",
        "impact_ground" | "impact_tank" | "armor_clang_1" | "armor_clang_2" | "penetration_hit"
        | "near_miss" => "impacts",
        "kill_explosion" | "turret_pop" | "turret_rip" | "cook_off" => "fire-cookoff",
        "breech_clack" | "brass_clatter" | "hammer_strike" | "pickup" | "alarm_shout" => {
            "interaction-crew"
        }
        _ => "weapons",
    }
}

fn runtime_bank_cues(root: &Path) -> io::Result<Vec<Cue>> {
    let bank = root.join("runtime-bank");
    let manifest = fs::read_to_string(bank.join("manifest.csv"))?;
    let mut cues = Vec::new();
    for line in manifest
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let name = line.split(',').next().unwrap().trim();
        let path = bank.join(format!("{name}.wav"));
        if !path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("runtime manifest names missing WAV {}", path.display()),
            ));
        }
        cues.push(Cue {
            category: category_for_runtime(name),
            name: format!("runtime_{name}"),
            path,
        });
    }
    Ok(cues)
}

fn sfx_cues(root: &Path) -> io::Result<Vec<Cue>> {
    let dir = root.join("impact-signatures");
    let names = [
        "live_76mm_long_range_front_ricochet",
        "live_76mm_long_range_oblique_weak",
        "live_76mm_close_right_side_penetration",
        "live_122mm_heavy_oblique_filled_penetration",
        "live_mg_gravel_right",
    ];
    names
        .into_iter()
        .map(|name| {
            let path = dir.join(format!("{name}.wav"));
            if !path.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("sfx tool output missing {}", path.display()),
                ));
            }
            Ok(Cue {
                category: if name.contains("mg_gravel") {
                    "weapons"
                } else {
                    "impacts"
                },
                name: name.to_string(),
                path,
            })
        })
        .collect()
}

fn read_wav(path: &Path) -> io::Result<Pcm> {
    let bytes = fs::read(path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a RIFF/WAVE file",
        ));
    }
    let mut cursor = 12usize;
    let mut format = None;
    let mut data = None;
    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        let start = cursor + 8;
        let end = start.saturating_add(size).min(bytes.len());
        if id == b"fmt " && size >= 16 {
            format = Some((
                u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()),
                u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()),
                u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()),
                u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap()),
            ));
        } else if id == b"data" {
            data = Some((start, end));
        }
        cursor = end + (size & 1);
    }
    let (encoding, channels, sample_rate, bits) =
        format.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing fmt chunk"))?;
    if encoding != 1 || channels != 2 || sample_rate != SAMPLE_RATE || bits != 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected PCM16 stereo 44100 Hz, found encoding={encoding} channels={channels} rate={sample_rate} bits={bits}"),
        ));
    }
    let (start, end) =
        data.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing data chunk"))?;
    let samples = bytes[start..end]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32767.0)
        .collect();
    Ok(Pcm {
        sample_rate,
        channels,
        samples,
    })
}

fn rel<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

fn build_master(root: &Path, cues: &[Cue]) -> io::Result<()> {
    let gap_frames = (MASTER_GAP_S * SAMPLE_RATE as f32) as usize;
    let lead_frames = (MASTER_LEAD_S * SAMPLE_RATE as f32) as usize;
    let mut master = vec![0.0f32; lead_frames * 2];
    let mut cue_rows = Vec::new();
    let mut position_frames = lead_frames;
    for (index, cue) in cues.iter().enumerate() {
        let pcm = read_wav(&cue.path)?;
        let source_peak = peak(&pcm.samples);
        if source_peak <= f32::EPSILON {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "silent cue in audition",
            ));
        }
        let gain = MASTER_TARGET_PEAK / source_peak;
        let start = position_frames as f32 / SAMPLE_RATE as f32;
        master.extend(pcm.samples.iter().map(|sample| sample * gain));
        position_frames += pcm.samples.len() / 2;
        let end = position_frames as f32 / SAMPLE_RATE as f32;
        cue_rows.push((index + 1, start, end, source_peak, gain, cue));
        master.resize(master.len() + gap_frames * 2, 0.0);
        position_frames += gap_frames;
    }
    master.resize(master.len() + lead_frames * 2, 0.0);
    acoustics::write_wav_stereo(
        root.join("Kessel_full_sound_audition.wav"),
        &master,
        SAMPLE_RATE,
    )?;

    let mut csv = io::BufWriter::new(fs::File::create(root.join("AUDITION_CUES.csv"))?);
    writeln!(csv, "index,start_seconds,end_seconds,duration_seconds,category,name,source_file,source_peak,master_gain,master_peak")?;
    for (index, start, end, source_peak, gain, cue) in &cue_rows {
        writeln!(
            csv,
            "{index},{start:.3},{end:.3},{:.3},{},{},{},{source_peak:.6},{gain:.6},{MASTER_TARGET_PEAK:.6}",
            end - start,
            cue.category,
            cue.name,
            rel(root, &cue.path).display()
        )?;
    }

    let mut md = io::BufWriter::new(fs::File::create(root.join("AUDITION_CUES.md"))?);
    writeln!(md, "# Kessel full sound audition cues\n")?;
    writeln!(md, "Master: `Kessel_full_sound_audition.wav`\n")?;
    writeln!(md, "Every source clip is peak-normalized independently to -6.0 dBFS in the master; the individual WAV files remain at the exact level produced by the game DSP. Cues have {:.2} s of silence between them.\n", MASTER_GAP_S)?;
    writeln!(md, "| # | Start | End | Category | Cue | Source |")?;
    writeln!(md, "|---:|---:|---:|---|---|---|")?;
    for (index, start, end, _, _, cue) in cue_rows {
        writeln!(
            md,
            "| {index} | {start:.3} | {end:.3} | {} | `{}` | `{}` |",
            cue.category,
            cue.name,
            rel(root, &cue.path).display()
        )?;
    }
    Ok(())
}

fn collect_wavs(path: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_wavs(&path, out)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("wav") {
            out.push(path);
        }
    }
    Ok(())
}

fn write_validation(root: &Path, master_cue_count: usize) -> io::Result<()> {
    let mut wavs = Vec::new();
    collect_wavs(root, &mut wavs)?;
    wavs.sort();
    let mut maximum = 0.0f32;
    let mut clipped = 0usize;
    let mut total_frames = 0usize;
    for wav in &wavs {
        let pcm = read_wav(wav)?;
        if pcm.channels != 2 || pcm.sample_rate != SAMPLE_RATE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "WAV format drift",
            ));
        }
        maximum = maximum.max(peak(&pcm.samples));
        clipped += pcm
            .samples
            .iter()
            .filter(|sample| sample.abs() >= 1.0)
            .count();
        total_frames += pcm.samples.len() / 2;
    }
    if clipped != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("validation found {clipped} clipped samples"),
        ));
    }
    let master = read_wav(&root.join("Kessel_full_sound_audition.wav"))?;
    let mut report = io::BufWriter::new(fs::File::create(root.join("VALIDATION.md"))?);
    writeln!(report, "# Audition package validation\n")?;
    writeln!(report, "- WAV files checked: {}", wavs.len())?;
    writeln!(report, "- Master cue count: {master_cue_count}")?;
    writeln!(
        report,
        "- Format: 16-bit PCM, 2 channels, {} Hz",
        SAMPLE_RATE
    )?;
    writeln!(
        report,
        "- Highest individual sample peak: {:.6} ({:.2} dBFS)",
        maximum,
        20.0 * maximum.log10()
    )?;
    writeln!(
        report,
        "- Master peak: {:.6} ({:.2} dBFS)",
        peak(&master.samples),
        20.0 * peak(&master.samples).log10()
    )?;
    writeln!(report, "- Clipped samples: {clipped}")?;
    writeln!(
        report,
        "- Aggregate checked duration: {:.2} seconds",
        total_frames as f32 / SAMPLE_RATE as f32
    )?;
    writeln!(report, "\nPASS: every WAV decoded as stereo 44.1 kHz PCM16 and no sample reached digital full scale.")?;
    Ok(())
}

fn main() -> io::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts/audio/game-audition-2026-08-27"));
    fs::create_dir_all(&root)?;

    let mut cues = runtime_bank_cues(&root)?;
    cues.extend(render_generated_layers(&root)?);
    cues.extend(sfx_cues(&root)?);
    build_master(&root, &cues)?;
    write_validation(&root, cues.len())?;
    println!(
        "[audition] PASS — {} master cues, outputs in {}",
        cues.len(),
        root.display()
    );
    Ok(())
}
