//! Authored crew-seat instrument, not historical Tiger equipment. The five source-mounted
//! lenses show the supplied CURRENT occupants; this module owns no roster or assignment state.
//! Black role symbols are painted directly on the source wall. Three finite luminous wire
//! segments replace, rather than overlay, the selected part of a permanent black orthogonal bus.
//! Lens and wire radiance use the ordinary cabin transport, including their source geometry.
use crate::gltf_load::CpuMesh;
use crate::indicator_pictograms::{self, Glyph};
use crate::interior_rig::{FiniteCubeLamp, LampEmission};
use crate::renderer::{PointLight, Renderer};
use glam::{Mat4, Vec3};
use serde_json::{json, Value};

pub(super) const SEAT_NAMES: [&str; 5] = ["Commander", "Gunner", "Loader", "Radioman", "Driver"];
pub(super) const SHADOW_REGISTRATIONS: usize = 8;
// Surveyed inner wall Turret_0099, source tiger_turret.glb, engine +X is vehicle left.
const WALL: Vec3 = Vec3::new(1.15327, 2.015, 0.26206);
const NORMAL: Vec3 = Vec3::new(-0.9753286, 0.0, -0.220758);
const HALF: f32 = 0.007;
const FOOT: f32 = 0.009;
const PLINTH: f32 = 0.004;
const REAR_HALF_WIDTH: f32 = 0.044;
const ROW_Y: f32 = 0.032;
const WIRE_WIDTH: f32 = 0.0015;
const GLYPH_SIZE: f32 = 0.024;
const PAINT_LIFT: f32 = 0.0002;
const COLORS: [[u8; 3]; 4] = [[65, 235, 85], [255, 160, 32], [235, 42, 28], [35, 37, 34]];
const WIRE_COLOR: [u8; 3] = [255, 170, 45];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Condition { Healthy, Wounded, Dead, Empty }
impl Condition {
    fn name(self) -> &'static str {
        match self { Self::Healthy => "healthy", Self::Wounded => "wounded", Self::Dead => "dead", Self::Empty => "empty" }
    }
    fn color_index(self) -> usize {
        match self { Self::Healthy => 0, Self::Wounded => 1, Self::Dead => 2, Self::Empty => 3 }
    }
    pub(super) fn from_casualty(c: &crate::first_aid::Casualty) -> Self {
        if c.dead { Self::Dead } else if c.is_wounded() || !c.is_conscious() { Self::Wounded } else { Self::Healthy }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SeatStatus {
    pub actor_id: Option<u32>,
    pub person_id: Option<u64>,
    pub name: String,
    pub trained_role: String,
    pub condition: Condition,
    pub moving: bool,
}
impl Default for SeatStatus {
    fn default() -> Self {
        Self { actor_id: None, person_id: None, name: String::new(), trained_role: String::new(), condition: Condition::Empty, moving: false }
    }
}

pub(super) struct Panel {
    static_parts: Vec<(usize, Mat4)>,
    lenses: [[usize; 4]; 5],
    branches: [usize; 5],
    spine: [usize; 2],
    glow: [usize; 3],
    paint_max_projection_m: f32,
    mounts: [Mat4; 5],
}

fn frame() -> Mat4 {
    let n = NORMAL.normalize();
    Mat4::from_cols(Vec3::Y.cross(n).extend(0.0), Vec3::Y.extend(0.0), n.extend(0.0), WALL.extend(1.0))
}
/// Top-down seating schematic, viewed with the tank's front at the top. Keep the
/// canonical state indices unchanged: Commander, Gunner, Loader, Radioman, Driver.
fn seat_point(i: usize) -> Vec3 {
    [Vec3::new(0.0,-ROW_Y,0.0), Vec3::new(-REAR_HALF_WIDTH,-ROW_Y,0.0),
     Vec3::new(REAR_HALF_WIDTH,-ROW_Y,0.0), Vec3::new(REAR_HALF_WIDTH*0.5,ROW_Y,0.0),
     Vec3::new(-REAR_HALF_WIDTH*0.5,ROW_Y,0.0)][i]
}
fn glyph_point(i:usize)->Vec3 {let p=seat_point(i);p+Vec3::Y*(0.025*p.y.signum())}
fn lens_model(mount: Mat4) -> Mat4 { mount * Mat4::from_translation(Vec3::Z*(PLINTH+HALF)) }
fn mount_frames(patch: &CpuMesh) -> [Mat4;5] {
    std::array::from_fn(|i| {
        let intended=frame().transform_point3(seat_point(i));
        let (surface,normal,_)=crate::lamp_mounts::closest_surface(patch,intended+NORMAL.normalize()*0.01);
        let right=Vec3::Y.cross(normal).normalize();let up=normal.cross(right);
        Mat4::from_cols(right.extend(0.0),up.extend(0.0),normal.extend(0.0),surface.extend(1.0))
    })
}
fn finite_power(power: f32) -> f32 { if power.is_finite() { power.max(0.0) } else { 0.0 } }
fn pulse(c: Condition, time_s: f32, power: f32) -> bool {
    finite_power(power) > 0.0 && match c {
        Condition::Empty => false,
        Condition::Wounded => time_s.is_finite() && time_s.rem_euclid(1.2) < 0.6,
        Condition::Healthy | Condition::Dead => true,
    }
}
fn route(selected: Option<usize>, hovered: Option<usize>) -> Option<(usize, usize)> {
    selected.zip(hovered).filter(|&(a,b)| a < 5 && b < 5 && a != b)
}
fn push(scene: &mut Vec<CpuMesh>, mesh: CpuMesh) -> usize { let i = scene.len(); scene.push(mesh); i }
fn unit_box(color: [u8; 3]) -> CpuMesh { crate::prims::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), color) }

