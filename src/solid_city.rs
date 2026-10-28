//! The solid city in the game loop (2026-09-29): the player Tiger rests on and meets the district's
//! building solids and rubble, soldiers stand on rubble, and shells lose their penetration in it.
//!
//! - Tank ground contact (`tank_support`): every frame the hull settles on the highest supports
//!   under its tracks: the terrain, the rubble within a step of them, and every building module's
//!   solid its tracks can carry on (floors, plinths, foundations, external steps, stairs, wall
//!   stubs): a track takes up a new support only at its leading end, within the Tiger's step, or
//!   where it already lies (`tank_samples`). A move is refused where the Tiger's documented limits
//!   say it must be (a step over 0.76 m, a gradient over 35°, a crest reaching the 0.47 m belly),
//!   at the point where its tracks meet the obstacle. A building module that stops it is crushed
//!   through the same crush table and destruction path walls use (`Material::crush`,
//!   `damage_structure`): timber bursts at speed, brick grinds, concrete and stone crawl-grind;
//!   a masonry or concrete floor hanging over the tracks, terrain and rubble cannot be crushed and
//!   stop it. A suspended floor or stair the Tiger comes to rest on fails under it: a 57 t tank on
//!   5 m² of track loads a floor at about 11 t/m², where residential floors of the period were
//!   designed for a live load of the order of 0.2 t/m² [I].
//! - Rubble (`building_rubble`, `BuildingSet::settle_rubble`): a failed module's mound is laid
//!   where it fell and its instance moved there; its sight occluder stands on its building's ground
//!   up to the mound's crest region.
//! - Feet stand on rubble within a 0.45 m step (the buildings' step-up), and shells lose
//!   penetration in rubble by the thickness they cross.

use super::*;
use crate::building_blueprint::ModuleKind as BuildingModuleKind;
use crate::tank_support::{self, Line, Refusal, RestPlane, Settled, SupportSample};

