// A fleeting curved refraction of existing scene radiance: never a glowing opaque cube.
// Radius is a representative point within this frame's real 343 m/s exposure interval.
struct Front { origin_radius:vec4<f32>, direction_cap:vec4<f32>, optical:vec4<f32>, };
struct Plume { base_heat:vec4<f32>, shape:vec4<f32>, };
struct Uniform { inverse_view_projection:mat4x4<f32>, view_projection:mat4x4<f32>,
    camera:vec4<f32>, viewport:vec4<f32>, fronts:array<Front,4>, plumes:array<Plume,16>, };
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
// ── The physical refraction (pressure_front.rs `physical_refraction`, KESSEL_REFRACTION=physical).
// A front's air: refractivity excess `optical.z` at the front (radius w), falling behind it over the
// positive phase's length `optical.y` as the Friedlander shape (1 - x) e^-x, nothing ahead of it.
// A ray is turned by the transverse gradient of n integrated along it (paraxial): the smooth part
// sampled along the chord, and the front's jump where the ray crosses it, -dn r_perp / |cos| at
// each crossing, its grazing divergence cut where the crossing's footprint reaches a pixel. The
// bent ray leaves the point of closest approach and is traced against the scene; what it meets is
// lit as the frame shows it there (named: the radiance is gathered from the lit frame at the true
// hit, not reshaded along the bent direction).
fn physical_front(index:u32)->bool { return u.fronts[index].optical.x==1.0 && u.fronts[index].optical.y>0.0; }
fn front_excess(f:Front, r:f32)->f32 {
    let radius=f.origin_radius.w; let length=f.optical.y;
    if(r>radius||r<radius-length){return 0.0;}
    let x=(radius-r)/length;
    return f.optical.z*(1.0-x)*exp(-x);
}
fn front_slope(f:Front, r:f32)->f32 {
    // d(excess)/dr inside the profile (the jump at the front is taken separately).
    let radius=f.origin_radius.w; let length=f.optical.y;
    if(r>=radius||r<radius-length){return 0.0;}
    let x=(radius-r)/length;
    return f.optical.z*(2.0-x)*exp(-x)/length;
}
fn trace_hit(origin:vec3<f32>, direction:vec3<f32>, reach:f32)->f32 {
    var query:ray_query;
    rayQueryInitialize(&query,tlas,RayDesc(RAY_FLAG_FORCE_OPAQUE,0xffu,0.002,reach,origin,direction));
    while(rayQueryProceed(&query)) {}
    let hit=rayQueryGetCommittedIntersection(&query);
    return select(-1.0,hit.t,hit.kind!=RAY_QUERY_INTERSECTION_NONE);
}
// ── Hot-object plumes (pressure_front.rs `HeatPlume`): Heskestad's centreline excess and width,
// its turbulence as hashed gradient noise at 0.38 of the local excess, half the width in scale,
// rising at the centreline speed. Air's refractivity follows its density, 1/T.
const PLUME_AIR_K:f32=253.0; const PLUME_N0:f32=2.26e-4*1.39;
fn hash3(p:vec3<f32>)->f32 { return fract(sin(dot(p,vec3(127.1,311.7,74.7)))*43758.5453); }
fn noise3(p:vec3<f32>)->f32 {
    let i=floor(p); let f=fract(p); let w=f*f*(3.0-2.0*f);
    let a=mix(mix(hash3(i),hash3(i+vec3(1.0,0.0,0.0)),w.x),mix(hash3(i+vec3(0.0,1.0,0.0)),hash3(i+vec3(1.0,1.0,0.0)),w.x),w.y);
    let b=mix(mix(hash3(i+vec3(0.0,0.0,1.0)),hash3(i+vec3(1.0,0.0,1.0)),w.x),mix(hash3(i+vec3(0.0,1.0,1.0)),hash3(i+vec3(1.0,1.0,1.0)),w.x),w.y);
    return mix(a,b,w.z)*2.0-1.0;
}
fn plume_heat(p:Plume, z:f32)->vec3<f32> {
    // (centreline excess K, half-width m, rise m/s) at height z.
    let q=max(p.base_heat.w,0.0); let zz=max(z,max(p.shape.y,0.005));
    let k=9.1*pow(PLUME_AIR_K/(9.81*1005.0*1005.0*1.39*1.39),1.0/3.0);
    let excess=min(k*pow(q,2.0/3.0)*pow(zz,-5.0/3.0),p.shape.x);
    let width=max(0.12*zz,max(p.shape.y,0.005));
    let rise=3.4*pow(9.81*q/(1005.0*1.39*PLUME_AIR_K),1.0/3.0)*pow(max(zz,0.05),-1.0/3.0);
    return vec3(excess,width,rise);
}
fn plume_index(p:Plume, world:vec3<f32>, time:f32)->f32 {
    // Refractivity excess over the ambient air at `world`.
    let d=world-p.base_heat.xyz; let z=d.y;
    if(z<=0.0||z>p.shape.z){return 0.0;}
    let h=plume_heat(p,z); let r2=dot(d.xz,d.xz);
    let mean=h.x*exp(-r2/(h.y*h.y));
    if(mean<0.05){return 0.0;}
    let scale=max(0.5*h.y,0.003);
    let q=vec3(d.x,d.y-h.z*time,d.z)/scale+vec3(p.shape.w*17.0);
    let t=mean*(1.0+0.38*1.732*noise3(q));
    return PLUME_N0*(PLUME_AIR_K/(PLUME_AIR_K+t)-1.0);
}
fn plume_bend(ray:vec3<f32>, scene_distance:f32, refractor:ptr<function,f32>)->vec3<f32> {
    var bend=vec3(0.0);
    let count=u32(u.viewport.z); let time=u.viewport.w;
    for(var index=0u;index<min(count,16u);index++) {
        let p=u.plumes[index];
        // The plume's bounding cylinder: radius its width at the top, height its reach.
        let top=p.shape.z; let reach=plume_heat(p,top).y*2.5;
        let rel=u.camera.xyz-p.base_heat.xyz;
        let a=dot(ray.xz,ray.xz); let b=dot(rel.xz,ray.xz); let c=dot(rel.xz,rel.xz)-reach*reach;
        let disc=b*b-a*c; if(a<1.0e-8||disc<=0.0){continue;}
        var near=max((-b-sqrt(disc))/a,0.0); var far=min((-b+sqrt(disc))/a,scene_distance);
        if(far<=near){continue;}
        let steps=24u; let ds=(far-near)/f32(steps); let e=max(0.002,0.1*ds);
        var hit=false;
        for(var k=0u;k<steps;k++) {
            let s=near+(f32(k)+0.5)*ds; let w=u.camera.xyz+ray*s;
            let n=plume_index(p,w,time);
            if(n==0.0){continue;}
            hit=true;
            let g=vec3(plume_index(p,w+vec3(e,0.0,0.0),time)-plume_index(p,w-vec3(e,0.0,0.0),time),
                       plume_index(p,w+vec3(0.0,e,0.0),time)-plume_index(p,w-vec3(0.0,e,0.0),time),
                       plume_index(p,w+vec3(0.0,0.0,e),time)-plume_index(p,w-vec3(0.0,0.0,e),time))/(2.0*e);
            bend+=(g-ray*dot(g,ray))*ds;
        }
        if(hit){ let mid=0.5*(near+far); *refractor=select(mid,min(*refractor,mid),*refractor>0.0); }
    }
    return bend;
}
fn physical_refraction(in:VertexOut, original:vec4<f32>)->vec4<f32> {
    let pixel=vec2<i32>(in.position.xy);
    let depth=textureLoad(scene_depth,pixel,0);
    let endpoint=relative_at(in.uv,min(depth,0.999999));
    let ray=normalize(endpoint);
    let scene_distance=select(10000.0,length(endpoint),depth<0.999999);
    // The angle one pixel spans, for the grazing cut.
    let next=normalize(relative_at(in.uv+vec2(1.0/u.viewport.x,0.0),0.5));
    let pixel_angle=max(acos(clamp(dot(next,normalize(relative_at(in.uv,0.5))),-1.0,1.0)),1.0e-5);
    var bend=vec3(0.0);
    var refractor=-1.0;
    for(var index=0u;index<4u;index++) {
        if(!physical_front(index)){continue;}
        let f=u.fronts[index];
        let radius=f.origin_radius.w;
        let relative=u.camera.xyz-f.origin_radius.xyz;
        let b=dot(relative,ray);
        let discriminant=b*b-dot(relative,relative)+radius*radius;
        if(discriminant<=0.0){continue;}
        let root=sqrt(discriminant);
        let near=max(-b-root,0.0); let far=min(-b+root,scene_distance);
        if(far<=near){continue;}
        // The smooth profile, sampled along the chord.
        let steps=32u; let ds=(far-near)/f32(steps);
        for(var k=0u;k<steps;k++) {
            let s=near+(f32(k)+0.5)*ds;
            let point=relative+ray*s;
            let r=length(point);
            if(r<1.0e-4){continue;}
            let radial=point/r;
            if(dot(radial,f.direction_cap.xyz)<f.direction_cap.w){continue;}
            let slope=front_slope(f,r);
            bend+=(radial-ray*dot(radial,ray))*slope*ds;
        }
        // The jump at each crossing of the front.
        for(var side=0;side<2;side++) {
            let s=select(-b+root,-b-root,side==0);
            if(s<=0.0||s>=scene_distance){continue;}
            let radial=normalize(relative+ray*s);
            if(dot(radial,f.direction_cap.xyz)<f.direction_cap.w){continue;}
            let footprint=max(pixel_angle*s,1.0e-4);
            let cosine=max(abs(dot(radial,ray)),sqrt(2.0*footprint/max(radius,1.0e-3)));
            bend-=(radial-ray*dot(radial,ray))*f.optical.z/cosine;
        }
        let closest=clamp(-b,near,far);
        refractor=select(closest,min(refractor,closest),refractor>0.0);
    }
    bend+=plume_bend(ray,scene_distance,&refractor);
    if(refractor<=0.0||dot(bend,bend)<1.0e-14){return original;}
    let turned=normalize(ray+bend);
    let start=ray*refractor;
    let reach=max(scene_distance*2.0,50.0);
    let t=trace_hit(u.camera.xyz+start,turned,reach);
    let target_rel=start+turned*select(10000.0,t,t>0.0);
    let p=u.view_projection*vec4(target_rel,1.0);
    if(p.w<=0.0){return original;}
    let uv=vec2(p.x/p.w*0.5+0.5,0.5-p.y/p.w*0.5);
    if(any(uv<vec2(0.0))||any(uv>vec2(1.0))){return original;}
    // What the bent ray meets must be what the frame shows there.
    let shown_depth=textureLoad(scene_depth,vec2<i32>(uv*u.viewport.xy),0);
    if(t>0.0&&shown_depth<0.999999){
        let shown=length(relative_at(uv,shown_depth));
        if(abs(shown-length(target_rel))>max(0.05*length(target_rel),0.05)){return original;}
    }
    return vec4(textureSampleLevel(scene_color,scene_sampler,uv,0.0).rgb,original.a);
}

@fragment fn fs(in:VertexOut)->@location(0) vec4<f32> {
    let original=textureSampleLevel(scene_color,scene_sampler,in.uv,0.0);
    if(physical_front(0u)||physical_front(1u)||physical_front(2u)||physical_front(3u)||u.viewport.z>0.0) {
        return physical_refraction(in,original);
    }
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
