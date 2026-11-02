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
// An incandescent spark's colour (rm_sparks.wgsl `spark_glow` is the twin): Planck-band ratios
// at its temperature on the palette's authored display scale; (initial kelvin, fraction of life).
fn spark_glow(glow:vec2<f32>)->vec3<f32> {
    let kelvin=mix(glow.x,1100.0,pow(clamp(glow.y,0.0,1.0),0.7));
    let wavelengths=vec3(650.0,550.0,450.0);
    let reference=exp(14387769.0/(650.0*kelvin))-1.0;
    let chroma=pow(vec3(650.0)/wavelengths,vec3(5.0))*reference/(exp(vec3(14387769.0)/(wavelengths*kelvin))-vec3(1.0));
    return chroma*mix(5.3,9.9,clamp((kelvin-1500.0)/1300.0,0.0,1.0));
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

// The strike kinds' shared pieces (weapon_particle_math.rs is the reference): the record's heavy flag and whether `parameters.z` carries
// the ground height; a per-burst share in 0..1 that is the burst's own for its whole life; and
// the arc of a spark under gravity with one bounce off the known ground, then rest on it.
fn spark_strike_flags(w: f32) -> vec2<bool> {
    let known = w >= 2.0;
    return vec2(w - select(0.0, 2.0, known) > 0.5, known);
}
fn spark_burst_share(seed: u32, salt: u32) -> f32 { return weapon_random(seed * 7919u + salt); }
struct SparkArc { center: vec3<f32>, velocity: vec3<f32>, resting: bool };
fn spark_strike_arc(origin: vec3<f32>, velocity: vec3<f32>, t: f32, ground: f32, known: bool, q: f32) -> SparkArc {
    let g = vec3(0.0, 9.81, 0.0);
    if (!known) { return SparkArc(origin + velocity * t - g * (0.5 * t * t), velocity - g * t, false); }
    let drop = max(origin.y - ground, 0.0);
    let contact_t = (velocity.y + sqrt(velocity.y * velocity.y + 19.62 * drop)) / 9.81;
    if (t <= contact_t) { return SparkArc(origin + velocity * t - g * (0.5 * t * t), velocity - g * t, false); }
    let d = velocity * contact_t - g * (0.5 * contact_t * contact_t);
    let v = velocity - g * contact_t;
    let contact = vec3(origin.x + d.x, ground, origin.z + d.z);
    let rebound = vec3(v.x * 0.55, -v.y * (0.2 + 0.2 * clamp(q, 0.0, 1.0)), v.z * 0.55);
    let t2 = t - contact_t;
    let rest_t = 2.0 * rebound.y / 9.81;
    if (t2 <= rest_t) { return SparkArc(contact + rebound * t2 - g * (0.5 * t2 * t2), rebound - g * t2, false); }
    return SparkArc(contact + rebound * rest_t - g * (0.5 * rest_t * rest_t), vec3(0.0), true);
}
// Samples drawn along each spark's path for the spark kinds (0-3, 5, 6); the record carries
// SPARK_MULTIPLIER (= SPARK_TRAIL) samples a spark (rm_sparks.wgsl is the main view's twin).
const SPARK_TRAIL:u32=1u;
// Ground ejecta grains a record carries per source grain (`GROUND_MULTIPLIER`): a quadrature of
// an estimated mass, not sparks.
const GROUND_QUADRATURE:u32=1000u;
@vertex fn vs_spark(@builtin(vertex_index) vertex:u32,
    @location(0) origin_age:vec4<f32>,@location(1) direction_life:vec4<f32>,
    @location(2) normal_offset:vec4<f32>,@location(3) parameters:vec4<f32>,@location(4) identity:vec4<u32>)->WeaponOut {
    let index=vertex/6u;let corner=vertex%6u;if(index>=identity.y){return weapon_hidden();}
    let kind=identity.w;var t=origin_age.w;let n=normal_offset.xyz;
    // Each spark as a dotted streak of SPARK_TRAIL samples over 14 ms, its whole light shared
    // among them; the ground's and the rim's samples stay quadrature.
    let streaked=kind<=3u || kind==5u || kind==6u;
    let sample=select(index,index/SPARK_TRAIL,streaked);
    if(streaked){if(sample>=identity.z){return weapon_hidden();}t=max(t-f32(index%SPARK_TRAIL)*(0.014/f32(SPARK_TRAIL)),0.0);}
    let gain=select(1.0,f32(identity.y)/(f32(identity.z)*f32(SPARK_TRAIL)),streaked);
    var glow=vec2(2300.0,0.0);
    let seed=identity.x*select(3948571u,9348571u,kind<=1u)+sample*977u;let q=weapon_random(seed);
    var velocity=vec3(0.0);var axis=vec3(0.0,0.0,1.0);var width=0.0;var tail=0.0;var page=2u;var rim_offset=vec3(0.0);
    var arc_center=vec3(0.0);var arc_set=false;
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
        // The full count's grains (`identity.z` before the thousandfold quadrature); a view that
        // draws fewer (`thinned_count`) gives each the optical depth of the grains it stands for.
        width=2.0*pow(3.0*parameters.y/(4.0*3.14159265359*parameters.w*f32(identity.z*GROUND_QUADRATURE)),1.0/3.0);
        tail=width;page=6u;
    }else if(kind==5u || kind==6u){
        let delay=select(q*select(0.45,direction_life.w,kind==5u),0.0,sample==0u);
        t-=delay;let life=select(17.0,0.65,kind==5u);if(t<0.0 || t>=life){return weapon_hidden();}
        let spray=weapon_direction(seed+19u);
        if(kind==5u){velocity=normalize(n+spray*0.11)*(26.0+16.0*q);}
        else{
            let horizontal=vec3(n.x,0.0,n.z)/max(length(n.xz),0.000001);
            velocity=vec3(0.0,77.3316*select(0.7+0.3*q,1.0,sample==0u),0.0)
                +horizontal*(6.0+12.0*q)+vec3(spray.x,0.0,spray.z)*12.0;
            if(dot(velocity,n)<0.0){velocity-=n*dot(velocity,n);}
            let speed_cap=sqrt(max(0.0,2.0*parameters.x/max(parameters.y,0.001)));
            velocity*=min(1.0,speed_cap/max(length(velocity),0.000001));
        }
        let current_velocity=velocity-vec3(0.0,9.81*t,0.0);axis=current_velocity/max(length(current_velocity),0.000001);
        width=select((0.007+0.006*q)*exp(-t/8.0),(0.008+0.006*q)*sqrt(1.0-t/life),kind==5u);
        tail=clamp(length(velocity-vec3(0.0,9.81*t,0.0))*0.002,width,0.15);
        page=select(2u,4u,t<0.15);
        glow=vec2(2300.0+600.0*weapon_random(seed+37u),t/life);
    }else if(kind==3u){
        // Incandescent dust off a struck plate: the solver's energy and incidence, size-set
        // burning times, this burst's own tilt and spread of leaving times (rm_sparks.wgsl).
        let flags=spark_strike_flags(parameters.w);
        let b1=spark_burst_share(identity.x,1u);let b2=spark_burst_share(identity.x,2u);
        t-=weapon_random(seed+23u)*(0.004+0.008*b1);if(t<0.0){return weapon_hidden();}
        let size=weapon_random(seed+29u);
        let life=(0.08+0.22*q)*(0.7+0.6*size);if(t>=life){return weapon_hidden();}
        let spray=weapon_direction(seed+19u);let outward=spray-n*(2.0*min(dot(spray,n),0.0));
        let cosine=parameters.y;
        let tilt=weapon_direction(identity.x*31u+7u)*0.14;
        let ray=normalize(direction_life.xyz*(1.0-cosine)*0.8+outward*0.55+n*0.25+tilt);
        let speed=min(sqrt(parameters.x/3200.0),2.0)*(4.0+18.0*pow(q,1.4))*(0.4+0.6*sqrt(cosine))*(0.9+0.2*b2);
        let arc=spark_strike_arc(origin_age.xyz,ray*speed,t,parameters.z,flags.y,weapon_random(seed+31u));
        arc_center=arc.center;arc_set=true;velocity=ray*speed;
        axis=select(normalize(arc.velocity),normalize(vec3(ray.x,0.0,ray.z)),arc.resting);
        width=(0.0015+0.002*size)*sqrt(1.0-t/life);
        tail=select(clamp(speed*0.002,width,0.04),width,arc.resting);
        page=select(select(2u,4u,t<0.4*life),3u,t<0.1*life);
        glow=vec2(2200.0+900.0*weapon_random(seed+37u),t/life);
    }else if(kind==2u){
        // The fan off a glancing plate: along the outgoing ray, lifting and spreading with its
        // steepness, retained-energy speed law, size-set burning, one bounce off the ground
        // (rm_sparks.wgsl; weapon_particle_math.rs is the reference).
        let flags=spark_strike_flags(parameters.w);let heavy=flags.x;
        let b1=spark_burst_share(identity.x,1u);let b2=spark_burst_share(identity.x,2u);let b3=spark_burst_share(identity.x,3u);
        let cosine=clamp(dot(direction_life.xyz,n),0.0,1.0);
        let hot=select(1.0,clamp(sqrt(parameters.x/select(2500.0,1.8e6,heavy)),0.75,1.35),parameters.x>0.0);
        t-=weapon_random(seed+23u)*select(0.006+0.010*b1,0.012+0.020*b1,heavy);if(t<0.0){return weapon_hidden();}
        let size=weapon_random(seed+29u);
        let life=select(0.2+0.4*q,0.45+0.9*q,heavy)*(0.7+0.6*size);if(t>=life){return weapon_hidden();}
        let spread=(0.18+0.32*cosine)*(0.85+0.3*b2);
        var ray=normalize(direction_life.xyz*(0.55+0.3*(1.0-cosine))+n*(0.12+0.45*cosine)+weapon_direction(seed+19u)*spread);
        if(dot(ray,n)<0.05){ray=normalize(ray+n*(0.06-dot(ray,n)));}
        let speed=select(4.0+16.0*pow(q,1.5),12.0+34.0*pow(q,1.5),heavy)*hot*(0.9+0.2*b3);
        let arc=spark_strike_arc(origin_age.xyz,ray*speed,t,parameters.z,flags.y,weapon_random(seed+31u));
        arc_center=arc.center;arc_set=true;velocity=ray*speed;
        axis=select(normalize(arc.velocity),normalize(vec3(ray.x,0.0,ray.z)),arc.resting);
        width=select(0.0025+0.005*size,0.006+0.012*size,heavy)*max(1.0-t/life,0.15);
        tail=select(clamp(length(arc.velocity)*0.008,0.025,0.32),width,arc.resting);
        let white=0.8+0.4*hot;
        page=select(select(2u,4u,t<0.18*life*white),3u,t<0.05*life*white);
        glow=vec2((2100.0+900.0*weapon_random(seed+37u))*(0.85+0.15*hot),t/life);
    }else{
        // This burst's own lift, narrowing, speed share, lifetimes and leaving times; a blast's
        // parameters.x is its charge against an 88 mm HE round's (rm_sparks.wgsl).
        let b1=spark_burst_share(identity.x,1u);let b2=spark_burst_share(identity.x,2u);let b3=spark_burst_share(identity.x,3u);
        let b4=spark_burst_share(identity.x,4u);let b5=spark_burst_share(identity.x,5u);
        let charge=select(1.0,clamp(sqrt(parameters.x),0.7,1.3),kind==0u && parameters.x>0.0);
        t-=weapon_random(seed+23u)*(0.01+0.03*b1);if(t<0.0){return weapon_hidden();}
        let size=weapon_random(seed+29u);
        let life=(0.45+1.4*q)*(0.8+0.4*b2)*(0.75+0.5*size);if(t>=life){return weapon_hidden();}
        var ray=weapon_direction(seed+17u);ray.y=abs(ray.y)*(0.6+0.3*b3)+(0.08+0.2*b4);
        if(kind==1u){ray=weapon_rotate(vec3(0.0,1.0,0.0),n,ray);}
        velocity=normalize(ray)*(10.0+28.0*pow(q,1.3))*parameters.z*charge*(0.9+0.2*b5);
        let current=velocity-vec3(0.0,9.81*t,0.0);axis=normalize(current);
        width=(0.006+0.012*size)*sqrt(1.0-t/life);tail=min(length(current)*0.008,0.32);
        page=select(select(2u,4u,t<0.35*life),3u,t<0.08*life);
        glow=vec2(2200.0+1200.0*weapon_random(seed+37u)*select(0.75,1.0,kind==0u),t/life);
    }
    var center=select(origin_age.xyz+velocity*t-vec3(0.0,4.905*t*t,0.0),origin_age.xyz+rim_offset,kind==8u);
    if(arc_set){center=arc_center;}
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
    // Capped as in rm_sparks.wgsl: a spark passing the eye is not a bloom blob.
    let projected_area=min(select(width*(width+tail),width*width,streaked)*pow(weapon_cam.extent.z/clip.w,2.0),6.0);
    let weight=projected_area*f32(identity.z)/f32(identity.y);
    let position=vec4(vec2(edge.x*2.0-1.0,1.0-edge.y*2.0)*clip.w,clip.z,clip.w);
    if(kind==7u){
        // Ground records distribute a finite estimated mass over the actual requested grains.
        // This is matte matter, never an unlit incandescent color or duplicated smoke volume.
        let area=width*width*0.78539816339*pow(weapon_cam.extent.z/clip.w,2.0)*f32(identity.z*GROUND_QUADRATURE)/f32(identity.y);
        return WeaponOut(position,center,mix(vec3(0.16,0.115,0.075),vec3(0.65,0.72,0.80),parameters.z),area);
    }
    if(kind==8u){return WeaponOut(position,center,rim_color(parameters.z,t)*weight,0.0);}
    return WeaponOut(position,center,select(weapon_palette(page),spark_glow(glow),streaked)*weight*gain,0.0);
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
        return WeaponOut(vec4(vec2(edge.x*2.0-1.0,1.0-edge.y*2.0)*clip.w,clip.z,clip.w),(wa+wb)*0.5,vec3(1.0,0.705,0.348)*4.725,0.0);
    }
    let normal=normalize(vec2(-delta.y,delta.x));
    var endpoints=array<u32,6>(0u,1u,0u,0u,1u,1u);var sides=array<f32,6>(-0.5,-0.5,0.5,0.5,-0.5,0.5);
    let endpoint=endpoints[vertex];var clip=select(a,b,endpoint==1u);
    clip.x+=normal.x*sides[vertex]*2.0/weapon_cam.extent.x*clip.w;
    clip.y+=normal.y*sides[vertex]*2.0/weapon_cam.extent.y*clip.w;
    return WeaponOut(clip,select(wa,wb,endpoint==1u),vec3(1.0,0.705,0.348)*4.725,0.0);
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
