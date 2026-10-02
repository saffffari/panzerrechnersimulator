//! Panzerfaust's single live contact path. Factions do not filter collision or damage.
//! The held prop is 1:1 geometry; fuze distance starts at the physical nose, while a launch
//! clearance sweep catches bodies/walls intersecting the weapon before that nose.
use super::*;
use crate::ballistics::{self as ballistic, ArmorScheme, Terminal};
use crate::panzerfaust::{HeldPose, PROJECTILE_NOSE_Z, WARHEAD_DIAMETER_M};

#[derive(Clone, Copy, Debug)]
enum ContactKind {
    Armor(VehicleArmorContact),
    Soldier(usize),
    Crew(CouchCrewCasualty, crew_physics::CrewBodyPart),
    Structure(usize),
    HardCover,
    Tree(usize),
    Terrain,
}
#[derive(Clone, Copy, Debug)]
struct Contact {
    t: f32,
    point: Vec3,
    kind: ContactKind,
}
fn nearer(best: &mut Option<Contact>, candidate: Option<Contact>) {
    if let Some(hit) = candidate {
        if best.is_none_or(|old| hit.t < old.t) {
            *best = Some(hit);
        }
    }
}

// Existing tank armor data omits roofs/bellies. Those surfaces remain opaque contacts;
// they must not let a bomb pass right through a tank or inherit unrelated side-plate thickness.
fn vehicle_contact(
    tank: &Tank,
    vehicle: ProjectileVehicle,
    scheme: ArmorScheme,
    start: Vec3,
    end: Vec3,
) -> Option<Contact> {
    let mut best = None;
    for part in [ArmorPart::Hull, ArmorPart::Turret] {
        let Some((lo, hi)) = rig_armor_bounds(scheme, part) else {
            continue;
        };
        let model = if part == ArmorPart::Hull {
            tank.hull_model()
        } else {
            tank.turret_model()
        };
        let inv = model.inverse();
        let Some((fraction, normal)) = segment_local_aabb_contact(
            inv.transform_point3(start),
            inv.transform_point3(end),
            lo,
            hi,
        ) else {
            continue;
        };
        let point = start.lerp(end, fraction);
        let kind = if normal.y.abs() > 0.5 {
            ContactKind::HardCover
        } else {
            ContactKind::Armor(VehicleArmorContact {
                vehicle,
                part,
                facing: facing_from_local_normal(normal, part),
                local_point: inv.transform_point3(point),
                world_normal: model.transform_vector3(normal).normalize_or_zero(),
            })
        };
        nearer(
            &mut best,
            Some(Contact {
                t: start.distance(end) * fraction,
                point,
                kind,
            }),
        );
    }
    best
}

/// Double-sided actual source triangles, so a shot inside an empty cabin does not collide with
/// an enclosing solid AABB at distance zero. Both entering and exiting faces remain physical.
fn mesh_contact(mesh: &CpuMesh, model: Mat4, start: Vec3, end: Vec3) -> Option<(f32, Vec3)> {
    let inv = model.inverse();
    let o = inv.transform_point3(start);
    let delta = inv.transform_vector3(end - start);
    if segment_local_aabb_contact(o, o + delta, mesh.min, mesh.max).is_none() {
        return None;
    }
    let mut best = None;
    for tri in mesh.indices.chunks_exact(3) {
        let a = Vec3::from(mesh.vertices[tri[0] as usize].pos);
        let b = Vec3::from(mesh.vertices[tri[1] as usize].pos);
        let c = Vec3::from(mesh.vertices[tri[2] as usize].pos);
        let e1 = b - a;
        let e2 = c - a;
        let cross = delta.cross(e2);
        let det = e1.dot(cross);
        if det.abs() < 1e-8 {
            continue;
        }
        let inverse = det.recip();
        let from_a = o - a;
        let u = from_a.dot(cross) * inverse;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = from_a.cross(e1);
        let v = delta.dot(q) * inverse;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let fraction = e2.dot(q) * inverse;
        if (0.0..=1.0).contains(&fraction) && best.is_none_or(|(f, _)| fraction < f) {
            best = Some((
                fraction,
                model.transform_vector3(e1.cross(e2)).normalize_or_zero(),
            ));
        }
    }
    best
}

