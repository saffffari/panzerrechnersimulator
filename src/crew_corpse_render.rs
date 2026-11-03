//! Solo corpses keep actor-owned render pages after their five living appearance slots are reused.
//! Geometry and atlases come from the same reviewed source figures as the existing casualty path.
use super::*;

pub(super) const CAPACITY: usize = crate::expedition::INITIAL_PERSONNEL;
const PARTS: usize = crew_physics::CREW_BODY_PART_COUNT;
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
struct Pages {
    appearance: usize,
    flesh: [usize; PARTS],
    bones: [usize; PARTS],
    helmet: usize,
    gore: usize,
}
/// Actor-owned source allocations survive even when a treated body is no longer drawn. Their
/// allocation order is not role order: soldiers can be wounded or killed in any sequence.
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
pub(super) struct Layout { pages:[Option<Pages>;CAPACITY] }
impl Layout {
    pub(super) fn validate_for_actors(&self,actors:&[u32])->anyhow::Result<()> {
        for index in self.allocation_order()? {
            anyhow::ensure!(actors.contains(&(singleplayer_crew::SOLO_ACTOR_BASE+index as u32)),"retained corpse page has no person");
        } Ok(())
    }
    fn allocation_order(&self)->anyhow::Result<Vec<usize>> {
        let mut order=Vec::new();let mut used=std::collections::BTreeSet::new();
        for (actor,page) in self.pages.iter().enumerate() {
            let Some(page)=page else {continue;};
            anyhow::ensure!(page.appearance<5,"invalid retained corpse appearance");
            let slots:Vec<_>=page.flesh.iter().chain(&page.bones).chain([&page.helmet,&page.gore]).copied().collect();
            anyhow::ensure!(slots.iter().enumerate().all(|(offset,slot)|page.flesh[0].checked_add(offset)==Some(*slot)&&used.insert(*slot)),
                "overlapping or discontinuous retained corpse source pages");
            order.push(actor);
        }
        order.sort_by_key(|actor|self.pages[*actor].as_ref().unwrap().flesh[0]);Ok(order)
    }
}
pub(super) struct Corpses {
    flesh: [[usize; PARTS]; 5],
    bones: [usize; PARTS],
    helmet: usize,
    gore: usize,
    pages: [Option<Pages>; CAPACITY],
    /// Restore-only presentation must not allocate a body which had not yet been presented
    /// when the snapshot was taken. The next ordinary update performs that same first draw.
    preserve_saved_allocations:bool,
    source_triangles: Vec<Vec<Vec<[Vec3;3]>>>,
    /// Voxel tissue for struck bodies (`crew_corpse_tissue`); absent without the fitted skeleton.
    tissue: Option<crew_corpse_tissue::TissueDraw>,
}
fn actor_slot(actor_id: u32) -> Option<usize> {
    actor_id.checked_sub(singleplayer_crew::SOLO_ACTOR_BASE).map(|i|i as usize).filter(|&i|i<CAPACITY)
}
impl Corpses {
    pub(super) fn preserve_saved_allocations(&mut self,preserve:bool){self.preserve_saved_allocations=preserve;}
    pub(super) fn snapshot_layout(&self)->Layout {Layout{pages:std::array::from_fn(|i|self.pages[i].clone())}}
    /// Recreate immutable source pools before the renderer imports retained draw lists. This
    /// does not draw, advance a body, duplicate a person, or assume casualties arrived by rank.
    pub(super) fn restore_layout(&mut self,layout:Layout,r:&mut Renderer)->anyhow::Result<()> {
        let order=layout.allocation_order()?;
        anyhow::ensure!(self.pages.iter().zip(&layout.pages).all(|(old,saved)|old.is_none()||old==saved),
            "candidate corpse allocation differs from retained source identity");
        for actor in order {
            if self.pages[actor].is_some(){continue;}
            let saved=layout.pages[actor].as_ref().unwrap();
            anyhow::ensure!(r.retained_instance_pool_count()==saved.flesh[0],"retained corpse allocation order differs at actor {actor}");
            let appearance=saved.appearance;
            let actual=Pages {
                appearance,
                flesh:std::array::from_fn(|part|r.add_registered_moving_pool(self.flesh[appearance][part],1).expect("validated corpse source page")),
                bones:std::array::from_fn(|part|r.add_registered_moving_pool(self.bones[part],1).expect("validated bone source page")),
                helmet:r.add_registered_moving_pool(self.helmet,1).expect("validated helmet source"),
                gore:r.add_registered_moving_pool(self.gore,1).expect("validated blood source"),
            };
            anyhow::ensure!(&actual==saved,"rebuilt corpse source pages differ at actor {actor}");
            self.pages[actor]=Some(actual);
        }
        Ok(())
    }
    /// `fitted_bones`: the owner's PSX skeleton fitted inside the relaxed figure (`crew_skeleton`);
    /// without it the reviewed skeleton stands in and bodies are not drawn opened.
    pub(super) fn allocate(device:&wgpu::Device,queue:&wgpu::Queue,r:&mut Renderer,
        fitted_bones:Option<&[Option<CpuMesh>;PARTS]>)->anyhow::Result<Self> {
        let mut flesh=Vec::new();
        let mut source_triangles=Vec::new();
        let mut figures=Vec::new();
        for (appearance,path) in CREW_RELAXED_GLBS.into_iter().enumerate() {
            let mesh=load_full_crew_mesh(path,"persistent solo corpse")
                .ok_or_else(||anyhow::anyhow!("corpse source {path} is unavailable"))?;
            let source=partition_relaxed_crew_mesh(&mesh);
            let mut pages=Vec::new();
            let mut triangles=Vec::new();
            for (part,mesh) in source.into_iter().enumerate() {
                let mesh=mesh.ok_or_else(||anyhow::anyhow!("corpse source {appearance} lacks anatomical page {part}"))?;
                triangles.push(mesh.indices.chunks_exact(3).map(|ids|
                    [0,1,2].map(|i|Vec3::from_array(mesh.vertices[ids[i]as usize].pos))).collect());
                pages.push(r.register_instanced_prototype(device,queue,&mesh));
            }
            flesh.push(pages.try_into().ok().expect("eleven source pages"));
            source_triangles.push(triangles);
            figures.push(mesh);
        }
        let skeleton=fitted_bones.cloned().or_else(||load_reviewed_skeleton_mesh().and_then(|source|partition_reviewed_skeleton_mesh(&source)))
            .ok_or_else(||anyhow::anyhow!("reviewed corpse skeleton is unavailable"))?;
        let helmet_mesh=load_crew_steel_helmet()?;
        let tissue=fitted_bones.and_then(|bones|crew_corpse_tissue::TissueDraw::allocate(device,queue,r,&figures,bones,&helmet_mesh)
            .map_err(|e|eprintln!("[crew] voxel tissue unavailable: {e:#}")).ok());
        let mut bones=Vec::new();
        for (part,mesh) in skeleton.into_iter().enumerate() {
            let mesh=mesh.ok_or_else(||anyhow::anyhow!("reviewed skeleton lacks anatomical page {part}"))?;
            bones.push(r.register_instanced_prototype(device,queue,&mesh));
        }
        let helmet=r.register_instanced_prototype(device,queue,&helmet_mesh);
        let gore=r.register_instanced_prototype(device,queue,&crew_gore_mesh());
        Ok(Self {flesh:flesh.try_into().ok().expect("five source appearances"),
            bones:bones.try_into().ok().expect("eleven source pages"),helmet,gore,
            pages:std::array::from_fn(|_|None),preserve_saved_allocations:false,source_triangles,tissue})
    }
    /// Solve the strikes that killed the ragdoll's man on his cells: the share of each part's soft
    /// cells that remain.
    pub(super) fn strike_tissue(&mut self,ragdoll:&CrewRagdoll,strikes:&[crew_tissue::Strike])->Option<[f32;PARTS]> {
        self.tissue.as_mut().map(|tissue|tissue.strike(ragdoll,strikes))
    }
    /// Move the pieces of solved bodies (`crew_tissue::integrate_chunks`).
    pub(super) fn integrate_tissue(&mut self,world:&FragmentWorld,player:&Tank,dt:f32) {
        if let Some(tissue)=self.tissue.as_mut() {tissue.integrate(world,player,dt);}
    }
    pub(super) fn reset_tissue(&mut self) {
        if let Some(tissue)=self.tissue.as_mut() {tissue.reset();}
    }
    pub(super) fn hide(&self,r:&mut Renderer) {
        if let Some(tissue)=&self.tissue {tissue.hide(r);}
        for pages in self.pages.iter().flatten() {
            for &pool in pages.flesh.iter().chain(&pages.bones).chain([&pages.helmet,&pages.gore]) {
                r.set_moving_instanced_mats(pool,&[]);
            }
        }
    }
    pub(super) fn draw(&mut self,player:&Tank,bodies:&[CrewRagdoll],living:&[u32],appearances:&[(u32,crate::lan::LanRole)],r:&mut Renderer) {
        let mut present=[false;CAPACITY];
        if let Some(tissue)=self.tissue.as_mut() {tissue.begin_frame();}
        for body in bodies {
            let Some(index)=actor_slot(body.casualty.peer_id) else {continue;};
            present[index]=true;
            if self.preserve_saved_allocations&&self.pages[index].is_none(){continue;}
            let appearance=crew_identity_index(appearances.iter().find(|(actor,_)|*actor==body.casualty.peer_id)
                .map_or(body.casualty.identity,|(_,identity)|*identity));
            let pages=self.pages[index].get_or_insert_with(||Pages {
                appearance,
                flesh:std::array::from_fn(|part|r.add_registered_moving_pool(self.flesh[appearance][part],1).expect("one body page")),
                bones:std::array::from_fn(|part|r.add_registered_moving_pool(self.bones[part],1).expect("one bone page")),
                helmet:r.add_registered_moving_pool(self.helmet,1).expect("one helmet"),
                gore:r.add_registered_moving_pool(self.gore,1).expect("one blood pool"),
            });
            // A legacy fresh-match reset may assign a different face to the same bounded ordinal.
            // Rebind its existing pages instead of allocating an unbounded new GPU pool per retry.
            if pages.appearance!=appearance&&!self.preserve_saved_allocations {
                for part in 0..PARTS {
                    r.set_moving_instanced_mats(pages.flesh[part],&[]);
                    assert!(r.rebind_empty_moving_pool(pages.flesh[part],self.flesh[appearance][part]));
                }
                pages.appearance=appearance;
            }
            // A body solved on its cells is drawn as its cells, never as the figure's triangles or
            // the skeleton page (the skeleton is voxelised among them); his helmet stays a mesh.
            let peer=body.casualty.peer_id;
            let voxel=self.tissue.as_ref().is_some_and(|tissue|tissue.has_body(peer));
            if voxel {
                let models=crew_physics::CrewBodyPart::ALL.map(|part|crew_ragdoll_bone_segment_model(player,body,part));
                // A part is joined to its parent while both are in the articulated simulation and
                // its own joint has not come apart.
                let joined=crew_physics::CrewBodyPart::ALL.map(|part|{
                    let s=body.segments[part.index()];
                    s.present&&!s.detached&&crew_anatomical_parent(part).is_none_or(|p|body.segments[p.index()].present)
                });
                if let Some(tissue)=self.tissue.as_mut() {tissue.add_body(peer,appearance,&models,&joined);}
            }
            for part in crew_physics::CrewBodyPart::ALL {
                let i=part.index();
                if voxel {
                    show(r,pages.flesh[i],None);
                    show(r,pages.bones[i],None);
                    continue;
                }
                let (flesh,bone)=casualty_anatomy_visibility(body.segments[i].present);
                show(r,pages.flesh[i],flesh.then(||crew_ragdoll_segment_model(player,body,part)));
                show(r,pages.bones[i],bone.then(||crew_ragdoll_bone_segment_model(player,body,part)));
            }
            // The helmet is a rigid body, never cells: on his head until it came away.
            let loose=self.tissue.as_ref().and_then(|tissue|tissue.loose_helmet_model(peer,player));
            show(r,pages.helmet,loose.or_else(||crew_ragdoll_helmet_model(player,body)));
            // A solved body's blood is the droplets and their film (`surface_blood`), not the old
            // under-body pool slabs, which float at odd angles off a body that lies on uneven parts.
            show(r,pages.gore,(!voxel&&body.settled&&!living.contains(&body.casualty.peer_id)).then(||crew_ragdoll_gore_model(player,body)));
        }
        if let Some(tissue)=self.tissue.as_mut() {
            let faces:Vec<(u32,usize)>=self.pages.iter().enumerate().filter_map(|(i,p)|
                p.as_ref().map(|p|(singleplayer_crew::SOLO_ACTOR_BASE+i as u32,p.appearance))).collect();
            tissue.add_pieces(player,&|actor|faces.iter().find(|(a,_)|*a==actor).map_or(0,|(_,f)|*f));
            tissue.end_frame(r);
        }
        for (index,pages) in self.pages.iter().enumerate() {
            if present[index] {continue;}
            if let Some(pages)=pages {
                for &pool in pages.flesh.iter().chain(&pages.bones).chain([&pages.helmet,&pages.gore]) {
                    r.set_moving_instanced_mats(pool,&[]);
                }
            }
        }
    }
    pub(super) fn receipt(&self)->serde_json::Value {
        serde_json::json!({"available":true,"tissue":self.tissue.as_ref().map(crew_corpse_tissue::TissueDraw::receipt),"capacity":CAPACITY,"allocated_actors":self.pages.iter().flatten().count(),
            "pages_per_actor":PARTS*2+2,"actor_pages":self.pages.iter().enumerate().filter_map(|(index,p)|
                p.as_ref().map(|p|serde_json::json!({"actor_id":singleplayer_crew::SOLO_ACTOR_BASE+index as u32,
                    "appearance":p.appearance,"flesh_pools":p.flesh,"bone_pools":p.bones,"helmet_pool":p.helmet}))).collect::<Vec<_>>()})
    }
    /// Same retained flesh triangles and part matrices used by drawing, for bounded drag sweeps.
    pub(super) fn world_vertices(&self,player:&Tank,body:&CrewRagdoll,registered:Option<crate::lan::LanRole>)->Vec<Vec3>{
        let appearance=source_appearance(registered,actor_slot(body.casualty.peer_id).and_then(|i|self.pages[i].as_ref().map(|p|p.appearance)),body.casualty.identity);let mut points=Vec::new();
        for part in crew_physics::CrewBodyPart::ALL{if !body.segments[part.index()].present{continue;}
            let model=crew_ragdoll_segment_model(player,body,part);
            for triangle in &self.source_triangles[appearance][part.index()]{points.extend(triangle.map(|p|model.transform_point3(p)));}
        }points
    }
    /// Picking runs only for nearby retained bodies, against the exact source flesh triangles.
    /// No enlarged interaction box can select a corpse through the open space beside it.
    pub(super) fn ray(&self,player:&Tank,bodies:&[CrewRagdoll],eye:Vec3,dir:Vec3,reach:f32,registered:impl Fn(u32)->Option<crate::lan::LanRole>)->Option<(u32,f32)> {
        let mut best:Option<(u32,f32)>=None;
        for body in bodies {
            let center=crew_ragdoll_segment_model(player,body,crew_physics::CrewBodyPart::Torso)
                .transform_point3(crew_physics::CrewBodyPart::Torso.rest_center());
            if center.distance(eye)>reach+1.2 {continue;}
            let appearance=source_appearance(registered(body.casualty.peer_id),actor_slot(body.casualty.peer_id).and_then(|i|self.pages[i].as_ref().map(|p|p.appearance)),body.casualty.identity);
            for part in crew_physics::CrewBodyPart::ALL {
                if !body.segments[part.index()].present {continue;}
                let model=crew_ragdoll_segment_model(player,body,part);let inv=model.inverse();
                let origin=inv.transform_point3(eye);let direction=inv.transform_vector3(dir);
                for triangle in &self.source_triangles[appearance][part.index()] {
                    if let Some(t)=ray_triangle(origin,direction,*triangle) {
                        if t<=reach && best.is_none_or(|(_,old)|t<old) {best=Some((body.casualty.peer_id,t));}
                    }
                }
            }
        }
        best
    }
}
// Draw and contact both resolve the actor's immutable source identity, even if its live
// control slot was reassigned. A saved page remains a fallback for an already retained actor.
fn source_appearance(registered:Option<crate::lan::LanRole>,retained:Option<usize>,slot:crate::lan::LanRole)->usize {
    registered.map(crew_identity_index).or(retained).unwrap_or_else(||crew_identity_index(slot))
}
fn ray_triangle(origin:Vec3,dir:Vec3,t:[Vec3;3])->Option<f32> {
    let e1=t[1]-t[0];let e2=t[2]-t[0];let p=dir.cross(e2);let det=e1.dot(p);
    if det.abs()<1e-9 {return None;}
    let v=origin-t[0];let u=v.dot(p)/det;if !(0.0..=1.0).contains(&u) {return None;}
    let q=v.cross(e1);let w=dir.dot(q)/det;if w<0.0||u+w>1.0 {return None;}
    let distance=e2.dot(q)/det;(distance>0.0).then_some(distance)
}
fn show(r:&mut Renderer,pool:usize,model:Option<Mat4>) {
    if let Some(model)=model {r.set_moving_instanced_mats(pool,&[model]);}
    else {r.set_moving_instanced_mats(pool,&[]);}
}
impl Game {
    pub fn crew_corpse_render_receipt(&self)->serde_json::Value {
        self.crew_corpse_render.borrow().as_ref().map_or_else(||serde_json::json!({"available":false}),Corpses::receipt)
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test]fn source_contact_appearance_survives_assignment_and_unpresented_body(){
        use crate::lan::LanRole;
        let original=LanRole::Loader;let reassigned=LanRole::Gunner;
        assert_eq!(source_appearance(Some(original),None,reassigned),crew_identity_index(original));
        assert_eq!(source_appearance(Some(original),Some(crew_identity_index(original)),reassigned),crew_identity_index(original));
        assert_eq!(source_appearance(None,Some(crew_identity_index(original)),reassigned),crew_identity_index(original));
    }

