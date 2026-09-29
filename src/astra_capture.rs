//! Repeatable interior films, with source-derived labels and actual terminal-event audio.
//! Every case constructs a fresh tank; the fixed impact seed is never searched for a casualty.

use super::*;
use std::io::Write;

const FPS: u32 = 24;
const SECONDS: f32 = 8.0;
const HIT_T: f32 = 2.0;
const CW: u32 = 1280;
const CH: u32 = 720;
const MASTER_W: u32 = 1920;
const MASTER_H: u32 = 1080;

fn capture_pause_ms() -> u64 {
    std::env::var("KESSEL_CAPTURE_FRAME_PAUSE_MS")
        .ok().and_then(|value| value.parse::<u64>().ok()).unwrap_or(0).min(5_000)
}

fn capture_frame(
    r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, pause_ms: u64,
) -> anyhow::Result<Vec<u8>> {
    let started = Instant::now();
    // The RGBA capture waits for GPU completion and mapped readback before returning. Rest only
    // after that wait, so queued GPU work cannot continue saturating the desktop during the pause.
    let rgba = r.render_to_rgba_with_hud(device, queue, CW, CH)?;
    if pause_ms > 0 {
        let rest = started.elapsed().mul_f32(3.0)
            .max(std::time::Duration::from_millis(pause_ms))
            .min(std::time::Duration::from_secs(5));
        std::thread::sleep(rest);
    }
    Ok(rgba)
}

fn impact_params_json(p: acoustics::ImpactParams) -> serde_json::Value {
    serde_json::json!({
        "shell":format!("{:?}",p.shell), "caliber_mm":p.caliber_mm, "mass_kg":p.mass_kg,
        "velocity":p.velocity, "plate_thickness_mm":p.plate_thickness_mm,
        "effective_mm":p.effective_mm, "impact_angle_deg":p.impact_angle_deg,
        "residual_v":p.residual_v, "behind_armor":p.behind_armor,
        "panel":format!("{:?}",p.panel), "penetrated":p.penetrated,
        "bearing_deg":p.bearing_deg, "station_dist_m":p.station_dist_m, "seed":p.seed,
    })
}

fn damage_sound_context_json(c: acoustics::DamageSoundContext) -> serde_json::Value {
    serde_json::json!({
        "casualty_count":c.casualty_count, "module_hits":c.module_hits,
        "internal_detonation":c.internal_detonation, "catastrophic":c.catastrophic,
    })
}