// Loaded only when the weapon first queries the Tiger, retained on CPU. No renderer/GPU readback
// and no source mutation. These are the same partitioned source assemblies the player sees.
struct TigerMeshes {
    body: [CpuMesh; 3],
    lids: [CpuMesh; 4],
    holder: CpuMesh,
    visor: super::tiger_driver_shutters::Source,
    periscopes: Vec<super::tiger_periscopes::OpticalPart>,
    gunner_sight: [CpuMesh;2],
    cupola_lid: Option<CpuMesh>,
    cupola_grip: CpuMesh,
}
fn tiger_meshes() -> Option<&'static TigerMeshes> {
    static MESHES: OnceLock<Option<TigerMeshes>> = OnceLock::new();
    MESHES
        .get_or_init(|| match tiger_hatches::load_player_tiger() {
            Ok((body, lids, holder, periscopes, sight, visor, cupola_grip, _driver_controls)) => Some(TigerMeshes {
                body,
                lids,
                holder,
                visor,
                periscopes,
                gunner_sight: sight.parts,
                cupola_grip: super::tiger_cupola::into_hatch_frame(cupola_grip),
                cupola_lid: crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hatch.glb")
                    .ok(),
            }),
            Err(error) => {
                eprintln!("[panzerfaust] Tiger collision source unavailable: {error:#}");
                None
            }
        })
        .as_ref()
}

impl Game {
    fn panzerfaust_tiger_contact(&self, start: Vec3, end: Vec3) -> Option<Contact> {
        let meshes = tiger_meshes()?;
        let hull = self.player.hull_model();
        let turret = self.player.turret_model();
        let lids = self.roof_hatches.models(hull, turret);
        let mut best = None;
        for (mesh, model, part) in [
            (&meshes.body[0], hull, Some(ArmorPart::Hull)),
            (&meshes.body[1], turret, Some(ArmorPart::Turret)),
            (&meshes.body[2], self.player.visual_gun_model(), None),
            (&meshes.lids[0], lids[0], None),
            (&meshes.lids[1], lids[1], None),
            (&meshes.lids[2], lids[2], None),
            (&meshes.lids[3], lids[3], None),
            (&meshes.cupola_grip, super::cupola_hatch_model(turret, self.hatch_t), None),
            (&meshes.holder, hull, None),
            (&meshes.visor.guides,hull,None),
        ] {
            if let Some((fraction, normal)) = mesh_contact(mesh, model, start, end) {
                let point = start.lerp(end, fraction);
                let local_normal = model.inverse().transform_vector3(normal);
                // The existing armor scheme has no roof/belly/material-per-triangle table. Such
                // geometry still stops the bomb; it cannot masquerade as penetrable side armor.
                let kind = if let Some(part) = part.filter(|_| local_normal.y.abs() < 0.5) {
                    ContactKind::Armor(VehicleArmorContact {
                        vehicle: ProjectileVehicle::Player,
                        part,
                        facing: facing_from_local_normal(local_normal, part),
                        local_point: model.inverse().transform_point3(point),
                        world_normal: normal,
                    })
                } else {
                    ContactKind::HardCover
                };
                nearer(
                    &mut best,
                    Some(Contact {
                        t: start.distance(end) * fraction,
                        point,
                        kind,
                    }),
                );
            }
        }
        // These housings, mirrors and glass panes were partitioned out of the turret for
        // optical shading. Transparency to light does not remove physical projectile contact.
        // Until their separate fracture/material response exists, retain a hard contact rather
        // than assigning turret-armor thickness to glass or allowing the bomb through the block.
        for (mesh, model) in meshes.periscopes.iter().map(|part|(&part.mesh,turret))
            .chain(meshes.gunner_sight.iter().map(|mesh|(mesh,self.player.gun_model())))
            .chain(meshes.visor.moving.iter().zip(self.driver_shutters.models(hull))) {
            if let Some((fraction, _)) = mesh_contact(mesh, model, start, end) {
                nearer(
                    &mut best,
                    Some(Contact {
                        t: start.distance(end) * fraction,
                        point: start.lerp(end, fraction),
                        kind: ContactKind::HardCover,
                    }),
                );
            }
        }
        if let Some(mesh) = &meshes.cupola_lid {
            let lift = smoothstep(0.0, 0.5, self.hatch_t) * 0.15;
            let swing = smoothstep(0.5, 1.0, self.hatch_t) * (-110_f32).to_radians();
            let model = turret
                * Mat4::from_translation(Vec3::new(0.963, 2.614 + lift, -0.333))
                * Mat4::from_rotation_y(swing);
            if let Some((fraction, _)) = mesh_contact(mesh, model, start, end) {
                nearer(
                    &mut best,
                    Some(Contact {
                        t: start.distance(end) * fraction,
                        point: start.lerp(end, fraction),
                        kind: ContactKind::HardCover,
                    }),
                );
            }
        }
        best
    }

