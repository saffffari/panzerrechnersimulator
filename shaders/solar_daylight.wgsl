// Solar-elevation daylight law shared by every stylized environment emitter: sky dome, diffuse
// sky/ground fill, world-fog in-scatter and the direct beam. Each factor is 1 at the Act I
// reference sun (31 deg elevation), so the balance authored there is unchanged; a lower sun dims
// the environment and reddens the beam, as it physically does. Night and twilight palettes keep
// their own authored radiance; callers apply these factors to the daylight terms only.
const REFERENCE_SUN_SIN: f32 = 0.51503807; // sin(31 deg)

// Horizontal diffuse illuminance under an overcast sky (CIE overcast sky, Krochmann fit):
// E ~= 0.3 + 21 sin(h) klx, relative to the reference elevation.
fn diffuse_daylight(sun_sin: f32) -> f32 {
    return (0.3 + 21.0 * max(sun_sin, 0.0)) / (0.3 + 21.0 * REFERENCE_SUN_SIN);
}

// Kasten-Young relative optical air mass, valid from the horizon to the zenith.
fn relative_air_mass(sun_sin: f32) -> f32 {
    let s = clamp(sun_sin, 0.0, 1.0);
    return 1.0 / (s + 0.50572 * pow(degrees(asin(s)) + 6.07995, -1.6364));
}

// Clear-path beam transmittance per channel in Meinel form, T^(AM^0.678). Shorter wavelengths
// are scattered out of long low-sun paths first. Relative to the reference elevation.
fn beam_daylight(sun_sin: f32) -> vec3<f32> {
    let clear = vec3(0.78, 0.70, 0.58);
    return pow(clear, vec3(pow(relative_air_mass(sun_sin), 0.678)))
         / pow(clear, vec3(pow(relative_air_mass(REFERENCE_SUN_SIN), 0.678)));
}

// Direct beam through the cloud deck. The authored transmittance holds at the reference sun; a
// lower sun crosses proportionally more of the same layer (Beer-Lambert along the slant path), so
// the transmittance is raised to the ratio of relative air masses.
fn cloud_beam(overcast: f32, storm: f32, sun_sin: f32) -> f32 {
    let reference = mix(1.0, 0.42, clamp(overcast, 0.0, 1.0)) * mix(1.0, 0.70, clamp(storm, 0.0, 1.0));
    return pow(reference, relative_air_mass(sun_sin) / relative_air_mass(REFERENCE_SUN_SIN));
}

// Radiance of the sky at the horizon: the limit an optically thick near-ground medium converges
// to, so fog and the sky it meets are one light source. Twilight's sun-side glow is directional
// and stays in the sky shader.
fn horizon_sky_radiance(overcast: f32, storm: f32, night: f32, sun_sin: f32) -> vec3<f32> {
    let darkness = clamp(night / 0.8, 0.0, 1.0);
    let day = mix(vec3(0.38, 0.48, 0.53), vec3(0.28, 0.33, 0.35), clamp(overcast, 0.0, 1.0))
        * mix(1.0, 0.58, clamp(storm, 0.0, 1.0));
    return mix(day * diffuse_daylight(sun_sin), day * vec3(0.11, 0.16, 0.27), darkness);
}
