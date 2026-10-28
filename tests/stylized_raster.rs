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

fn compose(source: &str) -> String { compose_visibility(source,true) }
fn compose_visibility(source: &str, portable:bool) -> String {
    let visibility=|group,geometry,instances| if portable {visibility_shader(group,geometry,instances)}else{String::new()};
    let lighting=if portable {include_str!("../shaders/scene_light_visibility.wgsl")}else{include_str!("../shaders/shadow_map_visibility.wgsl")};
    let compare=if source.contains("// @scene-visibility-mesh") {"shadow_samp"}else{"sky_visibility_compare"};
    with_observer(source).replace("// @raster-particle-lighting",include_str!("../shaders/raster_particle_lighting.wgsl"))
        .replace("// @camera-relative-projection",include_str!("../shaders/camera_relative.wgsl"))
        .replace("// @campaign-terrain-material",include_str!("../shaders/terrain_material.wgsl"))
        .replace("// @sky-visibility-sampling",include_str!("../shaders/sky_visibility_sampling.wgsl"))
        .replace("// @cabin-visibility-sampling",include_str!("../shaders/cabin_visibility.wgsl"))
        .replace("// @weather-density",include_str!("../shaders/weather_volume.wgsl"))
        .replace("// @cabin-medium",include_str!("../shaders/cabin_medium.wgsl"))
        .replace("// @scene-visibility-mesh", &visibility(2,9,10))
        .replace("// @scene-visibility-smoke", &visibility(3,3,4))
        .replace("// @scene-visibility-particles", &visibility(2,3,4))
        .replace("// @scene-light-visibility",lighting)
        .replace("SHADOW_SKY_COMPARE",compare)
}
fn visibility_shader(group:u32,geometry:u32,instances:u32)->String {
    include_str!("../shaders/scene_visibility.wgsl").replace("// @environment-optical-path",include_str!("../shaders/scene_environment_visibility.wgsl")).replace("SV_GROUP",&group.to_string())
        .replace("SV_GEOMETRY_BINDING",&geometry.to_string()).replace("SV_INSTANCE_BINDING",&instances.to_string())
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
    assert_eq!(&layout(&mesh,"Camera").1[..layout(&camera,"Camera").1.len()],layout(&camera,"Camera").1.as_slice());
    assert_eq!(layout(&mesh,"Camera").0,1328+384+16);
    assert_eq!(layout(&mesh,"Push").0,80);
    let bindings:std::collections::BTreeSet<_>=mesh.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(bindings,[(0,0),(0,1),(0,2),(1,0),(1,1),(1,2),(1,3),
        (2,0),(2,1),(2,2),(2,3),(2,4),(2,5),(2,6),(2,7),(2,8),(2,9),(2,10),(2,11),(3,0),(3,1),(3,2)].into_iter().collect());
    // The probe the mesh reads is the probe the compute pass writes.
    assert_eq!(layout(&mesh,"EnvironmentProbe").0,68*16);
    assert_eq!(layout(&mesh,"NearShadow"),(80,vec![("view_proj".into(),0),("scale".into(),64)]));
    // One map for every admitted point emitter, plus exact emitter/torch metadata.
    assert_eq!(layout(&mesh,"CabinShadows"),(1072,vec![("lamps".into(),0),("projections".into(),512),("torch_position".into(),1024),("torch_direction".into(),1040),("range".into(),1056)]));
    let old_smoke=include_str!("../shaders/smoke.wgsl");
    let old=parse(&(structure(include_str!("../shaders/cabin_medium.wgsl"),"CabinMedium")+&structure(old_smoke,"Puff")+&structure(old_smoke,"Smoke")));
    let smoke=parse(&compose(SMOKE));
    assert_eq!(layout(&smoke,"CabinShadows"),layout(&mesh,"CabinShadows"));
    let smoke_bindings:std::collections::BTreeSet<_>=smoke.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(smoke_bindings,[(0,0),(0,1),(1,0),(1,1),(1,2),(1,3),(2,0),(2,1),(3,0),(3,1),(3,2),(3,3),(3,4)].into_iter().collect());
    assert_eq!(layout(&smoke,"Smoke"),layout(&old,"Smoke"));
    assert_eq!(layout(&smoke,"Smoke").0,1840+384);
    assert_eq!(layout(&smoke,"PhysicalEnvironment"),layout(&sky,"PhysicalEnvironment"));
    assert_eq!(layout(&smoke,"PhysicalEnvironment").0,240);
    assert_eq!(layout(&smoke,"EnvironmentProbe").0,68*16);
}

