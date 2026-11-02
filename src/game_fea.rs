//! Game side of the layered damage model (`crate::fea_damage`, docs/FEA_DAMAGE_MODEL_20261102.md):
//! the player's Tiger's shot line in its hull frame, the chain's consequences for its modules, the
//! residual penetrator's own path through crew and live rounds, and rounds lodged in the plate or
//! inside the vehicle.
//!
//! The struck plate's outcome is the existing calibrated `ImpactEvent`; nothing here changes
//! whether a plate is perforated. A perforated casing knocks its system out; a casing struck but
//! not perforated leaves the system working. The penetrator reaches crew and live rounds through
//! the same anatomical volumes and cartridge boxes as the fragments, with its real mass and speed;
//! injury outcomes stay with the existing crew model.
use super::*;
use crate::fea_damage::components::{self, Side, System};
use crate::fea_damage::{chain, lodged};

/// Contact identity of the residual penetrator in fragment contact records. Fragments use
/// `penetration_id << 10 | index`; the penetrator sets the top bit.
pub(super) fn penetrator_contact_id(penetration_id: u64) -> u64 {
    (1u64 << 63) | (penetration_id & ((1u64 << 62) - 1))
}

/// A round held by the Tiger's steel: in the struck plate (nose in, base outside) or inside a
/// component it stopped in.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct LodgedRound {
    pub(super) id: u64,
    #[serde(with = "crate::ballistics::shell_spec_ref")]
    pub(super) shell: &'static crate::ballistics::ShellSpec,
    pub(super) frame: BloodFrame,
    /// Nose tip and travel direction (nose forward) in `frame`.
    pub(super) tip_local: Vec3,
    pub(super) dir_local: Vec3,
    /// How far it went into the layer that holds it, m.
    pub(super) depth_m: f32,
    pub(super) in_struck_plate: bool,
    /// The component it stopped in, if any (catalog id).
    pub(super) component: Option<String>,
    pub(super) extraction: lodged::Extraction,
    pub(super) struck_at_s: f32,
}

/// One chain run on the player's Tiger, kept for diagnostics, receipts and cross sections.
#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct FeaChainReceipt {
    pub(super) shell: &'static str,
    pub(super) terminal: crate::ballistics::Terminal,
    pub(super) entry_hull: Vec3,
    pub(super) dir_hull: Vec3,
    pub(super) far_wall_mm: Option<f32>,
    pub(super) result: chain::ChainResult,
    pub(super) knocked_out: Vec<ModuleKind>,
    pub(super) struck_not_breached: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct FeaRuntime {
    pub(super) lodged: Vec<LodgedRound>,
    pub(super) receipts: Vec<FeaChainReceipt>,
    next_id: u64,
}

impl FeaRuntime {
    /// Receipts kept for diagnostics (oldest dropped first; lodged rounds are never dropped).
    pub(super) const MAX_RECEIPTS: usize = 32;
    fn push_receipt(&mut self, receipt: FeaChainReceipt) {
        if self.receipts.len() >= Self::MAX_RECEIPTS {
            self.receipts.remove(0);
        }
        self.receipts.push(receipt);
    }
}

/// The module a component system belongs to, if the game has one for it.
pub(super) fn module_for_system(system: System) -> Option<ModuleKind> {
    match system {
        System::Engine => Some(ModuleKind::Engine),
        System::Fuel(_) => Some(ModuleKind::Fuel),
        System::Gearbox | System::Steering => Some(ModuleKind::Transmission),
        System::FinalDrive(Side::Left) => Some(ModuleKind::LeftTrack),
        System::FinalDrive(Side::Right) => Some(ModuleKind::RightTrack),
        System::Radio => Some(ModuleKind::Radio),
        System::HullMg => Some(ModuleKind::HullMg),
        System::MainGun => Some(ModuleKind::MainGun),
        System::CoaxMg => Some(ModuleKind::CoaxMg),
        System::GunnerSight => Some(ModuleKind::GunnerSight),
        System::TurretTraverse => Some(ModuleKind::TurretTraverse),
        // No live cooling circuit, air-filter state or seat state yet: recorded in the receipt.
        System::AirFilters | System::Cooling(_) | System::Ammunition | System::Seat => None,
    }
}

impl Game {
    /// Turret yaw and gun elevation as the component envelopes need them.
    fn fea_articulation(&self) -> components::Articulation {
        let p = &self.player;
        components::Articulation {
            hull_to_turret: (Mat4::from_translation(p.parts.turret_pivot) * Mat4::from_rotation_y(p.turret_yaw)).inverse(),
            gun_elevation: p.gun_pitch,
        }
    }

