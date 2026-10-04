//! Space traverses the open aperture. LMB selection of the articulated lid stays independent.
use super::*;
use hatch_traversal::{Aperture, choose_target, exit_offsets, target_score};

impl Game {
    /// Local identity is not a seat lock. Only another living crewman occupying the destination
    /// post blocks a hatch; the same mask protects ordinary interior station transitions.
    pub(super) fn hatch_entry_post_available(&self, index: usize) -> bool {
        if index >= 5 { return false; }
        let role = match Self::access_hatch_station(index) {
            Station::Commander => crate::lan::LanRole::Commander,
            Station::Driver => crate::lan::LanRole::Driver,
            Station::Gunner => crate::lan::LanRole::Gunner,
            Station::Loader => crate::lan::LanRole::Loader,
            Station::Radioman => crate::lan::LanRole::Radio,
        };
        self.lan_post_mask & (1 << role as u8) == 0
    }

    /// Outside, Space is an ordinary jump unless the player aims at a reachable open hatch.
    /// Tank proximity alone never starts an assisted climb or consumes the jump.
    pub(super) fn try_space_traversal(&mut self, r: &Renderer) -> bool {
        // A dismounted man at an untaken spare Tiger climbs it by the existing mount ritual.
        // His own living Tiger's aimed hatch keeps priority; a lost Tiger never blocks this.
        if self.spare_mount_target(r).is_some()
            && (!self.player.alive || self.space_hatch_target(r).is_none())
        {
            self.toggle_dismount();
            self.consume_primary_press();
            return true;
        }
        if self.climb.active || !self.player.alive || self.activity.is_some()
            || self.switch_t > 0.0 || self.carried_casing.is_some()
        { return false; }
        // The commander leaves by HOLDING Space with the lid open (the shell runs the hold and its
        // ring). A tap does nothing here, and never opens the lid; Q drops back into the turret.
        if self.mode == Mode::Tank && self.station == Station::Commander {
            self.hint_cupola_closed();
            return true;
        }
        if self.mode == Mode::Tank && self.station == Station::Gunner {
            if !self.hatch_entry_post_available(1) {
                self.hover_label = Some("TURRET CABIN OCCUPIED - CREWMAN BLOCKS THE WAY");
                return true;
            }
            self.sights_mag = 0;
            self.commander_outside = false;
            self.jump_station(Station::Loader);
            self.consume_primary_press();
            return true;
        }
        if let Some(index) = self.space_hatch_target(r) {
            self.start_hatch_passage(index, r);
            return true;
        }
        false
    }

    /// The untaken spare Tiger a living dismounted man stands at and faces. Proximity alone does
    /// not take the jump: he must be looking at the hull he means to climb.
    pub(super) fn spare_mount_target(&self, r: &Renderer) -> Option<usize> {
        if self.mode != Mode::Foot || self.climb.active || self.foot.hp <= 0.0
            || self.activity.is_some() || self.switch_t > 0.0 || self.carried_casing.is_some()
        { return None; }
        let spare = self.nearest_spare(6.0)?;
        faces_spare_hull(r.camera.pos, r.camera.forward(), self.abandoned[spare].0).then_some(spare)
    }

    pub(super) fn natural_hatch_station(&self, index: usize) -> Option<Station> {
        natural_hatch_station(index, self.lan_post_mask)
    }

    fn access_aperture(&self, index: usize) -> Aperture {
        Aperture {
            center: self.access_hatch_center(index),
            normal: self.access_hatch_frame(index).transform_vector3(if index < 4 {
                tiger_hatches::APERTURE_NORMALS[index]
            } else { Vec3::Y }).normalize(),
            radius: if index < 4 { tiger_hatches::APERTURE_RADII[index] } else { 0.25 },
        }
    }

    pub(super) fn hatch_passage_camera_inside(&self, eye: Vec3) -> bool {
        let aperture = self.access_aperture(self.last_access_hatch);
        (eye - aperture.center).dot(aperture.normal) < 0.0
    }

    pub(super) fn space_hatch_target(&self, r: &Renderer) -> Option<usize> {
        if self.climb.active || !self.player.alive || self.activity.is_some()
            || self.switch_t > 0.0
            || self.carried_casing.is_some()
            || self.player.speed.abs() >= 0.6 { return None; }
        let entering = self.mode == Mode::Foot;
        let preferred = if entering { None } else {
            match self.station {
                Station::Driver => Some(0), Station::Radioman => Some(2),
                Station::Commander => Some(4), _ => None,
            }
        };
        let models = self.fire_surface_models();
        let contacts = self.cabin_fire.source_contacts(&models);
        let mut candidates = Vec::with_capacity(5);
        for index in 0..5 {
            if !self.access_hatch_open(index) { continue; }
            if entering && !self.hatch_entry_post_available(index) { continue; }
            let aperture = self.access_aperture(index);
            let Some(score) = target_score(r.camera.pos, r.camera.forward(),
                entering.then_some(self.foot.pos), aperture, entering) else { continue; };
            if entering && !score.aimed { continue; }
            // A near face of the opening must be visible. Do not hit-test the open lid's new
            // location, or require a standing body to fit a crawl-sized opening.
            let near = aperture.center + aperture.normal * if entering { 0.07 } else { -0.07 };
            if contacts.sweep_batch(&[[r.camera.pos, near]])[0].is_some() { continue; }
            candidates.push((index, score));
        }
        choose_target(&candidates, preferred)
    }

