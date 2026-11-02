//! BVH acceleration of an unchanged source-point support query. Every input point is retained.
//! This does not repair the sampling limitations of the caller's vertex-based contact model.
//! The owner must keep the source slice immutable after construction; only its length is checked
//! on queries. A rectangle bound must include every height_at result within its closed rectangle.
//! A nonfinite rectangle bound disables pruning for that branch.
use glam::{Mat4, Vec2, Vec3};

const LEAF_POINTS:usize=32;
#[derive(Clone,Copy)]
struct Node {min:Vec3,max:Vec3,first:u32,count:u32,children:[u32;2]}
pub struct SourcePointBvh {order:Vec<u32>,nodes:Vec<Node>}
#[derive(Clone,Copy,Debug)]
pub struct Contact {pub source_index:usize,pub offset:Vec3,pub required_origin_y:f32}
#[derive(Clone,Copy,Debug,Default)]
pub struct QueryStats {pub visited_nodes:usize,pub pruned_nodes:usize,pub tested_points:usize,pub bound_queries:usize}
pub struct SupportQuery {
    pub required_origin_y:f32,
    pub contacts:Vec<Contact>,
    /// Conservative transformed source-AABB radius, for numerical angular-step bounds only.
    pub radius_upper_m:f32,
    pub stats:QueryStats,
}

impl SourcePointBvh {
    pub fn new(points:&[Vec3])->Option<Self> {
        if points.is_empty()||points.len()>u32::MAX as usize||points.iter().any(|p|!p.is_finite()) {return None;}
        let mut result=Self{order:(0..points.len() as u32).collect(),nodes:Vec::new()};
        build(points,&mut result.order,0,&mut result.nodes);Some(result)
    }
    pub fn len(&self)->usize {self.order.len()}
    pub fn bounds(&self)->(Vec3,Vec3) {(self.nodes[0].min,self.nodes[0].max)}
    /// offset_model maps source points to world-oriented offsets from the hull origin, including
    /// local articulation but excluding hull world XYZ. origin_xz is added at terrain queries.
    /// Results and contacts match brute-force height_at(origin_xz+offset.xz)-offset.y, including
    /// the exact f32 `best - candidate <= contact_band_m` boundary. Contacts retain source order.
    pub fn query(&self,points:&[Vec3],offset_model:Mat4,origin_xz:Vec2,contact_band_m:f32,
        height_at:&impl Fn(f32,f32)->f32,
        max_ground_height:&impl Fn(Vec2,Vec2)->f32)->Option<SupportQuery> {
        if points.len()!=self.order.len()||!offset_model.is_finite()||!origin_xz.is_finite()
            ||!contact_band_m.is_finite()||contact_band_m<0.0
            ||offset_model.x_axis.w!=0.0||offset_model.y_axis.w!=0.0||offset_model.z_axis.w!=0.0||offset_model.w_axis.w!=1.0 {return None;}
        let (lo,hi)=transformed_bounds(self.nodes[0],offset_model);
        let r=lo.abs().max(hi.abs()).as_dvec3().length();
        let radius_upper_m=round_up(r);if !radius_upper_m.is_finite(){return None;}
        let mut stats=QueryStats::default();
        // First establish the exact maximum, then enumerate its band. This avoids storing and
        // repeatedly filtering an order-dependent cloud of provisional contacts.
        let (best,_)=self.pass(points,offset_model,origin_xz,0.0,None,height_at,max_ground_height,&mut stats);
        if !best.is_finite(){return None;}
        let (_,mut contacts)=self.pass(points,offset_model,origin_xz,contact_band_m,Some(best),height_at,max_ground_height,&mut stats);
        contacts.sort_unstable_by_key(|c|c.source_index);
        Some(SupportQuery{required_origin_y:best,contacts,radius_upper_m,stats})
    }
    #[allow(clippy::too_many_arguments)]
    fn pass(&self,points:&[Vec3],model:Mat4,at:Vec2,band:f32,fixed:Option<f32>,
        height_at:&impl Fn(f32,f32)->f32,bound:&impl Fn(Vec2,Vec2)->f32,stats:&mut QueryStats)->(f32,Vec<Contact>) {
        let mut best=fixed.unwrap_or(f32::NEG_INFINITY);let mut contacts=Vec::new();
        let mut stack=vec![(0_u32,node_upper(self.nodes[0],model,at,bound,stats))];
        while let Some((index,upper))=stack.pop() {
            stats.visited_nodes+=1;
            let pruned=if fixed.is_some(){best-upper>band}else{upper<=best};
            if pruned {stats.pruned_nodes+=1;continue;}
            let node=self.nodes[index as usize];
            if node.count!=0 {
                for &source_index in &self.order[node.first as usize..(node.first+node.count) as usize] {
                    stats.tested_points+=1;
                    let offset=model.transform_point3(points[source_index as usize]);
                    if !offset.is_finite(){continue;}
                    let h=height_at(at.x+offset.x,at.y+offset.z);
                    let required=h-offset.y;
                    if !h.is_finite()||!required.is_finite(){continue;}
                    if fixed.is_some() {
                        if best-required<=band {contacts.push(Contact{source_index:source_index as usize,offset,required_origin_y:required});}
                    } else {best=best.max(required);}
                }
            } else {
                let [a,b]=node.children;
                let ua=node_upper(self.nodes[a as usize],model,at,bound,stats);
                let ub=node_upper(self.nodes[b as usize],model,at,bound,stats);
                // Visit the larger possible support first, improving pruning without changing it.
                if ua>=ub {stack.push((b,ub));stack.push((a,ua));}
                else {stack.push((a,ua));stack.push((b,ub));}
            }
        }
        (best,contacts)
    }
}

