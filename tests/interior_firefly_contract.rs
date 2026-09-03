//! Source-level regression gate for the interior firefly fixes.
//!
//! The High renderer deliberately keeps its surface and volume estimators unbiased. These checks
//! lock in the variance controls that make that physically transported light usable in the Tiger
//! interior without replacing it with radiance clamps, fake fill, or screen-space cleanup.

const RENDERER: &str = include_str!("../src/renderer.rs");
const COMBINE: &str = include_str!("../shaders/combine.wgsl");
const RESOLVE: &str = include_str!("../shaders/resolve.wgsl");
const SMOKE: &str = include_str!("../shaders/smoke.wgsl");
const VOLUME_ACCUM: &str = include_str!("../shaders/volume_accum.wgsl");

fn without_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

fn function_source<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("missing function signature: {signature}"));
    let open = source[start..]
        .find('{')
        .map(|offset| start + offset)
        .unwrap_or_else(|| panic!("missing function body: {signature}"));
    let mut depth = 0usize;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        match *byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[start..=open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated function body: {signature}");
}

fn assert_in_order(source: &str, needles: &[&str]) {
    let mut cursor = 0usize;
    for needle in needles {
        let relative = source[cursor..]
            .find(needle)
            .unwrap_or_else(|| panic!("missing ordered source fragment: {needle}"));
        cursor += relative + needle.len();
    }
}

#[test]
fn glossy_transport_uses_visible_normal_ggx_with_bounded_throughput() {
    let code = without_line_comments(COMBINE);
    let compact_code = compact(&code);
    let estimator = function_source(&code, "fn sample_ggx_vndf(");

    for required in [
        "let stretched_view = normalize",
        "let visible_normal =",
        "let unstretched = normalize",
        "return normalize(basis * unstretched)",
    ] {
        assert!(
            estimator.contains(required),
            "VNDF sampler lost visible-normal construction: {required}"
        );
    }
    assert!(!code.contains("fn sample_ggx_half("));
    assert!(compact_code.contains(
        "letvndf_weight=bounce_F*bounce_G/max(geometry_schlick_ggx(NdotV,roughness),1e-5);"
    ));
    assert!(compact_code.contains("spec_col+=reflection_li*vndf_weight;"));
    assert_in_order(
        &code,
        &[
            "let microfacet = sample_ggx_vndf(",
            "let reflection_li = secondary_incident(",
            "let vndf_weight =",
            "spec_col += reflection_li * vndf_weight",
        ],
    );

    // With the renderer's separable Smith model, G2 / G1(view) reduces to G1(light), which is
    // bounded by one. Guard the mathematical premise behind using this estimator without a clamp.
    for roughness_step in 1..=20 {
        let roughness = roughness_step as f32 / 20.0;
        let k = (roughness + 1.0).powi(2) / 8.0;
        for cosine_step in 0..=20 {
            let cosine = cosine_step as f32 / 20.0;
            let g1_light = cosine / (cosine * (1.0 - k) + k);
            assert!((0.0..=1.0 + f32::EPSILON).contains(&g1_light));
        }
    }
}

#[test]
fn diffuse_and_specular_converge_separately_while_emission_is_immediate() {
    let combine_code = compact(&without_line_comments(COMBINE));
    let resolve_code = compact(&without_line_comments(RESOLVE));
    let renderer_code = without_line_comments(RENDERER);

    for output in [
        "@location(0)diffuse:vec4<f32>",
        "@location(1)specular:vec4<f32>",
        "@location(2)immediate:vec4<f32>",
    ] {
        assert!(
            combine_code.contains(output),
            "missing combine carrier: {output}"
        );
    }
    assert!(combine_code.contains("out.immediate=vec4(g2.rgb,1.0);"));
    assert!(resolve_code.contains("@group(1)@binding(0)varaccum_tex:texture_2d<f32>;"));
    assert!(resolve_code.contains("@group(1)@binding(1)varspecular_tex:texture_2d<f32>;"));
    assert!(resolve_code.contains("@group(1)@binding(2)varimmediate_tex:texture_2d<f32>;"));
    assert!(resolve_code.contains("returnvec4(g0.rgb*irr+specular+immediate,1.0);"));

    for required in [
        "spec_accum_binds: [wgpu::BindGroup; 2]",
        "\"lit-diff\"",
        "\"lit-specular\"",
        "\"lit-immediate-emission\"",
        "mk_accum(&format!(\"spec-accum-a-v{vi}\"))",
        "mk_accum(&format!(\"spec-accum-b-v{vi}\"))",
        "label: Some(\"diffuse-accum\")",
        "label: Some(\"specular-accum\")",
        "&vc.spec_accum_binds[vc.accum_flip]",
    ] {
        assert!(
            renderer_code.contains(required),
            "renderer lost an independent temporal carrier: {required}"
        );
    }

    let diffuse_bind = function_source(&renderer_code, "let mk_accum_bind = |out: usize|");
    let specular_bind = function_source(&renderer_code, "let mk_spec_accum_bind = |out: usize|");
    assert!(diffuse_bind.contains("TextureView(&lit_diff_view)"));
    assert!(!diffuse_bind.contains("lit_emission_view"));
    assert!(specular_bind.contains("TextureView(&lit_other_view)"));
    assert!(!specular_bind.contains("lit_emission_view"));

    let render_path = &renderer_code[renderer_code
        .find("label: Some(\"combine\")")
        .expect("combine render pass")..];
    assert_in_order(
        render_path,
        &[
            "&self.lit_diff_view",
            "&self.lit_other_view",
            "&self.lit_emission_view",
            "label: Some(\"diffuse-accum\")",
            "label: Some(\"specular-accum\")",
            "label: Some(\"resolve\")",
        ],
    );
}

