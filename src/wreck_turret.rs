//! Detached turret rigid body derived from the loaded turret/gun surfaces. Thin-shell mass,
//! pressure coupling and inelastic contact are explicit approximations, not an ammo simulation.
use super::surface_fire;
use crate::{ballistics::{ArmorScheme, Facing}, gltf_load::CpuMesh, terrain::Heightfield};
use glam::{Mat3, Mat4, Quat, Vec3};
use std::{collections::BTreeMap, sync::Arc};

const STEEL_DENSITY: f32 = 7_850.0;
const SUPPORT_SPACING_M: f32 = 0.18;
const SUBSTEP_S: f32 = 1.0 / 120.0;

#[derive(Clone)]
pub(super) struct Source {
    turret: Vec<([Vec3; 3], f32)>,
    gun: Vec<([Vec3; 3], f32)>,
    turret_points: Vec<Vec3>,
    gun_points: Vec<Vec3>,
    pub support_gap_bound_m: f32,
    hull: Arc<surface_fire::CabinFire>,
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
            hull: Arc::new(surface_fire::CabinFire::new(&[(0, &meshes[0])])) }
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

    pub fn advance(&mut self, dt:f32, terrain:&Heightfield, hull_world:Mat4) {
        if self.settled || !dt.is_finite() || dt<=0.0 {return;}
        let steps=(dt.min(0.1)/SUBSTEP_S).ceil().max(1.0) as usize;
        let h=dt.min(0.1)/steps as f32;
        let mut models=[Mat4::IDENTITY;surface_fire::FRAMES];models[0]=hull_world;
        let source=self.source.clone();
        let hull=source.hull.source_contacts(&models);
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
                    contacts.push((p,normal,(ground+0.002-p.y)/normal.y.max(0.1)));
                }
                // During ascent the turret is leaving its ring. On descent query the same
                // loaded hull triangles so it can come to rest on its own engine deck.
                if self.velocity.y<=0.0 {
                    let from=previous.transform_point3(local)+Vec3::Y*0.004;
                    hull_paths.push([from,p-Vec3::Y*0.004]);hull_ends.push(p);
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
                        contacts.push((point,normal,(point-p).dot(normal).max(0.0)+0.002));
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
            // Average the nearly coplanar deepest supports into one contact manifold. Picking
            // a single deepest source sample made arbitrary float-rounding ties choose opposite
            // corners of a resting plate, changing its torque with world translation.
            contacts.sort_unstable_by(|a,b|b.2.total_cmp(&a.2));
            let (_,deepest_normal,depth)=contacts[0];
            let mut point=Vec3::ZERO;let mut normal=Vec3::ZERO;let mut count=0.0;
            for &(p,n,d) in &contacts {
                if d>=depth-0.001 && n.dot(deepest_normal)>0.94 {
                    point+=p-self.center;normal+=n;count+=1.0;
                }
            }
            let point=self.center+point/count;let normal=normal.normalize();
            self.center+=normal*depth;
            self.highest_center_y=self.highest_center_y.max(self.center.y);
            self.max_contact_correction_m=self.max_contact_correction_m.max(depth);
            let arm=point-self.center;
            let inverse=self.inverse_world();
            let contact_velocity=self.velocity+(inverse*self.momentum).cross(arm);
            let vn=contact_velocity.dot(normal);
            if vn<0.0 {
                let cross=arm.cross(normal);
                let effective=1.0/self.mass_kg+cross.dot(inverse*cross);
                let impulse=normal*(-vn*1.02/effective);
                self.velocity+=impulse/self.mass_kg;self.momentum+=arm.cross(impulse);
            }
            let damping=(-9.0*h).exp();
            self.velocity*=damping;self.momentum*=damping;
            if self.velocity.length()<0.28 && self.angular_speed()<0.18 {
                self.quiet_s+=h;
                if self.quiet_s>0.45 {
                    self.settled=true;self.velocity=Vec3::ZERO;self.momentum=Vec3::ZERO;break;
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
            openings:Default::default()};
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
            openings:Default::default()};
        let mut rise=0.0_f32;
        for _ in 0..1440 {
            body.advance(1.0/120.0,&hf,Mat4::IDENTITY);
            rise=rise.max(body.center.y-initial_center.y);
            if body.settled {break;}
        }
        println!("source T34: mass_kg={:.3}, speed_mps={speed:.3}, spin_rad_s={spin:.3}, COM_rise_m={rise:.3}, support_samples={}, gap_bound_m={:.4}, settled={}, final_speed={:.3}, final_spin={:.3}, quiet_s={:.3}, center={:?}",
            body.mass_kg,body.points.len(),source.support_gap_bound_m,body.settled,
            body.velocity.length(),body.angular_speed(),body.quiet_s,body.center);
        assert!(rise<2.0,"not the previous ten-to-twenty metre ballistic toss: {rise}");
        assert!(spin<0.5);
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
            Heightfield {res,spacing,heights,min_h:-30.0,max_h:30.0,openings:Default::default()}
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
