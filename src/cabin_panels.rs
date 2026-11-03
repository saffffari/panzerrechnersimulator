//! The radio operator's sets and the driver's instrument cluster in the style of a 1960s Braun
//! audio set (owner's design, 3 November 2026): brushed silver fronts, black knobs, glass over
//! every window, small red, green and turquoise indicator lamps and amber panel lamps. The amber
//! lamps are the light of the hull's front corners; every lens is a finite emitter on the cabin
//! light list, switched by the cabin lamp switch, its light shadowed by the scene. A design
//! choice, not a reconstruction: the Tiger's FuG 5 sets and the driver's gauges looked nothing
//! like this.
//!
//! The source boxes stay as they are (`Hull_0355`, the radio rack; `Hull_0240`, the driver's
//! panel): collision, the damage model and the crew's reach keep using them. Each new front is a
//! closed slab standing on its box's face, 20-32 mm deep, with real bores for its windows and
//! dials; the source's own relief (knob stubs up to 31 mm proud) lies inside it. Drawn-only
//! geometry: a fragment crosses up to the slab's depth before it meets the source face (named in
//! docs/PHYSICAL_TRUTH_AUDIT.md).
//!
//! Moving parts: the radio's tuning needle (red, standing on a pivot below the scale, swinging
//! left and right) and its frequency knob, groundwork driven by `set_radio_tuning`; the driver's
//! tachometer and coolant needles; the three amber drums of his fuel counter, which roll as an
//! odometer's do.
use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
use crate::interior_rig::FiniteCubeLamp;
use crate::renderer::{PointLight, Renderer};
use glam::{Mat4, Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// A flat instrument face in the hull frame: its lower-left corner as its operator sees it, +u to
/// his right, +v up, +w out of the face toward him. Panel geometry is built in this frame.
#[derive(Clone, Copy, Debug)]
pub(super) struct Face {
    origin: Vec3,
    u: Vec3,
    v: Vec3,
    w: Vec3,
}

impl Face {
    fn model(&self) -> Mat4 {
        Mat4::from_cols(self.u.extend(0.0), self.v.extend(0.0), self.w.extend(0.0), self.origin.extend(1.0))
    }
}

/// The radio rack's faces look toward the radio operator (-x); his right is the hull's front (+z).
/// The two sets' frames (the source's rim, 11 mm proud of its panels) stand at x = -0.128; the
/// supply box below is flush with them. Measured from `Hull_0355` (orthographic survey).
const RADIO_U: Vec3 = Vec3::Z;
const RADIO_W: Vec3 = Vec3::NEG_X;
const RIM_X: f32 = -0.128;
pub(super) const RADIO_SETS: [Face; 2] = [
    Face { origin: Vec3::new(RIM_X, 1.605, 1.390), u: RADIO_U, v: Vec3::Y, w: RADIO_W },
    Face { origin: Vec3::new(RIM_X, 1.605, 1.773), u: RADIO_U, v: Vec3::Y, w: RADIO_W },
];
const RADIO_SET_SIZE: [Vec2; 2] = [Vec2::new(0.375, 0.230), Vec2::new(0.369, 0.230)];
const RADIO_SUPPLY: Face = Face { origin: Vec3::new(RIM_X, 1.370, 1.390), u: RADIO_U, v: Vec3::Y, w: RADIO_W };
const RADIO_SUPPLY_SIZE: Vec2 = Vec2::new(0.378, 0.225);
/// Front depth: the source's knob stubs reach 20 mm past the rim (x = -0.148).
const RADIO_DEPTH: f32 = 0.024;
const SUPPLY_DEPTH: f32 = 0.020;

/// The driver's panel face (`Hull_0240`, the plane its ignition lamp stands on), normal toward him.
const DRIVER_NORMAL: Vec3 = Vec3::new(-0.759, -0.051, -0.649);
/// The face's front plane (distance along the normal) and the cluster's corner in its (u, v):
/// above the ignition button, which stands 14 mm proud below it.
const DRIVER_PLANE: f32 = -2.327;
const DRIVER_CORNER: Vec2 = Vec2::new(0.758, 1.598);
const DRIVER_SIZE: Vec2 = Vec2::new(0.212, 0.137);
const DRIVER_DEPTH: f32 = 0.032;

pub(super) fn driver_face() -> Face {
    let w = DRIVER_NORMAL.normalize();
    let u = Vec3::Y.cross(w).normalize();
    let v = w.cross(u);
    Face { origin: u * DRIVER_CORNER.x + v * DRIVER_CORNER.y + w * DRIVER_PLANE, u, v, w }
}

// ── Radio set layout (face metres) ──
const SCALE_WINDOW: [f32; 4] = [0.024, 0.095, 0.250, 0.205];
const SCALE_FLOOR: f32 = 0.006;
/// The needle stands on a pivot below the printed scale, in the window's dark lower strip.
const NEEDLE_PIVOT: Vec2 = Vec2::new(0.137, 0.105);
const NEEDLE_W: f32 = 0.010;
const NEEDLE_LENGTH: f32 = 0.094;
/// Full swing either side of upright.
pub(super) const RADIO_NEEDLE_SWING: f32 = 35.0 * PI / 180.0;
/// The frequency knob turns this many times end to end of the scale.
pub(super) const RADIO_KNOB_TURNS: f32 = 2.5;
const FREQ_KNOB: (Vec2, f32, f32) = (Vec2::new(0.315, 0.062), 0.022, 0.018);
const RADIO_GLASS: [f32; 2] = [0.019, 0.021];

// ── Driver cluster layout ──
const GAUGES: [Vec2; 2] = [Vec2::new(0.050, 0.068), Vec2::new(0.162, 0.068)];
const GAUGE_R: f32 = 0.031;
const GAUGE_FLOOR: f32 = 0.010;
const GAUGE_NEEDLE_W: f32 = 0.0135;
/// Dial sweep: 270 degrees, from lower left (minimum) clockwise to lower right.
const GAUGE_SWEEP: f32 = 1.5 * PI;
const DRUM_WINDOW: [f32; 4] = [0.0870, 0.0625, 0.1250, 0.0735];
const DRUM_U: [f32; 3] = [0.0935, 0.1060, 0.1185];
const DRUM_AXIS_W: f32 = 0.0155;
/// Decagon apothem and width: a 9.1 mm digit on each of ten faces.
const DRUM_APOTHEM: f32 = 0.014;
const DRUM_WIDTH: f32 = 0.011;
const DRIVER_GLASS: [f32; 2] = [0.0306, 0.0316];

/// Colours (sRGB).
const SILVER: [u8; 3] = [190, 192, 194];
/// rm_gbuffer.wgsl `RM_INTERIOR_PAINT` (linear 0.7231, 0.6867, 0.6172) encoded as sRGB.
const INTERIOR_PAINT_SRGB: [u8; 3] = [221, 216, 206];
const NEEDLE_RED: [u8; 3] = [214, 28, 22];
const DRUM_AMBER: [u8; 3] = [232, 148, 38];
const SCALE_CREAM: [u8; 3] = [232, 227, 212];
const DIAL_BLACK: [u8; 3] = [26, 26, 28];
const DIAL_INK: [u8; 3] = [232, 226, 206];
const PRINT_BLACK: [u8; 3] = [22, 22, 24];

/// The panel lamps: amber pilot lamps (the corners' light) and the 8 mm indicators. Appearance
/// calibrations in the cabin's own scale (`interior_rig::CABIN_PRACTICAL_POWER_SCALE`), not
/// measured bulbs. An amber lamp's 14 mm lens emits about half the dome lamp's power.
pub const PANEL_AMBER: FiniteCubeLamp = FiniteCubeLamp { half_size_m: 0.007, srgb: [255, 150, 40], emission: 85.0 };
pub const DRIVER_AMBER: FiniteCubeLamp = FiniteCubeLamp { half_size_m: 0.006, srgb: [255, 150, 40], emission: 85.0 };
pub const INDICATOR_RED: FiniteCubeLamp = FiniteCubeLamp { half_size_m: 0.004, srgb: [255, 40, 28], emission: 1.5 };
pub const INDICATOR_GREEN: FiniteCubeLamp = FiniteCubeLamp { half_size_m: 0.004, srgb: [70, 255, 96], emission: 1.5 };
pub const INDICATOR_TURQUOISE: FiniteCubeLamp = FiniteCubeLamp { half_size_m: 0.004, srgb: [40, 225, 215], emission: 1.5 };
const BEZEL_FOOT: f32 = 1.35;
const BEZEL_HEIGHT: f32 = 0.003;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Station { RadioRear, RadioFront, Driver }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LampRole {
    /// Lit whenever the cabin lamps are on.
    Pilot,
    /// The radio's mains, receive and transmit lamps.
    RadioPower,
    RadioReceive,
    RadioTransmit,
    /// The driver's generator (engine turning), oil warning (engine stopped or coolant at the
    /// boil) and panel-power lamps.
    Charging,
    OilWarning,
    DriverPower,
}

/// Every panel lamp: its station, face position, fixture and what lights it. The order is the
/// cabin light list's (`SHADOW_REGISTRATIONS`).
pub(super) const LAMPS: [(Station, Vec2, FiniteCubeLamp, LampRole); 11] = [
    (Station::RadioRear, Vec2::new(0.330, 0.200), PANEL_AMBER, LampRole::Pilot),
    (Station::RadioRear, Vec2::new(0.278, 0.200), INDICATOR_GREEN, LampRole::RadioPower),
    (Station::RadioRear, Vec2::new(0.298, 0.200), INDICATOR_TURQUOISE, LampRole::RadioReceive),
    (Station::RadioFront, Vec2::new(0.330, 0.200), PANEL_AMBER, LampRole::Pilot),
    (Station::RadioFront, Vec2::new(0.278, 0.200), INDICATOR_RED, LampRole::RadioTransmit),
    (Station::RadioFront, Vec2::new(0.298, 0.200), INDICATOR_GREEN, LampRole::RadioPower),
    (Station::Driver, Vec2::new(0.018, 0.120), DRIVER_AMBER, LampRole::Pilot),
    (Station::Driver, Vec2::new(0.194, 0.120), DRIVER_AMBER, LampRole::Pilot),
    (Station::Driver, Vec2::new(0.084, 0.118), INDICATOR_RED, LampRole::OilWarning),
    (Station::Driver, Vec2::new(0.106, 0.118), INDICATOR_GREEN, LampRole::DriverPower),
    (Station::Driver, Vec2::new(0.128, 0.118), INDICATOR_TURQUOISE, LampRole::Charging),
];
pub(super) const SHADOW_REGISTRATIONS: usize = LAMPS.len();

fn station_face(station: Station) -> Face {
    match station {
        Station::RadioRear => RADIO_SETS[0],
        Station::RadioFront => RADIO_SETS[1],
        Station::Driver => driver_face(),
    }
}
fn station_depth(station: Station) -> f32 {
    if station == Station::Driver { DRIVER_DEPTH } else { RADIO_DEPTH }
}
/// A lamp lens's model in the hull frame: a cube standing on its bezel on the panel front.
pub(super) fn lamp_model(i: usize) -> Mat4 {
    let (station, at, fixture, _) = LAMPS[i];
    let w = station_depth(station) + BEZEL_HEIGHT + fixture.half_size_m;
    station_face(station).model() * Mat4::from_translation(at.extend(w))
}

// ── Materials: one 64 x 64 atlas per body mesh, base colour and metallic-roughness ──
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Swatch { Black, Bezel, Bore, Chrome, Grille, Key }
const ATLAS: usize = 64;
const SILVER_ROWS: usize = 48;
fn swatch_texel(s: Swatch) -> (usize, usize) {
    let k = s as usize;
    (k * 8, SILVER_ROWS + 4)
}
fn swatch_uv(s: Swatch) -> [f32; 2] {
    let (x, y) = swatch_texel(s);
    [(x as f32 + 4.0) / ATLAS as f32, (y as f32) / ATLAS as f32]
}
fn swatch_finish(s: Swatch) -> ([u8; 3], f32, f32) {
    // (sRGB base, metallic, roughness)
    match s {
        Swatch::Black => ([24, 24, 26], 0.0, 0.42),
        Swatch::Bezel => ([14, 14, 15], 0.0, 0.25),
        Swatch::Bore => ([34, 34, 36], 0.0, 0.70),
        Swatch::Chrome => ([222, 223, 226], 1.0, 0.16),
        Swatch::Grille => ([12, 12, 12], 0.0, 0.85),
        Swatch::Key => ([226, 224, 216], 0.0, 0.35),
    }
}
fn hash(n: u32) -> f32 {
    let x = n.wrapping_mul(2654435761) >> 8;
    (x & 0xffff) as f32 / 65535.0
}
/// Brushed aluminium: grain along +u, one atlas row per streak; the base and the roughness vary a
/// little row by row. Rows 0-47; the swatches sit in 8 x 8 cells below.
fn atlases() -> (CpuImage, CpuImage) {
    let mut base = vec![0u8; ATLAS * ATLAS * 4];
    let mut mr = vec![0u8; ATLAS * ATLAS * 4];
    for y in 0..ATLAS {
        for x in 0..ATLAS {
            let o = (y * ATLAS + x) * 4;
            let (rgb, metal, rough) = if y < SILVER_ROWS {
                // The slab bodies wear the Tiger's interior paint (the owner, 2026-11-03: "braun panels
                // material grey should be changed to the same interior metal as the rest of the
                // tank"): rm_gbuffer.wgsl `RM_INTERIOR_PAINT` in sRGB, its 0.70 roughness, dielectric.
                (INTERIOR_PAINT_SRGB, 0.0, 0.70)
            } else {
                let k = (x / 8).min(5);
                let s = [Swatch::Black, Swatch::Bezel, Swatch::Bore, Swatch::Chrome, Swatch::Grille, Swatch::Key][k];
                swatch_finish(s)
            };
            base[o..o + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            mr[o..o + 4].copy_from_slice(&[255, (rough * 255.0) as u8, (metal * 255.0) as u8, 255]);
        }
    }
    (CpuImage::new(ATLAS as u32, ATLAS as u32, base), CpuImage::new(ATLAS as u32, ATLAS as u32, mr))
}
fn silver_uv(v: f32, height: f32) -> [f32; 2] {
    [0.5, ((v / height).clamp(0.0, 1.0) * (SILVER_ROWS as f32 - 1.0) + 0.5) / ATLAS as f32]
}

/// Triangles in a panel's own frame. Quads are wound counter-clockwise seen from outside.
#[derive(Default)]
struct Builder {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl Builder {
    fn quad(&mut self, p: [Vec3; 4], uv: [[f32; 2]; 4]) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let t = (p[1] - p[0]).normalize_or_zero();
        let base = self.vertices.len() as u32;
        for k in 0..4 {
            self.vertices.push(Vertex { pos: p[k].to_array(), normal: n.to_array(), uv: uv[k], tangent: [t.x, t.y, t.z, 1.0] });
        }
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    fn quad_s(&mut self, p: [Vec3; 4], s: Swatch) {
        let uv = swatch_uv(s);
        self.quad(p, [uv; 4]);
    }
    /// An axis-aligned box in the face frame.
    fn cuboid(&mut self, lo: Vec3, hi: Vec3, s: Swatch) {
        let c = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        let (a, b) = (lo, hi);
        self.quad_s([c(a.x, a.y, b.z), c(b.x, a.y, b.z), c(b.x, b.y, b.z), c(a.x, b.y, b.z)], s);
        self.quad_s([c(b.x, a.y, a.z), c(a.x, a.y, a.z), c(a.x, b.y, a.z), c(b.x, b.y, a.z)], s);
        self.quad_s([c(a.x, a.y, a.z), c(a.x, a.y, b.z), c(a.x, b.y, b.z), c(a.x, b.y, a.z)], s);
        self.quad_s([c(b.x, a.y, b.z), c(b.x, a.y, a.z), c(b.x, b.y, a.z), c(b.x, b.y, b.z)], s);
        self.quad_s([c(a.x, b.y, b.z), c(b.x, b.y, b.z), c(b.x, b.y, a.z), c(a.x, b.y, a.z)], s);
        self.quad_s([c(a.x, a.y, a.z), c(b.x, a.y, a.z), c(b.x, a.y, b.z), c(a.x, a.y, b.z)], s);
    }
    /// A closed cylinder standing on the face at `c`, from w = `w0` to `w1`, its top capped.
    fn cylinder(&mut self, c: Vec2, r: f32, w0: f32, w1: f32, n: usize, side: Swatch, top: Swatch) {
        let at = |i: usize, w: f32| {
            let a = TAU * i as f32 / n as f32;
            Vec3::new(c.x + r * a.cos(), c.y + r * a.sin(), w)
        };
        for i in 0..n {
            self.quad_s([at(i, w0), at(i + 1, w0), at(i + 1, w1), at(i, w1)], side);
        }
        self.disc(c, r, w1, n, top, true);
    }
    /// A flat polygon disc facing +w (`up`) or -w.
    fn disc(&mut self, c: Vec2, r: f32, w: f32, n: usize, s: Swatch, up: bool) {
        let uv = swatch_uv(s);
        let normal = if up { Vec3::Z } else { Vec3::NEG_Z };
        let base = self.vertices.len() as u32;
        self.vertices.push(Vertex { pos: [c.x, c.y, w], normal: normal.to_array(), uv, tangent: [1.0, 0.0, 0.0, 1.0] });
        for i in 0..n {
            let a = TAU * i as f32 / n as f32;
            self.vertices.push(Vertex { pos: [c.x + r * a.cos(), c.y + r * a.sin(), w], normal: normal.to_array(), uv, tangent: [1.0, 0.0, 0.0, 1.0] });
        }
        for i in 0..n as u32 {
            let (a, b) = (base + 1 + i, base + 1 + (i + 1) % n as u32);
            if up { self.indices.extend([base, a, b]); } else { self.indices.extend([base, b, a]); }
        }
    }
    fn mesh(self, texture: Option<CpuImage>, mr: Option<CpuImage>) -> CpuMesh {
        let min = self.vertices.iter().fold(Vec3::splat(f32::INFINITY), |a, v| a.min(Vec3::from(v.pos)));
        let max = self.vertices.iter().fold(Vec3::splat(f32::NEG_INFINITY), |a, v| a.max(Vec3::from(v.pos)));
        let mut mesh = CpuMesh::basic(self.vertices, self.indices, texture, min, max);
        if mr.is_some() {
            mesh.metallic_roughness_texture = mr;
            mesh.metallic_factor = 1.0;
            mesh.roughness_factor = 1.0;
        }
        mesh
    }
}

/// An opening through a front slab. A window's bore is walled to `floor`; what lies at the floor
/// (a print, the drums' dark well) is its own part.
#[derive(Clone, Copy, Debug)]
enum Hole {
    Rect { lo: Vec2, hi: Vec2, floor: f32, well: Option<Swatch> },
    Disc { c: Vec2, r: f32, floor: f32 },
}
const DISC_SIDES: usize = 32;
const DISC_RIM: f32 = 0.004;

impl Hole {
    /// The rectangle the hole owns on the front face.
    fn tile(&self) -> (Vec2, Vec2) {
        match *self {
            Hole::Rect { lo, hi, .. } => (lo, hi),
            Hole::Disc { c, r, .. } => (c - Vec2::splat(r + DISC_RIM), c + Vec2::splat(r + DISC_RIM)),
        }
    }
}

/// A closed slab `size` x `depth` standing on the face (w = 0 at the source face), its front
/// brushed silver, pierced by `holes`. The front is the grid of the holes' tile edges minus the
/// tiles; a disc's tile is an exact square annulus whose 32 rays include the square's corners.
fn slab(b: &mut Builder, size: Vec2, depth: f32, holes: &[Hole]) {
    let mut us = vec![0.0, size.x];
    let mut vs = vec![0.0, size.y];
    for h in holes {
        let (lo, hi) = h.tile();
        us.extend([lo.x, hi.x]);
        vs.extend([lo.y, hi.y]);
    }
    for list in [&mut us, &mut vs] {
        list.sort_by(f32::total_cmp);
        list.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    }
    let sv = |v: f32| silver_uv(v, size.y);
    for i in 0..us.len() - 1 {
        for j in 0..vs.len() - 1 {
            let (u0, u1, v0, v1) = (us[i], us[i + 1], vs[j], vs[j + 1]);
            let mid = Vec2::new(u0 + u1, v0 + v1) * 0.5;
            if holes.iter().any(|h| { let (lo, hi) = h.tile(); mid.cmpgt(lo).all() && mid.cmplt(hi).all() }) {
                continue;
            }
            b.quad([Vec3::new(u0, v0, depth), Vec3::new(u1, v0, depth), Vec3::new(u1, v1, depth), Vec3::new(u0, v1, depth)],
                [sv(v0), sv(v0), sv(v1), sv(v1)]);
        }
    }
    for h in holes {
        match *h {
            Hole::Rect { lo, hi, floor, well } => {
                let p = |u: f32, v: f32, w: f32| Vec3::new(u, v, w);
                // Walls face into the opening.
                b.quad_s([p(hi.x, lo.y, floor), p(lo.x, lo.y, floor), p(lo.x, lo.y, depth), p(hi.x, lo.y, depth)], Swatch::Bezel);
                b.quad_s([p(lo.x, hi.y, floor), p(hi.x, hi.y, floor), p(hi.x, hi.y, depth), p(lo.x, hi.y, depth)], Swatch::Bezel);
                b.quad_s([p(lo.x, lo.y, floor), p(lo.x, hi.y, floor), p(lo.x, hi.y, depth), p(lo.x, lo.y, depth)], Swatch::Bezel);
                b.quad_s([p(hi.x, hi.y, floor), p(hi.x, lo.y, floor), p(hi.x, lo.y, depth), p(hi.x, hi.y, depth)], Swatch::Bezel);
                if let Some(s) = well {
                    b.quad_s([p(lo.x, lo.y, floor), p(hi.x, lo.y, floor), p(hi.x, hi.y, floor), p(lo.x, hi.y, floor)], s);
                }
            }
            Hole::Disc { c, r, floor } => {
                let (lo, hi) = h.tile();
                let ring = |i: usize, radius: f32| {
                    let a = TAU * i as f32 / DISC_SIDES as f32;
                    let d = Vec2::new(a.cos(), a.sin());
                    c + d * radius
                };
                let square = |i: usize| {
                    let a = TAU * i as f32 / DISC_SIDES as f32;
                    let d = Vec2::new(a.cos(), a.sin());
                    let s = (r + DISC_RIM) / d.x.abs().max(d.y.abs());
                    (c + d * s).clamp(lo, hi)
                };
                for i in 0..DISC_SIDES {
                    let (a, bb, cc, dd) = (ring(i, r), square(i), square(i + 1), ring(i + 1, r));
                    b.quad([a.extend(depth), bb.extend(depth), cc.extend(depth), dd.extend(depth)],
                        [sv(a.y), sv(bb.y), sv(cc.y), sv(dd.y)]);
                    // The bore's wall, facing in.
                    b.quad_s([ring(i + 1, r).extend(floor), ring(i, r).extend(floor), ring(i, r).extend(depth), ring(i + 1, r).extend(depth)], Swatch::Bezel);
                }
            }
        }
    }
    // The slab's edges, silver, and its back against the source face.
    let p = |u: f32, v: f32, w: f32| Vec3::new(u, v, w);
    let (x, y) = (size.x, size.y);
    b.quad([p(0.0, 0.0, 0.0), p(x, 0.0, 0.0), p(x, 0.0, depth), p(0.0, 0.0, depth)], [sv(0.0); 4]);
    b.quad([p(x, y, 0.0), p(0.0, y, 0.0), p(0.0, y, depth), p(x, y, depth)], [sv(y); 4]);
    b.quad([p(0.0, y, 0.0), p(0.0, 0.0, 0.0), p(0.0, 0.0, depth), p(0.0, y, depth)], [sv(0.0), sv(0.0), sv(y), sv(y)]);
    b.quad([p(x, 0.0, 0.0), p(x, y, 0.0), p(x, y, depth), p(x, 0.0, depth)], [sv(0.0), sv(y), sv(y), sv(0.0)]);
    b.quad_s([p(0.0, 0.0, 0.0), p(0.0, y, 0.0), p(x, y, 0.0), p(x, 0.0, 0.0)], Swatch::Bore);
}

/// A knob on the slab's front: black body, a bright cap a little smaller (the Braun look).
fn knob(b: &mut Builder, c: Vec2, r: f32, height: f32, front: f32) {
    b.cylinder(c, r, front, front + height, 20, Swatch::Black, Swatch::Black);
    b.cylinder(c, r * 0.62, front + height, front + height + 0.0015, 20, Swatch::Chrome, Swatch::Chrome);
}
/// A lamp's black bezel under its lens.
fn bezel(b: &mut Builder, at: Vec2, half: f32, front: f32) {
    let s = half * BEZEL_FOOT;
    b.cuboid(Vec3::new(at.x - s, at.y - s, front), Vec3::new(at.x + s, at.y + s, front + BEZEL_HEIGHT), Swatch::Bezel);
}

fn radio_set_body(set: usize) -> CpuMesh {
    let size = RADIO_SET_SIZE[set];
    let mut b = Builder::default();
    let [u0, v0, u1, v1] = SCALE_WINDOW;
    slab(&mut b, size, RADIO_DEPTH, &[Hole::Rect { lo: Vec2::new(u0, v0), hi: Vec2::new(u1, v1), floor: SCALE_FLOOR, well: None }]);
    // The needle's pivot pin, standing out of the card.
    b.cylinder(NEEDLE_PIVOT, 0.004, SCALE_FLOOR, NEEDLE_W + 0.003, 12, Swatch::Chrome, Swatch::Chrome);
    knob(&mut b, Vec2::new(0.315, 0.150), 0.015, 0.014, RADIO_DEPTH);
    knob(&mut b, Vec2::new(0.060, 0.045), 0.013, 0.012, RADIO_DEPTH);
    knob(&mut b, Vec2::new(0.130, 0.045), 0.013, 0.012, RADIO_DEPTH);
    // A row of four piano keys, the band switch.
    for k in 0..4 {
        let u = 0.172 + k as f32 * 0.023;
        b.cuboid(Vec3::new(u, 0.030, RADIO_DEPTH), Vec3::new(u + 0.020, 0.060, RADIO_DEPTH + 0.008), Swatch::Key);
    }
    let station = if set == 0 { Station::RadioRear } else { Station::RadioFront };
    for (s, at, fixture, _) in LAMPS {
        if s == station { bezel(&mut b, at, fixture.half_size_m, RADIO_DEPTH); }
    }
    b.mesh(None, None)
}

fn radio_supply_body() -> CpuMesh {
    let mut b = Builder::default();
    // A loudspeaker grille of fourteen slots, and the set's mains and volume knobs.
    let holes: Vec<Hole> = (0..14).map(|k| {
        let v = 0.040 + k as f32 * 0.0105;
        Hole::Rect { lo: Vec2::new(0.030, v), hi: Vec2::new(0.250, v + 0.0045), floor: SUPPLY_DEPTH - 0.004, well: Some(Swatch::Grille) }
    }).collect();
    slab(&mut b, RADIO_SUPPLY_SIZE, SUPPLY_DEPTH, &holes);
    knob(&mut b, Vec2::new(0.318, 0.150), 0.020, 0.016, SUPPLY_DEPTH);
    knob(&mut b, Vec2::new(0.318, 0.070), 0.013, 0.012, SUPPLY_DEPTH);
    b.mesh(None, None)
}

fn driver_body() -> CpuMesh {
    let mut b = Builder::default();
    let [u0, v0, u1, v1] = DRUM_WINDOW;
    let mut holes: Vec<Hole> = GAUGES.iter().map(|&c| Hole::Disc { c, r: GAUGE_R, floor: GAUGE_FLOOR }).collect();
    holes.push(Hole::Rect { lo: Vec2::new(u0, v0), hi: Vec2::new(u1, v1), floor: 0.001, well: Some(Swatch::Bore) });
    slab(&mut b, DRIVER_SIZE, DRIVER_DEPTH, &holes);
    // The gauges' centre pins, silver, through the needles' hubs.
    for c in GAUGES {
        b.cylinder(c, 0.0028, GAUGE_FLOOR, GAUGE_NEEDLE_W + 0.0035, 12, Swatch::Chrome, Swatch::Chrome);
    }
    // The fuel counter's reset knob under its window.
    knob(&mut b, Vec2::new(0.106, 0.040), 0.006, 0.008, DRIVER_DEPTH);
    for (s, at, fixture, _) in LAMPS {
        if s == Station::Driver { bezel(&mut b, at, fixture.half_size_m, DRIVER_DEPTH); }
    }
    b.mesh(None, None)
}

fn with_atlas(mesh: CpuMesh) -> CpuMesh {
    let (base, mr) = atlases();
    let mut mesh = mesh;
    mesh.texture = Some(base);
    mesh.metallic_roughness_texture = Some(mr);
    mesh.metallic_factor = 1.0;
    mesh.roughness_factor = 1.0;
    mesh
}

// ── Prints ──
fn image(w: usize, h: usize, f: impl Fn(f32, f32) -> [u8; 3]) -> CpuImage {
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let c = f((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            rgba.extend([c[0], c[1], c[2], 255]);
        }
    }
    CpuImage::new(w as u32, h as u32, rgba)
}

/// The radio's tuning scale on its card: two wavebands of ticks on arcs about the needle's pivot,
/// cream card, black print, a red band on the upper scale. Below the scale the card is dark so the
/// needle's foot and pivot read.
fn scale_image() -> CpuImage {
    let [u0, v0, u1, v1] = SCALE_WINDOW;
    image(256, 128, |s, t| {
        let p = Vec2::new(u0 + s * (u1 - u0), v1 - t * (v1 - v0));
        let d = p - NEEDLE_PIVOT;
        let r = d.length();
        let a = d.x.atan2(d.y); // 0 upright, positive to the right
        let deg = a.to_degrees();
        if p.y < 0.128 { return [48, 48, 50]; }
        let tick = |step: f32, width_deg: f32| ((deg / step).round() * step - deg).abs() < width_deg;
        let inside = deg.abs() <= RADIO_NEEDLE_SWING.to_degrees() + 1.0;
        if inside {
            // Upper band.
            if (0.0855..0.0870).contains(&r) { return PRINT_BLACK; }
            if (0.0885..0.0905).contains(&r) && deg > 15.0 { return NEEDLE_RED; }
            if (0.0790..0.0855).contains(&r) && tick(10.0, 0.45) { return PRINT_BLACK; }
            if (0.0820..0.0855).contains(&r) && tick(2.5, 0.30) { return PRINT_BLACK; }
            // Lower band, offset by half a step.
            if (0.0650..0.0660).contains(&r) { return PRINT_BLACK; }
            if (0.0660..0.0700).contains(&r) && tick(5.0, 0.35) { return PRINT_BLACK; }
        }
        SCALE_CREAM
    })
}

/// Both of the driver's dials in one image: left the tachometer, right the coolant thermometer.
/// Black enamel, cream ticks over 270 degrees, the last two divisions red.
fn dial_image() -> CpuImage {
    image(256, 128, |s, t| {
        let local = Vec2::new((s * 2.0).fract() * 2.0 - 1.0, 1.0 - t * 2.0);
        let r = local.length();
        let a = local.x.atan2(local.y); // clockwise from up
        let along = (a + GAUGE_SWEEP * 0.5) / GAUGE_SWEEP; // 0 at minimum, 1 at maximum
        if r > 0.98 { return DIAL_BLACK; }
        if (0.0..=1.0).contains(&along) {
            let div = along * 10.0;
            let major = (div - div.round()).abs() * GAUGE_SWEEP / 10.0 * r < 0.035;
            let minor = (div * 2.0 - (div * 2.0).round()).abs() * GAUGE_SWEEP / 20.0 * r < 0.022;
            if along > 0.8 && (0.84..0.92).contains(&r) { return NEEDLE_RED; }
            if major && (0.70..0.92).contains(&r) { return DIAL_INK; }
            if minor && (0.80..0.92).contains(&r) { return DIAL_INK; }
        }
        DIAL_BLACK
    })
}

const DIGITS: [[u8; 7]; 10] = [
    [14, 17, 19, 21, 25, 17, 14], [4, 12, 4, 4, 4, 4, 14], [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30], [2, 6, 10, 18, 31, 2, 2], [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14], [31, 1, 2, 4, 8, 8, 8], [14, 17, 17, 14, 17, 17, 14],
    [14, 17, 17, 15, 1, 1, 14],
];
/// The drums' strip: ten 16 x 16 cells top to bottom, 0-9, dark numerals on amber.
fn drum_image() -> CpuImage {
    let (w, h) = (16, 160);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (digit, row) = (y / 16, y % 16);
            let (gx, gy) = (x as i32 - 3, row as i32 - 1);
            let ink = (0..10).contains(&gx) && (0..14).contains(&gy)
                && DIGITS[digit][(gy / 2) as usize] & (1 << (4 - gx / 2)) != 0;
            let c = if ink { [30, 20, 10] } else { DRUM_AMBER };
            rgba.extend([c[0], c[1], c[2], 255]);
        }
    }
    CpuImage::new(w as u32, h as u32, rgba)
}

fn printed_quad(lo: Vec2, hi: Vec2, w: f32, uv_lo: [f32; 2], uv_hi: [f32; 2], b: &mut Builder) {
    b.quad([Vec3::new(lo.x, lo.y, w), Vec3::new(hi.x, lo.y, w), Vec3::new(hi.x, hi.y, w), Vec3::new(lo.x, hi.y, w)],
        [[uv_lo[0], uv_hi[1]], [uv_hi[0], uv_hi[1]], [uv_hi[0], uv_lo[1]], [uv_lo[0], uv_lo[1]]]);
}

fn scale_cards() -> CpuMesh {
    let mut b = Builder::default();
    let [u0, v0, u1, v1] = SCALE_WINDOW;
    let mut parts = Vec::new();
    for set in 0..2 {
        let mut one = Builder::default();
        printed_quad(Vec2::new(u0, v0), Vec2::new(u1, v1), SCALE_FLOOR, [0.0, 0.0], [1.0, 1.0], &mut one);
        parts.push((one, RADIO_SETS[set].model()));
    }
    for (one, model) in parts { append_transformed(&mut b, one, model); }
    b.mesh(Some(scale_image()), None)
}

fn dial_faces() -> CpuMesh {
    let mut b = Builder::default();
    for (k, c) in GAUGES.iter().enumerate() {
        let base = b.vertices.len() as u32;
        let uv = |p: Vec2| {
            let l = (p - *c) / GAUGE_R;
            [(k as f32 + (l.x + 1.0) * 0.5) * 0.5, (1.0 - l.y) * 0.5]
        };
        b.vertices.push(Vertex { pos: [c.x, c.y, GAUGE_FLOOR], normal: [0.0, 0.0, 1.0], uv: uv(*c), tangent: [1.0, 0.0, 0.0, 1.0] });
        for i in 0..DISC_SIDES {
            let a = TAU * i as f32 / DISC_SIDES as f32;
            let p = *c + Vec2::new(a.cos(), a.sin()) * GAUGE_R;
            b.vertices.push(Vertex { pos: [p.x, p.y, GAUGE_FLOOR], normal: [0.0, 0.0, 1.0], uv: uv(p), tangent: [1.0, 0.0, 0.0, 1.0] });
        }
        for i in 0..DISC_SIDES as u32 {
            b.indices.extend([base, base + 1 + i, base + 1 + (i + 1) % DISC_SIDES as u32]);
        }
    }
    b.mesh(Some(dial_image()), None).transformed(driver_face().model())
}

fn append_transformed(into: &mut Builder, part: Builder, model: Mat4) {
    let mesh = part.mesh(None, None).transformed(model);
    let base = into.vertices.len() as u32;
    into.vertices.extend(mesh.vertices);
    into.indices.extend(mesh.indices.into_iter().map(|i| i + base));
}

/// Closed glass prisms: the scale windows' panes (radio) and the dials' and counter's (driver).
fn glass_mesh(rects: &[(Vec2, Vec2)], discs: &[(Vec2, f32)], w: [f32; 2], face: &[Face]) -> CpuMesh {
    let mut out = Builder::default();
    for f in face {
        let mut b = Builder::default();
        for &(lo, hi) in rects { b.cuboid(lo.extend(w[0]), hi.extend(w[1]), Swatch::Key); }
        for &(c, r) in discs {
            b.cylinder(c, r, w[0], w[1], DISC_SIDES, Swatch::Key, Swatch::Key);
            b.disc(c, r, w[0], DISC_SIDES, Swatch::Key, false);
        }
        append_transformed(&mut out, b, f.model());
    }
    let mut mesh = out.mesh(None, None);
    mesh.roughness_factor = 0.06;
    mesh
}

fn needle_mesh(width: f32, tail: f32, length: f32, thickness: f32) -> CpuMesh {
    let mut b = Builder::default();
    b.cuboid(Vec3::new(-width * 0.5, -tail, 0.0), Vec3::new(width * 0.5, length, thickness), Swatch::Black);
    // A hub round the pivot.
    b.cylinder(Vec2::ZERO, width * 1.6, 0.0, thickness, 12, Swatch::Black, Swatch::Black);
    let mut mesh = b.mesh(Some(crate::prims::solid_color(NEEDLE_RED)), None);
    mesh.roughness_factor = 0.45;
    mesh
}

/// The frequency knob about its own axis (+w), its foot on the slab: black, a bright cap and a
/// white index line so its turning shows.
fn freq_knob_mesh() -> CpuMesh {
    let (_, r, h) = FREQ_KNOB;
    let mut b = Builder::default();
    b.cylinder(Vec2::ZERO, r, 0.0, h, 24, Swatch::Black, Swatch::Black);
    b.cylinder(Vec2::ZERO, r * 0.62, h, h + 0.0015, 24, Swatch::Chrome, Swatch::Chrome);
    b.cuboid(Vec3::new(-0.0012, r * 0.66, h - 0.0004), Vec3::new(0.0012, r * 0.95, h + 0.0006), Swatch::Key);
    with_atlas(b.mesh(None, None))
}

/// One fuel drum about its axis (+u): a decagonal prism, digit k on the face turned k x 36 degrees
/// up from the front, so rolling forward brings the next digit up from below.
fn drum_mesh() -> CpuMesh {
    let half_h = DRUM_APOTHEM * (PI / 10.0).tan();
    let x = DRUM_WIDTH * 0.5;
    let mut b = Builder::default();
    for k in 0..10 {
        let rot = Mat4::from_rotation_x(k as f32 * TAU / 10.0);
        let p = |y: f32, xx: f32| rot.transform_point3(Vec3::new(xx, y, DRUM_APOTHEM));
        let (t0, t1) = (k as f32 / 10.0, (k + 1) as f32 / 10.0);
        b.quad([p(-half_h, -x), p(-half_h, x), p(half_h, x), p(half_h, -x)], [[0.0, t1], [1.0, t1], [1.0, t0], [0.0, t0]]);
    }
    // End caps (amber, the strip's blank margin).
    let r = DRUM_APOTHEM / (PI / 10.0).cos();
    for side in [-x, x] {
        let base = b.vertices.len() as u32;
        let n = if side > 0.0 { Vec3::X } else { Vec3::NEG_X };
        b.vertices.push(Vertex { pos: [side, 0.0, 0.0], normal: n.to_array(), uv: [0.05, 0.05], tangent: [0.0, 0.0, 1.0, 1.0] });
        for i in 0..10 {
            let a = TAU * (i as f32 + 0.5) / 10.0;
            b.vertices.push(Vertex { pos: [side, -r * a.sin(), r * a.cos()], normal: n.to_array(), uv: [0.05, 0.05], tangent: [0.0, 0.0, 1.0, 1.0] });
        }
        for i in 0..10u32 {
            let (a, c) = (base + 1 + i, base + 1 + (i + 1) % 10);
            if side > 0.0 { b.indices.extend([base, a, c]); } else { b.indices.extend([base, c, a]); }
        }
    }
    let mut mesh = b.mesh(Some(drum_image()), None);
    mesh.roughness_factor = 0.5;
    mesh
}

/// Odometer positions of the three drums (hundreds, tens, units) for a reading: the units drum
/// turns with the value; each higher drum turns through its step while the drum below passes from
/// 9 to 0.
pub(super) fn drum_positions(value: f32) -> [f32; 3] {
    let value = if value.is_finite() { value.clamp(0.0, 999.0) } else { 0.0 };
    let units = value % 10.0;
    let tens = (value / 10.0).floor() % 10.0 + (units - 9.0).clamp(0.0, 1.0);
    let hundreds = (value / 100.0).floor() % 10.0 + ((value % 100.0) - 99.0).clamp(0.0, 1.0);
    [hundreds, tens, units]
}

/// A gauge needle's angle, clockwise from upright, for a reading in [lo, hi].
pub(super) fn gauge_angle(value: Option<f32>, lo: f32, hi: f32) -> f32 {
    let along = value.filter(|v| v.is_finite()).map_or(0.0, |v| ((v - lo) / (hi - lo)).clamp(0.0, 1.0));
    -GAUGE_SWEEP * 0.5 + GAUGE_SWEEP * along
}

/// What the instruments show this frame. Everything comes from the simulation.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PanelState {
    pub engine_rpm: f32,
    pub coolant_c: Option<f32>,
    pub fuel_liters: Option<f32>,
    pub radio_transmitting: bool,
}

