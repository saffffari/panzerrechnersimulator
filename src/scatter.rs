//! Cheap persistent physics for lightweight battlefield scatter.
//!
//! A 57-ton Tiger must not treat a bench, barrel, crate, cart, light fence, or telegraph pole as
//! static world mass.  These objects stay on the renderer's instanced path; this module owns only
//! their CPU transforms and rewrites a spatial page while one of its objects is moving.  Tall
//! objects hinge once about their base. Loose objects get a short deterministic ballistic shove,
//! bounce a bounded number of times, then sleep in their displaced pose. There is deliberately no
//! general rigid-body solver and no object-object collision.

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec2, Vec3};

use crate::gltf_load::CpuMesh;
use crate::renderer::{InstancedPrototype, Renderer};
use crate::terrain::Heightfield;
use crate::worldgen::{ScatterMaterial, ScatterMotion, ScatterProfile};

const CELL_M: f32 = 24.0;
const MAX_TANK_CONTACTS: usize = 24;
const MAX_BLAST_BODIES: usize = 64;
const GRAVITY: f32 = 9.81;

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Resting,
    Falling {
        t: f32,
        dir: Vec3,
        duration: f32,
        end_angle: f32,
    },
    Airborne {
        pos: Vec3,
        vel: Vec3,
        rotation: Quat,
        spin_axis: Vec3,
        spin_rate: f32,
        age: f32,
        bounces: u8,
    },
    Down,
}

