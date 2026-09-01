//! Hot-plug gamepad input for the embodied controls.
//!
//! `gilrs` gives Xbox and PlayStation pads one semantic layout (South/East/etc.).  This module
//! deliberately exports a small, game-shaped frame rather than leaking backend events through the
//! app.  A controller can arrive after boot; the most recently used connected pad becomes active.
//!
//! COUCH CO-OP: `poll_all` additionally reports EVERY connected pad as its own `PadFrame` in a
//! stable seat order (gilrs id order), each with independent edge state — the splitscreen crew
//! mode deals pads to stations (first pad = gunner, second = driver, third = loader) while `active`
//! keeps the solo/menu behavior unchanged. Each frame also carries a stable peer id derived from
//! gilrs' device id, so removing one pad cannot silently transfer the remaining pads between jobs.

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};

/// Local controller ids live above ordinary small peer/player ids. `u32::MAX` remains available as
/// the LAN roster's NONE sentinel; the practical gilrs id space is tiny compared with this range.
const PAD_PEER_ID_BASE: u32 = 0x8000_0000;

fn peer_id_from_raw_gamepad_id(raw: usize) -> u32 {
    let raw = u32::try_from(raw).expect("gilrs GamepadId does not fit in u32");
    PAD_PEER_ID_BASE
        .checked_add(raw)
        .filter(|&id| id != u32::MAX)
        .expect("gilrs GamepadId exhausted the local controller peer-id range")
}

fn peer_id(id: GamepadId) -> u32 {
    peer_id_from_raw_gamepad_id(usize::from(id))
}

#[derive(Clone, Copy, Default)]
struct Buttons {
    south: bool,
    east: bool,
    north: bool,
    west: bool,
    rb: bool,
    lt: bool,
    rt: bool,
    select: bool,
    start: bool,
    left_thumb: bool,
    right_thumb: bool,
    dpad_up: bool,
    dpad_down: bool,
    dpad_left: bool,
    dpad_right: bool,
}

fn edge_baseline(now: Buttons, previous: Buttons, armed: bool) -> Buttons {
    if armed {
        previous
    } else {
        // Prime from reality: held-at-discovery is a level, not a new press.
        now
    }
}

#[derive(Clone, Copy, Default)]
pub struct PadFrame {
    /// Stable identity for this physical pad during the process lifetime. Unlike the position in
    /// `PadsPoll::seats`, this does not change when a lower-id controller disconnects. It is already
    /// namespaced for direct use as a local `CrewFrame::roster` peer id.
    pub peer_id: u32,
    pub connected: bool,
    pub drive: f32,
    pub steer: f32,
    pub look_x: f32,
    pub look_y: f32,
    pub primary: bool,
    pub aim: bool,
    pub jump: bool,
    pub reload: bool,
    pub coax: bool,
    pub crouch: bool,
    pub ignition: bool,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub dpad_up_pressed: bool,
    pub dpad_down_pressed: bool,
    pub dpad_left_pressed: bool,
    pub dpad_right_pressed: bool,
    pub interact_pressed: bool,
    pub back_pressed: bool,
    pub dismount_held: bool,
    pub zoom_pressed: bool,
    pub map_pressed: bool,
    pub menu_pressed: bool,
}

/// One frame of everything: the legacy "whichever pad the player touched last" merged view for
/// menus/solo, plus every connected pad individually for the couch crew seats.
#[derive(Default)]
pub struct PadsPoll {
    pub active: PadFrame,
    /// All connected pads in seat order (gilrs id order — stable across frames while plugged in).
    pub seats: Vec<PadFrame>,
}

/// Per-pad edge memory (previous button levels + the discovery baseline latch).
#[derive(Clone, Copy, Default)]
struct PadEdges {
    previous: Buttons,
    armed: bool,
}

pub struct Gamepads {
    gilrs: Option<Gilrs>,
    active: Option<GamepadId>,
    edges: Vec<(GamepadId, PadEdges)>,
}

impl Default for Gamepads {
    fn default() -> Self {
        match Gilrs::new() {
            Ok(gilrs) => Self {
                gilrs: Some(gilrs),
                active: None,
                edges: Vec::new(),
            },
            Err(err) => {
                eprintln!("[controller] unavailable: {err}");
                Self {
                    gilrs: None,
                    active: None,
                    edges: Vec::new(),
                }
            }
        }
    }
}

impl Gamepads {
    /// Solo/menu view: the most recently used pad only (exact pre-couch behavior).
    #[allow(dead_code)]
    pub fn poll(&mut self) -> PadFrame {
        self.poll_all().active
    }

    pub fn poll_all(&mut self) -> PadsPoll {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return PadsPoll::default();
        };

        while let Some(event) = gilrs.next_event() {
            match event.event {
                EventType::Connected => {
                    let pad = gilrs.gamepad(event.id);
                    println!(
                        "[controller] connected: {} ({}) — Xbox/PlayStation layout active",
                        pad.name(),
                        event.id
                    );
                    self.active = Some(event.id);
                    reset_edges(&mut self.edges, event.id);
                }
                EventType::Disconnected => {
                    println!("[controller] disconnected: {}", event.id);
                    if self.active == Some(event.id) {
                        self.active = None;
                    }
                    self.edges.retain(|(id, _)| *id != event.id);
                }
                EventType::ButtonPressed(..)
                | EventType::ButtonChanged(..)
                | EventType::AxisChanged(..) => {
                    // Whichever pad the player touches is the pad that owns the MENU/solo frame.
                    // Couch seats are positional and unaffected by who touched last.
                    if self.active != Some(event.id) {
                        self.active = Some(event.id);
                        println!(
                            "[controller] active pad: {}",
                            gilrs.gamepad(event.id).name()
                        );
                    }
                }
                _ => {}
            }
        }