#[test]
fn particle_pipeline_preserves_world_carrier_and_depth_exposure_bindings() {
    let source=include_str!("../shaders/pixel_particles.wgsl");
    let original=parse(&(structure(include_str!("../shaders/cabin_medium.wgsl"),"CabinMedium")+&structure(source,"ParticleUniform")+&structure(source,"Exposure")));
    let raster=parse(&compose(PARTICLES));
    assert_eq!(layout(&raster,"ParticleUniform"),layout(&original,"ParticleUniform"));
    assert_eq!(layout(&raster,"Exposure"),layout(&original,"Exposure"));
    assert_eq!(layout(&raster,"ParticleUniform").0,1392+384+16);
    let bindings:std::collections::BTreeSet<_>=raster.global_variables.iter()
        .filter_map(|(_,v)|v.binding.as_ref().map(|b|(b.group,b.binding))).collect();
    assert_eq!(bindings,[(0,0),(0,1),(0,2),(0,3),(1,0),(1,1),(1,2),(1,3),(2,0),(2,1),(2,2),(2,3),(2,4)].into_iter().collect());
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
    // Group 1: the eye's veiling-glare pyramid (four levels and their sampler, fs_glare).
    assert_eq!(bindings,[(0,0),(0,1),(0,2),(0,3),(0,4),(0,5),(1,0),(1,1),(1,2),(1,3),(1,4)].into_iter().collect());
    let depth=module.global_variables.iter().find(|(_,v)|v.name.as_deref()==Some("scene_depth")).unwrap().1;
    assert!(matches!(module.types[depth.ty].inner,naga::TypeInner::Image {
        dim:naga::ImageDimension::D2,arrayed:false,class:naga::ImageClass::Depth {multi:false}}));
}

#[test]
fn point_depth_keeps_thin_armor_resolution_across_battle_distance() {
    let source=include_str!("../shaders/point_shadow.wgsl");let module=parse(source);
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::all())
        .validate(&module).unwrap();
    let far=512.0_f32;
    for range in [0.014_f32,4.0,140.0,220.0,500.0] {
        let plate=range/far;let receiver=(range+0.008)/far;
        assert!(receiver>plate,"8 mm receiver separation must survive at {range}m");
        assert!((plate*far-range).abs()<0.000062);
    }
    assert!(source.contains("in.face_distance/cam.far"));
    assert!(source.contains("linear*position+(model[3].xyz-cam.cam_pos)"));
    assert!(!source.contains("project(pc.model*vec4"));
    let host=include_str!("../src/cabin_shadows.rs");
    assert!(host.contains("face_view_proj_range(glam::Vec3::ZERO, face, (NEAR,far))"));
    assert!(host.contains("cam_pos: lamp.pos.to_array()"));
    // CPU culling remains in absolute world space; only GPU vertex arithmetic is relative.
    assert!(host.contains("let matrix = face_view_proj_range(lamp.pos, face, (NEAR,far));"));
    assert!(MESH.contains("cross(dpdx(in.relative_world), dpdy(in.relative_world))"));
    assert!(MESH.contains("in.relative_world + (cam.cam_pos - source.xyz)"));
    assert!(MESH.contains("max(0.00005,major*0.00000024)"));
}

