//! Source-level release gate for Kessel's physical-truth boundary.

//!
//! These checks deliberately inspect only code which can enter a normal build. Rust test modules,
//! `cfg(any())` migration references, and comments are masked before matching, so documenting a
//! retired shortcut cannot itself fail the gate. The assertions use concrete resource, function,
//! and shader-expression names rather than broad words such as "ambient" or "outline".

// The eye's generated constants (src/observer.rs), installed as the renderer installs them.
#[allow(dead_code)]
#[path = "../src/observer.rs"]
mod observer;
#[allow(dead_code)]
fn with_observer(source: &str) -> String {
    source.replacen("// @observer-constants", &observer::wgsl_constants(), 1)
        .replace("// @weather-density",include_str!("../shaders/weather_volume.wgsl"))
        .replace("// @cabin-medium",include_str!("../shaders/cabin_medium.wgsl"))
}

#[path = "../src/protective_glass.rs"]
mod protective_glass;

const RENDERER_RS: &str = include_str!("../src/renderer.rs");
const GAME_RS: &str = include_str!("../src/game.rs");
const MAIN_RS: &str = include_str!("../src/main.rs");

const COMBINE_WGSL: &str = include_str!("../shaders/combine.wgsl");
const MESH_WGSL: &str = include_str!("../shaders/mesh.wgsl");
const GBUF_WGSL: &str = include_str!("../shaders/gbuf.wgsl");
const RESOLVE_WGSL: &str = include_str!("../shaders/resolve.wgsl");
const CRUSH_WGSL: &str = include_str!("../shaders/crush.wgsl");
const SMOKE_WGSL: &str = include_str!("../shaders/smoke.wgsl");

const PHYSICAL_TRUTH_LEDGER: &str = include_str!("../docs/PHYSICAL_TRUTH_AUDIT.md");

#[test]
fn camera_response_shader_parses_and_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&with_observer(CRUSH_WGSL))
        .unwrap_or_else(|error| panic!("camera response WGSL parse failed: {error}"));
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    );
    validator
        .validate(&module)
        .unwrap_or_else(|error| panic!("camera response WGSL validation failed: {error}"));
}

#[test]
fn clean_camera_endpoint_cannot_execute_screen_grain_or_dither() {
    let source=compact(&shader_code(CRUSH_WGSL));
    let retro=braced_block(&source,"if(retro>0.001)");
    let ldr=braced_block(&source,"if(cr.ldr_dither>0.0)");
    let threshold=braced_block(&source,"fndisplay_threshold(");
    let enabled=braced_block(&source,"if(cr.retro>0.001||cr.ldr_dither>0.0)");
    // One shared native-pixel decision, evaluated only when at least one consumer is enabled.
    // Clean/off must neither sample blue noise nor evaluate a Bayer pattern.
    assert_eq!(enabled.matches("display_threshold(vec2<i32>(in.pos.xy))").count(),1);
    assert!(enabled.contains("native_threshold=display_threshold("));
    assert_eq!(source.matches("display_threshold(").count(),2,"one definition and one gated call");
    assert!(source.contains("varnative_threshold=0.0;"));
    assert!(retro.contains("letordered=native_threshold;"));
    assert!(ldr.contains("letthreshold=native_threshold;"));
    assert_eq!(retro.matches("textureLoad(depth_tex").count(),1);
    assert_eq!(source.matches("textureLoad(depth_tex").count(),1,"depth mask must stay retro-only");
    assert!(!CRUSH_WGSL.contains("temporal_grain("));
    assert!(!CRUSH_WGSL.contains("fn noise2"));
    assert!(!CRUSH_WGSL.contains("var grain_amt"));
    // A stationary resolved image has no clock-dependent pattern in its quantization branch.
    for code in [retro,ldr,threshold,enabled] {assert!(!code.contains("cr.frame"));}
    assert!(threshold.contains("(px.y&3)*4+(px.x&3)"));
    assert!(threshold.contains("textureLoad(blue_noise,vec2(px.x&63,px.y&63),0)"));
}