/// Bound the source search to this physical patch before projecting paint, so importing a
/// hundred-thousand-triangle tank does not multiply the build cost by every painted vertex.
fn wall_patch(source: &CpuMesh) -> CpuMesh {
    let inverse = frame().inverse();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for tri in source.indices.chunks_exact(3) {
        let v = [source.vertices[tri[0] as usize], source.vertices[tri[1] as usize], source.vertices[tri[2] as usize]];
        let q = v.map(|v| inverse.transform_point3(Vec3::from(v.pos)));
        let lo = q[0].min(q[1]).min(q[2]); let hi = q[0].max(q[1]).max(q[2]);
        if lo.x > 0.08 || hi.x < -0.065 || lo.y > 0.11 || hi.y < -0.11 || lo.z > 0.025 || hi.z < -0.025 { continue; }
        if (q[1] - q[0]).cross(q[2] - q[0]).length_squared() < 1e-18 { continue; }
        let base = vertices.len() as u32;
        vertices.extend(v); indices.extend([base, base+1, base+2]);
    }
    CpuMesh::basic(vertices, indices, None, WALL-Vec3::splat(0.25), WALL+Vec3::splat(0.25))
}

impl Panel {
    pub(super) fn append(scene: &mut Vec<CpuMesh>, turret_source: &CpuMesh) -> Self {
        let patch = wall_patch(turret_source);
        assert!(!patch.indices.is_empty(), "crew indicator needs its actual Tiger wall source");
        let mounts=mount_frames(&patch);
        let mut static_parts = Vec::new();
        let mut paint_max_projection_m = 0.0_f32;
        let glyphs = [Glyph::Commander, Glyph::Gunner, Glyph::Loader, Glyph::Radio, Glyph::Driver];
        let lenses = std::array::from_fn(|i| {
            let foot = crate::prims::box_mesh(Vec3::new(-FOOT,-FOOT,0.0), Vec3::new(FOOT,FOOT,PLINTH), [34,35,32]);
            static_parts.push((push(scene,foot), mounts[i]));
            let mut paint = indicator_pictograms::glyph_mesh(glyphs[i], GLYPH_SIZE, [12,13,11])
                .transformed(frame()*Mat4::from_translation(glyph_point(i)));
            let tangent = Vec3::Y.cross(NORMAL.normalize());
            paint.min = Vec3::splat(f32::INFINITY); paint.max = Vec3::splat(f32::NEG_INFINITY);
            for vertex in &mut paint.vertices {
                let p = Vec3::from(vertex.pos);
                let (q,n,_) = crate::lamp_mounts::closest_surface(&patch,p+NORMAL.normalize()*0.01);
                paint_max_projection_m = paint_max_projection_m.max(p.distance(q));
                let p = q + n*PAINT_LIFT;
                vertex.pos = p.to_array(); vertex.normal = n.to_array();
                vertex.tangent = [tangent.x,tangent.y,tangent.z,1.0];
                paint.min = paint.min.min(p); paint.max = paint.max.max(p);
            }
            static_parts.push((push(scene,paint),Mat4::IDENTITY));
            std::array::from_fn(|j| push(scene,crate::prims::box_mesh(Vec3::splat(-HALF),Vec3::splat(HALF),COLORS[j])))
        });
        let branches = std::array::from_fn(|_|push(scene,unit_box([12,13,11])));
        let spine = std::array::from_fn(|_|push(scene,unit_box([12,13,11])));
        let glow = std::array::from_fn(|_|push(scene,unit_box(WIRE_COLOR)));
        Self { static_parts,lenses,branches,spine,glow,paint_max_projection_m,mounts }
    }

