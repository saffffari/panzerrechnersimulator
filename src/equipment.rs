//! Embodied stowage: source-sized equipment stays in the scene when carried or set down.
use super::*;

/// One field repair the crewman can do with the toolbox.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum RepairJob {
    Module(ModuleKind),
    /// Swap a cracked cupola block for a spare.
    VisionBlock(usize),
}

/// A put-down kit follows the basket when inside; an outside kit remains at its world position.
#[derive(Clone, Copy)]
pub(super) struct ToolRest {
    pub turret: bool,
    pub model: Mat4,
}

/// One launcher and two spare bombs form a single carried loadout. Spare bombs are
/// a compact game representation of reserve shots, rather than five additional tubes.
pub(super) fn panzerfaust_rack_mesh(index: usize) -> &'static CpuMesh {
    assert!(index < PANZERFAUST_RACK);
    static TUBE: OnceLock<CpuMesh> = OnceLock::new();
    static BOMB: OnceLock<CpuMesh> = OnceLock::new();
    if index == 0 { TUBE.get_or_init(crate::panzerfaust::body_mesh) }
    else { BOMB.get_or_init(crate::panzerfaust::projectile_mesh) }
}

pub(super) fn panzerfaust_rack_local_pose(index: usize) -> Mat4 {
    assert!(index < PANZERFAUST_RACK);
    // Horizontal upper port-wall stowage at the side escape hatch's height. The
    // aft ends stop before the escape aperture, leaving its complete swing clear.
    let origin = match index {
        0 => Vec3::new(-0.98, 2.26, 0.05),
        1 => Vec3::new(-0.98, 2.14, -0.30),
        _ => Vec3::new(-0.98, 1.94, -0.40),
    };
    Mat4::from_translation(origin)
}

pub(super) fn panzerfaust_brackets() -> CpuMesh {
    let mut parts = Vec::new();
    // Two slim saddles per item fasten directly to the port wall. No backing plate
    // crosses the escape hatch or hides another interaction target.
    for i in 0..PANZERFAUST_RACK {
        let model = panzerfaust_rack_local_pose(i);
        let along = if i == 0 { [-0.42, 0.14] } else { [-0.18, -0.10] };
        for z in along {
            parts.push(box_mesh(Vec3::new(-0.18, -0.046, z - 0.012),
                Vec3::new(0.042, -0.030, z + 0.012), [61, 66, 67]).transformed(model));
            parts.push(box_mesh(Vec3::new(0.032, -0.044, z - 0.012),
                Vec3::new(0.044, 0.010, z + 0.012), [61, 66, 67]).transformed(model));
        }
    }
    let mut out = parts.remove(0);
    for part in parts {
        let base = out.vertices.len() as u32;
        out.vertices.extend(part.vertices);
        out.indices.extend(part.indices.into_iter().map(|i| i + base));
        out.min = out.min.min(part.min);
        out.max = out.max.max(part.max);
    }
    out.metallic_factor = 0.62;
    out.roughness_factor = 0.79;
    out
}

/// Restow only the unfired shots: one launcher plus the remaining reserve bombs.
pub(super) fn panzerfaust_restowed_slots(shots: u8) -> [bool; PANZERFAUST_RACK] {
    std::array::from_fn(|i| i >= usize::from(shots).min(PANZERFAUST_RACK))
}

pub(super) fn toolbox_mesh() -> &'static CpuMesh {
    static MESH: OnceLock<CpuMesh> = OnceLock::new();
    MESH.get_or_init(|| {
        crate::gltf_load::load_glb(&format!("{MP2}Props/toolbox_mx_1.glb"))
        .unwrap_or_else(|_| {
            box_mesh(Vec3::new(-0.22, 0.0, -0.12),
            Vec3::new(0.22, 0.16, 0.12), [95, 64, 48])
        })
    })
}

pub(super) fn repair_in_reach(eye: Vec3, look: Vec3, point: Vec3, radius: f32) -> bool {
    let delta = point - eye;
    let along = delta.dot(look.normalize_or_zero());
    along > 0.08 && delta.length() <= 1.65
        && (delta - look.normalize_or_zero() * along).length() <= radius
}

impl Game {
    pub fn carried_tools(&self) -> bool { self.carrying_track_kit }
    pub fn carried_panzerfaust(&self) -> bool { self.panzerfaust_claimed }

