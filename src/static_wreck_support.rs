//! Static whole-source wreck support, including inverted poses.
//! All original source vertices are retained. A bounded energy search proposes orientation;
//! actual contact polygon/estimated COM and a full-footprint terrain certificate admit it.
//! This is initial placement, not crash dynamics, suspension, or a swept path through props.
use glam::{Mat4,Quat,Vec2,Vec3};
use crate::{gltf_load::CpuMesh,source_point_support::{SourcePointBvh,SupportQuery},terrain::Heightfield};

const BAND:f32=0.005;
const MARGIN:f32=0.010;
const MAX_PATCH_NODES:usize=65_536;
const MAX_SOURCE_TRIANGLES:usize=100_000;
const DESCENT_QUERIES:usize=96;
const AUTHORING_NEIGHBORS:usize=8;
// Keep one earlier best pose, without rerunning descent, so a later narrow minimum does not
// discard an already useful neighborhood. Each authoring phase has eight fixed probes.
const CHECKPOINT_QUERY:usize=DESCENT_QUERIES-AUTHORING_NEIGHBORS;
const MAX_AUTHORING_QUERIES:usize=2*AUTHORING_NEIGHBORS;
const MAX_QUERIES:usize=DESCENT_QUERIES+MAX_AUTHORING_QUERIES;
const MAX_FACE_PAIRS:usize=250_000;
const MAX_FACE_CONTACTS:usize=65_536;
const MAX_CORRECTION:f32=12.0*std::f32::consts::PI/180.0;

#[derive(Clone,Copy,Debug)]
pub enum Rejected {Invalid,SourceBudget,Extent,Opening,PatchBudget,NoSupport,SearchBudget,
    FaceBudget,ContactBudget,Unstable, UnsupportedAfterClearance{range_m:f32}}
pub struct SourceStaticBody {
    points:Vec<Vec3>, // all CpuMesh source vertices, including coincident seam vertices
    triangles:Vec<[u32;3]>,
    bvh:SourcePointBvh,
    estimated_com:Vec3,
    radius:f32,
}
pub struct Admitted {
    pub model:Mat4,
    pub terrain_residual_range_m:f32,
    pub numerical_allowance_m:f32,
    pub contact_count:usize,
    pub support_queries:usize,
    pub descent_queries:usize,
    pub authoring_queries:usize,
    pub checkpoint_authoring_queries:usize,
    pub checkpoint_face_certificates:usize,
    pub tested_points:usize,
    pub tested_face_pairs:usize,
    pub face_height_correction_m:f32,
    pub face_refinements:usize,
    pub added_face_witnesses:usize,
    /// Explicit initial-placement correction, not a simulated downhill/falling trajectory.
    pub authoring_neighbor:Option<AuthoringNeighbor>,
}
pub struct AuthoringNeighbor {pub angle_delta:Vec2,pub maximum_source_displacement_m:f32,pub com_height_change_m:f64,
    pub reference_descent_query:usize}
#[derive(Clone,Copy)]
struct SearchCheckpoint {angles:Vec2,vertex_height:f32}

