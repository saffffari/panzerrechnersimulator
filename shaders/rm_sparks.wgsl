// Raymarch backend ballistic emitters: analytic spark showers, hot rims, pressure jets, ground
// ejecta and one-pixel tracer strips, from the simulation's 80-byte burst/segment records. The
// trajectory model is the engine's established cosmetic ballistics (weapon_particle_math.rs);
// this program only changes how each sample is lit and attenuated: incandescent samples emit
// through the integrated froxel medium, matte ejecta is lit by the key beam and the sky.
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var integrated: texture_3d<f32>;
@group(0) @binding(3) var linear_samp: sampler;

struct WeaponOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) radiance: vec3<f32>,
    @location(2) @interpolate(flat) opacity: f32,
};
fn weapon_random(seed: u32) -> f32 { return rm_hash01(seed); }
// weapon_particle_math.rs `ground_speed_share`: the crater-ejecta power law, 0.15..1.
fn ground_speed_share(u: f32) -> f32 {
    let floor = pow(0.15, 1.2);
    return pow(floor / (1.0 - clamp(u, 0.0, 1.0) * (1.0 - floor)), 1.0 / 1.2);
}
fn weapon_direction(seed: u32) -> vec3<f32> {
    let y = weapon_random(seed) * 2.0 - 1.0; let a = weapon_random(seed + 1u) * 6.28318530718;
    let r = sqrt(max(0.0, 1.0 - y * y)); return vec3(r * cos(a), y, r * sin(a));
}
fn weapon_rotate(source_axis: vec3<f32>, target_axis: vec3<f32>, v: vec3<f32>) -> vec3<f32> {
    let d = dot(source_axis, target_axis);
    if (d < -0.999999) {
        let axis = normalize(cross(source_axis, select(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), abs(source_axis.x) > 0.9)));
        return axis * (2.0 * dot(axis, v)) - v;
    }
    let q = normalize(vec4(cross(source_axis, target_axis), 1.0 + d));
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}
fn weapon_hidden() -> WeaponOut { return WeaponOut(vec4(2.0, 2.0, 2.0, 1.0), vec3(0.0), vec3(0.0), 0.0); }
// Incandescent spark pages: the hottest page is a warm white-yellow rather than the shader
// backend's near-white, so a dense shower reads as burning steel instead of confetti.
fn weapon_palette(page: u32) -> vec3<f32> {
    if (page == 3u) { return vec3(1.0, 0.82, 0.50) * 6.5; }
    if (page == 4u) { return vec3(1.0, 0.5906, 0.0482) * 4.38; }
    return vec3(1.0, 0.2874, 0.0116) * 1.10;
}
// An incandescent spark's colour: the three Planck-band ratios at its temperature (red = 1), on
// the palette's authored display scale (dull orange 1.1 at 1500 K to 2.4 at 2800 K and above).
// `glow` is (initial kelvin, fraction of life); a spark cools from its own initial temperature
// to a dull 1100 K by the end of its burn. Not calibrated steel emissivity (2026-11-02).
fn spark_glow(glow: vec2<f32>) -> vec3<f32> {
    let kelvin = mix(glow.x, 1100.0, pow(clamp(glow.y, 0.0, 1.0), 0.7));
    let wavelengths = vec3(650.0, 550.0, 450.0);
    let reference = exp(14387769.0 / (650.0 * kelvin)) - 1.0;
    let chroma = pow(vec3(650.0) / wavelengths, vec3(5.0)) * reference / (exp(vec3(14387769.0) / (wavelengths * kelvin)) - vec3(1.0));
    return chroma * mix(1.1, 2.4, clamp((kelvin - 1500.0) / 1300.0, 0.0, 1.0));
}
// `tau` is the rim's cooling time constant (the record's w lane; weapon_particle_math `rim_tau_s`).
fn rim_color(initial_kelvin: f32, age: f32, tau: f32) -> vec3<f32> {
    let kelvin = 293.0 + max(initial_kelvin - 293.0, 0.0) * exp(-max(age, 0.0) / max(tau, 0.05));
    if (kelvin < 700.0) { return vec3(0.0); }
    let wavelengths = vec3(650.0, 550.0, 450.0);
    let reference = exp(14387769.0 / (650.0 * 1800.0)) - 1.0;
    return pow(vec3(650.0) / wavelengths, vec3(5.0)) * reference
        / (exp(vec3(14387769.0) / (wavelengths * kelvin)) - vec3(1.0)) * 4.725;
}

