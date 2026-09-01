//! Deterministic, mesh-free layout for an explorable asset-library gallery.
//!
//! The yard is deliberately only a piece of spatial planning. It never resolves an asset path,
//! opens a GLB, or asks the renderer for bounds. Every item receives the same nominal footprint;
//! the editor remains responsible for loading models and can compensate for unusually large
//! assets after it knows their real bounds.

use std::fmt;

use glam::{Vec2, Vec3};

/// Blank category labels are collected here instead of creating an unlabeled yard block.
pub const UNCATEGORIZED: &str = "Uncategorized";

/// Small read-only interface accepted by the layout planner.
///
/// `assetlib::AssetEntry` implements this below, while tests and future importers can provide the
/// same three strings without depending on the scanner or renderer.
pub trait AssetYardEntry {
    fn yard_path(&self) -> &str;
    fn yard_name(&self) -> &str;
    fn yard_category(&self) -> &str;
}

impl AssetYardEntry for crate::assetlib::AssetEntry {
    fn yard_path(&self) -> &str {
        &self.path
    }

    fn yard_name(&self) -> &str {
        &self.name
    }

    fn yard_category(&self) -> &str {
        &self.category
    }
}

/// Tunable physical dimensions of an asset yard. All distances are world metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssetYardConfig {
    /// Number of assets across one category's mini-grid.
    pub assets_per_row: usize,
    /// Number of category blocks packed across one row of the overall yard.
    pub category_blocks_per_row: usize,
    /// Centre-to-centre distance between neighboring asset cells.
    pub cell_spacing_m: f32,
    /// Square footprint reserved for every asset before any mesh is loaded.
    pub nominal_footprint_m: f32,
    /// Clear promenade between the nominal bounds of category blocks.
    pub category_gap_m: f32,
    /// Clear ground outside the outermost category block.
    pub edge_margin_m: f32,
    /// Height of the perfectly flat gallery floor.
    pub ground_y: f32,
    /// Height above the floor used by the initial orbit/fly-camera target.
    pub camera_target_height_m: f32,
}

impl Default for AssetYardConfig {
    fn default() -> Self {
        Self {
            assets_per_row: 10,
            category_blocks_per_row: 4,
            cell_spacing_m: 16.0,
            nominal_footprint_m: 10.0,
            category_gap_m: 24.0,
            edge_margin_m: 24.0,
            ground_y: 0.0,
            camera_target_height_m: 1.6,
        }
    }
}

/// A bad custom layout setting. The default configuration is always valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetYardConfigError {
    pub field: &'static str,
    pub requirement: &'static str,
}

impl fmt::Display for AssetYardConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} must {}", self.field, self.requirement)
    }
}

impl std::error::Error for AssetYardConfigError {}

fn derived_dimension_error() -> AssetYardConfigError {
    AssetYardConfigError {
        field: "derived_dimensions",
        requirement: "remain finite",
    }
}

impl AssetYardConfig {
    /// Reject configurations that would overlap nominal footprints or erase walkable aisles.
    pub fn validate(self) -> Result<Self, AssetYardConfigError> {
        const MIN_WALKWAY_M: f32 = 2.0;

        let invalid = |field, requirement| AssetYardConfigError { field, requirement };
        if self.assets_per_row == 0 {
            return Err(invalid("assets_per_row", "be at least one"));
        }
        if self.category_blocks_per_row == 0 {
            return Err(invalid("category_blocks_per_row", "be at least one"));
        }
        if !self.cell_spacing_m.is_finite() || self.cell_spacing_m <= 0.0 {
            return Err(invalid("cell_spacing_m", "be finite and positive"));
        }
        if !self.nominal_footprint_m.is_finite() || self.nominal_footprint_m <= 0.0 {
            return Err(invalid("nominal_footprint_m", "be finite and positive"));
        }
        if self.cell_spacing_m - self.nominal_footprint_m < MIN_WALKWAY_M {
            return Err(invalid(
                "cell_spacing_m",
                "leave at least a two-metre aisle between nominal footprints",
            ));
        }
        if !self.category_gap_m.is_finite() || self.category_gap_m < MIN_WALKWAY_M {
            return Err(invalid(
                "category_gap_m",
                "be finite and leave at least a two-metre promenade",
            ));
        }
        if !self.edge_margin_m.is_finite() || self.edge_margin_m < MIN_WALKWAY_M {
            return Err(invalid(
                "edge_margin_m",
                "be finite and leave at least two metres around the yard",
            ));
        }
        if !self.ground_y.is_finite() {
            return Err(invalid("ground_y", "be finite"));
        }
        if !self.camera_target_height_m.is_finite() || self.camera_target_height_m < 0.0 {
            return Err(invalid(
                "camera_target_height_m",
                "be finite and non-negative",
            ));
        }
        Ok(self)
    }
}