    /// The shot line through the player's Tiger, hull frame, for a strike at `impact_world`
    /// travelling along `dir_world`, with the far wall found by coming back along the line from
    /// outside to the first source surface (classified by the same plate classes as the entry).
    pub(super) fn fea_shot_line(&self, impact_world: Vec3, dir_world: Vec3,
        event: &crate::ballistics::ImpactEvent) -> chain::ShotLine {
        let hull = self.player.hull_model();
        let inv = hull.inverse();
        let dir_world = dir_world.normalize_or_zero();
        let entry = inv.transform_point3(impact_world);
        let dir = inv.transform_vector3(dir_world).normalize_or_zero();
        let cos = event.impact_angle_deg.to_radians().cos().clamp(0.05, 1.0);
        let plate_path = event.plate.thickness_mm * 0.001 / cos;
        const REACH_M: f32 = 12.0;
        let models = self.fire_surface_models();
        let far = impact_world + dir_world * REACH_M;
        let near = impact_world + dir_world * (plate_path + 0.02);
        let far_wall = projectile_armor_source::contact(&self.cabin_fire, &models, &self.player,
            ProjectileVehicle::Player, crate::ballistics::ArmorScheme::TigerI, far, near)
            .and_then(|hit| {
                let ProjectileContactKind::Vehicle(armor) = hit.kind else { return None };
                let plate = armor.surface_plate?.plate;
                let s_outer = (hit.pos - impact_world).dot(dir_world);
                let normal_hull = inv.transform_vector3(armor.world_normal).normalize_or_zero();
                let cos_far = dir.dot(normal_hull).abs().clamp(0.05, 1.0);
                let s_inner = s_outer - plate.thickness_mm * 0.001 / cos_far;
                (s_inner > plate_path + 0.01).then_some(chain::FarWall {
                    s_inner,
                    plate_mm: plate.thickness_mm,
                    material: crate::fea_damage::materials::MaterialId::RolledArmour,
                    cos_obliquity: cos_far,
                    normal: normal_hull,
                })
            });
        chain::ShotLine { entry, dir, plate_path_m: plate_path, far_wall, max_s: REACH_M }
    }

    /// Run the chain for a kinetic strike on the player's Tiger. `None` for HE and HEAT, whose
    /// terminals keep their existing rules.
    pub(super) fn fea_run_chain(&self, impact_world: Vec3, dir_world: Vec3,
        event: &crate::ballistics::ImpactEvent) -> Option<(chain::ShotLine, chain::ChainResult)> {
        use crate::ballistics::ShellKind;
        if !matches!(event.shell.kind, ShellKind::Ap | ShellKind::Apcbc | ShellKind::Apcr) {
            return None;
        }
        let line = self.fea_shot_line(impact_world, dir_world, event);
        let hull_inv = self.player.hull_model().inverse();
        let rounds: Vec<chain::RoundBox> = self.live_ammunition_in_frame(BloodFrame::Hull).iter().map(|r| {
            chain::RoundBox { location_id: r.stable_id, center: r.center,
                axes: [r.rotation * Vec3::X, r.rotation * Vec3::Y, r.rotation * Vec3::Z], half: r.half_extents }
        }).collect();
        let fuel = self.tank_fuel_liters().unwrap_or(TIGER_FUEL_CAPACITY_L);
        let state = chain::VehicleState {
            articulation: self.fea_articulation(),
            fuel_fill: (fuel / TIGER_FUEL_CAPACITY_L).clamp(0.0, 1.0),
            gravity_hull: hull_inv.transform_vector3(Vec3::NEG_Y).normalize_or_zero(),
            live_rounds: &rounds,
        };
        let layers = chain::build_layers(&line, components::tiger_components(), &state);
        let result = chain::run(event, &layers, crate::fea_damage::materials::AIR.density);
        Some((line, result))
    }

