//! The duel: two players, a Tiger each with its living crew, on the Canyon. Child module of
//! `game.rs` (like `platoon.rs`), so it can drive the one `AllyTiger` rig that stands for the
//! other player's tank and the authoritative projectile path.
//!
//! How it works (version 1, 10 November 2026):
//! * Each machine runs its own complete game: its own Tiger, crew, damage, fire and repairs.
//! * Thirty times a second each sends the other its tank's pose (hull, turret, gun), whether it is
//!   still in action, and its last few main-gun shots (shell, muzzle position, direction).
//! * The other player's Tiger is drawn from those poses. Each of his shots is fired here as a real
//!   shell from his muzzle along his direction, and what it does to MY tank is decided here, by
//!   the ordinary damage model. Likewise my shots are decided on his machine.
//! * So a hit is judged where the victim is, on the victim's machine. With delay between the two,
//!   a shot that looked on target on the shooter's screen can miss a moving tank: named, and the
//!   right way round (nobody is killed by a shot his own screen showed missing).
//!
//! Not in version 1, named here and in docs/DUEL_MODE_20261110.md: the crew's gunner does not
//! engage the other Tiger by himself (the player lays and fires); no machine-gun, smoke or wreck
//! state is sent; no reconnection; no lobby (the joiner types the host's address).

use super::{Game, ProjectileShooter};
use crate::tank::{self, Tank};
use glam::{Vec2, Vec3};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Instant;

/// `KESSEL_DUEL=host`, `host:PORT` or `join:ADDRESS[:PORT]` (the launcher's `--duel-host` and
/// `--duel-join ADDRESS`).
pub const ENV: &str = "KESSEL_DUEL";
pub const DEFAULT_PORT: u16 = 28944;
const MAGIC: [u8; 4] = *b"PZD1";
const SEND_INTERVAL_S: f32 = 1.0 / 30.0;
/// Shots repeated in every packet, so one lost packet does not lose a shot.
const FIRES_KEPT: usize = 6;
/// How far apart along the course the two Tigers start (`KESSEL_DUEL_SEPARATION_M` for the
/// two-process check, where the two must see each other at once).
const START_SEPARATION_M: f32 = 700.0;
fn start_separation_m() -> f32 {
    std::env::var("KESSEL_DUEL_SEPARATION_M").ok().and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= 30.0).unwrap_or(START_SEPARATION_M)
}
/// The other tank's drawn pose closes on the latest received one with this time constant.
const POSE_SMOOTH_S: f32 = 0.08;
/// Silence longer than this is reported as a lost connection.
const SILENCE_S: f32 = 5.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Host { port: u16 },
    Join { address: String },
}

/// What the environment asks for, if anything.
pub fn requested() -> Option<Request> {
    parse_request(&std::env::var(ENV).ok()?)
}

