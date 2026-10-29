//! Finite physical 88 mm rounds spilled from authored Tiger wrecks and hand-stowed in real racks.
use super::*;
pub(super) const MAX_SALVAGE_ROUNDS: usize = 96;
const PER_WRECK: usize = 12;

pub(super) fn is_tiger_hull_asset(path: &str) -> bool {
    path == "assets/authoring/battlefield/tiger_wreck_20260925_hull.glb"
        || path.contains("/tiger/") && path.rsplit('/').next().is_some_and(|name|
        name.starts_with("tiger_hull") && name.ends_with(".glb"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exterior_rounds_do_not_follow_the_player_tank_transform() {
        let sh = LooseShell { world_space: true, pos: Vec3::new(10.0, 0.1, 5.0),
            rotation: Quat::IDENTITY, ..LooseShell::default() };
        let tank = Mat4::from_rotation_translation(Quat::from_rotation_y(1.0), Vec3::new(-55.0, 6.0, 40.0));
        assert_eq!(sh.world_model(tank).w_axis.truncate(), sh.pos);
        assert_ne!(LooseShell { world_space: false, ..sh }.world_model(tank).w_axis.truncate(), sh.pos);
    }

    #[test]
    #[ignore = "GPU and source Tiger assets; tests actual pickup, rack visibility and finite inventory"]
    fn salvaged_round_uses_actual_hands_and_empty_rack_without_duplication() -> anyhow::Result<()> {
        use crate::tank_editor_scene::{TankAssetId, TankEditorScene};
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut r, mut g) = Game::build_tank_editor(&device, &queue,
            &TankEditorScene::new(TankAssetId::TigerI))?;
        g.mute_audio();
        g.mode = Mode::Foot;
        g.foot.pos = Vec3::new(5.0, g.hf.height_at(5.0, 0.0), 0.0);
        let pos = Vec3::new(5.0, g.hf.height_at(5.0, -0.9) + 0.053, -0.9);
        g.loose[0] = LooseShell { active: true, world_space: true, pos,
            rotation: Quat::IDENTITY, nature_he: false, ..LooseShell::default() };
        r.camera.pos = g.foot.eye();
        r.camera.set_forward((pos - r.camera.pos).normalize());
        g.foot.yaw = r.camera.yaw;
        g.foot.pitch = r.camera.pitch;
        g.update(&TankInput { mg_coax: true, fire2: true, ..TankInput::default() }, 1.0/60.0, &mut r);
        assert!(g.carry_mesh == Some(CarriedMesh::LooseShell(0)), "crosshair pickup failed");
        assert!(!g.loose[0].active, "source body duplicated after pickup");
        let pistol_ammo = g.foot.ammo;
        for _ in 0..8 {
            g.update(&TankInput { mg_coax: true, fire2: true, ..TankInput::default() }, 1.0/60.0, &mut r);
        }
        assert_eq!(g.foot.ammo, pistol_ammo, "carrying a round fired the pistol");

        // The traversal regression separately proves carrying through the actual hatch. This
        // fixture advances to the cabin to isolate finite inventory and real rack sight lines.
        g.mode = Mode::Tank;
        g.station = Station::Loader;
        g.walk_pos = Vec3::new(0.85, 0.0, 0.08);
        g.switch_t = 0.0;
        g.slot_taken.fill(false);
        let eye = g.station_eye(Station::Loader);
        assert!(!g.try_stow_carried_round(eye, Vec3::Z), "full rack accepted a duplicate");
        let mut target = None;
        'views: for (walk, crouch) in [
            (Vec3::new(0.85, 0.0, 0.08), 0.0), (Vec3::new(0.85, 0.0, 0.08), 1.0),
            (Vec3::new(0.70, 0.0, -0.55), 0.0), (Vec3::new(0.70, 0.0, -0.55), 1.0),
            (Vec3::new(-0.70, 0.0, -0.55), 0.0), (Vec3::new(-0.70, 0.0, -0.55), 1.0)] {
          g.walk_pos = walk;
          g.loader_crouch_t = crouch;
          let eye = g.station_eye(Station::Loader);
          for (i, &(frame, p, q)) in crate::interior_rig::SHELL_SLOTS.iter().enumerate() {
            if g.slot_nature_he[i] { continue; }
            let parent = if frame == crate::interior_rig::Frame::Hull { g.player.hull_model() }
                else { g.player.turret_model() };
            let point = (parent * Mat4::from_rotation_translation(Quat::from_array(q), p)).w_axis.truncate();
            let look = (point-eye).normalize();
            g.slot_taken[i] = true;
            if g.empty_rack_under_aim(eye, look, false) == Some(i) { target = Some((i, eye, look)); break 'views; }
            g.slot_taken[i] = false;
        }
        }
        let (slot, eye, look) = target.expect("actual cabin eye should reach at least one AP cradle");
        assert!(g.try_stow_carried_round(eye, look));
        assert!(!g.slot_taken[slot]);
        assert!(g.carry_mesh.is_none());
        assert!(!g.loose[0].active);
        assert!(!g.try_stow_carried_round(eye, look), "second placement duplicated the cartridge");
        println!("[salvage-review] actual world pickup, occupied rack rejection and manual AP stow into source slot {slot} PASS");
        Ok(())
    }
}

impl LooseShell {
    pub(super) fn world_model(&self, turret: Mat4) -> Mat4 {
        let local = Mat4::from_rotation_translation(self.rotation, self.pos);
        if self.world_space { local } else { turret * local }
    }
}

impl Game {
    pub(super) fn seed_wreck_salvage(&mut self, mut hulls: Vec<Mat4>) {
        // Select nearest hulls deterministically if a much larger authored map exceeds the pool.
        hulls.sort_by(|a, b| a.w_axis.truncate().distance_squared(self.player.pos)
            .total_cmp(&b.w_axis.truncate().distance_squared(self.player.pos)));
        hulls.dedup_by(|a, b| a.w_axis.truncate().distance_squared(b.w_axis.truncate()) < 0.01);
        let mut count = 0;
        for hull in hulls.into_iter().take(MAX_SALVAGE_ROUNDS / PER_WRECK) {
            for round in 0..PER_WRECK {
                // Exposed, spilled stowage alongside the port track, accessible without walking
                // through armor. All twelve are real rendered bodies, never an invisible cache.
                let local = Vec3::new(-2.15 - (round % 3) as f32 * 0.13, 0.0,
                    -1.1 + (round / 3) as f32 * 0.34);
                let mut pos = hull.transform_point3(local);
                pos.y = self.hf.height_at(pos.x, pos.z) + 0.053;
                let (_, rotation, _) = hull.to_scale_rotation_translation();
                self.loose[count] = LooseShell { active: true, world_space: true,
                    pos, rotation, nature_he: round >= 9, ..LooseShell::default() };
                count += 1;
            }
        }
        if count > 0 { println!("[salvage] {count} physical 88mm rounds beside authored Tiger wrecks (9 AP + 3 HE each)"); }
    }

    pub(super) fn drop_world_round(&mut self, carried: CarriedMesh, index: usize,
        nature_he: bool, inherit_swing: bool) -> bool {
        self.transfer_cartridge_state(carried.ammunition_location_id(), 0x2_0000 + index as u32);
        let (_, rotation, pos) = self.carried_round_model().to_scale_rotation_translation();
        self.loose[index] = LooseShell { active: true, world_space: true, pos, rotation,
            nature_he, vel: if inherit_swing { self.carry_vel } else { Vec3::ZERO },
            ..LooseShell::default() };
        self.carry_mesh = None;
        self.grab = false;
        self.ram = 0.0;
        true
    }

    pub(super) fn update_exterior_ammunition(&mut self, input: &TankInput, dt: f32,
        r: &mut Renderer) -> bool {
        if self.carry_mesh.is_none() && self.held_item.is_none() && !self.ext_equipped
            && !self.carrying_track_kit && !self.compass_equipped && !self.map_held
            && !self.panzerfaust_claimed && !self.climb.active {
            let target = self.loose.iter().enumerate().filter(|(_, sh)| sh.active && sh.world_space)
                .filter_map(|(i, sh)| ray_loader_round(r.camera.pos, r.camera.forward(),
                    sh.world_model(self.player.turret_model()), 2.2).map(|distance| (i, distance)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, distance)) = target {
                let clear = self.nearest_cover_ray(r.camera.pos, r.camera.forward(), distance)
                    .is_none_or(|(hit, _)| hit >= distance - 0.08);
                if clear {
                    self.hover_label = Some(if self.loose[i].nature_he {
                        "SALVAGE 88MM HE [HOLD LMB]"
                    } else { "SALVAGE 88MM AP [HOLD LMB]" });
                    if input.mg_coax && !self.foot.prev_fire {
                        let sh = self.loose[i];
                        self.loose[i].active = false;
                        self.carry_mesh = Some(CarriedMesh::LooseShell(i));
                        self.grab = true;
                        self.set_carry_pose(sh.pos, sh.rotation, sh.vel, Vec3::ZERO);
                        self.consume_primary_press();
                        // A live 88 lifted off the snow by a wreck (silent before the recordings).
                        self.recorded(crate::audio::RecordedFx::ShellPickup, 0.0, sh.pos);
                    }
                }
            }
        }
        if self.carry_mesh.is_none() { return false; }
        self.foot.aiming = false;
        self.foot.prev_fire = input.fire2;
        r.camera.fov_y = 68.0f32.to_radians();
        r.reticle = false;
        self.hide_p08(r);
        r.set_visible(self.slots.panzerfaust, false);
        self.update_carry(r, dt, CARRY_K_ROUND, CARRY_D_ROUND);
        self.hover_label = Some("PICK UP 88MM ROUND [LMB]");
        if !input.mg_coax { self.drop_carried_round(true); }
        true
    }

    pub(super) fn empty_rack_under_aim(&self, eye: Vec3, look: Vec3, high_explosive: bool) -> Option<usize> {
        crate::interior_rig::SHELL_SLOTS.iter().enumerate()
            .filter(|(i, _)| self.slot_taken[*i]
                && self.slot_nature_he[*i] == high_explosive)
            .filter_map(|(i, &(frame, pos, q))| {
                let parent = if frame == crate::interior_rig::Frame::Hull {
                    self.player.hull_model()
                } else { self.player.turret_model() };
                let model = parent * Mat4::from_rotation_translation(Quat::from_array(q), pos);
                ray_loader_round(eye, look, model, 1.9).map(|distance| (i, distance))
            }).filter(|(_, distance)| self.equipment_path_clear(eye, eye + look * *distance, 0.10))
            .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
    }

    pub(super) fn try_stow_carried_round(&mut self, eye: Vec3, look: Vec3) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader { return false; }
        let Some(carried) = self.carry_mesh else { return false; };
        let nature_he = carried.nature_he(&self.slot_nature_he, &self.loose);
        let Some(index) = self.empty_rack_under_aim(eye, look, nature_he) else { return false; };
        if let Some(mission) = &mut self.winter_mission {
            mission.stowed(carried.ammunition_location_id(),self.rounds_spent);
        }
        self.transfer_cartridge_state(carried.ammunition_location_id(), crate::interior_rig::shell_source_id(index));
        self.slot_taken[index] = false;
        if let CarriedMesh::LooseShell(i) = carried { self.loose[i].active = false; }
        self.carry_mesh = None;
        self.grab = false;
        self.ram = 0.0;
        if let Some(audio) = &self.audio { audio.play_control("breech", 0.6); }
        println!("[salvage] {} round hand-stowed in rack slot {index}", if nature_he { "HE" } else { "AP" });
        true
    }

    pub(super) fn carried_round_can_stow(&self, eye: Vec3, look: Vec3) -> bool {
        self.carry_mesh.is_some_and(|carried| self.empty_rack_under_aim(eye, look,
            carried.nature_he(&self.slot_nature_he, &self.loose)).is_some())
    }

    pub(super) fn drive_world_round(&mut self, index: usize, dt: f32, r: &mut Renderer) {
        let sh = self.loose[index];
        let dt = dt.clamp(0.0, 0.05);
        let resting = sh.vel.length_squared() < 1e-8
            && (sh.pos.y - self.hf.height_at(sh.pos.x, sh.pos.z) - 0.053).abs() < 0.002;
        let mut velocity = if resting { Vec3::ZERO } else { sh.vel - Vec3::Y * 9.81 * dt };
        let mut position = sh.pos + velocity * dt;
        let floor = self.hf.height_at(position.x, position.z) + 0.053;
        let delta = position - sh.pos;
        if delta.length_squared() > 1e-8 {
            if let Some((hit, _)) = self.nearest_cover_ray(sh.pos, delta.normalize(), delta.length() + 0.053) {
                position = sh.pos + delta.normalize() * (hit - 0.053).max(0.0);
                velocity = Vec3::ZERO;
            }
        }
        if position.y < floor { position.y = floor; velocity = Vec3::ZERO; }
        self.loose[index].pos = position;
        self.loose[index].vel = velocity;
        let ap = self.slots.loose_shell[index];
        let he = self.slots.loose_shell_he[index];
        r.set_visible(ap, !sh.nature_he);
        r.set_visible(he, sh.nature_he);
        let slot = if sh.nature_he { he } else { ap };
        r.set_emissive(slot, 0.0);
        r.set_model(slot, Mat4::from_rotation_translation(sh.rotation, position));
    }
}
