// Four bounded raster bloom passes, before the fixed camera tone map. RGB retains the compact
// emissive/lens halo, including the colored outdoor-aperture halo. Openings
// are identified from visible scene depth and depth-tested optic masks, never inferred from a
// bright cabin wall or a hatch-open flag. No ray tracing, exposure adaptation or temporal history.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(3) var glow_mask: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler; // bilinear-clamp, so the half-res upsample stays smooth
@group(0) @binding(4) var scene_depth: texture_depth_2d;
struct BloomPortal { depth_and_view: vec4<f32>, }; // near, far, inside physical cabin, reserved
@group(0) @binding(5) var<uniform> portal: BloomPortal;

// Share the exact GPU-adapted camera exposure used by crush. The scalar sizes preserve
// CrushUniform's 44-byte exposure offset and 56-byte host span without another camera state.
struct BloomCamera {
    @size(44) _before_exposure: f32,
    exposure: f32,
    @size(8) _after_exposure: f32,
};
@group(0) @binding(2) var<uniform> camera: BloomCamera;

// texel = 1/target_size of `src`; dir picks the separable blur axis (ignored by bright/composite).
struct BloomPush {
    texel: vec2<f32>,
    dir: vec2<f32>,
};
var<push_constant> pc: BloomPush;

// The threshold is in exposed camera energy. A flame visible after dark adaptation must
// receive the same optical halo as an equally bright lamp, regardless of raw world radiance.
const THRESHOLD: f32 = 1.30;
const KNEE: f32 = 0.7;
const INTENSITY: f32 = 0.18;
const SPREAD: f32 = 1.0; // a restrained halo preserves the boundary of small emissive cells
const PORTAL_INTENSITY: f32 = 0.14; // modest lens response; never adds surface illumination
// The sky discs' most exposed brightness into the halo and the veil (fs_bright).
const SKY_DISC_LIMIT: f32 = 6.0;

