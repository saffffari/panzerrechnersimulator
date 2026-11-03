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
// [19] bright coverage, [20] bright luminance cd/m^2, [21] arithmetic field onset reference,
// [22] inside reference log10 photopic, [23] inside reference log10 scotopic (observer::inside_meter).
// [24] glare veil over the field cd/m^2, [25] over the Gaussian (diagnostics), [27] last frame's
// log-average photopic cd/m^2, [28] share of cells above GLARE_RATIO times it, [29] the scotopic
// background (observer::GLARE_*), [30] 1 while the first-frame snap waits for that background.
// [31] the clock time until which a fresh snap repeats (observer::SNAP_HOLD_S). Lane 26 remains
// reserved (32 allocated).
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
    // 1: the eye is inside the hull (observer::inside_meter); 2: an optic's own eye, adapted at once
    // to its field when it returns after OBS_OPTIC_RESNAP_GAP.
    mode: f32,
    // The rate of the slow pigment processes over real time: the sky clock's rate with the day
    // cycle on (Renderer::pigment_rate), 1 otherwise. Named in docs/PHYSICAL_TRUTH_AUDIT.md.
    pigment_rate: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
};
@group(0) @binding(2) var<uniform> clock: ExposureClock;

// The same 256 strided accumulations used by the original single workgroup, distributed
// across independent workgroups. Each lane's sample order and the final reduction tree stay
// unchanged. This is scheduling only: no fewer pixels, changed meter or adaptation law.
// v: the photopic glare veil of small bright sources over the field, Gaussian and fovea windows,
// cd/m^2, already per window (not to be averaged); u: x the number of cells above GLARE_RATIO
// times the background, y..w the scotopic veil over the same three windows.
struct MeterSum { p: vec4<f32>, s: vec4<f32>, w: vec4<f32>, v: vec4<f32>, u: vec4<f32> };
@group(0) @binding(3) var<storage, read_write> meter_sums: array<MeterSum, 256>;

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
// Inside the hull from OBS_DAZZLE_FROM_LOG; outside (and at an optic) from the lower
// OBS_OUTSIDE_DAZZLE_FROM_LOG, where no inside damping stands in front of it, and only toward a
// photopic target: the cones' share `m_target` of it (CIE 191) from OBS_OUTSIDE_DAZZLE_MESOPIC_FROM
// to 1 (observer::rise_tau, observer::outside_rise_tau).
fn rise_tau(base_tau: f32, excess_log: f32, inside: bool, m_target: f32) -> f32 {
    let onset = select(OBS_OUTSIDE_DAZZLE_FROM_LOG, OBS_DAZZLE_FROM_LOG, inside);
    let ramp = select(OBS_OUTSIDE_DAZZLE_RAMP_LOG, OBS_DAZZLE_RAMP_LOG, inside);
    let photopic = select(smoothstep(OBS_OUTSIDE_DAZZLE_MESOPIC_FROM, 1.0, m_target), 1.0, inside);
    return mix(base_tau, OBS_DAZZLE_TAU, smoothstep(onset, onset + ramp, excess_log) * photopic);
}

fn bleach_step(b: f32, retinal: f32, regeneration: f32, photosensitivity: f32, dt: f32) -> f32 {
    let bleached = 1.0 - (1.0 - b) * exp(-(retinal / photosensitivity) * dt);
    return clamp(bleached - regeneration * bleached / (bleached + OBS_PIGMENT_KM) * dt, 0.0, 1.0);
}