fn build(points:&[Vec3],order:&mut[u32],first:u32,nodes:&mut Vec<Node>)->u32 {
    let (min,max)=order.iter().fold((Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)),
        |(lo,hi),&i|(lo.min(points[i as usize]),hi.max(points[i as usize])));
    let index=nodes.len() as u32;
    nodes.push(Node{min,max,first,count:order.len() as u32,children:[0;2]});
    if order.len()>LEAF_POINTS {
        let size=max-min;let axis=if size.x>=size.y&&size.x>=size.z {0}else if size.y>=size.z {1}else{2};
        let mid=order.len()/2;
        order.select_nth_unstable_by(mid,|&a,&b|points[a as usize][axis].total_cmp(&points[b as usize][axis]).then(a.cmp(&b)));
        let (a,b)=order.split_at_mut(mid);
        let children=[build(points,a,first,nodes),build(points,b,first+mid as u32,nodes)];
        nodes[index as usize].count=0;nodes[index as usize].children=children;
    }
    index
}

fn node_upper(node:Node,model:Mat4,at:Vec2,bound:&impl Fn(Vec2,Vec2)->f32,stats:&mut QueryStats)->f32 {
    let (lo,hi)=transformed_bounds(node,model);
    // These are the same f32 additions as the actual point query, rounded outward once more.
    let min=Vec2::new(next_down(at.x+lo.x),next_down(at.y+lo.z));
    let max=Vec2::new(next_up(at.x+hi.x),next_up(at.y+hi.z));
    if !min.is_finite()||!max.is_finite(){return f32::INFINITY;}
    stats.bound_queries+=1;let ground=bound(min,max);
    if !ground.is_finite(){return f32::INFINITY;}
    let upper=next_up(ground-lo.y);if upper.is_finite(){upper}else{f32::INFINITY}
}

fn transformed_bounds(node:Node,model:Mat4)->(Vec3,Vec3) {
    let columns=[model.x_axis,model.y_axis,model.z_axis];
    let mut lo=Vec3::ZERO;let mut hi=Vec3::ZERO;
    for row in 0..3 {
        let t=model.w_axis[row] as f64;let (mut a,mut b,mut magnitude)=(t,t,t.abs());
        for column in 0..3 {
            let c=columns[column][row] as f64;
            let x=c*node.min[column] as f64;let y=c*node.max[column] as f64;
            a+=x.min(y);b+=x.max(y);magnitude+=x.abs().max(y.abs());
        }
        // Includes product/addition or fused-operation rounding in the actual f32 Mat4 query.
        // 8*EPS exceeds gamma(6) for 3 products + 3 additions; absolute term covers subnormals.
        let error=8.0*f32::EPSILON as f64*magnitude+1e-37;
        lo[row]=round_down(a-error);hi[row]=round_up(b+error);
    }
    (lo,hi)
}
fn next_up(x:f32)->f32 {if x==f32::INFINITY{return x;}if x==0.0{return f32::from_bits(1);}f32::from_bits(if x>0.0{x.to_bits()+1}else{x.to_bits()-1})}
fn next_down(x:f32)->f32 {if x==f32::NEG_INFINITY{return x;}if x==0.0{return f32::from_bits(0x80000001);}f32::from_bits(if x>0.0{x.to_bits()-1}else{x.to_bits()+1})}
fn round_up(x:f64)->f32 {let f=x as f32;if (f as f64)<x {next_up(f)}else{f}}
fn round_down(x:f64)->f32 {let f=x as f32;if (f as f64)>x {next_down(f)}else{f}}