    fn pages(start:usize,appearance:usize)->Pages {Pages{appearance,flesh:std::array::from_fn(|i|start+i),bones:std::array::from_fn(|i|start+PARTS+i),helmet:start+PARTS*2,gore:start+PARTS*2+1}}
    #[test]fn casualty_source_layout_retains_reverse_death_order_and_rejects_aliases(){
        let mut layout=Layout{pages:std::array::from_fn(|_|None)};
        layout.pages[12]=Some(pages(400,3));layout.pages[1]=Some(pages(424,1));
        assert_eq!(layout.allocation_order().unwrap(),vec![12,1]);
        let saved=bincode::serialize(&layout).unwrap();let restored:Layout=bincode::deserialize(&saved).unwrap();assert_eq!(restored,layout);
        layout.pages[2]=Some(pages(400,0));assert!(layout.allocation_order().is_err());
        layout.pages[2]=None;layout.pages[1].as_mut().unwrap().gore=999;assert!(layout.allocation_order().is_err());
    }
    #[test] fn every_finite_actor_has_a_distinct_permanent_body_slot() {
        let base=singleplayer_crew::SOLO_ACTOR_BASE;
        for i in 0..CAPACITY {assert_eq!(actor_slot(base+i as u32),Some(i));}
        assert_eq!(actor_slot(base-1),None);assert_eq!(actor_slot(base+CAPACITY as u32),None);
        // The same visual role returning for a new person cannot reuse the previous corpse slot.
        assert_ne!(actor_slot(base),actor_slot(base+5));
    }
}