fn substep(lp: f32, ls: f32, coverage: f32, bright: f32, field: f32, monocular: bool,
           transient_source: bool, inside: bool, dt: f32) {
    let goal = pupil_diameter(lp, monocular);
    let constricting = goal < state[4];
    state[4] = relax(state[4], goal, select(OBS_PUPIL_DILATE_FAST_TAU, OBS_PUPIL_CONSTRICT_TAU,
        constricting), dt);
    state[5] = relax(state[5], goal, select(OBS_PUPIL_DILATE_SLOW_TAU, OBS_PUPIL_CONSTRICT_TAU,
        goal < state[5]), dt);
    let area = pupil_area(current_pupil());
    let pigment_dt = dt * max(clock.pigment_rate, 1.0);
    state[6] = bleach_step(state[6], lp * area, OBS_CONE_REGEN, OBS_CONE_Q, pigment_dt);
    state[7] = bleach_step(state[7], ls * area, OBS_ROD_REGEN, OBS_ROD_Q, pigment_dt);
    let retinal_ratio = log10(area / pupil_area(goal));
    let cone_target = log10(lp) + retinal_ratio;
    let rod_target = log10(ls) + retinal_ratio;
    // Contrast alone mistakes a steady window for a new flash every frame. Detect onset
    // from threshold-independent field energy so changing adaptation cannot re-arm that window.
    let rising = field > OBS_FLASH_ONSET_RATIO * state[21];
    state[21] = relax(state[21], field, OBS_FLASH_ONSET_TAU, dt);
    let flashing = transient_source && rising && coverage > OBS_FLASH_MIN_COVERAGE
        && bright > OBS_FLASH_ABOVE_ADAPTATION * cone_adaptation();
    // Rising: the comfortable rate, falling to the dazzle rate for a step far up: inside the hull
    // a cabin decades brighter (observer::rise_tau), outside a step into daylight
    // (observer::outside_rise_tau). Not during a weapon's flash: what a flash of tens of
    // milliseconds leaves is its afterimage (the recovery fit, lanes 10-11), and the dazzle rate
    // on top of it counted the flash twice (observer::Eye::substep).
    let cone_rise = select(cone_target - state[8], 0.0, flashing);
    let rod_rise = select(rod_target - state[9], 0.0, flashing);
    let m_target = mesopic_coefficient(lp, ls);
    state[8] = relax(state[8], cone_target,
        select(OBS_CONE_TAU_DOWN, rise_tau(OBS_CONE_TAU_UP, cone_rise, inside, m_target), cone_target > state[8]), dt);
    state[9] = relax(state[9], rod_target,
        select(OBS_ROD_TAU_DOWN, rise_tau(OBS_ROD_TAU_UP, rod_rise, inside, m_target), rod_target > state[9]), dt);
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

fn sample_meter(li: u32) -> MeterSum {
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
    var sv = vec4(0.0);
    // The field and Gaussian windows in square degrees, for a source's veil over each.
    let field_deg2 = degrees(2.0 * atan(tan_x)) * degrees(2.0 * atan(tan_y));
    let sigma_deg = OBS_GAUSSIAN_SIGMA_DEG;
    let cell_sr = (2.0 * tan_x / f32(grid.x)) * (2.0 * tan_y / f32(grid.y));
    let outdoors = (u32(clock.mode + 0.5) & 3u) == 0u;
    var su = vec4(0.0);
    // Glare treatment needs the previous frame's background; it stands down while the bright
    // cells cover too much of the field (lane 28, their share last frame).
    let glare_on = outdoors && state[27] > 0.0 && state[28] < OBS_GLARE_MAX_COVERAGE;
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
        // A small source far brighter than the frame (a lit window, a fire, the Sun's disc)
        // adapts the rest of the field only through the eye's scatter round its image, not as a
        // share of the field average: the retina adapts locally (observer.rs GLARE_*). Against the
        // previous frame's log-average (geometric mean) luminance, outdoors,
        // and while such cells cover under GLARE_MAX_COVERAGE of the field (a large bright area
        // adapts the eye as before), a cell above GLARE_RATIO times it enters the fovea, Gaussian
        // and field windows at that background and adds the CIE disability-glare veil to each.
        var field_p = lp;
        var field_s = ls;
        let background = state[27];
        let background_s = state[29];
        let glare_cell = background > 0.0 && lp > OBS_GLARE_RATIO * background;
        if (glare_cell) {
            su.x += 1.0;
        }
        if (glare_on && glare_cell) {
            let solid = cell_sr / pow(length(ray), 3.0);
            let e_p = (lp - background) * solid;
            let e_s = max(ls - background_s, 0.0) * solid;
            let theta = degrees(angle);
            let gauss = OBS_GLARE_INTEGRAL_DEG2 * exp(-0.5 * theta * theta / (sigma_deg * sigma_deg))
                / (6.2831853 * sigma_deg * sigma_deg);
            let field = OBS_GLARE_INTEGRAL_DEG2 / max(field_deg2, 1.0);
            // The fovea's mean veil: 10/theta^2 at the window's centre for a source outside it
            // (named); for a source inside, at theta from the centre, the disc's mean of 10/r^2 cut
            // at CIE 146's lower validity bound theta_min, 20 ln((R^2 - theta^2) / (theta_min R)) / R^2,
            // never under its value at the rim.
            let r_f = OBS_FOVEA_RADIUS_DEG;
            let inside_mean = 20.0 * log(max(r_f * r_f - theta * theta, 1.0e-6) / (OBS_GLARE_MIN_DEG * r_f))
                / (r_f * r_f);
            let fov = select(10.0 / max(theta * theta, r_f * r_f),
                max(inside_mean, 10.0 / (r_f * r_f)), theta <= r_f);
            sv += vec4(e_p * field, e_p * gauss, e_p * fov, 0.0);
            su += vec4(0.0, e_s * field, e_s * gauss, e_s * fov);
            field_p = background;
            field_s = background_s;
        }
        sp += vec4(field_p, wg * field_p, wf * field_p, 0.0);
        ss += vec4(field_s, wg * field_s, wf * field_s, 0.0);
        // w: the frame's log10 luminance sum (its log-average, the inside reference's floor).
        sw += vec4(1.0, wg, wf, log10(clamp_luminance(lp)));
        if (lp > flash_level) {
            sp.w += lp;
            ss.w += 1.0;
        }
        i += 256u;
    }
    return MeterSum(sp, ss, sw, sv, su);
}

