//! The expedition's physical field guide. Diagnosis belongs to the reader: this module never
//! receives vehicle faults, health, or repair progress. Its diagrams explain the game's current
//! track-repair interaction; they are not a historical workshop-manual facsimile.

use crate::{
    camera::Camera,
    gltf_load::{CpuImage, CpuMesh, Vertex},
    renderer::Renderer,
    ui::Align,
};
use glam::{Mat4, Vec2, Vec3};
use std::hash::{Hash, Hasher};

pub const SPREAD_COUNT: u8 = 16;
pub const PART_COUNT: usize = 5;
// A Fibel: A5 landscape pages (210 x 148 mm) bound on the short edge, the open book 0.43 m
// wide (docs/REPAIR_MANUAL_REDESIGN_20261102.md). The covers overhang the pages by 4 mm.
const WIDTH: f32 = 0.432;
const HEIGHT: f32 = 0.158;
const PAGE_WIDTH: f32 = 0.210;
const PAGE_HEIGHT: f32 = 0.148;
const PAGE_Z: f32 = 0.013;
pub const TURN_SECONDS: f32 = 0.48;
const MAX_ZOOM: f32 = 2.6;
/// Body type: 0.028 of the page height is 4.1 mm on the paper (the Fibel sets about 3.5 mm).
const BODY_TEXT_SIZE: f32 = 0.028;
const BODY_LINE_HEIGHT: f32 = 0.034;
/// Characters per column: beside a plate, or in each of two columns.
const COLUMN_WRAP_BESIDE_PLATE: usize = 36;
const COLUMN_WRAP: usize = 40;
const INK: [f32; 4] = [0.08, 0.08, 0.08, 1.];
/// The Fibel's spot colour, one warm red, and its cream paper.
pub const RED: [f32; 4] = [0.78, 0.17, 0.10, 1.];
pub const PAPER: [f32; 4] = [0.90, 0.87, 0.76, 1.];
const FADED: [f32; 4] = [0.40, 0.37, 0.32, 1.];
const PRINT_STROKE: f32 = 0.0022;
thread_local! {
    /// The schematics' pen for the page being painted: the print stroke on the 2048-wide page,
    /// one pixel on the pixel book's page (`draw_page_pixel`).
    static STROKE: std::cell::Cell<f32> = const { std::cell::Cell::new(PRINT_STROKE) };
}
fn stroke() -> f32 {
    STROKE.with(|s| s.get())
}
/// The plates the pages name, baked from the owner's Tiger CAD by tools/bake_manual_plates.py
/// into `assets/manual/plates/<name>.png` under the asset root (not committed).
pub const PLATE_NAMES: [&str; 7] = ["engine_iso", "engine_fan_drive_red", "drivetrain_iso", "transmission_red", "fuel_red", "running_gear_side", "drivetrain_exploded"];
/// The widths the pixel plates are baked at (`tools/bake_manual_plates_px.py`).
pub const PLATE_PIXEL_WIDTHS: [u32; 6] = [96, 128, 160, 192, 224, 256];

/// The top-down reading overlay (2 November 2026, the owner's pick over the physical book at
/// the crush resolution): the open book drawn flat at display resolution over the crushed
/// frame, its retained page ink as images, chapter tabs down the fore-edge, and a leaf that
/// narrows to the spine and widens on the other side when a page turns. `KESSEL_MANUAL_3D=1`
/// keeps the physical book in the scene instead (the loupe belongs to that book).
pub fn overlay_mode() -> bool {
    static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| !std::env::var_os("KESSEL_MANUAL_3D").is_some_and(|v| v == "1"))
}
/// The chapter tabs down the fore-edge: the label and the spread each opens.
pub const CHAPTER_TABS: [(&str, u8); 6] = [("BOOK", 0), ("TRACK", 1), ("PARTS", 8), ("CREW", 12), ("WINCH", 14), ("FIRE", 15)];
/// The pixel book: the open spread spans this share of the internal grid's width at zoom 1 and
/// at most this share of its height; the page is this much wider than tall (the owner, 2
/// November 2026: "a bit taller", less empty paper than the A5 landscape's 1.42).
pub const OVERLAY_SPREAD_SHARE: f32 = 0.80;
pub const OVERLAY_MAX_HEIGHT_SHARE: f32 = 0.86;
pub const PIXEL_PAGE_ASPECT: f32 = 1.25;

/// The page texture of the pixel book: exactly the internal pixels one page covers at zoom 1.
pub fn overlay_page_pixels(grid: (u32, u32)) -> (u32, u32) {
    let (iw, ih) = (grid.0.max(64) as f32, grid.1.max(64) as f32);
    let mut pw = (OVERLAY_SPREAD_SHARE * iw * 0.5).round();
    let mut ph = (pw / PIXEL_PAGE_ASPECT).round();
    if ph > (OVERLAY_MAX_HEIGHT_SHARE * ih).floor() {
        ph = (OVERLAY_MAX_HEIGHT_SHARE * ih).floor();
        pw = (ph * PIXEL_PAGE_ASPECT).round();
    }
    (pw.max(64.0) as u32, ph.max(48.0) as u32)
}

/// Where the overlay book lies: in internal pixels (`px`, origin top-left) and the same as
/// normalized rectangles of the frame (the display's pointer is normalized the same way, the
/// blit keeping the whole grid).
#[derive(Clone, Copy, Debug)]
pub struct OverlayLayout {
    /// The spread, both pages, in internal pixels: x, y, w, h.
    pub px: [i32; 4],
    /// The pixel unit of the trim: 1 on a 360-line grid, 2 on 576.
    pub unit: i32,
    /// The zoom level drawn: 1, or 2 pixels per texel.
    pub zoom: i32,
    /// The spread, normalized: x, y, w, h.
    pub spread: [f32; 4],
    pub page_w: f32,
    pub page_h: f32,
    /// One tab per chapter, normalized x, y, w, h, down the right fore-edge.
    pub tabs: [[f32; 4]; CHAPTER_TABS.len()],
    /// The same tabs in internal pixels.
    pub tabs_px: [[i32; 4]; CHAPTER_TABS.len()],
}

pub fn overlay_layout(grid: (u32, u32), state: &ManualState) -> OverlayLayout {
    let (iw, ih) = (grid.0.max(64) as i32, grid.1.max(64) as i32);
    let (pw, ph) = overlay_page_pixels(grid);
    let (pw, ph) = (pw as i32, ph as i32);
    let zoom = if state.zoom >= 1.5 { 2 } else { 1 };
    let (w, h) = (2 * pw * zoom, ph * zoom);
    // Zoom about the focus: the focused point of the spread stays where it was at zoom 1.
    let (x1, y1) = ((iw - 2 * pw) / 2, (ih - ph) / 2);
    let x = x1 + (state.focus.x * (2 * pw) as f32 * (1 - zoom) as f32).round() as i32;
    let y = y1 + (state.focus.y * ph as f32 * (1 - zoom) as f32).round() as i32;
    let unit = ((ih as f32 / 360.0).round() as i32).max(1);
    let (tw, th, gap) = (46 * unit, 14 * unit, 3 * unit);
    // The book and its tabs are centred together, so the tabs stay on the frame.
    let x = x - (tw + 6 * unit) / 2;
    let mut tabs_px = [[0; 4]; CHAPTER_TABS.len()];
    for (i, tab) in tabs_px.iter_mut().enumerate() {
        *tab = [x + w, y + 10 * unit + i as i32 * (th + gap), tw, th];
    }
    let norm = |r: [i32; 4]| [r[0] as f32 / iw as f32, r[1] as f32 / ih as f32, r[2] as f32 / iw as f32, r[3] as f32 / ih as f32];
    let spread = norm([x, y, w, h]);
    OverlayLayout {
        px: [x, y, w, h], unit, zoom, spread, page_w: spread[2] * 0.5, page_h: spread[3],
        tabs: tabs_px.map(norm), tabs_px,
    }
}

/// What a pointer on the overlay meets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverlayHit {
    /// A point of the spread, as the uv `click` reads (x across both pages).
    Page(Vec2),
    /// A chapter tab: the spread it opens.
    Tab(u8),
}

pub fn overlay_hit(grid: (u32, u32), state: &ManualState, pointer: Vec2) -> Option<OverlayHit> {
    if !state.held || !pointer.is_finite() {
        return None;
    }
    let lay = overlay_layout(grid, state);
    for ((_, target), [tx, ty, tw, th]) in CHAPTER_TABS.iter().zip(lay.tabs) {
        if pointer.x >= tx && pointer.x <= tx + tw && pointer.y >= ty && pointer.y <= ty + th {
            return Some(OverlayHit::Tab(*target));
        }
    }
    overlay_uv(grid, state, pointer).map(OverlayHit::Page)
}

/// The spread uv under a display pointer, when it is on the pages.
pub fn overlay_uv(grid: (u32, u32), state: &ManualState, pointer: Vec2) -> Option<Vec2> {
    if !state.held || !pointer.is_finite() {
        return None;
    }
    let [x, y, w, h] = overlay_layout(grid, state).spread;
    let uv = Vec2::new((pointer.x - x) / w, (pointer.y - y) / h);
    (uv.min_element() >= 0.0 && uv.max_element() <= 1.0).then_some(uv)
}

/// The page point under the pointer in whichever presentation is live: the overlay's pages or
/// the physical book's (`book_uv`).
pub fn pointer_uv(camera: &Camera, grid: (u32, u32), pointer: Vec2, state: &ManualState) -> Option<Vec2> {
    if overlay_mode() {
        overlay_uv(grid, state, pointer)
    } else {
        book_uv(camera, pointer, state)
    }
}

/// The index of the chapter (its tab) a spread belongs to.
pub fn chapter_of(spread: u8) -> usize {
    CHAPTER_TABS.iter().rposition(|(_, first)| *first <= spread).unwrap_or(0)
}

