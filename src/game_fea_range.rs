//! `fea-range`: the owner's FEA range, a windowed review editor for the layered damage model
//! (docs/FEA_DAMAGE_MODEL_20261102.md) built like the world editor (`edit`): the Canyon v2 winter
//! workshop's snow field (act 1), the player's Tiger with its five-man solo crew at the review spot
//! (`game_fea_review::TIGER_AT`), rendered by the shipping raymarch renderer and simulated by the
//! game's own update. Nothing here is a second simulation:
//!
//! - **Shot**: a round of any catalogue shell (`ballistics`) is launched by
//!   `Game::debug_launch_round` 9 m short of the first vehicle surface on its line, with the speed,
//!   flown distance and age of a shot from the chosen range (the impact velocity is the shell's own
//!   `v(x) = v0·e^(−kx)` at that range; setting a velocity sets the range that gives it). Its line
//!   is aimed by clicking the tank (the camera's ray: an interior surface seen through the section
//!   is aimed along the same line from outside) or at a crewman, and turned by azimuth/elevation.
//!   The ordinary flight sweep, terminal resolver, layered chain, voxel lattice, fragment tracing,
//!   ignition and the crew's voxel tissue resolve it.
//! - **Run and timeline**: always live. A run fires the shot (on a fresh Tiger unless switched
//!   off) and the simulation plays it: the first span one fragment step (1/480 s) per frame, then
//!   1/30 s per frame, then on in real time; the camera, section and gizmos stay free throughout
//!   and every run frame's picture is kept as the timeline's preview. Space pauses; → steps a
//!   frame; dragging the timeline shows the previews and letting go makes that frame live: on
//!   from the present, or back from the run's checkpoints (`rewind`: the state put back, not
//!   re-simulated; the status line says whether the stepped frame matched its checkpoint bit for
//!   bit). Every run starts from the same kept fresh Tiger, so a run repeats (`--shots` checks a
//!   tape re-run against the first run field by field).
//! - **Section**: the film section (`Renderer::set_film_section`), a camera-only plane through the
//!   Tiger; shadows, light, ray queries and every simulation see the whole closed tank.
//! - **Sky**: the renderer's sky clock held at the chosen date and local time (Stalingrad), the
//!   weather override, the eye's exposure automatic or held.
//! - **Tissue**: review-only overrides of the crew lattice's authored constants
//!   (`crew_tissue::TissueTuning`), never a shipping default; P prints them for hand-over.
//!
//! `fea-range --shots PREFIX` renders the editor headless (stills of the interface, a short run,
//! a scrubbed frame, a re-simulation from the fresh Tiger and seeks from the checkpoints, each
//! with its agreement check). `KESSEL_FEA_RANGE_PROBE=determinism` runs the short run three times
//! and names the first field and frame where runs part; `=rewind-memory` reports the checkpoints'
//! memory over the editor's own run. Requires the raymarch renderer (set when KESSEL_RENDERER is
//! unset).
use super::*;
#[path = "game_fea_range/rewind.rs"]
mod rewind;
#[path = "game_fea_range/cache.rs"]
mod cache;
use crate::{act_manifest, celestial_sky, ephemeris, graphics, world};
use std::collections::HashSet;
use glam::Vec4;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

/// One fixed hot-fragment step: the slow-motion frame.
const SLOW_DT: f32 = 1.0 / 480.0;
const TICK: f32 = 1.0 / 60.0;
/// Real-time frames are recorded at 30 a second: two game ticks each.
const REAL_TICKS_PER_FRAME: u32 = 2;
/// After this long with nothing changing the window renders ten frames a second (the owner plays
/// on the same machine).
const IDLE_AFTER_S: f32 = 2.0;
/// Where the section's offset is measured from (hull frame): the fighting compartment's middle.
const SECTION_PIVOT: Vec3 = Vec3::new(0.0, 1.4, 0.0);
/// The vehicle's box for the section's meshes (hull frame), as the film's engine section.
const TANK_LO: Vec3 = Vec3::new(-2.6, -0.4, -4.8);
const TANK_HI: Vec3 = Vec3::new(2.6, 4.0, 8.5);
/// The Tiger's place: the canyon's middle, on its floor's midline (about −24 m) between the burnt
/// wrecks up the valley and the review spot at its foot (the owner, 2026-11-03), clear of props in
/// both canyon manifests; headed up the canyon. T moves it in the window.
const TIGER_AT: Vec2 = Vec2::new(-655.0, 100.0);
const TIGER_HEADING: f32 = -0.15;
/// Stalingrad is three hours ahead of UTC in the campaign's clock (`game_fea_film`: 18:30 local is
/// 15:30Z).
const LOCAL_UTC_OFFSET_H: f64 = 3.0;

/// Review-only (`fea-range`'s LAMP POWER): a factor on the cabin lamps' radiant power (their
/// light and their glowing bulbs alike; `Game` multiplies the lamp switch by it). 1 everywhere
/// else: no play path sets it (test `the_lamp_power_is_one_unless_the_range_sets_it`).
static REVIEW_LAMP_GAIN_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);

pub(super) fn review_lamp_gain() -> f32 {
    f32::from_bits(REVIEW_LAMP_GAIN_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

/// Review-only (`fea-range`'s EFFECTS dials): the impact smoke's density factor and onset delay
/// (`weapon_vfx::impact_smoke_build`); 1 and the authored delay in every other path.
static REVIEW_SMOKE_DENSITY_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);
static REVIEW_SMOKE_DELAY_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);

pub(super) fn review_smoke_density() -> f32 {
    f32::from_bits(REVIEW_SMOKE_DENSITY_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

pub(super) fn review_smoke_delay() -> f32 {
    match REVIEW_SMOKE_DELAY_BITS.load(std::sync::atomic::Ordering::Relaxed) {
        u32::MAX => super::weapon_vfx::IMPACT_SMOKE_DELAY_S,
        bits => f32::from_bits(bits),
    }
}

fn set_review_smoke(density: f32, delay: Option<f32>) {
    REVIEW_SMOKE_DENSITY_BITS.store(density.to_bits(), std::sync::atomic::Ordering::Relaxed);
    REVIEW_SMOKE_DELAY_BITS.store(delay.map_or(u32::MAX, f32::to_bits), std::sync::atomic::Ordering::Relaxed);
}

fn set_review_lamp_gain(gain: f32) {
    REVIEW_LAMP_GAIN_BITS.store(gain.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// The gun that fires each catalogue round and the vehicles that carried it (`shells()` order).
const SHOOTERS: [&str; 16] = [
    "M-42 45 MM ANTI-TANK GUN (TOWED)",
    "20-K 45 MM: T-70, BT-7, T-26",
    "20-K 45 MM: T-70, BT-7, T-26",
    "ZIS-2 57 MM ANTI-TANK GUN; ZIS-4: T-34-57",
    "F-34 / ZIS-5 76 MM: T-34/76, KV-1",
    "F-34 / ZIS-5 76 MM: T-34/76, KV-1",
    "F-34 / ZIS-5 76 MM: T-34/76, KV-1",
    "D-5T / ZIS-S-53 85 MM: T-34-85, KV-85, IS-1, SU-85",
    "D-10S 100 MM: SU-100",
    "D-25T 122 MM: IS-2, IS-3",
    "M-10T 152 MM: KV-2",
    "ML-20S 152 MM: ISU-152, SU-152",
    "KWK 36 88 MM: TIGER I",
    "KWK 36 88 MM: TIGER I",
    "KWK 36 88 MM: TIGER I",
    "KWK 36 88 MM: TIGER I",
];

fn shells() -> [&'static crate::ballistics::ShellSpec; 16] {
    use crate::ballistics as b;
    [&b::BR_240_M42, &b::BR_240, &b::BR_240P, &b::BR_271_ZIS2, &b::BR_350B, &b::OF_350_F34, &b::BR_354P, &b::BR_365,
        &b::BR_412, &b::BR_471, &b::OF_530, &b::BR_540, &b::PZGR_39, &b::PZGR_40, &b::SPRGR_88, &b::PZGR_39_43]
}

/// Range (m) at which `shell` arrives at `v` m/s by its own decay law (0 at or above the muzzle).
fn range_for_velocity(shell: &crate::ballistics::ShellSpec, v: f32) -> f32 {
    if v >= shell.muzzle_velocity || shell.drag_k <= 0.0 {
        return 0.0;
    }
    ((shell.muzzle_velocity / v.max(1.0)).ln() / shell.drag_k).clamp(0.0, 6_000.0)
}

/// What the shot is aimed at.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    /// A point picked on the tank (hull frame).
    Point(Vec3),
    /// A crewman's body where he is at the moment of firing.
    Man(CrewRole),
}

#[derive(Clone, Copy, Debug)]
struct ShotSetup {
    shell: usize,
    range_m: f32,
    target: Target,
    /// Where the round comes from, hull frame: bearing (0 = from ahead, +90 = from the left, the
    /// hull's +X) and elevation (+ = from above), degrees.
    from_az: f32,
    from_el: f32,
    seed: u32,
}

impl ShotSetup {
    fn spec(&self) -> &'static crate::ballistics::ShellSpec {
        shells()[self.shell % shells().len()]
    }
    /// The round's travel direction, hull frame.
    fn dir_hull(&self) -> Vec3 {
        let (az, el) = (self.from_az.to_radians(), self.from_el.to_radians());
        -Vec3::new(el.cos() * az.sin(), el.sin(), el.cos() * az.cos())
    }
    fn set_dir_hull(&mut self, d: Vec3) {
        let from = -d.normalize_or_zero();
        self.from_az = from.x.atan2(from.z).to_degrees();
        self.from_el = from.y.clamp(-1.0, 1.0).asin().to_degrees();
    }
    fn impact_velocity(&self) -> f32 {
        self.spec().velocity_at(self.range_m)
    }
}

#[derive(Clone, Copy, Debug)]
struct SceneSetup {
    racks_loaded: bool,
    cabin_lamps: bool,
    fresh_each_run: bool,
    slow_s: f32,
    run_s: f32,
}

#[derive(Clone, Copy, Debug)]
struct Section {
    on: bool,
    /// Normal into the removed half, hull frame, as yaw (about +Y from +Z) and pitch.
    yaw: f32,
    pitch: f32,
    /// Offset of the plane from `SECTION_PIVOT` along its normal (m).
    offset: f32,
}

impl Section {
    /// The plane's normal in the hull frame: always one of the hull's axes, either side (the owner,
    /// 2026-11-03: "no more oblique sections only ortho"); any yaw and pitch fall to the nearest.
    fn normal_hull(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        ortho_axis(Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos()))
    }
    fn set_normal_hull(&mut self, n: Vec3) {
        let n = ortho_axis(n);
        self.yaw = if n.y.abs() > 0.5 { 0.0 } else { n.x.atan2(n.z).to_degrees() };
        self.pitch = n.y.clamp(-1.0, 1.0).asin().to_degrees();
    }
    /// Its yaw and pitch put on the axis they lie nearest (a saved oblique plane, a turned dial).
    fn snap(&mut self) {
        self.set_normal_hull(self.normal_hull());
    }
}

/// The hull axis (signed) nearest `n`.
fn ortho_axis(n: Vec3) -> Vec3 {
    let a = n.abs();
    if a.x >= a.y && a.x >= a.z {
        Vec3::X * n.x.signum()
    } else if a.y >= a.z {
        Vec3::Y * n.y.signum()
    } else {
        Vec3::Z * if n.z < 0.0 { -1.0 } else { 1.0 }
    }
}

#[derive(Clone, Copy, Debug)]
struct Sky {
    /// Days after 1942-12-21.
    day: i32,
    /// Local time, hours.
    local_h: f32,
    weather: f32,
    /// None: the eye's own automatic exposure; Some: held at this exposure.
    exposure: Option<f32>,
    /// The sky's own clock: it runs on the wall clock at `rate` (sky seconds a second) while
    /// `running`, apart from the simulation (pausing, stepping, resets and seeks leave it be).
    running: bool,
    rate: f32,
}

impl Sky {
    fn instant(&self) -> ephemeris::UtcInstant {
        ephemeris::UtcInstant::from_calendar(1942, 12, 21, 0, 0, 0.0)
            .plus_seconds(self.day as f64 * 86_400.0 + (self.local_h as f64 - LOCAL_UTC_OFFSET_H) * 3_600.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// The present state, the camera free (always, but while the timeline is dragged).
    Live,
    /// The timeline dragged: showing frame `i`'s stored preview until it is let go.
    Scrub(usize),
}

/// The run the timeline is of: its schedule (the scene's slow span and length) and its length in
/// frames.
#[derive(Clone, Copy, Debug)]
struct RunInfo {
    scene: SceneSetup,
    total: u32,
}

/// What the simulation has been given since the last fresh Tiger: a backward seek builds a fresh
/// Tiger the same way and gives it the same again, up to the frame wanted.
#[derive(Clone, Copy, Debug)]
enum Tape {
    Step(f32),
    Fire(ShotSetup, crew_tissue::TissueTuning),
}

/// Every adjustable row of the sidebar.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Knob {
    Shell, Gun, Range, Velocity, FromAz, FromEl, TargetKind, Seed, SavedShot, AddShot, CacheSim, RenderShot,
    Racks, Lamps, LampPower, Engine, Fresh, SlowSpan, RunLength, TimeSpeed, Loop,
    MovingCamera, Wear(usize), SnowTank, SnowWorld, MaterialsGame, SmokeDensity, SmokeDelay, EffectsAuthored, Concussion, FlashAfterimage,
    Focal, SavedCam, AddCam, DeleteCam, ExportCams,
    SectionOn, SectionPreset, SnapToShot, SectionYaw, SectionPitch, SectionOffset,
    Day, Time, SkyRunning, SkyRate, Weather, AutoExposure, Exposure,
    Look, LookEv, Saturation, Warmth, Gamma, DitherPattern, PaletteLevels, OrderedDither, LdrDither, Retro,
    WhiteBalance, WhiteBalanceStrength, Fog, LookDefaults,
    Tear(usize), Stretch(usize), Forward, Outward, Reach, Spread, Landing, TissueAuthored,
}

/// How a row is worked: a stepper (< >) for discrete choices, a toggle, a knob for continuous
/// values, an action, or only shown.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Info,
    Stepper,
    Toggle,
    Dial,
    Action,
}

fn kind(knob: Knob) -> Kind {
    use Knob::*;
    match knob {
        Gun => Kind::Info,
        Shell | TargetKind | Seed | SavedShot | SavedCam | SectionPreset | Day | Look | DitherPattern => Kind::Stepper,
        Racks | Lamps | Engine | Fresh | Loop | MovingCamera | SectionOn | SkyRunning | AutoExposure | WhiteBalance
        | Concussion | FlashAfterimage => Kind::Toggle,
        CacheSim | RenderShot | AddShot | AddCam | DeleteCam | ExportCams | EffectsAuthored | SnapToShot | LookDefaults | MaterialsGame | TissueAuthored => Kind::Action,
        _ => Kind::Dial,
    }
}

/// A dial's range and scale (logarithmic or linear).
fn dial_range(knob: Knob) -> (f32, f32, bool) {
    use Knob::*;
    match knob {
        Range => (0.0, 3_000.0, false),
        Velocity => (150.0, 1_200.0, false),
        FromAz => (-180.0, 180.0, false),
        FromEl => (-60.0, 80.0, false),
        LampPower => (1.0, 1.0e6, true),
        SlowSpan => (0.0, 1.0, false),
        RunLength => (0.1, 20.0, true),
        TimeSpeed => (0.01, 4.0, true),
        Focal => (12.0, 400.0, true),
        SectionYaw => (-180.0, 180.0, false),
        SectionPitch => (-90.0, 90.0, false),
        SectionOffset => (-3.0, 3.0, false),
        Time => (0.0, 24.0, false),
        SkyRate => (1.0, 3_600.0, true),
        Weather => (0.0, 1.0, false),
        Exposure => (1.0, 1.0e6, true),
        LookEv => (1.0 / 16.0, 16.0, true),
        Saturation => (0.0, 2.0, false),
        Warmth => (-1.0, 1.0, false),
        Gamma => (0.5, 2.0, false),
        PaletteLevels => (2.0, 255.0, true),
        OrderedDither | LdrDither | Retro | WhiteBalanceStrength => (0.0, 1.0, false),
        Fog => (0.0001, 0.25, true),
        Wear(_) | SnowTank | SnowWorld => (0.0, 1.0, false),
        SmokeDensity => (0.0, 4.0, false),
        SmokeDelay => (0.0, 2.0, false),
        Tear(_) => (1.0e3, 1.0e8, true),
        Stretch(_) => (0.005, 5.0, true),
        Forward | Outward => (0.0, 2.0, false),
        Reach => (0.01, 0.6, true),
        Spread => (0.25, 8.0, true),
        Landing => (0.01, 1.0e4, true),
        _ => (0.0, 1.0, false),
    }
}

/// The sidebar's disclosable groups.
const GROUPS: [&str; 9] = ["SHOT", "SCENE", "CAMERA", "SECTION", "SKY", "LOOK", "MATERIALS", "EFFECTS", "TISSUE (REVIEW)"];

fn group_rows(group: usize) -> Vec<Knob> {
    use Knob::*;
    match group {
        0 => vec![Shell, Gun, Range, Velocity, FromAz, FromEl, TargetKind, Seed, SavedShot, AddShot],
        1 => vec![TimeSpeed, Loop, Racks, Lamps, LampPower, Engine, SlowSpan, RunLength],
        2 => vec![Focal, SavedCam, AddCam, DeleteCam, ExportCams],
        3 => vec![SectionOn, SectionPreset, SnapToShot, SectionYaw, SectionPitch, SectionOffset],
        4 => vec![Day, Time, SkyRunning, SkyRate, Weather, AutoExposure, Exposure],
        5 => vec![Look, LookEv, Saturation, Warmth, Gamma, DitherPattern, PaletteLevels, OrderedDither, LdrDither, Retro,
            WhiteBalance, WhiteBalanceStrength, Fog, LookDefaults],
        6 => vec![Wear(1), Wear(0), Wear(2), Wear(3), SnowTank, SnowWorld, MaterialsGame],
        7 => vec![SmokeDensity, SmokeDelay, Concussion, FlashAfterimage, EffectsAuthored],
        _ => {
            let mut rows: Vec<Knob> = (0..4).map(Tear).collect();
            rows.extend((0..4).map(Stretch));
            rows.extend([Forward, Outward, Reach, Spread, Landing, TissueAuthored]);
            rows
        }
    }
}

const TISSUE_NAMES: [&str; 4] = ["SKIN", "MUSCLE", "BONE", "FLUID"];
/// The world editor's display looks (`editor::RENDER_LOOKS`).
const LOOKS: [(f32, &str); 7] = [(0.0, "STANDARD"), (6.0, "DELTAP REAL"), (1.0, "DELTAP"), (5.0, "DELTAP TINTED"),
    (4.0, "PASTEL"), (2.0, "PICO-8"), (3.0, "DELTAP RAW")];
const DITHERS: [&str; 4] = ["SCREEN BAYER4", "BLUE NOISE", "SPHERE BAYER8", "SCREEN BAYER8"];

// The panel: a bar along the bottom above the timeline (window fractions, top-left origin), the
// viewport the whole window (the owner, 2026-11-03: "disclosable bottom bar"). Its upper line the
// status, the time and HIDE; its lower line the mode tabs, the buttons and the groups' headers,
// each a disclosure (closed > / open v) whose rows open upward above it, several at once.
const BAR_H: f32 = 0.056;
const BAR_TOP: f32 = 1.0 - 0.008 - BAR_H;
/// The bar's lower line (mode tabs, buttons, headers): its top and height.
const BAR_ROW_Y: f32 = BAR_TOP + 0.028;
const BAR_ROW_H: f32 = 0.024;
/// The buttons' left edge, their width and the last (wide) one's.
const BUTTONS_X: f32 = 0.150;
const BUTTON_W: f32 = 0.040;
const BUTTON_WIDE: f32 = 0.066;
/// The group headers' left edge.
const HEADERS_X: f32 = BUTTONS_X + 6.0 * (BUTTON_W + 0.003) + BUTTON_WIDE + 0.012;
/// The timeline line's thickness (px) and the clip's colour (Blender's orange, linear).
const TL_PX: f32 = 5.0;
const AMBER: [f32; 4] = [0.807, 0.205, 0.006, 1.0];
const RED: [f32; 4] = [0.95, 0.22, 0.18, 1.0];
const GREEN: [f32; 4] = [0.35, 0.9, 0.3, 1.0];
const BLUE: [f32; 4] = [0.3, 0.5, 1.0, 1.0];
/// Black.
const PANEL_FILL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const INK: [f32; 4] = [0.92, 0.92, 0.92, 1.0];
const DIM: [f32; 4] = [0.42, 0.42, 0.42, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const BOX: [f32; 4] = [0.035, 0.035, 0.035, 1.0];
const ROW_H: f32 = 0.0185;
const TEXT: f32 = 0.0145;
/// The bar's left margin.
const LEFT_X: f32 = 0.007;
const BUTTONS: [(&str, usize, usize); 5] = [("RESET", 0, 0), ("FIRE", 1, 0), ("PLAY", 2, 0),
    ("REC", 0, 1), ("STILL", 1, 1)];
/// BATTLE mode's buttons: WATCH (both sides' AI) and PLAY (the player takes the Tiger), SAVE and
/// the saved layouts (`BATTLE_LAYOUTS`).
const BATTLE_BUTTONS: [(&str, usize, usize); 7] = [("RESET", 0, 0), ("WATCH", 1, 0), ("PLAY", 2, 0), ("SAVE", 0, 1), ("REC", 1, 1),
    ("STILL", 2, 1), ("LAYOUTS", 0, 2)];
/// The editor's modes, as tabs at the panel's top left. WORLD (placing objects from the asset
/// browser, the old `edit` worldbuilder's replacement) is promised and not built: greyed out.
const MODES: [&str; 3] = ["RANGE", "BATTLE", "WORLD"];

/// An open group's column of rows: its width.
const COLS_W: f32 = 0.166;
const BTN_W: f32 = 0.011;
/// The panel's hide button (its show button sits in the same corner when it is hidden).
const HIDE_W: f32 = 0.045;
/// A dial turns through its whole range over this much vertical drag (window height).
const DIAL_DRAG: f32 = 0.3;
/// The shot handle's distance from the aim point (m).
const SHOT_HANDLE_M: f32 = 0.6;
// The section's rotation handle: box size and the arrows' distance from its pad (window height).
const HANDLE_BOX: f32 = 0.016;
const HANDLE_REACH: f32 = 0.022;
/// The handles show their arrows (and the shot handle its caption) only with the cursor this near.
const HANDLE_NEAR: f32 = 0.06;
/// Screen distance of a centre-pad drag per 45° step.
const HANDLE_DRAG_STEP: f32 = 0.04;

/// A group's name on its header: its first word ("TISSUE (REVIEW)" is TISSUE).
fn header_name(name: &str) -> &str {
    name.split(" (").next().unwrap_or(name)
}

/// A button's rectangle (x, y, w, h) on the bar's lower line: in order, the last one wide.
fn button_rect(col: usize, row: usize) -> [f32; 4] {
    let index = row * 3 + col;
    let x = BUTTONS_X + index as f32 * (BUTTON_W + 0.003);
    [x, BAR_ROW_Y, if index == 6 { BUTTON_WIDE } else { BUTTON_W }, BAR_ROW_H]
}

/// The panel's hide button (the bar's upper line, right), or, hidden, its show button (the
/// window's bottom right).
fn hide_rect(hidden: bool) -> [f32; 4] {
    if hidden { [1.0 - HIDE_W - 0.006, 1.0 - 0.034, HIDE_W, 0.026] } else { [1.0 - HIDE_W - 0.006, BAR_TOP + 0.003, HIDE_W, 0.020] }
}



/// What a press in the sidebar did.
enum Click {
    /// It was in the viewport, not the sidebar.
    Viewport,
    Used,
    /// It took hold of a dial (dragged up and down to turn it).
    Dial(Knob),
}

/// The dial face's size (px) and its image: a ring.
const DIAL_FACE: u32 = 32;

fn dial_face() -> Vec<u8> {
    let n = DIAL_FACE as i32;
    let mut out = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let r = ((x as f32 + 0.5 - n as f32 * 0.5).powi(2) + (y as f32 + 0.5 - n as f32 * 0.5).powi(2)).sqrt() / (n as f32 * 0.5);
            let a = if (0.80..=0.98).contains(&r) { 255 } else if r < 0.80 { 40 } else { 0 };
            out.extend_from_slice(&[255, 255, 255, a]);
        }
    }
    out
}

/// REC: the viewport's frames (no interface) piped to ffmpeg at 30 fps, scaled up by a whole factor
/// by nearest neighbour (the internal grid's pixels kept; 4x of 1280x720 is the Studio Display's
/// 5120x2880).
struct Recorder {
    path: String,
    size: (u32, u32),
    encoder: std::process::Child,
    frames: u64,
}

/// The whole scale factor that brings the internal grid's height to 2880 (at least 1).
fn display_scale(ih: u32) -> u32 {
    (2880 / ih.max(1)).max(1)
}

fn capture_stamp() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Game {
    /// The FEA range's battle: the placed T-34s in the act's enemy slots (as a campaign contact
    /// streams in, `campaign`'s wave code), live and fighting with their own AI; every other slot
    /// empty and far away. Returns how many were placed (the pool's size bounds it).
    fn range_stage_battle(&mut self, t34s: &[(Vec2, f32)]) -> usize {
        let n = t34s.len().min(self.enemies.len());
        for i in 0..self.enemies.len() {
            self.enemy_active[i] = false;
            self.enemy_skill[i] = 0;
            self.enemy_concealed[i] = false;
            self.enemies[i].alive = false;
            self.enemies[i].speed = 0.0;
            self.enemies[i].pos = Vec3::new(3000.0 + 40.0 * i as f32, 0.0, 3000.0);
            self.wrecks[i] = Wreck::default();
            self.enemy_crew.clear_tank(i);
        }
        for (i, &(p, heading)) in t34s.iter().take(n).enumerate() {
            let mut enemy = Tank::new(p.x, p.y, heading, &self.hf, tank::T34_PARTS);
            enemy.kind = EnemyKind::T34;
            enemy.reload = ENEMY_RELOAD * (i as f32 + 0.7) / n.max(1) as f32;
            self.enemies[i] = enemy;
            self.enemy_active[i] = true;
            self.enemy_skill[i] = 42;
        }
        self.stealth = stealth::Stealth::new(self.enemies.len());
        self.projectiles.retain(|p| matches!(p.shooter, ProjectileShooter::Player));
        self.targets_only = false;
        n
    }
}

/// The game's effects of a crewman's eyes, which the range shows only when switched on (EFFECTS):
/// the concussion's defocus (`crush.wgsl`: four offset copies of the frame round each pixel) and
/// the eye's flash afterimage (`observer_flash_source`). The eye's local adaptation is off in the
/// game itself (`observer::LOCAL_ADAPTATION_STRENGTH` 0, 2026-11-03), so it has no switch here.
#[derive(Clone, Copy, Debug)]
struct EyeFx {
    concussion: bool,
    flash: bool,
}

/// A battle layout for level design (`BATTLE_LAYOUTS`): the Tiger's start, the T-34s and the
/// Tiger's route.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct BattleLayout {
    name: String,
    tiger_at: [f32; 2],
    tiger_heading: f32,
    t34s: Vec<[f32; 3]>,
    route: Vec<[f32; 2]>,
}

const BATTLE_LAYOUTS: &str = "catalog/fea_range_battle_layouts.v1.json";

/// A saved camera (`target/fea_range_cameras.json`; the shared set in `CAMERA_CATALOG`): its name,
/// its place and view in the Tiger's hull frame (it follows the Tiger wherever it stands), and the
/// world pose it was saved at.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct SavedCam {
    #[serde(default)]
    name: String,
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
    focal_mm: f32,
    #[serde(default)]
    hull_eye: Option<[f32; 3]>,
    #[serde(default)]
    hull_forward: Option<[f32; 3]>,
    /// The section as it stood when the camera was saved (on or off, and its plane).
    #[serde(default)]
    section: Option<SavedSection>,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct SavedSection {
    on: bool,
    yaw: f32,
    pitch: f32,
    offset: f32,
}

const CAMERAS_PATH: &str = "target/fea_range_cameras.json";
/// The tracked camera set for development (interior and exterior views of the Tiger, hull frame).
const CAMERA_CATALOG: &str = "catalog/fea_range_cameras.v1.json";

/// A name for a camera from where it stands and looks in the hull frame (+Z forward, +X left).
fn camera_name(eye: Vec3, forward: Vec3) -> String {
    let inside = eye.x.abs() < 1.6 && eye.y > 0.3 && eye.y < 2.95 && eye.z > -3.3 && eye.z < 3.4;
    let side = |x: f32| if x > 0.3 { "left" } else if x < -0.3 { "right" } else { "centre" };
    let place = if inside {
        if eye.z < -1.15 {
            "interior engine bay".to_string()
        } else if eye.z > 1.5 {
            format!("interior hull front, {}", if eye.x > 0.2 { "driver's side" } else if eye.x < -0.2 { "radio operator's side" } else { "between the seats" })
        } else if eye.y > 1.9 {
            format!("interior turret, {}", if eye.x > 0.3 { "commander's and gunner's side" } else if eye.x < -0.3 { "loader's side" } else { "centre" })
        } else {
            format!("interior fighting compartment, {} low", side(eye.x))
        }
    } else {
        let end = if eye.z > 1.5 { "front" } else if eye.z < -1.5 { "rear" } else { "" };
        let height = if eye.y > 6.0 { "high " } else if eye.y < 1.5 { "low " } else { "" };
        format!("exterior {height}{end}{}{}", if end.is_empty() { "" } else { " " }, side(eye.x)).replace("  ", " ")
    };
    let look = if forward.y < -0.9 { "straight down" } else {
        let (ax, az) = (forward.x.abs(), forward.z.abs());
        if az >= ax { if forward.z > 0.0 { "forward" } else { "aft" } } else if forward.x > 0.0 { "to the left" } else { "to the right" }
    };
    format!("{place}, looking {look}")
}
/// The editor's settings between launches (`--shots` neither reads nor writes it).
const PRESETS_PATH: &str = "target/fea_range_presets.json";
const STATE_PATH: &str = "target/fea_range_state.json";

