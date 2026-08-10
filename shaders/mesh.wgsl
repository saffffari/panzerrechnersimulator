// Raster G-buffer pass. Per-draw model matrix + params via push constants. PS1 read comes from the
// low internal res + the crush (posterize/dither/nearest upscale) — the vertex snap was removed
// (it mangled close-range/interior geometry); affine texture warp remains. Cook-Torrance PBR
// (GGX/Smith/Schlick) direct sun light + the cold
// sky/ground ambient fill (General Winter grading). Emissive path (params.x>0) outputs bright HDR
// directly so muzzle flash / tracers / fires bloom through the crush — unlit, not PBR-shaded.

struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>, // sun ortho cascade — shared with shadow.wgsl so both agree
    cam_pos: vec3<f32>,
    fog_density: f32,
    sun_dir: vec3<f32>,           // TO the sun (lit dir); single source of truth for shading + shadow
    night: f32,                   // 0 = day (Tag), 1 = night (Nacht) — moonlight + emissive-window gate
    snap_res: vec2<f32>,
    interior_light: f32,          // 0 = open/exposed, 1 = buttoned-up inside — warm tungsten fill
    time: f32,                    // sim seconds — grass wind phase
    track_half: f32,              // tank-track map world half-extent (m); 0 = no track sampling
    n_lights: f32,                // active positioned point lights
    _pad2: f32,
    _pad3: f32,
    // Point lights: 2 vec4 each — [pos.xyz, radius], [color.rgb, intensity]. N_LIGHTS=32 → 64 vec4.
    lights: array<vec4<f32>, 64>,
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

// Shadow term for the sun's DIRECT light: 1 = fully lit, 0 = fully occluded. Only the direct term is
// multiplied by this — the cold hemispheric ambient stays unshadowed so shaded areas fall to deep
// cold-blue, not pure black (winter grade). Uses a normal/slope depth bias to kill acne and a 3×3
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
    // w = reserved. Non-PBR meshes carry sane fallback factors baked in on the CPU side
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

// Shared vertex body: transform by `model`, PS1 vertex-snap in NDC, hand the world-space frame to
// the fragment stage. Called by both the push-constant path (`vs`) and the instanced path
// (`vs_inst`, per-instance model matrix from the instance buffer).
fn project_vertex(model: mat4x4<f32>, pos: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>,
                  tangent: vec4<f32>) -> VsOut {
    let world = (model * vec4(pos, 1.0)).xyz;
    let clip = cam.view_proj * vec4(world, 1.0);

    // (The PS1 vertex snap used to live here — REMOVED: quantizing NDC onto the pixel grid mangled
    // close-range geometry (the whole interior) into shearing polygons. The retro read comes from
    // the 640×360 internal res + posterize/dither/nearest-upscale; geometry renders faithfully.)

    var out: VsOut;
    out.clip = clip;
    out.uv = uv;
    out.normal = normalize((model * vec4(normal, 0.0)).xyz);
    out.world = world;
    out.tangent = vec4(normalize((model * vec4(tangent.xyz, 0.0)).xyz), tangent.w);
    return out;
}

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) tangent: vec4<f32>) -> VsOut {
    var p = pos;
    // OUTLINE shell (params.w == 6): inflate along the local normal so the front-face-culled
    // back shell pokes out around the silhouette as a rim (an inverted-hull outline).
    if (pc.params.w > 5.5 && pc.params.w < 6.5) {
        p = pos + normal * 0.010; // half-width tactile rim; close interior props no longer wear halos
    }
    return project_vertex(pc.model, p, normal, uv, tangent);
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
    if (pc.params.w > 3.5) {
        // GRASS: wind sway. The per-blade wind factor rides the instance matrix's [3][3] slot
        // (openness-baked at scatter time: open fields ripple, forest floor holds still). Two
        // stacked sines make gusts; only the blade's upper part bends (local y).
        let wind = i3.w;
        let ph = cam.time * 2.3 + i3.x * 0.31 + i3.z * 0.23;
        let gust = sin(ph) * 0.7 + sin(ph * 2.63 + 1.7) * 0.3;
        let bend = clamp(p.y / 0.5, 0.0, 1.0);
        p.x += gust * 0.16 * wind * bend;
        p.z += cos(ph * 0.77) * 0.06 * wind * bend;
    }
    return project_vertex(mat4x4<f32>(i0, i1, i2, vec4(i3.xyz, 1.0)), p, normal, uv, tangent);
}

