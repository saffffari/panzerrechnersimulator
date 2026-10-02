// Portable exact-source triangle visibility. Source positions/indices/UVs are not simplified.
// Two read-only storage bindings; all offsets/counts are u32 WORDS. Both BVH levels are threaded
// preorder trees: escape links strictly advance, so traversal cannot exhaust a shader stack.
// Status 0 clear, 1 opaque/covered alpha, 2 dielectric, 3 unresolved material, 4 invalid payload/ray.
// A caller MUST inspect status; unresolved/invalid is not a physical opaque or clear result.
@group(SV_GROUP) @binding(SV_GEOMETRY_BINDING) var<storage,read> sv_geometry: array<u32>;
@group(SV_GROUP) @binding(SV_INSTANCE_BINDING) var<storage,read> sv_instances: array<u32>;

const SV_CLEAR=0u;
const SV_BLOCKED=1u;
const SV_DIELECTRIC=2u;
const SV_UNRESOLVED=3u;
const SV_INVALID=4u;
const SV_NODE_WORDS=12u;
const SV_INSTANCE_WORDS=32u;

struct SceneVisibilityHit {
    status:u32, t:f32, instance_id:u32, geometry_id:u32,
    primitive_id:u32, material_class:u32, material_id:u32, front_face:u32,
    barycentrics:vec2<f32>, uv:vec2<f32>, normal:vec3<f32>, instance_record:u32, triangle_record:u32,
};
fn sv_finite3(x:vec3<f32>)->bool {return all(abs(x)<=vec3(3.402823e38));}
fn sv_range(start:u32,count:u32,length:u32)->bool {return count<=length && start<=length-count;}
// Exact u32 product-overflow check without max-u32 integer division. NVIDIA 615.71.09
// rejects the otherwise valid SPIR-V for `0xffffffffu / runtime_height` inside this leaf.
// Each 16-bit product and the three-term carry sum fit in u32; no coverage is relaxed.
fn sv_mul_overflow(a:u32,b:u32)->bool {
    let a0=a&65535u;let a1=a>>16u;let b0=b&65535u;let b1=b>>16u;
    let p00=a0*b0;let p10=a1*b0;let p01=a0*b1;
    return a1*b1!=0u || (p10>>16u)!=0u || (p01>>16u)!=0u
        || ((p00>>16u)+(p10&65535u)+(p01&65535u))>65535u;
}
fn sv_g3(base:u32)->vec3<f32> {return vec3(bitcast<f32>(sv_geometry[base]),bitcast<f32>(sv_geometry[base+1u]),bitcast<f32>(sv_geometry[base+2u]));}
fn sv_i3(base:u32)->vec3<f32> {return vec3(bitcast<f32>(sv_instances[base]),bitcast<f32>(sv_instances[base+1u]),bitcast<f32>(sv_instances[base+2u]));}
fn sv_invalid()->SceneVisibilityHit {var h:SceneVisibilityHit;h.status=SV_INVALID;return h;}

// Matches solid-grass source vertices: the affine model excludes the packed wind row.
fn sv_bend_point(p:vec3<f32>,tip:vec3<f32>)->vec3<f32> {
    let u=clamp(p.y/0.48,0.0,1.0);return p+tip*u*u;
}
struct SceneBentBounds {lo:vec3<f32>,hi:vec3<f32>};
fn sv_bend_bounds(lo:vec3<f32>,hi:vec3<f32>,tip:vec3<f32>)->SceneBentBounds {
    var result:SceneBentBounds;result.lo=lo;result.hi=hi;
    if(all(tip==vec3(0.0))){return result;}
    let low=clamp(lo.y/0.48,0.0,1.0);let high=clamp(hi.y/0.48,0.0,1.0);
    let a=tip*low*low;let b=tip*high*high;
    // Includes multiply/add rounding; no geometric simplification or inflated grass thickness.
    let error=(max(abs(lo),abs(hi))+max(abs(a),abs(b)))*0.00000095367431640625;
    result.lo=lo+min(a,b)-error;result.hi=hi+max(a,b)+error;return result;
}

fn sv_box(origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32,
          lo:vec3<f32>,hi:vec3<f32>)->bool {
    var near=lower;var far=upper;
    for(var axis=0u;axis<3u;axis+=1u){
        if(direction[axis]==0.0){if(origin[axis]<lo[axis] || origin[axis]>hi[axis]){return false;}}
        else{let a=(lo[axis]-origin[axis])/direction[axis];let b=(hi[axis]-origin[axis])/direction[axis];
            near=max(near,min(a,b));far=min(far,max(a,b));if(near>far){return false;}}
    }return true;
}

