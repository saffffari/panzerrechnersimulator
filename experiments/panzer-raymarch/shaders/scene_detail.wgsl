// Analytic geometry for an isolated transport/parity study. These are authored shapes, not
// native tank assets, armor meshes, physics bodies, or a recorded/live firefight.
// Every ray must query this same function. Bounds only prune objects that cannot beat the
// current minimum; they never become visible geometry. Distances are conservative metres.
// Material IDs: 1 snow, 2 painted armor, 3 steel, 4 masonry, 6 weathered wood, 7 evergreen.

fn detail_box(p:vec3<f32>,half:vec3<f32>)->f32 {
    let q=abs(p)-half;
    return length(max(q,vec3(0.0)))+min(max(q.x,max(q.y,q.z)),0.0);
}
fn detail_roundbox(p:vec3<f32>,half:vec3<f32>,radius:f32)->f32 {
    return detail_box(p,half-vec3(radius))-radius;
}
fn detail_capsule(p:vec3<f32>,a:vec3<f32>,b:vec3<f32>,radius:f32)->f32 {
    let ab=b-a;let along=clamp(dot(p-a,ab)/dot(ab,ab),0.0,1.0);
    return length(p-a-ab*along)-radius;
}
fn detail_cylinder(p:vec3<f32>,radius:f32,half_height:f32)->f32 {
    let d=vec2(length(p.xz)-radius,abs(p.y)-half_height);
    return length(max(d,vec2(0.0)))+min(max(d.x,d.y),0.0);
}
fn detail_ellipsoid_bound(p:vec3<f32>,radius:vec3<f32>)->f32 {
    // Scaled unit-sphere field times the minimum scale: Lipschitz <= 1. This deliberately
    // underestimates distance rather than using an approximate ellipsoid step that can overshoot.
    // The enclosing box is also a lower distance bound. Its max with the scaled sphere
    // retains conservatism while preventing thin snow caps from limiting every far ray step.
    return max((length(p/radius)-1.0)*min(radius.x,min(radius.y,radius.z)),detail_box(p,radius));
}
fn detail_cone_bound(p:vec3<f32>,radius:f32,height:f32)->f32 {
    // Intersection of normalized conical side and top/bottom half-spaces. A conservative
    // distance bound at the cap/corner, not a smooth-min or an overstepping cone approximation.
    let slope=radius/height;
    let side=(length(p.xz)+slope*p.y-radius)/sqrt(1.0+slope*slope);
    return max(side,max(-p.y,p.y-height));
}
fn detail_yaw(p:vec3<f32>,angle:f32)->vec3<f32> {
    let c=cos(angle);let s=sin(angle);return vec3(c*p.x-s*p.z,p.y,s*p.x+c*p.z);
}
fn detail_nearest(a:vec2<f32>,distance:f32,material:f32)->vec2<f32> {
    return select(a,vec2(distance,material),distance<a.x);
}

fn detail_t34(p:vec3<f32>)->vec2<f32> {
    // T-34-like proportions and inclined hull silhouette. No source armor accuracy is claimed.
    let q=p-vec3(0.0,1.06,0.0);
    var hull=detail_box(q,vec3(1.44,0.49,2.77));
    hull=max(hull,(q.z+0.90*q.y-2.49)/sqrt(1.81));
    hull=max(hull,(-q.z+0.68*q.y-2.59)/sqrt(1.4624));
    hull=max(hull,(abs(q.x)+0.35*q.y-1.31)/sqrt(1.1225));
    var result=vec2(hull,2.0);
    result=detail_nearest(result,detail_box(p-vec3(0.0,0.65,-0.08),vec3(1.12,0.27,2.36)),2.0);
    let track=vec3(abs(p.x)-1.43,p.y-0.66,p.z);
    let outer=detail_roundbox(track,vec3(0.31,0.52,2.72),0.10);
    let opening=detail_roundbox(track,vec3(0.46,0.32,2.43),0.08);
    result=detail_nearest(result,max(outer,-opening),3.0);
    let wheel_z=clamp(round(p.z/1.02),-2.0,2.0)*1.02;
    let wheel=vec3(p.z-wheel_z,abs(p.x)-1.43,p.y-0.66);
    result=detail_nearest(result,detail_cylinder(wheel,0.445,0.24),2.0);
    result=detail_nearest(result,detail_cylinder(wheel,0.12,0.275),3.0);
    let turret=p-vec3(0.0,1.89,0.30);
    result=detail_nearest(result,detail_ellipsoid_bound(turret,vec3(1.03,0.56,1.20)),2.0);
    result=detail_nearest(result,detail_cylinder(p-vec3(0.35,2.42,0.13),0.30,0.035),3.0);
    result=detail_nearest(result,detail_roundbox(p-vec3(0.0,1.92,1.41),vec3(0.39,0.29,0.18),0.10),2.0);
    // Barrel axis is +Z. Two rear fuel drums give the silhouettes a second readable feature.
    result=detail_nearest(result,detail_cylinder(vec3(p.x,p.z-2.91,p.y-1.96),0.064,1.48),3.0);
    result=detail_nearest(result,detail_cylinder(vec3(abs(p.x)-1.17,p.z+1.85,p.y-1.49),0.23,0.55),2.0);
    result=detail_nearest(result,detail_box(p-vec3(0.0,1.575,-1.78),vec3(0.94,0.025,0.65)),3.0);
    // A thin snow cap sits on the engine deck, and is geometry shared by all rays.
    result=detail_nearest(result,detail_ellipsoid_bound(p-vec3(0.0,1.63,-1.79),vec3(0.83,0.085,0.53)),1.0);
    return result;
}

