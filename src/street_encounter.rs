//! The winter encounter in a street district (urban intro, 29 Sep 2026).
//!
//! When the manifest revision carries a `street_encounter` block (`crate::street_plan`, written by
//! the district tool), the encounter is laid in the streets instead of the courtyard's open ground:
//!
//! - **The guns are sited by a field-of-fire probe on the built district.** Every candidate the
//!   plan offers (the rubble of a razed plot, or beside a building corner) is measured with the
//!   game's own tests: the ground must take the gun and its crew (no cover, record or occluder within
//!   2 m, under 0.8 m of relief), the crew must be able to come up to it from behind (25 m of
//!   walkable ground), it must not stand in a platoon's beat, and it must not see the spawn or the
//!   house within its 400 m opening range (the courtyard's guns stood beyond point blank of both).
//!   Its field of fire is then every kill-zone point (the tram street north out of the courtyard and
//!   the Tiger's other ways out north, sampled every 8 m) between 100 m (inside that the crew goes to
//!   ground) and 400 m that the gun commander's eye sees through `los_clear` and that its round can
//!   reach: nothing standing, no story cover, door, tree or terrain between the muzzle and the
//!   Tiger's lower side. A point scores its line's weight times (0.25 + 0.75 of the sine between
//!   the line of fire and the Tiger's travel), so flanking fire counts four times fire along the
//!   street, times a range weight (1 from 120 to 350 m, 0.6 closer, falling to 0.5 at 400 m). The
//!   platoon's two guns are the pair with the best combined field (each point counted once at the
//!   better gun, half again when both guns see it from opposite sides of the Tiger's path) that
//!   stand at least 60 m apart and do not fire into each other. The gun is laid on the centre of its
//!   own field, and only what lies inside its sector (±60°) or within point blank counts, as those
//!   are all it engages. Sources: docs/research/SOVIET_AT_WEAPONS_BRIEF_20260928.md (flanking fire from
//!   concealment, open at 300-400 m, tungsten for close flank shots); R1 §7.1 (S3 pp. 296-298).
//! - **The platoons walk street beats and drive along the streets** (`street_nav`,
//!   `winter_encounter_ai::Streets`): a 4 m grid of cells on the district's ways where a T-34 on a
//!   route has room (`street_route_cell_clear`), A* routes along it, pure pursuit; wingmen drive to
//!   their place on their leader's own track in column.
use super::*;
use crate::street_plan::{KillZoneLine, SiteKind, StreetPlan};
use super::street_nav::{StreetNav, NAV_CELL_M};

/// Kill-zone sampling pitch along each centreline.
const KILL_ZONE_STEP_M: f32 = 8.0;
/// The gun commander's eye above the ground (`update_at_guns`).
const GUN_EYE_M: f32 = 1.1;
/// The 45 mm aims at the Tiger's lower side, this high (at_guns::choose_shot flank aim).
const LOWER_SIDE_M: f32 = 0.75;
/// Ground a gun and its crew need: half-size of the clear box round the axle.
const GUN_GROUND_HALF_M: f32 = 2.0;
const GUN_GROUND_RELIEF_M: f32 = 0.8;
/// The crew and its ammunition come up from behind: walkable ground this far back.
const REAR_APPROACH_M: f32 = 25.0;
/// A gun does not stand in its own tanks' beat: off the carriageway they drive (their clear band is
/// 4.5 m either side of the street's centreline).
const GUN_OFF_BEAT_M: f32 = 12.0;
/// The platoon's two guns stand at least this far apart.
const GUN_SEPARATION_M: f32 = 60.0;
/// Pairs are drawn from this many best single sites.
const PAIR_POOL: usize = 48;
/// The nav grid covers everything the plan names, and this much open ground round it.
const NAV_MARGIN_M: f32 = 150.0;
/// Half-size of the box a T-34 needs clear on a street route: its hull's half-width (1.79 m) and
/// 1.2 m either side. A route runs through cells that are all clear, so a hull on it keeps its
/// length on the route too; the manoeuvre veto checks its width over the next 4 and 8 m.
pub(super) const ROUTE_CLEAR_HALF_M: f32 = 3.0;
/// Relief across that box a T-34 drives over (about 16°; its limit is 30°).
const ROUTE_RELIEF_M: f32 = 1.2;

/// One sampled point of the kill zone.
#[derive(Clone, Copy, Debug)]
pub(super) struct KillPoint {
    pub at: Vec2,
    pub ground: f32,
    /// Unit direction of the Tiger's travel along the street.
    pub travel: Vec2,
    pub weight: f32,
    pub tram_street: bool,
}

/// A kill-zone point a sited gun can see and hit.
#[derive(Clone, Copy, Debug)]
pub(super) struct FieldPoint {
    pub point: usize,
    pub range_m: f32,
    /// |sin| of the angle between the line of fire and the Tiger's travel (1 = broadside).
    pub flank: f32,
    pub score: f32,
}

