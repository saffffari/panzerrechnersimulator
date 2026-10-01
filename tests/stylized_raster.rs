//! CPU-only acceptance of the optional raster backend's complete shaders and existing GPU ABIs.

//! This establishes shader validity/resource compatibility; actual camera captures own visual QA.
// The eye's generated constants (src/observer.rs), installed as the renderer installs them.
#[allow(dead_code)]
#[path = "../src/observer.rs"]
mod observer;
#[allow(dead_code)]
fn with_observer(source: &str) -> String {
    source.replacen("// @observer-constants", &observer::wgsl_constants(), 1)
}

use wgpu::naga;

const MESH: &str = include_str!("../shaders/stylized_mesh.wgsl");
const PROBE: &str = include_str!("../shaders/environment_probe.wgsl");
const SMOKE: &str = include_str!("../shaders/stylized_smoke.wgsl");
const PARTICLES: &str = include_str!("../shaders/stylized_particles.wgsl");

fn compose(source: &str) -> String {
    with_observer(source).replace("// @camera-relative-projection",include_str!("../shaders/camera_relative.wgsl"))
        .replace("// @campaign-terrain-material",include_str!("../shaders/terrain_material.wgsl"))
        .replace("// @sky-visibility-sampling",include_str!("../shaders/sky_visibility_sampling.wgsl"))
        .replace("// @cabin-visibility-sampling",include_str!("../shaders/cabin_visibility.wgsl"))
        .replace("// @weather-density",include_str!("../shaders/weather_volume.wgsl"))
}
/// The physical atmosphere library prepended as renderer.rs prepends it
/// (`physical_atmosphere_transport_shader_source`): everything before the snow section.
fn physical(entry: &str) -> String {
    let library=include_str!("../shaders/physical_environment.wgsl");
    let atmosphere=library.split_once("// --- Persistent physical snow ---").unwrap().0;
    compose(&format!("{atmosphere}\n{entry}"))
}
fn wgsl_constant(source: &str, name: &str) -> f32 {
    let line=source.lines().find(|l|l.trim_start().starts_with(&format!("const {name} ")))
        .unwrap_or_else(||panic!("{name} missing"));
    line.split('=').nth(1).unwrap().trim().trim_end_matches(';').trim().parse().unwrap()
}
fn parse(source: &str) -> naga::Module {
    naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{}",e.emit_to_string(source)))
}
fn structure(source: &str, name: &str) -> String {
    let start=source.find(&format!("struct {name} {{")).unwrap();
    let end=source[start..].find("\n};").unwrap()+start+3;
    source[start..end].to_owned()
}
fn layout(module: &naga::Module, name: &str) -> (u32,Vec<(String,u32)>) {
    let ty=module.types.iter().find(|(_,ty)|ty.name.as_deref()==Some(name)).unwrap().1;
    let naga::TypeInner::Struct{members,span}= &ty.inner else {panic!("not a structure")};
    (*span,members.iter().map(|m|(m.name.clone().unwrap(),m.offset)).collect())
}