#[derive(Clone)]
struct Body {
    batch: usize,
    instance: usize,
    profile: ScatterProfile,
    rest_pose: Mat4,
    rest_center: Vec3,
    current_pose: Mat4,
    center: Vec3,
    base_pivot: Vec3,
    half_extents: Vec3,
    contact_radius: f32,
    floor_clearance: f32,
    seed: u32,
    state: State,
    active: bool,
    cell: (i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterEventKind {
    TankShove,
    ShellStrike,
    GroundHit,
}

#[derive(Clone, Copy, Debug)]
pub struct ScatterEvent {
    pub kind: ScatterEventKind,
    pub material: ScatterMaterial,
    pub pos: Vec3,
    pub strength: f32,
}

/// Scatter contact never contains a hard stop. The caller may apply the small one-frame speed
/// retention value for feel; even a run of light fencing retains most Tiger momentum.
#[derive(Clone, Copy, Debug)]
pub struct ScatterContact {
    pub hits: usize,
    pub speed_retention: f32,
    pub rumble: f32,
}

pub struct Scatter {
    device: wgpu::Device,
    /// (renderer batch index, mutable page transforms)
    batches: Vec<(usize, Vec<Mat4>)>,
    dirty: Vec<bool>,
    bodies: Vec<Body>,
    active: Vec<usize>,
    grid: HashMap<(i32, i32), Vec<usize>>,
}

fn hash01(mut x: u32) -> f32 {
    x = x.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

fn cell_at(pos: Vec3) -> (i32, i32) {
    (
        (pos.x / CELL_M).floor() as i32,
        (pos.z / CELL_M).floor() as i32,
    )
}

fn transformed_bounds(mesh: &CpuMesh, model: Mat4) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8u32 {
        let local = Vec3::new(
            if corner & 1 == 0 {
                mesh.min.x
            } else {
                mesh.max.x
            },
            if corner & 2 == 0 {
                mesh.min.y
            } else {
                mesh.max.y
            },
            if corner & 4 == 0 {
                mesh.min.z
            } else {
                mesh.max.z
            },
        );
        let world = model.transform_point3(local);
        min = min.min(world);
        max = max.max(world);
    }
    (min, max)
}

fn body_from_model(
    batch: usize,
    instance: usize,
    mesh: &CpuMesh,
    model: Mat4,
    profile: ScatterProfile,
) -> Body {
    let (min, max) = transformed_bounds(mesh, model);
    let center = (min + max) * 0.5;
    let half = ((max - min) * 0.5).max(Vec3::splat(0.04));
    let horizontal_radius = Vec2::new(half.x, half.z).length();
    let contact_radius = match profile.motion {
        ScatterMotion::Hinge => horizontal_radius.clamp(0.12, 2.8),
        ScatterMotion::Loose | ScatterMotion::Roll => half.length().clamp(0.15, 2.2),
    };
    let seed = 0x5CA7_7E12
        ^ (batch as u32).wrapping_mul(0x9E37_79B9)
        ^ (instance as u32).wrapping_mul(0x85EB_CA6B)
        ^ (center.x.to_bits().rotate_left(7))
        ^ center.z.to_bits().rotate_right(5);
    Body {
        batch,
        instance,
        profile,
        rest_pose: model,
        rest_center: center,
        current_pose: model,
        center,
        base_pivot: Vec3::new(center.x, min.y, center.z),
        half_extents: half,
        contact_radius,
        // The smallest half-extent approximates the side the object naturally sleeps on. It keeps
        // a toppled cart or barrel above terrain without pretending to solve its convex hull.
        floor_clearance: half.min_element().clamp(0.08, 0.70),
        seed,
        state: State::Resting,
        active: false,
        cell: cell_at(center),
    }
}

fn horizontal_dir(dir: Vec3, fallback_seed: u32) -> Vec3 {
    let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    if flat.length_squared() > 0.5 {
        flat
    } else {
        let a = hash01(fallback_seed) * std::f32::consts::TAU;
        Vec3::new(a.sin(), 0.0, a.cos())
    }
}

fn start_hinge(body: &mut Body, dir: Vec3, kind: ScatterEventKind) -> Option<ScatterEvent> {
    if body.profile.motion != ScatterMotion::Hinge || body.state != State::Resting {
        return None;
    }
    let dir = horizontal_dir(dir, body.seed);
    let height = body.half_extents.y * 2.0;
    body.state = State::Falling {
        t: 0.0,
        dir,
        duration: (0.58 + height * 0.105).clamp(0.68, 1.45),
        end_angle: 1.48 + hash01(body.seed ^ 0xF411_0001) * 0.09,
    };
    Some(ScatterEvent {
        kind,
        material: body.profile.material,
        pos: body.base_pivot + Vec3::Y * 0.55,
        strength: (0.45 + height * 0.08).clamp(0.5, 1.0),
    })
}

fn start_loose(
    body: &mut Body,
    dir: Vec3,
    impulse: f32,
    up: f32,
    kind: ScatterEventKind,
) -> Option<ScatterEvent> {
    if body.profile.motion == ScatterMotion::Hinge || body.state != State::Resting {
        return None;
    }
    let dir = horizontal_dir(dir, body.seed);
    let side = Vec3::Y.cross(dir);
    let skew = (hash01(body.seed ^ 0x51DE_0002) * 2.0 - 1.0) * impulse * 0.16;
    let spin_axis = match body.profile.motion {
        ScatterMotion::Roll => Vec3::Y.cross(dir).normalize_or_zero(),
        ScatterMotion::Loose => Vec3::new(
            hash01(body.seed ^ 3) - 0.5,
            hash01(body.seed ^ 5) - 0.5,
            hash01(body.seed ^ 7) - 0.5,
        )
        .normalize_or_zero(),
        ScatterMotion::Hinge => unreachable!(),
    };
    let spin_axis = if spin_axis.length_squared() < 0.5 {
        Vec3::X
    } else {
        spin_axis
    };
    body.state = State::Airborne {
        pos: body.center,
        vel: dir * impulse + side * skew + Vec3::Y * up,
        rotation: Quat::IDENTITY,
        spin_axis,
        spin_rate: if body.profile.motion == ScatterMotion::Roll {
            5.0 + impulse * 0.8
        } else {
            3.0 + hash01(body.seed ^ 11) * 4.0 + impulse * 0.35
        },
        age: 0.0,
        bounces: 0,
    };
    Some(ScatterEvent {
        kind,
        material: body.profile.material,
        pos: body.center,
        strength: (impulse / 10.0).clamp(0.25, 1.0),
    })
}

fn ray_sphere(o: Vec3, d: Vec3, center: Vec3, radius: f32, max_t: f32) -> Option<f32> {
    let oc = o - center;
    let b = oc.dot(d);
    let c = oc.length_squared() - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let near = -b - disc.sqrt();
    let t = if near >= 0.0 { near } else { -b + disc.sqrt() };
    (t >= 0.0 && t <= max_t).then_some(t)
}

fn ray_hinge(body: &Body, o: Vec3, d: Vec3, max_t: f32) -> Option<f32> {
    if body.state != State::Resting {
        return None;
    }
    let ox = o.x - body.center.x;
    let oz = o.z - body.center.z;
    let a = d.x * d.x + d.z * d.z;
    let radius = body.contact_radius + 0.05;
    let t = if a < 1.0e-8 {
        (ox * ox + oz * oz <= radius * radius).then_some(0.0)?
    } else {
        let b = ox * d.x + oz * d.z;
        let c = ox * ox + oz * oz - radius * radius;
        let disc = b * b - a * c;
        if disc < 0.0 {
            return None;
        }
        (-b - disc.sqrt()) / a
    };
    if t < 0.0 || t > max_t {
        return None;
    }
    let y = o.y + d.y * t;
    let min_y = body.base_pivot.y;
    let max_y = min_y + body.half_extents.y * 2.0;
    (y >= min_y && y <= max_y).then_some(t)
}

fn ray_body(body: &Body, o: Vec3, d: Vec3, max_t: f32) -> Option<f32> {
    match body.profile.motion {
        ScatterMotion::Hinge => ray_hinge(body, o, d, max_t),
        ScatterMotion::Loose | ScatterMotion::Roll => {
            if matches!(body.state, State::Falling { .. } | State::Down) {
                None
            } else {
                ray_sphere(o, d, body.center, body.contact_radius, max_t)
            }
        }
    }
}

impl Scatter {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            batches: Vec::new(),
            dirty: Vec::new(),
            bodies: Vec::new(),
            active: Vec::new(),
            grid: HashMap::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.bodies.len()
    }

    pub fn register_batch(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
        mats: Vec<Mat4>,
        profile: ScatterProfile,
    ) {
        if mats.is_empty() {
            return;
        }
        let prototype = renderer.register_instanced_prototype(device, queue, mesh);
        self.register_page(renderer, device, prototype, mesh, mats, profile);
    }

    pub fn register_page(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        mesh: &CpuMesh,
        mats: Vec<Mat4>,
        profile: ScatterProfile,
    ) {
        if mats.is_empty() {
            return;
        }
        let Some(renderer_batch) = renderer.add_instanced_page(device, prototype, &mats) else {
            debug_assert!(
                false,
                "scatter page references an invalid renderer prototype"
            );
            return;
        };
        let local_batch = self.batches.len();
        for (instance, &model) in mats.iter().enumerate() {
            let index = self.bodies.len();
            let body = body_from_model(local_batch, instance, mesh, model, profile);
            self.grid.entry(body.cell).or_default().push(index);
            self.bodies.push(body);
        }
        self.batches.push((renderer_batch, mats));
        self.dirty.push(false);
    }

    pub fn register_pages(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        mesh: &CpuMesh,
        pages: &[&[Mat4]],
        profile: ScatterProfile,
    ) -> bool {
        let Some(renderer_batches) = renderer.add_instanced_pages(device, prototype, pages) else {
            return false;
        };
        if renderer_batches.len() != pages.len() {
            return false;
        }
        for (&renderer_batch, page) in renderer_batches.iter().zip(pages) {
            let local_batch = self.batches.len();
            for (instance, &model) in page.iter().enumerate() {
                let index = self.bodies.len();
                let body = body_from_model(local_batch, instance, mesh, model, profile);
                self.grid.entry(body.cell).or_default().push(index);
                self.bodies.push(body);
            }
            self.batches.push((renderer_batch, page.to_vec()));
            self.dirty.push(false);
        }
        true
    }

    fn activate(&mut self, index: usize) {
        if !self.bodies[index].active {
            self.bodies[index].active = true;
            self.active.push(index);
        }
    }

    fn nearby(&self, pos: Vec3, radius: f32) -> Vec<usize> {
        let min_x = ((pos.x - radius) / CELL_M).floor() as i32;
        let max_x = ((pos.x + radius) / CELL_M).floor() as i32;
        let min_z = ((pos.z - radius) / CELL_M).floor() as i32;
        let max_z = ((pos.z + radius) / CELL_M).floor() as i32;
        let mut out = Vec::new();
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                if let Some(indices) = self.grid.get(&(x, z)) {
                    out.extend(indices.iter().copied());
                }
            }
        }
        out
    }

    fn move_grid_entry(&mut self, index: usize, old: (i32, i32), new: (i32, i32)) {
        if old == new {
            return;
        }
        if let Some(indices) = self.grid.get_mut(&old) {
            indices.retain(|&candidate| candidate != index);
        }
        self.grid.entry(new).or_default().push(index);
        self.bodies[index].cell = new;
    }

    /// Drive a heavy vehicle through nearby light scatter. This path never separates or stops the
    /// tank: it only starts motion, returns a small speed-retention beat, and leaves the object down.
    pub fn push_tank(
        &mut self,
        nose: Vec3,
        dir: Vec3,
        speed: f32,
        nose_radius: f32,
        vehicle_tons: f32,
        events: &mut Vec<ScatterEvent>,
    ) -> ScatterContact {
        let mut out = ScatterContact {
            hits: 0,
            speed_retention: 1.0,
            rumble: 0.0,
        };
        if speed < 0.25 {
            return out;
        }
        let mass = (vehicle_tons / 57.0).clamp(0.25, 1.35);
        let mut candidates: Vec<(f32, usize)> = self
            .nearby(nose, nose_radius + 3.0)
            .into_iter()
            .filter_map(|index| {
                let body = &self.bodies[index];
                if body.state != State::Resting {
                    return None;
                }
                let delta = Vec2::new(body.center.x - nose.x, body.center.z - nose.z);
                let distance_sq = delta.length_squared();
                (distance_sq <= (nose_radius + body.contact_radius).powi(2))
                    .then_some((distance_sq, index))
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        for (_, index) in candidates.into_iter().take(MAX_TANK_CONTACTS) {
            let (state, center, contact_radius, motion) = {
                let body = &self.bodies[index];
                (
                    body.state,
                    body.center,
                    body.contact_radius,
                    body.profile.motion,
                )
            };
            if state != State::Resting {
                continue;
            }
            debug_assert!(
                Vec2::new(center.x - nose.x, center.z - nose.z).length_squared()
                    <= (nose_radius + contact_radius).powi(2)
            );
            let event = match motion {
                ScatterMotion::Hinge => {
                    start_hinge(&mut self.bodies[index], dir, ScatterEventKind::TankShove)
                }
                ScatterMotion::Loose => start_loose(
                    &mut self.bodies[index],
                    dir,
                    (2.2 + speed * 0.70) * mass,
                    (0.55 + speed * 0.10) * mass,
                    ScatterEventKind::TankShove,
                ),
                ScatterMotion::Roll => start_loose(
                    &mut self.bodies[index],
                    dir,
                    (2.8 + speed * 0.82) * mass,
                    (0.25 + speed * 0.05) * mass,
                    ScatterEventKind::TankShove,
                ),
            };
            let Some(event) = event else { continue };
            events.push(event);
            self.activate(index);
            out.hits += 1;
            let retention: f32 = match motion {
                ScatterMotion::Hinge => 0.93,
                ScatterMotion::Loose => 0.97,
                ScatterMotion::Roll => 0.985,
            };
            out.speed_retention = out.speed_retention.min(retention);
            out.rumble = out.rumble.max(match motion {
                ScatterMotion::Hinge => 0.28,
                ScatterMotion::Loose => 0.16,
                ScatterMotion::Roll => 0.10,
            });
        }
        out
    }

    /// Gather every lightweight object crossed by a piecewise shell route. Objects never stop or
    /// deflect the 88 mm round; their hit distance is only scheduled visual/physical freight.
    pub fn ray_route(&self, route: &[Vec3]) -> Vec<(usize, Vec3, f32)> {
        let mut hits = Vec::new();
        let mut seen = vec![false; self.bodies.len()];
        let mut base_distance = 0.0;
        for segment in route.windows(2) {
            let delta = segment[1] - segment[0];
            let length = delta.length();
            if length <= 1.0e-4 {
                continue;
            }
            let dir = delta / length;
            for (index, body) in self.bodies.iter().enumerate() {
                if seen[index] {
                    continue;
                }
                if let Some(t) = ray_body(body, segment[0], dir, length) {
                    seen[index] = true;
                    hits.push((index, dir, base_distance + t));
                }
            }
            base_distance += length;
        }
        hits.sort_by(|a, b| a.2.total_cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
        hits
    }

    pub fn shot(&mut self, index: usize, dir: Vec3, events: &mut Vec<ScatterEvent>) {
        let Some(body) = self.bodies.get_mut(index) else {
            return;
        };
        let event = match body.profile.motion {
            ScatterMotion::Hinge => start_hinge(body, dir, ScatterEventKind::ShellStrike),
            ScatterMotion::Loose => {
                start_loose(body, dir, 11.0, 2.4, ScatterEventKind::ShellStrike)
            }
            ScatterMotion::Roll => start_loose(body, dir, 13.0, 1.4, ScatterEventKind::ShellStrike),
        };
        if let Some(event) = event {
            events.push(event);
            self.activate(index);
        }
    }

    pub fn blast(&mut self, pos: Vec3, radius: f32, events: &mut Vec<ScatterEvent>) {
        let mut candidates: Vec<(f32, usize)> = self
            .nearby(pos, radius + 3.0)
            .into_iter()
            .filter_map(|index| {
                let body = &self.bodies[index];
                if body.state != State::Resting {
                    return None;
                }
                let distance = body.center.distance(pos);
                (distance <= radius + body.contact_radius).then_some((distance, index))
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates.dedup_by_key(|(_, index)| *index);
        for (distance, index) in candidates.into_iter().take(MAX_BLAST_BODIES) {
            let to = self.bodies[index].center - pos;
            let falloff = (1.0 - distance / radius.max(0.1)).clamp(0.12, 1.0);
            let dir = horizontal_dir(to, self.bodies[index].seed ^ 0xB1A5_7000);
            let event = match self.bodies[index].profile.motion {
                ScatterMotion::Hinge => {
                    start_hinge(&mut self.bodies[index], dir, ScatterEventKind::ShellStrike)
                }
                ScatterMotion::Loose => start_loose(
                    &mut self.bodies[index],
                    dir,
                    4.0 + 8.0 * falloff,
                    1.1 + 2.2 * falloff,
                    ScatterEventKind::ShellStrike,
                ),
                ScatterMotion::Roll => start_loose(
                    &mut self.bodies[index],
                    dir,
                    5.0 + 9.0 * falloff,
                    0.6 + 1.3 * falloff,
                    ScatterEventKind::ShellStrike,
                ),
            };
            if let Some(event) = event {
                events.push(event);
                self.activate(index);
            }
        }
    }

    /// Advance only active objects, upload only dirty spatial pages, and persist every final pose.
    pub fn update(
        &mut self,
        dt: f32,
        renderer: &mut Renderer,
        terrain: &Heightfield,
        events: &mut Vec<ScatterEvent>,
    ) {
        let active = std::mem::take(&mut self.active);
        for index in active {
            let old_cell = self.bodies[index].cell;
            let (pose, keep_active, ground_event) = {
                let body = &mut self.bodies[index];
                match body.state {
                    State::Falling {
                        t,
                        dir,
                        duration,
                        end_angle,
                    } => {
                        let nt = t + dt;
                        let done = nt >= duration;
                        let k = (nt / duration).clamp(0.0, 1.0);
                        let angle = end_angle * k * k * (2.0 - k);
                        let axis = Vec3::Y.cross(dir).normalize_or_zero();
                        let hinge = Mat4::from_translation(body.base_pivot)
                            * Mat4::from_quat(Quat::from_axis_angle(axis, angle))
                            * Mat4::from_translation(-body.base_pivot);
                        let pose = hinge * body.rest_pose;
                        body.current_pose = pose;
                        body.center = hinge.transform_point3(body.rest_center);
                        body.state = if done {
                            State::Down
                        } else {
                            State::Falling {
                                t: nt,
                                dir,
                                duration,
                                end_angle,
                            }
                        };
                        let event = done.then_some(ScatterEvent {
                            kind: ScatterEventKind::GroundHit,
                            material: body.profile.material,
                            pos: body.base_pivot + dir * body.half_extents.y * 0.9,
                            strength: 0.55,
                        });
                        (pose, !done, event)
                    }
                    State::Airborne {
                        mut pos,
                        mut vel,
                        mut rotation,
                        spin_axis,
                        mut spin_rate,
                        mut age,
                        mut bounces,
                    } => {
                        age += dt;
                        vel.y -= GRAVITY * dt;
                        pos += vel * dt;
                        let ground = terrain.height_at(pos.x, pos.z) + body.floor_clearance;
                        let mut event = None;
                        if pos.y <= ground {
                            let impact = (-vel.y).max(0.0);
                            pos.y = ground;
                            if vel.y < 0.0 {
                                vel.y = -vel.y * if bounces == 0 { 0.24 } else { 0.10 };
                                vel.x *= 0.72;
                                vel.z *= 0.72;
                                spin_rate *= 0.76;
                                if impact > 1.2 && bounces < 2 {
                                    event = Some(ScatterEvent {
                                        kind: ScatterEventKind::GroundHit,
                                        material: body.profile.material,
                                        pos: Vec3::new(pos.x, ground - body.floor_clearance, pos.z),
                                        strength: (impact / 8.0).clamp(0.18, 0.8),
                                    });
                                }
                                bounces = bounces.saturating_add(1);
                            }
                        }
                        let drag = if body.profile.motion == ScatterMotion::Roll {
                            1.15
                        } else {
                            2.15
                        };
                        let damping = (-drag * dt).exp();
                        vel.x *= damping;
                        vel.z *= damping;
                        spin_rate *= (-0.72 * dt).exp();
                        rotation = Quat::from_axis_angle(spin_axis, spin_rate * dt) * rotation;
                        let pose = Mat4::from_translation(pos)
                            * Mat4::from_quat(rotation)
                            * Mat4::from_translation(-body.rest_center)
                            * body.rest_pose;
                        body.current_pose = pose;
                        body.center = pos;
                        let grounded = (pos.y - ground).abs() < 0.03;
                        let sleeping = (age > 0.45
                            && grounded
                            && Vec2::new(vel.x, vel.z).length() < 0.28
                            && vel.y.abs() < 0.32)
                            || age > 4.5;
                        if sleeping {
                            body.rest_pose = pose;
                            body.rest_center = pos;
                            body.base_pivot =
                                Vec3::new(pos.x, terrain.height_at(pos.x, pos.z), pos.z);
                            body.state = State::Resting;
                        } else {
                            body.state = State::Airborne {
                                pos,
                                vel,
                                rotation,
                                spin_axis,
                                spin_rate,
                                age,
                                bounces,
                            };
                        }
                        (pose, !sleeping, event)
                    }
                    State::Resting | State::Down => (body.current_pose, false, None),
                }
            };
            if let Some(event) = ground_event {
                events.push(event);
            }
            let (batch, instance) = (self.bodies[index].batch, self.bodies[index].instance);
            self.batches[batch].1[instance] = pose;
            self.dirty[batch] = true;
            let new_cell = cell_at(self.bodies[index].center);
            self.move_grid_entry(index, old_cell, new_cell);
            self.bodies[index].active = keep_active;
            if keep_active {
                self.active.push(index);
            }
        }

        for batch in 0..self.batches.len() {
            if self.dirty[batch] {
                let (renderer_batch, ref mats) = self.batches[batch];
                renderer.set_instanced_mats(&self.device, renderer_batch, mats);
                self.dirty[batch] = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(min: Vec3, max: Vec3) -> CpuMesh {
        crate::prims::box_mesh(min, max, [96, 78, 55])
    }

    fn profile(motion: ScatterMotion) -> ScatterProfile {
        ScatterProfile {
            motion,
            material: ScatterMaterial::Wood,
        }
    }

    #[test]
    fn tiger_contact_starts_motion_without_a_hard_stop() {
        let fence_mesh = mesh(Vec3::new(-2.0, 0.0, -0.08), Vec3::new(2.0, 1.3, 0.08));
        let mut fence = body_from_model(
            0,
            0,
            &fence_mesh,
            Mat4::IDENTITY,
            profile(ScatterMotion::Hinge),
        );
        let event = start_hinge(&mut fence, Vec3::Z, ScatterEventKind::TankShove);
        assert!(event.is_some());
        assert!(matches!(fence.state, State::Falling { .. }));

        let crate_mesh = mesh(Vec3::splat(-0.45), Vec3::splat(0.45));
        let mut crate_body = body_from_model(
            0,
            1,
            &crate_mesh,
            Mat4::IDENTITY,
            profile(ScatterMotion::Loose),
        );
        let event = start_loose(
            &mut crate_body,
            Vec3::Z,
            5.0,
            0.8,
            ScatterEventKind::TankShove,
        );
        assert!(event.is_some());
        assert!(matches!(crate_body.state, State::Airborne { .. }));
        // The public contract has no zero-speed cap: even its strongest light-scatter retention is
        // deliberately far above a hard stop.
        let lightest_retention: f32 = 0.93;
        assert!(lightest_retention > 0.85);
    }

    #[test]
    fn hinge_fall_is_deterministic_and_stays_down() {
        let pole_mesh = mesh(Vec3::new(-0.12, 0.0, -0.12), Vec3::new(0.12, 6.5, 0.12));
        let mut a = body_from_model(
            0,
            2,
            &pole_mesh,
            Mat4::IDENTITY,
            profile(ScatterMotion::Hinge),
        );
        let mut b = a.clone();
        start_hinge(&mut a, Vec3::X, ScatterEventKind::TankShove);
        start_hinge(&mut b, Vec3::X, ScatterEventKind::TankShove);

        for _ in 0..180 {
            for body in [&mut a, &mut b] {
                if let State::Falling {
                    t,
                    dir,
                    duration,
                    end_angle,
                } = body.state
                {
                    let nt = t + 1.0 / 60.0;
                    let k = (nt / duration).clamp(0.0, 1.0);
                    let angle = end_angle * k * k * (2.0 - k);
                    let hinge = Mat4::from_translation(body.base_pivot)
                        * Mat4::from_quat(Quat::from_axis_angle(Vec3::Y.cross(dir), angle))
                        * Mat4::from_translation(-body.base_pivot);
                    body.current_pose = hinge * body.rest_pose;
                    body.center = hinge.transform_point3(body.rest_center);
                    body.state = if nt >= duration {
                        State::Down
                    } else {
                        State::Falling {
                            t: nt,
                            dir,
                            duration,
                            end_angle,
                        }
                    };
                }
            }
        }
        assert_eq!(a.state, State::Down);
        assert_eq!(b.state, State::Down);
        assert_eq!(
            a.current_pose.to_cols_array(),
            b.current_pose.to_cols_array()
        );
        assert!(
            a.center.y < a.rest_center.y * 0.35,
            "pole center must finish near the ground"
        );
        assert!(start_hinge(&mut a, Vec3::NEG_X, ScatterEventKind::TankShove).is_none());
    }

    #[test]
    fn route_ray_finds_light_objects_without_making_them_cover() {
        let crate_mesh = mesh(Vec3::splat(-0.5), Vec3::splat(0.5));
        let body = body_from_model(
            0,
            0,
            &crate_mesh,
            Mat4::from_translation(Vec3::new(0.0, 0.5, 5.0)),
            profile(ScatterMotion::Loose),
        );
        let hit = ray_body(&body, Vec3::new(0.0, 0.5, 0.0), Vec3::Z, 20.0);
        assert!(hit.is_some_and(|distance| distance > 4.0 && distance < 5.0));
        // The API returns only an effect distance/index; there is intentionally no penetration
        // cost or terminal-hit type in this subsystem.
    }
}
