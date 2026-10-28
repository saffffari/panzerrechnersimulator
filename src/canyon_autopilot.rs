//! `KESSEL_CANYON_AUTOPILOT=1`: the rule-based Canyon crew (`CanyonPilot`, the same driver as the
//! headless `canyon-ai-run`) plays the windowed Canyon run in place of the keyboard and mouse, so
//! the opt-in session log (`KESSEL_SESSION_LOG`) can record a full course on the real presented
//! frame path. It changes nothing about the run itself: same world, same defence, same damage.
//! The run ends the process once the after-action card is up (exit 0 completed, 1 knocked out),
//! when the route and the gun have both been idle for 240 s (exit 3, `pilot_stalled`), or at
//! `KESSEL_CANYON_AUTOPILOT_MAX_S` seconds of play (default 2400; exit 4, `time_limit`).
use crate::game::{CanyonPilot, Game, TankInput};
use crate::renderer::Renderer;
use std::time::{Duration, Instant};

pub struct CanyonAutopilot {
    pilot: CanyonPilot,
    t: f32,
    max_s: f32,
    highwater: f64,
    last_shots: u64,
    last_activity: f32,
    next_print: f32,
    last_key: String,
    exit_at: Option<(Instant, i32, &'static str)>,
}

pub fn requested() -> bool {
    std::env::var("KESSEL_CANYON_AUTOPILOT").as_deref() == Ok("1")
}

impl CanyonAutopilot {
    pub fn new(g: &Game) -> Self {
        let max_s = std::env::var("KESSEL_CANYON_AUTOPILOT_MAX_S").ok()
            .and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite() && *v > 0.0).unwrap_or(2400.0);
        println!("[autopilot] rule-based Canyon crew drives this run (max {max_s} s)");
        Self {
            pilot: CanyonPilot::new(g), t: 0.0, max_s, highwater: 0.0, last_shots: 0,
            last_activity: 0.0, next_print: 0.0, last_key: String::new(), exit_at: None,
        }
    }

    /// The crew's input for this frame, plus the trace line (`[canyon-ai]`, the headless format)
    /// whenever the beat, phase, shots, kills, hits, breech or module state change, and every 10 s.
    pub fn step(&mut self, g: &mut Game, r: &Renderer, dt: f32) -> TankInput {
        let input = self.pilot.step(g, r, self.t, dt);
        self.t += dt;
        if self.exit_at.is_none() && self.t >= self.next_print {
            self.next_print = self.t + 1.0;
            let mut state = g.canyon_ai_state(&self.pilot, self.t);
            state["mission_beat"] = serde_json::json!(g.mission_current_beat());
            let key = format!("{}|{}|{}|{}|{}|{}|{}", state["mission_beat"], state["phase"], state["shots"],
                state["kills"], state["incoming_hits"], state["breech"], state["broken_modules"]);
            if key != self.last_key || (self.t as u32) % 10 == 0 {
                println!("[canyon-ai] {state}");
                self.last_key = key;
            }
            let progress = state["progress"].as_f64().unwrap_or(0.0);
            let shots = state["shots"].as_u64().unwrap_or(0);
            if progress > self.highwater + 0.002 || shots != self.last_shots {
                self.highwater = self.highwater.max(progress);
                self.last_shots = shots;
                self.last_activity = self.t;
            }
            if self.t - self.last_activity > 240.0 {
                self.finish(3, "pilot_stalled", Duration::ZERO);
            } else if self.t > self.max_s {
                self.finish(4, "time_limit", Duration::ZERO);
            }
        }
        input
    }

    /// The after-action card is showing: leave it up a few seconds for the log, then exit.
    pub fn run_over(&mut self, completed: bool) {
        if completed { self.finish(0, "completed", Duration::from_secs(4)) }
        else { self.finish(1, "knocked_out", Duration::from_secs(4)) }
    }

    fn finish(&mut self, code: i32, outcome: &'static str, after: Duration) {
        if self.exit_at.is_none() {
            println!("[autopilot-result] {{\"outcome\":\"{outcome}\",\"play_s\":{:.1},\"progress\":{:.4},\"reloads\":{}}}",
                self.t, self.highwater, self.pilot.reloads);
            self.exit_at = Some((Instant::now() + after, code, outcome));
        }
    }

    /// Exit code once the run is over and its hold has elapsed.
    pub fn exit_due(&self) -> Option<i32> {
        self.exit_at.filter(|(at, _, _)| Instant::now() >= *at).map(|(_, code, _)| code)
    }
}