// The strike kinds' shared pieces (weapon_particle_math.rs `strike_flags`, `burst_share`,
// `strike_arc`, the CPU reference): the record's heavy flag and whether `parameters.z` carries
// the ground height; a per-burst share in 0..1 that is the burst's own for its whole life; and
// the arc of a spark under gravity with one bounce off the known ground, then rest on it.
fn spark_strike_flags(w: f32) -> vec2<bool> {
    let known = w >= 2.0;
    return vec2(w - select(0.0, 2.0, known) > 0.5, known);
}
fn spark_burst_share(seed: u32, salt: u32) -> f32 { return weapon_random(seed * 7919u + salt); }
struct SparkArc { center: vec3<f32>, velocity: vec3<f32>, resting: bool };
fn spark_strike_arc(origin: vec3<f32>, velocity: vec3<f32>, t: f32, ground: f32, known: bool, q: f32) -> SparkArc {
    let g = vec3(0.0, 9.81, 0.0);
    if (!known) { return SparkArc(origin + velocity * t - g * (0.5 * t * t), velocity - g * t, false); }
    let drop = max(origin.y - ground, 0.0);
    let contact_t = (velocity.y + sqrt(velocity.y * velocity.y + 19.62 * drop)) / 9.81;
    if (t <= contact_t) { return SparkArc(origin + velocity * t - g * (0.5 * t * t), velocity - g * t, false); }
    let d = velocity * contact_t - g * (0.5 * contact_t * contact_t);
    let v = velocity - g * contact_t;
    let contact = vec3(origin.x + d.x, ground, origin.z + d.z);
    let rebound = vec3(v.x * 0.55, -v.y * (0.2 + 0.2 * clamp(q, 0.0, 1.0)), v.z * 0.55);
    let t2 = t - contact_t;
    let rest_t = 2.0 * rebound.y / 9.81;
    if (t2 <= rest_t) { return SparkArc(contact + rebound * t2 - g * (0.5 * t2 * t2), rebound - g * t2, false); }
    return SparkArc(contact + rebound * rest_t - g * (0.5 * rest_t * rest_t), vec3(0.0), true);
}
fn pixel_quad(center: vec3<f32>, corner: u32) -> vec4<f32> {
    let clip = fr.view_proj * vec4(center - fr.cam_pos.xyz, 1.0);
    if (clip.w <= 0.0001 || clip.z < 0.0 || clip.z >= clip.w) { return vec4(2.0, 2.0, 2.0, 1.0); }
    let ndc = clip.xy / clip.w;
    let pixel = floor(vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * fr.res.xy);
    var corners = array<vec2<f32>, 6>(vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0));
    let edge = (pixel + corners[corner]) / fr.res.xy;
    return vec4(vec2(edge.x * 2.0 - 1.0, 1.0 - edge.y * 2.0) * clip.w, clip.z, clip.w);
}

// Samples drawn along each spark's path for the streaked kinds (vs_spark: 2, 5, 6).
// One sample a spark: the spark where it is at the frame's instant, as every other body in this
// renderer (no motion blur anywhere); 8 and 24 samples along its path drew as bars (2026-11-02).
const SPARK_TRAIL: u32 = 1u;

