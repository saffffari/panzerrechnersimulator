// Stylized raster backend: deterministic surface lighting, no ray queries or temporal history.
// Normal transforms, authored geometry and material channels are shared with the source renderer.
// HDR uses ordinary scene units; select fixed ~1.3 exposure for this backend (not physical320).
// @camera-relative-projection

struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>, // sun ortho cascade — shared with shadow.wgsl so both agree
    cam_pos: vec3<f32>,
    fog_density: f32,
    sun_dir: vec3<f32>,           // TO the sun (lit dir); single source of truth for shading + shadow
    night: f32,                   // 0 = day (Tag), 1 = night (Nacht) — moonlight + emissive-window gate
    snap_res: vec2<f32>,
    interior_light: f32,          // 0 = open/exposed, 1 = buttoned-up inside (view treatment only)
    time: f32,                    // sim seconds — grass wind phase
    track_half: f32,              // tank-track map world half-extent (m); 0 = no track sampling
    n_lights: f32,                // active positioned point lights
    _pad2: f32,
    _pad3: f32,
    // Compact spherical emitters: [pos.xyz, source radius m], [linear color, radiant intensity].
    lights: array<vec4<f32>, 64>,
    inv_view_proj: mat4x4<f32>,
    atmosphere: vec4<f32>, // overcast, storm, act mood, dawn(-1)/dusk(+1)
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var albedo_tex: texture_2d<f32>;    // base color, sRGB
@group(1) @binding(1) var mr_tex: texture_2d<f32>;        // G = roughness, B = metallic (glTF conv.)
@group(1) @binding(2) var normal_tex: texture_2d<f32>;    // tangent-space normal map, linear
@group(1) @binding(3) var samp: sampler;

// Nested sun cascades: near machinery/contact detail plus the broad exterior map. Their matrices
// are snapped in light/world space; the sampling scale is in metres, not arbitrary depth units.
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;
@group(2) @binding(2) var near_shadow_map: texture_depth_2d;
struct NearShadow { view_proj: mat4x4<f32>, scale: vec4<f32> };
@group(2) @binding(3) var<uniform> near_shadow: NearShadow;
@group(2) @binding(4) var cabin_shadow_maps: texture_depth_cube_array;
struct CabinShadows { lamps: array<vec4<f32>,2>, range: vec4<f32> };
@group(2) @binding(5) var<uniform> cabin_shadows: CabinShadows;

// World tank-track rut map (R8, additive tread stamps; renderer::stamp_tank_tracks). Terrain
// (params.w == 3) darkens + dip-shades where tanks have driven — the depression is a LIGHTING
// dip (gradient-perturbed normal), which under the 480×270 crush reads as real displacement.
@group(3) @binding(0) var track_tex: texture_2d<f32>;
@group(3) @binding(1) var track_samp: sampler;

// @campaign-terrain-material


fn cascade_shadow(map: texture_depth_2d, matrix: mat4x4<f32>, world: vec3<f32>,
                  n: vec3<f32>, ndl: f32, world_texel: f32, depth_range: f32) -> f32 {
    let normal_bias = max(0.0005, world_texel * (0.12 + 0.18 * (1.0 - ndl)));
    let projected = matrix * vec4(world + n * normal_bias, 1.0);
    let p = projected.xyz / projected.w;
    let uv = p.xy * vec2(0.5, -0.5) + vec2(0.5);
    if (any(uv < vec2(0.0)) || any(uv > vec2(1.0)) || p.z < 0.0 || p.z > 1.0) { return 1.0; }
    // The old 0.00070 over a 180m interval could detach a shadow by 12.6cm. This receiver bias
    // remains within millimetres for near-map surfaces and scales with actual texels.
    let bias_m = max(0.00035, world_texel * (0.10 + 0.35 * (1.0 - ndl)));
    let ref_depth = p.z - bias_m / depth_range;
    let texel = 0.45 / vec2<f32>(textureDimensions(map));
    return 0.25 * (
        textureSampleCompareLevel(map, shadow_samp, uv + texel * vec2(-1.0,-1.0), ref_depth) +
        textureSampleCompareLevel(map, shadow_samp, uv + texel * vec2( 1.0,-1.0), ref_depth) +
        textureSampleCompareLevel(map, shadow_samp, uv + texel * vec2(-1.0, 1.0), ref_depth) +
        textureSampleCompareLevel(map, shadow_samp, uv + texel * vec2( 1.0, 1.0), ref_depth));
}