#[test]
fn raster_shaders_validate_without_ray_query_capability() {
    for (name,source) in [("mesh",MESH),("smoke",SMOKE),("particles",PARTICLES),("ssao",include_str!("../shaders/raster_ssao.wgsl")),("outline",include_str!("../shaders/hover_outline.wgsl"))] {
        let source=compose(source);let module=parse(&source);
        let capabilities=naga::valid::Capabilities::all() & !naga::valid::Capabilities::RAY_QUERY;
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(),capabilities)
            .validate(&module).unwrap_or_else(|e|panic!("{name}: {e:?}"));
        assert!(module.entry_points.iter().any(|e|e.name=="vs" && e.stage==naga::ShaderStage::Vertex));
        assert!(module.entry_points.iter().any(|e|e.name=="fs" && e.stage==naga::ShaderStage::Fragment));
        for forbidden in ["rayQuery", "acceleration_structure", "temporal_sample", "texture_storage", "history_tex"] {
            assert!(!source.contains(forbidden),"{name}: forbidden dependency {forbidden}");
        }
        if name=="mesh" {assert!(module.entry_points.iter().any(|e|e.name=="vs_inst"));}
    }
    // The raster paths draw the physical sky and fill their environment from the physical
    // atmosphere; both compile without ray queries.
    for (name,source) in [("sky",physical(include_str!("../shaders/sky.wgsl"))),("probe",physical(PROBE))] {
        let module=parse(&source);
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all() & !naga::valid::Capabilities::RAY_QUERY)
            .validate(&module).unwrap_or_else(|e|panic!("{name}: {e:?}"));
        assert!(!source.contains("rayQuery"),"{name} must not trace");
    }
    let probe=parse(&physical(PROBE));
    for entry in ["probe_directions","probe_finish"] {
        assert!(probe.entry_points.iter().any(|e|e.name==entry && e.stage==naga::ShaderStage::Compute),"{entry}");
    }
}

#[test]
fn the_stylized_sky_and_daylight_palettes_are_gone() {
    // The raster backend's sky, sun and cloud light came from hand-fitted palettes; they are
    // replaced by the physical atmosphere (sky.wgsl and the environment probe).
    for gone in ["shaders/stylized_sky.wgsl","shaders/solar_daylight.wgsl"] {
        assert!(!std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(gone).exists(),"{gone} returned");
    }
    for (name,source) in [("mesh",MESH),("smoke",SMOKE),("particles",PARTICLES)] {
        for palette in ["@solar-daylight","stylized_environment","diffuse_daylight","night_floor"] {
            assert!(!source.contains(palette),"{name}: {palette}");
        }
    }
}

#[test]
fn raster_media_share_the_world_medium_optics_of_combine() {
    let combine=include_str!("../shaders/combine.wgsl");
    for (world,probe,fog) in [
        ("WORLD_AEROSOL_PHASE_G","PROBE_AEROSOL_PHASE_G","FOG_AEROSOL_PHASE_G"),
        ("WORLD_SNOW_PHASE_G","PROBE_SNOW_PHASE_G","FOG_SNOW_PHASE_G"),
        ("WORLD_AEROSOL_SINGLE_SCATTER_ALBEDO","PROBE_AEROSOL_SINGLE_SCATTER_ALBEDO","FOG_AEROSOL_SINGLE_SCATTER_ALBEDO"),
        ("WORLD_SNOW_SINGLE_SCATTER_ALBEDO","PROBE_SNOW_SINGLE_SCATTER_ALBEDO","FOG_SNOW_SINGLE_SCATTER_ALBEDO"),
    ] {
        let reference=wgsl_constant(combine,world);
        assert_eq!(wgsl_constant(PROBE,probe),reference,"{probe}");
        assert_eq!(wgsl_constant(SMOKE,fog),reference,"{fog}");
    }
}

