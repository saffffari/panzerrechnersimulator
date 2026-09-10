// Bounded deterministic raster volumes. Existing Smoke ABI, depth terminates every local march.
// World fog is analytic; eight midpoint samples per intersecting volume cover compact muzzle/breech/cabin media.
// Output is premultiplied HDR for source-over blending. No acceleration structure or history.
struct Puff {
    pos: vec3<f32>,   // world-space emitter (muzzle) position
    age: f32,         // seconds since this plume was born
    dir: vec3<f32>,   // gun forward — the plume drifts a little along this and lofts up
    strength: f32,    // 0 = inactive slot
};

struct Smoke {
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    fog_density: f32,     // homogeneous world-medium extinction coefficient (m^-1)
    sun_dir: vec3<f32>,   // TO the sun (matches mesh.wgsl / the shadow cascade)
    frame: f32,
    puffs: array<Puff, 12>,
    breech_gas: Puff,     // small, low-density propellant haze at the opened breech
    n_lights: f32,        // active positioned point lights (0..32), same set the mesh pass gets
    night: f32,           // 0 = day, 1 = night
    overcast: f32,        // same direct-sun cloud attenuation used by combine.wgsl
    world_max_distance: f32,
    // Interleaved [px,py,pz,source_radius] / [r,g,b,radiant_intensity] per light. N_LIGHTS=32.
    lights: array<vec4<f32>, 64>,
    interior: f32,        // base haze density scale
    dust: f32,            // dust-mote sparkle amount
    world_snow_fraction: f32, // 0 generic aerosol, 1 highly scattering blowing snow
    _pad1: f32,
    atmo_origin: vec3<f32>, // tank hull origin — dust is sampled relative to this (rides the hull)
    atmo_time: f32,         // simulation time — identical for every split view in this frame
    fog_layer: vec4<f32>,   // bottom_y, top_y; explicit support of the world aerosol
    transport_steps: vec4<f32>, // world, cabin, plume, secondary local-medium quadrature
};