/// Draw the open book flat over the crushed frame on its own pixel grid: the boards two units
/// wide, the pages one texel per pixel (two at zoom), hard spine lines, a hard offset shadow,
/// the turning leaf, the ribbon and the chapter tabs in the pixel face. `light` scales the
/// paper: the overlay is not lit by the scene, so this is a named reading-light scalar
/// (docs/PHYSICAL_TRUTH_AUDIT.md). The ink is flushed before the blit (`request_pixel_ink`).
pub fn draw_manual_overlay(r: &mut Renderer, state: &ManualState, light: f32) {
    let Some(images) = r.manual_page_images() else { return };
    if !state.held {
        return;
    }
    let grid = (r.iw, r.ih);
    let (iw, ih) = (grid.0.max(64) as f32, grid.1.max(64) as f32);
    let light = light.clamp(0.0, 1.0);
    let lit = |c: [f32; 3], a: f32| [c[0] * light, c[1] * light, c[2] * light, a];
    let lay = overlay_layout(grid, state);
    let [x, y, w, h] = lay.px;
    let u = lay.unit;
    let pw = w / 2;
    let rect = |r: &mut Renderer, x: i32, y: i32, w: i32, h: i32, c: [f32; 4]| {
        r.fill_rect(x as f32 / iw, y as f32 / ih, w as f32 / iw, h as f32 / ih, c);
    };
    let image = |r: &mut Renderer, handle: u32, x: i32, y: i32, w: i32, h: i32, c: [f32; 4]| {
        r.draw_image(handle, x as f32 / iw, y as f32 / ih, w as f32 / iw, h as f32 / ih, c);
    };
    let board = [0.17, 0.20, 0.15];
    // The hard shadow on the desk, the boards two units round the pages.
    rect(r, x - 2 * u + 3 * u, y - 2 * u + 3 * u, w + 4 * u, h + 4 * u, [0.0, 0.0, 0.0, 0.5]);
    rect(r, x - 2 * u, y - 2 * u, w + 4 * u, h + 4 * u, lit(board, 1.0));
    image(r, images[0], x, y, pw, h, lit([1.0, 1.0, 1.0], 1.0));
    image(r, images[1], x + pw, y, pw, h, lit([1.0, 1.0, 1.0], 1.0));
    // The spine: two hard lines, no gradient.
    rect(r, x + pw - u, y, 2 * u, h, [0.0, 0.0, 0.0, 0.45]);
    rect(r, x + pw - 3 * u, y, 2 * u, h, [0.0, 0.0, 0.0, 0.14]);
    rect(r, x + pw + u, y, 2 * u, h, [0.0, 0.0, 0.0, 0.14]);
    if let Some((over_right, width)) = state.overlay_leaf() {
        // The leaf hinged at the spine: its width the cosine of its angle, a hard shadow beside
        // its free edge as wide as the sine.
        let lw = (pw as f32 * width).round() as i32;
        let s = (1.0 - width * width).max(0.0).sqrt();
        let sh = (pw as f32 * 0.06 * s).round() as i32;
        let shade = 0.55 + 0.45 * width;
        if over_right {
            rect(r, x + pw + lw, y, sh, h, [0.0, 0.0, 0.0, 0.4]);
            image(r, images[2], x + pw, y, lw.max(1), h, lit([shade, shade, shade], 1.0));
        } else {
            rect(r, x + pw - lw - sh, y, sh, h, [0.0, 0.0, 0.0, 0.4]);
            image(r, images[3], x + pw - lw, y, lw.max(1), h, lit([shade, shade, shade], 1.0));
        }
    }
    if state.bookmark == Some(state.spread) {
        // The ribbon's tail out of the foot of the marked spread.
        rect(r, x + pw / 2 - 2 * u, y + h - 6 * u, 4 * u, 22 * u, lit([RED[0], RED[1], RED[2]], 1.0));
    }
    let chapter = chapter_of(state.spread);
    for (i, ((label, _), [tx, ty, tw, th])) in CHAPTER_TABS.iter().zip(lay.tabs_px).enumerate() {
        let active = i == chapter;
        let tw = if active { tw + 6 * u } else { tw };
        rect(r, tx + 2 * u, ty + 2 * u, tw, th, [0.0, 0.0, 0.0, 0.4]);
        rect(r, tx, ty, tw, th, lit(if active { [0.90, 0.87, 0.76] } else { [0.78, 0.74, 0.62] }, 1.0));
        rect(r, tx, ty, u, th, [0.0, 0.0, 0.0, 0.2]);
        let ink = if active { RED } else { INK };
        let label_w = crate::pixel_font::width_px(label, u as u32) as i32;
        crate::pixel_font::draw_at(r, label, tx + (tw - label_w) / 2, ty + (th - 7 * u) / 2, u as u32, lit([ink[0], ink[1], ink[2]], 1.0), grid);
    }
    r.request_pixel_ink();
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
struct PageTurn {
    from: u8,
    to: u8,
    elapsed: f32,
}

#[derive(Clone, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ManualState {
    pub held: bool,
    spread: u8,
    bookmark: Option<u8>,
    turn: Option<PageTurn>,
    zoom: f32,
    focus: Vec2,
    /// The plates found under the asset root: name -> (image handle, width, height in texels).
    #[serde(skip)]
    plates: std::collections::HashMap<&'static str, (u32, u32, u32)>,
    /// The pixel bake's width the plates were loaded at (0: the 1600-wide print plates).
    #[serde(skip)]
    plates_px: u32,
    #[serde(skip)]
    plates_loaded: bool,
    /// The right button held over the open book: the loupe is in the hand.
    #[serde(skip)]
    pub loupe_wanted: bool,
    /// Where the loupe stands this frame, when it does (set by `prepare_repair_manual`).
    #[serde(skip)]
    pub loupe: Option<LoupeView>,
}

/// A reading glass over the page: a ring of `LOUPE_DIAMETER` held `LOUPE_HEIGHT` above the
/// paper under the pointer, and the camera that looks through it (the lens pass of the crush's
/// mask mode 5): at the lens, down the page normal, with a field such that the
/// `LOUPE_DIAMETER / LOUPE_POWER` of paper under the glass spans the lens's own circle on the
/// screen at the frame's pixel density, so the composite reads the pass texel for texel and
/// the glass shows the paper `LOUPE_POWER` times larger than the eye sees it beside the glass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoupeView {
    pub eye: Vec3,
    pub forward: Vec3,
    /// The lens pass's vertical field (wide: the patch under the glass fills only the circle).
    pub fov_y: f32,
    /// The lens on the screen: centre uv and radius in height units.
    pub centre_uv: Vec2,
    pub radius_uv: f32,
    /// How much of the pass the circle reads per screen unit: 1 when the field could be made
    /// wide enough, more when the lens is small on the screen (the field is capped at 120 deg).
    pub sample_scale: f32,
    pub lens_model: Mat4,
}
const LOUPE_PASS_MAX_FOV: f32 = 120.0 * std::f32::consts::PI / 180.0;
pub const LOUPE_DIAMETER: f32 = 0.064;
pub const LOUPE_HEIGHT: f32 = 0.055;
pub const LOUPE_POWER: f32 = 3.0;

/// The lens over the page under `pointer`, and the camera through it, or none when the
/// pointer is off the book.
pub fn loupe_view(camera: &Camera, pointer: Vec2, state: &ManualState) -> Option<LoupeView> {
    if !state.held || state.turning() || !pointer.is_finite() {
        return None;
    }
    let base = models(camera, state)[0];
    // The lens is dragged, so the pointer's ray meets the lens's own plane, LOUPE_HEIGHT over
    // the open spread; the paper under the lens is straight below it along the page normal.
    let inv = camera.view_proj().inverse();
    let unproject = |z: f32| {
        let p = inv * glam::Vec4::new(pointer.x * 2. - 1., 1. - pointer.y * 2., z, 1.);
        p.truncate() / p.w
    };
    let (start, end) = (unproject(0.), unproject(1.));
    let inverse_base = base.inverse();
    let local_start = inverse_base.transform_point3(start);
    let direction = inverse_base.transform_vector3(end - start);
    if direction.z.abs() < 1e-6 {
        return None;
    }
    let t = (PAGE_Z + LOUPE_HEIGHT - local_start.z) / direction.z;
    if t < 0. || !t.is_finite() {
        return None;
    }
    let lens_local = local_start + direction * t;
    let uv = Vec2::new(lens_local.x / (PAGE_WIDTH * 2.) + 0.5, 0.5 - lens_local.y / PAGE_HEIGHT);
    if uv.min_element() < 0. || uv.max_element() > 1. {
        return None;
    }
    let eye = base.transform_point3(lens_local);
    let normal = base.transform_vector3(Vec3::Z).normalize_or_zero();
    // The lens on the screen: its centre projected, its radius from its distance to the eye.
    let clip = camera.view_proj() * eye.extend(1.0);
    if clip.w <= 1e-4 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    let centre_uv = Vec2::new(0.5 + ndc.x * 0.5, 0.5 - ndc.y * 0.5);
    let distance = (eye - camera.pos).dot(camera.forward()).max(0.02);
    let radius_uv = ((LOUPE_DIAMETER * 0.5 / distance) / (2.0 * (camera.fov_y * 0.5).tan())).max(1e-3);
    // The paper under the glass: a patch LOUPE_DIAMETER / LOUPE_POWER across, LOUPE_HEIGHT away.
    // It must fill 2 * radius_uv of the pass's frame height, so the pass is read texel for
    // texel; a lens small on the screen would need a field beyond 120 degrees, and is then read
    // at a scale instead.
    let tan_half_patch = (LOUPE_DIAMETER / LOUPE_POWER * 0.5) / LOUPE_HEIGHT;
    let wanted_tan_half = tan_half_patch / (2.0 * radius_uv);
    let tan_half = wanted_tan_half.min((LOUPE_PASS_MAX_FOV * 0.5).tan());
    let fov_y = 2.0 * tan_half.atan();
    let sample_scale = wanted_tan_half / tan_half;
    let lens_model = Mat4::from_translation(eye) * Mat4::from_cols(base.x_axis, base.y_axis, base.z_axis, glam::Vec4::W);
    Some(LoupeView { eye, forward: -normal, fov_y, centre_uv, radius_uv, sample_scale, lens_model })
}

/// The glass: a ring of 32 segments round the lens, its handle toward the reader's right hand.
/// Authored at real scale; the lens itself is the crush's composite, not a mesh.
pub fn loupe_mesh() -> CpuMesh {
    let (mut v, mut i) = (Vec::new(), Vec::new());
    let ring = LOUPE_DIAMETER * 0.5;
    let tube = 0.0035;
    for n in 0..32 {
        let a0 = n as f32 * std::f32::consts::TAU / 32.;
        let a1 = (n + 1) as f32 * std::f32::consts::TAU / 32.;
        let (c0, c1) = (Vec3::new(a0.cos(), a0.sin(), 0.) * ring, Vec3::new(a1.cos(), a1.sin(), 0.) * ring);
        let out0 = Vec3::new(a0.cos(), a0.sin(), 0.) * tube;
        let out1 = Vec3::new(a1.cos(), a1.sin(), 0.) * tube;
        let up = Vec3::Z * tube;
        quad(&mut v, &mut i, [c0 - out0 - up, c1 - out1 - up, c1 + out1 - up, c0 + out0 - up], [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]);
        quad(&mut v, &mut i, [c0 + out0 + up, c1 + out1 + up, c1 - out1 + up, c0 - out0 + up], [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]);
        quad(&mut v, &mut i, [c0 + out0 - up, c1 + out1 - up, c1 + out1 + up, c0 + out0 + up], [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]);
        quad(&mut v, &mut i, [c0 - out0 + up, c1 - out1 + up, c1 - out1 - up, c0 - out0 - up], [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]);
    }
    solid_box(&mut v, &mut i, Vec3::new(ring - 0.002, -0.006, -0.004), Vec3::new(ring + 0.075, 0.006, 0.004));
    build(v, i, [46, 42, 36, 255])
}