@vertex fn vs_spark(@builtin(vertex_index) vertex: u32,
    @location(0) origin_age: vec4<f32>, @location(1) direction_life: vec4<f32>,
    @location(2) normal_offset: vec4<f32>, @location(3) parameters: vec4<f32>, @location(4) identity: vec4<u32>) -> WeaponOut {
    let index = vertex / 6u; let corner = vertex % 6u; if (index >= identity.y) { return weapon_hidden(); }
    let kind = identity.w; var t = origin_age.w; let n = normal_offset.xyz;
    // The spark kinds (a blast's, a plate strike's, a ricochet's, a bullet's, a pressure vent's
    // and a cook-off's: 0-3, 5, 6) are the record's identity.z sparks, each a dotted streak of
    // SPARK_TRAIL samples along its own path over 14 ms (an eye's exposure) carrying the spark's
    // whole light; a record carries SPARK_MULTIPLIER (= SPARK_TRAIL) samples a spark. Scattered
    // each in its own direction, a thousandfold quadrature made a continuous sheet, and a shower
    // fanning toward the eye drew as a solid wedge or cone (2026-10-29 for kinds 2, 5, 6; the
    // blast and plate-strike kinds followed on 2026-11-02, the owner's "clusters" on the fx stills).
    let streaked = kind <= 3u || kind == 5u || kind == 6u;
    let sample = select(index, index / SPARK_TRAIL, streaked);
    if (streaked) {
        if (sample >= identity.z) { return weapon_hidden(); }
        t = max(t - f32(index % SPARK_TRAIL) * (0.014 / f32(SPARK_TRAIL)), 0.0);
    }
    let seed = identity.x * select(3948571u, 9348571u, kind <= 1u) + sample * 977u; let q = weapon_random(seed);
    var velocity = vec3(0.0); var axis = vec3(0.0, 0.0, 1.0); var width = 0.0; var tail = 0.0; var page = 2u; var rim_offset = vec3(0.0);
    var gain = select(1.0, f32(identity.y) / (f32(identity.z) * f32(SPARK_TRAIL)), streaked);
    // (initial kelvin, fraction of life) of a streaked spark, for `spark_glow`.
    var glow = vec2(2300.0, 0.0);
    // The strike kinds place their sample themselves (a bounce is not a parabola).
    var arc_center = vec3(0.0); var arc_set = false;
    if (kind == 9u) {
        // Detonation fragment streaks: casing shards and burning residue leaving at 250-1500 m/s
        // (most of them slow), slowing under drag (x = v0 tau (1 - e^(-t/tau))), incandescent
        // for a few hundredths to a quarter of a second. Ten samples trail each shard along its
        // own path so a streak is a dotted radial line, not a shell of points. The ground sends
        // them up and out.
        let shard = index / 10u; let trail = index % 10u;
        let fs = identity.x * 3948571u + shard * 977u;
        let fq = weapon_random(fs);
        let life = 0.04 + 0.2 * fq * fq; if (t >= life) { return weapon_hidden(); }
        let ts = max(t - f32(trail) * 0.0012, 0.0);
        var ray = weapon_direction(fs + 23u);
        ray.y = ray.y * 0.8 + 0.25;
        ray = normalize(ray);
        let sq = weapon_random(fs + 29u);
        let v0 = 250.0 + 1250.0 * sq * sq;
        let tau = 0.11;
        let travelled = v0 * tau * (1.0 - exp(-ts / tau));
        t = ts;
        velocity = ray * travelled / max(ts, 0.0005);
        axis = ray;
        width = 0.012 + 0.012 * fq; tail = clamp(v0 * exp(-ts / tau) * 0.002, 0.2, 3.0);
        page = select(4u, 3u, ts < 0.05); gain = 0.25 * (1.0 - ts / life) * (1.0 - 0.06 * f32(trail));
    } else if (kind == 10u) {
        // Burning fragments and residue: heavy shards arcing out at 15-55 m/s, glowing for
        // seconds, coming to rest on the ground (parameters.x = ground y, y = 1 when known).
        let life = 2.0 + 3.0 * q; if (t >= life) { return weapon_hidden(); }
        var ray = weapon_direction(seed + 23u); ray.y = abs(ray.y) * 0.9 + 0.3; ray = normalize(ray);
        velocity = ray * (15.0 + 40.0 * weapon_random(seed + 29u));
        if (parameters.y > 0.5) {
            let drop = origin_age.y - parameters.x;
            let t_land = (velocity.y + sqrt(max(velocity.y * velocity.y + 19.62 * drop, 0.0))) / 9.81;
            t = min(t, t_land);
        }
        let current = velocity - vec3(0.0, 9.81 * t, 0.0); axis = current / max(length(current), 0.000001);
        let landed = parameters.y > 0.5 && origin_age.w > t + 0.0001;
        width = 0.018 + 0.02 * q; tail = select(clamp(length(current) * 0.01, width, 0.4), width * 0.5, landed);
        page = select(select(2u, 4u, origin_age.w < 0.6), 3u, origin_age.w < 0.12);
        gain = pow(1.0 - origin_age.w / life, 1.5) * 0.3;
    } else if (kind == 11u) {
        // Molten spray off struck armour: droplets 20-80 m/s in a cone about the plate normal,
        // cooling through white, yellow and red over a second.
        let life = 0.5 + 0.7 * q; if (t >= life) { return weapon_hidden(); }
        let spray = weapon_direction(seed + 23u); let outward = spray - n * (2.0 * min(dot(spray, n), 0.0));
        let ray = normalize(n * (0.5 + 0.5 * q) + outward * 0.8 + direction_life.xyz * 0.15);
        velocity = ray * (20.0 + 60.0 * weapon_random(seed + 29u));
        let current = velocity - vec3(0.0, 9.81 * t, 0.0); axis = current / max(length(current), 0.000001);
        width = 0.006 + 0.008 * q; tail = clamp(length(current) * 0.006, width, 0.35);
        page = select(select(2u, 4u, t < 0.35), 3u, t < 0.12); gain = 0.8 * (1.0 - t / life);
    } else if (kind == 8u) {
        if (t >= 6.0) { return weapon_hidden(); }
        let inner = parameters.x; let outer = inner + parameters.y;
        let radius = sqrt(inner * inner + (outer * outer - inner * inner) * (f32(index) + 0.5) / f32(identity.y));
        let angle = f32(index) * 2.3999631 + weapon_random(identity.x) * 6.28318530718;
        rim_offset = (direction_life.xyz * cos(angle) + cross(n, direction_life.xyz) * sin(angle)) * radius;
        width = sqrt(3.14159265359 * (outer * outer - inner * inner) / f32(identity.y)); axis = n;
    } else if (kind == 7u) {
        if (t >= 3.0) { return weapon_hidden(); }
        let spray = weapon_direction(seed + 19u); let outward = spray - n * (2.0 * min(dot(spray, n), 0.0));
        let tangent_raw = direction_life.xyz - n * dot(direction_life.xyz, n);
        let tangent = tangent_raw / max(length(tangent_raw), 0.000001);
        let grazing = 1.0 - clamp(-dot(direction_life.xyz, n), 0.0, 1.0);
        let ray = select(normalize(n * (0.6 + 0.4 * q) + tangent * (0.15 + 0.60 * grazing) + outward * 0.4), n, index == 0u);
        let speed = min(sqrt(2.0 * parameters.x / max(parameters.y, 0.0001)), sqrt(2.0 * 9.81 * 6.096));
        // Speed apart from direction, by the crater-ejecta power law (weapon_particle_math.rs
        // `ground_speed_share`).
        velocity = ray * speed * select(ground_speed_share(weapon_random(seed + 41u)), 1.0, index == 0u);
        let current_velocity = velocity - vec3(0.0, 9.81 * t, 0.0); axis = current_velocity / max(length(current_velocity), 0.000001);
        width = 2.0 * pow(3.0 * parameters.y / (4.0 * 3.14159265359 * parameters.w * f32(identity.y)), 1.0 / 3.0);
        tail = width; page = 6u;
    } else if (kind == 5u || kind == 6u) {
        let delay = select(q * select(0.45, direction_life.w, kind == 5u), 0.0, sample == 0u);
        t -= delay; let life = select(17.0, 0.65, kind == 5u); if (t < 0.0 || t >= life) { return weapon_hidden(); }
        let spray = weapon_direction(seed + 19u);
        if (kind == 5u) { velocity = normalize(n + spray * 0.11) * (26.0 + 16.0 * q); }
        else {
            let horizontal = vec3(n.x, 0.0, n.z) / max(length(n.xz), 0.000001);
            velocity = vec3(0.0, 77.3316 * select(0.7 + 0.3 * q, 1.0, sample == 0u), 0.0)
                + horizontal * (6.0 + 12.0 * q) + vec3(spray.x, 0.0, spray.z) * 12.0;
            if (dot(velocity, n) < 0.0) { velocity -= n * dot(velocity, n); }
            let speed_cap = sqrt(max(0.0, 2.0 * parameters.x / max(parameters.y, 0.001)));
            velocity *= min(1.0, speed_cap / max(length(velocity), 0.000001));
        }
        let current_velocity = velocity - vec3(0.0, 9.81 * t, 0.0); axis = current_velocity / max(length(current_velocity), 0.000001);
        width = select((0.007 + 0.006 * q) * exp(-t / 8.0), (0.008 + 0.006 * q) * sqrt(1.0 - t / life), kind == 5u);
        tail = clamp(length(velocity - vec3(0.0, 9.81 * t, 0.0)) * 0.002, width, 0.15);
        page = select(2u, 4u, t < 0.15);
        glow = vec2(2300.0 + 600.0 * weapon_random(seed + 37u), t / life);
    } else if (kind == 3u) {
        // Incandescent dust off a perforated or bounced plate: the solver's energy and
        // incidence; each spark's size sets how long it burns; this burst's share tilts the fan a
        // few degrees and spreads the leaving times over a few milliseconds.
        let flags = spark_strike_flags(parameters.w);
        let b1 = spark_burst_share(identity.x, 1u); let b2 = spark_burst_share(identity.x, 2u);
        t -= weapon_random(seed + 23u) * (0.004 + 0.008 * b1); if (t < 0.0) { return weapon_hidden(); }
        let size = weapon_random(seed + 29u);
        let life = (0.08 + 0.22 * q) * (0.7 + 0.6 * size); if (t >= life) { return weapon_hidden(); }
        let spray = weapon_direction(seed + 19u); let outward = spray - n * (2.0 * min(dot(spray, n), 0.0));
        let cosine = parameters.y;
        let tilt = weapon_direction(identity.x * 31u + 7u) * 0.14;
        let ray = normalize(direction_life.xyz * (1.0 - cosine) * 0.8 + outward * 0.55 + n * 0.25 + tilt);
        let speed = min(sqrt(parameters.x / 3200.0), 2.0) * (4.0 + 18.0 * pow(q, 1.4)) * (0.4 + 0.6 * sqrt(cosine)) * (0.9 + 0.2 * b2);
        let arc = spark_strike_arc(origin_age.xyz, ray * speed, t, parameters.z, flags.y, weapon_random(seed + 31u));
        arc_center = arc.center; arc_set = true; velocity = ray * speed;
        axis = select(normalize(arc.velocity), normalize(vec3(ray.x, 0.0, ray.z)), arc.resting);
        width = (0.0015 + 0.002 * size) * sqrt(1.0 - t / life);
        tail = select(clamp(speed * 0.002, width, 0.04), width, arc.resting);
        page = select(select(2u, 4u, t < 0.4 * life), 3u, t < 0.1 * life);
        glow = vec2(2200.0 + 900.0 * weapon_random(seed + 37u), t / life);
    } else if (kind == 2u) {
        // The fan off a glancing plate: along the outgoing ray, lifting off the plate more and
        // spreading wider the steeper the ricochet, a slow-heavy speed law scaled by the round's
        // retained energy, each spark burning for a time set by its size, this burst's share
        // setting its fan width, its speed share and the spread of leaving times, and the sparks
        // bouncing off the ground once (weapon_particle_math.rs is the reference).
        let flags = spark_strike_flags(parameters.w); let heavy = flags.x;
        let b1 = spark_burst_share(identity.x, 1u); let b2 = spark_burst_share(identity.x, 2u); let b3 = spark_burst_share(identity.x, 3u);
        let cosine = clamp(dot(direction_life.xyz, n), 0.0, 1.0);
        let hot = select(1.0, clamp(sqrt(parameters.x / select(2500.0, 1.8e6, heavy)), 0.75, 1.35), parameters.x > 0.0);
        t -= weapon_random(seed + 23u) * select(0.006 + 0.010 * b1, 0.012 + 0.020 * b1, heavy); if (t < 0.0) { return weapon_hidden(); }
        let size = weapon_random(seed + 29u);
        let life = select(0.2 + 0.4 * q, 0.45 + 0.9 * q, heavy) * (0.7 + 0.6 * size); if (t >= life) { return weapon_hidden(); }
        let spread = (0.18 + 0.32 * cosine) * (0.85 + 0.3 * b2);
        var ray = normalize(direction_life.xyz * (0.55 + 0.3 * (1.0 - cosine)) + n * (0.12 + 0.45 * cosine) + weapon_direction(seed + 19u) * spread);
        if (dot(ray, n) < 0.05) { ray = normalize(ray + n * (0.06 - dot(ray, n))); }
        let speed = select(4.0 + 16.0 * pow(q, 1.5), 12.0 + 34.0 * pow(q, 1.5), heavy) * hot * (0.9 + 0.2 * b3);
        let arc = spark_strike_arc(origin_age.xyz, ray * speed, t, parameters.z, flags.y, weapon_random(seed + 31u));
        arc_center = arc.center; arc_set = true; velocity = ray * speed;
        axis = select(normalize(arc.velocity), normalize(vec3(ray.x, 0.0, ray.z)), arc.resting);
        width = select(0.0025 + 0.005 * size, 0.006 + 0.012 * size, heavy) * max(1.0 - t / life, 0.15);
        tail = select(clamp(length(arc.velocity) * 0.008, 0.025, 0.32), width, arc.resting);
        let white = 0.8 + 0.4 * hot;
        page = select(select(2u, 4u, t < 0.18 * life * white), 3u, t < 0.05 * life * white);
        glow = vec2((2100.0 + 900.0 * weapon_random(seed + 37u)) * (0.85 + 0.15 * hot), t / life);
    } else {
        // A heavy exterior blast's sparks trail four samples along each path (a dotted
        // radial streak) instead of filling a dome with points; light impacts are unchanged.
        var sseed = seed; var sq = q;
        let heavy_blast = kind == 0u && parameters.w > 0.5 && parameters.z > 2.0;
        if (heavy_blast) {
            let shard = index / 4u; let trail = index % 4u;
            // Every third shard: the record's count fills a dome otherwise.
            if (shard % 3u != 0u) { return weapon_hidden(); }
            sseed = identity.x * 9348571u + shard * 977u; sq = weapon_random(sseed);
            t = max(t - f32(trail) * 0.0025, 0.0);
            gain = 0.55 * (1.0 - 0.12 * f32(trail));
        }
        // The hemisphere's lift and narrowing, the speed share, the lifetimes and the spread of
        // leaving times are this burst's own; a blast's parameters.x is its charge against an
        // 88 mm HE round's (the reference is weapon_particle_math.rs).
        let b1 = spark_burst_share(identity.x, 1u); let b2 = spark_burst_share(identity.x, 2u); let b3 = spark_burst_share(identity.x, 3u);
        let b4 = spark_burst_share(identity.x, 4u); let b5 = spark_burst_share(identity.x, 5u);
        let charge = select(1.0, clamp(sqrt(parameters.x), 0.7, 1.3), kind == 0u && parameters.x > 0.0);
        t -= weapon_random(sseed + 23u) * (0.01 + 0.03 * b1); if (t < 0.0) { return weapon_hidden(); }
        let size = weapon_random(sseed + 29u);
        let life = (0.45 + 1.4 * sq) * (0.8 + 0.4 * b2) * (0.75 + 0.5 * size); if (t >= life) { return weapon_hidden(); }
        var ray = weapon_direction(sseed + 17u); ray.y = abs(ray.y) * (0.6 + 0.3 * b3) + (0.08 + 0.2 * b4);
        if (kind == 1u) { ray = weapon_rotate(vec3(0.0, 1.0, 0.0), n, ray); }
        velocity = normalize(ray) * (10.0 + 28.0 * pow(sq, 1.3)) * parameters.z * charge * (0.9 + 0.2 * b5);
        let current = velocity - vec3(0.0, 9.81 * t, 0.0); axis = normalize(current);
        width = (0.006 + 0.012 * size) * sqrt(1.0 - t / life); tail = min(length(current) * 0.008, 0.32);
        page = select(select(2u, 4u, t < 0.35 * life), 3u, t < 0.08 * life);
        glow = vec2(2200.0 + 1200.0 * weapon_random(seed + 37u) * select(0.75, 1.0, kind == 0u), t / life);
    }
    var center = select(origin_age.xyz + velocity * t - vec3(0.0, 4.905 * t * t, 0.0), origin_age.xyz + rim_offset, kind == 8u);
    if (arc_set) { center = arc_center; }
    if (kind != 0u && kind < 9u) {
        let local_normal = weapon_rotate(axis, vec3(0.0, 0.0, 1.0), n);
        let support = dot(abs(local_normal), vec3(width, width, tail)) * 0.5;
        if (dot(center - origin_age.xyz, n) + normal_offset.w < support) { return weapon_hidden(); }
    }
    let clip = fr.view_proj * vec4(center - fr.cam_pos.xyz, 1.0);
    if (clip.w <= 0.0001 || clip.z < 0.0 || clip.z >= clip.w) { return weapon_hidden(); }
    let focal = fr.view_proj[1][1] * fr.res.y * 0.5;
    // A streak drawn as one native pixel carries its whole projected area; cap it so a shard
    // passing the eye does not become a bloom blob.
    // A streaked spark covers its cross-section only: its tail is the support test's, not a smear
    // of its light into the pixel (which made every spark a bloom blob).
    // A hot-rim grain (kind 8) is one of 512 on a ring a few pixels across: capped at a pixel so the
    // ring stays a ring (the plate's own rim draws the surface; these are its incandescent dust).
    let projected_area = min(select(width * (width + tail), width * width, streaked) * pow(focal / clip.w, 2.0), select(select(6.0, 3.0, kind >= 9u), 1.0, kind == 8u));
    let weight = projected_area * f32(identity.z) / f32(identity.y);
    let position = pixel_quad(center, corner);
    if (kind == 7u) {
        let area = width * width * 0.78539816339 * pow(focal / clip.w, 2.0);
        return WeaponOut(position, center, mix(vec3(0.16, 0.115, 0.075), vec3(0.65, 0.72, 0.80), parameters.z), area);
    }
    if (kind == 8u) { return WeaponOut(position, center, rim_color(parameters.z, t, parameters.w) * weight, 0.0); }
    return WeaponOut(position, center, select(weapon_palette(page), spark_glow(glow), streaked) * weight * gain, 0.0);
}