fn save_master(path: &str, rgba: &[u8]) -> anyhow::Result<()> {
    let frame = image::RgbaImage::from_raw(CW, CH, rgba.to_vec())
        .ok_or_else(|| anyhow::anyhow!("wrong master frame size"))?;
    image::imageops::resize(&frame, MASTER_W, MASTER_H, image::imageops::FilterType::Nearest).save(path)?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum View { Witness, Loader, Gunner, Driver, Commander }

#[derive(Clone, Copy)]
struct Case {
    slug: &'static str,
    shell: &'static ballistics::ShellSpec,
    range: f32,
    left: bool,
    cupola: bool,
    loader_hatch: bool,
    driver_hatch: bool,
    view: View,
}

fn cases(calibers: bool) -> Vec<Case> {
    let base = Case { slug: "", shell: &ballistics::BR_350B, range: 300.0, left: false,
        cupola: true, loader_hatch: false, driver_hatch: false, view: View::Witness };
    if calibers {
        [
            ("01_45mm_br240_m42", &ballistics::BR_240_M42),
            ("02_57mm_br271_zis2", &ballistics::BR_271_ZIS2),
            ("03_76mm_br350b", &ballistics::BR_350B),
            ("04_76mm_br354p_apcr", &ballistics::BR_354P),
            ("05_85mm_br365", &ballistics::BR_365),
            ("06_100mm_br412", &ballistics::BR_412),
            ("07_122mm_br471", &ballistics::BR_471),
            ("08_152mm_br540", &ballistics::BR_540),
        ].into_iter().map(|(slug, shell)| Case { slug, shell, ..base }).collect()
    } else {
        vec![
            Case { slug: "01_witness_buttoned_76mm_right", range: 50.0, cupola: false, ..base },
            Case { slug: "02_witness_cupola_open_85mm_left", shell: &ballistics::BR_365, left: true, ..base },
            Case { slug: "03_loader_roof_open_76mm_apcr", shell: &ballistics::BR_354P, view: View::Loader, loader_hatch: true, cupola: false, ..base },
            Case { slug: "04_gunner_cupola_open_122mm_right", shell: &ballistics::BR_471, range: 500.0, view: View::Gunner, ..base },
            Case { slug: "05_driver_hatch_open_57mm", shell: &ballistics::BR_271_ZIS2, range: 500.0, cupola: false, driver_hatch: true, view: View::Driver, ..base },
            Case { slug: "06_commander_roof_hatches_open_152mm_left", shell: &ballistics::BR_540, range: 500.0, left: true, loader_hatch: true, view: View::Commander, ..base },
        ]
    }
}

fn camera(g: &mut Game, r: &mut Renderer, view: View) {
    let role = match view {
        View::Witness | View::Driver => lan::LanRole::Driver,
        View::Loader => lan::LanRole::Loader,
        View::Gunner => lan::LanRole::Gunner,
        View::Commander => lan::LanRole::Commander,
    };
    if let Some(id) = g.couch_role_occupant(role) { g.drive_couch_crew_rigs_for_view(id, r); }
    g.configure_crew_render_view(role, false, r);
    let tm = g.player.turret_model();
    if matches!(view, View::Witness) {
        r.camera.pos = tm.transform_point3(Vec3::new(-0.45, 2.34, -0.84));
        r.camera.set_forward(tm.transform_point3(Vec3::new(0.0, 1.45, 0.18)) - r.camera.pos);
        r.camera.fov_y = 80f32.to_radians();
    } else {
        let (eye, forward, fov, _, _) = g.coop_view_camera(role);
        r.camera.pos = eye;
        // The gunner capture looks beside the sight into the working compartment; the eye is the
        // actual crew station, not an opaque magnified-optic card. Other stations retain their view.
        let forward = if matches!(view, View::Gunner) {
            tm.transform_point3(Vec3::new(-0.12, 1.64, 0.55)) - eye
        } else if matches!(view, View::Commander) {
            tm.transform_point3(Vec3::new(-0.3, 1.65, 0.35)) - eye
        } else { forward };
        r.camera.set_forward(forward);
        r.camera.fov_y = fov.max(72f32.to_radians());
    }
    r.camera.aspect = CW as f32 / CH as f32;
    r.optic_mask = 0.0;
    r.optic_blast = 0.0;
    r.reticle = false;
    r.crew_ko = 0.0;
    r.weather_shelter = 1.0;
    r.set_active_view(4);
}

// Representative platform/gun identities for an isolated terminal-contact test. The fixture
// does not render a firing vehicle or claim that all these platforms shared one historical battle.
// BR-354P retains the repository's unverified designation instead of silently renaming the shell.
fn incoming_profile_label(shell: &ballistics::ShellSpec) -> &'static str {
    match shell.name {
        "45mm BR-240 (M-42)" => "TOWED M-42 / 45MM M-42 / BR-240",
        "57mm BR-271 (ZiS-2)" => "TOWED ZIS-2 / 57MM ZIS-2 / BR-271",
        "76mm BR-350B" => "T-34 / 76.2MM F-34 / BR-350B",
        "76mm BR-354P" => "T-34 / 76.2MM F-34 / BR-354P? APCR",
        "85mm BR-365" => "SU-85 / 85MM D-5S / BR-365",
        "100mm BR-412" => "SU-100 / 100MM D-10S / BR-412",
        "122mm BR-471" => "IS-2 / 122MM D-25T / BR-471",
        "152mm BR-540" => "ISU-152 / 152MM ML-20S / BR-540",
        "152mm OF-530" => "KV-2 / 152MM M-10T / OF-530 HE",
        _ => shell.name,
    }
}

