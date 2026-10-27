//! Paired witness movies from ONE continuous ordinary simulation and ONE gameplay audio clock.
//! These files are diagnostics/showcases, not a live frame-rate benchmark or replayed fake hits.
use super::*;
use serde_json::{json, Value};

const SIM_HZ: u32 = 120;
const FPS: u32 = 24;
const STEP: f32 = 1.0 / SIM_HZ as f32;

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Platoon,
    Firing {
        range: f32,
        bearing: f32,
        turret: bool,
    },
}

impl Scenario {
    fn parse(name: &str) -> anyhow::Result<Self> {
        Ok(match name {
            "platoon" => Self::Platoon,
            "side-30" => Self::Firing { range:30.0,bearing:90.0,turret:true },
            "side-100" => Self::Firing { range:100.0,bearing:90.0,turret:true },
            "oblique-100" => Self::Firing { range:100.0,bearing:130.0,turret:true },
            "front-300" => Self::Firing { range:300.0,bearing:0.0,turret:false },
            "rear-80" => Self::Firing { range:80.0,bearing:180.0,turret:false },
            _ => anyhow::bail!("crew-battle mode must be platoon, side-30, side-100, oblique-100, front-300 or rear-80"),
        })
    }

    fn label(self) -> String {
        match self {
            Self::Platoon => "AI TIGER vs 3 T-34s - ONE CONTINUOUS SIMULATION".into(),
            Self::Firing { range, bearing, .. } => {
                format!("STAGED T-34 FIRING - BR-350B 76mm - {range:.0}m - BEARING {bearing:.0}deg")
            }
        }
    }
}

fn sample_count(tick: u64) -> usize {
    (((tick + 1) * u64::from(acoustics::SAMPLE_RATE) / u64::from(SIM_HZ))
        - (tick * u64::from(acoustics::SAMPLE_RATE) / u64::from(SIM_HZ))) as usize
}

fn crew_ledger(game: &Game) -> Value {
    json!(game
        .crew
        .iter()
        .map(|p| json!({"identity":p.role.name(),"alive":p.ok}))
        .collect::<Vec<_>>())
}

fn file_sha256(path: &str) -> anyhow::Result<String> {
    // Windows has no sha256sum; certutil prints the digest on its second line.
    let (result, windows) = match Command::new("sha256sum").arg(path).output() {
        Ok(result) => (result, false),
        Err(_) => (Command::new("certutil").args(["-hashfile", path, "SHA256"]).output()?, true),
    };
    ensure!(
        result.status.success(),
        "cannot hash capture dependency {path}"
    );
    let text = String::from_utf8(result.stdout)?;
    if windows {
        return text.lines().nth(1).map(|line| line.trim().replace(' ', "").to_lowercase())
            .filter(|digest| digest.len() == 64)
            .ok_or_else(|| anyhow::anyhow!("certutil gave no digest for {path}"));
    }
    Ok(text
        .split_whitespace()
        .next()
        .context("empty sha256sum")?
        .to_owned())
}

struct Picture {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    path: String,
}

impl Picture {
    fn new(path: String, r: &Renderer, label: &str, view: &str) -> anyhow::Result<Self> {
        // Text comes from the fixed scenario enum and fixed view labels, never shell evaluation.
        let filter = format!("scale=1920:1080:flags=neighbor,format=yuv420p,drawtext=text='{label}':fontcolor=white:fontsize=23:x=24:y=22:box=1:boxcolor=black@0.65:boxborderw=8,drawtext=text='{view} - SAME RUN AND CLOCK':fontcolor=white:fontsize=21:x=24:y=h-48:box=1:boxcolor=black@0.65:boxborderw=8");
        let mut child = Command::new("ffmpeg")
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
                &format!("{}x{}", r.iw, r.ih),
                "-framerate",
                &FPS.to_string(),
                "-i",
                "-",
                "-an",
                "-vf",
                &filter,
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "16",
                "-movflags",
                "+faststart",
                &path,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("starting crew-battle FFmpeg")?;
        let input = Some(child.stdin.take().context("FFmpeg input unavailable")?);
        Ok(Self { child, input, path })
    }

