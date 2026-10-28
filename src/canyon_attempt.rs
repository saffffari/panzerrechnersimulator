//! Explicit, bounded Canyon crew-input evaluation and first-person highlight recorder.
use crate::{
    acoustics, act_manifest,
    game::{CanyonPilot, Game, TankInput},
    graphics, mechanic_capture,
    renderer::Renderer,
    world,
};
use glam::Vec2;
use std::{
    fs,
    io::Write,
    process::{Child, Command, Stdio},
};

struct Cut {
    start_s: f32,
    end_s: f32,
    label: String,
}
struct Film {
    prefix: String,
    encoder: Child,
    samples: Vec<f32>,
    frames: u64,
    cuts: Vec<Cut>,
    segments: Vec<serde_json::Value>,
}
impl Film {
    fn new(prefix: &str, path: &str) -> anyhow::Result<Self> {
        let values: Vec<serde_json::Value> = serde_json::from_slice(&fs::read(path)?)?;
        let cuts: Vec<Cut> = values
            .iter()
            .map(|v| {
                Ok(Cut {
                    start_s: v["start_s"]
                        .as_f64()
                        .ok_or_else(|| anyhow::anyhow!("missing start_s"))?
                        as f32,
                    end_s: v["end_s"]
                        .as_f64()
                        .ok_or_else(|| anyhow::anyhow!("missing end_s"))?
                        as f32,
                    label: v["label"].as_str().unwrap_or("attempt").into(),
                })
            })
            .collect::<anyhow::Result<_>>()?;
        let mut end = 0.0;
        for c in &cuts {
            anyhow::ensure!(
                c.start_s >= end && c.end_s > c.start_s,
                "cuts must be sorted and disjoint"
            );
            end = c.end_s;
        }
        anyhow::ensure!(!cuts.is_empty(), "no film cuts");
        let encoder = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                "1920x1080",
                "-framerate",
                "30",
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "17",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                &format!("{prefix}_silent.mp4"),
            ])
            .stdin(Stdio::piped())
            .spawn()?;
        Ok(Self {
            prefix: prefix.into(),
            encoder,
            samples: Vec::new(),
            frames: 0,
            cuts,
            segments: Vec::new(),
        })
    }
    fn step(
        &mut self,
        g: &mut Game,
        r: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        step: usize,
    ) -> anyhow::Result<()> {
        let audio = g.render_audio_capture_frames((acoustics::SAMPLE_RATE / 60) as usize)?;
        let t = step as f32 / 60.0;
        let Some((index, cut)) = self
            .cuts
            .iter()
            .enumerate()
            .find(|(_, c)| t >= c.start_s && t < c.end_s)
        else {
            return Ok(());
        };
        if self.segments.last().and_then(|x| x["index"].as_u64()) != Some(index as u64) {
            self.segments.push(serde_json::json!({"index":index,"label":cut.label,"simulation_start_s":t,"simulation_end_s":cut.end_s,"film_start_s":self.frames as f64/30.0}));
            println!("[canyon-film] {} at {t:.2}s", cut.label);
        }
        self.samples.extend(audio);
        if step % 2 == 0 {
            let rgba = mechanic_capture::gameplay_frame(g, r, device, queue)?;
            if step == ((cut.start_s + 2.0) * 60.0).round() as usize {
                image::save_buffer(
                    format!("{}_cut{index:02}_preview.png", self.prefix),
                    &rgba,
                    1920,
                    1080,
                    image::ColorType::Rgba8,
                )?;
            }
            self.encoder.stdin.as_mut().unwrap().write_all(&rgba)?;
            self.frames += 1;
        }
        Ok(())
    }
    fn terminal(&mut self, g: &mut Game, r: &mut Renderer, device: &wgpu::Device,
        queue: &wgpu::Queue) -> anyhow::Result<()> {
        self.segments.push(serde_json::json!({"label":"KIA / interior-only turret witness aftermath",
            "film_start_s":self.frames as f64/30.0,"replay":g.last_fatal.is_some(),
            "audio":"live fatal tail, then silent slow motion","damage_reapplied":false}));
        for tick in 0..(crate::DEATH_END*60.0).round() as usize {
            let since=tick as f32/60.0;
            g.advance_terminal_audio(1.0/60.0,r,since);
            g.set_audio_level(1.0-crate::death_fade(since));
            if since>=crate::DEATH_FADE {g.set_audio_silenced(true);}
            self.samples.extend(g.render_audio_capture_frames((acoustics::SAMPLE_RATE/60) as usize)?);
            if tick%2!=0 {continue;}
            let rgba = if let Some(phase) = crate::terminal_visual_phase(since).filter(|_|g.last_fatal.is_some()) {
                g.render_replay(phase,r);
                let mut pixels=r.render_replay_rgba(device,queue,since,
                    crate::renderer::ReplayCapturePolicy::Current)?;
                if since < crate::DEATH_FADE {
                    for pixel in pixels.chunks_exact_mut(4) { for c in &mut pixel[..3] {
                        *c = (*c as f32 * (1.0-crate::death_fade(since))).round() as u8;
                    }}
                }
                let native=image::RgbaImage::from_raw(r.iw,r.ih,pixels).unwrap();
                image::imageops::resize(&native,1920,1080,image::imageops::FilterType::Nearest).into_raw()
            } else { [0,0,0,255].repeat(1920*1080) };
            self.encoder.stdin.as_mut().unwrap().write_all(&rgba)?;
            self.frames+=1;
        }
        Ok(())
    }
    fn finish(mut self) -> anyhow::Result<()> {
        drop(self.encoder.stdin.take());
        anyhow::ensure!(self.encoder.wait()?.success(), "video encoder failed");
        anyhow::ensure!(self.frames > 0, "no frames recorded");
        acoustics::attenuate_over_peak(&mut self.samples, 0.94);
        let wav = format!("{}_soundtrack.wav", self.prefix);
        acoustics::write_wav_stereo(&wav, &self.samples, acoustics::SAMPLE_RATE)?;
        let status = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-y",
                "-i",
                &format!("{}_silent.mp4", self.prefix),
                "-i",
                &wav,
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
                &format!("{}.mp4", self.prefix),
            ])
            .status()?;
        anyhow::ensure!(status.success(), "sound mux failed");
        fs::write(
            format!("{}_segments.json", self.prefix),
            serde_json::to_vec_pretty(
                &serde_json::json!({"frames":self.frames,"fps":30,"audio_s":self.samples.len() as f64/2.0/acoustics::SAMPLE_RATE as f64,"segments":self.segments}),
            )?,
        )?;
        Ok(())
    }
}