/// The range editor's state: the game, its renderer and everything the panel sets.
struct Range {
    device: wgpu::Device,
    queue: wgpu::Queue,
    r: Renderer,
    g: Game,
    shot: ShotSetup,
    scene: SceneSetup,
    section: Section,
    sky: Sky,
    tuning: crew_tissue::TissueTuning,
    mode: Mode,
    /// Everything given to the simulation since the fresh Tiger it started from (its seed and scene),
    /// and for each run frame how much of it had been given then.
    tape: Vec<Tape>,
    tape_base: (u32, SceneSetup, Vec2, f32),
    /// The run's checkpoints (`rewind`): a backward seek puts the state back instead of re-running.
    rewind: rewind::Rewind,
    run_tape: Vec<usize>,
    /// The engine running (off by default: no exhaust).
    engine: bool,
    /// A moving camera keeps the renderer's history (accumulated, smeared) rather than restarting
    /// it (sharp, noisier).
    accumulate_moving: bool,
    /// The present run was fired with F (into the live state): it plays on and never loops back.
    free_fire: bool,
    /// The tank's surface state held at these values (exterior grime, interior grime, glass grime,
    /// mechanical wear; snow on the tank and on the ground), or the game's own.
    wear: Option<[f32; 4]>,
    snow: Option<[f32; 2]>,
    /// The run the timeline is of, the run frame the live state stands at, and whether the
    /// simulation has gone on in real time past the run's end.
    run: Option<RunInfo>,
    frame_at: Option<u32>,
    past_run: bool,
    /// This frame's viewport is the live frame captured for the timeline's preview.
    captured: bool,
    /// Sim time since the last shot (s), as the live state stands.
    shot_t: f32,
    /// The simulation plays (always, unless paused), at `time_speed` (run frames per window frame;
    /// seconds of game time per second past the run), the clock's owed fraction carried.
    playing: bool,
    time_speed: f32,
    play_clock: f32,
    /// The clip marked on the timeline (run frames, inclusive) and whether play loops over it
    /// (or over the whole run without one).
    clip: Option<(u32, u32)>,
    looping: bool,
    // Fly camera.
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    /// Focal length of a 35 mm-format lens (24 mm frame height): the field of view.
    focal_mm: f32,
    speed: f32,
    // Display of recorded frames.
    frame_tex: wgpu::Texture,
    frame_image: u32,
    shown_frame: Option<usize>,
    status: String,
    cache: cache::Cache,
    shot_presets: Vec<serde_json::Value>,
    shot_preset_at: Option<usize>,
    help: bool,
    muted: bool,
    idle_s: f32,
    /// The last chain summary for the status line.
    last_chain: String,
    /// Where the section's rotation handle was drawn this frame (window 0..1), if it was.
    handle_at: Option<[f32; 2]>,
    /// REC in progress, and the last viewport frame captured (what it writes).
    recorder: Option<Recorder>,
    last_rgba: Option<(Vec<u8>, u32, u32)>,
    /// The interface hidden (Tab): the viewport fills the window.
    hide_ui: bool,
    /// Every dial's value as the editor started.
    dial_defaults: Vec<(Knob, f32)>,
    /// Smoothed frame time (s), simulation and render time (ms) a frame, for the readout.
    perf: [f32; 3],
    eye_fx: EyeFx,
    /// BATTLE mode: the placed T-34s (x, z, heading), the Tiger's route, the Tiger's AI crew while
    /// watching, the player at the controls (and paused), the input from the window.
    battle: bool,
    t34s: Vec<(Vec2, f32)>,
    route: Vec<Vec2>,
    pilot: Option<CanyonPilot>,
    pilot_t: f32,
    player_control: bool,
    play_paused: bool,
    play_input: TankInput,
    layout_at: Option<usize>,
    /// The orbit point set by a double click.
    orbit_pivot: Option<Vec3>,
    /// The cursor as of the last interface drawn (window 0..1).
    cursor: [f32; 2],
    /// The corner gizmo's axis labels (window 0..1), and the axis view last snapped to.
    axis_labels: [[f32; 2]; 3],
    snapped: Option<(usize, bool)>,
    /// The camera of the last frame posed (eye, yaw, pitch, focal): a moved camera restarts the
    /// renderer's history.
    last_pose: Option<(Vec3, f32, f32, f32)>,
    /// The window's size (px).
    win: [f32; 2],
    /// The viewport image's size.
    frame_size: (u32, u32),
    /// Headless stills: the live frame is shown through the viewport image.
    live_image: bool,
    /// Where the Tiger stands (x, z) and its heading (rad).
    tiger_at: Vec2,
    tiger_heading: f32,
    /// The groups open on the panel (each in its own columns, left to right, as far as they fit).
    open: [bool; GROUPS.len()],
    cameras: Vec<SavedCam>,
    camera_at: Option<usize>,
    /// The fog density set by the LOOK group's dial (the physical medium), held each frame.
    fog: Option<f32>,
    /// A dial's face (a ring, drawn small).
    dial_image: u32,
    /// The display look as the renderer started (LOOK DEFAULTS restores it).
    look_start: (f32, [f32; 4], f32, Option<crate::renderer::display_tuning::DisplayTuning>, f32),
}

fn fmt_kj(v: f32) -> String {
    if v >= 1.0e6 { format!("{:.2} MJ/M3", v / 1.0e6) } else { format!("{:.1} KJ/M3", v / 1.0e3) }
}

impl Range {
    fn new(device: wgpu::Device, queue: wgpu::Queue, headless: bool) -> anyhow::Result<Self> {
        // The Canyon run's own course (`canyon::manifest_path`, as `canyon_game` selects it).
        act_manifest::select_runtime_manifest(Some(crate::canyon::manifest_path()))?;
        let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
        let frame_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fea-range-frame"),
            size: wgpu::Extent3d { width: r.iw, height: r.ih, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let frame_image = r.import_diagnostic_image(&device, &frame_tex.create_view(&wgpu::TextureViewDescriptor::default()));
        // The review scene as `fea-play` and `fea-film` set it: enemies away and holding fire, the
        // Tiger at the review spot on the snow field.
        g.debug_banish_enemies();
        g.debug_hold_enemy_fire();
        g.targets_only = true;
        g.god = false;
        g.debug_place_tiger(TIGER_AT, TIGER_HEADING);
        let sky = Sky { day: 0, local_h: 13.5, weather: 0.0, exposure: None, running: true, rate: 60.0 };
        let mut range = Self {
            device, queue, r, g,
            shot: ShotSetup { shell: 3, range_m: 400.0, target: Target::Man(CrewRole::Loader), from_az: -90.0, from_el: 0.0,
                seed: 0x5EC7_A000 },
            scene: SceneSetup { racks_loaded: false, cabin_lamps: true, fresh_each_run: true, slow_s: 0.25, run_s: 4.0 },
            section: Section { on: false, yaw: 0.0, pitch: 90.0, offset: 0.5 },
            sky,
            tuning: crew_tissue::TissueTuning::AUTHORED,
            mode: Mode::Live,
            tape: Vec::new(),
            rewind: rewind::Rewind::default(),
            tape_base: (0, SceneSetup { racks_loaded: false, cabin_lamps: true, fresh_each_run: true, slow_s: 0.25, run_s: 4.0 },
                TIGER_AT, TIGER_HEADING),
            run_tape: Vec::new(),
            engine: false,
            accumulate_moving: false,
            free_fire: false,
            wear: None,
            snow: None,
            run: None,
            frame_at: None,
            past_run: false,
            captured: false,
            shot_t: 0.0,
            playing: false,
            time_speed: 1.0,
            play_clock: 0.0,
            clip: None,
            looping: true,
            eye: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            focal_mm: 35.0,
            speed: 1.5,
            frame_tex,
            frame_image,
            shown_frame: None,
            status: "RESET: a fresh Tiger and crew".into(),
            cache: cache::Cache::default(),
            shot_presets: std::fs::read_to_string(PRESETS_PATH).ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v["presets"].as_array().cloned()).unwrap_or_default(),
            shot_preset_at: None,
            help: false,
            muted: false,
            idle_s: 0.0,
            last_chain: String::new(),
            handle_at: None,
            axis_labels: [[-1.0, -1.0]; 3],
            cursor: [0.5, 0.5],
            orbit_pivot: None,
            eye_fx: EyeFx { concussion: false, flash: false },
            battle: false,
            t34s: Vec::new(),
            route: Vec::new(),
            pilot: None,
            pilot_t: 0.0,
            player_control: false,
            play_paused: false,
            play_input: TankInput::default(),
            layout_at: None,
            perf: [1.0 / 60.0, 0.0, 0.0],
            hide_ui: false,
            recorder: None,
            last_rgba: None,
            dial_defaults: Vec::new(),
            snapped: None,
            last_pose: None,
            win: [1920.0, 1080.0],
            frame_size: (0, 0),
            live_image: false,
            tiger_at: TIGER_AT,
            tiger_heading: TIGER_HEADING,
            open: [true, false, false, false, false, false, false, false, false],
            cameras: std::fs::read_to_string(CAMERAS_PATH).ok().and_then(|t| serde_json::from_str(&t).ok())
                .or_else(|| std::fs::read_to_string(CAMERA_CATALOG).ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .and_then(|v| serde_json::from_value(v["cameras"].clone()).ok()))
                .unwrap_or_default(),
            camera_at: None,
            dial_image: 0,
            fog: None,
            look_start: (0.0, [1.0, 1.0, 0.0, 1.0], 0.0, None, 0.0),
        };
        range.dial_image = range.r.upload_image(&range.device, &range.queue, &dial_face(), DIAL_FACE, DIAL_FACE);
        range.look_start = (range.r.look, range.r.display_grade, range.r.display_dither_pattern, range.r.display_tuning,
            range.r.fog_density);
        range.dial_defaults = range.dial_values();
        let saved = !headless && std::path::Path::new(STATE_PATH).exists();
        if saved {
            range.load_state();
        }
        range.apply_sky();
        range.fresh();
        if !saved {
            // The first camera: outside, off the right front, the Tiger in the middle.
            let hull = range.g.player.hull_model();
            let eye = hull.transform_point3(Vec3::new(-7.5, 3.4, 6.5));
            range.look_at(eye, hull.transform_point3(Vec3::new(0.0, 1.4, 0.0)));
        }
        // Explicit live launch: start the wall-clock simulation without firing before the owner is ready.
        if !headless {
            range.free_fire = true;
            range.playing = true;
            range.time_speed = 1.0;
            range.status = "LIVE PLAYGROUND: F FIRES; SPACE PAUSES; BACKSPACE RESETS; REC SAVES VIDEO".into();
        }
        Ok(range)
    }

    fn look_at(&mut self, eye: Vec3, at: Vec3) {
        let mut c = crate::camera::Camera::new(1.0);
        c.set_forward(at - eye);
        self.eye = eye;
        self.yaw = c.yaw;
        self.pitch = c.pitch;
    }

    /// The vertical field of view (degrees) of the focal length on a 35 mm frame (24 mm high).
    fn fov_deg(&self) -> f32 {
        (2.0 * (12.0 / self.focal_mm).atan()).to_degrees()
    }

    fn forward(&self) -> Vec3 {
        let mut c = crate::camera::Camera::new(1.0);
        c.yaw = self.yaw;
        c.pitch = self.pitch;
        c.forward()
    }

    fn apply_sky(&mut self) {
        let clock = celestial_sky::SkyClock::new(self.sky.instant(), self.r.sky_clock.site);
        self.r.set_sky_clock(clock);
        self.r.set_weather_override(Some(self.sky.weather));
        self.r.exposure_override = self.sky.exposure;
        if let Some(fog) = self.fog {
            self.r.fog_density = fog;
        }
    }

    fn apply_tuning(&self) {
        // Review-only: the override is set only while it differs from the authored constants.
        if self.tuning == crew_tissue::TissueTuning::AUTHORED {
            crew_tissue::set_review_tuning(None);
        } else {
            crew_tissue::set_review_tuning(Some(self.tuning));
        }
    }

    fn step(&mut self, dt: f32) {
        // The battle's driver: the Tiger's AI crew while watching, the player's keys while playing.
        let input = if let Some(mut pilot) = self.pilot.take() {
            let input = pilot.step(&mut self.g, &self.r, self.pilot_t, dt);
            self.pilot_t += dt;
            self.pilot = Some(pilot);
            input
        } else if self.player_control && !self.play_paused {
            self.play_input.clone()
        } else {
            TankInput::default()
        };
        self.g.engine_on = self.engine || self.pilot.is_some() || self.player_control;
        // The game reads the renderer's camera as its eye and ears (the sound's listener, the
        // head's sway, a hit's camera kick): every step starts from the editor's camera, so a run
        // repeats whether or not its frames were drawn.
        self.place_camera();
        self.g.update(&input, dt, &mut self.r);
        self.tape.push(Tape::Step(dt));
    }

