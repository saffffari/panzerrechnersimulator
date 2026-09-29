//! The tracked urban-intro district sample (catalog/examples/urban_intro_district.sample.json,
//! written by `tools/worldgen/urban_intro_district.py --write-sample`; its Python test checks the
//! file is current) must pass the strict `building_instances` codec and the grammar's limits, and
//! its footprints must stay inside their blocks and off the streets.
use super::*;
use crate::building_grammar::check_spec;

const SAMPLE: &str = include_str!("../catalog/examples/urban_intro_district.sample.json");

fn rect(value: &serde_json::Value) -> (f32, f32, f32, f32) {
    let r: Vec<f32> = value
        .as_array()
        .expect("rect array")
        .iter()
        .map(|v| v.as_f64().expect("rect number") as f32)
        .collect();
    assert_eq!(r.len(), 4, "rect is [x0, x1, z0, z1]");
    (r[0], r[1], r[2], r[3])
}

fn overlaps(fp: (Vec2, Vec2), r: (f32, f32, f32, f32), tol: f32) -> bool {
    fp.0.x < r.1 - tol && r.0 < fp.1.x - tol && fp.0.y < r.3 - tol && r.2 < fp.1.y - tol
}

#[test]
fn urban_intro_district_sample_passes_the_codec_and_the_grammar_limits() {
    let doc: serde_json::Value = serde_json::from_str(SAMPLE).expect("sample JSON parses");
    let placements = placements_from_json(&doc[BUILDING_INSTANCES_KEY])
        .expect("strict codec accepts the sample");
    assert!(placements.len() >= 20, "{} instances", placements.len());
    for placement in &placements {
        check_spec(&placement.spec).unwrap_or_else(|error| panic!("{}: {error}", placement.id));
        assert!(placement.id.starts_with("urban_intro."), "{}", placement.id);
        let quarter = placement.yaw_deg / 90.0;
        assert!(
            (quarter - quarter.round()).abs() < 1e-4,
            "{} yaw {}",
            placement.id,
            placement.yaw_deg
        );
    }
    for typology in Typology::ALL {
        assert!(
            placements.iter().any(|p| p.spec.typology == typology),
            "the sample has no {}",
            typology.as_str()
        );
    }
    let round_trip = placements_from_json(&placements_to_json(&placements)).expect("round trip");
    assert_eq!(round_trip, placements);
}

