//! Dependency-free LAN crew co-op for Kessel.
//!
//! The simulation remains host authoritative. Clients send only their station's controls; the
//! host merges them, fills empty seats, advances the real `Game`, and broadcasts compact snapshots.
//! UDP is intentional here: station input and world snapshots are replaceable state, so a late
//! packet is worse than a lost one. Repeated hello/welcome packets make joining robust on a LAN.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 28_460;
const MAGIC: &[u8; 4] = b"KSL1";
const VERSION: u8 = 1;
const NONE_PEER: u32 = u32::MAX;
const MAX_PACKET: usize = 2_048;
const PEER_TIMEOUT: Duration = Duration::from_secs(5);
const HELLO_PERIOD: Duration = Duration::from_millis(350);
const INPUT_PERIOD: Duration = Duration::from_millis(20);
const WORLD_PERIOD: Duration = Duration::from_millis(50);

const P_HELLO: u8 = 1;
const P_WELCOME: u8 = 2;
const P_INPUT: u8 = 3;
const P_WORLD: u8 = 4;
const P_CLAIM: u8 = 5;
const P_FULL: u8 = 6;

pub const F_FIRE_MAIN: u16 = 1 << 0;
pub const F_FIRE_COAX: u16 = 1 << 1;
pub const F_FIRE_HULL: u16 = 1 << 2;
pub const F_SERVICE_BREECH: u16 = 1 << 3;
pub const F_IGNITION: u16 = 1 << 4;
pub const F_EXTINGUISH: u16 = 1 << 5;
pub const F_REPAIR: u16 = 1 << 6;
pub const F_BINOCULARS: u16 = 1 << 7;
pub const F_HATCH_TOGGLE: u16 = 1 << 8;
pub const F_DISMOUNTED: u16 = 1 << 9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum LanRole {
    #[default]
    Commander = 0,
    Driver = 1,
    Gunner = 2,
    Loader = 3,
    Radio = 4,
}

impl LanRole {
    pub const ALL: [Self; 5] = [
        Self::Commander,
        Self::Driver,
        Self::Gunner,
        Self::Loader,
        Self::Radio,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "commander" | "cmd" | "kommandant" => Some(Self::Commander),
            "driver" | "drv" | "fahrer" => Some(Self::Driver),
            "gunner" | "gun" | "richtschuetze" | "richtschutze" => Some(Self::Gunner),
            "loader" | "load" | "ladeschuetze" | "ladeschutze" => Some(Self::Loader),
            "radio" | "radioman" | "funker" => Some(Self::Radio),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Commander => "COMMANDER",
            Self::Driver => "DRIVER",
            Self::Gunner => "GUNNER",
            Self::Loader => "LOADER",
            Self::Radio => "RADIO / BOW MG",
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        Self::ALL.get(value as usize).copied()
    }