    fn panzerfaust_crew(&self) -> Vec<CouchCrewBody> {
        let mut bodies = couch_crew_bodies(
            self.couch_crew_frame,
            &self.couch_kia_peers,
            &self.couch_crew_identities,
        );
        bodies.retain(|body| {
            self.couch_local_peer != Some(body.casualty.peer_id)
                && !COUCH_CREW_ROLES.into_iter().any(|role| {
                    self.couch_crew_frame.roster[role as usize] == body.casualty.peer_id
                        && self
                            .couch_crew_frame
                            .input(role)
                            .flag(crate::lan::F_DISMOUNTED)
                })
        });
        for body in &mut bodies {
            let (scale, segments) =
                crew_pose_segments_in_frame(&self.player, body.casualty.role, BloodFrame::Hull);
            let hull = self.player.hull_model();
            body.hit_volumes = crew_hit_volumes_from_segments(&segments, scale).map(|volume| {
                let (_, rotation, _) = hull.to_scale_rotation_translation();
                crew_damage::CrewHitVolume::new(
                    volume.part,
                    hull.transform_point3(volume.center),
                    rotation * volume.rotation,
                    volume.half_extents,
                )
            });
        }
        bodies
    }

    /// Single nearest-hit competition across all actors and live world cover. No team immunity,
    /// no metre-sized near cull, and no preselected enemy captured at trigger time.
    fn panzerfaust_contact(&self, start: Vec3, end: Vec3, actors: bool) -> Option<Contact> {
        let delta = end - start;
        let length = delta.length();
        if length < 1e-6 {
            return None;
        }
        let dir = delta / length;
        let mut best = self.panzerfaust_tiger_contact(start, end);
        for (i, enemy) in self.enemies.iter().enumerate() {
            if !self.enemy_active.get(i).copied().unwrap_or(false) {
                continue;
            }
            nearer(
                &mut best,
                vehicle_contact(
                    enemy,
                    ProjectileVehicle::Enemy(i),
                    enemy_armor(enemy.kind),
                    start,
                    end,
                ),
            );
        }
        for (i, ally) in self.allies.iter().enumerate() {
            nearer(
                &mut best,
                vehicle_contact(
                    &ally.tank,
                    ProjectileVehicle::Ally(i),
                    ArmorScheme::TigerI,
                    start,
                    end,
                ),
            );
        }
        if actors {
            for (i, soldier) in self.soldiers.iter().enumerate() {
                if !soldier.alive {
                    continue;
                }
                let height = if i < self.soviet_soldier_count {
                    1.75
                } else {
                    crate::infantry_combat::GERMAN_BODY_HEIGHTS[soldier.crouch_stage.min(4)]
                };
                let radius = WARHEAD_DIAMETER_M * 0.5;
                let center = soldier.pos + Vec3::Y * height * 0.5;
                if let Some(fraction) = crew_damage::swept_sphere_oriented_box_fraction(
                    start,
                    end,
                    radius,
                    center,
                    Quat::IDENTITY,
                    Vec3::new(0.25, height * 0.5, 0.25),
                ) {
                    nearer(
                        &mut best,
                        Some(Contact {
                            t: length * fraction,
                            point: start.lerp(end, fraction),
                            kind: ContactKind::Soldier(i),
                        }),
                    );
                }
            }
            for body in self.panzerfaust_crew() {
                for volume in body.hit_volumes {
                    if let Some(fraction) = crew_damage::swept_sphere_obb_fraction(
                        start,
                        end,
                        WARHEAD_DIAMETER_M * 0.5,
                        volume,
                    ) {
                        nearer(
                            &mut best,
                            Some(Contact {
                                t: length * fraction,
                                point: start.lerp(end, fraction),
                                kind: ContactKind::Crew(body.casualty, volume.part),
                            }),
                        );
                    }
                }
            }
        }
        for (i, structure) in self.destructibles.iter().enumerate() {
            if structure.alive {
                if let Some(t) =
                    ray_aabb_near(start, dir, structure.min, structure.max, length, 0.0)
                {
                    nearer(
                        &mut best,
                        Some(Contact {
                            t,
                            point: start + dir * t,
                            kind: ContactKind::Structure(i),
                        }),
                    );
                }
            }
        }
        for &(min, max) in &self.story_cover {
            if let Some(t) = ray_aabb_near(start, dir, min, max, length, 0.0) {
                nearer(
                    &mut best,
                    Some(Contact {
                        t,
                        point: start + dir * t,
                        kind: ContactKind::HardCover,
                    }),
                );
            }
        }
        if let Some((t, i)) = self.trees.ray(start, dir, length) {
            nearer(
                &mut best,
                Some(Contact {
                    t,
                    point: start + dir * t,
                    kind: ContactKind::Tree(i),
                }),
            );
        }
        if let Some(hit) = swept_terrain_contact(start, end, &self.hf) {
            nearer(
                &mut best,
                Some(Contact {
                    t: hit.t_m,
                    point: hit.pos,
                    kind: ContactKind::Terrain,
                }),
            );
        }
        best
    }

