//! Slow-motion cross sections of the layered damage model: `fea-cross-section PREFIX [CASE]`.
//! CPU only: the baked Tiger meshes load through the CPU glTF reader and no wgpu device is created.
//! Each case fires a real round of the game's catalogue at a point of the Tiger, finds the struck
//! plate and the far wall on the same source triangles the game uses, runs the same chain and voxel
//! lattice the game runs, flies the lattice's fragments with drag, and draws frames about 1/1000 of
//! real time apart: the vehicle cut by the vertical plane of the shot, the components in section
//! coloured by system, the round, the fragments by temperature, the lattice around the hole, the
//! compartment pressure, and the systems knocked out. docs/FEA_DAMAGE_MODEL_20261102.md, "Stage 5".
//!
//! Section-view simplifications (named; the game does not use them): fragments fly ballistically
//! with drag until they leave the cabin proxy box, with no bounces drawn; all 64 hull cradles hold a
//! round; the fuel tanks are 80 % full; turret forward, gun level.
use super::*;
use crate::fea_damage::section::{heat_colour, rgb, Svg};
use crate::fea_damage::{chain, compartment, components, fragments, lattice};

/// The 14.5 mm B-32 round of the PTRD-41 anti-tank rifle, for the case set only (no anti-tank rifle
/// fires in the game yet): 64 g at 1,012 m/s; about 25 mm at 100 m against plate at 30 degrees
/// (estimate from published PTRD data); drag fitted to a published 800 m/s at 500 m (estimate).
/// Its incendiary composition is not modelled.
pub(crate) static B32_PTRD: crate::ballistics::ShellSpec = crate::ballistics::ShellSpec {
    name: "14.5mm B-32 (PTRD-41)",
    kind: crate::ballistics::ShellKind::Ap,
    caliber_mm: 14.5,
    mass_kg: 0.064,
    muzzle_velocity: 1012.0,
    filler_g: 0.0,
    drag_k: 4.7e-4,
    pen_ref_mm: 25.0,
    ref_range_m: 100.0,
};

pub(crate) struct SectionCase {
    pub name: &'static str,
    pub title: &'static str,
    pub shell: &'static crate::ballistics::ShellSpec,
    pub range_m: f32,
    /// A point inside the Tiger the shot is aimed through (hull frame) and its travel direction.
    pub aim: Vec3,
    pub dir: Vec3,
}

pub(crate) fn cases() -> Vec<SectionCase> {
    use crate::ballistics as b;
    vec![
        SectionCase { name: "front_lower_76apcr", title: "76 mm BR-354P APCR, 100 m, front lower plate, Tiger climbing a 15 deg slope at the gun",
            shell: &b::BR_354P, range_m: 100.0, aim: Vec3::new(-0.04, 1.0, 2.0), dir: Vec3::new(0.0, -0.27, -1.0) },
        SectionCase { name: "front_lower_45ap", title: "45 mm BR-240 (M-42) AP, 100 m, front lower plate",
            shell: &b::BR_240_M42, range_m: 100.0, aim: Vec3::new(-0.04, 1.0, 2.0), dir: Vec3::new(0.0, -0.03, -1.0) },
        SectionCase { name: "side_fuel_45apcr", title: "45 mm BR-240P APCR, 100 m, left side at the fuel tank",
            shell: &b::BR_240P, range_m: 100.0, aim: Vec3::new(1.148, 1.633, -1.609), dir: Vec3::new(-1.0, -0.02, 0.1) },
        SectionCase { name: "side_final_drive_45apcr", title: "45 mm BR-240P APCR, 100 m, lower hull side at the left final drive",
            shell: &b::BR_240P, range_m: 100.0, aim: Vec3::new(1.232, 0.98, 3.0), dir: Vec3::new(-1.0, -0.02, 0.0) },
        SectionCase { name: "side_fuel_57aphe", title: "57 mm BR-271 APHE, 500 m, left side at the fuel tank",
            shell: &b::BR_271_ZIS2, range_m: 500.0, aim: Vec3::new(1.148, 1.633, -1.609), dir: Vec3::new(-1.0, -0.02, 0.1) },
        SectionCase { name: "front_76apbc", title: "76 mm BR-350B APBC, 500 m, front plate at the radio operator",
            shell: &b::BR_350B, range_m: 500.0, aim: Vec3::new(-0.35, 1.75, 1.8), dir: Vec3::new(0.0, -0.02, -1.0) },
        SectionCase { name: "front_85aphe", title: "85 mm BR-365 APHE, 300 m, front plate at the radio operator",
            shell: &b::BR_365, range_m: 300.0, aim: Vec3::new(-0.35, 1.75, 1.8), dir: Vec3::new(0.0, -0.02, -1.0) },
        SectionCase { name: "front_122ap", title: "122 mm BR-471 AP, 1000 m, front plate at the radio operator",
            shell: &b::BR_471, range_m: 1000.0, aim: Vec3::new(-0.35, 1.75, 1.8), dir: Vec3::new(0.0, -0.02, -1.0) },
        SectionCase { name: "turret_front_100aphe", title: "100 mm BR-412 APHE, 500 m, turret front",
            shell: &b::BR_412, range_m: 500.0, aim: Vec3::new(0.3, 2.3, 0.6), dir: Vec3::new(0.0, -0.02, -1.0) },
        SectionCase { name: "side_ammo_76apcr", title: "76 mm BR-354P APCR, 200 m, left sponson at the ammunition",
            shell: &b::BR_354P, range_m: 200.0, aim: Vec3::new(1.35, 1.66, 0.61), dir: Vec3::new(-1.0, -0.03, 0.0) },
        SectionCase { name: "side_crew_152aphe", title: "152 mm BR-540 APHE (660 g), 500 m, left sponson into the fighting compartment",
            shell: &b::BR_540, range_m: 500.0, aim: Vec3::new(1.35, 1.66, 0.61), dir: Vec3::new(-1.0, -0.03, 0.0) },
        SectionCase { name: "side_14mm_ptrd", title: "14.5 mm B-32 (PTRD-41), 100 m, upper hull side",
            shell: &B32_PTRD, range_m: 100.0, aim: Vec3::new(1.5, 1.6, 0.4), dir: Vec3::new(-1.0, -0.02, 0.0) },
        SectionCase { name: "glacis_ricochet_76", title: "76 mm BR-350B APBC, 300 m, grazing the glacis (ricochet)",
            shell: &b::BR_350B, range_m: 300.0, aim: Vec3::new(0.3, 1.47, 2.9), dir: Vec3::new(0.0, -0.06, -1.0) },
        SectionCase { name: "engine_deck_76aphe", title: "76 mm BR-350B APBC from above and behind, engine deck",
            shell: &b::BR_350B, range_m: 300.0, aim: Vec3::new(0.0, 1.9, -2.0), dir: Vec3::new(0.0, -0.75, 0.66) },
        SectionCase { name: "roof_he_76", title: "76 mm OF-350 HE on the engine deck",
            shell: &b::OF_350_F34, range_m: 300.0, aim: Vec3::new(0.0, 1.9, -2.0), dir: Vec3::new(0.0, -0.75, 0.66) },
    ]
}

struct Rig {
    hull: crate::gltf_load::CpuMesh,
    turret: crate::gltf_load::CpuMesh,
    gun: crate::gltf_load::CpuMesh,
    source: surface_fire::CabinFire,
    tank: Tank,
    models: [Mat4; surface_fire::FRAMES],
    rounds: Vec<chain::RoundBox>,
}