#[test]
fn raster_camera_material_push_and_smoke_layouts_match_existing_pipeline() {
    let old_mesh=include_str!("../shaders/mesh.wgsl");
    let camera=parse(&structure(old_mesh,"Camera"));
    let mesh=parse(&compose(MESH));let sky=parse(&physical(include_str!("../shaders/sky.wgsl")));
    assert_eq!(layout(&mesh,"Camera"),layout(&camera,"Camera"));
    assert_eq!(layout(&mesh,"Camera").0,1296);
    assert_eq!(layout(&mesh,"Push").0,80);
    let bindings:std::collections::BTreeSet<_>=mesh.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(bindings,[(0,0),(0,1),(0,2),(1,0),(1,1),(1,2),(1,3),
        (2,0),(2,1),(2,2),(2,3),(2,4),(2,5),(2,6),(2,7),(2,8),(3,0),(3,1)].into_iter().collect());
    // The probe the mesh reads is the probe the compute pass writes.
    assert_eq!(layout(&mesh,"EnvironmentProbe").0,68*16);
    assert_eq!(layout(&mesh,"NearShadow"),(80,vec![("view_proj".into(),0),("scale".into(),64)]));
    // Eight fixed cabin fixtures plus eight source-coupled world fires, as in the Rust bank.
    assert_eq!(layout(&mesh,"CabinShadows"),(320,vec![("lamps".into(),0),("torch_position".into(),272),("torch_direction".into(),288),("range".into(),304)]));
    let old_smoke=include_str!("../shaders/smoke.wgsl");
    let old=parse(&(structure(old_smoke,"Puff")+&structure(old_smoke,"Smoke")));
    let smoke=parse(&compose(SMOKE));
    assert_eq!(layout(&smoke,"CabinShadows"),layout(&mesh,"CabinShadows"));
    let smoke_bindings:std::collections::BTreeSet<_>=smoke.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(smoke_bindings,[(0,0),(0,1),(1,0),(1,1),(1,2),(1,3),(2,0),(3,0),(3,1),(3,2)].into_iter().collect());
    assert_eq!(layout(&smoke,"Smoke"),layout(&old,"Smoke"));
    assert_eq!(layout(&smoke,"Smoke").0,1840);
    assert_eq!(layout(&smoke,"PhysicalEnvironment"),layout(&sky,"PhysicalEnvironment"));
    assert_eq!(layout(&smoke,"PhysicalEnvironment").0,240);
    assert_eq!(layout(&smoke,"EnvironmentProbe").0,68*16);
}

#[test]
fn particle_pipeline_preserves_world_carrier_and_depth_exposure_bindings() {
    let source=include_str!("../shaders/pixel_particles.wgsl");
    let original=parse(&(structure(source,"ParticleUniform")+&structure(source,"Exposure")));
    let raster=parse(&with_observer(PARTICLES));
    assert_eq!(layout(&raster,"ParticleUniform"),layout(&original,"ParticleUniform"));
    assert_eq!(layout(&raster,"Exposure"),layout(&original,"Exposure"));
    assert_eq!(layout(&raster,"ParticleUniform").0,1392);
    let bindings:std::collections::BTreeSet<_>=raster.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(bindings,[(0,0),(0,1),(0,2)].into_iter().collect());
    assert!(PARTICLES.contains("textureLoad(depth_tex,pixel,0)"));
    assert!(PARTICLES.contains("world - u.camera.xyz"));
}

#[test]
fn optic_and_glow_shaders_validate_without_ray_queries() {
    for (name, source) in [("optic", include_str!("../shaders/raster_optics.wgsl")),
        ("glow",include_str!("../shaders/stylized_bloom.wgsl"))] {
        let module=parse(source);
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all() & !naga::valid::Capabilities::RAY_QUERY)
            .validate(&module).unwrap_or_else(|e|panic!("{name}: {e:?}"));
    }
}

#[test]
fn aperture_bloom_preserves_camera_abi_and_binds_real_scene_depth() {
    let module=parse(include_str!("../shaders/stylized_bloom.wgsl"));
    assert_eq!(layout(&module,"BloomCamera"),(56,vec![
        ("_before_exposure".into(),0),("exposure".into(),44),("_after_exposure".into(),48)]));
    assert_eq!(layout(&module,"BloomPortal"),(16,vec![("depth_and_view".into(),0)]));
    let bindings:std::collections::BTreeSet<_>=module.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(bindings,[(0,0),(0,1),(0,2),(0,3),(0,4),(0,5)].into_iter().collect());
    let depth=module.global_variables.iter().find(|(_,v)|v.name.as_deref()==Some("scene_depth")).unwrap().1;
    assert!(matches!(module.types[depth.ty].inner,naga::TypeInner::Image {
        dim:naga::ImageDimension::D2,arrayed:false,class:naga::ImageClass::Depth {multi:false}}));
}