struct PhysicalEnvironment {
    planet_center_radius: vec4<f32>,
    atmosphere_cloud_bottom: vec4<f32>,
    cloud_top_lwc_radius_coverage: vec4<f32>,
    rayleigh_scatter_ozone_center: vec4<f32>,
    mie_scatter_extinction_g_ozone_half: vec4<f32>,
    ozone_absorption_cloud_albedo: vec4<f32>,
    solar_irradiance_angle: vec4<f32>,
    sun_direction_water_density: vec4<f32>,
    cloud_optics_scales: vec4<f32>,
    integration: vec4<u32>,
};
@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var<uniform> sm: Smoke;
@group(2) @binding(0) var<uniform> environment: PhysicalEnvironment;
const LOCAL_STEPS: i32 = 8;
const PUFF_LIFE: f32 = 1.0;
fn unit_or_up(value: vec3<f32>) -> vec3<f32> {
    return value / max(length(value),0.0001) + select(vec3(0.0),vec3(0.0,1.0,0.0),dot(value,value)<1e-8);
}
fn sphere_interval(origin: vec3<f32>, ray: vec3<f32>, center: vec3<f32>, radius: f32) -> vec2<f32> {
    let delta = origin-center; let projection=dot(delta,ray);
    let discriminant=projection*projection-dot(delta,delta)+radius*radius;
    if (discriminant <= 0.0) { return vec2(1.0,-1.0); }
    let root=sqrt(discriminant); return vec2(-projection-root,-projection+root);
}
fn slab_length(origin: vec3<f32>, ray: vec3<f32>, limit: f32) -> f32 {
    if (sm.fog_layer.y <= sm.fog_layer.x) { return 0.0; }
    if (abs(ray.y)<1e-6) {
        return select(0.0,limit,origin.y>=sm.fog_layer.x && origin.y<=sm.fog_layer.y);
    }
    let a=(sm.fog_layer.x-origin.y)/ray.y; let b=(sm.fog_layer.y-origin.y)/ray.y;
    return max(min(max(a,b),limit)-max(min(a,b),0.0),0.0);
}
fn puff_shape(puff: Puff) -> vec4<f32> {
    let age=max(puff.age,0.0);
    return vec4(puff.pos+puff.dir*(0.55*age)+vec3(0.0,3.4*age,0.0),0.5+1.8*age);
}
fn plume_density(point: vec3<f32>, puff: Puff) -> f32 {
    if (puff.strength<=0.0 || puff.age<0.0 || puff.age>=PUFF_LIFE) { return 0.0; }
    let shape=puff_shape(puff); let q=(point-shape.xyz)/shape.w;
    let radius=length(q); if (radius>=1.0) { return 0.0; }
    let envelope=smoothstep(0.0,0.04,puff.age)*(1.0-smoothstep(0.30,0.85,puff.age));
    // Coherent volume-space lobes move with the plume. No frame hash or bright one-pixel noise.
    let lobes=0.76+0.12*sin(q.x*5.0+q.y*3.0)+0.12*sin(q.z*4.0-q.y*5.0);
    return (1.0-smoothstep(0.15,1.0,radius))*lobes*envelope*clamp(puff.strength,0.0,1.0);
}
fn gas_shape(puff: Puff) -> vec4<f32> {
    let age=max(puff.age,0.0);let axis=unit_or_up(puff.dir);
    return vec4(puff.pos+axis*(0.06+0.20*age)+vec3(0.0,0.04+0.11*age,0.0),0.24+0.16*age);
}
fn gas_density(point: vec3<f32>) -> f32 {
    let puff=sm.breech_gas;
    if (puff.strength<=0.0 || puff.age<0.0 || puff.age>=2.2) { return 0.0; }
    let axis=unit_or_up(puff.dir);let q=point-gas_shape(puff).xyz;
    let along=dot(q,axis);let axial=along/(0.16+0.16*puff.age);
    let radial=length(q-axis*along)/(0.09+0.10*puff.age);
    let shape=sqrt(axial*axial+radial*radial);
    return (1.0-smoothstep(0.18,1.0,shape))*clamp(puff.strength,0.0,0.30)*0.34
        *(1.0-smoothstep(1.2,2.2,puff.age));
}
fn cabin_density(point: vec3<f32>) -> f32 {
    let radius=distance(point,sm.atmo_origin);
    return max(sm.interior,0.0)*0.010*(1.0-smoothstep(2.8,3.35,radius));
}
fn smoke_light(point: vec3<f32>, ray: vec3<f32>) -> vec3<f32> {
    let night=clamp(sm.night,0.0,1.0);
    var color=mix(vec3(0.31,0.33,0.34),vec3(0.065,0.073,0.083),night);
    let sun=unit_or_up(sm.sun_dir);
    color += mix(vec3(0.25,0.24,0.21),vec3(0.018,0.020,0.024),night)
        * (0.35+0.65*max(dot(ray,sun),0.0))*(1.0-clamp(sm.overcast,0.0,1.0)*0.5);
    let count=min(max(i32(sm.n_lights),0),32);
    for (var index=0; index<count; index+=1) {
        let source=sm.lights[index*2];let emission=sm.lights[index*2+1];
        let delta=source.xyz-point;
        let falloff=min(max(emission.w,0.0)/max(dot(delta,delta),max(source.w*source.w,0.04)),4.0);
        color += emission.rgb*falloff*0.08;
    }
    return color;
}
// Each support is integrated separately: a distant enemy plume cannot stretch one shared
// interval so far that all samples skip the player's small breech-gas volume.
fn volume_depth(origin: vec3<f32>, ray: vec3<f32>, interval: vec2<f32>, scene_distance: f32,
                kind: i32) -> vec2<f32> {
    let enter=max(interval.x,0.03); let leave=min(interval.y,scene_distance);
    if (leave<=enter) { return vec2(0.0); }
    let step=(leave-enter)/f32(LOCAL_STEPS); var tau=0.0;
    for (var sample=0;sample<LOCAL_STEPS;sample+=1) {
        let point=origin+ray*(enter+(f32(sample)+0.5)*step);
        var density=0.0;
        if (kind<12) { density=plume_density(point,sm.puffs[kind]); }
        else if (kind==12) { density=gas_density(point); }
        else { density=cabin_density(point); }
        tau+=max(density,0.0)*5.2*step;
    }
    return vec2(tau,tau*(enter+leave)*0.5);
}
struct VsOut { @builtin(position) pos: vec4<f32>, };
@vertex
fn vs(@builtin(vertex_index) index: u32) -> VsOut {
    var positions=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    var out: VsOut;out.pos=vec4(positions[index],0.0,1.0);return out;
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims=vec2<f32>(textureDimensions(depth_tex));
    let uv=in.pos.xy/dims;let ndc=vec2(uv.x*2.0-1.0,1.0-uv.y*2.0);
    let near_h=sm.inv_view_proj*vec4(ndc,0.0,1.0);
    let far_h=sm.inv_view_proj*vec4(ndc,1.0,1.0);
    let ray=normalize(far_h.xyz/far_h.w-near_h.xyz/near_h.w);
    let origin=sm.cam_pos;
    let depth=textureLoad(depth_tex,vec2<i32>(in.pos.xy),0);
    var scene_distance=max(sm.world_max_distance,1.0);
    if (depth<1.0) {
        let surface=sm.inv_view_proj*vec4(ndc,depth,1.0);
        scene_distance=max(length(surface.xyz/surface.w),0.0);
    }
    let fog_distance=min(scene_distance,max(sm.world_max_distance,0.0));
    let fog_transmittance=exp(-max(sm.fog_density,0.0)*slab_length(origin,ray,fog_distance));
    let cloud=clamp(max(sm.overcast,environment.cloud_top_lwc_radius_coverage.w*0.5),0.0,1.0);
    let fog_color=mix(vec3(0.45,0.52,0.56),vec3(0.52,0.55,0.56),cloud)
        * mix(vec3(1.0),vec3(0.075,0.088,0.11),clamp(sm.night,0.0,1.0));

    var depth_moment=vec2(0.0);
    for (var index=0;index<12;index+=1) {
        let puff=sm.puffs[index];
        if (puff.strength<=0.0 || puff.age<0.0 || puff.age>=PUFF_LIFE) { continue; }
        let shape=puff_shape(puff);
        depth_moment+=volume_depth(origin,ray,sphere_interval(origin,ray,shape.xyz,shape.w),scene_distance,index);
    }
    if (sm.breech_gas.strength>0.0 && sm.breech_gas.age<2.2) {
        let shape=gas_shape(sm.breech_gas);
        depth_moment+=volume_depth(origin,ray,sphere_interval(origin,ray,shape.xyz,shape.w),scene_distance,12);
    }
    if (sm.interior>0.001) {
        depth_moment+=volume_depth(origin,ray,sphere_interval(origin,ray,sm.atmo_origin,3.35),scene_distance,13);
    }
    let transmittance=exp(-depth_moment.x);
    var radiance=vec3(0.0);
    if (depth_moment.x>0.00001) {
        let center=origin+ray*(depth_moment.y/depth_moment.x);
        radiance=smoke_light(center,ray)*(1.0-transmittance);
    }
    radiance+=transmittance*fog_color*(1.0-fog_transmittance);
    return vec4(max(radiance,vec3(0.0)),clamp(1.0-transmittance*fog_transmittance,0.0,1.0));
}
