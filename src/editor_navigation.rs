//! Engine-agnostic navigation state for the worldbuilder.
//!
//! The editor owns the mapping from keys, mouse motion, and wheel events into these helpers. This
//! module only deals in `glam` values, which keeps the navigation rules reusable by the live game
//! view, preview renderer, and headless editor tests.

use glam::{Mat4, Vec2, Vec3};

use crate::{interior_rig, tank::Tank};

/// The three ways the worldbuilder can be inhabited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorViewMode {
    /// Unreal-style camera orbiting a point of interest.
    #[default]
    Orbit,
    /// Grounded first-person inspection on foot.
    Walk,
    /// A live Tiger with the camera mounted at one of its crew stations.
    Tiger,
}

/// The transform an object placed inside the Tiger should inherit.
///
/// Hull children follow driving and steering only. Turret children additionally follow traverse;
/// this is the important distinction for props placed in the fighting compartment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorAttachmentFrame {
    Hull,
    Turret,
}

impl EditorAttachmentFrame {
    /// The live parent transform for an object stored in this attachment frame.
    #[must_use]
    pub fn parent_model(self, tank: &Tank) -> Mat4 {
        match self {
            Self::Hull => tank.hull_model(),
            Self::Turret => tank.turret_model(),
        }
    }

    /// Convert an editor-local object transform into its live world transform.
    #[must_use]
    pub fn to_world(self, tank: &Tank, local_model: Mat4) -> Mat4 {
        self.parent_model(tank) * local_model
    }

    /// Convert a world-space placement into the local transform that should be persisted.
    #[must_use]
    pub fn to_local(self, tank: &Tank, world_model: Mat4) -> Mat4 {
        self.parent_model(tank).inverse() * world_model
    }

    /// Bridge to the authored Tiger interior rig's frame type.
    #[must_use]
    pub const fn interior_frame(self) -> interior_rig::Frame {
        match self {
            Self::Hull => interior_rig::Frame::Hull,
            Self::Turret => interior_rig::Frame::Turret,
        }
    }
}

/// The five playable Tiger crew stations exposed by the editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorSeat {
    #[default]
    Commander,
    Gunner,
    Loader,
    Driver,
    Radioman,
}

impl EditorSeat {
    pub const ALL: [Self; 5] = [
        Self::Commander,
        Self::Gunner,
        Self::Loader,
        Self::Driver,
        Self::Radioman,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Commander => "Commander",
            Self::Gunner => "Gunner",
            Self::Loader => "Loader",
            Self::Driver => "Driver",
            Self::Radioman => "Radioman",
        }
    }

    /// The frame a prop placed from this station should use by default.
    #[must_use]
    pub const fn attachment_frame(self) -> EditorAttachmentFrame {
        match self {
            Self::Driver | Self::Radioman => EditorAttachmentFrame::Hull,
            Self::Commander | Self::Gunner | Self::Loader => EditorAttachmentFrame::Turret,
        }
    }

    /// The live transform inherited by props placed at this station.
    #[must_use]
    pub fn parent_model(self, tank: &Tank) -> Mat4 {
        self.attachment_frame().parent_model(tank)
    }
}

/// A renderer-neutral camera pose. `forward` and `up` are normalized and orthogonal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorCameraPose {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
}

impl EditorCameraPose {
    /// Camera-right in world space.
    #[must_use]
    pub fn right(self) -> Vec3 {
        self.forward.cross(self.up).normalize_or_zero()
    }

    /// A right-handed view matrix suitable for the existing renderer.
    #[must_use]
    pub fn view_matrix(self) -> Mat4 {
        Mat4::look_to_rh(self.position, self.forward, self.up)
    }
}

pub const ORBIT_MIN_DISTANCE: f32 = 0.25;
pub const ORBIT_MAX_DISTANCE: f32 = 6_000.0;
pub const ORBIT_ZOOM_PER_STEP: f32 = 0.16;
const MAX_LOOK_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.01;

/// State for an Unreal-style point-of-interest orbit camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCameraState {
    pub pivot: Vec3,
    pub distance: f32,
    /// Camera convention shared with `camera.rs`: zero looks along -Z; positive turns toward +X.
    pub yaw: f32,
    /// Positive pitch looks upward.
    pub pitch: f32,
}

impl Default for OrbitCameraState {
    fn default() -> Self {
        Self {
            pivot: Vec3::ZERO,
            distance: 12.0,
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: -25.0_f32.to_radians(),
        }
    }
}

impl OrbitCameraState {
    #[must_use]
    pub fn new(pivot: Vec3, distance: f32) -> Self {
        let mut state = Self {
            pivot,
            ..Self::default()
        };
        state.set_distance(distance);
        state
    }

