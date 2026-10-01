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
) -> anyhow::Result<Option<u32>> {
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
    let img = image::open(image_path)?.into_rgba8();
    Ok(Some(r.upload_image(
        device,
        queue,
        img.as_raw(),
        img.width(),
        img.height(),
    )))
}

impl Game {
    pub fn draw_canyon_map(
        &self,
        r: &mut Renderer,
        pencil: &[(u8, Vec2)],
        pins: &[(u8, Vec2)],
        cursor: Vec2,
    ) -> bool {
        let Some(image) = self.canyon_paper else {
            return false;
        };
        r.draw_image(image, 0.17, 0.34, 0.66, 0.59, [1.0; 4]);
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
