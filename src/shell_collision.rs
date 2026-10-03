//! Finite cartridge bodies against retained source triangles. Translation uses continuous SAT;
//! rotation is split by surface travel, so thin steel cannot be crossed by a fast carry swing.
//! The conservative box envelope preserves the authored length/diameter, with no radial cabin
//! clamp, floor teleport, view-dependent collision, or body lifetime.
use glam::{Mat4, Quat, Vec3};

const SKIN: f32 = 0.001;
const ROTATION_SURFACE_STEP: f32 = 0.001;

#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub half: Vec3,
    pub mass: f32,
}

#[derive(Clone, Copy)]
pub struct Carry {
    pub position: Vec3,
    pub rotation: Quat,
    pub stiffness: f32,
    pub damping: f32,
}

#[derive(Default, Debug)]
pub struct Report {
    pub impact_speed: f32,
    /// Resolved contact belonging to the strongest impulse in this step, before response.
    pub tangential_speed: f32,
    pub contact_point: Vec3,
    pub grounded: bool,
    pub budget_limited: bool,
}

/// Shipping drop physics has a fixed shared budget, independent of mesh density, mouse
/// momentum and the duration of a stalled frame. Exhaustion defers movement, never skips steel.
pub struct DropBudget {
    pub tests_left: usize,
    pub queries_left: usize,
    triangles: Vec<[Vec3; 3]>,
}
impl Default for DropBudget {
    fn default() -> Self {
        Self { tests_left: 16_384, queries_left: 96, triangles: Vec::with_capacity(512) }
    }
}

#[derive(Clone, Copy, Default)]
pub struct DropRest {
    pub sleeping: bool,
    quiet_s: f32,
    scene_signature: u64,
}

/// Sleeping contacts must wake when the tank, floor, gun, or any hatch moves. Camera movement
/// and renderer frame counters are deliberately absent: they do not move collision surfaces.
pub fn scene_signature(models: &[Mat4]) -> u64 {
    models.iter().flat_map(|m| m.to_cols_array()).fold(0xcbf29ce484222325, |h, v|
        (h ^ v.to_bits() as u64).wrapping_mul(0x100000001b3))
}

impl DropBudget {
    fn contact(&mut self, body: &Body, delta: Vec3,
        source: &super::surface_fire::SourceContacts<'_>) -> Result<Option<Hit>, ()> {
        if self.tests_left == 0 || self.queries_left == 0 { return Err(()); }
        self.queries_left -= 1;
        // Tight swept OBB bounds, rather than the old full-length bounding sphere around
        // an entire frame's possible motion. The BVH query reuses this scratch allocation.
        let half = (body.rotation * Vec3::X).abs() * body.half.x
            + (body.rotation * Vec3::Y).abs() * body.half.y
            + (body.rotation * Vec3::Z).abs() * body.half.z + Vec3::splat(SKIN);
        let end = body.position + delta;
        if !source.world_triangles_into(body.position.min(end) - half,
            body.position.max(end) + half, &mut self.triangles, self.tests_left) {
            self.queries_left = 0;
            return Err(());
        }
        self.tests_left -= self.triangles.len();
        Ok(sweep(body, delta, &self.triangles))
    }
}

#[derive(Clone, Copy, Debug)]
struct Hit { fraction: f32, normal: Vec3, depth: f32 }

