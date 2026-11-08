//! A dead man's steel helmet as a rigid body (2026-11-02, the owner: helmets are not voxels, only
//! organic matter is; the helmet keeps the crew's look). While its strap holds it rides the head
//! segment; a fragment meeting its shell is stopped or slowed by punching a plug out of it, and
//! the momentum the fragment loses goes to the helmet; a burst pushes the light shell harder than
//! the head under it. When the helmet's momentum against the head passes the strap's hold, or the
//! head's soft tissue is mostly gone, it comes away as a rigid body of its own mesh: gravity, the
//! vehicle's surfaces met at support points on its shell, sequential contact impulses with
//! restitution and Coulomb friction, and rest once quiet.
use super::*;
use super::crew_tissue::Strike;
use crate::gltf_load::CpuMesh;
use glam::Mat3;

/// The helmet's mass (kg; authored, about a German M35's).
const HELMET_MASS_KG: f32 = 1.0;
/// Shell thickness (m) and the steel's shear strength (Pa) for plugging (authored game-level
/// values: a millimetre of hard helmet steel, about half its tensile strength in shear).
const SHELL_THICKNESS_M: f32 = 0.0011;
const SHELL_SHEAR_PA: f32 = 6.0e8;
const STEEL_DENSITY: f32 = 7_850.0;
/// The strap's hold (N·s): momentum against the head beyond it tears the helmet away (authored:
/// a leather chin strap's breaking force, about a kilonewton, over about a millisecond).
const STRAP_HOLD_NS: f32 = 1.0;
/// Support points on the shell met against the vehicle's surfaces (farthest-point sampled from
/// its vertices; named: not its whole surface).
const SUPPORT_POINTS: usize = 32;
/// Contact restitution (above `BOUNCE_MPS` closing) and Coulomb friction, steel on painted steel
/// (authored).
const RESTITUTION: f32 = 0.3;
const BOUNCE_MPS: f32 = 0.5;
const FRICTION: f32 = 0.45;
/// A sweep starts this far behind a support point so a point resting on a face keeps it.
const SWEEP_MARGIN_M: f32 = 0.006;
/// Quiet in contact (m/s, rad/s) for `REST_S` seconds: the helmet rests.
const REST_MPS: f32 = 0.08;
const REST_RAD_S: f32 = 0.6;
const REST_S: f32 = 0.3;

/// The helmet's shape in the relaxed figure's frame (the frame its mesh is authored in).
pub(super) struct HelmetShape {
    triangles: Vec<[Vec3; 3]>,
    /// Centre of mass and inverse inertia about it (unit scale), the shell's mass spread by area.
    pub com: Vec3,
    inverse_inertia: Mat3,
    support: Vec<Vec3>,
    /// Mean projected area (m²), Cauchy's relation as the part volumes use it.
    projected_area_m2: f32,
}

fn outer(v: Vec3) -> Mat3 {
    Mat3::from_cols(v * v.x, v * v.y, v * v.z)
}

/// Möller–Trumbore, both faces: the distance along `dir` (unit) to the triangle.
fn ray_triangle(origin: Vec3, dir: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1.0e-12 {
        return None;
    }
    let t = origin - a;
    let u = t.dot(p) / det;
    let q = t.cross(e1);
    let v = dir.dot(q) / det;
    if u < 0.0 || v < 0.0 || u + v > 1.0 {
        return None;
    }
    let s = e2.dot(q) / det;
    (s >= 0.0).then_some(s)
}

