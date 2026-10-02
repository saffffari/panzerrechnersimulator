// Bounded specular transmitted-path estimator for distant environment radiance. It follows
// actual source interfaces, returning the outgoing direction: it is NOT a finite light-connection
// solver. Window Fresnel reflection branches and diffuse surface bounce remain separate work.
// Mirror 0.90 and ideal coated-lens 0.96 throughput are the existing reconstructed periscope
// prescription from protective_glass.wgsl, not historical coating measurements.
struct SceneEnvironmentPath {
    status:u32, interfaces:u32, air_path_m:f32, glass_path_m:f32,
    origin:vec3<f32>, _pad0:f32,
    direction:vec3<f32>, _pad1:f32,
    throughput:vec3<f32>, _pad2:f32,
};
fn scene_environment_path(origin:vec3<f32>,direction:vec3<f32>,glass_material_id:u32,
    ior:f32,absorption_per_m:vec3<f32>)->SceneEnvironmentPath {
    var result:SceneEnvironmentPath;result.status=SV_UNRESOLVED;result.origin=origin;
    result.direction=direction;result.throughput=vec3(1.0);
    if(!sv_finite3(direction) || dot(direction,direction)<=0.0 || !(ior>=1.0)
        || !sv_finite3(absorption_per_m) || any(absorption_per_m<vec3(0.0))){result.status=SV_INVALID;return result;}
    if(arrayLength(&sv_instances)<8u){result.status=SV_INVALID;return result;}
    var p=origin;var d=normalize(direction);var inside=0xffffffffu;var previous=vec4(0xffffffffu);
    let budget=clamp(sv_instances[7u]*4u,16u,64u);
    for(var step=0u;step<budget;step+=1u){
        // Only an optical-candidate-free segment may stop at any opaque triangle. Its hit
        // distance is irrelevant to a blocked sky connection; ordered paths always use nearest.
        let lower=select(0.0,0.0001,step==0u);
        let any_opaque=inside==0xffffffffu && scene_visibility_no_optics(p,d,lower,1e8);
        let hit=scene_visibility_trace_mode_filtered(p,d,lower,1e8,any_opaque,previous);
        result.origin=p;result.direction=d;
        if(hit.status==SV_INVALID){result.status=SV_INVALID;return result;}
        if(hit.status==SV_CLEAR){
            if(inside==0xffffffffu){result.status=SV_CLEAR;}
            return result;
        }
        if(hit.status==SV_BLOCKED){result.status=SV_BLOCKED;result.throughput=vec3(0.0);return result;}
        if(inside==0xffffffffu){result.air_path_m+=hit.t;}
        else{result.glass_path_m+=hit.t;result.throughput*=exp(-absorption_per_m*hit.t);}
        let point=p+d*hit.t;let n=select(hit.normal,-hit.normal,dot(d,hit.normal)>0.0);
        if(hit.material_class==5u){
            let focal=bitcast<f32>(sv_instances[hit.instance_record+27u]);
            if(inside!=0xffffffffu || !(focal>0.001)){return result;}
            let axis=-n;let cosine=dot(d,axis);if(cosine<0.05){return result;}
            let height=point-sv_i3(hit.instance_record+24u);
            d=normalize(d/cosine-(height-axis*dot(height,axis))/focal);
            result.throughput*=vec3(0.96);
        }else if(hit.material_class==4u){
            if(inside!=0xffffffffu){return result;}
            d=normalize(reflect(d,n));result.throughput*=vec3(0.90);
        }else if(hit.material_class==1u){
            let exiting=inside!=0xffffffffu;
            if(hit.material_id!=glass_material_id || (exiting && (hit.instance_id!=inside || hit.front_face!=0u))
                || (!exiting && hit.front_face==0u)){return result;}
            let eta_in=select(1.0,ior,exiting);let eta_out=select(ior,1.0,exiting);
            let refracted=refract(d,n,eta_in/eta_out);
            if(dot(refracted,refracted)<0.5){d=normalize(reflect(d,n));}
            else{
                result.throughput*=vec3(1.0-sv_fresnel(-dot(d,n),eta_in,eta_out));
                d=normalize(refracted);inside=select(hit.instance_id,0xffffffffu,exiting);
            }
        }else{return result;}
        result.interfaces+=1u;
        // Only the previous planar triangle is excluded. A metric self-hit slab would erase
        // valid adjacent side exits near a pane rim (observed at 26 micrometres in the game).
        // Beer still starts at the actual interface; other source triangles are untouched.
        p=point;previous=vec4(hit.instance_id,hit.primitive_id,hit.geometry_id,hit.triangle_record);
    }
    result.origin=p;result.direction=d;return result;
}