    pub fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LanMission {
    Garage = 0,
    Arena = 1,
    Campaign = 2,
}

impl LanMission {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "garage" | "training" => Some(Self::Garage),
            "arena" | "battle" | "gefecht" => Some(Self::Arena),
            "campaign" | "kampagne" => Some(Self::Campaign),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Garage => "GARAGE",
            Self::Arena => "ARENA",
            Self::Campaign => "CAMPAIGN",
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Garage),
            1 => Some(Self::Arena),
            2 => Some(Self::Campaign),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum LanModeConfig {
    Host { bind: String },
    Join { server: String },
}

#[derive(Clone, Debug)]
pub struct LanConfig {
    pub mode: LanModeConfig,
    pub requested_role: LanRole,
    pub name: String,
    pub mission: LanMission,
    pub seed: u64,
}

impl LanConfig {
    pub fn from_args(args: &[String]) -> Result<Option<Self>, String> {
        let mut host = false;
        let mut join: Option<String> = None;
        let mut bind = format!("0.0.0.0:{DEFAULT_PORT}");
        let mut role = LanRole::Commander;
        let mut name = std::env::var("USER").unwrap_or_else(|_| "crew".to_string());
        let mut mission = LanMission::Garage;
        let mut seed = 1u64;
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--host" => host = true,
                "--join" => {
                    i += 1;
                    join = Some(args.get(i).ok_or("--join needs HOST or HOST:PORT")?.clone());
                }
                "--bind" => {
                    i += 1;
                    bind = args.get(i).ok_or("--bind needs IP:PORT")?.clone();
                }
                "--role" => {
                    i += 1;
                    let value = args.get(i).ok_or("--role needs a crew role")?;
                    role =
                        LanRole::parse(value).ok_or_else(|| format!("unknown role '{value}'"))?;
                }
                "--name" => {
                    i += 1;
                    name = args.get(i).ok_or("--name needs a callsign")?.clone();
                }
                "--mission" => {
                    i += 1;
                    let value = args.get(i).ok_or("--mission needs garage|arena|campaign")?;
                    mission = LanMission::parse(value)
                        .ok_or_else(|| format!("unknown mission '{value}'"))?;
                }
                "--seed" => {
                    i += 1;
                    seed = args
                        .get(i)
                        .ok_or("--seed needs an integer")?
                        .parse()
                        .map_err(|_| "--seed needs an integer".to_string())?;
                }
                _ => {}
            }
            i += 1;
        }
        if host && join.is_some() {
            return Err("choose either --host or --join".into());
        }
        let mode = if host {
            Some(LanModeConfig::Host { bind })
        } else if let Some(mut server) = join {
            if !server.contains(':') {
                server.push_str(&format!(":{DEFAULT_PORT}"));
            }
            Some(LanModeConfig::Join { server })
        } else {
            None
        };
        Ok(mode.map(|mode| Self {
            mode,
            requested_role: role,
            name: name.chars().take(24).collect(),
            mission,
            seed,
        }))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoleInput {
    pub drive: f32,
    pub steer: f32,
    pub look_dx: f32,
    pub look_dy: f32,
    pub traverse: f32,
    pub elevate: f32,
    pub flags: u16,
    /// The physical perch this player currently occupies. The assigned role remains the authority
    /// channel; this drives interior crew bodies and prevents walking at the breech from becoming
    /// an accidental drive/fire command.
    pub post: LanRole,
}

impl RoleInput {
    pub fn flag(self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    fn sanitized(mut self) -> Self {
        self.drive = finite_unit(self.drive);
        self.steer = finite_unit(self.steer);
        self.traverse = finite_unit(self.traverse);
        self.elevate = finite_unit(self.elevate);
        self.look_dx = finite_clamp(self.look_dx, 4096.0);
        self.look_dy = finite_clamp(self.look_dy, 4096.0);
        self
    }
}

fn finite_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn finite_clamp(value: f32, cap: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-cap, cap)
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrewFrame {
    pub inputs: [RoleInput; 5],
    /// Actual human occupancy. Inputs for empty roles may still contain fallback/AI commands.
    pub roster: [u32; 5],
}

impl Default for CrewFrame {
    fn default() -> Self {
        Self {
            inputs: [RoleInput::default(); 5],
            roster: [NONE_PEER; 5],
        }
    }
}

impl CrewFrame {
    pub fn input(self, role: LanRole) -> RoleInput {
        self.inputs[role as usize]
    }

    pub fn occupied(self, role: LanRole) -> bool {
        self.roster[role as usize] != NONE_PEER
    }

    pub fn human_count(self) -> usize {
        self.roster.iter().filter(|&&id| id != NONE_PEER).count()
    }

