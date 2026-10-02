// Playable raster visibility: exact source triangles rasterized into finite-resolution maps.
// The portable optical solver remains separately selectable; these maps approximate glass as
// transmitting without refraction and omit subpixel transient carriers from the occluder set.
const PORTABLE_VISIBILITY:bool=false;
var<private> scene_visibility_unresolved:bool;
fn map_visibility(k:i32,point:vec3<f32>,bias_m:f32)->vec2<f32> {
    let matrix=sky_visibility.view_proj[k];
    let p=matrix*vec4(point,1.0);
    if(any(abs(p.xy)>=vec2(0.99999)) || p.z<=0.0 || p.z>=1.0){return vec2(1.0,0.0);}
    let texels=vec2<f32>(textureDimensions(sky_visibility_map))/sky_visibility.grid.xy;
    let inset=vec2(0.5)/texels;
    let uv=clamp(p.xy*vec2(0.5,-0.5)+vec2(0.5),inset,vec2(1.0)-inset);
    let cell=vec2(f32(k%i32(sky_visibility.grid.x)),f32(k/i32(sky_visibility.grid.x)));
    let depth_per_m=length(vec3(matrix[0].z,matrix[1].z,matrix[2].z));
    return vec2(textureSampleCompareLevel(sky_visibility_map,SHADOW_SKY_COMPARE,
        (cell+uv)/sky_visibility.grid.xy,p.z-bias_m*depth_per_m),1.0);
}
fn map_direction_visibility(point:vec3<f32>,lane:i32)->vec3<f32> {
    let local=map_visibility(lane,point,max(0.0005,sky_visibility.grid.w*0.35));
    let global=map_visibility(lane+32,point,max(0.0005,sky_visibility.global_grid.x*0.65));
    // Both footprints are source geometry. Global coverage is fitted to loaded geometry and
    // the camera's complete medium/far support; there is no all-lit out-of-near-atlas branch.
    if(global.y<=0.0){scene_visibility_unresolved=true;return vec3(1.0,0.0,1.0);}
    return vec3(min(local.x,global.x));
}
fn scene_environment_transmission(origin:vec3<f32>,direction:vec3<f32>)->vec3<f32> {
    if(dot(normalize(direction),sky_visibility.sun_direction.xyz)>0.999999){
        let local=map_visibility(64,origin,max(0.0005,sky_visibility.grid.w*0.35));
        let global=map_visibility(65,origin,max(0.0005,sky_visibility.global_grid.x*0.65));
        if(global.y<=0.0){scene_visibility_unresolved=true;return vec3(1.0,0.0,1.0);}
        return vec3(min(local.x,global.x));
    }
    var best=-2.0;var lane=0;
    for(var k=0;k<32;k+=1){let c=dot(normalize(direction),sky_visibility.directions[k].xyz);if(c>best){best=c;lane=k;}}
    return map_direction_visibility(origin,lane);
}
fn scene_environment_radiance(origin:vec3<f32>,direction:vec3<f32>,lane:i32)->vec3<f32> {
    if(all(probe.lanes[lane].xyz==vec3(0.0))){return vec3(0.0);}
    return probe.lanes[lane].xyz*map_direction_visibility(origin,lane);
}
fn scene_point_transmission(source:vec3<f32>,radius:f32,receiver:vec3<f32>)->vec3<f32> {
    let lamp=cabin_fixture_index(source);
    if(lamp<0){scene_visibility_unresolved=true;return vec3(1.0,0.0,1.0);}
    let offset=receiver-source;let major=max(abs(offset.x),max(abs(offset.y),abs(offset.z)));
    let limits=cabin_shadow_range(lamp);
    if(major>=limits.y){scene_visibility_unresolved=true;return vec3(1.0,0.0,1.0);}
    // Only the numerical 5 mm source-near support is unsampled, never an 8 m light cutoff.
    if(major<=limits.x){return vec3(1.0);}
    let bias=max(0.00005,major*0.00000024);
    return vec3(textureSampleCompareLevel(cabin_shadow_maps,shadow_samp,offset,lamp,
        cabin_shadow_reference(major,bias,lamp)));
}
fn scene_particle_visibility(point:vec3<f32>,direction:vec3<f32>)->f32 {
    return dot(scene_environment_transmission(point,direction),vec3(1.0/3.0));
}
