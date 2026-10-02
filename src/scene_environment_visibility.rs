//! CPU reference for the bounded direction-returning source environment path. Mirrors and ideal
//! thin lenses use the existing reconstructed periscope prescription; no finite source-to-point
//! connection, diffuse bounce, or Fresnel reflected-path branch is inferred from this estimator.
use super::*;
#[derive(Clone, Debug)]
pub struct EnvironmentStep {
    pub instance: u32,
    pub material: MaterialClass,
    pub point: Vec3,
    pub incoming: Vec3,
    pub outgoing: Vec3,
    pub event: &'static str,
}
#[derive(Clone, Debug)]
pub struct EnvironmentPath {
    pub status: TransmissionStatus,
    pub throughput: Option<Vec3>,
    pub origin: Vec3,
    pub direction: Vec3,
    pub glass_path_m: f32,
    pub air_path_m: f32,
    pub reason: &'static str,
    pub steps: Vec<EnvironmentStep>,
}
fn refracted(d: Vec3, n: Vec3, eta: f32) -> Option<Vec3> {
    let cosine=-d.dot(n);let discriminant=1.0-eta*eta*(1.0-cosine*cosine);
    (discriminant>=0.0).then(||(eta*d+(eta*cosine-discriminant.sqrt())*n).normalize())
}
impl SceneVisibility {
    pub fn environment_path(&self, origin: Vec3, direction: Vec3, glass: Dielectric) -> Result<EnvironmentPath> {
        require(origin.is_finite() && direction.is_finite() && direction.length_squared()>0.0
            && glass.ior.is_finite() && glass.ior>=1.0 && glass.absorption_per_m.is_finite()
            && glass.absorption_per_m.min_element()>=0.0,"invalid environment path")?;
        let mut result=EnvironmentPath{status:TransmissionStatus::Unresolved,throughput:None,
            origin,direction:direction.normalize(),glass_path_m:0.0,air_path_m:0.0,
            reason:"interface_budget",steps:Vec::new()};
        let mut p=origin;let mut d=direction.normalize();let mut inside=None;let mut throughput=Vec3::ONE;let mut previous=None;
        let budget=(self.instance_words[7]*4).clamp(16,64);
        for _ in 0..budget {
            result.origin=p;result.direction=d;
            let Some(hit)=self.trace_continuation(VisibilityRay{origin:p,direction:d,t_min:if previous.is_some(){0.0}else{0.0001},t_max:1e8},previous)? else {
                if inside.is_none(){result.status=TransmissionStatus::Clear;result.throughput=Some(throughput);result.reason="environment_exit";}
                else {result.reason="open_dielectric";}
                return Ok(result);
            };
            if matches!(hit.material,MaterialClass::Opaque|MaterialClass::AlphaTest) {
                result.status=TransmissionStatus::Blocked;result.throughput=Some(Vec3::ZERO);result.reason="opaque_source_occluder";return Ok(result);
            }
            if inside.is_some(){result.glass_path_m+=hit.t;throughput*=(-glass.absorption_per_m*hit.t).exp();}
            else{result.air_path_m+=hit.t;}
            let point=p+d*hit.t;let n=if d.dot(hit.normal)>0.0 {-hit.normal}else{hit.normal};
            let incoming=d;
            let event=match hit.material {
                MaterialClass::ThinLens=>{
                    let instance=self.instances.iter().find(|v|v.id==hit.instance_id).ok_or(VisibilityError("lens instance absent"))?;
                    let focal=instance.lens_center_focal[3];let center=Vec3::from_slice(&instance.lens_center_focal);
                    if inside.is_some() || focal<=0.001 {result.reason="invalid_lens_medium_or_power";return Ok(result);}
                    let axis=-n;let cosine=d.dot(axis);
                    if cosine<0.05 {result.reason="lens_outside_paraxial_prescription";return Ok(result);}
                    let height=point-center;d=(d/cosine-(height-axis*height.dot(axis))/focal).normalize();throughput*=0.96;"ideal_thin_lens"
                },
                MaterialClass::Mirror=>{
                    if inside.is_some(){result.reason="mirror_inside_glass";return Ok(result);}
                    d=(d-2.0*d.dot(n)*n).normalize();throughput*=0.90;"source_mirror"
                },
                MaterialClass::Dielectric=>{
                    let exiting=inside.is_some();
                    if hit.material_id!=glass.material_id || (exiting && (inside!=Some(hit.instance_id) || hit.front_face))
                        || (!exiting && !hit.front_face) {result.reason="unpaired_or_nested_dielectric";return Ok(result);}
                    let (eta_in,eta_out)=if exiting {(glass.ior,1.0)}else{(1.0,glass.ior)};
                    if let Some(next)=refracted(d,n,eta_in/eta_out){
                        throughput*=1.0-fresnel_dielectric(-d.dot(n),eta_in,eta_out);d=next;
                        inside=if exiting {None}else{Some(hit.instance_id)};
                        if exiting {"glass_exit"}else{"glass_entry"}
                    }else{d=(d-2.0*d.dot(n)*n).normalize();"total_internal_reflection"}
                },
                _=>{result.reason="unresolved_source_material";return Ok(result);}
            };
            result.steps.push(EnvironmentStep{instance:hit.instance_id,material:hit.material,point,incoming,outgoing:d,event});
            p=point;previous=Some(hit);
        }
        result.origin=p;result.direction=d;Ok(result)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::gltf_load::Vertex;
    pub(crate) fn mesh(triangles:&[[Vec3;3]])->CpuMesh {
        let vertices=triangles.iter().flatten().map(|p|Vertex{pos:p.to_array(),normal:[0.,0.,1.],uv:[0.;2],tangent:[1.,0.,0.,1.]}).collect::<Vec<_>>();
        let indices=(0..vertices.len() as u32).collect();CpuMesh::basic(vertices,indices,None,Vec3::ZERO,Vec3::ZERO)
    }
    pub(crate) fn quad(points:[Vec3;4],normal:Vec3)->[[Vec3;3];2] {
        let mut p=points;if (p[1]-p[0]).cross(p[2]-p[0]).dot(normal)<0.0{p.swap(1,3);}[[p[0],p[1],p[2]],[p[0],p[2],p[3]]]
    }
    pub(crate) fn pane(prism:bool)->CpuMesh {
        let v=|x:f32,y:f32,z:f32|Vec3::new(x,y,if prism && z<0.0{0.7+0.1*x}else{z});
        let faces=[([v(-1.,-1.,1.),v(1.,-1.,1.),v(1.,1.,1.),v(-1.,1.,1.)],Vec3::Z),
            ([v(-1.,-1.,-1.),v(1.,-1.,-1.),v(1.,1.,-1.),v(-1.,1.,-1.)],-Vec3::Z),
            ([v(-1.,-1.,-1.),v(-1.,1.,-1.),v(-1.,1.,1.),v(-1.,-1.,1.)],-Vec3::X),
            ([v(1.,-1.,-1.),v(1.,1.,-1.),v(1.,1.,1.),v(1.,-1.,1.)],Vec3::X),
            ([v(-1.,1.,-1.),v(1.,1.,-1.),v(1.,1.,1.),v(-1.,1.,1.)],Vec3::Y),
            ([v(-1.,-1.,-1.),v(1.,-1.,-1.),v(1.,-1.,1.),v(-1.,-1.,1.)],-Vec3::Y)];
        mesh(&faces.into_iter().flat_map(|(p,n)|quad(p,n)).collect::<Vec<_>>())
    }
    pub(crate) fn glass()->Dielectric{Dielectric{material_id:1,ior:1.52,absorption_per_m:Vec3::new(0.08,0.035,0.02)}}
    pub(crate) fn placement(id:u32,geometry:GeometryId,material:MaterialClass)->VisibilityInstance{
        VisibilityInstance{id,geometry,material,material_id:1,model:Mat4::IDENTITY,bend_world:Vec2::ZERO,lens_center_focal:[0.;4]}
    }
    pub(crate) fn optical_parity_fixture()->SceneVisibility {
        let mut scene=SceneVisibility::new();let cube=scene.register_mesh(&pane(false)).unwrap();let prism=scene.register_mesh(&pane(true)).unwrap();
        let plane=scene.register_mesh(&mesh(&quad([Vec3::new(-1.,-1.,0.),Vec3::new(1.,-1.,0.),Vec3::new(1.,1.,0.),Vec3::new(-1.,1.,0.)],Vec3::Z))).unwrap();
        let mut instances=Vec::new();
        for (id,x,geometry,material) in [(1,0.,cube,MaterialClass::Dielectric),(2,4.,prism,MaterialClass::Dielectric),
            (3,8.,plane,MaterialClass::Mirror),(4,12.,plane,MaterialClass::ThinLens),(5,16.,plane,MaterialClass::ThinLens),
            (6,16.,plane,MaterialClass::Opaque),(7,20.,plane,MaterialClass::Dielectric)] {
            let mut a=placement(id,geometry,material);a.model=Mat4::from_translation(Vec3::new(x,0.,if id==6 {-0.5}else{0.}));
            if material==MaterialClass::ThinLens {a.lens_center_focal=[x,0.,0.,0.5];}instances.push(a);
        }
        for (id,x) in [(8,24.0),(10,32.0)] {
            let mut a=placement(id,cube,MaterialClass::Dielectric);a.model=Mat4::from_scale_rotation_translation(Vec3::new(1.,1.,0.00002),glam::Quat::IDENTITY,Vec3::new(x,0.,0.));instances.push(a);
        }
        let mut edge=placement(9,cube,MaterialClass::Dielectric);edge.model=Mat4::from_translation(Vec3::new(28.,0.,0.));instances.push(edge);
        let mut close_armor=placement(11,plane,MaterialClass::Opaque);close_armor.model=Mat4::from_translation(Vec3::new(32.,0.,-0.000022));instances.push(close_armor);
        scene.update_instances(&instances).unwrap();scene
    }
    pub(crate) fn optical_parity_ray(id:usize)->(Vec3,Vec3,Vec3) {
        let (kind,sample)=(id/64,id%64);let mut jitter=Vec3::new(((sample%8) as f32-3.5)*0.001,((sample/8) as f32-3.5)*0.001,0.);
        let (o,d)=match kind {0=>(Vec3::new(-0.4,0.,2.),Vec3::new(0.2,0.,-1.).normalize()),1=>(Vec3::new(0.35,0.,2.),Vec3::new(0.6,0.,-1.).normalize()),
            2=>(Vec3::new(4.,0.,2.),-Vec3::Z),3=>(Vec3::new(8.,0.,1.),-Vec3::Z),4=>(Vec3::new(12.1,0.,1.),-Vec3::Z),
            5=>(Vec3::new(16.1,0.,1.),-Vec3::Z),6=>(Vec3::new(20.,0.,1.),-Vec3::Z),7=>(Vec3::new(24.2,0.1,1.),-Vec3::Z),
            8=>{jitter.x*=0.001;(Vec3::new(28.399984,0.1,2.),Vec3::new(0.6,0.,-1.).normalize())},_=>(Vec3::new(32.2,0.1,1.),-Vec3::Z)};
        (o+jitter,d,jitter)
    }
    #[test]
    fn all_optical_fixture_rays_have_the_declared_physical_termination(){
        let scene=optical_parity_fixture();
        for id in 0..640 {
            let (o,d,_)=optical_parity_ray(id);let result=scene.environment_path(o,d,glass()).unwrap();
            let expected=match id/64 {5|9=>TransmissionStatus::Blocked,6=>TransmissionStatus::Unresolved,_=>TransmissionStatus::Clear};
            assert_eq!(result.status,expected,"fixture ray {id}: {result:?}");
        }
    }
    #[test]
    fn parallel_window_measures_refracted_beer_and_preserves_exit_direction(){
        let mut scene=SceneVisibility::new();let geometry=scene.register_mesh(&pane(false)).unwrap();scene.update_instances(&[placement(1,geometry,MaterialClass::Dielectric)]).unwrap();
        let d=Vec3::new(0.2,0.,-1.).normalize();let result=scene.environment_path(Vec3::new(-0.4,0.,2.),d,glass()).unwrap();
        assert_eq!(result.status,TransmissionStatus::Clear);assert!(result.direction.distance(d)<1e-6);
        let cosine=(1.0-(1.0-d.z*d.z)/glass().ior.powi(2)).sqrt();let distance=2.0/cosine;
        assert!((result.glass_path_m-distance).abs()<1e-5);
        let f=1.0-fresnel_dielectric(-d.z,1.0,glass().ior);let expected=(-glass().absorption_per_m*distance).exp()*f*f;
        assert!(result.throughput.unwrap().distance(expected)<1e-5);
    }
    #[test]
    fn prism_exit_uses_snell_and_finite_point_connection_stays_unresolved(){
        let mut scene=SceneVisibility::new();let geometry=scene.register_mesh(&pane(true)).unwrap();scene.update_instances(&[placement(1,geometry,MaterialClass::Dielectric)]).unwrap();
        let o=Vec3::new(0.,0.,2.);let p=scene.environment_path(o,-Vec3::Z,glass()).unwrap();
        assert_eq!(p.status,TransmissionStatus::Clear,"{p:?}");assert!(p.direction.x.abs()>0.04);assert_eq!(p.steps.len(),2);
        let n=Vec3::new(-0.1,0.,1.).normalize();let expected=refracted(-Vec3::Z,n,1.52).unwrap();assert!(p.direction.distance(expected)<1e-5);
        assert_eq!(scene.transmission(VisibilityRay{origin:o,direction:-Vec3::Z,t_min:0.0001,t_max:10.},glass()).unwrap().status,TransmissionStatus::Unresolved);
    }
    #[test]
    fn total_internal_reflection_follows_actual_side_then_exits(){
        let mut scene=SceneVisibility::new();let geometry=scene.register_mesh(&pane(false)).unwrap();scene.update_instances(&[placement(1,geometry,MaterialClass::Dielectric)]).unwrap();
        let d=Vec3::new(0.6,0.,-1.).normalize();let p=scene.environment_path(Vec3::new(0.35,0.,2.),d,glass()).unwrap();
        assert_eq!(p.status,TransmissionStatus::Clear);assert_eq!(p.steps.iter().filter(|s|s.event=="total_internal_reflection").count(),1);
        assert!(p.direction.distance(Vec3::new(-d.x,d.y,d.z))<1e-5);
    }
    #[test]
    fn secondary_source_triangle_exclusion_keeps_micrometre_exits_and_neighboring_armor(){
        let mut scene=SceneVisibility::new();let geometry=scene.register_mesh(&pane(false)).unwrap();
        let mut thin=placement(1,geometry,MaterialClass::Dielectric);
        thin.model=Mat4::from_scale(Vec3::new(1.,1.,0.00002));scene.update_instances(&[thin]).unwrap();
        let p=scene.environment_path(Vec3::new(0.2,0.1,1.),-Vec3::Z,glass()).unwrap();
        assert_eq!(p.status,TransmissionStatus::Clear);assert_eq!(p.steps.len(),2);
        assert!((p.glass_path_m-0.00004).abs()<1e-7,"40 micrometre slab exit: {p:?}");
        let plane=scene.register_mesh(&mesh(&quad([Vec3::new(-1.,-1.,0.),Vec3::new(1.,-1.,0.),Vec3::new(1.,1.,0.),Vec3::new(-1.,1.,0.)],Vec3::Z))).unwrap();
        let mut armor=placement(2,plane,MaterialClass::Opaque);armor.model=Mat4::from_translation(Vec3::new(0.,0.,-0.000022));
        scene.update_instances(&[thin,armor]).unwrap();
        assert_eq!(scene.environment_path(Vec3::new(0.2,0.1,1.),-Vec3::Z,glass()).unwrap().status,TransmissionStatus::Blocked);
        scene.update_instances(&[placement(1,geometry,MaterialClass::Dielectric)]).unwrap();
        let d=Vec3::new(0.6,0.,-1.).normalize();let p=scene.environment_path(Vec3::new(0.399984,0.1,2.),d,glass()).unwrap();
        assert_eq!(p.status,TransmissionStatus::Clear,"{p:?}");
        assert_eq!(p.steps.iter().filter(|s|s.event=="total_internal_reflection").count(),1,"side exit remains present: {p:?}");
        assert!(p.steps[1].point.distance(p.steps[0].point)<0.0001);
        assert!(p.direction.distance(Vec3::new(-d.x,d.y,d.z))<1e-5);
    }
    #[test]
    fn finite_lens_and_mirror_are_directional_and_source_occluders_remain(){
        let mut scene=SceneVisibility::new();let geometry=scene.register_mesh(&mesh(&quad([Vec3::new(-1.,-1.,0.),Vec3::new(1.,-1.,0.),Vec3::new(1.,1.,0.),Vec3::new(-1.,1.,0.)],Vec3::Z))).unwrap();
        let mut lens=placement(1,geometry,MaterialClass::ThinLens);lens.lens_center_focal=[0.,0.,0.,0.5];scene.update_instances(&[lens]).unwrap();
        let p=scene.environment_path(Vec3::new(0.1,0.,1.),-Vec3::Z,glass()).unwrap();assert_eq!(p.status,TransmissionStatus::Clear);
        assert!(p.direction.distance(Vec3::new(-0.2,0.,-1.).normalize())<1e-6);assert_eq!(p.throughput,Some(Vec3::splat(0.96)));
        let reverse=scene.environment_path(p.origin+p.direction*0.5,-p.direction,glass()).unwrap();assert!(reverse.direction.distance(Vec3::Z)<1e-6);
        let mut armor=placement(2,geometry,MaterialClass::Opaque);armor.model=Mat4::from_translation(-Vec3::Z*0.5);
        scene.update_instances(&[lens,armor]).unwrap();assert_eq!(scene.environment_path(Vec3::new(0.1,0.,1.),-Vec3::Z,glass()).unwrap().status,TransmissionStatus::Blocked);
        let mirror=placement(1,geometry,MaterialClass::Mirror);scene.update_instances(&[mirror]).unwrap();let p=scene.environment_path(Vec3::Z,-Vec3::Z,glass()).unwrap();assert_eq!(p.direction,Vec3::Z);assert_eq!(p.throughput,Some(Vec3::splat(0.9)));
    }
}
