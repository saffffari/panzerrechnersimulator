const SMOKE: &str = include_str!("../shaders/smoke.wgsl");

fn function_body(name: &str) -> &'static str {
    let start = SMOKE
        .find(name)
        .unwrap_or_else(|| panic!("missing shader function {name}"));
    let open = SMOKE[start..].find('{').unwrap() + start;
    let bytes = SMOKE.as_bytes();
    let mut depth = 0_i32;
    for index in open..bytes.len() {
        match bytes[index] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &SMOKE[open..=index];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated shader function {name}");
}

#[test]
fn bounded_snow_path_is_orders_two_plus_with_hg_nee_and_rr() {
    let path = function_body("fn bounded_world_higher_order_scatter(");
    for required in [
        "first_free_flight",
        "sample_henyey_greenstein_direction(",
        "world_medium_interval(",
        "geometry_visibility(",
        "continuation_direct_nee(",
        "throughput = throughput * single_scatter_albedo",
        "let survival = clamp(throughput, 0.20, 0.95)",
        "throughput = throughput / survival",
    ] {
        assert!(
            path.contains(required),
            "missing path contract {required:?}"
        );
    }
    assert!(SMOKE.contains("const BOUNDED_VOLUME_CONTINUATION_EVENTS: i32 = 4"));
    assert!(SMOKE.contains("const BOUNDED_VOLUME_PATH_LIMIT_M: f32 = 4000.0"));

    // Order one remains its own deterministic camera march. The continuation estimator is added
    // once after it, rather than called per tap and accidentally counting the direct source again.
    assert_eq!(
        SMOKE.matches("bounded_world_higher_order_scatter(").count(),
        2
    );
    let call = SMOKE
        .rfind("world_rgb = world_rgb + bounded_world_higher_order_scatter(")
        .expect("orders>=2 camera contribution");
    let order_one = SMOKE
        .find("world_single_scatter_albedo * incident")
        .expect("order-one camera quadrature");
    assert!(order_one < call);
}

#[test]
fn continuation_nee_separates_finite_sun_from_diffuse_environment() {
    let environment = function_body("fn continuation_environment_nee(");
    assert!(environment.contains(").inscattered_radiance"));
    assert!(!environment.contains(".direct_solar_radiance"));
    assert!(environment.contains("henyey_greenstein("));
    // The continuation now samples an HG/aperture mixture, not a uniform sphere. Its full
    // directional proposal PDF replaces the old constant reciprocal density 4pi. Using 4pi here
    // would bias the already anisotropic HG sampler and double-weight the forward snow lobe.
    assert!(environment.contains("let base_pdf = henyey_greenstein("));
    assert!(environment.contains("let proposal = daylight_sample("));
    assert!(environment.contains("let direction = proposal.direction;"));
    assert!(environment.contains("henyey_greenstein(dot(direction, view_ray), phase_g) / max(proposal.pdf, 1e-20)"));
    assert!(!environment.contains("* (4.0 * PI)"));

    let direct = function_body("fn continuation_direct_nee(");
    assert_eq!(direct.matches("sun_in_scatter(").count(), 1);
    assert_eq!(direct.matches("continuation_environment_nee(").count(), 1);
    assert_eq!(direct.matches("local_in_scatter(").count(), 1);

    for forbidden in [
        "snow_tint",
        "whiteout_color",
        "ambient_floor",
        "brightness_floor",
        "camera_fill",
    ] {
        assert!(
            !SMOKE.contains(forbidden),
            "forbidden volume shortcut {forbidden}"
        );
    }
}

#[test]
fn world_unscattered_term_is_analytic_beer_lambert() {
    assert!(SMOKE.contains("world_a = 1.0 - exp(-sm.fog_density * (world_iv.y - world_iv.x))"));
    assert!(!SMOKE.contains("world_a = world_a + world_trans * a"));
}

#[test]
fn inverse_hg_sampler_has_declared_mean_cosine() {
    fn sample_cosine(u: f64, g: f64) -> f64 {
        if g.abs() <= 1.0e-3 {
            return 1.0 - 2.0 * u;
        }
        let ratio = (1.0 - g * g) / (1.0 - g + 2.0 * g * u);
        ((1.0 + g * g - ratio * ratio) / (2.0 * g)).clamp(-1.0, 1.0)
    }

    for g in [0.0_f64, 0.42, 0.76] {
        let samples = 200_000_u32;
        let mean = (0..samples)
            .map(|i| sample_cosine((f64::from(i) + 0.5) / f64::from(samples), g))
            .sum::<f64>()
            / f64::from(samples);
        assert!((mean - g).abs() < 2.0e-5, "g={g}, sampled mean={mean}");
    }
}

#[test]
fn camera_free_flight_probability_matches_exact_slab_opacity() {
    let sigma_t = 0.0068_f64;
    let distance_m = 205.0_f64;
    let expected = 1.0 - (-sigma_t * distance_m).exp();
    let samples = 500_000_u32;
    let collisions = (0..samples)
        .filter(|&i| {
            let u = (f64::from(i) + 0.5) / f64::from(samples);
            let free_flight = -(1.0 - u).ln() / sigma_t;
            free_flight < distance_m
        })
        .count();
    let measured = collisions as f64 / f64::from(samples);
    assert!((measured - expected).abs() < 3.0e-6);
}
