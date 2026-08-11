//! Kessel front-end: the GERMAN title screen, menu, and death screen. The engine had no text before
//! this — here we bake a glyph coverage atlas from DejaVu Sans (regular + bold, bundled under
//! `assets/fonts/`) with the pure-Rust `ab_glyph` rasterizer, and draw text as textured quads at
//! DISPLAY resolution, composited over the final blit (OUTSIDE the PS1 crush) so the UI stays crisp.
//!
//! `Ui` owns the atlas + the two `text.wgsl` pipelines and is held by the `Renderer`; the front-end
//! calls `Renderer::draw_text` / `draw_text_bold` to queue strings, then one of the `render_ui_*`
//! entry points flushes them. All on-screen strings live in this file (`ALL GERMAN — no English`):
//! the menu labels, the fallen-Panzer-ace epitaph table, and the layout functions `draw_menu` /
//! `draw_death` that both the live window and the headless `ui` subcommand share.

use std::collections::HashMap;
use std::net::SocketAddr;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use wgpu::util::DeviceExt;

use crate::lan::{DiscoveredSession, LanMission, PROTOCOL_VERSION};
use crate::renderer::Renderer;

// PIXEL FONTS (playtest: humanist strokes turned to mush through the hard-threshold pixelation —
// pixel fonts are DESIGNED for it). VT323 (OFL): terminal pixel face for body text; Press Start 2P
// (OFL): chunky arcade face for titles / headers / key labels. Both carry Latin-1 (umlauts, †).
const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/VT323-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

/// Pixel height each glyph is rasterized at into the atlas. High enough that the huge title stays
/// reasonably sharp when scaled up at 1080p; the crisp-UI look tolerates a little bilinear softness.
const ATLAS_PX: f32 = 110.0;
const ATLAS_W: u32 = 1024;
const PAD: u32 = 2; // guard band between packed glyphs so bilinear taps don't bleed neighbours

/// Every character the front-end can draw: printable ASCII + German umlauts/ß + memorial and
/// punctuation marks used by the menu (guillemets, middot, dagger, em dash).
fn charset() -> Vec<char> {
    let mut v: Vec<char> = (0x20u8..=0x7Eu8).map(|b| b as char).collect();
    v.extend("äöüßÄÖÜ†•·—«»‹›".chars());
    v
}

/// Horizontal placement of a queued string relative to its `x` anchor. The front-end lays every
/// current string out centred; `Left`/`Right` round out the general `draw_text` API for future HUD use.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Time-of-day the player picks on the title page. Stored and carried into Playing; the scene-side
/// day-vs-night wiring reads it LATER (the night-look pass) — nothing here applies it to the game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimeOfDay {
    Tag,
    Nacht,
}

impl TimeOfDay {
    pub fn label(self) -> &'static str {
        match self {
            TimeOfDay::Tag => "Day",
            TimeOfDay::Nacht => "Night",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            TimeOfDay::Tag => TimeOfDay::Nacht,
            TimeOfDay::Nacht => TimeOfDay::Tag,
        }
    }
}

/// The chosen play mode. German display labels; ASCII identifiers (Ueberleben) for portability.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayMode {
    /// The DEV GARAGE gunnery yard — the default boot scene (world::garage()).
    Garage,
    Kampagne,
    Gefecht,
    /// SPLITSCREEN couch crew co-op: the STÜTZPUNKT take-&-hold op on the act-0 bowl.
    /// Keyboard = commander, first pad = gunner, second pad = driver.
    Koop,
    /// Direct load of the act-2 map (Oderbruch spring) — a map-test entry, no act flow.
    TestAct2,
    /// Direct load of the act-3 map (Halbe) — a map-test entry, no act flow.
    TestAct3,
}

// Menu item indices (order they appear on screen).
pub const IDX_KAMPAGNE: usize = 0;
pub const IDX_GEFECHT: usize = 1;
pub const IDX_KOOP: usize = 2;
pub const IDX_TESTACT2: usize = 3;
pub const IDX_TESTACT3: usize = 4;
pub const IDX_GARAGE: usize = 5;
pub const IDX_TIME: usize = 6;
pub const IDX_INVINCIBLE: usize = 7;
pub const IDX_SETTINGS: usize = 8;
pub const IDX_QUIT: usize = 9;
pub const MENU_COUNT: usize = 10;

// The crew co-op page keeps its first three actions fixed, follows them with however many nearby
// Tigers are currently advertising, then ends with Back. Keeping this mapping pure makes a browse
// list that changes under us deterministic for keyboard and controller navigation.
pub const LAN_IDX_HOST: usize = 0;
pub const LAN_IDX_MANUAL: usize = 1;
pub const LAN_IDX_SPLITSCREEN: usize = 2;
pub const LAN_SESSION_START: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanMenuSelection {
    Host,
    ManualAddress,
    SplitScreen,
    Session(usize),
    Back,
}

#[derive(Clone, Debug)]
pub struct LanMenuModel {
    pub selected: usize,
    pub mission: LanMission,
    pub manual_address: String,
    pub editing_address: bool,
    pub status: String,
}

impl Default for LanMenuModel {
    fn default() -> Self {
        Self {
            selected: LAN_IDX_HOST,
            mission: LanMission::Campaign,
            manual_address: String::new(),
            editing_address: false,
            status: "Hosts appear automatically. Press R to retry discovery.".to_owned(),
        }
    }
}

impl LanMenuModel {
    pub fn row_count(session_count: usize) -> usize {
        LAN_SESSION_START + session_count + 1
    }

    pub fn back_index(session_count: usize) -> usize {
        Self::row_count(session_count) - 1
    }

    pub fn selection(&self, session_count: usize) -> LanMenuSelection {
        if self.selected == LAN_IDX_HOST {
            LanMenuSelection::Host
        } else if self.selected == LAN_IDX_MANUAL {
            LanMenuSelection::ManualAddress
        } else if self.selected == LAN_IDX_SPLITSCREEN {
            LanMenuSelection::SplitScreen
        } else if self.selected < Self::back_index(session_count) {
            LanMenuSelection::Session(self.selected - LAN_SESSION_START)
        } else {
            LanMenuSelection::Back
        }
    }

    pub fn move_selection(&mut self, delta: isize, session_count: usize) {
        let count = Self::row_count(session_count) as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
    }

    pub fn clamp_selection(&mut self, session_count: usize) {
        self.selected = self.selected.min(Self::back_index(session_count));
    }

    /// Keep a discovered host selected by network identity while the auto-refresh list reorders.
    /// If that exact host vanished, stay on the nearest remaining session row instead of silently
    /// moving the cursor to an unrelated row at the old absolute index.
    pub fn reconcile_session_servers(
        &mut self,
        previous: Option<(SocketAddr, usize)>,
        servers: &[SocketAddr],
    ) {
        let Some((server, previous_index)) = previous else {
            self.clamp_selection(servers.len());
            return;
        };
        if let Some(index) = servers.iter().position(|candidate| *candidate == server) {
            self.selected = LAN_SESSION_START + index;
        } else if servers.is_empty() {
            self.selected = Self::back_index(0);
        } else {
            self.selected = LAN_SESSION_START + previous_index.min(servers.len() - 1);
        }
    }

    pub fn cycle_mission(&mut self, forward: bool) {
        self.mission = match (self.mission, forward) {
            (LanMission::Garage, true) | (LanMission::Campaign, false) => LanMission::Arena,
            (LanMission::Arena, true) | (LanMission::Garage, false) => LanMission::Campaign,
            (LanMission::Campaign, true) | (LanMission::Arena, false) => LanMission::Garage,
        };
    }

    pub fn append_address_text(&mut self, text: &str) {
        for ch in text.chars() {
            if self.manual_address.len() >= 96 {
                break;
            }
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | ':' | '[' | ']') {
                self.manual_address.push(ch);
            }
        }
    }
}

#[cfg(test)]
mod lan_menu_tests {
    use super::*;

    #[test]
    fn dynamic_session_rows_map_and_wrap_deterministically() {
        let mut menu = LanMenuModel::default();
        assert_eq!(menu.selection(2), LanMenuSelection::Host);
        menu.move_selection(-1, 2);
        assert_eq!(menu.selection(2), LanMenuSelection::Back);
        menu.move_selection(1, 2);
        assert_eq!(menu.selection(2), LanMenuSelection::Host);

        menu.selected = LAN_SESSION_START + 1;
        assert_eq!(menu.selection(2), LanMenuSelection::Session(1));
        menu.clamp_selection(0);
        assert_eq!(menu.selection(0), LanMenuSelection::Back);
    }

