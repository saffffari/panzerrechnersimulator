//! `sound-world <outdir>`: a CPU-only proof of the traced acoustics and the foley catalogue,
//! rendered through the ordinary calibrated mixer (`Audio::new_capture`) to WAV, with an mp4 of
//! the waveform and a JSON receipt of every measured quantity.
//!
//! The rooms are traced through the real Tiger source meshes (voxelized at 5 cm, as in the game),
//! a flat snow field and, where stated, one brick wall; nothing about a sound's level is authored
//! here: each comes from `sound_propagation` and the cue's reference.

use super::game_acoustics::{engine_direct_share, CabinVoxels, CABIN_ROOM_CONSTANT_M2};
use super::*;
use crate::acoustics::foley::{Cue, Gait, Ground as FootGround};
use crate::acoustics::raytrace::{self as rt, AcousticScene, BoxScene, Hit, Material, StereoIr, TraceSettings};
use crate::sound_propagation::{propagate, Atmosphere, Emission, Enclosure, Path, Transmission};
use serde_json::{json, Value};

/// Tiger voxels plus a flat ground and optional brick boxes, in the world frame.
struct ProofScene<'a> {
    voxels: &'a CabinVoxels,
    models: [Mat4; surface_fire::FRAMES],
    inverses: [Mat4; surface_fire::FRAMES],
    boxes: BoxScene,
    ground: Option<Material>,
    tiger: bool,
}

impl AcousticScene for ProofScene<'_> {
    fn first_hit(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<Hit> {
        let mut best = if self.tiger { self.voxels.hit(&self.models, &self.inverses, origin, dir, max_distance.min(30.0)) } else { None };
        if let Some(h) = self.boxes.first_hit(origin, dir, max_distance) {
            if best.is_none_or(|b| h.distance < b.distance) { best = Some(h); }
        }
        if let Some(material) = self.ground {
            if dir.y < -1.0e-6 {
                let t = -origin.y / dir.y;
                if t > 1.0e-5 && t <= max_distance && best.is_none_or(|b| t < b.distance) {
                    best = Some(Hit { distance: t, normal: Vec3::Y, material });
                }
            }
        }
        best
    }
}

struct Reel {
    audio: crate::audio::Audio,
    samples: Vec<f32>,
}

impl Reel {
    fn now_s(&self) -> f64 {
        self.samples.len() as f64 / 2.0 / f64::from(crate::acoustics::SAMPLE_RATE)
    }
    fn run(&mut self, seconds: f32) {
        let frames = (seconds * crate::acoustics::SAMPLE_RATE as f32) as usize;
        if let Some(pcm) = self.audio.render_capture_frames(frames) { self.samples.extend(pcm); }
    }
    fn energy_since(&self, start_s: f64) -> (f64, f64) {
        let start = (start_s * f64::from(crate::acoustics::SAMPLE_RATE)) as usize * 2;
        let (mut l, mut r) = (0.0f64, 0.0f64);
        for f in self.samples[start.min(self.samples.len())..].chunks_exact(2) {
            l += f64::from(f[0] * f[0]);
            r += f64::from(f[1] * f[1]);
        }
        (l, r)
    }
}

fn foley_ear(cue: Cue, distance: f32, enclosure: Enclosure, headset: bool) -> crate::sound_propagation::EarLevel {
    let e = Emission { lpk_db: cue.ref_dba_1m() + 22.0, laf_db: cue.ref_dba_1m(), ref_m: 1.0,
        law: crate::sound_levels::DistanceLaw::Spherical, class: crate::sound_levels::SpectrumClass::Mechanical,
        spectrum: crate::sound_propagation::class_spectrum(crate::sound_levels::SpectrumClass::Mechanical),
        a_duration_ms: 1.0, tnt_kg: 0.0, directivity_db: 0.0 };
    let mut ear = propagate(&e, &Path { distance_m: distance, atmosphere: Atmosphere::WINTER, occluded: false,
        enclosure, headset });
    if matches!(enclosure, Enclosure::OnFoot) { ear.transmission = Transmission::Direct; }
    ear
}