    /// Find source-supported feet beside this particular hatch, with standing headroom. The
    /// side door can lead onto the hull deck or terrain; roof exits must stay on the tank.
    fn hatch_exit_landing(&self, index: usize) -> Option<Vec3> {
        let aperture = self.access_aperture(index);
        let models = self.fire_surface_models();
        let contacts = self.cabin_fire.source_contacts(&models);
        let trace = std::env::var_os("KESSEL_HATCH_TRACE").is_some();
        for offset in exit_offsets(aperture.normal, index == 3) {
            let sample = aperture.center + offset;
            let bottom = self.hf.height_at(sample.x, sample.z);
            let from = sample + Vec3::Y * 0.65;
            let to = Vec3::new(sample.x, bottom - 0.05, sample.z);
            let feet = if let Some(hit) = contacts.sweep_batch(&[[from, to]])[0] {
                if trace { println!("[hatch-landing] {index} offset={offset:?} frame={} feet={:?}", hit.frame, models[hit.frame].transform_point3(hit.point)); }
                if matches!(hit.frame, 3 | 4 | 6 | 14 | 15) { continue; } // not on an open lid
                let normal = models[hit.frame].transform_vector3(hit.normal).normalize();
                if normal.y < 0.65 { continue; }
                models[hit.frame].transform_point3(hit.point)
            } else if index == 3 {
                Vec3::new(sample.x, bottom, sample.z)
            } else { continue; };
            if index != 3 && (feet - aperture.center).dot(aperture.normal) < -0.40 { continue; }
            let clear = [Vec3::ZERO, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z]
                .into_iter().all(|offset| {
                    let base = feet + offset * crate::foot::BODY_RADIUS;
                    let from = base + Vec3::Y * 0.04;
                    contacts.sweep_batch(&[[from,
                        base + Vec3::Y * crate::foot::STANDING_HEIGHT]])[0].is_none()
                        && self.nearest_cover_ray(from, Vec3::Y,
                            crate::foot::STANDING_HEIGHT - 0.04).is_none()
                });
            if trace { println!("[hatch-landing] {index} feet={feet:?} clear={clear}"); }
            if clear { return Some(feet); }
        }
        None
    }

    pub(super) fn start_hatch_passage(&mut self, index: usize, r: &Renderer) -> bool {
        let entering = self.mode == Mode::Foot;
        if index >= 5 || !self.access_hatch_open(index) || self.climb.active
            || (entering && !self.hatch_entry_post_available(index)) { return false; }
        let aperture = self.access_aperture(index);
        let exit_feet = if entering { None } else { self.hatch_exit_landing(index) };
        if !entering && exit_feet.is_none() {
            self.blocked_hatch_exit = Some(index);
            self.hover_label = Some("HATCH EXIT BLOCKED - CLEAR THE LANDING");
            println!("[hatch] exit {index} blocked: no supported landing with headroom");
            return false;
        }
        let inner = aperture.center - aperture.normal * 0.22;
        let outer = aperture.center + aperture.normal * 0.25;
        let mut cams = vec![r.camera.pos];
        if entering {
            // Move over a roof hole before lowering through it, instead of cutting diagonally
            // through the roof. A side opening uses the same two sides of its authored plane.
            if index != 3 {
                cams.push(aperture.center + aperture.normal
                    * (r.camera.pos - aperture.center).dot(aperture.normal).max(0.40));
            }
            cams.extend([outer, inner, self.station_eye(Self::access_hatch_station(index))]);
        } else {
            let destination = exit_feet.unwrap() + Vec3::Y * crate::foot::EYE;
            // A commander who already has his head out continues upward, never ducks back in.
            if (r.camera.pos - aperture.center).dot(aperture.normal) < 0.0 {
                cams.push(inner);
                cams.push(outer);
            } else if index == 3 { cams.push(outer); }
            if index != 3 {
                cams.push(aperture.center + aperture.normal
                    * (destination - aperture.center).dot(aperture.normal).max(0.40));
            }
            cams.push(destination);
        }
        self.last_access_hatch = index;
        self.blocked_hatch_exit = None;
        self.foot.yaw = r.camera.yaw;
        self.foot.pitch = r.camera.pitch;
        self.foot.pos = r.camera.pos - Vec3::Y * crate::foot::EYE;
        self.foot.airborne = false;
        self.foot.vel_y = 0.0;
        self.foot.crouch_t = 0.0;
        self.mode = Mode::Foot;
        self.gunner_optic = false;
        self.sights_mag = 0;
        self.commander_outside = false;
        self.driver_hatch = false;
        self.climb = Climb { active: true, mounting: entering, step: 0, t: 0.0,
            anim: true, automatic: true, exit_feet, cams,
            end: if entering { ClimbEnd::IntoTank } else { ClimbEnd::Afoot } };
        self.hover_roof_hatch = None;
        self.hover_hatch = false;
        self.hover_lever = false;
        self.hover_station = None;
        self.hover = None;
        self.hover_label = None;
        self.consume_primary_press();
        println!("[hatch] Space {} via aperture {index}", if entering { "enter" } else { "exit" });
        true
    }