    #[test]
    fn mission_cycle_is_reversible() {
        let mut menu = LanMenuModel::default();
        assert_eq!(menu.mission, LanMission::Campaign);
        menu.cycle_mission(true);
        assert_eq!(menu.mission, LanMission::Garage);
        menu.cycle_mission(false);
        assert_eq!(menu.mission, LanMission::Campaign);
    }

    #[test]
    fn manual_address_input_accepts_hosts_ipv4_and_bracketed_ipv6_only() {
        let mut menu = LanMenuModel::default();
        menu.append_address_text(" hal.local [fe80::1]:28460 / nope ");
        assert_eq!(menu.manual_address, "hal.local[fe80::1]:28460nope");
    }

    #[test]
    fn discovery_refresh_preserves_host_identity_across_sort_and_expiry() {
        let a: SocketAddr = "192.168.1.10:28460".parse().unwrap();
        let b: SocketAddr = "192.168.1.11:28460".parse().unwrap();
        let c: SocketAddr = "192.168.1.12:28460".parse().unwrap();
        let mut menu = LanMenuModel::default();
        menu.selected = LAN_SESSION_START + 1;

        menu.reconcile_session_servers(Some((b, 1)), &[c, b, a]);
        assert_eq!(menu.selection(3), LanMenuSelection::Session(1));
        menu.reconcile_session_servers(Some((b, 1)), &[b, a, c]);
        assert_eq!(menu.selection(3), LanMenuSelection::Session(0));

        menu.reconcile_session_servers(Some((b, 0)), &[a, c]);
        assert_eq!(menu.selection(2), LanMenuSelection::Session(0));
        menu.reconcile_session_servers(Some((a, 0)), &[]);
        assert_eq!(menu.selection(0), LanMenuSelection::Back);
    }
}

/// Live selection + settings state for the title menu.
#[derive(Clone, Copy)]
pub struct MenuModel {
    pub selected: usize,
    pub time: TimeOfDay,
    /// Invincible toggle: the run can't kill you (Game::god). For sightseeing and testing.
    pub invincible: bool,
    /// True when this menu was opened from a live game. Escape then returns to that exact run
    /// instead of discarding campaign state or rebuilding the world.
    pub resume_available: bool,
}

impl Default for MenuModel {
    fn default() -> Self {
        MenuModel {
            selected: IDX_KAMPAGNE,
            time: TimeOfDay::Tag,
            invincible: false,
            resume_available: false,
        }
    }
}

/// Fallen Panzer aces (real names + real birthdays) used as the death-screen memorial epitaph. The
/// death year is stylized to 1942 (the Kessel); several of these men actually survived the war.
/// German memorial notation: `*` = born, `†` = died. Birthdays in German date format (Tag. Monat).
pub const ACES: [(&str, &str); 8] = [
    ("Michael Wittmann", "* 22. April 1914"),
    ("Kurt Knispel", "* 20. September 1921"),
    ("Otto Carius", "* 27. Mai 1922"),
    ("Ernst Barkmann", "* 25. August 1919"),
    ("Johannes Bölter", "* 19. Februar 1915"),
    ("Rudolf von Ribbentrop", "* 11. Mai 1921"),
    ("Albert Ernst", "* 8. November 1912"),
    ("Paul Egger", "* 4. April 1916"),
];

// ── Front-end palette (linear RGBA; the display target is sRGB so the hardware encodes on write) ──
// The menu is now PURE BLACK behind LEFT-aligned WHITE text (the spinning pixel-Tiger fills the right
// third). Title + menu items are white; the selected item switches to warm amber + a left caret so
// the selection reads clearly against white siblings. The old cold-blue gradient consts survive only
// for the controls reference (`draw_controls`).
const BG_MENU_TOP: [f32; 4] = [0.015, 0.028, 0.055, 1.0];
const BG_MENU_BOTTOM: [f32; 4] = [0.045, 0.070, 0.110, 1.0];
const BG_BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

const COL_TITLE: [f32; 4] = [0.97, 0.98, 1.00, 1.0]; // near-pure white title
const COL_SUBTITLE: [f32; 4] = [0.62, 0.66, 0.72, 1.0]; // soft grey-white caption
const COL_ITEM: [f32; 4] = [0.88, 0.90, 0.94, 1.0]; // legible white for unselected items
const COL_SELECT: [f32; 4] = [1.00, 0.72, 0.28, 1.0]; // warm amber — the current-selection marker
const COL_HINT: [f32; 4] = [0.58, 0.62, 0.68, 1.0]; // dim grey-white control hint

const COL_DEATH: [f32; 4] = [0.86, 0.86, 0.88, 1.0];
const COL_DEATH_CAUSE: [f32; 4] = [0.66, 0.46, 0.32, 1.0]; // muted scorched amber — subordinate to the headline
const COL_ACE_NAME: [f32; 4] = [0.78, 0.80, 0.84, 1.0];
const COL_ACE_DATE: [f32; 4] = [0.58, 0.62, 0.70, 1.0];

// Mission instruments: warm, slightly dirty dial paint rather than bright videogame white. The HUD
// is intentionally sparse; these values are what a commander could plausibly ask over the intercom.
const COL_MISSION: [f32; 4] = [0.86, 0.84, 0.73, 0.82];
const COL_MISSION_DIM: [f32; 4] = [0.55, 0.55, 0.49, 0.64];
const COL_MISSION_WARN: [f32; 4] = [0.82, 0.46, 0.24, 0.88];
const COL_MISSION_CLEAR: [f32; 4] = [0.67, 0.72, 0.61, 0.72];

// Campaign conclusions are almost colourless. They should feel like a name read from stone after
// the engine stops, not like a victory/reward screen.
const COL_ENDING_TITLE: [f32; 4] = [0.80, 0.81, 0.79, 1.0];
const COL_ENDING_BODY: [f32; 4] = [0.57, 0.59, 0.57, 1.0];
const COL_ENDING_STONE: [f32; 4] = [0.39, 0.42, 0.40, 1.0];

/// A queued string (built into quads at flush time, when the target size is known).
struct TextCmd {
    text: String,
    x: f32,
    y: f32,
    size: f32,
    color: [f32; 4],
    align: Align,
    bold: bool,
}

/// An uploaded RGBA texture in the image registry (asset-browser thumbnails, preview inset, the
/// shared 1×1 white used for tinted panel/selection rects). Kept alive by holding the texture.
struct ImageTex {
    bind: wgpu::BindGroup,
    _tex: wgpu::Texture,
}

/// A queued image quad: `rect` is [x0,y0,x1,y1] in normalized [0,1] top-left coords; `color` tints
/// the sampled texel (rgb multiply + alpha). Drawn in queue order, all BEFORE text, so labels sit on
/// top of thumbnails and panels.
struct ImgCmd {
    handle: u32,
    rect: [f32; 4],
    color: [f32; 4],
}

/// Baked metrics + atlas placement for one glyph, all in ATLAS_PX pixel units.
#[derive(Clone, Copy, Default)]
struct GlyphMetric {
    uv0: [f32; 2],
    uv1: [f32; 2],
    size: [f32; 2], // rasterized bitmap size
    off: [f32; 2],  // offset from the pen baseline (min.x, min.y; min.y negative = above baseline)
    advance: f32,
}

/// Intermediate rasterized glyph before it is packed into the shared atlas.
struct Raster {
    key: (bool, char),
    w: u32,
    h: u32,
    off: [f32; 2],
    advance: f32,
    bmp: Vec<u8>, // w*h coverage, 0..255
}

pub struct Ui {
    glyphs: HashMap<(bool, char), GlyphMetric>,
    ascent: f32, // ATLAS_PX units — distance from the top anchor down to the baseline

    // GPU
    atlas_bind: wgpu::BindGroup,
    shader: wgpu::ShaderModule,
    text_layout: wgpu::PipelineLayout,
    bg_layout: wgpu::PipelineLayout,
    text_pipes: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    bg_pipes: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    vbuf: Option<wgpu::Buffer>, // kept alive across submit; replaced each flush