    pub(super) fn fire_panzerfaust(&mut self, hold: HeldPose, renderer: &mut Renderer) {
        let id = self.next_projectile_id;
        self.next_projectile_id = id.wrapping_add(1).max(1);
        self.foot.cooldown = 0.65;
        self.foot.panzerfaust_ready = false;
        self.foot.panzerfaust_flash_t = 0.20;
        self.weapon_vfx.muzzle(hold.nose(), hold.direction, weapon_vfx::Weapon::Rocket);
        self.weapon_vfx.muzzle(hold.rear(), -hold.direction, weapon_vfx::Weapon::Rocket);
        self.foot.recoil = 1.45;
        self.panzerfaust_shot = PanzerfaustShot {
            active: true,
            id,
            origin: hold.origin,
            age: 0.0,
            pos: hold.nose(),
            vel: hold.direction * crate::foot::PANZERFAUST_V0,
            distance: 0.0,
        };
        // A long viewmodel can already intersect a nearby actor or wall. Sweep from the held
        // tube reference to its nose now; never teleport a spawned bomb through that obstruction.
        let launch_contact = self.panzerfaust_contact(hold.origin, hold.nose(), true);
        let rear = hold.rear();
        let backblast: Vec<_> = self
            .soldiers
            .iter()
            .enumerate()
            .filter_map(|(i, soldier)| {
                if !soldier.alive {
                    return None;
                }
                let target = soldier.pos + Vec3::Y;
                let to = target - rear;
                let distance = to.length();
                (distance < 10.0
                    && to.normalize_or_zero().dot(-hold.direction) > 0.60
                    && self.panzerfaust_contact(rear, target, false).is_none())
                .then_some((i, distance))
            })
            .collect();
        for (i, distance) in backblast {
            if distance < 3.0 {
                self.kill_soldier(i, "Panzerfaust backblast");
            } else {
                self.soldiers[i].flinch_t = self.soldiers[i].flinch_t.max(1.0);
            }
        }
        // The launch at the firer's ear. No measurement of a recoilless launcher was found; the
        // report's PTRD-41 firer's-ear figures (170 dB, LAE 134 dB(A)) stand in [E].
        let ear = self.own_vehicle_ear_level(crate::sound_levels::PTRD41_1M.lpk_db - 6.0,
            crate::sound_levels::PTRD41_1M.laf_db - 6.0,
            crate::sound_levels::SpectrumClass::SmallArms, crate::sound_levels::PTRD41_1M.a_duration_ms);
        self.play_and_hear_at_ear(crate::audio::Sfx::EnemyGun, &ear, 0.0);
        self.push_noise(hold.nose(), 180.0, 3.5);
        self.propagate_alarm(hold.nose());
        if let Some(runtime) = self.campaign.as_mut() {
            runtime.battle_pressure = (runtime.battle_pressure + 8.0).min(100.0);
        }
        println!(
            "[absitzen] PANZERFAUST 60 #{id} fired — {} fresh tubes carried",
            self.foot.panzerfaust_spares
        );
        if let Some(contact) = launch_contact {
            self.panzerfaust_impact(contact, false, renderer);
        }
    }

