// Compile and exercise the shared CPU reference and exact GPU ABI independently from the live
// renderer integration so neither can silently drift.
#[allow(dead_code)]
#[path = "../src/physical_environment.rs"]
mod physical_environment;

#[path = "../src/protective_glass.rs"]
mod protective_glass;

use glam::DVec3;
use physical_environment::{
    altitude_m, bounded_medium_vertical_optical_depth, bounded_source_flux_closure,
    delta_eddington_diffuse_flux, delta_eddington_flux, diffuse_uncollided_flux_transmittance,
    integrate_environment, AtmosphereParameters, IntegrationQuality, SnowFieldParameters,
};

#[test]
fn gpu_transport_library_parses_and_validates_as_wgsl() {
    let source = include_str!("../shaders/physical_environment.wgsl");
    let module = wgpu::naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("physical environment WGSL parse failed: {error}"));
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    );
    validator
        .validate(&module)
        .unwrap_or_else(|error| panic!("physical environment WGSL validation failed: {error}"));
}

#[test]
fn live_overcast_uses_physical_two_stream_cloud_transport_without_a_color_floor() {
    let physical = include_str!("../shaders/physical_environment.wgsl");
    let renderer = include_str!("../src/renderer.rs");
    for required in [
        "fn physical_delta_eddington_flux(",
        "fn physical_delta_eddington_diffuse_flux(",
        "fn physical_upper_atmosphere_diffuse_flux(",
        "let cloud_boundary = physical_cloud_diffuse_boundary(",
        "cloud_boundary.handled_cloud_scattering != 0u",
    ] {
        assert!(
            physical.contains(required),
            "missing cloud transport contract {required:?}"
        );
    }
    assert!(renderer
        .contains("physical_environment.cloud.coverage = self.overcast.clamp(0.0, 1.0) as f64"));
    for forbidden in ["cloud_brightness_floor", "overcast_tint", "flat_white_sky"] {
        assert!(!physical.contains(forbidden));
    }
}

#[test]
fn live_environment_shader_compositions_parse_and_validate() {
    let library = include_str!("../shaders/physical_environment.wgsl");
    let smoke = include_str!("../shaders/smoke.wgsl");
    let high_smoke = smoke
        .replacen(
            "// @smoke-ray-query-tlas",
            "@group(1) @binding(0) var smoke_scene_tlas: acceleration_structure;",
            1,
        )
        .replacen(
            "// @smoke-non-rt-no-direct-transport\n    return 0.0;",
            r#"let ray_min = 0.01;
    if (max_distance <= ray_min * 2.0) { return 1.0; }
    let ray_max = max(max_distance - ray_min, ray_min);
    var rq: ray_query;
    rayQueryInitialize(&rq, smoke_scene_tlas, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE,
        0xFFu, ray_min, ray_max, origin, normalize(direction)));
    while (rayQueryProceed(&rq)) { }
    return select(1.0, 0.0,
        rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE);"#,
            1,
        );
    for (label, entry) in [
        ("visible sky", include_str!("../shaders/sky.wgsl")),
        ("High surface", include_str!("../shaders/combine.wgsl")),
        ("portable volume", smoke),
        ("High volume", high_smoke.as_str()),
    ] {
        let entry = protective_glass::inject(entry, false).replace(
            "// @campaign-terrain-material", include_str!("../shaders/terrain_material.wgsl"));
        let source = format!("{library}\n{entry}");
        assert_eq!(
            source.matches("fn physical_environment_transport(").count(),
            1
        );
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("{label} WGSL parse failed: {error}"));
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .unwrap_or_else(|error| panic!("{label} WGSL validation failed: {error}"));
    }
}

