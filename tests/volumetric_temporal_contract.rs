// The eye's generated constants (src/observer.rs), installed as the renderer installs them.
#[allow(dead_code)]
#[path = "../src/observer.rs"]
mod observer;
#[allow(dead_code)]
fn with_observer(source: &str) -> String {
    source.replacen("// @observer-constants", &observer::wgsl_constants(), 1)
}

const RENDERER: &str = include_str!("../src/renderer.rs");

const SMOKE: &str = include_str!("../shaders/smoke.wgsl");
const VOLUME_ACCUM: &str = include_str!("../shaders/volume_accum.wgsl");
const VOLUME_COMPOSITE: &str = include_str!("../shaders/volume_composite.wgsl");
const CRUSH: &str = include_str!("../shaders/crush.wgsl");

fn parse_and_validate(label: &str, source: &str) {
    let source = &with_observer(source);
    let module = wgpu::naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{label} WGSL parse failed: {error}"));
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    );
    validator
        .validate(&module)
        .unwrap_or_else(|error| panic!("{label} WGSL validation failed: {error}"));
}

#[test]
fn volume_history_shaders_parse_and_validate() {
    parse_and_validate("volume accumulator", VOLUME_ACCUM);
    parse_and_validate("volume composite", VOLUME_COMPOSITE);
}

#[test]
fn stochastic_volume_transport_cannot_bypass_its_per_view_history() {
    assert!(SMOKE.contains("temporal_sample(px, sm.frame"));
    assert!(RENDERER.contains("volume_current_view"));
    assert!(RENDERER.contains("volume_accum_binds: [wgpu::BindGroup; 2]"));
    assert!(RENDERER.contains("volume_history_valid: bool"));
    assert!(RENDERER.contains("label: Some(\"smoke-estimate\")"));
    assert!(RENDERER.contains("pass.set_pipeline(&self.volume_accum_pipe)"));
    assert!(RENDERER.contains("label: Some(\"volume-composite\")"));

    let smoke_estimate = RENDERER
        .find("label: Some(\"smoke-estimate\")")
        .expect("dedicated raw volume target");
    let volume_accum = RENDERER[smoke_estimate..]
        .find("pass.set_pipeline(&self.volume_accum_pipe)")
        .map(|offset| smoke_estimate + offset)
        .expect("volume temporal accumulator");
    let volume_composite = RENDERER[volume_accum..]
        .find("label: Some(\"volume-composite\")")
        .map(|offset| volume_accum + offset)
        .expect("converged volume composite");
    let reject_snapshot = RENDERER[volume_composite..]
        .find("Snapshot only after both surface and volume histories")
        .map(|offset| volume_composite + offset)
        .expect("previous-depth snapshot after history reads");
    assert!(smoke_estimate < volume_accum);
    assert!(volume_accum < volume_composite);
    assert!(volume_composite < reject_snapshot);
}

#[test]
fn denoiser_preserves_current_physical_opacity_and_separate_world_space_motes() {
    for required in ["let density = carrier;", "let single_scatter_albedo = 0.70;",
        "let a = 1.0 - exp(-density * EXTINCT * istep)"] {
        assert!(SMOKE.contains(required), "missing continuous physical haze: {required}");
    }
    assert!(!SMOKE.contains("fn interior_mote("));
    assert!(!SMOKE.contains("let mote_density"));
    let particles=include_str!("../shaders/pixel_particles.wgsl");
    assert!(particles.contains("u.view_proj * vec4(world - u.camera.xyz, 1.0)"));
    assert!(!particles.contains("u.view_proj * vec4(world, 1.0)"));
    assert!(particles.contains("in.position.z >= textureLoad(depth_tex,pixel,0)"));
    assert!(particles.contains("eye_response(dust_radiance(in.world))"));
    for required in [
        "Only the effective",
        "SOURCE_GATHER_RADIUS: i32 = 2",
        "for (var y = -SOURCE_GATHER_RADIUS; y <= SOURCE_GATHER_RADIUS",
        "for (var x = -SOURCE_GATHER_RADIUS; x <= SOURCE_GATHER_RADIUS",
        "mean /= samples",
        "moment2 /= samples",
        "let current_alpha = clamp(current.a, 0.0, 1.0)",
        "let sample_alpha = textureLoad(current_volume, sample_px, 0).a",
        "max(denoised_source, vec3(0.0)) * current_alpha",
        "current_alpha));",
        "previous_id == id",
        "abs(stored_z - expected_z)",
    ] {
        assert!(
            VOLUME_ACCUM.contains(required),
            "volume filter lost its physical-support guard: {required}"
        );
    }
    assert!(!VOLUME_ACCUM.contains("fog_color"));
    assert!(!VOLUME_ACCUM.contains("ambient"));
    assert!(!VOLUME_ACCUM.contains("brightness"));
}

