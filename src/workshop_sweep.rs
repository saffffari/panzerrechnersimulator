//! Conservative contact admission for a moving source triangle, not a replacement collider.
//! The caller supplies actual source triangles in a shared nearby coordinate frame. For a
//! rigid rotation, the start/end convex hull is enlarged by the arc sagitta. Restricting
//! maximum corner travel bounds the conservative envelope; this does not simulate contact
//! forces, deformation, or a freely moving rigid body. Invalid input fails closed.
use glam::{DVec3, Vec3};

pub const MAX_CORNER_STEP_M: f64 = 0.001;

#[derive(Clone, Copy, Debug)]
pub struct RotationSteps {
    pub count: usize,
    pub angle_per_step: f64,
    pub max_corner_travel_m: f64,
    pub arc_padding_m: f64,
}

/// Source radius is the farthest actual moving vertex from its measured hinge axis.
/// A bounded admission budget fails closed instead of silently taking a coarse step.
pub fn rotation_steps(
    source_radius_m: f64,
    angle_rad: f64,
    max_steps: usize,
) -> Option<RotationSteps> {
    if !source_radius_m.is_finite()
        || source_radius_m < 0.0
        || !angle_rad.is_finite()
        || angle_rad.abs() > std::f64::consts::PI
        || max_steps == 0
    {
        return None;
    }
    let required = (source_radius_m * angle_rad.abs() / MAX_CORNER_STEP_M)
        .ceil()
        .max(1.0);
    if !required.is_finite() || required > max_steps as f64 {
        return None;
    }
    let count = required as usize;
    if count > max_steps {
        return None;
    }
    let step = angle_rad / count as f64;
    let max_corner_travel_m = source_radius_m * (2.0 * (step.abs() * 0.5).sin());
    // This form does not cancel to zero for small, nonzero angles.
    let arc_padding_m = source_radius_m * (2.0 * (step * 0.25).sin().powi(2));
    if !max_corner_travel_m.is_finite()
        || !arc_padding_m.is_finite()
        || max_corner_travel_m > MAX_CORNER_STEP_M * (1.0 + 16.0 * f64::EPSILON)
    {
        return None;
    }
    Some(RotationSteps {
        count,
        angle_per_step: step,
        max_corner_travel_m,
        arc_padding_m,
    })
}

// Relative-coordinate subtraction and three-product projections also round in f64.
// The floor handles sub-metre local scenes; this is arithmetic uncertainty, not source
// fitting clearance. Caller padding still owns f32 source/world-transform uncertainty.
fn projection_roundoff(points: impl Iterator<Item = DVec3>) -> f64 {
    64.0 * f64::EPSILON * points.fold(1.0_f64, |m, p| m.max(p.abs().max_element()))
}

/// SAT between the convex hull of six source corner endpoints and one source obstacle
/// triangle. All hull face normals and edge directions are present in the enumerated triples
/// and pairs; redundant axes only add work. This includes edge/face contacts missed by three
/// vertex rays. `padding_m` must include the rotational sagitta and caller's numeric bound.
pub fn triangle_sweep_blocked(
    start: [Vec3; 3],
    end: [Vec3; 3],
    obstacle: [Vec3; 3],
    padding_m: f64,
) -> bool {
    if start
        .iter()
        .chain(&end)
        .chain(&obstacle)
        .any(|p| !p.is_finite())
        || !padding_m.is_finite()
        || padding_m < 0.0
    {
        return true;
    }
    // Subtraction in double precision preserves the supplied f32 coordinates without adding
    // large world-position projection error. It cannot undo prior world-transform rounding.
    let origin = start[0].as_dvec3();
    let hull =
        [start[0], start[1], start[2], end[0], end[1], end[2]].map(|p| p.as_dvec3() - origin);
    let tri = obstacle.map(|p| p.as_dvec3() - origin);
    let roundoff = projection_roundoff(hull.into_iter().chain(tri));
    let separate = |axis: DVec3| {
        let length = axis.length();
        if length < 1e-14 {
            return false;
        }
        let h = hull.map(|p| p.dot(axis));
        let t = tri.map(|p| p.dot(axis));
        let hlo = h.into_iter().fold(f64::INFINITY, f64::min);
        let hhi = h.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let tlo = t.into_iter().fold(f64::INFINITY, f64::min);
        let thi = t.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let margin = (padding_m + roundoff) * length;
        hhi + margin < tlo || thi + margin < hlo
    };
    // World/local box axes cheaply reject most unrelated candidates, including degenerate
    // zero-area inputs, before the complete convex separating-axis set is considered.
    if [DVec3::X, DVec3::Y, DVec3::Z].into_iter().any(separate) {
        return false;
    }
    let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
    if separate(normal) {
        return false;
    }
    // Coplanar zero-thickness triangles also need in-plane edge normals; their ordinary
    // edge/edge cross products alone all point out of plane.
    for i in 0..3 {
        if separate(normal.cross(tri[(i + 1) % 3] - tri[i])) {
            return false;
        }
    }
    for a in 0..6 {
        for b in a + 1..6 {
            let edge = hull[b] - hull[a];
            if separate(normal.cross(edge)) {
                return false;
            }
            for c in b + 1..6 {
                if separate(edge.cross(hull[c] - hull[a])) {
                    return false;
                }
            }
            for i in 0..3 {
                if separate(edge.cross(tri[(i + 1) % 3] - tri[i])) {
                    return false;
                }
            }
        }
    }
    true
}