    /// Record a round the chain left lodged. `part` is the struck armour part (the frame of a round
    /// held in the plate); a round stopped inside goes to the frame of the component holding it.
    fn fea_record_lodged(&mut self, line: &chain::ShotLine, result: &chain::ChainResult,
        event: &crate::ballistics::ImpactEvent, part: ArmorPart, now_s: f32) -> Option<u64> {
        let lodging = result.lodged?;
        let crossing = result.crossings.get(lodging.crossing)?;
        let shell = event.shell;
        let d = shell.caliber_mm * 0.001;
        let nose = lodged::nose_length_calibres(shell.kind) * d;
        let tip_hull = line.entry + line.dir * lodging.tip_s;
        let component = crossing.layer.kind.component();
        let set = components::tiger_components();
        let component_frame = component.and_then(|c| set.envelopes.iter().find(|e| e.spec == c)).map(|e| e.frame);
        let frame = match (lodging.in_struck_plate, part, component_frame) {
            (true, ArmorPart::Turret, _) => BloodFrame::Turret,
            (true, ArmorPart::Hull, _) => BloodFrame::Hull,
            (false, _, Some(components::Frame::Turret | components::Frame::Gun)) => BloodFrame::Turret,
            _ => BloodFrame::Hull,
        };
        let hull = self.player.hull_model();
        let parent_inv = blood_parent_model(&self.player, frame).inverse();
        let tip_local = parent_inv.transform_point3(hull.transform_point3(tip_hull));
        let dir_local = parent_inv.transform_vector3(hull.transform_vector3(line.dir)).normalize_or_zero();
        let yield_pa = match crossing.layer.kind {
            chain::LayerKind::Casing { material, .. } => material.get().yield_stress,
            chain::LayerKind::Interior { matrix, .. } => matrix.get().yield_stress,
            _ => crate::fea_damage::materials::ROLLED_ARMOUR.yield_stress,
        };
        // Held over the body length past its nose; inside machinery it is not reachable by hand.
        let held = (lodging.depth_m - nose).max(0.0);
        let reachable = lodging.in_struck_plate
            || matches!(crossing.layer.kind, chain::LayerKind::Casing { .. });
        let extraction = lodged::extraction(lodged::estimated_grip_pa(yield_pa), d, held, reachable);
        let id = self.fea.next_id;
        self.fea.next_id += 1;
        let name = component.map(|c| components::COMPONENT_SPECS[c].id.to_string());
        println!("[fea] {} lodged {} ({:.0} mm in{}): {:.1} kN to start, {:?}", shell.name,
            if lodging.in_struck_plate { "in the struck plate".to_string() } else {
                format!("inside, in the {}", name.as_deref().unwrap_or("structure")) },
            lodging.depth_m * 1000.0, if reachable { "" } else { ", not reachable" },
            extraction.force_n / 1000.0, extraction.removal);
        self.fea.lodged.push(LodgedRound { id, shell, frame, tip_local, dir_local, depth_m: lodging.depth_m,
            in_struck_plate: lodging.in_struck_plate, component: name, extraction, struck_at_s: now_s });
        Some(id)
    }

    /// A kinetic round the struck plate stopped: it may stay lodged in it.
    pub(super) fn fea_nonpenetration(&mut self, armor: &VehicleArmorContact,
        event: &crate::ballistics::ImpactEvent, impact_world: Vec3, dir_world: Vec3, now_s: f32) {
        let Some((line, result)) = self.fea_run_chain(impact_world, dir_world, event) else { return };
        self.fea_record_lodged(&line, &result, event, armor.part, now_s);
        self.fea.push_receipt(FeaChainReceipt { shell: event.shell.name, terminal: event.terminal,
            entry_hull: line.entry, dir_hull: line.dir, far_wall_mm: line.far_wall.map(|w| w.plate_mm),
            result, knocked_out: Vec::new(), struck_not_breached: Vec::new() });
    }