    pub(super) fn finish_hatch_passage_view(&mut self) {
        self.commander_outside = false;
        self.driver_hatch = false;
        self.driver_visor_lean = 0.0;
        self.switch_t = 0.0;
        let base = self.player.turret_forward();
        self.head_yaw = self.foot.yaw - base.x.atan2(-base.z);
        self.head_pitch = if self.station == Station::Commander {
            (self.foot.pitch + 0.10).clamp(-0.5, 0.7)
        } else { (self.foot.pitch - base.y.asin()).clamp(-1.2, 1.0) };
    }

    pub(super) fn update_hatch_access_prompt(&mut self, r: &Renderer) {
        // Preserve LMB equipment/handle prompts; the opening has its own Space hint.
        if self.hover_roof_hatch.is_some() || self.hover_lever || self.hover.is_some() { return; }
        if self.mode == Mode::Tank && self.station == Station::Commander
            && !self.commander_outside && !self.climb.active {
            self.hover_label = Some("DESCEND INTO TURRET CABIN [SPACE]");
            return;
        }
        let Some(index) = self.space_hatch_target(r) else {
            self.blocked_hatch_exit = None;
            if self.spare_mount_target(r).is_some() {
                self.hover_label = Some("CLIMB ABANDONED TIGER [SPACE]");
            }
            return;
        };
        if self.blocked_hatch_exit == Some(index) {
            self.hover_label = Some("HATCH EXIT BLOCKED - CLEAR THE LANDING");
            return;
        }
        self.hover_label = Some(match (self.mode == Mode::Foot, index) {
            (true, 0) => "ENTER DRIVER HATCH [SPACE]",
            (true, 1) => "ENTER LOADER HATCH [SPACE]",
            (true, 2) => "ENTER RADIO HATCH [SPACE]",
            (true, 3) => "ENTER SIDE ESCAPE HATCH [SPACE]",
            (true, _) => "ENTER CUPOLA [SPACE]",
            (false, 0) => "EXIT DRIVER HATCH [SPACE]",
            (false, 1) => "EXIT LOADER HATCH [SPACE]",
            (false, 2) => "EXIT RADIO HATCH [SPACE]",
            (false, 3) => "EXIT SIDE ESCAPE HATCH [SPACE]",
            (false, _) => "EXIT CUPOLA [SPACE]",
        });
    }

    /// Aim at the actual moving roof lid rather than the hole left behind after it opens.
    /// The source aperture centre is transformed by the current authored lid hinge pose; ordinary
    /// source-triangle hover and LMB must still find and operate the visible assembly.
    pub fn debug_face_front_hatch_lid(&mut self, index: usize) -> bool {
        if !matches!(index, 0 | 2) || self.climb.active { return false; }
        let hull = self.player.hull_model();
        let model = self.roof_hatches.models(hull, self.player.turret_model())[index];
        let point = model.transform_point3(tiger_hatches::APERTURE_CENTERS[index]);
        if self.mode == Mode::Foot { self.dbg_look_foot_at(point); return true; }
        self.debug_face_hull_point(hull.inverse().transform_point3(point))
    }

    /// Capture-only head direction, never movement, opening a lid, or entering a station.
    pub fn debug_face_hatch_aperture(&mut self, index: usize) -> bool {
        if index >= 5 || self.climb.active { return false; }
        let point = self.access_hatch_center(index);
        if self.mode == Mode::Foot { self.dbg_look_foot_at(point); return true; }
        if matches!(self.station, Station::Driver | Station::Radioman) {
            return self.debug_face_hull_point(self.player.hull_model().inverse().transform_point3(point));
        }
        let to = (point - self.station_eye(self.station)).normalize_or_zero();
        let base = self.player.turret_forward();
        if matches!(self.station, Station::Loader | Station::Commander) {
            self.head_yaw = to.x.atan2(-to.z) - base.x.atan2(-base.z);
            self.head_pitch = if self.station == Station::Commander {
                (to.y.asin() + 0.10).clamp(-0.5, 0.7)
            } else { (to.y.asin() - base.y.asin()).clamp(-1.2, 1.0) };
        }
        true
    }
}


