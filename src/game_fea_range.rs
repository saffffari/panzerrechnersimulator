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
//!   from the present, or back by repeating the run step for step on a fresh Tiger. Measured
//!   2026-11-03 (`--shots`): the game does not repeat a strike bit for bit (the chain and scars
//!   agreed; the crew's tissue and the surface blood did not, between runs and between re-runs),
//!   so going back is the same shot re-run, not a copy, and the status line says so.
//! - **Section**: the film section (`Renderer::set_film_section`), a camera-only plane through the
//!   Tiger; shadows, light, ray queries and every simulation see the whole closed tank.
//! - **Sky**: the renderer's sky clock held at the chosen date and local time (Stalingrad), the
//!   weather override, the eye's exposure automatic or held.
//! - **Tissue**: review-only overrides of the crew lattice's authored constants
//!   (`crew_tissue::TissueTuning`), never a shipping default; P prints them for hand-over.
//!
//! `fea-range --shots PREFIX` renders the editor headless (stills of the interface, a short run,
//! a scrubbed frame and a live re-simulation with its agreement check). Requires the raymarch
//! renderer (set when KESSEL_RENDERER is unset).
use super::*;
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
/// Recorded frames are kept in memory up to this many bytes; a run stops recording past it.
const RECORD_BUDGET_BYTES: usize = 1_500_000_000;
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

fn set_review_lamp_gain(gain: f32) {
    REVIEW_LAMP_GAIN_BITS.store(gain.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

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
    fn normal_hull(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos())
    }
    fn set_normal_hull(&mut self, n: Vec3) {
        let n = n.normalize_or_zero();
        self.yaw = n.x.atan2(n.z).to_degrees();
        self.pitch = n.y.clamp(-1.0, 1.0).asin().to_degrees();
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

/// A recorded frame: its sim time after the shot and the frame index of the run's schedule.
struct Frame {
    t: f32,
    /// The viewport's crop of the frame, `w` x `h`.
    rgba: Vec<u8>,
    w: u32,
    h: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// The present state, the camera free (always, but while the timeline is dragged).
    Live,
    /// The timeline dragged: showing frame `i`'s stored preview until it is let go.
    Scrub(usize),
}

/// The run the timeline is of: its setup (a backward seek repeats it) and its length in frames.
#[derive(Clone, Copy, Debug)]
struct RunInfo {
    shot: ShotSetup,
    scene: SceneSetup,
    tuning: crew_tissue::TissueTuning,
    fresh: bool,
    total: u32,
}

/// Every adjustable row of the sidebar.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Knob {
    Shell, Range, Velocity, FromAz, FromEl, TargetKind, Seed,
    Racks, Lamps, LampPower, Fresh, SlowSpan, RunLength, TimeSpeed, Loop,
    Focal, SavedCam, AddCam, DeleteCam,
    SectionOn, SectionPreset, SnapToShot, SectionYaw, SectionPitch, SectionOffset,
    Day, Time, SkyRunning, SkyRate, Weather, AutoExposure, Exposure,
    Look, LookEv, Saturation, Warmth, Gamma, DitherPattern, PaletteLevels, OrderedDither, LdrDither, Retro,
    WhiteBalance, WhiteBalanceStrength, Fog, LookDefaults,
    Tear(usize), Stretch(usize), Forward, Outward, Reach, Landing, TissueAuthored,
}

/// How a row is worked: a stepper (< >) for discrete choices, a toggle, a knob for continuous
/// values, or an action.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Stepper,
    Toggle,
    Dial,
    Action,
}

fn kind(knob: Knob) -> Kind {
    use Knob::*;
    match knob {
        Shell | TargetKind | Seed | SavedCam | SectionPreset | Day | Look | DitherPattern => Kind::Stepper,
        Racks | Lamps | Fresh | Loop | SectionOn | SkyRunning | AutoExposure | WhiteBalance => Kind::Toggle,
        AddCam | DeleteCam | SnapToShot | LookDefaults | TissueAuthored => Kind::Action,
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
        LampPower => (1.0, 4_096.0, true),
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
        Tear(_) => (1.0e3, 1.0e8, true),
        Stretch(_) => (0.005, 5.0, true),
        Forward | Outward => (0.0, 2.0, false),
        Reach => (0.01, 0.6, true),
        Landing => (0.01, 1.0e4, true),
        _ => (0.0, 1.0, false),
    }
}

/// The sidebar's disclosable groups.
const GROUPS: [&str; 7] = ["SHOT", "SCENE", "CAMERA", "SECTION", "SKY", "LOOK", "TISSUE (REVIEW)"];

fn group_rows(group: usize) -> Vec<Knob> {
    use Knob::*;
    match group {
        0 => vec![Shell, Range, Velocity, FromAz, FromEl, TargetKind, Seed],
        1 => vec![TimeSpeed, Loop, Racks, Lamps, LampPower, Fresh, SlowSpan, RunLength],
        2 => vec![Focal, SavedCam, AddCam, DeleteCam],
        3 => vec![SectionOn, SectionPreset, SnapToShot, SectionYaw, SectionPitch, SectionOffset],
        4 => vec![Day, Time, SkyRunning, SkyRate, Weather, AutoExposure, Exposure],
        5 => vec![Look, LookEv, Saturation, Warmth, Gamma, DitherPattern, PaletteLevels, OrderedDither, LdrDither, Retro,
            WhiteBalance, WhiteBalanceStrength, Fog, LookDefaults],
        _ => {
            let mut rows: Vec<Knob> = (0..4).map(Tear).collect();
            rows.extend((0..4).map(Stretch));
            rows.extend([Forward, Outward, Reach, Landing, TissueAuthored]);
            rows
        }
    }
}

const TISSUE_NAMES: [&str; 4] = ["SKIN", "MUSCLE", "BONE", "FLUID"];
/// The world editor's display looks (`editor::RENDER_LOOKS`).
const LOOKS: [(f32, &str); 7] = [(0.0, "STANDARD"), (6.0, "DELTAP REAL"), (1.0, "DELTAP"), (5.0, "DELTAP TINTED"),
    (4.0, "PASTEL"), (2.0, "PICO-8"), (3.0, "DELTAP RAW")];
const DITHERS: [&str; 4] = ["SCREEN BAYER4", "BLUE NOISE", "SPHERE BAYER8", "SCREEN BAYER8"];