    pub(super) fn update_panzerfaust_shot(&mut self, dt: f32, renderer: &mut Renderer) {
        if !self.panzerfaust_shot.active {
            renderer.set_visible(self.slots.panzerfaust_projectile, false);
            return;
        }
        let old = self.panzerfaust_shot.pos;
        let velocity = self.panzerfaust_shot.vel;
        let dt = dt.max(0.0);
        let delta = velocity * dt - Vec3::Y * (4.905 * dt * dt);
        let (length, reached_range) =
            clamp_panzerfaust_segment(self.panzerfaust_shot.distance, delta.length());
        let next = old + delta.normalize_or_zero() * length;
        let contact = self.panzerfaust_contact(old, next, true);
        let travelled = contact.map_or(length, |hit| hit.t);
        let fraction = if delta.length() > 1e-6 {
            travelled / delta.length()
        } else {
            0.0
        };
        self.panzerfaust_shot.distance += travelled;
        self.panzerfaust_shot.age += dt * fraction;
        self.panzerfaust_shot.vel = velocity - Vec3::Y * (9.81 * dt * fraction);
        self.panzerfaust_shot.pos = contact.map_or(next, |hit| hit.point);
        if let Some(contact) = contact {
            self.panzerfaust_impact(
                contact,
                self.panzerfaust_shot.distance >= PANZERFAUST_ARMING_DISTANCE,
                renderer,
            );
        } else if reached_range {
            // The old arbitrary air puff looked like a detonation. Range is only retirement.
            self.panzerfaust_shot.active = false;
        }
        renderer.set_visible(
            self.slots.panzerfaust_projectile,
            self.panzerfaust_shot.active,
        );
        if self.panzerfaust_shot.active {
            let direction = self.panzerfaust_shot.vel.normalize_or_zero();
            renderer.set_model(
                self.slots.panzerfaust_projectile,
                Mat4::from_rotation_translation(
                    Quat::from_rotation_arc(Vec3::Z, direction),
                    self.panzerfaust_shot.pos - direction * PROJECTILE_NOSE_Z,
                ),
            );
        }
    }