    cmds: Vec<TextCmd>,
    bg_top: [f32; 4],
    bg_bottom: [f32; 4],

    // ── image quads (asset-browser thumbnails + tinted rects) ──
    img_bgl: wgpu::BindGroupLayout,
    img_layout: wgpu::PipelineLayout,
    img_pipes: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    img_sampler: wgpu::Sampler,
    images: Vec<ImageTex>, // registry; index = handle returned by upload_image
    img_cmds: Vec<ImgCmd>,
    img_vbuf: Option<wgpu::Buffer>,
    white: u32, // handle of a 1×1 white texture → tinted filled rects
}

impl Ui {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let (glyphs, ascent, atlas_w, atlas_h, pixels) = build_atlas();

        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-atlas"),
            size: wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        // Linear-clamp DOWN-samples the high-res (ATLAS_PX) coverage cleanly into the small internal-
        // res glyph quads (averaging preserves thin strokes); `text.wgsl::fs_text` then HARD-THRESHOLDS
        // that coverage so every internal-res texel is fully on or off — after the nearest upscale the
        // glyph edges read as crisp pixel steps, not a soft antialiased gradient.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let text_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-text-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let atlas_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-atlas-bind"),
            layout: &text_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/text.wgsl").into()),
        });
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-text-pl"),
            bind_group_layouts: &[&text_bgl],
            push_constant_ranges: &[],
        });
        let bg_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-bg-pl"),
            bind_group_layouts: &[],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::FRAGMENT,
                range: 0..32, // two vec4 (top + bottom colours)
            }],
        });

        // Image quads: an RGBA texture + linear-clamp sampler per registered image. The layout is
        // texture@0 + sampler@1, matching text.wgsl's `img`/`samp` bindings.
        let img_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-img-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let img_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-img-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let img_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-img-pl"),
            bind_group_layouts: &[&img_bgl],
            push_constant_ranges: &[],
        });

        let mut ui = Ui {
            glyphs,
            ascent,
            atlas_bind,
            shader,
            text_layout,
            bg_layout,
            text_pipes: HashMap::new(),
            bg_pipes: HashMap::new(),
            vbuf: None,
            cmds: Vec::new(),
            bg_top: BG_BLACK,
            bg_bottom: BG_BLACK,
            img_bgl,
            img_layout,
            img_pipes: HashMap::new(),
            img_sampler,
            images: Vec::new(),
            img_cmds: Vec::new(),
            img_vbuf: None,
            white: 0,
        };
        // Handle 0 = a 1×1 opaque white; drawing it with a tint yields filled panel/selection rects.
        ui.white = ui.upload_image(device, queue, &[255, 255, 255, 255], 1, 1);
        ui
    }

    /// Register an RGBA8 image (`rgba` = `w*h*4` bytes, row-major, no padding) and return its handle
    /// for `draw_image`. Uploaded once; handles are stable for the life of the `Ui`.
    pub fn upload_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> u32 {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-image"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w.max(1) * 4),
                rows_per_image: Some(h.max(1)),
            },
            wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-image-bind"),
            layout: &self.img_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.img_sampler),
                },
            ],
        });
        self.images.push(ImageTex { bind, _tex: tex });
        (self.images.len() - 1) as u32
    }

    /// Queue an image quad. `x,y,w,h` are normalized [0,1] with the origin top-left; `color` tints
    /// the texel (use white to draw as-is). Drawn before text so labels overlay it.
    pub fn draw_image(&mut self, handle: u32, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.img_cmds.push(ImgCmd {
            handle,
            rect: [x, y, x + w, y + h],
            color,
        });
    }

    /// Queue a filled, tinted rectangle (the 1×1 white handle under a tint) — panels, highlights,
    /// the drag ghost. Same coordinate convention as `draw_image`.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let white = self.white;
        self.draw_image(white, x, y, w, h, color);
    }

    pub fn set_bg(&mut self, top: [f32; 4], bottom: [f32; 4]) {
        self.bg_top = top;
        self.bg_bottom = bottom;
    }

    pub fn clear(&mut self) {
        self.cmds.clear();
        self.img_cmds.clear();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
        align: Align,
        bold: bool,
    ) {
        self.cmds.push(TextCmd {
            text: text.to_string(),
            x,
            y,
            size,
            color,
            align,
            bold,
        });
    }

    fn ensure_pipes(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if !self.bg_pipes.contains_key(&format) {
            let pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ui-bg"),
                layout: Some(&self.bg_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_bg"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_bg"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });
            self.bg_pipes.insert(format, pipe);
        }
        if !self.text_pipes.contains_key(&format) {
            let alpha_blend = wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            };
            let attrs = [
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ];
            let pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ui-text"),
                layout: Some(&self.text_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_text"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    }],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_text"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(alpha_blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });
            self.text_pipes.insert(format, pipe);
        }
        if !self.img_pipes.contains_key(&format) {
            let alpha_blend = wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            };
            let attrs = [
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ];
            let pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ui-img"),
                layout: Some(&self.img_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_img"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    }],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_img"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(alpha_blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });
            self.img_pipes.insert(format, pipe);
        }
    }

    /// Build the [pos.xy, uv.xy, color.rgba] vertex stream for the queued image quads, plus the
    /// per-image draw runs (handle, first-vertex, count) so each quad binds its own texture.
    fn build_image_vertices(&self) -> (Vec<f32>, Vec<(u32, u32, u32)>) {
        let mut v: Vec<f32> = Vec::with_capacity(self.img_cmds.len() * 48);
        let mut runs: Vec<(u32, u32, u32)> = Vec::with_capacity(self.img_cmds.len());
        for cmd in &self.img_cmds {
            let [x0, y0, x1, y1] = cmd.rect;
            // normalized top-left [0,1] → NDC (y flips)
            let nx0 = x0 * 2.0 - 1.0;
            let nx1 = x1 * 2.0 - 1.0;
            let ny0 = 1.0 - y0 * 2.0;
            let ny1 = 1.0 - y1 * 2.0;
            let c = cmd.color;
            let start = (v.len() / 8) as u32;
            let mut vert = |nx: f32, ny: f32, u: f32, w: f32| {
                v.extend_from_slice(&[nx, ny, u, w, c[0], c[1], c[2], c[3]]);
            };
            vert(nx0, ny0, 0.0, 0.0);
            vert(nx1, ny0, 1.0, 0.0);
            vert(nx0, ny1, 0.0, 1.0);
            vert(nx1, ny0, 1.0, 0.0);
            vert(nx1, ny1, 1.0, 1.0);
            vert(nx0, ny1, 0.0, 1.0);
            runs.push((cmd.handle, start, 6));
        }
        (v, runs)
    }

    /// Build the interleaved [pos.xy, uv.xy, color.rgba] vertex stream for the queued strings, laid
    /// out for a `cw`×`ch` target. Coordinates: `x`/`y` are normalized [0,1] with the origin at the
    /// top-left; `size` is the em height as a fraction of target height.
    fn build_vertices(&self, cw: u32, ch: u32) -> Vec<f32> {
        let tw = cw as f32;
        let th = ch as f32;
        let mut v: Vec<f32> = Vec::new();
        for cmd in &self.cmds {
            let factor = (cmd.size * th) / ATLAS_PX;
            let mut width = 0.0f32;
            // Characters missing from the baked atlas advance a space width (instead of zero) so
            // the surrounding letters don't silently jam together; measure + draw must agree.
            let fallback_adv = self
                .glyphs
                .get(&(cmd.bold, ' '))
                .map(|g| g.advance)
                .unwrap_or(0.0);
            for c in cmd.text.chars() {
                width += self
                    .glyphs
                    .get(&(cmd.bold, c))
                    .map(|g| g.advance)
                    .unwrap_or(fallback_adv)
                    * factor;
            }
            let mut pen_x = match cmd.align {
                Align::Left => cmd.x * tw,
                Align::Center => cmd.x * tw - width * 0.5,
                Align::Right => cmd.x * tw - width,
            };
            let baseline = cmd.y * th + self.ascent * factor;
            for c in cmd.text.chars() {
                let g = match self.glyphs.get(&(cmd.bold, c)) {
                    Some(g) => *g,
                    None => {
                        pen_x += fallback_adv * factor; // unknown glyph: leave a gap, keep the pen moving
                        continue;
                    }
                };
                if g.size[0] > 0.0 && g.size[1] > 0.0 {
                    let x0 = pen_x + g.off[0] * factor;
                    let y0 = baseline + g.off[1] * factor;
                    let x1 = x0 + g.size[0] * factor;
                    let y1 = y0 + g.size[1] * factor;
                    // pixels → NDC (y flips: pixel-down is NDC-up)
                    let nx0 = x0 / tw * 2.0 - 1.0;
                    let nx1 = x1 / tw * 2.0 - 1.0;
                    let ny0 = 1.0 - y0 / th * 2.0;
                    let ny1 = 1.0 - y1 / th * 2.0;
                    let [u0, v0] = g.uv0;
                    let [u1, v1] = g.uv1;
                    let col = cmd.color;
                    let mut vert = |nx: f32, ny: f32, u: f32, vv: f32| {
                        v.extend_from_slice(&[nx, ny, u, vv, col[0], col[1], col[2], col[3]]);
                    };
                    vert(nx0, ny0, u0, v0);
                    vert(nx1, ny0, u1, v0);
                    vert(nx0, ny1, u0, v1);
                    vert(nx1, ny0, u1, v0);
                    vert(nx1, ny1, u1, v1);
                    vert(nx0, ny1, u0, v1);
                }
                pen_x += g.advance * factor;
            }
        }
        v
    }

    /// Record the background gradient + all queued text into `encoder`, targeting `view`. Clears the
    /// target first (owns the whole frame). Drains the command queue. `cw`/`ch` are the target's pixel
    /// dimensions (surface size or capture size).
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        cw: u32,
        ch: u32,
    ) {
        self.flush(device, encoder, view, format, cw, ch, true);
    }

    /// Record ONLY the queued text OVER whatever is already in `view` (LoadOp::Load, no background
    /// fill). Used by the main-menu path so the text composites on top of the crushed turntable Tiger
    /// instead of erasing it.
    pub fn render_over(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        cw: u32,
        ch: u32,
    ) {
        self.flush(device, encoder, view, format, cw, ch, false);
    }

    /// Shared flush. `draw_bg` = true clears the target and paints the background gradient before the
    /// text; false loads the existing contents and draws text only (composite overlay).
    #[allow(clippy::too_many_arguments)]
    fn flush(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        cw: u32,
        ch: u32,
        draw_bg: bool,
    ) {
        self.ensure_pipes(device, format);
        let verts = self.build_vertices(cw, ch);
        let n_verts = (verts.len() / 8) as u32;
        if n_verts > 0 {
            self.vbuf = Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("ui-vbuf"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            );
        }
        let (img_verts, img_runs) = self.build_image_vertices();
        if !img_verts.is_empty() {
            self.img_vbuf = Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("ui-img-vbuf"),
                    contents: bytemuck::cast_slice(&img_verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            );
        }
        let mut bg_push = [0u8; 32];
        bg_push[0..16].copy_from_slice(bytemuck::bytes_of(&self.bg_top));
        bg_push[16..32].copy_from_slice(bytemuck::bytes_of(&self.bg_bottom));

        let load = if draw_bg {
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        } else {
            wgpu::LoadOp::Load
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if draw_bg {
            pass.set_pipeline(&self.bg_pipes[&format]);
            pass.set_push_constants(wgpu::ShaderStages::FRAGMENT, 0, &bg_push);
            pass.draw(0..3, 0..1);
        }
        // Images FIRST (panels, thumbnails, highlights) so text labels overlay them. One draw per
        // quad — each binds its own texture; the shared 1×1 white handle covers tinted rects.
        if let (Some(vbuf), false) = (&self.img_vbuf, img_runs.is_empty()) {
            pass.set_pipeline(&self.img_pipes[&format]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            for (handle, start, count) in &img_runs {
                if let Some(tex) = self.images.get(*handle as usize) {
                    pass.set_bind_group(0, &tex.bind, &[]);
                    pass.draw(*start..*start + *count, 0..1);
                }
            }
        }
        if let (Some(vbuf), true) = (&self.vbuf, n_verts > 0) {
            pass.set_pipeline(&self.text_pipes[&format]);
            pass.set_bind_group(0, &self.atlas_bind, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..n_verts, 0..1);
        }
        drop(pass);
        self.cmds.clear();
        self.img_cmds.clear();
    }
}

/// Rasterize both weights of the full charset with `ab_glyph` and shelf-pack them into one R8
/// coverage atlas. Returns the glyph metric map, the ascent (ATLAS_PX units), the atlas dimensions,
/// and the packed pixels.
fn build_atlas() -> (HashMap<(bool, char), GlyphMetric>, f32, u32, u32, Vec<u8>) {
    let scale = PxScale::from(ATLAS_PX);
    let mut rasters: Vec<Raster> = Vec::new();
    let mut ascent = ATLAS_PX * 0.8;

    for (bold, bytes) in [(false, FONT_REGULAR), (true, FONT_BOLD)] {
        let font = FontRef::try_from_slice(bytes).expect("bundled font failed to parse");
        let sf = font.as_scaled(scale);
        if !bold {
            ascent = sf.ascent();
        }
        for ch in charset() {
            let id = font.glyph_id(ch);
            let advance = sf.h_advance(id);
            if let Some(outlined) = font.outline_glyph(id.with_scale(scale)) {
                let b = outlined.px_bounds();
                let w = b.width().ceil().max(0.0) as u32;
                let h = b.height().ceil().max(0.0) as u32;
                let mut bmp = vec![0u8; (w * h) as usize];
                outlined.draw(|x, y, c| {
                    if x < w && y < h {
                        bmp[(y * w + x) as usize] = (c * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                    }
                });
                rasters.push(Raster {
                    key: (bold, ch),
                    w,
                    h,
                    off: [b.min.x, b.min.y],
                    advance,
                    bmp,
                });
            } else {
                // whitespace / no outline — advance only, no quad
                rasters.push(Raster {
                    key: (bold, ch),
                    w: 0,
                    h: 0,
                    off: [0.0, 0.0],
                    advance,
                    bmp: Vec::new(),
                });
            }
        }
    }

    // Shelf-pack into ATLAS_W-wide rows.
    let mut placements: Vec<(usize, u32, u32)> = Vec::new(); // (raster idx, px, py)
    let mut cx = PAD;
    let mut cy = PAD;
    let mut row_h = 0u32;
    for (i, r) in rasters.iter().enumerate() {
        if r.w == 0 || r.h == 0 {
            continue;
        }
        if cx + r.w + PAD > ATLAS_W {
            cx = PAD;
            cy += row_h + PAD;
            row_h = 0;
        }
        placements.push((i, cx, cy));
        cx += r.w + PAD;
        row_h = row_h.max(r.h);
    }
    let atlas_h = cy + row_h + PAD;
    let mut pixels = vec![0u8; (ATLAS_W * atlas_h) as usize];

    let mut glyphs: HashMap<(bool, char), GlyphMetric> = HashMap::new();
    // whitespace glyphs (advance only)
    for r in &rasters {
        if r.w == 0 || r.h == 0 {
            glyphs.insert(
                r.key,
                GlyphMetric {
                    advance: r.advance,
                    ..Default::default()
                },
            );
        }
    }
    for (i, px, py) in placements {
        let r = &rasters[i];
        for row in 0..r.h {
            let dst = ((py + row) * ATLAS_W + px) as usize;
            let src = (row * r.w) as usize;
            pixels[dst..dst + r.w as usize].copy_from_slice(&r.bmp[src..src + r.w as usize]);
        }
        glyphs.insert(
            r.key,
            GlyphMetric {
                uv0: [px as f32 / ATLAS_W as f32, py as f32 / atlas_h as f32],
                uv1: [
                    (px + r.w) as f32 / ATLAS_W as f32,
                    (py + r.h) as f32 / atlas_h as f32,
                ],
                size: [r.w as f32, r.h as f32],
                off: r.off,
                advance: r.advance,
            },
        );
    }

    (glyphs, ascent, ATLAS_W, atlas_h, pixels)
}

// ── Screen composition (shared by the live window and the headless `ui` subcommand) ──────────────

/// Compose the German title screen: the huge `der Kessel` logo, the mode/setting list with the
/// current selection highlighted in amber, and a control hint. Queues into the renderer; the caller
/// flushes with a `render_ui_*` entry point.
pub fn draw_menu(r: &mut Renderer, m: &MenuModel) {
    // Pure-black backdrop. The turntable pixel-Tiger (drawn by the menu render pass, before this text)
    // fills the right third; all text is LEFT-aligned in the left column.
    r.set_ui_bg(BG_BLACK, BG_BLACK);

    let x = 0.075; // left column anchor (all rows share it)
    r.draw_text_bold("der Kessel", x, 0.125, 0.150, COL_TITLE, Align::Left);
    r.draw_text(
        "Winter 1942 — based on a true story",
        x,
        0.305,
        0.036,
        COL_SUBTITLE,
        Align::Left,
    );

    // Always show the ‹ value › arrows so the day/night toggle reads as adjustable even when the row
    // isn't selected (press ← / → to flip Tag ↔ Nacht).
    let time_label = format!("Time of day  ‹ {} ›", m.time.label());
    let invincible_label = format!(
        "Invincible  ‹ {} ›",
        if m.invincible { "On" } else { "Off" }
    );
    let labels: [(String, &str); MENU_COUNT] = [
        ("Campaign".to_string(), ""),
        (
            "Arena".to_string(),
            "the training bowl — mortal fight in the ruins",
        ),
        (
            "Crew co-op".to_string(),
            "host or join LAN - or share this screen",
        ),
        ("Test: Act 2 map".to_string(), "Oderbruch, spring"),
        ("Test: Act 3 map".to_string(), "Halbe"),
        ("Dev Garage".to_string(), "back to the gunnery yard"),
        (time_label, ""),
        (invincible_label, ""),
        ("Settings".to_string(), ""),
        ("Quit".to_string(), ""),
    ];

    let y0 = 0.40;
    let dy = 0.055; // 10 rows now — tightened so Quit clears the control hint at the bottom
    for (i, (label, en)) in labels.iter().enumerate() {
        let selected = i == m.selected;
        let y = y0 + dy * i as f32;
        if selected {
            // A left caret hangs into the margin + amber colour marks the current selection; the
            // unselected siblings are indented to sit flush with the caret's label.
            let text = format!("›  {}", label);
            r.draw_text_bold(&text, x, y, 0.050, COL_SELECT, Align::Left);
            r.draw_text(en, x + 0.30, y + 0.014, 0.024, COL_HINT, Align::Left);
        } else {
            r.draw_text_bold(label, x + 0.028, y, 0.044, COL_ITEM, Align::Left);
            r.draw_text(en, x + 0.30, y + 0.012, 0.022, COL_HINT, Align::Left);
        }
    }

    let hint = if m.resume_available {
        "arrows move · enter selects · esc resumes"
    } else {
        "arrows move · enter selects · quit is explicit"
    };
    r.draw_text(hint, x, 0.93, 0.030, COL_HINT, Align::Left);
}

fn draw_lan_row(r: &mut Renderer, selected: bool, y: f32, label: &str, detail: &str) {
    let (x, detail_x) = (0.075, 0.49);
    if selected {
        r.draw_text_bold(&format!(">  {label}"), x, y, 0.041, COL_SELECT, Align::Left);
        r.draw_text(detail, detail_x, y + 0.008, 0.025, COL_HINT, Align::Left);
    } else {
        r.draw_text_bold(label, x + 0.028, y, 0.036, COL_ITEM, Align::Left);
        r.draw_text(detail, detail_x, y + 0.007, 0.022, COL_HINT, Align::Left);
    }
}

fn compact_label(value: &str, max_chars: usize) -> String {
    let mut out: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

/// Friend-facing crew setup page. Discovery is passive and continuously refreshed by the app; the
/// fixed actions remain usable even when privacy settings or a firewall block the browse socket.
pub fn draw_lan_menu(
    r: &mut Renderer,
    m: &LanMenuModel,
    sessions: &[DiscoveredSession],
    browser_available: bool,
) {
    r.set_ui_bg(BG_BLACK, BG_BLACK);
    let x = 0.075;
    r.draw_text_bold("CREW CO-OP", x, 0.075, 0.082, COL_TITLE, Align::Left);
    r.draw_text(
        "One Tiger. Every crewman works from his actual station.",
        x,
        0.175,
        0.032,
        COL_SUBTITLE,
        Align::Left,
    );
    let browse = if browser_available {
        format!(
            "NEARBY TIGERS: {}   -   AUTO-REFRESHING UDP {}",
            sessions.len(),
            crate::lan::DISCOVERY_PORT
        )
    } else {
        "BROWSE UNAVAILABLE - MANUAL IP STILL WORKS".to_owned()
    };
    r.draw_text(&browse, x, 0.225, 0.026, COL_ITEM, Align::Left);
    if !m.status.is_empty() {
        r.draw_text(
            &compact_label(&m.status, 76),
            x,
            0.265,
            0.025,
            COL_HINT,
            Align::Left,
        );
    }

    draw_lan_row(
        r,
        m.selected == LAN_IDX_HOST,
        0.325,
        &format!("Host LAN  ‹ {} ›", m.mission.label()),
        "left/right mission - enter hosts",
    );
    let address = if m.manual_address.is_empty() {
        if m.editing_address {
            "_".to_owned()
        } else {
            "host or IP".to_owned()
        }
    } else if m.editing_address {
        format!("{} _", compact_label(&m.manual_address, 34))
    } else {
        compact_label(&m.manual_address, 34)
    };
    draw_lan_row(
        r,
        m.selected == LAN_IDX_MANUAL,
        0.390,
        &format!("Manual IP  {address}"),
        if m.editing_address {
            "type address - enter joins - esc cancels"
        } else {
            "enter to type; port optional"
        },
    );
    draw_lan_row(
        r,
        m.selected == LAN_IDX_SPLITSCREEN,
        0.455,
        "Split-screen crew",
        "keyboard + up to two controllers",
    );

    r.draw_text_bold(
        "JOIN A NEARBY TIGER",
        x,
        0.530,
        0.026,
        COL_SUBTITLE,
        Align::Left,
    );
    const VISIBLE: usize = 4;
    let selected_session = match m.selection(sessions.len()) {
        LanMenuSelection::Session(index) => Some(index),
        _ => None,
    };
    let start = if sessions.len() <= VISIBLE {
        0
    } else if matches!(m.selection(sessions.len()), LanMenuSelection::Back) {
        sessions.len() - VISIBLE
    } else {
        selected_session
            .unwrap_or(0)
            .saturating_sub(1)
            .min(sessions.len() - VISIBLE)
    };
    let end = (start + VISIBLE).min(sessions.len());
    if sessions.is_empty() {
        r.draw_text(
            "No host seen yet. Wait a moment, press R, or use Manual IP.",
            x + 0.028,
            0.585,
            0.027,
            COL_HINT,
            Align::Left,
        );
    } else {
        for (row, index) in (start..end).enumerate() {
            let session = &sessions[index];
            let full = session.beacon.occupied_crew >= session.beacon.max_crew;
            let condition = if !session.compatible() {
                format!(
                    "INCOMPATIBLE v{} (this build v{})",
                    session.beacon.protocol_version, PROTOCOL_VERSION
                )
            } else if full {
                format!(
                    "FULL {}/{}",
                    session.beacon.occupied_crew, session.beacon.max_crew
                )
            } else {
                let scene = if session.beacon.mission == LanMission::Campaign {
                    format!("CAMPAIGN - ACT {}", session.beacon.act.max(1))
                } else {
                    session.beacon.mission.label().to_owned()
                };
                format!(
                    "{scene} - {}/{} CREW - ENTER JOINS",
                    session.beacon.occupied_crew, session.beacon.max_crew
                )
            };
            draw_lan_row(
                r,
                m.selected == LAN_SESSION_START + index,
                0.575 + row as f32 * 0.060,
                &compact_label(&session.beacon.session_name, 24),
                &condition,
            );
        }
        if start > 0 {
            r.draw_text("more above", 0.40, 0.542, 0.019, COL_HINT, Align::Left);
        }
        if end < sessions.len() {
            r.draw_text("more below", 0.40, 0.815, 0.019, COL_HINT, Align::Left);
        }
    }

    draw_lan_row(
        r,
        matches!(m.selection(sessions.len()), LanMenuSelection::Back),
        0.840,
        "Back",
        "return to title",
    );
    r.draw_text(
        "arrows move - enter selects - R refreshes - esc goes back",
        x,
        0.930,
        0.028,
        COL_HINT,
        Align::Left,
    );
}

/// Compose the death screen. `text_alpha` gates the memorial: 0 → the pure-black beat (nothing
/// drawn); >0 → the `Du bist gestorben` headline, the German cause of death, and the chosen ace's
/// epitaph, all faded by that alpha. `cause` is the already-localized GERMAN cause-of-death line
/// (e.g. "Durchschlag — Flanke"); empty draws nothing for it.
pub fn draw_death(r: &mut Renderer, ace_idx: usize, cause: &str, text_alpha: f32) {
    r.set_ui_bg(BG_BLACK, BG_BLACK);
    let a = text_alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        return; // black screen, no text yet
    }
    let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * a];

    r.draw_text_bold("You died", 0.5, 0.33, 0.092, fade(COL_DEATH), Align::Center);

    // The specific cause of death, in German — set just under the headline, smaller + muted so it
    // reads as a subordinate caption, not competing with the "Du bist gestorben" line above it.
    if !cause.is_empty() {
        r.draw_text(
            cause,
            0.5,
            0.45,
            0.034,
            fade(COL_DEATH_CAUSE),
            Align::Center,
        );
    }

    let (name, birth) = ACES[ace_idx % ACES.len()];
    r.draw_text_bold(name, 0.5, 0.53, 0.050, fade(COL_ACE_NAME), Align::Center);
    r.draw_text(
        &format!("{birth}   † 1942"),
        0.5,
        0.605,
        0.040,
        fade(COL_ACE_DATE),
        Align::Center,
    );
}

/// Restrained in-mission commander instrumentation. This is deliberately not an objective list:
/// it exposes only the act, current pressure wave, physical stores, and whether the route is still
/// held. `wave_total == 0` omits the pressure readout for quiet traversal beats.
#[allow(clippy::too_many_arguments)]
pub fn draw_mission_hud(
    r: &mut Renderer,
    act: u8,
    wave: usize,
    wave_total: usize,
    fuel_liters: f32,
    rack_rounds: usize,
    goal_locked: bool,
    finale: bool,
    friendly_tigers: u8,
    town_integrity: u8,
) {
    // Two narrow black-glass plates leave almost the entire world unobstructed.
    r.fill_rect(0.018, 0.022, 0.205, 0.075, [0.0, 0.0, 0.0, 0.34]);
    r.fill_rect(0.777, 0.022, 0.205, 0.075, [0.0, 0.0, 0.0, 0.34]);

    let act_label = if finale {
        format!("AKT {act} · HALBE")
    } else {
        format!("AKT {act}")
    };
    r.draw_text_bold(&act_label, 0.034, 0.033, 0.021, COL_MISSION, Align::Left);
    if wave_total > 0 {
        let pressure = if finale { "ANGRIFF" } else { "FEINDDRUCK" };
        r.draw_text(
            &format!("{pressure}  {}/{}", wave.min(wave_total), wave_total),
            0.034,
            0.064,
            0.022,
            if finale {
                COL_MISSION_WARN
            } else {
                COL_MISSION_DIM
            },
            Align::Left,
        );
    } else {
        r.draw_text(
            "FUNKSTILLE",
            0.034,
            0.064,
            0.022,
            COL_MISSION_DIM,
            Align::Left,
        );
    }

    let fuel = fuel_liters.max(0.0).min(9999.0);
    r.draw_text_bold(
        &format!("KRAFTSTOFF  {fuel:03.0} L"),
        0.966,
        0.033,
        0.020,
        if fuel <= 60.0 {
            COL_MISSION_WARN
        } else {
            COL_MISSION
        },
        Align::Right,
    );
    r.draw_text(
        &format!("88 mm  {rack_rounds:02}"),
        0.966,
        0.064,
        0.022,
        if rack_rounds <= 5 {
            COL_MISSION_WARN
        } else {
            COL_MISSION_DIM
        },
        Align::Right,
    );

    // A status whisper at the bottom edge, not an arrow. It says whether the line has broken but
    // never where to drive; navigation still belongs to the paper map and visible landscape.
    let (gate, color) = if goal_locked {
        (
            if finale {
                "HALBE · LINIE HALTEN"
            } else {
                "WESTWEG NOCH GEHALTEN"
            },
            COL_MISSION_WARN,
        )
    } else {
        (
            if finale {
                "DER WEG IST OFFEN"
            } else {
                "WESTWEG FREI"
            },
            COL_MISSION_CLEAR,
        )
    };
    let gate = if finale {
        format!(
            "{gate}  ·  ROTTE {}  ·  ORT {:02}%",
            friendly_tigers.saturating_add(1),
            town_integrity
        )
    } else {
        gate.to_string()
    };
    r.draw_text(&gate, 0.5, 0.936, 0.022, color, Align::Center);
}

/// Conclusive campaign coda. Black, stone-grey type, and a single cemetery mark keep the ending
/// quiet; kills and surviving stores are recorded as an after-action ledger rather than a score.
pub fn draw_campaign_ending(
    r: &mut Renderer,
    ending: crate::campaign::CampaignEnding,
    total_kills: u16,
    fuel_liters: f32,
    scars: u16,
    alpha: f32,
) {
    r.set_ui_bg(BG_BLACK, BG_BLACK);
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * a];
    let (title, coda) = match ending {
        crate::campaign::CampaignEnding::TownHeld => (
            "The road remained open.",
            "At dawn, birds returned to the forest cemetery.",
        ),
        crate::campaign::CampaignEnding::BreakoutSurvives => (
            "Some made it through.",
            "Behind them, the pines burned without sound.",
        ),
        crate::campaign::CampaignEnding::PyrrhicSurvival => (
            "The Tiger came home alone.",
            "No engine answered from the trees.",
        ),
    };

    r.draw_text(
        "HALBE · APRIL 1945",
        0.5,
        0.165,
        0.025,
        fade(COL_ENDING_STONE),
        Align::Center,
    );
    r.draw_text(
        "†",
        0.5,
        0.245,
        0.058,
        fade(COL_ENDING_STONE),
        Align::Center,
    );
    r.draw_text_bold(
        title,
        0.5,
        0.365,
        0.047,
        fade(COL_ENDING_TITLE),
        Align::Center,
    );
    r.draw_text(
        coda,
        0.5,
        0.455,
        0.029,
        fade(COL_ENDING_BODY),
        Align::Center,
    );

    let fuel = fuel_liters.max(0.0).min(9999.0);
    r.draw_text(
        &format!("{total_kills:02} GEGNER   ·   {fuel:03.0} L   ·   {scars:02} NARBEN"),
        0.5,
        0.620,
        0.024,
        fade(COL_ENDING_STONE),
        Align::Center,
    );
    r.draw_text_bold(
        "ENDE",
        0.5,
        0.795,
        0.027,
        fade(COL_ENDING_BODY),
        Align::Center,
    );
}

