//! The mission's binding to the simulation (src/mission.rs, docs/MISSIONS.md): enabling a sheet on
//! the built map, the per-step observation, the requests (platoons, alerts, flares, smoke), the
//! modeled emitters (the burning star's light and smoke, the smoke column) and the read-outs for
//! the paper map, the pocket compass and the frame. Nothing here changes a combatant's laws: a
//! release is the course director's own platoon entry, an alert is the stealth model's own
//! Searching state and the guns' own `heard` flag.
use super::*;
use crate::mission::{self, FlareColor, Mission, MissionState, Observation, Place, Request};
use crate::renderer::PointLight;

/// Visible size of the burning star's emitting grain (m). The star itself is a few centimetres;
/// at a kilometre it is sub-pixel, so the grain carries the emission the display's glare then
/// spreads. The light's source radius stays the true `FLARE_SOURCE_RADIUS_M`.
const FLARE_GRAIN_M: f32 = 0.60;
/// The grain's emissive scale: a burning magnesium star is brighter than sunlit snow by orders
/// of magnitude, so the grain must saturate the display even by day for the glare to spread it.
const FLARE_GRAIN_EMISSIVE: f32 = 1400.0;
/// The column rises at about this rate; wind leans it by the ratio.
const SMOKE_RISE_M_S: f32 = 2.2;

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (k, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if k > 0 { out.push('_'); }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// A point `fraction` of the way along `route` by arc length.
pub fn route_point(route: &[Vec2], fraction: f32) -> Option<Vec2> {
    if route.is_empty() { return None; }
    let mut lengths = Vec::with_capacity(route.len());
    let mut total = 0.0;
    lengths.push(0.0);
    for edge in route.windows(2) {
        total += edge[0].distance(edge[1]);
        lengths.push(total);
    }
    let target = fraction.clamp(0.0, 1.0) * total;
    for (k, edge) in route.windows(2).enumerate() {
        if lengths[k + 1] >= target {
            let span = (lengths[k + 1] - lengths[k]).max(1e-6);
            return Some(edge[0].lerp(edge[1], ((target - lengths[k]) / span).clamp(0.0, 1.0)));
        }
    }
    route.last().copied()
}

impl Game {
    /// Enable the sheet beside the selected manifest (or the act's tracked sheet). `Ok(false)`
    /// when the map has none; an unreadable sheet is an error the caller reports and plays on.
    pub fn enable_mission_from_sheet(&mut self) -> anyhow::Result<bool> {
        let manifest = crate::act_manifest::selected_runtime_manifest().map(|(_, path)| path);
        let Some(path) = mission::locate(manifest.as_deref(), self.act.id) else { return Ok(false) };
        let sheet = mission::load(&path)?;
        self.enable_mission(sheet)?;
        Ok(true)
    }

    pub fn enable_mission(&mut self, sheet: Mission) -> anyhow::Result<()> {
        let state = MissionState::new(sheet, &|place| self.resolve_place(place))?;
        println!("[mission] {:?} enabled from {}: {} beats, first {:?}", state.mission.id,
            state.mission.source.display(), state.beats.len(), state.mission.beats[0].title);
        self.mission = Some(state);
        self.mission_objectives.clear();
        Ok(())
    }

    fn resolve_place(&self, place: &Place) -> Option<Vec2> {
        let route = self.campaign.as_ref().map(|c| &c.route);
        Some(match place {
            Place::World(p) => *p,
            Place::RouteFraction(f) => route_point(&self.canyon_spec.as_ref()?.route, *f)?,
            Place::CourseStart => self.canyon_spec.as_ref()?.start,
            Place::CourseGoal => self.canyon_spec.as_ref()?.goal,
            Place::ActSpawn => route.map_or(self.act.spawn, |r| Vec2::new(r.start.x as f32, r.start.z as f32)),
            Place::ActGoal => route.map_or(self.act.goal, |r| Vec2::new(r.exit.x as f32, r.exit.z as f32)),
            Place::RouteSite(index) => {
                let site = route?.sites.get(*index)?;
                Vec2::new(site.position.x as f32, site.position.z as f32)
            }
            Place::EncounterRole(role) => self.campaign.as_ref()?.plan.encounters.iter()
                .find(|e| snake(&format!("{:?}", e.role)) == *role)?.anchor,
        })
    }

    pub fn mission_enabled(&self) -> bool { self.mission.is_some() }
    pub fn mission_finished(&self) -> bool { self.mission.as_ref().is_some_and(|m| m.finished) }
    pub fn mission_report(&self) -> Option<serde_json::Value> { self.mission.as_ref().map(MissionState::report) }
    pub fn mission_overlay(&self) -> Option<mission::Overlay> { self.mission.as_ref().map(MissionState::overlay) }
    /// The current beat's number (from 1) and title.
    pub fn mission_current_beat(&self) -> Option<(usize, String)> {
        let m = self.mission.as_ref()?;
        let index = m.current()?;
        Some((index + 1, m.mission.beats[index].title.clone()))
    }
    pub fn mission_map_guidance(&self) -> Option<mission::MapGuidance> {
        let actor = self.mission_actor();
        self.mission.as_ref().map(|m| m.map_guidance(actor))
    }
    /// The march bearing set on the pocket compass (degrees), while a beat with a bearing is open.
    pub fn mission_march_bearing(&self) -> Option<f32> {
        let actor = self.mission_actor();
        self.mission.as_ref().and_then(|m| m.march_bearing_deg(actor))
    }

    fn mission_actor(&self) -> Vec2 {
        if self.mode == Mode::Foot { Vec2::new(self.foot.pos.x, self.foot.pos.z) }
        else { Vec2::new(self.player.pos.x, self.player.pos.z) }
    }

    fn mission_observation(&self, r: &Renderer, input: &TankInput) -> Observation {
        let tiger = Vec2::new(self.player.pos.x, self.player.pos.z);
        let hull = self.player.forward();
        let view = r.camera.forward();
        let route_fraction = self.canyon_run.as_ref()
            .map_or(0.0, |run| crate::canyon_director::route_progress(&run.spec.route, tiger));
        let specs = crate::campaign::objective_specs(self.act.id).ok();
        let objectives_complete = specs.map(|specs| specs.iter()
            .filter(|spec| self.mission_objectives.iter().any(|(id, n)| *id == spec.id && *n >= spec.target))
            .map(|spec| format!("{:?}", spec.id)).collect()).unwrap_or_default();
        Observation {
            actor: self.mission_actor(),
            tiger,
            alive: self.player.alive && self.game_over().is_none(),
            on_foot: self.mode == Mode::Foot,
            hull_forward: Vec2::new(hull.x, hull.z),
            view_forward: Vec2::new(view.x, view.z),
            route_fraction,
            dead_tanks: self.enemies.iter().zip(&self.enemy_active)
                .filter(|(e, active)| **active && !e.alive).map(|(e, _)| Vec2::new(e.pos.x, e.pos.z)).collect(),
            silenced_guns: self.at_guns.iter().flatten().filter(|g| !g.working())
                .map(|g| Vec2::new(g.pos.x, g.pos.z)).collect(),
            live_allies: self.allies.iter().filter(|a| a.tank.alive).map(|a| Vec2::new(a.tank.pos.x, a.tank.pos.z)).collect(),
            allies_total: self.allies.len(),
            course_finished: self.canyon_finished().is_some(),
            objectives_complete,
            moved: input.drive.abs() > 0.2 || input.steer.abs() > 0.2,
            wind: r.weather_wind_world * self.weather_wind_scale,
        }
    }

    /// Each simulation step after the directors: observe, advance the beats, carry out requests.
    pub(super) fn update_mission(&mut self, dt: f32, r: &mut Renderer, input: &TankInput) {
        if self.mission.is_none() { return; }
        // The campaign director's credits this step, before the front end drains them.
        if let Some(runtime) = &self.campaign {
            for signal in &runtime.signals {
                if let MissionSignal::Objective(id, amount) = signal {
                    match self.mission_objectives.iter_mut().find(|(k, _)| k == id) {
                        Some((_, n)) => *n = n.saturating_add(*amount),
                        None => self.mission_objectives.push((*id, *amount)),
                    }
                }
            }
        }
        let obs = self.mission_observation(r, input);
        let requests = self.mission.as_mut().unwrap().step(dt, &obs);
        for request in requests { self.apply_mission_request(request, r); }
        for line in self.mission.as_mut().unwrap().take_log() { println!("{line}"); }
        self.drive_mission_flares(r);
    }

    fn apply_mission_request(&mut self, request: Request, r: &mut Renderer) {
        match request {
            Request::Release(release) => {
                let sent = self.mission_release_platoon(release.count, release.fraction, r);
                println!("[mission] release of {} at {:?}: {}", release.count, release.fraction,
                    if sent { "admitted" } else { "no safe entry, deferred" });
            }
            Request::Alert { at, radius_m } => self.mission_alert(at, radius_m),
            Request::LiveCap(cap) => {
                if let Some(d) = self.canyon_run.as_mut().and_then(|run| run.director.as_mut()) {
                    d.live_cap = cap.unwrap_or(crate::canyon_director::MAX_LIVE).min(crate::canyon_director::MAX_LIVE);
                }
            }
            Request::RadioKey => {
                if let Some(audio) = &self.audio { audio.play_control_at("radio_key", 0.40, 20.0); }
            }
            Request::FireFlare { at, color } => {
                let ground = Vec3::new(at.x, self.hf.height_at(at.x, at.y), at.y);
                let wind = r.weather_wind_world * self.weather_wind_scale;
                let seed = self.ai_tick as u64 ^ (at.x.to_bits() as u64) << 20 ^ (self.mission.as_ref().unwrap().elapsed_s.to_bits() as u64) << 40;
                self.mission.as_mut().unwrap().launch_flare(ground, color, wind, seed);
            }
            Request::LightSmoke { at, height_m, duration_s } => {
                let base = Vec3::new(at.x, self.hf.height_at(at.x, at.y), at.y);
                self.mission.as_mut().unwrap().light_smoke(base, height_m, duration_s);
            }
        }
    }

    /// Enemies within `radius_m` of `at` have heard the Tiger: their platoons search its last
    /// known area (the stealth model's own Searching state, with its own timeout) and the guns'
    /// crews stand to. No one is given a fix the model would not give.
    fn mission_alert(&mut self, at: Vec2, radius_m: f32) {
        let center = self.player.center();
        let mut platoons: Vec<usize> = Vec::new();
        for (i, enemy) in self.enemies.iter().enumerate() {
            if !self.enemy_active[i] || !enemy.alive { continue; }
            if Vec2::new(enemy.pos.x, enemy.pos.z).distance(at) > radius_m { continue; }
            let pi = self.stealth.platoon_index_of(i);
            if !platoons.contains(&pi) { platoons.push(pi); }
        }
        for pi in &platoons {
            let platoon = &mut self.stealth.platoons[*pi];
            if matches!(platoon.state, stealth::AlertState::Unaware) {
                platoon.state = stealth::AlertState::Searching;
                platoon.pending = false;
                platoon.search_timer = stealth::SEARCH_TIMEOUT;
                platoon.last_known = center;
                println!("[mission] platoon {pi} alerted -> Searching");
            }
        }
        let origin = Vec3::new(at.x, self.hf.height_at(at.x, at.y) + 1.1, at.y);
        self.at_guns_hear(origin, radius_m);
    }

    /// The burning stars as the scene's point lights (modeled emitters with a true source size).
    pub(super) fn mission_lights(&self) -> Vec<PointLight> {
        let Some(m) = &self.mission else { return Vec::new() };
        m.flares.iter().filter(|f| f.burning()).map(|f| PointLight {
            pos: f.pos,
            source_radius: mission::FLARE_SOURCE_RADIUS_M,
            color: f.color.color(),
            radiant_intensity: mission::FLARE_RADIANT_INTENSITY * f.brightness(),
        }).collect()
    }

    /// The stars' grains: cloned emissive cubes, placed each step while a star burns.
    fn drive_mission_flares(&mut self, r: &mut Renderer) {
        let Some(m) = &self.mission else { return };
        let burning: Vec<(Vec3, f32)> = m.flares.iter().filter(|f| f.burning()).map(|f| (f.pos, f.brightness())).collect();
        while self.mission_flare_slots.len() < burning.len().min(4) {
            let slot = r.clone_actor_mesh(self.slots.mg_impact[0]);
            self.mission_flare_slots.push(slot);
        }
        for (k, &slot) in self.mission_flare_slots.iter().enumerate() {
            match burning.get(k) {
                Some(&(pos, brightness)) => {
                    r.set_visible(slot, true);
                    r.set_emissive(slot, FLARE_GRAIN_EMISSIVE * brightness.max(0.05));
                    r.set_model(slot, Mat4::from_scale_rotation_translation(Vec3::splat(FLARE_GRAIN_M), Quat::IDENTITY, pos));
                }
                None => r.set_visible(slot, false),
            }
        }
    }

    /// The stars' smoke (the launch trail, then the burning composition's thread) and the smoke
    /// columns, as the volumetric pool's media. Called after the pool is cleared each step.
    pub(super) fn mission_smoke(&self, r: &mut Renderer) {
        let Some(m) = &self.mission else { return };
        let wind = r.weather_wind_world * self.weather_wind_scale;
        for flare in &m.flares {
            if flare.age_s < 4.0 {
                // The launch: a thread of propellant smoke along the climb.
                if let (Some(first), Some(last)) = (flare.trail.first(), flare.trail.last()) {
                    r.add_smoke_sweep(*first + Vec3::new(wind.x, 0.0, wind.y) * flare.age_s * 0.5, *last, 2.4 + flare.age_s * 0.6,
                        0.30 / (1.0 + flare.age_s * 0.5), flare.age_s, 0.80);
                }
            }
            if flare.burning() {
                // The burning star leaves a thin white thread above itself, drifting off downwind.
                let tail = flare.pos + Vec3::Y * 6.0 + Vec3::new(wind.x, 0.8, wind.y) * 5.0;
                r.add_smoke_sweep(flare.pos + Vec3::Y * 0.6, tail, 2.2, 0.34, flare.age_s * 0.7, 0.86);
            }
        }
        for column in &m.smoke {
            let strength = (column.age_s / 20.0).clamp(0.0, 1.0) * (1.0 - column.age_s / column.duration_s).clamp(0.0, 1.0);
            if strength <= 0.0 { continue; }
            let lean = Vec3::new(wind.x, 0.0, wind.y) / SMOKE_RISE_M_S;
            let height = column.height_m * (0.35 + 0.65 * strength);
            let point = |f: f32| column.base + Vec3::Y * height * f + lean * height * f * f * 0.5;
            let roll = column.age_s * 0.13;
            r.add_smoke_sweep(point(0.0) + Vec3::Y * 1.5, point(0.33), 3.5 + 4.0 * strength, 0.34 * strength, roll, 0.08);
            r.add_smoke_sweep(point(0.33), point(0.66), 6.0 + 6.0 * strength, 0.20 * strength, roll + 1.9, 0.10);
            r.add_smoke_sweep(point(0.66), point(1.0), 9.0 + 9.0 * strength, 0.10 * strength, roll + 3.7, 0.14);
        }
    }

    /// Capture and test fixture (disclosed where used): open beat `index` now, its predecessors
    /// marked done without credit, so a cue can be staged without driving the course.
    pub fn debug_mission_open_beat(&mut self, index: usize, r: &mut Renderer) -> anyhow::Result<()> {
        let m = self.mission.as_mut().ok_or_else(|| anyhow::anyhow!("no mission enabled"))?;
        anyhow::ensure!(index < m.beats.len(), "no beat {index}");
        m.debug_open(index);
        let obs = self.mission_observation(r, &TankInput::default());
        let requests = self.mission.as_mut().unwrap().step(0.0, &obs);
        for request in requests { self.apply_mission_request(request, r); }
        for line in self.mission.as_mut().unwrap().take_log() { println!("{line}"); }
        Ok(())
    }

    /// Capture fixture: fire a star of `color` from `at` now.
    pub fn debug_mission_flare(&mut self, at: Vec2, color: FlareColor, r: &mut Renderer) {
        if self.mission.is_none() { return; }
        self.apply_mission_request(Request::FireFlare { at, color }, r);
    }
    /// Capture fixture: light a smoke column at `at` now, already well established.
    pub fn debug_mission_smoke(&mut self, at: Vec2, height_m: f32, age_s: f32, r: &mut Renderer) {
        if self.mission.is_none() { return; }
        self.apply_mission_request(Request::LightSmoke { at, height_m, duration_s: 1800.0 }, r);
        if let Some(column) = self.mission.as_mut().and_then(|m| m.smoke.last_mut()) { column.age_s = age_s; }
    }
    /// Capture fixture: fold the order sheet away now.
    pub fn debug_mission_fold_order(&mut self) {
        if let Some(m) = self.mission.as_mut() { m.debug_fold_order(); }
    }
    /// Capture fixture: say a line on the net now.
    pub fn debug_mission_say(&mut self, from: &str, text: &str) {
        if let Some(m) = self.mission.as_mut() { m.debug_say(from, text); }
    }
    pub fn mission_flare_positions(&self) -> Vec<Vec3> {
        self.mission.as_ref().map(|m| m.flares.iter().filter(|f| f.burning()).map(|f| f.pos).collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_fractions_follow_arc_length() {
        let route = [Vec2::ZERO, Vec2::new(100.0, 0.0), Vec2::new(100.0, 300.0)];
        assert_eq!(route_point(&route, 0.0), Some(Vec2::ZERO));
        assert_eq!(route_point(&route, 0.25), Some(Vec2::new(100.0, 0.0)));
        assert_eq!(route_point(&route, 0.5), Some(Vec2::new(100.0, 100.0)));
        assert_eq!(route_point(&route, 1.0), Some(Vec2::new(100.0, 300.0)));
        assert_eq!(route_point(&[], 0.5), None);
        assert_eq!(snake("BlockingForce"), "blocking_force");
        assert_eq!(snake("Boss"), "boss");
    }

    #[test]
    fn the_authored_sheets_parse_and_name_only_places_the_maps_have() {
        for (path, beats) in [("assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json", 6), ("assets/worldgen/act1/mission.v1.json", 6)] {
            let sheet = mission::load(std::path::Path::new(path)).unwrap();
            assert_eq!(sheet.beats.len(), beats, "{path}");
            assert_eq!(sheet.order.len(), 3, "{path}: the order is three lines");
            // The Act One sheet keeps the campaign wordless on the air; the Canyon has its net.
            let on_air = sheet.beats.iter().flat_map(|b| b.open.iter().chain(&b.done).chain(&b.nudge)).count();
            if path.contains("act1") { assert_eq!(on_air, 0); } else { assert!(on_air >= 10); }
        }
        let canyon = mission::load(std::path::Path::new("assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json")).unwrap();
        let edits: serde_json::Value = serde_json::from_slice(&std::fs::read("assets/worldgen/workshops/gaea-canyon-run-v2/manifest_edits.json").unwrap()).unwrap();
        let spec = crate::canyon::from_edits(Some(&edits)).unwrap().unwrap();
        let resolve = |place: &Place| Some(match place {
            Place::World(p) => *p,
            Place::RouteFraction(f) => route_point(&spec.route, *f)?,
            Place::CourseStart => spec.start,
            Place::CourseGoal => spec.goal,
            _ => return None,
        });
        let state = MissionState::new(canyon, &resolve).unwrap();
        // Beats lie along the course in order, within the playable corridor.
        let fractions: Vec<f32> = state.beats.iter().map(|b| crate::canyon_director::route_progress(&spec.route, b.at)).collect();
        assert!(fractions.windows(2).all(|w| w[1] > w[0]), "{fractions:?}");
        assert!(state.beats.last().unwrap().at.distance(spec.goal) < 1.0);
        for b in &state.beats { assert!(b.at.x > -1150.0 && b.at.x < 2000.0 && b.at.y > -920.0 && b.at.y < 1120.0); }
    }
}
