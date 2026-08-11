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

fn mood_color(winter: vec3<f32>, oder: vec3<f32>, halbe: vec3<f32>, mood: f32) -> vec3<f32> {
    let a = mix(winter, oder, smoothstep(0.0, 0.5, mood));
    return mix(a, halbe, smoothstep(0.5, 1.0, mood));
}

// Integer world-cell hash shared verbatim with gbuf.wgsl. Terrain detail must be stable between
// Metal forward and Vulkan deferred rendering; a sine hash produces subtly different thresholds.
fn terrain_hash(cell: vec2<i32>) -> f32 {
    var h = bitcast<u32>(cell.x) * 1664525u ^ bitcast<u32>(cell.y) * 1013904223u;
    h = (h ^ (h >> 16u)) * 2246822519u;
    h = (h ^ (h >> 13u)) * 3266489917u;
    return f32(h ^ (h >> 16u)) / 4294967295.0;
}

// The 8k campaign drapes are useful MACRO maps, but their source information is much coarser than
// their PNG dimensions. At foot level the old shader exposed those macro texels as multi-metre
// blocks. Re-palette that signal into an act material, then lay crisp deterministic world-space
// cells over it. Derivative gates retire each layer before it aliases in the distance.
fn terrain_material(raw: vec3<f32>, world: vec3<f32>, normal_y: f32) -> vec3<f32> {
    let mood = clamp(cam.atmosphere.z, 0.0, 1.0);
    let macro_luma = clamp(dot(raw, vec3(0.299, 0.587, 0.114)), 0.0, 1.0);
    let macro_shape = smoothstep(0.08, 0.90, macro_luma);

    let winter = mix(vec3(0.55, 0.56, 0.55), vec3(0.88, 0.89, 0.87), macro_shape);
    let oder = mix(vec3(0.17, 0.18, 0.145), vec3(0.39, 0.39, 0.30), macro_shape);
    let halbe = mix(vec3(0.15, 0.135, 0.12), vec3(0.36, 0.32, 0.255), macro_shape);
    var col = mood_color(winter, oder, halbe, mood);

    let footprint = max(fwidth(world.x), fwidth(world.z));
    let fine_vis = 1.0 - smoothstep(0.16, 0.48, footprint);
    let coarse_vis = 1.0 - smoothstep(0.80, 2.20, footprint);
    let fine_cell = vec2<i32>(floor(world.xz * 4.0));     // 25 cm pixel-material cells
    let coarse_cell = vec2<i32>(floor(world.xz * 0.70)); // metre-scale clods / drift crust
    let fine = terrain_hash(fine_cell) - 0.5;
    let coarse = terrain_hash(coarse_cell) - 0.5;
    col *= 1.0 + fine * 0.18 * fine_vis + coarse * 0.13 * coarse_vis;

    // Sparse hard-edged inclusions: dirty ice in Stalingrad, pale stone in Oderbruch, and ash in
    // Halbe. These are deliberately binary cells, not filtered noise, so the 640x360 silhouette
    // remains crisp after nearest-neighbour upscale.
    let fleck = select(0.0, 1.0, terrain_hash(fine_cell + vec2<i32>(71, -29)) > 0.925) * fine_vis;
    let fleck_col = mood_color(vec3(0.30, 0.305, 0.30), vec3(0.48, 0.46, 0.36),
                                 vec3(0.47, 0.43, 0.37), mood);
    col = mix(col, fleck_col, fleck * 0.38);

    // Exposed steep faces hold less snow and more wet soil/soot. Lighting still supplies the main
    // slope cue; this restrained material shift prevents vertical banks from looking painted white.
    let slope = 1.0 - clamp(normal_y, 0.0, 1.0);
    let slope_col = mood_color(vec3(0.31, 0.30, 0.28), vec3(0.15, 0.14, 0.11),
                                 vec3(0.13, 0.115, 0.10), mood);
    return mix(col, slope_col, slope * slope * 0.58);
}

fn terrain_micro_normal(world: vec3<f32>) -> vec3<f32> {
    let footprint = max(fwidth(world.x), fwidth(world.z));
    let vis = 1.0 - smoothstep(0.16, 0.48, footprint);
    let c = vec2<i32>(floor(world.xz * 4.0));
    let h = terrain_hash(c);
    let dx = terrain_hash(c + vec2<i32>(1, 0)) - h;
    let dz = terrain_hash(c + vec2<i32>(0, 1)) - h;
    return vec3(dx, 0.0, dz) * (0.16 * vis);
}