    pub fn gunner_is_loader(self) -> bool {
        !self.occupied(LanRole::Loader)
            && self.occupied(LanRole::Gunner)
            && matches!(self.human_count(), 2 | 3)
    }
}

fn fill_empty_roles(mut frame: CrewFrame) -> CrewFrame {
    let fallback_role = if frame.occupied(LanRole::Commander) {
        LanRole::Commander
    } else {
        LanRole::ALL
            .into_iter()
            .find(|&role| frame.occupied(role))
            .unwrap_or(LanRole::Commander)
    };
    let fallback = frame.input(fallback_role);
    if !frame.occupied(LanRole::Commander) {
        frame.inputs[LanRole::Commander as usize] = fallback;
    }
    // A two-person tank lets the commander cover the missing driver. At three crew the doctrine
    // changes: somebody must actually claim and sit in Driver, so the machine cannot be driven by
    // a commander looking down from the cupola.
    if !frame.occupied(LanRole::Driver) && frame.human_count() <= 2 {
        frame.inputs[LanRole::Driver as usize] = fallback;
    }
    for role in [LanRole::Gunner, LanRole::Radio] {
        if !frame.occupied(role) {
            frame.inputs[role as usize] = fallback;
        }
    }
    if !frame.occupied(LanRole::Loader) {
        frame.inputs[LanRole::Loader as usize] = if frame.gunner_is_loader() {
            frame.input(LanRole::Gunner)
        } else {
            RoleInput {
                flags: F_SERVICE_BREECH,
                post: LanRole::Loader,
                ..RoleInput::default()
            }
        };
    }
    frame
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetTankState {
    pub pos: [f32; 3],
    pub heading: f32,
    pub speed: f32,
    pub turret_yaw: f32,
    pub gun_pitch: f32,
    pub reload: f32,
    pub hp: f32,
    pub alive: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldSnapshot {
    pub seq: u32,
    pub act: u8,
    pub player: NetTankState,
    pub enemies: Vec<NetTankState>,
    pub engine_on: bool,
    pub hatch_open: bool,
    pub breech: u8,
    pub rounds_spent: u32,
    pub he_loaded: bool,
    pub fire: f32,
    pub module_mask: u16,
    pub crew_mask: u8,
    pub act_complete: bool,
}

#[derive(Clone, Debug)]
pub struct LanPoll {
    pub crew: CrewFrame,
    pub snapshot: Option<WorldSnapshot>,
    pub connected: bool,
    pub status: String,
}

#[derive(Debug)]
struct Peer {
    id: u32,
    addr: SocketAddr,
    role: LanRole,
    name: String,
    input: RoleInput,
    last_seq: u32,
    last_seen: Instant,
}

#[derive(Debug)]
enum SessionKind {
    Host { peers: Vec<Peer>, next_id: u32 },
    Client { server: SocketAddr },
}

pub struct LanSession {
    socket: UdpSocket,
    kind: SessionKind,
    name: String,
    requested_role: LanRole,
    local_role: LanRole,
    local_id: u32,
    mission: LanMission,
    seed: u64,
    seq: u32,
    world_seq: u32,
    connected: bool,
    last_hello: Instant,
    last_input: Instant,
    last_world: Instant,
    last_rx: Instant,
    latest_crew: CrewFrame,
    pending_snapshot: Option<WorldSnapshot>,
    pending_pulses: u16,
    status: String,
}

impl LanSession {
    pub fn new(config: &LanConfig) -> io::Result<Self> {
        let now = Instant::now();
        let (socket, kind, connected, local_id, status) = match &config.mode {
            LanModeConfig::Host { bind } => {
                let socket = UdpSocket::bind(bind)?;
                socket.set_nonblocking(true)?;
                socket.set_broadcast(true)?;
                let addr = socket.local_addr()?;
                println!(
                    "[lan] HOST {addr} — {} — {}",
                    config.mission.label(),
                    config.requested_role.label()
                );
                (
                    socket,
                    SessionKind::Host {
                        peers: Vec::new(),
                        next_id: 1,
                    },
                    true,
                    0,
                    format!("HOST {}", addr),
                )
            }
            LanModeConfig::Join { server } => {
                let server = resolve_one(server)?;
                let bind = if server.is_ipv6() {
                    "[::]:0"
                } else {
                    "0.0.0.0:0"
                };
                let socket = UdpSocket::bind(bind)?;
                socket.set_nonblocking(true)?;
                println!(
                    "[lan] JOIN {server} — requesting {}",
                    config.requested_role.label()
                );
                (
                    socket,
                    SessionKind::Client { server },
                    false,
                    NONE_PEER,
                    format!("CONNECTING TO {server}"),
                )
            }
        };
        Ok(Self {
            socket,
            kind,
            name: config.name.clone(),
            requested_role: config.requested_role,
            local_role: config.requested_role,
            local_id,
            mission: config.mission,
            seed: config.seed,
            seq: 0,
            world_seq: 0,
            connected,
            last_hello: now - HELLO_PERIOD,
            last_input: now - INPUT_PERIOD,
            last_world: now - WORLD_PERIOD,
            last_rx: now,
            latest_crew: CrewFrame::default(),
            pending_snapshot: None,
            pending_pulses: 0,
            status,
        })
    }

    pub fn local_role(&self) -> LanRole {
        self.local_role
    }

    pub fn mission(&self) -> LanMission {
        self.mission
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn is_host(&self) -> bool {
        matches!(self.kind, SessionKind::Host { .. })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn cycle_role(&mut self) {
        self.requested_role = self.local_role.next();
        match &mut self.kind {
            SessionKind::Host { peers, .. } => {
                let wanted =
                    first_available(self.requested_role, self.local_id, self.local_role, peers);
                self.local_role = wanted;
                self.requested_role = wanted;
                self.status = format!("HOST — {}", wanted.label());
            }
            SessionKind::Client { server } => {
                let packet = encode_claim(self.local_id, self.requested_role);
                let _ = self.socket.send_to(&packet, *server);
                self.status = format!("REQUESTING {}", self.requested_role.label());
            }
        }
    }

    pub fn poll(&mut self, local_input: RoleInput) -> LanPoll {
        self.recv_all();
        let now = Instant::now();
        match &mut self.kind {
            SessionKind::Host { peers, .. } => {
                peers.retain(|peer| now.duration_since(peer.last_seen) <= PEER_TIMEOUT);
                let mut frame = CrewFrame::default();
                frame.roster[self.local_role as usize] = self.local_id;
                frame.inputs[self.local_role as usize] = local_input.sanitized();
                for peer in peers.iter() {
                    frame.roster[peer.role as usize] = peer.id;
                    frame.inputs[peer.role as usize] = peer.input;
                }
                self.latest_crew = fill_empty_roles(frame);
                self.connected = true;
                self.status = format!(
                    "HOST · {} · {}/5 CREW",
                    self.local_role.label(),
                    frame.human_count()
                );
            }
            SessionKind::Client { server } => {
                self.pending_pulses |= local_input.flags & F_HATCH_TOGGLE;
                if !self.connected && now.duration_since(self.last_hello) >= HELLO_PERIOD {
                    let _ = self.socket.send_to(
                        &encode_hello(self.requested_role, &self.name, self.mission, self.seed),
                        *server,
                    );
                    self.last_hello = now;
                }
                if self.connected && now.duration_since(self.last_input) >= INPUT_PERIOD {
                    self.seq = self.seq.wrapping_add(1);
                    let mut outbound = local_input.sanitized();
                    outbound.flags |= self.pending_pulses;
                    if self
                        .socket
                        .send_to(
                            &encode_input(self.local_id, self.seq, self.local_role, outbound),
                            *server,
                        )
                        .is_ok()
                    {
                        self.pending_pulses = 0;
                    }
                    self.last_input = now;
                }
                if self.connected && now.duration_since(self.last_rx) > PEER_TIMEOUT {
                    self.connected = false;
                    self.local_id = NONE_PEER;
                    self.status = "HOST LOST — RECONNECTING".into();
                }
            }
        }
        LanPoll {
            crew: self.latest_crew,
            snapshot: self.pending_snapshot.take(),
            connected: self.connected,
            status: self.status.clone(),
        }
    }

    pub fn publish(&mut self, mut snapshot: WorldSnapshot) {
        let SessionKind::Host { peers, .. } = &self.kind else {
            return;
        };
        let now = Instant::now();
        if now.duration_since(self.last_world) < WORLD_PERIOD {
            return;
        }
        self.world_seq = self.world_seq.wrapping_add(1);
        snapshot.seq = self.world_seq;
        let packet = encode_world(&snapshot, self.latest_crew, self.mission, self.seed);
        for peer in peers {
            let _ = self.socket.send_to(&packet, peer.addr);
        }
        self.last_world = now;
    }

    fn recv_all(&mut self) {
        let mut buf = [0u8; MAX_PACKET];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) => self.handle_packet(&buf[..n], addr),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    self.status = format!("LAN RECEIVE ERROR: {e}");
                    break;
                }
            }
        }
    }

    fn handle_packet(&mut self, data: &[u8], addr: SocketAddr) {
        let Ok(mut rd) = Reader::packet(data) else {
            return;
        };
        let Some(kind) = rd.u8() else { return };
        match kind {
            P_HELLO => self.handle_hello(rd, addr),
            P_WELCOME => self.handle_welcome(rd, addr),
            P_INPUT => self.handle_input(rd, addr),
            P_WORLD => self.handle_world(rd, addr),
            P_CLAIM => self.handle_claim(rd, addr),
            P_FULL => {
                if matches!(self.kind, SessionKind::Client { server } if server == addr) {
                    self.status = "TANK FULL — FIVE CREW ALREADY ABOARD".into();
                }
            }
            _ => {}
        }
    }

    fn handle_hello(&mut self, mut rd: Reader<'_>, addr: SocketAddr) {
        let SessionKind::Host { peers, next_id } = &mut self.kind else {
            return;
        };
        let Some(requested) = rd.u8().and_then(LanRole::from_u8) else {
            return;
        };
        let Some(mission) = rd.u8().and_then(LanMission::from_u8) else {
            return;
        };
        let Some(seed) = rd.u64() else { return };
        let Some(name) = rd.string() else { return };
        if let Some(peer) = peers.iter_mut().find(|peer| peer.addr == addr) {
            peer.last_seen = Instant::now();
            let packet = encode_welcome(peer.id, peer.role, self.mission, self.seed);
            let _ = self.socket.send_to(&packet, addr);
            return;
        }
        if peers.len() >= 4 {
            let _ = self.socket.send_to(&packet_header(P_FULL), addr);
            return;
        }
        let id = *next_id;
        *next_id = next_id.wrapping_add(1).max(1);
        let role = first_available(requested, id, self.local_role, peers);
        println!("[lan] {name} joined as {} from {addr}", role.label());
        if mission != self.mission || seed != self.seed {
            println!(
                "[lan] overriding client scene with host {} seed {}",
                self.mission.label(),
                self.seed
            );
        }
        peers.push(Peer {
            id,
            addr,
            role,
            name,
            input: RoleInput::default(),
            last_seq: 0,
            last_seen: Instant::now(),
        });
        let _ = self
            .socket
            .send_to(&encode_welcome(id, role, self.mission, self.seed), addr);
    }

    fn handle_welcome(&mut self, mut rd: Reader<'_>, addr: SocketAddr) {
        let SessionKind::Client { server } = self.kind else {
            return;
        };
        if addr != server {
            return;
        }
        let (Some(id), Some(role), Some(mission), Some(seed)) = (
            rd.u32(),
            rd.u8().and_then(LanRole::from_u8),
            rd.u8().and_then(LanMission::from_u8),
            rd.u64(),
        ) else {
            return;
        };
        self.local_id = id;
        self.local_role = role;
        self.requested_role = role;
        self.mission = mission;
        self.seed = seed;
        self.connected = true;
        self.last_rx = Instant::now();
        self.status = format!("CONNECTED · {} · {}", role.label(), mission.label());
        println!(
            "[lan] joined tank #{id} as {} — {} seed {seed}",
            role.label(),
            mission.label()
        );
    }

    fn handle_input(&mut self, mut rd: Reader<'_>, addr: SocketAddr) {
        let SessionKind::Host { peers, .. } = &mut self.kind else {
            return;
        };
        let (Some(id), Some(seq), Some(role), Some(input)) = (
            rd.u32(),
            rd.u32(),
            rd.u8().and_then(LanRole::from_u8),
            decode_role_input(&mut rd),
        ) else {
            return;
        };
        let Some(peer) = peers
            .iter_mut()
            .find(|peer| peer.id == id && peer.addr == addr)
        else {
            return;
        };
        if role != peer.role || !seq_newer(seq, peer.last_seq) {
            return;
        }
        peer.last_seq = seq;
        peer.input = input.sanitized();
        peer.last_seen = Instant::now();
    }

    fn handle_world(&mut self, mut rd: Reader<'_>, addr: SocketAddr) {
        let SessionKind::Client { server } = self.kind else {
            return;
        };
        if addr != server {
            return;
        }
        let Some((snapshot, crew, mission, seed)) = decode_world_body(&mut rd) else {
            return;
        };
        if self
            .pending_snapshot
            .as_ref()
            .is_some_and(|old| !seq_newer(snapshot.seq, old.seq))
        {
            return;
        }
        self.latest_crew = crew;
        self.pending_snapshot = Some(snapshot);
        self.mission = mission;
        self.seed = seed;
        self.last_rx = Instant::now();
        self.connected = true;
        self.status = format!(
            "{} · {} · {}/5 CREW",
            self.local_role.label(),
            mission.label(),
            crew.human_count()
        );
    }

    fn handle_claim(&mut self, mut rd: Reader<'_>, addr: SocketAddr) {
        let SessionKind::Host { peers, .. } = &mut self.kind else {
            return;
        };
        let (Some(id), Some(wanted)) = (rd.u32(), rd.u8().and_then(LanRole::from_u8)) else {
            return;
        };
        let Some(index) = peers
            .iter()
            .position(|peer| peer.id == id && peer.addr == addr)
        else {
            return;
        };
        let current = peers[index].role;
        let role = first_available(wanted, id, self.local_role, peers);
        if role != current || wanted == current {
            peers[index].role = role;
            peers[index].input = RoleInput::default();
            peers[index].last_seen = Instant::now();
            println!("[lan] {} moved to {}", peers[index].name, role.label());
        }
        let _ = self
            .socket
            .send_to(&encode_welcome(id, role, self.mission, self.seed), addr);
    }
}

fn resolve_one(value: &str) -> io::Result<SocketAddr> {
    value
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "address resolved to nothing"))
}

