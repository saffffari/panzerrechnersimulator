// Raymarch backend one-pixel world particles drawn in HDR before bloom and crush: the
// simulation-owned snow (kind 0) and cabin dust (kind 1), lit by the key beam, the sky and the
// clustered lamps; and the fire grains themselves as embers (kind 2, colour and intensity packed
// into the second lane), self-luminous. All are seen through the integrated froxel medium.
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var integrated: texture_3d<f32>;
@group(0) @binding(3) var linear_samp: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) world: vec3<f32>,
    @location(1) @interpolate(flat) packed: u32,
};

@vertex
fn vs(@builtin(vertex_index) vertex: u32, @location(0) world: vec3<f32>, @location(1) packed: u32) -> VsOut {
    var out: VsOut;
    out.world = world;
    out.packed = packed;
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
    let kind = in.packed & 0xFu;
    let uv = (vec2<f32>(pixel) + vec2(0.5)) * fr.res.zw;
    let slice = clamp((rm_slice(dist) - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
    let transmittance = textureSampleLevel(integrated, linear_samp, vec3(uv, slice), 0.0).a;
    let focal = fr.view_proj[1][1] * fr.res.y * 0.5;
    if (kind == 2u) {
        // Ember: a 1 cm incandescent grain; projected coverage inside the pixel scales its energy.
        let intensity = f32((in.packed >> 8u) & 0xFFu) / 32.0;
        let colour = vec3(f32((in.packed >> 16u) & 0x1Fu) / 31.0, f32((in.packed >> 21u) & 0x3Fu) / 63.0, f32((in.packed >> 27u) & 0x1Fu) / 31.0);
        let coverage = clamp(pow(0.012 * focal / max(dist, 0.05), 2.0), 0.02, 1.0);
        return vec4(colour * intensity * coverage * transmittance, 0.0);
    }
    // Weather snow stays out of the cabin air and the cabin's dust motes stay in it (as on the
    // shader backend); a mote takes the sky only where the cover map sees it through a hatch.
    let cabin_domain = rm_cabin_domain(in.world);
    if (kind == 0u && cabin_domain >= 0) { discard; return vec4(0.0); }
    if (kind == 1u && cabin_domain < 0 && rm_cabin_air_on()) { discard; return vec4(0.0); }
    let dust = kind == 1u;
    // Projected coverage of a 2.5 mm flake (0.4 mm mote) inside one native pixel.
    let size_m = select(0.0025, 0.0004, dust);
    let coverage = clamp(pow(size_m * focal / max(dist, 0.05), 2.0), select(0.05, 0.02, dust), 1.0);
    let albedo = select(0.92, 0.55, dust);
    var radiance = vec3(0.0);
    if (fr.sun_radiance.w > 0.5) {
        let vis = rm_sun_visibility(in.world, 0.0);
        radiance += fr.sun_radiance.rgb * vis * rm_hg(dot(ray, fr.sun_dir.xyz), 0.55) * 4.0;
    }
    // A flake scatters mostly forward: it carries the brighter sky behind it more than the ground.
    radiance += mix(rm_sky_isotropic(), rm_sky_hemisphere(vec3(0.0, 1.0, 0.0)), 0.6) * 1.25 * mix(select(0.08, 0.0, cabin_domain >= 0), 1.0, rm_sky_visibility(in.world, 0.0));
    let cell = rm_cell_index(uv, dist);
    let lstart = light_cell_start[cell];
    let lend = light_cell_start[cell + 1u];
    for (var i = lstart; i < lend; i += 1u) {
        let index = light_cell_items[i];
        let s = rm_light_sample(index, in.world);
        if (max(s.e.r, max(s.e.g, s.e.b)) <= 0.0) { continue; }
        radiance += s.e * rm_light_visibility(index, in.world, 0.02) * rm_hg(dot(ray, s.dir), 0.55) * 4.0;
    }
    radiance *= albedo;
    let alpha = coverage * transmittance;
    return vec4(radiance * alpha, alpha);
}