pub(super) const TACHOMETER_MAX_RPM: f32 = 3000.0;
pub(super) const COOLANT_SCALE_C: [f32; 2] = [40.0, 120.0];
/// The oil lamp lights below this speed (a stopped engine has no oil pressure).
const OIL_PRESSURE_RPM: f32 = 400.0;

pub(super) struct CabinPanels {
    statics: Vec<usize>,
    glass: [usize; 2],
    radio_needles: [usize; 2],
    radio_knobs: [usize; 2],
    gauge_needles: [usize; 2],
    drums: [usize; 3],
    lenses: [usize; SHADOW_REGISTRATIONS],
    /// Each radio set's tuning, 0 at the scale's left end to 1 at its right.
    tuning: [f32; 2],
}

impl CabinPanels {
    pub(super) fn append(scene: &mut Vec<CpuMesh>) -> Self {
        let mut push = |mesh: CpuMesh| { scene.push(mesh); scene.len() - 1 };
        let statics = vec![
            push(with_atlas(radio_set_body(0)).transformed(RADIO_SETS[0].model())),
            push(with_atlas(radio_set_body(1)).transformed(RADIO_SETS[1].model())),
            push(with_atlas(radio_supply_body()).transformed(RADIO_SUPPLY.model())),
            push(with_atlas(driver_body()).transformed(driver_face().model())),
            push(scale_cards()),
            push(dial_faces()),
        ];
        let [u0, v0, u1, v1] = SCALE_WINDOW;
        let glass = [
            push(glass_mesh(&[(Vec2::new(u0, v0), Vec2::new(u1, v1))], &[], RADIO_GLASS, &RADIO_SETS)),
            push(glass_mesh(&[(Vec2::new(DRUM_WINDOW[0], DRUM_WINDOW[1]), Vec2::new(DRUM_WINDOW[2], DRUM_WINDOW[3]))],
                &GAUGES.map(|c| (c, GAUGE_R)), DRIVER_GLASS, &[driver_face()])),
        ];
        let radio_needles = std::array::from_fn(|_| push(needle_mesh(0.0024, 0.006, NEEDLE_LENGTH, 0.0012)));
        let radio_knobs = std::array::from_fn(|_| push(freq_knob_mesh()));
        let gauge_needles = std::array::from_fn(|_| push(needle_mesh(0.0022, 0.006, GAUGE_R * 0.86, 0.0012)));
        let drums = std::array::from_fn(|_| push(drum_mesh()));
        let lenses = std::array::from_fn(|i| {
            let f = LAMPS[i].2;
            push(crate::prims::box_mesh(Vec3::splat(-f.half_size_m), Vec3::splat(f.half_size_m), f.srgb))
        });
        Self { statics, glass, radio_needles, radio_knobs, gauge_needles, drums, lenses, tuning: [0.35, 0.62] }
    }