    pub(super) fn configure(&self,r:&mut Renderer) {
        for &(slot,model) in &self.static_parts { r.set_model(slot,model); r.set_emissive(slot,0.0); }
        for slot in self.lenses.iter().flatten().chain(self.glow.iter()) {
            r.mark_own_emissive(*slot); r.mark_indicator_lamp(*slot); r.set_visible(*slot,false); r.set_emissive(*slot,0.0);
        }
        for &slot in self.branches.iter().chain(&self.spine) { r.set_visible(slot,false); r.set_emissive(slot,0.0); }
    }

    /// Hide the physical pieces if their parent turret is no longer rendered. Ordinary world
    /// visibility otherwise belongs to scene geometry, not the current camera or HUD mode.
    pub(super) fn hide(&self,r:&mut Renderer) {
        for &(slot,_) in &self.static_parts {r.set_visible(slot,false);}
        for &slot in self.lenses.iter().flatten().chain(&self.branches).chain(&self.spine).chain(&self.glow) {r.set_visible(slot,false);}
    }

    /// Always returns eight ordered shadow registrations, including unpowered sources. Caller
    /// appends positive-output entries to ordinary lights and ALL entries to cabin shadows.
    pub(super) fn drive(&self,r:&mut Renderer,tm:Mat4,states:&[SeatStatus;5],time_s:f32,power:f32,
        selected:Option<usize>,hovered:Option<usize>) -> [(PointLight,Option<usize>);SHADOW_REGISTRATIONS] {
        for &(slot,model) in &self.static_parts { r.set_model(slot,tm*model); r.set_visible(slot,true); }
        let zero = PointLight { pos:tm.transform_point3(WALL),source_radius:0.001,color:Vec3::ZERO,radiant_intensity:0.0 };
        let mut out = [(zero,None);SHADOW_REGISTRATIONS];
        for i in 0..5 {
            let c = states[i].condition;
            let chosen = c.color_index();
            for (j,&slot) in self.lenses[i].iter().enumerate() { r.set_visible(slot,j==chosen); }
            let emission = FiniteCubeLamp { half_size_m:HALF,srgb:COLORS[chosen],emission:1.2 }
                .sample(if pulse(c,time_s,power) { finite_power(power) } else { 0.0 });
            let slot = self.lenses[i][chosen];
            out[i] = (drive_emitter(r,slot,tm*lens_model(self.mounts[i]),emission),Some(slot));
        }
        let active = route(selected,hovered);
        for i in 0..5 {
            let visible = active.map_or(true,|(a,b)|i!=a && i!=b);
            drive_wire(r,self.branches[i],tm,branch(i),visible,0.0);
        }
        let (low,high) = active.map(|(a,b)|(seat_point(a).x.min(seat_point(b).x),seat_point(a).x.max(seat_point(b).x))).unwrap_or((0.0,0.0));
        drive_wire(r,self.spine[0],tm,spine(-REAR_HALF_WIDTH,low),low> -REAR_HALF_WIDTH+1e-6,0.0);
        drive_wire(r,self.spine[1],tm,spine(high,REAR_HALF_WIDTH),high<REAR_HALF_WIDTH-1e-6,0.0);
        // A selected source with no hovered destination leaves the entire black wire in place.
        if active.is_none() {
            drive_wire(r,self.spine[0],tm,spine(-REAR_HALF_WIDTH,REAR_HALF_WIDTH),true,0.0);
            r.set_visible(self.spine[1],false);
        }
        let geometry = active.map(|(a,b)|[branch(a),spine(low,high),branch(b)])
            .unwrap_or([branch(0),spine(-REAR_HALF_WIDTH,REAR_HALF_WIDTH),branch(4)]);
        for j in 0..3 {
            let slot = self.glow[j];
            let e = wire_emission(geometry[j].1,if active.is_some() { finite_power(power) } else {0.0});
            let model = wire_model(tm,geometry[j]);
            let light = drive_emitter(r,slot,model,e);
            r.set_visible(slot,active.is_some());
            out[5+j] = (light,Some(slot));
        }
        out
    }

