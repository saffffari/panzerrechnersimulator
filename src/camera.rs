//! Camera → view-projection. glTF/engine convention: Y-up, right-handed, -Z forward.

use glam::{Mat4, Vec3};

#[derive(Clone, Copy)]
pub struct Camera {
    pub pos: Vec3,
    pub yaw: f32,   // radians; 0 looks down -Z, +yaw turns toward +X
    pub pitch: f32, // radians; + looks up
    pub fov_y: f32,
    pub aspect: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 60f32.to_radians(),
            aspect,
            // Interior props sit centimetres from the eye while squeezing past the breech/cupola.
            // 10 cm visibly sliced the hatch rim and held tools; 2.5 cm keeps them intact without
            // giving up too much depth precision across the 6 km battlefield.
            znear: 0.025,
            zfar: 6000.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(sy * cp, sp, -cy * cp).normalize()
    }

    #[allow(dead_code)] // for driver-view strafing / free-fly
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    /// Point the camera along a world direction (used to look down the gun).
    pub fn set_forward(&mut self, dir: Vec3) {
        let d = dir.normalize_or_zero();
        if d.length_squared() < 1e-6 {
            return;
        }
        self.yaw = d.x.atan2(-d.z);
        self.pitch = d.y.clamp(-0.999, 0.999).asin();
    }

    pub fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_to_rh(self.pos, self.forward(), Vec3::Y);
        // wgpu NDC z in [0,1] → perspective_rh (not the _gl variant).
        let proj = Mat4::perspective_rh(self.fov_y, self.aspect.max(0.01), self.znear, self.zfar);
        proj * view
    }
}