fn first_available(requested: LanRole, id: u32, host_role: LanRole, peers: &[Peer]) -> LanRole {
    let free = |role: LanRole| {
        !(id != 0 && host_role == role)
            && !peers.iter().any(|peer| peer.id != id && peer.role == role)
    };
    let mut role = requested;
    for _ in 0..LanRole::ALL.len() {
        if free(role) {
            return role;
        }
        role = role.next();
    }
    requested
}

fn seq_newer(new: u32, old: u32) -> bool {
    new != old && new.wrapping_sub(old) < (u32::MAX / 2)
}

fn packet_header(kind: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(kind);
    out
}

fn encode_hello(role: LanRole, name: &str, mission: LanMission, seed: u64) -> Vec<u8> {
    let mut w = Writer::packet(P_HELLO);
    w.u8(role as u8);
    w.u8(mission as u8);
    w.u64(seed);
    w.string(name);
    w.0
}

fn encode_welcome(id: u32, role: LanRole, mission: LanMission, seed: u64) -> Vec<u8> {
    let mut w = Writer::packet(P_WELCOME);
    w.u32(id);
    w.u8(role as u8);
    w.u8(mission as u8);
    w.u64(seed);
    w.0
}

fn encode_claim(id: u32, role: LanRole) -> Vec<u8> {
    let mut w = Writer::packet(P_CLAIM);
    w.u32(id);
    w.u8(role as u8);
    w.0
}