#[test]
fn optical_quadrature_composes_for_full_and_gameplay_without_pixel_noise_or_energy_gain() {
    let entry = include_str!("../shaders/combine.wgsl");
    let terminal = entry.split("fn shade_secondary(").nth(1).unwrap()
        .split("fn non_glass_incident(").next().unwrap();
    let quadrature = terminal.split("let pair_index =").nth(1).unwrap()
        .split("let sky_sample =").next().unwrap();
    // A common sample pattern produces coherent material/visibility changes. Seeding each
    // screen pixel or world cell would merely freeze speckles instead of fixing their source.
    for forbidden in ["hash13(", "seed", "px.", "cam._pad2", "hit.position"] {
        assert!(!quadrature.contains(forbidden), "optical quadrature varies with {forbidden}");
    }
    assert!(terminal.contains("sun_dir = normalize(environment.sun_direction_water_density.xyz)"));
    assert!(terminal.contains("let sky_samples = select(1, max(i32(cam.transport_passes.z), 1), stable_optical)"));
    assert!(terminal.contains("/ f32(sky_samples)"), "four samples must be averaged, not added as four sources");
    assert!(entry.contains("let stable_optical = cam.transport_passes.z > 1.5"));
    assert!(entry.contains("shade_secondary(hit, direction, seed, stable_optical)"));
    assert!(entry.contains("shade_secondary(hit, direction, seed, false)"));
    assert!(entry.contains("let medium_seed = select(seed, vec3(0.5), stable_optical)"));
    assert!(entry.contains("max_distance, background, medium_seed)"));

    // Validate the real composed surface shader plus both profile branches. The profile copies
    // substitute only the new lane; the live source retains its uniform-bounded loop for Vulkan.
    let entry = protective_glass::inject(entry, false).replace(
        "// @campaign-terrain-material", include_str!("../shaders/terrain_material.wgsl"));
    for (label, entry) in [
        ("runtime", entry.clone()),
        ("full", entry.replace("cam.transport_passes.z", "1.0")),
        ("gameplay optics", entry.replace("cam.transport_passes.z", "4.0")),
    ] {
        let source = format!("{}\n{entry}", include_str!("../shaders/physical_environment.wgsl"));
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("{label} optical composition failed: {error}"));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        ).validate(&module)
            .unwrap_or_else(|error| panic!("{label} optical validation failed: {error}"));
    }
}

#[test]
fn resolved_snow_portable_and_high_compositions_parse_and_validate() {
    let library = include_str!("../shaders/physical_environment.wgsl");
    let entry = include_str!("../shaders/snow.wgsl");
    let portable = format!("{library}\n{entry}");
    let high = portable
        .replacen(
            "// @physical-snow-ray-query-tlas",
            "@group(1) @binding(0) var physical_snow_scene_tlas: acceleration_structure;",
            1,
        )
        .replacen(
            "// @physical-snow-no-geometry-visibility\n    return 1.0;",
            r#"let ray_min = 0.002;
    if (max_distance <= ray_min * 2.0) { return 1.0; }
    var rq: ray_query;
    rayQueryInitialize(&rq, physical_snow_scene_tlas, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE,
        0xFFu, ray_min, max(max_distance - ray_min, ray_min), origin, normalize(direction)));
    while (rayQueryProceed(&rq)) { }
    return select(1.0, 0.0,
        rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE);"#,
            1,
        );
    for (label, source) in [("portable snow", portable), ("High snow", high)] {
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("{label} WGSL parse failed: {error}"));
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .unwrap_or_else(|error| panic!("{label} WGSL validation failed: {error}"));
    }
}

