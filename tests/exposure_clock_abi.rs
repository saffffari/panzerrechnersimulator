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
