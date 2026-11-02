//! Finite loose depot rounds on a hand-pushed trolley. Cartridges transfer into the existing
//! exterior round/hands/rack machinery; securing an objective never adds an ammunition count.
//! The trolley is a project-authored utility prop, not a claim of a specific historical model.
use super::*;
use serde::{Serialize,Deserialize};
const ROUNDS:usize=12;

#[derive(Clone,Debug,Serialize,Deserialize)]
pub(in crate::game) struct Cart {
    pub id:u64,pub site:u64,pub position:Vec3,pub heading:f32,pub spin:f32,pub he:bool,
    pub remaining:[bool;ROUNDS],pub body:usize,pub wheels:[usize;4],pub rounds:[usize;ROUNDS],
    pub pushed_by:Option<crate::expedition::PersonId>,
}
impl Cart {
    fn root(&self)->Mat4{Mat4::from_rotation_translation(Quat::from_rotation_y(self.heading),self.position)}
    fn round_model(&self,n:usize)->Mat4{self.root()*Mat4::from_translation(Vec3::new(-0.46+(n%6)as f32*0.18,0.78+(n/6)as f32*0.11,0.))}
    fn mass(&self)->f32{42.+self.remaining.iter().filter(|b|**b).count()as f32*15.6}
    fn source_id(&self,n:usize)->u32{0x40_000+self.id as u32*ROUNDS as u32+n as u32}
}
fn body_mesh()->CpuMesh {
    let mut parts=vec![crate::prims::box_mesh(Vec3::new(-0.62,0.51,-0.61),Vec3::new(0.62,0.59,0.61),[78,76,64])];
    for x in [-0.60,0.60] {parts.push(crate::prims::box_mesh(Vec3::new(x-0.025,0.59,-0.61),Vec3::new(x+0.025,0.84,0.61),[78,76,64]));
        parts.push(crate::prims::box_mesh(Vec3::new(x-0.025,0.59,-1.25),Vec3::new(x+0.025,0.66,-0.5),[78,76,64]));}
    parts.push(crate::prims::box_mesh(Vec3::new(-0.62,0.59,-1.29),Vec3::new(0.62,0.66,-1.24),[78,76,64]));
    let mut out=parts.remove(0);for p in parts {let base=out.vertices.len()as u32;out.vertices.extend(p.vertices);out.indices.extend(p.indices.into_iter().map(|i|i+base));out.min=out.min.min(p.min);out.max=out.max.max(p.max);}out
}
fn wheel_mesh()->CpuMesh {
    let mut v=Vec::new();let mut idx=Vec::new();
    for side in [-0.06,0.06] {for n in 0..16 {let a=n as f32*std::f32::consts::TAU/16.;v.push(crate::gltf_load::Vertex{pos:[side,a.cos()*0.27,a.sin()*0.27],normal:[0.,a.cos(),a.sin()],uv:[0.,0.],tangent:[1.,0.,0.,1.]});}}
    for n in 0..16 {let a=n as u32;let b=(n+1)%16;idx.extend([a,b as u32,b as u32+16,a,b as u32+16,a+16]);}
    for n in 1..15 {idx.extend([0,n+1,n,16,16+n,16+n+1]);}
    let mut m=CpuMesh::basic(v,idx,Some(crate::prims::solid_color([45,45,41])),Vec3::new(-0.06,-0.27,-0.27),Vec3::new(0.06,0.27,0.27));m.metallic_factor=0.4;m
}
const WHEELS:[Vec3;4]=[Vec3::new(-0.68,0.27,-0.42),Vec3::new(0.68,0.27,-0.42),Vec3::new(-0.68,0.27,0.42),Vec3::new(0.68,0.27,0.42)];
pub(super) fn install(scene:&mut Vec<CpuMesh>,spec:&crate::expedition_theatre::Spec,hf:&terrain::Heightfield)->Vec<Cart>{
    let mut carts=Vec::new();
    for site in &spec.sites {for (n,he) in [false,true].into_iter().filter(|he|site.rewards.iter().any(|s|s==if *he{"ammunition.88_he"}else{"ammunition.88_ap"})).enumerate(){
        let x=site.position[0]-if site.kind==Kind::Bridge{135.}else{8.}-n as f32*2.;
        let z=site.position[1]+if site.kind==Kind::Bridge{30.}else{8.};
        let body=scene.len();scene.push(body_mesh());let wheels=std::array::from_fn(|_|{let i=scene.len();scene.push(wheel_mesh());i});
        let rounds=std::array::from_fn(|_|{let i=scene.len();scene.push(round_88_mesh(he));i});
        carts.push(Cart{id:carts.len()as u64+1,site:site.id,position:Vec3::new(x,hf.height_at(x,z),z),heading:0.,spin:0.,he,remaining:[true;ROUNDS],body,wheels,rounds,pushed_by:None});
    }}carts
}
fn cart_support_height(crossings:&[Crossing],p:Vec3,terrain:f32)->f32 {
    crossings.iter().find_map(|b|b.deck_at(Vec2::new(p.x,p.z))).unwrap_or(terrain)
}
fn coupled_candidate(position:Vec3,heading:f32,previous:Vec3,requested:Vec3,look_yaw:f32,
    mass:f32,dt:f32)->(Vec3,f32,Vec3) {
    let limit=dt.max(0.)*(1.1-(mass-42.)/450.).max(0.35);
    let delta=(requested-previous).with_y(0.).clamp_length_max(limit);
    let target_heading=std::f32::consts::PI-look_yaw;
    let turn=((target_heading-heading+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI)
        .clamp(-1.2*dt.max(0.),1.2*dt.max(0.));
    let root=Mat4::from_rotation_translation(Quat::from_rotation_y(heading),position);
    let offset=root.inverse().transform_point3(previous);
    let at=|fraction:f32|{let yaw=heading+turn*fraction;let pos=position+delta*fraction;
        let foot=Mat4::from_rotation_translation(Quat::from_rotation_y(yaw),pos).transform_point3(offset);(pos,yaw,foot)};
    let candidate=at(1.);let distance=(candidate.2-previous).with_y(0.).length();
    at(if distance>limit {limit/distance}else{1.})
}
fn coupled_admitted(cart_clear:bool,wanted:Vec3,admitted:Vec3)->bool {
    cart_clear&&wanted.distance(admitted)<=0.002
}
fn cart_source_triangles(cart:&Cart)->Vec<[Vec3;3]> {
    let mut out=Vec::new();let mut append=|mesh:CpuMesh,model:Mat4|{
        out.extend(mesh.indices.chunks_exact(3).map(|t|[0,1,2].map(|i|model.transform_point3(Vec3::from_array(mesh.vertices[t[i]as usize].pos)))));
    };
    append(body_mesh(),Mat4::IDENTITY);
    for p in WHEELS {append(wheel_mesh(),Mat4::from_translation(p)*Mat4::from_rotation_x(cart.spin));}
    for n in 0..ROUNDS {if cart.remaining[n] {append(round_88_mesh(cart.he),Mat4::from_translation(Vec3::new(-0.46+(n%6)as f32*0.18,0.78+(n/6)as f32*0.11,0.)));}}
    out
}
fn source_motion_clear(before:&[Vec3],after:&[Vec3],floor_before:f32,floor_after:f32,
    query:&impl Fn(Vec3,Vec3)->Option<(Vec3,Vec3)>)->bool {
    let clear=|a:Vec3,b:Vec3|a.distance_squared(b)<1e-12||query(a,b).is_none_or(|(point,normal)|
        normal.y.abs()>0.85&&point.y<=floor_before+0.025&&floor_after>=point.y-0.002);
    before.iter().zip(after).all(|(&a,&b)|clear(a,b))&&after.chunks_exact(3).zip(before.chunks_exact(3)).all(|(a,b)|
        (0..3).all(|i|clear(a[i],a[(i+1)%3])||!clear(b[i],b[(i+1)%3])))
}
#[derive(Clone,Copy)]enum Pick{Round(usize,usize),Handle(usize)}
impl Game {
    fn depot_pick(&self,eye:Vec3,dir:Vec3)->Option<Pick>{
        if self.mode!=Mode::Foot||!self.controlled_soldier_alive()||self.halftrack_occupied()||self.carry_mesh.is_some()||self.held_item.is_some()||self.personal_hand_item_active(){return None;}
        let t=self.theatre.as_ref()?;let mut best:Option<(f32,Pick)>=None;
        for (i,c) in t.ammo_carts.iter().enumerate(){
            if c.position.distance(eye)>4. {continue;}
            for n in 0..ROUNDS {if c.remaining[n] {if let Some(d)=ray_loader_round(eye,dir,c.round_model(n),2.2){if best.is_none_or(|b|d<b.0)&&self.equipment_path_clear(eye,eye+dir*d,0.02){best=Some((d,Pick::Round(i,n)));}}}}
            let inv=c.root().inverse();if let Some(d)=ray_loader_ammunition_mesh(eye,dir,c.root(),2.2,&body_mesh()){
                let p=inv.transform_point3(eye+dir*d);if p.z< -1.2&&best.is_none_or(|b|d<b.0)&&self.equipment_path_clear(eye,eye+dir*d,0.02){best=Some((d,Pick::Handle(i)));}
            }
        }best.map(|(_,p)|p)
    }
    pub fn depot_hover(&self,eye:Vec3,dir:Vec3)->Option<String>{match self.depot_pick(eye,dir)?{
        Pick::Round(i,_)=>Some(format!("88 MM {} / HOLD LMB TO CARRY",if self.theatre.as_ref()?.ammo_carts[i].he{"HE"}else{"AP"})),
        Pick::Handle(i)=>Some(format!("PUSH AMMUNITION CART / {:.0} KG / LMB, X RELEASE",self.theatre.as_ref()?.ammo_carts[i].mass())),
    }}
    pub fn depot_interact(&mut self,eye:Vec3,dir:Vec3)->bool{
        let Some(pick)=self.depot_pick(eye,dir)else{return false};
        match pick {
            Pick::Handle(i)=>{let person=self.expedition_state().and_then(|s|s.controlled_person);
                for cart in &mut self.theatre.as_mut().unwrap().ammo_carts {cart.pushed_by=None;}
                self.theatre.as_mut().unwrap().ammo_carts[i].pushed_by=person;},
            Pick::Round(i,n)=>{
                let Some(slot)=self.loose.iter().enumerate().find(|(i,s)|!s.active&&self.carry_mesh!=Some(CarriedMesh::LooseShell(*i))).map(|(i,_)|i)else{return true};
                let cart=&self.theatre.as_ref().unwrap().ammo_carts[i];let pose=cart.round_model(n);let he=cart.he;let id=cart.source_id(n);
                let (_,rotation,position)=pose.to_scale_rotation_translation();
                self.loose[slot]=LooseShell{active:false,world_space:true,pos:position,rotation,nature_he:he,..Default::default()};
                self.transfer_cartridge_state(id,0x2_0000+slot as u32);
                self.carry_mesh=Some(CarriedMesh::LooseShell(slot));self.set_carry_pose(position,rotation,Vec3::ZERO,Vec3::ZERO);self.grab=true;
                self.theatre.as_mut().unwrap().ammo_carts[i].remaining[n]=false;
            }
        }self.consume_primary_press();true
    }
    pub fn release_depot_cart(&mut self)->bool {let Some(t)=self.theatre.as_mut()else{return false};let mut released=false;for c in &mut t.ammo_carts{released|=c.pushed_by.take().is_some();}released}
    /// Coupled admission runs after ordinary foot contact and before camera/hand presentation.
    /// It retains the actual previous relative position, not an assumed handle-side teleport.
    pub(in crate::game) fn advance_depot_cart(&mut self,previous:Vec3,was_airborne:bool,dt:f32,r:&Renderer) {
        if !dt.is_finite()||dt<=0. {return;}
        let Some(mut theatre)=self.theatre.take()else{return};
        let person=self.expedition_state().and_then(|s|s.controlled_person);
        let active=theatre.ammo_carts.iter().position(|c|c.pushed_by.is_some());
        if let Some(i)=active {
            for (n,cart) in theatre.ammo_carts.iter_mut().enumerate(){if n!=i{cart.pushed_by=None;}}
            let cart=&mut theatre.ammo_carts[i];
            if cart.pushed_by!=person||!self.controlled_soldier_alive()||self.mode!=Mode::Foot
                ||was_airborne||self.foot.vel_y>0.||previous.distance(cart.position)>3.5 {
                cart.pushed_by=None;
            }else{
                let support=|p:Vec3|cart_support_height(&theatre.crossings,p,self.hf.height_at(p.x,p.z));
                let (next_position,next_heading,mut operator)=coupled_candidate(cart.position,cart.heading,
                    previous,self.foot.pos,self.foot.yaw,cart.mass(),dt);
                let next_position=next_position.with_y(support(next_position));operator.y=support(operator);
                let before=cart.root();let after=Mat4::from_rotation_translation(Quat::from_rotation_y(next_heading),next_position);
                let moved=next_position-cart.position;let forward=Vec3::new(next_heading.sin(),0.,next_heading.cos());
                let next_spin=cart.spin+moved.dot(forward)/0.27;
                let mut next_cart=cart.clone();next_cart.spin=next_spin;
                let local_source=cart_source_triangles(cart);let next_source=cart_source_triangles(&next_cart);
                let before_points=local_source.iter().flatten().map(|p|before.transform_point3(*p)).collect::<Vec<_>>();
                let after_points=next_source.iter().flatten().map(|p|after.transform_point3(*p)).collect::<Vec<_>>();
                let lo=before_points.iter().chain(&after_points).fold(previous.min(operator),|a,p|a.min(*p))-Vec3::splat(0.5);
                let hi=before_points.iter().chain(&after_points).fold(previous.max(operator)+Vec3::Y*self.foot.body_height(),|a,p|a.max(*p))+Vec3::splat(0.5);
                let mut triangles=Vec::new();let mut excluded=vec![cart.body];excluded.extend(cart.wheels);excluded.extend(cart.rounds);
                let complete=self.halftrack_runtime.as_ref().is_some_and(|v|v.world.nearby_excluding(r,lo,hi,&mut triangles,&excluded));
                let models=self.fire_surface_models();triangles.extend(self.cabin_fire.source_contacts(&models).world_triangles(lo,hi));
                let world=singleplayer_crew::field_people::SourceIndex::new(triangles);
                let carrier=self.recovery_body_contact_triangles();
                let query=|a:Vec3,b:Vec3|[world.contact(a,b),carrier.contact(a,b)].into_iter().flatten()
                    .min_by(|x,y|a.distance_squared(x.0).total_cmp(&a.distance_squared(y.0)));
                let min_before=before_points.iter().map(|p|p.y).fold(f32::INFINITY,f32::min);
                let min_after=after_points.iter().map(|p|p.y).fold(f32::INFINITY,f32::min);
                let clear=complete&&(next_position.y-cart.position.y).abs()
                    <=Vec2::new(next_position.x-cart.position.x,next_position.z-cart.position.z).length()*0.35+0.015
                    &&source_motion_clear(&before_points,&after_points,min_before,min_after,&query);
                let mut foot=crate::foot::FootSoldier{pos:operator,crouch_t:self.foot.crouch_t,
                    vel_y:self.foot.vel_y,airborne:self.foot.airborne,..Default::default()};
                foot.resolve_solid_motion(previous,was_airborne,&query);
                if coupled_admitted(clear,operator,foot.pos) {
                    cart.spin=next_spin;cart.position=next_position;cart.heading=next_heading;
                    self.foot.pos=foot.pos;self.foot.vel_y=foot.vel_y;self.foot.airborne=foot.airborne;
                }else{self.foot.pos=previous;}
            }
        }
        self.theatre=Some(theatre);
    }
    /// Presentation and abandoned-handle cleanup only; movement has already passed admission.
    pub(in crate::game) fn update_depot_carts(&mut self,_dt:f32,r:&mut Renderer){
        let person=self.expedition_state().and_then(|s|s.controlled_person);
        let capable=self.controlled_soldier_alive()&&self.mode==Mode::Foot&&!self.halftrack_occupied();
        let Some(t)=self.theatre.as_mut()else{return};
        for c in &mut t.ammo_carts {
            if c.pushed_by.is_some()&&(!capable||c.pushed_by!=person){c.pushed_by=None;}
            r.set_visible(c.body,true);r.set_model(c.body,c.root());
            for (i,&slot) in c.wheels.iter().enumerate(){r.set_visible(slot,true);r.set_model(slot,c.root()*Mat4::from_translation(WHEELS[i])*Mat4::from_rotation_x(c.spin));}
            for n in 0..ROUNDS{r.set_visible(c.rounds[n],c.remaining[n]);r.set_model(c.rounds[n],c.round_model(n));}
        }
    }
    pub(in crate::game) fn resolve_depot_foot(&mut self,previous:Vec3,airborne:bool){
        let Some(t)=&self.theatre else{return};let mesh=body_mesh();let sources:Vec<_>=t.ammo_carts.iter().filter(|c|c.pushed_by.is_none()&&c.position.distance(previous)<3.).map(|c|c.root()).collect();
        if sources.is_empty(){return;}
        let contacts=surface_fire::CabinFire::new(&[(0,&mesh)]);
        self.foot.resolve_solid_motion(previous,airborne,|from,to|{
            sources.iter().filter_map(|m|contacts.sweep(from,to,&[*m;surface_fire::FRAMES])
                .map(|h|(m.transform_point3(h.point),m.transform_vector3(h.normal).normalize_or_zero())))
                .min_by(|a,b|from.distance_squared(a.0).total_cmp(&from.distance_squared(b.0)))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trolley_support_uses_the_visible_bridge_not_the_river_bed() {
        let crossing=Crossing{center:Vec2::new(80.,20.),deck_y:12.,half_length:50.,half_width:5.};
        assert_eq!(cart_support_height(&[crossing.clone()],Vec3::new(80.,12.,20.),4.),12.);
        assert_eq!(cart_support_height(&[crossing],Vec3::new(80.,4.,26.),4.),4.);
    }
    #[test]
    fn trolley_turn_retains_the_real_operator_offset_and_bounded_travel() {
        let cart=Vec3::new(11.,3.,20.);let heading=0.2;
        let root=Mat4::from_rotation_translation(Quat::from_rotation_y(heading),cart);
        let local=Vec3::new(0.25,0.,-1.85);let before=root.transform_point3(local);
        let (next,yaw,operator)=coupled_candidate(cart,heading,before,before+Vec3::X,0.1,210.,1./60.);
        let after=Mat4::from_rotation_translation(Quat::from_rotation_y(yaw),next);
        assert!(after.inverse().transform_point3(operator).distance(local)<0.00001,
            "turning must not snap an offset operator onto an invented 1.7 m anchor");
        assert!(operator.distance(before)<0.02);assert!(next.distance(cart)<0.02);
        let (next,yaw,operator)=coupled_candidate(cart,heading,before,before,0.1,210.,0.);
        assert_eq!(next,cart);assert_eq!(yaw,heading);assert!(operator.distance(before)<0.00001);
    }
    #[test]
    fn clear_cart_cannot_pull_its_operator_through_an_actual_wall_triangle() {
        let before=Vec3::new(0.,0.,-1.8);let wanted=before+Vec3::X*0.04;
        let wall=[Vec3::new(0.25,0.,-3.),Vec3::new(0.25,3.,-3.),Vec3::new(0.25,1.5,0.)];
        let mut person=crate::foot::FootSoldier::default();person.pos=wanted;
        person.resolve_solid_motion(before,false,|a,b|singleplayer_crew::field_people::source_contact(a,b,&[wall]));
        assert!(person.pos.distance(wanted)>0.002,"fixture must block the operator's shoulder");
        assert!(!coupled_admitted(true,wanted,person.pos),"a clear cart does not authorize a blocked person");
        assert!(coupled_admitted(true,wanted,wanted));assert!(!coupled_admitted(false,wanted,wanted));
    }
    #[test]
    fn trolley_source_sweep_blocks_world_wall_but_not_empty_triangle_bounds() {
        let wall=[Vec3::new(0.,0.,0.),Vec3::new(0.,2.,0.),Vec3::new(0.,0.,2.)];
        let query=|a,b|singleplayer_crew::field_people::source_contact(a,b,&[wall]);
        assert!(!source_motion_clear(&[Vec3::new(-0.1,0.5,0.5)],&[Vec3::new(0.1,0.5,0.5)],0.,0.,&query));
        assert!(source_motion_clear(&[Vec3::new(-0.1,1.8,1.8)],&[Vec3::new(0.1,1.8,1.8)],0.,0.,&query));
    }
}
