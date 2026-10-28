// THE EYE: the crewman's observer model, a reproduction transform (not physics). It meters the
// transported HDR before bloom and presentation-only particles, advances one view's eye (pupil,
// cone and rod pigments, neural adaptation, flash afterimage) and writes the display mapping that
// crush.wgsl applies. It reads only the HDR and writes only its own state; the renderer copies the
// mapping lanes into the crush uniform. A scene-source flag admits flash detection without
// supplying any radiance: looking at a static bright aperture must never create a flash event.
// src/observer.rs is the reference implementation and the
// source of every constant below (generated, identical in both presets).
// @observer-constants

@group(0) @binding(0) var hdr: texture_2d<f32>;
// [0] cone exposure per unit radiance, [1] last clock time (< 0: not yet observed),
// [2] rod exposure per scotopic cd/m^2, [3] rod inverse saturation, [4] fast pupil mm,
// [5] slow pupil mm, [6] cone bleach, [7] rod bleach, [8] cone neural log10 cd/m^2,
// [9] rod neural log10 scot cd/m^2, [10] flash energy td s, [11] seconds since the flash,
// [12] cone adaptation cd/m^2, [13] rod adaptation scot cd/m^2, [14] mesopic m,
// [15] metered photopic cd/m^2, [16] metered scotopic, [17] pupil mm, [18] afterimage cd/m^2,
// [19] bright coverage, [20] bright luminance cd/m^2, [21] arithmetic field onset reference.
// Lanes 22..31 remain reserved; renderer already allocates 32 lanes.
@group(0) @binding(1) var<storage, read_write> state: array<f32>;

struct ExposureClock {
    time_s: f32,
    // Vertical field of view (radians) and aspect of the metered view, and 1 when one eye looks
    // through a sight (monocular, 491 deg^2 field) rather than both eyes over the natural field.
    fov_y: f32,
    aspect: f32,
    monocular: f32,
    transient_source: f32,
    // The veiling glare of small point sources the frame draws after this meter (the small-arms
    // tracers), already weighted by the meter's three windows: photopic and scotopic cd/m^2
    // (renderer_raymarch.rs `point_veil`). A point source acts on adaptation through the eye's
    // scatter round its image, not as a share of the field average.
    veil_photopic: f32,
    veil_scotopic: f32,
    _pad2: f32,
};
@group(0) @binding(2) var<uniform> clock: ExposureClock;

// Per-thread partial sums: linear photopic sums (field, gaussian, fovea, bright luminance),
// linear scotopic sums (field, gaussian, fovea, bright count), weights (field, gaussian,
// fovea, arithmetic photopic sum). Spatial integration happens before temporal log adaptation:
// black cabin pixels must not erase the contribution of a small bright opening to the field.
var<workgroup> partial_p: array<vec4<f32>, 256>;
var<workgroup> partial_s: array<vec4<f32>, 256>;
var<workgroup> partial_w: array<vec4<f32>, 256>;

fn clamp_luminance(value: f32) -> f32 {
    // A corrupt sample must never poison this view's persistent eye.
    if (!(value >= 0.0 && value <= 1e30)) { return OBS_METER_MIN; }
    return clamp(value, OBS_METER_MIN, OBS_METER_MAX);
}

fn pupil_diameter(luminance: f32, monocular: bool) -> f32 {
    let area = select(OBS_BINOCULAR_FIELD, OBS_SIGHT_FIELD, monocular);
    let eyes = select(1.0, 0.1, monocular);
    let x = pow(clamp_luminance(luminance) * area * eyes / 846.0, 0.41);
    let stanley_davies = 7.75 - 5.75 * (x / (x + 2.0));
    return clamp(stanley_davies + (OBS_AGE - 28.58) * (0.021323 - 0.0095623 * stanley_davies),
        1.5, 9.0);
}

fn pupil_area(diameter: f32) -> f32 {
    return 0.78539816 * diameter * diameter;
}

fn cone_tvi_log(l: f32) -> f32 {
    if (l <= -2.6) { return -0.72; }
    if (l < 1.9) { return pow(0.249 * l + 0.65, 2.7) - 0.72; }
    return l - 1.255;
}

fn rod_tvi_log(l: f32) -> f32 {
    if (l <= -3.94) { return -2.86; }
    if (l < -1.44) { return pow(0.405 * l + 1.6, 2.18) - 2.86; }
    return l - 0.395;
}

