//! Numerical/ABI checks for the camera-oriented display dither. Native clips own visual QA.
#[allow(dead_code)]
#[path = "../src/observer.rs"]
mod observer;
use glam::{DVec3, DVec2};
use wgpu::naga;

#[test]
fn complete_crush_shader_validates_and_preserves_eye_offsets() {
    let source=include_str!("../shaders/crush.wgsl")
        .replace("// @observer-constants",&observer::wgsl_constants());
    let module=naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e|panic!("{}",e.emit_to_string(&source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::all())
        .validate(&module).unwrap();
    let (_,ty)=module.types.iter().find(|(_,ty)|ty.name.as_deref()==Some("Crush")).unwrap();
    let naga::TypeInner::Struct {members,span}=&ty.inner else {panic!("Crush is not a struct")};
    assert_eq!(*span,128);
    for (name,offset) in [("rod_exposure",16),("rod_inverse_saturation",20),("exposure",44),
        ("dither_pattern",56),("display_grade",64),("dither_right",80),("dither_up",96),("dither_forward",112)] {
        assert_eq!(members.iter().find(|m|m.name.as_deref()==Some(name)).unwrap().offset,offset);
    }
}

fn rank(x:i32,y:i32)->u32 {
    let (x,y)=((x&7)as u32,(y&7)as u32);
    let mut out=0;
    for bit in 0..3 {out=(out<<2)|((((x>>bit)^(y>>bit))&1)<<1)|((y>>bit)&1);}
    out
}
fn threshold(ray:DVec3,height:f64,fov:f64)->f64 {
    let density=height/(2.0*(fov*0.5).tan());
    let row=((ray.y.clamp(-1.0,1.0).asin()+std::f64::consts::FRAC_PI_2)*density).floor();
    let latitude=(row+0.5)/density-std::f64::consts::FRAC_PI_2;
    let columns=((std::f64::consts::TAU*density*latitude.cos().max(0.0)/8.0).round()*8.0).max(8.0);
    let column=((ray.x.atan2(-ray.z)+std::f64::consts::PI)/std::f64::consts::TAU*columns).floor();
    (rank(column as i32,row as i32)as f64+0.5)/64.0-0.5
}
#[test]
fn ordered_tile_covers_every_threshold_once_with_zero_bias() {
    let mut ranks=(0..8).flat_map(|y|(0..8).map(move |x|rank(x,y))).collect::<Vec<_>>();
    ranks.sort_unstable();assert_eq!(ranks,(0..64).collect::<Vec<_>>());
    assert_eq!(ranks.iter().map(|r|(*r as f64+0.5)/64.0-0.5).sum::<f64>(),0.0);
    for y in -16..16 {assert_eq!(rank(-1,y),rank(7,y));assert_eq!(rank(0,y),rank(8,y));}
}
#[test]
fn rotating_camera_retains_same_world_direction_pattern_at_both_resolutions() {
    let fov=60_f64.to_radians();
    let directions=[DVec3::new(0.173,0.113,-1.0).normalize(),DVec3::new(-0.197,-0.091,-1.0).normalize()];
    for height in [360.0,540.0] {
        for world in directions {
            let expected=threshold(world,height,fov);
            for yaw in [-0.30_f64,-0.11,0.0,0.14,0.31] {
                for pitch in [-0.10_f64,0.0,0.10] {
                    let forward=DVec3::new(yaw.sin()*pitch.cos(),pitch.sin(),-yaw.cos()*pitch.cos());
                    let right=forward.cross(DVec3::Y).normalize();let up=right.cross(forward);
                    let z=world.dot(forward);
                    let q=DVec2::new(world.dot(right)/z,world.dot(up)/z);
                    let reconstructed=(forward+right*q.x+up*q.y).normalize();
                    assert!(reconstructed.distance(world)<1e-12);
                    assert_eq!(threshold(reconstructed,height,fov),expected);
                }
            }
        }
        for ray in [DVec3::Y,-DVec3::Y,DVec3::Z,-DVec3::Z] {
            assert!(threshold(ray,height,fov).is_finite());
        }
        // Centre angular dot footprint remains approximately one native pixel.
        let density=height/(2.0*(fov*0.5).tan());
        let pixels=(1.0/density).tan()*height/(2.0*(fov*0.5).tan());
        assert!((pixels-1.0).abs()<0.00002);
    }
}