/// Reconcile a completed physical crossing without taking another crewman's post. A dismounted
/// local player leaves at least one of the five posts available in the five-person crew model.
fn natural_hatch_station(index: usize, occupied: u8) -> Option<Station> {
    let preferred = Game::access_hatch_station(index);
    [preferred, Station::Loader, Station::Commander, Station::Gunner,
        Station::Driver, Station::Radioman].into_iter().find(|station| {
        let role = match station {
            Station::Commander => crate::lan::LanRole::Commander,
            Station::Driver => crate::lan::LanRole::Driver,
            Station::Gunner => crate::lan::LanRole::Gunner,
            Station::Loader => crate::lan::LanRole::Loader,
            Station::Radioman => crate::lan::LanRole::Radio,
        };
        occupied & (1 << role as u8) == 0
    })
}

/// Foot (yaw, pitch) that continues an in-tank head view: the exact inverse of
/// `Game::finish_hatch_passage_view`, which maps a foot view back onto the head.
pub(super) fn hatch_exit_foot_view(turret_forward: Vec3, head_yaw: f32, head_pitch: f32,
    commander: bool) -> (f32, f32) {
    let yaw = head_yaw + turret_forward.x.atan2(-turret_forward.z);
    let pitch = if commander { head_pitch - 0.10 }
        else { head_pitch + turret_forward.y.clamp(-1.0, 1.0).asin() };
    if yaw.is_finite() && pitch.is_finite() { (yaw, pitch.clamp(-1.4, 1.4)) } else { (0.0, 0.0) }
}

