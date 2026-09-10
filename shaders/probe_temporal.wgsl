// Same-source-pixel temporal statistics. No spatial neighborhood, source-power rescaling,
// clipping or screen-space palette is applied. Source-generation changes invalidate immediately.
struct Params {
    local_inverse_projection:mat4x4<f32>,
    world_to_local:mat4x4<f32>,
    previous_world_to_local:mat4x4<f32>,
    control:vec4<u32>, // face, history valid, max captures, reserved
};
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var raw:texture_2d_array<f32>;
@group(0) @binding(2) var g0:texture_2d_array<f32>;
@group(0) @binding(3) var g1:texture_2d_array<f32>;
@group(0) @binding(4) var g2:texture_2d_array<f32>;
@group(0) @binding(5) var depth:texture_depth_2d_array;
@group(0) @binding(6) var ids:texture_2d_array<u32>;
@group(0) @binding(7) var previous_g0:texture_2d_array<f32>;
@group(0) @binding(8) var previous_g1:texture_2d_array<f32>;
@group(0) @binding(9) var previous_depth:texture_depth_2d_array;
@group(0) @binding(10) var previous_ids:texture_2d_array<u32>;
@group(0) @binding(11) var history:texture_2d_array<f32>;
@group(0) @binding(12) var moments:texture_2d_array<f32>;
@group(0) @binding(13) var next_history:texture_storage_2d_array<rgba32float,write>;
@group(0) @binding(14) var next_moments:texture_storage_2d_array<rgba32float,write>;
@group(0) @binding(15) var resolved:texture_storage_2d_array<rgba16float,write>;
fn position(px:vec2<i32>,size:vec2<u32>,d:f32)->vec3<f32> {
    let uv=(vec2<f32>(px)+vec2(0.5))/vec2<f32>(size);
    let h=p.local_inverse_projection*vec4(uv.x*2.0-1.0,1.0-uv.y*2.0,d,1.0);
    return h.xyz/h.w;
}
@compute @workgroup_size(8,8) fn cs(@builtin(global_invocation_id) gid:vec3<u32>) {
    let size=textureDimensions(raw);if(any(gid.xy>=size)){return;}
    let px=vec2<i32>(gid.xy);let face=i32(p.control.x);
    let sample=textureLoad(raw,px,face,0);
    let d=textureLoad(depth,px,face,0);
    let id=textureLoad(ids,px,face,0).x;
    let a=textureLoad(g0,px,face,0);let b=textureLoad(g1,px,face,0);let c=textureLoad(g2,px,face,0);
    let valid=d<1.0&&a.a<=0.5&&b.a>=0.45&&c.a<=0.05&&(id&0x800000u)==0u
        &&sample.a<0.0&&dot(b.xyz,b.xyz)>0.5&&all(sample.rgb>=vec3(0.0));
    if(!valid){
        textureStore(next_history,px,face,vec4(0.0));textureStore(next_moments,px,face,vec4(0.0));
        textureStore(resolved,px,face,sample);return;
    }
    var count=1.0;var mean=sample.rgb;var second=sample.rgb*sample.rgb;
    if(p.control.y!=0u){
        let pd=textureLoad(previous_depth,px,face,0);
        let pa=textureLoad(previous_g0,px,face,0);let pb=textureLoad(previous_g1,px,face,0);
        let n=normalize((p.world_to_local*vec4(b.xyz,0.0)).xyz);
        let pn=normalize((p.previous_world_to_local*vec4(pb.xyz,0.0)).xyz);
        let separation=position(px,size,pd)-position(px,size,d);
        let old=textureLoad(history,px,face,0);let old_second=textureLoad(moments,px,face,0);
        let compatible=pd<1.0&&textureLoad(previous_ids,px,face,0).x==id
            &&pa.a<=0.5&&dot(pb.xyz,pb.xyz)>0.5&&dot(n,pn)>0.995
            &&abs(dot(separation,n))<=0.004&&abs(pb.a-b.a)<0.02&&length(pa.rgb-a.rgb)<0.02;
        if(compatible&&old.a>=1.0){
            count=min(old.a+1.0,f32(p.control.z));let weight=1.0/count;
            mean=mix(old.rgb,sample.rgb,weight);
            second=mix(old_second.rgb,sample.rgb*sample.rgb,weight);
        }
    }
    // The first and second moments come from the actual transported samples. Alpha records
    // the effective capture count; moments.w is the nonnegative mean RGB population variance.
    let variance=max(second-mean*mean,vec3(0.0));
    textureStore(next_history,px,face,vec4(mean,count));
    textureStore(next_moments,px,face,vec4(second,(variance.x+variance.y+variance.z)/3.0));
    textureStore(resolved,px,face,vec4(mean,sample.a));
}