#[cfg(test)] mod tests {
    use super::*;
    use glam::Quat;
    fn points()->Vec<Vec3> {(0..4096).map(|i|Vec3::new(((i*37)%127) as f32*0.07-4.,
        (i%64) as f32*0.12,((i*19)%113) as f32*0.08-4.)).collect()}
    fn compare(points:&[Vec3],model:Mat4,at:Vec2,band:f32,height:&impl Fn(f32,f32)->f32,bound:&impl Fn(Vec2,Vec2)->f32)->SupportQuery {
        let source=SourcePointBvh::new(points).unwrap();assert_eq!(source.len(),points.len());
        let all:Vec<_>=points.iter().map(|p|model.transform_point3(*p)).map(|p|(p,height(at.x+p.x,at.y+p.z)-p.y)).collect();
        let best=all.iter().map(|(_,h)|*h).filter(|h|h.is_finite()).fold(f32::NEG_INFINITY,f32::max);
        let expected:Vec<_>=all.iter().enumerate().filter(|(_,(_,h))|best-*h<=band).map(|(i,_)|i).collect();
        let result=source.query(points,model,at,band,height,bound).unwrap();
        assert_eq!(result.required_origin_y.to_bits(),best.to_bits());
        assert_eq!(result.contacts.iter().map(|c|c.source_index).collect::<Vec<_>>(),expected);
        for c in &result.contacts {assert_eq!(c.offset,all[c.source_index].0);assert_eq!(c.required_origin_y,all[c.source_index].1);}
        assert!(all.iter().all(|(p,_)|p.as_dvec3().length()<=result.radius_upper_m as f64));result
    }
    #[test] fn exact_support_and_contacts_on_uneven_rotated_translated_surface() {
        let p=points();let h=|x:f32,z:f32|(0.17*x+0.08*z).max(-0.11*x+0.04*z+0.3);
        let bound=|a:Vec2,b:Vec2|[h(a.x,a.y),h(a.x,b.y),h(b.x,a.y),h(b.x,b.y)].into_iter().fold(f32::NEG_INFINITY,f32::max);
        for at in [Vec2::ZERO,Vec2::new(5250.,-4800.)] {
            let m=Mat4::from_scale_rotation_translation(Vec3::new(1.2,0.7,0.9),Quat::from_euler(glam::EulerRot::YXZ,0.7,-0.35,0.4),Vec3::new(0.3,1.7,-0.2));
            compare(&p,m,at,0.125,&h,&bound);
        }
    }
    #[test] fn equal_contact_band_and_duplicate_source_points_are_retained() {
        let p:Vec<_>=(0..256).map(|i|Vec3::new((i%8) as f32,if i%3==0 {0.125}else{0.},0.)).collect();
        let r=compare(&p,Mat4::IDENTITY,Vec2::ZERO,0.125,&|_,_|1.,&|_,_|1.);
        assert_eq!(r.contacts.len(),p.len());
    }
    #[test] fn broad_and_unknown_rectangle_bounds_preserve_brute_force_result() {
        let p=points();for upper in [1e6,f32::INFINITY,f32::NAN] {
            compare(&p,Mat4::from_rotation_z(0.2),Vec2::new(-30.,41.),0.03,&|x,z|0.03*x-0.07*z,&|_,_|upper);
        }
    }
    #[test] fn conservative_branch_pruning_reduces_actual_point_queries() {
        let p=points();let r=compare(&p,Mat4::IDENTITY,Vec2::ZERO,0.001,&|_,_|0.,&|_,_|0.);
        assert!(r.stats.pruned_nodes>0);assert!(r.stats.tested_points<p.len(),"two passes must still beat one brute-force scan: {:?}",r.stats);
    }
    #[test] fn transformed_boxes_contain_every_actual_point_under_large_cancellation() {
        let p=points();let tree=SourcePointBvh::new(&p).unwrap();
        let m=Mat4::from_scale_rotation_translation(Vec3::new(-1.7,0.4,2.1),Quat::from_euler(glam::EulerRot::YXZ,2.1,1.2,-0.8),Vec3::new(10000.,-5000.,9000.));
        for n in &tree.nodes {let (lo,hi)=transformed_bounds(*n,m);
            if n.count!=0 {for &i in &tree.order[n.first as usize..(n.first+n.count) as usize] {
                let v=m.transform_point3(p[i as usize]);assert!(v.cmpge(lo).all()&&v.cmple(hi).all());
            }}
        }
        assert!(tree.query(&p[..p.len()-1],m,Vec2::ZERO,0.,&|_,_|0.,&|_,_|0.).is_none());
    }
}
