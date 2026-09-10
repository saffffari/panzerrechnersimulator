//! Embodied stowage: source-sized equipment stays in the scene when carried or set down.
use super::*;

/// A put-down kit follows the basket when inside; an outside kit remains at its world position.
#[derive(Clone, Copy)]
pub(super) struct ToolRest {
    pub turret: bool,
    pub model: Mat4,
}

pub(super) fn panzerfaust_rack_local_pose(index: usize) -> Mat4 {
    assert!(index < PANZERFAUST_RACK);
    // Two deep by three high, above both authored turret collars. A slight inward/rearward
    // bearing follows the port wall and clears the loader eyepiece at the forward tube ends.
    // All 1.045 m of every tube is retained; heads face the escape-hatch end of the compartment.
    let direction = Vec3::new(0.20, 0.0, -(1.0_f32 - 0.20 * 0.20).sqrt());
    Mat4::from_rotation_translation(crate::foot::held_weapon_rotation(direction, Vec3::Y),
        Vec3::new(-0.88 + (index % 2) as f32 * 0.16, 1.87 + (index / 2) as f32 * 0.17, -0.21))
}

pub(super) fn panzerfaust_brackets() -> CpuMesh {
    let mut parts = Vec::new();
    // Two upright straps and six narrow cradles. The contact positions lie on the launch tube,
    // clear of its trigger and warhead, rather than cutting across either silhouette.
    for along in [-0.35, 0.12] {
        let rear = panzerfaust_rack_local_pose(0).transform_point3(Vec3::Z * along);
        parts.push(box_mesh(Vec3::new(-1.075, 1.79, rear.z - 0.020),
            Vec3::new(-1.050, 2.25, rear.z + 0.020), [61,66,67]));
        for row in 0..3 {
            let far = panzerfaust_rack_local_pose(row * 2 + 1).transform_point3(Vec3::Z * along);
            parts.push(box_mesh(Vec3::new(-1.06, far.y - 0.052, far.z - 0.017),
                Vec3::new(far.x + 0.045, far.y - 0.035, far.z + 0.017), [61,66,67]));
            for column in 0..2 {
                let model = panzerfaust_rack_local_pose(row * 2 + column);
                for x in [-0.039, 0.039] {
                    parts.push(box_mesh(Vec3::new(x - 0.006, -0.045, along - 0.012),
                        Vec3::new(x + 0.006, 0.014, along + 0.012), [61,66,67]).transformed(model));
                }
            }
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

pub(super) fn toolbox_mesh() -> &'static CpuMesh {
    static MESH: OnceLock<CpuMesh> = OnceLock::new();
    MESH.get_or_init(|| crate::gltf_load::load_glb(&format!("{MP2}Props/toolbox_mx_1.glb"))
        .unwrap_or_else(|_| box_mesh(Vec3::new(-0.22, 0.0, -0.12),
            Vec3::new(0.22, 0.16, 0.12), [95, 64, 48])))
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
                self.hover_label = Some(if self.physical_repair_target(eye, look).is_some() {
                    "REPAIR DAMAGED PART [HOLD LMB]; Q/E: PUT TOOLS DOWN"
                } else { "TOOLBOX + HAMMER - CARRY TO DAMAGED PART; Q/E: PUT DOWN" });
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
            static TUBE: OnceLock<CpuMesh> = OnceLock::new();
            let mesh = TUBE.get_or_init(crate::panzerfaust::body_mesh);
            for i in 0..PANZERFAUST_RACK {
                if self.panzerfaust_rack_taken[i] { continue; }
                let model = self.player.turret_model() * panzerfaust_rack_local_pose(i);
                if let Some(t) = ray_loader_ammunition_mesh(eye, look, model, 1.85, mesh) {
                    if t < nearest && self.equipment_path_clear(eye, eye + look * t, 0.035) {
                        nearest = t;
                        self.hover = Some(Hotspot::PanzerfaustRack(i));
                        self.hover_label = Some("TAKE THIS PANZERFAUST [LMB]");
                    }
                }
            }
        }
    }

    pub(super) fn take_panzerfaust(&mut self, index: usize) -> bool {
        if index >= PANZERFAUST_RACK || self.panzerfaust_rack_taken[index]
            || !self.equipment_hands_free() { return false; }
        self.panzerfaust_rack_taken[index] = true;
        self.panzerfaust_rack_count = self.panzerfaust_rack_count.saturating_sub(1);
        self.panzerfaust_source = Some(index);
        self.panzerfaust_claimed = true;
        self.foot.panzerfaust_ready = true;
        self.foot.panzerfaust_spares = 0;
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
                .map_or(ground, |hit| models[hit.frame].transform_point3(hit.point).y);
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

    pub(super) fn physical_repair_target(&self, eye: Vec3, look: Vec3) -> Option<ModuleKind> {
        if !self.carrying_track_kit { return None; }
        self.modules.iter().filter(|m| !m.functional && repair_time(m.kind).is_some()
            && !matches!(m.kind, ModuleKind::LeftTrack | ModuleKind::RightTrack))
            .filter_map(|m| {
                let point = self.repair_point(m.kind);
                (repair_in_reach(eye, look, point, 0.22)
                    && self.equipment_path_clear(eye, point, 0.28))
                    .then_some((m.kind, eye.distance_squared(point)))
            }).min_by(|a,b| a.1.total_cmp(&b.1)).map(|(kind, _)| kind)
    }

    fn repair_point(&self, kind: ModuleKind) -> Vec3 {
        use ModuleKind::*;
        match kind {
            TurretTraverse => self.player.turret_model().transform_point3(crate::interior_rig::WHEEL2_POS),
            CoaxMg => self.player.visual_gun_model().transform_point3(Vec3::new(-0.22, -0.16, -0.25)),
            _ => self.player.hull_model().transform_point3(kind.station()),
        }
    }

    pub(super) fn update_carried_repair(&mut self, trigger: bool, dt: f32, eye: Vec3, look: Vec3) {
        if trigger && !self.primary_pickup_press {
            if let Some(kind) = self.physical_repair_target(eye, look) {
                if self.activity != Some(Activity::Repair(kind)) {
                    self.activity = Some(Activity::Repair(kind));
                    self.repair_t = 0.0;
                }
                let previous = self.repair_t;
                self.repair_t += dt;
                if (previous / 0.48) as u32 != (self.repair_t / 0.48) as u32 {
                    self.hammer_t = 0.38;
                    if let Some(audio) = &self.audio {
                        audio.play_at_volume(crate::audio::Sfx::HammerStrike, 0.42);
                    }
                }
                self.hover_label = Some("REPAIRING DAMAGED PART [HOLD LMB]");
                if self.repair_t >= repair_time(kind).unwrap() {
                    if let Some(module) = self.modules.iter_mut().find(|m| m.kind == kind) {
                        module.functional = true;
                    }
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
        let (turret, loader_lid, optics, _, escape) = tiger_periscopes::load_turret().unwrap();
        let tube = crate::panzerfaust::body_mesh();
        let optics: Vec<_> = optics.iter().map(|o| &o.mesh).collect();
        let stationary: Vec<_> = std::iter::once(&turret).chain(optics).collect();
        for step in 0..=20 {
            let phase = step as f32 / 20.0;
            let hatches = tiger_hatches::RoofHatches { phase: [phase;4], ..Default::default() };
            let poses = hatches.models(Mat4::IDENTITY, Mat4::IDENTITY);
            let objects: Vec<_> = stationary.iter().map(|m| (*m,Mat4::IDENTITY,"turret/optic"))
                .chain([(&loader_lid,poses[1],"loader lid"),(&escape,poses[3],"escape lid")]).collect();
            for i in 0..PANZERFAUST_RACK {
                let inverse = panzerfaust_rack_local_pose(i).inverse();
                for (mesh, model, name) in &objects {
                    let transform = inverse * *model;
                    for ids in mesh.indices.chunks_exact(3) {
                        let tri = [0,1,2].map(|k|transform.transform_point3(Vec3::from_array(mesh.vertices[ids[k] as usize].pos)));
                        assert!(!triangle_hits_tube(tri,&tube),
                            "tube {i} intersects {name}, hatch phase {phase}: {tri:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn panzerfausts_leave_the_full_size_88mm_rounds_and_their_pull_paths_clear() {
        let tube = crate::panzerfaust::body_mesh();
        let round = crate::procedural_ordnance::ap_round_mesh();
        for i in 0..PANZERFAUST_RACK {
            let inverse = panzerfaust_rack_local_pose(i).inverse();
            for (slot, (_, point, rotation)) in crate::interior_rig::SHELL_SLOTS.iter().enumerate() {
                // Preserve each full cartridge's body and the first 8 cm of its extraction path.
                // Hull frame is aligned with turret for the authored stowage acceptance pose.
                for pull in [0.0, 0.08] {
                    let q = Quat::from_array(*rotation);
                    let model = inverse * Mat4::from_rotation_translation(q, *point - q * Vec3::Z * pull);
                    for ids in round.indices.chunks_exact(3) {
                        let tri = [0,1,2].map(|k|model.transform_point3(Vec3::from_array(round.vertices[ids[k] as usize].pos)));
                        assert!(!triangle_hits_tube(tri,&tube),
                            "tube {i} blocks ammunition slot {slot}, pull {pull}");
                    }
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
    fn full_size_rack_tubes_are_separate_and_individually_pickable() {
        let mesh = crate::panzerfaust::body_mesh();
        for i in 0..PANZERFAUST_RACK {
            let model = panzerfaust_rack_local_pose(i);
            assert!((model.to_scale_rotation_translation().0 - Vec3::ONE).length() < 1e-6);
            let eye = model.transform_point3(Vec3::Z * -1.20);
            let direction = model.transform_vector3(Vec3::Z);
            assert!(ray_loader_ammunition_mesh(eye, direction, model, 1.0, &mesh).is_some());
            for j in 0..PANZERFAUST_RACK {
                if i != j {
                    assert!(ray_loader_ammunition_mesh(eye, direction,
                        panzerfaust_rack_local_pose(j), 1.0, &mesh).is_none());
                }
            }
        }
    }
}