// A small-arms tracer pellet's intensity, renderer units (`TRACER_INTENSITY` in
// renderer_raymarch.rs, about 60 cd; its head lights the scene with the same value).
const RM_TRACER_INTENSITY: f32 = 0.0035;

// The eyepiece's barrel distortion (rm_lens.wgsl: the frame at q shows the scene at q (1 + k r^2),
// r = |q| / 0.45) applied to a clip position drawn after the lens pass: the point where the scene
// point lands, by two fixed-point steps. Only while the TZF (optic 1) or the binoculars (3) are up.
fn eyepiece_clip(c: vec4<f32>) -> vec4<f32> {
    let mode = fr.optic.x;
    let tzf = abs(mode - 1.0) < 0.5;
    let binocular = abs(mode - 3.0) < 0.5;
    if (!(tzf || binocular) || c.w <= 0.0) { return c; }
    let aspect = fr.res.x / fr.res.y;
    let k = select(0.055, 0.035, binocular);
    let s = (c.xy / c.w) * vec2(0.5 * aspect, 0.5);
    var q = s;
    for (var i = 0; i < 2; i += 1) { q = s / (1.0 + k * dot(q, q) / (0.45 * 0.45)); }
    return vec4(q / vec2(0.5 * aspect, 0.5) * c.w, c.z, c.w);
}