fn encode_input(id: u32, seq: u32, role: LanRole, input: RoleInput) -> Vec<u8> {
    let mut w = Writer::packet(P_INPUT);
    w.u32(id);
    w.u32(seq);
    w.u8(role as u8);
    encode_role_input(&mut w, input);
    w.0
}

fn encode_role_input(w: &mut Writer, input: RoleInput) {
    for value in [
        input.drive,
        input.steer,
        input.look_dx,
        input.look_dy,
        input.traverse,
        input.elevate,
    ] {
        w.f32(value);
    }
    w.u16(input.flags);
    w.u8(input.post as u8);
}

fn decode_role_input(rd: &mut Reader<'_>) -> Option<RoleInput> {
    Some(RoleInput {
        drive: rd.f32()?,
        steer: rd.f32()?,
        look_dx: rd.f32()?,
        look_dy: rd.f32()?,
        traverse: rd.f32()?,
        elevate: rd.f32()?,
        flags: rd.u16()?,
        post: LanRole::from_u8(rd.u8()?)?,
    })
}

fn encode_tank(w: &mut Writer, tank: NetTankState) {
    for value in tank.pos {
        w.f32(value);
    }
    for value in [
        tank.heading,
        tank.speed,
        tank.turret_yaw,
        tank.gun_pitch,
        tank.reload,
        tank.hp,
    ] {
        w.f32(value);
    }
    w.u8(tank.alive as u8);
}

