//! Detached turret rigid body derived from the loaded turret/gun surfaces. Thin-shell mass,
//! pressure coupling and inelastic contact are explicit approximations, not an ammo simulation.
use super::surface_fire;
use crate::{ballistics::{ArmorScheme, Facing}, gltf_load::CpuMesh, terrain::Heightfield};
use glam::{Mat3, Mat4, Quat, Vec3};
use std::{collections::BTreeMap, sync::Arc};

const STEEL_DENSITY: f32 = 7_850.0;
const SUPPORT_SPACING_M: f32 = 0.18;
const SUBSTEP_S: f32 = 1.0 / 120.0;
const CONTACT_FRICTION:f32=0.50; // estimated dry steel/ground Coulomb coefficient

#[derive(Clone)]
pub(super) struct Source {
    turret: Vec<([Vec3; 3], f32)>,
    gun: Vec<([Vec3; 3], f32)>,
    turret_points: Vec<Vec3>,
    gun_points: Vec<Vec3>,
    pub support_gap_bound_m: f32,
    hull: Arc<surface_fire::CabinFire>,
    // Identical source positions, but only triangles whose rendered vertex normals establish
    // an unambiguous outward face. The ordinary collision source remains untouched.
    support_hull: Arc<surface_fire::CabinFire>,
    hull_bounds:[Vec3;2],
}

fn support_mesh(mesh:&CpuMesh)->CpuMesh {
    let mut support=mesh.clone();support.indices.clear();
    for ids in mesh.indices.chunks_exact(3) {
        let p=std::array::from_fn::<Vec3,3,_>(|i|Vec3::from_array(mesh.vertices[ids[i] as usize].pos));
        let geometric=(p[1]-p[0]).cross(p[2]-p[0]).normalize_or_zero();
        let authored=ids.iter().map(|&i|Vec3::from_array(mesh.vertices[i as usize].normal)).sum::<Vec3>().normalize_or_zero();
        let alignment=geometric.dot(authored);
        if !alignment.is_finite()||alignment.abs()<=1e-4 {continue;}
        support.indices.extend(if alignment>0.0 {[ids[0],ids[1],ids[2]]}else{[ids[0],ids[2],ids[1]]});
    }
    support
}

// Exact source-AABB broadphase only: it never creates a contact or impulse.
fn hull_bounds(mesh:&CpuMesh)->[Vec3;2] {
    mesh.vertices.iter().fold([Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)],|[lo,hi],v| {
        let p=Vec3::from_array(v.pos);[lo.min(p),hi.max(p)]
    })
}
fn world_bounds([lo,hi]:[Vec3;2],model:Mat4)->[Vec3;2] {
    let mut result=[Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)];
    for x in [lo.x,hi.x] {for y in [lo.y,hi.y] {for z in [lo.z,hi.z] {
        let p=model.transform_point3(Vec3::new(x,y,z));result[0]=result[0].min(p);result[1]=result[1].max(p);
    }}}
    // Outward numerical padding scales with world-coordinate rounding, not collision thickness.
    let error=8.0*f32::EPSILON*result[0].abs().max(result[1].abs()).max(Vec3::ONE);
    [result[0]-error,result[1]+error]
}
// A sufficient static-friction condition: each retained contact can individually supply
// a vertical reaction, then the source support polygon can distribute those positive forces.
fn vertical_force_feasible(normal:Vec3)->bool {
    normal.y>0.0 && Vec3::new(normal.x,0.0,normal.z).length()<=CONTACT_FRICTION*normal.y
}
fn may_touch(from:Vec3,to:Vec3,[lo,hi]:[Vec3;2])->bool {
    from.min(to).cmple(hi).all()&&from.max(to).cmpge(lo).all()
}

fn outer(v: Vec3) -> Mat3 { Mat3::from_cols(v * v.x, v * v.y, v * v.z) }

fn source_triangles(mesh: &CpuMesh, armor: ArmorScheme, gun: bool) -> Vec<([Vec3; 3], f32)> {
    mesh.indices.chunks_exact(3).filter_map(|tri| {
        let p = std::array::from_fn(|i| Vec3::from_array(mesh.vertices[tri[i] as usize].pos));
        let cross = (p[1] - p[0]).cross(p[2] - p[0]);
        let area = cross.length() * 0.5;
        if area < 1e-10 || !area.is_finite() { return None; }
        let normal = cross.normalize();
        let facing = if normal.z > 0.55 { Facing::TurretFront }
            else if normal.z < -0.55 { Facing::TurretRear } else { Facing::TurretSide };
        // Source meshes do not contain measured plate-wall interiors. Integrate an explicit
        // thin-steel-shell estimate: existing armor scheme on walls, 30% side thickness on
        // roof/floor and 25 mm on the attached gun shell. These are not historical mass claims.
        let thickness = if gun { 0.025 } else {
            armor.plate(facing).thickness_mm * 0.001 * if normal.y.abs() > 0.65 {0.30} else {1.0}
        };
        Some((p, area * thickness * STEEL_DENSITY))
    }).collect()
}

