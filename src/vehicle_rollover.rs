//! Bounded support for a vehicle whose track-up axis crossed the horizon.
//!
//! The ordinary ride is a three-axis sprung hull, not a six-axis crash solver. Continuing its
//! track/belly springs after inversion puts the visible roof through terrain and still allows
//! powered travel. Source vertices supply the contact polygon; an unsupported centre of mass
//! tips about its nearest support edge under gravity. No resting angle is prescribed.
//! This remains a bounded, high-friction settling model, not a six-axis crash solver: heading
//! is retained, impacts are inelastic, and sliding, gun deformation and recovery are not solved.

use glam::{Mat3, Mat4, Vec2, Vec3};
use std::sync::OnceLock;
use crate::tank::{EnemyKind, Tank};
use crate::source_point_support::{SourcePointBvh, QueryStats};

/// Retain exact source vertices, separated by articulation. Cached only on the unusual rolled
/// path; normal driving neither loads another mesh nor scans its vertices.
struct BodySource { parts: [Vec<Vec3>; 2], bounds:[Option<SourcePointBvh>;2], max_triangle_edge_m: f32 }
static SOURCES: [OnceLock<BodySource>; 7] = [const { OnceLock::new() }; 7];

fn body_source(tank: &Tank) -> &'static BodySource {
    use crate::vehicle_spec::Model;
    let tiger = crate::vehicle_spec::for_tank(tank.kind, &tank.parts).model == Model::TigerI;
    let index = if tiger { 0 } else { match tank.kind {
        EnemyKind::T34 => 1, EnemyKind::Su100 => 2, EnemyKind::Kv2 => 3,
        EnemyKind::Isu152 => 4, EnemyKind::Is2 => 5, EnemyKind::Is3 => 6,
    }};
    SOURCES[index].get_or_init(|| {
        // These are precisely the same source factories used for the drawn vehicles. The few
        // code-native enemy types keep their drawn geometry, not a new collision-only box.
        let meshes = if tiger { crate::tank::build_tiger() } else { match tank.kind {
            EnemyKind::T34 => crate::tank::build_t34(), EnemyKind::Su100 => crate::tank::build_su100(),
            EnemyKind::Kv2 => crate::tank::build_kv2(), EnemyKind::Isu152 => crate::tank::build_isu152(),
            EnemyKind::Is2 => crate::tank::build_is2(), EnemyKind::Is3 => crate::tank::build_is3(),
        }};
        let max_triangle_edge_m = meshes[..2].iter().flat_map(|mesh| {
            mesh.indices.chunks_exact(3).map(move |tri| {
                let p: [Vec3; 3] = std::array::from_fn(|i|
                    Vec3::from_array(mesh.vertices[tri[i] as usize].pos));
                p[0].distance(p[1]).max(p[1].distance(p[2])).max(p[2].distance(p[0]))
            })
        }).filter(|v| v.is_finite()).fold(0.0_f32, f32::max);
        let parts:[Vec<Vec3>;2]=std::array::from_fn(|part| {
            let mut seen = std::collections::HashSet::new();
            meshes[part].vertices.iter().filter_map(|v| {
                let p = Vec3::from_array(v.pos);
                (p.is_finite() && seen.insert(v.pos.map(f32::to_bits))).then_some(p)
            }).collect()
        });
        let bounds=parts.each_ref().map(|points|SourcePointBvh::new(points));
        BodySource {parts,bounds,max_triangle_edge_m}
    })
}

fn required_origin_height(parts: &[Vec<Vec3>; 2], models: [Mat4; 2], origin_y: f32, turret_attached: bool,
    height_at: impl Fn(f32,f32)->f32) -> Option<f32> {
    let mut required = f32::NEG_INFINITY;
    for (index, (points, model)) in parts.iter().zip(models).enumerate() {
        if index == 1 && !turret_attached { continue; }
        for &point in points {
            let world = model.transform_point3(point);
            let support = height_at(world.x, world.z);
            if world.is_finite() && support.is_finite() {
                required = required.max(support - (world.y - origin_y));
            }
        }
    }
    required.is_finite().then_some(required)
}

/// Vertex-sampled contact with the same support surface the caller uses for ordinary wheels.
/// Exact on a plane. Sub-vertex terrain peaks can still be missed; no triangle/terrain CCD claim.
/// Barrel is excluded because its deformation and digging contact are not modeled.
pub(crate) fn rolled_support_height(tank: &Tank, height_at: impl Fn(f32,f32)->f32) -> Option<f32> {
    rolled_support_height_for_parts(tank, !tank.kind.casemate(), height_at)
}