/// Read a complete balanced block, so a call after an if's closing brace cannot satisfy the gate.
fn braced_block<'a>(source:&'a str,marker:&str)->&'a str {
    let start=source.find(marker).unwrap_or_else(||panic!("missing block {marker}"));
    let open=start+source[start..].find('{').expect("block opening brace");
    let mut depth=0;
    for (offset,byte) in source.as_bytes()[open..].iter().enumerate() {
        if *byte==b'{' {depth+=1;}
        if *byte==b'}' {depth-=1;if depth==0 {return &source[open+1..open+offset];}}
    }
    panic!("unclosed block {marker}");
}

fn mask_bytes_except_newlines(bytes: &mut [u8], start: usize, end: usize) {
    for byte in &mut bytes[start..end] {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

fn quoted_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    // A brace in a Rust char (including the quote reached after a byte prefix) is not structure.
    // Consume exactly one scalar/escape and require its closing quote, so lifetimes such as 'a
    // and 'static remain code instead of swallowing everything up to a later apostrophe.
    if bytes.get(start) == Some(&b'\'') {
        let mut cursor = start + 1;
        match bytes.get(cursor).copied()? {
            b'\\' => {
                cursor += 1;
                match bytes.get(cursor).copied()? {
                    b'u' if bytes.get(cursor + 1) == Some(&b'{') => {
                        cursor += 2;
                        while bytes.get(cursor).is_some_and(|b| *b != b'}') { cursor += 1; }
                        if bytes.get(cursor) != Some(&b'}') { return None; }
                        cursor += 1;
                    }
                    b'x' => cursor += 3,
                    _ => cursor += 1,
                }
            }
            b'\n' | b'\r' | b'\'' => return None,
            scalar => {
                cursor += match scalar {
                    0x00..=0x7f => 1,
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return None,
                };
            }
        }
        return (bytes.get(cursor) == Some(&b'\'')).then_some(cursor + 1);
    }
    if bytes.get(start) == Some(&b'"') {
        let mut i = start + 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i = (i + 2).min(bytes.len()),
                b'"' => return Some(i + 1),
                _ => i += 1,
            }
        }
        return Some(bytes.len());
    }

    // Rust raw strings: r"...", r#"..."#, and their byte-string br#"..."# variants.
    let raw_start = if bytes.get(start) == Some(&b'r') {
        start
    } else if bytes.get(start) == Some(&b'b') && bytes.get(start + 1) == Some(&b'r') {
        start + 1
    } else {
        return None;
    };
    let mut quote = raw_start + 1;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }
    let hashes = quote - raw_start - 1;
    let mut i = quote + 1;
    while i < bytes.len() {
        let suffix_start = i + 1;
        let suffix_end = suffix_start + hashes;
        if bytes[i] == b'"'
            && bytes
                .get(suffix_start..suffix_end)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            return Some(suffix_end);
        }
        i += 1;
    }
    Some(bytes.len())
}

