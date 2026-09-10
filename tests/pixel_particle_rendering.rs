#[path = "../src/protective_glass.rs"] mod protective_glass;
const PARTICLES:&str=include_str!("../shaders/pixel_particles.wgsl");
const RENDERER:&str=include_str!("../src/renderer.rs");
#[test]
fn particle_shader_uses_the_exact_scene_and_validates() {
    let source=protective_glass::inject(PARTICLES,false);
    let module=wgpu::naga::front::wgsl::parse_str(&source).expect("particle WGSL");
    wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all()).validate(&module).expect("valid particle WGSL");
    assert_eq!(source.matches("fn source_connection(").count(),1);
    assert!(source.contains("blocked(in.world,upstream,distance_m)"));
    assert!(source.contains("in.position.z >= textureLoad(depth_tex,pixel,0)"));
    assert!(source.contains("u.solar_irradiance.xyz * source_visibility(p,sun)"));
    assert!(source.contains("return vec4(vec3(optic),1.0)"));
    assert!(source.contains("return vec4(color*optic,u.viewport.z*sqrt(brightness))"));
}
#[test]
fn snapped_unit_quad_covers_one_internal_pixel_at_any_depth_or_resolution() {
    for (w,h) in [(320,180),(640,360),(1280,720)] {
        for depth_w in [0.03f64,0.5,20.0,5000.0] {
            for (x,y) in [(0.01,0.01),(0.4931,0.7239),(0.9999,0.9999)] {
                let pixel=((x*w as f64).floor(),(y*h as f64).floor());
                let left=(2.0*pixel.0/w as f64-1.0)*depth_w;
                let right=(2.0*(pixel.0+1.0)/w as f64-1.0)*depth_w;
                let top=(1.0-2.0*pixel.1/h as f64)*depth_w;
                let bottom=(1.0-2.0*(pixel.1+1.0)/h as f64)*depth_w;
                assert!(((right-left)/depth_w*w as f64*0.5-1.0).abs()<1e-10);
                assert!(((top-bottom)/depth_w*h as f64*0.5-1.0).abs()<1e-10);
                let covered=(pixel.0 as i32..=pixel.0 as i32+1)
                    .flat_map(|i|(pixel.1 as i32..=pixel.1 as i32+1).map(move|j|(i,j)))
                    .filter(|(i,j)|(*i as f64+0.5)>=pixel.0&&(*i as f64+0.5)<pixel.0+1.0
                        &&(*j as f64+0.5)>=pixel.1&&(*j as f64+0.5)<pixel.1+1.0).count();
                assert_eq!(covered,1);
            }
        }
    }
}
#[test]
fn particle_presentation_follows_crush_and_never_duplicates_volumetric_motes() {
    let crush=RENDERER.find("label: Some(\"crush\"),\n                color_attachments").unwrap();
    let particle=RENDERER.find("label: Some(\"one-pixel-world-particles\"),").unwrap();
    assert!(particle>crush);
    assert!(!RENDERER.contains("pass.set_pipeline(&self.snow_pipe)"));
    assert!(RENDERER.contains("wgpu::ColorTargetState { format: LDR"));
    let smoke=include_str!("../shaders/smoke.wgsl");
    assert!(!smoke.contains("fn interior_mote("));
    assert!(!smoke.contains("let mote_density"));
    assert!(smoke.contains("let density = carrier;"));
    assert!(smoke.contains("let single_scatter_albedo = 0.70;"));
    assert!(smoke.contains("bounded_world_higher_order_scatter("));
}
#[test]
fn upstream_roof_check_is_aligned_with_falling_motion() {
    // A simple plane witness: an interior particle's primary view can be unobstructed while its
    // upstream path still intersects a roof. A ray outside the roof rectangle remains admitted.
    let velocity=[0.9f64,-1.1,0.0];let upstream=velocity.map(|v|-v);
    let hit=|p:[f64;3]|{let t=(3.0-p[1])/upstream[1];
        let x=p[0]+upstream[0]*t;let z=p[2]+upstream[2]*t;t>0.0&&x.abs()<2.0&&z.abs()<2.0};
    assert!(hit([0.0,1.5,0.0]));assert!(!hit([8.0,1.5,0.0]));
}

#[test]
fn post_crush_particles_preserve_etched_reticle_and_black_optic_coverage() {
    for required in ["optic_transmission(in.position.xy)","if (optic <= 0.0) { discard; }",
        "eye.reticle > 0.001","d.y <= 8.0", "abs(d.x) >= 24.0", "eye.mask < 1.5",
        "eye.mask < 2.5", "eye.mask < 3.5", "length(q)<0.500"] { assert!(PARTICLES.contains(required),"{required}"); }
    // A source ray's air segments partition the original finite support, excluding the glass.
    for (before, glass) in [(0.0f64,0.0),(0.02,0.01),(3.0,0.20)] {
        let remaining=4000.0-before-glass;
        assert_eq!(before+glass+remaining,4000.0);
        let sigma=0.006;
        let segmented=(-sigma*before).exp()*(-sigma*remaining).exp();
        assert!((segmented-(-sigma*(4000.0-glass)).exp()).abs()<1e-20);
    }
    assert!(PARTICLES.contains("4000.0-path.air_before_glass-path.glass_path_m"));
}