// Canonical endpoint order makes a shared projected edge exactly antisymmetric, even when the
// driver contracts multiply/subtract to FMA. Without it, both adjacent triangles can reject the
// same internal diagonal by opposite rounding errors (observed on NVIDIA prism-exit rays).
fn sv_edge(a:vec2<f32>,b:vec2<f32>)->f32 {
    let swap=a.x>b.x || (a.x==b.x && a.y>b.y);
    let first=select(a,b,swap);let second=select(b,a,swap);
    let determinant=first.x*second.y-first.y*second.x;
    return select(determinant,-determinant,swap);
}

// Dominant-axis projected edge functions. Shared edges use the same operations; no determinant
// epsilon discards thin triangles. The ray parameter is unchanged by affine instance transforms.
// Return t, barycentric b/c, and 0 miss / 1 hit / -1 numerical overflow.
fn sv_triangle(origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32,
               p0:vec3<f32>,p1:vec3<f32>,p2:vec3<f32>)->vec4<f32>{
    let absolute=abs(direction);var z=2u;
    if(absolute.x>=absolute.y && absolute.x>=absolute.z){z=0u;}
    else if(absolute.y>=absolute.z){z=1u;}
    if(direction[z]==0.0){return vec4(0.0);}
    var x=(z+1u)%3u;var y=(x+1u)%3u;
    if(direction[z]<0.0){let swap=x;x=y;y=swap;}
    let s=vec3(direction[x]/direction[z],direction[y]/direction[z],1.0/direction[z]);
    let a=p0-origin;let b=p1-origin;let c=p2-origin;
    let aa=vec3(a[x]-s.x*a[z],a[y]-s.y*a[z],a[z]*s.z);
    let bb=vec3(b[x]-s.x*b[z],b[y]-s.y*b[z],b[z]*s.z);
    let cc=vec3(c[x]-s.x*c[z],c[y]-s.y*c[z],c[z]*s.z);
    let e=vec3(sv_edge(bb.xy,cc.xy),sv_edge(cc.xy,aa.xy),sv_edge(aa.xy,bb.xy));
    let determinant=e.x+e.y+e.z;
    if(!sv_finite3(e) || !(abs(determinant)<=3.402823e38)){return vec4(0.0,0.0,0.0,-1.0);}
    if((min(e.x,min(e.y,e.z))<0.0 && max(e.x,max(e.y,e.z))>0.0) || determinant==0.0){return vec4(0.0);}
    let t=(e.x*aa.z+e.y*bb.z+e.z*cc.z)/determinant;
    if(!(abs(t)<=3.402823e38)){return vec4(0.0,0.0,0.0,-1.0);}
    if(t<lower || t>upper){return vec4(0.0);}
    return vec4(t,e.y/determinant,e.z/determinant,1.0);
}

