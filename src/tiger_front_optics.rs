//! Front observation optics for the raster renderer. Entries are retained source faces, never
//! screens placed in front of armor. The radio roof pane was opaque in the original export;
//! partition its two actual triangles so the lid does not draw over its own live image.
use crate::gltf_load::{load_glb_partitioned_subtrees, CpuMesh};
use crate::renderer::RasterOpticSpec;
use glam::Vec3;

fn triangle(mesh: &CpuMesh, tri: &[u32]) -> [Vec3; 3] {
    [0, 1, 2].map(|k| Vec3::from(mesh.vertices[tri[k] as usize].pos))
}
fn normal(p: [Vec3; 3]) -> Vec3 { (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero() }

#[derive(Clone, Copy)]
struct Bore { center: Vec3, axis: Vec3, radius: f32, start: f32, end: f32 }
impl Bore {
    fn basis(self) -> (Vec3,Vec3) { (Vec3::X,self.axis.cross(Vec3::X).normalize()) }
    fn planes(self) -> Vec<(Vec3,f32)> {
        let (u,v)=self.basis();
        let mut planes:Vec<_>=(0..24).map(|k| {let angle=k as f32*std::f32::consts::TAU/24.0;
            let n=u*angle.cos()+v*angle.sin();(n,n.dot(self.center)+self.radius)}).collect();
        planes.push((self.axis,self.axis.dot(self.center)+self.end));
        planes.push((-self.axis,(-self.axis).dot(self.center)-self.start));
        planes
    }
}
fn ray(mesh:&CpuMesh,o:Vec3,d:Vec3)->Vec<f32> {
    mesh.indices.chunks_exact(3).filter_map(|tri|{
        let [a,b,c]=triangle(mesh,tri);let e1=b-a;let e2=c-a;let p=d.cross(e2);let det=e1.dot(p);
        if det.abs()<1e-8{return None;} let s=o-a;let u=s.dot(p)/det;let q=s.cross(e1);
        let v=d.dot(q)/det;let t=e2.dot(q)/det;
        (u>=0.0&&v>=0.0&&u+v<=1.0&&t>0.00001&&t<1.0).then_some(t)
    }).collect()
}
fn mg_source() -> anyhow::Result<(Bore,[CpuMesh;2])> {
    let mut parts=load_glb_partitioned_subtrees("assets/tanks/tiger/baked/tiger_hull.glb",
        &["Hull_0241_housing","Hull_0200","Hull_0175"])?;
    let outer=parts.pop().unwrap();let inner=parts.pop().unwrap();let scope=parts.pop().unwrap();
    let rim=selected(&scope,|p| {
        normal(p).z>0.99&&p.iter().all(|v|v.z>2.339)
    });
    anyhow::ensure!(!rim.indices.is_empty(),"fixed MG objective rim missing");
    let axis=normal(triangle(&rim,&rim.indices[..3]));let center=rim.center();
    let radius=(rim.max.x-rim.min.x)*0.5;
    anyhow::ensure!((center-Vec3::new(-0.547085,1.781586,2.340364)).length()<0.001
        && (0.011..0.012).contains(&radius),"MG source objective dimensions changed");
    let mut bore=Bore{center,axis,radius,start:f32::INFINITY,end:f32::NEG_INFINITY};
    let (u,v)=bore.basis();
    for ring in [0.0,0.75,1.0] {for k in 0..24 {
        let angle=k as f32*std::f32::consts::TAU/24.0;
        let o=center+(u*angle.cos()+v*angle.sin())*radius*ring;
        for source in [&inner,&outer] {for t in ray(source,o,axis) {bore.start=bore.start.min(t);bore.end=bore.end.max(t);}}
    }}
    anyhow::ensure!(bore.start>0.04&&bore.end<0.25&&bore.end>bore.start,"MG ball source intersection moved");
    bore.start-=0.0005;bore.end+=0.0005;
    Ok((bore,[inner,outer]))
}
fn key(p:[Vec3;3])->[[i32;3];3] {
    let mut k=p.map(|v|v.to_array().map(|x|(x*100000.0).round() as i32));k.sort();k
}
/// Reconstruct the missing finite telescope bore only in the two exact named ball components.
/// All original assets, other hull triangles, texture pages and interpolated UVs are preserved.
pub(super) fn repair_mg_bore(mut hull:CpuMesh)->anyhow::Result<CpuMesh> {
    let (bore,source)=mg_source()?;
    let source_keys:std::collections::HashSet<_>=source.iter().flat_map(|mesh| {
            mesh.indices.chunks_exact(3)
        .map(|tri|key(triangle(mesh,tri)))
        }).collect();
    let planes=bore.planes();let mut vertices= hull.vertices.clone();let mut indices=Vec::new();
    let mut touched=0;let mut wall_sample=None;
    for tri in hull.indices.chunks_exact(3) {
        let p=triangle(&hull,tri);let polygon=tri.iter().map(|&i|hull.vertices[i as usize]).collect::<Vec<_>>();
        if !source_keys.contains(&key(p)) {
            indices.extend_from_slice(tri);continue;}
        let intersects=planes.iter().all(|(n,d)|p.iter().any(|point|n.dot(*point)<=*d));
        if !intersects {
            indices.extend_from_slice(tri);continue;}
        touched+=1;wall_sample=Some(polygon[0]);let mut remaining=polygon;
        for &(n,d) in &planes {
            super::tiger_driver_visor::append_polygon(&mut vertices,&mut indices,
                &super::tiger_driver_visor::half_polygon(&remaining,n,d,false));
            remaining=super::tiger_driver_visor::half_polygon(&remaining,n,d,true);
            if remaining.is_empty(){break;}
        }
    }
    anyhow::ensure!(touched>0,"MG bore repair did not find its exact source ball triangles");
    // Finite metal bore walls hide the thickness of the two clipped ball skins. They use a
    // preserved source material sample, not an unlit visual overlay.
    let sample=wall_sample.unwrap();let (u,v)=bore.basis();
    for k in 0..24 {
        let make=|i:usize,t:f32| {let a=(i as f32+0.5)*std::f32::consts::TAU/24.0;
            let radial=u*a.cos()+v*a.sin();let mut vertex=sample;
            vertex.pos=(bore.center+bore.axis*t+radial*(bore.radius/(std::f32::consts::PI/24.0).cos())).to_array();
            vertex.normal=(-radial).to_array();vertex};
        let polygon=[make(k,bore.start),make(k+1,bore.start),make(k+1,bore.end),make(k,bore.end)];
        super::tiger_driver_visor::append_polygon(&mut vertices,&mut indices,&polygon);
    }
    hull.vertices=vertices;hull.indices=indices;
    Ok(hull)
}
fn selected(mesh: &CpuMesh, predicate: impl Fn([Vec3; 3]) -> bool) -> CpuMesh {
    let mut part = mesh.clone();
    part.indices.clear();
    for tri in mesh.indices.chunks_exact(3) {
        if predicate(triangle(mesh, tri)) { part.indices.extend_from_slice(tri); }
    }
    part.min = Vec3::splat(f32::INFINITY); part.max = Vec3::splat(f32::NEG_INFINITY);
    for &i in &part.indices { let p = Vec3::from(part.vertices[i as usize].pos); part.min = part.min.min(p); part.max = part.max.max(p); }
    part
}
fn radio_entry(p: [Vec3; 3]) -> bool {
    let n = normal(p);
    n.x > 0.86 && n.z < -0.49 && p.iter().all(|v| {
            v.y >= 1.7809 && v.y <= 1.8493
        && v.x >= -0.9373 && v.x <= -0.8707 && v.z >= 1.9380 && v.z <= 2.0529
        })
}

pub(super) fn split_radio_roof_pane(lid: &mut CpuMesh) -> anyhow::Result<CpuMesh> {
    let pane = selected(lid, radio_entry);
    anyhow::ensure!(pane.indices.len() == 6, "radio roof eyepiece source changed: expected two actual face triangles");
    lid.indices = lid.indices.chunks_exact(3).filter(|tri| !radio_entry(triangle(lid, tri)))
        .flatten().copied().collect();
    Ok(pane)
}

fn spec(entry: &CpuMesh, mesh: usize, objective: Vec3, objective_outward: Vec3) -> anyhow::Result<RasterOpticSpec> {
    anyhow::ensure!(!entry.indices.is_empty(), "missing source optical entry");
    let entry_outward = -normal(triangle(entry, &entry.indices[..3]));
    let up = (Vec3::Y - entry_outward * Vec3::Y.dot(entry_outward)).normalize();
    let right = up.cross(entry_outward).normalize();
    let center = entry.center();
    let mut min = glam::Vec2::splat(f32::INFINITY); let mut max = glam::Vec2::splat(f32::NEG_INFINITY);
    for &i in &entry.indices {
        let d = Vec3::from(entry.vertices[i as usize].pos) - center;
        let p = glam::Vec2::new(d.dot(right), d.dot(up)); min = min.min(p); max = max.max(p);
    }
    let corners = [(min.x,min.y),(max.x,min.y),(max.x,max.y),(min.x,max.y)]
        .map(|(x,y)| center + right*x + up*y);
    Ok(RasterOpticSpec {mesh,
        entry_indices: 0..6,
        entry_corners:corners,entry_outward,objective,objective_outward,
        preserve_ray_direction:false,eyepiece:None})
}

pub(super) fn raster_specs(driver: &CpuMesh, radio: &CpuMesh, fixed: &CpuMesh,
    slots: [usize; 3]) -> anyhow::Result<[RasterOpticSpec; 3]> {
    let mut source = load_glb_partitioned_subtrees("assets/tanks/tiger/baked/tiger_hull.glb",
        &["Hull_0300_housing", "Hull_0299", "Hull_0241_housing"])?;
    let _fixed_housing = source.pop().unwrap(); let radio_housing = source.pop().unwrap();
    let driver_housing = source.pop().unwrap();
    // Upper forward faces above the roof, not the lower mechanical slit hidden behind the lid.
    let upper_face = |mesh: &CpuMesh, sign: f32| {
        selected(mesh, |p| {
        let n = normal(p); n.x * sign > 0.86 && n.z > 0.49
            && p.iter().all(|v| v.y >= 2.0514) && p.iter().any(|v| v.y > 2.10)
    })
    };
    let driver_exit = upper_face(&driver_housing, 1.0);
    let radio_exit = upper_face(&radio_housing, -1.0);
    anyhow::ensure!(driver_exit.indices.len() == 6 && radio_exit.indices.len() == 6,
        "roof objective source face topology changed");
    let (bore,_)=mg_source()?;
    Ok([
        spec(driver,slots[0],driver_exit.center(),Vec3::new(0.8660254,0.0,0.5))?,
        spec(radio,slots[1],radio_exit.center(),Vec3::new(-0.8660254,0.0,0.5))?,
        spec(fixed,slots[2],bore.center+bore.axis*bore.end,bore.axis)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn front_optic_partition_preserves_every_source_triangle_and_registers_three_views() {
        let (_, mut lids, _, _, _, _, _, _) = super::super::tiger_hatches::load_player_tiger().unwrap();
        let original = lids[2].clone();
        let radio = split_radio_roof_pane(&mut lids[2]).unwrap();
        assert_eq!(lids[2].indices.len() + radio.indices.len(), original.indices.len());
        let (glass, fixed, driver) = super::super::tiger_driver_visor::load_optics().unwrap();
        assert!(!glass.indices.is_empty());
        let specs = raster_specs(&driver,&radio,&fixed,[0,1,2]).unwrap();
        for (index,spec) in specs.iter().enumerate() {
            assert_eq!(spec.mesh,index);
            assert!(spec.entry_outward.z > 0.49);
            assert!(spec.objective.is_finite());
            assert!(spec.entry_corners.iter().all(|v|v.is_finite()));
        }
        assert!(specs[0].objective.y > 2.07 && specs[1].objective.y > 2.07);
        assert_eq!(radio.indices.len(),6);
        assert!((radio.center().y-driver.center().y).abs()<1e-5);
        // Radio lid includes a small authored offset; reflecting the driver pane would miss it.
        assert!((radio.center().x+driver.center().x).abs()>0.002);
    }
    #[test]
    fn front_optic_mg_bore_opens_a_finite_source_registered_rayfield_and_preserves_surrounding_armor() {
        let (bore,source)=mg_source().unwrap();let (u,v)=bore.basis();
        let (tiger,_,_,_,_,_, _, _)=super::super::tiger_hatches::load_player_tiger().unwrap();
        let repaired=&tiger[0];
        for ring in [0.0,0.4,0.8] {for k in 0..24 {
            let a=k as f32*std::f32::consts::TAU/24.0;
            let o=bore.center+(u*a.cos()+v*a.sin())*bore.radius*ring;
            assert!(source.iter().any(|mesh|!ray(mesh,o,bore.axis).is_empty()));
            assert!(ray(repaired,o,bore.axis).is_empty(),"bore ray ring {ring}, azimuth {k} blocked");
        }}
        for k in 0..24 {
            let a=k as f32*std::f32::consts::TAU/24.0;
            let o=bore.center+(u*a.cos()+v*a.sin())*bore.radius*1.4;
            assert!(!ray(repaired,o,bore.axis).is_empty(),"armor outside aperture was removed at {k}");
        }
    }
}