#[test]
fn volume_transport_uses_temporal_scrambling_and_phase_distributed_environment() {
    let smoke = include_str!("../shaders/smoke.wgsl");
    for required in [
        "fn temporal_sample(pixel: vec2<i32>, frame: f32, lane: u32)",
        "pcg_hash((u32(frame) & 0xffffu) ^ lane)",
        "struct EnvironmentScatterPair",
        "sample_henyey_greenstein_direction(view_ray, 1.0 - unit_sample, phase_g)",
        "out.weighted_radiance_a = incident_a",
        "out.weighted_radiance_b = incident_b",
        "fn environment_scatter_pair(",
        "if ((quadrature_lane & 1) != 0)",
        "p, rd, source_sample, world_environment, i, world_phase_g",
        "p, rd, source_sample, interior_environment, i, AEROSOL_PHASE_G",
        "p, rd, source_sample, plume_environment, i, AEROSOL_PHASE_G",
        "world_snow_fraction: f32",
        "const BLOWING_SNOW_PHASE_G: f32 = 0.76",
        "const BLOWING_SNOW_SINGLE_SCATTER_ALBEDO: f32 = 0.995",
        "world_single_scatter_albedo * incident",
    ] {
        assert!(
            smoke.contains(required),
            "missing volume noise contract {required:?}"
        );
    }
    assert!(!smoke.contains("struct EnvironmentScatterSample"));
    assert!(!smoke.contains("fn environment_scatter_sample("));
    assert!(!smoke.contains("hash13(vec3(in.pos.xy, sm.frame"));
    assert!(!smoke.contains("snow_tint"));
    assert!(!smoke.contains("whiteout_color"));
    assert!(!smoke.contains("ambient_floor"));

    // Resolved motes are now explicit one-pixel world carriers. Continuous volume transport
    // retains its own temporal sampling; it must not also inject a discrete density field.
    let particles = include_str!("../shaders/pixel_particles.wgsl");
    let field = include_str!("../src/pixel_particles.rs");
    assert!(!smoke.contains("fn interior_mote("));
    assert!(field.contains("fn dust_position(seed: u64, id: usize, time_s: f32)"));
    assert!(field.contains("fn snow_identity(seed: u64, cell: [i64; 3], sample: u32)"));
    assert!(field.contains("Some(previous) if time == previous => return"));
    assert!(particles.contains("u.view_proj * vec4(world - u.camera.xyz, 1.0)"));
    assert!(particles.contains("blocked(in.world,upstream,distance_m)"));

}

#[test]
fn gpu_library_contains_no_authored_sky_or_camera_shell_shortcuts() {
    let source = include_str!("../shaders/physical_environment.wgsl");
    for forbidden in [
        "sun_glow",
        "mood_color",
        "whiteout_veil",
        "ground_spindrift",
        "flake_layer",
        "camera_amount",
        "screen-space",
    ] {
        assert!(
            !source.contains(forbidden),
            "physical environment reintroduced forbidden shortcut `{forbidden}`"
        );
    }
    let generator_start = source.find("fn physical_snow_particle(").unwrap();
    let generator_end = source[generator_start..]
        .find("fn physical_snow_particle_interaction(")
        .map(|offset| generator_start + offset)
        .unwrap();
    let generator = &source[generator_start..generator_end];
    assert!(!generator.contains("camera"));
    assert!(!generator.contains("ray."));
}