/// Admission is performed before any renderer, walk-surface, salvage or collision consumer.
/// The source index never changes. Unresolved originals are retained and explicitly reported;
/// a failure does not make a physical object disappear or certify its unsupported old pose.
pub fn admit_manifest_pages(path:&str,pages:&mut[crate::editor::ManifestPage],mesh:&CpuMesh,
    hf:&Heightfield,ownership:&crate::static_wreck_ownership::Ownership)->Vec<serde_json::Value> {
    use crate::static_wreck_ownership::Policy;
    let eligible=pages.iter().flat_map(|p|&p.source_indices).any(|&i|ownership.policy(i,path).is_some());
    if !eligible{return Vec::new();}
    let needs_source=pages.iter().flat_map(|p|&p.source_indices)
        .any(|&i|matches!(ownership.policy(i,path),Some(Policy::Independent)));
    let source=needs_source.then(||SourceStaticBody::new(mesh));
    let node=|i:u32,j:u32|hf.detail.as_ref().map_or_else(
        ||hf.heights[j as usize*hf.res+i as usize],|d|d.lattice_node_height(i,j));
    let mut receipt=Vec::new();
    for page in pages {
        for (&index,model) in page.source_indices.iter().zip(&mut page.transforms) {
            let Some(policy)=ownership.policy(index,path) else {continue;};
            match policy {
                Policy::Preserve(reason)=>receipt.push(serde_json::json!({"asset":path,"source_index":index,
                    "status":"preserved_owner_pose","reason":reason})),
                Policy::Independent=>{
                    let original=*model;
                    let result=source.as_ref().expect("independent source was requested").as_ref()
                        .map_err(|e|*e).and_then(|s|s.place(original,hf,&node));
                    match result {
                        Ok(admitted)=>{
                            *model=admitted.model;
                            receipt.push(serde_json::json!({"asset":path,"source_index":index,"status":"admitted",
                                "original_model":original.to_cols_array(),"model":model.to_cols_array(),
                                "com_model":"uniform source surface-area thin-shell estimate",
                                "com_source_m":source.as_ref().unwrap().as_ref().unwrap().estimated_com().to_array(),
                                "source_vertices_retained":mesh.vertices.len(),"source_triangles_retained":mesh.indices.len()/3,
                                "terrain_residual_range_m":admitted.terrain_residual_range_m,
                                "numerical_allowance_m":admitted.numerical_allowance_m,"contact_count":admitted.contact_count,
                                "support_queries":admitted.support_queries,"descent_queries":admitted.descent_queries,
                                "authoring_queries":admitted.authoring_queries,
                                "checkpoint_authoring_queries":admitted.checkpoint_authoring_queries,
                                "checkpoint_face_certificates":admitted.checkpoint_face_certificates,
                                "query_limits":{"descent":DESCENT_QUERIES,"authoring":MAX_AUTHORING_QUERIES,
                                    "authoring_per_neighborhood":AUTHORING_NEIGHBORS,"total":MAX_QUERIES},
                                "tested_source_points":admitted.tested_points,
                                "tested_face_pairs":admitted.tested_face_pairs,"face_height_correction_m":admitted.face_height_correction_m,
                                "face_refinements":admitted.face_refinements,"added_face_witnesses":admitted.added_face_witnesses,
                                "placement_method":if admitted.authoring_neighbor.is_some(){"nearby_full_face_certified_authoring_pose"}else{"bounded_energy_search"},
                                "authoring_neighbor":admitted.authoring_neighbor.as_ref().map(|n|serde_json::json!({
                                    "angle_delta_rad":n.angle_delta.to_array(),"maximum_source_displacement_m":n.maximum_source_displacement_m,
                                    "com_height_change_m":n.com_height_change_m,"reference_descent_query":n.reference_descent_query,
                                    "simulated_motion":false,"neighbor_prop_collision_verified":false}))}));
                        }
                        Err(reason)=>receipt.push(serde_json::json!({"asset":path,"source_index":index,
                            "status":"unresolved_original_pose_retained","reason":format!("{reason:?}"),
                            "model":original.to_cols_array()})),
                    }
                }
            }
        }
    }
    receipt
}
impl SourceStaticBody {
    /// Uses the actual flattened source geometry. Uniform surface-area density estimates a thin
    /// shell's COM; this is NOT a measured vehicle mass distribution or historical specification.
    /// The owner decides which complete independent assets are eligible; no filename inference.
    pub fn new(mesh:&CpuMesh)->Result<Self,Rejected> {
        if mesh.indices.len()%3!=0{return Err(Rejected::Invalid);}
        if mesh.indices.len()/3>MAX_SOURCE_TRIANGLES{return Err(Rejected::SourceBudget);}
        let mut area=0.0_f64;let mut moment=glam::DVec3::ZERO;
        for t in mesh.indices.chunks_exact(3) {
            if t.iter().any(|&i|i as usize>=mesh.vertices.len()){return Err(Rejected::Invalid);}
            let [a,b,c]=[t[0],t[1],t[2]].map(|i|Vec3::from(mesh.vertices[i as usize].pos).as_dvec3());
            let w=(b-a).cross(c-a).length()*0.5;
            if !w.is_finite(){return Err(Rejected::Invalid);}
            area+=w;moment+=(a+b+c)*(w/3.0);
        }
        if area<=1e-12{return Err(Rejected::Invalid);}
        Self::with_estimated_com(mesh,(moment/area).as_vec3())
    }
    pub fn with_estimated_com(mesh:&CpuMesh,estimated_com:Vec3)->Result<Self,Rejected> {
        if mesh.indices.len()%3!=0||mesh.indices.iter().any(|&i|i as usize>=mesh.vertices.len()){return Err(Rejected::Invalid);}
        if mesh.indices.len()/3>MAX_SOURCE_TRIANGLES{return Err(Rejected::SourceBudget);}
        if !estimated_com.is_finite(){return Err(Rejected::Invalid);}
        let points:Vec<_>=mesh.vertices.iter().map(|v|Vec3::from(v.pos)).collect();
        let bvh=SourcePointBvh::new(&points).ok_or(Rejected::Invalid)?;
        let (lo,hi)=bvh.bounds();let radius=lo.abs().max(hi.abs()).length();
        let triangles=mesh.indices.chunks_exact(3).map(|t|[t[0],t[1],t[2]]).collect();
        Ok(Self{points,triangles,bvh,estimated_com,radius})
    }
    pub fn estimated_com(&self)->Vec3 {self.estimated_com}
    pub fn source_vertex_count(&self)->usize {self.points.len()}
    /// exact_node reads the actual coarse or detail lattice node directly; never obtain it by
    /// grade_at(i*step-half), whose inverse grid arithmetic can interpolate a neighbouring node.
    /// original_model and returned model apply to the SAME actual source mesh, including any
    /// source-baked overturned orientation. A rejected placement returns a reason, not a deletion.
    pub fn place(&self,original_model:Mat4,hf:&Heightfield,
        exact_node:&impl Fn(u32,u32)->f32)->Result<Admitted,Rejected> {
        let canonical=original_model;
        let (s,q,t)=canonical.to_scale_rotation_translation();
        if !canonical.is_finite()||s.min_element()<=0.
            ||s.max_element()-s.min_element()>1e-5 {return Err(Rejected::Invalid);}
        let rebuilt=Mat4::from_scale_rotation_translation(s,q,t);
        if canonical.to_cols_array().iter().zip(rebuilt.to_cols_array()).any(|(a,b)|(a-b).abs()>1e-5){return Err(Rejected::Invalid);}
        let at=Vec2::new(t.x,t.z);let scale=s.x;
        let patch=Patch::new(hf,at,self.radius*scale,exact_node)?;
        let (yaw,initial_pitch,initial_roll)=q.to_euler(glam::EulerRot::YXZ);
        let mut angles=Vec2::new(initial_pitch,initial_roll);
        let mut queries=0;let mut tested=0;let mut face_pairs=0;let mut refinements=0;
        let mut checkpoint:Option<SearchCheckpoint>=None;
        // Cutting-plane witnesses are actual clipped points on retained source faces. They
        // supplement the immutable original vertex BVH, never replace/remove source geometry.
        let mut witnesses:Vec<Vec3>=Vec::new();
        let mut witnessed:std::collections::HashSet<_>=self.points.iter().map(|p|p.to_array().map(f32::to_bits)).collect();
        let query=|angles:Vec2,witnesses:&[Vec3]| {
            let r=Quat::from_euler(glam::EulerRot::YXZ,yaw,angles.x,angles.y);
            let m=Mat4::from_scale_rotation_translation(Vec3::splat(scale),r,Vec3::ZERO);
            self.bvh.query(&self.points,m,at,BAND,&|x,z|hf.grade_at(x,z),&|_,_|patch.max_height+patch.height_roundoff)
                .map(|mut result| {
                    let vertex_height=result.required_origin_y;
                    for (i,&point) in witnesses.iter().enumerate() {
                        let p=m.transform_point3(point);let required=hf.grade_at(at.x+p.x,at.y+p.z)-p.y;
                        result.required_origin_y=result.required_origin_y.max(required);
                        result.contacts.push(crate::source_point_support::Contact {source_index:self.points.len()+i,
                            offset:p,required_origin_y:required});
                    }
                    result.contacts.retain(|c|result.required_origin_y-c.required_origin_y<=BAND);
                    result.stats.tested_points+=witnesses.len();
                    (r,result,vertex_height)
                })
        };
        let (mut rotation,mut support,mut vertex_height)=query(angles,&witnesses).ok_or(Rejected::NoSupport)?;
        queries+=1;tested+=support.stats.tested_points;
        let mut step=0.05_f32;
        // A face certificate's mandatory numerical lift consumes the physical contact band.
        // Preserve the largest already measured allowance in later proposals; it is a search
        // constraint only. The final pose still receives its own unchanged exact certificate.
        let mut search_lift=0.0_f32;
        loop {
        let mut stopped=None;
        // Source/terrain geometry drives the minimum. A scripted upright/rest angle is never
        // substituted; a stable inverted roof/barrel contact may already be the correct answer.
        while !stable(&support,rotation*(self.estimated_com*scale),search_lift) {
            if queries>=DESCENT_QUERIES{stopped=Some(Rejected::SearchBudget);break;}
            let energy=support.required_origin_y+(rotation*(self.estimated_com*scale)).y;
            let mut improvement=None;let mut lowest=energy;
            // Coordinate descent alone stalls on diagonal source support ridges: either
            // pitch or roll by itself raises one endpoint, while a coupled tilt lowers COM.
            // This proposal follows gravity about the closest actual support edge. It still
            // has to lower the measured full-source energy and pass the unchanged margin.
            let guide=gravity_angle_direction(&support,rotation*(self.estimated_com*scale),yaw,angles.x,search_lift);
            for d in [Vec2::new(step,0.),Vec2::new(-step,0.),Vec2::new(0.,step),Vec2::new(0.,-step)]
                .into_iter().chain(guide.map(|d|d*step)) {
                let candidate=angles+d;
                if (candidate-Vec2::new(initial_pitch,initial_roll)).abs().max_element()>MAX_CORRECTION {continue;}
                if queries>=DESCENT_QUERIES {break;}
                let (r,sup,vertex)=query(candidate,&witnesses).ok_or(Rejected::NoSupport)?;
                queries+=1;tested+=sup.stats.tested_points;
                let e=sup.required_origin_y+(r*(self.estimated_com*scale)).y;
                let mut supported=false;
                if e<lowest-1e-6 {
                    supported=stable(&sup,r*(self.estimated_com*scale),search_lift);
                    lowest=e;improvement=Some((candidate,r,sup,vertex));
                }
                if queries==CHECKPOINT_QUERY {
                    // Match the best pose at this exact search cutoff, including an earlier
                    // better candidate in the current iteration, not merely the latest trial.
                    let (a,v)=improvement.as_ref().map(|(a,_,_,v)|(*a,*v)).unwrap_or((angles,vertex_height));
                    checkpoint=Some(SearchCheckpoint {angles:a,vertex_height:v});
                }
                // A downhill supported candidate receives its face certificate immediately.
                if supported {break;}
            }
            if let Some((a,r,sup,vertex))=improvement {angles=a;rotation=r;support=sup;vertex_height=vertex;}
            else {step*=0.5;if step<0.00005{stopped=Some(Rejected::Unstable);break;}}
        }
        let offset=Mat4::from_scale_rotation_translation(Vec3::splat(scale),rotation,Vec3::ZERO);
        let (lo,hi)=self.bvh.bounds();let mut min=Vec2::splat(f32::INFINITY);let mut max=Vec2::splat(f32::NEG_INFINITY);
        for i in 0..8 {
            let p=offset.transform_point3(Vec3::new(if i&1==0{lo.x}else{hi.x},if i&2==0{lo.y}else{hi.y},if i&4==0{lo.z}else{hi.z}));
            let v=Vec2::new(p.x,p.z)+at;min=min.min(v);max=max.max(v);
        }
        let pad=0.0001+8.*f32::EPSILON*(at.abs().max_element()+self.radius*scale);
        min-=Vec2::splat(pad);max+=Vec2::splat(pad);
        if !hf.openings.meeting(min,max).is_empty(){return Err(Rejected::Opening);}
        let certificate=patch.certificate(min,max)?;
        let allowance=0.00005+2.*patch.height_roundoff+8.*f32::EPSILON*(2.*self.radius*scale
            +(at.abs().max_element()+self.radius*scale)*certificate.max_grade);
        let lift=certificate.residual_range+allowance;
        let com=rotation*(self.estimated_com*scale);
        // Affine/near-affine terrain receives a cheap conservative whole-face certificate.
        // Curved ground instead checks the actual low source faces; no blanket terrain sink.
        let (y,contact_count,face_height_correction_m)=if lift<BAND&&stable(&support,com,lift) {
            (round_up(support.required_origin_y as f64+lift as f64),contacts_after_lift(&support,lift).len(),support.required_origin_y-vertex_height)
        }else{
            // Check faces even when vertex descent stalled: true face support can exist with
            // no original vertex near the terrain. Charge all refinement passes to the SAME
            // existing cumulative face-pair budget; preprocessing cannot grow without bound.
            let faces=patch.source_faces(&self.points,&self.triangles,offset,at,vertex_height,
                allowance,MAX_FACE_PAIRS-face_pairs)?;
            face_pairs+=faces.pairs;
            if !stable_points(&faces.contacts,com) {
                #[cfg(test)] {
                    let raw=contacts_after_lift(&support,0.);let clear=contacts_after_lift(&support,allowance);
                    let mut gaps:Vec<_>=support.contacts.iter().map(|c|support.required_origin_y as f64-c.required_origin_y as f64).collect();
                    gaps.sort_by(f64::total_cmp);gaps.truncate(12);
                    eprintln!("{}",serde_json::json!({"static_support_diagnostic":"face_rejected",
                        "at":at.to_array(),"angles":angles.to_array(),"queries":queries,"refinements":refinements,"search_step_rad":step,
                        "allowance_m":allowance,"search_lift_m":search_lift,"vertex_required_y":vertex_height,
                        "witness_required_y":support.required_origin_y,"face_y":faces.y,
                        "raw_contacts":raw.len(),"clearance_contacts":clear.len(),"face_contacts":faces.contacts.len(),
                        "raw_margin_m":contact_margin(&raw,com),"clearance_margin_m":contact_margin(&clear,com),
                        "face_margin_m":contact_margin(&faces.contacts,com),"nearest_vertex_gaps_m":gaps,
                        "nearest_face_gaps_m":faces.gaps_m.iter().copied().take(12).collect::<Vec<_>>()}));
                }
                if stopped.is_some()||queries>=DESCENT_QUERIES {
                    return self.authoring_after_failure(at,scale,yaw,angles,Vec2::new(initial_pitch,initial_roll),faces.y,
                        checkpoint,&patch,hf,&mut queries,&mut tested,&mut face_pairs,refinements,witnesses.len())?
                        .ok_or(stopped.unwrap_or(Rejected::SearchBudget));
                }
                let previous_y=round_up(support.required_origin_y as f64+search_lift as f64);
                let previous_lift=search_lift;search_lift=search_lift.max(allowance);
                let tightened=search_lift>previous_lift;
                let inverse=offset.inverse();let before=witnesses.len();
                for &point in &faces.contacts {
                    let source=inverse.transform_point3(point);
                    if source.is_finite()&&witnessed.insert(source.to_array().map(f32::to_bits)) {
                        if witnesses.len()>=MAX_FACE_CONTACTS{return Err(Rejected::ContactBudget);}
                        witnesses.push(source);
                    }
                }
                if witnesses.len()==before&&!tightened {
                    return self.authoring_after_failure(at,scale,yaw,angles,Vec2::new(initial_pitch,initial_roll),faces.y,
                        checkpoint,&patch,hf,&mut queries,&mut tested,&mut face_pairs,refinements,witnesses.len())?
                        .ok_or(Rejected::UnsupportedAfterClearance{range_m:certificate.residual_range});
                }
                refinements+=1;
                if witnesses.len()!=before {
                    (rotation,support,vertex_height)=query(angles,&witnesses).ok_or(Rejected::NoSupport)?;
                    queries+=1;tested+=support.stats.tested_points;
                    if queries==CHECKPOINT_QUERY {checkpoint=Some(SearchCheckpoint {angles,vertex_height});}
                }
                let after_y=round_up(support.required_origin_y as f64+search_lift as f64).max(faces.y);
                let height_change=(after_y as f64-previous_y as f64).max(0.) as f32;
                #[cfg(test)] let previous_step=step;
                step=refinement_step(step,height_change,self.radius*scale);
                #[cfg(test)] eprintln!("{}",serde_json::json!({"static_support_diagnostic":"refinement_step",
                    "at":at.to_array(),"support_height_change_m":height_change,"source_radius_m":self.radius*scale,
                    "step_before_rad":previous_step,"step_after_rad":step,"queries":queries}));
                continue;
            }
            (faces.y,faces.contacts.len(),faces.y-vertex_height-allowance)
        };
        let model=Mat4::from_scale_rotation_translation(Vec3::splat(scale),rotation,Vec3::new(at.x,y,at.y));
        return Ok(Admitted{model,terrain_residual_range_m:certificate.residual_range,numerical_allowance_m:allowance,
            contact_count,support_queries:queries,descent_queries:queries,authoring_queries:0,
            checkpoint_authoring_queries:0,checkpoint_face_certificates:0,
            tested_points:tested,tested_face_pairs:face_pairs,face_height_correction_m,
            face_refinements:refinements,added_face_witnesses:witnesses.len(),authoring_neighbor:None});
        }
    }

    /// Try the final neighborhood first. Only on failure, re-certify the retained earlier
    /// pose and its neighborhood. This never reruns descent, translates X/Z or changes gates.
    /// The extra certificate is for an already queried pose and shares the same face budget.
    fn authoring_after_failure(&self,at:Vec2,scale:f32,yaw:f32,angles:Vec2,initial:Vec2,rejected_y:f32,
        checkpoint:Option<SearchCheckpoint>,patch:&Patch,hf:&Heightfield,queries:&mut usize,
        tested:&mut usize,face_pairs:&mut usize,refinements:usize,witnesses:usize)->Result<Option<Admitted>,Rejected> {
        let descent_queries=*queries;
        let result=self.authoring_neighbor(at,scale,yaw,angles,initial,rejected_y,patch,hf,
            queries,tested,face_pairs,refinements,witnesses)?;
        if result.is_some() {return Ok(result);}
        let Some(saved)=checkpoint else {return Ok(None);};
        if saved.angles.to_array().map(f32::to_bits)==angles.to_array().map(f32::to_bits)
            ||*face_pairs>=MAX_FACE_PAIRS||*queries>=MAX_QUERIES {return Ok(None);}
        let rotation=Quat::from_euler(glam::EulerRot::YXZ,yaw,saved.angles.x,saved.angles.y);
        let offset=Mat4::from_scale_rotation_translation(Vec3::splat(scale),rotation,Vec3::ZERO);
        let (lo,hi)=self.bvh.bounds();let mut min=Vec2::splat(f32::INFINITY);let mut max=-min;
        for i in 0..8 {
            let p=offset.transform_point3(Vec3::new(if i&1==0{lo.x}else{hi.x},if i&2==0{lo.y}else{hi.y},if i&4==0{lo.z}else{hi.z}));
            let v=Vec2::new(p.x,p.z)+at;min=min.min(v);max=max.max(v);
        }
        let pad=0.0001+8.*f32::EPSILON*(at.abs().max_element()+self.radius*scale);
        min-=Vec2::splat(pad);max+=Vec2::splat(pad);
        if !hf.openings.meeting(min,max).is_empty(){return Ok(None);}
        let certificate=patch.certificate(min,max)?;
        let allowance=0.00005+2.*patch.height_roundoff+8.*f32::EPSILON*(2.*self.radius*scale
            +(at.abs().max_element()+self.radius*scale)*certificate.max_grade);
        let faces=match patch.source_faces(&self.points,&self.triangles,offset,at,saved.vertex_height,
            allowance,MAX_FACE_PAIRS.saturating_sub(*face_pairs)) {
            Ok(f)=>f,
            Err(Rejected::FaceBudget)=>{*face_pairs=MAX_FACE_PAIRS;return Ok(None);}
            Err(e)=>return Err(e),
        };
        *face_pairs+=faces.pairs;
        let before_checkpoint=*queries;
        let mut result=self.authoring_neighbor(at,scale,yaw,saved.angles,initial,faces.y,patch,hf,
            queries,tested,face_pairs,refinements,witnesses)?;
        if let Some(ref mut admitted)=result {
            admitted.descent_queries=descent_queries;
            admitted.authoring_queries=*queries-descent_queries;
            admitted.checkpoint_authoring_queries=*queries-before_checkpoint;
            admitted.checkpoint_face_certificates=1;
            admitted.authoring_neighbor.as_mut().unwrap().reference_descent_query=CHECKPOINT_QUERY;
        }
        Ok(result)
    }

    /// Initial authoring admission may choose a slightly higher but well-supported pose.
    /// This is deliberately separate from monotone gravity proposals. Eight extra authoring queries
    /// and the SAME cumulative face budget bound the work; no source point moves >BAND from
    /// the rejected face-fitted pose. X/Z, scale, yaw and ownership remain unchanged.
    fn authoring_neighbor(&self,at:Vec2,scale:f32,yaw:f32,angles:Vec2,initial:Vec2,rejected_y:f32,
        patch:&Patch,hf:&Heightfield,queries:&mut usize,tested:&mut usize,face_pairs:&mut usize,
        refinements:usize,witnesses:usize)->Result<Option<Admitted>,Rejected> {
        let step=authoring_neighbor_step(self.radius*scale);
        let descent_queries=*queries;
        let rejected_rotation=Quat::from_euler(glam::EulerRot::YXZ,yaw,angles.x,angles.y);
        let rejected_offset=Mat4::from_scale_rotation_translation(Vec3::splat(scale),rejected_rotation,Vec3::ZERO);
        let reference:Vec<_>=self.points.iter().map(|&p|rejected_offset.transform_point3(p)).collect();
        let reference_energy=rejected_y as f64+(rejected_rotation*(self.estimated_com*scale)).y as f64;
        let mut best:Option<Admitted>=None;let mut lowest=f64::INFINITY;
        for delta in [Vec2::new(-1.,-1.),Vec2::new(-1.,0.),Vec2::new(-1.,1.),Vec2::new(0.,-1.),
            Vec2::new(0.,1.),Vec2::new(1.,-1.),Vec2::new(1.,0.),Vec2::new(1.,1.)].map(|d|d*step) {
            if *queries>=MAX_QUERIES {break;}
            let candidate=angles+delta;
            if (candidate-initial).abs().max_element()>MAX_CORRECTION {continue;}
            *queries+=1;
            let rotation=Quat::from_euler(glam::EulerRot::YXZ,yaw,candidate.x,candidate.y);
            let offset=Mat4::from_scale_rotation_translation(Vec3::splat(scale),rotation,Vec3::ZERO);
            let (lo,hi)=self.bvh.bounds();let mut min=Vec2::splat(f32::INFINITY);let mut max=-min;
            for i in 0..8 {
                let p=offset.transform_point3(Vec3::new(if i&1==0{lo.x}else{hi.x},if i&2==0{lo.y}else{hi.y},if i&4==0{lo.z}else{hi.z}));
                let v=Vec2::new(p.x,p.z)+at;min=min.min(v);max=max.max(v);
            }
            let pad=0.0001+8.*f32::EPSILON*(at.abs().max_element()+self.radius*scale);
            min-=Vec2::splat(pad);max+=Vec2::splat(pad);
            if !hf.openings.meeting(min,max).is_empty(){continue;}
            let certificate=patch.certificate(min,max)?;
            let allowance=0.00005+2.*patch.height_roundoff+8.*f32::EPSILON*(2.*self.radius*scale
                +(at.abs().max_element()+self.radius*scale)*certificate.max_grade);
            let posed:Vec<_>=self.points.iter().map(|&p|offset.transform_point3(p)).collect();
            *tested+=posed.len();
            // Rigorous lower seed; complete retained faces supply the actual support height.
            let minimum_ground=patch.values.iter().copied().fold(f32::INFINITY,f32::min) as f64;
            let maximum_source=posed.iter().map(|p|p.y).fold(f32::NEG_INFINITY,f32::max) as f64;
            let lower_seed=-round_up(-(minimum_ground-maximum_source));
            let faces=match patch.source_faces(&self.points,&self.triangles,offset,at,lower_seed,
                allowance,MAX_FACE_PAIRS.saturating_sub(*face_pairs)) {
                Ok(f)=>f,
                Err(Rejected::FaceBudget)=>{*face_pairs=MAX_FACE_PAIRS;break;}
                Err(e)=>return Err(e),
            };
            *face_pairs+=faces.pairs;
            let com=rotation*(self.estimated_com*scale);
            if !stable_points(&faces.contacts,com){continue;}
            // Compare local offsets + the represented origin-height difference in f64,
            // avoiding cancellation at distant world coordinates. Affine face displacement
            // is bounded by its vertices, so every retained source face inherits this limit.
            let dy=glam::DVec3::Y*(faces.y as f64-rejected_y as f64);
            let movement=posed.iter().zip(&reference).map(|(p,r)|(p.as_dvec3()-r.as_dvec3()+dy).length())
                .fold(0.0_f64,f64::max);
            if movement>BAND as f64 {continue;}
            let energy=faces.y as f64+com.y as f64;
            if energy>=lowest {continue;}
            let vertex_height=patch.posed_vertex_height(&posed,at)?;
            lowest=energy;
            best=Some(Admitted {
                model:Mat4::from_scale_rotation_translation(Vec3::splat(scale),rotation,Vec3::new(at.x,faces.y,at.y)),
                terrain_residual_range_m:certificate.residual_range,numerical_allowance_m:allowance,
                contact_count:faces.contacts.len(),support_queries:0,descent_queries,authoring_queries:0,
                checkpoint_authoring_queries:0,checkpoint_face_certificates:0,
                tested_points:0,tested_face_pairs:0,
                face_height_correction_m:(faces.y as f64-vertex_height-allowance as f64) as f32,
                face_refinements:refinements,added_face_witnesses:witnesses,
                authoring_neighbor:Some(AuthoringNeighbor {angle_delta:delta,
                    maximum_source_displacement_m:round_up(movement),com_height_change_m:energy-reference_energy,
                    reference_descent_query:descent_queries}),
            });
        }
        if let Some(ref mut admitted)=best {
            admitted.support_queries=*queries;admitted.tested_points=*tested;admitted.tested_face_pairs=*face_pairs;
            admitted.authoring_queries=*queries-descent_queries;
        }
        Ok(best)
    }
}

