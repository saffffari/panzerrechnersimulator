// @scene-visibility-mesh
// @scene-light-visibility
// @cabin-medium
// @weather-density
// Raster surface lighting: deterministic, no ray queries or temporal history. It serves the shader
// backend and the physical backend without ray queries. Normal transforms, authored geometry and
// material channels are shared with the ray-traced renderer, and so are the units: HDR holds
// spectral radiance (W m^-2 sr^-1 nm^-1 at 680/550/440 nm) lit by the physical environment probe,
// the key body's beam and the positioned emitters. The crewman's eye maps it to the display.
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
    track_half: f32,              // act track enable; sparse track cells remain 0.125 m, 0 = off
    n_lights: f32,                // active positioned point lights
    _pad2: f32,
    _pad3: f32,
    // Compact spherical emitters: [pos.xyz, source radius m], [linear color, radiant intensity].
    lights: array<vec4<f32>, 64>,
    inv_view_proj: mat4x4<f32>,
    atmosphere: vec4<f32>, // overcast, storm, act mood, dawn(-1)/dusk(+1)
    world_medium:vec4<f32>,
    transport_passes:vec4<f32>,
    cabin_medium:CabinMedium,
    medium_transport:vec4<f32>,
};
@group(2) @binding(11) var cabin_air_columns:texture_2d_array<f32>;
fn cabin_transform(i:u32)->mat4x4<f32> {return cam.cabin_medium.world_to_local[i];}
fn cabin_grid(i:u32)->vec4<f32> {return cam.cabin_medium.grid_min_step[i];}
fn cabin_size(i:u32)->vec4<u32> {return cam.cabin_medium.grid_size[i];}
fn cabin_min(i:u32)->vec3<f32> {return cam.cabin_medium.bounds_min[i].xyz;}
fn cabin_max(i:u32)->vec3<f32> {return cam.cabin_medium.bounds_max[i].xyz;}
fn source_background_medium_transmittance(origin:vec3<f32>,direction:vec3<f32>,distance_m:f32)->f32 {
    let depth=weather_and_cabin_optical_depth(origin,direction,distance_m,cam.world_medium.zw,
        cam.world_medium.x,cam.world_medium.y,cam.medium_transport.yzw,24u,cam.medium_transport.x);
    return exp(-depth.x-depth.y);
}
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
// @cabin-visibility-sampling
@group(2) @binding(5) var<uniform> cabin_shadows: CabinShadows;
// Near-field environment visibility (src/sky_visibility.rs): 24 sky and 8 ground directions, each an
// orthographic depth tile of the real scene geometry around the camera. Terrain never occludes.
struct SkyVisibility {
    view_proj:array<mat4x4<f32>,66>,
    directions:array<vec4<f32>,32>,
    grid:vec4<f32>,
    global_grid:vec4<f32>, // coarse world texel, near depth span, global depth span, reserved
    sun_direction:vec4<f32>,
};
@group(2) @binding(6) var sky_visibility_map: texture_depth_2d;
@group(2) @binding(7) var<uniform> sky_visibility: SkyVisibility;

// World tank-track rut map (R8, additive tread stamps; renderer::stamp_tank_tracks). Terrain
// (params.w == 3) changes material reflectance where the drawn treads have passed.
// Legacy mesh/gbuf paths also perturb the shading normal; terrain geometry is unchanged.
@group(3) @binding(0) var track_tex: texture_2d<f32>;
@group(3) @binding(1) var track_samp: sampler;
@group(3) @binding(2) var track_pages: texture_2d<u32>;

// Fixed 12.5 cm cells in persistent 64 m pages. Loading the containing cell avoids linear
// filtering beyond the tread edge or across unrelated atlas pages. Maximum edge error is
// half a cell along each world axis (8.84 cm at a diagonal), independent of act size.
fn sample_track(world: vec2<f32>) -> f32 {
    let tile = vec2<i32>(floor(world / 64.0));
    let lookup = tile + vec2<i32>(128);
    if (any(lookup < vec2<i32>(0)) || any(lookup >= vec2<i32>(256))) { return 0.0; }
    let entry = textureLoad(track_pages, lookup, 0).r;
    if (entry == 0u) { return 0.0; }
    let page = entry - 1u;
    let local = vec2<i32>(floor((world - vec2<f32>(tile) * 64.0) / 0.125));
    let atlas = vec2<i32>(i32(page % 16u), i32(page / 16u)) * 512 + local;
    return textureLoad(track_tex, atlas, 0).r;
}