/// Separate service-motion admission for an ALREADY fitted/touching source pair.
/// `obstacle` winding must give its verified outward normal. The actual fixed hinge and
/// axis define the entire rigid path, not just its two endpoint poses. `source_error_m`
/// is the measured maximum signed-plane clearance uncertainty for this source pair,
/// including any prior f32 transforms; it is not an arbitrary allowed penetration depth.
///
/// The whole moving triangle must begin outside a support plane of the actual finite
/// obstacle triangle within the measured uncertainty band. The face's outward plane is
/// tried first. Cardinal support planes also handle departure from a seated finite edge;
/// they may not face into the obstacle's verified outward normal. Initially touching
/// vertices must move strictly outward throughout the stroke; initially clear vertices
/// must remain provably clear. Tangent, entering, crossing, degenerate and unresolved cases
/// fail closed. Every obstacle triangle is still checked; this exempts no solid or root.
pub fn rotation_contact_separates(
    start: [Vec3; 3],
    obstacle: [Vec3; 3],
    hinge: Vec3,
    axis: Vec3,
    angle_rad: f64,
    source_error_m: f64,
) -> bool {
    if start.iter().chain(&obstacle).any(|p| !p.is_finite())
        || !hinge.is_finite()
        || !axis.is_finite()
        || !angle_rad.is_finite()
        || angle_rad == 0.0
        || angle_rad.abs() > std::f64::consts::PI
        || !source_error_m.is_finite()
        || source_error_m < 0.0
    {
        return false;
    }
    let pivot = hinge.as_dvec3();
    let a = start.map(|p| p.as_dvec3() - pivot);
    let b = obstacle.map(|p| p.as_dvec3() - pivot);
    let k = axis.as_dvec3();
    let klen = k.length();
    if klen < 1e-14 {
        return false;
    }
    let k = k / klen;
    let e1 = obstacle[1].as_dvec3() - obstacle[0].as_dvec3();
    let e2 = obstacle[2].as_dvec3() - obstacle[0].as_dvec3();
    let normal = e1.cross(e2);
    let nlen = normal.length();
    if nlen < 1e-14 || nlen <= 64.0 * f64::EPSILON * e1.length() * e2.length() {
        return false;
    }
    let n = normal / nlen;
    let projection_error = projection_roundoff(a.into_iter().chain(b));
    // An arbitrary computed axis suffices for SAT, but this exception needs the actual
    // outward plane. Cross-product conditioning can amplify normal error on sliver faces.
    // Bound its effect on distances and velocities, and reject a plane whose uncertainty
    // already exceeds the caller's measured source-error scale plus ordinary roundoff.
    let normal_error = 16.0 * f64::EPSILON * e1.length() * e2.length() / nlen;
    let reach = a.into_iter().map(|p| p.length()).fold(0.0_f64, f64::max) + b[0].length();
    let normal_distance_error = normal_error * reach;
    if normal_distance_error > source_error_m + projection_error {
        return false;
    }
    if leaves_plane(a, b[0], k, n, angle_rad, source_error_m,
        projection_error, normal_error, normal_distance_error) {return true;}
    // A seated chamfer edge is finite: the moving face can straddle its infinite plane
    // while lying wholly above its highest corner. The support point comes from that
    // exact triangle, never a body AABB or guessed clearance. The analytic interval proof
    // below covers every moving corner for the entire curved stroke, not just endpoints.
    for support_normal in [DVec3::X,-DVec3::X,DVec3::Y,-DVec3::Y,DVec3::Z,-DVec3::Z] {
        if support_normal.dot(n)<0.0 {continue;}
        let support=*b.iter().max_by(|u,v|u.dot(support_normal).total_cmp(&v.dot(support_normal))).unwrap();
        if leaves_plane(a,support,k,support_normal,angle_rad,source_error_m,
            projection_error,0.0,0.0) {return true;}
    }
    false
}

