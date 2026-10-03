//! Bounded enemy-crew aftermath. Approximate enemy seats/roof exits are explicit gameplay
//! geometry; injuries use the same tissue policy and actual resolved penetration shower as Tiger.
use super::{crew_damage as damage, crew_physics as physics, *};
use glam::Quat;
pub(super) const CAPACITY: usize = 128;
const COUNT: usize = physics::CREW_BODY_PART_COUNT;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Phase {
    Seated,
    Waiting,
    Climbing,
    Falling,
    Escaping,
    Dead,
}
#[derive(Clone)]
pub(super) struct Person {
    tank: usize,
    identity: usize,
    phase: Phase,
    age: f32,
    segments: [physics::CrewSegment; COUNT],
    missing: [bool; COUNT],
    wounded: bool,
    bleeding: bool,
    burn_s: f32,
    pos: Vec3,
    velocity: Vec3,
    exit: Vec3,
    heading: f32,
    interior_floor: f32,
    blood_pos: Vec3,
    hull: Mat4,
}
#[derive(Default)]
pub(super) struct Aftermath {
    people: Vec<Person>,
    pub(super) parts: [usize; COUNT],
    pub(super) stains: usize,
    pub(super) flames: usize,
}
#[derive(Clone, Copy, Default, Debug)]
pub(super) struct DamageOutcome {
    pub disabled: bool,
    pub ammo: bool,
    pub fuel: bool,
    pub engine: bool,
}
fn module_contacts(fragments: &[HotFragment]) -> DamageOutcome {
    // Explicit coarse enemy compartment boxes, hull-local metres. A module only
    // reacts to a solved fragment contact with deposited kinetic work, never a roll.
    let mut result = DamageOutcome::default();
    for f in fragments {
        let energy = 0.5 * 7800.0 * f.size.powi(3) * f.vel.length_squared();
        if energy < 20.0 {
            continue;
        }
        for (i, center, half) in [
            (0, Vec3::new(0.0, 0.9, 2.0), Vec3::new(0.85, 0.6, 0.7)),
            (1, Vec3::new(1.0, 0.7, 0.3), Vec3::new(0.25, 0.45, 1.2)),
            (2, Vec3::new(-1.1, 0.65, 1.8), Vec3::new(0.22, 0.4, 0.75)),
        ] {
            let volume = damage::CrewHitVolume::new(
                physics::CrewBodyPart::Torso,
                center,
                Quat::IDENTITY,
                half,
            );
            if damage::swept_sphere_obb_fraction(
                f.pos,
                f.pos + f.vel.normalize_or_zero() * 3.0,
                f.size * 0.52,
                volume,
            )
            .is_some()
            {
                match i {
                    0 => result.engine = true,
                    1 => result.ammo = true,
                    _ => result.fuel = true,
                }
            }
        }
    }
    result
}
fn seats(kind: EnemyKind) -> Vec<Vec3> {
    // Enemy rigs have no authored seated interiors. These stations are broad compartment
    // approximations, not measured historic seat coordinates. All are in hull-local metres.
    let mut seats = vec![
        Vec3::new(-0.65, 0.45, -1.4),
        Vec3::new(0.65, 0.45, -1.4),
        Vec3::new(-0.60, 1.15, 0.0),
        Vec3::new(0.60, 1.15, 0.30),
    ];
    if matches!(kind, EnemyKind::Kv2 | EnemyKind::Isu152) {
        seats.push(Vec3::new(-0.55, 1.0, 0.8));
    }
    if kind.casemate() {
        for p in &mut seats[2..] {
            p.y = 0.75;
        }
    }
    seats
}
fn apply_damage(
    segments: &mut [physics::CrewSegment; COUNT],
    entry: Vec3,
    fragments: &[HotFragment],
    filler_j: f32,
) -> (bool, [bool; COUNT], bool) {
    let mut work = [0.0; COUNT];
    // One fragment transfers energy only to its first contacted anatomical volume. A 3 m
    // swept travel is bounded by the enemy compartment approximation, never an infinite ray.
    for fragment in fragments {
        let dir = fragment.vel.normalize_or_zero();
        let mut first: Option<(f32, usize)> = None;
        for (i, p) in segments.iter().enumerate() {
            let volume = damage::CrewHitVolume::new(p.part, p.pos, p.rot, p.part.half_extents());
            if let Some(t) = damage::swept_sphere_obb_fraction(
                fragment.pos,
                fragment.pos + dir * 3.0,
                fragment.size * 0.52,
                volume,
            ) {
                if first.is_none_or(|old| t < old.0) {
                    first = Some((t, i));
                }
            }
        }
        if let Some((_, i)) = first {
            let mass = 7800.0 * fragment.size.powi(3);
            let energy = 0.5 * mass * fragment.vel.length_squared();
            work[i] += energy;
            let hit_part = segments[i].part;
            physics::apply_fragment_impulse_to_part(
                segments,
                hit_part,
                fragment.pos + dir * 3.0 * first.unwrap().0,
                fragment.vel,
                energy,
                1.0,
            );
        }
    }
    let mut dead = false;
    let mut missing = [false; COUNT];
    let mut wounded = false;
    for (i, p) in segments.iter_mut().enumerate() {
        let result = damage::resolve_crew_damage(damage::CrewDamageInput {
            part: p.part,
            fragment_energy_j: work[i],
            explosive_energy_j: filler_j,
            blast_distance_m: p.pos.distance(entry),
        });
        wounded |= result.outcome != damage::CrewTissueOutcome::None;
        // Central/head destruction is lethal; a struck distal limb can remain survivable.
        if matches!(
            p.part,
            physics::CrewBodyPart::Head
                | physics::CrewBodyPart::Torso
                | physics::CrewBodyPart::Pelvis
        ) && (result.outcome >= damage::CrewTissueOutcome::Detachment
            || result.total_work_j >= 12.0)
        {
            dead = true;
        }
        p.detached |= result.detach_joint;
        p.present &= !result.hide_mesh_part;
        missing[i] = p.detached;
    }
    let impulses = physics::confined_segment_blast_impulses(segments, entry, filler_j, 1.0);
    for (part, impulse) in segments.iter_mut().zip(impulses) {
        if part.present {
            part.vel += impulse / part.part.mass_kg();
        }
    }
    for (parent, child) in [(3, 4), (5, 6), (7, 8), (9, 10)] {
        if missing[parent] {
            missing[child] = true;
            segments[child].detached = true;
        }
    }
    (dead, missing, wounded)
}
impl Aftermath {
    pub(super) fn allocate(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer) -> Self {
        // Use the actual Soviet character and atlas, normalized only into shared anatomical metres.
        let mut source = crate::gltf_load::load_glb(SOVIET_POSE_PATHS[0])
            .unwrap_or_else(|_| soviet_infantry_mesh([78, 82, 70]));
        let s = 1.53 / (source.max.y - source.min.y).max(0.1);
        let floor = source.min.y;
        for v in &mut source.vertices {
            let mut p = Vec3::from(v.pos);
            p.y -= floor;
            p *= s;
            v.pos = p.to_array();
        }
        source.min = Vec3::new(source.min.x * s, 0.0, source.min.z * s);
        source.max = Vec3::new(source.max.x * s, 1.53, source.max.z * s);
        let pages = partition_relaxed_crew_mesh(&source);
        let parts = std::array::from_fn(|i| {
            let mesh = pages[i].clone().unwrap_or_else(|| {
                crew_segment_fallback_mesh(physics::CrewBodyPart::ALL[i], [78, 82, 70])
            });
            let prototype = r.register_instanced_prototype(device, queue, &mesh);
            r.add_moving_instanced_pool(device, prototype, CAPACITY)
                .expect("enemy crew part pool")
        });
        let stain_proto = r.register_instanced_prototype(device, queue, &crew_gore_mesh());
        let stains = r
            .add_moving_instanced_pool(device, stain_proto, CAPACITY)
            .expect("enemy blood pool");
        let flames = r
            .add_dynamic_instanced_pool(
                device,
                queue,
                &blood_grain_mesh([255, 153, 24]),
                CAPACITY * 8,
            )
            .expect("burned crew flame pool");
        r.set_batch_flags(flames, 2.0);
        assert!(r.set_dynamic_instanced_emissive(flames, 8.0));
        Self {
            people: Vec::new(),
            parts,
            stains,
            flames,
        }
    }
    pub(super) fn resolve(
        &mut self,
        tank_index: usize,
        tank: &Tank,
        projectile: &AuthoritativeProjectile,
        event: crate::ballistics::ImpactEvent,
        entry_world: Vec3,
        catastrophic: bool,
    ) -> DamageOutcome {
        if event.terminal != crate::ballistics::Terminal::Penetration {
            return DamageOutcome::default();
        }
        let model = tank.hull_model();
        let inverse = model.inverse();
        let entry = inverse.transform_point3(entry_world);
        let axis = inverse
            .transform_vector3(projectile.velocity)
            .normalize_or_zero();
        let fragments = generate_penetration_fragments(
            projectile.id,
            BloodFrame::Hull,
            entry,
            axis,
            event,
            projectile.seed,
        );
        let stations = seats(tank.kind);
        // Consume each fragment at the first person along its solved sweep. Do not assign
        // its full kinetic energy repeatedly to every crew member along the ray.
        let mut by_person: Vec<Vec<HotFragment>> = stations.iter().map(|_| Vec::new()).collect();
        let module_damage = module_contacts(&fragments);
        for fragment in fragments {
            let mut nearest: Option<(f32, usize)> = None;
            for (identity, seat) in stations.iter().enumerate() {
                for p in physics::seated_segments(*seat, Quat::IDENTITY, 1.0) {
                    let volume =
                        damage::CrewHitVolume::new(p.part, p.pos, p.rot, p.part.half_extents());
                    if let Some(t) = damage::swept_sphere_obb_fraction(
                        fragment.pos,
                        fragment.pos + fragment.vel.normalize_or_zero() * 3.0,
                        fragment.size * 0.52,
                        volume,
                    ) {
                        if nearest.is_none_or(|old| t < old.0) {
                            nearest = Some((t, identity));
                        }
                    }
                }
            }
            if let Some((_, identity)) = nearest {
                by_person[identity].push(fragment);
            }
        }
        for (identity, seat) in stations.into_iter().enumerate() {
            if self.people.len() >= CAPACITY {
                break;
            }
            let previous = self
                .people
                .iter()
                .position(|p| p.tank == tank_index && p.identity == identity);
            let mut segments = previous
                .map(|i| {
                    let mut parts = self.people[i].segments;
                    for p in &mut parts {
                        p.pos = inverse.transform_point3(p.pos);
                        p.rot = Quat::from_mat4(&inverse) * p.rot;
                    }
                    parts
                })
                .unwrap_or_else(|| physics::seated_segments(seat, Quat::IDENTITY, 1.0));
            let (mut dead, missing, wounded) = apply_damage(
                &mut segments,
                entry,
                &by_person[identity],
                if event.shell.kind == crate::ballistics::ShellKind::Heat {
                    0.0
                } else {
                    event.shell.filler_g * 4184.0
                },
            );
            // A catastrophic ammunition event admits no invented survivors. Retained tissue
            // damage still follows this penetration; subsequent cookoff also blocks remaining crew.
            dead |= catastrophic || previous.is_some_and(|i| self.people[i].phase == Phase::Dead);
            for p in &mut segments {
                p.pos = model.transform_point3(p.pos);
                p.rot = Quat::from_mat4(&model) * p.rot;
                p.vel = model.transform_vector3(p.vel);
                p.angular_vel = model.transform_vector3(p.angular_vel);
            }
            let side = if identity % 2 == 0 { -1.0 } else { 1.0 };
            let exit =
                model.transform_point3(Vec3::new(side * 0.65, 2.65, seat.z.clamp(-0.8, 0.8)));
            let pos = model.transform_point3(seat);
            let floor = model.transform_point3(Vec3::new(seat.x, 0.9, seat.z)).y;
            let mut person = Person {
                tank: tank_index,
                identity,
                phase: if dead { Phase::Dead } else { Phase::Seated },
                age: 0.0,
                segments,
                missing,
                wounded,
                bleeding: !by_person[identity].is_empty() || missing.iter().any(|p| *p),
                burn_s: 0.0,
                pos,
                velocity: Vec3::ZERO,
                exit,
                heading: tank.heading + side * std::f32::consts::FRAC_PI_2,
                interior_floor: floor,
                blood_pos: pos,
                hull: model,
            };
            if let Some(i) = previous {
                person.wounded |= self.people[i].wounded;
                person.bleeding |= self.people[i].bleeding;
                for j in 0..COUNT {
                    person.missing[j] |= self.people[i].missing[j];
                }
                self.people[i] = person;
            } else {
                self.people.push(person);
            }
        }
        if module_damage.ammo {
            for p in self.people.iter_mut().filter(|p| p.tank == tank_index) {
                p.phase = Phase::Dead;
            }
        }
        let crew_lost = self
            .people
            .iter()
            .filter(|p| p.tank == tank_index)
            .all(|p| p.phase == Phase::Dead);
        DamageOutcome {
            disabled: crew_lost || module_damage.engine || module_damage.ammo || module_damage.fuel,
            ammo: module_damage.ammo,
            fuel: module_damage.fuel,
            engine: module_damage.engine,
        }
    }
    pub(super) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({"capacity":CAPACITY,"geometry":"approximate enemy seats and roof exits",
            "people":self.people.iter().map(|p|serde_json::json!({"tank":p.tank,"identity":p.identity,
                "phase":format!("{:?}",p.phase),"wounded":p.wounded,"missing":p.missing,
                "burn_exposure_s":p.burn_s,"position":p.pos.to_array()})).collect::<Vec<_>>()})
    }
    pub(super) fn clear_tank(&mut self, index: usize) {
        self.people.retain(|p| p.tank != index);
    }
    pub(super) fn update(
        &mut self,
        dt: f32,
        enemies: &[Tank],
        wrecks: &[Wreck],
        hf: &Heightfield,
        occluders: &[stealth::Occluder],
    ) {
        let dt = dt.clamp(0.0, 0.05);
        for person in &mut self.people {
            person.age += dt;
            let Some(tank) = enemies.get(person.tank) else {
                continue;
            };
            let Some(wreck) = wrecks.get(person.tank) else {
                continue;
            };
            if tank.alive || wreck.kind == WreckKind::None {
                let hull = tank.hull_model();
                let change = hull * person.hull.inverse();
                for part in &mut person.segments {
                    part.pos = change.transform_point3(part.pos);
                    part.rot = Quat::from_mat4(&change) * part.rot;
                }
                person.pos = change.transform_point3(person.pos);
                person.blood_pos = change.transform_point3(person.blood_pos);
                person.hull = hull;
                continue;
            }
            if wreck.cooked && matches!(person.phase, Phase::Waiting | Phase::Climbing) {
                person.phase = Phase::Dead;
            }
            let escape = Vec3::new(person.heading.sin(), 0.0, -person.heading.cos());
            if person.phase == Phase::Seated {
                person.phase = Phase::Waiting;
                person.age = 0.0;
            }
            match person.phase {
                Phase::Seated => {}
                Phase::Waiting => {
                    if person.age > 1.0 + person.identity as f32 * 0.7 {
                        person.phase = Phase::Climbing;
                    }
                }
                Phase::Climbing => {
                    let delta = person.exit - person.pos;
                    person.pos += delta.normalize_or_zero() * dt * 0.75;
                    if delta.length() < 0.08 {
                        person.phase = Phase::Falling;
                        person.velocity = escape * 2.0 + Vec3::Y * 1.1;
                    }
                }
                Phase::Falling => {
                    // Traverse the retained roof until past the track footprint; only then fall.
                    person.velocity.y -= 9.81 * dt;
                    person.pos += person.velocity * dt;
                    let local = tank.hull_model().inverse().transform_point3(person.pos);
                    let ground = hf.height_at(person.pos.x, person.pos.z);
                    let support = if local.x.abs() < 1.8 && local.z.abs() < 3.3 {
                        tank.pos.y + 2.65
                    } else {
                        ground
                    };
                    if person.pos.y < support {
                        person.pos.y = support;
                        person.velocity.y = 0.0;
                        if support == ground {
                            person.phase = Phase::Escaping;
                        }
                    }
                }
                Phase::Escaping => {
                    let injured_leg = person.missing[7]
                        || person.missing[8]
                        || person.missing[9]
                        || person.missing[10];
                    let speed = if injured_leg {
                        0.22
                    } else if person.wounded {
                        0.9
                    } else {
                        1.7
                    };
                    let next = person.pos + escape * speed * dt;
                    let blocked = occluders.iter().any(|o| {
                        !(o.min.distance_squared(tank.pos - Vec3::new(3.4, 0.0, 3.4)) < 0.001)
                            && next.x > o.min.x
                            && next.x < o.max.x
                            && next.z > o.min.z
                            && next.z < o.max.z
                            && next.y + 0.8 < o.max.y
                    });
                    if !blocked {
                        person.pos = next;
                    }
                    person.pos.y = hf.height_at(person.pos.x, person.pos.z);
                }
                Phase::Dead => {}
            }
            // The heat source is the actual burning wreck. Transfer is a coarse 2.1 m hull
            // proximity estimate, not cloth ignition calibrated to source flame triangles.
            if wreck.burning && person.pos.distance(tank.center()) < 2.1 {
                person.burn_s = (person.burn_s + dt).min(18.0);
            } else {
                person.burn_s = (person.burn_s - dt * 0.12).max(0.0);
            }
            if person.burn_s > 12.0 {
                person.phase = Phase::Dead;
            }
            if person.phase == Phase::Dead {
                physics::integrate_free_motion(&mut person.segments, dt);
                physics::solve_joint_constraints(&mut person.segments, 1.0, dt, 3);
                for part in &mut person.segments {
                    let ground = hf.height_at(part.pos.x, part.pos.z);
                    let local = tank.hull_model().inverse().transform_point3(part.pos);
                    let floor = if local.x.abs() < 1.6 && local.z.abs() < 2.8 {
                        person.interior_floor
                    } else {
                        ground
                    };
                    let radius = part.part.half_extents().min_element();
                    if part.pos.y < floor + radius {
                        part.pos.y = floor + radius;
                        part.vel *= 0.6;
                        part.vel.y = 0.0;
                        part.grounded = true;
                    }
                }
            } else {
                let yaw = Quat::from_rotation_y(person.heading);
                let rest = if person.phase == Phase::Waiting {
                    physics::seated_segments(person.pos, yaw, 1.0)
                } else {
                    physics::rest_segments(person.pos, yaw, 1.0)
                };
                for (i, part) in person.segments.iter_mut().enumerate() {
                    if !person.missing[i] {
                        part.pos = rest[i].pos;
                        part.rot = rest[i].rot;
                    } else if part.present {
                        part.vel.y -= 9.81 * dt;
                        part.pos += part.vel * dt;
                        let h = hf.height_at(part.pos.x, part.pos.z)
                            + part.part.half_extents().min_element();
                        if part.pos.y < h {
                            part.pos.y = h;
                            part.vel = Vec3::ZERO;
                        }
                    }
                }
            }
        }
    }
    pub(super) fn draw(&self, r: &mut Renderer) {
        let mut mats: [Vec<Mat4>; COUNT] = std::array::from_fn(|_| Vec::new());
        let mut stains = Vec::new();
        let mut flames = Vec::new();
        for person in &self.people {
            for (i, p) in person
                .segments
                .iter()
                .enumerate()
                .filter(|(_, p)| p.present)
            {
                mats[i].push(
                    Mat4::from_rotation_translation(p.rot, p.pos)
                        * Mat4::from_translation(-p.part.rest_center()),
                );
            }
            if person.bleeding {
                let h = person.interior_floor.max(person.blood_pos.y - 0.3);
                stains.push(Mat4::from_translation(Vec3::new(
                    person.blood_pos.x,
                    h + 0.008,
                    person.blood_pos.z,
                )));
            }
            if person.burn_s > 2.0 {
                for k in 0..8 {
                    let s = hash01((person.tank * 13 + person.identity * 31 + k) as u32);
                    let p = person.segments[physics::CrewBodyPart::Torso.index()].pos;
                    let pos = p + Vec3::new(
                        (s * 17.0).sin() * 0.2,
                        (person.age * 2.2 + s).fract() * 0.65,
                        (s * 29.0).cos() * 0.17,
                    );
                    let distance = pos.distance(r.camera.pos);
                    let pixel = 2.0 * distance * (r.camera.fov_y * 0.5).tan() / r.ih as f32;
                    flames.push(Mat4::from_scale_rotation_translation(
                        Vec3::splat(pixel.max(0.008)),
                        Quat::IDENTITY,
                        pos,
                    ));
                }
            }
        }
        for (batch, models) in self.parts.into_iter().zip(&mats) {
            assert!(r.set_moving_instanced_mats(batch, models));
        }
        assert!(r.set_moving_instanced_mats(self.stains, &stains));
        assert!(r.set_dynamic_instanced_mats(self.flames, &flames));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_path_cannot_disable_or_ignite_vehicle() {
        let d = module_contacts(&[]);
        assert!(!d.engine && !d.fuel && !d.ammo);
    }
    #[test]
    fn no_contact_no_filler_preserves_every_segment() {
        let mut s = physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
        let (dead, missing, wounded) = apply_damage(&mut s, Vec3::splat(5.0), &[], 0.0);
        assert!(!dead && !wounded);
        assert!(!missing.iter().any(|v| *v));
    }
    #[test]
    fn isolated_forearm_loss_does_not_kill_central_body() {
        let mut s = physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
        let p = s[4].pos;
        let f = HotFragment {
            pos: p - Vec3::Z * 0.15,
            vel: Vec3::Z * 200.0,
            size: 0.01,
            ..HotFragment::default()
        };
        let (dead, missing, wounded) = apply_damage(&mut s, p, &[f], 0.0);
        assert!(!dead && wounded);
        assert!(missing[4]);
    }
    #[test]
    fn internal_ammunition_energy_is_fatal() {
        let mut s = physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
        let (dead, _, wounded) = apply_damage(&mut s, Vec3::Y, &[], 4_184_000.0);
        assert!(dead && wounded);
    }
}