impl Default for ManualState {
    fn default() -> Self {
        Self {
            held: false,
            spread: 0,
            bookmark: None,
            turn: None,
            zoom: 1.0,
            focus: Vec2::splat(0.5),
            plates: std::collections::HashMap::new(),
            plates_loaded: false,
            plates_px: 0,
            loupe_wanted: false,
            loupe: None,
        }
    }
}

impl ManualState {
    /// Load the plates once, when the book is first opened: a missing plate leaves its page on
    /// the schematic and is named in the log; a present plate is uploaded as an ink image. With
    /// `pixel_width` the pixel bakes (`tools/bake_manual_plates_px.py`, the widest at most that
    /// wide) are loaded, nearest-sampled; without, the 1600-wide print plates.
    pub fn load_plates(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, pixel_width: Option<u32>) {
        let px = pixel_width.map(|w| PLATE_PIXEL_WIDTHS.iter().copied().filter(|&p| p <= w).max().unwrap_or(PLATE_PIXEL_WIDTHS[0])).unwrap_or(0);
        if self.plates_loaded && self.plates_px == px {
            return;
        }
        self.plates_loaded = true;
        self.plates_px = px;
        self.plates.clear();
        let mut found = Vec::new();
        for name in PLATE_NAMES {
            let path = if px > 0 { format!("assets/manual/plates/px{px}/{name}.png") } else { format!("assets/manual/plates/{name}.png") };
            let Ok(bytes) = crate::resource::read(&path) else { continue };
            match image::load_from_memory(&bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let handle = if px > 0 { renderer.upload_image_nearest(device, queue, rgba.as_raw(), w, h) } else { renderer.upload_image(device, queue, rgba.as_raw(), w, h) };
                    self.plates.insert(name, (handle, w, h));
                    found.push(name);
                }
                Err(error) => eprintln!("[manual] plate {path} unreadable: {error}"),
            }
        }
        println!("[manual] plates{}: {} of {} present ({})", if px > 0 { format!(" at {px} px") } else { String::new() }, found.len(), PLATE_NAMES.len(), found.join(", "));
    }
    /// A plate's image handle and its size in texels.
    pub fn plate(&self, name: &str) -> Option<(u32, u32, u32)> {
        self.plates.get(name).copied()
    }
    pub fn spread(&self) -> u8 {
        self.spread
    }
    pub fn bookmark(&self) -> Option<u8> {
        self.bookmark
    }
    pub fn turning(&self) -> bool {
        self.turn.is_some()
    }
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Camera-independent identity of the visible ink and leaf pose. Used only to reject stale
    /// indirect lighting when pages change; it contains no damage or diagnostic information.
    pub(crate) fn presentation_key(&self) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.page_bindings().hash(&mut hash);
        self.bookmark.hash(&mut hash);
        self.zoom.to_bits().hash(&mut hash);
        self.focus.x.to_bits().hash(&mut hash);
        self.focus.y.to_bits().hash(&mut hash);
        self.turning().hash(&mut hash);
        self.leaf_angle().to_bits().hash(&mut hash);
        hash.finish()
    }

    /// Restore only durable reading state. A closed game does not reopen a book or replay a turn.
    pub fn restore_state(&mut self, spread: u8, bookmark: Option<u8>) -> anyhow::Result<()> {
        anyhow::ensure!(spread < SPREAD_COUNT, "manual spread out of range");
        anyhow::ensure!(
            bookmark.is_none_or(|p| p < SPREAD_COUNT),
            "manual bookmark out of range"
        );
        self.spread = spread;
        self.bookmark = bookmark;
        self.turn = None;
        self.held = false;
        Ok(())
    }

    pub fn set_held(&mut self, held: bool) {
        self.held = held;
        if !held {
            // Closing midway completes the deliberate page choice; reopening resumes there.
            if let Some(turn) = self.turn.take() {
                self.spread = turn.to;
            }
        }
    }

    fn go_to(&mut self, target: u8) -> bool {
        if !self.held || self.turn.is_some() || target >= SPREAD_COUNT || target == self.spread {
            return false;
        }
        self.turn = Some(PageTurn {
            from: self.spread,
            to: target,
            elapsed: 0.0,
        });
        // Bring a closely inspected page back to arm's length before turning it. At maximum
        // reading zoom a full-width physical leaf would otherwise swing through the eye.
        self.zoom = 1.0;
        self.focus = Vec2::splat(0.5);
        true
    }

    pub fn turn(&mut self, direction: i8) -> bool {
        let target = (i16::from(self.spread) + i16::from(direction.signum()))
            .clamp(0, i16::from(SPREAD_COUNT - 1)) as u8;
        self.go_to(target)
    }

    /// A chapter tab: the leaves turn to that spread.
    pub fn open_spread(&mut self, spread: u8) -> bool {
        self.go_to(spread)
    }

    /// The turning leaf as the overlay draws it: over the right page (else the left) and its
    /// width as a fraction of a page, the cosine of its angle.
    pub fn overlay_leaf(&self) -> Option<(bool, f32)> {
        self.turn?;
        let c = self.leaf_angle().cos();
        Some((c >= 0.0, c.abs()))
    }

    pub fn tick(&mut self, dt: f32) {
        if !self.held || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        if let Some(turn) = &mut self.turn {
            turn.elapsed += dt;
            if turn.elapsed >= TURN_SECONDS {
                self.spread = turn.to;
                self.turn = None;
            }
        }
    }

    pub fn zoom_by(&mut self, steps: f32) {
        if self.held && !self.turning() && steps.is_finite() {
            self.zoom = (self.zoom * 1.18_f32.powf(steps.clamp(-8.0, 8.0))).clamp(1.0, MAX_ZOOM);
        }
    }

    pub fn focus(&mut self, uv: Vec2) {
        if uv.is_finite() {
            self.focus = uv.clamp(Vec2::splat(0.05), Vec2::splat(0.95));
        }
    }

    /// Page-relative pointing is intersected with the real book by `book_uv`, not a screen panel.
    /// Page corners turn leaves. The top margin stores/recalls a ribbon; printed choices navigate.
    pub fn click(&mut self, uv: Vec2) -> bool {
        if !self.held
            || self.turn.is_some()
            || !uv.is_finite()
            || uv.min_element() < 0.0
            || uv.max_element() > 1.0
        {
            return false;
        }
        let side = u8::from(uv.x >= 0.5);
        let x = if side == 0 {
            uv.x * 2.0
        } else {
            (uv.x - 0.5) * 2.0
        };
        if uv.y < 0.07 {
            if x < 0.33 {
                return self.go_to(0);
            }
            if x > 0.67 {
                return self.bookmark.is_some_and(|p| self.go_to(p));
            }
            self.bookmark = if self.bookmark == Some(self.spread) {
                None
            } else {
                Some(self.spread)
            };
            return true;
        }
        if uv.y > 0.94 {
            return if x < 0.22 {
                self.turn(-1)
            } else if x > 0.78 {
                self.turn(1)
            } else {
                false
            };
        }
        let page = page_definition(self.spread * 2 + side);
        if !(0.06..=0.94).contains(&x) {
            return false;
        }
        for (index, choice) in page.choices.iter().enumerate() {
            let y = 0.81 + index as f32 * 0.065;
            if (y..=y + 0.052).contains(&uv.y) {
                return self.go_to(choice.target);
            }
        }
        false
    }

    /// Textures for the stationary left/right pages and the two faces of the turning leaf.
    pub fn page_bindings(&self) -> [u8; 4] {
        match self.turn {
            Some(t) if t.to > t.from => [t.from * 2, t.to * 2 + 1, t.from * 2 + 1, t.to * 2],
            Some(t) => [t.to * 2, t.from * 2 + 1, t.to * 2 + 1, t.from * 2],
            None => [
                self.spread * 2,
                self.spread * 2 + 1,
                self.spread * 2 + 1,
                self.spread * 2,
            ],
        }
    }

    pub fn visible_part(&self, part: usize) -> bool {
        self.held && part < PART_COUNT && (part < 3 || self.turn.is_some())
    }

    pub fn leaf_angle(&self) -> f32 {
        let Some(turn) = self.turn else {
            return 0.0;
        };
        let p = (turn.elapsed / TURN_SECONDS).clamp(0.0, 1.0);
        let smooth = p * p * (3.0 - 2.0 * p);
        -std::f32::consts::PI
            * if turn.to > turn.from {
                smooth
            } else {
                1.0 - smooth
            }
    }
}

#[derive(Clone, Copy)]
enum Drawing {
    Track,
    Gap,
    Link,
    Pin,
    Drive,
    Recovery,
    Patient,
    Winch,
    Stowage,
}
struct Choice {
    label: &'static str,
    target: u8,
}
struct Page {
    title: &'static str,
    /// The chapter, set in red in the margin as the Fibel labels its sections.
    margin: &'static str,
    /// The paragraphs, each led by a red word ("SO", "OTHERWISE", "MIND") or none.
    text: &'static [(&'static str, &'static str)],
    /// A plate from the works drawings, when the asset root has it; else the schematic.
    plate: Option<&'static str>,
    drawing: Drawing,
    choices: &'static [Choice],
}

