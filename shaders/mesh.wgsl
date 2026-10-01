// @camera-relative-projection
// Portable forward raster pass. Per-draw model matrix + params via push constants. PS1 read comes from the
// low internal res + the crush (posterize/dither/nearest upscale) — the vertex snap was removed
// (it mangled close-range/interior geometry); affine texture warp remains. Cook-Torrance PBR
// (GGX/Smith/Schlick) currently carries only shadow-mapped direct sun. Indirect/environment and
// local-emitter transport stay absent until the exact software triangle BVH lands; this shader may
// not replace them with ambient fill or unshadowed point lights. Emissive materials output their own
// radiance and receive bloom downstream.

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

// Sun shadow map: single ortho depth cascade rendered by shadow.wgsl. Sampled with a comparison
// sampler for hardware PCF, then hand-widened to a 3×3 kernel below.
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;

// World tank-track rut map (R8, additive tread stamps; renderer::stamp_tank_tracks). Terrain
// (params.w == 3) darkens + dip-shades where tanks have driven — the depression is a LIGHTING
// dip (gradient-perturbed normal), which under the 480×270 crush reads as real displacement.
@group(3) @binding(0) var track_tex: texture_2d<f32>;
@group(3) @binding(1) var track_samp: sampler;

// @campaign-terrain-material

// Shadow term for the sun's DIRECT light: 1 = fully lit, 0 = fully occluded. Uses a normal/slope
// depth bias to kill acne and a 3×3
// PCF kernel (on top of the sampler's own 2×2) so the edge reads soft, not stair-stepped at 480×270.
fn sun_shadow(world: vec3<f32>, n: vec3<f32>, NdotL: f32) -> f32 {
    let lc = cam.light_view_proj * vec4(world, 1.0);
    // Orthographic → w == 1, but divide anyway for correctness.
    let proj = lc.xyz / lc.w;
    // NDC.xy (−1..1, y up) → shadow-map UV (0..1, y down).
    let uv = proj.xy * vec2(0.5, -0.5) + vec2(0.5, 0.5);
    let depth = proj.z;

    // Outside the cascade footprint or behind the far plane → treat as lit (no phantom shadow on
    // distant terrain the map doesn't cover).
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || depth > 1.0) {
        return 1.0;
    }

    // Slope-scaled depth bias: grazing angles (small NdotL) need more bias to avoid shadow acne;
    // steep angles need little, so peter-panning stays minimal.
    let bias = clamp(0.0012 * tan(acos(clamp(NdotL, 0.0, 1.0))), 0.0004, 0.0035);
    let ref_depth = depth - bias;

    let dims = vec2<f32>(textureDimensions(shadow_map));
    let texel = 1.0 / dims;
    var sum = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let off = vec2<f32>(f32(x), f32(y)) * texel;
            sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + off, ref_depth);
        }
    }
    return sum / 9.0;
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
    @location(0) @interpolate(linear) uv: vec2<f32>, // affine warp
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