pub fn parse_request(raw: &str) -> Option<Request> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("host") { return Some(Request::Host { port: DEFAULT_PORT }); }
    if let Some(port) = raw.strip_prefix("host:") {
        return port.trim().parse().ok().map(|port| Request::Host { port });
    }
    let address = raw.strip_prefix("join:")?.trim();
    if address.is_empty() { return None; }
    // A bare host name or IPv4 address takes the default port.
    let with_port = if address.rsplit_once(':').is_some_and(|(_, port)| port.parse::<u16>().is_ok()) {
        address.to_string()
    } else {
        format!("{address}:{DEFAULT_PORT}")
    };
    Some(Request::Join { address: with_port })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TigerState {
    pub pos: Vec3,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
    pub turret_yaw: f32,
    pub gun_pitch: f32,
    pub speed: f32,
    /// Still fighting (its commander alive and the tank not destroyed).
    pub in_action: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireEvent {
    pub id: u32,
    pub he: bool,
    pub origin: Vec3,
    pub direction: Vec3,
    pub seed: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    pub sequence: u32,
    pub state: TigerState,
    pub fires: Vec<FireEvent>,
}

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48 + self.fires.len() * 33);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&self.sequence.to_le_bytes());
        let s = &self.state;
        for v in [s.pos.x, s.pos.y, s.pos.z, s.heading, s.pitch, s.roll, s.turret_yaw, s.gun_pitch, s.speed] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.push(u8::from(s.in_action));
        out.push(self.fires.len().min(FIRES_KEPT) as u8);
        for fire in self.fires.iter().take(FIRES_KEPT) {
            out.extend_from_slice(&fire.id.to_le_bytes());
            out.push(u8::from(fire.he));
            for v in [fire.origin.x, fire.origin.y, fire.origin.z, fire.direction.x, fire.direction.y, fire.direction.z] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&fire.seed.to_le_bytes());
        }
        out
    }

    /// `None` for anything that is not a whole, finite packet of this version.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut at = 0usize;
        let mut take = |n: usize| -> Option<&[u8]> { let s = bytes.get(at..at + n)?; at += n; Some(s) };
        if take(4)? != MAGIC { return None; }
        let sequence = u32::from_le_bytes(take(4)?.try_into().ok()?);
        let mut f = [0.0f32; 9];
        for v in &mut f { *v = f32::from_le_bytes(take(4)?.try_into().ok()?); }
        if f.iter().any(|v| !v.is_finite()) { return None; }
        let in_action = take(1)?[0] != 0;
        let count = take(1)?[0] as usize;
        if count > FIRES_KEPT { return None; }
        let mut fires = Vec::with_capacity(count);
        for _ in 0..count {
            let id = u32::from_le_bytes(take(4)?.try_into().ok()?);
            let he = take(1)?[0] != 0;
            let mut g = [0.0f32; 6];
            for v in &mut g { *v = f32::from_le_bytes(take(4)?.try_into().ok()?); }
            if g.iter().any(|v| !v.is_finite()) { return None; }
            let seed = u32::from_le_bytes(take(4)?.try_into().ok()?);
            let direction = Vec3::new(g[3], g[4], g[5]);
            // A shot's direction is a unit vector; anything else is not a shot.
            if (direction.length() - 1.0).abs() > 0.01 { return None; }
            fires.push(FireEvent { id, he, origin: Vec3::new(g[0], g[1], g[2]), direction, seed });
        }
        Some(Self { sequence, state: TigerState { pos: Vec3::new(f[0], f[1], f[2]), heading: f[3], pitch: f[4],
            roll: f[5], turret_yaw: f[6], gun_pitch: f[7], speed: f[8], in_action }, fires })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The other Tiger went out of action first.
    Won,
    /// Mine did.
    Lost,
}

pub struct Duel {
    socket: UdpSocket,
    /// The other machine: known from the start when joining, learnt from its first packet when hosting.
    peer: Option<SocketAddr>,
    pub host: bool,
    send_in_s: f32,
    sequence: u32,
    sent_fires: Vec<FireEvent>,
    next_fire_id: u32,
    /// Highest shot id of the other player's already fired here.
    seen_fire_id: u32,
    newest_sequence: u32,
    latest: Option<TigerState>,
    last_heard: Option<Instant>,
    pub outcome: Option<Outcome>,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub fires_received: u32,
}

impl Duel {
    pub fn open(request: &Request) -> anyhow::Result<Self> {
        let (socket, peer, host) = match request {
            Request::Host { port } => (UdpSocket::bind(("0.0.0.0", *port))
                .map_err(|e| anyhow::anyhow!("cannot listen on UDP port {port}: {e}"))?, None, true),
            Request::Join { address } => {
                let peer = address.to_socket_addrs()
                    .map_err(|e| anyhow::anyhow!("cannot resolve {address}: {e}"))?
                    .next().ok_or_else(|| anyhow::anyhow!("{address} resolves to no address"))?;
                (UdpSocket::bind(("0.0.0.0", 0))?, Some(peer), false)
            }
        };
        socket.set_nonblocking(true)?;
        Ok(Self { socket, peer, host, send_in_s: 0.0, sequence: 0, sent_fires: Vec::new(), next_fire_id: 1,
            seen_fire_id: 0, newest_sequence: 0, latest: None, last_heard: None, outcome: None,
            packets_sent: 0, packets_received: 0, fires_received: 0 })
    }

