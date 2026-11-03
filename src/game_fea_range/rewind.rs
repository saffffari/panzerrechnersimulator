//! The range's rewind: the game's state at the end of run frames, kept in memory and put back
//! without simulating again. Review-only (`fea-range`); no play path builds one.
//!
//! A checkpoint is the game's own world-save state (`game_world_state`'s owners, encoded as the
//! world save encodes them, every field by name) plus what the world save does not keep: the
//! crew's voxel tissue and its pieces (`crew_corpse_tissue::TissueState`) and the damage model's
//! chain receipts, and the game's clock and the renderer's pressure fronts and wind. A field unchanged since the previous checkpoint is
//! shared with it, and a changed one is kept as the byte runs that changed (`Blob`), so a
//! checkpoint costs what the frame changed. Putting one back assigns only the parts that differ
//! from the present state; the game's next update draws them (the range steps the frame being
//! shown from the checkpoint before it, so the picture is the update's own).
//!
//! Not kept (the strike does not read them): the renderer's own draw state, sky and camera; the
//! grass about the camera (its wind bend); the expedition's transport, workshop, progression and
//! runtime.
use super::*;
use crate::expedition_world_save::StateFields;
use std::collections::BTreeMap;
use std::sync::Arc;

/// One field's encoded bytes at one checkpoint: whole, or a patch on the checkpoint before it
/// (spans copied from its bytes and the bytes that are new), at most `MAX_PATCHES` in a row before
/// a whole copy.
enum Blob {
    /// `wait`: checkpoints to go before a patch is tried again (one just failed: the field is
    /// changing throughout, e.g. blood in flight).
    Whole { bytes: Vec<u8>, wait: u8 },
    Patch { prev: Arc<Blob>, depth: u16, ops: Vec<u8> },
}

const MAX_PATCHES: u16 = 15;
const PATCH_RETRY: u8 = 4;
/// The patch's matching unit (bytes).
const BLOCK: usize = 32;
const HASH_BASE: u64 = 0x0000_0100_0000_01B3;

fn varint(out: &mut Vec<u8>, mut n: usize) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn read_varint(ops: &[u8], at: &mut usize) -> usize {
    let (mut n, mut shift) = (0usize, 0);
    loop {
        let b = ops[*at];
        *at += 1;
        n |= ((b & 0x7F) as usize) << shift;
        if b < 0x80 {
            return n;
        }
        shift += 7;
    }
}

fn block_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |h, &b| h.wrapping_mul(HASH_BASE).wrapping_add(u64::from(b) + 1))
}

/// `new` as spans of `old` and new bytes: `(len << 1 | 1, offset)` copies `len` bytes of `old`,
/// `(len << 1, bytes)` adds them. Spans of `old` are found where the last copy's shift continues,
/// else by a rolling hash of `BLOCK`-byte windows against `old`'s aligned blocks. `None` when the
/// new bytes come to half of `new` (a whole copy is as cheap).
fn diff(old: &[u8], new: &[u8]) -> Option<Vec<u8>> {
    let mut blocks: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for at in (0..old.len().saturating_sub(BLOCK - 1)).step_by(BLOCK) {
        blocks.entry(block_hash(&old[at..at + BLOCK])).or_insert(at);
    }
    let top = (0..BLOCK - 1).fold(1u64, |p, _| p.wrapping_mul(HASH_BASE));
    let (mut ops, mut fresh) = (Vec::new(), 0usize);
    let (mut at, mut start, mut shift) = (0usize, 0usize, 0isize);
    let mut hash: Option<u64> = None;
    let flush = |ops: &mut Vec<u8>, from: usize, to: usize| {
        if to > from {
            varint(ops, (to - from) << 1);
            ops.extend_from_slice(&new[from..to]);
        }
    };
    while at + BLOCK <= new.len() {
        let window = &new[at..at + BLOCK];
        let matches = |o: usize| o + BLOCK <= old.len() && &old[o..o + BLOCK] == window;
        let same = at as isize + shift;
        let found = if same >= 0 && matches(same as usize) {
            Some(same as usize)
        } else {
            let h = *hash.get_or_insert_with(|| block_hash(window));
            blocks.get(&h).copied().filter(|&o| matches(o))
        };
        match found {
            Some(o) => {
                let mut len = BLOCK;
                while at + len < new.len() && o + len < old.len() && old[o + len] == new[at + len] {
                    len += 1;
                }
                flush(&mut ops, start, at);
                varint(&mut ops, (len << 1) | 1);
                varint(&mut ops, o);
                shift = o as isize - at as isize;
                at += len;
                start = at;
                hash = None;
            }
            None => {
                fresh += 1;
                if fresh * 2 >= new.len() {
                    return None;
                }
                if at + BLOCK < new.len() {
                    let h = hash.unwrap_or_else(|| block_hash(window));
                    let out = (u64::from(new[at]) + 1).wrapping_mul(top);
                    hash = Some(h.wrapping_sub(out).wrapping_mul(HASH_BASE).wrapping_add(u64::from(new[at + BLOCK]) + 1));
                }
                at += 1;
            }
        }
    }
    flush(&mut ops, start, new.len());
    (ops.len() * 2 < new.len()).then_some(ops)
}

impl Blob {
    fn decode(&self) -> Vec<u8> {
        match self {
            Blob::Whole { bytes, .. } => bytes.clone(),
            Blob::Patch { prev, ops, .. } => {
                let old = prev.decode();
                let mut bytes = Vec::with_capacity(old.len());
                let mut at = 0;
                while at < ops.len() {
                    let tag = read_varint(ops, &mut at);
                    let len = tag >> 1;
                    if tag & 1 == 1 {
                        let from = read_varint(ops, &mut at);
                        bytes.extend_from_slice(&old[from..from + len]);
                    } else {
                        bytes.extend_from_slice(&ops[at..at + len]);
                        at += len;
                    }
                }
                bytes
            }
        }
    }

    fn depth(&self) -> u16 {
        match self {
            Blob::Whole { .. } => 0,
            Blob::Patch { depth, .. } => *depth,
        }
    }