impl HelmetShape {
    pub(super) fn new(mesh: &CpuMesh) -> Self {
        let triangles: Vec<[Vec3; 3]> = mesh.indices.chunks_exact(3)
            .map(|t| [0, 1, 2].map(|i| Vec3::from(mesh.vertices[t[i] as usize].pos))).collect();
        let area: Vec<f32> = triangles.iter().map(|[a, b, c]| 0.5 * (*b - *a).cross(*c - *a).length()).collect();
        let total: f32 = area.iter().sum::<f32>().max(1.0e-9);
        let (mut first, mut second) = (Vec3::ZERO, Mat3::ZERO);
        for (&[a, b, c], &s) in triangles.iter().zip(&area) {
            let m = HELMET_MASS_KG * s / total;
            let sum = a + b + c;
            first += sum * (m / 3.0);
            // Exact second moment of a uniformly weighted triangular lamina.
            second += (outer(a) + outer(b) + outer(c) + outer(sum)) * (m / 12.0);
        }
        let com = first / HELMET_MASS_KG;
        let central = second - outer(com) * HELMET_MASS_KG;
        let inertia = Mat3::IDENTITY * (central.x_axis.x + central.y_axis.y + central.z_axis.z) - central;
        // Farthest-point sampling of the vertices, from the lowest (the rim) outward.
        let mut points: Vec<Vec3> = mesh.vertices.iter().map(|v| Vec3::from(v.pos)).collect();
        points.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)).then(a.z.total_cmp(&b.z)));
        points.dedup_by(|a, b| a.distance_squared(*b) < 1.0e-10);
        let mut support = Vec::new();
        if let Some(&first) = points.first() {
            support.push(first);
            let mut nearest: Vec<f32> = points.iter().map(|p| p.distance_squared(first)).collect();
            while support.len() < SUPPORT_POINTS.min(points.len()) {
                let (far, _) = nearest.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).expect("points");
                let chosen = points[far];
                support.push(chosen);
                for (d, p) in nearest.iter_mut().zip(&points) {
                    *d = d.min(p.distance_squared(chosen));
                }
            }
        }
        Self { triangles, com, inverse_inertia: inertia.inverse(), support, projected_area_m2: total / 4.0 }
    }

    /// The nearest crossing of the shell along the segment (figure frame): its distance from `from`.
    fn first_crossing(&self, from: Vec3, to: Vec3) -> Option<f32> {
        let length = from.distance(to);
        let dir = (to - from) / length.max(1.0e-9);
        self.triangles.iter().filter_map(|&t| ray_triangle(from, dir, t)).filter(|&s| s <= length)
            .fold(None, |best: Option<f32>, s| Some(best.map_or(s, |b| b.min(s))))
    }

    /// The strikes that reach the head once the shell has had its say, and the momentum given to
    /// the helmet (figure frame) with the point it acted at. A fragment crossing the shell punches
    /// a plug out of it (shear plugging: the work ½·P·t²·τ over the plug's perimeter P, the
    /// residual speed by Recht and Ipson's momentum form, the plug flying on with it; named, the
    /// shell's exit side and the hole are not modelled). One slower than the plug's ballistic limit
    /// stops in the shell and the helmet takes all its momentum. A burst's work on the head reaches
    /// the shell in proportion to its projected area and drives it at its own speed; its push
    /// against the head is the burst's impulse less the head's own motion (the head's velocity in
    /// the figure frame, `head_velocity`).
    pub(super) fn through_shell(&self, strikes: &[Strike], head_velocity: Vec3) -> (Vec<Strike>, Vec3, Vec3) {
        let head = crew_physics::CrewBodyPart::Head;
        let mut out = Vec::with_capacity(strikes.len());
        let (mut impulse, mut moment_at, mut weight) = (Vec3::ZERO, Vec3::ZERO, 0.0f32);
        for strike in strikes {
            let dir = strike.dir.normalize_or_zero();
            if strike.part != head || !(strike.work_j > 0.0) {
                out.push(*strike);
                continue;
            }
            if dir == Vec3::ZERO || strike.speed_mps <= 0.0 {
                // A burst: the shell's share of the fluence the head met, at its own speed.
                let share = self.projected_area_m2 / crew_damage::mean_projected_area_m2(head).max(1.0e-6);
                let away = (self.com - strike.at).normalize_or_zero();
                let speed = (2.0 * strike.work_j * share / HELMET_MASS_KG).sqrt();
                let push = HELMET_MASS_KG * (away * speed - head_velocity);
                if push.dot(away) > 0.0 {
                    impulse += push;
                    moment_at += self.com * push.length();
                    weight += push.length();
                }
                out.push(*strike);
                continue;
            }
            let from = strike.at - dir * 0.35;
            let Some(s) = self.first_crossing(from, strike.at + dir * 0.35) else {
                out.push(*strike);
                continue;
            };
            let mass = 2.0 * strike.work_j / (strike.speed_mps * strike.speed_mps);
            let side = (mass / STEEL_DENSITY).cbrt();
            let plug = STEEL_DENSITY * side * side * SHELL_THICKNESS_M;
            let plugging_j = 0.5 * 4.0 * side * SHELL_THICKNESS_M * SHELL_THICKNESS_M * SHELL_SHEAR_PA;
            let limit2 = 2.0 * plugging_j / mass;
            let v2 = strike.speed_mps * strike.speed_mps;
            let hit = from + dir * s;
            let lost = if v2 <= limit2 {
                mass * strike.speed_mps
            } else {
                let residual = mass / (mass + plug) * (v2 - limit2).sqrt();
                out.push(Strike { speed_mps: residual, work_j: 0.5 * (mass + plug) * residual * residual, ..*strike });
                mass * strike.speed_mps - (mass + plug) * residual
            };
            impulse += dir * lost;
            moment_at += hit * lost;
            weight += lost;
        }
        let at = if weight > 0.0 { moment_at / weight } else { self.com };
        (out, impulse, at)
    }
}

