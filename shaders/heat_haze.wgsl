// Debug-only screen-space heat shimmer. Prescribed source plumes, not computed gas temperature.
// Refracts existing HDR, with foreground rejection. Offscreen transmission is not solved.
struct Source { origin_radius:vec4<f32>, optical:vec4<f32>, };
struct Uniform { inverse_view_projection:mat4x4<f32>, view_projection:mat4x4<f32>,
    camera:vec4<f32>, viewport:vec4<f32>, sources:array<Source,16>, };
@group(0) @binding(0) var scene_color:texture_2d<f32>;
@group(0) @binding(1) var scene_sampler:sampler;
@group(0) @binding(2) var<uniform> u:Uniform;
@group(0) @binding(3) var scene_depth:texture_depth_2d;
struct VertexOut { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, };
@vertex fn vs(@builtin(vertex_index) i:u32)->VertexOut {
    var corners=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    let p=corners[i];return VertexOut(vec4(p,0.0,1.0),vec2(p.x*0.5+0.5,0.5-p.y*0.5));
}
fn relative_at(uv:vec2<f32>, depth:f32)->vec3<f32> {
    let p=u.inverse_view_projection*vec4(uv.x*2.0-1.0,1.0-uv.y*2.0,depth,1.0);
    return p.xyz/p.w;
}
@fragment fn fs(in:VertexOut)->@location(0) vec4<f32> {
    let original=textureSampleLevel(scene_color,scene_sampler,in.uv,0.0);
    let depth=textureLoad(scene_depth,vec2<i32>(in.position.xy),0);
    let endpoint=relative_at(in.uv,min(depth,0.999999));
    let ray=normalize(endpoint);
    let scene_distance=select(10000.0,length(endpoint),depth<0.999999);
    var displacement=vec2(0.0);var nearest=10000.0;var strength=0.0;
    for(var i=0u;i<16u;i++) {
        let s=u.sources[i];let height=s.optical.x;
        if(height<=0.0||s.optical.y<=0.0){continue;}
        let radius=s.origin_radius.w;
        let center=s.origin_radius.xyz+vec3(0.0,height*0.5,0.0);
        let axes=vec3(radius,height*0.5,radius);
        let o=(u.camera.xyz-center)/axes;let d=ray/axes;
        let a=dot(d,d);let b=dot(o,d);let c=dot(o,o)-1.0;
        let disc=b*b-a*c;if(disc<=0.0){continue;}
        let root=sqrt(disc);let near=max((-b-root)/a,0.005);
        let far=min((-b+root)/a,scene_distance-0.01);
        if(far<=near){continue;}
        let travel=(near+far)*0.5;
        let p=u.camera.xyz+ray*travel-s.origin_radius.xyz;
        let h=clamp(p.y/height,0.0,1.0);
        let r2=dot(p.xz,p.xz)/(radius*radius);
        let envelope=smoothstep(0.0,0.14,h)*(1.0-smoothstep(0.45,1.0,h))
            *pow(max(1.0-r2,0.0),2.0)*s.optical.y;
        // Advected smooth curls climb at a source-relative speed; no screen-locked noise.
        let t=u.camera.w+s.optical.z;
        let q=p/max(radius,0.04);
        let phase=q.y*5.0-t*3.2;
        let warp=vec3(sin(q.x*4.1+phase)*cos(q.z*3.7-phase*0.6),
            sin(q.x*2.8+q.z*3.2-phase*0.8)*0.45,
            cos(q.z*4.3+phase*1.1)*sin(q.x*3.3-phase*0.5));
        let bend=u.view_projection*vec4(ray*travel+warp*0.012*envelope,1.0);
        if(bend.w<=0.0){continue;}
        let uv=vec2(bend.x/bend.w*0.5+0.5,0.5-bend.y/bend.w*0.5);
        displacement+=(uv-in.uv)*min((far-near)/max(radius,0.02),1.0);
        strength=max(strength,envelope);nearest=min(nearest,near);
    }
    if(strength<=0.0){return original;}
    // Bound the prototype at 2 native pixels; avoid smearing the entire image or dragging UI.
    let offset=clamp(displacement,-vec2(2.0)/u.viewport.xy,vec2(2.0)/u.viewport.xy);
    let shifted=clamp(in.uv+offset,vec2(0.0),vec2(0.999999));
    let shifted_depth=textureLoad(scene_depth,vec2<i32>(shifted*u.viewport.xy),0);
    if(shifted_depth<0.999999 && length(relative_at(shifted,shifted_depth))<nearest-0.01){return original;}
    let refracted=textureSampleLevel(scene_color,scene_sampler,shifted,0.0).rgb;
    return vec4(mix(original.rgb,refracted,min(strength*2.0,0.85)),original.a);
}