fn authoring_neighbor_step(radius:f32)->f32 {
    // Diagonal Euler change is at most 2*step. Radius-derived chord <=BAND;
    // the caller additionally checks actual TOTAL movement after fitting height.
    if radius<=0. {0.}else{(BAND/(2.*radius)).min(1.).asin()}
}

fn refinement_step(previous:f32,height_change:f32,radius:f32)->f32 {
    // Keep the converged angular resolution when the already sampled source could cover the
    // newly measured height correction within that step. A genuinely new high face witness
    // may enlarge the proposal by its radius-derived chord angle, at most the existing initial
    // 0.05 rad step. Every proposal still passes energy, authored-angle and final face checks.
    if height_change<=0.||radius<=0. {return previous;}
    let chord=2.*radius*(previous*0.5).sin();
    if height_change<=chord {return previous;}
    let angle=2.*(height_change/(2.*radius)).clamp(0.,1.).asin();
    previous.max(angle.min(0.05))
}

fn stable(s:&SupportQuery,com:Vec3,lift:f32)->bool {
    stable_points(&contacts_after_lift(s,lift),com)
}
fn contacts_after_lift(s:&SupportQuery,lift:f32)->Vec<Vec3> {
    // Same directed-rounding policy as Patch::source_faces; adding the allowance cannot
    // quietly retain a point whose final separation exceeds the physical 5 mm band.
    let y=round_up(s.required_origin_y as f64+lift as f64);
    s.contacts.iter().filter(|c|y as f64-c.required_origin_y as f64<=BAND as f64).map(|c|c.offset).collect()
}
fn gravity_angle_direction(s:&SupportQuery,com:Vec3,yaw:f32,pitch:f32,lift:f32)->Option<Vec2> {
    let contacts=contacts_after_lift(s,lift);
    let polygon=crate::vehicle_ground::support_polygon(&contacts);
    let pivot=crate::vehicle_ground::support_pivot(&polygon,com)?;
    let lever=com-pivot;let yaw_rotation=Quat::from_rotation_y(yaw);
    // Euler YXZ's actual world axes; derivative of COM height about each pivot axis is
    // (axis cross lever).y. Negating it supplies a downhill proposal, not a scripted pose.
    let axes=[yaw_rotation*Vec3::X,yaw_rotation*Quat::from_rotation_x(pitch)*Vec3::Z];
    let direction=Vec2::new(axes[0].x*lever.z-axes[0].z*lever.x,axes[1].x*lever.z-axes[1].z*lever.x);
    (direction.is_finite()&&direction.length_squared()>1e-16).then(||direction.normalize())
}
#[cfg(test)]
fn contact_margin(points:&[Vec3],com:Vec3)->Option<f32> {
    let p=crate::vehicle_ground::support_polygon(points);
    if p.len()<3{return None;}
    p.iter().zip(p.iter().cycle().skip(1)).map(|(&a,&b)| {
        let edge=Vec2::new(b.x-a.x,b.z-a.z);let d=Vec2::new(com.x-a.x,com.z-a.z);
        (edge.x*d.y-edge.y*d.x)/edge.length()
    }).min_by(f32::total_cmp)
}
fn stable_points(points:&[Vec3],com:Vec3)->bool {
    let p=crate::vehicle_ground::support_polygon(&points);
    p.len()>=3&&p.iter().zip(p.iter().cycle().skip(1)).all(|(&a,&b)| {
        let edge=Vec2::new(b.x-a.x,b.z-a.z);let d=Vec2::new(com.x-a.x,com.z-a.z);
        edge.x*d.y-edge.y*d.x>MARGIN*edge.length()
    })
}

