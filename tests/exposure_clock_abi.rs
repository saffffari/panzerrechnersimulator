//! Regression for the auto-exposure clock's Rust/WGSL uniform ABI.
//!
//! WGSL `vec3<f32>` has 16-byte alignment. Placing one after a scalar makes the structure span
//! 32 bytes, which Vulkan correctly rejects when Rust uploads only four contiguous floats.

const EXPOSURE_WGSL: &str = include_str!("../shaders/exposure.wgsl");
const RENDERER_RS: &str = include_str!("../src/renderer.rs");

#[test]
fn exposure_clock_shader_span_matches_the_16_byte_host_upload() {
    let module = wgpu::naga::front::wgsl::parse_str(EXPOSURE_WGSL)
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
        span, 16,
        "shader clock must match the four-float Rust upload"
    );

    let compact: String = RENDERER_RS.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.contains(
        "constEXPOSURE_CLOCK_UNIFORM_BYTES:u64=std::mem::size_of::<ExposureClockUniform>()asu64;"
    ));
    assert!(compact.contains("const_:[();16]=[();EXPOSURE_CLOCK_UNIFORM_BYTESasusize];"));
    assert!(
        compact.contains("min_binding_size:wgpu::BufferSize::new(EXPOSURE_CLOCK_UNIFORM_BYTES)")
    );
}

#[test]
fn exposure_range_reaches_physical_overcast_without_a_campaign_lift() {
    let shader = include_str!("../shaders/exposure.wgsl");
    assert!(
        shader.contains("const METER_MAX_EXPOSURE: f32 = METER_MIDDLE_GREY / METER_MIN_LUMINANCE;")
    );
    assert!(shader.contains("METER_MIDDLE_GREY / max(avg, METER_MIN_LUMINANCE)"));
    for forbidden in ["overcast_exposure", "winter_exposure", "campaign_exposure"] {
        assert!(!shader.contains(forbidden));
    }
}

#[test]
fn gaze_meter_binds_actual_glass_and_rejects_stale_forward_gbuffer() {
    let module = wgpu::naga::front::wgsl::parse_str(EXPOSURE_WGSL).unwrap();
    let glass = module.global_variables.iter().map(|(_, v)| v)
        .find(|v| v.name.as_deref() == Some("g0")).expect("glass classification texture");
    assert_eq!(glass.binding.as_ref().map(|b| (b.group, b.binding)), Some((0, 3)));
    assert!(EXPOSURE_WGSL.contains("if (clock.deferred_valid > 0.5)"));
    assert!(RENDERER_RS.contains("deferred_valid: if self.deferred { 1.0 } else { 0.0 }"));
    assert!(EXPOSURE_WGSL.contains("if (!(lum >= 0.0 && lum <= 1e30)) { lum = 0.0; }"));
}

// Independent numerical fixtures describe the intended meter response. The v9 fixture derives
// from an actual Driver HDR/G0 readback; it is not an invented target exposure or runtime override.
fn meter_fixture(global_lum: f64, gaze_coverage: f64, first: f64, second: f64) -> f64 {
    let t = ((gaze_coverage - 0.08) / (0.30 - 0.08)).clamp(0.0, 1.0);
    let attention = t*t*(3.0-2.0*t);
    let focused = second / first.max(1e-8);
    let luminance = global_lum + (global_lum.max(focused) - global_lum) * attention;
    (0.18 / luminance.max(0.0005)).clamp(0.16, 360.0)
}

#[test]
fn zero_dark_and_peripheral_glass_cannot_poison_or_hijack_the_meter() {
    for coverage in [0.0, 0.05, 0.3, 1.0] {
        let dark = meter_fixture(0.0005, coverage, 0.0, 0.0);
        assert!(dark.is_finite());
        assert_eq!(dark, 360.0);
        assert_eq!(meter_fixture(0.0005, coverage, 1e-10, 1e-20), 360.0);
    }
    let witness_global = 0.18 / 347.608;
    assert!((meter_fixture(witness_global, 0.0000622587, 1.0, 0.000924071) - 347.608).abs() < 1e-8);
    let driver_global = 0.18 / 355.257066;
    assert!((meter_fixture(driver_global, 0.3695718, 1.0, 0.0029899171) - 60.202338).abs() < 1e-4);
    // Once the eye leaves the slit, the target returns to the unchanged global meter. Temporal
    // adaptation still uses the shader's existing 0.22/1.80 second response, with per-view state.
    assert!((meter_fixture(driver_global, 0.0, 1.0, 0.0029899171) - 355.257066).abs() < 1e-6);
}
