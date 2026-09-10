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
    assert!(branch.contains("m = display_decode(display_grade);"));
    assert!(shader.contains("var m = aces(c);"));
    assert!(!shader.contains("m = floor(m * cr.levels"));
}
