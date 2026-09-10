//! Explicit semantic anchor for the existing compiled house in standalone authored worlds.
use crate::act_manifest::{ActManifest, MarkerData};
use glam::Vec2;

pub(super) fn center(manifest: &ActManifest) -> anyhow::Result<Option<Vec2>> {
    resolve(manifest.value(), &manifest.effective_markers()?,
        manifest.terrain.identity.world_size_m * 0.5)
}

fn resolve(value: &serde_json::Value, markers: &[MarkerData], half_extent: f32) -> anyhow::Result<Option<Vec2>> {
    let Some(config) = value["authoring"].get("runtime_feature_building") else { return Ok(None); };
    anyhow::ensure!(value["authoring"]["mode"] == "playtest", "runtime feature building requires explicit Playtest authoring");
    anyhow::ensure!(config["blueprint"] == "feature-v1", "unknown authored feature blueprint");
    let name = config["marker_name"].as_str().filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("runtime feature requires an editor marker name"))?;
    let reserve = config["reserved_half_extent_m"].as_f64().map(|n|n as f32)
        .filter(|n|n.is_finite() && (8.0..=64.0).contains(n))
        .ok_or_else(|| anyhow::anyhow!("runtime feature requires 8..64 m reserved half extent"))?;
    let found: Vec<_> = markers.iter().filter(|m| !m.deleted && m.name == name).collect();
    anyhow::ensure!(found.len() == 1, "runtime feature requires one live marker named {name:?}");
    let pos = Vec2::new(found[0].pos.x, found[0].pos.z);
    anyhow::ensure!(pos.is_finite() && half_extent.is_finite()
        && pos.abs().max_element() + reserve <= half_extent,
        "runtime feature footprint lies outside actual terrain");
    Ok(Some(pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    fn config() -> serde_json::Value {
        serde_json::json!({"authoring":{"mode":"playtest","runtime_feature_building":{
            "blueprint":"feature-v1","marker_name":"Functional house","reserved_half_extent_m":18}}})
    }
    fn marker(x: f32, z: f32) -> MarkerData {
        MarkerData {name:"Functional house".into(),kind:"Monument".into(),pos:Vec3::new(x,0.0,z),deleted:false}
    }
    #[test]
    fn explicit_feature_follows_effective_editor_marker_without_old_coordinate_fallback() {
        assert_eq!(resolve(&config(), &[marker(-55.0,155.0)],384.0).unwrap(),Some(Vec2::new(-55.0,155.0)));
        assert_eq!(resolve(&config(), &[marker(-60.0,160.0)],384.0).unwrap(),Some(Vec2::new(-60.0,160.0)));
        let mut deleted=marker(-55.0,155.0);deleted.deleted=true;
        assert!(resolve(&config(),&[deleted],384.0).is_err());
        assert!(resolve(&config(),&[marker(0.0,0.0),marker(1.0,1.0)],384.0).is_err());
    }
    #[test]
    fn old_worlds_do_not_acquire_a_house_and_invalid_authored_footprints_fail() {
        assert_eq!(resolve(&serde_json::json!({}),&[],384.0).unwrap(),None);
        assert!(resolve(&config(),&[marker(370.0,0.0)],384.0).is_err());
        let mut canonical=config();canonical["authoring"]["mode"]=serde_json::json!("campaign");
        assert!(resolve(&canonical,&[marker(0.0,0.0)],384.0).is_err());
    }
}
