// Geometry-derived importance proposals; no emission, light gain, or visibility is stored here.
struct DaylightTriangle { a_area: vec4<f32>, b_cumulative: vec4<f32>, c: vec4<f32>, };
struct DaylightProposals {
    counts: vec4<f32>, center: vec4<f32>, triangles: array<DaylightTriangle, 64>,
};
@group(DAYLIGHT_GROUP) @binding(6) var<uniform> daylight_proposals: DaylightProposals;
struct DaylightDirection { direction: vec3<f32>, pdf: f32, };
fn daylight_mixture_weight(p: vec3<f32>) -> f32 {
    if (daylight_proposals.counts.x < 0.5 || daylight_proposals.counts.y <= 0.0
        || distance(p, daylight_proposals.center.xyz) > daylight_proposals.counts.z) { return 0.0; }
    return 0.5;
}
fn daylight_area_pdf(p: vec3<f32>, d: vec3<f32>) -> f32 {
    var pdf = 0.0;
    // Sum every preimage of this direction, including opposite faces of a closed pane. Using
    // only the sampled or nearest triangle would double-count overlapping area proposals.
    for (var i = 0u; i < u32(daylight_proposals.counts.x); i += 1u) {
        let tri = daylight_proposals.triangles[i];
        let e1 = tri.b_cumulative.xyz - tri.a_area.xyz;
        let e2 = tri.c.xyz - tri.a_area.xyz;
        let h = cross(d, e2);
        let det = dot(e1, h);
        if (abs(det) < 1e-12) { continue; }
        let delta = p - tri.a_area.xyz;
        let u = dot(delta, h) / det;
        let q = cross(delta, e1);
        let v = dot(d, q) / det;
        let t = dot(e2, q) / det;
        if (u < 0.0 || v < 0.0 || u + v > 1.0 || t <= 0.0) { continue; }
        let cosine = abs(dot(normalize(cross(e1, e2)), d));
        pdf += t * t / (daylight_proposals.counts.y * max(cosine, 1e-12));
    }
    return pdf;
}
fn daylight_sample(p: vec3<f32>, n: vec3<f32>, base_direction: vec3<f32>,
                   base_pdf: f32, random: vec4<f32>, cosine_base: bool,
                   phase_g: f32) -> DaylightDirection {
    let mixture = daylight_mixture_weight(p);
    var out = DaylightDirection(base_direction, base_pdf);
    if (mixture == 0.0) { return out; }
    if (random.x < mixture) {
        let area_sample = random.y * daylight_proposals.counts.y;
        var selected = u32(daylight_proposals.counts.x) - 1u;
        for (var i = 0u; i < u32(daylight_proposals.counts.x); i += 1u) {
            if (area_sample < daylight_proposals.triangles[i].b_cumulative.w) { selected = i; break; }
        }
        let tri = daylight_proposals.triangles[selected];
        let root = sqrt(random.z);
        let point = tri.a_area.xyz * (1.0 - root)
            + tri.b_cumulative.xyz * (root * (1.0 - random.w))
            + tri.c.xyz * (root * random.w);
        // A receiver exactly on a proposal has zero measure. Keep a finite baseline sample.
        if (dot(point - p, point - p) > 1e-20) { out.direction = normalize(point - p); }
    }
    // Re-evaluate BOTH proposal densities at the final direction, including a selected pane.
    // Volume's baseline is HG around n; surface's baseline remains cosine about its normal.
    let g = clamp(phase_g, -0.999, 0.999);
    let phase_pdf = (1.0 - g * g) / (4.0 * 3.14159265359
        * pow(max(1.0 + g * g - 2.0 * g * clamp(dot(n, out.direction), -1.0, 1.0), 1e-6), 1.5));
    let directional_base_pdf = select(phase_pdf,
        max(dot(n, out.direction), 0.0) / 3.14159265359, cosine_base);
    out.pdf = (1.0 - mixture) * directional_base_pdf
        + mixture * daylight_area_pdf(p, out.direction);
    return out;
}