pub fn run(outdir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    crate::sound_propagation::set_active_atmosphere(Atmosphere::WINTER);
    let (parts, _, _, _, _, _, _, _) = tiger_hatches::load_player_tiger()?;
    let lid = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hatch.glb")?;
    let cabin = surface_fire::CabinFire::new(&[(0, &parts[0]), (1, &parts[1]), (2, &parts[2]), (6, &lid)]);
    let voxels = CabinVoxels::build(&cabin);
    println!("[sound-world] cabin voxels: {} frames from {} triangles in {:.0} ms", voxels.frames.len(), voxels.triangles, voxels.build_ms);
    let hf = Heightfield { res: 2, spacing: 2000.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0,
        openings: Default::default(), detail: None };
    let tank = Tank::new(0.0, 0.0, 0.0, &hf, tank::TIGER_PARTS);
    let (hull, turret) = (tank.hull_model(), tank.turret_model());
    let models_at = |cupola: f32| {
        let mut m = [hull; surface_fire::FRAMES];
        m[1] = turret; m[2] = tank.gun_model(); m[6] = cupola_hatch_model(turret, cupola);
        m
    };
    let eye = turret.transform_point3(tiger_periscopes::COMMANDER_EYE - Vec3::Y * 0.1);
    let forward = hull.transform_vector3(Vec3::Z).normalize();
    let right = forward.cross(Vec3::Y);
    let air = Atmosphere::WINTER.air_absorption_bands_db_per_m();
    let c = Atmosphere::WINTER.speed_of_sound();
    let mut reel = Reel { audio: crate::audio::Audio::new_capture(), samples: Vec::new() };
    let mut cases: Vec<Value> = Vec::new();

    // ── 1. One T-34 report, five places ────────────────────────────────────────────────────────
    let shell = &crate::ballistics::BR_350B;
    let charge = crate::sound_levels::propellant_charge_kg(shell.caliber_mm, shell.mass_kg, shell.muzzle_velocity);
    let braked = crate::sound_levels::gun_is_braked(shell.caliber_mm, shell.muzzle_velocity);
    let report: Vec<f32> = crate::acoustics::synth_gun_report(shell.caliber_mm, 0.0, 0.0, 0x7034)
        .chunks_exact(2).map(|f| (f[0] + f[1]) * std::f32::consts::FRAC_1_SQRT_2).collect();
    // A 60 m brick wall standing beside the listener (its z span centred on him).
    let brick = |x0: f32| BoxScene { solids: vec![(Vec3::new(x0, 0.0, 10.0), Vec3::new(x0 + 0.6, 7.0, 70.0), Material::Masonry)], rooms: Vec::new() };
    struct Place { name: &'static str, listener: Vec3, source_dir: Vec3, range: f32, enclosure: Enclosure,
        cupola: f32, tiger: bool, wall: Option<f32>, virtual_source: Vec3, rays: u32, cabin: bool }
    let field = Vec3::new(40.0, 1.7, 40.0);
    let places = [
        Place { name: "buttoned_report_from_right", listener: eye, source_dir: right, range: 100.0, enclosure: Enclosure::Buttoned,
            cupola: 0.0, tiger: true, wall: None, virtual_source: eye + right * 0.45, rays: 768, cabin: true },
        Place { name: "cupola_open_report_from_right", listener: eye, source_dir: right, range: 100.0, enclosure: Enclosure::HatchOpen,
            cupola: 1.0, tiger: true, wall: None,
            virtual_source: turret.transform_point3(tiger_periscopes::COMMANDER_EYE + Vec3::Y * 0.25), rays: 768, cabin: true },
        Place { name: "head_out_report_from_right", listener: eye + Vec3::Y * 0.65, source_dir: right, range: 100.0, enclosure: Enclosure::HeadOut,
            cupola: 1.0, tiger: true, wall: None, virtual_source: eye + Vec3::Y * 0.65 + right * 60.0, rays: 2048, cabin: false },
        Place { name: "on_foot_brick_wall_right_report_ahead", listener: field, source_dir: Vec3::Z, range: 60.0, enclosure: Enclosure::OnFoot,
            cupola: 1.0, tiger: false, wall: Some(field.x - 3.6), virtual_source: field + Vec3::Z * 60.0, rays: 2048, cabin: false },
        Place { name: "on_foot_open_snow_report_ahead", listener: field, source_dir: Vec3::Z, range: 60.0, enclosure: Enclosure::OnFoot,
            cupola: 1.0, tiger: false, wall: None, virtual_source: field + Vec3::Z * 60.0, rays: 2048, cabin: false },
    ];
    assert!(Vec3::Z.cross(Vec3::Y).x < 0.0, "right is −X for a +Z head");
    for place in &places {
        let models = models_at(place.cupola);
        let scene = ProofScene { voxels: &voxels, models, inverses: models.map(|m| m.inverse()),
            boxes: place.wall.map_or_else(BoxScene::default, brick), ground: Some(Material::Snow), tiger: place.tiger };
        let settings = if place.cabin {
            TraceSettings { rays: place.rays, max_order: 40, max_time_s: 0.45, receiver_radius_m: 0.25, speed_m_s: c, air_db_per_m: air, seed: 21 }
        } else {
            TraceSettings::outdoor(place.rays, place.virtual_source.distance(place.listener), c, air, 21)
        };
        let gram = rt::trace(&scene, place.virtual_source, place.listener, Vec3::Z, &settings);
        let ir = StereoIr::synthesize(&gram, 9);
        let path = Path { distance_m: place.range, atmosphere: Atmosphere::WINTER, occluded: false,
            enclosure: place.enclosure, headset: place.enclosure != Enclosure::OnFoot };
        let mut ear = propagate(&Emission::gun(charge, braked, 90.0), &path);
        if place.enclosure == Enclosure::HeadOut { ear.transmission = Transmission::Direct; }
        let bearing = bearing_deg_from(Vec3::Z, place.source_dir);
        let start = reel.now_s();
        reel.audio.play_world_mono_in_room(&report, &ear, bearing, crate::audio::Room::Traced(&ir));
        reel.run(2.6);
        let (l, r) = reel.energy_since(start);
        let laf = crate::reproduction::laf_max_dbfs(&reel.samples[(start * 88_200.0) as usize..]);
        reel.run(0.8);
        println!("[sound-world] {}: reflected {:+.1} dB at 1 kHz, T20 {:?} s, lateral {:+.2}, escaped {:.2}, L/R {:+.1} dB",
            place.name, gram.reflected_db()[5], gram.decay_times_s()[5], gram.mean_lateral(), gram.escaped,
            10.0 * (r / l.max(1e-20)).log10());
        cases.push(json!({"case": place.name, "starts_s": start, "ear_laf_db": ear.laf_db, "bearing_deg": bearing,
            "enclosure": format!("{:?}", place.enclosure), "trace": gram.summary(), "ir_samples": ir.len(),
            "rendered_right_over_left_db": 10.0 * (r / l.max(1e-20)).log10(), "rendered_laf_max_dbfs": laf}));
    }

    // ── 2. A footstep where you are ────────────────────────────────────────────────────────────
    let timber_room = BoxScene { solids: Vec::new(), rooms: vec![rt::Room { min: Vec3::new(30.0, 0.0, 30.0),
        max: Vec3::new(34.0, 2.8, 35.0), material: Material::Timber, openings: vec![(2, 0, [31.5, 32.5], [0.0, 2.1])] }] };
    let steps = [
        ("loader_boots_on_turret_floor", FootGround::TurretFloor, eye - Vec3::Y * 0.6, true, BoxScene::default(), None),
        ("boots_on_timber_floor_indoors", FootGround::WoodFloor, Vec3::new(32.0, 1.7, 32.5), false, timber_room, None),
        ("boots_in_fresh_snow_open_field", FootGround::FreshSnow, field, false, BoxScene::default(), Some(Material::Snow)),
    ];
    for (name, ground, listener, cabin, boxes, floor) in steps {
        let models = models_at(0.0);
        let scene = ProofScene { voxels: &voxels, models, inverses: models.map(|m| m.inverse()), boxes,
            ground: floor.or(Some(Material::Timber)), tiger: cabin };
        let feet = listener - Vec3::Y * 1.55 + Vec3::Z * 0.2;
        let settings = if cabin {
            TraceSettings { rays: 768, max_order: 40, max_time_s: 0.45, receiver_radius_m: 0.25, speed_m_s: c, air_db_per_m: air, seed: 4 }
        } else {
            TraceSettings { rays: 2048, max_order: 60, max_time_s: 1.0, receiver_radius_m: 0.5, speed_m_s: c, air_db_per_m: air, seed: 4 }
        };
        let gram = rt::trace(&scene, feet, listener, Vec3::Z, &settings);
        let ir = StereoIr::synthesize(&gram, 3);
        let cue = Cue::Step(ground, Gait::Walk);
        let start = reel.now_s();
        for k in 0..6u32 {
            let ear = foley_ear(cue, 1.6, Enclosure::OnFoot, cabin);
            reel.audio.play_foley_at_ear(cue, 1.0, 100 + k, &ear, if k % 2 == 0 { 350.0 } else { 10.0 }, crate::audio::Room::Traced(&ir));
            reel.run(0.55);
        }
        reel.run(0.8);
        println!("[sound-world] {name}: reflected {:+.1} dB, T20 {:?} s", gram.reflected_db()[5], gram.decay_times_s()[5]);
        cases.push(json!({"case": name, "starts_s": start, "cue": cue.name(), "trace": gram.summary()}));
    }

    // ── 3. The engine from each station ────────────────────────────────────────────────────────
    let engine = hull.transform_point3(ModuleKind::Engine.station());
    let stations: Vec<Value> = [("driver", hull.transform_point3(Vec3::new(-0.75, 1.75, 1.95))),
        ("radio_operator", hull.transform_point3(Vec3::new(0.75, 1.75, 1.95))),
        ("gunner", turret.transform_point3(Vec3::new(-0.45, 2.0, 0.55))),
        ("commander", eye), ("loader", turret.transform_point3(Vec3::new(0.45, 1.9, -0.2)))]
        .iter().map(|(name, at)| {
            let r = engine.distance(*at);
            let energy = |r: f32| 2.0 / (4.0 * std::f32::consts::PI * r.max(0.5).powi(2)) + 4.0 / CABIN_ROOM_CONSTANT_M2;
            json!({"station": name, "distance_to_engine_m": r, "direct_share": engine_direct_share(r),
                "lateral_retention": engine_direct_share(r).sqrt(),
                "level_offset_db": 10.0 * (energy(r) / energy(1.6)).log10(),
                "bearing_facing_forward_deg": bearing_deg_from(forward, engine - *at)})
        }).collect();

    // ── 4. The catalogue ───────────────────────────────────────────────────────────────────────
    let catalogue_start = reel.now_s();
    let mut catalogue = Vec::new();
    for (i, cue) in Cue::all().into_iter().enumerate() {
        let bearing = (i as f32 * 37.0) % 360.0;
        let start = reel.now_s();
        let ear = foley_ear(cue, 2.0, Enclosure::OnFoot, false);
        let mono = if cue.is_loop() { cue.render(1.0, i as u32) } else { Vec::new() };
        if cue.is_loop() {
            let clip: Vec<f32> = mono.iter().take(crate::acoustics::SAMPLE_RATE as usize * 2).copied().collect();
            reel.audio.play_world_mono_in_room(&clip, &ear, bearing, crate::audio::Room::Parametric(crate::acoustics::ReverbParams::DIRECT));
            reel.run(2.2);
        } else {
            reel.audio.play_foley_at_ear(cue, 1.0, i as u32, &ear, bearing, crate::audio::Room::Parametric(crate::acoustics::ReverbParams::DIRECT));
            reel.run(0.9);
        }
        catalogue.push(json!({"cue": cue.name(), "starts_s": start, "ref_dba_1m": cue.ref_dba_1m(),
            "ear_laf_db_at_2m": ear.laf_db, "bearing_deg": bearing, "loop": cue.is_loop()}));
    }

    // ── Checks on the measured numbers ─────────────────────────────────────────────────────────
    let get = |name: &str, key: &str| cases.iter().find(|c| c["case"] == name)
        .and_then(|c| c["trace"][key].as_f64().or_else(|| c[key].as_f64())).unwrap_or(f64::NAN);
    let refl = |name: &str| cases.iter().find(|c| c["case"] == name)
        .and_then(|c| c["trace"]["reflected_db"][5].as_f64()).unwrap_or(f64::NAN);
    let checks = json!({
        "buttoned_cabin_rings_hardest": refl("buttoned_report_from_right") > refl("cupola_open_report_from_right"),
        "open_cupola_lets_energy_out": get("cupola_open_report_from_right", "escaped") > get("buttoned_report_from_right", "escaped"),
        "head_out_is_drier_than_inside": refl("head_out_report_from_right") < refl("cupola_open_report_from_right"),
        "report_from_the_right_is_heard_right": get("buttoned_report_from_right", "rendered_right_over_left_db") > 0.0
            && get("head_out_report_from_right", "rendered_right_over_left_db") > 3.0,
        "brick_wall_on_the_right_returns_from_the_right": get("on_foot_brick_wall_right_report_ahead", "mean_lateral") > 0.05,
        "brick_wall_adds_reflected_energy": refl("on_foot_brick_wall_right_report_ahead") > refl("on_foot_open_snow_report_ahead"),
        "turret_floor_rings_longer_than_snow": refl("loader_boots_on_turret_floor") > refl("boots_in_fresh_snow_open_field"),
        "catalogue_has_hundreds": catalogue.len() >= 300,
    });
    let passed = checks.as_object().is_some_and(|c| c.values().all(|v| v.as_bool() == Some(true)));
    let wav = format!("{outdir}/sound_world_20261028.wav");
    crate::acoustics::write_wav_stereo(&wav, &reel.samples, crate::acoustics::SAMPLE_RATE)?;
    let mp4 = format!("{outdir}/sound_world_20261028.mp4");
    let status = std::process::Command::new("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-y", "-i", &wav,
        "-filter_complex", "[0:a]showwaves=s=1280x360:mode=cline:split_channels=1:colors=0xd8c08a|0x8ab8d8,format=yuv420p[v]",
        "-map", "[v]", "-map", "0:a", "-c:v", "libx264", "-preset", "veryfast", "-c:a", "aac", "-b:a", "192k", &mp4]).status();
    let receipt = json!({"schema": "panzer.sound-world-proof.v1",
        "model": {"rooms": "acoustics::raytrace stochastic energy ray tracing per octave band, Tiger source triangles voxelized at 5 cm, snow ground, brick boxes; binaural IR from the echogram",
            "foley": "acoustics::foley contact synthesis, levels from each cue's 1 m reference through sound_propagation",
            "engine": "direction from the engine compartment; inside, lateral retention sqrt(direct share) and the Hopkinson-Stryker level offset with R = 12 m2 [E]"},
        "cabin_voxels": {"frames": voxels.frames.len(), "source_triangles": voxels.triangles, "build_ms": voxels.build_ms,
            "note": "proof loads hull, turret, gun and cupola lid meshes only; the game's roof lids and visor shutters are absent here, so their openings are open"},
        "cases": cases, "engine_by_station": stations, "catalogue_starts_s": catalogue_start, "catalogue": catalogue,
        "checks": checks, "passed": passed, "wav": wav, "mp4": mp4, "mp4_encoded": status.map(|s| s.success()).unwrap_or(false),
        "perceptually_auditioned": false});
    let path = format!("{outdir}/sound_world_20261028.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&receipt)?)?;
    println!("[sound-world] {} cues, {:.0} s of audio; checks passed: {passed}; receipt {path}", Cue::all().len(), reel.now_s());
    anyhow::ensure!(passed, "sound-world checks failed: {checks}");
    Ok(())
}
