// A fleeting curved refraction of existing scene radiance: never a glowing opaque cube.
// Radius is a representative point within this frame's real 343 m/s exposure interval.
struct Front { origin_radius:vec4<f32>, direction_cap:vec4<f32>, optical:vec4<f32>, };
struct Uniform { inverse_view_projection:mat4x4<f32>, view_projection:mat4x4<f32>,
    camera:vec4<f32>, viewport:vec4<f32>, fronts:array<Front,4>, };
@group(0) @binding(0) var scene_color:texture_2d<f32>;
@group(0) @binding(1) var scene_sampler:sampler;
@group(0) @binding(2) var<uniform> u:Uniform;
@group(0) @binding(3) var scene_depth:texture_depth_2d;
@group(1) @binding(0) var tlas:acceleration_structure;
struct VertexOut { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, };
@vertex fn vs(@builtin(vertex_index) i:u32)->VertexOut {
    var corners=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    let p=corners[i];return VertexOut(vec4(p,0.0,1.0),vec2(p.x*0.5+0.5,0.5-p.y*0.5));
}
fn relative_at(uv:vec2<f32>, depth:f32)->vec3<f32> {
    let p=u.inverse_view_projection*vec4(uv.x*2.0-1.0,1.0-uv.y*2.0,depth,1.0);
    return p.xyz/p.w;
}
fn source_blocked(origin:vec3<f32>, direction:vec3<f32>, distance_m:f32)->bool {
    var query:ray_query;
    rayQueryInitialize(&query,tlas,RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT|RAY_FLAG_FORCE_OPAQUE,
        0xffu,0.002,max(distance_m-0.004,0.003),origin,direction));
    while(rayQueryProceed(&query)) {}
    return rayQueryGetCommittedIntersection(&query).kind!=RAY_QUERY_INTERSECTION_NONE;
}
@fragment fn fs(in:VertexOut)->@location(0) vec4<f32> {
    let original=textureSampleLevel(scene_color,scene_sampler,in.uv,0.0);
    let pixel=vec2<i32>(in.position.xy);
    let depth=textureLoad(scene_depth,pixel,0);
    let endpoint=relative_at(in.uv,min(depth,0.999999));
    let ray=normalize(endpoint);
    let scene_distance=select(10000.0,length(endpoint),depth<0.999999);
    var displacement=vec2(0.0);var weight=0.0;var nearest_front=10000.0;
    for(var index=0u;index<4u;index++) {
        let front=u.fronts[index];let radius=front.origin_radius.w;
        if(front.optical.x<=0.0||radius<=0.0){continue;}
        let relative=u.camera.xyz-front.origin_radius.xyz;
        let b=dot(relative,ray);let discriminant=b*b-dot(relative,relative)+radius*radius;
        if(discriminant<=0.0){continue;}
        let root=sqrt(discriminant);let near=-b-root;let far=-b+root;
        let travel=select(far,near,near>0.005);
        if(travel<=0.005||travel>=scene_distance-0.005){continue;}
        let point=u.camera.xyz+ray*travel;
        let radial=normalize(point-front.origin_radius.xyz);
        let cap=dot(radial,front.direction_cap.xyz);
        if(cap<front.direction_cap.w){continue;}
        if(source_blocked(front.origin_radius.xyz, radial, radius)){continue;}
        // Inward source-facing cap and the spherical silhouette fade continuously, not in cells.
        let edge=select(smoothstep(front.direction_cap.w,front.direction_cap.w+0.16,cap),1.0,front.direction_cap.w < -0.99);
        let facing=abs(dot(radial,ray));
        let optical=min(front.optical.x,0.08)*edge*(0.30+0.70*(1.0-facing));
        let p=u.view_projection*vec4(ray*travel+radial*0.03,1.0);
        let projected=vec2(p.x/p.w*0.5+0.5,0.5-p.y/p.w*0.5)-in.uv;
        let direction=projected/max(length(projected),0.000001);
        displacement+=direction*min(front.optical.z,1.5)/u.viewport.xy*optical/0.08;
        weight=max(weight,optical);nearest_front=min(nearest_front,travel);
    }
    if(weight<=0.0){return original;}
    let offset=clamp(displacement,-vec2(1.5)/u.viewport.xy,vec2(1.5)/u.viewport.xy);
    let shifted=clamp(in.uv+offset,vec2(0.0),vec2(0.999999));
    let shifted_pixel=vec2<i32>(shifted*u.viewport.xy);
    let shifted_depth=textureLoad(scene_depth,shifted_pixel,0);
    if(shifted_depth<0.999999 && length(relative_at(shifted,shifted_depth))<nearest_front-0.005){return original;}
    let refracted=textureSampleLevel(scene_color,scene_sampler,shifted,0.0).rgb;
    return vec4(mix(original.rgb, refracted, weight),original.a);
}
