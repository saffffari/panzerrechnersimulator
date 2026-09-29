//! Bloom and tonemapping must read the same adapted camera, including dark interiors.

#[test]
fn bloom_camera_matches_the_crush_uniform_exposure_offset_and_span() {
    let shader = include_str!("../shaders/bloom.wgsl");
    let module = wgpu::naga::front::wgsl::parse_str(shader).expect("bloom shader parses");
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    ).validate(&module).expect("bloom shader validates");
    let ty = module.types.iter().map(|(_, ty)| ty)
        .find(|ty| ty.name.as_deref() == Some("BloomCamera")).expect("shared camera view");
    let wgpu::naga::TypeInner::Struct { members, span } = &ty.inner else {
        panic!("camera view must remain a struct");
    };
    assert_eq!(*span, 56, "must bind the existing CrushUniform without overrunning it");
    assert_eq!(members.iter().find(|m|m.name.as_deref()==Some("exposure")).unwrap().offset, 44);
    let renderer = include_str!("../src/renderer.rs");
    let host = renderer.split("struct CrushUniform {").nth(1).unwrap().split('}').next().unwrap();
    let bytes = |fields: &str| fields.matches(": f32").count()*4
        + fields.matches(": [f32; 3]").count()*12;
    assert_eq!(bytes(host), *span as usize);
    // The cone channel lane (not the eye's rod lanes at bytes 16-20) sits at byte 44.
    assert_eq!(bytes(host.split("\n    exposure: f32").next().unwrap()), 44);
    assert_eq!(bytes(host.split("rod_exposure: f32").next().unwrap()), 16);
}

#[test]
fn adapted_camera_is_available_to_bloom_before_tonemapping() {
    let renderer = include_str!("../src/renderer.rs");
    let exposure = renderer.find("// ---- S4 auto-exposure: meter transported HDR before bloom").unwrap();
    let bloom = renderer.find("// ---- emissive bloom (half-res), BEFORE crush").unwrap();
    let crush = renderer[bloom..].find("label: Some(\"crush\")").unwrap()+bloom;
    assert!(exposure < bloom && bloom < crush);
    let binding = renderer.split("let make_bloom_bind").nth(1).unwrap()
        .split("let bloom_bind_hdr").next().unwrap();
    assert!(binding.contains("resource: crush_buf.as_entire_binding()"));
    // Particle coverage is deliberately applied after glow and tonemapping: a dust/snow
    // point remains exactly one native pixel even in a brightly exposed view.
    let particles = renderer[crush..].find("label: Some(\"one-pixel-world-particles\")").unwrap()+crush;
    assert!(particles > crush);
}
