// @weather-field
fn weather_unpartitioned_optical_moment(origin:vec3<f32>,ray:vec3<f32>,limit:f32,layer:vec2<f32>,sigma:f32,snow:f32,offset:vec3<f32>,steps:u32,theatre:f32)->vec2<f32> {
    var begin=0.0;var end=max(limit,0.0);
    if (abs(ray.y)<1e-6) {if (origin.y<layer.x || origin.y>layer.y) {return vec2(0.0);}}
    else {let a=(layer.x-origin.y)/ray.y;let b=(layer.y-origin.y)/ray.y;begin=max(min(a,b),0.0);end=min(max(a,b),end);}
    if (end<=begin || sigma<=0.0) {return vec2(0.0);}
    if (snow<=0.0 && theatre<0.5) {let tau=sigma*(end-begin);return vec2(tau,tau*(begin+end)*0.5);}
    let count=clamp(steps,8u,48u);var depth=vec2(0.0);
    for (var i=0u;i<count;i+=1u) {
        // Quadratic spacing resolves nearby drifting banks before spending samples far away.
        let a=f32(i)/f32(count);let b=f32(i+1u)/f32(count);
        let lo=begin+(end-begin)*a*a;let hi=begin+(end-begin)*b*b;
        let midpoint=0.5*(lo+hi);let point=origin+ray*midpoint;let tau=theatre_extinction(sigma,point,theatre)*weather_relative_density(point,offset,snow,theatre)*(hi-lo);
        depth+=vec2(tau,tau*midpoint);
    }
    return depth;
}

// Integrate each exterior segment, including the portion after a ray leaves a real port/hatch.
// No camera/station switch can clear outside fog, and short interior paths are exactly empty.
fn weather_optical_moment(origin:vec3<f32>,ray:vec3<f32>,limit:f32,layer:vec2<f32>,sigma:f32,snow:f32,offset:vec3<f32>,steps:u32,theatre:f32)->vec2<f32> {
    var cursor=0.0;var depth=vec2(0.0);
    loop {
        if(cursor>=limit){break;}
        let segment=cabin_next_span(origin,ray,cursor,limit);
        let exterior_end=select(segment.x,limit,segment.y<=segment.x);
        if(exterior_end>cursor){let moment=weather_unpartitioned_optical_moment(origin+ray*cursor,ray,exterior_end-cursor,layer,sigma,snow,offset,steps,theatre);depth+=moment+vec2(0.0,moment.x*cursor);}
        if(segment.y<=segment.x){break;}
        cursor=segment.y+CABIN_RAY_EPSILON;
    }
    return depth;
}

fn weather_optical_depth(origin:vec3<f32>,ray:vec3<f32>,limit:f32,layer:vec2<f32>,sigma:f32,snow:f32,offset:vec3<f32>,steps:u32,theatre:f32)->f32 {
    return weather_optical_moment(origin,ray,limit,layer,sigma,snow,offset,steps,theatre).x;
}

// Point-source transport needs exterior weather AND interior extinction along the same ray.
// Share the exact cavity interval walk, retaining the existing exterior integration endpoints,
// per-segment sample positions/count, summation order and cabin endpoint epsilon. The two optical
// depths stay separate so the caller retains the same exp(-world-cabin) evaluation order.
fn weather_and_cabin_optical_depth(origin:vec3<f32>,ray:vec3<f32>,limit:f32,layer:vec2<f32>,sigma:f32,snow:f32,offset:vec3<f32>,steps:u32,theatre:f32,cabin_sigma:f32)->vec2<f32> {
    if(sigma<=0.0 && cabin_sigma<=0.0){return vec2(0.0);}
    var cursor=0.0;var depth=vec2(0.0);var interior_length=0.0;
    loop {
        if(cursor>=limit){break;}
        let segment=cabin_next_span(origin,ray,cursor,limit);
        let exterior_end=select(segment.x,limit,segment.y<=segment.x);
        if(sigma>0.0 && exterior_end>cursor){let moment=weather_unpartitioned_optical_moment(origin+ray*cursor,ray,exterior_end-cursor,layer,sigma,snow,offset,steps,theatre);depth+=moment+vec2(0.0,moment.x*cursor);}
        if(segment.y<=segment.x){break;}
        interior_length+=segment.y-segment.x;
        cursor=segment.y+CABIN_RAY_EPSILON;
    }
    return vec2(depth.x,max(cabin_sigma,0.0)*min(interior_length,max(limit,0.0)));
}