    fn panzerfaust_impact(&mut self, contact: Contact, armed: bool, renderer: &mut Renderer) {
        self.panzerfaust_shot.active = false;
        renderer.set_visible(self.slots.panzerfaust_projectile, false);
        let shot = self.panzerfaust_shot.clone();
        let dir = shot.vel.normalize_or_zero();
        let energy = 0.5 * ballistic::PANZERFAUST_60.mass_kg * shot.vel.length_squared();
        println!(
            "[panzerfaust] #{} {:?} at {:.2} m, armed={armed}, {:.0} J",
            shot.id, contact.kind, shot.distance, energy
        );
        match contact.kind {
            ContactKind::Soldier(i) => {
                // Same 100 hp body policy across both factions, now scaled by the actual moving
                // bomb's energy. The fuze controls explosion; an unarmed heavy bomb still collides.
                let hp = self
                    .mixed_combat
                    .actors
                    .get(i)
                    .map_or(100.0, |actor| actor.hp);
                let damage = (energy / 10.0).max(0.0);
                if damage >= hp {
                    self.kill_soldier(i, "Panzerfaust direct impact");
                } else if let Some(actor) = self.mixed_combat.actors.get_mut(i) {
                    actor.hp -= damage;
                }
                self.soldiers[i].flinch_t = 1.0;
            }
            ContactKind::Crew(casualty, part) => {
                self.panzerfaust_damage_crew(casualty, part, contact.point, shot.vel, energy)
            }
            ContactKind::Armor(armor) => {
                let scheme = match armor.vehicle {
                    ProjectileVehicle::Enemy(i) => enemy_armor(self.enemies[i].kind),
                    _ => ArmorScheme::TigerI,
                };
                let mut plate = scheme.plate(armor.facing);
                // Enemy OBB normals represent unsloped rig faces. Their data-table construction
                // slope must still contribute. Own Tiger triangles already carry their real slope.
                let cosine = dir.dot(armor.world_normal).abs().clamp(0.0, 1.0);
                let cosine = if armor.vehicle == ProjectileVehicle::Player {
                    cosine
                } else {
                    cosine * plate.slope_deg.to_radians().cos()
                };
                if armor.vehicle == ProjectileVehicle::Player {
                    plate = self
                        .armor_scars
                        .plate_at(plate, armor.facing, armor.local_point);
                }
                let event = ballistic::resolve_shaped_charge(
                    &ballistic::PANZERFAUST_60,
                    shot.distance,
                    plate,
                    cosine.acos().to_degrees(),
                    armed,
                );
                let mut projectile = AuthoritativeProjectile::new(
                    shot.id,
                    ProjectileShooter::Player,
                    &ballistic::PANZERFAUST_60,
                    shot.origin,
                    dir,
                    shot.id as u32,
                );
                projectile.distance_m = shot.distance;
                projectile.age_s = shot.age;
                projectile.velocity = shot.vel;
                let outcome = if event.terminal == Terminal::Penetration {
                    Outcome::Penetrate
                } else {
                    Outcome::Bounce
                };
                self.resolve_authoritative_vehicle_impact(
                    &projectile,
                    armor,
                    event,
                    outcome,
                    contact.point,
                    renderer,
                );
            }
            ContactKind::Structure(i) if armed => self.damage_structure(i, 95.0, renderer),
            ContactKind::Tree(i) if armed => {
                let mut events = Vec::new();
                self.trees.shot(i, dir, &mut events);
                self.route_tree_events(events);
            }
            _ => {}
        }
        if armed {
            self.spawn_blast(contact.point, 1.8, 8.0, 0.18);
            // Shared exterior blast damage has real cover and faction-neutral actor targets. Move
            // just back off the struck face to avoid burying the whole effect inside solid armor.
            self.mixed_he_blast(
                shot.id,
                contact.point - dir * 0.08,
                ballistic::PANZERFAUST_60.filler_g / 1000.0,
                ProjectileShooter::Player,
            );
            self.schedule_sound_from(crate::audio::Sfx::ImpactGround, contact.point);
        } else {
            self.spawn_dust(contact.point, 0.18);
            self.schedule_sound_from(crate::audio::Sfx::ArmorClang1, contact.point);
        }
    }

