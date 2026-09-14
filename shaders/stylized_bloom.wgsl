// Four bounded raster bloom passes, before the fixed camera tone map. RGB retains the compact
// emissive/lens halo; the spare alpha channel carries a broader outdoor-aperture halo. Openings
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
const INTENSITY: f32 = 0.14;
const SPREAD: f32 = 0.8; // a restrained halo preserves the boundary of small emissive cells
const PORTAL_SPREAD: f32 = 3.2; // about 26 internal pixels of support, independent of output size
const PORTAL_INTENSITY: f32 = 0.34;

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
    let c = textureSample(src, samp, in.uv).rgb;
    let exposure = max(camera.exposure, 1e-6);
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
    var aperture_glow=0.0;
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
            aperture_glow+=min(luminance,1.4)*smoothstep(0.025,0.20,luminance*exposure)*visible;
        }
    }}
    optical_glow*=0.25*0.65;
    return vec4(max(c*contrib,optical_glow), aperture_glow*0.25);
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
    // Fixed 9-tap broad alpha blur reuses the same targets and draw calls. Lamp RGB keeps its
    // former kernel, while exterior light wraps gently around nearby aperture edges.
    var aperture = 0.0;
    if (portal.depth_and_view.z > 0.5) {
        let p = pc.texel * pc.dir * PORTAL_SPREAD;
        aperture = textureSample(src,samp,in.uv).a*w0;
        aperture += (textureSample(src,samp,in.uv+p).a+textureSample(src,samp,in.uv-p).a)*w1;
        aperture += (textureSample(src,samp,in.uv+p*2.0).a+textureSample(src,samp,in.uv-p*2.0).a)*w2;
        aperture += (textureSample(src,samp,in.uv+p*3.0).a+textureSample(src,samp,in.uv-p*3.0).a)*w3;
        aperture += (textureSample(src,samp,in.uv+p*4.0).a+textureSample(src,samp,in.uv-p*4.0).a)*w4;
    }
    return vec4(col, aperture);
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    // Additive-blended into the HDR target on the pass side; emit the halo pre-scaled.
    let c = textureSample(src, samp, in.uv);
    var surround = 0.0;
    if (portal.depth_and_view.z > 0.5) {
        let size = vec2<i32>(textureDimensions(scene_depth));
        let pixel = clamp(vec2<i32>(in.pos.xy),vec2(0),size-vec2(1));
        let aperture = aperture_source(textureLoad(scene_depth,pixel,0),
            textureLoad(glow_mask,pixel,0).a);
        // Keep the scene through the opening/eyepiece at its original contrast. The broad
        // component blooms onto its frame and nearby cabin, never back over the sight picture.
        // Use the full-resolution mask so even a one-pixel optic edge remains well defined.
        surround = 1.0-aperture;
    }
    // This is lens scatter over the visible image, not an unshadowed surface light or a volume.
    // A near-neutral cool halo suits daylight without tinting the existing warm cabin lamps.
    let daylight = vec3(0.91,0.96,1.0)*c.a*PORTAL_INTENSITY*surround;
    return vec4(c.rgb * INTENSITY + daylight, 1.0);
}
