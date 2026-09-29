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
const SKY: &str = include_str!("../shaders/stylized_sky.wgsl");
const SMOKE: &str = include_str!("../shaders/stylized_smoke.wgsl");
const PARTICLES: &str = include_str!("../shaders/stylized_particles.wgsl");

fn compose(source: &str) -> String {
    with_observer(source).replace("// @camera-relative-projection",include_str!("../shaders/camera_relative.wgsl"))
        .replace("// @campaign-terrain-material",include_str!("../shaders/terrain_material.wgsl"))
        .replace("// @solar-daylight",include_str!("../shaders/solar_daylight.wgsl"))
        .replace("// @sky-visibility-sampling",include_str!("../shaders/sky_visibility_sampling.wgsl"))
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
    for (name,source) in [("mesh",MESH),("sky",SKY),("smoke",SMOKE),("particles",PARTICLES),("ssao",include_str!("../shaders/raster_ssao.wgsl")),("outline",include_str!("../shaders/hover_outline.wgsl"))] {
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
}

#[test]
fn raster_camera_material_push_and_smoke_layouts_match_existing_pipeline() {
    let old_mesh=include_str!("../shaders/mesh.wgsl");
    let camera=parse(&structure(old_mesh,"Camera"));
    let mesh=parse(&compose(MESH));let sky=parse(&compose(SKY));
    assert_eq!(layout(&mesh,"Camera"),layout(&camera,"Camera"));
    assert_eq!(layout(&sky,"Camera"),layout(&camera,"Camera"));
    assert_eq!(layout(&mesh,"Camera").0,1296);
    assert_eq!(layout(&mesh,"Push").0,80);
    let bindings:std::collections::BTreeSet<_>=mesh.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(bindings,[(0,0),(0,1),(0,2),(1,0),(1,1),(1,2),(1,3),
        (2,0),(2,1),(2,2),(2,3),(2,4),(2,5),(2,6),(2,7),(3,0),(3,1)].into_iter().collect());
    assert_eq!(layout(&mesh,"NearShadow"),(80,vec![("view_proj".into(),0),("scale".into(),64)]));
    assert_eq!(layout(&mesh,"CabinShadows"),(48,vec![("lamps".into(),0),("range".into(),32)]));
    let old_smoke=include_str!("../shaders/smoke.wgsl");
    let old=parse(&(structure(old_smoke,"Puff")+&structure(old_smoke,"Smoke")));
    let smoke=parse(&compose(SMOKE));
    assert_eq!(layout(&smoke,"Smoke"),layout(&old,"Smoke"));
    assert_eq!(layout(&smoke,"Smoke").0,1616);
    assert_eq!(layout(&smoke,"PhysicalEnvironment").0,160);
    assert_eq!(layout(&sky,"PhysicalEnvironment").0,160);
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
