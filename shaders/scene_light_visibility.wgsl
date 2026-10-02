const PORTABLE_VISIBILITY:bool=true;
// Exact loaded source triangles, including terrain and visible view-local bodies. These rays do
// not depend on the camera's shadow-map footprint. The finite ray endpoint is beyond the source
// scene's practical extent (1e8 m), rather than a distance beyond which visibility becomes open.
var<private> scene_visibility_unresolved:bool;
fn scene_light_transmission(origin:vec3<f32>,direction:vec3<f32>,distance_m:f32)->vec3<f32> {
    if(distance_m<=0.0001){return vec3(1.0);}
    let result=scene_visibility_transmission(origin,direction,0.0001,distance_m,1u,1.52,vec3(0.080,0.035,0.020));
    if(result.status==SV_CLEAR || result.status==SV_BLOCKED){return result.transmission;}
    // Unsupported optical geometry is a conspicuous diagnostic, never an invented lighting
    // answer. Production review must resolve these pixels or explicitly reject that view.
    scene_visibility_unresolved=true;
    return vec3(1.0,0.0,1.0);
}
fn scene_environment_transmission(origin:vec3<f32>,direction:vec3<f32>)->vec3<f32> {
    return scene_light_transmission(origin,direction,1e8);
}
fn scene_point_transmission(source:vec3<f32>,radius:f32,receiver:vec3<f32>)->vec3<f32> {
    let delta=source-receiver;let range=length(delta);
    if(range<=0.0001){return vec3(1.0);}
    // Arrival is at the actual registered emitting surface, or the compact source position.
    // An enclosing source radius never removes armor from this visibility connection.
    var emitter_id=0xffffffffu;
    let lamp=cabin_fixture_index(source);
    if(lamp>=0 && cabin_shadows.lamps[lamp].w>=2.0){emitter_id=u32(cabin_shadows.lamps[lamp].w-2.0);}
    let result=scene_visibility_transmission_to_emitter(receiver,delta/range,0.0001,max(range-0.0001,0.0001),
        1u,1.52,vec3(0.080,0.035,0.020),emitter_id);
    if(result.status==SV_CLEAR || result.status==SV_BLOCKED){return result.transmission;}
    scene_visibility_unresolved=true;return vec3(1.0,0.0,1.0);
}

// Radiance in the RECEIVER's initial direction after the source optical assembly. Unlike a
// finite point-source visibility factor this may leave toward a different environment direction.
// The probe is already a 32-cell cone-mean angular approximation. Bent exits use its nearest
// angular cell; unchanged directions retain their exact original lane and quadrature weight.
fn scene_environment_radiance(origin:vec3<f32>,direction:vec3<f32>,lane:i32)->vec3<f32> {
    if(all(probe.lanes[lane].xyz==vec3(0.0)) && scene_visibility_no_optics(origin,direction,0.0001,1e8)){
        return vec3(0.0);
    }
    let path=scene_environment_path(origin,direction,1u,1.52,vec3(0.080,0.035,0.020));
    if(path.status==SV_BLOCKED){return vec3(0.0);}
    if(path.status!=SV_CLEAR){scene_visibility_unresolved=true;return vec3(1.0,0.0,1.0);}
    var sample_lane=lane;
    if(dot(path.direction,normalize(direction))<0.9999999){
        var best=-2.0;
        for(var k=0;k<32;k+=1){
            let cosine=dot(path.direction,probe.lanes[36+k].xyz);
            if(cosine>best){best=cosine;sample_lane=k;}
        }
    }
    return probe.lanes[sample_lane].xyz*path.throughput;
}

fn scene_particle_visibility(point:vec3<f32>,direction:vec3<f32>)->f32 {
 return select(0.0,1.0,scene_visibility_trace(point,direction,0.0001,1e8).status==SV_CLEAR);
}