/// Detached turrets are independent wreck bodies. Their old mounted pose must not provide an
/// invisible support beneath the inverted hull; Game supplies its authoritative wreck membership.
pub(crate) fn rolled_support_height_for_parts(tank: &Tank, turret_attached: bool,
    height_at: impl Fn(f32,f32)->f32) -> Option<f32> {
    required_origin_height(&body_source(tank).parts, [tank.hull_model(), tank.turret_model()],
        tank.pos.y, turret_attached, height_at)
}

// Numerical contact band, not extra collision geometry. End poses are lifted to the actual
// sampled surface. Angular steps move the most distant source point at most 10 mm; contact
// crossings are then bisected twelve times (<= 2.5 micrometres of that step).
const CONTACT_M: f32 = 0.001;
const CORNER_STEP_M: f32 = 0.01;
const STEP_S: f32 = 1.0 / 120.0;
const CONTACT_ANGULAR_DAMPING: f32 = 1.5; // s^-1, dissipative engineering estimate

struct Support {
    height: f32,
    // World-oriented offsets from the hull origin, avoiding large-map cancellation in the
    // support-polygon test. Y remains the actual source point, never a synthetic footprint.
    contacts: Vec<Vec3>,
    radius: f32,
    stats: QueryStats,
}

fn orientation(tank: &Tank) -> Mat3 {
    Mat3::from_rotation_y(tank.heading) * Mat3::from_rotation_x(-tank.pitch)
        * Mat3::from_rotation_z(tank.roll)
}

fn source_models(tank:&Tank)->[Mat4;2] {
    let rotation=Mat4::from_mat3(orientation(tank));
    let turret = if tank.kind.casemate() { Mat4::IDENTITY } else {
        Mat4::from_translation(tank.parts.turret_pivot) * Mat4::from_rotation_y(tank.turret_yaw)
    };
    [rotation,rotation*turret]
}

fn support_at(tank: &Tank, parts: &[Vec<Vec3>; 2], attached: bool,
    height_at: &impl Fn(f32,f32)->f32) -> Option<Support> {
    let models=source_models(tank);
    let mut highest = f32::NEG_INFINITY;
    let mut contacts: Vec<(f32,Vec3)> = Vec::new();
    let mut radius = 0.0_f32;
    let mut stats=QueryStats::default();
    for (part, points) in parts.iter().enumerate() {
        if part == 1 && !attached { continue; }
        for &point in points {
            let p=models[part].transform_point3(point);stats.tested_points+=1;
            let h = height_at(tank.pos.x+p.x, tank.pos.z+p.z)-p.y;
            if !p.is_finite() || !h.is_finite() { continue; }
            radius = radius.max(p.length());
            if h > highest {
                highest = h;
                contacts.retain(|&(old, _)| highest-old<=CONTACT_M);
            }
            if highest-h<=CONTACT_M { contacts.push((h,p)); }
        }
    }
    highest.is_finite().then(|| Support {
        height: highest, contacts: contacts.into_iter().map(|(_,p)|p).collect(), radius,stats,
    })
}

fn support_at_bounded(tank:&Tank,parts:&[Vec<Vec3>;2],bounds:&[Option<SourcePointBvh>;2],
    attached:bool,height_at:&impl Fn(f32,f32)->f32,upper:&impl Fn(Vec2,Vec2)->f32)->Option<Support> {
    let models=source_models(tank);let mut highest=f32::NEG_INFINITY;let mut contacts=Vec::new();
    let mut radius=0.0_f32;let mut stats=QueryStats::default();
    for part in 0..2 {
        if part==1&&!attached {continue;}
        let Some(index)=bounds[part].as_ref() else {continue;};
        let Some(query)=index.query(&parts[part],models[part],Vec2::new(tank.pos.x,tank.pos.z),
            CONTACT_M,height_at,upper) else {continue;};
        highest=highest.max(query.required_origin_y);radius=radius.max(query.radius_upper_m);
        contacts.extend(query.contacts);
        stats.tested_points+=query.stats.tested_points;stats.bound_queries+=query.stats.bound_queries;
        stats.visited_nodes+=query.stats.visited_nodes;stats.pruned_nodes+=query.stats.pruned_nodes;
    }
    highest.is_finite().then(||Support {height:highest,radius,stats,
        contacts:contacts.into_iter().filter(|c|highest-c.required_origin_y<=CONTACT_M).map(|c|c.offset).collect()})
}