fn aperture_source(depth: f32, mask: f32) -> f32 {
    // Depth is the nearest actual triangle (including the hatch lid and its handle). The clear
    // sky depth also maps to far. A conservative 4–7 m transition excludes nearby cabin surfaces;
    // optical relays bypass distance because their visible glass is at the observer's eye.
    let near = portal.depth_and_view.x;
    let far = portal.depth_and_view.y;
    let metres = near * far / max(far - depth * (far - near), 1e-6);
    let opening = smoothstep(4.0, 7.0, metres);
    let relay = step(1.5, mask);
    return max(opening, relay) * portal.depth_and_view.z;
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_bright(in: VsOut) -> @location(0) vec4<f32> {
    var c = textureSample(src, samp, in.uv).rgb;
    let exposure = max(camera.exposure, 1e-6);
    // The Sun's and the Moon's discs (sky pixels, at the scene's far depth) feed the halo and the
    // eye's veil at most at SKY_DISC_LIMIT times display white: at ten thousand times white the
    // Sun's halo and veil buried its disc in a round glow. Owner-directed presentation (2026-10-28,
    // "a visibly stepped border for the sun"; docs/PHYSICAL_TRUTH_AUDIT.md): the disc keeps its
    // full radiance in the frame and in the eye's meter.
    let depth_size = vec2<i32>(textureDimensions(scene_depth));
    let corner = vec2<i32>(floor(in.uv * vec2<f32>(depth_size) - vec2(0.5)));
    var sky = true;
    for (var y = 0; y < 2; y += 1) { for (var x = 0; x < 2; x += 1) {
        let p = clamp(corner + vec2(x, y), vec2(0), depth_size - vec2(1));
        sky = sky && textureLoad(scene_depth, p, 0) >= 1.0;
    } }
    let raw = max(c.r, max(c.g, c.b)) * exposure;
    if (sky && raw > SKY_DISC_LIMIT) { c *= SKY_DISC_LIMIT / raw; }
    let exposed = c * exposure;
    let br = max(exposed.r, max(exposed.g, exposed.b));
    // Soft-knee threshold (Karis / UE bloom): smooth quadratic ramp across [T-KNEE, T+KNEE].
    let soft = clamp(br - THRESHOLD + KNEE, 0.0, 2.0 * KNEE);
    let soft2 = soft * soft / (4.0 * KNEE + 1e-5);
    let contrib = max(soft2, br - THRESHOLD) / max(br, 1e-5);
    // Authored lamp lenses and optical panes receive a soft halo even below the bright-pass
    // threshold. Keep their unblurred HDR image unchanged; dark views stay dark at night.
    let size=vec2<i32>(textureDimensions(glow_mask));
    let first=vec2<i32>(floor(in.uv*vec2<f32>(size)-vec2(0.5)));
    var optical_glow=vec3(0.0);
    var aperture_glow=vec3(0.0);
    // Downsample each source radiance together with its own mask. Point-sampling a mask
    // beside bilinear HDR drops one-pixel lenses on alternating half-resolution blocks.
    for(var y=0;y<2;y+=1) {for(var x=0;x<2;x+=1) {
        let pixel=clamp(first+vec2(x,y),vec2(0),size-vec2(1));
        let mask=textureLoad(glow_mask,pixel,0).a;
        let radiance=max(textureLoad(src,pixel,0).rgb,vec3(0.0));
        optical_glow+=radiance*clamp(mask,0.0,1.0);
        if (portal.depth_and_view.z > 0.5) {
            let luminance=dot(radiance,vec3(0.2126,0.7152,0.0722));
            let visible=aperture_source(textureLoad(scene_depth,pixel,0),mask);
            // Sampled radiance, not a constant white source: dim night/storm windows remain dim.
            // A cap prevents a muzzle flash in the sight from becoming a broad screen-white flare.
            let limited_radiance = radiance * min(1.0,1.4/max(luminance,1e-6));
            aperture_glow+=limited_radiance*smoothstep(0.025,0.20,luminance*exposure)*visible;
        }
    }}
    optical_glow*=0.25*0.65;
    // Carry all three channels through both separable passes. A scalar luminance in alpha
    // cannot reconstruct the color of a fire seen through a viewport.
    return vec4(max(c*contrib,optical_glow) + aperture_glow*0.25*(PORTAL_INTENSITY/INTENSITY),1.0);
}

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    // 9-tap Gaussian (weights sum ~1), spacing scaled by SPREAD for a wider soft halo.
    let w0 = 0.227027;
    let w1 = 0.1945946;
    let w2 = 0.1216216;
    let w3 = 0.054054;
    let w4 = 0.016216;
    let o = pc.texel * pc.dir * SPREAD;
    var col = textureSample(src, samp, in.uv).rgb * w0;
    col += textureSample(src, samp, in.uv + o * 1.0).rgb * w1;
    col += textureSample(src, samp, in.uv - o * 1.0).rgb * w1;
    col += textureSample(src, samp, in.uv + o * 2.0).rgb * w2;
    col += textureSample(src, samp, in.uv - o * 2.0).rgb * w2;
    col += textureSample(src, samp, in.uv + o * 3.0).rgb * w3;
    col += textureSample(src, samp, in.uv - o * 3.0).rgb * w3;
    col += textureSample(src, samp, in.uv + o * 4.0).rgb * w4;
    col += textureSample(src, samp, in.uv - o * 4.0).rgb * w4;
    // The same compact camera kernel applies inside and outside. Cabin visibility must come
    // from light transport, not an enlarged lens halo laid over its walls.
    return vec4(col,1.0);
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    // Additive-blended into the HDR target on the pass side; emit the halo pre-scaled.
    let c = textureSample(src, samp, in.uv);
    var surround = 1.0;
    if (portal.depth_and_view.z > 0.5) {
        let size = vec2<i32>(textureDimensions(scene_depth));
        let pixel = clamp(vec2<i32>(in.pos.xy),vec2(0),size-vec2(1));
        let aperture = aperture_source(textureLoad(scene_depth,pixel,0),
            textureLoad(glow_mask,pixel,0).a);
        // Keep the scene through the opening/eyepiece at its original contrast. The compact
        // component blooms onto its frame and nearby cabin, never back over the sight picture.
        // Use the full-resolution mask so even a one-pixel optic edge remains well defined.
        surround = 1.0-aperture;
    }
    // This is lens scatter over the visible image, not an unshadowed surface light or a volume.
    // No fixed tint: orange fire produces an orange surround, cool sky a cool surround.
    // pc.dir.x > 0 overrides the halo's strength (the raymarch backend, whose eye scatter is the
    // veil pass: RM_BLOOM_INTENSITY in renderer_raymarch.rs).
    let intensity = select(INTENSITY, pc.dir.x, pc.dir.x > 0.0);
    return vec4(c.rgb * intensity * surround, 1.0);
}