fn cone_equivalent_background_log(t: f32) -> f32 {
    let knee = pow(0.249 * 1.9 + 0.65, 2.7) - 0.72;
    if (t <= -0.72) { return -2.6; }
    if (t < knee) { return (pow(t + 0.72, 1.0 / 2.7) - 0.65) / 0.249; }
    return t + 1.255;
}

fn rod_equivalent_background_log(t: f32) -> f32 {
    let knee = pow(0.405 * -1.44 + 1.6, 2.18) - 2.86;
    if (t <= -2.86) { return -3.94; }
    if (t < knee) { return (pow(t + 2.86, 1.0 / 2.18) - 1.6) / 0.405; }
    return t + 0.395;
}

fn log10(x: f32) -> f32 {
    return log2(x) * 0.30102999566;
}

fn mesopic_coefficient(photopic: f32, scotopic: f32) -> f32 {
    let v = 683.0 / 1699.0;
    let lp = clamp_luminance(photopic);
    let ls = clamp_luminance(scotopic);
    var m = 0.5;
    for (var i = 0; i < 12; i += 1) {
        let mesopic = (m * lp + (1.0 - m) * ls * v) / (m + (1.0 - m) * v);
        m = clamp(0.767 + 0.3334 * log10(max(mesopic, OBS_METER_MIN)), 0.0, 1.0);
    }
    return m;
}

fn steady_bleach(retinal: f32, regeneration: f32, photosensitivity: f32) -> f32 {
    let k = max(retinal, 0.0) / photosensitivity;
    if (k <= 0.0) { return 0.0; }
    let b = regeneration - k + k * OBS_PIGMENT_KM;
    return clamp((-b + sqrt(b * b + 4.0 * k * k * OBS_PIGMENT_KM)) / (2.0 * k), 0.0, 1.0);
}

fn flash_afterimage(energy: f32, since: f32) -> f32 {
    if (energy <= 0.0) { return 0.0; }
    let t = max(since, OBS_FLASH_MIN_RECOVERY);
    return min(0.19 * pow(20.0 / t, 2.5) * pow(energy / 9.0e5, 1.325), OBS_METER_MAX);
}

fn current_pupil() -> f32 {
    return state[4] * (1.0 - OBS_PUPIL_SLOW_SHARE) + state[5] * OBS_PUPIL_SLOW_SHARE;
}

fn cone_adaptation() -> f32 {
    let bleached = cone_equivalent_background_log(-0.72 + OBS_CONE_BLEACH_LOG * state[6]);
    return pow(10.0, max(state[8], bleached)) + flash_afterimage(state[10], state[11]);
}

fn rod_adaptation() -> f32 {
    let bleached = rod_equivalent_background_log(-2.86 + OBS_ROD_BLEACH_LOG * state[7]);
    return pow(10.0, max(state[9], bleached)) + flash_afterimage(state[10], state[11]);
}

fn relax(value: f32, goal: f32, tau: f32, dt: f32) -> f32 {
    return value + (goal - value) * (1.0 - exp(-dt / tau));
}

fn bleach_step(b: f32, retinal: f32, regeneration: f32, photosensitivity: f32, dt: f32) -> f32 {
    let bleached = 1.0 - (1.0 - b) * exp(-(retinal / photosensitivity) * dt);
    return clamp(bleached - regeneration * bleached / (bleached + OBS_PIGMENT_KM) * dt, 0.0, 1.0);
}

