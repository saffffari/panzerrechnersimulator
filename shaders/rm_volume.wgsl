// Raymarch backend froxel lighting. One thread per froxel: evaluate the media there (weather,
// cabin haze, plumes, binned fire/soot/smoke/fireball/glow parcels), march toward the key beam and
// the shadowed lamps through that same medium, gather sky and the clustered lamps' in-scatter,
// add emission, and blend with the reprojected previous frame. Output: in-scattered radiance per
// metre (rgb) and extinction per metre (a). rm_integrate.wgsl turns this into a per-slice table.
struct Parcel { a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, };
// a: position xyz, radius m   b: colour rgb, kind (0 fire, 1 soot, 2 smoke, 3 glow, 4 segment, 5 fireball,
// 6 muzzle flash, 7 exhaust: smoke whose c.w is its excess heat 0..1, 8 a baked explosion volume:
// a.w its bounding radius, c.x emission and c.y extinction per metre at full value, c.z its atlas
// slot, c.w its yaw, d.xyz its half extents, d.w its flame's temperature 0..1)
// c: emission per m, extinction per m, noise seed, fireball age 0..1   d: segment end xyz
@group(0) @binding(1) var<storage, read> parcels: array<Parcel>;
@group(0) @binding(2) var<storage, read> cell_start: array<u32>;
@group(0) @binding(3) var<storage, read> cell_items: array<u32>;
@group(0) @binding(4) var history: texture_3d<f32>;
@group(0) @binding(5) var linear_samp: sampler;
@group(0) @binding(6) var out_scatter: texture_storage_3d<rgba16float, write>;
@group(0) @binding(7) var depth_tex: texture_depth_2d;
@group(0) @binding(8) var out_heat: texture_storage_3d<rgba16float, write>;
// The baked explosion volumes (explosion_volumes.rs): (density, flame), 64^3 slots stacked in depth.
@group(0) @binding(9) var explosion_atlas: texture_3d<f32>;

// Density and flame of atlas slot `slot` at `local` in [-1, 1]^3 (y up); the depth is clamped to
// the slot's own texel centres so neighbouring slots never bleed into each other.
fn explosion_sample(local: vec3<f32>, slot: u32) -> vec2<f32> {
    let side = f32(textureDimensions(explosion_atlas).x);
    let slots = f32(textureDimensions(explosion_atlas).z) / side;
    let uvw = local * 0.5 + vec3(0.5);
    let z = (f32(slot) + clamp(uvw.z, 0.5 / side, 1.0 - 0.5 / side)) / max(slots, 1.0);
    return textureSampleLevel(explosion_atlas, linear_samp, vec3(uvw.x, uvw.y, z), 0.0).rg;
}

const SUN_MARCH_STEPS: u32 = 6u;
const LAMP_MARCH_STEPS: u32 = 4u;

