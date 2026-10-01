//! Exterior burning residue / escaping gas on source-mesh surfaces. Steel is not fuel.
//! The graph is a 22 cm source-triangle sampling, with a maximum 48 cm connection gap;
//! heat spread and fuel distribution are authored responses to damage, not combustion CFD.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec3};
use std::collections::{BinaryHeap, HashMap};

#[derive(Clone, Copy, Debug)]
pub struct Patch { pub position: Vec3, pub normal: Vec3, pub strength: f32, pub id: usize }
#[derive(Default)]
pub struct Surface { points: Vec<(Vec3, Vec3)>, edges: Vec<Vec<(usize, f32)>> }
#[derive(Clone, Default)]
pub struct Burn { distance: Vec<f32> }

fn key(p: Vec3) -> (i32, i32, i32) {
    let p = (p / 0.22).floor().as_ivec3(); (p.x,p.y,p.z)
}
impl Surface {
    pub fn from_mesh(mesh: &CpuMesh) -> Self {
        let mut bins: HashMap<_,Vec<usize>> = HashMap::new();
        let mut points: Vec<(Vec3,Vec3)> = Vec::new();
        for tri in mesh.indices.chunks_exact(3) {
            let [a,b,c] = std::array::from_fn(|i|Vec3::from_array(mesh.vertices[tri[i] as usize].pos));
            let geometric = (b-a).cross(c-a).normalize_or_zero();
            if geometric.length_squared()<0.5 {continue;}
            let normal = tri.iter().map(|&i|Vec3::from_array(mesh.vertices[i as usize].normal)).sum::<Vec3>().normalize_or_zero();
            let normal = if normal.length_squared()>0.5 {normal} else {geometric};
            // Underbody is neither an exposed leak nor an upward hot-gas wash surface.
            if normal.y < -0.65 {continue;}
            let divisions = ((a.distance(b).max(b.distance(c)).max(c.distance(a))/0.22).ceil() as usize).clamp(1,96);
            for i in 0..=divisions { for j in 0..=divisions-i {
                let p = a+(b-a)*(i as f32/divisions as f32)+(c-a)*(j as f32/divisions as f32);
                let bin = bins.entry(key(p)).or_default();
                if bin.iter().any(|&k|points[k].1.dot(normal)>0.75) {continue;}
                bin.push(points.len()); points.push((p,normal));
            }}
        }
        let mut edges = vec![Vec::new();points.len()];
        for (i,&(p,n)) in points.iter().enumerate() {
            let (x,y,z)=key(p);
            for dz in -3..=3 {for dy in -3..=3 {for dx in -3..=3 {
                if let Some(ids)=bins.get(&(x+dx,y+dy,z+dz)) {for &j in ids {
                    let (q,m)=points[j]; let d=p.distance(q);
                    // Do not cross directly between opposed faces of a thin steel plate.
                    if i!=j && d<=0.48 && n.dot(m)>-0.20 {edges[i].push((j,d));}
                }}
            }}}
        }
        Self { points,edges }
    }
    pub fn ignite(&self, source: Vec3) -> Burn {
        let mut distance=vec![f32::INFINITY;self.points.len()];
        let Some(root)=self.points.iter().enumerate().min_by(|a,b|
            a.1.0.distance_squared(source).total_cmp(&b.1.0.distance_squared(source))).map(|v|v.0) else {return Burn{distance};};
        let mut heap=BinaryHeap::new(); distance[root]=0.0;
        heap.push((std::cmp::Reverse(0u32),root));
        while let Some((std::cmp::Reverse(bits),i))=heap.pop() {
            let d=f32::from_bits(bits); if d>distance[i] {continue;}
            for &(j,gap) in &self.edges[i] {
                let next=d+gap;
                if next<distance[j] {distance[j]=next;heap.push((std::cmp::Reverse(next.to_bits()),j));}
            }
        }
        Burn {distance}
    }
    pub fn patches(&self,burn:&Burn,model:Mat4,age:f32,load:f32,strength:f32)->Vec<Patch> {
        if strength<=0.0 || age<0.0 {return Vec::new();}
        let speed=0.24+0.25*load.clamp(0.0,1.0);
        let mut candidates:Vec<_>=self.points.iter().enumerate().filter_map(|(i,&(p,n))| {
            let distance=*burn.distance.get(i)?;
            let since=age-distance/speed;
            if since<0.0 || !since.is_finite(){return None;}
            let growth=(since/0.8).clamp(0.0,1.0);
            let local_fuel=(-distance/5.0).exp();
            let pulse=0.80+0.20*(age*2.3+i as f32*1.73).sin();
            let intensity=strength*growth*local_fuel*pulse;
            (intensity>0.02).then_some((i,p,n,intensity,distance))
        }).collect();
        // Hot entry remains visible; later roots occupy separate surface area rather than
        // stacking many emitters at the original breach.
        candidates.sort_by(|a,b|a.4.total_cmp(&b.4));
        let mut selected:Vec<Patch>=Vec::new();
        while !candidates.is_empty() && selected.len()<12 {
            let best=if selected.is_empty() {0} else {
                candidates.iter().enumerate().max_by(|(_,a),(_,b)| {
                    let score=|v:&(usize,Vec3,Vec3,f32,f32)| {
                        let p=model.transform_point3(v.1);
                        selected.iter().map(|q|q.position.distance_squared(p)).fold(f32::INFINITY,f32::min)*v.3.sqrt()
                    };
                    score(a).total_cmp(&score(b))
                }).unwrap().0
            };
            let (i,p,n,intensity,_)=candidates.swap_remove(best);
            let position=model.transform_point3(p);
            if selected.iter().any(|q|q.position.distance_squared(position)<0.60_f32.powi(2)) {continue;}
            selected.push(Patch{position,normal:model.transform_vector3(n).normalize_or_zero(),strength:intensity,id:i});
        }
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fire_spreads_from_damage_around_real_horizontal_and_vertical_surfaces() {
        let mesh=crate::prims::box_mesh(Vec3::new(-1.0,0.0,-1.0),Vec3::new(1.0,1.0,1.0),[0;3]);
        let surface=Surface::from_mesh(&mesh);
        let burn=surface.ignite(Vec3::new(1.0,0.7,0.0));
        let young=surface.patches(&burn,Mat4::IDENTITY,0.5,0.8,1.0);
        let old=surface.patches(&burn,Mat4::IDENTITY,8.0,0.8,1.0);
        assert!(!young.is_empty()); assert!(old.len()>young.len());
        assert!(old.iter().any(|p|p.normal.y>0.8));
        assert!(old.iter().any(|p|p.normal.y.abs()<0.2));
        for p in old { assert!((p.position.x.abs()-1.0).abs()<0.001 || (p.position.z.abs()-1.0).abs()<0.001 || (p.position.y-1.0).abs()<0.001); }
        assert!(surface.patches(&burn,Mat4::IDENTITY,8.0,0.8,0.0).is_empty());
    }
    #[test]
    fn disconnected_surface_cannot_ignite_across_empty_space() {
        let mut mesh=crate::prims::box_mesh(Vec3::ZERO,Vec3::ONE,[0;3]);
        let other=crate::prims::box_mesh(Vec3::splat(4.0),Vec3::splat(5.0),[0;3]);
        let base=mesh.vertices.len() as u32;
        mesh.vertices.extend(other.vertices);mesh.indices.extend(other.indices.into_iter().map(|i|i+base));
        let surface=Surface::from_mesh(&mesh);let burn=surface.ignite(Vec3::Y);
        assert!(surface.patches(&burn,Mat4::IDENTITY,100.0,1.0,1.0).iter().all(|p|p.position.max_element()<1.01));
    }
}