struct Patch {step:f64,half:f64,first:glam::IVec2,width:usize,height:usize,values:Vec<f32>,max_height:f32,height_roundoff:f32}
struct Certificate {residual_range:f32,max_grade:f32}
struct FaceSupport {y:f32,contacts:Vec<Vec3>,pairs:usize,
    #[cfg(test)] gaps_m:Vec<f32>,
    #[cfg(test)] unrounded_required_y:f64,
}
impl Patch {
    fn new(hf:&Heightfield,at:Vec2,radius:f32,node:&impl Fn(u32,u32)->f32)->Result<Self,Rejected> {
        let (step,half,res)=hf.detail.as_ref().map_or((hf.spacing as f64,(hf.res as f64-1.)*hf.spacing as f64*0.5,hf.res),
            |d|(d.lattice as f64,d.half as f64,d.lattice_res));
        if !step.is_finite()||step<=0.||res<2{return Err(Rejected::Invalid);}
        let pad=0.001+8.*f32::EPSILON*(at.abs().max_element()+radius);
        let r=radius+pad;
        let first=glam::IVec2::new(((at.x as f64-r as f64+half)/step).floor() as i32,((at.y as f64-r as f64+half)/step).floor() as i32);
        let last=glam::IVec2::new(((at.x as f64+r as f64+half)/step).floor() as i32+1,((at.y as f64+r as f64+half)/step).floor() as i32+1);
        if first.min_element()<0||last.max_element()>=res as i32{return Err(Rejected::Extent);}
        let width=(last.x-first.x+1) as usize;let height=(last.y-first.y+1) as usize;
        if width.checked_mul(height).is_none_or(|n|n>MAX_PATCH_NODES){return Err(Rejected::PatchBudget);}
        let mut values=Vec::with_capacity(width*height);let mut min_height=f32::INFINITY;let mut max_height=f32::NEG_INFINITY;
        for z in first.y..=last.y {for x in first.x..=last.x {
            let h=node(x as u32,z as u32);if !h.is_finite(){return Err(Rejected::Invalid);}
            min_height=min_height.min(h);max_height=max_height.max(h);values.push(h);
        }}
        let height_roundoff=8.*f32::EPSILON*(3.*min_height.abs().max(max_height.abs())+1.);
        Ok(Self{step,half,first,width,height,values,max_height,height_roundoff})
    }
    fn h(&self,x:i32,z:i32)->f64 {self.values[(z-self.first.y) as usize*self.width+(x-self.first.x) as usize] as f64}
    /// Vertex-only comparison for the authoring receipt, from the already posed vertices and
    /// the same cached terrain triangles as source_faces. No second BVH/support query or
    /// per-point detail lock is hidden in the 96-descent plus two 8-authoring-pose budgets.
    fn posed_vertex_height(&self,posed:&[Vec3],at:Vec2)->Result<f64,Rejected> {
        let mut best=f64::NEG_INFINITY;
        for p in posed {
            let gx=(at.x as f64+p.x as f64+self.half)/self.step;
            let gz=(at.y as f64+p.z as f64+self.half)/self.step;
            let x=gx.floor() as i32;let z=gz.floor() as i32;
            if x<self.first.x||z<self.first.y||x+1>=self.first.x+self.width as i32||z+1>=self.first.y+self.height as i32 {
                return Err(Rejected::Extent);
            }
            let tx=gx-x as f64;let tz=gz-z as f64;
            let (a,b,c,d)=(self.h(x,z),self.h(x+1,z),self.h(x,z+1),self.h(x+1,z+1));
            let height=if tx+tz<=1. {a+(b-a)*tx+(c-a)*tz}else{d+(c-d)*(1.-tx)+(b-d)*(1.-tz)};
            best=best.max(height-p.y as f64);
        }
        if best.is_finite(){Ok(best)}else{Err(Rejected::NoSupport)}
    }
    /// Final support from the complete retained source. High faces are culled only when their
    /// lowest point is above the maximum actual terrain node by more than the contact band.
    /// Each remaining source face is clipped by every overlapping terrain triangle's vertical
    /// prism. Both heights are affine there, so their maximum difference occurs at a clipped
    /// polygon vertex. Vertical source faces reduce to segments and use the same clipping.
    fn source_faces(&self,points:&[Vec3],triangles:&[[u32;3]],model:Mat4,at:Vec2,
        vertex_y:f32,allowance:f32,pair_budget:usize)->Result<FaceSupport,Rejected> {
        let translated=glam::DVec3::new(at.x as f64,0.,at.y as f64);
        let posed:Vec<_>=points.iter().map(|&p|model.transform_point3(p).as_dvec3()+translated).collect();
        let mut best=vertex_y as f64;let mut contacts:Vec<(Vec3,f64)>=Vec::new();let mut pairs=0;
        let cell=|v:f64|((v+self.half)/self.step).floor() as i32;
        for indices in triangles {
            let t=indices.map(|i|posed[i as usize]);
            let lo=t[0].min(t[1]).min(t[2]);let hi=t[0].max(t[1]).max(t[2]);
            if lo.y+best>self.max_height as f64+self.height_roundoff as f64+BAND as f64 {continue;}
            // Include cells on both sides of exact lattice edges. Numerical padding is also
            // bounded by the final physical clearance allowance; it cannot lower the source.
            let eps=1e-9;
            let first=glam::IVec2::new(cell(lo.x-eps),cell(lo.z-eps));
            let last=glam::IVec2::new(cell(hi.x+eps),cell(hi.z+eps));
            if first.x<self.first.x||first.y<self.first.y||last.x+1>=self.first.x+self.width as i32||last.y+1>=self.first.y+self.height as i32 {return Err(Rejected::Extent);}
            for z in first.y..=last.y {for x in first.x..=last.x {
                let [a,b,c,d]=[(x,z),(x+1,z),(x,z+1),(x+1,z+1)].map(|(i,j)|glam::DVec3::new(i as f64*self.step-self.half,self.h(i,j),j as f64*self.step-self.half));
                let local_max=a.y.max(b.y).max(c.y).max(d.y);
                if lo.y+best>local_max+self.height_roundoff as f64+BAND as f64 {continue;}
                for terrain in [[a,c,b],[b,c,d]] {
                    if pairs>=pair_budget {return Err(Rejected::FaceBudget);}pairs+=1;
                    let (poly,n)=clip_vertical_triangle(t,terrain);
                    if n==0 {continue;}
                    let [v0,v1,v2]=terrain;let u=v1-v0;let v=v2-v0;let det=u.x*v.z-u.z*v.x;
                    let gx=(u.y*v.z-v.y*u.z)/det;let gz=(u.x*v.y-v.x*u.y)/det;
                    for p in &poly[..n] {
                        let h=v0.y+gx*(p.x-v0.x)+gz*(p.z-v0.z);let required=h-p.y;
                        if !required.is_finite(){return Err(Rejected::Invalid);}
                        if required>best {best=required;contacts.retain(|(_,y)|best-*y+allowance as f64<=BAND as f64);}
                        if best-required+allowance as f64<=BAND as f64 {
                            if contacts.len()>=MAX_FACE_CONTACTS{return Err(Rejected::ContactBudget);}
                            contacts.push(((*p-translated).as_vec3(),required));
                        }
                    }
                }
            }}
        }
        let y=round_up(best+allowance as f64);
        contacts.retain(|(_,required)|y as f64-*required<=BAND as f64);
        Ok(FaceSupport{y,
            #[cfg(test)] unrounded_required_y:best,
            #[cfg(test)] gaps_m:contacts.iter().map(|(_,required)|(y as f64-*required) as f32).collect(),
            contacts:contacts.into_iter().map(|(p,_)|p).collect(),pairs})
    }
    /// Subtract any affine reference plane P. For each source-face point, its vertex support
    /// interpolation can undershoot terrain by at most max(H-P)-min(H-P). H-P is affine on each
    /// actual terrain triangle, so ALL extrema occur at lattice corners (including both sides
    /// of the diagonal). The enclosing full cells make this conservative at footprint edges.
    fn certificate(&self,min:Vec2,max:Vec2)->Result<Certificate,Rejected> {
        let cell=|v:f32|((v as f64+self.half)/self.step).floor() as i32;
        let first=glam::IVec2::new(cell(min.x),cell(min.y));let last=glam::IVec2::new(cell(max.x)+1,cell(max.y)+1);
        if first.x<self.first.x||first.y<self.first.y||last.x>=self.first.x+self.width as i32||last.y>=self.first.y+self.height as i32{return Err(Rejected::Extent);}
        let cx=(first.x+last.x) as f64*0.5;let cz=(first.y+last.y) as f64*0.5;
        let count=((last.x-first.x+1)*(last.y-first.y+1)) as f64;
        let mut mean=0.;for z in first.y..=last.y {for x in first.x..=last.x {mean+=self.h(x,z);}}mean/=count;
        let (mut xx,mut zz,mut xh,mut zh)=(0.,0.,0.,0.);
        for z in first.y..=last.y {for x in first.x..=last.x {
            let dx=(x as f64-cx)*self.step;let dz=(z as f64-cz)*self.step;let h=self.h(x,z)-mean;
            xx+=dx*dx;zz+=dz*dz;xh+=dx*h;zh+=dz*h;
        }}
        let gx=xh/xx;let gz=zh/zz;if !gx.is_finite()||!gz.is_finite(){return Err(Rejected::Invalid);}
        let (mut low,mut high,mut max_grade)=(f64::INFINITY,f64::NEG_INFINITY,0.0_f64);
        for z in first.y..=last.y {for x in first.x..=last.x {
            let residual=self.h(x,z)-mean-gx*(x as f64-cx)*self.step-gz*(z as f64-cz)*self.step;
            low=low.min(residual);high=high.max(residual);
            if x<last.x&&z<last.y {
                let (a,b,c,d)=(self.h(x,z),self.h(x+1,z),self.h(x,z+1),self.h(x+1,z+1));
                max_grade=max_grade.max((b-a).hypot(c-a)/self.step).max((d-c).hypot(d-b)/self.step);
            }
        }}
        // Round upward explicitly, retaining the f64 interval rather than shaving a certificate.
        Ok(Certificate{residual_range:round_up(high-low),max_grade:round_up(max_grade)})
    }
}