fn sun_shadow(world: vec3<f32>, n: vec3<f32>, ndl: f32) -> f32 {
    let far = cascade_shadow(shadow_map,cam.light_view_proj,world,n,ndl,
        near_shadow.scale.z,near_shadow.scale.w);
    let projected = near_shadow.view_proj * vec4(world,1.0);
    let p = projected.xyz / projected.w;
    let edge = max(abs(p.x),abs(p.y));
    if (edge >= 1.0 || p.z <= 0.0 || p.z >= 1.0) { return far; }
    let near = cascade_shadow(near_shadow_map,near_shadow.view_proj,world,n,ndl,
        near_shadow.scale.x,near_shadow.scale.y);
    return mix(near,far,smoothstep(0.88,1.0,edge));
}

fn cabin_light_shadow(source: vec3<f32>, world: vec3<f32>, n: vec3<f32>, ndl: f32) -> f32 {
    for (var lamp = 0; lamp < 2; lamp += 1) {
        let fixture = cabin_shadows.lamps[lamp];
        if (fixture.w < 0.5 || distance(source,fixture.xyz) > 0.0001) { continue; }
        let distance_m = distance(world,source);
        let bias_m = max(0.0007,min(distance_m,8.0)*cabin_shadows.range.z*0.22)
            * (1.0 + 0.5*(1.0-ndl));
        let offset = world + n * bias_m - source;
        let major = max(max(abs(offset.x),abs(offset.y)),abs(offset.z));
        let near = cabin_shadows.range.x;
        let far = cabin_shadows.range.y;
        if (major <= near) { return 1.0; }
        // Cubemap faces store ordinary perspective depth, whose view-Z is the largest absolute
        // direction component. Radial distance here would detach every off-axis lamp shadow.
        let reference = far/(far-near) - far*near/((far-near)*max(major-bias_m,near));
        return textureSampleCompareLevel(cabin_shadow_maps,shadow_samp,offset,lamp,min(reference,0.999999));
    }
    return 1.0;
}

struct Push {
    model: mat4x4<f32>,
    // x = emissive intensity (0 = lit PBR surface); y = metallic factor; z = roughness factor;
    // w = render mode (-1 sparse cubic smoke, 0 ordinary lit, positive values special modes).
    // Non-PBR meshes carry sane fallback factors baked in on the CPU side
    // (gltf_load::CpuMesh — metallic 0.0 / roughness 0.9, fully rough dielectric) so this shader
    // never has to branch between "has PBR data" and "doesn't" — every mesh samples the same
    // three textures (1×1 neutral fallbacks stand in for missing maps) and runs the same formula.
    params: vec4<f32>,
};
var<push_constant> pc: Push;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>, // perspective-correct source texels
    @location(1) normal: vec3<f32>,
    @location(2) world: vec3<f32>,
    @location(3) tangent: vec4<f32>, // xyz = world tangent, w = bitangent handedness
};

// WGSL has no portable matrix `inverse()` builtin.  For a column-major linear transform M, the
// columns of M^-T are its three cofactor vectors divided by det(M).  This is the exact normal
// transform for every nonsingular affine instance, including mirrored and non-uniform scales.
fn inverse_transpose_3x3(linear: mat3x3<f32>) -> mat3x3<f32> {
    let cofactor_0 = cross(linear[1], linear[2]);
    let cofactor_1 = cross(linear[2], linear[0]);
    let cofactor_2 = cross(linear[0], linear[1]);
    let det = dot(linear[0], cofactor_0);
    if (abs(det) <= 1e-12) {
        // A singular model collapses its triangles to zero area, so no physically meaningful
        // surface normal exists. Keep the shader finite; CPU-side transform validation owns this
        // invalid-instance case and visible source geometry must never use it.
        return mat3x3<f32>(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0),
                           vec3(0.0, 0.0, 1.0));
    }
    return mat3x3<f32>(cofactor_0, cofactor_1, cofactor_2) * (1.0 / det);
}

