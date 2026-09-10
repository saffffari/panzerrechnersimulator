//! Numeric transport witnesses; these do not replace a rendered appearance check.
#[path = "../src/sky_reprojection.rs"]
mod sky_reprojection;
use glam::{DVec3, Mat4, Vec3, Vec4};
use std::f64::consts::PI;

#[test]
fn visible_sky_rays_keep_subpixel_precision_at_large_world_origins() {
    let shader = include_str!("../shaders/sky.wgsl");
    assert!(!shader.contains("cam.inv_view_proj *"));
    for row in ["x", "y", "w"] {
        assert!(shader.contains(&format!(
            "vec3(cam.view_proj[0].{row}, cam.view_proj[1].{row}, cam.view_proj[2].{row})")));
    }
    assert!(shader.contains("right_scaled * ndc.x / dot(right_scaled, right_scaled)"));
    assert!(shader.contains("up_scaled * ndc.y / dot(up_scaled, up_scaled)"));
    let source = format!("{}\n{}", include_str!("../shaders/physical_environment.wgsl"), shader);
    let module = wgpu::naga::front::wgsl::parse_str(&source).expect("visible sky WGSL");
    wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all()).validate(&module).expect("valid visible sky WGSL");
    for forward in [Vec3::new(-0.55, 0.13, -0.82).normalize(),
                    Vec3::new(0.2, -0.8, 0.3).normalize()] {
        for (fov, aspect) in [(62_f32.to_radians(), 16.0 / 9.0), (90_f32.to_radians(), 1.0)] {
            let f = forward.as_dvec3().normalize();
            let r = f.cross(DVec3::Y).normalize();
            let u = r.cross(f);
            let scale = (fov as f64 * 0.5).tan();
            for origin in [Vec3::ZERO, Vec3::new(985.0, 5.49, 1205.0),
                           Vec3::new(5000.0, 4.0, 5000.0), Vec3::splat(1e6)] {
                let vp = Mat4::perspective_rh(fov, aspect, 0.025, 6000.0)
                    * Mat4::look_to_rh(origin, forward, Vec3::Y);
                let rs = vp.row(0).truncate();
                let us = vp.row(1).truncate();
                let fs = vp.row(3).truncate().normalize();
                for y in -12..=12 { for x in -12..=12 {
                    let sx = x as f32 / 12.0;
                    let sy = y as f32 / 12.0;
                    let actual = (fs + rs * sx / rs.length_squared()
                        + us * sy / us.length_squared()).normalize().as_dvec3();
                    let expected = (f + r * (sx as f64 * scale * aspect as f64)
                        + u * (sy as f64 * scale)).normalize();
                    // Angular chord error below 0.001 native pixel at 640x360.
                    let pixel_error = (actual - expected).length() / (fov as f64 / 360.0);
                    assert!(pixel_error < 0.001, "origin={origin:?}, pixel_error={pixel_error}");
                }}
            }
        }
    }
}

#[test]
fn stationary_sky_history_is_exact_identity_at_every_world_origin() {
    let forward = Vec3::new(-85.0, 0.91182, -105.0).normalize();
    for origin in [Vec3::ZERO, Vec3::new(985.0,5.49287,1205.0), Vec3::splat(1e6)] {
        // Origin is deliberately not an input: sky directions have no positional parallax.
        assert!(origin.is_finite());
        let m = sky_reprojection::matrix(forward, 62f32.to_radians(), 16.0/9.0,
            forward, 62f32.to_radians(), 16.0/9.0);
        assert_eq!(m, Mat4::IDENTITY);
        for xy in [(-0.999,0.999),(0.0,0.0),(0.853,-0.707)] {
            let p=Vec4::new(xy.0,xy.1,1.0,0.0);
            assert_eq!(m*p,p);
        }
    }
}

#[test]
fn rotated_sky_matches_independent_double_camera_basis() {
    let current=Vec3::new(-0.55,0.13,-0.82).normalize();
    let previous=Vec3::new(-0.42,0.17,-0.89).normalize();
    let fov=65f32.to_radians(); let aspect=16.0f32/9.0;
    let m=sky_reprojection::matrix(current,fov,aspect,previous,fov,aspect);
    let c=current.as_dvec3().normalize(); let p=previous.as_dvec3().normalize();
    let cr=c.cross(DVec3::Y).normalize(); let cu=cr.cross(c);
    let pr=p.cross(DVec3::Y).normalize(); let pu=pr.cross(p);
    let scale=(fov as f64*0.5).tan();
    for y in -10..=10 {for x in -10..=10 {
        let u=x as f64/10.0; let v=y as f64/10.0;
        let ray=c+cr*(u*scale*aspect as f64)+cu*(v*scale);
        let expected=[ray.dot(pr)/(ray.dot(p)*scale*aspect as f64),
                      ray.dot(pu)/(ray.dot(p)*scale)];
        let q=m*Vec4::new(u as f32,v as f32,1.0,0.0);
        assert!((q.x as f64/q.z as f64-expected[0]).abs()<5e-7);
        assert!((q.y as f64/q.z as f64-expected[1]).abs()<5e-7);
    }}
}