fn substep(lp: f32, ls: f32, coverage: f32, bright: f32, field: f32, monocular: bool,
           transient_source: bool, dt: f32) {
    let goal = pupil_diameter(lp, monocular);
    let constricting = goal < state[4];
    state[4] = relax(state[4], goal, select(OBS_PUPIL_DILATE_FAST_TAU, OBS_PUPIL_CONSTRICT_TAU,
        constricting), dt);
    state[5] = relax(state[5], goal, select(OBS_PUPIL_DILATE_SLOW_TAU, OBS_PUPIL_CONSTRICT_TAU,
        goal < state[5]), dt);
    let area = pupil_area(current_pupil());
    state[6] = bleach_step(state[6], lp * area, OBS_CONE_REGEN, OBS_CONE_Q, dt);
    state[7] = bleach_step(state[7], ls * area, OBS_ROD_REGEN, OBS_ROD_Q, dt);
    let retinal_ratio = log10(area / pupil_area(goal));
    let cone_target = log10(lp) + retinal_ratio;
    let rod_target = log10(ls) + retinal_ratio;
    state[8] = relax(state[8], cone_target,
        select(OBS_CONE_TAU_DOWN, OBS_CONE_TAU_UP, cone_target > state[8]), dt);
    state[9] = relax(state[9], rod_target,
        select(OBS_ROD_TAU_DOWN, OBS_ROD_TAU_UP, rod_target > state[9]), dt);
    // Contrast alone mistakes a steady window for a new flash every frame. Detect onset
    // from threshold-independent field energy so changing adaptation cannot re-arm that window.
    let rising = field > OBS_FLASH_ONSET_RATIO * state[21];
    state[21] = relax(state[21], field, OBS_FLASH_ONSET_TAU, dt);
    let flashing = transient_source && rising && coverage > OBS_FLASH_MIN_COVERAGE
        && bright > OBS_FLASH_ABOVE_ADAPTATION * cone_adaptation();
    state[10] = state[10] * exp(-dt / OBS_FLASH_MEMORY_TAU);
    if (flashing) {
        state[10] += coverage * bright * area * dt;
        state[11] = 0.0;
    } else {
        state[11] = min(state[11] + dt, 1.0e6);
    }
}

fn write_mapping(monocular: bool) {
    let cone = cone_adaptation();
    let rod = rod_adaptation();
    let display_threshold = cone_tvi_log(log10(OBS_DISPLAY_ADAPTATION));
    let cone_scale = pow(10.0, display_threshold - cone_tvi_log(log10(cone)));
    let rod_scale = pow(10.0, display_threshold - rod_tvi_log(log10(rod)));
    let m = mesopic_coefficient(cone, rod);
    let key = OBS_DISPLAY_KEY / OBS_DISPLAY_ADAPTATION;
    let pupil = current_pupil();
    let pupil_gain = pupil_area(pupil) / pupil_area(pupil_diameter(cone, monocular));
    state[0] = key * cone_scale * OBS_LUMINANCE_PER_UNIT_RADIANCE * pupil_gain;
    state[2] = key * (1.0 - m) * rod_scale * pupil_gain;
    state[3] = pupil_area(pupil) / OBS_ROD_SATURATION_TD;
    state[12] = cone;
    state[13] = rod;
    state[14] = m;
    state[17] = pupil;
    state[18] = flash_afterimage(state[10], state[11]);
}