/// Exact translational OBB/triangle interval, including edges and both sides of thin steel.
fn sweep_triangle(body: &Body, delta: Vec3, triangle: [Vec3; 3]) -> Option<Hit> {
    let inverse = body.rotation.conjugate();
    let tri = triangle.map(|p| inverse * (p - body.position));
    let motion = inverse * delta;
    let edges = [tri[1] - tri[0], tri[2] - tri[1], tri[0] - tri[2]];
    let normal = edges[0].cross(edges[1]);
    if normal.length_squared() < 1e-16 { return None; }
    let mut axes = [Vec3::ZERO; 13];
    axes[..3].copy_from_slice(&[Vec3::X, Vec3::Y, Vec3::Z]);
    axes[3] = normal;
    for (i, edge) in edges.into_iter().enumerate() {
        for (j, axis) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
            axes[4 + i * 3 + j] = edge.cross(axis);
        }
    }
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    let mut enter_normal = Vec3::ZERO;
    let mut overlap = f32::INFINITY;
    let mut overlap_normal = Vec3::ZERO;
    for axis in axes {
        if axis.length_squared() < 1e-14 { continue; }
        let axis = axis.normalize();
        let projections = tri.map(|p| p.dot(axis));
        let lo = projections.into_iter().fold(f32::INFINITY, f32::min);
        let hi = projections.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let radius = axis.abs().dot(body.half);
        let speed = motion.dot(axis);
        let positive_depth = radius - lo;
        let negative_depth = hi + radius;
        let depth = positive_depth.min(negative_depth);
        if depth < overlap {
            overlap = depth;
            overlap_normal = if positive_depth < negative_depth { -axis } else { axis };
        }
        if speed.abs() < 1e-9 {
            if lo > radius || hi < -radius { return None; }
            continue;
        }
        let a = (lo - radius) / speed;
        let b = (hi + radius) / speed;
        let near = a.min(b);
        let far = a.max(b);
        if near > enter {
            enter = near;
            enter_normal = -axis * speed.signum();
        }
        exit = exit.min(far);
        if enter > exit || exit < 0.0 || enter > 1.0 { return None; }
    }
    let local_normal = if enter_normal.length_squared() > 0.5 {
        enter_normal
    } else { overlap_normal };
    if local_normal.length_squared() < 0.5 { return None; }
    let normal = body.rotation * local_normal;
    // Existing contact permits motion away from the surface. A tiny resting overlap must not
    // turn a tangential hand movement into a welded shell.
    if enter == 0.0 && overlap < SKIN * 0.1 && delta.dot(normal) >= -1e-8 { return None; }
    Some(Hit { fraction: enter, normal, depth: if enter == 0.0 { overlap.max(0.0) } else { 0.0 } })
}

fn sweep(body: &Body, delta: Vec3, triangles: &[[Vec3; 3]]) -> Option<Hit> {
    triangles.iter().filter_map(|&tri| sweep_triangle(body, delta, tri))
        .min_by(|a,b| a.fraction.total_cmp(&b.fraction).then_with(|| b.depth.total_cmp(&a.depth)))
}

impl Body {
    pub fn model(&self) -> Mat4 { Mat4::from_rotation_translation(self.rotation, self.position) }

    /// Interactive ammunition drops. The archival exact solver below is retained for offline
    /// geometry tests; it must not run in the render loop. This path caps catch-up, spin, contacts,
    /// and shared triangle work, queries the BVH at each pose, and sleeps stationary contacts.
    pub fn advance_dropped(&mut self, dt: f32, source: &super::surface_fire::SourceContacts<'_>,
        signature: u64, rest: &mut DropRest, budget: &mut DropBudget, restitution: f32) -> Report {
        let mut report = Report::default();
        if !dt.is_finite() || dt <= 0.0 { return report; }
        if rest.scene_signature != signature {
            rest.sleeping = false;
            rest.quiet_s = 0.0;
            rest.scene_signature = signature;
        }
        if rest.sleeping { return report; }
        // Slow frames do not demand more simulation work than two normal 60 Hz frames.
        let dt = dt.min(1.0 / 30.0);
        let steps = (dt * 120.0).ceil().clamp(1.0, 4.0) as usize;
        let h = dt / steps as f32;
        let old_position = self.position;
        let old_rotation = self.rotation;
        self.velocity = if self.velocity.is_finite() { self.velocity.clamp_length_max(12.0) } else { Vec3::ZERO };
        self.angular_velocity = if self.angular_velocity.is_finite() {
            self.angular_velocity.clamp_length_max(12.0)
        } else { Vec3::ZERO };
        for _ in 0..steps {
            self.velocity.y -= 9.8 * h;
            let mut remaining = h;
            for _ in 0..4 {
                let delta = self.velocity * remaining;
                let hit = match budget.contact(self, delta, source) {
                    Ok(Some(hit)) => hit,
                    Ok(None) => { self.position += delta; break; }
                    Err(()) => { report.budget_limited = true; return report; }
                };
                let safe = (hit.fraction - SKIN / delta.length().max(SKIN)).max(0.0);
                self.position += delta * safe;
                self.position += hit.normal * (hit.depth + SKIN).min(0.004);
                self.impulse(hit.normal, restitution, &mut report);
                self.velocity = self.velocity.clamp_length_max(12.0);
                self.angular_velocity = self.angular_velocity.clamp_length_max(12.0);
                let inward = self.velocity.dot(hit.normal);
                if inward < 0.0 { self.velocity -= hit.normal * inward; }
                remaining *= 1.0 - hit.fraction;
                if remaining < 1e-6 { break; }
            }
            // At most four 1 cm surface-travel rotation checks. Reduce the attempted angular
            // travel when necessary instead of skipping intermediate collision tests.
            let max_angle = 0.04 / self.half.length().max(0.001);
            let angular = self.angular_velocity.clamp_length_max(max_angle / h);
            let angle = angular.length() * h;
            let rotations = (angle * self.half.length() / 0.01).ceil().clamp(1.0, 4.0) as usize;
            let rotation_step = Quat::from_scaled_axis(angular * (h / rotations as f32));
            if angle > 1e-5 {
                for _ in 0..rotations {
                    let previous = self.rotation;
                    self.rotation = (rotation_step * previous).normalize();
                    match budget.contact(self, Vec3::ZERO, source) {
                        Ok(None) => {},
                        Ok(Some(hit)) => {
                            self.rotation = previous;
                            self.impulse(hit.normal, restitution, &mut report);
                            self.velocity = self.velocity.clamp_length_max(12.0);
                            self.angular_velocity = self.angular_velocity.clamp_length_max(12.0);
                            self.angular_velocity *= 0.45;
                            break;
                        },
                        Err(()) => {
                            self.rotation = previous;
                            report.budget_limited = true;
                            return report;
                        },
                    }
                }
            }
            if report.grounded {
                // Continuous rolling loss, independent of the number of substeps. The old
                // 0.72 per 120 Hz contact erased ~98% of angular motion in 0.1 seconds,
                // stopping long brass cases before they could roll or make another contact.
                let angular_loss = (-2.8 * h).exp();
                let sliding_loss = (-3.2 * h).exp();
                self.angular_velocity *= angular_loss;
                self.velocity.x *= sliding_loss;
                self.velocity.z *= sliding_loss;
            }
            self.velocity = self.velocity.clamp_length_max(12.0);
            self.angular_velocity = self.angular_velocity.clamp_length_max(12.0);
        }
        let quiet = report.grounded && self.velocity.length_squared() < 0.04
            && self.angular_velocity.length_squared() < 0.25
            && self.position.distance_squared(old_position) < 0.0001
            && self.rotation.angle_between(old_rotation) < 0.02;
        rest.quiet_s = if quiet { rest.quiet_s + dt } else { 0.0 };
        if rest.quiet_s >= 0.25 {
            rest.sleeping = true;
            self.velocity = Vec3::ZERO;
            self.angular_velocity = Vec3::ZERO;
        }
        report
    }

