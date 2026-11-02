//! Conservative source-surface range queries for exact vertex support pruning.
use super::*;
use glam::Vec2;

pub(crate) fn round_bound(height: f32, magnitude: f32) -> f32 {
    // Bounds are not contact geometry. Cover the f32 triangle interpolation's rounding.
    height + 16.0 * f32::EPSILON * magnitude.max(1.0)
}
impl Heightfield {
    pub(crate) fn grade_upper_bound(&self, min: Vec2, max: Vec2) -> f32 {
        if !min.is_finite() || !max.is_finite() || min.cmpgt(max).any()
            || self.res < 2 || !self.spacing.is_finite() || self.spacing <= 0.0 {
            return f32::INFINITY;
        }
        if let Some(detail) = &self.detail { return detail.height_upper_bound(min, max); }
        let half = (self.res as f32 - 1.0) * 0.5;
        let last = (self.res - 2) as f32;
        let lower = (min / self.spacing + Vec2::splat(half)).floor().clamp(Vec2::ZERO, Vec2::splat(last));
        let upper = (max / self.spacing + Vec2::splat(half)).floor().clamp(Vec2::ZERO, Vec2::splat(last));
        let mut high = f32::NEG_INFINITY;
        let mut magnitude = 1.0f32;
        // Each interpolation is a convex combination of the vertices of one of these cells.
        for z in lower.y as usize..=upper.y as usize + 1 {
            for x in lower.x as usize..=upper.x as usize + 1 {
                let h = self.heights[z * self.res + x];
                if !h.is_finite() { return f32::INFINITY; }
                high = high.max(h); magnitude = magnitude.max(h.abs());
            }
        }
        round_bound(high, magnitude)
    }
    pub(crate) fn height_upper_bound(&self, min: Vec2, max: Vec2) -> f32 {
        self.opening_upper_bound(min, max, self.grade_upper_bound(min, max))
    }
    pub(crate) fn vehicle_height_upper_bound(&self, min: Vec2, max: Vec2) -> f32 {
        self.opening_upper_bound(min, max, self.grade_upper_bound(min, max) - vehicle_sinkage_m())
    }
    fn opening_upper_bound(&self, min: Vec2, max: Vec2, mut bound: f32) -> f32 {
        for opening in self.openings.list() {
            if min.x <= opening.max.x && max.x >= opening.min.x
                && min.y <= opening.max.y && max.y >= opening.min.y {
                bound = bound.max(round_bound(opening.floor_y, opening.floor_y.abs()));
            }
        }
        bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rectangle_bound_contains_grid_triangles_boundaries_and_opening_floors() {
        let mut hf = Heightfield {res:4,spacing:1.0,heights:vec![0.,2.,-1.,5.,3.,-7.,6.,1.,-2.,4.,2.,7.,0.,1.,-4.,0.],
            min_h:-7.,max_h:7.,detail:None,openings:Default::default()};
        hf.openings=crate::terrain_opening::TerrainOpenings::new(vec![crate::terrain_opening::TerrainOpening{
            min:Vec2::new(-0.2,-0.2),max:Vec2::new(0.4,0.3),floor_y:9.,base_y:8.,floor_drawn:true}]);
        for (min,max) in [(Vec2::splat(-10.),Vec2::splat(-5.)),(Vec2::splat(-0.5),Vec2::splat(0.5)),
            (Vec2::new(-1.2,0.1),Vec2::new(0.2,1.1)),(Vec2::splat(5.),Vec2::splat(10.))] {
            let upper=hf.height_upper_bound(min,max);
            let vehicle_upper=hf.vehicle_height_upper_bound(min,max);
            for j in 0..=40 {for i in 0..=40 {
                let p=min+(max-min)*Vec2::new(i as f32/40.,j as f32/40.);
                assert!(hf.height_at(p.x,p.y)<=upper);
                assert!(hf.vehicle_height_at(p.x,p.y)<=vehicle_upper);
            }}
        }
        assert_eq!(hf.height_upper_bound(Vec2::splat(f32::NAN),Vec2::ONE),f32::INFINITY);
    }
}
