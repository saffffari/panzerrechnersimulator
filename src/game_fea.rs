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
use crate::fea_damage::{chain, lattice, lodged};

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

/// The voxel lattice's solve of one strike on the player's Tiger, in the hull frame.
#[derive(Clone, Debug)]
pub(super) struct LatticeOutcome {
    pub(super) measured: crate::armor_damage::MeasuredScar,
    pub(super) geometry: lattice::MeasuredGeometry,
    pub(super) clusters: Vec<lattice::Cluster>,
    /// The plate frame: origin at the strike on the struck face, x along the shot's in-plane
    /// direction, y across it, z into the plate (all hull-local).
    pub(super) origin_hull: Vec3,
    pub(super) axes_hull: [Vec3; 3],
    pub(super) thickness_m: f32,
    pub(super) particles: usize,
    pub(super) steps: u64,
    pub(super) cell_mm: f32,
    pub(super) wall_ms: f32,
    pub(super) ledger: lattice::Ledger,
    pub(super) grip_normal_n: f32,
    /// The rim's solved cooling (`armor_damage::RIM_CURVE_TIMES`).
    pub(super) rim_curve: crate::armor_damage::RimCurve,
}

impl LatticeOutcome {
    pub(super) fn plate_to_hull(&self, p: Vec3) -> Vec3 {
        self.origin_hull + self.axes_hull[0] * p.x + self.axes_hull[1] * p.y + self.axes_hull[2] * p.z
    }
    pub(super) fn plate_dir_to_hull(&self, v: Vec3) -> Vec3 {
        self.axes_hull[0] * v.x + self.axes_hull[1] * v.y + self.axes_hull[2] * v.z
    }
}

/// Everything solved for the strike being resolved now; consumed by the scar, the penetration's
/// consequences and its fragments, then cleared.
#[derive(Clone, Debug)]
pub(super) struct ImpactSolve {
    pub(super) projectile_id: u64,
    pub(super) line: chain::ShotLine,
    pub(super) chain: chain::ChainResult,
    pub(super) lattice: Option<LatticeOutcome>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct FeaRuntime {
    pub(super) lodged: Vec<LodgedRound>,
    pub(super) receipts: Vec<FeaChainReceipt>,
    pub(super) current: Option<ImpactSolve>,
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
        event: &crate::ballistics::ImpactEvent, part: ArmorPart, now_s: f32, grip_normal_n: Option<f32>) -> Option<u64> {
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
        // The lattice's measured grip on the body when it solved this plate; otherwise the named
        // stage-1 estimate.
        let extraction = match (lodging.in_struck_plate, grip_normal_n) {
            (true, Some(grip)) => lodged::extraction_from_grip(grip, held, reachable),
            _ => lodged::extraction(lodged::estimated_grip_pa(yield_pa), d, held, reachable),
        };
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
        let solved = self.fea.current.as_ref().map(|s| (s.line, s.chain.clone(), s.lattice.as_ref().map(|l| l.grip_normal_n)));
        let Some((line, result, grip)) = solved.or_else(|| self.fea_run_chain(impact_world, dir_world, event)
            .map(|(l, r)| (l, r, None))) else { return };
        self.fea_record_lodged(&line, &result, event, armor.part, now_s, grip);
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
        let current = self.fea.current.as_ref().filter(|s| s.projectile_id == penetration_id)
            .map(|s| (s.line, s.chain.clone(), s.lattice.as_ref().map(|l| l.grip_normal_n)));
        let Some((line, chain_result, grip)) = current.or_else(|| self.fea_run_chain(impact_world, dir_world, event)
            .map(|(l, r)| (l, r, None))) else { return };
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
        self.fea_record_lodged(&line, &chain_result, event, part, now, grip);
        println!("[fea] {}: {}", shell.name, chain_result.summary());
        if !knocked_out.is_empty() || !struck_not_breached.is_empty() {
            println!("[fea]   breached {:?}; struck, not breached {:?}", knocked_out, struck_not_breached);
        }
        self.fea.push_receipt(FeaChainReceipt { shell: shell.name, terminal: event.terminal, entry_hull: line.entry,
            dir_hull: line.dir, far_wall_mm: line.far_wall.map(|w| w.plate_mm), result: chain_result,
            knocked_out, struck_not_breached });
    }