    /// A ram guide requests a pose, not permission to draw through steel. Sweep the whole
    /// finite body along that path and stop at the first contact; no sideways teleport to a seat.
    pub fn constrain_pose(&mut self, position: Vec3, rotation: Quat, triangles: &[[Vec3; 3]]) {
        let from = self.position;
        let start_rotation = self.rotation;
        let angle = start_rotation.angle_between(rotation);
        let steps = (angle * self.half.length() / ROTATION_SURFACE_STEP).ceil().max(1.0) as usize;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let goal = from.lerp(position,t);
            let delta = goal - self.position;
            if let Some(hit) = sweep(self,delta,triangles) {
                self.position += delta * (hit.fraction - SKIN / delta.length().max(SKIN)).max(0.0);
                return;
            }
            self.position = goal;
            let previous = self.rotation;
            self.rotation = start_rotation.slerp(rotation,t).normalize();
            if sweep(self,Vec3::ZERO,triangles).is_some() {
                self.rotation = previous;
                return;
            }
        }
    }

    /// Bounds include every orientation and possible reflected path during this frame.
    pub fn query_bounds(&self, dt: f32, carry: Option<Carry>) -> (Vec3, Vec3) {
        let acceleration = carry.map_or(9.8, |c|
            (c.position - self.position).length() * c.stiffness + self.velocity.length() * c.damping);
        let reach = self.half.length() + self.velocity.length() * dt + acceleration * dt * dt + 0.03;
        (self.position - Vec3::splat(reach), self.position + Vec3::splat(reach))
    }

    fn inverse_inertia(&self, vector: Vec3) -> Vec3 {
        let h2 = self.half * self.half;
        let diagonal = Vec3::new(h2.y+h2.z, h2.x+h2.z, h2.x+h2.y) * (self.mass / 3.0);
        self.rotation * ((self.rotation.conjugate() * vector) / diagonal.max(Vec3::splat(1e-6)))
    }

    fn contact_arm(&self, normal: Vec3) -> Vec3 {
        let local = self.rotation.conjugate() * normal;
        // Face contact uses its centre; an oblique contact retains its finite-length lever arm.
        let support = local.map(|v| if v.abs() < 1e-4 { 0.0 } else { -v.signum() });
        self.rotation * (self.half * support)
    }

    fn impulse(&mut self, normal: Vec3, restitution: f32, report: &mut Report) {
        let arm = self.contact_arm(normal);
        let point_velocity = self.velocity + self.angular_velocity.cross(arm);
        let inward = point_velocity.dot(normal);
        report.grounded |= normal.y > 0.5;
        if -inward > report.impact_speed {
            report.impact_speed = -inward;
            report.tangential_speed = (point_velocity - normal * inward).length();
            report.contact_point = self.position + arm;
        }
        if inward >= 0.0 { return; }
        let effective = 1.0 / self.mass + normal.dot(self.inverse_inertia(arm.cross(normal)).cross(arm));
        let bounce = if inward < -0.7 { restitution } else { 0.0 };
        let magnitude = -(1.0 + bounce) * inward / effective.max(1e-6);
        let impulse = normal * magnitude;
        self.velocity += impulse / self.mass;
        self.angular_velocity += self.inverse_inertia(arm.cross(impulse));
        let relative = self.velocity + self.angular_velocity.cross(arm);
        let tangent = relative - normal * relative.dot(normal);
        if tangent.length_squared() > 1e-8 {
            let direction = tangent.normalize();
            let effective = 1.0 / self.mass + direction.dot(self.inverse_inertia(arm.cross(direction)).cross(arm));
            let friction = -direction * (tangent.length() / effective.max(1e-6)).min(magnitude * 0.42);
            self.velocity += friction / self.mass;
            self.angular_velocity += self.inverse_inertia(arm.cross(friction));
        }
    }

    pub fn advance(&mut self, dt: f32, triangles: &[[Vec3; 3]], carry: Option<Carry>, restitution: f32) -> Report {
        let mut report = Report::default();
        if !dt.is_finite() || dt <= 0.0 { return report; }
        let steps = (dt / (1.0 / 120.0)).ceil().max(1.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            if let Some(target) = carry {
                self.velocity += ((target.position - self.position) * target.stiffness
                    - self.velocity * target.damping) * h;
                let mut difference = target.rotation * self.rotation.conjugate();
                if difference.w < 0.0 { difference = -difference; }
                self.angular_velocity += (difference.to_scaled_axis() * 24.0
                    - self.angular_velocity * 9.0) * h;
            } else { self.velocity.y -= 9.8 * h; }
            let mut remaining = h;
            for _ in 0..6 {
                let delta = self.velocity * remaining;
                let Some(hit) = sweep(self, delta, triangles) else {
                    self.position += delta;
                    break;
                };
                let travel = delta.length();
                let safe = (hit.fraction - SKIN / travel.max(SKIN)).max(0.0);
                self.position += delta * safe;
                // Recover only shallow pre-existing overlaps locally, never snap to a floor or
                // across a wall. A deeply interpenetrating authored start remains an explicit limit.
                self.position += hit.normal * (hit.depth + SKIN).min(0.004);
                self.impulse(hit.normal, restitution, &mut report);
                let inward = self.velocity.dot(hit.normal);
                if inward < 0.0 { self.velocity -= hit.normal * inward; }
                remaining *= 1.0 - hit.fraction;
                if remaining < 1e-6 { break; }
            }
            // Exact finite-shape endpoint overlap, with <=1 mm outer-surface motion between
            // checks. This bounds rotational penetration independently of mouse/frame speed.
            let angle = self.angular_velocity.length() * h;
            let rotations = (angle * self.half.length() / ROTATION_SURFACE_STEP).ceil().max(1.0) as usize;
            let rotation_step = Quat::from_scaled_axis(self.angular_velocity * (h / rotations as f32));
            for _ in 0..rotations {
                let old = self.rotation;
                self.rotation = (rotation_step * old).normalize();
                if let Some(hit) = sweep(self, Vec3::ZERO, triangles) {
                    self.rotation = old;
                    self.impulse(hit.normal, restitution, &mut report);
                    // The attempted spin itself must not keep pushing the long end through steel.
                    let arm = self.contact_arm(hit.normal);
                    let torque_axis = arm.cross(hit.normal);
                    if torque_axis.length_squared() > 1e-10 {
                        let axis = torque_axis.normalize();
                        let into = self.angular_velocity.dot(axis);
                        if into < 0.0 { self.angular_velocity -= axis * into; }
                    }
                    break;
                }
            }
            self.angular_velocity *= (-0.12 * h).exp();
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(x: f32) -> [[Vec3; 3]; 2] {
        let [a,b,c,d] = [Vec3::new(x,-5.0,-5.0),Vec3::new(x,5.0,-5.0),
            Vec3::new(x,5.0,5.0),Vec3::new(x,-5.0,5.0)];
        [[a,b,c],[a,c,d]]
    }
    fn body() -> Body { Body { position: Vec3::ZERO, rotation: Quat::IDENTITY,
        velocity: Vec3::ZERO, angular_velocity: Vec3::ZERO,
        half: Vec3::new(0.071,0.071,0.465), mass: 15.0 } }

    #[test]
    fn fast_held_round_stops_its_long_end_at_thin_wall() {
        let mut b = body();
        b.rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        b.velocity = Vec3::X * 80.0;
        let target = Carry { position: Vec3::X * 4.0, rotation: b.rotation, stiffness: 45.0, damping: 8.0 };
        b.advance(0.12,&wall(0.8),Some(target),0.1);
        assert!(b.position.x + b.half.z <= 0.802, "full body escaped: {b:?}");
        assert!(b.position.x > -0.1, "no opposite-side teleport");
    }

    #[test]
    fn falling_case_contacts_machinery_before_basket_floor() {
        let mut b = body();
        b.position = Vec3::new(0.0,1.0,0.0);
        b.velocity = Vec3::new(0.0,-40.0,0.0);
        let shelf = wall(0.4).map(|t|t.map(|v|Vec3::new(v.y,v.x,v.z)));
        let report = b.advance(0.08,&shelf,None,0.28);
        assert!(report.impact_speed > 10.0);
        assert!(b.position.y - b.half.y >= 0.398, "fell through breech shelf: {b:?}");
    }

    #[test]
    fn rapid_carry_turn_cannot_sweep_nose_through_wall() {
        let mut b = body();
        b.position.x = 0.20;
        b.angular_velocity = Vec3::Y * 60.0;
        let start = b.position;
        let energy_budget = b.mass / 6.0 * (b.half.x*b.half.x + b.half.z*b.half.z) * 60.0*60.0;
        b.advance(0.04,&wall(0.45),None,0.1);
        let extent = (b.rotation * Vec3::X).x.abs()*b.half.x
            +(b.rotation * Vec3::Y).x.abs()*b.half.y+(b.rotation * Vec3::Z).x.abs()*b.half.z;
        assert!(b.position.x + extent <= 0.452, "rotation crossed steel: {b:?}");
        assert!(0.5*b.mass*b.velocity.length_squared() < energy_budget,
            "contact must not manufacture linear energy: {b:?}");
        assert!(b.position.distance(start) <= b.velocity.length()*0.04 + 0.012,
            "position changed beyond actual impulse travel: {b:?}");
    }

    #[test]
    fn oblique_drop_preserves_bounce_and_angular_response() {
        let mut b = body();
        b.position = Vec3::new(0.0,0.8,0.0);
        b.rotation = Quat::from_rotation_x(0.35);
        b.velocity = Vec3::new(0.8,-5.0,0.3);
        let floor = wall(0.0).map(|t|t.map(|v|Vec3::new(v.y,v.x,v.z)));
        let report = b.advance(0.15,&floor,None,0.3);
        assert!(report.grounded && report.impact_speed > 1.0);
        assert!(b.angular_velocity.length()>0.01, "contact must create tumble/roll");
        assert!(b.position.y > 0.0 && b.velocity.is_finite());
    }

    #[test]
    fn narrow_breech_aperture_rejects_finite_width_but_accepts_clear_gap() {
        let b = body();
        let edge = [Vec3::new(0.04,-1.0,0.3),Vec3::new(0.04,1.0,0.3),Vec3::new(1.0,1.0,0.3)];
        assert!(sweep_triangle(&b,Vec3::Z,edge).is_some(), "centre ray misses the rim but body must hit");
        let clear = edge.map(|p|p+Vec3::X*0.2);
        assert!(sweep_triangle(&b,Vec3::Z,clear).is_none());
    }

    #[test]
    fn released_body_settles_and_remains_present_after_old_retirement_time() {
        let mut b = body();
        b.position = Vec3::new(0.0,0.8,0.0);
        b.velocity = Vec3::new(0.7,-0.3,0.4);
        b.angular_velocity = Vec3::new(0.2,0.3,1.0);
        let floor = wall(0.0).map(|t|t.map(|v|Vec3::new(v.y,v.x,v.z)));
        for _ in 0..1_920 { b.advance(1.0/60.0,&floor,None,0.28); }
        assert!(b.position.is_finite() && b.rotation.is_finite());
        assert!(b.position.y > 0.0 && b.position.y < 0.55, "must stay on represented floor: {b:?}");
        assert!(b.velocity.length() < 0.5, "contact friction should settle the body: {b:?}");
    }
}