/// Terminal Act 3 defeat. Unlike the ordinary death screen this offers no retry memorial or heroic
/// score language: one factual line, then silence.
pub fn draw_final_defeat(r: &mut Renderer, reason: crate::campaign::FinalDefeatReason, alpha: f32) {
    r.set_ui_bg(BG_BLACK, BG_BLACK);
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * a];
    let detail = match reason {
        crate::campaign::FinalDefeatReason::TigerDestroyed => {
            "The engine burned out beside the forest road."
        }
        crate::campaign::FinalDefeatReason::CrewLost => "No one answered the intercom.",
        crate::campaign::FinalDefeatReason::TownOverrun => {
            "The town disappeared beneath the tracks."
        }
        crate::campaign::FinalDefeatReason::BreakoutCutOff => "The road west closed before dawn.",
    };

    r.draw_text(
        "HALBE · APRIL 1945",
        0.5,
        0.205,
        0.025,
        fade(COL_ENDING_STONE),
        Align::Center,
    );
    r.draw_text(
        "†",
        0.5,
        0.290,
        0.058,
        fade(COL_ENDING_STONE),
        Align::Center,
    );
    r.draw_text_bold(
        "The breakout ended here.",
        0.5,
        0.410,
        0.047,
        fade(COL_ENDING_TITLE),
        Align::Center,
    );
    r.draw_text(
        detail,
        0.5,
        0.505,
        0.029,
        fade(COL_ENDING_BODY),
        Align::Center,
    );
    r.draw_text_bold(
        "ENDE",
        0.5,
        0.745,
        0.027,
        fade(COL_ENDING_STONE),
        Align::Center,
    );
}

