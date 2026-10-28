//! The actual Gaea survey drawing, uploaded once and printed onto the folded, normally lit paper.
//! Runtime checks the measured route against the artwork's provenance so another course never
//! silently inherits the wrong geography. No enemy or live-player coordinates are printed.
use super::*;
use crate::ui::Align;
const META: &str = "catalog/maps/canyon_survey_20260930.json";

pub(super) fn load(
    r: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    spec: Option<&crate::canyon::CourseSpec>,
    v2: bool,
) -> anyhow::Result<Option<(u32, [f32; 4])>> {
    let Some(spec) = spec else { return Ok(None) };
    let metadata=if v2 {"catalog/maps/canyon_v2_survey_20260930.json"} else {META};
    if !std::path::Path::new(metadata).exists() {
        return Ok(None);
    }
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(metadata)?)?;
    let route: Vec<[f32; 2]> = serde_json::from_value(meta["route"].clone())?;
    if route.len() != spec.route.len()
        || route
            .iter()
            .zip(&spec.route)
            .any(|(a, b)| Vec2::from(*a).distance(*b) > 0.01)
    {
        return Ok(None);
    }
    let image_path = meta["image"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("canyon survey image missing"))?;
    let bounds: [f32; 4] = serde_json::from_value(meta["bounds_xz"].clone())
        .map_err(|e| anyhow::anyhow!("canyon survey bounds_xz: {e}"))?;
    let img = image::open(image_path)?.into_rgba8();
    Ok(Some((r.upload_image(
        device,
        queue,
        img.as_raw(),
        img.width(),
        img.height(),
    ), bounds)))
}

/// The survey drawing's plan projection: world (x, z) to the image's normalized (u, v). The
/// drawing (tools/worldgen/canyon_paper_map.py) places an equal-aspect axes box of the bounds
/// at [0.035, 0.045, 0.93, 0.91] of a 16 x 9.6 figure; the bounds fill its height and are centred
/// in its width. North is -Z, at the top.
pub fn survey_uv(bounds: [f32; 4], p: Vec2) -> (f32, f32) {
    let [xmin, xmax, zmin, zmax] = bounds;
    let (box_w, box_h) = (0.93 * 16.0, 0.91 * 9.6);
    let scale = box_h / (zmax - zmin).max(1.0);
    let data_w = (xmax - xmin) * scale;
    let left = (0.035 * 16.0 + (box_w - data_w) * 0.5) / 16.0;
    let u = left + (p.x - xmin) * scale / 16.0;
    let v = 0.045 + (p.y - zmin) / (zmax - zmin).max(1.0) * 0.91;
    (u, v)
}

impl Game {
    /// The held sheet's normalized (u, v) of a world point (the survey fills the printed rect).
    pub fn canyon_survey_uv(&self, p: Vec2) -> (f32, f32) {
        self.canyon_paper.map_or((0.5, 0.5), |(_, bounds)| survey_uv(bounds, p))
    }

    pub fn draw_canyon_map(
        &self,
        r: &mut Renderer,
        pencil: &[(u8, Vec2)],
        pins: &[(u8, Vec2)],
        cursor: Vec2,
    ) -> bool {
        let Some((image, bounds)) = self.canyon_paper else {
            return false;
        };
        r.draw_image(image, 0.17, 0.34, 0.66, 0.59, [1.0; 4]);
        // The mission's marks (src/mission.rs) in the survey's own projection.
        if let Some(guidance) = self.mission_map_guidance() {
            let proj = |p: Vec2| { let (u, v) = survey_uv(bounds, p); (0.17 + u * 0.66, 0.34 + v * 0.59) };
            crate::ui::draw_mission_marks(r, &guidance, &proj, (0.17, 0.34, 0.66, 0.59));
        }
        for &(_, p) in pencil.iter().filter(|(act, _)| *act == self.act.id) {
            r.fill_rect(
                0.17 + p.x * 0.66,
                0.34 + p.y * 0.59,
                0.0018,
                0.0024,
                [0.18, 0.16, 0.13, 0.90],
            );
        }
        for &(_, p) in pins.iter().filter(|(act, _)| *act == self.act.id) {
            r.draw_text(
                "x",
                0.17 + p.x * 0.66,
                0.34 + p.y * 0.59,
                0.022,
                [0.58, 0.08, 0.065, 1.0],
                Align::Center,
            );
        }
        r.fill_rect(
            0.17 + cursor.x * 0.66,
            0.34 + cursor.y * 0.59,
            0.0015,
            0.0025,
            [0.2, 0.18, 0.14, 0.80],
        );
        true
    }
}
