//! Game state: the world scene, the player Tiger, a swarm of T-34s, firing, and the FX. Owns the
//! mapping from game entities to renderer mesh slots and pushes transforms each frame. This is the
//! first-playable loop: drive the Tiger across the Stalingrad snow, lay the gun on the T-34s in the
//! fog, fire, kill them. Each enemy is an independent shooter for now — coordinated swarm AI
//! (flank/converge) is the next step.

use glam::{Mat4, Quat, Vec2, Vec3};

use crate::foot::{FootSoldier, Pickup, PickupKind};
use crate::gltf_load::CpuMesh;
use crate::infantry::{Alert, NoiseEvent, Soldier};
use crate::prims::box_mesh;
use crate::renderer::{DayPhase, PointLight, Renderer};
use crate::tank::{self, EnemyKind, Tank, GUN_LEN};
use crate::terrain::{self, Heightfield, TerrainConfig};

// Stealth / detection system (per-enemy detection meters + platoon alert state machine). Kept in a
// sibling file, pulled in as a private submodule of `game` (no main.rs change needed).
#[path = "stealth.rs"]
mod stealth;
use stealth::AlertState;

/// Enemy behaviour mode this frame, derived from the stealth state: an unaware tank patrols and only
/// scans, a searching one sweeps toward a last-known area, a tracking one lays on and fires the
/// player. This is the whole diegetic tell — the turret only swings onto you once you've been made.
#[derive(Copy, Clone, PartialEq, Eq)]
enum AiMode {
    Patrol, // Unaware — slow advance, scanning, never fires the player
    Search, // Searching — hunts the platoon's last-known area, sweeping the turret, never fires
    Track,  // spotted / Engaged — flanks + fires the player (the classic combat behaviour)
}

/// ABSITZEN: which body the player is driving — the Tiger, or the dismounted soldier.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Tank,
    Foot,
}

/// The embodied outside-track ritual.  Progress survives if the player has to duck away from the
/// exposed flank, but every phase still has to be performed at the broken run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TrackRepairPhase {
    #[default]
    GrabEnd,
    PullEnd,
    FitLink,
    HammerPin,
}

/// Number of pre-allocated supply pickups (fixed at build; the renderer scene can't grow).
const N_PICKUPS: usize = 14;

/// ABSITZEN enemy infantry: pre-allocated soldier slots (2 meshes each) + return-fire muzzle pool.
const N_SOLDIERS: usize = 8;
const INF_FX: usize = 3; // infantry muzzle-flash boxes, cycled round-robin like the enemy FX pool
const ALARM_RADIUS: f32 = 40.0; // m — a blown encounter cascades to neighbours within this
const INF_DAMAGE: f32 = 34.0; // hp per infantry round that connects (~3 = down)
const ENEMY_RELOAD: f32 = 5.5; // s — per-enemy cadence (the swarm's volume of fire = the pressure)
const N_ENEMIES: usize = 6; // swarm size (independent shooters for now)
const E_FX_POOL: usize = 6; // enemy tracer/impact FX slots, cycled round-robin so shots overlap
const MAX_CAMPAIGN_HULKS: usize = 24; // enough to retain every pre-final-wave campaign kill
const NEAR_MISS_RADIUS: f32 = 9.0; // m — an incoming round passing this close (but missing) cracks past
                                   // A missed round whose IMPACT POINT buries itself this close to the player kicks up a snow/dust PUFF
                                   // and cracks a near-miss impact — so incoming fire snaps around you. (Distinct from NEAR_MISS_RADIUS,
                                   // which is the round's closest approach in FLIGHT; this is where it actually LANDS.)
const NEAR_MISS_PUFF_RADIUS: f32 = 8.0; // m — impact-to-player distance for a snow-kick near-miss
const N_SNOW: usize = 6; // pooled snow/dust near-miss puff slots, cycled round-robin

// 6x-ish Dienstglas framing. The old 4° field was effectively a 17-18x screen-space zoom from the
// normal commander/on-foot views: powerful enough to amplify every distant render artifact and too
// tight to scan with. Ten degrees keeps the binoculars stronger than the 5x TZF stop while retaining
// enough context to search the horizon.
const BINOCULAR_FOV_DEG: f32 = 10.0;

// ── Coaxial + hull MG34 (rapid automatic fire) ──────────────────────────────────────────────────
// The two MG34s: the coaxial (fired down the gunner's sight, on the main-gun axis) and the hull/bow
// gun (the radio-operator's forward MG). RAPID automatic fire paced by a firing-rate timer so rounds
// aren't spawned one-per-frame. MGs are USELESS against armour — they only clatter + spark off a
// T-34; they are lethal to SOFT targets (infantry — none in the tank scene yet; see `mg_hit`).
const MG_FIRE_INTERVAL: f32 = 0.075; // s between rounds — ~13 rounds/sec (MG34 cyclic-rate feel)
const MG_TRACER_EVERY: u32 = 2; // every Nth round is a visible tracer (classic tracer stream)
const MG_TRACER_SPEED: f32 = 700.0; // m/s — thin, fast streaks (vs the 450 m/s cannon shell)
const MG_RANGE: f32 = 900.0; // m — MG hitscan reach
const MG_DISP: f32 = 0.010; // fire-cone dispersion (MGs spray, they don't lay a precise bolt)
const MG_FLASH_DUR: f32 = 0.065; // s — muzzle-flash persistence (≈ the fire interval, so it reads
                                 // as a near-continuous strobing flash while firing, not stutter)
const N_MG_TRACER: usize = 8; // pooled MG tracer streaks (coax + hull share; one MG at a time)
const N_MG_IMPACT: usize = 8; // pooled MG impact spurts (armour sparks / snow-dirt kicks)
const MG_SOUND_EVERY: u32 = 4; // only every Nth round plays an impact sound (don't flood sinks)
const MG_HEAT_PER_SEC: f32 = 0.28; // barrel heat gained per second of sustained fire (→ ~3.5 s to cook)
const MG_COOL_PER_SEC: f32 = 0.45; // heat shed per second when not firing
const MG_COOL_RESUME: f32 = 0.35; // once overheated, must cool below this before it fires again
const VOL_MG_GRAVEL: f32 = 0.55; // rounds clattering off a T-34's armour — light, not a cannon CLANG
const VOL_MG_GROUND: f32 = 0.40; // rounds spitting up snow/dirt on the terrain

// ── Speed of sound: distant combat is SEEN, then HEARD a beat later ─────────────────────────────
// Every combat sound whose SOURCE is away from the player (enemy gunfire, kill/turret/cook-off
// explosions, impacts) is queued and released only once its wavefront reaches the player: delay =
// range / 343 m/s, volume attenuated by range. The player's OWN main gun bypasses this (instant —
// you're right there). Ambient horizon explosions ride the same queue at long delay + low volume.
const SPEED_OF_SOUND: f32 = 343.0; // m/s
const SOUND_REF_DIST: f32 = 180.0; // m — volume falls off as ref/(ref+range); ~half at this range
                                   // Combat SFX base volumes (before range attenuation). Nudged above unity so incoming/outgoing fire
                                   // reads CLEARLY against the deliberately near-silent engine+wind mix (the CLANGs and cracks are the
                                   // point). Close sources (enemy rounds landing on the Tiger) stay ~full; distant ones fall off.
const VOL_ENEMY_GUN: f32 = 1.2;
const VOL_IMPACT_TANK: f32 = 1.15;
const VOL_IMPACT_GROUND: f32 = 0.9;
const VOL_CLANG: f32 = 1.25; // the CLANG of a bounce should ring out clearly
const VOL_NEAR_MISS: f32 = 1.25; // the supersonic crack of a round snapping past
const VOL_PENETRATION: f32 = 1.2;
const VOL_KILL: f32 = 1.5; // big kill / turret-pop / cook-off booms carry across the field
const VOL_AMBIENT_EXP: f32 = 1.5; // distant battlefield boom base — heavily range-attenuated
                                  // Ambient battlefield explosions: a faint, delayed boom out on the horizon around the player at
                                  // randomized intervals, so the whole front reads as a battle in progress (not just this duel).
const AMBIENT_EXP_MIN: f32 = 6.0; // s — shortest gap between distant explosions
const AMBIENT_EXP_MAX: f32 = 20.0; // s — longest gap (the spread keeps the front feeling alive)
const AMBIENT_DIST_MIN: f32 = 380.0; // m — distant, out on the horizon
const AMBIENT_DIST_MAX: f32 = 850.0;

// ── Kill-payoff / wreck model ─────────────────────────────────────────────────────────────────
// Destroying a T-34 is the whole point of the fight, so it has to LAND. Every kill leaves a
// persistent BURNING WRECK (orange emissive flicker + a rising smoke column); a chunk of kills go
// CATASTROPHIC — the ammunition detonates, the turret is blown clean off and tumbles away; the rest
// just burn, and some of those cook off their stowed ammunition seconds later with a delayed WHUMP.
const CATASTROPHIC_CHANCE: f32 = 0.35; // fraction of kills that pop the turret (ammo-rack detonation)
const COOKOFF_CHANCE: f32 = 0.55; // of the *burning* (non-pop) wrecks, fraction that later cook off
const COOKOFF_MIN: f32 = 3.0; // s — earliest a burning wreck cooks off
const COOKOFF_MAX: f32 = 8.0; // s — latest
const TURRET_POP_V0_MIN: f32 = 14.0; // m/s — turret launch (vertical) velocity, ammo-rack blast
const TURRET_POP_V0_MAX: f32 = 20.0;
const N_BLAST: usize = 4; // pooled big emissive fireball FX (kills / pops / cook-offs overlap)
const N_DEBRIS: usize = 12; // pooled spark/debris FX flung from a blast
                            // Burning-wreck hull glow. Deliberately HOT (well above the shader's 6.0 white-hot knee) so a wreck
                            // blooms into a clearly-readable orange fire even out at the sniper-range (240–400 m) the swarm
                            // fights from — at that distance a subtle glow just disappears into the snow.
                            // Burn light belongs to flame/smoke, not every square metre of steel. A faint ember lift preserves
                            // silhouette detail at night while the hull itself stays matte, soot-black and physically plausible.
const HULL_GLOW: f32 = 0.28;

/// How a destroyed enemy reads as a wreck.
#[derive(Copy, Clone, PartialEq, Eq, Default)]
enum WreckKind {
    #[default]
    None, // still alive — not a wreck
    Burn,      // hull + turret stay, engulfed in flame (may cook off later)
    TurretPop, // CATASTROPHIC — the turret was blown off and is tumbling ballistically
}

/// Per-enemy wreck state, held in a `Vec<Wreck>` parallel to `Game::enemies`. Inert until that
/// enemy is destroyed (`kind == None`). Drives the persistent burn glow, the tumbling turret-pop
/// physics, and the delayed cook-off.
#[derive(Clone, Default)]
struct Wreck {
    kind: WreckKind,
    burning: bool,
    fire_t: f32, // seconds since the wreck was created (fire flicker + smoke clock)
    // ── turret-pop ballistics (the turret mesh flies off) ──
    t_pos: Vec3, // world position of the tumbling turret ring
    t_vel: Vec3, // integrated under gravity
    t_spin_axis: Vec3,
    t_spin_rate: f32,  // rad/s
    t_ang: f32,        // accumulated tumble angle
    t_base_rot: Quat,  // the turret's world orientation at the instant it popped
    t_gun_local: Mat4, // gun transform relative to the turret (so the barrel flies with it)
    t_rest_y: f32,     // world Y the turret comes to rest at
    landed: bool,
    // ── delayed cook-off ──
    cookoff_t: Option<f32>, // countdown to the secondary; None = never cooks off (or already did)
    cooked: bool,
    cook_flash: f32, // brief fire-intensity spike right after a cook-off, decays to 0
}

impl Wreck {
    /// World transform for the tumbling turret mesh.
    fn turret_pop_xf(&self) -> Mat4 {
        let spin = Quat::from_axis_angle(self.t_spin_axis, self.t_ang);
        Mat4::from_rotation_translation(spin * self.t_base_rot, self.t_pos)
    }
    /// World transform for the gun mesh, rigidly attached to the tumbling turret.
    fn gun_pop_xf(&self) -> Mat4 {
        self.turret_pop_xf() * self.t_gun_local
    }
}

/// One pooled big emissive fireball (kill / turret-pop / cook-off blast).
#[derive(Clone, Default)]
struct Blast {
    t: f32,   // life remaining
    dur: f32, // total life
    pos: Vec3,
    scale: f32, // peak radius
    hot: f32,   // peak emissive intensity
}

/// One pooled snow/dust near-miss puff: a quick white-grey burst that EXPANDS and fades where an
/// incoming round buried itself in the snow beside the player. Non-orange (own-colour emissive path)
/// so it reads as kicked-up snow, not fire.
#[derive(Clone, Default)]
struct SnowPuff {
    t: f32,   // life remaining
    dur: f32, // total life
    pos: Vec3,
    scale: f32, // peak radius (closer near-miss = bigger)
}

/// One pooled MG tracer streak: the thin/fast MG round, animated with the same muzzle→impact lerp
/// as the cannon tracer but small + rapid. Coax and hull share the pool (only one MG fires at once).
#[derive(Clone, Default)]
struct MgTracer {
    active: bool,
    t: f32,
    dur: f32,
    from: Vec3,
    to: Vec3,
    dir: Vec3,
}

/// One pooled MG impact spurt: a quick burst where a round terminates. `tank` = sparks off armour
/// (hot, small, bright); otherwise a snow/dirt kick (softer, larger). Own-colour warm-white so the
/// snow reads as kicked-up powder, not fire (the armour sparks blow white-hot through bloom).
#[derive(Clone, Default)]
struct MgImpact {
    t: f32,
    dur: f32,
    pos: Vec3,
    tank: bool,
}

/// One pooled spark/debris chunk flung from a blast, falling under gravity.
#[derive(Clone, Default)]
struct Spark {
    t: f32, // life remaining
    dur: f32,
    pos: Vec3,
    vel: Vec3,
    size: f32,
}

/// Flickering emissive intensity for a burning wreck: layered sines + hashed grain around `base`,
/// tripled briefly by a cook-off `flash`. Reads as hot, unsteady flame against the cold scene.
fn fire_glow(fire_t: f32, seed: u32, base: f32, flash: f32) -> f32 {
    let s = seed as f32 * 0.618;
    let a = (fire_t * 13.0 + s).sin();
    let b = (fire_t * 27.0 + s * 2.3).sin();
    let n = hash01((fire_t * 50.0) as u32 ^ seed) - 0.5;
    let flick = (0.72 + 0.16 * a + 0.09 * b + 0.20 * n).clamp(0.35, 1.35);
    base * flick * (1.0 + 2.5 * flash)
}

/// Deterministic pseudo-random in [0,1) from a counter (no rand crate — matches the enemy-AI
/// dispersion jitter elsewhere in this file).
fn hash01(n: u32) -> f32 {
    ((n.wrapping_mul(2654435761) >> 8) & 0xffff) as f32 / 65536.0
}

/// Near-miss snow-puff decision from a round's closest-approach horizontal distance to the player:
/// `Some(close)` with close in (0,1] (1 right on the hull, →0 at the radius) when the round snapped
/// within `NEAR_MISS_PUFF_RADIUS`, `None` (no puff — the round passed too wide) otherwise. `close`
/// scales the puff size + sound (closer = bigger/louder). Pure so the radius + scaling is
/// unit-testable without a GPU.
fn near_miss_puff_scale(horiz: f32) -> Option<f32> {
    if horiz <= NEAR_MISS_PUFF_RADIUS {
        Some(1.0 - (horiz / NEAR_MISS_PUFF_RADIUS).clamp(0.0, 1.0))
    } else {
        None
    }
}

/// GLSL-style smoothstep for FX envelopes.
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ── Engine-native destructibility: shells knock down the ruined city's walls/facades ─────────────
// A subset of the tall standing structures — the near city walls/facades + the free-standing cover
// walls — are DESTRUCTIBLE. The scene is FIXED at `Game::build` (no mesh can be added at runtime), so
// each destructible pre-allocates BOTH its intact mesh AND a hidden low-rubble heap at the same
// footprint (see `city::rubble_heap`); a main-gun round that strikes one chips its HP and, at 0 HP,
// COLLAPSES it — the intact is hidden, the rubble revealed, a debris + masonry-dust burst is flung
// from the pooled FX, a heavy collapse boom scheduled, and the piece's LOS occluder DOWNGRADED to
// knee height so a felled wall stops concealing a Tiger. HP scales with size so a big HE/AP round
// drops a small wall in one hit and a large facade in ~three. Deterministic (hash01, no rand).
const STRUCT_SHELL_DAMAGE: f32 = 55.0; // one main-gun HE/AP round vs masonry
const STRUCT_HP_MIN: f32 = 50.0; // smallest wall — one shell drops it
const STRUCT_HP_MAX: f32 = 155.0; // largest facade — ~three shells
const STRUCT_MAX_DESTRUCTIBLES: usize = 44; // hard cap — keep the set a few dozen, cheap
const CITY_DESTRUCT_CAP: usize = 18; // of those, at most this many near-front city wall pieces
const COLLAPSE_OCCLUDER_H: f32 = 0.6; // m — occluder top after collapse (below the tank sightline)

/// One destructible standing structure (a near city wall/facade or a free-standing cover wall). The
/// scene is fixed at build time, so BOTH the intact mesh and a hidden collapsed-rubble heap at the
/// same footprint are pre-allocated up front; destruction just flips their visibility, bursts debris,
/// and downgrades the LOS occluder. All-`Copy` so `collapse_structure` can lift a snapshot out of the
/// `Vec` before mutating the rest of `Game`.
#[derive(Clone, Copy)]
struct Destructible {
    intact: usize, // renderer slot of the standing mesh (hidden on collapse)
    rubble: usize, // renderer slot of the pre-allocated collapsed heap (revealed on collapse)
    occ: usize,    // index into `Game::occluders` — downgraded to knee height on collapse
    min: Vec3,     // world AABB of the intact piece (the shell-impact hit test)
    max: Vec3,
    hp: f32,
    max_hp: f32,
    alive: bool,
    seed: u32,       // deterministic debris/dust spread
    class: Material, // material class — drives the crush gate (and, later, shell response)
}

impl Destructible {
    fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    /// Apply `dmg`; returns true if this hit dropped the piece to rubble (0 HP). A felled piece takes
    /// no further damage. Pure — the FX/occluder consequences live in `collapse_structure`.
    fn hit(&mut self, dmg: f32) -> bool {
        if !self.alive {
            return false;
        }
        self.hp -= dmg;
        if self.hp <= 0.0 {
            self.hp = 0.0;
            self.alive = false;
            true
        } else {
            false
        }
    }
}

/// Scratch record for a destructible collected during scene assembly, before its paired rubble heap
/// is pre-allocated (see `Game::build`). Promoted to a `Destructible` once the rubble slot is known.
struct DestructibleBuild {
    intact: usize,
    occ: usize,
    min: Vec3,
    max: Vec3,
    hp: f32,
    seed: u32,
    class: Material,
}

/// HP for a destructible scaled by its footprint: a small wall segment drops in one shell, a large
/// facade takes ~three. `max_dim` = the piece's largest world extent. Pure so it's unit-testable.
fn structure_hp(max_dim: f32) -> f32 {
    let size01 = ((max_dim - 3.0) / 6.0).clamp(0.0, 1.0); // ~3 m wall → 0, ~9 m facade → 1
    STRUCT_HP_MIN + (STRUCT_HP_MAX - STRUCT_HP_MIN) * size01
}

/// The LOS occluder a wall leaves behind once it collapses: the top is dropped to knee height so the
/// felled wall no longer breaks a tank-centre sightline (matches the "can't hide behind a knee-high
/// brick pile" rule the intact occluders already rely on). Pure so the downgrade is unit-testable.
fn collapsed_occluder(o: stealth::Occluder) -> stealth::Occluder {
    stealth::Occluder {
        min: o.min,
        max: Vec3::new(o.max.x, o.min.y + COLLAPSE_OCCLUDER_H, o.max.z),
    }
}

// ── Material classes + TRACK CRUSH (docs/DESTRUCTION.md §2/§4, slice 1) ──────────────────────────
// Every destructible carries a material class that answers how it responds to being DRIVEN INTO.
// Crush is a momentum gate: the 57-tonne Tiger (crush power 3) grinds through brick, crawls through
// stone, and is stopped dead by bunker concrete; a lighter vehicle treats every tier one step
// harder (enemies aren't wired to crush yet — see §7 of the doc). The signature moment is the
// GRIND: speed clamps against the wall while crush damage flows into the same HP pool the gun
// uses, then the piece collapses through the same `collapse_structure` path and the tank surges
// over the fresh rubble. Shell-side properties (ap_cost/deflection) land with the pen-ray slice.
/// Timber/Stone/Bunker have no act-1 spawners yet (huts and bunkers are act-2/3 world classes) —
/// they're in the table now so the crush gate and its tests cover the full tier ladder.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Debug)]
enum Material {
    Timber, // izba walls, barns, plank barricades — bursts on contact at speed
    Brick,  // the ruined city's walls/facades (every act-1 destructible today)
    Stone,  // heavy masonry — a Tiger-only crawl-grind
    Bunker, // poured concrete — never crushes, never collapses
}

/// How a material meets a crush-power-3 vehicle (the Tiger).
/// `tier`: 1 bursts at speed / 2 the grind / 3 crawl-grind / 4 hard stop.
/// `vmax`: speed cap (m/s) while pressed against the piece. `dps`: crush damage per second.
#[derive(Clone, Copy)]
struct CrushSpec {
    tier: u8,
    vmax: f32,
    dps: f32,
}

impl Material {
    fn crush(self) -> CrushSpec {
        match self {
            Material::Timber => CrushSpec {
                tier: 1,
                vmax: 7.0,
                dps: 400.0,
            },
            Material::Brick => CrushSpec {
                tier: 2,
                vmax: 3.5,
                dps: 65.0,
            },
            Material::Stone => CrushSpec {
                tier: 3,
                vmax: 1.3,
                dps: 30.0,
            },
            Material::Bunker => CrushSpec {
                tier: 4,
                vmax: 0.0,
                dps: 0.0,
            },
        }
    }
}

/// The 88 round from the PBR pixel-tank-shell asset: longest axis aligned to +Z, scaled to the
/// real 8.8 cm KwK 36 round (~0.93 m), bbox-centred, and its GREEN projectile body re-tinted per
/// NATURE (the brass case is left alone): dark grey = PzGr (AP), yellow = SprGr (HE).
fn load_88_round(he: bool) -> Option<crate::gltf_load::CpuMesh> {
    // The game-ready bakes (pre-tinted grey PzGr / yellow SprGr, tools/blender_live.py pipeline),
    // texture-capped to 512 — these clone across 65 rack slots. Long axis (Y) -> +Z, real length.
    let path = if he {
        "assets/88mm/baked/shell_HE.glb"
    } else {
        "assets/88mm/baked/shell_AP.glb"
    };
    normalize_round(crate::gltf_load::load_glb_capped(path, 512).ok()?, 0.93)
}

/// The spent case (solid mouth walls baked in), same normalization at case length.
fn load_88_case() -> Option<crate::gltf_load::CpuMesh> {
    normalize_round(
        crate::gltf_load::load_glb_capped("assets/88mm/baked/shell_case.glb", 256).ok()?,
        0.57,
    )
}

/// Long axis -> +Z, scaled to `len` metres, bbox-centred at the origin.
fn normalize_round(m: crate::gltf_load::CpuMesh, len: f32) -> Option<crate::gltf_load::CpuMesh> {
    let ext = m.max - m.min;
    let rot = if ext.x >= ext.y && ext.x >= ext.z {
        Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
    } else if ext.y >= ext.x && ext.y >= ext.z {
        Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)
    } else {
        Mat4::IDENTITY
    };
    let scale = len / ext.max_element().max(1e-3);
    let center = (m.min + m.max) * 0.5;
    Some(
        m.transformed(Mat4::from_scale(Vec3::splat(scale)) * rot * Mat4::from_translation(-center)),
    )
}

/// Scale a mesh's base-color texture RGB by `f` (clamped) — the fix for dark-authored PSX
/// albedos on props that must READ (the signal-red extinguisher): re-light the albedo, don't
/// fight the cold grade with emissive.
fn brighten_albedo(mut m: crate::gltf_load::CpuMesh, f: f32) -> crate::gltf_load::CpuMesh {
    if let Some(t) = m.texture.as_mut() {
        for px in t.rgba.chunks_exact_mut(4) {
            for c in &mut px[..3] {
                *c = ((*c as f32) * f).min(255.0) as u8;
            }
        }
    }
    m
}

/// Point + direction at `dist` along a piecewise tracer route (transit holes need the exact
/// spot/heading the round punched through at).
fn route_at(route: &[Vec3], dist: f32) -> (Vec3, Vec3) {
    let mut remaining = dist;
    for w in route.windows(2) {
        let len = w[0].distance(w[1]);
        if remaining <= len || len <= 1e-6 {
            let dir = (w[1] - w[0]) / len.max(1e-6);
            return (w[0] + dir * remaining.min(len), dir);
        }
        remaining -= len;
    }
    let last = *route.last().unwrap_or(&Vec3::ZERO);
    let dir = if route.len() >= 2 {
        (last - route[route.len() - 2]).normalize_or_zero()
    } else {
        Vec3::Z
    };
    (last, dir)
}

/// One foam blob: flying from the nozzle, then STUCK where it hit, shrinking out. Turret-local.
#[derive(Clone, Copy, Default)]
struct Foam {
    active: bool,
    stuck: bool,
    pos: Vec3,
    vel: Vec3,
    t: f32, // life remaining (stuck) / flight timeout (flying)
    t0: f32,
    size: f32,
    seed: u32,
}

/// One RTT periscope screen (from tiger_rtt_screens.json): an eyepiece face set whose atlas
/// tile shows what its aperture sees. `frame`: 0 hull / 1 turret / 2 gun.
struct RttScreen {
    frame: usize,
    centroid: Vec3,
    bearing: Vec3,
    tile: u32,
}

/// What one pen-ray hop terminated on (fire-time resolution, docs/DESTRUCTION.md §3).
enum HopHit {
    Enemy(usize),
    Struct(usize),
    HardCover,
    Tree(usize),
    Terrain,
    Nothing,
}

/// Wall-transit properties for the AP pen ray (docs/DESTRUCTION.md §3.1): what one pass through
/// this material costs the round. `cost_mm` spends the pen budget; `deflect_mrad` is the hashed
/// angular kick (miss distance grows with the wall→target gap for free); `disrupt` models yaw +
/// cap stripping — it multiplies the round's effective pen DOWN at the terminal armor hit.
#[derive(Clone, Copy)]
struct ApTransit {
    cost_mm: f32,
    deflect_mrad: f32,
    disrupt: f32,
}

impl Material {
    fn ap(self) -> ApTransit {
        match self {
            Material::Timber => ApTransit {
                cost_mm: 12.0,
                deflect_mrad: 5.0,
                disrupt: 0.0,
            },
            Material::Brick => ApTransit {
                cost_mm: 35.0,
                deflect_mrad: 15.0,
                disrupt: 0.30,
            },
            Material::Stone => ApTransit {
                cost_mm: 70.0,
                deflect_mrad: 25.0,
                disrupt: 0.60,
            },
            Material::Bunker => ApTransit {
                cost_mm: 9999.0,
                deflect_mrad: 0.0,
                disrupt: 1.0,
            },
        }
    }
}

// AP is structurally LIGHT (a hole, not a collapse); HE does the demolition (radial, with
// falloff). One shell type used to flat-rate 55 HP — the natures split it.
const STRUCT_AP_HOLE_DAMAGE: f32 = 15.0; // a clean transit punch-through
const STRUCT_AP_TERMINAL_DAMAGE: f32 = 30.0; // the round DIED in this wall — bigger bite
const HE_STRUCT_DAMAGE: f32 = 85.0; // radial peak at the burst — one-shots a small wall
const HE_BLAST_RADIUS: f32 = 7.0; // m — structure/tree damage radius
const TREE_AP_COST_SMALL: f32 = 3.0; // mm — a fir barely registers on the budget
const TREE_AP_COST_BIG: f32 = 15.0; // mm — a mature trunk eats a little
const PEN_RAY_MAX_HOPS: usize = 6; // walls a single round can chain through

const TIMBER_BURST_SPEED: f32 = 2.2; // m/s — above this, tier-1 pieces burst instead of grinding
const CRUSH_NOSE_R: f32 = 1.4; // m — bow contact radius (Tiger hull is ~3.7 m wide)
const CRUSH_NOSE_LEN: f32 = 3.2; // m — hull pivot → bow, along the direction of travel

/// Bow-circle vs wall-AABB contact in the ground plane: `Some((push, overlap))` when the nose
/// circle penetrates the box, where `push` is the outward XZ unit-ish vector that separates it.
/// Degenerate fully-inside case pushes back along `-dir`. Pure so the contact test is unit-testable.
fn nose_contact(nose: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<(Vec3, f32)> {
    let cx = nose.x.clamp(min.x, max.x);
    let cz = nose.z.clamp(min.z, max.z);
    let (dx, dz) = (nose.x - cx, nose.z - cz);
    let d2 = dx * dx + dz * dz;
    if d2 >= CRUSH_NOSE_R * CRUSH_NOSE_R {
        return None;
    }
    if d2 < 1e-6 {
        // Nose centre is inside the box (a fast frame skipped the skin) — back straight out.
        return Some((-Vec3::new(dir.x, 0.0, dir.z), CRUSH_NOSE_R));
    }
    let d = d2.sqrt();
    Some((Vec3::new(dx / d, 0.0, dz / d), CRUSH_NOSE_R - d))
}

// ── Campaign / skirmish OPENING geometry: spawn on the player's terms, the wind field, the dead ──
// The engagement opens with the Tiger mustered FAR OUT on the open Volga steppe, well SOUTH of the
// ruined city, hull facing NORTH (+Z) toward the ruins — and CRUCIALLY out of contact: at t=0 every
// enemy sits beyond the stationary detection radius (`stealth::RADIUS_STILL`), so a still Tiger is a
// grey ghost the half-blind swarm cannot see and can sit at spawn undetected indefinitely. This is
// deliberate — once T-34s close on a lone Tiger it is over, so the fight MUST start at sniper range on
// the player's terms: you DRIVE IN across the empty snow to bring the enemy into your gunsight, not the
// other way round. The swarm rings the city to the north — the OUTSKIRTS platoon is the near picket
// line the driving player meets first (at long range — Tigers were snipers, so the spacing is held);
// the IN-CITY platoon is dug DEEPER into the streets behind the building cover (partially concealed —
// the stealth LOS respects those occluders), reached only after the approach. The split falls on the
// platoon boundary so the stealth model's contiguous PLATOON_SIZE=3 groups each muster wholly in one
// zone. Frozen bodies thicken toward the city edge, sparse out on the open approach. A cold steppe wind
// blows hard over the open ground and is broken by the city canyon.

/// World (x, z) the player Tiger musters at — FAR out on the open steppe, well south of the built-up
/// area, hull facing +Z (north) toward the city. Held beyond every enemy's stationary detection radius
/// so the fight opens UNDETECTED and the player drives in to make contact (see the scenario note
/// above). Shared by `Game::build` and the spawn-layout test.
const PLAYER_SPAWN: (f32, f32) = (-30.0, -560.0);

/// The parametric city's grid centre (packed x, z), north of the spawn. Shared by `city::generate`
/// (the ruins the player faces) and `wind_at` (the canyon that shelters the wind).
const CITY_CENTER: Vec2 = Vec2::new(-30.0, 110.0);

/// Which combat zone a swarm member musters in. OUTSKIRTS tanks sit out on the open steppe at sniper
/// range (the classic long approach); IN-CITY tanks are dug into the ruins behind the building cover
/// (partially concealed — the stealth LOS already respects those occluders). The split lands on the
/// platoon boundary so each PLATOON_SIZE=3 platoon is wholly one or the other.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum SpawnZone {
    Outskirts,
    InCity,
}

/// Muster points (world x, z) + zone for the swarm — the OPENING array (see the scenario note above).
/// Platoon 0 (0,1,2) = OUTSKIRTS: the near sniper picket ring around the city's southern approach,
/// ~575-580 m out from the far spawn (all beyond the ~450 m stationary detection radius, so the player
/// makes NO contact until it drives in); platoon 1 (3,4,5) = IN-CITY, dug DEEPER into the northern
/// streets among the ruins, ~685-705 m out (met only after pushing into the city). Headings are derived
/// toward the player at spawn time.
const SWARM: [(f32, f32, SpawnZone); N_ENEMIES] = [
    // OUTSKIRTS — the sniper picket ring on the open ground around the city's southern approach; the
    // first (long-range) contact the driving player makes. All ~575-580 m from spawn (> RADIUS_STILL).
    (-160.0, 5.0, SpawnZone::Outskirts), // south-west flank picket
    (110.0, 5.0, SpawnZone::Outskirts),  // south-east flank picket
    (-50.0, 15.0, SpawnZone::Outskirts), // southern approach picket — dead ahead down the drive-in axis
    // IN-CITY — dug DEEPER into the northern streets, behind facades (city occluders partially conceal
    // them). Reached only after the approach; ~685-705 m from spawn (deeper than the outskirts ring).
    (-45.0, 125.0, SpawnZone::InCity), // deep north-west street intersection
    (-15.0, 125.0, SpawnZone::InCity), // deep north-east street intersection
    (-45.0, 143.0, SpawnZone::InCity), // deepest — far north-west street, well into the ruins
];

/// Per-swarm-member vehicle kind, PARALLEL to `SWARM` (kept separate so the spawn-layout test's tuple
/// destructuring is untouched). The two OUTSKIRTS FLANK pickets (#0 SW, #1 SE) are SU-100 tank
/// destroyers standing OVERWATCH at long standoff — the sniper counter to the T-34 rush: their 100 mm
/// D-10S defeats the Tiger's frontal plate at range, and they hold back / reposition instead of
/// charging (see `overwatch_maneuver`). The southern-approach picket (#2) and the whole IN-CITY
/// platoon (#3,4,5) are T-34-76s that advance + flank. This keeps platoon 0 a mixed sniper-screen and
/// platoon 1 a pure rush, without disturbing the PLATOON_SIZE=3 stealth grouping.
const SWARM_KINDS: [EnemyKind; N_ENEMIES] = [
    EnemyKind::Su100, // #0 SW flank — overwatch sniper
    EnemyKind::Su100, // #1 SE flank — overwatch sniper
    EnemyKind::T34,   // #2 southern approach picket
    EnemyKind::T34,   // #3 in-city
    EnemyKind::T34,   // #4 in-city
    EnemyKind::T34,   // #5 in-city
];

// ── SU-100 overwatch-sniper standoff band (metres) ───────────────────────────────────────────────
// The SU-100 is a long-range sniper, NOT a brawler: it holds inside this range band and turns its
// HULL to lay the fixed casemate gun. If the Tiger closes inside the min, it REVERSES to keep the
// range (never charges); if the Tiger drifts beyond the max, it creeps forward to re-establish LOS.
const SU100_STANDOFF_MIN: f32 = 280.0; // m — closer than this ⇒ back off to keep range
const SU100_STANDOFF_MAX: f32 = 520.0; // m — farther than this ⇒ creep in to hold LOS

// ── Zoned wind field ─────────────────────────────────────────────────────────────────────────────
// A cold prevailing steppe wind out of the west, blowing across the open ground toward the city. It
// is STRONGER + gustier in the open and CALMER in the sheltered city canyon (the streets break it).
const WIND_OPEN: f32 = 5.4; // open-steppe wind strength (drift units, not literal m/s)
const WIND_CITY: f32 = 1.7; // sheltered wind deep in the ruined streets
const WIND_SHELTER_R: f32 = 58.0; // ~city-core radius over which the shelter fades to full open wind

/// Local horizontal wind at a world position, as a ground-plane (x, z) vector. Full cold-steppe
/// strength out in the open, dropping to a sheltered breeze inside the city canyon; a slow, purely
/// spatial gust keeps the field from reading dead-uniform. Pure + deterministic (no rand, no time) so
/// it is unit-testable and drives the smoke / fire / snow the same way every run. Drives the
/// volumetric smoke + wreck-flame drift in `Game`, and the snow overlay leans with the same easterly.
fn wind_at(pos: Vec3) -> Vec2 {
    let p = Vec2::new(pos.x, pos.z);
    // 0 deep in the city core (sheltered) → 1 out past the ruins (full open wind).
    let open = smoothstep(
        WIND_SHELTER_R * 0.45,
        WIND_SHELTER_R * 1.4,
        p.distance(CITY_CENTER),
    );
    let strength = WIND_CITY + (WIND_OPEN - WIND_CITY) * open;
    // Prevailing westerly, drifting a touch south; a gentle position-varying gust (±20%).
    let dir = Vec2::new(1.0, 0.28).normalize();
    let gust = 1.0 + 0.20 * (p.x * 0.011 + p.y * 0.017).sin();
    dir * (strength * gust)
}

/// Smoke-plume drift direction for a local wind (`wind_at` output): a strong upward loft with a
/// downwind lean scaled by the wind strength. The renderer normalizes this and adds its own loft, so
/// only the lean RATIO matters — a hard open-steppe wind lays the column over, a sheltered street
/// wind lets it rise almost straight.
fn smoke_drift(wind: Vec2) -> Vec3 {
    Vec3::new(wind.x * 0.14, 1.0, wind.y * 0.14)
}

/// A single frozen corpse, prone in the snow, built from flat-shaded boxes (matching the engine's
/// procedural-box set-dressing style — no import pipeline). Local frame: the body lies along +Z (head
/// toward +Z), y=0 at the snow contact, so a placement transform just yaws + grounds it. `color` is a
/// snow-dusted greatcoat tint. One mesh (merged boxes, one solid tint) so it clones cheaply per body.
fn corpse_mesh(color: [u8; 3]) -> CpuMesh {
    // torso, head, two legs, two splayed arms — a hunched soldier fallen face-down in the drift.
    let boxes: [(Vec3, Vec3); 6] = [
        (Vec3::new(-0.24, 0.0, -0.30), Vec3::new(0.24, 0.26, 0.40)), // torso
        (Vec3::new(-0.13, 0.0, 0.40), Vec3::new(0.13, 0.20, 0.66)),  // head
        (Vec3::new(-0.22, 0.0, -0.95), Vec3::new(-0.02, 0.18, -0.28)), // left leg
        (Vec3::new(0.02, 0.0, -0.95), Vec3::new(0.22, 0.18, -0.28)), // right leg
        (Vec3::new(-0.46, 0.0, -0.02), Vec3::new(-0.24, 0.13, 0.34)), // left arm (splayed)
        (Vec3::new(0.24, 0.0, -0.02), Vec3::new(0.46, 0.13, 0.34)),  // right arm
    ];
    let mut verts: Vec<crate::gltf_load::Vertex> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut mn = Vec3::splat(f32::INFINITY);
    let mut mx = Vec3::splat(f32::NEG_INFINITY);
    for (lo, hi) in boxes {
        let b = box_mesh(lo, hi, color);
        let base = verts.len() as u32;
        verts.extend_from_slice(&b.vertices);
        idx.extend(b.indices.iter().map(|i| base + i));
        mn = mn.min(lo);
        mx = mx.max(hi);
    }
    CpuMesh::basic(verts, idx, Some(crate::prims::solid_color(color)), mn, mx)
}

/// Chunky low-poly seated tanker used by LAN co-op. Each network role owns one instance, allowing
/// remote bodies to move between physical posts while keeping the local first-person perch clear.
fn lan_crew_mesh(color: [u8; 3]) -> CpuMesh {
    let boxes: [(Vec3, Vec3); 8] = [
        (Vec3::new(-0.24, 0.52, -0.15), Vec3::new(0.24, 1.18, 0.15)),
        (Vec3::new(-0.16, 1.18, -0.13), Vec3::new(0.16, 1.48, 0.13)),
        (Vec3::new(-0.34, 0.68, -0.10), Vec3::new(-0.21, 1.08, 0.10)),
        (Vec3::new(0.21, 0.68, -0.10), Vec3::new(0.34, 1.08, 0.10)),
        (Vec3::new(-0.22, 0.18, -0.12), Vec3::new(-0.03, 0.62, 0.12)),
        (Vec3::new(0.03, 0.18, -0.12), Vec3::new(0.22, 0.62, 0.12)),
        (Vec3::new(-0.22, 0.0, 0.03), Vec3::new(-0.04, 0.25, 0.38)),
        (Vec3::new(0.04, 0.0, 0.03), Vec3::new(0.22, 0.25, 0.38)),
    ];
    let mut verts = Vec::new();
    let mut idx = Vec::new();
    let mut mn = Vec3::splat(f32::INFINITY);
    let mut mx = Vec3::splat(f32::NEG_INFINITY);
    for (lo, hi) in boxes {
        let b = box_mesh(lo, hi, color);
        let base = verts.len() as u32;
        verts.extend_from_slice(&b.vertices);
        idx.extend(b.indices.iter().map(|i| base + i));
        mn = mn.min(lo);
        mx = mx.max(hi);
    }
    CpuMesh::basic(verts, idx, Some(crate::prims::solid_color(color)), mn, mx)
}

// ── Armor / ballistics model ────────────────────────────────────────────────────────────────
// A shell no longer does flat damage: the outcome depends on WHICH armor arc it strikes (relative
// to the hit tank's HULL forward) and the RANGE. The Tiger's frontal plate shrugs off the T-34's
// 76 mm at range (bounce); its flanks and rear can be holed, more easily the closer the shot. The
// T-34 is thinner-skinned so the player mostly penetrates, but a well-angled frontal turret hit at
// long range can still bounce — so the player has to close/flank for the weak spots too.

/// Which armor arc an incoming shell strikes, from the horizontal angle between the target tank's
/// hull forward and the direction back toward the shooter.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Arc {
    #[default]
    Front,
    Side,
    Rear,
}

impl Arc {
    fn name(self) -> &'static str {
        match self {
            Arc::Front => "FRONT",
            Arc::Side => "SIDE",
            Arc::Rear => "REAR",
        }
    }
}

/// Classify the arc: FRONT within ±60° of hull forward, REAR within ±60° of dead astern, SIDE in
/// the 60–120° flanks between. `to_shooter` is the (world) vector from the target toward the shooter.
fn hit_arc(hull_forward: Vec3, to_shooter: Vec3) -> Arc {
    let f = Vec3::new(hull_forward.x, 0.0, hull_forward.z).normalize_or_zero();
    let s = Vec3::new(to_shooter.x, 0.0, to_shooter.z).normalize_or_zero();
    let c = f.dot(s);
    if c >= 0.5 {
        Arc::Front
    }
    // within 60° of the front
    else if c <= -0.5 {
        Arc::Rear
    }
    // within 60° of the rear
    else {
        Arc::Side
    }
}

// The old hand-authored penetration-probability tables (tiger/su100/t34_pen_chance) are GONE —
// outcomes now come from `ballistics::resolve`: real shells (mass, velocity decay, filler) vs real
// plates (thickness, slope, material) at computed obliquity. The historic behaviours those tables
// hand-encoded (Tiger frontal immunity to the 76 mm, the SU-100 breaking it, the 88 skipping off a
// T-34 glacis at long range) now EMERGE from the physics — see ballistics.rs tests.

/// The struck face for the ballistic plate lookup: the coarse hit arc refined by whether the round
/// landed on the turret or hull band (the same roll the spall model uses to pick the compartment).
fn facing_from(arc: Arc, turret: bool) -> crate::ballistics::Facing {
    use crate::ballistics::Facing as F;
    match (arc, turret) {
        (Arc::Front, false) => F::HullFront,
        (Arc::Side, false) => F::HullSide,
        (Arc::Rear, false) => F::HullRear,
        (Arc::Front, true) => F::TurretFront,
        (Arc::Side, true) => F::TurretSide,
        (Arc::Rear, true) => F::TurretRear,
    }
}

/// Outward world normal of the struck face — feeds the horizontal-obliquity term (a flanking shot
/// into the front plate arrives oblique; a perpendicular shot arrives square).
fn face_normal(arc: Arc, hull_forward: Vec3, to_shooter: Vec3) -> Vec3 {
    let f = Vec3::new(hull_forward.x, 0.0, hull_forward.z).normalize_or_zero();
    match arc {
        Arc::Front => f,
        Arc::Rear => -f,
        Arc::Side => {
            let right = Vec3::Y.cross(f).normalize_or_zero();
            if right.dot(to_shooter) >= 0.0 {
                right
            } else {
                -right
            }
        }
    }
}

/// The shell a given enemy kind fires (the physics the old kind-aware tables encoded).
fn enemy_shell(kind: EnemyKind) -> &'static crate::ballistics::ShellSpec {
    match kind {
        EnemyKind::T34 => &crate::ballistics::BR_350B,
        EnemyKind::Su100 => &crate::ballistics::BR_412,
        EnemyKind::Kv2 => &crate::ballistics::OF_530,
        EnemyKind::Isu152 => &crate::ballistics::BR_540,
        EnemyKind::Is2 | EnemyKind::Is3 => &crate::ballistics::BR_471,
    }
}

/// The armor scheme a given enemy kind wears (for the player's shells striking THEM).
fn enemy_armor(kind: EnemyKind) -> crate::ballistics::ArmorScheme {
    match kind {
        EnemyKind::T34 => crate::ballistics::ArmorScheme::T34_76,
        EnemyKind::Su100 => crate::ballistics::ArmorScheme::Su100,
        EnemyKind::Kv2 => crate::ballistics::ArmorScheme::Kv2,
        EnemyKind::Isu152 => crate::ballistics::ArmorScheme::Isu152,
        EnemyKind::Is2 => crate::ballistics::ArmorScheme::Is2,
        EnemyKind::Is3 => crate::ballistics::ArmorScheme::Is3,
    }
}

/// Bearing (deg, 0 = dead ahead, 90 = right, 270 = left) of a world direction relative to a facing.
/// Drives the stereo pan of hull impacts — a hit from the left CLANGS in the left ear.
fn bearing_deg_from(facing_forward: Vec3, to_source: Vec3) -> f32 {
    let f = Vec3::new(facing_forward.x, 0.0, facing_forward.z).normalize_or_zero();
    let t = Vec3::new(to_source.x, 0.0, to_source.z).normalize_or_zero();
    if f == Vec3::ZERO || t == Vec3::ZERO {
        return 0.0;
    }
    // Physical right is forward x up (right-handed): Y.cross(f) is the LEFT vector and mirrored
    // every pan — a T-34 firing from your right CLANGed in your left ear.
    let right = f.cross(Vec3::Y);
    t.dot(right).atan2(t.dot(f)).to_degrees().rem_euclid(360.0)
}

/// What happened when a shell arrived at a target.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
enum Outcome {
    #[default]
    Ground, // hit the terrain (or nothing) — no target struck
    NearMiss,  // passed close by the target but missed — supersonic crack
    Bounce,    // struck armor but was shrugged off — CLANG, negligible damage
    Penetrate, // holed the armor — real damage
}

/// The specific way the player's Tiger was knocked out, captured at the instant of the killing
/// penetration (see `resolve_player_penetration`) straight from the resolved armor outcome, so the
/// death screen can name the cause in GERMAN. Deterministic — no rand.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DeathCause {
    AmmoDetonation, // catastrophic — the ammunition rack went up, the turret was blown off
    FuelFire,       // catastrophic — the fuel took light, the tank burned out with its crew
    CrewLost,       // the crew was incapacitated one casualty at a time (no single clean breach)
    Penetration(Arc), // a killing hole through the flank / rear / (point-blank) front plate
    OnFoot,         // shot down in the open — bailed out, never reached another Tiger
}

impl DeathCause {
    /// Every cause the game can actually inflict, in a stable order — for the headless death-screen
    /// preview (`ui death out.png [ace_idx] [cause_idx]`).
    pub const ALL: [DeathCause; 6] = [
        DeathCause::AmmoDetonation,
        DeathCause::FuelFire,
        DeathCause::CrewLost,
        DeathCause::Penetration(Arc::Side),
        DeathCause::Penetration(Arc::Rear),
        DeathCause::OnFoot,
    ];

    /// The GERMAN cause-of-death line for the death screen (the UI font carries the umlauts + em
    /// dash). Short + evocative, sits subordinate under the "Du bist gestorben" headline.
    pub fn de(self) -> &'static str {
        match self {
            DeathCause::AmmoDetonation => "Ammunition detonation — turret blown off",
            DeathCause::FuelFire => "Fuel fire — burned out",
            DeathCause::CrewLost => "Crew lost",
            DeathCause::Penetration(Arc::Front) => "Penetration — front armor",
            DeathCause::Penetration(Arc::Side) => "Penetration — flank",
            DeathCause::Penetration(Arc::Rear) => "Penetration — rear",
            DeathCause::OnFoot => "Shot down on foot — no Tiger reached",
        }
    }
}

// ── In-tank damage model: a crew of 5 + modules as individuals, driven by a SPALL CONE ───────────
// When an enemy shell PENETRATES the armour (the arc/range roll above decides bounce vs penetrate —
// the Tiger's front stays near-immune), it does NOT do flat damage: it sprays a cone of white-hot
// spall + armour fragments INTO the crew compartment from the entry point, along the shell path.
// Whichever crew/modules sit in that cone get hit, weighted by proximity to the cone axis + entry
// (deterministic hash — no rand). The entry HEIGHT decides whether the hull or the turret was holed:
// a LOW hull-side pen sweeps the driver / radio-op + engine / transmission / fuel / tracks / ammo;
// a HIGH turret pen sweeps the gunner / loader / commander + gun / traverse / sight / coax. All
// positions are the real interior geometry (hull-local: +Z fwd, +Y up, +X right, origin at ground).

/// The five Tiger crewmen. Commander / gunner / loader ride the turret; the driver and the
/// radio-operator (who also mans the hull MG) ride the hull front.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CrewRole {
    Commander,
    Gunner,
    Loader,
    Driver,
    Radioman,
}

impl CrewRole {
    pub const ALL: [CrewRole; 5] = [
        CrewRole::Commander,
        CrewRole::Gunner,
        CrewRole::Loader,
        CrewRole::Driver,
        CrewRole::Radioman,
    ];

    pub fn name(self) -> &'static str {
        match self {
            CrewRole::Commander => "commander",
            CrewRole::Gunner => "gunner",
            CrewRole::Loader => "loader",
            CrewRole::Driver => "driver",
            CrewRole::Radioman => "radio-operator",
        }
    }

    /// Hull-local seat position (the real interior layout the spall cone is tested against).
    fn station(self) -> Vec3 {
        match self {
            CrewRole::Driver => Vec3::new(-0.70, 1.05, 1.70), // hull front-left
            CrewRole::Radioman => Vec3::new(0.70, 1.05, 1.70), // hull front-right (ball-MG)
            CrewRole::Gunner => Vec3::new(-0.50, 1.95, 0.30), // turret front-left
            CrewRole::Loader => Vec3::new(0.55, 1.90, 0.10),  // turret right
            CrewRole::Commander => Vec3::new(0.35, 2.05, -0.50), // turret rear (cupola)
        }
    }
}

/// One player crewman: his role + whether he is still fighting or incapacitated.
#[derive(Copy, Clone, Debug)]
pub struct Crewman {
    pub role: CrewRole,
    pub ok: bool,
}

/// Every individually-modelled subsystem. Turret group (gun / traverse / sight / coax) and hull group
/// (engine / transmission / tracks / fuel / ammo / hull-MG / radio).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ModuleKind {
    MainGun,
    TurretTraverse,
    GunnerSight,
    CoaxMg,
    Engine,
    Transmission,
    LeftTrack,
    RightTrack,
    Fuel,
    AmmoRack,
    HullMg,
    Radio,
}

impl ModuleKind {
    pub const ALL: [ModuleKind; 12] = [
        ModuleKind::MainGun,
        ModuleKind::TurretTraverse,
        ModuleKind::GunnerSight,
        ModuleKind::CoaxMg,
        ModuleKind::Engine,
        ModuleKind::Transmission,
        ModuleKind::LeftTrack,
        ModuleKind::RightTrack,
        ModuleKind::Fuel,
        ModuleKind::AmmoRack,
        ModuleKind::HullMg,
        ModuleKind::Radio,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModuleKind::MainGun => "main gun",
            ModuleKind::TurretTraverse => "turret traverse",
            ModuleKind::GunnerSight => "gunner sight (TZF)",
            ModuleKind::CoaxMg => "coaxial MG",
            ModuleKind::Engine => "engine",
            ModuleKind::Transmission => "transmission",
            ModuleKind::LeftTrack => "left track",
            ModuleKind::RightTrack => "right track",
            ModuleKind::Fuel => "fuel",
            ModuleKind::AmmoRack => "ammunition rack",
            ModuleKind::HullMg => "hull MG",
            ModuleKind::Radio => "radio",
        }
    }

    /// Hull-local position (real interior geometry).
    fn station(self) -> Vec3 {
        match self {
            ModuleKind::MainGun => Vec3::new(0.00, 1.90, 1.00),
            ModuleKind::TurretTraverse => Vec3::new(0.00, 0.90, 0.00),
            ModuleKind::GunnerSight => Vec3::new(-0.40, 1.95, 0.90),
            ModuleKind::CoaxMg => Vec3::new(0.20, 1.90, 0.95),
            ModuleKind::Engine => Vec3::new(0.00, 1.05, -2.40), // rear deck
            ModuleKind::Transmission => Vec3::new(0.00, 0.75, 2.20), // Tiger's front drive
            ModuleKind::LeftTrack => Vec3::new(-1.75, 0.50, 0.00),
            ModuleKind::RightTrack => Vec3::new(1.75, 0.50, 0.00),
            ModuleKind::Fuel => Vec3::new(0.70, 0.95, -1.70), // rear-side tanks
            ModuleKind::AmmoRack => Vec3::new(0.00, 0.85, -0.10), // sponson stowage (big radius)
            ModuleKind::HullMg => Vec3::new(0.75, 1.05, 1.90),
            ModuleKind::Radio => Vec3::new(0.55, 1.15, 1.50),
        }
    }

    /// Spall hit radius — most modules are compact; the ammunition + running gear are spread out, so
    /// a low side pen anywhere along the hull can catch them.
    fn radius(self) -> f32 {
        match self {
            ModuleKind::AmmoRack => 1.70, // stowed along both sponsons
            ModuleKind::Engine => 0.85,
            ModuleKind::Fuel => 0.75,
            ModuleKind::LeftTrack | ModuleKind::RightTrack => 0.65,
            _ => 0.40,
        }
    }

    /// How readily fully-exposed spall disables it — heavy steel resists, optics / radio are fragile,
    /// the ammunition rarely takes a fragment but detonates catastrophically when it does.
    fn vuln(self) -> f32 {
        match self {
            ModuleKind::AmmoRack => 0.42,
            ModuleKind::GunnerSight | ModuleKind::Radio => 0.72,
            ModuleKind::CoaxMg | ModuleKind::HullMg => 0.62,
            ModuleKind::LeftTrack | ModuleKind::RightTrack => 0.66,
            ModuleKind::Engine => 0.60,
            ModuleKind::MainGun => 0.55,
            ModuleKind::TurretTraverse | ModuleKind::Transmission | ModuleKind::Fuel => 0.52,
        }
    }
}

/// One player module: its kind + whether it still works.
#[derive(Copy, Clone, Debug)]
pub struct Module {
    pub kind: ModuleKind,
    pub functional: bool,
}

/// Fresh, undamaged player crew / modules (also used by the spall unit tests).
pub fn default_crew() -> Vec<Crewman> {
    CrewRole::ALL
        .iter()
        .map(|&role| Crewman { role, ok: true })
        .collect()
}
pub fn default_modules() -> Vec<Module> {
    ModuleKind::ALL
        .iter()
        .map(|&kind| Module {
            kind,
            functional: true,
        })
        .collect()
}

// Spall-cone shape.
const SPALL_HALF_TAN: f32 = 0.577; // tan(30°) cone half-angle
const SPALL_REACH: f32 = 3.2; // m — the spall crosses the compartment, then loses energy
const CREW_VULN: f32 = 0.90; // crewmen are soft targets in the cone

// Interior fire / cook-off.
const FIRE_GROWTH: f32 = 0.10; // interior fire intensity gained per second once alight
const FIRE_MAX: f32 = 1.4;
const FIRE_AMMO_THRESH: f32 = 0.55; // once the fire is this established it starts reaching the ammo
const FIRE_TO_AMMO: f32 = 5.0; // s from there to a cook-off catastrophic kill if unchecked

const CREW_KO_DUR: f32 = 2.1; // s — full crew-KO screen effect (red splatter + hand-wipe)
const N_INT_FIRE: usize = 4; // interior flame FX slots seen in the player's own view

/// Exposure of an interior item at `item` (hull-local, hit radius `item_r`) to spall spraying from
/// `entry` along `dir` — a cone of half-angle atan(`half_tan`) reaching `reach` metres. Returns
/// 0 (clear) .. 1 (dead in the cone at the entry): nearer the axis AND the entry = more. A vertical
/// band term suppresses items far above/below the entry height, so a hull pen favours hull occupants
/// and a turret pen favours turret occupants — the entry height picks the compartment. Pure geometry.
fn spall_exposure(
    entry: Vec3,
    dir: Vec3,
    item: Vec3,
    item_r: f32,
    half_tan: f32,
    reach: f32,
) -> f32 {
    let v = item - entry;
    let along = v.dot(dir);
    if along > reach || along < -item_r {
        return 0.0;
    }
    let a = along.max(0.0);
    let perp = (v - dir * along).length();
    let allowed = 0.40 + a * half_tan + item_r;
    if perp > allowed {
        return 0.0;
    }
    let axial = 1.0 - (a / reach).clamp(0.0, 1.0);
    let radial = 1.0 - (perp / allowed).clamp(0.0, 1.0);
    // Tight vertical band + a low off-axis floor: POSITION IS THE DEFENCE (solo crew — where
    // your body is when the plate lets go decides whether the cone finds you). The old soft band
    // (0.5..1.5) gave a turret-posted body ~60% exposure to a HULL pen — station choice barely
    // mattered; now a compartment away is a near-miss, dead-on-axis is death.
    let band = 1.0 - smoothstep(0.25, 0.95, (item.y - entry.y).abs());
    ((0.12 + 0.88 * radial) * (0.45 + 0.55 * axial) * band).clamp(0.0, 1.0)
}

/// What a spall cone through the crew compartment did: which crew / modules it knocked out, and
/// whether it struck the ammunition (→ instant catastrophic) or lit a fuel / engine FIRE. Pure +
/// deterministic (seeded hash) so it is unit-testable without a GPU / renderer.
#[derive(Default, Debug)]
pub struct SpallResult {
    /// The spall cone caught the PLAYER'S BODY at its actual interior position ("it's just you
    /// now" — there is no abstract crew; where you are standing/sitting decides if you're hit).
    pub hit_body: bool,
    pub hit_modules: Vec<ModuleKind>,
    pub ammo: bool, // the ammunition rack was struck — catastrophic
    pub fire: bool, // fuel or engine was struck — a fire starts
}

/// Resolve a spall cone (all inputs hull-local) against the PLAYER'S BODY (at `body`, hull-local —
/// wherever he actually is; `None` = dismounted, nobody aboard) and the modules IN PLACE.
/// Deterministic off `seed`. Shared by the live game and tests.
fn resolve_spall(
    entry: Vec3,
    dir: Vec3,
    body: Option<Vec3>,
    modules: &mut [Module],
    seed: u32,
) -> SpallResult {
    let mut out = SpallResult::default();
    let mut tick = seed;
    if let Some(b) = body {
        let e = spall_exposure(entry, dir, b, 0.35, SPALL_HALF_TAN, SPALL_REACH);
        tick = tick.wrapping_add(1);
        if e > 0.0 && hash01(tick.wrapping_mul(6151)) < e * CREW_VULN {
            out.hit_body = true;
        }
    }
    for md in modules.iter_mut() {
        if !md.functional {
            continue;
        }
        let k = md.kind;
        let e = spall_exposure(
            entry,
            dir,
            k.station(),
            k.radius(),
            SPALL_HALF_TAN,
            SPALL_REACH,
        );
        tick = tick.wrapping_add(1);
        if e > 0.0 && hash01(tick.wrapping_mul(9337)) < e * k.vuln() {
            md.functional = false;
            out.hit_modules.push(k);
            match k {
                ModuleKind::AmmoRack => out.ammo = true,
                ModuleKind::Fuel | ModuleKind::Engine => out.fire = true,
                _ => {}
            }
        }
    }
    out
}

/// The killing penetration, captured WHOLE at the instant the Tiger is knocked out — the data the
/// instant-replay reconstructs the kill from. All world-space; `hit_body` / `hit_modules` are what
/// that final blow (or the fire it lit) took out. Public + complete for the replay (whose consumer
/// does not exist yet — hence the fields are write-only for now).
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct FatalHit {
    pub shooter_pos: Vec3, // enemy muzzle the round came from
    pub shell_dir: Vec3,   // world shell travel direction
    pub entry_point: Vec3, // world point the round holed the armour
    pub spall_dir: Vec3,   // world direction the spall cone sprayed into the interior
    pub hit_body: bool,
    pub hit_modules: Vec<ModuleKind>,
    pub cause: DeathCause,
}

// ── Death instant-replay (kill-cam) — phase timeline + camera/shell math ─────────────────────────
// The replay runs on a normalised phase `t` in [0,1] (the front-end maps ~4.5 s of real time onto
// it). The ~0.3 s of real ballistics is STRETCHED across most of the phase (the shell window below)
// so the round reads in slow-motion. All the math here is pure world-space geometry — no GPU, no
// game state — so the camera framing + shell lerp are unit-testable. Deterministic (no rand).
const N_SHARD: usize = 18; // pooled spall/shrapnel fragment FX for the kill-cam cone
const REPLAY_MUZZLE_END: f32 = 0.12; // shooter muzzle flash lives [0, this]
const REPLAY_SHELL_T0: f32 = 0.08; // shell leaves the muzzle
const REPLAY_SHELL_T1: f32 = 0.60; // shell reaches the Tiger (slow-mo: most of the phase)
const REPLAY_IMPACT_T0: f32 = 0.58; // penetration flash punches in
const REPLAY_IMPACT_END: f32 = 0.82; // flash decays to a lingering ember
const REPLAY_SPALL_T0: f32 = 0.60; // spall cone starts spraying inward
const REPLAY_SPALL_T1: f32 = 0.96; // by here the fragments have crossed the compartment

/// Fraction of the shell's flight completed at phase `t` (0 at the muzzle, 1 at the entry point).
fn replay_shell_frac(t: f32) -> f32 {
    ((t - REPLAY_SHELL_T0) / (REPLAY_SHELL_T1 - REPLAY_SHELL_T0)).clamp(0.0, 1.0)
}

/// World position of the streaking replay shell at phase `t`: lerps `shooter → entry` over the slow-mo
/// shell window, parked at either end outside it.
fn replay_shell_pos(shooter: Vec3, entry: Vec3, t: f32) -> Vec3 {
    shooter.lerp(entry, replay_shell_frac(t))
}

/// Project a WORLD direction into the camera's screen basis, returning a normalized 2D vector in uv
/// space (x = screen-right, y = screen-DOWN — matching crush.wgsl's uv convention). Used to point the
/// crew-KO blood spray along the incoming round's on-screen axis. Falls back to a down-right diagonal
/// when the direction is nearly parallel to the view ray (round coming straight at the optic → no
/// readable lateral component).
fn project_screen_dir(world: Vec3, cam: &crate::camera::Camera) -> [f32; 2] {
    let w = world.normalize_or_zero();
    if w.length_squared() < 1e-6 {
        return [0.72, 0.69];
    }
    let right = cam.right();
    let up = right.cross(cam.forward()); // orthonormal camera up
    let v = Vec2::new(w.dot(right), -w.dot(up)); // uv y points down → negate the up component
    if v.length() < 0.12 {
        return [0.72, 0.69];
    }
    v.normalize().to_array()
}

/// Cinematic kill-cam framing at phase `t`: returns `(eye, look_target)`. Opens beside the shooter's
/// muzzle looking down the shell corridor, then dollies in toward the Tiger's entry point as the
/// round arrives, ending tight on the penetration + spall. The eye is offset to the SIDE of the shell
/// path so the streak reads in profile.
fn replay_camera(shooter: Vec3, entry: Vec3, spall_dir: Vec3, t: f32) -> (Vec3, Vec3) {
    let sd = (entry - shooter).normalize_or_zero();
    let mut side = sd.cross(Vec3::Y).normalize_or_zero();
    if side.length_squared() < 1e-4 {
        side = Vec3::X;
    }
    let up = Vec3::Y;
    // Wide: just behind + beside the muzzle, looking down the path toward the distant Tiger.
    let eye_a = shooter + side * 3.5 - sd * 4.0 + up * 2.6;
    let look_a = shooter + sd * 40.0 + up * 0.5;
    // Tight: beside + slightly back from the entry, looking at the hole (a touch into the interior).
    let eye_b = entry + side * 6.5 - sd * 3.5 + up * 2.4;
    let look_b = entry + spall_dir.normalize_or_zero() * 1.2;
    // Push-in lags the shell slightly (smoothstep vs the shell's linear ramp) so the round pulls
    // ahead into frame before the camera catches up on the Tiger.
    let push = smoothstep(0.12, 0.70, t);
    (eye_a.lerp(eye_b, push), look_a.lerp(look_b, push))
}

const TERRAIN_PATH: &str = "assets/terrain/stalingrad_100km_height16.png";

// ── Ruined-settlement set dressing (all PSX packs, real-world-metric GLBs) ──
const MP: &str =
    "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/";
const NAT: &str = "assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/";
// Building shells / walls (~4×3 m each) — silhouette pieces.
const STRUCT_WALL: &str = "Modular Structures/wall_1.glb";
const STRUCT_WALL_HOLE1: &str = "Modular Structures/wall_1_tunnel_hole_1.glb"; // shell-holed wall
const STRUCT_WALL_HOLE2: &str = "Modular Structures/wall_1_tunnel_hole_2.glb";
const STRUCT_PILLAR: &str = "Modular Structures/pillar_1.glb";
const STRUCT_DOORWAY: &str = "Modular Structures/doorway_1.glb";
const STRUCT_ARC: &str = "Modular Structures/arc_1_wall_1.glb";
// Ground clutter — debris (~0.4–2.5 m).
const DEBRIS_CRATE1: &str = "Large Props/wooden_crate_1.glb";
const DEBRIS_CRATE3: &str = "Large Props/wooden_crate_3.glb";
const DEBRIS_SUPPLY: &str = "Large Props/supply_crate_1.glb";
const DEBRIS_BARREL1: &str = "Large Props/metal_barrel_mp_1.glb";
const DEBRIS_BARREL2: &str = "Large Props/metal_barrel_mp_2.glb";
const DEBRIS_AMMO: &str = "Items & Weapons/ammo_box_1.glb";
const RUBBLE_STONE3: &str = "stone_3.glb";
const RUBBLE_STONE5: &str = "stone_5.glb";
const RUBBLE_LOG: &str = "tree_log_1.glb";

// ── PSX Mega Pack II v1.8 — richer dressing pass (roofs, brick rubble, barricades, skyline) ──
const MP2: &str =
    "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/";
// Roofs — 4×~3–4m modules, same footprint scale as the MP1 wall_1.glb (probed: both 4m wide,
// 3m wall height), so a roof can be dropped on an existing wall's dx/dz/rot/scale directly.
const ROOF_MH1: &str = "Modular Structures/roof_mh_1.glb";
const ROOF_MH2: &str = "Modular Structures/roof_mh_2.glb";
const ROOF_BX1: &str = "Modular Structures/roof_bx_1.glb";
// Brick-pile rubble — more decayed-looking than the NAT stone rubble already in use.
const RUBBLE_BRICKS1: &str = "Debris & Misc/debris_bricks_mx_1.glb"; // flat scattered brick field
const RUBBLE_BRICKS2: &str = "Debris & Misc/debris_bricks_mx_2.glb";
const RUBBLE_STACK1: &str = "Debris & Misc/bricks_stacked_mx_1.glb"; // small toppled brick stack
const RUBBLE_STACK2: &str = "Debris & Misc/bricks_stacked_mx_2.glb";
const RUBBLE_STACK3: &str = "Debris & Misc/bricks_stacked_mx_3.glb";
const RUBBLE_STACK4: &str = "Debris & Misc/bricks_stacked_mx_4.glb";
// Barricades — defensive chokepoint dressing near doorways/pillars.
const BARRICADE_A1: &str = "Modular Props/barricade_a_1.glb";
const BARRICADE_A2: &str = "Modular Props/barricade_a_2.glb";
const BARRICADE_B1: &str = "Modular Props/barricade_b_1.glb";
const BARRICADE_B3: &str = "Modular Props/barricade_b_3.glb";
const BARRICADE_B4: &str = "Modular Props/barricade_b_4.glb";
// Industrial skyline silhouettes — far settlement-edge backdrop variety.
const CHIMNEY: &str = "Large Props & Machinery/chimney_a_1.glb";
const CONTAINER: &str = "Large Props & Machinery/shipping_container_mx_1.glb";

#[derive(Default, Clone, Copy)]
pub struct TankInput {
    pub drive: f32,    // W/S  [-1,1]
    pub steer: f32,    // A/D
    pub traverse: f32, // ←/→
    pub elevate: f32,  // ↑/↓
    pub fire: bool,    // Space
    /// Held MG34 trigger (Left Shift). ONE trigger, routed by view: in the gunner optic it fires
    /// the COAXIAL MG (parallel to the main gun); in the commander view it chatters the HULL/BOW MG.
    /// The player mans whichever MG their current station covers (single-crew simplification).
    pub mg_coax: bool,
    /// Held C — the coax MG trigger at the GUNNER's sights. (It rode Ctrl until the playtest made
    /// Ctrl the EXIT-the-sights key; LMB there is the main gun, so the coax needed its own finger.)
    pub coax: bool,
    pub look_dx: f32, // mouse delta this frame (commander head-look)
    pub look_dy: f32,
    pub binoculars: bool, // right mouse held (commander / on-foot Luger aim)
    /// F held — grab the extinguisher and knock down the interior fire (locks ALL stations while it
    /// runs). The genuine race against the ammo cook-off (`fire_ammo_timer`).
    pub extinguish: bool,
    /// R held — field-repair the highest-priority repairable downed module the current station reaches
    /// (also locks all stations; tracks need the driver hatched out).
    pub repair: bool,
    pub run: bool, // legacy (running is the DEFAULT gait on foot now; crouch is the slow one)
    pub aim: bool, // right mouse HELD — on-foot Luger aim
    pub fire2: bool, // left mouse — on-foot Luger fire
    pub crouch: bool, // ctrl — on-foot crouch (lower eye, slower, quieter, smaller target)
    pub reload: bool, // R — on-foot Luger magazine change
    pub jump: bool, // Space on foot — jump, OR advance the climb ritual at a perch
    pub ignition: bool, // Z — engine shutoff / crank (the silent-watch scout loop)
    pub hatch_toggle: bool, // one-frame LAN interact pulse; solo hatch clicks act directly
    /// LAN crew authority for this frame. `None` preserves the exact solo control path. When set,
    /// simulation duties come from their real stations while this process's ordinary input remains
    /// the local crewman's camera/body input.
    pub coop: Option<crate::lan::CrewFrame>,
    pub coop_host: bool,
}

/// INSIDE-view interactive hotspots: look at the thing, hold LMB to work it. Turret-local
/// anchor points; the ready rack keeps its full grab-lift-flick reload feel.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Hotspot {
    GunLever,          // the breech lever: LMB-hold + pull to open/close the breech
    LoadedRound,       // lever open: pull the live round back out to change ammunition
    RackSlot(usize),   // grab THIS round off the rack (index into interior_rig::SHELL_SLOTS)
    SpentCasing,       // the hot case sitting in the spent breech — hold LMB to pull it free
    FloorShell(usize), // a dropped round rolling on the basket floor — pick it back up
    Extinguisher,      // hold: spray (same path as held F)
    RepairKit,         // hold: field repair (same path as held G)
}

/// Which mesh rides the loader's hands while a round is carried (the very shell you grabbed
/// leaves its rack slot / floor spot and follows the carry spring — nothing teleports).
#[derive(Clone, Copy, PartialEq)]
enum CarriedMesh {
    RackShell(usize),  // renderer slot shell0 + i keeps drawing, at the carry transform
    LooseShell(usize), // a scooped floor round: its loose_shell slot keeps drawing
}

/// A dropped main-gun round: a free rigid body in TURRET-LOCAL space (it rides the tank).
/// Gravity, one soft bounce off the basket floor, then it ROLLS — velocity decays, the long
/// axis swings perpendicular to travel (a cylinder can only roll sideways), and it clatters
/// off the basket wall. Look at it + LMB to pick it back up.
#[derive(Clone, Copy, Default)]
struct LooseShell {
    active: bool,
    pos: Vec3, // turret-local
    vel: Vec3, // turret-local m/s
    axis: f32, // long-axis yaw (rad, turret-local)
    spin: f32, // roll about the long axis (visual)
    nature_he: bool,
}

const N_LOOSE: usize = 2;
/// A short, articulated visual run for the loose end the player drags back over the road wheels.
const N_REPAIR_SHOES: usize = 18;
const BASKET_FLOOR: f32 = 1.45 + 0.07; // turret-local floor plane + shell radius
const BASKET_R: f32 = 1.00; // fighting-compartment wall (radial clamp)

// ── Crew-of-one interior workload: STATIONS, MANUAL RELOAD, FIRE-FIGHTING, REPAIR ─────────────────
// The scarce resource is the one player's HANDS, expressed as `station` (the posted seat whose
// continuous controls are live) + `activity` (a discrete emergency beat — extinguish / repair — that
// yanks both hands off any station). Manual reload is a station duty at the breech (Loader): eject a
// hot casing, then MASH a fresh round home. The existing 5 crew `ok` flags gate speed, not the
// player: an ok crewman means the job runs assisted/fast, a KO'd one means the lone survivor does it
// himself, degraded (slower ram, a manual eject fumble) — never impossible.

/// Seconds to move BETWEEN seats: within the turret (gunner↔loader↔commander) vs clambering to the
/// hull driver's seat (through the turret basket) — the whole point is that switching costs a beat.
const SWITCH_TURRET: f32 = 0.55;
const SWITCH_HULL: f32 = 0.95;
/// Ram progress (0..1) added per Space mash: assisted (loader ok) seats in ~3 mashes; one-handed
/// (loader KO'd) takes ~7-8. Decays if you stop, so it reads as physically driving the round home.
const RAM_PER_PRESS_OK: f32 = 0.34;
const RAM_PER_PRESS_KO: f32 = 0.14;
const RAM_DECAY: f32 = 0.25;
/// A lone survivor (loader KO'd) must fumble the hot casing free before it ejects (assisted = instant).
const EJECT_FUMBLE: f32 = 0.8;
/// Extinguisher: unclip time before foam flows, then fire knocked down per second (> FIRE_GROWTH so
/// spraying always wins the race — but every second is a second the swarm shoots an un-aiming Tiger).
const EXT_GRAB: f32 = 0.35;
const EXT_RATE: f32 = 0.5;
/// The READY-RACK hotspot, turret-local — where the loader's crosshair must rest to grab a round
/// (shared by the hover test and `debug_loader_face_rack`).
const RACK_SPOT: Vec3 = Vec3::new(0.0, 1.83, 0.55); // legacy ready-rack point (headless aim helper)
                                                    // ── The PHYSICAL reload (user design: no auto-placement — you carry the weight) ─────────────────
                                                    // A grabbed round is a sprung mass on the hand anchor: it LAGS, swings, and droops — 13 kg of
                                                    // brass and shell. The ram only engages with the round bodily AT the tray mouth; the spent case
                                                    // must be PULLED from the breech and tossed by hand before anything can be loaded.
const TRAY_MOUTH: Vec3 = crate::interior_rig::ROUND_READY; // authored tray entrance / handoff pose
                                                           // The extinguisher bracket: Turret_0232's spot in the blend (rear-left wall, calibrated against
                                                           // ROUND_SEAT to 7 mm). The cylinder is excluded from the bake; the interactive bottle lives here.
const EXT_MOUNT_BASE: Vec3 = Vec3::new(-0.350, 0.848, -1.043);
const EXT_MOUNT_MID: Vec3 = Vec3::new(-0.350, 1.13, -1.043);
// ── The BREECH LEVER (user design 2026-07-06): pull UP to open the breech — only then can the
// hot case come out and a fresh round go in; pull DOWN after ramming to CLOSE, which is what
// completes the load. The gun fires only closed. Pivots calibrated from the blend (gun-local).
const GUNLEVER_POS: Vec3 = Vec3::new(-0.252, -0.054, -0.987);
const BREECHCOVER_POS: Vec3 = Vec3::new(0.001, -0.018, -0.886);
const LEVER_SWING: f32 = 1.15; // rad about local X at full open (sign flipped: playtest said inverted)
const COVER_DROP: f32 = 0.212; // m — the block cover slides down with the lever
const LEVER_OPEN: f32 = 0.8; // open enough to work the breech
const LEVER_SHUT: f32 = 0.15; // closed enough to fire
                              // The HATCH LEVER on the cupola pedestal. Position extracted from the blend 2026-07-11:
                              // object 'Turret_0144' (data 'Hatch Lever') world (0.9596, 0.3358, 2.5622) minus Turret_Pivot
                              // (-0.0044, 0.0007, 0.4198), Blender->engine (bx, bz, -by). The old ESTIMATED value floated
                              // the knob ~40 cm off the pedestal, mid-compartment, right on the CUPOLA click anchor.
const HATCHLEVER_POS: Vec3 = Vec3::new(0.964, 2.142, -0.335);
// Toolbox re-homed beside the extinguisher on the rear-left wall (playtest: it blocked the
// turret walk at its old spot). Hotspot at grab height; mesh sits just below it.
const TOOLBOX_MOUNT: Vec3 = Vec3::new(-0.75, 1.00, -0.98);
const TOOLBOX_MESH: Vec3 = Vec3::new(-0.75, 0.88, -0.98);
// The BREECH LAMP (user design): one annunciator-style lamp ON the breech, loader-side.
// GREEN = loaded & lever shut (ready) / ORANGE = lever open (reloading) / RED = shut on a
// spent case (or empty) — the reload cycle readable at a glance from anywhere in the turret.
const BREECH_LAMP_POS: Vec3 = Vec3::new(0.20, 0.12, -0.92); // gun-local, right of the block
const RAM_REACH: f32 = 0.86; // m — arm's reach to the authored tray mouth; crosshair gate does precision
const CARRY_K_ROUND: f32 = 48.0; // spring stiffness — heavy, visible lag
const CARRY_D_ROUND: f32 = 8.5;
const CARRY_K_CASE: f32 = 90.0; // the empty case is light — snappier in the hands
const CARRY_D_CASE: f32 = 11.0;
const CASING_PULL_TIME: f32 = 0.55; // s of held LMB to work the hot case out of the chamber
                                    // ── VOXEL FOAM (user design): the extinguisher SHOOTS blocky blobs that stick where they land
                                    // (turret floor, basket wall, the breech box) and gradually sink away. Turret-local, pooled,
                                    // deterministic — the sprayed compartment stays foamed for a while, then recovers.
const N_FOAM: usize = 28;
const FOAM_RATE: f32 = 26.0; // blobs per second while the trigger is squeezed
const FOAM_LIFE: f32 = 6.5; // s a stuck blob persists (hashed ±)
const FOAM_FLOOR: f32 = 1.45; // turret-local basket floor (matches the casing plane)
const FOAM_WALL_R: f32 = 1.05; // basket wall radius
const CARRY_WALK_SLOW: f32 = 0.72; // walk-speed factor while lugging a round
const N_CASING: usize = 4; // pooled rigid-body shell casings (round-robin)
const N_EXT: usize = 3; // pooled extinguisher spray puffs
const N_LAMP: usize = 4; // interior annunciator lamps (fire / breech / repair / station)

/// The posted crew STATION the one player is manning. Maps 1:1 onto `CrewRole` for the damage gates.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Station {
    Gunner,
    Loader,
    Driver,
    Radioman,
    Commander,
}

impl Station {
    /// The driver rides the HULL; gunner/loader/commander ride the TURRET. A turret↔hull move costs
    /// more (clambering through the basket) than a within-turret shuffle.
    fn in_hull(self) -> bool {
        matches!(self, Station::Driver | Station::Radioman)
    }
}

/// View-local weather shelter, separate from the physical precipitation gate. Depth is the final
/// authority: armor in front of the eye blocks every field layer, while a real hatch/optic opening
/// exposes distant scene depth and therefore distant snow. An open cupola viewed from inside admits
/// a little mid-distance weather but never paints flakes onto the nearby steel compartment.
fn tank_weather_shelter(station: Station, exposed: bool, hatch_open: bool) -> f32 {
    if exposed {
        0.0
    } else if hatch_open && matches!(station, Station::Commander | Station::Loader) {
        0.68
    } else {
        1.0
    }
}

/// Cost (seconds) of a station change — compartment-aware.
fn switch_cost(a: Station, b: Station) -> f32 {
    if a.in_hull() != b.in_hull() {
        SWITCH_HULL
    } else {
        SWITCH_TURRET
    }
}

/// Ram progress added per Space mash, degraded when the loader is down.
fn ram_per_press(loader_ok: bool) -> f32 {
    if loader_ok {
        RAM_PER_PRESS_OK
    } else {
        RAM_PER_PRESS_KO
    }
}

/// Main-gun breech state (player only). `Spent` = fired, hot casing still in the gun; `Empty` =
/// casing out, no round; `Loaded` = a round is seated and the gun can fire.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Breech {
    Loaded,
    Spent,
    Empty,
}

/// A discrete tactile emergency the player is mid-way through — it suppresses ALL station controls
/// while it runs ("both hands full"). Reload (eject/ram) is a Loader-STATION duty, not an Activity,
/// so the aim↔reload triage is enforced by the station switch, not by locking out the seat.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Activity {
    Extinguish,
    Repair(ModuleKind),
}

/// Which downed modules are a sane field-fix, the station that reaches them, and how long (exposed)
/// it takes. `None` = battle-destroyed / catastrophic, never repairable in the field.
fn repair_time(k: ModuleKind) -> Option<f32> {
    use ModuleKind::*;
    match k {
        LeftTrack | RightTrack => Some(8.0), // thrown track — re-set it, but hatch-out + stationary
        Transmission => Some(7.0),
        TurretTraverse => Some(3.0),  // clear a jam
        Radio => Some(2.0),           // reconnect
        CoaxMg | HullMg => Some(1.6), // clear a stoppage
        _ => None,                    // MainGun / GunnerSight / Engine / Fuel / AmmoRack: no
    }
}

/// One pooled rigid-body shell casing: ejected from the breech, tumbles + falls under gravity, bounces
/// once off the turret floor, clatters. Reuses the turret-pop / spark integration style. No rand.
#[derive(Clone, Default)]
struct Casing {
    active: bool,
    pos: Vec3,
    vel: Vec3,
    spin_axis: Vec3,
    ang: f32,
    spin: f32,
    t: f32, // life remaining
    bounced: bool,
    floor_y: f32, // world Y of the turret-floor bounce plane
    /// Riding the loader's hands (just pulled from the breech): physics + retirement paused;
    /// `drive_casings` draws it at the carry transform until the toss releases it.
    carried: bool,
}

/// The two surviving Tigers which join the player for Halbe. They are deliberately lightweight
/// compared with the player's embodied machine: formation driving and a slow, visible gun cycle are
/// enough to make the last stand a platoon action instead of another solitary arena.
#[derive(Clone)]
struct AllyTiger {
    tank: Tank,
    fire_cd: f32,
    formation_side: f32,
}

#[derive(Clone)]
struct CampaignHulk {
    kind: EnemyKind,
    models: [Mat4; 3],
    ember: f32,
    burning: bool,
}

struct Slots {
    tiger: [usize; 3],               // hull, turret, gun
    tiger_glass: [Option<usize>; 3], // Schutzglas parts (hull visor / cupola blocks / TZF), see-through
    tiger_rtt: [Option<usize>; 3],   // RTT periscope screen meshes (atlas-bound)
    hatchlever: usize,               // the cupola hatch lever (E toggles the hatch)
    gunlever: usize,                 // the breech lever (animated part, gun frame)
    breechcover: usize,              // the breech block cover (slides open with the lever)
    t34: Vec<[usize; 3]>,            // one T-34 hull/turret/gun triple per enemy slot
    su100: Vec<[usize; 3]>, // matching SU-100 triple; campaign waves can swap rigs per slot
    kv2: Vec<[usize; 3]>,
    isu152: Vec<[usize; 3]>,
    is2: Vec<[usize; 3]>,
    is3: Vec<[usize; 3]>,
    allies: Vec<[usize; 3]>, // two friendly Tigers in the Halbe finale
    hulk_t34: Vec<[usize; 3]>,
    hulk_su100: Vec<[usize; 3]>,
    hulk_kv2: Vec<[usize; 3]>,
    hulk_isu152: Vec<[usize; 3]>,
    hulk_is2: Vec<[usize; 3]>,
    hulk_is3: Vec<[usize; 3]>,
    /// ABANDONED TIGERS (the Last Tiger rule): up to 3 spare hulls scattered per act — reach one
    /// on foot after bailing out and the run continues. Empty on the arena (act 0).
    abandoned: Vec<[usize; 3]>,
    blast: [usize; N_BLAST], // pooled big fireballs (kill / turret-pop / cook-off)
    debris: [usize; N_DEBRIS], // pooled spark/debris chunks
    flame: Vec<usize>,       // one persistent flickering flame plume per enemy wreck
    interior_fire: Vec<usize>, // the PLAYER's own interior fire, seen from inside as it spreads
    bulb: usize,             // warm interior bulb, lit only when the player is buttoned up
    hatch: usize,            // cupola hatch lid (swings on a side hinge about engine-Y)
    // Animated interior parts (Phase 5): real baked meshes the engine drives.
    tpz_needle: usize,    // TPZ dial needle — spins 1:1 with turret-vs-hull bearing
    wheel1: usize,        // elevation handwheel — cranks with gun pitch
    wheel2: usize,        // traverse handwheel — cranks with turret yaw
    chamber_round: usize, // the chambered 8.8 round, visible while Breech::Loaded
    chamber_round_he: usize, // the SprGr twin (visibility swaps on the loaded nature)
    shell0: usize,        // first of SHELL_SLOTS.len() contiguous rack-shell instances
    snow: [usize; N_SNOW], // pooled near-miss snow/dust puffs (round-robin)
    // ── Crew-of-one interior workload FX (all pushed before the mg_flash/muzzle tail) ──
    breech: usize,    // main-gun breech block (heat rim glows amber right after firing)
    chambered: usize, // brass round seated in the breech (visible only when Loaded)
    ram_round: usize, // fresh round sliding home, Z driven by `ram`
    ready_rack: [usize; 3], // ready-rack rounds beside the breech (diegetic dressing)
    loose_shell: [usize; N_LOOSE], // dropped rounds rolling on the basket floor
    loose_shell_he: [usize; N_LOOSE], // yellow SprGr visual twins
    hammer: usize,    // first-person hammer (the track-repair ritual)
    toolbox: usize,   // the track-repair kit on the turret wall (hidden while carried)
    track_repair_shoes: [usize; N_REPAIR_SHOES], // loose run physically pulled over the wheels
    track_replacement_link: usize, // fresh link moved out of the kit and into the open break
    extinguisher: usize, // the wall-mounted extinguisher bottle (in hands while spraying)
    casing: [usize; N_CASING], // tumbling ejected casings (rigid body)
    ext_spray: [usize; N_EXT], // extinguisher foam cones (legacy stream)
    foam: [usize; N_FOAM], // voxel foam blobs: shot from the nozzle, stick, fade
    annun: [usize; N_LAMP], // interior annunciator lamps (fire/breech/repair/station)
    breech_lamp: [usize; 3], // ON-BREECH state lamp: [green, orange, red] (one lit at a time)
    lan_crew: [usize; 5], // one visible low-poly body per connected role
    // ── Coax + hull MG34 FX (positioned before the muzzle/tracer/impact/smoke/e_fx tail so the
    // final-16 legacy FX layout — `mesh_count()-13` = smoke — is unchanged) ──
    mg_flash: usize, // single muzzle-flash mesh, placed at the live MG's port
    mg_tracer: [usize; N_MG_TRACER],
    mg_impact: [usize; N_MG_IMPACT],
    // ── ABSITZEN on-foot slots — pushed BEFORE the FX block so the FX block stays last and
    // `run_fire_demo`'s `mesh_count()-13` invariant holds untouched.
    pickup0: usize,             // first supply-pickup mesh slot
    n_pickups: usize,           // count of contiguous pickup slots
    luger: usize,               // first-person Luger pistol
    luger_flash: usize,         // Luger muzzle-flash box
    soldier0: usize, // first infantry mesh slot; N_SOLDIERS contiguous, 2 meshes each (body, rifle)
    inf_flash: [usize; INF_FX], // infantry return-fire muzzle boxes
    muzzle: usize,
    tracer: usize,
    impact: usize,
    smoke: usize,
    e_tracer: [usize; E_FX_POOL], // enemy return-fire tracer pool
    e_impact: [usize; E_FX_POOL],
    // ── Death instant-replay (kill-cam) FX — driven ONLY by render_replay, never in normal play ──
    r_shell: usize,             // the incoming shell streak (shooter → entry, slow-mo)
    r_muzzle: usize,            // shooter muzzle flash at t≈0
    r_flash: usize,             // penetration entry flash / lingering ember
    r_shards: [usize; N_SHARD], // spall/shrapnel fragments sprayed into the interior
    // X-RAY interior proxies (kill-cam only): the 5 crew + 12 modules as schematic boxes shown
    // through the screen-door-ghosted hull, the ones THIS shell killed flashing hot.
    r_crew: [usize; 5],
    r_mod: [usize; 12],
}

#[derive(Default)]
struct Fx {
    muzzle_t: f32,
    tracer_active: bool,
    tracer_t: f32,
    tracer_dur: f32,
    tracer_from: Vec3,
    tracer_to: Vec3,
    tracer_dir: Vec3,
    pending_target: Option<usize>, // enemy index the in-flight shell is resolving against
    pending_struct: Option<usize>, // destructible the round TERMINATED in (AP budget died / HE face)
    pending_tree: Option<usize>,   // destructible-tree index the shell struck (snaps on impact)
    // ── Pen-ray freight (docs/DESTRUCTION.md §3): everything the round did along its route.
    // Each entry carries its DISTANCE down the route — the effect applies the moment the tracer
    // passes it (the wall puffs as the round punches it), not seconds later at landing.
    pending_transits: Vec<(usize, f32)>, // walls holed clean THROUGH (light chip each)
    pending_trees: Vec<(usize, Vec3, f32)>, // trunks snapped along the way (+ local dir, dist)
    pending_he: Option<(Vec3, f32)>,     // HE: burst point + blast radius (radial resolution)
    tracer_route: Vec<Vec3>,             // waypoints muzzle→terminal; kinks at wall transits
    pending_outcome: Outcome, // Bounce (clang, survives) / Penetrate (killed) / Ground (miss)
    impact_t: f32,
    impact_pos: Vec3,
    impact_kind: Outcome, // how to render the impact FX (spark vs big blast vs ground puff)
    smoke_t: f32,         // muzzle smoke plume lifetime (longer, drifting cloud at the barrel)
    smoke_dur: f32,
    recoil: f32, // camera kick-back energy, decays each frame
}

/// One in-flight enemy shot (tracer + its impact). Pooled so several T-34s can be firing at once;
/// `Game::e_shots` holds E_FX_POOL of these, cycled round-robin as enemies fire.
#[derive(Default, Clone)]
struct EnemyShot {
    tracer_active: bool,
    tracer_t: f32,
    tracer_dur: f32,
    tracer_from: Vec3,
    tracer_to: Vec3,
    tracer_dir: Vec3,
    outcome: Outcome, // resolved at fire time (arc + range), applied when the tracer lands
    arc: Arc, // which armor arc the round struck — carried to impact so a killing penetration
    // can name the facing (flank/rear/front) as the player's cause of death
    /// The full ballistic resolution (shell, striking velocity, plate, terminal effect) — carried to
    /// impact so the parametric CLANG is synthesized from what ACTUALLY hit (acoustics.rs bridge).
    ev: Option<crate::ballistics::ImpactEvent>,
    /// Whether the ballistic roll struck the TURRET band (vs hull) — kept so the sound's panel and
    /// the spall model's compartment agree about where the round landed.
    turret_hit: bool,
    /// Enemy-vs-enemy: the OTHER enemy tank this round terminated on (friendly fire / brawls).
    /// Resolved at fire time; a Penetrate destroys it when the tracer lands.
    hit_enemy: Option<usize>,
    near_pt: Vec3, // for a NearMiss: the point on the round's path CLOSEST to the Tiger (where it
    // snapped by) — the snow puff is kicked up on the ground here, not downrange
    pending_struct: Option<usize>, // destructible-structure index this round struck (a stray wall hit)
    impact_t: f32,
    impact_pos: Vec3,
    impact_kind: Outcome, // how to render the impact FX
}

/// A combat sound in flight to the player's ears — held in `Game::scheduled` and released by
/// `update` once its wavefront arrives (delay = source range / speed of sound). `buf`, when set,
/// is a SYNTHESIZED interleaved-stereo buffer (the parametric distant gun report) played instead of
/// the sampled `sfx` — the flash→bang path renders what that gun sounds like AFTER the air has
/// eaten its highs (acoustics::synth_gun_report). `volume` is the
/// already-range-attenuated amplitude handed to `Audio::play_at_volume`. Cheap and deterministic:
/// the queue is a plain Vec drained each frame, no allocation per sound beyond the push.
struct ScheduledSound {
    delay: f32, // seconds of travel time still remaining
    sfx: crate::audio::Sfx,
    volume: f32, // range-attenuated playback volume
    /// World origin retained until the wavefront arrives. The listener may have moved or turned in
    /// the intervening seconds, so pan and the direct acoustic ray are resolved at hear-time.
    source: Vec3,
    /// Synthesized stereo buffer (distant gun report etc.) — played INSTEAD of `sfx` when present.
    buf: Option<Vec<f32>>,
}

/// THE CLIMB RITUAL (embodied-interior Stage 1): mounting/dismounting the Tiger is a timed,
/// camera-scripted sequence up a ladder of perches — each Space is a committed ~0.5 s move (the
/// vulnerability window while enemies hunt you). Symmetric: 3 moves in, 3 moves out.
#[derive(Clone)]
struct Climb {
    active: bool,
    mounting: bool,  // true = ground → inside; false = inside → ground
    step: usize,     // index of the perch we're AT (or lerping FROM while `anim`)
    t: f32,          // 0..1 lerp progress within the current move
    anim: bool,      // true = a move is running (input locked); false = perched, awaiting Space
    cams: Vec<Vec3>, // camera world positions for each perch (eye-height baked in)
    end: ClimbEnd,   // what finishing the sequence does
}

#[derive(Clone, Copy)]
enum ClimbEnd {
    Afoot,
    IntoTank,
    IntoSpare(usize),
}

impl Default for Climb {
    fn default() -> Self {
        Climb {
            active: false,
            mounting: false,
            step: 0,
            t: 0.0,
            anim: false,
            cams: Vec::new(),
            end: ClimbEnd::Afoot,
        }
    }
}

/// Seconds per climb move — long enough to read as a physical heave, short enough to stay tense.
const CLIMB_STEP_S: f32 = 0.55;

/// The on-screen prompt for the current climb perch (Space advances; the last descent also walks off).
fn climb_prompt(mounting: bool, step: usize, n: usize) -> &'static str {
    if mounting {
        match step {
            0 => "CLIMB ONTO THE HULL [SPACE]",
            1 => "UP ONTO THE TURRET [SPACE]",
            _ => "DROP INTO THE HATCH [SPACE]",
        }
    } else {
        let _ = n;
        match step {
            0 => "PULL UP OUT OF THE CUPOLA [SPACE]",
            _ => "PULL OVER — ONTO THE CUPOLA [SPACE]",
        }
    }
}

/// Clamp an interior walk step to the TURRET FLOOR footprint, sliding around the breech box
/// (the one obstacle). Turret-local; only x/z move.
fn clamp_interior_walk(from: Vec3, to: Vec3) -> Vec3 {
    use crate::interior_rig as rig;
    let pad = 0.16; // body radius
    let mut p = to;
    p.x = p.x.clamp(rig::FLOOR_MIN.x + pad, rig::FLOOR_MAX.x - pad);
    p.z = p.z.clamp(rig::FLOOR_MIN.z + pad, rig::FLOOR_MAX.z - pad);
    let (bmin, bmax) = (rig::BREECH_MIN, rig::BREECH_MAX);
    let hit = |q: Vec3| {
        q.x > bmin.x - pad && q.x < bmax.x + pad && q.z > bmin.z - pad && q.z < bmax.z + pad
    };
    if hit(p) {
        // slide along whichever axis stays clear; a fully-blocked step stops
        let tryx = Vec3::new(p.x, p.y, from.z);
        let tryz = Vec3::new(from.x, p.y, p.z);
        p = if !hit(tryx) {
            tryx
        } else if !hit(tryz) {
            tryz
        } else {
            from
        };
    }
    p
}

/// Director events handed to the persistent campaign coordinator. They are deliberately terse: the
/// world communicates these beats through space, wrecks and objects; this enum only prevents a scene
/// rebuild from forgetting what the player actually did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionSignal {
    Objective(crate::campaign::ObjectiveId, u16),
    Discovery(crate::campaign::StorySiteId),
}

/// Minimal front-end telemetry. This is ammunition/fuel pressure and battle cadence, not a quest
/// checklist—the player still reads routes and story from the environment and paper map.
#[derive(Clone, Copy, Debug)]
pub struct MissionHud {
    pub act: u8,
    pub encounter: usize,
    pub encounters: usize,
    pub fuel_liters: f32,
    pub rack_rounds: usize,
    pub route_locked: bool,
    pub finale: bool,
    pub friendly_tigers: u8,
    pub town_integrity: u8,
}

// ── KOOP demo mission: TAKE & HOLD the compound (STÜTZPUNKT) ─────────────────────────────────────
// A small self-contained op on the act-0 arena bowl, built for the splitscreen couch crew: drive
// up from the southern approach, clear the two dug-in defenders out of the origin ruin compound,
// raise the capture by standing your Tiger in the yard, then hold it against three counterattack
// waves of T-34s. Orthogonal to the campaign director — pure `Game` state, HUD read by the
// front-end via `compound_hud`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompoundPhase {
    /// Driving up — the compound is held by a small picket.
    Advance,
    /// Inside the fight for the yard: clear defenders, stand in the zone to raise the capture.
    Take,
    /// A staged counterattack wave is live.
    Hold,
    /// Between waves (or before the first): a breather with a countdown.
    Breather,
    /// All waves broken — the position held.
    Won,
}

/// Front-end snapshot for the STÜTZPUNKT HUD.
#[derive(Clone, Copy, Debug)]
pub struct CompoundHud {
    pub phase: CompoundPhase,
    pub capture: f32,
    pub wave: u8,
    pub waves: u8,
    pub enemies_left: u8,
    pub timer: f32,
    pub dist_m: f32,
    pub bearing_deg: f32,
    pub contested: bool,
}

struct CompoundMission {
    center: Vec2,
    radius: f32,
    phase: CompoundPhase,
    capture: f32,
    wave: u8,
    timer: f32,
    seed: u32,
    contested: bool,
}

pub const COMPOUND_WAVES: u8 = 3;

#[derive(Clone, Debug)]
struct CampaignRuntime {
    plan: crate::campaign::EncounterPlan,
    encounter: usize,
    unit_cursor: usize,
    wave_live: bool,
    transition_t: f32,
    distance_m: f32,
    last_pos: Vec2,
    milestone_bits: u16,
    discovered: Vec<crate::campaign::StorySiteId>,
    signals: Vec<MissionSignal>,
    inventory: crate::campaign::CampaignInventory,
    fuel_liters: f32,
    total_kills: u16,
    battle_pressure: f32,
    debug_force_triggers: bool,
    /// A rejected completion is edge-triggered: the crew must physically leave the gate before it
    /// can be tried again.  Without this latch, standing inside the goal retries every frame.
    gate_rearm_wait: bool,
}

fn story_sites(act: u8) -> &'static [(crate::campaign::StorySiteId, f32, f32)] {
    use crate::campaign::StorySiteId::*;
    const ACT1: &[(crate::campaign::StorySiteId, f32, f32)] = &[
        (Act1GrainElevator, 900.0, 1100.0),
        (Act1BarmaleyFountain, 300.0, -150.0),
        (Act1UnivermagCellar, -842.0, -258.0),
        (Act1PitomnikAirfield, -1340.0, -860.0),
    ];
    const ACT2: &[(crate::campaign::StorySiteId, f32, f32)] = &[
        (Act2KienitzIceTracks, 1300.0, 500.0),
        (Act2OldHouse, 430.0, 205.0),
        (Act2DrownedOderbruch, -1490.0, 515.0),
        (Act2ReitweinBunker, -690.0, -805.0),
    ];
    const ACT3: &[(crate::campaign::StorySiteId, f32, f32)] = &[
        (Act3JammedColumn, -860.0, 315.0),
        (Act3DryTiger, -1265.0, -75.0),
        (Act3AutobahnBlockingLine, 650.0, -300.0),
        (Act3ForestCemetery, -1460.0, -245.0),
    ];
    match act {
        1 => ACT1,
        2 => ACT2,
        3 => ACT3,
        _ => &[],
    }
}

pub struct Game {
    pub hf: Heightfield,
    /// The act this world was built from — the paper map (M) and act-flow logic read it.
    pub act: crate::world::ActMap,
    pub player: Tank,
    /// How the player Tiger was knocked out, set at the instant of the killing penetration (see
    /// `resolve_player_penetration`). `None` until dead; read by the front-end for the death screen.
    pub death_cause: Option<DeathCause>,
    /// Runtime ground cover (camera-chunked wind-swept blades; kessel-map-design). None only if
    /// construction is refactored; built for every act (winter acts get sparse dead straw).
    grass: Option<crate::grass::Grass>,
    pub enemies: Vec<Tank>,
    allies: Vec<AllyTiger>,
    ally_shot_tick: u32,
    campaign_hulks: Vec<CampaignHulk>,
    finale_town_integrity: f32,
    civilian_column_alive: bool,
    ally_attrition: f32,
    ally_damage_tick: u32,
    /// Logical activation mask for the six reusable GPU enemy slots. Campaign encounters stream
    /// several authored groups through these slots; inactive ones must not render as phantom wrecks.
    enemy_active: Vec<bool>,
    enemy_skill: Vec<u8>,
    enemy_concealed: Vec<bool>,
    campaign: Option<CampaignRuntime>,
    /// KOOP demo mission — TAKE & HOLD the arena ruin compound (`start_compound`). Orthogonal to
    /// the campaign director; `None` everywhere else.
    compound: Option<CompoundMission>,
    wrecks: Vec<Wreck>, // per-enemy wreck state (parallel to `enemies`); inert until destroyed
    blasts: Vec<Blast>, // pooled fireball FX (len == N_BLAST)
    sparks: Vec<Spark>, // pooled debris FX (len == N_DEBRIS)
    snow_puffs: Vec<SnowPuff>, // pooled near-miss snow/dust puffs (len == N_SNOW)
    // ── Coax + hull MG34 state ──
    mg_tracers: Vec<MgTracer>, // pooled MG tracer streaks (len == N_MG_TRACER)
    mg_impacts: Vec<MgImpact>, // pooled MG impact spurts (len == N_MG_IMPACT)
    next_mg_tracer: usize,     // round-robin cursor into mg_tracers / mg_tracer mesh slots
    next_mg_impact: usize,     // round-robin cursor into mg_impacts / mg_impact mesh slots
    mg_timer: f32,             // countdown to the next MG round while the trigger is held
    mg_round: u32, // rounds fired — tracer cadence + dispersion + sound pacing (no rand)
    mg_flash_t: f32, // muzzle-flash flicker persistence (re-armed each round)
    mg_flash_pos: Vec3, // world position of the live MG's muzzle flash this frame
    mg_heat: f32,  // 0..1 barrel heat (light overheat model — can't hold it forever)
    mg_overheated: bool, // locked out after heat hit 1.0; clears once cooled below MG_COOL_RESUME
    mg_hot: bool,  // rounds actually left the barrel this frame (drives the MG audio loop)
    gun_shots: u32, // main-gun shots fired — cycles the per-shot report variants (no rand)
    engine_death_cued: bool, // the Maybach death rattle has played (module KO or tank lost) — once
    next_blast: usize, // round-robin cursor into blasts / blast mesh slots
    next_spark: usize, // round-robin cursor into sparks / debris mesh slots
    next_snow: usize, // round-robin cursor into snow_puffs / snow mesh slots
    kill_tick: u32, // counter-based rolls for catastrophic / cook-off (deterministic)
    pub gunner_optic: bool, // false = commander drive view (hull), true = gunner sight (down the gun)
    pub commander_outside: bool, // C key: head OUT of the cupola (requires hatch_open) vs periscope
    /// The physical cupola hatch: open exposes you (fire vents, but fire/splinters/eyes get in);
    /// you MUST open it to bail. `hatch_t` eases 0(closed)→1(open) so the lid swings, not snaps.
    pub hatch_open: bool,
    hatch_t: f32,
    hover_hatch: bool, // the crosshair is resting on the hatch this frame (highlightable)
    hover_lever: bool, // the crosshair is resting on the cupola hatch LEVER (LMB/E works it)
    /// Click-to-enter: the STATION the crosshair is resting on this frame (gunsight → Gunner,
    /// breech → Loader, cupola well → Commander). LMB enters it; replaces the C/Shift/Q hotkeys.
    hover_station: Option<Station>,
    /// Main-gun rounds fired this run — depletes the visible rack (65 authored slots).
    rounds_spent: u32,
    /// Interior walk position (turret-local, on the TURRET FLOOR) while posted INSIDE (Loader).
    walk_pos: Vec3,
    /// INTERIOR HAZE (0..1): propellant smoke fills the buttoned-up fighting compartment. A shot
    /// adds a little (muzzle backdraft); OPENING THE BREECH to reload adds the big puff (gas vents
    /// off the hot casing). The bore-evacuator FAN drains a steady rate; an OPEN HATCH vents it
    /// fast (natural draft). Rapid sustained fire without venting → you're firing blind in your own
    /// smoke. Pushed to the crush veil, gated by being buttoned up inside.
    interior_haze: f32,
    /// A fired case has one charge of trapped bore gas. The first opening of the block consumes it;
    /// waving the lever afterward cannot manufacture another smoke cloud.
    bore_smoke_pending: bool,
    pub chase_cam: bool,  // F1 dev/photo cam — NOT the shipped first-person view
    force_buttoned: bool, // KESSEL_BUTTONED headless hook: pin the commander buttoned-up interior view
    force_driver: bool,   // KESSEL_DRIVER headless hook: pin the driver station (visor-slit view)
    head_yaw: f32,        // commander head-look offset from hull forward (mouse)
    head_pitch: f32,
    /// Interior warmth 0..1: 1 while buttoned up (gunner optic / hatch closed), 0 when exposed. Drives
    /// the warm tungsten fill + bulb. Pushed to the renderer's `interior_light` uniform each frame.
    interior_light: f32,
    slots: Slots,
    fx: Fx,
    e_shots: Vec<EnemyShot>, // enemy return-fire FX pool (len == E_FX_POOL)
    next_e_fx: usize,        // round-robin cursor into e_shots / the enemy FX mesh slots
    ai_tick: u32,            // counter-based aim jitter (deterministic, no rand)
    armor_tick: u32,         // counter-based penetration roll (deterministic, no rand)
    /// Concussion state: elapsed seconds into the current deafening + its total duration (0 = none).
    /// While active, the whole mix ducks under the tinnitus ring (acoustics::concussion_duck).
    concussion_t: f32,
    concussion_dur: f32,
    /// Set once the player reaches the act's goal gate (the red X). The front-end reads it and
    /// advances the act flow (W5); never reset within a life.
    pub act_complete: bool,
    /// TZF magnification stage (RMB cycles 0/1/2 = 25 deg / 12 deg / 6 deg true field). Stages 0-1
    /// match the historical TZF 9b/9c fields (2.5x / 5x); stage 2 is HONEST COMPENSATION for the
    /// 640-px sensor: a trained eye through 5x glass resolves ~5x finer than our pixel grid, so
    /// without it a T-34 at the Tiger's real 800-1500 m envelope is a 3-px dither blob. The third
    /// stage restores the historical engagement envelope (and makes the 4-mil stadia triangles
    /// span ~14 px — readable for ranging) without touching the pixel aesthetic.
    pub sights_mag: u8,
    /// God mode (the menu's Invincible toggle; also the trainer's parked referee): penetrations,
    /// fire cook-offs, and on-foot hits all resolve (sound/FX) but never damage crew/modules or kill.
    pub god: bool,
    /// INSIDE-view manual reload: LMB is holding a shell (mouse lifts it to the breech; a flick rams).
    grab: bool,
    /// GUNNER mouse lay (kb+mouse controls): the mouse writes this DESIRED lay (hull-relative
    /// turret yaw + gun pitch); the turret/gun CHASE it at their real traverse/elevation rates —
    /// flick to a target, then wait for the hydraulics. Synced to the actual lay off-station.
    lay_yaw: f32,
    lay_pitch: f32,
    /// Dropped rounds loose in the fighting compartment (see `LooseShell`).
    loose: [LooseShell; N_LOOSE],
    // ── The physical reload's hands (see TRAY_MOUTH block for the design) ──
    slot_taken: Vec<bool>, // per SHELL_SLOTS rack round: grabbed/spent
    carry_mesh: Option<CarriedMesh>, // Some while a ROUND rides the hands (grab is the bool twin)
    carry_pos: Vec3,       // sprung carry state, world space
    carry_vel: Vec3,
    carry_dir: Vec3,               // the round's nose, chases the look with droop
    carried_casing: Option<usize>, // casings-pool index riding the hands (hot case pulled free)
    casing_pull: f32,              // 0..1 — LMB-held progress working the case out
    drive_cmd: f32,                // the GATED drive command this frame (engine-audio rev source)
    /// ENGINE SHUTOFF (Z — Zündung): the silent-watch / scout loop. Off = immobile, engine loop
    /// silent, detection envelope collapsed (see update_stealth). Restart is a crank + a LOUD beat.
    pub engine_on: bool,
    pub engine_crank_t: f32, // > 0: the starter is turning; engine catches when it hits 0
    start_noise_t: f32,      // post-start racket: temporarily widens the envelope
    prev_ignition: bool,
    /// Scout marks (dismounted recon): hand-inked enemy positions for the paper map. Position
    /// snapshots at marking time — the ink does not follow a tank that moves (honest recon).
    pub marks: Vec<glam::Vec2>,
    mark_dwell: Option<(usize, f32)>, // (enemy index, seconds the glasses have held on him)
    /// E-equipped: the extinguisher is OFF the wall and in your hands (LMB sprays; E puts it back).
    pub ext_equipped: bool,
    /// Breech lever position: 0 = closed/down (fireable), 1 = fully open (breech workable).
    lever_open: f32,
    lever_pulling: bool, // LMB is working the lever THIS frame — freezes the head-look (ram-style)
    hatchlever_t: f32,   // eased hatch-lever swing (chases the hatch state)
    slot_nature_he: Vec<bool>, // per rack slot: false = PzGr (grey), true = SprGr (yellow)
    rtt_screens: Vec<RttScreen>,
    port_aps: Vec<RttScreen>, // see-through port openings — interior light-shaft apertures
    rtt_dev: Option<(wgpu::Device, wgpu::Queue)>, // clones for the per-frame periscope tile render
    rtt_warmed: u32, // screens warm-started so far (every tile renders once, then gaze-driven)
    foams: [Foam; N_FOAM],
    next_foam: usize,
    foam_emit_t: f32,
    ext_want_spray: bool, // trigger squeezed this frame (foam FX answers it, fire or not)
    /// TRACK REPAIR ritual: take the kit inside, then outside grab the loose run, physically drag it
    /// back over the wheels, fit a replacement shoe, and hammer its pin home.
    carrying_track_kit: bool,
    track_repair_phase: TrackRepairPhase,
    track_pull: f32, // 0..1 — loose run dragged back over the wheels with downward mouse pulls
    track_hits: u8,  // hammer blows landed on the break so far
    hammer_t: f32,   // >0: swing animation running (counts down)
    /// INSIDE-view hotspot under the crosshair this frame (mouse-look + point-and-interact).
    hover: Option<Hotspot>,
    /// German hover prompt for the HUD (drawn by the front-end over the crushed frame).
    pub hover_label: Option<&'static str>,
    /// Backspace bail-out counter (stub until the on-foot mode lands).
    bail_presses: u32,
    scheduled: Vec<ScheduledSound>, // speed-of-sound delayed combat audio (see schedule_sound_from)
    ambient_t: f32,                 // countdown to the next distant battlefield explosion
    ambient_tick: u32,              // counter-based pseudo-random (interval + bearing + clip pick)
    audio: Option<crate::audio::Audio>,
    // ── Stealth / detection ──────────────────────────────────────────────────────────────────
    pub night: bool, // day/night detection scaling; the menu's Tag/Nacht will feed this
    stealth: stealth::Stealth, // per-enemy detection meters + the platoon alert state machine
    /// World-space cover boxes that BREAK a T-34's sightline to the player: the city buildings, the
    /// ruined-town wall/barricade dressing, the scattered battlefield cover, and the hull of every
    /// tank already killed (appended in `destroy_enemy`). Built once at scene-build time and read by
    /// `los_clear` — this is what makes hiding behind hard cover actually conceal the Tiger.
    occluders: Vec<stealth::Occluder>,
    /// Static substantial pieces from the additive story layer. Unlike cans, grass and luggage,
    /// these wrecks/barricades stop a tank and catch rounds as well as blocking sight.
    story_cover: Vec<(Vec3, Vec3)>,
    /// Engine-native destructible structures — near city walls/facades + free-standing cover walls.
    /// Each carries its intact + pre-allocated rubble mesh slots, world AABB, per-piece HP, and the
    /// index of its dedicated LOS occluder (downgraded on collapse). Built once in `Game::build`.
    destructibles: Vec<Destructible>,
    /// Destructible trees (docs/DESTRUCTION.md §5): hinge-fall pines/firs on the instanced path.
    pub trees: crate::trees::Trees,
    /// Chunk-grid walls + ballistic debris (docs/DESTRUCTION.md §6): thin axis-aligned walls
    /// carry a pre-allocated brick-course grid — AP opens holes, HE blows spheres, unsupported
    /// courses cascade, dead chunks fly then freeze as persistent rubble.
    pub chunks: crate::rubble::ChunkWalls,
    // TRACK CRUSH state (docs/DESTRUCTION.md §4): the camera rumble energy while grinding a wall
    // (decays each frame), its deterministic oscillator phase, the throttle on grind FX/audio, and
    // whether last frame was already in contact (gates the one-shot contact crunch + telemetry).
    crush_rumble: f32,
    crush_phase: f32,
    crush_fx_t: f32,
    crush_was: bool,
    /// Last frame's LMB, for edge-triggering the main gun (a held/click-through LMB must not fire).
    prev_lmb: bool,
    /// The CHAMBERED round's nature: false = PzGr 39 (AP), true = SprGr (HE). Toggled by T —
    /// interim instant hand-swap until the loader-station ritual owns shell selection.
    he_loaded: bool,
    /// Index of the guaranteed procedural demo wall in `destructibles` (always present, even without
    /// `assets/`), driven by the `KESSEL_DEMOLISH` headless hook. `None` only if the wall was culled.
    demo_struct: Option<usize>,
    // ── In-tank damage model (player Tiger only) ───────────────────────────────────────────────
    /// The five crewmen as individuals — each with an interior station + ok/incapacitated state.
    pub crew: Vec<Crewman>,
    /// The twelve modules as individuals — each with a functional flag.
    pub modules: Vec<Module>,
    /// The killing penetration, captured for the instant-replay. `None` until the Tiger is dead.
    pub last_fatal: Option<FatalHit>,
    fire: f32, // interior fire intensity (0 = none); grows once alight
    fire_active: bool,
    fire_t: f32,                             // fire clock (drives flicker)
    fire_ammo_timer: Option<f32>, // countdown once the fire threatens the ammo → cook-off kill
    fire_origin: Option<(Vec3, Vec3, Vec3)>, // (shooter, entry, shell_dir) that lit the fire — for the death record
    crew_ko_t: f32, // crew-KO screen-effect timer (counts down from CREW_KO_DUR)
    crew_ko_world_dir: Vec3, // world travel vector of the round that fired the KO (→ screen spray axis)
    force_crew_ko: Option<f32>, // KESSEL_CREWKO=<seconds> pins the effect at a phase for a screenshot
    force_crew_ko_dir: Option<[f32; 2]>, // KESSEL_CREWKO_DIR=<deg> pins the screen spray axis for a screenshot
    // ── Death instant-replay (kill-cam) ────────────────────────────────────────────────────────
    replay_cue: u8, // one-shot audio gate for the replay (0 none, 1 muzzle, 2 impact)
    // ── Crew-of-one interior workload (stations / manual reload / fire-fighting / repair) ─────────
    /// The posted station the ONE player is manning (source of truth). `gunner_optic` is a derived
    /// mirror set each frame so the existing gunner-view reads keep working.
    pub station: Station,
    switch_from: Station,       // seat being left, for the camera scramble lerp
    switch_t: f32,              // >0 = clambering between seats: NO station control, NO activity
    switch_dur: f32,            // the full cost of the in-progress switch (for the lerp)
    prev_space: bool,           // Space edge-detect (loader mash + single-shot fire safety)
    activity: Option<Activity>, // an emergency beat in progress — suppresses all station controls
    breech: Breech,             // main-gun breech state (Loaded / Spent / Empty)
    ram: f32,                   // 0..1 seating progress of the round being rammed
    eject_t: f32,               // manual-eject fumble timer (KO'd loader frees the hot casing)
    ext_t: f32,                 // extinguisher grab/spray timer
    repair_t: f32,              // field-repair progress (seconds into the current repair)
    coop_reload_clock: f32,     // LAN loader hold -> repeated physical ram/eject beats
    lan_local_role: Option<crate::lan::LanRole>, // body hidden for this process's own camera
    lan_post_mask: u8, // physical posts occupied by remote humans; blocks camera/body overlap
    prev_coop_hatch: bool,
    casings: Vec<Casing>,   // pooled rigid-body casings (len == N_CASING)
    next_casing: usize,     // round-robin cursor into casings / casing mesh slots
    pub driver_hatch: bool, // driver hatched out (exposed) to re-set a thrown track
    // ── ABSITZEN dismount state ──
    pub mode: Mode,
    foot: FootSoldier,
    pickups: Vec<Pickup>,
    // ── ABSITZEN stealth ──
    soldiers: Vec<Soldier>,
    cover: Vec<(Vec2, f32)>, // coarse LOS-blocking cylinders over the wall clusters (§6)
    noise: [NoiseEvent; 8],  // discrete loud-event ring (running footfalls, Luger shots)
    next_noise: usize,
    inf_flash_t: [f32; INF_FX], // per-slot muzzle-flash lifetime
    inf_flash_pos: [Vec3; INF_FX],
    next_inf_fx: usize,
    banked: crate::foot::Supplies, // exfil tally — only counts once you walk back and remount
    // ── Neuroevolution (M1 seam) ──
    /// When set, this net drives every enemy's maneuver + fire-discipline (the gun lay, stealth
    /// gates, reload and ballistics stay scripted — the net proposes, the rules dispose). `None`
    /// = the shipped hand-coded AI. Installed by the `train` subcommand / a weights artifact.
    pub enemy_policy: Option<crate::ai::Mlp>,
    /// Training telemetry: rounds that connected with the player this match (bounces + pens).
    pub stat_hits_on_player: u32,
    pub stat_pens_on_player: u32,
    /// Occluder count at scene build — `destroy_enemy` appends wreck occluders during a match;
    /// `reset_match` truncates back to this so training matches don't accumulate ghost cover.
    base_occluders: usize,
    /// The act-aware spawn this world was built with (arena ≠ story spawn) — reset_match stages here.
    spawn_xz: (f32, f32),
    /// Per-enemy coax-MG burst pacing vs the dismounted player (Foot mode only).
    enemy_mg_cd: Vec<f32>,
    /// BRAWL (Academy course F / films): enemies split into two teams by index parity and hunt
    /// EACH OTHER — the player is ignored. Enemy-vs-enemy ballistics resolves the hits.
    pub brawl: bool,
    /// DEV GARAGE: enemies are inert TARGETS — no AI, no movement, no return fire — but still
    /// fully destroyable by player fire. The gunnery range.
    pub targets_only: bool,
    /// The Last Tiger spares: world pose + whether each has been taken. Parallel to
    /// `slots.abandoned`.
    abandoned: Vec<(Vec3, f32, bool)>,
    /// A hulk swap queued by a spare-Tiger remount: (spare index, old hull/turret/gun pose) —
    /// applied next frame where the renderer is available.
    pending_hulk: Option<(usize, Mat4, Mat4, Mat4)>,
    /// The climb ritual state (mount/dismount timed sequence).
    climb: Climb,
    prev_jump: bool, // Space edge-detect on foot (jump / climb-advance)
}

impl Game {
    /// Build the full scene + renderer and return the game. `night` picks the time-of-day the scene
    /// is lit + dressed for (the menu's Tag/Nacht): it sets `Game::night` (stealth uses it for reduced
    /// night detection) and, via the renderer's `night` uniform, drives the dark moonlit sky/lighting
    /// and switches the city's window/lamp emissives ON (glowing) at Nacht, OFF at Tag.
    /// Legacy/default entry — builds the Stalingrad city act (Act 1). Kept so the many headless demos
    /// + the menu boot don't need to change; the act-driven maps go through `build_act`.
    pub fn build(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        night: bool,
    ) -> anyhow::Result<(Renderer, Game)> {
        Self::build_act(device, queue, night, &crate::world::act(1))
    }

    /// Build the world for a specific act: the terrain window (`TerrainConfig::for_act`) and the
    /// procedural town's seed + ruination come from the `ActMap`. Phase-0 hook for the per-act maps;
    /// spawn/goal-gate wiring + fuller procedural generation land in the following increments.
    pub fn build_act(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        night: bool,
        act: &crate::world::ActMap,
    ) -> anyhow::Result<(Renderer, Game)> {
        let manifest = if act.id >= 1 {
            let manifest = crate::act_manifest::ActManifest::load_for_act(act.id)?;
            manifest.log_terrain("game");
            Some(manifest)
        } else {
            None
        };
        let resolved_act = match &manifest {
            Some(manifest) => crate::world::resolve_manifest(act.clone(), manifest)?,
            None => act.clone(),
        };
        let act = &resolved_act;
        let (terrain_mesh, hf) = match &manifest {
            Some(manifest) => terrain::load_authored_grid(
                &manifest.terrain.heights,
                manifest.terrain.identity.res,
                manifest.terrain.identity.spacing,
                &manifest.terrain.texture,
            )?,
            None => {
                println!(
                    "[kessel] game act 0 terrain: explicit legacy DEM path={} dimensions=360x360",
                    TERRAIN_PATH
                );
                terrain::load(TERRAIN_PATH, &TerrainConfig::for_act(act))?
            }
        };

        let mut scene: Vec<CpuMesh> = vec![terrain_mesh];

        // Scene-mesh slots of the city's emissive window panes / lamp heads, paired with their glow
        // intensity. Marked as night-gated emissives on the renderer AFTER it's built (below), and
        // shown only at Nacht — a dark building at Tag, a facade speckled with lit windows at Nacht.
        let mut window_slots: Vec<(usize, f32)> = Vec::new();

        // Cover occluders (LOS blockers for the stealth system). Accumulated alongside the scene as
        // the dressing is placed, then handed to `Game`. Kept coarse — a few dozen boxes total — so
        // the per-enemy per-frame sightline test stays a handful of cheap slab checks.
        let mut occluders: Vec<stealth::Occluder> = Vec::new();

        // Destructible standing structures — collected as the scene is assembled (near city walls/
        // facades + the free-standing cover walls). Each records its intact mesh slot, world AABB, HP,
        // and the index of its DEDICATED LOS occluder; the paired collapsed-rubble heap is
        // pre-allocated in one batch after all the static dressing but BEFORE the tank/FX tail, so the
        // final-16 FX layout (`mesh_count()-13` = smoke) is untouched.
        let mut dbuild: Vec<DestructibleBuild> = Vec::new();

        // ── Landmark hero meshes. The world assembles IN ENGINE (kessel-worldgen-workflow): the
        // user models landmarks in Blender and drops them at assets/hero/act<id>/<slug>.glb (origin
        // at ground contact, Y-up glTF, same conventions as the tank bakes); each act landmark tries
        // that path first (tank.rs-style) and otherwise raises a proportioned placeholder tower, so
        // navigation-by-silhouette works before the art exists and upgrades the moment it lands.
        for lm in &act.landmarks {
            let gy = hf.height_at(lm.pos.x, lm.pos.y) - 0.3;
            let path = format!(
                "assets/hero/act{}/{}.glb",
                act.id,
                crate::world::slug(&lm.name)
            );
            match crate::gltf_load::load_glb(&path) {
                Ok(m) => {
                    let xf = Mat4::from_translation(Vec3::new(lm.pos.x, gy, lm.pos.y));
                    scene.push(m.transformed(xf));
                    println!("[kessel] landmark '{}' ← {}", lm.name, path);
                }
                Err(_) => {
                    let h = lm.kind.height_hint();
                    let half = (h * 0.18).clamp(1.6, 6.0);
                    scene.push(crate::prims::box_mesh(
                        Vec3::new(lm.pos.x - half, gy, lm.pos.y - half),
                        Vec3::new(lm.pos.x + half, gy + h, lm.pos.y + half),
                        [172, 168, 158],
                    ));
                    println!(
                        "[kessel] landmark '{}': placeholder {:.0} m tower — drop {} to replace",
                        lm.name, h, path
                    );
                }
            }
        }

        // ABSITZEN supply-pickup placements: (kind, x, z) — hand-placed near the ruin clusters and
        // doorways already in the scene (north doorway/cluster, south doorway/cluster, the far
        // settlement edge, and a few just inside the city ring at (-30, 110)).
        // Depth-tiered — this IS the pacing dial. Safe ring (by the Tiger, no patrol) teaches the
        // collect verb; mid clusters add a route to cross; the DEEP city-ring caches hold the
        // cigarettes (the treasure) and best ammo, guarded by the sentries.
        const PICKUP_SPOTS: [(PickupKind, f32, f32); N_PICKUPS] = [
            (PickupKind::Ammo, -9.0, 9.0),      // safe ring — north doorway
            (PickupKind::Medical, -12.0, 11.0), // north cluster
            (PickupKind::Fuel, -15.0, 13.5),
            (PickupKind::Ammo, -22.0, 15.0),
            (PickupKind::Fuel, -20.0, -14.0), // south doorway
            (PickupKind::Ammo, 9.0, -12.0),   // south cluster
            (PickupKind::Medical, 12.0, -14.0),
            (PickupKind::Ammo, -30.0, -18.0),
            (PickupKind::Fuel, -6.0, 44.0),   // far settlement edge
            (PickupKind::Ammo, -30.0, 108.0), // deep — just inside the city ring (guarded)
            (PickupKind::Medical, -28.0, 112.0),
            (PickupKind::Fuel, -33.0, 110.0),
            (PickupKind::Cigarettes, -31.0, 110.0), // the treasure — deep in the ring
            (PickupKind::Cigarettes, -27.0, 107.0),
        ];

        // ── Parametric ruined Stalingrad — a shelled settlement to the north of the engagement axis
        // (the east-west firing corridor at z≈0 stays open; full city-edge scenario is a later pass).
        // Clustered into a coarse occluder grid: every tall (wall-height) piece merges into its
        // ~14 m cell's AABB, so a whole building footprint reads as one opaque cover box rather than
        // dozens of per-wall boxes. Flat rubble/floor slabs (below the sightline) are skipped.
        const CITY_CELL: f32 = 22.0;
        let mut city_cells: std::collections::HashMap<(i32, i32), (Vec3, Vec3)> =
            std::collections::HashMap::new();
        let mut city_solid: Vec<(Vec3, Vec3)> = Vec::new();
        for p in crate::city::generate(
            &crate::city::CityParams {
                center: CITY_CENTER,
                blocks_x: 3,
                blocks_z: 3,
                seed: act.seed,
                ruination: act.ruination,
            },
            &hf,
        ) {
            let (mn, mx) = (p.mesh.min, p.mesh.max);
            // A SOLID structural piece: tall AND with a real footprint. Emissive window panes / lamp
            // heads and thin posts/pillars never block a tank-centre sightline, so they're excluded.
            let tall =
                p.window.is_none() && mx.y - mn.y > 2.0 && (mx.x - mn.x).max(mx.z - mn.z) > 1.5;
            // A near-FRONT wall/facade (tall AND thin in one horizontal axis — a standing wall, not a
            // floor slab) becomes DESTRUCTIBLE with its OWN dedicated occluder, held OUT of the coarse
            // cover cluster so knocking it down actually reopens that sightline. Capped + front-half
            // only, so the set stays small and is the ruins the player actually faces.
            let wall_like = tall && (mx.x - mn.x).min(mx.z - mn.z) < 1.6;
            let near_front = mn.z < CITY_CENTER.y - 6.0;
            if tall {
                if wall_like && near_front && dbuild.len() < CITY_DESTRUCT_CAP {
                    let occ = occluders.len();
                    occluders.push(stealth::Occluder::from_aabb(mn, mx));
                    let max_dim = (mx - mn).max_element();
                    dbuild.push(DestructibleBuild {
                        intact: scene.len(),
                        occ,
                        min: mn,
                        max: mx,
                        hp: structure_hp(max_dim),
                        seed: 0x51A1_0000 ^ scene.len() as u32,
                        class: Material::Brick, // the ruined city is masonry throughout
                    });
                } else {
                    // Keep the real piece AABB for tank contact. The merged cell below is only a
                    // cheap LOS proxy; using it for physics would invisibly fill whole streets.
                    city_solid.push((mn, mx));
                    // Cluster the remaining solid pieces into the coarse cover grid as before.
                    let c = p.mesh.center();
                    let key = (
                        (c.x / CITY_CELL).floor() as i32,
                        (c.z / CITY_CELL).floor() as i32,
                    );
                    let e = city_cells.entry(key).or_insert((mn, mx));
                    e.0 = e.0.min(mn);
                    e.1 = e.1.max(mx);
                }
            }
            if let Some(intensity) = p.window {
                window_slots.push((scene.len(), intensity)); // slot = index this mesh takes below
            }
            scene.push(p.mesh);
        }
        for (_, (mn, mx)) in &city_cells {
            occluders.push(stealth::Occluder::from_aabb(*mn, *mx));
        }

        // ── Ruined-town set dressing around the spawn ──────────────────────────
        // A tiny loader cache: load a GLB once, reuse clones. Missing assets are skipped.
        let mut cache: std::collections::HashMap<String, Option<CpuMesh>> =
            std::collections::HashMap::new();
        // `y_override`: normally None, which grounds the piece on the terrain via hf.height_at +
        // sink (existing behavior). Roof pieces pass Some(y) instead — a roof shouldn't sit on the
        // snow, it needs to sit at a fixed height above the wall it caps.
        // `occlude`: when true, the placed piece's world AABB is also registered as an LOS occluder
        // (so it both RENDERS and BREAKS SIGHT). Silhouette cover — walls, arches, barricades,
        // containers, the scattered battlefield cover below — passes true; small ground clutter and
        // above-the-sightline roofs pass false (they'd never block a tank-centre sightline anyway).
        let mut place = |scene: &mut Vec<CpuMesh>,
                         occ: &mut Vec<stealth::Occluder>,
                         occlude: bool,
                         base: &str,
                         rel: &str,
                         dx: f32,
                         dz: f32,
                         rot: f32,
                         s: f32,
                         sink: f32,
                         y_override: Option<f32>| {
            let path = format!("{base}{rel}");
            let model = cache
                .entry(path.clone())
                .or_insert_with(|| crate::gltf_load::load_glb(&path).ok());
            if let Some(m) = model {
                let y = y_override.unwrap_or_else(|| hf.height_at(dx, dz) + sink);
                let xf = Mat4::from_translation(Vec3::new(dx, y, dz))
                    * Mat4::from_rotation_y(rot)
                    * Mat4::from_scale(Vec3::splat(s));
                let placed = m.clone().transformed(xf);
                if occlude {
                    occ.push(stealth::Occluder::from_aabb(placed.min, placed.max));
                }
                scene.push(placed);
            }
        };

        // Building shells / walls — mid distance, clustered into rough footprints on BOTH sides of
        // the spawn, deliberately kept off the east–west firing corridor to the T-34 (z≈0, x∈[-55,0]).
        // Buildings sink 0.3 m so they don't float on the snow slope.
        let structs: [(&str, f32, f32, f32, f32); 16] = [
            // north ruin cluster (a gutted house, left of the gun line)
            (STRUCT_WALL, -12.0, 11.0, 0.15, 1.9),
            (STRUCT_WALL_HOLE1, -15.0, 13.5, 1.65, 1.9),
            (STRUCT_DOORWAY, -9.0, 9.0, 0.15, 1.8),
            (STRUCT_PILLAR, -16.0, 9.5, 0.00, 2.0),
            // near north-west lone wall + arch
            (STRUCT_ARC, -26.0, 17.0, 2.20, 1.9),
            (STRUCT_WALL_HOLE2, -22.0, 15.0, -0.60, 1.8),
            // south ruin cluster (right of the gun line)
            (STRUCT_WALL, 9.0, -12.0, 2.55, 1.9),
            (STRUCT_WALL_HOLE1, 12.0, -14.0, 2.10, 1.8),
            (STRUCT_PILLAR, 13.0, -10.0, 0.00, 2.0),
            (STRUCT_DOORWAY, -20.0, -14.0, 1.05, 1.8),
            (STRUCT_ARC, -30.0, -18.0, 0.70, 1.9),
            // one piece behind the player (east) framing the spawn
            (STRUCT_WALL_HOLE1, 18.0, 7.0, -0.90, 1.9),
            // far north settlement edge, receding into the fog (fills the elevated recon look)
            (STRUCT_WALL, -6.0, 44.0, 0.30, 2.0),
            (STRUCT_WALL_HOLE1, -1.0, 47.0, 1.70, 2.0),
            (STRUCT_ARC, -13.0, 41.0, 0.90, 2.0),
            (STRUCT_PILLAR, 3.0, 43.0, 0.00, 2.1),
        ];
        for (rel, dx, dz, rot, s) in structs {
            // Building shells / walls are the primary hard cover near the spawn — register each as
            // an LOS occluder so a Tiger parked behind one is genuinely concealed.
            let si = scene.len();
            let oi = occluders.len();
            place(
                &mut scene,
                &mut occluders,
                true,
                MP,
                rel,
                dx,
                dz,
                rot,
                s,
                -0.30,
                None,
            );
            // Wall-type shells become DESTRUCTIBLE (doorways / pillars / arches are left standing).
            // Only if the asset actually loaded (place pushed exactly one mesh AND its occluder).
            let is_wall =
                rel == STRUCT_WALL || rel == STRUCT_WALL_HOLE1 || rel == STRUCT_WALL_HOLE2;
            if is_wall
                && scene.len() == si + 1
                && occluders.len() == oi + 1
                && dbuild.len() < STRUCT_MAX_DESTRUCTIBLES
            {
                let (mn, mx) = (scene[si].min, scene[si].max);
                dbuild.push(DestructibleBuild {
                    intact: si,
                    occ: oi,
                    min: mn,
                    max: mx,
                    hp: structure_hp((mx - mn).max_element()),
                    seed: 0x57A0_0000 ^ si as u32,
                    class: Material::Brick,
                });
            }
        }

        // Roofs — cap 3 of the wall clusters above so their silhouettes read as intact/
        // partially-intact buildings instead of bare wall shells. Each entry reuses the same
        // dx/dz/rot/scale as the wall it sits on (both packs' wall module is ~4m wide, 3m tall
        // unscaled, so the footprint lines up); the y is a fixed height above that wall's top,
        // not grounded on terrain, with a small overlap so there's no seam gap.
        let roofs: [(&str, f32, f32, f32, f32); 3] = [
            // north ruin cluster — caps the wall at (-12, 11)
            (ROOF_MH1, -12.0, 11.0, 0.15, 1.9),
            // south ruin cluster — caps the wall at (9, -12)
            (ROOF_BX1, 9.0, -12.0, 2.55, 1.9),
            // far settlement-edge cluster — caps the wall at (-6, 44), gives the skyline a roofline
            (ROOF_MH2, -6.0, 44.0, 0.30, 2.0),
        ];
        for (rel, dx, dz, rot, s) in roofs {
            let wall_top = hf.height_at(dx, dz) - 0.30 + 3.0 * s; // matches the wall's own sink + scaled height
            let y = wall_top - 0.15 * s; // slight overlap into the wall top, no seam gap
                                         // Roofs sit above the tank-centre sightline (their wall below already occludes), so they
                                         // don't need to be occluders themselves.
            place(
                &mut scene,
                &mut occluders,
                false,
                MP2,
                rel,
                dx,
                dz,
                rot,
                s,
                0.0,
                Some(y),
            );
        }

        // Brick-pile rubble — supplements (does not replace) the NAT stone rubble below, scattered
        // near the ruin clusters and along the far settlement edge. Hand-placed (not the hashy
        // scatter) so each one can sit deliberately near a cluster; positions checked by hand
        // against the corridor/tank guards used by the debris loop.
        let brick_rubble: [(&str, f32, f32, f32, f32); 7] = [
            (RUBBLE_BRICKS1, -14.0, 8.0, 0.40, 1.2),
            (RUBBLE_STACK1, -19.0, 16.0, 1.10, 2.2),
            (RUBBLE_BRICKS2, -24.0, 12.5, 2.00, 1.1),
            (RUBBLE_STACK2, 11.0, -9.0, 0.75, 2.0),
            (RUBBLE_BRICKS1, -18.0, -18.0, 1.60, 1.3),
            (RUBBLE_STACK3, -33.0, -15.0, 2.40, 2.3),
            (RUBBLE_STACK4, -4.0, 40.0, 0.20, 2.1),
        ];
        for (rel, dx, dz, rot, s) in brick_rubble {
            // Low decorative rubble — visual detail, not silhouette cover; left off the occluder list.
            place(
                &mut scene,
                &mut occluders,
                false,
                MP2,
                rel,
                dx,
                dz,
                rot,
                s,
                -0.10,
                None,
            ); // beds slightly into snow, like the NAT rubble
        }

        // Barricades — defensive chokepoint dressing near doorways/pillars, kept well off the
        // east–west firing corridor (same guard the debris loop uses).
        let barricades: [(&str, f32, f32, f32, f32); 5] = [
            (BARRICADE_A1, -8.0, 8.0, 1.20, 1.1), // fronting the north doorway
            (BARRICADE_B1, -17.0, 8.0, 1.90, 1.0), // beside the north pillar
            (BARRICADE_A2, -21.0, -15.5, 1.00, 1.2), // fronting the south doorway
            (BARRICADE_B3, 14.0, -9.0, 2.60, 1.0), // beside the south pillar
            (BARRICADE_B4, 4.0, 44.0, 0.50, 1.1), // far-edge pillar, dressing the skyline cluster
        ];
        for (rel, dx, dz, rot, s) in barricades {
            // Barricades are chest-high hard cover — register them.
            place(
                &mut scene,
                &mut occluders,
                true,
                MP2,
                rel,
                dx,
                dz,
                rot,
                s,
                0.0,
                None,
            );
        }

        // Industrial silhouettes — far settlement edge, adds skyline variety alongside the existing
        // far-edge wall/arc/pillar cluster. The container is a solid vehicle-sized block; the tall
        // chimney is a real sight-breaker too.
        place(
            &mut scene,
            &mut occluders,
            true,
            MP2,
            CHIMNEY,
            -20.0,
            46.0,
            0.0,
            7.0,
            0.0,
            None,
        );
        place(
            &mut scene,
            &mut occluders,
            true,
            MP2,
            CONTAINER,
            12.0,
            43.0,
            0.4,
            1.0,
            0.0,
            None,
        );

        // Ground clutter — crates, barrels, ammo boxes, rubble stones, logs. Deterministic scatter
        // (hashy trig off the loop index; no rand crate). Corridor-guarded so nothing lands on the
        // tank↔tank sightline, and nothing spawns within ~6 m of either tank.
        let debris = [
            (MP, DEBRIS_CRATE1, 2.0),
            (MP, DEBRIS_CRATE3, 2.0),
            (MP, DEBRIS_SUPPLY, 2.2),
            (MP, DEBRIS_BARREL1, 1.3),
            (MP, DEBRIS_BARREL2, 1.3),
            (MP, DEBRIS_AMMO, 2.2),
            (NAT, RUBBLE_STONE3, 0.8),
            (NAT, RUBBLE_STONE5, 1.0),
            (NAT, RUBBLE_LOG, 1.1),
        ];
        for i in 0..36u32 {
            let fi = i as f32;
            let a = fi * 2.399963; // golden-ish angle
            let r = 7.0 + (i % 6) as f32 * 5.0 + 5.0 * (fi * 1.7).sin().abs(); // 7..42 m
            let dx = r * a.cos();
            let mut dz = r * a.sin();
            // keep the firing corridor (z≈0 between the tanks) clear
            if dx < -2.0 && dx > -53.0 && dz.abs() < 7.0 {
                dz += if dz >= 0.0 { 8.0 } else { -8.0 };
            }
            // don't crowd either tank
            let near_player = (dx * dx + dz * dz).sqrt() < 6.5;
            let near_enemy = ((dx + 55.0).powi(2) + dz * dz).sqrt() < 6.5;
            if near_player || near_enemy {
                continue;
            }
            let (base, rel, s0) = debris[(i as usize) % debris.len()];
            let s = s0 * (0.75 + 0.5 * (fi * 0.9).cos().abs()); // scale variety
            let rot = a * 1.7;
            let sink = if base == NAT { -0.15 } else { 0.0 }; // rubble beds slightly into snow
                                                              // Small scattered ground clutter — decoration, not registered cover (keeps the occluder
                                                              // count down; the deliberate cover below is what the player actually hides behind).
            place(
                &mut scene,
                &mut occluders,
                false,
                base,
                rel,
                dx,
                dz,
                rot,
                s,
                sink,
                None,
            );
        }

        // ── COVER: real concealment between the spawn and the swarm's western approach lanes ─────
        // Substantially more HARD cover strewn flank-to-flank so the player can bound cover-to-cover
        // instead of crossing open snow: low sandbag-ish barricades, toppled-brick rubble berms,
        // building-shell walls, and wrecked-vehicle-sized blocks (shipping containers / crate
        // stacks). EACH is registered as an LOS occluder (occlude=true) so it both RENDERS and
        // BREAKS a T-34's sightline. Deterministic (hash01, no rand). Corridor-aware: pieces march
        // WEST down both flanks (the swarm's approach), with only a sparse LOW stagger near the z≈0
        // firing lane — the tank↔tank corridor is thinned, never walled — and the immediate spawn is
        // left open so a Tiger sitting in the clear is still spotted (the detection path stays live).
        const COVER_N: u32 = 30;
        let cover_palette: [(&str, &str, f32, f32); 10] = [
            (MP2, BARRICADE_A1, 1.5, 0.0), // low sandbag-ish wall
            (MP2, BARRICADE_B1, 1.4, 0.0),
            (MP2, RUBBLE_STACK1, 2.8, -0.1), // toppled-brick berm
            (MP2, RUBBLE_STACK3, 2.8, -0.1),
            (MP2, BARRICADE_B4, 1.4, 0.0),
            (MP, STRUCT_WALL, 2.0, -0.3), // building-shell wall (tall flank cover)
            (MP, STRUCT_WALL_HOLE1, 2.0, -0.3),
            (MP, STRUCT_ARC, 2.0, -0.3),
            (MP2, CONTAINER, 1.0, 0.0),    // wrecked-vehicle-sized block
            (MP, DEBRIS_SUPPLY, 2.4, 0.0), // supply-crate stack
        ];
        for i in 0..COVER_N {
            let h = |s: u32| hash01(i.wrapping_mul(2654435761).wrapping_add(s));
            // March WEST across the approach; each lane's cover gets steadily more distant.
            let dx = -16.0 - (i as f32) * 4.4 - h(1) * 6.0; // ~-16 .. ~-160 m
            let corridor = i % 3 == 2;
            let dz = if i % 3 == 0 {
                16.0 + h(2) * 74.0 // north flank  (z 16..90)
            } else if i % 3 == 1 {
                -16.0 - h(3) * 74.0 // south flank  (z -16..-90)
            } else {
                (h(4) - 0.5) * 16.0 // near-corridor stagger (|z| < 8) — duck cover, not a wall
            };
            // Keep the immediate spawn open so the stationary Tiger stays detectable.
            if (dx * dx + dz * dz).sqrt() < 13.0 {
                continue;
            }
            // The corridor stagger stays LOW (barricades / berms only) so the firing lane is thinned,
            // not blocked by a full building shell; the flanks may draw the tall cover too.
            let pick = if corridor {
                (h(5) * 5.0) as usize % 5
            } else {
                ((i + (h(5) * 10.0) as u32) as usize) % cover_palette.len()
            };
            let (base, rel, s0, sink) = cover_palette[pick];
            let s = s0 * (0.85 + 0.4 * h(6));
            let rot = h(7) * std::f32::consts::TAU;
            let si = scene.len();
            let oi = occluders.len();
            place(
                &mut scene,
                &mut occluders,
                true,
                base,
                rel,
                dx,
                dz,
                rot,
                s,
                sink,
                None,
            );
            // Free-standing cover WALLS are destructible too (barricades / rubble berms / containers
            // stay put). Guard on the asset having loaded and on the overall cap.
            let is_wall = rel == STRUCT_WALL || rel == STRUCT_WALL_HOLE1;
            if is_wall
                && scene.len() == si + 1
                && occluders.len() == oi + 1
                && dbuild.len() < STRUCT_MAX_DESTRUCTIBLES
            {
                let (mn, mx) = (scene[si].min, scene[si].max);
                dbuild.push(DestructibleBuild {
                    intact: si,
                    occ: oi,
                    min: mn,
                    max: mx,
                    hp: structure_hp((mx - mn).max_element()),
                    seed: 0xC0F0_0000 ^ si as u32,
                    class: Material::Brick,
                });
            }
        }

        // ── Frozen dead in the snow ──────────────────────────────────────────────────────────────
        // Corpse props strewn across the drift for grim atmosphere: THICKEST here at the city's edge
        // and back through the ruined streets (where the fighting was), a THIN scatter out on the open
        // steppe. Deterministic (hash01), grounded on the heightfield, varied yaw, a few half-buried
        // in the drift. Static geometry — pushed BEFORE the tank / FX tail so the FX slot layout is
        // untouched. Prone + low, so they are NOT registered as LOS occluders (you can't hide a Tiger
        // behind a body). Two snow-dusted greatcoat tints, cloned per placement.
        // The corpse: a baked PSX deadman (posed face-down in the drift) when the asset is present,
        // else a procedural box body. Fresh clones / worktrees don't have gitignored `assets/`, so the
        // box form keeps the scene grim without it. Dark, frozen greatcoat tones on the fallback so it
        // reads against the white snow (the crush's cold grade lifts the scene — a pale corpse vanishes).
        // ── The OPENING ARRAY, worldbuilder-authoritative (docs/ACT1_MAP.md §5): manifest
        // `Anchor*` markers override the authored SWARM constants — position from the marker,
        // vehicle from the kind string (AnchorT34 / AnchorSu100 / AnchorKv2 / AnchorIs2 /
        // AnchorIs3 / AnchorIsu152), InCity when the kind or name says "city". The enemy count
        // stays N_ENEMIES (the rig slots are fixed): anchors fill from #0, constants pad the
        // rest. `anchored` = hand-placed → near-zero muster jitter (precision was the intent).
        let mut swarm: Vec<(f32, f32, SpawnZone, EnemyKind, bool)> = SWARM
            .iter()
            .zip(SWARM_KINDS)
            .map(|(&(x, z, zone), kind)| (x, z, zone, kind, false))
            .collect();
        if let Some(manifest) = &manifest {
            let markers = manifest.effective_markers()?;
            let anchors: Vec<_> = markers
                .iter()
                .filter(|m| m.kind.starts_with("Anchor"))
                .collect();
            for (i, m) in anchors.iter().take(N_ENEMIES).enumerate() {
                let key = format!("{} {}", m.kind, m.name).to_lowercase();
                let kind = if key.contains("su100") || key.contains("su-100") {
                    EnemyKind::Su100
                } else if key.contains("kv2") || key.contains("kv-2") {
                    EnemyKind::Kv2
                } else if key.contains("isu152") || key.contains("isu-152") {
                    EnemyKind::Isu152
                } else if key.contains("is3") || key.contains("is-3") {
                    EnemyKind::Is3
                } else if key.contains("is2") || key.contains("is-2") {
                    EnemyKind::Is2
                } else {
                    EnemyKind::T34
                };
                let zone = if key.contains("city") {
                    SpawnZone::InCity
                } else {
                    SpawnZone::Outskirts
                };
                swarm[i] = (m.pos.x, m.pos.z, zone, kind, true);
            }
            if !anchors.is_empty() {
                println!(
                    "[kessel] act {}: {} enemy anchors from the manifest{}",
                    act.id,
                    anchors.len().min(N_ENEMIES),
                    if anchors.len() > N_ENEMIES {
                        " (extra anchors beyond the 6 rigs ignored)"
                    } else {
                        ""
                    }
                );
            }
        }

        const CORPSE_GLB: &str = "assets/flavor/Characters/deadman-psx/baked/deadman_prone.glb";
        let corpse_glb = crate::gltf_load::load_glb(CORPSE_GLB).ok();
        let corpse_grey = corpse_mesh([62, 58, 52]);
        let corpse_dark = corpse_mesh([44, 40, 36]);
        println!(
            "[kessel] corpses: {}",
            if corpse_glb.is_some() {
                "baked deadman GLB"
            } else {
                "procedural box fallback"
            }
        );
        // (x0, z0, width, depth, count): a dense carpet across the city-edge approach the player faces,
        // a medium scatter into the near ruined streets, and a sparse handful out in the open snow.
        let corpse_bands: [(f32, f32, f32, f32, u32); 3] = [
            (-84.0, 34.0, 104.0, 40.0, 13), // EDGE — dense, right across the forward field
            (-66.0, 72.0, 86.0, 42.0, 6),   // RUINS / STREETS — into the near city
            (-96.0, -48.0, 196.0, 74.0, 5), // OPEN STEPPE — thin
        ];
        let mut body_seed = 0u32;
        for (bx, bz, bw, bd, count) in corpse_bands {
            for _ in 0..count {
                body_seed = body_seed.wrapping_add(1);
                let h = |s: u32| hash01(body_seed.wrapping_mul(2654435761).wrapping_add(s));
                let x = bx + h(1) * bw;
                let z = bz + h(2) * bd;
                // Keep bodies off the tanks — the player spawn and every muster point.
                let clear = (x - PLAYER_SPAWN.0).hypot(z - PLAYER_SPAWN.1) > 5.0
                    && swarm
                        .iter()
                        .all(|(sx, sz, ..)| (x - sx).hypot(z - sz) > 5.0);
                if !clear {
                    continue;
                }
                let yaw = h(3) * std::f32::consts::TAU;
                let buried = h(4) < 0.22; // ~1 in 5 half-sunk under the drift
                let sink = if buried { 0.10 + h(5) * 0.06 } else { 0.02 };
                let s = 0.92 + h(6) * 0.20; // size variety
                let base = corpse_glb.as_ref().unwrap_or_else(|| {
                    if h(7) < 0.4 {
                        &corpse_dark
                    } else {
                        &corpse_grey
                    }
                });
                let y = hf.height_at(x, z) - sink;
                let xf = Mat4::from_translation(Vec3::new(x, y, z))
                    * Mat4::from_rotation_y(yaw)
                    * Mat4::from_scale(Vec3::splat(s));
                scene.push(base.clone().transformed(xf));
            }
        }

        // ── Destructibility: guaranteed demo wall + the batch of pre-allocated rubble heaps ────────
        // A guaranteed-present DESTRUCTIBLE demo wall: a procedural masonry wall at a known open spot
        // the player faces, so the headless demolish demo (KESSEL_DEMOLISH) and the playtest always
        // have a wall to knock down even on a fresh clone with NO `assets/` (the GLB walls above would
        // be absent). Registered exactly like any other destructible, first in the list.
        let demo_c = Vec3::new(-40.0, 0.0, 46.0);
        let demo_g = hf.height_at(demo_c.x, demo_c.z);
        let demo_min = Vec3::new(demo_c.x - 3.0, demo_g, demo_c.z - 0.4);
        let demo_max = Vec3::new(demo_c.x + 3.0, demo_g + 4.2, demo_c.z + 0.4);
        let demo_struct = if dbuild.len() < STRUCT_MAX_DESTRUCTIBLES {
            let idx = dbuild.len();
            let occ = occluders.len();
            occluders.push(stealth::Occluder::from_aabb(demo_min, demo_max));
            dbuild.push(DestructibleBuild {
                intact: scene.len(),
                occ,
                min: demo_min,
                max: demo_max,
                hp: structure_hp((demo_max - demo_min).max_element()),
                seed: 0x00DE_3001,
                class: Material::Brick,
            });
            scene.push(box_mesh(demo_min, demo_max, [128, 120, 108])); // pale ruined concrete wall
            Some(idx)
        } else {
            None
        };

        // Pre-allocate the paired COLLAPSED-RUBBLE heaps (hidden until felled). Pushed HERE — after all
        // static dressing but BEFORE the tiger/FX tail — so the final-16 FX layout is untouched. Each
        // heap sits at its wall's footprint and is revealed the instant that wall drops.
        let mut destructibles: Vec<Destructible> = Vec::with_capacity(dbuild.len());
        let mut rubble_slots: Vec<usize> = Vec::with_capacity(dbuild.len());
        let mut chunks = crate::rubble::ChunkWalls::default();
        for (di, d) in dbuild.iter().enumerate() {
            let rubble = scene.len();
            scene.push(crate::city::rubble_heap(d.min, d.max, d.seed));
            rubble_slots.push(rubble);
            // Thin axis-aligned walls get the CHUNK GRID (docs/DESTRUCTION.md §6) — pushed here,
            // still before the tank/FX tail, hidden until the first hole activates the set.
            if crate::rubble::chunkable(d.min, d.max) {
                chunks.register(di, d.intact, d.min, d.max, d.seed, &mut scene);
            }
            destructibles.push(Destructible {
                intact: d.intact,
                rubble,
                occ: d.occ,
                min: d.min,
                max: d.max,
                hp: d.hp,
                max_hp: d.hp,
                alive: true,
                seed: d.seed,
                class: d.class,
            });
        }
        println!(
            "[kessel] destructibles: {} standing structures ({} chunk-gridded, {} chunks)",
            destructibles.len(),
            destructibles
                .iter()
                .enumerate()
                .filter(|(i, _)| chunks.debug_counts(*i).0 > 0)
                .count(),
            chunks.chunk_count()
        );

        // player Tiger + one enemy per swarm member (each gets its own hull/turret/gun slots). The
        // kind picks the mesh set: a T-34 (turreted) or an SU-100 (casemate — its middle "turret" slot
        // is a hidden placeholder, so the [hull, turret, gun] triple stays uniform). Parallel to the
        // enemy-Tank spawn below, both keyed on `SWARM_KINDS` by index.
        // ── ARENA (act 0) dressing: a pine ring + a few extra cover walls scattered through the
        // duel bowl, baked as static scene meshes. Pushed HERE — before the tank/FX slots — so the
        // Slots indices computed below are untouched. Pines are visual (no occluder: thin trunks);
        // the walls DO occlude (they're what a duelist learns to fight around).
        // The pine RING is destructible now: transforms are collected here and registered as an
        // instanced TREE batch in the district section below (the scene/Slots layout is untouched
        // — trees draw on the instanced path, not as scene meshes). Scale spread widened down to
        // 0.55 so the ring carries both classes: snap-under-the-tracks firs AND shove-over pines.
        let mut arena_pines: Vec<Mat4> = Vec::new();
        if act.id == 0 {
            for k in 0..22u32 {
                let ang = hash01(act.seed ^ (k * 131)) * std::f32::consts::TAU;
                let rad = 120.0 + hash01(act.seed ^ (k * 733)) * 260.0;
                let (x, z) = (ang.sin() * rad, ang.cos() * rad);
                let sc = 0.55 + hash01(act.seed ^ (k * 271)) * 0.95;
                arena_pines.push(Mat4::from_scale_rotation_translation(
                    Vec3::splat(sc),
                    Quat::from_rotation_y(hash01(k * 17) * std::f32::consts::TAU),
                    Vec3::new(x, hf.height_at(x, z) - 0.1, z),
                ));
            }
            // Free-standing cover walls on the open half of the bowl (the ruins hold the north).
            for k in 0..6u32 {
                let ang = 2.4 + hash01(act.seed ^ (k * 97)) * 2.6; // southern arc
                let rad = 90.0 + hash01(act.seed ^ (k * 389)) * 140.0;
                let (x, z) = (ang.sin() * rad, ang.cos() * rad);
                let gy = hf.height_at(x, z) - 0.2;
                let (hw, hh) = (6.5, 2.4); // hull-down wall: long enough to hide a T-34
                let rot = hash01(k * 41) * std::f32::consts::PI;
                let (sn, cs) = rot.sin_cos();
                scene.push(box_mesh(
                    Vec3::new(x - hw * cs.abs() - 0.4, gy, z - hw * sn.abs() - 0.4),
                    Vec3::new(x + hw * cs.abs() + 0.4, gy + hh, z + hw * sn.abs() + 0.4),
                    [148, 140, 128],
                ));
                occluders.push(stealth::Occluder::from_aabb(
                    Vec3::new(x - hw, gy, z - hw),
                    Vec3::new(x + hw, gy + hh, z + hw),
                ));
            }
        }

        let tiger0 = scene.len();
        scene.extend(tank::build_tiger());
        // Schutzglas vision-block parts (see-through GLASS mode) — player Tiger only. Pushed
        // immediately after the three tiger parts; indices recorded explicitly so nothing drifts.
        let mut tiger_glass: [Option<usize>; 3] = [None, None, None];
        for (k, gm) in tank::load_tiger_glass().into_iter().enumerate() {
            if let Some(m) = gm {
                tiger_glass[k] = Some(scene.len());
                scene.push(m);
            }
        }
        // RTT periscope screens: the eyepiece meshes (atlas-UV'd) + their aperture metadata.
        let mut tiger_rtt: [Option<usize>; 3] = [None, None, None];
        for (k, gm) in tank::load_tiger_rtt().into_iter().enumerate() {
            if let Some(m) = gm {
                tiger_rtt[k] = Some(scene.len());
                scene.push(m);
            }
        }
        let mut rtt_screens: Vec<RttScreen> = Vec::new();
        if let Ok(txt) = std::fs::read_to_string("assets/tanks/tiger/baked/tiger_rtt_screens.json")
        {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                for e in v.as_array().cloned().unwrap_or_default() {
                    let fr = match e["frame"].as_str().unwrap_or("turret") {
                        "hull" => 0,
                        "gun" => 2,
                        _ => 1,
                    };
                    let g = |k: &str, i: usize| e[k][i].as_f64().unwrap_or(0.0) as f32;
                    rtt_screens.push(RttScreen {
                        frame: fr,
                        centroid: Vec3::new(g("centroid", 0), g("centroid", 1), g("centroid", 2)),
                        bearing: Vec3::new(g("bearing", 0), g("bearing", 1), g("bearing", 2)),
                        tile: e["tile"].as_u64().unwrap_or(0) as u32,
                    });
                }
            }
        }
        println!("[kessel] RTT periscopes: {} screens", rtt_screens.len());
        // The see-through PORT openings (tiger_ports.json) — light-shaft apertures for the
        // interior atmosphere pass. Same shape as the screens (frame + centroid + outward bearing).
        let mut port_aps: Vec<RttScreen> = Vec::new();
        if let Ok(txt) = std::fs::read_to_string("assets/tanks/tiger/baked/tiger_ports.json") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                for e in v.as_array().cloned().unwrap_or_default() {
                    let fr = match e["frame"].as_str().unwrap_or("turret") {
                        "hull" => 0,
                        "gun" => 2,
                        _ => 1,
                    };
                    let g = |k: &str, i: usize| e[k][i].as_f64().unwrap_or(0.0) as f32;
                    port_aps.push(RttScreen {
                        frame: fr,
                        centroid: Vec3::new(g("centroid", 0), g("centroid", 1), g("centroid", 2)),
                        bearing: Vec3::new(g("bearing", 0), g("bearing", 1), g("bearing", 2)),
                        tile: 0,
                    });
                }
            }
        }
        println!("[kessel] port apertures: {}", port_aps.len());

        // The hatch lever on the cupola pedestal (E = work it; placeholder box until the blend
        // rename lands and part_hatchlever.glb bakes).
        let hatchlever = scene.len();
        scene.push(
            crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_part_hatchlever.glb")
                .map(|m| brighten_albedo(m, 3.0)) // lift the dark-PSX bake so it isn't a black lump
                .unwrap_or_else(|_| {
                    box_mesh(
                        Vec3::new(-0.015, 0.0, -0.015),
                        Vec3::new(0.015, 0.22, 0.015),
                        [110, 106, 96],
                    )
                }),
        );

        // The breech lever + cover: animated pivot-origin parts in the GUN frame.
        let gunlever = scene.len();
        scene.push(
            crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_part_gunlever.glb")
                .unwrap_or_else(|_| {
                    box_mesh(
                        Vec3::new(-0.02, 0.0, -0.02),
                        Vec3::new(0.02, 0.30, 0.02),
                        [96, 98, 92],
                    )
                }),
        );
        let breechcover = scene.len();
        scene.push(
            crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_part_breechcover.glb")
                .unwrap_or_else(|_| {
                    box_mesh(
                        Vec3::new(-0.10, -0.15, -0.02),
                        Vec3::new(0.10, 0.15, 0.02),
                        [88, 90, 86],
                    )
                }),
        );
        let mut t34: Vec<[usize; 3]> = Vec::with_capacity(N_ENEMIES);
        let mut su100: Vec<[usize; 3]> = Vec::with_capacity(N_ENEMIES);
        let mut kv2: Vec<[usize; 3]> = Vec::with_capacity(N_ENEMIES);
        let mut isu152: Vec<[usize; 3]> = Vec::with_capacity(N_ENEMIES);
        let mut is2: Vec<[usize; 3]> = Vec::with_capacity(N_ENEMIES);
        let mut is3: Vec<[usize; 3]> = Vec::with_capacity(N_ENEMIES);
        let prototypes = [
            tank::build_t34(),
            tank::build_su100(),
            tank::build_kv2(),
            tank::build_isu152(),
            tank::build_is2(),
            tank::build_is3(),
        ];
        for _ in 0..N_ENEMIES {
            for (prototype, slots) in prototypes.iter().zip([
                &mut t34,
                &mut su100,
                &mut kv2,
                &mut isu152,
                &mut is2,
                &mut is3,
            ]) {
                let base = scene.len();
                scene.extend(prototype.iter().cloned());
                slots.push([base, base + 1, base + 2]);
            }
        }

        // ACT 3 PLATOON: the vision promises two other Tigers at Halbe. They spawn echeloned behind
        // the player's scarred machine, then follow and engage through the real tracer/kill path.
        let mut ally_slots: Vec<[usize; 3]> = Vec::new();
        let mut ally_state: Vec<AllyTiger> = Vec::new();
        if act.id == 3 {
            let route = (act.goal - act.spawn).normalize_or_zero();
            let right = Vec2::new(route.y, -route.x);
            let heading = route.x.atan2(route.y);
            for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
                let p = act.spawn - route * (18.0 + i as f32 * 7.0) + right * side * 14.0;
                let tank = Tank::new(p.x, p.y, heading, &hf, tank::TIGER_PARTS);
                let base = scene.len();
                scene.extend(tank::build_tiger());
                ally_slots.push([base, base + 1, base + 2]);
                ally_state.push(AllyTiger {
                    tank,
                    fire_cd: 4.5 + i as f32 * 2.2,
                    formation_side: side,
                });
            }
        }

        // Streamed contacts reuse six live rigs, but their dead should not evaporate when the next
        // wave arrives. A separate graveyard pool freezes the exact final transforms of up to 24
        // campaign wrecks. Prototypes are cloned so optional asset loading happens only once.
        let mut hulk_t34: Vec<[usize; 3]> = Vec::new();
        let mut hulk_su100: Vec<[usize; 3]> = Vec::new();
        let mut hulk_kv2: Vec<[usize; 3]> = Vec::new();
        let mut hulk_isu152: Vec<[usize; 3]> = Vec::new();
        let mut hulk_is2: Vec<[usize; 3]> = Vec::new();
        let mut hulk_is3: Vec<[usize; 3]> = Vec::new();
        if act.id >= 1 {
            for _ in 0..MAX_CAMPAIGN_HULKS {
                for (prototype, slots) in prototypes.iter().zip([
                    &mut hulk_t34,
                    &mut hulk_su100,
                    &mut hulk_kv2,
                    &mut hulk_isu152,
                    &mut hulk_is2,
                    &mut hulk_is3,
                ]) {
                    let base = scene.len();
                    scene.extend(prototype.iter().cloned());
                    slots.push([base, base + 1, base + 2]);
                }
            }
        }

        // ── ABANDONED TIGERS (the Last Tiger rule): 3 spare hulls scattered along the act's
        // spawn→goal corridor with hashed lateral offsets — bail out of a burning tank, cross the
        // map on foot, climb into one, fight on. None on the training arena (act 0).
        let mut abandoned_slots: Vec<[usize; 3]> = Vec::new();
        let mut abandoned_state: Vec<(Vec3, f32, bool)> = Vec::new();
        if act.id >= 1 {
            let sp = act.spawn;
            let gl = act.goal;
            for (k, t) in [0.30f32, 0.55, 0.80].iter().enumerate() {
                let n = act.seed ^ ((k as u32 + 1) * 0x00AB_1E5);
                let along = sp + (gl - sp) * *t;
                let dirn = (gl - sp).normalize_or_zero();
                let perp = glam::Vec2::new(-dirn.y, dirn.x);
                let lat = (hash01(n) - 0.5) * 700.0; // ±350 m off the corridor
                let p2 = along + perp * lat;
                let heading = hash01(n ^ 0x77) * std::f32::consts::TAU;
                let pos = Vec3::new(p2.x, hf.height_at(p2.x, p2.y), p2.y);
                let base = scene.len();
                scene.extend(tank::build_tiger());
                abandoned_slots.push([base, base + 1, base + 2]);
                abandoned_state.push((pos, heading, false));
            }
        }

        // ── DEV GARAGE static props (countryside gunnery yard) ──────────────────────────────────
        // Inserted BEFORE the FX tail so the `mesh_count()-13` smoke-box offset is preserved. All
        // static (no per-frame slots): a concrete pad under the Tiger, a few STATIC Tiger targets
        // downrange (visual "including tigers"), and — if KESSEL_GARAGE_MODEL is set — an arbitrary
        // GLB on a plinth beside the Tiger to eyeball new assets in-engine at the real crush res.
        if act.garage {
            // Concrete staging pad under the parked Tiger.
            let (gx, gz) = (0.0f32, -180.0);
            let gy = hf.height_at(gx, gz);
            scene.push(box_mesh(
                Vec3::new(gx - 6.0, gy - 0.15, gz - 5.0),
                Vec3::new(gx + 6.0, gy + 0.05, gz + 5.0),
                [90, 92, 96],
            ));
            // Static Tiger targets downrange (silhouettes to range on with the TZF stadia).
            for &(tx, tz) in &[(-70.0f32, 120.0f32), (60.0, 210.0), (-20.0, 360.0)] {
                let ty = hf.height_at(tx, tz);
                let face = (gx - tx).atan2(gz - tz);
                let xf =
                    Mat4::from_translation(Vec3::new(tx, ty, tz)) * Mat4::from_rotation_y(face);
                for part in tank::build_tiger() {
                    scene.push(part.transformed(xf));
                }
            }
            // Model-drop plinth + the dropped GLB (env-driven).
            let (mx, mz) = (10.0f32, -180.0);
            let my = hf.height_at(mx, mz);
            scene.push(box_mesh(
                Vec3::new(mx - 1.5, my - 0.1, mz - 1.5),
                Vec3::new(mx + 1.5, my + 0.4, mz + 1.5),
                [70, 72, 78],
            ));
            if let Ok(path) = std::env::var("KESSEL_GARAGE_MODEL") {
                match crate::gltf_load::load_glb(&path) {
                    Ok(m) => {
                        // AUTO-FIT: scale the model so its largest dimension is ~4 m (tank-ish
                        // display size), so ANY asset — a bolt, a soldier, a whole building — shows
                        // at a consistent, viewable size on the plinth. KESSEL_GARAGE_SCALE overrides
                        // the 4 m target for finer inspection. Base sits on the plinth top, centred.
                        let span = (m.max - m.min).max_element().max(1e-3);
                        let target = std::env::var("KESSEL_GARAGE_SCALE")
                            .ok()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(4.0f32);
                        let s = target / span;
                        let c = (m.min + m.max) * 0.5;
                        let xf = Mat4::from_translation(Vec3::new(mx, my + 0.4, mz))
                            * Mat4::from_scale(Vec3::splat(s))
                            * Mat4::from_translation(Vec3::new(-c.x, -m.min.y, -c.z));
                        scene.push(m.transformed(xf));
                        println!("[garage] dropped model {path} (span {span:.1} m → ×{s:.2} to {target:.1} m)");
                    }
                    Err(e) => println!("[garage] KESSEL_GARAGE_MODEL {path} failed: {e}"),
                }
            }
        }

        // Big emissive fireball + spark/debris FX pools for the kill payoff (kills, turret-pops,
        // cook-offs). Deliberately pushed BEFORE the muzzle/tracer/impact/smoke/e_fx block so those
        // stay the FINAL 16 meshes — the headless fire demo locates the grey smoke box as
        // `mesh_count()-13`, so the tail layout must not shift. Hot orange bodies (the emissive
        // shader path forces fire-orange anyway, but the 18% source-hue mix keeps them warm).
        let mut blast = [0usize; N_BLAST];
        for b in &mut blast {
            *b = scene.len();
            scene.push(crate::prims::faceted_sphere_mesh([255, 120, 40]));
        }
        let mut debris = [0usize; N_DEBRIS];
        for d in &mut debris {
            *d = scene.len();
            scene.push(box_mesh(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                [255, 160, 60],
            ));
        }
        // One tall flame plume per enemy — parked on its wreck once destroyed, flickering hot so the
        // burning wreck reads as fire (a rising flame column) even at sniper range, not just a faint
        // glow on the hull. Also before the muzzle group to keep the final-16 tail intact.
        let mut flame: Vec<usize> = Vec::with_capacity(N_ENEMIES);
        for _ in 0..N_ENEMIES {
            flame.push(scene.len());
            scene.push(box_mesh(
                Vec3::new(-0.9, -0.2, -0.9),
                Vec3::new(0.9, 3.2, 0.9),
                [255, 110, 25],
            ));
        }

        // Interior fire FX — the PLAYER's own tank burning, seen from INSIDE (fuel/engine hit → fire
        // that spreads). Reuses the emissive box pattern; positioned + scaled per frame in
        // `drive_interior_fire` as the fire builds. Pushed BEFORE the muzzle group so the final-16 FX
        // tail (which the headless fire demo locates by offset) stays exactly where it was.
        let mut interior_fire: Vec<usize> = Vec::with_capacity(N_INT_FIRE);
        for _ in 0..N_INT_FIRE {
            interior_fire.push(scene.len());
            scene.push(box_mesh(
                Vec3::new(-0.5, -0.4, -0.5),
                Vec3::new(0.5, 1.2, 0.5),
                [255, 105, 30],
            ));
        }

        // Death instant-replay (kill-cam) FX: the incoming shell streak, the shooter muzzle flash,
        // the penetration entry flash, and a cone of spall/shrapnel fragments. Only ever driven by
        // `render_replay` (dead in normal play). Pushed BEFORE the muzzle group with the other pooled
        // FX so the final-16 legacy FX tail (which the headless fire demo locates as
        // `mesh_count()-13`) stays exactly where it was.
        let r_shell = scene.len();
        scene.push(box_mesh(
            Vec3::new(-0.12, -0.12, -1.0),
            Vec3::new(0.12, 0.12, 1.0),
            [255, 235, 210],
        )); // white-hot streak
        let r_muzzle = scene.len();
        scene.push(box_mesh(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            [255, 150, 45],
        ));
        let r_flash = scene.len();
        scene.push(box_mesh(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            [255, 175, 95],
        ));
        let mut r_shards = [0usize; N_SHARD];
        for s in &mut r_shards {
            *s = scene.len();
            scene.push(box_mesh(
                Vec3::new(-0.1, -0.1, -0.5),
                Vec3::new(0.1, 0.1, 0.5),
                [255, 150, 70],
            ));
        }
        // Kill-cam X-RAY interior proxies (crew silhouettes + module blocks — War-Thunder-style
        // anatomy, PS1-schematic). Own-colour emissives: hit ones flash hot, the rest sit faint.
        let r_crew: [usize; 5] = std::array::from_fn(|_| {
            let s = scene.len();
            scene.push(box_mesh(
                Vec3::new(-0.24, 0.0, -0.20),
                Vec3::new(0.24, 0.85, 0.20),
                [232, 202, 168],
            ));
            s
        });
        let r_mod: [usize; 12] = std::array::from_fn(|_| {
            let s = scene.len();
            scene.push(box_mesh(
                Vec3::splat(-0.26),
                Vec3::splat(0.26),
                [110, 150, 205],
            ));
            s
        });

        // Warm interior bulb + near-miss snow-puff pool. Pushed HERE — BEFORE the muzzle/tracer/impact/
        // smoke/e_fx block — so those stay the FINAL 16 meshes and the headless fire demo's
        // `mesh_count()-13` smoke-box offset is unchanged. Both take the own-colour (ungated) emissive
        // path: the bulb glows warm amber, the snow puffs glow white-grey (marked below via
        // `mark_own_emissive`).
        let bulb = scene.len();
        scene.push(box_mesh(
            Vec3::splat(-0.065),
            Vec3::splat(0.065),
            [255, 205, 140],
        )); // small warm tungsten bulb
            // Cupola HATCH lid: the real baked assembly (hinge post + disc + handle) exported from the
            // Blender rig with the mesh origin ON the hinge axis (`cupola hatch pivot`), so rotating
            // about engine-Y (Blender-Z, the vertical) swings it open horizontally around its side
            // hinge. Falls back to the old procedural lid+stub box if the bake is missing. Own-colour
            // emissive so the hover highlight can pulse it.
        let hatch = scene.len();
        match crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hatch.glb") {
            Ok(m) => {
                println!(
                    "[kessel] cupola hatch: baked GLB ({} verts)",
                    m.vertices.len()
                );
                // Same panzer-grey repaint as the hull/turret/gun bakes — the raw albedo is
                // authored near-black and vanishes against the cupola well in the crushed frame.
                scene.push(crate::tank::repaint_german_grey(m));
            }
            Err(e) => {
                println!("[kessel] cupola hatch: bake missing ({e}) — procedural stand-in");
                let mut lid = box_mesh(
                    Vec3::new(0.03, -0.06, -0.40),
                    Vec3::new(0.78, 0.06, 0.40),
                    [128, 130, 122],
                );
                let stub = box_mesh(
                    Vec3::new(-0.07, -0.14, -0.11),
                    Vec3::new(0.09, 0.16, 0.11),
                    [86, 88, 80],
                );
                lid.vertices.extend(stub.vertices.iter().cloned());
                let base = (lid.indices.iter().copied().max().unwrap_or(0) + 1)
                    .max((lid.vertices.len() - stub.vertices.len()) as u32);
                for i in &stub.indices {
                    lid.indices.push(i + base);
                }
                scene.push(lid);
            }
        }
        // ── Animated interior parts (Phase 5): the real baked meshes the engine drives — TPZ
        // needle, both handwheels, the chambered round, and the rack (the one textured round
        // instanced at every Shells-collection slot; slots deplete as rounds are spent). Each GLB's
        // origin is its pivot (dial centre / wheel hub / the authored seat pose). Fallback = tiny
        // box, so a missing bake never breaks the build.
        let part = |p: &str, c: [u8; 3]| {
            crate::gltf_load::load_glb(p)
                .unwrap_or_else(|_| box_mesh(Vec3::splat(-0.01), Vec3::splat(0.01), c))
        };
        let tpz_needle = scene.len();
        scene.push(part(
            "assets/tanks/tiger/baked/tiger_part_tpz.glb",
            [200, 40, 30],
        ));
        let wheel1 = scene.len();
        scene.push(part(
            "assets/tanks/tiger/baked/tiger_part_wheel1.glb",
            [60, 60, 60],
        ));
        let wheel2 = scene.len();
        scene.push(part(
            "assets/tanks/tiger/baked/tiger_part_wheel2.glb",
            [60, 60, 60],
        ));
        let round_mesh = part(
            "assets/tanks/tiger/baked/tiger_part_round.glb",
            [214, 168, 78],
        );
        // The PBR 88 rounds, tinted per NATURE (grey PzGr / yellow SprGr); baked-round fallback.
        let ap_round = load_88_round(false).unwrap_or_else(|| round_mesh.clone());
        let he_round = load_88_round(true).unwrap_or_else(|| round_mesh.clone());
        let chamber_round = scene.len();
        scene.push(ap_round.clone());
        let chamber_round_he = scene.len();
        scene.push(he_round.clone());
        // Stowage plan: ~2/3 PzGr, 1/3 SprGr, hashed across the rack — grab a slot, get its nature.
        let slot_nature_he: Vec<bool> = (0..crate::interior_rig::SHELL_SLOTS.len())
            .map(|i| hash01(0x88AA_0000 ^ (i as u32).wrapping_mul(2654435761)) < 0.34)
            .collect();
        let shell0 = scene.len();
        for he in &slot_nature_he {
            scene.push(if *he {
                he_round.clone()
            } else {
                ap_round.clone()
            });
        }
        let mut snow = [0usize; N_SNOW];
        for s in &mut snow {
            *s = scene.len();
            scene.push(box_mesh(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                [226, 230, 238],
            )); // white-grey snow puff
        }

        // ── Crew-of-one interior workload FX ─────────────────────────────────────────────────────
        // Breech block + reload rounds + rigid-body casings + extinguisher foam + annunciator lamps.
        // Pushed HERE — BEFORE the mg_flash/muzzle/tracer/impact/smoke/e_fx tail — so the final-16
        // legacy FX layout (`mesh_count()-13` = smoke) is untouched. The rounds / casings / foam / lamps
        // take the own-colour emissive path (marked below); the breech is a lit steel block.
        let breech = scene.len();
        scene.push(box_mesh(
            Vec3::new(-0.20, -0.16, -0.22),
            Vec3::new(0.20, 0.16, 0.22),
            [64, 66, 60],
        )); // dark steel breech
        let chambered = scene.len();
        scene.push(box_mesh(
            Vec3::new(-0.09, -0.09, -0.30),
            Vec3::new(0.09, 0.09, 0.30),
            [214, 168, 78],
        )); // brass round
        let ram_round = scene.len();
        scene.push(box_mesh(
            Vec3::new(-0.09, -0.09, -0.30),
            Vec3::new(0.09, 0.09, 0.30),
            [214, 168, 78],
        ));
        let mut ready_rack = [0usize; 3];
        for s in &mut ready_rack {
            *s = scene.len();
            scene.push(box_mesh(
                Vec3::new(-0.07, -0.07, -0.26),
                Vec3::new(0.07, 0.07, 0.26),
                [198, 156, 74],
            ));
        }
        let loose_mesh = load_88_round(false).unwrap_or_else(|| {
            box_mesh(
                Vec3::new(-0.07, -0.07, -0.28),
                Vec3::new(0.07, 0.07, 0.28),
                [206, 162, 76],
            )
        });
        let mut loose_shell = [0usize; N_LOOSE];
        for s in &mut loose_shell {
            *s = scene.len();
            scene.push(loose_mesh.clone()); // a dropped round IS a round
        }
        let loose_he_mesh = load_88_round(true).unwrap_or_else(|| loose_mesh.clone());
        let mut loose_shell_he = [0usize; N_LOOSE];
        for s in &mut loose_shell_he {
            *s = scene.len();
            scene.push(loose_he_mesh.clone());
        }
        let case_mesh = load_88_case().unwrap_or_else(|| {
            box_mesh(
                Vec3::new(-0.055, -0.055, -0.20),
                Vec3::new(0.055, 0.055, 0.20),
                [222, 176, 86],
            )
        });
        let mut casing = [0usize; N_CASING];
        for s in &mut casing {
            *s = scene.len();
            scene.push(case_mesh.clone()); // the real spent case (solid mouth walls baked in)
        }
        let mut foam_slots = [0usize; N_FOAM];
        for s in &mut foam_slots {
            *s = scene.len();
            scene.push(box_mesh(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                [238, 242, 240],
            )); // foam voxel
        }
        let mut ext_spray = [0usize; N_EXT];
        for s in &mut ext_spray {
            *s = scene.len();
            scene.push(box_mesh(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                [236, 240, 246],
            )); // whitish CO2/foam puff
        }
        // Annunciator lamps — distinct authored colours (red / amber / blue / green) modulated by
        // emissive intensity per state. Own-colour path so they glow their own hue, not FX orange.
        let annun_cols: [[u8; 3]; N_LAMP] =
            [[255, 46, 30], [255, 172, 44], [70, 128, 255], [70, 255, 96]];
        let mut annun = [0usize; N_LAMP];
        for (i, col) in annun_cols.iter().enumerate() {
            annun[i] = scene.len();
            scene.push(box_mesh(Vec3::splat(-0.028), Vec3::splat(0.028), *col));
        }
        // The on-breech reload lamp (green/orange/red variants share one mount).
        let mut breech_lamp = [0usize; 3];
        for (k, col) in [[60u8, 235, 90], [245, 160, 40], [235, 45, 30]]
            .iter()
            .enumerate()
        {
            breech_lamp[k] = scene.len();
            scene.push(box_mesh(Vec3::splat(-0.022), Vec3::splat(0.022), *col));
        }

        // LAN crew bodies live before the legacy FX tail. Slight field-grey variation makes bodies
        // separable in the cramped compartment without turning them into gamey team-colour dolls.
        let lan_cols = [
            [82, 88, 76],
            [72, 80, 70],
            [88, 86, 72],
            [76, 82, 74],
            [68, 78, 70],
        ];
        let lan_crew = std::array::from_fn(|i| {
            let slot = scene.len();
            scene.push(lan_crew_mesh(lan_cols[i]));
            slot
        });

        // ── Coax + hull MG34 FX pools ──────────────────────────────────────────────────────────
        // Pushed HERE — BEFORE the muzzle/tracer/impact/smoke/e_fx block — so those stay the FINAL
        // 16 meshes and the headless fire demo's `mesh_count()-13` smoke-box offset is unchanged.
        // Flash + tracers take the FX (gunfire-orange) path; the impact spurts take the own-colour
        // warm-white path (marked below) so a snow/dirt kick reads as powder while an armour hit
        // still blows white-hot through bloom.
        let mg_flash = scene.len();
        scene.push(box_mesh(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            [255, 190, 90],
        )); // hot muzzle flash
        let mut mg_tracer = [0usize; N_MG_TRACER];
        for s in &mut mg_tracer {
            *s = scene.len();
            scene.push(box_mesh(
                Vec3::new(-0.12, -0.12, -1.0),
                Vec3::new(0.12, 0.12, 1.0),
                [255, 230, 150],
            )); // thin yellow-white streak
        }
        let mut mg_impact = [0usize; N_MG_IMPACT];
        for s in &mut mg_impact {
            *s = scene.len();
            scene.push(box_mesh(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                [250, 240, 220],
            )); // warm-white spurt/spark
        }

        // ── ABSITZEN on-foot meshes: supply pickups + the held Luger. Pushed BEFORE the FX block
        // so the FX block stays the LAST 16 slots and `run_fire_demo`'s `mesh_count()-13` invariant
        // holds untouched. Meshes are untransformed clones — each instance is positioned per frame
        // via `set_model` so it can be grounded and hidden on collect (NOT baked with `.transformed`).
        let pickup_box =
            |c: [u8; 3]| box_mesh(Vec3::new(-0.4, 0.0, -0.4), Vec3::new(0.4, 0.7, 0.4), c);
        let ammo_mesh = crate::gltf_load::load_glb(&format!("{MP}{DEBRIS_AMMO}"))
            .unwrap_or_else(|_| pickup_box([96, 82, 44]));
        let fuel_mesh = crate::gltf_load::load_glb(&format!("{MP}{DEBRIS_BARREL1}"))
            .unwrap_or_else(|_| pickup_box([60, 84, 62]));
        let med_mesh = crate::gltf_load::load_glb(&format!("{MP}{DEBRIS_SUPPLY}"))
            .unwrap_or_else(|_| pickup_box([120, 44, 44]));
        // Cigarettes — the REAL pack model (cigs_packet_1, 16×10×4 cm at ground contact), scaled
        // up at placement into readable loot. (MEMORY: pack albedos are near-black, so the 1.5
        // emissive glint at seed is mandatory or the treasure reads as missing in the ruin's gloom.)
        let cig_mesh =
            crate::gltf_load::load_glb(&format!("{MP}Items & Weapons/cigs_packet_1.glb"))
                .unwrap_or_else(|_| {
                    box_mesh(
                        Vec3::new(-0.12, 0.0, -0.08),
                        Vec3::new(0.12, 0.16, 0.08),
                        [180, 170, 150],
                    )
                });
        let pickup0 = scene.len();
        for (kind, _, _) in PICKUP_SPOTS {
            let m = match kind {
                PickupKind::Ammo => &ammo_mesh,
                PickupKind::Fuel => &fuel_mesh,
                PickupKind::Medical => &med_mesh,
                PickupKind::Cigarettes => &cig_mesh,
            };
            scene.push(m.clone());
        }

        // First-person Luger pistol + its muzzle-flash box.
        let luger = scene.len();
        scene.push(
            crate::gltf_load::load_glb("assets/flavor/luger/luger.glb").unwrap_or_else(|_| {
                eprintln!("[absitzen] luger.glb missing — using box placeholder");
                box_mesh(
                    Vec3::new(-0.04, -0.10, -0.02),
                    Vec3::new(0.04, 0.02, 0.18),
                    [40, 38, 36],
                )
            }),
        );
        let hammer = scene.len();
        scene.push(
            crate::gltf_load::load_glb(&format!("{MP2}Weapons & Tools/hammer_mx_1.glb"))
                .unwrap_or_else(|_| {
                    box_mesh(
                        Vec3::new(-0.03, -0.02, -0.16),
                        Vec3::new(0.03, 0.10, 0.16),
                        [110, 96, 80],
                    )
                }),
        );
        let toolbox = scene.len();
        scene.push(
            crate::gltf_load::load_glb(&format!("{MP2}Props/toolbox_mx_1.glb")).unwrap_or_else(
                |_| {
                    box_mesh(
                        Vec3::new(-0.22, 0.0, -0.12),
                        Vec3::new(0.22, 0.16, 0.12),
                        [140, 60, 40],
                    )
                },
            ),
        );
        // Separate shoes let the broken run visibly sag behind the tank and then articulate
        // back across the road wheels as the player pulls.  These are intentionally plain steel:
        // the ritual needs a readable moving silhouette more than another authored prop.
        let repair_shoe_mesh = box_mesh(
            Vec3::new(-0.30, -0.045, -0.065),
            Vec3::new(0.30, 0.045, 0.065),
            [56, 59, 57],
        );
        let track_repair_shoes = std::array::from_fn(|_| {
            let slot = scene.len();
            scene.push(repair_shoe_mesh.clone());
            slot
        });
        let track_replacement_link = scene.len();
        scene.push(box_mesh(
            Vec3::new(-0.32, -0.055, -0.075),
            Vec3::new(0.32, 0.055, 0.075),
            [94, 89, 72],
        ));
        let extinguisher = scene.len();
        scene.push(
            crate::gltf_load::load_glb("assets/flavor/psx-fire-extinguisher/extinguisher.glb")
                .map(|m| brighten_albedo(m, 2.4)) // authored near-black red (dark-PSX-albedo) —
                // lift it to signal red or the cold grade
                // crushes the bottle to a silhouette
                .unwrap_or_else(|_| {
                    box_mesh(
                        Vec3::new(-0.09, 0.0, -0.09),
                        Vec3::new(0.09, 0.55, 0.09),
                        [150, 40, 34],
                    )
                }),
        );
        let luger_flash = scene.len();
        scene.push(box_mesh(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            [255, 236, 188],
        ));

        // ── ABSITZEN infantry: N_SOLDIERS bodies + rifles, then INF_FX muzzle-flash boxes. ALL of
        // this is pushed BEFORE the FX block below so the FX block stays the LAST 16 slots and
        // `run_fire_demo`'s `mesh_count()-13` invariant is untouched. Placeholder greatcoat-grey
        // capsule (body) + slung-rifle box; real PSX rigs drop into these same slots later.
        let soldier0 = scene.len();
        for _ in 0..N_SOLDIERS {
            scene.push(box_mesh(
                Vec3::new(-0.3, 0.0, -0.3),
                Vec3::new(0.3, 1.8, 0.3),
                [78, 82, 70],
            )); // body
            scene.push(box_mesh(
                Vec3::new(-0.03, -0.03, -0.30),
                Vec3::new(0.03, 0.03, 0.30),
                [30, 28, 26],
            )); // rifle
        }
        let inf_flash: [usize; INF_FX] = std::array::from_fn(|_| {
            let s = scene.len();
            scene.push(box_mesh(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                [255, 236, 188],
            ));
            s
        });

        // FX meshes (positioned/scaled per frame). Round FX are pushed HOTTER — saturated
        // orange/red bodies feeding the emissive shader's source-hue mix (the intensities are bumped
        // in update_fx) so muzzle flash, tracer and impact read searing hot against the cold scene.
        let muzzle = scene.len();
        scene.push(box_mesh(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            [255, 150, 45],
        ));
        let tracer = scene.len();
        scene.push(box_mesh(
            Vec3::new(-0.12, -0.12, -1.0),
            Vec3::new(0.12, 0.12, 1.0),
            [255, 120, 30],
        ));
        let impact = scene.len();
        scene.push(box_mesh(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            [255, 150, 50],
        ));
        let smoke = scene.len();
        scene.push(box_mesh(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            [150, 148, 140],
        )); // dirty grey plume
            // enemy return-fire FX pool (redder tracer so incoming reads distinct from outgoing) — a
            // pool of E_FX_POOL tracer+impact pairs so several T-34s can have shots in the air at once.
        let mut e_tracer = [0usize; E_FX_POOL];
        let mut e_impact = [0usize; E_FX_POOL];
        for k in 0..E_FX_POOL {
            e_tracer[k] = scene.len();
            scene.push(box_mesh(
                Vec3::new(-0.12, -0.12, -1.0),
                Vec3::new(0.12, 0.12, 1.0),
                [255, 138, 96],
            ));
            e_impact[k] = scene.len();
            scene.push(box_mesh(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                [255, 190, 150],
            ));
        }

        let slots = Slots {
            tiger: [tiger0, tiger0 + 1, tiger0 + 2],
            tiger_glass,
            tiger_rtt,
            hatchlever,
            gunlever,
            breechcover,
            t34,
            su100,
            kv2,
            isu152,
            is2,
            is3,
            allies: ally_slots,
            hulk_t34,
            hulk_su100,
            hulk_kv2,
            hulk_isu152,
            hulk_is2,
            hulk_is3,
            abandoned: abandoned_slots,
            blast,
            debris,
            flame,
            interior_fire,
            bulb,
            snow,
            hatch,
            tpz_needle,
            wheel1,
            wheel2,
            chamber_round,
            chamber_round_he,
            shell0,
            breech,
            chambered,
            ram_round,
            ready_rack,
            loose_shell,
            loose_shell_he,
            casing,
            ext_spray,
            foam: foam_slots,
            annun,
            breech_lamp,
            lan_crew,
            mg_flash,
            mg_tracer,
            mg_impact,
            pickup0,
            n_pickups: N_PICKUPS,
            luger,
            luger_flash,
            hammer,
            toolbox,
            track_repair_shoes,
            track_replacement_link,
            extinguisher,
            soldier0,
            inf_flash,
            muzzle,
            tracer,
            impact,
            smoke,
            e_tracer,
            e_impact,
            r_shell,
            r_muzzle,
            r_flash,
            r_shards,
            r_crew,
            r_mod,
        };

        let mut renderer = Renderer::new(device, queue, &scene);
        // M-D ray pass: hand the terrain grid to the GPU (origin-centred, so origin = -half·spacing).
        renderer.upload_heightfield(
            device,
            queue,
            hf.res,
            hf.spacing,
            (hf.res as f32 - 1.0) * 0.5 * hf.spacing,
            &hf.heights,
        );

        // ── The ACT-SCALE district (worldgen.rs): thousands of instanced module placements — the
        // shelled city grid with walkable interiors, the western village/pine fringe, the bunker —
        // surrounding the hand-tuned combat block (which keeps its occluders/destructibles/windows
        // and is held clear by the exclusion radius). Instanced + bucketed: a handful of culled
        // draw calls, not thousands of meshes.
        // Every campaign act comes from the same strict, versioned manifest the editor opened.
        let mut manifest_placements = 0usize;
        // Destructible-trees subsystem (docs/DESTRUCTION.md §5): every tree-asset instance —
        // manifest placements, worldgen pines, the arena ring — registers here instead of raw
        // add_instanced, so each trunk can hinge-fall. Same draw path, plus per-tree state.
        let mut trees = crate::trees::Trees::new(device);
        // Act 1's manifest replaces its procedural DISTRICT; the parametric city core above
        // (occluders, destructibles, window glow) is built independently and survives untouched.
        if let Some(manifest) = &manifest {
            let mpath = manifest.path();
            let batches = crate::editor::manifest_batches_from(manifest)?;
            let occ_before = occluders.len();
            for (apath, mats) in &batches {
                let lower = apath.to_lowercase();
                let is_tree = lower.contains("pine")
                    || lower.contains("tree")
                    || lower.contains("birch")
                    || lower.contains("spruce")
                    || lower.contains("fir_");
                let mesh = crate::worldgen::load_module(apath).ok_or_else(|| {
                    anyhow::anyhow!("manifest asset {apath:?} exists but failed GLB validation")
                })?;
                // MANIFEST OCCLUDERS (2026-07-07): solid structures from the
                // manifest now block stealth LOS — the same tall+footprint rule
                // the city pieces use. Previously manifest worlds (acts 2/3) had
                // NO cover at all, which quietly gutted the sniper thesis there.
                // Trees excluded (concealment is not cover; a trunk won't hide a
                // hull), capped so a dense manifest can't drown the LOS test.
                if !is_tree {
                    let (mn, mx) = (mesh.min, mesh.max);
                    let solid = mx.y - mn.y > 2.0 && (mx.x - mn.x).max(mx.z - mn.z) > 1.5;
                    if solid {
                        for m in mats {
                            if occluders.len() - occ_before >= 600 {
                                break;
                            }
                            // world AABB from the 8 transformed corners
                            let mut wmn = Vec3::splat(f32::INFINITY);
                            let mut wmx = Vec3::splat(f32::NEG_INFINITY);
                            for k in 0..8u32 {
                                let c = Vec3::new(
                                    if k & 1 == 0 { mn.x } else { mx.x },
                                    if k & 2 == 0 { mn.y } else { mx.y },
                                    if k & 4 == 0 { mn.z } else { mx.z },
                                );
                                let w = m.transform_point3(c);
                                wmn = wmn.min(w);
                                wmx = wmx.max(w);
                            }
                            occluders.push(stealth::Occluder::from_aabb(wmn, wmx));
                        }
                    }
                }
                if is_tree {
                    trees.register_batch(&mut renderer, device, queue, &mesh, mats.clone());
                } else {
                    renderer.add_instanced(device, queue, &mesh, mats);
                }
                manifest_placements += mats.len();
            }
            println!(
                "[kessel] act {} world from {}: {manifest_placements} placements, {} LOS occluders",
                act.id,
                mpath.display(),
                occluders.len() - occ_before
            );
        }
        if manifest.is_some() {
            anyhow::ensure!(
                manifest_placements > 0,
                "act {} versioned manifest contains no loadable placements",
                act.id
            );
        }
        // Act 0 has only its explicit engine-authored arena dressing. Campaign acts have already
        // loaded their mandatory manifest above. There is intentionally no procedural substitute.
        let gw = crate::worldgen::GenWorld {
            batches: Vec::new(),
            tree_batches: Vec::new(),
            cover_aabbs: Vec::new(),
            placements: 0,
        };
        for (mesh, mats) in &gw.batches {
            renderer.add_instanced(device, queue, mesh, mats);
        }
        for (mesh, mats) in &gw.tree_batches {
            trees.register_batch(&mut renderer, device, queue, mesh, mats.clone());
        }
        // The manifest owns macro geography, but it cannot be allowed to erase the game's north
        // star. This deterministic additive layer supplies human evidence, route debris, household
        // objects, wreck columns and act-specific wordless tableaus even when a full manifest loaded.
        let story = crate::worldgen::generate_story_overlay(act, &hf, 320.0);
        let mut story_cover = story.cover_aabbs.clone();
        // Procedural buildings used to be visual/LOS-only unless selected for destruction. A tank
        // could therefore park a facade inside its occupied interior. Their exact solid-piece boxes
        // join the same static-mass contact path as authored wreck columns and barricades.
        story_cover.extend(city_solid);
        for &(min, max) in &story_cover {
            occluders.push(stealth::Occluder::from_aabb(min, max));
        }
        for (mesh, mats) in &story.batches {
            renderer.add_instanced(device, queue, mesh, mats);
        }
        for (mesh, mats) in &story.tree_batches {
            trees.register_batch(&mut renderer, device, queue, mesh, mats.clone());
        }
        if story.placements > 0 {
            println!(
                "[kessel] environmental story: {} placed traces for act {}",
                story.placements, act.id
            );
        }
        if !arena_pines.is_empty() {
            if let Some(pine) = crate::worldgen::load_kit(crate::worldgen::Kit::Pine) {
                trees.register_batch(&mut renderer, device, queue, &pine, arena_pines);
            } else {
                println!("[kessel] arena: pine kit missing — bowl stays treeless");
            }
        }
        let (n_batches, n_inst) = renderer.instanced_stats();
        println!(
            "[kessel] district: {n_inst} instances in {n_batches} batches ({} destructible trees)",
            trees.count()
        );

        // ── Tank-track rut map covers the whole act window (see mesh.wgsl params.w == 3). ──
        renderer.track_half = act.half_extent_m.max(500.0);
        renderer.mark_track_receiver(0); // scene mesh 0 = the terrain heightfield
                                         // Schutzglas parts: baked separate + GLASS-mode capable, but see-through is OFF for now —
                                         // the cupola blocks are MIRROR periscopes (entrance aperture above, eyepiece below): the
                                         // outside view exits a different face than the one you look at, so transparency is
                                         // geometrically wrong. The plan (user-confirmed): RTT eyepieces — a small offscreen camera
                                         // at each entrance aperture rendered onto the eyepiece face. Until then they draw opaque.
                                         // The PORT glass (direct-view slits) renders SEE-THROUGH — geometrically correct for
                                         // them, unlike the mirror periscopes (which get RTT screens instead).
        for s in tiger_glass.iter().flatten() {
            renderer.set_glass(*s, true);
        }
        // The RTT screens sample the periscope atlas, unlit (they ARE the light).
        for s in tiger_rtt.iter().flatten() {
            renderer.set_mesh_rtt_screen(*s);
        }
        // NOT emissive: the bottle already got the sanctioned dark-albedo fix (brighten_albedo
        // ×2.4 at load). The old own-emissive glow stacked on top of that made it the brightest
        // object in the game — an unlit lamp in a lit interior (sim-light pillar violation).
        for s in breech_lamp {
            renderer.mark_own_emissive(s); // the reload lamp is a lamp
        }
        // ── Runtime grass (grass.rs): density/wind from the act's ground-class mask. ──
        let grass_mask = match act.id {
            2 => Some("assets/terrain/act2/ground_class.png"),
            3 => Some("assets/terrain/act3/ground_class.png"),
            _ => None, // act 1 / arena: winter — sparse dead straw, no mask
        };
        let grass = Some(crate::grass::Grass::new(
            &mut renderer,
            device,
            queue,
            grass_mask,
            act.half_extent_m,
            act.seed,
        ));

        renderer.reticle = true;
        // Campaign-wide weather language. Winter is an active blizzard; the homeland maps retain
        // readable engagement distances but sit under wet/smoke-heavy overcast instead of blue sky.
        renderer.apply_campaign_atmosphere(
            act.id,
            act.season == crate::world::Season::Winter,
            if night {
                DayPhase::Night
            } else {
                DayPhase::Day
            },
        );
        renderer.set_weather_seed(act.seed);
        for (slot, intensity) in &window_slots {
            renderer.mark_window(*slot);
            renderer.set_emissive(*slot, *intensity);
            renderer.set_visible(*slot, night);
        }
        // Warm interior bulb + snow puffs take the own-colour (ungated) emissive path so they glow
        // with their authored amber / white-grey instead of the FX orange. Bulb is driven per frame in
        // `update` (lit only when buttoned up); the snow puffs sit idle until a near-miss arms one.
        renderer.mark_own_emissive(slots.bulb);
        renderer.set_visible(slots.bulb, false);
        renderer.mark_own_emissive(slots.hatch);
        for &s in slots.snow.iter() {
            renderer.mark_own_emissive(s);
            renderer.set_visible(s, false);
        }
        // Crew-of-one interior FX: the brass rounds / casings glow warm, the foam glows white, the
        // lamps glow their own colour — all own-colour emissive (ungated), hidden until driven.
        for &s in [slots.chambered, slots.ram_round]
            .iter()
            .chain(slots.ready_rack.iter())
            .chain(slots.casing.iter())
            .chain(slots.ext_spray.iter())
            .chain(slots.annun.iter())
        {
            renderer.mark_own_emissive(s);
            renderer.set_visible(s, false);
        }
        renderer.set_visible(slots.breech, false); // lit steel block, shown only buttoned-up
                                                   // MG34 FX: flash + tracers ride the FX gunfire path (hidden until fired); the impact spurts
                                                   // take the own-colour warm-white path so a snow/dirt kick reads as powder, not orange fire.
        renderer.set_visible(slots.mg_flash, false);
        for &s in slots.mg_tracer.iter() {
            renderer.set_visible(s, false);
        }
        for &s in slots.mg_impact.iter() {
            renderer.mark_own_emissive(s);
            renderer.set_visible(s, false);
        }
        // The pre-allocated collapsed-rubble heaps sit HIDDEN until their wall is felled (the intact
        // wall is shown; on collapse this flips — intact hidden, rubble revealed).
        for &s in &rubble_slots {
            renderer.set_visible(s, false);
        }

        // Campaign geography is authoritative: the Tiger begins at the cross printed on the paper
        // map, not at the old Stalingrad sandbox coordinate. Face the actual escape axis so the first
        // view, map, route, and world manifest all describe the same journey.
        let spawn_xz = (act.spawn.x, act.spawn.y);
        let to_goal = act.goal - act.spawn;
        let spawn_heading = to_goal.x.atan2(to_goal.y);
        let player = Tank::new(
            spawn_xz.0,
            spawn_xz.1,
            spawn_heading,
            &hf,
            tank::TIGER_PARTS,
        );

        // Swarm muster (see `SWARM`): the OPENING array on the player's terms. The OUTSKIRTS platoon
        // (0,1,2) is the near sniper picket ring around the city's southern approach — the first (long-
        // range) contact the driving player makes; it then ADVANCES + flanks (see `enemy_maneuver`) to
        // reach the Tiger's penetrable sides/rear once alerted. The IN-CITY platoon (3,4,5) is dug DEEPER
        // into the northern ruined streets among the building cover, partially concealed (the stealth LOS
        // respects those occluders) — reached only after the approach. Every enemy starts beyond the
        // stationary detection radius, so a still Tiger opens UNDETECTED and must drive in to make contact.
        // Headings lay toward the spawn. Staggered opening reload so there's no one-frame volley.
        let (px, pz) = spawn_xz;
        let mut enemies: Vec<Tank> = Vec::with_capacity(N_ENEMIES);
        for (i, &(bx, bz, zone, kind, anchored)) in swarm.iter().enumerate() {
            let n = i as u32;
            // Wide muster jitter out in the open; a tight one in the city so a hashed nudge can't shove
            // an in-city tank inside a building wall. Hand-placed manifest anchors get almost none —
            // the worldbuilder put the tank THERE.
            let jit = if anchored {
                2.0
            } else if zone == SpawnZone::Outskirts {
                20.0
            } else {
                6.0
            };
            let (mut dx, mut dz) = (
                bx + (hash01(n * 2 + 1) - 0.5) * jit,
                bz + (hash01(n * 2 + 2) - 0.5) * jit,
            );
            if act.id == 0 {
                // ARENA: a seeded 300 m muster ring around the spawn (the authored SWARM positions
                // belong to act 1's approach geometry, mostly outside the arena window).
                let ang = hash01(act.seed ^ (n * 7919)) * std::f32::consts::TAU;
                dx = px + ang.sin() * 300.0;
                dz = pz + ang.cos() * 300.0;
            }
            let heading = (px - dx).atan2(pz - dz) + (hash01(n + 91) - 0.5) * 0.3; // face the spawn, ±0.15 rad
            let parts = tank::parts_for(kind);
            let mut e = Tank::new(dx, dz, heading, &hf, parts);
            e.kind = kind;
            e.reload = ENEMY_RELOAD * (i as f32 + 0.5) / N_ENEMIES as f32; // staggered opening cadence
            enemies.push(e);
        }

        // hide FX initially
        for s in [muzzle, tracer, impact, smoke] {
            renderer.set_visible(s, false);
        }
        for k in 0..E_FX_POOL {
            renderer.set_visible(e_tracer[k], false);
            renderer.set_visible(e_impact[k], false);
        }
        for &s in blast
            .iter()
            .chain(debris.iter())
            .chain(slots.flame.iter())
            .chain(slots.interior_fire.iter())
        {
            renderer.set_visible(s, false);
        }
        // Replay kill-cam FX start hidden too (only render_replay ever shows them).
        for &s in slots
            .r_shards
            .iter()
            .chain([slots.r_shell, slots.r_muzzle, slots.r_flash].iter())
        {
            renderer.set_visible(s, false);
        }
        for &s in slots.r_crew.iter().chain(slots.r_mod.iter()) {
            renderer.set_visible(s, false);
            renderer.mark_own_emissive(s); // faint schematic glow; hit ones flash hot in the replay
        }

        // Held tools and the loose repair run start hidden; their interaction loops own visibility.
        renderer.set_visible(luger, false);
        renderer.set_visible(luger_flash, false);
        renderer.set_visible(hammer, false);
        for &slot in &slots.track_repair_shoes {
            renderer.set_visible(slot, false);
        }
        renderer.set_visible(slots.track_replacement_link, false);

        // Seed the supply pickups: ground each on the terrain, position its slot once, give it a
        // small emissive glint (MEMORY: the pack pickup albedos are authored near-black, so without
        // a glint they read as missing in fog/interior — don't blame the renderer).
        let mut pickups: Vec<Pickup> = Vec::with_capacity(N_PICKUPS);
        for (i, (kind, x, z)) in PICKUP_SPOTS.iter().enumerate() {
            let slot = pickup0 + i;
            let pos = Vec3::new(*x, hf.height_at(*x, *z), *z);
            // The cigarette pack is true-scale (16 cm) — grow it into oversized loot and give each
            // cache a hash yaw so two packs side by side don't read as copy-paste.
            let (scale, yaw) = match kind {
                PickupKind::Cigarettes => {
                    (2.2, hash01(i as u32 * 0x9E37 + 5) * std::f32::consts::TAU)
                }
                _ => (1.0, 0.0),
            };
            renderer.set_model(
                slot,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(scale),
                    glam::Quat::from_rotation_y(yaw),
                    pos,
                ),
            );
            renderer.set_visible(slot, true);
            renderer.set_emissive(slot, 1.5);
            pickups.push(Pickup {
                pos,
                slot,
                kind: *kind,
                collected: false,
            });
        }

        // ── ABSITZEN cover cylinders — coarse LOS occluders over the wall-cluster footprints (§6):
        // buildings aren't in the heightfield, so these stand in for them. Ducking behind a cluster
        // actually breaks a soldier's line of sight and drops your detection.
        let cover: Vec<(Vec2, f32)> = vec![
            (Vec2::new(-13.0, 11.5), 5.0),  // north ruin cluster
            (Vec2::new(-24.0, 16.0), 3.5),  // north-west lone wall + arch
            (Vec2::new(10.0, -12.5), 4.5),  // south ruin cluster
            (Vec2::new(-25.0, -16.0), 4.0), // south doorway + arch
            (Vec2::new(-6.0, 44.0), 4.5),   // far settlement edge
            (Vec2::new(-30.0, 110.0), 8.0), // city ring (the cigarette cache)
        ];

        // ── ABSITZEN infantry patrols. Routes are seeded deterministically per index, threaded
        // through the corridor between the spawn and the deep caches. Soldiers 0-5 patrol the mid
        // clusters (two loops of three); 6-7 are sentries on the cigarette cache in the city ring.
        // A repeated waypoint makes an idle scanning sentry.
        let soldier_seeds: [(f32, f32, f32, [(f32, f32); 4]); N_SOLDIERS] = [
            // north-approach patrol — its cone sweeps the path from the spawn to the north cluster.
            (
                -9.0,
                20.0,
                0.0,
                [(-9.0, 20.0), (-4.0, 12.0), (-16.0, 12.0), (-14.0, 22.0)],
            ),
            (
                -20.0,
                18.0,
                0.6,
                [(-20.0, 18.0), (-28.0, 22.0), (-26.0, 30.0), (-16.0, 26.0)],
            ),
            (
                -2.0,
                30.0,
                3.1,
                [(-2.0, 30.0), (-8.0, 40.0), (2.0, 42.0), (4.0, 32.0)],
            ),
            // south-approach patrol
            (
                10.0,
                -18.0,
                3.6,
                [(10.0, -18.0), (2.0, -20.0), (0.0, -12.0), (12.0, -12.0)],
            ),
            (
                -22.0,
                -18.0,
                1.0,
                [
                    (-22.0, -18.0),
                    (-30.0, -20.0),
                    (-30.0, -12.0),
                    (-20.0, -12.0),
                ],
            ),
            (
                -6.0,
                60.0,
                3.14,
                [(-6.0, 60.0), (-14.0, 72.0), (-2.0, 80.0), (2.0, 66.0)],
            ),
            // cigarette-cache sentries (idle, scanning) inside the city ring
            (-24.0, 104.0, 3.6, [(-24.0, 104.0); 4]),
            (-36.0, 116.0, 0.4, [(-36.0, 116.0); 4]),
        ];
        let mut soldiers: Vec<Soldier> = Vec::with_capacity(N_SOLDIERS);
        for (i, (x, z, yaw, route)) in soldier_seeds.iter().enumerate() {
            let body_slot = soldier0 + 2 * i;
            let rifle_slot = soldier0 + 2 * i + 1;
            let pos = Vec3::new(*x, hf.height_at(*x, *z), *z);
            let route = std::array::from_fn(|k| {
                let (rx, rz) = route[k];
                Vec3::new(rx, hf.height_at(rx, rz), rz)
            });
            renderer.set_emissive(body_slot, 0.5); // greatcoat grey reads too dark otherwise
            renderer.set_visible(body_slot, false); // shown only on foot
            renderer.set_visible(rifle_slot, false);
            soldiers.push(Soldier::new(
                pos,
                *yaw,
                route,
                i as u32 + 1,
                body_slot,
                rifle_slot,
            ));
        }
        for s in inf_flash {
            renderer.set_visible(s, false);
        }

        // KESSEL_CREWKO=<seconds> forces the crew-KO screen effect PINNED at that phase, for a
        // headless screenshot of the red splatter (~0.3 s) / hand-wipe (~1.8 s). Off in normal play.
        let force_crew_ko = std::env::var("KESSEL_CREWKO")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());
        // KESSEL_CREWKO_DIR=<degrees> pins the on-screen blood-spray axis for a headless screenshot
        // (0° = →, 90° = ↓ in screen space, y-down). Off → the game projects the real incoming round.
        let force_crew_ko_dir = std::env::var("KESSEL_CREWKO_DIR")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|deg| {
                let r = deg.to_radians();
                [r.cos(), r.sin()]
            });
        // KESSEL_BUTTONED pins the player BUTTONED UP in the commander interior (hatch closed, not the
        // gunner optic) so the warm-dark interior can be shot headless with an explicit recon camera or
        // the plain gunner-view screenshot. Off in normal play.
        let force_buttoned = std::env::var("KESSEL_BUTTONED").is_ok();
        let force_driver = std::env::var("KESSEL_DRIVER").is_ok();

        let stealth = stealth::Stealth::new(enemies.len());
        let base_occluders = occluders.len();
        let mut game = Game {
            hf,
            player,
            act: act.clone(),
            death_cause: None,
            grass,
            wrecks: vec![Wreck::default(); enemies.len()],
            enemies,
            allies: ally_state,
            ally_shot_tick: 0,
            campaign_hulks: Vec::new(),
            finale_town_integrity: 100.0,
            civilian_column_alive: true,
            ally_attrition: 0.0,
            ally_damage_tick: 0,
            enemy_active: vec![true; N_ENEMIES],
            enemy_skill: vec![42; N_ENEMIES],
            enemy_concealed: vec![false; N_ENEMIES],
            campaign: None,
            compound: None,
            blasts: vec![Blast::default(); N_BLAST],
            sparks: vec![Spark::default(); N_DEBRIS],
            snow_puffs: vec![SnowPuff::default(); N_SNOW],
            mg_tracers: vec![MgTracer::default(); N_MG_TRACER],
            mg_impacts: vec![MgImpact::default(); N_MG_IMPACT],
            next_mg_tracer: 0,
            next_mg_impact: 0,
            mg_timer: 0.0,
            mg_round: 0,
            mg_flash_t: 0.0,
            mg_flash_pos: Vec3::ZERO,
            mg_heat: 0.0,
            mg_overheated: false,
            mg_hot: false,
            gun_shots: 0,
            engine_death_cued: false,
            next_blast: 0,
            next_spark: 0,
            next_snow: 0,
            kill_tick: 0,
            // KESSEL_BUTTONED forces the commander hatch CLOSED (buttoned up) for headless verification
            // of the warm-dark interior; otherwise open (exposed) by default.
            gunner_optic: false,
            commander_outside: false,
            // KESSEL_HATCH_OPEN pins the cupola lid OPEN (fully swung, hatch_t pre-eased) for
            // headless verification of the baked hatch's swing arc.
            hatch_open: std::env::var("KESSEL_HATCH_OPEN").is_ok(),
            hatch_t: if std::env::var("KESSEL_HATCH_OPEN").is_ok() {
                1.0
            } else {
                0.0
            },
            hover_hatch: false,
            hover_lever: false,
            interior_haze: 0.0,
            bore_smoke_pending: false,
            force_buttoned,
            force_driver,
            chase_cam: false,
            head_yaw: 0.0,
            head_pitch: 0.0,
            interior_light: 0.0,
            slots,
            fx: Fx::default(),
            e_shots: vec![EnemyShot::default(); E_FX_POOL],
            next_e_fx: 0,
            ai_tick: 0,
            armor_tick: 0,
            concussion_t: 0.0,
            concussion_dur: 0.0,
            act_complete: false,
            sights_mag: 0,
            god: false,
            grab: false,
            bail_presses: 0,
            lay_yaw: 0.0,
            lay_pitch: 0.0,
            hover: None,
            hover_label: None,
            hover_station: None,
            rounds_spent: 0,
            walk_pos: Vec3::new(0.85, 0.0, 0.15), // the loader's post, right of the breech
            loose: [LooseShell::default(); N_LOOSE],
            slot_taken: vec![false; crate::interior_rig::SHELL_SLOTS.len()],
            carry_mesh: None,
            carry_pos: Vec3::ZERO,
            carry_vel: Vec3::ZERO,
            carry_dir: Vec3::Z,
            carried_casing: None,
            casing_pull: 0.0,
            drive_cmd: 0.0,
            ext_equipped: false,
            engine_on: true,
            engine_crank_t: 0.0,
            start_noise_t: 0.0,
            prev_ignition: false,
            marks: Vec::new(),
            mark_dwell: None,
            ext_want_spray: false,
            lever_open: 0.0,
            lever_pulling: false,
            hatchlever_t: 0.0,
            slot_nature_he,
            rtt_screens,
            port_aps,
            rtt_dev: Some((device.clone(), queue.clone())),
            rtt_warmed: 0,
            foams: [Foam::default(); N_FOAM],
            next_foam: 0,
            foam_emit_t: 0.0,
            carrying_track_kit: false,
            track_repair_phase: TrackRepairPhase::GrabEnd,
            track_pull: 0.0,
            track_hits: 0,
            hammer_t: 0.0,
            scheduled: Vec::new(),
            ambient_t: AMBIENT_EXP_MIN,
            ambient_tick: 0,
            audio: crate::audio::Audio::new(),
            night, // Tag/Nacht from the title menu: feeds stealth's reduced night detection + the look
            stealth,
            occluders,
            story_cover,
            destructibles,
            trees,
            chunks,
            demo_struct,
            crush_rumble: 0.0,
            crush_phase: 0.0,
            crush_fx_t: 0.0,
            crush_was: false,
            prev_lmb: false,
            he_loaded: false,
            crew: default_crew(),
            modules: default_modules(),
            last_fatal: None,
            fire: 0.0,
            fire_active: false,
            fire_t: 0.0,
            fire_ammo_timer: None,
            fire_origin: None,
            crew_ko_t: 0.0,
            crew_ko_world_dir: Vec3::Z,
            force_crew_ko,
            force_crew_ko_dir,
            replay_cue: 0,
            // Crew-of-one interior workload. Boots posted at the Commander's cupola (matching the
            // buttoned/hatch default) with one round up the spout.
            station: Station::Commander,
            switch_from: Station::Commander,
            switch_t: 0.0,
            switch_dur: 0.0,
            prev_space: false,
            activity: None,
            breech: Breech::Loaded,
            ram: 0.0,
            eject_t: 0.0,
            ext_t: 0.0,
            repair_t: 0.0,
            coop_reload_clock: 0.0,
            lan_local_role: None,
            lan_post_mask: 0,
            prev_coop_hatch: false,
            casings: vec![Casing::default(); N_CASING],
            next_casing: 0,
            driver_hatch: false,
            mode: Mode::Tank,
            foot: FootSoldier::default(),
            pickups,
            soldiers,
            cover,
            noise: [NoiseEvent::default(); 8],
            next_noise: 0,
            inf_flash_t: [0.0; INF_FX],
            inf_flash_pos: [Vec3::ZERO; INF_FX],
            next_inf_fx: 0,
            banked: crate::foot::Supplies::default(),
            enemy_policy: None,
            stat_hits_on_player: 0,
            stat_pens_on_player: 0,
            base_occluders,
            spawn_xz,
            enemy_mg_cd: vec![0.0; N_ENEMIES],
            brawl: false,
            targets_only: false,
            abandoned: abandoned_state,
            pending_hulk: None,
            climb: Climb::default(),
            prev_jump: false,
        };
        for (i, ally) in game.allies.iter().enumerate() {
            let sl = game.slots.allies[i];
            renderer.set_model(sl[0], ally.tank.hull_model());
            renderer.set_model(sl[1], ally.tank.turret_model());
            renderer.set_model(sl[2], ally.tank.gun_model());
        }
        if !game.allies.is_empty() {
            println!(
                "[kessel] Halbe platoon linked: {} friendly Tigers in formation",
                game.allies.len()
            );
        }
        // Pose the abandoned Tigers once (static scenery until taken).
        for (i, (pos, heading, _)) in game.abandoned.iter().enumerate() {
            let t = Tank::new(pos.x, pos.z, *heading, &game.hf, tank::TIGER_PARTS);
            let sl = game.slots.abandoned[i];
            renderer.set_model(sl[0], t.hull_model());
            renderer.set_model(sl[1], t.turret_model());
            renderer.set_model(sl[2], t.gun_model());
        }
        if !game.abandoned.is_empty() {
            println!(
                "[kessel] the Last Tiger rule: {} abandoned Tigers scattered on this act",
                game.abandoned.len()
            );
        }
        println!(
            "[kessel] cover: {} LOS occluders ({} city cells + dressing + battlefield cover)",
            game.occluders.len(),
            city_cells.len()
        );
        // Recommended recon camera for the demolish demo — south of the demo wall, laid on it, so the
        // same explicit camera shows the intact wall BEFORE and (with KESSEL_DEMOLISH) rubble AFTER.
        if let Some(idx) = game.demo_struct {
            let c = game.destructibles[idx].center();
            println!(
                "[kessel] demolish demo wall centre ({:.1},{:.1},{:.1}) — recon cam: {:.1} {:.1} {:.1} 180 -6 0",
                c.x, c.y, c.z, c.x, c.y + 2.2, c.z - 13.0,
            );
        }
        // KESSEL_NEARMISS pins a near-miss snow puff just beside the Tiger, for a headless screenshot
        // of the near-miss FX (a recon-cam frame shows the kicked-up snow next to the hull). The one
        // update the screenshot harness runs decays it only a frame, so it renders near peak. Off in play.
        if std::env::var("KESSEL_NEARMISS").is_ok() {
            let p = game.player.center();
            game.spawn_snow_puff(p + Vec3::new(3.2, -1.2, 2.2), 0.85);
        }
        // KESSEL_DEMOLISH shells the demo wall until it COLLAPSES right now (rubble revealed + a fresh
        // debris/dust burst), so a single-frame recon screenshot captures the AFTER state; the SAME
        // recon camera with the env unset shows the intact wall BEFORE. Exercises the real
        // damage → collapse → occluder-downgrade path (not a shortcut). Off in normal play.
        if std::env::var("KESSEL_DEMOLISH").is_ok() {
            if let Some(idx) = game.demo_struct {
                let mut guard = 0;
                while game.destructibles[idx].alive && guard < 16 {
                    game.damage_structure(idx, STRUCT_SHELL_DAMAGE, &mut renderer);
                    guard += 1;
                }
                println!("[kessel] KESSEL_DEMOLISH: felled demo wall in {guard} shell hit(s)");
            }
        }
        Ok((renderer, game))
    }

    /// Turn a disposable act scene into one attempt of the persistent campaign. The six physical
    /// enemy slots become a streaming encounter pool, while the one Tiger receives the exact stores
    /// and damage state carried out of the previous act.
    pub fn configure_campaign(&mut self, start: &crate::campaign::ActStart) {
        assert_eq!(
            self.act.id, start.act_id,
            "campaign start must match the built act"
        );

        let heading = {
            let d = self.act.goal - start.spawn;
            d.x.atan2(d.y)
        };
        self.player = Tank::new(
            start.spawn.x,
            start.spawn.y,
            heading,
            &self.hf,
            tank::TIGER_PARTS,
        );
        self.spawn_xz = (start.spawn.x, start.spawn.y);
        self.player.hp = start.persistent.tiger.hull_condition as f32;
        self.player.alive = start.persistent.tiger.hull_condition > 0;
        for (module, system) in self
            .modules
            .iter_mut()
            .zip(crate::campaign::TigerSystem::ALL)
        {
            module.functional = start.persistent.tiger.functional(system);
        }

        // Rebuild the visible rack from typed carried ammunition. APCR currently rides the AP mesh
        // and ballistic family, but remains a distinct persistent count for the future shell model.
        self.slot_taken.fill(true);
        let inv = &start.persistent.inventory;
        let ap_like = usize::from(inv.ap_rounds.saturating_add(inv.apcr_rounds));
        let he = usize::from(inv.he_rounds);
        let rack = (ap_like + he).min(self.slot_taken.len());
        for i in 0..rack {
            self.slot_taken[i] = false;
            self.slot_nature_he[i] = i >= ap_like.min(rack);
        }
        self.rounds_spent = self.slot_taken.iter().filter(|taken| **taken).count() as u32;
        self.he_loaded = false;
        self.breech = if rack > 0 {
            Breech::Loaded
        } else {
            Breech::Empty
        };
        self.foot.ammo = u32::from(inv.luger_rounds);

        // The authored campaign follows one named, increasingly scarred Tiger. The sandbox's
        // mountable replacement hulls remain as environmental relics, but cannot launder damage or
        // silently replace the persistent vehicle during a campaign attempt.
        for (_, _, taken) in &mut self.abandoned {
            *taken = true;
        }

        self.campaign = Some(CampaignRuntime {
            plan: start.encounters.clone(),
            encounter: 0,
            unit_cursor: 0,
            wave_live: false,
            transition_t: 0.0,
            distance_m: 0.0,
            last_pos: start.spawn,
            milestone_bits: 0,
            discovered: Vec::new(),
            signals: Vec::new(),
            inventory: inv.clone(),
            fuel_liters: f32::from(inv.fuel_liters),
            total_kills: 0,
            battle_pressure: 0.0,
            debug_force_triggers: false,
            gate_rearm_wait: false,
        });
        self.act_complete = false;
        // Contacts exist in the terrain, not at a menu timer. Start with the reusable rigs benched;
        // the director activates whichever route trigger the player actually crosses or provokes.
        for i in 0..self.enemies.len() {
            self.enemy_active[i] = false;
            self.enemy_skill[i] = 0;
            self.enemy_concealed[i] = false;
            self.enemies[i].alive = false;
        }
        println!(
            "[campaign] act {} attempt {} — {} encounters / {} units, fuel {} L, rack {}",
            start.act_id,
            start.attempt_number,
            start.encounters.encounters.len(),
            start.encounters.total_units(),
            inv.fuel_liters,
            rack,
        );
    }

    /// Let the campaign exit trigger fire again after the persistent objective layer rejects an
    /// attempted completion.  The director intentionally edge-triggers the gate so it cannot spam
    /// signals every frame; a rejected attempt must therefore clear both halves of that latch.
    pub fn rearm_campaign_gate(&mut self) {
        self.act_complete = false;
        if let Some(runtime) = &mut self.campaign {
            runtime.gate_rearm_wait = true;
        }
    }

    /// Copy the live, battered Tiger and scavenged stores back into the persistent run checkpoint.
    pub fn export_campaign_state(&self, out: &mut crate::campaign::PersistentSnapshot) {
        for ((module, system), condition) in self
            .modules
            .iter()
            .zip(crate::campaign::TigerSystem::ALL)
            .zip(out.tiger.system_condition.iter_mut())
        {
            let _ = system; // ordering is the stable bridge between the two engine-agnostic enums
            *condition = if module.functional { 100 } else { 0 };
        }
        out.tiger.hull_condition = self.player.hp.clamp(0.0, 100.0) as u8;
        out.tiger.crew_fitness = if self.player.alive { 100 } else { 0 };
        out.tiger.interior_fire = (self.fire.clamp(0.0, 1.0) * 100.0) as u8;
        out.tiger.visible_scars = out
            .tiger
            .visible_scars
            .saturating_add(self.stat_pens_on_player.min(u16::MAX as u32) as u16);

        if let Some(runtime) = &self.campaign {
            out.inventory = runtime.inventory.clone();
            out.inventory.fuel_liters = runtime.fuel_liters.clamp(0.0, u16::MAX as f32) as u16;
            let mut ap_like = 0u16;
            let mut he = 0u16;
            for (i, taken) in self.slot_taken.iter().enumerate() {
                if *taken {
                    continue;
                }
                if self.slot_nature_he[i] {
                    he = he.saturating_add(1);
                } else {
                    ap_like = ap_like.saturating_add(1);
                }
            }
            out.inventory.apcr_rounds = out.inventory.apcr_rounds.min(ap_like);
            out.inventory.ap_rounds = ap_like.saturating_sub(out.inventory.apcr_rounds);
            out.inventory.he_rounds = he;
            out.inventory.luger_rounds = self.foot.ammo.min(u16::MAX as u32) as u16;
            // Supplies only become campaign property after the player physically gets them back to
            // the Tiger. Each pickup is a useful cache, not one literal cartridge or litre.
            out.inventory.ap_rounds = out
                .inventory
                .ap_rounds
                .saturating_add((self.banked.ammo as u16).saturating_mul(6));
            out.inventory.fuel_liters = out
                .inventory
                .fuel_liters
                .saturating_add((self.banked.fuel as u16).saturating_mul(35));
            out.inventory.medical_supplies = out
                .inventory
                .medical_supplies
                .saturating_add(self.banked.medical.min(u8::MAX as u32) as u8);
            out.inventory.cigarettes = out
                .inventory
                .cigarettes
                .saturating_add(self.banked.cigarettes.min(u16::MAX as u32) as u16);
        }
    }

    pub fn drain_mission_signals(&mut self) -> Vec<MissionSignal> {
        self.campaign
            .as_mut()
            .map(|runtime| std::mem::take(&mut runtime.signals))
            .unwrap_or_default()
    }

    pub fn mission_hud(&self) -> Option<MissionHud> {
        let runtime = self.campaign.as_ref()?;
        Some(MissionHud {
            act: self.act.id,
            encounter: (runtime.encounter + usize::from(runtime.wave_live))
                .min(runtime.plan.encounters.len()),
            encounters: runtime.plan.encounters.len(),
            fuel_liters: runtime.fuel_liters,
            rack_rounds: self.slot_taken.iter().filter(|taken| !**taken).count(),
            route_locked: runtime.wave_live
                || runtime.plan.encounters[runtime.encounter.min(runtime.plan.encounters.len())..]
                    .iter()
                    .any(|encounter| encounter.mandatory),
            finale: self.act.final_battle,
            friendly_tigers: self.friendly_tigers_surviving(),
            town_integrity: self.finale_town_integrity_percent(),
        })
    }

    pub fn campaign_kills(&self) -> u16 {
        self.campaign
            .as_ref()
            .map_or(0, |runtime| runtime.total_kills)
    }

    pub fn friendly_tigers_surviving(&self) -> u8 {
        self.allies
            .iter()
            .filter(|ally| ally.tank.alive)
            .count()
            .min(u8::MAX as usize) as u8
    }

    pub fn finale_town_integrity_percent(&self) -> u8 {
        self.finale_town_integrity.clamp(0.0, 100.0).round() as u8
    }

    pub fn civilian_column_survived(&self) -> bool {
        self.civilian_column_alive
    }

    pub fn campaign_final_defeat_reason(&self) -> Option<crate::campaign::FinalDefeatReason> {
        if self.campaign.is_none() || self.act.id != 3 || self.act_complete {
            return None;
        }
        if self.finale_town_integrity <= 0.0 {
            Some(crate::campaign::FinalDefeatReason::TownOverrun)
        } else {
            None
        }
    }

    /// Deterministic headless campaign smoke-test hook. It resolves every streamed contact through
    /// the same death/director path, then moves the Tiger to the authored gate. This is not bound in
    /// live play; it exists so CI can prove Act 1 → Act 2 → finale without simulating hours of input.
    pub fn debug_complete_campaign_act(&mut self) {
        if let Some(runtime) = self.campaign.as_mut() {
            runtime.distance_m = 5000.0;
            runtime.battle_pressure = 100.0;
            runtime.debug_force_triggers = true;
        }
        let mut guard = 0usize;
        loop {
            let done = self.campaign.as_ref().is_none_or(|runtime| {
                runtime.encounter >= runtime.plan.encounters.len() && !runtime.wave_live
            });
            if done {
                break;
            }
            for i in 0..self.enemies.len() {
                if self.enemy_active[i] && self.enemies[i].alive {
                    self.destroy_enemy(i);
                }
            }
            self.update_campaign_director(5.0);
            guard += 1;
            assert!(guard < 128, "campaign contact stream failed to terminate");
        }
        self.player.pos.x = self.act.goal.x;
        self.player.pos.z = self.act.goal.y;
        self.player.pos.y = self.hf.height_at(self.player.pos.x, self.player.pos.z);
        self.update_campaign_director(0.1);
    }

    fn spawn_campaign_wave(&mut self) {
        let Some(runtime) = self.campaign.as_ref() else {
            return;
        };
        if runtime.encounter >= runtime.plan.encounters.len() {
            return;
        }
        self.archive_campaign_wrecks();
        let runtime = self.campaign.as_ref().expect("campaign checked above");
        let encounter = runtime.plan.encounters[runtime.encounter].clone();
        let begin = runtime.unit_cursor;
        let end = (begin + N_ENEMIES).min(encounter.units.len());
        let units: Vec<_> = encounter.units[begin..end].to_vec();

        for i in 0..self.enemies.len() {
            self.enemy_active[i] = false;
            self.enemy_skill[i] = 0;
            self.enemy_concealed[i] = false;
            self.enemies[i].alive = false;
            self.enemies[i].speed = 0.0;
            self.wrecks[i] = Wreck::default();
        }
        for (i, unit) in units.iter().enumerate() {
            use crate::campaign::CampaignEnemy;
            let kind = match unit.kind {
                CampaignEnemy::T34 => EnemyKind::T34,
                CampaignEnemy::Su100 => EnemyKind::Su100,
                CampaignEnemy::Kv2 => EnemyKind::Kv2,
                CampaignEnemy::Isu152 => EnemyKind::Isu152,
                CampaignEnemy::Is2 => EnemyKind::Is2,
                CampaignEnemy::Is3 => EnemyKind::Is3,
            };
            let p = encounter.anchor + unit.local_offset;
            let heading = (self.player.pos.x - p.x).atan2(self.player.pos.z - p.y);
            let parts = tank::parts_for(kind);
            let mut enemy = Tank::new(p.x, p.y, heading, &self.hf, parts);
            enemy.kind = kind;
            enemy.reload = ENEMY_RELOAD * (i as f32 + 0.7) / units.len().max(1) as f32;
            self.enemies[i] = enemy;
            self.enemy_active[i] = true;
            self.enemy_skill[i] = unit.skill;
            self.enemy_concealed[i] = unit.concealed;
        }
        self.stealth = stealth::Stealth::new(self.enemies.len());
        for shot in &mut self.e_shots {
            *shot = EnemyShot::default();
        }
        self.enemy_mg_cd.fill(0.0);
        if let Some(runtime) = self.campaign.as_mut() {
            runtime.unit_cursor = end;
            runtime.wave_live = true;
            runtime.transition_t = 0.0;
        }
        println!(
            "[director] contact {}/{} {:?} {:?}: {} armour (units {}..{})",
            self.campaign.as_ref().unwrap().encounter + 1,
            self.campaign.as_ref().unwrap().plan.encounters.len(),
            encounter.corridor,
            encounter.role,
            units.len(),
            begin + 1,
            end,
        );
    }

    fn archive_campaign_wrecks(&mut self) {
        if self.campaign.is_none() || self.slots.hulk_t34.is_empty() {
            return;
        }
        for i in 0..self.enemies.len() {
            if self.campaign_hulks.len() >= self.slots.hulk_t34.len() {
                break;
            }
            if !self.enemy_active[i] || self.enemies[i].alive {
                continue;
            }
            let e = &self.enemies[i];
            let w = &self.wrecks[i];
            let models = match w.kind {
                WreckKind::TurretPop => [e.hull_model(), w.turret_pop_xf(), w.gun_pop_xf()],
                _ => [e.hull_model(), e.turret_model(), e.gun_model()],
            };
            self.campaign_hulks.push(CampaignHulk {
                kind: e.kind,
                models,
                ember: if w.burning { 0.22 } else { 0.0 },
                burning: w.burning,
            });
        }
    }

    fn campaign_trigger_ready(
        runtime: &CampaignRuntime,
        encounter: &crate::campaign::EncounterSpec,
        pos: Vec2,
    ) -> bool {
        use crate::campaign::{EncounterRole, EncounterTrigger, FinalePhase, ObjectiveId};
        if runtime.debug_force_triggers || runtime.unit_cursor > 0 {
            return true;
        }
        // Physically entering the prepared position always springs it, even when the nominal trigger
        // is noise or phase. That prevents a silent player from driving through dormant armour.
        if pos.distance(encounter.anchor) <= encounter.activation_radius_m {
            return true;
        }
        match encounter.trigger {
            EncounterTrigger::EnterRadius => false,
            EncounterTrigger::NoisePressure(threshold) => {
                runtime.battle_pressure >= f32::from(threshold)
            }
            EncounterTrigger::AfterObjective(objective) => match objective {
                ObjectiveId::Act1WitnessCityLayers => {
                    runtime
                        .discovered
                        .iter()
                        .filter(|site| site.act() == 1)
                        .count()
                        >= 2
                }
                ObjectiveId::Act1BreakWesternCord => runtime.plan.encounters
                    [..runtime.encounter.min(runtime.plan.encounters.len())]
                    .iter()
                    .any(|past| past.role == EncounterRole::BlockingForce),
                ObjectiveId::Act2FindOldHouse => runtime
                    .discovered
                    .contains(&crate::campaign::StorySiteId::Act2OldHouse),
                ObjectiveId::Act2TraceHomecoming => {
                    runtime
                        .discovered
                        .iter()
                        .filter(|site| site.act() == 2)
                        .count()
                        >= 2
                }
                _ => false,
            },
            EncounterTrigger::FinalePhase(phase) => match phase {
                FinalePhase::Approach => runtime.distance_m >= 160.0,
                FinalePhase::Preparation => runtime.milestone_bits & 0b1110 != 0,
                FinalePhase::Assault => runtime.milestone_bits & 0b1110 == 0b1110,
                FinalePhase::LastStand => runtime.plan.encounters
                    [..runtime.encounter.min(runtime.plan.encounters.len())]
                    .iter()
                    .any(|past| past.role == EncounterRole::AssaultWave),
                FinalePhase::Epilogue | FinalePhase::Resolved => false,
            },
        }
    }

    /// Pick the first triggered route from the unresolved plan. Optional encounters before that
    /// route are genuinely bypassed; an untriggered mandatory cordon stops the scan and cannot be
    /// skipped. This gives spatial route choice without needing simultaneous GPU enemy pools.
    fn next_campaign_encounter(runtime: &CampaignRuntime, pos: Vec2) -> Option<usize> {
        for index in runtime.encounter..runtime.plan.encounters.len() {
            let encounter = &runtime.plan.encounters[index];
            if Self::campaign_trigger_ready(runtime, encounter, pos) {
                return Some(index);
            }
            if encounter.mandatory {
                break;
            }
        }
        None
    }

    fn update_campaign_director(&mut self, dt: f32) {
        let Some(_) = self.campaign else { return };
        // Exploration belongs to the crew, not only the hull.  A dismounted commander can read a
        // story site, scout a route, and prepare Halbe even when the Tiger is immobilised or dry.
        let actor = self.threat_center();
        let pos = Vec2::new(actor.x, actor.z);
        let mut spawn_next = false;
        let mut objective_events: Vec<MissionSignal> = Vec::new();

        {
            let runtime = self.campaign.as_mut().unwrap();
            if runtime.gate_rearm_wait && !self.act.reached_goal(pos) {
                runtime.gate_rearm_wait = false;
                runtime.milestone_bits &= !(1 << 15);
            }
            let step = pos.distance(runtime.last_pos).min(80.0);
            runtime.distance_m += step;
            runtime.last_pos = pos;

            // A Tiger I burns roughly half a litre per kilometre only on paper; mud, snow, idling,
            // damaged running gear and combat manoeuvre make this campaign consumption deliberately
            // harsher. Engine-off reconnaissance is therefore materially valuable.
            if self.engine_on && self.player.alive {
                let moving = self.player.speed.abs();
                let burn = (0.018 + moving * 0.010) * dt;
                runtime.fuel_liters = (runtime.fuel_liters - burn).max(0.0);
                if runtime.fuel_liters <= 0.0 {
                    self.engine_on = false;
                }
            }

            let mut milestone = |bit: u16, signal: MissionSignal| {
                if runtime.milestone_bits & bit == 0 {
                    runtime.milestone_bits |= bit;
                    objective_events.push(signal);
                }
            };
            match self.act.id {
                1 if runtime.distance_m > 220.0 => milestone(
                    1 << 0,
                    MissionSignal::Objective(
                        crate::campaign::ObjectiveId::Act1ChooseBreakoutRoute,
                        1,
                    ),
                ),
                2 if runtime.distance_m > 650.0 => milestone(
                    1 << 0,
                    MissionSignal::Objective(crate::campaign::ObjectiveId::Act2CrossOderbruch, 1),
                ),
                3 => {
                    if runtime.distance_m > 160.0 {
                        milestone(
                            1 << 0,
                            MissionSignal::Objective(
                                crate::campaign::ObjectiveId::Act3LinkWithPlatoon,
                                1,
                            ),
                        );
                    }
                    for (bit, threshold) in
                        [(1u16 << 1, 420.0), (1u16 << 2, 850.0), (1u16 << 3, 1320.0)]
                    {
                        if runtime.distance_m > threshold {
                            milestone(
                                bit,
                                MissionSignal::Objective(
                                    crate::campaign::ObjectiveId::Act3PrepareDefenses,
                                    1,
                                ),
                            );
                        }
                    }
                }
                _ => {}
            }

            for &(site, sx, sz) in story_sites(self.act.id) {
                if pos.distance(Vec2::new(sx, sz)) <= 125.0 && !runtime.discovered.contains(&site) {
                    runtime.discovered.push(site);
                    objective_events.push(MissionSignal::Discovery(site));
                }
            }

            let all_dead = self
                .enemy_active
                .iter()
                .zip(&self.enemies)
                .all(|(active, enemy)| !*active || !enemy.alive);
            if runtime.wave_live && all_dead {
                runtime.wave_live = false;
                runtime.transition_t = if self.act.final_battle { 2.2 } else { 3.8 };
                let encounter = &runtime.plan.encounters[runtime.encounter];
                if runtime.unit_cursor >= encounter.units.len() {
                    let cleared = runtime.encounter;
                    if let Some(site) = encounter.story_site {
                        if !runtime.discovered.contains(&site) {
                            runtime.discovered.push(site);
                            objective_events.push(MissionSignal::Discovery(site));
                        }
                    }
                    use crate::campaign::{EncounterRole, ObjectiveId};
                    match (self.act.id, encounter.role) {
                        (1, EncounterRole::BlockingForce) => objective_events.push(
                            MissionSignal::Objective(ObjectiveId::Act1BreakWesternCord, 1),
                        ),
                        (1, EncounterRole::Boss) => objective_events
                            .push(MissionSignal::Objective(ObjectiveId::Act1DefeatKv2, 1)),
                        (2, EncounterRole::Boss) => objective_events
                            .push(MissionSignal::Objective(ObjectiveId::Act2DefeatIsu152, 1)),
                        (3, EncounterRole::AssaultWave) => objective_events.push(
                            MissionSignal::Objective(ObjectiveId::Act3HoldFirstAssault, 1),
                        ),
                        (3, EncounterRole::ArmouredWedge) => objective_events
                            .push(MissionSignal::Objective(ObjectiveId::Act3BreakArmada, 1)),
                        _ => {}
                    }
                    if self.act.id == 3 && cleared < 3 {
                        objective_events.push(MissionSignal::Objective(
                            ObjectiveId::Act3PrepareDefenses,
                            1,
                        ));
                    }
                    runtime.encounter += 1;
                    runtime.unit_cursor = 0;
                }
            }
            if !runtime.wave_live && runtime.encounter < runtime.plan.encounters.len() {
                runtime.transition_t -= dt;
                if runtime.transition_t <= 0.0 {
                    if let Some(next) = Self::next_campaign_encounter(runtime, pos) {
                        if next > runtime.encounter {
                            for skipped in runtime.encounter..next {
                                println!(
                                    "[director] bypassed optional {:?} route {:?}",
                                    runtime.plan.encounters[skipped].role,
                                    runtime.plan.encounters[skipped].corridor
                                );
                            }
                            runtime.encounter = next;
                        }
                        spawn_next = true;
                    }
                }
            }

            let route_clear = !runtime.wave_live
                && !runtime.plan.encounters[runtime.encounter.min(runtime.plan.encounters.len())..]
                    .iter()
                    .any(|encounter| encounter.mandatory);
            if route_clear
                && !runtime.gate_rearm_wait
                && self.act.reached_goal(pos)
                && runtime.milestone_bits & (1 << 15) == 0
            {
                runtime.milestone_bits |= 1 << 15;
                use crate::campaign::{ObjectiveId, StorySiteId};
                match self.act.id {
                    1 => objective_events.push(MissionSignal::Objective(
                        ObjectiveId::Act1ReachWesternExit,
                        1,
                    )),
                    2 => objective_events.push(MissionSignal::Objective(
                        ObjectiveId::Act2ReachEmptyRendezvous,
                        1,
                    )),
                    3 => {
                        if !runtime
                            .discovered
                            .contains(&StorySiteId::Act3ForestCemetery)
                        {
                            runtime.discovered.push(StorySiteId::Act3ForestCemetery);
                            objective_events
                                .push(MissionSignal::Discovery(StorySiteId::Act3ForestCemetery));
                        }
                    }
                    _ => {}
                }
                self.act_complete = true;
                println!(
                    "[director] all contacts cleared — act {} end gate open",
                    self.act.id
                );
            }
            runtime.signals.extend(objective_events);
        }
        if spawn_next {
            self.spawn_campaign_wave();
        }
    }

    /// Drive the Halbe wingmen in a loose echelon and let them take deliberate shots at exposed
    /// armour. Their rounds use the existing in-flight tracer and destruction pipeline, so an ally
    /// kill leaves the same wreck, fire, debris, sound delay and hard cover as the player's kill.
    fn update_allied_tigers(&mut self, dt: f32) {
        if self.allies.is_empty() {
            return;
        }
        let player_pos = self.player.pos;
        let player_forward = self.player.forward();
        let player_right = self.player.right();
        let mut fired: Vec<(usize, Vec3, Vec3)> = Vec::new();

        for ally_index in 0..self.allies.len() {
            if !self.allies[ally_index].tank.alive {
                continue;
            }
            let ally_pos = self.allies[ally_index].tank.pos;
            let target = self
                .enemies
                .iter()
                .enumerate()
                .filter(|(i, enemy)| self.enemy_active[*i] && enemy.alive)
                .filter(|(_, enemy)| self.los_clear(ally_pos + Vec3::Y * 2.0, enemy.center()))
                .min_by(|(_, a), (_, b)| {
                    ally_pos
                        .distance_squared(a.pos)
                        .total_cmp(&ally_pos.distance_squared(b.pos))
                })
                .map(|(i, enemy)| (i, enemy.center()));

            let ally = &mut self.allies[ally_index];
            ally.fire_cd = (ally.fire_cd - dt).max(0.0);
            let formation = player_pos - player_forward * (16.0 + ally_index as f32 * 7.0)
                + player_right * ally.formation_side * 14.0;
            let to_formation = formation - ally.tank.pos;
            let formation_dist = Vec2::new(to_formation.x, to_formation.z).length();
            let combat_stop =
                target.is_some_and(|(_, target_pos)| ally.tank.pos.distance(target_pos) < 1050.0);
            let drive = if combat_stop {
                0.0
            } else if formation_dist > 9.0 {
                0.62
            } else {
                0.0
            };
            let steer = if formation_dist > 2.0 {
                let want = to_formation.x.atan2(to_formation.z);
                let mut err = want - ally.tank.heading;
                while err > std::f32::consts::PI {
                    err -= std::f32::consts::TAU;
                }
                while err < -std::f32::consts::PI {
                    err += std::f32::consts::TAU;
                }
                (err * 1.8).clamp(-1.0, 1.0)
            } else {
                0.0
            };
            ally.tank.update(drive, steer, 0.0, 0.0, dt, &self.hf);

            if let Some((target_index, target_pos)) = target {
                let laid = ally.tank.ai_aim(target_pos, dt);
                if laid && ally.tank.can_fire() && ally.fire_cd <= 0.0 {
                    let (muzzle, _) = ally.tank.muzzle();
                    fired.push((target_index, muzzle, target_pos));
                    ally.tank.fired();
                    ally.fire_cd = 8.5 + ally_index as f32 * 1.7;
                }
            }
        }

        for (target_index, muzzle, target_pos) in fired {
            self.ally_shot_tick = self.ally_shot_tick.wrapping_add(1);
            let slot = self.next_e_fx;
            self.next_e_fx = (self.next_e_fx + 1) % E_FX_POOL;
            let distance = muzzle.distance(target_pos);
            self.e_shots[slot] = EnemyShot {
                tracer_active: true,
                tracer_t: 0.0,
                tracer_dur: (distance / 790.0).max(0.06),
                tracer_from: muzzle,
                tracer_to: target_pos,
                tracer_dir: (target_pos - muzzle).normalize_or_zero(),
                outcome: Outcome::Penetrate,
                hit_enemy: Some(target_index),
                near_pt: target_pos,
                ..EnemyShot::default()
            };
            self.spawn_blast(muzzle, 2.2, 18.0, 0.16);
            let reports = crate::audio::Sfx::PLAYER_GUNS_EXT;
            let report = reports[self.ally_shot_tick as usize % reports.len()];
            self.schedule_sound_from(report, muzzle, 1.05);
            println!(
                "[platoon] Tiger {} firing on contact {} at {:.0} m",
                2 + (self.ally_shot_tick as usize & 1),
                target_index + 1,
                distance
            );
        }
    }

    /// The Halbe battle has things to lose beyond the player hull. Unanswered enemy armour chews up
    /// the town and, on a deterministic pressure clock, finds the two wingmen. Fast, clean gunnery
    /// preserves both; a long desperate hold can end with burnt-out friends, a ruined town, or the
    /// civilian road cut even if the player's Tiger is still running.
    fn update_finale_pressure(&mut self, dt: f32) {
        let wave_live = self
            .campaign
            .as_ref()
            .is_some_and(|runtime| runtime.wave_live);
        if self.act.id != 3 || !wave_live {
            return;
        }
        let hostile = self
            .enemies
            .iter()
            .enumerate()
            .filter(|(i, enemy)| self.enemy_active[*i] && enemy.alive)
            .count();
        if hostile == 0 {
            return;
        }

        self.finale_town_integrity =
            (self.finale_town_integrity - hostile as f32 * 0.012 * dt).max(0.0);
        self.ally_attrition += hostile as f32 * dt;
        while self.ally_attrition >= 280.0 {
            self.ally_attrition -= 280.0;
            let living: Vec<usize> = self
                .allies
                .iter()
                .enumerate()
                .filter(|(_, ally)| ally.tank.alive)
                .map(|(i, _)| i)
                .collect();
            if living.is_empty() {
                break;
            }
            self.ally_damage_tick = self.ally_damage_tick.wrapping_add(1);
            let index = living[self.ally_damage_tick as usize % living.len()];
            let ally = &mut self.allies[index].tank;
            ally.hp = (ally.hp - 38.0).max(0.0);
            if ally.hp <= 0.0 {
                ally.alive = false;
                ally.speed = 0.0;
                let center = ally.center();
                self.spawn_blast(center, 7.0, 24.0, 0.75);
                self.schedule_sound_from(crate::audio::Sfx::CookOff, center, VOL_KILL);
                self.finale_town_integrity = (self.finale_town_integrity - 8.0).max(0.0);
                println!(
                    "[platoon] Tiger {} knocked out — one less gun at Halbe",
                    index + 2
                );
            } else {
                println!(
                    "[platoon] Tiger {} hit under sustained pressure — {:.0}% fighting condition",
                    index + 2,
                    ally.hp
                );
            }
        }
        if self.finale_town_integrity <= 22.0 {
            self.civilian_column_alive = false;
        }
    }

    pub fn update(&mut self, input: &TankInput, dt: f32, r: &mut Renderer) {
        // LAN co-op splits the physical jobs without forking the simulation. The ordinary `input`
        // always remains this machine's view/body control; these station channels are the host-
        // merged authority used for the actual drivetrain, lay, weapons and breech.
        let coop = input.coop;
        let driver_input = coop.map(|frame| frame.input(crate::lan::LanRole::Driver));
        let gunner_input = coop.map(|frame| frame.input(crate::lan::LanRole::Gunner));
        let loader_input = coop.map(|frame| frame.input(crate::lan::LanRole::Loader));
        let radio_input = coop.map(|frame| frame.input(crate::lan::LanRole::Radio));
        let commander_input = coop.map(|frame| frame.input(crate::lan::LanRole::Commander));
        self.lan_post_mask = coop.map_or(0, |frame| {
            crate::lan::LanRole::ALL
                .iter()
                .filter(|&&role| frame.occupied(role) && self.lan_local_role != Some(role))
                .fold(0u8, |mask, &role| {
                    mask | (1 << frame.input(role).post as u8)
                })
        });
        self.drive_lan_crew_rigs(coop, r);
        // KOOP STÜTZPUNKT mission phases (inert None outside the mission).
        self.update_compound(dt);
        let mut activity_input = *input;
        if let Some(frame) = coop {
            activity_input.extinguish |= crate::lan::LanRole::ALL
                .iter()
                .any(|&role| frame.input(role).flag(crate::lan::F_EXTINGUISH));
            activity_input.repair |= crate::lan::LanRole::ALL
                .iter()
                .any(|&role| frame.input(role).flag(crate::lan::F_REPAIR));
        }
        let coop_hatch = coop.is_some_and(|frame| {
            crate::lan::LanRole::ALL
                .iter()
                .any(|&role| frame.input(role).flag(crate::lan::F_HATCH_TOGGLE))
        });
        if input.coop_host && coop_hatch && !self.prev_coop_hatch {
            self.toggle_hatch();
        }
        self.prev_coop_hatch = input.coop_host && coop_hatch;
        // Keep the renderer's time-of-day in step with the game (drives the night sky/lighting + snow
        // density); constant per session, but cheap and keeps the two in sync from a single source.
        r.night = if self.night { 1.0 } else { 0.0 };
        r.time += dt;
        self.drive_cmd = 0.0; // re-asserted by the station gate when driving is actually live

        // ── Tank tracks: every moving tank compresses the snow/dirt under its treads (rut map). ──
        if self.player.alive && self.player.speed.abs() > 0.25 {
            r.stamp_tank_tracks(self.player.pos, self.player.heading, 0.28);
        }
        for e in self
            .enemies
            .iter()
            .filter(|e| e.alive && e.speed.abs() > 0.25)
        {
            r.stamp_tank_tracks(e.pos, e.heading, 0.28);
        }
        for ally in self
            .allies
            .iter()
            .filter(|ally| ally.tank.alive && ally.tank.speed.abs() > 0.25)
        {
            r.stamp_tank_tracks(ally.tank.pos, ally.tank.heading, 0.28);
        }
        // ── Grass window follows the camera (rebuilds only on chunk crossings). ──
        if let Some(g) = self.grass.as_mut() {
            let hf = &self.hf;
            g.update(r, r.camera.pos, |x, z| hf.height_at(x, z));
        }

        // ABSITZEN: dispatch on mode. Foot = walk the dismounted soldier + a third camera branch;
        // Tank = the original stations + drive + fire + per-station camera. In BOTH modes the world
        // stays live below (enemy AI runs in each arm; FX pools + tank/enemy transform pushes run in
        // the shared tail) so the abandoned Tiger and the swarm keep rendering while dismounted.
        if let Mode::Foot = self.mode {
            self.player.update(0.0, 0.0, 0.0, 0.0, dt, &self.hf); // parked Tiger settles on terrain
            if self.climb.active {
                self.update_climb(input, dt, r); // mount/dismount ritual owns the camera + foot pos
            } else {
                self.update_foot(input, dt, r);
            }
            // On foot: binoculars get the double-circle optic mask; otherwise the frame is clean.
            r.optic_mask = if input.binoculars && !self.climb.active {
                3.0
            } else {
                0.0
            };
            // One-click mount: crosshair on a mountable Tiger's cupola → the hover tell.
            if !self.climb.active {
                self.hover_label = if self
                    .foot_mount_hover(r.camera.forward(), r.camera.pos)
                    .is_some()
                {
                    Some("AUFSITZEN [click]")
                } else {
                    None
                };
            }
            self.update_infantry(input, dt, r); // soldiers see/hear/shoot; drives their slots (you're EXPOSED mid-climb)
            self.enemy_ai(dt);
            self.prev_jump = input.jump; // Space edge-detect for jump / climb-advance
                                         // A climb that finished INTO the tank this frame flips the mode — the tank arm picks
                                         // up next frame; nothing else to do here.
            if matches!(self.mode, Mode::Tank) {
                // fall through to the shared tail below via the closing brace
            }
            // Outside the hull: no warm interior fill, bulb off.
            self.interior_light = 0.0;
            r.interior_light = 0.0;
            r.precipitation = if self.act.season == crate::world::Season::Winter {
                1.0
            } else {
                0.0
            };
            r.weather_shelter = 0.0;
            r.set_visible(self.slots.bulb, false);
        } else {
            // Headless hook: pin the commander buttoned-up interior view every frame (the screenshot
            // harness forces the gunner optic after build, so re-assert this over it). No-op in play.
            if self.force_buttoned {
                self.station = Station::Commander;
                self.commander_outside = false;
            }
            if self.force_driver {
                self.station = Station::Driver; // headless: the visor-slit view
            }

            // Advance the station-switch scramble. While it runs NO station control is honoured and no
            // emergency activity may start — you are physically clambering between seats.
            if self.switch_t > 0.0 {
                self.switch_t = (self.switch_t - dt).max(0.0);
            }

            // Emergency activities (F = extinguisher, R = field repair) pull BOTH hands off any station.
            // Run first so `activity` is set before the station gate reads it.
            self.update_activities(&activity_input, dt);

            // ── ENGINE SHUTOFF (Z): kill the Maybach for the silent watch — the scout loop is
            // shut down → slip out → mark → remount → crank → execute. The crank takes a beat and
            // the START is a loud event (start_noise_t widens the envelope) — that's the price.
            let ignition = driver_input.is_some_and(|i| i.flag(crate::lan::F_IGNITION))
                || commander_input.is_some_and(|i| i.flag(crate::lan::F_IGNITION))
                || (coop.is_none() && input.ignition);
            let ign_edge = ignition && !self.prev_ignition;
            self.prev_ignition = ignition;
            if ign_edge
                && matches!(self.mode, Mode::Tank)
                && self.player.alive
                && self.module_ok(ModuleKind::Engine)
            {
                if self.engine_on {
                    self.engine_on = false;
                    self.engine_crank_t = 0.0;
                    if let Some(a) = &self.audio {
                        a.play_at_volume(crate::audio::Sfx::EngineDeath, 0.4); // the wind-down IS the shutoff
                    }
                    println!("[kessel] MOTOR AUS — silent watch (Z cranks it back up)");
                } else if self.engine_crank_t <= 0.0 {
                    self.engine_crank_t = 1.8;
                    println!("[kessel] ANLASSER — cranking…");
                }
            }
            if self.engine_crank_t > 0.0 {
                self.engine_crank_t -= dt;
                if self.engine_crank_t <= 0.0 {
                    self.engine_on = true;
                    self.start_noise_t = 5.0; // the cold Maybach barks — everyone nearby hears it
                    println!("[kessel] MOTOR AN — the whole steppe heard that");
                }
            }
            self.start_noise_t = (self.start_noise_t - dt).max(0.0);

            // Derived mirror: `gunner_optic` (still read by MG routing, optic-blast, recoil, the headless
            // sites) is true iff the player is posted at the gunner's sight.
            self.gunner_optic = matches!(self.station, Station::Gunner);

            // ── Station gate: only the ACTIVELY-manned seat's continuous controls are live, and only
            // while not mid-switch and not mid-activity. The existing DAMAGE gates stack on top (a downed
            // driver / wrecked track still immobilises; a downed gunner still freezes the lay). Space is
            // the ONE context button — at the gunner's seat it FIRES, at the breech it services the reload.
            if self.player.alive {
                let acting =
                    (self.switch_t <= 0.0 && self.activity.is_none()).then_some(self.station);
                let at = |s: Station| acting == Some(s);

                // THE VIEW MODEL (playtest 2): views are instruments, not seats — you can DRIVE from
                // every one of them (inside / periscope / cupola / sights); the gun is laid and fired
                // only through the SIGHTS; the INSIDE view is where the hands-on work happens (the
                // mouse reload below, extinguisher, repair, the map). Crew man-down rules return later.
                let mobile = self.engine_on
                    && self.module_ok(ModuleKind::Engine)
                    && self.module_ok(ModuleKind::Transmission)
                    && self.module_ok(ModuleKind::LeftTrack)
                    && self.module_ok(ModuleKind::RightTrack);
                // INSIDE (Loader) you are ON YOUR FEET: WASD walks the body around the fighting
                // compartment instead of driving the tank (you can't drive from the loader's boots).
                let afoot_inside = at(Station::Loader);
                let drive_cmd = driver_input.map_or(input.drive, |i| i.drive);
                let steer_cmd = driver_input.map_or(input.steer, |i| i.steer);
                let (drive, steer) =
                    if mobile && (coop.is_some() || (acting.is_some() && !afoot_inside)) {
                        (drive_cmd, steer_cmd)
                    } else {
                        (0.0, 0.0)
                    };
                // The engine-audio rev reads THIS (gated) command, not the raw key — holding W while
                // walking the interior must not roar the engine (it read as "driving from inside").
                self.drive_cmd = drive;
                // (the interior walk itself lives in the Loader camera arm below, where the true look
                // direction is known — real FPS movement, not a rail)
                let lay_ok = coop.is_some() || at(Station::Gunner);
                // ── KB+MOUSE GUN LAY: in the sights the MOUSE writes the desired lay; the turret and
                // gun chase it at their real rates (the Tiger's slow hydraulic traverse IS the game —
                // you flick to the target and wait for the turret to arrive). Arrows still nudge.
                // Zoomed sight = finer mouse for the precision lay. Off-station the target tracks the
                // actual lay so re-entering the sights never jumps.
                if lay_ok {
                    // Sens ÷5 (playtest: full traverse speed arrived within a few mm of mouse — no
                    // room for a fine lay). MINUS on look_dx: turret-yaw+ swings toward +X, which is
                    // screen-LEFT for a camera looking +Z — the raw += read as inverted aim. (The
                    // arrow keys were already sign-compensated; the mouse never was — it was dead in
                    // windowed play until the input fix, so nobody had ever felt it.)
                    let sens = [0.00044, 0.00018, 0.00009][self.sights_mag.min(2) as usize];
                    let look_dx = gunner_input.map_or(input.look_dx, |i| i.look_dx);
                    let look_dy = gunner_input.map_or(input.look_dy, |i| i.look_dy);
                    let traverse_cmd = gunner_input.map_or(input.traverse, |i| i.traverse);
                    let elevate_cmd = gunner_input.map_or(input.elevate, |i| i.elevate);
                    self.lay_yaw += -look_dx * sens + traverse_cmd * 0.9 * dt;
                    self.lay_pitch = (self.lay_pitch - look_dy * sens + elevate_cmd * 0.25 * dt)
                        .clamp(-0.139, 0.261);
                } else {
                    self.lay_yaw = self.player.turret_yaw;
                    self.lay_pitch = self.player.gun_pitch;
                }
                let chase_yaw = {
                    let mut d = (self.lay_yaw - self.player.turret_yaw) % std::f32::consts::TAU;
                    if d > std::f32::consts::PI {
                        d -= std::f32::consts::TAU;
                    }
                    if d < -std::f32::consts::PI {
                        d += std::f32::consts::TAU;
                    }
                    (d * 8.0).clamp(-1.0, 1.0)
                };
                let chase_pitch =
                    ((self.lay_pitch - self.player.gun_pitch) * 10.0).clamp(-1.0, 1.0);
                let traverse = if lay_ok && self.module_ok(ModuleKind::TurretTraverse) {
                    chase_yaw
                } else {
                    0.0
                };
                let elevate = if lay_ok && self.module_ok(ModuleKind::MainGun) {
                    chase_pitch
                } else {
                    0.0
                };
                self.player
                    .update(drive, steer, traverse, elevate, dt, &self.hf);
                self.resolve_tank_crush(dt, r); // bow vs standing walls: burst / grind / hard stop

                // Fire: LMB while in the sights (the mouse IS the gun now). Single-crew — Space no longer
                // fires (that was a multi-crew relic); Space is the cupola climb now.
                // EDGE-triggered + click-guarded: the LMB press that clicked INTO the sights (station
                // anchor still hovered from last frame) must not also pull the trigger, and a button
                // held from before the lay/travel-lock gates opened must not auto-fire the cannon the
                // instant they do — only a FRESH press taken at the sights fires. (Mirrors the MG's
                // `click_hover` guard below.)
                let main_trigger = gunner_input.is_some_and(|i| i.flag(crate::lan::F_FIRE_MAIN))
                    || (coop.is_none() && input.mg_coax);
                let lmb_edge = main_trigger && !self.prev_lmb;
                let click_consumed =
                    coop.is_none() && (self.hover_station.is_some() || self.hover_hatch);
                if lay_ok && lmb_edge && !click_consumed && self.player_can_fire() {
                    self.fire(r);
                    self.player.fired();
                    self.on_fired();
                }

                // INSIDE: the manual reload, by hand. E equips a specific round; LMB grips the lever
                // or commits the aligned forward shove that rams the round home.
                if at(Station::Loader) {
                    // ── THE LEVER: hold LMB on it and move the mouse WITH the handle: down opens,
                    // up closes. Head-look locks while gripped so the lever—not the camera—moves.
                    // the detents. Hands must be free — you can't work the lever holding a round.
                    self.lever_pulling = input.mg_coax
                        && self.hover == Some(Hotspot::GunLever)
                        && self.carry_mesh.is_none()
                        && self.carried_casing.is_none();
                    if self.lever_pulling {
                        let before = self.lever_open;
                        self.lever_open =
                            (self.lever_open + input.look_dy * 0.0045).clamp(0.0, 1.0);
                        let crossed_open = before < LEVER_OPEN && self.lever_open >= LEVER_OPEN;
                        let crossed_shut = before > LEVER_SHUT && self.lever_open <= LEVER_SHUT;
                        if crossed_open || crossed_shut {
                            if let Some(a) = &self.audio {
                                a.play_at_volume(crate::audio::Sfx::BreechClack, 0.7);
                            }
                            if crossed_open {
                                // Exactly one vent per fired case. Re-working an empty block does
                                // not conjure repeated orange puffs into the compartment.
                                if self.bore_smoke_pending {
                                    self.interior_haze = (self.interior_haze + 0.22).min(1.0);
                                    self.bore_smoke_pending = false;
                                }
                            }
                        }
                    }
                    // ── THE HOT CASE: after a shot it SITS in the breech. Hold LMB on it to work it
                    // out (CASING_PULL_TIME of grip), then it rides your hands until you let go —
                    // the toss inherits the carry swing and the case clatters off the basket floor.
                    if matches!(self.breech, Breech::Spent)
                        && self.carried_casing.is_none()
                        && self.carry_mesh.is_none()
                        && self.lever_open >= LEVER_OPEN
                    {
                        if input.mg_coax && self.hover == Some(Hotspot::SpentCasing) {
                            self.casing_pull = (self.casing_pull + dt / CASING_PULL_TIME).min(1.0);
                            if self.casing_pull >= 1.0 {
                                self.casing_pull = 0.0;
                                self.breech = Breech::Empty;
                                let tm = self.player.turret_model();
                                self.carry_pos = tm.transform_point3(Vec3::new(0.1, 1.88, 0.05));
                                self.carry_vel = Vec3::ZERO;
                                self.carry_dir = r.camera.forward();
                                let k = self.spawn_casing_from(self.carry_pos, Vec3::ZERO);
                                self.casings[k].carried = true;
                                self.carried_casing = Some(k);
                                if let Some(a) = &self.audio {
                                    a.play_at_volume(crate::audio::Sfx::BrassClatter, 0.55);
                                }
                            }
                        } else {
                            self.casing_pull = (self.casing_pull - dt * 3.0).max(0.0);
                        }
                    }
                    if let Some(k) = self.carried_casing {
                        self.update_carry(r, dt, CARRY_K_CASE, CARRY_D_CASE);
                        self.casings[k].t = 30.0; // no retiring in the hands
                        if !input.mg_coax {
                            // TOSS — the case leaves with the hands' momentum.
                            let vel = self.carry_vel
                                + r.camera.forward() * 1.4
                                + Vec3::new(0.0, 0.6, 0.0);
                            self.casings[k].carried = false;
                            self.casings[k].pos = self.carry_pos;
                            self.casings[k].vel = vel;
                            self.carried_casing = None;
                        }
                    }

                    // ── THE ROUND: E-equip a SPECIFIC shell off the rack (or a dropped one),
                    // carry its whole weight to the breech, and only there does the ram engage.
                    if !matches!(self.breech, Breech::Loaded) && self.carried_casing.is_none() {
                        // Track kit: LMB on the toolbox takes it (the fix itself happens outside).
                        if input.mg_coax
                            && !self.carrying_track_kit
                            && self.hover == Some(Hotspot::RepairKit)
                            && (!self.module_ok(ModuleKind::LeftTrack)
                                || !self.module_ok(ModuleKind::RightTrack))
                        {
                            self.carrying_track_kit = true;
                            self.track_repair_phase = TrackRepairPhase::GrabEnd;
                            self.track_pull = 0.0;
                            self.track_hits = 0;
                            println!(
                                "[kessel] KETTENWERKZEUG genommen — raus, loses Kettenende greifen"
                            );
                        }
                        // E equips a round into the loader's hands. LMB is reserved for the
                        // embodied work after that: lever, hot case, and the final forward ram.
                        if !self.grab && self.carry_mesh.is_some() {
                            self.grab = false;
                            if self.ram < 0.35 {
                                // Let go mid-carry: the round DROPS out of your arms with its swing.
                                let inv = self.player.turret_model().inverse();
                                let li = match self.carry_mesh {
                                    Some(CarriedMesh::LooseShell(i)) => Some(i),
                                    _ => self.loose.iter().position(|sh| !sh.active),
                                };
                                if let Some(i) = li {
                                    let sh = &mut self.loose[i];
                                    sh.active = true;
                                    sh.nature_he = self.he_loaded;
                                    sh.pos = inv.transform_point3(self.carry_pos);
                                    sh.vel = inv.transform_vector3(self.carry_vel * 0.6)
                                        + Vec3::new(0.0, -0.4, 0.0);
                                    sh.axis = hash01(self.kill_tick.wrapping_mul(97))
                                        * std::f32::consts::TAU;
                                    sh.spin = 0.0;
                                    self.ram = 0.0;
                                    // A rack shell that hits the floor is a floor shell now.
                                    if let Some(CarriedMesh::RackShell(ri)) = self.carry_mesh {
                                        let _ = ri; // its rack slot stays empty; the loose mesh takes over
                                    }
                                    if let Some(a) = &self.audio {
                                        a.play_at_volume(crate::audio::Sfx::BrassClatter, 0.4);
                                    }
                                }
                                self.carry_mesh = None;
                            }
                            // ram >= 0.35: it's in the tray — sags back slowly (RAM_DECAY below).
                        }
                        if self.grab {
                            self.breech = Breech::Empty; // hands on the round — the breech stands open
                            self.update_carry(r, dt, CARRY_K_ROUND, CARRY_D_ROUND);
                            // The ram engages ONLY with the round bodily at the tray mouth — no
                            // tray magnetism from across the basket. Carry it there first.
                            // Line the crosshair/carry hand up with the open tray, hold LMB, then
                            // SHOVE the mouse forward (fast upward screen motion). The first committed
                            // shove locks the view until the round seats or the grip is released.
                            let mouth = self.player.turret_model().transform_point3(TRAY_MOUTH);
                            let line = (mouth - r.camera.pos).normalize_or_zero();
                            let crosshair_lined = r.camera.forward().dot(line) > 0.965;
                            if self.carry_pos.distance(mouth) < RAM_REACH
                                && crosshair_lined
                                && self.lever_open >= LEVER_OPEN
                            {
                                let shove = (-input.look_dy - 8.0).max(0.0);
                                if input.mg_coax && shove > 0.0 {
                                    self.ram = (self.ram + shove * 0.014).clamp(0.0, 1.0);
                                } else if !input.mg_coax {
                                    self.ram = (self.ram - RAM_DECAY * dt).max(0.0);
                                }
                                if self.ram >= 1.0 {
                                    // RAMMED. The gun is up.
                                    self.breech = Breech::Loaded;
                                    self.ram = 0.0;
                                    self.grab = false;
                                    self.carry_mesh = None;
                                    if let Some(a) = &self.audio {
                                        a.play_at_volume(crate::audio::Sfx::BreechClack, 0.9);
                                    }
                                }
                            } else {
                                self.ram = (self.ram - RAM_DECAY * dt).max(0.0);
                            }
                        }
                    }
                } else if self.grab || self.carried_casing.is_some() {
                    // Left the station with full hands: the round/case drops where you stood.
                    if let Some(k) = self.carried_casing.take() {
                        self.casings[k].carried = false;
                        self.casings[k].vel = Vec3::new(0.0, -0.5, 0.0);
                    }
                    if self.grab {
                        self.grab = false;
                        let inv = self.player.turret_model().inverse();
                        if let Some(i) = match self.carry_mesh {
                            Some(CarriedMesh::LooseShell(i)) => Some(i),
                            _ => self.loose.iter().position(|sh| !sh.active),
                        } {
                            self.loose[i].active = true;
                            self.loose[i].nature_he = self.he_loaded;
                            self.loose[i].pos = inv.transform_point3(self.carry_pos);
                            self.loose[i].vel = Vec3::new(0.0, -0.4, 0.0);
                        }
                        self.carry_mesh = None;
                    }
                }

                // In co-op the loader works a network-stable version of this exact breech state
                // machine: hold the service control to pull/eject, then repeated ram strokes. An
                // empty seat is AI-assisted so two- and three-person crews never hard-lock.
                if let (Some(frame), Some(loader)) = (coop, loader_input) {
                    let servicing = loader.flag(crate::lan::F_SERVICE_BREECH)
                        || (!frame.occupied(crate::lan::LanRole::Loader)
                            && !frame.gunner_is_loader());
                    let local_manual_loader = input.coop_host
                        && at(Station::Loader)
                        && self.lan_local_role.is_some_and(|role| {
                            role == crate::lan::LanRole::Loader
                                || (role == crate::lan::LanRole::Gunner && frame.gunner_is_loader())
                        });
                    if servicing && !local_manual_loader {
                        self.coop_reload_clock += dt;
                        let pulse = self.coop_reload_clock >= 0.22;
                        if pulse {
                            self.coop_reload_clock = 0.0;
                        }
                        self.service_breech(pulse, dt);
                    } else {
                        self.coop_reload_clock = 0.0;
                    }
                }

                // A half-driven round bleeds back out if you leave the breech / stop mashing.
                if coop.is_none()
                    && matches!(self.breech, Breech::Empty)
                    && acting != Some(Station::Loader)
                {
                    self.ram = (self.ram - RAM_DECAY * dt).max(0.0);
                }
            }
            self.prev_space = input.fire;
            self.prev_lmb = gunner_input.is_some_and(|i| i.flag(crate::lan::F_FIRE_MAIN))
                || (coop.is_none() && input.mg_coax);

            // ── Coaxial + hull MG34 ────────────────────────────────────────────────────────────────
            // ONE held trigger (Left Shift → TankInput::mg_coax), routed by station: at the GUNNER it fires
            // the COAXIAL MG34 (main-gun axis); at the COMMANDER it chatters the HULL/BOW MG34. You can't
            // work an MG while clambering between seats, mid-reload at another station, or mid-activity.
            let mg_manned = coop.is_some()
                || (self.switch_t <= 0.0
                    && self.activity.is_none()
                    && matches!(self.station, Station::Gunner | Station::Commander));
            // KB+MOUSE: at the sights LMB is the MAIN GUN and Ctrl EXITS the sights, so the coax
            // rides held C there; the commander's hull MG keeps LMB (nothing else competes up there).
            let coop_coax = gunner_input.is_some_and(|i| i.flag(crate::lan::F_FIRE_COAX));
            let coop_hull = radio_input.is_some_and(|i| i.flag(crate::lan::F_FIRE_HULL));
            let mg_trigger = if coop.is_some() {
                coop_coax || coop_hull
            } else {
                match self.station {
                    Station::Gunner => input.coax,
                    // The commander's MG only with your HEAD OUT of the hatch (playtest: LMB while
                    // buttoned kept chattering it during interior clicks — and there's nothing an MG
                    // solves from inside a closed tank anyway).
                    Station::Commander => input.mg_coax && self.commander_outside,
                    _ => false, // no MG from the interior walk or the driver's visor
                }
            };
            // Hovering a clickable (station anchor / hatch) turns LMB into the CLICK — don't also
            // rattle off an MG burst on the same press.
            let click_hover = coop.is_none() && (self.hover_station.is_some() || self.hover_hatch);
            self.update_mg(
                dt,
                mg_trigger && mg_manned && !click_hover,
                if coop.is_some() {
                    coop_coax
                } else {
                    self.gunner_optic
                },
            );

            // enemy return-fire AI: track the player and shoot when laid + reloaded.
            self.enemy_ai(dt);

            // Hide the on-foot Luger while riding in the tank.
            r.set_visible(self.slots.luger, false);
            r.set_visible(self.slots.luger_flash, false);
            r.set_visible(self.slots.hammer, false);
            // Freeze + hide the infantry while buttoned up (they only exist for the dismounted loop).
            for s in &self.soldiers {
                r.set_visible(s.body_slot, false);
                r.set_visible(s.rifle_slot, false);
            }
            for s in self.slots.inf_flash {
                r.set_visible(s, false);
            }

            // ── Per-station first-person camera (four seats; lerped across a station switch so you SEE
            // the scramble). GUNNER: down the sight. LOADER: at the breech, hands busy (no free-look).
            // DRIVER: hull-forward through the visor slit. COMMANDER: cupola head-look + binoculars. ──
            let (fwd, fov, reticle): (Vec3, f32, bool) = match self.station {
                Station::Gunner => {
                    let (_mpos, mdir) = self.player.muzzle();
                    // A destroyed gunner sight (TZF) blinds the optic — no reticle to lay with.
                    let fov = [25f32, 12.0, 6.0][self.sights_mag.min(2) as usize].to_radians();
                    (mdir, fov, self.module_ok(ModuleKind::GunnerSight))
                }
                Station::Loader => {
                    // INSIDE: a real first-person body. Full 360° mouselook (frozen only while your
                    // hands are on a round — the mouse is lifting the shell then), WASD walks relative
                    // to where you LOOK, clamped to the TURRET FLOOR footprint with the breech as the
                    // one obstacle. Not a rail.
                    const SENS: f32 = 0.0022;
                    let ram_locked = self.grab && input.mg_coax && self.ram > 0.0;
                    if !self.lever_pulling && !ram_locked {
                        self.head_yaw += input.look_dx * SENS; // unclamped — turn all the way around
                        self.head_pitch = (self.head_pitch - input.look_dy * SENS).clamp(-1.2, 1.0);
                    }
                    let tf = self.player.turret_forward();
                    let cam_yaw = tf.x.atan2(-tf.z) + self.head_yaw;
                    // walk in the ground plane of the LOOK direction (world), then into turret space
                    let f_w = Vec3::new(cam_yaw.sin(), 0.0, -cam_yaw.cos());
                    let r_w = Vec3::new(cam_yaw.cos(), 0.0, cam_yaw.sin());
                    let wish_w = (f_w * input.drive + r_w * -input.steer).normalize_or_zero();
                    if wish_w.length_squared() > 0.01 {
                        let ang = self.player.heading + self.player.turret_yaw;
                        let wish_l = glam::Quat::from_rotation_y(-ang) * wish_w;
                        let next = self.walk_pos + wish_l * 1.15 * dt;
                        self.walk_pos = clamp_interior_walk(self.walk_pos, next);
                    }
                    // Binoculars work here too: the B toggle is reachable from the interior walk and
                    // already draws the double-circle mask. Use the same service-glass field as every
                    // other station so changing bodies never changes the instrument's magnification.
                    let fov = if input.binoculars {
                        BINOCULAR_FOV_DEG.to_radians()
                    } else {
                        62f32.to_radians()
                    };
                    (tf, fov, false)
                }
                Station::Driver => (self.player.forward(), 50f32.to_radians(), false),
                Station::Radioman => (self.player.forward(), 54f32.to_radians(), false),
                Station::Commander => {
                    // PERISCOPE (buttoned, safe) / CUPOLA HEAD-OUT (exposed): free look with the mouse,
                    // and the ARROW KEYS whip the view around fast — the "glance over your shoulder"
                    // the user asked for. Head-out gets the widest field; the periscope is nearly as
                    // wide and turns just as fast, but keeps your skull behind armor.
                    const SENS: f32 = 0.0022;
                    const ARROW_LOOK: f32 = 2.6; // rad/s — fast scan on the arrow keys
                    self.head_yaw += input.look_dx * SENS - input.traverse * ARROW_LOOK * dt;
                    self.head_pitch = (self.head_pitch - input.look_dy * SENS
                        + input.elevate * ARROW_LOOK * 0.5 * dt)
                        .clamp(-0.5, 0.7);
                    let fov = if input.binoculars {
                        BINOCULAR_FOV_DEG.to_radians()
                    } else if self.commander_outside {
                        72f32.to_radians()
                    } else {
                        64f32.to_radians()
                    };
                    (self.player.turret_forward(), fov, input.binoculars)
                }
            };
            r.camera.set_forward(fwd);
            r.camera.fov_y = fov;
            r.reticle = reticle;
            // Optic mask: the eye pressed to the eyepiece. TZF = circle (reticle inside), driver's
            // visor = letterbox slit, binoculars = double-circle. Commander slits/head-out = unmasked
            // (the cut cupola ring does the framing physically).
            r.optic_mask = match self.station {
                Station::Gunner => 1.0,
                Station::Driver | Station::Radioman => 2.0,
                _ if input.binoculars => 3.0,
                _ => 0.0,
            };
            if matches!(self.station, Station::Commander) {
                r.camera.yaw += self.head_yaw;
                r.camera.pitch = self.head_pitch - 0.10; // slight default downtilt over the deck
            }
            if matches!(self.station, Station::Loader) {
                r.camera.yaw += self.head_yaw;
                r.camera.pitch = (r.camera.pitch + self.head_pitch).clamp(-1.2, 1.2);
            }
            // ── INSIDE hotspots: what the crosshair rests on (mouse-to-interact). Computed here where
            // the final camera is known; the held-action gates read it next frame (1-frame latency is
            // imperceptible at 60 Hz). The rack glints when looked at — the "you can grab this" tell.
            self.hover = None;
            self.hover_label = None;
            // Mode-gated: the INSIDE hotspots exist only while you're actually IN the tank — the
            // station freezes on dismount, and walking past the hull outside was popping the
            // breech-reload prompts through the armor.
            if matches!(self.mode, Mode::Tank)
                && matches!(self.station, Station::Loader)
                && self.switch_t <= 0.0
            {
                let tm = self.player.turret_model();
                let hm = self.player.hull_model();
                let eye = r.camera.pos;
                let look = r.camera.forward();
                let track_down = !self.module_ok(ModuleKind::LeftTrack)
                    || !self.module_ok(ModuleKind::RightTrack);
                let spots2: [(Hotspot, Vec3, &str); 2] = [
                    (
                        Hotspot::Extinguisher,
                        EXT_MOUNT_MID,
                        "FEUERLÖSCHER: LMB halten",
                    ),
                    (
                        Hotspot::RepairKit,
                        TOOLBOX_MOUNT,
                        if track_down {
                            "KETTENWERKZEUG NEHMEN [LMB]"
                        } else {
                            "WERKZEUG: LMB halten (Reparatur)"
                        },
                    ),
                ];
                let mut best = 0.94f32;
                for (spot, local, label) in spots2 {
                    let wp = tm.transform_point3(local);
                    let to = wp - eye;
                    if to.length() > 2.6 {
                        continue;
                    }
                    let d = to.normalize_or_zero().dot(look);
                    if d > best {
                        best = d;
                        self.hover = Some(spot);
                        self.hover_label = Some(label);
                    }
                }
                // The RACK, round by round: look at a specific stowed shell to grab THAT shell.
                if !matches!(self.breech, Breech::Loaded)
                    && self.carry_mesh.is_none()
                    && self.carried_casing.is_none()
                {
                    for (i, (frame, pos, _q)) in crate::interior_rig::SHELL_SLOTS.iter().enumerate()
                    {
                        if self.slot_taken[i] {
                            continue;
                        }
                        let m = if *frame == crate::interior_rig::Frame::Hull {
                            hm
                        } else {
                            tm
                        };
                        let wp = m.transform_point3(*pos);
                        let to = wp - eye;
                        if to.length() > 1.9 {
                            continue;
                        }
                        let d = to.normalize_or_zero().dot(look);
                        if d > best {
                            best = d;
                            self.hover = Some(Hotspot::RackSlot(i));
                            self.hover_label = Some("GRANATE AUSRÜSTEN [E]");
                        }
                    }
                }
                // The BREECH LEVER: the gate of the whole cycle. Contextual prompt by state.
                {
                    let wp = self.player.gun_model().transform_point3(GUNLEVER_POS);
                    let to = wp - eye;
                    if std::env::var("KESSEL_LEVER_DBG").is_ok() {
                        println!(
                            "[lever-dbg] dist={:.2} dot={:.2} lever={:.2} breech={:?}",
                            to.length(),
                            to.normalize_or_zero().dot(look),
                            self.lever_open,
                            matches!(self.breech, Breech::Loaded)
                        );
                    }
                    if to.length() < 1.8 && to.normalize_or_zero().dot(look) > 0.88 {
                        let label = if self.lever_open < LEVER_OPEN {
                            Some(if matches!(self.breech, Breech::Loaded) {
                                "VERSCHLUSS ÖFFNEN — GRANATE ENTLADEN"
                            } else {
                                "VERSCHLUSSHEBEL ÖFFNEN [LMB + Maus abwärts]"
                            })
                        } else if self.lever_open > LEVER_SHUT
                            && matches!(self.breech, Breech::Loaded)
                        {
                            Some("VERSCHLUSS SCHLIESSEN [LMB + Maus aufwärts]")
                        } else if self.lever_open > LEVER_SHUT {
                            Some("VERSCHLUSSHEBEL [LMB + Maus]")
                        } else {
                            None
                        };
                        if let Some(l) = label {
                            self.hover = Some(Hotspot::GunLever);
                            self.hover_label = Some(l);
                        }
                    }
                }
                // The HOT CASE in the spent breech: pull it before anything can be loaded.
                if matches!(self.breech, Breech::Spent) && self.carried_casing.is_none() {
                    let wp = tm.transform_point3(Vec3::new(0.1, 1.88, 0.15));
                    let to = wp - eye;
                    if to.length() < 2.0 && to.normalize_or_zero().dot(look) > 0.90 {
                        self.hover = Some(Hotspot::SpentCasing);
                        self.hover_label = Some("HEISSE HÜLSE ZIEHEN [LMB halten]");
                    }
                }
                // A live round is not an inventory flag: open the block, look at the exposed base,
                // and E physically pulls it back into the loader's hands so ammunition can change.
                if matches!(self.breech, Breech::Loaded)
                    && self.lever_open >= LEVER_OPEN
                    && self.carry_mesh.is_none()
                {
                    let wp = tm.transform_point3(crate::interior_rig::ROUND_READY);
                    let to = wp - eye;
                    if to.length() < 2.0 && to.normalize_or_zero().dot(look) > 0.90 {
                        self.hover = Some(Hotspot::LoadedRound);
                        self.hover_label = Some("GELADENE GRANATE ZIEHEN [E]");
                    }
                }
                // Dropped rounds: look at one on the floor to scoop it back up.
                for (i, sh) in self.loose.iter().enumerate() {
                    if !sh.active {
                        continue;
                    }
                    let wp = tm.transform_point3(sh.pos);
                    let to = wp - eye;
                    if to.length() > 2.2 {
                        continue;
                    }
                    let d = to.normalize_or_zero().dot(look);
                    if d > best {
                        best = d;
                        self.hover = Some(Hotspot::FloorShell(i));
                        self.hover_label = Some("GRANATE AUFHEBEN [E]");
                    }
                }
                // Hands full: the only prompts that matter are the carry itself.
                if self.carried_casing.is_some() {
                    self.hover = None;
                    self.hover_label = Some("HÜLSE — loslassen: WEGWERFEN");
                } else if self.grab {
                    self.hover = None;
                    let mouth = tm.transform_point3(TRAY_MOUTH);
                    // The prompt tells the TRUTH about the gate: at the mouth with the lever shut,
                    // say so — the old label said "pull up" while the ram silently ignored you.
                    let lined = r.camera.forward().dot((mouth - eye).normalize_or_zero()) > 0.965;
                    self.hover_label = Some(if self.carry_pos.distance(mouth) >= RAM_REACH {
                        "ZUM VERSCHLUSS TRAGEN"
                    } else if !lined {
                        "KREUZHAAR AUF VERSCHLUSS AUSRICHTEN"
                    } else if self.lever_open < LEVER_OPEN {
                        "VERSCHLUSS ZU — Hebel öffnen!"
                    } else {
                        "LMB HALTEN — MAUS SCHNELL VORWÄRTS RAMMEN"
                    });
                }
                // (No emissive glint on the hovered shell — set_emissive without emissive_own lands on
                // the gunfire-orange FX path and painted the round RED; the outline rim is the tell now.)
            }
            // Eye position — blended from the seat you're leaving while the switch runs.
            let eye_to = self.station_eye(self.station);
            r.camera.pos = if self.switch_t > 0.0 && self.switch_dur > 0.0 {
                let b = smoothstep(0.0, 1.0, 1.0 - self.switch_t / self.switch_dur);
                self.station_eye(self.switch_from).lerp(eye_to, b)
            } else {
                eye_to
            };

            // Ambience: the target soundscape is deliberately near-silent and tense — just ENGINE + WIND
            // as the bed, so the sounds of incoming/outgoing FIRE (the speed-of-sound queue below) are
            // what the player actually hears. Interior hull-hum whenever buttoned up (gunner optic, or
            // commander hatch closed) at a subtle level; full exterior wind when exposed. Crossfades over
            // ~1.4s, no hard cut. (The old distress drone + drive-stinger cinematic swells were stripped:
            // they read as a jarring ringing/whine over the quiet — removed on purpose.)
            // Buttoned up = inside the tank: any turret/hull station with the hatch shut. Only an exposed
            // Commander (hatch open) or a hatched-out Driver (re-setting a track) is open to the air.
            // Head-out only counts if the hatch is actually OPEN (you can't peer through a closed lid).
            if !self.hatch_open {
                self.commander_outside = false;
            }
            let exposed = (matches!(self.station, Station::Commander)
                && self.commander_outside
                && self.hatch_open)
                || (matches!(self.station, Station::Driver) && self.driver_hatch);
            let buttoned_up = !exposed;

            // Interior warmth: 1 while buttoned up (the warm tungsten fill washes the near steel), 0 when
            // exposed. Set directly (not time-eased) so it snaps with the view change — matching the hatch
            // toggle's instant FOV change, and so a single headless screenshot frame captures it fully.
            // Pushed to the renderer's interior_light uniform (mesh.wgsl's warm interior fill).
            self.interior_light = if buttoned_up { 1.0 } else { 0.0 };
            r.interior_light = self.interior_light;
            // Buttoned up, every ray starts INSIDE the player's own occluder box — the traced
            // shadow/AO would just uniformly crush the compartment. Fade it out with interiority.
            r.rt_mix = 1.0 - self.interior_light;
            // The smoke veil: only when buttoned up INSIDE, and NOT down the gunner's sight — your eye
            // is pressed to sealed optic glass there, the compartment smoke is behind you, so the sight
            // picture (and its reticle) stays clean. Applies in the commander/loader/driver views where
            // you're actually looking around the smoky compartment. Head-out/on-foot: no veil.
            r.interior_haze = if self.gunner_optic {
                0.0
            } else {
                (0.06 + self.interior_haze * 0.88) * self.interior_light
            };
            // Snow exists in the WORLD even while the crew is behind armor. Shelter controls which
            // depth-visible layers a view may see: nearby steel stays clean, but an open hatch or
            // optic can reveal distant flakes. Spring maps skip the renderer pass altogether.
            r.precipitation = if self.act.season == crate::world::Season::Winter {
                1.0
            } else {
                0.0
            };
            r.weather_shelter = tank_weather_shelter(self.station, exposed, self.hatch_open);

            // The physical warm bulb: a single dim tungsten lamp under the turret roof near the crew, lit
            // only when buttoned up. Turret-local so it rides with the crew as the turret traverses; a gentle
            // flicker keeps it alive without reading as fire. Own-colour emissive (amber), marked in build.
            {
                let bulb_local = Vec3::new(0.22, 2.02, 0.48); // turret-local: under the roof over the breech, in the crew's forward view
                let pos = self.player.turret_model().transform_point3(bulb_local);
                let flick = 0.90 + 0.10 * (self.stealth.t * 7.0).sin();
                // The legacy bulb BOX is retired — the real lamp set (interior_rig LAMPS) lights the
                // compartment now, and the emissive amber cube read as a floating "red block".
                r.set_visible(self.slots.bulb, false);
                r.set_emissive(self.slots.bulb, 2.2 * self.interior_light * flick);
                r.set_model(
                    self.slots.bulb,
                    Mat4::from_scale_rotation_translation(Vec3::splat(1.0), Quat::IDENTITY, pos),
                );
            }

            // ── Cupola hatch: ease the swing, render the lid on its side hinge (rotates about engine-Y,
            // the vertical, swinging horizontally), and pulse it when the crosshair rests on it (the
            // hover tell for interactables — Stage 2 groundwork). ──
            let want = if self.hatch_open { 1.0 } else { 0.0 };
            self.hatch_t += (want - self.hatch_t).clamp(-3.0 * dt, 3.0 * dt);
            // Turret-local hinge axis from the bake (`cupola hatch pivot`, engine frame): on the
            // cupola drum's right rim — the baked assembly's origin sits ON this axis, so translate
            // here + rotate about Y and the disc swings around its real hinge post.
            let hinge = Vec3::new(0.963, 2.614, -0.333);
            // TWO-STEP (the real Tiger cupola): LIFT 0.15 m up the hinge post, THEN swing ~110° aside
            // toward the rear (past 90° so it rests clearly clear of the well). The phases DON'T
            // overlap: the lid fully rises before it starts to turn. Both read the single eased
            // `hatch_t`, so CLOSING reverses for free — swing back over first (t 1→0.5), then drop
            // down the post (t 0.5→0).
            let t = self.hatch_t;
            let lift = smoothstep(0.0, 0.5, t) * 0.15; // phase 1: rise on the post
            let swing = smoothstep(0.5, 1.0, t) * (-110f32).to_radians(); // phase 2: swing aside (aft)
            let hatch_m = self.player.turret_model()
                * Mat4::from_translation(hinge + Vec3::Y * lift)
                * Mat4::from_rotation_y(swing);
            r.set_model(self.slots.hatch, hatch_m);
            r.set_visible(self.slots.hatch, true);
            if std::env::var("KESSEL_HATCH_DEBUG").is_ok() {
                let hw = hatch_m.transform_point3(Vec3::ZERO);
                let tw = self.player.turret_model().transform_point3(Vec3::ZERO);
                println!("[hatch-dbg] turret origin {tw:?} hinge world {hw:?} t={t:.2} lift={lift:.3} swing={swing:.2}");
            }
            // Hover: is the crosshair pointing at the hatch (from a commander view)? Aim at the DISC
            // centre — the baked disc hangs off the hinge toward −x (the cupola hole), not +x.
            let hatch_world = self
                .player
                .turret_model()
                .transform_point3(hinge + Vec3::new(-0.40, 0.0, -0.09));
            let to_hatch = (hatch_world - r.camera.pos).normalize_or_zero();
            // ── INTERACTION PRIORITY at the cupola (2026-07-11 untangle): the LEVER, the CUPOLA
            // station anchor, and the hatch DISC all overlap up there, and the disc used to win
            // every gaze from BOTH stations — so "click into the cupola" was unreachable, and with
            // the lid open the click EJECTED you from the tank (click_hatch_or_exit's old exit arm).
            // Now: 1. lever (the diegetic hatch control, LMB or E, both stations) → 2. station
            // anchors (CUPOLA/GUNNER/BREECH entry) → 3. hatch disc (Commander only — from the
            // loader's pit the lid is not the verb, going UP is).
            let lever_w = self.player.turret_model().transform_point3(HATCHLEVER_POS);
            let to_lever = lever_w - r.camera.pos;
            self.hover_lever = matches!(self.station, Station::Commander | Station::Loader)
            && self.interior_light > 0.02 // the lever only draws (and clicks) on inside views
            && to_lever.length() < 1.9
            && to_lever.normalize_or_zero().dot(r.camera.forward()) > 0.90;
            if self.hover_lever {
                self.hover_label = Some(if self.hatch_open {
                    "LUKENHEBEL — Luke ZU [LMB/E]"
                } else {
                    "LUKENHEBEL — Luke AUF [LMB/E]"
                });
            }
            let hovered = !self.hover_lever
                && matches!(self.station, Station::Commander)
                && to_hatch.dot(r.camera.forward()) > 0.90;
            self.hover_hatch = hovered;
            // Hovered interactable: a STEADY emissive OUTLINE (inverted-hull rim) that the bloom
            // haloes and the interior haze scatters — it glows into the smoke volume. No fill pulse.
            r.set_emissive(self.slots.hatch, 0.0);
            // The tell rides EVERY interactable, not just the hatch: clear the whole candidate set,
            // then light whatever the crosshair actually rests on this frame. The rim glows into the
            // haze and haloes under bloom — the "you can touch this" cue across the compartment.
            r.set_outline(self.slots.hatch, if hovered { 1.0 } else { 0.0 });
            r.set_outline(
                self.slots.hatchlever,
                if self.hover_lever { 1.0 } else { 0.0 },
            );
            r.set_outline(self.slots.gunlever, 0.0);
            r.set_outline(self.slots.chamber_round, 0.0);
            r.set_outline(self.slots.chamber_round_he, 0.0);
            r.set_outline(self.slots.extinguisher, 0.0);
            r.set_outline(self.slots.toolbox, 0.0);
            for i in 0..crate::interior_rig::SHELL_SLOTS.len() {
                r.set_outline(self.slots.shell0 + i, 0.0);
            }
            for &s in self.slots.loose_shell.iter() {
                r.set_outline(s, 0.0);
            }
            for &s in self.slots.loose_shell_he.iter() {
                r.set_outline(s, 0.0);
            }
            match self.hover {
                Some(Hotspot::GunLever) => r.set_outline(self.slots.gunlever, 1.0),
                Some(Hotspot::LoadedRound) => r.set_outline(
                    if self.he_loaded {
                        self.slots.chamber_round_he
                    } else {
                        self.slots.chamber_round
                    },
                    1.0,
                ),
                Some(Hotspot::Extinguisher) => r.set_outline(self.slots.extinguisher, 1.0),
                Some(Hotspot::RepairKit) => r.set_outline(self.slots.toolbox, 1.0),
                Some(Hotspot::RackSlot(i)) => r.set_outline(self.slots.shell0 + i, 1.0),
                Some(Hotspot::FloorShell(i)) => {
                    let shells = if self.loose.get(i).is_some_and(|s| s.nature_he) {
                        &self.slots.loose_shell_he
                    } else {
                        &self.slots.loose_shell
                    };
                    if let Some(&s) = shells.get(i) {
                        r.set_outline(s, 1.0);
                    }
                }
                _ => {}
            }

            // ── Click-to-enter STATIONS (single-crew): rest the crosshair on a station's physical
            // anchor and LMB enters it. Anchors are turret-local engine coords from the interior bake
            // (tiger_interior.json): the TZF gunsight, the breech (loader's post), the cupola well.
            // Replaces the multi-crew C/Shift/Q hotkeys. The baked turret is one merged mesh, so the
            // hover tell is the prompt label (a per-part outline needs the Phase-5 part split).
            self.hover_station = None;
            if !hovered
                && !self.hover_lever
                && matches!(self.station, Station::Commander | Station::Loader)
            {
                let tm = self.player.turret_model();
                let cands: &[(Station, Vec3, &'static str)] = &[
                    (
                        Station::Gunner,
                        Vec3::new(0.536, 1.99, 0.81),
                        "GUNNER SIGHT [click]",
                    ),
                    (
                        Station::Loader,
                        Vec3::new(-0.049, 1.92, 0.31),
                        "BREECH — LOAD [click]",
                    ),
                    (
                        Station::Commander,
                        Vec3::new(0.564, 2.46, -0.42),
                        "CUPOLA [click]",
                    ),
                ];
                let fwd = r.camera.forward();
                let mut best = 0.93f32; // tighter than the hatch: these anchors sit close together
                for (s, local, label) in cands {
                    if *s == self.station {
                        continue;
                    }
                    // From the CUPOLA (Commander) the ONLY move is down into the turret — you can't
                    // reach across into the gunner sight; Q/click the breech drops you inside first.
                    if matches!(self.station, Station::Commander) && *s != Station::Loader {
                        continue;
                    }
                    let anchor = tm.transform_point3(*local);
                    // Must be CLOSE to the physical optic/anchor — no clicking the sight from the far
                    // side of the tank. You lean into it. (Anchors sit ~0.3-1 m from an adjacent seat.)
                    if (anchor - r.camera.pos).length() > 1.35 {
                        continue;
                    }
                    let to = (anchor - r.camera.pos).normalize_or_zero();
                    let d = to.dot(fwd);
                    if d > best {
                        best = d;
                        self.hover_station = Some(*s);
                        self.hover_label = Some(label);
                    }
                }
            }

            // Non-campaign modes retain the simple red-X sandbox exit. A campaign attempt routes through
            // the director below, where reaching the same place cannot bypass live contacts or story.
            if self.campaign.is_none() && !self.act_complete && self.player.alive {
                let p = self.player.center();
                if self.act.reached_goal(glam::Vec2::new(p.x, p.z)) {
                    self.act_complete = true;
                    println!(
                        "[kessel] ZIEL erreicht — act {} '{}' complete",
                        self.act.id, self.act.name
                    );
                }
            }
        } // end Mode::Tank arm

        if self.player.alive {
            self.update_campaign_director(dt);
        }
        self.update_allied_tigers(dt);
        self.update_finale_pressure(dt);

        // Concussion recovery: while deafened, the whole mix ducks to a floor and climbs back over
        // seconds (the tinnitus ring, played once at the hit, emerges from under the ducked world).
        // Master volume is set BEFORE the ambience/engine ramps below so they follow the duck.
        // Runs in BOTH modes — dismounting while deafened must not freeze the mix at the floor.
        if self.concussion_dur > 0.0 {
            self.concussion_t += dt;
            let duck = crate::acoustics::concussion_duck(self.concussion_t, self.concussion_dur);
            if let Some(a) = &mut self.audio {
                a.set_master_volume(duck);
            }
            if self.concussion_t >= self.concussion_dur {
                self.concussion_dur = 0.0;
            }
        }

        // Ambience runs in BOTH modes: on foot you are in the open air (Exterior); in the tank the
        // hatch state decides. Recomputed here (the tank arm's `buttoned_up` is scoped to the arm).
        let buttoned_up = matches!(self.mode, Mode::Tank)
            && !((matches!(self.station, Station::Commander) && self.commander_outside)
                || (matches!(self.station, Station::Driver) && self.driver_hatch));
        if let Some(a) = &self.audio {
            let amb = if buttoned_up {
                crate::audio::Ambience::Interior
            } else {
                crate::audio::Ambience::Exterior
            };
            a.set_ambience(amb, dt);
            // Engine loop: throttle winds it up immediately (the ROAR when moving), sustained by
            // ground speed; stopped = idle hum only; a knocked-out Tiger's motor dies to idle. On
            // foot the throttle input is your LEGS — the parked Tiger idles, it does not rev.
            let speed_frac = (self.player.speed.abs() / tank::TOP_SPEED).clamp(0.0, 1.0);
            let power = if self.player.alive && matches!(self.mode, Mode::Tank) {
                (0.4 * self.drive_cmd.abs().min(1.0) + 0.6 * speed_frac).clamp(0.0, 1.0)
            } else {
                0.0
            };
            // Engine OFF = true silence (not idle); the loop also dies with the tank. A parked
            // Tiger left RUNNING keeps idling while you scout on foot — that's your choice.
            a.set_engine(power, self.engine_on && self.player.alive, dt);
            // MG sustained-fire loop: runs while rounds are leaving the barrel (set by update_mg),
            // perspective picked by the same buttoned-up state as the ambience.
            a.set_mg(
                matches!(self.mode, Mode::Tank) && self.mg_hot,
                buttoned_up,
                dt,
            );
            // Bend the outgoing shell shriek's pitch down + fade it over the round's flight.
            a.update_shell_shriek(dt);
        }

        // Speed of sound: release delayed combat sounds whose wavefront has now reached the player.
        // The source survives in the queue so the final direct ray, head-relative bearing, snow/wall
        // reflections and hull transmission are evaluated where the listener is when the bang lands.
        let mut arrived = Vec::new();
        let mut i = 0;
        while i < self.scheduled.len() {
            self.scheduled[i].delay -= dt;
            if self.scheduled[i].delay <= 0.0 {
                arrived.push(self.scheduled.swap_remove(i));
            } else {
                i += 1;
            }
        }
        if !arrived.is_empty() {
            let listener = r.camera.pos;
            let facing = r.camera.forward();
            let buttoned = matches!(self.mode, Mode::Tank)
                && !((matches!(self.station, Station::Commander) && self.commander_outside)
                    || (matches!(self.station, Station::Driver) && self.driver_hatch));
            let hard_surfaces = self
                .occluders
                .iter()
                .filter(|o| ((o.min + o.max) * 0.5).distance(listener) < 85.0)
                .count();
            let reverb = if buttoned {
                crate::acoustics::ReverbParams::STEEL_INTERIOR
            } else if hard_surfaces >= 3 {
                crate::acoustics::ReverbParams::VILLAGE
            } else {
                crate::acoustics::ReverbParams::OPEN_SNOW
            };
            for s in arrived {
                let bearing = bearing_deg_from(facing, s.source - listener);
                // A wreck blast begins inside its own newly registered hull AABB. Step beyond any
                // cover box containing the origin so the source cannot occlude itself; buildings
                // and terrain farther along the path still participate normally.
                let mut acoustic_origin = s.source + Vec3::new(0.0, 1.2, 0.0);
                let toward_listener = (listener - acoustic_origin).normalize_or_zero();
                for cover in &self.occluders {
                    let inside = acoustic_origin.x >= cover.min.x
                        && acoustic_origin.x <= cover.max.x
                        && acoustic_origin.y >= cover.min.y
                        && acoustic_origin.y <= cover.max.y
                        && acoustic_origin.z >= cover.min.z
                        && acoustic_origin.z <= cover.max.z;
                    if inside {
                        acoustic_origin +=
                            toward_listener * ((cover.max - cover.min).length() * 0.5 + 0.25);
                    }
                }
                let world_block = if self.los_clear(acoustic_origin, listener) {
                    0.0
                } else {
                    0.78
                };
                let hull_block = if buttoned { 0.34 } else { 0.0 };
                let occlusion = 1.0 - (1.0 - world_block) * (1.0 - hull_block);
                if let Some(a) = &self.audio {
                    if let Some(buf) = s.buf {
                        // Parametric reports may arrive pre-panned, but the listener can rotate
                        // during flight. Collapse once and spatialise again against the live head.
                        let mono: Vec<f32> = buf
                            .chunks_exact(2)
                            .map(|frame| (frame[0] + frame[1]) * std::f32::consts::FRAC_1_SQRT_2)
                            .collect();
                        let dry = crate::acoustics::spatialize_mono(&mono, bearing, occlusion);
                        let mut wet = crate::acoustics::schroeder_reverb(&dry, reverb);
                        if buttoned {
                            crate::acoustics::interior_muffle(&mut wet, 0.92);
                        }
                        a.play_stereo(wet, s.volume);
                    } else {
                        a.play_world_muffled(
                            s.sfx,
                            s.volume,
                            bearing,
                            occlusion,
                            reverb,
                            if buttoned { 0.92 } else { 0.0 },
                        );
                    }
                }
            }
        }

        // Ambient battlefield explosions: a distant boom somewhere on the horizon around the player
        // every ~6-20s (randomized via hash), delivered through the speed-of-sound queue (long delay
        // + heavy range attenuation) so the whole front reads as at war, not just this duel.
        self.ambient_t -= dt;
        if self.ambient_t <= 0.0 {
            self.ambient_tick = self.ambient_tick.wrapping_add(13);
            let ang = hash01(self.ambient_tick) * std::f32::consts::TAU;
            let dist = AMBIENT_DIST_MIN
                + hash01(self.ambient_tick.wrapping_add(1)) * (AMBIENT_DIST_MAX - AMBIENT_DIST_MIN);
            let src = self.player.center() + Vec3::new(ang.cos() * dist, 0.0, ang.sin() * dist);
            let picks = crate::audio::Sfx::AMBIENT_BOOMS;
            let idx = ((hash01(self.ambient_tick.wrapping_add(2)) * picks.len() as f32) as usize)
                .min(picks.len() - 1);
            self.schedule_sound_from(picks[idx], src, VOL_AMBIENT_EXP);
            self.ambient_t = AMBIENT_EXP_MIN
                + hash01(self.ambient_tick.wrapping_add(3)) * (AMBIENT_EXP_MAX - AMBIENT_EXP_MIN);
        }

        // Gun recoil kick. In the commander/exterior view it's a heavy lurch so the shot has
        // weight; in the tight gunner optic it must be a sharp shake, NOT a lurch (a 2 m backward
        // blast there throws the eye through the tank into the fog). Set to 1.0 on fire, bleeds off.
        if self.fx.recoil > 0.0 {
            let kick = self.fx.recoil;
            let back = -r.camera.forward();
            if self.gunner_optic {
                r.camera.pos += back * (0.10 * kick);
                r.camera.pitch += 0.05 * kick;
            } else {
                r.camera.pos += back * (1.6 * kick);
                r.camera.pitch += 0.12 * kick;
            }
            self.fx.recoil = (self.fx.recoil - dt * 4.5).max(0.0); // ~0.22 s settle
        }

        // Crush shudder: while the tracks grind a wall the whole eye judders — a low-frequency
        // heave + a pitch buzz, deterministic (phase oscillator, no rand), spiking on the give.
        if self.crush_rumble > 0.0 {
            self.crush_phase += dt;
            let k = self.crush_rumble;
            r.camera.pos.y += (self.crush_phase * 61.0).sin() * 0.035 * k;
            r.camera.pitch += (self.crush_phase * 47.0).sin() * 0.0045 * k;
            self.crush_rumble = (self.crush_rumble - dt * 2.2).max(0.0);
        }

        // Snow, wreck smoke and flames agree on one zoned WORLD wind. Camera rotation only changes
        // how the vector projects on screen; it can never flip the storm with the viewpoint.
        r.set_weather_wind(wind_at(r.camera.pos));

        // ── RTT periscopes: with the commander buttoned behind the blocks, render the ONE tile
        // whose bearing best matches the look (the rest hold stale frames — period-honest). The
        // aperture camera is WIDE; the tile is its centre crop (a pinhole centre crop IS the
        // narrow-FOV image, so the distorted periphery is discarded).
        if let Some((dev, q)) = self.rtt_dev.clone() {
            let inside = matches!(self.station, Station::Commander | Station::Loader)
                && !(matches!(self.station, Station::Commander) && self.commander_outside);
            if matches!(self.mode, Mode::Tank) && inside && !self.rtt_screens.is_empty() {
                let models = [
                    self.player.hull_model(),
                    self.player.turret_model(),
                    self.player.gun_model(),
                ];
                // ALL periscopes live EVERY frame (user 2026-07-08): the whole ring re-renders, not
                // just the gaze-matched one — no more frozen eyepieces. One offscreen scene render
                // per screen (cheap at the internal res); the main frame re-renders HDR after.
                let saved = (
                    r.camera.pos,
                    r.camera.yaw,
                    r.camera.pitch,
                    r.camera.fov_y,
                    r.weather_shelter,
                );
                // The RTT camera sits at the exterior periscope head. Render the outdoor storm into
                // the tile; the interior screen/eyepiece geometry provides the real shelter later.
                r.weather_shelter = 0.0;
                for sc in self.rtt_screens.iter() {
                    let m = models[sc.frame];
                    // Bias toward the HORIZON: block-face normals tilt skyward, and a periscope full
                    // of sky is useless — the fight is at ground level.
                    let raw = m.transform_vector3(sc.bearing).normalize_or_zero();
                    let wd = (Vec3::new(raw.x, (raw.y - 0.18).clamp(-0.25, 0.05), raw.z))
                        .normalize_or_zero();
                    // A physical periscope does not change magnification because another optic is
                    // raised in front of it. Keep the RTT projection fixed: binoculars now enlarge
                    // the SAME tile the player saw in normal view instead of silently replacing it
                    // with a different, tighter scene.
                    let optic_deg = 19.0f32;
                    let fov = 2.0
                        * (optic_deg.to_radians().tan()
                            * (r.ih as f32 / crate::renderer::RTT_TILE_H as f32))
                            .atan();
                    // Eye at the periscope HEAD, not the eyepiece: high enough to clear the turret
                    // roof and far enough out to clear the mantlet — at +0.30/+0.40 the forward
                    // periscopes caught the gun's recoil cylinders as giant swaying "horns".
                    let ap =
                        m.transform_point3(sc.centroid + Vec3::new(0.0, 0.55, 0.0)) + wd * 0.75;
                    r.camera.pos = ap;
                    r.camera.set_forward(wd);
                    r.camera.fov_y = fov;
                    r.render_rtt_tile(&dev, &q, sc.tile);
                }
                (
                    r.camera.pos,
                    r.camera.yaw,
                    r.camera.pitch,
                    r.camera.fov_y,
                    r.weather_shelter,
                ) = saved;
            }
        }

        // ── Interior atmosphere: hazy air + SUN SHAFTS through the real openings + drifting
        // dust (the Fury look). Apertures each frame: the cupola hatch mouth when open (THE
        // shaft), the see-through ports (tiger_ports.json), and the periscope heads (weak spill —
        // mirror optics leak far less than an open hole). Gains fold in how sun-facing each
        // opening is, so the lee side of the tank stays dark and the shafts swing as you turn.
        {
            let inside = matches!(self.mode, Mode::Tank)
                && !matches!(self.station, Station::Gunner) // the optic view is its own world
                && !(matches!(self.station, Station::Commander) && self.commander_outside);
            if inside && std::env::var("KESSEL_NO_ATMO").is_err() {
                let sun = r.shaft_sun_dir();
                let beam = -sun;
                let models = [
                    self.player.hull_model(),
                    self.player.turret_model(),
                    self.player.gun_model(),
                ];
                let mut aps: Vec<crate::renderer::Aperture> = Vec::new();
                if self.hatch_open {
                    // Cupola mouth: local centre of the periscope ring, a touch above it.
                    let hp = models[1].transform_point3(Vec3::new(0.564, 2.62, -0.42));
                    aps.push(crate::renderer::Aperture {
                        pos: hp,
                        radius: 0.32,
                        dir: beam,
                        gain: 0.9 + sun.y.max(0.0) * 2.0,
                    });
                }
                // ONLY the true through-openings beam (hatch above + the see-through ports).
                // The periscope heads are mirror optics — no shaft reaches the compartment —
                // and feeding them in flooded the ring around the commander's eye with beams
                // (a full-frame wash that dithered into grain).
                for s in self.port_aps.iter() {
                    let m = models[s.frame];
                    let out_w = m.transform_vector3(s.bearing).normalize_or_zero();
                    let gain = 0.45 + sun.dot(out_w).max(0.0) * 1.8;
                    if gain < 0.02 {
                        continue;
                    }
                    aps.push(crate::renderer::Aperture {
                        pos: m.transform_point3(s.centroid),
                        radius: 0.060,
                        dir: beam,
                        gain,
                    });
                }
                // The periscope eyepieces cast a glow-spill ALONG THEIR OWN OPTIC AXIS into the
                // compartment (2026-07-08: "it should be a LIGHT SOURCE that makes that shaft").
                // First cut pointed these along -sun with a pin radius — tiny bright PILLS
                // floating mid-air, disconnected from the optics (the "weird blobs"). Now the
                // beam starts at the pane and runs inward the way the light actually leaves it.
                for sc in self.rtt_screens.iter() {
                    let m = models[sc.frame];
                    let out_w = m.transform_vector3(sc.bearing).normalize_or_zero();
                    let gain = 0.75 + sun.dot(out_w).max(0.0) * 2.0;
                    aps.push(crate::renderer::Aperture {
                        pos: m.transform_point3(sc.centroid),
                        radius: 0.045,
                        dir: -out_w, // inward: the spill leaves the eyepiece toward the crew
                        gain,
                    });
                }
                aps.sort_by(|a, b| b.gain.total_cmp(&a.gain));
                // Anchor the dust to the hull so motes hang still relative to the crew, not the world.
                let hull_o = models[0].transform_point3(Vec3::ZERO);
                r.set_interior_atmosphere(0.08 + self.interior_haze * 0.78, 0.030, hull_o, &aps);
            } else {
                r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO, &[]);
            }
        }

        // push tank transforms
        r.set_model(self.slots.tiger[0], self.player.hull_model());
        r.set_model(self.slots.tiger[1], self.player.turret_model());
        r.set_model(self.slots.tiger[2], self.player.gun_model());
        let glass_models = [
            self.player.hull_model(),
            self.player.turret_model(),
            self.player.gun_model(),
        ];
        for (k, s) in self.slots.tiger_glass.iter().enumerate() {
            if let Some(s) = s {
                r.set_model(*s, glass_models[k]);
            }
        }
        for (k, s) in self.slots.tiger_rtt.iter().enumerate() {
            if let Some(s) = s {
                r.set_model(*s, glass_models[k]);
            }
        }

        // Advance wreck fire clocks, tumbling turret-pop physics, and delayed cook-offs.
        self.update_wrecks(dt);

        // Advance falling trees (shared tail — trees keep toppling while on foot too) and route
        // their events into the pooled FX + the ranged-audio scheduler.
        let mut tree_events = Vec::new();
        self.trees.update(dt, r, &mut tree_events);
        self.route_tree_events(tree_events);

        // Advance ballistic wall chunks + support cascades (docs/DESTRUCTION.md §6).
        self.chunks.update(dt, r, &self.hf);

        // Advance the PLAYER's interior fire: it grows once lit and, once established, reaches the
        // ammunition on a countdown — an unchecked fire cooks off the ammo (catastrophic kill).
        self.update_fire(dt);
        self.update_haze(dt);

        // Push enemy transforms. A LIVING enemy draws normally (lit, no glow). A DEAD one stays on
        // the field as a BURNING WRECK — its hull (and, unless the turret popped, its turret) keep
        // their frozen death pose and glow with a flickering orange fire; a catastrophic kill drives
        // the blown-off turret/gun along their ballistic arc instead.
        for ei in 0..self.enemies.len() {
            let rigs = [
                self.slots.t34[ei],
                self.slots.su100[ei],
                self.slots.kv2[ei],
                self.slots.isu152[ei],
                self.slots.is2[ei],
                self.slots.is3[ei],
            ];
            if !self.enemy_active[ei] {
                for rig in rigs {
                    for slot in rig {
                        r.set_visible(slot, false);
                    }
                }
                r.set_visible(self.slots.flame[ei], false);
                continue;
            }
            let e = &self.enemies[ei];
            let active_rig = match e.kind {
                EnemyKind::T34 => 0,
                EnemyKind::Su100 => 1,
                EnemyKind::Kv2 => 2,
                EnemyKind::Isu152 => 3,
                EnemyKind::Is2 => 4,
                EnemyKind::Is3 => 5,
            };
            let slot = rigs[active_rig];
            for (index, rig) in rigs.into_iter().enumerate() {
                if index != active_rig {
                    for dormant in rig {
                        r.set_visible(dormant, false);
                    }
                }
            }
            if e.alive {
                for (i, m) in [e.hull_model(), e.turret_model(), e.gun_model()]
                    .iter()
                    .enumerate()
                {
                    r.set_visible(slot[i], true);
                    r.set_emissive(slot[i], 0.0);
                    r.set_model(slot[i], *m);
                }
                // A casemate SU-100 has no separate turret mesh — its middle slot is a hidden placeholder.
                if e.kind.casemate() {
                    r.set_visible(slot[1], false);
                }
                r.set_visible(self.slots.flame[ei], false);
                continue;
            }
            let w = &self.wrecks[ei];
            let seed = (ei as u32).wrapping_mul(2654435761);
            let glow = fire_glow(w.fire_t, seed, HULL_GLOW, w.cook_flash);
            // hull: always kept as the burning body
            r.set_visible(slot[0], true);
            r.set_emissive(slot[0], glow);
            r.set_model(slot[0], e.hull_model());
            // Tall flickering flame plume rising off the wreck — reads as fire (a flame column) at
            // range far better than the flat hull glow alone; height + intensity flicker off fire_t.
            let flick = fire_glow(w.fire_t, seed ^ 0x9e37, 1.0, w.cook_flash).clamp(0.4, 4.0);
            let fbase = e.pos + Vec3::new(0.0, 0.4, 0.0);
            // The flame column leans downwind — hard out in the open, near-upright in the sheltered
            // streets (zoned wind via `wind_at`), so the fire visibly reads the local gust.
            let wind = wind_at(e.pos);
            let tilt = Quat::from_rotation_arc(
                Vec3::Y,
                Vec3::new(wind.x * 0.09, 1.0, wind.y * 0.09).normalize(),
            );
            r.set_visible(self.slots.flame[ei], true);
            r.set_emissive(self.slots.flame[ei], 7.5 + 4.5 * flick);
            r.set_model(
                self.slots.flame[ei],
                Mat4::from_scale_rotation_translation(
                    Vec3::new(0.9 + 0.2 * flick, 0.7 + 0.6 * flick, 0.9 + 0.2 * flick),
                    tilt,
                    fbase,
                ),
            );
            match w.kind {
                WreckKind::TurretPop => {
                    // The fresh blast can light flying metal, but once it lands it becomes charred
                    // steel lit by the nearby flame—not a solid orange emissive object.
                    let hot = if w.landed {
                        glow * 0.6
                    } else {
                        (glow + 2.4).min(5.0)
                    };
                    r.set_visible(slot[1], true);
                    r.set_emissive(slot[1], hot);
                    r.set_model(slot[1], w.turret_pop_xf());
                    r.set_visible(slot[2], true);
                    r.set_emissive(slot[2], hot * 0.7);
                    r.set_model(slot[2], w.gun_pop_xf());
                }
                _ => {
                    // Burn wreck: turret + gun stay settled on the hull, turret engulfed too.
                    r.set_visible(slot[1], true);
                    r.set_emissive(slot[1], glow * 0.85);
                    r.set_model(slot[1], e.turret_model());
                    r.set_visible(slot[2], true);
                    r.set_emissive(slot[2], glow * 0.4);
                    r.set_model(slot[2], e.gun_model());
                }
            }
            // A casemate SU-100 wreck has no separate turret mesh (and never turret-pops) — keep its
            // placeholder slot hidden; the casemate burns as part of the hull mesh.
            if e.kind.casemate() {
                r.set_visible(slot[1], false);
            }
        }

        // The Halbe platoon remains visually legible as two complete Tigers, independently posed
        // from the player's machine. Their muzzle/tracer events are driven above by the ally AI.
        for (i, ally) in self.allies.iter().enumerate() {
            let sl = self.slots.allies[i];
            let wreck_glow = if ally.tank.alive { 0.0 } else { 0.18 };
            for (part, model) in [
                ally.tank.hull_model(),
                ally.tank.turret_model(),
                ally.tank.gun_model(),
            ]
            .into_iter()
            .enumerate()
            {
                r.set_visible(sl[part], true);
                r.set_emissive(sl[part], wreck_glow * if part == 0 { 1.0 } else { 0.55 });
                r.set_model(sl[part], model);
            }
        }

        // Frozen wrecks from earlier streamed contacts stay where they died. This makes the route
        // accumulate a readable history of the fight and keeps every retained LOS occluder honest.
        for i in 0..self.slots.hulk_t34.len() {
            let rigs = [
                self.slots.hulk_t34[i],
                self.slots.hulk_su100[i],
                self.slots.hulk_kv2[i],
                self.slots.hulk_isu152[i],
                self.slots.hulk_is2[i],
                self.slots.hulk_is3[i],
            ];
            let Some(hulk) = self.campaign_hulks.get(i) else {
                for rig in rigs {
                    for slot in rig {
                        r.set_visible(slot, false);
                    }
                }
                continue;
            };
            let live_index = match hulk.kind {
                EnemyKind::T34 => 0,
                EnemyKind::Su100 => 1,
                EnemyKind::Kv2 => 2,
                EnemyKind::Isu152 => 3,
                EnemyKind::Is2 => 4,
                EnemyKind::Is3 => 5,
            };
            let live = rigs[live_index];
            for (rig_index, rig) in rigs.into_iter().enumerate() {
                if rig_index != live_index {
                    for slot in rig {
                        r.set_visible(slot, false);
                    }
                }
            }
            for part in 0..3 {
                let visible = !(hulk.kind.casemate() && part == 1);
                r.set_visible(live[part], visible);
                r.set_model(live[part], hulk.models[part]);
                r.set_emissive(
                    live[part],
                    if part == 0 {
                        hulk.ember
                    } else {
                        hulk.ember * 0.55
                    },
                );
            }
        }

        // A spare-Tiger remount queued a hulk swap: the spare's meshes move to the old hull's
        // death pose (it stands as the wreck you abandoned), and the player meshes now render at
        // the spare's position via the normal player-transform pushes.
        if let Some((k, h, t, gn)) = self.pending_hulk.take() {
            let sl = self.slots.abandoned[k];
            r.set_model(sl[0], h);
            r.set_model(sl[1], t);
            r.set_model(sl[2], gn);
        }

        self.update_fx(dt, r);
        self.drive_lights(r);
        self.drive_interior_parts(r);
        // Ray-pass occluders: the live tanks as yaw OBBs (contact shadows under every hull).
        {
            let mut bx: Vec<(Vec3, Vec3, f32)> = Vec::new();
            if self.player.alive {
                bx.push((
                    self.player.center() + Vec3::Y * 0.3,
                    Vec3::new(1.85, 1.25, 3.4),
                    self.player.heading,
                ));
            }
            for ally in self.allies.iter().filter(|ally| ally.tank.alive) {
                if bx.len() >= crate::renderer::MAX_RT_BOXES {
                    break;
                }
                bx.push((
                    ally.tank.center() + Vec3::Y * 0.3,
                    Vec3::new(1.85, 1.25, 3.4),
                    ally.tank.heading,
                ));
            }
            for e in self.enemies.iter().filter(|e| e.alive) {
                if bx.len() >= crate::renderer::MAX_RT_BOXES {
                    break;
                }
                let half = match e.kind {
                    EnemyKind::T34 => Vec3::new(1.50, 1.10, 3.10),
                    EnemyKind::Su100 => Vec3::new(1.55, 1.05, 3.25),
                    EnemyKind::Kv2 => Vec3::new(1.70, 2.05, 3.40),
                    EnemyKind::Isu152 => Vec3::new(1.70, 1.35, 3.35),
                    EnemyKind::Is2 => Vec3::new(1.65, 1.45, 3.35),
                    EnemyKind::Is3 => Vec3::new(1.70, 1.35, 3.45),
                };
                bx.push((e.center() + Vec3::Y * 0.25, half, e.heading));
            }
            r.set_rt_boxes(&bx);
        }

        // Blood-smear overlay RETIRED (solo crew — there are no crewmen to lose beside you, and
        // your own death cuts straight to the kill-cam). The crush path stays; never driven.
        r.crew_ko = 0.0;
    }

    /// SOLO CREW: it's just you, and you're alive if the game is running — the man-down speed
    /// penalties dissolved into the station model (you can only work the seat you're AT).
    fn crew_ok(&self, _role: CrewRole) -> bool {
        true
    }
    /// True iff module `kind` still works.
    fn module_ok(&self, kind: ModuleKind) -> bool {
        self.modules.iter().any(|m| m.kind == kind && m.functional)
    }

    /// Can the player actually get a round off — a round SEATED in the breech (`Loaded`), the
    /// travel-lock + return-to-battery (Tank::can_fire), and the gun + gunner intact. The LOADER gate
    /// is gone: a dead loader stops the RELOAD (slower/manual), not the firing — the gun fires iff a
    /// round is up the spout (see `service_breech` / `on_fired`).
    fn player_can_fire(&self) -> bool {
        self.lever_open <= LEVER_SHUT // the breech must be CLOSED — the lever finishes the load
            && matches!(self.breech, Breech::Loaded)
            && self.player.can_fire()
            && self.module_ok(ModuleKind::MainGun)
            && self.crew_ok(CrewRole::Gunner)
    }

    /// Derived 0..100 integrity readout (crew + modules − fire), purely so the headless telemetry
    /// keeps reading sensibly. Never chipped by a bounce — only real damage moves it.
    fn recompute_hp(&mut self) {
        if !self.player.alive {
            self.player.hp = 0.0;
            return;
        }
        // Solo crew: hp is the MACHINE's state (modules + fire) — the man is binary (see
        // resolve_spall's hit_body).
        let mods = self.modules.iter().filter(|m| m.functional).count() as f32
            / self.modules.len().max(1) as f32;
        let fire_pen = (self.fire * 20.0).min(25.0);
        self.player.hp = (mods * 100.0 - fire_pen).clamp(0.0, tank::MAX_HP);
    }

    // ── Station system ───────────────────────────────────────────────────────────────────────────
    /// Request a move to station `s`. Ignored if already there or mid-switch; otherwise flips the
    /// target immediately and arms the switch cost (during which no seat control is honoured).
    pub fn request_station(&mut self, s: Station) {
        if s == self.station || self.switch_t > 0.0 {
            return;
        }
        self.switch_from = self.station;
        self.station = s;
        self.switch_dur = switch_cost(self.switch_from, s);
        self.switch_t = self.switch_dur;
    }

    /// World eye position for a station's first-person camera (used for the switch lerp too).
    fn station_eye(&self, s: Station) -> Vec3 {
        match s {
            Station::Gunner => self.player.gunsight(),
            // INSIDE: standing on the TURRET FLOOR wherever you've walked (WASD moves the body).
            // Height puts the VISION PORTS AT EYE LEVEL (turret side slits sit ~1.28 above the
            // floor) — the old 1.52 had you towering over them.
            Station::Loader => self.player.turret_model().transform_point3(Vec3::new(
                self.walk_pos.x,
                crate::interior_rig::FLOOR_STAND_Y + 1.29,
                self.walk_pos.z,
            )),
            // DRIVER: seated behind his visor slit (the slit is a real hole now — aperture at
            // hull-local (-0.448, 1.749, 2.119)); eye at seat depth, just under slit centre, clear
            // of the transmission that fills the space right at the glacis.
            Station::Driver => self
                .player
                .hull_model()
                .transform_point3(Vec3::new(-0.46, 1.62, 1.55)),
            // RADIO / BOW MG: the mirrored hull station behind the ball mount and wireless set.
            Station::Radioman => self
                .player
                .hull_model()
                .transform_point3(Vec3::new(0.66, 1.58, 1.48)),
            Station::Commander => {
                if self.commander_outside {
                    // HEAD OUT: eyes above the hatch ring, over the cupola blocks.
                    let mut p = self.player.commander_eye();
                    p.y += 0.52;
                    p
                } else {
                    // BUTTONED: eye AT the cupola vision-slit ring (the 7 blocks are cut open) —
                    // centre of the cupola at slit height, so free-look pans across real slits of
                    // daylight with armor between them. The old +0.30 "see over the deck" lift was
                    // a workaround from when the blocks were baked opaque.
                    self.player
                        .turret_model()
                        .transform_point3(Vec3::new(0.564, 2.457, -0.423))
                }
            }
        }
    }

    /// Place connected crew bodies at the physical posts reported in their input packets. The local
    /// role stays invisible (the camera is that body); every remote role remains visible through the
    /// real compartment geometry, including a gunner climbing across to serve the breech.
    fn drive_lan_crew_rigs(&self, coop: Option<crate::lan::CrewFrame>, r: &mut Renderer) {
        let Some(frame) = coop else {
            for &slot in &self.slots.lan_crew {
                r.set_visible(slot, false);
            }
            return;
        };
        for role in crate::lan::LanRole::ALL {
            let slot = self.slots.lan_crew[role as usize];
            let post = frame.input(role).post;
            let local_body = self.lan_local_role == Some(role);
            let local_perch = self.lan_local_role.is_some()
                && post
                    == match self.station {
                        Station::Commander => crate::lan::LanRole::Commander,
                        Station::Driver => crate::lan::LanRole::Driver,
                        Station::Gunner => crate::lan::LanRole::Gunner,
                        Station::Loader => crate::lan::LanRole::Loader,
                        Station::Radioman => crate::lan::LanRole::Radio,
                    };
            let visible = frame.occupied(role)
                && !frame.input(role).flag(crate::lan::F_DISMOUNTED)
                && !local_body
                && !local_perch;
            r.set_visible(slot, visible);
            if !visible {
                continue;
            }
            let (base, scale) = match post {
                crate::lan::LanRole::Driver => (
                    self.player.hull_model() * Mat4::from_translation(Vec3::new(-0.46, 0.38, 1.45)),
                    0.80,
                ),
                crate::lan::LanRole::Radio => (
                    self.player.hull_model() * Mat4::from_translation(Vec3::new(0.66, 0.38, 1.42)),
                    0.80,
                ),
                crate::lan::LanRole::Gunner => (
                    self.player.turret_model()
                        * Mat4::from_translation(Vec3::new(-0.50, 0.82, 0.25)),
                    0.78,
                ),
                crate::lan::LanRole::Loader => (
                    self.player.turret_model()
                        * Mat4::from_translation(Vec3::new(0.72, 0.72, 0.12)),
                    0.82,
                ),
                crate::lan::LanRole::Commander => (
                    self.player.turret_model()
                        * Mat4::from_translation(Vec3::new(0.36, 1.00, -0.48)),
                    0.80,
                ),
            };
            r.set_model(slot, base * Mat4::from_scale(Vec3::splat(scale)));
            r.set_emissive(slot, 0.18);
        }
    }

    // ── Manual reload (breech + casing eject + ram) ────────────────────────────────────────────────
    /// Called the instant a round is fired: the breech is now dead until serviced. If the loader is up
    /// he yanks the hot brass immediately (assisted — only the RAM is left for the player); a KO'd
    /// loader leaves the casing in the gun for the lone survivor to fumble free (see `service_breech`).
    fn on_fired(&mut self) {
        self.rounds_spent += 1; // telemetry (the rack depletes per-slot at GRAB time now)
        self.ram = 0.0;
        // The HOT CASE stays seated in the breech — the loader must PULL it by hand (LMB held on
        // the SpentCasing hotspot) and toss it before a fresh round can go in. The auto/KO'd path
        // (`service_breech`) still frees it with the Space fumble.
        self.breech = Breech::Spent;
        self.bore_smoke_pending = true;
    }

    /// Headless auto-crew: the scripted Tiger opponent (playtest / duel / trainer) services the
    /// breech through the REAL state machine below — the human path is the INSIDE-view mouse
    /// ritual, which no auto-policy can perform, and without this the auto-Tiger fought every
    /// match on its single pre-loaded round. Gated exactly like the mouse path: posted at the
    /// Loader, not mid-switch, not mid-activity.
    pub fn auto_service_breech(&mut self, space_edge: bool, dt: f32) {
        if self.switch_t <= 0.0
            && self.activity.is_none()
            && matches!(self.station, Station::Loader)
        {
            self.service_breech(space_edge, dt);
        }
    }

    /// Service the breech from the Loader station (Space). Sequential by breech state: eject the spent
    /// casing (a fumble gate only when the loader is down — otherwise it was auto-yanked on firing),
    /// then MASH a fresh round home (degraded, but never impossible, one-handed).
    fn service_breech(&mut self, space_edge: bool, dt: f32) {
        match self.breech {
            Breech::Loaded => {}
            Breech::Spent => {
                // Lone survivor freeing the hot casing: a short fumble while posted at the breech.
                self.eject_t += dt;
                if self.lever_open < LEVER_OPEN && self.bore_smoke_pending {
                    self.interior_haze = (self.interior_haze + 0.22).min(1.0);
                    self.bore_smoke_pending = false;
                }
                self.lever_open = 1.0; // the auto path throws the lever as part of the fumble
                if self.eject_t >= EJECT_FUMBLE {
                    self.eject_t = 0.0;
                    self.breech = Breech::Empty;
                    self.eject_casing();
                }
            }
            Breech::Empty => {
                // No rounds if the rack is gone (historically it cooked off anyway) — ram blocked.
                if !self.module_ok(ModuleKind::AmmoRack) {
                    return;
                }
                // …and the rack must actually still HOLD a round (per-slot stowage now).
                let Some(next_slot) = self.slot_taken.iter().position(|t| !t) else {
                    return;
                };
                if space_edge {
                    self.ram += ram_per_press(self.crew_ok(CrewRole::Loader));
                    if self.ram >= 1.0 {
                        self.ram = 0.0;
                        self.breech = Breech::Loaded;
                        self.lever_open = 0.0; // …and slams the breech shut
                        self.slot_taken[next_slot] = true; // the auto-crew takes from the rack too
                        if let Some(a) = &self.audio {
                            a.play_at_volume(crate::audio::Sfx::BreechClack, 0.9);
                        } // breech CLACK
                    }
                } else {
                    self.ram = (self.ram - RAM_DECAY * dt).max(0.0);
                }
            }
        }
    }

    /// Spawn one tumbling rigid-body casing out of the breech mouth (up + outboard + back), with hashed
    /// spin (deterministic — no rand). Clatters on eject; bounces once off the turret floor.
    fn eject_casing(&mut self) {
        let k = self.next_casing % N_CASING;
        self.next_casing = self.next_casing.wrapping_add(1);
        self.kill_tick = self.kill_tick.wrapping_add(1);
        let seed = self.kill_tick.wrapping_mul(2654435761);
        let s = |n: u32| hash01(seed.wrapping_add(n));
        let turret = self.player.turret_model();
        let mouth = turret.transform_point3(Vec3::new(0.35, 1.90, 0.35));
        let right = turret.transform_vector3(Vec3::X).normalize_or_zero();
        let back = turret.transform_vector3(-Vec3::Z).normalize_or_zero();
        let vel =
            right * (1.6 + 1.2 * s(1)) + Vec3::Y * (2.6 + 1.2 * s(2)) + back * (1.2 + 0.9 * s(3));
        let floor_y = turret.transform_point3(Vec3::new(0.0, 1.45, 0.0)).y; // turret-basket floor
        let c = &mut self.casings[k];
        c.active = true;
        c.pos = mouth;
        c.vel = vel;
        c.spin_axis = Vec3::new(s(4) - 0.5, s(5) - 0.5, s(6) - 0.5).normalize_or_zero();
        c.spin = 6.0 + 9.0 * s(7);
        c.ang = 0.0;
        c.t = 4.0;
        c.bounced = false;
        c.floor_y = floor_y;
        if let Some(a) = &self.audio {
            a.play_at_volume(crate::audio::Sfx::BrassClatter, 0.7);
        } // brass clatter
    }

    // ── Fire-fighting + repair (global emergency activities — both hands full) ──────────────────────
    /// Drive the F=extinguisher / R=repair activities. Setting `self.activity` here suppresses every
    /// station control (via the `acting` gate) for the duration — you cannot aim, drive, or reload
    /// with the extinguisher or a wrench in your hands. Reads/writes ONLY the existing fire/module
    /// state (no parallel model).
    fn update_activities(&mut self, input: &TankInput, dt: f32) {
        if !self.player.alive || self.switch_t > 0.0 {
            self.activity = None;
            return;
        }
        // Extinguisher (priority — the fire is THE emergency): grab, then spray while held. Every
        // second of spray is a second the swarm shoots a Tiger that can't shoot back.
        // Trigger paths: held F anywhere; LMB while HOVERING the bottle (legacy quick-grab); or
        // the EQUIP pattern — E took the bottle off the wall, LMB uses what's in your hands.
        let extinguish = input.extinguish
            || (input.mg_coax && self.hover == Some(Hotspot::Extinguisher))
            || (self.ext_equipped && input.mg_coax);
        self.ext_want_spray = extinguish;
        if extinguish {
            if !matches!(self.activity, Some(Activity::Extinguish)) {
                self.activity = Some(Activity::Extinguish);
                self.ext_t = 0.0;
            }
            self.ext_t += dt;
            if self.ext_t >= EXT_GRAB && self.fire_active {
                self.fire = (self.fire - EXT_RATE * dt).max(0.0);
                // The buy-back of the cook-off countdown is handled in update_fire (fire < threshold).
                if self.fire < 0.05 {
                    self.fire_active = false;
                    self.fire = 0.0;
                    self.fire_ammo_timer = None;
                    println!("[kessel] FIRE out — extinguished");
                }
            }
            return;
        }
        if matches!(self.activity, Some(Activity::Extinguish)) {
            self.activity = None;
        }

        // Field repair (R): the highest-priority repairable downed module the current station reaches.
        let repair = input.repair || (input.mg_coax && self.hover == Some(Hotspot::RepairKit));
        if repair {
            if let Some(kind) = self.repair_target() {
                if !matches!(self.activity, Some(Activity::Repair(k)) if k == kind) {
                    self.activity = Some(Activity::Repair(kind));
                    self.repair_t = 0.0;
                }
                self.repair_t += dt;
                if let Some(need) = repair_time(kind) {
                    if self.repair_t >= need {
                        if let Some(m) = self.modules.iter_mut().find(|m| m.kind == kind) {
                            m.functional = true;
                        }
                        println!("[kessel] REPAIR — {} restored", kind.name());
                        self.activity = None;
                        self.repair_t = 0.0;
                        self.recompute_hp();
                    }
                }
                return;
            }
        }
        if matches!(self.activity, Some(Activity::Repair(_))) {
            self.activity = None;
            self.repair_t = 0.0;
        }
    }

    /// The repairable downed module the current station can reach right now (highest priority first).
    /// Track/transmission fixes require the Driver to be HATCHED OUT (exposed) — the classic dangerous
    /// "thrown a track under fire" choice.
    fn repair_target(&self) -> Option<ModuleKind> {
        use ModuleKind::*;
        let candidates: &[ModuleKind] = match self.station {
            // Tracks are NOT here any more: a thrown track is the ON-FOOT hammer ritual
            // (kit from the turret toolbox → dismount → hammer the new link in at the break).
            Station::Driver if self.driver_hatch => &[Transmission],
            Station::Gunner => &[TurretTraverse, CoaxMg],
            Station::Commander => &[HullMg, Radio],
            _ => &[],
        };
        candidates.iter().copied().find(|&k| !self.module_ok(k))
    }

    /// Any repairable module currently down (drives the annunciator's repair lamp + the task readout).
    fn any_repairable_down(&self) -> bool {
        self.modules
            .iter()
            .any(|m| !m.functional && repair_time(m.kind).is_some())
    }

    /// Advance the player's interior fire (fuel/engine hit → a growing blaze). Once well-established
    /// it starts reaching the ammunition — a countdown that, if it elapses, cooks off the ammo.
    /// Populate this frame's positioned point lights (the new lighting system). Demonstrated now
    /// with the player + enemy MUZZLE FLASHES and the interior FIRE glow; the buttoned-up bulb +
    /// the cupola periscope apertures plug in here once the interior geometry lands.
    /// Drive the animated interior parts off the live tank state: the TPZ needle sweeps with the
    /// turret's bearing, the handwheels crank (geared far past 1:1 so laying visibly spins them),
    /// the chambered round shows exactly while one is up the spout, and the rack depletes.
    fn drive_interior_parts(&mut self, r: &mut Renderer) {
        use crate::interior_rig as rig;
        let tm = self.player.turret_model();
        let hm = self.player.hull_model();
        let yaw = self.player.turret_yaw;
        // TPZ needle: 1:1 turret bearing, about the dial-face axis (from the backing's transform).
        let tpz_axis = Vec3::new(0.772, 0.0, 0.636).normalize();
        r.set_model(
            self.slots.tpz_needle,
            tm * Mat4::from_translation(rig::TPZ_NEEDLE_POS)
                * Mat4::from_axis_angle(tpz_axis, -yaw),
        );
        r.set_model(
            self.slots.wheel1,
            tm * Mat4::from_translation(rig::WHEEL1_POS)
                * Mat4::from_axis_angle(Vec3::X, self.player.gun_pitch * 40.0),
        );
        r.set_model(
            self.slots.wheel2,
            tm * Mat4::from_translation(rig::WHEEL2_POS)
                * Mat4::from_axis_angle(Vec3::Y, yaw * 30.0),
        );
        let loaded_now = matches!(self.breech, Breech::Loaded);
        r.set_visible(self.slots.chamber_round, loaded_now && !self.he_loaded);
        r.set_visible(self.slots.chamber_round_he, loaded_now && self.he_loaded);
        r.set_model(
            self.slots.chamber_round,
            tm * Mat4::from_translation(rig::ROUND_SEAT),
        );
        r.set_model(
            self.slots.chamber_round_he,
            tm * Mat4::from_translation(rig::ROUND_SEAT),
        );
        // Per-slot stowage: a shell draws in its rack slot until GRABBED (slot_taken) — the very
        // shell you take leaves the rack and rides the carry (drive_reload_fx redraws it there).
        let carried = match self.carry_mesh {
            Some(CarriedMesh::RackShell(i)) => Some(i),
            _ => None,
        };
        for (i, (frame, pos, q)) in rig::SHELL_SLOTS.iter().enumerate() {
            if carried == Some(i) {
                continue; // in the hands — drive_reload_fx owns its transform this frame
            }
            let m = if *frame == rig::Frame::Hull { hm } else { tm };
            let quat = glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize();
            r.set_model(
                self.slots.shell0 + i,
                m * Mat4::from_rotation_translation(quat, *pos),
            );
            r.set_visible(self.slots.shell0 + i, !self.slot_taken[i]);
            // Emissive stays OFF: any nonzero value here rides the gunfire-orange FX path (the
            // shells aren't own-emissive) and painted rounds red. Reset defensively — the value
            // is persistent renderer state and an old glint could stick to a grabbed shell.
            r.set_emissive(self.slots.shell0 + i, 0.0);
        }
    }

    fn drive_lights(&mut self, r: &mut Renderer) {
        let mut lights: Vec<PointLight> = Vec::new();
        // Player muzzle flash: a bright warm flash at the barrel while it's hot — lights the tank,
        // the ground, and (at night) the whole nearby field for the frame.
        if self.fx.muzzle_t > 0.0 {
            let (mpos, _) = self.player.muzzle();
            let f = (self.fx.muzzle_t / 0.09).clamp(0.0, 1.0);
            lights.push(PointLight {
                pos: mpos,
                radius: 26.0,
                color: Vec3::new(1.0, 0.74, 0.40),
                intensity: 11.0 * f,
            });
        }
        // Interior fire glow (also lights the compartment once you can see it buttoned up).
        if self.fire_active {
            let c = self.player.center() + Vec3::Y * 0.6;
            let flick = 0.82 + 0.18 * (self.fire_t * 11.0).sin();
            lights.push(PointLight {
                pos: c,
                radius: 15.0,
                color: Vec3::new(1.0, 0.42, 0.16),
                intensity: 3.2 * self.fire.min(1.0) * flick,
            });
        }
        // Explosions are light sources, not merely bright geometry. The flash is short and violent;
        // `Renderer::set_lights` keeps the strongest sources when the battlefield is saturated.
        for b in self.blasts.iter().filter(|b| b.t > 0.0) {
            let f = (b.t / b.dur.max(0.01)).clamp(0.0, 1.0);
            lights.push(PointLight {
                pos: b.pos,
                radius: b.scale * (2.2 + 1.4 * (1.0 - f)),
                color: Vec3::new(1.0, 0.42 + 0.25 * f, 0.12),
                intensity: b.hot * (0.18 + 0.82 * f),
            });
        }
        // Burning wrecks illuminate their own soot columns and turn every completed engagement into
        // persistent orange geography rather than a collection of unlit glowing boxes.
        for (i, w) in self.wrecks.iter().enumerate().filter(|(_, w)| w.burning) {
            let flick = 0.78 + 0.22 * (w.fire_t * 9.7 + i as f32).sin();
            lights.push(PointLight {
                pos: self.enemies[i].center() + Vec3::Y * 0.7,
                radius: 14.0,
                color: Vec3::new(1.0, 0.30, 0.08),
                intensity: 3.8 * flick,
            });
        }
        // Enemy muzzle flashes light the field too — the swarm's fire flickers across the snow.
        for k in 0..E_FX_POOL {
            if self.e_shots[k].tracer_active && self.e_shots[k].tracer_t < 0.06 {
                lights.push(PointLight {
                    pos: self.e_shots[k].tracer_from,
                    radius: 22.0,
                    color: Vec3::new(1.0, 0.70, 0.34),
                    intensity: 7.0,
                });
            }
        }

        // ── The buttoned-up interior: light through the REAL holes ──
        // Every cut aperture (visor, ports, the cupola slit ring, the periscope shafts) becomes a
        // SUN-DRIVEN point light just inside its opening: bright when the hole faces the sun,
        // fading as the tank/turret turns away — the light physically moves with traverse. Plus the
        // three real lamps (dome / TPZ / ignition). Only worth the slots while you're inside.
        if matches!(self.mode, Mode::Tank) && self.interior_light > 0.02 {
            use crate::interior_rig::{self as rig, Frame};
            let hm = self.player.hull_model();
            let tm = self.player.turret_model();
            let sun = Vec3::new(-0.5, 0.55, 0.25).normalize(); // = renderer SUN_DIR (to the sun)
            let day = 1.0 - if self.night { 1.0 } else { 0.0 };
            for ap in rig::APERTURES {
                let m = if ap.frame == Frame::Hull { &hm } else { &tm };
                let p = m.transform_point3(ap.pos - ap.normal * 0.16); // pulled just inside
                let n = m.transform_vector3(ap.normal).normalize_or_zero();
                // Facing the sun → a blade of daylight; away → a faint skylight; night → a sliver.
                // DIM by design: 14 small holes in a sealed steel box read as pools, not floodlights.
                let sun_dot = n.dot(sun).max(0.0);
                let intensity = day * (0.30 + 3.2 * sun_dot) + (1.0 - day) * 0.08;
                lights.push(PointLight {
                    pos: p,
                    radius: 0.85,
                    color: Vec3::new(0.72, 0.82, 1.00), // cold winter daylight
                    intensity,
                });
            }
            for &(frame, pos, color, base, radius) in rig::LAMPS {
                let m = if frame == Frame::Hull { &hm } else { &tm };
                lights.push(PointLight {
                    pos: m.transform_point3(pos),
                    radius: radius * 0.52,
                    color,
                    intensity: base * 0.30,
                });
            }
        }
        r.set_lights(&lights);
    }

    /// Interior-haze tuning (user's accumulator + fan model). A shot puffs a little; the BREECH
    /// OPEN on reload puffs more; the fan + an open hatch drain it.
    fn update_haze(&mut self, dt: f32) {
        const FAN_DRAIN: f32 = 0.030; // /s — fumes accumulate across a fast engagement
        const HATCH_VENT: f32 = 0.40; // /s — extra draft while the hatch is open
        let drain = FAN_DRAIN + if self.hatch_open { HATCH_VENT } else { 0.0 };
        self.interior_haze = (self.interior_haze - drain * dt).clamp(0.0, 1.0);
    }

    fn update_fire(&mut self, dt: f32) {
        if !self.fire_active {
            return;
        }
        self.fire_t += dt;
        self.fire = (self.fire + dt * FIRE_GROWTH).min(FIRE_MAX);
        // An OPEN hatch vents smoke + flame — the fire climbs slower (the reason to pop it despite
        // the exposure). Enough to matter in the cook-off race, not enough to beat the extinguisher.
        if self.hatch_open {
            self.fire = (self.fire - dt * FIRE_GROWTH * 0.6).max(0.0);
        }
        // The extinguisher (update_activities) can knock `fire` back below the ammo threshold — the
        // moment it does, the cook-off countdown is bought back (cancelled), regardless of who moved
        // `fire`. This is the whole point of the race.
        if self.fire < FIRE_AMMO_THRESH {
            self.fire_ammo_timer = None;
        }
        if self.player.alive && self.fire >= FIRE_AMMO_THRESH && self.fire_ammo_timer.is_none() {
            self.fire_ammo_timer = Some(FIRE_TO_AMMO);
            println!(
                "[kessel] interior FIRE reaching the ammunition — cook-off in {FIRE_TO_AMMO:.0}s"
            );
        }
        if let Some(t) = self.fire_ammo_timer.as_mut() {
            *t -= dt;
            if *t <= 0.0 {
                self.fire_ammo_timer = None;
                if self.player.alive {
                    self.kill_fire_cookoff();
                }
            }
        }
    }

    // ── ABSITZEN headless-verification accessors ──
    pub fn foot_pos(&self) -> Vec3 {
        self.foot.pos
    }
    pub fn dbg_is_foot(&self) -> bool {
        matches!(self.mode, Mode::Foot)
    }
    pub fn player_center(&self) -> Vec3 {
        self.player.center()
    }
    /// Terrain height under the soldier's feet (should equal `foot_pos().y` — proves grounding).
    pub fn foot_ground(&self) -> f32 {
        self.hf.height_at(self.foot.pos.x, self.foot.pos.z)
    }
    pub fn foot_flash_active(&self) -> bool {
        self.foot.flash_t > 0.0
    }
    pub fn supplies_total(&self) -> u32 {
        self.foot.supplies.total()
    }
    pub fn pickups_collected(&self) -> usize {
        self.pickups.iter().filter(|p| p.collected).count()
    }
    /// Number of collected pickups of a specific kind (headless cigarette-cache check).
    pub fn collected_of(&self, kind: PickupKind) -> usize {
        self.pickups
            .iter()
            .filter(|p| p.collected && p.kind == kind)
            .count()
    }
    // ── stealth telemetry (diegetic readout only; there is NO on-screen detection meter) ──
    pub fn is_crouched(&self) -> bool {
        self.foot.crouch_t > 0.5
    }
    pub fn crouch_t(&self) -> f32 {
        self.foot.crouch_t
    }
    pub fn foot_eye_y(&self) -> f32 {
        self.foot.eye().y
    }
    pub fn foot_hp(&self) -> f32 {
        self.foot.hp.max(0.0)
    }
    pub fn banked_total(&self) -> u32 {
        self.banked.total()
    }
    pub fn n_soldiers(&self) -> usize {
        self.soldiers.len()
    }
    pub fn soldiers_alive(&self) -> usize {
        self.soldiers.iter().filter(|s| s.alive).count()
    }
    pub fn soldier_pos(&self, i: usize) -> Vec3 {
        self.soldiers[i].pos
    }
    pub fn soldier_detect(&self, i: usize) -> f32 {
        self.soldiers[i].detect
    }
    pub fn soldier_state(&self, i: usize) -> &'static str {
        self.soldiers[i].alert.name()
    }
    /// Teleport the foot player to a grounded (x, z) — headless-only staging for stealth beats.
    pub fn dbg_place_foot(&mut self, x: f32, z: f32) {
        self.foot.pos = Vec3::new(x, self.hf.height_at(x, z), z);
    }
    /// Force-heal + calm for headless staging between beats (also clears the noise ring so a
    /// lingering shot from a previous beat doesn't force-alarm the next one).
    pub fn dbg_reset_foot(&mut self) {
        self.foot.hp = crate::foot::FOOT_HP;
        self.foot.crouch_t = 0.0;
        self.foot.cooldown = 0.0; // clear the Luger rate-limit between beats
        self.foot.prev_fire = false; // so the next fire is a clean rising edge
        self.noise = [NoiseEvent::default(); 8];
        for s in &mut self.soldiers {
            s.alert = Alert::Unaware;
            s.detect = 0.0;
            s.has_invest = false;
            s.aim_windup = 0.0;
        }
    }
    /// Park soldier `i` at a fixed post facing `(fx, fz)` as a stationary sentry — headless staging
    /// so a stealth beat has clean, cover-free geometry regardless of the seeded patrol.
    pub fn dbg_stage_soldier(&mut self, i: usize, sx: f32, sz: f32, fx: f32, fz: f32) {
        let pos = Vec3::new(sx, self.hf.height_at(sx, sz), sz);
        let s = &mut self.soldiers[i];
        s.pos = pos;
        s.route = [pos; 4];
        s.leg = 0;
        s.alive = true;
        s.alert = Alert::Unaware;
        s.detect = 0.0;
        s.has_invest = false;
        s.aim_windup = 0.0;
        s.fire_cd = 0.0;
        let (dx, dz) = (fx - sx, fz - sz);
        if dx * dx + dz * dz > 1e-6 {
            s.yaw = dx.atan2(-dz);
        }
    }
    /// Set on-foot hp (headless staging for the KIA flow).
    pub fn dbg_set_foot_hp(&mut self, hp: f32) {
        self.foot.hp = hp;
    }

    /// Headless: pose the BUTTONED commander looking `yaw` rad off turret-forward at `pitch`
    /// (0.35 rad up = the periscope ring; ~0 = the compartment) — the `cupola` subcommand's view.
    pub fn dbg_cupola_view(&mut self, yaw: f32, pitch: f32) {
        self.station = Station::Commander;
        self.commander_outside = false;
        self.switch_t = 0.0;
        self.head_yaw = yaw;
        self.head_pitch = pitch;
    }
    /// Point the foot camera (yaw + pitch) at a world target — headless aiming for the Luger beat,
    /// since `face_foot_toward` only sets yaw and a dead-flat ray sails over/under a distant soldier
    /// on sloped terrain.
    pub fn dbg_look_foot_at(&mut self, target: Vec3) {
        let eye = self.foot.eye();
        let to = target - eye;
        let flat = (to.x * to.x + to.z * to.z).sqrt();
        if flat > 1e-4 {
            self.foot.yaw = to.x.atan2(-to.z);
            self.foot.pitch = to.y.atan2(flat);
        }
    }
    /// Aim the soldier's heading at a world (x, z) — used by the headless walk-to-pickup demo.
    pub fn face_foot_toward(&mut self, x: f32, z: f32) {
        let dx = x - self.foot.pos.x;
        let dz = z - self.foot.pos.z;
        if dx * dx + dz * dz > 1e-6 {
            self.foot.yaw = dx.atan2(-dz); // forward = (sin, 0, -cos) points at the target
        }
    }

    /// ABSITZEN: toggle between riding the Tiger and moving on foot. Dismount only when the tank is
    /// alive and (near-)stationary; remount only when standing close to the abandoned hull.
    /// E / bail / spare-reach: START the climb ritual (the actual mount/dismount happens when the
    /// timed sequence finishes — see `update_climb` / `finish_climb`). Ignored mid-climb.
    /// On foot and within reach of a mountable Tiger (a spare, or your own parked hull)? Lets the
    /// front-end route Space → climb-IN without spamming "denied" every time you jump in the open.
    pub fn can_mount(&self) -> bool {
        matches!(self.mode, Mode::Foot)
            && !self.climb.active
            && (self.nearest_spare(6.0).is_some()
                || (self.player.alive && self.foot.pos.distance(self.player.center()) < 6.0))
    }

    pub fn toggle_dismount(&mut self) {
        if self.climb.active {
            return;
        }
        match self.mode {
            // Bailing (even WHILE ON FIRE — the Last Tiger moment) starts the climb OUT the hatch,
            // but you have to OPEN THE HATCH first (you can't climb through a closed lid).
            // BAIL: normally you stop first, BUT a burning Tiger is a death trap — while on fire you
            // bail at ANY speed (the Last Tiger moment; kessel-3act-narrative). A fire bail also
            // slams the hatch open if it was shut — you are not fighting the lid with flames licking up.
            Mode::Tank
                if self.player.alive && (self.player.speed.abs() < 0.6 || self.fire_active) =>
            {
                if !self.hatch_open && self.fire_active {
                    self.hatch_open = true;
                    println!("[kessel] FIRE BAIL — hatch kicked open");
                }
                if self.hatch_open {
                    // SEEN EXITING: silhouetting yourself on the deck is the one hole in the
                    // silent watch — any enemy with a clear line inside its envelope catches
                    // the movement and its meter jumps (close = spotted outright).
                    let deck = self.player.center() + Vec3::Y * 2.2;
                    let env = stealth::detection_radius(0.0, self.night)
                        * if self.engine_on { 1.0 } else { 0.85 }; // eyes don't need ears
                    for i in 0..self.enemies.len() {
                        if !self.enemies[i].alive {
                            continue;
                        }
                        let ec = self.enemies[i].center();
                        let d = ec.distance(deck);
                        if d < env && self.los_clear(ec, deck) {
                            let bump = if d < 220.0 { 1.0 } else { 0.75 };
                            self.stealth.meters[i] = (self.stealth.meters[i] + bump).min(1.0);
                            println!("[kessel] SEEN EXITING — enemy #{i} at {:.0} m caught you on the deck", d);
                        }
                    }
                    self.begin_climb(false, ClimbEnd::Afoot);
                } else {
                    self.hover_label = Some("HATCH IS CLOSED — look at it and CLICK");
                    println!("[kessel] can't bail — the hatch is closed (click it to open)");
                }
            }
            // Reaching an untaken abandoned Tiger on foot → climb IN (the run continues).
            Mode::Foot if self.nearest_spare(6.0).is_some() => {
                let k = self.nearest_spare(6.0).unwrap();
                let (pos, heading, _) = self.abandoned[k];
                let t = Tank::new(pos.x, pos.z, heading, &self.hf, tank::TIGER_PARTS);
                self.begin_climb_on(&t, true, ClimbEnd::IntoSpare(k));
            }
            // Reaching your own parked Tiger on foot → climb IN (EXFIL banks the haul on arrival).
            Mode::Foot
                if self.foot.pos.distance(self.player.center()) < 6.0 && self.player.alive =>
            {
                let t = self.player.clone();
                self.begin_climb_on(&t, true, ClimbEnd::IntoTank);
            }
            _ => {
                println!(
                    "[absitzen] dismount/remount denied (tank moving, or too far to reach one)"
                );
            }
        }
    }

    /// Camera perch positions (eye-height baked in) up the tank `t`, for the mount or dismount ladder.
    fn perch_cams(&self, t: &Tank, mounting: bool, start: Vec3) -> Vec<Vec3> {
        let hull_deck = t.hull_model().transform_point3(Vec3::new(0.55, 1.55, 1.3)) + Vec3::Y * 1.0;
        let turret_roof = t
            .turret_model()
            .transform_point3(Vec3::new(0.0, 2.05, -0.1))
            + Vec3::Y * 0.9;
        let cupola_top = t
            .turret_model()
            .transform_point3(Vec3::new(0.45, 2.35, -0.45))
            + Vec3::Y * 0.45;
        let inside = t.commander_eye();
        // Ground beside the hull (right side), at eye height.
        let g = t.pos + t.right() * 3.2;
        let ground = Vec3::new(g.x, self.hf.height_at(g.x, g.z) + crate::foot::EYE, g.z);
        if mounting {
            // ground(where you stand) → hull deck → turret roof → drop into the hatch
            vec![start, hull_deck, turret_roof, inside]
        } else {
            // Single-crew climb OUT: TWO committed moves — pull up out of the cupola, then over the
            // rim onto the cupola top. You end on foot up top; no scripted jump-down to the ground.
            let _ = (turret_roof, ground); // (former 3rd/4th perches — the jump-down is gone)
            let cupola_pull = t
                .turret_model()
                .transform_point3(Vec3::new(0.45, 2.35, -0.45))
                + Vec3::Y * 0.20;
            vec![inside, cupola_pull, cupola_top]
        }
    }

    fn begin_climb(&mut self, mounting: bool, end: ClimbEnd) {
        let t = self.player.clone();
        self.begin_climb_on(&t, mounting, end);
    }

    fn begin_climb_on(&mut self, t: &Tank, mounting: bool, end: ClimbEnd) {
        let start = if mounting {
            self.foot.eye()
        } else {
            t.commander_eye()
        };
        let cams = self.perch_cams(t, mounting, start);
        // The ritual runs in the Foot arm regardless of direction (the man is on/around the tank).
        self.mode = Mode::Foot;
        self.gunner_optic = false;
        self.foot.hp = self.foot.hp.max(1.0);
        self.foot.airborne = false;
        self.foot.vel_y = 0.0;
        self.climb = Climb {
            active: true,
            mounting,
            step: 0,
            t: 0.0,
            anim: false,
            cams,
            end,
        };
        println!(
            "[kessel] CLIMB {} — {}",
            if mounting { "IN" } else { "OUT" },
            if mounting {
                "reach the hull"
            } else {
                "up out of the cupola"
            }
        );
    }

    /// Advance the climb ritual one frame. Space (jump edge) drives each committed move; the final
    /// descent also accepts a forward step (walk off). Returns having driven the camera.
    fn update_climb(&mut self, input: &TankInput, dt: f32, r: &mut Renderer) {
        // Free mouse-look throughout — only POSITION is scripted.
        self.foot.look(input.look_dx, input.look_dy);
        r.camera.yaw = self.foot.yaw;
        r.camera.pitch = self.foot.pitch;
        r.camera.fov_y = 68f32.to_radians();
        r.reticle = false;
        r.set_visible(self.slots.luger, false);
        r.set_visible(self.slots.luger_flash, false);
        r.set_visible(self.slots.hammer, false);

        let jump_edge = input.jump && !self.prev_jump;
        let n = self.climb.cams.len();
        if self.climb.anim {
            self.climb.t += dt / CLIMB_STEP_S;
            let a = self.climb.step;
            let b = (a + 1).min(n - 1);
            let s = smoothstep(0.0, 1.0, self.climb.t.min(1.0));
            r.camera.pos = self.climb.cams[a].lerp(self.climb.cams[b], s);
            if self.climb.t >= 1.0 {
                self.climb.anim = false;
                self.climb.t = 0.0;
                self.climb.step = b;
                if b == n - 1 {
                    self.finish_climb();
                    return;
                }
            }
        } else {
            r.camera.pos = self.climb.cams[self.climb.step];
            let last_move = self.climb.step == n - 2;
            let walk_off = last_move && !self.climb.mounting && input.drive > 0.3;
            if jump_edge || walk_off {
                self.climb.anim = true;
                self.climb.t = 0.0;
            }
            self.hover_label = Some(climb_prompt(self.climb.mounting, self.climb.step, n));
        }
        // Project the man's ground position under the camera so the crews still track him (exposed!).
        let c = r.camera.pos;
        self.foot.pos = Vec3::new(c.x, self.hf.height_at(c.x, c.z), c.z);
    }

    fn finish_climb(&mut self) {
        let end = self.climb.end;
        self.climb = Climb::default();
        match end {
            ClimbEnd::Afoot => {
                // The old instant exit moved the body to the snow but forgot the authoritative
                // mode flip, so the next frame snapped control/camera straight back into the tank.
                self.mode = Mode::Foot;
                self.commander_outside = false;
                // Drop straight DOWN under the cupola (user 2026-07-08) — you came out the cupola
                // hatch, you land beneath it, not teleported to the hull side.
                let cup = self
                    .player
                    .turret_model()
                    .transform_point3(Vec3::new(0.564, 2.46, -0.42));
                self.foot.pos = Vec3::new(cup.x, self.hf.height_at(cup.x, cup.z), cup.z);
                self.foot.airborne = false;
                self.foot.vel_y = 0.0;
                println!("[absitzen] ON FOOT — clear of the hatch");
            }
            ClimbEnd::IntoTank => {
                self.banked.absorb(&self.foot.supplies);
                self.foot.supplies = crate::foot::Supplies::default();
                self.mode = Mode::Tank;
                println!(
                    "[absitzen] REMOUNT — back in the Tiger (banked total {})",
                    self.banked.total()
                );
            }
            ClimbEnd::IntoSpare(k) => {
                let (pos, heading, _) = self.abandoned[k];
                let old_pose = (self.player.pos, self.player.heading);
                let hulk = Tank::new(
                    old_pose.0.x,
                    old_pose.0.z,
                    old_pose.1,
                    &self.hf,
                    tank::TIGER_PARTS,
                );
                self.pending_hulk =
                    Some((k, hulk.hull_model(), hulk.turret_model(), hulk.gun_model()));
                self.player = Tank::new(pos.x, pos.z, heading, &self.hf, tank::TIGER_PARTS);
                self.modules = default_modules();
                self.fire = 0.0;
                self.fire_active = false;
                self.fire_ammo_timer = None;
                self.breech = Breech::Loaded;
                self.death_cause = None;
                self.last_fatal = None;
                self.abandoned[k].2 = true;
                self.recompute_hp();
                self.banked.absorb(&self.foot.supplies);
                self.foot.supplies = crate::foot::Supplies::default();
                self.mode = Mode::Tank;
                println!("[kessel] ANOTHER CHANCE — you climbed into an abandoned Tiger");
            }
        }
    }

    /// True while the climb ritual is running (front-end: block station input, etc.).
    pub fn is_climbing(&self) -> bool {
        self.climb.active
    }

    /// Toggle the cupola hatch (H, or a click when hovered). Closing it also pulls your head in.
    pub fn toggle_hatch(&mut self) {
        if self.climb.active || !matches!(self.mode, Mode::Tank) {
            return;
        }
        self.hatch_open = !self.hatch_open;
        if !self.hatch_open {
            self.commander_outside = false;
        }
        println!(
            "[kessel] hatch {}",
            if self.hatch_open {
                "OPEN — exposed (fire vents, but so does the danger)"
            } else {
                "CLOSED — buttoned up"
            }
        );
    }
    pub fn hatch_is_open(&self) -> bool {
        self.hatch_open
    }
    pub fn dbg_open_hatch(&mut self) {
        self.hatch_open = true;
    }
    pub fn hover_hatch(&self) -> bool {
        self.hover_hatch
    }

    /// Click the hatch DISC (commander view): toggle it open/closed — push the lid. Clicking the
    /// hatch NEVER exits the tank (the old exit arm ejected the loader who was aiming at the
    /// CUPOLA anchor behind it — 2026-07-11 untangle). Exiting is Space at the open cupola.
    pub fn click_hatch_or_exit(&mut self) {
        self.toggle_hatch();
    }

    /// True when `climb_out` would actually go (drives the X-hold HUD ring: it must never
    /// fill and then do nothing — e.g. rolling too fast to dismount without burning).
    pub fn can_climb_out(&self) -> bool {
        self.hatch_open && self.player.alive && (self.player.speed.abs() < 0.6 || self.fire_active)
    }

    /// The deliberate climb OUT through the open cupola (X held up top / the fire bail). Split
    /// from the hatch click so no stray interior click can put you on the snow.
    pub fn climb_out(&mut self) {
        if self.hatch_open
            && self.player.alive
            && (self.player.speed.abs() < 0.6 || self.fire_active)
        {
            self.climb = Climb {
                end: ClimbEnd::Afoot,
                ..Climb::default()
            };
            self.finish_climb(); // instant: out through the open hatch, on foot beside the hull
            println!(
                "[kessel] AUSGESTIEGEN — {}",
                if self.fire_active && self.player.speed.abs() >= 0.6 {
                    "FIRE BAIL at speed"
                } else {
                    "one click out"
                }
            );
        }
    }

    /// ON FOOT: is the crosshair on the cupola of a mountable Tiger (own or spare) within reach?
    /// Returns the mount target; drives the hover label + the one-click mount.
    fn foot_mount_hover(&self, cam_fwd: Vec3, cam_pos: Vec3) -> Option<ClimbEnd> {
        if !matches!(self.mode, Mode::Foot) || self.climb.active {
            return None;
        }
        let cupola_of = |t: &Tank| {
            t.turret_model()
                .transform_point3(Vec3::new(0.564, 2.46, -0.42))
        };
        // own parked Tiger
        if self.player.alive && self.foot.pos.distance(self.player.center()) < 7.0 {
            let to = (cupola_of(&self.player) - cam_pos).normalize_or_zero();
            if to.dot(cam_fwd) > 0.88 {
                return Some(ClimbEnd::IntoTank);
            }
        }
        // an abandoned spare
        if let Some(k) = self.nearest_spare(7.0) {
            let (pos, heading, _) = self.abandoned[k];
            let t = Tank::new(pos.x, pos.z, heading, &self.hf, tank::TIGER_PARTS);
            let to = (cupola_of(&t) - cam_pos).normalize_or_zero();
            if to.dot(cam_fwd) > 0.88 {
                return Some(ClimbEnd::IntoSpare(k));
            }
        }
        None
    }

    /// LMB on foot: one click on the cupola mounts the tank. Returns true if consumed.
    pub fn click_mount(&mut self, cam_fwd: Vec3, cam_pos: Vec3) -> bool {
        if let Some(end) = self.foot_mount_hover(cam_fwd, cam_pos) {
            self.climb = Climb {
                end,
                ..Climb::default()
            };
            self.finish_climb(); // instant: in through the cupola, commander position
            true
        } else {
            false
        }
    }
    /// LMB on a hovered station anchor → enter that station (click-to-enter). Returns true if the
    /// click was consumed (so the front-end doesn't also treat it as a trigger pull).
    pub fn click_station(&mut self) -> bool {
        if let Some(s) = self.hover_station {
            self.commander_outside = false;
            self.sights_mag = 0;
            self.jump_station(s);
            return true;
        }
        false
    }
    pub fn dbg_outline_hatch(&self, r: &mut Renderer) {
        r.set_outline(self.slots.hatch, 1.5);
    }

    /// Headless: drive the active climb ritual to completion by feeding synthetic Space edges.
    /// Returns the number of committed moves performed.
    pub fn dbg_complete_climb(&mut self, r: &mut Renderer) -> usize {
        let dt = 1.0 / 60.0;
        let mut moves = 0;
        let mut guard = 0;
        while self.climb.active && guard < 1200 {
            let perched = !self.climb.anim;
            let inp = TankInput {
                jump: perched,
                ..Default::default()
            };
            let was_anim = self.climb.anim;
            self.update(&inp, dt, r);
            if !was_anim && self.climb.anim {
                moves += 1;
            } // a move just started
            guard += 1;
        }
        moves
    }

    /// ABSITZEN: drive the dismounted soldier — movement, mouse-look camera, supply collection, and
    /// the held Luger (aim / fire / muzzle-flash). Called once per frame while `Mode::Foot`.
    fn update_foot(&mut self, input: &TankInput, dt: f32, r: &mut Renderer) {
        // While the hand owns the loose track, mouse motion is the drag gesture rather than
        // head-look. Check against the pre-move position so the camera cannot dive toward the
        // ground during a pull stroke.
        let pulling_at_run = self.carrying_track_kit
            && self.track_repair_phase == TrackRepairPhase::PullEnd
            && input.fire2
            && [
                (ModuleKind::LeftTrack, -1.55),
                (ModuleKind::RightTrack, 1.55),
            ]
            .into_iter()
            .any(|(kind, side)| {
                !self.module_ok(kind)
                    && self.foot_pos().distance(
                        self.player
                            .hull_model()
                            .transform_point3(Vec3::new(side, 0.9, -0.4)),
                    ) < 2.4
            });
        // On foot the tank drive/steer are reinterpreted: drive→forward, −steer→strafe (steer is
        // A−D, so strafe-right = D−A = −steer). No second input struct needed.
        self.foot.update(
            input.drive,
            -input.steer,
            input.run,
            input.crouch,
            input.jump && !self.prev_jump, // jump on the Space rising edge
            if pulling_at_run { 0.0 } else { input.look_dx },
            if pulling_at_run { 0.0 } else { input.look_dy },
            dt,
            &self.hf,
        );

        // Camera at human eye height, look driven directly from accumulated mouse deltas.
        r.camera.pos = self.foot.eye();
        r.camera.yaw = self.foot.yaw;
        r.camera.pitch = self.foot.pitch;
        self.foot.aiming = input.aim && !input.binoculars; // RMB raises the Luger
                                                           // BINOCULARS (Left Shift toggle, the Last Tiger scan): service-glass zoom to search
                                                           // the horizon for an abandoned Tiger. Overrides the pistol; no reticle — it's glass, not a gun.
        r.camera.fov_y = if input.binoculars {
            BINOCULAR_FOV_DEG.to_radians()
        } else if self.foot.aiming {
            45.0f32.to_radians()
        } else {
            68.0f32.to_radians()
        };
        r.reticle = self.foot.aiming && self.foot.reload_t <= 0.0 && !input.binoculars;
        // Muzzle-flip: a sharp upward camera kick that decays fast (~0.2 s), plus the pistol pose
        // kick below — the Luger finally pushes back.
        if self.foot.recoil > 0.0 {
            r.camera.pitch += 0.055 * self.foot.recoil;
            self.foot.recoil = (self.foot.recoil - dt * 5.5).max(0.0);
        }
        // Magazine change (R): drop, seat, rack — firing locked out while it runs.
        if self.foot.reload_t > 0.0 {
            self.foot.reload_t -= dt;
            if self.foot.reload_t <= 0.0 {
                let take = (crate::foot::MAG_CAP - self.foot.mag).min(self.foot.ammo);
                self.foot.mag += take;
                self.foot.ammo -= take;
                println!(
                    "[absitzen] reload — mag {}/{} (reserve {})",
                    self.foot.mag,
                    crate::foot::MAG_CAP,
                    self.foot.ammo
                );
                if let Some(a) = &self.audio {
                    a.play_at_volume(crate::audio::Sfx::Pickup, 0.5);
                } // mag seats
            }
        } else if input.reload && self.foot.mag < crate::foot::MAG_CAP && self.foot.ammo > 0 {
            self.foot.reload_t = crate::foot::RELOAD_S;
        }

        // ── Supply scavenging: walk within 1.4 m (planar) of a pickup to collect it.
        for p in &mut self.pickups {
            if !p.collected
                && Vec2::new(self.foot.pos.x - p.pos.x, self.foot.pos.z - p.pos.z).length() < 1.4
            {
                p.collected = true;
                r.set_visible(p.slot, false);
                self.foot.supplies.add(p.kind);
                match p.kind {
                    PickupKind::Ammo => self.foot.ammo += 12, // Luger rounds
                    PickupKind::Medical => {
                        self.foot.hp = (self.foot.hp + 34.0).min(crate::foot::FOOT_HP)
                    }
                    _ => {}
                }
                if let Some(a) = &self.audio {
                    a.play(crate::audio::Sfx::Pickup);
                }
                println!(
                    "[absitzen] picked up {:?} — ammo {} fuel {} med {} cigs {} (hp {:.0})",
                    p.kind,
                    self.foot.supplies.ammo,
                    self.foot.supplies.fuel,
                    self.foot.supplies.medical,
                    self.foot.supplies.cigarettes,
                    self.foot.hp
                );
            }
        }

        // ── The Luger, held in view. World matrix from the camera basis (no view-space pass exists);
        // renders in the normal pass ~0.3 m from the eye — known limitation: clips into close walls
        // and takes fog tint. Acceptable for the slice.
        let eye = r.camera.pos;
        let f = r.camera.forward();
        let rt = f.cross(Vec3::Y).normalize_or_zero();
        let up = rt.cross(f);
        let hip = eye + f * 0.35 + rt * 0.16 - up * 0.20; // lowered at the hip
        let aim = eye + f * 0.30 + rt * 0.00 - up * 0.08; // centered when RMB-aiming
        let mut pistol_pos = if self.foot.aiming { aim } else { hip };
        if self.foot.reload_t > 0.0 {
            pistol_pos -= up * 0.09 - f * 0.02; // dipped out of the eyeline while the mag changes
        }
        pistol_pos -= f * 0.045 * self.foot.recoil; // recoil shove, straight back
        let mut rot = Quat::from_rotation_arc(Vec3::Z, f); // point the model down the view
        if self.foot.recoil > 0.0 {
            rot = Quat::from_axis_angle(rt, -0.22 * self.foot.recoil) * rot; // muzzle flip
        }
        r.set_visible(self.slots.luger, !input.binoculars);
        r.set_model(
            self.slots.luger,
            Mat4::from_scale_rotation_translation(Vec3::splat(1.0), rot, pistol_pos),
        );

        // ── Fire (LMB rising edge, rate-limited by cooldown, gated on rounds).
        let fire_edge = input.fire2 && !self.foot.prev_fire;
        self.foot.prev_fire = input.fire2;

        // ── TRACK REPAIR ritual: the thrown run is physical work, not a repair progress bar.
        //  1. click the loose end,
        //  2. hold LMB and repeatedly pull the mouse DOWN to drag it over the road wheels,
        //  3. click the fresh shoe into the open break,
        //  4. land four discrete hammer blows to drive the connecting pin home.
        // Progress survives retreating from the exposed flank. LMB belongs to the ritual anywhere
        // near the break, so an accidental Luger round can never skip or contaminate a phase.
        const HAMMER_BLOWS: u8 = 4;
        let broken_track = if !self.module_ok(ModuleKind::LeftTrack) {
            Some((ModuleKind::LeftTrack, -1.55))
        } else if !self.module_ok(ModuleKind::RightTrack) {
            Some((ModuleKind::RightTrack, 1.55))
        } else {
            None
        };
        let mut at_break = false;
        if self.carrying_track_kit {
            if let Some((kind, side)) = broken_track {
                let anchor = self
                    .player
                    .hull_model()
                    .transform_point3(Vec3::new(side, 0.9, -0.4));
                if self.foot_pos().distance(anchor) < 2.4 {
                    at_break = true;
                    match self.track_repair_phase {
                        TrackRepairPhase::GrabEnd => {
                            self.hover_label = Some("LOSES KETTENENDE GREIFEN [LMB]");
                            if fire_edge {
                                self.track_repair_phase = TrackRepairPhase::PullEnd;
                                println!(
                                    "[kessel] KETTE — loses Ende gegriffen; über die Laufrollen ziehen"
                                );
                            }
                        }
                        TrackRepairPhase::PullEnd => {
                            self.hover_label = Some(match (self.track_pull * 4.0) as u8 {
                                0 => "KETTE ZIEHEN [LMB + Maus nach unten]",
                                1 => "KETTE 25% — WEITER ZIEHEN [LMB + Maus ab]",
                                2 => "KETTE 50% — WEITER ZIEHEN [LMB + Maus ab]",
                                _ => "KETTE 75% — NOCH EIN ZUG [LMB + Maus ab]",
                            });
                            if input.fire2 {
                                // A committed downward hand stroke advances the run. Upward mouse
                                // travel is the player's reset stroke and therefore never undoes it.
                                // Cap each frame so a noisy mouse packet cannot teleport the chain.
                                let stroke = (input.look_dy.max(0.0) * 0.010).min(0.16);
                                self.track_pull = (self.track_pull + stroke).min(1.0);
                                if self.track_pull >= 1.0 {
                                    self.track_repair_phase = TrackRepairPhase::FitLink;
                                    println!(
                                        "[kessel] KETTE — loser Lauf liegt wieder über den Rollen"
                                    );
                                }
                            }
                        }
                        TrackRepairPhase::FitLink => {
                            self.hover_label = Some("ERSATZGLIED IN DIE LÜCKE SETZEN [LMB]");
                            if fire_edge {
                                self.track_repair_phase = TrackRepairPhase::HammerPin;
                                self.track_hits = 0;
                                if let Some(a) = &self.audio {
                                    a.play_at_volume(crate::audio::Sfx::BrassClatter, 0.45);
                                }
                                println!(
                                    "[kessel] KETTE — Ersatzglied aus dem Satz eingesetzt; Bolzen eintreiben"
                                );
                            }
                        }
                        TrackRepairPhase::HammerPin => {
                            self.hover_label = Some(match HAMMER_BLOWS - self.track_hits {
                                1 => "KETTENBOLZEN: noch 1 Schlag [LMB]",
                                2 => "KETTENBOLZEN: noch 2 Schläge [LMB]",
                                3 => "KETTENBOLZEN: noch 3 Schläge [LMB]",
                                _ => "KETTENBOLZEN EINTREIBEN [LMB]",
                            });
                            if fire_edge && self.hammer_t <= 0.0 {
                                self.hammer_t = 0.38; // wind up + strike
                            }
                        }
                    }
                }
                if self.hammer_t > 0.0 {
                    let was = self.hammer_t;
                    self.hammer_t -= dt;
                    // the STRIKE lands mid-swing: clang + one link driven home
                    if was > 0.14 && self.hammer_t <= 0.14 {
                        self.track_hits += 1;
                        if let Some(a) = &self.audio {
                            a.play_at_volume(crate::audio::Sfx::HammerStrike, 0.9);
                        }
                        if self.track_hits >= HAMMER_BLOWS {
                            if let Some(m) = self.modules.iter_mut().find(|m| m.kind == kind) {
                                m.functional = true;
                            }
                            self.carrying_track_kit = false;
                            self.track_repair_phase = TrackRepairPhase::GrabEnd;
                            self.track_pull = 0.0;
                            self.track_hits = 0;
                            self.hammer_t = 0.0;
                            self.recompute_hp();
                            println!(
                                "[kessel] KETTE — neuer Bolzen sitzt, Kette wieder auf ({})",
                                kind.name()
                            );
                        }
                    }
                }
            }
        }

        // The whole ritual holsters the Luger. The hammer itself appears only once the fresh link
        // is seated; the pull phases leave the hands clear against the moving track run.
        if at_break {
            r.set_visible(self.slots.luger, false);
        }
        if (at_break && self.track_repair_phase == TrackRepairPhase::HammerPin)
            || self.hammer_t > 0.0
        {
            r.set_visible(self.slots.hammer, true);
            let swing = if self.hammer_t > 0.0 {
                let t = self.hammer_t / 0.38;
                if t > 0.37 {
                    (1.0 - t) * 1.9
                } else {
                    1.2 - (0.37 - t) * 5.5
                } // wind up, then slam
            } else {
                0.35
            };
            let f = r.camera.forward();
            let rgt = r.camera.right();
            let hp = r.camera.pos + f * 0.45 + rgt * 0.22 - Vec3::new(0.0, 0.16, 0.0);
            let pose = glam::Quat::from_axis_angle(rgt, -swing)
                * glam::Quat::from_rotation_arc(Vec3::Z, f);
            r.set_model(self.slots.hammer, Mat4::from_rotation_translation(pose, hp));
        } else {
            r.set_visible(self.slots.hammer, false);
        }

        if fire_edge
            && !at_break
            && self.foot.cooldown <= 0.0
            && self.foot.mag > 0
            && self.foot.reload_t <= 0.0
        {
            self.foot.cooldown = 0.25;
            self.foot.flash_t = 0.05;
            self.foot.mag -= 1;
            self.foot.recoil = 1.0;
            // Hitscan from the eye (finite max so ray_terrain's march always terminates).
            let (o, d) = (r.camera.pos, r.camera.forward());
            let mut best = 1500.0f32;
            let mut which: Option<usize> = None;
            for (i, e) in self.enemies.iter().enumerate() {
                if e.alive {
                    if let Some(t) = ray_sphere(o, d, e.center(), 3.6) {
                        if t < best {
                            best = t;
                            which = Some(i);
                        }
                    }
                }
            }
            // Soldier pass BEFORE terrain — the hit still needs to REGISTER (stagger + feedback)
            // or the lie would read as a bug. What it never does is kill (see below).
            let mut hit_soldier: Option<usize> = None;
            for (i, s) in self.soldiers.iter().enumerate() {
                if !s.alive {
                    continue;
                }
                // Small-arms near cull (the tank helper's GUN_LEN cull whiffed at knife range),
                // and the same cover cylinders that break a soldier's SIGHT also stop BULLETS —
                // no instakills straight through a wall cluster.
                if let Some(t) =
                    ray_sphere_near(o, d, s.pos + Vec3::Y * crate::infantry::EYE_H, 0.6, 0.25)
                {
                    if t < best
                        && !crate::infantry::segment_blocked(
                            o,
                            s.pos + Vec3::Y * crate::infantry::EYE_H,
                            &self.hf,
                            &self.cover,
                        )
                    {
                        best = t;
                        hit_soldier = Some(i);
                    }
                }
            }
            if let Some(t) = ray_terrain(o, d, &self.hf, best) {
                if t < best {
                    best = t;
                    which = None;
                    hit_soldier = None;
                }
            }
            if let Some(i) = hit_soldier {
                // THE LUGER LIE (design 2026-07-07): the round HITS — he staggers, drops for a
                // beat, and gets back up. It kills NOTHING, ever, and nothing in the game says
                // so. The pistol exists so you think you might need it; the noise event below
                // makes firing it actively worse than useless. Evasion was always the answer.
                self.soldiers[i].flinch_t = 1.2;
                println!("[absitzen] LUGER hit → soldier #{i} staggers … and gets back up ({:.0} m, mag {}/{})", best, self.foot.mag, crate::foot::MAG_CAP);
            } else {
                println!(
                    "[absitzen] LUGER fire → {:?} at {:.0} m (mag {}/{} + {})",
                    which,
                    best,
                    self.foot.mag,
                    crate::foot::MAG_CAP,
                    self.foot.ammo
                );
            }
            if let Some(a) = &self.audio {
                a.play(crate::audio::Sfx::LugerShot);
            }
            // Firing is LOUD — one big noise event at the muzzle flips every nearby patrol to
            // Suspicious and anyone with LOS to Alarmed. Evasion is the survival path.
            let shot_pos = self.foot.pos;
            self.push_noise(shot_pos, 120.0, 3.0);
            self.propagate_alarm(shot_pos);
        }

        // ── SCOUT MARKING (the dismounted-recon loop): glasses up, hold on an enemy tank for a
        // beat, and it's inked onto the paper map — a position SNAPSHOT (the ink doesn't follow
        // a tank that moves; that staleness is the game). Marks feed the plan, the Tiger executes.
        if input.binoculars {
            let eye = r.camera.pos;
            let look = r.camera.forward();
            let mut best: Option<(usize, f32)> = None; // (enemy, cos) — tightest to centre wins
            for (i, e) in self.enemies.iter().enumerate() {
                if !e.alive {
                    continue;
                }
                let to = e.center() - eye;
                let dist = to.length();
                if dist > 900.0 {
                    continue;
                }
                let cos = to.normalize_or_zero().dot(look);
                if cos > 0.9990
                    && cos > best.map(|(_, c)| c).unwrap_or(0.0)
                    && self.los_clear(eye, e.center())
                {
                    best = Some((i, cos));
                }
            }
            match (best, self.mark_dwell) {
                (Some((i, _)), Some((j, t))) if i == j => {
                    let t = t + dt;
                    if t >= 0.8 {
                        let pos = glam::Vec2::new(self.enemies[i].pos.x, self.enemies[i].pos.z);
                        self.marks.retain(|m| m.distance(pos) > 40.0); // re-mark supersedes old ink
                        self.marks.push(pos);
                        self.hover_label = Some("MARKED ON THE MAP");
                        println!(
                            "[absitzen] MARKED — enemy #{i} inked at ({:.0}, {:.0}); {} marks",
                            pos.x,
                            pos.y,
                            self.marks.len()
                        );
                        self.mark_dwell = None;
                    } else {
                        self.hover_label = Some("HOLD THE GLASSES ON HIM …");
                        self.mark_dwell = Some((i, t));
                    }
                }
                (Some((i, _)), _) => self.mark_dwell = Some((i, 0.0)),
                (None, _) => self.mark_dwell = None,
            }
        } else {
            self.mark_dwell = None;
        }

        // Muzzle flash box at the barrel tip while it's hot.
        if self.foot.flash_t > 0.0 {
            let tip = pistol_pos + f * 0.22;
            r.set_visible(self.slots.luger_flash, true);
            r.set_emissive(self.slots.luger_flash, 8.0);
            r.set_model(
                self.slots.luger_flash,
                Mat4::from_scale_rotation_translation(Vec3::splat(0.10), Quat::IDENTITY, tip),
            );
        } else {
            r.set_visible(self.slots.luger_flash, false);
        }
    }

    /// Push a discrete loud event into the noise ring (round-robin, oldest overwritten).
    fn push_noise(&mut self, pos: Vec3, strength: f32, ttl: f32) {
        self.noise[self.next_noise] = NoiseEvent { pos, strength, ttl };
        self.next_noise = (self.next_noise + 1) % self.noise.len();
    }

    /// Cascade awareness: every living soldier within `ALARM_RADIUS` of the disturbance jumps to
    /// (at least) Suspicious and turns toward `at`. One blown encounter lights up the neighbourhood.
    fn propagate_alarm(&mut self, at: Vec3) {
        for s in &mut self.soldiers {
            if s.alive && Vec2::new(s.pos.x - at.x, s.pos.z - at.z).length() < ALARM_RADIUS {
                s.raise_alarm(at);
            }
        }
    }

    /// ABSITZEN infantry sim: run each soldier's vision/hearing/state machine, cascade alarms,
    /// resolve return fire against the foot player, then push soldier + muzzle-flash transforms.
    /// Foot death is handled at the tail.
    fn update_infantry(&mut self, input: &TankInput, dt: f32, r: &mut Renderer) {
        // Age the noise ring.
        for ev in &mut self.noise {
            if ev.ttl > 0.0 {
                ev.ttl -= dt;
            }
        }

        let player_eye = self.foot.eye();
        let moving = input.drive.abs() > 0.01 || input.steer.abs() > 0.01;
        let running = input.run && !input.crouch;
        let prof = self.foot.profile(moving, running);

        // Step every soldier. Collect shots + alarm cascades, resolving after the borrow ends.
        let n = self.soldiers.len();
        for i in 0..n {
            let shot =
                self.soldiers[i].update(player_eye, prof, &self.noise, &self.cover, &self.hf, dt);
            if self.soldiers[i].alarm_signal {
                self.soldiers[i].alarm_signal = false;
                let at = self.soldiers[i].invest;
                for j in 0..n {
                    if j != i
                        && self.soldiers[j].alive
                        && Vec2::new(self.soldiers[j].pos.x - at.x, self.soldiers[j].pos.z - at.z)
                            .length()
                            < ALARM_RADIUS
                    {
                        self.soldiers[j].raise_alarm(at);
                    }
                }
                let source = self.soldiers[i].pos + Vec3::Y * 1.5;
                self.schedule_sound_from(crate::audio::Sfx::AlarmShout, source, 1.0);
                println!("[absitzen] ALARM — soldier #{i} fully detected you and raised the alarm");
            }
            if let Some(s) = shot {
                self.resolve_infantry_shot(i, s);
            }
        }

        // Age the infantry muzzle-flash pool.
        for k in 0..INF_FX {
            let slot = self.slots.inf_flash[k];
            if self.inf_flash_t[k] > 0.0 {
                self.inf_flash_t[k] -= dt;
                r.set_visible(slot, true);
                r.set_emissive(slot, 8.0);
                r.set_model(
                    slot,
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(0.16),
                        Quat::IDENTITY,
                        self.inf_flash_pos[k],
                    ),
                );
            } else {
                r.set_visible(slot, false);
            }
        }

        // Push soldier transforms: body capsule + rifle (raised/level while Alarmed, else slung).
        for s in &self.soldiers {
            r.set_visible(s.body_slot, s.alive);
            r.set_visible(s.rifle_slot, s.alive);
            if !s.alive {
                continue;
            }
            let fwd = s.forward();
            let rt = Vec3::new(s.yaw.cos(), 0.0, s.yaw.sin());
            let body_rot = Quat::from_rotation_arc(Vec3::Z, fwd);
            r.set_model(
                s.body_slot,
                Mat4::from_scale_rotation_translation(Vec3::ONE, body_rot, s.pos),
            );
            let (rifle_pos, rifle_dir) = if s.alert == Alert::Alarmed {
                (s.pos + Vec3::Y * 1.35 + fwd * 0.35 + rt * 0.12, fwd) // shouldered, levelled
            } else {
                (s.pos + Vec3::Y * 1.0 - rt * 0.16, rt) // slung across the back
            };
            let rifle_rot = Quat::from_rotation_arc(Vec3::Z, rifle_dir);
            r.set_model(
                s.rifle_slot,
                Mat4::from_scale_rotation_translation(Vec3::ONE, rifle_rot, rifle_pos),
            );
        }

        self.check_foot_death();
    }

    /// Resolve one infantry shot against the foot player: LOS-gated `ray_sphere`, `INF_DAMAGE` on a
    /// hit, plus a muzzle-flash so the shot reads on screen.
    fn resolve_infantry_shot(&mut self, i: usize, shot: crate::infantry::InfantryShot) {
        let target = self.foot.eye();
        // Small-arms near cull (0.25 m): a soldier who has hunted to arm's length must still be
        // able to connect — the tank helper's GUN_LEN cull blanked everything inside ~2.4 m.
        if let Some(t) = ray_sphere_near(shot.from, shot.dir, target, 0.6, 0.25) {
            if t <= shot.range {
                // Not blocked by terrain before it reaches the player?
                let blocked = ray_terrain(shot.from, shot.dir, &self.hf, t).is_some();
                if !blocked {
                    if !self.god {
                        self.foot.hp -= INF_DAMAGE;
                    }
                    println!(
                        "[absitzen] soldier #{i} HIT you for {:.0} (hp {:.0})",
                        INF_DAMAGE,
                        self.foot.hp.max(0.0)
                    );
                }
            }
        }
        // Muzzle flash at the soldier's rifle regardless of hit.
        let k = self.next_inf_fx;
        self.next_inf_fx = (self.next_inf_fx + 1) % INF_FX;
        self.inf_flash_t[k] = 0.06;
        self.inf_flash_pos[k] = shot.from + shot.dir * 0.4;
        self.schedule_sound_from(crate::audio::Sfx::RifleShot, shot.from, 1.0);
    }

    /// KIA on foot is REAL death now (the Last Tiger rule: bail out, run, reach another Tiger —
    /// or die in the snow). No more free respawns; the front-end reads `game_over()`.
    fn check_foot_death(&mut self) {
        if self.foot.hp > 0.0 {
            return;
        }
        if self.death_cause.is_none() {
            self.death_cause = Some(DeathCause::OnFoot);
            println!(
                "[absitzen] KIA on foot — the run ends in the open ({})",
                DeathCause::OnFoot.de()
            );
        }
    }

    /// The run is over when the man is dead: killed IN the tank, or shot down on foot. Sandbox play
    /// retains the Last Tiger replacement-hull escape; the authored campaign is explicitly the same
    /// scarred vehicle across all three acts, so losing that Tiger while bailed out resolves the
    /// checkpoint instead of leaving a living crewman permanently stranded with no legal mount.
    pub fn game_over(&self) -> Option<DeathCause> {
        match self.mode {
            Mode::Tank if !self.player.alive => {
                Some(self.death_cause.unwrap_or(DeathCause::CrewLost))
            }
            Mode::Foot if self.foot.hp <= 0.0 => Some(DeathCause::OnFoot),
            Mode::Foot if self.campaign.is_some() && !self.player.alive => {
                Some(self.death_cause.unwrap_or(DeathCause::FuelFire))
            }
            _ => None,
        }
    }

    /// Headless: the untaken spare-Tiger positions (the Last Tiger verifier reads these).
    pub fn abandoned_positions(&self) -> Vec<Vec3> {
        self.abandoned
            .iter()
            .filter(|(_, _, t)| !t)
            .map(|(p, _, _)| *p)
            .collect()
    }

    /// Nearest UNTAKEN abandoned Tiger within `r` metres of the foot player.
    fn nearest_spare(&self, r: f32) -> Option<usize> {
        let fp = self.foot.pos;
        self.abandoned
            .iter()
            .enumerate()
            .filter(|(_, (p, _, taken))| !taken && p.distance(fp) < r)
            .min_by(|a, b| a.1 .0.distance(fp).total_cmp(&b.1 .0.distance(fp)))
            .map(|(i, _)| i)
    }

    /// Index of the nearest living enemy to the player, or None if the swarm is wiped out.
    fn nearest_enemy(&self) -> Option<usize> {
        let p = self.player.center();
        self.enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| e.alive)
            .min_by(|(_, a), (_, b)| {
                a.center()
                    .distance_squared(p)
                    .total_cmp(&b.center().distance_squared(p))
            })
            .map(|(i, _)| i)
    }

    /// Lay the gun directly on the nearest living enemy (auto-aim, used by the headless fire demo).
    /// The player does this by hand with the arrow keys. No-op if the swarm is already dead.
    pub fn aim_at_enemy(&mut self) {
        let i = match self.nearest_enemy() {
            Some(i) => i,
            None => return,
        };
        let hull = self.player.hull_model();
        let pivot = hull.transform_point3(self.player.parts.turret_pivot);
        let aim = (self.enemies[i].center() - pivot).normalize();
        let local = hull.inverse().transform_vector3(aim);
        self.player.turret_yaw = local.x.atan2(local.z);
        self.player.gun_pitch = local.y.clamp(-0.999, 0.999).asin().clamp(-0.139, 0.261);
        // Keep the mouse-lay target in step so the chase doesn't unwind a scripted lay.
        self.lay_yaw = self.player.turret_yaw;
        self.lay_pitch = self.player.gun_pitch;
    }

    /// Rest the loader's crosshair on the READY RACK from wherever he stands (headless demos). The
    /// free-look interior made the Loader's default gaze turret-forward, which no longer lands on
    /// the rack — a player turns his head first; scripted demos do it through the same
    /// `head_yaw`/`head_pitch` the mouse drives, then the grab-lift-flick runs the real input path.
    /// Headless: is a round riding the hands? / the hot case? / rack rounds left / floor shells.
    pub fn debug_carrying(&self) -> bool {
        self.grab
    }
    pub fn debug_casing_carried(&self) -> bool {
        self.carried_casing.is_some()
    }
    pub fn debug_rack_left(&self) -> usize {
        self.slot_taken.iter().filter(|t| !**t).count()
    }
    pub fn debug_loose_count(&self) -> usize {
        self.loose.iter().filter(|s| s.active).count()
    }

    /// Headless: lever state + gaze helper for the breech-lever choreography.
    pub fn debug_lever(&self) -> f32 {
        self.lever_open
    }
    pub fn debug_loader_face_lever(&mut self) {
        self.loader_face_world(self.player.gun_model().transform_point3(GUNLEVER_POS));
    }

    /// Headless: turn the loader's gaze onto the SPENT-CASE anchor in the breech (the pull).
    pub fn debug_loader_face_breech(&mut self) {
        self.loader_face_world(
            self.player
                .turret_model()
                .transform_point3(Vec3::new(0.1, 1.88, 0.15)),
        );
    }

    /// Headless: line the crosshair up with the authored shell handoff pose for the forward ram.
    pub fn debug_loader_face_tray(&mut self) {
        self.loader_face_world(self.player.turret_model().transform_point3(TRAY_MOUTH));
    }

    /// Headless: aim the interior gaze at a world point (solves head_yaw/pitch off the turret base).
    fn loader_face_world(&mut self, target: Vec3) {
        let eye = self.station_eye(Station::Loader);
        let to = (target - eye).normalize_or_zero();
        let tf = self.player.turret_forward();
        let wrap = |a: f32| {
            (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        self.head_yaw = wrap(to.x.atan2(-to.z) - tf.x.atan2(-tf.z));
        self.head_pitch =
            (to.y.clamp(-0.999, 0.999).asin() - tf.y.clamp(-0.999, 0.999).asin()).clamp(-1.2, 1.0);
    }

    /// Headless: aim the interior gaze at a TURRET-LOCAL point (verification frames).
    pub fn debug_loader_face_local(&mut self, v: Vec3) {
        self.loader_face_world(self.player.turret_model().transform_point3(v));
    }

    pub fn debug_loader_face_rack(&mut self) {
        // Face the NEAREST still-stowed shell (per-slot grabbing — the fixed RACK_SPOT is legacy).
        let eye = self.station_eye(Station::Loader);
        let tm = self.player.turret_model();
        let hm = self.player.hull_model();
        let target = crate::interior_rig::SHELL_SLOTS
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.slot_taken[*i])
            .map(|(_, (frame, pos, _))| {
                let m = if *frame == crate::interior_rig::Frame::Hull {
                    hm
                } else {
                    tm
                };
                m.transform_point3(*pos)
            })
            .min_by(|a, b| {
                a.distance_squared(eye)
                    .partial_cmp(&b.distance_squared(eye))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or_else(|| tm.transform_point3(RACK_SPOT));
        self.loader_face_world(target);
    }

    #[allow(dead_code)]
    fn debug_loader_face_rack_legacy(&mut self) {
        let eye = self.station_eye(Station::Loader);
        let to = (self.player.turret_model().transform_point3(RACK_SPOT) - eye).normalize_or_zero();
        // The Loader camera is set_forward(turret_forward) then offset by head_yaw/head_pitch
        // (see the per-station camera in update); solve the offsets against that base.
        let tf = self.player.turret_forward();
        let wrap = |a: f32| {
            (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        self.head_yaw = wrap(to.x.atan2(-to.z) - tf.x.atan2(-tf.z));
        self.head_pitch =
            (to.y.clamp(-0.999, 0.999).asin() - tf.y.clamp(-0.999, 0.999).asin()).clamp(-1.2, 1.0);
    }

    /// Force a shot regardless of reload/travel-lock (headless verification). Seats a round + posts
    /// the player at the gunner's seat first, so combat telemetry keeps firing under the manual-reload
    /// model (the same reconciliation `debug_reload` provides for the playtest loop).
    pub fn debug_fire(&mut self, r: &mut Renderer) {
        self.debug_reload();
        self.aim_at_enemy();
        self.fire(r);
        self.player.fired();
        self.on_fired();
    }

    /// Headless hook: fire along the CURRENT gun lay (no auto-aim) — for demos shooting at
    /// something other than a tank (a tree, a wall).
    pub fn debug_fire_laid(&mut self, r: &mut Renderer) {
        self.debug_reload();
        self.fire(r);
        self.player.fired();
        self.on_fired();
    }

    /// Land the player's in-flight round NOW: apply its outcome (kill / clang / ground), chip the
    /// struck destructible, and retire the tracer slot. Called when the tracer's flight completes
    /// (update_fx) and by `fire()` when a follow-up shot needs the slot while a round is still
    /// flying — overwriting silently deleted the first round's damage.
    fn land_player_shot(&mut self, r: &mut Renderer) {
        self.fx.tracer_active = false;
        r.set_visible(self.slots.tracer, false);
        self.fx.impact_pos = self.fx.tracer_to;
        let outcome = self.fx.pending_outcome;
        self.fx.impact_kind = outcome;
        self.fx.impact_t = match outcome {
            Outcome::Penetrate => 0.20,
            Outcome::Bounce => 0.11,
            _ => 0.16,
        };
        // Impact is DOWNRANGE (at the target/ground): seen now, heard a beat later — delayed
        // + attenuated by range from the player to the impact point via the speed-of-sound queue.
        let ipos = self.fx.impact_pos;
        match outcome {
            Outcome::Penetrate => {
                self.schedule_sound_from(crate::audio::Sfx::ImpactTank, ipos, VOL_IMPACT_TANK);
                self.schedule_sound_from(crate::audio::Sfx::PenetrationHit, ipos, VOL_PENETRATION);
                if let Some(idx) = self.fx.pending_target {
                    println!(
                        "[kessel] {} #{idx} destroyed.",
                        self.enemies[idx].kind.name()
                    );
                    // The kill payoff: burning wreck, ~35% turret-pop, scheduled cook-off,
                    // big fireball + debris + kill/turret-pop explosion audio (also delayed).
                    self.destroy_enemy(idx);
                }
            }
            Outcome::Bounce => {
                // The player's round glanced a well-angled T-34 — CLANG, it survives.
                let c = crate::audio::Sfx::ARMOR_CLANGS;
                let pick = c[(hash01(self.armor_tick.wrapping_add(31)) * c.len() as f32) as usize
                    % c.len()];
                self.schedule_sound_from(pick, ipos, VOL_CLANG);
            }
            _ => {
                self.schedule_sound_from(crate::audio::Sfx::ImpactGround, ipos, VOL_IMPACT_GROUND);
            }
        }
        self.fx.pending_target = None;
        // ── Sweep any pen-ray freight the in-flight pass didn't reach (a shot landed early by a
        // follow-up fire() call, or float edge cases at the terminal point).
        for (si, dist) in std::mem::take(&mut self.fx.pending_transits) {
            let (entry, dir) = route_at(&self.fx.tracer_route, dist);
            self.chunks.hole(si, entry, dir, r);
            self.damage_structure(si, STRUCT_AP_HOLE_DAMAGE, r);
        }
        for (ti, dir, _) in std::mem::take(&mut self.fx.pending_trees) {
            let mut ev = Vec::new();
            self.trees.shot(ti, dir, &mut ev);
            self.route_tree_events(ev);
        }
        // The wall the round TERMINATED in (AP budget died there): the bigger bite. When the round
        // was HE the radial blast below does the demolition instead of a flat chip.
        if let Some(si) = self.fx.pending_struct.take() {
            if self.fx.pending_he.is_none() {
                self.damage_structure(si, STRUCT_AP_TERMINAL_DAMAGE, r);
            }
        }
        // A terminal tree hit (HE burst on a trunk, or an AP round whose route ended there).
        if let Some(ti) = self.fx.pending_tree.take() {
            let dir = self.fx.tracer_dir;
            let mut ev = Vec::new();
            self.trees.shot(ti, dir, &mut ev);
            self.route_tree_events(ev);
        }
        // ── HE burst: radial structure damage with falloff, trees snapped outward, sympathetic
        // dust shaken off everything standing in 2× the radius (docs/DESTRUCTION.md §1/§3).
        if let Some((bp, rad)) = self.fx.pending_he.take() {
            for i in 0..self.destructibles.len() {
                let dd = self.destructibles[i];
                if !dd.alive {
                    continue;
                }
                let cx = bp.clamp(dd.min, dd.max);
                let dist = cx.distance(bp);
                if dist < rad {
                    self.chunks.blast(i, bp, rad, r); // blow the sphere out of the grid first
                    self.damage_structure(i, HE_STRUCT_DAMAGE * (1.0 - dist / rad), r);
                } else if dist < rad * 2.0 {
                    self.spawn_dust(dd.center() + Vec3::new(0.0, 0.8, 0.0), 0.35);
                }
            }
            let mut ev = Vec::new();
            self.trees.blast(bp, rad, &mut ev);
            self.route_tree_events(ev);
            self.spawn_dust(bp, 1.0);
            self.spawn_debris(
                bp + Vec3::new(0.0, 0.5, 0.0),
                6,
                8.0,
                0x4E_B145 ^ self.armor_tick.wrapping_mul(31),
            );
            self.schedule_sound_from(crate::audio::Sfx::KillExplosion, bp, VOL_KILL * 0.8);
        }
    }

    /// Headless hook: instantly seat a round + post at the gunner's seat, so a forced shot can fire
    /// without hand-cranking a reload (used by `debug_fire` and the playtest's fire policy).
    pub fn debug_reload(&mut self) {
        self.breech = Breech::Loaded;
        self.lever_open = 0.0; // headless force: breech closed and ready
        self.ram = 0.0;
        self.station = Station::Gunner;
        self.switch_t = 0.0;
        self.activity = None;
        self.player.reload = 0.0;
    }

    /// Headless hook: light an interior fuel fire just under the ammo threshold (the extinguisher
    /// race — `firefight` demo). Records a plausible fire origin for the fatal record on cook-off.
    pub fn debug_start_fire(&mut self) {
        self.fire_active = true;
        self.fire = self.fire.max(0.50);
        self.fire_t = 0.0;
        let c = self.player.center();
        self.fire_origin = Some((c + Vec3::new(-60.0, 1.0, 0.0), c, Vec3::X));
    }

    /// Headless hook: knock out a module so the interactive repair path can be exercised.
    pub fn debug_break_module(&mut self, kind: ModuleKind) {
        if let Some(m) = self.modules.iter_mut().find(|m| m.kind == kind) {
            m.functional = false;
        }
        self.recompute_hp();
    }

    /// Headless hook: force EVERY platoon straight to Engaged (and mark all living enemies as
    /// spotters) so the swarm lays on and fires the Tiger immediately — the deterministic contact the
    /// driving player triggers in the real game. Used by the `su100` overwatch-sniper demo to capture
    /// the SU-100 return fire without waiting on the stealth accrual to develop.
    /// Executor observation for enemy `i` — egocentric, normalized to ~[-1,1] (the doc's §2 at
    /// smoke-test width). Partial observability: the Tiger's true position is only fed while THIS
    /// tank tracks it (spotted / platoon Engaged); otherwise the slot is zeroed with a blind flag.
    pub fn executor_obs(&self, i: usize) -> [f32; crate::ai::OBS] {
        let e = &self.enemies[i];
        let tracks = matches!(self.enemy_mode(i), AiMode::Track);
        let pc = self.player.center();
        let ec = e.center();
        let to = pc - ec;
        let range = Vec2::new(to.x, to.z).length();
        // Bearing error: where the Tiger is relative to MY hull heading.
        let bearing = to.x.atan2(to.z) - e.heading;
        let (dxn, dzn, rn) = if tracks {
            (
                (to.x / 300.0).clamp(-1.0, 1.0),
                (to.z / 300.0).clamp(-1.0, 1.0),
                (range / 300.0).min(1.0),
            )
        } else {
            (0.0, 0.0, 0.0) // blind: no fix on the Tiger
        };
        // Nearest living ally, relative.
        let mut ally = Vec2::ZERO;
        let mut best = f32::INFINITY;
        for (j, o) in self.enemies.iter().enumerate() {
            if j != i && o.alive {
                let d = Vec2::new(o.pos.x - e.pos.x, o.pos.z - e.pos.z);
                if d.length() < best {
                    best = d.length();
                    ally = d / 100.0;
                }
            }
        }
        [
            dxn,
            dzn,
            rn,
            if tracks { bearing.sin() } else { 0.0 },
            if tracks { bearing.cos() } else { 0.0 },
            e.speed / tank::TOP_SPEED,
            (e.reload / ENEMY_RELOAD).clamp(0.0, 1.0),
            if e.kind == EnemyKind::T34 { 1.0 } else { -1.0 },
            if tracks && self.los_clear(ec + Vec3::Y * 1.5, pc) {
                1.0
            } else {
                -1.0
            },
            if tracks { 1.0 } else { -1.0 }, // blind flag (doubles as alert level at this width)
            // The Tiger facing I expose myself to: dot(my bearing FROM the tiger, tiger forward)
            // — +1 = I'm in his frontal arc (bad), -1 = I'm behind him (flank payoff).
            if tracks {
                (ec - pc).normalize_or_zero().dot(self.player.forward())
            } else {
                0.0
            },
            1.0, // bias
            ally.x.clamp(-1.0, 1.0),
            ally.y.clamp(-1.0, 1.0),
        ]
    }

    /// AI-Tiger observation, normalized to roughly `[-1, 1]`.
    ///
    /// This is deliberately a different *meaning* over the same fixed MLP width as
    /// `executor_obs`: a Tiger policy controls the player vehicle while the ordinary enemy policy
    /// still controls every T-34. The net chooses only drive, steer and trigger intent; crew
    /// movement, breech service, gun lay, travel lock, armor and ballistics remain authoritative.
    pub fn tiger_executor_obs(&self) -> [f32; crate::ai::OBS] {
        let pc = self.player.center();
        let nearest = self.enemies.iter().filter(|e| e.alive).min_by(|a, b| {
            a.center()
                .distance_squared(pc)
                .total_cmp(&b.center().distance_squared(pc))
        });

        let living = self.enemies.iter().filter(|e| e.alive).count() as f32;
        let total = self.enemies.len().max(1) as f32;
        let mut obs = [0.0f32; crate::ai::OBS];
        obs[5] = (self.player.speed / tank::TOP_SPEED).clamp(-1.0, 1.0);
        obs[6] = self.player.reload.clamp(0.0, 1.0);
        obs[7] = if self.breech_loaded() { 1.0 } else { -1.0 };
        obs[8] = (self.player.hp / tank::MAX_HP).clamp(0.0, 1.0);
        obs[9] = (living / total).clamp(0.0, 1.0);
        obs[13] = 1.0;

        if let Some(e) = nearest {
            let to = e.center() - pc;
            let flat = Vec2::new(to.x, to.z);
            let range = flat.length();
            let bearing = to.x.atan2(to.z) - self.player.heading;
            obs[0] = (to.x / 300.0).clamp(-1.0, 1.0);
            obs[1] = (to.z / 300.0).clamp(-1.0, 1.0);
            obs[2] = (range / 300.0).min(1.0);
            obs[3] = bearing.sin();
            obs[4] = bearing.cos();

            let to_dir = Vec3::new(to.x, 0.0, to.z).normalize_or_zero();
            let turret = self.player.turret_forward();
            let turret_flat = Vec3::new(turret.x, 0.0, turret.z).normalize_or_zero();
            obs[10] = turret_flat.cross(to_dir).y.clamp(-1.0, 1.0);

            // +1 means the Tiger is squarely in this T-34's frontal arc; -1 means it has reached
            // the enemy's rear. This lets evolution learn when to deny a flanking shot.
            let toward_tiger = Vec3::new(-to.x, 0.0, -to.z).normalize_or_zero();
            obs[11] = e.forward().dot(toward_tiger).clamp(-1.0, 1.0);
            obs[12] = (e.speed / tank::TOP_SPEED).clamp(-1.0, 1.0);
        }
        obs
    }

    /// Reset and stage an all-T-34 gauntlet using the six reusable full-detail enemy slots.
    /// Returns the active indices. This conversion is intentionally local to the experiment: the
    /// authored campaign still musters its normal SU-100/T-34 mix on the next reset/build.
    pub fn stage_t34_gauntlet(&mut self, count: usize, seed: u32, ring_m: f32) -> Vec<usize> {
        self.reset_match(seed, ring_m);
        let count = count.clamp(1, self.enemies.len());

        // Keep the lab out of the authored ruin cluster. Besides giving the film a clean armored
        // arena, this prevents one candidate's collapsed wall from changing the next candidate's
        // line of sight — essential for honest, repeatable evolution.
        let old_center = self.player.pos;
        let lab_x = 210.0;
        let lab_z = -210.0;
        let lab_center = Vec3::new(lab_x, self.hf.height_at(lab_x, lab_z), lab_z);
        let offset = lab_center - old_center;
        self.player.pos = lab_center;
        for (i, e) in self.enemies.iter_mut().enumerate() {
            if i < count {
                e.pos += offset;
                e.pos.y = self.hf.height_at(e.pos.x, e.pos.z);
                e.kind = tank::EnemyKind::T34;
                e.parts = tank::T34_PARTS;
                e.alive = true;
                e.hp = tank::MAX_HP;
            } else {
                e.alive = false;
                e.pos = Vec3::new(-3000.0 - i as f32 * 10.0, 0.0, -3000.0);
            }
        }
        (0..count).collect()
    }

    /// Reset the world for one training match: fresh Tiger at spawn, enemies re-mustered on a
    /// seeded ring, all combat state cleared. Smoke-test scope — structural damage to the city
    /// (destructibles) intentionally persists; matches are staged on open ground around the spawn.
    pub fn reset_match(&mut self, seed: u32, ring_m: f32) {
        // Tiger: parked at spawn, whole, loaded.
        let (px, pz) = self.spawn_xz;
        self.player = Tank::new(px, pz, 0.0, &self.hf, tank::TIGER_PARTS);
        self.crew = default_crew();
        self.modules = default_modules();
        self.death_cause = None;
        self.last_fatal = None;
        self.fire = 0.0;
        self.fire_active = false;
        self.fire_ammo_timer = None;
        self.fire_origin = None;
        self.breech = Breech::Loaded;
        self.station = Station::Commander;
        self.switch_from = Station::Commander;
        self.switch_t = 0.0;
        self.switch_dur = 0.0;
        self.activity = None;
        self.lever_open = 0.0;
        self.lever_pulling = false;
        self.ram = 0.0;
        self.eject_t = 0.0;
        self.ext_t = 0.0;
        self.repair_t = 0.0;
        self.prev_space = false;
        self.prev_lmb = false;
        self.grab = false;
        self.carry_mesh = None;
        self.carried_casing = None;
        self.casing_pull = 0.0;
        self.loose.fill(LooseShell::default());
        self.casings.fill(Casing::default());
        self.slot_taken.fill(false);
        self.rounds_spent = 0;
        self.ext_equipped = false;
        self.carrying_track_kit = false;
        self.track_repair_phase = TrackRepairPhase::GrabEnd;
        self.track_pull = 0.0;
        self.track_hits = 0;
        self.hammer_t = 0.0;
        self.driver_hatch = false;
        self.engine_on = true;
        self.engine_crank_t = 0.0;
        self.start_noise_t = 0.0;
        self.drive_cmd = 0.0;
        self.act_complete = false;
        self.mode = Mode::Tank;
        self.recompute_hp();
        // Enemies: a seeded muster ring around the spawn (open steppe), facing in, staggered reloads.
        let n = self.enemies.len();
        for i in 0..n {
            self.enemy_active[i] = true;
            self.enemy_skill[i] = 42;
            self.enemy_concealed[i] = false;
            let h = seed.wrapping_add(i as u32 * 7919);
            let ang = hash01(h) * std::f32::consts::TAU;
            let r = ring_m * (0.85 + 0.3 * hash01(h ^ 0xA5A5));
            let (ex, ez) = (px + ang.sin() * r, pz + ang.cos() * r);
            let kind = self.enemies[i].kind;
            let parts = tank::parts_for(kind);
            let mut e = Tank::new(ex, ez, (px - ex).atan2(pz - ez), &self.hf, parts);
            e.kind = kind;
            e.reload = ENEMY_RELOAD * (i as f32 + 0.5) / n as f32;
            self.enemies[i] = e;
            self.wrecks[i] = Wreck::default();
        }
        // Combat state: stealth machine, in-flight shells, FX, delayed audio, telemetry.
        self.stealth = stealth::Stealth::new(n);
        for shot in &mut self.e_shots {
            *shot = EnemyShot::default();
        }
        self.fx = Fx::default();
        self.scheduled.clear();
        self.concussion_t = 0.0;
        self.concussion_dur = 0.0;
        self.stat_hits_on_player = 0;
        self.stat_pens_on_player = 0;
        // Determinism: the counter-seeded hash ticks ARE the match's randomness — they must start
        // from the same base every match or "same genome, same seed" diverges (armor_tick seeds
        // the penetration rolls, ai_tick the aim dispersion).
        self.ai_tick = 0;
        self.armor_tick = 0;
        self.kill_tick = 0;
        self.ambient_tick = 0;
        self.ambient_t = AMBIENT_EXP_MIN;
        self.next_e_fx = 0;
        self.next_blast = 0;
        self.next_spark = 0;
        self.next_snow = 0;
        for b in &mut self.blasts {
            *b = Blast::default();
        }
        for sp in &mut self.sparks {
            *sp = Spark::default();
        }
        for sn in &mut self.snow_puffs {
            *sn = SnowPuff::default();
        }
        // Drop the wreck occluders appended by destroy_enemy in earlier matches.
        self.occluders.truncate(self.base_occluders);
        for cd in &mut self.enemy_mg_cd {
            *cd = 0.0;
        }
    }

    /// External-camera filming: the crew-KO blood splash is first-person feedback — wipe it so an
    /// aerial replay doesn't smear red over the whole frame when the Tiger takes a penetration.
    pub fn clear_crew_ko(&mut self) {
        self.crew_ko_t = 0.0;
    }

    /// Silence the mix (training runs hundreds of matches; every shot would open a real sink).
    pub fn mute_audio(&mut self) {
        self.audio = None;
    }

    /// Put this process's camera/body at its claimed LAN station without a solo clamber delay. The
    /// host simulation still receives all five stations concurrently through `TankInput::coop`.
    pub fn set_lan_role(&mut self, role: crate::lan::LanRole) {
        self.lan_local_role = Some(role);
        let station = match role {
            crate::lan::LanRole::Commander => Station::Commander,
            crate::lan::LanRole::Driver => Station::Driver,
            crate::lan::LanRole::Gunner => Station::Gunner,
            crate::lan::LanRole::Loader => Station::Loader,
            crate::lan::LanRole::Radio => Station::Radioman,
        };
        if self.station != station {
            self.station = station;
            self.switch_from = station;
            self.switch_t = 0.0;
            self.switch_dur = 0.0;
            self.grab = false;
            self.carry_mesh = None;
        }
    }

    /// The gunner's ADS view composites the live optic circle INSIDE the turret interior
    /// (crush mask 4) instead of floating it in black. Off during the seat-switch scramble
    /// (the camera is mid-lerp between stations) and in the dev chase cam.
    pub fn optic_composite_active(&self) -> bool {
        matches!(self.mode, Mode::Tank)
            && matches!(self.station, Station::Gunner)
            && self.switch_t <= 0.0
            && !self.chase_cam
    }

    /// EYEPIECE COMPOSITE interior camera: the gunner's head just behind the TZF eyepiece
    /// (turret-local anchor (0.536, 1.99, 0.81) is the authored sight), looking down the
    /// turret's forward axis so the optic circle sits dead-center; wide field so the breech,
    /// wall and roof frame it. Returns (eye, forward, fov_y).
    pub fn gunner_interior_camera(&self) -> (Vec3, Vec3, f32) {
        let tm = self.player.turret_model();
        let eye = tm.transform_point3(Vec3::new(0.536, 1.955, 0.34));
        let sight = tm.transform_point3(Vec3::new(0.536, 1.975, 0.81));
        (eye, sight - eye, 66f32.to_radians())
    }

    /// COUCH-COOP: camera pose for a splitscreen viewport showing another crewman's station this
    /// frame. Pure function of the tank pose (the gun IS the gunner's eye, the visor is the
    /// driver's), so extra views render after `update` without touching the local player's
    /// camera bookkeeping. Returns (eye, forward, fov_y, optic_mask, reticle).
    pub fn coop_view_camera(&self, role: crate::lan::LanRole) -> (Vec3, Vec3, f32, f32, bool) {
        match role {
            crate::lan::LanRole::Gunner => {
                let (_mpos, mdir) = self.player.muzzle();
                let fov = [25f32, 12.0, 6.0][self.sights_mag.min(2) as usize].to_radians();
                (
                    self.player.gunsight(),
                    mdir,
                    fov,
                    1.0,
                    self.module_ok(ModuleKind::GunnerSight),
                )
            }
            crate::lan::LanRole::Driver => {
                // The couch driver must actually SEE to drive: eye up near the visor line and the
                // camera aimed at the slit aperture (hull-local (-0.448, 1.749, 2.119)), so the
                // daylight band sits dead-center in the letterbox instead of being cropped away
                // (the solo seat eye looks level and shows mostly glacis interior).
                let hm = self.player.hull_model();
                let eye = hm.transform_point3(Vec3::new(-0.448, 1.72, 1.78));
                let slit = hm.transform_point3(Vec3::new(-0.448, 1.749, 2.119));
                (eye, slit - eye, 48f32.to_radians(), 2.0, false)
            }
            crate::lan::LanRole::Radio => (
                self.station_eye(Station::Radioman),
                self.player.forward(),
                54f32.to_radians(),
                2.0,
                false,
            ),
            // Commander/Loader viewports are the local player's own (posed by `update`).
            _ => (
                self.station_eye(Station::Commander),
                self.player.turret_forward(),
                64f32.to_radians(),
                0.0,
                false,
            ),
        }
    }

    /// Compact host-authoritative state for LAN reconciliation. Transient particles remain local;
    /// the machine, ammunition, crew/modules and every active enemy are authoritative.
    pub fn net_snapshot(&self) -> crate::lan::WorldSnapshot {
        let tank = |t: &Tank| crate::lan::NetTankState {
            pos: t.pos.to_array(),
            heading: t.heading,
            speed: t.speed,
            turret_yaw: t.turret_yaw,
            gun_pitch: t.gun_pitch,
            reload: t.reload,
            hp: t.hp,
            alive: t.alive,
        };
        let module_mask = self
            .modules
            .iter()
            .enumerate()
            .fold(0u16, |mask, (i, module)| {
                mask | ((module.functional as u16) << i)
            });
        let crew_mask = self
            .crew
            .iter()
            .enumerate()
            .fold(0u8, |mask, (i, crew)| mask | ((crew.ok as u8) << i));
        crate::lan::WorldSnapshot {
            seq: 0,
            act: self.act.id,
            player: tank(&self.player),
            enemies: self.enemies.iter().map(tank).collect(),
            engine_on: self.engine_on,
            hatch_open: self.hatch_open,
            breech: match self.breech {
                Breech::Loaded => 0,
                Breech::Spent => 1,
                Breech::Empty => 2,
            },
            rounds_spent: self.rounds_spent,
            he_loaded: self.he_loaded,
            fire: self.fire,
            module_mask,
            crew_mask,
            act_complete: self.act_complete,
        }
    }

    /// Reconcile a client prediction to the host. Invalid/non-finite tank records are ignored as a
    /// unit; a malformed LAN packet can never poison physics with NaNs.
    pub fn apply_net_snapshot(&mut self, snapshot: &crate::lan::WorldSnapshot) {
        fn apply(dst: &mut Tank, src: crate::lan::NetTankState) {
            let values = [
                src.pos[0],
                src.pos[1],
                src.pos[2],
                src.heading,
                src.speed,
                src.turret_yaw,
                src.gun_pitch,
                src.reload,
                src.hp,
            ];
            if !values.iter().all(|value| value.is_finite()) {
                return;
            }
            dst.pos = Vec3::from_array(src.pos);
            dst.heading = src.heading;
            dst.speed = src.speed;
            dst.turret_yaw = src.turret_yaw;
            dst.gun_pitch = src.gun_pitch;
            dst.reload = src.reload.max(0.0);
            dst.hp = src.hp.clamp(0.0, tank::MAX_HP);
            dst.alive = src.alive;
        }
        apply(&mut self.player, snapshot.player);
        for (i, src) in snapshot.enemies.iter().copied().enumerate() {
            if i >= self.enemies.len() {
                break;
            }
            let was_alive = self.enemies[i].alive;
            apply(&mut self.enemies[i], src);
            if self.enemies[i].alive {
                // A client can briefly predict a kill before the authoritative snapshot arrives.
                // Remove that speculative wreck completely instead of leaving a burning ghost.
                self.wrecks[i] = Wreck::default();
            } else if was_alive {
                // The host owns the kill result. Give late-arriving remote kills an immediate,
                // deterministic burning wreck; subsequent snapshots keep the dead tank aligned.
                self.wrecks[i] = Wreck {
                    kind: WreckKind::Burn,
                    burning: true,
                    ..Wreck::default()
                };
            }
        }
        self.engine_on = snapshot.engine_on;
        self.hatch_open = snapshot.hatch_open;
        self.breech = match snapshot.breech {
            0 => Breech::Loaded,
            1 => Breech::Spent,
            _ => Breech::Empty,
        };
        self.rounds_spent = snapshot.rounds_spent;
        self.he_loaded = snapshot.he_loaded;
        self.fire = snapshot.fire.clamp(0.0, 1.0);
        for (i, module) in self.modules.iter_mut().enumerate() {
            module.functional = snapshot.module_mask & (1 << i) != 0;
        }
        for (i, crew) in self.crew.iter_mut().enumerate() {
            crew.ok = snapshot.crew_mask & (1 << i) != 0;
        }
        self.act_complete = snapshot.act_complete;
    }

    pub fn debug_engage_all(&mut self) {
        for i in 0..self.enemies.len() {
            if self.enemies[i].alive {
                self.stealth.spotted[i] = true;
                self.stealth.meters[i] = 1.0;
            }
        }
        let pos = self.player.center();
        for p in self.stealth.platoons.iter_mut() {
            if p.state != stealth::AlertState::Eliminated {
                p.state = stealth::AlertState::Engaged;
                p.pending = false;
                p.last_known = pos;
            }
        }
    }

    // ── Headless telemetry accessors for the reload / firefight / repair demos + playtest policy ──
    /// Ram seating progress 0..1 (the mash-driven "drive the round home").
    pub fn ram_progress(&self) -> f32 {
        self.ram
    }
    /// Headless: hand the player the track kit (the toolbox-hotspot pickup is a live-window UX).
    pub fn dbg_take_track_kit(&mut self) {
        self.carrying_track_kit = true;
        self.track_repair_phase = TrackRepairPhase::GrabEnd;
        self.track_pull = 0.0;
        self.track_hits = 0;
    }
    /// Current outside-track phase for headless filmstrip/demo policy.
    pub fn track_repair_phase_label(&self) -> &'static str {
        match self.track_repair_phase {
            TrackRepairPhase::GrabEnd => "grab",
            TrackRepairPhase::PullEnd => "pull",
            TrackRepairPhase::FitLink => "fit",
            TrackRepairPhase::HammerPin => "hammer",
        }
    }
    /// How far the loose run has been pulled back over the wheels.
    pub fn track_pull_progress(&self) -> f32 {
        self.track_pull
    }
    /// World position of a track's repair anchor (where the on-foot hammer ritual happens).
    pub fn player_track_anchor(&self, kind: ModuleKind) -> Vec3 {
        let side = if matches!(kind, ModuleKind::LeftTrack) {
            -1.55
        } else {
            1.55
        };
        self.player
            .hull_model()
            .transform_point3(Vec3::new(side, 0.9, -0.4))
    }
    /// Loose (dropped) rounds currently on the basket floor — headless drop-physics check.
    pub fn loose_shells_active(&self) -> usize {
        self.loose.iter().filter(|s| s.active).count()
    }
    /// (turret-local position, ground speed) of loose round `i`, if active.
    pub fn loose_shell_state(&self, i: usize) -> Option<(Vec3, f32)> {
        let s = self.loose.get(i)?;
        s.active
            .then(|| (s.pos, Vec3::new(s.vel.x, 0.0, s.vel.z).length()))
    }
    /// Breech state label for the filmstrip logs.
    pub fn breech_label(&self) -> &'static str {
        match self.breech {
            Breech::Loaded => "loaded",
            Breech::Spent => "spent",
            Breech::Empty => "empty",
        }
    }
    /// True iff a round is seated + the gun can fire (breech `Loaded`).
    pub fn breech_loaded(&self) -> bool {
        matches!(self.breech, Breech::Loaded)
    }
    /// Headless crew/policy gate: the requested station has finished its physical clamber time.
    pub fn station_ready(&self, station: Station) -> bool {
        self.station == station && self.switch_t <= 0.0 && self.activity.is_none()
    }
    /// Full authoritative main-gun gate for an auto crew. Unlike `Tank::can_fire`, this also
    /// checks the physical breech, gun module and gunner.
    pub fn auto_can_fire(&self) -> bool {
        self.player_can_fire()
    }
    /// Number of ejected casings currently in flight (for the casing-toss frame).
    pub fn active_casings(&self) -> usize {
        self.casings.iter().filter(|c| c.active).count()
    }
    /// Interior fire intensity (0 = out).
    pub fn fire_level(&self) -> f32 {
        self.fire
    }
    pub fn haze_level(&self) -> f32 {
        self.interior_haze
    }
    /// Is the interior fire alight?
    pub fn is_fire_active(&self) -> bool {
        self.fire_active
    }
    /// Seconds left on the cook-off countdown once the fire threatens the ammo (`None` = not armed).
    pub fn cookoff_remaining(&self) -> Option<f32> {
        self.fire_ammo_timer
    }
    /// Whether a module still works (repair-demo verification).
    pub fn is_module_functional(&self, kind: ModuleKind) -> bool {
        self.module_ok(kind)
    }

    /// Queue a combat sound whose SOURCE is at `src` in the world, to be HEARD after its travel time
    /// to the player (range / speed of sound) at a range-attenuated volume. Distant gunfire and
    /// explosions are thus SEEN first (muzzle flash / fireball) and heard a beat later, fainter the
    /// further off. The player's OWN main gun bypasses this (fires instantly — you're right there).
    fn schedule_sound_from(&mut self, sfx: crate::audio::Sfx, src: Vec3, base_vol: f32) {
        let d = src.distance(self.threat_center());
        let delay = d / SPEED_OF_SOUND;
        let atten = SOUND_REF_DIST / (SOUND_REF_DIST + d); // 1 at the tank, falling off with range
        self.scheduled.push(ScheduledSound {
            delay,
            sfx,
            volume: base_vol * atten,
            source: src,
            buf: None,
        });
    }

    /// Nearest thing along a ray hop: enemies (spheres), standing walls, standing trunks, terrain.
    fn nearest_hop(&self, o: Vec3, d: Vec3, max_t: f32) -> (f32, HopHit) {
        let mut t_best = max_t;
        let mut hit = HopHit::Nothing;
        for (i, e) in self.enemies.iter().enumerate() {
            if !e.alive {
                continue;
            }
            if let Some(t) = ray_sphere(o, d, e.center(), 3.6) {
                if t < t_best {
                    t_best = t;
                    hit = HopHit::Enemy(i);
                }
            }
        }
        if let Some(t) = ray_terrain(o, d, &self.hf, t_best) {
            if t < t_best {
                t_best = t;
                hit = HopHit::Terrain;
            }
        }
        if let Some((t, di)) = self.ray_destructible(o, d, t_best) {
            if t < t_best {
                t_best = t;
                hit = HopHit::Struct(di);
            }
        }
        for &(min, max) in &self.story_cover {
            if let Some(t) = ray_aabb(o, d, min, max, t_best) {
                if t < t_best {
                    t_best = t;
                    hit = HopHit::HardCover;
                }
            }
        }
        if let Some((t, ti)) = self.trees.ray(o, d, t_best) {
            if t < t_best {
                t_best = t;
                hit = HopHit::Tree(ti);
            }
        }
        (t_best, hit)
    }

    fn fire(&mut self, r: &mut Renderer) {
        let (mpos, mdir) = self.player.muzzle();
        let max = 1500.0;
        let shell: &'static crate::ballistics::ShellSpec = if self.he_loaded {
            &crate::ballistics::SPRGR_88
        } else {
            &crate::ballistics::PZGR_39
        };

        // The round's route: waypoints from the muzzle to wherever it terminates. AP walks the
        // PEN RAY (docs/DESTRUCTION.md §3): every wall transit spends budget, kicks the line by a
        // hashed deflection, and stacks yaw disruption; whatever pen remains fights the terminal
        // plate. HE flies to the FIRST surface and stops there (the blast radius does the rest,
        // resolved at impact). Walls are CONCEALMENT, not cover.
        let mut route: Vec<Vec3> = vec![mpos];
        let mut transits: Vec<(usize, f32)> = Vec::new();
        let mut pass_trees: Vec<(usize, Vec3, f32)> = Vec::new();
        let mut hit_enemy: Option<usize> = None;
        let mut hit_struct: Option<usize> = None;
        let mut hit_tree: Option<usize> = None;
        let mut player_flight_time: Option<f32> = None;
        let mut outcome = Outcome::Ground;

        let (mut o, mut d) = (mpos, mdir);
        let mut cum_range = 0.0f32;
        let mut penalty_mm = 0.0f32;
        let mut disrupt = 0.0f32;
        for _hop in 0..PEN_RAY_MAX_HOPS {
            let (t, hop) = self.nearest_hop(o, d, max - cum_range);
            cum_range += t;
            let hp = o + d * t;
            match hop {
                HopHit::Enemy(idx) => {
                    route.push(hp);
                    hit_enemy = Some(idx);
                    let e = &self.enemies[idx];
                    let arc = hit_arc(e.forward(), o - e.center());
                    self.armor_tick = self.armor_tick.wrapping_add(1);
                    let turret = hash01(self.armor_tick.wrapping_mul(2777)) < 0.42;
                    let facing = facing_from(arc, turret);
                    let normal = face_normal(arc, e.forward(), o - e.center());
                    let horiz = crate::ballistics::horiz_obliquity_deg(d, normal);
                    let ev = crate::ballistics::resolve(
                        shell,
                        cum_range,
                        enemy_armor(e.kind).plate(facing),
                        horiz,
                        self.armor_tick,
                    );
                    player_flight_time = Some(ev.flight_time);
                    // Wall transits degrade the terminal duel: budget spent + yaw disruption
                    // (docs/DESTRUCTION.md §3.1). A clean shot reduces to ev.terminal exactly.
                    let pen_eff = (ev.pen_mm - penalty_mm) * (1.0 - disrupt);
                    let pen = match ev.terminal {
                        crate::ballistics::Terminal::Penetration => pen_eff > ev.effective_mm,
                        _ => false,
                    };
                    println!(
                        "[kessel] FIRE ({}) → {} #{idx} {} ({}) at {:.0} m: {:.0}({:.0}) mm pen vs {:.0} mm eff @ {:.0}° → {}{}",
                        shell.name, e.kind.name(), arc.name(), ev.plate.name, cum_range,
                        ev.pen_mm, pen_eff, ev.effective_mm, ev.impact_angle_deg,
                        if pen { "Penetration" } else { "held" },
                        if transits.is_empty() { String::new() }
                        else { format!(" (through {} wall{})", transits.len(),
                                       if transits.len() == 1 { "" } else { "s" }) });
                    outcome = if pen {
                        Outcome::Penetrate
                    } else {
                        Outcome::Bounce
                    };
                    break;
                }
                HopHit::Struct(di) => {
                    route.push(hp);
                    if self.he_loaded {
                        // HE: detonates on the wall FACE. The radial blast (applied at land)
                        // does the structural work — no flat wall damage here.
                        hit_struct = Some(di);
                        println!(
                            "[kessel] FIRE (SprGr) → burst on a structure at {:.0} m",
                            cum_range
                        );
                        break;
                    }
                    let ap = self.destructibles[di].class.ap();
                    let budget = crate::ballistics::budget_mm(shell, cum_range);
                    if penalty_mm + ap.cost_mm >= budget {
                        // The round dies IN this wall — a bigger bite, no exit.
                        hit_struct = Some(di);
                        println!("[kessel] FIRE (PzGr) → round died in a wall at {:.0} m ({} transits, {:.0}/{:.0} mm spent)",
                            cum_range, transits.len(), penalty_mm + ap.cost_mm, budget);
                        break;
                    }
                    // PUNCH THROUGH: spend budget, stack disruption, kink the line (hashed).
                    penalty_mm += ap.cost_mm;
                    disrupt = (disrupt + ap.disrupt).min(0.85);
                    transits.push((di, cum_range));
                    self.armor_tick = self.armor_tick.wrapping_add(1);
                    let right = d.cross(Vec3::Y).normalize_or_zero();
                    let up = right.cross(d).normalize_or_zero();
                    let mrad = ap.deflect_mrad * 0.001;
                    d = (d
                        + right * ((hash01(self.armor_tick) * 2.0 - 1.0) * mrad)
                        + up * ((hash01(self.armor_tick.wrapping_add(97)) * 2.0 - 1.0) * mrad))
                        .normalize_or_zero();
                    o = hp + d * 0.05;
                }
                HopHit::HardCover => {
                    route.push(hp);
                    println!(
                        "[kessel] FIRE ({}) → hard environmental cover at {:.0} m",
                        shell.name, cum_range
                    );
                    break;
                }
                HopHit::Tree(ti) => {
                    if self.he_loaded {
                        route.push(hp);
                        hit_tree = Some(ti);
                        println!(
                            "[kessel] FIRE (SprGr) → burst on a tree at {:.0} m",
                            cum_range
                        );
                        break;
                    }
                    // AP passes a trunk: a fir is obliterated in passing; a mature pine snaps
                    // too but eats a little budget and nudges the line.
                    let big = self.trees.is_big(ti);
                    penalty_mm += if big {
                        TREE_AP_COST_BIG
                    } else {
                        TREE_AP_COST_SMALL
                    };
                    pass_trees.push((ti, d, cum_range));
                    if big {
                        self.armor_tick = self.armor_tick.wrapping_add(1);
                        let right = d.cross(Vec3::Y).normalize_or_zero();
                        d = (d + right * ((hash01(self.armor_tick) * 2.0 - 1.0) * 0.008))
                            .normalize_or_zero();
                        route.push(hp);
                    }
                    o = hp + d * 0.05;
                }
                HopHit::Terrain | HopHit::Nothing => {
                    route.push(hp);
                    println!(
                        "[kessel] FIRE ({}) → ground/miss at {:.0} m{}",
                        shell.name,
                        cum_range,
                        if transits.is_empty() {
                            String::new()
                        } else {
                            format!(
                                " (holed {} wall{} en route)",
                                transits.len(),
                                if transits.len() == 1 { "" } else { "s" }
                            )
                        }
                    );
                    break;
                }
            }
        }
        if route.len() < 2 {
            route.push(o + d * (max - cum_range).max(1.0)); // hop cap exhausted mid-wall: fly on out
        }
        let hit_pos = *route.last().unwrap();
        let hit_t = cum_range.min(max);

        // A PREVIOUS round still in flight rides this same FX slot — overwriting it silently
        // deleted its damage. Land it FIRST (its outcome/target were fixed at fire time; only the
        // visual streak is cut short by the follow-up shot). Must happen BEFORE the new shot's
        // tracer state is armed below — land_player_shot retires tracer_active.
        if self.fx.tracer_active {
            self.land_player_shot(r);
        }
        self.fx.muzzle_t = 0.09;
        self.fx.smoke_t = 1.4; // slow drifting plume at the muzzle
        self.fx.smoke_dur = 1.4;
        self.fx.recoil = 1.0; // full camera kick, decays in update
        self.interior_haze = (self.interior_haze + 0.11).min(1.0); // muzzle backdraft into the compartment
        self.fx.tracer_active = true;
        self.fx.tracer_t = 0.0;
        // REAL time of flight from the ballistic solve when a tank was struck (the 88 takes ~0.66 s
        // to 500 m — you watch the round go); the old visual constant only for ground/structure hits.
        self.fx.tracer_dur = player_flight_time.unwrap_or(hit_t / 450.0).max(0.05);
        self.fx.tracer_from = mpos;
        self.fx.tracer_to = hit_pos;
        self.fx.tracer_dir = mdir;
        self.fx.tracer_route = route;
        self.fx.pending_target = hit_enemy;
        self.fx.pending_struct = hit_struct;
        self.fx.pending_tree = hit_tree;
        self.fx.pending_transits = transits;
        self.fx.pending_trees = pass_trees;
        // HE always bursts wherever it stopped — wall face, hull, trunk, or the snow.
        self.fx.pending_he = if self.he_loaded {
            Some((hit_pos, HE_BLAST_RADIUS))
        } else {
            None
        };
        self.fx.pending_outcome = outcome;
        // The player's OWN main gun fires INSTANTLY (you're right there). The report is picked by
        // LISTENING PERSPECTIVE — head out of a hatch hears the full outdoor blast (4 variants),
        // buttoned up hears the crack through the hull (2 variants) — cycled per shot so successive
        // rounds don't sound identical. The round then screams away downrange as an outgoing shell
        // shriek that Dopplers down over its flight (tracer duration).
        self.gun_shots = self.gun_shots.wrapping_add(1);
        if let Some(runtime) = self.campaign.as_mut() {
            runtime.battle_pressure = (runtime.battle_pressure + 14.0).min(100.0);
        }
        if let Some(a) = &self.audio {
            let head_out = (matches!(self.station, Station::Commander) && self.commander_outside)
                || (matches!(self.station, Station::Driver) && self.driver_hatch);
            let pick = if head_out {
                let g = crate::audio::Sfx::PLAYER_GUNS_EXT;
                g[self.gun_shots as usize % g.len()]
            } else {
                let g = crate::audio::Sfx::PLAYER_GUNS_INT;
                g[self.gun_shots as usize % g.len()]
            };
            a.play(pick);
            a.start_shell_shriek(self.fx.tracer_dur);
        }
        // The HIT under the sample (playtest: the recording alone didn't punch): a synthesized
        // muzzle concussion — broadband crack + 40–70 Hz body, boomier when buttoned up.
        let head_out = (matches!(self.station, Station::Commander) && self.commander_outside)
            || (matches!(self.station, Station::Driver) && self.driver_hatch);
        let punch = crate::acoustics::synth_gun_punch(!head_out, self.gun_shots);
        self.scheduled.push(ScheduledSound {
            delay: 0.0,
            sfx: crate::audio::Sfx::PLAYER_GUNS_INT[0], // ignored — buf plays instead
            volume: 1.25,
            source: self.player.center(),
            buf: Some(punch),
        });
    }

    /// Number of MG rounds fired so far (headless MG demo telemetry).
    pub fn mg_round_count(&self) -> u32 {
        self.mg_round
    }

    /// V/R quick views: INSTANTLY jump to the gunner's sight / the breech while that crewman LIVES
    /// (you look through the living man's position — commander delegation); with him dead the jump
    /// refuses and you must physically clamber into the seat (1-4, with the switch cost).
    pub fn jump_station(&mut self, s: Station) {
        let post = match s {
            Station::Commander => crate::lan::LanRole::Commander,
            Station::Driver => crate::lan::LanRole::Driver,
            Station::Gunner => crate::lan::LanRole::Gunner,
            Station::Loader => crate::lan::LanRole::Loader,
            Station::Radioman => crate::lan::LanRole::Radio,
        };
        if self.lan_post_mask & (1 << post as u8) != 0 {
            println!(
                "[lan] {} POST OCCUPIED — crewman blocks the way",
                post.label()
            );
            return;
        }
        if s != self.station {
            self.station = s;
            self.switch_t = 0.0;
        }
    }

    /// Backspace bail-out (stub until the on-foot mode lands): ten presses from the cupola or
    /// periscope and the commander abandons the vehicle.
    pub fn press_bail(&mut self) {
        if matches!(self.station, Station::Commander) {
            self.bail_presses += 1;
            if self.bail_presses >= 10 {
                // Absitzen: bailing from the cupola IS the dismount (same gates as E) — stationary,
                // OR at any speed while ON FIRE (the Last Tiger moment; toggle_dismount owns the rule).
                self.bail_presses = 0;
                self.toggle_dismount();
            }
        }
    }

    /// ARENA mode: repurpose the built act-1 world as a combat test range — a handful of enemies
    /// repositioned CLOSE and in the open west of the spawn, in clear sight, already hunting.
    pub fn arena_setup(&mut self) {
        // MORTAL by design (was god-mode) — but `god` is NOT forced off here: the fresh build
        // defaults it false and the menu's Invincible toggle (set after build) must survive.
        // The player parks at the ruin field's southern edge, hull facing the structures — the
        // fight happens AMONG cover, not on an open field. (Coordinates are the origin ruin
        // dressing, which the act-0 bowl keeps.)
        let (px, pz) = (24.0, -34.0);
        self.player = Tank::new(
            px,
            pz,
            (0.0f32 - px).atan2(4.0 - pz),
            &self.hf,
            tank::TIGER_PARTS,
        );
        self.spawn_xz = (px, pz);
        // Three contacts threaded through the ruins at 40-90 m, staggered reloads so the opening
        // isn't a one-frame volley. The rest of the swarm is benched.
        let spots = [
            (0usize, -46.0, 18.0),  // SU-100 half-left, long sightline down the gun line
            (2usize, -2.0, 34.0),   // T-34 among the north ruin cluster
            (3usize, -30.0, -16.0), // T-34 by the south doorway, close
        ];
        for (i, e) in self.enemies.iter_mut().enumerate() {
            if let Some(&(_, x, z)) = spots.iter().find(|(k, _, _)| *k == i) {
                e.pos = Vec3::new(x, self.hf.height_at(x, z), z);
                e.heading = (px - x).atan2(pz - z); // laid toward the player's park
                e.alive = true;
                e.reload = ENEMY_RELOAD * (0.4 + i as f32 * 0.35);
            } else {
                e.alive = false;
                e.pos = Vec3::new(-3000.0, 0.0, -3000.0);
            }
        }
        println!(
            "[kessel] ARENA: 3 enemies at 40-90 m in the ruin field, player MORTAL, clear day"
        );
    }

    /// KOOP: stage the TAKE & HOLD op on the act-0 bowl. The compound is the origin ruin cluster
    /// (the arena's authored cover); the crew starts on the southern approach with a short
    /// shake-down drive, two dug-in defenders hold the yard, three counterattack waves follow.
    pub fn start_compound(&mut self, seed: u32) {
        let center = Vec2::new(0.0, 10.0);
        let (px, pz) = (36.0, -430.0);
        self.player = Tank::new(
            px,
            pz,
            (center.x - px).atan2(center.y - pz),
            &self.hf,
            tank::TIGER_PARTS,
        );
        self.spawn_xz = (px, pz);
        // The picket: two T-34s dug in at the compound edges, guns south toward the approach.
        // They start UNAWARE — a careful crew opens the fight on its own terms.
        let posts = [(-26.0f32, 2.0f32), (18.0, 38.0)];
        for (i, e) in self.enemies.iter_mut().enumerate() {
            if let Some(&(x, z)) = posts.get(i) {
                let mut t = Tank::new(x, z, (px - x).atan2(pz - z), &self.hf, tank::T34_PARTS);
                t.kind = tank::EnemyKind::T34;
                t.reload = ENEMY_RELOAD * (0.5 + i as f32 * 0.4);
                *e = t;
                self.enemy_active[i] = true;
                self.enemy_skill[i] = 38;
            } else {
                e.alive = false;
                e.pos = Vec3::new(-3000.0 - i as f32 * 10.0, 0.0, -3000.0);
                self.enemy_active[i] = false;
            }
            self.enemy_concealed[i] = false;
        }
        for w in &mut self.wrecks {
            *w = Wreck::default();
        }
        self.stealth = stealth::Stealth::new(self.enemies.len());
        self.compound = Some(CompoundMission {
            center,
            radius: 55.0,
            phase: CompoundPhase::Advance,
            capture: 0.0,
            wave: 0,
            timer: 0.0,
            seed,
            contested: false,
        });
        println!(
            "[koop] STÜTZPUNKT: take the ruin compound at ({:.0},{:.0}), hold {} waves",
            center.x, center.y, COMPOUND_WAVES
        );
    }

    /// Muster one counterattack wave on a bearing ring ~600 m out, already alerted (a counter-
    /// attack knows where it is going). Reuses the six enemy slots: earlier waves' wrecks yield
    /// their slots; the leftover slots are benched so they don't linger as phantom cover.
    fn stage_compound_wave(&mut self, wave: u8, seed: u32, center: Vec2) {
        let n = usize::from(2 + wave).min(self.enemies.len()); // 3 / 4 / 5 attackers
        let base_bearing = match wave {
            1 => 0.0f32,               // from the north
            2 => 1.9,                  // from the east-southeast
            _ => 4.4,                  // from the west
        };
        for i in 0..self.enemies.len() {
            if i < n {
                let h = seed
                    .wrapping_add(u32::from(wave).wrapping_mul(0x9E37_79B9))
                    .wrapping_add(i as u32 * 7919);
                let ang = base_bearing + (hash01(h) - 0.5) * 0.9;
                let r = 560.0 + 140.0 * hash01(h ^ 0x5A5A);
                let (ex, ez) = (center.x + ang.sin() * r, center.y + ang.cos() * r);
                let mut e = Tank::new(
                    ex,
                    ez,
                    (center.x - ex).atan2(center.y - ez),
                    &self.hf,
                    tank::T34_PARTS,
                );
                e.kind = tank::EnemyKind::T34;
                e.reload = ENEMY_RELOAD * (0.5 + i as f32 * 0.35);
                self.enemies[i] = e;
                self.enemy_active[i] = true;
                self.enemy_skill[i] = 30 + wave * 12;
            } else {
                self.enemies[i].alive = false;
                self.enemies[i].pos = Vec3::new(-3000.0 - i as f32 * 10.0, 0.0, -3000.0);
                self.enemy_active[i] = false;
            }
            self.enemy_concealed[i] = false;
            self.wrecks[i] = Wreck::default();
            if let Some(cd) = self.enemy_mg_cd.get_mut(i) {
                *cd = 0.0;
            }
        }
        // Wave slots recycle earlier kills: drop their wreck occluders with them.
        self.occluders.truncate(self.base_occluders);
        self.stealth = stealth::Stealth::new(self.enemies.len());
        self.debug_engage_all();
        println!("[koop] WELLE {wave}/{COMPOUND_WAVES}: {n} T-34 rollen an");
    }

    /// Per-frame STÜTZPUNKT phase logic (no-op unless `start_compound` ran).
    fn update_compound(&mut self, dt: f32) {
        let Some(mut m) = self.compound.take() else {
            return;
        };
        let p = Vec2::new(self.player.pos.x, self.player.pos.z);
        let dist = p.distance(m.center);
        let enemy_near = self.enemies.iter().enumerate().any(|(i, e)| {
            self.enemy_active.get(i).copied().unwrap_or(false)
                && e.alive
                && Vec2::new(e.pos.x, e.pos.z).distance(m.center) < m.radius + 90.0
        });
        let all_dead = self
            .enemies
            .iter()
            .enumerate()
            .all(|(i, e)| !e.alive || !self.enemy_active.get(i).copied().unwrap_or(false));
        m.contested = enemy_near;
        match m.phase {
            CompoundPhase::Advance => {
                if dist < m.radius {
                    m.phase = CompoundPhase::Take;
                }
            }
            CompoundPhase::Take => {
                if dist < m.radius && !enemy_near {
                    m.capture = (m.capture + dt / 14.0).min(1.0);
                    if m.capture >= 1.0 {
                        m.phase = CompoundPhase::Breather;
                        m.timer = 22.0;
                        m.wave = 0;
                        println!("[koop] STÜTZPUNKT GENOMMEN — Gegenangriff kommt");
                    }
                } else if dist > m.radius {
                    m.capture = (m.capture - dt / 40.0).max(0.0);
                }
            }
            CompoundPhase::Breather => {
                m.timer -= dt;
                if m.timer <= 0.0 {
                    if m.wave >= COMPOUND_WAVES {
                        m.phase = CompoundPhase::Won;
                        println!("[koop] STELLUNG GEHALTEN — mission won");
                    } else {
                        m.wave += 1;
                        self.stage_compound_wave(m.wave, m.seed, m.center);
                        m.phase = CompoundPhase::Hold;
                    }
                }
            }
            CompoundPhase::Hold => {
                if all_dead {
                    m.timer = if m.wave >= COMPOUND_WAVES { 6.0 } else { 24.0 };
                    m.phase = CompoundPhase::Breather;
                    println!("[koop] WELLE {} zerschlagen", m.wave);
                }
            }
            CompoundPhase::Won => {}
        }
        self.compound = Some(m);
    }

    /// Front-end snapshot of the STÜTZPUNKT mission (None when the mission isn't running).
    pub fn compound_hud(&self) -> Option<CompoundHud> {
        let m = self.compound.as_ref()?;
        let p = Vec2::new(self.player.pos.x, self.player.pos.z);
        let enemies_left = self
            .enemies
            .iter()
            .enumerate()
            .filter(|(i, e)| e.alive && self.enemy_active.get(*i).copied().unwrap_or(false))
            .count() as u8;
        Some(CompoundHud {
            phase: m.phase,
            capture: m.capture,
            wave: m.wave,
            waves: COMPOUND_WAVES,
            enemies_left,
            timer: m.timer.max(0.0),
            dist_m: p.distance(m.center),
            bearing_deg: crate::world::bearing(p, m.center),
            contested: m.contested,
        })
    }

    /// DEV GARAGE: park the Tiger on the pad and stand the enemy tanks downrange as INERT targets —
    /// no AI, no return fire, but fully destroyable. Ranges span the TZF's working band (100–600 m)
    /// so the stadia read + the three magnification stages have something to bite on. Called after
    /// building the garage world.
    pub fn garage_setup(&mut self) {
        self.targets_only = true;
        let (px, pz) = (0.0f32, -180.0);
        self.player = Tank::new(px, pz, 0.0, &self.hf, tank::TIGER_PARTS); // hull facing downrange (+Z)
        self.spawn_xz = (px, pz);
        // Enemy targets threaded across the countryside at increasing range (mixed T-34 / SU-100).
        let spots = [
            (0usize, -34.0, 140.0),
            (1usize, 48.0, 300.0),
            (2usize, -80.0, 240.0),
            (3usize, 20.0, 470.0),
            (4usize, 90.0, 190.0),
            (5usize, -50.0, 600.0),
        ];
        for (i, e) in self.enemies.iter_mut().enumerate() {
            if let Some(&(_, x, z)) = spots.iter().find(|(k, _, _)| *k == i) {
                e.pos = Vec3::new(x, self.hf.height_at(x, z), z);
                e.heading = (px - x).atan2(pz - z); // broadside-ish toward the firing line
                e.alive = true;
            } else {
                e.alive = false;
                e.pos = Vec3::new(-3000.0, 0.0, -3000.0);
            }
        }
        println!(
            "[garage] gunnery yard: {} inert targets at 100–600 m, no return fire",
            spots.len()
        );
    }

    /// The legacy grey smoke-box FX slot — exposed so the headless demos address it through `Slots`
    /// instead of the old `mesh_count()-13` tail arithmetic. Scene ordering is no longer load-bearing
    /// for anything outside `Game::build`.
    pub fn smoke_slot(&self) -> usize {
        self.slots.smoke
    }

    /// Drive the coaxial / hull MG34 for the frame. `firing` = trigger held; `coax` picks the gun
    /// (true = coaxial, down the gunner's sight; false = hull/bow MG). Paces rounds off a firing-rate
    /// timer (~13/s) so it never spawns one-per-frame, and runs a LIGHT barrel-heat model so the gun
    /// can't be held forever: heat rises while firing and, on overheat, LOCKS the trigger out until it
    /// cools back below MG_COOL_RESUME (a brief forced pause). Firing is gated on the player being
    /// alive AND the crewman who mans this gun (coax = GUNNER, hull = RADIO-OPERATOR) AND the MG
    /// module (CoaxMg / HullMg) still being functional — a dead crewman or a wrecked module silences
    /// it (reads those exact damage-model flags).
    fn update_mg(&mut self, dt: f32, firing: bool, coax: bool) {
        let crew_ok = if coax {
            self.crew_ok(CrewRole::Gunner)
        } else {
            self.crew_ok(CrewRole::Radioman)
        };
        let module_ok = if coax {
            self.module_ok(ModuleKind::CoaxMg)
        } else {
            self.module_ok(ModuleKind::HullMg)
        };
        let can_fire = self.player.alive && firing && crew_ok && module_ok && !self.mg_overheated;
        self.mg_hot = can_fire; // the audio block (near set_engine) drives the MG loop off this

        if can_fire {
            self.mg_heat = (self.mg_heat + MG_HEAT_PER_SEC * dt).min(1.0);
            if self.mg_heat >= 1.0 {
                self.mg_overheated = true;
            }
            // Emit as many rounds as the elapsed time allows (usually 0–1 per frame at 60 fps) so the
            // cyclic rate is frame-rate independent without ever bursting a whole belt in one frame.
            self.mg_timer -= dt;
            let mut guard = 0;
            while self.mg_timer <= 0.0 && guard < 4 {
                self.mg_timer += MG_FIRE_INTERVAL;
                self.fire_mg_round(coax);
                guard += 1;
            }
        } else {
            // Not firing: shed heat, clear the overheat lock once cool enough, and reset the
            // sub-interval so the first round after re-pulling the trigger fires promptly.
            self.mg_heat = (self.mg_heat - MG_COOL_PER_SEC * dt).max(0.0);
            if self.mg_overheated && self.mg_heat <= MG_COOL_RESUME {
                self.mg_overheated = false;
            }
            self.mg_timer = 0.0;
        }

        if self.mg_flash_t > 0.0 {
            self.mg_flash_t -= dt;
        }
    }

    /// Spawn ONE MG round from the live port (coax or hull). Hitscans to the nearest living T-34 or
    /// the terrain, arms a small impact (armour sparks vs a snow/dirt spurt), streams a thin/fast
    /// tracer on the cadence, and flickers the muzzle. Deterministic dispersion off the round counter
    /// (no rand). Crucially it does NOT damage tanks — the soft-target consequence goes through
    /// `mg_hit` (a no-op against armour by design).
    fn fire_mg_round(&mut self, coax: bool) {
        let (mpos, mdir) = if coax {
            self.player.coax_muzzle()
        } else {
            self.player.hull_mg_muzzle()
        };
        self.mg_round = self.mg_round.wrapping_add(1);
        let n = self.mg_round;
        // Deterministic fire-cone jitter in [-1,1]^2 (same counter-hash style as the enemy dispersion).
        let j = |k: u32| ((((k.wrapping_mul(2654435761)) >> 8) & 0xffff) as f32 / 32768.0) - 1.0;
        let dir = (mdir + Vec3::X * (j(n) * MG_DISP) + Vec3::Y * (j(n.wrapping_add(53)) * MG_DISP))
            .normalize();

        // Hitscan: nearest living T-34 (clatter) else terrain (spurt), out to MG_RANGE.
        let mut hit_t = MG_RANGE;
        let mut hit_enemy: Option<usize> = None;
        for (i, e) in self.enemies.iter().enumerate() {
            if !e.alive {
                continue;
            }
            if let Some(t) = ray_sphere(mpos, dir, e.center(), 3.6) {
                if t < hit_t {
                    hit_t = t;
                    hit_enemy = Some(i);
                }
            }
        }
        if let Some(t) = ray_terrain(mpos, dir, &self.hf, hit_t) {
            if t < hit_t {
                hit_t = t;
                hit_enemy = None;
            }
        }
        let hit_pos = mpos + dir * hit_t;

        // Muzzle flash at the port (flicker persistence re-armed every round → stroboscopic chatter).
        self.mg_flash_t = MG_FLASH_DUR;
        self.mg_flash_pos = mpos;

        // Tracer on the cadence: thin, fast, small.
        if n % MG_TRACER_EVERY == 0 {
            let k = self.next_mg_tracer;
            self.next_mg_tracer = (self.next_mg_tracer + 1) % N_MG_TRACER;
            self.mg_tracers[k] = MgTracer {
                active: true,
                t: 0.0,
                dur: (hit_t / MG_TRACER_SPEED).max(0.02),
                from: mpos,
                to: hit_pos,
                dir,
            };
        }

        // Impact spurt every round (visual); sound only every Nth round (13/s would flood the sinks).
        let tank = hit_enemy.is_some();
        {
            let k = self.next_mg_impact;
            self.next_mg_impact = (self.next_mg_impact + 1) % N_MG_IMPACT;
            self.mg_impacts[k] = MgImpact {
                t: 0.06,
                dur: 0.06,
                pos: hit_pos,
                tank,
            };
        }
        if n % MG_SOUND_EVERY == 0 {
            if tank {
                // Rounds rattling off a T-34's armour — a light metallic gravel clatter (they CLATTER,
                // they do NOT penetrate). Reuse the armour-clang variants at a low, non-cannon volume.
                let c = crate::audio::Sfx::ARMOR_CLANGS;
                let pick = c[(n as usize) % c.len()];
                self.schedule_sound_from(pick, hit_pos, VOL_MG_GRAVEL);
            } else {
                // Rounds spitting up snow/dirt on the terrain.
                self.schedule_sound_from(crate::audio::Sfx::ImpactGround, hit_pos, VOL_MG_GROUND);
            }
        }

        // Terminal effect on whatever was hit. Against armour this is intentionally inert.
        self.mg_hit(hit_pos, hit_enemy);
    }

    /// Terminal MG-round resolution. MGs are USELESS against armour: a hit on a T-34 only clatters +
    /// sparks (handled by the caller) and does ZERO damage — no penetration, no HP chip, no kill.
    /// They ARE lethal to SOFT targets (infantry), but there are none in the tank scene yet. This is
    /// the SEAM where an infantryman caught in the burst would be dropped; kept side-effect-free on
    /// tanks by design so the MG can never knock one out.
    fn mg_hit(&mut self, _point: Vec3, _hit_enemy: Option<usize>) {
        // SEAM (Absitzen / infantry pass): resolve a lethal soft-target hit here — a rifleman in the
        // burst cone at `_point` goes down. Deliberately does nothing to a T-34 (`_hit_enemy`): the
        // MG only ever clatters off armour.
    }

    /// Enemy T-34 combat AI: ADVANCE toward the player (splitting to the flanks so the swarm
    /// envelops rather than bunching head-on), slew the turret on, and fire on the move at a slow,
    /// inaccurate cadence. Driving is how a T-34 gets from the Tiger's immune frontal arc round to
    /// its penetrable sides — so movement, not raw damage, is what makes the fight lethal.
    /// Deliberately gentler than the player (longer reload, dispersion). Uncoordinated for now —
    /// bounding overwatch / role assignment is the NEXT step.
    fn enemy_ai(&mut self, dt: f32) {
        // DEV GARAGE: targets are inert — skip detection, maneuver, and fire entirely. They still
        // settle on the terrain and take player hits (that path is elsewhere), they just never act.
        if self.targets_only {
            for e in self.enemies.iter_mut().filter(|e| e.alive) {
                e.update(0.0, 0.0, 0.0, 0.0, dt, &self.hf);
            }
            return;
        }
        // Detection + platoon alert state machine first: accrue the per-enemy detection meters,
        // resolve the 5-second rule, and log every SPOTTED / platoon transition. The per-enemy
        // behaviour branch below reads the fresh alert state to decide who may track and fire.
        self.update_stealth(dt);

        for i in 0..self.enemies.len() {
            if !self.enemies[i].alive {
                continue;
            }
            let mode = self.enemy_mode(i);
            // A casemate SU-100 aims by turning its whole HULL (no turret), and holds at long standoff
            // instead of charging — so its TRACK maneuver + lay take their own branch (overwatch sniper).
            let casemate = self.enemies[i].kind.casemate();

            // Maneuver by mode. A TRACKING T-34 flanks the known player (the only way onto the Tiger's
            // penetrable sides); a TRACKING SU-100 holds overwatch at long range; a SEARCHING tank
            // drives toward the platoon's last-known area; an UNAWARE tank just advances on a slow
            // patrol. `update` ticks its reload.
            // BRAWL movement: pursue the other team's nearest tank (the scripted maneuvers all
            // navigate relative to the PLAYER — useless in a melee).
            let brawl_target: Option<Vec3> = if self.brawl {
                let me = self.enemies[i].center();
                (0..self.enemies.len())
                    .filter(|&o| o != i && o % 2 != i % 2 && self.enemies[o].alive)
                    .min_by(|&a, &b| {
                        self.enemies[a]
                            .center()
                            .distance(me)
                            .total_cmp(&self.enemies[b].center().distance(me))
                    })
                    .map(|o| self.enemies[o].center())
            } else {
                None
            };
            // NEUROEVOLUTION seam: with a policy installed, the net proposes maneuver + a fire
            // hold; without one, today's scripted maneuvers run. Everything downstream (gun lay,
            // reload cadence, stealth mode gates, ballistics) is identical for both.
            let (drive, steer, net_fire) = if let Some(t) = brawl_target {
                // Simple pursuit: close to fighting range, then circle for the flank.
                let e = &self.enemies[i];
                let to = t - e.center();
                let want = to.x.atan2(to.z);
                let mut dh = want - e.heading;
                while dh > std::f32::consts::PI {
                    dh -= std::f32::consts::TAU;
                }
                while dh < -std::f32::consts::PI {
                    dh += std::f32::consts::TAU;
                }
                let range = to.length();
                let drive = if range > 70.0 { 1.0 } else { 0.45 };
                (drive, dh.clamp(-1.0, 1.0), true)
            } else if let Some(net) = &self.enemy_policy {
                let a = net.forward(&self.executor_obs(i));
                (a[0], a[1], a[2] > 0.0)
            } else {
                let (d, st) = match mode {
                    AiMode::Track => {
                        if casemate {
                            self.overwatch_maneuver(i)
                        } else {
                            self.enemy_maneuver(i)
                        }
                    }
                    AiMode::Search => self.search_maneuver(i),
                    AiMode::Patrol => self.patrol_maneuver(i),
                };
                (d, st, true)
            };
            self.enemies[i].update(drive, steer, 0.0, 0.0, dt, &self.hf);

            if !self.player.alive {
                continue;
            }

            // Gun lay + fire by mode. ONLY a tank that has spotted the player (or whose platoon is
            // Engaged) lays on and fires. A turreted tank slews the turret; a casemate lays by having
            // the maneuver put the hull on target, then just elevating the gun (`casemate_aim`).
            let lay = |e: &mut Tank, tgt: Vec3| {
                if casemate {
                    e.casemate_aim(tgt, dt)
                } else {
                    e.ai_aim(tgt, dt)
                }
            };
            match mode {
                AiMode::Track => {
                    // BRAWL: lay on the other team's nearest tank; else the player-threat.
                    let target = match (self.brawl, brawl_target) {
                        (true, Some(t)) => t,
                        (true, None) => continue,  // foe team wiped — hold
                        _ => self.threat_center(), // the Tiger — or the MAN, when dismounted
                    };
                    let laid = lay(&mut self.enemies[i], target);
                    if laid && self.enemies[i].reload <= 0.0 && net_fire {
                        self.enemy_fire(i);
                        let skill =
                            f32::from(self.enemy_skill.get(i).copied().unwrap_or(42)) / 100.0;
                        self.enemies[i].reload = ENEMY_RELOAD * (1.16 - 0.42 * skill);
                    }
                    // Dismounted: the coax chews at the man between main-gun rounds.
                    if matches!(self.mode, Mode::Foot) {
                        self.enemy_mg_burst(i, dt);
                    }
                }
                AiMode::Search => {
                    let scan = self.search_scan_point(i);
                    lay(&mut self.enemies[i], scan); // sweep a last-known area — never fires
                }
                AiMode::Patrol => {
                    let scan = self.patrol_scan_point(i);
                    lay(&mut self.enemies[i], scan); // idle scan across the front — never fires
                }
            }
        }
    }

    /// This frame's behaviour mode for enemy `i`, derived from the stealth state. An individually
    /// spotted tank (or any member of an Engaged platoon) TRACKS; a Searching platoon's tanks SEARCH;
    /// everyone else is on PATROL, blissfully unaware of the Tiger in their midst.
    fn enemy_mode(&self, i: usize) -> AiMode {
        if self.stealth.spotted[i] {
            return AiMode::Track;
        }
        match self.stealth.platoon_state(i) {
            AlertState::Engaged => AiMode::Track,
            AlertState::Searching => AiMode::Search,
            _ => AiMode::Patrol, // Unaware / Eliminated
        }
    }

    /// Accrue every enemy's detection meter, fire the SPOTTED events, and run the per-platoon alert
    /// state machine (the 5-second rule, re-acquisition, lose-contact, and platoon wipe). Everything
    /// the harness reads to confirm the stealth model is working is logged here.
    /// What the swarm perceives and engages: the Tiger — or, dismounted, the MAN. The parked,
    /// cold hull is ignored while you're on foot (no motor, no muzzle flash, a grey lump).
    fn threat_center(&self) -> Vec3 {
        match self.mode {
            Mode::Tank => self.player.center(),
            Mode::Foot => self.foot.pos + Vec3::Y * 1.1,
        }
    }

    fn update_stealth(&mut self, dt: f32) {
        if !self.player.alive {
            self.stealth.t += dt;
            return;
        }
        let player_pos = self.threat_center();
        // Detection envelope by mode: a WALKING MAN is far harder to spot than a 57-tonne tank —
        // roughly a third of the envelope, crouching cuts it again (low silhouette in the snow).
        let (radius, move_mult) = match self.mode {
            Mode::Tank => {
                let sf = (self.player.speed.abs() / tank::TOP_SPEED).clamp(0.0, 1.0);
                let open = if self.hatch_open { 1.18 } else { 1.0 }; // an open hatch = a bigger signature
                                                                     // SILENT WATCH: engine off collapses the envelope — a cold, still, buttoned
                                                                     // Tiger is a grey rock in the snow. The minute after a restart is the opposite.
                let silent = if self.engine_on { 1.0 } else { 0.42 };
                let bark = if self.start_noise_t > 0.0 { 1.7 } else { 1.0 };
                (
                    stealth::detection_radius(sf, self.night) * open * silent * bark,
                    stealth::movement_mult(sf) * if self.engine_on { 1.0 } else { 0.55 } * bark,
                )
            }
            Mode::Foot => {
                let sf = self.foot.speed_frac;
                let crouch = 1.0 - 0.45 * self.foot.crouch_t;
                (
                    stealth::detection_radius(sf, self.night) * 0.34 * crouch,
                    stealth::movement_mult(sf),
                )
            }
        };

        // Compute each living enemy's instantaneous accrual rate from its sightline + LOS; hand the
        // whole vector to the (pure, testable) stealth state machine to accrue meters and transition
        // the platoons.
        let n = self.enemies.len();
        let mut rate = vec![0.0f32; n];
        let mut alive = vec![false; n];
        for i in 0..n {
            alive[i] = self.enemies[i].alive;
            if !alive[i] {
                continue;
            }
            // Sightline: the blind T-34 sees best out its hull FRONT, but the crew traverse the
            // turret to look around — so the effective cone is the better of hull- and turret-forward.
            let ecenter = self.enemies[i].center();
            let hull_f = self.enemies[i].forward();
            let turret_f = self.enemies[i].turret_forward();
            let mut to = player_pos - ecenter;
            to.y = 0.0;
            let dist = to.length();
            let dir = if dist > 1e-3 { to / dist } else { Vec3::Z };
            let cos = hull_f.dot(dir).max(turret_f.dot(dir));
            let r = stealth::accrual_rate(cos, dist, radius, move_mult);
            // Line of sight: terrain (a rise, a hull-down position) between the tank and the player
            // kills the accrual — cover is camouflage.
            rate[i] = if r > 0.0 && self.los_clear(ecenter, player_pos) {
                let skill = f32::from(self.enemy_skill.get(i).copied().unwrap_or(42)) / 100.0;
                r * (0.72 + 0.68 * skill)
            } else {
                0.0
            };
        }

        self.stealth.tick(dt, &rate, &alive, player_pos);
    }

    /// Is the straight line from `from` to `to` clear of BOTH cover and terrain? A cover box on the
    /// sightline (a building, wall shell, rubble berm, container, or a killed tank's hull) breaks it
    /// outright — this is what makes hiding behind hard cover genuinely conceal the Tiger. Terrain is
    /// then checked with the raymarch: a hit short of the target means a rise/berm blocks the line
    /// (hull-down concealment).
    fn los_clear(&self, from: Vec3, to: Vec3) -> bool {
        let d = to - from;
        let dist = d.length();
        if dist < 1e-3 {
            return true;
        }
        // Hard cover first — cheap slab tests against the (few dozen) occluder boxes.
        if stealth::los_blocked(&self.occluders, from, to) {
            return false;
        }
        let dir = d / dist;
        match ray_terrain(from, dir, &self.hf, dist) {
            Some(t) => t >= dist - 3.0, // only "blocked" if the ground rises before the target
            None => true,
        }
    }

    /// Unaware patrol: a slow advance on the tank's own heading with a gentle weave — believable
    /// movement, not a charge, and NO flanking (it has no idea the Tiger is out there).
    fn patrol_maneuver(&self, i: usize) -> (f32, f32) {
        if self.enemy_concealed.get(i).copied().unwrap_or(false) {
            return (0.0, 0.0);
        }
        let steer = 0.15 * (self.stealth.t * 0.2 + i as f32 * 1.3).sin();
        (0.32, steer)
    }

    /// Searching: drive toward the platoon's last-known area, easing off once close so the tank
    /// mills and sweeps instead of driving straight through it.
    fn search_maneuver(&self, i: usize) -> (f32, f32) {
        let e = &self.enemies[i];
        let lk = self.stealth.platoons[self.stealth.platoon_index_of(i)].last_known;
        let mut to = lk - e.pos;
        to.y = 0.0;
        let dist = to.length().max(0.001);
        let dir = to / dist;
        let want = dir.x.atan2(dir.z);
        let mut dh = want - e.heading;
        while dh > std::f32::consts::PI {
            dh -= std::f32::consts::TAU;
        }
        while dh < -std::f32::consts::PI {
            dh += std::f32::consts::TAU;
        }
        let steer = (dh * 2.5).clamp(-1.0, 1.0);
        let drive = if dist < 25.0 { 0.2 } else { 0.6 };
        (drive, steer)
    }

    /// Turret scan target for an unaware tank: a point sweeping slowly across its own front.
    fn patrol_scan_point(&self, i: usize) -> Vec3 {
        let e = &self.enemies[i];
        let ang = 0.6 * (self.stealth.t * 0.3 + i as f32 * 2.1).sin();
        let dir = Quat::from_rotation_y(ang) * e.forward();
        e.center() + dir * 120.0 + Vec3::new(0.0, 1.0, 0.0)
    }

    /// Turret scan target for a searching tank: sweeping laterally across the last-known area.
    fn search_scan_point(&self, i: usize) -> Vec3 {
        let e = &self.enemies[i];
        let lk = self.stealth.platoons[self.stealth.platoon_index_of(i)].last_known;
        let mut to = lk - e.center();
        to.y = 0.0;
        let dist = to.length().max(0.001);
        let dir = to / dist;
        let perp = Vec3::new(-dir.z, 0.0, dir.x);
        let sweep = (self.stealth.t * 0.8 + i as f32 * 1.7).sin() * 30.0;
        Vec3::new(lk.x, e.center().y, lk.z) + perp * sweep
    }

    /// Drive/steer command for enemy `i` — a flank maneuver, not a charge. A T-34 can't hurt the
    /// Tiger head-on, so driving straight up its immune front is death. Instead each enemy ORBITS
    /// around the Tiger's frontal arc (moving laterally, holding range so it stays in the harmless
    /// bounce zone) until it's abreast or behind, THEN bores in on the soft flank/rear. Half the
    /// swarm swings left, half right (deterministic by index), so the Tiger gets enveloped.
    fn enemy_maneuver(&self, i: usize) -> (f32, f32) {
        let e = &self.enemies[i];
        let mut to_player = self.player.pos - e.pos;
        to_player.y = 0.0;
        let dist = to_player.length().max(0.001);
        let dir = to_player / dist; // toward the player
        let perp = Vec3::new(-dir.z, 0.0, dir.x); // left of that (ground plane)
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };

        // How deep in the TIGER's frontal arc this enemy still sits (relative to the player's hull
        // facing): 1 = dead ahead of the Tiger, 0 = already on its flank, <0 = behind it.
        let from_player = (e.pos - self.player.pos) * Vec3::new(1.0, 0.0, 1.0);
        let front_dot = self.player.forward().dot(from_player.normalize_or_zero());
        let swing = ((front_dot - 0.15) / 0.85).clamp(0.0, 1.0); // 1 while still in front, 0 on the flank

        // Blend tangential (swing around the front) with radial (close the range). While swinging,
        // back off the range slightly so it doesn't wander into the Tiger's close-front kill zone;
        // once abreast/behind, drive hard onto the penetrable side/rear.
        let radial_w = 0.10 * swing + 1.10 * (1.0 - swing);
        let desired = (perp * (side * (0.8 + 0.7 * swing)) + dir * radial_w).normalize_or_zero();

        let want = desired.x.atan2(desired.z); // heading whose forward is (sin,·,cos)
        let mut dh = want - e.heading;
        while dh > std::f32::consts::PI {
            dh -= std::f32::consts::TAU;
        }
        while dh < -std::f32::consts::PI {
            dh += std::f32::consts::TAU;
        }
        let steer = (dh * 2.5).clamp(-1.0, 1.0); // proportional; hard-over when far off heading

        // Advance hard; ease the throttle through a sharp turn, and coast at knife range so a
        // flanker holds its killing position on the side instead of ramming through the origin.
        let mut drive = 0.95;
        if dh.abs() > 0.7 {
            drive *= 0.5;
        }
        if dist < 30.0 {
            drive *= 0.4;
        }
        (drive, steer)
    }

    /// SU-100 OVERWATCH-SNIPER maneuver — the deliberate counter to the T-34 rush. Unlike a T-34, it
    /// does NOT charge or flank: it holds at LONG standoff and turns its whole HULL to lay the fixed
    /// casemate gun on the Tiger (the casemate has no traverse, so the hull IS the aim). Range-keeping:
    /// if the Tiger closes inside `SU100_STANDOFF_MIN` it REVERSES to open the range back up (front +
    /// gun kept on the threat, never brawling); if the Tiger drifts beyond `SU100_STANDOFF_MAX` it
    /// creeps forward to re-establish LOS; inside the band it sits still and snipes. A tiny deterministic
    /// hashed jog on the hold keeps a sniping pair from looking frozen (matches the no-rand style).
    fn overwatch_maneuver(&self, i: usize) -> (f32, f32) {
        let e = &self.enemies[i];
        let mut to = self.player.pos - e.pos;
        to.y = 0.0;
        let dist = to.length().max(0.001);
        let dir = to / dist;
        // Steer the HULL to point at the player — a welded casemate lays entirely by hull heading.
        let want = dir.x.atan2(dir.z);
        let mut dh = want - e.heading;
        while dh > std::f32::consts::PI {
            dh -= std::f32::consts::TAU;
        }
        while dh < -std::f32::consts::PI {
            dh += std::f32::consts::TAU;
        }
        let steer = (dh * 2.5).clamp(-1.0, 1.0);
        // Range discipline: back off if closed, creep in if too far, hold + snipe inside the band.
        let drive = if dist < SU100_STANDOFF_MIN {
            -0.7 // Tiger too close → reverse, keeping the gun on it (a TD does not brawl)
        } else if dist > SU100_STANDOFF_MAX {
            0.35 // too far to reliably lay → creep forward into a firing position
        } else {
            // In the band: hold station, with a barely-perceptible deterministic jog so an overwatch
            // pair isn't dead-frozen. Never enough to leave the sniper range.
            0.06 * (self.stealth.t * 0.15 + i as f32 * 2.7).sin()
        };
        (drive, steer)
    }

    /// Hitscan from enemy `i`'s muzzle toward the player, with a small deterministic dispersion so
    /// the T-34 misses sometimes. Grabs the next FX pool slot (round-robin) so overlapping shots
    /// each animate independently. Reduces player HP on a hit; knocks out the Tiger at 0.
    /// Coax-MG burst against the DISMOUNTED player (Track mode, close). Paced per enemy; hit
    /// chance falls with range, crouch and sprint-jink; a hit WOUNDS (infantry-scale — the man is
    /// not a one-burst kill at range), and every miss kicks visible snow around him. The main gun
    /// stays live too (`enemy_fire` resolves it against the man) — the MG is the accurate one.
    fn enemy_mg_burst(&mut self, i: usize, dt: f32) {
        self.enemy_mg_cd[i] -= dt;
        if self.enemy_mg_cd[i] > 0.0 {
            return;
        }
        let kind = self.enemies[i].kind;
        let from = self.enemies[i]
            .turret_model()
            .transform_point3(Vec3::new(0.20, 1.05, 1.15)); // coax port
        let target = self.foot.pos + Vec3::Y * (0.6 + 0.55 * (1.0 - self.foot.crouch_t));
        let range = from.distance(target);
        if range > 260.0 || !self.los_clear(from, target) {
            return; // out of MG reach / masked — re-check next frame
        }
        self.enemy_mg_cd[i] = 1.3;
        self.ai_tick = self.ai_tick.wrapping_add(1);
        let p = (0.42 - range / 700.0 - 0.16 * self.foot.crouch_t - 0.10 * self.foot.speed_frac)
            .clamp(0.04, 0.5);
        // Muzzle flash on the infantry FX pool + a ranged crack through the sound queue.
        let k = self.next_inf_fx;
        self.next_inf_fx = (self.next_inf_fx + 1) % INF_FX;
        self.inf_flash_t[k] = 0.07;
        self.inf_flash_pos[k] = from;
        self.schedule_sound_from(crate::audio::Sfx::RifleShot, from, 0.9);
        if hash01(self.ai_tick.wrapping_mul(48271)) < p {
            if !self.god {
                self.foot.hp -= 24.0;
            }
            println!(
                "[absitzen] MG burst from {} #{i} HIT at {:.0} m — hp {:.0}",
                kind.name(),
                range,
                self.foot.hp.max(0.0)
            );
        } else {
            // Snow spurts walk around him — suppression you can SEE.
            let jx = hash01(self.ai_tick.wrapping_mul(7919)) - 0.5;
            let jz = hash01(self.ai_tick.wrapping_mul(104_729)) - 0.5;
            let m = self.foot.pos + Vec3::new(jx * 6.0, 0.0, jz * 6.0);
            self.spawn_snow_puff(Vec3::new(m.x, self.hf.height_at(m.x, m.z), m.z), 0.8);
        }
    }

    fn enemy_fire(&mut self, i: usize) {
        if let Some(runtime) = self.campaign.as_mut() {
            runtime.battle_pressure = (runtime.battle_pressure + 5.0).min(100.0);
        }
        let kind = self.enemies[i].kind; // T-34 76 mm vs SU-100 100 mm — drives the penetration roll
        let (mpos, mdir) = self.enemies[i].muzzle();

        // Dispersion grows with the firer's speed: a T-34 shooting on the move (as it closes/flanks)
        // is far less accurate than one that's stopped. counter-based jitter in [-1,1]^2. A STOPPED
        // shooter stays tight (rounds connect and CLANG); a flanker firing on the move genuinely
        // SCATTERS — some rounds crack past and some kick up snow at the Tiger's feet (near-miss
        // puffs). Close-range shots still connect (disp·range is tiny up close), so the lethal
        // knife-fight hits are unaffected — only mid/long-range fire-on-the-move throws near-misses.
        self.ai_tick = self.ai_tick.wrapping_add(1);
        let j = |n: u32| ((((n.wrapping_mul(2654435761)) >> 8) & 0xffff) as f32 / 32768.0) - 1.0;
        let speed_frac = (self.enemies[i].speed.abs() / tank::TOP_SPEED).clamp(0.0, 1.0);
        let skill = f32::from(self.enemy_skill.get(i).copied().unwrap_or(42)) / 100.0;
        let disp = (0.004 + 0.014 * speed_frac) * (1.18 - 0.48 * skill);
        let dir = (mdir
            + Vec3::X * (j(self.ai_tick) * disp)
            + Vec3::Y * (j(self.ai_tick.wrapping_add(97)) * disp))
            .normalize();

        let max = 1500.0;
        let mut hit_t = max;
        let mut hit_player = false;
        let mut hit_struct: Option<usize> = None; // a stray enemy round can knock down a wall too
                                                  // Dismounted: the shell is aimed at (and resolved against) the MAN, not the parked hull.
                                                  // A direct 76/100 mm hit on a human is not a damage roll. Near misses land a snow-kick at
                                                  // his feet through the normal ground path — the terror is free.
        if matches!(self.mode, Mode::Foot) {
            if let Some(t) = ray_sphere_near(mpos, dir, self.foot.pos + Vec3::Y * 0.9, 0.55, 2.0) {
                if t < max {
                    let ipos = mpos + dir * t;
                    self.foot.hp = 0.0;
                    println!(
                        "[absitzen] DIRECT HIT — a {} shell found you on foot",
                        self.enemies[i].kind.name()
                    );
                    self.spawn_snow_puff(
                        Vec3::new(ipos.x, self.hf.height_at(ipos.x, ipos.z), ipos.z),
                        1.0,
                    );
                }
            }
        }
        if matches!(self.mode, Mode::Tank) {
            if let Some(t) = ray_sphere(mpos, dir, self.player.center(), 3.6) {
                if t < hit_t {
                    hit_t = t;
                    hit_player = true;
                    hit_struct = None;
                }
            }
        }
        // Fellow enemy tanks block shells too — friendly fire is real (a T-34 crossing an
        // SU-100's lay eats the round), and it's what makes T-34-vs-T-34 BRAWLS possible.
        let mut hit_enemy: Option<usize> = None;
        for oi in 0..self.enemies.len() {
            if oi == i || !self.enemies[oi].alive {
                continue;
            }
            if let Some(t) = ray_sphere(mpos, dir, self.enemies[oi].center(), 3.4) {
                if t < hit_t {
                    hit_t = t;
                    hit_enemy = Some(oi);
                    hit_player = false;
                    hit_struct = None;
                }
            }
        }
        if let Some(t) = ray_terrain(mpos, dir, &self.hf, hit_t) {
            if t < hit_t {
                hit_t = t;
                hit_player = false;
                hit_struct = None;
            }
        }
        if let Some((t, di)) = self.ray_destructible(mpos, dir, hit_t) {
            if t < hit_t {
                hit_t = t;
                hit_player = false;
                hit_struct = Some(di);
            }
        }
        let hit_pos = mpos + dir * hit_t;

        // Resolve the outcome vs the Tiger's armor (arc + range), or classify the miss as a close
        // supersonic crack vs a wide/ground round. Applied when the tracer lands. A bounce does
        // NOTHING but clang; the discrete crew/module consequence of a penetration is rolled on
        // impact (see `resolve_player_penetration`).
        let range = mpos.distance(self.player.center());
        let mut hit_facing = Arc::Front; // armor arc struck (only meaningful on a hit); carried to impact
        let mut near_pt = hit_pos; // closest-approach point (where a near miss snapped by); default = impact
        let mut shot_ev: Option<crate::ballistics::ImpactEvent> = None;
        let mut shot_turret = false;
        let outcome = if hit_player {
            let arc = hit_arc(self.player.forward(), mpos - self.player.center());
            hit_facing = arc;
            self.armor_tick = self.armor_tick.wrapping_add(1);
            // PHYSICAL, kind-aware resolution: the firing kind's real shell (T-34 → BR-350B, SU-100 →
            // BR-412) vs the Tiger's real plate on the struck face. The SU-100 defeating the front at
            // range while the 76 mm only ever clangs off it now falls out of the DeMarre solve.
            let turret = hash01(self.armor_tick.wrapping_mul(2777)) < 0.42;
            shot_turret = turret;
            let facing = facing_from(arc, turret);
            let normal = face_normal(arc, self.player.forward(), mpos - self.player.center());
            let horiz = crate::ballistics::horiz_obliquity_deg(dir, normal);
            let ev = crate::ballistics::resolve(
                enemy_shell(kind),
                range,
                crate::ballistics::ArmorScheme::TigerI.plate(facing),
                horiz,
                self.armor_tick,
            );
            shot_ev = Some(ev);
            let pen = matches!(ev.terminal, crate::ballistics::Terminal::Penetration);
            println!(
                "[kessel] {} #{i} → {} ({}) at {:.0} m: {:.0} mm pen vs {:.0} mm eff @ {:.0}° → {:?}",
                kind.name(), arc.name(), ev.plate.name, range, ev.pen_mm, ev.effective_mm,
                ev.impact_angle_deg, ev.terminal);
            if pen {
                Outcome::Penetrate
            } else {
                Outcome::Bounce
            }
        } else if let Some(oi) = hit_enemy {
            // Enemy-vs-enemy: the firing kind's real shell vs the STRUCK tank's real armor.
            let e = &self.enemies[oi];
            let arc = hit_arc(e.forward(), mpos - e.center());
            self.armor_tick = self.armor_tick.wrapping_add(1);
            let facing = facing_from(arc, hash01(self.armor_tick.wrapping_mul(2777)) < 0.42);
            let normal = face_normal(arc, e.forward(), mpos - e.center());
            let horiz = crate::ballistics::horiz_obliquity_deg(dir, normal);
            let range_e = mpos.distance(e.center());
            let ev = crate::ballistics::resolve(
                enemy_shell(kind),
                range_e,
                enemy_armor(e.kind).plate(facing),
                horiz,
                self.armor_tick,
            );
            let pen = matches!(ev.terminal, crate::ballistics::Terminal::Penetration);
            println!(
                "[kessel] {} #{i} → {} #{oi} {} at {:.0} m: {:.0} mm pen vs {:.0} mm eff → {:?}",
                kind.name(),
                e.kind.name(),
                arc.name(),
                range_e,
                ev.pen_mm,
                ev.effective_mm,
                ev.terminal
            );
            if pen {
                Outcome::Penetrate
            } else {
                Outcome::Bounce
            }
        } else if hit_struct.is_some() {
            // The round terminated on a standing structure downrange — no near-miss at the Tiger.
            Outcome::Ground
        } else {
            // Closest approach of the round to the player: a near miss cracks past. Capture that
            // closest point (`near_pt`) so the snow puff kicks up on the ground where the round
            // snapped by the Tiger, not downrange where a wide round eventually buries itself.
            let oc = self.player.center() - mpos;
            let tca = oc.dot(dir);
            let cpa = mpos + dir * tca.max(0.0);
            let near =
                tca > 0.0 && tca < hit_t && cpa.distance(self.player.center()) < NEAR_MISS_RADIUS;
            if near {
                near_pt = cpa;
                println!(
                    "[kessel] {} #{i} → near miss (crack) at {:.0} m",
                    kind.name(),
                    range
                );
                Outcome::NearMiss
            } else {
                Outcome::Ground
            }
        };

        // claim the next pool slot, wrapping around so a busy swarm cycles through the FX meshes.
        let slot = self.next_e_fx;
        self.next_e_fx = (self.next_e_fx + 1) % E_FX_POOL;
        let shot = &mut self.e_shots[slot];
        shot.tracer_active = true;
        shot.tracer_t = 0.0;
        // Real time of flight for rounds that will strike the Tiger; visual constant otherwise.
        shot.tracer_dur = shot_ev
            .map(|e| e.flight_time)
            .unwrap_or(hit_t / 450.0)
            .max(0.05);
        shot.tracer_from = mpos;
        shot.tracer_to = hit_pos;
        shot.tracer_dir = dir;
        shot.outcome = outcome;
        shot.arc = hit_facing;
        shot.ev = shot_ev;
        shot.turret_hit = shot_turret;
        shot.near_pt = near_pt;
        shot.pending_struct = hit_struct;
        shot.hit_enemy = hit_enemy;
        // The enemy's report is DISTANT — you see its muzzle flash downrange, then hear the boom a
        // beat later (delay = range/343). The report itself is SYNTHESIZED for this gun at this
        // range: air absorption has eaten the crack, leaving the low body, panned to its bearing
        // (flash→bang — the kessel-sound-pillar centerpiece). The sampled EnemyGun clip stays as a
        // close-range layer so nearby muzzle blasts keep their bite.
        {
            let d = mpos.distance(self.threat_center());
            let report = crate::acoustics::synth_gun_report(
                enemy_shell(kind).caliber_mm,
                d,
                0.0, // source-domain centre; live head bearing is applied when the wave arrives
                self.ai_tick.wrapping_mul(977),
            );
            let atten = SOUND_REF_DIST / (SOUND_REF_DIST + d);
            self.scheduled.push(ScheduledSound {
                delay: d / SPEED_OF_SOUND,
                sfx: crate::audio::Sfx::EnemyGun,
                volume: VOL_ENEMY_GUN * atten,
                source: mpos,
                buf: Some(report),
            });
            if d < 120.0 {
                // Close gun: keep the sampled crack layered under the synth body.
                self.schedule_sound_from(crate::audio::Sfx::EnemyGun, mpos, VOL_ENEMY_GUN * 0.6);
            }
        }
    }

    /// Apply a shell that HOLED the player's armour. This is never a flat HP chip: it computes a
    /// SPALL CONE from the entry point + shell direction into the crew compartment and hits whatever
    /// crew/modules fall in it (deterministic). Consequences follow — the ammunition detonating is an
    /// instant catastrophic kill; a fuel/engine hit lights a fire; the crew being wiped out knocks the
    /// Tiger out. `arc` names the facing (for the death cause); the world geometry drives the spall.
    /// `turret_hit`: the shot-time turret/hull band roll (see `EnemyShot::turret_hit`) so the
    /// CLANG's synthesized panel and the spall compartment agree; `None` re-rolls (headless-only).
    fn resolve_player_penetration(
        &mut self,
        arc: Arc,
        shooter: Vec3,
        entry_w: Vec3,
        shell_dir_w: Vec3,
        turret_hit: Option<bool>,
    ) {
        if !self.player.alive {
            return;
        }
        if self.god {
            return;
        } // ARENA invincibility: the hit rings and flashes, nothing breaks
          // Bring the entry point + shell direction into the tank's own frame. Clamp the entry (from
          // the bounding-sphere hit) onto the hull skin, and quantise its HEIGHT to the hull or turret
          // band — the round holed one or the other, and that picks which compartment the spall sweeps.
        let hull = self.player.hull_model();
        let hull_inv = hull.inverse();
        let mut entry = hull_inv.transform_point3(entry_w);
        entry.x = entry.x.clamp(-1.9, 1.9);
        entry.z = entry.z.clamp(-3.2, 3.2);
        self.armor_tick = self.armor_tick.wrapping_add(1);
        let turret_hit =
            turret_hit.unwrap_or_else(|| hash01(self.armor_tick.wrapping_mul(2777)) < 0.42); // turret is the smaller target
        entry.y = if turret_hit { 1.95 } else { 1.05 };
        // Spall sprays INWARD: blend the shell path with a pull toward the compartment centre so even
        // a grazing round sprays across the crew instead of straight out through the far wall.
        let dir_local = hull_inv.transform_vector3(shell_dir_w).normalize_or_zero();
        let to_centre = (Vec3::new(0.0, entry.y, 0.0) - entry).normalize_or_zero();
        let mut spall = (dir_local * 0.6 + to_centre * 0.4).normalize_or_zero();
        if spall.length_squared() < 0.1 {
            spall = to_centre;
        }

        let seed = self
            .armor_tick
            .wrapping_mul(2654435761)
            .wrapping_add(if turret_hit { 7 } else { 3 });
        // IT'S JUST YOU: the spall cone is tested against YOUR BODY at its actual interior
        // position — the seat you're posted at right now (later: wherever you're standing, once
        // the walkable interior lands). At the breech when a flank round comes through the turret?
        // That's where you are. Dismounted → nobody aboard, only steel gets hurt.
        let body = matches!(self.mode, Mode::Tank).then(|| {
            hull_inv.transform_point3(self.station_eye(self.station)) - Vec3::new(0.0, 0.35, 0.0)
        });
        let mut res = resolve_spall(entry, spall, body, &mut self.modules, seed);
        // OPEN HATCH: a penetrating round's spall + overpressure vents up through the open cupola
        // and can catch the commander it would otherwise have missed (forgetting to button up is
        // its own death). Only when posted at the cupola, only on a real penetration.
        if self.hatch_open && !res.hit_body && matches!(self.station, Station::Commander) {
            self.armor_tick = self.armor_tick.wrapping_add(1);
            if hash01(self.armor_tick.wrapping_mul(7919)) < 0.5 {
                res.hit_body = true;
                println!("[kessel] SPLINTERS through the OPEN HATCH — you're hit");
            }
        }

        for &kind in &res.hit_modules {
            println!("[kessel] SPALL — {} knocked out", kind.name());
            if kind == ModuleKind::Engine {
                self.cue_engine_death(1.0); // the Maybach takes the spall and sputters out
            }
        }
        self.recompute_hp();

        // ── Consequences / death conditions ──
        // Ammunition struck → INSTANT catastrophic (the enemy turret-pop, applied to the player).
        if res.ammo {
            self.player_catastrophic(shooter, entry_w, shell_dir_w, res);
            return;
        }
        // Fuel/engine hit → a fire that grows toward the ammo (the cook-off timer runs in update_fire).
        if res.fire && !self.fire_active {
            self.fire_active = true;
            self.fire = self.fire.max(0.16);
            self.fire_origin = Some((shooter, entry_w, shell_dir_w));
            println!("[kessel] FIRE — fuel/engine alight in the hull");
        }
        // The spall caught YOUR BODY → that's the death. Position is everything: the same round
        // through the same plate misses a man posted at a different station.
        if res.hit_body {
            self.player.alive = false;
            self.recompute_hp();
            self.cue_engine_death(0.7); // nobody at the controls — the motor winds down
            let cause = match arc {
                Arc::Side | Arc::Rear => DeathCause::Penetration(arc),
                Arc::Front => DeathCause::CrewLost, // the man is lost — the frontal wear-down read
            };
            self.death_cause = Some(cause);
            let spall_world = hull.transform_vector3(spall).normalize_or_zero();
            self.last_fatal = Some(FatalHit {
                shooter_pos: shooter,
                shell_dir: shell_dir_w,
                entry_point: entry_w,
                spall_dir: spall_world,
                hit_body: true,
                hit_modules: res.hit_modules,
                cause,
            });
            println!(
                "[kessel] KILLED — spall caught you at the {:?} station ({})",
                self.station,
                cause.de()
            );
        }
    }

    /// The Maybach dies — the long sputter-out recording. Cued ONCE per life: when the engine
    /// module is knocked out, or when the Tiger is lost with the motor still turning (quieter,
    /// under whatever killed it).
    fn cue_engine_death(&mut self, volume: f32) {
        if self.engine_death_cued {
            return;
        }
        self.engine_death_cued = true;
        if let Some(a) = &self.audio {
            a.play_at_volume(crate::audio::Sfx::EngineDeath, volume);
        }
    }

    /// The player's ammunition detonated — instant catastrophic kill (the enemy turret-pop applied to
    /// the Tiger): big blast + a raging interior inferno + the ammo-detonation death cause + the fatal
    /// record for the replay.
    fn player_catastrophic(
        &mut self,
        shooter: Vec3,
        entry_w: Vec3,
        shell_dir_w: Vec3,
        res: SpallResult,
    ) {
        self.player.alive = false;
        for c in &mut self.crew {
            c.ok = false;
        }
        self.fire_active = true;
        self.fire = FIRE_MAX;
        self.recompute_hp();
        self.cue_engine_death(0.7); // the wreck's motor dies under the blast
        self.death_cause = Some(DeathCause::AmmoDetonation);
        let centre = self.player.center();
        self.spawn_blast(centre + Vec3::new(0.0, 0.8, 0.0), 9.0, 26.0, 0.85);
        self.spawn_debris(centre, 12, 16.0, self.armor_tick.wrapping_mul(2246822519));
        self.schedule_sound_from(crate::audio::Sfx::TurretPop, centre, VOL_KILL);
        self.schedule_sound_from(crate::audio::Sfx::TurretRip, centre, VOL_KILL);
        let mut hit_modules = res.hit_modules;
        if !hit_modules.contains(&ModuleKind::AmmoRack) {
            hit_modules.push(ModuleKind::AmmoRack);
        }
        self.last_fatal = Some(FatalHit {
            shooter_pos: shooter,
            shell_dir: shell_dir_w,
            entry_point: entry_w,
            spall_dir: shell_dir_w.normalize_or_zero(),
            hit_body: true,
            hit_modules,
            cause: DeathCause::AmmoDetonation,
        });
        println!("[kessel] CATASTROPHIC — ammunition detonation, Tiger destroyed.");
    }

    /// The interior fire reached the ammunition — a delayed cook-off catastrophic kill (fuel-fire
    /// death cause). Uses the fire-starting shot's geometry for the fatal record.
    fn kill_fire_cookoff(&mut self) {
        if self.god {
            return;
        } // invincible: the fire burns, the ammo never goes
        self.player.alive = false;
        for c in &mut self.crew {
            c.ok = false;
        }
        if let Some(m) = self
            .modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::AmmoRack)
        {
            m.functional = false;
        }
        self.fire = FIRE_MAX;
        self.recompute_hp();
        self.cue_engine_death(0.7); // the wreck's motor dies under the cook-off
        self.death_cause = Some(DeathCause::FuelFire);
        let centre = self.player.center();
        self.spawn_blast(centre + Vec3::new(0.0, 0.8, 0.0), 9.0, 26.0, 0.9);
        self.spawn_debris(
            centre,
            12,
            16.0,
            self.kill_tick.wrapping_add(777).wrapping_mul(2246822519),
        );
        self.schedule_sound_from(crate::audio::Sfx::CookOff, centre, VOL_KILL);
        let (shooter, entry, dir) = self.fire_origin.unwrap_or((centre, centre, Vec3::Z));
        self.last_fatal = Some(FatalHit {
            shooter_pos: shooter,
            shell_dir: dir,
            entry_point: entry,
            spall_dir: dir.normalize_or_zero(),
            hit_body: true,
            hit_modules: vec![ModuleKind::Fuel, ModuleKind::AmmoRack],
            cause: DeathCause::FuelFire,
        });
        println!("[kessel] FIRE reached the ammunition — cook-off, Tiger burned out.");
    }

    /// A player round PENETRATED and killed enemy `idx` — the payoff. Marks the enemy dead, sets up
    /// its burning wreck, and rolls the catastrophic outcome:
    ///   ~35% CATASTROPHIC (ammo rack) → the turret is LAUNCHED ballistically off the hull with a
    ///        big instant fireball + debris at the ring, and a heavy turret-pop blast sound.
    ///   else NORMAL → hull + turret stay and BURN; schedule a cook-off (`COOKOFF_MIN..MAX` s), which
    ///        ~55% of the time actually fires a delayed secondary (otherwise it just burns out).
    /// Deterministic via `hash01` (no rand), matching the rest of the file.
    fn destroy_enemy(&mut self, idx: usize) {
        // Snapshot the (Copy) death pose before taking any &mut borrow of self.
        let (epos, eheading, eyaw, epitch, eparts, ekind, alive) = {
            let e = match self.enemies.get(idx) {
                Some(e) => e,
                None => return,
            };
            (
                e.pos,
                e.heading,
                e.turret_yaw,
                e.gun_pitch,
                e.parts,
                e.kind,
                e.alive,
            )
        };
        if !alive {
            return;
        }
        {
            let e = &mut self.enemies[idx];
            e.alive = false;
            e.speed = 0.0;
        }
        if self.enemy_active.get(idx).copied().unwrap_or(false) {
            if let Some(runtime) = self.campaign.as_mut() {
                runtime.total_kills = runtime.total_kills.saturating_add(1);
                runtime.battle_pressure = (runtime.battle_pressure + 18.0).min(100.0);
            }
        }
        // The knocked-out hull becomes hard cover: register a tank-hull-sized LOS occluder at the
        // wreck so the rest of the swarm can no longer see the player straight through it (and a
        // Tiger can now deliberately break contact behind a burning T-34). ~6.8 m footprint, ~2.6 m
        // tall — even a turret-popped hull still breaks a tank-centre sightline.
        {
            let half = Vec3::new(3.4, 0.0, 3.4);
            let top = Vec3::new(0.0, 2.6, 0.0);
            self.occluders
                .push(stealth::Occluder::from_aabb(epos - half, epos + half + top));
        }
        let center = epos + Vec3::new(0.0, eparts.center_h, 0.0);
        let ground = epos.y;
        let ring = epos + Quat::from_rotation_y(eheading) * eparts.turret_pivot; // world turret ring

        self.kill_tick = self.kill_tick.wrapping_add(1);
        let seed = self
            .kill_tick
            .wrapping_mul(2246822519)
            .wrapping_add(idx as u32 * 7919);
        // A CASEMATE SU-100 has no turret to blow off — it can only BURN (and maybe cook off), never
        // turret-pop. A turreted T-34 rolls the catastrophic ammo-rack detonation as before.
        let catastrophic = !ekind.casemate() && hash01(seed) < CATASTROPHIC_CHANCE;

        {
            let w = &mut self.wrecks[idx];
            *w = Wreck {
                kind: WreckKind::Burn,
                burning: true,
                ..Wreck::default()
            };
            if catastrophic {
                w.kind = WreckKind::TurretPop;
                let vy = TURRET_POP_V0_MIN
                    + (TURRET_POP_V0_MAX - TURRET_POP_V0_MIN) * hash01(seed.wrapping_add(11));
                let lat = 2.0 + 4.0 * hash01(seed.wrapping_add(23)); // small random lateral drift
                let la = hash01(seed.wrapping_add(31)) * std::f32::consts::TAU;
                w.t_pos = ring;
                w.t_vel = Vec3::new(la.cos() * lat, vy, la.sin() * lat);
                let axis = Vec3::new(
                    hash01(seed.wrapping_add(41)) - 0.5,
                    hash01(seed.wrapping_add(43)) - 0.5,
                    hash01(seed.wrapping_add(47)) - 0.5,
                );
                w.t_spin_axis = axis.normalize_or_zero();
                if w.t_spin_axis.length_squared() < 0.01 {
                    w.t_spin_axis = Vec3::X;
                }
                w.t_spin_rate = 6.0 + 8.0 * hash01(seed.wrapping_add(53)); // rad/s tumble
                w.t_base_rot = Quat::from_rotation_y(eheading + eyaw);
                w.t_gun_local =
                    Mat4::from_translation(eparts.gun_trunnion) * Mat4::from_rotation_x(-epitch);
                w.t_rest_y = ground + 0.4;
                // A turret-popped tank has already vented its ammunition — no secondary cook-off.
                w.cookoff_t = None;
            } else {
                // schedule a possible cook-off
                let delay =
                    COOKOFF_MIN + (COOKOFF_MAX - COOKOFF_MIN) * hash01(seed.wrapping_add(101));
                let will_cook = hash01(seed.wrapping_add(103)) < COOKOFF_CHANCE;
                w.cookoff_t = if will_cook { Some(delay) } else { None };
            }
        }

        if catastrophic {
            // Big INSTANT fireball at the turret ring + a spray of glowing debris. The BLAST is seen
            // now; its heavy boom + tearing metal arrive delayed by range to the wreck.
            self.spawn_blast(ring, 9.0, 26.0, 0.85);
            self.spawn_debris(ring, 11, 15.0, seed);
            self.schedule_sound_from(crate::audio::Sfx::TurretPop, ring, VOL_KILL);
            self.schedule_sound_from(crate::audio::Sfx::TurretRip, ring, VOL_KILL);
            println!(
                "[kessel] {} #{idx} CATASTROPHIC KILL — ammo rack, turret blown off!",
                ekind.name()
            );
        } else {
            let cooks = self.wrecks[idx].cookoff_t;
            self.spawn_blast(center, 6.5, 20.0, 0.7);
            self.spawn_debris(center, 8, 11.0, seed);
            self.schedule_sound_from(crate::audio::Sfx::KillExplosion, center, VOL_KILL);
            match cooks {
                Some(d) => println!(
                    "[kessel] {} #{idx} BURNING wreck — cook-off in {d:.1}s.",
                    ekind.name()
                ),
                None => println!("[kessel] {} #{idx} BURNING wreck.", ekind.name()),
            }
        }
    }

    /// Advance every wreck: fire clock, the tumbling turret-pop ballistics, and the delayed cook-off
    /// countdown. When a burning wreck's timer elapses it fires a SECONDARY EXPLOSION — a big delayed
    /// fireball + heavy boom + a brief spike in its own fire ("it sat there burning, then WHUMP").
    fn update_wrecks(&mut self, dt: f32) {
        for i in 0..self.wrecks.len() {
            if self.wrecks[i].kind == WreckKind::None {
                continue;
            }
            {
                let w = &mut self.wrecks[i];
                w.fire_t += dt;
                if w.cook_flash > 0.0 {
                    w.cook_flash = (w.cook_flash - dt * 1.2).max(0.0);
                }
                if w.kind == WreckKind::TurretPop && !w.landed {
                    w.t_vel.y -= 9.8 * dt;
                    w.t_pos += w.t_vel * dt;
                    w.t_ang += w.t_spin_rate * dt;
                    if w.t_pos.y <= w.t_rest_y {
                        w.t_pos.y = w.t_rest_y;
                        w.t_vel = Vec3::ZERO;
                        w.landed = true;
                    }
                }
            }
            // cook-off trigger
            let fire = {
                let w = &mut self.wrecks[i];
                match w.cookoff_t.as_mut() {
                    Some(ct) => {
                        *ct -= dt;
                        if *ct <= 0.0 {
                            w.cookoff_t = None;
                            w.cooked = true;
                            w.cook_flash = 1.0;
                            true
                        } else {
                            false
                        }
                    }
                    None => false,
                }
            };
            if fire {
                let center = self.enemies[i].center();
                self.kill_tick = self.kill_tick.wrapping_add(1);
                let seed = self
                    .kill_tick
                    .wrapping_mul(668265263)
                    .wrapping_add(i as u32 * 104729);
                self.spawn_blast(center + Vec3::new(0.0, 0.5, 0.0), 8.0, 24.0, 0.8);
                self.spawn_debris(center, 10, 14.0, seed);
                self.schedule_sound_from(crate::audio::Sfx::CookOff, center, VOL_KILL);
                println!(
                    "[kessel] {} #{i} COOK-OFF — secondary detonation.",
                    self.enemies[i].kind.name()
                );
            }
        }
    }

    /// Claim the next pooled fireball slot (round-robin) and arm it.
    fn spawn_blast(&mut self, pos: Vec3, scale: f32, hot: f32, dur: f32) {
        let k = self.next_blast;
        self.next_blast = (self.next_blast + 1) % N_BLAST;
        self.blasts[k] = Blast {
            t: dur,
            dur,
            pos,
            scale,
            hot,
        };
        // The expanding pressure front lifts a low dust/snow skirt. This outlives the white core and
        // makes the detonation visibly touch terrain instead of hovering as an emissive primitive.
        self.spawn_dust(pos - Vec3::Y * 0.25, (scale / 9.0).clamp(0.45, 1.25));
    }

    /// Claim the next pooled snow/dust puff slot (round-robin) and arm it at a near-miss impact.
    /// `close` in [0,1] (1 = right on the hull, 0 = at the near-miss radius) scales the burst bigger +
    /// longer-lived the closer the round landed. Deterministic — the size is a pure function of `close`.
    fn spawn_snow_puff(&mut self, pos: Vec3, close: f32) {
        let k = self.next_snow;
        self.next_snow = (self.next_snow + 1) % N_SNOW;
        let dur = 0.40 + 0.18 * close;
        self.snow_puffs[k] = SnowPuff {
            t: dur,
            dur,
            pos: pos + Vec3::new(0.0, 0.3, 0.0), // lift off the snow so it reads as a kicked-up cloud
            scale: 1.1 + 1.9 * close,
        };
        println!(
            "[kessel] NEAR-MISS — snow puff at {:.1} m (close {:.2})",
            pos.distance(self.player.center()),
            close
        );
    }

    /// Fling `count` glowing debris chunks from `pos`, deterministically (seeded by `seed`).
    fn spawn_debris(&mut self, pos: Vec3, count: usize, speed: f32, seed: u32) {
        for j in 0..count {
            let k = self.next_spark;
            self.next_spark = (self.next_spark + 1) % N_DEBRIS;
            let s = seed.wrapping_add((j as u32).wrapping_mul(2246822519));
            let a = hash01(s) * std::f32::consts::TAU;
            let up = 0.5 + 1.1 * hash01(s.wrapping_add(7)); // mostly upward
            let sp = speed * (0.4 + 0.9 * hash01(s.wrapping_add(13)));
            let vel = Vec3::new(a.cos() * sp * 0.6, up * sp, a.sin() * sp * 0.6);
            let life = 0.7 + 0.6 * hash01(s.wrapping_add(19));
            self.sparks[k] = Spark {
                t: life,
                dur: life,
                pos,
                vel,
                size: 0.22 + 0.4 * hash01(s.wrapping_add(23)),
            };
        }
    }

    /// Claim the next pooled snow/dust puff slot (round-robin) and arm it as a grey MASONRY-DUST
    /// cloud — reuses the near-miss snow-puff pool + its own-colour white-grey emissive, so a felled
    /// wall throws up a believable dust cloud with no extra mesh. `scale` in [0,1]+ sizes the burst.
    fn spawn_dust(&mut self, pos: Vec3, scale: f32) {
        let k = self.next_snow;
        self.next_snow = (self.next_snow + 1) % N_SNOW;
        let dur = 0.5 + 0.5 * scale;
        self.snow_puffs[k] = SnowPuff {
            t: dur,
            dur,
            pos: pos + Vec3::new(0.0, 0.3, 0.0),
            scale: 1.4 + 2.4 * scale,
        };
    }

    /// Ray vs the destructible-structure registry: the nearest STILL-STANDING piece whose world AABB
    /// the ray enters within `max_t`, as (entry distance, index). Felled pieces (now rubble) no longer
    /// block. O(destructibles) — a few dozen cheap slab tests, only on the rare frames a gun fires.
    fn ray_destructible(&self, o: Vec3, d: Vec3, max_t: f32) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        for (i, ds) in self.destructibles.iter().enumerate() {
            if !ds.alive {
                continue;
            }
            if let Some(t) = ray_aabb(o, d, ds.min, ds.max, max_t) {
                if best.map_or(true, |(bt, _)| t < bt) {
                    best = Some((t, i));
                }
            }
        }
        best
    }

    /// TRACK CRUSH (docs/DESTRUCTION.md §4): resolve the player's bow against every standing
    /// destructible. Until now the Tiger clipped straight through walls — this is the first
    /// tank-vs-structure contact in the engine, and it's a crush gate rather than a plain collider:
    /// tier 1 (timber) BURSTS on contact at speed; tiers 2–3 are the GRIND — speed clamps against
    /// the piece while crush damage flows into the same HP pool the gun chips, until it collapses
    /// through the same `collapse_structure` path and the tank surges over the fresh rubble; tier 4
    /// (bunker) is a hard stop. Grind FX/audio are throttled (`crush_fx_t`), the contact crunch +
    /// telemetry fire once per contact (`crush_was`), and `crush_rumble` feeds the camera shudder.
    /// Player-only for now — enemy T-34s still treat walls as ghosts (AI crush is a doc §7 hook).
    fn resolve_tank_crush(&mut self, dt: f32, r: &mut Renderer) {
        let sgn = if self.player.speed >= 0.0 { 1.0 } else { -1.0 };
        let dir = self.player.forward() * sgn;
        let nose = self.player.pos + dir * CRUSH_NOSE_LEN;
        let mut contact = false;
        for i in 0..self.destructibles.len() {
            let d = self.destructibles[i];
            if !d.alive {
                continue;
            }
            // A piece wholly above the hull deck can't meet the bow (arches, high sills).
            if d.min.y > self.player.pos.y + 2.2 {
                continue;
            }
            let Some((push, overlap)) = nose_contact(nose, dir, d.min, d.max) else {
                continue;
            };
            let spec = d.class.crush();
            let speed = self.player.speed.abs();
            // Tier 1: light structure BURSTS at speed — no push-back, drive straight through it.
            if spec.tier == 1 && speed > TIMBER_BURST_SPEED {
                self.collapse_structure(i, r);
                self.player.speed *= 0.65;
                self.crush_rumble = 1.2;
                continue;
            }
            contact = true;
            // Pressed against the piece: separate the bow, re-ground, and cap the speed. The tank
            // stays shoved to the face each frame — that sustained press IS the grind.
            self.player.pos += push * overlap;
            self.player.pos.y = self.hf.height_at(self.player.pos.x, self.player.pos.z);
            if spec.tier >= 4 {
                self.player.speed = 0.0; // bunker concrete: dead stop, chips only (and no HP chip)
                continue;
            }
            self.player.speed = self.player.speed.clamp(-spec.vmax, spec.vmax);
            // Crush damage flows only while actually pushing, not while parked touching a wall.
            if speed > 0.4 {
                let c = Vec3::new(
                    nose.x.clamp(d.min.x, d.max.x),
                    d.min.y + 0.8,
                    nose.z.clamp(d.min.z, d.max.z),
                );
                if self.destructibles[i].hit(spec.dps * dt) {
                    self.collapse_structure(i, r);
                    self.crush_rumble = 1.5; // the give — the surge over fresh rubble
                    continue;
                }
                self.crush_rumble = self.crush_rumble.max(0.55);
                // Throttled plough FX: masonry chips + dust churning at the contact point.
                if self.crush_fx_t <= 0.0 {
                    self.crush_fx_t = 0.22;
                    let seed =
                        d.seed ^ (self.destructibles[i].hp as u32).wrapping_mul(2_654_435_761);
                    self.spawn_debris(c, 2, 3.5, seed);
                    self.spawn_dust(c, 0.45);
                }
                if !self.crush_was {
                    // Contact crunch + one line of telemetry, once per grind.
                    self.schedule_sound_from(crate::audio::Sfx::ImpactGround, c, VOL_IMPACT_GROUND);
                    println!(
                        "[kessel] KETTEN-CRUSH — grinding piece #{i} ({:?}, {:.0}% integrity)",
                        d.class,
                        100.0 * self.destructibles[i].hp / self.destructibles[i].max_hp.max(1.0)
                    );
                }
            }
        }
        // Authored wreck columns and barricades are static battlefield mass, not holograms. They
        // hard-stop the bow; small story clutter was intentionally excluded from `story_cover` so
        // cans, grass, boards and luggage remain crushable visual dressing.
        for &(min, max) in &self.story_cover {
            if min.y > self.player.pos.y + 2.2 {
                continue;
            }
            let Some((push, overlap)) = nose_contact(nose, dir, min, max) else {
                continue;
            };
            contact = true;
            self.player.pos += push * overlap;
            self.player.pos.y = self.hf.height_at(self.player.pos.x, self.player.pos.z);
            self.player.speed = 0.0;
            self.crush_rumble = self.crush_rumble.max(0.45);
        }
        // Tanks are battlefield mass too. Resolve a conservative XZ footprint against every active
        // enemy (living or wrecked): no more driving one Tiger through a T-34, and close-quarter
        // shoves become noisy, speed-killing emergencies instead of hologram overlap.
        const TANK_SEPARATION: f32 = 4.35;
        for i in 0..self.enemies.len() {
            if !self.enemy_active.get(i).copied().unwrap_or(false) {
                continue;
            }
            let delta = Vec3::new(
                self.enemies[i].pos.x - self.player.pos.x,
                0.0,
                self.enemies[i].pos.z - self.player.pos.z,
            );
            let dist = delta.length();
            if dist >= TANK_SEPARATION {
                continue;
            }
            let n = if dist > 1e-4 {
                delta / dist
            } else {
                self.player.forward()
            };
            let overlap = TANK_SEPARATION - dist;
            let enemy_moves = self.enemies[i].alive;
            let player_share = if enemy_moves { 0.58 } else { 1.0 };
            self.player.pos -= n * overlap * player_share;
            if enemy_moves {
                self.enemies[i].pos += n * overlap * (1.0 - player_share);
                self.enemies[i].pos.y = self
                    .hf
                    .height_at(self.enemies[i].pos.x, self.enemies[i].pos.z);
                self.enemies[i].speed *= 0.22;
            }
            self.player.pos.y = self.hf.height_at(self.player.pos.x, self.player.pos.z);
            self.player.speed *= 0.18;
            self.crush_rumble = self.crush_rumble.max(0.70);
            contact = true;
            if !self.crush_was {
                let hit = (self.player.center() + self.enemies[i].center()) * 0.5;
                self.schedule_sound_from(crate::audio::Sfx::ImpactTank, hit, VOL_IMPACT_TANK);
                println!("[kessel] PANZERKONTAKT — steel-on-steel with target #{i}");
            }
        }
        // Trees under the bow (docs/DESTRUCTION.md §5): firs snap and are ploughed under; big
        // pines take a sustained tier-3 shove from the Tiger (crush power 3) and give way.
        let mut tree_events = Vec::new();
        let tc = self.trees.crush(
            nose,
            dir,
            self.player.speed.abs(),
            3,
            CRUSH_NOSE_R,
            dt,
            &mut tree_events,
        );
        if let Some(cap) = tc.speed_cap {
            self.player.speed = self.player.speed.clamp(-cap, cap);
            contact = true;
        }
        self.crush_rumble = self.crush_rumble.max(tc.rumble);
        self.route_tree_events(tree_events);

        self.crush_fx_t = (self.crush_fx_t - dt).max(0.0);
        self.crush_was = contact;
    }

    /// Advance the sprung carry (the loader's laden hands): the mass chases the hand anchor with
    /// lag and overshoot — the weight — and the nose chases the look direction slowly, drooping.
    /// Uses LAST frame's camera (this runs pre-camera in update), which is exactly the hand.
    fn update_carry(&mut self, r: &Renderer, dt: f32, k: f32, d: f32) {
        // Low and forward — hefted at the waist, not pressed to the eye (close geometry through
        // the near plane reads as a wall of brass).
        let hand = r.camera.pos + r.camera.forward() * 0.68 - Vec3::new(0.0, 0.26, 0.0);
        let acc = (hand - self.carry_pos) * k - self.carry_vel * d;
        self.carry_vel += acc * dt;
        self.carry_pos += self.carry_vel * dt;
        // Nose: cradled DIAGONALLY across the body (nose off to the left, drooping) — dead
        // nose-forward reads as a flat red disc of case-head filling the view. Chases slowly.
        let fwd = r.camera.forward();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        let target = (fwd * 0.7 - right * 0.55 + Vec3::new(0.0, -0.30, 0.0)).normalize_or_zero();
        let rate = 1.0 - (-5.0 * dt).exp();
        self.carry_dir = (self.carry_dir + (target - self.carry_dir) * rate).normalize_or_zero();
    }

    /// Spawn a pooled casing at an explicit position/velocity (the hand TOSS — vs `eject_casing`'s
    /// breech-mouth kick for the auto/KO'd path). Returns the pool index.
    fn spawn_casing_from(&mut self, pos: Vec3, vel: Vec3) -> usize {
        let k = self.next_casing % N_CASING;
        self.next_casing = self.next_casing.wrapping_add(1);
        self.kill_tick = self.kill_tick.wrapping_add(1);
        let seed = self.kill_tick.wrapping_mul(2654435761);
        let s = |n: u32| hash01(seed.wrapping_add(n));
        let floor_y = self
            .player
            .turret_model()
            .transform_point3(Vec3::new(0.0, 1.45, 0.0))
            .y;
        let c = &mut self.casings[k];
        c.active = true;
        c.carried = false;
        c.pos = pos;
        c.vel = vel;
        c.spin_axis = Vec3::new(s(4) - 0.5, s(5) - 0.5, s(6) - 0.5).normalize_or_zero();
        c.spin = 3.0 + 6.0 * s(7);
        c.ang = 0.0;
        c.t = 30.0;
        c.bounced = false;
        c.floor_y = floor_y;
        k
    }

    /// Route tree events (snap cracks, ground thuds) into the pooled FX and the ranged-audio
    /// scheduler. The trees subsystem itself stays deaf and mute — this is its voice.
    fn route_tree_events(&mut self, events: Vec<crate::trees::TreeEvent>) {
        for ev in events {
            match ev {
                crate::trees::TreeEvent::Snap { pos } => {
                    // The crack + a burst of needles/snow off the trunk base.
                    self.spawn_dust(pos + Vec3::new(0.0, 0.6, 0.0), 0.45);
                    self.spawn_debris(
                        pos + Vec3::new(0.0, 1.0, 0.0),
                        3,
                        4.0,
                        0x7EE_C4AC ^ (pos.x.abs() * 7.0) as u32 ^ (pos.z.abs() * 13.0) as u32,
                    );
                    self.schedule_sound_from(crate::audio::Sfx::ImpactGround, pos, 0.75);
                }
                crate::trees::TreeEvent::Thud { pos } => {
                    // The trunk slams the drift: a long dust wash + the heavy ground thud.
                    self.spawn_dust(pos, 0.9);
                    self.spawn_dust(pos + Vec3::new(0.0, 0.5, 0.0), 0.5);
                    self.schedule_sound_from(
                        crate::audio::Sfx::ImpactGround,
                        pos,
                        VOL_IMPACT_GROUND,
                    );
                }
            }
        }
    }

    /// A main-gun round struck destructible structure `idx`: chip its HP; on depletion, COLLAPSE it.
    /// A non-fatal chip still kicks up a little dust + a few masonry chips so a hit on a still-standing
    /// wall reads. No-op if the index is stale or the piece is already rubble.
    fn damage_structure(&mut self, idx: usize, dmg: f32, r: &mut Renderer) {
        if idx >= self.destructibles.len() || !self.destructibles[idx].alive {
            return;
        }
        let dropped = self.destructibles[idx].hit(dmg);
        let c = self.destructibles[idx].center();
        if self.act.id == 3 && self.campaign.is_some() {
            self.finale_town_integrity =
                (self.finale_town_integrity - if dropped { 5.0 } else { 0.65 }).max(0.0);
            if self.finale_town_integrity <= 22.0 {
                self.civilian_column_alive = false;
            }
        }
        if dropped {
            self.collapse_structure(idx, r);
        } else {
            // Still standing — a puff of masonry dust + a few chips at the strike point.
            let ds = &self.destructibles[idx];
            let frac = ds.hp / ds.max_hp.max(1.0);
            println!(
                "[kessel] STRUCTURE HIT — piece #{idx} staggered, {:.0}% integrity left",
                frac * 100.0
            );
            let seed = ds.seed ^ (ds.hp as u32).wrapping_mul(2_654_435_761);
            self.spawn_debris(c, 3, 5.5, seed);
            self.spawn_dust(c, 0.5);
        }
    }

    /// Collapse destructible `idx`: hide the intact mesh, reveal its pre-allocated rubble heap,
    /// downgrade its LOS occluder to knee height (a felled wall stops concealing a tank), fling a
    /// debris burst + masonry dust from the pooled FX, and schedule a heavy collapse boom. Everything
    /// is deterministic (the piece's own seed). No-op if already rubble.
    fn collapse_structure(&mut self, idx: usize, r: &mut Renderer) {
        // Snapshot the (Copy) record so the rest of `self` is free to mutate below.
        let d = self.destructibles[idx];
        self.destructibles[idx].alive = false;
        self.destructibles[idx].hp = 0.0;
        // Intact → gone, rubble → shown. If the wall carries a chunk grid, everything still
        // standing in it tumbles outward too — the crumble, not just the swap.
        r.set_visible(d.intact, false);
        r.set_visible(d.rubble, true);
        self.chunks.collapse(idx, r);
        // Downgrade the LOS occluder so the stealth model reflects the new (open) cover state.
        if d.occ < self.occluders.len() {
            self.occluders[d.occ] = collapsed_occluder(self.occluders[d.occ]);
        }
        let c = d.center();
        // Debris burst (flung glowing chunks) + a big masonry dust cloud, both decaying from the pool.
        self.spawn_debris(c + Vec3::new(0.0, 0.4, 0.0), 10, 9.0, d.seed);
        self.spawn_dust(c, 1.0);
        self.spawn_dust(c + Vec3::new(0.0, 1.2, 0.0), 0.7);
        // A heavy collapse boom — downrange, so it's delayed + range-attenuated like every impact.
        self.schedule_sound_from(crate::audio::Sfx::KillExplosion, c, VOL_KILL);
        println!("[kessel] STRUCTURE COLLAPSE — piece #{idx} felled at ({:.1},{:.1}) — occluder downgraded",
            c.x, c.z);
    }

    /// Assign the volumetric smoke pool each frame: player fire, muzzle blast, active detonations,
    /// then every burning wreck in distance order. Twelve slots let a full six-tank engagement keep
    /// its smoke history instead of silently dropping all but one wreck.
    fn drive_smoke(&mut self, r: &mut Renderer) {
        const N_PUFFS: usize = crate::renderer::MAX_PUFFS;
        const ROLL: f32 = 0.75; // < the ~0.9s envelope, so the column re-emits before it dies
        r.clear_smoke(); // zero every slot; the active emitters below re-arm the ones they need
        let mut slot = 0usize;
        // The player's OWN interior fire smokes heavily once established — takes the first slot so the
        // acrid smoke reads in the player's view as the tank burns.
        if self.fire > 0.30 && slot < N_PUFFS {
            let base = self.player.center() + Vec3::new(0.0, 0.6, 0.0);
            let age = (self.fire_t * 0.9) % ROLL;
            // The acrid interior smoke leans downwind with the local (sheltered-at-the-edge) wind.
            r.set_smoke_slot(slot, base, smoke_drift(wind_at(base)), age, 1.0);
            slot += 1;
        }
        // muzzle blast plume takes the next slot while it's live
        if self.fx.smoke_t > 0.0 && slot < N_PUFFS {
            let (mpos, mdir) = self.player.muzzle();
            let age = self.fx.smoke_dur - self.fx.smoke_t;
            // Puffs forward off the barrel, then the local wind takes it downwind.
            let w = wind_at(mpos);
            let dir = Vec3::new(mdir.x * 0.4 + w.x * 0.14, 0.85, mdir.z * 0.4 + w.y * 0.14);
            r.set_smoke_slot(slot, mpos, dir, age, 1.0);
            slot += 1;
        }
        // A detonation throws a dirty hot cloud behind the white core. It starts compact and rides
        // the same wind field as fire and weather, making the blast feel anchored in the scene.
        for (i, b) in self.blasts.iter().enumerate() {
            if slot >= N_PUFFS {
                break;
            }
            if b.t <= 0.0 {
                continue;
            }
            let age = (b.dur - b.t + i as f32 * 0.07).min(0.92);
            r.set_smoke_slot(slot, b.pos, smoke_drift(wind_at(b.pos)), age, 0.75);
            slot += 1;
        }
        for (i, ally) in self
            .allies
            .iter()
            .enumerate()
            .filter(|(_, ally)| !ally.tank.alive)
        {
            if slot >= N_PUFFS {
                break;
            }
            let base = ally.tank.center();
            let age = (self.fire_t + i as f32 * 0.37) % ROLL;
            r.set_smoke_slot(slot, base, smoke_drift(wind_at(base)), age, 1.0);
            slot += 1;
        }
        // Streamed waves may be gone, but their burning hulls are not. Preserve a few of the
        // nearest archived columns so a cleared route keeps its smoke history instead of switching
        // abruptly to a clean glowing model when the next contact spawns.
        let listener = self.threat_center();
        let mut archived_burning: Vec<(f32, usize)> = self
            .campaign_hulks
            .iter()
            .enumerate()
            .filter(|(_, hulk)| hulk.burning)
            .map(|(i, hulk)| {
                let pos = hulk.models[0].w_axis.truncate();
                (pos.distance_squared(listener), i)
            })
            .collect();
        archived_burning.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, i) in archived_burning {
            if slot >= N_PUFFS {
                break;
            }
            let base = self.campaign_hulks[i].models[0].w_axis.truncate() + Vec3::Y * 1.6;
            let age = (self.fire_t + i as f32 * 0.41) % ROLL;
            r.set_smoke_slot(slot, base, smoke_drift(wind_at(base)), age, 0.9);
            slot += 1;
        }
        // nearest burning wrecks fill the rest — a rolling, rising smoke column each
        let p = self.player.center();
        let mut burning: Vec<(f32, usize)> = Vec::new();
        for i in 0..self.wrecks.len() {
            if self.wrecks[i].burning {
                burning.push((self.enemies[i].pos.distance_squared(p), i));
            }
        }
        burning.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, i) in burning {
            if slot >= N_PUFFS {
                break;
            }
            let base = self.enemies[i].pos + Vec3::new(0.0, 1.6, 0.0);
            // phase-offset per wreck so their columns don't all re-emit on the same frame
            let phase = (i as f32) * 0.31;
            let age = (self.wrecks[i].fire_t + phase) % ROLL;
            // A wreck out on the open steppe lays its smoke column right over; one in the sheltered
            // streets rises far straighter (zoned wind via `wind_at`).
            r.set_smoke_slot(slot, base, smoke_drift(wind_at(base)), age, 1.0);
            slot += 1;
        }
        let _ = slot; // remaining slots were already zeroed by clear_smoke above
    }

    fn update_fx(&mut self, dt: f32, r: &mut Renderer) {
        // muzzle flash — bigger + hotter (white-hot core: emissive well above the shader's 6.0 knee)
        if self.fx.muzzle_t > 0.0 {
            self.fx.muzzle_t -= dt;
            let (mpos, _) = self.player.muzzle();
            let s = 1.2 + 8.0 * self.fx.muzzle_t; // fades down
            r.set_visible(self.slots.muzzle, true);
            r.set_emissive(self.slots.muzzle, 13.0);
            r.set_model(
                self.slots.muzzle,
                Mat4::from_scale_rotation_translation(Vec3::splat(s), Quat::IDENTITY, mpos),
            );
        } else {
            r.set_visible(self.slots.muzzle, false);
        }

        // tracer — flies the PIECEWISE pen-ray route: it visibly KINKS and re-heads at each wall
        // it punches through (the player watches the round transit — teaches the mechanic, no UI).
        if self.fx.tracer_active {
            self.fx.tracer_t += dt;
            let frac = (self.fx.tracer_t / self.fx.tracer_dur).clamp(0.0, 1.0);
            let (cur, seg_dir) = if self.fx.tracer_route.len() >= 2 {
                let total: f32 = self
                    .fx
                    .tracer_route
                    .windows(2)
                    .map(|w| w[0].distance(w[1]))
                    .sum::<f32>()
                    .max(1e-3);
                let mut remaining = frac * total;
                let mut cur = *self.fx.tracer_route.last().unwrap();
                let mut dir = self.fx.tracer_dir;
                for w in self.fx.tracer_route.windows(2) {
                    let len = w[0].distance(w[1]);
                    if remaining <= len {
                        dir = (w[1] - w[0]) / len.max(1e-6);
                        cur = w[0] + dir * remaining;
                        break;
                    }
                    remaining -= len;
                }
                (cur, dir)
            } else {
                (
                    self.fx.tracer_from.lerp(self.fx.tracer_to, frac),
                    self.fx.tracer_dir,
                )
            };
            let rot = Quat::from_rotation_arc(Vec3::Z, seg_dir);
            r.set_visible(self.slots.tracer, true);
            r.set_emissive(self.slots.tracer, 17.0); // blows to a white-hot core through bloom
                                                     // Laser-like: thin + long, so the round reads as a bright streak snapping downrange.
            r.set_model(
                self.slots.tracer,
                Mat4::from_scale_rotation_translation(Vec3::new(0.5, 0.5, 7.5), rot, cur),
            );
            // Apply en-route pen-ray effects THE MOMENT the round passes them: the wall puffs as
            // it's punched, the trunk snaps as the round clips it — not seconds later at landing.
            let traveled = frac
                * self
                    .fx
                    .tracer_route
                    .windows(2)
                    .map(|w| w[0].distance(w[1]))
                    .sum::<f32>()
                    .max(1e-3);
            let due_walls: Vec<(usize, f32)> = {
                let v = &mut self.fx.pending_transits;
                let due = v.iter().filter(|(_, d)| *d <= traveled).copied().collect();
                v.retain(|(_, d)| *d > traveled);
                due
            };
            for (si, dist) in due_walls {
                // Open the visible hole exactly where the round punched through (§6), then chip.
                let (entry, dir) = route_at(&self.fx.tracer_route, dist);
                self.chunks.hole(si, entry, dir, r);
                self.damage_structure(si, STRUCT_AP_HOLE_DAMAGE, r);
            }
            let due_trees: Vec<(usize, Vec3)> = {
                let v = &mut self.fx.pending_trees;
                let due = v
                    .iter()
                    .filter(|(_, _, d)| *d <= traveled)
                    .map(|(i, dir, _)| (*i, *dir))
                    .collect();
                v.retain(|(_, _, d)| *d > traveled);
                due
            };
            for (ti, dir) in due_trees {
                let mut ev = Vec::new();
                self.trees.shot(ti, dir, &mut ev);
                self.route_tree_events(ev);
            }
            if frac >= 1.0 {
                self.land_player_shot(r);
            }
        }

        // Muzzle-blast timer + gunner-optic obscuration envelope. The actual volumetric puff slots
        // (the muzzle plume AND the rising columns off nearby burning wrecks) are assigned together
        // in `drive_smoke` at the end of this function — there are only MAX_PUFFS=3 slots, so they're
        // shared, not driven independently here.
        r.set_visible(self.slots.smoke, false); // legacy box stays hidden; the volume does the work
        if self.fx.smoke_t > 0.0 {
            self.fx.smoke_t -= dt;
            let age = self.fx.smoke_dur - self.fx.smoke_t; // seconds since fire
            r.optic_blast = if self.gunner_optic {
                let rise = smoothstep(0.0, 0.06, age);
                let fall = 1.0 - smoothstep(0.15, 0.7, age);
                rise * fall
            } else {
                0.0
            };
        } else {
            r.optic_blast = 0.0;
        }
        // A destroyed gunner sight leaves the optic permanently smeared (degraded aim) whenever the
        // player is down the gun — a persistent obscuration floor on top of any muzzle-blast veil.
        if self.gunner_optic && !self.module_ok(ModuleKind::GunnerSight) {
            r.optic_blast = r.optic_blast.max(0.45);
        }

        // impact — a killing blast (penetration), a sharp bright ricochet spark (bounce), or a
        // soft ground puff (miss). Each reads differently: the spark is small + very hot + brief.
        if self.fx.impact_t > 0.0 {
            self.fx.impact_t -= dt;
            let (base, emissive) = match self.fx.impact_kind {
                Outcome::Penetrate => (4.6, 17.0), // big, hot kill blast (a dedicated fireball stacks on top)
                Outcome::Bounce => (0.9, 14.0),    // small, searing spark
                _ => (1.4, 7.0),
            };
            let s = base * (0.4 + 4.0 * self.fx.impact_t);
            r.set_visible(self.slots.impact, true);
            r.set_emissive(self.slots.impact, emissive);
            r.set_model(
                self.slots.impact,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(s),
                    Quat::IDENTITY,
                    self.fx.impact_pos,
                ),
            );
        } else {
            r.set_visible(self.slots.impact, false);
        }

        // enemy return-fire FX pool — every slot animates its own tracer + impact so several T-34s
        // can be shooting at once. Redder tracer than the player's outgoing round.
        for k in 0..E_FX_POOL {
            let tr = self.slots.e_tracer[k];
            let im = self.slots.e_impact[k];
            // tracer
            if self.e_shots[k].tracer_active {
                self.e_shots[k].tracer_t += dt;
                let frac = (self.e_shots[k].tracer_t / self.e_shots[k].tracer_dur).clamp(0.0, 1.0);
                let cur = self.e_shots[k]
                    .tracer_from
                    .lerp(self.e_shots[k].tracer_to, frac);
                let rot = Quat::from_rotation_arc(Vec3::Z, self.e_shots[k].tracer_dir);
                r.set_visible(tr, true);
                r.set_emissive(tr, 14.0); // incoming laser-streak (redder mesh tint), also blooms hot
                r.set_model(
                    tr,
                    Mat4::from_scale_rotation_translation(Vec3::new(0.45, 0.45, 6.5), rot, cur),
                );
                if frac >= 1.0 {
                    self.e_shots[k].tracer_active = false;
                    r.set_visible(tr, false);
                    let outcome = self.e_shots[k].outcome;
                    let hit_facing = self.e_shots[k].arc;
                    self.e_shots[k].impact_pos = self.e_shots[k].tracer_to;
                    self.e_shots[k].impact_kind = outcome;
                    self.e_shots[k].impact_t = match outcome {
                        Outcome::Penetrate => 0.20,
                        Outcome::Bounce => 0.11,
                        _ => 0.16,
                    };
                    // Incoming rounds impact ON/near the Tiger — range ~0, so the queue plays these
                    // essentially instantly at full volume (you're right in it); a wide round that
                    // buries itself downrange is correctly delayed instead.
                    let ipos = self.e_shots[k].impact_pos;
                    // Enemy-vs-enemy landing: the struck FELLOW TANK takes the consequence — a
                    // penetration destroys it (full kill payoff), a bounce clangs. None of the
                    // player-interior feedback (hull voice / spall / concussion) applies.
                    if let Some(oi) = self.e_shots[k].hit_enemy.take() {
                        match outcome {
                            Outcome::Penetrate if self.enemies[oi].alive => {
                                println!(
                                    "[kessel] {} #{oi} destroyed by {}.",
                                    self.enemies[oi].kind.name(),
                                    if self.brawl {
                                        "the opposing team"
                                    } else {
                                        "FRIENDLY fire"
                                    }
                                );
                                self.destroy_enemy(oi);
                                self.schedule_sound_from(
                                    crate::audio::Sfx::PenetrationHit,
                                    ipos,
                                    VOL_PENETRATION,
                                );
                            }
                            Outcome::Bounce => {
                                let c = crate::audio::Sfx::ARMOR_CLANGS;
                                let pick = c[(hash01(self.armor_tick.wrapping_add(k as u32 * 11))
                                    * c.len() as f32)
                                    as usize
                                    % c.len()];
                                self.schedule_sound_from(pick, ipos, VOL_CLANG);
                            }
                            _ => {}
                        }
                        continue;
                    }
                    match outcome {
                        Outcome::Bounce | Outcome::Penetrate => {
                            self.stat_hits_on_player += 1;
                            if outcome == Outcome::Penetrate {
                                self.stat_pens_on_player += 1;
                            }
                            // THE parametric hull voice (kessel-sound-pillar): synthesize this exact
                            // hit — shell mass/velocity/type + the struck panel + bounce-vs-through —
                            // panned to the side it came from, LOUDER the closer it landed to the
                            // manned station. Structure-borne through the steel: played instantly,
                            // unmuffled (you hear your own hull through your bones, not the air).
                            // ABSITZEN: only while you're IN the tank — dismounted, a hit on the
                            // parked hull is an external impact heard at range, and it must not
                            // deafen/concuss a crewman who isn't aboard.
                            if matches!(self.mode, Mode::Foot) {
                                self.schedule_sound_from(
                                    crate::audio::Sfx::ImpactTank,
                                    ipos,
                                    VOL_IMPACT_TANK,
                                );
                            } else if let Some(ev) = self.e_shots[k].ev {
                                let shooter = self.e_shots[k].tracer_from;
                                let pc = self.player.center();
                                // Bearing relative to YOUR HEAD (the camera): a hit on the plate
                                // you're facing rings ahead; turn your head and the same panel's
                                // clang moves ears with you. (Was hull-relative — wrong the moment
                                // the player looks anywhere but dead ahead.)
                                let bearing = bearing_deg_from(r.camera.forward(), shooter - pc);
                                // Manned-station distance to the entry point, in the hull frame.
                                let seat = match self.station {
                                    Station::Driver => CrewRole::Driver.station(),
                                    Station::Radioman => CrewRole::Radioman.station(),
                                    Station::Gunner => CrewRole::Gunner.station(),
                                    Station::Loader => CrewRole::Loader.station(),
                                    Station::Commander => CrewRole::Commander.station(),
                                };
                                let entry_l =
                                    self.player.hull_model().inverse().transform_point3(ipos);
                                let station_dist = (entry_l - seat).length().max(0.15);
                                let facing = facing_from(hit_facing, self.e_shots[k].turret_hit);
                                let params = crate::acoustics::params_from_event(
                                    &ev,
                                    facing,
                                    bearing,
                                    station_dist,
                                    self.armor_tick.wrapping_add(k as u32 * 131),
                                );
                                let buf = crate::acoustics::synth_impact(params);
                                if let Some(a) = &self.audio {
                                    a.play_stereo(
                                        buf,
                                        if outcome == Outcome::Penetrate {
                                            VOL_PENETRATION
                                        } else {
                                            VOL_CLANG
                                        },
                                    );
                                }
                                // CONCUSSION: a penetration, or a heavy shell slamming the plate near
                                // you, deafens — the world ducks away under a tinnitus ring and
                                // recovers over seconds (acoustics::concussion_duck drives the mix).
                                let heavy_bounce =
                                    ev.shell.caliber_mm >= 100.0 && station_dist < 2.2;
                                if outcome == Outcome::Penetrate || heavy_bounce {
                                    let intensity = if outcome == Outcome::Penetrate {
                                        1.0
                                    } else {
                                        0.6
                                    };
                                    self.concussion_t = 0.0;
                                    self.concussion_dur = 3.0 + 5.0 * intensity;
                                    if let Some(a) = &self.audio {
                                        a.play_stereo(
                                            crate::acoustics::synth_tinnitus(
                                                intensity,
                                                self.armor_tick,
                                            ),
                                            0.9,
                                        );
                                    }
                                }
                            } else {
                                // No ballistic event (shouldn't happen on a hull hit) — legacy clang.
                                let c = crate::audio::Sfx::ARMOR_CLANGS;
                                let pick =
                                    c[(hash01(self.armor_tick.wrapping_add(k as u32 * 7 + 3))
                                        * c.len() as f32)
                                        as usize
                                        % c.len()];
                                self.schedule_sound_from(pick, ipos, VOL_CLANG);
                            }
                            if outcome == Outcome::Penetrate {
                                // The sampled penetration layer keeps its body under the synth voice.
                                self.schedule_sound_from(
                                    crate::audio::Sfx::PenetrationHit,
                                    ipos,
                                    VOL_PENETRATION,
                                );
                                // Hand the SHELL geometry (shooter muzzle → entry on the Tiger →
                                // travel direction) to the spall model for the right compartment.
                                let shooter = self.e_shots[k].tracer_from;
                                let dir = self.e_shots[k].tracer_dir;
                                self.resolve_player_penetration(
                                    hit_facing,
                                    shooter,
                                    ipos,
                                    dir,
                                    Some(self.e_shots[k].turret_hit),
                                );
                            }
                        }
                        Outcome::NearMiss => {
                            // The round snapped past the Tiger. Kick up a snow/dust PUFF on the ground
                            // where it passed CLOSEST (near_pt), scaled by how close it whipped by, and
                            // crack a near-miss report (louder the closer) — so incoming fire snaps
                            // around you. A round that passed wide of the puff radius just cracks (no
                            // puff), so far/wide rounds don't spam the effect.
                            let pc = self.player.center();
                            let np = self.e_shots[k].near_pt;
                            let horiz = Vec3::new(np.x - pc.x, 0.0, np.z - pc.z).length(); // beside-you distance
                            if let Some(close) = near_miss_puff_scale(horiz) {
                                let ground = Vec3::new(np.x, self.hf.height_at(np.x, np.z), np.z);
                                self.spawn_snow_puff(ground, close);
                                self.schedule_sound_from(
                                    crate::audio::Sfx::NearMiss,
                                    np,
                                    VOL_NEAR_MISS * (0.7 + 0.5 * close),
                                );
                            } else {
                                self.schedule_sound_from(
                                    crate::audio::Sfx::NearMiss,
                                    np,
                                    VOL_NEAR_MISS,
                                );
                            }
                        }
                        Outcome::Ground => {
                            self.schedule_sound_from(
                                crate::audio::Sfx::ImpactGround,
                                ipos,
                                VOL_IMPACT_GROUND,
                            );
                        }
                    }
                    // A stray enemy round that struck a destructible wall chips (or drops) it now.
                    if let Some(si) = self.e_shots[k].pending_struct.take() {
                        self.damage_structure(si, STRUCT_SHELL_DAMAGE, r);
                    }
                }
            }
            // impact — bright spark for a bounce, big blast for a penetration, puff otherwise.
            if self.e_shots[k].impact_t > 0.0 {
                self.e_shots[k].impact_t -= dt;
                let (base, emissive) = match self.e_shots[k].impact_kind {
                    Outcome::Penetrate => (2.6, 10.0),
                    Outcome::Bounce => (0.9, 14.0), // small, searing ricochet spark
                    _ => (1.2, 6.0),
                };
                let s = base * (0.4 + 4.0 * self.e_shots[k].impact_t);
                r.set_visible(im, true);
                r.set_emissive(im, emissive);
                r.set_model(
                    im,
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(s),
                        Quat::IDENTITY,
                        self.e_shots[k].impact_pos,
                    ),
                );
            } else {
                r.set_visible(im, false);
            }
        }

        // Big fireball blasts (kill / turret-pop / cook-off): expand outward while fading, hottest
        // early so they punch through the crush and bloom.
        for k in 0..N_BLAST {
            let slot = self.slots.blast[k];
            let b = &mut self.blasts[k];
            if b.t > 0.0 {
                b.t -= dt;
                let frac = (b.t / b.dur).clamp(0.0, 1.0); // 1 → 0 over its life
                let grow = 1.0 - frac; // 0 → 1
                let s = b.scale * (0.35 + 1.15 * grow); // expanding fireball
                let hot = b.hot * (0.25 + 0.75 * frac); // brightest at the flash
                let wobble = Vec3::new(
                    0.82 + 0.22 * (grow * 19.0 + k as f32).sin().abs(),
                    0.70 + 0.55 * grow,
                    0.82 + 0.22 * (grow * 17.0 + k as f32 * 1.7).cos().abs(),
                );
                let rot = Quat::from_rotation_y(grow * (1.4 + k as f32 * 0.3));
                r.set_visible(slot, true);
                r.set_emissive(slot, hot);
                r.set_model(
                    slot,
                    Mat4::from_scale_rotation_translation(wobble * s, rot, b.pos),
                );
            } else {
                r.set_visible(slot, false);
            }
        }

        // Spark/debris chunks flung from a blast, falling under gravity and cooling as they go.
        for k in 0..N_DEBRIS {
            let slot = self.slots.debris[k];
            let sp = &mut self.sparks[k];
            if sp.t > 0.0 {
                sp.t -= dt;
                sp.vel.y -= 16.0 * dt;
                sp.pos += sp.vel * dt;
                let frac = (sp.t / sp.dur).clamp(0.0, 1.0);
                r.set_visible(slot, true);
                r.set_emissive(slot, 3.0 + 8.0 * frac); // cools as it falls
                r.set_model(
                    slot,
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(sp.size * (0.5 + frac)),
                        Quat::IDENTITY,
                        sp.pos,
                    ),
                );
            } else {
                r.set_visible(slot, false);
            }
        }

        // Near-miss snow/dust puffs: a quick white-grey burst that EXPANDS + fades (via its own-colour
        // emissive) and drifts up a touch where an incoming round buried itself in the snow beside the
        // Tiger. Own-colour (not FX orange) so it reads as kicked-up snow, not fire.
        for k in 0..N_SNOW {
            let slot = self.slots.snow[k];
            let sp = &mut self.snow_puffs[k];
            if sp.t > 0.0 {
                sp.t -= dt;
                let frac = (sp.t / sp.dur).clamp(0.0, 1.0); // 1 → 0 over its life
                let grow = 1.0 - frac; // 0 → 1
                let s = sp.scale * (0.35 + 1.15 * grow); // expanding cloud
                r.set_visible(slot, true);
                r.set_emissive(slot, 3.4 * frac); // bright snow-white, fading out
                r.set_model(
                    slot,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(s, s * 0.8, s),
                        Quat::IDENTITY,
                        sp.pos + Vec3::new(0.0, grow * 0.7, 0.0),
                    ),
                );
            } else {
                r.set_visible(slot, false);
            }
        }

        // Coax / hull MG34 FX: muzzle flicker, thin/fast tracer streaks, and impact spurts.
        self.drive_mg_fx(dt, r);

        // The player's OWN interior fire, seen from inside — builds + spreads as the fire grows.
        self.drive_interior_fire(r);

        // Crew-of-one interior workload FX: tumbling casings, the breech + reload rounds, the
        // extinguisher foam, and the annunciator task-readout lamps.
        self.drive_casings(dt, r);
        self.drive_foam(dt, r);
        self.drive_loose_shells(dt, r);
        self.drive_reload_fx(dt, r);
        self.drive_track_repair_fx(r);
        self.drive_ext_fx(dt, r);
        self.drive_annunciators(r);

        // Assign the shared volumetric smoke slots (interior fire + muzzle plume + burning-wreck columns).
        self.drive_smoke(r);
    }

    /// Integrate + draw DROPPED main-gun rounds: gravity in turret-local space, a soft bounce off
    /// the basket floor with a clatter, then rolling — velocity decays, the long axis swings
    /// perpendicular to travel (a cylinder rolls sideways), the roll spin follows ground speed,
    /// and the basket wall reflects them. They stay until scooped up (Hotspot::FloorShell).
    fn drive_loose_shells(&mut self, dt: f32, r: &mut Renderer) {
        let tm = self.player.turret_model();
        let show = matches!(self.station, Station::Loader) || true; // visible from any interior view
        for k in 0..N_LOOSE {
            let ap_slot = self.slots.loose_shell[k];
            let he_slot = self.slots.loose_shell_he[k];
            let sh = &mut self.loose[k];
            if !sh.active {
                r.set_visible(ap_slot, false);
                r.set_visible(he_slot, false);
                continue;
            }
            sh.vel.y -= 9.8 * dt;
            sh.pos += sh.vel * dt;
            let grounded = sh.pos.y <= BASKET_FLOOR + 0.005;
            if sh.pos.y < BASKET_FLOOR && sh.vel.y < 0.0 {
                sh.pos.y = BASKET_FLOOR;
                if sh.vel.y < -0.7 {
                    sh.vel.y = -sh.vel.y * 0.28; // one soft bounce
                    if let Some(a) = &self.audio {
                        a.play_at_volume(crate::audio::Sfx::BrassClatter, 0.5);
                    }
                } else {
                    sh.vel.y = 0.0;
                }
            }
            if grounded {
                // rolling: decay, swing the axis perpendicular to travel, spin with ground speed
                let hv = Vec3::new(sh.vel.x, 0.0, sh.vel.z);
                let speed = hv.length();
                if speed > 0.02 {
                    let decel = (1.6 * dt).min(speed);
                    let hv2 = hv * ((speed - decel) / speed);
                    sh.vel.x = hv2.x;
                    sh.vel.z = hv2.z;
                    let roll_axis = hv.normalize().cross(Vec3::Y); // long axis ⊥ travel
                    let want = roll_axis.x.atan2(roll_axis.z);
                    let mut d = (want - sh.axis) % std::f32::consts::TAU;
                    if d > std::f32::consts::PI {
                        d -= std::f32::consts::TAU;
                    }
                    if d < -std::f32::consts::PI {
                        d += std::f32::consts::TAU;
                    }
                    sh.axis += d.clamp(-4.0 * dt, 4.0 * dt);
                    sh.spin += speed / 0.07 * dt;
                } else {
                    sh.vel.x = 0.0;
                    sh.vel.z = 0.0;
                }
            }
            // basket wall (radial clamp around the turret centreline)
            let radial = Vec3::new(sh.pos.x, 0.0, sh.pos.z - 0.25);
            let rd = radial.length();
            if rd > BASKET_R {
                let n = radial / rd;
                sh.pos -= n * (rd - BASKET_R);
                let vn = sh.vel.dot(n);
                if vn > 0.0 {
                    sh.vel -= n * vn * 1.35; // reflect with loss
                    if vn > 0.5 {
                        if let Some(a) = &self.audio {
                            a.play_at_volume(crate::audio::Sfx::BrassClatter, 0.35);
                        }
                    }
                }
            }
            let _ = show;
            let slot = if sh.nature_he { he_slot } else { ap_slot };
            r.set_visible(ap_slot, !sh.nature_he);
            r.set_visible(he_slot, sh.nature_he);
            // Lit like the racked rounds — emissive here hit the gunfire-orange FX path and
            // rendered every dropped round RED. The interior fill is what makes it read.
            r.set_emissive(slot, 0.0);
            r.set_model(
                slot,
                tm * Mat4::from_translation(sh.pos)
                    * Mat4::from_rotation_y(sh.axis)
                    * Mat4::from_rotation_z(sh.spin),
            );
            // Keep the hidden twin co-located; switching nature can never flash at its stale origin.
            let hidden = if sh.nature_he { ap_slot } else { he_slot };
            r.set_model(
                hidden,
                tm * Mat4::from_translation(sh.pos)
                    * Mat4::from_rotation_y(sh.axis)
                    * Mat4::from_rotation_z(sh.spin),
            );
        }
    }

    /// Integrate + draw the VOXEL FOAM: flying blobs shot from the extinguisher stick to the
    /// first surface they meet (basket floor, basket wall, the breech box) and then shrink out
    /// over seconds. All turret-local (the foam rides the tank). Deterministic.
    fn drive_foam(&mut self, dt: f32, r: &mut Renderer) {
        let turret = self.player.turret_model();
        let (bmin, bmax) = (
            crate::interior_rig::BREECH_MIN,
            crate::interior_rig::BREECH_MAX,
        );
        for k in 0..N_FOAM {
            let slot = self.slots.foam[k];
            let f = &mut self.foams[k];
            if !f.active {
                r.set_visible(slot, false);
                continue;
            }
            if f.stuck {
                f.t -= dt;
                if f.t <= 0.0 {
                    f.active = false;
                    r.set_visible(slot, false);
                    continue;
                }
            } else {
                f.t -= dt;
                f.vel.y -= 5.5 * dt; // light, frothy — sub-gravity fall
                f.pos += f.vel * dt;
                let radial = Vec3::new(f.pos.x, 0.0, f.pos.z);
                let hit_floor = f.pos.y <= FOAM_FLOOR + f.size * 0.5;
                let hit_wall = radial.length() >= FOAM_WALL_R;
                let hit_breech = f.pos.x > bmin.x - 0.03
                    && f.pos.x < bmax.x + 0.03
                    && f.pos.y > bmin.y - 0.03
                    && f.pos.y < bmax.y + 0.03
                    && f.pos.z > bmin.z - 0.03
                    && f.pos.z < bmax.z + 0.03;
                if hit_floor || hit_wall || hit_breech {
                    if hit_floor {
                        f.pos.y = FOAM_FLOOR + f.size * 0.45;
                    }
                    if hit_wall {
                        let over = radial.length() - FOAM_WALL_R;
                        f.pos -= radial.normalize_or_zero() * over;
                    }
                    f.stuck = true;
                    f.t = f.t0;
                } else if f.t <= 0.0 {
                    f.active = false; // flew out into the open (hatch etc.) — gone
                    r.set_visible(slot, false);
                    continue;
                }
            }
            // Blocky blob; stuck blobs swell in then shrink out through the tail of their life.
            let life = if f.stuck {
                (f.t / f.t0.max(0.01)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let sc = f.size
                * if f.stuck {
                    ((1.0 - life) * 8.0).min(1.0) * 0.5 + (life * 2.5).min(1.0)
                } else {
                    1.0
                };
            let wp = turret.transform_point3(f.pos);
            r.set_visible(slot, true);
            // Foam is matte chalk-white. Emissive particles went through the gunfire palette and
            // turned orange as soon as they stuck to the floor.
            r.set_emissive(slot, 0.0);
            r.set_model(
                slot,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(sc.max(0.012)),
                    Quat::from_axis_angle(
                        Vec3::new(0.3, 0.8, 0.5).normalize(),
                        hash01(f.seed) * std::f32::consts::TAU,
                    ),
                    wp,
                ),
            );
        }
    }

    /// Integrate + draw the tumbling ejected shell casings: gravity, one restitution bounce off the
    /// turret-basket floor (with a clatter), then retire. Own-colour warm brass so they read against
    /// the cold steel without looking like fire. Deterministic (spin/vel hashed at eject).
    fn drive_casings(&mut self, dt: f32, r: &mut Renderer) {
        for k in 0..N_CASING {
            let slot = self.slots.casing[k];
            let c = &mut self.casings[k];
            if !c.active {
                r.set_visible(slot, false);
                continue;
            }
            if c.carried {
                // In the loader's hands (just pulled from the chamber): no physics, no retiring —
                // drawn at the carry spring with a slow admire-the-thing turn.
                c.ang += 0.8 * dt;
                c.pos = self.carry_pos;
                r.set_visible(slot, true);
                r.set_emissive(slot, 0.9); // still hot, not a lantern
                r.set_model(
                    slot,
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(0.78),
                        Quat::from_axis_angle(c.spin_axis, c.ang),
                        c.pos,
                    ),
                );
                continue;
            }
            c.t -= dt;
            if c.t <= 0.0 {
                c.active = false;
                r.set_visible(slot, false);
                continue;
            }
            c.vel.y -= 9.8 * dt;
            c.pos += c.vel * dt;
            c.ang += c.spin * dt;
            if c.pos.y < c.floor_y && c.vel.y < 0.0 {
                c.pos.y = c.floor_y;
                c.vel.y = -c.vel.y * 0.4; // one soft bounce
                c.vel.x *= 0.7;
                c.vel.z *= 0.7;
                c.spin *= 0.6;
                if !c.bounced {
                    c.bounced = true;
                    if let Some(a) = &self.audio {
                        a.play_at_volume(crate::audio::Sfx::BrassClatter, 0.5);
                    }
                }
            }
            r.set_visible(slot, true);
            r.set_emissive(slot, 1.1); // warm own-colour brass
            r.set_model(
                slot,
                Mat4::from_scale_rotation_translation(
                    Vec3::ONE,
                    Quat::from_axis_angle(c.spin_axis, c.ang),
                    c.pos,
                ),
            );
        }
    }

    /// Draw the breech + reload rounds (only meaningful buttoned-up, so gate on interior_light). The
    /// breech glows an amber HEAT rim right after firing; the chambered round shows warm when Loaded;
    /// the ram round slides home as `ram` climbs; the ready rack sits beside the breech.
    fn drive_reload_fx(&mut self, dt: f32, r: &mut Renderer) {
        let show = self.interior_light > 0.02;
        // The breech / chambered round / ram round / ready rack used to be emissive STAND-IN boxes
        // (the interior before the model existed). The real breech + `Round in the chamber` + the 65
        // rack shells are now baked into the interior GLBs, so these placeholders are redundant — and
        // being warm-emissive boxes ~1 m off the commander's eye, they posterised RED under the cold
        // grade and floated in your face. Hidden here; Phase 5 re-drives the heat-glow / loaded tells
        // on the REAL baked meshes instead. Flip to re-enable the old stand-ins.
        const LEGACY_INTERIOR_BOXES: bool = false;
        let turret = self.player.turret_model();
        let at = |local: Vec3| turret * Mat4::from_translation(local);

        // The track-repair kit, re-homed beside the extinguisher (rear-left wall).
        r.set_visible(self.slots.toolbox, show && !self.carrying_track_kit);
        r.set_emissive(self.slots.toolbox, 0.6);
        r.set_model(self.slots.toolbox, at(TOOLBOX_MESH));

        // The ON-BREECH reload lamp: green = ready, orange = lever open (working), red = shut
        // on a spent case / empty chamber. One of the three colour meshes lit at a time.
        let gm_lamp = self.player.gun_model() * Mat4::from_translation(BREECH_LAMP_POS);
        let lamp_state = if self.lever_open > LEVER_SHUT {
            1 // orange: the breech is open — the reload is in your hands
        } else if matches!(self.breech, Breech::Loaded) {
            0 // green: round seated, lever shut — the gun is UP
        } else {
            2 // red: shut on a spent case (or empty) — work the lever
        };
        for (k, &slot) in self.slots.breech_lamp.iter().enumerate() {
            r.set_visible(slot, show && k == lamp_state);
            r.set_emissive(slot, 2.4);
            r.set_model(slot, gm_lamp);
        }

        // The breech LEVER swings about its local X as it opens; the block COVER slides down
        // with it (the falling-block breech). Both live in the GUN frame (they elevate with it).
        // Hatch lever chases the hatch state (turret frame, on the cupola pedestal).
        let hl_target = if self.hatch_is_open() { 1.0 } else { 0.0 };
        self.hatchlever_t += (hl_target - self.hatchlever_t) * (1.0 - (-8.0f32 * dt).exp());
        r.set_visible(self.slots.hatchlever, self.interior_light > 0.02); // inside views only
                                                                          // The knob is a flat crank disc, thin axis = engine Y (bake bbox 0.24×0.24×0.11): it
                                                                          // TURNS about its own vertical axis like a valve — X-rotation flipped it through the
                                                                          // pedestal ("rotating on the wrong axis").
        r.set_model(
            self.slots.hatchlever,
            turret
                * Mat4::from_translation(HATCHLEVER_POS)
                * Mat4::from_rotation_y(-1.3 * self.hatchlever_t),
        );

        let gm = self.player.gun_model();
        r.set_model(
            self.slots.gunlever,
            gm * Mat4::from_translation(GUNLEVER_POS)
                * Mat4::from_rotation_x(LEVER_SWING * self.lever_open),
        );
        r.set_model(
            self.slots.breechcover,
            gm * Mat4::from_translation(
                BREECHCOVER_POS + Vec3::new(0.0, -COVER_DROP * self.lever_open, 0.0),
            ),
        );

        // Breech block — heat rim decays with the return-to-battery lock after a shot.
        let heat = if matches!(self.breech, Breech::Spent) {
            1.0
        } else {
            self.player.reload.clamp(0.0, 1.0)
        };
        r.set_visible(self.slots.breech, show && LEGACY_INTERIOR_BOXES);
        r.set_emissive(self.slots.breech, 3.0 * heat);
        r.set_model(self.slots.breech, at(Vec3::new(0.0, 1.82, 0.26)));

        // Chambered round — warm/bright = "loaded" at a glance.
        let loaded = show && matches!(self.breech, Breech::Loaded);
        r.set_visible(self.slots.chambered, loaded && LEGACY_INTERIOR_BOXES);
        r.set_emissive(self.slots.chambered, 1.4);
        r.set_model(self.slots.chambered, at(Vec3::new(0.0, 1.84, 0.50)));

        // Legacy ram-round stand-in stays retired; the CARRIED SHELL below is the real mesh.
        r.set_visible(self.slots.ram_round, LEGACY_INTERIOR_BOXES);

        // ── The round in the HANDS: the very rack/floor shell you grabbed, riding the carry
        // spring (lag + swing + droop = the weight), blending onto the tray axis as `ram` climbs.
        if let Some(cm) = self.carry_mesh {
            let slot = match cm {
                CarriedMesh::RackShell(i) => self.slots.shell0 + i,
                CarriedMesh::LooseShell(i) => {
                    if self.loose[i].nature_he {
                        self.slots.loose_shell_he[i]
                    } else {
                        self.slots.loose_shell[i]
                    }
                }
            };
            let align = (self.ram / 0.16).clamp(0.0, 1.0);
            let enter = ((self.ram - 0.10) / 0.90).clamp(0.0, 1.0);
            let ready = self
                .player
                .turret_model()
                .transform_point3(crate::interior_rig::ROUND_READY);
            let seat = self
                .player
                .turret_model()
                .transform_point3(crate::interior_rig::ROUND_SEAT);
            let p = self.carry_pos.lerp(ready, align).lerp(seat, enter);
            // Nose: the sprung carry_dir while hefted, straightening onto the tray axis (turret
            // forward) as the round enters the guide.
            let tf = self.player.turret_forward().normalize_or_zero();
            let nose = (self.carry_dir.lerp(tf, align)).normalize_or_zero();
            r.set_visible(slot, true);
            r.set_model(
                slot,
                Mat4::from_rotation_translation(glam::Quat::from_rotation_arc(Vec3::Z, nose), p),
            );
        }

        // Ready rack — three rounds stowed across the breech (in the loader's forward view, but far
        // enough off the eye that they read as rounds, not a blown-out amber wall).
        for (i, &slot) in self.slots.ready_rack.iter().enumerate() {
            r.set_visible(slot, show && LEGACY_INTERIOR_BOXES);
            r.set_emissive(slot, 0.7);
            r.set_model(slot, at(Vec3::new(-0.35, 1.48 + i as f32 * 0.15, 0.34)));
        }
    }

    /// Draw the loose track end as an articulated run of shoes. At progress 0 it trails and sags
    /// behind the break; at progress 1 it is draped back across the road wheels. The fresh link
    /// waits visibly outboard until the placement click seats it in the gap.
    fn drive_track_repair_fx(&self, r: &mut Renderer) {
        let broken_side = if !self.module_ok(ModuleKind::LeftTrack) {
            Some(-1.55)
        } else if !self.module_ok(ModuleKind::RightTrack) {
            Some(1.55)
        } else {
            None
        };
        let show_run = self.carrying_track_kit && broken_side.is_some();
        if !show_run {
            for &slot in &self.slots.track_repair_shoes {
                r.set_visible(slot, false);
                r.set_outline(slot, 0.0);
            }
            r.set_visible(self.slots.track_replacement_link, false);
            r.set_outline(self.slots.track_replacement_link, 0.0);
            return;
        }

        let side: f32 = broken_side.unwrap();
        let hull = self.player.hull_model();
        let pull = match self.track_repair_phase {
            TrackRepairPhase::GrabEnd => 0.0,
            TrackRepairPhase::PullEnd => self.track_pull,
            TrackRepairPhase::FitLink | TrackRepairPhase::HammerPin => 1.0,
        };
        // The baked hull already contains its intact-looking track skin. Lift the interactive shoes
        // just outboard of that skin so the working run is readable and the hammer can meet it.
        let shoe_x = side + side.signum() * 0.38;
        let shoe_pos = |material_u: f32| {
            // Slide one fixed-length chain along a two-part route: loose tail → open break → wheel
            // crowns. Unlike pointwise lerp, this never compresses all the shoes into one block
            // halfway through the pull.
            let route_t = material_u + pull;
            if route_t <= 1.0 {
                let v = route_t;
                Vec3::new(
                    shoe_x,
                    0.18 + v * 0.56 - (v * std::f32::consts::PI).sin() * 0.08,
                    -2.67 + v * 2.25,
                )
            } else {
                let v = route_t - 1.0;
                Vec3::new(
                    shoe_x,
                    0.74 + (v * std::f32::consts::PI).sin() * 0.34,
                    -0.42 + v * 3.10,
                )
            }
        };
        for (i, &slot) in self.slots.track_repair_shoes.iter().enumerate() {
            let u = i as f32 / (N_REPAIR_SHOES - 1) as f32;
            let p = shoe_pos(u);
            let q = shoe_pos((u + 0.01).min(1.0));
            let tangent = q - p;
            let pitch = -tangent.y.atan2(tangent.z);
            let local =
                Mat4::from_scale_rotation_translation(Vec3::ONE, Quat::from_rotation_x(pitch), p);
            r.set_visible(slot, true);
            r.set_emissive(slot, 0.20);
            r.set_outline(
                slot,
                if self.track_repair_phase == TrackRepairPhase::GrabEnd && i == N_REPAIR_SHOES - 1 {
                    0.8
                } else {
                    0.0
                },
            );
            r.set_model(slot, hull * local);
        }

        let show_link = matches!(
            self.track_repair_phase,
            TrackRepairPhase::FitLink | TrackRepairPhase::HammerPin
        );
        let link_slot = self.slots.track_replacement_link;
        r.set_visible(link_slot, show_link);
        r.set_outline(
            link_slot,
            if self.track_repair_phase == TrackRepairPhase::FitLink {
                0.9
            } else {
                0.0
            },
        );
        if show_link {
            let outboard = if self.track_repair_phase == TrackRepairPhase::FitLink {
                side.signum() * 0.38
            } else {
                0.0
            };
            let local = Vec3::new(shoe_x + outboard, 0.76, -0.30);
            r.set_emissive(
                link_slot,
                if self.track_repair_phase == TrackRepairPhase::FitLink {
                    0.75
                } else {
                    0.24
                },
            );
            r.set_model(link_slot, hull * Mat4::from_translation(local));
        }
    }

    /// Draw the extinguisher foam — a few whitish own-colour puffs spraying at the nearest interior
    /// fire spot while the player is actively knocking the fire down (post-grab). The flame boxes
    /// shrink for free because `drive_interior_fire` already scales off `self.fire`.
    fn drive_ext_fx(&mut self, dt: f32, r: &mut Renderer) {
        let show_interior = self.interior_light > 0.02;
        let grabbing = matches!(self.activity, Some(Activity::Extinguish)) || self.ext_equipped;
        let turret = self.player.turret_model();

        // The BOTTLE (real psx-fire-extinguisher bake): mounted on the turret wall at the
        // Extinguisher hotspot; while the activity runs it rides in the hands instead —
        // lerped off the wall over the grab beat, so you SEE yourself take it down.
        let wall_p = turret.transform_point3(EXT_MOUNT_BASE);
        // Held pose: low-left corner of the view, FPS-equipment style — out of the sightline.
        let hand_p =
            self.station_eye(self.station) + r.camera.forward() * 0.52 + r.camera.right() * -0.30
                - Vec3::new(0.0, 0.44, 0.0);
        // Held: fully in the hands (E-equip) or ramping there over the grab beat (F/LMB path).
        let k = if self.ext_equipped {
            1.0
        } else if grabbing {
            (self.ext_t / EXT_GRAB).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bp = wall_p.lerp(hand_p, k);
        r.set_visible(self.slots.extinguisher, show_interior || grabbing);
        // Lit like everything else (the ×2.4 albedo lift at load is what makes it read; driving
        // emissive here would put it on the unlit FX path and blow it out again).
        // On the WALL the bottle is bolted to the TURRET (it must turn with the tank — a
        // world-fixed identity rotation read as "spinning" whenever the hull turned); in the
        // HANDS it faces the camera, held steady.
        let wall_rot = glam::Quat::from_rotation_y(self.player.heading + self.player.turret_yaw);
        let hand_rot = glam::Quat::from_rotation_y(r.camera.yaw);
        r.set_model(
            self.slots.extinguisher,
            Mat4::from_scale_rotation_translation(
                Vec3::splat(0.45 - 0.17 * k), // full-size on its bracket, compact in the hands
                if k > 0.5 { hand_rot } else { wall_rot },
                bp,
            ),
        );

        // Spray whenever the trigger is squeezed with the bottle up: at a live fire it fights the
        // fire; with no fire it's a test squirt forward (the foam must answer the LMB, always).
        let spraying =
            self.ext_want_spray && (self.ext_equipped || (grabbing && self.ext_t >= EXT_GRAB));
        // ── VOXEL FOAM: while spraying, SHOOT blobs from the nozzle toward the aim point; they
        // fly turret-local, stick to whatever they hit, and shrink away over seconds.
        if spraying {
            self.foam_emit_t -= dt;
            let inv = turret.inverse();
            let hull_m = self.player.hull_model();
            let aim_w = if self.fire_active {
                hull_m.transform_point3(Vec3::new(0.0, 1.15, 0.50))
            } else {
                r.camera.pos + r.camera.forward() * 1.5 - Vec3::new(0.0, 0.45, 0.0)
            };
            let nozzle_w = bp + Vec3::new(0.0, 0.55 * 0.45, 0.0);
            while self.foam_emit_t <= 0.0 {
                self.foam_emit_t += 1.0 / FOAM_RATE;
                self.kill_tick = self.kill_tick.wrapping_add(1);
                let sd = self.kill_tick.wrapping_mul(2654435761);
                let h = |n: u32| hash01(sd.wrapping_add(n));
                let k = self.next_foam % N_FOAM;
                self.next_foam = self.next_foam.wrapping_add(1);
                let dir_w = (aim_w - nozzle_w).normalize_or_zero();
                let jit = Vec3::new(h(1) - 0.5, h(2) - 0.5, h(3) - 0.5) * 1.1;
                self.foams[k] = Foam {
                    active: true,
                    stuck: false,
                    pos: inv.transform_point3(nozzle_w),
                    vel: inv.transform_vector3(dir_w * (3.4 + 1.2 * h(4)) + jit),
                    t: 1.2, // flight timeout
                    t0: FOAM_LIFE * (0.7 + 0.6 * h(5)),
                    size: 0.030 + 0.032 * h(6),
                    seed: sd,
                };
            }
        }
        if !spraying {
            for &slot in self.slots.ext_spray.iter() {
                r.set_visible(slot, false);
            }
            return;
        }
        let hull = self.player.hull_model();
        // Aim at the fire's base when one burns; else a test squirt ahead of the nozzle.
        let target = if self.fire_active {
            hull.transform_point3(Vec3::new(0.0, 1.15, 0.50))
        } else {
            r.camera.pos + r.camera.forward() * 1.5 - Vec3::new(0.0, 0.45, 0.0)
        };
        // Foam streams from the held bottle's nozzle (its top), not from thin air.
        let nozzle = bp + Vec3::new(0.0, 0.55 * 0.45, 0.0);
        for (k, &slot) in self.slots.ext_spray.iter().enumerate() {
            let n = k as u32;
            let along =
                0.30 + 0.55 * hash01(self.fire_t.mul_add(37.0, n as f32) as u32 ^ (n * 911));
            let jit = Vec3::new(
                hash01((n * 7).wrapping_add((self.fire_t * 60.0) as u32)) - 0.5,
                hash01((n * 13).wrapping_add((self.fire_t * 55.0) as u32)) - 0.5,
                hash01((n * 17).wrapping_add((self.fire_t * 50.0) as u32)) - 0.5,
            ) * 0.35;
            // the stream DROOPS under gravity toward the fire base and billows as it travels
            let droop = Vec3::new(0.0, -0.22 * along * along, 0.0);
            let pos = nozzle.lerp(target, along) + jit + droop;
            let pulse = 1.0 + 0.18 * (self.fire_t * 31.0 + k as f32 * 2.1).sin();
            let s = (0.16 + 0.34 * along) * pulse;
            r.set_visible(slot, true);
            r.set_emissive(slot, 2.6 + 0.8 * (1.0 - along));
            r.set_model(
                slot,
                Mat4::from_scale_rotation_translation(Vec3::splat(s), Quat::IDENTITY, pos),
            );
        }
    }

    /// Drive the interior ANNUNCIATOR strip — four coloured lamps that surface the ONE top task without
    /// any text HUD (the frame is a pure crushed 3D buffer): RED = fire (pulse rate ramps as the
    /// cook-off timer shrinks — the countdown you FEEL), AMBER = breech not loaded, BLUE = a repairable
    /// module down, GREEN = the active station (pulses when it needs service). Own-colour emissive,
    /// turret-local, visible only buttoned-up. Deterministic (no rand).
    fn drive_annunciators(&mut self, r: &mut Renderer) {
        let show = self.interior_light > 0.02;
        let turret = self.player.turret_model();
        let t = self.stealth.t;
        let pulse = |rate: f32, hi: f32, lo: f32| if (t * rate).sin() > 0.0 { hi } else { lo };

        // 0: fire lamp (red) — pulse rate ramps up as the cook-off countdown drains.
        let fire_e = if self.fire_active {
            let urg = self
                .fire_ammo_timer
                .map(|tt| (1.0 - (tt / FIRE_TO_AMMO)).clamp(0.0, 1.0))
                .unwrap_or(0.0);
            pulse(6.0 + 22.0 * urg, 6.0, 0.5)
        } else {
            0.0
        };
        // 1: breech lamp (amber) — steady-dim when loaded, pulsing when the gun needs reloading.
        let breech_e = if matches!(self.breech, Breech::Loaded) {
            0.0
        } else {
            pulse(5.0, 4.0, 0.4)
        };
        // 2: repair lamp (blue) — a repairable module is down.
        let repair_e = if self.any_repairable_down() {
            pulse(4.0, 3.5, 0.4)
        } else {
            0.0
        };
        // 3: station lamp (green) — steady when nothing pending, pulses when the current view has work.
        let needs = self.fire_active
            || !matches!(self.breech, Breech::Loaded)
            || self.any_repairable_down();
        let station_e = if needs { pulse(8.0, 4.0, 0.6) } else { 2.4 };

        let emis = [fire_e, breech_e, repair_e, station_e];
        for (i, &slot) in self.slots.annun.iter().enumerate() {
            let local = Vec3::new(-0.30 + i as f32 * 0.11, 1.55, 0.72); // low panel in the crew's forward view
            r.set_visible(slot, show);
            r.set_emissive(slot, emis[i]);
            r.set_model(slot, turret * Mat4::from_translation(local));
        }
    }

    /// Animate the coaxial / hull MG34 FX: the stroboscopic muzzle-flash flicker at the live port,
    /// the pooled thin/fast tracer streaks (muzzle→impact lerp), and the pooled impact spurts (hot
    /// white armour sparks vs softer snow/dirt kicks). Deterministic flicker (hash01, no rand).
    fn drive_mg_fx(&mut self, dt: f32, r: &mut Renderer) {
        // Muzzle flash: a rapid stroboscopic flicker while rounds are cycling; hashed per round so it
        // jitters like real MG chatter rather than a steady lamp.
        if self.mg_flash_t > 0.0 {
            let f = (self.mg_flash_t / MG_FLASH_DUR).clamp(0.0, 1.0);
            let jitter = 0.55 + 0.75 * hash01(self.mg_round.wrapping_mul(2654435761) ^ 0x4d3);
            let s = 0.30 + 0.55 * f * jitter;
            r.set_visible(self.slots.mg_flash, true);
            r.set_emissive(self.slots.mg_flash, 9.0 * jitter);
            r.set_model(
                self.slots.mg_flash,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(s),
                    Quat::IDENTITY,
                    self.mg_flash_pos,
                ),
            );
        } else {
            r.set_visible(self.slots.mg_flash, false);
        }

        // Thin/fast tracer streaks — small + short vs the cannon tracer (0.5,0.5,7.5), so they read
        // as a rapid MG stream snapping downrange, not big cannon bolts.
        for k in 0..N_MG_TRACER {
            let slot = self.slots.mg_tracer[k];
            let tr = &mut self.mg_tracers[k];
            if tr.active {
                tr.t += dt;
                let frac = (tr.t / tr.dur).clamp(0.0, 1.0);
                let cur = tr.from.lerp(tr.to, frac);
                let rot = Quat::from_rotation_arc(Vec3::Z, tr.dir);
                r.set_visible(slot, true);
                r.set_emissive(slot, 14.0); // blooms to a hot yellow-white core
                r.set_model(
                    slot,
                    Mat4::from_scale_rotation_translation(Vec3::new(0.20, 0.20, 2.4), rot, cur),
                );
                if frac >= 1.0 {
                    tr.active = false;
                    r.set_visible(slot, false);
                }
            } else {
                r.set_visible(slot, false);
            }
        }

        // Impact spurts (own-colour warm-white): armour hits are small + white-hot sparks; terrain
        // hits are larger + softer snow/dirt kicks. A quick expand-and-fade burst either way.
        for k in 0..N_MG_IMPACT {
            let slot = self.slots.mg_impact[k];
            let im = &mut self.mg_impacts[k];
            if im.t > 0.0 {
                im.t -= dt;
                let frac = (im.t / im.dur).clamp(0.0, 1.0); // 1 → 0 over its life
                let grow = 1.0 - frac;
                let (base, emissive) = if im.tank { (0.30, 15.0) } else { (0.55, 5.5) };
                let s = base * (0.5 + 2.2 * grow);
                r.set_visible(slot, true);
                r.set_emissive(slot, emissive * frac.max(0.2));
                r.set_model(
                    slot,
                    Mat4::from_scale_rotation_translation(Vec3::splat(s), Quat::IDENTITY, im.pos),
                );
            } else {
                r.set_visible(slot, false);
            }
        }
    }

    /// The player's own tank burning — interior emissive flame that BUILDS as the fire spreads.
    /// Successive anchor points ignite at higher fire levels (visible spread); all zeroed when the
    /// fire is out. Positioned in hull-local interior spots so it reads in the first-person view.
    fn drive_interior_fire(&mut self, r: &mut Renderer) {
        const SPOTS: [Vec3; N_INT_FIRE] = [
            Vec3::new(0.00, 1.15, 0.50),  // hull floor, ahead of the crew
            Vec3::new(-0.60, 1.25, 1.35), // driver's station
            Vec3::new(0.50, 1.70, -0.20), // turret basket
            Vec3::new(0.10, 1.95, 0.30),  // up into the turret
        ];
        let hull = self.player.hull_model();
        for k in 0..N_INT_FIRE {
            let slot = self.slots.interior_fire[k];
            let thresh = k as f32 * 0.24; // each spot ignites deeper into the fire → visible spread
            if self.fire <= thresh + 0.02 {
                r.set_visible(slot, false);
                continue;
            }
            let local = (self.fire - thresh).clamp(0.0, 1.0);
            let flick = fire_glow(
                self.fire_t,
                (k as u32).wrapping_mul(2654435761) ^ 0x51ed,
                1.0,
                0.0,
            )
            .clamp(0.4, 1.6);
            let pos = hull.transform_point3(SPOTS[k]);
            let s = 0.5 + 1.3 * local * flick;
            r.set_visible(slot, true);
            r.set_emissive(slot, (6.0 + 12.0 * local) * flick);
            r.set_model(
                slot,
                Mat4::from_scale_rotation_translation(
                    Vec3::new(s * 0.7, s, s * 0.7),
                    Quat::IDENTITY,
                    pos,
                ),
            );
        }
    }

    // ── Death instant-replay (kill-cam) ──────────────────────────────────────────────────────────
    /// Render one frame of the fatal-shot replay at normalised phase `t` in [0,1], driven ENTIRELY
    /// off `last_fatal`: the enemy firing (muzzle flash), the shell streaking in over slow-motion
    /// ballistics, the penetration, and the spall/shrapnel cone spraying through the interior to the
    /// crew/modules it killed — the "you see exactly what killed you" beat. No-op (leaves the last
    /// frame) if there is no fatal record. The world is frozen at the death pose; this only overrides
    /// the camera and drives the dedicated replay FX slots (it never touches the live FX pools).
    pub fn render_replay(&mut self, t: f32, r: &mut Renderer) {
        let Some(f) = self.last_fatal.clone() else {
            return;
        };
        let t = t.clamp(0.0, 1.0);

        // Cinematic look: reticle off, no crew-KO/optic overlay, and thin the shipped combat fog so
        // the distant shooter reads across the ~300 m shell corridor.
        r.reticle = false;
        r.crew_ko = 0.0;
        r.optic_blast = 0.0;
        r.fog_density = 0.0018;

        // Freeze-frame world: re-push the Tiger's death-pose transforms (enemies keep theirs from the
        // last live update / the headless build) and hide the volatile live FX so no leftover
        // in-flight tracer / impact / death-blast from the killing frame lingers under the replay FX.
        r.set_model(self.slots.tiger[0], self.player.hull_model());
        r.set_model(self.slots.tiger[1], self.player.turret_model());
        r.set_model(self.slots.tiger[2], self.player.gun_model());
        let glass_models = [
            self.player.hull_model(),
            self.player.turret_model(),
            self.player.gun_model(),
        ];
        for (k, s) in self.slots.tiger_glass.iter().enumerate() {
            if let Some(s) = s {
                r.set_model(*s, glass_models[k]);
            }
        }
        for (k, s) in self.slots.tiger_rtt.iter().enumerate() {
            if let Some(s) = s {
                r.set_model(*s, glass_models[k]);
            }
        }
        for s in [self.slots.muzzle, self.slots.tracer, self.slots.impact] {
            r.set_visible(s, false);
        }
        for k in 0..E_FX_POOL {
            r.set_visible(self.slots.e_tracer[k], false);
            r.set_visible(self.slots.e_impact[k], false);
        }
        for k in 0..N_BLAST {
            r.set_visible(self.slots.blast[k], false);
        }
        for k in 0..N_DEBRIS {
            r.set_visible(self.slots.debris[k], false);
        }
        // Interior bulb + any live near-miss snow puffs are irrelevant to the external kill-cam.
        r.set_visible(self.slots.bulb, false);
        for k in 0..N_SNOW {
            r.set_visible(self.slots.snow[k], false);
        }
        // Any in-flight MG chatter from the killing frame is hidden too.
        r.set_visible(self.slots.mg_flash, false);
        for k in 0..N_MG_TRACER {
            r.set_visible(self.slots.mg_tracer[k], false);
        }
        for k in 0..N_MG_IMPACT {
            r.set_visible(self.slots.mg_impact[k], false);
        }

        // ── X-RAY anatomy (from the impact onward): ghost the Tiger to a screen-door schematic
        // and light the interior — 5 crew silhouettes + 12 module blocks at their real hull-local
        // damage-model positions, with the crew/modules THIS shell killed flashing hot while the
        // rest sit faint. The "you see exactly what killed you" beat, WT-style, through PS1 glass.
        let xray = t >= REPLAY_IMPACT_T0;
        for &slot in &self.slots.tiger {
            r.set_xray(slot, xray);
        }
        let hull = self.player.hull_model();
        let flick = 2.4 + 1.9 * (t * 46.0).sin(); // hot strobe for the casualties
                                                  // ONE body — you, at the seat you died at. The other silhouette slots stay dark (solo crew).
        for (i, &slot) in self.slots.r_crew.iter().enumerate() {
            let show = xray && i == 0;
            r.set_visible(slot, show);
            if show {
                let seat = hull
                    .inverse()
                    .transform_point3(self.station_eye(self.station))
                    - Vec3::new(0.0, 0.77, 0.0);
                r.set_model(slot, hull * Mat4::from_translation(seat));
                r.set_emissive(slot, if f.hit_body { flick } else { 0.30 });
            }
        }
        for (i, md) in self.modules.iter().enumerate() {
            let slot = self.slots.r_mod[i];
            r.set_visible(slot, xray);
            if xray {
                r.set_model(slot, hull * Mat4::from_translation(md.kind.station()));
                let hit = f.hit_modules.contains(&md.kind);
                r.set_emissive(slot, if hit { flick * 1.1 } else { 0.22 });
            }
        }

        // Kill-cam camera: opens beside the shooter down the shell path, dollies onto the entry.
        let (eye, look) = replay_camera(f.shooter_pos, f.entry_point, f.spall_dir, t);
        r.camera.pos = eye;
        r.camera.set_forward(look - eye);
        r.camera.fov_y = 50f32.to_radians();

        // Optional slow-mo audio cue (diegetic, one-shot): the distant report as the round leaves,
        // the penetration slam as it holes the plate. Gated so it fires once per replay pass. Fire-
        // and-forget on the existing Sfx sinks; silent if there's no audio device.
        if t <= 0.02 {
            self.replay_cue = 0;
        }
        if self.replay_cue == 0 {
            if let Some(a) = &self.audio {
                a.play(crate::audio::Sfx::EnemyGun);
            }
            self.replay_cue = 1;
        }
        if t >= REPLAY_IMPACT_T0 && self.replay_cue < 2 {
            if let Some(a) = &self.audio {
                a.play(crate::audio::Sfx::ImpactTank);
                a.play(crate::audio::Sfx::PenetrationHit);
            }
            self.replay_cue = 2;
        }

        // Shooter muzzle flash at t≈0.
        if t < REPLAY_MUZZLE_END {
            let f0 = 1.0 - t / REPLAY_MUZZLE_END; // 1 → 0
            let s = 1.4 + 6.0 * f0;
            r.set_visible(self.slots.r_muzzle, true);
            r.set_emissive(self.slots.r_muzzle, 15.0);
            r.set_model(
                self.slots.r_muzzle,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(s),
                    Quat::IDENTITY,
                    f.shooter_pos,
                ),
            );
        } else {
            r.set_visible(self.slots.r_muzzle, false);
        }

        // The shell: a thin white-hot streak travelling shooter → entry over the slow-mo window.
        let sfrac = replay_shell_frac(t);
        if sfrac > 0.0 && sfrac < 1.0 {
            let pos = replay_shell_pos(f.shooter_pos, f.entry_point, t);
            let dir = (f.entry_point - f.shooter_pos).normalize_or_zero();
            let rot = Quat::from_rotation_arc(Vec3::Z, dir);
            r.set_visible(self.slots.r_shell, true);
            r.set_emissive(self.slots.r_shell, 18.0); // blows to a white-hot core through bloom
            r.set_model(
                self.slots.r_shell,
                Mat4::from_scale_rotation_translation(Vec3::new(0.35, 0.35, 9.0), rot, pos),
            );
        } else {
            r.set_visible(self.slots.r_shell, false);
        }

        // Penetration flash at the entry, then a lingering ember through the final hold.
        if t >= REPLAY_IMPACT_T0 && t < REPLAY_IMPACT_END {
            let a = (t - REPLAY_IMPACT_T0) / (REPLAY_IMPACT_END - REPLAY_IMPACT_T0); // 0 → 1
            let s = (3.4 * (1.2 - a)).max(0.4);
            r.set_visible(self.slots.r_flash, true);
            r.set_emissive(self.slots.r_flash, 20.0 * (1.0 - 0.55 * a));
            r.set_model(
                self.slots.r_flash,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(s),
                    Quat::IDENTITY,
                    f.entry_point,
                ),
            );
        } else if t >= REPLAY_IMPACT_END {
            r.set_visible(self.slots.r_flash, true);
            r.set_emissive(self.slots.r_flash, 6.0);
            r.set_model(
                self.slots.r_flash,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.55),
                    Quat::IDENTITY,
                    f.entry_point,
                ),
            );
        } else {
            r.set_visible(self.slots.r_flash, false);
        }

        // Spall / shrapnel cone into the interior.
        self.drive_replay_spall(&f, t, r);
    }

    /// Spray the spall/shrapnel cone for the replay: `N_SHARD` glowing fragments streak from the
    /// entry point along the spall axis toward what the fatal blow actually struck — YOUR body
    /// position and/or the modules (`hit_body` / `hit_modules`, world-space through the death-pose
    /// hull). Fragments fan with deterministic angular spread (hashed, no rand), staggered so they
    /// read as a spray rather than a lockstep volley.
    fn drive_replay_spall(&mut self, f: &FatalHit, t: f32, r: &mut Renderer) {
        let hull = self.player.hull_model();
        let mut targets: Vec<Vec3> = Vec::new();
        if f.hit_body {
            targets.push(self.station_eye(self.station) - Vec3::new(0.0, 0.35, 0.0));
        }
        for &kind in &f.hit_modules {
            targets.push(hull.transform_point3(kind.station()));
        }
        if targets.is_empty() {
            // No discrete station recorded — spray a generic swath across the compartment.
            targets.push(f.entry_point + f.spall_dir.normalize_or_zero() * 2.0);
        }
        let entry = f.entry_point;
        let axis = f.spall_dir.normalize_or_zero();
        let mut nrm = axis.cross(Vec3::Y).normalize_or_zero();
        if nrm.length_squared() < 1e-4 {
            nrm = Vec3::X;
        }
        let binrm = axis.cross(nrm).normalize_or_zero();

        for i in 0..N_SHARD {
            let slot = self.slots.r_shards[i];
            let h = |k: u32| hash01((i as u32).wrapping_mul(2654435761) ^ k.wrapping_mul(40503));
            // Each fragment fires slightly after the impact and crosses the compartment over its own
            // brief window, so the cone erupts rather than snapping into place.
            let s0 = REPLAY_SPALL_T0 + h(1) * 0.06;
            let dur = 0.16 + 0.14 * h(2);
            if t < s0 {
                r.set_visible(slot, false);
                continue;
            }
            let local = ((t - s0) / dur).clamp(0.0, 1.0);
            if local >= 1.0 || t > REPLAY_SPALL_T1 {
                r.set_visible(slot, false);
                continue;
            }
            // Destination: a struck station plus a hashed lateral spread → the fan of the cone.
            let dest0 = targets[i % targets.len()];
            let spread = 0.35 + 0.55 * (dest0 - entry).length().min(3.0) / 3.0;
            let off = (nrm * (h(3) - 0.5) + binrm * (h(4) - 0.5)) * spread * 2.0;
            let dest = dest0 + off;
            let pos = entry.lerp(dest, local);
            let dir = (dest - entry).normalize_or_zero();
            let rot = Quat::from_rotation_arc(Vec3::Z, dir);
            let sz = 0.9 + 0.5 * h(5);
            r.set_visible(slot, true);
            r.set_emissive(slot, 16.0 * (1.0 - 0.6 * local)); // cools as it crosses the compartment
            r.set_model(
                slot,
                Mat4::from_scale_rotation_translation(
                    Vec3::new(sz * 0.5, sz * 0.5, sz * (1.2 + 2.0 * local)),
                    rot,
                    pos,
                ),
            );
        }
    }

    /// LMB/E on the HATCH LEVER (cupola pedestal): work it — the hatch opens/closes. Reachable
    /// from the commander's seat and the interior walk. Uses the per-frame `hover_lever` gaze
    /// (the SAME test that draws the outline + prompt, so what glows is what clicks).
    pub fn interact_hatch_lever(&mut self, _cam_fwd: Vec3, _cam_pos: Vec3) -> bool {
        if !matches!(self.mode, Mode::Tank) || !self.hover_lever {
            return false;
        }
        self.toggle_hatch();
        if let Some(a) = &self.audio {
            a.play_at_volume(crate::audio::Sfx::BreechClack, 0.5);
        }
        println!(
            "[kessel] LUKENHEBEL — Luke {}",
            if self.hatch_is_open() { "AUF" } else { "ZU" }
        );
        true
    }

    pub fn hover_hatch_lever(&self) -> bool {
        self.hover_lever
    }

    pub fn hovering_extinguisher(&self) -> bool {
        self.hover == Some(Hotspot::Extinguisher)
    }

    pub fn hovering_repair_kit(&self) -> bool {
        self.hover == Some(Hotspot::RepairKit)
    }

    /// E on the extinguisher: take it off the wall (equip — LMB then sprays); E again anywhere
    /// puts it back on its bracket. Returns true if the press was consumed. The equip pattern:
    /// E grabs/stows the TOOL, LMB uses what's in your hands.
    pub fn interact_equip(&mut self) -> bool {
        if self.ext_equipped {
            self.ext_equipped = false;
            println!("[kessel] FEUERLÖSCHER zurück an die Wand");
            return true;
        }
        if matches!(self.station, Station::Loader) && self.hover == Some(Hotspot::Extinguisher) {
            self.ext_equipped = true;
            println!("[kessel] FEUERLÖSCHER in der Hand — LMB: sprühen, E: zurück");
            return true;
        }
        false
    }

    /// E on a stowed or dropped 8.8 cm round: put that exact round into the loader's hands.
    /// Carrying is latched; LMB is then free to operate the breech and perform the deliberate
    /// forward ram instead of also serving as the pickup button.
    pub fn interact_round(&mut self, cam_fwd: Vec3) -> bool {
        if matches!(self.mode, Mode::Tank)
            && matches!(self.station, Station::Loader)
            && self.carry_mesh.is_some()
        {
            // E is a true equip toggle. The next loader update converts the latched round into a
            // loose floor shell with its current hand momentum; there is no inventory teleport.
            self.grab = false;
            println!("[kessel] GRANATE LOSGELASSEN");
            return true;
        }
        if matches!(self.mode, Mode::Tank)
            && matches!(self.station, Station::Loader)
            && matches!(self.breech, Breech::Loaded)
            && self.lever_open >= LEVER_OPEN
            && self.hover == Some(Hotspot::LoadedRound)
            && self.carried_casing.is_none()
            && !self.ext_equipped
        {
            let Some(i) = self.loose.iter().position(|shell| !shell.active) else {
                println!("[kessel] KEIN FREIER PLATZ — erst eine Bodengranate aufnehmen");
                return true;
            };
            self.loose[i].nature_he = self.he_loaded;
            self.carry_pos = self
                .player
                .turret_model()
                .transform_point3(crate::interior_rig::ROUND_READY);
            self.carry_vel = Vec3::ZERO;
            self.carry_dir = self.player.turret_forward().normalize_or_zero();
            self.carry_mesh = Some(CarriedMesh::LooseShell(i));
            self.grab = true;
            self.breech = Breech::Empty;
            println!(
                "[kessel] ENTLADEN: {} — in den Händen",
                if self.he_loaded {
                    "SprGr (HE)"
                } else {
                    "PzGr 39 (AP)"
                }
            );
            if let Some(a) = &self.audio {
                a.play_at_volume(crate::audio::Sfx::BreechClack, 0.45);
            }
            return true;
        }
        if !matches!(self.mode, Mode::Tank)
            || !matches!(self.station, Station::Loader)
            || matches!(self.breech, Breech::Loaded)
            || self.carried_casing.is_some()
            || self.ext_equipped
        {
            return false;
        }

        let tm = self.player.turret_model();
        let hm = self.player.hull_model();
        match self.hover {
            Some(Hotspot::RackSlot(i)) if !self.slot_taken[i] => {
                self.slot_taken[i] = true;
                let (frame, pos, _q) = crate::interior_rig::SHELL_SLOTS[i];
                let m = if frame == crate::interior_rig::Frame::Hull {
                    hm
                } else {
                    tm
                };
                self.carry_pos = m.transform_point3(pos);
                self.carry_vel = Vec3::ZERO;
                self.carry_dir = cam_fwd;
                self.carry_mesh = Some(CarriedMesh::RackShell(i));
                self.grab = true;
                self.he_loaded = self.slot_nature_he.get(i).copied().unwrap_or(false);
                println!(
                    "[kessel] AUSGERÜSTET: {} — zum Verschluss tragen",
                    if self.he_loaded {
                        "SprGr (HE, gelb)"
                    } else {
                        "PzGr 39 (AP, grau)"
                    }
                );
            }
            Some(Hotspot::FloorShell(i)) if self.loose[i].active => {
                let local = self.loose[i].pos;
                self.loose[i].active = false;
                self.carry_pos = tm.transform_point3(local);
                self.carry_vel = Vec3::ZERO;
                self.carry_dir = cam_fwd;
                self.carry_mesh = Some(CarriedMesh::LooseShell(i));
                self.grab = true;
                self.he_loaded = self.loose[i].nature_he;
                println!("[kessel] GRANATE VOM BODEN ausgerüstet");
            }
            _ => return false,
        }
        if let Some(a) = &self.audio {
            a.play_at_volume(crate::audio::Sfx::BreechClack, 0.35);
        }
        true
    }

    /// Headless-only: the guaranteed demo wall's world AABB + whether it still stands, for the
    /// `crush` subcommand to aim the approach at and assert against. `None` only if it was culled.
    pub fn debug_demo_wall(&self) -> Option<(Vec3, Vec3, bool)> {
        self.demo_struct.map(|i| {
            let d = &self.destructibles[i];
            (d.min, d.max, d.alive)
        })
    }

    /// T — swap the chambered round's NATURE (PzGr AP ↔ SprGr HE). Interim instant hand-swap;
    /// the loader-station ritual owns real shell selection later (rack → grab → ram).
    pub fn toggle_nature(&mut self) {
        self.he_loaded = !self.he_loaded;
        println!(
            "[kessel] GELADEN: {}",
            if self.he_loaded {
                "SprGr (HE) — Sprenggranate"
            } else {
                "PzGr 39 (AP) — Panzergranate"
            }
        );
    }

    /// Player-facing T binding: ammunition is selected by the physical round, never by changing a
    /// chamber flag through steel. The key remains as a discoverability reminder for old muscle
    /// memory while the embodied loader ritual owns the actual swap.
    pub fn request_nature_swap(&self) {
        println!(
            "[kessel] MUNITION IST KÖRPERLICH — Verschluss öffnen, geladene Granate mit E ziehen, gewünschte graue/gelbe Granate mit E aufnehmen"
        );
    }

    /// The sights-HUD ammo tag: (label, colour).
    pub fn nature_hud(&self) -> (&'static str, [f32; 4]) {
        if self.he_loaded {
            ("SprGr — HE", [0.95, 0.55, 0.35, 0.9])
        } else {
            ("PzGr 39 — AP", [0.75, 0.88, 0.70, 0.9])
        }
    }

    /// Headless-only: park enemy `idx` at an exact spot (the overpen demo stages a T-34 behind
    /// the demo wall). Grounds it on the heightfield.
    pub fn debug_place_enemy(&mut self, idx: usize, x: f32, z: f32, heading: f32) {
        if let Some(e) = self.enemies.get_mut(idx) {
            e.pos = Vec3::new(x, self.hf.height_at(x, z), z);
            e.heading = heading;
            e.speed = 0.0;
        }
    }

    /// Headless-only: is enemy `idx` still alive? (overpen demo assertion)
    pub fn debug_enemy_alive(&self, idx: usize) -> bool {
        self.enemies.get(idx).map_or(false, |e| e.alive)
    }

    /// Headless-only: the first living T-34 in the swarm (the overpen demo wants the thin-skinned
    /// kind behind its wall, not an SU-100 casemate).
    pub fn debug_first_t34(&self) -> Option<usize> {
        self.enemies
            .iter()
            .position(|e| e.alive && matches!(e.kind, crate::tank::EnemyKind::T34))
    }

    /// Headless-only: move every living enemy ~3 km out so a demo has a guaranteed-clear firing
    /// range (they stay alive — no victory/wreck side effects — just far out of every line).
    pub fn debug_banish_enemies(&mut self) {
        for (i, e) in self.enemies.iter_mut().enumerate() {
            if e.alive {
                e.pos.x = 3000.0 + 40.0 * i as f32;
                e.pos.z = 3000.0;
                e.speed = 0.0;
            }
        }
    }

    /// Headless-only: (solid, flying, frozen) chunk counts of the demo wall's grid.
    pub fn debug_demo_chunks(&self) -> (usize, usize, usize) {
        self.demo_struct
            .map_or((0, 0, 0), |i| self.chunks.debug_counts(i))
    }

    /// Headless-only: park every enemy's reload clock at infinity so a demo can run near the city
    /// without incoming fire (the `crush` subcommand teleports the Tiger to the demo wall, well
    /// inside detection range — the swarm may see it, but must not shoot the demo apart).
    pub fn debug_hold_enemy_fire(&mut self) {
        for e in &mut self.enemies {
            e.reload = f32::INFINITY;
        }
    }

    /// Headless-only: force a fatal flank PENETRATION so `last_fatal` is populated for the replay
    /// harness (the `replay` subcommand / `KESSEL_REPLAY` hook). Uses the nearest living T-34's muzzle
    /// as the shooter and a real left-flank entry, then drives the actual penetration resolver until
    /// the Tiger is knocked out — so the recorded FatalHit (entry, spall dir, downed crew/modules)
    /// comes from the genuine spall model, not a fabrication. Deterministic (armor_tick advances).
    pub fn debug_fatal_penetration(&mut self, arc: Arc) {
        let center = self.player.center();
        let hull = self.player.hull_model();
        let entry_w = hull.transform_point3(Vec3::new(-1.9, 1.3, 0.2)); // left flank, hull-mid height
        let shooter = self
            .nearest_enemy()
            .map(|i| self.enemies[i].muzzle().0)
            .unwrap_or(center + Vec3::new(-260.0, 1.0, 30.0));
        let shell_dir = (entry_w - shooter).normalize_or_zero();
        for _ in 0..48 {
            if !self.player.alive {
                break;
            }
            self.resolve_player_penetration(arc, shooter, entry_w, shell_dir, None);
        }
    }
}

/// Tank-gun ray/sphere test: hits nearer than the 88's own barrel length are culled so a shell
/// can't "hit" something the muzzle is already past. Small arms use [`ray_sphere_near`].
fn ray_sphere(o: Vec3, d: Vec3, c: Vec3, r: f32) -> Option<f32> {
    ray_sphere_near(o, d, c, r, GUN_LEN)
}

/// Ray/sphere with an explicit near cull. Hand weapons (Luger, infantry rifles) fire from the
/// eye, not past a 3 m barrel — culling at GUN_LEN made point-blank shots whiff in BOTH
/// directions (a soldier at arm's length could neither hit nor be hit), exactly the Luger's
/// last-resort use case.
fn ray_sphere_near(o: Vec3, d: Vec3, c: Vec3, r: f32, near_cull: f32) -> Option<f32> {
    let oc = o - c;
    let b = oc.dot(d);
    let cc = oc.dot(oc) - r * r;
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let near = -b - s;
    // Prefer the near surface, but at point-blank range the near root falls behind the muzzle
    // (or goes negative when the muzzle is inside the sphere) — fall back to the far root so a
    // shell that clearly passes through the target still scores a hit instead of a phantom miss.
    if near > near_cull {
        Some(near)
    } else {
        let far = -b + s;
        if far > near_cull {
            Some(far)
        } else {
            None
        }
    }
}

pub(crate) fn ray_terrain(from: Vec3, dir: Vec3, hf: &Heightfield, maxd: f32) -> Option<f32> {
    let step = 1.5;
    let mut t = GUN_LEN;
    while t < maxd {
        let p = from + dir * t;
        if p.y <= hf.height_at(p.x, p.z) {
            return Some(t);
        }
        t += step;
    }
    None
}

/// Ray/AABB slab test bounded to (GUN_LEN, `max_t`]. Returns the ENTRY distance along `d` (which must
/// be unit length) if the ray crosses the box within range, else None. Mirrors ray_terrain/ray_sphere's
/// GUN_LEN near-cull (a box wrapping the muzzle never self-hits) and the stealth slab-test style.
fn ray_aabb(o: Vec3, d: Vec3, min: Vec3, max: Vec3, max_t: f32) -> Option<f32> {
    let o = o.to_array();
    let d = d.to_array();
    let lo = min.to_array();
    let hi = max.to_array();
    let mut tmin = GUN_LEN;
    let mut tmax = max_t;
    for k in 0..3 {
        if d[k].abs() < 1e-6 {
            if o[k] < lo[k] || o[k] > hi[k] {
                return None; // parallel to this slab and outside it → miss
            }
        } else {
            let inv = 1.0 / d[k];
            let mut t1 = (lo[k] - o[k]) * inv;
            let mut t2 = (hi[k] - o[k]) * inv;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    Some(tmin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_hits_aabb_then_depletes_hp_and_downgrades_occluder() {
        // ── AABB hit test: a round laid ON the wall registers a hit; one laid 90° off it misses. ──
        let min = Vec3::new(-1.0, 0.0, 40.0);
        let max = Vec3::new(1.0, 4.0, 41.0);
        let eye = Vec3::new(0.0, 2.0, 0.0);
        assert!(
            ray_aabb(eye, Vec3::new(0.0, 0.0, 1.0), min, max, 100.0).is_some(),
            "a round laid straight on the wall must hit its AABB"
        );
        assert!(
            ray_aabb(eye, Vec3::new(1.0, 0.0, 0.0), min, max, 100.0).is_none(),
            "a round laid 90° off the wall must miss it"
        );
        // The entry distance is the near face of the box.
        let t = ray_aabb(eye, Vec3::new(0.0, 0.0, 1.0), min, max, 100.0).unwrap();
        assert!(
            (t - 40.0).abs() < 1e-3,
            "entry distance should land on the near face, got {t}"
        );

        // ── HP depletion: HP scales with size, and a 3-hit facade only drops on the third shell. ──
        assert!(
            structure_hp(9.0) > structure_hp(3.0),
            "a bigger facade must be tougher"
        );
        assert!(
            structure_hp(3.0) <= STRUCT_SHELL_DAMAGE,
            "a small wall must drop in one shell"
        );
        let mut d = Destructible {
            intact: 0,
            rubble: 1,
            occ: 0,
            min,
            max,
            hp: structure_hp(9.0),
            max_hp: structure_hp(9.0),
            alive: true,
            seed: 1,
            class: Material::Brick,
        };
        assert!(
            !d.hit(STRUCT_SHELL_DAMAGE),
            "first shell must not drop a large facade"
        );
        assert!(
            !d.hit(STRUCT_SHELL_DAMAGE),
            "second shell must not drop it either"
        );
        assert!(d.hit(STRUCT_SHELL_DAMAGE), "third shell must collapse it");
        assert!(!d.alive, "a collapsed wall is no longer alive");
        assert!(
            !d.hit(STRUCT_SHELL_DAMAGE),
            "a felled wall takes no further damage"
        );

        // ── Occluder downgrade: the standing wall breaks a tank-centre sightline; the rubble does not.
        let from = Vec3::new(0.0, 1.5, 0.0);
        let to = Vec3::new(0.0, 1.5, 60.0);
        let intact = stealth::Occluder::from_aabb(min, max);
        assert!(
            intact.blocks_segment(from, to),
            "the standing wall must break the sightline"
        );
        let rubble = collapsed_occluder(intact);
        assert!(
            !rubble.blocks_segment(from, to),
            "the collapsed rubble must no longer break it"
        );
    }

    #[test]
    fn crush_tier_ladder_holds() {
        // The material ladder (docs/DESTRUCTION.md §2/§4): softer = lower tier, faster grind,
        // more damage per second. Bunker concrete neither yields speed nor takes crush damage.
        let (t, b, s, k) = (
            Material::Timber.crush(),
            Material::Brick.crush(),
            Material::Stone.crush(),
            Material::Bunker.crush(),
        );
        assert!(
            t.tier < b.tier && b.tier < s.tier && s.tier < k.tier,
            "tiers must escalate"
        );
        assert!(
            t.vmax > b.vmax && b.vmax > s.vmax,
            "harder walls hold the tank slower"
        );
        assert!(t.dps > b.dps && b.dps > s.dps, "softer walls crush faster");
        assert_eq!(k.vmax, 0.0, "bunker concrete is a dead stop");
        assert_eq!(k.dps, 0.0, "bunker concrete takes no crush damage");
        // Grind arithmetic: a small brick wall gives way in ~a second of pushing; a big facade
        // resists noticeably longer — but both fall well inside a playable grind.
        let small = structure_hp(3.0) / b.dps;
        let large = structure_hp(9.0) / b.dps;
        assert!(
            small < 1.0,
            "a small wall must give way in under a second, got {small:.2}s"
        );
        assert!(
            large > 1.5 * small && large < 4.0,
            "a facade should grind ~2-3s, got {large:.2}s"
        );
        // A Tiger at speed bursts timber outright rather than grinding it.
        assert!(
            TIMBER_BURST_SPEED < crate::tank::TOP_SPEED * 0.5,
            "the burst threshold must be reachable well below top speed"
        );
    }

    #[test]
    fn bow_contact_finds_walls_and_leaves_clear_ground_alone() {
        // A wall dead ahead of the bow: contact, pushed back out (-Z, toward the approach).
        let (min, max) = (Vec3::new(-3.0, 0.0, 10.0), Vec3::new(3.0, 4.0, 10.8));
        let dir = Vec3::Z;
        let hit = nose_contact(Vec3::new(0.0, 0.0, 9.0), dir, min, max);
        let (push, overlap) = hit.expect("bow 1.0 m from the face must be in contact (radius 1.4)");
        assert!(
            push.z < -0.9,
            "separation must push back toward the approach, got {push:?}"
        );
        assert!(
            (overlap - 0.4).abs() < 1e-3,
            "overlap = radius - gap = 0.4, got {overlap}"
        );
        // Well short of the wall: no contact. Off to the side past the end: no contact.
        assert!(nose_contact(Vec3::new(0.0, 0.0, 7.0), dir, min, max).is_none());
        assert!(nose_contact(Vec3::new(6.0, 0.0, 10.4), dir, min, max).is_none());
        // Degenerate fully-inside frame: still separates, straight back along the approach.
        let (push, overlap) = nose_contact(Vec3::new(0.0, 0.0, 10.4), dir, min, max)
            .expect("a bow centre inside the box must still resolve");
        assert!(
            push.z < -0.9 && overlap > 1.0,
            "inside case backs out along -dir"
        );
    }

    #[test]
    fn pen_ray_budget_asymmetry() {
        // The pen-ray arithmetic (docs/DESTRUCTION.md §3): walls are concealment, not cover —
        // but only if your gun brought the budget. The SAME hut/brick transit that barely
        // registers on the 88 defangs the 76 mm — the range-asymmetry pillar, derived.
        use crate::ballistics::{budget_mm, BR_350B, PZGR_39};
        let b88 = budget_mm(&PZGR_39, 100.0);
        let b76 = budget_mm(&BR_350B, 100.0);
        assert!(
            b88 > 110.0 && b88 < 160.0,
            "88 budget at 100 m ≈ 120-150 mm, got {b88:.0}"
        );
        assert!(b76 < b88, "the 76 mm must bring less budget than the 88");
        // Through TIMBER (clean pass): the 88 loses almost nothing.
        let t = Material::Timber.ap();
        assert!(
            (b88 - t.cost_mm) * (1.0 - t.disrupt) > 100.0,
            "a hut must not save a T-34"
        );
        // Through BRICK: the 88 still kills a T-34 flank (~48 mm eff)…
        let br = Material::Brick.ap();
        let eff88 = (b88 - br.cost_mm) * (1.0 - br.disrupt);
        assert!(
            eff88 > 48.0,
            "88 through brick must still hole a T-34 flank, got {eff88:.0}"
        );
        // …while the 76 through the same wall bounces off the Tiger's 80 mm flank.
        let eff76 = (b76 - br.cost_mm) * (1.0 - br.disrupt);
        assert!(
            eff76 < 80.0,
            "76 through brick must NOT hole the Tiger flank, got {eff76:.0}"
        );
        // Bunker concrete ends any budget.
        assert!(
            Material::Bunker.ap().cost_mm > b88 * 2.0,
            "no shell out-budgets a bunker"
        );
        // Budgets decay with range.
        assert!(
            budget_mm(&PZGR_39, 1000.0) < b88,
            "penetration must fall off with range"
        );
    }

    #[test]
    fn tiger_front_is_immune_flanks_are_not() {
        // Historical contract, now EMERGENT from the ballistics engine rather than hand-tabled: the
        // T-34's 76 mm cannot defeat the Tiger's frontal armor at combat range; the flanks can fall.
        use crate::ballistics::{self, ArmorScheme, Facing, Terminal};
        for &rng in &[200.0f32, 500.0] {
            let ev = ballistics::resolve(
                &ballistics::BR_350B,
                rng,
                ArmorScheme::TigerI.plate(Facing::HullFront),
                0.0,
                5,
            );
            assert_ne!(
                ev.terminal,
                Terminal::Penetration,
                "T-34 76 mm must not pen the Tiger front at {rng:.0} m ({:.0} vs {:.0} mm)",
                ev.pen_mm,
                ev.effective_mm
            );
        }
        // The 80 mm flank falls to the same shell up close.
        let side = ballistics::resolve(
            &ballistics::BR_350B,
            200.0,
            ArmorScheme::TigerI.plate(Facing::HullSide),
            5.0,
            5,
        );
        assert_eq!(
            side.terminal,
            Terminal::Penetration,
            "76 mm should hole the Tiger side at 200 m"
        );
    }

    #[test]
    fn su100_100mm_penetrates_the_tiger_front_where_a_t34_76mm_bounces() {
        // The whole point of the SU-100, now physics: the BR-412's pen exceeds the Tiger's frontal
        // effective thickness at combat range, where the BR-350B never reaches it.
        use crate::ballistics::{self, Facing, Terminal};
        let plate = ballistics::ArmorScheme::TigerI.plate(Facing::HullFront);
        for &rng in &[200.0f32, 400.0, 700.0, 1000.0] {
            let su = ballistics::resolve(enemy_shell(EnemyKind::Su100), rng, plate, 0.0, 9);
            let t34 = ballistics::resolve(enemy_shell(EnemyKind::T34), rng, plate, 0.0, 9);
            assert_eq!(
                su.terminal,
                Terminal::Penetration,
                "SU-100 must pen the Tiger front at {rng:.0} m ({:.0} vs {:.0} mm)",
                su.pen_mm,
                su.effective_mm
            );
            assert_ne!(
                t34.terminal,
                Terminal::Penetration,
                "T-34 must never pen the Tiger front at {rng:.0} m ({:.0} vs {:.0} mm)",
                t34.pen_mm,
                t34.effective_mm
            );
        }
        // Flanks stay lethal for BOTH kinds (the SU-100 doesn't make the flanks safer).
        let side = ballistics::ArmorScheme::TigerI.plate(Facing::HullSide);
        for kind in [EnemyKind::T34, EnemyKind::Su100] {
            let ev = ballistics::resolve(enemy_shell(kind), 300.0, side, 5.0, 11);
            assert_eq!(
                ev.terminal,
                Terminal::Penetration,
                "{kind:?} must hole the Tiger side at 300 m"
            );
        }
    }

    #[test]
    fn overwatch_su100s_present_at_long_standoff_and_the_band_is_sane() {
        // Exactly the two OUTSKIRTS flank pickets are SU-100s standing overwatch; the rest are T-34s.
        assert_eq!(SWARM_KINDS.len(), N_ENEMIES);
        let su_count = SWARM_KINDS
            .iter()
            .filter(|k| **k == EnemyKind::Su100)
            .count();
        assert_eq!(su_count, 2, "an overwatch PAIR of SU-100s");
        assert!(
            SWARM_KINDS[0] == EnemyKind::Su100 && SWARM_KINDS[1] == EnemyKind::Su100,
            "the SU-100s are the two flank pickets (indices 0,1)"
        );
        assert!(
            SWARM_KINDS[2..].iter().all(|k| *k == EnemyKind::T34),
            "everything else is a T-34"
        );
        // Every SU-100 musters at genuine long standoff from the player spawn (a sniper, not a picket
        // in the Tiger's lap) — well beyond the standoff band's minimum keep-range.
        let spawn = Vec2::new(PLAYER_SPAWN.0, PLAYER_SPAWN.1);
        for (i, k) in SWARM_KINDS.iter().enumerate() {
            if *k == EnemyKind::Su100 {
                let d = Vec2::new(SWARM[i].0, SWARM[i].1).distance(spawn);
                assert!(
                    d > SU100_STANDOFF_MAX,
                    "SU-100 #{i} opens at long range ({d:.0} m)"
                );
            }
        }
        // The standoff band is a real, ordered keep-range.
        assert!(SU100_STANDOFF_MIN > 0.0 && SU100_STANDOFF_MIN < SU100_STANDOFF_MAX);
    }

    #[test]
    fn spall_cone_prefers_the_holed_compartment_by_height() {
        // A LOW hull-side entry sprays the hull crew far more than the (higher) turret crew.
        let entry = Vec3::new(-1.4, 1.05, 1.4); // left front sponson, hull band
        let spall = (Vec3::new(0.0, 1.05, 0.5) - entry).normalize();
        let driver = spall_exposure(
            entry,
            spall,
            CrewRole::Driver.station(),
            0.35,
            SPALL_HALF_TAN,
            SPALL_REACH,
        );
        let cmd = spall_exposure(
            entry,
            spall,
            CrewRole::Commander.station(),
            0.35,
            SPALL_HALF_TAN,
            SPALL_REACH,
        );
        assert!(driver > 0.0, "hull crew must be in a hull pen's cone");
        assert!(
            driver > cmd,
            "a hull pen must favour hull crew over turret crew"
        );

        // A HIGH turret entry flips it: the turret crew is exposed far more than the (lower) driver.
        let tentry = Vec3::new(-0.9, 1.95, 0.4);
        let tspall = (Vec3::new(0.2, 1.95, 0.0) - tentry).normalize();
        let gunner = spall_exposure(
            tentry,
            tspall,
            CrewRole::Gunner.station(),
            0.35,
            SPALL_HALF_TAN,
            SPALL_REACH,
        );
        let tdriver = spall_exposure(
            tentry,
            tspall,
            CrewRole::Driver.station(),
            0.35,
            SPALL_HALF_TAN,
            SPALL_REACH,
        );
        assert!(gunner > 0.0);
        assert!(gunner > tdriver, "a turret pen must favour turret crew");
    }

    #[test]
    fn hull_side_pen_can_catch_a_body_in_the_hull_and_wreck_a_hull_module() {
        // SOLO CREW: the spall tests the PLAYER'S BODY at its actual position. A low front-side
        // pen must (over a seed sweep) catch a body posted at the driver's seat AND wreck a hull
        // module — and must NOT touch a body up in the turret (position is the defence).
        let entry = Vec3::new(-1.5, 1.05, 1.2);
        let spall = (Vec3::new(0.0, 1.05, 0.3) - entry).normalize();
        let mut driver_hits = 0u32;
        let mut gunner_hits = 0u32;
        let mut any_mod = false;
        for s in 0..80u32 {
            let mut modules = default_modules();
            let res = resolve_spall(
                entry,
                spall,
                Some(CrewRole::Driver.station()),
                &mut modules,
                s.wrapping_mul(2654435761).wrapping_add(1),
            );
            if res.hit_body {
                driver_hits += 1;
            }
            if res.hit_modules.iter().any(|m| {
                matches!(
                    m,
                    ModuleKind::Transmission
                        | ModuleKind::HullMg
                        | ModuleKind::Radio
                        | ModuleKind::LeftTrack
                        | ModuleKind::RightTrack
                )
            }) {
                any_mod = true;
            }
            let mut mods2 = default_modules();
            let res2 = resolve_spall(
                entry,
                spall,
                Some(CrewRole::Gunner.station()),
                &mut mods2,
                s.wrapping_mul(2654435761).wrapping_add(1),
            );
            if res2.hit_body {
                gunner_hits += 1;
            }
        }
        assert!(
            driver_hits > 0,
            "a hull-side pen should sometimes catch a body at the driver's seat"
        );
        assert!(
            any_mod,
            "a hull-side pen should sometimes wreck a hull module"
        );
        // Position is the defence: the same rounds catch a body at the driver's seat far more
        // often than one posted up in the turret (the cone can still splash upward occasionally).
        assert!(
            driver_hits > gunner_hits * 2,
            "position must matter: driver seat {driver_hits} vs turret {gunner_hits}"
        );
    }

    #[test]
    fn low_sponson_pen_can_detonate_the_ammunition() {
        // A low pen through the sponson can strike the ammunition rack → the catastrophic flag, with
        // the ammo rack reported knocked out. (In game this is an instant turret-pop kill.)
        let entry = Vec3::new(-1.8, 0.95, -0.2); // low, mid-hull side, into the sponson stowage
        let spall = (Vec3::new(0.0, 0.85, -0.1) - entry).normalize();
        let mut detonated = false;
        for s in 0..80u32 {
            let mut modules = default_modules();
            let res = resolve_spall(
                entry,
                spall,
                None,
                &mut modules,
                s.wrapping_mul(40507).wrapping_add(9),
            );
            if res.ammo {
                detonated = true;
                assert!(res.hit_modules.contains(&ModuleKind::AmmoRack));
            }
        }
        assert!(
            detonated,
            "a low sponson pen must be able to set off the ammunition"
        );
    }

    #[test]
    fn rear_bay_pen_can_start_a_fire() {
        // A rear/side pen into the engine/fuel bay lights a fire (which the cook-off timer then grows
        // toward the ammo).
        let entry = Vec3::new(-1.4, 1.05, -1.9);
        let spall = (Vec3::new(0.2, 1.0, -2.2) - entry).normalize();
        let mut lit = false;
        for s in 0..80u32 {
            let mut modules = default_modules();
            let res = resolve_spall(
                entry,
                spall,
                None,
                &mut modules,
                s.wrapping_mul(15013).wrapping_add(5),
            );
            if res.fire {
                lit = true;
                assert!(res
                    .hit_modules
                    .iter()
                    .any(|m| matches!(m, ModuleKind::Engine | ModuleKind::Fuel)));
            }
        }
        assert!(
            lit,
            "a rear/side pen into the engine/fuel bay must be able to start a fire"
        );
    }

    #[test]
    fn crew_and_modules_start_intact_and_the_model_is_complete() {
        // The coarse crew-COUNT is gone: it's 5 individuals + 12 modules, all intact to begin with.
        let crew = default_crew();
        let modules = default_modules();
        assert_eq!(crew.len(), 5);
        assert_eq!(modules.len(), 12);
        assert!(crew.iter().all(|c| c.ok));
        assert!(modules.iter().all(|m| m.functional));
        // Every role/kind is distinct (no accidental dupes in the interior layout).
        for (i, a) in CrewRole::ALL.iter().enumerate() {
            for b in &CrewRole::ALL[i + 1..] {
                assert_ne!(a, b);
            }
        }
        for (i, a) in ModuleKind::ALL.iter().enumerate() {
            for b in &ModuleKind::ALL[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn near_miss_puff_only_within_radius_and_scales_by_closeness() {
        // A round that snapped past WIDE of the radius kicks up no puff (far/wide rounds don't spam).
        assert!(near_miss_puff_scale(NEAR_MISS_PUFF_RADIUS + 0.5).is_none());
        assert!(near_miss_puff_scale(20.0).is_none());
        // On the hull → full-strength puff; at the radius edge → ~0; strictly monotonic between.
        let hull = near_miss_puff_scale(0.0).unwrap();
        let mid = near_miss_puff_scale(NEAR_MISS_PUFF_RADIUS * 0.5).unwrap();
        let edge = near_miss_puff_scale(NEAR_MISS_PUFF_RADIUS).unwrap();
        assert!(
            (hull - 1.0).abs() < 1e-6,
            "a round on the hull is the biggest puff"
        );
        assert!(
            edge < 1e-6,
            "a round at the radius edge is a vanishing puff"
        );
        assert!(
            hull > mid && mid > edge,
            "closer near-miss = bigger puff (deterministic scaling)"
        );
    }

    #[test]
    fn replay_shell_travels_from_shooter_to_entry() {
        let s = Vec3::new(-260.0, 2.0, 30.0);
        let e = Vec3::new(0.0, 1.5, 0.0);
        // Parked at the muzzle before the shell window, arrived at the entry after it.
        assert!(replay_shell_pos(s, e, 0.0).distance(s) < 1e-3);
        assert!(replay_shell_pos(s, e, 1.0).distance(e) < 1e-3);
        // Mid-window: strictly in between, and the flight fraction advances monotonically.
        let mid = REPLAY_SHELL_T0 + 0.5 * (REPLAY_SHELL_T1 - REPLAY_SHELL_T0);
        let pm = replay_shell_pos(s, e, mid);
        assert!(pm.distance(s) > 1.0 && pm.distance(e) > 1.0);
        assert!(replay_shell_frac(0.3) <= replay_shell_frac(0.5));
        assert!((replay_shell_frac(mid) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn swarm_opens_beyond_contact_with_incity_dug_deeper() {
        // CAMPAIGN / SKIRMISH OPENING GEOMETRY. The Tiger musters FAR out on the open steppe and the
        // whole swarm rings the city to the north, so at t=0 (stationary) NOT ONE enemy is inside the
        // stationary detection radius: the player opens with NO contact and must DRIVE IN to make it —
        // the fight starts at sniper range on the player's terms, never point-blank at spawn.
        assert_eq!(SWARM.len(), N_ENEMIES);
        // The stealth model groups enemies into contiguous PLATOON_SIZE=3 platoons, so the zone split
        // must land exactly on that boundary: platoon 0 wholly OUTSKIRTS, platoon 1 wholly IN-CITY.
        assert_eq!(
            N_ENEMIES % stealth::PLATOON_SIZE,
            0,
            "swarm must divide into whole platoons"
        );
        assert!((0..3).all(|i| SWARM[i].2 == SpawnZone::Outskirts));
        assert!((3..6).all(|i| SWARM[i].2 == SpawnZone::InCity));

        let spawn = Vec2::new(PLAYER_SPAWN.0, PLAYER_SPAWN.1);
        let city = CITY_CENTER;

        // NO initial contact: every enemy — the NEAREST included — starts beyond the stationary
        // detection radius, so a still Tiger accrues zero detection at spawn (see stealth::accrual_rate,
        // which returns 0 outside the radius) and first contact requires closing the range by driving.
        let nearest = SWARM
            .iter()
            .map(|(x, z, _)| Vec2::new(*x, *z).distance(spawn))
            .fold(f32::INFINITY, f32::min);
        assert!(
            nearest > stealth::RADIUS_STILL,
            "the nearest enemy at spawn ({nearest:.0} m) must sit beyond the {:.0} m stationary \
             detection radius — the player opens with NO contact",
            stealth::RADIUS_STILL
        );

        // Both zones ring the CITY (arrayed around it, not scattered), and the IN-CITY platoon is dug
        // DEEPER: the whole outskirts picket ring is nearer the spawn than any in-city tank, so a driving
        // player meets the long-range pickets FIRST and reaches the in-city group only by pushing into
        // the ruins.
        let mut farthest_outskirt = 0.0f32;
        let mut nearest_incity = f32::INFINITY;
        for (x, z, zone) in SWARM {
            let d_spawn = Vec2::new(x, z).distance(spawn);
            let d_city = Vec2::new(x, z).distance(city);
            match zone {
                SpawnZone::Outskirts => {
                    assert!(
                        d_city < 220.0,
                        "outskirts ring the city ({d_city:.0} m from centre)"
                    );
                    farthest_outskirt = farthest_outskirt.max(d_spawn);
                }
                SpawnZone::InCity => {
                    assert!(
                        d_city < 120.0,
                        "in-city tanks sit inside the ruins ({d_city:.0} m from centre)"
                    );
                    nearest_incity = nearest_incity.min(d_spawn);
                }
            }
        }
        assert!(
            nearest_incity > farthest_outskirt,
            "the in-city platoon must be dug deeper than the outskirts ring (nearest in-city \
             {nearest_incity:.0} m vs farthest outskirt {farthest_outskirt:.0} m from spawn)"
        );
    }

    #[test]
    fn wind_is_calmer_in_the_city_than_the_open_steppe() {
        // Sheltered inside the ruined city canyon; full cold-steppe wind out on the open ground.
        let city = wind_at(Vec3::new(CITY_CENTER.x, 0.0, CITY_CENTER.y));
        let open = wind_at(Vec3::new(CITY_CENTER.x + 400.0, 0.0, CITY_CENTER.y));
        assert!(
            open.length() > city.length(),
            "the open steppe must blow harder than the sheltered streets"
        );
        // Always a real, finite breeze anywhere on the field — never dead calm, never a gale.
        for p in [
            Vec3::ZERO,
            Vec3::new(-300.0, 0.0, 30.0),
            Vec3::new(0.0, 0.0, 120.0),
            Vec3::new(PLAYER_SPAWN.0, 0.0, PLAYER_SPAWN.1),
        ] {
            let w = wind_at(p);
            assert!(
                w.length() > 0.5 && w.length() < 20.0,
                "wind stays physically plausible ({})",
                w.length()
            );
        }
        // Deterministic: same position, same wind, every call.
        assert_eq!(
            wind_at(Vec3::new(10.0, 0.0, 20.0)),
            wind_at(Vec3::new(10.0, 0.0, 20.0))
        );
    }

    #[test]
    fn tank_weather_shelter_keeps_steel_clean_but_openings_live() {
        // A player physically outside gets the complete near-camera field.
        assert_eq!(tank_weather_shelter(Station::Commander, true, true), 0.0);

        // Closed armor and the protected fixed optics suppress both camera-near snow shells. Depth
        // still decides whether the far shells are visible through the small real aperture.
        for station in [
            Station::Driver,
            Station::Gunner,
            Station::Loader,
            Station::Commander,
        ] {
            assert_eq!(tank_weather_shelter(station, false, false), 1.0);
        }
        assert_eq!(tank_weather_shelter(Station::Gunner, false, true), 1.0);

        // Commander/loader hatch views are sheltered by the turret but retain mid/far snow through
        // the opening; this value deliberately sits strictly between sealed and fully exposed.
        let open_hatch = tank_weather_shelter(Station::Commander, false, true);
        assert!(open_hatch > 0.0 && open_hatch < 1.0);
        assert_eq!(
            tank_weather_shelter(Station::Loader, false, true),
            open_hatch
        );
    }

    #[test]
    fn station_switch_costs_a_beat_and_hull_is_farther() {
        // Moving within the turret is quicker than clambering to/from the hull driver's seat, and
        // every switch costs a real, positive beat (you can't act while moving between seats).
        assert!(switch_cost(Station::Gunner, Station::Loader) > 0.0);
        assert!(
            switch_cost(Station::Gunner, Station::Commander)
                < switch_cost(Station::Gunner, Station::Driver),
            "turret↔hull must cost more than a within-turret shuffle"
        );
        assert_eq!(
            switch_cost(Station::Driver, Station::Gunner),
            switch_cost(Station::Gunner, Station::Driver),
            "the cost is symmetric"
        );
        // Driver is the only hull seat; the other three share the turret.
        assert!(Station::Driver.in_hull());
        for s in [Station::Gunner, Station::Loader, Station::Commander] {
            assert!(!s.in_hull());
        }
    }

    #[test]
    fn manual_reload_ram_seats_assisted_fast_and_ko_slow_but_possible() {
        // Assisted (loader ok): ~3 mashes seat a round. Simulate the mash loop.
        let mut ram = 0.0f32;
        let mut n = 0;
        while ram < 1.0 {
            ram += ram_per_press(true);
            n += 1;
        }
        assert!(
            n <= 3,
            "an assisted loader seats a round in ~3 mashes (took {n})"
        );

        // One-handed (loader KO'd): SLOWER — more mashes — but never impossible.
        let mut ram = 0.0f32;
        let mut n = 0;
        while ram < 1.0 {
            ram += ram_per_press(false);
            n += 1;
        }
        assert!(
            n >= 6 && n <= 9,
            "a KO'd loader is slower (~7-8 mashes), got {n}"
        );
        assert!(
            ram_per_press(true) > ram_per_press(false),
            "assisted ram must be faster per press"
        );
        // A round that stops being mashed bleeds back out (decay is a real, positive rate).
        assert!(RAM_DECAY > 0.0);
    }

    #[test]
    fn extinguisher_wins_the_cookoff_race() {
        // Spraying must out-pace the fire's own growth (otherwise you could never win).
        assert!(
            EXT_RATE > FIRE_GROWTH,
            "the extinguisher must knock the fire down faster than it grows"
        );
        // From a fully-established fire, held spray knocks it below the ammo threshold BEFORE the
        // 5 s cook-off would elapse — a genuine but winnable race (mirrors update_fire's guard: the
        // countdown is bought back the instant `fire` drops under FIRE_AMMO_THRESH).
        let dt = 1.0 / 60.0;
        let mut fire = FIRE_MAX;
        let mut t = 0.0f32;
        // Model the EXT_GRAB unclip delay before foam flows.
        t += EXT_GRAB;
        while fire >= FIRE_AMMO_THRESH {
            fire = (fire - EXT_RATE * dt).max(0.0);
            t += dt;
        }
        assert!(
            t < FIRE_TO_AMMO,
            "a mid-fire knockdown must beat the cook-off (took {t:.2}s of {FIRE_TO_AMMO}s)"
        );
        // But it's TIGHT — you can't dawdle: the knockdown itself burns real seconds of exposure.
        assert!(
            t > 1.0,
            "the knockdown is not instant — it costs exposed seconds"
        );
    }

    #[test]
    fn repair_table_is_historically_sane() {
        use ModuleKind::*;
        // Field-fixable: tracks/transmission (slow, exposed), jams/stoppages/radio (quick).
        for k in [
            LeftTrack,
            RightTrack,
            Transmission,
            TurretTraverse,
            Radio,
            CoaxMg,
            HullMg,
        ] {
            assert!(repair_time(k).is_some(), "{k:?} should be a field-fix");
        }
        // Battle-destroyed / catastrophic — never a field-fix.
        for k in [MainGun, GunnerSight, Engine, Fuel, AmmoRack] {
            assert!(
                repair_time(k).is_none(),
                "{k:?} must NOT be repairable in the field"
            );
        }
        // A thrown track is the long, exposed job; clearing an MG stoppage is quick.
        assert!(repair_time(LeftTrack).unwrap() > repair_time(CoaxMg).unwrap());
    }

    #[test]
    fn replay_camera_pushes_from_shooter_toward_entry() {
        let s = Vec3::new(-260.0, 2.0, 30.0);
        let e = Vec3::new(0.0, 1.5, 0.0);
        let sd = (e - s).normalize();
        let (eye0, _) = replay_camera(s, e, sd, 0.0);
        let (eye1, _) = replay_camera(s, e, sd, 1.0);
        // Opens near the shooter, closes tight on the holed Tiger (the entry point).
        assert!(
            eye0.distance(s) < eye0.distance(e),
            "replay opens near the shooter"
        );
        assert!(
            eye1.distance(e) < eye1.distance(s),
            "replay closes on the entry"
        );
        // And it dollies inward, not outward, across the phase.
        assert!(
            eye1.distance(e) < eye0.distance(e),
            "the camera pushes in over the replay"
        );
    }

    #[test]
    fn interior_walk_clamps_to_floor_and_breech() {
        use crate::interior_rig as rig;
        // Stepping past the floor edge clamps inside the footprint (with body padding).
        let from = Vec3::new(0.5, 0.0, 0.0);
        let p = super::clamp_interior_walk(from, Vec3::new(5.0, 0.0, 5.0));
        assert!(
            p.x <= rig::FLOOR_MAX.x && p.z <= rig::FLOOR_MAX.z,
            "clamped to the floor: {p}"
        );
        // Walking straight at the breech from the loader's side stops or slides — never enters.
        let from = Vec3::new(0.6, 0.0, 0.3);
        let to = Vec3::new(0.0, 0.0, 0.3); // dead centre of the breech box
        let p = super::clamp_interior_walk(from, to);
        let pad = 0.15;
        let inside = p.x > rig::BREECH_MIN.x - pad
            && p.x < rig::BREECH_MAX.x + pad
            && p.z > rig::BREECH_MIN.z - pad
            && p.z < rig::BREECH_MAX.z + pad;
        assert!(!inside, "the breech blocks the body: {p}");
    }
}