#[test]
fn act_i_high_surfaces_and_sky_share_the_bounded_snow_transport() {
    let combine = include_str!("../shaders/combine.wgsl");
    let physical = include_str!("../shaders/physical_environment.wgsl");

    // Both primary and exact-secondary surface bounces retain the finite solar disk as the
    // uncollided directional term from the bounded slab solve.
    assert!(
        combine
            .matches("let solar_source = world_medium_solar_source(solar_path.origin, sun_dir);")
            .count()
            >= 2,
        "primary and secondary sunlight must use the bounded source closure"
    );

    // Environment samples carry exact directional Beer transmission plus only the scattered
    // delta-Eddington remainder. The solar remainder is injected into this diffuse estimator, not
    // the explicit disk NEE, keeping the source terms disjoint.
    for required in [
        "fn world_medium_vertical_optical_depth(",
        "fn world_medium_solar_source(",
        "fn world_medium_environment_source(",
        "solar_source.direct_irradiance",
        "solar_source.diffuse_radiance",
        "directional_transmittance + diffuse_transmittance.y",
        "* world_medium_source_transmittance(ro, light_dir, distance_to_light)",
        "* world_medium_source_transmittance(ro, ldir, ld)",
        "* world_medium_source_transmittance(origin, direction, hit.ray_t)",
    ] {
        assert!(
            combine.contains(required),
            "missing source-side snow transport contract {required:?}"
        );
    }
    for required in [
        "fn physical_diffuse_uncollided_flux_transmittance(",
        "fn physical_bounded_direct_source_transmittance(",
        "fn physical_bounded_diffuse_scattered_transmittance(",
        "max(diffuse.transmittance - uncollided, 0.0)",
    ] {
        assert!(
            physical.contains(required),
            "missing shared bounded-source flux contract {required:?}"
        );
    }
    let closure_start = physical
        .find("fn physical_diffuse_uncollided_flux_transmittance(")
        .unwrap();
    let closure_end = physical[closure_start..]
        .find("fn physical_ozone_column_density_m(")
        .map(|offset| closure_start + offset)
        .unwrap();
    let closure_source = &physical[closure_start..closure_end];
    assert!(!closure_source.contains("for ("));
    assert!(!closure_source.contains("while ("));

    // The constant-cost endpoint closure must not enter the existing runtime world-medium march;
    // that would nest it with the atmosphere/source quadratures and multiply Vulkan compile cost.
    let march_start = combine.find("fn world_medium_radiance(").unwrap();
    let march_end = combine[march_start..]
        .find("fn shade_secondary(")
        .map(|offset| march_start + offset)
        .unwrap();
    let march = &combine[march_start..march_end];
    assert!(!march.contains("physical_bounded_direct_source_transmittance("));
    assert!(!march.contains("physical_bounded_diffuse_scattered_transmittance("));
    for removed_uncollided_only_path in [
        "sky_irr = environment_incident(ro, hdir)",
        "return environment_incident(origin, direction)\n            * world_medium_source_transmittance(origin, direction, 4000.0);",
    ] {
        assert!(!combine.contains(removed_uncollided_only_path));
    }
    for forbidden in ["flat_white_sky", "ambient_floor", "snow_sky_tint"] {
        assert!(
            !combine.contains(forbidden),
            "forbidden daytime-whiteout shortcut {forbidden:?}"
        );
    }

    // Compiler shape is part of the Vulkan release contract. The actual receiver must feed one
    // inlined atmosphere evaluation, including when the independently bounded mist overlaps cloud.
    let environment_start = combine
        .find("fn world_medium_environment_source(")
        .expect("bounded environment source");
    let environment_end = combine[environment_start..]
        .find("fn world_medium_radiance(")
        .map(|offset| environment_start + offset)
        .expect("world-medium march after source closure");
    let environment_source = &combine[environment_start..environment_end];
    assert_eq!(
        environment_source.matches("environment_incident(").count(),
        1
    );
    assert!(environment_source.contains("environment_incident(origin, direction)"));
    assert!(!combine.contains("world_medium_upper_boundary_position"));

    // Likewise, glass miss/hit branches build terminal state before entering one medium march.
    let non_glass_start = combine
        .find("fn non_glass_incident(")
        .expect("non-glass continuation");
    let non_glass_end = combine[non_glass_start..]
        .find("fn glass_incident(")
        .map(|offset| non_glass_start + offset)
        .expect("glass transport after non-glass continuation");
    assert_eq!(
        combine[non_glass_start..non_glass_end]
            .matches("world_medium_radiance(")
            .count(),
        1
    );
}

fn act_two_low_cloud() -> AtmosphereParameters {
    let mut atmosphere = AtmosphereParameters::default();
    atmosphere.sun_direction = DVec3::new(-0.58, 0.40, 0.30).normalize();
    atmosphere.mie_scale_height_m = 650.0;
    atmosphere.mie_scattering_m_inv = 2.8e-6;
    atmosphere.mie_extinction_m_inv = 3.4e-6;
    atmosphere.mie_asymmetry = 0.78;
    atmosphere.cloud.bottom_altitude_m = 430.0;
    atmosphere.cloud.top_altitude_m = 1250.0;
    atmosphere.cloud.liquid_water_content_kg_m3 = 0.18e-3;
    atmosphere.cloud.droplet_effective_radius_m = 11.0e-6;
    atmosphere.cloud.coverage = 0.91;
    atmosphere
}