// Exact shared source edge plus identical oriented source plane. This recognizes a single
// tessellated optical face without hiding neighboring rim faces or separate nearby geometry.
fn sv_source_neighbors(a0:vec3<f32>,a1:vec3<f32>,a2:vec3<f32>,b0:vec3<f32>,b1:vec3<f32>,b2:vec3<f32>)->bool {
    let count=u32(all(a0==b0)||all(a0==b1)||all(a0==b2))
        +u32(all(a1==b0)||all(a1==b1)||all(a1==b2))+u32(all(a2==b0)||all(a2==b1)||all(a2==b2));
    if(count<2u){return false;}
    let an=cross(a1-a0,a2-a0);let bn=cross(b1-b0,b2-b0);
    let sa=max(abs(an.x),max(abs(an.y),abs(an.z)));let sb=max(abs(bn.x),max(abs(bn.y),abs(bn.z)));
    return sa>0.0 && sb>0.0 && all(an/sa==bn/sb);
}
fn sv_blas_mode_filtered(record:u32,origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32,any_opaque:bool,previous:vec4<u32>)->SceneVisibilityHit {
    let ilen=arrayLength(&sv_instances);let glen=arrayLength(&sv_geometry);
    if(!sv_range(record,SV_INSTANCE_WORDS,ilen)){return sv_invalid();}
    // Scalar source loads avoid dynamically indexed function-local arrays in NVIDIA's
    // Vulkan compiler while preserving the same exact matrix and triangle calculations.
    let inverse=mat4x4(vec4(sv_i3(record),bitcast<f32>(sv_instances[record+3u])),
        vec4(sv_i3(record+4u),bitcast<f32>(sv_instances[record+7u])),
        vec4(sv_i3(record+8u),bitcast<f32>(sv_instances[record+11u])),
        vec4(sv_i3(record+12u),bitcast<f32>(sv_instances[record+15u])));
    let tip=sv_i3(record+21u);
    if(!sv_finite3(tip)){return sv_invalid();}
    let local_origin=(inverse*vec4(origin,1.0)).xyz;
    let local_direction=(inverse*vec4(direction,0.0)).xyz;
    if(!sv_finite3(local_origin) || !sv_finite3(local_direction)){return sv_invalid();}
    let g=sv_instances[record+16u];if(!sv_range(g,16u,glen)){return sv_invalid();}
    let root=sv_geometry[g];let nodes=sv_geometry[g+1u];
    if(nodes>glen/SV_NODE_WORDS || !sv_range(root,nodes*SV_NODE_WORDS,glen)){return sv_invalid();}
    let end=root+nodes*SV_NODE_WORDS;
    var cursor=root;var steps=0u;var best:SceneVisibilityHit;best.t=upper;
    loop {
        if(cursor>=end){break;}
        if(steps>=nodes || !sv_range(cursor,SV_NODE_WORDS,end)){return sv_invalid();}steps+=1u;
        let escape=sv_geometry[cursor+3u];
        if(escape<=cursor || escape>end || (escape-root)%SV_NODE_WORDS!=0u){return sv_invalid();}
        let bounds=sv_bend_bounds(sv_g3(cursor),sv_g3(cursor+4u),tip);
        if(!sv_finite3(bounds.lo) || !sv_finite3(bounds.hi)){return sv_invalid();}
        if(!sv_box(local_origin,local_direction,lower,best.t,bounds.lo,bounds.hi)){cursor=escape;continue;}
        let count=sv_geometry[cursor+8u];
        if(count==0u){cursor+=SV_NODE_WORDS;continue;}
        let first=sv_geometry[cursor+7u];
        if(!sv_range(first,count,sv_geometry[g+5u])){return sv_invalid();}
        for(var item=0u;item<count;item+=1u){
            let tri=sv_geometry[g+4u]+(first+item)*4u;
            if(!sv_range(tri,4u,glen)){return sv_invalid();}
            if(sv_instances[record+17u]==previous.x && sv_geometry[tri+3u]==previous.y){continue;}
            let i0=sv_geometry[tri];let i1=sv_geometry[tri+1u];let i2=sv_geometry[tri+2u];
            let vertex_count=sv_geometry[g+3u];let vertex_base=sv_geometry[g+2u];
            if(any(vec3(i0,i1,i2)>=vec3(vertex_count)) || vertex_count>glen/8u
                || !sv_range(vertex_base,vertex_count*8u,glen)){return sv_invalid();}
            let a0=vertex_base+i0*8u;let a1=vertex_base+i1*8u;let a2=vertex_base+i2*8u;
            let p0=sv_bend_point(sv_g3(a0),tip);let p1=sv_bend_point(sv_g3(a1),tip);let p2=sv_bend_point(sv_g3(a2),tip);
            if(sv_instances[record+17u]==previous.x && g==previous.z){
                if(!sv_range(previous.w,4u,glen)){return sv_invalid();}
                let j=vec3(sv_geometry[previous.w],sv_geometry[previous.w+1u],sv_geometry[previous.w+2u]);
                if(any(j>=vec3(vertex_count))){return sv_invalid();}
                let q0=sv_bend_point(sv_g3(vertex_base+j.x*8u),tip);let q1=sv_bend_point(sv_g3(vertex_base+j.y*8u),tip);let q2=sv_bend_point(sv_g3(vertex_base+j.z*8u),tip);
                if(sv_source_neighbors(p0,p1,p2,q0,q1,q2)){continue;}
            }
            let hit=sv_triangle(local_origin,local_direction,lower,best.t,p0,p1,p2);
            if(hit.w<0.0){return sv_invalid();}if(hit.w==0.0){continue;}
            if(previous.x!=0xffffffffu && hit.x==0.0){continue;}
            let uv0=vec2(bitcast<f32>(sv_geometry[a0+4u]),bitcast<f32>(sv_geometry[a0+5u]));
            let uv1=vec2(bitcast<f32>(sv_geometry[a1+4u]),bitcast<f32>(sv_geometry[a1+5u]));
            let uv2=vec2(bitcast<f32>(sv_geometry[a2+4u]),bitcast<f32>(sv_geometry[a2+5u]));
            let uv=uv0*(1.0-hit.y-hit.z)+uv1*hit.y+uv2*hit.z;
            let material=sv_instances[record+18u];
            if(material==2u){
                let width=sv_geometry[g+7u];let height=sv_geometry[g+8u];
                if(width==0u || height==0u || sv_mul_overflow(width,height)){return sv_invalid();}
                let texel=min(vec2<u32>(floor(fract(uv)*vec2(f32(width),f32(height)))),vec2(width-1u,height-1u));
                let pixel=texel.y*width+texel.x;let address=sv_geometry[g+6u]+pixel/4u;
                if(address>=glen){return sv_invalid();}
                let alpha=(sv_geometry[address]>>((pixel%4u)*8u))&255u;
                if(alpha<128u){continue;}
            }
            let local_normal=cross(p1-p0,p2-p0);
            let world_normal=(transpose(inverse)*vec4(local_normal,0.0)).xyz*bitcast<f32>(sv_instances[record+20u]);
            let normal_scale=max(abs(world_normal.x),max(abs(world_normal.y),abs(world_normal.z)));
            if(!sv_finite3(world_normal) || normal_scale<=0.0){return sv_invalid();}
            best.status=SV_BLOCKED;if(material==1u){best.status=SV_DIELECTRIC;}
            if(material>2u){best.status=SV_UNRESOLVED;}
            best.t=hit.x;best.barycentrics=hit.yz;best.uv=uv;best.normal=normalize(world_normal/normal_scale);
            best.instance_id=sv_instances[record+17u];best.geometry_id=g;best.primitive_id=sv_geometry[tri+3u];
            best.material_class=material;best.material_id=sv_instances[record+19u];
            best.front_face=select(0u,1u,dot(direction,best.normal)<0.0);best.instance_record=record;best.triangle_record=tri;
            if(any_opaque && best.status==SV_BLOCKED){return best;}
        }cursor=escape;
    }return best;
}