    pub fn local_port(&self) -> u16 { self.socket.local_addr().map(|a| a.port()).unwrap_or(0) }
    pub fn connected(&self) -> bool { self.last_heard.is_some_and(|t| t.elapsed().as_secs_f32() < SILENCE_S) }
    pub fn ever_connected(&self) -> bool { self.last_heard.is_some() }

    fn note_fire(&mut self, he: bool, origin: Vec3, direction: Vec3, seed: u32) {
        let id = self.next_fire_id;
        self.next_fire_id = self.next_fire_id.wrapping_add(1).max(1);
        self.sent_fires.push(FireEvent { id, he, origin, direction: direction.normalize_or_zero(), seed });
        if self.sent_fires.len() > FIRES_KEPT { self.sent_fires.remove(0); }
        // A shot goes out at once, not at the next thirtieth.
        self.send_in_s = 0.0;
    }

    /// Everything waiting on the socket: the newest pose, and the shots not yet fired here.
    fn receive(&mut self) -> Vec<FireEvent> {
        let mut new_fires = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            let (len, from) = match self.socket.recv_from(&mut buffer) {
                Ok(received) => received,
                Err(_) => break, // nothing waiting (or a transient error: the next update tries again)
            };
            let Some(packet) = Packet::decode(&buffer[..len]) else { continue };
            // The host takes the first valid sender as the other player; after that, only him.
            match self.peer {
                None => self.peer = Some(from),
                Some(peer) if peer != from => continue,
                _ => {}
            }
            self.packets_received += 1;
            self.last_heard = Some(Instant::now());
            for fire in &packet.fires {
                if fire.id > self.seen_fire_id {
                    self.seen_fire_id = fire.id;
                    new_fires.push(*fire);
                }
            }
            if packet.sequence >= self.newest_sequence {
                self.newest_sequence = packet.sequence;
                self.latest = Some(packet.state);
            }
        }
        new_fires
    }

    fn send(&mut self, state: TigerState) {
        let Some(peer) = self.peer else { return };
        self.sequence = self.sequence.wrapping_add(1);
        let packet = Packet { sequence: self.sequence, state, fires: self.sent_fires.clone() };
        if self.socket.send_to(&packet.encode(), peer).is_ok() { self.packets_sent += 1; }
    }
}

/// The two starts on a course: the authored start, and a point `START_SEPARATION_M` along the
/// route facing back down it. Without a route, 400 m ahead of the start along its heading.
pub fn starts(start: Vec2, heading: f32, route: &[Vec2]) -> [(Vec2, f32); 2] {
    starts_at(start, heading, route, start_separation_m())
}
pub fn starts_at(start: Vec2, heading: f32, route: &[Vec2], separation_m: f32) -> [(Vec2, f32); 2] {
    let ahead = Vec2::new(heading.sin(), heading.cos());
    let mut far = (start + ahead * 400.0, heading + std::f32::consts::PI);
    let mut walked = 0.0;
    // The route's nearest knot to the start, then forward along it.
    let from = route.iter().enumerate()
        .min_by(|a, b| a.1.distance_squared(start).total_cmp(&b.1.distance_squared(start))).map_or(0, |(i, _)| i);
    for pair in route[from.min(route.len())..].windows(2) {
        let leg = pair[1] - pair[0];
        let len = leg.length();
        if len < 1e-3 { continue; }
        if walked + len >= separation_m {
            let at = pair[0] + leg * ((separation_m - walked) / len);
            let back = -leg / len;
            far = (at, back.x.atan2(back.y));
            break;
        }
        walked += len;
        // A route shorter than the separation: its last knot, facing back.
        let back = -leg / len;
        far = (pair[1], back.x.atan2(back.y));
    }
    [(start, heading), far]
}