    /// Solve one kinetic strike on the player's Tiger: the chain, then the voxel lattice of the
    /// struck plate. The lattice runs to completion inside this tick: a long solve holds the tick
    /// (logged) rather than letting any consumer see a half-solved plate or skip the contact.
    pub(super) fn fea_solve_impact(&mut self, projectile_id: u64, seed: u32, armor: &VehicleArmorContact,
        event: &crate::ballistics::ImpactEvent, impact_world: Vec3, dir_world: Vec3) {
        use crate::fea_damage::chain::Fate;
        self.fea.current = None;
        let Some((line, chain_result)) = self.fea_run_chain(impact_world, dir_world, event) else { return };
        let motion = chain_result.crossings.first().and_then(|c| match c.fate {
            Fate::Perforated | Fate::Burst => Some(lattice::Motion::Perforate { v_out: c.v_out }),
            Fate::Stopped { depth_m } => Some(lattice::Motion::Stop { depth_m }),
            // Ricochets and shatters keep the empirical scar: their path out of the plate is not
            // prescribed to the lattice (named).
            _ => None,
        });
        let hull_inv = self.player.hull_model().inverse();
        let lattice = motion.map(|motion| {
            let mut e3 = -hull_inv.transform_vector3(armor.world_normal).normalize_or_zero();
            if e3.dot(line.dir) < 0.0 {
                e3 = -e3;
            }
            let tangential = line.dir - e3 * line.dir.dot(e3);
            let e1 = tangential.try_normalize().unwrap_or_else(|| e3.any_orthonormal_vector());
            let e2 = e3.cross(e1);
            let obliquity = line.dir.dot(e3).clamp(-1.0, 1.0).acos().min(1.4);
            let shell = event.shell;
            let calibre = shell.caliber_mm * 0.001;
            // A sub-calibre core goes into the plate; the carrier is not modelled (named).
            let body_d = if shell.kind == crate::ballistics::ShellKind::Apcr { 0.37 * calibre } else { calibre };
            let material = match event.plate.material {
                crate::ballistics::ArmorMaterial::Cast => crate::fea_damage::materials::MaterialId::CastArmour,
                _ => crate::fea_damage::materials::MaterialId::RolledArmour,
            };
            let local_seed = crate::armor_damage::position_seed(armor.local_point) ^ seed.rotate_left(13);
            let impact = lattice::PlateImpact {
                material,
                thickness_m: event.plate.thickness_mm * 0.001,
                obliquity_rad: obliquity,
                projectile: lattice::ProjectileShape {
                    mass_kg: shell.mass_kg,
                    radius_m: 0.5 * body_d,
                    nose_m: lodged::nose_length_calibres(shell.kind) * body_d,
                },
                v_in: event.striking_v,
                motion,
                seed: local_seed,
            };
            let started = std::time::Instant::now();
            let mut solver = lattice::Lattice::new(impact);
            solver.solve();
            let wall_ms = started.elapsed().as_secs_f32() * 1000.0;
            let geometry = solver.measure();
            let ledger = solver.ledger_now();
            let measured = crate::armor_damage::MeasuredScar {
                perforated: matches!(motion, lattice::Motion::Perforate { .. }),
                aperture_mm: geometry.aperture_mm,
                back_radius_mm: geometry.back_radius_mm,
                petals: geometry.petals,
                petal_mm: geometry.petal_mm,
                plug: geometry.plug,
                cavity_mm: geometry.cavity_mm,
                worked_radius_mm: geometry.worked_radius_mm,
                rim_kelvin: geometry.rim_kelvin,
            };
            println!("[fea] lattice {} particles, {:.1} mm cells, {} steps, {:.0} ms (the tick waited for it): aperture {:.0} mm, petals {}, plug {}, lip {:.0} mm, bulge {:.0} mm, rim {:.0} K, energy closure {:.2} %",
                solver.n, solver.h * 1000.0, solver.step, wall_ms, geometry.aperture_mm, geometry.petals, geometry.plug,
                geometry.lip_mm, geometry.bulge_mm, geometry.rim_kelvin, ledger.closure() * 100.0);
            LatticeOutcome {
                measured,
                geometry,
                clusters: solver.clusters(),
                origin_hull: line.entry,
                axes_hull: [e1, e2, e3],
                thickness_m: impact.thickness_m,
                particles: solver.n,
                steps: solver.step,
                cell_mm: solver.h * 1000.0,
                wall_ms,
                ledger,
                grip_normal_n: solver.body_grip_normal_n(),
                rim_curve: {
                    let samples = solver.rim_cooling(&crate::armor_damage::RIM_CURVE_TIMES);
                    let mut kelvin = [crate::armor_damage::AMBIENT_K; 10];
                    for (k, v) in kelvin.iter_mut().zip(samples) { *k = v; }
                    crate::armor_damage::RimCurve { kelvin }
                },
            }
        });
        self.fea.current = Some(ImpactSolve { projectile_id, line, chain: chain_result, lattice });
    }

    /// The measured terminal geometry and solved rim cooling for the strike being resolved, if
    /// the lattice solved it.
    pub(super) fn fea_measured_scar(&self) -> Option<(crate::armor_damage::MeasuredScar, Option<crate::armor_damage::RimCurve>)> {
        self.fea.current.as_ref()?.lattice.as_ref().map(|l| (l.measured, Some(l.rim_curve)))
    }

