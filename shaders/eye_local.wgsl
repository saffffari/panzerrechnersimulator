// THE EYE'S LOCAL ADAPTATION: the adaptation luminance each region of the retina pools from its
// own neighbourhood (src/observer.rs LOCAL_*; docs/PHYSICAL_TRUTH_AUDIT.md, "The eye adapts place
// by place"). A reproduction transform like the meter (exposure.wgsl): it reads the transported
// HDR and writes only this map, which crush.wgsl reads to place each pixel's cone gain between
// the eye's one adaptation state and its own neighbourhood's.
//
// 1. `cells`: the photopic luminance of each meter cell, the cell's own pixels averaged (up to
//    4 x 4 a cell, as the meter reads them), as log10 cd/m^2.
// 2. `blur_x`, `blur_y`: a Gaussian over that log map, sigma in cells from the view's degrees per
//    cell: the log-domain pooling of Vangorp et al. 2015's local adaptation model (observer.rs
//    LOCAL_POOLING_SIGMA_DEG). The result is written to a filterable texture the crush samples.
// @observer-constants

struct LocalParams {
    // x, y: the grid (cells); z, w: the pooling sigma in cells, horizontally and vertically.
    grid: vec4<f32>,
};

@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: LocalParams;
@group(0) @binding(2) var<storage, read_write> cells: array<f32>;
@group(0) @binding(3) var<storage, read_write> rows: array<f32>;
@group(0) @binding(4) var pooled: texture_storage_2d<rgba16float, write>;

// The largest pooling radius in cells (three sigma); beyond it the kernel is truncated.
const MAX_RADIUS: i32 = 24;

fn grid() -> vec2<u32> {
    return vec2<u32>(u32(params.grid.x), u32(params.grid.y));
}

@compute @workgroup_size(8, 8)
fn cells_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let g = grid();
    if (id.x >= g.x || id.y >= g.y) { return; }
    let dim = vec2<f32>(textureDimensions(hdr));
    let lo = vec2<i32>(floor(vec2<f32>(id.xy) / vec2<f32>(g) * dim));
    let hi = max(vec2<i32>(floor(vec2<f32>(id.xy + vec2(1u)) / vec2<f32>(g) * dim)), lo + vec2(1));
    let size = hi - lo;
    let n = min(size, vec2(4));
    let last = vec2<i32>(dim) - vec2(1);
    var c = vec3(0.0);
    for (var y = 0; y < n.y; y += 1) {
        for (var x = 0; x < n.x; x += 1) {
            c += textureLoad(hdr, min(lo + (vec2(x, y) * size) / n, last), 0).rgb;
        }
    }
    c /= f32(n.x * n.y);
    var l = dot(c, vec3(OBS_PHOTOPIC_R, OBS_PHOTOPIC_G, OBS_PHOTOPIC_B));
    // A corrupt sample must never poison the map (as the meter's clamp_luminance).
    if (!(l >= 0.0 && l <= 1e30)) { l = OBS_METER_MIN; }
    l = clamp(l, OBS_METER_MIN, OBS_METER_MAX);
    cells[id.y * g.x + id.x] = log2(l) * 0.30102999566;
}

fn weight(d: f32, sigma: f32) -> f32 {
    return exp(-0.5 * d * d / max(sigma * sigma, 1e-6));
}

@compute @workgroup_size(8, 8)
fn blur_x(@builtin(global_invocation_id) id: vec3<u32>) {
    let g = grid();
    if (id.x >= g.x || id.y >= g.y) { return; }
    let sigma = params.grid.z;
    let radius = min(i32(ceil(3.0 * sigma)), MAX_RADIUS);
    var sum = 0.0;
    var wsum = 0.0;
    for (var d = -radius; d <= radius; d += 1) {
        let x = clamp(i32(id.x) + d, 0, i32(g.x) - 1);
        let w = weight(f32(d), sigma);
        sum += w * cells[id.y * g.x + u32(x)];
        wsum += w;
    }
    rows[id.y * g.x + id.x] = sum / max(wsum, 1e-6);
}

@compute @workgroup_size(8, 8)
fn blur_y(@builtin(global_invocation_id) id: vec3<u32>) {
    let g = grid();
    if (id.x >= g.x || id.y >= g.y) { return; }
    let sigma = params.grid.w;
    let radius = min(i32(ceil(3.0 * sigma)), MAX_RADIUS);
    var sum = 0.0;
    var wsum = 0.0;
    for (var d = -radius; d <= radius; d += 1) {
        let y = clamp(i32(id.y) + d, 0, i32(g.y) - 1);
        let w = weight(f32(d), sigma);
        sum += w * rows[u32(y) * g.x + id.x];
        wsum += w;
    }
    textureStore(pooled, vec2<i32>(id.xy), vec4(sum / max(wsum, 1e-6), 0.0, 0.0, 1.0));
}
