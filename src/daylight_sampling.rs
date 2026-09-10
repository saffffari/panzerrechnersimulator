//! Geometry-only proposal table. These triangles carry area, never radiance or visibility.
use glam::Vec3;
pub const MAX_TRIANGLES: usize = 64;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ProposalTriangle {
    pub a_area: [f32; 4],
    pub b_cumulative: [f32; 4],
    pub c: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Proposals {
    pub meta: [f32; 4], // count, summed area, sampling radius, reserved
    pub center: [f32; 4],
    pub triangles: [ProposalTriangle; MAX_TRIANGLES],
}
impl Proposals {
    pub fn from_triangles(triangles: impl IntoIterator<Item = [Vec3; 3]>) -> Self {
        use bytemuck::Zeroable;
        let mut candidates: Vec<_> = triangles
            .into_iter()
            .filter_map(|p| {
                let area = (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5;
                (p.iter().all(|v| v.is_finite()) && area.is_finite() && area > 1e-10)
                    .then_some((area, p))
            })
            .collect();
        // Bounded proposal work, without simplifying or expanding any selected source triangle.
        // The baseline half of the mixture retains support even for omitted tiny triangles.
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        candidates.truncate(MAX_TRIANGLES);
        let mut out = Self::zeroed();
        let mut total = 0.0;
        let mut center = Vec3::ZERO;
        for (i, (area, p)) in candidates.iter().enumerate() {
            total += area;
            center += (p[0] + p[1] + p[2]) * (*area / 3.0);
            out.triangles[i] = ProposalTriangle {
                a_area: p[0].extend(*area).to_array(),
                b_cumulative: p[1].extend(total).to_array(),
                c: p[2].extend(0.0).to_array(),
            };
        }
        out.meta = [candidates.len() as f32, total, 6.5, 0.0];
        if total > 0.0 {
            out.center = (center / total).extend(0.0).to_array();
        }
        out
    }
}
