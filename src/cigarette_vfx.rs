//! Bounded cosmetic cigarette smoke/ash. Burn state belongs to cigarette::State and visible
//! shortening belongs to hand_prop_models. This uses the existing native one-pixel fine-VFX
//! depth/lighting path; no screen overlay, smoke-volume slot, point light or damage is added.

use crate::{cigarette::View, hand_prop_models::Anchors, renderer::Renderer};
use glam::{Mat4, Quat, Vec3};

pub const MAX_SMOKE: usize = 192;
pub const MAX_ASH: usize = 24;
const CAPACITY: usize = MAX_SMOKE + MAX_ASH;
const SMOKE_COLOR: [u8; 3] = [142, 148, 151];
const ASH_COLOR: [u8; 3] = [87, 83, 76];
// Authored smoke/ember presentation, not a calibrated combustion or health simulation.
const TOBACCO_LENGTH_M: f32 = 0.062;
/// Phase of the tap at which the ash column lets go.
const ASH_FALLS_AT: f32 = 0.35;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub ember_intensity: f32,
    pub ember_color: [u8; 3],
    pub char_length_m: f32,
    pub ash_length_m: f32,
    pub ash_coverage: f32,
}

/// Pure surface treatment for Athena's actual clipped end cap/sleeves. Consumption is supplied
/// by gameplay. Fine ash periodically sheds as that end recedes; the filter is never rescaled.
pub fn surface(view: View) -> Surface {
    let consumed = (1.0 - finite_unit(view.burn_remaining)) * TOBACCO_LENGTH_M;
    let puff = finite_unit(view.puff_phase);
    let burning = view.cigarette_present && view.lit;
    // The ash grows through each quarter and is knocked off at the tap that ends it.
    let tap = finite_unit(view.ash_tap);
    let ash_cycle = if tap > 0.0 && tap < ASH_FALLS_AT { 1.0 } else { finite_unit(view.quarter_progress) };
    Surface {
        ember_intensity: if burning { 2.2 + 7.0 * puff } else { 0.0 },
        ember_color: [255, (112.0 + 56.0 * puff) as u8, (18.0 + 14.0 * puff) as u8],
        char_length_m: if view.cigarette_present && (burning || consumed > 0.00001) {
            0.0007 + 0.0006 * puff
        } else {
            0.0
        },
        ash_length_m: if view.cigarette_present {
            (0.00012 + ash_cycle * 0.0060).min(consumed)
        } else {
            0.0
        },
        ash_coverage: if view.cigarette_present && consumed > 0.00001 {
            (0.25 + 0.65 * ash_cycle) * (1.0 - 0.25 * puff)
        } else {
            0.0
        },
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Air {
    /// Current Tiger hull transform, even when the listener is on foot. Previously emitted
    /// cabin wisps remain carried by that air frame when the player changes station/exits.
    pub hull_model: Mat4,
    pub inside: bool,
    /// Local, already sheltered exterior velocity. Do not pass an unsheltered global storm.
    pub world_velocity: Vec3,
    /// Hull-relative cabin flow, usually zero: exterior blizzard wind is not cabin flow.
    pub cabin_velocity: Vec3,
}
impl Default for Air {
    fn default() -> Self {
        Self {
            hull_model: Mat4::IDENTITY,
            inside: false,
            world_velocity: Vec3::ZERO,
            cabin_velocity: Vec3::ZERO,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Slots {
    smoke: usize,
    ash: usize,
}

pub fn allocate(device: &wgpu::Device, queue: &wgpu::Queue, renderer: &mut Renderer) -> Slots {
    let page = |r: &mut Renderer, color, capacity| {
        let mesh = crate::prims::faceted_sphere_mesh(color)
            .transformed(Mat4::from_scale(Vec3::splat(0.5)));
        let slot = r
            .add_dynamic_instanced_pool(device, queue, &mesh, capacity)
            .expect("bounded cigarette particle page");
        // Fine-VFX soot means matte lit/extinguishing grains, never self-emitting white smoke.
        r.set_fine_vfx_batch(slot, color, true, 1);
        assert!(r.set_dynamic_instanced_emissive(slot, 0.0));
        slot
    };
    Slots {
        smoke: page(renderer, SMOKE_COLOR, MAX_SMOKE),
        ash: page(renderer, ASH_COLOR, MAX_ASH),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Tip,
    Exhale,
    Ash,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Particle {
    id: u32,
    kind: Kind,
    pos: Vec3,
    velocity: Vec3,
    age: f64,
    life: f32,
    cabin: bool,
}
#[derive(Clone)]
pub struct Effects {
    seed: u32,
    sequence: u32,
    particles: Vec<Particle>,
    tip_credit: f64,
    breath_credit: f64,
    ash_credit: f64,
    previous_burn: Option<f32>,
    previous_tap: f32,
    previous_anchors: Option<Anchors>,
    previous_hull: Option<Mat4>,
    smoke_mats: Vec<Mat4>,
    smoke_ids: Vec<[u32; 4]>,
    ash_mats: Vec<Mat4>,
    ash_ids: Vec<[u32; 4]>,
}
impl Effects {
    pub fn new(seed: u64) -> Self {
        Self {
            seed: (seed ^ (seed >> 32)) as u32,
            sequence: 0,
            particles: Vec::with_capacity(CAPACITY),
            tip_credit: 0.0,
            breath_credit: 0.0,
            ash_credit: 0.0,
            previous_burn: None,
            previous_tap: 0.0,
            previous_anchors: None,
            previous_hull: None,
            smoke_mats: Vec::with_capacity(MAX_SMOKE),
            smoke_ids: Vec::with_capacity(MAX_SMOKE),
            ash_mats: Vec::with_capacity(MAX_ASH),
            ash_ids: Vec::with_capacity(MAX_ASH),
        }
    }

    /// Realtime age/transport, not an animation-frame loop. Existing particles continue after
    /// stowing. Calling with invalid/zero dt changes nothing, including emission history.
    pub fn update(&mut self, dt: f32, view: View, anchors: &Anchors, air: Air) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let hull = if air.hull_model.is_finite() && air.hull_model.determinant().abs() > 1e-6 {
            air.hull_model
        } else {
            self.previous_hull.unwrap_or(Mat4::IDENTITY)
        };
        let delta = self
            .previous_hull
            .map_or(Mat4::IDENTITY, |old| hull * old.inverse());
        let world_flow = finite_flow(air.world_velocity);
        let cabin_flow = hull.transform_vector3(finite_flow(air.cabin_velocity));
        for particle in &mut self.particles {
            if particle.cabin {
                particle.pos = delta.transform_point3(particle.pos);
                particle.velocity = delta.transform_vector3(particle.velocity);
            }
            advance_particle(
                particle,
                f64::from(dt),
                if particle.cabin {
                    cabin_flow
                } else {
                    world_flow
                },
            );
        }
        self.particles
            .retain(|p| p.age < f64::from(p.life) && p.pos.is_finite());
        let visible = view.cigarette_present && anchors.cigarette_visible;
        let mut previous = self.previous_anchors.unwrap_or(*anchors);
        if air.inside {
            previous.tip = delta.transform_point3(previous.tip);
            previous.mouth = delta.transform_point3(previous.mouth);
        }
        // Only interpolate actual short hand motion. Teleports/station cuts cannot leave a
        // metre-long line of newborn smoke through intervening armor or scenery.
        let tip_from = if previous.tip.distance_squared(anchors.tip) < 0.25 {
            previous.tip
        } else {
            anchors.tip
        };
        let mouth_from = if previous.mouth.distance_squared(anchors.mouth) < 0.25 {
            previous.mouth
        } else {
            anchors.mouth
        };
        let flow = if air.inside { cabin_flow } else { world_flow };
        if visible && view.lit && anchors.tip.is_finite() {
            let rate = 34.0 + finite_unit(view.puff_phase) * 12.0;
            let (count, remainder) = emission_budget(self.tip_credit, rate, dt);
            self.tip_credit = remainder;
            self.emit(
                count,
                remainder,
                rate,
                dt,
                Kind::Tip,
                tip_from,
                anchors.tip,
                anchors.forward,
                air.inside,
                flow,
            );
        } else {
            self.tip_credit = 0.0;
        }
        if finite_unit(view.exhale) > 0.0 && anchors.mouth.is_finite() {
            let rate = 120.0 * finite_unit(view.exhale);
            let (count, remainder) = emission_budget(self.breath_credit, rate, dt);
            self.breath_credit = remainder;
            self.emit(
                count,
                remainder,
                rate,
                dt,
                Kind::Exhale,
                mouth_from,
                anchors.mouth,
                anchors.forward,
                air.inside,
                flow,
            );
        } else {
            self.breath_credit = 0.0;
        }
        let remaining = finite_unit(view.burn_remaining);
        if visible && view.lit && anchors.tip.is_finite() {
            if let Some(old) = self.previous_burn {
                let lost = (old - remaining).max(0.0);
                self.ash_credit += f64::from(lost * TOBACCO_LENGTH_M / 0.00045);
                let count = (self.ash_credit.floor() as usize).min(MAX_ASH);
                self.ash_credit = self.ash_credit.fract();
                for _ in 0..count {
                    self.birth(
                        Kind::Ash,
                        anchors.tip,
                        anchors.forward,
                        air.inside,
                        flow,
                        0.0,
                    );
                }
            }
        } else {
            self.ash_credit = 0.0;
        }
        // The tap: the whole column of ash comes off at once.
        let tap = finite_unit(view.ash_tap);
        if visible && tap >= ASH_FALLS_AT && self.previous_tap < ASH_FALLS_AT {
            for _ in 0..12 {
                self.birth(Kind::Ash, anchors.tip, anchors.forward, air.inside, flow, 0.0);
            }
        }
        self.previous_tap = tap;
        self.previous_burn = view.cigarette_present.then_some(remaining);
        self.previous_anchors = Some(*anchors);
        self.previous_hull = Some(hull);
        self.prepare();
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        count: usize,
        remainder: f64,
        rate: f32,
        dt: f32,
        kind: Kind,
        from: Vec3,
        to: Vec3,
        forward: Vec3,
        cabin: bool,
        flow: Vec3,
    ) {
        // A long stall samples only the bounded recent history that can still be alive; no
        // enormous catch-up loop, time dilation, or burst of old particles at the current tip.
        let count = count.min(MAX_SMOKE);
        for i in (0..count).rev() {
            let age = (i as f64 + remainder) / f64::from(rate.max(0.001));
            if age > 2.5 {
                continue;
            }
            let fraction = (1.0 - age / f64::from(dt)).clamp(0.0, 1.0) as f32;
            self.birth(kind, from.lerp(to, fraction), forward, cabin, flow, age);
        }
    }

    fn birth(
        &mut self,
        kind: Kind,
        source: Vec3,
        forward: Vec3,
        cabin: bool,
        flow: Vec3,
        age: f64,
    ) {
        let same_kind = self
            .particles
            .iter()
            .filter(|p| (p.kind == Kind::Ash) == (kind == Kind::Ash))
            .count();
        let limit = if kind == Kind::Ash {
            MAX_ASH
        } else {
            MAX_SMOKE
        };
        if same_kind >= limit {
            let oldest = self
                .particles
                .iter()
                .enumerate()
                .filter(|(_, p)| (p.kind == Kind::Ash) == (kind == Kind::Ash))
                .max_by(|(_, a), (_, b)| a.age.total_cmp(&b.age))
                .map(|(i, _)| i)
                .unwrap();
            self.particles.swap_remove(oldest);
        }
        self.sequence = self.sequence.wrapping_add(1);
        let id = self.sequence;
        let key = id ^ self.seed;
        let jitter = Vec3::new(
            random(key) - 0.5,
            random(key ^ 0x23) - 0.5,
            random(key ^ 0x51) - 0.5,
        );
        let forward = if forward.is_finite() {
            forward.normalize_or_zero()
        } else {
            Vec3::Z
        };
        let (velocity, life) = match kind {
            Kind::Tip => (
                flow + Vec3::Y * 0.08 + jitter * 0.025,
                1.8 + random(key ^ 0xa3) * 0.7,
            ),
            Kind::Exhale => (
                flow + forward * (0.48 + random(key ^ 0xa3) * 0.12) + jitter * 0.065,
                1.0 + random(key ^ 0x93) * 0.75,
            ),
            Kind::Ash => (
                flow + jitter * 0.09 - Vec3::Y * 0.04,
                0.75 + random(key ^ 0x93) * 0.45,
            ),
        };
        let mut particle = Particle {
            id,
            kind,
            pos: source + jitter * if kind == Kind::Exhale { 0.006 } else { 0.0016 },
            velocity,
            age: 0.0,
            life,
            cabin,
        };
        advance_particle(&mut particle, age, flow);
        if particle.age < f64::from(life) {
            self.particles.push(particle);
        }
    }

    fn prepare(&mut self) {
        self.smoke_mats.clear();
        self.smoke_ids.clear();
        self.ash_mats.clear();
        self.ash_ids.clear();
        for p in &self.particles {
            let age = p.age as f32;
            let phase = (age / p.life).clamp(0.0, 1.0);
            let fade = ((1.0 - phase) / 0.30).clamp(0.0, 1.0).sqrt();
            let (width, height) = match p.kind {
                Kind::Tip => (0.0012 + age * 0.0010, 0.0040 + age * 0.0030),
                Kind::Exhale => (0.0018 + age * 0.0017, 0.0035 + age * 0.0040),
                Kind::Ash => (0.0007, 0.0012),
            };
            let length = Vec3::new(width, height, width) * fade;
            let heading = if p.kind == Kind::Ash {
                Quat::from_rotation_y(age * 2.1)
            } else {
                Quat::from_rotation_arc(
                    Vec3::Y,
                    p.velocity
                        .normalize_or_zero()
                        .try_normalize()
                        .unwrap_or(Vec3::Y),
                )
            };
            let matrix = Mat4::from_scale_rotation_translation(length, heading, p.pos);
            let id = [0xc196_0001, p.id, p.kind as u32, self.seed];
            if p.kind == Kind::Ash {
                self.ash_mats.push(matrix);
                self.ash_ids.push(id);
            } else {
                self.smoke_mats.push(matrix);
                self.smoke_ids.push(id);
            }
        }
    }

    /// Submit every frame, including when the hand prop is hidden, so residual wisps naturally
    /// expire and stale instances are cleared. The shader uses ordinary depth for every view.
    pub fn submit(&self, renderer: &mut Renderer, slots: &Slots) {
        assert!(renderer.set_dynamic_instanced_mats(slots.smoke, &self.smoke_mats));
        assert!(renderer.set_fine_vfx_particle_ids(slots.smoke, &self.smoke_ids));
        assert!(renderer.set_dynamic_instanced_mats(slots.ash, &self.ash_mats));
        assert!(renderer.set_fine_vfx_particle_ids(slots.ash, &self.ash_ids));
    }

    pub fn receipt(&self) -> serde_json::Value {
        serde_json::json!({"smoke_parents":self.smoke_mats.len(),"ash_parents":self.ash_mats.len(),
            "capacity_smoke":MAX_SMOKE,"capacity_ash":MAX_ASH,"sequence":self.sequence,
            "source_geometry":"matte fine particles using native pixel depth/lighting", "self_emitting_smoke":false})
    }
}

fn finite_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn finite_flow(value: Vec3) -> Vec3 {
    if value.is_finite() {
        value.clamp_length_max(30.0)
    } else {
        Vec3::ZERO
    }
}
fn random(mut v: u32) -> f32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846ca68b);
    v ^= v >> 16;
    (v >> 8) as f32 / 16_777_216.0
}
fn emission_budget(previous: f64, rate: f32, dt: f32) -> (usize, f64) {
    let accumulated = previous + f64::from(rate) * f64::from(dt);
    (accumulated.floor() as usize, accumulated.fract())
}
fn curl(kind: Kind, age: f32, id: u32) -> Vec3 {
    if kind == Kind::Ash {
        return Vec3::ZERO;
    }
    let phase = random(id) * std::f32::consts::TAU;
    let width = (age * 0.004).min(0.008);
    Vec3::new(
        (phase + age * 4.1).sin(),
        (phase + age * 3.4).cos() * 0.25,
        (phase + age * 3.7).cos(),
    ) * width
}
fn advance_particle(p: &mut Particle, dt: f64, flow: Vec3) {
    if dt <= 0.0 {
        return;
    }
    let before = p.age as f32;
    p.age += dt;
    if p.age >= f64::from(p.life) {
        return;
    }
    let (drag, rise) = match p.kind {
        Kind::Tip => (2.2, 0.115),
        Kind::Exhale => (2.8, 0.065),
        Kind::Ash => (10.0, -0.38),
    };
    let target = flow + Vec3::Y * rise;
    let decay = (-drag * dt as f32).exp();
    let initial = p.velocity - target;
    p.pos +=
        target * dt as f32 + initial * ((1.0 - decay) / drag) + curl(p.kind, p.age as f32, p.id)
            - curl(p.kind, before, p.id);
    p.velocity = target + initial * decay;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view() -> View {
        View {
            equipped: true,
            cigarette_present: true,
            lit: true,
            burn_remaining: 0.9,
            ..View::default()
        }
    }
    fn anchors() -> Anchors {
        Anchors {
            tip: Vec3::new(0.1, 1.2, 0.3),
            mouth: Vec3::new(0.0, 1.3, 0.1),
            lighter: Vec3::ZERO,
            forward: Vec3::Z,
            radius: 0.003706,
            cigarette_visible: true,
            lighter_visible: false,
        }
    }

    #[test]
    fn ember_brightens_on_toke_and_ash_comes_from_actual_consumption() {
        let cold = surface(View {
            lit: false,
            burn_remaining: 1.0,
            ..view()
        });
        let idle = surface(view());
        let draw = surface(View {
            puff_phase: 1.0,
            ..view()
        });
        assert_eq!(cold.ember_intensity, 0.0);
        assert_eq!(cold.ash_length_m, 0.0);
        assert!(draw.ember_intensity > idle.ember_intensity * 4.0);
        assert!(draw.ember_color[1] > idle.ember_color[1]);
        assert!((0.0..=0.0015).contains(&draw.ash_length_m));
        assert_eq!(surface(View::default()).char_length_m, 0.0);
    }

    #[test]
    fn live_tip_and_exhale_sources_are_distinct_and_stowing_stops_new_smoke() {
        let mut fx = Effects::new(7);
        let a = anchors();
        fx.update(0.1, view(), &a, Air::default());
        assert!(fx
            .particles
            .iter()
            .all(|p| p.kind == Kind::Tip && p.pos.distance(a.tip) < 0.03));
        fx.update(
            0.1,
            View {
                exhale: 1.0,
                ..view()
            },
            &a,
            Air::default(),
        );
        assert!(fx
            .particles
            .iter()
            .any(|p| p.kind == Kind::Exhale && p.pos.distance(a.mouth) < 0.08));
        let sequence = fx.sequence;
        fx.update(3.0, View::default(), &a, Air::default());
        assert_eq!(fx.sequence, sequence);
        assert!(fx.particles.is_empty());
        assert!(fx.smoke_mats.is_empty());
    }

    #[test]
    fn real_elapsed_time_preserves_emission_rate_and_analytic_particle_motion() {
        let a = anchors();
        let mut counts = Vec::new();
        for hz in [30, 60, 144] {
            let mut fx = Effects::new(7);
            for _ in 0..hz {
                fx.update(1.0 / hz as f32, view(), &a, Air::default());
            }
            counts.push(fx.sequence);
        }
        assert!(counts.iter().all(|n| (*n as i32 - 34).abs() <= 1));
        let original = Particle {
            id: 7,
            kind: Kind::Exhale,
            pos: Vec3::ZERO,
            velocity: Vec3::Z * 0.6,
            age: 0.0,
            life: 2.0,
            cabin: false,
        };
        let mut full = original;
        let mut split = original;
        advance_particle(&mut full, 0.75, Vec3::X * 0.2);
        for _ in 0..75 {
            advance_particle(&mut split, 0.01, Vec3::X * 0.2);
        }
        assert!(full.pos.distance(split.pos) < 2e-5);
        assert!(full.velocity.distance(split.velocity) < 2e-5);
    }

    #[test]
    fn cabin_particles_follow_hull_motion_while_exterior_particles_do_not() {
        let a = anchors();
        let mut inside = Effects::new(7);
        let mut outside = Effects::new(7);
        inside.update(
            0.2,
            view(),
            &a,
            Air {
                inside: true,
                ..Air::default()
            },
        );
        outside.update(0.2, view(), &a, Air::default());
        let hull = Mat4::from_translation(Vec3::X * 5.0);
        inside.update(
            0.01,
            View::default(),
            &a,
            Air {
                hull_model: hull,
                ..Air::default()
            },
        );
        outside.update(
            0.01,
            View::default(),
            &a,
            Air {
                hull_model: hull,
                ..Air::default()
            },
        );
        assert!(
            inside.particles[0]
                .pos
                .distance(outside.particles[0].pos + Vec3::X * 5.0)
                < 1e-5
        );
    }

    #[test]
    fn pause_is_strict_and_hitches_stay_bounded_with_stable_ids() {
        let mut fx = Effects::new(7);
        let a = anchors();
        fx.update(0.1, view(), &a, Air::default());
        let particles = fx.particles.clone();
        let receipt = fx.receipt();
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            fx.update(dt, View::default(), &a, Air::default());
            assert_eq!(fx.particles, particles);
            assert_eq!(fx.receipt(), receipt);
        }
        let initial_ids: Vec<_> = fx.particles.iter().map(|p| p.id).collect();
        fx.update(0.01, view(), &a, Air::default());
        for id in initial_ids {
            assert!(fx.particles.iter().any(|p| p.id == id));
        }
        for step in 0..100 {
            fx.update(
                10.0,
                View {
                    exhale: 1.0,
                    burn_remaining: 1.0 - step as f32 / 100.0,
                    ..view()
                },
                &a,
                Air::default(),
            );
            assert!(fx.smoke_mats.len() <= MAX_SMOKE && fx.ash_mats.len() <= MAX_ASH);
            assert!(fx.particles.len() <= CAPACITY);
            assert!(fx
                .smoke_mats
                .iter()
                .chain(&fx.ash_mats)
                .all(|m| m.is_finite()));
        }
    }
}
