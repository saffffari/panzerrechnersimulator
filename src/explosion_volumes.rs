//! Baked explosion volumes for the raymarch medium (`tools/bake_explosion_volumes.py`).
//!
//! Each slot is a 64^3 grid of density and flame (u8 each, normalised per slot) resampled from an
//! OpenVDB ground explosion, Y up, with the shape's aspect (extent over its largest extent). A
//! detonation draws one `Burst` slot (fire and ejecta, its first second) cross-fading into one
//! `Cloud` slot (soot and lifted earth, the seconds after); `renderer_raymarch.rs` sizes and times
//! them from the blast. The atlas is a local runtime asset: without it the medium keeps its
//! parcel-built detonation.

use glam::Vec3;

/// Voxels along each side of a slot.
pub const SLOT: u32 = 64;
/// Where the bake writes the atlas, relative to the asset root.
pub const ATLAS_DIR: &str = "assets/vfx/explosion_volumes";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Burst,
    Cloud,
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub role: Role,
    /// The shape's extent over its largest extent, atlas axes (x, y up, z).
    pub aspect: Vec3,
    pub source: String,
}

pub struct ExplosionVolumes {
    pub slots: Vec<Slot>,
    /// 64 x 64 x (64 n) voxels of (density, flame), x fastest, then y, then z.
    pub voxels: Vec<u8>,
}

impl ExplosionVolumes {
    /// The baked atlas under the asset root (`ATLAS_DIR`).
    pub fn load() -> anyhow::Result<Self> {
        let manifest = crate::resource::read_to_string(format!("{ATLAS_DIR}/atlas.json"))?;
        let voxels = crate::resource::read(format!("{ATLAS_DIR}/atlas.bin"))?;
        Self::parse(&manifest, voxels)
    }

    fn parse(manifest: &str, voxels: Vec<u8>) -> anyhow::Result<Self> {
        let m: serde_json::Value = serde_json::from_str(manifest)?;
        let schema = m["schema"].as_str().unwrap_or_default();
        anyhow::ensure!(schema == "kessel.explosion-volumes.v1", "unknown atlas schema {schema:?}");
        anyhow::ensure!(m["slot"].as_u64() == Some(SLOT as u64), "atlas slot {} is not {SLOT}", m["slot"]);
        let listed = m["slots"].as_array().cloned().unwrap_or_default();
        let count = m["count"].as_u64().unwrap_or(0) as usize;
        anyhow::ensure!(count == listed.len() && count > 0, "atlas count {count} for {} slots", listed.len());
        let expected = (SLOT * SLOT * SLOT) as usize * 2 * count;
        anyhow::ensure!(voxels.len() == expected, "atlas holds {} bytes, {expected} expected", voxels.len());
        let slots = listed.iter().map(|s| {
            let source = s["source"].as_str().unwrap_or_default().to_owned();
            let role = match s["role"].as_str().unwrap_or_default() {
                "burst" => Role::Burst,
                "cloud" => Role::Cloud,
                other => anyhow::bail!("unknown slot role {other:?}"),
            };
            let a = s["aspect"].as_array().cloned().unwrap_or_default();
            anyhow::ensure!(a.len() == 3, "slot {source} has no aspect");
            let aspect = Vec3::new(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, a[2].as_f64().unwrap_or(0.0) as f32);
            anyhow::ensure!(aspect.is_finite() && aspect.min_element() > 0.0 && aspect.max_element() <= 1.0 + 1e-4,
                "slot {source} has aspect {aspect:?}");
            Ok(Slot { role, aspect, source })
        }).collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self { slots, voxels })
    }

    /// The atlas's depth in voxels (its slots stacked).
    pub fn depth(&self) -> u32 { SLOT * self.slots.len() as u32 }

    /// A slot of `role` chosen by `pick` in 0..1: its index and aspect.
    pub fn pick(&self, role: Role, pick: f32) -> Option<(u32, Vec3)> {
        let candidates: Vec<usize> = (0..self.slots.len()).filter(|&i| self.slots[i].role == role).collect();
        if candidates.is_empty() { return None; }
        let k = ((pick.clamp(0.0, 0.9999) * candidates.len() as f32) as usize).min(candidates.len() - 1);
        let i = candidates[k];
        Some((i as u32, self.slots[i].aspect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(roles: &[&str]) -> String {
        let slots: Vec<String> = roles.iter().enumerate().map(|(i, r)| format!(
            r#"{{"role":"{r}","source":"Ground_explosion_{i}.vdb","aspect":[1.0,0.5,0.9]}}"#)).collect();
        format!(r#"{{"schema":"kessel.explosion-volumes.v1","slot":64,"count":{},"slots":[{}]}}"#, roles.len(), slots.join(","))
    }

    #[test]
    fn picks_stay_within_their_role_and_size_is_checked() {
        let n = 3;
        let v = ExplosionVolumes::parse(&manifest(&["burst", "cloud", "cloud"]), vec![0; 64 * 64 * 64 * 2 * n]).unwrap();
        assert_eq!(v.depth(), 192);
        for k in 0..50 {
            let (i, aspect) = v.pick(Role::Cloud, k as f32 / 49.0).unwrap();
            assert!(i == 1 || i == 2);
            assert_eq!(aspect, Vec3::new(1.0, 0.5, 0.9));
            assert_eq!(v.pick(Role::Burst, k as f32 / 49.0).unwrap().0, 0);
        }
        assert!(ExplosionVolumes::parse(&manifest(&["burst"]), vec![0; 10]).is_err());
        assert!(ExplosionVolumes::parse(&manifest(&["fireball"]), vec![0; 64 * 64 * 64 * 2]).is_err());
    }
}
