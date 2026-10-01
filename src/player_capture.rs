//! Read-only geometry queries reused from the validated player-input recorder.
//! They do not move actors, reload ammunition, aim the gun or alter combat.
use super::*;
impl Game {
    pub fn capture_walk_position(&self) -> Vec3 {
        self.walk_pos
    }
    pub fn capture_gaze_target(&self, step: &serde_json::Value) -> Option<Vec3> {
        let tm = self.player.turret_model();
        if let Some(a) = step["gaze_local"].as_array() {
            return Some(tm.transform_point3(Vec3::new(
                a[0].as_f64()? as f32,
                a[1].as_f64()? as f32,
                a[2].as_f64()? as f32,
            )));
        }
        let name = step["gaze"].as_str()?;
        Some(match name {
            "lever" => self
                .player
                .visual_gun_model()
                .transform_point3(GUNLEVER_POS),
            "breech" => spent_case_grip(self.player.visual_gun_model(), self.casing_pull),
            "tray" => self.reload_mouth_world(),
            "gunner" => self
                .player
                .gun_model()
                .transform_point3(tank::TIGER_SIGHT_EYEPIECE - tank::TIGER_PARTS.gun_trunnion),
            "cupola" => tm.transform_point3(Vec3::new(0.564, 2.46, -0.42)),
            "rack_ap" | "rack" => {
                let eye = self.station_eye(Station::Loader);
                crate::interior_rig::SHELL_SLOTS
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {
                        !self.slot_taken[*i] && (name == "rack" || !self.slot_nature_he[*i])
                    })
                    .map(|(_, (frame, pos, _))| {
                        if *frame == crate::interior_rig::Frame::Hull {
                            self.player.hull_model().transform_point3(*pos)
                        } else {
                            tm.transform_point3(*pos)
                        }
                    })
                    .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))?
            }
            "enemy" => {
                let i = step["enemy"]
                    .as_u64()
                    .map(|i| i as usize)
                    .or_else(|| self.nearest_enemy())?;
                let e = self.enemies.get(i)?;
                let (min, max) = rig_armor_bounds(enemy_armor(e.kind), ArmorPart::Turret)?;
                // Human-style tracking includes lead for travel time and mouse-follow lag.
                let point = e.turret_model().transform_point3((min + max) * 0.5);
                let lead = point.distance(self.player.center()) / 780.0 + 0.085;
                point + e.forward() * e.speed * lead
            }
            n if n.starts_with("block") => tm.transform_point3(
                tiger_periscopes::CUPOLA_CENTERS[n[5..].parse::<usize>().ok()?.min(6)],
            ),
            _ => return None,
        })
    }
    pub fn capture_gaze_error(&self, target: Vec3, r: &Renderer) -> (Vec2, Vec2) {
        let wrap = |a: f32| {
            (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        if self.mode == Mode::Tank && self.station == Station::Gunner {
            let shell = if self.he_loaded {
                &crate::ballistics::SPRGR_88
            } else {
                &crate::ballistics::PZGR_39
            };
            let (yaw, pitch) = scripted_gun_lay(&self.player, target, shell);
            let sens = [0.00044, 0.00018, 0.00009][self.sights_mag.min(2) as usize];
            // The player's mouse lays a desired position; the real traverse/elevation drives chase
            // it. Overshoot with the hand while acquiring, then ease off as the barrel catches up.
            // A full-angle fivefold overshoot aliases by 2π and creates a false resting
            // direction 72 degrees from the target. Limit the hand's lead, not gun motion.
            let hand_yaw = self.player.turret_yaw
                + (wrap(yaw - self.player.turret_yaw) * 5.0).clamp(-0.9, 0.9);
            let hand_pitch = (self.player.gun_pitch + (pitch - self.player.gun_pitch) * 5.0)
                .clamp(-0.139, 0.261);
            return (
                Vec2::new(wrap(hand_yaw - self.lay_yaw), hand_pitch - self.lay_pitch),
                Vec2::splat(-sens),
            );
        }
        let to = (target - self.station_eye(self.station)).normalize_or_zero();
        let tf = self.player.turret_forward();
        let yaw = wrap(to.x.atan2(-to.z) - tf.x.atan2(-tf.z));
        let pitch = if self.station == Station::Commander {
            to.y.clamp(-0.999, 0.999).asin() + 0.10
        } else {
            to.y.clamp(-0.999, 0.999).asin() - tf.y.clamp(-0.999, 0.999).asin()
        };
        if self.mode == Mode::Foot {
            let to = (target - r.camera.pos).normalize_or_zero();
            let f = r.camera.forward();
            return (
                Vec2::new(
                    wrap(to.x.atan2(-to.z) - f.x.atan2(-f.z)),
                    to.y.asin() - f.y.asin(),
                ),
                Vec2::new(0.0022, -0.0022),
            );
        }
        (
            Vec2::new(wrap(yaw - self.head_yaw), pitch - self.head_pitch),
            Vec2::new(0.0022, -0.0022),
        )
    }
    pub fn capture_aim_ready(&self, target: Vec3) -> bool {
        let shell = if self.he_loaded {
            &crate::ballistics::SPRGR_88
        } else {
            &crate::ballistics::PZGR_39
        };
        let (yaw, pitch) = scripted_gun_lay(&self.player, target, shell);
        let dy = (yaw - self.player.turret_yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        self.station_ready(Station::Gunner)
            && self.breech_loaded()
            && dy.abs() < 0.0007
            && (pitch - self.player.gun_pitch).abs() < 0.0005
    }
    pub fn capture_combat_receipt(&self) -> serde_json::Value {
        let step = serde_json::json!({"gaze":"enemy"});
        let target = self.capture_gaze_target(&step);
        let (muzzle, direction) = self.player.muzzle();
        serde_json::json!({"target":target.map(|p|p.to_array()),"muzzle":muzzle.to_array(),"bore":direction.to_array(),
            "gun_pitch":self.player.gun_pitch,"lay_pitch":self.lay_pitch,"lay_yaw":self.lay_yaw,
            "hits":self.stat_hits_by_player,"kills":self.stat_kills_by_player,
            "impact":self.fx.impact_pos.to_array(),"impact_kind":format!("{:?}",self.fx.impact_kind)})
    }
    pub fn capture_hull_turn(&self, target: Vec3) -> f32 {
        let to = target - self.player.pos;
        let error = (to.x.atan2(to.z) - self.player.heading + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        if error.abs() < 0.018 {
            0.0
        } else {
            (error * 2.0).clamp(-0.85, 0.85)
        }
    }
    pub fn capture_walk_input(&self, target: Vec2) -> (f32, f32) {
        if self.station != Station::Loader || self.mode != Mode::Tank {
            return (0.0, 0.0);
        }
        let local = Vec3::new(target.x - self.walk_pos.x, 0.0, target.y - self.walk_pos.z);
        if local.length() < 0.018 {
            return (0.0, 0.0);
        }
        let world = glam::Quat::from_rotation_y(self.player.heading + self.player.turret_yaw)
            * local.normalize();
        let tf = self.player.turret_forward();
        let yaw = tf.x.atan2(-tf.z) + self.head_yaw;
        (
            world.dot(Vec3::new(yaw.sin(), 0.0, -yaw.cos())),
            -world.dot(Vec3::new(yaw.cos(), 0.0, yaw.sin())),
        )
    }
}
