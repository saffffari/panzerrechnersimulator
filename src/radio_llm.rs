//! Platoon radio: the LLM layer that turns the commander's free-text radio traffic into ONE
//! structured order from a fixed grammar, plus a terse reply per addressed wingman.
//!
//! Boundaries (docs/PLATOON_MODE.md):
//! * The model never steers a tank. It emits `{addressee, order, target, replies}`; the order is
//!   validated against [`OrderKind`] and executed by the deterministic platoon AI (`src/platoon.rs`)
//!   at a frame boundary. Words and orders are the only things that cross from the model into the sim.
//! * Calls run on one worker thread. The frame only [`RadioLlm::submit`]s and [`RadioLlm::poll`]s.
//! * With no `ANTHROPIC_API_KEY`, no network, a timeout, a refusal, or an invalid response, the same
//!   transmission goes through [`parse_offline`] (keyword parser + templated replies that read the
//!   same context), so the mode works with no key and no network. Every message logs which path
//!   handled it (`[radio]` lines) and the worker keeps a transcript for the headless proof.
//! * Transport is the system `curl` (Schannel on Windows, OpenSSL on Linux) driven through a config
//!   file on stdin, so the key never appears in a command line and no new crate touches Cargo.lock.
//!
//! Model: `claude-haiku-5-5` by default (fastest current model; a radio call is a tiny
//! classification + one line of text). Override with `KESSEL_RADIO_MODEL`. Effort is `low`.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const API_VERSION: &str = "2026-10-21";
pub const DEFAULT_MODEL: &str = "claude-haiku-5-5";
pub const DEFAULT_TIMEOUT_MS: u64 = 6000;
pub const PLAYER_CALLSIGN: &str = "Zugführer";

/// Price table (USD per million tokens, input/output) for the cost line in the log. Unknown models
/// report `cost=n/a`.
const PRICES: &[(&str, f64, f64)] = &[
    ("claude-haiku-5-5", 0.10, 0.50),
    ("claude-sonnet-5-5", 2.00, 10.00),
    ("claude-opus-5-5", 4.00, 20.00),
];

// ------------------------------------------------------------------------------------------------
// Order grammar
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrderKind {
    Follow,
    Hold,
    Advance,
    FlankLeft,
    FlankRight,
    Engage,
    CoverMe,
    CeaseFire,
    FallBack,
    ReportStatus,
    Spot,
}

impl OrderKind {
    pub const ALL: [OrderKind; 11] = [
        OrderKind::Follow,
        OrderKind::Hold,
        OrderKind::Advance,
        OrderKind::FlankLeft,
        OrderKind::FlankRight,
        OrderKind::Engage,
        OrderKind::CoverMe,
        OrderKind::CeaseFire,
        OrderKind::FallBack,
        OrderKind::ReportStatus,
        OrderKind::Spot,
    ];

    pub fn name(self) -> &'static str {
        match self {
            OrderKind::Follow => "follow",
            OrderKind::Hold => "hold",
            OrderKind::Advance => "advance",
            OrderKind::FlankLeft => "flank_left",
            OrderKind::FlankRight => "flank_right",
            OrderKind::Engage => "engage",
            OrderKind::CoverMe => "cover_me",
            OrderKind::CeaseFire => "cease_fire",
            OrderKind::FallBack => "fall_back",
            OrderKind::ReportStatus => "report_status",
            OrderKind::Spot => "spot",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        OrderKind::ALL.iter().copied().find(|k| k.name() == s)
    }

    /// Short meaning handed to the model and printed in the docs.
    pub fn meaning(self) -> &'static str {
        match self {
            OrderKind::Follow => "resume formation behind the leader",
            OrderKind::Hold => "stop where you are, keep weapons ready",
            OrderKind::Advance => "move to the target (marker, bearing, or forward along the route)",
            OrderKind::FlankLeft => "swing wide to the leader's left and push forward",
            OrderKind::FlankRight => "swing wide to the leader's right and push forward",
            OrderKind::Engage => "fire at the target contact or sector; if none given, nearest threat",
            OrderKind::CoverMe => "halt in a firing position and overwatch the leader's heading",
            OrderKind::CeaseFire => "stop shooting, keep tracking",
            OrderKind::FallBack => "reverse out of contact toward the leader's rear",
            OrderKind::ReportStatus => "report ammunition, damage, crew",
            OrderKind::Spot => "report what you can see from your position",
        }
    }
}

/// What an order points at. Bearings are compass degrees (0 = north, clockwise), as the context
/// hands them to the model.
#[derive(Clone, Debug, PartialEq)]
pub enum OrderTarget {
    None,
    Bearing(f32),
    /// An exact contact label from the context (`"T-34 #2"`).
    Contact(String),
    /// A route marker name from the context.
    Marker(String),
    /// `left` / `right` / `front` / `rear`, relative to the leader's heading.
    Sector(String),
}

impl OrderTarget {
    pub fn describe(&self) -> String {
        match self {
            OrderTarget::None => "-".to_string(),
            OrderTarget::Bearing(b) => format!("bearing {:.0}", b),
            OrderTarget::Contact(c) => format!("contact {c}"),
            OrderTarget::Marker(m) => format!("marker {m}"),
            OrderTarget::Sector(s) => format!("sector {s}"),
        }
    }