#[cfg(test)]
thread_local! {
    /// Print every obstacle sample (exploration tests).
    pub(super) static DEBUG_SAMPLES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Support source ids: a destruction record's index, the terrain, or a rubble mound (the top bit
/// set, with the building instance and the module, 0xFFFF for a burned-floor layer).
pub(super) const SOURCE_TERRAIN: u32 = u32::MAX;
const RUBBLE_FLAG: u32 = 1 << 31;
const LAYER_MODULE: u32 = 0xFFFF;

fn rubble_source(instance: usize, module: Option<usize>) -> u32 {
    RUBBLE_FLAG | ((instance as u32 & 0x7FFF) << 16) | module.map_or(LAYER_MODULE, |m| (m as u32).min(LAYER_MODULE - 1))
}

/// The building instance and module (`None`: a burned-floor layer) of a rubble source.
fn rubble_of(source: u32) -> Option<(usize, Option<usize>)> {
    if source == SOURCE_TERRAIN || source & RUBBLE_FLAG == 0 {
        return None;
    }
    let module = source & 0xFFFF;
    Some((((source >> 16) & 0x7FFF) as usize, (module != LAYER_MODULE).then_some(module as usize)))
}

/// A destruction record's index, when the source is one.
fn record_of(source: u32) -> Option<usize> {
    (source & RUBBLE_FLAG == 0).then_some(source as usize)
}

/// The hull centre stops this far inside the last row of the terrain grid: the bow's contact disc
/// (3.2 m ahead, 1.4 m radius) stays on drawn ground. Beyond the grid the height query only
/// repeats its edge row, an undrawn surface with nothing on it.
pub(super) const TERRAIN_EDGE_CLEARANCE_M: f32 = 4.6;

/// Where a vehicle step that would leave the terrain grid ends instead: on the limit, or `None`
/// when the step stays inside (or began outside, e.g. a staged fixture, which is left alone).
pub(super) fn terrain_edge_step(hf: &Heightfield, previous: Vec2, requested: Vec2) -> Option<Vec2> {
    let limit = (hf.res.saturating_sub(1)) as f32 * 0.5 * hf.spacing - TERRAIN_EDGE_CLEARANCE_M;
    if !(limit > 0.0) || !requested.is_finite() || previous.abs().max_element() > limit {
        return None;
    }
    (requested.abs().max_element() > limit)
        .then(|| requested.clamp(Vec2::splat(-limit), Vec2::splat(limit)))
}

impl Game {
    /// The player Tiger stops at the edge of the modelled ground as at an unyielding obstacle.
    pub(super) fn constrain_player_to_terrain(&mut self, previous: Vec3) {
        let requested = Vec2::new(self.player.pos.x, self.player.pos.z);
        if let Some(resolved) = terrain_edge_step(&self.hf, Vec2::new(previous.x, previous.z), requested) {
            self.player.pos.x = resolved.x;
            self.player.pos.z = resolved.y;
            self.player.speed = 0.0;
        }
    }
}

/// Radius around the hull origin whose building solids a settle considers (the hull's 3.6 m half
/// length plus a frame's travel).
const TANK_SOLID_REACH_M: f32 = 5.5;
/// Bisection steps that find where a refused move meets its obstacle (0.1 % of the move).
const CONTACT_BISECTIONS: usize = 10;
/// Farther than a frame's travel (12 m/s for 0.1 s) plus a vehicle contact's push: the hull was
/// placed, not driven, since its last settle.
const TANK_REST_CONTINUITY_M: f32 = 2.0;
/// How far above a track's bottom a building solid under an already-covered stretch of track may
/// stand and still count as carrying it (the tracks' own sag and the contact scatter of a settle).
const TRACK_CONTACT_TOLERANCE_M: f32 = 0.05;

/// Loose brick rubble's resistance to a kinetic round per metre of path, in mm of the pen budget:
/// the brick wall transit's 35 mm (`Material::Brick`, DESTRUCTION.md §2) is for about 0.5 m of
/// solid brickwork (the grammar's 0.51-0.64 m walls, 0.4 m kit walls), 70 mm per metre, scaled by
/// the density of loose rubble (solid ÷ the 1.5 bulking of P G17): 47 mm/m. An engineering
/// estimate [I] from the game's own brick table, by areal density.
pub(super) const RUBBLE_AP_COST_MM_PER_M: f32 = 47.0;
/// Yaw disruption and angular kick per metre of rubble crossed, from the brick transit's 0.30 and
/// 15 mrad per wall, by the same scaling [I].
pub(super) const RUBBLE_DISRUPT_PER_M: f32 = 0.4;
pub(super) const RUBBLE_DEFLECT_MRAD_PER_M: f32 = 20.0;
/// March step of a round through rubble.
const RUBBLE_MARCH_M: f32 = 0.1;

/// A soldier steps onto rubble at most this far above his feet in one move (the buildings'
/// `MAX_STEP_UP_M`), and keeps his footing going down it by this much per frame.
pub(super) const FOOT_RUBBLE_STEP_M: f32 = 0.45;
const FOOT_RUBBLE_STEP_DOWN_M: f32 = 0.35;

/// Material class of a building module for the crush gate and shell transit: its kit asset's
/// material. Concrete (the grammar's slabs, landings, steps and concrete stairs) crawl-grinds like
/// stone under a Tiger; sheet iron and the timber of wooden houses burst at speed; the one steel
/// beam asset is stone-strong. The featured house's frame-only window module forms the brick piers
/// and sill of its wall (a kit note since F1), so it grinds as brick.
pub(super) fn building_module_material(asset_id: &str) -> Material {
    use crate::building_kit::KitMaterial as K;
    if asset_id == "pizza.structure.window_frame" {
        return Material::Brick;
    }
    match crate::building_kit::asset(asset_id).map(|asset| asset.material) {
        Some(K::Brick) | None => Material::Brick,
        Some(K::Stone) | Some(K::Concrete) => Material::Stone,
        Some(K::Timber) => Material::Timber,
        Some(K::Metal) if asset_id == "urban.metal.beam" => Material::Stone,
        Some(K::Metal) => Material::Timber,
        // A snow deposit has no solids and no destruction record; if one is ever asked, it gives
        // way like timber.
        Some(K::Snow) => Material::Timber,
    }
}

impl Game {
    /// What a destruction record offers the Tiger's push (`tank_push`), from its material, its
    /// size and how it stands: a building module's walls and plinths are held by the building and
    /// are punched through, its slabs, landings and stairs slide off their bearings when met
    /// edge-on, its front steps are loose blocks; a free-standing wall or pier (a manifest yard
    /// wall, a gate pier, a stub) bends over about its base; timber and sheet iron break; a thin
    /// pier (under 0.3 m) is a timber pole; bunker concrete holds.
    pub(super) fn push_resistance(&self, record: usize) -> crate::tank_push::Resistance {
        use crate::tank_push as push;
        let d = self.destructibles[record];
        let ext = (d.max - d.min).max(Vec3::splat(0.01));
        let (t, w, h) = (ext.x.min(ext.z), ext.x.max(ext.z), ext.y);
        if d.class == Material::Bunker {
            return push::Resistance::UNYIELDING;
        }
        if d.class == Material::Timber {
            return push::TIMBER;
        }
        let Some((instance, module)) = self.buildings.owner(record) else {
            if t < 0.3 && h > 2.0 * w {
                return push::TIMBER; // a pole
            }
            return push::free_standing_brick(t, w, h);
        };
        let runtime = self.buildings.get(instance);
        let placement = &runtime.layout().placements()[module];
        let concrete = d.class == Material::Stone;
        // What the bow meets of the piece: its width (up to the bow's) and the height of it inside
        // the bow's face, from the hull's base up; a piece met higher (by the upper hull) at least
        // its lowest 0.3 m.
        let base = self.player.pos.y;
        let h_contact = (d.max.y.min(base + push::BOW_HEIGHT_M) - d.min.y.max(base)).max(0.3_f32.min(h));
        // A piece that still carries another (the support graph of its building) is held by the
        // building; one that carries nothing (a burned house's footing, a stub, a pier left
        // standing) only bends over about its base.
        let id = &placement.module_id;
        let carries = runtime.blueprint().support_edges.iter().any(|edge| {
            edge.supporter == *id
                && runtime
                    .layout()
                    .placements()
                    .iter()
                    .position(|p| p.module_id == edge.dependent)
                    .is_some_and(|dependent| {
                        runtime.module_intact(dependent)
                            && runtime.layout().placements()[dependent].kind.is_structural()
                    })
        });
        match placement.kind {
            BuildingModuleKind::Slab | BuildingModuleKind::Stair => push::slab_off_bearing(
                ext.x * ext.y * ext.z,
                if concrete { push::CONCRETE_DENSITY } else { push::BRICK_DENSITY },
            ),
            BuildingModuleKind::Foundation if id.as_str().contains(".steps") => push::loose_block(t, w, h),
            _ if !carries => push::free_standing_brick(t, w, h),
            _ if concrete => push::restrained_concrete(t, w, h_contact),
            _ => push::restrained_brick(t, w, h_contact),
        }
    }

    /// Press the Tiger's bow on a destruction record for one frame (`tank_push::press`): its
    /// speed pays the obstacle's resistance, the record takes damage in proportion to the work it
    /// absorbed (shell damage already taken counts as work done), and it falls when the work is
    /// complete. Returns true when it gave way.
    pub(super) fn press_on_structure(&mut self, record: usize, pitch_along: f32, dt: f32, r: &mut Renderer) -> bool {
        let d = self.destructibles[record];
        if !d.alive {
            return true;
        }
        let resist = self.push_resistance(record);
        let absorbed = if resist.work_j.is_finite() {
            resist.work_j * (1.0 - d.hp / d.max_hp.max(1.0e-6)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let step = crate::tank_push::press(self.player.speed, pitch_along, resist, absorbed, dt);
        self.crush_rumble=self.crush_rumble.max(crate::tank_push::collision_rumble(self.player.speed,step.speed));
        self.player.speed = step.speed;
        if step.gave_way {
            self.damage_structure(record, d.max_hp.max(d.hp).max(1.0) * 2.0, r);
            return true;
        }
        if step.work_j > 0.0 && resist.work_j.is_finite() {
            self.damage_structure(record, d.max_hp * step.work_j / resist.work_j.max(1.0), r);
        }
        !self.destructibles[record].alive
    }
}

/// One building solid near the tank: a live destruction record of a building module.
#[derive(Clone, Copy, Debug)]
pub(super) struct TankSolid {
    record: usize,
    min: Vec3,
    max: Vec3,
    /// A wall, pillar, partition or door frame: the bow's crush gate owns it once it stands
    /// higher than a step.
    barrier: bool,
}

/// The player hull's last settle: its pose, the plane its track bottoms lie on as it stands, and
/// the rest plane its supports gave it (which the pose follows at a limited rate).
#[derive(Clone, Copy, Debug)]
pub(super) struct TankRest {
    pub pose: TankPose,
    pub plane: RestPlane,
    pub settled: RestPlane,
}

/// Where the hull stands: position of the hull origin (ground contact under its centre), heading,
/// nose-up pitch and left-up roll.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TankPose {
    pub pos: Vec3,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
}

impl TankPose {
    fn lerp(self, to: TankPose, t: f32) -> TankPose {
        TankPose {
            pos: self.pos.lerp(to.pos, t),
            heading: self.heading + (to.heading - self.heading) * t,
            pitch: self.pitch,
            roll: self.roll,
        }
    }
}

/// What the player Tiger's tracks met this frame, for films, tests and the build log.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TankGroundState {
    pub pitch_deg: f32,
    pub roll_deg: f32,
    /// Height of the hull origin above the terrain under it.
    pub above_terrain_m: f32,
    /// What carries the tracks: "terrain", "rubble", "building".
    pub carried_by: &'static str,
    /// The limit that refused this frame's move: "step", "overhead", "gradient", "belly" or "".
    pub refused: &'static str,
    pub refused_value: f32,
    /// What the refused move met: "terrain", "rubble" or the building module's kind.
    pub obstacle: String,
    /// A building record the tracks are crushing or that gave way under the tank.
    pub crushing: Option<usize>,
    pub gave_way: u32,
    pub falling: bool,
    /// Rubble came to lie higher under the hull than driving could lift it (it fell around a
    /// standing tank): the hull stays where it was, embedded.
    pub embedded: bool,
    /// Largest step, gradient (degrees) and highest rubble climbed so far.
    pub max_climbed_step_m: f32,
    pub max_pitch_deg: f32,
    pub max_rubble_above_terrain_m: f32,
    /// Share of the weight on the belly this frame (0 on the tracks alone), and the largest so far.
    pub belly_share: f32,
    pub max_belly_share: f32,
}

impl Game {
    /// The player hull's pose.
    pub(super) fn player_pose(&self) -> TankPose {
        TankPose {
            pos: self.player.pos,
            heading: self.player.heading,
            pitch: self.player.pitch,
            roll: self.player.roll,
        }
    }

    /// The live building solids around the hull that its tracks can rest on or meet. Buried
    /// solids (a sealed cellar's) and roofs are out of reach; legacy walls stay with the bow's
    /// crush gate.
    pub(super) fn tank_solids(&self, center: Vec3) -> Vec<TankSolid> {
        let mut out = Vec::new();
        for record in self.structures_near(center, TANK_SOLID_REACH_M) {
            let d = &self.destructibles[record];
            if !d.alive {
                continue;
            }
            let Some((instance, module)) = self.buildings.owner(record) else { continue };
            let kind = self.buildings.get(instance).layout().placements()[module].kind;
            if kind == BuildingModuleKind::Roof || buried_below_ground(&self.hf, d.min, d.max) {
                continue;
            }
            out.push(TankSolid {
                record,
                min: d.min,
                max: d.max,
                barrier: self.buildings.destructible_is_crush_barrier(record),
            });
        }
        out
    }

    /// What a track finds at a world XZ point: the highest support (terrain, rubble, a building
    /// solid whose top is at most `admit`) and, apart from it, the highest building solid that
    /// rises above `admit` within the hull's height `top` over the track's contact `ref_y` (an
    /// obstacle: it can stop or be crushed, never lift the hull). A standing wall more than a step
    /// above the contact ahead of the tracks (`covered` false) is the bow's crush gate's; one
    /// under a stretch of track the hull already covered meets the tracks themselves.
    fn tank_support_at(
        &self,
        xz: Vec2,
        (ref_y, admit, covered): (f32, f32, bool),
        top: f32,
        solids: &[TankSolid],
        rubble: &crate::building_set::LocalRubble<'_>,
    ) -> (f32, u32, Option<tank_support::Obstacle>) {
        let mut height = if self.campaign_channel {
            act2_bridge_deck_height(&self.hf, xz).unwrap_or_else(|| self.hf.height_at(xz.x, xz.y))
        } else {
            self.hf.height_at(xz.x, xz.y)
        };
        // The winter snowpack compacts under the loaded track
        // (`vehicle_ground::WINTER_TRACK_SINKAGE_M`, 0.30 m of 0.40 m settled snow). This changes
        // the physical support pose, so hull, collision, ports and visuals agree. Cellar floors
        // and the authored bridge remain hard supports.
        if self.act.season == crate::world::Season::Winter
            && self.hf.openings.at(xz.x,xz.y).is_none()
            && !(self.campaign_channel && act2_bridge_deck_height(&self.hf,xz).is_some()) {
            height -= crate::vehicle_ground::WINTER_TRACK_SINKAGE_M;
        }
        let mut source = SOURCE_TERRAIN;
        if let Some(hit) = rubble.top_at_from(xz, ref_y + tank_support::TIGER_STEP_M) {
            if hit.height > height {
                height = hit.height;
                source = rubble_source(hit.instance, hit.module);
            }
        }
        let mut obstacle: Option<tank_support::Obstacle> = None;
        for solid in solids {
            if xz.x < solid.min.x || xz.x > solid.max.x || xz.y < solid.min.z || xz.y > solid.max.z {
                continue;
            }
            if solid.min.y >= ref_y + top {
                continue; // above the hull (or turret) here: an upper floor, a balcony
            }
            if solid.max.y > admit {
                let above = solid.max.y - ref_y;
                if solid.barrier && above > tank_support::TIGER_STEP_M && !covered {
                    continue; // a standing wall ahead: the bow's crush gate meets it
                }
                let found = tank_support::Obstacle {
                    height: above,
                    source: solid.record as u32,
                    overhead: solid.min.y > ref_y + tank_support::TIGER_TRACK_TOP_M,
                };
                if obstacle.map_or(true, |best| (!found.overhead, found.height) > (!best.overhead, best.height)) {
                    obstacle = Some(found);
                }
                continue;
            }
            if solid.max.y > height {
                height = solid.max.y;
                source = solid.record as u32;
            }
        }
        (height, source, obstacle)
    }

    /// Support samples under both tracks (three points across each track's width, the highest
    /// taken) and the belly, at the stations of `tank_support`, for a hull at `pose`.
    ///
    /// Which building solids can carry a track depends on where the tracks were (`previous`, the
    /// last settled pose and its rest plane): under a stretch of track that already lay over the
    /// point, only a solid at or below that track's bottom (within [`TRACK_CONTACT_TOLERANCE_M`])
    /// carries it, because a rigid track takes up a new, higher support only at its leading end,
    /// where the rising run meets it; there a solid up to a step (0.76 m) above the ground plane
    /// under the hull is climbed, and the step rule (`tank_support::largest_step`) judges it
    /// against the ground at its foot. Anything higher is an obstacle. A standing tank therefore
    /// never climbs; before this rule a pitched hull admitted a wall top up to a step above its
    /// raised nose, pitched further onto it, and rose up the wall frame by frame (42.7° on the
    /// facade of `urban_intro.c5r2.w01`, 2026-09-29). Terrain and rubble always carry.
    fn tank_samples(
        &self,
        pose: TankPose,
        solids: &[TankSolid],
        rubble: &crate::building_set::LocalRubble<'_>,
        previous: Option<&TankRest>,
    ) -> Vec<SupportSample> {
        let rotation = glam::Mat3::from_rotation_y(pose.heading)
            * glam::Mat3::from_rotation_x(-pose.pitch)
            * glam::Mat3::from_rotation_z(pose.roll);
        let cos_p = pose.pitch.cos();
        let stations = tank_support::track_stations();
        let belly = tank_support::belly_stations();
        let half = tank_support::TIGER_TRACK_HALF_WIDTH;
        // Where a world point lay under the previous pose: (contact plane height there, the
        // admission height for building solids).
        let admission = |world: Vec3, line: Line| -> (f32, f32, bool) {
            let Some(rest) = previous else {
                return (pose.pos.y, pose.pos.y + tank_support::TIGER_STEP_M, false);
            };
            let prev = rest.pose;
            let d = Vec2::new(world.x - prev.pos.x, world.z - prev.pos.z);
            let (sin, cos) = prev.heading.sin_cos();
            let along = d.dot(Vec2::new(sin, cos));
            let x = d.dot(Vec2::new(cos, -sin));
            // The hull as it stands, and the rest its supports gave it (the pose turns onto a
            // new rest at a limited rate, and falls under gravity): a support under either
            // carries the track.
            let ref_y = rest.plane.height(along, x);
            let cos_tilt = prev.pitch.cos() * prev.roll.cos();
            let s = along / prev.pitch.cos().max(0.2);
            let covered = line != Line::Belly && (x - line.x()).abs() <= half + 0.05;
            match tank_support::track_bottom(s).filter(|_| covered) {
                Some(bottom) => {
                    // The rest carries each track on its centre line (the support is the
                    // highest point across the track's width), so the track bottom is read there.
                    let x = line.x();
                    let rest_y = rest.plane.height(along, x).max(rest.settled.height(along, x));
                    (ref_y, rest_y + bottom * cos_tilt + TRACK_CONTACT_TOLERANCE_M, true)
                }
                None => {
                    let rest_y = ref_y.max(rest.settled.height(along, x));
                    (ref_y, rest_y + tank_support::TIGER_STEP_M, false)
                }
            }
        };
        let mut out = Vec::with_capacity(stations.len() * 2 + belly.len());
        let sample = |line: Line, s: f32, offsets: &[f32]| {
            let along = s * cos_p;
            let mut best = (f32::NEG_INFINITY, SOURCE_TERRAIN);
            let mut obstacle: Option<tank_support::Obstacle> = None;
            for &dx in offsets {
                let x = line.x() + dx;
                let world = pose.pos + rotation * Vec3::new(x, 0.0, s);
                let top = tank_support::top_at(s, x);
                let adm = admission(world, line);
                let (height, source, blocking) =
                    self.tank_support_at(Vec2::new(world.x, world.z), adm, top, solids, rubble);
                #[cfg(test)]
                if DEBUG_SAMPLES.with(|d| d.get()) {
                    if let Some(o) = blocking {
                        println!(
                            "    dbg {line:?} s {s:5.2} dx {dx:+.2} world ({:.2},{:.2}) ref {:.3} admit {:.3} covered {} support {:.3} obst +{:.3} src {}",
                            world.x, world.z, adm.0, adm.1, adm.2, height, o.height, o.source
                        );
                    }
                }
                if height > best.0 {
                    best = (height, source);
                }
                if let Some(found) = blocking {
                    if obstacle.map_or(true, |o| (!found.overhead, found.height) > (!o.overhead, o.height)) {
                        obstacle = Some(found);
                    }
                }
            }
            SupportSample { line, s, along, height: best.0, source: best.1, obstacle }
        };
        // Five points across each 0.78 m track (0.195 m apart) at stations 0.1 m apart, nine
        // across the belly (0.2 m apart) at stations 0.2 m apart: a peak between samples of a
        // surface whose slope is at most s stands at most about 0.1 s above them (8 cm on a rubble
        // flank at its steepest; building boxes' faces are seen within 0.1 m of travel).
        let track = [-half, -0.5 * half, 0.0, 0.5 * half, half];
        let belly_x = [-0.8, -0.6, -0.4, -0.2, 0.0, 0.2, 0.4, 0.6, 0.8];
        for &s in &stations {
            for line in [Line::LeftTrack, Line::RightTrack] {
                out.push(sample(line, s, &track));
            }
        }
        for &s in &belly {
            out.push(sample(Line::Belly, s, &belly_x));
        }
        out
    }

    fn settle_tank_pose(
        &self,
        pose: TankPose,
        solids: &[TankSolid],
        rubble: &crate::building_set::LocalRubble<'_>,
        forward: bool,
        previous: Option<&TankRest>,
    ) -> Option<Settled> {
        let samples = self.tank_samples(pose, solids, rubble, previous);
        tank_support::settle(&samples, forward, pose.pitch.cos() * pose.roll.cos(), 0.0)
    }

    /// Settle the player Tiger after its horizontal move from `before`: the rest pose on whatever
    /// its tracks meet, or, where a limit refuses the move, the pose where they meet the obstacle,
    /// which a crushable building module then takes crush damage at. `settle_loads` lets the
    /// tank's weight fail the suspended floors it rests on (off for the quiet re-settle after a
    /// vehicle contact, which has no renderer).
    pub(super) fn settle_player_tank(&mut self, before: TankPose, dt: f32, r: Option<&mut Renderer>) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        // The move the drive made (horizontal: on a pitched hull it covers `cos pitch` of it).
        let mut after = self.player_pose();
        // A zero-time call settles a contact projection that is already in world XZ.
        // Applying the drive's slope correction again would undo separation every frame.
        let travel = Vec2::new(after.pos.x - before.pos.x, after.pos.z - before.pos.z)
            * if dt > 0.0 { before.pitch.cos() } else { 1.0 };
        after.pos = Vec3::new(before.pos.x + travel.x, before.pos.y, before.pos.z + travel.y);
        after.pitch = before.pitch;
        after.roll = before.roll;
        let moved = travel.length_squared() > 1.0e-10 || (after.heading - before.heading).abs() > 1.0e-7;
        let forward = self.player.speed >= 0.0;
        let solids = self.tank_solids(after.pos);
        let reach = Vec2::splat(TANK_SOLID_REACH_M);
        let xz = Vec2::new(after.pos.x, after.pos.z);
        let rubble = self.buildings.rubble_near(xz - reach, xz + reach);
        // The last settle describes where the tracks were only if the hull is still there: a
        // placement (a spawn, a respawn, a fixture) starts the tracks afresh.
        let reference = self.tank_rest.filter(|rest| {
            let gap = Vec2::new(rest.pose.pos.x - before.pos.x, rest.pose.pos.z - before.pos.z);
            gap.length() <= TANK_REST_CONTINUITY_M && (rest.pose.pos.y - before.pos.y).abs() <= TANK_REST_CONTINUITY_M
        });
        let settled_before = self.settle_tank_pose(before, &solids, &rubble, forward, reference.as_ref());
        let Some(settled_after) = self.settle_tank_pose(after, &solids, &rubble, forward, reference.as_ref()) else {
            return;
        };
        let refusal = moved
            .then(|| tank_support::refusal(settled_before.as_ref(), &settled_after, forward))
            .flatten();
        let (pose, settled) = match refusal {
            None => (after, settled_after),
            Some(_) => {
                // Where the tracks meet the obstacle: the furthest fraction of the move allowed.
                let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
                let mut best = settled_before.clone();
                for _ in 0..CONTACT_BISECTIONS {
                    let mid = 0.5 * (lo + hi);
                    let trial = before.lerp(after, mid);
                    match self.settle_tank_pose(trial, &solids, &rubble, forward, reference.as_ref()) {
                        Some(s) if tank_support::refusal(settled_before.as_ref(), &s, forward).is_none() => {
                            lo = mid;
                            best = Some(s);
                        }
                        _ => hi = mid,
                    }
                }
                let pose = before.lerp(after, lo);
                match best {
                    Some(s) => (pose, s),
                    None => (before, settled_after),
                }
            }
        };
        drop(rubble);
        self.player.pos.x = pose.pos.x;
        self.player.pos.z = pose.pos.z;
        self.player.heading = pose.heading;

        // Height: the tracks lift the hull onto what they climb, no faster than driving up a
        // steep face lets them (rubble that lands around a standing tank does not lift it: the
        // hull stays embedded in it); where the supports drop away faster than the tank can drive
        // down a 60° slope, it falls under gravity. A hull just placed (no continuous last
        // settle: a spawn, a fixture) is set on its rest.
        let placed = reference.is_none();
        let mut rest_y = settled.plane.y0;
        let max_rise = (self.player.speed.abs() * 2.0 + 0.1) * dt + 0.002;
        let embedded = !placed && rest_y > before.pos.y + max_rise;
        if embedded {
            rest_y = before.pos.y + max_rise;
        }
        let follow = (self.player.speed.abs() * 1.8 + 0.5) * dt + 0.02;
        let falling = !placed && rest_y < before.pos.y - follow;
        if falling {
            self.tank_fall_vy -= 9.81 * dt;
            self.player.pos.y = (before.pos.y + self.tank_fall_vy * dt).max(rest_y);
            if self.player.pos.y <= rest_y + 1.0e-4 {
                self.tank_fall_vy = 0.0;
            }
        } else {
            self.player.pos.y = rest_y;
            self.tank_fall_vy = 0.0;
        }
        // The hull turns onto its new supports at most as fast as it tips over an edge.
        let max_turn = if placed { f32::INFINITY } else { 1.5 * dt + 1.0e-4 };
        self.player.pitch = before.pitch + (settled.plane.pitch() - before.pitch).clamp(-max_turn, max_turn);
        self.player.roll = before.roll + (settled.plane.slope_x.atan() - before.roll).clamp(-max_turn, max_turn);
        self.tank_rest = Some(TankRest {
            pose: self.player_pose(),
            plane: RestPlane {
                y0: self.player.pos.y,
                slope_s: self.player.pitch.tan(),
                slope_x: self.player.roll.tan(),
            },
            settled: settled.plane,
        });
        self.tank_ground.embedded = embedded;

        // Grade resistance and the selected gear are integrated by the authoritative player
        // powertrain. A steady-state speed cap here would discard climbing momentum instantly.

        // Telemetry.
        let terrain = self.hf.height_at(self.player.pos.x, self.player.pos.z);
        let state = &mut self.tank_ground;
        state.pitch_deg = self.player.pitch.to_degrees();
        state.roll_deg = self.player.roll.to_degrees();
        state.above_terrain_m = self.player.pos.y - terrain;
        state.falling = falling;
        state.carried_by = if settled.carriers.iter().any(|&c| record_of(c).is_some()) {
            "building"
        } else if settled.carriers.iter().any(|&c| rubble_of(c).is_some()) {
            "rubble"
        } else {
            "terrain"
        };
        state.max_pitch_deg = state.max_pitch_deg.max(state.pitch_deg.abs());
        if let Some((step, _)) = settled.step {
            if step <= tank_support::TIGER_STEP_M {
                state.max_climbed_step_m = state.max_climbed_step_m.max(step);
            }
        }
        if state.carried_by == "rubble" {
            state.max_rubble_above_terrain_m = state.max_rubble_above_terrain_m.max(state.above_terrain_m);
        }
        state.refused = "";
        state.obstacle.clear();
        state.refused_value = 0.0;
        state.crushing = None;
        state.belly_share = settled.belly_share;
        state.max_belly_share = state.max_belly_share.max(settled.belly_share);

        let Some(r) = r else { return };
        let mut contact = false;
        // High-centred: the belly carries a share of the weight. A building solid under it (front
        // steps, a plinth, a wall stub) meets the nose and belly as any obstacle does and gives way
        // by its strength; on the ground or rubble the tracks lose the grip the belly takes and
        // the belly drags (`tank_push::belly_drag`). Past what the push can overcome the tank is
        // hung up, its tracks turning, as a physical state rather than a refusal.
        if settled.belly_share > 0.0 {
            let direction = if self.player.speed >= 0.0 { 1.0 } else { -1.0 };
            match settled.belly_support.and_then(record_of) {
                Some(record) if self.destructibles.get(record).is_some_and(|d| d.alive) => {
                    contact |= self.crush_low_obstacle(record, pose, dt, r);
                }
                _ => {
                    let drag = crate::tank_push::belly_drag(settled.belly_share, self.player.pitch);
                    self.player.speed =
                        crate::tank_push::press(self.player.speed, self.player.pitch * direction, drag, 0.0, dt).speed;
                    if self.player.speed == 0.0 {
                        self.tank_ground.refused = "belly";
                        self.tank_ground.refused_value = settled.belly_share;
                        self.tank_ground.obstacle =
                            settled.belly_support.map_or_else(|| "terrain".to_owned(), |s| self.support_name(s));
                        contact = true;
                    }
                }
            }
        }
        if let Some(refusal) = refusal {
            let (kind, value, source) = match refusal {
                Refusal::Step(height, source) => ("step", height, source),
                Refusal::Overhead(height, source) => ("overhead", height, source),
                Refusal::Gradient(angle) => ("gradient", angle.to_degrees(), SOURCE_TERRAIN),
                Refusal::Tipping(angle) => ("tipping", angle.to_degrees(), SOURCE_TERRAIN),
            };
            self.tank_ground.refused = kind;
            self.tank_ground.refused_value = value;
            self.tank_ground.obstacle = self.support_name(source);
            let crushable = |record: usize| match kind {
                // Rising from the ground within the hull's reach: the bow and tracks meet it as
                // they meet a wall, and its material decides.
                "step" => true,
                // Hanging over the tracks, the hull or turret meets it edge-on: a wall (the
                // masonry over a doorway, a wall standing on a floor the tracks are below) is
                // punched through, timber and sheet iron break, and a floor, landing or stair
                // slides off its bearing against friction on its own weight (`push_resistance`);
                // how firmly a slab is cast into its walls is not known, so its weight alone
                // holds it (docs/PHYSICAL_TRUTH_AUDIT.md, tank ground contact row).
                "overhead" => self.buildings.owner(record).is_some(),
                _ => false,
            };
            match record_of(source) {
                Some(record) if crushable(record) => contact = self.crush_low_obstacle(record, pose, dt, r),
                _ => {
                    // Terrain, rubble, a gradient, a crest under the belly or a floor over the
                    // turret: the Tiger stops there.
                    self.player.speed = 0.0;
                    contact = true;
                }
            }
            if !self.tank_obstacle_was {
                println!(
                    "[panzerrechnersystem] TANK STOPPED — {kind} {value:.2} at ({:.1},{:.1}) against {}",
                    self.player.pos.x,
                    self.player.pos.z,
                    self.tank_ground.obstacle
                );
            }
        }
        self.tank_obstacle_was = contact;

        // The weight. A suspended floor or stair under the tracks, or under the rubble they rest
        // on, fails at once; a wall stub the tracks ride over is crushed at its material's grind
        // rate; a foundation on grade and the terrain carry it.
        let mut loaded: Vec<usize> = Vec::new();
        let rotation = glam::Mat3::from_rotation_y(pose.heading)
            * glam::Mat3::from_rotation_x(-self.player.pitch)
            * glam::Mat3::from_rotation_z(self.player.roll);
        for &(line, s, source) in &settled.carrier_stations {
            if let Some(record) = record_of(source) {
                loaded.push(record);
            } else if let Some((instance, Some(module))) = rubble_of(source) {
                // Through the rubble into the floors it lies on, under this station (the load
                // spreads through loose brick by about half a metre).
                let at = self.player.pos + rotation * Vec3::new(line.x(), 0.0, s);
                let Some(mound) = self.buildings.rubble_mound(instance, module) else { continue };
                for &floor in &mound.resting_on {
                    for binding in self.buildings.bindings(instance, floor) {
                        let under = at.x >= binding.min.x - 0.5
                            && at.x <= binding.max.x + 0.5
                            && at.z >= binding.min.z - 0.5
                            && at.z <= binding.max.z + 0.5;
                        if under {
                            loaded.extend(binding.destructible_index);
                        }
                    }
                }
            }
        }
        loaded.sort_unstable();
        loaded.dedup();
        let mut failed_modules = std::collections::BTreeSet::new();
        for record in loaded {
            if !self.destructibles.get(record).is_some_and(|d| d.alive) {
                continue;
            }
            let Some((instance, module)) = self.buildings.owner(record) else { continue };
            let kind = self.buildings.get(instance).layout().placements()[module].kind;
            match kind {
                BuildingModuleKind::Foundation => {} // on grade: the ground carries it
                BuildingModuleKind::Slab | BuildingModuleKind::Stair => {
                    if !failed_modules.insert((instance, module)) {
                        continue;
                    }
                    let remaining = self
                        .buildings
                        .integrity_for_destructible(record)
                        .map_or(1.0, |(hp, _)| f32::from(hp) + 1.0);
                    println!(
                        "[panzerrechnersystem] FLOOR GAVE WAY — {:?} {} of {} under the Tiger's weight",
                        kind,
                        self.buildings.get(instance).layout().placements()[module].module_id,
                        self.buildings.get(instance).instance_id()
                    );
                    self.tank_ground.gave_way += 1;
                    let at = self.player.center();
                    self.foley(crate::acoustics::foley::Cue::Vehicle(crate::acoustics::foley::VehicleCue::FloorCollapse), 1.0, 0.0, at);
                    self.tank_ground.crushing = Some(record);
                    self.damage_structure(record, remaining, r);
                    self.crush_rumble = self.crush_rumble.max(1.2);
                }
                _ => {
                    let dps = self.destructibles[record].class.crush().dps;
                    if dps > 0.0 {
                        self.damage_structure(record, dps * dt, r);
                        self.crush_rumble = self.crush_rumble.max(0.55);
                    }
                }
            }
        }
    }

    /// What a support source is, for telemetry and the log.
    fn support_name(&self, source: u32) -> String {
        if source == SOURCE_TERRAIN {
            return "terrain".to_owned();
        }
        if let Some((instance, module)) = rubble_of(source) {
            let building = self.buildings.get(instance);
            return match module {
                Some(module) => format!(
                    "rubble of {} {}",
                    building.instance_id(),
                    building.layout().placements()[module].module_id
                ),
                None => format!("burned-floor rubble of {}", building.instance_id()),
            };
        }
        let record = source as usize;
        match self.buildings.owner(record) {
            Some((instance, module)) => {
                let placement = &self.buildings.get(instance).layout().placements()[module];
                format!(
                    "{:?} {} ({:?}) of {}",
                    placement.kind,
                    placement.module_id,
                    self.destructibles[record].class,
                    self.buildings.get(instance).instance_id()
                )
            }
            None => format!("record {record}"),
        }
    }
    /// The tracks meet a building module higher than a step: the crush gate walls use. Timber
    /// bursts at speed; brick grinds and stone and concrete crawl-grind, taking crush damage
    /// through `damage_structure` while the tank presses on; bunker concrete stops it. Returns
    /// true while in contact.
    fn crush_low_obstacle(&mut self, record: usize, pose: TankPose, dt: f32, r: &mut Renderer) -> bool {
        let d = self.destructibles[record];
        if !d.alive {
            return false;
        }
        let speed = self.player.speed.abs();
        self.tank_ground.crushing = Some(record);
        // The tracks and bow press on it by the push model (`tank_push`, `Game::push_resistance`):
        // a plinth or wall held by its building is punched through by a ram, not a crawl; front
        // steps shift; a slab met edge-on slides off its bearing.
        let dir = if self.player.speed >= 0.0 { 1.0 } else { -1.0 };
        if self.press_on_structure(record, self.player.pitch * dir, dt, r) {
            return true;
        }
        if speed > 0.4 {
            let (sin, cos) = pose.heading.sin_cos();
            let nose = pose.pos + Vec3::new(sin, 0.0, cos) * dir * tank_support::TIGER_FRONT_Z;
            let c = Vec3::new(
                nose.x.clamp(d.min.x, d.max.x),
                d.max.y.min(pose.pos.y + 0.8),
                nose.z.clamp(d.min.z, d.max.z),
            );
            if self.crush_fx_t <= 0.0 {
                self.crush_fx_t = 0.22;
                let seed = d.seed ^ (self.destructibles[record].hp as u32).wrapping_mul(2_654_435_761);
                self.spawn_debris(c, 2, 3.5, seed);
                self.spawn_dust(c, 0.45);
            }
            if !self.tank_obstacle_was {
                self.schedule_emission(crate::audio::Sfx::ImpactGround, c,
                    crate::sound_propagation::Emission::from_level(&crate::sound_levels::PROP_IMPACT_1M).shifted(10.0), None, None);
                println!(
                    "[panzerrechnersystem] KETTEN-CRUSH — grinding low piece #{record} ({:?}, {:.0}% integrity)",
                    d.class,
                    100.0 * self.destructibles[record].hp / self.destructibles[record].max_hp.max(1.0)
                );
            }
        }
        true
    }

    /// Print the support samples of the hull as it stands that carry an obstacle (test builds).
    #[cfg(test)]
    pub(super) fn debug_tank_obstacles(&self) {
        let pose = self.player_pose();
        let solids = self.tank_solids(pose.pos);
        let reach = Vec2::splat(TANK_SOLID_REACH_M);
        let xz = Vec2::new(pose.pos.x, pose.pos.z);
        let rubble = self.buildings.rubble_near(xz - reach, xz + reach);
        let samples = self.tank_samples(pose, &solids, &rubble, self.tank_rest.as_ref());
        if let Some(rest) = self.tank_rest {
            println!(
                "  rest: pose ({:.2},{:.2},{:.2}) pitch {:.1} roll {:.1}; settled y0 {:.2} pitch {:.1} roll {:.1}",
                rest.pose.pos.x, rest.pose.pos.y, rest.pose.pos.z, rest.pose.pitch.to_degrees(), rest.pose.roll.to_degrees(),
                rest.settled.y0, rest.settled.slope_s.atan().to_degrees(), rest.settled.slope_x.atan().to_degrees()
            );
        }
        for sample in samples.iter().filter(|s| s.obstacle.is_some()) {
            let o = sample.obstacle.unwrap();
            println!(
                "  obstacle {:?} s {:5.2} support {:6.2} ({}) obstacle +{:.2} overhead {} {}",
                sample.line, sample.s, sample.height, self.support_name(sample.source), o.height, o.overhead,
                self.support_name(o.source)
            );
        }
    }

    /// What the player Tiger's tracks met this frame.
    pub fn tank_ground_state(&self) -> &TankGroundState {
        &self.tank_ground
    }

    /// Lay the rubble of `instance`'s newly failed modules (or lay its pile again) and give every
    /// laid mound's instance its transform, its visibility and its sight occluder; a failed
    /// module whose rubble stays in a sealed cellar shows none.
    pub(super) fn sync_building_rubble(&mut self, instance: usize, r: &mut Renderer, full: bool) {
        let hf = &self.hf;
        let (mut laid, mut buried) = self
            .buildings
            .settle_rubble(instance, &|xz: Vec2| hf.height_at(xz.x, xz.y));
        if full {
            // Every failed module: its mound shows where the pile laid it, or none shows.
            laid.clear();
            buried.clear();
            let runtime = self.buildings.get(instance);
            for module in 0..runtime.layout().placements().len() {
                if runtime.module_intact(module) || runtime.module_proxies(module).is_empty() {
                    continue;
                }
                if self.buildings.rubble_mound(instance, module).is_some() {
                    laid.push(module);
                } else {
                    buried.push(module);
                }
            }
        }
        let ground_y = self.buildings.ground_y(instance);
        for module in laid {
            let Some(mound) = self.buildings.rubble_mound(instance, module) else { continue };
            let transform = mound.transform;
            let crest_top = mound.core_floor();
            // A mound laid in an open cellar (below the pad) stands its occluder on its own base.
            let ground_y = ground_y.min(transform.w_axis.y);
            let bindings = self.buildings.bindings(instance, module);
            let mut moved = std::collections::BTreeSet::new();
            for binding in &bindings {
                if let Some(crate::renderer::SceneRef::Instance(handle)) = binding.rubble {
                    if moved.insert((handle.batch, handle.instance)) {
                        r.set_instance_matrix(handle, transform);
                        r.set_instance_visible(handle, true);
                    }
                }
                if let Some(occluder) = binding.occluder_index {
                    if occluder < self.occluders.len() {
                        let standing = stealth::Occluder::from_aabb(binding.min, binding.max);
                        self.occluders[occluder] = rubble_occluder(standing, ground_y, crest_top);
                    }
                }
            }
        }
        for module in buried {
            for binding in self.buildings.bindings(instance, module) {
                if let Some(rubble) = binding.rubble {
                    r.set_ref_visible(rubble, false);
                }
            }
        }
    }

    /// Stand the dismounted soldier on rubble: rubble at most a step above his feet carries him,
    /// a higher face turns the move back, and walking down a mound keeps his footing.
    pub(super) fn ground_foot_on_rubble(&mut self, previous: Vec3, previously_airborne: bool) {
        let xz = Vec2::new(self.foot.pos.x, self.foot.pos.z);
        let Some(hit) = self.buildings.rubble_top_from(xz, previous.y.max(self.foot.pos.y) + FOOT_RUBBLE_STEP_M) else { return };
        let top = hit.height;
        let feet = self.foot.pos.y;
        let stepping_down = !previously_airborne && top >= previous.y - FOOT_RUBBLE_STEP_DOWN_M;
        if top < feet - 1.0e-3 && !stepping_down {
            return; // the rubble lies below the feet: they stand on something higher, or fall
        }
        if self.foot.airborne && self.foot.vel_y > 0.0 && top < feet {
            return; // rising in a jump
        }
        if top - previous.y > FOOT_RUBBLE_STEP_M + 1.0e-3 && top > feet + 1.0e-3 {
            // A rubble face higher than a step: the feet stay where they were.
            self.foot.pos.x = previous.x;
            self.foot.pos.z = previous.z;
            self.foot.pos.y = self.foot.pos.y.max(previous.y.min(top));
            return;
        }
        self.foot.pos.y = top;
        self.foot.vel_y = 0.0;
        self.foot.airborne = false;
    }

    /// Where a round travelling `start → end` first enters rubble (below a mound's or a
    /// burned-floor layer's surface), as (distance along the segment, point, building instance).
    pub(super) fn rubble_segment_contact(&self, start: Vec3, end: Vec3) -> Option<(f32, Vec3, usize)> {
        let length = start.distance(end);
        if !(length > 1.0e-6) {
            return None;
        }
        let min = Vec2::new(start.x.min(end.x), start.z.min(end.z));
        let max = Vec2::new(start.x.max(end.x), start.z.max(end.z));
        let rubble = self.buildings.rubble_near(min, max);
        if rubble.is_empty() {
            return None;
        }
        let direction = (end - start) / length;
        let below = |p: Vec3| rubble.top_at_from(Vec2::new(p.x, p.z), p.y).filter(|hit| p.y < hit.height);
        let steps = (length / RUBBLE_MARCH_M).ceil().max(1.0) as usize;
        let mut previous = 0.0_f32;
        for k in 1..=steps {
            let t = (k as f32 * RUBBLE_MARCH_M).min(length);
            let p = start + direction * t;
            if let Some(hit) = below(p) {
                // Refine the entry between the last point above and this one.
                let (mut lo, mut hi) = (previous, t);
                for _ in 0..12 {
                    let mid = 0.5 * (lo + hi);
                    if below(start + direction * mid).is_some() {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                return Some((hi, start + direction * hi, hit.instance));
            }
            previous = t;
        }
        None
    }

    /// A round's path through rubble from its entry point: where it leaves the rubble (`None`
    /// when the ground or its budget stops it inside), the rubble thickness crossed and where it
    /// stopped or left.
    pub(super) fn rubble_transit(&self, entry: Vec3, direction: Vec3, budget_mm: f32) -> RubbleTransit {
        let direction = direction.normalize_or_zero();
        let reach = 30.0_f32;
        let a = Vec2::new(entry.x, entry.z);
        let b = a + Vec2::new(direction.x, direction.z) * reach;
        let rubble = self.buildings.rubble_near(a.min(b), a.max(b));
        let mut t = 0.0_f32;
        loop {
            t += RUBBLE_MARCH_M;
            let p = entry + direction * t;
            let spent = t * RUBBLE_AP_COST_MM_PER_M;
            if spent >= budget_mm {
                let stop = budget_mm / RUBBLE_AP_COST_MM_PER_M;
                return RubbleTransit { exit: None, thickness_m: stop, end: entry + direction * stop };
            }
            if p.y <= self.hf.height_at(p.x, p.z) {
                return RubbleTransit { exit: None, thickness_m: t, end: p };
            }
            match rubble.top_at_from(Vec2::new(p.x, p.z), p.y) {
                Some(hit) if p.y < hit.height => {}
                _ => return RubbleTransit { exit: Some(p), thickness_m: t, end: p },
            }
            if t > reach {
                return RubbleTransit { exit: Some(p), thickness_m: t, end: p };
            }
        }
    }
}

/// A round's path through rubble (`Game::rubble_transit`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RubbleTransit {
    pub exit: Option<Vec3>,
    pub thickness_m: f32,
    pub end: Vec3,
}

/// The sight occluder a failed module leaves over its rubble: its footprint, from its building's
/// ground to the lowest point of its mound's crest region, so it never stands above the drawn
/// rubble (before 2026-09-29 it was a knee-high box whatever lay there).
pub(super) fn rubble_occluder(standing: stealth::Occluder, ground_y: f32, crest_top: f32) -> stealth::Occluder {
    stealth::Occluder {
        min: Vec3::new(standing.min.x, ground_y, standing.min.z),
        max: Vec3::new(
            standing.max.x,
            crest_top.clamp(ground_y, ground_y + RUBBLE_OCCLUDER_REACH_M),
            standing.max.z,
        ),
    }
}

/// Highest a rubble occluder may stand above its building's ground (the sight index holds each
/// building occluder up to this height).
pub(super) const RUBBLE_OCCLUDER_REACH_M: f32 = 12.0;

/// GPU scenarios on the generated urban-intro district (`tools/worldgen/urban_intro_district.py`).
#[cfg(test)]
pub(super) mod district {
    use super::*;

    pub const DISTRICT: &str = "assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json";

    /// The district's inspection-lighting copy (`tools/worldgen/urban_intro_views.py
    /// --write-inspect`): the same terrain and buildings under a higher sun, for stills.
    pub const DISTRICT_INSPECT: &str =
        "assets/worldgen/workshops/act1-urban-intro-20260928-inspect/manifest.v1.json";

    pub fn build() -> anyhow::Result<(Renderer, Game)> {
        // Built here rather than through `build_gpu`: a `Game` moved through one more frame by
        // value overflows a test thread's stack in the dev profile.
        anyhow::ensure!(std::path::Path::new(DISTRICT).is_file(), "generate the district first");
        let act = crate::act_manifest::select_runtime_manifest(Some(DISTRICT.into()))?
            .expect("the district is a campaign act");
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let built = Game::build_act(&device, &queue, false, &crate::world::act(act));
        crate::act_manifest::select_runtime_manifest(None)?;
        let (r, mut g) = built?;
        g.god = true;
        g.debug_banish_enemies();
        for soldier in &mut g.soldiers {
            soldier.alive = false;
        }
        Ok((r, g))
    }

    /// The district from `manifest` with its GPU device and queue (for rendering stills); no
    /// enemies, no soldiers, god mode.
    pub fn build_gpu(manifest: &str) -> anyhow::Result<(wgpu::Device, wgpu::Queue, Renderer, Game)> {
        anyhow::ensure!(std::path::Path::new(manifest).is_file(), "generate the district first");
        let act = crate::act_manifest::select_runtime_manifest(Some(manifest.into()))?
            .expect("the district is a campaign act");
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let built = Game::build_act(&device, &queue, false, &crate::world::act(act));
        crate::act_manifest::select_runtime_manifest(None)?;
        let (r, mut g) = built?;
        g.god = true;
        g.debug_banish_enemies();
        for soldier in &mut g.soldiers {
            soldier.alive = false;
        }
        Ok((device, queue, r, g))
    }

    /// Put the Tiger at rest at (x, z) facing `heading_deg` (0 = +Z, 90 = +X), settled on its
    /// supports.
    pub fn place(g: &mut Game, r: &mut Renderer, x: f32, z: f32, heading_deg: f32) {
        g.player.pos = Vec3::new(x, g.hf.height_at(x, z), z);
        g.player.heading = heading_deg.to_radians();
        g.player.speed = 0.0;
        g.player.pitch = 0.0;
        g.player.roll = 0.0;
        g.tank_rest = None;
        g.tank_fall_vy = 0.0;
        g.tank_ground = TankGroundState::default();
        for _ in 0..5 {
            g.update(&TankInput::default(), 1.0 / 60.0, r);
        }
    }

    /// A start in the street in front of a building's facade (its local +Z side), `back_m` from
    /// the facade and `lateral_m` along it, facing the facade: (x, z, heading in degrees).
    pub fn in_front_of(g: &Game, building: &str, back_m: f32, lateral_m: f32) -> (f32, f32, f32) {
        let instance = g.buildings.index_of(building).expect("building");
        let world = g.buildings.get(instance).local_to_world();
        let out = world.transform_vector3(Vec3::Z).normalize();
        let along = world.transform_vector3(Vec3::X).normalize();
        let (lo, hi) = g.buildings.get(instance).world_bounds();
        let centre = (lo + hi) * 0.5;
        // The facade line: the building's extent along its outward normal.
        let half = ((hi - lo) * 0.5).dot(out.abs());
        let start = centre + out * (half + back_m) + along * lateral_m;
        let heading = (-out.x).atan2(-out.z).to_degrees();
        (start.x, start.z, heading)
    }

    /// Print a building's modules whose solids stand within `max_height` of its pad.
    pub fn describe_low_modules(g: &Game, building: &str, max_height: f32) {
        let instance = g.buildings.index_of(building).expect("building");
        let runtime = g.buildings.get(instance);
        let ground = g.buildings.ground_y(instance);
        for (module, placement) in runtime.layout().placements().iter().enumerate() {
            for &(min, max) in runtime.module_proxies(module) {
                if min.y - ground > max_height || max.y < ground - 0.05 {
                    continue;
                }
                println!(
                    "  {:<34} {:?} {:<28} {} y {:5.2}..{:5.2} x {:7.2}..{:7.2} z {:7.2}..{:7.2}",
                    placement.module_id.as_str(), placement.kind, placement.asset_id.as_str(),
                    if runtime.module_intact(module) { "intact" } else { "FAILED" },
                    min.y - ground, max.y - ground, min.x, max.x, min.z, max.z
                );
                break;
            }
        }
    }

    /// Drive with `drive` for `seconds` at 60 Hz, printing the ground state every half second.
    pub fn drive(g: &mut Game, r: &mut Renderer, drive: f32, seconds: f32, label: &str) -> Vec<(f32, Vec3, TankGroundState)> {
        let mut log = Vec::new();
        let frames = (seconds * 60.0).round() as usize;
        // URBAN2_EVERY=<frames>[,from_s,to_s]: print more often inside a window.
        let every: Vec<f32> = std::env::var("URBAN2_EVERY")
            .ok()
            .map(|v| v.split(',').filter_map(|p| p.parse().ok()).collect())
            .unwrap_or_default();
        for frame in 0..frames {
            let t = (frame + 1) as f32 / 60.0;
            let window = every.len() == 3 && t >= every[1] && t <= every[2];
            if window && std::env::var("URBAN2_SAMPLES").is_ok() {
                println!("  -- frame t {t:.3}");
            }
            DEBUG_SAMPLES.with(|d| d.set(window && std::env::var("URBAN2_SAMPLES").is_ok()));
            g.update(&TankInput { drive, ..TankInput::default() }, 1.0 / 60.0, r);
            DEBUG_SAMPLES.with(|d| d.set(false));
            let state = g.tank_ground_state().clone();
            let dense = every.len() == 3 && t >= every[1] && t <= every[2] && (frame + 1) % (every[0].max(1.0) as usize) == 0;
            if (frame + 1) % 30 == 0 || dense {
                println!(
                    "[{label}] t {t:5.2} pos ({:7.2},{:6.2},{:7.2}) v {:5.2} pitch {:5.1} roll {:5.1} above {:4.2} on {:<8} refused {:<8} {:5.2} {} crush {:?} gave {}",
                    g.player.pos.x, g.player.pos.y, g.player.pos.z, g.player.speed,
                    state.pitch_deg, state.roll_deg, state.above_terrain_m, state.carried_by,
                    state.refused, state.refused_value, state.obstacle, state.crushing, state.gave_way
                );
            }
            log.push((t, g.player.pos, state));
        }
        log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vehicle_stops_at_the_edge_of_the_terrain_grid() {
        // 201 samples at 5 m: the grid spans +-500 m.
        let hf = Heightfield { res: 201, spacing: 5.0, heights: vec![0.0; 201 * 201],
            min_h: 0.0, max_h: 0.0, openings: Default::default(), detail: None };
        let limit = 500.0 - TERRAIN_EDGE_CLEARANCE_M;
        assert_eq!(terrain_edge_step(&hf, Vec2::new(10.0, 0.0), Vec2::new(11.0, 0.5)), None);
        assert_eq!(terrain_edge_step(&hf, Vec2::new(limit - 0.1, 40.0), Vec2::new(limit + 0.3, 40.2)),
            Some(Vec2::new(limit, 40.2)), "the step ends on the limit and may still slide along it");
        assert_eq!(terrain_edge_step(&hf, Vec2::new(0.0, -limit + 0.05), Vec2::new(3.0, -limit - 9.0)),
            Some(Vec2::new(3.0, -limit)));
        // A fixture staged beyond the grid is not dragged back; a degenerate grid constrains nothing.
        assert_eq!(terrain_edge_step(&hf, Vec2::new(600.0, 0.0), Vec2::new(601.0, 0.0)), None);
        let tiny = Heightfield { res: 2, spacing: 1.0, heights: vec![0.0; 4],
            min_h: 0.0, max_h: 0.0, openings: Default::default(), detail: None };
        assert_eq!(terrain_edge_step(&tiny, Vec2::ZERO, Vec2::new(50.0, 0.0)), None);
        assert_eq!(terrain_edge_step(&hf, Vec2::ZERO, Vec2::new(f32::NAN, 0.0)), None);
    }

    #[test]
    #[ignore = "exploration: requires a GPU and the generated urban district"]
    fn explore_the_tiger_on_the_district() -> anyhow::Result<()> {
        let (mut r, mut g) = district::build()?;
        if let Ok(target) = std::env::var("URBAN2_EXPLORE") {
            // URBAN2_EXPLORE=<building id>[,back_m,lateral_m,drive,seconds]
            let mut parts = target.split(',');
            let building = parts.next().unwrap().to_owned();
            let mut next = |default: f32| parts.next().and_then(|v| v.parse().ok()).unwrap_or(default);
            let (back, lateral, drive, seconds) = (next(10.0), next(0.0), next(0.6), next(20.0));
            district::describe_low_modules(&g, &building, 1.6);
            let (x, z, heading) = district::in_front_of(&g, &building, back, lateral);
            println!("start ({x:.1}, {z:.1}) heading {heading:.0}");
            district::place(&mut g, &mut r, x, z, heading);
            district::drive(&mut g, &mut r, drive, seconds, &building);
            g.debug_tank_obstacles();
            return Ok(());
        }
        // The verifier's drive (round 1, issue 1): into block c3r2.w01 over its raised floor.
        district::place(&mut g, &mut r, 1852.0, 296.0, 90.0);
        district::drive(&mut g, &mut r, 0.6, 9.0, "c3r2.w01");
        let instance = g.buildings.index_of("urban_intro.c3r2.w01").unwrap();
        let runtime = g.buildings.get(instance);
        println!("ground {:.2}", g.buildings.ground_y(instance));
        for &module in g.buildings.rubble_pile_order(instance) {
            let mound = g.buildings.rubble_mound(instance, module).unwrap();
            let placement = &runtime.layout().placements()[module];
            let t = mound.transform;
            println!(
                "  {:<34} {:?} {:<28} base {:6.2} scale ({:.2},{:.2},{:.2}) crest {:.2} budget {:.2} added {:.2} top {:6.2}",
                placement.module_id.as_str(), placement.kind, placement.asset_id.as_str(), t.w_axis.y,
                t.x_axis.truncate().length(), t.y_axis.y, t.z_axis.truncate().length(),
                mound.shape.crest, mound.shape.budget, mound.added_volume, mound.world_max.y
            );
        }
        Ok(())
    }

    /// Failed modules of a building whose id starts with one of `prefixes`.
    fn failed_modules(g: &Game, building: &str, prefixes: &[&str]) -> Vec<String> {
        let instance = g.buildings.index_of(building).expect("building");
        let runtime = g.buildings.get(instance);
        runtime
            .layout()
            .placements()
            .iter()
            .enumerate()
            .filter(|(module, placement)| {
                !runtime.module_intact(*module)
                    && prefixes.iter().any(|prefix| placement.module_id.as_str().starts_with(prefix))
            })
            .map(|(_, placement)| placement.module_id.as_str().to_owned())
            .collect()
    }

    /// Step: a burned wooden house's 0.45 m brick footing (below the Tiger's 0.76 m) is climbed,
    /// the hull bridges its footings (6.5 m apart under a 7 m hull), grinds the stove's brick pier,
    /// crosses the 0.6 m stove footing and comes down on the far side.
    #[test]
    #[ignore = "requires a GPU and the generated urban district"]
    fn the_tiger_climbs_a_footing_below_its_step_limit_and_bridges_it() -> anyhow::Result<()> {
        let (mut r, mut g) = district::build()?;
        let (x, z, heading) = district::in_front_of(&g, "urban_intro.c0r0.e00", 8.0, 0.0);
        district::place(&mut g, &mut r, x, z, heading);
        let log = district::drive(&mut g, &mut r, 0.5, 11.0, "step");
        let last = &log.last().unwrap().2;
        assert!(
            (0.40..=tank_support::TIGER_STEP_M).contains(&last.max_climbed_step_m),
            "largest step climbed {:.2} m", last.max_climbed_step_m
        );
        let on_footing = log.iter().filter(|(_, _, s)| s.carried_by == "building" && s.above_terrain_m > 0.4).count();
        assert!(on_footing > 60, "carried by the footings for {on_footing} frames");
        assert!(last.max_pitch_deg < 15.0, "a 0.45 m step pitches the hull {:.1}°", last.max_pitch_deg);
        assert!(!log.iter().any(|(_, _, s)| s.refused == "step" && s.refused_value > tank_support::TIGER_STEP_M));
        // Through the house: the far footing is at x 1596.0-1596.4.
        let end = log.last().unwrap().1;
        assert!(end.x < 1590.0, "the Tiger came down on the far side: x {:.1}", end.x);
        assert_eq!(failed_modules(&g, "urban_intro.c0r0.e00", &["module.s0.stove"]).len(), 1, "the stove pier was ground down");
        Ok(())
    }

    /// Crushing into a raised ground floor: an intact merchant house's plinth (the cellar wall,
    /// 0.8 m) and facade bays are punched through by the Tiger's momentum (`tank_push`: a crawl
    /// could not), its stone front steps are shoved aside rather than high-centring the hull, its
    /// landing and ground floor give way under the Tiger's weight, and the hull climbs their rubble
    /// into the house without ever pitching past its gradient while standing (the runaway climb
    /// of 2026-09-29 reached 42.7°). It comes to rest on the rubble slope against the house's
    /// load-bearing cross walls, which a Tiger on a 20° slope cannot push through.
    #[test]
    #[ignore = "requires a GPU and the generated urban district"]
    fn the_tiger_crushes_into_a_raised_ground_floor() -> anyhow::Result<()> {
        let (mut r, mut g) = district::build()?;
        let building = "urban_intro.c5r2.w01";
        let (x, z, heading) = district::in_front_of(&g, building, 8.0, 0.0);
        district::place(&mut g, &mut r, x, z, heading);
        let log = district::drive(&mut g, &mut r, 0.6, 20.0, "raised floor");
        let last = &log.last().unwrap().2;
        let walls = failed_modules(&g, building, &["module.sc.front", "module.s0.front"]);
        assert!(walls.len() >= 3, "plinth and facade ground: {walls:?}");
        let floors = failed_modules(&g, building, &["module.s0.floor", "module.s0.landing"]);
        assert!(last.gave_way >= 1 && floors.len() >= 2, "the raised floor gave way: {floors:?}");
        assert!(log.iter().all(|(_, _, s)| s.refused != "belly"), "never held on its belly by the front steps");
        // The bow stands inside the house, more than a metre past the facade line (x 2096).
        let end = log.last().unwrap().1;
        let bow = end.x + tank_support::TIGER_FRONT_Z * last.pitch_deg.to_radians().cos();
        assert!(bow > 2097.0, "the bow inside the facade line: x {bow:.2}");
        assert!(last.max_pitch_deg <= 36.0, "pitch {:.1}°", last.max_pitch_deg);
        assert!(last.above_terrain_m > 1.0 && last.carried_by == "rubble", "{last:?}");
        Ok(())
    }

    /// A rubble slope: the shelled merchant house's front mound (its fallen facade bays) is climbed
    /// to more than a metre above the street, pitching the hull like the slope.
    #[test]
    #[ignore = "requires a GPU and the generated urban district"]
    fn the_tiger_climbs_a_rubble_slope() -> anyhow::Result<()> {
        let (mut r, mut g) = district::build()?;
        let (x, z, heading) = district::in_front_of(&g, "urban_intro.c3r1.w01", 8.0, 0.0);
        district::place(&mut g, &mut r, x, z, heading);
        let log = district::drive(&mut g, &mut r, 0.6, 9.0, "rubble slope");
        let last = &log.last().unwrap().2;
        assert!(last.max_rubble_above_terrain_m > 1.0, "on rubble {:.2} m up", last.max_rubble_above_terrain_m);
        assert!((12.0..=35.0).contains(&last.max_pitch_deg), "pitch {:.1}°", last.max_pitch_deg);
        let on_rubble = log.iter().filter(|(_, _, s)| s.carried_by == "rubble").count();
        assert!(on_rubble > 300, "carried by rubble for {on_rubble} of {} frames", log.len());
        Ok(())
    }

    /// Held only by what exceeds its push (`tank_push`): at a crawl in first gear the Tiger comes
    /// to rest against the intact workshop `c5r3.w03`'s front wall, whose punching strength exceeds
    /// its tractive effort, and the wall stands; backed off and driven at it, its momentum breaks
    /// through and the wall falls. The raised concrete ground floor of `c4r2.w00` (1.31 m above
    /// the street, the step that held the Tiger before) is shoved off its bearing and falls.
    #[test]
    #[ignore = "requires a GPU and the generated urban district"]
    fn the_tiger_is_held_only_by_what_exceeds_its_push() -> anyhow::Result<()> {
        let (mut r, mut g) = district::build()?;
        let workshop = "urban_intro.c5r3.w03";
        let (x, z, heading) = district::in_front_of(&g, workshop, 5.0, 0.0);
        district::place(&mut g, &mut r, x, z, heading);
        // First gear at rated revolutions: 2.8 km/h.
        let crawl = crate::tank_push::FIRST_GEAR_M_S / crate::tank::TOP_SPEED;
        let log = district::drive(&mut g, &mut r, crawl, 12.0, "crawl");
        let tail = &log[log.len() - 120..];
        assert!(tail[0].1.distance(tail[tail.len() - 1].1) < 0.02, "at rest against the wall");
        assert_eq!(g.player.speed, 0.0);
        // A window pier weaker than the push may break at a crawl; the tank rests against a piece
        // stronger than its push, which stands.
        let holding = g.structures_near(g.player.pos + g.player.forward() * CRUSH_NOSE_LEN, CRUSH_NOSE_R)
            .into_iter()
            .filter(|&k| g.destructibles[k].alive && g.buildings.owner(k).is_some_and(|(i, _)| g.buildings.get(i).instance_id() == workshop))
            .filter(|&k| g.buildings.destructible_is_crush_barrier(k))
            .max_by(|&a, &b| g.push_resistance(a).force_n.total_cmp(&g.push_resistance(b).force_n))
            .expect("the tank rests against the workshop's wall");
        let resist = g.push_resistance(holding);
        assert!(resist.force_n > crate::tank_push::drive_force(0.0, 0.0), "held by {resist:?}");
        let (x, z, heading) = district::in_front_of(&g, workshop, 16.0, 0.0);
        district::place(&mut g, &mut r, x, z, heading);
        district::drive(&mut g, &mut r, 0.6, 6.0, "ram");
        assert!(!g.destructibles[holding].alive, "the ram broke through what held the crawl");

        let house = "urban_intro.c4r2.w00";
        let (x, z, heading) = district::in_front_of(&g, house, 10.0, 0.0);
        district::place(&mut g, &mut r, x, z, heading);
        let log = district::drive(&mut g, &mut r, 0.6, 16.0, house);
        let floor = failed_modules(&g, house, &["module.s0.floor.c03.front"]);
        assert!(!floor.is_empty(), "the 1.31 m concrete floor slid off its bearing");
        let held_by_the_floor = log.iter().rev().take(120)
            .all(|(_, _, s)| s.refused == "step" && s.obstacle.contains("module.s0.floor.c03.front"));
        assert!(!held_by_the_floor, "the floor no longer holds the Tiger as a step");
        Ok(())
    }

    /// The bulldozing film (2026-09-29): from inside the yard of block c4r2 the Tiger drives south
    /// through a stripped garden tree, the brick yard wall on the block's street line and across
    /// street D into the burned merchant house `c4r3.e00`: the tree bends over and falls, the wall
    /// is ploughed down with its debris, and the house's side wall is breached by the tank's
    /// momentum. Frames under a chase camera every 0.1 s to `captures/plough_film_20260929_*.png`
    /// and, when ffmpeg is on the path, `captures/plough_film_20260929.mp4`.
    #[test]
    #[ignore = "evidence film: requires a GPU and the generated urban district"]
    fn film_the_tiger_ploughing_a_tree_a_wall_and_a_house() -> anyhow::Result<()> {
        let (device, queue, mut r, mut g) = district::build_gpu(district::DISTRICT)?;
        let (cw, ch) = (r.iw, r.ih);
        let tree = Vec3::new(2069.856, 0.0, 331.0);
        let wall = Vec3::new(2073.0, 0.0, 347.8);
        let house = "urban_intro.c4r3.e00";
        let aim = Vec3::new(2076.5, 0.0, 366.4);
        let dir = (aim - tree).normalize();
        let start = tree - dir * 14.0;
        let heading = dir.x.atan2(dir.z).to_degrees();
        district::place(&mut g, &mut r, start.x, start.z, heading);
        let wall_record = (0..g.destructibles.len())
            .filter(|&k| g.destructibles[k].alive && g.buildings.owner(k).is_none())
            .min_by(|&a, &b| {
                let da = Vec2::new(g.destructibles[a].center().x - wall.x, g.destructibles[a].center().z - wall.z).length();
                let db = Vec2::new(g.destructibles[b].center().x - wall.x, g.destructibles[b].center().z - wall.z).length();
                da.total_cmp(&db)
            })
            .expect("the yard wall is a destruction record");
        let down_before = g.trees.debug_down_count();
        let mut frames = 0usize;
        let mut t = 0.0_f32;
        let mut next_frame = 0.0_f32;
        while t < 16.0 {
            g.update(&TankInput { drive: 0.55, ..TankInput::default() }, 1.0 / 60.0, &mut r);
            t += 1.0 / 60.0;
            if t >= next_frame {
                next_frame += 0.1;
                let (sin, cos) = g.player.heading.sin_cos();
                let forward = Vec3::new(sin, 0.0, cos);
                let right = Vec3::new(cos, 0.0, -sin);
                let centre = g.player.pos + Vec3::Y * 1.5;
                r.camera.pos = centre - forward * 13.0 + right * 6.0 + Vec3::Y * 6.5;
                r.camera.set_forward(centre + forward * 6.0 - r.camera.pos);
                r.camera.fov_y = 60f32.to_radians();
                r.reticle = false;
                r.optic_mask = 0.0;
                r.render_to_png(&device, &queue, &format!("captures/plough_film_20260929_{frames:04}.png"), cw, ch)?;
                frames += 1;
            }
        }
        let status = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-framerate", "10", "-i", "captures/plough_film_20260929_%04d.png",
                "-pix_fmt", "yuv420p", "-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2", "captures/plough_film_20260929.mp4"])
            .status();
        println!("[plough-film] {frames} frames; ffmpeg: {status:?}");
        assert!(g.trees.debug_down_count() > down_before, "the garden tree fell");
        assert!(!g.destructibles[wall_record].alive, "the yard wall was ploughed down");
        assert!(!failed_modules(&g, house, &["module."]).is_empty(), "the house was breached");
        Ok(())
    }

    /// Evidence, not an assertion: external stills of the Tiger in the four film scenes, under the
    /// district's inspection lighting, from a camera beside (or behind and above) the hull, written
    /// to `captures/urban2_solid_city_still_<scene>_<moment>.png`.
    #[test]
    #[ignore = "evidence stills: requires a GPU and the generated urban district"]
    fn stills_of_the_tiger_in_the_solid_city() -> anyhow::Result<()> {
        let (device, queue, mut r, mut g) = district::build_gpu(district::DISTRICT_INSPECT)?;
        let (cw, ch) = (r.iw * 2, r.ih * 2);
        // (scene, building, back, drive, the camera beside the hull (else behind it), moments)
        let scenes: [(&str, &str, f32, f32, bool, &[(f32, &str)]); 4] = [
            ("step", "urban_intro.c0r0.e00", 8.0, 0.5, true, &[(4.0, "on_the_footing"), (9.3, "over_the_stove_footing")]),
            ("raised_floor", "urban_intro.c5r2.w01", 8.0, 0.6, false, &[(9.0, "at_the_facade"), (20.0, "on_the_rubble_inside")]),
            ("rubble_slope", "urban_intro.c3r1.w01", 8.0, 0.6, true, &[(6.5, "up_the_mound")]),
            ("stopped", "urban_intro.c2r0.r00", 10.0, 0.6, false, &[(18.0, "hung_on_the_crest")]),
        ];
        for (scene, building, back, drive, beside, moments) in scenes {
            let (x, z, heading) = district::in_front_of(&g, building, back, 0.0);
            district::place(&mut g, &mut r, x, z, heading);
            let mut t = 0.0_f32;
            for &(at, moment) in moments {
                while t < at {
                    g.update(&TankInput { drive, ..TankInput::default() }, 1.0 / 60.0, &mut r);
                    t += 1.0 / 60.0;
                }
                let (sin, cos) = g.player.heading.sin_cos();
                let forward = Vec3::new(sin, 0.0, cos);
                let right = Vec3::new(cos, 0.0, -sin);
                let centre = g.player.pos + Vec3::Y * 1.5;
                r.camera.pos = if beside {
                    centre + right * 13.0 + Vec3::Y * 3.0 + forward * 2.0
                } else {
                    centre - forward * 12.0 + Vec3::Y * 7.0
                };
                r.camera.set_forward(centre + forward * 1.5 - r.camera.pos);
                r.camera.fov_y = 55f32.to_radians();
                r.reticle = false;
                r.optic_mask = 0.0;
                let state = g.tank_ground_state().clone();
                let path = format!("captures/urban2_solid_city_still_{scene}_{moment}.png");
                r.render_to_png(&device, &queue, &path, cw, ch)?;
                println!(
                    "[still] {path}: pitch {:.1} roll {:.1} above {:.2} on {} refused {} {}",
                    state.pitch_deg, state.roll_deg, state.above_terrain_m, state.carried_by, state.refused, state.obstacle
                );
            }
        }
        Ok(())
    }

    /// Feet climb rubble a step at a time: a soldier walking up the shelled merchant house's front
    /// mound stands on it, never rising more than a step in one move.
    #[test]
    #[ignore = "requires a GPU and the generated urban district"]
    fn a_soldier_walks_up_a_rubble_mound() -> anyhow::Result<()> {
        let (mut r, mut g) = district::build()?;
        let (x, z, _) = district::in_front_of(&g, "urban_intro.c3r1.w01", 6.0, 1.5);
        g.mode = Mode::Foot;
        g.foot.pos = Vec3::new(x, g.hf.height_at(x, z), z);
        g.foot.yaw = std::f32::consts::FRAC_PI_2; // facing +x, the facade
        g.foot.airborne = false;
        let mut previous = g.foot.pos;
        let mut highest = 0.0_f32;
        for _ in 0..(6 * 60) {
            g.update(&TankInput { drive: 1.0, ..TankInput::default() }, 1.0 / 60.0, &mut r);
            let feet = g.foot.pos;
            assert!(feet.y - previous.y <= FOOT_RUBBLE_STEP_M + 0.02, "a {:.2} m rise in one move", feet.y - previous.y);
            highest = highest.max(feet.y - g.hf.height_at(feet.x, feet.z));
            previous = feet;
        }
        assert!(highest > 0.5, "stood {highest:.2} m up on the rubble");
        Ok(())
    }

    /// A round through a rubble mound: the mound of the shelled merchant house stops a round on
    /// a level path at 0.4 m, and costs it 47 mm per metre it crosses.
    #[test]
    #[ignore = "requires a GPU and the generated urban district"]
    fn a_round_meets_rubble_and_loses_its_budget_by_thickness() -> anyhow::Result<()> {
        let (_r, g) = district::build()?;
        let instance = g.buildings.index_of("urban_intro.c3r1.w01").unwrap();
        let ground = g.buildings.ground_y(instance);
        let start = Vec3::new(1845.0, ground + 0.4, 105.0);
        let end = Vec3::new(1870.0, ground + 0.4, 105.0);
        let (distance, entry, hit) = g.rubble_segment_contact(start, end).expect("the round meets the mound");
        assert_eq!(hit, instance);
        assert!(distance > 1.0 && entry.x < 1868.0, "entry {entry:?}");
        let transit = g.rubble_transit(entry, Vec3::X, 110.0);
        assert!(transit.thickness_m > 0.1);
        match transit.exit {
            Some(exit) => assert!((exit.x - entry.x - transit.thickness_m).abs() < 0.11),
            None => assert!(transit.thickness_m <= 110.0 / RUBBLE_AP_COST_MM_PER_M + 1.0e-3),
        }
        Ok(())
    }

    #[test]
    fn building_modules_take_their_kit_material_for_crush_and_transit() {
        assert_eq!(building_module_material("pizza.structure.wall"), Material::Brick);
        assert_eq!(building_module_material("pizza.structure.window_frame"), Material::Brick);
        assert_eq!(building_module_material("pizza.structure.floor"), Material::Stone);
        assert_eq!(building_module_material("pizza.structure.stairs"), Material::Stone);
        assert_eq!(building_module_material("urban.timber.wall"), Material::Timber);
        assert_eq!(building_module_material("urban.timber.floor"), Material::Timber);
        assert_eq!(building_module_material("urban.metal.roof"), Material::Timber);
        assert_eq!(building_module_material("urban.metal.beam"), Material::Stone);
        // Timber bursts at speed, brick grinds, concrete crawl-grinds: the wall crush table.
        assert_eq!(Material::Timber.crush().tier, 1);
        assert_eq!(Material::Brick.crush().tier, 2);
        assert_eq!(Material::Stone.crush().tier, 3);
    }

    #[test]
    fn rubble_costs_a_round_by_the_thickness_it_crosses() {
        // Per metre of rubble a round spends about two thirds of what a 0.5 m brick wall costs it
        // per metre of brick, by density (1.5 bulking).
        let brick_per_m = Material::Brick.ap().cost_mm / 0.5;
        assert!((RUBBLE_AP_COST_MM_PER_M - brick_per_m / 1.5).abs() < 1.0);
        // A PzGr 39 at 500 m (about 110 mm) crosses 2 m of rubble and keeps a budget; 3 m stops it.
        let budget = crate::ballistics::budget_mm(&crate::ballistics::PZGR_39, 500.0);
        assert!(budget - 2.0 * RUBBLE_AP_COST_MM_PER_M > 0.0);
        assert!(budget - 3.0 * RUBBLE_AP_COST_MM_PER_M < 0.0, "budget {budget}");
    }

    #[test]
    fn a_rubble_occluder_never_stands_above_the_drawn_rubble() {
        let standing = stealth::Occluder::from_aabb(Vec3::new(0.0, 5.0, 0.0), Vec3::new(4.0, 8.0, 0.5));
        let o = rubble_occluder(standing, 1.0, 3.2);
        assert_eq!((o.min.y, o.max.y), (1.0, 3.2));
        let low = rubble_occluder(standing, 1.0, 1.1);
        assert!((low.max.y - 1.1).abs() < 1e-6);
        let buried = rubble_occluder(standing, 1.0, 0.4);
        assert!((buried.max.y - 1.0).abs() < 1e-6);
    }
}