    pub(super) fn configure(&self, r: &mut Renderer) {
        for &slot in &self.glass {
            r.set_glass(slot, true);
            r.mark_instrument_cover(slot);
        }
        for &slot in &self.lenses {
            r.mark_own_emissive(slot);
            r.mark_indicator_lamp(slot);
        }
        for &slot in self.statics.iter().chain(&self.glass).chain(&self.radio_needles).chain(&self.radio_knobs)
            .chain(&self.gauge_needles).chain(&self.drums).chain(&self.lenses) {
            r.set_emissive(slot, 0.0);
            r.set_visible(slot, false);
        }
    }

    /// Groundwork for the radio operator's tuning: `t` from 0 (scale's left end) to 1 (right).
    /// The frequency knob turns `RADIO_KNOB_TURNS` times end to end; the needle follows.
    pub(super) fn set_radio_tuning(&mut self, set: usize, t: f32) {
        if let Some(slot) = self.tuning.get_mut(set) {
            if t.is_finite() { *slot = t.clamp(0.0, 1.0); }
        }
    }
    #[allow(dead_code)]
    pub(super) fn radio_tuning(&self, set: usize) -> f32 { self.tuning[set] }
    /// The needle's angle, clockwise from upright, for a tuning.
    pub(super) fn radio_needle_angle(t: f32) -> f32 { -RADIO_NEEDLE_SWING + 2.0 * RADIO_NEEDLE_SWING * t.clamp(0.0, 1.0) }