/// A helmet that came away from its man: its pose and motion in its frame (`frame`'s local
/// coordinates), the mesh drawn at `pos + rot·scale·(p − com)`.
#[derive(Clone, Debug)]
pub(super) struct HelmetBody {
    pub frame: BloodFrame,
    pub pos: Vec3,
    pub rot: Quat,
    pub scale: f32,
    pub vel: Vec3,
    pub spin: Vec3,
    pub settled: bool,
    quiet_s: f32,
}

impl HelmetBody {
    /// Whether the momentum against the head (figure frame) tears the strap.
    pub(super) fn strap_tears(impulse: Vec3) -> bool {
        impulse.length() > STRAP_HOLD_NS
    }

    /// Let the helmet go from the ragdoll's head with the head's motion and `impulse` (figure
    /// frame) acting at `at` (figure frame).
    pub(super) fn release(shape: &HelmetShape, ragdoll: &CrewRagdoll, impulse: Vec3, at: Vec3) -> Self {
        let head = crew_physics::CrewBodyPart::Head;
        let segment = ragdoll.segments[head.index()];
        let figure_to_local = crew_segment_local_pose(segment) * Mat4::from_scale(Vec3::splat(ragdoll.scale))
            * Mat4::from_translation(-head.rest_center());
        let pos = figure_to_local.transform_point3(shape.com);
        let body = Self { frame: ragdoll.frame, pos, rot: segment.rot, scale: ragdoll.scale, vel: Vec3::ZERO,
            spin: Vec3::ZERO, settled: false, quiet_s: 0.0 };
        let j = segment.rot * impulse;
        let arm = segment.rot * ((at - shape.com) * ragdoll.scale);
        Self {
            vel: segment.vel + segment.angular_vel.cross(pos - segment.pos) + j / HELMET_MASS_KG,
            spin: segment.angular_vel + body.inverse_world(shape) * arm.cross(j),
            ..body
        }
    }

    fn inverse_world(&self, shape: &HelmetShape) -> Mat3 {
        let r = Mat3::from_quat(self.rot);
        r * (shape.inverse_inertia / (self.scale * self.scale)) * r.transpose()
    }

    /// Its frame-local model: mesh (figure frame) to its frame.
    pub(super) fn local_model(&self, shape: &HelmetShape) -> Mat4 {
        Mat4::from_scale_rotation_translation(Vec3::splat(self.scale), self.rot, self.pos) * Mat4::from_translation(-shape.com)
    }

    /// Advance by `dt` under `gravity` (frame-local) against the surfaces `surface` finds along a
    /// segment (frame-local: the fraction along it and the face's normal).
    pub(super) fn advance(&mut self, shape: &HelmetShape, dt: f32, gravity: Vec3,
        surface: &dyn Fn(Vec3, Vec3) -> Option<(f32, Vec3)>) {
        if self.settled || !(dt > 0.0) {
            return;
        }
        let steps = ((dt * 240.0).ceil()).max((self.vel.length() * dt / 0.01).ceil()).clamp(1.0, 64.0) as usize;
        let h = dt / steps as f32;
        let up = (-gravity).normalize_or_zero();
        for _ in 0..steps {
            self.vel += gravity * h;
            let before: Vec<Vec3> = shape.support.iter().map(|&p| self.pos + self.rot * ((p - shape.com) * self.scale)).collect();
            self.pos += self.vel * h;
            self.rot = (Quat::from_scaled_axis(self.spin * h) * self.rot).normalize();
            let mut contacts: Vec<(Vec3, Vec3, f32)> = Vec::new();
            for (&p, &was) in shape.support.iter().zip(&before) {
                let now = self.pos + self.rot * ((p - shape.com) * self.scale);
                let moved = now - was;
                let dir = if moved.length() > 1.0e-6 { moved.normalize() } else { -up };
                let from = was - dir * SWEEP_MARGIN_M;
                let to = now + dir * 0.001;
                if let Some((fraction, normal)) = surface(from, to) {
                    let at = from.lerp(to, fraction);
                    let normal = if normal.dot(dir) > 0.0 { -normal } else { normal };
                    let depth = (at - now).dot(normal);
                    if depth > -0.001 {
                        contacts.push((now - self.pos, normal, depth.max(0.0)));
                    }
                }
            }
            if contacts.is_empty() {
                if self.vel.length() > REST_MPS * 4.0 {
                    self.quiet_s = 0.0;
                }
                continue;
            }
            let deepest = contacts.iter().max_by(|a, b| a.2.total_cmp(&b.2)).copied().expect("contacts");
            self.pos += deepest.1 * deepest.2;
            self.solve(shape, &contacts);
            if self.vel.length() < REST_MPS && self.spin.length() < REST_RAD_S
                && contacts.iter().any(|c| c.1.dot(up) > 0.5) {
                self.quiet_s += h;
                if self.quiet_s > REST_S {
                    self.vel = Vec3::ZERO;
                    self.spin = Vec3::ZERO;
                    self.settled = true;
                    return;
                }
            } else {
                self.quiet_s = (self.quiet_s - h).max(0.0);
            }
        }
        if self.pos.length() > 12.0 {
            self.settled = true;
        }
    }