// Shared vertex body: transform by `model`, PS1 vertex-snap in NDC, hand the world-space frame to
// the fragment stage. Called by both the push-constant path (`vs`) and the instanced path
// (`vs_inst`, per-instance model matrix from the instance buffer).
fn project_vertex(model: mat4x4<f32>, pos: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>,
                  tangent: vec4<f32>) -> VsOut {
    let world = (model * vec4(pos, 1.0)).xyz;
    let clip = cam.view_proj * vec4(camera_relative_position(model, pos, cam.cam_pos), 1.0);
    let linear = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    let normal_to_world = inverse_transpose_3x3(linear);
    let handedness = tangent.w * select(-1.0, 1.0, determinant(linear) >= 0.0);

    // (The PS1 vertex snap used to live here — REMOVED: quantizing NDC onto the pixel grid mangled
    // close-range geometry (the whole interior) into shearing polygons. The retro read comes from
    // the 640×360 internal res + posterize/dither/nearest-upscale; geometry renders faithfully.)

    var out: VsOut;
    out.clip = clip;
    out.uv = uv;
    out.normal = normalize(normal_to_world * normal);
    out.world = world;
    out.tangent = vec4(normalize(linear * tangent.xyz), handedness);
    return out;
}

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) tangent: vec4<f32>) -> VsOut {
    return project_vertex(pc.model, pos, normal, uv, tangent);
}

// Instanced path: the model matrix arrives as four per-instance vec4 columns (locations 4-7) from
// an instance-rate vertex buffer — one draw call renders every placement of a repeated module
// (walls, rubble, pines…). params still ride the push constants, set once per batch.
@vertex
fn vs_inst(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
           @location(3) tangent: vec4<f32>,
           @location(4) i0: vec4<f32>, @location(5) i1: vec4<f32>,
           @location(6) i2: vec4<f32>, @location(7) i3: vec4<f32>) -> VsOut {
    var p = pos;
    var n = normal;
    var t = tangent.xyz;
    let model = mat4x4<f32>(vec4(i0.xyz, 0.0), vec4(i1.xyz, 0.0),
                           vec4(i2.xyz, 0.0), vec4(i3.xyz, 1.0));
    if (pc.params.w > 3.5 && pc.params.w < 4.5) {
        // SOLID_GRASS_BEGIN: identical in forward/G-buffer. CPU integrates a damped common world
        // wind; unused affine-row values carry its tip displacement. Random root yaw never rotates
        // the weather. A quadratic centerline fixes both root position and root tangent.
        let linear = mat3x3<f32>(i0.xyz, i1.xyz, i2.xyz);
        let local_tip = transpose(inverse_transpose_3x3(linear)) * vec3(i0.w, 0.0, i2.w);
        let u = clamp(pos.y / 0.48, 0.0, 1.0);
        let derivative = local_tip * (2.0 * u / 0.48);
        p += local_tip * u * u;
        // Inverse transpose of the bending Jacobian (I + derivative outer local-Y).
        n.y -= dot(derivative, n) / max(1.0 + derivative.y, 0.1);
        t += derivative * t.y;
        // SOLID_GRASS_END
    }
    return project_vertex(model, p, normalize(n), uv, vec4(normalize(t), tangent.w));
}

const PI = 3.14159265359;

// GGX / Trowbridge-Reitz normal distribution function.
fn distribution_ggx(NdotH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-8);
}

// Smith geometry term (Schlick-GGX approximation), direct-lighting k.
fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    let gv = NdotV / (NdotV * (1.0 - k) + k);
    let gl = NdotL / (NdotL * (1.0 - k) + k);
    return gv * gl;
}