    fn lamp_lit(role: LampRole, state: &PanelState) -> f32 {
        let running = state.engine_rpm >= OIL_PRESSURE_RPM;
        let boiling = state.coolant_c.is_some_and(|c| c.is_finite() && c >= 100.0);
        let on = match role {
            LampRole::Pilot | LampRole::RadioPower | LampRole::RadioReceive | LampRole::DriverPower => true,
            // The transmitter's lamp: lit while it sends; otherwise its heaters' standby glow.
            LampRole::RadioTransmit => return if state.radio_transmitting { 1.0 } else { 0.45 },
            LampRole::Charging => running,
            LampRole::OilWarning => !running || boiling,
        };
        if on { 1.0 } else { 0.0 }
    }

    /// Place every part on the hull and light the lamps. Returns the panel lamps' shadow
    /// registrations in `LAMPS` order, unpowered ones included at zero output.
    pub(super) fn drive(&self, r: &mut Renderer, hm: Mat4, power: f32, state: &PanelState)
        -> [(PointLight, Option<usize>); SHADOW_REGISTRATIONS] {
        for &slot in self.statics.iter().chain(&self.glass) {
            r.set_model(slot, hm);
            r.set_visible(slot, true);
            r.set_emissive(slot, 0.0);
        }
        let place = |r: &mut Renderer, slot: usize, model: Mat4| {
            r.set_model(slot, model);
            r.set_visible(slot, true);
            r.set_emissive(slot, 0.0);
        };
        for set in 0..2 {
            let face = hm * RADIO_SETS[set].model();
            let t = self.tuning[set];
            place(r, self.radio_needles[set], face * Mat4::from_translation(NEEDLE_PIVOT.extend(NEEDLE_W))
                * Mat4::from_rotation_z(-Self::radio_needle_angle(t)));
            let (c, _, _) = FREQ_KNOB;
            place(r, self.radio_knobs[set], face * Mat4::from_translation(c.extend(RADIO_DEPTH))
                * Mat4::from_rotation_z(-t * RADIO_KNOB_TURNS * TAU));
        }
        let face = hm * driver_face().model();
        let angles = [gauge_angle(Some(state.engine_rpm), 0.0, TACHOMETER_MAX_RPM),
            gauge_angle(state.coolant_c, COOLANT_SCALE_C[0], COOLANT_SCALE_C[1])];
        for k in 0..2 {
            place(r, self.gauge_needles[k], face * Mat4::from_translation(GAUGES[k].extend(GAUGE_NEEDLE_W))
                * Mat4::from_rotation_z(-angles[k]));
        }
        let drum_centre_v = (DRUM_WINDOW[1] + DRUM_WINDOW[3]) * 0.5;
        for (k, pos) in drum_positions(state.fuel_liters.unwrap_or(0.0)).into_iter().enumerate() {
            place(r, self.drums[k], face * Mat4::from_translation(Vec3::new(DRUM_U[k], drum_centre_v, DRUM_AXIS_W))
                * Mat4::from_rotation_x(-pos * TAU / 10.0));
        }
        std::array::from_fn(|i| {
            let (_, _, fixture, role) = LAMPS[i];
            let model = hm * lamp_model(i);
            let e = fixture.sample(Self::lamp_lit(role, state) * power);
            let slot = self.lenses[i];
            r.set_visible(slot, true);
            r.set_emissive(slot, e.visible_emissive);
            r.set_model(slot, model);
            (PointLight { pos: model.transform_point3(Vec3::ZERO), source_radius: e.source_radius_m,
                color: e.linear_color, radiant_intensity: e.radiant_intensity }, Some(slot))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_are_right_handed_and_point_at_their_operators() {
        for face in RADIO_SETS.iter().chain([&RADIO_SUPPLY]).copied().chain([driver_face()]) {
            assert!((face.u.cross(face.v) - face.w).length() < 1e-5);
            assert!(face.u.dot(face.v).abs() < 1e-5);
        }
        // The radio operator sits at x ~ -1; the driver behind and right of his panel.
        assert!(RADIO_SETS[0].w.dot(Vec3::new(-1.0, 1.6, 1.6) - RADIO_SETS[0].origin) > 0.0);
        let d = driver_face();
        assert!(d.w.dot(Vec3::new(0.85, 1.6, 1.55) - d.origin) > 0.0);
        // The driver's cluster corner lies on the panel's measured front plane.
        assert!((d.origin.dot(DRIVER_NORMAL.normalize()) - DRIVER_PLANE).abs() < 1e-5);
    }

    #[test]
    fn odometer_drums_carry_only_through_the_last_unit() {
        assert_eq!(drum_positions(420.0), [4.0, 2.0, 0.0]);
        let p = drum_positions(419.5);
        assert!((p[2] - 9.5).abs() < 1e-4 && (p[1] - 1.5).abs() < 1e-4 && (p[0] - 4.0).abs() < 1e-4);
        let p = drum_positions(399.25);
        assert!((p[0] - 3.25).abs() < 1e-4 && (p[1] - 9.25).abs() < 1e-4);
        assert_eq!(drum_positions(f32::NAN), [0.0; 3]);
    }

    #[test]
    fn gauges_sweep_270_degrees_and_unknown_reads_minimum() {
        assert!((gauge_angle(Some(0.0), 0.0, 3000.0) + 0.75 * PI).abs() < 1e-5);
        assert!((gauge_angle(Some(3000.0), 0.0, 3000.0) - 0.75 * PI).abs() < 1e-5);
        assert!(gauge_angle(Some(1500.0), 0.0, 3000.0).abs() < 1e-5);
        assert_eq!(gauge_angle(None, 40.0, 120.0), gauge_angle(Some(-5.0), 40.0, 120.0));
    }

    #[test]
    fn radio_needle_and_knob_follow_the_tuning() {
        let mut p = CabinPanels { statics: vec![], glass: [0; 2], radio_needles: [0; 2], radio_knobs: [0; 2],
            gauge_needles: [0; 2], drums: [0; 3], lenses: [0; SHADOW_REGISTRATIONS], tuning: [0.5; 2] };
        assert!(CabinPanels::radio_needle_angle(0.5).abs() < 1e-6);
        p.set_radio_tuning(1, 2.0);
        assert_eq!(p.radio_tuning(1), 1.0);
        p.set_radio_tuning(1, f32::NAN);
        assert_eq!(p.radio_tuning(1), 1.0);
        assert!((CabinPanels::radio_needle_angle(0.0) + RADIO_NEEDLE_SWING).abs() < 1e-6);
        // At full swing the needle's tip stays inside the scale window.
        for t in [0.0, 1.0] {
            let a = CabinPanels::radio_needle_angle(t);
            let tip = NEEDLE_PIVOT + Vec2::new(a.sin(), a.cos()) * NEEDLE_LENGTH;
            assert!(tip.x > SCALE_WINDOW[0] && tip.x < SCALE_WINDOW[2] && tip.y < SCALE_WINDOW[3], "{tip}");
        }
    }

    #[test]
    fn needles_drums_and_glass_clear_each_other() {
        // Radio: card < needle < glass < front.
        assert!(SCALE_FLOOR < NEEDLE_W && NEEDLE_W + 0.0012 < RADIO_GLASS[0] && RADIO_GLASS[1] < RADIO_DEPTH);
        // Driver: dial < needle < pin top < glass < front; the drums turn inside the slab, under the glass.
        assert!(GAUGE_FLOOR < GAUGE_NEEDLE_W && GAUGE_NEEDLE_W + 0.0035 < DRIVER_GLASS[0] && DRIVER_GLASS[1] <= DRIVER_DEPTH);
        let circum = DRUM_APOTHEM / (PI / 10.0).cos();
        assert!(DRUM_AXIS_W - circum > 0.0, "drums stay in front of the source face");
        assert!(DRUM_AXIS_W + circum < DRIVER_GLASS[0], "a rolling drum's corners clear the glass");
        assert!(2.0 * DRUM_APOTHEM * (PI / 10.0).tan() < DRUM_WINDOW[3] - DRUM_WINDOW[1]);
        assert!(DRUM_U[0] - DRUM_WIDTH * 0.5 > DRUM_WINDOW[0] && DRUM_U[2] + DRUM_WIDTH * 0.5 < DRUM_WINDOW[2]);
    }

    #[test]
    fn every_lamp_and_knob_stays_on_its_slab() {
        for (station, at, fixture, _) in LAMPS {
            let size = match station { Station::RadioRear => RADIO_SET_SIZE[0], Station::RadioFront => RADIO_SET_SIZE[1], Station::Driver => DRIVER_SIZE };
            let s = fixture.half_size_m * BEZEL_FOOT;
            assert!(at.x - s > 0.0 && at.y - s > 0.0 && at.x + s < size.x && at.y + s < size.y, "{station:?} {at}");
        }
        for (i, a) in LAMPS.iter().enumerate() {
            for b in &LAMPS[i + 1..] {
                if a.0 == b.0 {
                    assert!((a.1 - b.1).abs().max_element() > (a.2.half_size_m + b.2.half_size_m) * BEZEL_FOOT, "{a:?} {b:?}");
                }
            }
        }
        // The driver's lamps clear the dials' rims.
        for (station, at, fixture, _) in LAMPS {
            if station != Station::Driver { continue; }
            for c in GAUGES {
                let d = (at - c).abs() - Vec2::splat(GAUGE_R + DISC_RIM + fixture.half_size_m * BEZEL_FOOT);
                assert!(d.max_element() > 0.0, "{at} {c}");
            }
        }
    }

    #[test]
    fn slab_front_closes_round_its_bores() {
        let mut b = Builder::default();
        slab(&mut b, DRIVER_SIZE, DRIVER_DEPTH, &[Hole::Disc { c: GAUGES[0], r: GAUGE_R, floor: GAUGE_FLOOR }]);
        // Front area = slab - disc (the annulus is exact; the bore's polygon has 32 sides).
        let mut front = 0.0;
        for t in b.indices.chunks_exact(3) {
            let p: [Vec3; 3] = std::array::from_fn(|k| Vec3::from(b.vertices[t[k] as usize].pos));
            if p.iter().all(|q| (q.z - DRIVER_DEPTH).abs() < 1e-6) {
                front += (p[1] - p[0]).cross(p[2] - p[0]).z * 0.5;
            }
        }
        let polygon = 0.5 * DISC_SIDES as f32 * GAUGE_R * GAUGE_R * (TAU / DISC_SIDES as f32).sin();
        let expected = DRIVER_SIZE.x * DRIVER_SIZE.y - polygon;
        assert!((front - expected).abs() < 1e-6, "{front} {expected}");
    }
}