/// One asset's stable slot in the yard. `source_index` indexes the slice passed to [`layout`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssetYardPlacement {
    pub source_index: usize,
    pub category_index: usize,
    pub row: usize,
    pub column: usize,
    pub position: Vec3,
    /// Cardinal rotation in editor-compatible degrees, derived from the asset identity.
    pub yaw_deg: f32,
}

/// Bounds and indexing metadata for one category mini-grid.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetYardCategory {
    pub name: String,
    /// First index in [`AssetYardLayout::placements`].
    pub first_placement: usize,
    pub placement_count: usize,
    pub rows: usize,
    pub columns: usize,
    /// Inclusive nominal X/Z bounds of the category's asset footprints.
    pub min: Vec2,
    /// Inclusive nominal X/Z bounds of the category's asset footprints.
    pub max: Vec2,
}

impl AssetYardCategory {
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }
}

/// Complete spatial plan for a flat, explorable asset gallery.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetYardLayout {
    pub placements: Vec<AssetYardPlacement>,
    pub categories: Vec<AssetYardCategory>,
    pub ground_y: f32,
    pub nominal_footprint_m: f32,
    /// Tight half-extents of all nominal asset footprints (X/Z), before the edge margin.
    pub content_half_extents: Vec2,
    /// Rectangular floor half-extents including the edge margin (X/Z).
    pub ground_half_extents: Vec2,
    /// Half-extent of a square floor guaranteed to enclose `ground_half_extents`.
    pub half_extent_m: f32,
    /// Ground-level player/tank anchor near the yard's south edge.
    pub spawn_position: Vec3,
    /// Editor-camera convention: zero faces -Z, 180 degrees faces into the yard along +Z.
    pub spawn_yaw_deg: f32,
    /// Useful initial orbit target at the geometric centre of the gallery.
    pub camera_target: Vec3,
}

impl AssetYardLayout {
    /// Nominal footprint bounds of a placed item in X/Z space.
    pub fn placement_bounds(&self, placement: &AssetYardPlacement) -> (Vec2, Vec2) {
        let half = Vec2::splat(self.nominal_footprint_m * 0.5);
        let center = Vec2::new(placement.position.x, placement.position.z);
        (center - half, center + half)
    }

    /// True when an X/Z point lies on the rectangular floor.
    pub fn contains_xz(&self, point: Vec2) -> bool {
        point.x >= -self.ground_half_extents.x
            && point.x <= self.ground_half_extents.x
            && point.y >= -self.ground_half_extents.y
            && point.y <= self.ground_half_extents.y
    }
}

#[derive(Debug)]
struct PendingCategory {
    name: String,
    source_indices: Vec<usize>,
    rows: usize,
    columns: usize,
    width: f32,
    depth: f32,
}

fn category_label(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        UNCATEGORIZED
    } else {
        trimmed
    }
}

fn stable_yaw<T: AssetYardEntry + ?Sized>(entry: &T) -> f32 {
    // FNV-1a 64, with explicit separators so concatenated fields cannot alias each other.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in [
        category_label(entry.yard_category()),
        entry.yard_name(),
        entry.yard_path(),
    ] {
        for &byte in part.as_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    ((hash >> 32) & 3) as f32 * 90.0
}

fn pending_categories<T: AssetYardEntry>(
    entries: &[T],
    config: AssetYardConfig,
) -> Vec<PendingCategory> {
    let mut ordered: Vec<usize> = (0..entries.len()).collect();
    ordered.sort_by(|&a, &b| {
        let a = &entries[a];
        let b = &entries[b];
        category_label(a.yard_category())
            .cmp(category_label(b.yard_category()))
            .then_with(|| a.yard_name().cmp(b.yard_name()))
            .then_with(|| a.yard_path().cmp(b.yard_path()))
    });

    let mut grouped: Vec<(String, Vec<usize>)> = Vec::new();
    for source_index in ordered {
        let label = category_label(entries[source_index].yard_category());
        match grouped.last_mut() {
            Some((name, indices)) if name == label => indices.push(source_index),
            _ => grouped.push((label.to_owned(), vec![source_index])),
        }
    }

    grouped
        .into_iter()
        .map(|(name, source_indices)| {
            let columns = source_indices.len().min(config.assets_per_row);
            let rows = source_indices.len().div_ceil(config.assets_per_row);
            let width = config.nominal_footprint_m
                + columns.saturating_sub(1) as f32 * config.cell_spacing_m;
            let depth =
                config.nominal_footprint_m + rows.saturating_sub(1) as f32 * config.cell_spacing_m;
            PendingCategory {
                name,
                source_indices,
                rows,
                columns,
                width,
                depth,
            }
        })
        .collect()
}

