//! Source-aware audit: a passing software regression is not a historical penetration certificate.
//! Runnable as part of the game bin tests or by compiling ballistics.rs alone with its dependencies.
use super::*;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../tests/fixtures/armor_reference_20260925.v1.json");
const SEEDS: u32 = 256;

fn plate(thickness_mm: f32, slope_deg: f32) -> Plate {
    Plate {
        name: "audit RHA plate (material assumption)",
        thickness_mm,
        slope_deg,
        material: ArmorMaterial::Rha,
    }
}

// Invert the actual thickness-dependent slope law; dividing a flat budget by one fixed angle
// multiplier would silently neglect the model's thickness/caliber term.
fn threshold_mm(shell: &'static ShellSpec, range_m: f32, slope: f32) -> f32 {
    let mut lo = 0.0;
    let mut hi = 1000.0;
    for _ in 0..32 {
        let mid = (lo + hi) * 0.5;
        if effective_thickness(shell, &plate(mid, slope), 0.0) < budget_mm(shell, range_m) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

fn event_json(e: ImpactEvent) -> Value {
    json!({"terminal":format!("{:?}",e.terminal), "striking_velocity_mps":e.striking_v,
        "impact_angle_deg":e.impact_angle_deg, "effective_mm":e.effective_mm,
        "penetration_budget_mm":e.pen_mm, "residual_velocity_mps":e.residual_v,
        "behind_armor_gameplay_scalar":e.behind_armor})
}

fn ensemble(shell: &'static ShellSpec, range: f32, plate: Plate, horizontal: f32) -> Value {
    let mut penetrating = 0;
    let mut positive_residual = 0;
    for seed in 0..SEEDS {
        let e = resolve(shell, range, plate, horizontal, seed);
        penetrating += u32::from(e.terminal == Terminal::Penetration);
        positive_residual += u32::from(e.residual_v > 0.0);
        assert!(e.pen_mm.is_finite() && e.effective_mm.is_finite());
        assert!(e.residual_v.is_finite() && e.behind_armor.is_finite());
        assert!(e.residual_v >= 0.0 && e.residual_v <= e.striking_v);
        if e.terminal != Terminal::Penetration {
            assert_eq!(e.residual_v, 0.0);
        }
    }
    json!({"seeds":SEEDS,"penetration_terminals":penetrating,
        "positive_residual_events":positive_residual,
        "seed_7_event":event_json(resolve(shell,range,plate,horizontal,7)),
        "probability_interpretation":"Game seed ensemble only; source criterion/probability not established"})
}

#[test]
fn source_fixture_preserves_units_identity_and_unknown_criteria() {
    let f: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        f["primary_numeric_comparisons"].as_array().unwrap().len(),
        8
    );
    let expected = [
        (500, 0, 129),
        (500, 30, 110),
        (1000, 0, 119),
        (1000, 30, 101),
        (1500, 0, 110),
        (1500, 30, 92),
        (2000, 0, 100),
        (2000, 30, 84),
    ];
    for (r, (yards, angle, mm)) in f["primary_numeric_comparisons"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected)
    {
        assert_eq!(r["range_yards"], yards);
        assert_eq!(r["angle_from_normal_deg"], angle);
        assert_eq!(r["reported_plate_mm"], mm);
        assert_eq!(r["source"], "ttt17");
    }
    assert_eq!(500.0_f64 * 0.9144, 457.2);
    assert!((2750.0_f64 * 0.3048 - 838.2).abs() < 1e-10);
    assert!((21.0_f64 * 0.45359237 - 9.52543977).abs() < 1e-10);
    assert_eq!(f["sources"]["ttt17"]["eligible_to_fit_live_pzgr39"], false);
    assert!(f["sources"]["ttt17"]["historical_acceptance_tolerance_mm"].is_null());
    assert!(f["sources"]["ttt17"]["physical_uncertainty_mm"].is_null());
    assert_eq!(f["sources"]["t34_crop"]["quarantined"], true);
}

#[test]
fn clean_budget_preserves_existing_positive_margin_event_exactly() {
    for shell in [&PZGR_39, &PZGR_40, &BR_350B] {
        for range in [100.0, 500.0, 1500.0] {
            let p = plate(25.0, 0.0);
            for seed in 0..64 {
                let clean = resolve(shell, range, p, 0.0, seed);
                let budgeted = resolve_with_budget_loss(shell, range, p, 0.0, seed, 0.0, 0.0);
                assert_eq!(clean.terminal, Terminal::Penetration);
                assert_eq!(event_json(clean), event_json(budgeted));
            }
        }
    }
}

#[test]
fn surviving_cover_reduces_residual_energy_and_post_armor_scalar() {
    for shell in [&PZGR_39, &PZGR_40, &BR_350B] {
        for range in [100.0, 500.0, 1500.0] {
            let p = plate(20.0, 0.0);
            let clean = resolve_with_budget_loss(shell, range, p, 0.0, 7, 0.0, 0.0);
            let spent = resolve_with_budget_loss(shell, range, p, 0.0, 7, 10.0, 0.10);
            assert_eq!(spent.terminal, Terminal::Penetration);
            assert!(spent.pen_mm < clean.pen_mm);
            assert!(spent.residual_v > 0.0 && spent.residual_v < clean.residual_v);
            assert!(spent.behind_armor > 0.0 && spent.behind_armor < clean.behind_armor);
            assert_eq!(spent.striking_v, clean.striking_v); // exterior range-law velocity preserved
            assert_eq!(spent.effective_mm, clean.effective_mm);
            // Independent energy balance: retained equivalent motion must cover plate work plus
            // residual motion (undo the existing .92 residual factor before comparing).
            let original_v2 = clean.striking_v.powi(2);
            let retained_v2 = original_v2 * (spent.pen_mm / clean.pen_mm).powf(2.0 / DEMARRE_N);
            let plate_work = original_v2 - (clean.residual_v / 0.92).powi(2);
            let observed = plate_work + (spent.residual_v / 0.92).powi(2);
            assert!(
                (retained_v2 - observed).abs() < 0.2,
                "energy accounting {retained_v2} vs {observed}"
            );
        }
    }
}

#[test]
fn cover_budget_is_monotonic_and_cannot_leave_energy_after_stopping() {
    let p = plate(45.0, 30.0);
    for shell in [&PZGR_39, &PZGR_40, &BR_350B] {
        for seed in 0..64 {
            let mut prev = resolve_with_budget_loss(shell, 500.0, p, 0.0, seed, 0.0, 0.0);
            for loss in (5..=240).step_by(5) {
                let e = resolve_with_budget_loss(shell, 500.0, p, 0.0, seed, loss as f32, 0.0);
                assert!(e.pen_mm <= prev.pen_mm && e.residual_v <= prev.residual_v);
                assert!(e.behind_armor <= prev.behind_armor);
                if e.terminal != Terminal::Penetration {
                    assert_eq!(e.residual_v, 0.0);
                    assert_eq!(e.behind_armor, 0.0);
                }
                prev = e;
            }
            assert_eq!(prev.terminal, Terminal::Bounce);
            assert_eq!(prev.pen_mm, 0.0);
        }
    }
}

#[test]
fn corrupt_or_exhausted_costs_fail_closed_and_negative_costs_cannot_add_energy() {
    let p = plate(40.0, 0.0);
    let clean = resolve_with_budget_loss(&PZGR_39, 500.0, p, 0.0, 7, 0.0, 0.0);
    let clamped = resolve_with_budget_loss(&PZGR_39, 500.0, p, 0.0, 7, -99.0, -0.5);
    assert_eq!(event_json(clean), event_json(clamped));
    for (loss, disruption) in [
        (f32::NAN, 0.0),
        (f32::INFINITY, 0.0),
        (0.0, f32::NAN),
        (0.0, f32::INFINITY),
        (0.0, 2.0),
        (1000.0, 0.0),
    ] {
        let e = resolve_with_budget_loss(&PZGR_39, 500.0, p, 0.0, 7, loss, disruption);
        assert_eq!(e.terminal, Terminal::Bounce);
        assert_eq!(e.pen_mm, 0.0);
        assert_eq!(e.residual_v, 0.0);
        assert_eq!(e.behind_armor, 0.0);
    }
    // A marginal random terminal at/below deterministic limit had no through energy and was
    // already rejected by the authoritative caller. Preserve that rule in the shared resolver.
    let threshold = threshold_mm(&PZGR_39, 500.0, 0.0);
    for seed in 0..SEEDS {
        let e = resolve_with_budget_loss(
            &PZGR_39,
            500.0,
            plate(threshold + 0.01, 0.0),
            0.0,
            seed,
            0.0,
            0.0,
        );
        assert_eq!(e.terminal, Terminal::Bounce);
        assert_eq!(e.residual_v, 0.0);
    }
}

#[test]
fn budget_loss_preserves_shatter_and_does_not_reinterpret_chemical_effects() {
    let fh = Plate {
        material: ArmorMaterial::FaceHardened,
        ..plate(80.0, 10.0)
    };
    assert_eq!(
        resolve(&BR_350B, 60.0, fh, 0.0, 7).terminal,
        Terminal::Shatter
    );
    let spent = resolve_with_budget_loss(&BR_350B, 60.0, fh, 0.0, 7, 50.0, 0.5);
    assert_eq!(spent.terminal, Terminal::Shatter);
    assert_eq!(spent.residual_v, 0.0);
    assert_eq!(spent.behind_armor, 0.0);
    for shell in [&SPRGR_88, &OF_530, &PANZERFAUST_60] {
        for mm in [5.0, 45.0, 150.0] {
            let p = plate(mm, 30.0);
            let clean = resolve(shell, 100.0, p, 0.0, 7);
            let spent = resolve_with_budget_loss(shell, 100.0, p, 0.0, 7, 1000.0, 1.0);
            assert_eq!(event_json(clean), event_json(spent));
        }
    }
}

#[test]
fn coarse_face_angle_convention_applies_nominal_slope_once() {
    let incoming = -Vec3::Z;
    let coarse_front = Vec3::Z;
    assert_eq!(horiz_obliquity_deg(incoming, coarse_front), 0.0);
    let t34 = ArmorScheme::T34_76.plate(Facing::HullFront);
    let e = resolve(
        &PZGR_39,
        500.0,
        t34,
        horiz_obliquity_deg(incoming, coarse_front),
        7,
    );
    assert!((e.impact_angle_deg - 60.0).abs() < 0.0001);
    // Rotating both coarse contact normal and incoming ray together cannot change obliquity.
    for yaw in [0.2, 0.8, 1.5, 2.9] {
        let q = glam::Quat::from_rotation_y(yaw);
        assert!(horiz_obliquity_deg(q * incoming, q * coarse_front) < 0.03);
    }
    // A plate tilted 30 degrees and a vertical plate approached horizontally at 30 degrees
    // present the same total angle under the current level-shot approximation.
    let a = effective_thickness(&PZGR_39, &plate(75.0, 30.0), 0.0);
    let b = effective_thickness(&PZGR_39, &plate(75.0, 0.0), 30.0);
    assert!((a - b).abs() < 0.0001);
}

#[test]
fn source_comparison_and_model_limit_report() {
    let source: Value = serde_json::from_str(FIXTURE).unwrap();
    let mut comparisons = Vec::new();
    for row in source["primary_numeric_comparisons"].as_array().unwrap() {
        let range_m = row["range_yards"].as_f64().unwrap() * 0.9144;
        let angle = row["angle_from_normal_deg"].as_f64().unwrap() as f32;
        let expected = row["reported_plate_mm"].as_f64().unwrap();
        let actual = threshold_mm(&PZGR_39, range_m as f32, angle);
        comparisons.push(json!({"source":"ttt17","source_page":14,
            "source_range_yards":row["range_yards"],"range_m":range_m,
            "angle_from_normal_deg":angle,"source_reported_plate_mm":expected,
            "live_pzgr39_threshold_mm":actual,"difference_mm":actual as f64-expected,
            "relative_difference_percent":100.0*(actual as f64/expected-1.0),
            "acceptance":"Not evaluated: projectile identity differs and historical criterion/tolerance unknown",
            "live_event_at_reported_plate":ensemble(&PZGR_39,range_m as f32,plate(expected as f32,angle),0.0)}));
    }
    let secondary:Vec<Value>=source["secondary_candidates_not_calibration"]["pzgr39_rows_30deg"]
        .as_array().unwrap().iter().map(|row| {
            let range=row[0].as_f64().unwrap() as f32;
            let expected=row[1].as_f64().unwrap();
            let actual=threshold_mm(&PZGR_39,range,30.0);
            json!({"range_m":range,"compilation_mm":expected,"live_threshold_mm":actual,
                "difference_mm":actual as f64-expected,"status":"Unverified secondary candidate, not calibration"})
        }).collect();
    let mut characterizations = Vec::new();
    for range in [100.0, 500.0, 1000.0, 1500.0, 2000.0] {
        for horizontal in [0.0, 30.0, 60.0, 75.0, 89.0] {
            characterizations.push(json!({"case":"Current T34_76 front definition: 45 mm RHA, 60 deg nominal slope",
                "source_status":"Model characterization; not a verified historical firing setup",
                "range_m":range,"requested_horizontal_deg":horizontal,
                "live":ensemble(&PZGR_39,range,ArmorScheme::T34_76.plate(Facing::HullFront),horizontal)}));
        }
    }
    let kv:Vec<Value>=[20.0,70.0].into_iter().map(|angle|json!({
        "source":"kv_trial","row":2,"source_observation":"Through perforation of 75 mm turret rear at 1500 m",
        "interpretation":"Conditional single-angle RHA assumption; no projectile-identity equivalence proven",
        "assumed_total_angle_deg":angle,"live":ensemble(&PZGR_39,1500.0,plate(75.0,angle),0.0)})).collect();
    let p = plate(40.0, 0.0);
    let budget: Vec<Value> = [(0.0, 0.0), (20.0, 0.0), (20.0, 0.15), (200.0, 0.0)]
        .into_iter()
        .map(|(loss, disruption)| {
            json!({"loss_mm":loss,"disruption_fraction":disruption,
            "event":event_json(resolve_with_budget_loss(&PZGR_39,500.0,p,0.0,7,loss,disruption))})
        })
        .collect();
    let report = json!({"schema":"kessel.armor-calibration-receipt.v1","date":"2026-09-25",
        "fixture":"tests/fixtures/armor_reference_20260925.v1.json",
        "solver":"src/ballistics.rs, production functions, no reimplemented solver",
        "historical_fidelity_validated":false,
        "historical_fit_parameters_changed":[],
        "numeric_tolerances":{"energy_balance_m2_per_s2":0.2,"coarse_angle_deg":0.0001,
            "threshold_search_mm":"32 bisection steps over 0..1000; f32 resolution; numerical only",
            "historical_acceptance_mm":null},
        "primary_sources":source["sources"],"primary_numeric_comparisons":comparisons,
        "secondary_candidates":secondary,"kv_trial_angle_sensitivity":kv,
        "t34_model_characterization":characterizations,"cover_budget_regression":budget,
        "unresolved":["KE Bounce conflates ricochet and failed penetration; no explicit ricochet law",
            "Horizontal incidence capped at 75 deg; kinetic slope response capped at 80 deg after normalization",
            "Coarse OBB faces and nominal slopes; incoming vertical angle and roof/belly are not represented by this route",
            "Residual factor 0.92 and behind-armor scalar lack measured spall/fragment or APHE fuze calibration",
            "The marginal 8 percent band is game stochasticity, not a source-derived penetration criterion",
            "Cap/disruption budget is conservative equivalent-energy accounting, not measured yaw physics"]});
    if let Ok(path) = std::env::var("KESSEL_ARMOR_AUDIT_OUTPUT") {
        let path = std::path::Path::new(&path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        eprintln!("Armor source audit written to {}", path.display());
    }
    for c in report["primary_numeric_comparisons"].as_array().unwrap() {
        eprintln!("TTT17 range={:.1}m angle={:.0} source={:.0}mm live={:.3}mm delta={:+.3}mm [NOT like-for-like]",
            c["range_m"].as_f64().unwrap(),c["angle_from_normal_deg"].as_f64().unwrap(),
            c["source_reported_plate_mm"].as_f64().unwrap(),c["live_pzgr39_threshold_mm"].as_f64().unwrap(),
            c["difference_mm"].as_f64().unwrap());
    }
    assert_eq!(comparisons.len(), 8);
    assert!(!report["historical_fidelity_validated"].as_bool().unwrap());
}
