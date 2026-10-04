//! Opt-in playtest session log. `KESSEL_SESSION_LOG=<path>` writes one JSON line per second of
//! play: frame, simulation and render-submission times on the CPU, per-pass GPU times sampled on
//! one fenced frame every `KESSEL_SESSION_LOG_GPU_S` seconds (default 2; needs
//! `KESSEL_GPU_PROFILE=1`), render work counts, and the gameplay snapshot from
//! `Game::session_telemetry`. It changes no gameplay or rendering decision. The fenced GPU
//! sample waits for that one frame to finish on the GPU, so the sampled frame itself is slower
//! than an ordinary pipelined frame; its CPU frame time is excluded from the statistics.
use std::io::Write;
use std::time::Instant;

pub struct SessionLog {
    out: std::io::BufWriter<std::fs::File>,
    started: Instant,
    last_line: Instant,
    last_gpu: Instant,
    gpu_period_s: f32,
    frame_ms: Vec<f32>,
    sim_ms: Vec<f32>,
    render_ms: Vec<f32>,
    gpu: Option<Vec<(String, f64)>>,
    gpu_frame_ms: Option<f32>,
    skip_next_frame_time: bool,
    lines: u64,
}

fn stats(values: &mut Vec<f32>) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::Value::Null;
    }
    values.sort_by(f32::total_cmp);
    let n = values.len();
    let at = |q: f32| values[(((n - 1) as f32) * q).round() as usize];
    let mean = values.iter().sum::<f32>() / n as f32;
    serde_json::json!({"n": n, "mean": mean, "p50": at(0.5), "p95": at(0.95), "p99": at(0.99), "max": values[n - 1]})
}

impl SessionLog {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("KESSEL_SESSION_LOG")?;
        let path = std::path::PathBuf::from(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = match std::fs::File::create(&path) {
            Ok(file) => file,
            Err(error) => {
                eprintln!("[session-log] cannot create {}: {error}", path.display());
                return None;
            }
        };
        let gpu_period_s = std::env::var("KESSEL_SESSION_LOG_GPU_S").ok()
            .and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite() && *v >= 0.25).unwrap_or(2.0);
        println!("[session-log] writing {} (GPU sample every {gpu_period_s} s)", path.display());
        let now = Instant::now();
        Some(Self {
            out: std::io::BufWriter::new(file), started: now, last_line: now, last_gpu: now,
            gpu_period_s, frame_ms: Vec::new(), sim_ms: Vec::new(), render_ms: Vec::new(),
            gpu: None, gpu_frame_ms: None, skip_next_frame_time: false, lines: 0,
        })
    }

    /// One presented frame: wall time since the previous frame, time inside the simulation
    /// update, and time submitting the render (CPU side, before the GPU finishes it).
    pub fn frame(&mut self, frame_ms: f32, sim_ms: f32, render_ms: f32) {
        if std::mem::take(&mut self.skip_next_frame_time) {
            return;
        }
        self.frame_ms.push(frame_ms);
        self.sim_ms.push(sim_ms);
        self.render_ms.push(render_ms);
    }

    pub fn gpu_due(&self) -> bool {
        self.last_gpu.elapsed().as_secs_f32() >= self.gpu_period_s
    }

    /// Per-pass GPU milliseconds of one fenced frame and the wall time that frame took end to end.
    pub fn gpu_sample(&mut self, passes: Vec<(String, f64)>, fenced_frame_ms: f32) {
        self.last_gpu = Instant::now();
        // The fence inflates this frame and the interval to the next one.
        self.skip_next_frame_time = true;
        if !passes.is_empty() {
            self.gpu = Some(passes);
            self.gpu_frame_ms = Some(fenced_frame_ms);
        }
    }

    pub fn line_due(&self) -> bool {
        self.last_line.elapsed().as_secs_f32() >= 1.0
    }

    pub fn write(&mut self, game: serde_json::Value, render: serde_json::Value) {
        let gpu = self.gpu.take().map(|passes| {
            let total: f64 = passes.iter().map(|(_, ms)| ms).sum();
            let mut passes = passes;
            passes.sort_by(|a, b| b.1.total_cmp(&a.1));
            serde_json::json!({
                "note": "one fenced frame; passes may overlap on the GPU, so the sum is an upper bound",
                "pass_sum_ms": total,
                "fenced_frame_wall_ms": self.gpu_frame_ms.take(),
                "passes_ms": passes.into_iter().map(|(label, ms)| serde_json::json!([label, ms])).collect::<Vec<_>>(),
            })
        });
        let frames = self.frame_ms.len();
        let span = self.last_line.elapsed().as_secs_f32().max(1e-3);
        let line = serde_json::json!({
            "t_s": self.started.elapsed().as_secs_f32(),
            "fps": frames as f32 / span,
            "frame_ms": stats(&mut self.frame_ms),
            "cpu_sim_ms": stats(&mut self.sim_ms),
            "cpu_render_submit_ms": stats(&mut self.render_ms),
            "gpu": gpu,
            "render": render,
            "game": game,
        });
        self.frame_ms.clear();
        self.sim_ms.clear();
        self.render_ms.clear();
        self.last_line = Instant::now();
        if writeln!(self.out, "{line}").is_ok() {
            self.lines += 1;
            // A crash or a kill must not lose the session; a line a second is cheap to flush.
            let _ = self.out.flush();
        }
    }
}