    /// A fresh Tiger and crew at the review spot (`game_fea_film::fresh_tiger`), settled for a
    /// second of game time.
    ///
    /// The settled Tiger is kept (`rewind`) and put back for the same setup: `reset_match` leaves
    /// some of the game's clocks and counters running on (the projectile serial, the armour clock,
    /// the infantry), so a Tiger built again does not start a strike from the same state.
    fn fresh(&mut self) {
        // Every reset and run rebuilds the Tiger through the match reset. The kept checkpoint is the
        // world save's state, which holds no shells in flight, cook-off or thrown turret, so putting
        // it back left the last run's shell flying and its damage in place (owner, 2026-11-03:
        // "reset is still autoplaying and also firing the shell from previous run").
        self.rewind.fresh = None;
        let key = format!("{:?}", (self.shot.seed, self.scene.racks_loaded, self.scene.cabin_lamps, self.tiger_at,
            self.tiger_heading, self.engine, self.muted));
        let kept = self.rewind.fresh.as_ref().filter(|(k, _)| *k == key).map(|(_, c)| c.clone());
        let restored = kept.is_some_and(|c| match c.restore(&mut self.g, &mut self.r) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("[fea-range] the kept fresh Tiger could not be put back ({e:#}); building it again");
                false
            }
        });
        // Weapon smoke, flashes and the dust and snow puffs are presentation the checkpoint does not
        // keep (`NOT_KEPT`), so a reset showed the last run's smoke drifting on (owner, 2026-11-03).
        self.g.weapon_vfx = Default::default();
        for puff in self.g.snow_puffs.iter_mut() { puff.t = 0.0; }
        // The renderer's own volumetric plumes stay lit until set again: cleared too.
        self.r.clear_smoke();
        self.g.fx.smoke_t = 0.0;
        if restored {
            // The checkpoint is the world save's state, which keeps no cook-off: a turret thrown in
            // an earlier run stayed thrown through every reset. A fresh Tiger has none.
            self.g.cookoff.reset();
            self.g.player.turret_thrown = None;
        }
        if !restored {
            self.build_fresh();
            // Kept, and put back at once: the first run starts as every later one does.
            match rewind::Checkpoint::capture(&self.g, &self.r, None) {
                Ok((c, _)) => {
                    if let Err(e) = c.restore(&mut self.g, &mut self.r) {
                        eprintln!("[fea-range] the fresh Tiger could not be put back: {e:#}");
                    }
                    self.rewind.fresh = Some((key, c));
                }
                Err(e) => eprintln!("[fea-range] the fresh Tiger is not kept: {e:#}"),
            }
        }
        // BATTLE: the placed T-34s staged on the kept Tiger's world (as placed now, every time).
        if self.battle {
            let placed = self.g.range_stage_battle(&self.t34s);
            if placed < self.t34s.len() {
                self.status = format!("{} T-34S PLACED: THE ACT'S POOL HOLDS {placed}", self.t34s.len());
            }
        }
        self.shot_t = 0.0;
        self.last_chain.clear();
        self.tape.clear();
        self.tape_base = (self.shot.seed, self.scene, self.tiger_at, self.tiger_heading);
    }

    /// Build the fresh Tiger: the match reset, the place, a second's settling.
    fn build_fresh(&mut self) {
        self.g.enable_singleplayer_crew();
        self.g.reset_match(self.shot.seed, 0.0);
        self.g.dbg_reset_foot();
        self.g.player_burn.extinguish();
        self.g.debug_banish_enemies();
        self.g.debug_hold_enemy_fire();
        self.g.targets_only = true;
        self.g.god = false;
        self.g.debug_place_tiger(self.tiger_at, self.tiger_heading);
        if !self.scene.racks_loaded {
            self.g.debug_empty_ammunition();
        }
        self.g.set_cabin_lights(self.scene.cabin_lamps);
        self.g.jump_station(Station::Commander);
        if self.muted {
            self.g.mute_audio();
        }
        for _ in 0..60 {
            self.step(TICK);
        }
    }

    /// The shot's aim point and travel direction (world).
    fn aim_line(&self) -> (Vec3, Vec3) {
        let hull = self.g.player.hull_model();
        let dir = hull.transform_vector3(self.shot.dir_hull()).normalize_or_zero();
        let aim = match self.shot.target {
            Target::Point(p) => hull.transform_point3(p),
            Target::Man(role) => self.g.crew_solo_man_point(role),
        };
        (aim, dir)
    }

    /// The first vehicle surface on the shot's line coming in from 40 m out (the round's entry),
    /// and its normal: the round is launched 9 m short of it, so it meets everything on its line.
    fn entry(&self) -> Option<(Vec3, Vec3)> {
        let (aim, dir) = self.aim_line();
        let models = self.g.fire_surface_models();
        let from = aim - dir * 40.0;
        let to = aim + dir * 0.02;
        let contact = self.g.cabin_fire.sweep(from, to, &models)?;
        let normal = models[contact.frame].transform_vector3(contact.normal).normalize_or_zero();
        Some((from.lerp(to, contact.fraction), normal))
    }

    fn fire(&mut self) {
        let (aim, dir) = self.aim_line();
        let target = self.entry().map_or(aim, |(p, _)| p);
        let shell = self.shot.spec();
        self.g.debug_launch_round(shell, false, target, dir, self.shot.range_m, self.shot.seed);
        self.shot_t = 0.0;
        self.tape.push(Tape::Fire(self.shot, self.tuning));
    }

    fn slow_frames(&self, scene: &SceneSetup) -> u32 {
        (scene.slow_s / SLOW_DT).round() as u32
    }

    fn total_frames(&self, scene: &SceneSetup) -> u32 {
        let slow = self.slow_frames(scene);
        let rest = ((scene.run_s - slow as f32 * SLOW_DT).max(0.0) * 30.0).round() as u32;
        slow + rest
    }

    /// Advance the run's schedule by its frame `k`: one fragment step in the slow span, two ticks
    /// after it.
    fn advance_frame(&mut self, k: u32, scene: &SceneSetup) {
        if k < self.slow_frames(scene) {
            self.step(SLOW_DT);
            self.shot_t += SLOW_DT;
        } else {
            for _ in 0..REAL_TICKS_PER_FRAME {
                self.step(TICK);
            }
            self.shot_t += TICK * REAL_TICKS_PER_FRAME as f32;
        }
    }

    fn reset(&mut self) {
        self.clear_caches();
        self.mode = Mode::Live;
        self.fresh();
        self.run = None;
        self.frame_at = None;
        self.past_run = false;
        self.run_tape.clear();
        self.shown_frame = None;
        self.rewind.clear_run();
        // Held at the fresh Tiger (the owner, 2026-11-03: "reset is playing not reseting": with no
        // run left the tick played on in real time from it).
        self.playing = false;
        self.status = "RESET: A FRESH TIGER AND CREW (PAUSED; SPACE PLAYS)".into();
    }

    /// RUN: the shot on a fresh Tiger (or, with FRESH EACH RUN off, into the present state).
    fn run(&mut self) {
        self.free_fire = false;
        if self.scene.fresh_each_run {
            self.fresh();
        }
        self.fire_now();
    }

    /// F: the shot fired into the present state at once; its run starts on the timeline.
    fn fire_now(&mut self) {
        self.apply_tuning();
        self.shown_frame = None;
        let total = self.total_frames(&self.scene);
        self.run = Some(RunInfo { scene: self.scene, total });
        self.run_tape = vec![usize::MAX; total as usize];
        self.frame_at = None;
        self.past_run = false;
        self.fire();
        self.rewind.start_run(&self.g, &self.r, total, self.tape.len());
        self.mode = Mode::Live;
        self.playing = true;
        self.status = format!("FIRED: {} at {:.0} m ({:.0} m/s)", self.shot.spec().name, self.shot.range_m, self.shot.impact_velocity());
    }

    /// Time after the shot at the end of run frame `k` (the run's schedule).
    fn frame_time(&self, k: u32) -> f32 {
        let scene = self.run.map_or(self.scene, |r| r.scene);
        let slow = self.slow_frames(&scene);
        if k < slow { (k + 1) as f32 * SLOW_DT } else { slow as f32 * SLOW_DT + (k + 1 - slow) as f32 * TICK * REAL_TICKS_PER_FRAME as f32 }
    }

    /// The run frame the live simulation steps to next, if the run has one left.
    fn next_run_frame(&self) -> Option<u32> {
        let run = self.run?;
        let next = self.frame_at.map_or(0, |f| f + 1);
        (!self.past_run && next < run.total).then_some(next)
    }

    /// Step the run's next frame.
    fn run_frame(&mut self, k: u32) {
        let Some(run) = self.run else { return };
        let started = std::time::Instant::now();
        self.advance_frame(k, &run.scene);
        let step_s = started.elapsed().as_secs_f32();
        self.frame_at = Some(k);
        if let Some(slot) = self.run_tape.get_mut(k as usize) {
            *slot = self.tape.len();
        }
        self.rewind.keep(&self.g, &self.r, k, self.shot_t, self.tape.len(), step_s);
        if k + 1 == run.total {
            self.status = format!("RUN DONE at {:.2} s: the simulation plays on in real time (SPACE pauses)", self.shot_t);
        }
    }

    /// The live state at run frame `index`: on from the present frame, or (backward) the latest
    /// checkpoint before it put back (`rewind`) and the frames from it stepped again, at least the
    /// frame itself (its picture is then the update's own), the result compared field by field
    /// with the frame's own checkpoint. Without a checkpoint, a fresh Tiger given the same steps and
    /// shots again (the tape). The camera is the editor's and is never moved by it.
    fn seek(&mut self, index: u32) {
        if self.cached_seek(index as usize) { return; }
        self.mode = Mode::Live;
        let Some(run) = self.run else { return };
        let index = index.min(run.total.saturating_sub(1));
        let current = self.frame_at.filter(|_| !self.past_run);
        if current == Some(index) {
            return;
        }
        if current.is_some_and(|c| index > c) {
            for k in current.unwrap_or(0) + 1..=index {
                self.run_frame(k);
            }
            self.status = format!("ON TO {:.4} S", self.shot_t);
            return;
        }
        if self.seek_kept(index) {
            return;
        }
        let Some(&upto) = self.run_tape.get(index as usize).filter(|&&n| n != usize::MAX) else {
            // Not reached yet in this run: step on from its start instead.
            return;
        };
        let started = std::time::Instant::now();
        let tape = std::mem::take(&mut self.tape);
        let (keep_shot, keep_scene, keep_tuning, keep_at, keep_heading) =
            (self.shot, self.scene, self.tuning, self.tiger_at, self.tiger_heading);
        let (seed, scene, at, heading) = self.tape_base;
        self.shot.seed = seed;
        self.scene = scene;
        self.tiger_at = at;
        self.tiger_heading = heading;
        self.fresh();
        for event in &tape[..upto.min(tape.len())] {
            match *event {
                Tape::Step(dt) => {
                    self.step(dt);
                    self.shot_t += dt;
                }
                Tape::Fire(shot, tuning) => {
                    self.shot = shot;
                    self.tuning = tuning;
                    self.apply_tuning();
                    self.fire();
                }
            }
        }
        self.shot = keep_shot;
        self.scene = keep_scene;
        self.tuning = keep_tuning;
        self.tiger_at = keep_at;
        self.tiger_heading = keep_heading;
        self.apply_tuning();
        self.frame_at = Some(index);
        self.past_run = false;
        self.status = format!("BACK TO {:.4} S: RE-RUN FROM A FRESH TIGER IN {:.1} S",
            self.shot_t, started.elapsed().as_secs_f32());
    }

    /// A backward seek from the run's checkpoints (`rewind`): the one before the frame put back and
    /// the frames from it stepped, so the frame's picture is its own update's; or, when the frame's
    /// own update was slow (the strike's solves), the frame's checkpoint put back, a 1 µs update
    /// for its picture and the checkpoint put back again. False when there is none to use.
    fn seek_kept(&mut self, index: u32) -> bool {
        let started = std::time::Instant::now();
        let own = self.rewind.at(index).filter(|_| !self.rewind.disabled && self.rewind.step_s(index) > rewind::SLOW_FRAME_S).cloned();
        if let Some(checkpoint) = own {
            let put_back = checkpoint.restore(&mut self.g, &mut self.r).and_then(|()| {
                self.place_camera();
                self.g.update(&TankInput::default(), rewind::PICTURE_STEP_S, &mut self.r);
                checkpoint.restore(&mut self.g, &mut self.r)
            });
            match put_back {
                Ok(()) => {
                    self.tape.truncate(checkpoint.tape_len);
                    self.shot_t = checkpoint.shot_t;
                    self.frame_at = Some(index);
                    self.past_run = false;
                    self.status = format!("BACK TO {:.4} S: PUT BACK IN {:.2} S (THE FRAME'S OWN UPDATE TOOK {:.1} S; DRAWN FROM A 1 US STEP)",
                        self.shot_t, started.elapsed().as_secs_f32(), self.rewind.step_s(index));
                    return true;
                }
                Err(e) => eprintln!("[fea-range] the checkpoint could not be put back ({e:#})"),
            }
        }
        let Some((from, checkpoint)) = self.rewind.before(index).map(|(f, c)| (f, c.clone())) else { return false };
        if let Err(e) = checkpoint.restore(&mut self.g, &mut self.r) {
            eprintln!("[fea-range] the checkpoint could not be put back ({e:#}); re-running the tape");
            return false;
        }
        self.tape.truncate(checkpoint.tape_len);
        self.shot_t = checkpoint.shot_t;
        self.frame_at = from;
        self.past_run = false;
        let first = from.map_or(0, |f| f + 1);
        for k in first..=index {
            self.run_frame(k);
        }
        let kept = self.rewind.prints_at(index);
        let now = rewind::Checkpoint::capture(&self.g, &self.r, None).map(|(c, _)| c.prints());
        let check = match (kept, now) {
            (Some(kept), Ok(now)) => match rewind::parted(&kept, &now).len() {
                0 => "BIT-EXACT".to_string(),
                n => format!("DIFFERS FROM THE FIRST TIME IN {n} FIELDS"),
            },
            _ => "NOT CHECKED".to_string(),
        };
        self.status = format!("BACK TO {:.4} S: PUT BACK AND {} FRAME{} STEPPED IN {:.2} S ({check})",
            self.shot_t, index + 1 - first, if index == first { "" } else { "S" }, started.elapsed().as_secs_f32());
        true
    }

    /// The renderer's camera at the editor's.
    fn place_camera(&mut self) {
        self.r.camera.pos = self.eye;
        self.r.camera.yaw = self.yaw;
        self.r.camera.pitch = self.pitch;
        self.r.camera.fov_y = self.fov_deg().to_radians();
    }

    /// The camera, the section and the crew drawing for this frame.
    fn pose_view(&mut self) {
        if self.pose_cached_view() { return; }
        // Playing the Tiger: the game placed its own camera, sights and crew for the player's
        // station; the editor's camera follows it so a pause (Esc) orbits from there.
        if self.player_control && !self.play_paused {
            self.apply_sky();
            self.g.drive_lights(&mut self.r);
            self.g.drive_armor_scars(&mut self.r);
            self.r.set_film_section(None, &[]);
            super::crew_corpse_tissue::set_review_section(None);
            self.eye = self.r.camera.pos;
            self.yaw = self.r.camera.yaw;
            self.pitch = self.r.camera.pitch;
            self.last_pose = None;
            self.apply_eye_fx();
            return;
        }
        self.apply_sky();
        // The lamps, fires and scene emitters from the present state (the simulation may be held:
        // a lamp switched or its power changed shows without a game step).
        self.g.drive_lights(&mut self.r);
        self.g.drive_armor_scars(&mut self.r);
        // The crewman the camera stands inside is not drawn (a first-person view from his post), as
        // the game leaves the player's own body out of his view.
        let inside = self.crew_at_eye();
        self.g.drive_singleplayer_crew_rigs_for_view(inside, &mut self.r);
        self.place_camera();
        // A moved camera restarts the transport's history (as `fea-film` does): the accumulator's
        // reprojection otherwise smears the view while it moves. Fewer samples, sharp frames.
        let pose = (self.eye, self.yaw, self.pitch, self.focal_mm);
        if let Some(w) = self.wear {
            self.r.wear = w;
        }
        if let Some([tank, world]) = self.snow {
            self.r.player_snow_load = tank;
            self.r.world_snow_load = world;
        }
        // Restarting the history every frame (tried 2026-11-03 against the held-camera haze) left the
        // picture unconverged: TV static. The history restarts only when the camera moves.
        // Camera motion is reprojected by the renderer; explicit cuts reset history.
        // Clearing all lighting histories here made illumination build up after every move.
        self.last_pose = Some(pose);
        self.r.optic_mask = 0.0;
        self.r.reticle = false;
        // The game's hovered-object outline is an interaction cue for play, not for this view.
        self.r.set_hovered_meshes(&[]);

        // Inside the hull's box the eye keeps the compartment's light and air; outside it is
        // an observer in the open (as `fea-film`'s Inside and Outside views).
        let local = self.g.player.hull_model().inverse().transform_point3(self.eye);
        let inside = local.x.abs() < 1.6 && local.y > 0.4 && local.y < 2.9 && local.z > -3.2 && local.z < 3.4;
        if !inside {
            self.r.interior_light = 0.0;
            self.r.interior_haze = 0.0;
            self.r.weather_shelter = 0.0;
            self.r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
        }
        match self.section_plane() {
            Some(plane) => {
                let meshes = self.section_meshes();
                self.r.set_film_section(Some(plane), &meshes);
                // A body solved on its cells is cut solid: its inside cells at the plane drawn too
                // (redrawn here, so a held simulation shows the plane where it is now).
                super::crew_corpse_tissue::set_review_section(Some(plane));
                self.g.drive_physical_crew_casualties(&mut self.r);
            }
            None => {
                self.r.set_film_section(None, &[]);
                if super::crew_corpse_tissue::review_section_set() {
                    super::crew_corpse_tissue::set_review_section(None);
                    self.g.drive_physical_crew_casualties(&mut self.r);
                }
            }
        }
        self.apply_eye_fx();
    }

    /// The EFFECTS toggles over what the game's step left for this frame (after `drive_lights`,
    /// which raises the flash flag): each switched off is off in the frame drawn.
    fn apply_eye_fx(&mut self) {
        if !self.eye_fx.concussion {
            self.r.concussion = 0.0;
        }
        if !self.eye_fx.flash {
            self.r.observer_flash_source = false;
        }
    }

    fn section_plane(&self) -> Option<Vec4> {
        if !self.section.on {
            return None;
        }
        let hull = self.g.player.hull_model();
        let n_local = self.section.normal_hull();
        let n = hull.transform_vector3(n_local).normalize_or_zero();
        let p = hull.transform_point3(SECTION_PIVOT + n_local * self.section.offset);
        Some(n.extend(n.dot(p)))
    }

    fn section_meshes(&self) -> Vec<usize> {
        let hull = self.g.player.hull_model();
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for bits in 0..8u32 {
            let local = Vec3::new(if bits & 1 == 0 { TANK_LO.x } else { TANK_HI.x }, if bits & 2 == 0 { TANK_LO.y } else { TANK_HI.y },
                if bits & 4 == 0 { TANK_LO.z } else { TANK_HI.z });
            let p = hull.transform_point3(local);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        // Everything of the vehicle, not only its solid parts: the glass, the periscope screens,
        // the lamp lenses, the crew and the effects inside it are cut with it (the camera's only).
        self.r.meshes_centred_within(lo, hi, 20.0)
    }

    /// The camera ray through the window point `cursor` (0..1; the viewport's centre for a point
    /// in the sidebar).
    fn ray(&self, cursor: [f32; 2]) -> (Vec3, Vec3) {
        let cursor = self.to_frame(cursor).unwrap_or([0.5, 0.5]);
        let f = self.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(f).normalize_or_zero();
        let t = (self.fov_deg().to_radians() * 0.5).tan();
        let aspect = self.r.iw as f32 / self.r.ih.max(1) as f32;
        let x = (cursor[0] * 2.0 - 1.0) * t * aspect;
        let y = (1.0 - cursor[1] * 2.0) * t;
        (self.eye, (f + right * x + up * y).normalize())
    }

    /// The window point of a world point (None behind the camera or outside the viewport).
    fn project(&self, p: Vec3) -> Option<[f32; 2]> {
        let mut c = crate::camera::Camera::new(self.r.iw as f32 / self.r.ih.max(1) as f32);
        c.pos = self.eye;
        c.yaw = self.yaw;
        c.pitch = self.pitch;
        c.fov_y = self.fov_deg().to_radians();
        let clip = c.view_proj() * p.extend(1.0);
        if clip.w <= 1.0e-4 {
            return None;
        }
        self.to_window([(clip.x / clip.w + 1.0) * 0.5, (1.0 - clip.y / clip.w) * 0.5])
    }

    /// Aim at the vehicle surface under the cursor (beyond the section's cut, if the eye is in
    /// the removed half); the direction follows the camera's ray unless `keep_dir`.
    fn pick(&mut self, cursor: [f32; 2], keep_dir: bool) {
        let (mut from, dir) = self.ray(cursor);
        if let Some(plane) = self.section_plane() {
            let n = plane.truncate();
            let side = n.dot(from) - plane.w;
            let denom = n.dot(dir);
            if side > 0.0 && denom < 0.0 {
                from += dir * (side / -denom + 0.002);
            }
        }
        let models = self.g.fire_surface_models();
        let to = from + dir * 60.0;
        match self.g.cabin_fire.sweep(from, to, &models) {
            Some(contact) => {
                let hit = from.lerp(to, contact.fraction);
                let hull_inv = self.g.player.hull_model().inverse();
                self.shot.target = Target::Point(hull_inv.transform_point3(hit));
                if !keep_dir {
                    // A new aim starts the shot square to the surface (along its inward normal),
                    // not along the camera's ray (owner, 2026-11-03).
                    // Always from outside in, also when the surface is picked from inside the tank
                    // (owner): the normal is turned to point toward the hull's centre.
                    let n = contact.normal.normalize_or_zero();
                    let centre = self.g.player.hull_model().transform_point3(SECTION_PIVOT);
                    let inward = if n.dot(centre - hit) > 0.0 { n } else { -n };
                    let incoming = if n.length_squared() < 0.5 { dir } else { inward };
                    self.shot.set_dir_hull(hull_inv.transform_vector3(incoming));
                }
                self.status = format!("AIM: picked at {:.2?} (hull frame)", hull_inv.transform_point3(hit).to_array());
            }
            None => self.status = "AIM: no vehicle surface under the cursor".into(),
        }
    }

    // ── The panel's knobs ───────────────────────────────────────────────────────────────────────

    fn knob_label(&self, knob: Knob) -> (String, String) {
        let t = &self.tuning;
        let a = crew_tissue::TissueTuning::AUTHORED;
        let mark = |changed: bool| if changed { " *" } else { "" };
        match knob {
            Knob::CacheSim => ("FIRE SHOT".into(), "WATCH AND CACHE / CANCEL".into()),
            Knob::RenderShot => ("RENDER SHOT".into(), "CACHE FRAMES + MP4".into()),
            Knob::SavedShot => ("SHOT PRESET".into(), self.shot_preset_at
                .and_then(|i| self.shot_presets.get(i)).and_then(|v| v["name"].as_str())
                .unwrap_or("SELECT < >").to_uppercase()),
            Knob::AddShot => ("SAVE PRESET".into(), "SAVE AS NEXT SHOT".into()),
            Knob::Shell => ("SHELL".into(), self.shot.spec().name.to_uppercase()),
            Knob::Gun => ("FIRED BY".into(), SHOOTERS[self.shot.shell % SHOOTERS.len()].into()),
            Knob::Range => ("RANGE".into(), format!("{:.0} M", self.shot.range_m)),

            Knob::Velocity => ("IMPACT VELOCITY".into(), format!("{:.0} M/S (MUZZLE {:.0})", self.shot.impact_velocity(), self.shot.spec().muzzle_velocity)),
            Knob::FromAz => ("FROM BEARING".into(), format!("{:+.0} DEG {}", self.shot.from_az, match self.shot.from_az.round() as i32 {
                -20..=20 => "AHEAD", 21..=159 => "LEFT", -159..=-21 => "RIGHT", _ => "BEHIND" })),
            Knob::FromEl => ("FROM ELEVATION".into(), format!("{:+.0} DEG", self.shot.from_el)),
            Knob::TargetKind => ("AIM AT".into(), match self.shot.target {
                Target::Point(p) => format!("POINT {:.2} {:.2} {:.2}", p.x, p.y, p.z),
                Target::Man(role) => format!("{:?}", role).to_uppercase(),
            }),
            Knob::Seed => ("SEED".into(), format!("{:08X}", self.shot.seed)),
            Knob::Racks => ("RACKS".into(), if self.scene.racks_loaded { "LOADED" } else { "EMPTY" }.into()),
            Knob::Lamps => ("CABIN LAMPS".into(), if self.scene.cabin_lamps { "ON" } else { "OFF" }.into()),
            Knob::LampPower => ("LAMP POWER".into(), format!("X{:.0}{}", review_lamp_gain(),
                if review_lamp_gain() != 1.0 { " * (REVIEW)" } else { "" })),
            Knob::Engine => ("ENGINE".into(), if self.engine { "ON (EXHAUST)" } else { "OFF" }.into()),
            Knob::MovingCamera => ("MOVING CAMERA".into(), if self.accumulate_moving { "ON: ACCUMULATED (SMEARS)" } else { "OFF: SHARP" }.into()),
            Knob::Wear(k) => (["INTERIOR GRIME", "EXTERIOR GRIME", "GLASS GRIME", "MECHANICAL WEAR"][k].into(),
                format!("{:.2}{}", self.r.wear[k], if self.wear.is_some() { " HELD" } else { "" })),
            Knob::SnowTank => ("SNOW ON TANK".into(), format!("{:.2}{}", self.r.player_snow_load, if self.snow.is_some() { " HELD" } else { "" })),
            Knob::SnowWorld => ("SNOW ON GROUND".into(), format!("{:.2}{}", self.r.world_snow_load, if self.snow.is_some() { " HELD" } else { "" })),
            Knob::SmokeDensity => ("IMPACT SMOKE".into(), format!("X{:.2}{}", review_smoke_density(),
                if review_smoke_density() != 1.0 { " * (REVIEW)" } else { "" })),
            Knob::SmokeDelay => ("SMOKE DELAY".into(), format!("{:.2} S{}", review_smoke_delay(),
                if review_smoke_delay() != super::weapon_vfx::IMPACT_SMOKE_DELAY_S { " * (REVIEW)" } else { "" })),
            Knob::EffectsAuthored => ("RESTORE AUTHORED".into(), "SMOKE AS AUTHORED".into()),
            Knob::MaterialsGame => ("GAME'S OWN".into(), if self.wear.is_none() && self.snow.is_none() { "IN FORCE".into() } else { "CLICK".into() }),
            Knob::TimeSpeed => ("TIME SPEED".into(), format!("X{:.2}{}", self.time_speed,
                if self.time_speed < 1.0 { " (SLOWER)" } else if self.time_speed > 1.0 { " (FASTER)" } else { "" })),
            Knob::Loop => ("LOOP".into(), if self.looping { "ON" } else { "OFF" }.into()),
            Knob::Focal => ("FOCAL LENGTH".into(), format!("{:.0} MM ({:.0} DEG)", self.focal_mm, self.fov_deg())),
            Knob::ExportCams => ("SAVE TO CATALOG".into(), "FOR DEVELOPMENT".into()),
            Knob::SavedCam => ("CAMERA".into(), match self.camera_at {
                Some(i) => format!("{}/{} {}", i + 1, self.cameras.len(), self.cameras[i].name.to_uppercase()),
                None if self.cameras.is_empty() => "NONE SAVED".into(),
                None => format!("FREE ({} SAVED)", self.cameras.len()),
            }),
            Knob::AddCam => ("ADD CAMERA (K)".into(), "SAVE THIS VIEW".into()),
            Knob::DeleteCam => ("DELETE CAMERA".into(), self.camera_at.map_or("-".into(), |i| format!("NUMBER {}", i + 1))),
            Knob::AutoExposure => ("AUTO EXPOSURE".into(), if self.sky.exposure.is_none() { "ON (EYE)" } else { "OFF (HELD)" }.into()),
            Knob::Look => ("LOOK".into(), LOOKS.iter().find(|(v, _)| (*v - self.r.look).abs() < 0.1).map_or("?", |(_, n)| *n).into()),
            Knob::LookEv => ("EXPOSURE COMP".into(), format!("{:+.2} EV", self.r.display_grade[0].max(1e-4).log2())),
            Knob::Concussion => ("CONCUSSION BLUR".into(), if self.eye_fx.concussion { "ON (GAME)" } else { "OFF" }.into()),
            Knob::FlashAfterimage => ("FLASH AFTERIMAGE".into(), if self.eye_fx.flash { "ON (GAME)" } else { "OFF" }.into()),
            Knob::Saturation => ("SATURATION".into(), format!("{:.2}", self.r.display_grade[1])),
            Knob::Warmth => ("WARMTH".into(), format!("{:+.2}", self.r.display_grade[2])),
            Knob::Gamma => ("GAMMA".into(), format!("{:.2}", self.r.display_grade[3])),
            Knob::DitherPattern => ("DITHER PATTERN".into(), DITHERS[(self.r.display_dither_pattern.round() as usize).min(3)].into()),
            Knob::PaletteLevels => ("PALETTE LEVELS".into(), format!("{:.0}", self.tuning_display().palette_levels)),
            Knob::OrderedDither => ("ORDERED DITHER".into(), format!("{:.2}", self.tuning_display().ordered_dither)),
            Knob::LdrDither => ("LDR DITHER".into(), format!("{:.2}", self.tuning_display().ldr_dither)),
            Knob::Retro => ("RETRO STRENGTH".into(), format!("{:.2}", self.tuning_display().retro_strength)),
            Knob::WhiteBalance => ("WHITE BALANCE".into(), if self.tuning_display().ambient_white_balance { "ON" } else { "OFF" }.into()),
            Knob::WhiteBalanceStrength => ("BALANCE STRENGTH".into(), format!("{:.2}", self.tuning_display().ambient_white_balance_strength)),
            Knob::Fog => ("FOG DENSITY".into(), format!("{:.4}", self.r.fog_density)),
            Knob::LookDefaults => ("RESTORE LOOK".into(), "AS STARTED".into()),
            Knob::Fresh => ("FRESH EACH RUN".into(), if self.scene.fresh_each_run { "YES" } else { "NO (STACK)" }.into()),
            Knob::SlowSpan => ("1/480 S SPAN".into(), format!("{:.3} S", self.scene.slow_s)),
            Knob::RunLength => ("RUN LENGTH".into(), format!("{:.1} S", self.scene.run_s)),
            Knob::SectionOn => ("SECTION".into(), if self.section.on { "ON" } else { "OFF" }.into()),
            Knob::SectionPreset => ("PRESET".into(), "TOP LEFT RIGHT FRONT REAR".into()),
            Knob::SnapToShot => ("SNAP TO SHOT (G)".into(), "PLANE OF THE SHOT".into()),
            Knob::SectionYaw => ("NORMAL YAW".into(), format!("{:+.0} DEG", self.section.yaw)),
            Knob::SectionPitch => ("NORMAL PITCH".into(), format!("{:+.0} DEG", self.section.pitch)),
            Knob::SectionOffset => ("OFFSET".into(), format!("{:+.2} M", self.section.offset)),
            Knob::Day => ("DATE".into(), self.sky.instant().to_iso()[..10].to_string()),
            Knob::Time => ("LOCAL TIME".into(), format!("{:02}:{:02}", self.sky.local_h.floor() as i32, ((self.sky.local_h.fract()) * 60.0).round() as i32 % 60)),
            Knob::SkyRunning => ("SKY CLOCK".into(), if self.sky.running { "ON (RUNS)" } else { "OFF (HELD)" }.into()),
            Knob::SkyRate => ("SKY RATE".into(), format!("X{:.0} ({:.1} MIN/S)", self.sky.rate, self.sky.rate / 60.0)),
            Knob::Weather => ("WEATHER".into(), format!("{:.1}", self.sky.weather)),
            Knob::Exposure => ("EXPOSURE".into(), match self.sky.exposure { None => "AUTO (EYE)".into(), Some(e) => format!("{e:.0}") }),
            Knob::Tear(k) => (format!("TEAR {}", TISSUE_NAMES[k]), format!("{}{}", fmt_kj(t.tear_j_m3[k]), mark(t.tear_j_m3[k] != a.tear_j_m3[k]))),
            Knob::Stretch(k) => (format!("STRETCH {}", TISSUE_NAMES[k]), format!("{:.3}{}", t.critical_stretch[k], mark(t.critical_stretch[k] != a.critical_stretch[k]))),
            Knob::Forward => ("FORWARD SHARE".into(), format!("{:.2}{}", t.forward, mark(t.forward != a.forward))),
            Knob::Outward => ("OUTWARD SHARE".into(), format!("{:.2}{}", t.outward, mark(t.outward != a.outward))),
            Knob::Reach => ("ACTIVE REACH".into(), format!("{:.3} M{}", t.active_reach_m, mark(t.active_reach_m != a.active_reach_m))),
            Knob::Spread => ("MOMENTUM SPREAD".into(), format!("X{:.2}{}", t.spread, mark(t.spread != a.spread))),
            Knob::Landing => ("LANDING BREAKUP".into(), format!("X{:.2}{}", t.landing_breakup, mark(t.landing_breakup != a.landing_breakup))),
            Knob::TissueAuthored => ("RESTORE AUTHORED".into(), if *t == a { "IN FORCE".into() } else { "CLICK".into() }),
        }
    }

    /// Adjust `knob` by `dir` steps (+1/−1); `fine` and `coarse` scale the step.
    fn adjust(&mut self, knob: Knob, dir: f32, fine: bool, coarse: bool) {
        let s = dir * if fine { 0.1 } else if coarse { 10.0 } else { 1.0 };
        // A multiplicative step for energies and factors: 1.25 per click.
        let ratio = 1.25f32.powf(s);
        let n = shells().len() as i32;
        match knob {
            // The range is kept; the new shell's own velocity at it follows.
            Knob::CacheSim => self.cache_simulation(),
            Knob::RenderShot => self.render_cached_shot(),
            Knob::SavedShot => {
                if !self.shot_presets.is_empty() {
                    let n = self.shot_presets.len() as i32;
                    let i = self.shot_preset_at.map_or(if dir > 0.0 { 0 } else { n - 1 },
                        |i| (i as i32 + dir as i32).rem_euclid(n)) as usize;
                    let preset = self.shot_presets[i].clone();
                    self.apply_state(&preset["settings"]);
                    self.apply_sky();
                    self.fresh();
                    self.shot_preset_at = Some(i);
                    self.status = format!("PRESET {} LOADED", preset["name"].as_str().unwrap_or(""));
                }
            }
            Knob::AddShot => {
                let mut n = 1;
                while self.shot_presets.iter().any(|p| p["name"].as_str() == Some(format!("shot{n}").as_str())) { n += 1; }
                let name = format!("shot{n}");
                let mut presets = self.shot_presets.clone();
                presets.push(serde_json::json!({"name": name, "settings": self.state_json()}));
                let json = serde_json::json!({"schema": "fea-range-presets/v1", "presets": presets});
                let result = (|| -> anyhow::Result<()> {
                    let path = std::path::Path::new(PRESETS_PATH);
                    let temp = path.with_extension("json.tmp");
                    std::fs::write(&temp, serde_json::to_vec_pretty(&json)?)?;
                    std::fs::rename(temp, path)?;
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        self.shot_presets = presets;
                        self.shot_preset_at = Some(self.shot_presets.len() - 1);
                        self.status = format!("PRESET {name} SAVED");
                    }
                    Err(e) => self.status = format!("PRESET NOT SAVED: {e}"),
                }
            }
            Knob::Shell => self.shot.shell = (self.shot.shell as i32 + dir as i32).rem_euclid(n) as usize,
            Knob::Range => self.shot.range_m = (self.shot.range_m + 50.0 * s).clamp(0.0, 6_000.0),
            Knob::Velocity => {
                let v = (self.shot.impact_velocity() + 10.0 * s).max(50.0);
                self.shot.range_m = range_for_velocity(self.shot.spec(), v);
            }
            Knob::FromAz => self.shot.from_az = ((self.shot.from_az + 5.0 * s + 180.0).rem_euclid(360.0)) - 180.0,
            Knob::FromEl => self.shot.from_el = (self.shot.from_el + 1.0 * s).clamp(-60.0, 80.0),
            Knob::TargetKind => {
                let order = [CrewRole::Commander, CrewRole::Gunner, CrewRole::Loader, CrewRole::Driver, CrewRole::Radioman];
                self.shot.target = match self.shot.target {
                    Target::Point(_) => Target::Man(order[if dir > 0.0 { 0 } else { 4 }]),
                    Target::Man(role) => {
                        let i = order.iter().position(|r| *r == role).unwrap_or(0) as i32 + dir as i32;
                        if (0..5).contains(&i) { Target::Man(order[i as usize]) } else {
                            // Past either end: the man's present point, as a picked point.
                            Target::Point(self.g.player.hull_model().inverse().transform_point3(self.g.crew_solo_man_point(role)))
                        }
                    }
                };
            }
            Knob::Seed => self.shot.seed = self.shot.seed.wrapping_add_signed(s as i32),
            Knob::Racks => self.scene.racks_loaded = !self.scene.racks_loaded,
            Knob::Lamps => {
                self.scene.cabin_lamps = !self.scene.cabin_lamps;
                self.g.set_cabin_lights(self.scene.cabin_lamps);
            }
            Knob::LampPower => {
                let gain = (review_lamp_gain() * 2f32.powf(s)).clamp(1.0, 1.0e6);
                set_review_lamp_gain(if (gain - 1.0).abs() < 1.0e-3 { 1.0 } else { gain });
            }
            Knob::Fresh => self.scene.fresh_each_run = !self.scene.fresh_each_run,
            Knob::SlowSpan => self.scene.slow_s = (self.scene.slow_s + 0.05 * s).clamp(0.0, 2.0),
            Knob::RunLength => self.scene.run_s = (self.scene.run_s + 0.5 * s).clamp(0.1, 60.0),
            Knob::SectionOn => self.section.on = !self.section.on,
            Knob::SectionPreset => {
                let presets = [Vec3::Y, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z];
                let n = self.section.normal_hull();
                let now = presets.iter().position(|p| p.dot(n) > 0.999).map_or(-1, |i| i as i32);
                let next = (now + dir as i32).rem_euclid(presets.len() as i32) as usize;
                self.section.set_normal_hull(presets[next]);
                self.section.offset = if next == 0 { 0.5 } else { 0.0 };
                self.section.on = true;
            }
            Knob::SectionYaw => {
                self.section.yaw = ((self.section.yaw + 90.0 * s + 180.0).rem_euclid(360.0)) - 180.0;
                self.section.snap();
            }
            Knob::SectionPitch => {
                self.section.pitch = (self.section.pitch + 90.0 * s).clamp(-90.0, 90.0);
                self.section.snap();
            }
            Knob::SectionOffset => self.section.offset = (self.section.offset + 0.02 * s).clamp(-6.0, 6.0),
            Knob::Day => self.sky.day = (self.sky.day + s as i32).clamp(-365, 365),
            Knob::Time => self.sky.local_h = (self.sky.local_h + 0.25 * s).rem_euclid(24.0),
            Knob::Weather => self.sky.weather = (self.sky.weather + 0.1 * s).clamp(0.0, 1.0),
            Knob::Exposure => {
                // A third of a stop per click from the eye's present value; below 1 returns to auto.
                let now = self.sky.exposure.unwrap_or_else(|| self.metered_exposure().unwrap_or(1000.0));
                let next = now * 2f32.powf(s / 3.0);
                self.sky.exposure = if dir < 0.0 && self.sky.exposure.is_none() { Some(next) }
                    else if next < 1.0 { None } else { Some(next) };
            }
            Knob::Tear(k) => self.tuning.tear_j_m3[k] = (self.tuning.tear_j_m3[k] * ratio).clamp(100.0, 1.0e9),
            Knob::Stretch(k) => self.tuning.critical_stretch[k] = (self.tuning.critical_stretch[k] * 1.1f32.powf(s)).clamp(0.001, 10.0),
            Knob::Forward => self.tuning.forward = (self.tuning.forward + 0.05 * s).clamp(0.0, 2.0),
            Knob::Outward => self.tuning.outward = (self.tuning.outward + 0.05 * s).clamp(0.0, 2.0),
            Knob::Reach => self.tuning.active_reach_m = (self.tuning.active_reach_m + 0.01 * s).clamp(0.01, 0.6),
            Knob::Landing => self.tuning.landing_breakup = (self.tuning.landing_breakup * ratio).clamp(0.01, 1.0e4),
            Knob::TissueAuthored => self.tuning = crew_tissue::TissueTuning::AUTHORED,
            Knob::SnapToShot => self.snap_section_to_shot(),
            Knob::Gun => {}
            Knob::Loop => self.looping = !self.looping,
            Knob::Engine => self.engine = !self.engine,
            Knob::MovingCamera => self.accumulate_moving = !self.accumulate_moving,
            Knob::EffectsAuthored => set_review_smoke(1.0, None),
            Knob::MaterialsGame => {
                self.wear = None;
                self.snow = None;
            }
            Knob::SkyRunning => self.sky.running = !self.sky.running,
            Knob::SavedCam => {
                if !self.cameras.is_empty() {
                    let n = self.cameras.len() as i32;
                    let next = self.camera_at.map_or(if dir > 0.0 { 0 } else { n - 1 }, |i| (i as i32 + dir as i32).rem_euclid(n));
                    self.view_camera(next as usize);
                }
            }
            Knob::AddCam => {
                let inv = self.g.player.hull_model().inverse();
                let (hull_eye, hull_forward) = (inv.transform_point3(self.eye), inv.transform_vector3(self.forward()).normalize_or_zero());
                let name = camera_name(hull_eye, hull_forward);
                self.cameras.push(SavedCam { name: name.clone(), eye: self.eye.to_array(), yaw: self.yaw, pitch: self.pitch,
                    focal_mm: self.focal_mm, hull_eye: Some(hull_eye.to_array()), hull_forward: Some(hull_forward.to_array()),
                    section: Some(SavedSection { on: self.section.on, yaw: self.section.yaw, pitch: self.section.pitch,
                        offset: self.section.offset }) });
                self.camera_at = Some(self.cameras.len() - 1);
                self.save_cameras();
                self.status = format!("CAMERA {} SAVED: {} (KEY {})", self.cameras.len(), name.to_uppercase(), self.cameras.len());
            }
            Knob::ExportCams => {
                let json = serde_json::json!({
                    "schema": "fea-range-cameras/v1",
                    "frame": "the Tiger's hull frame: +Z forward, +X left, +Y up, metres; the eye and forward are hull_eye and hull_forward",
                    "saved_at_tiger": {"at": self.tiger_at.to_array(), "heading_rad": self.tiger_heading},
                    "cameras": self.cameras,
                });
                self.status = match std::fs::write(CAMERA_CATALOG, serde_json::to_string_pretty(&json).unwrap_or_default()) {
                    Ok(()) => format!("{} CAMERAS WRITTEN TO {CAMERA_CATALOG}", self.cameras.len()),
                    Err(e) => format!("CATALOG NOT WRITTEN: {e}"),
                };
            }
            Knob::DeleteCam => {
                if let Some(i) = self.camera_at.take() {
                    self.cameras.remove(i);
                    self.save_cameras();
                    self.status = format!("CAMERA {} DELETED", i + 1);
                }
            }
            Knob::Concussion => self.eye_fx.concussion = !self.eye_fx.concussion,
            Knob::FlashAfterimage => self.eye_fx.flash = !self.eye_fx.flash,
            Knob::AutoExposure => {
                self.sky.exposure = match self.sky.exposure {
                    None => Some(self.metered_exposure().unwrap_or(1000.0)),
                    Some(_) => None,
                };
            }
            Knob::Look => {
                let i = LOOKS.iter().position(|(v, _)| (*v - self.r.look).abs() < 0.1).unwrap_or(0) as i32;
                self.r.look = LOOKS[(i + dir as i32).rem_euclid(LOOKS.len() as i32) as usize].0;
            }
            Knob::DitherPattern => self.r.display_dither_pattern = (self.r.display_dither_pattern + dir).rem_euclid(4.0),
            Knob::WhiteBalance => {
                let mut t = self.tuning_display();
                t.ambient_white_balance = !t.ambient_white_balance;
                self.r.display_tuning = Some(t);
            }
            Knob::LookDefaults => {
                let (look, grade, dither, tuning, fog) = self.look_start;
                self.r.look = look;
                self.r.display_grade = grade;
                self.r.display_dither_pattern = dither;
                self.r.display_tuning = tuning;
                self.r.fog_density = fog;
                self.fog = None;
            }
            // The other dials: a fiftieth of their range a step (a two-hundredth fine).
            other => {
                let t = self.dial_t(other) + s * 0.02;
                self.set_dial_t(other, t);
            }
        }
        self.apply_tuning();
    }

    /// The display tuning in force (the renderer's defaults at its grid until a knob sets one).
    fn tuning_display(&self) -> crate::renderer::display_tuning::DisplayTuning {
        self.r.display_tuning.unwrap_or_else(|| crate::renderer::display_tuning::DisplayTuning::at_resolution(self.r.iw, self.r.ih))
    }

    /// A dial's value.
    fn dial_get(&self, knob: Knob) -> f32 {
        let t = &self.tuning;
        let d = self.tuning_display();
        match knob {
            Knob::Range => self.shot.range_m,
            Knob::Velocity => self.shot.impact_velocity(),
            Knob::FromAz => self.shot.from_az,
            Knob::FromEl => self.shot.from_el,
            Knob::LampPower => review_lamp_gain(),
            Knob::SlowSpan => self.scene.slow_s,
            Knob::RunLength => self.scene.run_s,
            Knob::TimeSpeed => self.time_speed,
            Knob::Wear(k) => self.r.wear[k],
            Knob::SmokeDensity => review_smoke_density(),
            Knob::SmokeDelay => review_smoke_delay(),
            Knob::SnowTank => self.r.player_snow_load,
            Knob::SnowWorld => self.r.world_snow_load,
            Knob::Focal => self.focal_mm,
            Knob::SectionYaw => self.section.yaw,
            Knob::SectionPitch => self.section.pitch,
            Knob::SectionOffset => self.section.offset,
            Knob::Time => self.sky.local_h,
            Knob::SkyRate => self.sky.rate,
            Knob::Weather => self.sky.weather,
            Knob::Exposure => self.sky.exposure.unwrap_or(1000.0),
            Knob::LookEv => self.r.display_grade[0],
            Knob::Saturation => self.r.display_grade[1],
            Knob::Warmth => self.r.display_grade[2],
            Knob::Gamma => self.r.display_grade[3],
            Knob::PaletteLevels => d.palette_levels,
            Knob::OrderedDither => d.ordered_dither,
            Knob::LdrDither => d.ldr_dither,
            Knob::Retro => d.retro_strength,
            Knob::WhiteBalanceStrength => d.ambient_white_balance_strength,
            Knob::Fog => self.r.fog_density,
            Knob::Tear(k) => t.tear_j_m3[k],
            Knob::Stretch(k) => t.critical_stretch[k],
            Knob::Forward => t.forward,
            Knob::Outward => t.outward,
            Knob::Reach => t.active_reach_m,
            Knob::Spread => t.spread,
            Knob::Landing => t.landing_breakup,
            _ => 0.0,
        }
    }

    /// Set a dial (clamped to its range).
    fn dial_set(&mut self, knob: Knob, v: f32) {
        let (lo, hi, _) = dial_range(knob);
        let v = v.clamp(lo, hi);
        let mut d = self.tuning_display();
        match knob {
            Knob::Range => self.shot.range_m = v,
            Knob::Velocity => self.shot.range_m = range_for_velocity(self.shot.spec(), v),
            Knob::FromAz => self.shot.from_az = v,
            Knob::FromEl => self.shot.from_el = v,
            Knob::LampPower => set_review_lamp_gain(if v < 1.01 { 1.0 } else { v }),
            Knob::SlowSpan => self.scene.slow_s = v,
            Knob::RunLength => self.scene.run_s = v,
            Knob::TimeSpeed => self.time_speed = if (v - 1.0).abs() < 0.03 { 1.0 } else { v },
            Knob::Wear(k) => {
                let mut w = self.wear.unwrap_or(self.r.wear);
                w[k] = v;
                self.wear = Some(w);
                self.r.wear = w;
            }
            Knob::SmokeDensity => set_review_smoke(if (v - 1.0).abs() < 0.02 { 1.0 } else { v },
                Some(review_smoke_delay()).filter(|d| *d != super::weapon_vfx::IMPACT_SMOKE_DELAY_S)),
            Knob::SmokeDelay => set_review_smoke(review_smoke_density(),
                Some(v).filter(|d| (d - super::weapon_vfx::IMPACT_SMOKE_DELAY_S).abs() > 0.005)),
            Knob::SnowTank | Knob::SnowWorld => {
                let mut sn = self.snow.unwrap_or([self.r.player_snow_load, self.r.world_snow_load]);
                sn[if knob == Knob::SnowTank { 0 } else { 1 }] = v;
                self.snow = Some(sn);
                [self.r.player_snow_load, self.r.world_snow_load] = sn;
            }
            Knob::Focal => self.focal_mm = v,
            Knob::SectionYaw => { self.section.yaw = v; self.section.snap(); self.section.on = true; }
            Knob::SectionPitch => { self.section.pitch = v; self.section.snap(); self.section.on = true; }
            Knob::SectionOffset => { self.section.offset = v; self.section.on = true; }
            Knob::Time => self.sky.local_h = v.min(23.99),
            Knob::SkyRate => self.sky.rate = v,
            Knob::Weather => self.sky.weather = v,
            Knob::Exposure => self.sky.exposure = Some(v),
            Knob::LookEv => self.r.display_grade[0] = v,
            Knob::Saturation => self.r.display_grade[1] = v,
            Knob::Warmth => self.r.display_grade[2] = v,
            Knob::Gamma => self.r.display_grade[3] = v,
            Knob::PaletteLevels => { d.palette_levels = v.round(); self.r.display_tuning = Some(d); }
            Knob::OrderedDither => { d.ordered_dither = v; self.r.display_tuning = Some(d); }
            Knob::LdrDither => { d.ldr_dither = v; self.r.display_tuning = Some(d); }
            Knob::Retro => { d.retro_strength = v; self.r.display_tuning = Some(d); }
            Knob::WhiteBalanceStrength => { d.ambient_white_balance_strength = v; self.r.display_tuning = Some(d); }
            Knob::Fog => self.fog = Some(v),
            Knob::Tear(k) => self.tuning.tear_j_m3[k] = v,
            Knob::Stretch(k) => self.tuning.critical_stretch[k] = v,
            Knob::Forward => self.tuning.forward = v,
            Knob::Outward => self.tuning.outward = v,
            Knob::Reach => self.tuning.active_reach_m = v,
            Knob::Spread => self.tuning.spread = v,
            Knob::Landing => self.tuning.landing_breakup = v,
            _ => {}
        }
        self.apply_tuning();
    }

    /// A dial's position, 0..1 of its range.
    fn dial_t(&self, knob: Knob) -> f32 {
        let (lo, hi, log) = dial_range(knob);
        let v = self.dial_get(knob).clamp(lo, hi);
        if log { (v / lo).ln() / (hi / lo).ln() } else { (v - lo) / (hi - lo) }
    }

    fn set_dial_t(&mut self, knob: Knob, t: f32) {
        let (lo, hi, log) = dial_range(knob);
        let t = t.clamp(0.0, 1.0);
        self.dial_set(knob, if log { lo * (hi / lo).powf(t) } else { lo + (hi - lo) * t });
    }

    /// Look through saved camera `i`.
    fn view_camera(&mut self, i: usize) {
        if let Some(c) = self.cameras.get(i).cloned() {
            match (c.hull_eye, c.hull_forward) {
                (Some(e), Some(f)) => {
                    let hull = self.g.player.hull_model();
                    self.eye = hull.transform_point3(Vec3::from_array(e));
                    self.set_view_forward(hull.transform_vector3(Vec3::from_array(f)));
                }
                _ => {
                    self.eye = Vec3::from_array(c.eye);
                    self.yaw = c.yaw;
                    self.pitch = c.pitch;
                }
            }
            self.focal_mm = c.focal_mm;
            if let Some(sec) = c.section {
                self.section = Section { on: sec.on, yaw: sec.yaw, pitch: sec.pitch, offset: sec.offset };
                self.section.snap();
            }
            self.camera_at = Some(i);
            self.status = format!("CAMERA {}: {} ({:.0} MM)", i + 1, c.name.to_uppercase(), c.focal_mm);
        } else {
            self.status = format!("NO CAMERA {} (K OR ADD CAMERA SAVES ONE)", i + 1);
        }
    }

    fn save_cameras(&self) {
        let _ = std::fs::write(CAMERAS_PATH, serde_json::to_string_pretty(&self.cameras).unwrap_or_default());
    }

    /// The eye's present automatic exposure (to start a held exposure from).
    fn metered_exposure(&mut self) -> Option<f32> {
        self.r.debug_read_active_exposure(&self.device, &self.queue).ok().map(|[e, _]| e)
    }

    /// The knob set as Rust constants and JSON, for the FEA session (P).
    fn print_tuning(&self) -> String {
        let t = &self.tuning;
        let text = format!(
            "const TEAR_J_M3: [f32; 4] = [{:.1}, {:.1}, {:.1}, {:.1}];\nconst CRITICAL_STRETCH: [f32; 4] = [{:.4}, {:.4}, {:.4}, {:.4}];\n\
             const FORWARD: f32 = {:.3};\nconst OUTWARD: f32 = {:.3};\nconst ACTIVE_REACH_M: f32 = {:.4};\n// landing breakup factor x{:.3} (1.0 authored)\n",
            t.tear_j_m3[0], t.tear_j_m3[1], t.tear_j_m3[2], t.tear_j_m3[3], t.critical_stretch[0], t.critical_stretch[1],
            t.critical_stretch[2], t.critical_stretch[3], t.forward, t.outward, t.active_reach_m, t.landing_breakup);
        let json = serde_json::json!({"tear_j_m3": t.tear_j_m3, "critical_stretch": t.critical_stretch, "forward": t.forward,
            "outward": t.outward, "active_reach_m": t.active_reach_m, "landing_breakup": t.landing_breakup,
            "shot": {"shell": self.shot.spec().name, "range_m": self.shot.range_m, "impact_mps": self.shot.impact_velocity(),
                "from_az_deg": self.shot.from_az, "from_el_deg": self.shot.from_el, "seed": self.shot.seed,
                "target": format!("{:?}", self.shot.target)}});
        let path = "target/fea_range_tuning.json";
        let _ = std::fs::write(path, serde_json::to_string_pretty(&json).unwrap_or_default());
        println!("[fea-range] tissue knobs (review override; written to {path}):\n{text}");
        text
    }

    /// A digest of the solved state (the crew's bodies and tissue, the chain, the scars) to check
    /// a re-simulation against its recording.
    fn digest(&self) -> String {
        let bodies = self.g.debug_crew_solo_bodies();
        let chain: Vec<String> = self.g.fea.receipts.iter().map(|c| c.result.summary()).collect();
        let scars = self.g.armor_scars.scars().len();
        let tissue = self.g.crew_corpse_render.borrow().as_ref().map(|c| c.receipt()["tissue"].clone());
        format!("bodies={bodies} chain={chain:?} scars={scars} tissue={}", tissue.unwrap_or_default())
    }

    // ── Per-frame ──────────────────────────────────────────────────────────────────────────────

    /// Advance the simulation by one window frame of `dt` seconds while it plays, at the time
    /// speed: the run's frames, looping over the clip or
    /// the run when LOOP is on, stopping at a clip's end without it, and real time past the run.
    fn tick(&mut self, dt: f32) {
        self.captured = false;
        if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("cache") && self.tick_cache(dt) { return; }
        if self.sky.running {
            self.sky.local_h += dt * self.sky.rate / 3600.0;
            while self.sky.local_h >= 24.0 {
                self.sky.local_h -= 24.0;
                self.sky.day += 1;
            }
        }
        // A casualty's tissue solving on its worker (`Game::casualty_tissue_pending`): the
        // simulation waits for it without holding the window (the next step would block on it).
        // Solves already done are taken back here (the world is frozen while any is pending, so
        // this is the strike's own result, as the game's death screen takes them back); nothing
        // else would, and the range waited for ever once the player's own man was killed.
        self.g.settle_casualty_tissue(false);
        if self.playing && self.mode == Mode::Live {
            if self.next_run_frame().is_some() {
                self.play_clock += self.time_speed;
                let mut frames = 0;
                while self.play_clock >= 1.0 && frames < 4 {
                    self.play_clock -= 1.0;
                    frames += 1;
                    let Some(k) = self.next_run_frame() else { break };
                    self.run_frame(k);
                    if self.at_play_end(k) {
                        break;
                    }
                }
                self.play_clock = self.play_clock.min(1.0);
            } else if self.looping && !self.free_fire && self.run.is_some() {
                self.loop_back();
            } else {
                // While recording, a frame is 1/30 s of game time whatever the window's frame rate,
                // so the file plays at true speed.
                let dt = if self.recorder.is_some() { TICK * REAL_TICKS_PER_FRAME as f32 } else { dt };
                self.play_clock += dt * self.time_speed;
                let mut steps = 0;
                while self.play_clock >= TICK && steps < if self.recorder.is_some() { 64 } else { 4 } {
                    self.play_clock -= TICK;
                    self.step(TICK);
                    self.shot_t += TICK;
                    steps += 1;
                }
                self.play_clock = self.play_clock.min(TICK);
                self.past_run = self.run.is_some();
            }
            self.idle_s = 0.0;
        }
        if let Some(c) = self.g.fea.receipts.last() {
            self.last_chain = format!("{} {:?}: {}", c.shell, c.terminal, c.result.summary());
        }
    }

    /// Run frame `k` just played: at the clip's end, loop back or pause.
    fn at_play_end(&mut self, k: u32) -> bool {
        let Some((_, end)) = self.clip else { return false };
        if k < end {
            return false;
        }
        if self.looping && !self.free_fire {
            self.loop_back();
        } else {
            self.playing = false;
            self.status = "CLIP END: PAUSED (LOOP REPEATS IT)".into();
        }
        true
    }

    /// Space: the run (or its clip) again from its start, playing.
    fn replay(&mut self) {
        if self.cached_seek(self.clip.map_or(0, |c| c.0 as usize)) {
            self.playing = true; return;
        }
        if self.run.is_some() {
            let start = self.clip.map_or(0, |c| c.0);
            self.seek(start);
            self.playing = true;
            self.status = format!("REPLAY FROM {:.4} S (THE SHOT RE-RUN; TISSUE DETAIL CAN DIFFER)", self.shot_t);
        } else {
            self.status = "NO RUN TO REPLAY: ENTER RUNS, F FIRES".into();
        }
    }

    /// Back to the clip's start (or the run's), by re-running the shot to it.
    fn loop_back(&mut self) {
        let start = self.clip.map_or(0, |c| c.0);
        self.seek(start);
        self.status = format!("LOOP: BACK TO {:.4} S (THE SHOT RE-RUN; TISSUE DETAIL CAN DIFFER)", self.shot_t);
    }

    // ── The interface ──────────────────────────────────────────────────────────────────────────

    fn dot(&mut self, p: [f32; 2], size: f32, color: [f32; 4]) {
        let aspect = self.win[0] / self.win[1].max(1.0);
        self.r.fill_rect(p[0] - size * 0.5 / aspect, p[1] - size * 0.5, size / aspect, size, color);
    }

    fn line(&mut self, a: Vec3, b: Vec3, color: [f32; 4]) {
        for k in 0..=40 {
            if let Some(p) = self.project(a.lerp(b, k as f32 / 40.0)) {
                self.dot(p, 0.002, color);
            }
        }
    }

    /// The shot's gizmo (its line in from 6 m out, the entry and aim points) and the section's
    /// outline and rotation handle, over the viewport.
    fn draw_gizmos(&mut self) {
        if let Some(p) = self.orbit_pivot.and_then(|p| self.project(p)) {
            self.dot(p, 0.008, INK);
            self.dot(p, 0.005, BLACK);
        }
        // The shot's gizmo shows only while the SHOT tab is open (owner, 2026-11-03).
        if self.open[0] {
        let (aim, dir) = self.aim_line();
        let entry = self.entry();
        let start = entry.map_or(aim, |(p, _)| p);
        self.line(start, aim, [0.6, 0.6, 0.6, 0.7]);
        if let Some(p) = self.project(aim) {
            self.dot(p, 0.010, INK);
        }
        // The curves themselves are the handles. Separate radii keep the elevation arc
        // independently accessible even for a level shot (where the old dots coincided).
        for which in 0..2 {
            let points = self.shot_curve(which);
            let colour = if which == 0 { GREEN } else { RED };
            for pair in points.windows(2) {
                if let (Some(a), Some(b)) = (self.project(pair[0]), self.project(pair[1])) {
                    self.screen_segment(a, b, colour);
                }
            }
        }
        let ((_, el_point), _) = self.shot_frames(aim, dir);
        self.line(el_point, aim, [0.75, 0.75, 0.75, 0.9]);
        if let Some((e, n)) = entry {
            if let Some(p) = self.project(e) {
                self.dot(p, 0.009, [0.95, 0.15, 0.1, 1.0]);
            }
            self.line(e, e + n * 0.6, [0.6, 0.6, 0.6, 0.8]);
        }
        }
        if let Some(plane) = self.section_plane() {
            let n = plane.truncate();
            let centre = self.g.player.hull_model().transform_point3(SECTION_PIVOT);
            let on = centre - n * (n.dot(centre) - plane.w);
            let u = n.cross(if n.y.abs() > 0.9 { Vec3::X } else { Vec3::Y }).normalize_or_zero();
            let v = n.cross(u).normalize_or_zero();
            let c = [0.85, 0.85, 0.85, 0.6];
            let k = 3.5;
            let corners = [on + (u + v) * k, on + (u - v) * k, on + (-u - v) * k, on + (-u + v) * k];
            for i in 0..4 {
                self.line(corners[i], corners[(i + 1) % 4], c);
            }
            self.line(on, on + n * 0.8, c);
            let hull = self.g.player.hull_model();
            for (axis, colour) in [(Vec3::X, RED), (Vec3::Y, GREEN), (Vec3::Z, BLUE)] {
                self.line(on, on + hull.transform_vector3(axis).normalize_or_zero() * 0.4, colour);
            }
            self.draw_handle(on);
        }
    }

    /// The shot's handle points (bearing on the ring, elevation on the arc) and the frames they
    /// move in: the horizontal from-direction, the hull's up, X and Z (world).
    fn shot_frames(&self, aim: Vec3, dir: Vec3) -> ((Vec3, Vec3), (Vec3, Vec3, Vec3, Vec3)) {
        let hull = self.g.player.hull_model();
        let up = hull.transform_vector3(Vec3::Y).normalize_or_zero();
        let (x_axis, z_axis) = (hull.transform_vector3(Vec3::X).normalize_or_zero(), hull.transform_vector3(Vec3::Z).normalize_or_zero());
        let from = -dir;
        let h = (from - up * from.dot(up)).try_normalize().unwrap_or(z_axis);
        ((aim + h * SHOT_HANDLE_M, aim + from * SHOT_HANDLE_M), (h, up, x_axis, z_axis))
    }

    fn shot_curve(&self, which: usize) -> Vec<Vec3> {
        let (aim, dir) = self.aim_line();
        let (_, (h, up, x, z)) = self.shot_frames(aim, dir);
        (0..=128).map(|k| {
            let t = k as f32 / 128.0;
            if which == 0 {
                let a = t * std::f32::consts::TAU;
                aim + (x * a.sin() + z * a.cos()) * SHOT_HANDLE_M
            } else {
                let a = (-60.0 + 140.0 * t).to_radians();
                aim + (h * a.cos() + up * a.sin()) * (SHOT_HANDLE_M * 1.22)
            }
        }).collect()
    }

    fn screen_segment(&mut self, a: [f32; 2], b: [f32; 2], colour: [f32; 4]) {
        let length = ((b[0] - a[0]) * self.win[0]).hypot((b[1] - a[1]) * self.win[1]);
        let steps = (length / 1.5).ceil().clamp(1.0, 512.0) as usize;
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            self.dot([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
                2.5 / self.win[1].max(1.0), colour);
        }
    }

    /// Pick the nearest visible curve segment, using a constant pixel hit width.
    fn shot_handle_hit(&self, c: [f32; 2]) -> Option<usize> {
        if !self.open[0] { return None; }
        let pixels = |p: [f32; 2]| Vec2::new(p[0] * self.win[0], p[1] * self.win[1]);
        let c = pixels(c);
        let mut best = (9.0f32, None);
        for which in 0..2 {
            for pair in self.shot_curve(which).windows(2) {
                if let (Some(a), Some(b)) = (self.project(pair[0]), self.project(pair[1])) {
                    let (a, b) = (pixels(a), pixels(b));
                    let ab = b - a;
                    let t = ((c - a).dot(ab) / ab.length_squared().max(1e-8)).clamp(0.0, 1.0);
                    let d = c.distance(a + ab * t);
                    if d < best.0 { best = (d, Some(which)); }
                }
            }
        }
        best.1
    }

    /// A shot handle dragged to `c`: the bearing from where the cursor's ray meets the hull's
    /// horizontal plane through the aim point, or the elevation from where it meets the shot's
    /// vertical plane.
    fn shot_cursor_angle(&self, which: usize, c: [f32; 2]) -> Option<f32> {
        let (aim, dir) = self.aim_line();
        let (_, (h, up, _, _)) = self.shot_frames(aim, dir);
        let (o, d) = self.ray(c);
        let normal = if which == 0 { up } else { h.cross(up).normalize_or_zero() };
        let denom = d.dot(normal);
        if denom.abs() < 1.0e-4 { return None; }
        let t = (aim - o).dot(normal) / denom;
        if t <= 0.0 { return None; }
        let v = o + d * t - aim;
        if which == 0 {
            let local = self.g.player.hull_model().inverse().transform_vector3(v);
            (local.x.hypot(local.z) > 1.0e-3).then(|| local.x.atan2(local.z).to_degrees())
        } else {
            Some(v.dot(up).atan2(v.dot(h)).to_degrees())
        }
    }

    fn drag_shot_handle(&mut self, which: usize, offset: f32, c: [f32; 2]) {
        let Some(angle) = self.shot_cursor_angle(which,c) else { return };
        if which == 0 { self.shot.from_az = (angle + offset + 180.0).rem_euclid(360.0)-180.0; }
        else { self.shot.from_el = (angle + offset).clamp(-60.0,80.0); }
        self.status = format!("SHOT FROM BEARING {:+.0}, ELEVATION {:+.0}", self.shot.from_az, self.shot.from_el);
    }

    /// The section in the shot's plane: vertical, through the aim point and along the shot's line,
    /// the half on the camera's side cut away.
    fn snap_section_to_shot(&mut self) {
        let hull = self.g.player.hull_model();
        let inv = hull.inverse();
        let (aim, _) = self.aim_line();
        let aim_h = inv.transform_point3(aim);
        let d = self.shot.dir_hull();
        let mut n = d.cross(Vec3::Y);
        if n.length_squared() < 1.0e-6 {
            n = Vec3::X;
        }
        let mut n = n.normalize();
        if n.dot(inv.transform_point3(self.eye) - aim_h) < 0.0 {
            n = -n;
        }
        self.section.set_normal_hull(n);
        self.section.offset = n.dot(aim_h - SECTION_PIVOT);
        self.section.on = true;
        self.status = "SECTION IN THE SHOT'S PLANE (THE CAMERA'S SIDE CUT AWAY)".into();
    }

    /// REC: start or stop writing the viewport to a video in the captures folder.
    fn toggle_recording(&mut self) {
        if let Some(mut rec) = self.recorder.take() {
            drop(rec.encoder.stdin.take());
            let ok = rec.encoder.wait().is_ok_and(|s| s.success());
            self.status = format!("RECORDED {:.1} S TO {}{}", rec.frames as f32 / 30.0, rec.path, if ok { "" } else { " (ENCODER FAILED)" });
            println!("[fea-range] {}", self.status);
            return;
        }
        let (_, w, h) = self.crop_rgba(&vec![0u8; (self.r.iw * self.r.ih * 4) as usize]);
        let k = display_scale(self.r.ih);
        let path = crate::capture_target(&format!("fea_range_rec_{}.mp4", capture_stamp()));
        let size = format!("{w}x{h}");
        let scale = format!("scale=iw*{k}:ih*{k}:flags=neighbor,crop=trunc(iw/2)*2:trunc(ih/2)*2");
        match std::process::Command::new("nice")
            .args(["-n", "10", "ffmpeg", "-v", "error", "-nostdin", "-y", "-f", "rawvideo", "-pixel_format", "rgba",
                "-video_size", &size, "-framerate", "30", "-i", "pipe:0", "-an", "-vf", &scale, "-c:v", "libx264", "-preset",
                "medium", "-crf", "10", "-pix_fmt", "yuv444p", "-movflags", "+faststart", &path])
            .stdin(std::process::Stdio::piped())
            .spawn() {
            Ok(encoder) => {
                self.status = format!("RECORDING THE VIEWPORT TO {path} ({}x{}, x{k} NEAREST)", w * k, h * k);
                self.recorder = Some(Recorder { path, size: (w, h), encoder, frames: 0 });
            }
            Err(e) => self.status = format!("REC: FFMPEG DID NOT START: {e}"),
        }
    }

    /// The present viewport frame into the recording (captured now if this frame has none).
    fn record_frame(&mut self) {
        let Some(size) = self.recorder.as_ref().map(|r| r.size) else { return };
        if !self.captured {
            self.pose_view();
            if let Ok(rgba) = self.r.render_to_rgba(&self.device, &self.queue, self.r.iw, self.r.ih) {
                let (rgba, w, h) = self.crop_rgba(&rgba);
                self.upload_image(&rgba, w, h);
                self.captured = true;
                self.last_rgba = Some((rgba, w, h));
            }
        }
        let Some((rgba, w, h)) = &self.last_rgba else { return };
        if (*w, *h) != size {
            self.toggle_recording();
            self.status = "REC STOPPED: THE VIEWPORT CHANGED SIZE".into();
            return;
        }
        use std::io::Write;
        let ok = self.recorder.as_mut().is_some_and(|rec| {
            let written = rec.encoder.stdin.as_mut().is_some_and(|i| i.write_all(rgba).is_ok());
            rec.frames += written as u64;
            written
        });
        if !ok {
            self.toggle_recording();
        }
    }

    /// STILL: the present state held while the renderer's own transport converges (48 frames from
    /// a restarted history, the eye's exposure metered), then the whole internal frame written at
    /// the Studio Display's size by nearest neighbour (game pixels kept).
    fn still(&mut self) {
        self.pose_view();
        self.r.reset_history();
        let started = std::time::Instant::now();
        if let Err(e) = crate::converge_headless_screenshot(&mut self.r, &self.device, &self.queue, 48) {
            self.status = format!("STILL FAILED: {e:#}");
            return;
        }
        let k = display_scale(self.r.ih);
        let path = crate::capture_target(&format!("fea_range_still_{}.png", capture_stamp()));
        self.status = match self.r.render_to_png(&self.device, &self.queue, &path, self.r.iw * k, self.r.ih * k) {
            Ok(()) => format!("STILL {}x{} TO {path} ({:.1} S)", self.r.iw * k, self.r.ih * k, started.elapsed().as_secs_f32()),
            Err(e) => format!("STILL FAILED: {e:#}"),
        };
        println!("[fea-range] {}", self.status);
    }

    /// The crewman whose body the camera is inside: within 0.3 m of the line from his torso up
    /// to his head (0.6 m along the hull's up).
    fn crew_at_eye(&self) -> Option<CrewRole> {
        let up = self.g.player.hull_model().transform_vector3(Vec3::Y).normalize_or_zero();
        CrewRole::ALL.iter().copied().filter_map(|role| {
            let torso = self.g.crew_solo_man_point(role);
            let along = (self.eye - torso).dot(up);
            let off = (self.eye - torso - up * along.clamp(0.0, 0.6)).length();
            (along > -0.35 && along < 0.75 && off < 0.3).then_some((role, off))
        }).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(role, _)| role)
    }

    /// BATTLE mode on or off: a fresh Tiger, with the placed T-34s live (held) or with none.
    fn set_battle(&mut self, on: bool) {
        self.battle = on;
        self.stop_battle();
        self.run = None;
        self.frame_at = None;
        self.playing = !on;
        self.fresh();
        self.status = if on {
            "BATTLE: CLICK THE MAP FOR A T-34, DRAG TO MOVE, CTRL+CLICK A ROUTE POINT, DELETE REMOVES, T PLACES THE TIGER".into()
        } else {
            "RANGE".into()
        };
    }

    fn stop_battle(&mut self) {
        self.pilot = None;
        self.player_control = false;
        self.play_paused = false;
    }

    /// WATCH (the Tiger's AI crew, `CanyonPilot`, along the route: the drawn one, else the
    /// course's) or PLAY (the player at the controls): a fresh Tiger and the placed T-34s, live.
    fn start_battle(&mut self, player: bool) {
        self.stop_battle();
        self.run = None;
        self.frame_at = None;
        self.fresh();
        let at = Vec2::new(self.g.player.pos.x, self.g.player.pos.z);
        if player {
            self.player_control = true;
            self.g.jump_station(Station::Commander);
            self.status = "PLAYING: WASD DRIVE, MOUSE LOOK, ARROWS TRAVERSE, LMB FIRE, 1-5 STATIONS, ESC PAUSES".into();
        } else {
            let pilot = if self.route.len() >= 2 {
                let mut route = vec![at];
                route.extend(self.route.iter().copied());
                CanyonPilot::from_route(route, at)
            } else if self.g.canyon_spec.is_some() {
                CanyonPilot::new(&self.g)
            } else {
                let target = if self.t34s.is_empty() { at + Vec2::new(0.0, 200.0) }
                    else { self.t34s.iter().map(|t| t.0).sum::<Vec2>() / self.t34s.len() as f32 };
                CanyonPilot::from_route(vec![at, at.lerp(target, 0.5), target], at)
            };
            self.pilot = Some(pilot);
            self.pilot_t = 0.0;
            self.status = format!("WATCHING: THE TIGER'S AI CREW AGAINST {} T-34S", self.t34s.len().min(self.g.enemies.len()));
        }
        self.playing = true;
        self.mode = Mode::Live;
    }

    /// The placed T-34 under `c` (window), if any.
    fn t34_at(&self, c: [f32; 2]) -> Option<usize> {
        let aspect = self.win[0] / self.win[1].max(1.0);
        self.t34s.iter().enumerate().filter_map(|(i, (p, _))| {
            let w = Vec3::new(p.x, self.g.hf.height_at(p.x, p.y) + 1.2, p.y);
            self.project(w).map(|s| (i, ((s[0] - c[0]) * aspect).hypot(s[1] - c[1])))
        }).filter(|(_, d)| *d < 0.03).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
    }

    /// A T-34 on the snow under `c`, facing the Tiger.
    fn place_t34(&mut self, c: [f32; 2]) {
        if let Some(p) = self.ground_at(c) {
            let at = Vec2::new(p.x, p.z);
            let tiger = Vec2::new(self.g.player.pos.x, self.g.player.pos.z);
            let to = tiger - at;
            self.t34s.push((at, to.x.atan2(to.y)));
            self.g.range_stage_battle(&self.t34s);
            self.status = format!("T-34 {} AT X {:.0} Z {:.0}, {:.0} M FROM THE TIGER", self.t34s.len(), at.x, at.y, to.length());
        }
    }

    fn move_t34(&mut self, i: usize, c: [f32; 2]) {
        if let (Some(p), Some(slot)) = (self.ground_at(c), self.t34s.get(i).copied()) {
            let at = Vec2::new(p.x, p.z);
            let tiger = Vec2::new(self.g.player.pos.x, self.g.player.pos.z);
            let to = tiger - at;
            self.t34s[i] = (at, if to.length() > 1.0 { to.x.atan2(to.y) } else { slot.1 });
            self.g.range_stage_battle(&self.t34s);
        }
    }

    fn add_route_point(&mut self, c: [f32; 2]) {
        if let Some(p) = self.ground_at(c) {
            self.route.push(Vec2::new(p.x, p.z));
            self.status = format!("ROUTE POINT {}", self.route.len());
        }
    }

    /// Delete: the T-34 or route point nearest the cursor.
    fn delete_near(&mut self, c: [f32; 2]) {
        if let Some(i) = self.t34_at(c) {
            self.t34s.remove(i);
            self.g.range_stage_battle(&self.t34s);
            self.status = format!("T-34 REMOVED ({} LEFT)", self.t34s.len());
            return;
        }
        let aspect = self.win[0] / self.win[1].max(1.0);
        let near = self.route.iter().enumerate().filter_map(|(i, p)| {
            self.project(Vec3::new(p.x, self.g.hf.height_at(p.x, p.y) + 0.3, p.y))
                .map(|s| (i, ((s[0] - c[0]) * aspect).hypot(s[1] - c[1])))
        }).filter(|(_, d)| *d < 0.03).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i);
        if let Some(i) = near {
            self.route.remove(i);
            self.status = format!("ROUTE POINT REMOVED ({} LEFT)", self.route.len());
        }
    }

    fn save_layout(&mut self) {
        let mut layouts: Vec<BattleLayout> = std::fs::read_to_string(BATTLE_LAYOUTS).ok()
            .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let name = format!("layout {}", layouts.len() + 1);
        layouts.push(BattleLayout { name: name.clone(), tiger_at: self.tiger_at.to_array(), tiger_heading: self.tiger_heading,
            t34s: self.t34s.iter().map(|(p, h)| [p.x, p.y, *h]).collect(), route: self.route.iter().map(|p| p.to_array()).collect() });
        self.status = match std::fs::write(BATTLE_LAYOUTS, serde_json::to_string_pretty(&layouts).unwrap_or_default()) {
            Ok(()) => format!("{} SAVED TO {BATTLE_LAYOUTS}", name.to_uppercase()),
            Err(e) => format!("LAYOUT NOT SAVED: {e}"),
        };
        self.layout_at = Some(layouts.len() - 1);
    }

    fn load_layout(&mut self, dir: i32) {
        let layouts: Vec<BattleLayout> = std::fs::read_to_string(BATTLE_LAYOUTS).ok()
            .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if layouts.is_empty() {
            self.status = "NO SAVED LAYOUTS (SAVE LAYOUT KEEPS ONE)".into();
            return;
        }
        let n = layouts.len() as i32;
        let i = self.layout_at.map_or(if dir > 0 { 0 } else { n - 1 }, |i| (i as i32 + dir).rem_euclid(n)) as usize;
        let l = &layouts[i];
        self.tiger_at = Vec2::from_array(l.tiger_at);
        self.tiger_heading = l.tiger_heading;
        self.t34s = l.t34s.iter().map(|t| (Vec2::new(t[0], t[1]), t[2])).collect();
        self.route = l.route.iter().map(|p| Vec2::from_array(*p)).collect();
        self.layout_at = Some(i);
        let name = l.name.to_uppercase();
        self.stop_battle();
        self.playing = false;
        self.fresh();
        self.status = format!("{name} LOADED");
    }

    /// A ring on the ground about `centre` (a live object's mark in the battle editor).
    fn ground_ring(&mut self, centre: Vec2, radius: f32, colour: [f32; 4]) {
        for k in 0..72 {
            let a = k as f32 / 72.0 * std::f32::consts::TAU;
            let p = centre + Vec2::new(a.sin(), a.cos()) * radius;
            if let Some(s) = self.project(Vec3::new(p.x, self.g.hf.height_at(p.x, p.y) + 0.08, p.y)) {
                self.dot(s, 0.004, colour);
            }
        }
    }

    /// The battle's marks in edit (none while watching or playing): a white ring at the Tiger's
    /// base, a red ring at each T-34's with its number and heading, and the Tiger's route.
    fn draw_battle_marks(&mut self) {
        let tiger = Vec2::new(self.g.player.pos.x, self.g.player.pos.z);
        self.ground_ring(tiger, 4.2, INK);
        let marks: Vec<(Vec2, f32)> = self.t34s.clone();
        for (i, (p, h)) in marks.into_iter().enumerate() {
            self.ground_ring(p, 3.8, RED);
            let ahead = p + Vec2::new(h.sin(), h.cos()) * 5.0;
            let (a, b) = (Vec3::new(p.x, self.g.hf.height_at(p.x, p.y) + 0.1, p.y),
                Vec3::new(ahead.x, self.g.hf.height_at(ahead.x, ahead.y) + 0.1, ahead.y));
            self.line(a, b, RED);
            if let Some(s) = self.project(Vec3::new(p.x, self.g.hf.height_at(p.x, p.y) + 3.4, p.y)) {
                self.text_bold(&format!("{}", i + 1), s[0], s[1], TEXT * 0.8, RED, crate::ui::Align::Center);
            }
        }
        let mut last = Vec3::new(tiger.x, self.g.player.pos.y + 0.3, tiger.y);
        let points: Vec<Vec3> = self.route.iter().map(|p| Vec3::new(p.x, self.g.hf.height_at(p.x, p.y) + 0.3, p.y)).collect();
        for p in points {
            self.line(last, p, INK);
            if let Some(s) = self.project(p) {
                self.dot(s, 0.008, INK);
            }
            last = p;
        }
    }

    /// The viewport's height: the whole window.
    fn view_h(&self) -> f32 {
        1.0
    }

    /// The viewport's left edge: the window's (the bar lies over the view's foot).
    fn view_x(&self) -> f32 {
        0.0
    }

    /// Whether `c` is on the panel's hide (or show) button.
    fn on_hide_button(&self, c: [f32; 2]) -> bool {
        let [x, y, w, h] = hide_rect(self.hide_ui);
        c[0] >= x && c[0] <= x + w && c[1] >= y && c[1] <= y + h
    }

    /// Whether `c` is on the panel.
    fn in_panel(&self, c: [f32; 2]) -> bool {
        !self.hide_ui && (c[1] >= BAR_TOP || self.columns().iter().any(|&(_, x, top)|
            c[0] >= x && c[0] <= x + COLS_W && c[1] >= top - 0.004))
    }

    /// The timeline line's height: the window's bottom, across the viewport.
    fn timeline_y(&self) -> f32 {
        1.0
    }

    /// Frame the Tiger: the camera keeps its direction and stands 10 m from the hull's middle.
    fn frame_tiger(&mut self) {
        let target = self.g.player.hull_model().transform_point3(SECTION_PIVOT);
        self.eye = target - self.forward() * 10.0;
        self.orbit_pivot = Some(target);
        self.keep_above_ground();
        self.camera_at = None;
        self.status = "CENTRED ON THE TIGER (THE ORBIT POINT ITS MIDDLE)".into();
    }

    /// Every dial's value as the editor starts (right-click resets a dial to it).
    fn dial_values(&self) -> Vec<(Knob, f32)> {
        (0..GROUPS.len()).flat_map(group_rows).filter(|k| kind(*k) == Kind::Dial).map(|k| (k, self.dial_get(k))).collect()
    }

    fn reset_dial(&mut self, knob: Knob) {
        if let Some(&(_, v)) = self.dial_defaults.iter().find(|(k, _)| *k == knob) {
            self.dial_set(knob, v);
            let (label, value) = self.knob_label(knob);
            self.status = format!("{label} RESET TO {value}");
        }
    }

    /// The editor's settings (`STATE_PATH`), kept between launches.
    fn state_json(&self) -> serde_json::Value {
        let d = self.tuning_display();
        let t = &self.tuning;
        serde_json::json!({
            "shot": {"shell": self.shot.shell, "range_m": self.shot.range_m, "from_az": self.shot.from_az, "from_el": self.shot.from_el,
                "seed": self.shot.seed, "target": match self.shot.target {
                    Target::Point(p) => serde_json::json!({"point": p.to_array()}),
                    Target::Man(role) => serde_json::json!({"man": role}) }},
            "scene": {"racks_loaded": self.scene.racks_loaded, "cabin_lamps": self.scene.cabin_lamps,
                "fresh_each_run": self.scene.fresh_each_run, "slow_s": self.scene.slow_s, "run_s": self.scene.run_s},
            "section": {"on": self.section.on, "yaw": self.section.yaw, "pitch": self.section.pitch, "offset": self.section.offset},
            "sky": {"day": self.sky.day, "local_h": self.sky.local_h, "weather": self.sky.weather, "exposure": self.sky.exposure,
                "running": self.sky.running, "rate": self.sky.rate},
            "tissue": {"tear_j_m3": t.tear_j_m3, "critical_stretch": t.critical_stretch, "forward": t.forward,
                "outward": t.outward, "active_reach_m": t.active_reach_m, "landing_breakup": t.landing_breakup, "spread": t.spread},
            "look": {"look": self.r.look, "grade": self.r.display_grade, "dither": self.r.display_dither_pattern,
                "palette_levels": d.palette_levels, "ordered_dither": d.ordered_dither, "ldr_dither": d.ldr_dither,
                "retro": d.retro_strength, "white_balance": d.ambient_white_balance,
                "white_balance_strength": d.ambient_white_balance_strength, "tuned": self.r.display_tuning.is_some(), "fog": self.fog,
                "eye_fx": {"concussion": self.eye_fx.concussion, "flash": self.eye_fx.flash}},
            "effects": {"smoke_density": review_smoke_density(), "smoke_delay": review_smoke_delay()},
            "lamp_gain": review_lamp_gain(), "time_speed": self.time_speed, "looping": self.looping,
            "engine": self.engine, "accumulate_moving": self.accumulate_moving, "wear": self.wear, "snow": self.snow,
            "camera": {"eye": self.eye.to_array(), "yaw": self.yaw, "pitch": self.pitch, "focal_mm": self.focal_mm, "speed": self.speed},
            "tiger": {"at": self.tiger_at.to_array(), "heading": self.tiger_heading},
            "battle": {"on": self.battle, "t34s": self.t34s.iter().map(|(p, h)| [p.x, p.y, *h]).collect::<Vec<_>>(),
                "route": self.route.iter().map(|p| p.to_array()).collect::<Vec<_>>()},
            "open": self.open,
        })
    }

    fn save_state(&self) {
        let _ = std::fs::write(STATE_PATH, serde_json::to_string_pretty(&self.state_json()).unwrap_or_default());
    }

    /// Settings from the last launch (whatever is missing or unreadable keeps its default).
    fn load_state(&mut self) {
        let Some(v) = std::fs::read_to_string(STATE_PATH).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return };
        self.apply_state(&v);
    }

    fn apply_state(&mut self, v: &serde_json::Value) {
        let f = |v: &serde_json::Value| v.as_f64().map(|x| x as f32);
        let arr4 = |v: &serde_json::Value| -> Option<[f32; 4]> {
            let a: Vec<f32> = v.as_array()?.iter().filter_map(|x| x.as_f64().map(|x| x as f32)).collect();
            (a.len() == 4).then(|| [a[0], a[1], a[2], a[3]])
        };
        let s = &v["shot"];
        if let Some(x) = s["shell"].as_u64() { self.shot.shell = (x as usize).min(shells().len() - 1); }
        if let Some(x) = f(&s["range_m"]) { self.shot.range_m = x; }
        if let Some(x) = f(&s["from_az"]) { self.shot.from_az = x; }
        if let Some(x) = f(&s["from_el"]) { self.shot.from_el = x; }
        if let Some(x) = s["seed"].as_u64() { self.shot.seed = x as u32; }
        if let Some(p) = s["target"]["point"].as_array().filter(|p| p.len() == 3) {
            self.shot.target = Target::Point(Vec3::new(f(&p[0]).unwrap_or(0.0), f(&p[1]).unwrap_or(0.0), f(&p[2]).unwrap_or(0.0)));
        } else if let Ok(role) = serde_json::from_value::<CrewRole>(s["target"]["man"].clone()) {
            self.shot.target = Target::Man(role);
        }
        if let Some(density) = f(&v["effects"]["smoke_density"]) {
            set_review_smoke(density, f(&v["effects"]["smoke_delay"]));
        }
        let sc = &v["scene"];
        if let Some(x) = sc["racks_loaded"].as_bool() { self.scene.racks_loaded = x; }
        if let Some(x) = sc["cabin_lamps"].as_bool() { self.scene.cabin_lamps = x; }
        if let Some(x) = sc["fresh_each_run"].as_bool() { self.scene.fresh_each_run = x; }
        if let Some(x) = f(&sc["slow_s"]) { self.scene.slow_s = x; }
        if let Some(x) = f(&sc["run_s"]) { self.scene.run_s = x; }
        let se = &v["section"];
        if let Some(x) = se["on"].as_bool() { self.section.on = x; }
        if let Some(x) = f(&se["yaw"]) { self.section.yaw = x; }
        if let Some(x) = f(&se["pitch"]) { self.section.pitch = x; }
        self.section.snap();
        if let Some(x) = f(&se["offset"]) { self.section.offset = x; }
        let b = &v["battle"];
        if let Some(x) = b["on"].as_bool() { self.battle = x; }
        if let Some(list) = b["t34s"].as_array() {
            self.t34s = list.iter().filter_map(|t| {
                let a = t.as_array()?;
                Some((Vec2::new(f(a.first()?)?, f(a.get(1)?)?), f(a.get(2)?)?))
            }).collect();
        }
        if let Some(list) = b["route"].as_array() {
            self.route = list.iter().filter_map(|t| {
                let a = t.as_array()?;
                Some(Vec2::new(f(a.first()?)?, f(a.get(1)?)?))
            }).collect();
        }
        let sk = &v["sky"];
        if let Some(x) = sk["day"].as_i64() { self.sky.day = x as i32; }
        if let Some(x) = f(&sk["local_h"]) { self.sky.local_h = x.rem_euclid(24.0); }
        if let Some(x) = f(&sk["weather"]) { self.sky.weather = x; }
        self.sky.exposure = f(&sk["exposure"]);
        if let Some(x) = sk["running"].as_bool() { self.sky.running = x; }
        if let Some(x) = f(&sk["rate"]) { self.sky.rate = x; }
        let t = &v["tissue"];
        if let (Some(a), Some(b)) = (arr4(&t["tear_j_m3"]), arr4(&t["critical_stretch"])) {
            self.tuning.tear_j_m3 = a;
            self.tuning.critical_stretch = b;
        }
        if let Some(x) = f(&t["forward"]) { self.tuning.forward = x; }
        if let Some(x) = f(&t["outward"]) { self.tuning.outward = x; }
        if let Some(x) = f(&t["active_reach_m"]) { self.tuning.active_reach_m = x; }
        if let Some(x) = f(&t["landing_breakup"]) { self.tuning.landing_breakup = x; }
        if let Some(x) = f(&t["spread"]) { self.tuning.spread = x; }
        self.apply_tuning();
        let l = &v["look"];
        if let Some(x) = f(&l["look"]) { self.r.look = x; }
        if let Some(x) = arr4(&l["grade"]) { self.r.display_grade = x; }
        let fx = &l["eye_fx"];
        if let Some(x) = fx["concussion"].as_bool() { self.eye_fx.concussion = x; }
        if let Some(x) = fx["flash"].as_bool() { self.eye_fx.flash = x; }
        if let Some(x) = f(&l["dither"]) { self.r.display_dither_pattern = x; }
        if l["tuned"].as_bool() == Some(true) {
            let mut d = self.tuning_display();
            if let Some(x) = f(&l["palette_levels"]) { d.palette_levels = x; }
            if let Some(x) = f(&l["ordered_dither"]) { d.ordered_dither = x; }
            if let Some(x) = f(&l["ldr_dither"]) { d.ldr_dither = x; }
            if let Some(x) = f(&l["retro"]) { d.retro_strength = x; }
            if let Some(x) = l["white_balance"].as_bool() { d.ambient_white_balance = x; }
            if let Some(x) = f(&l["white_balance_strength"]) { d.ambient_white_balance_strength = x; }
            self.r.display_tuning = Some(d);
        }
        self.fog = f(&l["fog"]);
        if let Some(x) = f(&v["lamp_gain"]) { set_review_lamp_gain(x.max(1.0)); }
        if let Some(x) = f(&v["time_speed"]) { self.time_speed = x; }
        if let Some(x) = v["engine"].as_bool() { self.engine = x; }
        if let Some(x) = v["accumulate_moving"].as_bool() { self.accumulate_moving = x; }
        self.wear = arr4(&v["wear"]);
        self.snow = v["snow"].as_array().filter(|a| a.len() == 2).map(|a| [f(&a[0]).unwrap_or(0.0), f(&a[1]).unwrap_or(0.0)]);
        let c = &v["camera"];
        if let Some(e) = c["eye"].as_array().filter(|e| e.len() == 3) {
            self.eye = Vec3::new(f(&e[0]).unwrap_or(0.0), f(&e[1]).unwrap_or(0.0), f(&e[2]).unwrap_or(0.0));
            if let (Some(y), Some(p)) = (f(&c["yaw"]), f(&c["pitch"])) { self.yaw = y; self.pitch = p; }
        }
        if let Some(x) = f(&c["focal_mm"]) { self.focal_mm = x; }
        if let Some(x) = f(&c["speed"]) { self.speed = x.clamp(0.05, 200.0); }
        if let Some(a) = v["tiger"]["at"].as_array().filter(|a| a.len() == 2) {
            self.tiger_at = Vec2::new(f(&a[0]).unwrap_or(TIGER_AT.x), f(&a[1]).unwrap_or(TIGER_AT.y));
        }
        if let Some(x) = f(&v["tiger"]["heading"]) { self.tiger_heading = x; }
        if let Some(o) = v["open"].as_array().filter(|o| o.len() == GROUPS.len()) {
            for (slot, x) in self.open.iter_mut().zip(o) { *slot = x.as_bool().unwrap_or(*slot); }
            // Normalize settings saved by builds which allowed several open panels.
            if let Some(first) = self.open.iter().position(|&open| open) {
                self.open = [false; GROUPS.len()]; self.open[first] = true;
            }
        }
        self.status = format!("SETTINGS FROM THE LAST LAUNCH ({STATE_PATH})");
    }

    /// The camera never below the snow: at least 0.15 m above the terrain under it.
    fn keep_above_ground(&mut self) {
        let floor = self.g.hf.height_at(self.eye.x, self.eye.z) + 0.15;
        if self.eye.y < floor {
            self.eye.y = floor;
        }
    }

    /// The point orbit, pan and dolly work about: the one set by a double click, else the point
    /// under the cursor.
    fn orbit_point(&self, c: [f32; 2]) -> Vec3 {
        self.orbit_pivot.unwrap_or_else(|| self.pivot_at(c))
    }

    /// Double click: the orbit point on the vehicle or ground under the cursor.
    fn set_orbit_point(&mut self, c: [f32; 2]) {
        let p = self.pivot_at(c);
        self.orbit_pivot = Some(p);
        self.status = format!("ORBIT POINT SET ({:.1}, {:.1}, {:.1}); DOUBLE-CLICK AGAIN TO MOVE IT", p.x, p.y, p.z);
    }

    /// The point a camera drag turns about or moves toward: the vehicle's surface under the
    /// cursor, else the ground, else 10 m ahead.
    fn pivot_at(&self, c: [f32; 2]) -> Vec3 {
        let (from, dir) = self.ray(c);
        let to = from + dir * 200.0;
        if let Some(contact) = self.g.cabin_fire.sweep(from, to, &self.g.fire_surface_models()) {
            return from.lerp(to, contact.fraction);
        }
        self.ground_at(c).unwrap_or(from + dir * 10.0)
    }

    fn set_view_forward(&mut self, f: Vec3) {
        let mut c = crate::camera::Camera::new(1.0);
        c.set_forward(f);
        self.yaw = c.yaw;
        self.pitch = c.pitch.clamp(-1.55, 1.55);
    }

    /// Orbit about `pivot` by a drag of (`dx`, `dy`) of the window: the eye and its view turn
    /// together (about the vertical, then about the view's right), so the pivot stays put.
    fn orbit(&mut self, pivot: Vec3, dx: f32, dy: f32) {
        let f = self.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let mut q = Quat::from_rotation_y(-dx * 3.0);
        let tilted = Quat::from_axis_angle(right, -dy * 3.0) * f;
        if tilted.y.abs() < 0.995 {
            q = q * Quat::from_axis_angle(right, -dy * 3.0);
        }
        self.eye = pivot + q * (self.eye - pivot);
        self.set_view_forward(q * f);
        self.keep_above_ground();
        self.camera_at = None;
    }

    /// Pan: the view slides across, the point under the cursor following it.
    fn pan(&mut self, pivot: Vec3, dx: f32, dy: f32) {
        let f = self.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(f).normalize_or_zero();
        let depth = (pivot - self.eye).dot(f).max(0.5);
        let span = 2.0 * depth * (self.fov_deg().to_radians() * 0.5).tan();
        let aspect = (1.0 - self.view_x()) * self.win[0] / (self.view_h() * self.win[1].max(1.0));
        self.eye += (-right * dx / (1.0 - self.view_x()) * aspect + up * dy / self.view_h()) * span;
        self.keep_above_ground();
        self.camera_at = None;
    }

    /// Dolly toward (or away from) the point: right or down moves in.
    fn dolly(&mut self, pivot: Vec3, dx: f32, dy: f32) {
        let to = pivot - self.eye;
        let d = to.length();
        if d < 1.0e-3 {
            return;
        }
        let step = ((dx + dy) * 3.0 * d).min(d - 0.2);
        self.eye += to / d * step;
        self.keep_above_ground();
        self.camera_at = None;
    }

    /// The axis label under `c`, if any (0 X, 1 Y, 2 Z).
    fn axis_hit(&self, c: [f32; 2]) -> Option<usize> {
        let aspect = self.win[0] / self.win[1].max(1.0);
        self.axis_labels.iter().position(|p| ((c[0] - p[0]) * aspect).hypot(c[1] - p[1]) < 0.018)
    }

    /// Look along the hull's axis from its positive side at the tank (again: from the other side),
    /// at the present distance.
    fn snap_view(&mut self, axis: usize) {
        let hull = self.g.player.hull_model();
        let target = hull.transform_point3(SECTION_PIVOT);
        let positive = self.snapped != Some((axis, true));
        let a = hull.transform_vector3([Vec3::X, Vec3::Y, Vec3::Z][axis]).normalize_or_zero();
        let side = if positive { a } else { -a };
        // A view straight down or up keeps a hair of tilt (the camera's yaw is undefined there).
        let view = (-side + if axis == 1 { hull.transform_vector3(Vec3::Z) * 0.02 } else { Vec3::ZERO }).normalize();
        let dist = self.eye.distance(target).clamp(6.0, 60.0);
        self.eye = target - view * dist;
        self.set_view_forward(view);
        self.keep_above_ground();
        self.snapped = Some((axis, positive));
        self.camera_at = None;
        self.status = format!("VIEW ALONG THE HULL'S {}{} AXIS", if positive { "+" } else { "-" }, ["X", "Y", "Z"][axis]);
    }

    /// The five crew as a plan of the tank seen from above, front up (the tank's left on the
    /// left): each man at his seat, white alive, amber wounded, dim amber out cold, red with a
    /// cross dead.
    fn draw_crew_plan(&mut self, at: [f32; 2], height: f32) {
        let aspect = self.win[0] / self.win[1].max(1.0);
        let w = height * 0.55 / aspect;
        let px = 1.0 / self.win[1].max(1.0);
        let outline = [0.35, 0.35, 0.35, 1.0];
        let frame = |r: &mut Renderer, x: f32, y: f32, fw: f32, fh: f32| {
            r.fill_rect(x, y, fw, px, outline);
            r.fill_rect(x, y + fh, fw, px, outline);
            r.fill_rect(x, y, px / aspect, fh, outline);
            r.fill_rect(x + fw, y, px / aspect, fh, outline);
        };
        frame(&mut self.r, at[0], at[1], w, height);
        frame(&mut self.r, at[0] + w * 0.18, at[1] + height * 0.30, w * 0.64, height * 0.44);
        let seats = [(CrewRole::Driver, 0.30, 0.15, "D"), (CrewRole::Radioman, 0.70, 0.15, "R"), (CrewRole::Gunner, 0.36, 0.40, "G"),
            (CrewRole::Commander, 0.34, 0.63, "C"), (CrewRole::Loader, 0.67, 0.52, "L")];
        for (role, u, v, letter) in seats {
            let man = self.g.crew_solo.man(role);
            let colour = if man.dead { RED } else if man.unconscious_s > 0.0 { [0.45, 0.2, 0.02, 1.0] }
                else if !man.wounds.is_empty() { AMBER } else { INK };
            let c = [at[0] + w * u, at[1] + height * v];
            let d = height * 0.16;
            self.r.fill_rect(c[0] - d * 0.5 / aspect, c[1] - d * 0.5, d / aspect, d, colour);
            self.text_bold(if man.dead { "X" } else { letter }, c[0], c[1], TEXT * 0.6, BLACK, crate::ui::Align::Center);
        }
    }

    /// The hull's axes as the camera sees them (X red, Y green, Z blue), in the viewport's corner;
    /// a label clicked looks along its axis.
    fn draw_axes_gizmo(&mut self) {
        let aspect = self.win[0] / self.win[1].max(1.0);
        let f = self.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(f).normalize_or_zero();
        // On the panel, right of the buttons; with the panel hidden, the viewport's corner.
        let centre = if self.hide_ui { [0.06 / aspect, 1.0 - 0.07] } else { [0.075, BAR_TOP - 0.055] };
        let hull = self.g.player.hull_model();
        for (axis, colour, name) in [(Vec3::X, RED, "X"), (Vec3::Y, GREEN, "Y"), (Vec3::Z, BLUE, "Z")] {
            let d = hull.transform_vector3(axis).normalize_or_zero();
            let (x, y) = (d.dot(right), -d.dot(up));
            for k in 1..=12 {
                let t = k as f32 / 12.0 * 0.04;
                self.dot([centre[0] + x * t / aspect, centre[1] + y * t], 0.004, colour);
            }
            let at = [centre[0] + x * 0.05 / aspect, centre[1] + y * 0.05];
            self.r.fill_rect(at[0] - 0.011 / aspect, at[1] - 0.011, 0.022 / aspect, 0.022, [0.0, 0.0, 0.0, 0.6]);
            self.text_bold(name, at[0], at[1], TEXT * 0.8, colour, crate::ui::Align::Center);
            self.axis_labels[["X", "Y", "Z"].iter().position(|n| *n == name).unwrap_or(0)] = at;
        }
    }

    /// Turn the section by whole 90° steps (axis to axis): `horizontal` about the hull's vertical (yaw),
    /// `vertical` tilting the normal (pitch, held within ±90°). The plane first snaps to the
    /// 45° grid, so free adjustments land back on it.
    fn rotate_section(&mut self, horizontal: i32, vertical: i32) {
        // Axis to axis: a quarter turn a step (sections are orthogonal only).
        let snap = |a: f32| (a / 90.0).round() * 90.0;
        self.section.on = true;
        self.section.yaw = ((snap(self.section.yaw) + 90.0 * horizontal as f32 + 180.0).rem_euclid(360.0)) - 180.0;
        self.section.pitch = (snap(self.section.pitch) + 90.0 * vertical as f32).clamp(-90.0, 90.0);
        self.section.snap();
        self.status = format!("SECTION NORMAL YAW {:+.0}, PITCH {:+.0} (HULL FRAME)", self.section.yaw, self.section.pitch);
    }

    /// The rotation handle's parts (window 0..1): centre pad, then left, right, up, down arrows.
    fn handle_parts(at: [f32; 2], aspect: f32) -> [([f32; 2], (i32, i32)); 5] {
        let (dx, dy) = (HANDLE_REACH / aspect, HANDLE_REACH);
        [(at, (0, 0)), ([at[0] - dx, at[1]], (-1, 0)), ([at[0] + dx, at[1]], (1, 0)), ([at[0], at[1] - dy], (0, 1)),
            ([at[0], at[1] + dy], (0, -1))]
    }

    /// Which part of the rotation handle is under `cursor`: Some((0, 0)) for the centre pad,
    /// else the step its arrow gives.
    fn handle_hit(&self, cursor: [f32; 2]) -> Option<(i32, i32)> {
        let at = self.handle_at?;
        let aspect = self.win[0] / self.win[1].max(1.0);
        Self::handle_parts(at, aspect).into_iter().find(|(p, _)| {
            (cursor[0] - p[0]).abs() * aspect <= HANDLE_BOX * 0.5 && (cursor[1] - p[1]).abs() <= HANDLE_BOX * 0.5
        }).map(|(_, step)| step)
    }

    /// The section's rotation handle at the plane's centre in the viewport (its lower right corner
    /// when that is out of view): a small pad, its four arrows only with the cursor near.
    fn draw_handle(&mut self, centre: Vec3) {
        let aspect = self.win[0] / self.win[1].max(1.0);
        let margin = (HANDLE_REACH + HANDLE_BOX) / aspect;
        let view_h = self.timeline_y();
        let at = self.project(centre).filter(|p| p[0] > margin && p[0] < 1.0 - margin
            && p[1] > HANDLE_REACH + HANDLE_BOX && p[1] < view_h - HANDLE_REACH - 2.0 * HANDLE_BOX)
            .unwrap_or([1.0 - margin - 0.01, view_h - HANDLE_REACH - 2.0 * HANDLE_BOX]);
        self.handle_at = Some(at);
        let near = ((self.cursor[0] - at[0]) * aspect).hypot(self.cursor[1] - at[1]) < HANDLE_NEAR;
        let w = 0.008;
        self.r.fill_rect(at[0] - w * 0.5 / aspect, at[1] - w * 0.5, w / aspect, w, INK);
        self.r.fill_rect(at[0] - w * 0.3 / aspect, at[1] - w * 0.3, w * 0.6 / aspect, w * 0.6, BLACK);
        if near {
            for (p, step) in Self::handle_parts(at, aspect).into_iter().skip(1) {
                let label = match step { (-1, 0) => "<", (1, 0) => ">", (0, 1) => "^", _ => "v" };
                self.text_bold(label, p[0], p[1], TEXT * 0.75, INK, crate::ui::Align::Center);
            }
        }
    }

    /// A line of text centred on `y` (the pixel font's `y` is near the glyphs' top: measured on the
    /// 2026-11-03 stills, regular glyphs sit about 0.46 of their size below it, bold 0.38).
    fn text(&mut self, s: &str, x: f32, y: f32, size: f32, color: [f32; 4], align: crate::ui::Align) {
        self.r.draw_text(s, x, y - 0.46 * size, size, color, align);
    }

    fn text_bold(&mut self, s: &str, x: f32, y: f32, size: f32, color: [f32; 4], align: crate::ui::Align) {
        self.r.draw_text_bold(s, x, y - 0.38 * size, size, color, align);
    }

    /// The open groups' columns: each group's index, its column's left edge and top. A column
    /// stands above its header, pushed right past the column before it (never off the window).
    fn columns(&self) -> Vec<(usize, f32, f32)> {
        let headers = self.header_rects();
        let mut next_x = 0.0f32;
        let mut out = Vec::new();
        for g in (0..GROUPS.len()).filter(|&g| self.open[g]) {
            let x = headers[g][0].max(next_x);
            let top = BAR_TOP - 0.006 - group_rows(g).len() as f32 * ROW_H;
            next_x = x + COLS_W + 0.004;
            out.push((g, x, top));
        }
        // Past the window's right edge, the whole row of columns moves left (as far as the window).
        let over = (next_x - 1.0).max(0.0);
        for c in &mut out {
            c.1 = (c.1 - over).max(0.004);
        }
        out
    }

    /// The open groups' rows: each row's knob, its column's left edge and its centre's height
    /// (headers are the bar's, `header_rects`).
    fn layout(&self) -> (Vec<(Knob, f32, f32)>, Vec<(usize, f32, f32)>) {
        let mut rows = Vec::new();
        for (g, x, top) in self.columns() {
            for (i, k) in group_rows(g).into_iter().enumerate() {
                rows.push((k, x, top + (i as f32 + 0.5) * ROW_H));
            }
        }
        (rows, Vec::new())
    }

    fn rows(&self) -> Vec<(Knob, f32, f32)> {
        self.layout().0
    }

    /// The row under `c`, with its column's left edge.
    fn row_at(&self, c: [f32; 2]) -> Option<(Knob, f32)> {
        if !self.in_panel(c) {
            return None;
        }
        self.rows().into_iter().find(|(_, x, y)| c[0] >= *x && c[0] <= x + COLS_W - 0.008 && (c[1] - y).abs() <= ROW_H * 0.5)
            .map(|(k, x, _)| (k, x))
    }

    /// The groups' headers on the bar (x, y, w, h), one a group.
    fn header_rects(&self) -> Vec<[f32; 4]> {
        let size = TEXT * 0.68;
        let mut x = HEADERS_X;
        GROUPS.iter().map(|name| {
            let w = (header_name(name).len() + 2) as f32 * size * self.win[1] / self.win[0].max(1.0) + 0.006;
            let rect = [x, BAR_ROW_Y, w, BAR_ROW_H];
            x += w + 0.003;
            rect
        }).collect()
    }

    /// A small dial at (`x`, `y`) (its centre), turned to `t` of its range.
    fn dial(&mut self, x: f32, y: f32, t: f32) {
        let aspect = self.win[0] / self.win[1].max(1.0);
        let d = ROW_H * 0.92;
        self.r.draw_image(self.dial_image, x - d * 0.5 / aspect, y - d * 0.5, d / aspect, d, INK);
        let angle = (-135.0 + 270.0 * t.clamp(0.0, 1.0)).to_radians();
        let (sx, sy) = (angle.sin(), -angle.cos());
        for k in 1..=3 {
            let r = d * 0.13 * k as f32;
            let s = d * 0.16;
            self.r.fill_rect(x + sx * r / aspect - s * 0.5 / aspect, y + sy * r - s * 0.5, s / aspect, s, INK);
        }
    }

    fn draw_ui(&mut self, cursor: [f32; 2]) {
        self.cursor = cursor;
        self.r.ui_reset();
        self.handle_at = None;
        let scrubbing = matches!(self.mode, Mode::Scrub(_));
        // The viewport: a dragged timeline's stored frame, or the live frame captured this frame
        // for the timeline (or, headless, the live frame); otherwise `render` blits the live view.
        // The timeline keeps no pictures: dragging it moves only the playhead; the view stays live.
        if self.captured || self.live_image {
            let x0 = self.view_x();
            self.r.draw_image(self.frame_image, x0, 0.0, 1.0 - x0, self.view_h(), [1.0, 1.0, 1.0, 1.0]);
        }
        // Playing the Tiger: the game's own view, nothing of the editor over it.
        if self.player_control && !self.play_paused {
            return;
        }
        if self.battle && self.pilot.is_none() && !self.player_control {
            self.draw_battle_marks();
        }
        if self.hide_ui {
            self.draw_timeline(cursor);
            let [x, y, w, h] = hide_rect(true);
            self.r.fill_rect(x, y, w, h, [0.0, 0.0, 0.0, 0.6]);
            self.text_bold("PANEL", x + w * 0.5, y + h * 0.5, TEXT * 0.75, INK, crate::ui::Align::Center);
            return;
        }
        if self.mode == Mode::Live && !self.battle {
            self.draw_gizmos();
        }
        self.r.fill_rect(0.0, BAR_TOP, 1.0, 1.0 - BAR_TOP, PANEL_FILL);
        self.draw_axes_gizmo();
        // The left block: title and time, the buttons, the status.
        let mode = match (self.mode, self.run) {
            (Mode::Scrub(i), _) => format!("TO FRAME {}", i + 1),
            (Mode::Live, _) if !self.playing => "PAUSED".into(),
            (Mode::Live, Some(run)) if !self.past_run => format!("RUN {}/{}", self.frame_at.map_or(0, |f| f + 1), run.total),
            (Mode::Live, _) => "REAL TIME".into(),
        };
        let t = match self.mode {
            Mode::Scrub(i) => self.frame_time(i as u32),
            _ => self.shot_t,
        };
        let chosen = if self.battle { 1 } else { 0 };
        for (m, [x, y, w, h]) in self.mode_tab_rects().into_iter().enumerate() {
            let (fill, ink) = if m == chosen { (INK, BLACK) } else if m == 2 { (BOX, [0.28, 0.28, 0.28, 1.0]) } else { (BOX, INK) };
            self.r.fill_rect(x, y, w, h, fill);
            self.text_bold(MODES[m], x + w * 0.5, y + h * 0.5, TEXT * 0.72, ink, crate::ui::Align::Center);
        }
        let mode = if self.battle {
            if self.player_control { if self.play_paused { "PLAYING, PAUSED".into() } else { "PLAYING".into() } }
            else if self.pilot.is_some() { if self.playing { "WATCHING".into() } else { "WATCHING, PAUSED".into() } }
            else { format!("EDIT: {} T-34S", self.t34s.len()) }
        } else { mode };
        let [hx, _, _, _] = hide_rect(false);
        self.text(&format!("{mode}  T {:.2} S   {:.0} FPS  SIM {:.0} MS  DRAW {:.0} MS   H: HELP", t, 1.0 / self.perf[0].max(1.0e-4),
            self.perf[1], self.perf[2]), hx - 0.008, BAR_TOP + 0.013, TEXT * 0.66, DIM, crate::ui::Align::Right);
        self.draw_crew_plan([LEFT_X, BAR_TOP - 0.088], 0.075);
        for (label, col, row) in self.buttons() {
            let [x, y, w, h] = button_rect(col, row);
            let lit = if self.battle {
                (label == "WATCH" && self.pilot.is_some()) || (label == "PLAY" && self.player_control) || (label == "REC" && self.recorder.is_some())
            } else {
                (label == "PLAY" && !self.playing) || (label == "REC" && self.recorder.is_some()) || (label == "LOOP" && self.looping)
            };
            let hover = cursor[0] >= x && cursor[0] <= x + w && cursor[1] >= y && cursor[1] <= y + h;
            let fill = if label == "FIRE" || (self.battle && label == "PLAY" && !lit) { if hover { [0.85, 0.12, 0.08, 1.0] } else { [0.62, 0.06, 0.04, 1.0] } }
                else if lit { INK } else if hover { [0.09, 0.09, 0.09, 1.0] } else { BOX };
            self.r.fill_rect(x, y, w, h, fill);
            if hover && !self.battle {
                let hint = match label {
                    "RESET" => "RESTORE A FRESH TANK AND CREW, PAUSED; KEEP CAMERA AND SHOT SETTINGS",
                    "FIRE" => "FIRE ONE SHELL NOW (F); KEEP ALL EXISTING DAMAGE",
                    "LIVE" => "INTERACTIVE VIEW: F FIRES A SHELL; SPACE PAUSES / RESUMES",
                    "SIMULATE" => "FRESH TANK -> SIMULATE ONCE -> CACHE; CLICK AGAIN TO CANCEL",
                    "PLAY" => "SPACE PAUSES / RESUMES THE LIVE SIMULATION",
                    "RENDER" => "CURRENT VIEW -> CACHED IMAGES + MP4; EVERY FRAME, NO REAL-TIME LIMIT",
                    "REC" => "RECORD THE VIEWPORT AS YOU USE IT; CLICK AGAIN TO STOP",
                    "STILL" => "SAVE A CONVERGED STILL OF THE CURRENT VIEW",
                    _ => "REPEAT THE SHOT OR SELECTED CLIP",
                };
                self.text(hint, LEFT_X, BAR_TOP + 0.013, TEXT * 0.66, INK, crate::ui::Align::Left);
            }
            let rec_label = self.recorder.as_ref().map(|r| format!("REC {:.0} S", r.frames as f32 / 30.0));
            let loop_label = match (self.looping, self.clip) {
                (true, Some(_)) => "LOOP: CLIP",
                (true, None) => "LOOP: RUN",
                (false, _) => "LOOP OFF",
            };
            let text = match label {
                "FIRE" if self.cache.busy() => "CACHING",
                "SIMULATE" if self.cache.busy() => "CANCEL",
                "RENDER" if !self.cache.rendering() => "EXPORT VIDEO",
                "RENDER" if self.cache.rendering() => "CANCEL",
                "WATCH" | "PLAY" if self.battle && lit => "STOP",
                "LAYOUTS" => "NEXT LAYOUT",
                "PLAY" if !self.battle && self.playing => "PAUSE",
                "LOOP" => loop_label,
                "REC" if rec_label.is_some() => rec_label.as_deref().unwrap_or("REC"),
                other => other,
            };
            self.text_bold(text, x + w * 0.5, y + h * 0.5, TEXT * 0.68, if lit { BLACK } else { INK }, crate::ui::Align::Center);
        }
        let mut notes: Vec<(String, [f32; 4])> = vec![(self.status.to_uppercase(), INK)];
        if self.battle && self.pilot.is_none() && !self.player_control {
            notes.push(("CLICK: A T-34  DRAG: MOVE IT  CTRL+CLICK: ROUTE POINT  DEL: REMOVE  T: THE TIGER  ENTER: WATCH".into(), DIM));
        }
        if !self.last_chain.is_empty() {
            notes.push((self.last_chain.to_uppercase(), DIM));
        }
        if self.section.on {
            notes.push(("SECTION FOR THE CAMERA ONLY: LIGHT AND PHYSICS SEE THE WHOLE TANK".into(), DIM));
        }
        if scrubbing {
            notes.push(("LET GO OF THE TIMELINE TO TAKE THE SIMULATION TO THIS FRAME".into(), INK));
        }
        // The status: the bar's upper line, its first note (and the next ones while room is left).
        let line: Vec<String> = notes.iter().map(|(t, _)| t.clone()).collect();
        let chars = ((hx - 0.36 - LEFT_X) * self.win[0] / (0.45 * TEXT * 0.66 * self.win[1])).max(10.0) as usize;
        let status: String = line.join("   ").chars().take(chars).collect();
        self.text(&status, LEFT_X, BAR_TOP + 0.013, TEXT * 0.66, INK, crate::ui::Align::Left);
        // The groups' headers: a disclosure each (closed >, open v), lit while open.
        for (g, [x, y, w, h]) in self.header_rects().into_iter().enumerate() {
            let open = self.open[g];
            let hover = cursor[0] >= x && cursor[0] <= x + w && cursor[1] >= y && cursor[1] <= y + h;
            self.r.fill_rect(x, y, w, h, if open { INK } else if hover { [0.09, 0.09, 0.09, 1.0] } else { BOX });
            self.text_bold(&format!("{} {}", if open { "v" } else { ">" }, header_name(GROUPS[g])), x + w * 0.5, y + h * 0.5, TEXT * 0.68,
                if open { BLACK } else { INK }, crate::ui::Align::Center);
        }
        // The open groups' columns: a black ground under each, its name at its head.
        for (_, x, top) in self.columns() {
            self.r.fill_rect(x - 0.003, top - 0.004, COLS_W + 0.003, BAR_TOP - top + 0.004, PANEL_FILL);
        }
        let [x, y, w, h] = hide_rect(false);
        self.r.fill_rect(x, y, w, h, BOX);
        self.text_bold("HIDE", x + w * 0.5, y + h * 0.5, TEXT * 0.7, INK, crate::ui::Align::Center);
        // The chosen group's rows.
        let aspect = self.win[0] / self.win[1].max(1.0);
        for (knob, cx, y) in self.rows() {
            let right = cx + COLS_W - 0.012;
            let minus = right - 2.0 * BTN_W - 0.003;
            let plus = right - BTN_W;
            let value_x = cx + 0.064;
            if cursor[0] >= cx && cursor[0] <= right && (cursor[1] - y).abs() <= ROW_H * 0.5 {
                self.r.fill_rect(cx - 0.003, y - ROW_H * 0.5, right - cx + 0.006, ROW_H, [0.05, 0.05, 0.05, 1.0]);
            }
            let (label, value) = self.knob_label(knob);
            self.text(&label, cx + 0.004, y, TEXT * 0.8, DIM, crate::ui::Align::Left);
            match kind(knob) {
                Kind::Info => self.text(&value, value_x, y, TEXT * 0.72, DIM, crate::ui::Align::Left),
                Kind::Stepper => {
                    self.text(&value, value_x, y, TEXT * 0.8, INK, crate::ui::Align::Left);
                    for (x, sign) in [(minus, "<"), (plus, ">")] {
                        self.r.fill_rect(x, y - ROW_H * 0.4, BTN_W, ROW_H * 0.8, BOX);
                        self.text_bold(sign, x + BTN_W * 0.5, y, TEXT * 0.85, INK, crate::ui::Align::Center);
                    }
                }
                Kind::Toggle => {
                    let on = matches!(value.as_str(), "ON" | "YES" | "LOADED") || value.starts_with("ON ") || value.starts_with("ON:");
                    self.text(&value, value_x, y, TEXT * 0.8, if on { INK } else { DIM }, crate::ui::Align::Left);
                    let w = 2.0 * BTN_W + 0.003;
                    self.r.fill_rect(minus, y - ROW_H * 0.4, w, ROW_H * 0.8, if on { INK } else { BOX });
                    self.text_bold(if on { "ON" } else { "OFF" }, minus + w * 0.5, y, TEXT * 0.75,
                        if on { BLACK } else { DIM }, crate::ui::Align::Center);
                }
                Kind::Dial => {
                    self.text(&value, value_x, y, TEXT * 0.8, INK, crate::ui::Align::Left);
                    let t = self.dial_t(knob);
                    self.dial(right - ROW_H * 0.5 / aspect, y, t);
                }
                Kind::Action => {
                    let w = right - value_x;
                    self.r.fill_rect(value_x, y - ROW_H * 0.4, w, ROW_H * 0.8, BOX);
                    self.text(&value, value_x + w * 0.5, y, TEXT * 0.75, INK, crate::ui::Align::Center);
                }
            }
        }
        self.draw_timeline(cursor);
        if self.help {
            let lines = [
                "RMB HOLD: LOOK    WASD/QE: FLY    SHIFT: FAST    CTRL: SLOW    WHEEL: FLY SPEED    [ ]: FOCAL LENGTH",
                "ALT+LMB: ORBIT    ALT+MMB (OR MMB): PAN    ALT+RMB: DOLLY    DOUBLE-CLICK: THE ORBIT POINT    CLICK X/Y/Z: VIEW ALONG IT",
                "LMB ON THE TANK: AIM (DIRECTION FROM THE CAMERA)    CTRL+LMB: AIM, KEEP DIRECTION",
                "SHOT HANDLES: GREEN ON THE RING SETS THE BEARING, RED ON THE ARC THE ELEVATION (SHIFT+LMB DRAG: BOTH)",
                "C: SECTION ON/OFF    V: AXIS NEAREST THE CAMERA    G: AXIS NEAREST THE SHOT'S PLANE    HANDLE OR ALT+ARROWS: TURN 90",
                "PANEL: ONE TAB OPEN AT A TIME; DRAG A DIAL UP/DOWN (SHIFT FINE), WHEEL STEPS, RMB RESETS",
                "K: ADD CAMERA    1-9: VIEW THROUGH CAMERA    T: MOVE THE TIGER TO THE SNOW UNDER THE CURSOR",
                "F / ENTER: FIRE A SHELL    BACKSPACE: RESET    Z (OR .): CENTRE ON THE TIGER",
                "SPACE: PAUSE / RESUME LIVE SIMULATION    BACKSPACE: FRESH TANK, PAUSED",
                "LEFT/RIGHT: ONE FRAME BACK/ON (PAUSES; SHIFT TEN)    HOME/END: THE RUN'S START/END    DRAG THE TIMELINE",
                "LIVE SIMULATION: NO AUTOMATIC SIMULATION OR IMAGE CACHING",
                "TAB OR HIDE: THE PANEL    R: RECORD THE VIEWPORT    F12: FULL-FRAME STILL 5120X2880    SETTINGS AND CAMERAS KEPT",
                "P: PRINT TISSUE KNOBS    M: MUTE    H: THIS HELP    ESC TWICE: QUIT",
            ];
            self.r.fill_rect(0.01, 0.02, 0.98, 0.03 * lines.len() as f32 + 0.02, [0.0, 0.0, 0.0, 0.85]);
            for (k, l) in lines.iter().enumerate() {
                self.text(l, 0.02, 0.04 + 0.03 * k as f32, TEXT * 0.9, INK, crate::ui::Align::Left);
            }
        }
    }

    /// The timeline (`draw_ui`).
    fn draw_timeline(&mut self, cursor: [f32; 2]) {
        // The timeline: a 5 px white line along the bottom edge from the left to the frame the
        // simulation stands at; a clip in orange; on hover the playhead's tick and a readout.
        let base = self.timeline_y();
        if !self.battle {
            let state = if self.playing { "LIVE" } else { "PAUSED" };
            let recording = if self.recorder.is_some() { "RECORDING VIDEO" } else { "VIDEO NOT RECORDING" };
            self.text(&format!("{state} | F: FIRE | SPACE: PAUSE / RESUME | BACKSPACE: RESET | {recording}"),
                self.view_x() + 0.008, base - 0.034, TEXT * 0.75, AMBER, crate::ui::Align::Left);
        }
        let hovered = (cursor[1] - base).abs() < 0.018 || matches!(self.mode, Mode::Scrub(_));
        if let Some(run) = self.run {
            let n = run.total.max(1) as f32;
            let px = 1.0 / self.win[1].max(1.0);
            let thick = TL_PX * px;
            let at = match self.mode {
                Mode::Scrub(i) => i as f32 + 1.0,
                Mode::Live if self.past_run => n,
                Mode::Live => self.frame_at.map_or(0.0, |f| f as f32 + 1.0),
            };
            let x0 = self.view_x();
            let x = |k: f32| x0 + (1.0 - x0) * k / n;
            self.r.fill_rect(x0, base - thick, x(at) - x0, thick, INK);
            // The clip over the line, its ends ticked.
            if let Some((a, b)) = self.clip {
                self.r.fill_rect(x(a as f32), base - thick, x(b as f32 + 1.0) - x(a as f32), thick, AMBER);
                for edge in [a as f32, b as f32 + 1.0] {
                    self.r.fill_rect(x(edge) - 1.0 / self.win[0], base - 5.0 * thick, 2.0 / self.win[0], 5.0 * thick, AMBER);
                }
            }
            if hovered {
                let head = x(at);
                self.r.fill_rect(head - 1.0 / self.win[0], base - 4.0 * thick, 2.0 / self.win[0], 4.0 * thick, INK);
                let t = match self.mode {
                    Mode::Scrub(i) => self.frame_time(i as u32),
                    _ => self.shot_t,
                };
                let clip = self.clip.map_or(String::new(), |(a, b)| format!("  CLIP {}-{}{}", a + 1, b + 1, if self.looping { " LOOP" } else { "" }));
                let label = format!("{:.0}/{} {t:.3} S{clip}", at, run.total);
                let align = if head > 0.85 { crate::ui::Align::Right } else { crate::ui::Align::Left };
                self.text(&label, head + if head > 0.85 { -0.004 } else { 0.004 }, base - 4.0 * thick - 0.008, TEXT * 0.7, INK, align);
            }
        } else if hovered {
            self.text("SET UP SHOT, THEN FIRE (F / ENTER)", 0.5 * (1.0 + self.view_x()), base - 0.012, TEXT * 0.7, DIM, crate::ui::Align::Center);
        }
    }

    /// A press on the panel: what it did.
    fn click_ui(&mut self, cursor: [f32; 2], fine: bool, coarse: bool) -> Click {
        let [x, y, w, h] = hide_rect(self.hide_ui);
        if cursor[0] >= x && cursor[0] <= x + w && cursor[1] >= y && cursor[1] <= y + h {
            self.hide_ui = !self.hide_ui;
            return Click::Used;
        }
        if !self.in_panel(cursor) {
            return Click::Viewport;
        }
        for (label, col, row) in self.buttons() {
            let [x, y, w, h] = button_rect(col, row);
            if cursor[0] >= x && cursor[0] <= x + w && cursor[1] >= y && cursor[1] <= y + h {
                self.button(label);
                return Click::Used;
            }
        }
        if let Some(m) = self.mode_tab_rects().iter().position(|[x, y, w, h]| cursor[0] >= *x && cursor[0] <= x + w && cursor[1] >= *y && cursor[1] <= y + h) {
            self.choose_mode(m);
            return Click::Used;
        }
        if let Some(g) = self.header_rects().iter().position(|[x, y, w, h]| cursor[0] >= *x && cursor[0] <= x + w && cursor[1] >= *y && cursor[1] <= y + h) {
            // One settings panel at a time; clicking the active tab closes it.
            let opening = !self.open[g];
            self.open = [false; GROUPS.len()];
            self.open[g] = opening;
            return Click::Used;
        }
        if let Some((knob, cx)) = self.row_at(cursor) {
            let right = cx + COLS_W - 0.012;
            let minus = right - 2.0 * BTN_W - 0.003;
            match kind(knob) {
                Kind::Dial => return Click::Dial(knob),
                Kind::Stepper => {
                    let dir = if cursor[0] >= minus && cursor[0] < right - BTN_W { -1.0 } else { 1.0 };
                    self.adjust(knob, dir, fine, coarse);
                }
                Kind::Toggle | Kind::Action => self.adjust(knob, 1.0, fine, coarse),
                Kind::Info => {}
            }
        }
        if let Some(axis) = self.axis_hit(cursor) {
            self.snap_view(axis);
        }
        Click::Used
    }

    /// The wheel on the panel: on a row it steps the row.
    fn wheel_ui(&mut self, cursor: [f32; 2], dy: f32, fine: bool, coarse: bool) {
        let _ = &dy;
        match self.row_at(cursor) {
            Some((knob, _)) if !matches!(kind(knob), Kind::Action | Kind::Info) => self.adjust(knob, dy.signum(), fine, coarse),
            Some(_) => {}
            None => {}
        }
    }

    fn track_hit(&self, cursor: [f32; 2]) -> Option<usize> {
        let n = self.run.map_or(0, |r| r.total as usize);
        let x0 = self.view_x();
        (n > 0 && cursor[0] >= x0 && (cursor[1] - self.timeline_y()).abs() < 0.012)
            .then(|| ((((cursor[0] - x0) / (1.0 - x0)).clamp(0.0, 1.0) * n as f32) as usize).min(n - 1))
    }

    fn buttons(&self) -> Vec<(&'static str, usize, usize)> {
        if self.battle { BATTLE_BUTTONS.to_vec() } else { BUTTONS.to_vec() }
    }

    /// The mode tabs' rectangles (x, y, w, h), as `MODES`.
    fn mode_tab_rects(&self) -> Vec<[f32; 4]> {
        let size = TEXT * 0.72;
        let mut x = LEFT_X;
        MODES.iter().map(|name| {
            let w = name.len() as f32 * size * self.win[1] / self.win[0].max(1.0) + 0.010;
            let rect = [x, BAR_ROW_Y, w, BAR_ROW_H];
            x += w + 0.003;
            rect
        }).collect()
    }

    /// A mode tab clicked: RANGE, BATTLE, or WORLD (not built yet).
    fn choose_mode(&mut self, m: usize) {
        match m {
            0 if self.battle => self.set_battle(false),
            1 if !self.battle => self.set_battle(true),
            2 => self.status = "WORLD EDITOR: NOT BUILT YET (TAB WILL SWITCH BATTLE AND WORLD)".into(),
            _ => {}
        }
    }

    fn button(&mut self, label: &str) {
        if self.battle {
            match label {
                "RESET" => {
                    self.stop_battle();
                    self.playing = false;
                    self.fresh();
                    self.status = "RESET: THE TIGER AND THE T-34S BACK AT THEIR MARKS".into();
                }
                "WATCH" | "PLAY" => {
                    let running = if label == "WATCH" { self.pilot.is_some() } else { self.player_control };
                    if running {
                        self.stop_battle();
                        self.playing = false;
                        self.fresh();
                        self.status = "STOPPED: BACK AT THE MARKS".into();
                    } else {
                        self.start_battle(label == "PLAY");
                    }
                }
                "SAVE" => self.save_layout(),
                "LAYOUTS" => self.load_layout(1),
                "REC" => self.toggle_recording(),
                "STILL" => self.still(),
                _ => {}
            }
            return;
        }
        match label {
            "LIVE" => {
                if !self.free_fire && self.run.is_none() { self.free_fire = true; }
                self.mode = Mode::Live;
                self.free_fire = true;
                self.playing = true;
                self.status = "LIVE VIEW: F FIRES A SHELL NOW; SPACE PAUSES / RESUMES".into();
            }
            "START" | "RESET" => { self.reset(); self.free_fire = true; },
            "FIRE" => {
                self.run = None;
                self.frame_at = None;
                self.past_run = false;
                self.play_clock = 0.0;
                self.mode = Mode::Live;
                self.apply_tuning();
                self.fire();
                self.free_fire = true;
                self.playing = true;
                self.status = "LIVE: SHELL FIRED; SPACE PAUSES / RESUMES; RESET RESTORES THE TANK".into();
            },
            "SIMULATE" | "RUN" => self.cache_simulation(),
            "PLAY" => self.playing = !self.playing,
            "RENDER" => self.render_cached_shot(),
            "LOOP" => self.looping = !self.looping,
            "REC" => self.toggle_recording(),
            "STILL" => self.still(),
            _ => {}
        }
    }

    /// One frame on or back (the simulation pauses): on is the run's next frame (1/480 s inside
    /// the slow-motion span, 1/30 s after it) or 1/30 s of real time past the run; back seeks.
    fn step_frame(&mut self, forward: bool, count: u32) {
        self.playing = false;
        if self.step_cached_frame(forward,count) { return; }
        if forward {
            for _ in 0..count {
                match self.next_run_frame() {
                    Some(k) => self.run_frame(k),
                    None => {
                        for _ in 0..REAL_TICKS_PER_FRAME {
                            self.step(TICK);
                        }
                        self.shot_t += TICK * REAL_TICKS_PER_FRAME as f32;
                        self.past_run = self.run.is_some();
                    }
                }
            }
            self.status = format!("PAUSED AT {:.4} S", self.shot_t);
        } else if let Some(at) = self.frame_at {
            let at = if self.past_run { self.run.map_or(0, |r| r.total) } else { at };
            self.seek(at.saturating_sub(count));
        }
    }

    /// The window's crop of the internal frame shown in the viewport (uv offset, uv size): the
    /// centre of the frame at the viewport's own aspect, so pixels stay square.
    fn crop(&self) -> [f32; 4] {
        let viewport = (1.0 - self.view_x()) * self.win[0] / (self.view_h() * self.win[1].max(1.0));
        let frame = self.r.iw as f32 / self.r.ih.max(1) as f32;
        if viewport <= frame {
            let w = viewport / frame;
            [0.5 - 0.5 * w, 0.0, w, 1.0]
        } else {
            let h = frame / viewport;
            [0.0, 0.5 - 0.5 * h, 1.0, h]
        }
    }

    /// A window point (0..1) as a point of the internal frame (0..1); None in the sidebar.
    fn to_frame(&self, c: [f32; 2]) -> Option<[f32; 2]> {
        let x0 = self.view_x();
        if c[1] > self.view_h() || c[0] < x0 {
            return None;
        }
        let k = self.crop();
        Some([k[0] + (c[0] - x0) / (1.0 - x0) * k[2], k[1] + c[1] / self.view_h() * k[3]])
    }

    /// A point of the internal frame as a window point; None outside the viewport.
    fn to_window(&self, f: [f32; 2]) -> Option<[f32; 2]> {
        let k = self.crop();
        let (x, y) = ((f[0] - k[0]) / k[2], (f[1] - k[1]) / k[3]);
        let x0 = self.view_x();
        ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)).then(|| [x0 + x * (1.0 - x0), y * self.view_h()])
    }

    /// The crop of a full internal-grid frame the viewport shows, and its size.
    fn crop_rgba(&self, rgba: &[u8]) -> (Vec<u8>, u32, u32) {
        let k = self.crop();
        let (iw, ih) = (self.r.iw as f32, self.r.ih as f32);
        let (x0, y0) = ((k[0] * iw).round() as usize, (k[1] * ih).round() as usize);
        let (w, h) = (((k[2] * iw).round() as usize).max(1), ((k[3] * ih).round() as usize).max(1));
        let stride = self.r.iw as usize * 4;
        let mut out = Vec::with_capacity(w * h * 4);
        for row in y0..(y0 + h).min(self.r.ih as usize) {
            out.extend_from_slice(&rgba[row * stride + x0 * 4..row * stride + (x0 + w) * 4]);
        }
        let h = out.len() / (w * 4);
        (out, w as u32, h as u32)
    }

    /// Put `rgba` (`w` x `h`) in the viewport's image, recreating the texture for a new size.
    fn upload_image(&mut self, rgba: &[u8], w: u32, h: u32) {
        if self.frame_size != (w, h) {
            self.frame_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("fea-range-frame"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.frame_image = self.r.import_diagnostic_image(&self.device, &self.frame_tex.create_view(&wgpu::TextureViewDescriptor::default()));
            self.frame_size = (w, h);
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.frame_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.shown_frame = None;
    }

    /// The ground under the window point `cursor` (the first crossing of the camera ray below the
    /// heightfield, metre steps to 2 km).
    fn ground_at(&self, cursor: [f32; 2]) -> Option<Vec3> {
        let (from, dir) = self.ray(cursor);
        (1..8000).map(|k| from + dir * (k as f32 * 0.25)).find(|p| p.y < self.g.hf.height_at(p.x, p.z))
    }

    /// Move the Tiger (a fresh one, with its crew) to the snow under `cursor`.
    fn move_tiger(&mut self, cursor: [f32; 2]) {
        match self.ground_at(cursor) {
            Some(p) => {
                let offset = self.eye - self.g.player.pos;
                self.tiger_at = Vec2::new(p.x, p.z);
                self.fresh();
                self.eye = self.g.player.pos + offset;
                self.status = format!("TIGER MOVED TO X {:.0} Z {:.0} (HEADING {:.2} RAD)", p.x, p.z, self.tiger_heading);
            }
            None => self.status = "NO GROUND UNDER THE CURSOR".into(),
        }
    }

    /// Render a frame into `view`: the live scene into the viewport, or a recorded frame, and the
    /// sidebar over it.
    fn render(&mut self, view: &wgpu::TextureView, format: wgpu::TextureFormat, w: u32, h: u32, cursor: [f32; 2]) {
        self.win = [w as f32, h as f32];
        if self.recorder.is_some() && self.mode == Mode::Live {
            self.record_frame();
        }
        // Drawn while the timeline is dragged too: the timeline keeps no pictures, so skipping the
        // view there left the window black (owner, 2026-11-03).
        if !self.captured {
            self.pose_view();
            self.r.set_active_view(0);
            let rect = crate::renderer::BlitRect { src_uv: self.crop(),
                dst_px: [self.view_x() * w as f32, 0.0, (1.0 - self.view_x()) * w as f32, self.view_h() * h as f32], clear: true };
            self.r.render_to_surface_view(&self.device, &self.queue, view, format, &rect);
        }
        self.draw_ui(cursor);
        self.r.render_hud_over_surface(&self.device, &self.queue, view, format, w, h);
    }
}

// ── The window ─────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    /// Shift+LMB: turning the shot's direction.
    Turn,
    /// Alt+LMB: orbiting the camera about a point; Alt+MMB (or MMB): panning; Alt+RMB: dollying
    /// toward the point (the point under the cursor when the drag began).
    Orbit(Vec3),
    Pan(Vec3),
    Dolly(Vec3),
    /// LMB on the timeline.
    Track,
    /// RMB on the timeline: marking a clip from the frame first pressed.
    Clip(usize),
    /// LMB on a shot handle: 0 the bearing ring, 1 the elevation arc.
    ShotHandle(usize, f32),
    /// LMB on a dial: the dial, its position when taken and the cursor's height then.
    Dial(Knob, f32, f32),
    /// LMB on the section handle's pad: 90° steps per `HANDLE_DRAG_STEP` of motion (the motion
    /// not yet spent).
    Rotate([f32; 2]),
    /// LMB on a placed T-34 in BATTLE edit: moving it over the snow.
    T34(usize),
}