#[derive(Clone, Debug)]
pub(super) struct SitedGun {
    pub at: Vec2,
    pub heading: f32,
    pub kind: SiteKind,
    pub on: String,
    pub score: f32,
    pub field: Vec<FieldPoint>,
}

/// The probe's result, kept for the snapshot, the review staging and the evidence.
#[derive(Clone, Debug)]
pub(super) struct StreetSiting {
    pub kill_zone: Vec<KillPoint>,
    pub guns: [SitedGun; 2],
    pub receipt: serde_json::Value,
}

fn sample_kill_zone(lines: &[KillZoneLine], ground: impl Fn(Vec2) -> f32) -> Vec<KillPoint> {
    let mut out = Vec::new();
    for line in lines {
        for segment in line.points.windows(2) {
            let (a, b) = (segment[0], segment[1]);
            let n = ((a.distance(b) / KILL_ZONE_STEP_M).ceil() as usize).max(1);
            let travel = (b - a).normalize_or_zero();
            for k in 0..=n {
                let at = a.lerp(b, k as f32 / n as f32);
                out.push(KillPoint { at, ground: ground(at), travel, weight: line.weight, tram_street: line.tram_street });
            }
        }
    }
    out
}

fn range_weight(range_m: f32) -> f32 {
    if range_m < 120.0 { 0.6 } else if range_m <= 350.0 { 1.0 } else { 1.0 - 0.5 * ((range_m - 350.0) / 50.0).min(1.0) }
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

impl Game {
    /// The street district's drivable-ground grid: cells on the district's ways (or outside it) where
    /// a T-34 on a route has room (`street_route_cell_clear`), over everything the plan names (beats,
    /// kill zone, gun sites) with a margin of open ground.
    pub(super) fn build_street_nav(&self, plan: &StreetPlan) -> StreetNav {
        let mut lo = Vec2::splat(f32::MAX);
        let mut hi = Vec2::splat(f32::MIN);
        let mut grow = |p: Vec2| { lo = lo.min(p); hi = hi.max(p); };
        plan.beats.iter().flatten().for_each(|&p| grow(p));
        plan.kill_zone.iter().flat_map(|line| line.points.iter()).for_each(|&p| grow(p));
        plan.gun_sites.iter().for_each(|site| grow(site.at));
        let edge = self.act.half_extent_m - 20.0;
        let lo = (lo - Vec2::splat(NAV_MARGIN_M)).max(Vec2::splat(-edge));
        let hi = (hi + Vec2::splat(NAV_MARGIN_M)).min(Vec2::splat(edge));
        StreetNav::build(lo, hi, NAV_CELL_M, |p| plan.ways.contains(p) && self.street_route_cell_clear(p))
    }

    /// Room for a T-34 on a street route at `p`: no story cover, standing record or sight occluder
    /// within `ROUTE_CLEAR_HALF_M` of it up to the hull's roof, and under `ROUTE_RELIEF_M` of relief
    /// across that box. (`winter_patrol_point_clear`, the open-ground probe, asks for a 4.46 m
    /// half-box at any heading and 1.2 m of relief over it: the whole hull turning on the spot, which
    /// a street's poles and ramps rarely leave.)
    pub(super) fn street_route_cell_clear(&self, p: Vec2) -> bool {
        let half = ROUTE_CLEAR_HALF_M;
        let ground = self.hf.height_at(p.x, p.y);
        !self.box_region_blocked(Vec3::new(p.x - half, ground + 0.15, p.y - half), Vec3::new(p.x + half, ground + 2.7, p.y + half))
            && [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)].iter()
                .all(|&(x, z)| (self.hf.height_at(p.x + x * half, p.y + z * half) - ground).abs() < ROUTE_RELIEF_M)
    }

    /// Room for a T-34 at `from` to drive its own width to `p`, on the district's ways: no story cover, standing record or
    /// sight occluder within its half-width and 0.4 m of the point up to its roof, and under 1.2 m of
    /// relief there (the street layout's manoeuvre veto; `winter_patrol_point_clear` asks the same for
    /// the whole hull at any heading).
    pub(super) fn street_way_clear(&self, from: Vec2, p: Vec2) -> bool {
        // Off the ways already (pushed, or mustered aside), a hull may drive back onto them.
        if self.street_plan.as_ref().is_some_and(|plan| plan.ways.contains(from) && !plan.ways.contains(p)) { return false; }
        let half = tank::T34_PARTS.hull_half_extents.x + 0.4;
        let ground = self.hf.height_at(p.x, p.y);
        !self.box_region_blocked(Vec3::new(p.x - half, ground + 0.15, p.y - half), Vec3::new(p.x + half, ground + 2.7, p.y + half))
            && [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)].iter()
                .all(|&(x, z)| (self.hf.height_at(p.x + x * half, p.y + z * half) - ground).abs() < 1.2)
    }

    /// Nothing the gun's round would strike stands between `from` and `to`: no standing record,
    /// story cover, door, tree or terrain (the authoritative projectile's own contacts, less the
    /// moving bodies).
    pub(super) fn street_line_of_fire_clear(&self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        let distance = delta.length();
        if distance < 1e-3 { return true; }
        !self.structure_or_story_on_sight(from, to, 0.3)
            && !self.world_doors.blocks_segment(from, to)
            && !self.trees.blocks_segment(from, to)
            && ray_terrain_near(from, delta / distance, &self.hf, distance - 1.0, 0.5).is_none()
    }

    /// The field-of-fire probe: choose the platoon's two gun sites among the plan's candidates on
    /// the built district (see the module doc). `beats` are the platoons' beats as fielded.
    pub(super) fn site_street_at_guns(&self, plan: &StreetPlan, beats: &[Vec<Vec2>; 2]) -> anyhow::Result<StreetSiting> {
        let started = std::time::Instant::now();
        let kill_zone = sample_kill_zone(&plan.kill_zone, |p| self.hf.height_at(p.x, p.y));
        let tiger_at = |p: Vec2| Vec3::new(p.x, self.hf.height_at(p.x, p.y) + tank::TIGER_PARTS.center_h, p.y);
        let watched = [tiger_at(self.act.spawn), tiger_at(winter_mission::HOUSE)];
        let legs: Vec<(Vec2, Vec2)> = beats.iter()
            .flat_map(|beat| beat.iter().zip(beat.iter().cycle().skip(1)).map(|(&a, &b)| (a, b))).collect();
        let mut rejected = std::collections::BTreeMap::<&str, usize>::new();
        let mut valid: Vec<(usize, SitedGun, Vec<f32>)> = Vec::new();
        for (index, site) in plan.gun_sites.iter().enumerate() {
            let p = site.at;
            let g = self.hf.height_at(p.x, p.y);
            let why = if legs.iter().any(|&(a, b)| segment_distance(p, a, b) < GUN_OFF_BEAT_M) {
                Some("in a platoon's beat")
            } else if self.box_region_blocked(Vec3::new(p.x - GUN_GROUND_HALF_M, g + 0.2, p.y - GUN_GROUND_HALF_M),
                Vec3::new(p.x + GUN_GROUND_HALF_M, g + 1.4, p.y + GUN_GROUND_HALF_M)) {
                Some("no ground for the gun and crew")
            } else if [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)].iter().any(|&(sx, sz)|
                (self.hf.height_at(p.x + sx * GUN_GROUND_HALF_M, p.y + sz * GUN_GROUND_HALF_M) - g).abs() > GUN_GROUND_RELIEF_M) {
                Some("ground too steep to lay the gun")
            } else {
                None
            };
            if let Some(why) = why { *rejected.entry(why).or_default() += 1; continue; }
            let eye = Vec3::new(p.x, g + GUN_EYE_M, p.y);
            if watched.iter().any(|&q| eye.distance(q) <= at_guns::OPEN_FIRE_M && self.los_clear(eye, q)) {
                *rejected.entry("sees the spawn or the house").or_default() += 1;
                continue;
            }
            let mut field = Vec::new();
            for (k, point) in kill_zone.iter().enumerate() {
                let centre = Vec3::new(point.at.x, point.ground + tank::TIGER_PARTS.center_h, point.at.y);
                let range_m = eye.distance(centre);
                if !(at_guns::GROUND_M..=at_guns::OPEN_FIRE_M).contains(&range_m) || !self.los_clear(eye, centre) {
                    continue;
                }
                let aim = Vec3::new(point.at.x, point.ground + LOWER_SIDE_M, point.at.y);
                let (muzzle, _) = at_guns::AtGun::new(Vec3::new(p.x, g, p.y), at_guns::bearing(eye, aim)).muzzle();
                if !self.street_line_of_fire_clear(muzzle, aim) { continue; }
                let flank = (point.at - p).normalize_or_zero().perp_dot(point.travel).abs();
                let score = point.weight * (0.25 + 0.75 * flank) * range_weight(range_m);
                field.push(FieldPoint { point: k, range_m, flank, score });
            }
            // Laid on the centre of what it sees; beyond point blank it engages only inside its sector
            // (at_guns: ±60°), so what lies outside is not its field.
            let seen: f32 = field.iter().map(|f| f.score).sum();
            let heading = (seen > 0.0).then(|| {
                let centre = field.iter().map(|f| kill_zone[f.point].at * f.score).sum::<Vec2>() / seen;
                (centre.x - p.x).atan2(centre.y - p.y)
            }).unwrap_or(0.0);
            field.retain(|f| {
                let at = kill_zone[f.point].at;
                f.range_m <= at_guns::POINT_BLANK_M
                    || at_guns::wrap((at.x - p.x).atan2(at.y - p.y) - heading).abs() <= at_guns::SECTOR_HALF
            });
            if field.len() < 3 {
                *rejected.entry("no field of fire over the kill zone").or_default() += 1;
                continue;
            }
            let total: f32 = field.iter().map(|f| f.score).sum();
            let mut scores = vec![0.0f32; kill_zone.len()];
            for f in &field { scores[f.point] = f.score; }
            // The crew comes up from behind, over ground a man can walk.
            let back = -Vec2::new(heading.sin(), heading.cos());
            let mut previous = g;
            let rear_clear = (1..=5).all(|s| {
                let q = p + back * (REAR_APPROACH_M * s as f32 / 5.0);
                let h = self.hf.height_at(q.x, q.y);
                let walkable = (h - previous).abs() < 1.0
                    && !self.box_region_blocked(Vec3::new(q.x - 0.35, h + 0.3, q.y - 0.35), Vec3::new(q.x + 0.35, h + 1.7, q.y + 0.35));
                previous = h;
                walkable
            });
            if !rear_clear {
                *rejected.entry("no approach from behind").or_default() += 1;
                continue;
            }
            valid.push((index, SitedGun { at: p, heading, kind: site.kind, on: site.on.clone(), score: total, field }, scores));
        }
        anyhow::ensure!(valid.len() >= 2, "street encounter: only {} of {} candidate gun sites can fight here ({rejected:?})",
            valid.len(), plan.gun_sites.len());
        valid.sort_by(|a, b| b.1.score.total_cmp(&a.1.score).then(a.0.cmp(&b.0)));
        let pool = &valid[..valid.len().min(PAIR_POOL)];
        let eye_of = |gun: &SitedGun| Vec3::new(gun.at.x, self.hf.height_at(gun.at.x, gun.at.y) + GUN_EYE_M, gun.at.y);
        let mut best: Option<(f32, usize, usize)> = None;
        for a in 0..pool.len() {
            for b in a + 1..pool.len() {
                let (ga, gb) = (&pool[a].1, &pool[b].1);
                if ga.at.distance(gb.at) < GUN_SEPARATION_M { continue; }
                let facing = |from: &SitedGun, to: &SitedGun| at_guns::wrap((to.at.x - from.at.x).atan2(to.at.y - from.at.y) - from.heading).abs()
                    <= at_guns::SECTOR_HALF;
                if (facing(ga, gb) || facing(gb, ga)) && self.los_clear(eye_of(ga), eye_of(gb)) { continue; }
                let mut score = 0.0;
                for (k, point) in kill_zone.iter().enumerate() {
                    let (sa, sb) = (pool[a].2[k], pool[b].2[k]);
                    score += sa.max(sb);
                    let side = |gun: Vec2| (gun - point.at).perp_dot(point.travel).signum();
                    if sa > 0.0 && sb > 0.0 && side(ga.at) != side(gb.at) { score += 0.5 * sa.min(sb); }
                }
                if best.is_none_or(|(s, _, _)| score > s) { best = Some((score, a, b)); }
            }
        }
        let (pair_score, a, b) = best.ok_or_else(|| anyhow::anyhow!(
            "street encounter: no two of the {} fighting gun sites stand {GUN_SEPARATION_M} m apart without firing into each other", pool.len()))?;
        let guns = [pool[a].1.clone(), pool[b].1.clone()];
        let describe = |gun: &SitedGun| {
            let seen = |tram: bool| gun.field.iter().filter(|f| kill_zone[f.point].tram_street == tram).count() as f32 * KILL_ZONE_STEP_M;
            let (near, far) = gun.field.iter().fold((f32::MAX, 0.0f32), |(n, f), p| (n.min(p.range_m), f.max(p.range_m)));
            serde_json::json!({
                "kind": gun.kind.name(), "on": gun.on, "at": [gun.at.x, gun.at.y],
                "heading_deg": gun.heading.to_degrees(), "score": gun.score,
                "kill_zone_points_seen": gun.field.len(),
                "tram_street_seen_m": seen(true), "ways_out_seen_m": seen(false),
                "mean_flank_sine": gun.field.iter().map(|f| f.flank).sum::<f32>() / gun.field.len() as f32,
                "range_m": [near, far],
            })
        };
        // Which side of the tram street a site stands on: the district tool draws it north-south
        // (x constant), so east is the larger x.
        let tram_x = plan.kill_zone.iter().find(|line| line.tram_street).map(|line| line.points[0].x);
        let best_on = |east: bool| valid.iter().filter(|(_, gun, _)| tram_x.is_some_and(|x| (gun.at.x > x) == east)).take(3)
            .map(|(_, gun, _)| describe(gun)).collect::<Vec<_>>();
        let receipt = serde_json::json!({
            "best_single_sites": valid.iter().take(12).map(|(_, gun, _)| describe(gun)).collect::<Vec<_>>(),
            "best_sites_by_side_of_the_tram_street": {"west": best_on(false), "east": best_on(true)},
            "layout": "streets", "candidates": plan.gun_sites.len(), "fighting_candidates": valid.len(),
            "rejected": rejected, "kill_zone_points": kill_zone.len(), "kill_zone_step_m": KILL_ZONE_STEP_M,
            "pair_score": pair_score, "guns": [describe(&guns[0]), describe(&guns[1])],
            "probe_ms": started.elapsed().as_secs_f64() * 1e3,
            "rule": "field-of-fire probe on the built district (src/street_encounter.rs module doc)",
        });
        Ok(StreetSiting { kill_zone, guns, receipt })
    }

    /// Debug/capture telemetry of the street layout, if the encounter is in the streets.
    pub fn street_encounter_snapshot(&self) -> Option<serde_json::Value> {
        let siting = self.street_siting.as_ref()?;
        let nav = self.winter_encounter.as_ref().and_then(|e| e.streets()).map(|s| {
            let (w, h) = s.nav().size();
            serde_json::json!({"cells": w * h, "clear_cells": s.nav().clear_cells(), "cell_m": NAV_CELL_M})
        });
        Some(serde_json::json!({"siting": siting.receipt, "nav": nav,
            "beats": self.winter_encounter.as_ref().map(|e| (0..2).map(|p| e.route(p).iter().map(|q| [q.x, q.y]).collect::<Vec<_>>()).collect::<Vec<_>>())}))
    }

    /// A place for the Tiger in gun `g`'s field of fire about `range_m` out: a kill-zone point it
    /// sees and hits (on the tram street first, then the other streets of the district, then the open
    /// ground beyond), where the Tiger's hull stands clear and its eye at the gunner's height sees the
    /// whole gun position (its shield and a metre and a half either side). For the review staging and
    /// the tests; `None` outside the streets.
    pub(super) fn street_at_gun_stage(&self, g: usize, range_m: f32) -> Option<Vec3> {
        let siting = self.street_siting.as_ref()?;
        let gun = self.at_guns.get(g).copied().flatten()?;
        let (district_lo, district_hi) = self.street_plan.as_ref()?.ways.district;
        let outside = |f: &FieldPoint| {
            let at = siting.kill_zone[f.point].at;
            !(at.cmpge(district_lo).all() && at.cmple(district_hi).all())
        };
        let side_street = |f: &FieldPoint| !siting.kill_zone[f.point].tram_street;
        let mut field = siting.guns.get(g)?.field.clone();
        field.sort_by(|a, b| outside(a).cmp(&outside(b)).then(side_street(a).cmp(&side_street(b)))
            .then((a.range_m - range_m).abs().total_cmp(&(b.range_m - range_m).abs())));
        let reach = tank::TIGER_PARTS.hull_half_extents.length() + 0.3;
        field.iter().map(|f| siting.kill_zone[f.point]).find_map(|point| {
            let p = Vec3::new(point.at.x, point.ground, point.at.y);
            let hull_clear = !self.box_region_blocked(Vec3::new(p.x - reach, p.y + 0.3, p.z - reach),
                Vec3::new(p.x + reach, p.y + 2.8, p.z + reach));
            let across = Vec3::new(gun.pos.z - p.z, 0.0, p.x - gun.pos.x).normalize_or_zero();
            let whole_position = [-1.5f32, 0.0, 1.5].iter().all(|&side|
                self.los_clear(p + Vec3::Y * 2.4, gun.pos + across * side + Vec3::Y * 0.7));
            (hull_clear && whole_position
                && self.los_clear(gun.pos + Vec3::Y * GUN_EYE_M, p + Vec3::Y * tank::TIGER_PARTS.center_h)).then_some(p)
        })
    }
}

