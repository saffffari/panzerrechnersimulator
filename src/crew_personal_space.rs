//! Living crew contact for the interior walk. Targets are the indexed vertices currently drawn,
//! including arm skinning, body transforms, slump and visible wound masks. The walking player is
//! a source-sized vertical capsule; this is locomotion contact, not a replacement damage solver.
use super::*;

const SKIN: f32 = 0.001;
/// Below this fraction of the capsule radius from a body surface the walker is inside the body,
/// not touching it: the step is released rather than clamped so he can walk out.
const DEPENETRATION_FRACTION: f32 = 0.55;
const MAX_ADVANCE: usize = 24;
const MAX_SLIDES: usize = 4;

#[derive(Clone, Copy, Debug)]
struct Bounds { min: Vec3, max: Vec3 }
impl Bounds {
    fn empty() -> Self { Self { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) } }
    fn add(&mut self, p: Vec3) { self.min=self.min.min(p); self.max=self.max.max(p); }
    fn overlaps(self, b: Self) -> bool { self.min.cmple(b.max).all() && b.min.cmple(self.max).all() }
}
struct Node { bounds: Bounds, range: std::ops::Range<usize>, children: Option<(usize,usize)> }
struct Soup { triangles: Vec<[Vec3;3]>, nodes: Vec<Node> }
impl Soup {
    fn new(mut triangles: Vec<[Vec3;3]>) -> Self {
        triangles.retain(|t| (t[1]-t[0]).cross(t[2]-t[0]).length_squared()>1e-12);
        let mut soup=Self {triangles,nodes:Vec::new()};
        if !soup.triangles.is_empty() { soup.node(0..soup.triangles.len()); }
        soup
    }
    fn node(&mut self, range: std::ops::Range<usize>) -> usize {
        let mut bounds=Bounds::empty(); for t in &self.triangles[range.clone()] {for &p in t {bounds.add(p);}}
        let index=self.nodes.len(); self.nodes.push(Node{bounds,range:range.clone(),children:None});
        if range.len()>8 {
            let span=bounds.max-bounds.min;
            let axis=if span.x>=span.y && span.x>=span.z {0} else if span.y>=span.z {1} else {2};
            self.triangles[range.clone()].sort_unstable_by(|a,b| {
                let ca=a[0][axis]+a[1][axis]+a[2][axis];let cb=b[0][axis]+b[1][axis]+b[2][axis];ca.total_cmp(&cb)
            });
            let mid=range.start+range.len()/2;
            let left=self.node(range.start..mid);let right=self.node(mid..range.end);
            self.nodes[index].children=Some((left,right));
        }
        index
    }
    fn candidates(&self, bounds: Bounds, out:&mut Vec<[Vec3;3]>, model:Mat4) {
        if self.nodes.is_empty() {return;}
        let mut stack=vec![0];
        while let Some(i)=stack.pop() {
            let node=&self.nodes[i];if !node.bounds.overlaps(bounds) {continue;}
            if let Some((a,b))=node.children {stack.push(b);stack.push(a);} else {
                for t in &self.triangles[node.range.clone()] {out.push(t.map(|p|model.transform_point3(p)));}
            }
        }
    }
}
struct Source { positions: Vec<Vec3>, indices: Vec<u32> }
impl Source {
    fn load(path:&str)->anyhow::Result<Self> {
        let m=crate::gltf_load::load_glb(path)?;
        anyhow::ensure!(m.indices.len()/3<=1200,"crew source exceeds reviewed contact budget");
        Ok(Self{positions:m.vertices.iter().map(|v|Vec3::from_array(v.pos)).collect(),indices:m.indices})
    }
}
struct Cached { key:(bool,u64,u16), soup:Soup }
#[derive(Default)]
pub(super) struct CrewPersonalSpace {
    sources:Vec<Option<Source>>,
    cache:[Option<Cached>;5],
    radius:f32,
    height:f32,
    pub candidates:usize,
    pub bodies:usize,
    pub depenetrations:usize,
    pub queries:u64,
}
impl CrewPersonalSpace {
    pub(super) fn new()->Self {
        let sources=CREW_SEATED_GLBS.into_iter().chain(CREW_STANDING_GLBS).map(|p| {
            Source::load(p).map_err(|e|eprintln!("[crew contact] source unavailable {p}: {e:#}")).ok()
        }).collect::<Vec<_>>();
        // Calibrate a locomotion capsule from the source torso and pelvis, rather than placing
        // invisible authored blocking boxes around other people. Arms do not widen this capsule.
        let mut radius=0.0f32;let mut height=0.0f32;
        if let Some(source)=sources.get(5).and_then(Option::as_ref) {
            for &p in &source.positions {
                height=height.max(p.y);
                if matches!(crew_physics::classify_relaxed_mesh_point(p),
                    crew_physics::CrewBodyPart::Torso | crew_physics::CrewBodyPart::Pelvis) {
                    radius=radius.max(Vec2::new(p.x,p.z).length());
                }
            }
        }
        Self{sources,radius,height,..Self::default()}
    }
    fn clamp(&mut self, game:&Game, from:Vec3, to:Vec3)->Vec3 {
        self.candidates=0;self.bodies=0;
        if !from.is_finite() || !to.is_finite() {return from;}
        // Missing private source assets are already reported at load. Never invent a blocker
        // or freeze the player when no verified contact surface is available.
        if self.radius<=0.0 || self.height<=0.0 {return to;}
        if (to-from).length_squared()<1e-10 {return to;}
        self.queries+=1;
        let radius=self.radius;
        let height=(self.height-game.loader_crouch_t*0.50).max(radius*2.0);
        let base=Vec3::new(from.x,crate::interior_rig::FLOOR_STAND_Y,from.z);
        let axis=[base+Vec3::Y*radius,base+Vec3::Y*(height-radius)];
        let delta=Vec3::new(to.x-from.x,0.0,to.z-from.z);
        let viewer=game.singleplayer_embodied_role().map(singleplayer_crew::body_post);
        let turret_inverse=game.player.turret_model().inverse();
        let mut triangles=Vec::new();
        for body in game.physical_crew_bodies(BloodFrame::Turret) {
            if Some(body.casualty.identity)==viewer {continue;}
            if game.crew_actor_has_retained_body(body.casualty.peer_id) {continue;}
            let id=crew_identity_index(body.casualty.identity);
            let standing=game.crew_standing_post(body.casualty.role);
            let Some(source)=self.sources.get(id+if standing{5}else{0}).and_then(Option::as_ref) else {continue;};
            let pose=game.crew_arm_animation.contact_pose(id);
            let missing=game.living_wounds.missing(body.casualty.peer_id);
            // Commander standing meshes currently retain their full visible geometry.
            let mask=if body.casualty.role==crate::lan::LanRole::Commander {0} else {missing.iter().enumerate().fold(0u16,|v,(i,b)|v|((*b as u16)<<i))};
            let key=(standing,pose.map_or(0,|(v,_)|v),mask);
            if self.cache[id].as_ref().is_none_or(|c|c.key!=key) {
                let indices=if mask==0 {source.indices.clone()} else {
                    crew_arm_animation::wounded_pose_indices(id,missing,standing).unwrap_or_else(|_|source.indices.clone())
                };
                let positions=pose.map(|(_,v)|v.iter().map(|p|Vec3::from_array(p.pos)).collect::<Vec<_>>());
                let positions=positions.as_deref().unwrap_or(&source.positions);
                let triangles=indices.chunks_exact(3).filter_map(|t| {
                    Some([*positions.get(t[0]as usize)?,*positions.get(t[1]as usize)?,*positions.get(t[2]as usize)?])
                }).collect();
                self.cache[id]=Some(Cached{key,soup:Soup::new(triangles)});
            }
            let role=crew_solo::role_of_post(body.casualty.identity);
            let slump=crew_solo::slump_pose(game.crew_solo.man(role));
            let model=turret_inverse*game.singleplayer_rig_model(body.casualty)*slump;
            let inv=model.inverse();let scale=inv.x_axis.truncate().length().max(inv.y_axis.truncate().length()).max(inv.z_axis.truncate().length());
            let mut bounds=Bounds::empty();
            for p in axis {bounds.add(inv.transform_point3(p));}
            // Sliding around an oblique limb can move outside the original straight sweep's
            // AABB. Every projected remainder is no longer than the requested movement, so
            // this travel-radius bound contains all four slide passes without a second scan.
            let reach=(delta.length()+radius+SKIN)*scale;
            bounds.min-=Vec3::splat(reach);bounds.max+=Vec3::splat(reach);
            if let Some(c)=&self.cache[id] {c.soup.candidates(bounds,&mut triangles,model);}
            self.bodies+=1;
        }
        self.candidates=triangles.len();
        // A walker who starts inside another man (a seat left straight into the gunner's side,
        // 2026-11-01: "stuck after exiting gunner seat in front of the gunner") is not held
        // there: every contact at the start would block every direction. Deep overlap releases
        // the step so he can walk out; a merely touching capsule keeps the sweep's back-out rule.
        if starts_inside(axis,radius,&triangles) {self.depenetrations+=1;return to;}
        let displacement=slide_capsule(axis,radius,delta,&triangles);
        Vec3::new(from.x+displacement.x,to.y,from.z+displacement.z)
    }
}