fn label(r: &mut Renderer, case: Case, hit: bool, penetrated: bool) {
    use ui::Align;
    r.fill_rect(0.018, 0.892, 0.68, 0.089, [0.008, 0.009, 0.008, 0.72]);
    r.draw_text_bold(&format!("{} / {:.0} M", incoming_profile_label(case.shell), case.range),
        0.028, 0.902, 0.022, [0.85, 0.81, 0.69, 1.0], Align::Left);
    let state = if !hit { "READY" } else if penetrated { "PENETRATION" } else { "ARMOR HOLDS" };
    r.draw_text(&format!("CONTACT TEST / {} SIDE / {}", if case.left { "LEFT" } else { "RIGHT" }, state),
        0.029, 0.945, 0.025, [0.7, 0.69, 0.63, 1.0], Align::Left);
}

fn audio(
    params: acoustics::ImpactParams,
    context: acoustics::DamageSoundContext,
    contacts: &[(f32, acoustics::FragmentCollisionParams)],
    seed: u32,
) -> anyhow::Result<(Vec<f32>, f32)> {
    let event = acoustics::synth_damage_impact_body(params, context);
    let sr = acoustics::SAMPLE_RATE as f32;
    let mut mix = vec![0.0; (SECONDS * sr) as usize * 2];
    let mut l = 0.0;
    let mut r = 0.0;
    for n in 0..mix.len()/2 {
        l += 0.002 * (terror_noise(seed.wrapping_add(n as u32)) - l);
        r += 0.002 * (terror_noise(seed.wrapping_add(n as u32).wrapping_add(7777)) - r);
        mix[2*n] = l * 0.045;
        mix[2*n+1] = r * 0.045;
    }
    // Same listener laws as gameplay: the bed loses level and detail after the strike; the
    // strike lands untouched, then its tail is heard through the damaged ear.
    let (listen, concussion) = (acoustics::offline_listener, event.profile.concussion);
    listen(&mut mix, -HIT_T, acoustics::ListenerBus::World, concussion, params.bearing_deg);
    let mut strike = event.main.clone();
    listen(&mut strike, 0.0, acoustics::ListenerBus::Impact, concussion, params.bearing_deg);
    terror_mix(&mut mix, &strike, HIT_T, 0.94);
    for &(relative, contact) in contacts {
        let mut fragment = acoustics::synth_fragment_collision_batch(&[contact]);
        listen(&mut fragment, relative, acoustics::ListenerBus::World, concussion,
            params.bearing_deg);
        terror_mix(&mut mix, &fragment, HIT_T+relative, 0.18);
    }
    if !event.tinnitus.is_empty() { terror_mix(&mut mix, &event.tinnitus, HIT_T+0.025, 0.10); }
    // Fixed gain preserves event-to-event level; only an overloaded sum gets linear attenuation.
    // No final tanh: distortion would manufacture harmonics over the new structural modes.
    for sample in &mut mix { *sample *= 0.88 * 0.97; }
    anyhow::ensure!(mix.iter().all(|x| x.is_finite()), "nonfinite review sound");
    let peak_guard_gain = acoustics::attenuate_over_peak(&mut mix, 0.94);
    let (le, re) = acoustics::channel_energy(&mix);
    anyhow::ensure!(le > 0.01 && re > 0.01, "silent review sound");
    Ok((mix, peak_guard_gain))
}

