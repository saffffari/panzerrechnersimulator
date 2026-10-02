// @scene-visibility-smoke
// @scene-light-visibility
// @cabin-medium
// @weather-density
// Bounded deterministic raster volumes. Existing Smoke ABI, depth terminates every local march.
// World fog is analytic; eight midpoint samples per intersecting volume cover compact muzzle/breech/cabin media.
// Output is premultiplied HDR for source-over blending. No acceleration structure or history.
struct Puff {
    pos: vec3<f32>,   // world-space emitter (muzzle) position
    age: f32,         // seconds since this plume was born
    dir: vec3<f32>,   // gun forward — the plume drifts a little along this and lofts up
    strength: f32,    // 0 = inactive slot
    shape: vec4<f32>, // explicit radius / density / rolling phase (radius zero uses legacy puff)
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
    storm: f32,           // storm veil 0..1, as in the camera atmosphere
    atmo_origin: vec3<f32>, // tank hull origin — dust is sampled relative to this (rides the hull)
    atmo_time: f32,         // simulation time — identical for every split view in this frame
    fog_layer: vec4<f32>,   // bottom_y, top_y; explicit support of the world aerosol
    transport_steps: vec4<f32>, // world, cabin, plume, secondary local-medium quadrature
    weather_motion: vec4<f32>, // shared wind displacement and weather time
    cabin_medium: CabinMedium,
};

@group(2) @binding(1) var cabin_air_columns:texture_2d_array<f32>;
fn cabin_transform(i:u32)->mat4x4<f32> {return sm.cabin_medium.world_to_local[i];}
fn cabin_grid(i:u32)->vec4<f32> {return sm.cabin_medium.grid_min_step[i];}
fn cabin_size(i:u32)->vec4<u32> {return sm.cabin_medium.grid_size[i];}
fn cabin_min(i:u32)->vec3<f32> {return sm.cabin_medium.bounds_min[i].xyz;}
fn cabin_max(i:u32)->vec3<f32> {return sm.cabin_medium.bounds_max[i].xyz;}

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
    secondary_irradiance_angle: vec4<f32>,
    secondary_direction: vec4<f32>,
    airglow_zenith_altitude: vec4<f32>,
    cloud_motion: vec4<f32>,
    star_frame: vec4<f32>,
};
@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var<uniform> sm: Smoke;
// Near-field environment visibility (src/sky_visibility.rs), shared with the surface pass.
struct SkyVisibility {
    view_proj:array<mat4x4<f32>,66>,
    directions:array<vec4<f32>,32>,
    grid:vec4<f32>,
    global_grid:vec4<f32>, // coarse world texel, near depth span, global depth span, reserved
    sun_direction:vec4<f32>,
};
@group(1) @binding(0) var sky_visibility_map: texture_depth_2d;
@group(1) @binding(1) var sky_visibility_compare: sampler_comparison;
@group(1) @binding(2) var<uniform> sky_visibility: SkyVisibility;
// The physical environment probe (src/environment_probe.rs): lane 32 the key body's beam
// irradiance at the camera, 34/35 the mean sky and ground radiance, in renderer units.
struct EnvironmentProbe { lanes: array<vec4<f32>, 68> };
@group(1) @binding(3) var<uniform> probe: EnvironmentProbe;
@group(2) @binding(0) var<uniform> environment: PhysicalEnvironment;
// @sky-visibility-sampling
@group(3) @binding(0) var cabin_shadow_maps: texture_depth_cube_array;
@group(3) @binding(1) var shadow_samp: sampler_comparison;
// @cabin-visibility-sampling
@group(3) @binding(2) var<uniform> cabin_shadows: CabinShadows;
const LOCAL_STEPS: i32 = 8;
// combine.wgsl's world-medium optics (tests keep the three copies identical).
const FOG_AEROSOL_PHASE_G = 0.42;
const FOG_SNOW_PHASE_G = 0.76;
const FOG_AEROSOL_SINGLE_SCATTER_ALBEDO = 0.88;
const FOG_SNOW_SINGLE_SCATTER_ALBEDO = 0.995;
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

// Advected finite 3D matter. Smooth radial support and coherent erosion remove hard cell faces.
fn smoke_billow_density(point:vec3<f32>,puff:Puff)->f32 {
    // dir is the half-axis of a sustained source sweep (zero for an ordinary sphere).
    // Pull back through the affine stretch; CPU density carries the inverse Jacobian.
    let delta=point-puff.pos;
    let axis_length=length(puff.dir);
    let axis=puff.dir/max(axis_length,0.000001);
    let stretch=1.0+axis_length/max(puff.shape.x,0.02);
    let q=(delta-axis*dot(delta,axis)*(1.0-1.0/stretch))/max(puff.shape.x,0.02);
    let radius=length(q); if(radius>=1.0 || puff.strength<=0.0){return 0.0;}
    let phase=puff.shape.z;
    let warp=vec3(sin(q.y*4.1+phase),sin(q.z*3.7-phase*0.8),sin(q.x*4.7+phase*0.6))*0.23;
    let noise=weather_noise(q*3.2+warp+vec3(0.0,-phase*0.35,0.0));
    let fine=weather_noise(q*7.1+warp+vec3(7.3,phase*0.2,4.1));
    let envelope=1.0-smoothstep(0.12,1.0,radius);
    return puff.shape.y*envelope*smoothstep(0.15,0.75,noise*0.75+fine*0.25);
}