@compute @workgroup_size(8)
fn meter(@builtin(global_invocation_id) id: vec3<u32>) {
    meter_sums[id.x] = sample_meter(id.x);
}

@compute @workgroup_size(256)
fn cs(@builtin(local_invocation_index) li: u32) {
    finish_meter(li, meter_sums[li]);
}

// Retained solely for matched benchmark/regression comparison with the original scheduling.
@compute @workgroup_size(256)
fn cs_serial(@builtin(local_invocation_index) li: u32) {
    finish_meter(li, sample_meter(li));
}

fn finish_meter(li: u32, sum: MeterSum) {
    partial_p[li] = sum.p;
    partial_s[li] = sum.s;
    partial_w[li] = sum.w;
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
    // The glare sums reuse two of the arrays (12 KiB of workgroup memory, under WebGPU's 16 KiB).
    let p = partial_p[0];
    let s = partial_s[0];
    workgroupBarrier();
    partial_p[li] = sum.v;
    partial_s[li] = sum.u;
    workgroupBarrier();
    stride = 128u;
    loop {
        if (stride == 0u) { break; }
        if (li < stride) {
            partial_p[li] += partial_p[li + stride];
            partial_s[li] += partial_s[li + stride];
        }
        workgroupBarrier();
        stride = stride >> 1u;
    }
    if (li != 0u) { return; }

    let w = partial_w[0];
    let v = partial_p[0];
    let u = partial_s[0];
    let field_p = p.x / max(w.x, 1.0) + v.x;
    let field_s = s.x / max(w.x, 1.0) + u.y;
    let gauss_p = select(field_p, p.y / w.y + v.y, w.y > 1e-6);
    let gauss_s = select(field_s, s.y / w.y + u.z, w.y > 1e-6);
    let fovea_p = select(gauss_p, p.z / w.z + v.z, w.z > 0.5);
    let fovea_s = select(gauss_s, s.z / w.z + u.w, w.z > 0.5);
    // One shared view adaptation driven by the incident light in the existing spatial windows.
    // No aperture classification, independent port exposure, or scene-dependent luminance floor.
    let metered_p = clamp_luminance(OBS_FOVEA_WEIGHT * fovea_p + OBS_GAUSSIAN_WEIGHT * gauss_p
        + OBS_FIELD_WEIGHT * field_p + max(clock.veil_photopic, 0.0));
    let metered_s = clamp_luminance(OBS_FOVEA_WEIGHT * fovea_s + OBS_GAUSSIAN_WEIGHT * gauss_s
        + OBS_FIELD_WEIGHT * field_s + max(clock.veil_scotopic, 0.0));
    let mode = u32(clock.mode + 0.5);
    let inside = (mode & 1u) != 0u;
    let optic = (mode & 2u) != 0u;
    // Inside the hull (observer::inside_meter): a quarter of the cabin's log changes about the
    // slow reference. The reference follows the meter everywhere and never lies below the frame's
    // log-average (observer::inside_reference_floor): a port in view barely lifts the log-average,
    // the whole cabin brightening (a fire) lifts it and the reference with it at once. The
    // scotopic log-average is the photopic one shifted by the field's S/P ratio (named).
    let log_average_p = w.w / max(w.x, 1.0);
    let log_average_s = log_average_p + log10(clamp_luminance(field_s) / clamp_luminance(field_p));
    var lp = metered_p;
    var ls = metered_s;
    if (inside && state[1] >= 0.0) {
        state[22] = max(state[22], log_average_p);
        state[23] = max(state[23], log_average_s);
        lp = clamp_luminance(pow(10.0, state[22] + OBS_INSIDE_RESPONSE * (log10(metered_p) - state[22])));
        ls = clamp_luminance(pow(10.0, state[23] + OBS_INSIDE_RESPONSE * (log10(metered_s) - state[23])));
    }
    let field = clamp_luminance(field_p);
    let coverage = s.w / max(w.x, 1.0);
    let bright = select(0.0, p.w / max(s.w, 1.0), s.w > 0.0);
    let monocular = clock.monocular > 0.5;
    // Unprimed: no frame has yet given the glare treatment its background (lane 27).
    let glare_unprimed = (mode & 3u) == 0u && state[27] <= 0.0;
    state[15] = lp;
    state[16] = ls;
    // The glare treatment's state for the next frame: the frame's log-average photopic and
    // scotopic luminance (the background a glare cell stands in for) and the glare cells' share.
    // Diagnostics: the photopic veil added to the field and Gaussian windows.
    state[24] = v.x;
    state[25] = v.y;
    state[27] = pow(10.0, w.w / max(w.x, 1.0));
    state[28] = u.x / max(w.x, 1.0);
    state[29] = clamp_luminance(state[27] * clamp_luminance(field_s) / clamp_luminance(field_p));
    state[19] = coverage;
    state[20] = bright;

    // A view's first observed frame: the crewman is already adapted to what he sees, as a real
    // observer is before the first presented frame. An optic's own eye snaps too when the eye
    // comes back to it (observer::OPTIC_RESNAP_GAP_S).
    // The snap repeats for OBS_SNAP_HOLD seconds of frames (lane 31): a view returned to after a
    // gap shows its first frames from transport histories that are minutes stale (the TZF at night
    // first metered 2.8 cd/m^2 of the noon it last saw, snapped and bleached its cones to it, and
    // stayed black: 2026-11-03, eye-review), so the eye takes its field once the frames are fresh.
    let fresh = state[1] < 0.0 || (optic && clock.time_s - state[1] > OBS_OPTIC_RESNAP_GAP);
    if (fresh) { state[31] = clock.time_s + OBS_SNAP_HOLD; }
    if (fresh || state[30] > 0.5 || (clock.time_s < state[31] && state[1] >= 0.0)) {
        state[22] = log10(metered_p);
        state[23] = log10(metered_s);
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
        // Outdoors, the snap waits one frame for the glare treatment's background, so the eye is
        // not first adapted to small sources the treatment then takes out of the field.
        state[30] = select(0.0, 1.0, glare_unprimed);
        state[1] = clock.time_s;
        write_mapping(monocular);
        return;
    }

    // Simulated seconds since the last observation, integrated in steps of at most 0.25 s.
    let dt = clamp(clock.time_s - state[1], 0.0, 1.0);
    state[1] = clock.time_s;
    let follow = 1.0 - exp(-dt / OBS_INSIDE_REFERENCE_TAU);
    state[22] += (log10(metered_p) - state[22]) * follow;
    state[23] += (log10(metered_s) - state[23]) * follow;
    if (dt > 0.0) {
        let steps = u32(ceil(dt / OBS_MAX_STEP));
        let h = dt / f32(max(steps, 1u));
        for (var k = 0u; k < steps; k += 1u) {
            substep(lp, ls, coverage, bright, field, monocular, clock.transient_source > 0.5, inside, h);
        }
    }
    write_mapping(monocular);
}