fn mux(raw: &str, wav: &str, output: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("ffmpeg").args([
        "-hide_banner", "-loglevel", "warning", "-y", "-i", raw, "-i", wav,
        "-map", "0:v:0", "-map", "1:a:0", "-vf", "scale=1920:1080:flags=neighbor,format=yuv420p",
        "-c:v", "libx264", "-preset", "medium", "-crf", "15", "-c:a", "aac", "-b:a", "224k",
        "-shortest", "-movflags", "+faststart", output,
    ]).status()?;
    anyhow::ensure!(status.success(), "failed to mux {output}");
    Ok(())
}

#[allow(dead_code)] // Retained for replacement with an actual posed interior mesh diagnostic.
fn damage_appendix(
    stem: &str, case: Case, g: &Game, r: &mut Renderer,
    device: &wgpu::Device, queue: &wgpu::Queue, samples: &[f32],
) -> anyhow::Result<String> {
    let snapshot = g.damage_readout();
    let anatomy = format!("{stem}_anatomy.png");
    r.ui_reset();
    snapshot.draw_anatomy(r, CW as f32 / CH as f32);
    // The dense diagnostic needs the display-resolution HUD path. The menu-only PNG path
    // rasterizes at 320x180, where these small labels lose entire glyph strokes.
    let pause_ms = capture_pause_ms();
    let anatomy_pixels = capture_frame(r, device, queue, pause_ms)?;
    save_master(&anatomy, &anatomy_pixels)?;
    let raw = format!("{stem}_damage_video_only.mp4");
    let wav = format!("{stem}_damage_soundtrack.wav");
    let video = format!("{stem}_damage.mp4");
    let mut encoder = spawn_damage_reel_encoder(&raw, CW, CH)?;
    let mut pipe = encoder.stdin.take().unwrap();
    for _ in 0..FPS*2 { pipe.write_all(&anatomy_pixels)?; }
    let hull = g.player.hull_model();
    let target = hull.transform_point3(Vec3::new(0.0, 1.35, 0.0));
    let set_camera = |r: &mut Renderer, angle: f32| {
        r.camera.pos = hull.transform_point3(Vec3::new(angle.cos()*5.7, 4.9, angle.sin()*5.7));
        r.camera.set_forward(target-r.camera.pos);
        r.camera.fov_y = 58f32.to_radians();
    };
    set_camera(r, -2.45);
    r.reset_history();
    let _ = converge_headless_screenshot(r, device, queue, 24)?;
    for frame in 0..FPS*2 {
        set_camera(r, -2.45 + frame as f32/(FPS*2-1) as f32*0.45);
        r.ui_reset();
        r.fill_rect(0.0, 0.0, 1.0, 1.0, [0.004, 0.012, 0.018, 0.65]);
        snapshot.draw_cutaway(r, r.camera.view_proj());
        r.draw_text(&format!("{} / {:.0} M / CONTACT TEST",incoming_profile_label(case.shell),case.range),
            0.028,0.036,0.019,[0.8,0.78,0.67,1.0],ui::Align::Left);
        let rgba = capture_frame(r, device, queue, pause_ms)?;
        if frame == FPS { save_master(&format!("{stem}_xray.png"), &rgba)?; }
        pipe.write_all(&rgba)?;
    }
    drop(pipe);
    anyhow::ensure!(encoder.wait()?.success(),"damage appendix encoding failed");
    // Replay the same impact and four-second tail while inspecting the retained post-impact state.
    let start = (HIT_T*acoustics::SAMPLE_RATE as f32) as usize*2;
    acoustics::write_wav_stereo(&wav,&samples[start..],acoustics::SAMPLE_RATE)?;
    mux(&raw,&wav,&video)?;
    Ok(video)
}