    /// The bytes this version holds itself.
    fn stored(&self) -> usize {
        match self {
            Blob::Whole { bytes, .. } => bytes.len(),
            Blob::Patch { ops, .. } => ops.len(),
        }
    }
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The game's state at one moment.
#[derive(Clone)]
pub(super) struct Checkpoint {
    id: u64,
    /// Every encoded field, `owner/field`.
    fields: BTreeMap<String, Arc<Blob>>,
    tissue: Option<crew_corpse_tissue::TissueState>,
    receipts: Vec<game_fea::FeaChainReceipt>,
    current: Option<game_fea::ImpactSolve>,
    /// What the game changes in a run that the world save does not hold (`Unsaved`).
    unsaved: Arc<Unsaved>,
    /// The surface blood: a copy (its world-save encoding sorts every map and set entry by its
    /// bytes, a tenth of a second a frame late in a run, where a copy takes milliseconds but holds
    /// about 12 MB); past `ENCODE_BLOOD_ABOVE_BYTES`, encoded and patched on the rewind's worker.
    blood: BloodKept,
    /// The game's clock (the renderer's `time`: `Game::update` advances it and reads the armour
    /// clock, pressure arrivals and projectile flight from it; the world save keeps it too).
    clock_s: f32,
    /// Bytes this checkpoint holds that the one before it does not share.
    pub(super) own_bytes: usize,
    /// The range's own count at this moment: time after the shot and the tape's length.
    pub(super) shot_t: f32,
    pub(super) tape_len: usize,
}

fn put_raw(owner: &str, fields: StateFields, out: &mut BTreeMap<String, Vec<u8>>) {
    for (key, bytes) in fields.into_raw() {
        out.insert(format!("{owner}/{key}"), bytes);
    }
}

#[derive(Clone)]
enum BloodKept {
    Copy(Arc<surface_blood::SurfaceBlood>),
    Encoded(Arc<Blob>),
}

/// The surface blood's world-save bytes.
fn encode_blood(blood: &surface_blood::SurfaceBlood) -> anyhow::Result<Vec<u8>> {
    let mut fields = StateFields::default();
    fields.put("surface_blood", blood)?;
    fields.into_raw().remove("surface_blood").ok_or_else(|| anyhow::anyhow!("surface blood not encoded"))
}

impl BloodKept {
    fn bytes(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            BloodKept::Copy(blood) => encode_blood(blood),
            BloodKept::Encoded(blob) => Ok(blob.decode()),
        }
    }

    fn get(&self) -> anyhow::Result<surface_blood::SurfaceBlood> {
        match self {
            BloodKept::Copy(blood) => Ok((**blood).clone()),
            BloodKept::Encoded(blob) => {
                let mut fields = StateFields::from_raw([("surface_blood".to_string(), blob.decode())].into());
                fields.take("surface_blood")
            }
        }
    }

    fn held(&self) -> usize {
        match self {
            BloodKept::Copy(blood) => blood.copy_bytes(),
            BloodKept::Encoded(blob) => blob.stored(),
        }
    }
}

/// A field's next version as a patch on `before` (its blob and bytes) where that pays.
fn next_blob(before: Option<(&Arc<Blob>, &Vec<u8>)>, bytes: &[u8]) -> Arc<Blob> {
    match before {
        Some((blob, old)) if old.as_slice() == bytes => blob.clone(),
        Some((blob, _)) if matches!(**blob, Blob::Whole { wait: 1.., .. }) => {
            let Blob::Whole { wait, .. } = **blob else { unreachable!() };
            Arc::new(Blob::Whole { bytes: bytes.to_vec(), wait: wait - 1 })
        }
        Some((blob, old)) if bytes.len() >= 4 * BLOCK && blob.depth() < MAX_PATCHES => match diff(old, bytes) {
            Some(ops) => Arc::new(Blob::Patch { prev: blob.clone(), depth: blob.depth() + 1, ops }),
            None => Arc::new(Blob::Whole { bytes: bytes.to_vec(), wait: PATCH_RETRY }),
        },
        _ => Arc::new(Blob::Whole { bytes: bytes.to_vec(), wait: 0 }),
    }
}

/// Encodes the run's blood copies in frame order off the simulation thread, each as a patch on
/// the one before; the encoded blob replaces the copy in its checkpoint when it comes back.
struct BloodWorker {
    jobs: std::sync::mpsc::Sender<(u64, u32, Arc<surface_blood::SurfaceBlood>)>,
    /// (run, frame, encoded blood, the bytes it holds that the frame before does not share).
    done: std::sync::mpsc::Receiver<(u64, u32, Arc<Blob>, usize)>,
    pending: usize,
}

impl BloodWorker {
    fn start() -> Self {
        let (jobs, inbox) = std::sync::mpsc::channel::<(u64, u32, Arc<surface_blood::SurfaceBlood>)>();
        let (outbox, done) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut last: Option<(u64, Arc<Blob>, Vec<u8>)> = None;
            for (run, frame, blood) in inbox {
                let Ok(bytes) = encode_blood(&blood) else { continue };
                drop(blood);
                let before = last.as_ref().filter(|(r, _, _)| *r == run).map(|(_, b, old)| (b, old));
                let blob = next_blob(before, &bytes);
                let own = if before.is_some_and(|(b, _)| Arc::ptr_eq(b, &blob)) { 0 } else { blob.stored() };
                last = Some((run, blob.clone(), bytes));
                if outbox.send((run, frame, blob, own)).is_err() {
                    return;
                }
            }
        });
        Self { jobs, done, pending: 0 }
    }
}

/// What the game changes in a run that the world save does not hold, kept as copies: the thrown
/// turret and the cook-off, the scars' solved rim cooling (the save skips it), the queued and
/// travelling blast fronts and what they last did, the strike's effects and flashes, the player
/// Tiger's ground contact, the armour's own surface fires and the rest named below. Every other
/// field of `Game` is held by the world save or listed in `NOT_KEPT` (test
/// `every_game_field_is_kept_or_named`).
#[derive(Clone)]
struct Unsaved {
    turret_thrown: Option<Mat4>,
    cookoff: game_cookoff::Cookoff,
    armor_scars: crate::armor_damage::ArmorField,
    wreck_turrets: Vec<Option<wreck_turret::Body>>,
    interior_soot: f32,
    fea_effects: game_fea_effects::FeaEffects,
    pending_fronts: Vec<(u64, Vec3, f32, bool)>,
    pressure_sweep_t: f32,
    pressure_sweep_fronts: Vec<crate::pressure_front::Front>,
    pressure_sweep_window: (f32, f32),
    pressure_observer: Option<Vec3>,
    pressure_startled: u32,
    pressure_last_kick: f32,
    pressure_grass_peak: f32,
    pressure_tree_peak: f32,
    last_exterior_strike: Option<serde_json::Value>,
    /// The armour's own surface fires (`CabinFire`, kept as its world-save fields).
    projectile_player_armor: StateFields,
    projectile_enemy_armor: StateFields,
    armour_flashes: Vec<ArmourFlash>,
    tableau_smoke: Vec<crate::urban_tableaux::SmokeSource>,
    tank_ground: solid_city::TankGroundState,
    enemy_stall_s: Vec<f32>,
    enemy_room: Vec<[f32; 4]>,
    enemy_room_frame: u32,
    couch_crew_frame: crate::lan::CrewFrame,
    couch_crew_identities: CouchCrewIdentities,
    last_frame_dt: f32,
    brass_cost: BrassCost,
    fire_presentation_phase: u32,
    replay_scar_cutoff: Option<f32>,
}