    fn panzerfaust_damage_crew(
        &mut self,
        casualty: CouchCrewCasualty,
        part: crew_physics::CrewBodyPart,
        point: Vec3,
        velocity: Vec3,
        energy: f32,
    ) {
        if self.god || energy < crew_damage::MIN_INJURY_WORK_J {
            return;
        }
        let previous = self.couch_casualties.len();
        let wiped = record_couch_casualties(
            self.couch_crew_frame,
            &self.couch_crew_identities,
            &mut self.couch_kia_peers,
            &mut self.couch_casualties,
            &mut self.couch_last_casualty,
            &[casualty],
        );
        if self.couch_casualties.len() == previous {
            return;
        }
        let dir = velocity.normalize_or_zero();
        self.crew_ko_t = CREW_KO_DUR;
        self.crew_ko_world_dir = dir;
        let seed = self.panzerfaust_shot.id as u32 ^ casualty.peer_id;
        let damage = crew_damage::resolve_crew_damage(crew_damage::CrewDamageInput {
            part,
            fragment_energy_j: energy,
            explosive_energy_j: 0.0,
            blast_distance_m: 0.0,
        });
        self.spawn_couch_ragdoll_impact(
            casualty,
            CrewRagdollImpact {
                hit_part: part,
                point_world: point,
                velocity_world: velocity,
                energy_j: energy,
                blast_origin_world: point,
                explosive_energy_j: 0.0,
                behind_armor: 1.0,
                blast_proximity_m: 0.0,
            },
            seed,
        );
        if let Some(body) = self
            .couch_ragdolls
            .iter_mut()
            .find(|body| body.casualty == casualty)
        {
            if damage.detach_joint {
                body.segments[part.index()].detached = true;
            }
            let mut removed = [false; crew_physics::CREW_BODY_PART_COUNT];
            removed[part.index()] = damage.hide_mesh_part;
            apply_crew_pulverization(body, &removed);
        }
        self.spawn_cubic_blood(
            casualty,
            Some(point),
            dir,
            Vec3::ZERO,
            1.0,
            1.0,
            seed,
            previous,
            damage.blood_cube_count,
        );
        if wiped {
            self.player.alive = false;
            self.death_cause = Some(DeathCause::CrewLost);
            self.recompute_hp();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_inside_cabin_reaches_surface_instead_of_hitting_empty_bounds() {
        let mesh = box_mesh(Vec3::splat(-1.0), Vec3::splat(1.0), [100; 3]);
        let model =
            Mat4::from_rotation_translation(Quat::from_rotation_y(0.7), Vec3::new(5.0, 2.0, -4.0));
        let start = model.transform_point3(Vec3::ZERO);
        let end = model.transform_point3(Vec3::Z * 2.0);
        let (fraction, _) = mesh_contact(&mesh, model, start, end).unwrap();
        assert!((fraction - 0.5).abs() < 1e-5);
        assert!(mesh_contact(&mesh, model, start, model.transform_point3(Vec3::Z * 0.5)).is_none());
    }
    #[test]
    fn nearer_wall_blocks_body_and_nearer_body_blocks_tank() {
        let mut best = None;
        let target = |t, kind| {
            Some(Contact {
                t,
                point: Vec3::Z * t,
                kind,
            })
        };
        nearer(&mut best, target(10.0, ContactKind::HardCover));
        nearer(&mut best, target(0.2, ContactKind::Soldier(26)));
        assert!(matches!(best.unwrap().kind, ContactKind::Soldier(26)));
        nearer(&mut best, target(0.1, ContactKind::HardCover));
        assert!(matches!(best.unwrap().kind, ContactKind::HardCover));
    }
    #[test]
    fn moving_bomb_hits_near_crew_before_fuze_arms() {
        let volume = crew_damage::CrewHitVolume::new(
            crew_physics::CrewBodyPart::Torso,
            Vec3::new(0.0, 1.2, 0.55),
            Quat::IDENTITY,
            Vec3::new(0.20, 0.30, 0.12),
        );
        let start = Vec3::new(0.0, 1.2, 0.0);
        let end = start + Vec3::Z;
        let fraction =
            crew_damage::swept_sphere_obb_fraction(start, end, WARHEAD_DIAMETER_M * 0.5, volume)
                .unwrap();
        assert!(fraction < 0.4 && fraction < PANZERFAUST_ARMING_DISTANCE);
        let work = 0.5 * ballistic::PANZERFAUST_60.mass_kg * crate::foot::PANZERFAUST_V0.powi(2);
        let result = crew_damage::resolve_crew_damage(crew_damage::CrewDamageInput {
            part: volume.part,
            fragment_energy_j: work,
            explosive_energy_j: 0.0,
            blast_distance_m: 0.0,
        });
        assert!(result.total_work_j > crew_damage::MIN_INJURY_WORK_J);
        assert_eq!(result.blast_work_j, 0.0);
    }
    #[test]
    fn heat_penetration_does_not_place_external_filler_inside_cabin() {
        let event = ballistic::resolve_shaped_charge(
            &ballistic::PANZERFAUST_60,
            15.0,
            ArmorScheme::TigerI.plate(ballistic::Facing::HullSide),
            0.0,
            true,
        );
        let frame = BloodFrame::Hull;
        let fragments = generate_penetration_fragments(
            12,
            frame,
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::NEG_X,
            event,
            44,
        );
        let profile = fragment_shower_profile(
            true,
            event.behind_armor,
            event.shell.caliber_mm,
            event.residual_v,
            0.0,
        );
        assert_eq!(fragments.len(), profile.fragment_count);
        assert!(!matches!(
            profile.kind,
            FragmentShowerKind::Explosive | FragmentShowerKind::Catastrophic
        ));
    }
}