    /// The layered consequences of a perforation: the systems the residual round breached, and its
    /// own contacts with crew volumes and live cartridges in the penetration frame.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fea_penetration_consequences(&mut self, impact_world: Vec3, dir_world: Vec3,
        event: &crate::ballistics::ImpactEvent, part: ArmorPart, penetration_id: u64, frame: BloodFrame,
        solo_body: Option<Vec3>, couch_bodies: &[CouchCrewBody], live_ammunition: &[LiveAmmoRound],
        result: &mut SpallResult, contacts: &mut Vec<FragmentDamageContact>) {
        let Some((line, chain_result)) = self.fea_run_chain(impact_world, dir_world, event) else { return };
        let hull = self.player.hull_model();
        let parent_inv = blood_parent_model(&self.player, frame).inverse();
        let to_frame = |p_hull: Vec3| parent_inv.transform_point3(hull.transform_point3(p_hull));
        let dir_frame = parent_inv.transform_vector3(hull.transform_vector3(line.dir)).normalize_or_zero();
        let shell = event.shell;
        let mass = shell.mass_kg;
        let radius = shell.caliber_mm * 0.0005;
        let contact_id = penetrator_contact_id(penetration_id);
        let mut knocked_out = Vec::new();
        let mut struck_not_breached = Vec::new();

        // Systems breached by the round itself.
        for hit in &chain_result.hits {
            let spec = &components::COMPONENT_SPECS[hit.component];
            if !hit.breached {
                struck_not_breached.push(spec.id.to_string());
                continue;
            }
            let Some(kind) = module_for_system(spec.system) else { continue };
            let Some(module) = self.modules.iter_mut().find(|m| m.kind == kind) else { continue };
            // Where it entered this component, at the speed it had there.
            let Some(first) = chain_result.crossings.iter().find(|c| c.layer.kind.component() == Some(hit.component)) else { continue };
            if module.functional {
                module.functional = false;
                module.fault = tiger_damage::Fault::Spall;
            }
            if !result.hit_modules.contains(&kind) {
                result.hit_modules.push(kind);
            }
            // Same existing rule as a fragment reaching these modules.
            result.fire |= matches!(kind, ModuleKind::Fuel | ModuleKind::Engine);
            knocked_out.push(kind);
            let p = to_frame(line.entry + line.dir * first.layer.s0);
            contacts.push(FragmentDamageContact { fragment_id: contact_id, target: FragmentDamageTarget::Module(kind),
                body_part: None, local_point: p, local_velocity: dir_frame * first.v_in, speed_mps: first.v_in,
                energy_j: 0.5 * mass * first.v_in * first.v_in, internal_blast_origin_local: None });
        }

        // The penetrator's straight path, crossing by crossing, against crew and live rounds.
        let mut contacted = vec![0u16; couch_bodies.len()];
        for crossing in chain_result.crossings.iter().skip(1) {
            if crossing.v_in <= 0.0 {
                break;
            }
            let end_s = match crossing.fate {
                chain::Fate::Stopped { depth_m } => crossing.layer.s0 + depth_m,
                _ => crossing.layer.s1,
            };
            let from = to_frame(line.entry + line.dir * crossing.layer.s0);
            let to = to_frame(line.entry + line.dir * end_s);
            let speed = crossing.v_in;
            let energy = 0.5 * mass * speed * speed;
            if let chain::LayerKind::LiveRound { location_id } = crossing.layer.kind {
                if !result.ammo {
                    if let Some(round) = live_ammunition.iter().find(|r| r.stable_id == location_id) {
                        if let Some(module) = self.modules.iter_mut().find(|m| m.kind == ModuleKind::AmmoRack) {
                            module.functional = false;
                            module.fault = tiger_damage::Fault::Spall;
                        }
                        if !result.hit_modules.contains(&ModuleKind::AmmoRack) {
                            result.hit_modules.push(ModuleKind::AmmoRack);
                        }
                        result.ammo = true;
                        contacts.push(FragmentDamageContact { fragment_id: contact_id,
                            target: FragmentDamageTarget::Module(ModuleKind::AmmoRack), body_part: None,
                            local_point: from, local_velocity: dir_frame * speed, speed_mps: speed, energy_j: energy,
                            internal_blast_origin_local: Some(round.center) });
                    }
                }
            }
            if let Some(center) = solo_body {
                if !result.hit_body {
                    if let Some(f) = swept_segment_sphere_fraction(from, to, center, 0.35 + radius) {
                        result.hit_body = true;
                        contacts.push(FragmentDamageContact { fragment_id: contact_id, target: FragmentDamageTarget::SoloBody,
                            body_part: None, local_point: from.lerp(to, f), local_velocity: dir_frame * speed,
                            speed_mps: speed, energy_j: energy, internal_blast_origin_local: None });
                    }
                }
            }
            for (index, body) in couch_bodies.iter().enumerate() {
                if swept_segment_sphere_fraction(from, to, body.pos, 1.05 + radius).is_none() {
                    continue;
                }
                let mut hits: Vec<(f32, crew_physics::CrewBodyPart)> = body.hit_volumes.iter().filter_map(|volume| {
                    let bit = 1u16 << volume.part.index();
                    if contacted[index] & bit != 0 { return None; }
                    crew_damage::swept_sphere_obb_fraction(from, to, radius, *volume).map(|f| (f, volume.part))
                }).collect();
                hits.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.index().cmp(&b.1.index())));
                for (f, part) in hits {
                    contacted[index] |= 1u16 << part.index();
                    contacts.push(FragmentDamageContact { fragment_id: contact_id,
                        target: FragmentDamageTarget::CouchCrew(body.casualty), body_part: Some(part),
                        local_point: from.lerp(to, f), local_velocity: dir_frame * speed, speed_mps: speed,
                        energy_j: energy, internal_blast_origin_local: None });
                }
            }
        }
        let now = self.armor_clock_s;
        self.fea_record_lodged(&line, &chain_result, event, part, now);
        println!("[fea] {}: {}", shell.name, chain_result.summary());
        if !knocked_out.is_empty() || !struck_not_breached.is_empty() {
            println!("[fea]   breached {:?}; struck, not breached {:?}", knocked_out, struck_not_breached);
        }
        self.fea.push_receipt(FeaChainReceipt { shell: shell.name, terminal: event.terminal, entry_hull: line.entry,
            dir_hull: line.dir, far_wall_mm: line.far_wall.map(|w| w.plate_mm), result: chain_result,
            knocked_out, struck_not_breached });
    }
}
