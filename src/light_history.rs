//! Eligibility for reacting quickly to a source step. Smooth source motion/flicker follows the
//! ordinary temporal response; this is not a reset and does not reject valid history by itself.
#[derive(Clone, Debug)]
pub struct Light {
    pub position: [f32; 3],
    pub radius: f32,
    pub power: [f32; 3],
}
#[derive(Clone, Debug)]
pub struct Sources {
    pub sun_direction: [f32; 3],
    pub solar_fraction: f32,
    pub weather: [f32; 4], // overcast, extinction, storm, precipitation
    pub act: u8,
    pub lights: Vec<Light>,
}
fn relative_step(a: f32, b: f32, fraction: f32, floor: f32) -> bool {
    (a - b).abs() > floor.max(fraction * a.abs().max(b.abs()))
}
fn switched(a: f32, b: f32) -> bool {
    (a > 1e-8) != (b > 1e-8)
}
impl Sources {
    /// Compare successive rendered inputs for this view. Updating the baseline every frame is
    /// intentional: gradual changes should converge normally, not repeatedly erase rare samples.
    pub fn material_step_from(&self, previous: Option<&Self>) -> bool {
        let Some(old) = previous else {
            return true;
        };
        if self.act != old.act || self.lights.len() != old.lights.len() {
            return true;
        }
        let angle_delta = self
            .sun_direction
            .iter()
            .zip(old.sun_direction)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>();
        if angle_delta > 0.008_726_535_f32.powi(2)
            || switched(self.solar_fraction, old.solar_fraction)
            || relative_step(self.solar_fraction, old.solar_fraction, 0.15, 0.005)
        {
            return true;
        }
        for i in 0..4 {
            if relative_step(
                self.weather[i],
                old.weather[i],
                0.15,
                if i == 1 { 0.0001 } else { 0.03 },
            ) {
                return true;
            }
        }
        self.lights.iter().zip(&old.lights).any(|(a, b)| {
            let ap = a.power.iter().copied().fold(0.0f32, f32::max);
            let bp = b.power.iter().copied().fold(0.0f32, f32::max);
            if switched(ap, bp) {
                return true;
            }
            if ap <= 1e-8 && bp <= 1e-8 {
                return false;
            }
            let displacement = a
                .position
                .iter()
                .zip(b.position)
                .map(|(x, y)| (x - y) * (x - y))
                .sum::<f32>();
            displacement > 0.20 * 0.20
                || relative_step(a.radius, b.radius, 0.20, 0.005)
                || a.power
                    .iter()
                    .zip(b.power)
                    .any(|(x, y)| relative_step(*x, y, 0.20, 1e-6))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> Sources {
        Sources {
            sun_direction: [0.0, 1.0, 0.0],
            solar_fraction: 1.0,
            weather: [0.9, 0.006, 0.8, 0.9],
            act: 1,
            lights: vec![Light {
                position: [0.0, 2.0, 0.0],
                radius: 0.035,
                power: [0.04, 0.02, 0.01],
            }],
        }
    }
    #[test]
    fn static_and_gradual_source_changes_preserve_eligibility() {
        let mut previous = base();
        assert!(!previous.material_step_from(Some(&previous)));
        for frame in 1..200 {
            let mut now = previous.clone();
            now.lights[0].position[0] += 0.003;
            for value in &mut now.lights[0].power {
                *value *= 1.002;
            }
            let angle = frame as f32 * 0.0001;
            now.sun_direction = [angle.sin(), angle.cos(), 0.0];
            assert!(!now.material_step_from(Some(&previous)));
            previous = now;
        }
    }
    #[test]
    fn flash_creation_removal_and_power_steps_are_eligible() {
        let old = base();
        let mut new = old.clone();
        new.lights[0].power = [0.0; 3];
        assert!(new.material_step_from(Some(&old)));
        assert!(old.material_step_from(Some(&new)));
        new = old.clone();
        new.lights[0].power[0] *= 2.0;
        assert!(new.material_step_from(Some(&old)));
        new = old.clone();
        new.lights.clear();
        assert!(new.material_step_from(Some(&old)));
    }
    #[test]
    fn significant_source_or_weather_discontinuities_are_eligible() {
        let old = base();
        let mut new = old.clone();
        new.lights[0].position[0] = 0.3;
        assert!(new.material_step_from(Some(&old)));
        new = old.clone();
        new.weather[0] = 0.0;
        assert!(new.material_step_from(Some(&old)));
        new = old.clone();
        new.sun_direction = [0.02, 0.9998, 0.0];
        assert!(new.material_step_from(Some(&old)));
        new = old.clone();
        new.solar_fraction = 0.0;
        assert!(new.material_step_from(Some(&old)));
    }
}
