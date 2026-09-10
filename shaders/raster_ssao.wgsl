// Deterministic depth-neighbourhood ambient occlusion. No noise rotation, temporal history,
// ray queries or blur of the scene image. Only the separately stored ambient term is removed.
@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var ambient_tex: texture_2d<f32>;
struct Parameters {
    inv_view_proj: mat4x4<f32>,
    tuning: vec4<f32>, // world radius, strength, focal pixels, reserved
};
var<push_constant> p: Parameters;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let xy=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    return vec4(xy[i],0.0,1.0);
}
fn position(pixel: vec2<i32>, size: vec2<i32>) -> vec3<f32> {
    let uv=(vec2<f32>(pixel)+vec2(0.5))/vec2<f32>(size);
    let z=textureLoad(depth_tex,clamp(pixel,vec2(0),size-vec2(1)),0);
    let h=p.inv_view_proj*vec4(uv*vec2(2.0,-2.0)+vec2(-1.0,1.0),z,1.0);
    return h.xyz/h.w;
}
@fragment fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let size=vec2<i32>(textureDimensions(depth_tex));
    let pixel=vec2<i32>(frag.xy);
    if (textureLoad(depth_tex,pixel,0)>=0.999999) { return vec4(0.0); }
    let ambient=textureLoad(ambient_tex,pixel,0).rgb;
    if (max(ambient.r,max(ambient.g,ambient.b))<0.00001) { return vec4(0.0); }
    let c=position(pixel,size);
    let dx1=position(pixel+vec2(1,0),size)-c;
    let dx0=c-position(pixel-vec2(1,0),size);
    let dy1=position(pixel+vec2(0,1),size)-c;
    let dy0=c-position(pixel-vec2(0,1),size);
    let dx=select(dx0,dx1,dot(dx1,dx1)<dot(dx0,dx0));
    let dy=select(dy0,dy1,dot(dy1,dy1)<dot(dy0,dy0));
    let cross_normal=cross(dx,dy);
    if (dot(cross_normal,cross_normal)<1e-16) { return vec4(0.0); }
    let n0=normalize(cross_normal);
    let n=n0*select(-1.0,1.0,dot(n0,-c)>=0.0);
    let radius=p.tuning.x;
    let projected_radius=radius*p.tuning.z/max(length(c),0.05);
    let pixel_radius=clamp(projected_radius,1.0,64.0);
    let directions=array<vec2<f32>,8>(vec2(1.0,0.0),vec2(0.7071,0.7071),
        vec2(0.0,1.0),vec2(-0.7071,0.7071),vec2(-1.0,0.0),vec2(-0.7071,-0.7071),
        vec2(0.0,-1.0),vec2(0.7071,-0.7071));
    var obscured=0.0;
    for(var d=0;d<8;d+=1) {
        var horizon=0.0;
        for(var step=1;step<=3;step+=1) {
            let q=pixel+vec2<i32>(round(directions[d]*pixel_radius*f32(step)/3.0));
            if(any(q<vec2(0))||any(q>=size)||textureLoad(depth_tex,q,0)>=0.999999) {continue;}
            let v=position(q,size)-c;
            let distance=length(v);
            if(distance<0.0001||distance>=radius) {continue;}
            let height=max(dot(n,v)-0.012,0.0);
            let occlusion=height/distance*(1.0-smoothstep(radius*0.25,radius,distance));
            horizon=max(horizon,occlusion);
        }
        obscured+=horizon;
    }
    let occlusion=clamp(obscured*(p.tuning.y/8.0)*2.3,0.0,0.80)*smoothstep(0.4,1.5,projected_radius);
    // ReverseSubtract blending: destination HDR minus occluded ambient. Lamps, direct sunlight,
    // highlights, portals and sky retain their original radiance and sharp pixels.
    return vec4(ambient*occlusion,0.0);
}