fn atmosphere_air(deep: f32) -> vec3<f32> {
    let mood = clamp(cam.atmosphere.z, 0.0, 1.0);
    let overcast = clamp(cam.atmosphere.x, 0.0, 1.0);
    let twilight = clamp(cam.atmosphere.w, -1.0, 1.0);
    let near_air = mood_color(vec3(0.39, 0.41, 0.42), vec3(0.32, 0.34, 0.30),
                              vec3(0.31, 0.28, 0.26), mood);
    let far_air = mood_color(vec3(0.32, 0.34, 0.35), vec3(0.27, 0.29, 0.26),
                             vec3(0.27, 0.24, 0.22), mood);
    var day_air = mix(near_air, far_air, clamp(deep, 0.0, 1.0));
    let bruise = mix(vec3(0.25, 0.16, 0.13), vec3(0.30, 0.11, 0.08), twilight * 0.5 + 0.5);
    day_air = mix(day_air, bruise, abs(twilight) * 0.16 * (1.0 - overcast * 0.35));
    let night_air = mood_color(vec3(0.050, 0.055, 0.058), vec3(0.046, 0.053, 0.047),
                               vec3(0.056, 0.046, 0.042), mood);
    return mix(day_air, night_air, clamp(cam.night, 0.0, 1.0));
}

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
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        albedo = terrain_material(raw, in.world, normalize(in.normal).y);
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
        n = normalize(n + terrain_micro_normal(in.world));
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
        // Act II is wettest; snow crust and Halbe ash stay mostly matte.
        roughness = mix(0.94, mix(0.74, 0.88, smoothstep(0.5, 1.0, cam.atmosphere.z)),
                        smoothstep(0.0, 0.5, cam.atmosphere.z));
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
    let day_amb = mix(vec3(0.11, 0.112, 0.11), vec3(0.25, 0.25, 0.24), sky)
        * mix(1.0, 0.94, clamp(cam.atmosphere.x, 0.0, 1.0));
    let night_amb = mix(vec3(0.080, 0.082, 0.080), vec3(0.17, 0.17, 0.16), sky);
    var cold_ambient = mix(day_amb, night_amb, clamp(cam.night, 0.0, 1.0));
    // TRUE INTERIOR COLORS (playtest): the cold-blue SKY ambient is fake light inside a sealed
    // hull. Buttoned up, near geometry drops it to a small NEUTRAL remnant — the dome bulb, the
    // aperture point lights, and the warm fill below are the only real interior light.
    let interior_kill = clamp(cam.interior_light, 0.0, 1.0) * exp(-dist * 0.11);
    let neutral_amb = vec3(dot(cold_ambient, vec3(0.3333)));
    cold_ambient = mix(cold_ambient, neutral_amb * 0.35, interior_kill);
    // A sealed spring overcast remains a broad diffuse source even when the low sun loses the
    // ground plane. Preserve the winter and Halbe grades exactly at the mood endpoints, while an
    // Oderbruch-only bell lift keeps its dark soil above the retro crush toe at low-key hours.
    let low_key = 1.0 - smoothstep(0.08, 0.40, cam.sun_dir.y);
    let campaign_mood = clamp(cam.atmosphere.z, 0.0, 1.0);
    let oder_weight = 4.0 * campaign_mood * (1.0 - campaign_mood);
    let oder_fill = oder_weight
        * (0.40 + low_key * 0.36 + clamp(cam.night, 0.0, 1.0) * 0.24);
    let spring_fill = 1.0 + campaign_mood
        * (0.35 + low_key * 0.90 + clamp(cam.night, 0.0, 1.0) * 1.15)
        * (1.0 - interior_kill)
        + oder_fill * (1.0 - interior_kill);
    let ambient = cold_ambient * spring_fill * mix(albedo, F0, metallic);

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

    // Heavy cloud strips some of the cheerful source-palette chroma from nearby terrain and props.
    // Emissive effects returned above stay saturated, so fire/tracers still punch through the murk.
    let weather_desat = clamp(cam.atmosphere.x * 0.35 + cam.atmosphere.y * 0.35, 0.0, 0.72);
    let weather_luma = dot(col, vec3(0.299, 0.587, 0.114));
    col = mix(col, vec3(weather_luma), weather_desat);

    // Aerial perspective: the cold winter BLUE is in-scattered by the air and accumulates with
    // distance. Near surfaces (low fog) keep their neutral steel; the far field and horizon wash to
    // a luminous cool blue. A second, slower ramp deepens the blue SATURATION of the deep distance,
    // so range reads as blue air receding rather than a flat grey veil — while brightness holds.
    let fog = 1.0 - exp(-cam.fog_density * dist);
    let deep_air = 1.0 - exp(-cam.fog_density * dist * 0.5); // saturates further out
    let air = atmosphere_air(deep_air);
    col = mix(col, air, clamp(fog, 0.0, 1.0));
    return vec4(col, 1.0);
}