/// The photo analysis's first open issue (P G2): the kit's 4 m bay against the photographed
/// 2.7-3.4 m window axes of 1930s blocks (2.6-3.4 m of merchant houses). The grammar fits every
/// facade to the documented axes and scales the kit window with the bay; this measures the result
/// on the district sample's facades and pins the residual differences it reports: windows up to
/// 0.11 m wider than documented (the kit window is half its bay), two-bay shop windows up to
/// 0.55 m wider than the documented 2.2-2.5 m, and on merchant houses' tall storeys a sill up to
/// 0.33 m above the documented 0.9 m (the kit sill is a third of the storey).
#[test]
fn urban_intro_district_facades_keep_the_photographed_window_axes() {
    use crate::building_placement::Typology;
    let doc: serde_json::Value = serde_json::from_str(SAMPLE).expect("sample JSON parses");
    let placements = placements_from_json(&doc[BUILDING_INSTANCES_KEY]).expect("sample parses");
    let mut measured = 0;
    for placement in &placements {
        let (axes, width, sill_max) = match placement.spec.typology {
            Typology::ApartmentBlock => ((2.7, 3.4), (1.3, 1.45), 0.9),
            Typology::MerchantBrickHouse => ((2.6, 3.4), (1.1, 1.4), 0.9),
            _ => continue,
        };
        let blueprint = crate::building_grammar::blueprint_for(&placement.spec, &placement.id).unwrap();
        let floors: std::collections::BTreeMap<&str, f32> = blueprint
            .floors
            .iter()
            .map(|f| (f.id.as_str(), f.elevation_m))
            .collect();
        let mut xs: Vec<f32> = Vec::new();
        for m in &blueprint.modules {
            let parts: Vec<&str> = m.id.as_str().split('.').collect();
            if parts.len() != 4 || parts[2] != "front" || !m.asset_id.as_str().starts_with("urban.brick.window") {
                continue;
            }
            let t = m.transform;
            // The kit window: 2.0 m of the 4 m bay wide, sill at 1.031 of its 3 m height.
            let w = 2.0 * t.scale[0];
            let sill = t.translation_m[1] + 1.031 * t.scale[1];
            // The finished floor lies the grammar's 0.3 m floor zone above the piece's base (the
            // storey's floor may have burned, and then it has no FloorDef).
            let floor = m
                .floor_id
                .as_ref()
                .and_then(|f| floors.get(f.as_str()).copied())
                .unwrap_or(t.translation_m[1] + crate::building_grammar::FLOOR_ZONE_M);
            if 4.0 * t.scale[0] > axes.1 + 0.01 {
                // A two-bay shop window (P G2: shop windows about 2.2-2.5 m, [08-13], [04-10]); the
                // kit window over two fitted bays is up to 0.55 m wider.
                assert!(w >= 2.2 - 0.1 && w <= 2.5 + 0.55, "{}: shop window {w:.2} m wide", m.id);
                continue;
            }
            assert!(w >= width.0 - 1e-3 && w <= width.1 + 0.11, "{}: window {w:.2} m wide", m.id);
            if parts[1] != "sc" {
                assert!(sill - floor >= sill_max - 0.1 && sill - floor <= sill_max + 0.33, "{}: sill {:.2} m", m.id, sill - floor);
            }
            if parts[1] == "s1" || (placement.spec.storeys == 1 && parts[1] == "s0") {
                xs.push(t.translation_m[0]);
            }
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.windows(2) {
            let axis = pair[1] - pair[0];
            // Adjacent window bays (a door or shop bay between two windows doubles the axis).
            if axis < axes.1 * 1.5 {
                assert!(axis >= axes.0 - 1e-3 && axis <= axes.1 + 1e-3, "{}: axis {axis:.2} m", placement.id);
                measured += 1;
            }
        }
    }
    assert!(measured > 20, "{measured} axes measured");
}

#[test]
fn urban_intro_district_sample_footprints_stay_in_their_blocks_and_off_the_streets() {
    let doc: serde_json::Value = serde_json::from_str(SAMPLE).expect("sample JSON parses");
    let placements = placements_from_json(&doc[BUILDING_INSTANCES_KEY]).expect("sample parses");
    let blocks: Vec<(String, (f32, f32, f32, f32))> = doc["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .map(|b| {
            (
                b["id"].as_str().expect("block id").to_owned(),
                rect(&b["rect"]),
            )
        })
        .collect();
    let streets: Vec<(f32, f32, f32, f32)> = doc["streets"]
        .as_array()
        .expect("streets")
        .iter()
        .map(|s| rect(&s["rect"]))
        .collect();
    assert!(!blocks.is_empty() && !streets.is_empty());
    for placement in &placements {
        let (min, max) = placement.footprint_xz();
        let block_id = placement
            .id
            .split('.')
            .nth(1)
            .expect("block part of the id");
        let (_, block) = blocks
            .iter()
            .find(|(id, _)| id == block_id)
            .unwrap_or_else(|| panic!("{} names no sample block", placement.id));
        assert!(
            min.x >= block.0 - 1e-3
                && max.x <= block.1 + 1e-3
                && min.y >= block.2 - 1e-3
                && max.y <= block.3 + 1e-3,
            "{} leaves block {block_id}",
            placement.id
        );
        for street in &streets {
            assert!(
                !overlaps((min, max), *street, 0.01),
                "{} stands in a street",
                placement.id
            );
        }
    }
}