fn sv_blas_mode(record:u32,origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32,any_opaque:bool)->SceneVisibilityHit {
    return sv_blas_mode_filtered(record,origin,direction,lower,upper,any_opaque,vec4(0xffffffffu));
}
fn sv_blas(record:u32,origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32)->SceneVisibilityHit {
    return sv_blas_mode(record,origin,direction,lower,upper,false);
}
fn scene_visibility_trace_mode_filtered(origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32,any_opaque:bool,previous:vec4<u32>)->SceneVisibilityHit {
    let length=arrayLength(&sv_instances);
    if(length<8u || sv_instances[0]!=0x53564953u || sv_instances[1]!=1u
        || !sv_finite3(origin) || !sv_finite3(direction) || max(abs(direction.x),max(abs(direction.y),abs(direction.z)))<=0.0
        || !(lower>=0.0 && upper>=lower && upper<=3.402823e38)){return sv_invalid();}
    let root=sv_instances[2u];let nodes=sv_instances[3u];
    if(nodes>length/SV_NODE_WORDS || !sv_range(root,nodes*SV_NODE_WORDS,length)){return sv_invalid();}
    let end=root+nodes*SV_NODE_WORDS;var cursor=root;var steps=0u;
    var best:SceneVisibilityHit;best.t=upper;
    loop {
        if(cursor>=end){break;}
        if(steps>=nodes || !sv_range(cursor,SV_NODE_WORDS,end)){return sv_invalid();}steps+=1u;
        let escape=sv_instances[cursor+3u];
        if(escape<=cursor || escape>end || (escape-root)%SV_NODE_WORDS!=0u){return sv_invalid();}
        if(!sv_box(origin,direction,lower,best.t,sv_i3(cursor),sv_i3(cursor+4u))){cursor=escape;continue;}
        let count=sv_instances[cursor+8u];if(count==0u){cursor+=SV_NODE_WORDS;continue;}
        let first=sv_instances[cursor+7u];if(!sv_range(first,count,sv_instances[5u])){return sv_invalid();}
        for(var item=0u;item<count;item+=1u){
            let record=sv_instances[4u]+(first+item)*SV_INSTANCE_WORDS;
            let hit=sv_blas_mode_filtered(record,origin,direction,lower,best.t,any_opaque,previous);
            if(hit.status==SV_INVALID || (any_opaque && hit.status==SV_BLOCKED)){return hit;}
            if(hit.status!=SV_CLEAR){best=hit;}
        }cursor=escape;
    }return best;
}

