//! The crewman's anatomical voxel volume (2026-11-02, the owner: build the crew's voxels from the
//! character model, the skeleton and organs inside it, each tissue with its physical properties).
//! The volume is a `.kvox` file (`tools/anatomy/kvox.py`: magic, a JSON header, one byte per voxel
//! per channel, x fastest, then z, then y) in the relaxed crew figure's frame that `crew_tissue`
//! voxelises; its labels name rows of the tissue table (`catalog/crew_anatomy_tissues.v1.json`:
//! density from the IT'IS Foundation database, a strength class mapped onto `crew_tissue`'s skin,
//! muscle and bone, a blood fraction from ICRP 89, a display colour). Built by
//! `tools/anatomy/build_anatomy.py`; see docs/CREW_ANATOMY_VOXELS.md.
//!
//! `KESSEL_CREW_ANATOMY=<path>` reads another volume (a preliminary one), `KESSEL_CREW_ANATOMY=off`
//! none; without a volume `crew_tissue` builds its three classes from the figure and skeleton.
use glam::Vec3;

/// The volume's place under the asset root.
pub const VOLUME: &str = "assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/anatomy/german_panzertruppe_face01_relaxed_anatomy.kvox";
/// The tissue table (tracked).
pub const TABLE: &str = "catalog/crew_anatomy_tissues.v1.json";
const MAGIC: &[u8; 8] = b"KVOXANAT";

/// How a tissue holds together in `crew_tissue`'s lattice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
    Skin,
    Muscle,
    Bone,
    /// Blood, CSF, urine, gut contents: mass, and only the weakest hold (`crew_tissue`).
    Fluid,
    /// Air (outside the body, the airways): no cell.
    Gas,
}

/// One row of the tissue table.
#[derive(Clone, Debug)]
pub struct Tissue {
    pub id: u8,
    pub key: String,
    pub density_kg_m3: f32,
    pub strength: Strength,
    pub colour: [u8; 3],
    pub blood_volume_fraction: f32,
}

/// A loaded volume and its table.
pub struct Volume {
    pub dims: [usize; 3],
    pub cell_m: f32,
    /// The minimum corner of voxel (0, 0, 0) in the relaxed figure's frame.
    pub origin: Vec3,
    /// One label per voxel, x fastest, then z, then y (`crew_tissue`'s grid order).
    pub labels: Vec<u8>,
    pub tissues: Vec<Tissue>,
    /// Where it was read from (receipts).
    pub source: String,
}

impl Volume {
    pub fn label(&self, i: usize, j: usize, k: usize) -> u8 {
        self.labels[(j * self.dims[2] + k) * self.dims[0] + i]
    }

    pub fn tissue(&self, label: u8) -> Option<&Tissue> {
        self.tissues.iter().find(|t| t.id == label)
    }
}

fn colour(hex: &str) -> anyhow::Result<[u8; 3]> {
    let hex = hex.trim_start_matches('#');
    anyhow::ensure!(hex.len() == 6, "colour {hex:?} is not #rrggbb");
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
    Ok([byte(0)?, byte(2)?, byte(4)?])
}

/// The tissue table's rows.
pub fn parse_table(text: &str) -> anyhow::Result<Vec<Tissue>> {
    let table: serde_json::Value = serde_json::from_str(text)?;
    let rows = table["labels"].as_array().ok_or_else(|| anyhow::anyhow!("tissue table without labels"))?;
    rows.iter().map(|row| {
        let strength = match row["strength_class"].as_str().unwrap_or("") {
            "skin" => Strength::Skin,
            "muscle" => Strength::Muscle,
            "bone" => Strength::Bone,
            "fluid" => Strength::Fluid,
            "gas" => Strength::Gas,
            other => anyhow::bail!("tissue {:?}: unknown strength class {other:?}", row["key"]),
        };
        Ok(Tissue {
            id: u8::try_from(row["id"].as_u64().ok_or_else(|| anyhow::anyhow!("tissue without id"))?)?,
            key: row["key"].as_str().unwrap_or("").to_string(),
            density_kg_m3: row["density_kg_m3"].as_f64().ok_or_else(|| anyhow::anyhow!("tissue {:?} without density", row["key"]))? as f32,
            strength,
            colour: colour(row["colour"].as_str().unwrap_or("#808080"))?,
            blood_volume_fraction: row["blood_volume_fraction"].as_f64().unwrap_or(0.0) as f32,
        })
    }).collect()
}