fn rig() -> Rig {
    let [hull, turret, gun] = crate::tank::build_tiger();
    let hf = crate::terrain::Heightfield { res: 2, spacing: 1000.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0,
        openings: Default::default(), detail: None };
    let tank = Tank::new(0.0, 0.0, 0.0, &hf, crate::tank::TIGER_PARTS);
    // The game's armour-only cache for shell contacts holds the hull and turret source triangles.
    let source = surface_fire::CabinFire::new(&[(0, &hull), (1, &turret)]);
    let mut models = [Mat4::IDENTITY; surface_fire::FRAMES];
    models[0] = tank.hull_model();
    models[1] = tank.turret_model();
    models[2] = tank.gun_model();
    let hull_inv = tank.hull_model().inverse();
    let radius = crate::procedural_ordnance::LIVE_MAX_DIAMETER_M * 0.5;
    let half = Vec3::new(radius, radius, crate::procedural_ordnance::LIVE_LENGTH_M * 0.5);
    let rounds = crate::interior_rig::SHELL_SLOTS.iter().enumerate().map(|(index, (frame, position, q))| {
        let parent = if *frame == crate::interior_rig::Frame::Hull { tank.hull_model() } else { tank.turret_model() };
        let rotation = Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize();
        let (_, r, c) = (hull_inv * parent * Mat4::from_rotation_translation(rotation, *position)).to_scale_rotation_translation();
        chain::RoundBox { location_id: crate::interior_rig::shell_source_id(index), center: c,
            axes: [r * Vec3::X, r * Vec3::Y, r * Vec3::Z], half }
    }).collect();
    Rig { hull, turret, gun, source, tank, models, rounds }
}

/// Lattice state kept for one frame: the section slice of the plate (plate frame).
struct LatticeFrame {
    t: f32,
    points: Vec<(Vec3, f32, f32)>,
    tip_s: f32,
}

struct FlyingFragment {
    pos: Vec3,
    vel: Vec3,
    size: f32,
    temp_k: f32,
    born_s: f32,
}

/// Where review media go: with `KESSEL_CAPTURES_DIR` set (the owner's shared folder), there, flat,
/// prefixed with this checkout's folder name as the capture gallery expects; otherwise as given.
pub(crate) fn shared_capture_prefix(prefix: &str) -> String {
    let Ok(dir) = std::env::var("KESSEL_CAPTURES_DIR") else { return prefix.to_string() };
    // `capture_target` (main.rs) already routes bare and `captures/...` names into the shared
    // folder with the checkout's name in front; such a prefix stands as it is.
    if std::path::Path::new(prefix).starts_with(&dir) {
        return prefix.to_string();
    }
    let stem = std::path::Path::new(prefix).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "fea".into());
    let worktree = std::env::current_dir().ok().and_then(|d| d.file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_else(|| "checkout".into());
    let _ = std::fs::create_dir_all(&dir);
    std::path::Path::new(&dir).join(format!("{worktree}-{stem}")).to_string_lossy().to_string()
}

pub(crate) fn run(prefix: &str, only: Option<&str>) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let prefix_owned = shared_capture_prefix(prefix);
    let prefix = prefix_owned.as_str();
    let rig = rig();
    println!("[fea-section] Tiger meshes loaded ({:.1} s)", started.elapsed().as_secs_f32());
    let mut receipts = Vec::new();
    for case in cases() {
        if only.is_some_and(|name| !case.name.starts_with(name)) {
            continue;
        }
        let receipt = run_case(&rig, &case, prefix)?;
        receipts.push(receipt);
    }
    let path = format!("{prefix}_receipt.json");
    std::fs::write(&path, serde_json::to_string_pretty(&serde_json::json!({
        "tool": "fea-cross-section",
        "doc": "docs/FEA_DAMAGE_MODEL_20261102.md",
        "slow_motion": "about 1/1000 of real time between frames",
        "cases": receipts,
    }))?)?;
    println!("[fea-section] wrote {path} ({:.1} s)", started.elapsed().as_secs_f32());
    Ok(())
}

