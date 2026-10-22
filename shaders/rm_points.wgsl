// Raymarch backend one-pixel world particles (simulation-owned snow and cabin dust) drawn in HDR
// before bloom and crush, lit by the key beam, the sky and the lamps, seen through the integrated
// froxel medium. Each accepted particle covers exactly one native pixel.
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var integrated: texture_3d<f32>;
@group(0) @binding(3) var linear_samp: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) world: vec3<f32>,
    @location(1) @interpolate(flat) kind: u32,
};

@vertex
fn vs(@builtin(vertex_index) vertex: u32, @location(0) world: vec3<f32>, @location(1) kind: u32) -> VsOut {
    var out: VsOut;
    out.world = world;
    out.kind = kind;
    let clip = fr.view_proj * vec4(world - fr.cam_pos.xyz, 1.0);
    if (clip.w <= 0.001 || clip.z < 0.0 || clip.z >= clip.w) {
        out.position = vec4(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    let ndc = clip.xy / clip.w;
    let pixel = floor(vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * fr.res.xy);
    var corners = array<vec2<f32>, 6>(vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0));
    let edge = (pixel + corners[vertex]) / fr.res.xy;
    out.position = vec4(vec2(edge.x * 2.0 - 1.0, 1.0 - edge.y * 2.0) * clip.w, clip.z, clip.w);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(depth_tex));
    let pixel = vec2<i32>(in.position.xy);
    if (any(pixel < vec2(0)) || any(pixel >= dims) || in.position.z >= textureLoad(depth_tex, pixel, 0)) { discard; }
    let rel = in.world - fr.cam_pos.xyz;
    let dist = length(rel);
    let ray = rel / max(dist, 0.001);
    let dust = in.kind == 1u;
    // Projected coverage of a 2.5 mm flake (0.4 mm mote) inside one native pixel.
    let focal = fr.view_proj[1][1] * fr.res.y * 0.5;
    let size_m = select(0.0025, 0.0004, dust);
    let coverage = clamp(pow(size_m * focal / max(dist, 0.05), 2.0), select(0.05, 0.02, dust), 1.0);
    let albedo = select(0.92, 0.55, dust);
    var radiance = vec3(0.0);
    if (fr.sun_radiance.w > 0.5) {
        let vis = rm_sun_visibility(in.world, 0.0);
        radiance += fr.sun_radiance.rgb * vis * rm_hg(dot(ray, fr.sun_dir.xyz), 0.55) * 4.0;
    }
    radiance += rm_sky_isotropic() * mix(0.08, 1.0, rm_sky_visibility(in.world, 0.0));
    let n_lights = u32(fr.light_count.x);
    for (var i = 0u; i < n_lights; i += 1u) {
        let l = fr.lights[i * 2u];
        let c = fr.lights[i * 2u + 1u];
        let dv = l.xyz - in.world;
        let r2 = max(dot(dv, dv), l.w * l.w);
        let e = c.rgb * c.w / r2;
        if (max(e.r, max(e.g, e.b)) < 0.002) { continue; }
        let ldir = normalize(dv);
        radiance += e * rm_light_visibility(i, in.world, 0.02) * rm_hg(dot(ray, ldir), 0.55) * 4.0;
    }
    radiance *= albedo;
    let slice = clamp((rm_slice(dist) - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
    let uv = (vec2<f32>(pixel) + vec2(0.5)) * fr.res.zw;
    let transmittance = textureSampleLevel(integrated, linear_samp, vec3(uv, slice), 0.0).a;
    let alpha = coverage * transmittance;
    return vec4(radiance * alpha, alpha);
}
