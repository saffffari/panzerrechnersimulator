// Source cube-face texel cells lifted onto their measured tangent plane. This is a diffuse
// carrier presentation, not a billboard overlay or a denoiser. Exact eye visibility wins.
struct SplatVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) source: u32,
    @location(1) @interpolate(flat) center: vec3<f32>,
    @location(2) @interpolate(flat) normal: vec3<f32>,
};
fn source_position(face:i32, pixel:vec2<f32>, depth:f32)->vec3<f32> {
    let size=vec2<f32>(textureDimensions(probe_depth));
    let uv=pixel/size;
    let h=p.faces[face].local_inv_vp*vec4(uv.x*2.0-1.0,1.0-uv.y*2.0,depth,1.0);
    return h.xyz/h.w;
}
fn eye_clip(relative:vec3<f32>)->vec4<f32> {
    let z=dot(relative,p.forward_local.xyz);
    let x=dot(relative,p.right_local.xyz)/dot(p.right_local.xyz,p.right_local.xyz);
    let y=dot(relative,p.up_local.xyz)/dot(p.up_local.xyz,p.up_local.xyz);
    let a=p.projection.y/(p.projection.y-p.projection.x);
    return vec4(x,y,a*(z-p.projection.x),z);
}
@vertex fn vs(@builtin(vertex_index) vertex:u32,@builtin(instance_index) instance:u32)->SplatVertex {
    let size=textureDimensions(probe_depth);
    let area=size.x*size.y;
    let face=i32(instance/area);
    let index=instance%area;
    let pixel=vec2<i32>(i32(index%size.x),i32(index/size.x));
    var output:SplatVertex;
    // Invalid cells become a degenerate triangle outside the clip volume.
    output.clip=vec4(2.0,2.0,2.0,1.0);output.source=instance;
    output.center=vec3(0.0);output.normal=vec3(0.0);
    let f=p.faces[face];
    if(p.projection.z<0.5||f.validity.x<0.5||f.validity.y>0.25){return output;}
    let d=textureLoad(probe_depth,pixel,face,0);
    let id=textureLoad(probe_id,pixel,face,0).x;
    let g0=textureLoad(probe_g0,pixel,face,0);
    let g1=textureLoad(probe_g1,pixel,face,0);
    let g2=textureLoad(probe_g2,pixel,face,0);
    let carrier=textureLoad(probe_diffuse,pixel,face,0);
    if(d>=1.0||(id&0x800000u)!=0u||g0.a>0.5||g1.a<0.45||g2.a>0.05
        ||dot(g1.xyz,g1.xyz)<0.5||carrier.a>=0.0){return output;}
    let center=source_position(face,vec2<f32>(pixel)+vec2(0.5),d);
    let n=normalize((f.capture_world_to_local*vec4(g1.xyz,0.0)).xyz);
    let probe_to_center=center-p.probe_local.xyz;
    let distance=length(probe_to_center);
    if(distance<0.05||distance>8.0||dot(n,-probe_to_center/distance)<0.35){return output;}
    var corners=array<vec2<f32>,6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),
        vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0));
    // Exact source-cell extent: no guessed screen-space enlargement. Each corner follows the
    // source camera ray to this tangent plane, preserving plane tilt when the eye moves.
    var corner=vec3(0.0);
    // Validate the complete cell for every invocation. Culling only one vertex could create
    // a stretched triangle connecting an invalid corner to the valid part of the source cell.
    for(var c=0u;c<6u;c++) {
        let ray=source_position(face,vec2<f32>(pixel)+corners[c],d)-p.probe_local.xyz;
        let denominator=dot(n,ray);
        if(abs(denominator)<0.0001){return output;}
        let t=dot(n,probe_to_center)/denominator;
        let lifted=p.probe_local.xyz+ray*t;
        if(t<=0.0||length(lifted-center)>distance*4.0/f32(size.x)){return output;}
        if(c==vertex){corner=lifted;}
    }
    output.clip=eye_clip(corner-p.eye_local.xyz);
    output.center=center;output.normal=n;
    return output;
}
struct SplatOutput {@location(0) diffuse:vec4<f32>,@location(1) mask:vec4<f32>,};
@fragment fn fs(input:SplatVertex)->SplatOutput {
    let size=textureDimensions(eye_diffuse);
    let pixel=vec2<i32>(input.clip.xy);
    let depth=textureLoad(eye_depth,pixel,0);
    let id=textureLoad(eye_id,pixel,0).x;
    let g0=textureLoad(eye_g0,pixel,0);
    let g1=textureLoad(eye_g1,pixel,0);
    let g2=textureLoad(eye_g2,pixel,0);
    if(depth>=1.0||g0.a>0.5||(id&0x800000u)!=0u||g1.a<0.45||g2.a>0.05
        ||dot(g1.xyz,g1.xyz)<0.5){discard;}
    let source_size=textureDimensions(probe_depth);
    let area=source_size.x*source_size.y;
    let face=i32(input.source/area);
    let index=input.source%area;
    let source=vec2<i32>(i32(index%source_size.x),i32(index/source_size.x));
    if(textureLoad(probe_id,source,face,0).x!=id){discard;}
    let pg0=textureLoad(probe_g0,source,face,0);
    let pg1=textureLoad(probe_g1,source,face,0);
    if(abs(pg1.a-g1.a)>0.05||length(pg0.rgb-g0.rgb)>0.18){discard;}
    let n=normalize((p.world_to_local*vec4(g1.xyz,0.0)).xyz);
    if(dot(n,input.normal)<0.99){discard;}
    let ndc=vec2((f32(pixel.x)+0.5)/f32(size.x)*2.0-1.0,
        1.0-(f32(pixel.y)+0.5)/f32(size.y)*2.0);
    let position=p.eye_local.xyz+(p.forward_local.xyz+p.right_local.xyz*ndc.x
        +p.up_local.xyz*ndc.y)*linear_depth(depth);
    // Full ID alone cannot distinguish two plates in the same mesh. Do not loosen this
    // plane guard with range: the primary camera-relative projection keeps its error bounded.
    if(abs(dot(input.center-position,n))>0.004){discard;}
    let distance=length(p.probe_local.xyz-position);
    if(distance<0.05||distance>8.0){discard;}
    let probe_view=(p.probe_local.xyz-position)/distance;
    let eye_view=normalize(p.eye_local.xyz-position);
    if(dot(probe_view,eye_view)<0.98||dot(n,probe_view)<0.35||dot(n,eye_view)<0.35){discard;}
    let fallback=textureLoad(eye_diffuse,pixel,0);
    let radiance=textureLoad(probe_diffuse,source,face,0);
    return SplatOutput(vec4(radiance.rgb,fallback.a),vec4(1.0,f32(face+1)/6.0,1.0,1.0));
}