    /// Exact oriented box intersection against each physical lens/plinth, not a HUD hit region.
    /// The caller additionally owns scene LOS (breech, crew, etc.) before accepting an interaction.
    pub(super) fn pick(&self,tm:Mat4,eye:Vec3,dir:Vec3,max_m:f32)->Option<(usize,f32)> {
        if !max_m.is_finite() || max_m<=0.0 || !dir.is_finite() { return None; }
        (0..5).filter_map(|i| {
            let inv=(tm*self.mounts[i]).inverse();let o=inv.transform_point3(eye);let d=inv.transform_vector3(dir.normalize_or_zero());
            if d.z>=-1e-6{return None;}
            let lo=Vec3::new(-FOOT,-FOOT,0.0); let hi=Vec3::new(FOOT,FOOT,PLINTH+2.0*HALF);
            ray_box(o,d,lo,hi,max_m).map(|t|(i,t))
        }).min_by(|a,b|a.1.total_cmp(&b.1))
    }
    pub(super) fn inspection_pose(&self,tm:Mat4)->(Vec3,Vec3) {
        (tm.transform_point3(frame().transform_point3(Vec3::new(0.0,0.0,0.01))),tm.transform_vector3(NORMAL).normalize())
    }
    pub(super) fn receipt(&self,states:&[SeatStatus;5],time_s:f32,power:f32,selected:Option<usize>,hovered:Option<usize>)->Value {
        json!({"source":"Tiger Turret_0099; authored gameplay instrument", "wall_turret_local":WALL.to_array(),
            "inward_normal":NORMAL.normalize().to_array(),"paint_max_projection_m":self.paint_max_projection_m,
            "layout":"tank front up; two hull seats ahead of three turret seats",
            "front_indices_left_to_right":[4,3],"rear_indices_left_to_right":[1,0,2],
            "shadow_registrations":SHADOW_REGISTRATIONS,"selected":selected,"hovered":hovered,
            "wire_route":route(selected,hovered),"time_s":time_s,"power":finite_power(power),
            "seats":(0..5).map(|i|json!({"index":i,"seat":SEAT_NAMES[i],"actor_id":states[i].actor_id,
                "person_id":states[i].person_id,"name":states[i].name,"trained_role":states[i].trained_role,
                "condition":states[i].condition.name(),"pulse":pulse(states[i].condition,time_s,power),
                "moving":states[i].moving,"panel_position_m":seat_point(i).to_array(),"lens_turret_local":lens_model(self.mounts[i]).transform_point3(Vec3::ZERO).to_array(),
                "mesh":self.lenses[i][states[i].condition.color_index()]})).collect::<Vec<_>>()})
    }
}