// The cold blue AIR the world dissolves into with distance (ART_BIBLE §4a). This is where the
// winter blue now lives — a luminous cool blue in-scatter that grows with range, so far geometry
// and the horizon wash blue while NEAR surfaces stay neutral steel. Kept in step with sky.wgsl's
// mid/horizon band so terrain fading to fog blends seamlessly into the sky.
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

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let raw4 = textureSample(albedo_tex, samp, in.uv);
    let raw = raw4.rgb;
    // Compute footprints before material branches. Macro colour is bilinear/trilinear only on
    // terrain; source prop textures retain their existing nearest texel presentation.
    let terrain_uv_dx = dpdx(in.uv) * vec2<f32>(textureDimensions(albedo_tex));
    let terrain_uv_dy = dpdy(in.uv) * vec2<f32>(textureDimensions(albedo_tex));
    let terrain_macro_lod = log2(max(max(length(terrain_uv_dx), length(terrain_uv_dy)), 1.0));
    let terrain_footprint = max(length(dpdx(in.world.xz)), length(dpdy(in.world.xz)));
    let sparse_cubic_smoke = pc.params.w < -0.5;
    // A microscopic opaque soot carrier cannot intersect the eye/near plane as a screen-sized
    // slab. This is per-view clipping only: world position and5–7mm cube size never change.
    if (sparse_cubic_smoke && distance(in.world, cam.cam_pos) < 0.16) { discard; }

    // SMOOTH GLASS (params.w == 8): the vision-port panes — near-clear laminate drawn in its own
    // premultiplied-blend pass AFTER the opaque scene (depth read-only), so the world shows
    // through undisturbed. A fresnel sheen rises at grazing angles, the sun throws a hard little
    // glint across the pane, and a faint cool tint says "thick laminated block".
    if (pc.params.w > 7.5) {
        let n0 = normalize(in.normal);
        let v = normalize(cam.cam_pos - in.world);
        let n = n0 * sign(dot(n0, v) + 1e-4); // panes are looked through from both faces
        let ndv = clamp(dot(n, v), 0.0, 1.0);
        let fres = pow(1.0 - ndv, 3.0);
        let a = 0.05 + 0.30 * fres;
        let glint = pow(max(dot(reflect(-v, n), normalize(cam.sun_dir)), 0.0), 120.0);
        let tint = vec3(0.55, 0.68, 0.66); // cool laminate green-grey
        let rgb = tint * a + vec3(1.0, 0.97, 0.90) * glint * 0.6;
        return vec4(rgb, a);
    }
    // GRASS (params.w == 4): alpha-test the blade texture — hard PS1 cutout, no sorting.
    if (pc.params.w > 3.5 && pc.params.w < 4.5 && raw4.a < 0.5) {
        discard;
    }

    // TERRAIN (params.w == 3): sample the tank-track rut map once + its gradient for the dip.
    var track = 0.0;
    var track_grad = vec2(0.0);
    if (pc.params.w > 2.5 && pc.params.w < 3.5 && cam.track_half > 1.0) {
        let tuv = in.world.xz / (2.0 * cam.track_half) + vec2(0.5);
        let texel = 1.0 / 4096.0;
        track = textureSampleLevel(track_tex, track_samp, tuv, 0.0).r;
        let tx = textureSampleLevel(track_tex, track_samp, tuv + vec2(texel, 0.0), 0.0).r
               - textureSampleLevel(track_tex, track_samp, tuv - vec2(texel, 0.0), 0.0).r;
        let tz = textureSampleLevel(track_tex, track_samp, tuv + vec2(0.0, texel), 0.0).r
               - textureSampleLevel(track_tex, track_samp, tuv - vec2(0.0, texel), 0.0).r;
        track_grad = vec2(tx, tz);
    }

    if (!sparse_cubic_smoke && pc.params.x > 0.0 && pc.params.w > 1.5 && pc.params.w < 2.5) {
        // OWN-COLOUR emissive, NOT night-gated: the warm interior bulb (amber tungsten) and the
        // near-miss snow/dust puff (white-grey) both take this path — the mesh's authored colour
        // driven straight to HDR so it blooms softly and takes the crush, fading via the emissive
        // intensity the game animates. A brighter core lifts the densest centre toward incandescent.
        var oc = raw * pc.params.x;
        oc = oc + raw * max(pc.params.x - 3.0, 0.0) * 0.15;
        return vec4(oc, 1.0);
    }

    if (!sparse_cubic_smoke && pc.params.x > 0.0 && pc.params.w > 0.5 && pc.params.w < 1.5) {
        // NIGHT window / streetlamp: glow with the mesh's OWN authored colour (a mosaic of warm-
        // yellow / cold-blue / teal lit windows + warm lamp heads), gated by the night factor so the
        // pane is dark at Tag and blooming at Nacht. Unlit — driven straight into HDR so the halo
        // reads through the downstream bloom + crush against the dark residential night.
        let n = clamp(cam.night, 0.0, 1.0);
        var wcol = raw * pc.params.x * n;
        // A soft own-colour core on the brightest panes/lamps so their centre reads incandescent.
        wcol = wcol + raw * max(pc.params.x - 3.0, 0.0) * 0.18 * n;
        return vec4(wcol, 1.0);
    }

    if (!sparse_cubic_smoke && pc.params.x > 0.0 && pc.params.w < 0.5) {
        // Gunfire: searing saturated orange with a white-hot core, driven hard into HDR so it
        // blooms through the crush and pops against the cold blue world (ART_BIBLE §2). Unlit —
        // never PBR-shaded, so it stays bright regardless of any material data on the FX mesh.
        let orange = vec3(1.0, 0.24, 0.03); // saturated fire orange/red
        let tinted = mix(orange, raw, 0.18);  // mostly orange, a little of the source hue
        // Scale the raw FX intensity down so the tonemap keeps the HUE instead of clipping every
        // channel to flat white — the body stays a searing saturated orange.
        var col = tinted * pc.params.x * 0.30;
        // A restrained white-hot bias only on the very brightest flashes (muzzle / big impact),
        // so their centre reads incandescent without washing the whole box to white.
        col = col + vec3(1.0, 0.88, 0.62) * max(pc.params.x - 6.0, 0.0) * 0.18;
        return vec4(col, 1.0);
    }

    // The sampled sRGB texture has already been decoded to linear by the texture format. Preserve
    // source albedo exactly; visibility comes from transport rather than a runtime gamma lift.
    var albedo = raw;
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        albedo = terrain_material(textureSampleLevel(albedo_tex, terrain_detail_sampler,
                                   in.uv, terrain_macro_lod), in.world, normalize(in.normal).y,
                                   cam.atmosphere.z, terrain_footprint);
    }

    // Tank ruts: compressed snow/dirt reads darker and cooler where treads have been.
    if (track > 0.004) {
        let t = clamp(track * 1.4, 0.0, 1.0);
        albedo = albedo * mix(vec3(1.0), vec3(0.50, 0.53, 0.60), t);
    }

    // ---- tangent-space normal mapping ----
    let geo_n = normalize(in.normal);
    let T = normalize(in.tangent.xyz - geo_n * dot(geo_n, in.tangent.xyz)); // re-orthogonalize
    let B = cross(geo_n, T) * in.tangent.w;
    let TBN = mat3x3<f32>(T, B, geo_n);
    let nsample = textureSample(normal_tex, samp, in.uv).rgb * 2.0 - vec3(1.0);
    var n = normalize(TBN * nsample); // flat (0,0,1) fallback texel resolves to geo_n exactly
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        n = normalize(n + terrain_micro_normal(
            in.world, max(fwidth(in.world.x), fwidth(in.world.z))));
    }
    // Rut dip: tilt the normal along the track-depth gradient so lighting shades a depression
    // (equivalent to displacing by -depth without needing vertex density the terrain lacks).
    if (track > 0.004) {
        n = normalize(n + vec3(track_grad.x, 0.0, track_grad.y) * 6.0);
    }

    // ---- metallic-roughness ----
    let mr = textureSample(mr_tex, samp, in.uv);
    var roughness = clamp(mr.g * pc.params.z, 0.045, 1.0);
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        // Act II is wettest; winter crust and Act-III soil stay mostly matte.
        roughness = terrain_roughness(cam.atmosphere.z);
    }
    let metallic = clamp(mr.b * pc.params.y, 0.0, 1.0);
    let F0 = mix(vec3(0.04), albedo, metallic); // dielectric base reflectance vs. metal (tinted)

    // Low winter sun from the west-ish; cool sky-bounce fill. More directional than before so
    // form reads (snow slopes, tank facets) instead of flat grey. Now lifted to the shared uniform
    // so the shadow pass projects from the SAME direction we shade with. Real GI replaces this in M-D.
    let sun_dir = normalize(cam.sun_dir);
    let view_dir = normalize(cam.cam_pos - in.world);
    let half_dir = normalize(view_dir + sun_dir);

    let NdotL = max(dot(n, sun_dir), 0.0);
    let NdotV = max(dot(n, view_dir), 1e-4);
    let NdotH = max(dot(n, half_dir), 0.0);
    let VdotH = max(dot(view_dir, half_dir), 0.0);

    // Pale cold winter sun — near-white, faintly warm so lit snow separates from the blue shadow.
    // Kept at the SAME magnitude as the old flat-Lambert term (not premultiplied by 1/PI) so a
    // fully-rough dielectric (the non-PBR default) reproduces very close to the old exposure —
    // only the new specular lobe and metals add brightness beyond that baseline.
    // NACHT: the key light drops to a DIM, COOL moonlight from the same direction — enough to model
    // the snow slopes / tank facets in silver-blue, low enough that the scene goes dark and moonlit
    // so the emissive windows + lamp halos carry the frame.
    let day_sun = vec3(1.20, 1.18, 1.10) * 1.45;
    let moon_sun = vec3(0.21, 0.22, 0.23); // neutral storm moonlight, no electric-blue wash
    let cloud_key = mix(1.0, 0.42, clamp(cam.atmosphere.x, 0.0, 1.0));
    let sun_radiance = mix(day_sun, moon_sun, clamp(cam.night, 0.0, 1.0)) * cloud_key;

    let D = distribution_ggx(NdotH, roughness);
    let G = geometry_smith(NdotV, NdotL, roughness);
    let F = fresnel_schlick(VdotH, F0);
    let spec = (D * G * F) / max(4.0 * NdotV * NdotL, 1e-4);

    let kd = (vec3(1.0) - F) * (1.0 - metallic); // metals: no diffuse lobe
    let diffuse = kd * albedo;

    // The source-geometry shadow map occludes direct sunlight. Missing indirect transport remains
    // missing instead of being replaced with a view/campaign-tuned constant.
    let shadow = sun_shadow(in.world, n, NdotL);
    let direct = (diffuse + spec) * sun_radiance * NdotL * shadow;

    var col = direct;

    return vec4(col, 1.0);
}