/// The view is turned toward the spare's hull (its ground origin, taken at deck height).
fn faces_spare_hull(eye: Vec3, forward: Vec3, spare_pos: Vec3) -> bool {
    let to = spare_pos + Vec3::Y * 1.5 - eye;
    to.normalize_or_zero().dot(forward.normalize_or_zero()) > 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climbing_out_keeps_the_head_view_and_round_trips_with_the_entry_mapping() {
        let base = (Quat::from_rotation_y(0.7) * Quat::from_rotation_x(-0.05) * Vec3::Z).normalize();
        for (head_yaw, head_pitch, commander) in [(0.0, 0.0, true), (1.2, 0.3, true),
            (-2.0, -0.2, false), (0.4, 0.5, false)] {
            let (yaw, pitch) = hatch_exit_foot_view(base, head_yaw, head_pitch, commander);
            // `finish_hatch_passage_view`, restated.
            let back_yaw = yaw - base.x.atan2(-base.z);
            let back_pitch = if commander { pitch + 0.10 } else { pitch - base.y.asin() };
            assert!((back_yaw - head_yaw).abs() < 1.0e-5 && (back_pitch - head_pitch).abs() < 1.0e-5);
        }
        // Looking straight along the turret gives the camera-convention yaw of that direction.
        let (yaw, _) = hatch_exit_foot_view(Vec3::X, 0.0, 0.0, true);
        assert!((Vec3::new(yaw.sin(), 0.0, -yaw.cos()) - Vec3::X).length() < 1.0e-5);
        assert_eq!(hatch_exit_foot_view(Vec3::NAN, 0.0, 0.0, false), (0.0, 0.0));
    }

    #[test]
    fn spare_mount_needs_the_view_on_the_hull_not_mere_proximity() {
        let spare = Vec3::new(10.0, 0.0, 0.0);
        let eye = Vec3::new(6.0, 1.7, 0.0);
        assert!(faces_spare_hull(eye, Vec3::X, spare));
        assert!(faces_spare_hull(eye, Vec3::new(1.0, 0.0, 0.5).normalize(), spare));
        assert!(!faces_spare_hull(eye, -Vec3::X, spare), "back turned: Space stays a jump");
        assert!(!faces_spare_hull(eye, Vec3::Z, spare));
        assert!(!faces_spare_hull(eye, Vec3::ZERO, spare));
        assert!(!faces_spare_hull(Vec3::NAN, Vec3::X, spare));
    }

    #[test]
    fn natural_open_hatch_entry_uses_a_free_post_even_when_the_hatch_post_is_occupied() {
        use crate::lan::LanRole;
        assert_eq!(natural_hatch_station(4, 0), Some(Station::Commander));
        assert_eq!(natural_hatch_station(4, 1 << LanRole::Commander as u8), Some(Station::Loader));
        for free in LanRole::ALL {
            let occupied = LanRole::ALL.into_iter().filter(|role| *role != free)
                .fold(0u8, |mask, role| mask | (1 << role as u8));
            assert!(natural_hatch_station(4, occupied).is_some(),
                "a dismounted player must retain a real vacant post: {free:?}");
        }
        assert_eq!(natural_hatch_station(4, 0xff), None);
    }

    fn press(game: &mut Game, renderer: &mut Renderer, jump: bool) {
        let mut crew = crate::lan::CrewFrame::default();
        crew.roster[crate::lan::LanRole::Commander as usize] = 11;
        crew.inputs[crate::lan::LanRole::Commander as usize] = crate::lan::RoleInput {
            post: match game.station {
                Station::Commander => crate::lan::LanRole::Commander,
                Station::Driver => crate::lan::LanRole::Driver,
                Station::Gunner => crate::lan::LanRole::Gunner,
                Station::Loader => crate::lan::LanRole::Loader,
                Station::Radioman => crate::lan::LanRole::Radio,
            },
            flags: if game.mode == Mode::Foot { crate::lan::F_DISMOUNTED } else { 0 },
            ..crate::lan::RoleInput::default()
        };
        let carrying = game.carry_mesh.is_some();
        game.update(&TankInput { jump, mg_coax: carrying, fire2: carrying,
            coop: Some(crew), coop_host: true, couch_crew: true,
            ..TankInput::default() }, 1.0 / 60.0, renderer);
    }

    fn finish(game: &mut Game, renderer: &mut Renderer) {
        for _ in 0..180 {
            if !game.climb.active { break; }
            press(game, renderer, false);
        }
        assert!(!game.climb.active, "one Space traversal must finish without another press");
    }

    #[test]
    #[ignore = "requires a GPU and source Tiger assets"]
    fn source_tiger_couch_space_jumps_outside_and_crosses_only_aimed_open_hatch()
        -> anyhow::Result<()> {
        use crate::tank_editor_scene::{TankAssetId, TankEditorScene};
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut renderer, mut game) = Game::build_tank_editor(
            &device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?;
        game.mute_audio();
        game.god = true;
        game.set_lan_role(crate::lan::LanRole::Commander);
        game.mode = Mode::Foot;
        let hull = game.player.hull_model();
        let ground = hull.transform_point3(Vec3::new(3.1, 0.0, 2.6));
        game.foot.pos = Vec3::new(ground.x, game.hf.height_at(ground.x, ground.z), ground.z);
        game.foot.airborne = false;
        game.foot.vel_y = 0.0;
        renderer.camera.pos = game.foot.eye();
        renderer.camera.set_forward(hull.transform_vector3(-Vec3::X));
        game.foot.yaw = renderer.camera.yaw;
        game.foot.pitch = renderer.camera.pitch;
        let ground_feet = game.foot.pos;
        press(&mut game, &mut renderer, true);
        assert!(!game.climb.active, "outside Space must not start an assisted climb");
        assert!(game.foot.airborne && game.foot.vel_y > 0.0, "outside Space must jump");
        assert!(game.foot.pos.y < ground_feet.y + 0.2, "one tick must not teleport onto the deck");
        assert!(!game.hatch_open, "climbing cannot operate the lid");
        assert_eq!(game.space_hatch_target(&renderer), None);
        assert!(!game.start_hatch_passage(4, &renderer), "closed cupola cannot be entered");

        // Place the fixture on the independently validated roof landing beside the cupola.
        // This bypasses walking only; aperture selection and passage still use shipping queries.
        game.last_access_hatch = 4;
        let landing = game.hatch_exit_landing(4).expect("cupola must have a supported roof landing");
        game.foot.pos = landing;
        renderer.camera.pos = game.foot.eye();
        renderer.camera.set_forward(game.access_hatch_center(4) - renderer.camera.pos);
        game.foot.yaw = renderer.camera.yaw;
        game.foot.pitch = renderer.camera.pitch;
        game.hover = None;
        game.update_hatch_hover(&renderer);
        assert_eq!(game.hover_roof_hatch, Some(4), "LMB must target the actual closed cupola lid");
        assert!(game.click_articulated_hatch(), "source lid click must open it");
        for _ in 0..90 { press(&mut game, &mut renderer, false); }
        assert!(game.access_hatch_open(4), "wait for the physical lid to finish opening");
        game.lan_post_mask = 1 << crate::lan::LanRole::Commander as u8;
        assert!(!game.hatch_entry_post_available(4));
        assert!(!game.start_hatch_passage(4, &renderer), "occupied post rejects entry");
        game.lan_post_mask = 0;
        assert_eq!(game.space_hatch_target(&renderer), Some(4));
        assert!(game.start_hatch_passage(4, &renderer));
        finish(&mut game, &mut renderer);
        assert!(game.mode == Mode::Tank);
        assert_eq!(game.station, Station::Commander);
        assert_eq!(game.lan_local_role, Some(crate::lan::LanRole::Commander));
        press(&mut game, &mut renderer, true);
        assert_eq!(game.station, Station::Loader, "buttoned cupola Space must descend into cabin");
        game.sync_lan_role(crate::lan::LanRole::Commander);
        assert_eq!(game.station, Station::Loader, "identity sync must not undo the descent");

        game.jump_station(Station::Commander);
        game.commander_outside = true;
        press(&mut game, &mut renderer, false);
        press(&mut game, &mut renderer, true);
        assert!(game.climb.active, "head-out Space must pull through the open cupola");
        finish(&mut game, &mut renderer);
        assert!(game.mode == Mode::Foot);
        assert!(game.foot.pos.y > ground_feet.y + 1.0);
        assert!(!game.commander_outside);

        // A previously opened lid permits the same route with a salvaged physical round in hand.
        // Fixture pickup supplies an existing external LooseShell; no round is invented by entry.
        game.foot.pos = ground_feet;
        game.foot.airborne = false;
        game.foot.vel_y = 0.0;
        renderer.camera.pos = game.foot.eye();
        renderer.camera.set_forward(hull.transform_vector3(-Vec3::X));
        game.foot.yaw = renderer.camera.yaw;
        game.foot.pitch = renderer.camera.pitch;
        let round_pos = renderer.camera.pos + renderer.camera.forward() * 0.65;
        game.loose[0] = LooseShell { world_space: true, pos: round_pos,
            ..LooseShell::default() };
        game.carry_mesh = Some(CarriedMesh::LooseShell(0));
        game.grab = true;
        game.set_carry_pose(round_pos, Quat::IDENTITY, Vec3::ZERO, Vec3::ZERO);
        press(&mut game, &mut renderer, false);
        press(&mut game, &mut renderer, true);
        assert!(!game.climb.active && game.foot.airborne,
            "carrying a round must retain the ordinary outside jump");
        assert!(matches!(game.carry_mesh, Some(CarriedMesh::LooseShell(0))));
        game.foot.pos = game.hatch_exit_landing(4).unwrap();
        renderer.camera.pos = game.foot.eye();
        renderer.camera.set_forward(game.access_hatch_center(4) - renderer.camera.pos);
        game.foot.yaw = renderer.camera.yaw;
        game.foot.pitch = renderer.camera.pitch;
        assert!(game.start_hatch_passage(4, &renderer));
        finish(&mut game, &mut renderer);
        assert!(matches!(game.carry_mesh, Some(CarriedMesh::LooseShell(0))));
        press(&mut game, &mut renderer, true);
        assert_eq!(game.station, Station::Loader);
        assert!(matches!(game.carry_mesh, Some(CarriedMesh::LooseShell(0))),
            "the same physical salvage round must arrive in the main turret cabin");
        println!("[hatch-space] ordinary jump/open cupola/cabin/exposed exit PASS");
        Ok(())
    }

    #[test]
    #[ignore = "requires a GPU and source Tiger assets"]
    fn held_round_excludes_portable_pickup_and_local_mg_without_blocking_remote_crew()
        -> anyhow::Result<()> {
        use crate::tank_editor_scene::{TankAssetId, TankEditorScene};
        use crate::lan::{CrewFrame, LanRole, RoleInput, F_FIRE_COAX};
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut renderer, mut game) = Game::build_tank_editor(
            &device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?;
        game.mute_audio();
        game.mode = Mode::Foot;
        game.foot.pos = Vec3::new(10.0, 0.0, 10.0);
        // The isolated editor intentionally has no world pickups. This state-only can fixture
        // exercises the real hover/interaction methods; its slot is never submitted for drawing.
        let can = game.pickups.len();
        game.pickups.push(Pickup { pos: game.foot.pos - Vec3::Z,
            slot: game.slots.luger, kind: PickupKind::Fuel, collected: false,
            portable: Some(PortableItem::full_jerry_can()) });
        let eye = game.foot.eye();
        let look = (game.pickups[can].pos + Vec3::Y * 0.31 - eye).normalize();
        assert_eq!(game.portable_item_hover(look, eye), Some(can));
        // Fixture holds an existing floor-round body. Neither guard may transfer its ownership.
        game.carry_mesh = Some(CarriedMesh::LooseShell(0));
        game.grab = true;
        assert_eq!(game.portable_item_hover(look, eye), None);
        assert!(!game.interact_portable_item(look, eye));
        assert!(game.held_item.is_none());
        assert!(matches!(game.carry_mesh, Some(CarriedMesh::LooseShell(0))));
        game.carry_mesh = None;
        game.grab = false;
        assert!(game.interact_portable_item(look, eye), "free hands must still pick up the can");
        assert_eq!(game.held_item, Some(can));
        assert!(game.interact_portable_item(look, eye));

        game.mode = Mode::Tank;
        game.jump_station(Station::Commander);
        game.commander_outside = true;
        game.hover_station = None;
        game.hover_hatch = false;
        game.primary_pickup_press = false;
        game.carry_mesh = Some(CarriedMesh::LooseShell(0));
        game.grab = true;
        let held = TankInput { mg_coax: true, fire2: true, ..TankInput::default() };
        let shots = game.mg_round;
        for _ in 0..30 { game.update_crew_vehicle_mg(&held, 1.0 / 60.0); }
        assert_eq!(game.mg_round, shots, "a shell-carry grip cannot pull the local hull MG trigger");
        game.carry_mesh = None;
        game.grab = false;
        for _ in 0..30 { game.update_crew_vehicle_mg(&held, 1.0 / 60.0); }
        assert!(game.mg_round > shots, "ordinary free-handed Commander MG remains usable");
        game.carry_mesh = Some(CarriedMesh::LooseShell(0));
        game.grab = true;
        let mut frame = CrewFrame::default();
        frame.roster[LanRole::Gunner as usize] = 22;
        frame.inputs[LanRole::Gunner as usize] = RoleInput {
            post: LanRole::Gunner, flags: F_FIRE_COAX, ..RoleInput::default()
        };
        let shots = game.mg_round;
        let remote = TankInput { coop: Some(frame), coop_host: true, ..TankInput::default() };
        for _ in 0..30 { game.update_crew_vehicle_mg(&remote, 1.0 / 60.0); }
        assert!(game.mg_round > shots, "local held round must not disable another crewman's coax");
        // Keep renderer alive for the source-scene fixture; no screenshot or native input claim.
        game.hide_p08(&mut renderer);
        println!("[held-round] exclusive hands, local MG grip isolation and remote coax authority PASS");
        Ok(())
    }

    #[test]
    #[ignore = "requires a GPU and source Tiger assets"]
    fn source_tiger_solo_physical_round_requires_flick_and_closed_breech()
        -> anyhow::Result<()> {
        use crate::tank_editor_scene::{TankAssetId, TankEditorScene};
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut renderer, mut game) = Game::build_tank_editor(
            &device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?;
        game.mute_audio();
        game.god = true;
        game.jump_station(Station::Loader);
        let tick = |game: &mut Game, renderer: &mut Renderer, held: bool, motion: Vec2| {
            game.update(&TankInput { mg_coax: held, fire2: held,
                look_dx: motion.x, look_dy: motion.y, ..TankInput::default() },
                1.0 / 60.0, renderer);
        };
        for direction in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
            // Each scenario starts with an empty, open chamber and one actual rack cartridge.
            // Only inventory and gaze are staged: pickup, spring carry, guided seating, lever
            // clicks and indication all advance through the ordinary solo Game::update path.
            assert!(game.debug_stage_last_reload_round());
            game.breech = Breech::Empty;
            game.lever_open = 1.0;
            game.ram = 0.0;
            game.debug_loader_face_rack();
            for _ in 0..3 { tick(&mut game, &mut renderer, false, Vec2::ZERO); }
            let Some(Hotspot::RackSlot(slot)) = game.hover else {
                panic!("actual nearest rack round must be targetable, got {:?}", game.hover);
            };
            tick(&mut game, &mut renderer, true, Vec2::ZERO);
            assert!(matches!(game.carry_mesh, Some(CarriedMesh::RackShell(i)) if i == slot));
            assert!(game.slot_taken[slot], "pickup must vacate the actual source rack slot");
            game.debug_loader_face_tray();
            for _ in 0..60 { tick(&mut game, &mut renderer, true, Vec2::ZERO); }
            let mouth = game.reload_mouth_world();
            let aim = renderer.camera.forward().dot((mouth - renderer.camera.pos).normalize());
            assert!(manual_ram_eligible(game.breech, game.lever_open,
                game.carry_pos.distance(mouth), aim),
                "fixture must reach the real mouth: distance={}, aim={aim}", game.carry_pos.distance(mouth));
            assert!(matches!(game.breech, Breech::Empty), "holding aligned for a second cannot load");
            assert_eq!(game.ram, 0.0);
            assert_eq!(game.breech_lamp_state(), 1, "open empty breech is amber");

            // Four short input deltas form one deliberate flick. Intermediate geometry must move
            // towards the source chamber; no test writes the carried pose or the load completion.
            tick(&mut game, &mut renderer, true, direction * 10.0);
            assert!(game.ram > 0.0 && game.ram < 1.0);
            let guided = game.carried_round_model();
            let seated = chambered_round_model(game.player.visual_gun_model());
            assert!(guided.transform_vector3(Vec3::Z).normalize()
                .dot(seated.transform_vector3(Vec3::Z).normalize()) > 0.995,
                "the loading guide must align the physical cartridge with the chamber");
            tick(&mut game, &mut renderer, true, direction * 10.0);
            assert!(game.carried_round_model().transform_point3(Vec3::ZERO)
                .distance(seated.transform_point3(Vec3::ZERO))
                < guided.transform_point3(Vec3::ZERO).distance(seated.transform_point3(Vec3::ZERO)),
                "the next gesture sample must advance the aligned cartridge into its chamber");
            for _ in 0..2 { tick(&mut game, &mut renderer, true, direction * 10.0); }
            for _ in 0..30 {
                if matches!(game.breech, Breech::Loaded) { break; }
                tick(&mut game, &mut renderer, true, Vec2::ZERO);
            }
            assert!(matches!(game.breech, Breech::Loaded), "physical round must seat after flick {direction:?}");
            assert!(game.carry_mesh.is_none() && !game.grab);
            assert!(game.lever_open >= LEVER_OPEN, "ramming cannot silently close the breech");
            assert_eq!(game.breech_lamp_state(), 1, "seated round with open block remains amber");
            assert!(!game.player_can_fire());
            game.debug_loader_face_lever();
            for _ in 0..3 { tick(&mut game, &mut renderer, false, Vec2::ZERO); }
            assert_eq!(game.hover, Some(Hotspot::GunLever), "visible lever must own this click");
            tick(&mut game, &mut renderer, true, Vec2::ZERO);
            assert!(game.lever_open <= LEVER_SHUT);
            assert!(matches!(game.breech, Breech::Loaded));
            assert_eq!(game.breech_lamp_state(), 0, "loaded and closed is green");
            assert!(game.player_can_fire());
        }
        println!("[physical-ram] source rack pickup, held-still rejection, four flick axes, amber→green PASS");
        Ok(())
    }

    #[test]
    #[ignore = "requires a GPU and source Tiger assets"]
    fn remote_crew_keep_vehicle_authority_and_cabin_view_while_local_body_is_outside()
        -> anyhow::Result<()> {
        use crate::tank_editor_scene::{TankAssetId, TankEditorScene};
        use crate::lan::{CrewFrame, LanRole, RoleInput, F_DISMOUNTED, F_FIRE_MAIN, F_SERVICE_BREECH};
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut renderer, mut game) = Game::build_tank_editor(
            &device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?;
        game.mute_audio();
        game.god = true;
        game.set_lan_role(LanRole::Commander);
        game.mode = Mode::Foot;
        game.foot.pos = Vec3::new(12.0, 0.0, 12.0);
        game.foot.airborne = false;
        game.engine_on = true;
        let mut frame = CrewFrame::default();
        frame.roster[LanRole::Commander as usize] = 11;
        frame.roster[LanRole::Gunner as usize] = 22;
        frame.inputs[LanRole::Commander as usize] = RoleInput {
            post: LanRole::Commander, flags: F_DISMOUNTED, ..RoleInput::default()
        };
        frame.inputs[LanRole::Gunner as usize] = RoleInput {
            post: LanRole::Gunner, flags: F_FIRE_MAIN, traverse: 1.0, ..RoleInput::default()
        };
        let tick = |game: &mut Game, renderer: &mut Renderer, frame: CrewFrame| {
            game.update(&TankInput { coop: Some(frame), coop_host: true, couch_crew: true,
                ..TankInput::default() }, 1.0 / 60.0, renderer);
        };
        let shots = game.main_gun_shot_sequence();
        let yaw = game.player.turret_yaw;
        for _ in 0..12 { tick(&mut game, &mut renderer, frame); }
        assert!(game.mode == Mode::Foot);
        assert_eq!(game.main_gun_shot_sequence(), shots + 1,
            "living remote Gunner fires once even while keyboard Commander is outside");
        assert!((game.player.turret_yaw - yaw).abs() > 0.001);
        game.configure_crew_render_view(LanRole::Gunner, false, &mut renderer);
        assert_eq!(renderer.interior_light, 1.0);
        assert!(renderer.interior_haze > 0.0);
        assert_eq!(renderer.rt_mix, 0.0);
        game.configure_crew_render_view(LanRole::Commander, false, &mut renderer);
        assert_eq!(renderer.interior_light, 0.0, "dismounted local body must stay exterior");

        // The human reload control may extract a spent case, but holding still cannot seat a round.
        frame.inputs[LanRole::Gunner as usize].flags = F_SERVICE_BREECH;
        frame.inputs[LanRole::Gunner as usize].traverse = 0.0;
        frame.inputs[LanRole::Loader as usize] = frame.input(LanRole::Gunner);
        for _ in 0..180 { tick(&mut game, &mut renderer, frame); }
        assert!(matches!(game.breech, Breech::Empty));
        assert_eq!(game.ram, 0.0, "holding a human service control must not auto-ram");
        frame.inputs[LanRole::Loader as usize].look_dy = -35.0;
        tick(&mut game, &mut renderer, frame);
        assert!(matches!(game.breech, Breech::Loaded), "any-direction flick seats the human round");

        // The same second human can take Driver; do not invent a third local player.
        frame.roster[LanRole::Gunner as usize] = 0;
        frame.inputs[LanRole::Gunner as usize] = RoleInput::default();
        frame.inputs[LanRole::Loader as usize] = RoleInput::default();
        frame.roster[LanRole::Driver as usize] = 22;
        frame.inputs[LanRole::Driver as usize] = RoleInput {
            post: LanRole::Driver, drive: 0.8, ..RoleInput::default()
        };
        let position = game.player.pos;
        for _ in 0..90 { tick(&mut game, &mut renderer, frame); }
        assert!(game.player.pos.distance(position) > 0.15,
            "living remote Driver must move the tank while the keyboard body is outside");
        assert!(game.mode == Mode::Foot);
        frame.inputs[LanRole::Driver as usize].flags = F_DISMOUNTED;
        tick(&mut game, &mut renderer, frame);
        assert_eq!(game.drive_cmd, 0.0, "dismounted driver's stale input must be rejected");
        println!("[couch-outside] remote Gunner/Driver, protected cabin and human ram gesture PASS");
        Ok(())
    }
}