/// Build a yard using the walkable default dimensions.
///
/// Calling this directly with `&[assetlib::AssetEntry]` requires no adapter:
/// `let yard = asset_yard::layout(&library);`
pub fn layout<T: AssetYardEntry>(entries: &[T]) -> AssetYardLayout {
    try_layout(entries, AssetYardConfig::default()).expect("default asset-yard config is valid")
}

/// Build a yard with explicit physical dimensions. No paths are resolved and no meshes are loaded.
pub fn try_layout<T: AssetYardEntry>(
    entries: &[T],
    config: AssetYardConfig,
) -> Result<AssetYardLayout, AssetYardConfigError> {
    let config = config.validate()?;
    let pending = pending_categories(entries, config);
    let camera_target_y = config.ground_y + config.camera_target_height_m;
    if !camera_target_y.is_finite()
        || pending
            .iter()
            .any(|category| !category.width.is_finite() || !category.depth.is_finite())
    {
        return Err(derived_dimension_error());
    }
    if pending.is_empty() {
        let ground_half_extents = Vec2::splat(config.edge_margin_m);
        return Ok(AssetYardLayout {
            placements: Vec::new(),
            categories: Vec::new(),
            ground_y: config.ground_y,
            nominal_footprint_m: config.nominal_footprint_m,
            content_half_extents: Vec2::ZERO,
            ground_half_extents,
            half_extent_m: ground_half_extents.max_element(),
            spawn_position: Vec3::new(0.0, config.ground_y, -config.edge_margin_m * 0.5),
            spawn_yaw_deg: 180.0,
            camera_target: Vec3::new(0.0, camera_target_y, 0.0),
        });
    }

    // Every block column gets the same pitch. A short category is centered in its lane instead of
    // stretching its cells, while the fixed pitch guarantees a full promenade on both sides.
    let max_block_width = config.nominal_footprint_m
        + config.assets_per_row.saturating_sub(1) as f32 * config.cell_spacing_m;
    let block_pitch_x = max_block_width + config.category_gap_m;
    if !max_block_width.is_finite() || !block_pitch_x.is_finite() {
        return Err(derived_dimension_error());
    }
    let block_rows = pending.len().div_ceil(config.category_blocks_per_row);

    let mut placements = Vec::with_capacity(entries.len());
    let mut categories = Vec::with_capacity(pending.len());
    let mut content_min = Vec2::splat(f32::INFINITY);
    let mut content_max = Vec2::splat(f32::NEG_INFINITY);
    let mut cursor_z = 0.0f32;

    for block_row in 0..block_rows {
        let first = block_row * config.category_blocks_per_row;
        let last = (first + config.category_blocks_per_row).min(pending.len());
        let row = &pending[first..last];
        let row_depth = row
            .iter()
            .map(|category| category.depth)
            .fold(0.0, f32::max);

        for (block_column, category) in row.iter().enumerate() {
            let lane_min_x = block_column as f32 * block_pitch_x;
            let lane_center_x = lane_min_x + max_block_width * 0.5;
            let min_x = lane_center_x - category.width * 0.5;
            let max_x = lane_center_x + category.width * 0.5;
            let min_z = cursor_z;
            let max_z = cursor_z + category.depth;
            let first_placement = placements.len();

            for (local_index, &source_index) in category.source_indices.iter().enumerate() {
                let grid_row = local_index / config.assets_per_row;
                let grid_column = local_index % config.assets_per_row;
                placements.push(AssetYardPlacement {
                    source_index,
                    category_index: first + block_column,
                    row: grid_row,
                    column: grid_column,
                    position: Vec3::new(
                        min_x
                            + config.nominal_footprint_m * 0.5
                            + grid_column as f32 * config.cell_spacing_m,
                        config.ground_y,
                        min_z
                            + config.nominal_footprint_m * 0.5
                            + grid_row as f32 * config.cell_spacing_m,
                    ),
                    yaw_deg: stable_yaw(&entries[source_index]),
                });
            }

            categories.push(AssetYardCategory {
                name: category.name.clone(),
                first_placement,
                placement_count: category.source_indices.len(),
                rows: category.rows,
                columns: category.columns,
                min: Vec2::new(min_x, min_z),
                max: Vec2::new(max_x, max_z),
            });
            content_min.x = content_min.x.min(min_x);
            content_min.y = content_min.y.min(min_z);
            content_max.x = content_max.x.max(max_x);
            content_max.y = content_max.y.max(max_z);
        }

        cursor_z += row_depth;
        if block_row + 1 < block_rows {
            cursor_z += config.category_gap_m;
        }
    }

    // All construction above uses a positive quadrant because that makes block packing simple.
    // Recenter once so editor orbit controls, terrain generation, and authored placement math can
    // all retain their normal origin-centred convention.
    let centre = (content_min + content_max) * 0.5;
    for placement in &mut placements {
        placement.position.x -= centre.x;
        placement.position.z -= centre.y;
    }
    for category in &mut categories {
        category.min -= centre;
        category.max -= centre;
    }

    let content_half_extents = (content_max - content_min) * 0.5;
    let ground_half_extents = content_half_extents + Vec2::splat(config.edge_margin_m);
    let spawn_position = Vec3::new(
        0.0,
        config.ground_y,
        -content_half_extents.y - config.edge_margin_m * 0.5,
    );
    if !content_half_extents.is_finite()
        || !ground_half_extents.is_finite()
        || !spawn_position.is_finite()
        || placements
            .iter()
            .any(|placement| !placement.position.is_finite())
    {
        return Err(derived_dimension_error());
    }

    Ok(AssetYardLayout {
        placements,
        categories,
        ground_y: config.ground_y,
        nominal_footprint_m: config.nominal_footprint_m,
        content_half_extents,
        ground_half_extents,
        half_extent_m: ground_half_extents.max_element(),
        spawn_position,
        spawn_yaw_deg: 180.0,
        camera_target: Vec3::new(0.0, camera_target_y, 0.0),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Clone, Debug)]
    struct TestEntry {
        path: String,
        name: String,
        category: String,
    }

    impl AssetYardEntry for TestEntry {
        fn yard_path(&self) -> &str {
            &self.path
        }

        fn yard_name(&self) -> &str {
            &self.name
        }

        fn yard_category(&self) -> &str {
            &self.category
        }
    }

    fn entry(category: &str, name: impl Into<String>) -> TestEntry {
        let name = name.into();
        TestEntry {
            path: format!("assets/{category}/{name}.glb"),
            name,
            category: category.to_owned(),
        }
    }

    fn by_path(entries: &[TestEntry], yard: &AssetYardLayout) -> BTreeMap<String, ([f32; 3], f32)> {
        yard.placements
            .iter()
            .map(|placement| {
                (
                    entries[placement.source_index].path.clone(),
                    (placement.position.to_array(), placement.yaw_deg),
                )
            })
            .collect()
    }

    #[test]
    fn layout_is_stable_across_calls_and_input_order() {
        let entries = vec![
            entry("Props", "radio"),
            entry("Buildings", "hangar"),
            entry("Props", "barrel"),
            entry("Nature", "pine"),
            entry("Buildings", "shed"),
        ];
        let first = layout(&entries);
        assert_eq!(first, layout(&entries));

        let reordered = vec![
            entries[4].clone(),
            entries[2].clone(),
            entries[0].clone(),
            entries[3].clone(),
            entries[1].clone(),
        ];
        let second = layout(&reordered);
        assert_eq!(by_path(&entries, &first), by_path(&reordered, &second));
        assert_eq!(
            first.categories, second.categories,
            "category geometry is independent of scanner order"
        );
    }

    #[test]
    fn nominal_cell_footprints_never_overlap() {
        let entries: Vec<_> = (0..31)
            .map(|index| {
                entry(
                    if index < 19 { "Props" } else { "Structures" },
                    format!("asset_{index:02}"),
                )
            })
            .collect();
        let yard = layout(&entries);
        for (index, a) in yard.placements.iter().enumerate() {
            let (amin, amax) = yard.placement_bounds(a);
            for b in &yard.placements[index + 1..] {
                let (bmin, bmax) = yard.placement_bounds(b);
                let separated =
                    amax.x <= bmin.x || bmax.x <= amin.x || amax.y <= bmin.y || bmax.y <= amin.y;
                assert!(separated, "nominal footprints overlap: {a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn categories_have_separate_blocks_and_contiguous_placement_ranges() {
        let entries: Vec<_> = [
            ("Charlie", 13usize),
            ("Alpha", 4),
            ("Bravo", 12),
            ("Delta", 3),
            ("Echo", 14),
        ]
        .into_iter()
        .flat_map(|(category, count)| {
            (0..count).map(move |index| entry(category, format!("{category}_{index:02}")))
        })
        .collect();
        let config = AssetYardConfig {
            assets_per_row: 5,
            category_blocks_per_row: 3,
            ..Default::default()
        };
        let yard = try_layout(&entries, config).unwrap();
        assert_eq!(
            yard.categories
                .iter()
                .map(|category| category.name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "Bravo", "Charlie", "Delta", "Echo"]
        );

        for (category_index, category) in yard.categories.iter().enumerate() {
            let range =
                category.first_placement..category.first_placement + category.placement_count;
            assert!(yard.placements[range]
                .iter()
                .all(|placement| placement.category_index == category_index));
        }
        for (index, a) in yard.categories.iter().enumerate() {
            for b in &yard.categories[index + 1..] {
                let x_gap = (b.min.x - a.max.x).max(a.min.x - b.max.x);
                let z_gap = (b.min.y - a.max.y).max(a.min.y - b.max.y);
                assert!(
                    x_gap >= config.category_gap_m - 0.001
                        || z_gap >= config.category_gap_m - 0.001,
                    "category blocks are not promenade-separated: {a:?} and {b:?}"
                );
            }
        }
    }

    #[test]
    fn reported_bounds_enclose_content_spawn_and_camera_target() {
        let entries: Vec<_> = (0..57)
            .map(|index| {
                entry(
                    ["A", "B", "C", "D", "E"][index % 5],
                    format!("item_{index:02}"),
                )
            })
            .collect();
        let yard = layout(&entries);
        for placement in &yard.placements {
            let (min, max) = yard.placement_bounds(placement);
            assert!(min.x >= -yard.content_half_extents.x - 0.001);
            assert!(max.x <= yard.content_half_extents.x + 0.001);
            assert!(min.y >= -yard.content_half_extents.y - 0.001);
            assert!(max.y <= yard.content_half_extents.y + 0.001);
        }
        for category in &yard.categories {
            assert!(category.min.x >= -yard.content_half_extents.x - 0.001);
            assert!(category.max.x <= yard.content_half_extents.x + 0.001);
            assert!(category.min.y >= -yard.content_half_extents.y - 0.001);
            assert!(category.max.y <= yard.content_half_extents.y + 0.001);
        }
        assert!(yard.contains_xz(Vec2::new(yard.spawn_position.x, yard.spawn_position.z)));
        assert!(yard.contains_xz(Vec2::new(yard.camera_target.x, yard.camera_target.z)));
        assert!(yard.half_extent_m >= yard.ground_half_extents.x);
        assert!(yard.half_extent_m >= yard.ground_half_extents.y);

        let empty = layout::<TestEntry>(&[]);
        assert!(empty.placements.is_empty());
        assert!(empty.categories.is_empty());
        assert!(empty.contains_xz(Vec2::new(empty.spawn_position.x, empty.spawn_position.z)));
        assert_eq!(empty.camera_target.x, 0.0);
        assert_eq!(empty.camera_target.z, 0.0);
    }

    #[test]
    fn derived_dimension_overflow_is_rejected() {
        let entries = vec![
            entry("Props", "a"),
            entry("Props", "b"),
            entry("Props", "c"),
        ];
        let excessive_height = AssetYardConfig {
            ground_y: f32::MAX,
            camera_target_height_m: f32::MAX,
            ..Default::default()
        };
        assert_eq!(
            try_layout(&entries, excessive_height).unwrap_err(),
            derived_dimension_error()
        );

        let excessive_spacing = AssetYardConfig {
            assets_per_row: 3,
            cell_spacing_m: f32::MAX,
            nominal_footprint_m: 1.0,
            ..Default::default()
        };
        assert_eq!(
            try_layout(&entries, excessive_spacing).unwrap_err(),
            derived_dimension_error()
        );
    }
}