impl Unsaved {
    fn capture(g: &Game) -> anyhow::Result<Self> {
        Ok(Self {
            turret_thrown: g.player.turret_thrown,
            cookoff: g.cookoff.clone(),
            armor_scars: g.armor_scars.clone(),
            wreck_turrets: g.wrecks.iter().map(|w| w.t_body.clone()).collect(),
            interior_soot: g.interior_soot,
            fea_effects: g.fea_effects.clone(),
            pending_fronts: g.pending_fronts.clone(),
            pressure_sweep_t: g.pressure_sweep_t,
            pressure_sweep_fronts: g.pressure_sweep_fronts.clone(),
            pressure_sweep_window: g.pressure_sweep_window,
            pressure_observer: g.pressure_observer,
            pressure_startled: g.pressure_startled,
            pressure_last_kick: g.pressure_last_kick,
            pressure_grass_peak: g.pressure_grass_peak,
            pressure_tree_peak: g.pressure_tree_peak,
            last_exterior_strike: g.last_exterior_strike.clone(),
            projectile_player_armor: g.projectile_player_armor.export_world_state()?,
            projectile_enemy_armor: g.projectile_enemy_armor.export_world_state()?,
            armour_flashes: g.armour_flashes.clone(),
            tableau_smoke: g.tableau_smoke.clone(),
            tank_ground: g.tank_ground.clone(),
            enemy_stall_s: g.enemy_stall_s.clone(),
            enemy_room: g.enemy_room.clone(),
            enemy_room_frame: g.enemy_room_frame,
            couch_crew_frame: g.couch_crew_frame,
            couch_crew_identities: g.couch_crew_identities.clone(),
            last_frame_dt: g.last_frame_dt,
            brass_cost: g.brass_cost,
            fire_presentation_phase: g.fire_presentation_phase,
            replay_scar_cutoff: g.replay_scar_cutoff,
        })
    }

    /// Put back (after the world-save owners, whose `player` and `armor_scars` these complete).
    fn restore(&self, g: &mut Game) -> anyhow::Result<()> {
        g.player.turret_thrown = self.turret_thrown;
        g.cookoff = self.cookoff.clone();
        g.armor_scars = self.armor_scars.clone();
        for (wreck, body) in g.wrecks.iter_mut().zip(&self.wreck_turrets) {
            wreck.t_body = body.clone();
        }
        g.interior_soot = self.interior_soot;
        g.fea_effects = self.fea_effects.clone();
        g.pending_fronts = self.pending_fronts.clone();
        g.pressure_sweep_t = self.pressure_sweep_t;
        g.pressure_sweep_fronts = self.pressure_sweep_fronts.clone();
        g.pressure_sweep_window = self.pressure_sweep_window;
        g.pressure_observer = self.pressure_observer;
        g.pressure_startled = self.pressure_startled;
        g.pressure_last_kick = self.pressure_last_kick;
        g.pressure_grass_peak = self.pressure_grass_peak;
        g.pressure_tree_peak = self.pressure_tree_peak;
        g.last_exterior_strike = self.last_exterior_strike.clone();
        g.projectile_player_armor.import_world_state(self.projectile_player_armor.clone())?;
        g.projectile_enemy_armor.import_world_state(self.projectile_enemy_armor.clone())?;
        g.armour_flashes = self.armour_flashes.clone();
        g.tableau_smoke = self.tableau_smoke.clone();
        g.tank_ground = self.tank_ground.clone();
        g.enemy_stall_s = self.enemy_stall_s.clone();
        g.enemy_room = self.enemy_room.clone();
        g.enemy_room_frame = self.enemy_room_frame;
        g.couch_crew_frame = self.couch_crew_frame;
        g.couch_crew_identities = self.couch_crew_identities.clone();
        g.last_frame_dt = self.last_frame_dt;
        g.brass_cost = self.brass_cost;
        g.fire_presentation_phase = self.fire_presentation_phase;
        g.replay_scar_cutoff = self.replay_scar_cutoff;
        Ok(())
    }

    /// Checksums of the parts that print (`Checkpoint::prints`).
    fn prints(&self, out: &mut BTreeMap<String, u64>) {
        use crate::expedition_world_save::checksum;
        let mut put = |k: &str, text: String| {
            out.insert(format!("unsaved/{k}"), checksum(text.as_bytes()));
        };
        put("turret_thrown", format!("{:?}", self.turret_thrown));
        let c = &self.cookoff;
        put("cookoff", format!("{:?}", (c.clock_s, c.first_s, c.first_at, c.black_from_s, c.launched_s, c.turret.is_some(),
            &c.path, c.holes_m2, c.ignitions, c.detonations, c.fuel_kg)));
        put("armor_scars", format!("{:?}", self.armor_scars));
        put("wreck_turrets", format!("{:?}", self.wreck_turrets.iter().map(Option::is_some).collect::<Vec<_>>()));
        put("interior_soot", format!("{:?}", self.interior_soot));
        put("fea_effects", format!("{:?}", self.fea_effects));
        put("pressure", format!("{:?}", (&self.pending_fronts, self.pressure_sweep_t, &self.pressure_sweep_fronts,
            self.pressure_sweep_window, self.pressure_observer, self.pressure_startled, self.pressure_last_kick,
            self.pressure_grass_peak, self.pressure_tree_peak)));
        put("last_exterior_strike", format!("{:?}", self.last_exterior_strike));
        put("armour_flashes", format!("{:?}", self.armour_flashes));
        put("tank_ground", format!("{:?}", self.tank_ground));
        put("couch_crew_frame", format!("{:?}", self.couch_crew_frame));
    }
}

/// `Game` fields neither the world save nor `Unsaved` holds, each with why a run does not need
/// it back (test `every_game_field_is_kept_or_named`).
pub(super) const NOT_KEPT: &[(&str, &str)] = &[
    ("cigarette_fx_slots", "renderer slots"), ("act", "the act's authored data"), ("tank_editor_scene", "editor scene"),
    ("grass", "the grass about the camera (see the module doc)"), ("platoon", "radio orders, not in the range"),
    ("enemy_report_audit", "an audit log"), ("campaign", "campaign coordinator, not in the range"),
    ("expedition_runtime", "expedition, not in the range"), ("support_camp", "expedition camp"),
    ("halftrack_runtime", "expedition halftrack"), ("theatre", "expedition theatre"), ("river", "expedition rivers"),
    ("workshop", "workshop runtime, untouched by a strike"), ("crew_status_panel", "a panel's display"),
    ("tpz_cover", "a periscope cover's presentation"), ("cabin_panels", "instrument panels' presentation"),
    ("enemy_gear_snow", "presentation of enemy running gear"), ("ally_gear_snow", "presentation of ally running gear"),
    ("weather_view", "the view's weather sample"), ("winter_mission", "mission authoring"), ("slots", "renderer slots"),
    ("tiger_dressing", "authored dressing"), ("enigma_dressing", "authored dressing"), ("order_printer", "printer presentation"),
    ("stab_net", "radio language model"), ("projectile_cover", "acceleration structure"),
    ("projectile_flight_audio", "sound voices"), ("crew_battle_audit", "an audit log"), ("armor_net_cursor", "a lookup cursor"),
    ("scar_geometry_cache", "a cache keyed by the scars"), ("rtt_screens", "periscope render targets"),
    ("rtt_dev", "GPU device"), ("cupola_grip_source", "source mesh"), ("cupola_spindle_source", "source mesh"),
    ("last_player_damage_sound", "the last sound's parameters"), ("hover", "the crosshair's hover"),
    ("hover_label", "the crosshair's hover"), ("scheduled_audio_audit", "an audit log"), ("audio", "the sound device"),
    ("tableau_attachments", "authored tableaux"), ("enemy_policy_over_orders", "AI policy setting"),
    ("scatter_heavy", "source data"), ("destructible_groups", "source index"), ("world_index", "source index"),
    ("story_cover_shared", "source data"), ("door_links", "source data"), ("manifest_walk_surfaces", "source data"),
    ("lan_ai_frame", "LAN, not in the range"), ("street_plan", "source data"), ("canyon_spec", "source data"),
    ("canyon_run", "canyon run bookkeeping"), ("sim_phase", "profiling"), ("canyon_siting", "source data"),
    ("street_siting", "source data"), ("harness_streets", "harness setting"), ("harness_gun_shots", "harness setting"),
    ("wreck_fire_surfaces", "source triangles"), ("wreck_patch_cache", "a cache"), ("player_fire_surfaces", "source triangles"),
    ("wreck_turret_sources", "source meshes"), ("death_replay", "the death's kill-cam (not used in the range)"),
    ("lan_local_role", "LAN, not in the range"), ("soviet_pose_batches", "renderer batches"),
    ("german_pose_batches", "renderer batches"), ("enemy_ai_stats", "AI statistics"),
    ("campaign_pedestrian_shots", "campaign telemetry"), ("campaign_foot_decisions", "campaign telemetry"),
    ("abandoned_collision", "colliders of abandoned tanks (campaign)"),
];