/// A `.kvox` file's dimensions, cell, origin and label channel.
pub fn parse_volume(data: &[u8]) -> anyhow::Result<([usize; 3], f32, Vec3, Vec<u8>)> {
    anyhow::ensure!(data.len() >= 16 && &data[..8] == MAGIC, "not a KVOX anatomy volume");
    let version = u32::from_le_bytes(data[8..12].try_into()?);
    anyhow::ensure!(version == 1, "unsupported KVOX version {version}");
    let length = u32::from_le_bytes(data[12..16].try_into()?) as usize;
    anyhow::ensure!(data.len() >= 16 + length, "KVOX header runs past the file");
    let header: serde_json::Value = serde_json::from_slice(&data[16..16 + length])?;
    anyhow::ensure!(header["format"] == "kessel.crew-anatomy-voxels", "unexpected KVOX format {}", header["format"]);
    let dims: Vec<usize> = header["dims"].as_array().map(|d| d.iter().filter_map(|v| v.as_u64()).map(|v| v as usize).collect())
        .unwrap_or_default();
    anyhow::ensure!(dims.len() == 3 && dims.iter().all(|&d| d > 0), "KVOX dims {:?}", header["dims"]);
    let cell_m = header["voxel_size_m"].as_f64().ok_or_else(|| anyhow::anyhow!("KVOX without voxel size"))? as f32;
    let origin: Vec<f32> = header["origin_m"].as_array().map(|o| o.iter().filter_map(|v| v.as_f64()).map(|v| v as f32).collect())
        .unwrap_or_default();
    anyhow::ensure!(origin.len() == 3, "KVOX origin {:?}", header["origin_m"]);
    let channels = header["channels"].as_array().ok_or_else(|| anyhow::anyhow!("KVOX without channels"))?;
    let count = dims[0] * dims[1] * dims[2];
    anyhow::ensure!(data.len() == 16 + length + count * channels.len(), "KVOX length does not match its dims and channels");
    let index = channels.iter().position(|c| c["name"] == "label").ok_or_else(|| anyhow::anyhow!("KVOX without a label channel"))?;
    let at = 16 + length + index * count;
    Ok(([dims[0], dims[1], dims[2]], cell_m, Vec3::new(origin[0], origin[1], origin[2]), data[at..at + count].to_vec()))
}

/// The crew's volume and table, if there is a volume (see the module doc).
pub fn load() -> anyhow::Result<Option<Volume>> {
    let path = match std::env::var("KESSEL_CREW_ANATOMY") {
        Ok(value) if value.trim().eq_ignore_ascii_case("off") => return Ok(None),
        Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => VOLUME.to_string(),
    };
    if !crate::resource::exists(&path) {
        return Ok(None);
    }
    let (dims, cell_m, origin, labels) = parse_volume(&crate::resource::read(&path)?)?;
    // The table is tracked: the checkout's own, else the resource root's.
    let table = std::fs::read_to_string(TABLE).or_else(|_| crate::resource::read_to_string(TABLE))?;
    let tissues = parse_table(&table)?;
    for label in labels.iter().copied().collect::<std::collections::BTreeSet<u8>>() {
        anyhow::ensure!(tissues.iter().any(|t| t.id == label), "{path}: label {label} has no row in {TABLE}");
    }
    Ok(Some(Volume { dims, cell_m, origin, labels, tissues, source: path }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(header: &str, payload: &[u8]) -> Vec<u8> {
        let mut text = header.as_bytes().to_vec();
        while (16 + text.len()) % 8 != 0 {
            text.push(b' ');
        }
        let mut data = MAGIC.to_vec();
        data.extend(1u32.to_le_bytes());
        data.extend((text.len() as u32).to_le_bytes());
        data.extend(text);
        data.extend(payload);
        data
    }

    #[test]
    fn a_volume_reads_its_label_channel_in_grid_order() {
        let header = r#"{"format": "kessel.crew-anatomy-voxels", "version": 1, "dims": [2, 3, 2], "voxel_size_m": 0.01,
            "origin_m": [-0.01, 0.0, -0.01], "channels": [{"name": "label"}, {"name": "bone"}]}"#;
        let labels: Vec<u8> = (0..12).collect();
        let mut payload = labels.clone();
        payload.extend([255u8; 12]);
        let (dims, cell, origin, read) = parse_volume(&volume(header, &payload)).unwrap();
        assert_eq!((dims, cell, origin), ([2, 3, 2], 0.01, Vec3::new(-0.01, 0.0, -0.01)));
        assert_eq!(read, labels);
        let v = Volume { dims, cell_m: cell, origin, labels: read, tissues: Vec::new(), source: String::new() };
        // x fastest, then z, then y.
        assert_eq!(v.label(1, 0, 0), 1);
        assert_eq!(v.label(0, 0, 1), 2);
        assert_eq!(v.label(0, 1, 0), 4);
        assert!(parse_volume(&volume(header, &payload[..20])).is_err());
    }

    #[test]
    fn the_tracked_table_maps_every_row_to_a_strength() {
        let tissues = parse_table(&std::fs::read_to_string(TABLE).unwrap()).unwrap();
        let get = |key: &str| tissues.iter().find(|t| t.key == key).unwrap_or_else(|| panic!("{key}"));
        assert_eq!(get("exterior").strength, Strength::Gas);
        assert_eq!(get("cortical_bone").strength, Strength::Bone);
        assert_eq!(get("muscle").strength, Strength::Muscle);
        assert_eq!(get("heart_blood").strength, Strength::Fluid);
        assert!(tissues.iter().all(|t| t.density_kg_m3 > 0.0));
    }
}