// --- Veiling glare of the crewman's eye (raymarch backend chain) -----------------------------
// Intraocular scatter spreads a bright source over the retina: the CIE general disability glare
// equation (Vos & van den Berg 1999) gives the veil as L_v = E_glare * 5 / theta^2 (cd/m^2 per
// lux) between about 1 and 30 degrees. Each octave ring of angle therefore carries the same
// veil energy, 5 * 2 pi theta dtheta / theta^2 ~ 2.2 % of the source luminance per octave. Four
// rings (1.5, 3, 6, 12 deg) of eight taps each sample the Gaussian-blurred bright pass; the
// inner degree is the compact bloom. GLARE_SCALE halves the CIE veil for play (named in
// docs/PHYSICAL_TRUTH_AUDIT.md); the sun, a muzzle flash and a night fire still veil the eye.
const GLARE_RING_SHARE: f32 = 0.0218;
const GLARE_SCALE: f32 = 0.5;
@group(1) @binding(0) var glare_l1: texture_2d<f32>;
@group(1) @binding(1) var glare_l2: texture_2d<f32>;
@group(1) @binding(2) var glare_l3: texture_2d<f32>;
@group(1) @binding(3) var glare_l4: texture_2d<f32>;
@group(1) @binding(4) var glare_samp: sampler;
// One ring of taps from its pyramid level, enough of them that neighbouring taps lie within 1.5 of
// that level's texels (12 to 64), so a point source spreads into a smooth veil instead of copies of
// itself: in a narrow field (the TZF's 25 deg, the binoculars' 10) twelve taps of the outer rings
// lay several texels apart and drew a ring of dots round the Moon (2026-10-28). `radius_px` is the
// ring's radius in the bloom target's pixels, `texel_px` the level's texel in those pixels.
fn glare_ring(level: u32, uv: vec2<f32>, radius: vec2<f32>, phase: f32, radius_px: f32, texel_px: f32) -> vec3<f32> {
    var sum = vec3(0.0);
    let taps = clamp(u32(ceil(6.2831853 * radius_px / (1.5 * texel_px))), 12u, 64u);
    let step_angle = 6.2831853 / f32(taps);
    for (var k = 0u; k < taps; k += 1u) {
        let a = f32(k) * step_angle + phase;
        let p = uv + vec2(cos(a), sin(a)) * radius;
        var c = vec3(0.0);
        switch level {
            case 1u: { c = textureSample(glare_l1, glare_samp, p).rgb; }
            case 2u: { c = textureSample(glare_l2, glare_samp, p).rgb; }
            case 3u: { c = textureSample(glare_l3, glare_samp, p).rgb; }
            default: { c = textureSample(glare_l4, glare_samp, p).rgb; }
        }
        sum += c;
    }
    return sum / f32(taps);
}
@fragment
fn fs_glare(in: VsOut) -> @location(0) vec4<f32> {
    // pc.dir.x carries the bloom target's pixels per degree of view.
    let px_per_deg = max(pc.dir.x, 0.5);
    var veil = vec3(0.0);
    for (var ring = 0u; ring < 4u; ring += 1u) {
        let theta_deg = 1.5 * exp2(f32(ring));
        let radius = theta_deg * px_per_deg * pc.texel;
        let phase = select(0.0, 0.2617994, (ring & 1u) == 1u);
        veil += glare_ring(ring + 1u, in.uv, radius, phase, theta_deg * px_per_deg, exp2(f32(ring + 2u)));
    }
    return vec4(veil * (GLARE_RING_SHARE * GLARE_SCALE), 1.0);
}
@fragment
fn fs_composite_glare(in: VsOut) -> @location(0) vec4<f32> {
    // The veil is already in luminance units of the source; it adds to the frame as is.
    return vec4(textureSample(src, samp, in.uv).rgb, 1.0);
}
