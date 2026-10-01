//! One evolving storm state drives cloud water coverage, wind, snowfall and ground extinction.
#[derive(Clone, Copy, Debug)]
pub struct Weather {
    pub snow: f32,
    pub extinction: f32,
    pub cloud: f32,
    pub storm: f32,
    pub wind: f32,
}
pub fn sample(intensity: f32, time: f32) -> Weather {
    let strength = intensity.clamp(0.0, 8.0);
    // Slow fronts plus shorter gusts; continuous and deterministic across every view.
    let pulse = 0.84 + 0.10 * (time * 0.029 + 0.8).sin() + 0.06 * (time * 0.083 + 2.1).sin();
    let snow = strength * pulse;
    Weather {
        snow,
        extinction: 0.0006 + 0.015 * snow,
        cloud: (0.72 * strength.sqrt()).clamp(0.0, 1.0),
        storm: strength.min(1.0),
        wind: 0.7 + 4.5 * snow.sqrt(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn severe_storm_couples_snow_wind_and_visibility_without_jumps() {
        let clear = sample(0.0, 100.0);
        let severe = sample(8.0, 100.0);
        assert_eq!(clear.snow, 0.0);
        assert!(severe.snow > 5.0 && severe.extinction > 0.075 && severe.wind > 10.0);
        assert_eq!(severe.cloud, 1.0);
        for i in 0..1000 {
            let a = sample(8.0, i as f32 * 0.1);
            let b = sample(8.0, i as f32 * 0.1 + 0.1);
            assert!((a.snow - b.snow).abs() < 0.01);
            assert!(a.extinction.is_finite());
        }
    }
}