fn page_definition(page: u8) -> Page {
    use Drawing::*;
    match page {
        0 => Page { title: "FIELD REPAIR MANUAL", margin: "THE BOOK", plate: None, drawing: Track,
            text: &[("", "TIGER / CREW EDITION"), ("FIRST", "Start with the symptom you can observe. Make one check at a time."), ("ALWAYS", "Keep this book dry. Pass it to the next crew. Record unfinished work before leaving the tank.")],
            choices: &[Choice { label: "TANK WILL NOT MOVE  >", target: 1 }, Choice { label: "VIEW / LOOSE PARTS  >", target: 8 }] },
        1 => Page { title: "BEFORE YOU OPEN UP", margin: "THE BOOK", plate: None, drawing: Recovery,
            text: &[("STOP", "Stop somewhere covered if the tank can still move. Watch the threat before leaving it."), ("THEN", "Listen, look, then test. Do not keep driving a visibly broken track."), ("OTHERWISE", "An unsafe repair position calls for recovery, not more exposure.")],
            choices: &[Choice { label: "WOUNDED / RECOVERY  >", target: 12 }, Choice { label: "BEGIN DIAGNOSIS  >", target: 1 }] },
        2 => Page { title: "1 / NO MOVEMENT", margin: "TRACK", plate: Some("drivetrain_iso"), drawing: Drive,
            text: &[("ASK", "IS THE ENGINE RUNNING?"), ("LISTEN", "Listen for the engine and check its response at the driver's controls."), ("MIND", "A missing driver also removes remote driving. Try the actual driver's station.")],
            choices: &[Choice { label: "YES - CHECK DRIVE  >", target: 2 }, Choice { label: "NO - POWER / RECOVERY  >", target: 6 }] },
        3 => Page { title: "MAKE A USEFUL CHECK", margin: "TRACK", plate: Some("transmission_red"), drawing: Drive,
            text: &[("STOP", "Stop the tank before inspecting outside."), ("SO", "An engine sound alone does not prove drive reaches the tracks."), ("NEVER", "Do not guess which part broke from the impact sound. Inspect both sides.")],
            choices: &[Choice { label: "ENGINE RESPONDS  >", target: 2 }, Choice { label: "RETURN TO SYMPTOMS  >", target: 0 }] },
        4 => Page { title: "2 / INSPECT TRACKS", margin: "TRACK", plate: Some("running_gear_side"), drawing: Gap,
            text: &[("ASK", "IS THERE A VISIBLE GAP OR BROKEN LINK?"), ("LOOK", "Look along each track, including its exposed connecting-pin end."), ("MIND", "Compare both sides. Keep clear of the track while another person works the controls.")],
            choices: &[Choice { label: "YES - PREPARE REPAIR  >", target: 3 }, Choice { label: "NO - OTHER DRIVE FAULT  >", target: 6 }] },
        5 => Page { title: "WHAT YOU ARE FINDING", margin: "TRACK", plate: Some("running_gear_side"), drawing: Gap,
            text: &[("WHY", "The track must form a continuous chain around the running gear."), ("SO", "A spare shoe replaces the missing link. Its connecting pin joins the chain."), ("NOT", "A damaged engine or transmission will not be fixed by fitting a track link.")],
            choices: &[Choice { label: "GAP FOUND  >", target: 3 }, Choice { label: "NO VISIBLE GAP  >", target: 6 }] },
        6 => Page { title: "3 / PREPARE", margin: "TRACK", plate: None, drawing: Link,
            text: &[("FIRST", "Stop and secure the tank. Find cover and account for the crew."), ("BRING", "Bring the track kit, a compatible spare link and its pin to the damaged side."), ("MIND", "Work from the exposed end of the link. An armor plate between you and the part blocks access.")],
            choices: &[Choice { label: "TOOLS AND ACCESS READY  >", target: 4 }, Choice { label: "CANNOT REACH IT SAFELY  >", target: 6 }] },
        7 => Page { title: "KNOW YOUR TOOLS", margin: "TRACK", plate: None, drawing: Pin,
            text: &[("TOOLS", "Use the track kit at the damaged track."), ("THEN", "Pull loose ends together at the pull point. Fit the spare in the gap."), ("NOT", "Hammer the connecting-pin end. Striking armor will not repair the track.")],
            choices: &[Choice { label: "FIT THE REPLACEMENT  >", target: 4 }, Choice { label: "CHECK THE GAP AGAIN  >", target: 2 }] },
        8 => Page { title: "4 / JOIN THE TRACK", margin: "TRACK", plate: None, drawing: Link,
            text: &[("FIRST", "Bring the loose track ends together using the exposed repair point."), ("THEN", "Position the spare shoe in the gap. Align its pin opening with the adjoining links."), ("ALWAYS", "Leave the tank stationary throughout the work.")],
            choices: &[Choice { label: "LINK IS ALIGNED  >", target: 5 }, Choice { label: "NEED TO RECHECK ACCESS  >", target: 3 }] },
        9 => Page { title: "WORK AT THE PART", margin: "TRACK", plate: None, drawing: Link,
            text: &[("HANDS", "Pull the ends together first. Carry a real spare shoe to the gap and hold LMB to seat it."), ("RELEASE", "Release LMB after each step. Put the part down with X only on a clear support."), ("THEN", "Then bring the toolbox to the exposed pin.")],
            choices: &[Choice { label: "CONTINUE TO PIN  >", target: 5 }, Choice { label: "RETURN TO PREPARATION  >", target: 3 }] },
        10 => Page { title: "5 / CONNECT THE PIN", margin: "TRACK", plate: None, drawing: Pin,
            text: &[("HAMMER", "Work the connecting pin at its exposed end with the hammer."), ("WATCH", "Each real contact advances the connection. Watch the fitted link, not a health bar."), ("THEN", "When the joint is complete, put the tool away and inspect the track.")],
            choices: &[Choice { label: "TRACK JOINED - TEST  >", target: 7 }, Choice { label: "LINK NOT ALIGNED  >", target: 4 }] },
        11 => Page { title: "AFTER INTERRUPTION", margin: "TRACK", plate: None, drawing: Gap,
            text: &[("AFTER", "After a stop, inspect the joint before resuming."), ("MIND", "A spare beside the tank does not mean the repair is done."), ("ANYONE", "Another person can finish it. The physical joint determines completion.")],
            choices: &[Choice { label: "CHECK COMPLETION  >", target: 7 }, Choice { label: "BACK TO PIN WORK  >", target: 5 }] },
        12 => Page { title: "POWER OR DRIVE FAULT", margin: "TRACK", plate: Some("drivetrain_exploded"), drawing: Drive,
            text: &[("ASK", "No engine response, intact tracks or still immobile? Check again."), ("CHECK", "Check controls and damage; track repairs fix no other system."), ("SO", "This chapter covers links only. Seek recovery for other or major faults.")],
            choices: &[Choice { label: "RECHECK SYMPTOMS  >", target: 0 }, Choice { label: "RECHECK TRACKS  >", target: 2 }] },
        13 => Page { title: "RECOVERY REQUIRED", margin: "TRACK", plate: None, drawing: Recovery,
            text: &[("LEAVE", "Leave the tank where it can be found. Mark its map position."), ("RECORD", "Record surviving crew, damage, tools left and threat direction."), ("MIND", "A tow or heavy lift needs suitable equipment; this book cannot do it.")],
            choices: &[Choice { label: "RETURN TO INDEX  >", target: 0 }, Choice { label: "TRACK REPAIR CHECKLIST  >", target: 7 }] },
        14 => Page { title: "6 / FUNCTION CHECK", margin: "TRACK", plate: Some("running_gear_side"), drawing: Track,
            text: &[("CLEAR", "Clear people and loose tools from the running gear."), ("TEST", "From cover, test movement gently at the driver's controls. Check that both tracks respond."), ("OTHERWISE", "If the tank still will not move, stop. Return to diagnosis rather than repeating the same repair.")],
            choices: &[Choice { label: "MOVEMENT RESTORED  >", target: 0 }, Choice { label: "STILL IMMOBILE  >", target: 6 }] },
        15 => Page { title: "TRACK CHAPTER INDEX", margin: "TRACK", plate: None, drawing: Track,
            text: &[("", "NO MOVEMENT - pages 3-4"), ("", "INSPECT BOTH TRACKS - 5-6"), ("", "PREPARE / TOOLS - 7-8"), ("", "FIT LINK - 9-10"), ("", "CONNECT PIN - 11-12"), ("", "OTHER FAULT / RECOVERY - 13-14")],
            choices: &[Choice { label: "START DIAGNOSIS  >", target: 1 }, Choice { label: "GO TO PREPARATION  >", target: 3 }] },
        16 => Page { title: "VIEW / LOOSE PARTS", margin: "PARTS", plate: None, drawing: Drive,
            text: &[("ASK", "IS ONE VIEW DARK OR DAMAGED?"), ("COMPARE", "Compare another viewing block. Check that the affected part is present and secured."), ("MIND", "A loose or damaged sight cannot be fixed by aiming harder. Inspect its mounting.")],
            choices: &[Choice { label: "YES - SERVICE THE PART  >", target: 9 }, Choice { label: "SPARES / WORK LIGHT  >", target: 10 }] },
        17 => Page { title: "CHECK BEFORE REMOVAL", margin: "PARTS", plate: None, drawing: Recovery,
            text: &[("STOP", "Stop the tank. Leave your hands free and put the toolbox down for lifting."), ("LOOK", "Look directly at the exposed retainer. Hold LMB to release it, then let go."), ("THEN", "Hold again to draw the part out. Its old damage stays with it.")],
            choices: &[Choice { label: "PART IS FREE  >", target: 9 }, Choice { label: "RETURN TO SYMPTOMS  >", target: 0 }] },
        18 => Page { title: "REPLACE / SECURE / TEST", margin: "PARTS", plate: None, drawing: Link,
            text: &[("CARRY", "Carry a compatible part to the empty mounting. Hold LMB to seat it; release."), ("SECURE", "Hold at the retainer to secure it; release. Hold once more to check the component."), ("MIND", "A check does not heal a damaged replacement.")],
            choices: &[Choice { label: "VIEW RESTORED  >", target: 0 }, Choice { label: "STILL BLOCKED  >", target: 8 }] },
        19 => Page { title: "KEEP THE OLD PART", margin: "PARTS", plate: None, drawing: Recovery,
            text: &[("X", "X lowers a part onto clear, level ground or a supported tank surface."), ("LMB", "If it will not fit, choose a wider spot. Point at the part and click LMB to lift it."), ("MIND", "Parts stay where you leave them. Remember the location.")],
            choices: &[Choice { label: "TRANSPORT / LIGHT  >", target: 11 }, Choice { label: "RETURN TO FITTING  >", target: 9 }] },
        20 => Page { title: "SPARE TRACK CARRIERS", margin: "PARTS", plate: None, drawing: Pin,
            text: &[("TOOLS", "Bring the toolbox to a turret spare's exposed retaining pin."), ("HOLD", "Hold LMB for four hammer blows. Release, put the toolbox down, then draw out the shoe."), ("THEN", "Carry it to the prepared break. A damaged shoe is no sound spare.")],
            choices: &[Choice { label: "JOIN THE TRACK  >", target: 4 }, Choice { label: "TRACK CHAPTER INDEX  >", target: 7 }] },
        21 => Page { title: "WORK LAMP", margin: "PARTS", plate: Some("engine_fan_drive_red"), drawing: Drive,
            text: &[("CARRY", "Carry a battery work lamp to the bracket on the turret wall. Seat, secure and check it."), ("CLICK", "Click the lamp body to switch it. Its battery is finite; switch off when finished."), ("MIND", "The beam cannot light a part behind an obstruction.")],
            choices: &[Choice { label: "FITTING SEQUENCE  >", target: 9 }, Choice { label: "TRANSPORT / PARTS  >", target: 11 }] },
        22 => Page { title: "TRANSPORTING SPARES", margin: "PARTS", plate: None, drawing: Recovery,
            text: &[("STOP", "Stop the halftrack. Carry the part to the rear loading contact."), ("BED", "The bed needs a clear bay that fits the part. Loading keeps the same object."), ("THEN", "Unload at the stopped rear bed, then carry it to the Tiger.")],
            choices: &[Choice { label: "FIT THE COMPONENT  >", target: 9 }, Choice { label: "RETURN TO INDEX  >", target: 0 }] },
        23 => Page { title: "IF WORK WILL NOT START", margin: "PARTS", plate: None, drawing: Recovery,
            text: &[("ASK", "STOPPED? HANDS CLEAR?"), ("CHECK", "Check reach and access to the retainer. Release LMB between steps."), ("KEEP", "Use a compatible part and tool. Keep the old component: fitting a replacement never refills supplies.")],
            choices: &[Choice { label: "CHECK PART / MOUNT  >", target: 8 }, Choice { label: "CREW / RECOVERY  >", target: 12 }] },
        24 => Page { title: "CREW NOT RESPONDING?", margin: "CREW", plate: None, drawing: Patient,
            text: &[("ASK", "IS THE PERSON STILL ALIVE?"), ("KIT", "Take the field kit from camp. LMB equips it; aim at the wounded person."), ("ACT", "If a procedure is offered, act before bleeding takes its toll.")],
            choices: &[Choice { label: "YES - CARE / EVACUATE  >", target: 13 }, Choice { label: "NO - RECOVER THE TANK  >", target: 14 }] },
        25 => Page { title: "FIELD KIT", margin: "CREW", plate: None, drawing: Patient,
            text: &[("CLICK", "Click the patient to begin. Follow the shown hand action: hold, wrap, turn or watch."), ("SUPPLIES", "Supplies are finite. X interrupts work and puts the bag down."), ("NEVER", "Treatment does not restore a dead person.")],
            choices: &[Choice { label: "TRANSPORT TO CARE  >", target: 13 }, Choice { label: "RETURN TO INDEX  >", target: 0 }] },
        26 => Page { title: "LOAD THE WOUNDED", margin: "CREW", plate: None, drawing: Patient,
            text: &[("HANDS", "Use empty hands to hold the person with LMB. Clear a real path out of the tank."), ("CARRIER", "Bring him to the stopped carrier's rear loading contact. Hold to load a free stretcher."), ("MIND", "Bleeding continues during travel.")],
            choices: &[Choice { label: "FIELD TREATMENT  >", target: 12 }, Choice { label: "TANK ALSO STRANDED  >", target: 14 }] },
        27 => Page { title: "AT THE MEDICAL TENT", margin: "CREW", plate: None, drawing: Patient,
            text: &[("STOP", "Stop and unload the same person. Bring him beside an empty tent bed."), ("BED", "Aim at the bed and click LMB to admit him. The medic needs time and supplies."), ("MIND", "Taking him away interrupts camp care.")],
            choices: &[Choice { label: "RETURN TO FIELD CARE  >", target: 12 }, Choice { label: "TANK RECOVERY  >", target: 14 }] },
        28 => Page { title: "TANK CANNOT MOVE?", margin: "WINCH", plate: None, drawing: Winch,
            text: &[("WINCH", "Bring the winch on the carrier. At the rear ramp, click LMB to lower it."), ("ANCHOR", "Push it onto level ground; click to set the anchor. Take the cable to the Tiger's tow point."), ("ROUTE", "Keep a clear pulling route.")],
            choices: &[Choice { label: "TRACK REPAIR FIRST  >", target: 1 }, Choice { label: "AFTER RECOVERY  >", target: 15 }] },
        29 => Page { title: "WINCH WILL NOT PULL?", margin: "WINCH", plate: None, drawing: Winch,
            text: &[("CHECK", "Check fuel, anchor and cable. Leave 4 to 25 metres between winch and Tiger."), ("HOLD", "Hold LMB at the winch to pull. A steep slope, obstruction or overloaded line can stop it."), ("RELEASE", "Release before re-rigging.")],
            choices: &[Choice { label: "SET UP AGAIN  >", target: 14 }, Choice { label: "AFTER RECOVERY  >", target: 15 }] },
        30 => Page { title: "AFTER A FIRE", margin: "FIRE", plate: Some("fuel_red"), drawing: Stowage,
            text: &[("ASK", "FIRE OUT? ALL ROUNDS COLD?"), ("HOLD", "Hold LMB on each of the four ammunition banks until checked."), ("REPAIR", "Repair damaged engine or fuel parts before using the starter. Keep surviving parts.")],
            choices: &[Choice { label: "PARTS / MOUNTS  >", target: 8 }, Choice { label: "RETURN TO INDEX  >", target: 0 }] },
        _ => Page { title: "RETURN TO SERVICE", margin: "SERVICE", plate: Some("engine_iso"), drawing: Drive,
            text: &[("CLOSE", "Close and secure the engine access. Add fuel, then use the driver's starter."), ("CRANK", "A completed crank restarts only a safe, serviceable machine."), ("TEST", "Test each control. Other damage and crew losses remain.")],
            choices: &[Choice { label: "STILL IMMOBILE  >", target: 1 }, Choice { label: "FIND REPLACEMENT PART  >", target: 8 }] },
    }
}