@vertex fn vs_tracer(@builtin(vertex_index) vertex: u32,
    @location(0) start: vec4<f32>, @location(1) end: vec4<f32>,
    @location(2) unused_normal: vec4<f32>, @location(3) unused_parameters: vec4<f32>, @location(4) unused_meta: vec4<u32>) -> WeaponOut {
    var a = fr.view_proj * vec4(start.xyz - fr.cam_pos.xyz, 1.0);
    var b = fr.view_proj * vec4(end.xyz - fr.cam_pos.xyz, 1.0); var wa = start.xyz; var wb = end.xyz;
    if (a.z < 0.0 && b.z < 0.0) { return weapon_hidden(); }
    if (a.z < 0.0) { let t = a.z / (a.z - b.z); a = mix(a, b, t); a.z = 0.0; wa = mix(wa, wb, t); }
    if (b.z < 0.0) { let t = b.z / (b.z - a.z); b = mix(b, a, t); b.z = 0.0; wb = mix(wb, wa, t); }
    if (a.w <= 0.0 || b.w <= 0.0) { return weapon_hidden(); }
    a = eyepiece_clip(a);
    b = eyepiece_clip(b);
    let pa = a.xy / a.w * fr.res.xy * 0.5; let pb = b.xy / b.w * fr.res.xy * 0.5;
    let delta = pb - pa;
    // The strip is the pellet's path over this frame's exposure and carries exactly its light: each
    // pixel I x focal^2 / (depth^2 x the strip's length in pixels), so the whole strip delivers
    // I / depth^2 to the eye at any length or distance (2026-10-28: a fixed ~175 000 cd/m2 made a
    // near tracer some ten thousand times too bright, its glare a broad band across the view).
    let focal = 0.5 * fr.res.y / tan(0.5 * fr.optic.y);
    let per_pixel = RM_TRACER_INTENSITY * focal * focal / max(length(delta), 1.0);
    let tint = vec3(1.0, 0.705, 0.348);
    if (dot(delta, delta) <= 0.00000001) {
        let w = max(0.5 * (a.w + b.w), 0.05);
        return WeaponOut(pixel_quad((wa + wb) * 0.5, vertex), (wa + wb) * 0.5, tint * per_pixel / (w * w), 0.0);
    }
    let normal = normalize(vec2(-delta.y, delta.x));
    var endpoints = array<u32, 6>(0u, 1u, 0u, 0u, 1u, 1u); var sides = array<f32, 6>(-0.5, -0.5, 0.5, 0.5, -0.5, 0.5);
    let endpoint = endpoints[vertex]; var clip = select(a, b, endpoint == 1u);
    clip.x += normal.x * sides[vertex] * 2.0 / fr.res.x * clip.w;
    clip.y += normal.y * sides[vertex] * 2.0 / fr.res.y * clip.w;
    let w = max(clip.w, 0.05);
    return WeaponOut(clip, select(wa, wb, endpoint == 1u), tint * per_pixel / (w * w), 0.0);
}

