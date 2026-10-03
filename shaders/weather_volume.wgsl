// Shared world-space blowing-snow density. Wind advects the field; neither camera nor frame
// index participates in identity. Density is extinction matter, never an additive white overlay.
fn weather_hash(p:vec3<i32>)->f32 {
    var h=bitcast<u32>(p.x)*1597334677u ^ bitcast<u32>(p.y)*3812015801u ^ bitcast<u32>(p.z)*2798796415u;
    h=(h^(h>>16u))*2246822519u;h=(h^(h>>13u))*3266489917u;
    return f32((h^(h>>16u))>>8u)/16777216.0;
}
fn weather_noise(p:vec3<f32>)->f32 {
    let cell=vec3<i32>(floor(p));let f=fract(p);let u=f*f*(vec3(3.0)-2.0*f);
    let a=mix(weather_hash(cell),weather_hash(cell+vec3(1,0,0)),u.x);
    let b=mix(weather_hash(cell+vec3(0,1,0)),weather_hash(cell+vec3(1,1,0)),u.x);
    let c=mix(weather_hash(cell+vec3(0,0,1)),weather_hash(cell+vec3(1,0,1)),u.x);
    let d=mix(weather_hash(cell+vec3(0,1,1)),weather_hash(cell+vec3(1,1,1)),u.x);
    return mix(mix(a,b,u.y),mix(c,d,u.y),u.z);
}
fn weather_relative_density(p:vec3<f32>,offset:vec3<f32>,snow:f32)->f32 {
    let q=p-offset;
    let broad=weather_noise(q/vec3(110.0,42.0,110.0));
    let eddies=weather_noise(q/vec3(27.0,13.0,27.0)+vec3(19.7,3.1,-6.3));
    let banks=0.22+2.15*smoothstep(0.18,0.82,broad*0.72+eddies*0.28);
    return mix(1.0,banks,clamp(snow,0.0,1.0));
}
fn weather_unpartitioned_optical_moment(origin:vec3<f32>,ray:vec3<f32>,limit:f32,layer:vec2<f32>,sigma:f32,snow:f32,offset:vec3<f32>,steps:u32)->vec2<f32> {
    var begin=0.0;var end=max(limit,0.0);
    if (abs(ray.y)<1e-6) {if (origin.y<layer.x || origin.y>layer.y) {return vec2(0.0);}}
    else {let a=(layer.x-origin.y)/ray.y;let b=(layer.y-origin.y)/ray.y;begin=max(min(a,b),0.0);end=min(max(a,b),end);}
    if (end<=begin || sigma<=0.0) {return vec2(0.0);}
    if (snow<=0.0) {let tau=sigma*(end-begin);return vec2(tau,tau*(begin+end)*0.5);}
    let count=clamp(steps,8u,48u);var depth=vec2(0.0);
    for (var i=0u;i<count;i+=1u) {
        // Quadratic spacing resolves nearby drifting banks before spending samples far away.
        let a=f32(i)/f32(count);let b=f32(i+1u)/f32(count);
        let lo=begin+(end-begin)*a*a;let hi=begin+(end-begin)*b*b;
        let midpoint=0.5*(lo+hi);let tau=sigma*weather_relative_density(origin+ray*midpoint,offset,snow)*(hi-lo);
        depth+=vec2(tau,tau*midpoint);
    }
    return depth;
}

// Integrate each exterior segment, including the portion after a ray leaves a real port/hatch.
// No camera/station switch can clear outside fog, and short interior paths are exactly empty.
fn weather_optical_moment(origin:vec3<f32>,ray:vec3<f32>,limit:f32,layer:vec2<f32>,sigma:f32,snow:f32,offset:vec3<f32>,steps:u32)->vec2<f32> {
    var cursor=0.0;var depth=vec2(0.0);
    loop {
        if(cursor>=limit){break;}
        let segment=cabin_next_span(origin,ray,cursor,limit);
        let exterior_end=select(segment.x,limit,segment.y<=segment.x);
        if(exterior_end>cursor){let moment=weather_unpartitioned_optical_moment(origin+ray*cursor,ray,exterior_end-cursor,layer,sigma,snow,offset,steps);depth+=moment+vec2(0.0,moment.x*cursor);}
        if(segment.y<=segment.x){break;}
        cursor=segment.y+CABIN_RAY_EPSILON;
    }
    return depth;
}

fn weather_optical_depth(origin:vec3<f32>,ray:vec3<f32>,limit:f32,layer:vec2<f32>,sigma:f32,snow:f32,offset:vec3<f32>,steps:u32)->f32 {
    return weather_optical_moment(origin,ray,limit,layer,sigma,snow,offset,steps).x;
}

// Point-source transport needs exterior weather AND interior extinction along the same ray.
// Share the exact cavity interval walk, retaining the existing exterior integration endpoints,
// per-segment sample positions/count, summation order and cabin endpoint epsilon. The two optical
// depths stay separate so the caller retains the same exp(-world-cabin) evaluation order.
fn weather_and_cabin_optical_depth(origin:vec3<f32>,ray:vec3<f32>,limit:f32,layer:vec2<f32>,sigma:f32,snow:f32,offset:vec3<f32>,steps:u32,cabin_sigma:f32)->vec2<f32> {
    if(sigma<=0.0 && cabin_sigma<=0.0){return vec2(0.0);}
    var cursor=0.0;var depth=vec2(0.0);var interior_length=0.0;
    loop {
        if(cursor>=limit){break;}
        let segment=cabin_next_span(origin,ray,cursor,limit);
        let exterior_end=select(segment.x,limit,segment.y<=segment.x);
        if(sigma>0.0 && exterior_end>cursor){let moment=weather_unpartitioned_optical_moment(origin+ray*cursor,ray,exterior_end-cursor,layer,sigma,snow,offset,steps);depth+=moment+vec2(0.0,moment.x*cursor);}
        if(segment.y<=segment.x){break;}
        interior_length+=segment.y-segment.x;
        cursor=segment.y+CABIN_RAY_EPSILON;
    }
    return vec2(depth.x,max(cabin_sigma,0.0)*min(interior_length,max(limit,0.0)));
}