impl Game {
    /// Set the world up for a duel on a built map that carries one ally Tiger rig (the scene
    /// builds it when `requested()`): no T-34s, the two Tigers at the course's two starts, the
    /// living crew aboard.
    pub fn enable_duel(&mut self, request: &Request) -> anyhow::Result<()> {
        anyhow::ensure!(!self.allies.is_empty(), "this world was built without the other player's Tiger (set {ENV} before the map is built)");
        let net = Duel::open(request)?;
        self.playtest_setup();
        self.god = false;
        // No other enemy on the map: the practice T-34s are benched.
        for (i, enemy) in self.enemies.iter_mut().enumerate() {
            enemy.alive = false;
            enemy.pos = Vec3::new(-3000.0 - 20.0 * i as f32, 0.0, -3000.0);
            if let Some(active) = self.enemy_active.get_mut(i) { *active = false; }
        }
        let (start, heading, route) = match &self.canyon_spec {
            Some(spec) => (spec.start, spec.heading, spec.route.clone()),
            None => (Vec2::new(self.spawn_xz.0, self.spawn_xz.1), self.player.heading, Vec::new()),
        };
        let places = starts(start, heading, &route);
        let (mine, his) = if net.host { (places[0], places[1]) } else { (places[1], places[0]) };
        self.player_powertrain = crate::powertrain::Powertrain::default();
        self.player = Tank::new(mine.0.x, mine.0.y, mine.1, &self.hf, tank::TIGER_PARTS);
        self.spawn_xz = (mine.0.x, mine.0.y);
        self.allies.truncate(1);
        let other = &mut self.allies[0];
        other.tank = Tank::new(his.0.x, his.0.y, his.1, &self.hf, tank::TIGER_PARTS);
        other.tank.alive = true;
        other.tank.hp = tank::MAX_HP;
        self.set_playtest_return_fire(true);
        self.enable_singleplayer_crew();
        match (&request, net.host) {
            (Request::Host { port }, _) => println!("[duel] hosting on UDP port {port}: the other player joins this machine's address; start ({:.0}, {:.0})", mine.0.x, mine.0.y),
            (Request::Join { address }, _) => println!("[duel] joining {address}; start ({:.0}, {:.0})", mine.0.x, mine.0.y),
        }
        self.duel = Some(net);
        Ok(())
    }

    fn duel_own_state(&self) -> TigerState {
        let p = &self.player;
        TigerState { pos: p.pos, heading: p.heading, pitch: p.pitch, roll: p.roll, turret_yaw: p.turret_yaw,
            gun_pitch: p.gun_pitch, speed: p.speed, in_action: p.alive && self.game_over().is_none() }
    }

    /// The player's main gun fired: the other machine fires the same shell.
    pub(super) fn duel_note_fire(&mut self, he: bool, origin: Vec3, direction: Vec3, seed: u32) {
        if let Some(duel) = self.duel.as_mut() { duel.note_fire(he, origin, direction, seed); }
    }

