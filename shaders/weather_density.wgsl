// Fictional theatre climate boundary in world metres. CPU parity: weather_visibility.rs.
// No observer coordinate enters this field. Zero flag preserves the legacy act exactly.
fn theatre_north_weight(z:f32,theatre:f32)->f32 {
    if(theatre<0.5){return 1.0;}
    return smoothstep(600.0,2600.0,-z);
}
fn theatre_extinction(sigma:f32,p:vec3<f32>,theatre:f32)->f32 {
    if(theatre<0.5){return sigma;}
    return mix(0.00012,sigma,theatre_north_weight(p.z,theatre));
}
fn theatre_ground_mood(mood:f32,p:vec3<f32>,theatre:f32)->f32 {
    if(theatre<0.5){return mood;}
    return 0.75*(1.0-theatre_north_weight(p.z,theatre));
}
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
fn weather_relative_density(p:vec3<f32>,offset:vec3<f32>,snow:f32,theatre:f32)->f32 {
    let q=p-offset;
    let broad=weather_noise(q/vec3(110.0,42.0,110.0));
    let eddies=weather_noise(q/vec3(27.0,13.0,27.0)+vec3(19.7,3.1,-6.3));
    let banks=0.22+2.15*smoothstep(0.18,0.82,broad*0.72+eddies*0.28);
    return mix(1.0,banks,clamp(snow*theatre_north_weight(p.z,theatre),0.0,1.0));
}