#[test]
fn act_two_suspension_top_is_inside_cloud_but_the_real_receiver_is_below_it() {
    let atmosphere = act_two_low_cloud();
    // Recorded grove starting camera; the surface's centimetre-scale ray guard remains below
    // cloud as well. The old projected source origin was 253.5 metres above this receiver.
    let receiver = DVec3::new(200.0, 2.5024347, 150.0);
    let projected = DVec3::new(receiver.x, 256.0, receiver.z);
    assert!(altitude_m(&atmosphere, receiver) < atmosphere.cloud.bottom_altitude_m);
    assert!(altitude_m(&atmosphere, projected) > atmosphere.cloud.bottom_altitude_m);
    assert!(altitude_m(&atmosphere, projected) < atmosphere.cloud.top_altitude_m);
    let incident = |position| integrate_environment(&atmosphere, position, DVec3::Y,
        f64::INFINITY, IntegrationQuality::REFERENCE).inscattered_radiance;
    let actual = incident(receiver);
    let misplaced = incident(projected);
    assert!(actual.is_finite() && misplaced.is_finite());
    assert!(actual.min_element() > 0.0);
    assert!(actual.length() > misplaced.length(),
        "Act-II regression must expose the lost lower-cloud flux: receiver={actual:?}, projected={misplaced:?}");
}

#[test]
fn receiver_environment_queries_keep_below_inside_and_above_cloud_domains() {
    let atmosphere = act_two_low_cloud();
    for (world_y, expected_domain) in [(2.5, -1), (256.0, 0), (1200.0, 1)] {
        let receiver = DVec3::new(200.0, world_y, 150.0);
        let altitude = altitude_m(&atmosphere, receiver);
        let domain = if altitude < atmosphere.cloud.bottom_altitude_m {-1}
            else if altitude < atmosphere.cloud.top_altitude_m {0} else {1};
        assert_eq!(domain, expected_domain);
        let incident = integrate_environment(&atmosphere, receiver, DVec3::Y,
            f64::INFINITY, IntegrationQuality::REFERENCE).inscattered_radiance;
        assert!(incident.is_finite() && incident.min_element() >= 0.0);
        // With no independent suspension, camera and secondary-source environment are exactly
        // the same query in all three domains. No fictitious lower-cloud term is injected inside
        // or above cloud; those origins retain the existing physical environment solver.
        let vacuum = bounded_source_flux_closure(0.0, 0.82, 0.70, 0.5);
        assert_eq!(incident * (1.0 + vacuum.diffuse_scattered_transmittance), incident);
    }
}

#[test]
fn independent_suspension_transfer_is_continuous_across_cloud_base_overlap() {
    let atmosphere = act_two_low_cloud();
    let receiver = DVec3::new(200.0, 2.5, 150.0);
    let incident = integrate_environment(&atmosphere, receiver, DVec3::Y,
        f64::INFINITY, IntegrationQuality::REFERENCE).inscattered_radiance;
    let source = |top_y: f64, omega: f64| {
        let tau = bounded_medium_vertical_optical_depth(0.0026, receiver.y, -128.0, top_y);
        let closure = bounded_source_flux_closure(tau, omega, 0.70, 0.5);
        incident * ((-tau).exp() + closure.diffuse_scattered_transmittance)
    };
    // The old projected query switched cloud-solver branches at this crossing. The independent
    // suspension's support can now cross the same height with only continuous optical-depth change.
    let cloud_base_world_y = atmosphere.planet_center_m.y + atmosphere.planet_radius_m
        + atmosphere.cloud.bottom_altitude_m;
    let before = source(cloud_base_world_y - 0.001, 0.82);
    let after = source(cloud_base_world_y + 0.001, 0.82);
    assert!((after - before).length() / before.length() < 1e-4);
    let tau = bounded_medium_vertical_optical_depth(0.0026, receiver.y, -128.0, 256.0);
    assert!((source(256.0, 0.0) - incident * (-tau).exp()).length() < 1e-14,
        "an absorbing-only suspension applies Beer extinction exactly once");
}