/// The owner's fields, as its restore takes them.
fn owner_fields(fields: &BTreeMap<String, Vec<u8>>, owner: &str) -> StateFields {
    let prefix = format!("{owner}/");
    StateFields::from_raw(fields.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix))
        .map(|(k, v)| (k[prefix.len()..].to_string(), v.clone())).collect())
}

/// The encoded state of every owner (see the module doc).
fn encode(g: &Game, r: &Renderer) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
    let mut out = BTreeMap::new();
    put_raw("physical", g.capture_physical()?, &mut out);
    put_raw("tanks", g.capture_tanks()?, &mut out);
    put_raw("personal", g.capture_personal()?, &mut out);
    put_raw("projectiles", g.capture_projectiles()?, &mut out);
    put_raw("infantry", g.capture_infantry()?, &mut out);
    // The environment owner but its surface blood (kept as a copy: `Checkpoint::blood`).
    put_raw("environment", crate::capture_world_fields!(g; occluders, story_cover, scatter_heavy_absorbed, destructibles,
        chunks, interior_fire_lights, world_fire_lights), &mut out);
    // What each owner's world-save export adds to its fields (`game_world_state`), and the crew
    // owner's (`crew_world_state`).
    let mut x = StateFields::default();
    x.put("loose", &g.loose.as_slice())?;
    x.put("panzerfaust_rack_taken", &g.panzerfaust_rack_taken)?;
    x.put("inf_flash_t", &g.inf_flash_t)?;
    x.put("inf_flash_pos", &g.inf_flash_pos)?;
    x.put("tiger_refuel", &g.tiger_refuel.state)?;
    x.put("first_person", &g.first_person.as_ref().map(|p| p.export_world_state()).transpose()?)?;
    x.put("track_snow", &g.track_snow.export_world_state()?)?;
    x.put("fea", &g.fea.export_world_state())?;
    x.put("hand_props", &g.hand_props.as_ref().map(|p| p.export_world_state()).transpose()?)?;
    x.put("hot_fragments", &g.hot_fragments.as_slice())?;
    x.put("weapon_vfx", &g.weapon_vfx.export_world_state()?)?;
    x.put("cabin_fire", &g.cabin_fire.export_world_state()?)?;
    x.put("ammo_fire", &g.ammo_fire)?;
    x.put("scene_environment", &g.scene_environment.export_world_state()?)?;
    x.put("acoustics", &g.acoustics.export_world_state()?)?;
    x.put("trees", &g.trees.export_world_state()?)?;
    x.put("scatter", &g.scatter.export_world_state()?)?;
    x.put("buildings", &g.buildings.export_world_state()?)?;
    x.put("world_doors", &g.world_doors.export_world_state()?)?;
    x.put("camp_people", &g.camp_people.as_ref().map(|c| c.export_world_state()).transpose()?)?;
    x.put("terrain_detail", &g.hf.detail.as_ref().map(|d| d.export_world_state()).transpose()?)?;
    x.put("crew", &g.crew)?;
    x.put("singleplayer_crew", &g.singleplayer_crew)?;
    x.put("singleplayer_crew_active", &g.singleplayer_crew_active)?;
    x.put("crew_solo", &g.crew_solo)?;
    x.put("driver_controls", &g.driver_controls)?;
    x.put("crew_arm_animation", &g.crew_arm_animation.snapshot())?;
    x.put("living_wounds", &g.living_wounds.snapshot())?;
    x.put("couch_kia_peers", &g.couch_kia_peers)?;
    x.put("couch_casualties", &g.couch_casualties)?;
    x.put("couch_ragdolls", &g.couch_ragdolls)?;
    x.put("crew_body_recovery", &g.crew_body_recovery)?;
    x.put("couch_last_casualty", &g.couch_last_casualty)?;
    x.put("crew_panel_selected", &g.crew_panel_selected)?;
    x.put("crew_panel_notice", &g.crew_panel_notice)?;
    let space = &g.crew_personal_space;
    x.put("crew_contact_counts", &(space.candidates, space.bodies, space.queries))?;
    put_raw("x", x, &mut out);
    // The renderer's share of the simulation the game reads back: the travelling pressure fronts
    // (a blast's arrival at the listener) and the wind.
    let mut world = StateFields::default();
    world.put("pressure_fronts", &r.pressure_fronts)?;
    world.put("weather_wind_world", &r.weather_wind_world)?;
    put_raw("renderer", world, &mut out);
    Ok(out)
}

