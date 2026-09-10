//! Current-build whole-world still sweep. No grading or custom atmosphere/exposure values.
//! One live campaign world per act; combat is frozen after initial scene synchronization.
use super::*;
use glam::Vec2;
use serde_json::{json, Value};
use std::io::Write;

const MOTION_FPS: u32 = 24;

fn validate_motion_selection(
    act: Option<u8>,
    case: Option<&str>,
    seconds: Option<u32>,
) -> anyhow::Result<()> {
    let Some(seconds) = seconds else {
        return Ok(());
    };
    anyhow::ensure!(
        (1..=6).contains(&seconds),
        "KESSEL_ASTRA_SURVEY_VIDEO_SECONDS must be an integer from 1 to 6"
    );
    anyhow::ensure!(
        act.is_some_and(|id| (1..=3).contains(&id)),
        "motion survey requires one explicit KESSEL_ASTRA_SURVEY_ACT (1, 2 or 3)"
    );
    let exact_cases = [
        "landscape_dawn",
        "landscape_day",
        "landscape_dusk",
        "landscape_night",
        "roads_ruins_day",
        "bare_grove_ground_day",
        "landmark_day",
    ];
    anyhow::ensure!(case.is_some_and(|case| exact_cases.contains(&case)),
        "motion survey requires one EXACT KESSEL_ASTRA_SURVEY_CASE; partial/broad case filters are refused");
    Ok(())
}

/// Advance only renderer weather time, the camera and the survey's visual grass/wind stream.
/// Native-size readback avoids transferring an enlarged GPU image; ffmpeg does exact nearest 3x.
fn capture_motion(
    path: &str,
    seconds: u32,
    r: &mut Renderer,
    g: &mut Game,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> anyhow::Result<Value> {
    let (iw, ih) = (r.iw, r.ih);
    let (ow, oh) = (iw * 3, ih * 3);
    anyhow::ensure!(
        ow % 2 == 0 && oh % 2 == 0,
        "native 3x motion dimensions must be even for yuv420p"
    );
    let frames = seconds * MOTION_FPS;
    let pause_ms = std::env::var("KESSEL_CAPTURE_FRAME_PAUSE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        .min(5000);
    let eye = r.camera.pos;
    let forward = r.camera.forward();
    let translation = r.camera.right() * 0.6;
    let start_time = r.time;
    let precipitation = r.precipitation;
    let started = Instant::now();
    let mut encoder = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &format!("{iw}x{ih}"),
            "-framerate",
            &MOTION_FPS.to_string(),
            "-i",
            "-",
            "-an",
            "-vf",
            &format!("scale={ow}:{oh}:flags=neighbor,format=yuv420p"),
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "14",
            "-threads",
            "2",
            "-movflags",
            "+faststart",
            path,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|error| anyhow::anyhow!("starting bounded survey motion encoder: {error}"))?;
    let Some(mut pipe) = encoder.stdin.take() else {
        let _ = encoder.kill();
        let _ = encoder.wait();
        anyhow::bail!("survey motion encoder stdin unavailable");
    };
    let capture = (|| -> anyhow::Result<()> {
        for frame in 0..frames {
            let progress = frame as f32 / (frames - 1) as f32;
            let eased = progress * progress * (3.0 - 2.0 * progress);
            r.time = start_time + frame as f32 / MOTION_FPS as f32;
            r.camera.pos = eye + translation * eased;
            r.camera.set_forward(forward);
            r.weather_shelter = g.building_weather_shelter_at(r.camera.pos);
            g.prepare_world_survey(
                r,
                if frame == 0 {
                    0.0
                } else {
                    1.0 / MOTION_FPS as f32
                },
            );
            let frame_start = Instant::now();
            // This API completes the GPU submission and mapped readback before returning.
            let rgba = r.render_to_rgba(device, queue, iw, ih)?;
            if pause_ms > 0 {
                let rest = frame_start
                    .elapsed()
                    .mul_f32(3.0)
                    .max(std::time::Duration::from_millis(pause_ms))
                    .min(std::time::Duration::from_secs(5));
                std::thread::sleep(rest);
            }
            pipe.write_all(&rgba)?;
            if (frame + 1) % MOTION_FPS == 0 {
                println!(
                    "[astra-survey-motion] {}/{} frames; {:.1}s wall",
                    frame + 1,
                    frames,
                    started.elapsed().as_secs_f32()
                );
            }
        }
        Ok(())
    })();
    drop(pipe);
    if let Err(error) = capture {
        let _ = encoder.kill();
        let _ = encoder.wait();
        return Err(error);
    }
    anyhow::ensure!(
        encoder.wait()?.success(),
        "survey motion encoder failed for {path}"
    );
    Ok(json!({"video":path,"audio":false,"visual_only":true,
        "simulation":"no AI/combat ticks; renderer weather time and camera advance; ordinary grass/wind stream updates",
        "fps":MOTION_FPS,"frames":frames,"duration_seconds":seconds,
        "native_readback_resolution":[iw,ih],"encoded_resolution":[ow,oh],"nearest_integer_scale":3,
        "renderer_time_start_s":start_time,"renderer_time_last_frame_s":r.time,
        "camera":{"eye_start_m":eye.to_array(),"eye_end_m":r.camera.pos.to_array(),
            "forward_world":forward.to_array(),"translation_m":translation.to_array(),
            "path":"eye=start+translation*smoothstep(0,1,frame/(frames-1)); fixed view direction",
            "fov_y_degrees":r.camera.fov_y.to_degrees(),"aspect":r.camera.aspect},
        "precipitation_start":precipitation,"precipitation_end":r.precipitation,
        "atmosphere":"unchanged current campaign phase; camera-local weather shelter follows actual geometry",
        "capture_frame_pause_ms":pause_ms,"wall_seconds":started.elapsed().as_secs_f64()}))
}

#[derive(Clone)]
struct View {
    slug: &'static str,
    eye: Vec3,
    target: Vec3,
    fov_degrees: f32,
    evidence: Value,
}

fn grounded(g: &Game, p: Vec2, height: f32) -> Vec3 {
    Vec3::new(p.x, g.hf.height_at(p.x, p.y) + height, p.y)
}

fn landmark<'a>(g: &'a Game, wanted: &str) -> &'a world::Landmark {
    g.act
        .landmarks
        .iter()
        .find(|item| item.name.contains(wanted))
        .unwrap_or_else(|| g.act.landmarks.first().expect("campaign landmarks"))
}