    /// Unit direction from the camera toward the orbit pivot.
    #[must_use]
    pub fn forward(self) -> Vec3 {
        camera_forward(self.yaw, self.pitch)
    }

    #[must_use]
    pub fn position(self) -> Vec3 {
        self.pivot - self.forward() * self.distance
    }

    #[must_use]
    pub fn pose(self) -> EditorCameraPose {
        camera_pose(self.position(), self.forward(), 60.0_f32.to_radians())
    }

    /// Rotate by `(yaw, pitch)` radians. Pitch stops just short of the poles so panning remains
    /// well-defined.
    pub fn rotate(&mut self, delta_radians: Vec2) {
        if delta_radians.x.is_finite() {
            self.yaw = wrap_angle(self.yaw + delta_radians.x);
        }
        if delta_radians.y.is_finite() {
            self.pitch = (self.pitch + delta_radians.y).clamp(-MAX_LOOK_PITCH, MAX_LOOK_PITCH);
        }
    }

    /// Pan the pivot in camera-plane world units: X is right and Y is up on screen.
    pub fn pan(&mut self, delta_world: Vec2) {
        if !delta_world.is_finite() {
            return;
        }
        let forward = self.forward();
        let right = safe_camera_right(forward);
        let up = right.cross(forward).normalize_or_zero();
        self.pivot += right * delta_world.x + up * delta_world.y;
    }

    /// Dolly exponentially. Positive steps zoom in and negative steps zoom out.
    pub fn zoom(&mut self, steps: f32) {
        if !steps.is_finite() {
            return;
        }
        let scale = (-steps.clamp(-40.0, 40.0) * ORBIT_ZOOM_PER_STEP).exp();
        self.set_distance(self.distance * scale);
    }

    pub fn set_distance(&mut self, distance: f32) {
        if distance.is_finite() {
            self.distance = distance.clamp(ORBIT_MIN_DISTANCE, ORBIT_MAX_DISTANCE);
        }
    }
}

pub const WALK_EYE_HEIGHT: f32 = 1.72;
pub const WALK_SPEED: f32 = 3.8;

/// A first-person body whose authoritative position is its point of ground contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundedWalkState {
    pub feet: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub eye_height: f32,
    pub speed: f32,
}

impl Default for GroundedWalkState {
    fn default() -> Self {
        Self {
            feet: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            eye_height: WALK_EYE_HEIGHT,
            speed: WALK_SPEED,
        }
    }
}