/// Explicitly staged damage fixture, using the same terminal recorder as a real Canyon defeat.
pub fn record_terminal_review(prefix:&str,g:&mut Game,r:&mut Renderer,device:&wgpu::Device,
    queue:&wgpu::Queue) -> anyhow::Result<()> {
    let setup=format!("{prefix}_setup.json");
    fs::write(&setup,serde_json::to_vec_pretty(&serde_json::json!([
        {"start_s":0.0,"end_s":crate::DEATH_END,"label":"staged catastrophic penetration / terminal presentation fixture"}
    ]))?)?;
    let mut film=Film::new(prefix,&setup)?;
    film.terminal(g,r,device,queue)?;
    film.finish()
}

/// The harnesses keep their own time record beside the player's, so a headless finish never
/// becomes the title's BEST.
fn harness_times_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(crate::canyon::times_path()?.with_file_name("canyon-v2-times.headless.json"))
}

/// Write the after-action card of a headless run as facts (JSON) and as the card itself (PNG,
/// the same `ui::draw_run_card` the window shows). Returns the report.
pub fn write_run_card(prefix: &str, g: &Game, r: &mut Renderer, device: &wgpu::Device,
    queue: &wgpu::Queue) -> anyhow::Result<Option<crate::canyon::RunReport>> {
    let path = harness_times_path()?;
    let previous = crate::canyon::load_times(&path).best_s;
    let times = match g.canyon_finished() {
        Some(elapsed) => crate::canyon::record_time(&path, elapsed)?,
        None => crate::canyon::load_times(&path),
    };
    let Some(report) = g.canyon_run_report(times, previous) else { return Ok(None) };
    let facts = serde_json::json!({
        "schema": "panzer.canyon-run-card.v1",
        "headline": report.headline(), "cause": report.cause_line(),
        "outcome": format!("{:?}", report.outcome), "elapsed_s": report.elapsed_s,
        "progress": report.progress, "distance_m": report.distance_m, "length_m": report.length_m,
        "shots": report.shots, "kills": report.kills, "hits_taken": report.hits_taken,
        "penetrations_taken": report.penetrations_taken, "enemies_fielded": report.enemies_fielded,
        "broken_modules": report.broken_modules, "wounds": report.wounds, "seed": report.seed,
        "start_clock_local": report.start_clock, "weather": report.weather,
        "best_s": report.best_s, "new_best": report.new_best, "rows": report.rows(),
        "times_file": path, "note": "harness record, separate from the title's canyon-v2-times.v1.json",
    });
    fs::write(format!("{prefix}_runcard.json"), serde_json::to_vec_pretty(&facts)?)?;
    r.ui_reset();
    crate::ui::draw_run_card(r, &report, true);
    r.render_ui_to_png(device, queue, &format!("{prefix}_runcard.png"), 1280, 720)?;
    println!("[canyon-card] {}: {} — {}", prefix, report.headline(), report.cause_line());
    Ok(Some(report))
}