impl Checkpoint {
    /// The game's state now, and its encoded fields. A field equal to `previous`'s (given with its
    /// fields decoded) is shared with it; a changed one is kept as a patch on it when that is less
    /// than half of it.
    pub(super) fn capture(g: &Game, r: &Renderer, previous: Option<(&Checkpoint, &BTreeMap<String, Vec<u8>>)>)
        -> anyhow::Result<(Self, BTreeMap<String, Vec<u8>>)> {
        anyhow::ensure!(g.fea.started.is_none(), "a plate solve is being started inside this update");
        anyhow::ensure!(!g.casualty_tissue_pending(), "a casualty's tissue is solving on its worker");
        let started = std::time::Instant::now();
        let raw = encode(g, r)?;
        let encoded_ms = started.elapsed().as_secs_f32() * 1000.0;
        let mut slowest = (0.0f32, String::new());
        let mut own_bytes = 0;
        let fields = raw.iter().map(|(key, bytes)| {
            let before = previous.and_then(|(p, decoded)| Some((p.fields.get(key)?, decoded.get(key)?)));
            let field_started = std::time::Instant::now();
            let blob = next_blob(before, bytes);
            if !before.is_some_and(|(b, _)| Arc::ptr_eq(b, &blob)) {
                own_bytes += blob.stored() + key.len();
            }
            let ms = field_started.elapsed().as_secs_f32() * 1000.0;
            if ms > slowest.0 {
                slowest = (ms, key.clone());
            }
            (key.clone(), blob)
        }).collect();
        let previous = previous.map(|(p, _)| p);
        let tissue = g.crew_corpse_render.borrow().as_ref().and_then(|c| c.tissue_state(previous.and_then(|p| p.tissue.as_ref())));
        if let Some(state) = &tissue {
            for (peer, body, _) in &state.actors {
                let shared = previous.and_then(|p| p.tissue.as_ref())
                    .and_then(|t| t.actors.iter().find(|(q, _, _)| q == peer))
                    .is_some_and(|(_, b, _)| Arc::ptr_eq(b, body));
                if !shared {
                    own_bytes += body.present.len() + body.broken.len();
                }
            }
            own_bytes += state.pieces.iter().map(|p| std::mem::size_of_val(p) + p.cells.len() * 4).sum::<usize>();
        }
        if std::env::var_os("KESSEL_FEA_REWIND_TRACE").is_some() {
            println!("[fea-range] checkpoint in {:.1} ms: encoded in {encoded_ms:.1} ms, slowest field {} {:.1} ms",
                started.elapsed().as_secs_f32() * 1000.0, slowest.1, slowest.0);
        }
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        own_bytes += g.surface_blood.copy_bytes();
        Ok((Self { id, fields, tissue, receipts: g.fea.receipts.clone(), current: g.fea.current.clone(),
            unsaved: Arc::new(Unsaved::capture(g)?),
            blood: BloodKept::Copy(Arc::new(g.surface_blood.clone())), clock_s: r.time, own_bytes, shot_t: 0.0, tape_len: 0 }, raw))
    }

    /// Every field's bytes.
    fn decoded(&self) -> BTreeMap<String, Vec<u8>> {
        self.fields.iter().map(|(k, v)| (k.clone(), v.decode())).collect()
    }

    /// A checksum per field (the tissue's cells and pieces too): two moments agree when these do.
    pub(super) fn prints(&self) -> BTreeMap<String, u64> {
        use crate::expedition_world_save::checksum;
        let mut out: BTreeMap<String, u64> = self.decoded().iter().map(|(k, v)| (k.clone(), checksum(v))).collect();
        if let Some(state) = &self.tissue {
            for (peer, body, helmet) in &state.actors {
                let bits: Vec<u8> = body.present.iter().chain(&body.broken).map(|&b| b as u8).collect();
                out.insert(format!("tissue/{peer}.cells"), checksum(&bits));
                out.insert(format!("tissue/{peer}.helmet"), checksum(format!("{helmet:?}").as_bytes()));
            }
            out.insert("tissue/pieces".into(), checksum(format!("{:?}", state.pieces).as_bytes()));
        }
        out.insert("fea/receipts".into(), checksum(format!("{:?}", self.receipts).as_bytes()));
        out.insert("renderer/time".into(), u64::from(self.clock_s.to_bits()));
        self.unsaved.prints(&mut out);
        out.insert("environment/surface_blood".into(), self.blood.bytes().map_or(0, |b| checksum(&b)));
        out
    }

