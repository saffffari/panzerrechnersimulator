//! Explicit, bounded Canyon crew-input evaluation and first-person highlight recorder.
use crate::{
    acoustics, act_manifest,
    game::{CanyonPilot, Game, TankInput},
    graphics, mechanic_capture,
    renderer::Renderer,
    world,
};
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

pub fn run(prefix: &str, seed: u64, max_s: f32, cuts: Option<&str>) -> anyhow::Result<()> {
    anyhow::ensure!(
        max_s.is_finite() && max_s > 0.0,
        "positive finite duration required"
    );
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let manifest = "assets/worldgen/workshops/gaea-canyon-run/manifest.v1.json";
    act_manifest::select_runtime_manifest(Some(manifest.into()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.enable_canyon_course_seeded(seed)?;
    g.enable_singleplayer_crew();
    g.set_defer_rtt_to_render(true);
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
        if let Some(f) = film.as_mut() {
            f.step(&mut g, &mut r, &device, &queue, step)?;
        }
        if step % 60 == 0 || g.game_over().is_some() || g.canyon_finished().is_some() {
            state = g.canyon_ai_state(&pilot, t + dt);
            let key = format!(
                "{}|{}|{}|{}|{}|{}|{}",
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
    trace.flush()?;
    let receipt = serde_json::json!({"schema":"panzer.canyon-ai-attempt.v1","seed":seed,"max_seconds":max_s,"outcome":outcome,"wall_seconds":began.elapsed().as_secs_f64(),"final":state,"driver":"rule-based crew inputs; known map route; geometry sight within 420 m; no automatic repair or damage suppression","manifest":manifest,"simulation_hz":60,"film":cuts.is_some()});
    fs::write(
        format!("{prefix}_result.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("[canyon-result] {receipt}");
    if let Some(mut f) = film {
        if g.game_over().is_some() {
            // Native death presentation freezes the world and lets already-emitted audio
            // arrive. Keep the fatal frame and its report instead of cutting a tick early.
            let start = (state["time_s"].as_f64().unwrap() * 60.0).round() as usize;
            for tick in 0..180 {
                let since = (tick + 1) as f32 * dt;
                g.advance_terminal_audio(dt, &r, since);
                g.set_audio_level(1.0 - crate::death_fade(since));
                if since >= crate::DEATH_FADE {
                    g.set_audio_silenced(true);
                }
                f.step(&mut g, &mut r, &device, &queue, start + tick)?;
            }
        }
        f.finish()?;
    }
    Ok(())
}