// @campaign-terrain-material


// Bilinear comparison with a plane-correct reference at each participating texel centre.
// Hardware PCF cannot adjust the reference separately for its four hidden comparisons.
fn receiver_plane_shadow_tap(map: texture_depth_2d, sample_uv: vec2<f32>,
                             receiver_uv: vec2<f32>, depth: f32, gradient: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(map));
    let pixel = sample_uv * vec2<f32>(size) - vec2(0.5);
    let first = vec2<i32>(floor(pixel));
    let fraction = fract(pixel);
    var visibility = 0.0;
    for (var y = 0; y < 2; y += 1) {
        for (var x = 0; x < 2; x += 1) {
            let texel = clamp(first + vec2(x, y), vec2(0), size - vec2(1));
            let uv = (vec2<f32>(texel) + vec2(0.5)) / vec2<f32>(size);
            let reference = depth + dot(gradient, uv - receiver_uv);
            let weight = select(1.0 - fraction.x, fraction.x, x == 1)
                       * select(1.0 - fraction.y, fraction.y, y == 1);
            visibility += weight * select(0.0, 1.0, reference <= textureLoad(map, texel, 0));
        }
    }
    return visibility;
}

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
    // Orthographic light-space receiver-plane gradient. A shared reference depth for all
    // PCF neighbours makes a tilted surface shadow itself in repeating bands. Compare each
    // actual depth texel to this same triangle evaluated at that texel, retaining contact bias.
    let row_x = vec3(matrix[0].x, matrix[1].x, matrix[2].x);
    let row_y = vec3(matrix[0].y, matrix[1].y, matrix[2].y);
    let row_z = vec3(matrix[0].z, matrix[1].z, matrix[2].z);
    let plane = vec3(dot(n, row_x) / dot(row_x, row_x),
                     dot(n, row_y) / dot(row_y, row_y),
                     dot(n, row_z) / dot(row_z, row_z));
    var gradient = vec2(0.0);
    if (abs(plane.z) > 1e-6) {
        // UV reverses clip Y; both image axes span two clip units.
        gradient = vec2(-2.0 * plane.x, 2.0 * plane.y) / plane.z;
    }
    let texel = 0.45 / vec2<f32>(textureDimensions(map));
    return 0.25 * (
        receiver_plane_shadow_tap(map, uv + texel * vec2(-1.0,-1.0), uv, ref_depth, gradient) +
        receiver_plane_shadow_tap(map, uv + texel * vec2( 1.0,-1.0), uv, ref_depth, gradient) +
        receiver_plane_shadow_tap(map, uv + texel * vec2(-1.0, 1.0), uv, ref_depth, gradient) +
        receiver_plane_shadow_tap(map, uv + texel * vec2( 1.0, 1.0), uv, ref_depth, gradient));
}

const TERRAIN_VISIBILITY_LIFT_M: f32 = 0.25;
fn sun_shadow(world:vec3<f32>,n:vec3<f32>,ndl:f32)->vec3<f32> {
    return scene_environment_transmission(world+n*0.0001,normalize(cam.sun_dir));
}

// @sky-visibility-sampling
// Whether direction k's environment reaches this surface point. The receiver is offset along its
// face normal and toward the environment by texel-scaled amounts so a surface never occludes itself.
fn sky_direction_visibility(k: i32, world: vec3<f32>, receiver_normal: vec3<f32>) -> vec3<f32> {
    let receiver = world + receiver_normal * 0.0001;
    return sky_tile_visibility(sky_visibility_map, shadow_samp, sky_visibility.view_proj[k],
        sky_visibility.grid, k, receiver);
}

// The physical environment (src/environment_probe.rs): lanes 0-31 the atmosphere's radiance for
// each visibility direction (renderer units, W m^-2 sr^-1 nm^-1), 32 the key body's beam
// irradiance at the camera, 33 the diffuse horizontal sky irradiance, 34/35 the mean sky and
// ground radiance, 36-67 the directions themselves (xyz, solid angle). The same atmosphere the
// ray-traced path and the visible sky query; no palette.
struct EnvironmentProbe { lanes: array<vec4<f32>, 68> };
@group(2) @binding(8) var<uniform> probe: EnvironmentProbe;