fn decode_tank(rd: &mut Reader<'_>) -> Option<NetTankState> {
    Some(NetTankState {
        pos: [rd.f32()?, rd.f32()?, rd.f32()?],
        heading: rd.f32()?,
        speed: rd.f32()?,
        turret_yaw: rd.f32()?,
        gun_pitch: rd.f32()?,
        reload: rd.f32()?,
        hp: rd.f32()?,
        alive: rd.u8()? != 0,
    })
}

fn encode_world(
    snapshot: &WorldSnapshot,
    crew: CrewFrame,
    mission: LanMission,
    seed: u64,
) -> Vec<u8> {
    let mut w = Writer::packet(P_WORLD);
    w.u32(snapshot.seq);
    w.u8(mission as u8);
    w.u64(seed);
    for peer in crew.roster {
        w.u32(peer);
    }
    for input in crew.inputs {
        encode_role_input(&mut w, input);
    }
    encode_tank(&mut w, snapshot.player);
    w.u8(snapshot.act);
    w.u8(snapshot.enemies.len().min(24) as u8);
    for tank in snapshot.enemies.iter().take(24) {
        encode_tank(&mut w, *tank);
    }
    w.u8(snapshot.engine_on as u8);
    w.u8(snapshot.hatch_open as u8);
    w.u8(snapshot.breech);
    w.u32(snapshot.rounds_spent);
    w.u8(snapshot.he_loaded as u8);
    w.f32(snapshot.fire);
    w.u16(snapshot.module_mask);
    w.u8(snapshot.crew_mask);
    w.u8(snapshot.act_complete as u8);
    debug_assert!(w.0.len() <= MAX_PACKET);
    w.0
}