@compute @workgroup_size(256)
fn cs(@builtin(local_invocation_index) li: u32) {
    let dim = vec2<f32>(textureDimensions(hdr));
    let grid = vec2<u32>(u32(OBS_GRID_X), u32(OBS_GRID_Y));
    let total = grid.x * grid.y;
    let tan_y = tan(0.5 * clamp(clock.fov_y, 0.01, 3.0));
    let tan_x = tan_y * max(clock.aspect, 0.01);
    let sigma = radians(OBS_GAUSSIAN_SIGMA_DEG);
    let fovea = radians(OBS_FOVEA_RADIUS_DEG);
    // The previous frame's cone adaptation decides what counts as flash light.
    let flash_level = select(1.0e30, OBS_FLASH_ABOVE_ADAPTATION * state[12], state[1] >= 0.0);
    var sp = vec4(0.0);
    var ss = vec4(0.0);
    var sw = vec4(0.0);
    var i = li;
    loop {
        if (i >= total) { break; }
        let cell_index = vec2<u32>(i % grid.x, i / grid.x);
        let cell = vec2<f32>(cell_index) + vec2(0.5);
        let uv = cell / vec2<f32>(grid);
        // The cell's own pixels, averaged: an area meter, as the field average it models. One
        // point sample a cell caught or missed a source smaller than a cell (the Moon, a tracer) by
        // where it fell, and the eye jumped thirtyfold between grids (2026-10-28). Every pixel is
        // read up to 4 x 4 a cell (frames to 1024 x 576), evenly spaced beyond.
        let lo = vec2<i32>(floor(vec2<f32>(cell_index) / vec2<f32>(grid) * dim));
        let hi = max(vec2<i32>(floor(vec2<f32>(cell_index + vec2(1u)) / vec2<f32>(grid) * dim)), lo + vec2(1));
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
        let lp = clamp_luminance(dot(c, vec3(OBS_PHOTOPIC_R, OBS_PHOTOPIC_G, OBS_PHOTOPIC_B)));
        let ls = clamp_luminance(dot(c, vec3(OBS_SCOTOPIC_R, OBS_SCOTOPIC_G, OBS_SCOTOPIC_B)));
        let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
        let ray = vec3(ndc.x * tan_x, ndc.y * tan_y, 1.0);
        let angle = acos(clamp(1.0 / length(ray), -1.0, 1.0));
        let wg = exp(-0.5 * (angle * angle) / (sigma * sigma));
        let wf = select(0.0, 1.0, angle <= fovea);
        sp += vec4(lp, wg * lp, wf * lp, 0.0);
        ss += vec4(ls, wg * ls, wf * ls, 0.0);
        sw += vec4(1.0, wg, wf, lp);
        if (lp > flash_level) {
            sp.w += lp;
            ss.w += 1.0;
        }
        i += 256u;
    }
    partial_p[li] = sp;
    partial_s[li] = ss;
    partial_w[li] = sw;
    workgroupBarrier();
    var stride = 128u;
    loop {
        if (stride == 0u) { break; }
        if (li < stride) {
            partial_p[li] += partial_p[li + stride];
            partial_s[li] += partial_s[li + stride];
            partial_w[li] += partial_w[li + stride];
        }
        workgroupBarrier();
        stride = stride >> 1u;
    }
    if (li != 0u) { return; }

    let p = partial_p[0];
    let s = partial_s[0];
    let w = partial_w[0];
    let field_p = p.x / max(w.x, 1.0);
    let field_s = s.x / max(w.x, 1.0);
    let gauss_p = select(field_p, p.y / w.y, w.y > 1e-6);
    let gauss_s = select(field_s, s.y / w.y, w.y > 1e-6);
    let fovea_p = select(gauss_p, p.z / w.z, w.z > 0.5);
    let fovea_s = select(gauss_s, s.z / w.z, w.z > 0.5);
    // One shared view adaptation driven by the incident light in the existing spatial windows.
    // No aperture classification, independent port exposure, or scene-dependent luminance floor.
    let lp = clamp_luminance(OBS_FOVEA_WEIGHT * fovea_p + OBS_GAUSSIAN_WEIGHT * gauss_p
        + OBS_FIELD_WEIGHT * field_p + max(clock.veil_photopic, 0.0));
    let ls = clamp_luminance(OBS_FOVEA_WEIGHT * fovea_s + OBS_GAUSSIAN_WEIGHT * gauss_s
        + OBS_FIELD_WEIGHT * field_s + max(clock.veil_scotopic, 0.0));
    let field = clamp_luminance(w.w / max(w.x, 1.0));
    let coverage = s.w / max(w.x, 1.0);
    let bright = select(0.0, p.w / max(s.w, 1.0), s.w > 0.0);
    let monocular = clock.monocular > 0.5;
    state[15] = lp;
    state[16] = ls;
    state[19] = coverage;
    state[20] = bright;

    // A view's first observed frame: the crewman is already adapted to what he sees, as a real
    // observer is before the first presented frame. Only this sentinel snaps.
    if (state[1] < 0.0) {
        let pupil = pupil_diameter(lp, monocular);
        let area = pupil_area(pupil);
        state[4] = pupil;
        state[5] = pupil;
        state[6] = steady_bleach(lp * area, OBS_CONE_REGEN, OBS_CONE_Q);
        state[7] = steady_bleach(ls * area, OBS_ROD_REGEN, OBS_ROD_Q);
        state[8] = log10(lp);
        state[9] = log10(ls);
        state[10] = 0.0;
        state[11] = 1.0e6;
        state[21] = field;
        state[1] = clock.time_s;
        write_mapping(monocular);
        return;
    }

    // Simulated seconds since the last observation, integrated in steps of at most 0.25 s.
    let dt = clamp(clock.time_s - state[1], 0.0, 1.0);
    state[1] = clock.time_s;
    if (dt > 0.0) {
        let steps = u32(ceil(dt / OBS_MAX_STEP));
        let h = dt / f32(max(steps, 1u));
        for (var k = 0u; k < steps; k += 1u) {
            substep(lp, ls, coverage, bright, field, monocular, clock.transient_source > 0.5, h);
        }
    }
    write_mapping(monocular);
}
