//! Panzerrechnersystem front-end: the GERMAN title screen, menu, and death screen. The engine had no text before
//! this — here we bake a glyph coverage atlas from VT323 and Press Start 2P (bundled under
//! `assets/fonts/`) with the pure-Rust `ab_glyph` rasterizer, and draw text as textured quads at
//! DISPLAY resolution, composited over the final blit (OUTSIDE the PS1 crush) so the UI stays crisp.
//!
//! `Ui` owns the atlas + the two `text.wgsl` pipelines and is held by the `Renderer`; the front-end
//! calls `Renderer::draw_text` / `draw_text_bold` to queue strings, then one of the `render_ui_*`
//! entry points flushes them. The menu labels and the layout functions `draw_menu` /
//! `draw_pause_menu` live in this file and are shared by the live window and the headless `ui`
//! subcommand. Play itself carries no words (2026-09-28): death, the campaign codas and the
//! sector-change card are `draw_black`, and the held map is marks and symbols only.

use std::collections::HashMap;
use std::net::SocketAddr;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use wgpu::util::DeviceExt;

use crate::graphics::GraphicsPreset;
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

/// The chosen play mode. Display text is English; stored enum identifiers remain compatible.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayMode {
    /// The DEV GARAGE gunnery yard — the default boot scene (world::garage()).
    Garage,
    Kampagne,
    /// The Act One campaign mission with its one crewman, skipping the intro and tutorial.
    Playtest,
    Gefecht,
    /// Two-player local crew co-op on the selected Act One map, split top and bottom.
    /// Device ownership is selected on the crew setup page. The Radio/Bow-MG post is omitted.
    Koop,
    /// Direct load of the act-2 map (Oderbruch spring) — a map-test entry, no act flow.
    TestAct2,
    /// Direct load of the act-3 map (Halbe) — a map-test entry, no act flow.
    TestAct3,
    /// The CANYON playtest course (src/canyon.rs): one crewman punches through a defended ravine.
    Canyon,
}

// Menu item indices (order they appear on screen). Keep the campaign alias for existing callers.
pub const IDX_SINGLEPLAYER: usize = 0;
pub const IDX_KAMPAGNE: usize = IDX_SINGLEPLAYER;
pub const IDX_COOP: usize = 1;
pub const IDX_PLAYTEST: usize = 2;
pub const IDX_EDITOR: usize = 3;
pub const IDX_LOAD_CHECKPOINT: usize = 4;
pub const IDX_CANYON: usize = 5;
pub const MENU_LABELS: [&str; 8] = [
    "Singleplayer", "Co-op", "Act One Playtest", "Editor", "Load checkpoint", "Canyon Run", "Canyon Run v2", "Graphics",
];
pub const MENU_COUNT: usize = MENU_LABELS.len();
const MENU_ROW_TOP: f32 = 0.485;
const MENU_ROW_STEP: f32 = 0.049;
const MENU_ROW_LEFT: f32 = 0.20;
const MENU_ROW_RIGHT: f32 = 0.80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Singleplayer,
    Coop,
    Playtest,
    Editor,
    LoadCheckpoint,
    Canyon,
    CanyonV2,
    Graphics,
}

const MENU_ACTIONS: [MenuAction; MENU_COUNT] = [
    MenuAction::Singleplayer, MenuAction::Coop, MenuAction::Playtest,
    MenuAction::Editor, MenuAction::LoadCheckpoint, MenuAction::Canyon, MenuAction::CanyonV2, MenuAction::Graphics,
];

// The crew co-op page keeps its first four actions/settings fixed, follows them with nearby
// Tigers are currently advertising, then ends with Back. Keeping this mapping pure makes a browse
// list that changes under us deterministic for keyboard and controller navigation.
pub const LAN_IDX_HOST: usize = 0;
pub const LAN_IDX_MANUAL: usize = 1;
pub const LAN_IDX_SPLITSCREEN: usize = 2;
pub const LAN_IDX_COUCH_DEVICES: usize = 3;
pub const LAN_SESSION_START: usize = 4;

/// Which physical devices own the two local views. The role protocol still supports four crew
/// posts; the live co-op runtime admits one keyboard and one pad, or two pads. This selection is
/// copied into the run so revisiting the setup page cannot change authority in-place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CouchInputMode {
    #[default]
    KeyboardAndControllers,
    ControllersOnly,
}