#[test]
fn bounded_snow_source_closure_is_energy_separated_and_matches_act_i_column() {
    let optical_depth = bounded_medium_vertical_optical_depth(0.0068, -10.3, -128.0, 128.0);
    assert!((optical_depth - 0.94044).abs() < 1.0e-12);

    let uncollided = diffuse_uncollided_flux_transmittance(optical_depth);
    assert!(
        (uncollided - 0.237_631_234_3).abs() < 1.0e-10,
        "Act-I uncollided diffuse flux was {uncollided}"
    );
    let direct = delta_eddington_flux(optical_depth, 0.995, 0.76, 0.24);
    let diffuse = delta_eddington_diffuse_flux(optical_depth, 0.995, 0.76);
    let closure = bounded_source_flux_closure(optical_depth, 0.995, 0.76, 0.24);

    assert!((closure.direct_transmittance - direct.direct_transmittance).abs() < 1.0e-12);
    assert!(
        (closure.direct_transmittance + closure.direct_scattered_transmittance
            - direct.transmittance)
            .abs()
            < 1.0e-12
    );
    assert!((closure.diffuse_uncollided_transmittance - uncollided).abs() < 1.0e-12);
    assert!(
        (closure.diffuse_uncollided_transmittance + closure.diffuse_scattered_transmittance
            - diffuse.transmittance)
            .abs()
            < 1.0e-12
    );
    assert!((0.84..0.86).contains(&diffuse.transmittance));
    assert!((0.60..0.63).contains(&closure.diffuse_scattered_transmittance));

    // A neutral scalar closure preserves the incident source spectrum exactly.
    let incident = DVec3::new(0.02942, 0.03421, 0.03403);
    let transported = incident * closure.diffuse_scattered_transmittance;
    for channel in 0..3 {
        assert!(
            (transported[channel] / incident[channel] - closure.diffuse_scattered_transmittance)
                .abs()
                < 1.0e-12
        );
    }
}

#[test]
fn bounded_snow_source_closure_has_physical_vacuum_absorber_and_slab_limits() {
    let vacuum = bounded_source_flux_closure(0.0, 0.995, 0.76, 0.24);
    assert_eq!(vacuum.direct_transmittance, 1.0);
    assert_eq!(vacuum.direct_scattered_transmittance, 0.0);
    assert_eq!(vacuum.diffuse_uncollided_transmittance, 1.0);
    assert_eq!(vacuum.diffuse_scattered_transmittance, 0.0);

    let absorber = bounded_source_flux_closure(0.25, 0.0, 0.76, 0.24);
    assert_eq!(absorber.direct_scattered_transmittance, 0.0);
    assert_eq!(absorber.diffuse_scattered_transmittance, 0.0);

    assert_eq!(
        bounded_medium_vertical_optical_depth(0.0068, 256.0, -128.0, 128.0),
        0.0
    );
    assert_eq!(
        bounded_medium_vertical_optical_depth(0.0068, -256.0, -128.0, 128.0),
        0.0068 * 256.0
    );
    assert_eq!(
        bounded_medium_vertical_optical_depth(0.0068, 0.0, 128.0, -128.0),
        0.0
    );
}

#[test]
fn diffuse_estimators_use_lambertian_pi_and_the_actual_directional_pdf() {
    let source = include_str!("../shaders/combine.wgsl");
    for required in [
        "(kd * albedo / PI + spec) * solar_irradiance * NdotL",
        "(kd * albedo / PI + spec) * source * lNdotL",
        "let direct_diffuse_brdf = kd * albedo / PI;",
        "lkd * albedo * li / PI",
    ] {
        assert!(
            source.contains(required),
            "missing Lambertian direct-light normalization {required:?}"
        );
    }
    // The cosine baseline cancels Lambertian 1/pi. Near real panes its area/cosine mixture
    // instead retains cos(theta)/(pi*pdf_mixture), so the change affects variance, not energy.
    assert!(source.contains("var diff_col = direct_diff;"));
    assert!(!source.contains("var sky_irr"));
    assert!(source.contains("let diffuse_pdf_weight = max(dot(n, diffuse_dir), 0.0)"));
    assert!(source.contains("/ (PI * max(diffuse_sample.pdf, 1e-20))"));
    assert!(source.contains("/ (PI * max(sky_sample.pdf, 1e-20))"));
    let proposal = include_str!("../shaders/daylight_sampling.wgsl");
    assert!(proposal.contains("(1.0 - mixture) * directional_base_pdf"));
    assert!(proposal.contains("+ mixture * daylight_area_pdf(p, out.direction)"));
    // The mixture/VNDF pair still uses one runtime-bounded secondary call.
    assert!(source.contains(
        "var bounce_weight = array<vec3<f32>, 2>(diffuse_kd * albedo * diffuse_pdf_weight, reflection_weight);"
    ));
    assert!(source.contains(
        "let li = secondary_incident(bounce_origin, bounce_direction[slot], bounce_seed[slot]);"
    ));
    assert!(source.contains("diff_col += weight * li;"));
}