    /// The penetration's fragment population from the lattice: every cluster that broke free on
    /// the inside of the plate (its mass, centre, velocity and spin as solved), and, for a filled
    /// round that burst, its body's steel (mass minus filler) partitioned into the existing authored
    /// size shape and speeds (named: no fragmentation law). `None` when no lattice solved it.
    pub(super) fn fea_penetration_fragments(&self, penetration_id: u64, frame: BloodFrame,
        event: &crate::ballistics::ImpactEvent, seed: u32) -> Option<Vec<HotFragment>> {
        let solve = self.fea.current.as_ref().filter(|s| s.projectile_id == penetration_id)?;
        let lat = solve.lattice.as_ref()?;
        let hull = self.player.hull_model();
        let parent_inv = blood_parent_model(&self.player, frame).inverse();
        let to_frame = |p_plate: Vec3| parent_inv.transform_point3(hull.transform_point3(lat.plate_to_hull(p_plate)));
        let dir_to_frame = |v: Vec3| parent_inv.transform_vector3(hull.transform_vector3(lat.plate_dir_to_hull(v)));
        let profile = fragment_shower_profile(true, event.behind_armor, event.shell.caliber_mm, event.residual_v,
            if event.shell.kind == crate::ballistics::ShellKind::Heat { 0.0 } else { event.shell.filler_g });
        let penetration_seed = seed ^ penetration_id as u32 ^ (penetration_id >> 32) as u32 ^ 0x5A11_5A11;
        let mut out: Vec<HotFragment> = Vec::new();
        let make = |index: usize, pos: Vec3, vel: Vec3, size: f32, spin: Vec3, temp_k: f32| {
            let s = penetration_seed.wrapping_add((index as u32).wrapping_mul(2_246_822_519));
            HotFragment {
                id: (penetration_id << 10) | index as u64,
                penetration_id,
                active: true,
                settled: false,
                origin: pos,
                prev_pos: pos,
                entered_compartment: fragment_inside_compartment(frame, pos, size * 0.52),
                audio_whizzed: false,
                frame,
                pos,
                vel,
                size,
                ang: Vec3::new(hash01(s ^ 0x11), hash01(s ^ 0x22), hash01(s ^ 0x33)) * std::f32::consts::TAU,
                spin,
                age: 0.0,
                life: profile.fragment_life * (0.82 + 0.30 * hash01(s ^ 0x19)),
                bounces: 0,
                seed: s,
                temp_k,
                lodged: false,
            }
        };
        // Plate clusters on the inside, moving inward. Exterior ejecta leave as the cosmetic impact
        // sparks only (named): this pool is the interior's.
        for cluster in &lat.clusters {
            if out.len() >= N_HOT_FRAGMENTS {
                break;
            }
            if cluster.centre.z < 0.5 * lat.thickness_m || cluster.velocity.z <= 0.0 {
                continue;
            }
            let index = out.len();
            out.push(make(index, to_frame(cluster.centre), dir_to_frame(cluster.velocity), cluster.size_m,
                dir_to_frame(cluster.angular_velocity), cluster.temperature_k.max(crate::armor_damage::AMBIENT_K + 1.0)));
        }
        if solve.chain.burst {
            const STEEL: f32 = 7850.0;
            let body_mass = (event.shell.mass_kg - event.shell.filler_g * 0.001).max(0.0);
            let count = (N_HOT_FRAGMENTS - out.len()).min(512);
            if body_mass > 0.0 && count > 0 {
                let exit = lat.plate_to_hull(Vec3::ZERO) + solve.line.dir * solve.line.plate_path_m;
                let burst = parent_inv.transform_point3(hull.transform_point3(exit));
                let axis = parent_inv.transform_vector3(hull.transform_vector3(solve.line.dir)).normalize_or_zero();
                let mut side = axis.cross(Vec3::Y).normalize_or_zero();
                if side.length_squared() < 0.5 {
                    side = axis.cross(Vec3::X).normalize_or_zero();
                }
                let up = side.cross(axis).normalize_or_zero();
                let shape: Vec<f32> = (0..count).map(|i| {
                    let s = penetration_seed ^ 0xB0D7_0000 ^ (i as u32).wrapping_mul(2_654_435_761);
                    profile.min_size_m + (profile.max_size_m - profile.min_size_m) * hash01(s ^ 0xE1).powf(2.4)
                }).collect();
                let raw_mass: f32 = shape.iter().map(|s| STEEL * s * s * s).sum();
                let k = (body_mass / raw_mass.max(1e-12)).cbrt();
                for (i, base) in shape.iter().enumerate() {
                    let s = penetration_seed ^ 0xB0D7_0000 ^ (i as u32).wrapping_mul(2_654_435_761);
                    let radial = hash01(s ^ 0x51).sqrt();
                    let angle = hash01(s ^ 0xA7) * std::f32::consts::TAU;
                    let fan = (side * angle.cos() + up * angle.sin()) * radial * profile.cone_half_tan;
                    let dir = (axis + fan).normalize_or_zero();
                    let speed = profile.speed_min + (profile.speed_max - profile.speed_min) * hash01(s ^ 0xD3).sqrt();
                    let spin = Vec3::new(hash01(s ^ 0x44) - 0.5, hash01(s ^ 0x55) - 0.5, hash01(s ^ 0x66) - 0.5) * 40.0;
                    let index = out.len();
                    // The burst body's temperature is not solved: authored heat pages (temp 0).
                    out.push(make(index, burst, dir * speed, base * k, spin, 0.0));
                }
            }
        }
        Some(out)
    }
}