fn scene_visibility_trace_mode(origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32,any_opaque:bool)->SceneVisibilityHit {
    return scene_visibility_trace_mode_filtered(origin,direction,lower,upper,any_opaque,vec4(0xffffffffu));
}
fn scene_visibility_trace(origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32)->SceneVisibilityHit {
    return scene_visibility_trace_mode(origin,direction,lower,upper,false);
}
// Conservative broad phase only. The no-optics case may stop on an actual covered opaque
// triangle; glass/unresolved candidates retain the original ordered nearest-interface solver.
fn scene_visibility_no_optics(origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32)->bool {
    let length=arrayLength(&sv_instances);if(length<8u){return false;}
    if(sv_instances[6u]!=0x4f50544bu){return false;} // Older payloads retain ordered traversal.
    let root=sv_instances[2u];let nodes=sv_instances[3u];
    if(nodes>length/SV_NODE_WORDS || !sv_range(root,nodes*SV_NODE_WORDS,length)){return false;}
    let end=root+nodes*SV_NODE_WORDS;var cursor=root;var steps=0u;
    loop {
        if(cursor>=end){break;}
        if(steps>=nodes || !sv_range(cursor,SV_NODE_WORDS,end)){return false;}steps+=1u;
        let escape=sv_instances[cursor+3u];
        if(escape<=cursor || escape>end || (escape-root)%SV_NODE_WORDS!=0u){return false;}
        if(sv_instances[cursor+9u]==0u || !sv_box(origin,direction,lower,upper,sv_i3(cursor),sv_i3(cursor+4u))){cursor=escape;continue;}
        let count=sv_instances[cursor+8u];
        if(count>0u){
            let first=sv_instances[cursor+7u];if(!sv_range(first,count,sv_instances[5u])){return false;}
            for(var item=0u;item<count;item+=1u){
                let record=sv_instances[4u]+(first+item)*SV_INSTANCE_WORDS;
                if(!sv_range(record,SV_INSTANCE_WORDS,length)){return false;}
                let material=sv_instances[record+18u];if(material==0u || material==2u){continue;}
                let inverse=mat4x4(vec4(sv_i3(record),bitcast<f32>(sv_instances[record+3u])),
                    vec4(sv_i3(record+4u),bitcast<f32>(sv_instances[record+7u])),
                    vec4(sv_i3(record+8u),bitcast<f32>(sv_instances[record+11u])),
                    vec4(sv_i3(record+12u),bitcast<f32>(sv_instances[record+15u])));
                let geometry=sv_instances[record+16u];let glen=arrayLength(&sv_geometry);
                if(!sv_range(geometry,16u,glen)){return false;}
                let node=sv_geometry[geometry];if(!sv_range(node,SV_NODE_WORDS,glen)){return false;}
                let optical_bounds=sv_bend_bounds(sv_g3(node),sv_g3(node+4u),sv_i3(record+21u));
                if(sv_box((inverse*vec4(origin,1.0)).xyz,(inverse*vec4(direction,0.0)).xyz,
                    lower,upper,optical_bounds.lo,optical_bounds.hi)){return false;}
            }
            cursor=escape;continue;
        }
        cursor+=SV_NODE_WORDS;
    }
    return true;
}