fn puff_shape(puff: Puff) -> vec4<f32> {
    if (puff.shape.x>0.0) {return vec4(puff.pos,puff.shape.x+length(puff.dir));}
    let age=max(puff.age,0.0);
    return vec4(puff.pos+puff.dir*(0.55*age)+vec3(0.0,3.4*age,0.0),0.5+1.8*age);
}
fn plume_density(point: vec3<f32>, puff: Puff) -> f32 {
    if (puff.shape.x>0.0) {return smoke_billow_density(point,puff);}
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
    return select(0.0,max(sm.interior,0.0)*0.010,cabin_contains(point));
}
fn visible_fraction(total: f32, visible: f32) -> f32 {
    return select(1.0, visible / total, total > 1e-6);
}
fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let denominator = pow(max(1.0 + g * g - 2.0 * g * clamp(cos_theta, -1.0, 1.0), 1e-6), 1.5);
    return (1.0 - g * g) / (4.0 * 3.14159265 * denominator);
}
// Environment radiance scattered toward the camera: the probe's 32 directions weighted by the
// Henyey-Greenstein phase about the view ray (a normalised quadrature of the phase integral), each
// hemisphere scaled by the share of it that reaches the medium.
fn phase_weighted_environment(point:vec3<f32>, ray: vec3<f32>, g: f32) -> vec3<f32> {
    var sum = vec3(0.0);
    var weight = 0.0;
    for (var k = 0; k < 32; k += 1) {
        let d = probe.lanes[36 + k];
        let w = henyey_greenstein(dot(d.xyz, ray), g) * d.w;
        weight += w;
        sum += scene_environment_radiance(point,d.xyz,k) * w;
    }
    return sum / max(weight, 1e-6);
}
// Phase-weighted incident radiance of the gun smoke, breech gas and cabin haze: the physical
// environment through the openings, the key body's beam and every positioned emitter, each with
// the aerosol's Henyey-Greenstein phase; single scattering at the haze albedo of smoke.wgsl (0.70).
fn smoke_light(point: vec3<f32>, ray: vec3<f32>) -> vec3<f32> {
    let sun=unit_or_up(sm.sun_dir);
    var incident=phase_weighted_environment(point,ray,0.42);
    if(any(probe.lanes[32].xyz!=vec3(0.0))){
        incident+=probe.lanes[32].xyz*henyey_greenstein(dot(ray,sun),0.42)*scene_environment_transmission(point,sun);
    }
    let count=min(max(i32(sm.n_lights),0),32);
    for (var index=0; index<count; index+=1) {
        let source=sm.lights[index*2];let emission=sm.lights[index*2+1];
        if(emission.w<=0.0 || all(emission.rgb==vec3(0.0))){continue;}
        let delta=source.xyz-point;
        let d2=max(dot(delta,delta),max(source.w*source.w,0.0025));
        let visibility=cabin_volume_light_shadow(source.xyz,source.w,point);if(all(visibility<=vec3(0.0))){continue;}
        let direction=delta*inverseSqrt(d2);let range=max(sqrt(d2)-source.w,0.0);
        let tau=weather_optical_depth(point,direction,range,sm.fog_layer.xy,sm.fog_density,sm.world_snow_fraction,sm.weather_motion.xyz,u32(sm.transport_steps.x))
            +max(sm.interior,0.0)*0.010*5.2*cabin_path_length(point,direction,range);
        incident += emission.rgb*max(emission.w,0.0)/d2
            *henyey_greenstein(dot(ray,direction),0.42)*visibility*exp(-tau);
    }
    return 0.70*incident;
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
// Native-pixel stochastic coverage is a presentation quadrature of the existing local
// medium. Adjacent opacity levels preserve expected transmittance AND scattered radiance;
// no extra smoke is added. The threshold has no time/frame input, so static media do not boil.
// World fog and baseline cabin haze keep their continuous transport. Secondary attenuation
// continues to integrate the unquantized physical density.
fn plume_pixel_threshold(pixel:vec2<f32>)->f32 {
    let p=vec2<u32>(max(floor(pixel),vec2(0.0)));
    var h=p.x*0x9e3779b9u ^ p.y*0x85ebca6bu ^ 0x51ed19a1u;
    h=(h^(h>>16u))*0x7feb352du;h=(h^(h>>15u))*0x846ca68bu;h=h^(h>>16u);
    return (f32(h>>8u)+0.5)/16777216.0;
}
fn plume_pixel_alpha(tau:f32,plume_tau:f32,pixel:vec2<f32>)->f32 {
    let alpha=1.0-exp(-max(tau,0.0));
    if(plume_tau<=0.000001 || tau<=0.000001){return alpha;}
    let quantum=0.25*clamp(plume_tau/tau,0.0,1.0);
    let lower=floor(alpha/quantum)*quantum;
    let upper=min(lower+quantum,1.0);
    let probability=(alpha-lower)/max(upper-lower,0.000001);
    return select(lower,upper,plume_pixel_threshold(pixel)<probability);
}
struct VsOut { @builtin(position) pos: vec4<f32>, };
@vertex
fn vs(@builtin(vertex_index) index: u32) -> VsOut {
    var positions=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    var out: VsOut;out.pos=vec4(positions[index],0.0,1.0);return out;
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims=select(vec2<f32>(textureDimensions(depth_tex)),sm.fog_layer.zw,all(sm.fog_layer.zw>vec2(0.0)));
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
    let fog_moment=weather_optical_moment(origin,ray,fog_distance,sm.fog_layer.xy,max(sm.fog_density,0.0),sm.world_snow_fraction,sm.weather_motion.xyz,u32(sm.transport_steps.x));
    let fog_transmittance=exp(-fog_moment.x);
    // The world medium's single-scattering source: its albedo times the physical environment
    // (the probe's directions, phase-weighted) and the key body's beam through its phase,
    // with the aerosol/blowing-snow optics of smoke.wgsl. An optically thick layer converges to
    // this source, i.e. to the light it is immersed in.
    let snow=clamp(sm.world_snow_fraction,0.0,1.0);
    let fog_albedo=mix(FOG_AEROSOL_SINGLE_SCATTER_ALBEDO,FOG_SNOW_SINGLE_SCATTER_ALBEDO,snow);
    let fog_g=mix(FOG_AEROSOL_PHASE_G,FOG_SNOW_PHASE_G,snow);
    // Exact source visibility at the exterior density's optical-depth centroid. Zero-density
    // segments have no scattering connection to evaluate (and cannot create optical diagnostics).
    var fog_color=vec3(0.0);
    if(fog_moment.x>0.0){
        let fog_point=origin+ray*(fog_moment.y/max(fog_moment.x,1e-8));
        fog_color=fog_albedo*phase_weighted_environment(fog_point,ray,fog_g);
        if(any(probe.lanes[32].xyz!=vec3(0.0))){
            fog_color+=fog_albedo*probe.lanes[32].xyz*henyey_greenstein(dot(ray,unit_or_up(sm.sun_dir)),fog_g)*scene_environment_transmission(fog_point,unit_or_up(sm.sun_dir));
        }
    }

    var depth_moment=vec2(0.0);
    var scattering_depth=0.0;
    for (var index=0;index<12;index+=1) {
        let puff=sm.puffs[index];
        if (puff.strength<=0.0 || puff.age<0.0 || puff.age>=PUFF_LIFE) { continue; }
        let shape=puff_shape(puff);
        let moment=volume_depth(origin,ray,sphere_interval(origin,ray,shape.xyz,shape.w),scene_distance,index);
        depth_moment+=moment;
        scattering_depth+=moment.x*select(0.70,puff.shape.w,puff.shape.x>0.0);
    }
    let plume_depth=depth_moment.x;
    if (sm.breech_gas.strength>0.0 && sm.breech_gas.age<2.2) {
        let shape=gas_shape(sm.breech_gas);
        let moment=volume_depth(origin,ray,sphere_interval(origin,ray,shape.xyz,shape.w),scene_distance,12);
        depth_moment+=moment;scattering_depth+=moment.x*0.70;
    }
    if (sm.interior>0.001) {
        var cabin_cursor=0.0;
        loop {
            if(cabin_cursor>=scene_distance){break;}
            let interval=cabin_next_span(origin,ray,cabin_cursor,scene_distance);
            if(interval.y<=interval.x){break;}
            let tau=(interval.y-interval.x)*max(sm.interior,0.0)*0.010*5.2;
            depth_moment+=vec2(tau,tau*(interval.x+interval.y)*0.5);
            scattering_depth+=tau*0.70;
            cabin_cursor=interval.y+CABIN_RAY_EPSILON;
        }
    }
    let local_alpha=plume_pixel_alpha(depth_moment.x,plume_depth,in.pos.xy);
    let transmittance=1.0-local_alpha;
    var radiance=vec3(0.0);
    if (depth_moment.x>0.00001) {
        let center=origin+ray*(depth_moment.y/depth_moment.x);
        radiance=smoke_light(center,ray)*(1.0-transmittance)*scattering_depth/(0.70*depth_moment.x);
    }
    radiance+=transmittance*fog_color*(1.0-fog_transmittance);
    if(scene_visibility_unresolved){return vec4(1000.0,0.0,1000.0,1.0);}
    return vec4(max(radiance,vec3(0.0)),clamp(1.0-transmittance*fog_transmittance,0.0,1.0));
}