fn surface_samples(triangles: &[([Vec3; 3], f32)]) -> (Vec<Vec3>, f32) {
    let mut points = BTreeMap::new();
    let mut bound = 0.0_f32;
    for &(p, _) in triangles {
        let edge = p[0].distance(p[1]).max(p[1].distance(p[2])).max(p[2].distance(p[0]));
        let n = (edge / SUPPORT_SPACING_M).ceil().clamp(1.0, 64.0) as usize;
        bound = bound.max(edge / n as f32);
        for i in 0..=n { for j in 0..=n-i {
            let q = p[0] + (p[1]-p[0])*(i as f32/n as f32) + (p[2]-p[0])*(j as f32/n as f32);
            let key = (q * 100_000.0).round().as_ivec3();
            points.entry((key.x, key.y, key.z)).or_insert(q);
        }}
    }
    (points.into_values().collect(), bound)
}

impl Source {
    pub fn new(meshes: &[CpuMesh; 3], armor: ArmorScheme) -> Self {
        let turret = source_triangles(&meshes[1], armor, false);
        let gun = source_triangles(&meshes[2], armor, true);
        let (turret_points, a) = surface_samples(&turret);
        let (gun_points, b) = surface_samples(&gun);
        Self { turret, gun, turret_points, gun_points, support_gap_bound_m: a.max(b),
            hull: Arc::new(surface_fire::CabinFire::new(&[(0, &meshes[0])])),
            support_hull:Arc::new(surface_fire::CabinFire::new(&[(0,&support_mesh(&meshes[0]))])),
            hull_bounds:hull_bounds(&meshes[0]) }
    }
}

#[derive(Clone)]
pub(super) struct Body {
    source: Arc<Source>,
    points: Arc<Vec<Vec3>>,
    local_com: Vec3,
    pub mass_kg: f32,
    inverse_inertia: Mat3,
    pub center: Vec3,
    initial_center: Vec3,
    highest_center_y: f32,
    max_contact_correction_m: f32,
    rotation: Quat,
    pub velocity: Vec3,
    momentum: Vec3,
    pub launched_energy_j: f32,
    pub settled: bool,
    quiet_s: f32,
}

impl Body {
    pub fn launch(source: Arc<Source>, gun_local: Mat4, ring_world: Mat4,
        severity: f32, impact_point: Vec3, outward_normal: Vec3) -> Option<Self> {
        let mut mass = 0.0;
        let mut first = Vec3::ZERO;
        let mut second = Mat3::ZERO;
        for (part, transform) in [(&source.turret, Mat4::IDENTITY), (&source.gun, gun_local)] {
            for &(triangle, m) in part {
                let [a,b,c] = triangle.map(|p| transform.transform_point3(p));
                let sum = a+b+c;
                mass += m; first += sum * (m/3.0);
                // Exact second moment of a uniformly weighted triangular lamina.
                second += (outer(a)+outer(b)+outer(c)+outer(sum))*(m/12.0);
            }
        }
        if mass < 1.0 || !mass.is_finite() { return None; }
        let local_com = first/mass;
        let central = second - outer(local_com)*mass;
        let inertia = Mat3::IDENTITY*(central.x_axis.x+central.y_axis.y+central.z_axis.z)-central;
        if !inertia.determinant().is_finite() || inertia.determinant() <= 1e-8 { return None; }
        let inverse_inertia = inertia.inverse();
        let (_, rotation, _) = ring_world.to_scale_rotation_translation();
        let center = ring_world.transform_point3(local_com);
        let mut points = source.turret_points.clone();
        points.extend(source.gun_points.iter().map(|&p|gun_local.transform_point3(p)));
        let normal = Vec3::new(outward_normal.x,0.0,outward_normal.z).normalize_or_zero();
        let direction = (Vec3::Y*0.80 + normal*0.60).normalize();
        let energy = crate::sound_levels::AMMUNITION_DETONATION_TNT_KG * 4_184_000.0
            * (0.0012+0.0016*severity.clamp(0.0,1.0));
        let r = Mat3::from_quat(rotation);
        let inverse_world = r*inverse_inertia*r.transpose();
        // Distributed vent pressure acts mainly through the COM. The actual damaged side
        // supplies only a small eccentricity, replacing several random revolutions per second.
        let lever = (impact_point-center).clamp_length_max(3.0)*0.035;
        let angular_direction = lever.cross(direction);
        let coefficient = 1.0/mass + angular_direction.dot(inverse_world*angular_direction);
        let impulse = direction*(2.0*energy/coefficient).sqrt();
        Some(Self { source, points:Arc::new(points), local_com, mass_kg:mass, inverse_inertia,
            center, initial_center:center, highest_center_y:center.y, max_contact_correction_m:0.0,
            rotation, velocity:impulse/mass, momentum:lever.cross(impulse),
            launched_energy_j:energy, settled:false, quiet_s:0.0 })
    }