    /// Recorder convenience: turn the same loader head toward existing geometry only.
    /// This never moves the body, equips anything, edits a rack slot or changes tool state.
    pub fn debug_loader_face_equipment(&mut self, target: &str) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader {
            return false;
        }
        let point = match target {
            "toolbox" if !self.carrying_track_kit => self
                .toolbox_model()
                .transform_point3(toolbox_mesh().center()),
            "panzerfaust_rack" => {
                let mut points = (0..PANZERFAUST_RACK)
                    .filter(|&i| !self.panzerfaust_rack_taken[i])
                    .map(|i| {
                        (self.player.turret_model() * panzerfaust_rack_local_pose(i))
                            .transform_point3(Vec3::ZERO)
                    });
                let Some(first) = points.next() else {
                    return false;
                };
                let (sum, count) = points.fold((first, 1u32), |(sum, count), point| {
                    (sum + point, count + 1)
                });
                sum / count as f32
            }
            _ => return false,
        };
        self.loader_face_world(point);
        true
    }

    fn toolbox_model(&self) -> Mat4 {
        if let Some(rest) = self.toolbox_rest {
            if rest.turret { self.player.turret_model() * rest.model } else { rest.model }
        } else { self.player.turret_model() * Mat4::from_translation(TOOLBOX_MESH) }
    }

    fn equipment_hands_free(&self) -> bool {
        !self.carrying_track_kit && !self.ext_equipped && !self.compass_equipped
            && self.carry_mesh.is_none() && self.carried_casing.is_none()
            && self.held_item.is_none() && !self.panzerfaust_claimed
    }

    pub(super) fn equipment_path_clear(&self, eye: Vec3, point: Vec3, tolerance: f32) -> bool {
        let models = self.fire_surface_models();
        self.cabin_fire.sweep(eye, point, &models).is_none_or(|hit| {
            let world = models[hit.frame].transform_point3(hit.point);
            world.distance(point) <= tolerance
        })
    }

    /// Run after camera movement, in either mode, so a kit set down outside is still touchable.
    pub(super) fn update_equipment_hover(&mut self, r: &Renderer) {
        self.equipment_eye = r.camera.pos;
        self.equipment_look = r.camera.forward();
        if matches!(self.hover, Some(Hotspot::RepairKit | Hotspot::PanzerfaustRack(_))) {
            self.hover = None;
        }
        if self.climb.active || self.switch_t > 0.0 { return; }
        let eye = r.camera.pos;
        let look = r.camera.forward();
        if self.carrying_track_kit {
            self.hover = None;
            if self.mode == Mode::Tank {
                self.hover_label = Some(match self.physical_repair_target(eye, look) {
                    Some(job) => self.job_prompt(job),
                    None => "TOOLBOX + HAMMER - CARRY TO DAMAGED PART; Q/E: PUT DOWN",
                });
            }
            return;
        }
        if self.panzerfaust_claimed && self.mode == Mode::Tank {
            self.hover = None;
            self.hover_label = Some("PANZERFAUST IN HAND - CARRY OUTSIDE; Q/E: RETURN TO RACK");
            return;
        }
        if !self.equipment_hands_free() { return; }
        let mut nearest = f32::INFINITY;
        let model = self.toolbox_model();
        if let Some(t) = ray_loader_ammunition_mesh(eye, look, model, 1.85, toolbox_mesh()) {
            if self.equipment_path_clear(eye, eye + look * t, 0.04) {
                nearest = t;
                self.hover = Some(Hotspot::RepairKit);
                self.hover_label = Some("TAKE TOOLBOX + HAMMER [LMB]");
            }
        }
        // The body mesh, not the rack's whole bounding rectangle, owns each pickup.
        if self.mode == Mode::Tank && self.station == Station::Loader {
            for i in 0..PANZERFAUST_RACK {
                if self.panzerfaust_rack_taken[i] { continue; }
                let mesh = panzerfaust_rack_mesh(i);
                let model = self.player.turret_model() * panzerfaust_rack_local_pose(i);
                if let Some(t) = ray_loader_ammunition_mesh(eye, look, model, 1.85, mesh) {
                    if t < nearest && self.equipment_path_clear(eye, eye + look * t, 0.035) {
                        nearest = t;
                        self.hover = Some(Hotspot::PanzerfaustRack(i));
                        self.hover_label = Some("TAKE PANZERFAUST + SPARE AMMO [LMB]");
                    }
                }
            }
        }
    }

    pub(super) fn take_panzerfaust(&mut self, index: usize) -> bool {
        if index >= PANZERFAUST_RACK || self.panzerfaust_rack_taken[index]
            || !self.equipment_hands_free() { return false; }
        let shots = self.panzerfaust_rack_taken.iter().filter(|taken| !**taken).count() as u8;
        self.panzerfaust_rack_taken = [true; PANZERFAUST_RACK];
        self.panzerfaust_rack_count = 0;
        self.panzerfaust_source = Some(0);
        self.panzerfaust_claimed = true;
        self.foot.panzerfaust_ready = true;
        self.foot.panzerfaust_spares = shots.saturating_sub(1);
        self.foot.panzerfaust_swap_t = 0.0;
        self.foot.weapon = crate::foot::FootWeapon::Panzerfaust;
        self.primary_pickup_press = true;
        true
    }

    pub(super) fn take_toolbox(&mut self) -> bool {
        if !self.equipment_hands_free() { return false; }
        self.carrying_track_kit = true;
        self.toolbox_rest = None;
        self.primary_pickup_press = true;
        true
    }

    pub(super) fn put_down_toolbox(&mut self) {
        let eye = self.equipment_eye;
        let look = self.equipment_look;
        let mount = self.player.turret_model().transform_point3(TOOLBOX_MOUNT);
        self.toolbox_rest = if self.mode == Mode::Tank
            && repair_in_reach(eye, look, mount, 0.28)
            && self.equipment_path_clear(eye, mount, 0.22) {
            None
        } else {
            let models = self.fire_surface_models();
            let forward = Vec3::new(look.x, 0.0, look.z).normalize_or_zero();
            let mut position = eye + forward * 0.45 - Vec3::Y * 0.45;
            // Stop in front of steel before finding the support underneath, never through a wall.
            if let Some(hit) = self.cabin_fire.sweep(eye, position + forward * 0.24, &models) {
                position = models[hit.frame].transform_point3(hit.point) - forward * 0.26;
            }
            let ground = self.hf.height_at(position.x, position.z);
            let down = Vec3::new(position.x, ground - 0.2, position.z);
            position.y = self.cabin_fire.sweep(position, down, &models)
                .map_or(ground, |hit| {
                    models[hit.frame].transform_point3(hit.point).y
                });
            position.y += 0.012 - toolbox_mesh().min.y;
            let rotation = Quat::from_rotation_y(forward.x.atan2(forward.z));
            let world = Mat4::from_rotation_translation(rotation, position);
            let turret = self.mode == Mode::Tank;
            Some(ToolRest { turret, model: if turret {
                self.player.turret_model().inverse() * world
            } else { world } })
        };
        self.carrying_track_kit = false;
        self.activity = None;
        self.repair_t = 0.0;
        self.primary_pickup_press = true;
    }

    /// Every job the crewman could do on the tank right now, anywhere: modules whose fault has a
    /// field fix (or undiagnosed legacy damage the old repair table covers) and cracked cupola
    /// blocks while spares remain. Tracks have their own ritual.
    pub(super) fn open_repair_jobs(&self) -> Vec<RepairJob> {
        use tiger_damage::Fault;
        let mut jobs: Vec<RepairJob> = self.modules.iter().filter(|m| {
            !matches!(m.kind, ModuleKind::LeftTrack | ModuleKind::RightTrack) && match m.fault {
                Fault::None | Fault::Spall => !m.functional && repair_time(m.kind).is_some(),
                fault => fault.field_repair().is_some(),
            }
        }).map(|m| RepairJob::Module(m.kind)).collect();
        if self.exterior.spare_blocks > 0 {
            jobs.extend((0..tiger_damage::BLOCKS)
                .filter(|&i| self.exterior.blocks[i] == tiger_damage::Block::Opaque)
                .map(RepairJob::VisionBlock));
        }
        jobs
    }

    fn job_seconds(&self, job: RepairJob) -> f32 {
        use tiger_damage::Fault;
        match job {
            RepairJob::Module(kind) => match self.module_fault(kind) {
                Fault::None | Fault::Spall => repair_time(kind).unwrap_or(f32::INFINITY),
                fault => fault.field_repair().map_or(f32::INFINITY, |r| r.seconds),
            },
            // Thumbwheel holder: no tools, one man, from inside [T1]. Duration is tuning.
            RepairJob::VisionBlock(_) => 8.0,
        }
    }

    fn job_prompt(&self, job: RepairJob) -> &'static str {
        use tiger_damage::Fault;
        match job {
            RepairJob::VisionBlock(_) => "SWAP IN A SPARE VISION BLOCK [HOLD LMB]",
            RepairJob::Module(kind) => match self.module_fault(kind) {
                Fault::RecoilLeak => "TOP UP THE RECOIL BRAKE [HOLD LMB]",
                Fault::HydraulicFault => "KNOCK THE TRAVERSE CLUTCH FREE [HOLD LMB]",
                Fault::RingJammed => "KNOCK THE FRAGMENT OUT OF THE TURRET RING [HOLD LMB]",
                Fault::SightOutOfTrue => "BORESIGHT THE GUNNER'S SIGHT [HOLD LMB]",
                Fault::MgBarrel => "CHANGE THE MG BARREL [HOLD LMB]",
                Fault::RadioShock => "RESEAT THE RADIO PLUGS AND FUSES [HOLD LMB]",
                _ => "REPAIR DAMAGED PART [HOLD LMB]",
            },
        }
    }

    /// The nearest job whose part is in hand's reach along the look ray with a clear path. Armour
    /// between the crewman and the part rules it out, so outside jobs are done from outside.
    pub(super) fn physical_repair_target(&self, eye: Vec3, look: Vec3) -> Option<RepairJob> {
        if !self.carrying_track_kit { return None; }
        self.open_repair_jobs().into_iter().filter_map(|job| {
            let point = self.repair_point(job)?;
            (repair_in_reach(eye, look, point, 0.22)
                && self.equipment_path_clear(eye, point, 0.28))
                .then_some((job, eye.distance_squared(point)))
        }).min_by(|a,b| a.1.total_cmp(&b.1)).map(|(job, _)| job)
    }

    pub(super) fn repair_point(&self, job: RepairJob) -> Option<Vec3> {
        use ModuleKind::*;
        let turret = self.player.turret_model();
        Some(match job {
            RepairJob::VisionBlock(block) => {
                // The block's inner end, toward the commander's eye.
                let centre = tiger_periscopes::CUPOLA_CENTERS[block];
                turret.transform_point3(centre.lerp(tiger_periscopes::COMMANDER_EYE, 0.25))
            }
            RepairJob::Module(TurretTraverse) if self.module_fault(TurretTraverse)
                == tiger_damage::Fault::RingJammed => turret.transform_point3(self.exterior.ring_jam?),
            RepairJob::Module(TurretTraverse) => turret.transform_point3(crate::interior_rig::WHEEL2_POS),
            RepairJob::Module(CoaxMg) => self.player.visual_gun_model().transform_point3(Vec3::new(-0.22, -0.16, -0.25)),
            // The recoil brake's filler on the cradle top, on the loader's side (approximate,
            // gun-local; probed to be in the loader's sight and hand's reach).
            RepairJob::Module(MainGun) => self.player.visual_gun_model().transform_point3(Vec3::new(-0.12, 0.22, -0.55)),
            // Boresighting is adjusted at the sight body, just ahead of its eyepiece.
            RepairJob::Module(GunnerSight) => {
                let (eye, forward, _) = self.gunner_interior_camera();
                eye + forward * 0.12
            }
            RepairJob::Module(kind) => self.player.hull_model().transform_point3(kind.station()),
        })
    }

    pub(super) fn update_carried_repair(&mut self, trigger: bool, dt: f32, eye: Vec3, look: Vec3) {
        if trigger && !self.primary_pickup_press {
            if let Some(job) = self.physical_repair_target(eye, look) {
                if self.activity != Some(Activity::Repair(job)) {
                    self.activity = Some(Activity::Repair(job));
                    self.repair_t = 0.0;
                }
                let previous = self.repair_t;
                self.repair_t += dt;
                if (previous / 0.48) as u32 != (self.repair_t / 0.48) as u32 {
                    self.hammer_t = 0.38;
                    if let Some(audio) = &self.audio {
                        audio.play_at_ear(crate::audio::Sfx::HammerStrike,
                            crate::sound_levels::HAND_MECHANISM.laf_db + crate::audio::effort_db(0.42));
                    }
                }
                self.hover_label = Some(self.job_prompt(job));
                if self.repair_t >= self.job_seconds(job) {
                    self.finish_repair(job);
                    self.activity = None;
                    self.repair_t = 0.0;
                    self.recompute_hp();
                }
                return;
            }
        }
        if matches!(self.activity, Some(Activity::Repair(_))) {
            self.activity = None;
            self.repair_t = 0.0;
        }
    }

    fn finish_repair(&mut self, job: RepairJob) {
        match job {
            RepairJob::VisionBlock(block) => {
                self.exterior.blocks[block] = tiger_damage::Block::Clear;
                self.exterior.spare_blocks = self.exterior.spare_blocks.saturating_sub(1);
                println!("[repair] cupola block {block} replaced; {} spares left", self.exterior.spare_blocks);
            }
            RepairJob::Module(kind) => {
                if let Some(module) = self.modules.iter_mut().find(|m| m.kind == kind) {
                    println!("[repair] {} — {} put right", kind.name(), module.fault.name());
                    module.functional = true;
                    module.fault = tiger_damage::Fault::None;
                }
                match kind {
                    ModuleKind::TurretTraverse => self.exterior.ring_jam = None,
                    ModuleKind::GunnerSight => self.exterior.sight_error = Vec2::ZERO,
                    _ => {}
                }
            }
        }
    }

    /// Shared final draw: walking inside, climbing and walking outside use the same source props.
    pub(super) fn drive_carried_tools(&mut self, dt: f32, r: &mut Renderer) {
        let eye = r.camera.pos;
        let f = r.camera.forward();
        let right = r.camera.right();
        let up = right.cross(f).normalize_or_zero();
        let rotation = crate::foot::held_weapon_rotation(f, up);
        r.set_emissive(self.slots.toolbox, 0.0);
        r.set_visible(self.slots.toolbox, true);
        if self.carrying_track_kit {
            let grip = Vec3::new((toolbox_mesh().min.x + toolbox_mesh().max.x) * 0.5,
                toolbox_mesh().max.y, (toolbox_mesh().min.z + toolbox_mesh().max.z) * 0.5);
            let left_hand = eye + f * 0.48 - right * 0.31 - up * 0.30;
            r.set_model(self.slots.toolbox, Mat4::from_rotation_translation(rotation,
                left_hand - rotation * grip));
            let swing = if self.hammer_t > 0.0 {
                let t = self.hammer_t / 0.38;
                if t > 0.37 { (1.0 - t) * 1.9 } else { 1.2 - (0.37 - t) * 5.5 }
            } else { 0.24 };
            // Source head is at -Z, grip near +Z. Stand the head up above the right hand;
            // an uncorrected weapon basis pointed the head back into the holder's arm.
            let hammer_rot = Quat::from_axis_angle(right, -swing) * rotation
                * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
            r.set_visible(self.slots.hammer, true);
            r.set_model(self.slots.hammer, Mat4::from_rotation_translation(hammer_rot,
                eye + f * 0.46 + right * 0.26 - up * 0.28 - hammer_rot * Vec3::Z * 0.065));
            self.hide_p08(r);
            r.set_visible(self.slots.panzerfaust, false);
            r.set_visible(self.slots.panzerfaust_sight, false);
            // Track strikes advance in the existing four-hit ritual; other repairs use this clock.
            if self.mode != Mode::Foot || self.track_repair_phase != TrackRepairPhase::HammerPin {
                self.hammer_t = (self.hammer_t - dt).max(0.0);
            }
        } else {
            r.set_model(self.slots.toolbox, self.toolbox_model());
            r.set_visible(self.slots.hammer, self.toolbox_rest.is_some());
            if self.toolbox_rest.is_some() {
                // The 52 cm source hammer lies across the 60 cm lid, wholly above its surface.
                let model = self.toolbox_model() * Mat4::from_translation(Vec3::new(0.134,
                    toolbox_mesh().max.y + 0.055, 0.0)) * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
                r.set_model(self.slots.hammer, model);
            }
        }
        if self.panzerfaust_claimed && self.mode != Mode::Foot {
            let hold = crate::panzerfaust::held_pose(eye, f, right, up, false);
            r.set_visible(self.slots.panzerfaust, true);
            r.set_model(self.slots.panzerfaust, Mat4::from_rotation_translation(hold.rotation, hold.origin));
            r.set_visible(self.slots.panzerfaust_sight, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first seed whose strike in `zone` by a `calibre` round has `want` among its effects.
    fn seed_for(zone: tiger_damage::Zone, calibre_mm: f32, want: impl Fn(&tiger_damage::Effect) -> bool) -> u32 {
        let strike = tiger_damage::Strike { calibre_mm, energy_kj: 0.0, penetrated: false, high_explosive: false };
        (0..100_000).find(|&seed| tiger_damage::effects(zone, strike, seed).iter().any(&want))
            .expect("some shell has that effect")
    }

    /// Carry the kit to `job` and hold the trigger from `eye`, as the crewman would.
    fn work(game: &mut Game, job: RepairJob, eye: Vec3) -> bool {
        game.carrying_track_kit = true;
        game.primary_pickup_press = false;
        let point = game.repair_point(job).expect("job has a place");
        let look = (point - eye).normalize();
        assert_eq!(game.physical_repair_target(eye, look), Some(job),
            "{job:?} not in reach from {eye:?}: {:.2} m away, path clear {}", eye.distance(point),
            game.equipment_path_clear(eye, point, 0.28));
        for _ in 0..(60.0 / 0.05) as usize {
            game.update_carried_repair(true, 0.05, eye, look);
            if game.physical_repair_target(eye, look) != Some(job) { return true; }
        }
        false
    }

    #[test]
    #[ignore = "requires GPU and source Tiger assets"]
    fn exterior_strikes_break_what_they_hit_and_the_lone_crewman_puts_the_field_faults_right() -> anyhow::Result<()> {
        use tiger_damage::{Block, Effect, Fault, Side, Zone};
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (_renderer, mut g) = Game::build(&device, &queue, false)?;
        g.mute_audio();
        let t76 = &crate::ballistics::BR_350B;
        let at45 = &crate::ballistics::BR_240_M42;
        let turret = g.player.turret_model();

        // A 7.62 cm round on the mantlet: the recoil brake leaks, the oil safety cuts the firing
        // circuit, the gun still elevates. The crewman tops it up beside the breech.
        let seed = seed_for(Zone::Mantlet, 76.2, |e| *e == Effect::Fault(ModuleKind::MainGun, Fault::RecoilLeak));
        g.debug_exterior_strike(ArmorPart::Turret, Vec3::new(0.0, 1.88, 1.589), t76, 300.0, seed);
        assert_eq!(g.module_fault(ModuleKind::MainGun), Fault::RecoilLeak, "{}", g.exterior_damage_receipt());
        assert!(!g.module_ok(ModuleKind::MainGun));
        let loader_eye = g.station_eye(Station::Loader);
        assert!(work(&mut g, RepairJob::Module(ModuleKind::MainGun), loader_eye), "recoil brake topped up");
        assert!(g.module_ok(ModuleKind::MainGun) && g.module_fault(ModuleKind::MainGun) == Fault::None);

        // Shock knocks the sight out of true: its line wanders 2–5 mils off the bore; boresighting
        // brings it back.
        let seed = seed_for(Zone::Mantlet, 76.2, |e| *e == Effect::Fault(ModuleKind::GunnerSight, Fault::SightOutOfTrue));
        g.debug_exterior_strike(ArmorPart::Turret, Vec3::new(0.0, 1.88, 1.589), t76, 300.0, seed);
        let (_, bore) = g.player.muzzle();
        let error_mils = g.sight_line().angle_between(bore) * 1000.0;
        assert!((1.9..5.1).contains(&error_mils), "sight wandered {error_mils} mils");
        assert!(g.module_ok(ModuleKind::GunnerSight), "an untrue sight still shows its reticle");
        let (gunner_eye, _, _) = g.gunner_interior_camera();
        assert!(work(&mut g, RepairJob::Module(ModuleKind::GunnerSight), gunner_eye), "boresighted");
        assert!(g.sight_line().angle_between(g.player.muzzle().1) < 1e-4);

        // A 4.5 cm hit on the cupola: the struck block goes opaque; a spare goes in from inside.
        let seed = seed_for(Zone::Cupola { block: 2 }, 45.0, |e| *e == Effect::VisionBlock { block: 2, holder: false });
        g.debug_exterior_strike(ArmorPart::Turret, tiger_periscopes::CUPOLA_CENTERS[2], at45, 300.0, seed);
        assert_eq!(g.exterior.blocks[2], Block::Opaque, "{}", g.exterior_damage_receipt());
        let commander_eye = turret.transform_point3(tiger_periscopes::COMMANDER_EYE);
        assert!(work(&mut g, RepairJob::VisionBlock(2), commander_eye), "block swapped");
        assert_eq!(g.exterior.blocks[2], Block::Clear);
        assert_eq!(g.exterior.spare_blocks, tiger_damage::SPARE_BLOCKS - 1);

        // A round at the turret base wedges a fragment in the ring: no traverse. It can only be
        // knocked free from outside; from inside the armour is in the way.
        let seed = seed_for(Zone::TurretRing, 76.2, |e| *e == Effect::Fault(ModuleKind::TurretTraverse, Fault::RingJammed));
        g.debug_exterior_strike(ArmorPart::Hull, Vec3::new(2.06, 2.1, 0.0), t76, 300.0, seed);
        assert_eq!(g.module_fault(ModuleKind::TurretTraverse), Fault::RingJammed, "{}", g.exterior_damage_receipt());
        assert!(!g.module_ok(ModuleKind::TurretTraverse));
        let jam = g.repair_point(RepairJob::Module(ModuleKind::TurretTraverse)).unwrap();
        g.carrying_track_kit = true;
        assert!(g.physical_repair_target(loader_eye, (jam - loader_eye).normalize()).is_none(),
            "the ring cannot be reached through the armour");
        let outward = Vec3::new(jam.x - g.player.pos.x, 0.0, jam.z - g.player.pos.z).normalize();
        let on_deck = jam + outward * 0.55 + Vec3::Y * 0.75;
        g.mode = Mode::Foot;
        assert!(work(&mut g, RepairJob::Module(ModuleKind::TurretTraverse), on_deck), "fragment knocked free");
        g.mode = Mode::Tank;
        assert!(g.module_ok(ModuleKind::TurretTraverse) && g.exterior.ring_jam.is_none());

        // Running gear and track: a 76 mm round on the left track band breaks the track and holes
        // wheels; the tank is slower but its wheels never immobilise it; the track does.
        let seed = seed_for(Zone::RunningGear { side: Side::Left, track: true }, 76.2, |e| *e == Effect::BreakTrack(Side::Left));
        g.debug_exterior_strike(ArmorPart::Hull, Vec3::new(2.06, 0.2, 0.0), t76, 300.0, seed);
        assert!(!g.module_ok(ModuleKind::LeftTrack));
        assert!(g.exterior.running_gear[0] < 1.0 && g.exterior.speed_factor() >= 0.5);

        // A hit on the bow plate tears off the spare links hung there.
        let seed = seed_for(Zone::BowPlate, 76.2, |e| matches!(e, Effect::SpareLinksLost(_)));
        g.debug_exterior_strike(ArmorPart::Hull, Vec3::new(0.0, 1.5, 3.56), t76, 300.0, seed);
        assert_eq!(g.exterior.spare_links, 0, "{}", g.exterior_damage_receipt());
        Ok(())
    }

    fn triangle_overlaps_box(tri: [Vec3; 3], min: Vec3, max: Vec3) -> bool {
        let center = (min + max) * 0.5;
        let half = (max - min) * 0.5;
        let tri = tri.map(|p| p - center);
        let edges = [tri[1]-tri[0], tri[2]-tri[1], tri[0]-tri[2]];
        let mut axes = vec![Vec3::X, Vec3::Y, Vec3::Z, edges[0].cross(edges[1])];
        for edge in edges { for axis in [Vec3::X, Vec3::Y, Vec3::Z] { axes.push(edge.cross(axis)); } }
        axes.into_iter().filter(|a|a.length_squared()>1e-14).all(|axis| {
            let axis = axis.normalize();
            let q = tri.map(|p| p.dot(axis));
            let radius = half.dot(axis.abs());
            q.into_iter().fold(f32::INFINITY,f32::min) < radius - 0.002
                && q.into_iter().fold(f32::NEG_INFINITY,f32::max) > -radius + 0.002
        })
    }

    fn triangles_intersect(a: [Vec3;3], b: [Vec3;3]) -> bool {
        let ea = [a[1]-a[0],a[2]-a[1],a[0]-a[2]];
        let eb = [b[1]-b[0],b[2]-b[1],b[0]-b[2]];
        let na = ea[0].cross(ea[1]);
        let nb = eb[0].cross(eb[1]);
        let mut axes = vec![na,nb];
        for x in ea { for y in eb { axes.push(x.cross(y)); } }
        // Include in-plane axes for coplanar triangle faces (the normal alone has zero width).
        for x in ea.into_iter().chain(eb) { axes.push(na.cross(x)); axes.push(nb.cross(x)); }
        axes.into_iter().filter(|axis|axis.length_squared()>1e-14).all(|axis| {
            let axis=axis.normalize();
            let pa=a.map(|p|p.dot(axis));let pb=b.map(|p|p.dot(axis));
            pa.into_iter().fold(f32::INFINITY,f32::min) <= pb.into_iter().fold(f32::NEG_INFINITY,f32::max)+1e-5
                && pb.into_iter().fold(f32::INFINITY,f32::min) <= pa.into_iter().fold(f32::NEG_INFINITY,f32::max)+1e-5
        })
    }

    fn triangle_hits_tube(triangle: [Vec3;3], tube: &CpuMesh) -> bool {
        triangle_overlaps_box(triangle,tube.min,tube.max) && tube.indices.chunks_exact(3).any(|ids| {
            let body = [0,1,2].map(|k|Vec3::from_array(tube.vertices[ids[k] as usize].pos));
            triangles_intersect(triangle,body)
        })
    }

    #[test]
    fn source_clearance_uses_shaft_and_warhead_shapes() {
        let tube=crate::panzerfaust::body_mesh();
        let shaft_gap=[Vec3::new(-0.06,-0.15,-0.40),Vec3::new(-0.06,0.15,-0.40),Vec3::new(-0.06,0.0,0.10)];
        assert!(triangle_overlaps_box(shaft_gap,tube.min,tube.max));
        assert!(!triangle_hits_tube(shaft_gap,&tube));
        let warhead_wall=shaft_gap.map(|p|Vec3::new(p.x,p.y,p.z*0.2+0.39));
        assert!(triangle_hits_tube(warhead_wall,&tube));
        assert!(triangle_hits_tube(shaft_gap.map(|p|Vec3::new(0.0,p.y,p.z)),&tube));
    }

    #[test]
    fn rack_clears_real_turret_loader_optics_and_escape_hatch_sweep() {
        let (turret, loader_lid, optics, _, escape, _) = tiger_periscopes::load_turret().unwrap();
        let optics: Vec<_> = optics.iter().map(|o| &o.mesh).collect();
        let stationary: Vec<_> = std::iter::once(&turret).chain(optics).collect();
        for step in 0..=20 {
            let phase = step as f32 / 20.0;
            let hatches = tiger_hatches::RoofHatches { phase: [phase;4], ..Default::default() };
            let poses = hatches.models(Mat4::IDENTITY, Mat4::IDENTITY);
            let objects: Vec<_> = stationary.iter().map(|m| (*m,Mat4::IDENTITY,"turret/optic"))
                .chain([(&loader_lid,poses[1],"loader lid"),(&escape,poses[3],"escape lid")]).collect();
            for i in 0..PANZERFAUST_RACK {
                let tube = panzerfaust_rack_mesh(i);
                let inverse = panzerfaust_rack_local_pose(i).inverse();
                for (mesh, model, name) in &objects {
                    let transform = inverse * *model;
                    for ids in mesh.indices.chunks_exact(3) {
                        let tri = [0,1,2].map(|k| {
                            transform.transform_point3(Vec3::from_array(mesh.vertices[ids[k] as usize].pos))
                        });
                        assert!(!triangle_hits_tube(tri,&tube),
                            "tube {i} intersects {name}, hatch phase {phase}: {tri:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn panzerfausts_leave_the_full_size_88mm_rounds_and_their_pull_paths_clear() {
        let round = crate::procedural_ordnance::ap_round_mesh();
        for i in 0..PANZERFAUST_RACK {
            let tube = panzerfaust_rack_mesh(i);
            let inverse = panzerfaust_rack_local_pose(i).inverse();
            for (slot, (frame, point, rotation)) in crate::interior_rig::SHELL_SLOTS.iter().enumerate() {
                // Preserve each full cartridge's body and the first 8 cm of its extraction path.
                // Hull frame is aligned with turret for the authored stowage acceptance pose.
                for pull in [0.0, 0.08] {
                    let q = Quat::from_array(*rotation);
                    let point = if *frame == crate::interior_rig::Frame::Hull {
                        *point - crate::tank::TIGER_PARTS.turret_pivot
                    } else {
                        *point
                    };
                    let model = inverse * Mat4::from_rotation_translation(q, point - q * Vec3::Z * pull);
                    for ids in round.indices.chunks_exact(3) {
                        let tri = [0,1,2].map(|k| {
                            model.transform_point3(Vec3::from_array(round.vertices[ids[k] as usize].pos))
                        });
                        assert!(!triangle_hits_tube(tri,&tube),
                            "tube {i} blocks ammunition slot {slot}, pull {pull}");
                    }
                }
            }
        }
    }

    #[test]
    fn rack_preserves_the_ordinary_loader_head_and_mechanism_sight_rays() {
        let brackets = panzerfaust_brackets();
        let gun = Mat4::from_translation(crate::tank::TIGER_PARTS.gun_trunnion);
        let mut targets = vec![
            gun.transform_point3(GUNLEVER_POS),
            spent_case_grip(gun, 0.0),
            TRAY_MOUTH,
            crate::game::tiger_periscopes::LOADER_ENTRY,
        ];
        targets.extend(
            crate::interior_rig::SHELL_SLOTS
                .iter()
                .map(|(frame, point, _)| {
                    if *frame == crate::interior_rig::Frame::Hull {
                        *point - crate::tank::TIGER_PARTS.turret_pivot
                    } else {
                        *point
                    }
                }),
        );
        // Default eye, its first step onto the basket footprint, and both crouch extremes.
        // All rack slots remain targetable from the authored post. After walking, check the
        // working mechanisms; another physical rack may naturally occlude rear shelf rows.
        for x in [-0.85, -0.638] {
            for crouch in [0.0, 0.50] {
                let eye = Vec3::new(x, crate::interior_rig::FLOOR_STAND_Y + 1.29 - crouch, 0.15);
                for i in 0..PANZERFAUST_RACK {
                    let tube = panzerfaust_rack_mesh(i);
                    let pose = panzerfaust_rack_local_pose(i);
                    let local = pose.inverse().transform_point3(eye);
                    assert!(
                        // Head clearance is 14 cm around the eye. The 28 cm movement radius
                        // includes shoulders below eye level, not a 56 cm wide skull.
                        local.distance(local.clamp(tube.min, tube.max)) >= 0.14,
                        "tube {i} occupies ordinary loader head space at {eye:?}"
                    );
                    for (target, point) in targets
                        .iter()
                        .take(if x < -0.80 { targets.len() } else { 4 })
                        .enumerate()
                    {
                        let delta = *point - eye;
                        assert!(
                            ray_loader_ammunition_mesh(
                                eye,
                                delta.normalize(),
                                pose,
                                delta.length(),
                                &tube
                            )
                            .is_none(),
                            "tube {i} steals target {target} at {eye:?}"
                        );
                    }
                }
                for (target, point) in targets
                    .iter()
                    .take(if x < -0.80 { targets.len() } else { 4 })
                    .enumerate()
                {
                    let delta = *point - eye;
                    assert!(
                        ray_loader_ammunition_mesh(
                            eye,
                            delta.normalize(),
                            Mat4::IDENTITY,
                            delta.length(),
                            &brackets
                        )
                        .is_none(),
                        "bracket blocks target {target} at {eye:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn rack_leaves_full_cartridge_hand_swing_and_chamber_guide_clear() {
        let round = crate::procedural_ordnance::ap_round_mesh();
        let eye = Vec3::new(-0.85, crate::interior_rig::FLOOR_STAND_Y + 1.29, 0.15);
        let target = (TRAY_MOUTH - eye).normalize();
        let gun = Mat4::from_translation(crate::tank::TIGER_PARTS.gun_trunnion);
        let chamber = chambered_round_model(gun);
        let ready = chamber
            .transform_point3(crate::interior_rig::ROUND_READY - crate::interior_rig::ROUND_SEAT);
        let mut poses = Vec::new();
        for step in 0..=40 {
            let t = step as f32 / 40.0;
            let forward = Vec3::Z.lerp(target, t).normalize();
            let right = forward.cross(Vec3::Y).normalize();
            let direction = (forward * 0.7 - right * 0.55 - Vec3::Y * 0.30).normalize();
            poses.push(Mat4::from_rotation_translation(
                Quat::from_rotation_arc(Vec3::Z, direction),
                eye + forward * 0.68 - Vec3::Y * 0.26,
            ));
            poses.push(Mat4::from_rotation_translation(
                chamber.to_scale_rotation_translation().1,
                ready.lerp(chamber.transform_point3(Vec3::ZERO), t),
            ));
        }
        for i in 0..PANZERFAUST_RACK {
            let tube = panzerfaust_rack_mesh(i);
            let inverse = panzerfaust_rack_local_pose(i).inverse();
            for (step, pose) in poses.iter().enumerate() {
                let model = inverse * *pose;
                for ids in round.indices.chunks_exact(3) {
                    let triangle = [0, 1, 2].map(|k| {
                        model
                            .transform_point3(Vec3::from_array(round.vertices[ids[k] as usize].pos))
                    });
                    assert!(
                        !triangle_hits_tube(triangle, &tube),
                        "tube {i} occupies the full cartridge handling path at step {step}"
                    );
                }
            }
        }
    }

    #[test]
    fn repair_requires_the_component_in_front_and_within_hand_reach() {
        let eye = Vec3::new(0.0, 1.5, 0.0);
        assert!(repair_in_reach(eye, Vec3::Z, eye + Vec3::Z * 1.0, 0.22));
        assert!(!repair_in_reach(eye, Vec3::Z, eye - Vec3::Z, 0.22));
        assert!(!repair_in_reach(eye, Vec3::Z, eye + Vec3::Z * 2.0, 0.22));
        assert!(!repair_in_reach(eye, Vec3::Z, eye + Vec3::X, 0.22));
    }

    #[test]
    fn launcher_and_spare_bombs_are_full_size_horizontal_and_pickable() {
        for i in 0..PANZERFAUST_RACK {
            let mesh = panzerfaust_rack_mesh(i);
            let model = panzerfaust_rack_local_pose(i);
            assert!((model.to_scale_rotation_translation().0 - Vec3::ONE).length() < 1e-6);
            assert!(model.transform_vector3(Vec3::Z).y.abs() < 1e-6);
            let eye = Vec3::new(-0.638, crate::interior_rig::FLOOR_STAND_Y + 1.29, 0.15);
            let target = model.transform_point3(mesh.center());
            let direction = (target - eye).normalize();
            assert!(ray_loader_ammunition_mesh(eye, direction, model, 1.85, &mesh).is_some());
            for j in 0..PANZERFAUST_RACK {
                if i != j {
                    assert!(ray_loader_ammunition_mesh(eye, direction,
                        panzerfaust_rack_local_pose(j), eye.distance(target), panzerfaust_rack_mesh(j)).is_none());
                }
            }
        }
    }
    #[test]
    fn returning_the_kit_restores_only_its_unfired_ammunition() {
        assert_eq!(panzerfaust_restowed_slots(3), [false, false, false]);
        assert_eq!(panzerfaust_restowed_slots(2), [false, false, true]);
        assert_eq!(panzerfaust_restowed_slots(1), [false, true, true]);
        assert_eq!(panzerfaust_restowed_slots(0), [true, true, true]);
        assert_eq!(panzerfaust_restowed_slots(u8::MAX), [false, false, false]);
    }

    #[test]
    fn stowed_kit_is_reachable_without_clicking_through_the_source_turret() {
        let (turret, loader, optics, _, escape, _) = tiger_periscopes::load_turret().unwrap();
        let eye = Vec3::new(-0.638, crate::interior_rig::FLOOR_STAND_Y + 1.29, 0.15);
        for i in 0..PANZERFAUST_RACK {
            let mesh = panzerfaust_rack_mesh(i);
            let model = panzerfaust_rack_local_pose(i);
            let direction = (model.transform_point3(mesh.center()) - eye).normalize();
            let pickup = ray_loader_ammunition_mesh(eye, direction, model, 1.85, mesh)
                .expect("item must be within physical pickup reach");
            for source in [&turret, &loader, &escape].into_iter().chain(optics.iter().map(|p| &p.mesh)) {
                assert!(ray_loader_ammunition_mesh(eye, direction, Mat4::IDENTITY, pickup - 0.035, source).is_none(),
                    "source turret blocks pickup of item {i}");
            }
        }
    }

}