fn detail_dead_birch(p:vec3<f32>,variant:f32)->vec2<f32> {
    let lean=vec3(0.16*variant,0.0,-0.10*variant);
    let crotch=vec3(0.04,2.8,0.0)+lean*0.4;
    var result=vec2(detail_capsule(p,vec3(0.0),crotch,0.135),6.0);
    result=detail_nearest(result,detail_capsule(p,crotch,vec3(0.0,5.35,0.0)+lean,0.078),6.0);
    result=detail_nearest(result,detail_capsule(p,crotch,vec3(-1.13,4.02,0.24)+lean,0.068),6.0);
    result=detail_nearest(result,detail_capsule(p,vec3(-1.13,4.02,0.24)+lean,vec3(-1.46,4.69,0.34)+lean,0.036),6.0);
    result=detail_nearest(result,detail_capsule(p,vec3(0.04,3.72,0.0)+lean*0.7,vec3(0.98,4.69,-0.27)+lean,0.049),6.0);
    result=detail_nearest(result,detail_capsule(p,vec3(0.01,2.1,0.0),vec3(0.68,2.83,0.69),0.046),6.0);
    result=detail_nearest(result,detail_capsule(p,vec3(0.54,2.68,0.52),vec3(0.98,2.89,0.90),0.025),6.0);
    return result;
}
fn detail_conifer(p:vec3<f32>,variant:f32)->vec2<f32> {
    var result=vec2(detail_capsule(p,vec3(0.0),vec3(0.0,6.5,0.0),0.16),6.0);
    let broad=1.0+0.08*variant;
    result=detail_nearest(result,detail_cone_bound(p-vec3(0.0,1.35,0.0),1.47*broad,3.14),7.0);
    result=detail_nearest(result,detail_cone_bound(p-vec3(0.0,2.83,0.0),1.18*broad,2.80),7.0);
    result=detail_nearest(result,detail_cone_bound(p-vec3(0.0,4.35,0.0),0.78*broad,2.40),7.0);
    // Sparse branch-top loads leave evergreen visible rather than coating the entire tree white.
    result=detail_nearest(result,detail_ellipsoid_bound(p-vec3(0.42,2.55,0.40),vec3(0.83,0.13,0.68)),1.0);
    result=detail_nearest(result,detail_ellipsoid_bound(p-vec3(-0.29,3.91,-0.18),vec3(0.63,0.12,0.61)),1.0);
    result=detail_nearest(result,detail_ellipsoid_bound(p-vec3(0.10,5.40,0.10),vec3(0.42,0.095,0.39)),1.0);
    return result;
}

