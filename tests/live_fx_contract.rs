//! Focused shipping-source contract for the live penetration, blood, fire and extinguisher paths.
//!
//! `src/game.rs` still carries an intentionally disabled `cfg(any())` migration archive. These
//! assertions select only named live functions/constants so the archive cannot make a retired pulse
//! or visual-only spark appear implemented.

const GAME: &str = include_str!("../src/game.rs");

fn body_after(signature: &str, next_signature: &str) -> &'static str {
    GAME.split(signature)
        .nth(1)
        .and_then(|tail| tail.split(next_signature).next())
        .unwrap_or_else(|| panic!("could not isolate {signature}"))
}

#[test]
fn blood_is_one_thousand_twenty_four_dark_wet_equal_cubes_in_four_bounded_pages() {
    assert!(GAME.contains("const N_BLOOD_CUBES: usize = 1024;"));
    assert!(GAME.contains("const N_BLOOD_BATCHES: usize = 4;"));
    assert!(GAME.contains("const BLOOD_GRAIN_SIZE: f32 = 0.022;"));
    assert!(GAME.contains("const BLOOD_GRAINS_PER_BATCH: usize = N_BLOOD_CUBES / N_BLOOD_BATCHES;"));
    assert!(GAME.contains("let raw = 96.0 + (N_BLOOD_CUBES as f32 - 96.0) * smooth;"));

    let grain_mesh = body_after("fn blood_grain_mesh", "fn hot_fragment_mesh");
    assert!(grain_mesh.contains("box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), color)"));
    assert!(grain_mesh.contains("mesh.metallic_factor = 0.0;"));
    assert!(grain_mesh.contains("mesh.roughness_factor = 0.08;"));
    assert!(!grain_mesh.contains("set_emissive"));
    assert!(GAME.contains("let wet_blood = [[92, 5, 4], [112, 7, 5], [74, 3, 3], [101, 6, 4]];"));

    let draw = body_after("fn drive_cubic_blood", "fn drive_casings");
    assert!(draw.contains("for k in 0..N_BLOOD_CUBES"));
    assert!(draw.contains("Vec3::splat(cube.size)"));
    assert!(draw.contains("self.bake_blood_stain"));
    assert!(draw.contains("blood instance pool exceeded its fixed capacity"));
}

#[test]
fn live_fragment_heat_not_disabled_pulse_owns_long_decay_and_shadowed_emission() {
    let profile = body_after(
        "fn fragment_shower_profile",
        "fn generate_penetration_fragments",
    );
    assert!(
        profile.contains("fragment_life: 3.20 + energy * 1.65 + filler * 0.95 + caliber * 0.45")
    );

    let aggregate = body_after(
        "fn hot_fragment_aggregate_light",
        "fn micro_spark_heat_stage",
    );
    for required in [
        "fragment.active",
        "fragment.age < fragment.life",
        "6.0 * fragment.size.max(0.0).powi(2)",
        "weighted_pos / weight_sum",
        "world_pos.distance(centroid) + cube_radius",
        "peak * HOT_FRAGMENT_RADIOMETRIC_SCALE",
    ] {
        assert!(aggregate.contains(required), "aggregate lost {required}");
    }
    assert!(!aggregate.contains("camera"));
    assert!(!aggregate.contains("casualty"));

    let lights = body_after("fn drive_lights", "fn update_haze");
    assert!(lights.contains("hot_fragment_aggregate_light"));
    assert!(lights.contains("for frame in [BloodFrame::Hull, BloodFrame::Turret]"));
    assert!(lights.contains("lights.push(light)"));
    assert!(lights.contains("r.set_lights(&lights)"));

    let live_drive = body_after("fn drive_penetration_fx", "fn drive_penetration_fx_legacy");
    assert!(live_drive.contains("integrate_hot_fragment"));
    assert!(live_drive.contains("hot_fragment_heat_stage"));
    assert!(!live_drive.contains("penetration_light_envelope"));
    assert!(!live_drive.contains("PenetrationPulse"));
}

#[test]
fn fire_and_foam_are_axis_aligned_fixed_scale_cells_not_rotating_growing_prisms() {
    let flame = body_after("fn flame_voxel_sample", "fn smoke_voxel_sample");
    assert!(flame.contains("let generation = clock.floor().max(0.0) as u32;"));
    assert!(flame.contains("let layer ="));
    assert!(flame.contains("[0.160, 0.195, 0.230]"));
    assert!(flame.contains("[0.060, 0.072, 0.084]"));
    assert!(
        flame.contains("let size = base_sizes[tier] * if height_t < 0.42 { 1.0 } else { 0.78 };"),
        "lower flame-stack cells must keep the larger fixed cube size"
    );
    assert!(flame.contains("rotation: Quat::IDENTITY"));
    assert!(!flame.contains("Quat::from_rotation"));
    assert!(!flame.contains("sin()"));

    let soot = body_after("fn smoke_voxel_sample", "fn concussion_severity");
    assert!(soot.contains("let generation = clock.floor().max(0.0) as u32;"));
    assert!(soot.contains("let base_y = if exterior { 1.82 } else { 0.46 };"));
    assert!(soot.contains("[0.075, 0.092, 0.110]"));
    assert!(soot.contains("[0.020, 0.026, 0.032]"));
    assert!(soot.contains("rotation: Quat::IDENTITY"));
    assert!(!soot.contains("Quat::from_rotation"));
    assert!(!soot.contains("sin()"));

    let fire_draw = body_after("fn drive_interior_fire", "// ── Death instant-replay");
    assert!(fire_draw.contains("Vec3::splat(sample.size)"));
    assert!(fire_draw.contains("sample.rotation"));
    assert!(fire_draw.contains("voxel-flame instance pool exceeded its fixed capacity"));
    assert!(fire_draw.contains("voxel-soot instance pool exceeded its fixed capacity"));

    let foam_draw = body_after("fn drive_foam", "fn drive_penetration_fx");
    assert!(foam_draw.contains("Vec3::splat(scale)"));
    assert!(foam_draw.contains("Quat::IDENTITY"));
    assert!(!foam_draw.contains("Quat::from_axis_angle"));
    assert!(foam_draw.contains("foam instance pool exceeded its fixed capacity"));
}