pub fn run(prefix: &str, calibers: bool) -> anyhow::Result<()> {
    let probe = std::env::var_os("KESSEL_ASTRA_PROBE_ONLY").is_some();
    let pre_impact_only = std::env::var_os("KESSEL_ASTRA_PRE_IMPACT_ONLY").is_some();
    let pause_ms = capture_pause_ms();
    let filter = std::env::var("KESSEL_ASTRA_CASE").ok();
    if let Some(parent) = std::path::Path::new(prefix).parent() { std::fs::create_dir_all(parent)?; }
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let input = damage_reel_crew_input();
    let mut manifest = Vec::new();
    let mut videos = Vec::new();
    for case in cases(calibers).into_iter().filter(|c| filter.as_ref().is_none_or(|f| c.slug.contains(f))) {
        println!("[astra-interiors] starting {}", case.slug);
        let started = Instant::now();
        let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(0))?;
        g.mute_audio();
        g.debug_banish_enemies();
        g.set_lan_role(lan::LanRole::Commander);
        g.player.turret_yaw += 15f32.to_radians();
        g.hatch_open = case.cupola;
        g.commander_outside = false;
        g.debug_set_roof_hatches(case.driver_hatch,case.loader_hatch,true);
        for _ in 0..60 { g.update(&input, 1.0/60.0, &mut r); }
        anyhow::ensure!(g.couch_living_crew() == 4, "review crew did not initialize");
        r.apply_campaign_atmosphere(2, false, DayPhase::Day);
        r.overcast = 0.0;
        r.storm = 0.0;
        r.precipitation = 0.0;
        r.fog_density = 0.0;
        r.sun_dir = g.player.turret_model().transform_vector3(Vec3::new(-0.55, 0.74, -0.39).normalize());
        camera(&mut g, &mut r, case.view);
        r.reset_history();
        let settled = if !probe || pre_impact_only {
            let report = converge_headless_screenshot(&mut r, &device, &queue, 32)?;
            anyhow::ensure!(report.stable, "review exposure did not settle");
            Some(report)
        } else { None };
        let stem = format!("{prefix}_{}", case.slug);
        if pre_impact_only {
            // One converged actual camera image, then leave before impact, video encoding, audio
            // synthesis or the time-stepping render loop. This takes precedence over physics probe.
            r.ui_reset();
            r.begin_gpu_profile_frame();
            let rgba = capture_frame(&mut r, &device, &queue, pause_ms)?;
            let mut gpu_samples = gpu_profile::GpuProfileSummary::default();
            gpu_samples.record(r.read_gpu_profile_frame(&device, &queue)?);
            let image = format!("{stem}_pre_impact.png");
            save_master(&image, &rgba)?;
            let light_layers = if std::env::var_os("KESSEL_ASTRA_LIGHT_LAYERS").is_some() {
                Some(r.debug_capture_light_layers(&device, &queue, &format!("{stem}_pre_impact"))?)
            } else { None };
            let report = settled.expect("pre-impact convergence must run");
            std::fs::write(format!("{prefix}_pre_impact_manifest.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema":"kessel.astra-pre-impact-proof.v1", "image":image, "light_layers":light_layers,
                    "case":case.slug, "view":format!("{:?}",case.view),
                    "camera_world_m":r.camera.pos.to_array(),
                    "camera_forward":r.camera.forward().to_array(),
                    "hatches":{"cupola":case.cupola,"loader":case.loader_hatch,"driver":case.driver_hatch},
                    "native_render_size":[r.iw,r.ih], "capture_size":[CW,CH],
                    "encoded_size":[MASTER_W,MASTER_H], "impact_executed":false,
                    "transport_frames":report.transport_frames, "exposure_frames":report.exposure_frames,
                    "measured_exposure":report.measured_exposure, "exposure_stable":report.stable,
                    "capture_frame_pause_ms":pause_ms,
                    "gpu_pass_profile":gpu_samples.report(),
                    "profile_scope":"one settled camera frame; GPU pass costs only, not gameplay FPS",
                }))?)?;
            println!("[astra-interiors] pre-impact proof saved: {image}; no impact or video run");
            return Ok(());
        }
        let raw = format!("{stem}_video_only.mp4");
        let wav = format!("{stem}_soundtrack.wav");
        let video = format!("{stem}.mp4");
        let mut encoder = if probe { None } else { Some(spawn_damage_reel_encoder(&raw, CW, CH)?) };
        let mut pipe = encoder.as_mut().and_then(|child| child.stdin.take());
        let mut all_contacts = Vec::new();
        let mut peak_grains = 0;
        let mut sound = None;
        let seed = 0xA57A_0905; // identical, unfiltered comparison seed across every caliber
        let mut contact_position = Vec3::ZERO;
        let mut armor_contact = None;
        const DT: f32 = 1.0/120.0;
        let mut frames = 0;
        let mut gpu_samples = gpu_profile::GpuProfileSummary::default();
        for step in 0..(SECONDS/DT).round() as usize {
            let t = step as f32 * DT;
            camera(&mut g, &mut r, case.view);
            if step == (HIT_T/DT).round() as usize {
                let contact = g.debug_review_turret_side_impact(&mut r, case.shell, case.range, case.left, seed)?;
                contact_position = Vec3::from_array(contact.selected.world_m);
                armor_contact = Some(contact.to_json());
                sound = g.debug_last_player_damage_sound();
                println!("[astra-interiors] {}: one {} impact at {:.0} m, {} KIA", case.slug,
                    if sound.unwrap().0.penetrated { "penetrating" } else { "nonpenetrating" }, case.range,
                    4usize.saturating_sub(g.couch_living_crew()));
            }
            g.update(&input, DT, &mut r);
            peak_grains = peak_grains.max(g.active_cubic_blood());
            camera(&mut g, &mut r, case.view);
            if t >= HIT_T {
                all_contacts.extend(g.hot_fragment_collisions().iter().filter_map(|collision|
                    damage_reel_contact(*collision, &g, r.camera.pos, r.camera.forward())
                        .map(|contact| (t-HIT_T, contact))));
            }
            if (step+1)%5 == 0 {
                if !probe {
                    r.ui_reset();
                    label(&mut r, case, sound.is_some(), sound.is_some_and(|s| s.0.penetrated));
                    g.apply_cinematic_camera_impulses(&mut r);
                    if matches!(case.view, View::Witness) {
                        let tm = g.player.turret_model();
                        let mut local = tm.inverse().transform_point3(r.camera.pos);
                        local.y = local.y.min(2.34);
                        r.camera.pos = tm.transform_point3(local);
                    }
                    r.begin_gpu_profile_frame();
                    let rgba = capture_frame(&mut r, &device, &queue, pause_ms)?;
                    gpu_samples.record(r.read_gpu_profile_frame(&device, &queue)?);
                    pipe.as_mut().unwrap().write_all(&rgba)?;
                    if frames == 24 || frames == 59 || frames == 119 {
                        save_master(&format!("{stem}_frame_{frames:03}.png"), &rgba)?;
                    }
                }
                frames += 1;
            }
        }
        anyhow::ensure!(frames == (SECONDS * FPS as f32) as usize, "wrong frame count");
        let (params, context) = sound.ok_or_else(|| anyhow::anyhow!("no recorded impact"))?;
        let contacts = damage_reel_select_contacts(&all_contacts);
        let (samples, audio_peak_guard_gain) = audio(params, context, &contacts, seed)?;
        if !probe {
            drop(pipe);
            anyhow::ensure!(encoder.unwrap().wait()?.success(), "review encoder failed");
            acoustics::write_wav_stereo(&wav, &samples, acoustics::SAMPLE_RATE)?;
            mux(&raw, &wav, &video)?;
            videos.push(video.clone());
            // The rejected circle-based x-ray appendix is deliberately not dispatched. Its code
            // remains available until an actual posed crew/interior mesh view replaces it.
        }
        manifest.push(serde_json::json!({
            "case": case.slug, "video": video, "shell": case.shell.name,
            "representative_weapon_label": incoming_profile_label(case.shell),
            "firing_platform_recorded": false,
            "shell_kind": format!("{:?}",case.shell.kind), "range_m":case.range,
            "caliber_mm":params.caliber_mm, "striking_velocity_mps":params.velocity,
            "residual_velocity_mps":params.residual_v, "penetrated":params.penetrated,
            "contact_world_m":contact_position.to_array(), "impact_side":if case.left {"left"} else {"right"},
            "armor_contact":armor_contact,
            "impact_params":impact_params_json(params),
            "damage_sound_context":damage_sound_context_json(context),
            "audio_fragment_contacts":contacts.iter().map(|(relative,c)| serde_json::json!({
                "relative_to_impact_s":relative, "speed_mps":c.speed_mps,
                "material":format!("{:?}",c.material), "bearing_deg":c.bearing_deg,
                "distance_m":c.distance_m, "seed":c.seed,
            })).collect::<Vec<_>>(),
            "audio_master_fixed_gain":0.88*0.97, "audio_peak_guard_gain":audio_peak_guard_gain,
            "view":format!("{:?}",case.view), "hatches":{"cupola":case.cupola,"loader":case.loader_hatch,"driver":case.driver_hatch},
            "crew_kia":4usize.saturating_sub(g.couch_living_crew()), "peak_blood_grains":peak_grains,
            "frames":frames, "presentation_fps":FPS, "duration_s":SECONDS, "impact_time_s":HIT_T,
            "capture_frame_pause_ms":pause_ms, "wall_seconds":started.elapsed().as_secs_f32(),
            "gpu_pass_profile":gpu_samples.report(),
            "profile_scope":"paced fixed-camera impact review; GPU pass costs only, not gameplay FPS",
            "native_render_size":[r.iw,r.ih], "capture_size":[CW,CH], "encoded_size":[MASTER_W,MASTER_H],
            "seed":seed, "audio_bearing_deg":params.bearing_deg, "audio_station_distance_m":params.station_dist_m,
            "physical_fragment_contacts":all_contacts.len(), "auditioned_contacts":contacts.len(),
        }));
        println!("[astra-interiors] finished {} in {:.1}s",case.slug,started.elapsed().as_secs_f32());
    }
    anyhow::ensure!(!manifest.is_empty(), "no cases matched");
    std::fs::write(format!("{prefix}_manifest.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "schema":"kessel.astra-interior-review.v2", "cases":manifest,
        "simulation":"fresh tank per case; fixed seed; explicit isolated turret-part contact and common terminal resolver; the unfiltered whole-vehicle contact is also recorded because the coarse hull box overlaps empty space beside the turret; analytic range at contact, external flight not simulated",
        "audio":"full recorded impact params and damage context; structural impact/body synth plus serialized selected physical fragment contacts; quiet procedural bed; fixed gain and attenuation-only peak protection; not a capture of the live mixer or validated hatch acoustic transfer",
        "lighting":"same clear daylight and sun orientation across cases; exposure settles per view",
        "historical_scope":"ordnance comparison across the Tiger I service period, not one campaign date; see docs/SOVIET_AMMUNITION_CAPTURE_SOURCES.md",
    }))?)?;
    if videos.len() > 1 {
        let list = format!("{prefix}_concat.txt");
        let mut lines = String::new();
        for path in &videos {
            let path = std::fs::canonicalize(path)?;
            lines.push_str(&format!("file '{}'\n", path.display().to_string().replace('\'', "'\\''")));
        }
        std::fs::write(&list,lines)?;
        let status=std::process::Command::new("ffmpeg").args([
            "-hide_banner","-loglevel","warning","-y","-f","concat","-safe","0","-i",&list,
            "-c","copy","-movflags","+faststart",&format!("{prefix}.mp4")]).status()?;
        anyhow::ensure!(status.success(),"review montage failed");
    }
    Ok(())
}