// A wire tuple holds panel-local center and its ACTUAL box dimensions, not an illumination radius.
fn branch(i:usize)->(Vec3,Vec3) {
    let p=seat_point(i);let end=p.y-p.y.signum()*HALF;
    (Vec3::new(p.x,end*0.5,PLINTH),Vec3::new(WIRE_WIDTH,end.abs(),WIRE_WIDTH))
}
fn spine(a:f32,b:f32)->(Vec3,Vec3) {
    (Vec3::new((a+b)*0.5,0.0,PLINTH),Vec3::new((b-a).abs().max(1e-6),WIRE_WIDTH,WIRE_WIDTH))
}
fn wire_model(tm:Mat4,g:(Vec3,Vec3))->Mat4 { tm*frame()*Mat4::from_translation(g.0)*Mat4::from_scale(g.1) }
fn wire_emission(size:Vec3,power:f32)->LampEmission {
    let mut e=FiniteCubeLamp { half_size_m:0.5,srgb:WIRE_COLOR,emission:1.2 }.sample(power);
    // Same source-power-preserving isotropic approximation as FiniteCubeLamp. The finite
    // rectangular wire's area replaces the unit cube's area; no additional glow/fill is added.
    let area=2.0*(size.x*size.y+size.y*size.z+size.x*size.z);
    e.radiant_intensity*=area/6.0; e.source_radius_m=size.length()*0.5; e
}
fn drive_wire(r:&mut Renderer,slot:usize,tm:Mat4,g:(Vec3,Vec3),visible:bool,emission:f32) {
    r.set_model(slot,wire_model(tm,g));r.set_visible(slot,visible);r.set_emissive(slot,emission);
}
fn drive_emitter(r:&mut Renderer,slot:usize,model:Mat4,e:LampEmission)->PointLight {
    r.set_model(slot,model);r.set_visible(slot,true);r.set_emissive(slot,e.visible_emissive);
    PointLight { pos:model.transform_point3(Vec3::ZERO),source_radius:e.source_radius_m,color:e.linear_color,radiant_intensity:e.radiant_intensity }
}
fn ray_box(o:Vec3,d:Vec3,lo:Vec3,hi:Vec3,max:f32)->Option<f32> {
    let (mut near,mut far)=(0.0_f32,max);
    for axis in 0..3 {
        if d[axis].abs()<1e-8 { if o[axis]<lo[axis] || o[axis]>hi[axis] {return None;} }
        else {let a=(lo[axis]-o[axis])/d[axis];let b=(hi[axis]-o[axis])/d[axis];near=near.max(a.min(b));far=far.min(a.max(b));if far<near{return None;}}
    }
    Some(near)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crew_panel_health_phase_and_power_are_the_same_source_state() {
        assert!(pulse(Condition::Healthy,0.9,1.0));assert!(pulse(Condition::Dead,0.9,1.0));
        assert!(pulse(Condition::Wounded,0.2,1.0));assert!(!pulse(Condition::Wounded,0.8,1.0));
        for c in [Condition::Healthy,Condition::Wounded,Condition::Dead,Condition::Empty] {assert!(!pulse(c,0.2,0.0));}
        assert!(!pulse(Condition::Empty,0.2,1.0));assert_eq!(route(Some(1),Some(1)),None);assert_eq!(route(Some(1),Some(7)),None);
    }
    #[test]
    fn crew_panel_wire_energy_scales_with_actual_surface_area() {
        let a=Vec3::new(0.002,0.002,0.02);let b=Vec3::new(0.002,0.002,0.12);
        let area=|s:Vec3|2.0*(s.x*s.y+s.y*s.z+s.x*s.z);
        assert!((wire_emission(a,1.0).radiant_intensity/area(a)-wire_emission(b,1.0).radiant_intensity/area(b)).abs()<1e-8);
        assert_eq!(wire_emission(a,0.0).radiant_intensity,0.0);
        for a in 0..5 {for b in 0..5 {if a!=b {let low=seat_point(a).x.min(seat_point(b).x);let high=seat_point(a).x.max(seat_point(b).x);assert!(((low+REAR_HALF_WIDTH)+(high-low)+(REAR_HALF_WIDTH-high)-2.0*REAR_HALF_WIDTH).abs()<1e-6);}}}
    }
    #[test]
    fn crew_panel_two_front_three_rear_match_physical_seats() {
        assert_eq!(SEAT_NAMES[4],"Driver");assert_eq!(SEAT_NAMES[3],"Radioman");
        assert!(seat_point(4).x<seat_point(3).x);
        for i in [4,3] {assert!(seat_point(i).y>0.0);assert!(glyph_point(i).y>seat_point(i).y);}
        assert!(seat_point(1).x<seat_point(0).x&&seat_point(0).x<seat_point(2).x);
        for i in [1,0,2] {assert!(seat_point(i).y<0.0);assert!(glyph_point(i).y<seat_point(i).y);}
        for i in 0..5 {for j in 0..i {assert!(seat_point(i).distance(seat_point(j))>2.0*FOOT);}}
    }
    #[test]
    fn crew_panel_physical_picking_and_wires_share_seat_coordinates() {
        let panel=Panel{static_parts:Vec::new(),lenses:[[0;4];5],branches:[0;5],spine:[0;2],glow:[0;3],paint_max_projection_m:0.0,mounts:std::array::from_fn(|i|frame()*Mat4::from_translation(seat_point(i)))};
        let turret=Mat4::from_rotation_translation(glam::Quat::from_rotation_y(0.73),Vec3::new(12.,4.,-8.));
        let normal=turret.transform_vector3(NORMAL).normalize();
        for i in 0..5 {
            let at=(turret*lens_model(panel.mounts[i])).transform_point3(Vec3::ZERO);
            assert_eq!(panel.pick(turret,at+normal*0.4,-normal,0.8).unwrap().0,i);
            let(center,size)=branch(i);let p=seat_point(i);
            assert_eq!(center.x,p.x);assert!((center.y.abs()-size.y*0.5).abs()<1e-7);
            assert!((center.y.abs()+size.y*0.5-(p.y.abs()-HALF)).abs()<1e-7);
        }
        let empty=(turret*frame()).transform_point3(Vec3::new(0.0,0.0,0.4));
        assert!(panel.pick(turret,empty,-normal,0.8).is_none(),"wire bus is not an invisible seat button");
    }
    #[test]
    fn crew_panel_mounts_and_paint_contact_the_private_source_wall() {
        let path="assets/tanks/tiger/baked/tiger_turret.glb";
        if !crate::resource::exists(path) {eprintln!("private Tiger source unavailable: source-contact test skipped");return;}
        let source=crate::gltf_load::load_glb(path).unwrap();let patch=wall_patch(&source);
        let mut scene=Vec::new();let panel=Panel::append(&mut scene,&source);
        for i in 0..5 {
            let mount=panel.mounts[i];let foot=mount.transform_point3(Vec3::ZERO);let normal=mount.transform_vector3(Vec3::Z);
            assert!(crate::lamp_mounts::flat_patch(&patch,foot,normal,FOOT),"seat {i} leaves source wall");
            assert!(foot.distance(frame().transform_point3(seat_point(i)))<0.0021,"seat {i} projection leaves its logical position");
            let eye=lens_model(mount).transform_point3(Vec3::new(0.0,0.0,0.4));
            assert_eq!(panel.pick(Mat4::IDENTITY,eye,-normal,1.0).unwrap().0,i);
            assert!(panel.pick(Mat4::IDENTITY,eye,normal,1.0).is_none());
        }
        assert!(panel.paint_max_projection_m<0.004,"paint must stay on its adjacent source facet");
        for i in 0..5 {
            let paint=&scene[panel.static_parts[i*2+1].0];
            for v in &paint.vertices {let (_,_,d)=crate::lamp_mounts::closest_surface(&patch,Vec3::from(v.pos));assert!((d-PAINT_LIFT).abs()<0.00015);}
        }
        let eye=crate::game::tiger_periscopes::COMMANDER_EYE;
        let delta=WALL-eye;
        // Facing +Z, screen-right is -X. This actually mounts left of the normal forward view.
        assert!(delta.dot(Vec3::NEG_X)<0.0);assert!(delta.z>0.0);
    }
}
