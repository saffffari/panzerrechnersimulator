//! Read-only geometry queries reused from the validated player-input recorder.
//! They do not move actors, reload ammunition, aim the gun or alter combat.
use super::*;
impl Game {
    pub fn capture_gun_mechanism(&self) -> serde_json::Value {
        serde_json::json!({"gun_pitch_rad":self.player.gun_pitch,
            "turret_yaw_rad":self.player.turret_yaw,
            "elevation_normalized_rate":self.gun_elevation_motion.normalized_rate,
            "elevation_max_rate_rad_s":crate::tank::ELEV_RATE,
            "elevation_source_world_m":self.player.turret_model()
                .transform_point3(crate::interior_rig::WHEEL1_POS).to_array(),
            "scope":"Actual pose and source-motion control; not an audibility measurement"})
    }
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
    /// Fixed seed and geometry-only placement for real course defenders. No awareness, reload,
    /// aim, damage or target identity is changed. The first feasible authored site wins.
    pub fn capture_stage_course(&mut self, infantry:bool)->anyhow::Result<serde_json::Value> {
        self.enable_canyon_course_seeded(29)?;
        let spec=self.canyon_run.as_ref().unwrap().spec.clone();
        let reach=tank::TIGER_PARTS.hull_half_extents.length()+0.3;
        let clear=|at:Vec2| {
            let p=Vec3::new(at.x,self.hf.height_at(at.x,at.y),at.y);
            self.winter_patrol_point_clear(at) && !self.box_region_blocked(
                p+Vec3::new(-reach,0.3,-reach),p+Vec3::new(reach,2.8,reach))
        };
        let (point,target,identity)=if infantry {
            let mut selected=None;
            for (index,squad) in self.mixed_combat.squads.iter().enumerate() {
                let members:Vec<_>=squad.members.iter().filter_map(|&i|self.soldiers.get(i).map(|s|(i,s.pos))).collect();
                let center=members.iter().map(|(_,p)|*p).sum::<Vec3>()/members.len().max(1) as f32;
                let mut route=spec.route.clone();
                route.sort_by(|a,b| (a.distance(Vec2::new(center.x,center.z))-34.0).abs()
                    .total_cmp(&(b.distance(Vec2::new(center.x,center.z))-34.0).abs()));
                if let Some(at)=route.into_iter().find(|&at| {
                    let eye=Vec3::new(at.x,self.hf.height_at(at.x,at.y)+2.9,at.y);
                    let range=eye.distance(center);
                    (25.0..42.0).contains(&range) && clear(at)
                        && members.iter().filter(|(i,p)| {
                            let yaw=self.soldiers[*i].yaw;let delta=eye-*p;
                            Vec3::new(yaw.sin(),0.0,-yaw.cos()).dot(Vec3::new(delta.x,0.0,delta.z).normalize_or_zero())>0.342
                                && self.los_clear(eye,*p+Vec3::Y*1.2)
                        }).count()>=2
                }) {
                    let target=members.iter().find(|(_,p)|self.los_clear(
                        Vec3::new(at.x,self.hf.height_at(at.x,at.y)+2.9,at.y),*p+Vec3::Y*1.2)).unwrap();
                    selected=Some((at,target.1+Vec3::Y*1.2,serde_json::json!({"kind":"infantry_group",
                        "group":index,"members":members.iter().map(|(i,_)|*i).collect::<Vec<_>>(),"focus_member":target.0})));
                    break;
                }
            }
            selected.ok_or_else(||anyhow::anyhow!("no authored route point provides clear25–42m group review"))?
        } else {
            let gun=self.at_guns[0].ok_or_else(||anyhow::anyhow!("seeded course has no first gun"))?;
            let zone=&spec.gun_zones[0];let plan=self.street_plan.as_ref().unwrap();
            let lines:Vec<_>=plan.kill_zone.iter().filter(|l|l.id==zone.kill_zone).cloned().collect();
            let mut points=street_encounter::sample_kill_zone(&lines,|p|self.hf.height_at(p.x,p.y));
            points.sort_by(|a,b|(Vec3::new(a.at.x,a.ground,a.at.y).distance(gun.pos)-155.0).abs()
                .total_cmp(&(Vec3::new(b.at.x,b.ground,b.at.y).distance(gun.pos)-155.0).abs()));
            let point=points.into_iter().find(|p| {
                let center=Vec3::new(p.at.x,p.ground+1.5,p.at.y);
                let range=center.distance(gun.pos);
                (115.0..195.0).contains(&range) && clear(p.at)
                    && self.los_clear(gun.pos+Vec3::Y*1.1,center)
            }).ok_or_else(||anyhow::anyhow!("first seeded gun has no clear115–195m review point"))?;
            (point.at,gun.pos+Vec3::Y*0.95,serde_json::json!({"kind":"at_gun","gun":0,"zone":zone.id}))
        };
        let delta=target-Vec3::new(point.x,0.0,point.y);
        self.debug_place_tiger(point,delta.x.atan2(delta.z));
        Ok(serde_json::json!({"seed":29,"identity":identity,"focus_world_m":target.to_array(),
            "player_world_m":self.player.pos.to_array(),"source_heading_rad":self.player.heading,
            "active_t34_indices":self.enemies.iter().enumerate().filter(|(i,e)|e.alive && self.enemy_active[*i]).map(|(i,_)|i).collect::<Vec<_>>(),
            "initial_course":self.canyon_snapshot(),"initial_defenders":self.capture_course_receipt(),
            "scope":"Only initial Tiger position/heading staged on measured-clear authored route. Full seeded course defenders retained; no forced awareness, reload, hit, kill or outcome search."}))
    }
    pub fn capture_course_receipt(&self)->serde_json::Value {
        serde_json::json!({"course":self.canyon_snapshot(),"at_guns":self.at_gun_snapshot(),
            "mixed":self.mixed_combat_telemetry(),"exterior":self.exterior_damage_receipt(),
            "groups":self.mixed_combat.squads.iter().map(|s|serde_json::json!({"members":s.members,
                "positions":s.members.iter().filter_map(|&i|self.soldiers.get(i).map(|s|s.pos.to_array())).collect::<Vec<_>>()
            })).collect::<Vec<_>>()})
    }
    /// Accumulated live track travel and ground excitation, without changing their sampler.
    pub fn capture_running_gear(&self) -> serde_json::Value {
        serde_json::json!({"distance_m":self.running_gear_motion.distance_m,
            "ground_excitation_m":self.running_gear_motion.ground_excitation_m,
            "engine_ear_laf_dba":self.engine_ear_laf_dba,"engine_on":self.engine_on,
            "turret_yaw_rad":self.player.turret_yaw,"previous_turret_audio_yaw_rad":self.turret_audio_prev_yaw,
            "scope":"Fourteen source track-path ground samples; travel/excitation, not solved wheel forces or suspension compression"})
    }
    /// Read-only placement evidence for native combat reviews in any authored act. This checks
    /// the real terrain/cover LOS and gun-depression slope; it never removes an obstruction or
    /// changes detection, damage, aim, readiness, or any actor's position.
    pub fn capture_opening_lane(&self, position: Vec2) -> serde_json::Value {
        let delta=position-Vec2::new(self.player.pos.x,self.player.pos.z);
        let range=delta.length();
        let candidate=Tank::new(position.x,position.y,(-delta.x).atan2(-delta.y),&self.hf,tank::T34_PARTS);
        let slope=(candidate.pos.y-self.player.pos.y).atan2(range).to_degrees();
        let inside_map=position.x.abs()<self.act.half_extent_m-8.0 && position.y.abs()<self.act.half_extent_m-8.0;
        let clear=self.los_clear(candidate.center(),self.player.center());
        let near_cover=self.occluders.iter().filter(|o|((o.min+o.max)*0.5).distance(self.player.center())<150.0).count();
        serde_json::json!({"position":position.to_array(),"range_m":range,"slope_deg":slope,
            "inside_map":inside_map,"los_clear":clear,"nearby_cover_boxes":near_cover,
            "accepted":inside_map && range>=40.0 && range<=320.0 && slope.abs()<6.0 && clear})
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
