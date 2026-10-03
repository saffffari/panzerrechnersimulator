// Source-surface cosmetic trajectories, evaluated from stable emitter identity and elapsed age.
struct WeaponCamera { vp:mat4x4<f32>, eye:vec4<f32>, extent:vec4<f32> };
@group(0) @binding(2) var<uniform> weapon_cam:WeaponCamera;
struct WeaponOut {
    @builtin(position) position:vec4<f32>,
    @location(0) world:vec3<f32>,
    @location(1) @interpolate(flat) radiance:vec3<f32>,
    @location(2) @interpolate(flat) opacity:f32,
};
fn weapon_random(seed:u32)->f32 {
    var s=seed^(seed>>16u);s*=0x7feb352du;s^=s>>15u;s*=0x846ca68bu;s^=s>>16u;
    return f32(s>>8u)/16777216.0;
}
fn weapon_direction(seed:u32)->vec3<f32> {
    let y=weapon_random(seed)*2.0-1.0;let a=weapon_random(seed+1u)*6.28318530718;
    let r=sqrt(max(0.0,1.0-y*y));return vec3(r*cos(a),y,r*sin(a));
}
fn weapon_rotate(source_axis:vec3<f32>,target_axis:vec3<f32>,v:vec3<f32>)->vec3<f32> {
    let d=dot(source_axis,target_axis);
    if(d< -0.999999){let axis=normalize(cross(source_axis,select(vec3(1.0,0.0,0.0),vec3(0.0,1.0,0.0),abs(source_axis.x)>0.9)));return axis*(2.0*dot(axis,v))-v;}
    let q=normalize(vec4(cross(source_axis,target_axis),1.0+d));return v+2.0*cross(q.xyz,cross(q.xyz,v)+q.w*v);
}
fn weapon_hidden()->WeaponOut {
    return WeaponOut(vec4(2.0,2.0,2.0,1.0),vec3(0.0),vec3(0.0),0.0);
}
fn weapon_palette(page:u32)->vec3<f32> {
    if(page==3u){return vec3(1.0,0.9473,0.6795)*9.9;}
    if(page==4u){return vec3(1.0,0.5906,0.0482)*4.38;}
    return vec3(1.0,0.2874,0.0116)*5.30;
}
fn rim_color(initial_kelvin:f32,age:f32)->vec3<f32> {
    let kelvin=293.0+max(initial_kelvin-293.0,0.0)*exp(-max(age,0.0)/2.5);
    if(kelvin<700.0){return vec3(0.0);}
    // Three Planck-band ratios give the cooling color; the shared display radiance scale is
    // authored, not a claim of calibrated steel spectral emissivity or full thermal transport.
    let wavelengths=vec3(650.0,550.0,450.0);
    let reference=exp(14387769.0/(650.0*1800.0))-1.0;
    return pow(vec3(650.0)/wavelengths,vec3(5.0))*reference
        /(exp(vec3(14387769.0)/(wavelengths*kelvin))-vec3(1.0))*4.725;
}
@vertex fn vs_spark(@builtin(vertex_index) vertex:u32,
    @location(0) origin_age:vec4<f32>,@location(1) direction_life:vec4<f32>,
    @location(2) normal_offset:vec4<f32>,@location(3) parameters:vec4<f32>,@location(4) identity:vec4<u32>)->WeaponOut {
    let index=vertex/6u;let corner=vertex%6u;if(index>=identity.y){return weapon_hidden();}
    let kind=identity.w;var t=origin_age.w;let n=normal_offset.xyz;
    let seed=identity.x*select(3948571u,9348571u,kind<=1u)+index*977u;let q=weapon_random(seed);
    var velocity=vec3(0.0);var axis=vec3(0.0,0.0,1.0);var width=0.0;var tail=0.0;var page=2u;var rim_offset=vec3(0.0);
    if(kind==8u){
        if(t>=6.0){return weapon_hidden();}
        let inner=parameters.x;let outer=inner+parameters.y;
        let radius=sqrt(inner*inner+(outer*outer-inner*inner)*(f32(index)+0.5)/f32(identity.y));
        let angle=f32(index)*2.3999631+weapon_random(identity.x)*6.28318530718;
        rim_offset=(direction_life.xyz*cos(angle)+cross(n,direction_life.xyz)*sin(angle))*radius;
        width=sqrt(3.14159265359*(outer*outer-inner*inner)/f32(identity.y));axis=n;
    }else if(kind==7u){
        if(t>=3.0){return weapon_hidden();}
        let spray=weapon_direction(seed+19u);let outward=spray-n*(2.0*min(dot(spray,n),0.0));
        let tangent_raw=direction_life.xyz-n*dot(direction_life.xyz,n);
        let tangent=tangent_raw/max(length(tangent_raw),0.000001);
        let grazing=1.0-clamp(-dot(direction_life.xyz,n),0.0,1.0);
        let ray=select(normalize(n*(0.6+0.4*q)+tangent*(0.15+0.60*grazing)+outward*0.4),n,index==0u);
        let speed=min(sqrt(2.0*parameters.x/max(parameters.y,0.0001)),sqrt(2.0*9.81*6.096));
        velocity=ray*speed*select(0.4+0.6*q,1.0,index==0u);
        let current_velocity=velocity-vec3(0.0,9.81*t,0.0);axis=current_velocity/max(length(current_velocity),0.000001);
        width=2.0*pow(3.0*parameters.y/(4.0*3.14159265359*parameters.w*f32(identity.y)),1.0/3.0);
        tail=width;page=6u;
    }else if(kind==5u || kind==6u){
        let delay=select(q*select(0.45,direction_life.w,kind==5u),0.0,index==0u);
        t-=delay;let life=select(17.0,0.65,kind==5u);if(t<0.0 || t>=life){return weapon_hidden();}
        let spray=weapon_direction(seed+19u);
        if(kind==5u){velocity=normalize(n+spray*0.11)*(26.0+16.0*q);}
        else{
            let horizontal=vec3(n.x,0.0,n.z)/max(length(n.xz),0.000001);
            velocity=vec3(0.0,77.3316*select(0.7+0.3*q,1.0,index==0u),0.0)
                +horizontal*(6.0+12.0*q)+vec3(spray.x,0.0,spray.z)*12.0;
            if(dot(velocity,n)<0.0){velocity-=n*dot(velocity,n);}
            let speed_cap=sqrt(max(0.0,2.0*parameters.x/max(parameters.y,0.001)));
            velocity*=min(1.0,speed_cap/max(length(velocity),0.000001));
        }
        let current_velocity=velocity-vec3(0.0,9.81*t,0.0);axis=current_velocity/max(length(current_velocity),0.000001);
        width=select((0.007+0.006*q)*exp(-t/8.0),(0.008+0.006*q)*sqrt(1.0-t/life),kind==5u);
        tail=clamp(length(velocity-vec3(0.0,9.81*t,0.0))*0.002,width,0.15);
        page=select(2u,4u,t<0.15);
    }else if(kind==3u){
        let life=0.08+0.22*q;if(t>=life){return weapon_hidden();}
        let spray=weapon_direction(seed+19u);let outward=spray-n*(2.0*min(dot(spray,n),0.0));
        let cosine=parameters.y;axis=normalize(direction_life.xyz*(1.0-cosine)*0.8+outward*0.55+n*0.25);
        let speed=min(sqrt(parameters.x/3200.0),2.0)*(4.0+18.0*q)*(0.4+0.6*sqrt(cosine));
        velocity=axis*speed;width=(0.0015+0.002*q)*sqrt(1.0-t/life);tail=clamp(speed*0.002,width,0.04);
        page=select(select(2u,4u,t<0.075),3u,t<0.02);
    }else if(kind==2u){
        let heavy=parameters.w>0.5;let life=select(0.24+0.40*q,0.55+0.85*q,heavy);if(t>=life){return weapon_hidden();}
        axis=normalize(direction_life.xyz*0.75+weapon_direction(seed+19u)*0.30+n*0.25);
        if(dot(axis,n)<0.05){axis=normalize(axis+n*(0.06-dot(axis,n)));}
        let speed=select(5.0+15.0*q,14.0+30.0*q,heavy);velocity=axis*speed;
        width=select(0.003+0.005*q,0.008+0.010*q,heavy)*max(1.0-t/direction_life.w,0.15);
        tail=clamp(speed*0.008,0.025,0.32);page=select(select(2u,4u,t<0.16),3u,t<0.055);
    }else{
        let life=0.45+1.4*q;if(t>=life){return weapon_hidden();}
        var ray=weapon_direction(seed+17u);ray.y=abs(ray.y)*0.75+0.15;
        if(kind==1u){ray=weapon_rotate(vec3(0.0,1.0,0.0),n,ray);}
        velocity=normalize(ray)*(10.0+28.0*q)*parameters.z;
        let current=velocity-vec3(0.0,9.81*t,0.0);axis=normalize(current);
        width=(0.006+0.012*q)*sqrt(1.0-t/life);tail=min(length(current)*0.008,0.32);
        page=select(select(2u,4u,t<0.55),3u,t<0.12);
    }
    let center=select(origin_age.xyz+velocity*t-vec3(0.0,4.905*t*t,0.0),origin_age.xyz+rim_offset,kind==8u);
    if(kind!=0u){
        let local_normal=weapon_rotate(axis,vec3(0.0,0.0,1.0),n);
        let support=dot(abs(local_normal),vec3(width,width,tail))*0.5;
        if(dot(center-origin_age.xyz,n)+normal_offset.w<support){return weapon_hidden();}
    }
    let clip=weapon_cam.vp*vec4(center-weapon_cam.eye.xyz,1.0);
    if(clip.w<=0.0001 || clip.z<0.0 || clip.z>=clip.w){return weapon_hidden();}
    let ndc=clip.xy/clip.w;let pixel=floor(vec2(ndc.x*0.5+0.5,0.5-ndc.y*0.5)*weapon_cam.extent.xy);
    var corners=array<vec2<f32>,6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0));
    let edge=(pixel+corners[corner])/weapon_cam.extent.xy;
    // Density is quadrature, not1000x power. The geometric source support remains millimetres;
    // a native pixel integrates its apparent area. No per-view point-light power changes.
    let projected_area=width*(width+tail)*pow(weapon_cam.extent.z/clip.w,2.0);
    let weight=projected_area*f32(identity.z)/f32(identity.y);
    let position=vec4(vec2(edge.x*2.0-1.0,1.0-edge.y*2.0)*clip.w,clip.z,clip.w);
    if(kind==7u){
        // Ground records distribute a finite estimated mass over the actual requested grains.
        // This is matte matter, never an unlit incandescent color or duplicated smoke volume.
        let area=width*width*0.78539816339*pow(weapon_cam.extent.z/clip.w,2.0);
        return WeaponOut(position,center,mix(vec3(0.16,0.115,0.075),vec3(0.65,0.72,0.80),parameters.z),area);
    }
    if(kind==8u){return WeaponOut(position,center,rim_color(parameters.z,t)*weight,0.0);}
    return WeaponOut(position,center,weapon_palette(page)*weight,0.0);
}
@vertex fn vs_tracer(@builtin(vertex_index) vertex:u32,
    @location(0) start:vec4<f32>,@location(1) end:vec4<f32>,
    @location(2) unused_normal:vec4<f32>,@location(3) unused_parameters:vec4<f32>,@location(4) unused_meta:vec4<u32>)->WeaponOut {
    var a=weapon_cam.vp*vec4(start.xyz-weapon_cam.eye.xyz,1.0);
    var b=weapon_cam.vp*vec4(end.xyz-weapon_cam.eye.xyz,1.0);var wa=start.xyz;var wb=end.xyz;
    if(a.z<0.0 && b.z<0.0){return weapon_hidden();}
    if(a.z<0.0){let t=a.z/(a.z-b.z);a=mix(a,b,t);a.z=0.0;wa=mix(wa,wb,t);}
    if(b.z<0.0){let t=b.z/(b.z-a.z);b=mix(b,a,t);b.z=0.0;wb=mix(wb,wa,t);}
    if(a.w<=0.0 || b.w<=0.0){return weapon_hidden();}
    let pa=a.xy/a.w*weapon_cam.extent.xy*0.5;let pb=b.xy/b.w*weapon_cam.extent.xy*0.5;
    let delta=pb-pa;
    if(dot(delta,delta)<=0.00000001){
        let clip=(a+b)*0.5;let pixel=floor(vec2(clip.x/clip.w*0.5+0.5,0.5-clip.y/clip.w*0.5)*weapon_cam.extent.xy);
        var corners=array<vec2<f32>,6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0));
        let edge=(pixel+corners[vertex])/weapon_cam.extent.xy;
        return WeaponOut(vec4(vec2(edge.x*2.0-1.0,1.0-edge.y*2.0)*clip.w,clip.z,clip.w),(wa+wb)*0.5,vec3(1.0,0.4621,0.0296)*4.725,0.0);
    }
    let normal=normalize(vec2(-delta.y,delta.x));
    var endpoints=array<u32,6>(0u,1u,0u,0u,1u,1u);var sides=array<f32,6>(-0.5,-0.5,0.5,0.5,-0.5,0.5);
    let endpoint=endpoints[vertex];var clip=select(a,b,endpoint==1u);
    clip.x+=normal.x*sides[vertex]*2.0/weapon_cam.extent.x*clip.w;
    clip.y+=normal.y*sides[vertex]*2.0/weapon_cam.extent.y*clip.w;
    return WeaponOut(clip,select(wa,wb,endpoint==1u),vec3(1.0,0.4621,0.0296)*4.725,0.0);
}
@fragment fn fs_weapon(in:WeaponOut)->@location(0) vec4<f32> {
    let dims=vec2<i32>(textureDimensions(depth_tex));let pixel=vec2<i32>(in.position.xy);
    if(any(pixel<vec2(0)) || any(pixel>=dims) || in.position.z>=textureLoad(depth_tex,pixel,0)){discard;}
    if(in.opacity>0.0){
        let alpha=1.0-exp(-in.opacity);let medium=grain_medium(in.world,in.position.xy);
        let source=smoke_light(in.world,normalize(in.world-sm.cam_pos))*in.radiance;
        if(scene_visibility_unresolved){return vec4(1000.0,0.0,1000.0,1.0);}
        return vec4((source*(1.0-medium.a)+medium.rgb)*alpha,alpha);
    }
    return vec4(in.radiance*exp(-grain_tau(in.world)),0.0);
}