fn parcel_medium(p: vec3<f32>, cell: u32, flame_noise: f32, m_in: Medium) -> Medium {
    var m = m_in;
    // Flames accumulate into a temperature field: dense overlapping parcels become a white-hot
    // core, thin fringes cool to deep red. The emission is then a function of that field, so
    // neighbouring parcels merge into one body instead of reading as separate balls.
    var fire_heat = 0.0;
    var fire_emission = 0.0;
    let start = cell_start[cell];
    let end = cell_start[cell + 1u];
    for (var i = start; i < end; i += 1u) {
        let parcel = parcels[cell_items[i]];
        let kind = u32(parcel.b.w);
        var w = 0.0;
        var colour = parcel.b.rgb;
        if (kind == 8u) {
            // A baked explosion volume: its shape and its fire come from the atlas, not a ball.
            let rel = p - parcel.a.xyz;
            let cy = cos(parcel.c.w);
            let sy = sin(parcel.c.w);
            let local = vec3(cy * rel.x - sy * rel.z, rel.y, sy * rel.x + cy * rel.z) / max(parcel.d.xyz, vec3(0.01));
            if (any(abs(local) >= vec3(1.0))) { continue; }
            // The slot's faint floor (resampling haze) and its side and top faces fall to nothing,
            // so the box never shows; the base stands on the ground.
            let window = (1.0 - smoothstep(0.8, 1.0, abs(local.x))) * (1.0 - smoothstep(0.8, 1.0, abs(local.z)))
                * (1.0 - smoothstep(0.8, 1.0, local.y));
            let s = max(explosion_sample(local, u32(parcel.c.z + 0.5)) - vec2(0.03), vec2(0.0)) / 0.97 * window;
            let ext = parcel.c.y * s.x;
            m.sigma_t += ext;
            m.sigma_s += parcel.b.rgb * ext * 0.75;
            // Flames: their own grid, at the burst's temperature (hotter where the grid is dense).
            let flame = s.y * parcel.d.w;
            if (flame > 0.0) {
                m.emission += rm_heat_colour(clamp(0.3 + 0.6 * flame, 0.0, 1.0)) * parcel.c.x * flame;
                m.heat += flame * 1.5;
                m.sigma_t += parcel.c.y * 0.25 * flame;
            }
            continue;
        }
        if (kind == 4u) {
            // Tracer: glow around a segment, radius in a.w.
            let ab = parcel.d.xyz - parcel.a.xyz;
            let t = clamp(dot(p - parcel.a.xyz, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
            let dist = distance(p, parcel.a.xyz + ab * t) / max(parcel.a.w, 0.01);
            if (dist >= 1.0) { continue; }
            w = (1.0 - dist) * (1.0 - dist);
        } else {
            let q = (p - parcel.a.xyz) / max(parcel.a.w, 0.005);
            let dd = dot(q, q);
            if (dd >= 1.0) { continue; }
            if (kind == 0u) {
                // Flames: hot gas pockets whose brightness breaks into tongues with the shared
                // noise field and thins toward the edge; never a frame-hashed sparkle.
                let tongue = smoothstep(0.25, 0.95, flame_noise + 0.45 * (1.0 - dd));
                let body = (1.0 - dd) * (1.0 - dd);
                fire_heat += body * (0.6 + 1.2 * tongue);
                fire_emission += parcel.c.x * body * tongue * 2.2;
                m.heat += body * tongue * min(parcel.c.x, 2.0) * 0.35;
                let ext = parcel.c.y * body * tongue;
                m.sigma_t += ext;
                m.sigma_s += vec3(ext * 0.25);
                continue;
            } else if (kind == 5u) {
                // Fireball: turbulent shell, hottest at the core, breaking up as it cools.
                let age = parcel.c.w;
                let shell = rm_fbm(q * (2.5 + 2.0 * age) + vec3(parcel.c.z, -fr.cam_pos.w * 1.5, parcel.c.z * 0.7), 71u);
                let edge = smoothstep(0.15 + 0.55 * age, 0.95, shell + (1.0 - dd) * 0.8);
                w = edge * (0.4 + 0.6 * (1.0 - dd));
                colour = mix(rm_heat_colour(0.5), rm_heat_colour(0.84), (1.0 - dd) * (1.0 - age));
                m.heat += w * 1.5 * (1.0 - 0.5 * age);
            } else if (kind == 1u || kind == 2u) {
                w = (1.0 - dd) * (1.0 - dd) * (0.55 + 0.9 * flame_noise);
            } else if (kind == 7u) {
                // Exhaust: a billowing puff (a softer, larger-scale breakup than the flames'), its
                // heat shimmering the air while young.
                let billow = rm_noise(q * 1.8 + vec3(parcel.c.z, parcel.c.z * 0.7, 0.0), 89u);
                w = (1.0 - dd) * (1.0 - dd) * (0.45 + 0.9 * billow);
                m.heat += w * parcel.c.w * 0.5;
            } else if (kind == 6u) {
                // Muzzle flash: a prolate lobe along the bore (d = direction, c.w = radius/length),
                // brightest just ahead of the muzzle, torn by the shared noise.
                let axis = parcel.d.xyz;
                let along = dot(p - parcel.a.xyz, axis) / max(parcel.a.w, 0.01);
                let radial = length((p - parcel.a.xyz) - axis * dot(p - parcel.a.xyz, axis)) / max(parcel.a.w * parcel.c.w, 0.01);
                let lobe = along * along * 4.0 + radial * radial;
                if (lobe >= 1.0) { continue; }
                let torn = 0.5 + 0.5 * rm_noise(p * 6.0 + vec3(parcel.c.z), 83u);
                w = (1.0 - lobe) * (1.0 - lobe) * torn * (1.3 - 0.8 * max(along + 0.5, 0.0));
                colour = mix(rm_heat_colour(0.7), rm_heat_colour(1.0), (1.0 - lobe));
            } else {
                w = (1.0 - dd) * (1.0 - dd);
            }
        }
        m.emission += colour * parcel.c.x * w;
        let ext = parcel.c.y * w;
        m.sigma_t += ext;
        if (kind == 1u) { m.sigma_s += vec3(ext * 0.12); }
        else if (kind == 2u || kind == 7u) { m.sigma_s += parcel.b.rgb * ext * 0.75; }
        else if (kind == 5u || kind == 6u) { m.sigma_s += vec3(ext * 0.25); }
    }
    if (fire_emission > 0.0) {
        // The gun's report: its flash grains leave through the muzzle brake's lobes.
        let lobes = rm_brake_lobes(p);
        fire_emission *= lobes;
        fire_heat *= lobes;
        // Temperature from the local density: 0.3 (a lone lick, deep orange-red) to 1 (core).
        let temperature = clamp(0.18 + fire_heat * 0.12, 0.0, 0.72);
        m.emission += rm_heat_colour(temperature) * fire_emission * (0.5 + 0.7 * temperature) * 0.5;
    }
    return m;
}

@compute @workgroup_size(4, 4, 4)
fn cs(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = vec3<u32>(fr.vol.xyz);
    if (any(id >= dims)) { return; }
    let frame = u32(fr.vol.w);
    let seed = (id.x * 1973u + id.y * 9277u + id.z * 26699u) ^ (frame * 7919u);
    let jitter = vec3(rm_hash01(seed), rm_hash01(seed + 1u), rm_hash01(seed + 2u));
    let uv = (vec2<f32>(id.xy) + vec2(0.5) + (jitter.xy - vec2(0.5)) * 0.75) / fr.vol.xy;
    let kf = f32(id.z) + jitter.z;
    let d = rm_dist(kf);
    let ray = rm_ray(uv);
    let p = fr.cam_pos.xyz + ray * d;

    // Froxels behind the rendered surface at this pixel carry no visible in-scatter.
    let pixel = vec2<i32>(clamp(uv * fr.res.xy, vec2(0.0), fr.res.xy - vec2(1.0)));
    let depth = textureLoad(depth_tex, pixel, 0);
    let scene_d = select(1.0e9, length(rm_relative_from_depth(uv, depth)), depth < 1.0);
    let slice_width = rm_dist(kf + 1.0) - d;
    var m = rm_medium_at(p);
    let cabin_domain = rm_cabin_domain(p);
    let cell = vec3<u32>(id * RM_CELLS / dims);
    let cell_index = (cell.z * RM_CELLS.y + cell.y) * RM_CELLS.x + cell.x;
    let flame_noise = rm_fbm(p * vec3(9.0, 6.0, 9.0) + vec3(0.0, -fr.cam_pos.w * 2.6, 0.0), 41u);
    m = parcel_medium(p, cell_index, flame_noise, m);
    if (d > scene_d + slice_width) {
        textureStore(out_scatter, vec3<i32>(id), vec4(0.0, 0.0, 0.0, m.sigma_t));
        textureStore(out_heat, vec3<i32>(id), vec4(0.0));
        return;
    }
    textureStore(out_heat, vec3<i32>(id), vec4(m.heat, 0.0, 0.0, 0.0));

    var inscatter = m.emission;
    if (any(m.sigma_s > vec3(0.0))) {
        // Key beam: geometric visibility from the cascades, then the medium between here and
        // open sky marched through the same density field.
        if (fr.sun_radiance.w > 0.5 && fr.sun_dir.y > -0.05) {
            let vis = rm_sun_visibility(p, 0.0);
            if (vis > 0.001) {
                let reach = clamp((fr.medium.w - p.y) / max(fr.sun_dir.y, 0.08), 4.0, 60.0);
                let tau = rm_march_tau(p, fr.sun_dir.xyz, reach, SUN_MARCH_STEPS, jitter.x);
                inscatter += m.sigma_s * fr.sun_radiance.rgb * vis * exp(-tau)
                    * rm_phase_dense(dot(ray, fr.sun_dir.xyz), m.g, m.sigma_t);
            }
        }
        // Skylight: isotropic mean sky through the vertical cover map. Covered open air keeps 8 %
        // (the sky low on the horizon under eaves and trees); the cabin air only what the map
        // sees through an open hatch above it (light through the vision blocks is left out).
        let sky_vis = rm_sky_visibility(p, 0.0);
        inscatter += m.sigma_s * rm_sky_isotropic() * mix(select(0.08, 0.0, cabin_domain >= 0), 1.0, sky_vis);
        // Lamps, fires, flashes, tracer heads: the lights binned into this cell.
        let lstart = light_cell_start[cell_index];
        let lend = light_cell_start[cell_index + 1u];
        for (var i = lstart; i < lend; i += 1u) {
            let index = light_cell_items[i];
            let s = rm_light_sample(index, p);
            if (max(s.e.r, max(s.e.g, s.e.b)) <= 0.0) { continue; }
            let shadowed = lights[index].c.y >= 0.0;
            var vis = 1.0;
            var tau = 0.0;
            if (shadowed) {
                vis = rm_light_visibility(index, p, 0.02);
                if (vis <= 0.001) { continue; }
                tau = rm_march_tau(p, s.dir, s.dist, LAMP_MARCH_STEPS, jitter.y);
            } else {
                tau = m.sigma_t * min(s.dist, 12.0);
            }
            var shape = 1.0;
            // The report's own light leaves through the brake with the gas.
            if (fr.report.w >= 0.0 && fr.report.w < 0.22 && distance(lights[index].a.xyz, fr.report.xyz) < 1.5) { shape = rm_brake_lobes(p); }
            inscatter += m.sigma_s * s.e * vis * exp(-tau) * rm_phase_dense(dot(ray, s.dir), m.g, m.sigma_t) * shape;
        }
    }

    // Temporal history: reproject this froxel's world point into the previous frame's grid.
    var result = vec4(inscatter, m.sigma_t);
    if (fr.prev_cam_pos.w > 0.5) {
        // Cabin air rides its rigid frame: last frame it was where that frame's motion puts it,
        // so the history of the lit cabin follows the vehicle instead of smearing through it.
        var prev_p = p;
        if (cabin_domain >= 0) { prev_p = (fr.cabin_motion[u32(cabin_domain)] * vec4(p, 1.0)).xyz; }
        let prev_rel = prev_p - fr.prev_cam_pos.xyz;
        let clip = fr.prev_view_proj * vec4(prev_rel, 1.0);
        if (clip.w > 0.001) {
            let ndc = clip.xy / clip.w;
            let prev_uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
            let prev_slice = rm_slice(length(prev_rel));
            if (all(prev_uv > vec2(0.0)) && all(prev_uv < vec2(1.0)) && prev_slice > 0.0 && prev_slice < fr.vol.z) {
                let h = textureSampleLevel(history, linear_samp, vec3(prev_uv, prev_slice / fr.vol.z), 0.0);
                // Emission and dense local media change fast; thin weather converges slowly.
                let local = clamp(length(m.emission) * 4.0 + m.sigma_t * 2.0, 0.0, 1.0);
                let alpha = mix(0.10, 0.5, local);
                result = mix(h, result, alpha);
            }
        }
    }
    textureStore(out_scatter, vec3<i32>(id), result);
}
