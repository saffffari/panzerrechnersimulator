//! CPU validation for native-grid display dither; no device or running-game changes.
#[allow(dead_code)]
#[path = "../src/observer.rs"]
mod observer;
use wgpu::naga;
const SOURCE: &str=include_str!("../shaders/crush.wgsl");
#[test]
fn all_display_modes_validate_with_the_existing_eye_abi() {
    let source=SOURCE.replacen("// @observer-constants",&observer::wgsl_constants(),1);
    let module=naga::front::wgsl::parse_str(&source).unwrap_or_else(|e|panic!("{}",e.emit_to_string(&source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::empty()).validate(&module).unwrap();
    let (_,ty)=module.types.iter().find(|(_,t)|t.name.as_deref()==Some("Crush")).unwrap();
    let naga::TypeInner::Struct{members,span}= &ty.inner else{panic!("Crush structure")};
    assert_eq!(*span,128);
    for (name,offset) in [("exposure",44),("display_grade",64),("dither_right",80),("dither_up",96),("dither_forward",112)] {
        assert_eq!(members.iter().find(|m|m.name.as_deref()==Some(name)).unwrap().offset,offset);
    }
}
#[test]
fn screen_bayer8_uses_one_native_threshold_and_never_the_sphere_resolve() {
    let native=SOURCE.split_once("fn display_threshold").unwrap().1.split_once("struct VsOut").unwrap().0;
    assert!(native.contains("if(cr.dither_pattern>2.5)"));
    assert!(native.contains("bayer8_rank(px)"));
    assert!(!native.contains("sphere_threshold")&&!native.contains("cr.frame"));
    let fragment=SOURCE.split_once("@fragment").unwrap().1;
    assert_eq!(fragment.matches("display_threshold(").count(),1);
    assert!(fragment.contains("display_threshold(vec2<i32>(in.pos.xy))"));
    assert!(fragment.contains("let ordered = native_threshold"));
    assert!(fragment.contains("let threshold = native_threshold"));
    assert_eq!(fragment.matches("if (cr.dither_pattern > 1.5 && cr.dither_pattern < 2.5)").count(),2);
    let blit=include_str!("../shaders/blit.wgsl");
    assert!(!blit.contains("dither")||blit.contains("pass-through"));
}
