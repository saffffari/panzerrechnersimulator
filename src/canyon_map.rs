//! Monochrome line survey on the folded, normally lit paper. The survey's coordinate registration
//! is retained, but building outlines come from the live building compiler rather than colored
//! artwork. No enemy locations are invented. The owner's exact player-position aid is explicit.
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
    let bounds: [f32; 4] = serde_json::from_value(meta["bounds_xz"].clone())
        .map_err(|e| anyhow::anyhow!("canyon survey bounds_xz: {e}"))?;
    // Keep the existing image-handle/bounds integration contract. Actual ink is generated from
    // current geometry below; the original source artwork remains untouched on disk.
    Ok(Some((r.upload_image(
        device,
        queue,
        &[230,230,230,255],
        1,
        1,
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
        let Some((_, bounds)) = self.canyon_paper else {
            return false;
        };
        use crate::ui::map_linework::{self,INK,PAPER,FADED};
        let paper=(0.17,0.34,0.66,0.59);
        let proj = |p: Vec2| {let (u,v)=survey_uv(bounds,p);(paper.0+u*paper.2,paper.1+v*paper.3)};
        r.fill_rect(paper.0,paper.1,paper.2,paper.3,PAPER);
        map_linework::draw_buildings(r,&self.buildings,&proj,&|_|true,paper);
        // This course is a snow corridor. Its prescribed driving route is not a surveyed road,
        // so it must not masquerade as one on a road-and-building map.
        r.draw_text("N",0.795,0.43,0.024,INK,Align::Center);
        r.draw_text("^",0.795,0.455,0.026,INK,Align::Center);
        if let Some(spec)=&self.canyon_spec {
            let (x,y)=proj(spec.start);r.draw_text("+",x,y,0.026,FADED,Align::Center);
            let (x,y)=proj(spec.goal);r.draw_text_bold("X",x,y,0.030,INK,Align::Center);
        }
        // The mission's marks (src/mission.rs) in the survey's own projection.
        if let Some(guidance) = self.mission_map_guidance() {
            crate::ui::draw_mission_marks(r, &guidance, &proj, (0.17, 0.34, 0.66, 0.59));
        }
        map_linework::player_marker(r,proj(self.map_world_position()),paper);
        for &(_, p) in pencil.iter().filter(|(act, _)| *act == self.act.id) {
            r.fill_rect(
                0.17 + p.x * 0.66,
                0.34 + p.y * 0.59,
                0.0018,
                0.0024,
                [0.16, 0.16, 0.16, 0.90],
            );
        }
        for &(_, p) in pins.iter().filter(|(act, _)| *act == self.act.id) {
            r.draw_text(
                "x",
                0.17 + p.x * 0.66,
                0.34 + p.y * 0.59,
                0.022,
                INK,
                Align::Center,
            );
        }
        r.fill_rect(
            0.17 + cursor.x * 0.66,
            0.34 + cursor.y * 0.59,
            0.0015,
            0.0025,
            [0.2, 0.2, 0.2, 0.80],
        );
        true
    }

    /// Add geometry-derived footprints to the generic act survey. Call after its paper/road ink
    /// and before `render_held_map_texture`; canyon and winter already draw their own outlines.
    pub fn draw_campaign_building_map(&self,r:&mut Renderer){
        let half=self.act.half_extent_m.max(1.0);
        let project=|p:Vec2|(0.17+0.66*(0.50+p.x/half*0.39),0.34+0.59*(0.51+p.y/half*0.38));
        let visible=|p:Vec2| {
            let Some((route,sheets))=self.campaign_map_state()else{return true;};
            let start=Vec2::new(route.start.x as f32,route.start.z as f32);
            let near=|c:Vec2,x:f32,z:f32|(p.x-c.x).abs()<=half*x&&(p.y-c.y).abs()<=half*z;
            near(start,0.27,0.28)||route.sites.iter().take(usize::from(sheets.saturating_add(1).min(3)))
                .any(|site|near(Vec2::new(site.position.x as f32,site.position.z as f32),0.37,0.37))
        };
        crate::ui::map_linework::draw_buildings(r,&self.buildings,&project,&visible,(0.17,0.34,0.66,0.59));
        crate::ui::map_linework::player_marker(r,project(self.map_world_position()),(0.17,0.34,0.66,0.59));
    }
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn measured_survey_projection_keeps_north_up_and_positions_registered(){
        let bounds=[-1150.,2000.,-920.,1120.];
        let (u,v)=survey_uv(bounds,Vec2::new(425.,100.));assert!((u-0.5).abs()<1e-6);assert!((v-0.5).abs()<1e-6);
        assert!(survey_uv(bounds,Vec2::new(425.,0.)).1<v);
        assert!(survey_uv(bounds,Vec2::new(525.,100.)).0>u);
    }
}