/// Mask comments without changing byte offsets. String contents remain intact because some
/// contracts intentionally inspect shader include paths and GPU resource labels.
fn without_comments(source: &str) -> String {
    let input = source.as_bytes();
    let mut output = input.to_vec();
    let mut i = 0;
    while i < input.len() {
        if let Some(end) = quoted_literal_end(input, i) {
            i = end;
            continue;
        }
        if input.get(i..i + 2) == Some(b"//") {
            let end = input[i + 2..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(input.len(), |offset| i + 2 + offset);
            mask_bytes_except_newlines(&mut output, i, end);
            i = end;
            continue;
        }
        if input.get(i..i + 2) == Some(b"/*") {
            let mut end = i + 2;
            let mut depth = 1usize;
            while end < input.len() && depth > 0 {
                if input.get(end..end + 2) == Some(b"/*") {
                    depth += 1;
                    end += 2;
                } else if input.get(end..end + 2) == Some(b"*/") {
                    depth -= 1;
                    end += 2;
                } else {
                    end += 1;
                }
            }
            mask_bytes_except_newlines(&mut output, i, end);
            i = end;
            continue;
        }
        i += 1;
    }
    String::from_utf8(output).expect("masking ASCII comment bytes preserves UTF-8")
}

/// Mask quoted literals for structural scanning while retaining the original byte indices.
fn without_literals(source: &str) -> String {
    let input = source.as_bytes();
    let mut output = input.to_vec();
    let mut i = 0;
    while i < input.len() {
        if let Some(end) = quoted_literal_end(input, i) {
            mask_bytes_except_newlines(&mut output, i, end);
            i = end;
        } else {
            i += 1;
        }
    }
    String::from_utf8(output).expect("masking ASCII literal bytes preserves UTF-8")
}

/// Remove items/statements guarded by an always-false or test-only cfg. The source has already had
/// comments masked, and a literal-masked twin prevents braces or semicolons in strings from being
/// mistaken for Rust structure.
fn without_cfg_items(mut source: String, attribute: &str) -> String {
    loop {
        let structural = without_literals(&source);
        let Some(start) = structural.find(attribute) else {
            break;
        };
        let tail = structural.as_bytes();
        let mut cursor = start + attribute.len();
        while cursor < tail.len() && tail[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        let mut first_brace = None;
        let mut first_semicolon = None;
        for (offset, byte) in tail[cursor..].iter().enumerate() {
            match *byte {
                b'{' => {
                    first_brace = Some(cursor + offset);
                    break;
                }
                b';' => {
                    first_semicolon = Some(cursor + offset);
                    break;
                }
                _ => {}
            }
        }

        let end = if let Some(semicolon) = first_semicolon {
            semicolon + 1
        } else {
            let open = first_brace.unwrap_or_else(|| {
                panic!("{attribute} in source was not followed by a braced or semicolon item")
            });
            let mut depth = 0usize;
            let mut close = None;
            for (offset, byte) in tail[open..].iter().enumerate() {
                match *byte {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(open + offset + 1);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            close.unwrap_or_else(|| panic!("unbalanced item after {attribute}"))
        };

        let mut bytes = source.into_bytes();
        mask_bytes_except_newlines(&mut bytes, start, end);
        source = String::from_utf8(bytes).expect("cfg masking preserves UTF-8");
    }
    source
}

fn shipping_rust(source: &str) -> String {
    let source = without_comments(source);
    let source = without_cfg_items(source, "#[cfg(test)]");
    without_cfg_items(source, "#[cfg(any())]")
}

#[test]
fn cfg_mask_ignores_character_braces_but_preserves_lifetimes_and_shipping_code() {
    let source = r###"
fn before<'a>(value: &'a str) -> &'a str { value }
#[cfg(test)]
mod tests {
    fn diagnostic() {
        let close = '}'; let open = '{'; let byte = b'}';
        let escaped = '\''; let unicode = '\u{7d}'; let hex = '\x7b'; let wide = '東';
        let text = "// not a comment }"; let raw = r#"also }"#;
        let forbidden_test_only = "PTUniform";
    }
}
fn after<'static_lifetime>() { let real_shipping = "pt_out_view"; }
"###;
    let masked = shipping_rust(source);
    assert_eq!(masked.len(), source.len());
    assert_eq!(masked.matches('\n').count(), source.matches('\n').count());
    assert!(!masked.contains("PTUniform"));
    assert!(!masked.contains("fn diagnostic"));
    assert!(masked.contains("fn before<'a>(value: &'a str) -> &'a str"));
    assert!(masked.contains("fn after<'static_lifetime>()"));
    assert!(masked.contains("pt_out_view"), "real shipping violations must remain detectable");
    assert_eq!(quoted_literal_end(b"'a str", 0), None);
    assert_eq!(quoted_literal_end(b"'static str", 0), None);
}

fn shader_code(source: &str) -> String {
    without_comments(source)
}

fn compact(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn assert_absent(label: &str, source: &str, forbidden: &[&str]) {
    let found: Vec<_> = forbidden
        .iter()
        .copied()
        .filter(|needle| source.contains(needle))
        .collect();
    assert!(
        found.is_empty(),
        "{label} reintroduced forbidden shipping paths: {found:?}"
    );
}

fn contains_identifier(source:&str,identifier:&str)->bool {
    source.split(|c:char|!(c.is_alphanumeric()||c=='_')).any(|token|token==identifier)
}

#[test]
fn legacy_resource_identifier_checks_do_not_reject_distinct_live_resources() {
    assert!(!contains_identifier("self.raster_glass_pipe: RenderPipeline", "glass_pipe"));
    assert!(contains_identifier("self.glass_pipe: RenderPipeline", "glass_pipe"));
    assert!(contains_identifier("let glass_pipe = make_pipe();", "glass_pipe"));
    assert_eq!(braced_block("if(x){inside{nested}}outside()","if(x)"),"inside{nested}");
}

fn assert_present(label: &str, source: &str, required: &[&str]) {
    let missing: Vec<_> = required
        .iter()
        .copied()
        .filter(|needle| !source.contains(needle))
        .collect();
    assert!(
        missing.is_empty(),
        "{label} lost required physical-truth paths: {missing:?}"
    );
}

#[test]
fn retired_pathtrace_and_rtt_atlas_resources_cannot_enter_shipping_renderer() {
    let renderer = shipping_rust(RENDERER_RS);
    let game = shipping_rust(GAME_RS);
    let main = shipping_rust(MAIN_RS);
    for (label, source) in [
        ("renderer", renderer.as_str()),
        ("game", game.as_str()),
        ("main", main.as_str()),
    ] {
        assert_absent(
            label,
            source,
            &[
                "include_str!(\"../shaders/pathtrace.wgsl\")",
                "PTUniform",
                "pathtrace_pipe",
                "pathtrace_bind",
                "pt_out_tex",
                "pt_out_view",
                "pt_out_bind",
                "label: Some(\"pathtrace-",
            ],
        );
    }

    // Method-name compatibility shims may remain no-ops during call-site migration. What cannot
    // return is an allocated/bound/copied pasted-camera atlas in the renderer.
    assert_absent(
        "renderer",
        &renderer,
        &[
            "rtt_tex:",
            "rtt_view:",
            "rtt_bind:",
            "rtt_sampler:",
            "label: Some(\"rtt-atlas\")",
            "label: Some(\"rtt-bgl\")",
            "label: Some(\"rtt-bind\")",
            "copy_texture_to_texture(rtt_",
        ],
    );
}

#[test]
fn relay_lens_payload_cannot_be_read_as_emitted_radiance() {
    let high = shader_code(COMBINE_WGSL);
    let emission = high.split("fn emitted_radiance(").nth(1).unwrap()
        .split("fn cosine_direction(").next().unwrap();
    let guard = emission.find("if (hit.emission_mode == 0u || (hit.flags & 16u) != 0u) { return vec3(0.0); }")
        .expect("Disabled and optical lens records must terminate with zero radiance");
    for payload_read in ["hit.emission_scale", "rt_instances[hit.instance_record].emission"] {
        assert!(guard < emission.find(payload_read).unwrap(),
            "lens metadata was read before its non-emissive tag gate");
    }
    assert!(high.contains("if (instance.ids.z != 0u && (instance.ids.w & 16u) == 0u) {"),
        "the material-hit scale must also exclude lens payloads");
}

#[test]
fn high_path_commits_exact_triangle_material_texture_hits_and_one_bounce() {
    let high = shader_code(COMBINE_WGSL);
    let renderer = shipping_rust(RENDERER_RS);
    assert_present(
        "High renderer",
        &renderer,
        &[
            "physical_atmosphere_transport_shader_source(include_str!(",
            "\"../shaders/combine.wgsl\",",
            "let deferred = rt_on",
            "rt_surface_gpu: Option<RtSurfaceGpu>",
            "combine_rt_bind: Option<wgpu::BindGroup>",
        ],
    );
    assert_present(
        "High combine shader",
        &high,
        &[
            "@group(2) @binding(0) var tlas: acceleration_structure",
            "fn trace_exact_surface(",
            "rayQueryGetCommittedIntersection(&rq)",
            "hit.kind != RAY_QUERY_INTERSECTION_TRIANGLE",
            "hit.instance_custom_data",
            "hit.primitive_index",
            "hit.barycentrics",
            "hit.object_to_world",
            "hit.world_to_object",
            "let a = rt_vertex(",
            "var material = rt_material(material_id, uv)",
            "rt_sample_texture(base_tex, uv, 0.0)",
            "rt_sample_texture(mr_tex, uv, 0.0)",
            "rt_sample_texture(normal_tex, uv, 0.0)",
            "fn emitted_radiance(",
            "fn secondary_incident(",
            // The current visible-normal GGX estimator and cosine estimator feed one iterated
            // secondary call site. This preserves transport while containing Vulkan inlining.
            "fn sample_ggx_vndf(",
            "var bounce_weight = array<vec3<f32>, 2>(diffuse_kd * albedo * diffuse_pdf_weight, reflection_weight);",
            "let li = secondary_incident(bounce_origin, bounce_direction[slot], bounce_seed[slot]);",
            "let bounce_passes = max(i32(cam.transport_passes.y), 1);",
            "return world_medium_environment_source(",
        ],
    );

    // Exact dielectric reflection/transmission also shares one inlined continuation. These are
    // host-written invariant counts, not quality knobs or altered transport laws.
    for required in [
        "transport_passes: vec4<f32>",
        "let interface_passes = max(i32(cam.transport_passes.x), 1);",
        "radiance += non_glass_incident(pass_origin[slot], pass_direction[slot], pass_seed[slot])",
    ] {
        assert!(
            high.contains(required),
            "lost Vulkan containment: {required}"
        );
    }

    assert_absent(
        "High combine shader",
        &high,
        &[
            "screen_space_contact_ao",
            "screen_space_diffuse_bounce",
            "screen_space_gi",
            "ssao",
            "textureGather(",
            "textureSample(g0_tex",
            "textureSample(g1_tex",
            "textureSample(g2_tex",
            "textureLoad(lit_diff",
            "textureSample(hdr",
            "average_material",
            "RtBox",
            "heightfield",
        ],
    );

    // One primary fetch per required G-buffer lane and one primary depth fetch. Additional
    // neighbourhood reads would be a screen-space lighting/AO gather, not traced transport.
    for (needle, expected) in [
        ("textureLoad(g0_tex", 1),
        ("textureLoad(g1_tex", 1),
        ("textureLoad(g2_tex", 1),
        ("textureLoad(depth_tex", 1),
    ] {
        assert_eq!(
            high.matches(needle).count(),
            expected,
            "High combine changed its primary-only {needle:?} fetch contract"
        );
    }
}

#[test]
fn lighting_and_post_do_not_invent_readability_or_weather_energy() {
    let high = shader_code(COMBINE_WGSL);
    let forward = shader_code(MESH_WGSL);
    let gbuf = shader_code(GBUF_WGSL);
    let resolve = shader_code(RESOLVE_WGSL);
    let crush = shader_code(CRUSH_WGSL);
    let renderer = shipping_rust(RENDERER_RS);

    for (label, source) in [
        ("High combine", high.as_str()),
        ("forward surface", forward.as_str()),
        ("G-buffer", gbuf.as_str()),
        ("resolve", resolve.as_str()),
        ("crush", crush.as_str()),
        ("renderer", renderer.as_str()),
    ] {
        assert_absent(
            label,
            source,
            &[
                "ambient_floor",
                "readability_floor",
                "floor_day",
                "crew_floor",
                "cabin_bounce",
                "interior_kill",
                "spring_fill",
                "oder_fill",
                "authored_forward_exposure",
                "campaign_exposure",
                "atmosphere_air",
                "weather_desat",
                "mix(col, air",
                "mix(col, fog",
                "exp(-cam.fog_density",
            ],
        );
    }

    assert_present(
        "High environment sample",
        &high,
        &[
            "var diff_col = direct_diff",
            "let sky_path = source_connection(ro, sky_dir, 0.01, 2000.0)",
            "let sky = world_medium_environment_source(",
            "solar_source.diffuse_radiance",
        ],
    );
    assert_present("forward surface", &forward, &["var col = direct"]);
    assert_absent(
        "source albedo",
        &format!("{forward}\n{gbuf}"),
        &["pow(raw, vec3(0.65", "pow(raw, vec3(0.70", "raw * 1.5"],
    );

    // Camera fields may stay ABI-compatible, but post/surface code must not consume them as a
    // distance fog or view-state illumination knob.
    assert_eq!(resolve.matches("cam.fog_density").count(), 0);
    assert_eq!(resolve.matches("cam.atmosphere").count(), 0);
    assert_eq!(high.matches("cam.interior_light").count(), 0);
    assert_eq!(forward.matches("cam.interior_light").count(), 0);

    // This was the last explicit post-tonemap steel-readability lift. ACES exposure, a black point,
    // and the retro quantizer are camera/recording operations; additive shadow colour is not.
    assert_absent(
        "crush post",
        &compact(&crush),
        &["m=m+vec3(-0.004,0.003,0.012)*shadow"],
    );
}

#[test]
fn aperture_and_screen_overlay_proxies_cannot_reappear() {
    let renderer = shipping_rust(RENDERER_RS);
    let game = shipping_rust(GAME_RS);
    let forward = shader_code(MESH_WGSL);
    let gbuf = shader_code(GBUF_WGSL);
    let crush = shader_code(CRUSH_WGSL);
    let smoke = shader_code(SMOKE_WGSL);

    assert_absent(
        "renderer",
        &renderer,
        &[
            "struct InteriorGlow",
            "set_interior_glows",
            "MAX_APERTURES",
            "apertures: Vec<Aperture>",
            "shaft_sun_dir(",
            "outline_pipe",
            "xray_pipe",
            "label: Some(\"outline-",
            "label: Some(\"xray-",
        ],
    );
    assert!(!contains_identifier(&renderer,"glass_pipe"),
        "retired screen-overlay glass_pipe resource returned; raster_glass_pipe is a distinct surface pipeline");
    assert_absent(
        "game",
        &game,
        &[
            "select_interior_apertures",
            "port_aps",
            "interior_glows",
            ".set_outline(",
        ],
    );
    assert_absent(
        "volumetric shader",
        &smoke,
        &[
            "n_glows: f32",
            "glows: array",
            "n_aps: f32",
            "aps: array",
            "aperture_cone",
            "shaft_cone",
            "analytic_aperture",
        ],
    );
    assert_absent(
        "surface shaders",
        &format!("{forward}\n{gbuf}"),
        &[
            "glass_stipple",
            "stipple_keep",
            "screen_door",
            "xray_keep",
            "xray_alpha",
            "outline_scale",
            "outline_emissive",
        ],
    );
    let compact_surfaces = compact(&format!("{forward}\n{gbuf}"));
    assert_absent(
        "surface shader mode branches",
        &compact_surfaces,
        &[
            // Retired irregular glass stipple and x-ray checker discard bodies.
            "if((h^(h>>13u))%11u)>3u{discard;}",
            "if((px.x+px.y)%2u==0u){discard;}",
            // Retired inverted-hull inflation and emissive rim bodies.
            "if(pc.params.w>5.5&&pc.params.w<6.5){p=pos+normal*0.010;}",
            "if(pc.params.w>5.5){returnvec4(vec3(1.0,0.72,0.30)*(2.2*pc.params.x),1.0);}",
        ],
    );
    assert_absent(
        "crush shader",
        &crush,
        &[
            "cr.crew_ko",
            "cr.ko_dir",
            "crew_blood",
            "blood_splatter",
            "blood_overlay(",
            "cr.rt_mix",
        ],
    );
    assert!(
        !compact(&game).contains("set_xray(slot,true)"),
        "shipping gameplay attempted to enable the retired x-ray mode"
    );
}

#[test]
fn high_direct_view_glass_uses_geometric_dielectric_transport() {
    let renderer = shipping_rust(RENDERER_RS);
    let gbuf = shader_code(GBUF_WGSL);
    let combine = shader_code(COMBINE_WGSL);
    let resolve = shader_code(RESOLVE_WGSL);

    for required in [
        "glass: bool",
        "mesh.glass = on",
        "crate::rt::MASK_GLASS",
        "if m.glass {",
    ] {
        assert!(
            renderer.contains(required),
            "renderer omitted glass transport state: {required}"
        );
    }
    for required in [
        "pc.params.w > 4.5 && pc.params.w < 5.5",
        "return pack(raw, 2.0",
    ] {
        assert!(gbuf.contains(required), "G-buffer omitted: {required}");
    }
    for required in [
        "const GLASS_IOR = SOURCE_GLASS_IOR",
        "fn dielectric_fresnel(",
        "refract(incident, entry_normal, 1.0 / GLASS_IOR)",
        "trace_exact_surface_mask(",
        "RT_MASK_GLASS",
        "let thickness_m = max(exit_hit.ray_t",
        "exp(-GLASS_ABSORPTION_PER_M * thickness_m)",
        "world_medium: vec4<f32>",
        "fn world_medium_radiance(",
        "physical_bounded_medium_interval(",
        "background * throughput + inscattered",
        "return glass_incident(hit.position, hit.normal, direction, seed, (hit.flags & 28u) != 0u)",
    ] {
        assert!(
            combine.contains(required),
            "High glass path omitted: {required}"
        );
    }
    assert!(resolve.contains("g0.a > 0.5 && g0.a < 1.5"));

    // These would substitute an image or authored brightness for transported radiance.
    assert_absent(
        "High glass transport",
        &combine,
        &[
            "textureSample(g0_tex",
            "textureSampleLevel(g0_tex",
            "screen_uv",
            "aperture_emissive",
            "viewport_fill",
        ],
    );

    // Match physical_atmosphere_transport_shader_source and its shared source injection. The
    // weather/cabin libraries are actual production dependencies, not stubs for validation.
    let atmosphere=include_str!("../shaders/physical_environment.wgsl").split_once(
        "// --- Persistent physical snow ---------------------------------------------------------------")
        .expect("production atmosphere/snow split").0;
    let full_shader = format!(
        "{}\n{}",
        atmosphere,
        protective_glass::inject(COMBINE_WGSL, false).replace(
            "// @campaign-terrain-material", include_str!("../shaders/terrain_material.wgsl"))
            .replace("// @weather-density",include_str!("../shaders/weather_volume.wgsl"))
            .replace("// @cabin-medium",include_str!("../shaders/cabin_medium.wgsl"))
    );
    let full_shader=with_observer(&full_shader);
    let module = wgpu::naga::front::wgsl::parse_str(&full_shader)
        .unwrap_or_else(|error| panic!("High dielectric WGSL parse failed: {error}"));
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    );
    validator
        .validate(&module)
        .unwrap_or_else(|error| panic!("High dielectric WGSL validation failed: {error}"));
}

#[test]
fn projectile_result_and_penetration_presentation_follow_authoritative_impacts() {
    let game = shipping_rust(GAME_RS);
    let compact_game = compact(&game);
    assert_absent(
        "shipping projectile code",
        &game,
        &[
            "pending_target",
            "pending_outcome",
            "land_player_shot(",
            "legacy_fire_trigger_time_disabled(",
        ],
    );
    assert_present(
        "shipping projectile code",
        &game,
        &[
            "fn spawn_authoritative_projectile(",
            "fn advance_projectiles(",
            "fn resolve_authoritative_terminal(",
            "fn arm_authoritative_impact_presentation(",
        ],
    );

    for slot in ["pen_flash", "pen_beam", "pen_burst"] {
        assert!(
            !compact_game.contains(&format!("set_visible(self.slots.{slot},true)")),
            "authored {slot} became an independently activated penetration effect"
        );
    }
    assert!(
        compact_game
            .contains("forslotin[self.slots.pen_flash,self.slots.pen_beam,self.slots.pen_burst,]")
            && compact_game.contains("r.set_visible(slot,false)")
            && compact_game.contains("r.set_emissive(slot,0.0)"),
        "the retired authored penetration meshes are no longer forcibly inert"
    );
}

fn ledger_row(system: &str) -> &str {
    PHYSICAL_TRUTH_LEDGER
        .lines()
        .find(|line| line.starts_with(&format!("| {system} |")))
        .unwrap_or_else(|| panic!("physical-truth ledger lost the {system:?} row"))
}

#[test]
fn ledger_names_every_known_p0_instead_of_claiming_completion() {
    let atmosphere = ledger_row("Atmosphere / environment");
    assert!(atmosphere.contains("P0"));
    assert!(atmosphere.contains("not a solved atmospheric medium"));

    let snow = ledger_row("Snow / spindrift");
    assert!(snow.contains("P0"));
    assert!(snow.contains("immutable Lagrangian world cells"));
    assert!(snow.contains("no white-alpha particle"));
    assert!(snow.contains("Laptop keeps the bulk suspension"));

    let scene = ledger_row("RT scene completeness");
    assert!(scene.contains("P0"));
    assert!(scene.contains("view-local crew"));
    assert!(scene.contains("raster-only"));

    let optics = ledger_row("Materials / emission");
    assert!(
        optics.contains("P0"),
        "missing glass/periscope optics must remain explicitly classified P0"
    );
    assert!(optics.contains("full periscope optical path"));

    let crew_collision = ledger_row("Crew / gore");
    assert!(
        crew_collision.contains("P0"),
        "crew/fragment collision against visible triangles must remain explicitly classified P0"
    );
    assert!(crew_collision.contains("visible triangle interior"));

    let world_collision = ledger_row("World collision/destruction");
    assert!(
        world_collision.contains("P0"),
        "world collision proxies must remain explicitly classified P0"
    );
    assert!(world_collision.contains("source-derived triangle/convex data"));
}