impl CouchInputMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::KeyboardAndControllers => "Keyboard + one controller",
            Self::ControllersOnly => "Two controllers",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::KeyboardAndControllers => Self::ControllersOnly,
            Self::ControllersOnly => Self::KeyboardAndControllers,
        }
    }

    pub fn includes_keyboard(self) -> bool {
        self == Self::KeyboardAndControllers
    }

    pub fn controller_limit(self) -> usize {
        if self.includes_keyboard() {
            3
        } else {
            4
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanMenuSelection {
    Host,
    ManualAddress,
    SplitScreen,
    CouchDevices,
    Session(usize),
    Back,
}

#[derive(Clone, Debug)]
pub struct LanMenuModel {
    pub selected: usize,
    pub mission: LanMission,
    pub couch_input: CouchInputMode,
    pub manual_address: String,
    pub editing_address: bool,
    pub status: String,
}

impl Default for LanMenuModel {
    fn default() -> Self {
        Self {
            selected: LAN_IDX_HOST,
            mission: LanMission::Campaign,
            couch_input: CouchInputMode::default(),
            manual_address: String::new(),
            editing_address: false,
            status: "Hosts appear automatically. Press R to retry discovery.".to_owned(),
        }
    }
}

impl LanMenuModel {
    pub fn visible_session_start(&self, count: usize) -> usize {
        if count <= 4 { 0 }
        else if matches!(self.selection(count), LanMenuSelection::Back) { count-4 }
        else {
            let selected = match self.selection(count) { LanMenuSelection::Session(i) => i, _ => 0 };
            selected.saturating_sub(1).min(count-4)
        }
    }
    pub fn row_at(&self, x:f32, y:f32, session_count:usize) -> Option<usize> {
        if !(0.065..=0.94).contains(&x) { return None; }
        for (row,top) in [(LAN_IDX_HOST,0.325),(LAN_IDX_MANUAL,0.390),
            (LAN_IDX_SPLITSCREEN,0.443),(LAN_IDX_COUCH_DEVICES,0.495),
            (Self::back_index(session_count),0.840)] {
            if (top..top+0.045).contains(&y) { return Some(row); }
        }
        let start=self.visible_session_start(session_count);
        (start..(start+4).min(session_count)).enumerate().find_map(|(row,index)| {
            let top=0.595+row as f32*0.055;
            (top..top+0.045).contains(&y).then_some(LAN_SESSION_START+index)
        })
    }
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
        } else if self.selected == LAN_IDX_COUCH_DEVICES {
            LanMenuSelection::CouchDevices
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

    pub fn toggle_couch_input(&mut self) {
        self.couch_input = self.couch_input.toggled();
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
    fn couch_device_selector_changes_the_dynamic_pad_limit() {
        let mut menu = LanMenuModel::default();
        assert_eq!(menu.couch_input, CouchInputMode::KeyboardAndControllers);
        assert_eq!(menu.couch_input.controller_limit(), 3);
        menu.selected = LAN_IDX_COUCH_DEVICES;
        assert_eq!(menu.selection(0), LanMenuSelection::CouchDevices);
        menu.toggle_couch_input();
        assert_eq!(menu.couch_input, CouchInputMode::ControllersOnly);
        assert_eq!(menu.couch_input.controller_limit(), 4);
        assert!(!menu.couch_input.includes_keyboard());
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
    pub editor_page: bool,
    pub time: TimeOfDay,
    /// Invincible toggle: the run can't kill you (Game::god). For sightseeing and testing.
    pub invincible: bool,
    /// This machine's renderer budget. It is session-local and deliberately never serialized into
    /// LAN/campaign state; every peer is free to render the shared simulation differently.
    pub graphics: GraphicsPreset,
    /// True when this menu was opened from a live game. Escape then returns to that exact run
    /// instead of discarding campaign state or rebuilding the world.
    pub resume_available: bool,
    pub checkpoint_available: bool,
    /// The Canyon entry's line: the last and the best run's time (seconds), none before any run.
    pub canyon_times: Option<(f32, f32)>,
}

impl Default for MenuModel {
    fn default() -> Self {
        MenuModel {
            selected: IDX_KAMPAGNE,
            editor_page: false,
            time: TimeOfDay::Tag,
            invincible: false,
            graphics: GraphicsPreset::High,
            resume_available: false,
            checkpoint_available: false,
            canyon_times: None,
        }
    }
}

impl MenuModel {
    pub fn row_count(&self) -> usize {
        if self.editor_page { EDITOR_LABELS.len() }
        else { title_row_count(crate::graphics::fixed_playtest_graphics()) }
    }
    pub fn action(&self) -> Option<MenuAction> {
        if self.editor_page || self.selected >= self.row_count() { None }
        else { MENU_ACTIONS.get(self.selected).copied() }
    }
    pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        if !(MENU_ROW_LEFT..=MENU_ROW_RIGHT).contains(&x) { return None; }
        let (start, step) = if self.editor_page { (0.40, 0.067) } else { (MENU_ROW_TOP, MENU_ROW_STEP) };
        (0..self.row_count()).find(|&row| {
            let top = start + row as f32 * step;
            (top..top + step - 0.005).contains(&y)
        })
    }
}

fn title_row_count(fixed_graphics: bool) -> usize {
    MENU_COUNT - usize::from(fixed_graphics)
}

#[cfg(test)]
mod title_menu_tests {
    use super::*;

    #[test]
    fn launch_buttons_have_distinct_actions_and_click_regions() {
        let mut menu = MenuModel::default();
        assert_eq!(menu.action(), Some(MenuAction::Singleplayer));
        let expected = [
            MenuAction::Singleplayer, MenuAction::Coop, MenuAction::Playtest,
            MenuAction::Editor, MenuAction::LoadCheckpoint, MenuAction::Canyon, MenuAction::CanyonV2, MenuAction::Graphics,
        ];
        for (index, action) in expected.into_iter().enumerate() {
            let y = MENU_ROW_TOP + (index as f32 + 0.5) * MENU_ROW_STEP;
            menu.selected = menu.row_at(0.5, y).expect("visible row is clickable");
            assert_eq!(menu.selected, index);
            assert_eq!(menu.action(), Some(action));
        }
        assert_eq!(menu.row_at(MENU_ROW_LEFT - 0.01, MENU_ROW_TOP + 0.02), None);
        assert_eq!(menu.row_at(0.5, 0.95), None);
        menu.selected = MENU_COUNT;
        assert_eq!(menu.action(), None);
    }

    #[test]
    fn editor_rows_cannot_dispatch_a_title_launch_action() {
        let mut menu = MenuModel { editor_page: true, ..MenuModel::default() };
        for row in 0..EDITOR_LABELS.len() {
            let y = 0.40 + (row as f32 + 0.5) * 0.067;
            menu.selected = menu.row_at(0.5, y).expect("editor row is clickable");
            assert_eq!(menu.selected, row);
            assert_eq!(menu.action(), None);
        }
    }
}

pub const EDITOR_LABELS: [&str; 7] = [
    "Act One world", "Tiger I garage", "Tiger II garage", "IS-2 garage",
    "T-34/76 garage", "Panzer IV garage", "Back",
];
pub const EDITOR_TARGETS: [&str; 6] = ["world", "tiger_i", "tiger_ii", "is2", "t34_76", "panzer_iv"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PausePage {
    #[default]
    Main,
    Settings,
    Display,
    Graphics,
    ConfirmExit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseAction {
    Resume, RestartGame, MainMenu, SaveCheckpoint, LoadCheckpoint, Settings, ExitGame,
}

impl PauseAction {
    pub const ALL: [Self; 7] = [Self::Resume, Self::RestartGame, Self::MainMenu,
        Self::SaveCheckpoint, Self::LoadCheckpoint, Self::Settings, Self::ExitGame];

    fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume", Self::RestartGame => "Restart Game",
            Self::MainMenu => "Main Menu", Self::SaveCheckpoint => "Save checkpoint",
            Self::LoadCheckpoint => "Load checkpoint", Self::Settings => "Settings",
            Self::ExitGame => "Exit game",
        }
    }
}

#[derive(Default)]
pub struct PauseMenuModel {
    pub page: PausePage,
    pub selected: usize,
    pub message: String,
}

impl PauseMenuModel {
    pub fn row_count(&self) -> usize {
        match self.page {
            PausePage::Main => PauseAction::ALL.len(),
            PausePage::Settings if crate::graphics::fixed_playtest_graphics() => 4,
            PausePage::Settings | PausePage::Display | PausePage::Graphics => 6,
            PausePage::ConfirmExit => 2,
        }
    }

    /// Preserve the action IDs used by the input handler after hiding display/graphics pages.
    pub fn action_row(&self) -> usize {
        pause_action_row(self.page, self.selected, crate::graphics::fixed_playtest_graphics())
    }

    pub fn action(&self) -> Option<PauseAction> {
        (self.page == PausePage::Main).then(|| PauseAction::ALL.get(self.selected).copied()).flatten()
    }

    fn row_top(&self, row: usize) -> f32 {
        0.30 + row as f32 * if self.page == PausePage::Main { 0.064 } else { 0.085 }
    }

    pub fn move_selection(&mut self, forward: bool) {
        let count = self.row_count();
        self.selected = (self.selected + if forward { 1 } else { count - 1 }) % count;
    }

    pub fn show(&mut self, page: PausePage) {
        self.page = if crate::graphics::fixed_playtest_graphics()
            && matches!(page, PausePage::Display | PausePage::Graphics) {
            PausePage::Settings
        } else { page };
        self.selected = 0;
        self.message.clear();
    }

    pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        if !(0.20..=0.80).contains(&x) {
            return None;
        }
        (0..self.row_count()).find(|&row| {
            let top = self.row_top(row);
            let height = if self.page == PausePage::Main { 0.055 } else { 0.07 };
            (top..top + height).contains(&y)
        })
    }
}

fn pause_action_row(page: PausePage, row: usize, fixed_graphics: bool) -> usize {
    if page == PausePage::Settings && fixed_graphics { row + 2 } else { row }
}

#[cfg(test)]
mod pause_menu_tests {
    use super::*;

    #[test]
    fn fixed_profile_removes_only_graphics_routes_and_preserves_setting_actions() {
        assert_eq!(title_row_count(true), 7);
        assert_eq!(title_row_count(false), 8);
        assert!(!MENU_ACTIONS[..title_row_count(true)].contains(&MenuAction::Graphics));
        for row in 0..4 {
            assert_eq!(pause_action_row(PausePage::Settings, row, true), row + 2);
            assert_eq!(pause_action_row(PausePage::Settings, row, false), row);
            assert_eq!(pause_action_row(PausePage::Main, row, true), row);
        }
    }

    #[test]
    fn pause_main_buttons_share_mouse_keyboard_actions_without_overlapping_details() {
        let mut menu = PauseMenuModel::default();
        for (row, action) in PauseAction::ALL.into_iter().enumerate() {
            assert_eq!(menu.row_at(0.5, menu.row_top(row) + 0.027), Some(row));
            assert_eq!(menu.action(), Some(action));
            assert!(menu.row_top(row) + 0.055 < 0.80);
            assert_eq!(menu.row_at(0.5, menu.row_top(row) + 0.060), None);
            menu.move_selection(true);
        }
        assert_eq!(menu.selected, 0);
        menu.move_selection(false);
        assert_eq!(menu.action(), Some(PauseAction::ExitGame));
        assert_eq!(PauseAction::RestartGame.label(), "Restart Game");
        assert_eq!(PauseAction::MainMenu.label(), "Main Menu");
    }

    #[test]
    fn pause_subpages_retain_their_hitboxes_and_cannot_dispatch_main_actions() {
        let mut menu = PauseMenuModel::default();
        for page in [PausePage::Settings, PausePage::Display, PausePage::ConfirmExit] {
            menu.show(page);
            for row in 0..menu.row_count() {
                menu.selected = menu.row_at(0.5, 0.335 + row as f32 * 0.085).unwrap();
                assert_eq!(menu.selected, row);
                assert_eq!(menu.action(), None);
            }
        }
    }
}

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
    _tex: Option<wgpu::Texture>,
    _imported_view: Option<wgpu::TextureView>,
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
        // Linear-clamp samples the high-resolution coverage at the UI's output resolution.
        // `text.wgsl::fs_text` then thresholds coverage for crisp pixel-font edges. UI resolution
        // is independent of the 3D scene grid so small labels retain enough pixels for each stroke.
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
        self.images.push(ImageTex { bind, _tex: Some(tex), _imported_view: None });
        (self.images.len() - 1) as u32
    }

    /// Retain an existing live diagnostic view. A dedicated nearest sampler keeps its logical
    /// pixels sharp; ordinary thumbnails keep their existing image sampler and appearance.
    pub fn import_nearest_image_view(&mut self, device: &wgpu::Device, view: &wgpu::TextureView) -> u32 {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("diagnostic-image-nearest"),
            mag_filter: wgpu::FilterMode::Nearest, min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest, ..Default::default()
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("diagnostic-image-bind"), layout: &self.img_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        self.images.push(ImageTex { bind, _tex: None, _imported_view: Some(view.clone()) });
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

    /// Cache key for the held paper's queued ink. Does not include camera/world transforms:
    /// those move the actual mesh and compass, without forcing another albedo render.
    pub(crate) fn queued_content_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut key = std::collections::hash_map::DefaultHasher::new();
        self.cmds.len().hash(&mut key);
        self.img_cmds.len().hash(&mut key);
        for cmd in &self.cmds {
            cmd.text.hash(&mut key);
            for value in [cmd.x, cmd.y, cmd.size].into_iter().chain(cmd.color) {
                value.to_bits().hash(&mut key);
            }
            (cmd.align as u8).hash(&mut key);
            cmd.bold.hash(&mut key);
        }
        for cmd in &self.img_cmds {
            cmd.handle.hash(&mut key);
            for value in cmd.rect.into_iter().chain(cmd.color) { value.to_bits().hash(&mut key); }
        }
        key.finish()
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

/// Compose the title screen: the `Panzerrechnersystem` logo, the mode/setting list with the
/// current selection highlighted in amber, and a control hint. Queues into the renderer; the caller
/// flushes with a `render_ui_*` entry point.
pub fn draw_menu(r: &mut Renderer, m: &MenuModel) {
    // The driver's actual forward viewport supplies the image. Quiet type lives in the dark
    // upper/lower armor bands; it never competes with the daylight slit or adds a fake window.
    r.set_ui_bg(BG_BLACK, BG_BLACK);
    r.draw_text_bold("Panzerrechnersystem", 0.50, 0.105, 0.066, COL_TITLE, Align::Center);
    if m.editor_page {
        r.draw_text_bold("WORLD & TANK EDITORS", 0.50, 0.30, 0.030, COL_ITEM, Align::Center);
        for (i, label) in EDITOR_LABELS.iter().enumerate() {
            let text = if i == m.selected { format!(">  {label}  <") } else { (*label).to_owned() };
            r.draw_text_bold(&text, 0.50, 0.41 + i as f32 * 0.067, 0.032,
                if i == m.selected { COL_SELECT } else { COL_ITEM }, Align::Center);
        }
        r.draw_text("ISOLATED GARAGES / ORBIT / FIRST-PERSON PLACEMENT", 0.50, 0.90, 0.020, COL_HINT, Align::Center);
        r.draw_text("MOUSE OR ARROWS / ENTER    ESC BACK", 0.50, 0.94, 0.021, COL_HINT, Align::Center);
        return;
    }
    for (i, label) in MENU_LABELS.iter().take(m.row_count()).enumerate() {
        let selected = i == m.selected;
        let text = if selected { format!(">  {label}  <") } else { (*label).to_owned() };
        let enabled = i != IDX_LOAD_CHECKPOINT || m.checkpoint_available;
        let top = MENU_ROW_TOP + i as f32 * MENU_ROW_STEP;
        // The two launch actions are visible buttons as well as keyboard/controller rows.
        // Their full rectangles use exactly the same bounds as mouse hit testing above.
        if matches!(i, IDX_SINGLEPLAYER | IDX_COOP) {
            let border = if selected { [0.30, 0.17, 0.045, 1.0] } else { [0.075, 0.08, 0.09, 1.0] };
            r.fill_rect(MENU_ROW_LEFT, top, MENU_ROW_RIGHT - MENU_ROW_LEFT, MENU_ROW_STEP - 0.005, border);
            r.fill_rect(MENU_ROW_LEFT + 0.002, top + 0.003,
                MENU_ROW_RIGHT - MENU_ROW_LEFT - 0.004, MENU_ROW_STEP - 0.011,
                [0.006, 0.007, 0.009, 1.0]);
        }
        r.draw_text_bold(&text, 0.50, top + 0.010, 0.031,
            if !enabled { COL_HINT } else if selected { COL_SELECT } else { COL_ITEM }, Align::Center);
    }
    let canyon_line: String;
    let detail = match m.action() {
        Some(MenuAction::Graphics) => "RESOLUTION / QUALITY / COLOR / RETRO",
        Some(MenuAction::Singleplayer) => "ONE PLAYER / ACT ONE CAMPAIGN",
        Some(MenuAction::Coop) => "TWO PLAYERS / TOP & BOTTOM / LOCAL CREW CO-OP",
        Some(MenuAction::Playtest) => "ACT ONE / ONE CREWMAN / SKIP INTRO & TUTORIAL",
        Some(MenuAction::Editor) => "BUILD WORLDS & TANK INTERIORS",
        Some(MenuAction::LoadCheckpoint) if m.checkpoint_available => "CONTINUE YOUR SAVED CAMPAIGN",
        Some(MenuAction::LoadCheckpoint) => "NO CAMPAIGN CHECKPOINT SAVED",
        Some(MenuAction::Canyon) => match m.canyon_times {
            Some((last, best)) => {
                canyon_line = format!("PERMADEATH    LAST {}    BEST {}", crate::canyon::format_time(last),
                    crate::canyon::format_time(best));
                canyon_line.as_str()
            }
            None => "ONE TIGER / ADAPTIVE DEFENCE / PERMADEATH",
        },
        Some(MenuAction::CanyonV2) => "POPULATED GAEA CANYON / ADAPTIVE DEFENCE / PERMADEATH",
        None => "",
    };
    r.draw_text(detail, 0.50, 0.905, 0.020, COL_HINT, Align::Center);
    let hint = if m.resume_available {
        "MOUSE OR ARROWS / ENTER    ESC RESUMES"
    } else {
        "MOUSE OR ARROWS / ENTER    ESC QUITS"
    };
    r.draw_text(hint, 0.50, 0.955, 0.021, COL_HINT, Align::Center);
}

pub fn draw_pause_menu(
    r: &mut Renderer,
    m: &PauseMenuModel,
    can_save: bool,
    can_load: bool,
    fullscreen: bool,
    _retro: f32,
    show_stats: bool,
    haptics_enabled: bool,
    playtest_return_fire: Option<bool>,
    display: &crate::display_preview::DisplayPreview,
) {
    r.set_ui_bg(BG_BLACK, BG_BLACK);
    let title = match m.page {
        PausePage::Main => "Paused",
        PausePage::Settings => "Settings",
        PausePage::Graphics => "Graphics",
        PausePage::Display => "Display",
        PausePage::ConfirmExit => "Exit game?",
    };
    r.draw_text_bold(title, 0.50, 0.115, 0.072, COL_TITLE, Align::Center);
    r.draw_text("THE SIMULATION IS PAUSED", 0.50, 0.205, 0.023, COL_HINT, Align::Center);
    let rows: Vec<(String, bool)> = match m.page {
        PausePage::Main => PauseAction::ALL.into_iter().map(|action| {
            let enabled = match action {
                PauseAction::SaveCheckpoint => can_save,
                PauseAction::LoadCheckpoint => can_load,
                _ => true,
            };
            (action.label().to_owned(), enabled)
        }).collect(),
        PausePage::Graphics => Vec::new(),
        PausePage::Settings => vec![
            ("Display / Adafruit previews".to_owned(), true),
            ("Graphics / quality and color".to_owned(), true),
            (format!("Show FPS counter: {}", if show_stats { "On" } else { "Off" }), true),
            (format!("Haptics: {}", if haptics_enabled { "On" } else { "Off" }), true),
            (format!("Enemy return fire: {}", match playtest_return_fire {
                Some(true) => "On", Some(false) => "Off", None => "Live combat",
            }), playtest_return_fire.is_some()),
            ("Back".to_owned(), true),
        ].into_iter().skip(if crate::graphics::fixed_playtest_graphics() { 2 } else { 0 }).collect(),
        PausePage::Display => {
            let mut rows = vec![(if display.active.is_some() { "Restore desktop window".to_owned() }
                else { format!("Desktop: {}", if fullscreen { "Fullscreen" } else { "Windowed" }) }, true)];
            for (index, panel) in crate::display_preview::PANELS.iter().enumerate() {
                rows.push((format!("{}{}", panel.label,
                    if display.active == Some(index) { " *" } else { "" }), true));
            }
            rows.push((format!("Physical size: {}%", display.percent), true));
            rows.push(("Back".to_owned(), true));
            rows
        },
        PausePage::ConfirmExit => vec![("Keep playing".to_owned(), true), ("Exit to desktop".to_owned(), true)],
    };
    for (index, (label, enabled)) in rows.into_iter().enumerate() {
        let selected = index == m.selected;
        let text = if selected { format!(">  {label}  <") } else { label };
        let color = if !enabled { COL_HINT } else if selected { COL_SELECT } else { COL_ITEM };
        r.draw_text_bold(&text, 0.50, m.row_top(index) + 0.01, 0.038, color, Align::Center);
    }
    let detail = if !m.message.is_empty() {
        m.message.as_str()
    } else {
        match m.page {
            PausePage::Main if m.action() == Some(PauseAction::RestartGame) =>
                "RESTART THIS SESSION FROM ITS ENTRANCE / SAME MODE AND MAP",
            PausePage::Main if m.action() == Some(PauseAction::MainMenu) =>
                "END THIS SESSION AND RETURN TO THE TITLE",
            PausePage::Main if m.action() == Some(PauseAction::SaveCheckpoint) && !can_save =>
                "CHECKPOINTS ARE AVAILABLE IN CAMPAIGN AND ACT ONE PLAYTEST",
            PausePage::Main if m.action() == Some(PauseAction::LoadCheckpoint) && !can_load =>
                "NO SAVED CAMPAIGN CHECKPOINT",
            PausePage::Main if matches!(m.action(), Some(PauseAction::SaveCheckpoint | PauseAction::LoadCheckpoint)) =>
                "CHECKPOINTS RESTART AT THE CURRENT SECTOR ENTRANCE",
            PausePage::Settings if m.action_row() == 2 =>
                "SHOWS FRAME RATE AND TIMING — GRAPHICS QUALITY IS UNCHANGED",
            PausePage::Settings if m.action_row() == 4 => match playtest_return_fire {
                Some(false) => "PRACTICE TARGETS — ENEMY WEAPONS ARE HELD",
                Some(true) => "LIVE FIRE — ENEMIES CAN SHOOT BACK",
                None => "ENEMY FIRE FOLLOWS THE CURRENT MISSION",
            },
            PausePage::Display if m.selected == 4 => "LEFT / RIGHT: CALIBRATE SIZE WITH A RULER",
            PausePage::Display if display.active.is_some() && !display.measured => "SIZE ESTIMATED - CALIBRATE WITH A RULER",
            PausePage::Display => "NATIVE PIXELS / ACTUAL SIZE / BORDERLESS",
            PausePage::Settings => "LEFT / RIGHT TO CHANGE A SETTING",
            PausePage::ConfirmExit => "PROGRESS SINCE THE SAVED CHECKPOINT WILL BE LOST",
            _ => "",
        }
    };
    r.draw_text(detail, 0.50, 0.80, 0.023, COL_SUBTITLE, Align::Center);
    r.draw_text("MOUSE OR ARROWS / ENTER    ESC BACK", 0.50, 0.93, 0.021, COL_HINT, Align::Center);
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
        "One Tiger. Local co-op: two players, top and bottom.",
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
        0.443,
        "2-player co-op",
        "top / bottom - enter starts local co-op",
    );
    draw_lan_row(
        r,
        m.selected == LAN_IDX_COUCH_DEVICES,
        0.495,
        "Local devices",
        &format!("‹ {} ›", m.couch_input.label()),
    );

    r.draw_text_bold(
        "JOIN A NEARBY TIGER",
        x,
        0.555,
        0.026,
        COL_SUBTITLE,
        Align::Left,
    );
    const VISIBLE: usize = 4;
    let start = m.visible_session_start(sessions.len());
    let end = (start + VISIBLE).min(sessions.len());
    if sessions.is_empty() {
        r.draw_text(
            "No host seen yet. Wait a moment, press R, or use Manual IP.",
            x + 0.028,
            0.602,
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
                0.595 + row as f32 * 0.055,
                &compact_label(&session.beacon.session_name, 24),
                &condition,
            );
        }
        if start > 0 {
            r.draw_text("more above", 0.40, 0.568, 0.019, COL_HINT, Align::Left);
        }
        if end < sessions.len() {
            r.draw_text("more below", 0.40, 0.818, 0.019, COL_HINT, Align::Left);
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

/// Death, the campaign codas and the sector-change card are black and wordless (decided
/// 2026-09-28). The game tells its story without a single written word; the old death memorial also
/// named real Panzer aces with an invented death year. Callers own the timing and the sound.
pub fn draw_black(r: &mut Renderer) {
    r.set_ui_bg(BG_BLACK, BG_BLACK);
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
fn map_landmark_label(name: &str) -> &str {
    // Translate display text only: authored marker names also locate source assets.
    match name {
        "Getreidesilo" => "Grain Elevator",
        "Springbrunnen" => "Fountain",
        "Kasaner Kirche" => "Kazan Church",
        "Hauptbahnhof" => "Central Station",
        "Mamajew-Hügel" => "Mamayev Hill",
        _ => name,
    }
}

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
        r.draw_text(map_landmark_label(&lm.name), x, y + 0.042, 0.021, faded, Align::Center);
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
fn draw_cartographic_polyline<P, V>(
    r: &mut Renderer,
    points: &[crate::run_route::RoutePoint],
    project: &P,
    visible: &V,
    color: [f32; 4],
    thickness: f32,
    dash_period: usize,
) where
    P: Fn(f32, f32) -> (f32, f32),
    V: Fn(glam::Vec2) -> bool,
{
    for segment in points.windows(2) {
        let a = glam::Vec2::new(segment[0].x as f32, segment[0].z as f32);
        let b = glam::Vec2::new(segment[1].x as f32, segment[1].z as f32);
        let (ax, ay) = project(a.x, a.y);
        let (bx, by) = project(b.x, b.y);
        let screen_length = glam::Vec2::new(bx - ax, by - ay).length();
        let steps = (screen_length / (thickness * 0.62).max(0.0012))
            .ceil()
            .max(1.0) as usize;
        for step in 0..=steps {
            if dash_period > 0 && (step / 3) % dash_period == dash_period - 1 {
                continue;
            }
            let t = step as f32 / steps as f32;
            let world = a.lerp(b, t);
            if !visible(world) {
                continue;
            }
            let (x, y) = project(world.x, world.y);
            r.fill_rect(
                x - thickness * 0.5,
                y - thickness * 0.5,
                thickness,
                thickness,
                color,
            );
        }
    }
}

/// The same relative mouse/controller paper controls are used by the window and review recorder.
pub fn update_map_pencil(
    cursor: &mut glam::Vec2,
    marks: &mut Vec<(u8, glam::Vec2)>,
    sheet: u8,
    delta: glam::Vec2,
    primary: bool,
) {
    cursor.x = (cursor.x + delta.x * 0.0018).clamp(0.02, 0.98);
    cursor.y = (cursor.y + delta.y * 0.0018).clamp(0.03, 0.97);
    if primary && marks.len() < 12_000 {
        let add = marks.iter().rev().find(|(id, _)| *id == sheet)
            .map_or(true, |(_, p)| p.distance(*cursor) > 0.0035);
        if add { marks.push((sheet, *cursor)); }
    }
}

pub fn toggle_map_pin(pins: &mut Vec<(u8, glam::Vec2)>, sheet: u8, cursor: glam::Vec2) {
    if let Some(index) = pins.iter().position(|(id, p)| *id == sheet && p.distance(cursor) < 0.024) {
        pins.remove(index);
    } else if pins.len() < 96 {
        pins.push((sheet, cursor));
    }
}

/// Queue the survey, saved annotations and cursor for the physical paper's albedo texture.
/// The renderer consumes this queue before the world pass. No fake hands or compass are drawn.
/// The sheet carries no words (2026-09-28): marks, symbols and pencil numbers only.
pub fn draw_held_map_print(
    r: &mut Renderer,
    act: &crate::world::ActMap,
    scout_marks: &[glam::Vec2],
    pencil: &[(u8, glam::Vec2)],
    pins: &[(u8, glam::Vec2)],
    route: Option<&crate::run_route::RunRoutePlan>,
    map_sheets: u8,
    cursor: glam::Vec2,
    _heading_deg: f32,
    pencil_down: bool,
    player_position: glam::Vec2,
    mission: Option<&crate::mission::MapGuidance>,
) {
    let (mx, my, mw, mh) = (0.17f32, 0.34f32, 0.66f32, 0.59f32);
    let paper = [0.78, 0.73, 0.60, 0.98];
    let paper_hi = [0.88, 0.84, 0.71, 0.98];
    let ink = [0.18, 0.16, 0.13, 0.95];
    let faded = [0.36, 0.33, 0.27, 0.90];
    let graphite = [0.11, 0.12, 0.12, 0.82];
    let red = [0.58, 0.08, 0.065, 0.95];

    // Pigment only. Fold geometry and ordinary scene lighting make the actual paper edges.
    r.fill_rect(mx, my, mw, mh, paper);
    r.fill_rect(mx + 0.012, my + 0.012, mw - 0.024, 0.012, paper_hi);
    r.fill_rect(mx + mw * 0.495, my, 0.007, mh, [0.46, 0.42, 0.33, 0.28]);
    r.fill_rect(mx, my + mh * 0.53, mw, 0.006, [0.47, 0.42, 0.33, 0.22]);

    let half = act.half_extent_m.max(1.0);
    let proj = |wx: f32, wz: f32| -> (f32, f32) {
        (
            mx + mw * (0.50 + wx / half * 0.39),
            my + mh * (0.51 + wz / half * 0.38),
        )
    };
    let is_revealed = |point: glam::Vec2| -> bool {
        let Some(route) = route else { return true };
        let start = glam::Vec2::new(route.start.x as f32, route.start.z as f32);
        let inside_patch = |center: glam::Vec2, half_x: f32, half_z: f32| {
            (point.x - center.x).abs() <= half_x && (point.y - center.y).abs() <= half_z
        };
        inside_patch(start, half * 0.27, half * 0.28)
            || route
                .sites
                .iter()
                .take(usize::from(map_sheets.saturating_add(1).min(3)))
                .any(|site| {
                    inside_patch(
                        glam::Vec2::new(site.position.x as f32, site.position.z as f32),
                        half * 0.37,
                        half * 0.37,
                    )
                })
    };

    // The first small survey scrap is already in the map case. Each recovered intelligence sheet
    // is a second physical plane laid across previously blank paper, offset enough that its edge and
    // fold remain visible instead of behaving like a conventional fog-of-war texture.
    if let Some(route) = route {
        let (sx, sy) = proj(route.start.x as f32, route.start.z as f32);
        r.fill_rect(
            sx - mw * 0.105,
            sy - mh * 0.105,
            mw * 0.21,
            mh * 0.21,
            [0.84, 0.80, 0.67, 0.90],
        );
        r.fill_rect(
            sx - mw * 0.105,
            sy - mh * 0.105,
            mw * 0.21,
            0.003,
            [0.34, 0.31, 0.25, 0.52],
        );
        for (index, site) in route
            .sites
            .iter()
            .take(usize::from(map_sheets.min(3)))
            .enumerate()
        {
            let (cx, cy) = proj(site.position.x as f32, site.position.z as f32);
            let sw = mw * 0.29;
            let sh = mh * 0.28;
            let ox = (index as f32 - 1.0) * 0.006;
            let oy = if index % 2 == 0 { -0.004 } else { 0.005 };
            r.fill_rect(
                cx - sw * 0.5 + ox,
                cy - sh * 0.5 + oy,
                sw,
                sh,
                [0.86, 0.82, 0.69, 0.94],
            );
            r.fill_rect(
                cx - sw * 0.5 + ox,
                cy - sh * 0.5 + oy,
                sw,
                0.0035,
                [0.31, 0.28, 0.22, 0.50],
            );
            r.fill_rect(
                cx + ox,
                cy - sh * 0.5 + oy,
                0.003,
                sh,
                [0.42, 0.38, 0.30, 0.26],
            );
        }
    }

    // Ink from the authored sector plan is clipped to the physical scraps above. It gives the crew
    // roads, structures and terrain to navigate by, but intentionally never draws the generated
    // start-to-sites-to-exit route (that knowledge belongs to the players, pencil and compass).
    if let Some(route) = route {
        let cartography = &route.cartography;
        let map_scale = ((mw * 0.39 + mh * 0.38) * 0.5) / half.max(1.0);

        // Water and terrain contours sit below roads, as they would on a field survey sheet.
        for waterway in &cartography.waterways {
            draw_cartographic_polyline(
                r,
                &waterway.points,
                &proj,
                &is_revealed,
                [0.29, 0.39, 0.40, 0.72],
                (waterway.width_m as f32 * map_scale).clamp(0.0030, 0.0070),
                0,
            );
        }
        for barrier in &cartography.barriers {
            let color = match barrier.kind {
                crate::run_route::CartographicBarrierKind::Ridge => [0.43, 0.36, 0.25, 0.58],
                crate::run_route::CartographicBarrierKind::Hollow => [0.34, 0.38, 0.34, 0.48],
            };
            draw_cartographic_polyline(r, &barrier.points, &proj, &is_revealed, color, 0.00145, 3);
        }
        for road in &cartography.roads {
            draw_cartographic_polyline(
                r,
                &road.points,
                &proj,
                &is_revealed,
                [0.33, 0.29, 0.23, 0.73],
                (road.width_m as f32 * map_scale).clamp(0.00145, 0.0032),
                0,
            );
        }

        // Landmark refs become restrained building footprints rather than waypoint icons.
        for landmark in &cartography.landmarks {
            let point = glam::Vec2::new(landmark.position.x as f32, landmark.position.z as f32);
            if !is_revealed(point) {
                continue;
            }
            let (x, y) = proj(point.x, point.y);
            r.fill_rect(
                x - 0.0060,
                y - 0.0042,
                0.0120,
                0.0084,
                [0.27, 0.24, 0.20, 0.78],
            );
            r.fill_rect(
                x - 0.0040,
                y - 0.0060,
                0.0035,
                0.0032,
                [0.32, 0.28, 0.22, 0.70],
            );
            r.fill_rect(
                x + 0.0015,
                y + 0.0034,
                0.0050,
                0.0028,
                [0.32, 0.28, 0.22, 0.70],
            );
        }
        // The sole Act-II bridge is a physical crossing drawn across the channel, never an exit.
        for bridge in &cartography.bridges {
            if !is_revealed(glam::Vec2::new(
                bridge.position.x as f32,
                bridge.position.z as f32,
            )) {
                continue;
            }
            draw_cartographic_polyline(
                r,
                &[bridge.deck_start, bridge.deck_end],
                &proj,
                &is_revealed,
                [0.16, 0.15, 0.13, 0.92],
                (bridge.clear_width_m as f32 * map_scale).clamp(0.0024, 0.0040),
                0,
            );
        }
    }

    for lm in &act.landmarks {
        if !is_revealed(lm.pos) {
            continue;
        }
        let (x, y) = proj(lm.pos.x, lm.pos.y);
        r.draw_text_bold("·", x, y, 0.026, ink, Align::Center);
        let glyph = lm.kind.map_glyph().to_string();
        r.draw_text_bold(&glyph, x, y + 0.021, 0.015, faded, Align::Center);
    }
    let route_start = route
        .map(|plan| glam::Vec2::new(plan.start.x as f32, plan.start.z as f32))
        .unwrap_or(act.spawn);
    let (sx, sy) = proj(route_start.x, route_start.y);
    r.draw_text_bold("+", sx, sy, 0.030, ink, Align::Center);
    if let Some(route) = route {
        // Recovered sites keep a numbered matte pin. The next site is deliberately only a broad
        // question mark—enough direction to avoid aimlessness, never a live waypoint.
        for (index, site) in route.sites.iter().enumerate() {
            let (x, y) = proj(site.position.x as f32, site.position.z as f32);
            if index < usize::from(map_sheets.min(3)) {
                r.fill_rect(
                    x - 0.0011,
                    y + 0.001,
                    0.0022,
                    0.016,
                    [0.54, 0.55, 0.53, 0.90],
                );
                r.fill_rect(
                    x - 0.0052,
                    y - 0.0042,
                    0.0104,
                    0.0084,
                    [0.03, 0.03, 0.028, 0.98],
                );
                r.draw_text_bold(
                    &(index + 1).to_string(),
                    x + 0.008,
                    y - 0.010,
                    0.014,
                    ink,
                    Align::Left,
                );
            } else if index == usize::from(map_sheets.min(3)) {
                // Recovered paperwork narrows the next military position to a search box, never a
                // GPS-exact dot. The stable offset is deliberately large enough that the crew must
                // read roads, silhouettes, engine noise and the guarding armour to find the door.
                let phase = ((site.id ^ (site.id >> 32)) % 6283) as f32 * 0.001;
                let radius = half * 0.055;
                let clue = glam::Vec2::new(
                    (site.position.x as f32 + phase.sin() * radius)
                        .clamp(-half * 0.90, half * 0.90),
                    (site.position.z as f32 + phase.cos() * radius)
                        .clamp(-half * 0.90, half * 0.90),
                );
                let (hx, hy) = proj(clue.x, clue.y);
                let (hw, hh, c) = (0.026, 0.022, red);
                for (cx, cy) in [
                    (hx - hw, hy - hh),
                    (hx + hw, hy - hh),
                    (hx - hw, hy + hh),
                    (hx + hw, hy + hh),
                ] {
                    r.fill_rect(cx - 0.002, cy - 0.002, 0.004, 0.004, c);
                }
                r.draw_text_bold("?", hx, hy - 0.008, 0.032, red, Align::Center);
            }
        }
        if map_sheets >= 3 {
            let (gx, gy) = proj(route.exit.x as f32, route.exit.z as f32);
            // The extraction point is a physical red pin, not a magic border trigger or waypoint.
            r.fill_rect(
                gx - 0.0012,
                gy + 0.001,
                0.0024,
                0.019,
                [0.63, 0.57, 0.50, 0.95],
            );
            r.fill_rect(gx - 0.0062, gy - 0.0050, 0.0124, 0.0100, red);
            r.fill_rect(
                gx - 0.0045,
                gy - 0.0035,
                0.0040,
                0.0020,
                [0.88, 0.22, 0.16, 0.90],
            );
        }
    } else {
        let (gx, gy) = proj(act.goal.x, act.goal.y);
        r.draw_text_bold("X", gx, gy, 0.038, red, Align::Center);
    }
    for mark in scout_marks {
        let (x, y) = proj(mark.x, mark.y);
        r.draw_text_bold("x", x, y, 0.022, red, Align::Center);
    }

    let (px, py) = proj(player_position.x, player_position.y);
    r.fill_rect(px-0.004,py-0.006,0.008,0.012,[0.04,0.22,0.25,1.0]);
    // The mission's marks (src/mission.rs) in the same plan projection.
    if let Some(guidance) = mission {
        draw_mission_marks(r, guidance, &|p: glam::Vec2| proj(p.x, p.y), (mx, my, mw, mh));
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

    // Pin annotations retain their saved paper UV positions and compact cartographic symbols.
    for (_, p) in pins.iter().filter(|(id, _)| *id == act.id) {
        let px = mx + p.x * mw;
        let py = my + p.y * mh;
        r.fill_rect(
            px - 0.0012,
            py + 0.001,
            0.0024,
            0.018,
            [0.58, 0.59, 0.57, 0.94],
        );
        r.fill_rect(
            px - 0.0060,
            py - 0.0050,
            0.0120,
            0.0100,
            [0.025, 0.026, 0.024, 0.98],
        );
        r.fill_rect(
            px - 0.0045,
            py - 0.0035,
            0.0040,
            0.0020,
            [0.20, 0.21, 0.19, 0.82],
        );
    }

    // Relative-mouse cursor is registered to paper UVs, just like persistent graphite strokes.
    let cp = glam::Vec2::new(mx + cursor.x * mw, my + cursor.y * mh);
    r.fill_rect(
        cp.x - 0.003,
        cp.y - 0.003,
        0.006,
        0.006,
        if pencil_down { graphite } else { faded },
    );
}

/// The in-game controls reference (toggled with the Home / Pos1 key while playing). Two columns of
/// key → action, in English, on the cold menu backdrop. Reuses the pixelated UI path.
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
        "enter looked-at station",
    );
    row(r, lk, 0.275, "E", "interact / put away tool");
    row(r, lk, 0.320, "Q", "leave sights / lower tool");
    row(r, lk, 0.365, "Space", "climb / enter / leave hatch");
    row(r, lk, 0.405, "H / LS+A", "open / close own hatch");
    header(r, lh, 0.44, "FIGHTING");
    row(r, lk, 0.490, "W S A D", "drive / walk at own station");
    row(r, lk, 0.535, "LMB / MMB", "action / coax in gunner sight");
    row(r, lk, 0.580, "RMB / B", "focus or aim / binoculars");
    row(r, lk, 0.625, "Arrow keys", "gunner: traverse / elevation");
    row(r, lk, 0.670, "V / F", "turret pistol / held flashlight");
    row(r, lk, 0.715, "Z / Shift", "engine start-stop / foot sprint");

    // ── Right column: inside work + system ──
    let (rk, rh) = (0.78, 0.575);
    header(r, rh, 0.17, "LOADER'S HANDS");
    row(r, rk, 0.230, "LMB hold", "carry round / case");
    row(r, rk, 0.275, "LMB click", "breech lever / equip tool");
    row(r, rk, 0.320, "hold + flick", "mouse / stick to ram round");
    row(r, rk, 0.365, "release LMB", "drop round / case");
    header(r, rh, 0.44, "SYSTEM");
    row(
        r,
        rk,
        0.500,
        "C / Shift+C",
        "cigarette / compass",
    );
    row(r, rk, 0.540, "1 / 2", "P08 / Panzerfaust");
    row(r, rk, 0.580, "R", "reload P08 / fresh tube");
    row(
        r,
        rk,
        0.620,
        "Ctrl hold",
        "crouch (inside or outside)",
    );
    row(
        r,
        rk,
        0.660,
        "F6 / B (co-op)",
        "take vacant station",
    );
    row(
        r,
        rk,
        0.700,
        if crate::graphics::fixed_playtest_graphics() { "F12 · Esc" } else { "[ ] · F12 · Esc" },
        if crate::graphics::fixed_playtest_graphics() { "screenshot / pause" } else { "retro / shot / pause" },
    );

    r.draw_text("C draw pack/cigarette · hold RMB lighter · LMB light/toke · E pick up · M map",0.5,0.755,0.024,COL_ITEM,Align::Center);
    r.draw_text_bold(
        "CONTROLLER  Xbox / PlayStation",
        0.5,
        0.790,
        0.028,
        COL_SUBTITLE,
        Align::Center,
    );
    r.draw_text(
        "LS drive · RS look/aim · RT action/fire · LT focus · A interact · B back/throw/take over",
        0.5,
        0.825,
        0.026,
        COL_ITEM,
        Align::Center,
    );
    r.draw_text(
        "Y tap head out / hold dismount · X reload/swap tube · D-pad L/R foot weapon · RB coax · View map",
        0.5,
        0.860,
        0.024,
        COL_ITEM,
        Align::Center,
    );
    r.draw_text("Driver: PgUp / PgDn manual gear · End automatic",0.5,0.890,0.024,COL_ITEM,Align::Center);
    r.fill_rect(0.34,0.930,0.32,0.054,[0.12,0.13,0.14,1.0]);
    r.draw_text("Back  ·  Home / Esc",0.5,0.940,0.029,COL_ITEM,Align::Center);
}

// ── Missions (src/mission.rs, docs/MISSIONS.md): the order sheet, the net's lines and the paper's
// marks. The order and the lines are display-resolution text outside the crush, like the menus;
// the marks are pigment on the held sheet's albedo, registered to the paper like the pencil. ──────

const COL_NET: [f32; 4] = [0.74, 0.90, 0.76, 1.0];
const COL_NET_OWN: [f32; 4] = [0.92, 0.92, 0.88, 1.0];
const COL_NOTE: [f32; 4] = [0.86, 0.80, 0.62, 1.0];

/// Break a line into pieces of at most `width` characters at word boundaries.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() { line.push(' '); }
        line.push_str(word);
    }
    if !line.is_empty() { lines.push(line); }
    lines
}

pub fn draw_mission_overlay(r: &mut Renderer, overlay: &crate::mission::Overlay) {
    // The written order (Befehl): a typed field-order slip held up over the world, then folded.
    if let Some((title, lines, first, alpha)) = &overlay.order {
        let mut body: Vec<String> = Vec::new();
        for line in lines {
            body.extend(wrap_text(line, 66));
            body.push(String::new());
        }
        let (x, w) = (0.17f32, 0.66f32);
        let h = 0.135 + body.len() as f32 * 0.030 + 0.07;
        let y = 0.15;
        let a = alpha.clamp(0.0, 1.0);
        r.fill_rect(x + 0.006, y + 0.008, w, h, [0.0, 0.0, 0.0, 0.35 * a]);
        r.fill_rect(x, y, w, h, [0.80, 0.75, 0.62, 0.94 * a]);
        r.fill_rect(x, y, w, 0.004, [0.52, 0.46, 0.34, 0.6 * a]);
        r.fill_rect(x + 0.02, y + 0.085, w - 0.04, 0.0015, [0.30, 0.26, 0.20, 0.6 * a]);
        let ink = [0.16, 0.14, 0.11, 0.95 * a];
        let faded = [0.36, 0.33, 0.27, 0.90 * a];
        r.draw_text_bold(title, 0.5, y + 0.030, 0.022, ink, Align::Center);
        r.draw_text("Kampfgruppe  ·  Nachhut  ·  schriftlicher Befehl", 0.5, y + 0.062, 0.018, faded, Align::Center);
        let mut ty = y + 0.108;
        for line in &body {
            if !line.is_empty() { r.draw_text(line, x + 0.03, ty, 0.025, ink, Align::Left); }
            ty += 0.030;
        }
        r.draw_text_bold(first, x + 0.03, ty + 0.012, 0.018, [0.58, 0.08, 0.065, 0.95 * a], Align::Left);
        r.draw_text("M  the map carries the marks and the bearing", 0.5, y + h - 0.034, 0.018, faded, Align::Center);
    }
    // The net: a signals log, top left, oldest first.
    let mut y = 0.05;
    for (from, text, alpha) in &overlay.radio {
        let (color, line) = if from.is_empty() {
            ([COL_NOTE[0], COL_NOTE[1], COL_NOTE[2], 0.9 * alpha], text.clone())
        } else if from.eq_ignore_ascii_case("kiebitz") {
            ([COL_NET_OWN[0], COL_NET_OWN[1], COL_NET_OWN[2], 0.85 * alpha], format!("{}: {}", from.to_uppercase(), text))
        } else {
            ([COL_NET[0], COL_NET[1], COL_NET[2], 0.95 * alpha], format!("{}: {}", from.to_uppercase(), text))
        };
        r.draw_text(&line, 0.02, y, 0.020, color, Align::Left);
        y += 0.028;
    }
}

/// The mission's pigment on the held sheet. `proj` maps a world point to the sheet's normalized
/// screen position; `paper` is the printed rectangle (x, y, w, h) for the slip and clipping.
pub fn draw_mission_marks(
    r: &mut Renderer,
    guidance: &crate::mission::MapGuidance,
    proj: &dyn Fn(glam::Vec2) -> (f32, f32),
    paper: (f32, f32, f32, f32),
) {
    use crate::mission::MarkKind;
    let aspect = r.ih.max(1) as f32 / r.iw.max(1) as f32;
    let graphite = [0.07, 0.08, 0.08, 1.0];
    let red = [0.58, 0.06, 0.05, 1.0];
    let faded = [0.26, 0.26, 0.24, 0.80];
    let (px, py, pw, ph) = paper;
    let inside = |x: f32, y: f32| x >= px && x <= px + pw && y >= py && y <= py + ph;
    let dot = |r: &mut Renderer, x: f32, y: f32, s: f32, c: [f32; 4]| {
        if inside(x, y) { r.fill_rect(x - s * aspect * 0.5, y - s * 0.5, s * aspect, s, c); }
    };
    let ring = |r: &mut Renderer, cx: f32, cy: f32, radius: f32, n: usize, dashed: bool, c: [f32; 4]| {
        for k in 0..n {
            if dashed && k % 3 == 2 { continue; }
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            dot(r, cx + a.cos() * radius * aspect, cy + a.sin() * radius, 0.0042, c);
        }
    };
    let line = |r: &mut Renderer, (ax, ay): (f32, f32), (bx, by): (f32, f32), dashed: bool, c: [f32; 4]| {
        let steps = (((bx - ax) / aspect).hypot(by - ay) / 0.0036).ceil().max(1.0) as usize;
        for k in 0..=steps {
            if dashed && (k / 4) % 2 == 1 { continue; }
            let t = k as f32 / steps as f32;
            dot(r, ax + (bx - ax) * t, ay + (by - ay) * t, 0.0038, c);
        }
    };

    // The dead-reckoned position: a pencil ring the size of the crew's uncertainty.
    if let Some((estimate, error_m)) = guidance.estimate {
        let (ex, ey) = proj(estimate);
        let (fx, _) = proj(estimate + glam::Vec2::new(error_m, 0.0));
        let radius = ((fx - ex) / aspect).abs().clamp(0.006, 0.08);
        ring(r, ex, ey, radius, 32, true, graphite);
        dot(r, ex, ey, 0.006, graphite);
    }
    // The bearing: a dashed pencil line to the mark with its three figures.
    if let Some((from, to, bearing)) = guidance.bearing {
        let a = proj(from);
        let b = proj(to);
        line(r, a, b, true, graphite);
        let (mx, my) = (a.0 + (b.0 - a.0) * 0.45, a.1 + (b.1 - a.1) * 0.45);
        if inside(mx, my) {
            r.draw_text_bold(&format!("{:03.0}", bearing.rem_euclid(360.0)), mx + 0.014, my - 0.020, 0.017, graphite, Align::Left);
        }
    }
    for &(at, kind, number, done) in &guidance.marks {
        let (x, y) = proj(at);
        if !inside(x, y) { continue; }
        let color = if done { faded } else if matches!(kind, MarkKind::Pin | MarkKind::Cross) { red } else { graphite };
        match kind {
            MarkKind::Box => {
                let (hw, hh) = (0.024 * aspect, 0.021);
                for (cx, cy) in [(x - hw, y - hh), (x + hw, y - hh), (x - hw, y + hh), (x + hw, y + hh)] {
                    dot(r, cx, cy, 0.0065, color);
                }
                line(r, (x - hw, y - hh), (x - hw, y + hh), true, color);
                line(r, (x + hw, y - hh), (x + hw, y + hh), true, color);
                line(r, (x - hw, y - hh), (x + hw, y - hh), true, color);
                line(r, (x - hw, y + hh), (x + hw, y + hh), true, color);
            }
            MarkKind::Cross => {
                let d = 0.014;
                line(r, (x - d * aspect, y - d), (x + d * aspect, y + d), false, color);
                line(r, (x - d * aspect, y + d), (x + d * aspect, y - d), false, color);
            }
            MarkKind::Pin => {
                r.fill_rect(x - 0.0012, y + 0.001, 0.0024, 0.019, [0.63, 0.57, 0.50, 0.95]);
                r.fill_rect(x - 0.0062, y - 0.0050, 0.0124, 0.0100, color);
            }
            MarkKind::Circle => ring(r, x, y, 0.016, 22, false, color),
        }
        r.draw_text_bold(&number.to_string(), x + 0.024 * aspect + 0.008, y - 0.014, 0.017, color, Align::Left);
    }
    // The slip clipped to the sheet's corner: the open beat's number and title.
    if let Some(slip) = &guidance.slip {
        let (sw, sh) = (0.46f32, 0.052f32);
        let (sx, sy) = (px + 0.012, py + 0.012);
        r.fill_rect(sx + 0.003, sy + 0.004, sw, sh, [0.0, 0.0, 0.0, 0.18]);
        r.fill_rect(sx, sy, sw, sh, [0.86, 0.82, 0.70, 0.97]);
        r.fill_rect(sx, sy, sw, 0.003, [0.42, 0.38, 0.30, 0.5]);
        r.draw_text_bold(slip, sx + 0.012, sy + 0.015, 0.019, [0.12, 0.11, 0.09, 1.0], Align::Left);
    }
}