fn views(g: &Game, authored: &Value) -> Vec<View> {
    let name = match g.act.id {
        1 => "Getreidesilo",
        2 => "Old House",
        _ => "Rail Station",
    };
    let mark = landmark(g, name);
    let landmark_evidence = json!({"kind":format!("{:?}",mark.kind),"name":mark.name,
        "world_xz_m":mark.pos.to_array(),"source":"resolved authored manifest marker"});
    let landscape = View {
        slug: "landscape",
        eye: grounded(g, mark.pos + Vec2::new(85.0, 105.0), 6.0),
        target: grounded(g, mark.pos, 5.0),
        fov_degrees: 62.0,
        evidence: landmark_evidence.clone(),
    };
    // Find an actual authored road tile, or a damaged masonry wall in the winter city. The heading
    // comes from its placement, so road detail doesn't point across an imagined procedural road.
    let mut placements = authored["placements"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(index, p)| {
            let asset = p[0].as_u64()? as usize;
            let path = authored["assets"][asset].as_str()?;
            let relevant = if g.act.id == 1 {
                path.ends_with("wall_hr_1_hole_1.glb")
            } else {
                path.ends_with("asphalt_hr_1.glb")
            };
            if !relevant {
                return None;
            }
            let pos = Vec2::new(p[1].as_f64()? as f32, p[3].as_f64()? as f32);
            Some((
                pos.distance_squared(mark.pos),
                index,
                path,
                pos,
                p[4].as_f64().unwrap_or(0.0) as f32,
            ))
        })
        .collect::<Vec<_>>();
    placements.sort_by(|a, b| a.0.total_cmp(&b.0));
    let road = if let Some((_, index, path, pos, yaw)) = placements.first() {
        let angle = yaw.to_radians();
        let along = Vec2::new(angle.sin(), angle.cos());
        let side = Vec2::new(along.y, -along.x);
        View {
            slug: "roads_ruins",
            eye: grounded(g, *pos - along * 18.0 + side * 3.0, 1.85),
            target: grounded(
                g,
                *pos + along * 14.0,
                if g.act.id == 1 { 2.2 } else { 1.3 },
            ),
            fov_degrees: 64.0,
            evidence: json!({"source":"actual authored placement", "placement_index":index,"asset":path,"world_xz_m":pos.to_array(),"yaw_degrees":yaw}),
        }
    } else {
        View {
            slug: "roads_ruins",
            evidence: json!({"fallback":"no matching authored road/wall; landscape composition"}),
            ..landscape.clone()
        }
    };
    let grove = worldgen::seasonal_grove_survey_anchor(&g.act);
    let along = (g.act.goal - g.act.spawn).normalize_or_zero();
    let side = Vec2::new(-along.y, along.x);
    let ground = View {
        slug: "bare_grove_ground",
        eye: grounded(g, grove - along * 8.0 + side * 6.0, 0.78),
        target: grounded(g, grove + along * 8.0, 0.45),
        fov_degrees: 66.0,
        evidence: json!({"source":"deterministic first seasonal boundary grove in always-on world overlay",
            "grove_center_xz_m":grove.to_array(),"placement_is_historical_inference":true}),
    };
    let detail = View {
        slug: "landmark",
        eye: grounded(g, mark.pos + Vec2::new(32.0, 44.0), 2.4),
        target: grounded(g, mark.pos, mark.kind.height_hint() * 0.36),
        fov_degrees: 65.0,
        evidence: landmark_evidence,
    };
    vec![landscape, road, ground, detail]
}