    /// One update of the duel: take what arrived, pose the other Tiger, fire his shots, send mine.
    pub(super) fn update_duel(&mut self, dt: f32) {
        let Some(mut duel) = self.duel.take() else { return };
        let fires = duel.receive();
        if let (Some(latest), Some(other)) = (duel.latest, self.allies.first_mut()) {
            let tank = &mut other.tank;
            // Close on the received pose: a jump if it is far off (the first packet, a stall).
            let k = if tank.pos.distance(latest.pos) > 25.0 { 1.0 } else { 1.0 - (-dt.max(0.0) / POSE_SMOOTH_S).exp() };
            let angle = |from: f32, to: f32| {
                let d = (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                from + d * k
            };
            tank.pos = tank.pos.lerp(latest.pos, k);
            tank.heading = angle(tank.heading, latest.heading);
            tank.pitch = angle(tank.pitch, latest.pitch);
            tank.roll = angle(tank.roll, latest.roll);
            tank.turret_yaw = angle(tank.turret_yaw, latest.turret_yaw);
            tank.gun_pitch = angle(tank.gun_pitch, latest.gun_pitch);
            tank.speed = latest.speed;
            if tank.alive && !latest.in_action {
                tank.alive = false;
                tank.hp = 0.0;
                tank.speed = 0.0;
                let at = tank.center();
                self.spawn_blast(at, 7.0, 24.0, 0.75);
                if duel.outcome.is_none() {
                    duel.outcome = Some(Outcome::Won);
                    println!("[duel] the other Tiger is out of action: won");
                }
            }
        }
        for fire in fires {
            duel.fires_received += 1;
            let shell: &'static crate::ballistics::ShellSpec = if fire.he { &crate::ballistics::SPRGR_88 } else { &crate::ballistics::PZGR_39 };
            let id = self.spawn_authoritative_projectile(ProjectileShooter::Ally(0), shell, fire.origin, fire.direction, fire.seed);
            self.present_external_projectile(id, fire.origin, fire.direction);
            self.spawn_blast(fire.origin, 2.2, 18.0, 0.16);
            if let Some(other) = self.allies.first_mut() { other.tank.present_fired(); }
        }
        let own = self.duel_own_state();
        if !own.in_action && duel.outcome.is_none() {
            duel.outcome = Some(Outcome::Lost);
            println!("[duel] this Tiger is out of action: lost");
        }
        duel.send_in_s -= dt.max(0.0);
        if duel.send_in_s <= 0.0 {
            duel.send_in_s = SEND_INTERVAL_S;
            duel.send(own);
        }
        self.duel = Some(duel);
    }

    /// Fixture for `duel-run`: lay the player's gun on the other Tiger, whoever sits at it.
    /// True once laid.
    pub fn duel_fixture_lay_on_other(&mut self, dt: f32) -> bool {
        let Some(target) = self.allies.first().filter(|a| a.tank.alive).map(|a| a.tank.center()) else { return false };
        let aim = super::ballistic_aim_target(self.player.muzzle().0, target, &crate::ballistics::PZGR_39);
        self.player.ai_aim(aim, dt)
    }

    /// One line for the screen: waiting, lost connection, or the result.
    pub fn duel_status_line(&self) -> Option<String> {
        let duel = self.duel.as_ref()?;
        Some(match duel.outcome {
            Some(Outcome::Won) => "DUEL WON - THE OTHER TIGER IS OUT OF ACTION".into(),
            Some(Outcome::Lost) => "DUEL LOST".into(),
            None if !duel.ever_connected() && duel.host => format!("DUEL - WAITING FOR THE OTHER PLAYER ON PORT {}", duel.local_port()),
            None if !duel.ever_connected() => "DUEL - CALLING THE HOST".into(),
            None if !duel.connected() => "DUEL - NO WORD FROM THE OTHER PLAYER".into(),
            None => return None,
        })
    }

    /// Review evidence for the two-process check (`duel-run`).
    pub fn duel_receipt(&self) -> serde_json::Value {
        let Some(duel) = self.duel.as_ref() else { return serde_json::Value::Null };
        let other = self.allies.first().map(|a| &a.tank);
        serde_json::json!({
            "host": duel.host, "connected": duel.connected(), "packets_sent": duel.packets_sent,
            "packets_received": duel.packets_received, "shots_sent": duel.next_fire_id - 1,
            "shots_received": duel.fires_received,
            "outcome": duel.outcome.map(|o| if o == Outcome::Won { "won" } else { "lost" }),
            "own": {"pos": self.player.pos.to_array(), "in_action": self.duel_own_state().in_action},
            "other": other.map(|t| serde_json::json!({"pos": t.pos.to_array(), "heading": t.heading,
                "turret_yaw": t.turret_yaw, "alive": t.alive})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> TigerState {
        TigerState { pos: Vec3::new(-640.5, -12.25, 301.0), heading: 1.25, pitch: -0.04, roll: 0.02,
            turret_yaw: -2.5, gun_pitch: 0.08, speed: 6.5, in_action: true }
    }

    #[test]
    fn a_packet_survives_the_wire_and_damage_to_it_is_refused() {
        let fires = vec![
            FireEvent { id: 7, he: false, origin: Vec3::new(1.0, 2.0, 3.0), direction: Vec3::new(0.0, 0.6, 0.8), seed: 0xDEAD_BEEF },
            FireEvent { id: 8, he: true, origin: Vec3::new(-4.0, 2.5, 9.0), direction: Vec3::X, seed: 3 },
        ];
        let packet = Packet { sequence: 41, state: state(), fires };
        let bytes = packet.encode();
        assert_eq!(Packet::decode(&bytes), Some(packet.clone()));
        // Truncated, wrong magic, a non-finite pose, a direction that is not a unit vector.
        assert_eq!(Packet::decode(&bytes[..bytes.len() - 1]), None);
        let mut wrong = bytes.clone(); wrong[0] = b'X';
        assert_eq!(Packet::decode(&wrong), None);
        let mut nan = bytes.clone(); nan[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(Packet::decode(&nan), None);
        let bad = Packet { fires: vec![FireEvent { direction: Vec3::new(0.0, 3.0, 0.0), ..packet.fires[0] }], ..packet };
        assert_eq!(Packet::decode(&bad.encode()), None);
    }

    #[test]
    fn requests_parse_and_a_bare_address_takes_the_default_port() {
        assert_eq!(parse_request("host"), Some(Request::Host { port: DEFAULT_PORT }));
        assert_eq!(parse_request("host:4000"), Some(Request::Host { port: 4000 }));
        assert_eq!(parse_request("join:192.168.1.20"), Some(Request::Join { address: format!("192.168.1.20:{DEFAULT_PORT}") }));
        assert_eq!(parse_request("join:example.org:5000"), Some(Request::Join { address: "example.org:5000".into() }));
        assert_eq!(parse_request("join:"), None);
        assert_eq!(parse_request("solo"), None);
    }

    #[test]
    fn the_second_start_is_the_separation_along_the_route_facing_back() {
        let route = [Vec2::new(0.0, 0.0), Vec2::new(0.0, 500.0), Vec2::new(400.0, 500.0), Vec2::new(400.0, 2000.0)];
        let [(a, heading_a), (b, heading_b)] = starts_at(Vec2::new(5.0, 0.0), 0.0, &route, START_SEPARATION_M);
        assert_eq!((a, heading_a), (Vec2::new(5.0, 0.0), 0.0));
        // 500 m up, then 200 m east: the far start faces west, back along the leg.
        assert!(b.distance(Vec2::new(200.0, 500.0)) < 1e-3, "{b:?}");
        let back = Vec2::new(heading_b.sin(), heading_b.cos());
        assert!(back.distance(Vec2::new(-1.0, 0.0)) < 1e-4, "{back:?}");
        // No route: 400 m ahead, facing back.
        let [_, (c, heading_c)] = starts_at(Vec2::ZERO, 0.0, &[], START_SEPARATION_M);
        assert!(c.distance(Vec2::new(0.0, 400.0)) < 1e-3 && (heading_c - std::f32::consts::PI).abs() < 1e-6);
    }

    #[test]
    fn two_ends_exchange_poses_and_each_shot_arrives_once() {
        let mut host = Duel::open(&Request::Host { port: 0 }).unwrap();
        let port = host.local_port();
        let mut guest = Duel::open(&Request::Join { address: format!("127.0.0.1:{port}") }).unwrap();
        guest.note_fire(false, Vec3::new(1.0, 2.0, 3.0), Vec3::Z, 11);
        let mut got_fires = 0;
        for round in 0..40 {
            guest.send(TigerState { speed: round as f32, ..state() });
            host.send(state());
            std::thread::sleep(std::time::Duration::from_millis(2));
            got_fires += host.receive().len();
            guest.receive();
        }
        assert!(host.connected() && guest.connected());
        assert_eq!(got_fires, 1, "the shot is repeated in every packet and fired once");
        assert_eq!(host.latest.map(|s| s.speed), Some(39.0));
        assert_eq!(guest.latest, Some(state()));
    }
}