#[cfg(test)]
impl Game {
    /// Hull sample points of T-34 `i` that stand inside a standing building record, and inside
    /// story cover (evidence: a T-34 does not collide with walls, so this measures its driving).
    pub(super) fn t34_hull_intrusions(&self, i: usize) -> (usize, usize) {
        let enemy = &self.enemies[i];
        let (half, heading) = (enemy.parts.hull_half_extents, enemy.heading);
        let (right, forward) = (Vec2::new(heading.cos(), -heading.sin()), Vec2::new(heading.sin(), heading.cos()));
        let (mut building, mut story) = (0, 0);
        for a in 0..5 {
            for b in 0..9 {
                let local = Vec2::new(half.x * (a as f32 / 2.0 - 1.0) * 0.95, half.y * (b as f32 / 4.0 - 1.0) * 0.95);
                let xz = Vec2::new(enemy.pos.x, enemy.pos.z) + right * local.x + forward * local.y;
                for lift in [0.5f32, 1.5] {
                    let p = Vec3::new(xz.x, self.hf.height_at(xz.x, xz.y) + lift, xz.y);
                    let inside = |min: Vec3, max: Vec3| p.cmpge(min).all() && p.cmple(max).all();
                    if self.structures_near(p, 0.5).into_iter().any(|k| {
                        let d = &self.destructibles[k];
                        d.alive && self.buildings.owner(k).is_some() && inside(d.min, d.max)
                    }) { building += 1; }
                    if self.story_cover.iter().any(|&(min, max)| inside(min, max)) { story += 1; }
                }
            }
        }
        (building, story)
    }