// ── In-game navigation: physical paper map + pocket compass (no waypoints) ───────────────────────

const COL_COMPASS: [f32; 4] = [0.92, 0.94, 0.97, 0.85];
const COL_COMPASS_DIM: [f32; 4] = [0.60, 0.65, 0.72, 0.55];

/// Legacy isolated UI diagnostic for compass typography. Live play never calls this: heading exists
/// only on the pocket compass resting on the held map.
pub fn draw_compass(r: &mut Renderer, heading_deg: f32) {
    const PTS: [(&str, f32); 8] = [
        ("N", 0.0),
        ("NE", 45.0),
        ("E", 90.0),
        ("SE", 135.0),
        ("S", 180.0),
        ("SW", 225.0),
        ("W", 270.0),
        ("NW", 315.0),
    ];
    for (label, ang) in PTS {
        let mut d = ang - heading_deg;
        while d > 180.0 {
            d -= 360.0;
        }
        while d < -180.0 {
            d += 360.0;
        }
        if d.abs() > 60.0 {
            continue;
        }
        let x = 0.5 + d / 60.0 * 0.15;
        let near = d.abs() < 8.0;
        r.draw_text_bold(
            label,
            x,
            0.030,
            if near { 0.034 } else { 0.024 },
            if near { COL_COMPASS } else { COL_COMPASS_DIM },
            Align::Center,
        );
    }
    // centre caret + numeric bearing under the ribbon (Wehrmacht convention would be mils; degrees v1)
    r.draw_text("·", 0.5, 0.008, 0.026, COL_COMPASS, Align::Center);
    r.draw_text(
        &format!("{:03.0}", heading_deg.rem_euclid(360.0)),
        0.5,
        0.068,
        0.022,
        COL_COMPASS_DIM,
        Align::Center,
    );
}

