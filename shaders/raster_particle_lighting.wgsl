// @scene-visibility-particles
// @scene-light-visibility
// Raster carriers read the same physical environment and point-emitter visibility as surfaces.
// The 32-direction sky quadrature is a bounded angular approximation; no fixed color/fill exists.
struct SkyVisibility {
    view_proj:array<mat4x4<f32>,66>,
    directions:array<vec4<f32>,32>,
    grid:vec4<f32>,
    global_grid:vec4<f32>, // coarse world texel, near depth span, global depth span, reserved
    sun_direction:vec4<f32>,
};
struct EnvironmentProbe {lanes:array<vec4<f32>,68>};
@group(1) @binding(0) var sky_visibility_map:texture_depth_2d;
@group(1) @binding(1) var sky_visibility_compare:sampler_comparison;
@group(1) @binding(2) var<uniform> sky_visibility:SkyVisibility;
@group(1) @binding(3) var<uniform> probe:EnvironmentProbe;
// @sky-visibility-sampling
@group(2) @binding(0) var cabin_shadow_maps:texture_depth_cube_array;
@group(2) @binding(1) var shadow_samp:sampler_comparison;
// @cabin-visibility-sampling
@group(2) @binding(2) var<uniform> cabin_shadows:CabinShadows;
fn particle_direction_visibility(point:vec3<f32>,direction:vec3<f32>)->f32 {
    // Snow cannot travel through a pane, even though the pane transmits light.
    return scene_particle_visibility(point,direction);
}
fn particle_phase(cosine:f32,dust:bool)->f32 {
    // Snow carriers approximate randomly oriented diffuse flakes; cabin dust uses the same HG
    // anisotropy as the continuous aerosol. Albedo is applied once after incident transport.
    if(!dust){return 1.0/(4.0*3.14159265);}
    let g=0.42;return (1.0-g*g)/(4.0*3.14159265*pow(1.0+g*g-2.0*g*clamp(cosine,-1.0,1.0),1.5));
}
fn particle_radiance(point:vec3<f32>,dust:bool)->vec3<f32> {
    let view=normalize(point-u.camera.xyz);let sun=normalize(u.sun_direction.xyz);
    var incident=vec3(0.0);
    if(any(probe.lanes[32].xyz!=vec3(0.0))){
        incident=probe.lanes[32].xyz*particle_phase(dot(view,sun),dust)*scene_environment_transmission(point,sun);
    }
    let count=32;
    for(var i=0;i<count;i+=1){
        let direction=probe.lanes[36+i];
        incident+=scene_environment_radiance(point,direction.xyz,i)*direction.w*particle_phase(dot(view,direction.xyz),dust);
    }
    for(var i=0;i<min(i32(u.viewport.w),32);i+=1){
        let source=u.lights[i*2];let emission=u.lights[i*2+1];
        if(emission.w<=0.0 || all(emission.rgb==vec3(0.0))){continue;}
        let delta=source.xyz-point;
        let d2=max(dot(delta,delta),max(source.w*source.w,0.0025));
        let visibility=cabin_volume_light_shadow(source.xyz,source.w,point);if(all(visibility<=vec3(0.0))){continue;}
        let direction=delta*inverseSqrt(d2);let range=max(sqrt(d2)-source.w,0.0);
        let tau=weather_optical_depth(point,direction,range,u.world_medium.yz,u.world_medium.x,u.medium_transport.x,u.medium_transport.yzw,24u)
            +max(u.world_medium.w,0.0)*cabin_path_length(point,direction,range);
        incident+=emission.rgb*max(emission.w,0.0)/d2*particle_phase(dot(view,direction),dust)*visibility*exp(-tau);
    }
    if(scene_visibility_unresolved){return vec3(1000.0,0.0,1000.0);}
    return max(incident,vec3(0.0))*select(0.9,0.70,dust);
}