@fragment fn fs_weapon(in: WeaponOut) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(depth_tex)); let pixel = vec2<i32>(in.position.xy);
    if (any(pixel < vec2(0)) || any(pixel >= dims) || in.position.z >= textureLoad(depth_tex, pixel, 0)) { discard; }
    let rel = in.world - fr.cam_pos.xyz;
    let dist = length(rel);
    let slice = clamp((rm_slice(dist) - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
    let uv = (vec2<f32>(pixel) + vec2(0.5)) * fr.res.zw;
    let transmittance = textureSampleLevel(integrated, linear_samp, vec3(uv, slice), 0.0).a;
    if (in.opacity > 0.0) {
        // Matte ejecta: lit like a small rough surface facing up.
        let alpha = 1.0 - exp(-in.opacity);
        var lit = rm_sky_hemisphere(vec3(0.0, 1.0, 0.0)) * mix(0.1, 1.0, rm_sky_visibility(in.world, 0.0));
        if (fr.sun_radiance.w > 0.5) {
            lit += fr.sun_radiance.rgb * max(fr.sun_dir.y, 0.0) * rm_sun_visibility(in.world, 0.05) / RM_PI;
        }
        return vec4(in.radiance * lit * alpha * transmittance, alpha * transmittance);
    }
    return vec4(in.radiance * transmittance, 0.0);
}