fn probe_direction(k: i32) -> vec4<f32> {
    return probe.lanes[36 + k];
}

// Direction k's environment at this receiver: the probe radiance through the visibility atlas
// (open where the atlas is inactive or the receiver lies outside its footprint).
fn direction_visibility(k: i32, world: vec3<f32>, receiver_normal: vec3<f32>) -> vec3<f32> {
    if (k >= i32(sky_visibility.grid.z)) { return scene_environment_transmission(world+receiver_normal*0.0001,probe_direction(k).xyz); }
    return sky_direction_visibility(k, world, receiver_normal);
}

// Irradiance on the shading normal (normalised cosine quadrature: exactly pi L under a uniform open
// environment) and the lobe-averaged radiance about the mirror direction (glossy reflection).
struct EnvironmentLight { irradiance: vec3<f32>, glossy: vec3<f32> };
fn environment_light(world: vec3<f32>, receiver_normal: vec3<f32>, n: vec3<f32>,
                     mirror: vec3<f32>, sharpness: f32) -> EnvironmentLight {
    var irradiance = vec3(0.0);
    var glossy = vec3(0.0);
    var diffuse_weight = 0.0;
    var glossy_weight = 0.0;
    for (var k = 0; k < 32; k += 1) {
        let d = probe_direction(k);
        let wd = max(dot(n, d.xyz), 0.0) * d.w;
        let wg = select(0.0,pow(max(dot(mirror, d.xyz), 0.0), sharpness) * d.w,dot(n,d.xyz)>0.0);
        if (wd <= 0.0 && wg <= 0.0) { continue; }
        diffuse_weight += wd;
        glossy_weight += wg;
        let radiance = scene_environment_radiance(world+receiver_normal*0.0001,d.xyz,k);
        irradiance += radiance * wd;
        glossy += radiance * wg;
    }
    return EnvironmentLight(irradiance * (PI / max(diffuse_weight, 1e-6)),
        glossy / max(glossy_weight, 1e-6));
}

struct CubeReceiver { uv: vec2<f32>, face: i32 };

fn cube_receiver(direction: vec3<f32>) -> CubeReceiver {
    let a = abs(direction);
    var face: i32;
    var st: vec2<f32>;
    if (a.x >= a.y && a.x >= a.z) {
        face = select(1, 0, direction.x >= 0.0);
        st = vec2(select(direction.z, -direction.z, direction.x >= 0.0), -direction.y) / a.x;
    } else if (a.y >= a.z) {
        face = select(3, 2, direction.y >= 0.0);
        st = vec2(direction.x, select(-direction.z, direction.z, direction.y >= 0.0)) / a.y;
    } else {
        face = select(5, 4, direction.z >= 0.0);
        st = vec2(select(-direction.x, direction.x, direction.z >= 0.0), -direction.y) / a.z;
    }
    return CubeReceiver(st * 0.5 + vec2(0.5), face);
}

fn cube_texel_ray(face: i32, uv: vec2<f32>) -> vec3<f32> {
    let st = uv * 2.0 - vec2(1.0);
    switch face {
        case 0: { return vec3(1.0, -st.y, -st.x); }
        case 1: { return vec3(-1.0, -st.y, st.x); }
        case 2: { return vec3(st.x, 1.0, st.y); }
        case 3: { return vec3(st.x, -1.0, -st.y); }
        case 4: { return vec3(st.x, -st.y, 1.0); }
        default: { return vec3(-st.x, -st.y, -1.0); }
    }
}

fn cabin_receiver_plane_pcf(offset: vec3<f32>, n: vec3<f32>, bias_m: f32, lamp: i32) -> f32 {
    let receiver = cube_receiver(offset);
    let size = i32(textureDimensions(cabin_shadow_maps).x);
    let pixel = receiver.uv * f32(size) - vec2(0.5);
    let first = vec2<i32>(floor(pixel));
    let fraction = fract(pixel);
    let near = cabin_shadow_range(lamp).x;
    let plane_distance = dot(n, offset);
    var visibility = 0.0;
    for (var y = 0; y < 2; y += 1) {
        for (var x = 0; x < 2; x += 1) {
            // Stay on this face's exact texel centres: a cube comparison has no textureLoad,
            // and sampling between centres would reintroduce hidden, uncorrected PCF taps.
            let texel = clamp(first + vec2(x, y), vec2(0), vec2(size - 1));
            let uv = (vec2<f32>(texel) + vec2(0.5)) / f32(size);
            let ray = cube_texel_ray(receiver.face, uv);
            let denominator = dot(n, ray);
            var tap = 1.0;
            if (abs(denominator) > 1e-6) {
                // Exact perspective ray/receiver-plane intersection. The dominant ray axis
                // is one, so this is the face's view-Z, not radial distance to the lamp.
                let major = plane_distance / denominator;
                if (major > near) {
                    tap = textureSampleCompareLevel(cabin_shadow_maps, shadow_samp, ray, lamp,
                                                    cabin_shadow_reference(major, bias_m, lamp));
                }
            }
            let weight = select(1.0 - fraction.x, fraction.x, x == 1)
                       * select(1.0 - fraction.y, fraction.y, y == 1);
            visibility += weight * tap;
        }
    }
    return visibility;
}