fn detail_ruin(p:vec3<f32>)->vec2<f32> {
    // Finite wall with a real door, window and broken upper corner; all voids cast the same way.
    var wall=detail_box(p-vec3(0.0,1.85,0.0),vec3(4.20,1.85,0.37));
    wall=max(wall,-detail_box(p-vec3(-1.1,1.23,0.0),vec3(0.68,1.30,0.7)));
    wall=max(wall,-detail_box(p-vec3(1.43,2.21,0.0),vec3(0.65,0.68,0.7)));
    wall=max(wall,-detail_box(detail_yaw(p-vec3(3.76,3.89,0.0),0.0),vec3(1.10,0.85,0.8)));
    var result=vec2(wall,4.0);
    result=detail_nearest(result,detail_box(p-vec3(-4.02,0.99,-1.05),vec3(0.42,0.99,1.44)),4.0);
    result=detail_nearest(result,detail_box(detail_yaw(p-vec3(2.95,0.35,1.13),0.51),vec3(1.12,0.35,0.46)),4.0);
    result=detail_nearest(result,detail_box(detail_yaw(p-vec3(-2.45,0.25,1.42),-0.31),vec3(0.54,0.25,0.46)),4.0);
    result=detail_nearest(result,detail_ellipsoid_bound(p-vec3(2.84,0.67,1.12),vec3(0.84,0.11,0.42)),1.0);
    return result;
}

fn scene_detail(p:vec3<f32>)->vec2<f32> {
    var result=vec2(10000.0,4.0);
    // Coarse bounds must contain complete objects, including barrels and branch tips.
    let left=detail_yaw(p-vec3(-9.0,0.0,-10.0),0.39);
    if(detail_box(left-vec3(0.0,1.3,0.8),vec3(1.85,1.35,3.8))<result.x){
        let h=detail_t34(left);result=detail_nearest(result,h.x,h.y);}
    let right=detail_yaw(p-vec3(10.0,0.0,-16.0),-0.56);
    if(detail_box(right-vec3(0.0,1.3,0.8),vec3(1.85,1.35,3.8))<result.x){
        let h=detail_t34(right);result=detail_nearest(result,h.x,h.y);}
    let ruin=detail_yaw(p-vec3(-13.3,0.0,-3.6),0.29);
    if(detail_box(ruin-vec3(0.0,1.85,-0.1),vec3(4.65,1.90,2.8))<result.x){
        let h=detail_ruin(ruin);result=detail_nearest(result,h.x,h.y);}
    let far_wall=detail_yaw(p-vec3(5.4,0.0,-24.0),-0.16);
    if(detail_box(far_wall-vec3(0.0,1.85,-0.1),vec3(4.65,1.90,2.8))<result.x){
        let h=detail_ruin(far_wall);result=detail_nearest(result,h.x,h.y);}
    // Explicit fixed placement: no discontinuous nearest-cell repetition can lose a neighboring
    // tree's surface. Eleven inexpensive bounds reject most tree evaluations for every ray.
    let trees=array<vec4<f32>,11>(
        vec4(-5.8,0.0,7.7,0.0),vec4(8.2,0.0,4.9,1.0),vec4(-14.7,0.0,5.2,0.0),
        vec4(15.7,0.0,-0.4,1.0),vec4(-17.8,0.0,-12.1,1.0),vec4(-4.8,0.0,-16.5,0.0),
        vec4(3.7,0.0,-11.8,0.0),vec4(16.8,0.0,-20.2,1.0),vec4(-10.5,0.0,-26.6,1.0),
        vec4(0.7,0.0,-30.1,0.0),vec4(10.6,0.0,-31.3,1.0));
    for(var i=0u;i<11u;i+=1u){
        let q=detail_yaw(p-trees[i].xyz,f32(i)*1.718);
        if(detail_box(q-vec3(0.0,3.5,0.0),vec3(2.1,3.7,2.1))>=result.x){continue;}
        var h:vec2<f32>;
        if(trees[i].w<0.5){h=detail_dead_birch(q,f32(i%3u)-1.0);}
        else{h=detail_conifer(q,f32(i%3u)-1.0);}
        result=detail_nearest(result,h.x,h.y);
    }
    let mounds=array<vec4<f32>,5>(vec4(-5.1,-0.18,-6.0,1.0),vec4(6.5,-0.20,-5.6,1.4),
        vec4(-12.2,-0.16,-18.0,1.6),vec4(5.4,-0.18,-19.3,1.2),vec4(13.7,-0.20,7.1,1.0));
    for(var i=0u;i<5u;i+=1u){let q=p-mounds[i].xyz;let s=mounds[i].w;
        if(detail_box(q,vec3(1.9*s,0.63*s,1.3*s))<result.x){
            result=detail_nearest(result,detail_ellipsoid_bound(q,vec3(1.9*s,0.63*s,1.3*s)),1.0);}}
    return result;
}