    fn frame(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.input
            .as_mut()
            .context("encoder already finished")?
            .write_all(bytes)?;
        Ok(())
    }

    fn finish(&mut self) -> anyhow::Result<()> {
        self.input.take();
        ensure!(
            self.child.wait()?.success(),
            "FFmpeg rejected {}",
            self.path
        );
        Ok(())
    }
}

impl Drop for Picture {
    fn drop(&mut self) {
        self.input.take();
        // Only this recorder's own encoder; never touch a running game/user process on failure.
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[derive(Clone, Copy)]
struct SavedView {
    camera: crate::camera::Camera,
    reticle: bool,
    optic_mask: f32,
    optic_blast: f32,
    crew_ko: f32,
    light: f32,
    haze: f32,
    rt_mix: f32,
    shelter: f32,
    precipitation: f32,
}

impl SavedView {
    fn save(r: &Renderer) -> Self {
        Self {
            camera: r.camera,
            reticle: r.reticle,
            optic_mask: r.optic_mask,
            optic_blast: r.optic_blast,
            crew_ko: r.crew_ko,
            light: r.interior_light,
            haze: r.interior_haze,
            rt_mix: r.rt_mix,
            shelter: r.weather_shelter,
            precipitation: r.precipitation,
        }
    }
    fn restore(self, r: &mut Renderer) {
        r.camera = self.camera;
        r.reticle = self.reticle;
        r.optic_mask = self.optic_mask;
        r.optic_blast = self.optic_blast;
        r.crew_ko = self.crew_ko;
        r.interior_light = self.light;
        r.interior_haze = self.haze;
        r.rt_mix = self.rt_mix;
        r.weather_shelter = self.shelter;
        r.precipitation = self.precipitation;
        r.set_interior_atmosphere_enabled(self.light > 0.5);
        r.set_active_view(0);
    }
}

fn interior_witness(game: &Game, r: &mut Renderer, hull: bool) {
    // An explicitly detached lens, not a dead crewman's resurrected first-person view. Keep the
    // authored hull/turret body transforms and every casualty intact, including total crew loss.
    let (parent, eye, target, fov) = if hull {
        (
            game.player.hull_model(),
            Vec3::new(0.0, 1.75, 0.45),
            Vec3::new(0.0, 1.02, 1.78),
            88.0_f32,
        )
    } else {
        (
            game.player.turret_model(),
            Vec3::new(-0.45, 2.34, -0.84),
            Vec3::new(0.0, 1.45, 0.18),
            80.0_f32,
        )
    };
    r.camera.pos = parent.transform_point3(eye);
    r.camera
        .set_forward(parent.transform_point3(target) - r.camera.pos);
    r.camera.fov_y = fov.to_radians();
    r.reticle = false;
    r.optic_mask = 0.0;
    r.optic_blast = 0.0;
    r.weather_shelter = 1.0;
    r.set_active_view(4);
    game.configure_crew_render_view(LanRole::Loader, false, r);
    game.drive_singleplayer_crew_rigs_for_view(None, r);
    // crew_ko is an inert shader compatibility field; physical injury/grains/bodies remain visible.
    game.apply_cinematic_camera_impulses(r);
    let mut bounded = parent.inverse().transform_point3(r.camera.pos);
    bounded.y = bounded.y.min(eye.y);
    r.camera.pos = parent.transform_point3(bounded);
}

fn overhead_pose(points: &[Vec3], aspect: f32) -> (Vec3, Vec3, f32) {
    let focus = points.iter().copied().sum::<Vec3>() / points.len().max(1) as f32;
    // Put the longest fire line across the wider horizontal sensor. A sphere-fit/high diagonal
    // needlessly lifted the lens above the weather layer and turned even a 30m lane into white fog.
    let mut axis = Vec3::X;
    let mut longest = 0.0;
    for (i, a) in points.iter().enumerate() {
        for b in &points[i + 1..] {
            let delta = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
            if delta.length_squared() > longest {
                longest = delta.length_squared();
                axis = delta.normalize();
            }
        }
    }
    let elevation = 32.0_f32.to_radians();
    let back = axis.cross(Vec3::Y) * elevation.cos() + Vec3::Y * elevation.sin();
    let forward = -back;
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward).normalize();
    let fov = 76.0_f32.to_radians();
    let tan_y = (fov * 0.5).tan() * 0.86;
    let tan_x = tan_y * aspect.max(0.2);
    let mut distance = 12.0_f32;
    // Fit the full vehicle, not just each origin; muzzle length and turret height retain margin.
    for point in points {
        for x in [-5.0, 5.0] {
            for y in [-2.0, 3.0] {
                for z in [-5.0, 5.0] {
                    let offset = *point + Vec3::new(x, y, z) - focus;
                    let depth = offset.dot(forward);
                    distance = distance
                        .max(offset.dot(right).abs() / tan_x - depth)
                        .max(offset.dot(up).abs() / tan_y - depth);
                }
            }
        }
    }
    (focus + back * (distance + 1.0), focus, fov)
}

fn overhead_witness(game: &Game, active: &[usize], r: &mut Renderer) {
    let mut points = vec![game.player.center()];
    points.extend(active.iter().map(|&i| game.enemies[i].center())); // retain wrecks in the same battle
    let (mut eye, target, fov) = overhead_pose(&points, r.camera.aspect);
    eye.y = eye.y.max(game.hf.height_at(eye.x, eye.z) + 8.0);
    r.camera.pos = eye;
    r.camera.set_forward(target - eye);
    r.camera.fov_y = fov;
    r.reticle = false;
    r.optic_mask = 0.0;
    r.optic_blast = 0.0;
    r.crew_ko = 0.0;
    r.interior_light = 0.0;
    r.interior_haze = 0.0;
    r.rt_mix = 1.0;
    r.weather_shelter = 0.0;
    r.set_interior_atmosphere_enabled(false);
    r.set_active_view(1);
    game.drive_singleplayer_crew_rigs_for_view(None, r);
}

fn pilot_input(pilot: &mut TigerPilot<'_>, game: &mut Game, tick: u64) -> TankInput {
    pilot.input_with_dt(game, tick as usize, STEP)
}

/// Record paired interior/overhead witnesses of a real Act-I battle or one real T-34 discharge.
/// Root CLI: `crew-battle OUTDIR MODE SEED SECONDS T34_WEIGHTS TIGER_WEIGHTS`.
pub fn run_crew_battle_capture(
    outdir: &str,
    mode: &str,
    seed: u32,
    max_secs: f32,
    t34_weights: &str,
    tiger_weights: &str,
) -> anyhow::Result<()> {
    let scenario = Scenario::parse(mode)?;
    ensure!(
        max_secs.is_finite() && (6.0..=90.0).contains(&max_secs),
        "capture length must be 6..=90s"
    );
    ensure!(
        crate::graphics::shader_backend_enabled(),
        "crew-battle requires the shader graphics backend"
    );
    let t34_path = crate::resource::resolve(t34_weights)?;
    let tiger_path = crate::resource::resolve(tiger_weights)?;
    let t34_weights = t34_path.to_str().context("T34 policy path is not UTF-8")?;
    let tiger_weights = tiger_path
        .to_str()
        .context("Tiger policy path is not UTF-8")?;
    let requested_manifest = crate::act_manifest::playtest_manifest_path();
    ensure!(
        crate::act_manifest::select_runtime_manifest(Some(requested_manifest))? == Some(1),
        "crew battle requires an actual Act I manifest"
    );
    let manifest_path = crate::act_manifest::selected_runtime_manifest()
        .context("missing runtime manifest")?
        .1;
    let manifest_path = manifest_path
        .to_str()
        .context("manifest path is not UTF-8")?;
    let manifest_sha256 = file_sha256(manifest_path)?;
    let t34 =
        ai::Mlp::load(t34_weights).with_context(|| format!("loading T34 policy {t34_weights}"))?;
    let tiger = ai::Mlp::load(tiger_weights)
        .with_context(|| format!("loading Tiger policy {tiger_weights}"))?;
    let weights = json!({"t34":{"path":t34_weights,"sha256":file_sha256(t34_weights)?},
        "tiger":{"path":tiger_weights,"sha256":file_sha256(tiger_weights)?}});
    std::fs::create_dir_all(outdir)?;
    let stem = format!("crew_battle_{mode}_{seed:08x}_20260913_v2");
    let receipt = format!("{outdir}/{stem}.json");
    ensure!(
        !std::path::Path::new(&receipt).exists(),
        "capture receipt already exists; use a new seed/output path"
    );
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &crate::world::act(1))?;
    let count = if matches!(scenario, Scenario::Platoon) {
        3
    } else {
        1
    };
    let active = stage_take(&mut g, &t34, count, seed, 105.0);
    g.set_playtest_return_fire(true);
    let controlled = match scenario {
        Scenario::Platoon => None,
        Scenario::Firing {
            range,
            bearing,
            turret,
        } => Some(g.debug_stage_crew_battle_shooter(range, bearing, turret)?),
    };
    g.enable_singleplayer_crew();
    g.debug_begin_crew_battle_audit();
    g.begin_audio_capture();
    g.set_defer_rtt_to_render(true);
    let initial = g.debug_crew_battle_snapshot();
    ensure!(
        g.singleplayer_living_crew() == Some(1),
        "recording requires the living singleplayer crewman"
    );
    let label = scenario.label();
    let mut inside = Picture::new(
        format!("{outdir}/{stem}_interior_picture.mp4"),
        &r,
        &label,
        "DETACHED INTERIOR WITNESS",
    )?;
    let mut outside = Picture::new(
        format!("{outdir}/{stem}_overhead_picture.mp4"),
        &r,
        &label,
        "OVERHEAD WITNESS",
    )?;
    let mut samples = Vec::new();
    let mut timeline = Vec::new();
    let mut pilot = TigerPilot::new(&tiger);
    let frames = (max_secs as f64 * FPS as f64).ceil() as u64;
    let mut fired = None;
    let mut previous_crew = crew_ledger(&g);
    let mut previous_hits = 0;
    let start = Instant::now();
    let mut last_frame_state = initial.clone();
    for frame in 0..frames {
        for substep in 0..SIM_HZ / FPS {
            let tick = frame * u64::from(SIM_HZ / FPS) + u64::from(substep);
            let now = (tick + 1) as f64 / f64::from(SIM_HZ);
            g.debug_crew_battle_clock(tick + 1, now);
            let input = if let Some((index, target, _)) = &controlled {
                let laid = g.debug_aim_crew_battle_shooter(*index, *target, STEP);
                if tick >= u64::from(SIM_HZ) * 3 && fired.is_none() && laid {
                    let id = g
                        .debug_launch_crew_battle_shot(*index)
                        .context("real T34 muzzle discharge rejected")?;
                    fired = Some(id);
                    timeline.push(json!({"kind":"scripted_trigger","tick":tick,"at_s":tick as f64/f64::from(SIM_HZ),
                        "projectile_id":id,"actor_index":index,"state":g.debug_crew_battle_snapshot()}));
                }
                TankInput::default()
            } else {
                g.debug_engage_all(); // known-contact battle setup, disclosed in provenance
                pilot_input(&mut pilot, &mut g, tick)
            };
            // Continue physical casualties/fire after a fatal hit; never clear KIA or reset match.
            g.update(&input, STEP, &mut r);
            samples.extend(g.render_audio_capture_frames(sample_count(tick))?);
            let crew = crew_ledger(&g);
            if crew != previous_crew || g.stat_hits_on_player != previous_hits {
                timeline.push(
                    json!({"kind":"crew_or_armor_change","tick":tick+1,"at_s":now,
                    "frame":frame,"state":g.debug_crew_battle_snapshot()}),
                );
                previous_crew = crew;
                previous_hits = g.stat_hits_on_player;
            }
        }
        let before = g.debug_crew_battle_snapshot();
        let saved = SavedView::save(&r);
        let hull = matches!(scenario, Scenario::Firing { turret: false, .. });
        interior_witness(&g, &mut r, hull);
        inside.frame(&r.render_to_rgba(&device, &queue, r.iw, r.ih)?)?;
        overhead_witness(&g, &active, &mut r);
        outside.frame(&r.render_to_rgba(&device, &queue, r.iw, r.ih)?)?;
        saved.restore(&mut r);
        g.drive_singleplayer_crew_rigs_for_view(g.singleplayer_embodied_role(), &mut r);
        let after = g.debug_crew_battle_snapshot();
        ensure!(
            before == after,
            "witness rendering mutated authoritative battle state at frame {frame}"
        );
        last_frame_state = after;
        if frame % u64::from(FPS) == 0 {
            timeline.push(json!({"kind":"paired_frame","frame":frame,
                "tick":(frame+1)*u64::from(SIM_HZ/FPS),"at_s":(frame+1) as f64/f64::from(FPS),
                "same_state_for_both_views":true,"state":last_frame_state}));
        }
        if frame % u64::from(FPS * 5) == 0 {
            println!("[crew-battle] {mode} {:.1}/{max_secs:.1}s, hits {}, pens {}, crew {:?}, wall {:.1}s",
                (frame+1) as f32/FPS as f32,g.stat_hits_on_player,g.stat_pens_on_player,
                g.singleplayer_living_crew(),start.elapsed().as_secs_f32());
        }
    }
    inside.finish()?;
    outside.finish()?;
    let soundtrack = format!("{outdir}/{stem}_gameplay_audio.wav");
    ensure!(
        samples.iter().all(|x| x.is_finite()),
        "nonfinite gameplay audio"
    );
    let peak = samples.iter().map(|x| x.abs()).fold(0.0_f32, f32::max);
    acoustics::write_wav_stereo(&soundtrack, &samples, acoustics::SAMPLE_RATE)?;
    let interior = format!("{outdir}/{stem}_interior.mp4");
    let overhead = format!("{outdir}/{stem}_overhead.mp4");
    mux_showreel(&inside.path, &soundtrack, &interior)?;
    mux_showreel(&outside.path, &soundtrack, &overhead)?;
    let contacts = last_frame_state["contacts"].as_array().map_or(0, Vec::len);
    let passed = match scenario {
        Scenario::Platoon => contacts > 0,
        Scenario::Firing { .. } => fired.is_some() && g.stat_hits_on_player > 0,
    };
    let manifest = json!({"schema":"panzer.crew-battle-capture.v1","scenario":mode,"seed":seed,
        "act":{"id":g.act.id,"name":g.act.name,"season":format!("{:?}",g.act.season),
            "authored_spawn_xz":g.act.spawn.to_array(),"manifest_path":manifest_path,
            "manifest_sha256":manifest_sha256},"adapter":adapter.get_info().name,
        "staging":controlled.as_ref().map(|(_,_,staging)|staging),"weights":weights,
        "platoon_scope":"Three existing T-34-76 actors; installed enemy MLP and TigerPilot station workflow; known contacts disclosed. One singleplayer crewman aboard, one embodied AI controller changing his posts; the other seats are empty.",
        "audio":{"source":"ordinary runtime gameplay AudioMixer; one simulation clock; embodied Tiger listener shared by both witness tracks",
            "perceptually_auditioned":false,"sample_rate":acoustics::SAMPLE_RATE,"stereo_sample_frames":samples.len()/2,
            "peak":peak,"clipped_samples":samples.iter().filter(|s|s.abs()>=1.0).count(),"metadata":g.audio_capture_metadata()},
        "video":{"simulation_hz":SIM_HZ,"capture_fps":FPS,"frames_per_track":frames,
            "duration_s":frames as f64/f64::from(FPS),"internal_pixels":[r.iw,r.ih],"output_pixels":[1920,1080],
            "interior":interior,"overhead":overhead,"audio":soundtrack,
            "pairing":"Frame N in either video is the same authoritative tick; no replay/resimulation or alternate seed",
            "interior_scope":"Detached observer lens keeps real living/casualty bodies visible after injury; never a restored dead identity",
            "benchmark":false},"initial_state":initial,"final_state":last_frame_state,"timeline":timeline,
        "acceptance":{"actual_projectile_contact":passed,"target_penetrated":g.stat_pens_on_player>0,
            "crew_casualty":g.singleplayer_living_crew() == Some(0),
            "no_forced_damage":true,"visual_review_pending":true,
            "note":"A bounce, miss, or zero-casualty result remains that observed result; do not label it successful penetration or KIA"}});
    std::fs::write(&receipt, serde_json::to_vec_pretty(&manifest)?)?;
    ensure!(passed,"recorded real result failed target-contact acceptance; keep receipt but do not advertise it as a hit");
    println!("[crew-battle] paired tracks and provenance: {receipt}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paired_capture_clock_is_exact_for_both_views_and_audio() {
        let frames = 24_u64 * 90;
        let ticks = frames * u64::from(SIM_HZ / FPS);
        assert_eq!(
            (0..ticks).map(sample_count).sum::<usize>(),
            90 * acoustics::SAMPLE_RATE as usize
        );
        assert_eq!(ticks, 90 * u64::from(SIM_HZ));
    }
    #[test]
    fn overhead_contains_every_tank_with_margin_at_long_range() {
        for range in [30.0, 100.0, 300.0, 600.0] {
            let points = [
                Vec3::new(1700.0, 2.0, 1000.0),
                Vec3::new(1700.0 + range, 6.0, 1000.0),
                Vec3::new(1700.0, 1.0, 1000.0 + range),
            ];
            let (eye, target, fov) = overhead_pose(&points, 16.0 / 9.0);
            let mut camera = crate::camera::Camera::new(16.0 / 9.0);
            camera.pos = eye;
            camera.set_forward(target - eye);
            camera.fov_y = fov;
            for point in points {
                for x in [-5.0, 5.0] {
                    for y in [-2.0, 3.0] {
                        for z in [-5.0, 5.0] {
                            let clip =
                                camera.view_proj() * (point + Vec3::new(x, y, z)).extend(1.0);
                            let ndc = clip.truncate() / clip.w;
                            assert!(
                                clip.w > 0.0 && ndc.x.abs() < 0.90 && ndc.y.abs() < 0.90,
                                "{range}: {ndc:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn close_two_tank_witness_stays_below_high_weather_camera() {
        let (eye, target, _) = overhead_pose(&[Vec3::ZERO, Vec3::X * 30.0], 16.0 / 9.0);
        assert!(
            eye.y - target.y < 16.0,
            "close range witness needlessly above battlefield: {eye:?}"
        );
        assert!(
            eye.y - target.y > 6.0,
            "witness must stay above turret roofs"
        );
    }
    #[test]
    fn capture_cases_do_not_claim_an_unimplemented_t34_85() {
        assert!(Scenario::parse("t34-85-side").is_err());
        assert!(Scenario::parse("side-30")
            .unwrap()
            .label()
            .contains("BR-350B 76mm"));
    }
}