#[test]
fn aerosol_and_particles_are_partitioned_by_geometry_not_by_camera_flags() {
    let medium=compose(SMOKE);let particles=compose(PARTICLES);
    assert!(medium.contains("fn cabin_next_span("));
    assert!(medium.contains("fog_moment.y/max(fog_moment.x,1e-8)"));
    assert!(!SMOKE.contains("sphere_interval(origin,ray,sm.atmo_origin,3.35)"));
    assert!(!PARTICLES.contains("near_coverage"));
    assert!(!PARTICLES.contains("vec3(0.18,0.17,0.15)"));
    assert!(!PARTICLES.contains("vec3(0.80,0.83,0.84)"));
    assert!(particles.contains("cabin_volume_light_shadow(source.xyz,source.w,point)"));
    assert!(particles.contains("sky_tile_visibility("));
}

#[test]
fn local_smoke_native_pixel_coverage_preserves_mean_transport_without_frame_noise() {
    fn alpha(tau:f64,plume:f64,threshold:f64)->f64 {
        let value=1.0-(-tau.max(0.0)).exp();
        if plume<=0.000001||tau<=0.000001{return value;}
        let quantum=0.25*(plume/tau).clamp(0.0,1.0);
        let lower=(value/quantum).floor()*quantum;
        let upper=(lower+quantum).min(1.0);
        if threshold<(value-lower)/(upper-lower).max(0.000001) {upper}else{lower}
    }
    for tau in [0.0_f64,0.00001,0.01,0.10,0.40,1.0,3.0,10.0] {
        for plume_share in [0.0,0.1,0.5,1.0] {
            let expected=1.0-(-tau).exp();
            let n=65536;
            let mean=(0..n).map(|i|alpha(tau,tau*plume_share,(i as f64+0.5)/n as f64)).sum::<f64>()/n as f64;
            assert!((mean-expected).abs()<0.25/n as f64);
            // Premultiplied source and transmittance use the SAME coverage sample, preserving
            // the expected radiance of any HDR source/background, not just alpha in isolation.
            for (source,background) in [(0.01,12.0),(40.0,0.2),(1.0,1.0)] {
                let sampled=source*mean+background*(1.0-mean);
                let reference=source*expected+background*(1.0-expected);
                assert!((sampled-reference).abs()<(source-background).abs()*0.25/n as f64+1e-12);
            }
        }
    }
    let helper=SMOKE.split("fn plume_pixel_threshold").nth(1).unwrap().split("struct VsOut").next().unwrap();
    assert!(!helper.contains("frame")&&!helper.contains("time"));
    assert!(SMOKE.contains("plume_pixel_alpha(depth_moment.x,plume_depth,in.pos.xy)"));
    assert!(SMOKE.contains("let transmittance=1.0-local_alpha;"));
    // One threshold per native fragment; no fixed output-resolution tile, quad or frame cadence.
    assert!(helper.contains("floor(pixel)"));
}

#[test]
fn default_geometry_map_shaders_validate_and_preserve_shared_visibility() {
    for (name,source) in [("mesh",MESH),("smoke",SMOKE),("particles",PARTICLES)] {
        let source=compose_visibility(source,false);let module=parse(&source);
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::all()).validate(&module).unwrap_or_else(|e|panic!("{name}: {e:?}"));
        assert!(!source.contains("fn scene_visibility_trace("),"portable traversal must not compile into default");
        assert!(source.contains("map_direction_visibility"));
        assert!(source.contains("if(lamp<0){scene_visibility_unresolved=true"));
        assert_eq!(layout(&module,"SkyVisibility").0,4784);
    }
}

#[test]
fn shaded_and_hover_geometry_share_invariant_vertex_clip_positions() {
    let module=parse(&compose(MESH));
    for entry in module.entry_points.iter().filter(|e|e.name=="vs"||e.name=="vs_inst") {
        let result=entry.function.result.as_ref().unwrap();
        let naga::TypeInner::Struct{members,..}=&module.types[result.ty].inner else{panic!("vertex output must retain source varyings");};
        assert!(members.iter().any(|member|matches!(member.binding.as_ref(),
            Some(naga::Binding::BuiltIn(naga::BuiltIn::Position{invariant:true})))),
            "{} must not drift against the re-rasterized selection depth",entry.name);
    }
    assert!(MESH.contains("@fragment fn fs_hover()"));
}