fn cross_xz(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    (b.x-a.x)*(c.z-a.z)-(b.z-a.z)*(c.x-a.x)
}

/// Convex hull of measured contact points. A point or line is not declared stable merely
/// because its velocity is small; the gravity projection must lie in a nonzero-area polygon.
pub(crate) fn support_polygon(points: &[Vec3]) -> Vec<Vec3> {
    let mut sorted: Vec<_> = points.iter().copied().filter(|p|p.is_finite()).collect();
    sorted.sort_by(|a,b| a.x.total_cmp(&b.x).then(a.z.total_cmp(&b.z)));
    sorted.dedup_by(|a,b| a.x == b.x && a.z == b.z);
    if sorted.len() <= 2 { return sorted; }
    let mut hull = Vec::new();
    for &p in &sorted {
        while hull.len() >= 2 && cross_xz(hull[hull.len()-2],hull[hull.len()-1],p) <= 0.0 { hull.pop(); }
        hull.push(p);
    }
    let lower = hull.len();
    for &p in sorted.iter().rev().skip(1) {
        while hull.len() > lower && cross_xz(hull[hull.len()-2],hull[hull.len()-1],p) <= 0.0 { hull.pop(); }
        hull.push(p);
    }
    hull.pop();
    hull
}

pub(crate) fn stable_support(polygon: &[Vec3], com: Vec3) -> bool {
    com.is_finite() && polygon.len() >= 3 && (0..polygon.len()).all(|i|
        cross_xz(polygon[i],polygon[(i+1)%polygon.len()],com) > 0.0)
}

pub(crate) fn support_pivot(polygon: &[Vec3], com: Vec3) -> Option<Vec3> {
    if polygon.len() == 1 { return Some(polygon[0]); }
    (0..polygon.len()).map(|i| {
        let a=polygon[i]; let b=polygon[(i+1)%polygon.len()];
        let d=Vec2::new(b.x-a.x,b.z-a.z);
        let t=(Vec2::new(com.x-a.x,com.z-a.z).dot(d)/d.length_squared().max(1e-20)).clamp(0.0,1.0);
        a+(b-a)*t
    }).min_by(|a,b| {
        Vec2::new(a.x-com.x,a.z-com.z).length_squared()
            .total_cmp(&Vec2::new(b.x-com.x,b.z-com.z).length_squared())
    })
}

/// Same source-vertex support as before, now with gravitational angular settling. The existing
/// vehicle specification supplies its explicitly estimated COM and uniform-box inertia; these
/// are not measured mass properties, and the COM estimate does not change after turret loss.
/// Contact follows actual attached geometry. Terrain peaks between source vertices can still
/// be missed: max_triangle_edge_m records the source sampling gap, not a vertical error bound.
pub(crate) fn advance_rolled_hull_on(tank: &mut Tank, dt: f32, turret_attached: bool,
    height_at: impl Fn(f32,f32)->f32) {
    let source=body_source(tank);
    advance_source(tank,&source.parts,dt,turret_attached,&height_at);
}

/// Same source points and contact-band query, pruned only by caller-certified rectangle height
/// bounds. Nonfinite bounds disable pruning. The transformed source AABB bounds angular travel;
/// it never supplies a contact point or collision surface. No persisted state is added.
pub(crate) fn advance_rolled_hull_bounded(tank:&mut Tank,dt:f32,turret_attached:bool,
    height_at:impl Fn(f32,f32)->f32,upper:impl Fn(Vec2,Vec2)->f32) {
    let source=body_source(tank);
    advance_query(tank,dt,|tank|support_at_bounded(tank,&source.parts,&source.bounds,
        turret_attached,&height_at,&upper));
}

/// Largest edge of the cached rendered hull/turret triangles. This bounds source sampling
/// spacing only; a terrain peak between samples still needs a terrain-gradient error bound.
pub(crate) fn rolled_source_sample_gap_m(tank: &Tank) -> f32 {
    body_source(tank).max_triangle_edge_m
}

fn advance_source(tank: &mut Tank, parts: &[Vec<Vec3>;2], dt: f32, attached: bool,
    height_at: &impl Fn(f32,f32)->f32) {
    advance_query(tank,dt,|tank|support_at(tank,parts,attached,height_at));
}