    pub fn model(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation,self.center-self.rotation*self.local_com)
    }
    fn inverse_world(&self) -> Mat3 {
        let r=Mat3::from_quat(self.rotation);r*self.inverse_inertia*r.transpose()
    }
    pub fn angular_speed(&self) -> f32 { (self.inverse_world()*self.momentum).length() }
    pub fn kinetic_energy(&self) -> f32 {
        0.5*self.mass_kg*self.velocity.length_squared()+0.5*self.momentum.dot(self.inverse_world()*self.momentum)
    }
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"estimated_mass_kg":self.mass_kg,"launch_energy_j":self.launched_energy_j,
            "center":self.center.to_array(),"velocity":self.velocity.to_array(),
            "angular_speed_rad_s":self.angular_speed(),"kinetic_energy_j":self.kinetic_energy(),
            "settled":self.settled,"support_samples":self.points.len(),
            "initial_center":self.initial_center.to_array(),
            "peak_com_rise_m":self.highest_center_y-self.initial_center.y,
            "max_contact_correction_m":self.max_contact_correction_m,
            "support_gap_bound_m":self.source.support_gap_bound_m})
    }

    // Segment normals face the ray, so recover authored-normal-oriented winding at the LOCAL hit.
    // Unknown orientation was excluded from this support-only cache. 0.1 mm is a triangle-fit
    // tolerance; a normalized upward component <=1e-4 is not certified as a support.
    fn gravity_face(local:&surface_fire::SourceContacts<'_>,p:Vec3,world:Mat4)->bool {
        const E:f32=0.0001;
        local.world_triangles(p-Vec3::splat(E),p+Vec3::splat(E)).into_iter().any(|[a,b,c]| {
            let n=(b-a).cross(c-a).normalize_or_zero();
            world.transform_vector3(n).normalize_or_zero().y>1e-4
                && (p-a).dot(n).abs()<=E
                && [(a,b),(b,c),(c,a)].into_iter().all(|(u,v)|
                    (v-u).cross(p-u).dot(n)>=-E*(v-u).length())
        })
    }

    // Recheck the existing source samples; no support proxy or assumed ring/deck plane.
    // The 4 mm probe is the existing finite source-contact reach, not a new solid thickness.
    // Carrier motion that leaves this support unchanged does not wake the body: tangential
    // friction/transport is not solved. No carrier cache or world-save layout is added.
    fn supported_at_rest(&self, terrain:&Heightfield, hull:&surface_fire::SourceContacts<'_>,
        hull_world:Mat4) -> bool {
        let mut contacts=Vec::new();let mut paths=Vec::new();
        let identity=[Mat4::IDENTITY;surface_fire::FRAMES];
        let local=self.source.support_hull.source_contacts(&identity);
        let bounds=world_bounds(self.source.hull_bounds,hull_world);
        for &local in self.points.iter() {
            let offset=self.rotation*(local-self.local_com);let p=self.center+offset;
            if (p.y-terrain.height_at(p.x,p.z)).abs()<=0.004 {
                let d=0.05;let normal=Vec3::new(terrain.height_at(p.x-d,p.z)-terrain.height_at(p.x+d,p.z),
                    2.0*d,terrain.height_at(p.x,p.z-d)-terrain.height_at(p.x,p.z+d)).normalize();
                if vertical_force_feasible(normal) {contacts.push(offset);}
            }
            let path=[p+Vec3::Y*0.004,p-Vec3::Y*0.004];
            if may_touch(path[0],path[1],bounds) {paths.push(path);}
        }
        // Terrain support alone can certify gravity balance without a second hull query.
        if crate::vehicle_ground::stable_support(&crate::vehicle_ground::support_polygon(&contacts),Vec3::ZERO) {return true;}
        for hit in hull.sweep_batch(&paths).into_iter().flatten() {
            if Self::gravity_face(&local,hit.point,hull_world)
                && vertical_force_feasible(hull_world.transform_vector3(hit.normal).normalize()) {
                contacts.push(hull_world.transform_vector3(hit.point)+(hull_world.w_axis.truncate()-self.center));
            }
        }
        crate::vehicle_ground::stable_support(
            &crate::vehicle_ground::support_polygon(&contacts),Vec3::ZERO)
    }

    // Per-substep accumulated impulses; no additional persistent/warm-start save state.
    // Coulomb coefficient 0.50 is an explicit dry steel/ground contact estimate. Reactions
    // are applied at actual source manifold vertices, never at an averaged/fabricated point.
    // Existing 2% restitution remains for impacts faster than 0.5 m/s; persistent contact
    // has zero bounce so gravity does not regenerate a tiny impact rebound every substep.
    fn solve_contacts(&mut self,contacts:&[(Vec3,Vec3)]) {
        const ITERATIONS:usize=12;
        let inverse=self.inverse_world();let inv_mass=1.0/self.mass_kg;
        let mut normal_impulses=vec![0.0_f32;contacts.len()];
        let mut tangent_impulses=vec![Vec3::ZERO;contacts.len()];
        let targets:Vec<_>=contacts.iter().map(|&(arm,normal)| {
            let closing=(self.velocity+(inverse*self.momentum).cross(arm)).dot(normal);
            if closing < -0.5 {-closing*0.02}else{0.0}
        }).collect();
        for _ in 0..ITERATIONS {for (i,&(arm,normal)) in contacts.iter().enumerate() {
            let lever=arm.cross(normal);let effective=inv_mass+lever.dot(inverse*lever);
            let closing=(self.velocity+(inverse*self.momentum).cross(arm)).dot(normal);
            let next=(normal_impulses[i]+(targets[i]-closing)/effective).max(0.0);
            let change=next-normal_impulses[i];normal_impulses[i]=next;
            self.velocity+=normal*(change*inv_mass);self.momentum+=lever*change;
            let v=self.velocity+(inverse*self.momentum).cross(arm);
            let tangent=v-normal*v.dot(normal);let speed=tangent.length();
            let wanted=if speed>1e-8 {
                let direction=tangent/speed;let lever=arm.cross(direction);
                tangent_impulses[i]-direction*(speed/(inv_mass+lever.dot(inverse*lever)))
            }else{tangent_impulses[i]};
            let next=wanted.clamp_length_max(CONTACT_FRICTION*normal_impulses[i]);
            let change=next-tangent_impulses[i];tangent_impulses[i]=next;
            self.velocity+=change*inv_mass;self.momentum+=arm.cross(change);
        }}
    }

    pub fn advance(&mut self, dt:f32, terrain:&Heightfield, hull_world:Mat4) {
        if !dt.is_finite() || dt<=0.0 {return;}
        let steps=(dt.min(0.1)/SUBSTEP_S).ceil().max(1.0) as usize;
        let h=dt.min(0.1)/steps as f32;
        let mut models=[Mat4::IDENTITY;surface_fire::FRAMES];models[0]=hull_world;
        let source=self.source.clone();
        let hull=source.hull.source_contacts(&models);
        let bounds=world_bounds(source.hull_bounds,hull_world);
        if self.settled {
            if self.supported_at_rest(terrain,&hull,hull_world) {return;}
            self.settled=false;self.quiet_s=0.0;
        }
        for _ in 0..steps {
            let previous=self.model();
            self.velocity.y-=9.81*h;
            self.center+=self.velocity*h;
            self.highest_center_y=self.highest_center_y.max(self.center.y);
            let angular=self.inverse_world()*self.momentum;
            self.rotation=(Quat::from_scaled_axis(angular*h)*self.rotation).normalize();
            let model=self.model();
            let mut contacts=Vec::new();
            let mut hull_paths=Vec::new();
            let mut hull_ends=Vec::new();
            for &local in self.points.iter() {
                let p=model.transform_point3(local);
                let ground=terrain.height_at(p.x,p.z);
                if p.y<ground+0.002 {
                    let d=0.05;
                    let normal=Vec3::new(terrain.height_at(p.x-d,p.z)-terrain.height_at(p.x+d,p.z),
                        2.0*d,terrain.height_at(p.x,p.z-d)-terrain.height_at(p.x,p.z+d)).normalize();
                    let depth=(ground+0.002-p.y)/normal.y.max(0.1);
                    contacts.push((p,normal,depth));
                }
                // During ascent the turret is leaving its ring. On descent query the same
                // loaded hull triangles so it can come to rest on its own engine deck.
                if self.velocity.y<=0.0 {
                    let from=previous.transform_point3(local)+Vec3::Y*0.004;
                    let to=p-Vec3::Y*0.004;
                    if may_touch(from,to,bounds) {hull_paths.push([from,to]);hull_ends.push(p);}
                }
            }
            if !hull_paths.is_empty() {
                for (hit,p) in hull.sweep_batch(&hull_paths).into_iter().zip(hull_ends) {
                    if let Some(hit)=hit {
                        // SourceContacts reports both fields in the contacted mesh's LOCAL
                        // frame. Contact depth and impulse are world-space operations: mixing
                        // these frames creates a position-sized correction away from origin.
                        let contact_model=models[hit.frame];
                        let point=contact_model.transform_point3(hit.point);
                        let mut normal=contact_model.transform_vector3(hit.normal).normalize();
                        if normal.y<0.0 {normal=-normal;}
                        // Probe reach is not penetration: an already separated point must not
                        // receive another 2 mm lift simply because the probe found the hull.
                        let depth=((point-p).dot(normal)+0.002).max(0.0);
                        contacts.push((point,normal,depth));
                    }
                }
            }
            if contacts.is_empty() {
                // Millimetre contact separation can alternate supported/free substeps. Keep
                // accumulated quiet contact time across that gap; reset it for real motion.
                if self.velocity.length()>=0.50 || self.angular_speed()>=0.32 {self.quiet_s=0.0;}
                else if self.velocity.length()>=0.28 || self.angular_speed()>=0.18 {
                    self.quiet_s=(self.quiet_s-h).max(0.0);
                }
                continue;
            }
            // Preserve the existing contact band and position correction. Independent real
            // manifold vertices carry separate reaction forces; a single averaged point cannot
            // balance an asymmetric source face and has no static-friction constraint.
            contacts.sort_unstable_by(|a,b|b.2.total_cmp(&a.2));
            let (_,deepest_normal,depth)=contacts[0];
            let selected:Vec<_>=contacts.iter().filter(|(_,n,d)|
                *d>=depth-0.001 && n.dot(deepest_normal)>0.94).copied().collect();
            let normal=selected.iter().map(|(_,n,_)|*n).sum::<Vec3>().normalize();
            let offsets:Vec<_>=selected.iter().map(|(p,_,_)|*p-self.center).collect();
            let polygon=crate::vehicle_ground::support_polygon(&offsets);
            let constraints:Vec<_>=polygon.into_iter().map(|p| {
                let (_,n,_)=selected.iter().find(|(q,_,_)|*q-self.center==p).unwrap();(p,*n)
            }).collect();
            self.center+=normal*depth;
            self.highest_center_y=self.highest_center_y.max(self.center.y);
            self.max_contact_correction_m=self.max_contact_correction_m.max(depth);
            self.solve_contacts(&constraints);
            let damping=(-9.0*h).exp();
            self.velocity*=damping;self.momentum*=damping;
            // The impact manifold contains pre-projection penetrations, not all final supports.
            // Requery the final source pose with the same 4 mm tolerance used when asleep;
            // never certify a point/edge merely because its velocity is small.
            let stable=self.velocity.length()<0.28 && self.angular_speed()<0.18
                && self.supported_at_rest(terrain,&hull,hull_world);
            if !stable {self.quiet_s=0.0;}
            if stable && self.velocity.length()<0.28 && self.angular_speed()<0.18 {
                self.quiet_s+=h;
                if self.quiet_s>0.45 {
                    self.settled=true;
                    self.velocity=Vec3::ZERO;self.momentum=Vec3::ZERO;break;
                }
            } else if self.velocity.length()>=0.50 || self.angular_speed()>=0.32 {
                self.quiet_s=0.0;
            } else {
                // Hysteresis lets low-energy contact rocking settle without each marginal
                // threshold crossing restarting the entire supported quiet interval.
                self.quiet_s=(self.quiet_s-h).max(0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(scale:f32) -> Arc<Source> {
        let b=crate::prims::box_mesh(Vec3::new(-1.0,0.0,-1.0)*scale,Vec3::new(1.0,1.0,1.0)*scale,[80;3]);
        let tiny=crate::prims::box_mesh(Vec3::splat(-0.001),Vec3::splat(0.001),[80;3]);
        Arc::new(Source::new(&[tiny.clone(),b,tiny],ArmorScheme::T34_76))
    }
    fn sleeping_fixture(rotation:Quat, hull_plate:bool) -> Body {
        let mut src=source(1.0);
        if hull_plate {
            let plate=crate::prims::box_mesh(Vec3::new(-2.0,-0.5,-2.0),Vec3::new(2.0,0.0,2.0),[80;3]);
            Arc::get_mut(&mut src).unwrap().hull=Arc::new(surface_fire::CabinFire::new(&[(0,&plate)]));
            Arc::get_mut(&mut src).unwrap().support_hull=Arc::new(surface_fire::CabinFire::new(&[(0,&support_mesh(&plate))]));
            Arc::get_mut(&mut src).unwrap().hull_bounds=hull_bounds(&plate);
        }
        let mut body=Body::launch(src,Mat4::IDENTITY,Mat4::IDENTITY,0.0,Vec3::ZERO,Vec3::X).unwrap();
        body.rotation=rotation;body.center=rotation*body.local_com;
        let lowest=body.points.iter().map(|&p|body.model().transform_point3(p).y).fold(f32::INFINITY,f32::min);
        body.center.y+=0.002-lowest;body.velocity=Vec3::ZERO;body.momentum=Vec3::ZERO;
        body.settled=true;body.quiet_s=0.46;
        body
    }
    fn ground(height:f32)->Heightfield {
        Heightfield {res:2,spacing:100.0,heights:vec![height;4],min_h:height,max_h:height,
            openings:Default::default(),detail:None}
    }
    #[test]
    fn a_source_corner_cannot_stay_asleep_but_a_supported_face_can() {
        let buried_hull=Mat4::from_translation(Vec3::Y*-10.0);
        let mut face=sleeping_fixture(Quat::IDENTITY,false);
        let position=face.center;
        face.advance(1.0/60.0,&ground(0.0),buried_hull);
        assert!(face.settled);assert_eq!(face.center,position);
        let mut corner=sleeping_fixture(Quat::from_rotation_x(0.27)*Quat::from_rotation_z(0.31),false);
        corner.advance(1.0/60.0,&ground(0.0),buried_hull);
        assert!(!corner.settled,"one source corner cannot certify gravity support");
    }
    #[test]
    fn removing_ground_or_moving_the_actual_supporting_hull_wakes_the_body() {
        let mut terrain_body=sleeping_fixture(Quat::IDENTITY,false);
        let y=terrain_body.center.y;
        terrain_body.advance(1.0/60.0,&ground(-2.0),Mat4::from_translation(Vec3::Y*-10.0));
        assert!(!terrain_body.settled && terrain_body.center.y<y);
        let mut on_hull=sleeping_fixture(Quat::IDENTITY,true);
        on_hull.advance(1.0/60.0,&ground(-10.0),Mat4::IDENTITY);
        assert!(on_hull.settled,"actual source plate carries the fixture");
        let y=on_hull.center.y;
        on_hull.advance(1.0/60.0,&ground(-10.0),Mat4::from_translation(Vec3::Y*-0.5));
        assert!(!on_hull.settled && on_hull.center.y<y,"removed source support must wake and fall");
    }

    #[test]
    fn an_awake_source_plate_does_not_climb_out_of_its_own_contact_band() {
        let mut body=sleeping_fixture(Quat::IDENTITY,true);
        body.settled=false;body.quiet_s=0.0;
        let initial=body.center;
        let floor=ground(-10.0);
        for _ in 0..120 {
            body.advance(1.0/120.0,&floor,Mat4::IDENTITY);
            assert!((body.center.y-initial.y).abs()<0.0001,
                "already-clear source must retain its 2 mm separation, not climb after every probe: initial={} current={}",initial.y,body.center.y);
        }
        assert!(body.settled,"actual plate support must remain continuously available to the quiet test");
    }

    #[test]
    fn a_plate_underside_cannot_certify_sleep_even_when_query_normal_faces_up() {
        let mut body=sleeping_fixture(Quat::IDENTITY,true);
        let roof=body.points.iter().map(|&p|body.model().transform_point3(p).y).fold(f32::NEG_INFINITY,f32::max);
        body.center.y+=-0.501-roof; // entire body below the actual source plate underside (-0.5)
        let models=[Mat4::IDENTITY;surface_fire::FRAMES];let source=body.source.clone();
        let hull=source.hull.source_contacts(&models);
        let p=body.model().transform_point3(Vec3::new(0.0,1.0,0.0));
        let hit=hull.sweep_batch(&[[p+Vec3::Y*0.004,p-Vec3::Y*0.004]])[0].unwrap();
        assert!(hit.normal.y>0.9,"fixture must exercise the ray-facing normal trap");
        assert!(!body.supported_at_rest(&ground(-10.0),&hull,Mat4::IDENTITY));
        body.advance(1.0/60.0,&ground(-10.0),Mat4::IDENTITY);
        assert!(!body.settled);
    }

    #[test]
    fn separate_source_reactions_balance_an_asymmetric_face_but_not_an_eccentric_point() {
        let mut body=sleeping_fixture(Quat::IDENTITY,false);body.settled=false;
        // Three actual points on the source bottom plane surround the COM, but their
        // arithmetic average is offset. One averaged impulse would introduce false torque.
        let face=[(Vec3::new(-1.0,-0.5,-1.0),Vec3::Y),
            (Vec3::new(1.0,-0.5,-1.0),Vec3::Y),(Vec3::new(0.5,-0.5,1.0),Vec3::Y)];
        for _ in 0..120 {body.velocity.y-=9.81/120.0;body.solve_contacts(&face);}
        assert!(body.velocity.length()<0.002 && body.angular_speed()<0.002,
            "separate source reactions must balance gravity: v={:?}, spin={}",body.velocity,body.angular_speed());
        body.velocity=Vec3::NEG_Y*0.1;body.momentum=Vec3::ZERO;
        body.solve_contacts(&[(Vec3::new(1.0,-0.5,0.0),Vec3::Y)]);
        assert!(body.angular_speed()>0.01,"an eccentric point still produces real tipping torque");
    }

    #[test]
    fn launch_is_energy_bounded_and_source_mass_changes_speed() {
        let make=|s|Body::launch(source(s),Mat4::IDENTITY,Mat4::from_translation(Vec3::Y*3.0),
            0.8,Vec3::new(1.0,3.5,0.0),Vec3::X).unwrap();
        let light=make(1.0);let heavy=make(2.0);
        for body in [&light,&heavy] {
            assert!((body.kinetic_energy()-body.launched_energy_j).abs()<body.launched_energy_j*0.001);
            assert!(body.angular_speed()<0.5);
            assert!(body.source.support_gap_bound_m<=SUPPORT_SPACING_M+1e-5);
            assert!(body.model().transform_point3(Vec3::ZERO).distance(Vec3::Y*3.0)<1e-5);
        }
        assert!(heavy.mass_kg>light.mass_kg*3.9);
        assert!(heavy.velocity.length()<light.velocity.length()*0.51);
    }
    #[test]
    fn source_body_lands_without_fixed_ring_height_or_perpetual_spin() {
        let hf=Heightfield {res:17,spacing:4.0,heights:vec![0.0;289],min_h:0.0,max_h:0.0,
            openings:Default::default(),detail:None};
        let mut body=Body::launch(source(1.0),Mat4::IDENTITY,Mat4::from_translation(Vec3::Y*3.0),
            0.8,Vec3::new(1.0,3.5,0.0),Vec3::X).unwrap();
        for _ in 0..1200 {body.advance(1.0/120.0,&hf,Mat4::from_translation(Vec3::Y*-10.0));}
        assert!(body.settled,"source body must settle, speed {} spin {}",body.velocity.length(),body.angular_speed());
        let lowest=body.points.iter().map(|&p|body.model().transform_point3(p).y).fold(f32::INFINITY,f32::min);
        assert!(lowest>=-0.025 && lowest<0.04,"actual source support must touch terrain: {lowest}");
    }
    #[test]
    fn loaded_t34_has_a_short_source_supported_detachment() {
        let meshes=crate::tank::build_t34();
        let source=Arc::new(Source::new(&meshes,ArmorScheme::T34_76));
        let part=crate::tank::T34_PARTS;
        let ring=Mat4::from_translation(part.turret_pivot);
        let gun=Mat4::from_translation(part.gun_trunnion);
        let mut body=Body::launch(source.clone(),gun,ring,0.9,
            Vec3::new(-1.2,2.0,0.0),Vec3::NEG_X).unwrap();
        let initial_center=body.center;
        let speed=body.velocity.length();let spin=body.angular_speed();
        let hf=Heightfield {res:17,spacing:4.0,heights:vec![0.0;289],min_h:0.0,max_h:0.0,
            openings:Default::default(),detail:None};
        let mut rise=0.0_f32;let clock=std::time::Instant::now();let mut steps=0;
        for _ in 0..1440 {
            steps+=1;
            body.advance(1.0/120.0,&hf,Mat4::IDENTITY);
            rise=rise.max(body.center.y-initial_center.y);
            if body.settled {break;}
        }
        eprintln!("actual_t34_contact_cost steps={steps} elapsed_ms={:.3} mean_step_ms={:.3} source_samples={}",clock.elapsed().as_secs_f64()*1000.0,clock.elapsed().as_secs_f64()*1000.0/steps as f64,body.points.len());
        println!("source T34: mass_kg={:.3}, speed_mps={speed:.3}, spin_rad_s={spin:.3}, COM_rise_m={rise:.3}, support_samples={}, gap_bound_m={:.4}, settled={}, final_speed={:.3}, final_spin={:.3}, quiet_s={:.3}, center={:?}",
            body.mass_kg,body.points.len(),source.support_gap_bound_m,body.settled,
            body.velocity.length(),body.angular_speed(),body.quiet_s,body.center);
        assert!(rise<2.0,"not the previous ten-to-twenty metre ballistic toss: {rise}");
        assert!(spin<0.5);
        eprintln!("actual_t34_final rotation_xyzw={:?} max_contact_correction_m={}",body.rotation.to_array(),body.max_contact_correction_m);
        assert!(body.settled,"actual source must settle within12s");
    }
    #[test]
    fn translated_rotated_sloped_t34_contacts_do_not_teleport_the_body() {
        let meshes=crate::tank::build_t34();
        let source=Arc::new(Source::new(&meshes,ArmorScheme::T34_76));
        // The failed native blizzard capture was near (69,5,-241). An origin-only fixture
        // cannot detect mixing hull-local Contact fields with world-space body positions.
        let translation=Vec3::new(69.413155,4.531465,-241.20612);
        let hull_rotation=Mat4::from_rotation_y(0.83)*Mat4::from_rotation_x(-0.055)
            *Mat4::from_rotation_z(0.038);
        let gun=Mat4::from_translation(crate::tank::T34_PARTS.gun_trunnion)
            *Mat4::from_rotation_x(0.024);
        let hulls=[hull_rotation,Mat4::from_translation(translation)*hull_rotation];
        let make=|hull:Mat4| {
            let ring=hull*Mat4::from_translation(crate::tank::T34_PARTS.turret_pivot)
                *Mat4::from_rotation_y(2.36);
            Body::launch(source.clone(),gun,ring,0.74284625,
                hull.transform_point3(Vec3::new(-1.2,1.0,0.1)),
                hull.transform_vector3(Vec3::NEG_X)).unwrap()
        };
        let terrain=|shift:Vec3| {
            let res=257;let spacing=4.0;
            let heights=(0..res).flat_map(|z|(0..res).map(move|x| {
                let x=(x as f32-128.0)*spacing;
                let z=(z as f32-128.0)*spacing;
                shift.y+0.018*(x-shift.x)-0.011*(z-shift.z)
            })).collect();
            Heightfield {res,spacing,heights,min_h:-30.0,max_h:30.0,openings:Default::default(),detail:None}
        };
        let ground=[terrain(Vec3::ZERO),terrain(translation)];
        let mut bodies=hulls.map(make);
        let initial=bodies[1].center;
        let mut largest_step=0.0_f32;let mut max_rise=0.0_f32;let mut equivariance_error=0.0_f32;
        for _ in 0..1440 {
            for i in 0..2 {
                let before=bodies[i].center;
                bodies[i].advance(1.0/120.0,&ground[i],hulls[i]);
                largest_step=largest_step.max(before.distance(bodies[i].center));
            }
            max_rise=max_rise.max(bodies[1].center.y-initial.y);
            equivariance_error=equivariance_error.max((bodies[1].center-translation).distance(bodies[0].center));
            assert!(largest_step<0.25,"source contact may not produce a position-sized correction: {largest_step}");
            assert!(max_rise<2.0,"restrained launch cannot teleport100m upward: {max_rise}");
            if bodies.iter().all(|body|body.settled) {break;}
        }
        println!("translated T34: max_step_m={largest_step:.6}, max_rise_m={max_rise:.6}, translation_error_m={equivariance_error:.6}, settled={:?}",bodies.each_ref().map(|body|body.settled));
        assert!(equivariance_error<0.20,"translated source contacts must agree within the18cm sampling bound: {equivariance_error}");
        assert!(bodies.iter().all(|body|body.settled));
    }
}

impl Body {
    pub(super) fn export_world_state(&self)->anyhow::Result<crate::expedition_world_save::StateFields> {
        Ok(crate::capture_world_fields!(self; points, local_com, mass_kg, inverse_inertia, center, initial_center, highest_center_y, max_contact_correction_m, rotation, velocity, momentum, launched_energy_j, settled, quiet_s))
    }
    pub(super) fn restore_world_state(source:Arc<Source>,mut state:crate::expedition_world_save::StateFields)->anyhow::Result<Self> {
        let body=Self {source,
            points:state.take("points")?,
            local_com:state.take("local_com")?,
            mass_kg:state.take("mass_kg")?,
            inverse_inertia:state.take("inverse_inertia")?,
            center:state.take("center")?,
            initial_center:state.take("initial_center")?,
            highest_center_y:state.take("highest_center_y")?,
            max_contact_correction_m:state.take("max_contact_correction_m")?,
            rotation:state.take("rotation")?,
            velocity:state.take("velocity")?,
            momentum:state.take("momentum")?,
            launched_energy_j:state.take("launched_energy_j")?,
            settled:state.take("settled")?,
            quiet_s:state.take("quiet_s")?,
        };
        state.finish()?;
        anyhow::ensure!(body.center.is_finite()&&body.velocity.is_finite()&&body.rotation.is_finite()&&body.mass_kg>0.0,"invalid detached turret state");
        anyhow::ensure!(!body.points.is_empty()&&body.points.len()<1_000_000&&body.points.iter().all(|p|p.is_finite()),"invalid detached turret source samples");
        Ok(body)
    }
}
