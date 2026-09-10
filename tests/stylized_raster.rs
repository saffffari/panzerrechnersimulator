//! CPU-only acceptance of the optional raster backend's complete shaders and existing GPU ABIs.
//! This establishes shader validity/resource compatibility; actual camera captures own visual QA.
use wgpu::naga;

const MESH: &str = include_str!("../shaders/stylized_mesh.wgsl");
const SKY: &str = include_str!("../shaders/stylized_sky.wgsl");
const SMOKE: &str = include_str!("../shaders/stylized_smoke.wgsl");
const PARTICLES: &str = include_str!("../shaders/stylized_particles.wgsl");

fn compose(source: &str) -> String {
    source.replace("// @camera-relative-projection",include_str!("../shaders/camera_relative.wgsl"))
        .replace("// @campaign-terrain-material",include_str!("../shaders/terrain_material.wgsl"))
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
    let mesh=parse(&compose(MESH));let sky=parse(SKY);
    assert_eq!(layout(&mesh,"Camera"),layout(&camera,"Camera"));
    assert_eq!(layout(&sky,"Camera"),layout(&camera,"Camera"));
    assert_eq!(layout(&mesh,"Camera").0,1296);
    assert_eq!(layout(&mesh,"Push").0,80);
    let bindings:std::collections::BTreeSet<_>=mesh.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(bindings,[(0,0),(0,1),(0,2),(1,0),(1,1),(1,2),(1,3),
        (2,0),(2,1),(2,2),(2,3),(2,4),(2,5),(3,0),(3,1)].into_iter().collect());
    assert_eq!(layout(&mesh,"NearShadow"),(80,vec![("view_proj".into(),0),("scale".into(),64)]));
    assert_eq!(layout(&mesh,"CabinShadows"),(48,vec![("lamps".into(),0),("range".into(),32)]));
    let old_smoke=include_str!("../shaders/smoke.wgsl");
    let old=parse(&(structure(old_smoke,"Puff")+&structure(old_smoke,"Smoke")));
    let smoke=parse(SMOKE);
    assert_eq!(layout(&smoke,"Smoke"),layout(&old,"Smoke"));
    assert_eq!(layout(&smoke,"Smoke").0,1616);
    assert_eq!(layout(&smoke,"PhysicalEnvironment").0,160);
    assert_eq!(layout(&sky,"PhysicalEnvironment").0,160);
}

#[test]
fn particle_pipeline_preserves_world_carrier_and_depth_exposure_bindings() {
    let source=include_str!("../shaders/pixel_particles.wgsl");
    let original=parse(&(structure(source,"ParticleUniform")+&structure(source,"Exposure")));
    let raster=parse(PARTICLES);
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