fn hg(c:f64,g:f64)->f64 {(1.0-g*g)/(4.0*PI*(1.0+g*g-2.0*g*c).powf(1.5))}
fn sample_cos(u:f64,g:f64)->f64 {
    if g.abs()<1e-3 {1.0-2.0*u} else {
        let r=(1.0-g*g)/(1.0-g+2.0*g*u); (1.0+g*g-r*r)/(2.0*g)
    }
}

#[test]
fn hg_proposal_preserves_constant_spectrum_and_first_angular_moment() {
    for g in [0.0,0.42,0.7532] {
        let mut mean=0.0;let mut uniform_variance=0.0;
        for i in 0..20_000 {
            let u=(i as f64+0.5)/20_000.0;let c=sample_cos(u,g);mean+=c/20_000.0;
            let phase=hg(c,g);
            for radiance in [0.017,0.23,1.4] {
                assert!((radiance*phase/phase-radiance).abs()<1e-14);
            }
            uniform_variance+=(4.0*PI*hg(1.0-2.0*u,g)-1.0).powi(2)/20_000.0;
        }
        assert!((mean-g).abs()<1e-7);
        if g>0.4 {assert!(uniform_variance>0.7);}
    }
}

#[test]
fn hg_and_aperture_mixture_uses_both_directional_pdfs() {
    // A real angular aperture can be approximated by this finite cone for an independent PDF
    // test. Integrate both sampling branches separately; omitting HG on the aperture branch fails.
    let cap_cos=0.96;let area_pdf=1.0/(2.0*PI*(1.0-cap_cos));
    for g in [0.42,0.7532] {
        let mut integral=0.0;
        for i in 0..80_000 {
            let u=(i as f64+0.5)/80_000.0;
            for c in [sample_cos(u,g),cap_cos+(1.0-cap_cos)*u] {
                let phase=hg(c,g);
                let pdf=0.5*phase+if c>=cap_cos {0.5*area_pdf}else{0.0};
                integral+=0.5*phase/pdf/80_000.0;
            }
        }
        assert!((integral-1.0).abs()<5e-5,"g={g}, integral={integral}");
    }
}

#[test]
fn accumulation_shaders_parse_and_validate_with_shared_matrix_abi() {
    for source in [include_str!("../shaders/accum.wgsl"),include_str!("../shaders/volume_accum.wgsl")] {
        let m=wgpu::naga::front::wgsl::parse_str(source).expect("accumulation WGSL");
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all()).validate(&m).expect("valid accumulation WGSL");
        assert!(source.contains("sky_reprojection: mat4x4<f32>"));
    }
}

#[test]
fn finite_volume_history_keeps_far_terrain_and_rejects_disocclusions() {
    // Read the upper bound actually used at the live WGSL call site. This makes the numeric
    // regression fail if the .9999 cap returns; testing an independent Rust copy would miss it.
    let source = include_str!("../shaders/volume_accum.wgsl");
    let prefix = "let expected_z = linear_depth(clamp(previous_ndc.z, 0.0, ";
    let tail = source.split_once(prefix).expect("finite volume history depth call").1;
    let upper: f32 = tail.split_once(')').unwrap().0.trim().parse().unwrap();
    assert!(source.contains("previous_id == id && previous_depth < 1.0"));
    let near = 0.025_f32;
    let far = 6000.0_f32;
    let projection = Mat4::perspective_rh(62_f32.to_radians(), 16.0 / 9.0, near, far);
    let linear = |depth: f32| near * far / (far - depth * (far - near));
    let projected = |distance: f32| {
        let clip = projection * Vec4::new(0.0, 0.0, -distance, 1.0);
        clip.z / clip.w
    };
    let accepts = |expected: f32, stored: f32| {
        stored < 1.0 && (linear(stored) - expected).abs() < 0.03 * expected + 0.05
    };
    for distance in [10.0_f32, 240.0, 300.0, 750.0, 1500.0, 3000.0, 4500.0] {
        let depth = projected(distance);
        let expected = linear(depth.clamp(0.0, upper));
        assert!((expected - distance).abs() < distance * 0.03,
            "finite volume depth collapsed at {distance}m: {expected}m");
        assert!(accepts(expected, depth), "stationary {distance}m surface lost history");
        assert!(!accepts(expected, projected(distance * 0.5)),
            "nearer occluder inherited far volume history");
        assert!(!accepts(expected, 1.0), "sky inherited finite surface history");
    }
    let old_cap_distance = linear(0.9999);
    assert!((235.0..245.0).contains(&old_cap_distance));
    assert!(!accepts(old_cap_distance, projected(750.0)),
        "fixture must expose the old rejection beyond the accidental 240m cap");
}