fn decode_world_body(rd: &mut Reader<'_>) -> Option<(WorldSnapshot, CrewFrame, LanMission, u64)> {
    let seq = rd.u32()?;
    let mission = LanMission::from_u8(rd.u8()?)?;
    let seed = rd.u64()?;
    let mut crew = CrewFrame::default();
    for peer in &mut crew.roster {
        *peer = rd.u32()?;
    }
    for input in &mut crew.inputs {
        *input = decode_role_input(rd)?.sanitized();
    }
    let player = decode_tank(rd)?;
    let act = rd.u8()?;
    let count = rd.u8()? as usize;
    if count > 24 {
        return None;
    }
    let mut enemies = Vec::with_capacity(count);
    for _ in 0..count {
        enemies.push(decode_tank(rd)?);
    }
    let snapshot = WorldSnapshot {
        seq,
        act,
        player,
        enemies,
        engine_on: rd.u8()? != 0,
        hatch_open: rd.u8()? != 0,
        breech: rd.u8()?,
        rounds_spent: rd.u32()?,
        he_loaded: rd.u8()? != 0,
        fire: finite_unit(rd.f32()?).max(0.0),
        module_mask: rd.u16()?,
        crew_mask: rd.u8()?,
        act_complete: rd.u8()? != 0,
    };
    Some((snapshot, crew, mission, seed))
}

struct Writer(Vec<u8>);