        // Seat order: every connected pad, sorted by gilrs id — stable while plugged in.
        let mut ids: Vec<GamepadId> = gilrs
            .gamepads()
            .filter(|(_, pad)| pad.is_connected())
            .map(|(id, _)| id)
            .collect();
        ids.sort_by_key(|id| usize::from(*id));

        if self.active.map_or(true, |id| !ids.contains(&id)) {
            self.active = ids.first().copied();
            if let Some(id) = self.active {
                println!("[controller] found at boot: {}", gilrs.gamepad(id).name());
                reset_edges(&mut self.edges, id);
            }
        }

        let mut out = PadsPoll::default();
        for id in ids {
            let slot = match self.edges.iter().position(|(eid, _)| *eid == id) {
                Some(i) => i,
                None => {
                    self.edges.push((id, PadEdges::default()));
                    self.edges.len() - 1
                }
            };
            let edges = &mut self.edges[slot].1;
            let frame = read_frame(gilrs.gamepad(id), edges);
            if self.active == Some(id) {
                out.active = frame;
            }
            out.seats.push(frame);
        }
        out
    }
}

fn reset_edges(edges: &mut Vec<(GamepadId, PadEdges)>, id: GamepadId) {
    edges.retain(|(eid, _)| *eid != id);
}

fn read_frame(pad: gilrs::Gamepad<'_>, edges: &mut PadEdges) -> PadFrame {
    let down = |button: Button| {
        pad.is_pressed(button)
            || pad
                .button_data(button)
                .is_some_and(|value| value.value() > 0.24)
    };
    let now = Buttons {
        south: down(Button::South),
        east: down(Button::East),
        north: down(Button::North),
        west: down(Button::West),
        rb: down(Button::RightTrigger),
        lt: down(Button::LeftTrigger2),
        rt: down(Button::RightTrigger2),
        select: down(Button::Select),
        start: down(Button::Start),
        left_thumb: down(Button::LeftThumb),
        right_thumb: down(Button::RightThumb),
        dpad_up: down(Button::DPadUp),
        dpad_down: down(Button::DPadDown),
        dpad_left: down(Button::DPadLeft),
        dpad_right: down(Button::DPadRight),
    };
    let previous = edge_baseline(now, edges.previous, edges.armed);
    let edge = |current: bool, previous: bool| current && !previous;
    let (lx, ly) = radial_deadzone(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
    let (rx, ry) = radial_deadzone(pad.value(Axis::RightStickX), pad.value(Axis::RightStickY));
    let frame = PadFrame {
        peer_id: peer_id(pad.id()),
        connected: true,
        drive: ly,
        // TankInput's positive steer is left; gamepads report positive X to the right.
        steer: -lx,
        look_x: rx,
        look_y: -ry,
        primary: now.rt,
        aim: now.lt,
        jump: now.south,
        reload: now.west,
        coax: now.rb,
        crouch: now.left_thumb,
        ignition: now.right_thumb,
        dpad_up: now.dpad_up,
        dpad_down: now.dpad_down,
        dpad_left: now.dpad_left,
        dpad_right: now.dpad_right,
        dpad_up_pressed: edge(now.dpad_up, previous.dpad_up),
        dpad_down_pressed: edge(now.dpad_down, previous.dpad_down),
        dpad_left_pressed: edge(now.dpad_left, previous.dpad_left),
        dpad_right_pressed: edge(now.dpad_right, previous.dpad_right),
        interact_pressed: edge(now.south, previous.south),
        back_pressed: edge(now.east, previous.east),
        dismount_held: now.north,
        zoom_pressed: edge(now.lt, previous.lt),
        map_pressed: edge(now.select, previous.select),
        menu_pressed: edge(now.start, previous.start),
    };
    edges.previous = now;
    edges.armed = true;
    frame
}

fn radial_deadzone(x: f32, y: f32) -> (f32, f32) {
    const DEAD: f32 = 0.18;
    let length = (x * x + y * y).sqrt();
    if length <= DEAD {
        return (0.0, 0.0);
    }
    let scaled = ((length - DEAD) / (1.0 - DEAD)).clamp(0.0, 1.0);
    let inv = scaled / length.max(1e-5);
    (x * inv, y * inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_deadzone_is_radial_and_rescaled() {
        assert_eq!(radial_deadzone(0.10, 0.10), (0.0, 0.0));
        let (x, y) = radial_deadzone(0.6, 0.0);
        assert!(x > 0.48 && x < 0.53 && y.abs() < 1e-6);
        let (x, y) = radial_deadzone(1.0, 1.0);
        assert!(x > 0.70 && y > 0.70);
    }

    #[test]
    fn an_unarmed_pad_baselines_held_buttons_instead_of_pressing_them() {
        let mut now = Buttons::default();
        now.south = true;
        let previous = edge_baseline(now, Buttons::default(), false);
        assert!(!(now.south && !previous.south));
        assert!(now.south); // level state remains available after the menu actually starts play
    }

    #[test]
    fn local_peer_ids_are_stable_distinct_and_roster_safe() {
        let first = peer_id_from_raw_gamepad_id(0);
        assert_eq!(first, peer_id_from_raw_gamepad_id(0));
        assert_ne!(first, peer_id_from_raw_gamepad_id(1));
        assert_eq!(first, PAD_PEER_ID_BASE);
        assert_ne!(first, u32::MAX);
        assert!(
            first > 4,
            "controller ids must not collide with small couch-player ids"
        );
    }
}