struct WindowState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    range: Range,
}

#[derive(Default)]
struct Input {
    keys: HashSet<KeyCode>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    cursor: [f32; 2],
    look: bool,
    mouse: Vec2,
    drag: Option<(Drag, [f32; 2])>,
}

struct App {
    state: Option<WindowState>,
    input: Input,
    last: Option<std::time::Instant>,
    active: Option<std::time::Instant>,
    escape: Option<std::time::Instant>,
    error: Option<String>,
    /// When the settings were last saved.
    saved: Option<std::time::Instant>,
    /// The last plain click in the viewport (a double click sets the orbit point): when, where,
    /// and the shot before it aimed.
    last_click: Option<(std::time::Instant, [f32; 2], ShotSetup)>,
    /// Playing the Tiger: the mouse buttons held (left, middle, right), the mouse's motion since
    /// the last frame, and whether the cursor is held by the game.
    play_buttons: [bool; 3],
    play_mouse: Vec2,
    grabbed: bool,
}

impl App {
    fn busy(&self) -> bool {
        self.state.as_ref().is_some_and(|s| {
            let r = &s.range;
            r.playing || r.cache.busy()
        }) || self.input.look || !self.input.keys.is_empty() || self.input.drag.is_some()
            || self.active.is_some_and(|t| t.elapsed().as_secs_f32() < IDLE_AFTER_S)
    }