// Closest points use triangle faces, finite edges and segment endpoints, including a segment
// through the face. No crew-bounding box is used in the narrow phase.
fn closest_triangle(p:Vec3,t:[Vec3;3])->Vec3 {
    let [a,b,c]=t;let ab=b-a;let ac=c-a;let ap=p-a;let d1=ab.dot(ap);let d2=ac.dot(ap);
    if d1<=0.0&&d2<=0.0{return a;}
    let bp=p-b;let d3=ab.dot(bp);let d4=ac.dot(bp);if d3>=0.0&&d4<=d3{return b;}
    let vc=d1*d4-d3*d2;if vc<=0.0&&d1>=0.0&&d3<=0.0{return a+ab*(d1/(d1-d3));}
    let cp=p-c;let d5=ab.dot(cp);let d6=ac.dot(cp);if d6>=0.0&&d5<=d6{return c;}
    let vb=d5*d2-d1*d6;if vb<=0.0&&d2>=0.0&&d6<=0.0{return a+ac*(d2/(d2-d6));}
    let va=d3*d6-d5*d4;if va<=0.0&&d4-d3>=0.0&&d5-d6>=0.0{return b+(c-b)*((d4-d3)/((d4-d3)+(d5-d6)));}
    let inv=1.0/(va+vb+vc);a+ab*(vb*inv)+ac*(vc*inv)
}
fn closest_segments(a:Vec3,b:Vec3,c:Vec3,d:Vec3)->(Vec3,Vec3) {
    let u=b-a;let v=d-c;let w=a-c;let uu=u.dot(u);let vv=v.dot(v);let uv=u.dot(v);let uw=u.dot(w);let vw=v.dot(w);
    let denom=uu*vv-uv*uv;
    let mut s=if denom>1e-12{((uv*vw-uw*vv)/denom).clamp(0.0,1.0)}else{0.0};
    let mut t=if vv>1e-12{(uv*s+vw)/vv}else{0.0};
    if t<0.0 {t=0.0;s=if uu>1e-12{(-uw/uu).clamp(0.0,1.0)}else{0.0};}
    else if t>1.0 {t=1.0;s=if uu>1e-12{((uv-uw)/uu).clamp(0.0,1.0)}else{0.0};}
    (a+u*s,c+v*t)
}
fn capsule_distance(axis:[Vec3;2],t:[Vec3;3])->(f32,Vec3) {
    let mut pair=(axis[0],closest_triangle(axis[0],t));
    let mut best=pair.0.distance_squared(pair.1);
    for candidate in [(axis[1],closest_triangle(axis[1],t)),
        closest_segments(axis[0],axis[1],t[0],t[1]),closest_segments(axis[0],axis[1],t[1],t[2]),
        closest_segments(axis[0],axis[1],t[2],t[0])] {
        let dist=candidate.0.distance_squared(candidate.1);if dist<best {best=dist;pair=candidate;}
    }
    let normal=(t[1]-t[0]).cross(t[2]-t[0]).normalize_or_zero();
    let denominator=normal.dot(axis[1]-axis[0]);
    if denominator.abs()>1e-8 {
        let fraction=normal.dot(t[0]-axis[0])/denominator;
        if (0.0..=1.0).contains(&fraction) {
            let point=axis[0]+(axis[1]-axis[0])*fraction;
            if closest_triangle(point,t).distance_squared(point)<1e-10 {return (0.0,normal);}
        }
    }
    let distance=best.sqrt();(distance,if distance>1e-7{(pair.0-pair.1)/distance}else{normal})
}
fn sweep_capsule(axis:[Vec3;2],radius:f32,delta:Vec3,t:[Vec3;3])->Option<(f32,Vec3)> {
    let speed=delta.length();if speed<1e-8{return None;}
    let mut fraction=0.0;let mut last_normal=Vec3::ZERO;
    for _ in 0..MAX_ADVANCE {
        let (distance,normal)=capsule_distance(axis.map(|p|p+delta*fraction),t);last_normal=normal;
        if distance<=radius+SKIN {
            // An already touching character may back out or move along the surface.
            if fraction<=1e-7 && delta.dot(normal)>=-1e-7 {return None;}
            return Some((fraction,normal));
        }
        fraction+=((distance-radius-SKIN)/speed).max(1e-6);
        if fraction>1.0{return None;}
    }
    // Bounded conservative advancement fails closed rather than tunnelling through a limb.
    Some((fraction,last_normal))
}
/// The capsule already overlaps a body surface by more than a touch: inside, not against.
fn starts_inside(axis:[Vec3;2],radius:f32,triangles:&[[Vec3;3]])->bool {
    triangles.iter().any(|t|capsule_distance(axis,*t).0<radius*DEPENETRATION_FRACTION)
}
fn slide_capsule(axis:[Vec3;2],radius:f32,delta:Vec3,triangles:&[[Vec3;3]])->Vec3 {
    let mut moved=Vec3::ZERO;let mut remaining=delta;
    for _ in 0..MAX_SLIDES {
        if remaining.length_squared()<1e-10 {break;}
        let contact=triangles.iter().filter_map(|&t|sweep_capsule(axis.map(|p|p+moved),radius,remaining,t))
            .min_by(|a,b|a.0.total_cmp(&b.0));
        let Some((fraction,normal))=contact else {moved+=remaining;break;};
        moved+=remaining*fraction;
        remaining*=1.0-fraction;
        let horizontal=Vec3::new(normal.x,0.0,normal.z).normalize_or_zero();
        if horizontal.length_squared()<0.5 {break;}
        let inward=remaining.dot(horizontal);
        if inward<0.0 {remaining-=horizontal*inward;} else {break;}
    }
    moved
}

