//! Shared shader composition for bounded source-side protective-glass transport.

pub const SHADER: &str = include_str!("../shaders/protective_glass.wgsl");
pub const MARKER: &str = "// @protective-glass-source-transport";

/// The surface and volume pass already bind the same exact packed scene. Only their TLAS variable
/// names and group indices differ; the numerical geometry/optics implementation stays identical.
pub fn inject(source: &str, volume: bool) -> String {
    let sampling_marker = "// @daylight-geometry-sampling";
    let sampling = if source.contains("var smoke_scene_tlas:") || !volume {
        include_str!("../shaders/daylight_sampling.wgsl")
            .replace("DAYLIGHT_GROUP", if volume { "1" } else { "2" })
    } else {
        // No ray-visible geometry on portable: preserve its baseline sampler and dark source.
        "struct DaylightDirection { direction: vec3<f32>, pdf: f32, };\nfn daylight_mixture_weight(p: vec3<f32>) -> f32 { return 0.0; }\nfn daylight_sample(p: vec3<f32>, n: vec3<f32>, base_direction: vec3<f32>, base_pdf: f32, random: vec4<f32>, cosine_base: bool, phase_g: f32) -> DaylightDirection { return DaylightDirection(base_direction, base_pdf); }".to_owned()
    };
    let source = source.replace(sampling_marker, &sampling);
    if !source.contains(MARKER) {
        return source;
    }
    assert_eq!(source.matches(MARKER).count(), 1);
    let shared = SHADER
        .replace(
            "source_scene_tlas",
            if volume { "smoke_scene_tlas" } else { "tlas" },
        )
        .replace("source_vertices_a", "rt_vertices_a")
        .replace("source_vertices_b", "rt_vertices_b")
        .replace("source_records", "rt_records")
        .replace("source_instances", "rt_instances");
    source.replacen(MARKER, &shared, 1)
}