fn cabin_light_shadow(source:vec3<f32>,radius:f32,world:vec3<f32>,offset:vec3<f32>,n:vec3<f32>)->vec3<f32> {
    if(PORTABLE_VISIBILITY){return hand_torch_gain(source,world)*scene_point_transmission(source,radius,world+n*0.0001);}
    let lamp=cabin_fixture_index(source);
    if(lamp<0){scene_visibility_unresolved=true;return vec3(1.0,0.0,1.0);}
    let local_offset=cabin_lamp_local(lamp,offset);
    let local_normal=cabin_lamp_local(lamp,n);
    let major=max(abs(local_offset.x),max(abs(local_offset.y),abs(local_offset.z)));
    let limits=cabin_shadow_range(lamp);
    if(major>=limits.y){
        if(cabin_shadow_reach_limited(lamp)){return vec3(0.0);}
        scene_visibility_unresolved=true;return vec3(1.0,0.0,1.0);
    }
    if(major<=limits.x){return vec3(hand_torch_gain(source,world));}
    let bias=max(0.00005,major*0.00000024);
    return vec3(hand_torch_gain(source,world)*cabin_receiver_plane_pcf(local_offset,local_normal,bias,lamp));
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
    @interpolate(perspective, centroid) @location(0) uv: vec2<f32>, // perspective-correct source texels
    @interpolate(perspective, centroid) @location(1) normal: vec3<f32>,
    @interpolate(perspective, centroid) @location(2) world: vec3<f32>,
    @interpolate(perspective, centroid) @location(3) tangent: vec4<f32>, // xyz = world tangent, w = bitangent handedness
    @interpolate(perspective, centroid) @location(4) relative_world: vec3<f32>, // preserve local receiver-plane precision
    // Derivatives need pixel-centre interpolation. Coverage centroids can shift/collapse at
    // primitive edges; they remain correct for the shaded receiver, but not a stable plane basis.
    @interpolate(perspective, center) @location(5) plane_relative_world: vec3<f32>,
};

// The selection pass and shaded pass share precisely invariant source clip positions.
// Fragment-side VsOut deliberately does not carry the vertex-only invariant attribute.
struct VertexOut {
    @invariant @builtin(position) clip: vec4<f32>,
    @interpolate(perspective, centroid) @location(0) uv: vec2<f32>, // perspective-correct source texels
    @interpolate(perspective, centroid) @location(1) normal: vec3<f32>,
    @interpolate(perspective, centroid) @location(2) world: vec3<f32>,
    @interpolate(perspective, centroid) @location(3) tangent: vec4<f32>, // xyz = world tangent, w = bitangent handedness
    @interpolate(perspective, centroid) @location(4) relative_world: vec3<f32>, // preserve local receiver-plane precision
    // Derivatives need pixel-centre interpolation. Coverage centroids can shift/collapse at
    // primitive edges; they remain correct for the shaded receiver, but not a stable plane basis.
    @interpolate(perspective, center) @location(5) plane_relative_world: vec3<f32>,
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
                  tangent: vec4<f32>) -> VertexOut {
    let world = (model * vec4(pos, 1.0)).xyz;
    let relative_world = camera_relative_position(model, pos, cam.cam_pos);
    let clip = cam.view_proj * vec4(relative_world, 1.0);
    let linear = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    let normal_to_world = inverse_transpose_3x3(linear);
    let handedness = tangent.w * select(-1.0, 1.0, determinant(linear) >= 0.0);

    // (The PS1 vertex snap used to live here — REMOVED: quantizing NDC onto the pixel grid mangled
    // close-range geometry (the whole interior) into shearing polygons. The retro read comes from
    // the 640×360 internal res + posterize/dither/nearest-upscale; geometry renders faithfully.)

    var out: VertexOut;
    out.clip = clip;
    out.uv = uv;
    out.normal = normalize(normal_to_world * normal);
    out.world = world;
    out.relative_world = relative_world;
    out.plane_relative_world = relative_world;
    out.tangent = vec4(normalize(linear * tangent.xyz), handedness);
    return out;
}

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) tangent: vec4<f32>) -> VertexOut {
    return project_vertex(pc.model, pos, normal, uv, tangent);
}