fn round_up(value:f64)->f32 {
    let f=value as f32;
    if (f as f64)>=value {return f;}
    if f==0. {f32::from_bits(1)} else if f>0. {f32::from_bits(f.to_bits()+1)}else{f32::from_bits(f.to_bits()-1)}
}

fn clip_vertical_triangle(source:[glam::DVec3;3],ground:[glam::DVec3;3])->([glam::DVec3;10],usize) {
    let mut poly=[glam::DVec3::ZERO;10];poly[..3].copy_from_slice(&source);let mut n=3;
    let signed=|a:glam::DVec3,b:glam::DVec3,p:glam::DVec3|(b.x-a.x)*(p.z-a.z)-(b.z-a.z)*(p.x-a.x);
    let sign=signed(ground[0],ground[1],ground[2]).signum();
    for i in 0..3 {
        if n==0{break;}
        let a=ground[i];let b=ground[(i+1)%3];let input=poly;let old_n=n;n=0;
        for j in 0..old_n {
            let p=input[j];let q=input[(j+1)%old_n];let dp=signed(a,b,p)*sign;let dq=signed(a,b,q)*sign;
            if dp>=0. {poly[n]=p;n+=1;}
            if (dp>=0.)!=(dq>=0.) {poly[n]=p+(q-p)*(dp/(dp-dq));n+=1;}
        }
    }
    (poly,n)
}