impl Writer {
    fn packet(kind: u8) -> Self {
        Self(packet_header(kind))
    }
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn u16(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn f32(&mut self, value: f32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn string(&mut self, value: &str) {
        let bytes = value.as_bytes();
        self.u8(bytes.len().min(255) as u8);
        self.0.extend_from_slice(&bytes[..bytes.len().min(255)]);
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn packet(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < 6 || &bytes[..4] != MAGIC || bytes[4] != VERSION {
            return Err(());
        }
        Ok(Self { bytes, at: 5 })
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let out = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(out)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(*self.take(1)?.first()?)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

pub fn run_loopback_verify() -> anyhow::Result<()> {
    let host_cfg = LanConfig {
        mode: LanModeConfig::Host {
            bind: "127.0.0.1:0".into(),
        },
        requested_role: LanRole::Commander,
        name: "host".into(),
        mission: LanMission::Arena,
        seed: 1942,
    };
    let mut host = LanSession::new(&host_cfg)?;
    let server = host.local_addr()?;
    let client_cfg = LanConfig {
        mode: LanModeConfig::Join {
            server: server.to_string(),
        },
        requested_role: LanRole::Gunner,
        name: "gunner".into(),
        mission: LanMission::Garage,
        seed: 0,
    };
    let mut client = LanSession::new(&client_cfg)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut saw_input = false;
    let mut saw_snapshot = false;
    while Instant::now() < deadline && !(saw_input && saw_snapshot) {
        let cp = client.poll(RoleInput {
            flags: F_FIRE_MAIN,
            look_dx: 7.0,
            post: LanRole::Gunner,
            ..RoleInput::default()
        });
        let hp = host.poll(RoleInput {
            drive: 1.0,
            post: LanRole::Commander,
            ..RoleInput::default()
        });
        saw_input |=
            hp.crew.occupied(LanRole::Gunner) && hp.crew.input(LanRole::Gunner).flag(F_FIRE_MAIN);
        host.publish(WorldSnapshot {
            player: NetTankState {
                pos: [1.0, 2.0, 3.0],
                alive: true,
                ..NetTankState::default()
            },
            ..WorldSnapshot::default()
        });
        saw_snapshot |= cp.snapshot.is_some_and(|s| s.player.pos == [1.0, 2.0, 3.0]);
        std::thread::sleep(Duration::from_millis(2));
    }
    anyhow::ensure!(saw_input, "host never received gunner input");
    anyhow::ensure!(saw_snapshot, "client never received authoritative snapshot");
    anyhow::ensure!(
        client.local_role() == LanRole::Gunner,
        "gunner role was not assigned"
    );
    anyhow::ensure!(
        client.mission() == LanMission::Arena && client.seed() == 1942,
        "host mission authority did not replicate"
    );
    println!("[lan-verify] PASS — handshake, role assignment, input, fallback and snapshot");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_players_make_commander_driver_and_gunner_loader() {
        let mut frame = CrewFrame::default();
        frame.roster[LanRole::Commander as usize] = 0;
        frame.roster[LanRole::Gunner as usize] = 1;
        frame.inputs[LanRole::Commander as usize].drive = 0.75;
        frame.inputs[LanRole::Gunner as usize] = RoleInput {
            flags: F_FIRE_MAIN | F_SERVICE_BREECH,
            post: LanRole::Loader,
            ..RoleInput::default()
        };
        let merged = fill_empty_roles(frame);
        assert_eq!(merged.input(LanRole::Driver).drive, 0.75);
        assert!(merged.input(LanRole::Gunner).flag(F_FIRE_MAIN));
        assert!(merged.input(LanRole::Loader).flag(F_SERVICE_BREECH));
        assert_eq!(merged.input(LanRole::Loader).post, LanRole::Loader);
        assert_eq!(merged.human_count(), 2);
    }

    #[test]
    fn three_players_require_a_real_driver() {
        let mut frame = CrewFrame::default();
        frame.roster[LanRole::Commander as usize] = 0;
        frame.roster[LanRole::Gunner as usize] = 1;
        frame.roster[LanRole::Radio as usize] = 2;
        frame.inputs[LanRole::Commander as usize].drive = 1.0;
        let merged = fill_empty_roles(frame);
        assert_eq!(merged.input(LanRole::Driver).drive, 0.0);

        frame.roster[LanRole::Radio as usize] = NONE_PEER;
        frame.roster[LanRole::Driver as usize] = 2;
        frame.inputs[LanRole::Driver as usize] = RoleInput {
            drive: 0.6,
            post: LanRole::Driver,
            ..RoleInput::default()
        };
        assert_eq!(fill_empty_roles(frame).input(LanRole::Driver).drive, 0.6);
    }

    #[test]
    fn a_joiner_can_never_take_the_hosts_seat() {
        let peers = Vec::new();
        assert_eq!(
            first_available(LanRole::Commander, 1, LanRole::Commander, &peers),
            LanRole::Driver
        );
        assert_eq!(
            first_available(LanRole::Gunner, 1, LanRole::Commander, &peers),
            LanRole::Gunner
        );
    }

    #[test]
    fn world_packet_round_trips_and_rejects_truncation() {
        let mut frame = CrewFrame::default();
        frame.roster[0] = 0;
        frame.inputs[2] = RoleInput {
            look_dx: -12.5,
            flags: F_FIRE_MAIN,
            ..RoleInput::default()
        };
        let snapshot = WorldSnapshot {
            seq: 9,
            player: NetTankState {
                pos: [4.0, 5.0, 6.0],
                hp: 88.0,
                alive: true,
                ..NetTankState::default()
            },
            enemies: vec![NetTankState {
                heading: 1.25,
                alive: true,
                ..NetTankState::default()
            }],
            engine_on: true,
            module_mask: 0x0fff,
            ..WorldSnapshot::default()
        };
        let packet = encode_world(&snapshot, frame, LanMission::Campaign, 42);
        let mut rd = Reader::packet(&packet).unwrap();
        assert_eq!(rd.u8(), Some(P_WORLD));
        let (out, crew, mission, seed) = decode_world_body(&mut rd).unwrap();
        assert_eq!(out, snapshot);
        assert_eq!(crew, frame);
        assert_eq!(mission, LanMission::Campaign);
        assert_eq!(seed, 42);
        let mut short = Reader::packet(&packet[..packet.len() - 3]).unwrap();
        assert_eq!(short.u8(), Some(P_WORLD));
        assert!(decode_world_body(&mut short).is_none());
    }

    #[test]
    fn hostile_float_payloads_are_sanitized() {
        let input = RoleInput {
            drive: f32::NAN,
            steer: 99.0,
            look_dx: f32::INFINITY,
            ..RoleInput::default()
        }
        .sanitized();
        assert_eq!(input.drive, 0.0);
        assert_eq!(input.steer, 1.0);
        assert_eq!(input.look_dx, 0.0);
    }
}