impl Game {
    pub(super) fn clamp_walk_against_crew(&mut self,from:Vec3,to:Vec3)->Vec3 {
        if !self.singleplayer_crew_active || self.mode!=Mode::Tank || self.station!=Station::Loader {return to;}
        let mut contacts=std::mem::take(&mut self.crew_personal_space);
        let result=contacts.clamp(self,from,to);self.crew_personal_space=contacts;result
    }
    pub fn crew_personal_space_receipt(&self)->serde_json::Value {
        serde_json::json!({"queries":self.crew_personal_space.queries,"bodies":self.crew_personal_space.bodies,"depenetrations":self.crew_personal_space.depenetrations,
            "triangle_candidates":self.crew_personal_space.candidates,"capsule_radius_m":self.crew_personal_space.radius,
            "capsule_height_m":self.crew_personal_space.height,"contact_skin_m":SKIN,
            "max_sweep_iterations":MAX_ADVANCE,"max_slide_iterations":MAX_SLIDES,"living_only":true})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wall()->[[Vec3;3];2] {[[Vec3::new(0.0,-2.0,-3.0),Vec3::new(0.0,3.0,-3.0),Vec3::new(0.0,3.0,3.0)],
        [Vec3::new(0.0,-2.0,-3.0),Vec3::new(0.0,3.0,3.0),Vec3::new(0.0,-2.0,3.0)]]}
    fn axis(x:f32,z:f32)->[Vec3;2] {[Vec3::new(x,0.2,z),Vec3::new(x,1.3,z)]}
    #[test]fn capsule_cannot_tunnel_through_a_thin_body_surface(){
        let movement=slide_capsule(axis(-1.0,0.0),0.2,Vec3::X*2.0,&wall());
        assert!(movement.x>0.79&&movement.x<=0.801,"{movement:?}");
    }
    #[test]fn a_diagonal_step_slides_along_the_contact(){
        let movement=slide_capsule(axis(-0.4,0.0),0.2,Vec3::new(0.5,0.0,0.5),&wall());
        assert!(movement.x<0.201&&movement.z>0.49,"{movement:?}");
    }
    #[test]fn a_touching_player_can_back_out_instead_of_deadlocking(){
        let movement=slide_capsule(axis(-0.19,0.0),0.2,Vec3::NEG_X*0.3,&wall());
        assert!((movement.x+0.3).abs()<1e-6);
    }
    #[test]fn partitioned_movement_reaches_the_same_contact(){
        let start=axis(-1.0,0.0);let delta=Vec3::new(1.4,0.0,0.6);
        let full=slide_capsule(start,0.2,delta,&wall());let mut fine=Vec3::ZERO;
        for _ in 0..60 {fine+=slide_capsule(start.map(|p|p+fine),0.2,delta/60.0,&wall());}
        assert!(full.distance(fine)<0.003,"{full:?} vs {fine:?}");
    }
    #[test]fn broadphase_preserves_only_nearby_source_triangles(){
        let mut triangles=wall().to_vec();
        for i in 0..32 {triangles.extend(wall().map(|t|t.map(|p|p+Vec3::X*(100.0+i as f32))));}
        let total=triangles.len();
        let soup=Soup::new(triangles);let mut out=Vec::new();
        soup.candidates(Bounds{min:Vec3::splat(-2.0),max:Vec3::splat(2.0)},&mut out,Mat4::IDENTITY);
        // Small leaves can contain remote triangles; they remain harmless exact geometry.
        assert!(out.iter().any(|t|t[0].x==0.0));
        assert!(out.len()<=8 && out.len()<total/4,"{} of {total} triangles",out.len());
        let result=slide_capsule(axis(-1.0,0.0),0.2,Vec3::X*2.0,&out);assert!(result.x<0.801);
    }
    #[test]fn a_walker_placed_inside_a_body_is_released_but_a_touching_one_is_not(){
        // The wall stands at x = 0. A capsule whose axis sits on it is inside the body; one whose
        // surface just touches it (axis at radius + skin) is only against it.
        let inside=axis(0.0,0.0);let touching=axis(0.2+SKIN*2.0,0.0);
        assert!(starts_inside(inside,0.2,&wall()));
        assert!(!starts_inside(touching,0.2,&wall()));
        // Against the wall the sweep still refuses to go through it.
        let through=slide_capsule(touching,0.2,Vec3::new(-0.3,0.0,0.0),&wall());
        assert!(through.x> -0.01,"{through}");
    }
    #[test]fn no_contacts_preserve_the_requested_movement(){
        assert_eq!(slide_capsule(axis(0.0,0.0),0.2,Vec3::new(0.2,0.0,-0.3),&[]),Vec3::new(0.2,0.0,-0.3));
    }
}

impl CrewPersonalSpace {pub(super) fn invalidate_world_cache(&mut self) {self.cache=std::array::from_fn(|_|None);}}