fn advance_query(tank:&mut Tank,dt:f32,query:impl Fn(&Tank)->Option<Support>) {
    let dt=if dt.is_finite() {dt.clamp(0.0,0.1)} else {0.0};
    let spec=crate::vehicle_spec::for_tank(tank.kind,&tank.parts);
    let cg=Vec3::Y*spec.cg_height_m;
    let [length,width,height]=spec.hull_lwh_m;
    // Inertia per kg; mass cancels against gravitational torque and the parallel-axis term.
    let inertia=Mat3::from_diagonal(Vec3::new(length*length+height*height,
        length*length+width*width,width*width+height*height)/12.0);
    tank.speed=0.0;
    tank.ride.overturned=true;
    tank.ride.active=true;
    let mut remaining=dt;
    // Zero dt is a contact projection only, never another gravity/angular integration clock.
    loop {
        let mut h=remaining.min(STEP_S);
        let Some(support)=query(tank) else {
            tank.ride.vy-=crate::vehicle_spec::G*h;
            tank.pos.y+=tank.ride.vy*h;
            remaining=(remaining-h).max(0.0);
            if remaining<=0.0 {break;}
            continue;
        };
        if tank.pos.y > support.height+CONTACT_M {
            let rates=Vec2::new(tank.ride.pitch_rate,tank.ride.roll_rate);
            if !rates.is_finite() {break;}
            while rates.length()*h*(support.radius+cg.length()) > CORNER_STEP_M {h*=0.5;}
            if h<=0.0 || remaining-h==remaining {break;}
            let old_com_offset=orientation(tank)*cg;
            tank.pitch+=rates.x*h; tank.roll+=rates.y*h;
            // Free angular motion is about the estimated COM, not the ground-contact origin.
            tank.pos+=old_com_offset-orientation(tank)*cg;
            tank.ride.vy-=crate::vehicle_spec::G*h;
            tank.pos.y+=tank.ride.vy*h;
            if let Some(after)=query(tank) {
                if tank.pos.y<=after.height {
                    tank.pos.y=after.height; tank.ride.vy=0.0;
                    tank.ride.pitch_rate=0.0; tank.ride.roll_rate=0.0;
                }
            }
            remaining=(remaining-h).max(0.0);
            if remaining<=0.0 {break;}
            continue;
        }
        tank.pos.y=support.height;
        tank.ride.vy=0.0;
        let rotation=orientation(tank);
        let com=rotation*cg;
        let polygon=support_polygon(&support.contacts);
        if stable_support(&polygon,com) {
            tank.ride.pitch_rate=0.0; tank.ride.roll_rate=0.0;
            break;
        }
        if h<=0.0 {break;}
        let Some(pivot)=support_pivot(&polygon,com) else {break;};
        let lever=com-pivot;
        let axes=[Mat3::from_rotation_y(tank.heading)*Vec3::NEG_X,
            Mat3::from_rotation_y(tank.heading)*Mat3::from_rotation_x(-tank.pitch)*Vec3::Z];
        let world_inertia=rotation*inertia*rotation.transpose();
        let about_pivot=|a:Vec3,b:Vec3| a.dot(world_inertia*b)
            +a.dot(b)*lever.length_squared()-a.dot(lever)*b.dot(lever);
        let a=about_pivot(axes[0],axes[0]); let b=about_pivot(axes[0],axes[1]);
        let c=about_pivot(axes[1],axes[1]); let determinant=a*c-b*b;
        if determinant <= 1e-8 || !determinant.is_finite() {break;}
        let torque=lever.cross(Vec3::NEG_Y*crate::vehicle_spec::G);
        let force=Vec2::new(torque.dot(axes[0]),torque.dot(axes[1]));
        let acceleration=Vec2::new(c*force.x-b*force.y,a*force.y-b*force.x)/determinant;
        let radius=(support.radius+pivot.length()).max(0.01);
        let max_angle=CORNER_STEP_M/radius;
        let previous_rates=Vec2::new(tank.ride.pitch_rate,tank.ride.roll_rate);
        if !previous_rates.is_finite() || !acceleration.is_finite() {break;}
        // Shorten the time step, never angular speed, to respect the contact sampling distance.
        while (previous_rates.length()+acceleration.length()*h)*h > max_angle {
            h*=0.5;
        }
        if h<=0.0 || remaining-h==remaining {break;}
        let damping=(-CONTACT_ANGULAR_DAMPING*h).exp();
        let mut rates=(previous_rates+acceleration*h)*damping;
        let angle=rates*h;
        let old_pos=tank.pos; let old_pitch=tank.pitch; let old_roll=tank.roll;
        let old_inverse=rotation.transpose();
        let mut fraction=1.0;
        let mut lo=0.0; let mut hi=1.0;
        // Keep the actual pivot point fixed. If another source surface enters the ground,
        // bracket the new sampled contact rather than lifting through it to force the tilt.
        // This is an endpoint subdivision, not continuous triangle/terrain collision detection.
        for iteration in 0..13 {
            tank.pitch=old_pitch+angle.x*fraction;
            tank.roll=old_roll+angle.y*fraction;
            tank.pos=old_pos+(pivot-orientation(tank)*old_inverse*pivot);
            let clear=query(tank)
                .is_none_or(|s| s.height<=tank.pos.y+CONTACT_M);
            if iteration==0 && clear {break;}
            if clear {lo=fraction;} else {hi=fraction;}
            fraction=if iteration==12 {lo} else {(lo+hi)*0.5};
        }
        tank.pitch=old_pitch+angle.x*fraction;
        tank.roll=old_roll+angle.y*fraction;
        tank.pos=old_pos+(pivot-orientation(tank)*old_inverse*pivot);
        if let Some(after)=query(tank) {
            tank.pos.y=tank.pos.y.max(after.height);
        }
        if fraction<1.0 {rates=Vec2::ZERO;}
        tank.ride.pitch_rate=rates.x; tank.ride.roll_rate=rates.y;
        remaining=(remaining-h).max(0.0);
        if remaining<=0.0 {break;}
    }
    tank.ride.pitch=tank.pitch; tank.ride.roll=tank.roll; tank.ride.y=tank.pos.y;
    tank.ride.xz=Vec2::new(tank.pos.x,tank.pos.z);
    tank.ride.load.fill(0.0);
    tank.ride.heave_accel=0.0; tank.ride.pitch_accel=0.0; tank.ride.roll_accel=0.0;
    tank.ride.along_accel=0.0; tank.ride.lateral_accel=0.0;
}

