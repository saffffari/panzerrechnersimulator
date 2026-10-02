// Appended to the shared raster-medium functions; lighting and attenuation have one source.
struct GrainCamera { view_proj:mat4x4<f32>, eye:vec4<f32>, extent:vec4<f32> };
@group(0) @binding(2) var<uniform> grain_cam:GrainCamera;
struct GrainOut {
    @builtin(position) position:vec4<f32>,
    @location(0) @interpolate(flat) world:vec3<f32>,
    @location(1) @interpolate(flat) emission:vec4<f32>,
    @location(2) @interpolate(flat) weight:f32,
};
fn grain_radical(mut_value:u32,base:u32)->f32 {
    var value=mut_value; var inverse=1.0/f32(base); var result=0.0;
    for(var digit=0u;digit<20u;digit+=1u){
        result+=f32(value%base)*inverse;value/=base;inverse/=f32(base);
        if(value==0u){break;}
    }
    return result;
}
fn grain_point(index:u32,seed:u32,soot:bool)->vec3<f32>{
    // Prefix identity depends only on parent seed and sample index, never frame or sample count.
    let phase=vec3(grain_radical(seed+1u,2u),grain_radical(seed+1u,3u),grain_radical(seed+1u,5u));
    let q=fract(vec3(grain_radical(index+1u,2u),grain_radical(index+1u,3u),grain_radical(index+1u,5u))+phase);
    if(!soot){return q-vec3(0.5);}
    // The source lobe's actual 12 vertices / 20 triangles define tetrahedra about its origin.
    let phi=(1.0+sqrt(5.0))*0.5;
    var raw=array<vec3<f32>,12>(vec3(-1.0,phi,0.0),vec3(1.0,phi,0.0),vec3(-1.0,-phi,0.0),vec3(1.0,-phi,0.0),
        vec3(0.0,-1.0,phi),vec3(0.0,1.0,phi),vec3(0.0,-1.0,-phi),vec3(0.0,1.0,-phi),
        vec3(phi,0.0,-1.0),vec3(phi,0.0,1.0),vec3(-phi,0.0,-1.0),vec3(-phi,0.0,1.0));
    var faces=array<vec3<u32>,20>(vec3(0u,11u,5u),vec3(0u,5u,1u),vec3(0u,1u,7u),vec3(0u,7u,10u),vec3(0u,10u,11u),
        vec3(1u,5u,9u),vec3(5u,11u,4u),vec3(11u,10u,2u),vec3(10u,7u,6u),vec3(7u,1u,8u),vec3(3u,9u,4u),
        vec3(3u,4u,2u),vec3(3u,2u,6u),vec3(3u,6u,8u),vec3(3u,8u,9u),vec3(4u,9u,5u),vec3(2u,4u,11u),
        vec3(6u,2u,10u),vec3(8u,6u,7u),vec3(9u,8u,1u));
    let ids=faces[(index+seed)%20u];
    let a=normalize(raw[ids.x])*(0.82+0.18*abs(sin(raw[ids.x].y*1.7)))*0.5;
    let b=normalize(raw[ids.y])*(0.82+0.18*abs(sin(raw[ids.y].y*1.7)))*0.5;
    let c=normalize(raw[ids.z])*(0.82+0.18*abs(sin(raw[ids.z].y*1.7)))*0.5;
    let r=sqrt(q.x);return (a*(1.0-r)+b*(r*(1.0-q.y))+c*(r*q.y))*pow(q.z,1.0/3.0);
}
@vertex fn vs_grain(@builtin(vertex_index) vertex:u32,
    @location(0) m0:vec4<f32>,@location(1) m1:vec4<f32>,@location(2) m2:vec4<f32>,@location(3) m3:vec4<f32>,
    @location(4) emission:vec4<f32>,@location(5) parameters:vec4<f32>)->GrainOut{
    let index=vertex/6u+u32(parameters.w);let corner=vertex%6u;
    let model=mat4x4<f32>(m0,m1,m2,m3);
    let point=grain_point(index,u32(parameters.z),emission.w<0.0);
    let world=(model*vec4(point,1.0)).xyz;
    let clip=grain_cam.view_proj*vec4(world-grain_cam.eye.xyz,1.0);
    var out:GrainOut;out.world=world;out.emission=emission;out.weight=parameters.x/parameters.y;
    if(clip.w<=0.001 || clip.z<0.0 || clip.z>=clip.w || index>=u32(parameters.y)){
        out.position=vec4(2.0,2.0,2.0,1.0);return out;
    }
    let ndc=clip.xy/clip.w;
    let pixel=floor(vec2(ndc.x*0.5+0.5,0.5-ndc.y*0.5)*grain_cam.extent.xy);
    var corners=array<vec2<f32>,6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0));
    let edge=(pixel+corners[corner])/grain_cam.extent.xy;
    out.position=vec4(vec2(edge.x*2.0-1.0,1.0-edge.y*2.0)*clip.w,clip.z,clip.w);return out;
}
fn grain_tau(world:vec3<f32>)->f32{
    let delta=world-sm.cam_pos;let distance_m=length(delta);let ray=delta/max(distance_m,0.0001);
    var tau=weather_optical_depth(sm.cam_pos,ray,distance_m,sm.fog_layer.xy,max(sm.fog_density,0.0),sm.world_snow_fraction,sm.weather_motion.xyz,u32(sm.transport_steps.x));
    for(var i=0;i<12;i+=1){let p=sm.puffs[i];if(p.strength<=0.0 || p.age<0.0 || p.age>=PUFF_LIFE){continue;}
        let s=puff_shape(p);tau+=volume_depth(sm.cam_pos,ray,sphere_interval(sm.cam_pos,ray,s.xyz,s.w),distance_m,i).x;}
    if(sm.breech_gas.strength>0.0 && sm.breech_gas.age<2.2){let s=gas_shape(sm.breech_gas);tau+=volume_depth(sm.cam_pos,ray,sphere_interval(sm.cam_pos,ray,s.xyz,s.w),distance_m,12).x;}
    if(sm.interior>0.001){tau+=max(sm.interior,0.0)*0.010*5.2*cabin_path_length(sm.cam_pos,ray,distance_m);}
    return tau;
}
@fragment fn fs_grain(in:GrainOut)->@location(0) vec4<f32>{
    let dims=vec2<i32>(textureDimensions(depth_tex));let pixel=vec2<i32>(in.position.xy);
    if(any(pixel<vec2(0)) || any(pixel>=dims) || in.position.z>=textureLoad(depth_tex,pixel,0)){discard;}
    let ray=normalize(in.world-sm.cam_pos);
    if(in.emission.w>=0.0){
        let x=in.emission.w;
        let radiance=in.emission.rgb*(x+max(x-3.0,0.0)*0.15);
        return vec4(radiance*in.weight*exp(-grain_tau(in.world)),0.0);
    }
    let alpha=1.0-exp(-max(in.weight,0.0));
    let medium=grain_medium(in.world,in.position.xy);
    // Restore the already-composited foreground medium when extinguishing the background.
    let source=smoke_light(in.world,ray)*in.emission.rgb;
    if(scene_visibility_unresolved){return vec4(1000.0,0.0,1000.0,1.0);}
    return vec4((source*(1.0-medium.a)+medium.rgb)*alpha,alpha);
}