// The cold blue AIR the world dissolves into with distance (ART_BIBLE §4a). This is where the
// winter blue now lives — a luminous cool blue in-scatter that grows with range, so far geometry
// and the horizon wash blue while NEAR surfaces stay neutral steel. Kept in step with sky.wgsl's
// mid/horizon band so terrain fading to fog blends seamlessly into the sky.
const FOG_COLOR = vec3<f32>(0.32, 0.44, 0.72);
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
    let dist = length(in.world - cam.cam_pos);

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
    // OUTLINE (params.w == 6): a flat emissive rim, colour · intensity(params.x). HDR-bright so
    // the bloom pass haloes it and the interior smoke veil scatters its glow.
    // SCHUTZGLAS (params.w == 7): laminated vision-block glass. A stable, irregular stipple avoids
    // the fake uniform checker grid while retaining order-independent PS1 transparency.
    if (pc.params.w > 6.5) {
        let px = vec2<u32>(u32(in.clip.x), u32(in.clip.y));
        let wp = vec3<u32>(abs(vec3<i32>(floor(in.world * 29.0))));
        let h = (px.x * 1597334677u) ^ (px.y * 3812015801u)
            ^ (wp.x * 95868913u) ^ (wp.y * 1103515245u) ^ (wp.z * 2654435761u);
        if ((h ^ (h >> 13u)) % 11u) > 3u {
            discard;
        }
        let g = dot(raw, vec3(0.299, 0.587, 0.114));
        let laminate = 0.88 + 0.12 * f32((h >> 8u) & 7u) / 7.0;
        return vec4(vec3(0.42, 0.72, 0.55) * (0.45 + 0.75 * g) * laminate, 1.0);
    }
    if (pc.params.w > 5.5) {
        return vec4(vec3(1.0, 0.72, 0.30) * (2.2 * pc.params.x), 1.0);
    }
    // GRASS (params.w == 4): alpha-test the blade texture — hard PS1 cutout, no sorting.
    if (pc.params.w > 4.5) {
        // X-RAY (params.w == 5): kill-cam ghost hull — PS1 screen-door transparency (checkerboard
        // discard, no blending/sorting) + a flat cool schematic shade, so the interior proxies and
        // spall cone read straight through the skin. Distance-fogless: the kill-cam is close.
        let px = vec2<u32>(u32(in.clip.x), u32(in.clip.y));
        if ((px.x + px.y) % 2u == 0u) {
            discard;
        }
        let g = dot(raw, vec3(0.299, 0.587, 0.114));
        return vec4(vec3(0.30, 0.38, 0.52) * (0.30 + 0.55 * g), 1.0);
    }
    if (pc.params.w > 3.5 && raw4.a < 0.5) {
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

    if (pc.params.x > 0.0 && pc.params.w > 1.5) {
        // OWN-COLOUR emissive, NOT night-gated: the warm interior bulb (amber tungsten) and the
        // near-miss snow/dust puff (white-grey) both take this path — the mesh's authored colour
        // driven straight to HDR so it blooms softly and takes the crush, fading via the emissive
        // intensity the game animates. A brighter core lifts the densest centre toward incandescent.
        var oc = raw * pc.params.x;
        oc = oc + raw * max(pc.params.x - 3.0, 0.0) * 0.15;
        // A whisper of atmosphere so a puff / bulb sits in the air, not pasted flat.
        let ofog = (1.0 - exp(-cam.fog_density * dist * 0.4)) * 0.2;
        oc = mix(oc, FOG_COLOR, ofog);
        return vec4(oc, 1.0);
    }

    if (pc.params.x > 0.0 && pc.params.w > 0.5) {
        // NIGHT window / streetlamp: glow with the mesh's OWN authored colour (a mosaic of warm-
        // yellow / cold-blue / teal lit windows + warm lamp heads), gated by the night factor so the
        // pane is dark at Tag and blooming at Nacht. Unlit — driven straight into HDR so the halo
        // reads through the downstream bloom + crush against the dark residential night.
        let n = clamp(cam.night, 0.0, 1.0);
        var wcol = raw * pc.params.x * n;
        // A soft own-colour core on the brightest panes/lamps so their centre reads incandescent.
        wcol = wcol + raw * max(pc.params.x - 3.0, 0.0) * 0.18 * n;
        // Whisper of night air so a distant lit window sits in the atmosphere, not floating flat.
        let wfog = (1.0 - exp(-cam.fog_density * dist * 0.5)) * 0.25;
        wcol = mix(wcol, vec3(0.02, 0.03, 0.06), wfog);
        return vec4(wcol, 1.0);
    }

    if (pc.params.x > 0.0) {
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
        // Gunfire punches through the fog — only a whisper of atmosphere so it stays searing hot
        // orange at any distance instead of washing out to cold blue.
        let fog = (1.0 - exp(-cam.fog_density * dist * 0.25)) * 0.3;
        col = mix(col, FOG_COLOR, fog);
        return vec4(col, 1.0);
    }

    // PSX-pack albedos are authored DARK (see memory). Lift as a stand-in for the bake re-grade.
    var albedo = pow(raw, vec3(0.65)) * 1.5;

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
    // Rut dip: tilt the normal along the track-depth gradient so lighting shades a depression
    // (equivalent to displacing by -depth without needing vertex density the terrain lacks).
    if (track > 0.004) {
        n = normalize(n + vec3(track_grad.x, 0.0, track_grad.y) * 6.0);
    }

    // ---- metallic-roughness ----
    let mr = textureSample(mr_tex, samp, in.uv);
    let roughness = clamp(mr.g * pc.params.z, 0.045, 1.0);
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
    let moon_sun = vec3(0.15, 0.19, 0.32); // cool moonlight — enough to read the city, still night
    let sun_radiance = mix(day_sun, moon_sun, clamp(cam.night, 0.0, 1.0));

    let D = distribution_ggx(NdotH, roughness);
    let G = geometry_smith(NdotV, NdotL, roughness);
    let F = fresnel_schlick(VdotH, F0);
    let spec = (D * G * F) / max(4.0 * NdotV * NdotL, 1e-4);

    let kd = (vec3(1.0) - F) * (1.0 - metallic); // metals: no diffuse lobe
    let diffuse = kd * albedo;

    // Sun shadow: occludes the DIRECT term only (ambient stays, so shadows go cold-blue not black).
    // This is what darkens the tank's shaded side + undercarriage and lands a cast shadow on snow.
    let shadow = sun_shadow(in.world, n, NdotL);
    let direct = (diffuse + spec) * sun_radiance * NdotL * shadow;

    // Cold sky/ground ambient fill (General Winter grading, ART_BIBLE §3), extended with a crude
    // ambient specular tint for metals (so shadowed steel doesn't read pure-black) instead of the
    // old flat multiply. DESATURATED on purpose: close/interior surfaces read as near-neutral
    // steel-grey with only a faint cool tint (B just above R) — the saturated winter BLUE now lives
    // in the atmosphere (FOG_COLOR below), not as paint on nearby geometry. Still cold in feel, not
    // a blue wash. Ambient stays unshadowed so shaded areas fall to a cool neutral, not black.
    let sky = clamp(n.y * 0.5 + 0.5, 0.0, 1.0); // up faces catch more cold sky
    // NACHT: the fill drops to a low, cool moonlit ambient — surfaces stay readable (not pure black)
    // but dark, so the lit windows / lamp halos are what the eye goes to.
    let day_amb = mix(vec3(0.10, 0.11, 0.13), vec3(0.23, 0.25, 0.29), sky);
    let night_amb = mix(vec3(0.030, 0.038, 0.062), vec3(0.062, 0.082, 0.130), sky);
    var cold_ambient = mix(day_amb, night_amb, clamp(cam.night, 0.0, 1.0));
    // TRUE INTERIOR COLORS (playtest): the cold-blue SKY ambient is fake light inside a sealed
    // hull. Buttoned up, near geometry drops it to a small NEUTRAL remnant — the dome bulb, the
    // aperture point lights, and the warm fill below are the only real interior light.
    let interior_kill = clamp(cam.interior_light, 0.0, 1.0) * exp(-dist * 0.11);
    let neutral_amb = vec3(dot(cold_ambient, vec3(0.3333)));
    cold_ambient = mix(cold_ambient, neutral_amb * 0.35, interior_kill);
    let ambient = cold_ambient * mix(albedo, F0, metallic);

    var col = ambient + direct;

    // ── POSITIONED POINT LIGHTS: the bulb, the cupola periscope apertures, a muzzle flash, fire
    // glow. Each adds a diffuse+spec lobe with a smooth radius falloff. Cheap — a handful of lights,
    // evaluated at internal res. This is what lights the buttoned-up interior (real aperture light,
    // not the old screen-distance fill) and flares the tank when the gun fires. ──
    let nl = i32(cam.n_lights);
    for (var i = 0; i < nl; i = i + 1) {
        let pr = cam.lights[i * 2];
        let ci = cam.lights[i * 2 + 1];
        let lp = pr.xyz;
        let lr = max(pr.w, 0.01);
        let to = lp - in.world;
        let d = length(to);
        if (d < lr) {
            let ldir = to / max(d, 1e-4);
            let lNdotL = max(dot(n, ldir), 0.0);
            // Smooth inverse-square-ish falloff, hard zero at the radius.
            let fade = pow(clamp(1.0 - d / lr, 0.0, 1.0), 2.0);
            let lHalf = normalize(view_dir + ldir);
            let lNdotH = max(dot(n, lHalf), 0.0);
            let lVdotH = max(dot(view_dir, lHalf), 0.0);
            let lD = distribution_ggx(lNdotH, roughness);
            let lG = geometry_smith(NdotV, lNdotL, roughness);
            let lF = fresnel_schlick(lVdotH, F0);
            let lspec = (lD * lG * lF) / max(4.0 * NdotV * lNdotL, 1e-4);
            let lkd = (vec3(1.0) - lF) * (1.0 - metallic);
            col = col + (lkd * albedo + lspec) * ci.rgb * ci.w * lNdotL * fade;
        }
    }

    // The true lamps above own the interior. This is only enough neutral/tungsten bounce to stop
    // unlit polygons collapsing to absolute black; it must never compete with an aperture shaft.
    let interior = clamp(cam.interior_light, 0.0, 1.0);
    if (interior > 0.0) {
        let warm = vec3(0.90, 0.55, 0.25);   // amber tungsten
        let near = exp(-dist * 0.11);         // strongest within a few metres — the interior
        col = col + warm * albedo * (0.075 * near * interior);
    }

    // Aerial perspective: the cold winter BLUE is in-scattered by the air and accumulates with
    // distance. Near surfaces (low fog) keep their neutral steel; the far field and horizon wash to
    // a luminous cool blue. A second, slower ramp deepens the blue SATURATION of the deep distance,
    // so range reads as blue air receding rather than a flat grey veil — while brightness holds.
    let fog = 1.0 - exp(-cam.fog_density * dist);
    let deep_air = 1.0 - exp(-cam.fog_density * dist * 0.5); // saturates further out
    let air_day = mix(FOG_COLOR, vec3(0.26, 0.40, 0.80), clamp(deep_air, 0.0, 1.0));
    // NACHT: the distant air is a DARK cool blue-grey (matched to sky.wgsl's night horizon) so far
    // geometry recedes into darkness instead of washing to the bright daytime blue.
    let air = mix(air_day, vec3(0.010, 0.018, 0.042), clamp(cam.night, 0.0, 1.0));
    col = mix(col, air, clamp(fog, 0.0, 1.0));
    return vec4(col, 1.0);
}