/// The support halfspace contains every vertex of the finite obstacle triangle. A rigid
/// triangle is the convex combination of its three rotating corners, so their exact
/// trigonometric interval extrema prove separation for its complete face as well.
fn leaves_plane(a:[DVec3;3],support:DVec3,k:DVec3,n:DVec3,angle_rad:f64,
    source_error_m:f64,projection_error:f64,normal_error:f64,normal_distance_error:f64)->bool {
    let arithmetic=projection_error+normal_distance_error;
    let tolerance=source_error_m+arithmetic;
    if !tolerance.is_finite(){return false;}
    let mut has_initial_contact = false;
    for p in a {
        let d0=(p-support).dot(n);
        if d0 < -tolerance {
            return false;
        }
        let radial = p - k * p.dot(k);
        // d(theta)=d0+A(cos(theta)-1)+B sin(theta). The stable evaluator
        // below is algebraically c+A cos(theta)+B sin(theta), c=d0-A.
        let ca = radial.dot(n);
        let cb = k.cross(radial).dot(n);
        let (minimum, _) = trig_range(d0, ca, cb, angle_rad);
        if !minimum.is_finite() || minimum < -tolerance {
            return false;
        }
        if d0 <= tolerance {
            has_initial_contact = true;
            // Derivative with respect to normalized stroke time, including sign of turn.
            let va = angle_rad * cb;
            let vb = -angle_rad * ca;
            let (minimum_velocity, _) = trig_range(va, va, vb, angle_rad);
            let velocity_error = 64.0 * f64::EPSILON * angle_rad.abs() * radial.length().max(1.0)
                + normal_error * angle_rad.abs() * radial.length();
            if !minimum_velocity.is_finite() || minimum_velocity <= velocity_error {
                return false;
            }
        } else if minimum <= tolerance + arithmetic {
            return false;
        }
    }
    has_initial_contact
}