// The sidebar (window fractions, top-left origin): every control on the left, the viewport right.
const SIDEBAR_W: f32 = 0.17;
/// The timeline shows (over the bottom of the window) while the cursor is below this height; the
/// viewport runs to the bottom edge.
const BAR_TOP: f32 = 0.975;
const VIEW_H: f32 = 1.0;
/// The timeline's line: its ends and height.
const TL_X0: f32 = 0.0;
const TL_X1: f32 = 1.0;
const TL_Y: f32 = 0.995;
/// The timeline line's thickness (px) and the clip's colour (Blender's orange, linear).
const TL_PX: f32 = 5.0;
const AMBER: [f32; 4] = [0.807, 0.205, 0.006, 1.0];
const RED: [f32; 4] = [0.95, 0.22, 0.18, 1.0];
const GREEN: [f32; 4] = [0.35, 0.9, 0.3, 1.0];
const BLUE: [f32; 4] = [0.3, 0.5, 1.0, 1.0];
/// Very dark grey (linear).
const SIDEBAR_FILL: [f32; 4] = [0.007, 0.007, 0.007, 1.0];
const INK: [f32; 4] = [0.92, 0.92, 0.92, 1.0];
const DIM: [f32; 4] = [0.42, 0.42, 0.42, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const BOX: [f32; 4] = [0.035, 0.035, 0.035, 1.0];
const PANEL_X: f32 = 0.008;
const PANEL_W: f32 = SIDEBAR_W - 2.0 * PANEL_X;
const ROW_H: f32 = 0.0185;
const HEADER_H: f32 = 0.024;
const TEXT: f32 = 0.0145;
const VALUE_X: f32 = PANEL_X + 0.058;
const BTN_W: f32 = 0.011;
const MINUS_X: f32 = PANEL_X + PANEL_W - 2.0 * BTN_W - 0.003;
const PLUS_X: f32 = PANEL_X + PANEL_W - BTN_W;
/// The buttons: label, column, row (three a row), under the title.
const BUTTONS: [(&str, usize, usize); 3] = [("RESET", 0, 0), ("RUN", 1, 0), ("PLAY", 2, 0)];
const BUTTON_H: f32 = 0.028;
const BUTTONS_Y: f32 = 0.044;

/// The groups' region (scrolled when they do not fit) and the status block under it.
const GROUPS_TOP: f32 = BUTTONS_Y + BUTTON_H + 0.016;
const GROUPS_BOTTOM: f32 = 0.86;
/// A dial turns through its whole range over this much vertical drag (window height).
const DIAL_DRAG: f32 = 0.3;
/// The shot handle's distance from the aim point (m).
const SHOT_HANDLE_M: f32 = 2.5;
// The section's rotation handle: box size and the arrows' distance from its pad (window height).
const HANDLE_BOX: f32 = 0.016;
const HANDLE_REACH: f32 = 0.022;
/// The handles show their arrows (and the shot handle its caption) only with the cursor this near.
const HANDLE_NEAR: f32 = 0.06;
/// Screen distance of a centre-pad drag per 45° step.
const HANDLE_DRAG_STEP: f32 = 0.04;

/// A button's rectangle (x, y, w, h).
fn button_rect(col: usize, row: usize) -> [f32; 4] {
    let gap = 0.004;
    let w = (PANEL_W - 2.0 * gap) / 3.0;
    [PANEL_X + col as f32 * (w + gap), BUTTONS_Y + row as f32 * (BUTTON_H + 0.005), w, BUTTON_H]
}

/// One drawn line of the groups: a group's header or one of its rows, at its centre's height.
#[derive(Clone, Copy, Debug)]
enum Line {
    Header(usize),
    Row(Knob),
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

/// A saved camera (`target/fea_range_cameras.json`).
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct SavedCam {
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
    focal_mm: f32,
}

const CAMERAS_PATH: &str = "target/fea_range_cameras.json";
/// The editor's settings between launches (`--shots` neither reads nor writes it).
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
    frames: Vec<Frame>,
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
    help: bool,
    muted: bool,
    idle_s: f32,
    /// The last chain summary for the status line.
    last_chain: String,
    /// Where the section's rotation handle was drawn this frame (window 0..1), if it was.
    handle_at: Option<[f32; 2]>,
    /// Where the shot's handle was drawn this frame (window 0..1), if it was.
    shot_handle_at: Option<[f32; 2]>,
    /// The interface hidden (Tab): the viewport fills the window.
    hide_ui: bool,
    /// Every dial's value as the editor started.
    dial_defaults: Vec<(Knob, f32)>,
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
    /// The sidebar's groups folded shut, and how far the groups are scrolled.
    collapsed: [bool; GROUPS.len()],
    scroll: f32,
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
            frames: Vec::new(),
            run: None,
            frame_at: None,
            past_run: false,
            captured: false,
            shot_t: 0.0,
            playing: true,
            time_speed: 1.0,
            play_clock: 0.0,
            clip: None,
            looping: false,
            eye: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            focal_mm: 35.0,
            speed: 4.0,
            frame_tex,
            frame_image,
            shown_frame: None,
            status: "RESET: a fresh Tiger and crew".into(),
            help: false,
            muted: false,
            idle_s: 0.0,
            last_chain: String::new(),
            handle_at: None,
            shot_handle_at: None,
            axis_labels: [[-1.0, -1.0]; 3],
            cursor: [0.5, 0.5],
            hide_ui: false,
            dial_defaults: Vec::new(),
            snapped: None,
            last_pose: None,
            win: [1920.0, 1080.0],
            frame_size: (0, 0),
            live_image: false,
            tiger_at: TIGER_AT,
            tiger_heading: TIGER_HEADING,
            collapsed: [false, false, false, false, true, true, true],
            scroll: 0.0,
            cameras: std::fs::read_to_string(CAMERAS_PATH).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default(),
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
        self.g.update(&TankInput::default(), dt, &mut self.r);
    }

    /// A fresh Tiger and crew at the review spot (`game_fea_film::fresh_tiger`), settled for a
    /// second of game time.
    fn fresh(&mut self) {
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
        self.shot_t = 0.0;
        self.last_chain.clear();
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
        self.mode = Mode::Live;
        self.fresh();
        self.run = None;
        self.frame_at = None;
        self.past_run = false;
        self.frames.clear();
        self.shown_frame = None;
        self.status = "RESET: a fresh Tiger and crew".into();
    }

    fn run(&mut self) {
        self.apply_tuning();
        if self.scene.fresh_each_run {
            self.fresh();
        }
        self.frames.clear();
        self.shown_frame = None;
        self.run = Some(RunInfo { shot: self.shot, scene: self.scene, tuning: self.tuning, fresh: self.scene.fresh_each_run,
            total: self.total_frames(&self.scene) });
        self.frame_at = None;
        self.past_run = false;
        self.fire();
        self.mode = Mode::Live;
        self.playing = true;
        self.status = format!("RUN: {} at {:.0} m ({:.0} m/s)", self.shot.spec().name, self.shot.range_m, self.shot.impact_velocity());
    }

    /// The run frame the live simulation steps to next, if the run has one left.
    fn next_run_frame(&self) -> Option<u32> {
        let run = self.run?;
        let next = self.frame_at.map_or(0, |f| f + 1);
        (!self.past_run && next < run.total).then_some(next)
    }

    /// Step the run's next frame, keeping its picture for the timeline when `capture`.
    fn run_frame(&mut self, k: u32, capture: bool) {
        let Some(run) = self.run else { return };
        self.advance_frame(k, &run.scene);
        self.frame_at = Some(k);
        if capture {
            self.pose_view();
            match self.r.render_to_rgba(&self.device, &self.queue, self.r.iw, self.r.ih) {
                Ok(rgba) => {
                    let (rgba, w, h) = self.crop_rgba(&rgba);
                    let bytes = rgba.len() * (k as usize + 1);
                    self.upload_image(&rgba, w, h);
                    self.captured = true;
                    self.frames.truncate(k as usize);
                    if bytes <= RECORD_BUDGET_BYTES && self.frames.len() == k as usize {
                        self.frames.push(Frame { t: self.shot_t, rgba, w, h });
                    }
                }
                Err(e) => self.status = format!("RUN: capture failed: {e:#}"),
            }
        }
        if k + 1 == run.total {
            self.status = format!("RUN DONE at {:.2} s: the simulation plays on in real time (SPACE pauses)", self.shot_t);
        }
    }

    /// The live state at run frame `index`: on from the present frame, or (backward) the run
    /// repeated step for step on a fresh Tiger. The game does not repeat a strike bit for bit
    /// (measured 2026-11-03: the chain and scars agree, the crew's tissue and the surface blood
    /// can differ), so a backward seek is a re-run of the same shot, not a copy, and says so.
    fn seek(&mut self, index: u32) {
        self.mode = Mode::Live;
        let Some(run) = self.run else { return };
        let index = index.min(run.total.saturating_sub(1));
        let current = self.frame_at.filter(|_| !self.past_run);
        if current == Some(index) {
            return;
        }
        if current.is_some_and(|c| index > c) {
            for k in current.unwrap_or(0) + 1..=index {
                self.run_frame(k, false);
            }
            self.status = format!("ON TO {:.4} S", self.shot_t);
            return;
        }
        if !run.fresh {
            self.status = "BACK IN TIME NEEDS A RUN ON A FRESH TIGER (FRESH TIGER EACH RUN)".into();
            return;
        }
        let (keep_shot, keep_scene, keep_tuning) = (self.shot, self.scene, self.tuning);
        self.shot = run.shot;
        self.scene = run.scene;
        self.tuning = run.tuning;
        self.apply_tuning();
        let started = std::time::Instant::now();
        self.fresh();
        self.fire();
        self.frame_at = None;
        self.past_run = false;
        for k in 0..=index {
            self.run_frame(k, false);
        }
        self.shot = keep_shot;
        self.scene = keep_scene;
        self.tuning = keep_tuning;
        self.apply_tuning();
        self.status = format!("BACK TO {:.4} S: THE SAME SHOT RE-RUN IN {:.1} S (TISSUE DETAIL CAN DIFFER FROM THE FIRST RUN)",
            self.shot_t, started.elapsed().as_secs_f32());
    }

    /// The camera, the section and the crew drawing for this frame.
    fn pose_view(&mut self) {
        self.apply_sky();
        // The lamps, fires and scene emitters from the present state (the simulation may be held:
        // a lamp switched or its power changed shows without a game step).
        self.g.drive_lights(&mut self.r);
        self.g.drive_armor_scars(&mut self.r);
        self.g.drive_singleplayer_crew_rigs_for_view(None, &mut self.r);
        self.r.camera.pos = self.eye;
        self.r.camera.yaw = self.yaw;
        self.r.camera.pitch = self.pitch;
        self.r.camera.fov_y = self.fov_deg().to_radians();
        // A moved camera restarts the transport's history (as `fea-film` does): the accumulator's
        // reprojection otherwise smears the view while it moves. Fewer samples, sharp frames.
        let pose = (self.eye, self.yaw, self.pitch, self.focal_mm);
        if self.last_pose.is_some_and(|p| p != pose) {
            self.r.reset_history();
        }
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
            }
            None => self.r.set_film_section(None, &[]),
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
        self.r.solid_meshes_within(lo, hi)
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
                    self.shot.set_dir_hull(hull_inv.transform_vector3(dir));
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
            Knob::Shell => ("SHELL".into(), self.shot.spec().name.to_uppercase()),
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
            Knob::TimeSpeed => ("TIME SPEED".into(), format!("X{:.2}{}", self.time_speed,
                if self.time_speed < 1.0 { " (SLOWER)" } else if self.time_speed > 1.0 { " (FASTER)" } else { "" })),
            Knob::Loop => ("LOOP".into(), if self.looping { "ON" } else { "OFF" }.into()),
            Knob::Focal => ("FOCAL LENGTH".into(), format!("{:.0} MM ({:.0} DEG)", self.focal_mm, self.fov_deg())),
            Knob::SavedCam => ("CAMERA".into(), match self.camera_at {
                Some(i) => format!("{} OF {} (KEY {})", i + 1, self.cameras.len(), i + 1),
                None if self.cameras.is_empty() => "NONE SAVED".into(),
                None => format!("FREE ({} SAVED)", self.cameras.len()),
            }),
            Knob::AddCam => ("ADD CAMERA (K)".into(), "SAVE THIS VIEW".into()),
            Knob::DeleteCam => ("DELETE CAMERA".into(), self.camera_at.map_or("-".into(), |i| format!("NUMBER {}", i + 1))),
            Knob::AutoExposure => ("AUTO EXPOSURE".into(), if self.sky.exposure.is_none() { "ON (EYE)" } else { "OFF (HELD)" }.into()),
            Knob::Look => ("LOOK".into(), LOOKS.iter().find(|(v, _)| (*v - self.r.look).abs() < 0.1).map_or("?", |(_, n)| *n).into()),
            Knob::LookEv => ("EXPOSURE COMP".into(), format!("{:+.2} EV", self.r.display_grade[0].max(1e-4).log2())),
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
                let gain = (review_lamp_gain() * 2f32.powf(s)).clamp(1.0, 4096.0);
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
            Knob::SectionYaw => self.section.yaw = ((self.section.yaw + 45.0 * s + 180.0).rem_euclid(360.0)) - 180.0,
            Knob::SectionPitch => self.section.pitch = (self.section.pitch + 45.0 * s).clamp(-90.0, 90.0),
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
            Knob::Loop => self.looping = !self.looping,
            Knob::SkyRunning => self.sky.running = !self.sky.running,
            Knob::SavedCam => {
                if !self.cameras.is_empty() {
                    let n = self.cameras.len() as i32;
                    let next = self.camera_at.map_or(if dir > 0.0 { 0 } else { n - 1 }, |i| (i as i32 + dir as i32).rem_euclid(n));
                    self.view_camera(next as usize);
                }
            }
            Knob::AddCam => {
                self.cameras.push(SavedCam { eye: self.eye.to_array(), yaw: self.yaw, pitch: self.pitch, focal_mm: self.focal_mm });
                self.camera_at = Some(self.cameras.len() - 1);
                self.save_cameras();
                self.status = format!("CAMERA {} SAVED (KEY {})", self.cameras.len(), self.cameras.len());
            }
            Knob::DeleteCam => {
                if let Some(i) = self.camera_at.take() {
                    self.cameras.remove(i);
                    self.save_cameras();
                    self.status = format!("CAMERA {} DELETED", i + 1);
                }
            }
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
            Knob::Focal => self.focal_mm = v,
            Knob::SectionYaw => { self.section.yaw = v; self.section.on = true; }
            Knob::SectionPitch => { self.section.pitch = v; self.section.on = true; }
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
        if let Some(c) = self.cameras.get(i).copied() {
            self.eye = Vec3::from_array(c.eye);
            self.yaw = c.yaw;
            self.pitch = c.pitch;
            self.focal_mm = c.focal_mm;
            self.camera_at = Some(i);
            self.status = format!("THROUGH CAMERA {} ({:.0} MM)", i + 1, c.focal_mm);
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
    /// speed: the run's frames (their pictures kept for the timeline), looping over the clip or
    /// the run when LOOP is on, stopping at a clip's end without it, and real time past the run.
    fn tick(&mut self, dt: f32) {
        self.captured = false;
        if self.sky.running {
            self.sky.local_h += dt * self.sky.rate / 3600.0;
            while self.sky.local_h >= 24.0 {
                self.sky.local_h -= 24.0;
                self.sky.day += 1;
            }
        }
        if self.playing && self.mode == Mode::Live {
            if self.next_run_frame().is_some() {
                self.play_clock += self.time_speed;
                let mut frames = 0;
                while self.play_clock >= 1.0 && frames < 4 {
                    self.play_clock -= 1.0;
                    frames += 1;
                    let Some(k) = self.next_run_frame() else { break };
                    self.run_frame(k, true);
                    if self.at_play_end(k) {
                        break;
                    }
                }
                self.play_clock = self.play_clock.min(1.0);
            } else if self.looping && self.run.is_some() {
                self.loop_back();
            } else {
                self.play_clock += dt * self.time_speed;
                let mut steps = 0;
                while self.play_clock >= TICK && steps < 4 {
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
        if self.looping {
            self.loop_back();
        } else {
            self.playing = false;
            self.status = "CLIP END: PAUSED (LOOP REPEATS IT)".into();
        }
        true
    }

    /// Back to the clip's start (or the run's), by re-running the shot to it.
    fn loop_back(&mut self) {
        let start = self.clip.map_or(0, |c| c.0);
        self.seek(start);
        self.status = format!("LOOP: BACK TO {:.4} S (THE SHOT RE-RUN; TISSUE DETAIL CAN DIFFER)", self.shot_t);
    }

    /// Show recorded frame `i` in the frame image (uploaded once per change).
    fn show_frame(&mut self, i: usize) {
        if self.shown_frame == Some(i) {
            return;
        }
        let Some(frame) = self.frames.get(i) else { return };
        let (rgba, w, h) = (frame.rgba.clone(), frame.w, frame.h);
        self.upload_image(&rgba, w, h);
        self.shown_frame = Some(i);
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
        let (aim, dir) = self.aim_line();
        let entry = self.entry();
        let start = entry.map_or(aim, |(p, _)| p);
        self.line(start, aim, [0.6, 0.6, 0.6, 0.7]);
        if let Some(p) = self.project(aim) {
            self.dot(p, 0.010, INK);
        }
        // The shot's handle: a point on a sphere about the aim point, on the line the round comes
        // in along; dragged over the sphere, it turns the shot.
        let handle = aim - dir * SHOT_HANDLE_M;
        self.line(handle, aim, [0.75, 0.75, 0.75, 0.9]);
        let f = self.forward();
        let u = f.cross(Vec3::Y).normalize_or_zero();
        let v = u.cross(f).normalize_or_zero();
        for k in 0..48 {
            let a = k as f32 / 48.0 * std::f32::consts::TAU;
            if let Some(p) = self.project(aim + (u * a.cos() + v * a.sin()) * SHOT_HANDLE_M) {
                self.dot(p, 0.0015, [0.5, 0.5, 0.5, 0.5]);
            }
        }
        if let Some(p) = self.project(handle) {
            self.dot(p, 0.010, INK);
            self.dot(p, 0.005, BLACK);
            let aspect = self.win[0] / self.win[1].max(1.0);
            if ((self.cursor[0] - p[0]) * aspect).hypot(self.cursor[1] - p[1]) < HANDLE_NEAR {
                self.text("SHOT", p[0], p[1] + 0.016, TEXT * 0.65, INK, crate::ui::Align::Center);
            }
            self.shot_handle_at = Some(p);
        }
        if let Some((e, n)) = entry {
            if let Some(p) = self.project(e) {
                self.dot(p, 0.009, [0.95, 0.15, 0.1, 1.0]);
            }
            self.line(e, e + n * 0.6, [0.6, 0.6, 0.6, 0.8]);
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

    fn shot_handle_hit(&self, c: [f32; 2]) -> bool {
        let aspect = self.win[0] / self.win[1].max(1.0);
        self.shot_handle_at.is_some_and(|p| ((c[0] - p[0]) * aspect).hypot(c[1] - p[1]) < 0.012)
    }

    /// The shot's handle dragged to `c`: the point of its sphere (about the aim point) under the
    /// cursor, the near side's, or the sphere's outline nearest the ray when it misses; the round
    /// comes in from there toward the aim point.
    fn drag_shot_handle(&mut self, c: [f32; 2]) {
        let (aim, _) = self.aim_line();
        let (o, d) = self.ray(c);
        let to = aim - o;
        let along = to.dot(d);
        let closest = o + d * along;
        let off = closest - aim;
        let r2 = SHOT_HANDLE_M * SHOT_HANDLE_M;
        let point = if off.length_squared() <= r2 {
            o + d * (along - (r2 - off.length_squared()).sqrt())
        } else {
            aim + off.normalize_or_zero() * SHOT_HANDLE_M
        };
        let dir = (aim - point).normalize_or_zero();
        if dir != Vec3::ZERO {
            self.shot.set_dir_hull(self.g.player.hull_model().inverse().transform_vector3(dir));
            self.status = format!("SHOT FROM BEARING {:+.0}, ELEVATION {:+.0}", self.shot.from_az, self.shot.from_el);
        }
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

    /// The sidebar's width (none while the interface is hidden).
    fn side(&self) -> f32 {
        if self.hide_ui { 0.0 } else { SIDEBAR_W }
    }

    /// Frame the Tiger: the camera keeps its direction and stands 10 m from the hull's middle.
    fn frame_tiger(&mut self) {
        let target = self.g.player.hull_model().transform_point3(SECTION_PIVOT);
        self.eye = target - self.forward() * 10.0;
        self.camera_at = None;
        self.status = "FRAMED THE TIGER".into();
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
                "outward": t.outward, "active_reach_m": t.active_reach_m, "landing_breakup": t.landing_breakup},
            "look": {"look": self.r.look, "grade": self.r.display_grade, "dither": self.r.display_dither_pattern,
                "palette_levels": d.palette_levels, "ordered_dither": d.ordered_dither, "ldr_dither": d.ldr_dither,
                "retro": d.retro_strength, "white_balance": d.ambient_white_balance,
                "white_balance_strength": d.ambient_white_balance_strength, "tuned": self.r.display_tuning.is_some(), "fog": self.fog},
            "lamp_gain": review_lamp_gain(), "time_speed": self.time_speed, "looping": self.looping,
            "camera": {"eye": self.eye.to_array(), "yaw": self.yaw, "pitch": self.pitch, "focal_mm": self.focal_mm},
            "tiger": {"at": self.tiger_at.to_array(), "heading": self.tiger_heading},
            "collapsed": self.collapsed,
        })
    }

    fn save_state(&self) {
        let _ = std::fs::write(STATE_PATH, serde_json::to_string_pretty(&self.state_json()).unwrap_or_default());
    }

    /// Settings from the last launch (whatever is missing or unreadable keeps its default).
    fn load_state(&mut self) {
        let Some(v) = std::fs::read_to_string(STATE_PATH).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return };
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
        if let Some(x) = f(&se["offset"]) { self.section.offset = x; }
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
        self.apply_tuning();
        let l = &v["look"];
        if let Some(x) = f(&l["look"]) { self.r.look = x; }
        if let Some(x) = arr4(&l["grade"]) { self.r.display_grade = x; }
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
        if let Some(x) = v["looping"].as_bool() { self.looping = x; }
        let c = &v["camera"];
        if let Some(e) = c["eye"].as_array().filter(|e| e.len() == 3) {
            self.eye = Vec3::new(f(&e[0]).unwrap_or(0.0), f(&e[1]).unwrap_or(0.0), f(&e[2]).unwrap_or(0.0));
            if let (Some(y), Some(p)) = (f(&c["yaw"]), f(&c["pitch"])) { self.yaw = y; self.pitch = p; }
        }
        if let Some(x) = f(&c["focal_mm"]) { self.focal_mm = x; }
        if let Some(a) = v["tiger"]["at"].as_array().filter(|a| a.len() == 2) {
            self.tiger_at = Vec2::new(f(&a[0]).unwrap_or(TIGER_AT.x), f(&a[1]).unwrap_or(TIGER_AT.y));
        }
        if let Some(x) = f(&v["tiger"]["heading"]) { self.tiger_heading = x; }
        if let Some(c) = v["collapsed"].as_array().filter(|c| c.len() == GROUPS.len()) {
            for (slot, x) in self.collapsed.iter_mut().zip(c) { *slot = x.as_bool().unwrap_or(*slot); }
        }
        self.status = format!("SETTINGS FROM THE LAST LAUNCH ({STATE_PATH})");
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
        self.camera_at = None;
    }

    /// Pan: the view slides across, the point under the cursor following it.
    fn pan(&mut self, pivot: Vec3, dx: f32, dy: f32) {
        let f = self.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(f).normalize_or_zero();
        let depth = (pivot - self.eye).dot(f).max(0.5);
        let span = 2.0 * depth * (self.fov_deg().to_radians() * 0.5).tan();
        let aspect = (1.0 - self.side()) * self.win[0] / (VIEW_H * self.win[1].max(1.0));
        self.eye += (-right * dx / (1.0 - self.side()) * aspect + up * dy / VIEW_H) * span;
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
        self.snapped = Some((axis, positive));
        self.camera_at = None;
        self.status = format!("VIEW ALONG THE HULL'S {}{} AXIS", if positive { "+" } else { "-" }, ["X", "Y", "Z"][axis]);
    }

    /// The hull's axes as the camera sees them (X red, Y green, Z blue), in the viewport's corner;
    /// a label clicked looks along its axis.
    fn draw_axes_gizmo(&mut self) {
        let aspect = self.win[0] / self.win[1].max(1.0);
        let f = self.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(f).normalize_or_zero();
        let centre = [self.side() + 0.075 / aspect, VIEW_H - 0.075];
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

    /// Turn the section by whole 45° steps: `horizontal` about the hull's vertical (yaw),
    /// `vertical` tilting the normal (pitch, held within ±90°). The plane first snaps to the
    /// 45° grid, so free adjustments land back on it.
    fn rotate_section(&mut self, horizontal: i32, vertical: i32) {
        let snap = |a: f32| (a / 45.0).round() * 45.0;
        self.section.on = true;
        self.section.yaw = ((snap(self.section.yaw) + 45.0 * horizontal as f32 + 180.0).rem_euclid(360.0)) - 180.0;
        self.section.pitch = (snap(self.section.pitch) + 45.0 * vertical as f32).clamp(-90.0, 90.0);
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
        let at = self.project(centre).filter(|p| p[0] > self.side() + margin && p[0] < 1.0 - margin
            && p[1] > HANDLE_REACH + HANDLE_BOX && p[1] < VIEW_H - HANDLE_REACH - 2.0 * HANDLE_BOX)
            .unwrap_or([1.0 - margin - 0.01, VIEW_H - HANDLE_REACH - 2.0 * HANDLE_BOX]);
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

    /// `text` broken into lines of at most `chars` characters at spaces.
    fn wrap(text: &str, chars: usize) -> Vec<String> {
        let mut lines = vec![String::new()];
        for word in text.split_whitespace() {
            let last = lines.last_mut().expect("one line");
            if !last.is_empty() && last.len() + 1 + word.len() > chars {
                lines.push(word.chars().take(chars).collect());
            } else {
                if !last.is_empty() {
                    last.push(' ');
                }
                last.push_str(word);
            }
        }
        lines
    }

    /// Characters a sidebar line holds at text size `size` (the regular pixel font is about 0.45
    /// of its size wide).
    fn line_chars(&self, size: f32) -> usize {
        ((PANEL_W * self.win[0]) / (0.45 * size * self.win[1])).max(10.0) as usize
    }

    /// The groups' lines, each with the height of its centre (scrolled).
    fn lines(&self) -> Vec<(Line, f32)> {
        let mut y = GROUPS_TOP - self.scroll;
        let mut out = Vec::new();
        for g in 0..GROUPS.len() {
            out.push((Line::Header(g), y + HEADER_H * 0.5));
            y += HEADER_H;
            if !self.collapsed[g] {
                for k in group_rows(g) {
                    out.push((Line::Row(k), y + ROW_H * 0.5));
                    y += ROW_H;
                }
            }
            y += 0.004;
        }
        out
    }

    /// How far the groups can scroll.
    fn max_scroll(&self) -> f32 {
        let h: f32 = (0..GROUPS.len()).map(|g| HEADER_H + 0.004
            + if self.collapsed[g] { 0.0 } else { ROW_H * group_rows(g).len() as f32 }).sum();
        (h - (GROUPS_BOTTOM - GROUPS_TOP)).max(0.0)
    }

    /// The group line under `c` (only lines inside the groups' region).
    fn line_at(&self, c: [f32; 2]) -> Option<Line> {
        if c[0] < PANEL_X || c[0] > PANEL_X + PANEL_W || c[1] < GROUPS_TOP || c[1] > GROUPS_BOTTOM {
            return None;
        }
        self.lines().into_iter().find(|(line, y)| {
            let h = if matches!(line, Line::Header(_)) { HEADER_H } else { ROW_H };
            (c[1] - y).abs() <= h * 0.5
        }).map(|(line, _)| line)
    }

    /// A label on an inverted (white) box, like the panel headers.
    fn label_box(&mut self, text: &str, x: f32, y: f32, size: f32) -> f32 {
        let w = text.len() as f32 * 1.0 * size * self.win[1] / self.win[0] + 0.008;
        self.r.fill_rect(x, y - size * 0.62, w, size * 1.24, INK);
        self.text_bold(text, x + 0.004, y, size, BLACK, crate::ui::Align::Left);
        w
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
        self.shot_handle_at = None;
        let scrubbing = matches!(self.mode, Mode::Scrub(_));
        // The viewport: a dragged timeline's stored frame, or the live frame captured this frame
        // for the timeline (or, headless, the live frame); otherwise `render` blits the live view.
        let shown = match self.mode {
            Mode::Scrub(i) => Some(i),
            Mode::Live => None,
        };
        if let Some(i) = shown {
            self.show_frame(i);
        }
        if shown.is_some() || self.captured || self.live_image {
            self.r.draw_image(self.frame_image, self.side(), 0.0, 1.0 - self.side(), VIEW_H, [1.0, 1.0, 1.0, 1.0]);
        }
        if self.hide_ui {
            self.draw_timeline(cursor);
            return;
        }
        if self.mode == Mode::Live {
            self.draw_gizmos();
        }
        self.draw_axes_gizmo();
        // The sidebar.
        self.r.fill_rect(0.0, 0.0, SIDEBAR_W, 1.0, SIDEBAR_FILL);
        let mode = match (self.mode, self.run) {
            (Mode::Scrub(i), _) => format!("PREVIEW {}", i + 1),
            (Mode::Live, _) if !self.playing => "PAUSED".into(),
            (Mode::Live, Some(run)) if !self.past_run => format!("RUN {}/{}", self.frame_at.map_or(0, |f| f + 1), run.total),
            (Mode::Live, _) => "REAL TIME".into(),
        };
        let t = match self.mode {
            Mode::Scrub(i) => self.frames.get(i).map_or(0.0, |f| f.t),
            _ => self.shot_t,
        };
        self.label_box("FEA RANGE", PANEL_X, 0.022, TEXT * 0.85);
        self.text(&format!("{mode}  T {:.4} S", t), PANEL_X + PANEL_W, 0.022, TEXT * 0.85, INK, crate::ui::Align::Right);
        // Buttons.
        for (label, col, row) in BUTTONS {
            let [x, y, w, h] = button_rect(col, row);
            let lit = label == "PLAY" && !self.playing;
            let hover = cursor[0] >= x && cursor[0] <= x + w && cursor[1] >= y && cursor[1] <= y + h;
            self.r.fill_rect(x, y, w, h, if lit { INK } else if hover { [0.09, 0.09, 0.09, 1.0] } else { BOX });
            let text = match label { "PLAY" if self.playing => "PAUSE", other => other };
            self.text_bold(text, x + w * 0.5, y + h * 0.5, TEXT * 0.9, if lit { BLACK } else { INK }, crate::ui::Align::Center);
        }
        self.draw_timeline(cursor);
        // The groups.
        let aspect = self.win[0] / self.win[1].max(1.0);
        for (line, y) in self.lines() {
            let h = if matches!(line, Line::Header(_)) { HEADER_H } else { ROW_H };
            if y - h * 0.5 < GROUPS_TOP - 1.0e-4 || y + h * 0.5 > GROUPS_BOTTOM + 1.0e-4 {
                continue;
            }
            match line {
                Line::Header(g) => {
                    self.label_box(GROUPS[g], PANEL_X, y, TEXT * 0.8);
                    self.text_bold(if self.collapsed[g] { "+" } else { "-" }, PANEL_X + PANEL_W, y, TEXT, DIM,
                        crate::ui::Align::Right);
                }
                Line::Row(knob) => {
                    let hover = cursor[0] >= PANEL_X && cursor[0] <= PANEL_X + PANEL_W && (cursor[1] - y).abs() <= ROW_H * 0.5;
                    if hover {
                        self.r.fill_rect(PANEL_X - 0.003, y - ROW_H * 0.5, PANEL_W + 0.006, ROW_H, [0.05, 0.05, 0.05, 1.0]);
                    }
                    let (label, value) = self.knob_label(knob);
                    self.text(&label, PANEL_X + 0.004, y, TEXT * 0.8, DIM, crate::ui::Align::Left);
                    match kind(knob) {
                        Kind::Stepper => {
                            self.text(&value, VALUE_X, y, TEXT * 0.8, INK, crate::ui::Align::Left);
                            for (x, sign) in [(MINUS_X, "<"), (PLUS_X, ">")] {
                                self.r.fill_rect(x, y - ROW_H * 0.4, BTN_W, ROW_H * 0.8, BOX);
                                self.text_bold(sign, x + BTN_W * 0.5, y, TEXT * 0.85, INK, crate::ui::Align::Center);
                            }
                        }
                        Kind::Toggle => {
                            let on = matches!(value.as_str(), "ON" | "YES" | "LOADED") || value.starts_with("ON ");
                            self.text(&value, VALUE_X, y, TEXT * 0.8, if on { INK } else { DIM }, crate::ui::Align::Left);
                            let w = 2.0 * BTN_W + 0.003;
                            self.r.fill_rect(MINUS_X, y - ROW_H * 0.4, w, ROW_H * 0.8, if on { INK } else { BOX });
                            self.text_bold(if on { "ON" } else { "OFF" }, MINUS_X + w * 0.5, y, TEXT * 0.75,
                                if on { BLACK } else { DIM }, crate::ui::Align::Center);
                        }
                        Kind::Dial => {
                            self.text(&value, VALUE_X, y, TEXT * 0.8, INK, crate::ui::Align::Left);
                            let t = self.dial_t(knob);
                            self.dial(PANEL_X + PANEL_W - ROW_H * 0.5 / aspect, y, t);
                        }
                        Kind::Action => {
                            let w = PANEL_X + PANEL_W - VALUE_X;
                            self.r.fill_rect(VALUE_X, y - ROW_H * 0.4, w, ROW_H * 0.8, BOX);
                            self.text(&value, VALUE_X + w * 0.5, y, TEXT * 0.75, INK, crate::ui::Align::Center);
                        }
                    }
                }
            }
        }
        if self.max_scroll() > 0.0 {
            let span = GROUPS_BOTTOM - GROUPS_TOP;
            let k = span / (span + self.max_scroll());
            let y = GROUPS_TOP + self.scroll / (span + self.max_scroll()) * span;
            self.r.fill_rect(SIDEBAR_W - 0.003, y, 0.0015, span * k, DIM);
        }
        // Status, the chain and the crew, wrapped to the sidebar.
        self.r.fill_rect(PANEL_X, GROUPS_BOTTOM + 0.006, PANEL_W, 0.001, DIM);
        let mut y = GROUPS_BOTTOM + 0.020;
        let alive = CrewRole::ALL.iter().filter(|&&role| !self.g.crew_solo.man(role).dead).count();
        let mut notes: Vec<(String, [f32; 4])> = vec![(self.status.to_uppercase(), INK)];
        notes.push((format!("CREW ALIVE {alive}/5. {}", self.last_chain.to_uppercase()), DIM));
        if self.section.on {
            notes.push(("SECTION FOR THE CAMERA ONLY: LIGHT AND PHYSICS SEE THE WHOLE TANK".into(), DIM));
        }
        if scrubbing {
            notes.push(("PREVIEW: LET GO OF THE TIMELINE TO GO LIVE AT THIS FRAME".into(), INK));
        }
        let chars = self.line_chars(TEXT * 0.75);
        'notes: for (text, colour) in notes {
            for line in Self::wrap(&text, chars).into_iter().take(3) {
                if y > 0.975 {
                    break 'notes;
                }
                self.text(&line, PANEL_X, y, TEXT * 0.75, colour, crate::ui::Align::Left);
                y += 0.018;
            }
        }
        if self.help {
            let lines = [
                "RMB HOLD: LOOK    WASD/QE: FLY    SHIFT: FAST    CTRL: SLOW    WHEEL: FLY SPEED    [ ]: FOCAL LENGTH",
                "ALT+LMB: ORBIT    ALT+MMB (OR MMB): PAN    ALT+RMB: DOLLY    CLICK X/Y/Z (CORNER): VIEW ALONG IT, AGAIN TO FLIP",
                "LMB ON THE TANK: AIM (DIRECTION FROM THE CAMERA)    CTRL+LMB: AIM, KEEP DIRECTION",
                "DRAG THE SHOT HANDLE (OR SHIFT+LMB DRAG): TURN THE SHOT",
                "C: SECTION ON/OFF    V: FACING THE CAMERA    G: IN THE SHOT'S PLANE    HANDLE OR ALT+ARROWS: TURN 45",
                "SIDEBAR: DRAG A DIAL UP/DOWN (SHIFT FINE), WHEEL ON A VALUE STEPS, WHEEL ON A LABEL SCROLLS, CLICK A HEADER FOLDS",
                "K: ADD CAMERA    1-9: VIEW THROUGH CAMERA    T: MOVE THE TIGER TO THE SNOW UNDER THE CURSOR",
                "ENTER: RUN    BACKSPACE: RESET    SPACE: PAUSE/PLAY (THE SIMULATION ALWAYS PLAYS UNLESS PAUSED)",
                "LEFT/RIGHT: ONE FRAME BACK/ON (PAUSES; SHIFT TEN)    HOME/END: THE RUN'S START/END    DRAG THE TIMELINE",
                "BACK IN TIME RE-RUNS THE SAME SHOT TO THAT FRAME (A FEW SECONDS; TISSUE DETAIL CAN DIFFER)",
                "TAB: HIDE THE INTERFACE    F: FRAME THE TIGER    RMB ON A DIAL: RESET IT    SETTINGS KEPT BETWEEN LAUNCHES",
                "P: PRINT TISSUE KNOBS    M: MUTE    F12: SCREENSHOT TO CAPTURES    H: THIS HELP    ESC TWICE: QUIT",
            ];
            let x = self.side() + 0.01;
            self.r.fill_rect(x, 0.02, 0.98 - x, 0.03 * lines.len() as f32 + 0.02, [0.0, 0.0, 0.0, 0.85]);
            for (k, l) in lines.iter().enumerate() {
                self.text(l, x + 0.01, 0.04 + 0.03 * k as f32, TEXT * 0.9, INK, crate::ui::Align::Left);
            }
        } else {
            self.text("H: HELP", PANEL_X + PANEL_W, BAR_TOP - 0.01, TEXT * 0.75, DIM, crate::ui::Align::Right);
        }
    }

    /// The timeline (`draw_ui`).
    fn draw_timeline(&mut self, cursor: [f32; 2]) {
        // The timeline: a 5 px white line along the bottom edge from the left to the frame the
        // simulation stands at; a clip in orange; on hover the playhead's tick and a readout.
        let hovered = cursor[1] >= BAR_TOP || matches!(self.mode, Mode::Scrub(_));
        if let Some(run) = self.run {
            let n = run.total.max(1) as f32;
            let px = 1.0 / self.win[1].max(1.0);
            let thick = TL_PX * px;
            let at = match self.mode {
                Mode::Scrub(i) => i as f32 + 1.0,
                Mode::Live if self.past_run => n,
                Mode::Live => self.frame_at.map_or(0.0, |f| f as f32 + 1.0),
            };
            let x = |k: f32| TL_X0 + (TL_X1 - TL_X0) * k / n;
            if let Some((a, b)) = self.clip {
                self.r.fill_rect(x(a as f32), 1.0 - thick, x(b as f32 + 1.0) - x(a as f32), thick, AMBER);
            }
            self.r.fill_rect(TL_X0, 1.0 - thick, x(at) - TL_X0, thick, INK);
            if hovered {
                let head = x(at);
                self.r.fill_rect(head - 1.0 / self.win[0], 1.0 - 4.0 * thick, 2.0 / self.win[0], 4.0 * thick, INK);
                let t = match self.mode {
                    Mode::Scrub(i) => self.frames.get(i).map_or(0.0, |f| f.t),
                    _ => self.shot_t,
                };
                let clip = self.clip.map_or(String::new(), |(a, b)| format!("  CLIP {}-{}{}", a + 1, b + 1, if self.looping { " LOOP" } else { "" }));
                let label = format!("{:.0}/{} {t:.3} S{clip}", at, run.total);
                let align = if head > 0.85 { crate::ui::Align::Right } else { crate::ui::Align::Left };
                self.text(&label, head + if head > 0.85 { -0.004 } else { 0.004 }, 1.0 - 4.0 * thick - 0.008, TEXT * 0.7, INK, align);
            }
        } else if hovered {
            self.text("NO RUN YET (ENTER RUNS)", 0.5, 1.0 - 0.012, TEXT * 0.7, DIM, crate::ui::Align::Center);
        }
    }

    /// A press in the sidebar: what it did.
    fn click_ui(&mut self, cursor: [f32; 2], fine: bool, coarse: bool) -> Click {
        if cursor[0] > self.side() {
            return Click::Viewport;
        }
        for (label, col, row) in BUTTONS {
            let [x, y, w, h] = button_rect(col, row);
            if cursor[0] >= x && cursor[0] <= x + w && cursor[1] >= y && cursor[1] <= y + h {
                self.button(label);
                return Click::Used;
            }
        }
        match self.line_at(cursor) {
            Some(Line::Header(g)) => {
                self.collapsed[g] = !self.collapsed[g];
                self.scroll = self.scroll.min(self.max_scroll());
            }
            Some(Line::Row(knob)) => match kind(knob) {
                Kind::Dial => return Click::Dial(knob),
                Kind::Stepper => {
                    let dir = if cursor[0] >= MINUS_X && cursor[0] < PLUS_X { -1.0 } else { 1.0 };
                    self.adjust(knob, dir, fine, coarse);
                }
                Kind::Toggle | Kind::Action => self.adjust(knob, 1.0, fine, coarse),
            },
            None => {}
        }
        Click::Used
    }

    /// The wheel in the sidebar: on a row's value it steps the row, elsewhere it scrolls.
    fn wheel_ui(&mut self, cursor: [f32; 2], dy: f32, fine: bool, coarse: bool) {
        match self.line_at(cursor) {
            Some(Line::Row(knob)) if cursor[0] >= VALUE_X && kind(knob) != Kind::Action => self.adjust(knob, dy.signum(), fine, coarse),
            _ => self.scroll = (self.scroll - dy * ROW_H * 2.0).clamp(0.0, self.max_scroll()),
        }
    }

    fn track_hit(&self, cursor: [f32; 2]) -> Option<usize> {
        let n = self.run.map_or(0, |r| r.total as usize);
        (n > 0 && cursor[1] >= BAR_TOP && cursor[0] >= TL_X0 - 0.01 && cursor[0] <= TL_X1 + 0.01)
            .then(|| ((((cursor[0] - TL_X0) / (TL_X1 - TL_X0)).clamp(0.0, 1.0) * n as f32) as usize).min(n - 1))
    }

    fn button(&mut self, label: &str) {
        match label {
            "RESET" => self.reset(),
            "RUN" => self.run(),
            "PLAY" => self.playing = !self.playing,
            _ => {}
        }
    }

    /// One frame on or back (the simulation pauses): on is the run's next frame (1/480 s inside
    /// the slow-motion span, 1/30 s after it) or 1/30 s of real time past the run; back seeks.
    fn step_frame(&mut self, forward: bool, count: u32) {
        self.playing = false;
        if forward {
            for _ in 0..count {
                match self.next_run_frame() {
                    Some(k) => self.run_frame(k, true),
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
        let viewport = (1.0 - self.side()) * self.win[0] / (VIEW_H * self.win[1].max(1.0));
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
        if c[0] < self.side() || c[1] > VIEW_H {
            return None;
        }
        let k = self.crop();
        Some([k[0] + (c[0] - self.side()) / (1.0 - self.side()) * k[2], k[1] + c[1] / VIEW_H * k[3]])
    }

    /// A point of the internal frame as a window point; None outside the viewport.
    fn to_window(&self, f: [f32; 2]) -> Option<[f32; 2]> {
        let k = self.crop();
        let (x, y) = ((f[0] - k[0]) / k[2], (f[1] - k[1]) / k[3]);
        ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)).then(|| [self.side() + x * (1.0 - self.side()), y * VIEW_H])
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
        (1..2000).map(|k| from + dir * k as f32).find(|p| p.y < self.g.hf.height_at(p.x, p.z))
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
        if self.mode == Mode::Live && !self.captured {
            self.pose_view();
            self.r.set_active_view(0);
            let rect = crate::renderer::BlitRect { src_uv: self.crop(),
                dst_px: [self.side() * w as f32, 0.0, (1.0 - self.side()) * w as f32, VIEW_H * h as f32], clear: true };
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
    /// LMB on the shot's handle: moved over its sphere about the aim point.
    ShotHandle,
    /// LMB on a dial: the dial, its position when taken and the cursor's height then.
    Dial(Knob, f32, f32),
    /// LMB on the section handle's pad: 45° steps per `HANDLE_DRAG_STEP` of motion (the motion
    /// not yet spent).
    Rotate([f32; 2]),
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
}

impl App {
    fn busy(&self) -> bool {
        self.state.as_ref().is_some_and(|s| {
            let r = &s.range;
            r.playing
        }) || self.input.look || !self.input.keys.is_empty() || self.input.drag.is_some()
            || self.active.is_some_and(|t| t.elapsed().as_secs_f32() < IDLE_AFTER_S)
    }

    fn key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        let Some(st) = self.state.as_mut() else { return };
        let rg = &mut st.range;
        match code {
            KeyCode::Escape => {
                if self.escape.is_some_and(|t| t.elapsed().as_secs_f32() < 2.0) {
                    rg.save_state();
                    event_loop.exit();
                } else {
                    self.escape = Some(std::time::Instant::now());
                    rg.status = "ESC AGAIN TO QUIT".into();
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter => rg.run(),
            KeyCode::Backspace => rg.reset(),
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
            KeyCode::KeyF => rg.frame_tiger(),
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
            KeyCode::F12 => {
                let path = crate::capture_target(&format!("fea_range_{}.png", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())));
                let (w, h) = (rg.r.iw * 3, rg.r.ih * 3);
                if rg.mode == Mode::Live {
                    rg.pose_view();
                }
                rg.draw_ui(self.input.cursor);
                rg.status = match rg.r.render_to_png_hud(&rg.device, &rg.queue, &path, w, h) {
                    Ok(()) => format!("SCREENSHOT {path}"),
                    Err(e) => format!("SCREENSHOT FAILED {e:#}"),
                };
            }
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
        // Fly camera.
        if input.look {
            rg.yaw += input.mouse.x * 0.0025;
            rg.pitch = (rg.pitch - input.mouse.y * 0.0025).clamp(-1.55, 1.55);
        }
        input.mouse = Vec2::ZERO;
        let f = rg.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let mut v = Vec3::ZERO;
        let held = |k: KeyCode| input.keys.contains(&k);
        if !input.ctrl || input.look {
            if held(KeyCode::KeyW) { v += f; }
            if held(KeyCode::KeyS) { v -= f; }
            if held(KeyCode::KeyD) { v += right; }
            if held(KeyCode::KeyA) { v -= right; }
            if held(KeyCode::KeyE) { v += Vec3::Y; }
            if held(KeyCode::KeyQ) { v -= Vec3::Y; }
        }
        let boost = if input.shift { 4.0 } else if input.ctrl { 0.25 } else { 1.0 };
        rg.eye += v.normalize_or_zero() * rg.speed * boost * dt;
        rg.tick(dt);
        let frame = match st.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                st.surface.configure(&rg.device, &st.config);
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        rg.render(&view, st.config.format, st.config.width, st.config.height, input.cursor);
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
                if let Some(st) = &self.state {
                    st.range.save_state();
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
                        Drag::ShotHandle => rg.drag_shot_handle(c),
                        Drag::Clip(a) => {
                            if let Some(b) = rg.track_hit([c[0], TL_Y]) {
                                rg.clip = (a != b).then(|| (a.min(b) as u32, a.max(b) as u32));
                            }
                        }
                        Drag::Track => {
                            if let Some(i) = rg.track_hit([c[0], TL_Y]) {
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
                } else if c[0] <= rg.side() {
                    rg.wheel_ui(c, dy, fine, coarse);
                } else {
                    rg.speed = (rg.speed * 1.2f32.powf(dy)).clamp(0.05, 200.0);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                match button {
                    MouseButton::Right if pressed && self.state.as_ref().is_some_and(|st| self.input.cursor[0] <= st.range.side()
                        && st.range.track_hit(self.input.cursor).is_none()) => {
                        let c = self.input.cursor;
                        if let Some(st) = self.state.as_mut() {
                            if let Some(Line::Row(knob)) = st.range.line_at(c).filter(|l| matches!(l, Line::Row(k) if kind(*k) == Kind::Dial)) {
                                st.range.reset_dial(knob);
                            }
                        }
                    }
                    MouseButton::Right if pressed && self.input.alt => {
                        let c = self.input.cursor;
                        if let Some(st) = self.state.as_ref() {
                            self.input.drag = Some((Drag::Dolly(st.range.pivot_at(c)), c));
                        }
                    }
                    MouseButton::Middle if pressed => {
                        let c = self.input.cursor;
                        if let Some(st) = self.state.as_ref() {
                            self.input.drag = Some((Drag::Pan(st.range.pivot_at(c)), c));
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
                        self.input.look = pressed;
                        if let Some(st) = self.state.as_ref() {
                            if pressed {
                                let _ = st.window.set_cursor_grab(CursorGrabMode::Locked)
                                    .or_else(|_| st.window.set_cursor_grab(CursorGrabMode::Confined));
                                st.window.set_cursor_visible(false);
                            } else {
                                let _ = st.window.set_cursor_grab(CursorGrabMode::None);
                                st.window.set_cursor_visible(true);
                            }
                        }
                    }
                    MouseButton::Left if pressed => {
                        let (c, shift, ctrl, alt) = (self.input.cursor, self.input.shift, self.input.ctrl, self.input.alt);
                        let Some(st) = self.state.as_mut() else { return };
                        let rg = &mut st.range;
                        if let Some(i) = rg.track_hit(c) {
                            rg.mode = Mode::Scrub(i);
                            rg.playing = false;
                            self.input.drag = Some((Drag::Track, c));
                        } else if rg.shot_handle_hit(c) {
                            self.input.drag = Some((Drag::ShotHandle, c));
                        } else if let Some(step) = rg.handle_hit(c).filter(|_| rg.mode == Mode::Live) {
                            if step == (0, 0) {
                                self.input.drag = Some((Drag::Rotate([0.0, 0.0]), c));
                            } else {
                                rg.rotate_section(step.0, step.1);
                            }
                        } else if c[0] <= rg.side() {
                            if let Click::Dial(knob) = rg.click_ui(c, ctrl, shift) {
                                self.input.drag = Some((Drag::Dial(knob, rg.dial_t(knob), c[1]), c));
                            }
                        } else if let Some(axis) = rg.axis_hit(c) {
                            rg.snap_view(axis);
                        } else if alt {
                            self.input.drag = Some((Drag::Orbit(rg.pivot_at(c)), c));
                        } else if shift {
                            self.input.drag = Some((Drag::Turn, c));
                        } else {
                            rg.pick(c, ctrl);
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
            if self.input.look {
                self.input.mouse += Vec2::new(dx as f32, dy as f32);
                self.active = Some(std::time::Instant::now());
            }
        }
    }
}

fn prepare_environment() {
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
    prepare_environment();
    anyhow::ensure!(graphics::raymarch_backend_enabled(), "fea-range requires KESSEL_RENDERER=raymarch");
    if let Some(prefix) = shots {
        return headless_shots(prefix);
    }
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { state: None, input: Input::default(), last: None, active: None, escape: None, error: None, saved: None };
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
    // 1. Live, the default shot (57 mm at the loader through the right turret side), help shown.
    rg.help = true;
    rg.collapsed = [false, false, true, false, false, false, true];
    let p1 = still(&mut rg, "live", true)?;
    rg.help = false;
    rg.collapsed = [false, false, false, false, true, true, true];
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
    println!("[fea-range] run: {} frames in {run_s:.1} s; {}", rg.frames.len(), rg.last_chain);
    checks.push(serde_json::json!({"check": "run played live, its previews kept", "pass": rg.frames.len() as u32 == total,
        "frames": rg.frames.len(), "wall_s": run_s}));
    rg.mode = Mode::Scrub(probe as usize);
    let p3 = still(&mut rg, "timeline", false)?;
    // 4. Back to that frame (let go of the timeline): the run repeated step for step, and its state
    // against the first run's there.
    rg.seek(probe);
    let live_digest = rg.digest();
    let agree = recorded_digest.as_deref() == Some(live_digest.as_str());
    println!("[fea-range] {} the re-run agrees with the first run at frame {probe}", if agree { "PASS" } else { "FAIL" });
    checks.push(serde_json::json!({"check": "a backward seek agrees with the first run", "pass": agree, "frame": probe,
        "recorded": recorded_digest, "live": live_digest}));
    // On to the end and back again: whether the re-run repeats itself.
    rg.seek(total - 1);
    rg.seek(probe);
    let again = rg.digest();
    let repeat = again == live_digest;
    println!("[fea-range] {} a second re-run agrees with the first re-run", if repeat { "PASS" } else { "FAIL" });
    checks.push(serde_json::json!({"check": "a backward seek repeats itself", "pass": repeat, "again": again}));
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
        let game = std::fs::read_to_string("src/game.rs").unwrap();
        assert!(game.contains("let practical_gain = power * game_fea_range::review_lamp_gain();"));
    }
}