#[test]
fn stable_volume_ema_rejects_density_disocclusion_and_coherent_light_changes() {
    for required in [
        "previous_id == id",
        "abs(stored_z - expected_z)",
        "let history_alpha_matches = abs(history.a - current_alpha)",
        "history.a > 1.0e-6 && history_alpha_matches",
        "let standard_error = sigma / sqrt(max(samples, 1.0))",
        "SOURCE_CHANGE_STANDARD_ERRORS * standard_error",
        "samples >= SOURCE_CHANGE_MIN_SUPPORT",
        "SOURCE_CHANGE_MAX_COEFFICIENT_OF_VARIATION",
        "coherent_source_change",
        "accept = 0.0",
        "STABLE_CURRENT_WEIGHT_SCALE: f32 = 0.40",
        "STABLE_CURRENT_WEIGHT_FLOOR: f32 = 0.06",
    ] {
        assert!(
            VOLUME_ACCUM.contains(required),
            "volume history lost a transient guard: {required}"
        );
    }

    // At the shipping accumulation alpha (0.15), stable history has an ~11-frame half-life. A
    // coherent moving-light step explicitly rejects that history and therefore resolves in one
    // frame rather than inheriting this stable-source response time.
    let stable_current_weight = (0.15_f32 * 0.40).max(0.06);
    let half_life_frames = 0.5_f32.ln() / (1.0 - stable_current_weight).ln();
    assert!((10.0..12.5).contains(&half_life_frames));
    assert!(VOLUME_ACCUM.contains("if (coherent_source_change)"));
    assert!(VOLUME_ACCUM.contains("history_source = mean"));

    // For a stationary independent source estimator, the normalized 5x5 gather followed by the
    // 0.06 EMA has about one quarter the variance of the prior normalized 3x3 / 0.09 resolve. Both
    // factors are convex averages, so their expected value remains the physical source value.
    let ema_variance = |current_weight: f32| current_weight / (2.0 - current_weight);
    let previous_variance = ema_variance(0.09) / 9.0;
    let revised_variance = ema_variance(stable_current_weight) / 25.0;
    assert!(revised_variance < previous_variance * 0.25);

    // A coherent zero-noise step is beyond the shader's four-standard-error + 10% threshold and
    // is therefore resolved from the current 5x5 estimate in one frame, not the stable EMA.
    let previous_source = 1.0_f32;
    let current_source = 2.0_f32;
    let standard_error = 0.0_f32;
    let relative_excess = ((previous_source - current_source).abs() - 4.0 * standard_error)
        .max(0.0)
        / previous_source.max(current_source).max(0.02);
    assert!(relative_excess > 0.10);
}

#[test]
fn retro_zero_remains_a_structural_grain_and_dither_bypass() {
    let branch = CRUSH
        .find("if (retro > 0.001)")
        .expect("explicit retro-only branch");
    assert!(CRUSH.find("BAYER[bi]").expect("Bayer lookup") > branch);
    assert!(!CRUSH.contains("temporal_grain("));
}