    /// Sequential contact impulses at the support points (arm from the centre of mass, face
    /// normal): restitution only on a real impact, Coulomb friction bounded by each normal impulse.
    fn solve(&mut self, shape: &HelmetShape, contacts: &[(Vec3, Vec3, f32)]) {
        let inverse = self.inverse_world(shape);
        let inv_mass = 1.0 / HELMET_MASS_KG;
        let targets: Vec<f32> = contacts.iter().map(|&(arm, normal, _)| {
            let closing = (self.vel + self.spin.cross(arm)).dot(normal);
            if closing < -BOUNCE_MPS { -closing * RESTITUTION } else { 0.0 }
        }).collect();
        let mut normal_total = vec![0.0f32; contacts.len()];
        let mut tangent_total = vec![Vec3::ZERO; contacts.len()];
        for _ in 0..10 {
            for (i, &(arm, normal, _)) in contacts.iter().enumerate() {
                let lever = arm.cross(normal);
                let effective = inv_mass + lever.dot(inverse * lever);
                let closing = (self.vel + self.spin.cross(arm)).dot(normal);
                let next = (normal_total[i] + (targets[i] - closing) / effective).max(0.0);
                let change = next - normal_total[i];
                normal_total[i] = next;
                self.vel += normal * (change * inv_mass);
                self.spin += inverse * (lever * change);
                let v = self.vel + self.spin.cross(arm);
                let tangent = v - normal * v.dot(normal);
                let speed = tangent.length();
                if speed > 1.0e-7 {
                    let t = tangent / speed;
                    let lever = arm.cross(t);
                    let wanted = tangent_total[i] - t * (speed / (inv_mass + lever.dot(inverse * lever)));
                    let next = wanted.clamp_length_max(FRICTION * normal_total[i]);
                    let change = next - tangent_total[i];
                    tangent_total[i] = next;
                    self.vel += change * inv_mass;
                    self.spin += inverse * arm.cross(change);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dome of a helmet: a 13 cm half sphere shell over a head at the figure's 1.4 m.
    fn dome() -> HelmetShape {
        let mut mesh = crate::prims::box_mesh(Vec3::ZERO, Vec3::ONE, [80, 80, 80]);
        mesh.vertices.clear();
        mesh.indices.clear();
        let (rings, segments) = (6u32, 16u32);
        let centre = Vec3::new(0.0, 1.40, 0.0);
        for r in 0..=rings {
            let polar = std::f32::consts::FRAC_PI_2 * r as f32 / rings as f32;
            for s in 0..segments {
                let a = std::f32::consts::TAU * s as f32 / segments as f32;
                let p = centre + Vec3::new(polar.sin() * a.cos(), polar.cos(), polar.sin() * a.sin()) * 0.13;
                mesh.vertices.push(mesh_vertex(p));
            }
        }
        for r in 0..rings {
            for s in 0..segments {
                let (a, b) = (r * segments + s, r * segments + (s + 1) % segments);
                let (c, d) = (a + segments, b + segments);
                mesh.indices.extend([a, c, b, b, c, d]);
            }
        }
        HelmetShape::new(&mesh)
    }

    fn mesh_vertex(p: Vec3) -> crate::gltf_load::Vertex {
        let mut v = crate::prims::box_mesh(Vec3::ZERO, Vec3::ONE, [80, 80, 80]).vertices[0];
        v.pos = p.to_array();
        v
    }

    #[test]
    fn a_slow_fragment_stops_in_the_shell_and_a_fast_one_punches_through() {
        let shape = dome();
        let head = crew_physics::CrewBodyPart::Head;
        let at = Vec3::new(0.0, 1.45, 0.0);
        let strike = |mass: f32, speed: f32| Strike { part: head, at, dir: Vec3::NEG_X, work_j: 0.5 * mass * speed * speed, speed_mps: speed };
        // 2 g at 40 m/s: below the plug's limit; the helmet takes all of its momentum.
        let (through, impulse, hit) = shape.through_shell(&[strike(0.002, 40.0)], Vec3::ZERO);
        assert!(through.is_empty());
        assert!((impulse - Vec3::NEG_X * 0.08).length() < 1.0e-4, "{impulse:?}");
        assert!(hit.x > 0.1, "met on the near side of the shell: {hit:?}");
        // 2 g at 600 m/s: through, a little slower, and the shell takes almost nothing.
        let (through, impulse, _) = shape.through_shell(&[strike(0.002, 600.0)], Vec3::ZERO);
        assert_eq!(through.len(), 1);
        assert!(through[0].speed_mps > 500.0 && through[0].speed_mps < 600.0, "{:?}", through[0]);
        assert!(impulse.length() < 0.3 && !HelmetBody::strap_tears(impulse), "{impulse:?}");
        // A fragment that misses the shell (below the rim) reaches the head untouched.
        let low = Strike { at: Vec3::new(0.0, 1.30, 0.0), ..strike(0.002, 600.0) };
        let (through, impulse, _) = shape.through_shell(&[low], Vec3::ZERO);
        assert_eq!(through[0].speed_mps, 600.0);
        assert_eq!(impulse, Vec3::ZERO);
    }

    #[test]
    fn a_burst_throws_the_light_shell_off_the_head() {
        let shape = dome();
        let head = crew_physics::CrewBodyPart::Head;
        let burst = Strike { part: head, at: Vec3::new(0.0, 1.40, 1.0), dir: Vec3::ZERO, work_j: 200.0, speed_mps: 0.0 };
        let (through, impulse, _) = shape.through_shell(&[burst], Vec3::ZERO);
        assert_eq!(through.len(), 1, "the head still takes its work");
        assert!(impulse.z < 0.0 && HelmetBody::strap_tears(impulse), "{impulse:?}");
        let gentle = Strike { work_j: 0.05, ..burst };
        let (_, impulse, _) = shape.through_shell(&[gentle], Vec3::ZERO);
        assert!(!HelmetBody::strap_tears(impulse), "{impulse:?}");
    }

    #[test]
    fn a_loose_helmet_falls_and_rests_on_the_floor() {
        let shape = dome();
        let mut body = HelmetBody { frame: BloodFrame::Hull, pos: shape.com + Vec3::new(0.0, 0.5, 0.0), rot: Quat::from_rotation_z(0.4),
            scale: 1.0, vel: Vec3::new(1.5, 0.0, 0.0), spin: Vec3::new(0.0, 2.0, 3.0), settled: false, quiet_s: 0.0 };
        let floor = 1.0f32;
        let surface = |from: Vec3, to: Vec3| {
            if (from.y - floor) * (to.y - floor) > 0.0 && from.y > floor {
                return None;
            }
            if from.y < floor && to.y < floor {
                return None;
            }
            let f = ((from.y - floor) / (from.y - to.y)).clamp(0.0, 1.0);
            Some((f, Vec3::Y))
        };
        for _ in 0..600 {
            body.advance(&shape, 1.0 / 60.0, Vec3::new(0.0, -9.81, 0.0), &surface);
            if body.settled {
                break;
            }
        }
        assert!(body.settled, "{body:?}");
        let lowest = shape.support.iter().map(|&p| (body.pos + body.rot * (p - shape.com)).y).fold(f32::INFINITY, f32::min);
        assert!((lowest - floor).abs() < 0.01, "rests on the floor: lowest support at {lowest}");
    }
}

impl HelmetShape {
    pub(super) fn particle_surfaces(&self,actor:u32,model:Mat4,scene:&mut particle_contacts::Scene){
        for (i,t) in self.triangles.iter().enumerate(){scene.triangle(particle_contacts::key(actor,3,i as u32),t.map(|p|model.transform_point3(p)));}
    }
}