#[cfg(test)] mod tests {
    use super::*;
    fn ground()->Heightfield {Heightfield{res:129,spacing:0.25,heights:vec![0.;129*129],min_h:0.,max_h:0.,openings:Default::default(),detail:None}}
    #[test] fn tiny_authoring_neighbors_cannot_manufacture_support_on_a_narrow_peak() {
        let mesh=crate::prims::box_mesh(Vec3::new(-1.,0.,-1.),Vec3::new(1.,1.,1.),[80;3]);
        let source=SourceStaticBody::new(&mesh).unwrap();let original:Vec<_>=source.points.clone();
        let mut hf=ground();hf.heights[64*129+64]=0.25;
        let patch=Patch::new(&hf,Vec2::ZERO,source.radius,&|i,j|hf.heights[j as usize*hf.res+i as usize]).unwrap();
        let face=patch.source_faces(&source.points,&source.triangles,Mat4::IDENTITY,Vec2::ZERO,0.,0.00005,MAX_FACE_PAIRS).unwrap();
        assert!(!stable_points(&face.contacts,source.estimated_com));
        let mut queries=DESCENT_QUERIES;let mut tested=0;let mut pairs=face.pairs;
        let result=source.authoring_neighbor(Vec2::ZERO,1.,0.,Vec2::ZERO,Vec2::ZERO,face.y,&patch,&hf,
            &mut queries,&mut tested,&mut pairs,0,0).unwrap();
        assert!(result.is_none(),"a 5mm adjustment cannot turn a narrow central terrain peak into a 10mm support inset");
        assert_eq!(queries,DESCENT_QUERIES+AUTHORING_NEIGHBORS,"one neighborhood adds exactly eight probes");
        assert!(pairs<=MAX_FACE_PAIRS&&pairs>face.pairs);
        assert_eq!(source.points,original,"failed admission must retain every source point");
        // Reusing an earlier search neighborhood must not manufacture support either, and
        // its second phase must be visible in the same finite counters and face budget.
        let changed=Vec2::new(0.0005,0.);
        let offset=Mat4::from_quat(Quat::from_euler(glam::EulerRot::YXZ,0.,changed.x,changed.y));
        let changed_face=patch.source_faces(&source.points,&source.triangles,offset,Vec2::ZERO,0.,0.00005,MAX_FACE_PAIRS).unwrap();
        queries=DESCENT_QUERIES;tested=0;pairs=changed_face.pairs;
        let result=source.authoring_after_failure(Vec2::ZERO,1.,0.,changed,Vec2::ZERO,changed_face.y,
            Some(SearchCheckpoint {angles:Vec2::ZERO,vertex_height:0.}),&patch,&hf,
            &mut queries,&mut tested,&mut pairs,0,0).unwrap();
        assert!(result.is_none(),"neither neighborhood can give a narrow peak a 10mm support inset");
        assert_eq!(queries,MAX_QUERIES,"96 descent +8 final +8 saved-neighborhood probes, with no repeated descent");
        assert!(pairs<=MAX_FACE_PAIRS);
    }
    #[test] fn tiny_authoring_diagonals_are_bounded_by_actual_source_radius() {
        let mesh=crate::prims::box_mesh(Vec3::new(-4.,-0.2,-0.7),Vec3::new(3.,1.8,0.9),[80;3]);
        let source=SourceStaticBody::new(&mesh).unwrap();let step=authoring_neighbor_step(source.radius);
        let base=Quat::from_euler(glam::EulerRot::YXZ,0.7,0.1,-0.05);
        for sign in [Vec2::new(1.,1.),Vec2::new(1.,-1.),Vec2::new(-1.,1.),Vec2::new(-1.,-1.)] {
            let q=Quat::from_euler(glam::EulerRot::YXZ,0.7,0.1+sign.x*step,-0.05+sign.y*step);
            let movement=source.points.iter().map(|&p|(q*p).as_dvec3().distance((base*p).as_dvec3())).fold(0.0_f64,f64::max);
            assert!(movement>0.001&&movement<BAND as f64,"source-derived diagonal movement {movement}");
        }
        // A high-energy candidate cannot be certified by a fresh face budget after prior work.
        let hf=ground();let patch=Patch::new(&hf,Vec2::ZERO,source.radius,&|_,_|0.).unwrap();
        let result=patch.source_faces(&source.points,&source.triangles,Mat4::IDENTITY,Vec2::ZERO,0.2,0.00005,0);
        assert!(matches!(result,Err(Rejected::FaceBudget)));
    }
    #[test] fn small_face_clearance_correction_preserves_the_converged_search_step() {
        let step=0.05/128.;
        assert_eq!(refinement_step(step,0.000288,5.),step,
            "a submillimetre clearance correction must not spend the remaining budget re-shrinking 0.05 rad");
        let radius=4.;let correction=2.*radius*(step*0.5).sin()*3.;
        let changed=refinement_step(step,correction,radius);
        assert!(changed>step&&changed<0.05);
        assert!((2.*radius*(changed*0.5).sin()-correction).abs()<1e-7);
        assert_eq!(refinement_step(step,0.25,radius),0.05,"a substantial new face reopens only the existing initial step");
        assert_eq!(refinement_step(step,0.,radius),step);
    }
    #[test] fn clearance_is_reserved_in_both_stability_and_gravity_guide() {
        let points=[Vec3::new(-1.,0.,-1.),Vec3::new(1.,0.,-1.),Vec3::new(0.,0.00475,1.)];
        let support=SupportQuery {required_origin_y:0.,contacts:points.into_iter().enumerate().map(|(i,p)|
            crate::source_point_support::Contact {source_index:i,offset:p,required_origin_y:-p.y}).collect(),
            radius_upper_m:2.,stats:Default::default()};
        let com=Vec3::new(0.,0.5,0.);
        assert!(stable(&support,com,0.),"raw 5 mm band includes the third real source point");
        assert_eq!(contacts_after_lift(&support,0.0005).len(),2);
        assert!(!stable(&support,com,0.0005),"mandatory clearance leaves a real edge, not a supported triangle");
        let direction=gravity_angle_direction(&support,com,0.,0.,0.0005).unwrap();
        assert!(direction.distance(Vec2::X)<1e-6,
            "the guide must tip around the remaining z=-1 source edge, not the discarded third point: {direction:?}");
    }
    #[test] fn whole_source_admits_both_tilted_and_inverted_rest_without_burial() {
        let mesh=crate::prims::box_mesh(Vec3::new(-1.,0.,-2.),Vec3::new(1.,1.,2.),[80;3]);
        let s=SourceStaticBody::new(&mesh).unwrap();let hf=ground();
        for roll in [0.065,std::f32::consts::PI] {
            let initial=Mat4::from_rotation_z(roll);let r=s.place(initial,&hf,&|i,j|hf.heights[j as usize*hf.res+i as usize]).unwrap();
            let mut contacts=0;for v in &mesh.vertices {let p=r.model.transform_point3(Vec3::from(v.pos));assert!(p.y>=-1e-6);if p.y<=BAND{contacts+=1;}}
            assert!(contacts>=4&&r.contact_count>=4);assert!(r.support_queries<=MAX_QUERIES);
            if roll>3. {assert!((r.model.transform_vector3(Vec3::Y)).y< -0.99,"do not force an overturned source upright");}
        }
    }
    #[test] fn terrain_peak_requires_face_witness_refinement_before_a_box_can_rest() {
        let mesh=crate::prims::box_mesh(Vec3::new(-1.,0.,-2.),Vec3::new(1.,1.,2.),[80;3]);
        let s=SourceStaticBody::new(&mesh).unwrap();let mut hf=ground();hf.heights[64*129+64]=0.25;
        let node=|i,j|hf.heights[j as usize*hf.res+i as usize];
        let patch=Patch::new(&hf,Vec2::ZERO,s.radius,&node).unwrap();
        let initial=patch.source_faces(&s.points,&s.triangles,Mat4::IDENTITY,Vec2::ZERO,0.,0.00005,MAX_FACE_PAIRS).unwrap();
        assert!(!stable_points(&initial.contacts,s.estimated_com),"one central peak cannot certify the original flat pose");
        // The long bottom can tilt down onto terrain within the existing 12-degree limit.
        // That is a real new resting pose, not permission to accept the original one-point balance.
        let result=s.place(Mat4::IDENTITY,&hf,&node).unwrap();
        assert!(result.face_refinements>0&&result.added_face_witnesses>0);
        assert!(result.contact_count>=3&&result.tested_face_pairs<=MAX_FACE_PAIRS);
        assert!(result.model.transform_vector3(Vec3::Y).distance(Vec3::Y)>0.01);
    }
    #[test] fn diagonal_support_ridge_requires_coupled_gravity_direction() {
        let mut mesh=crate::prims::box_mesh(Vec3::new(-1.,0.,-1.),Vec3::new(1.,1.,1.),[80;3]);
        // A closed box with a real V-shaped underside: two opposite diagonal corners touch
        // first. The biased COM must settle onto one sloping underside face, not balance
        // on that zero-width ridge. Every isolated +/- pitch/roll move initially raises it.
        for v in &mut mesh.vertices {if v.pos[1]==0.&&v.pos[0]*v.pos[2]>0. {v.pos[1]=0.1;}}
        let source=SourceStaticBody::with_estimated_com(&mesh,Vec3::new(0.,0.5,0.1)).unwrap();
        let energy=|pitch,roll| {
            let r=Quat::from_euler(glam::EulerRot::YXZ,0.,pitch,roll);
            let low=source.points.iter().map(|&p|(r*p).y).fold(f32::INFINITY,f32::min);
            -low+(r*source.estimated_com).y
        };
        for (p,r) in [(0.05,0.),(-0.05,0.),(0.,0.05),(0.,-0.05)] {assert!(energy(p,r)>energy(0.,0.)+0.01);}
        let hf=ground();let result=source.place(Mat4::IDENTITY,&hf,&|i,j|hf.heights[j as usize*hf.res+i as usize]).unwrap();
        assert!(result.contact_count>=3);
        assert!(source.points.iter().all(|&p|result.model.transform_point3(p).y>=-1e-6));
        assert!(result.model.transform_point3(source.estimated_com).y<0.499);
    }
    #[test] fn four_terrain_peaks_between_source_vertices_support_the_actual_lower_faces() {
        let mesh=crate::prims::box_mesh(Vec3::new(-1.,0.,-2.),Vec3::new(1.,1.,2.),[80;3]);
        let s=SourceStaticBody::new(&mesh).unwrap();let mut hf=ground();
        for (i,j) in [(61,58),(67,58),(61,70),(67,70)] {hf.heights[j*129+i]=0.2;}
        let r=s.place(Mat4::IDENTITY,&hf,&|i,j|hf.heights[j as usize*hf.res+i as usize]).unwrap();
        assert!(r.tested_face_pairs>0&&r.face_height_correction_m>0.199);
        assert!(r.model.w_axis.y>=0.2&&r.model.w_axis.y<=0.205);
        // The original planar bottom spans all four peaks. No vertex sits above a peak;
        // the support witness therefore has to come from actual face/terrain intersections.
        assert!(mesh.vertices.iter().filter(|v|v.pos[1]==0.).all(|v|hf.grade_at(v.pos[0],v.pos[2])==0.));
        assert!(r.contact_count>=4);
    }
    #[test] fn a_vertical_source_face_is_clipped_without_dropping_its_low_edge() {
        use glam::DVec3 as V;
        let (p,n)=clip_vertical_triangle([V::new(0.,-1.,-2.),V::new(0.,2.,-2.),V::new(0.,-1.,2.)],
            [V::new(-1.,0.,-1.),V::new(1.,0.,-1.),V::new(0.,0.,1.)]);
        assert!(n>=2);
        assert!(p[..n].iter().all(|v|v.z>=-1.000001&&v.z<=1.000001));
        assert!(p[..n].iter().any(|v|v.y<=-0.999999));
    }
    #[test] fn affine_terrain_has_zero_residual_but_saddle_cells_do_not() {
        let hf=ground();let p=Patch::new(&hf,Vec2::ZERO,3.,&|i,j|((i as f32-64.)*0.25)*0.125+((j as f32-64.)*0.25)*0.25).unwrap();
        assert!(p.certificate(Vec2::splat(-2.),Vec2::splat(2.)).unwrap().residual_range<1e-6);
        let p=Patch::new(&hf,Vec2::ZERO,3.,&|i,j|if i==64&&j==64{0.1}else{0.}).unwrap();
        assert!(p.certificate(Vec2::splat(-2.),Vec2::splat(2.)).unwrap().residual_range>=0.1);
    }
    #[test]
    #[ignore = "diagnostic only: bounded native Canyon260 whole-face neighborhood; does not alter admission"]
    fn native_canyon_260_whole_face_neighborhood_diagnostic() {
        let manifest=crate::act_manifest::ActManifest::load(std::path::Path::new(
            "assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json")).unwrap();
        let mut hf=Heightfield {res:manifest.terrain.identity.res,spacing:manifest.terrain.identity.spacing,
            heights:manifest.terrain.heights.clone(),min_h:0.,max_h:0.,openings:Default::default(),detail:None};
        hf.detail=Some(std::sync::Arc::new(crate::terrain_relief::Detail::load(manifest.path().parent().unwrap(),
            &manifest.value()["terrain"]["detail"],crate::world::weather_wind_profile(manifest.act_id).direction).unwrap()));
        let mesh=crate::gltf_load::load_glb("assets/authoring/battlefield/pz3_wreck_burnt.glb").unwrap();
        let source=SourceStaticBody::new(&mesh).unwrap();
        let original=Mat4::from_cols_array(&[-0.9054948,0.,-0.4243574,0.,0.,1.,0.,0.,
            0.4243574,0.,-0.9054948,0.,-736.029,-15.454517,594.589,1.]);
        let (scale,authored_rotation,t)=original.to_scale_rotation_translation();
        let (yaw,initial_pitch,initial_roll)=authored_rotation.to_euler(glam::EulerRot::YXZ);
        let at=Vec2::new(t.x,t.z);
        // Exact final rejected build52 pose. The diagnostic does not run or change descent.
        let final_angles=Vec2::new(0.024820247665047646,0.004338533151894808);
        let node=|i,j|hf.detail.as_ref().unwrap().lattice_node_height(i,j);
        let patch=Patch::new(&hf,at,source.radius*scale.x,&node).unwrap();
        #[derive(serde::Serialize)]
        struct Probe {
            at:[f32;2],angles:[f32;2],face_margin_m:Option<f32>,certified:bool,
            unrounded_face_energy_m:f64,placed_face_energy_m:f64,f32_placed_energy_m:f32,
            numerical_allowance_m:f32,face_y:f32,contacts:usize,face_pairs:usize,
            minimum_source_vertex_clearance_m:f32,elapsed_ms:f64,
        }
        let probe=|at:Vec2,angles:Vec2,patch:&Patch|->Result<Probe,Rejected> {
            let clock=std::time::Instant::now();
            if (angles-Vec2::new(initial_pitch,initial_roll)).abs().max_element()>MAX_CORRECTION {
                return Err(Rejected::Invalid);
            }
            let rotation=Quat::from_euler(glam::EulerRot::YXZ,yaw,angles.x,angles.y);
            let offset=Mat4::from_scale_rotation_translation(scale,rotation,Vec3::ZERO);
            let (lo,hi)=source.bvh.bounds();let mut min=Vec2::splat(f32::INFINITY);let mut max=-min;
            for i in 0..8 {
                let p=offset.transform_point3(Vec3::new(if i&1==0{lo.x}else{hi.x},if i&2==0{lo.y}else{hi.y},if i&4==0{lo.z}else{hi.z}));
                let v=Vec2::new(p.x,p.z)+at;min=min.min(v);max=max.max(v);
            }
            let pad=0.0001+8.*f32::EPSILON*(at.abs().max_element()+source.radius*scale.x);
            min-=Vec2::splat(pad);max+=Vec2::splat(pad);
            if !hf.openings.meeting(min,max).is_empty(){return Err(Rejected::Opening);}
            let certificate=patch.certificate(min,max)?;
            let allowance=0.00005+2.*patch.height_roundoff+8.*f32::EPSILON*(2.*source.radius*scale.x
                +(at.abs().max_element()+source.radius*scale.x)*certificate.max_grade);
            // Seed with a rigorous LOWER bound. Reusing the f32 grade-at vertex maximum
            // could conceal submicrometre face-energy changes in this diagnostic.
            // Every terrain point is >= the patch's minimum node, and every posed source
            // point is <= its maximum vertex height. Full retained faces then determine best.
            let minimum_ground=patch.values.iter().copied().fold(f32::INFINITY,f32::min) as f64;
            let maximum_source=source.points.iter().map(|&p|offset.transform_point3(p).y)
                .fold(f32::NEG_INFINITY,f32::max) as f64;
            let lower_seed=-round_up(-(minimum_ground-maximum_source));
            let faces=patch.source_faces(&source.points,&source.triangles,offset,at,lower_seed,
                allowance,MAX_FACE_PAIRS)?;
            let com=rotation*(source.estimated_com*scale.x);
            let model=Mat4::from_scale_rotation_translation(scale,rotation,Vec3::new(at.x,faces.y,at.y));
            let minimum_clearance=source.points.iter().map(|&p| {
                let w=model.transform_point3(p);w.y-hf.grade_at(w.x,w.z)
            }).fold(f32::INFINITY,f32::min);
            Ok(Probe {at:at.to_array(),angles:angles.to_array(),face_margin_m:contact_margin(&faces.contacts,com),
                certified:stable_points(&faces.contacts,com),unrounded_face_energy_m:faces.unrounded_required_y+com.y as f64,
                placed_face_energy_m:faces.y as f64+com.y as f64,f32_placed_energy_m:faces.y+com.y,
                numerical_allowance_m:allowance,face_y:faces.y,contacts:faces.contacts.len(),face_pairs:faces.pairs,
                minimum_source_vertex_clearance_m:minimum_clearance,elapsed_ms:clock.elapsed().as_secs_f64()*1000.})
        };
        let baseline=probe(at,final_angles,&patch).expect("baseline face certificate must be measurable");
        assert!(baseline.minimum_source_vertex_clearance_m>=-baseline.numerical_allowance_m);
        let baseline_energy=baseline.unrounded_face_energy_m;
        let baseline_placed=baseline.placed_face_energy_m;
        let mut probes=vec![serde_json::json!({"kind":"baseline","probe":baseline})];
        let mut angles_seen=std::collections::HashSet::new();angles_seen.insert(final_angles.to_array().map(f32::to_bits));
        let mut same_xz_supported=0;let mut same_xz_downhill_supported=0;let mut tested_pairs=baseline.face_pairs;
        // 7x7 coarse neighborhood plus 3x3 last-step neighborhood; <60 orientations.
        // This is a bounded feasibility sample, not a proof of global infeasibility.
        let fine=0.00002441406286_f32;
        let mut offsets=Vec::new();
        for x in -3..=3 {for y in -3..=3 {offsets.push(Vec2::new(x as f32,y as f32)*0.0005);}}
        for x in -1..=1 {for y in -1..=1 {offsets.push(Vec2::new(x as f32,y as f32)*fine);}}
        // Linear extrapolation from build52's last two margins; its actual energy is unknown.
        offsets.push(Vec2::new(-0.000912,0.));
        for delta in offsets {
            let angles=final_angles+delta;
            if !angles_seen.insert(angles.to_array().map(f32::to_bits)){continue;}
            match probe(at,angles,&patch) {
                Ok(p)=>{
                    assert!(p.minimum_source_vertex_clearance_m>=-p.numerical_allowance_m);
                    tested_pairs+=p.face_pairs;
                    if p.certified {same_xz_supported+=1;if p.unrounded_face_energy_m<baseline_energy {same_xz_downhill_supported+=1;}}
                    probes.push(serde_json::json!({"kind":"same_xz_orientation","angle_delta_rad":delta.to_array(),
                        "raw_energy_delta_m":p.unrounded_face_energy_m-baseline_energy,
                        "placed_energy_delta_m":p.placed_face_energy_m-baseline_placed,"probe":p}));
                }
                Err(e)=>probes.push(serde_json::json!({"kind":"same_xz_orientation","angle_delta_rad":delta.to_array(),"rejected":format!("{e:?}")})),
            }
        }
        // Only if orientation sampling finds no certificate, inspect small authoring offsets.
        // These are NOT physical sliding or collision-free relocation; nearby props/lanes
        // are deliberately not certified, and no returned pose is installed into the game.
        if same_xz_supported==0 {
            for distance in [0.05,0.10] {for direction in [Vec2::X,-Vec2::X,Vec2::Y,-Vec2::Y] {
                let shifted=at+direction*distance;
                let result=Patch::new(&hf,shifted,source.radius*scale.x,&node)
                    .and_then(|p|probe(shifted,final_angles,&p));
                match result {
                    Ok(p)=>{tested_pairs+=p.face_pairs;probes.push(serde_json::json!({"kind":"hypothetical_authoring_offset",
                        "requested_xz_delta_m":(direction*distance).to_array(),"actual_xz_delta_m":(shifted-at).to_array(),
                        "raw_energy_delta_m":p.unrounded_face_energy_m-baseline_energy,"probe":p}));}
                    Err(e)=>probes.push(serde_json::json!({"kind":"hypothetical_authoring_offset",
                        "requested_xz_delta_m":(direction*distance).to_array(),"rejected":format!("{e:?}")})),
                }
            }}
        }
        assert!(probes.len()<=66,"diagnostic remains independently bounded");
        eprintln!("{}",serde_json::json!({"static_support_diagnostic":"native_canyon260_neighborhood",
            "source_index":260,"source_vertices_retained":source.points.len(),"source_triangles_retained":source.triangles.len(),
            "scope":"test-only sampled feasibility; no runtime placement/cap/margin change; full original faces and exact loaded detail lattice",
            "com_model":"unchanged uniform source surface-area thin-shell estimate",
            "energy_model":"f64 sum of full-face support height and unchanged f32 posed COM; rendered placement additionally rounded upward",
            "contact_band_m":BAND,"required_margin_m":MARGIN,"max_face_pairs_per_probe":MAX_FACE_PAIRS,
            "same_xz_supported":same_xz_supported,"same_xz_downhill_supported":same_xz_downhill_supported,
            "total_face_pairs":tested_pairs,"probes":probes}));
    }
    #[test]
    #[ignore = "private Canyon sources: preserve previously certified build52/build54 placements"]
    fn native_canyon_preserves_known_safe_descent_and_authoring_placements() {
        let manifest=crate::act_manifest::ActManifest::load(std::path::Path::new(
            "assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json")).unwrap();
        let mut hf=Heightfield {res:manifest.terrain.identity.res,spacing:manifest.terrain.identity.spacing,
            heights:manifest.terrain.heights.clone(),min_h:0.,max_h:0.,openings:Default::default(),detail:None};
        hf.detail=Some(std::sync::Arc::new(crate::terrain_relief::Detail::load(manifest.path().parent().unwrap(),
            &manifest.value()["terrain"]["detail"],crate::world::weather_wind_profile(manifest.act_id).direction).unwrap()));
        // Exact original transforms from build52/54 runtime receipts, never installed outcomes.
        let cases=[
            (11962,"assets/authoring/battlefield/kv1_wreck_burnt.glb",true,
                [-0.6142114400863647,0.002965688705444336,-0.7891358733177185,0.,
                 0.025782307609915733,0.9995344877243042,-0.016310855746269226,0.,
                 0.7887201905250549,-0.030364058911800385,-0.614001989364624,0.,
                 319.47900390625,-14.349040031433105,-181.80299377441406,1.]),
            (12521,"assets/authoring/battlefield/lorry_wreck_burnt.glb",true,
                [-0.8237254619598389,0.03873605281114578,-0.5656648278236389,0.,
                 0.028866229578852654,0.9992348551750183,0.02639121189713478,0.,
                 0.5662543177604675,0.005410495214164257,-0.8242133855819702,0.,
                 362.5820007324219,-13.83899974822998,232.1719970703125,1.]),
            (1248,"assets/authoring/battlefield/kv1_wreck_burnt.glb",false,
                [-0.9397522211074829,0.,-0.3418562114238739,0.,0.,1.,0.,0.,
                 0.3418562114238739,0.,-0.9397522211074829,0.,-361.2030029296875,-19.09602165222168,-398.1889953613281,1.]),
            (11689,"assets/authoring/battlefield/pz3_wreck_burnt.glb",false,
                [-0.08184266090393066,0.012034699320793152,0.9965726733207703,0.,
                 -0.03484364226460457,0.9992812871932983,-0.01492890901863575,0.,
                 -0.9960361123085022,-0.035946041345596313,-0.08136451244354248,0.,
                 -764.8980102539062,-16.540180206298828,500.5790100097656,1.]),
        ];
        let mut failures=Vec::new();
        for (index,path,previous_descent_only,columns) in cases {
            let mesh=crate::gltf_load::load_glb(path).unwrap();let source=SourceStaticBody::new(&mesh).unwrap();
            let original=Mat4::from_cols_array(&columns);let clock=std::time::Instant::now();
            let result=source.place(original,&hf,&|i,j|hf.detail.as_ref().unwrap().lattice_node_height(i,j));
            let elapsed_ms=clock.elapsed().as_secs_f64()*1000.;
            match result {
                Ok(r)=>{
                    assert_eq!(r.model.w_axis.x.to_bits(),original.w_axis.x.to_bits());
                    assert_eq!(r.model.w_axis.z.to_bits(),original.w_axis.z.to_bits());
                    assert_eq!(source.points.len(),mesh.vertices.len());
                    assert!(r.descent_queries<=DESCENT_QUERIES&&r.authoring_queries<=MAX_AUTHORING_QUERIES);
                    assert_eq!(r.support_queries,r.descent_queries+r.authoring_queries);
                    assert!(r.support_queries<=MAX_QUERIES&&r.tested_face_pairs<=MAX_FACE_PAIRS&&r.contact_count>=3);
                    if previous_descent_only {
                        assert!(r.authoring_neighbor.is_none(),"source{index}: restoring the original search must preserve its supported descent");
                        assert_eq!(r.descent_queries,96,"the original certified pose used all96 descent proposals");
                    }
                    let min_clearance=source.points.iter().map(|&p| {let w=r.model.transform_point3(p);w.y-hf.grade_at(w.x,w.z)})
                        .fold(f32::INFINITY,f32::min);
                    assert!(min_clearance>=-r.numerical_allowance_m,"source{index}: source clearance{min_clearance}");
                    eprintln!("{}",serde_json::json!({"source_index":index,"status":"admitted","placement_ms":elapsed_ms,
                        "descent_queries":r.descent_queries,"authoring_queries":r.authoring_queries,"total_queries":r.support_queries,
                        "checkpoint_authoring_queries":r.checkpoint_authoring_queries,"checkpoint_face_certificates":r.checkpoint_face_certificates,
                        "authoring_reference_descent_query":r.authoring_neighbor.as_ref().map(|n|n.reference_descent_query),
                        "face_pairs":r.tested_face_pairs,"contacts":r.contact_count,"min_vertex_clearance_m":min_clearance,
                        "source_vertices_retained":source.points.len(),"model":r.model.to_cols_array()}));
                }
                Err(e)=>{failures.push(index);eprintln!("{}",serde_json::json!({"source_index":index,
                    "status":"previous_safe_placement_regressed","reason":format!("{e:?}"),"placement_ms":elapsed_ms}));}
            }
        }
        assert!(failures.is_empty(),"previously certified placements must remain admitted: {failures:?}");
    }
    #[test]
    #[ignore = "actual Canyon index260/private detail source; root runs the bounded CPU regression"]
    fn native_canyon_260_refines_face_support_after_clearance() {
        let manifest=crate::act_manifest::ActManifest::load(std::path::Path::new(
            "assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json")).unwrap();
        let mut hf=Heightfield {res:manifest.terrain.identity.res,spacing:manifest.terrain.identity.spacing,
            heights:manifest.terrain.heights.clone(),min_h:0.,max_h:0.,openings:Default::default(),detail:None};
        let detail=crate::terrain_relief::Detail::load(manifest.path().parent().unwrap(),
            &manifest.value()["terrain"]["detail"],crate::world::weather_wind_profile(manifest.act_id).direction).unwrap();
        // Queries and exact_node both read the same loaded detail lattice; no coarse redraw is needed.
        hf.detail=Some(std::sync::Arc::new(detail));
        let mesh=crate::gltf_load::load_glb("assets/authoring/battlefield/pz3_wreck_burnt.glb").unwrap();
        let source=SourceStaticBody::new(&mesh).unwrap();
        // Exact rejected instance260 from ground_canyon_b49_a.json; previously lifted against
        // hidden face contacts and returned UnsupportedAfterClearance(range=0.32617185m).
        let original=Mat4::from_cols_array(&[-0.9054948,0.,-0.4243574,0.,0.,1.,0.,0.,
            0.4243574,0.,-0.9054948,0.,-736.029,-15.454517,594.589,1.]);
        let clock=std::time::Instant::now();
        let result=source.place(original,&hf,&|i,j|hf.detail.as_ref().unwrap().lattice_node_height(i,j));
        let result=result.unwrap_or_else(|e|panic!("Canyon source260 remains unresolved: {e:?}"));
        assert_eq!(source.source_vertex_count(),mesh.vertices.len());
        assert!(result.contact_count>=3&&result.support_queries<=MAX_QUERIES&&result.tested_face_pairs<=MAX_FACE_PAIRS);
        assert!(result.face_refinements>0,"the former clearance-only rejection must feed actual face support back into descent");
        assert!(mesh.vertices.iter().all(|v| {let p=result.model.transform_point3(Vec3::from(v.pos));
            p.y-hf.grade_at(p.x,p.z)>=-result.numerical_allowance_m}));
        eprintln!("{}",serde_json::json!({"source_index":260,"status":"admitted","placement_ms":clock.elapsed().as_secs_f64()*1000.,
            "support_queries":result.support_queries,"face_pairs":result.tested_face_pairs,
            "face_refinements":result.face_refinements,"added_face_witnesses":result.added_face_witnesses,
            "contacts":result.contact_count,"model":result.model.to_cols_array()}));
    }
    #[test]
    #[ignore = "private prepared GLBs; root runs bounded native CPU admission explicitly"]
    fn native_prepared_wrecks_preserve_all_source_points_on_flat_and_mild_grade() {
        for path in ["assets/authoring/battlefield/kv1_wreck_burnt.glb",
            "assets/authoring/battlefield/pz3_wreck_burnt.glb"] {
            let mesh=crate::gltf_load::load_glb(path).unwrap();
            let start=std::time::Instant::now();let source=SourceStaticBody::new(&mesh).unwrap();
            let cache_ms=start.elapsed().as_secs_f64()*1000.;
            assert_eq!(source.source_vertex_count(),mesh.vertices.len());
            for slope in [0.,0.025] {
                let mut hf=ground();
                for j in 0..hf.res {for i in 0..hf.res {
                    hf.heights[j*hf.res+i]=(i as f32-64.)*hf.spacing*slope;
                }}
                let start=std::time::Instant::now();
                let result=source.place(Mat4::IDENTITY,&hf,&|i,j|hf.heights[j as usize*hf.res+i as usize]);
                let elapsed_ms=start.elapsed().as_secs_f64()*1000.;
                match result {
                    Ok(r)=>{
                        let min_clearance=mesh.vertices.iter().map(|v| {
                            let p=r.model.transform_point3(Vec3::from(v.pos));p.y-hf.grade_at(p.x,p.z)
                        }).fold(f32::INFINITY,f32::min);
                        eprintln!("{}",serde_json::json!({"source":path,"source_vertices":mesh.vertices.len(),
                            "source_triangles":mesh.indices.len()/3,"slope":slope,"cache_ms":cache_ms,
                            "placement_ms":elapsed_ms,"contacts":r.contact_count,"point_queries":r.support_queries,
                            "tested_points":r.tested_points,"face_pairs":r.tested_face_pairs,
                            "min_source_vertex_clearance_m":min_clearance,"model":r.model.to_cols_array()}));
                        assert!(min_clearance>=-0.00001,"{path}: a source point penetrates the drawn ground");
                        assert!(r.contact_count>=3&&r.support_queries<=MAX_QUERIES);
                    }
                    Err(reason)=>panic!("native source {path}, slope {slope}: {reason:?}; cache {cache_ms:.2}ms, attempt {elapsed_ms:.2}ms"),
                }
            }
        }
    }
    #[test]
    #[ignore = "private overturned sledge; root runs explicit bounded-rejection proof"]
    fn native_overturned_sledge_rejects_a_rest_beyond_authored_correction_limit() {
        let path="assets/authoring/battlefield/panje_sledge_overturned_wreck_detail.glb";
        let mesh=crate::gltf_load::load_glb(path).unwrap();
        let before:Vec<_>=mesh.vertices.iter().map(|v|v.pos).collect();
        let source=SourceStaticBody::new(&mesh).unwrap();let hf=ground();
        let start=std::time::Instant::now();let authored=Mat4::IDENTITY;
        let result=source.place(authored,&hf,&|i,j|hf.heights[j as usize*hf.res+i as usize]);
        let reason=result.err().expect("this source requires a correction beyond 12 degrees; do not force a pose");
        assert!(matches!(reason,Rejected::Unstable|Rejected::SearchBudget),"unexpected rejection: {reason:?}");
        assert_eq!(source.source_vertex_count(),before.len());
        assert!(mesh.vertices.iter().zip(before).all(|(v,p)|v.pos==p));
        eprintln!("{}",serde_json::json!({"source":path,"status":"unresolved_original_pose_retained",
            "reason":format!("{reason:?}"),"authored_model":authored.to_cols_array(),
            "maximum_correction_degrees":MAX_CORRECTION.to_degrees(),
            "placement_ms":start.elapsed().as_secs_f64()*1000.}));
    }
}