fn run_case(rig: &Rig, case: &SectionCase, out_prefix: &str) -> anyhow::Result<serde_json::Value> {
    // Frames are intermediate: they go to a temporary folder (kept with KESSEL_FEA_KEEP_FRAMES=1);
    // the contact sheet, the slow-motion clip and the receipt go to `out_prefix` (captures, flat).
    let keep_frames = std::env::var("KESSEL_FEA_KEEP_FRAMES").is_ok();
    let frame_dir = std::env::temp_dir().join(format!("kessel_fea_frames_{}_{}", std::process::id(), case.name));
    std::fs::create_dir_all(&frame_dir)?;
    let stem = std::path::Path::new(out_prefix).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "fea".into());
    let prefix_owned = if keep_frames { out_prefix.to_string() } else { frame_dir.join(&stem).to_string_lossy().to_string() };
    let prefix = prefix_owned.as_str();
    let clock = std::time::Instant::now();
    let stamp = |what: &str| println!("[fea-section]   {what}: {:.2} s", clock.elapsed().as_secs_f32());
    let hull = rig.tank.hull_model();
    let hull_inv = hull.inverse();
    let dir = case.dir.normalize();
    let dir_w = hull.transform_vector3(dir);
    let aim_w = hull.transform_point3(case.aim);
    let contact = projectile_armor_source::contact(&rig.source, &rig.models, &rig.tank, ProjectileVehicle::Player,
        crate::ballistics::ArmorScheme::TigerI, aim_w - dir_w * 12.0, aim_w + dir_w * 2.0)
        .ok_or_else(|| anyhow::anyhow!("{}: the shot line meets no armour", case.name))?;
    let ProjectileContactKind::Vehicle(armor) = contact.kind else { anyhow::bail!("{}: not a vehicle contact", case.name) };
    let plate = armor.surface_plate.map(|s| s.plate).unwrap_or_else(|| crate::ballistics::ArmorScheme::TigerI.plate(armor.facing));
    let horizontal = (-dir_w).dot(armor.world_normal).abs().clamp(0.0, 1.0).acos().to_degrees();
    let mut event = crate::ballistics::resolve(case.shell, case.range_m, plate, horizontal, 0x5EC7_10C5);
    if let Some(r) = crate::ballistics::ricochet::response(&event, dir_w * event.striking_v, armor.world_normal, 0) {
        event.terminal = crate::ballistics::Terminal::Ricochet;
        event.residual_v = r.velocity.length();
        event.behind_armor = 0.0;
    }
    let line = game_fea::shot_line(&rig.source, &rig.models, &rig.tank, contact.pos, dir_w, &event);
    let state = chain::VehicleState {
        articulation: components::Articulation::rest(crate::tank::TIGER_PARTS.turret_pivot),
        fuel_fill: 0.8,
        gravity_hull: Vec3::NEG_Y,
        live_rounds: &rig.rounds,
    };
    let kinetic = matches!(case.shell.kind, crate::ballistics::ShellKind::Ap | crate::ballistics::ShellKind::Apcbc
        | crate::ballistics::ShellKind::Apcr);
    let chain_result = kinetic.then(|| {
        let layers = chain::build_layers(&line, components::tiger_components(), &state);
        chain::run(&event, &layers, crate::fea_damage::materials::AIR.density)
    });
    println!("[fea-section] {}: {} on {} ({:.0} mm at {:.0} deg): {:?}, pen {:.0} / eff {:.0} mm", case.name,
        case.shell.name, plate.name, plate.thickness_mm, event.impact_angle_deg, event.terminal, event.pen_mm, event.effective_mm);
    if let Some(r) = &chain_result {
        println!("[fea-section]   {}", r.summary());
    }
    // The lattice, with snapshots of the section slice through the shot.
    let normal_hull = hull_inv.transform_vector3(armor.world_normal).normalize_or_zero();
    let seed = crate::armor_damage::position_seed(armor.local_point) ^ 0x5EC7;
    let mut lattice_frames: Vec<LatticeFrame> = Vec::new();
    let mut outcome = None;
    let mut solver_dims = None;
    if let Some(r) = &chain_result {
        if let Some((impact, axes)) = game_fea::lattice_problem(&event, &line, r, normal_hull, seed) {
            let t0 = std::time::Instant::now();
            let mut solver = lattice::Lattice::new(impact);
            let frame_dt = 25.0e-6f32;
            let mut next_t = 0.0f32;
            loop {
                if solver.t >= next_t - 0.5 * solver.dt {
                    let h = solver.h;
                    let points = (0..solver.n).filter_map(|p| {
                        let pos = solver.position(p);
                        (pos.y.abs() <= 0.75 * h).then(|| (pos, solver.temp[p], solver.broken_fraction(p)))
                    }).collect();
                    lattice_frames.push(LatticeFrame { t: solver.t, points, tip_s: solver.tip_s });
                    next_t += frame_dt;
                }
                if solver.advance(((next_t - solver.t) / solver.dt).ceil().max(1.0) as u64) {
                    break;
                }
            }
            let h = solver.h;
            let points = (0..solver.n).filter_map(|p| {
                let pos = solver.position(p);
                (pos.y.abs() <= 0.75 * h).then(|| (pos, solver.temp[p], solver.broken_fraction(p)))
            }).collect();
            lattice_frames.push(LatticeFrame { t: solver.t, points, tip_s: solver.tip_s });
            let wall_ms = t0.elapsed().as_secs_f32() * 1000.0;
            solver_dims = Some((solver.h, solver.dims, solver.origin, impact.thickness_m));
            outcome = Some(game_fea::lattice_outcome(&mut solver, line.entry, axes, wall_ms));
        }
    }
    stamp("chain and lattice");
    // The round's timeline along its path: piecewise by layer at the mean of its in and out speeds.
    let mut timeline: Vec<(f32, f32)> = vec![(0.0, 0.0)];
    let mut end_s = 0.0f32;
    if let Some(r) = &chain_result {
        let mut t = 0.0f32;
        for c in &r.crossings {
            let span = match c.fate {
                chain::Fate::Stopped { depth_m } => depth_m,
                _ => c.layer.s1 - c.layer.s0,
            };
            let v = 0.5 * (c.v_in + c.v_out).max(1.0);
            t += span / v;
            end_s = c.layer.s0 + span;
            timeline.push((t, end_s));
        }
        if let Some(v) = r.exit_speed {
            timeline.push((t + 2.0 / v.max(1.0), end_s + 2.0));
        }
    } else {
        timeline.push((1.0e-4, line.plate_path_m * 0.2));
    }
    let shell_at = |t: f32| -> Option<f32> {
        if t < 0.0 { return Some(t * event.striking_v); }
        for w in timeline.windows(2) {
            if t <= w[1].0 {
                let f = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-9);
                return Some(w[0].1 + (w[1].1 - w[0].1) * f);
            }
        }
        // Stopped rounds stay; rounds that left are drawn at the exit.
        timeline.last().map(|l| l.1)
    };
    // Fragments: the lattice's interior clusters (and a burst body) from the end of the solve.
    let mut flying: Vec<FlyingFragment> = Vec::new();
    let mut fragment_mass = 0.0f32;
    let mut fragment_ke = 0.0f32;
    if let Some(o) = &outcome {
        let t_born = lattice_frames.last().map_or(0.0, |f| f.t);
        for c in o.clusters.iter().filter(|c| c.leaves_inside(o.thickness_m, o.cell_mm * 0.001, o.measured.perforated)) {
            let v = o.plate_dir_to_hull(c.velocity);
            fragment_mass += c.mass_kg;
            fragment_ke += 0.5 * c.mass_kg * v.length_squared();
            flying.push(FlyingFragment { pos: o.plate_to_hull(c.centre), vel: v, size: c.size_m, temp_k: c.temperature_k, born_s: t_born });
        }
    }
    let burst = chain_result.as_ref().is_some_and(|r| r.burst);
    let mut pressure = None;
    // The burst's time on the frame clock (the round leaving the plate's back face).
    let mut burst_at = 0.0f32;
    if burst {
        let profile = fragment_shower_profile(true, event.behind_armor, event.shell.caliber_mm, event.residual_v, event.shell.filler_g);
        let body_mass = (event.shell.mass_kg - event.shell.filler_g * 0.001).max(0.0);
        let count = 400usize;
        let uniforms: Vec<f32> = (0..count).map(|i| hash01(0xB0D7_0000 ^ (i as u32).wrapping_mul(2_654_435_761) ^ 0xE1)).collect();
        let sizes = fragments::body_partition(body_mass, count, profile.min_size_m, profile.max_size_m, &uniforms);
        let burst_point = line.entry + line.dir * line.plate_path_m;
        let t_burst = timeline.get(1).map_or(0.0, |p| p.0);
        let mut side = line.dir.cross(Vec3::Y).normalize_or_zero();
        if side.length_squared() < 0.5 { side = line.dir.cross(Vec3::X).normalize_or_zero(); }
        let up = side.cross(line.dir).normalize_or_zero();
        let mut body_ke = 0.0f32;
        for (i, size) in sizes.iter().enumerate() {
            let s = 0xB0D7_0000u32 ^ (i as u32).wrapping_mul(2_654_435_761);
            let radial = hash01(s ^ 0x51).sqrt();
            let angle = hash01(s ^ 0xA7) * std::f32::consts::TAU;
            let fan = (side * angle.cos() + up * angle.sin()) * radial * profile.cone_half_tan;
            let speed = profile.speed_min + (profile.speed_max - profile.speed_min) * hash01(s ^ 0xD3).sqrt();
            let v = (line.dir + fan).normalize_or_zero() * speed;
            body_ke += 0.5 * fragments::cube_mass(*size) * v.length_squared();
            flying.push(FlyingFragment { pos: burst_point, vel: v, size: *size, temp_k: 0.0, born_s: t_burst });
        }
        fragment_mass += body_mass;
        fragment_ke += body_ke;
        // The game's rule (`game_fea`): every lid closed, the solved hole open; the filler's energy
        // less what the round's own body carries away (the plate's clusters took theirs from the
        // round's motion, not the filler).
        let hole_area = outcome.as_ref().map_or(0.0, |o| super::game_fea::hole_area_m2(&o.geometry));
        let openings = super::game_fea::crew_openings([0.0; 4], 0.0, hole_area);
        // A burst in the engine bay gets no crew compartment pressure (the game's rule).
        if super::game_fea::in_crew_air(armor.part, burst_point) {
            pressure = Some(compartment::burst(event.shell.filler_g * 0.001, compartment::TNT_J_PER_KG, body_ke,
                compartment::TIGER_CREW_AIR_M3, crate::armor_damage::AMBIENT_K, &openings,
                compartment::GAME_DT_S, compartment::GAME_DURATION_S));
            burst_at = t_burst;
        }
    }
    // Frame times: every 25 µs through the round's passage and the lattice, then the fragments.
    let solve_end = lattice_frames.last().map_or(0.0, |f| f.t).max(timeline.get(1).map_or(0.0, |p| p.0));
    let mut times: Vec<f32> = Vec::new();
    let mut t = -100.0e-6;
    while t <= solve_end + 1e-9 {
        times.push(t);
        t += 25.0e-6;
    }
    let round_end = timeline.last().map_or(0.0, |p| p.0).min(0.04);
    for ms in [1.0f32, 1.5, 2.0, 3.0, 4.0, 5.0, 7.0, 10.0, 14.0, 20.0, 28.0, 40.0] {
        let t = ms * 1e-3;
        if t > solve_end && (ms <= 10.0 || t <= round_end + 4.0e-3) {
            times.push(t);
        }
    }
    // The section plane: vertical through the shot (or along the hull for a near-vertical shot).
    let up = Vec3::Y;
    let mut a1 = (dir - up * dir.dot(up)).normalize_or_zero();
    if a1.length_squared() < 0.5 {
        a1 = Vec3::Z;
    }
    let plane_normal = a1.cross(up).normalize_or_zero();
    let origin = line.entry;
    let mut segments = Vec::new();
    crate::fea_damage::section::slice_mesh(&rig.hull, Mat4::IDENTITY, origin, plane_normal, a1, up, &mut segments);
    crate::fea_damage::section::slice_mesh(&rig.turret, hull_inv * rig.tank.turret_model(), origin, plane_normal, a1, up, &mut segments);
    crate::fea_damage::section::slice_mesh(&rig.gun, hull_inv * rig.tank.gun_model(), origin, plane_normal, a1, up, &mut segments);
    // Components in section.
    let set = components::tiger_components();
    let art = components::Articulation::rest(crate::tank::TIGER_PARTS.turret_pivot);
    let hits: Vec<(usize, bool, bool)> = chain_result.as_ref().map_or(Vec::new(),
        |r| r.hits.iter().map(|h| (h.component, h.breached, h.stopped)).collect());
    let mut cells: Vec<(usize, (f32, f32), f32)> = Vec::new();
    for env in &set.envelopes {
        let to_hull = |p: Vec3| match env.frame {
            components::Frame::Hull => p,
            components::Frame::Turret | components::Frame::Gun => p + crate::tank::TIGER_PARTS.turret_pivot,
        };
        for ix in 0..env.dims[0] as i64 {
            for iy in 0..env.dims[1] as i64 {
                for iz in 0..env.dims[2] as i64 {
                    if !env.occupied(ix, iy, iz) {
                        continue;
                    }
                    let c = to_hull(env.origin + (Vec3::new(ix as f32, iy as f32, iz as f32) + 0.5) * env.cell);
                    let _ = art;
                    if (c - origin).dot(plane_normal).abs() <= 0.5 * env.cell {
                        cells.push((env.spec, ((c - origin).dot(a1), (c - origin).dot(up)), env.cell));
                    }
                }
            }
        }
    }
    stamp("sections and component cells");
    // Drawing bounds from the hull section.
    let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    for (a, b) in &segments {
        for p in [a, b] {
            lo = (lo.0.min(p.0), lo.1.min(p.1));
            hi = (hi.0.max(p.0), hi.1.max(p.1));
        }
    }
    let view = (40.0f32, 70.0f32, 1300.0f32, 760.0f32);
    let scale = ((view.2 - 40.0) / (hi.0 - lo.0).max(0.5)).min((view.3 - 40.0) / (hi.1 - lo.1).max(0.5));
    let to_px = |p: (f32, f32)| (view.0 + 20.0 + (p.0 - lo.0) * scale, view.1 + view.3 - 20.0 - (p.1 - lo.1) * scale);
    let project = |p: Vec3| ((p - origin).dot(a1), (p - origin).dot(up));
    let mut knocked: Vec<String> = Vec::new();
    for h in hits.iter().filter(|h| h.1) {
        if let Some(m) = game_fea::module_for_system(components::COMPONENT_SPECS[h.0].system) {
            if !knocked.iter().any(|k| k == m.name()) {
                knocked.push(m.name().to_string());
            }
        }
    }
    let rounds_struck = chain_result.as_ref().map_or(0, |r| r.crossings.iter()
        .filter(|c| matches!(c.layer.kind, chain::LayerKind::LiveRound { .. })).count());
    let mut frame_files = Vec::new();
    let png = which_rsvg();
    // The static layer (section, components, labels) once per case; each frame draws over it.
    let static_name = format!("{prefix}_{}_static", case.name);
    {
        let mut svg = Svg::new(1920.0, 1080.0, [16, 16, 20]);
        svg.text(40.0, 40.0, 22.0, "#e8e8e8", case.title);
        svg.rect(view.0, view.1, view.2, view.3, "none", "#30333a", 1.0, 1.0);
        for &(spec, p, cell) in &cells {
            let c = system_colour(components::COMPONENT_SPECS[spec].system);
            let q = to_px(p);
            let s = cell * scale;
            svg.rect(q.0 - 0.5 * s, q.1 - 0.5 * s, s, s, &rgb(c), "none", 0.0, 0.85);
        }
        for (a, b) in &segments {
            svg.line(to_px(*a), to_px(*b), "#9aa0aa", 0.7, false);
        }
        for env in &set.envelopes {
            let pts: Vec<&(usize, (f32, f32), f32)> = cells.iter().filter(|c| c.0 == env.spec).collect();
            if pts.len() < 4 {
                continue;
            }
            let cx = pts.iter().map(|c| c.1 .0).sum::<f32>() / pts.len() as f32;
            let cy = pts.iter().map(|c| c.1 .1).sum::<f32>() / pts.len() as f32;
            let q = to_px((cx, cy));
            svg.text(q.0 - 30.0, q.1, 11.0, "#d0d0d0", components::COMPONENT_SPECS[env.spec].name);
        }
        std::fs::write(format!("{static_name}.svg"), svg.finish())?;
        if let Some(tool) = &png {
            let _ = std::process::Command::new(tool).args(["-o", &format!("{static_name}.png"), &format!("{static_name}.svg")]).status();
        }
    }
    let static_href = std::path::Path::new(&format!("{static_name}.png")).file_name()
        .map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    // Component outlines for the struck and breached, as the round reaches them.
    let mut outlines: Vec<(usize, (f32, f32), (f32, f32))> = Vec::new();
    for h in &hits {
        let pts: Vec<&(usize, (f32, f32), f32)> = cells.iter().filter(|c| c.0 == h.0).collect();
        if pts.is_empty() { continue; }
        let (mut l, mut u) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for c in &pts {
            l = (l.0.min(c.1 .0 - 0.5 * c.2), l.1.min(c.1 .1 - 0.5 * c.2));
            u = (u.0.max(c.1 .0 + 0.5 * c.2), u.1.max(c.1 .1 + 0.5 * c.2));
        }
        outlines.push((h.0, l, u));
    }
    for (fi, &t) in times.iter().enumerate() {
        let mut svg = Svg::new(1920.0, 1080.0, [16, 16, 20]);
        svg.image(&static_href, 1920.0, 1080.0);
        for &(spec, l, u) in &outlines {
            let Some(h) = hits.iter().find(|h| h.0 == spec) else { continue };
            if !shell_at(t).is_some_and(|s_now| s_now >= first_entry(&chain_result, spec)) {
                continue;
            }
            let (a, b) = (to_px((l.0, u.1)), to_px((u.0, l.1)));
            let colour = if h.1 { "#ff3b30" } else { "#ffb020" };
            svg.rect(a.0, a.1, b.0 - a.0, b.1 - a.1, "none", colour, 2.5, 1.0);
            svg.text(a.0, a.1 - 6.0, 12.0, colour, if h.1 { "breached" } else { "struck, casing held" });
        }
        // The shot line and the round.
        let a = to_px(project(line.entry - line.dir * 2.0));
        let b = to_px(project(line.entry + line.dir * end_s.max(line.plate_path_m)));
        svg.line(a, b, "#6a7a8a", 1.0, true);
        if let Some(s_now) = shell_at(t) {
            let d = event.shell.caliber_mm * 0.001;
            let tip = line.entry + line.dir * s_now;
            let tail = tip - line.dir * (4.0 * d);
            let side = plane_normal.cross(line.dir).normalize_or_zero() * (0.5 * d);
            let poly = [project(tip), project(tail + line.dir * 3.0 * d + side), project(tail + side),
                project(tail - side), project(tail + line.dir * 3.0 * d - side)].map(to_px);
            svg.polygon(&poly, "#b8c4d0", "#ffffff", 0.6);
        }
        // Fragments.
        for f in &flying {
            let age = t - f.born_s;
            if age < 0.0 {
                continue;
            }
            let (pos, temp, _) = fly(rig, f, age);
            let off = (pos - origin).dot(plane_normal).abs();
            let opacity = (1.0 - off / 0.6).clamp(0.15, 1.0);
            let c = if f.temp_k > 0.0 { heat_colour(temp, [170, 170, 175]) } else { [240, 170, 60] };
            svg.circle(to_px(project(pos)), (f.size * scale).max(1.6), &rgb(c), opacity);
        }
        // The lattice inset around the hole.
        let inset = (1380.0f32, 70.0f32, 500.0f32, 360.0f32);
        svg.rect(inset.0, inset.1, inset.2, inset.3, "#0c0c10", "#30333a", 1.0, 1.0);
        if let (Some((h, dims, lorigin, thick)), Some(lf)) = (solver_dims, lattice_frames.iter().rev().find(|f| f.t <= t + 1e-9).or(lattice_frames.first())) {
            let width_m = dims[0] as f32 * h;
            let s = ((inset.2 - 20.0) / width_m.max(1e-3)).min((inset.3 - 90.0) / (2.2 * thick).max(1e-3));
            let ox = inset.0 + 0.5 * inset.2 - (lorigin.x - 0.5 * h + 0.5 * width_m) * s;
            let oz = inset.1 + 0.5 * inset.3 - 0.5 * thick * s;
            for (p, temp, broken) in &lf.points {
                let c = heat_colour(*temp, [150, 150, 155]).map(|v| (v as f32 * (1.0 - 0.6 * broken)) as u8);
                let x = ox + p.x * s;
                let y = oz + p.z * s;
                if x >= inset.0 && x <= inset.0 + inset.2 && y >= inset.1 && y <= inset.1 + inset.3 {
                    svg.rect(x - 0.5 * h * s, y - 0.5 * h * s, h * s, h * s, &rgb(c), "none", 0.0, 1.0);
                }
            }
            svg.text(inset.0 + 8.0, inset.1 + inset.3 - 10.0, 12.0, "#a0a0a0",
                &format!("voxel lattice, {:.1} mm cells, t = {:.0} us (section through the shot)", h * 1000.0, lf.t * 1e6));
            // The struck face, the back face and the shot's direction in the plate frame.
            svg.text(inset.0 + 8.0, oz - 6.0, 11.0, "#c0c0c0", "struck face");
            svg.text(inset.0 + 8.0, oz + thick * s + 14.0, 11.0, "#c0c0c0", "back face");
            let ob = event.impact_angle_deg.to_radians();
            let (ax, az) = (inset.0 + 0.5 * inset.2, oz - 40.0);
            svg.line((ax - 30.0 * ob.sin(), az - 30.0 * ob.cos()), (ax, az), "#ffd060", 2.0, false);
            let _ = lf.tip_s;
        } else {
            svg.text(inset.0 + 12.0, inset.1 + 30.0, 13.0, "#a0a0a0", "no lattice solve for this terminal (empirical scar)");
        }
        // Text panel.
        let mut y = 470.0;
        let mut line_text = |svg: &mut Svg, text: &str, colour: &str| {
            svg.text(1380.0, y, 14.0, colour, text);
            y += 20.0;
        };
        line_text(&mut svg, &format!("t = {:.0} us after the strike (slow motion, about 1/1000)", t * 1e6), "#ffffff");
        line_text(&mut svg, &format!("{} at {:.0} m: {:.0} m/s striking", case.shell.name, case.range_m, event.striking_v), "#d8d8d8");
        line_text(&mut svg, &format!("{}: {:.0} mm at {:.0} deg; pen {:.0} / eff {:.0} mm: {:?}",
            plate.name, plate.thickness_mm, event.impact_angle_deg, event.pen_mm, event.effective_mm, event.terminal), "#d8d8d8");
        if let Some(r) = &chain_result {
            for c in r.crossings.iter().take(10) {
                let what = layer_name(c.layer.kind);
                line_text(&mut svg, &format!("  {what}: {:.0} -> {:.0} m/s, {:?}", c.v_in, c.v_out, short_fate(c.fate)), "#b8c0c8");
            }
            if let Some(l) = r.lodged {
                line_text(&mut svg, &format!("  lodged {:.0} mm into its layer", l.depth_m * 1000.0), "#ffd080");
            }
            if r.burst {
                line_text(&mut svg, "  the filler bursts behind the plate", "#ffb060");
            }
        }
        if rounds_struck > 0 {
            line_text(&mut svg, &format!("stowed rounds struck: {rounds_struck} (in play the existing rule initiates them)"), "#ff6b5b");
        }
        let shown: Vec<&String> = knocked.iter().collect();
        line_text(&mut svg, &format!("knocked out: {}", if shown.is_empty() { "nothing".to_string() } else {
            shown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ") }), if shown.is_empty() { "#a0d0a0" } else { "#ff6b5b" });
        if let Some(o) = &outcome {
            line_text(&mut svg, &format!("lattice: {} particles, {} steps, {:.0} ms wall, energy closure {:.2} %",
                o.particles, o.steps, o.wall_ms, o.ledger.closure() * 100.0), "#909aa4");
            line_text(&mut svg, &format!("hole {:.0} mm, petals {}, plug {}, lip {:.0} mm, bulge {:.0} mm, rim {:.0} K",
                o.geometry.aperture_mm, o.geometry.petals, o.geometry.plug, o.geometry.lip_mm, o.geometry.bulge_mm, o.geometry.rim_kelvin), "#909aa4");
        }
        line_text(&mut svg, &format!("fragments: {} ({:.2} kg, {:.0} kJ)", flying.len(), fragment_mass, fragment_ke / 1000.0), "#909aa4");
        if let Some(p) = &pressure {
            let since = t - burst_at;
            let now = if since < 0.0 { 0.0 } else { p.samples.iter().rev().find(|s| s.0 <= since).map_or(0.0, |s| s.1) };
            line_text(&mut svg, &format!("compartment: {:.1} kPa now, peak {:.1} kPa{}", now / 1000.0, p.peak_gauge_pa / 1000.0,
                if p.hatches_blown.is_empty() { String::new() } else { format!(", blown: {}", p.hatches_blown.iter().map(|h| h.0).collect::<Vec<_>>().join(", ")) }), "#c0a0ff");
        }
        let file = format!("{prefix}_{}_f{fi:03}.svg", case.name);
        std::fs::write(&file, svg.finish())?;
        if let Some(tool) = &png {
            let out = file.replace(".svg", ".png");
            let _ = std::process::Command::new(tool).args(["-o", &out, &file]).status();
        }
        frame_files.push(file);
    }
    stamp("frames");
    // Components the fragments came to rest in by the last frame (what the game's fragment
    // forecast would test against its module volumes; reported, not applied here).
    let mut fragment_struck: Vec<&'static str> = Vec::new();
    if let Some(&last) = times.last() {
        for f in &flying {
            if last <= f.born_s { continue; }
            if let (_, _, Some(spec)) = fly(rig, f, last - f.born_s) {
                let id = components::COMPONENT_SPECS[spec].id;
                if !fragment_struck.contains(&id) { fragment_struck.push(id); }
            }
        }
    }
    if png.is_some() {
        // A contact sheet of six frames and a slow-motion clip (12 frames a second), media for review.
        let pngs: Vec<String> = frame_files.iter().map(|f| f.replace(".svg", ".png")).collect();
        let pick: Vec<&String> = (0..6).map(|k| &pngs[(k * (pngs.len() - 1)) / 5]).collect();
        let sheet = format!("{out_prefix}_{}_sheet.png", case.name);
        let mut args: Vec<String> = pick.iter().map(|s| s.to_string()).collect();
        args.extend(["-tile".into(), "3x2".into(), "-geometry".into(), "960x540+4+4".into(), "-background".into(), "#101014".into(), sheet.clone()]);
        let _ = std::process::Command::new("montage").args(&args).status();
        let pattern = format!("{prefix}_{}_f%03d.png", case.name);
        let _ = std::process::Command::new("ffmpeg").args(["-y", "-loglevel", "error", "-framerate", "12", "-i", &pattern,
            "-pix_fmt", "yuv420p", "-vf", "scale=1920:1080", &format!("{out_prefix}_{}_slowmo.mp4", case.name)]).status();
    }
    if !keep_frames {
        let _ = std::fs::remove_dir_all(&frame_dir);
    }
    Ok(serde_json::json!({
        "case": case.name,
        "title": case.title,
        "shell": case.shell.name,
        "range_m": case.range_m,
        "struck_plate": plate.name,
        "plate_mm": plate.thickness_mm,
        "impact_angle_deg": event.impact_angle_deg,
        "terminal": format!("{:?}", event.terminal),
        "pen_mm": event.pen_mm,
        "effective_mm": event.effective_mm,
        "striking_v": event.striking_v,
        "residual_v": event.residual_v,
        "chain": chain_result.as_ref().map(|r| r.summary()),
        "lodged_mm": chain_result.as_ref().and_then(|r| r.lodged).map(|l| l.depth_m * 1000.0),
        "burst": burst,
        "knocked_out": knocked,
        "stowed_rounds_struck": rounds_struck,
        "components_holding_fragments": fragment_struck,
        "struck_not_breached": hits.iter().filter(|h| !h.1).map(|h| components::COMPONENT_SPECS[h.0].id).collect::<Vec<_>>(),
        "lattice": outcome.as_ref().map(|o| serde_json::json!({
            "particles": o.particles, "steps": o.steps, "cell_mm": o.cell_mm, "wall_ms": o.wall_ms,
            "energy_closure": o.ledger.closure(), "work_ratio": o.geometry.work_ratio,
            "aperture_mm": o.geometry.aperture_mm, "petals": o.geometry.petals, "plug": o.geometry.plug,
            "lip_mm": o.geometry.lip_mm, "bulge_mm": o.geometry.bulge_mm, "rim_kelvin": o.geometry.rim_kelvin,
            "grip_normal_n": o.grip_normal_n,
            "surface_heat_at_0_5_s": {"ring_mm": o.rim_curve.ring_m * 1000.0,
                "face_k": o.rim_curve.face_k.iter().map(|t| t.is_finite().then_some(*t)).collect::<Vec<_>>(),
                "bore_k": o.rim_curve.bore_k.iter().map(|t| t.is_finite().then_some(*t)).collect::<Vec<_>>(),
                "renderer_shape_at_wound_radius_of_aperture": o.rim_curve.heat_shape((o.geometry.aperture_mm * 1e-3).max(0.01))},
        })),
        "fragments": flying.len(),
        "fragment_mass_kg": fragment_mass,
        "fragment_kinetic_j": fragment_ke,
        "compartment_peak_kpa": pressure.as_ref().map(|p| p.peak_gauge_pa / 1000.0),
        "hatches_blown": pressure.as_ref().map(|p| p.hatches_blown.iter().map(|h| h.0).collect::<Vec<_>>()),
        "frames": frame_files.len(),
    }))
}

/// Path distance at which the chain first enters a component.
fn first_entry(chain_result: &Option<chain::ChainResult>, component: usize) -> f32 {
    chain_result.as_ref().and_then(|r| r.crossings.iter().find(|c| c.layer.kind.component() == Some(component)))
        .map_or(f32::MAX, |c| c.layer.s0)
}

/// A fragment `age` seconds after its birth: drag and gravity in substeps, cooling; it stops at the
/// first source triangle of the hull or turret meshes it enters (a surface it is leaving, with its
/// normal along the motion, is passed: fragments are born inside the struck plate's own solid, whose
/// source triangles are not perforated). Named section-view simplification: no bounce or embedding
/// drawn. Returns the position, the temperature and the component it came to rest in, if any.
fn fly(rig: &Rig, f: &FlyingFragment, age: f32) -> (Vec3, f32, Option<usize>) {
    let mut pos = f.pos;
    let mut vel = f.vel;
    let steps = ((age / 1.0e-4).ceil() as usize).clamp(1, 4000);
    let dt = age / steps as f32;
    let hull = rig.tank.hull_model();
    let hull_inv = hull.inverse();
    let mut stopped = false;
    'steps: for _ in 0..steps {
        let next = pos + vel * dt;
        let mut from = pos;
        for _ in 0..8 {
            let (a, b) = (hull.transform_point3(from), hull.transform_point3(next));
            let Some(hit) = rig.source.sweep(a, b, &rig.models) else { break };
            let normal = rig.models[hit.frame].inverse().transpose().transform_vector3(hit.normal).normalize_or_zero();
            let at = hull_inv.transform_point3(a.lerp(b, hit.fraction));
            if normal.dot(b - a) > 0.0 {
                // Leaving a solid: continue just past the surface.
                from = at + (next - pos).normalize_or_zero() * 1.0e-4;
                continue;
            }
            pos = at;
            vel = Vec3::ZERO;
            stopped = true;
            break 'steps;
        }
        pos = next;
        vel = fragments::drag_step(vel, f.size, 1.2, Vec3::new(0.0, -9.81, 0.0), dt);
    }
    let temp = if f.temp_k > 0.0 { fragments::cool(f.temp_k, f.size, vel.length(), crate::armor_damage::AMBIENT_K, age) } else { 0.0 };
    // The component whose envelope holds the resting point (or the point a cell short of it).
    let set = components::tiger_components();
    let art = components::Articulation::rest(crate::tank::TIGER_PARTS.turret_pivot);
    let probe = if stopped { pos + f.vel.normalize_or_zero() * 0.01 } else { pos };
    let inside = set.envelopes.iter().find(|env| env.contains(art.to_component(env, probe))).map(|env| env.spec);
    (pos, temp, inside)
}

fn system_colour(system: components::System) -> [u8; 3] {
    use components::System as S;
    match system {
        S::Engine | S::AirFilters => [107, 91, 62],
        S::Cooling(_) => [63, 90, 112],
        S::Fuel(_) => [138, 109, 31],
        S::Gearbox | S::Steering => [79, 107, 82],
        S::FinalDrive(_) => [63, 90, 74],
        S::Ammunition => [122, 59, 59],
        S::Radio => [90, 79, 122],
        S::HullMg | S::CoaxMg | S::MainGun => [85, 85, 90],
        S::GunnerSight => [70, 110, 120],
        S::TurretTraverse | S::Elevation => [100, 80, 60],
        S::Basket => [72, 72, 68],
        S::Seat => [60, 60, 64],
    }
}

fn layer_name(kind: chain::LayerKind) -> String {
    match kind {
        chain::LayerKind::StruckPlate => "struck plate".into(),
        chain::LayerKind::Armour { plate_mm, .. } => format!("far wall {plate_mm:.0} mm"),
        chain::LayerKind::Casing { component, entering, .. } => format!("{} casing {}",
            components::COMPONENT_SPECS[component].name, if entering { "in" } else { "out" }),
        chain::LayerKind::Interior { component, .. } => format!("{} interior", components::COMPONENT_SPECS[component].name),
        chain::LayerKind::Liquid { component, .. } => format!("{} (fuel)", components::COMPONENT_SPECS[component].name),
        chain::LayerKind::Vapour { component } => format!("{} (vapour)", components::COMPONENT_SPECS[component].name),
        chain::LayerKind::LiveRound { .. } => "a stowed 8.8 cm round".into(),
        chain::LayerKind::Air => "air".into(),
    }
}

fn short_fate(f: chain::Fate) -> String {
    match f {
        chain::Fate::Stopped { depth_m } => format!("stopped {:.0} mm in", depth_m * 1000.0),
        other => format!("{other:?}"),
    }
}

fn which_rsvg() -> Option<String> {
    ["/usr/bin/rsvg-convert", "/usr/local/bin/rsvg-convert"].iter()
        .find(|p| std::path::Path::new(p).exists()).map(|p| p.to_string())
}

/// `fea-bench [SEEDS]`: wall time per impact of the chain and of the voxel lattice over the kinetic
/// cases of the cross-section set, each from `seeds` impact states (the strike point moved by a few
/// millimetres), on this machine, separately from driving. CPU only.
pub(crate) fn bench(seeds: usize, out: &str) -> anyhow::Result<()> {
    let rig = rig();
    let mut chain_us: Vec<f32> = Vec::new();
    let mut lattice_ms: Vec<f32> = Vec::new();
    let mut rows = Vec::new();
    for case in cases() {
        if !matches!(case.shell.kind, crate::ballistics::ShellKind::Ap | crate::ballistics::ShellKind::Apcbc
            | crate::ballistics::ShellKind::Apcr) {
            continue;
        }
        for k in 0..seeds.max(1) {
            let hull = rig.tank.hull_model();
            let hull_inv = hull.inverse();
            let jitter = Vec3::new(0.0, 0.004 * k as f32, 0.003 * k as f32);
            let dir = case.dir.normalize();
            let dir_w = hull.transform_vector3(dir);
            let aim_w = hull.transform_point3(case.aim + jitter);
            let Some(contact) = projectile_armor_source::contact(&rig.source, &rig.models, &rig.tank, ProjectileVehicle::Player,
                crate::ballistics::ArmorScheme::TigerI, aim_w - dir_w * 12.0, aim_w + dir_w * 2.0) else { continue };
            let ProjectileContactKind::Vehicle(armor) = contact.kind else { continue };
            let plate = armor.surface_plate.map(|s| s.plate).unwrap_or_else(|| crate::ballistics::ArmorScheme::TigerI.plate(armor.facing));
            let horizontal = (-dir_w).dot(armor.world_normal).abs().clamp(0.0, 1.0).acos().to_degrees();
            let event = crate::ballistics::resolve(case.shell, case.range_m, plate, horizontal, 0x5EC7_10C5 ^ k as u32);
            let t0 = std::time::Instant::now();
            let line = game_fea::shot_line(&rig.source, &rig.models, &rig.tank, contact.pos, dir_w, &event);
            let state = chain::VehicleState {
                articulation: components::Articulation::rest(crate::tank::TIGER_PARTS.turret_pivot),
                fuel_fill: 0.8, gravity_hull: Vec3::NEG_Y, live_rounds: &rig.rounds,
            };
            let layers = chain::build_layers(&line, components::tiger_components(), &state);
            let result = chain::run(&event, &layers, crate::fea_damage::materials::AIR.density);
            let chain_time = t0.elapsed().as_secs_f32() * 1e6;
            chain_us.push(chain_time);
            let normal_hull = hull_inv.transform_vector3(armor.world_normal).normalize_or_zero();
            let seed = crate::armor_damage::position_seed(armor.local_point) ^ k as u32;
            if let Some((impact, _)) = game_fea::lattice_problem(&event, &line, &result, normal_hull, seed) {
                let t1 = std::time::Instant::now();
                let mut solver = lattice::Lattice::new(impact);
                solver.solve();
                let ms = t1.elapsed().as_secs_f32() * 1000.0;
                lattice_ms.push(ms);
                rows.push(serde_json::json!({"case": case.name, "seed": k, "chain_us": chain_time, "lattice_ms": ms,
                    "particles": solver.n, "steps": solver.step, "closure": solver.ledger_now().closure()}));
                println!("[fea-bench] {} #{k}: chain {:.0} us, lattice {} particles {} steps {:.0} ms",
                    case.name, chain_time, solver.n, solver.step, ms);
            }
        }
    }
    let pct = |v: &mut Vec<f32>, q: f32| -> f32 {
        if v.is_empty() { return 0.0; }
        v.sort_by(f32::total_cmp);
        v[((v.len() - 1) as f32 * q).round() as usize]
    };
    let summary = serde_json::json!({
        "threads": lattice::default_threads(),
        "impacts": lattice_ms.len(),
        "chain_us": {"p50": pct(&mut chain_us, 0.5), "p95": pct(&mut chain_us, 0.95), "p99": pct(&mut chain_us, 0.99),
            "max": chain_us.iter().cloned().fold(0.0, f32::max)},
        "lattice_ms": {"p50": pct(&mut lattice_ms, 0.5), "p95": pct(&mut lattice_ms, 0.95), "p99": pct(&mut lattice_ms, 0.99),
            "max": lattice_ms.iter().cloned().fold(0.0, f32::max)},
        "rows": rows,
    });
    println!("[fea-bench] {}", serde_json::to_string(&summary["lattice_ms"])?);
    std::fs::write(out, serde_json::to_string_pretty(&summary)?)?;
    Ok(())
}

/// `fea-fire-odds [SHOTS] [OUT]` (CPU only): how often a penetration of the Tiger starts a fire,
/// and what lit it (`fea_damage::ignition`; a round crossed: the ammunition, `cabin_heat`), for the
/// comparison with the wartime record (docs/FEA_DAMAGE_MODEL_20261102.md). Each shot is a round
/// of the Soviet anti-tank catalogue at a range drawn from 100-800 m, from an azimuth drawn round
/// the whole circle and 0-5 deg below level, aimed at a point drawn in the hull's armour box (two
/// shots in three) or the turret's; only those that perforate count. Each runs the game's own
/// chain, voxel lattice, fragment population and fragment tracing through the vehicle's surfaces
/// and measured components. Simplifications (named): the section rig's surfaces (hull and turret
/// meshes only, no lids, ports or workshop parts), every rack cradle holding a round, fuel 80 %,
/// no crew aboard, the turret forward and the gun level; the azimuths are uniform, not a battle's.
pub(crate) fn fire_odds(shots: usize, out: &str) -> anyhow::Result<()> {
    use crate::ballistics as b;
    use crate::fea_damage::ignition::{self, Flammable};
    let started = std::time::Instant::now();
    let rig = rig();
    let shells: [&'static b::ShellSpec; 7] = [&b::BR_240, &b::BR_240P, &b::BR_271_ZIS2, &b::BR_350B, &b::BR_354P, &b::BR_365, &b::BR_471];
    let hull = rig.tank.hull_model();
    let hull_inv = hull.inverse();
    let boxes = [
        (ArmorPart::Hull, rig.tank.hull_model(), rig_armor_bounds(b::ArmorScheme::TigerI, ArmorPart::Hull).expect("Tiger hull bounds")),
        (ArmorPart::Turret, rig.tank.turret_model(), rig_armor_bounds(b::ArmorScheme::TigerI, ArmorPart::Turret).expect("Tiger turret bounds")),
    ];
    let state = chain::VehicleState {
        articulation: components::Articulation::rest(crate::tank::TIGER_PARTS.turret_pivot),
        fuel_fill: 0.8, gravity_hull: Vec3::NEG_Y, live_rounds: &rig.rounds,
    };
    let rounds_in = |frame: BloodFrame| -> Vec<LiveAmmoRound> {
        let to_frame = blood_parent_model(&rig.tank, frame).inverse() * hull;
        rig.rounds.iter().map(|r| {
            let rotation = Quat::from_mat3(&glam::Mat3::from_cols(r.axes[0], r.axes[1], r.axes[2])).normalize();
            let (_, rot, centre) = (to_frame * Mat4::from_rotation_translation(rotation, r.center)).to_scale_rotation_translation();
            LiveAmmoRound { stable_id: r.location_id, center: centre, rotation: rot.normalize(), half_extents: r.half, explosive_energy_j: 0.0 }
        }).collect()
    };
    let mut rows = Vec::new();
    let (mut penetrations, mut misses, mut terminals) = (0usize, 0usize, std::collections::BTreeMap::<String, usize>::new());
    let mut causes = std::collections::BTreeMap::<&'static str, usize>::new();
    let mut by_facing = std::collections::BTreeMap::<String, (usize, usize)>::new();
    let mut trials_by_kind = std::collections::BTreeMap::<&'static str, (usize, usize, f32)>::new();
    for i in 0..shots {
        let h = |k: u32| hash01((i as u32).wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B) ^ 0xF1E0_0DD5);
        let shell = shells[((h(1) * shells.len() as f32) as usize).min(shells.len() - 1)];
        let range_m = 100.0 + 700.0 * h(2);
        let azimuth = h(3) * std::f32::consts::TAU;
        let below = (5.0 * h(4)).to_radians();
        let (part, model, (lo, hi)) = boxes[if h(5) < 2.0 / 3.0 { 0 } else { 1 }];
        let local = lo + (hi - lo) * Vec3::new(h(6), h(7), h(8));
        let aim_w = model.transform_point3(local);
        // From a shooter at `azimuth` round the hull (0 dead ahead), travelling toward the Tiger.
        let dir_hull = -Vec3::new(azimuth.sin() * below.cos(), below.sin(), azimuth.cos() * below.cos());
        let dir_w = hull.transform_vector3(dir_hull).normalize();
        let facing = if azimuth.cos() > std::f32::consts::FRAC_1_SQRT_2 { "front" }
            else if azimuth.cos() < -std::f32::consts::FRAC_1_SQRT_2 { "rear" } else { "side" };
        let Some(contact) = projectile_armor_source::contact(&rig.source, &rig.models, &rig.tank, ProjectileVehicle::Player,
            b::ArmorScheme::TigerI, aim_w - dir_w * 12.0, aim_w + dir_w * 2.0) else { misses += 1; continue };
        let ProjectileContactKind::Vehicle(armor) = contact.kind else { misses += 1; continue };
        let plate = armor.surface_plate.map(|s| s.plate).unwrap_or_else(|| b::ArmorScheme::TigerI.plate(armor.facing));
        let horizontal = (-dir_w).dot(armor.world_normal).abs().clamp(0.0, 1.0).acos().to_degrees();
        let seed = 0xF1E0_0000 ^ (i as u32).wrapping_mul(2_654_435_761);
        let mut event = b::resolve(shell, range_m, plate, horizontal, seed);
        if let Some(r) = b::ricochet::response(&event, dir_w * event.striking_v, armor.world_normal, 0) {
            event.terminal = b::Terminal::Ricochet;
            event.residual_v = r.velocity.length();
            event.behind_armor = 0.0;
        }
        *terminals.entry(format!("{:?}", event.terminal)).or_default() += 1;
        if !matches!(event.terminal, b::Terminal::Penetration) {
            continue;
        }
        penetrations += 1;
        let penetration_id = i as u64 + 1;
        let line = game_fea::shot_line(&rig.source, &rig.models, &rig.tank, contact.pos, dir_w, &event);
        let layers = chain::build_layers(&line, components::tiger_components(), &state);
        let chain_result = chain::run(&event, &layers, crate::fea_damage::materials::AIR.density);
        let frame = if armor.part == ArmorPart::Turret { BloodFrame::Turret } else { BloodFrame::Hull };
        let parent = blood_parent_model(&rig.tank, frame);
        let normal_hull = hull_inv.transform_vector3(armor.world_normal).normalize_or_zero();
        let lattice_seed = crate::armor_damage::position_seed(armor.local_point) ^ seed;
        let fragments = match game_fea::lattice_problem(&event, &line, &chain_result, normal_hull, lattice_seed) {
            Some((impact, axes)) => {
                let t0 = std::time::Instant::now();
                let mut solver = lattice::Lattice::new(impact);
                solver.solve();
                let outcome = game_fea::lattice_outcome(&mut solver, line.entry, axes, t0.elapsed().as_secs_f32() * 1000.0);
                game_fea::lattice_fragments(&line, chain_result.burst, &outcome, hull, parent, frame, penetration_id, &event, seed, None)
            }
            None => {
                let entry = parent.inverse().transform_point3(contact.pos);
                let axis = parent.inverse().transform_vector3(dir_w).normalize_or_zero();
                generate_penetration_fragments(penetration_id, frame, entry, axis, event, seed)
            }
        };
        let world = FragmentWorld::new(&rig.source, &rig.models, &rig.tank, state.articulation);
        let live = rounds_in(frame);
        let mut modules: Vec<Module> = ModuleKind::ALL.iter().map(|&kind| Module { kind, functional: true, fault: Default::default() }).collect();
        let (mut result, _, _) = resolve_fragment_damage(&fragments, None, &[], &world, &live, &mut modules);
        // The round itself: the live rounds it crosses, and what burns in the components it breaches
        // (the game's `fea_penetration_consequences`); then the strike's trials.
        let round_struck = chain_result.crossings.iter().skip(1).take_while(|c| c.v_in > 0.0)
            .any(|c| matches!(c.layer.kind, chain::LayerKind::LiveRound { .. }));
        for hit in chain_result.hits.iter().filter(|h| h.breached) {
            let Some(what) = ignition::flammable(&components::COMPONENT_SPECS[hit.component]) else { continue };
            result.enter_flammable(StrikeIgnition { local_point: Vec3::ZERO, local_velocity: Vec3::ZERO, component: hit.component,
                what, by_round: true, body_k: 0.0 });
        }
        result.ignite(penetration_id);
        let ammo = result.ammo || round_struck;
        let lit: Vec<&ignition::Trial> = result.ignition_trials.iter().filter(|t| t.lit).collect();
        let kind_name = |what: Flammable| match what {
            Flammable::Petrol => "petrol",
            Flammable::Engine => "engine",
            Flammable::Oil { .. } => "oil",
            Flammable::Upholstery => "upholstery",
            Flammable::Electrical => "electrical",
        };
        for t in &result.ignition_trials {
            let e = trials_by_kind.entry(kind_name(t.what)).or_default();
            e.0 += 1;
            e.1 += t.lit as usize;
            e.2 += t.chance;
        }
        let cause = if ammo { "ammunition" } else { lit.first().map_or("none", |t| kind_name(t.what)) };
        *causes.entry(cause).or_default() += 1;
        let key = format!("{facing} {}", if part == ArmorPart::Turret { "turret" } else { "hull" });
        let e = by_facing.entry(key).or_default();
        e.0 += 1;
        e.1 += (cause != "none") as usize;
        rows.push(serde_json::json!({"shot": i, "shell": shell.name, "range_m": range_m, "azimuth_deg": azimuth.to_degrees(),
            "aimed_part": format!("{part:?}"), "struck_part": format!("{:?}", armor.part), "plate": plate.name, "fragments": fragments.len(),
            "ammunition": ammo, "round_crossed_a_round": round_struck,
            "trials": result.ignition_trials.iter().map(|t| serde_json::json!({"component": components::COMPONENT_SPECS[t.component].id,
                "what": t.what.name(), "by_round": t.by_round, "body_k": t.body_k, "chance": t.chance, "lit": t.lit})).collect::<Vec<_>>(),
            "cause": cause}));
        println!("[fea-fire-odds] {i}: {} {range_m:.0} m {facing} {:?}: {} fragments; {cause}{}", shell.name, armor.part, fragments.len(),
            if result.ignition_trials.is_empty() { String::new() } else { format!(" ({} trials)", result.ignition_trials.len()) });
    }
    let fires: usize = causes.iter().filter(|(k, _)| **k != "none").map(|(_, v)| *v).sum();
    let rate = |n: usize| n as f32 / penetrations.max(1) as f32;
    let summary = serde_json::json!({
        "tool": "fea-fire-odds", "shots": shots, "misses": misses, "terminals": terminals, "penetrations": penetrations,
        "fires": fires, "fires_per_penetration": rate(fires),
        "causes_per_penetration": causes.iter().map(|(k, v)| (k.to_string(), rate(*v))).collect::<std::collections::BTreeMap<_, _>>(),
        "causes_count": causes,
        "by_facing": by_facing.iter().map(|(k, (n, f))| (k.clone(), serde_json::json!({"penetrations": n, "fires": f, "rate": *f as f32 / (*n).max(1) as f32})))
            .collect::<serde_json::Map<_, _>>(),
        "trials_by_kind": trials_by_kind.iter().map(|(k, (n, l, c))| (k.to_string(), serde_json::json!({"trials": n, "lit": l, "mean_chance": c / (*n).max(1) as f32})))
            .collect::<serde_json::Map<_, _>>(),
        "wartime": {
            "ors_normandy_1944": "AP-penetrated tanks burnt out: Tiger 4 of 5, Panther 14 of 22, Pz IV 4 of 5, Sherman 33 of 40; 3.25 penetrations per burnt-out Tiger (about 0.31 a penetration)",
            "ors_report_33_1945": "British tanks, one AP penetration: 55-56 % brewed up (36-37 of 66)",
            "amrl_1943": "over 90 % of tank fires from propellant lit by hot fragments tearing cases",
            "mrc_1946": "107 fires seen: fuel 20, ammunition 19, other 3, unknown 65",
            "nii48_1942": "T-34 (diesel) penetrated with extensive fire: 38 of 154 (repaired tanks; low)",
        },
        "simplifications": "section rig surfaces (hull and turret meshes); all rack cradles hold a round; fuel 80 %; no crew; turret forward; uniform azimuths",
        "wall_s": started.elapsed().as_secs_f32(),
        "rows": rows,
    });
    println!("[fea-fire-odds] {penetrations} penetrations of {shots} shots ({misses} missed the armour): {fires} fires ({:.0} %); causes {}",
        rate(fires) * 100.0, serde_json::to_string(&summary["causes_per_penetration"])?);
    println!("[fea-fire-odds] by facing {}", serde_json::to_string(&summary["by_facing"])?);
    println!("[fea-fire-odds] trials {}", serde_json::to_string(&summary["trials_by_kind"])?);
    if let Some(dir) = std::path::Path::new(out).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, serde_json::to_string_pretty(&summary)?)?;
    println!("[fea-fire-odds] wrote {out} ({:.0} s)", started.elapsed().as_secs_f32());
    Ok(())
}
