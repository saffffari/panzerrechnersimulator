//! Cigarette butts. One is thrown down when a cigarette reaches its filter, flies against the
//! same retained source triangles as spent brass, and then stays where it came to rest: on the
//! tank's floor it rides with the tank, on the ground it lies where it fell. Butts make no sound.
use super::*;
use crate::hand_prop_models::{BUTT_LENGTH_M, CIGARETTE_RADIUS_M};

/// Which frame a butt at rest belongs to.
const TURRET: u8 = 0;
const HULL: u8 = 1;
const WORLD: u8 = 2;
/// A butt still moving after this long is set down where it is.
const MAX_FLIGHT_S: f32 = 4.0;

#[derive(Clone)]
pub(super) struct Butt {
    pub active: bool,
    flying: bool,
    frame: u8,
    age: f32,
    body: shell_collision::Body,
    rest: shell_collision::DropRest,
    /// Pose in its frame once at rest.
    local: Mat4,
}
impl Default for Butt {
    fn default() -> Self {
        Self { active: false, flying: false, frame: WORLD, age: 0.0,
            body: shell_collision::Body { position: Vec3::ZERO, rotation: Quat::IDENTITY,
                velocity: Vec3::ZERO, angular_velocity: Vec3::ZERO,
                half: Vec3::new(CIGARETTE_RADIUS_M, CIGARETTE_RADIUS_M, BUTT_LENGTH_M * 0.5), mass: 0.001 },
            rest: Default::default(), local: Mat4::IDENTITY }
    }
}
/// The mesh runs from the filter at its origin along +Z; the body is centred.
fn butt_model(body: &shell_collision::Body) -> Mat4 {
    body.model() * Mat4::from_translation(Vec3::Z * (-BUTT_LENGTH_M * 0.5))
}

impl Game {
    /// Flick the finished cigarette away from where it was held, ahead and a little aside.
    pub(super) fn throw_cigarette_butt(&mut self, from: Vec3, forward: Vec3) {
        if !from.is_finite() || !forward.is_finite() || self.cigarette_butts.is_empty() { return; }
        let n = self.next_cigarette_butt;
        self.next_cigarette_butt = (n + 1) % self.cigarette_butts.len();
        let seed = n as u32 ^ self.kill_tick;
        let forward = forward.normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let seated_in_turret = self.mode == Mode::Tank
            && matches!(self.station, Station::Gunner | Station::Loader | Station::Commander);
        let butt = &mut self.cigarette_butts[n];
        *butt = Butt::default();
        butt.active = true;
        butt.flying = true;
        butt.frame = if seated_in_turret { TURRET } else { HULL };
        butt.body.position = from;
        butt.body.rotation = Quat::from_rotation_arc(Vec3::Z, forward);
        // Relative to the tank when aboard; the tank's own motion is added if it leaves.
        butt.body.velocity = forward * (1.3 + 0.5 * hash01(seed)) + right * (0.5 - hash01(seed ^ 0xB077)) * 0.9
            + Vec3::Y * 0.7;
        butt.body.angular_velocity = Vec3::new(9.0, 4.0 * hash01(seed ^ 0x51), 6.0);
    }

    pub(super) fn drive_cigarette_butts(&mut self, dt: f32, r: &mut Renderer) {
        let Some(props) = &self.hand_props else { return; };
        let frames = [self.player.turret_model(), self.player.hull_model()];
        let any_flying = self.cigarette_butts.iter().any(|b| b.active && b.flying);
        let hull_inverse = frames[1].inverse();
        let models = self.fire_surface_models();
        let source = self.cabin_fire.source_contacts(&models);
        let signature = shell_collision::scene_signature(&models);
        let mut budget = shell_collision::DropBudget::default();
        let steps = self.brass_steps;
        let on_foot_floor = (self.mode == Mode::Foot).then_some(self.foot.pos);
        for (butt, &slot) in self.cigarette_butts.iter_mut().zip(props.butt_slots()) {
            r.set_visible(slot, butt.active);
            if !butt.active { continue; }
            if butt.flying && any_flying && dt > 0.0 {
                butt.age += dt;
                let body = &mut butt.body;
                if brass_aboard(hull_inverse, body.position) {
                    let frame = usize::from(butt.frame.min(HULL));
                    carry_with_frame(steps[frame], &mut body.position, &mut body.rotation,
                        &mut body.velocity, &mut body.angular_velocity);
                    body.advance_dropped(dt, &source, signature, &mut butt.rest, &mut budget, 0.1);
                    if butt.rest.sleeping || butt.age > MAX_FLIGHT_S {
                        butt.flying = false;
                        butt.local = frames[frame].inverse() * butt_model(body);
                    }
                } else {
                    // Clear of the tank: it falls to the ground, or to the floor underfoot.
                    body.velocity.y -= 9.8 * dt;
                    body.position += body.velocity * dt;
                    let mut ground = self.hf.height_at(body.position.x, body.position.z);
                    if let Some(feet) = on_foot_floor {
                        if Vec2::new(feet.x - body.position.x, feet.z - body.position.z).length() < 4.0 {
                            ground = ground.max(feet.y);
                        }
                    }
                    if body.position.y <= ground + CIGARETTE_RADIUS_M || butt.age > MAX_FLIGHT_S * 2.0 {
                        body.position.y = ground + CIGARETTE_RADIUS_M;
                        // Lying flat, pointing wherever it was heading.
                        let heading = Vec3::new(body.velocity.x, 0.0, body.velocity.z).normalize_or(Vec3::Z);
                        body.rotation = Quat::from_rotation_arc(Vec3::Z, heading);
                        butt.frame = WORLD;
                        butt.flying = false;
                        butt.local = butt_model(body);
                    }
                }
            }
            let model = if butt.flying { butt_model(&butt.body) }
                else if butt.frame == WORLD { butt.local }
                else { frames[usize::from(butt.frame)] * butt.local };
            r.set_emissive(slot, 0.0);
            r.set_model(slot, model);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_butt_is_a_short_filter_length_body_centred_on_its_mesh() {
        let butt = Butt::default();
        assert!(!butt.active && !butt.flying);
        assert!((butt.body.half.z * 2.0 - BUTT_LENGTH_M).abs() < 1e-6);
        assert!(BUTT_LENGTH_M < 0.03 && BUTT_LENGTH_M > crate::hand_prop_models::FILTER_LENGTH_M);
        let model = butt_model(&butt.body);
        // The mesh's two ends sit either side of the body's centre.
        assert!((model.transform_point3(Vec3::ZERO).z + BUTT_LENGTH_M * 0.5).abs() < 1e-6);
        assert!((model.transform_point3(Vec3::Z * BUTT_LENGTH_M).z - BUTT_LENGTH_M * 0.5).abs() < 1e-6);
    }
}
