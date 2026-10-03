//! Display-code quantization witness. These numerical tests cover the transfer standard and
//! finite quantizer separately from shader parsing; they do not assert gameplay visibility.
fn encode(x: f64) -> f64 {
    if x <= 0.0031308 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}
fn decode(x: f64) -> f64 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
fn quantize(rgb: [f64; 3], ordered: f64) -> [f64; 3] {
    rgb.map(|x| {
        decode(
            ((encode(x) + ordered * 0.57 / 13.0) * 13.0)
                .round()
                .clamp(0.0, 13.0)
                / 13.0,
        )
    })
}
#[test]
fn transfer_roundtrip_is_neutral_monotone_and_preserves_black_and_white() {
    assert_eq!(decode(encode(0.0)), 0.0);
    assert!((decode(encode(1.0)) - 1.0).abs() < 1e-14);
    let mut previous = 0.0;
    for i in 0..=10_000 {
        let linear = i as f64 / 10_000.0;
        let display = encode(linear);
        assert!(display >= previous);
        previous = display;
        assert!((decode(display) - linear).abs() < 3e-8);
        let neutral = quantize([linear; 3], 0.0);
        assert_eq!(neutral[0], neutral[1]);
        assert_eq!(neutral[1], neutral[2]);
    }
}
#[test]
fn dim_blue_transport_survives_display_quantization_without_a_black_floor() {
    let blue = [0.001, 0.005, 0.020];
    assert_eq!(blue.map(|v: f64| (v * 13.0).round() / 13.0), [0.0; 3]);
    let visible = quantize(blue, 0.0);
    assert_eq!(visible[0], 0.0);
    assert!(visible[2] > visible[1] && visible[1] > 0.0);
    // The first nonzero display code requires ~0.0068 linear, versus 0.0769 before the fix.
    assert!((decode(1.0 / 13.0) - 0.006807128513726831).abs() < 1e-7);
    for lane in 0..16 {
        let ordered = (lane as f64 + 0.5) / 16.0 - 0.5;
        assert_eq!(quantize([0.0; 3], ordered), [0.0; 3]);
        assert!((quantize([1.0; 3], ordered)[0] - 1.0).abs() < 1e-14);
    }
}
#[test]
fn quantized_channel_error_is_bounded_in_display_codes_and_keeps_channel_order() {
    for i in 0..=1000 {
        let rgb = [
            i as f64 / 1000.0 * 0.2,
            i as f64 / 1000.0 * 0.5,
            i as f64 / 1000.0,
        ];
        let q = quantize(rgb, 0.0);
        assert!(q[0] <= q[1] && q[1] <= q[2]);
        for c in 0..3 {
            assert!((encode(q[c]) - encode(rgb[c])).abs() <= 0.5 / 13.0 + 1e-12);
        }
    }
    let shader = include_str!("../shaders/crush.wgsl");
    let branch = shader.split("if (retro > 0.001) {").nth(1).unwrap();
    assert!(branch.contains("let continuous_grade = display_encode(m);"));
    assert!(branch.contains("m = mix(m, display_decode(display_grade), retro);"));
    assert!(shader.contains("var m = aces(c);"));
    assert!(!shader.contains("m = floor(m * cr.levels"));
}

#[test]
fn native_ldr_preserves_the_first_dark_palette_code_at_every_threshold() {
    // Complete storage transfer, not just the preceding 31-step palette quantizer.
    // The former linear attachment maps part of this valid display step to zero.
    let first = decode(1.0 / 31.0);
    let mut old_black_pixels = 0;
    for rank in 0..64 {
        let threshold = (rank as f64 + 0.5) / 64.0 - 0.5;
        old_black_pixels += usize::from((first * 255.0 + threshold).round() == 0.0);
        let stored = (encode(first) * 255.0 + threshold).round() / 255.0;
        assert!(stored > 0.0, "valid palette code erased at threshold {rank}");
        assert!((stored - 1.0 / 31.0).abs() <= 1.0 / 255.0);
        // Sampling the sRGB intermediate and writing an sRGB final target is idempotent.
        assert!((encode(decode(stored)) - stored).abs() < 1e-12);
    }
    assert!(old_black_pixels > 0, "fixture must expose the prior storage error");
    assert!(include_str!("../src/renderer.rs").contains(
        "const LDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;"));
}

fn camera_shoulder(x: f64) -> f64 {
    if x <= 0.6 { (x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14) }
    else { let z=(1.0+3.047601074*(x-0.6)).ln();
        0.673290473+0.326709527*z/(z+2.0) }
}
#[test]
fn global_shoulder_keeps_bright_port_detail_and_has_no_knee_discontinuity() {
    let knee=camera_shoulder(0.6);
    assert!((camera_shoulder(0.600001)-knee).abs()<1e-6);
    let before=(knee-camera_shoulder(0.59999))/0.00001;
    let after=(camera_shoulder(0.60001)-knee)/0.00001;
    assert!((before-after).abs()<0.0001);
    let mut previous=0.0;
    for i in 0..1000 {
        let x=10_f64.powf(-6.0+i as f64*0.012);
        let y=camera_shoulder(x);
        assert!(y>previous && y<1.0); previous=y;
    }
    // Previous hard shoulder produced white for all three transported highlight levels.
    let values=[10.0,42.62,200.0].map(camera_shoulder);
    assert!(values[0]<values[1] && values[1]<values[2] && values[2]<1.0);
    assert_eq!(camera_shoulder(0.0),0.0);
}
