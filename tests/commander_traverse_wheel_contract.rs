use std::fs;

#[test]
fn commander_traverse_flywheel_uses_its_fore_aft_shaft_and_historical_ratio() {
    let rig = fs::read_to_string("src/interior_rig.rs").expect("read interior rig");
    let game = fs::read_to_string("src/game.rs").expect("read game");

    assert!(
        rig.contains("pub const COMMANDER_TRAVERSE_WHEEL_AXIS: Vec3 = Vec3::new(0.0, 0.0, -1.0);")
    );
    assert!(rig.contains("pub const COMMANDER_TRAVERSE_TURNS_PER_TURRET_REV: f32 = 595.0;"));
    assert!(game.contains("rig::COMMANDER_TRAVERSE_WHEEL_AXIS"));
    assert!(game.contains("yaw * rig::COMMANDER_TRAVERSE_TURNS_PER_TURRET_REV"));
    assert!(!game.contains("Mat4::from_axis_angle(Vec3::Y, yaw * 30.0)"));
}