#[test]
fn compact_lights_use_power_phase_importance_and_exact_discrete_pdf() {
    let combine_code = without_line_comments(COMBINE);
    let smoke_code = without_line_comments(SMOKE);
    let world_medium = compact(function_source(&combine_code, "fn world_medium_radiance("));
    let local_medium = compact(function_source(&smoke_code, "fn local_in_scatter("));

    for (label, source, phase) in [
        (
            "refracted world medium",
            world_medium.as_str(),
            "physical_henyey_greenstein_phase",
        ),
        (
            "interior volume",
            local_medium.as_str(),
            "henyey_greenstein",
        ),
    ] {
        for required in [
            "varweight_sum=0.0;",
            "letpower=dot(max(",
            "letselection_target=",
            "*weight_sum;",
            "varselected_weight=0.0;",
            "/max(",
        ] {
            assert!(
                source.contains(required),
                "{label} lost importance-sampling term: {required}"
            );
        }
        assert!(
            source.contains(phase),
            "{label} proposal lost its phase term"
        );
        assert!(
            source.contains("selected_weight/weight_sum"),
            "{label} contribution is not divided by its exact discrete PDF"
        );
        assert!(
            !source.contains("*f32(nl)"),
            "{label} reverted to uniform light-count weighting"
        );
    }

    assert!(world_medium.contains("power*attenuation*physical_henyey_greenstein_phase("));
    assert!(local_medium.contains("power*attenuation*henyey_greenstein("));
    assert!(local_medium.contains("letselection_pdf=selected_weight/weight_sum;"));
    assert!(local_medium.contains("/max(selection_pdf,1e-8);"));
}

#[test]
fn resolved_motes_are_sparse_and_energy_limited_relative_to_haze() {
    let smoke_code = without_line_comments(SMOKE);
    let mote = compact(function_source(&smoke_code, "fn interior_mote("));
    let march = compact(&smoke_code);

    assert!(mote.contains("if(occupancy<0.98){return0.0;}"));
    assert!(march.contains("letmote_density=carrier*dust*20.0;"));
    assert!(march.contains("letdensity=carrier+mote_density;"));
    assert!(!mote.contains("occupancy<0.90"));

    let occupied_fraction = 1.0_f32 - 0.98;
    let shipping_dust = 0.12_f32;
    let peak_relative_density = 1.0 + shipping_dust * 20.0;
    assert!(occupied_fraction <= 0.020_001);
    assert!(peak_relative_density <= 3.4 + f32::EPSILON);
}

#[test]
fn volume_history_needs_multi_sample_low_variance_support_to_call_a_light_change() {
    let code = without_line_comments(VOLUME_ACCUM);
    let compact_code = compact(&code);

    for required in [
        "constSOURCE_CHANGE_MIN_SUPPORT:f32=4.0;",
        "constSOURCE_CHANGE_MAX_COEFFICIENT_OF_VARIATION:f32=0.35;",
        "if(accept>0.5&&samples>=SOURCE_CHANGE_MIN_SUPPORT){",
        "letstandard_error=sigma/sqrt(max(samples,1.0));",
        "letlow_variance_support=max(",
        ")<=SOURCE_CHANGE_MAX_COEFFICIENT_OF_VARIATION;",
        "letcoherent_source_change=low_variance_support&&max(",
        "if(coherent_source_change){accept=0.0;history_source=mean;}",
    ] {
        assert!(
            compact_code.contains(required),
            "volume history lost its supported light-change gate: {required}"
        );
    }
    assert_in_order(
        &code,
        &[
            "samples >= SOURCE_CHANGE_MIN_SUPPORT",
            "let standard_error =",
            "let low_variance_support =",
            "let coherent_source_change = low_variance_support",
            "if (coherent_source_change)",
        ],
    );
}

#[test]
fn portable_volume_explicitly_binds_the_empty_visibility_abi_slot() {
    let code = without_line_comments(RENDERER);
    for required in [
        "smoke_empty_visibility_bind: wgpu::BindGroup",
        "label: Some(\"smoke-empty-visibility-bind\")",
        "pass.set_bind_group(1, &self.smoke_empty_visibility_bind, &[])",
    ] {
        assert!(
            code.contains(required),
            "portable volume transport lost its explicit group(1) binding: {required}"
        );
    }
}