    /// Parse the model's free `target` string into the typed form, checking contact and marker
    /// names against the context so the model cannot invent a target.
    pub fn parse(raw: &str, ctx: &RadioContext) -> OrderTarget {
        let t = raw.trim().to_ascii_lowercase();
        if t.is_empty() || t == "none" || t == "-" || t == "null" {
            return OrderTarget::None;
        }
        for w in ["left", "right", "front", "rear"] {
            if t == w || t == format!("sector {w}") {
                return OrderTarget::Sector(w.to_string());
            }
        }
        if let Some(rest) = t.strip_prefix("bearing") {
            if let Ok(b) = rest.trim().trim_end_matches('°').parse::<f32>() {
                return OrderTarget::Bearing(b.rem_euclid(360.0));
            }
        }
        if let Ok(b) = t.trim_end_matches('°').parse::<f32>() {
            return OrderTarget::Bearing(b.rem_euclid(360.0));
        }
        let body = t.strip_prefix("contact").unwrap_or(&t).trim();
        for wm in &ctx.wingmen {
            for c in &wm.contacts {
                if c.label.to_ascii_lowercase() == body {
                    return OrderTarget::Contact(c.label.clone());
                }
            }
        }
        for c in &ctx.leader_contacts {
            if c.label.to_ascii_lowercase() == body {
                return OrderTarget::Contact(c.label.clone());
            }
        }
        let body = t.strip_prefix("marker").unwrap_or(&t).trim();
        for m in &ctx.markers {
            if m.to_ascii_lowercase() == body {
                return OrderTarget::Marker(m.clone());
            }
        }
        OrderTarget::None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Addressee {
    All,
    Callsign(String),
}

impl Addressee {
    pub fn describe(&self) -> String {
        match self {
            Addressee::All => "all".to_string(),
            Addressee::Callsign(c) => c.clone(),
        }
    }
    pub fn includes(&self, callsign: &str) -> bool {
        match self {
            Addressee::All => true,
            Addressee::Callsign(c) => c.eq_ignore_ascii_case(callsign),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadioOrder {
    pub addressee: Addressee,
    pub kind: OrderKind,
    pub target: OrderTarget,
}

// ------------------------------------------------------------------------------------------------
// Context: what the sim hands the model (and the offline parser) about each wingman
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ContactContext {
    pub label: String,
    pub bearing_deg: f32,
    pub range_m: f32,
    /// `moving` / `stationary` / `burning` / `dead`
    pub state: String,
}

#[derive(Clone, Debug, Default)]
pub struct WingmanContext {
    pub callsign: String,
    pub commander: String,
    pub personality: String,
    pub alive: bool,
    /// Where the wingman is seen from the leader.
    pub bearing_from_leader_deg: f32,
    pub range_from_leader_m: f32,
    pub heading_deg: f32,
    pub speed_kph: f32,
    pub current_order: String,
    pub ap_rounds: u32,
    pub he_rounds: u32,
    pub hull_hits: u32,
    pub damage: Vec<String>,
    pub crew_dead: Vec<String>,
    pub contacts: Vec<ContactContext>,
}

#[derive(Clone, Debug, Default)]
pub struct RadioContext {
    pub leader_heading_deg: f32,
    pub leader_speed_kph: f32,
    pub clock: String,
    pub weather: String,
    pub leader_contacts: Vec<ContactContext>,
    pub markers: Vec<String>,
    pub wingmen: Vec<WingmanContext>,
}

impl RadioContext {
    pub fn to_json(&self) -> Value {
        let contact = |c: &ContactContext| {
            json!({"label": c.label, "bearing": c.bearing_deg.round(), "range_m": c.range_m.round(), "state": c.state})
        };
        json!({
            "leader": {
                "callsign": PLAYER_CALLSIGN,
                "heading": self.leader_heading_deg.round(),
                "speed_kph": self.leader_speed_kph.round(),
                "contacts": self.leader_contacts.iter().map(contact).collect::<Vec<_>>(),
            },
            "clock": self.clock,
            "weather": self.weather,
            "markers": self.markers,
            "wingmen": self.wingmen.iter().map(|w| json!({
                "callsign": w.callsign,
                "commander": w.commander,
                "personality": w.personality,
                "alive": w.alive,
                "bearing_from_leader": w.bearing_from_leader_deg.round(),
                "range_from_leader_m": w.range_from_leader_m.round(),
                "heading": w.heading_deg.round(),
                "speed_kph": w.speed_kph.round(),
                "current_order": w.current_order,
                "ammo": {"ap": w.ap_rounds, "he": w.he_rounds},
                "hull_hits_taken": w.hull_hits,
                "damage": w.damage,
                "crew_dead": w.crew_dead,
                "contacts": w.contacts.iter().map(contact).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

// ------------------------------------------------------------------------------------------------
// Results
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct RadioReply {
    pub callsign: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum RadioPath {
    Llm {
        model: String,
        latency_ms: u64,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: Option<f64>,
    },
    Fallback {
        reason: String,
        latency_ms: u64,
    },
}

impl RadioPath {
    pub fn label(&self) -> &'static str {
        match self {
            RadioPath::Llm { .. } => "llm",
            RadioPath::Fallback { .. } => "fallback",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RadioResult {
    pub request_id: u64,
    pub transmission: String,
    pub order: Option<RadioOrder>,
    pub replies: Vec<RadioReply>,
    pub path: RadioPath,
}

impl RadioResult {
    pub fn log_line(&self) -> String {
        let order = match &self.order {
            Some(o) => format!("{} to={} target={}", o.kind.name(), o.addressee.describe(), o.target.describe()),
            None => "none".to_string(),
        };
        match &self.path {
            RadioPath::Llm { model, latency_ms, input_tokens, output_tokens, cost_usd } => format!(
                "[radio] #{} path=llm model={} latency={}ms in={} out={} cost={} order={} replies={}",
                self.request_id,
                model,
                latency_ms,
                input_tokens,
                output_tokens,
                cost_usd.map(|c| format!("${c:.6}")).unwrap_or_else(|| "n/a".into()),
                order,
                self.replies.len()
            ),
            RadioPath::Fallback { reason, latency_ms } => format!(
                "[radio] #{} path=fallback reason={} latency={}ms order={} replies={}",
                self.request_id, reason, latency_ms, order, self.replies.len()
            ),
        }
    }

    pub fn to_json(&self) -> Value {
        let path = match &self.path {
            RadioPath::Llm { model, latency_ms, input_tokens, output_tokens, cost_usd } => json!({
                "path": "llm", "model": model, "latency_ms": latency_ms,
                "input_tokens": input_tokens, "output_tokens": output_tokens, "cost_usd": cost_usd,
            }),
            RadioPath::Fallback { reason, latency_ms } => {
                json!({"path": "fallback", "reason": reason, "latency_ms": latency_ms})
            }
        };
        json!({
            "id": self.request_id,
            "transmission": self.transmission,
            "order": self.order.as_ref().map(|o| json!({
                "addressee": o.addressee.describe(), "kind": o.kind.name(), "target": o.target.describe()
            })),
            "replies": self.replies.iter().map(|r| json!({"callsign": r.callsign, "text": r.text})).collect::<Vec<_>>(),
            "path": path,
        })
    }
}

// ------------------------------------------------------------------------------------------------
// The worker
// ------------------------------------------------------------------------------------------------

struct Request {
    id: u64,
    text: String,
    ctx: RadioContext,
}

#[derive(Clone, Debug)]
pub struct RadioConfig {
    pub api_key: Option<String>,
    pub model: String,
    pub timeout: Duration,
    pub curl: String,
    /// `KESSEL_RADIO_OFFLINE=1` forces the keyword parser even with a key (deterministic proofs).
    pub force_offline: bool,
}

impl RadioConfig {
    pub fn from_env() -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty());
        let model = std::env::var("KESSEL_RADIO_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        let timeout_ms = std::env::var("KESSEL_RADIO_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let curl = std::env::var("KESSEL_RADIO_CURL").unwrap_or_else(|_| "curl".into());
        let force_offline = std::env::var("KESSEL_RADIO_OFFLINE").map(|v| v == "1").unwrap_or(false);
        RadioConfig { api_key, model, timeout: Duration::from_millis(timeout_ms), curl, force_offline }
    }

    pub fn online(&self) -> bool {
        self.api_key.is_some() && !self.force_offline
    }

    pub fn mode_label(&self) -> String {
        if self.online() {
            format!("llm:{}", self.model)
        } else if self.force_offline {
            "offline (KESSEL_RADIO_OFFLINE=1)".into()
        } else {
            "offline (no ANTHROPIC_API_KEY)".into()
        }
    }
}

pub struct RadioLlm {
    tx: Sender<Request>,
    rx: Receiver<RadioResult>,
    next_id: u64,
    pending: usize,
    config: RadioConfig,
    pub transcript: Vec<RadioResult>,
}

impl Default for RadioLlm {
    fn default() -> Self {
        Self::new()
    }
}

impl RadioLlm {
    pub fn new() -> Self {
        Self::with_config(RadioConfig::from_env())
    }

    pub fn with_config(config: RadioConfig) -> Self {
        let (tx, worker_rx) = mpsc::channel::<Request>();
        let (result_tx, rx) = mpsc::channel::<RadioResult>();
        let worker_cfg = config.clone();
        std::thread::Builder::new()
            .name("radio-llm".into())
            .spawn(move || {
                while let Ok(req) = worker_rx.recv() {
                    let result = handle_request(&worker_cfg, req);
                    println!("{}", result.log_line());
                    if result_tx.send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn radio-llm worker");
        println!("[radio] net up, path={}", config.mode_label());
        RadioLlm { tx, rx, next_id: 1, pending: 0, config, transcript: Vec::new() }
    }

    pub fn config(&self) -> &RadioConfig {
        &self.config
    }

    pub fn pending(&self) -> usize {
        self.pending
    }

    /// Queue a transmission. Returns its id; the result arrives through [`RadioLlm::poll`].
    pub fn submit(&mut self, text: &str, ctx: RadioContext) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.pending += 1;
        let _ = self.tx.send(Request { id, text: text.to_string(), ctx });
        id
    }

    /// Drain finished results. Call once per frame; apply the orders at that frame boundary.
    pub fn poll(&mut self) -> Vec<RadioResult> {
        let mut out = Vec::new();
        while let Ok(r) = self.rx.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            self.transcript.push(r.clone());
            out.push(r);
        }
        out
    }

    /// Blocking helper for headless proofs: wait for one specific result (or the timeout).
    pub fn wait_for(&mut self, id: u64, max: Duration) -> Option<RadioResult> {
        let start = Instant::now();
        loop {
            for r in self.poll() {
                if r.request_id == id {
                    return Some(r);
                }
            }
            if start.elapsed() > max {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn handle_request(cfg: &RadioConfig, req: Request) -> RadioResult {
    let started = Instant::now();
    if cfg.online() {
        match call_llm(cfg, &req.text, &req.ctx) {
            Ok((order, replies, input_tokens, output_tokens)) => {
                let latency_ms = started.elapsed().as_millis() as u64;
                let cost_usd = PRICES
                    .iter()
                    .find(|(m, _, _)| *m == cfg.model)
                    .map(|(_, i, o)| (input_tokens as f64 * i + output_tokens as f64 * o) / 1_000_000.0);
                return RadioResult {
                    request_id: req.id,
                    transmission: req.text,
                    order,
                    replies,
                    path: RadioPath::Llm { model: cfg.model.clone(), latency_ms, input_tokens, output_tokens, cost_usd },
                };
            }
            Err(reason) => {
                let (order, replies) = parse_offline(&req.text, &req.ctx);
                return RadioResult {
                    request_id: req.id,
                    transmission: req.text,
                    order,
                    replies,
                    path: RadioPath::Fallback { reason, latency_ms: started.elapsed().as_millis() as u64 },
                };
            }
        }
    }
    let reason = if cfg.force_offline { "forced_offline" } else { "no_key" };
    let (order, replies) = parse_offline(&req.text, &req.ctx);
    RadioResult {
        request_id: req.id,
        transmission: req.text,
        order,
        replies,
        path: RadioPath::Fallback { reason: reason.into(), latency_ms: started.elapsed().as_millis() as u64 },
    }
}

// ------------------------------------------------------------------------------------------------
// Prompt
// ------------------------------------------------------------------------------------------------

pub fn system_prompt() -> String {
    let mut s = String::new();
    s.push_str("You are the radio net of a German heavy tank platoon: three Tigers breaking out of the Stalingrad pocket, winter 1942-43. ");
    s.push_str("The platoon leader (callsign ");
    s.push_str(PLAYER_CALLSIGN);
    s.push_str(") keys the set and speaks; you turn the transmission into exactly one order from the grammar for the addressed tank(s), then write each addressed wingman's reply.\n\n");
    s.push_str("ORDER GRAMMAR (use the exact id):\n");
    for k in OrderKind::ALL {
        s.push_str(&format!("- {}: {}\n", k.name(), k.meaning()));
    }
    s.push_str("- none: the transmission is not an order (acknowledgement, chatter, unclear). Addressed tanks still answer: ask the leader to say again, or acknowledge.\n\n");
    s.push_str("TARGET: \"\" when none; \"bearing N\" (compass degrees, 0 = north, clockwise; convert clock positions using the leader's heading: 12 o'clock = leader heading); \"contact <label>\" where <label> is EXACTLY a contact label from the context; \"marker <name>\" EXACTLY from context markers; or \"left\" / \"right\" / \"front\" / \"rear\". Never invent a contact or marker.\n");
    s.push_str("ADDRESSEE: \"all\" or one exact callsign. A transmission naming no tank addresses all.\n\n");
    s.push_str("REPLIES: one entry per addressed tank that is alive (dead tanks do not answer). Period tank radio procedure, not conversation: 3 to 14 words, in the language the leader used, acknowledge the order in the tank's own words, state only facts in that tank's context (ammunition, damage, dead crew, contacts with bearing and range). ");
    s.push_str("No invented sightings, no chatter, no modern slang, no humour. End with \"Out.\" when nothing more is expected, \"Over.\" when the leader must answer. Let the one-line personality colour word choice slightly, never length. ");
    s.push_str("A report_status reply names rounds, damage and crew; a spot reply names contacts or says no contact.\n");
    s
}

fn output_schema() -> Value {
    let mut orders: Vec<&str> = OrderKind::ALL.iter().map(|k| k.name()).collect();
    orders.push("none");
    json!({
        "type": "object",
        "properties": {
            "addressee": {"type": "string"},
            "order": {"type": "string", "enum": orders},
            "target": {"type": "string"},
            "replies": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {"callsign": {"type": "string"}, "text": {"type": "string"}},
                    "required": ["callsign", "text"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["addressee", "order", "target", "replies"],
        "additionalProperties": false
    })
}

pub fn request_body(model: &str, text: &str, ctx: &RadioContext) -> Value {
    let user = format!(
        "CONTEXT (truth from the simulation):\n{}\n\nTRANSMISSION from {}: \"{}\"",
        ctx.to_json(),
        PLAYER_CALLSIGN,
        text.replace('"', "'")
    );
    json!({
        "model": model,
        "max_tokens": 400,
        "system": system_prompt(),
        "output_config": {"effort": "low", "format": {"type": "json_schema", "schema": output_schema()}},
        "messages": [{"role": "user", "content": user}],
    })
}

type LlmOk = (Option<RadioOrder>, Vec<RadioReply>, u64, u64);

fn call_llm(cfg: &RadioConfig, text: &str, ctx: &RadioContext) -> Result<LlmOk, String> {
    let key = cfg.api_key.as_deref().ok_or("no_key")?;
    let body = request_body(&cfg.model, text, ctx);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let body_path = std::env::temp_dir().join(format!("kessel_radio_{}_{}.json", std::process::id(), stamp));
    std::fs::write(&body_path, serde_json::to_vec(&body).map_err(|e| format!("encode:{e}"))?)
        .map_err(|e| format!("tmp:{e}"))?;
    let body_path_str = body_path.to_string_lossy().replace('\\', "/");
    let max_time = cfg.timeout.as_secs_f64().max(1.0);
    let config_text = format!(
        "url = \"{API_URL}\"\nrequest = \"POST\"\nmax-time = {max_time:.1}\nconnect-timeout = 3\nsilent\nshow-error\nheader = \"content-type: application/json\"\nheader = \"x-api-key: {key}\"\nheader = \"anthropic-version: {API_VERSION}\"\ndata-binary = \"@{body_path_str}\"\nwrite-out = \"\\n%{{http_code}}\"\n"
    );
    let mut cmd = Command::new(&cfg.curl);
    cmd.args(["-K", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash under the fullscreen game
    }
    let result = (|| -> Result<LlmOk, String> {
        let mut child = cmd.spawn().map_err(|e| format!("curl_spawn:{e}"))?;
        child
            .stdin
            .take()
            .ok_or("curl_stdin")?
            .write_all(config_text.as_bytes())
            .map_err(|e| format!("curl_stdin:{e}"))?;
        let out = child.wait_with_output().map_err(|e| format!("curl_wait:{e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let (payload, code) = match stdout.rfind('\n') {
            Some(i) => (&stdout[..i], stdout[i + 1..].trim().to_string()),
            None => (stdout.as_str(), String::new()),
        };
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let err = err.trim().split_whitespace().take(6).collect::<Vec<_>>().join("_");
            return Err(if code.is_empty() || code == "000" {
                format!("curl_exit_{}:{}", out.status.code().unwrap_or(-1), err)
            } else {
                format!("http_{code}")
            });
        }
        if code != "200" {
            let v: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
            let kind = v["error"]["type"].as_str().unwrap_or("unknown");
            return Err(format!("http_{code}:{kind}"));
        }
        let v: Value = serde_json::from_str(payload).map_err(|e| format!("bad_json:{e}"))?;
        parse_llm_response(&v, ctx)
    })();
    let _ = std::fs::remove_file(&body_path);
    result
}

/// Validate the model's JSON against the grammar and the context. Anything off-grammar is an
/// `Err`, which routes the transmission to the offline parser.
pub fn parse_llm_response(v: &Value, ctx: &RadioContext) -> Result<LlmOk, String> {
    let stop = v["stop_reason"].as_str().unwrap_or("");
    if stop == "refusal" {
        return Err("refusal".into());
    }
    let text = v["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .ok_or("no_text_block")?;
    let input_tokens = v["usage"]["input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = v["usage"]["output_tokens"].as_u64().unwrap_or(0);
    let parsed: Value = serde_json::from_str(text).map_err(|e| format!("bad_output_json:{e}"))?;
    let (order, replies) = validate_output(&parsed, ctx)?;
    Ok((order, replies, input_tokens, output_tokens))
}

pub fn validate_output(parsed: &Value, ctx: &RadioContext) -> Result<(Option<RadioOrder>, Vec<RadioReply>), String> {
    let addressee_raw = parsed["addressee"].as_str().unwrap_or("all");
    let addressee = if addressee_raw.eq_ignore_ascii_case("all") || addressee_raw.trim().is_empty() {
        Addressee::All
    } else {
        let cs = ctx
            .wingmen
            .iter()
            .find(|w| w.callsign.eq_ignore_ascii_case(addressee_raw.trim()))
            .map(|w| w.callsign.clone());
        match cs {
            Some(c) => Addressee::Callsign(c),
            None => return Err(format!("bad_addressee:{addressee_raw}")),
        }
    };
    let order_raw = parsed["order"].as_str().ok_or("no_order")?;
    let order = if order_raw.eq_ignore_ascii_case("none") {
        None
    } else {
        let kind = OrderKind::from_name(order_raw).ok_or_else(|| format!("bad_order:{order_raw}"))?;
        let target = OrderTarget::parse(parsed["target"].as_str().unwrap_or(""), ctx);
        Some(RadioOrder { addressee: addressee.clone(), kind, target })
    };
    let mut replies = Vec::new();
    if let Some(items) = parsed["replies"].as_array() {
        for r in items {
            let cs = r["callsign"].as_str().unwrap_or("").trim();
            let text = r["text"].as_str().unwrap_or("").trim();
            let Some(w) = ctx.wingmen.iter().find(|w| w.callsign.eq_ignore_ascii_case(cs)) else { continue };
            if !w.alive || text.is_empty() || !addressee.includes(&w.callsign) {
                continue;
            }
            let mut text = text.to_string();
            if text.chars().count() > 160 {
                text = text.chars().take(157).collect::<String>() + "...";
            }
            replies.push(RadioReply { callsign: w.callsign.clone(), text });
        }
    }
    Ok((order, replies))
}

// ------------------------------------------------------------------------------------------------
// Offline fallback: keyword parser + templated replies
// ------------------------------------------------------------------------------------------------

fn normalize(text: &str) -> String {
    let mut s = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        let c = ch.to_lowercase().next().unwrap_or(ch);
        if c.is_alphanumeric() || c == '#' {
            s.push(c);
        } else if c == '\'' {
            // "o'clock" -> "oclock"
        } else {
            s.push(' ');
        }
    }
    s.split_whitespace().fold(String::from(" "), |mut acc, w| {
        acc.push_str(w);
        acc.push(' ');
        acc
    })
}

fn has(norm: &str, phrase: &str) -> bool {
    norm.contains(&format!(" {phrase} "))
}

fn has_any(norm: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|p| has(norm, p))
}

const KW_CEASE: &[&str] = &[
    "cease fire", "ceasefire", "hold fire", "hold your fire", "stop firing", "stop shooting", "check fire",
    "weapons hold", "feuer einstellen", "feuer halt", "nicht schiessen", "nicht schießen",
];
const KW_COVER: &[&str] = &["cover me", "cover", "overwatch", "covering fire", "deckung", "deck mich", "sichern", "feuerschutz"];
const KW_FALLBACK: &[&str] = &[
    "fall back", "fallback", "pull back", "withdraw", "retreat", "disengage", "back off", "reverse", "zurück",
    "zurueck", "absetzen", "rückzug", "rueckzug",
];
const KW_FLANK_L: &[&str] = &["flank left", "left flank", "go left", "swing left", "around the left", "links umgehen", "links rum", "nach links"];
const KW_FLANK_R: &[&str] = &["flank right", "right flank", "go right", "swing right", "around the right", "rechts umgehen", "rechts rum", "nach rechts"];
const KW_FLANK: &[&str] = &["flank", "umgehen", "umfassen"];
const KW_ENGAGE: &[&str] = &[
    "engage", "open fire", "fire at", "fire on", "fire", "attack", "kill", "take out", "destroy", "shoot", "hit",
    "target", "weapons free", "feuer frei", "angreifen", "vernichten", "beschiessen", "beschießen", "feuer",
];
const KW_STATUS: &[&str] = &[
    "status", "report", "sitrep", "ammo", "ammunition", "damage", "how are you", "how bad", "casualties",
    "lagebericht", "meldung", "zustand", "munition", "schaden", "wie steht",
];
const KW_SPOT: &[&str] = &[
    "what do you see", "what can you see", "spot", "any contact", "any contacts", "contacts", "do you see",
    "eyes on", "scan", "anything", "sighting", "was seht ihr", "was siehst du", "sichtung", "feindlage",
    "feind in sicht",
];
const KW_HOLD: &[&str] = &[
    "hold", "halt", "stop", "stay", "wait", "stand by", "hold position", "stay put", "halten", "stehen bleiben",
    "warten", "stellung halten",
];
const KW_ADVANCE: &[&str] = &[
    "advance", "move up", "move to", "move out", "go to", "push", "push up", "push forward", "forward", "proceed",
    "head to", "head for", "take the", "vorwärts", "vorwaerts", "vorgehen", "vorrücken", "vorruecken", "marsch", "los",
];
const KW_FOLLOW: &[&str] = &[
    "follow", "follow me", "form up", "on me", "with me", "rally", "regroup", "formation", "close up", "come along",
    "folgen", "folgt mir", "aufschliessen", "aufschließen", "mir nach", "sammeln",
];

fn parse_addressee(norm: &str, ctx: &RadioContext) -> Addressee {
    let mut hits: Vec<String> = Vec::new();
    for w in &ctx.wingmen {
        let cs = w.callsign.to_lowercase();
        let surname = w.commander.split_whitespace().last().unwrap_or("").to_lowercase();
        if has(norm, &cs) || (!surname.is_empty() && has(norm, &surname)) {
            hits.push(w.callsign.clone());
        }
    }
    let all_words = ["all", "everyone", "everybody", "both", "platoon", "all tanks", "all stations", "alle", "zug", "an alle"];
    if has_any(norm, &all_words) || hits.len() != 1 {
        Addressee::All
    } else {
        Addressee::Callsign(hits.remove(0))
    }
}

fn parse_kind(norm: &str) -> Option<OrderKind> {
    if has_any(norm, KW_CEASE) {
        return Some(OrderKind::CeaseFire);
    }
    if has_any(norm, KW_COVER) {
        return Some(OrderKind::CoverMe);
    }
    if has_any(norm, KW_FALLBACK) {
        return Some(OrderKind::FallBack);
    }
    if has_any(norm, KW_FLANK_L) {
        return Some(OrderKind::FlankLeft);
    }
    if has_any(norm, KW_FLANK_R) {
        return Some(OrderKind::FlankRight);
    }
    if has_any(norm, KW_FLANK) {
        return Some(if has(norm, "right") || has(norm, "rechts") { OrderKind::FlankRight } else { OrderKind::FlankLeft });
    }
    if has_any(norm, KW_STATUS) {
        return Some(OrderKind::ReportStatus);
    }
    if has_any(norm, KW_SPOT) {
        return Some(OrderKind::Spot);
    }
    if has_any(norm, KW_ENGAGE) {
        return Some(OrderKind::Engage);
    }
    if has_any(norm, KW_FOLLOW) {
        return Some(OrderKind::Follow);
    }
    if has_any(norm, KW_HOLD) {
        return Some(OrderKind::Hold);
    }
    if has_any(norm, KW_ADVANCE) {
        return Some(OrderKind::Advance);
    }
    None
}

fn word_number(w: &str) -> Option<u32> {
    match w {
        "one" | "eins" => Some(1),
        "two" | "zwei" | "zwo" => Some(2),
        "three" | "drei" => Some(3),
        "four" | "vier" => Some(4),
        "five" | "fünf" | "fuenf" => Some(5),
        "six" | "sechs" => Some(6),
        "seven" | "sieben" => Some(7),
        "eight" | "acht" => Some(8),
        "nine" | "neun" => Some(9),
        "ten" | "zehn" => Some(10),
        "eleven" | "elf" => Some(11),
        "twelve" | "zwölf" | "zwoelf" => Some(12),
        _ => w.parse().ok(),
    }
}

fn parse_target(norm: &str, kind: OrderKind, ctx: &RadioContext) -> OrderTarget {
    let words: Vec<&str> = norm.split_whitespace().collect();
    // "bearing 270" / "heading 270" / "azimuth 270" / "richtung 270"
    for (i, w) in words.iter().enumerate() {
        if matches!(*w, "bearing" | "heading" | "azimuth" | "richtung") {
            if let Some(n) = words.get(i + 1).and_then(|n| n.parse::<f32>().ok()) {
                return OrderTarget::Bearing(n.rem_euclid(360.0));
            }
        }
    }
    // "2 o'clock" / "two oclock" / "um zwei uhr": relative to the leader's heading
    for (i, w) in words.iter().enumerate() {
        if matches!(*w, "oclock" | "uhr") && i > 0 {
            if let Some(h) = word_number(words[i - 1]) {
                if (1..=12).contains(&h) {
                    return OrderTarget::Bearing((ctx.leader_heading_deg + (h % 12) as f32 * 30.0).rem_euclid(360.0));
                }
            }
        }
    }
    // Explicit contact label ("t-34 #2", "tank two", "number 2", "#2")
    let mut labels: Vec<&str> = ctx.leader_contacts.iter().map(|c| c.label.as_str()).collect();
    for w in &ctx.wingmen {
        labels.extend(w.contacts.iter().map(|c| c.label.as_str()));
    }
    labels.sort_unstable();
    labels.dedup();
    for label in &labels {
        let l = normalize(label);
        if norm.contains(l.trim_end()) {
            return OrderTarget::Contact(label.to_string());
        }
        if let Some(num) = label.rsplit('#').next().and_then(|n| n.trim().parse::<u32>().ok()) {
            for (i, w) in words.iter().enumerate() {
                let tankish = matches!(*w, "tank" | "t34" | "t" | "number" | "nr" | "panzer" | "contact" | "target");
                if tankish {
                    if let Some(n) = words.get(i + 1).and_then(|n| word_number(n.trim_start_matches('#'))) {
                        if n == num {
                            return OrderTarget::Contact(label.to_string());
                        }
                    }
                }
                if w.starts_with('#') && w.trim_start_matches('#').parse::<u32>().ok() == Some(num) {
                    return OrderTarget::Contact(label.to_string());
                }
            }
        }
    }
    for m in &ctx.markers {
        let l = normalize(m);
        if norm.contains(l.trim_end()) {
            return OrderTarget::Marker(m.clone());
        }
    }
    // Sector words (flank orders consume left/right themselves; "fall back" consumes "back")
    if !matches!(kind, OrderKind::FlankLeft | OrderKind::FlankRight | OrderKind::FallBack) {
        if has_any(norm, &["left", "links"]) {
            return OrderTarget::Sector("left".into());
        }
        if has_any(norm, &["right", "rechts"]) {
            return OrderTarget::Sector("right".into());
        }
        if has_any(norm, &["front", "ahead", "forward", "vorn", "vorne", "voraus"]) {
            return OrderTarget::Sector("front".into());
        }
        if has_any(norm, &["rear", "behind", "back", "hinten", "hinter uns"]) {
            return OrderTarget::Sector("rear".into());
        }
    }
    OrderTarget::None
}

/// The no-network path. Same inputs as the model, deterministic output.
pub fn parse_offline(text: &str, ctx: &RadioContext) -> (Option<RadioOrder>, Vec<RadioReply>) {
    let norm = normalize(text);
    let addressee = parse_addressee(&norm, ctx);
    let kind = parse_kind(&norm);
    let order = kind.map(|kind| RadioOrder { addressee: addressee.clone(), kind, target: parse_target(&norm, kind, ctx) });
    let mut replies = Vec::new();
    for w in &ctx.wingmen {
        if !w.alive || !addressee.includes(&w.callsign) {
            continue;
        }
        let text = match &order {
            Some(o) => compose_reply_offline(o, w),
            None => "Say again, over.".to_string(),
        };
        replies.push(RadioReply { callsign: w.callsign.clone(), text });
    }
    (order, replies)
}

fn nearest_contact(w: &WingmanContext) -> Option<&ContactContext> {
    w.contacts.iter().filter(|c| c.state != "dead").min_by(|a, b| a.range_m.total_cmp(&b.range_m))
}

fn contact_phrase(c: &ContactContext) -> String {
    format!("{} bearing {:.0}, {:.0} metres, {}", c.label, c.bearing_deg, (c.range_m / 50.0).round() * 50.0, c.state)
}

/// Templated replies that read the same state the model gets, so offline answers stay truthful.
pub fn compose_reply_offline(order: &RadioOrder, w: &WingmanContext) -> String {
    let live: Vec<&ContactContext> = w.contacts.iter().filter(|c| c.state != "dead").collect();
    match order.kind {
        OrderKind::Follow => "Verstanden. Forming up. Out.".into(),
        OrderKind::Hold => "Verstanden. Holding here. Out.".into(),
        OrderKind::Advance => match &order.target {
            OrderTarget::Marker(m) => format!("Verstanden. Moving to {m}. Out."),
            OrderTarget::Bearing(b) => format!("Verstanden. Advancing bearing {b:.0}. Out."),
            OrderTarget::Sector(s) => format!("Verstanden. Pushing {s}. Out."),
            _ => "Verstanden. Advancing. Out.".into(),
        },
        OrderKind::FlankLeft => "Verstanden. Going left. Out.".into(),
        OrderKind::FlankRight => "Verstanden. Going right. Out.".into(),
        OrderKind::Engage => {
            if w.ap_rounds == 0 && w.he_rounds == 0 {
                return "Negative. No ammunition left. Over.".into();
            }
            if w.damage.iter().any(|d| d.contains("gun")) {
                return "Negative. Gun out of action. Over.".into();
            }
            match &order.target {
                OrderTarget::Contact(c) => format!("Verstanden. Engaging {c}. Out."),
                OrderTarget::Bearing(b) => format!("Verstanden. Engaging bearing {b:.0}. Out."),
                OrderTarget::Sector(s) => format!("Verstanden. Engaging {s}. Out."),
                _ => match nearest_contact(w) {
                    Some(c) => format!("Verstanden. Engaging {}. Out.", c.label),
                    None => "No target in sight. Over.".into(),
                },
            }
        }
        OrderKind::CoverMe => "Verstanden. In position, covering you. Out.".into(),
        OrderKind::CeaseFire => "Verstanden. Ceasing fire. Out.".into(),
        OrderKind::FallBack => "Verstanden. Falling back. Out.".into(),
        OrderKind::ReportStatus => {
            let damage = if w.damage.is_empty() { "no damage".to_string() } else { w.damage.join(", ") };
            let crew = if w.crew_dead.is_empty() { "crew complete".to_string() } else { format!("{} dead", w.crew_dead.join(", ")) };
            let contact = match live.len() {
                0 => "no contact".to_string(),
                n => format!("{n} contacts"),
            };
            format!("{} AP, {} HE, {}, {}, {}. Out.", w.ap_rounds, w.he_rounds, damage, crew, contact)
        }
        OrderKind::Spot => {
            if live.is_empty() {
                "No contact. Out.".into()
            } else {
                let mut sorted = live.clone();
                sorted.sort_by(|a, b| a.range_m.total_cmp(&b.range_m));
                let listed: Vec<String> = sorted.iter().take(2).map(|c| contact_phrase(c)).collect();
                let more = if sorted.len() > 2 { format!(", {} more", sorted.len() - 2) } else { String::new() };
                format!("{}{}. Out.", listed.join("; "), more)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> RadioContext {
        let mut ctx = RadioContext { leader_heading_deg: 270.0, ..Default::default() };
        let mut a = WingmanContext {
            callsign: "Anton".into(),
            commander: "Feldwebel Hartmann".into(),
            alive: true,
            ap_rounds: 40,
            he_rounds: 30,
            ..Default::default()
        };
        a.contacts.push(ContactContext { label: "T-34 #2".into(), bearing_deg: 300.0, range_m: 640.0, state: "moving".into() });
        let b = WingmanContext { callsign: "Berta".into(), commander: "Unteroffizier Weiss".into(), alive: true, ..Default::default() };
        ctx.wingmen = vec![a, b];
        ctx.markers = vec!["Mühle".into()];
        ctx
    }

    #[test]
    fn keyword_orders() {
        let c = ctx();
        let cases = [
            ("Anton, follow me", OrderKind::Follow),
            ("all tanks hold", OrderKind::Hold),
            ("Berta hold fire", OrderKind::CeaseFire),
            ("engage the T-34 #2", OrderKind::Engage),
            ("Anton cover me", OrderKind::CoverMe),
            ("fall back, both of you", OrderKind::FallBack),
            ("Berta flank right", OrderKind::FlankRight),
            ("flank them", OrderKind::FlankLeft),
            ("status report", OrderKind::ReportStatus),
            ("what do you see", OrderKind::Spot),
            ("advance to the Mühle", OrderKind::Advance),
            ("Hartmann, feuer einstellen", OrderKind::CeaseFire),
        ];
        for (text, kind) in cases {
            let (o, _) = parse_offline(text, &c);
            assert_eq!(o.map(|o| o.kind), Some(kind), "{text}");
        }
    }

    #[test]
    fn addressee_and_targets() {
        let c = ctx();
        let (o, r) = parse_offline("Anton, engage T-34 #2", &c);
        let o = o.unwrap();
        assert_eq!(o.addressee, Addressee::Callsign("Anton".into()));
        assert_eq!(o.target, OrderTarget::Contact("T-34 #2".into()));
        assert_eq!(r.len(), 1);
        assert!(r[0].text.contains("T-34 #2"));
        let (o, _) = parse_offline("Hartmann, fire at tank two", &c);
        assert_eq!(o.unwrap().target, OrderTarget::Contact("T-34 #2".into()));
        let (o, r) = parse_offline("everyone engage bearing 300", &c);
        assert_eq!(o.unwrap().target, OrderTarget::Bearing(300.0));
        assert_eq!(r.len(), 2);
        let (o, _) = parse_offline("Berta, hit the one at 3 o'clock", &c);
        assert_eq!(o.unwrap().target, OrderTarget::Bearing(0.0));
        let (o, _) = parse_offline("Berta, move to the Mühle", &c);
        assert_eq!(o.unwrap().target, OrderTarget::Marker("Mühle".into()));
        let (o, r) = parse_offline("good morning", &c);
        assert!(o.is_none());
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn llm_output_validation() {
        let c = ctx();
        let good = json!({"addressee": "anton", "order": "engage", "target": "contact T-34 #2",
            "replies": [{"callsign": "Anton", "text": "Engaging. Out."}, {"callsign": "Berta", "text": "ignored"}]});
        let (o, r) = validate_output(&good, &c).unwrap();
        let o = o.unwrap();
        assert_eq!(o.kind, OrderKind::Engage);
        assert_eq!(o.target, OrderTarget::Contact("T-34 #2".into()));
        assert_eq!(r.len(), 1, "reply from a tank not addressed is dropped");
        let bad = json!({"addressee": "all", "order": "self_destruct", "target": "", "replies": []});
        assert!(validate_output(&bad, &c).is_err());
        let invented = json!({"addressee": "all", "order": "engage", "target": "contact KV-1 #9", "replies": []});
        assert_eq!(validate_output(&invented, &c).unwrap().0.unwrap().target, OrderTarget::None);
    }
}