// Schlick Fresnel.
fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (vec3(1.0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

// Broad orientation bands are anchored to the surface normal and world light direction. Moving
// the camera never re-seeds color or changes an exposure meter. Source texels remain nearest.
fn light_band(value: f32) -> f32 {
    return floor(clamp(value, 0.0, 1.0) * 6.0 + 0.5) / 6.0;
}

fn hemisphere(normal: vec3<f32>) -> vec3<f32> {
    let upper = light_band(normal.y * 0.5 + 0.5);
    let cloud = clamp(cam.atmosphere.x*0.6+cam.atmosphere.y*0.4,0.0,1.0);
    let twilight = clamp(abs(cam.atmosphere.w),0.0,1.0);
    let sky = mix(vec3(0.16,0.19,0.23),vec3(0.12,0.155,0.19),cloud);
    let ground = mix(vec3(0.080,0.078,0.065),vec3(0.060,0.066,0.070),cloud);
    let day = mix(ground,sky,upper);
    let evening = mix(vec3(0.065,0.047,0.060),vec3(0.12,0.125,0.19),upper);
    let night = mix(vec3(0.030,0.034,0.044),vec3(0.055,0.072,0.105),upper);
    return mix(mix(day,evening,twilight),night,
        clamp(cam.night/0.8,0.0,1.0)*(1.0-twilight*0.70));
}

fn surface_response(albedo: vec3<f32>, metallic: f32, roughness: f32,
                    normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    let nl = max(dot(normal, light), 0.0);
    if (nl <= 0.0) { return vec3(0.0); }
    let nv = max(dot(normal, view), 0.001);
    let half_sum = view + light;
    let h = half_sum / max(length(half_sum), 0.0001);
    let nh = max(dot(normal, h), 0.0);
    let vh = max(dot(view, h), 0.0);
    let f0 = mix(vec3(0.04), albedo, metallic);
    let fresnel = fresnel_schlick(vh, f0);
    // Broad bounded highlights retain the difference between painted armor and bare steel.
    let specular = min(distribution_ggx(nh, roughness) * geometry_smith(nv, nl, roughness)
        * fresnel / max(4.0 * nv * nl, 0.001), vec3(3.0));
    return (vec3(1.0) - fresnel) * (1.0 - metallic) * albedo * light_band(nl)
        + specular * nl;
}

struct SurfaceOutput {
    @location(0) color: vec4<f32>,
    @location(1) ambient: vec4<f32>,
};
fn unoccluded(color: vec4<f32>) -> SurfaceOutput {
    return SurfaceOutput(color, vec4(0.0));
}
fn glowing(color: vec4<f32>) -> SurfaceOutput {
    return SurfaceOutput(color, vec4(0.0,0.0,0.0,1.0));
}
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> { return shade(in).color; }
@fragment fn fs_ambient(in: VsOut) -> SurfaceOutput { return shade(in); }
fn shade(in: VsOut) -> SurfaceOutput {
    // Resolve all sampled channels before divergent effect branches. The bound material sampler
    // is nearest; the original scalar/atlas material factors remain authoritative.
    let raw4 = textureSample(albedo_tex, samp, in.uv);
    let mr = textureSample(mr_tex, samp, in.uv);
    let mapped_normal = textureSample(normal_tex, samp, in.uv).rgb * 2.0 - vec3(1.0);
    let macro_dx = dpdx(in.uv) * vec2<f32>(textureDimensions(albedo_tex));
    let macro_dy = dpdy(in.uv) * vec2<f32>(textureDimensions(albedo_tex));
    let macro_lod = log2(max(max(length(macro_dx), length(macro_dy)), 1.0));
    let footprint = max(length(dpdx(in.world.xz)), length(dpdy(in.world.xz)));
    let sparse_cubic_smoke = pc.params.w < -0.5;
    if (sparse_cubic_smoke && distance(in.world, cam.cam_pos) < 0.16) { discard; }
    if (pc.params.w > 3.5 && pc.params.w < 4.5 && raw4.a < 0.5) { discard; }

    let view = normalize(cam.cam_pos - in.world);
    let geometric_normal = normalize(in.normal);
    // The same premultiplied transparent-pane path as the existing forward pipeline. Optical
    // relay cameras are a separate backend feature; this does not invent a ray-query reflection.
    if (pc.params.w > 7.5) {
        let n = geometric_normal * select(-1.0, 1.0, dot(geometric_normal, view) >= 0.0);
        let fresnel = pow(1.0 - max(dot(n, view), 0.0), 3.0);
        let alpha = 0.025 + 0.18 * fresnel;
        let sheen = hemisphere(reflect(-view, n));
        // The pane's weak sheen contributes weak glow; looking through glass must not mark
        // the whole underlying scene as a fully emissive surface.
        return SurfaceOutput(vec4(mix(vec3(0.36, 0.43, 0.40), sheen, 0.45) * alpha, alpha),
            vec4(0.0, 0.0, 0.0, alpha));
    }
    if (!sparse_cubic_smoke && pc.params.x > 0.0) {
        if (pc.params.w > 1.5 && pc.params.w < 2.5) {
            // Lamp/status/foam own color remains its own color at day and night.
            return glowing(vec4(raw4.rgb * min(pc.params.x, 12.0), 1.0));
        }
        if (pc.params.w > 0.5 && pc.params.w < 1.5) {
            return glowing(vec4(raw4.rgb * min(pc.params.x, 12.0) * clamp(cam.night, 0.0, 1.0), 1.0));
        }
        if (pc.params.w < 0.5) {
            let fire_color = mix(vec3(1.0, 0.24, 0.03), raw4.rgb, 0.18);
            return unoccluded(vec4(fire_color * min(pc.params.x * 0.30, 9.0), 1.0));
        }
    }

    let terrain = pc.params.w > 2.5 && pc.params.w < 3.5;
    var albedo = raw4.rgb;
    var roughness = clamp(mr.g * pc.params.z, 0.12, 1.0);
    let metallic = clamp(mr.b * pc.params.y, 0.0, 1.0);
    var normal = geometric_normal;
    let tangent = in.tangent.xyz - normal * dot(normal, in.tangent.xyz);
    if (dot(tangent, tangent) > 1e-8) {
        let t = normalize(tangent);
        normal = normalize(mat3x3<f32>(t, cross(normal, t) * in.tangent.w, normal) * mapped_normal);
    }
    if (terrain) {
        albedo = terrain_material(textureSampleLevel(albedo_tex, terrain_detail_sampler,
            in.uv, macro_lod).rgb, in.world, geometric_normal.y, cam.atmosphere.z, footprint);
        roughness = terrain_roughness(cam.atmosphere.z);
        if (cam.track_half > 1.0) {
            let track_uv = in.world.xz / (2.0 * cam.track_half) + vec2(0.5);
            let track = textureSampleLevel(track_tex, track_samp, track_uv, 0.0).r;
            albedo *= mix(vec3(1.0), vec3(0.52, 0.54, 0.56), clamp(track * 1.4, 0.0, 1.0));
        }
    }
    if (sparse_cubic_smoke) { return unoccluded(vec4(albedo * hemisphere(normal), 1.0)); }

    let f0 = mix(vec3(0.04), albedo, metallic);
    let nv = max(dot(normal, view), 0.0);
    let fresnel = fresnel_schlick(nv, f0);
    var color = albedo * (1.0 - metallic) * hemisphere(normal);
    // A deterministic broad environment reflection keeps true bare metal legible. Paint remains
    // dielectric; this does not tint or globally remap any source texture/material assignment.
    color += fresnel * hemisphere(reflect(-view, normal)) * mix(0.75, 0.35, roughness);

    let ambient = color;
    let sun = normalize(cam.sun_dir);
    let overcast = clamp(cam.atmosphere.x, 0.0, 1.0);
    let twilight = clamp(abs(cam.atmosphere.w),0.0,1.0);
    let daylight = mix(vec3(1.48,1.42,1.29),vec3(1.54,0.85,0.48),twilight);
    let sun_color = mix(daylight,vec3(0.14,0.19,0.29),
        clamp(cam.night/0.8,0.0,1.0)*(1.0-twilight*0.70))
        * mix(1.0,0.42,overcast) * mix(1.0,0.70,clamp(cam.atmosphere.y,0.0,1.0));
    color += surface_response(albedo, metallic, roughness, normal, view, sun) * sun_color
        * sun_shadow(in.world, geometric_normal, max(dot(geometric_normal, sun), 0.0));

    // Existing finite spherical lights. The two named cabin fixtures use cached source-geometry
    // cubemap visibility; transient and other sources retain their ordinary bounded light loop.
    let light_count = min(max(i32(cam.n_lights), 0), 32);
    for (var index = 0; index < light_count; index += 1) {
        let source = cam.lights[index * 2];
        let emission = cam.lights[index * 2 + 1];
        let offset = source.xyz - in.world;
        let d2 = dot(offset, offset);
        if (d2 < 1e-8 || emission.w <= 0.0) { continue; }
        let direction = offset * inverseSqrt(d2);
        let irradiance = emission.rgb * min(emission.w / max(d2, max(source.w * source.w, 0.0025)), 12.0);
        color += surface_response(albedo, metallic, roughness, normal, view, direction) * irradiance
            * cabin_light_shadow(source.xyz,in.world,geometric_normal,max(dot(geometric_normal,direction),0.0));
    }
    return SurfaceOutput(vec4(max(color, vec3(0.0)), 1.0), vec4(ambient, 0.0));
}

@fragment fn fs_hover() -> @location(0) vec4<f32> { return vec4(1.0); }