    /// Put the game back as this checkpoint holds it. Only the owners and parts that differ from
    /// the present state are assigned (the heavy world owners, trees and buildings, rarely do).
    pub(super) fn restore(&self, g: &mut Game, r: &mut Renderer) -> anyhow::Result<()> {
        r.time = self.clock_s;
        // A casualty's tissue still solving on its worker belongs to the moment left: let go.
        g.deferred_carves.clear();
        let started = std::time::Instant::now();
        let now = encode(g, r)?;
        let fields = self.decoded();
        let trace = std::env::var_os("KESSEL_FEA_REWIND_TRACE").is_some();
        let mut restored: Vec<String> = Vec::new();
        let differs = |owner: &str| {
            let prefix = format!("{owner}/");
            let ours = fields.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix));
            let theirs = now.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix));
            !ours.eq(theirs)
        };
        if differs("physical") {
            restored.push(format!("physical {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.restore_physical(owner_fields(&fields, "physical"))?;
        }
        if differs("tanks") {
            restored.push(format!("tanks {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.restore_tanks(owner_fields(&fields, "tanks"))?;
        }
        if differs("personal") {
            restored.push(format!("personal {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.restore_personal(owner_fields(&fields, "personal"))?;
        }
        if differs("projectiles") {
            restored.push(format!("projectiles {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.restore_projectiles(owner_fields(&fields, "projectiles"))?;
        }
        if differs("infantry") {
            restored.push(format!("infantry {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.restore_infantry(owner_fields(&fields, "infantry"))?;
        }
        if differs("environment") {
            restored.push(format!("environment {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            crate::restore_world_fields!(g, owner_fields(&fields, "environment"); occluders, story_cover,
                scatter_heavy_absorbed, destructibles, chunks, interior_fire_lights, world_fire_lights);
        }
        g.surface_blood = self.blood.get()?;
        let mut x = owner_fields(&fields, "x");
        let changed = |key: &str| now.get(&format!("x/{key}")) != fields.get(&format!("x/{key}"));
        let mut world = owner_fields(&fields, "renderer");
        if differs("renderer") {
            r.pressure_fronts = world.take("pressure_fronts")?;
            r.weather_wind_world = world.take("weather_wind_world")?;
        }
        if changed("loose") {
            restored.push(format!("loose {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.loose = x.take::<Vec<LooseShell>>("loose")?.try_into().map_err(|_| anyhow::anyhow!("loose round capacity"))?;
        }
        if changed("panzerfaust_rack_taken") {
            restored.push(format!("panzerfaust_rack_taken {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.panzerfaust_rack_taken = x.take("panzerfaust_rack_taken")?;
        }
        if changed("inf_flash_t") || changed("inf_flash_pos") {
            restored.push(format!("inf_flash_t {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.inf_flash_t = x.take("inf_flash_t")?;
            g.inf_flash_pos = x.take("inf_flash_pos")?;
        }
        if changed("tiger_refuel") {
            restored.push(format!("tiger_refuel {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.tiger_refuel.state = x.take("tiger_refuel")?;
        }
        if changed("first_person") {
            restored.push(format!("first_person {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            if let (Some(arms), Some(p)) = (x.take::<Option<StateFields>>("first_person")?, g.first_person.as_ref()) {
                p.import_world_state(arms)?;
            }
        }
        if changed("track_snow") {
            restored.push(format!("track_snow {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.track_snow.import_world_state(x.take("track_snow")?)?;
        }
        if changed("fea") {
            restored.push(format!("fea {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            // Deferred plate solves start again from their problems (bit-exact whatever the
            // threads) and commit at their kept times.
            g.fea.import_world_state(x.take("fea")?)?;
        }
        g.fea.receipts = self.receipts.clone();
        g.fea.current = self.current.clone();
        self.unsaved.restore(g)?;
        if changed("hand_props") {
            restored.push(format!("hand_props {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            if let (Some(props), Some(p)) = (x.take::<Option<StateFields>>("hand_props")?, g.hand_props.as_mut()) {
                p.import_world_state(props)?;
            }
        }
        if changed("hot_fragments") {
            restored.push(format!("hot_fragments {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.hot_fragments = x.take::<Vec<HotFragment>>("hot_fragments")?.try_into().map_err(|_| anyhow::anyhow!("fragment capacity"))?;
        }
        if changed("weapon_vfx") {
            restored.push(format!("weapon_vfx {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.weapon_vfx.import_world_state(x.take("weapon_vfx")?)?;
        }
        if changed("cabin_fire") {
            restored.push(format!("cabin_fire {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.cabin_fire.import_world_state(x.take("cabin_fire")?)?;
        }
        if changed("ammo_fire") {
            restored.push(format!("ammo_fire {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.ammo_fire = x.take("ammo_fire")?;
        }
        if changed("scene_environment") {
            restored.push(format!("scene_environment {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.scene_environment.import_world_state(x.take("scene_environment")?)?;
        }
        // The sound's trace job and cache start empty from every checkpoint (its import clears
        // them, as a loaded save's does), so every run from one checkpoint traces alike.
        g.acoustics.import_world_state(x.take("acoustics")?)?;
        if changed("trees") {
            restored.push(format!("trees {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.trees.import_world_state(x.take("trees")?)?;
            g.trees.present_world_state(r);
        }
        if changed("scatter") {
            restored.push(format!("scatter {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.scatter.import_world_state(x.take("scatter")?)?;
            g.scatter.present_world_state(r);
        }
        if changed("buildings") {
            restored.push(format!("buildings {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            let state = x.take("buildings")?;
            let hf = &g.hf;
            g.buildings.import_world_state(state, &|p| hf.height_at(p.x, p.y))?;
        }
        if changed("world_doors") {
            restored.push(format!("world_doors {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            g.world_doors.import_world_state(x.take("world_doors")?)?;
            g.world_doors.upload(r);
        }
        if changed("camp_people") {
            restored.push(format!("camp_people {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            if let (Some(state), Some(camp)) = (x.take::<Option<StateFields>>("camp_people")?, g.camp_people.as_mut()) {
                camp.import_world_state(state)?;
                camp.present_world_state(r)?;
            }
        }
        if changed("terrain_detail") {
            restored.push(format!("terrain_detail {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            if let (Some(block), Some(detail)) = (x.take::<Option<crate::expedition_world_save::OwnerBlock>>("terrain_detail")?, g.hf.detail.as_ref()) {
                detail.import_world_state(&block)?;
                r.invalidate_terrain_detail();
            }
        }
        let crew_keys = ["crew", "singleplayer_crew", "singleplayer_crew_active", "crew_solo", "driver_controls",
            "crew_arm_animation", "living_wounds", "couch_kia_peers", "couch_casualties", "couch_ragdolls",
            "crew_body_recovery", "couch_last_casualty", "crew_panel_selected", "crew_panel_notice",
            "crew_contact_counts"];
        if crew_keys.iter().any(|k| changed(k)) {
            restored.push(format!("crew {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            // As the world save's crew owner puts them back (`import_crew_world_state`). The
            // corpse drawing's pages are renderer allocations, made once and kept: not put back.
            g.crew = x.take("crew")?;
            g.singleplayer_crew = x.take("singleplayer_crew")?;
            g.singleplayer_crew_active = x.take("singleplayer_crew_active")?;
            g.crew_solo = x.take("crew_solo")?;
            g.driver_controls = x.take("driver_controls")?;
            g.crew_arm_animation = CrewArmAnimation::restore(x.take("crew_arm_animation")?)?;
            g.living_wounds.restore(x.take("living_wounds")?)?;
            g.couch_kia_peers = x.take("couch_kia_peers")?;
            g.couch_casualties = x.take("couch_casualties")?;
            g.couch_ragdolls = x.take("couch_ragdolls")?;
            let mut recovery: crew_body_recovery::Recovery = x.take("crew_body_recovery")?;
            recovery.preserve_aid_source_from(&g.crew_body_recovery)?;
            g.crew_body_recovery = recovery;
            g.couch_last_casualty = x.take("couch_last_casualty")?;
            g.crew_panel_selected = x.take("crew_panel_selected")?;
            g.crew_panel_notice = x.take("crew_panel_notice")?;
            g.crew_personal_space.invalidate_world_cache();
            let space = &mut g.crew_personal_space;
            (space.candidates, space.bodies, space.queries) = x.take("crew_contact_counts")?;
            g.present_restored_crew_world_state(r);
        }
        if let Some(corpses) = g.crew_corpse_render.get_mut().as_mut() {
            match &self.tissue {
                Some(state) => corpses.restore_tissue_state(state),
                None => corpses.reset_tissue(),
            }
        }
        if trace {
            println!("[fea-range] put back in {:.0} ms: {restored:?} (each at its start)", started.elapsed().as_secs_f32() * 1000.0);
        }
        Ok(())
    }
}

/// The present run's checkpoints, and the fresh Tiger's.
#[derive(Default)]
pub(super) struct Rewind {
    /// The settled fresh Tiger for one setup (`Range::fresh` puts it back instead of building it
    /// again).
    pub(super) fresh: Option<(String, Checkpoint)>,
    /// Just after the shot was fired (before run frame 0).
    pub(super) fired: Option<Checkpoint>,
    /// At the end of each run frame, where kept.
    pub(super) frames: Vec<Option<Checkpoint>>,
    /// Bytes the run's checkpoints hold.
    pub(super) bytes: usize,
    pub(super) capture_s: f32,
    pub(super) captures: u32,
    /// Each kept frame's own update time when first played.
    step_s: Vec<f32>,
    /// The last checkpoint taken and its fields' bytes (the next one is kept as its changes).
    last: Option<(u64, BTreeMap<String, Vec<u8>>)>,
    /// The run's blood copies being encoded (`BloodWorker`), and the run they belong to.
    blood: Option<BloodWorker>,
    run_serial: u64,
    /// Off: backward seeks re-run the tape from the fresh Tiger (the harness's determinism check).
    pub(super) disabled: bool,
}

/// A frame whose own update took longer than this is not stepped again on a seek: its checkpoint
/// is put back and drawn from an update of `PICTURE_STEP_S`, then put back again.
pub(super) const SLOW_FRAME_S: f32 = 0.5;
pub(super) const PICTURE_STEP_S: f32 = 1.0e-6;

/// Fields of the listener's side that a re-run does not repeat and the strike does not read (the
/// sound's room tracing keeps a per-frame job and cache the world save does not; the player's
/// ears and arms follow the sound). Named, and left out of
/// the agreement checks only.
pub(super) const LISTENER_FIELDS: [&str; 3] = ["x/acoustics", "personal/hearing", "x/first_person"];

/// The fields two moments' prints part in, the listener's side left out.
pub(super) fn parted(a: &BTreeMap<String, u64>, b: &BTreeMap<String, u64>) -> Vec<String> {
    a.keys().chain(b.keys()).collect::<std::collections::BTreeSet<_>>().into_iter()
        .filter(|k| !LISTENER_FIELDS.contains(&k.as_str()) && a.get(*k) != b.get(*k)).cloned().collect()
}

/// The run's checkpoints are kept while they hold less than this; past it, every
/// `SPARSE_EVERY`-th frame only (a seek then steps on from the last kept one). The owner's machine
/// has 128 GB (2026-11-03: "budget is large").
pub(super) const REWIND_BUDGET_BYTES: usize = 32_000_000_000;
/// Below this the blood stays a copy in every checkpoint (about 12 MB each: faster to put back,
/// and no worker encoding beside the simulation); past it each new copy is encoded and patched on
/// the worker (`BloodWorker`).
pub(super) const ENCODE_BLOOD_ABOVE_BYTES: usize = 8_000_000_000;
pub(super) const SPARSE_EVERY: u32 = 8;

impl Rewind {
    /// No run: its checkpoints let go (a reset).
    pub(super) fn clear_run(&mut self) {
        self.frames.clear();
        self.step_s.clear();
        self.fired = None;
        self.bytes = 0;
        self.run_serial += 1;
    }

    /// A new run: its frames' checkpoints dropped, the moment of the shot kept.
    pub(super) fn start_run(&mut self, g: &Game, r: &Renderer, total: u32, tape_len: usize) {
        self.frames = (0..total).map(|_| None).collect();
        self.step_s = vec![0.0; total as usize];
        self.bytes = 0;
        self.run_serial += 1;
        self.fired = self.capture(g, r, None, 0.0, tape_len);
    }

    fn capture(&mut self, g: &Game, r: &Renderer, previous: Option<&Checkpoint>, shot_t: f32, tape_len: usize) -> Option<Checkpoint> {
        let started = std::time::Instant::now();
        let decoded = previous.map(|p| match &self.last {
            Some((id, raw)) if *id == p.id => std::borrow::Cow::Borrowed(raw),
            _ => std::borrow::Cow::Owned(p.decoded()),
        });
        let checkpoint = match Checkpoint::capture(g, r, previous.zip(decoded.as_deref())) {
            Ok((c, raw)) => {
                self.last = Some((c.id, raw));
                Checkpoint { shot_t, tape_len, ..c }
            }
            Err(e) => {
                eprintln!("[fea-range] no checkpoint: {e:#}");
                return None;
            }
        };
        self.capture_s += started.elapsed().as_secs_f32();
        self.captures += 1;
        self.bytes += checkpoint.own_bytes;
        Some(checkpoint)
    }

    /// The latest kept checkpoint at or before frame `k` (`None` for the moment of the shot).
    fn latest(&self, k: u32) -> Option<(Option<u32>, &Checkpoint)> {
        (0..=k.min(self.frames.len().saturating_sub(1) as u32)).rev()
            .find_map(|i| self.frames.get(i as usize)?.as_ref().map(|c| (Some(i), c)))
            .or_else(|| self.fired.as_ref().map(|c| (None, c)))
    }

    /// Keep frame `k`'s state (the game at its end), unless the budget says this frame is skipped.
    pub(super) fn keep(&mut self, g: &Game, r: &Renderer, k: u32, shot_t: f32, tape_len: usize, step_s: f32) {
        if self.frames.get(k as usize).is_none_or(|c| c.is_some()) {
            return;
        }
        self.step_s[k as usize] = step_s;
        // A frame that ends with a casualty's tissue solving on its worker (taken back at the next
        // update's start) is not kept: a seek steps through it from the frame before.
        if g.casualty_tissue_pending() {
            return;
        }
        if self.bytes > REWIND_BUDGET_BYTES && k % SPARSE_EVERY != 0 {
            return;
        }
        let previous = self.latest(k.saturating_sub(1)).filter(|(i, _)| k > 0 || i.is_none()).map(|(_, c)| c.clone());
        let checkpoint = self.capture(g, r, previous.as_ref(), shot_t, tape_len);
        if let Some(BloodKept::Copy(blood)) = checkpoint.as_ref().map(|c| &c.blood).filter(|_| self.bytes > ENCODE_BLOOD_ABOVE_BYTES) {
            let worker = self.blood.get_or_insert_with(BloodWorker::start);
            if worker.jobs.send((self.run_serial, k, blood.clone())).is_ok() {
                worker.pending += 1;
            }
        }
        self.frames[k as usize] = checkpoint;
        self.collect_blood(false);
    }

    /// Put the encoded blood that came back in its checkpoints, in place of the copies; with
    /// `wait`, until none is left out.
    pub(super) fn collect_blood(&mut self, wait: bool) {
        let Some(worker) = self.blood.as_mut() else { return };
        while worker.pending > 0 {
            let next = if wait { worker.done.recv().ok() } else { worker.done.try_recv().ok() };
            let Some((run, frame, blob, own)) = next else { break };
            worker.pending -= 1;
            if run != self.run_serial {
                continue;
            }
            if let Some(Some(c)) = self.frames.get_mut(frame as usize) {
                self.bytes = self.bytes.saturating_sub(c.blood.held()) + own;
                c.blood = BloodKept::Encoded(blob);
            }
        }
    }

    /// The checkpoint to start from to reach frame `k` by stepping: the latest kept before it.
    /// Returns the frame it holds (`None`: the moment of the shot).
    pub(super) fn before(&self, k: u32) -> Option<(Option<u32>, &Checkpoint)> {
        if self.disabled {
            return None;
        }
        match k.checked_sub(1) {
            Some(prev) => self.latest(prev),
            None => self.fired.as_ref().map(|c| (None, c)),
        }
    }

    /// Frame `k`'s own checkpoint's prints, to compare a stepped frame with.
    pub(super) fn prints_at(&self, k: u32) -> Option<BTreeMap<String, u64>> {
        self.frames.get(k as usize)?.as_ref().map(Checkpoint::prints)
    }

    pub(super) fn kept(&self) -> usize {
        self.frames.iter().filter(|c| c.is_some()).count()
    }

    /// The run's checkpoint bytes by field (each copy counted once): what the memory goes to.
    pub(super) fn bytes_by_field(&self) -> Vec<(String, usize)> {
        let mut seen = std::collections::HashSet::new();
        let mut by: BTreeMap<String, usize> = BTreeMap::new();
        for c in self.fired.iter().chain(self.frames.iter().flatten()) {
            for (k, v) in &c.fields {
                if seen.insert(Arc::as_ptr(v) as usize) {
                    *by.entry(k.clone()).or_default() += v.stored();
                }
            }
            if let Some(t) = &c.tissue {
                for (_, body, _) in &t.actors {
                    if seen.insert(Arc::as_ptr(body) as usize) {
                        *by.entry("tissue/bodies".into()).or_default() += body.present.len() + body.broken.len();
                    }
                }
                *by.entry("tissue/pieces".into()).or_default() += t.pieces.iter().map(|p| std::mem::size_of_val(p) + p.cells.len() * 4).sum::<usize>();
            }
            match &c.blood {
                BloodKept::Copy(blood) => *by.entry("environment/surface_blood (copies)".into()).or_default() += blood.copy_bytes(),
                BloodKept::Encoded(blob) => if seen.insert(Arc::as_ptr(blob) as usize) {
                    *by.entry("environment/surface_blood".into()).or_default() += blob.stored();
                },
            }
        }
        let mut by: Vec<(String, usize)> = by.into_iter().collect();
        by.sort_by(|a, b| b.1.cmp(&a.1));
        by
    }

    /// How long frame `k`'s own update took when it was first played (0 when not kept).
    pub(super) fn step_s(&self, k: u32) -> f32 {
        self.step_s.get(k as usize).copied().unwrap_or(0.0)
    }

    /// Frame `k`'s own checkpoint.
    pub(super) fn at(&self, k: u32) -> Option<&Checkpoint> {
        self.frames.get(k as usize)?.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every field of `Game` is held by a checkpoint (a world-save owner `encode` captures, or read
    /// from the game in this file's `encode` or `Unsaved`) or named in `NOT_KEPT` with its reason:
    /// a field added to `Game` fails here until it is one or the other (the owner, 2026-11-03: a
    /// thrown turret and a cook-off outlived every reset because neither was held).
    #[test]
    fn every_game_field_is_kept_or_named() {
        let game = std::fs::read_to_string("src/game.rs").unwrap();
        let start = game.find("\npub struct Game {").unwrap();
        let body = &game[start..start + game[start..].find("\n}\n").unwrap()];
        let fields: Vec<String> = body.lines().skip(1).filter_map(|line| {
            let rest = line.strip_prefix("    ")?;
            if rest.starts_with(' ') || rest.starts_with("//") || rest.starts_with('#') {
                return None;
            }
            let rest = rest.strip_prefix("pub(crate) ").or_else(|| rest.strip_prefix("pub(super) "))
                .or_else(|| rest.strip_prefix("pub ")).unwrap_or(rest);
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            (!name.is_empty() && rest[name.len()..].trim_start().starts_with(':') && !rest[name.len()..].trim_start().starts_with("::"))
                .then_some(name)
        }).collect();
        assert!(fields.len() > 400, "Game's fields not found ({})", fields.len());
        let saved = std::fs::read_to_string("src/game_world_state.rs").unwrap();
        let mut kept: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for owner in ["capture_physical", "capture_tanks", "capture_personal", "capture_projectiles", "capture_infantry",
            "capture_environment"] {
            let at = saved.find(&format!("fields_owner!({owner},")).unwrap();
            let list = &saved[at..][saved[at..].find(';').unwrap() + 1..];
            kept.extend(list[..list.find(");").unwrap()].split(',').map(|f| f.trim().to_string()).filter(|f| !f.is_empty()));
        }
        let ours = std::fs::read_to_string("src/game_fea_range/rewind.rs").unwrap();
        let ours = &ours[..ours.find("#[cfg(test)]").unwrap()];
        for (i, _) in ours.match_indices("g.") {
            let name: String = ours[i + 2..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            kept.insert(name);
        }
        kept.extend(NOT_KEPT.iter().map(|(f, _)| f.to_string()));
        let missing: Vec<&String> = fields.iter().filter(|f| !kept.contains(*f)).collect();
        assert!(missing.is_empty(), "Game fields neither kept by a checkpoint nor named in NOT_KEPT: {missing:?}");
        let stale: Vec<&str> = NOT_KEPT.iter().map(|(f, _)| *f).filter(|f| !fields.iter().any(|g| g == f)).collect();
        assert!(stale.is_empty(), "NOT_KEPT names fields Game no longer has: {stale:?}");
    }

    /// The environment owner's fields, as `encode` lists them with the surface blood apart, are
    /// the world save's (`game_world_state`): a field added there is not missed here.
    #[test]
    fn the_environment_fields_are_the_world_saves() {
        let saved = std::fs::read_to_string("src/game_world_state.rs").unwrap();
        let start = saved.find("fields_owner!(capture_environment,restore_environment,\"world.environment\";").unwrap();
        let list = &saved[start..][saved[start..].find(';').unwrap() + 1..];
        let list = &list[..list.find(");").unwrap()];
        let mut theirs: Vec<&str> = list.split(',').map(str::trim).filter(|f| !f.is_empty()).collect();
        let ours = std::fs::read_to_string("src/game_fea_range/rewind.rs").unwrap();
        let start = ours.find("capture_world_fields!(g; ").unwrap() + "capture_world_fields!(g; ".len();
        let mut mine: Vec<&str> = ours[start..start + ours[start..].find(')').unwrap()].split(',').map(str::trim).collect();
        mine.push("surface_blood");
        theirs.sort();
        mine.sort();
        assert_eq!(mine, theirs);
    }

    /// A chain of patches gives back every version's bytes exactly: bytes changed in place,
    /// inserted and removed.
    #[test]
    fn patches_give_back_every_version() {
        let mut versions = vec![(0..50_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect::<Vec<u8>>()];
        for v in 1..20u32 {
            let mut next = versions.last().unwrap().clone();
            for k in 0..(v * 37) as usize {
                let at = (k * 131 + v as usize * 17) % next.len();
                next[at] = next[at].wrapping_add(1 + v as u8);
            }
            let at = (v as usize * 7919) % next.len();
            if v % 3 == 0 {
                next.drain(at..at + 100);
            } else {
                next.splice(at..at, (0..60).map(|i| (i * v) as u8));
            }
            versions.push(next);
        }
        let mut blob = Arc::new(Blob::Whole { bytes: versions[0].clone(), wait: 0 });
        let mut chain = vec![blob.clone()];
        for pair in versions.windows(2) {
            let ops = diff(&pair[0], &pair[1]).expect("a small change is a patch");
            assert!(ops.len() * 4 < pair[1].len(), "{} bytes of ops for {}", ops.len(), pair[1].len());
            blob = Arc::new(Blob::Patch { prev: blob.clone(), depth: blob.depth() + 1, ops });
            chain.push(blob.clone());
        }
        for (blob, bytes) in chain.iter().zip(&versions) {
            assert_eq!(&blob.decode(), bytes);
        }
        let other: Vec<u8> = (0..50_000u32).map(|i| (i.wrapping_mul(40_503) >> 7) as u8).collect();
        assert!(diff(&versions[0], &other).is_none(), "unrelated bytes are kept whole");
    }
}