// Exact candidate extrema of c+a*cos(theta)+b*sin(theta) over the finite interval.
// `at_zero` stores c+a directly, avoiding cancellation at the fitted starting plane.
fn trig_range(at_zero: f64, a: f64, b: f64, angle: f64) -> (f64, f64) {
    let value = |theta: f64| at_zero - 2.0 * a * (theta * 0.5).sin().powi(2) + b * theta.sin();
    let end = value(angle);
    let mut low = at_zero.min(end);
    let mut high = at_zero.max(end);
    let lo = angle.min(0.0);
    let hi = angle.max(0.0);
    let phase = b.atan2(a);
    for turn in -2..=2 {
        let theta = phase + turn as f64 * std::f64::consts::PI;
        if theta >= lo && theta <= hi {
            let v = value(theta);
            low = low.min(v);
            high = high.max(v);
        }
    }
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thin_obstacle_inside_face_sweep_is_detected_without_a_vertex_hit() {
        let a = [
            Vec3::new(-1.0, -1.0, -0.5),
            Vec3::new(1.0, -1.0, -0.5),
            Vec3::new(0.0, 1.0, -0.5),
        ];
        let b = a.map(|p| p + Vec3::Z);
        // All three corner trajectories are far from this tiny triangle. The moving face
        // nevertheless crosses it midway through its stroke.
        let obstacle = [
            Vec3::new(-0.01, -0.01, 0.0),
            Vec3::new(0.01, -0.01, 0.0),
            Vec3::new(0.0, 0.01, 0.0),
        ];
        assert!(triangle_sweep_blocked(a, b, obstacle, 0.0));
        assert!(triangle_sweep_blocked(b, a, obstacle, 0.0));
        assert!(!triangle_sweep_blocked(
            a,
            b,
            obstacle.map(|p| p + Vec3::X * 2.0),
            0.0
        ));
    }
    #[test]
    fn close_source_clearance_is_not_replaced_with_a_large_proxy() {
        let a = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let b = a.map(|p| p + Vec3::Z * 0.001);
        let clear = a.map(|p| p + Vec3::Z * 0.0014);
        assert!(!triangle_sweep_blocked(a, b, clear, 0.0001));
        assert!(triangle_sweep_blocked(a, b, clear, 0.0005));
        assert!(triangle_sweep_blocked(a, b, b, 0.0));
        let coplanar = [
            Vec3::new(0.6, 0.6, 0.0),
            Vec3::new(1.2, 0.6, 0.0),
            Vec3::new(0.6, 1.2, 0.0),
        ];
        assert!(
            !triangle_sweep_blocked(a, a, coplanar, 0.0),
            "overlapping bounds are not triangle contact"
        );
    }
    #[test]
    fn hinge_subdivision_bounds_curved_corner_paths_and_rejects_over_budget() {
        let radius = 1.4;
        let angle = -0.02;
        let steps = rotation_steps(radius, angle, 64).unwrap();
        assert!(steps.max_corner_travel_m <= MAX_CORNER_STEP_M);
        assert!((steps.angle_per_step * steps.count as f64 - angle).abs() < 1e-12);
        // Actual circular midpoint lies outside its chord by exactly the declared sagitta.
        let half = steps.angle_per_step * 0.5;
        let midpoint = DVec3::new(radius * half.cos(), radius * half.sin(), 0.0);
        let chord = (DVec3::X * radius
            + DVec3::new(
                radius * steps.angle_per_step.cos(),
                radius * steps.angle_per_step.sin(),
                0.0,
            ))
            * 0.5;
        assert!((midpoint.distance(chord) - steps.arc_padding_m).abs() < 1e-12);
        assert!(rotation_steps(radius, angle, 2).is_none());
        assert!(rotation_steps(f64::NAN, angle, 64).is_none());
        assert!(triangle_sweep_blocked(
            [Vec3::NAN; 3],
            [Vec3::ZERO; 3],
            [Vec3::ZERO; 3],
            0.0
        ));
    }
    #[test]
    fn exact_midpoint_contact_survives_projection_roundoff() {
        // All values are exact f32 dyadic fractions. p is exactly the midpoint of b/c,
        // hence the obstacle touches that edge. Unguarded f64 projections produced an
        // apparent 4.8e-18 m separating gap on this nondegenerate hull-face axis.
        let scale = 16_777_216.0;
        let b = Vec3::new(-9_603_476.0, 1_048_454.0, -6_151_908.0) / scale;
        let c = Vec3::new(-15_234_764.0, 1_806_702.0, 8_634_852.0) / scale;
        let p = Vec3::new(-12_419_120.0, 1_427_578.0, 1_241_472.0) / scale;
        assert_eq!(p.as_dvec3(), (b.as_dvec3() + c.as_dvec3()) * 0.5);
        let moving = [Vec3::ZERO, b, c];
        let fixed = [p, p + Vec3::X * 0.25, p + Vec3::Y * 0.25];
        assert!(triangle_sweep_blocked(moving, moving, fixed, 0.0));
    }
    #[test]
    fn tiny_rotation_keeps_its_positive_sagitta_and_extreme_inputs_fail_closed() {
        let step = rotation_steps(1.4, 1e-10, 1).unwrap();
        assert!(step.arc_padding_m > 0.0);
        assert!((step.arc_padding_m / (1.4 * 1e-20 / 8.0) - 1.0).abs() < 1e-12);
        assert!(rotation_steps(1e308, 1.0, usize::MAX).is_none());
        let stationary = rotation_steps(1e308, 0.0, 1).unwrap();
        assert_eq!(stationary.max_corner_travel_m, 0.0);
        assert_eq!(stationary.arc_padding_m, 0.0);
    }
    fn resting_pair() -> ([Vec3; 3], [Vec3; 3]) {
        (
            [
                Vec3::new(0.5, -0.5, 0.0),
                Vec3::new(1.0, -0.5, 0.0),
                Vec3::new(0.5, 0.5, 0.0),
            ],
            [
                Vec3::new(-3.0, -3.0, 0.0),
                Vec3::new(3.0, -3.0, 0.0),
                Vec3::new(0.0, 3.0, 0.0),
            ],
        )
    }
    #[test]
    fn fitted_contact_opens_only_toward_the_verified_outward_side() {
        let (moving, fixed) = resting_pair();
        assert!(triangle_sweep_blocked(moving, moving, fixed, 0.0));
        assert!(rotation_contact_separates(
            moving,
            fixed,
            Vec3::ZERO,
            Vec3::Y,
            -0.25,
            0.0
        ));
        assert!(rotation_contact_separates(
            moving,
            fixed,
            Vec3::ZERO,
            -Vec3::Y,
            0.25,
            0.0
        ));
        assert!(!rotation_contact_separates(
            moving,
            fixed,
            Vec3::ZERO,
            Vec3::Y,
            0.25,
            0.0
        ));
        assert!(!rotation_contact_separates(
            moving,
            fixed,
            Vec3::ZERO,
            Vec3::Z,
            0.25,
            0.0
        ));
        assert!(!rotation_contact_separates(
            moving,
            fixed,
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            0.0
        ));
        assert!(!rotation_contact_separates(
            moving,
            [fixed[0], fixed[2], fixed[1]],
            Vec3::ZERO,
            Vec3::Y,
            -0.25,
            0.0
        ));
    }
    #[test]
    fn measured_fitted_error_is_not_permission_for_deeper_or_new_contact() {
        let (moving, fixed) = resting_pair();
        let shallow = moving.map(|p| p - Vec3::Z * 0.00000026);
        assert!(rotation_contact_separates(
            shallow,
            fixed,
            Vec3::ZERO,
            Vec3::Y,
            -0.25,
            0.00000030
        ));
        assert!(!rotation_contact_separates(
            shallow,
            fixed,
            Vec3::ZERO,
            Vec3::Y,
            -0.25,
            0.00000010
        ));
        assert!(!rotation_contact_separates(
            shallow,
            fixed,
            Vec3::ZERO,
            Vec3::Y,
            0.25,
            0.00000030
        ));
        let clear = moving.map(|p| p + Vec3::Z * 0.01);
        assert!(!rotation_contact_separates(
            clear,
            fixed,
            Vec3::ZERO,
            Vec3::Y,
            -0.25,
            0.0
        ));
        assert!(!rotation_contact_separates(
            moving,
            [Vec3::ZERO; 3],
            Vec3::ZERO,
            Vec3::Y,
            -0.25,
            0.0
        ));
    }
    #[test]
    fn interval_extrema_reject_outward_then_returning_motion() {
        let (moving, fixed) = resting_pair();
        // Endpoints alone look outward: sin(2.4)>0. The latter part is returning toward
        // the fitted plane, so it is not admitted by the separating-contact exception.
        assert!(!rotation_contact_separates(
            moving,
            fixed,
            Vec3::ZERO,
            Vec3::Y,
            -2.4,
            0.0
        ));
        // A clear start/end can hide a crossing halfway through a circular path.
        let (low, high) = trig_range(1.0, 0.0, -2.0, std::f64::consts::PI);
        assert!((low + 1.0).abs() < 1e-12);
        assert!((high - 1.0).abs() < 1e-12);
    }
    #[test]
    fn original_closed_stop_allows_reverse_separation_but_not_overclosure() {
        let (closed,fixed)=resting_pair();
        let open=closed.map(|p|glam::Quat::from_rotation_y(-0.2)*p);
        assert!(closed.iter().all(|p|p.z==0.0));
        for sample in 1..=20 {let angle=-0.2*sample as f32/20.0;
            assert!(closed.iter().all(|p|(glam::Quat::from_rotation_y(angle)**p).z>0.0));}
        assert!(triangle_sweep_blocked(open,closed,fixed,1e-7));
        assert!(!rotation_contact_separates(open,fixed,Vec3::ZERO,Vec3::Y,0.2,1e-7));
        assert!(rotation_contact_separates(closed,fixed,Vec3::ZERO,Vec3::Y,-0.2,1e-7));
        let over=closed.map(|p|glam::Quat::from_rotation_y(0.01)*p);
        assert!(triangle_sweep_blocked(open,over,fixed,1e-7));
        assert!(!rotation_contact_separates(over,fixed,Vec3::ZERO,Vec3::Y,-0.21,1e-7));
    }
    #[test]
    fn uncertain_sliver_plane_cannot_grant_a_contact_exception() {
        let (moving, _) = resting_pair();
        let sliver = [Vec3::ZERO, Vec3::X, Vec3::new(2.0, 1e-12, 0.0)];
        assert!(!rotation_contact_separates(
            moving,
            sliver,
            Vec3::ZERO,
            Vec3::Y,
            -0.25,
            0.0
        ));
        assert!(!rotation_contact_separates(
            moving,
            sliver,
            Vec3::ZERO,
            Vec3::Y,
            -0.25,
            1e-6
        ));
    }
    fn deck_chamfer_pair()->([Vec3;3],[Vec3;3],Vec3) {
        // Build34 source receipt: leaf triangle441 and Hull_0708 triangle25631.
        ([Vec3::new(0.4953767061,1.9542763233,-1.3491114378),
          Vec3::new(0.4964396060,1.9747091532,-1.3412079811),
          Vec3::new(0.4959439635,1.9542763233,-1.3425841331)],
         [Vec3::new(0.5682292581,1.9542765617,-1.4152748585),
          Vec3::new(0.4903896451,1.9153566360,-1.3374352455),
          Vec3::new(0.4903896451,1.9542765617,-1.3374352455)],
         Vec3::new(0.0,2.0087640285,-2.6276273727))
    }
    #[test]
    fn finite_chamfer_edge_separates_over_whole_curved_source_stroke() {
        let(a,b,hinge)=deck_chamfer_pair();let angle=-0.000747998311583485;
        let end=a.map(|p|hinge+glam::Quat::from_axis_angle(Vec3::X,angle as f32)*(p-hinge));
        let error=32.0*f32::EPSILON as f64*10.0;
        assert!(triangle_sweep_blocked(a,end,b,error));
        assert!(rotation_contact_separates(a,b,hinge,Vec3::X,angle,error));
        // The first two checks fail under the old infinite-face-only exception. The
        // source is initially within0.24micrometres; every interior pose departs upward.
        for sample in 1..=32 {
            let q=glam::Quat::from_axis_angle(Vec3::X,angle as f32*sample as f32/32.0);
            assert!(a.iter().all(|&p|(hinge+q*(p-hinge)).y>b.iter().map(|v|v.y).fold(f32::NEG_INFINITY,f32::max)));
        }
        assert!(!rotation_contact_separates(a,b,hinge,Vec3::X,-angle,error),"moving into the seat is blocked");
        assert!(!rotation_contact_separates(a.map(|p|p-Vec3::Y*0.0002),b,hinge,Vec3::X,angle,error),
            "finite-edge proof does not forgive deeper initial overlap");
        assert!(!rotation_contact_separates(a,b,hinge,Vec3::X,-2.4,error),
            "departing then returning within the interval is not a separating stroke");
    }
    #[test]
    fn departing_one_edge_does_not_waive_a_new_face_collision() {
        let(a,b,hinge)=deck_chamfer_pair();let angle=-0.000747998311583485;
        let error=32.0*f32::EPSILON as f64*10.0;
        let end=a.map(|p|hinge+glam::Quat::from_axis_angle(Vec3::X,angle as f32)*(p-hinge));
        let obstacle=[Vec3::new(0.48,1.9548,-1.36),Vec3::new(0.52,1.9548,-1.36),Vec3::new(0.50,1.9548,-1.32)];
        assert!(rotation_contact_separates(a,b,hinge,Vec3::X,angle,error));
        assert!(triangle_sweep_blocked(a,end,obstacle,error));
        assert!(!rotation_contact_separates(a,obstacle,hinge,Vec3::X,angle,error));
    }
}
