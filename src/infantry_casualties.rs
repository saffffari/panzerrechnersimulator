//! Bounded contact-driven infantry tissue and momentum; no damage on trigger time.
use super::*;
use crew_physics as physics;
const COUNT: usize = physics::CREW_BODY_PART_COUNT;
const CAP: usize = 128;
struct Body {
    index: usize,
    team: usize,
    parts: [physics::CrewSegment; COUNT],
    missing: [bool; COUNT],
    dead: bool,
    bleeding: bool,
}
struct Grain {
    pos: Vec3,
    vel: Vec3,
    age: f32,
}
pub(super) struct Casualties {
    bodies: Vec<Body>,
    pools: [[usize; COUNT]; 2],
    blood: usize,
    grains: Vec<Grain>,
    stains: usize,
}
#[derive(Default)]
pub(super) struct Hit {
    pub fatal: bool,
    pub wounded: bool,
    pub leg_loss: bool,
}
impl Casualties {
    /// Remove every body and blood grain for a reset match.
    pub fn reset(&mut self) {
        self.bodies.clear();
        self.grains.clear();
    }

    pub fn allocate(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer) -> Self {
        let sources=[SOVIET_POSE_PATHS[0],"assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_ww2_soldier_relaxed_static.glb"];
        let pools = sources.map(|path| {
            let mut source = crate::gltf_load::load_glb(path)
                .unwrap_or_else(|_| soviet_infantry_mesh([78, 82, 70]));
            let scale = 1.53 / (source.max.y - source.min.y).max(0.1);
            let floor = source.min.y;
            for v in &mut source.vertices {
                let mut p = Vec3::from(v.pos);
                p.y -= floor;
                v.pos = (p * scale).to_array();
            }
            source.min = Vec3::new(source.min.x * scale, 0.0, source.min.z * scale);
            source.max = Vec3::new(source.max.x * scale, 1.53, source.max.z * scale);
            let pages = partition_relaxed_crew_mesh(&source);
            std::array::from_fn(|i| {
                let mesh = pages[i].clone().unwrap_or_else(|| {
                    crew_segment_fallback_mesh(physics::CrewBodyPart::ALL[i], [78, 82, 70])
                });
                let prototype = r.register_instanced_prototype(device, queue, &mesh);
                r.add_moving_instanced_pool(device, prototype, CAP)
                    .expect("infantry anatomy pool")
            })
        });
        let blood = r
            .add_dynamic_instanced_pool(device, queue, &blood_grain_mesh([92, 8, 7]), 1024)
            .expect("infantry blood");
        let proto = r.register_instanced_prototype(device, queue, &crew_gore_mesh());
        let stains = r
            .add_moving_instanced_pool(device, proto, CAP)
            .expect("infantry wounds");
        Self {
            bodies: Vec::new(),
            pools,
            blood,
            grains: Vec::new(),
            stains,
        }
    }
    pub fn clear_actor(&mut self, index: usize) {
        self.bodies.retain(|b| b.index != index);
    }
    pub fn overrides(&self, index: usize) -> bool {
        self.bodies
            .iter()
            .find(|b| b.index == index)
            .is_some_and(|b| b.dead || b.missing.iter().any(|m| *m))
    }
    pub fn hit(
        &mut self,
        index: usize,
        soldier: &Soldier,
        team: usize,
        point: Vec3,
        velocity: Vec3,
        mass: f32,
        filler_j: f32,
        blast_only: bool,
    ) -> Hit {
        if !point.is_finite() || !velocity.is_finite() || !mass.is_finite() || mass < 0.0 {
            return Hit::default();
        }
        let entry = self.bodies.iter().position(|b| b.index == index);
        let i = if let Some(i) = entry {
            i
        } else {
            if self.bodies.len() >= CAP {
                return Hit::default();
            }
            self.bodies.push(Body {
                index,
                team,
                parts: posed_segments(soldier),
                missing: [false; COUNT],
                dead: !soldier.alive,
                bleeding: false,
            });
            self.bodies.len() - 1
        };
        let body = &mut self.bodies[i];
        if !body.dead {
            let rest = posed_segments(soldier);
            for p in 0..COUNT {
                if !body.missing[p] {
                    body.parts[p].pos = rest[p].pos;
                    body.parts[p].rot = rest[p].rot;
                }
            }
        }
        let dir = velocity.normalize_or_zero();
        let energy = 0.5 * mass * velocity.length_squared();
        let contact = if blast_only {
            None
        } else {
            // The round has already been stopped by the soldier's contact volume, which stands
            // taller than this anatomy. When the swept line passes above or beside every
            // segment, deposit into the nearest present part instead of consuming the round
            // with no effect. This is a named approximation until the anatomy is posed at the
            // visible body's full height.
            first_contact(&body.parts, point - dir * 0.7, point + dir * 2.0).or_else(|| {
                let nearest = physics::nearest_body_part(&body.parts, point, 1.0);
                body.parts.iter().position(|p| p.present && p.part == nearest)
            })
        };
        let mut output = Hit::default();
        for p in 0..COUNT {
            let part = body.parts[p].part;
            // Contact work is deposited once, in the first solved anatomical volume.
            // This tissue-work fraction is provisional; remainder exits rather than
            // being repeatedly assigned to all eleven anatomical segments.
            let kinetic = if contact == Some(p) {
                energy * 0.35
            } else {
                0.0
            };
            let result = crew_damage::resolve_crew_damage(crew_damage::CrewDamageInput {
                part,
                fragment_energy_j: kinetic,
                explosive_energy_j: filler_j,
                blast_distance_m: body.parts[p].pos.distance(point),
            });
            if result.outcome == crew_damage::CrewTissueOutcome::None {
                continue;
            }
            output.wounded = true;
            body.bleeding = true;
            let fatal = matches!(
                part,
                physics::CrewBodyPart::Head
                    | physics::CrewBodyPart::Torso
                    | physics::CrewBodyPart::Pelvis
            ) && (result.total_work_j >= 12.0
                || result.outcome >= crew_damage::CrewTissueOutcome::Detachment);
            body.dead |= fatal;
            output.fatal |= fatal;
            body.missing[p] |= result.detach_joint || result.hide_mesh_part;
            body.parts[p].detached |= result.detach_joint;
            body.parts[p].present &= !result.hide_mesh_part;
            if kinetic > 0.0 {
                physics::apply_fragment_impulse_to_part(
                    &mut body.parts,
                    part,
                    point,
                    velocity,
                    kinetic,
                    1.0,
                );
            }
            let wound = if kinetic > 0.0 {
                point
            } else {
                body.parts[p].pos
            };
            let count = result
                .blood_cube_count
                .min(48)
                .min(1024 - self.grains.len());
            for k in 0..count {
                let u = (k as f32 * 2.399963).sin();
                let v = (k as f32 * 2.399963).cos();
                self.grains.push(Grain {
                    pos: wound,
                    vel: dir * 1.2 + Vec3::new(u, 0.5 + v.abs(), v) * 1.3,
                    age: 0.0,
                });
            }
        }
        for (parent, child) in [(3, 4), (5, 6), (7, 8), (9, 10)] {
            if body.missing[parent] {
                body.missing[child] = true;
                body.parts[child].detached = true;
            }
        }
        if filler_j > 0.0 {
            let impulses =
                physics::confined_segment_blast_impulses(&body.parts, point, filler_j, 1.0);
            for (p, impulse) in body.parts.iter_mut().zip(impulses) {
                if p.present {
                    p.vel += impulse / p.part.mass_kg();
                }
            }
        }
        output.leg_loss = body.missing[7..].iter().any(|m| *m);
        output.fatal |= body.dead;
        output
    }
    pub fn update_draw(
        &mut self,
        dt: f32,
        soldiers: &[Soldier],
        hf: &Heightfield,
        r: &mut Renderer,
    ) {
        let dt = dt.clamp(0.0, 0.05);
        let mut mats: [[Vec<Mat4>; COUNT]; 2] =
            std::array::from_fn(|_| std::array::from_fn(|_| Vec::new()));
        let mut stains = Vec::new();
        for body in &mut self.bodies {
            let Some(soldier) = soldiers.get(body.index) else {
                continue;
            };
            body.dead |= !soldier.alive;
            if body.dead {
                physics::integrate_free_motion(&mut body.parts, dt);
                physics::solve_joint_constraints(&mut body.parts, 1.0, dt, 4);
            } else {
                let rest = posed_segments(soldier);
                for p in 0..COUNT {
                    if !body.missing[p] {
                        body.parts[p].pos = rest[p].pos;
                        body.parts[p].rot = rest[p].rot;
                    } else {
                        body.parts[p].vel.y -= 9.81 * dt;
                        body.parts[p].pos += body.parts[p].vel * dt;
                    }
                }
            }
            for p in &mut body.parts {
                let ground = hf.height_at(p.pos.x, p.pos.z) + p.part.half_extents().min_element();
                if p.pos.y < ground {
                    p.pos.y = ground;
                    p.vel.y = 0.0;
                    p.vel *= (-5.0 * dt).exp();
                    p.grounded = true;
                }
                if p.present && (body.dead || body.missing.iter().any(|m| *m)) {
                    mats[body.team.min(1)][p.part.index()].push(
                        Mat4::from_rotation_translation(p.rot, p.pos)
                            * Mat4::from_translation(-p.part.rest_center()),
                    );
                }
            }
            if body.bleeding {
                let p = body.parts[0].pos;
                stains.push(Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.3),
                    Quat::IDENTITY,
                    Vec3::new(p.x, hf.height_at(p.x, p.z) + 0.02, p.z),
                ));
            }
        }
        self.grains.retain_mut(|g| {
            g.age += dt;
            g.vel.y -= 9.81 * dt;
            g.pos += g.vel * dt;
            g.age < 2.0
        });
        let grains = self
            .grains
            .iter()
            .map(|g| {
                let pixel =
                    2.0 * g.pos.distance(r.camera.pos) * (r.camera.fov_y * 0.5).tan() / r.ih as f32;
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(pixel.max(0.004)),
                    Quat::IDENTITY,
                    g.pos,
                )
            })
            .collect::<Vec<_>>();
        for team in 0..2 {
            for p in 0..COUNT {
                r.set_moving_instanced_mats(self.pools[team][p], &mats[team][p]);
            }
        }
        r.set_dynamic_instanced_mats(self.blood, &grains);
        r.set_moving_instanced_mats(self.stains, &stains);
    }
}
pub(super) fn posed_segments(soldier: &Soldier) -> [physics::CrewSegment; COUNT] {
    let prone = soldier.is_prone_wounded();
    let rotation = if prone {
        crate::infantry_combat::prone_rotation(soldier.yaw)
    } else {
        // Soldier forward is (sin y, 0, -cos y) = from_rotation_y(PI - y) * Z, the same facing
        // the drawn figure gets from `from_rotation_arc(Z, forward)`. A bare `y` mirrored it.
        Quat::from_rotation_y(std::f32::consts::PI - soldier.yaw)
    };
    physics::rest_segments(
        soldier.pos + if prone { Vec3::Y * 0.18 } else { Vec3::ZERO },
        rotation,
        1.0,
    )
}
fn first_contact(parts: &[physics::CrewSegment; COUNT], from: Vec3, to: Vec3) -> Option<usize> {
    parts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.present)
        .filter_map(|(i, p)| {
            crew_damage::swept_sphere_obb_fraction(
                from,
                to,
                0.004,
                crew_damage::CrewHitVolume::new(p.part, p.pos, p.rot, p.part.half_extents()),
            )
            .map(|t| (t, i))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prone_wound_moves_contact_volumes_and_eye_together() {
        let mut soldier = Soldier::new(Vec3::ZERO, 0.0, [Vec3::ZERO; 4], 0, usize::MAX, usize::MAX);
        soldier.set_wounded_motion(0.12);
        let parts = posed_segments(&soldier);
        assert!(
            parts[physics::CrewBodyPart::Head.index()]
                .pos
                .distance(soldier.anatomical_eye())
                < 1e-5
        );
        assert!(
            first_contact(&parts, Vec3::new(0.0, 1.3, -3.0), Vec3::new(0.0, 1.3, 3.0)).is_none()
        );
        assert!(first_contact(
            &parts,
            Vec3::new(0.0, 0.18, -3.0),
            Vec3::new(0.0, 0.18, 3.0)
        )
        .is_some());
    }
    #[test]
    fn near_miss_is_not_an_anatomical_hit() {
        let p = physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
        assert_eq!(
            first_contact(&p, Vec3::new(4.0, 1.0, -1.0), Vec3::new(4.0, 1.0, 1.0)),
            None
        );
    }
    #[test]
    fn finite_path_consumes_only_first_anatomical_contact() {
        let p = physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
        let i = first_contact(&p, Vec3::new(0.0, 1.0, -2.0), Vec3::new(0.0, 1.0, 2.0));
        assert!(i.is_some());
    }
}
