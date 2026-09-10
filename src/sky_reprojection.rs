//! Rotation-only environment history. No world positions, near plane or far plane participate.
use glam::{Mat4, Vec3, Vec4};

pub fn matrix(current_forward: Vec3, current_fov: f32, current_aspect: f32,
              previous_forward: Vec3, previous_fov: f32, previous_aspect: f32) -> Mat4 {
    if current_forward == previous_forward && current_fov == previous_fov
        && current_aspect == previous_aspect {
        return Mat4::IDENTITY;
    }
    let forward = current_forward.normalize_or_zero();
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(forward);
    let previous_forward = previous_forward.normalize_or_zero();
    let previous_right = previous_forward.cross(Vec3::Y).normalize_or_zero();
    let previous_up = previous_right.cross(previous_forward);
    let scale = (current_fov * 0.5).tan();
    let previous_scale = (previous_fov.max(0.001) * 0.5).tan();
    let project = |ray: Vec3| Vec4::new(
        ray.dot(previous_right) / (previous_scale * previous_aspect.max(0.01)),
        ray.dot(previous_up) / previous_scale,
        ray.dot(previous_forward), 0.0);
    Mat4::from_cols(project(right * (scale * current_aspect.max(0.01))),
                    project(up * scale), project(forward), Vec4::ZERO)
}