fn nearby(authored: &Value, eye: Vec3) -> Value {
    let mut items = authored["placements"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(index, p)| {
            let point = Vec3::new(
                p[1].as_f64()? as f32,
                p[2].as_f64()? as f32,
                p[3].as_f64()? as f32,
            );
            let distance = Vec2::new(point.x - eye.x, point.z - eye.z).length();
            (distance < 160.0).then_some((distance, index, p[0].as_u64()? as usize, point))
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    json!({"authored_placements_within_160m":items.len(),"nearest_authored_placements":items.iter().take(12).map(|(distance,index,asset,point)|json!({
        "index":index,"asset":authored["assets"][*asset],"distance_xz_m":distance,"world_position_m":point.to_array()
    })).collect::<Vec<_>>(),"renderer_streaming":"normal spatial page culling; full act resident; grass window centered on survey camera"})
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let result = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).trim().to_owned())
}

fn revision() -> Value {
    let executable = std::env::current_exe().ok();
    let executable_sha256 = executable
        .as_ref()
        .and_then(|p| p.to_str())
        .and_then(|p| command_output("sha256sum", &[p]))
        .and_then(|text| text.split_whitespace().next().map(str::to_owned));
    let status = command_output("git", &["status", "--porcelain"]);
    json!({"git_head":command_output("git",&["rev-parse","HEAD"]),
        "worktree_dirty":status.as_ref().map(|s|!s.is_empty()),"worktree_status":status,
        "running_executable":executable.map(|p|p.to_string_lossy().into_owned()),"running_executable_sha256":executable_sha256})
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    if let Some(parent) = std::path::Path::new(prefix).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let act_filter = std::env::var("KESSEL_ASTRA_SURVEY_ACT")
        .ok()
        .map(|s| s.parse::<u8>())
        .transpose()?;
    anyhow::ensure!(
        act_filter.is_none_or(|id| (1..=3).contains(&id)),
        "survey act must be1,2 or3"
    );
    let case_filter = std::env::var("KESSEL_ASTRA_SURVEY_CASE").ok();
    let motion_seconds = std::env::var("KESSEL_ASTRA_SURVEY_VIDEO_SECONDS")
        .ok()
        .map(|value| value.parse::<u32>())
        .transpose()?;
    // Reject an accidental all-act/all-phase video job before creating a GPU device or world.
    validate_motion_selection(act_filter, case_filter.as_deref(), motion_seconds)?;
    let warmup = std::env::var("KESSEL_WARMUP")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(24)
        .max(24);
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    let preset = std::env::var("KESSEL_GRAPHICS_PRESET")
        .ok()
        .and_then(|s| graphics::GraphicsPreset::parse(&s))
        .unwrap_or_else(|| graphics::GraphicsPreset::for_adapter(info.device_type));
    graphics::set_active_preset(preset);
    let (device, queue) = request_device(&adapter);
    let build = revision();
    let mut records = Vec::new();
    let manifest_path = format!("{prefix}_manifest.json");
    for act_id in (1u8..=3).filter(|id| act_filter.is_none_or(|selected| *id == selected)) {
        let act = world::act(act_id);
        let authored: Value =
            serde_json::from_slice(&resource::read(act_manifest::path_for_act(act_id))?)?;
        // Keep legitimate night fixture/window geometry resident for all four current phase states.
        let (mut r, mut g) = Game::build_act(&device, &queue, true, &act)?;
        g.mute_audio();
        g.debug_banish_enemies();
        g.debug_hold_enemy_fire();
        g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
        r.set_active_view(0);
        anyhow::ensure!(
            r.exposure_override.is_none(),
            "survey refuses fixed exposure override; remove KESSEL_CAMERA_EXPOSURE"
        );
        anyhow::ensure!(
            !r.deferred || r.autoexp,
            "survey needs current adaptive exposure; remove KESSEL_NO_AUTOEXP"
        );
        let world_views = views(&g, &authored);
        for view in world_views {
            let phases = if view.slug == "landscape" {
                DayPhase::ALL.to_vec()
            } else {
                vec![DayPhase::Day]
            };
            for phase in phases {
                let case = format!("{}_{}", view.slug, phase.label());
                if case_filter.as_ref().is_some_and(|filter| {
                    if motion_seconds.is_some() {
                        case.as_str() != filter.as_str()
                    } else {
                        !case.contains(filter)
                    }
                }) {
                    continue;
                }
                println!(
                    "[astra-survey] act{act_id} {case} current preset={} native={}x{}",
                    r.graphics_preset.label(),
                    r.iw,
                    r.ih
                );
                let started = Instant::now();
                r.apply_campaign_atmosphere(act_id, g.act.season == world::Season::Winter, phase);
                r.set_weather_seed(g.act.seed);
                r.camera.pos = view.eye;
                r.camera.set_forward(view.target - view.eye);
                r.camera.fov_y = view.fov_degrees.to_radians();
                r.camera.aspect = r.iw as f32 / r.ih as f32;
                r.reticle = false;
                r.optic_mask = 0.0;
                r.optic_blast = 0.0;
                r.crew_ko = 0.0;
                r.concussion = 0.0;
                r.interior_light = 0.0;
                r.interior_haze = 0.0;
                r.rt_mix = 1.0;
                r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
                r.weather_shelter = g.building_weather_shelter_at(view.eye);
                r.time = 37.0;
                // Settle only grass's cheap damped wind state at the selected exterior eye. No AI,
                // weapons, damage or game clock is advanced during transport/exposure convergence.
                for _ in 0..60 {
                    g.prepare_world_survey(&mut r, 1.0 / 60.0);
                }
                r.ui_reset();
                r.reset_history();
                let convergence = converge_headless_screenshot(&mut r, &device, &queue, warmup)?;
                anyhow::ensure!(
                    convergence.stable,
                    "act{act_id} {case}: exposure failed convergence; no image exported"
                );
                let path = format!("{prefix}_act{act_id}_{case}.png");
                let (width, height) = (r.iw * 3, r.ih * 3);
                let scale = (width % r.iw == 0 && height % r.ih == 0).then_some(width / r.iw);
                r.render_to_png(&device, &queue, &path, width, height)?;
                let (blases, tlas_instances) = r.rt_stats();
                let weather = json!({"sun_direction_to_source":r.sun_dir.to_array(),"night_factor":r.night,"twilight":r.twilight,
                    "overcast":r.overcast,"storm":r.storm,"fog_extinction_m_inv":r.fog_density,"precipitation":r.precipitation,
                    "shelter":r.weather_shelter,"wind_world_m_s":r.weather_wind_world.to_array(),"seed":g.act.seed,"frozen_sim_time_s":r.time});
                let exposure = json!({"auto":r.autoexp,"override":r.exposure_override,"measured":convergence.measured_exposure,
                    "converged":convergence.stable,"transport_frames":convergence.transport_frames,"exposure_frames":convergence.exposure_frames});
                let rendering = json!({"native_resolution":[r.iw,r.ih],"output_resolution":[width,height],"nearest_integer_scale":scale,
                    "graphics_preset":r.graphics_preset.label(),"deferred":r.deferred,"ray_query":r.rt_on,
                    "hdr_format":"Rgba16Float (renderer HDR authority)","retro_master":r.master_retro,"accumulation":r.accum_on,
                    "shadow_resolution":r.shadow_res,"rt_blas_count":blases,"rt_instances":tlas_instances,
                    "capture_frame_pause_ms":std::env::var("KESSEL_CAPTURE_FRAME_PAUSE_MS").ok()});
                let camera = json!({"eye_m":view.eye.to_array(),"target_m":view.target.to_array(),"fov_y_degrees":view.fov_degrees,
                    "height_above_terrain_m":view.eye.y-g.hf.height_at(view.eye.x,view.eye.z)});
                let grass = g.world_survey_grass_stats();
                let motion = if let Some(seconds) = motion_seconds {
                    Some(capture_motion(
                        &format!("{prefix}_act{act_id}_{case}_motion.mp4"),
                        seconds,
                        &mut r,
                        &mut g,
                        &device,
                        &queue,
                    )?)
                } else {
                    None
                };
                records.push(json!({"image":path,"act":act_id,"act_name":g.act.name,
                    "map_status":if act_id==3 {"current Halbe prototype; canonical campaign finale is Berlin/Reichstag"}else{"current authored campaign world"},
                    "case":case,"phase":phase.label(),"camera":camera,
                    "composition_source":view.evidence,"scene_selection":nearby(&authored,view.eye),
                    "weather":weather,"exposure":exposure,"rendering":rendering,"motion":motion,
                    "grass":grass,"elapsed_seconds":started.elapsed().as_secs_f64()}));
                // Persist after each actual image so a filtered probe or later failed view has a
                // truthful partial manifest, never a claim that unrendered views already exist.
                std::fs::write(
                    &manifest_path,
                    serde_json::to_vec_pretty(&json!({
                    "schema":"astra.current-world-survey.v1","build_revision":build,
                    "adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},
                    "scope":"current full campaign worlds, frozen combat, unchanged current atmosphere and grading",
                    "filters":{"act":act_filter,"case":case_filter,"video_seconds":motion_seconds},"captures":records}))?,
                )?;
                println!("[astra-survey] wrote {path}");
            }
        }
        drop(g);
        drop(r);
    }
    anyhow::ensure!(!records.is_empty(), "survey filters matched no views");
    println!("[astra-survey] {} images: {manifest_path}", records.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_motion_requires_one_exact_bounded_selection_before_gpu_work() {
        assert!(validate_motion_selection(None, None, None).is_ok());
        for seconds in [0, 7, u32::MAX] {
            assert!(
                validate_motion_selection(Some(1), Some("landscape_day"), Some(seconds)).is_err()
            );
        }
        assert!(validate_motion_selection(None, Some("landscape_day"), Some(2)).is_err());
        for case in [None, Some(""), Some("landscape"), Some("day")] {
            assert!(validate_motion_selection(Some(1), case, Some(2)).is_err());
        }
        for seconds in [1, 6] {
            assert!(
                validate_motion_selection(Some(1), Some("landscape_day"), Some(seconds)).is_ok()
            );
            assert!(validate_motion_selection(
                Some(3),
                Some("bare_grove_ground_day"),
                Some(seconds)
            )
            .is_ok());
        }
    }
}
