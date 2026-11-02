//! Explicit expedition level authoring at build time: one cleared, graded camp pad and a
//! smooth earth apron. Both source terrain triangles and the authoritative heightfield change.
use crate::{gltf_load::CpuMesh, terrain::Heightfield};
use anyhow::{ensure, Result};
use glam::Vec2;

#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct GradeReceipt {
    pub core_min_m: [f32; 2],
    pub core_max_m: [f32; 2],
    pub apron_m: f32,
    pub elevation_m: f32,
    pub changed_native_nodes: usize,
    pub maximum_cut_fill_m: f32,
    pub native_detail: bool,
}

pub(super) fn clearance(hf: &Heightfield) -> f32 {
    2.0 * hf.spacing.max(hf.detail.as_ref().map_or(0., |d| d.spacing)) + 10.0
}

fn weight(p: Vec2, lo: Vec2, hi: Vec2, apron: f32) -> f32 {
    let outside = (lo - p).max(p - hi).max(Vec2::ZERO).length();
    let t = (1.0 - outside / apron).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn grade_nodes(
    heights: &mut [f32],
    mut surface: Option<&mut [[u8; 4]]>,
    res: usize,
    spacing: f32,
    lo: Vec2,
    hi: Vec2,
    apron: f32,
    elevation: f32,
) -> (usize, f32) {
    let half = (res - 1) as f32 * spacing * 0.5;
    let a = ((lo - Vec2::splat(apron) + Vec2::splat(half)) / spacing)
        .floor()
        .max(Vec2::ZERO)
        .as_uvec2();
    let b = ((hi + Vec2::splat(apron) + Vec2::splat(half)) / spacing)
        .ceil()
        .min(Vec2::splat((res - 1) as f32))
        .as_uvec2();
    let mut changed = 0;
    let mut maximum = 0f32;
    for z in a.y..=b.y {
        for x in a.x..=b.x {
            let p = Vec2::new(x as f32 * spacing - half, z as f32 * spacing - half);
            let w = weight(p, lo, hi, apron);
            if w <= 0. {
                continue;
            }
            let index = z as usize * res + x as usize;
            let old = heights[index];
            heights[index] = old + (elevation - old) * w;
            maximum = maximum.max((heights[index] - old).abs());
            changed += 1;
            if let Some(surface) = surface.as_deref_mut() {
                // Existing held-surface bit is authored ground, shared by native detail rendering
                // and physics. Keep the original ground class and snow material at the edge.
                if w >= 0.9999 {
                    surface[index][0] |= 0x80;
                    surface[index][1] = 0; // cleared camp floor is not an untouched snow drift
                    surface[index][2] = 255; // no old road rut through the level work area
                }
            }
        }
    }
    (changed, maximum)
}

pub(super) fn apply(
    hf: &mut Heightfield,
    ground: &mut CpuMesh,
    footprint_lo: Vec2,
    footprint_hi: Vec2,
    elevation: f32,
) -> Result<GradeReceipt> {
    ensure!(
        elevation.is_finite() && footprint_lo.is_finite() && footprint_hi.is_finite(),
        "invalid camp grading bounds"
    );
    let spacing = hf.spacing.max(hf.detail.as_ref().map_or(0., |d| d.spacing));
    let lo = footprint_lo - Vec2::splat(2. * spacing + 1.);
    let hi = footprint_hi + Vec2::splat(2. * spacing + 1.);
    let apron = 8.;
    ensure!(
        hf.openings
            .meeting(lo - Vec2::splat(apron), hi + Vec2::splat(apron))
            .is_empty(),
        "camp grading cannot cover a source cellar or trench opening"
    );
    let native_detail = hf.detail.is_some();
    let (changed, maximum) = if let Some(detail) = &hf.detail {
        let mut heights = detail.heights().to_vec();
        let mut surface = detail.surface().to_vec();
        let result = grade_nodes(
            &mut heights,
            Some(&mut surface),
            detail.res,
            detail.spacing,
            lo,
            hi,
            apron,
            elevation,
        );
        let graded = crate::terrain_relief::Detail::new(
            detail.res,
            detail.spacing,
            heights,
            surface,
            detail.wind,
            format!("{} + authored expedition camp pad", detail.source),
        )?;
        hf.attach_detail(std::sync::Arc::new(graded));
        // Existing cellar/opening geometry remains shared with the native detail renderer.
        hf.set_openings(hf.openings.list().to_vec());
        result
    } else {
        let result = grade_nodes(
            &mut hf.heights,
            None,
            hf.res,
            hf.spacing,
            lo,
            hi,
            apron,
            elevation,
        );
        hf.min_h = hf.heights.iter().copied().fold(f32::INFINITY, f32::min);
        hf.max_h = hf.heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        result
    };
    let texture = ground.texture.take();
    // Keep the whole cut/fill apron at source resolution too. Refining only the flat core
    // leaves coarse transition fans cutting across the newly curved ground outside it.
    let (mesh, _, _, _) = crate::terrain::authored_mesh_with_openings(
        hf, texture, &[(lo - Vec2::splat(apron), hi + Vec2::splat(apron))],
    );
    *ground = mesh;
    Ok(GradeReceipt {
        core_min_m: lo.to_array(),
        core_max_m: hi.to_array(),
        apron_m: apron,
        elevation_m: elevation,
        changed_native_nodes: changed,
        maximum_cut_fill_m: maximum,
        native_detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grading_changes_drawn_ground_and_collision_together_preserving_outside_nodes() {
        let res = 65usize;
        let spacing = 1.;
        let heights: Vec<_> = (0..res * res)
            .map(|i| (i % res) as f32 * 0.015 + (i / res) as f32 * 0.003)
            .collect();
        let outside = heights[0];
        let mut hf = Heightfield {
            res,
            spacing,
            heights,
            min_h: 0.,
            max_h: 2.,
            openings: Default::default(),
            detail: None,
        };
        let mut mesh = crate::terrain::authored_mesh_with_openings(&hf, None, &[]).0;
        let receipt = apply(&mut hf, &mut mesh, Vec2::splat(-8.), Vec2::splat(8.), 0.5).unwrap();
        assert!(receipt.changed_native_nodes > 0);
        assert_eq!(hf.heights[0], outside);
        for p in [Vec2::ZERO, Vec2::new(7.9, -6.3), Vec2::new(-7.2, 7.9)] {
            assert!((hf.height_at(p.x, p.y) - 0.5).abs() < 1e-6);
        }
        for vertex in &mesh.vertices {
            assert!((vertex.pos[1] - hf.height_at(vertex.pos[0], vertex.pos[2])).abs() < 1e-5);
        }
    }
    #[test]
    fn native_detail_uses_same_graded_height_without_old_relief_inside_pad() {
        let res = 65;
        let spacing = 1.;
        let mut hf = Heightfield {
            res,
            spacing,
            heights: vec![0.; res * res],
            min_h: 0.,
            max_h: 0.,
            openings: Default::default(),
            detail: None,
        };
        let detail = crate::terrain_relief::Detail::new(
            res,
            spacing,
            vec![0.; res * res],
            vec![[0, 40, 255, 128]; res * res],
            Vec2::X,
            "test source".into(),
        )
        .unwrap();
        hf.attach_detail(std::sync::Arc::new(detail));
        let mut mesh = crate::terrain::authored_mesh_with_openings(&hf, None, &[]).0;
        apply(&mut hf, &mut mesh, Vec2::splat(-8.), Vec2::splat(8.), 0.2).unwrap();
        for p in [Vec2::ZERO, Vec2::new(7.25, -7.75)] {
            assert!((hf.height_at(p.x, p.y) - 0.2).abs() < 1e-5);
            assert_eq!(hf.detail.as_ref().unwrap().snow_depth_at(p.x, p.y), 0.);
        }
    }
}