pub(crate) fn build_course(seed: u64) -> anyhow::Result<(wgpu::Device, wgpu::Queue, Renderer, Game, std::path::PathBuf)> {
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.enable_canyon_course_seeded(seed)?;
    g.enable_singleplayer_crew();
    g.set_defer_rtt_to_render(true);
    g.mute_audio();
    g.update(&TankInput::default(), 0.0, &mut r);
    Ok((device, queue, r, g, manifest))
}

/// The point `back_m` metres before the goal along the authored route, and the heading of travel there.
fn route_point_before_goal(route: &[Vec2], back_m: f32) -> (Vec2, f32) {
    let mut idx = route.len() - 1;
    let mut remaining = back_m;
    while idx > 0 && remaining > 0.0 {
        remaining -= route[idx].distance(route[idx - 1]);
        idx -= 1;
    }
    let at = route[idx];
    let next = route[(idx + 1).min(route.len() - 1)];
    let d = next - at;
    (at, d.x.atan2(d.y))
}

/// `canyon-finish-check PREFIX SEED [BACK_M] [MAX_S]`: the win path, verified by play. The Tiger is
/// staged (disclosed: `debug_place_tiger`) on the authored route `BACK_M` metres short of the
/// goal, with the whole defence live and return fire on, and the rule-based crew drives the rest.
/// Passes when the run completes: the clock stops, the time is kept, the card is written.
pub fn finish_check(prefix: &str, seed: u64, back_m: f32, max_s: f32) -> anyhow::Result<()> {
    let (device, queue, mut r, mut g, manifest) = build_course(seed)?;
    let route = g.canyon_route().ok_or_else(|| anyhow::anyhow!("no course route"))?;
    let (at, heading) = route_point_before_goal(&route, back_m);
    g.debug_place_tiger(at, heading);
    g.update(&TankInput::default(), 0.0, &mut r);
    let mut pilot = CanyonPilot::new(&g);
    let dt = 1.0 / 60.0;
    let mut outcome = "time_limit";
    let mut finished_at = None;
    let mut hits_before_finish = 0;
    for step in 0..(max_s / dt).ceil() as usize {
        let t = step as f32 * dt;
        let input = pilot.step(&mut g, &r, t, dt);
        g.update(&input, dt, &mut r);
        if step % 8 == 0 { crate::simulation_uploads::drain(&device, &queue)?; }
        if step % 60 == 0 { println!("[canyon-finish] {}", g.canyon_ai_state(&pilot, t)); }
        if g.game_over().is_some() { outcome = "knocked_out"; break; }
        if let Some(elapsed) = g.canyon_finished() {
            outcome = "completed";
            finished_at = Some(elapsed);
            hits_before_finish = g.stat_hits_on_player;
            // The world runs on through the finish fade; the clock must not.
            for _ in 0..(crate::canyon::FINISH_END_S / dt).ceil() as usize {
                g.update(&TankInput::default(), dt, &mut r);
            }
            anyhow::ensure!(g.canyon_finished() == Some(elapsed), "the finish time changed after the goal");
            break;
        }
    }
    crate::simulation_uploads::drain(&device, &queue)?;
    let report = write_run_card(prefix, &g, &mut r, &device, &queue)?;
    let receipt = serde_json::json!({
        "schema": "panzer.canyon-finish-check.v1", "seed": seed, "staged_back_m": back_m,
        "staged_at": [at.x, at.y], "outcome": outcome, "finished_s": finished_at,
        "hits_taken_before_finish": hits_before_finish, "final": g.canyon_ai_state(&pilot, 0.0),
        "manifest": manifest, "staging": "debug_place_tiger onto the authored route; defence, guns, riflemen and return fire untouched",
        "card": report.as_ref().map(|c| c.rows()),
    });
    fs::write(format!("{prefix}_result.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("[canyon-finish-result] {receipt}");
    anyhow::ensure!(outcome == "completed", "FAIL: the staged Tiger did not complete the course ({outcome})");
    println!("PASS: the course completes at the goal; time {:.1} s kept and carded", finished_at.unwrap_or(0.0));
    Ok(())
}

/// `canyon-foot-check PREFIX SEED [HOLD_S]`: does the garrison's fire reach a dismounted man?
/// The Tiger is staged (disclosed) 45 m from each of up to four posted riflemen in turn; the
/// crewman climbs out through the cupola and stands in the open for `HOLD_S` seconds. Nothing
/// about the riflemen, their perception or their weapons is touched. Passes when at least one
/// post wounds him.
pub fn foot_check(prefix: &str, seed: u64, hold_s: f32) -> anyhow::Result<()> {
    let (device, queue, mut r, mut g, manifest) = build_course(seed)?;
    let route = g.canyon_route().ok_or_else(|| anyhow::anyhow!("no course route"))?;
    let posts = g.canyon_garrison_posts();
    anyhow::ensure!(!posts.is_empty(), "the course has no garrison posts");
    let dt = 1.0 / 60.0;
    let mut trials = Vec::new();
    let mut wounded_any = false;
    // Posts spread along the course: first, a third, two thirds, last.
    let picks: Vec<usize> = [0.0, 0.33, 0.66, 0.99].iter()
        .map(|f| ((posts.len() - 1) as f32 * f).round() as usize).collect();
    for (k, &index) in picks.iter().enumerate() {
        let post = posts[index];
        // Stand 45 m from the post, on the side of the route (open ground, not the ruin's wall).
        let toward = route.iter().min_by(|a, b| a.distance_squared(post).total_cmp(&b.distance_squared(post)))
            .map(|p| (*p - post).normalize_or_zero()).unwrap_or(Vec2::X);
        let toward = if toward.length_squared() < 0.5 { Vec2::X } else { toward };
        let at = post + toward * 45.0;
        let heading = (post - at).x.atan2((post - at).y);
        if g.on_foot() { g.toggle_dismount(); g.dbg_complete_climb(&mut r); }
        g.debug_place_tiger(at, heading);
        g.update(&TankInput::default(), dt, &mut r);
        g.dbg_open_hatch();
        g.toggle_dismount();
        g.dbg_complete_climb(&mut r);
        let hp_start = g.foot_hp();
        let hits_start = g.stat_small_arms_hits_on_foot;
        let soldiers_start = g.soldiers_alive();
        let mut first_wound_s = None;
        for step in 0..(hold_s / dt).ceil() as usize {
            g.update(&TankInput::default(), dt, &mut r);
            if step % 8 == 0 { crate::simulation_uploads::drain(&device, &queue)?; }
            if first_wound_s.is_none() && g.foot_hp() < hp_start { first_wound_s = Some(step as f32 * dt); }
            if g.foot_hp() <= 0.0 { break; }
        }
        let trial = serde_json::json!({
            "post_index": index, "post": [post.x, post.y], "stood_at": [at.x, at.y],
            "on_foot": g.on_foot(), "hp_start": hp_start, "hp_end": g.foot_hp(),
            "small_arms_hits_on_foot": g.stat_small_arms_hits_on_foot - hits_start,
            "first_wound_s": first_wound_s, "soldiers_alive": soldiers_start, "held_s": hold_s,
        });
        println!("[canyon-foot] trial {k}: {trial}");
        wounded_any |= g.foot_hp() < hp_start;
        trials.push(trial);
        if g.foot_hp() <= 0.0 { break; }
    }
    crate::simulation_uploads::drain(&device, &queue)?;
    let receipt = serde_json::json!({
        "schema": "panzer.canyon-foot-check.v1", "seed": seed, "hold_s": hold_s, "trials": trials,
        "wounded": wounded_any, "manifest": manifest,
        "staging": "debug_place_tiger 45 m from a garrison post, cupola dismount via dbg_open_hatch/toggle_dismount/dbg_complete_climb; riflemen untouched",
    });
    fs::write(format!("{prefix}_result.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("[canyon-foot-result] {receipt}");
    anyhow::ensure!(wounded_any, "FAIL: no posted rifleman wounded the dismounted man in {hold_s} s at 45 m");
    println!("PASS: garrison rifle fire reaches the dismounted man");
    Ok(())
}

pub fn run(prefix: &str, seed: u64, max_s: f32, cuts: Option<&str>) -> anyhow::Result<()> {
    anyhow::ensure!(
        max_s.is_finite() && max_s > 0.0,
        "positive finite duration required"
    );
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.enable_canyon_course_seeded(seed)?;
    g.enable_singleplayer_crew();
    g.set_defer_rtt_to_render(true);
    // KESSEL_CANYON_AI_INVINCIBLE=1: a traversal and mission proof, never a survivability result.
    // The receipt's `final.god` and `driver` disclose it.
    let invincible = std::env::var("KESSEL_CANYON_AI_INVINCIBLE").as_deref() == Ok("1");
    if invincible { g.god = true; println!("[canyon-ai] INVINCIBLE traversal proof: the Tiger takes no damage"); }
    let mut film = cuts.map(|path| Film::new(prefix, path)).transpose()?;
    if film.is_none() {
        g.mute_audio();
    }
    let mut pilot = CanyonPilot::new(&g);
    let mut trace = std::io::BufWriter::new(fs::File::create(format!("{prefix}_trace.jsonl"))?);
    let dt = 1.0 / 60.0;
    g.update(&TankInput::default(), 0.0, &mut r);
    let mut state = g.canyon_ai_state(&pilot, 0.0);
    writeln!(trace, "{state}")?;
    let mut last_key = String::new();
    let mut last_activity = 0.0;
    let mut highwater = 0.0f64;
    let mut last_shots = 0;
    let mut outcome = "time_limit";
    let began = std::time::Instant::now();
    for step in 0..(max_s / dt).ceil() as usize {
        let t = step as f32 * dt;
        let input = pilot.step(&mut g, &r, t, dt);
        g.update(&input, dt, &mut r);
        if step % 8 == 0 { crate::simulation_uploads::drain(&device, &queue)?; }
        if let Some(f) = film.as_mut() {
            f.step(&mut g, &mut r, &device, &queue, step)?;
        }
        if step % 600 == 0 && std::env::var("KESSEL_T34_DBG").as_deref() == Ok("1") {
            println!("[t34-ai-state] {}", serde_json::json!({"t":t,"tiger":g.player.pos.to_array(),"tiger_heading":g.player.heading,"winter":g.winter_encounter_snapshot()}));
        }
        if step % 60 == 0 || g.game_over().is_some() || g.canyon_finished().is_some() {
            state = g.canyon_ai_state(&pilot, t + dt);
            state["mission_beat"] = serde_json::json!(g.mission_current_beat());
            state["mission_finished"] = serde_json::json!(g.mission_finished());
            let key = format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                state["mission_beat"],
                state["phase"],
                state["shots"],
                state["kills"],
                state["incoming_hits"],
                state["breech"],
                state["total_t34s"],
                state["broken_modules"]
            );
            if key != last_key || step % 3600 == 0 {
                println!("[canyon-ai] {state}");
                last_key = key;
                trace.flush()?;
            }
            writeln!(trace, "{state}")?;
            let progress = state["progress"].as_f64().unwrap_or(0.0);
            let shots = state["shots"].as_u64().unwrap_or(0);
            if progress > highwater + 0.002 || shots != last_shots {
                last_activity = t;
                highwater = highwater.max(progress);
                last_shots = shots;
            }
            if g.game_over().is_some() {
                outcome = "knocked_out";
                break;
            }
            if g.canyon_finished().is_some() {
                outcome = "completed";
                break;
            }
            if t - last_activity > 240.0 {
                outcome = "pilot_stalled";
                break;
            }
        }
    }
    crate::simulation_uploads::drain(&device, &queue)?;
    trace.flush()?;
    let receipt = serde_json::json!({"schema":"panzer.canyon-ai-attempt.v1","seed":seed,"max_seconds":max_s,"outcome":outcome,"wall_seconds":began.elapsed().as_secs_f64(),"final":state,"driver":if invincible {"rule-based crew inputs; known map route; geometry sight within 420 m; INVINCIBLE (KESSEL_CANYON_AI_INVINCIBLE=1): a traversal and mission proof, not a survivability result"} else {"rule-based crew inputs; known map route; geometry sight within 420 m; no automatic repair or damage suppression"},"manifest":manifest,"simulation_hz":60,"film":cuts.is_some(),"mission":g.mission_report(),"winter":g.winter_encounter_snapshot()});
    fs::write(
        format!("{prefix}_result.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("[canyon-result] {receipt}");
    // The run's own after-action card, as the window would show it.
    if g.game_over().is_some() || g.canyon_finished().is_some() {
        write_run_card(prefix, &g, &mut r, &device, &queue)?;
    }
    if let Some(mut f) = film {
        if g.game_over().is_some() {
            // Append every terminal event independently of the user's ordinary highlight cuts.
            f.terminal(&mut g,&mut r,&device,&queue)?;
        }
        f.finish()?;
    }
    Ok(())
}