pub(crate) fn advance_rolled_hull(tank: &mut Tank, support: Option<f32>, dt: f32) {
    let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
    tank.ride.overturned = true;
    tank.ride.active = true;
    tank.ride.pitch = tank.pitch;
    tank.ride.roll = tank.roll;
    // Freeze angular motion at its actual pose. A three-DOF ride cannot represent the rolling
    // collision impulse and moving center of rotation of a tumbling hull.
    tank.ride.pitch_rate = 0.0;
    tank.ride.roll_rate = 0.0;
    tank.speed = 0.0;
    tank.ride.vy -= crate::vehicle_spec::G * dt;
    let mut y = tank.pos.y + tank.ride.vy * dt;
    if let Some(support) = support {
        if y <= support { y = support; tank.ride.vy = 0.0; }
    }
    tank.pos.y = y;
    tank.ride.y = y;
    tank.ride.xz = Vec2::new(tank.pos.x, tank.pos.z);
    tank.ride.load.fill(0.0);
    tank.ride.heave_accel = 0.0;
    tank.ride.pitch_accel = 0.0;
    tank.ride.roll_accel = 0.0;
    tank.ride.along_accel = 0.0;
    tank.ride.lateral_accel = 0.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "private full Tiger source; opt-in bounded CPU support-cost receipt"]
    fn private_tiger_rolled_source_support_cost_receipt() {
        let ground=|uneven:bool| {
            let heights:Vec<f32>=(0..33).flat_map(|z|(0..33).map(move|x| {
                let (x,z)=(x as f32-16.0,z as f32-16.0);
                if uneven {0.023*x-0.017*z+0.13*(x*0.7).sin()*(z*0.5).cos()}else{0.0}
            })).collect();
            crate::terrain::Heightfield {res:33,spacing:1.0,
                min_h:heights.iter().copied().fold(f32::INFINITY,f32::min),
                max_h:heights.iter().copied().fold(f32::NEG_INFINITY,f32::max),
                heights,openings:Default::default(),detail:None}
        };
        let flat=ground(false);let uneven=ground(true);let mut detailed=ground(true);
        let detail=std::sync::Arc::new(crate::terrain_relief::Detail::new(33,1.0,
            detailed.heights.clone(),vec![[crate::terrain_relief::SNOW,100,0,0];33*33],
            Vec2::new(0.6,0.8),"rollover bound/cost fixture".into()).unwrap());
        assert!(detail.crater(Vec3::new(2.0,0.0,-3.0),3.0).is_some());
        detailed.attach_detail(detail);
        let seed=Tank::new(1.7,-2.4,0.43,&flat,crate::tank::TIGER_PARTS);
        let start=std::time::Instant::now();
        let source=body_source(&seed);
        let load_ms=start.elapsed().as_secs_f64()*1000.0;
        for (label,hf) in [("flat",flat),("uneven_grid",uneven),("detail_crater",detailed)] {
        let mut tank=seed.clone();tank.roll=std::f32::consts::PI-0.17;tank.alive=false;
        let start=std::time::Instant::now();
        let support=support_at(&tank,&source.parts,true,&|x,z|hf.height_at(x,z)).unwrap();
        let query_ms=start.elapsed().as_secs_f64()*1000.0;
        let start=std::time::Instant::now();
        let bounded=support_at_bounded(&tank,&source.parts,&source.bounds,true,
            &|x,z|hf.height_at(x,z),&|a,b|hf.height_upper_bound(a,b)).unwrap();
        let bounded_query_ms=start.elapsed().as_secs_f64()*1000.0;
        assert_eq!(support.height.to_bits(),bounded.height.to_bits());
        assert_eq!(support.contacts,bounded.contacts);
        if label=="flat" {assert!(bounded.stats.tested_points<support.stats.tested_points,
            "flat grade must prune actual source queries: {:?}",bounded.stats);
        }
        tank.pos.y=support.height;
        let mut bounded_tank=tank.clone();
        let start=std::time::Instant::now();
        advance_rolled_hull_on(&mut tank,1.0/120.0,true,|x,z|hf.height_at(x,z));
        let step_ms=start.elapsed().as_secs_f64()*1000.0;
        let start=std::time::Instant::now();
        advance_rolled_hull_bounded(&mut bounded_tank,1.0/120.0,true,
            |x,z|hf.height_at(x,z),|a,b|hf.height_upper_bound(a,b));
        let bounded_step_ms=start.elapsed().as_secs_f64()*1000.0;
        let after=support_at(&tank,&source.parts,true,&|x,z|hf.height_at(x,z)).unwrap();
        let bounded_after=support_at(&bounded_tank,&source.parts,true,&|x,z|hf.height_at(x,z)).unwrap();
        assert!(tank.pos.y>=after.height-1e-5&&!tank.alive);
        assert!(bounded_tank.pos.y>=bounded_after.height-1e-5&&!bounded_tank.alive);
        eprintln!("private_tiger_rollover terrain={label} hull_vertices={} turret_vertices={} max_source_edge_m={} source_load_ms={load_ms:.3} support_query_ms={query_ms:.3} bounded_query_ms={bounded_query_ms:.3} one_120hz_step_ms={step_ms:.3} bounded_step_ms={bounded_step_ms:.3} bounded_stats={:?}",
            source.parts[0].len(),source.parts[1].len(),source.max_triangle_edge_m,bounded.stats);
        }
    }

    #[test]
    fn bounded_support_matches_brute_force_maximum_and_every_contact_after_articulation() {
        let parts=[box_points(1.4),(0..3072).map(|i|Vec3::new((i%31) as f32*0.05-0.75,
            (i%47) as f32*0.03,((i*19)%43) as f32*0.06-1.1)).collect()];
        let bounds=parts.each_ref().map(|p|SourcePointBvh::new(p));
        let height=|x:f32,z:f32|(0.03*x-0.02*z).max(-0.01*x+0.04*z-1.0);
        let upper=|a:Vec2,b:Vec2|[height(a.x,a.y),height(a.x,b.y),height(b.x,a.y),height(b.x,b.y)]
            .into_iter().fold(f32::NEG_INFINITY,f32::max);
        for position in [Vec3::ZERO,Vec3::new(5500.0,7.0,-4400.0)] {
            for attached in [false,true] {
                let mut tank=fixture();tank.pos=position;tank.pitch=0.32;tank.roll=2.8;
                tank.heading=0.73;tank.turret_yaw=-0.59;tank.parts.turret_pivot=Vec3::new(0.1,1.2,-0.17);
                let brute=support_at(&tank,&parts,attached,&height).unwrap();
                for unknown in [false,true] {
                    let result=support_at_bounded(&tank,&parts,&bounds,attached,&height,
                        &|a,b|if unknown {f32::INFINITY}else{upper(a,b)}).unwrap();
                    assert_eq!(result.height.to_bits(),brute.height.to_bits());
                    assert_eq!(result.contacts,brute.contacts,"bounded query must preserve source contact order and band");
                    assert!(result.radius>=brute.radius,"only the angular travel bound may be conservative");
                }
            }
        }
    }

    fn fixture() -> Tank {
        let hf=crate::terrain::Heightfield {res:2,spacing:1000.0,heights:vec![0.0;4],
            min_h:0.0,max_h:0.0,openings:Default::default(),detail:None};
        let mut tank=Tank::new(0.0,0.0,0.0,&hf,crate::tank::T34_PARTS);
        tank.parts.turret_pivot=Vec3::ZERO;
        tank.roll=std::f32::consts::PI;
        tank.alive=false;
        tank
    }

    fn box_points(top: f32) -> Vec<Vec3> {
        let mut points=Vec::new();
        for x in [-1.0,1.0] {for y in [0.0,top] {for z in [-2.0,2.0] {
            points.push(Vec3::new(x,y,z));
        }}}
        points
    }

    #[test]
    fn contact_polygon_rejects_point_line_and_edge_balance() {
        let points=box_points(0.0);
        let hull=support_polygon(&points);
        assert_eq!(hull.len(),4);
        assert!(stable_support(&hull,Vec3::new(0.0,3.0,0.0)));
        assert!(!stable_support(&hull,Vec3::new(1.0,3.0,0.0)));
        assert!(!stable_support(&hull,Vec3::new(1.2,3.0,0.0)));
        let pivot=support_pivot(&hull,Vec3::new(1.2,3.0,0.4)).unwrap();
        assert!(pivot.distance(Vec3::new(1.0,0.0,0.4))<1e-6);
        for p in [vec![Vec3::ZERO],vec![Vec3::NEG_X,Vec3::ZERO,Vec3::X]] {
            assert!(!stable_support(&support_polygon(&p),Vec3::Y));
        }
        assert!(support_pivot(&[],Vec3::Y).is_none());
    }

    #[test]
    fn eccentric_single_source_contact_tips_under_gravity_without_a_rest_angle() {
        let parts=[box_points(1.0),vec![Vec3::new(0.75,2.2,0.15)]];
        let mut tank=fixture();
        tank.pos.y=support_at(&tank,&parts,true,&|_,_|0.0).unwrap().height;
        let before=tank.roll;
        let cg=Vec3::Y*crate::vehicle_spec::T34_76.cg_height_m;
        let before_com=tank.pos.y+(orientation(&tank)*cg).y;
        let support=support_at(&tank,&parts,true,&|_,_|0.0).unwrap();
        assert_eq!(support.contacts.len(),1,"fixture really rests on one eccentric source point");
        for _ in 0..60 {
            advance_source(&mut tank,&parts,1.0/60.0,true,&|_,_|0.0);
            let support=support_at(&tank,&parts,true,&|_,_|0.0).unwrap();
            assert!(tank.pos.y>=support.height-1e-5,"source must remain outside ground");
        }
        assert!((tank.roll-before).abs()>0.02,"gravity must tip unsupported COM");
        assert!(tank.pos.y+(orientation(&tank)*cg).y<before_com-0.005,"gravity lowers COM");
        assert_eq!(tank.heading,0.0);
        assert_eq!(tank.speed,0.0);
        assert!(!tank.alive);
    }

    #[test]
    fn a_supported_source_face_rests_without_prescribed_angles() {
        let parts=[box_points(1.4),Vec::new()];
        let mut tank=fixture();
        tank.heading=0.63;
        tank.pos.y=support_at(&tank,&parts,false,&|_,_|0.0).unwrap().height;
        let before=(tank.pos,tank.pitch,tank.roll);
        for _ in 0..120 {advance_source(&mut tank,&parts,1.0/60.0,false,&|_,_|0.0);}
        assert!(tank.pos.distance(before.0)<1e-6);
        assert_eq!((tank.pitch,tank.roll),(before.1,before.2));
        let support=support_at(&tank,&parts,false,&|_,_|0.0).unwrap();
        let com=orientation(&tank)*(Vec3::Y*crate::vehicle_spec::T34_76.cg_height_m);
        assert!(stable_support(&support_polygon(&support.contacts),com));
        assert_eq!((tank.ride.pitch_rate,tank.ride.roll_rate),(0.0,0.0));
    }

    #[test]
    fn detached_turret_is_absent_from_angular_support_and_hull_falls() {
        let parts=[box_points(1.4),vec![Vec3::new(0.75,2.8,0.0)]];
        let mut tank=fixture();
        tank.pos.y=support_at(&tank,&parts,true,&|_,_|0.0).unwrap().height;
        let high=tank.pos.y;
        advance_source(&mut tank,&parts,1.0/60.0,false,&|_,_|0.0);
        assert!(tank.pos.y<high && tank.pos.y>high-0.01,"support loss falls continuously");
        for _ in 0..180 {advance_source(&mut tank,&parts,1.0/60.0,false,&|_,_|0.0);}
        assert!((tank.pos.y-1.4).abs()<1e-5);
        assert_eq!(tank.roll,std::f32::consts::PI,"absent eccentric turret cannot tip hull");
        assert!(!tank.alive);
    }

    #[test]
    fn zero_dt_projection_preserves_existing_unsupported_angular_motion() {
        let parts=[box_points(1.0),vec![Vec3::new(0.75,2.2,0.15)]];
        let mut tank=fixture();
        tank.pos.y=support_at(&tank,&parts,true,&|_,_|0.0).unwrap().height;
        tank.ride.pitch_rate=0.03; tank.ride.roll_rate=-0.04;
        let before=(tank.pos,tank.pitch,tank.roll,tank.ride.pitch_rate,tank.ride.roll_rate);
        advance_source(&mut tank,&parts,0.0,true,&|_,_|0.0);
        assert_eq!((tank.pos,tank.pitch,tank.roll,tank.ride.pitch_rate,tank.ride.roll_rate),before);
    }

    #[test]
    fn inverted_source_roof_stays_above_ground_without_righting_pose() {
        let points = [vec![Vec3::new(-1.0, 0.2, -2.0), Vec3::new(1.0, 0.2, 2.0)],
            vec![Vec3::new(-0.8, 2.4, 0.0), Vec3::new(0.8, 2.4, 0.0)]];
        let rotation = Mat4::from_rotation_z(std::f32::consts::PI);
        let needed = required_origin_height(&points, [rotation; 2], 0.0, true, |_,_| 3.0).unwrap();
        assert!((needed - 5.4).abs() < 1e-5);
        for point in points.iter().flatten() {
            assert!(rotation.transform_point3(*point).y + needed >= 3.0 - 1e-5);
        }
        let hf = crate::terrain::Heightfield { res: 2, spacing: 1000.0, heights: vec![3.0;4],
            min_h: 3.0, max_h: 3.0, openings: Default::default(), detail: None };
        let mut tank = Tank::new(0.0, 0.0, 0.3, &hf, crate::tank::T34_PARTS);
        tank.roll = std::f32::consts::PI;
        tank.speed = 6.0;
        for _ in 0..120 { advance_rolled_hull(&mut tank, Some(needed), 1.0 / 60.0); }
        assert_eq!(tank.roll, std::f32::consts::PI);
        assert_eq!(tank.heading, 0.3);
        assert_eq!(tank.speed, 0.0);
        assert_eq!(tank.pos.y, needed);
    }

    #[test]
    fn detaching_the_supporting_turret_makes_the_hull_fall_onto_its_own_source() {
        let points = [vec![Vec3::new(-1.0, 1.1, -2.0), Vec3::new(1.0, 1.1, 2.0)],
            vec![Vec3::new(-0.8, 2.4, 0.0), Vec3::new(0.8, 2.4, 0.0)]];
        let rotation = Mat4::from_rotation_z(std::f32::consts::PI);
        let mounted = required_origin_height(&points, [rotation; 2], 0.0, true, |_,_| 0.0).unwrap();
        let detached = required_origin_height(&points, [rotation; 2], 0.0, false, |_,_| 0.0).unwrap();
        assert!((mounted - 2.4).abs() < 1e-5 && (detached - 1.1).abs() < 1e-5);
        let hf = crate::terrain::Heightfield { res: 2, spacing: 1000.0, heights: vec![0.0;4],
            min_h: 0.0, max_h: 0.0, openings: Default::default(), detail: None };
        let mut tank = Tank::new(0.0, 0.0, 0.0, &hf, crate::tank::T34_PARTS);
        tank.roll = std::f32::consts::PI;
        tank.pos.y = mounted;
        tank.alive = false; // destruction must not pin the hull on a now-absent source part
        advance_rolled_hull(&mut tank, Some(detached), 1.0 / 60.0);
        assert!(tank.pos.y < mounted && tank.pos.y > mounted - 0.01, "must fall, not teleport");
        for _ in 0..120 { advance_rolled_hull(&mut tank, Some(detached), 1.0 / 60.0); }
        assert!((tank.pos.y - detached).abs() < 1e-5);
        assert_eq!(tank.roll, std::f32::consts::PI);
        assert!(!tank.alive);
    }
}
