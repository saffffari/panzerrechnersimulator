//! Space traverses the open aperture. LMB selection of the articulated lid stays independent.
use super::*;
use hatch_traversal::{Aperture, choose_target, exit_offsets, target_score};

impl Game {
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
            || self.switch_t > 0.0 || self.couch_crew_active || self.lan_local_role.is_some()
            || self.carry_mesh.is_some() || self.carried_casing.is_some()
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
            let aperture = self.access_aperture(index);
            let Some(score) = target_score(r.camera.pos, r.camera.forward(),
                entering.then_some(self.foot.pos), aperture, entering) else { continue; };
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
                    contacts.sweep_batch(&[[base + Vec3::Y * 0.04,
                        base + Vec3::Y * crate::foot::STANDING_HEIGHT]])[0].is_none()
                });
            if trace { println!("[hatch-landing] {index} feet={feet:?} clear={clear}"); }
            if clear { return Some(feet); }
        }
        None
    }

    pub(super) fn start_hatch_passage(&mut self, index: usize, r: &Renderer) -> bool {
        let entering = self.mode == Mode::Foot;
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
        let Some(index) = self.space_hatch_target(r) else {
            self.blocked_hatch_exit = None;
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

    /// Capture-only head direction, never movement, opening a lid, or entering a station.
    pub fn debug_face_hatch_aperture(&mut self, index: usize) -> bool {
        if index >= 5 || self.climb.active { return false; }
        let point = self.access_hatch_center(index);
        if self.mode == Mode::Foot { self.dbg_look_foot_at(point); return true; }
        let to = (point - self.station_eye(self.station)).normalize_or_zero();
        let base = self.player.turret_forward();
        if matches!(self.station, Station::Loader | Station::Commander) {
            self.head_yaw = to.x.atan2(-to.z) - base.x.atan2(-base.z);
            self.head_pitch = if self.station == Station::Commander {
                (to.y.asin() + 0.10).clamp(-0.5, 0.7)
            } else { (to.y.asin() - base.y.asin()).clamp(-1.2, 1.0) };
        }
        // Front seats use their own reachable hatch when seated; aiming vertically through
        // their restricted optic is not required to get out.
        true
    }
}