#[test]
fn one_pixel_snow_is_world_positioned_roof_occluded_and_keeps_bulk_weather() {
    let snow = include_str!("../shaders/pixel_particles.wgsl");
    let physical = include_str!("../shaders/physical_environment.wgsl");
    for forbidden in ["whiteout_veil", "veil_alpha", "ground_spindrift", "flake_layer", "layer_dist"] {
        assert!(!snow.contains(forbidden), "snow retained forbidden proxy {forbidden:?}");
    }
    for required in ["u.view_proj * vec4(world - u.camera.xyz, 1.0)", "floor(vec2(ndc.x",
        "textureLoad(depth_tex", "blocked(in.world,upstream,distance_m)",
        "return vec4(vec3(optic),1.0)"] {
        assert!(snow.contains(required), "missing one-pixel 3D snow constraint {required:?}");
    }
    for required in [
        "world_medium: vec4<f32>",
        "fn physical_snow_bulk_transmittance(",
        "let solar_bulk_transmittance = physical_snow_bulk_transmittance(",
        "let environment_bulk_transmittance = physical_snow_bulk_transmittance(",
        "let view_bulk_transmittance = physical_snow_bulk_transmittance(",
    ] {
        assert!(
            physical.contains(required),
            "resolved snow bypassed shared bulk transport {required:?}"
        );
    }
}

#[test]
fn public_reference_entry_points_form_one_shared_environment_interface() {
    let atmosphere = AtmosphereParameters::default();
    let camera_miss = integrate_environment(
        &atmosphere,
        DVec3::new(0.0, 1.7, 0.0),
        DVec3::Y,
        f64::INFINITY,
        IntegrationQuality::INTERACTIVE_HIGH,
    );
    let secondary_surface_ray = integrate_environment(
        &atmosphere,
        DVec3::new(12.0, 3.0, -4.0),
        DVec3::new(0.3, 0.8, 0.2),
        f64::INFINITY,
        IntegrationQuality::INTERACTIVE_HIGH,
    );
    let finite_volume_source_ray = integrate_environment(
        &atmosphere,
        DVec3::new(-2.0, 2.0, 8.0),
        DVec3::new(-0.2, 0.6, 0.7),
        1_500.0,
        IntegrationQuality::INTERACTIVE_HIGH,
    );
    for result in [camera_miss, secondary_surface_ray, finite_volume_source_ray] {
        assert!(result.radiance().is_finite());
        assert!(result.transmittance.is_finite());
    }

    let snow = SnowFieldParameters::default();
    assert!(snow.number_density_m3().is_finite());
    assert!(snow.bulk_extinction_m_inv() > 0.0);
}

#[test]
fn campaign_valleys_remain_above_ground_and_clear_external_sky_is_not_black() {
    let atmosphere = AtmosphereParameters::default();
    let valley_camera = DVec3::new(0.0, -43.74 + 1.7, 0.0);
    assert!(altitude_m(&atmosphere, valley_camera) > 200.0);
    let sky = integrate_environment(
        &atmosphere,
        valley_camera,
        DVec3::Y,
        f64::INFINITY,
        IntegrationQuality::INTERACTIVE_HIGH,
    );
    assert!(sky.inscattered_radiance.min_element() > 0.0);
}

#[test]
fn altitude_partition_cannot_step_over_the_liquid_water_layer() {
    let mut atmosphere = AtmosphereParameters::default();
    atmosphere.cloud.coverage = 1.0;
    let transport = integrate_environment(
        &atmosphere,
        DVec3::new(0.0, 2.0, 0.0),
        DVec3::Y,
        f64::INFINITY,
        IntegrationQuality {
            view_steps: 1,
            source_steps: 1,
            solar_disk_samples: 1,
            cloud_column_steps: 1,
        },
    );
    assert!(
        transport.transmittance.max_element() < 0.5,
        "coarse full-height integration skipped the declared cloud layer: {:?}",
        transport.transmittance,
    );
}