    /// Evidence map of the street encounter: the measured streets (grey clear, dark blocked), the
    /// buildings (blue), the kill zone (green), the two guns (red) with lines to every point of
    /// their field of fire, the beats (yellow), T-34 tracks on patrol (cyan) and after contact
    /// (magenta), and the spawn and house (white).
    pub(super) fn write_street_encounter_map(&self, path: &str, tracks: &[Vec<Vec2>], contact: &[Vec<Vec2>]) -> anyhow::Result<()> {
        let encounter = self.winter_encounter.as_ref().ok_or_else(|| anyhow::anyhow!("no encounter"))?;
        let streets = encounter.streets().ok_or_else(|| anyhow::anyhow!("not in the streets"))?;
        let siting = self.street_siting.as_ref().ok_or_else(|| anyhow::anyhow!("no siting"))?;
        let (lo, hi) = streets.nav().bounds();
        let (w, h) = ((hi.x - lo.x) as u32, (hi.y - lo.y) as u32);
        let mut img = image::RgbImage::new(w, h);
        let px = |p: Vec2| ((p.x - lo.x) as i64, (p.y - lo.y) as i64);
        for y in 0..h {
            for x in 0..w {
                let clear = streets.nav().is_clear(lo + Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                img.put_pixel(x, y, image::Rgb(if clear { [178, 178, 182] } else { [52, 52, 60] }));
            }
        }
        let dot = |img: &mut image::RgbImage, p: Vec2, r: i64, c: [u8; 3]| {
            let (cx, cy) = px(p);
            for dy in -r..=r { for dx in -r..=r {
                let (x, y) = (cx + dx, cy + dy);
                if dx * dx + dy * dy <= r * r && x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                    img.put_pixel(x as u32, y as u32, image::Rgb(c));
                }
            }}
        };
        let line = |img: &mut image::RgbImage, a: Vec2, b: Vec2, c: [u8; 3], blend: f32| {
            let n = (a.distance(b).ceil() as usize).max(1);
            for s in 0..=n {
                let (x, y) = px(a.lerp(b, s as f32 / n as f32));
                if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                    let old = img.get_pixel(x as u32, y as u32).0;
                    let mix = |o: u8, n: u8| (o as f32 * (1.0 - blend) + n as f32 * blend) as u8;
                    img.put_pixel(x as u32, y as u32, image::Rgb([mix(old[0], c[0]), mix(old[1], c[1]), mix(old[2], c[2])]));
                }
            }
        };
        for building in self.buildings.instances() {
            let (min, max) = building.world_bounds();
            let c = [70, 110, 255];
            let (a, b, cc, d) = (Vec2::new(min.x, min.z), Vec2::new(max.x, min.z), Vec2::new(max.x, max.z), Vec2::new(min.x, max.z));
            for (p, q) in [(a, b), (b, cc), (cc, d), (d, a)] { line(&mut img, p, q, c, 1.0); }
        }
        for beat in 0..2 {
            let route = encounter.route(beat);
            for k in 0..route.len() { line(&mut img, route[k], route[(k + 1) % route.len()], [240, 210, 70], 0.9); }
        }
        for point in &siting.kill_zone { dot(&mut img, point.at, if point.tram_street { 2 } else { 1 }, [70, 220, 110]); }
        for track in tracks {
            for s in track.windows(2) { line(&mut img, s[0], s[1], [60, 230, 240], 1.0); }
        }
        for track in contact {
            for s in track.windows(2) { line(&mut img, s[0], s[1], [235, 70, 235], 1.0); }
        }
        for gun in &siting.guns {
            for f in &gun.field { line(&mut img, gun.at, siting.kill_zone[f.point].at, [255, 120, 60], 0.35); }
        }
        for gun in &siting.guns {
            dot(&mut img, gun.at, 4, [255, 40, 40]);
            line(&mut img, gun.at, gun.at + Vec2::new(gun.heading.sin(), gun.heading.cos()) * 40.0, [255, 40, 40], 1.0);
        }
        dot(&mut img, self.act.spawn, 4, [255, 255, 255]);
        dot(&mut img, winter_mission::HOUSE, 3, [255, 255, 255]);
        let big = image::imageops::resize(&img, w * 2, h * 2, image::imageops::FilterType::Nearest);
        big.save(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kill_zone_is_sampled_along_each_line_with_its_travel_and_weight() {
        let lines = vec![
            KillZoneLine { id: "st.main".into(), tram_street: true, weight: 1.0,
                points: vec![Vec2::new(0.0, 100.0), Vec2::new(0.0, -100.0)] },
            KillZoneLine { id: "st.a".into(), tram_street: false, weight: 0.5,
                points: vec![Vec2::new(-40.0, 0.0), Vec2::new(40.0, 0.0)] },
        ];
        let points = sample_kill_zone(&lines, |_| 3.0);
        assert_eq!(points.len(), 26 + 11);
        assert!(points[..26].iter().all(|p| p.tram_street && p.travel == Vec2::new(0.0, -1.0) && p.ground == 3.0));
        assert!(points[26..].iter().all(|p| !p.tram_street && p.weight == 0.5 && p.travel == Vec2::X));
        assert!((points[1].at.y - (100.0 - KILL_ZONE_STEP_M)).abs() < 1e-4);
    }

    /// The street layout on the real district (run with KESSEL_PLAYTEST_MANIFEST at the urban-intro
    /// workshop; any other manifest has no street plan and the test says so and passes). Optional
    /// KESSEL_STREET_EVIDENCE_PREFIX writes the evidence map and the receipt.
    #[test]
    #[ignore = "requires GPU and the urban-intro workshop"]
    fn street_encounter_sites_the_guns_and_walks_the_beats_along_the_streets() -> anyhow::Result<()> {
        // (then fights through them after contact)
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let path = crate::act_manifest::playtest_manifest_path();
        crate::act_manifest::select_runtime_manifest(Some(path.clone()))?;
        let manifest = crate::act_manifest::ActManifest::load(&path)?;
        let map = crate::world::resolve_manifest(crate::world::act(1), &manifest)?;
        let (mut renderer, mut game) = Game::build_playtest_act(&device, &queue, false, &map)?;
        game.mute_audio();
        if game.street_plan.is_none() {
            println!("{path:?} lays no street encounter: nothing to test here");
            crate::act_manifest::select_runtime_manifest(None)?;
            return Ok(());
        }
        game.enable_live_playtest_encounter();
        let siting = game.street_siting.clone().expect("the probe sites the guns");
        let guns = [game.at_guns[0].expect("gun A"), game.at_guns[1].expect("gun B")];
        assert!(guns[0].pos.distance(guns[1].pos) >= GUN_SEPARATION_M);
        for (g, sited) in siting.guns.iter().enumerate() {
            assert!(sited.field.len() >= 3, "gun {g} has a field of fire");
            let tram = sited.field.iter().filter(|f| siting.kill_zone[f.point].tram_street).count();
            println!("gun {g}: {} {} at {:?}, {} kill-zone points ({tram} on the tram street)",
                sited.kind.name(), sited.on, sited.at, sited.field.len());
        }
        assert!(siting.guns.iter().any(|gun| gun.field.iter().any(|f| siting.kill_zone[f.point].tram_street)),
            "the guns cover the tram street");
        // Fifteen minutes of patrol with no one to find: the man stands far south of the beats.
        game.god = true;
        game.mode = Mode::Foot;
        game.foot.speed_frac = 0.0;
        game.foot.pos = Vec3::new(1850.0, game.hf.height_at(1850.0, 580.0), 580.0);
        let fielded: Vec<usize> = (0..winter_encounter_ai::SLOTS).filter(|&i| winter_encounter_ai::fielded(i)).collect();
        let beats = [game.winter_encounter.as_ref().unwrap().route(0), game.winter_encounter.as_ref().unwrap().route(1)];
        let mut visited = [vec![None::<f32>; beats[0].len()], vec![None::<f32>; beats[1].len()]];
        let mut tracks = vec![Vec::new(); winter_encounter_ai::SLOTS];
        let (mut intrusion_frames, mut worst_intrusion, mut story_frames) = (0usize, 0usize, 0usize);
        let mut worst_station = 0.0f32;
        let mut square_pass = f32::MAX;
        let dt = 1.0 / 30.0;
        for frame in 0..900 * 30 {
            game.enemy_ai(dt);
            for &i in &fielded {
                let e = &game.enemies[i];
                let at = Vec2::new(e.pos.x, e.pos.z);
                if frame % 30 == 0 { tracks[i].push(at); }
                if frame % 5 == 0 {
                    let (building, story) = game.t34_hull_intrusions(i);
                    if building > 0 { intrusion_frames += 1; worst_intrusion = worst_intrusion.max(building); }
                    if story > 0 { story_frames += 1; }
                }
                if let winter_encounter_ai::Role::Leader = game.winter_encounter.as_ref().unwrap().role(i) {
                    let platoon = winter_encounter_ai::platoon_of(i);
                    for (k, corner) in beats[platoon].iter().enumerate() {
                        if at.distance(*corner) < 8.0 && visited[platoon][k].is_none() { visited[platoon][k] = Some(frame as f32 * dt); }
                    }
                    if platoon == 1 { square_pass = square_pass.min(at.distance(beats[1][4])); }
                }
                if frame > 30 * 30 {
                    if let winter_encounter_ai::Role::Wingman { leader, place } = game.winter_encounter.as_ref().unwrap().role(i) {
                        let gap = e.pos.distance(game.enemies[leader].pos) - winter_encounter_ai::SPACING_M * place as f32;
                        worst_station = worst_station.max(gap);
                    }
                }
            }
        }
        let patrol_snapshot = game.winter_encounter_snapshot();
        println!("corners first reached (s) {visited:?}; B's closest pass of its square corner {square_pass:.1} m; worst station gap {worst_station:.1} m; \
            hull-in-building frames {intrusion_frames} (worst {worst_intrusion} samples); hull-in-story-cover frames {story_frames}");
        // Then contact: the Tiger at the spawn fires its gun (the ordinary discharge; god mode keeps
        // the observer). Four minutes of search and fight in the streets, measured the same way.
        game.mode = Mode::Tank;
        game.player.speed = 0.0;
        game.fire(&mut renderer);
        let mut contact_tracks = vec![Vec::new(); winter_encounter_ai::SLOTS];
        let (mut contact_intrusions, mut contact_story) = (0usize, 0usize);
        let mut first_shot_s = None;
        let mut closest_m = f32::MAX;
        for frame in 0..240 * 30 {
            game.enemy_ai(dt);
            game.advance_projectiles(dt, &mut renderer);
            for &i in &fielded {
                let e = &game.enemies[i];
                if !e.alive { continue; }
                let at = Vec2::new(e.pos.x, e.pos.z);
                if frame % 30 == 0 { contact_tracks[i].push(at); }
                closest_m = closest_m.min(e.pos.distance(game.player.pos));
                if frame % 5 == 0 {
                    let (building, story) = game.t34_hull_intrusions(i);
                    if building > 0 { contact_intrusions += 1; worst_intrusion = worst_intrusion.max(building); }
                    if story > 0 { contact_story += 1; }
                }
            }
            if first_shot_s.is_none() && fielded.iter().any(|&i| game.enemy_main_gun_shot_sequence(i) > 0) {
                first_shot_s = Some(frame as f32 * dt);
            }
        }
        let contact_shots: u32 = fielded.iter().map(|&i| game.enemy_main_gun_shot_sequence(i)).sum();
        let snapshot = game.winter_encounter_snapshot();
        println!("after contact: first T-34 shot at {first_shot_s:?} s, {contact_shots} T-34 shots in 240 s, closest hull {closest_m:.0} m; \
            hull-in-building frames {contact_intrusions}; hull-in-story-cover frames {contact_story}");
        if let Ok(prefix) = std::env::var("KESSEL_STREET_EVIDENCE_PREFIX") {
            game.write_street_encounter_map(&format!("{prefix}_map.png"), &tracks, &contact_tracks)?;
            std::fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
                "schema": "panzer.street-encounter-review.v1", "manifest": path, "adapter": adapter.get_info().name,
                "siting": siting.receipt, "corners_first_reached_s": visited, "square_pass_m": square_pass,
                "worst_station_gap_m": worst_station, "hull_in_building_frames": intrusion_frames,
                "hull_in_story_cover_frames": story_frames, "after_patrol": patrol_snapshot,
                "contact": {"first_t34_shot_s": first_shot_s, "t34_shots": contact_shots, "closest_hull_m": closest_m,
                    "hull_in_building_frames": contact_intrusions, "hull_in_story_cover_frames": contact_story},
                "final": snapshot,
                "tracks_every_second": fielded.iter().map(|&i| (i, tracks[i].iter().map(|p| [p.x, p.y]).collect::<Vec<_>>()))
                    .collect::<std::collections::BTreeMap<_, _>>(),
                "contact_tracks_every_second": fielded.iter().map(|&i| (i, contact_tracks[i].iter().map(|p| [p.x, p.y]).collect::<Vec<_>>()))
                    .collect::<std::collections::BTreeMap<_, _>>(),
                "scope": "Source Game enemy_ai for 900 s of patrol at 30 Hz on the built district with the man far south (no contact), \
                    then the Tiger's ordinary discharge at the spawn and 240 s of enemy_ai and projectiles; god mode preserves the observer."
            }))?)?;
        }
        assert!(visited.iter().all(|v| v.iter().all(Option::is_some)), "every platoon walks every corner of its beat: {visited:?}");
        assert!(square_pass < 8.0, "platoon B crosses the square");
        assert_eq!(intrusion_frames + contact_intrusions, 0, "no T-34 hull enters a building ({worst_intrusion} samples at worst)");
        assert!(contact_shots > 0, "after the Tiger's shot the platoons find and engage it through the streets");
        assert!(worst_station < 60.0, "wingmen keep station along the streets: {worst_station:.1} m beyond their interval");
        crate::act_manifest::select_runtime_manifest(None)?;
        Ok(())
    }

    #[test]
    fn flanking_fire_in_the_doctrines_band_scores_highest() {
        assert_eq!(range_weight(200.0), 1.0);
        assert!(range_weight(110.0) < range_weight(150.0));
        assert!(range_weight(400.0) < range_weight(340.0) && range_weight(400.0) >= 0.5);
        assert!((segment_distance(Vec2::new(5.0, 3.0), Vec2::ZERO, Vec2::new(10.0, 0.0)) - 3.0).abs() < 1e-5);
    }
}
