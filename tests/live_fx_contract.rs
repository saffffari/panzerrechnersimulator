//! Focused shipping-source contract for the live penetration, blood, fire and extinguisher paths.
//!
//! `src/game.rs` still carries an intentionally disabled `cfg(any())` migration archive. These
//! assertions select only named live functions/constants so the archive cannot make a retired pulse
//! or visual-only spark appear implemented.

const GAME: &str = include_str!("../src/game.rs");
const SURFACE_BLOOD: &str = include_str!("../src/surface_blood.rs");
const SURFACE_FIRE: &str = include_str!("../src/surface_fire.rs");

fn body_after(signature: &str, next_signature: &str) -> &'static str {
    source_body_after(GAME, signature, next_signature)
}

fn source_body_after(source: &'static str, signature: &str, next_signature: &str) -> &'static str {
    source.split(signature)
        .nth(1)
        .and_then(|tail| tail.split(next_signature).next())
        .unwrap_or_else(|| panic!("could not isolate {signature}"))
}

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn blood_has_dense_dark_wet_equal_cubes_and_contact_driven_stains_in_four_bounded_pages() {
    assert!(GAME.contains("const N_BLOOD_CUBES: usize = 8192;"));
    assert!(GAME.contains("const BLOOD_VISUAL_DENSITY: usize = 8;"));
    assert!(GAME.contains("const N_BLOOD_BATCHES: usize = 4;"));
    assert!(GAME.contains("const BLOOD_GRAIN_SIZE: f32 = surface_blood::GRAIN_M;"));
    assert!(SURFACE_BLOOD.contains("pub const GRAIN_M: f32 = 0.004;"));
    assert!(SURFACE_BLOOD.contains("pub const CELL_M: f32 = 0.005;"));
    assert!(SURFACE_BLOOD.contains("pub const MAX_CELLS: usize = 65_536;"));
    assert!(GAME.contains("const BLOOD_GRAINS_PER_BATCH: usize = N_BLOOD_CUBES / N_BLOOD_BATCHES;"));
    assert!(GAME.contains("let raw = 96.0 + (N_BLOOD_CUBES as f32 - 96.0) * smooth;"));
    assert!(compact(GAME).contains("recorded_count.saturating_mul(BLOOD_VISUAL_DENSITY).min(N_BLOOD_CUBES)"));

    let grain_mesh = body_after("fn blood_grain_mesh", "fn hot_fragment_mesh");
    assert!(grain_mesh.contains("box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), color)"));
    assert!(grain_mesh.contains("mesh.metallic_factor = 0.0;"));
    assert!(grain_mesh.contains("mesh.roughness_factor = 0.08;"));
    assert!(!grain_mesh.contains("set_emissive"));
    assert!(GAME.contains("let wet_blood = [[92, 5, 4], [112, 7, 5], [74, 3, 3], [101, 6, 4]];"));

    let draw = body_after("fn drive_cubic_blood", "fn drive_casings");
    assert!(draw.contains("self.cabin_fire.source_contacts(&models)"));
    assert!(draw.contains("self.surface_blood.update(dt, &models, &source)"));
    assert!(draw.contains("self.surface_blood.wet_models(&models)"));
    assert!(draw.contains("self.surface_blood.render_runs()"));
    assert!(draw.contains("run.model(&models)"));
    assert!(!draw.contains("integrate_blood_cube"));
    assert!(!GAME.contains("fn deposit_blood_stain("));
    let wet = source_body_after(SURFACE_BLOOD, "pub fn update", "fn grow");
    assert!(wet.contains("source.sweep_batch(&paths, models)"));
    assert!(wet.contains("emission.owner, emission.actor_id, emission.seed"));
    assert!(SURFACE_BLOOD.contains("source.supports_cell(frame, &corners)"));
    assert!(SURFACE_BLOOD.contains("source.supports_batch(frame, &spans)"));
    assert!(SURFACE_BLOOD.contains("self.cells.len() >= self.cell_limit"));
    assert!(draw.contains("blood instance pool exceeded its fixed capacity"));
    assert!(draw.contains("blood stain page exceeded its fixed capacity"));
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
        "emitting_area_m2 += area_m2",
        "source_radius: (emitting_area_m2 / (4.0 * std::f32::consts::PI)).sqrt()",
        "radiant_intensity: peak",
    ] {
        assert!(aggregate.contains(required), "aggregate lost {required}");
    }
    assert!(!aggregate.contains("camera"));
    assert!(!aggregate.contains("casualty"));
    // Source area defines luminous support: an empty gap between chips must not shorten a
    // shadow ray and let an otherwise occluded source illuminate through the cabin wall.
    assert!(!aggregate.contains("world_pos.distance(centroid) + cube_radius"));
    assert!(!aggregate.contains("HOT_FRAGMENT_RADIOMETRIC_SCALE"));

    let lights = body_after("fn drive_lights_with_transients", "fn update_haze");
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
    assert!(flame.contains("[0.008, 0.009, 0.010]"));
    assert!(GAME.contains("const FLAME_CUBES_PER_INTERIOR_SPOT: usize = 6144;"));
    assert!(
        compact(flame).contains("letsize=base_sizes[tier]*ifexterior&&height_t>=0.42{0.78}else{1.0};"),
        "interior flame cells keep their 8–10 mm discrete size; only exterior tiers may differ"
    );
    assert!(flame.contains("rotation: Quat::IDENTITY"));
    assert!(!flame.contains("Quat::from_rotation"));
    assert!(!flame.contains("sin()"));

    let soot = body_after("fn smoke_voxel_sample", "fn concussion_severity");
    assert!(soot.contains("let generation = clock.floor().max(0.0) as u32;"));
    assert!(soot.contains("let base_y = if exterior { 1.82 } else { 0.46 };"));
    assert!(soot.contains("[0.075, 0.092, 0.110]"));
    assert!(soot.contains("[0.0025, 0.003, 0.0035]"));
    assert!(GAME.contains("const MAX_INTERIOR_SMOKE_CUBES: usize = 12288;"));
    assert!(soot.contains("rotation: Quat::IDENTITY"));
    assert!(!soot.contains("Quat::from_rotation"));
    assert!(!soot.contains("sin()"));

    let fire_draw = body_after("fn drive_interior_fire", "// ── Death instant-replay");
    assert!(fire_draw.contains("Vec3::splat(sample.size)"));
    assert!(fire_draw.contains("sample.rotation"));
    assert!(fire_draw.contains("voxel-flame instance pool exceeded its fixed capacity"));
    assert!(fire_draw.contains("voxel-soot instance pool exceeded its fixed capacity"));
    assert!(fire_draw.contains("self.cabin_fire.patch_surface"));
    assert!(fire_draw.contains("interior_soot_budget(patches.len())"));
    assert!(fire_draw.contains("self.cabin_fire.clear_source_paths(source, &endpoints, &models)"));

    let foam_draw = body_after("fn drive_foam", "fn drive_penetration_fx");
    assert!(foam_draw.contains("Vec3::splat(FOAM_GRAIN_SIZE)"));
    assert!(foam_draw.contains("Quat::IDENTITY"));
    assert!(!foam_draw.contains("Quat::from_axis_angle"));
    assert!(foam_draw.contains("bounded surface foam pool"));
    assert!(foam_draw.contains("self.cabin_fire.advance_foam"));
    assert!(foam_draw.contains("self.cabin_fire.render_points"));
    assert!(SURFACE_FIRE.contains("pub const GRAIN_M: f32 = 0.008;"));
    assert!(SURFACE_FIRE.contains("pub const MAX_AIRBORNE: usize = 4096;"));
    assert!(SURFACE_FIRE.contains("pub const MAX_DEPOSITS: usize = 65536;"));

    // The separately exercised surface-fire model must own the shipping Game path. Keeping
    // its causal tests green is insufficient if Game silently returns to global F suppression.
    let advancing = source_body_after(SURFACE_FIRE, "pub fn advance_foam", "pub fn render_points");
    assert!(advancing.contains("self.sweep_inverse(f.position, next, &inverse)"));
    assert!(advancing.contains("self.deposit(hit)"));
    assert!(!advancing.contains("self.deposits.clear"));
    let deposition = source_body_after(SURFACE_FIRE, "fn deposit", "pub fn advance_foam");
    assert!(deposition.contains("self.deposits.len() >= MAX_DEPOSITS"));
    assert!(deposition.contains("if value.layer == 0"));
    assert!(deposition.contains("if *layer >= MAX_LAYERS"));
    let render = source_body_after(SURFACE_FIRE, "pub fn render_points", "pub fn ensure_anchors");
    assert!(compact(render).contains("models[p.frame].transform_point3(p.point+p.normal*(GRAIN_M*(p.layerasf32+0.5)))"));
    let suppression = source_body_after(SURFACE_FIRE, "pub fn update_fire", "#[cfg(test)]");
    assert!(suppression.contains("let coating = self.coverage_at(anchor)"));
    assert!(suppression.contains("self.scan_ammunition("));
    assert!(SURFACE_FIRE.contains("fn scan_ammunition("));
    let live_fire = compact(body_after("fn update_fire", "// ── ABSITZEN"));
    assert!(live_fire.contains("self.cabin_fire.update_fire("));
    assert!(live_fire.contains("self.ammo_fire.advance("));
    assert!(live_fire.contains("self.live_ammunition_in_frame(BloodFrame::Hull)"));
    assert!(live_fire.contains("self.kill_fire_cookoff(was_burning)"));
    assert!(!live_fire.contains("self.cabin_fire.reset()"), "extinction preserves deposited foam");
    let spray = compact(body_after("fn drive_ext_fx", "fn annunciator_emissives"));
    assert!(spray.contains("letaim_w=r.camera.pos+r.camera.forward()*2.5;"));
    assert!(spray.contains("self.cabin_fire.emit(nozzle_w,"));
    assert!(!spray.contains("INTERIOR_FIRE_SPOTS"));
    assert!(!GAME.contains("self.fire -= EXT_RATE"));
}