impl GroundedWalkState {
    #[must_use]
    pub fn new(feet: Vec3) -> Self {
        Self {
            feet,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn eye_position(self) -> Vec3 {
        self.feet + Vec3::Y * self.eye_height.max(0.0)
    }

    #[must_use]
    pub fn forward(self) -> Vec3 {
        camera_forward(self.yaw, self.pitch)
    }

    #[must_use]
    pub fn pose(self) -> EditorCameraPose {
        camera_pose(self.eye_position(), self.forward(), 68.0_f32.to_radians())
    }

    pub fn rotate(&mut self, delta_radians: Vec2) {
        if delta_radians.x.is_finite() {
            self.yaw = wrap_angle(self.yaw + delta_radians.x);
        }
        if delta_radians.y.is_finite() {
            self.pitch = (self.pitch + delta_radians.y).clamp(-MAX_LOOK_PITCH, MAX_LOOK_PITCH);
        }
    }

    /// Re-ground the body without changing X/Z. Non-finite terrain samples are ignored.
    pub fn snap_to_ground(&mut self, mut ground_height: impl FnMut(f32, f32) -> f32) {
        let y = ground_height(self.feet.x, self.feet.z);
        if y.is_finite() {
            self.feet.y = y;
        }
    }

    /// Move relative to the view on the horizontal plane, then sample the ground at the new point.
    /// `local_motion.x` is strafe-right and `local_motion.y` is forward; diagonal input is capped
    /// to the same speed as cardinal movement.
    pub fn walk(
        &mut self,
        local_motion: Vec2,
        dt: f32,
        ground_height: impl FnMut(f32, f32) -> f32,
    ) {
        if !local_motion.is_finite() || !dt.is_finite() || dt <= 0.0 {
            self.snap_to_ground(ground_height);
            return;
        }

        let local_motion = local_motion.clamp_length_max(1.0);
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let forward = Vec3::new(sin_yaw, 0.0, -cos_yaw);
        let right = forward.cross(Vec3::Y);
        let wish = right * local_motion.x + forward * local_motion.y;
        self.feet += wish * self.speed.max(0.0) * dt;
        self.snap_to_ground(ground_height);
    }
}

// These match the live `Game::station_eye` / couch crew anchors. Keeping them here makes editor
// previews and in-game seats agree while leaving the authoritative articulation on `Tank`.
pub const TIGER_COMMANDER_EYE_TURRET: Vec3 = Vec3::new(0.564, 2.457, -0.423);
pub const TIGER_DRIVER_EYE_HULL: Vec3 = Vec3::new(0.535, 1.62, 1.55);
pub const TIGER_RADIOMAN_EYE_HULL: Vec3 = Vec3::new(-0.66, 1.58, 1.48);
pub const TIGER_LOADER_FEET_TURRET: Vec3 = Vec3::new(-0.85, interior_rig::FLOOR_STAND_Y, 0.15);
pub const TIGER_LOADER_EYE_TURRET: Vec3 = Vec3::new(
    TIGER_LOADER_FEET_TURRET.x,
    interior_rig::FLOOR_STAND_Y + 1.29,
    TIGER_LOADER_FEET_TURRET.z,
);

/// Camera pose at the selected Tiger station with its neutral, gameplay-matching view direction.
#[must_use]
pub fn tiger_seat_camera(tank: &Tank, seat: EditorSeat) -> EditorCameraPose {
    tiger_seat_camera_with_look(tank, seat, Vec2::ZERO)
}

/// Camera pose at a Tiger station with editor free-look.
///
/// `look_radians.x` turns right and `look_radians.y` looks up in the seat's attachment frame. The
/// gunner stays locked to the laid gun because that station represents the actual TZF optic.
#[must_use]
pub fn tiger_seat_camera_with_look(
    tank: &Tank,
    seat: EditorSeat,
    look_radians: Vec2,
) -> EditorCameraPose {
    let look = if look_radians.is_finite() {
        Vec2::new(
            wrap_angle(look_radians.x),
            look_radians.y.clamp(-MAX_LOOK_PITCH, MAX_LOOK_PITCH),
        )
    } else {
        Vec2::ZERO
    };

    match seat {
        EditorSeat::Gunner => camera_pose(
            tank.gunsight(),
            tank.gun_model().transform_vector3(Vec3::Z),
            25.0_f32.to_radians(),
        ),
        EditorSeat::Commander => seat_frame_camera(
            tank.turret_model(),
            TIGER_COMMANDER_EYE_TURRET,
            look,
            64.0_f32.to_radians(),
        ),
        EditorSeat::Loader => seat_frame_camera(
            tank.turret_model(),
            TIGER_LOADER_EYE_TURRET,
            look,
            62.0_f32.to_radians(),
        ),
        EditorSeat::Driver => seat_frame_camera(
            tank.hull_model(),
            TIGER_DRIVER_EYE_HULL,
            look,
            50.0_f32.to_radians(),
        ),
        EditorSeat::Radioman => seat_frame_camera(
            tank.hull_model(),
            TIGER_RADIOMAN_EYE_HULL,
            look,
            54.0_f32.to_radians(),
        ),
    }
}

fn seat_frame_camera(
    parent: Mat4,
    local_eye: Vec3,
    look_radians: Vec2,
    fov_y: f32,
) -> EditorCameraPose {
    let base_forward = parent.transform_vector3(Vec3::Z).normalize_or_zero();
    let base_up = parent.transform_vector3(Vec3::Y).normalize_or_zero();
    // Camera-right for the Tiger's +Z-forward frame is local -X. Building free-look from the
    // transformed basis preserves that convention at every hull heading and turret traverse.
    let base_right = base_forward.cross(base_up).normalize_or_zero();
    let (sin_yaw, cos_yaw) = look_radians.x.sin_cos();
    let (sin_pitch, cos_pitch) = look_radians.y.sin_cos();
    let horizontal = base_forward * cos_yaw + base_right * sin_yaw;
    let forward = horizontal * cos_pitch + base_up * sin_pitch;
    camera_pose(parent.transform_point3(local_eye), forward, fov_y)
}

fn camera_pose(position: Vec3, forward: Vec3, fov_y: f32) -> EditorCameraPose {
    let forward = if forward.is_finite() && forward.length_squared() > 1e-8 {
        forward.normalize()
    } else {
        Vec3::NEG_Z
    };
    let right = safe_camera_right(forward);
    let up = right.cross(forward).normalize_or_zero();
    EditorCameraPose {
        position,
        forward,
        up,
        fov_y,
    }
}

fn camera_forward(yaw: f32, pitch: f32) -> Vec3 {
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    Vec3::new(sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch).normalize_or_zero()
}

fn safe_camera_right(forward: Vec3) -> Vec3 {
    let right = forward.cross(Vec3::Y);
    if right.length_squared() > 1e-8 {
        right.normalize()
    } else {
        Vec3::X
    }
}

fn wrap_angle(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{tank::TIGER_PARTS, terrain::Heightfield};

    fn flat_ground() -> Heightfield {
        Heightfield {
            res: 2,
            spacing: 10.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
        }
    }

    fn assert_vec3_near(actual: Vec3, expected: Vec3) {
        assert!(
            actual.distance(expected) < 1e-5,
            "actual {actual:?}, expected {expected:?}"
        );
    }

    #[test]
    fn orbit_pose_keeps_the_requested_distance() {
        let mut orbit = OrbitCameraState::new(Vec3::new(8.0, 2.0, -3.0), 20.0);
        orbit.rotate(Vec2::new(0.7, -0.2));
        orbit.pan(Vec2::new(3.5, -1.25));
        orbit.zoom(2.0);

        let pose = orbit.pose();
        assert!((pose.position.distance(orbit.pivot) - orbit.distance).abs() < 1e-5);

        orbit.set_distance(0.0);
        assert_eq!(orbit.distance, ORBIT_MIN_DISTANCE);
        orbit.set_distance(f32::MAX);
        assert_eq!(orbit.distance, ORBIT_MAX_DISTANCE);
    }

    #[test]
    fn grounded_walk_keeps_the_eye_at_authored_height() {
        let mut walk = GroundedWalkState::new(Vec3::new(4.0, 99.0, -3.0));
        walk.snap_to_ground(|x, z| x * 0.25 - z * 0.5);
        assert!((walk.eye_position().y - walk.feet.y - WALK_EYE_HEIGHT).abs() < 1e-6);

        walk.walk(Vec2::new(0.5, 1.0), 0.5, |x, z| x * 0.25 - z * 0.5);
        let expected_ground = walk.feet.x * 0.25 - walk.feet.z * 0.5;
        assert!((walk.feet.y - expected_ground).abs() < 1e-6);
        assert!((walk.eye_position().y - expected_ground - WALK_EYE_HEIGHT).abs() < 1e-6);
    }

    #[test]
    fn seat_parent_and_camera_poses_follow_the_real_tank_rig() {
        let mut tank = Tank::new(12.0, -7.0, 0.63, &flat_ground(), TIGER_PARTS);
        tank.turret_yaw = -0.41;
        tank.gun_pitch = 0.12;

        assert_eq!(
            EditorSeat::Driver.attachment_frame(),
            EditorAttachmentFrame::Hull
        );
        assert_eq!(
            EditorSeat::Radioman.attachment_frame(),
            EditorAttachmentFrame::Hull
        );
        assert_eq!(
            EditorSeat::Commander.attachment_frame(),
            EditorAttachmentFrame::Turret
        );

        let hull = tank.hull_model();
        let turret = tank.turret_model();
        assert_vec3_near(
            EditorSeat::Driver
                .parent_model(&tank)
                .transform_point3(Vec3::ZERO),
            hull.transform_point3(Vec3::ZERO),
        );
        assert_vec3_near(
            EditorSeat::Loader
                .parent_model(&tank)
                .transform_point3(Vec3::ZERO),
            turret.transform_point3(Vec3::ZERO),
        );

        let driver = tiger_seat_camera(&tank, EditorSeat::Driver);
        assert_vec3_near(
            driver.position,
            hull.transform_point3(TIGER_DRIVER_EYE_HULL),
        );
        assert_vec3_near(driver.forward, tank.forward());

        let commander = tiger_seat_camera(&tank, EditorSeat::Commander);
        assert_vec3_near(
            commander.position,
            turret.transform_point3(TIGER_COMMANDER_EYE_TURRET),
        );
        assert_vec3_near(commander.forward, tank.turret_forward());

        let gunner = tiger_seat_camera(&tank, EditorSeat::Gunner);
        assert_vec3_near(gunner.position, tank.gunsight());
        assert_vec3_near(
            gunner.forward,
            tank.gun_model().transform_vector3(Vec3::Z).normalize(),
        );

        let local_prop = Mat4::from_translation(Vec3::new(-0.3, 1.1, 0.4));
        let world_prop = EditorAttachmentFrame::Turret.to_world(&tank, local_prop);
        let recovered = EditorAttachmentFrame::Turret.to_local(&tank, world_prop);
        assert_vec3_near(
            recovered.transform_point3(Vec3::ZERO),
            local_prop.transform_point3(Vec3::ZERO),
        );
    }
}