    fn key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        let Some(st) = self.state.as_mut() else { return };
        let rg = &mut st.range;
        // Playing the Tiger: Esc pauses (the editor's camera then orbits from the game's view);
        // 1-5 take a station; the other keys drive the game (`frame`).
        if rg.player_control {
            match code {
                KeyCode::Escape => {
                    rg.play_paused = !rg.play_paused;
                    rg.playing = !rg.play_paused;
                    rg.status = if rg.play_paused { "PAUSED: ORBIT AND LOOK AROUND, ESC PLAYS ON, STOP ENDS".into() } else { "PLAYING".into() };
                    return;
                }
                KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 | KeyCode::Digit4 | KeyCode::Digit5 if !rg.play_paused => {
                    let station = [Station::Commander, Station::Gunner, Station::Loader, Station::Driver, Station::Radioman]
                        [[KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5].iter().position(|k| *k == code).unwrap_or(0)];
                    rg.g.jump_station(station);
                    return;
                }
                KeyCode::F12 => {
                    rg.still();
                    return;
                }
                _ if !rg.play_paused => return,
                _ => {}
            }
        }
        if rg.battle {
            match code {
                KeyCode::Tab => {
                    rg.choose_mode(2);
                    return;
                }
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    rg.button("WATCH");
                    return;
                }
                KeyCode::Backspace => {
                    rg.button("RESET");
                    return;
                }
                KeyCode::Space => {
                    rg.playing = !rg.playing;
                    return;
                }
                KeyCode::Delete | KeyCode::KeyX if rg.pilot.is_none() && !rg.player_control => {
                    let c = self.input.cursor;
                    rg.delete_near(c);
                    return;
                }
                _ => {}
            }
        }
        match code {
            KeyCode::Escape => {
                if self.escape.is_some_and(|t| t.elapsed().as_secs_f32() < 2.0) {
                    rg.save_state();
                    if rg.recorder.is_some() {
                        rg.toggle_recording();
                    }
                    event_loop.exit();
                } else {
                    self.escape = Some(std::time::Instant::now());
                    rg.status = "ESC AGAIN TO QUIT".into();
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter => rg.button("FIRE"),
            KeyCode::Backspace => rg.button("START"),
            KeyCode::Space if self.input.shift => rg.button("PLAY"),
            KeyCode::Space => rg.button("PLAY"),
            KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::ArrowUp | KeyCode::ArrowDown if self.input.alt => {
                let step = match code { KeyCode::ArrowLeft => (-1, 0), KeyCode::ArrowRight => (1, 0), KeyCode::ArrowUp => (0, 1), _ => (0, -1) };
                rg.rotate_section(step.0, step.1);
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                rg.step_frame(code == KeyCode::ArrowRight, if self.input.shift { 10 } else { 1 });
            }
            KeyCode::KeyT => {
                let c = self.input.cursor;
                rg.move_tiger(c);
            }
            KeyCode::Home | KeyCode::End => {
                if let Some(run) = rg.run {
                    rg.playing = false;
                    rg.seek(if code == KeyCode::Home { 0 } else { run.total.saturating_sub(1) });
                }
            }
            KeyCode::KeyG => rg.adjust(Knob::SnapToShot, 1.0, false, false),
            KeyCode::Tab => rg.hide_ui = !rg.hide_ui,
            // F fires into the present state and plays on: it never loops back (owner, 2026-11-03).
            KeyCode::KeyF => {
                rg.button("FIRE");
            }
            KeyCode::Period | KeyCode::NumpadDecimal | KeyCode::KeyZ => rg.frame_tiger(),
            KeyCode::KeyC => rg.section.on = !rg.section.on,
            KeyCode::KeyV => {
                let n = rg.g.player.hull_model().inverse().transform_vector3(-rg.forward());
                rg.section.set_normal_hull(n);
                rg.section.offset = 0.0;
                rg.section.on = true;
            }
            KeyCode::KeyP => {
                rg.print_tuning();
                rg.status = "TISSUE KNOBS PRINTED (TERMINAL) AND WRITTEN TO TARGET/FEA_RANGE_TUNING.JSON".into();
            }
            KeyCode::KeyM => {
                rg.muted = true;
                rg.g.mute_audio();
                rg.status = "AUDIO OFF FOR THIS SESSION".into();
            }
            KeyCode::KeyH => rg.help = !rg.help,
            KeyCode::BracketLeft => rg.focal_mm = (rg.focal_mm / 1.25).max(12.0),
            KeyCode::BracketRight => rg.focal_mm = (rg.focal_mm * 1.25).min(400.0),
            KeyCode::KeyK => rg.adjust(Knob::AddCam, 1.0, false, false),
            KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 | KeyCode::Digit4 | KeyCode::Digit5 | KeyCode::Digit6
            | KeyCode::Digit7 | KeyCode::Digit8 | KeyCode::Digit9 => {
                let n = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6,
                    KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9].iter().position(|k| *k == code).unwrap_or(0);
                rg.view_camera(n);
            }
            KeyCode::F12 => rg.still(),
            KeyCode::KeyR => rg.toggle_recording(),
            _ => {}
        }
    }

    fn frame(&mut self) {
        let now = std::time::Instant::now();
        if self.saved.is_none_or(|t| now.duration_since(t).as_secs() >= 5) {
            if let Some(st) = &self.state {
                st.range.save_state();
            }
            self.saved = Some(now);
        }
        let dt = self.last.map_or(1.0 / 60.0, |t| (now - t).as_secs_f32()).min(0.1);
        self.last = Some(now);
        let input = &mut self.input;
        let Some(st) = self.state.as_mut() else { return };
        let rg = &mut st.range;
        // Playing the Tiger: the keys and mouse are the game's (as `Keys::tank_input`).
        let in_play = rg.player_control && !rg.play_paused;
        if in_play {
            let held = |k: KeyCode| input.keys.contains(&k) as i32 as f32;
            let [lmb, mmb, rmb] = self.play_buttons;
            rg.play_input = TankInput {
                drive: held(KeyCode::KeyW) - held(KeyCode::KeyS),
                steer: held(KeyCode::KeyA) - held(KeyCode::KeyD),
                traverse: held(KeyCode::ArrowLeft) - held(KeyCode::ArrowRight),
                elevate: held(KeyCode::ArrowUp) - held(KeyCode::ArrowDown),
                mg_coax: lmb,
                trigger: lmb,
                fire2: lmb,
                coax: mmb,
                aim: rmb,
                look_dx: self.play_mouse.x,
                look_dy: self.play_mouse.y,
                run: input.shift,
                crouch: input.ctrl,
                jump: input.keys.contains(&KeyCode::Space),
                enter: input.keys.contains(&KeyCode::KeyE),
                ignition: input.keys.contains(&KeyCode::KeyZ),
                repair: input.keys.contains(&KeyCode::KeyG),
                reload: input.keys.contains(&KeyCode::KeyR),
                fire_order: input.keys.contains(&KeyCode::KeyK),
                crew_yes: input.keys.contains(&KeyCode::KeyY),
                crew_no: input.keys.contains(&KeyCode::KeyN),
                ..TankInput::default()
            };
        } else {
            self.play_buttons = [false; 3];
        }
        self.play_mouse = Vec2::ZERO;
        let want_grab = in_play || input.look;
        if want_grab != self.grabbed {
            if want_grab {
                let _ = st.window.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| st.window.set_cursor_grab(CursorGrabMode::Confined));
            } else {
                let _ = st.window.set_cursor_grab(CursorGrabMode::None);
            }
            st.window.set_cursor_visible(!want_grab);
            self.grabbed = want_grab;
        }
        // Fly camera.
        if in_play {
            input.mouse = Vec2::ZERO;
        }
        if input.look && !in_play {
            rg.yaw += input.mouse.x * 0.0025;
            rg.pitch = (rg.pitch - input.mouse.y * 0.0025).clamp(-1.55, 1.55);
        }
        input.mouse = Vec2::ZERO;
        let f = rg.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let mut v = Vec3::ZERO;
        let held = |k: KeyCode| input.keys.contains(&k);
        if !in_play && (!input.ctrl || input.look) {
            if held(KeyCode::KeyW) { v += f; }
            if held(KeyCode::KeyS) { v -= f; }
            if held(KeyCode::KeyD) { v += right; }
            if held(KeyCode::KeyA) { v -= right; }
            if held(KeyCode::KeyE) { v += Vec3::Y; }
            if held(KeyCode::KeyQ) { v -= Vec3::Y; }
        }
        let boost = if input.shift { 4.0 } else if input.ctrl { 0.25 } else { 1.0 };
        rg.eye += v.normalize_or_zero() * rg.speed * boost * dt;
        rg.keep_above_ground();
        let sim_started = std::time::Instant::now();
        rg.tick(dt);
        let sim_ms = sim_started.elapsed().as_secs_f32() * 1000.0;
        let frame = match st.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                st.surface.configure(&rg.device, &st.config);
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let render_started = std::time::Instant::now();
        rg.render(&view, st.config.format, st.config.width, st.config.height, input.cursor);
        // Frame rate and where the time goes (smoothed): the simulation's step, the renderer's frame.
        let render_ms = render_started.elapsed().as_secs_f32() * 1000.0;
        let k = 0.1;
        rg.perf = [rg.perf[0] + (dt.max(1.0e-4) - rg.perf[0]) * k, rg.perf[1] + (sim_ms - rg.perf[1]) * k,
            rg.perf[2] + (render_ms - rg.perf[2]) * k];
        // KESSEL_FEA_RANGE_FRAMELOG=FILE: one JSON line a frame (wall dt, sim and render ms, the
        // game's own phase timings) to find what a strike's frames spend.
        static FRAME_LOG: std::sync::OnceLock<Option<std::sync::Mutex<std::io::BufWriter<std::fs::File>>>> = std::sync::OnceLock::new();
        if let Some(log) = FRAME_LOG.get_or_init(|| std::env::var_os("KESSEL_FEA_RANGE_FRAMELOG")
            .and_then(|p| std::fs::File::create(p).ok()).map(|f| std::sync::Mutex::new(std::io::BufWriter::new(f)))) {
            use std::io::Write;
            let line = serde_json::json!({"dt_ms": dt * 1000.0, "sim_ms": sim_ms, "render_ms": render_ms,
                "shot_t": rg.shot_t, "phases": rg.g.sim_phase.take()});
            if let Ok(mut f) = log.lock() { let _ = writeln!(f, "{line}"); let _ = f.flush(); }
        }
        frame.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("Panzerrechnersystem — FEA range");
        let attrs = if std::env::var("KESSEL_FEA_RANGE_WINDOWED").as_deref() == Ok("1") {
            attrs.with_inner_size(winit::dpi::PhysicalSize::new(1920, 1080))
        } else {
            attrs.with_maximized(true).with_fullscreen(Some(Fullscreen::Borderless(None)))
        };
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let built = (|| -> anyhow::Result<WindowState> {
            let size = window.inner_size();
            let instance = crate::create_instance();
            let surface = instance.create_surface(window.clone())?;
            let adapter = crate::request_adapter(&instance, Some(&surface));
            println!("[fea-range] adapter: {}", adapter.get_info().name);
            let (device, queue) = crate::request_device(&adapter);
            let caps = surface.get_capabilities(&adapter);
            let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format,
                width: size.width.max(1), height: size.height.max(1),
                present_mode: wgpu::PresentMode::AutoVsync, alpha_mode: caps.alpha_modes[0],
                view_formats: vec![], desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            let range = Range::new(device, queue, false)?;
            Ok(WindowState { window: window.clone(), surface, config, range })
        })();
        match built {
            Ok(st) => {
                st.window.request_redraw();
                self.state = Some(st);
            }
            Err(e) => {
                eprintln!("[fea-range] start failed: {e:#}");
                self.error = Some(format!("{e:#}"));
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.state.is_none() {
            return;
        }
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.active = Some(std::time::Instant::now());
        }
        match event {
            WindowEvent::CloseRequested => {
                if let Some(st) = self.state.as_mut() {
                    st.range.save_state();
                    st.range.finish_cached_render();
                    if st.range.recorder.is_some() {
                        st.range.toggle_recording();
                    }
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(st) = self.state.as_mut() {
                    if size.width > 0 && size.height > 0 {
                        st.config.width = size.width;
                        st.config.height = size.height;
                        st.surface.configure(&st.range.device, &st.config);
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.input.shift = m.state().shift_key();
                self.input.ctrl = m.state().control_key();
                self.input.alt = m.state().alt_key();
            }
            WindowEvent::Focused(false) => {
                self.input.keys.clear();
                self.input.look = false;
                self.input.drag = None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let Some(st) = self.state.as_mut() else { return };
                let c = [(position.x as f32 / st.config.width.max(1) as f32).clamp(0.0, 1.0),
                    (position.y as f32 / st.config.height.max(1) as f32).clamp(0.0, 1.0)];
                if let Some((drag, last)) = self.input.drag {
                    let (dx, dy) = (c[0] - last[0], c[1] - last[1]);
                    let rg = &mut st.range;
                    match drag {
                        Drag::Turn => {
                            rg.shot.from_az = ((rg.shot.from_az - dx * 180.0 + 180.0).rem_euclid(360.0)) - 180.0;
                            rg.shot.from_el = (rg.shot.from_el - dy * 90.0).clamp(-60.0, 80.0);
                        }
                        Drag::Orbit(pivot) => rg.orbit(pivot, dx, dy),
                        Drag::Pan(pivot) => rg.pan(pivot, dx, dy),
                        Drag::Dolly(pivot) => rg.dolly(pivot, dx, dy),
                        Drag::Rotate(mut owed) => {
                            owed[0] += dx;
                            owed[1] += dy;
                            let h = (owed[0] / HANDLE_DRAG_STEP).trunc() as i32;
                            let v = (-owed[1] / HANDLE_DRAG_STEP).trunc() as i32;
                            if h != 0 || v != 0 {
                                rg.rotate_section(h, v);
                                owed[0] -= h as f32 * HANDLE_DRAG_STEP;
                                owed[1] += v as f32 * HANDLE_DRAG_STEP;
                            }
                            self.input.drag = Some((Drag::Rotate(owed), c));
                            self.input.cursor = c;
                            return;
                        }
                        Drag::Dial(knob, t0, y0) => {
                            let k = if self.input.shift { 0.1 } else { 1.0 };
                            rg.set_dial_t(knob, t0 + (y0 - c[1]) / DIAL_DRAG * k);
                            if self.input.shift {
                                // Fine turning re-bases, so letting go of Shift does not jump.
                                self.input.drag = Some((Drag::Dial(knob, rg.dial_t(knob), c[1]), c));
                                self.input.cursor = c;
                                return;
                            }
                        }
                        Drag::ShotHandle(which, offset) => rg.drag_shot_handle(which, offset, c),
                        Drag::T34(i) => rg.move_t34(i, c),
                        Drag::Clip(a) => {
                            if let Some(b) = rg.track_hit([c[0], rg.timeline_y()]) {
                                rg.clip = (a != b).then(|| (a.min(b) as u32, a.max(b) as u32));
                            }
                        }
                        Drag::Track => {
                            if let Some(i) = rg.track_hit([c[0], rg.timeline_y()]) {
                                rg.mode = Mode::Scrub(i);
                                rg.playing = false;
                            }
                        }
                    }
                    self.input.drag = Some((drag, c));
                }
                self.input.cursor = c;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if dy == 0.0 {
                    return;
                }
                let (c, fine, coarse) = (self.input.cursor, self.input.ctrl, self.input.shift);
                let Some(st) = self.state.as_mut() else { return };
                let rg = &mut st.range;
                if rg.track_hit(c).is_some() {
                    rg.step_frame(dy < 0.0, if coarse { 10 } else { 1 });
                } else if rg.in_panel(c) {
                    rg.wheel_ui(c, dy, fine, coarse);
                } else {
                    rg.speed = (rg.speed * 1.2f32.powf(dy)).clamp(0.05, 200.0);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                // Playing the Tiger: the buttons are the game's.
                if self.state.as_ref().is_some_and(|st| st.range.player_control && !st.range.play_paused) {
                    match button {
                        MouseButton::Left => self.play_buttons[0] = pressed,
                        MouseButton::Middle => self.play_buttons[1] = pressed,
                        MouseButton::Right => self.play_buttons[2] = pressed,
                        _ => {}
                    }
                    return;
                }
                // BATTLE edit: a click on the snow places a T-34, on one drags it, Ctrl+click adds
                // a route point (Alt keeps the camera's orbit; the panel and timeline as ever).
                if button == MouseButton::Left && pressed && !self.input.alt && !self.input.shift {
                    let (c, ctrl) = (self.input.cursor, self.input.ctrl);
                    if let Some(st) = self.state.as_mut() {
                        let rg = &mut st.range;
                        if rg.battle && rg.pilot.is_none() && !rg.player_control && !rg.in_panel(c) && !rg.on_hide_button(c)
                            && rg.track_hit(c).is_none() && rg.axis_hit(c).is_none() {
                            if ctrl {
                                rg.add_route_point(c);
                            } else if let Some(i) = rg.t34_at(c) {
                                self.input.drag = Some((Drag::T34(i), c));
                            } else {
                                rg.place_t34(c);
                            }
                            return;
                        }
                    }
                }
                match button {
                    MouseButton::Right if pressed && self.state.as_ref().is_some_and(|st| st.range.in_panel(self.input.cursor)
                        && st.range.track_hit(self.input.cursor).is_none()) => {
                        let c = self.input.cursor;
                        if let Some(st) = self.state.as_mut() {
                            if let Some((knob, _)) = st.range.row_at(c).filter(|(k, _)| kind(*k) == Kind::Dial) {
                                st.range.reset_dial(knob);
                            }
                        }
                    }
                    MouseButton::Right if pressed && self.input.alt => {
                        let c = self.input.cursor;
                        if let Some(st) = self.state.as_ref() {
                            self.input.drag = Some((Drag::Dolly(st.range.orbit_point(c)), c));
                        }
                    }
                    MouseButton::Middle if pressed => {
                        let c = self.input.cursor;
                        if let Some(st) = self.state.as_ref() {
                            self.input.drag = Some((Drag::Pan(st.range.orbit_point(c)), c));
                        }
                    }
                    MouseButton::Middle | MouseButton::Right if !pressed
                        && matches!(self.input.drag, Some((Drag::Pan(_) | Drag::Dolly(_), _))) => {
                        self.input.drag = None;
                    }
                    MouseButton::Right if pressed && self.state.as_ref().and_then(|st| st.range.track_hit(self.input.cursor)).is_some() => {
                        let c = self.input.cursor;
                        if let Some(st) = self.state.as_mut() {
                            let i = st.range.track_hit(c).unwrap_or(0);
                            st.range.clip = None;
                            self.input.drag = Some((Drag::Clip(i), c));
                        }
                    }
                    MouseButton::Right if !pressed && matches!(self.input.drag, Some((Drag::Clip(_), _))) => {
                        self.input.drag = None;
                    }
                    MouseButton::Right => {
                        // The cursor is held while looking (`frame` grabs and lets go).
                        self.input.look = pressed;
                    }
                    MouseButton::Left if pressed => {
                        let (c, shift, ctrl, alt) = (self.input.cursor, self.input.shift, self.input.ctrl, self.input.alt);
                        let Some(st) = self.state.as_mut() else { return };
                        let rg = &mut st.range;
                        if let Some(i) = rg.track_hit(c) {
                            rg.mode = Mode::Scrub(i);
                            rg.playing = false;
                            self.input.drag = Some((Drag::Track, c));
                        } else if let Some(which) = rg.shot_handle_hit(c).filter(|_| !rg.in_panel(c) && !rg.on_hide_button(c)) {
                            let angle = rg.shot_cursor_angle(which,c).unwrap_or(
                                if which == 0 { rg.shot.from_az } else { rg.shot.from_el });
                            let value = if which == 0 { rg.shot.from_az } else { rg.shot.from_el };
                            self.input.drag = Some((Drag::ShotHandle(which,value-angle), c));
                        } else if let Some(step) = rg.handle_hit(c).filter(|_| rg.mode == Mode::Live) {
                            if step == (0, 0) {
                                self.input.drag = Some((Drag::Rotate([0.0, 0.0]), c));
                            } else {
                                rg.rotate_section(step.0, step.1);
                            }
                        } else if rg.in_panel(c) || rg.on_hide_button(c) {
                            if let Click::Dial(knob) = rg.click_ui(c, ctrl, shift) {
                                self.input.drag = Some((Drag::Dial(knob, rg.dial_t(knob), c[1]), c));
                            }
                        } else if let Some(axis) = rg.axis_hit(c) {
                            rg.snap_view(axis);
                        } else if alt {
                            self.input.drag = Some((Drag::Orbit(rg.orbit_point(c)), c));
                        } else if shift {
                            self.input.drag = Some((Drag::Turn, c));
                        } else {
                            // A second click within 0.35 s on the same spot: the orbit point
                            // there, and the first click's aim taken back.
                            let double = self.last_click.as_ref().filter(|(t, at, _)| t.elapsed().as_secs_f32() < 0.35
                                && ((at[0] - c[0]).hypot(at[1] - c[1])) < 0.01).map(|(_, _, shot)| *shot);
                            if let Some(shot) = double {
                                rg.shot = shot;
                                rg.set_orbit_point(c);
                                self.last_click = None;
                            } else {
                                self.last_click = Some((std::time::Instant::now(), c, rg.shot));
                                rg.pick(c, ctrl);
                            }
                        }
                    }
                    MouseButton::Left => {
                        // Letting go of the timeline makes the dragged frame live.
                        if let (Some((Drag::Track, _)), Some(st)) = (self.input.drag, self.state.as_mut()) {
                            if let Mode::Scrub(i) = st.range.mode {
                                st.range.seek(i as u32);
                            }
                        }
                        self.input.drag = None;
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                if event.state == ElementState::Pressed {
                    let repeat = !self.input.keys.insert(code);
                    if !repeat || matches!(code, KeyCode::ArrowLeft | KeyCode::ArrowRight) {
                        self.key(code, event_loop);
                    }
                } else {
                    self.input.keys.remove(&code);
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                if self.busy() {
                    event_loop.set_control_flow(ControlFlow::Poll);
                    if let Some(st) = &self.state {
                        st.window.request_redraw();
                    }
                } else {
                    // Nothing changing: ten frames a second (the transport still converges).
                    event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now()
                        + std::time::Duration::from_millis(100)));
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(st) = &self.state {
            if self.busy() || self.last.is_none_or(|t| t.elapsed().as_millis() >= 100) {
                st.window.request_redraw();
            }
        }
    }

    fn device_event(&mut self, _l: &ActiveEventLoop, _id: winit::event::DeviceId, event: winit::event::DeviceEvent) {
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.grabbed && !self.input.look {
                self.play_mouse += Vec2::new(dx as f32, dy as f32);
                self.active = Some(std::time::Instant::now());
            }
            if self.input.look {
                self.input.mouse += Vec2::new(dx as f32, dy as f32);
                self.active = Some(std::time::Instant::now());
            }
        }
    }
}

fn prepare_environment() {
    // The FEA session's review tune (`KESSEL_TISSUE_TUNE`) is honoured here as in `fea-film`.
    super::crew_tissue::allow_review_tune();
    // The shipping renderer and the owner's review look, unless chosen otherwise.
    if std::env::var_os("KESSEL_RENDERER").is_none() {
        std::env::set_var("KESSEL_RENDERER", "raymarch");
    }
    if std::env::var_os("KESSEL_LOOK").is_none() {
        std::env::set_var("KESSEL_LOOK", "deltap-real");
    }
    graphics::set_active_preset(graphics::GraphicsPreset::High);
}

/// `fea-range` (window) or `fea-range --shots PREFIX` (headless stills and the agreement check).
pub fn run(shots: Option<&str>) -> anyhow::Result<()> {
    // KESSEL_FEA_RANGE_SMOKE=0: the range draws no smoke supports (owner, 2026-11-03).
    if std::env::var("KESSEL_FEA_RANGE_SMOKE").as_deref() == Ok("0") {
        crate::renderer::SMOKE_OFF.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    prepare_environment();
    anyhow::ensure!(graphics::raymarch_backend_enabled(), "fea-range requires KESSEL_RENDERER=raymarch");
    if let Some(prefix) = shots {
        return headless_shots(prefix);
    }
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { state: None, input: Input::default(), last: None, active: None, escape: None, error: None, saved: None, last_click: None,
        play_buttons: [false; 3], play_mouse: Vec2::ZERO, grabbed: false };
    event_loop.run_app(&mut app)?;
    if let Some(e) = app.error {
        anyhow::bail!(e);
    }
    Ok(())
}

/// Headless verification: the interface live, with the section, a short run and a scrubbed
/// frame, and a live re-simulation at that frame compared with the recording's state there.
fn headless_shots(prefix: &str) -> anyhow::Result<()> {
    let prefix = super::game_fea_section::shared_capture_prefix(prefix);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let mut rg = Range::new(device, queue, true)?;
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("live") {
        rg.load_state();
        rg.time_speed = 1.0;
        let hull = rg.g.player.hull_model();
        rg.look_at(hull.transform_point3(Vec3::new(5.5,3.2,1.5)),
            hull.transform_point3(Vec3::new(0.0,1.2,0.1)));
        rg.section.on = true;
        rg.section.set_normal_hull(Vec3::X);
        rg.section.offset = 0.0;
        rg.shot.shell = 7;
        rg.shot.range_m = 100.0;
        rg.shot.from_az = -90.0;
        rg.shot.from_el = 0.0;
        rg.shot.target = Target::Point(Vec3::new(-1.18158, 2.52072, 0.05853));
        for _ in 0..16 { rg.pose_view(); let _ = rg.r.render_to_rgba(&rg.device, &rg.queue, rg.r.iw, rg.r.ih)?; }
        rg.button("FIRE");
        anyhow::ensure!(rg.playing && rg.free_fire && rg.run.is_none(), "FIRE did not start live simulation");
        let started = std::time::Instant::now();
        let mut costs = Vec::new();
        for _ in 0..120 {
            let frame = std::time::Instant::now();
            rg.tick(1.0/60.0);
            rg.pose_view();
            let _ = rg.r.render_to_rgba(&rg.device, &rg.queue, rg.r.iw, rg.r.ih)?;
            costs.push(frame.elapsed().as_secs_f64()*1000.0);
            anyhow::ensure!(!rg.cache.busy() && rg.cache.len() == 0, "live firing started a cache");
        }
        anyhow::ensure!(rg.shot_t > 1.9, "live simulation stalled: {}", rg.shot_t);
        let blood = rg.g.debug_surface_blood();
        println!("[live-probe] blood {blood}");
        anyhow::ensure!(blood["emitted"].as_u64().unwrap_or(0)>0 && blood["retained_cells"].as_u64().unwrap_or(0)>0,
            "penetrating side hit produced no visible deposited blood");
        let path = crate::capture_target(&format!("visual_blood_live_{}.png",capture_stamp()));
        rg.r.render_to_png(&rg.device,&rg.queue,&path,rg.r.iw,rg.r.ih)?;
        println!("[live-probe] capture {path}");
        rg.button("PLAY");
        let at = rg.shot_t;
        rg.tick(1.0/60.0);
        anyhow::ensure!(rg.shot_t == at, "PAUSE advanced simulation");
        rg.button("PLAY");
        rg.tick(1.0/30.0);
        anyhow::ensure!(rg.shot_t > at, "RESUME did not advance simulation");
        rg.button("RESET");
        anyhow::ensure!(!rg.playing && rg.run.is_none(), "RESET did not leave a fresh paused scene");
        costs.sort_by(f64::total_cmp);
        println!("[live-probe] FIRE advances 2 s; no caches; PAUSE/RESUME/RESET pass; wall {:.2} s, frame ms median {:.1}, p95 {:.1}, worst {:.1}",
            started.elapsed().as_secs_f64(), costs[60], costs[114], costs[119]);
        return Ok(());
    }
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("cache") {
        rg.load_state();
        return rg.cache_probe();
    }
    // KESSEL_FEA_RANGE_PROBE=media: a still and a short recording (fired shot), then stop.
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("media") {
        rg.still();
        rg.toggle_recording();
        rg.fire_now();
        for _ in 0..24 {
            rg.tick(1.0 / 60.0);
            rg.record_frame();
        }
        rg.toggle_recording();
        println!("[fea-range] media probe: {}", rg.status);
        return Ok(());
    }
    let (w, h) = (1920, 1080);
    let mut checks = Vec::new();
    let still = |rg: &mut Range, name: &str, converge: bool| -> anyhow::Result<String> {
        rg.live_image = rg.mode == Mode::Live;
        if rg.mode == Mode::Live {
            rg.pose_view();
            if converge {
                rg.r.reset_history();
                crate::converge_headless_screenshot(&mut rg.r, &rg.device, &rg.queue, 16)?;
            }
            // The live frame through the viewport's crop, as the window shows it.
            let rgba = rg.r.render_to_rgba(&rg.device, &rg.queue, rg.r.iw, rg.r.ih)?;
            let (rgba, w, h) = rg.crop_rgba(&rgba);
            rg.upload_image(&rgba, w, h);
        }
        rg.draw_ui([0.1, 0.1]);
        let path = format!("{prefix}_{name}.png");
        rg.r.render_to_png_hud(&rg.device, &rg.queue, &path, w, h)?;
        Ok(path)
    };
    // KESSEL_FEA_RANGE_PROBE=catalog: the saved cameras (target/fea_range_cameras.json) put in the
    // hull frame of the Tiger at the place they were saved (the editor's state file) and written to
    // the catalog.
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("catalog") {
        rg.cameras = std::fs::read_to_string(CAMERAS_PATH).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if let Some(v) = std::fs::read_to_string(STATE_PATH).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
            if let (Some(a), Some(h)) = (v["tiger"]["at"].as_array(), v["tiger"]["heading"].as_f64()) {
                rg.tiger_at = Vec2::new(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32);
                rg.tiger_heading = h as f32;
                rg.fresh();
            }
        }
        let inv = rg.g.player.hull_model().inverse();
        for c in &mut rg.cameras {
            let mut cam = crate::camera::Camera::new(1.0);
            cam.yaw = c.yaw;
            cam.pitch = c.pitch;
            let (e, f) = (inv.transform_point3(Vec3::from_array(c.eye)), inv.transform_vector3(cam.forward()).normalize_or_zero());
            c.hull_eye = Some(e.to_array());
            c.hull_forward = Some(f.to_array());
            if c.name.is_empty() {
                c.name = camera_name(e, f);
            }
            println!("[fea-range] camera {:?} {:?} {}", e, f, c.name);
        }
        rg.adjust(Knob::ExportCams, 1.0, false, false);
        println!("[fea-range] {}", rg.status);
        return Ok(());
    }
    // KESSEL_FEA_RANGE_PROBE=determinism: the short run three times from a fresh Tiger, every
    // field's checksum (`rewind::Checkpoint::prints`) at every frame, and where the runs part; then
    // backward seeks put back from the checkpoints, each against the first runs at that frame.
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("determinism") {
        let prints = |rg: &Range| rewind::Checkpoint::capture(&rg.g, &rg.r, None).map(|(c, _)| c.prints()).unwrap_or_default();
        rg.scene.slow_s = 0.1;
        rg.scene.run_s = 0.5;
        let mut runs = Vec::new();
        for pass in 0..3 {
            let started = std::time::Instant::now();
            rg.run();
            let fresh_s = started.elapsed().as_secs_f32();
            let total = rg.run.map_or(0, |r| r.total);
            let mut frames = vec![prints(&rg)];
            let (mut step_s, captures_before, capture_before) = (0.0, rg.rewind.captures, rg.rewind.capture_s);
            for k in 0..total {
                let started = std::time::Instant::now();
                rg.run_frame(k);
                step_s += started.elapsed().as_secs_f32();
                frames.push(prints(&rg));
            }
            let captures = (rg.rewind.captures - captures_before).max(1);
            println!("[fea-range] determinism pass {pass}: fresh in {fresh_s:.2} s; {total} frames in {step_s:.2} s ({:.0} ms a frame with its checkpoint, the checkpoint {:.1} ms); {} fields; {} checkpoints hold {:.1} MB",
                step_s * 1000.0 / total as f32, (rg.rewind.capture_s - capture_before) * 1000.0 / captures as f32, frames[0].len(),
                rg.rewind.kept(), rg.rewind.bytes as f64 / 1.0e6);
            println!("[fea-range]   {}", rg.digest());
            runs.push(frames);
        }
        let parted = rewind::parted;
        for (a, b) in [(0, 1), (1, 2)] {
            if let Some((frame, (x, y))) = runs[a].iter().zip(&runs[b]).enumerate().find(|(_, (x, y))| x != y) {
                let all: Vec<&String> = x.keys().filter(|k| x.get(*k) != y.get(*k)).collect();
                println!("[fea-range] runs {a} and {b} first part at frame {frame} (0 = just fired) in {all:?}");
            }
            match runs[a].iter().zip(&runs[b]).enumerate().find(|(_, (x, y))| !parted(x, y).is_empty()) {
                None => println!("[fea-range] PASS runs {a} and {b} agree in every field at every frame but {:?}", rewind::LISTENER_FIELDS),
                Some((frame, (x, y))) => println!("[fea-range] FAIL runs {a} and {b} part at frame {frame} (0 = just fired) in {:?}", parted(x, y)),
            }
        }
        rg.rewind.collect_blood(true);
        for (k, b) in rg.rewind.bytes_by_field().into_iter().take(14) {
            println!("[fea-range]   checkpoint memory: {k} {:.1} MB", b as f64 / 1.0e6);
        }
        let total = rg.run.map_or(0, |r| r.total);
        for index in [total - 1, 36, 47, 48, 5, 0, 59.min(total - 1), 12] {
            rg.seek(total - 1);
            let started = std::time::Instant::now();
            rg.seek(index);
            let seek_s = started.elapsed().as_secs_f32();
            let differ = parted(&prints(&rg), &runs[2][index as usize + 1]);
            println!("[fea-range] {} back to frame {index} in {seek_s:.2} s against the run it was kept from{}: {}", if differ.is_empty() { "PASS" } else { "FAIL" },
                if differ.is_empty() { String::new() } else { format!(" (differs in {differ:?})") }, rg.status);
        }
        return Ok(());
    }
    // KESSEL_FEA_RANGE_PROBE=reset: a full cook-off (every round in the racks struck) played on
    // until the turret is thrown, then RESET, and the state against the kept fresh Tiger's field
    // by field, with the Tiger's place, its turret, the cook-off, the fire and the crew named.
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("reset") {
        rg.scene.racks_loaded = true;
        rg.reset();
        let fresh = rg.rewind.fresh.as_ref().map(|(_, c)| c.prints()).unwrap_or_default();
        let summary = |g: &Game| format!("tiger at {:.3?} heading {:.4}, turret {}, cook-off {:.2} s ({} rounds gone), fire {:.2}, cabin fire {}, crew alive {}",
            g.player.pos, g.player.heading, if g.player.turret_thrown.is_some() { "THROWN" } else { "on" },
            g.cookoff.clock_s, g.cookoff.ignitions + g.cookoff.detonations, g.fire, g.cabin_fire.export_world_state().map_or(0, |f| f.fingerprints().len()),
            g.crew.iter().filter(|c| c.ok).count());
        let before = summary(&rg.g);
        let ids: Vec<u32> = rg.g.live_ammunition_in_frame(BloodFrame::Hull).iter().map(|r| r.stable_id).collect();
        rg.g.cookoff_strike(&ids);
        let mut steps = 0;
        while rg.g.player.turret_thrown.is_none() && steps < 60 * 20 {
            rg.step(TICK);
            rg.g.settle_casualty_tissue(true);
            steps += 1;
        }
        for _ in 0..60 {
            rg.step(TICK);
        }
        rg.g.settle_casualty_tissue(true);
        let during = summary(&rg.g);
        rg.reset();
        let after = summary(&rg.g);
        let now = rewind::Checkpoint::capture(&rg.g, &rg.r, None).map(|(c, _)| c.prints()).unwrap_or_default();
        let differ = rewind::parted(&fresh, &now);
        println!("[fea-range] reset probe: {} rounds struck; {:.1} s played", ids.len(), steps as f32 * TICK + 1.0);
        println!("[fea-range]   fresh:      {before}");
        println!("[fea-range]   cooked off: {during}");
        println!("[fea-range]   reset:      {after}");
        println!("[fea-range] {} reset gives the fresh Tiger in all {} fields{}", if differ.is_empty() && !fresh.is_empty() { "PASS" } else { "FAIL" },
            now.len(), if differ.is_empty() { String::new() } else { format!(" but {differ:?}") });
        return Ok(());
    }
    // KESSEL_FEA_RANGE_PROBE=rewind-memory: the editor's default run (its scene as set) played to
    // its end with its checkpoints; their memory and timings.
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("rewind-memory") {
        rg.run();
        let total = rg.run.map_or(0, |r| r.total);
        let started = std::time::Instant::now();
        let mut marks = Vec::new();
        for k in 0..total {
            rg.run_frame(k);
            if (k + 1) % 30 == 0 || k + 1 == total {
                marks.push(format!("{:.2} s: {:.0} MB", rg.shot_t, rg.rewind.bytes as f64 / 1.0e6));
            }
        }
        let played_s = started.elapsed().as_secs_f32();
        rg.rewind.collect_blood(true);
        println!("[fea-range] rewind memory: {} frames ({:.2} s slow, {:.1} s run) played in {played_s:.1} s, blood encoded by {:.1} s; {} kept, {:.0} MB ({:.2} MB a frame); a checkpoint {:.1} ms",
            total, rg.scene.slow_s, rg.scene.run_s, started.elapsed().as_secs_f32(), rg.rewind.kept(), rg.rewind.bytes as f64 / 1.0e6,
            rg.rewind.bytes as f64 / 1.0e6 / rg.rewind.kept().max(1) as f64, rg.rewind.capture_s * 1000.0 / rg.rewind.captures.max(1) as f32);
        println!("[fea-range] rewind memory over the run: {}", marks.join(", "));
        for (k, b) in rg.rewind.bytes_by_field().into_iter().take(8) {
            println!("[fea-range]   {k} {:.1} MB", b as f64 / 1.0e6);
        }
        for index in [total / 2, 3, total - 2] {
            rg.seek(total - 1);
            rg.seek(index);
            println!("[fea-range] seek to {index}: {}", rg.status);
        }
        return Ok(());
    }
    // KESSEL_FEA_RANGE_PROBE=flight: a round in flight seen from beside its path, close up.
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("flight") {
        rg.shot.shell = 9; // 122 mm BR-471
        rg.fire_now();
        rg.playing = false;
        for _ in 0..3 {
            rg.step_frame(true, 1);
        }
        let Some((pos, velocity)) = rg.g.projectiles.iter().find(|p| p.small_arm.is_none()).map(|p| (p.pos, p.velocity)) else {
            println!("[fea-range] flight probe: no round in flight");
            return Ok(());
        };
        let dir = velocity.normalize_or_zero();
        let side = dir.cross(Vec3::Y).normalize_or_zero();
        rg.look_at(pos + side * 1.5 - dir * 0.3 + Vec3::Y * 0.2, pos - dir * 0.4);
        rg.focal_mm = 50.0;
        rg.hide_ui = true;
        still(&mut rg, "flight", true)?;
        println!("[fea-range] flight probe: round at {pos:?}, {:.0} m/s", velocity.length());
        return Ok(());
    }
    // KESSEL_FEA_RANGE_PROBE=ui: the interface on two tabs, then stop.
    if std::env::var("KESSEL_FEA_RANGE_PROBE").as_deref() == Ok("ui") {
        still(&mut rg, "ui_shot", true)?;
        rg.open = [false, false, false, false, false, true, true, true, false];
        rg.section.on = true;
        still(&mut rg, "ui_look", false)?;
        return Ok(());
    }
    // 1. Live, the default shot (57 mm at the loader through the right turret side), help shown.
    rg.help = true;
    rg.open = [true, true, false, true, false, false, false, false, false];
    let p1 = still(&mut rg, "live", true)?;
    rg.help = false;
    rg.open = [false, true, true, true, false, false, false, false, false];
    // 2. The section: the top cut 1.9 m above the ground, the eye above the left side, the cabin
    // lamps at 32 times their power (the review dial), the eye's own exposure.
    rg.section = Section { on: true, yaw: 0.0, pitch: 90.0, offset: 0.5 };
    let hull = rg.g.player.hull_model();
    rg.look_at(hull.transform_point3(Vec3::new(3.0, 6.2, 0.4)), hull.transform_point3(Vec3::new(-0.25, 1.0, -0.2)));
    rg.focal_mm = 30.0;
    rg.dial_set(Knob::LampPower, 32.0);
    let p2 = still(&mut rg, "section", true)?;
    // 3. A short run: 0.1 s in 1/480 s steps, then real time to 0.5 s; the state's digest at a
    // middle frame.
    rg.scene.slow_s = 0.1;
    rg.scene.run_s = 0.5;
    rg.run();
    let probe = 36u32;
    let total = rg.run.map_or(0, |r| r.total);
    let mut recorded_digest = None;
    let started = std::time::Instant::now();
    while rg.frame_at != Some(total - 1) {
        rg.tick(1.0 / 60.0);
        if rg.frame_at == Some(probe) && recorded_digest.is_none() {
            recorded_digest = Some(rg.digest());
        }
    }
    rg.playing = false;
    let run_s = started.elapsed().as_secs_f32();
    println!("[fea-range] run: {total} frames in {run_s:.1} s; {}", rg.last_chain);
    checks.push(serde_json::json!({"check": "run played live to its end", "pass": rg.frame_at == Some(total - 1),
        "frames": total, "wall_s": run_s}));
    rg.mode = Mode::Scrub(probe as usize);
    let p3 = still(&mut rg, "timeline", false)?;
    // 4. Back to that frame (let go of the timeline) with the checkpoints off: the run repeated
    // step for step from the fresh Tiger (the tape), its state against the first run's there, by
    // the digest and every field's checksum (`rewind::Checkpoint::prints`, the listener's side
    // named and left out).
    let recorded_prints = rg.rewind.prints_at(probe).unwrap_or_default();
    let prints = |rg: &Range| rewind::Checkpoint::capture(&rg.g, &rg.r, None).map(|(c, _)| c.prints()).unwrap_or_default();
    rg.rewind.disabled = true;
    let started = std::time::Instant::now();
    rg.seek(probe);
    let rerun_s = started.elapsed().as_secs_f32();
    let live_digest = rg.digest();
    let live_prints = prints(&rg);
    let differ = rewind::parted(&recorded_prints, &live_prints);
    let agree = recorded_digest.as_deref() == Some(live_digest.as_str()) && differ.is_empty() && !recorded_prints.is_empty();
    println!("[fea-range] {} the re-run agrees with the first run at frame {probe} ({} fields; re-run in {rerun_s:.1} s){}",
        if agree { "PASS" } else { "FAIL" }, live_prints.len(), if differ.is_empty() { String::new() } else { format!(": {differ:?}") });
    checks.push(serde_json::json!({"check": "a backward seek agrees with the first run", "pass": agree, "frame": probe,
        "recorded": recorded_digest, "live": live_digest, "fields": live_prints.len(), "differ": differ,
        "not_compared": rewind::LISTENER_FIELDS, "wall_s": rerun_s}));
    // On to the end and back again: whether the re-run repeats itself.
    rg.seek(total - 1);
    rg.seek(probe);
    let again = rg.digest();
    let differ = rewind::parted(&live_prints, &prints(&rg));
    let repeat = again == live_digest && differ.is_empty();
    println!("[fea-range] {} a second re-run agrees with the first re-run{}", if repeat { "PASS" } else { "FAIL" },
        if differ.is_empty() { String::new() } else { format!(": {differ:?}") });
    checks.push(serde_json::json!({"check": "a backward seek repeats itself", "pass": repeat, "again": again, "differ": differ}));
    // 4b. The same seeks from the checkpoints (the editor's way): put back, not re-run. The first
    // frame, the strike's own (its update slow: put back and drawn from a 1 us step) and the
    // probe frame last (the still below), each against its checkpoint from the first run.
    rg.rewind.disabled = false;
    let slowest = (0..total).filter(|k| rg.rewind.at(*k).is_some())
        .max_by(|a, b| rg.rewind.step_s(*a).total_cmp(&rg.rewind.step_s(*b))).unwrap_or(0);
    for (name, index) in [("first", 0), ("slowest", slowest), ("probe", probe)] {
        rg.seek(total - 1);
        let started = std::time::Instant::now();
        rg.seek(index);
        let seek_s = started.elapsed().as_secs_f32();
        let differ = rewind::parted(&rg.rewind.prints_at(index).unwrap_or_default(), &prints(&rg));
        let pass = differ.is_empty() && rg.rewind.at(index).is_some();
        println!("[fea-range] {} back to frame {index} ({name}) from the checkpoints in {seek_s:.2} s: {}{}", if pass { "PASS" } else { "FAIL" },
            rg.status, if differ.is_empty() { String::new() } else { format!(": {differ:?}") });
        checks.push(serde_json::json!({"check": format!("a backward seek from the checkpoints ({name})"), "pass": pass, "frame": index,
            "wall_s": seek_s, "frame_update_s": rg.rewind.step_s(index), "status": rg.status, "differ": differ}));
    }
    rg.rewind.collect_blood(true);
    let memory: Vec<serde_json::Value> = rg.rewind.bytes_by_field().into_iter().take(12)
        .map(|(k, b)| serde_json::json!({"field": k, "mb": b as f64 / 1.0e6})).collect();
    println!("[fea-range] checkpoints: {} of {total} frames hold {:.1} MB ({:.2} MB a frame); a checkpoint took {:.1} ms; most: {}",
        rg.rewind.kept(), rg.rewind.bytes as f64 / 1.0e6, rg.rewind.bytes as f64 / 1.0e6 / rg.rewind.kept().max(1) as f64,
        rg.rewind.capture_s * 1000.0 / rg.rewind.captures.max(1) as f32,
        memory.iter().take(6).map(|m| format!("{} {:.1} MB", m["field"].as_str().unwrap_or(""), m["mb"].as_f64().unwrap_or(0.0))).collect::<Vec<_>>().join(", "));
    checks.push(serde_json::json!({"check": "checkpoint memory (information)", "pass": true, "kept": rg.rewind.kept(), "frames": total,
        "mb": rg.rewind.bytes as f64 / 1.0e6, "capture_ms": rg.rewind.capture_s * 1000.0 / rg.rewind.captures.max(1) as f32, "by_field": memory}));
    rg.snap_section_to_shot();
    let p4 = still(&mut rg, "live_here", true)?;
    // 5. The panel's tissue knobs changed and printed (review override set, then restored).
    rg.adjust(Knob::Tear(0), 1.0, false, false);
    let printed = rg.print_tuning();
    let set = crew_tissue::tissue_tuning() != crew_tissue::TissueTuning::AUTHORED;
    rg.adjust(Knob::TissueAuthored, 1.0, false, false);
    let restored = crew_tissue::tissue_tuning() == crew_tissue::TissueTuning::AUTHORED;
    checks.push(serde_json::json!({"check": "tissue override set and restored", "pass": set && restored, "printed": printed}));
    let receipt = serde_json::json!({"stills": [p1, p2, p3, p4], "checks": checks});
    std::fs::write(format!("{prefix}_receipt.json"), serde_json::to_string_pretty(&receipt)?)?;
    println!("[fea-range] receipt {prefix}_receipt.json");
    Ok(())
}

#[cfg(test)]
mod tests {
    /// The cabin lamps' review power factor is 1 unless the range's LAMP POWER dial sets it: no
    /// other source calls the setter, and the game multiplies the lamp switch by it.
    #[test]
    fn the_lamp_power_is_one_unless_the_range_sets_it() {
        assert_eq!(super::review_lamp_gain(), 1.0);
        let needle = ["set_review_lamp", "_gain("].concat();
        let mut setters = Vec::new();
        for entry in std::fs::read_dir("src").unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") && std::fs::read_to_string(&path).unwrap().contains(&needle) {
                setters.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        assert_eq!(setters, vec!["game_fea_range.rs".to_string()]);
        assert_eq!(super::review_smoke_density(), 1.0);
        assert_eq!(super::review_smoke_delay(), super::super::weapon_vfx::IMPACT_SMOKE_DELAY_S);
        let needle = ["set_review", "_smoke("].concat();
        let mut smoke_setters = Vec::new();
        for entry in std::fs::read_dir("src").unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") && std::fs::read_to_string(&path).unwrap().contains(&needle) {
                smoke_setters.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        assert_eq!(smoke_setters, vec!["game_fea_range.rs".to_string()]);
        let game = std::fs::read_to_string("src/game.rs").unwrap();
        assert!(game.contains("let practical_gain = power * game_fea_range::review_lamp_gain();"));
        // The tissue's section (inside cells drawn at a camera cut) is set by the range alone.
        let needle = ["set_review", "_section(Some"].concat();
        let mut section_setters = Vec::new();
        for entry in std::fs::read_dir("src").unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") && std::fs::read_to_string(&path).unwrap().contains(&needle) {
                section_setters.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        assert_eq!(section_setters, vec!["game_fea_range.rs".to_string()]);
    }

    /// Sections are orthogonal: any yaw and pitch (a dial, a saved oblique plane) draw on the
    /// hull axis nearest them, either side.
    #[test]
    fn the_section_is_always_on_a_hull_axis() {
        for yaw in (-180..=180).step_by(15) {
            for pitch in (-90..=90).step_by(15) {
                let mut s = super::Section { on: true, yaw: yaw as f32, pitch: pitch as f32, offset: 0.0 };
                let n = s.normal_hull();
                assert_eq!(n.abs().max_element(), 1.0);
                assert_eq!(n.abs().element_sum(), 1.0);
                s.snap();
                assert_eq!(s.normal_hull(), n);
                assert!(s.yaw % 90.0 == 0.0 && s.pitch % 90.0 == 0.0, "{} {}", s.yaw, s.pitch);
            }
        }
    }

    /// Each EFFECTS toggle switched off reaches the frame: `apply_eye_fx` runs last in both of
    /// `pose_view`'s paths, after the game's lights raised the flash flag.
    #[test]
    fn the_eye_effect_toggles_are_applied_last_in_every_view() {
        let source = std::fs::read_to_string("src/game_fea_range.rs").unwrap();
        let body = source.split("    fn pose_view(&mut self) {").nth(1).unwrap().split("\n    /// The EFFECTS toggles").next().unwrap();
        assert_eq!(body.matches("self.apply_eye_fx();").count(), 2);
        let play = body.split("return;").next().unwrap();
        assert!(play.find("drive_lights").unwrap() < play.find("self.apply_eye_fx();").unwrap());
        assert!(body.trim_end().trim_end_matches('}').trim_end().ends_with("self.apply_eye_fx();"));
        let apply = source.split("    fn apply_eye_fx(&mut self) {").nth(1).unwrap().split("\n    }\n").next().unwrap();
        for line in ["self.r.concussion = 0.0;", "self.r.observer_flash_source = false;"] {
            assert!(apply.contains(line), "{line}");
        }
        let crush = std::fs::read_to_string("shaders/crush.wgsl").unwrap();
        assert!(crush.contains("let concussion = clamp(cr.concussion, 0.0, 1.0);"));
    }
}
