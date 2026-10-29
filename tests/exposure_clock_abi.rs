//! Regression for the eye's clock uniform ABI and its meter contract.
//!
//! The source-qualified flash lane expands the clock to eight scalar lanes. Explicit scalar
//! padding avoids the implicit vec3 alignment which previously mismatched host uploads.

const EXPOSURE_WGSL: &str = include_str!("../shaders/exposure.wgsl");
const RENDERER_RS: &str = include_str!("../src/renderer.rs");

/// The eye shader with a stand-in constant block (the renderer installs the generated one).
fn composed() -> String {
    let names = ["OBS_PHOTOPIC_R", "OBS_PHOTOPIC_G", "OBS_PHOTOPIC_B", "OBS_SCOTOPIC_R",
        "OBS_SCOTOPIC_G", "OBS_SCOTOPIC_B", "OBS_LUMINANCE_PER_UNIT_RADIANCE", "OBS_METER_MIN",
        "OBS_METER_MAX", "OBS_FOVEA_RADIUS_DEG", "OBS_GAUSSIAN_SIGMA_DEG", "OBS_FOVEA_WEIGHT",
        "OBS_GAUSSIAN_WEIGHT", "OBS_FIELD_WEIGHT", "OBS_GRID_X", "OBS_GRID_Y", "OBS_AGE",
        "OBS_BINOCULAR_FIELD", "OBS_SIGHT_FIELD", "OBS_PUPIL_CONSTRICT_TAU",
        "OBS_PUPIL_DILATE_FAST_TAU", "OBS_PUPIL_DILATE_SLOW_TAU", "OBS_PUPIL_SLOW_SHARE",
        "OBS_PIGMENT_KM", "OBS_CONE_REGEN", "OBS_CONE_Q", "OBS_ROD_REGEN", "OBS_ROD_Q",
        "OBS_CONE_BLEACH_LOG", "OBS_ROD_BLEACH_LOG", "OBS_CONE_TAU_UP", "OBS_CONE_TAU_DOWN",
        "OBS_ROD_TAU_UP", "OBS_ROD_TAU_DOWN", "OBS_DISPLAY_ADAPTATION", "OBS_DISPLAY_KEY",
        "OBS_ROD_SATURATION_TD", "OBS_FLASH_ABOVE_ADAPTATION", "OBS_FLASH_MEMORY_TAU",
        "OBS_FLASH_MIN_RECOVERY", "OBS_FLASH_MIN_COVERAGE", "OBS_FLASH_ONSET_TAU",
        "OBS_FLASH_ONSET_RATIO", "OBS_MAX_STEP", "OBS_INSIDE_RESPONSE", "OBS_INSIDE_REFERENCE_TAU",
        "OBS_OPTIC_RESNAP_GAP", "OBS_DAZZLE_FROM_LOG", "OBS_DAZZLE_TAU"];
    let block: String = names.iter().map(|n| format!("const {n}: f32 = 1.0;\n")).collect();
    EXPOSURE_WGSL.replacen("// @observer-constants", &block, 1)
}

#[test]
fn exposure_clock_shader_span_matches_the_32_byte_host_upload() {
    let source = composed();
    let module = wgpu::naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("exposure WGSL parse failed: {error}"));
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    );
    validator
        .validate(&module)
        .unwrap_or_else(|error| panic!("exposure WGSL validation failed: {error}"));

    let clock = module
        .types
        .iter()
        .map(|(_, ty)| ty)
        .find(|ty| ty.name.as_deref() == Some("ExposureClock"))
        .expect("ExposureClock type");
    let span = match clock.inner {
        wgpu::naga::TypeInner::Struct { span, .. } => span,
        ref other => panic!("ExposureClock must remain a struct, got {other:?}"),
    };
    assert_eq!(
        span, 32,
        "shader clock must match the eight-float Rust upload"
    );

    let compact: String = RENDERER_RS.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.contains(
        "constEXPOSURE_CLOCK_UNIFORM_BYTES:u64=std::mem::size_of::<ExposureClockUniform>()asu64;"
    ));
    assert!(compact.contains("const_:[();32]=[();EXPOSURE_CLOCK_UNIFORM_BYTESasusize];"));
    assert!(
        compact.contains("min_binding_size:wgpu::BufferSize::new(EXPOSURE_CLOCK_UNIFORM_BYTES)")
    );
}

#[test]
fn the_eye_meters_physical_luminance_without_a_campaign_lift() {
    // Photometric and scotopic weights, a range reaching the moonless night, no campaign keys.
    for required in [
        "dot(c, vec3(OBS_PHOTOPIC_R, OBS_PHOTOPIC_G, OBS_PHOTOPIC_B))",
        "dot(c, vec3(OBS_SCOTOPIC_R, OBS_SCOTOPIC_G, OBS_SCOTOPIC_B))",
        "clamp(value, OBS_METER_MIN, OBS_METER_MAX)",
        "if (!(value >= 0.0 && value <= 1e30)) { return OBS_METER_MIN; }",
    ] {
        assert!(EXPOSURE_WGSL.contains(required), "missing {required}");
    }
    for forbidden in ["overcast_exposure", "winter_exposure", "campaign_exposure", "METER_MIDDLE_GREY",
        "0.2126, 0.7152, 0.0722"] {
        assert!(!EXPOSURE_WGSL.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn the_eye_is_view_local_and_source_admission_adds_no_radiance_resource() {
    let module = wgpu::naga::front::wgsl::parse_str(&composed()).unwrap();
    let bound: Vec<_> = module.global_variables.iter().map(|(_, v)| v)
        .filter_map(|v| v.binding.as_ref().map(|b| (b.group, b.binding, v.name.clone().unwrap())))
        .collect();
    assert_eq!(bound, vec![(0, 0, "hdr".to_string()), (0, 1, "state".to_string()),
        (0, 2, "clock".to_string())]);
    assert!(RENDERER_RS.contains("cpass.set_bind_group(0, &self.exp_binds[self.active_view], &[]);"));
    assert!(RENDERER_RS.contains("monocular: if (self.optic_mask - 1.0).abs() < 0.5 { 1.0 } else { 0.0 },"));
}