fn quad(v: &mut Vec<Vertex>, i: &mut Vec<u32>, p: [Vec3; 4], uv: [[f32; 2]; 4]) {
    let normal = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
    let tangent = (p[1] - p[0]).normalize();
    let start = v.len() as u32;
    for (point, tex) in p.into_iter().zip(uv) {
        v.push(Vertex {
            pos: point.to_array(),
            normal: normal.to_array(),
            uv: tex,
            tangent: [tangent.x, tangent.y, tangent.z, 1.0],
        });
    }
    i.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

fn solid_box(v: &mut Vec<Vertex>, i: &mut Vec<u32>, lo: Vec3, hi: Vec3) {
    let p = [
        Vec3::new(lo.x, lo.y, lo.z),
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(hi.x, hi.y, lo.z),
        Vec3::new(lo.x, hi.y, lo.z),
        Vec3::new(lo.x, lo.y, hi.z),
        Vec3::new(hi.x, lo.y, hi.z),
        Vec3::new(hi.x, hi.y, hi.z),
        Vec3::new(lo.x, hi.y, hi.z),
    ];
    for f in [
        [4, 5, 6, 7],
        [1, 0, 3, 2],
        [0, 4, 7, 3],
        [5, 1, 2, 6],
        [3, 7, 6, 2],
        [0, 1, 5, 4],
    ] {
        quad(
            v,
            i,
            f.map(|n| p[n]),
            [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
        );
    }
}

fn build(v: Vec<Vertex>, i: Vec<u32>, color: [u8; 4]) -> CpuMesh {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for vert in &v {
        let p = Vec3::from_array(vert.pos);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    CpuMesh::basic(v, i, Some(CpuImage::new(1, 1, color.to_vec())), lo, hi)
}

/// Bound cover/page blocks and four ink-bearing faces. Geometry is authored here at real scale.
pub fn mesh() -> [CpuMesh; PART_COUNT] {
    let (mut v, mut i) = (Vec::new(), Vec::new());
    solid_box(
        &mut v,
        &mut i,
        Vec3::new(-WIDTH / 2., -HEIGHT / 2., -0.004),
        Vec3::new(WIDTH / 2., HEIGHT / 2., 0.),
    );
    solid_box(
        &mut v,
        &mut i,
        Vec3::new(-0.006, -HEIGHT / 2., -0.006),
        Vec3::new(0.006, HEIGHT / 2., 0.01),
    );
    let cover = build(v, i, [58, 58, 54, 255]);
    let page = |left: bool, back: bool, block: bool| {
        let (mut v, mut i) = (Vec::new(), Vec::new());
        let (x0, x1) = if left {
            (-PAGE_WIDTH, -0.002)
        } else {
            (0.002, PAGE_WIDTH)
        };
        if block {
            solid_box(
                &mut v,
                &mut i,
                Vec3::new(x0, -PAGE_HEIGHT / 2., 0.001),
                Vec3::new(x1, PAGE_HEIGHT / 2., PAGE_Z - 0.0004),
            );
            // Page-block edges sample the blank margin, rather than wrapping printed words down
            // the cut paper edges. The actual reading face is appended below with full-page UVs.
            for vertex in &mut v {
                vertex.uv = [0.01, 0.5];
            }
        }
        let z = if block {
            PAGE_Z
        } else if back {
            -0.0003
        } else {
            0.0003
        };
        let p = [
            Vec3::new(x0, -PAGE_HEIGHT / 2., z),
            Vec3::new(x1, -PAGE_HEIGHT / 2., z),
            Vec3::new(x1, PAGE_HEIGHT / 2., z),
            Vec3::new(x0, PAGE_HEIGHT / 2., z),
        ];
        if back {
            quad(
                &mut v,
                &mut i,
                [p[1], p[0], p[3], p[2]],
                [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
            );
        } else {
            quad(&mut v, &mut i, p, [[0., 1.], [1., 1.], [1., 0.], [0., 0.]]);
        }
        build(v, i, [225, 225, 215, 255])
    };
    [
        cover,
        page(true, false, true),
        page(false, false, true),
        page(false, false, false),
        page(false, true, false),
    ]
}

pub fn reading_fov(camera: &Camera) -> f32 {
    camera.fov_y.max(
        2.0 * (30_f32.to_radians().tan() * (16.0 / 9.0 / camera.aspect.max(0.1)).max(1.0)).atan(),
    )
}

pub fn models(camera: &Camera, state: &ManualState) -> [Mat4; PART_COUNT] {
    let f = camera.forward();
    let r = camera.right();
    let u = r.cross(f).normalize_or_zero();
    let focus = Vec3::new(
        (state.focus.x - 0.5) * WIDTH,
        (0.5 - state.focus.y) * HEIGHT,
        0.0,
    );
    // Fit the whole resting spread, including the nearer lower cover corner. Raising it
    // alone clips the opposite top corner. Close reading keeps its intentional page crop.
    let resting = ((MAX_ZOOM - state.zoom) / (MAX_ZOOM - 1.)).clamp(0., 1.);
    let frame = Mat4::from_cols(
        r.extend(0.),
        u.extend(0.),
        (-f).extend(0.),
        (camera.pos
            + f * (0.31 / state.zoom + 0.015 * resting)
            + u * (-0.008 / state.zoom + 0.030 * resting))
            .extend(1.),
    );
    let base = frame
        // The loader's actual overhead fixture is to the side. Turn the book toward it as
        // well as upward; pitching alone left the main lamp behind the printed surface.
        * Mat4::from_rotation_y(30_f32.to_radians())
        * Mat4::from_rotation_x(-25_f32.to_radians())
        * Mat4::from_translation(-focus * (state.zoom - 1.).clamp(0., 1.));
    let leaf = base
        * Mat4::from_translation(Vec3::new(0., 0., PAGE_Z + 0.001))
        * Mat4::from_rotation_y(state.leaf_angle());
    let mut models = [base, base, base, leaf, leaf];
    // A focused page must not pull the other cover or a swinging leaf through the eye.
    // Extend the physical book only as far as its actual part bounds require, preserving
    // both the chosen page focus and the ordinary scene projection/depth test.
    let bounds = [
        (
            Vec3::new(-WIDTH / 2., -HEIGHT / 2., -0.006),
            Vec3::new(WIDTH / 2., HEIGHT / 2., 0.01),
        ),
        (
            Vec3::new(-PAGE_WIDTH, -PAGE_HEIGHT / 2., 0.001),
            Vec3::new(-0.002, PAGE_HEIGHT / 2., PAGE_Z),
        ),
        (
            Vec3::new(0.002, -PAGE_HEIGHT / 2., 0.001),
            Vec3::new(PAGE_WIDTH, PAGE_HEIGHT / 2., PAGE_Z),
        ),
        (
            Vec3::new(0.002, -PAGE_HEIGHT / 2., 0.0003),
            Vec3::new(PAGE_WIDTH, PAGE_HEIGHT / 2., 0.0003),
        ),
        (
            Vec3::new(0.002, -PAGE_HEIGHT / 2., -0.0003),
            Vec3::new(PAGE_WIDTH, PAGE_HEIGHT / 2., -0.0003),
        ),
    ];
    let mut nearest = f32::INFINITY;
    for (part, model) in models.iter().enumerate() {
        if part >= 3 && !state.turning() {
            continue;
        }
        let (lo, hi) = bounds[part];
        for x in [lo.x, hi.x] {
            for y in [lo.y, hi.y] {
                for z in [lo.z, hi.z] {
                    nearest = nearest
                        .min((model.transform_point3(Vec3::new(x, y, z)) - camera.pos).dot(f));
                }
            }
        }
    }
    if nearest < 0.065 {
        let clearance = Mat4::from_translation(f * (0.065 - nearest));
        for model in &mut models {
            *model = clearance * *model;
        }
    }
    models
}

/// Intersect a normalized screen pointer with the normally depth-tested reading surface.
/// Turning leaves consume clicks until settled; this avoids selecting ink hidden by a leaf.
pub fn book_uv(camera: &Camera, screen: Vec2, state: &ManualState) -> Option<Vec2> {
    if !state.held || state.turning() || !screen.is_finite() {
        return None;
    }
    let inv = camera.view_proj().inverse();
    let unproject = |z: f32| {
        let p = inv * glam::Vec4::new(screen.x * 2. - 1., 1. - screen.y * 2., z, 1.);
        p.truncate() / p.w
    };
    let start = unproject(0.);
    let end = unproject(1.);
    let inverse_model = models(camera, state)[0].inverse();
    let local_start = inverse_model.transform_point3(start);
    let direction = inverse_model.transform_vector3(end - start);
    if direction.z.abs() < 1e-6 {
        return None;
    }
    let t = (PAGE_Z - local_start.z) / direction.z;
    if t < 0. || !t.is_finite() {
        return None;
    }
    let p = local_start + direction * t;
    let uv = Vec2::new(p.x / (PAGE_WIDTH * 2.) + 0.5, 0.5 - p.y / PAGE_HEIGHT);
    (uv.min_element() >= 0. && uv.max_element() <= 1.).then_some(uv)
}

fn line(r: &mut Renderer, a: Vec2, b: Vec2, width: f32, color: [f32; 4]) {
    let steps = ((a.distance(b) / 0.002).ceil() as usize).clamp(1, 500);
    for n in 0..=steps {
        let p = a.lerp(b, n as f32 / steps as f32);
        r.fill_rect(p.x - width / 2., p.y - width / 2., width, width, color);
    }
}
fn outline(r: &mut Renderer, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
    r.fill_rect(x, y, w, stroke(), c);
    r.fill_rect(x, y + h, w, stroke(), c);
    r.fill_rect(x, y, stroke(), h, c);
    r.fill_rect(x + w, y, stroke(), h, c);
}
fn circle(r: &mut Renderer, c: Vec2, radius: f32, color: [f32; 4]) {
    for n in 0..36 {
        let p = |a: f32| c + Vec2::new(a.cos(), a.sin()) * radius;
        line(
            r,
            p(n as f32 * std::f32::consts::TAU / 36.),
            p((n + 1) as f32 * std::f32::consts::TAU / 36.),
            stroke(),
            color,
        );
    }
}
fn diagram(r: &mut Renderer, kind: Drawing) {
    let ink = INK;
    match kind {
        Drawing::Track | Drawing::Gap => {
            let gap = matches!(kind, Drawing::Gap);
            for n in 0..11 {
                if gap && (4..=6).contains(&n) {
                    continue;
                }
                let x = 0.15 + n as f32 * 0.06;
                outline(r, x, 0.59, 0.047, 0.025, ink);
                outline(r, x, 0.73, 0.047, 0.025, ink);
            }
            for n in 0..5 {
                circle(r, Vec2::new(0.22 + n as f32 * 0.14, 0.674), 0.041, ink);
            }
            if gap {
                r.draw_text("GAP", 0.48, 0.56, 0.027, ink, Align::Center);
                line(
                    r,
                    Vec2::new(0.48, 0.58),
                    Vec2::new(0.48, 0.62),
                    stroke(),
                    ink,
                );
            }
        }
        Drawing::Link | Drawing::Pin => {
            for n in 0..3 {
                let x = 0.17 + n as f32 * 0.22;
                outline(r, x, 0.63, 0.20, 0.105, ink);
                for k in 0..4 {
                    outline(r, x + 0.015 + k as f32 * 0.045, 0.65, 0.025, 0.05, ink);
                }
            }
            line(
                r,
                Vec2::new(0.12, 0.625),
                Vec2::new(0.88, 0.625),
                stroke(),
                ink,
            );
            if matches!(kind, Drawing::Pin) {
                line(r, Vec2::new(0.85, 0.57), Vec2::new(0.65, 0.57), 0.004, ink);
                line(
                    r,
                    Vec2::new(0.65, 0.57),
                    Vec2::new(0.70, 0.55),
                    stroke(),
                    ink,
                );
                r.draw_text("PIN", 0.85, 0.54, 0.027, ink, Align::Center);
            } else {
                r.draw_text("SPARE LINK", 0.5, 0.58, 0.027, ink, Align::Center);
            }
        }
        Drawing::Drive => {
            for (x, label) in [(0.12, "ENGINE"), (0.41, "DRIVE"), (0.7, "TRACK")] {
                outline(r, x, 0.65, 0.2, 0.08, ink);
                r.draw_text(label, x + 0.1, 0.675, 0.025, ink, Align::Center);
            }
            line(
                r,
                Vec2::new(0.32, 0.69),
                Vec2::new(0.41, 0.69),
                stroke(),
                ink,
            );
            line(
                r,
                Vec2::new(0.61, 0.69),
                Vec2::new(0.70, 0.69),
                stroke(),
                ink,
            );
        }
        Drawing::Recovery => {
            outline(r, 0.14, 0.67, 0.38, 0.08, ink);
            outline(r, 0.22, 0.62, 0.17, 0.05, ink);
            line(r, Vec2::new(0.39, 0.64), Vec2::new(0.6, 0.64), 0.008, ink);
            line(
                r,
                Vec2::new(0.52, 0.73),
                Vec2::new(0.84, 0.73),
                stroke(),
                ink,
            );
            for n in 0..3 {
                circle(r, Vec2::new(0.22 + n as f32 * 0.1, 0.75), 0.026, ink);
            }
            r.draw_text("RECORD LOCATION", 0.5, 0.56, 0.027, ink, Align::Center);
        }
        Drawing::Patient => {
            outline(r,0.13,0.64,0.74,0.085,ink);
            circle(r,Vec2::new(0.23,0.675),0.029,ink);
            line(r,Vec2::new(0.27,0.675),Vec2::new(0.60,0.675),0.009,ink);
            line(r,Vec2::new(0.60,0.675),Vec2::new(0.79,0.65),0.005,ink);
            line(r,Vec2::new(0.60,0.675),Vec2::new(0.79,0.70),0.005,ink);
            for x in [0.15,0.85] {line(r,Vec2::new(x,0.725),Vec2::new(x,0.755),stroke(),ink);}
            r.draw_text("SAME PERSON / CONTINUING CARE",0.5,0.58,0.023,ink,Align::Center);
        }
        Drawing::Winch => {
            outline(r,0.12,0.66,0.15,0.08,ink);
            circle(r,Vec2::new(0.195,0.69),0.025,ink);
            outline(r,0.63,0.65,0.22,0.09,ink);
            line(r,Vec2::new(0.27,0.685),Vec2::new(0.63,0.685),stroke(),ink);
            line(r,Vec2::new(0.27,0.71),Vec2::new(0.63,0.71),stroke(),ink);
            line(r,Vec2::new(0.14,0.74),Vec2::new(0.09,0.76),0.006,ink);
            r.draw_text("ANCHOR / CABLE / TOW POINT",0.5,0.58,0.024,ink,Align::Center);
        }
        Drawing::Stowage => {
            outline(r,0.26,0.60,0.48,0.155,ink);
            for (x,y,label) in [(0.29,0.615,"1"),(0.61,0.615,"2"),(0.29,0.69,"3"),(0.61,0.69,"4")] {
                outline(r,x,y,0.10,0.045,ink);
                r.draw_text(label,x+0.05,y+0.011,0.023,ink,Align::Center);
            }
            r.draw_text("FRONT",0.50,0.57,0.023,ink,Align::Center);
        }
    }
}

/// The page set for the pixel grid (the overlay book): the page texture is exactly the internal
/// pixels the page covers, the type the 5 x 7 pixel face at one dot per pixel (the title two),
/// the rules one pixel, the plates the pixel bakes drawn one texel per pixel, the schematics at
/// a one-pixel pen. The choice rows keep the bands `click` reads.
pub fn draw_page_pixel(r: &mut Renderer, page: u8, state: &ManualState, size: (u32, u32)) {
    use crate::pixel_font::{draw_at, width_px};
    let (pw, ph) = (size.0.max(64), size.1.max(48));
    let (fw, fh) = (pw as f32, ph as f32);
    let (ink, red, paper, faded) = (INK, RED, PAPER, FADED);
    let right_hand = page % 2 == 1;
    STROKE.with(|s| s.set(1.0 / fh));
    r.fill_rect(0., 0., 1., 1., paper);
    let p = page_definition(page);
    let m: i32 = 8;
    let (pwi, phi) = (pw as i32, ph as i32);
    // The running head: the three controls, small and red.
    draw_at(r, "INDEX", m, 3, 1, red, size);
    let mark = if state.bookmark == Some(state.spread) { "MARKED" } else { "MARK" };
    draw_at(r, mark, (pwi - width_px(mark, 1) as i32) / 2, 3, 1, red, size);
    draw_at(r, "RIBBON", pwi - m - width_px("RIBBON", 1) as i32, 3, 1, red, size);
    // The title in two-dot capitals, a red rule, the chapter in red.
    draw_at(r, p.title, m, 13, 2, ink, size);
    r.fill_rect(m as f32 / fw, 30.0 / fh, (pwi - 2 * m) as f32 / fw, 1.0 / fh, red);
    draw_at(r, p.margin, m, 34, 1, red, size);
    // The plate at the right when the asset root has it; the text in the column beside it.
    let plate = p.plate.and_then(|name| state.plate(name)).filter(|(_, w, _)| (*w as i32) < pwi - 3 * m - 60);
    let plate_w = plate.map_or(0, |(_, w, _)| w as i32);
    let column = if plate.is_some() { pwi - 3 * m - plate_w } else { pwi - 2 * m };
    let wrap = (column / 6).max(10) as usize;
    let mut y = 46;
    for (lead, text) in p.text {
        if !lead.is_empty() {
            draw_at(r, lead, m, y, 1, red, size);
            y += 9;
        }
        for line in crate::ui::wrap_text(text, wrap) {
            draw_at(r, &line, m, y, 1, ink, size);
            y += 9;
        }
        y += 4;
    }
    let caption_y = |r: &mut Renderer, text: &str, cx: i32, y: i32| {
        let w = width_px(text, 1) as i32;
        draw_at(r, text, (cx - w / 2).clamp(m, pwi - m - w), y, 1, faded, size);
    };
    match plate {
        Some((handle, w, h)) => {
            let (w, h) = (w as i32, h as i32);
            let x = pwi - m - w;
            r.draw_image(handle, x as f32 / fw, 46.0 / fh, w as f32 / fw, h as f32 / fh, [1., 1., 1., 1.]);
            caption_y(r, "1/35 KIT CAD", x + w / 2, 46 + h + 3);
        }
        None if y < (0.52 * fh) as i32 => {
            // The schematic under the text at a one-pixel pen, when the text leaves it room.
            diagram(r, p.drawing);
            caption_y(r, "SCHEMATIC", pwi / 2, (0.785 * fh) as i32);
        }
        None => {}
    }
    // The diagnosis choices at the foot: the rows `click` reads (y 0.81 + n * 0.065, 0.052 tall).
    for (index, choice) in p.choices.iter().enumerate() {
        let y = ((0.81 + index as f32 * 0.065) * fh).round() as i32;
        draw_at(r, ">", m, y + 3, 1, red, size);
        draw_at(r, choice.label.trim_end_matches(['>', ' ']), m + 9, y + 3, 1, ink, size);
        r.fill_rect(m as f32 / fw, (y + 13) as f32 / fh, 0.42, 1.0 / fh, faded);
    }
    // The folio at the outer corner; the turn marks at the foot where `click` turns the leaf.
    let folio = (page + 1).to_string();
    let fy = phi - 10;
    let fx = if right_hand { pwi - m - width_px(&folio, 1) as i32 } else { m };
    draw_at(r, &folio, fx, fy, 1, ink, size);
    draw_at(r, "< PREV", (0.14 * fw) as i32, fy, 1, faded, size);
    draw_at(r, "NEXT >", (0.86 * fw) as i32 - width_px("NEXT >", 1) as i32, fy, 1, faded, size);
    STROKE.with(|s| s.set(PRINT_STROKE));
}

/// Queue print for one page's albedo. The renderer consumes it before any screen HUD.
pub fn draw_page(r: &mut Renderer, page: u8, state: &ManualState) {
    use crate::ui::{FACE_MANUAL, FACE_MANUAL_BOLD};
    let (ink, red, paper, faded) = (INK, RED, PAPER, FADED);
    let right_hand = page % 2 == 1;
    STROKE.with(|s| s.set(PRINT_STROKE));
    r.fill_rect(0., 0., 1., 1., paper);
    // The running head: the book's three controls, small and red, as the Fibel sets its folios.
    r.draw_text_face("INDEX", 0.05, 0.026, 0.015, red, Align::Left, FACE_MANUAL);
    r.draw_text_face(if state.bookmark == Some(state.spread) { "MARKED" } else { "MARK" }, 0.50, 0.026, 0.015, red, Align::Center, FACE_MANUAL);
    r.draw_text_face("RIBBON", 0.95, 0.026, 0.015, red, Align::Right, FACE_MANUAL);
    let p = page_definition(page);
    // The title, letter-spaced capitals across the page, a red rule under it.
    let spaced: String = p.title.chars().flat_map(|c| [c, ' ']).collect::<String>().trim_end().to_owned();
    r.draw_text_face(&spaced, 0.05, 0.062, 0.032, ink, Align::Left, FACE_MANUAL_BOLD);
    r.fill_rect(0.05, 0.118, 0.90, PRINT_STROKE, red);
    r.draw_text_face(p.margin, 0.05, 0.132, 0.017, red, Align::Left, FACE_MANUAL);
    let plate = p.plate.and_then(|name| state.plate(name));
    // The body: beside a plate, one column on the left; without one, two columns above the
    // schematic. Each paragraph opens with its red lead word on its own line.
    let wrap = if plate.is_some() { COLUMN_WRAP_BESIDE_PLATE } else { COLUMN_WRAP };
    let columns: Vec<(f32, &[(&str, &str)])> = if plate.is_some() || p.text.len() < 3 {
        vec![(0.05, p.text)]
    } else {
        let split = p.text.len().div_ceil(2);
        vec![(0.05, &p.text[..split]), (0.52, &p.text[split..])]
    };
    for (x, paragraphs) in columns {
        let mut y = 0.175;
        for (lead, text) in paragraphs {
            if !lead.is_empty() {
                r.draw_text_face(lead, x, y, 0.022, red, Align::Left, FACE_MANUAL_BOLD);
                y += 0.032;
            }
            for line in crate::ui::wrap_text(text, wrap) {
                r.draw_text_face(&line, x, y, BODY_TEXT_SIZE, ink, Align::Left, FACE_MANUAL);
                y += BODY_LINE_HEIGHT;
            }
            y += 0.010;
        }
    }
    match plate {
        Some((handle, tw, th)) => {
            // The plate fills the right half, its proportions kept (the page is 1.414:1).
            let aspect = tw as f32 / th.max(1) as f32;
            let w = 0.45;
            let h = (w * 1.414 / aspect).min(0.60);
            r.draw_image(handle, 0.51, 0.165, w, h, [1., 1., 1., 1.]);
            r.draw_text_face("WORKS DRAWING · 1/35 KIT CAD · NOT TO SCALE", 0.735, 0.165 + h + 0.008, 0.013, faded, Align::Center, FACE_MANUAL);
        }
        None => {
            diagram(r, p.drawing);
            r.draw_text_face("SCHEMATIC · NOT TO SCALE", 0.5, 0.782, 0.014, faded, Align::Center, FACE_MANUAL);
        }
    }
    // The diagnosis choices at the foot: a red arrow, the label, a hairline; their rows are the
    // click zones `click` reads (y 0.81 + n * 0.065, 0.052 tall).
    for (index, choice) in p.choices.iter().enumerate() {
        let y = 0.81 + index as f32 * 0.065;
        r.draw_text_face(">", 0.05, y + 0.012, 0.024, red, Align::Left, FACE_MANUAL_BOLD);
        r.draw_text_face(choice.label.trim_end_matches(['>', ' ']), 0.085, y + 0.014, 0.021, ink, Align::Left, FACE_MANUAL_BOLD);
        r.fill_rect(0.05, y + 0.050, 0.42, 0.0012, faded);
    }
    // The folio at the outer corner; the turn marks at the foot where `click` turns the leaf.
    let (nx, align) = if right_hand { (0.95, Align::Right) } else { (0.05, Align::Left) };
    r.draw_text_face(&(page + 1).to_string(), nx, 0.952, 0.020, ink, align, FACE_MANUAL);
    r.draw_text_face("< PREV", 0.14, 0.955, 0.014, faded, Align::Left, FACE_MANUAL);
    r.draw_text_face("NEXT >", 0.86, 0.955, 0.014, faded, Align::Right, FACE_MANUAL);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loupe_stands_over_the_pointed_page_point_and_shows_it_three_times_larger() {
        let mut book = ManualState::default();
        book.held = true;
        let mut camera = Camera::new(16. / 9.);
        camera.pos = Vec3::new(0., 1.5, 0.);
        camera.set_forward(Vec3::Z);
        let pointer = Vec2::new(0.42, 0.52);
        let view = loupe_view(&camera, pointer, &book).expect("the pointer is on the book");
        let base = models(&camera, &book)[0];
        let normal = base.transform_vector3(Vec3::Z).normalize();
        assert!((view.forward + normal).length() < 1e-4, "looks down the page normal");
        let under = base.inverse().transform_point3(view.eye - normal * LOUPE_HEIGHT);
        assert!((under.z - PAGE_Z).abs() < 1e-4 && under.x.abs() < PAGE_WIDTH && under.y.abs() < PAGE_HEIGHT * 0.5,
            "the paper under the lens is on the open spread, LOUPE_HEIGHT below it: {under}");
        // The paper the circle shows: the pass's frame height in paper, times the circle's share
        // of it, times the read scale: a third of the lens's diameter, however the pass was set.
        let frame_across = 2. * LOUPE_HEIGHT * (view.fov_y * 0.5).tan();
        let paper_across = frame_across * 2. * view.radius_uv * view.sample_scale;
        assert!((paper_across - LOUPE_DIAMETER / LOUPE_POWER).abs() < 1e-4, "the glass shows a third of its diameter of paper: {paper_across}");
        assert!(view.sample_scale >= 1.0 && view.fov_y <= LOUPE_PASS_MAX_FOV + 1e-6, "{} {}", view.sample_scale, view.fov_y);
        assert!((view.centre_uv - pointer).length() < 0.005, "the lens sits under the pointer: {} vs {pointer}", view.centre_uv);
        assert!(view.radius_uv > 0.03 && view.radius_uv < 0.25, "a hand lens on the screen: {}", view.radius_uv);
        assert!(loupe_view(&camera, Vec2::new(0.02, 0.02), &book).is_none(), "off the book there is no lens");
    }

    #[test]
    fn the_overlay_book_fits_the_grid_one_texel_per_pixel_and_keeps_its_page_proportions() {
        let mut book = ManualState::default();
        book.held = true;
        for grid in [(1024u32, 576u32), (640, 360), (1280, 800), (1024, 432)] {
            let lay = overlay_layout(grid, &book);
            let [x, y, w, h] = lay.px;
            let (pw, ph) = overlay_page_pixels(grid);
            assert_eq!((w, h), (2 * pw as i32, ph as i32), "{grid:?}: one texel per pixel at zoom 1");
            assert!(x >= 0 && y >= 0 && x + w <= grid.0 as i32 && y + h <= grid.1 as i32, "{grid:?}: {:?}", lay.px);
            let page_aspect = pw as f32 / ph as f32;
            assert!((page_aspect - PIXEL_PAGE_ASPECT).abs() < 0.02, "{grid:?}: page aspect {page_aspect}");
            assert!(w as f32 >= 0.6 * grid.0 as f32, "{grid:?}: the spread spans the frame ({w})");
            for [tx, ty, tw, th] in lay.tabs_px {
                assert!(tx == x + w && ty >= y && ty + th <= y + h && tw > 0, "tabs down the fore-edge");
                assert!(tx + tw + 6 * lay.unit <= grid.0 as i32, "{grid:?}: the tabs stay on the frame");
            }
            assert_eq!(lay.unit, ((grid.1 as f32 / 360.0).round() as i32).max(1));
        }
        // Zoom about the pointed point: it stays under the pointer, two pixels per texel.
        let grid = (1024, 576);
        let focus = Vec2::new(0.25, 0.45);
        let at1 = overlay_layout(grid, &book).spread;
        let p1 = Vec2::new(at1[0] + focus.x * at1[2], at1[1] + focus.y * at1[3]);
        book.focus(focus);
        book.zoom_by(4.0);
        let lay2 = overlay_layout(grid, &book);
        let at2 = lay2.spread;
        let p2 = Vec2::new(at2[0] + focus.x * at2[2], at2[1] + focus.y * at2[3]);
        assert!(p1.distance(p2) < 1.5 / 576.0 && lay2.zoom == 2 && lay2.px[2] == 2 * overlay_layout(grid, &ManualState::default()).px[2], "{p1} {p2} {at1:?} {at2:?}");
    }

    #[test]
    fn tabs_open_their_chapters_and_the_leaf_narrows_to_the_spine() {
        let mut book = ManualState::default();
        book.held = true;
        let grid = (1024, 576);
        let lay = overlay_layout(grid, &book);
        let [tx, ty, tw, th] = lay.tabs[1];
        assert_eq!(overlay_hit(grid, &book, Vec2::new(tx + tw * 0.5, ty + th * 0.5)), Some(OverlayHit::Tab(1)));
        let [x, y, w, h] = lay.spread;
        match overlay_hit(grid, &book, Vec2::new(x + w * 0.75, y + h * 0.5)) {
            Some(OverlayHit::Page(uv)) => assert!((uv - Vec2::new(0.75, 0.5)).length() < 1e-5, "{uv}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(overlay_hit(grid, &book, Vec2::new(0.01, 0.01)), None);
        assert_eq!((chapter_of(8), chapter_of(13), chapter_of(15)), (2, 3, 5));
        assert!(book.open_spread(8));
        // The leaf starts over the right page a page wide, is edge-on at the spine midway,
        // then widens over the left page and lands as the new spread.
        let (right, start) = book.overlay_leaf().unwrap();
        assert!(right && (start - 1.0).abs() < 1e-5);
        book.tick(TURN_SECONDS * 0.5);
        let (_, mid) = book.overlay_leaf().unwrap();
        assert!(mid < 0.05, "{mid}");
        book.tick(TURN_SECONDS * 0.25);
        let (right, late) = book.overlay_leaf().unwrap();
        assert!(!right && late > 0.5, "{right} {late}");
        book.tick(TURN_SECONDS);
        assert!(book.overlay_leaf().is_none() && book.spread() == 8);
    }

    #[test]
    fn diagnosis_choices_and_repair_sequence_are_reachable_without_vehicle_state() {
        let mut book = ManualState::default();
        book.held = true;
        for target in [1, 2, 3, 4, 5, 7, 0] {
            let page = page_definition(book.spread * 2);
            let choice = page
                .choices
                .iter()
                .position(|c| c.target == target)
                .unwrap();
            assert!(book.click(Vec2::new(0.25, 0.835 + choice as f32 * 0.065)));
            book.tick(TURN_SECONDS + 0.01);
            assert_eq!(book.spread, target);
        }
    }

    #[test]
    fn partial_turn_has_correct_underlying_pages_and_finishes_on_close() {
        let mut book = ManualState::default();
        book.held = true;
        assert!(book.turn(1));
        assert_eq!(book.page_bindings(), [0, 3, 1, 2]);
        book.tick(TURN_SECONDS / 2.);
        assert!((book.leaf_angle() + std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!(!book.click(Vec2::new(0.25, 0.835)));
        book.set_held(false);
        assert_eq!(book.spread(), 1);
        assert!(!book.turning());
        book.set_held(true);
        assert!(book.turn(-1));
        assert_eq!(book.page_bindings(), [0, 3, 1, 2]);
        book.tick(TURN_SECONDS);
        assert_eq!(book.spread(), 0);
    }

    #[test]
    fn page_changes_invalidate_printed_surface_history_and_zoom_turn_stays_in_front_of_eye() {
        let mut book = ManualState::default();
        book.set_held(true);
        let initial = book.presentation_key();
        book.tick(0.1);
        assert_eq!(
            book.presentation_key(),
            initial,
            "an idle page keeps lighting history"
        );
        book.zoom_by(8.0);
        assert_ne!(book.presentation_key(), initial);
        assert!(book.turn(1));
        assert_eq!(book.zoom(), 1.0);
        let before_turn = book.presentation_key();
        book.tick(TURN_SECONDS / 2.0);
        assert_ne!(book.presentation_key(), before_turn);
        book.zoom_by(8.0);
        assert_eq!(
            book.zoom(),
            1.0,
            "a moving leaf cannot be pushed through the eye"
        );
        let camera = Camera::new(16.0 / 9.0);
        let leaf = models(&camera, &book)[3];
        for vertex in &mesh()[3].vertices {
            let world = leaf.transform_point3(Vec3::from_array(vertex.pos));
            assert!((world - camera.pos).dot(camera.forward()) > 0.06);
        }
        let parts = mesh();
        let assert_clearance = |book: &ManualState| {
            for (part, (mesh, model)) in parts.iter().zip(models(&camera, book)).enumerate() {
                if !book.visible_part(part) {
                    continue;
                }
                for vertex in &mesh.vertices {
                    let world = model.transform_point3(Vec3::from_array(vertex.pos));
                    assert!(
                        (world - camera.pos).dot(camera.forward()) > 0.06,
                        "book part {part} crossed the near-eye clearance"
                    );
                }
            }
        };
        for zoom in [0., 4., 8.] {
            for x in [0.05, 0.5, 0.95] {
                for y in [0.05, 0.5, 0.95] {
                    let mut book = ManualState::default();
                    book.set_held(true);
                    book.zoom_by(zoom);
                    book.focus(Vec2::new(x, y));
                    assert_clearance(&book);
                }
            }
        }
        for direction in [-1, 1] {
            let mut book = ManualState::default();
            book.restore_state(if direction < 0 { 1 } else { 0 }, None)
                .unwrap();
            book.set_held(true);
            assert!(book.turn(direction));
            for _ in 0..=24 {
                assert_clearance(&book);
                book.tick(TURN_SECONDS / 24.);
            }
        }
    }

    #[test]
    fn navigation_bounds_and_bookmark_survive_reopening() {
        let mut book = ManualState::default();
        book.held = true;
        assert!(!book.turn(-1));
        assert!(!book.click(Vec2::new(f32::NAN, 0.9)));
        let last = SPREAD_COUNT - 1;
        assert!(book.go_to(last));
        book.tick(1.);
        assert!(!book.turn(1));
        assert!(book.click(Vec2::new(0.25, 0.03)));
        assert_eq!(book.bookmark(), Some(last));
        let mut restored = ManualState::default();
        restored
            .restore_state(book.spread(), book.bookmark())
            .unwrap();
        assert_eq!(restored.spread(), last);
        assert_eq!(restored.bookmark(), Some(last));
        assert!(!restored.held);
        assert!(restored.restore_state(SPREAD_COUNT, None).is_err());
        assert!(restored.restore_state(0, Some(SPREAD_COUNT)).is_err());
    }

    #[test]
    fn physical_book_has_covers_thickness_and_reversed_leaf_normal() {
        let meshes = mesh();
        assert_eq!(meshes.len(), PART_COUNT);
        assert!(meshes[0].max.z - meshes[0].min.z > 0.01);
        assert!(meshes[1].max.z - meshes[1].min.z > 0.01);
        assert!(meshes[3].vertices[0].normal[2] > 0.99);
        assert!(meshes[4].vertices[0].normal[2] < -0.99);
        for mesh in meshes {
            for v in mesh.vertices {
                assert!(Vec3::from_array(v.normal).is_normalized());
            }
        }
    }

    #[test]
    fn reading_pointer_intersects_real_page_and_rejects_outside() {
        let camera = Camera::new(16. / 9.);
        let mut book = ManualState::default();
        book.held = true;
        let center =
            camera.view_proj() * models(&camera, &book)[0] * Vec3::new(0., 0., PAGE_Z).extend(1.);
        let ndc = center.truncate() / center.w;
        let pointer = Vec2::new(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        let hit = book_uv(&camera, pointer, &book).unwrap();
        assert!(hit.distance(Vec2::splat(0.5)) < 0.1);
        assert!(book_uv(&camera, Vec2::new(0., 0.), &book).is_none());
    }

    #[test]
    fn default_spread_projects_every_cover_and_page_vertex_inside_reading_frame() {
        let parts = mesh();
        let mut book = ManualState::default();
        book.set_held(true);
        for aspect in [4. / 3., 16. / 9., 21. / 9.] {
            for fov in [60_f32, 62.] {
                let mut camera = Camera::new(aspect);
                camera.pos = Vec3::new(22., 3., -41.);
                camera.yaw = 0.7;
                camera.pitch = -0.3;
                camera.fov_y = fov.to_radians();
                camera.fov_y = reading_fov(&camera);
                for (part, (mesh, model)) in parts.iter().zip(models(&camera, &book)).enumerate() {
                    if !book.visible_part(part) {
                        continue;
                    }
                    for vertex in &mesh.vertices {
                        let clip =
                            camera.view_proj() * model * Vec3::from_array(vertex.pos).extend(1.);
                        let ndc = clip.truncate() / clip.w;
                        assert!(clip.w > camera.znear && (0. ..=1.).contains(&ndc.z));
                        assert!(ndc.x.abs() <= 0.98 && ndc.y.abs() <= 0.98,
                            "resting book part {part} is cropped: {ndc:?}, aspect {aspect}, FOV {fov}");
                    }
                }
            }
        }
    }

    #[test]
    fn every_printed_choice_targets_a_real_spread_and_text_clears_the_diagram() {
        for number in 0..SPREAD_COUNT * 2 {
            let page = page_definition(number);
            assert!(page
                .choices
                .iter()
                .all(|choice| choice.target < SPREAD_COUNT));
            assert!(page.choices.len() <= 2);
            // The columns as `draw_page` splits them: beside a plate one column of 36 characters,
            // otherwise two of 40 above the schematic (which starts at y 0.55).
            let (wrap, columns): (usize, Vec<&[(&str, &str)]>) = if page.plate.is_some() || page.text.len() < 3 {
                (if page.plate.is_some() { COLUMN_WRAP_BESIDE_PLATE } else { COLUMN_WRAP }, vec![page.text])
            } else {
                let split = page.text.len().div_ceil(2);
                (COLUMN_WRAP, vec![&page.text[..split], &page.text[split..]])
            };
            for column in columns {
                let mut bottom = 0.175;
                for (lead, text) in column {
                    if !lead.is_empty() { bottom += 0.032; }
                    bottom += crate::ui::wrap_text(text, wrap).len() as f32 * BODY_LINE_HEIGHT + 0.010;
                }
                let limit = if page.plate.is_some() { 0.80 } else { 0.55 };
                assert!(bottom <= limit, "page {} body runs to {bottom:.2}, past {limit}", number + 1);
            }
        }
    }
}