/// The PAPER MAP (N2, toggled with M): a Messtischblatt-toned sheet with the act's landmarks, the
/// printed spawn cross and the hand-inked red X at the goal. NO live player marker — you place
/// yourself by compass + landmarks (the whole navigation design). Fullscreen, game paused.
/// `marks` = the scout loop's hand-inked enemy sightings (position snapshots at marking time —
/// deliberately stale if the enemy has since moved; recon ages).
pub fn draw_map(r: &mut Renderer, act: &crate::world::ActMap, marks: &[glam::Vec2]) {
    // Warm survey-sheet paper, slightly darker at the bottom edge.
    r.set_ui_bg([0.86, 0.82, 0.69, 1.0], [0.78, 0.73, 0.60, 1.0]);
    let ink = [0.20, 0.18, 0.15, 1.0];
    let faded = [0.42, 0.39, 0.33, 1.0];
    let red = [0.62, 0.10, 0.08, 1.0];

    r.draw_text_bold(
        &format!("MAP — {}", act.name),
        0.5,
        0.040,
        0.046,
        ink,
        Align::Center,
    );
    r.draw_text(act.caption, 0.5, 0.100, 0.024, faded, Align::Center);

    // Plan projection: world x (east) → right, world z (south) → down, centred on the act origin.
    let half = act.half_extent_m.max(1.0);
    let proj =
        |wx: f32, wz: f32| -> (f32, f32) { (0.5 + wx / half * 0.28, 0.535 + wz / half * 0.335) };

    for lm in &act.landmarks {
        let (x, y) = proj(lm.pos.x, lm.pos.y);
        r.draw_text_bold("•", x, y, 0.040, ink, Align::Center);
        r.draw_text(&lm.name, x, y + 0.042, 0.021, faded, Align::Center);
    }
    let (sx, sy) = proj(act.spawn.x, act.spawn.y);
    r.draw_text_bold("+", sx, sy, 0.042, ink, Align::Center);
    r.draw_text("Start", sx, sy + 0.040, 0.019, faded, Align::Center);
    let (gx, gy) = proj(act.goal.x, act.goal.y);
    r.draw_text_bold("X", gx, gy, 0.055, red, Align::Center);

    // Scouted enemy positions — smaller red pencil x's, distinct from the big goal X.
    for m in marks {
        let (mx, my) = proj(m.x, m.y);
        r.draw_text_bold("x", mx, my, 0.030, red, Align::Center);
    }

    // Margins: north arrow, scale note, close hint — survey-sheet furniture.
    r.draw_text_bold("N", 0.055, 0.050, 0.038, ink, Align::Center);
    r.draw_text("|", 0.055, 0.092, 0.030, ink, Align::Center);
    r.draw_text(
        &format!("sheet width {:.1} km", act.half_extent_m * 2.0 / 1000.0),
        0.945,
        0.930,
        0.021,
        faded,
        Align::Right,
    );
    r.draw_text("M to close", 0.055, 0.930, 0.021, faded, Align::Left);
}

