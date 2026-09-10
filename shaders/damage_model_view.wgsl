// Explicit diagnostic source sectioning. It is independent of gameplay transport and visibility.
struct Camera { vp:mat4x4<f32>, section:vec4<f32> }
struct Instance { model:mat4x4<f32>, normal:mat4x4<f32>, tint:vec4<f32>, flags:vec4<f32> }
@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var<storage,read> instances:array<Instance>;
@group(1) @binding(0) var albedo:texture_2d<f32>;
@group(1) @binding(1) var metallic_roughness:texture_2d<f32>;
@group(1) @binding(2) var normal_map:texture_2d<f32>;
@group(1) @binding(3) var source_sampler:sampler;
struct Out {
 @builtin(position) clip:vec4<f32>,
 @location(0) position:vec3<f32>, @location(1) normal:vec3<f32>, @location(2) uv:vec2<f32>,
 @location(3) @interpolate(flat) tint:vec4<f32>, @location(4) @interpolate(flat) flags:vec4<f32>,
}
@vertex fn vs(@location(0) p:vec3<f32>,@location(1) n:vec3<f32>,@location(2) uv:vec2<f32>,
 @location(3) tangent:vec4<f32>,@builtin(instance_index) index:u32)->Out {
 let item=instances[index];let hp=item.model*vec4<f32>(p,1.);
 var o:Out;o.clip=camera.vp*hp;
 o.position=hp.xyz;o.normal=(item.normal*vec4<f32>(n,0.)).xyz;o.uv=uv;o.tint=item.tint;o.flags=item.flags;
 return o;
}
@fragment fn fs(v:Out,@builtin(front_facing) front:bool)->@location(0) vec4<f32> {
 let source=textureSample(albedo,source_sampler,v.uv);
 if v.flags.x>0.5 {
   // Remove the roof and the near-side shell above the low cabin section. Gun and separately
   // retained interior controls bypass this shell cut. Every retained fragment remains an original
   // source triangle/material; no spherical module stand-ins or reconstructed hollow shell.
   if v.position.y>camera.section.w ||
      (v.position.y>camera.section.z && dot(v.position.xz,camera.section.xy)>0.) {discard;}
 }
 if source.a<0.1 {discard;}
 let n=normalize(v.normal)*select(-1.,1.,front);
 let light=0.38+0.62*abs(dot(n,normalize(vec3<f32>(0.45,0.85,0.3))));
 let base=mix(vec3<f32>(1.),source.rgb,v.flags.y*0.75);
 // A bounded view light is explicitly diagnostic, not a replacement cabin lighting result.
 return vec4<f32>(base*v.tint.rgb*light,1.);
}
