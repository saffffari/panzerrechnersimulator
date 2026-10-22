// Raymarch backend froxel lighting. One thread per froxel: evaluate the media there (weather,
// cabin haze, plumes, binned fire/soot/smoke/glow parcels), march toward the key beam and the
// shadowed lamps through that same medium, gather sky and lamp in-scatter, add emission, and blend
// with the reprojected previous frame. Output: in-scattered radiance per metre (rgb) and
// extinction per metre (a). rm_integrate.wgsl turns this into a per-slice transmittance table.
struct Parcel { a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, };
// a: position xyz, radius m   b: colour rgb, kind (0 fire, 1 soot, 2 smoke, 3 glow, 4 segment)
// c: emission per m, extinction per m, noise seed, unused   d: segment end xyz
@group(0) @binding(1) var<storage, read> parcels: array<Parcel>;
@group(0) @binding(2) var<storage, read> cell_start: array<u32>;
@group(0) @binding(3) var<storage, read> cell_items: array<u32>;
@group(0) @binding(4) var history: texture_3d<f32>;
@group(0) @binding(5) var linear_samp: sampler;
@group(0) @binding(6) var out_scatter: texture_storage_3d<rgba16float, write>;
@group(0) @binding(7) var depth_tex: texture_depth_2d;

const SUN_MARCH_STEPS: u32 = 6u;
const LAMP_MARCH_STEPS: u32 = 4u;

fn parcel_medium(p: vec3<f32>, cell: u32, flame_noise: f32, m_in: Medium) -> Medium {
    var m = m_in;
    let start = cell_start[cell];
    let end = cell_start[cell + 1u];
    for (var i = start; i < end; i += 1u) {
        let parcel = parcels[cell_items[i]];
        let kind = u32(parcel.b.w);
        var w = 0.0;
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
            w = (1.0 - dd) * (1.0 - dd);
            if (kind == 0u) {
                // Flames: the parcel is a hot gas pocket whose brightness breaks up and licks
                // upward with the shared noise field; never a frame-hashed sparkle.
                w *= 0.35 + 1.3 * flame_noise;
            } else if (kind == 1u || kind == 2u) {
                w *= 0.6 + 0.8 * flame_noise;
            }
        }
        m.emission += parcel.b.rgb * parcel.c.x * w;
        let ext = parcel.c.y * w;
        m.sigma_t += ext;
        if (kind == 1u) { m.sigma_s += vec3(ext * 0.12); }
        else if (kind == 2u) { m.sigma_s += parcel.b.rgb * ext * 0.75; }
        else if (kind == 0u) { m.sigma_s += vec3(ext * 0.3); }
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
    let cell = vec3<u32>(id * RM_CELLS / dims);
    let cell_index = (cell.z * RM_CELLS.y + cell.y) * RM_CELLS.x + cell.x;
    let flame_noise = rm_fbm(p * vec3(9.0, 6.0, 9.0) + vec3(0.0, -fr.cam_pos.w * 2.6, 0.0), 41u);
    m = parcel_medium(p, cell_index, flame_noise, m);
    if (d > scene_d + slice_width) {
        textureStore(out_scatter, vec3<i32>(id), vec4(0.0, 0.0, 0.0, m.sigma_t));
        return;
    }

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
                    * rm_hg(dot(ray, fr.sun_dir.xyz), m.g);
            }
        }
        // Skylight: isotropic mean sky through the vertical cover map.
        let sky_vis = rm_sky_visibility(p, 0.0);
        inscatter += m.sigma_s * rm_sky_isotropic() * mix(0.08, 1.0, sky_vis);
        // Lamps, fires and flashes.
        let n_lights = u32(fr.light_count.x);
        for (var i = 0u; i < n_lights; i += 1u) {
            let l = fr.lights[i * 2u];
            let c = fr.lights[i * 2u + 1u];
            let dv = l.xyz - p;
            let dist2 = dot(dv, dv);
            let r2 = max(dist2, l.w * l.w);
            let e = c.rgb * c.w / r2;
            if (max(e.r, max(e.g, e.b)) < 0.002) { continue; }
            let dist = sqrt(dist2);
            let dir = dv / max(dist, 0.001);
            let shadowed = (fr.light_count.y >= 0.0 && u32(fr.light_count.y) == i)
                || (fr.light_count.z >= 0.0 && u32(fr.light_count.z) == i);
            var vis = 1.0;
            var tau = 0.0;
            if (shadowed) {
                vis = rm_light_visibility(i, p, 0.02);
                if (vis <= 0.001) { continue; }
                tau = rm_march_tau(p, dir, dist, LAMP_MARCH_STEPS, jitter.y);
            } else {
                tau = m.sigma_t * min(dist, 12.0);
            }
            inscatter += m.sigma_s * e * vis * exp(-tau) * rm_hg(dot(ray, dir), m.g);
        }
    }

    // Temporal history: reproject this froxel's world point into the previous frame's grid.
    var result = vec4(inscatter, m.sigma_t);
    if (fr.prev_cam_pos.w > 0.5) {
        let prev_rel = p - fr.prev_cam_pos.xyz;
        let clip = fr.prev_view_proj * vec4(prev_rel, 1.0);
        if (clip.w > 0.001) {
            let ndc = clip.xy / clip.w;
            let prev_uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
            let prev_slice = rm_slice(length(prev_rel));
            if (all(prev_uv > vec2(0.0)) && all(prev_uv < vec2(1.0)) && prev_slice > 0.0 && prev_slice < fr.vol.z) {
                let h = textureSampleLevel(history, linear_samp, vec3(prev_uv, prev_slice / fr.vol.z), 0.0);
                // Emission and dense local media change fast; thin weather converges slowly.
                let local = clamp(length(m.emission) * 4.0 + m.sigma_t * 2.0, 0.0, 1.0);
                let alpha = mix(0.10, 0.45, local);
                result = mix(h, result, alpha);
            }
        }
    }
    textureStore(out_scatter, vec3<i32>(id), result);
}