struct SceneVisibilityTransmission {
    // 0 valid (possibly attenuated) connection, 1 physical opaque block, 3 unresolved, 4 invalid.
    // For 3/4 transmission is undefined; the caller must surface/handle status, never call it black.
    status:u32, interfaces:u32, straight_inside_distance_m:f32, _pad:f32,
    transmission:vec3<f32>, _pad2:f32,
};
fn sv_fresnel(cos_incident:f32,eta_i:f32,eta_t:f32)->f32 {
    let c=clamp(cos_incident,0.0,1.0);let eta=eta_i/eta_t;let sin2=eta*eta*(1.0-c*c);
    if(sin2>=1.0){return 1.0;}let ct=sqrt(1.0-sin2);
    let rs=(eta_i*c-eta_t*ct)/(eta_i*c+eta_t*ct);
    let rp=(eta_t*c-eta_i*ct)/(eta_t*c+eta_i*ct);
    return 0.5*(rs*rs+rp*rp);
}
fn sv_next_parameter(t:f32)->f32 {return bitcast<f32>(bitcast<u32>(t)+1u);}
// Thin/parallel pane straight-connection approximation, not refracted-path tracing. Source
// triangle entry/exit and their straight-segment separation are measured, Fresnel at both
// interfaces plus Beer absorption (per metre) applied. Lateral displacement and internal
// reflection transport are absent. Open/nonparallel/nested/wrong-material glass is unresolved.
fn scene_visibility_transmission_to_emitter(origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32,
    glass_material_id:u32,ior:f32,absorption_per_m:vec3<f32>,target_instance:u32)->SceneVisibilityTransmission {
    var result:SceneVisibilityTransmission;result.transmission=vec3(1.0);
    let speed=length(direction);
    if(!(ior>=1.0 && ior<=3.402823e38) || !sv_finite3(absorption_per_m)
        || any(absorption_per_m<vec3(0.0)) || !(speed>0.0 && speed<=3.402823e38)){
        result.status=SV_INVALID;return result;
    }
    if(target_instance==0xffffffffu && scene_visibility_no_optics(origin,direction,lower,upper)){
        let first=scene_visibility_trace_mode(origin,direction,lower,upper,true);
        if(first.status==SV_CLEAR){return result;}
        if(first.status==SV_BLOCKED){result.status=SV_BLOCKED;result.transmission=vec3(0.0);return result;}
        if(first.status==SV_INVALID){result.status=SV_INVALID;return result;}
        // An unexpected optical candidate uses the full solver below, never an assumed answer.
    }
    let unit=direction/speed;var next=lower;
    loop {
        let entry=scene_visibility_trace(origin,direction,next,upper);
        if(entry.status==SV_CLEAR){return result;}
        if(target_instance!=0xffffffffu && entry.instance_id==target_instance && entry.status!=SV_INVALID){return result;}
        if(entry.status==SV_BLOCKED){result.status=SV_BLOCKED;result.transmission=vec3(0.0);return result;}
        if(entry.status==SV_INVALID){result.status=SV_INVALID;return result;}
        if(entry.status!=SV_DIELECTRIC || entry.material_id!=glass_material_id || entry.front_face==0u || result.interfaces>=32u){
            result.status=SV_UNRESOLVED;return result;
        }
        next=sv_next_parameter(entry.t);
        if(next>upper){result.status=SV_UNRESOLVED;return result;}
        let exit=scene_visibility_trace(origin,direction,next,upper);
        if(target_instance!=0xffffffffu && exit.instance_id==target_instance && exit.status!=SV_INVALID){result.status=SV_UNRESOLVED;return result;}
        if(exit.status==SV_BLOCKED){result.status=SV_BLOCKED;result.transmission=vec3(0.0);return result;}
        if(exit.status==SV_INVALID){result.status=SV_INVALID;return result;}
        if(exit.status!=SV_DIELECTRIC || exit.instance_id!=entry.instance_id || exit.material_id!=entry.material_id
            || exit.front_face!=0u || dot(entry.normal,exit.normal)>-0.9999){result.status=SV_UNRESOLVED;return result;}
        let cos_incident=clamp(-dot(unit,entry.normal),0.0,1.0);
        let cos_inside=sqrt(max(0.0,1.0-(1.0-cos_incident*cos_incident)/(ior*ior)));
        let boundary=(1.0-sv_fresnel(cos_incident,1.0,ior))*(1.0-sv_fresnel(cos_inside,ior,1.0));
        let distance=(exit.t-entry.t)*speed;
        result.transmission*=exp(-absorption_per_m*distance)*boundary;
        result.straight_inside_distance_m+=distance;result.interfaces+=2u;
        next=sv_next_parameter(exit.t);if(next>upper){return result;}
    }
    result.status=SV_INVALID;return result;
}

fn scene_visibility_transmission(origin:vec3<f32>,direction:vec3<f32>,lower:f32,upper:f32,
    glass_material_id:u32,ior:f32,absorption_per_m:vec3<f32>)->SceneVisibilityTransmission {
    return scene_visibility_transmission_to_emitter(origin,direction,lower,upper,glass_material_id,ior,absorption_per_m,0xffffffffu);
}

// @environment-optical-path