/// Navigation as an embodied, hands-busy view: a folded survey sheet occupies the lower part of
/// the frame while the dangerous world remains visible above it. The compass is a separate physical
/// object resting on the paper and exists only during this view. `pencil` points are normalized to
/// the paper's drawable rectangle and are intentionally append-only: there is no eraser mechanic.
pub fn draw_held_map(
    r: &mut Renderer,
    act: &crate::world::ActMap,
    scout_marks: &[glam::Vec2],
    pencil: &[(u8, glam::Vec2)],
    cursor: glam::Vec2,
    heading_deg: f32,
    pencil_down: bool,
) {
    let (mx, my, mw, mh) = (0.17f32, 0.34f32, 0.66f32, 0.59f32);
    let paper = [0.78, 0.73, 0.60, 0.98];
    let paper_hi = [0.88, 0.84, 0.71, 0.98];
    let ink = [0.18, 0.16, 0.13, 0.95];
    let faded = [0.36, 0.33, 0.27, 0.90];
    let graphite = [0.11, 0.12, 0.12, 0.82];
    let red = [0.58, 0.08, 0.065, 0.95];

    // A shadow and slightly stepped edges make this read as a sheet held in space, not a menu.
    r.fill_rect(
        mx - 0.014,
        my + 0.018,
        mw + 0.028,
        mh + 0.025,
        [0.0, 0.0, 0.0, 0.68],
    );
    r.fill_rect(mx, my, mw, mh, paper);
    r.fill_rect(mx + 0.012, my + 0.012, mw - 0.024, 0.012, paper_hi);
    r.fill_rect(mx + mw * 0.495, my, 0.007, mh, [0.46, 0.42, 0.33, 0.28]);
    r.fill_rect(mx, my + mh * 0.53, mw, 0.006, [0.47, 0.42, 0.33, 0.22]);

    r.draw_text_bold(act.name, mx + 0.035, my + 0.025, 0.026, ink, Align::Left);
    r.draw_text(
        &format!(
            "BLATT {} · {:.1} km",
            act.id,
            act.half_extent_m * 2.0 / 1000.0
        ),
        mx + mw - 0.035,
        my + 0.030,
        0.017,
        faded,
        Align::Right,
    );

    let half = act.half_extent_m.max(1.0);
    let proj = |wx: f32, wz: f32| -> (f32, f32) {
        (
            mx + mw * (0.50 + wx / half * 0.39),
            my + mh * (0.51 + wz / half * 0.38),
        )
    };
    for lm in &act.landmarks {
        let (x, y) = proj(lm.pos.x, lm.pos.y);
        r.draw_text_bold("·", x, y, 0.026, ink, Align::Center);
        let glyph = lm.kind.map_glyph().to_string();
        r.draw_text_bold(&glyph, x, y + 0.021, 0.015, faded, Align::Center);
    }
    let (sx, sy) = proj(act.spawn.x, act.spawn.y);
    r.draw_text_bold("+", sx, sy, 0.030, ink, Align::Center);
    let (gx, gy) = proj(act.goal.x, act.goal.y);
    r.draw_text_bold("X", gx, gy, 0.038, red, Align::Center);
    for mark in scout_marks {
        let (x, y) = proj(mark.x, mark.y);
        r.draw_text_bold("x", x, y, 0.022, red, Align::Center);
    }

    // Player graphite: thousands of tiny stable points give a satisfyingly scratchy pencil line.
    for (_, p) in pencil.iter().filter(|(id, _)| *id == act.id) {
        r.fill_rect(
            mx + p.x * mw - 0.0016,
            my + p.y * mh - 0.0016,
            0.0032,
            0.0032,
            graphite,
        );
    }

    // Pocket compass laid on the lower-right fold. North rotates relative to the held sheet as the
    // player turns; no heading ribbon or numeric HUD survives outside this physical object.
    let cc = glam::Vec2::new(mx + mw * 0.82, my + mh * 0.76);
    let cr = 0.064f32;
    r.fill_rect(
        cc.x - cr,
        cc.y - cr,
        cr * 2.0,
        cr * 2.0,
        [0.055, 0.055, 0.050, 0.92],
    );
    r.fill_rect(
        cc.x - cr * 0.82,
        cc.y - cr * 0.82,
        cr * 1.64,
        cr * 1.64,
        [0.68, 0.63, 0.48, 0.96],
    );
    for i in 0..24 {
        let a = i as f32 / 24.0 * std::f32::consts::TAU;
        r.fill_rect(
            cc.x + a.sin() * cr * 0.69 - 0.002,
            cc.y - a.cos() * cr * 0.69 - 0.002,
            0.004,
            0.004,
            ink,
        );
    }
    let na = -heading_deg.to_radians();
    for i in -7..=7 {
        let t = i as f32 / 7.0 * cr * 0.55;
        let col = if i < 0 { [0.15, 0.15, 0.14, 0.95] } else { red };
        r.fill_rect(
            cc.x + na.sin() * t - 0.0025,
            cc.y - na.cos() * t - 0.0025,
            0.005,
            0.005,
            col,
        );
    }
    r.draw_text_bold("N", cc.x, cc.y - cr * 0.82, 0.016, ink, Align::Center);

    // Relative-mouse pencil cursor and the pencil itself. LMB lays graphite; nothing removes it.
    let cp = glam::Vec2::new(mx + cursor.x * mw, my + cursor.y * mh);
    r.fill_rect(
        cp.x - 0.003,
        cp.y - 0.003,
        0.006,
        0.006,
        if pencil_down { graphite } else { faded },
    );
    r.fill_rect(
        cp.x + 0.006,
        cp.y + 0.008,
        0.008,
        0.105,
        [0.54, 0.30, 0.12, 0.98],
    );
    r.fill_rect(
        cp.x + 0.007,
        cp.y + 0.005,
        0.006,
        0.016,
        [0.19, 0.17, 0.13, 0.98],
    );

    // Gloved hands occlude the lower corners and sell the held scale.
    r.fill_rect(
        mx - 0.025,
        my + mh - 0.045,
        0.15,
        0.11,
        [0.035, 0.038, 0.035, 0.98],
    );
    r.fill_rect(
        mx + mw - 0.125,
        my + mh - 0.045,
        0.15,
        0.11,
        [0.035, 0.038, 0.035, 0.98],
    );
    r.draw_text(
        "M: FALTEN   LMB: BLEISTIFT   KEIN RADIERGUMMI",
        0.5,
        0.955,
        0.017,
        [0.82, 0.80, 0.70, 0.82],
        Align::Center,
    );
}

