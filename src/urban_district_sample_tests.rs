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
    let placements = placements_from_json(&doc[BUILDING_INSTANCES_KEY]).expect("strict codec accepts the sample");
    assert!(placements.len() >= 20, "{} instances", placements.len());
    for placement in &placements {
        check_spec(&placement.spec).unwrap_or_else(|error| panic!("{}: {error}", placement.id));
        assert!(placement.id.starts_with("urban_intro."), "{}", placement.id);
        let quarter = placement.yaw_deg / 90.0;
        assert!((quarter - quarter.round()).abs() < 1e-4, "{} yaw {}", placement.id, placement.yaw_deg);
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

#[test]
fn urban_intro_district_sample_footprints_stay_in_their_blocks_and_off_the_streets() {
    let doc: serde_json::Value = serde_json::from_str(SAMPLE).expect("sample JSON parses");
    let placements = placements_from_json(&doc[BUILDING_INSTANCES_KEY]).expect("sample parses");
    let blocks: Vec<(String, (f32, f32, f32, f32))> = doc["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .map(|b| (b["id"].as_str().expect("block id").to_owned(), rect(&b["rect"])))
        .collect();
    let streets: Vec<(f32, f32, f32, f32)> =
        doc["streets"].as_array().expect("streets").iter().map(|s| rect(&s["rect"])).collect();
    assert!(!blocks.is_empty() && !streets.is_empty());
    for placement in &placements {
        let (min, max) = placement.footprint_xz();
        let block_id = placement.id.split('.').nth(1).expect("block part of the id");
        let (_, block) = blocks
            .iter()
            .find(|(id, _)| id == block_id)
            .unwrap_or_else(|| panic!("{} names no sample block", placement.id));
        assert!(
            min.x >= block.0 - 1e-3 && max.x <= block.1 + 1e-3 && min.y >= block.2 - 1e-3 && max.y <= block.3 + 1e-3,
            "{} leaves block {block_id}",
            placement.id
        );
        for street in &streets {
            assert!(!overlaps((min, max), *street, 0.01), "{} stands in a street", placement.id);
        }
    }
}