// Instanced path: the model matrix arrives as four per-instance vec4 columns (locations 4-7) from
// an instance-rate vertex buffer — one draw call renders every placement of a repeated module
// (walls, rubble, pines…). params still ride the push constants, set once per batch.
@vertex
fn vs_inst(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
           @location(3) tangent: vec4<f32>,
           @location(4) i0: vec4<f32>, @location(5) i1: vec4<f32>,
           @location(6) i2: vec4<f32>, @location(7) i3: vec4<f32>) -> VertexOut {
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

// Keep directional lighting continuous. Ordered display-palette coverage belongs in crush,
// after lamps, sky, reflections and bloom have been combined; upstream bands cannot be undone.
// Every source is in renderer units: the probe's physical environment and beam, the positioned
// emitters' radiant intensity, the authored emission. BRDFs are energy-normalised (Lambert
// albedo / pi), as in combine.wgsl.

// Directional albedo of the GGX specular lobe under uniform incidence (the split-sum environment
// BRDF; Karis 2014's analytic fit to its integral), so a rough surface seen at grazing angles
// reflects its environment as weakly as the ray-traced GGX estimator finds.
fn environment_brdf(f0: vec3<f32>, roughness: f32, nv: f32) -> vec3<f32> {
    let r = roughness * vec4(-1.0, -0.0275, -0.572, 0.022) + vec4(1.0, 0.0425, 1.04, -0.04);
    let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    let ab = vec2(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + vec3(ab.y);
}

// Reflected radiance per unit irradiance from direction `light` (BRDF x cosine).
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
    let specular = distribution_ggx(nh, roughness) * geometry_smith(nv, nl, roughness)
        * fresnel / max(4.0 * nv * nl, 0.001);
    return ((vec3(1.0) - fresnel) * (1.0 - metallic) * albedo / PI + specular) * nl;
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
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let result=shade(in);if(scene_visibility_unresolved){return vec4(1000.0,0.0,1000.0,1.0);}
    return result.color;
}
@fragment fn fs_ambient(in: VsOut) -> SurfaceOutput {
    let result=shade(in);if(scene_visibility_unresolved){return unoccluded(vec4(1000.0,0.0,1000.0,1.0));}
    return result;
}
// A filtered unit-normal map carries unresolved angular spread in its vector length.
// Toksvig's (1-L)/L estimate is mapped to the GGX width here as an isotropic
// approximation, not an exact convolution (GGX has long tails). Preserve the
// authored roughness for constant normals within RGB8 decoding quantization.
// No extra bias, camera-distance roughness, or presentation blur is applied.
fn filtered_normal_roughness(roughness: f32, filtered_length: f32) -> f32 {
    let quantization_bound = 0.006792356; // sqrt(3)/255, half a decoded RGB8 step per axis
    let l = clamp(filtered_length, 0.0001, 1.0);
    let angular_spread = max(1.0 - l - quantization_bound, 0.0) / l;
    let alpha = roughness * roughness;
    return pow(min(alpha * alpha + angular_spread, 1.0), 0.25);
}

fn shade(in: VsOut) -> SurfaceOutput {
    // Resolve channels before divergent branches. Albedo/material texels retain their sampler;
    // normals use linear within-level and mip filtering at the actual derivative footprint.
    let raw4 = textureSample(albedo_tex, samp, in.uv);
    let mr = textureSample(mr_tex, samp, in.uv);
    let mapped_normal = textureSample(normal_tex, terrain_detail_sampler, in.uv).rgb * 2.0 - vec3(1.0);
    let mapped_normal_length = length(mapped_normal);
    let macro_dx = dpdx(in.uv) * vec2<f32>(textureDimensions(albedo_tex));
    let macro_dy = dpdy(in.uv) * vec2<f32>(textureDimensions(albedo_tex));
    let macro_lod = log2(max(max(length(macro_dx), length(macro_dy)), 1.0));
    let plane_dx = dpdx(in.plane_relative_world);
    let plane_dy = dpdy(in.plane_relative_world);
    let footprint = max(length(plane_dx.xz), length(plane_dy.xz));
    // The shadow receiver is the rasterized triangle, not its interpolated shading normal.
    // Evaluate derivatives before any divergent material branch.
    // Absolute world interpolation quantizes sub-mm screen derivatives at battlefield
    // coordinates. This is the same rasterized plane, expressed relative to the eye.
    // Normalize the basis before crossing so degeneracy is angular, not a world-area cutoff.
    // A 25 mm eye-relief view has ~14 um pixels at 450p/14 degrees; its valid plane used to
    // fail the fixed m^4 threshold and feed the smoothed shading normal into shadow PCF.
    let plane_dx_sq = dot(plane_dx,plane_dx);
    let plane_dy_sq = dot(plane_dy,plane_dy);
    let face_cross = cross(plane_dx*inverseSqrt(max(plane_dx_sq,1e-30)),
                           plane_dy*inverseSqrt(max(plane_dy_sq,1e-30)));
    let sparse_cubic_smoke = pc.params.w < -0.5;
    if (sparse_cubic_smoke && distance(in.world, cam.cam_pos) < 0.16) { discard; }
    if (pc.params.w > 3.5 && pc.params.w < 4.5 && raw4.a < 0.5) { discard; }

    let view = normalize(-in.relative_world);
    let geometric_normal = normalize(in.normal);
    let face_length_sq = dot(face_cross, face_cross);
    let face_normal = face_cross * inverseSqrt(max(face_length_sq, 1e-20));
    let receiver_normal = select(geometric_normal,
        face_normal * select(-1.0, 1.0, dot(face_normal, geometric_normal) >= 0.0),
        plane_dx_sq>1e-30 && plane_dy_sq>1e-30 && face_length_sq>1e-12);
    // A transparent pane: premultiplied Fresnel reflection of the physical environment over the
    // transmitted scene. The pane emits nothing of its own.
    if (pc.params.w > 7.5) {
        let n = geometric_normal * select(-1.0, 1.0, dot(geometric_normal, view) >= 0.0);
        let fresnel = pow(1.0 - max(dot(n, view), 0.0), 3.0);
        let alpha = 0.025 + 0.18 * fresnel;
        let sheen = environment_light(in.world, receiver_normal, n, reflect(-view, n), 6.0).glossy;
        return SurfaceOutput(vec4(sheen * alpha, alpha), vec4(0.0, 0.0, 0.0, alpha));
    }
    // Authored emission, exactly as gbuf.wgsl writes it for the ray-traced path.
    if (!sparse_cubic_smoke && pc.params.x > 0.0) {
        let x = pc.params.x;
        if (pc.params.w > 1.5 && pc.params.w < 2.5) {
            // Lamp/status/foam own colour.
            return glowing(vec4(raw4.rgb * x + raw4.rgb * max(x - 3.0, 0.0) * 0.15, 1.0));
        }
        if (pc.params.w > 0.5 && pc.params.w < 1.5) {
            let nf = clamp(cam.night, 0.0, 1.0);
            return glowing(vec4((raw4.rgb * x + raw4.rgb * max(x - 3.0, 0.0) * 0.18) * nf, 1.0));
        }
        if (pc.params.w < 0.5) {
            let fire_color = mix(vec3(1.0, 0.24, 0.03), raw4.rgb, 0.18);
            return unoccluded(vec4(fire_color * x * 0.30
                + vec3(1.0, 0.88, 0.62) * max(x - 6.0, 0.0) * 0.18, 1.0));
        }
    }

    let terrain = pc.params.w > 2.5 && pc.params.w < 3.5;
    // The ground is a coarse mesh (cells of several metres) standing in for a smooth surface.
    // Asked from exactly on a facet, the sky and the Sun are hidden or shown by that one facet's
    // tilt, and every triangle shows as its own shade. The visibility question is asked from a
    // little above the smooth surface instead, clear of the facets' own planes. A receiver bias,
    // measured in metres; it removes contact shadow on the ground below that height.
    let visibility_origin = in.world + select(vec3(0.0), geometric_normal * TERRAIN_VISIBILITY_LIFT_M, terrain);
    var albedo = raw4.rgb;
    var roughness = clamp(mr.g * pc.params.z, 0.12, 1.0);
    let metallic = clamp(mr.b * pc.params.y, 0.0, 1.0);
    var normal = geometric_normal;
    let tangent = in.tangent.xyz - normal * dot(normal, in.tangent.xyz);
    if (dot(tangent, tangent) > 1e-8) {
        let t = normalize(tangent);
        // A fully cancelling footprint has no preferred tangent-space direction.
        if (mapped_normal_length > 0.0001) {
            normal = normalize(mat3x3<f32>(t, cross(normal, t) * in.tangent.w, normal) * mapped_normal);
        }
        roughness = filtered_normal_roughness(roughness, mapped_normal_length);
    }
    if (terrain) {
        albedo = terrain_material(textureSampleLevel(albedo_tex, terrain_detail_sampler,
            in.uv, macro_lod), in.world, geometric_normal.y, cam.atmosphere.z, footprint);
        roughness = terrain_roughness(cam.atmosphere.z);
        if (cam.track_half > 1.0) {
            let track = sample_track(in.world.xz);
            albedo *= mix(vec3(1.0), vec3(0.52, 0.54, 0.56), clamp(track * 1.4, 0.0, 1.0));
        }
    }
    let mirror = reflect(-view, normal);
    let environment = environment_light(visibility_origin, receiver_normal, normal, mirror,
        mix(6.0, 1.0, roughness));
    if (sparse_cubic_smoke) {
        return unoccluded(vec4(albedo / PI * environment.irradiance, 1.0));
    }

    let f0 = mix(vec3(0.04), albedo, metallic);
    let nv = max(dot(normal, view), 0.0);
    let fresnel = fresnel_schlick(nv, f0);
    // Diffuse environment (Lambert, with combine.wgsl's (1 - F) share) plus the environment's
    // lobe radiance weighted by the GGX directional albedo.
    var color = (vec3(1.0) - fresnel) * (1.0 - metallic) * albedo / PI * environment.irradiance;
    color += environment_brdf(f0, roughness, nv) * environment.glossy;

    let ambient = color;
    // The key body's beam (the Sun, or the Moon once the Sun has set) at the camera, through the
    // shadow cascades of the same direction.
    let key = normalize(cam.sun_dir);
    let beam = probe.lanes[32].xyz;
    if(dot(normal,key)>0.0 && any(beam>vec3(0.0))){
        color += surface_response(albedo, metallic, roughness, normal, view, key) * beam
            * sun_shadow(visibility_origin, receiver_normal, max(dot(receiver_normal, key), 0.0));
    }

    // Finite spherical emitters, inverse-square in their radiant intensity. Every admitted source
    // has source-geometry visibility, including transients, world lamps and the handheld torch.
    let light_count = min(max(i32(cam.n_lights), 0), 32);
    for (var index = 0; index < light_count; index += 1) {
        let source = cam.lights[index * 2];
        let emission = cam.lights[index * 2 + 1];
        // Keep the interpolated surface detail before adding the camera-to-lamp translation.
        // The source map uses the matching lamp-relative vertex construction.
        let receiver_offset = in.relative_world + (cam.cam_pos - source.xyz);
        let offset = -receiver_offset;
        let d2 = dot(offset, offset);
        if (d2 < 1e-8 || emission.w <= 0.0 || all(emission.rgb==vec3(0.0))) { continue; }
        let direction = offset * inverseSqrt(d2);
        if(dot(normal,direction)<=0.0){continue;}
        let irradiance = emission.rgb * emission.w / max(d2, max(source.w * source.w, 0.0025));
        let visibility=cabin_light_shadow(source.xyz,source.w,in.world,receiver_offset,receiver_normal);
        if(all(visibility<=vec3(0.0))){continue;}
        color += surface_response(albedo, metallic, roughness, normal, view, direction) * irradiance * visibility
            *source_background_medium_transmittance(in.world,direction,max(sqrt(d2)-source.w,0.0));
    }
    if(scene_visibility_unresolved){return unoccluded(vec4(1000.0,0.0,1000.0,1.0));}
    return SurfaceOutput(vec4(max(color, vec3(0.0)), 1.0), vec4(ambient, 0.0));
}

@fragment fn fs_hover() -> @location(0) vec4<f32> { return vec4(1.0); }