/// The between-acts title card (W5): pure black, the act number, its German title and the situation
/// caption, faded in by `alpha`. Doubles as the loading screen while the act's world builds.
pub fn draw_act_card(r: &mut Renderer, act: &crate::world::ActMap, alpha: f32) {
    r.set_ui_bg(BG_BLACK, BG_BLACK);
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * a];
    r.draw_text(
        &format!("ACT {}", act.id),
        0.5,
        0.32,
        0.034,
        fade(COL_SUBTITLE),
        Align::Center,
    );
    r.draw_text_bold(act.name, 0.5, 0.40, 0.095, fade(COL_TITLE), Align::Center);
    r.draw_text(act.caption, 0.5, 0.56, 0.030, fade(COL_ITEM), Align::Center);
    r.draw_text(
        &format!(
            "{} · {}",
            act.season.label(),
            if act.final_battle {
                "The last battle — you get exactly one attempt"
            } else {
                "press any key"
            }
        ),
        0.5,
        0.66,
        0.024,
        fade(COL_HINT),
        Align::Center,
    );
}

/// The in-game controls reference (toggled with the Home / Pos1 key while playing). Two columns of
/// key → action, all German, on the cold menu backdrop. Reuses the pixelated UI path.
pub fn draw_controls(r: &mut Renderer) {
    r.set_ui_bg(BG_MENU_TOP, BG_MENU_BOTTOM);
    r.draw_text_bold("Controls", 0.5, 0.06, 0.058, COL_TITLE, Align::Center);

    let row = |r: &mut Renderer, kx: f32, y: f32, key: &str, action: &str| {
        r.draw_text_bold(key, kx, y, 0.026, COL_SELECT, Align::Right);
        r.draw_text(action, kx + 0.015, y - 0.004, 0.040, COL_ITEM, Align::Left);
    };
    let header = |r: &mut Renderer, hx: f32, y: f32, t: &str| {
        r.draw_text_bold(t, hx, y, 0.030, COL_SUBTITLE, Align::Left);
    };

    // ── Left column: stations + fighting. THE RULES: LMB = your station's weapon or your
    // hands; RMB = look closer; E = interact with the tank's body. ──
    let (lk, lh) = (0.30, 0.055);
    header(r, lh, 0.17, "STATIONS");
    row(
        r,
        lk,
        0.230,
        "E / LMB",
        "enter the station you are LOOKING at",
    );
    row(r, lk, 0.275, "E", "hatch lever - climb - mount - tools");
    row(r, lk, 0.320, "Q / Space", "off the sights - on your feet");
    row(r, lk, 0.365, "X tap / hold", "head out / 1.5 s: dismount");
    header(r, lh, 0.44, "FIGHTING");
    row(r, lk, 0.500, "W S A D", "drive - from every view");
    row(r, lk, 0.545, "LMB", "fire: 88 (sights) / MG / Luger");
    row(r, lk, 0.590, "RMB", "look closer: zoom - binoculars - aim");
    row(r, lk, 0.635, "C hold", "coax MG (sights)");
    row(r, lk, 0.680, "Z", "engine off / crank - silent watch");

    // ── Right column: inside work + system ──
    let (rk, rh) = (0.78, 0.575);
    header(r, rh, 0.17, "LOADER'S HANDS");
    row(r, rk, 0.230, "E", "equip the round under the crosshair");
    row(r, rk, 0.275, "LMB + mouse", "lever: down opens - up closes");
    row(r, rk, 0.320, "fast forward", "LMB at breech: ram the round");
    row(r, rk, 0.365, "F / G", "extinguish / repair");
    header(r, rh, 0.44, "SYSTEM");
    row(
        r,
        rk,
        0.500,
        "M / LMB",
        "held map+compass / pencil (no erase)",
    );
    row(r, rk, 0.545, "Ctrl", "crouch (on foot)");
    row(r, rk, 0.590, "[ ]  F12", "retro - screenshot");
    row(r, rk, 0.635, "F6 (LAN)", "claim next open crew station");
    row(r, rk, 0.680, "Esc", "back to title");

    r.draw_text_bold(
        "CONTROLLER  Xbox / PlayStation",
        0.5,
        0.755,
        0.028,
        COL_SUBTITLE,
        Align::Center,
    );
    r.draw_text(
        "LS drive · RS look/lay · RT action/fire · LT aim/zoom · A interact · B back",
        0.5,
        0.805,
        0.026,
        COL_ITEM,
        Align::Center,
    );
    r.draw_text(
        "Y tap head out / hold dismount · X reload · RB coax · View map",
        0.5,
        0.845,
        0.024,
        COL_ITEM,
        Align::Center,
    );
    r.draw_text(
        "Home / Menu to close",
        0.5,
        0.925,
        0.032,
        COL_HINT,
        Align::Center,
    );
}
