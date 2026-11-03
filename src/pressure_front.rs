//! Bounded airborne pressure-front presentation. Arrival travels at the shared acoustic speed;
//! the visual is a representative subframe exposure of a curved refractive cap, not a gas solver.
//! It adds no radiance. A film frame may contain the whole cabin transit, so representative
//! shutter sampling must never be reported as slow-motion physical propagation.
use glam::{Mat4, Vec3};
use std::collections::VecDeque;

/// Speed of sound in the current act's air (319 m/s at the winter acts' −20 °C; `sound_propagation`).
pub fn speed_of_sound_m_s() -> f32 {
    crate::sound_propagation::speed_of_sound()
}
pub const MAX_FRONTS: usize = 4;
pub const MAX_OPACITY: f32 = 0.08;
const RECENT_IDS: usize = 64;

/// The winter acts' air at −20 °C and 101.3 kPa.
pub const AIR_DENSITY_KG_M3: f32 = 1.39;
/// The front is followed to where its overpressure falls under this (140 dB peak), below which
/// no modeled receiver responds; `reach_m` caps it at 240 m.
pub const REACH_FLOOR_KPA: f32 = 0.2;
pub const REACH_MAX_M: f32 = 240.0;

/// Incident peak overpressure (kPa) of a surface burst of `tnt_kg` at `distance_m`: the
/// Kingery-Bulmash fit the report levels use (`sound_levels::kingery_bulmash_kpa`).
pub fn overpressure_kpa(tnt_kg: f32, distance_m: f32) -> f32 {
    let w = tnt_kg.max(1.0e-6);
    crate::sound_levels::kingery_bulmash_kpa(distance_m.max(0.05) / w.cbrt())
}
/// Positive-phase duration (s): the report's A-duration law.
pub fn positive_phase_s(tnt_kg: f32, distance_m: f32) -> f32 {
    crate::sound_levels::blast_a_duration_ms(tnt_kg, distance_m) * 0.001
}
/// Peak particle velocity (m/s) behind the front, the weak-shock relation u = Δp / (ρ c). It
/// overstates a strong front (above ~50 kPa) by up to a third; those distances lie inside the
/// burst's damage radius, where the receivers are broken rather than bent.
pub fn particle_velocity_m_s(overpressure_kpa: f32) -> f32 {
    overpressure_kpa * 1000.0 / (AIR_DENSITY_KG_M3 * speed_of_sound_m_s())
}
/// The dynamic-pressure impulse (Pa s) a thin object standing in the flow takes over the
/// positive phase: ½ ρ u² over a triangular phase. This is what bends grass and crowns; the
/// side-on overpressure loads a small object only for its clearing time, microseconds.
pub fn drag_impulse_pa_s(tnt_kg: f32, distance_m: f32) -> f32 {
    let u = particle_velocity_m_s(overpressure_kpa(tnt_kg, distance_m));
    0.5 * (0.5 * AIR_DENSITY_KG_M3 * u * u) * positive_phase_s(tnt_kg, distance_m)
}
/// The radius the front is followed to (`REACH_FLOOR_KPA`, at most `REACH_MAX_M`).
pub fn reach_m(tnt_kg: f32) -> f32 {
    if overpressure_kpa(tnt_kg, REACH_MAX_M) >= REACH_FLOOR_KPA { return REACH_MAX_M; }
    let (mut lo, mut hi) = (0.5f32, REACH_MAX_M);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if overpressure_kpa(tnt_kg, mid) >= REACH_FLOOR_KPA { lo = mid; } else { hi = mid; }
    }
    0.5 * (lo + hi)
}
/// A dry grass blade in the flow: a flat leaf 5 mm wide at 0.7 g per metre, drag coefficient
/// 1.2, a cantilever whose tip takes the impulse. Returns the tip's velocity (m/s).
pub const BLADE_WIDTH_M: f32 = 0.005;
pub const BLADE_MASS_KG_PER_M: f32 = 0.0007;
pub const BLADE_DRAG: f32 = 1.2;
pub fn blade_tip_velocity_m_s(tnt_kg: f32, distance_m: f32) -> f32 {
    drag_impulse_pa_s(tnt_kg, distance_m) * BLADE_WIDTH_M * BLADE_DRAG / BLADE_MASS_KG_PER_M
}
/// A standing conifer in the flow: a crown a sixth of the square of its height (a cone a third
/// as wide as tall) at a porous drag of 0.6; the stem 500 kg/m³ over 0.4 of a cylinder of the
/// trunk's radius (taper and crown), of which 0.3 moves with the first mode. The crown's
/// velocity (m/s) after the impulse.
pub fn crown_tip_velocity_m_s(tnt_kg: f32, distance_m: f32, height_m: f32, trunk_r_m: f32) -> f32 {
    let area = height_m * height_m / 6.0;
    let mass = 500.0 * std::f32::consts::PI * trunk_r_m * trunk_r_m * height_m * 0.4;
    drag_impulse_pa_s(tnt_kg, distance_m) * area * 0.6 / (0.3 * mass).max(2.0)
}
/// A conifer's first sway frequency (Hz): 5 / height, the measured band (a 5 m pine near 1 Hz,
/// a 20 m one near a quarter).
pub fn tree_sway_hz(height_m: f32) -> f32 { 5.0 / height_m.max(2.0) }
/// A man in the open ducks at the report: nothing under 130 dB peak (a distant boom), a 0.3 s
/// start at 140 dB, a full 1.2 s cover at 170 dB. Presentation of a startle through the
/// soldier's flinch, not a body displacement: the drag impulse on a standing man at these
/// distances is millimetres per second.
pub fn startle_s(tnt_kg: f32, distance_m: f32) -> f32 {
    let db = crate::sound_levels::blast_peak_db(tnt_kg, distance_m);
    ((db - 130.0) / 40.0).clamp(0.0, 1.0) * 1.2
}
/// The exposed viewer's response to the front at the eye, in the armour-hit kick's units
/// (metres of eye displacement decaying over ~0.13 s): a quarter metre at 20 kPa, the front
/// that puts a standing man down, scaling with the square root of the overpressure below it.
/// Presentation of the startle and the chest thump; the head's physical displacement is
/// negligible.
pub fn eye_kick_m(overpressure_kpa: f32) -> f32 {
    0.25 * (overpressure_kpa / 20.0).clamp(0.0, 1.0).sqrt()
}
/// The blade-tip velocity impulse (m/s, world xz) the fronts give the grass at `point` with
/// an arrival in (t0, t1]: radial from each source.
pub fn blade_gust(fronts: &[Front], point: Vec3, t0: f32, t1: f32) -> glam::Vec2 {
    let mut gust = glam::Vec2::ZERO;
    for front in fronts {
        let Some(arrival) = front.arrival_at(point) else { continue };
        if arrival.at <= t0 || arrival.at > t1 { continue; }
        let radial = glam::Vec2::new(arrival.direction.x, arrival.direction.z).normalize_or_zero();
        gust += radial * blade_tip_velocity_m_s(front.tnt_kg, point.distance(front.origin));
    }
    gust
}

#[derive(Clone, Copy, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Front {
    pub id: u64,
    pub origin: Vec3,
    pub direction: Vec3,
    pub emitted_at: f32,
    pub strength: f32,
    pub max_radius: f32,
    /// -1 = sphere; 0 = inward hemisphere from an actual radiating surface.
    pub cap_cosine: f32,
    /// The source's TNT equivalent (kg): the overpressure, impulse and reach at every receiver.
    #[serde(default)]
    pub tnt_kg: f32,
}
impl Front {
    pub fn arrival_at(&self, listener: Vec3) -> Option<Arrival> {
        let delta = listener - self.origin;
        let distance = delta.length();
        if !distance.is_finite()
            || distance > self.max_radius
            || (distance > 0.001 && delta.dot(self.direction) / distance < self.cap_cosine)
        {
            return None;
        }
        Some(Arrival {
            id: self.id,
            at: self.emitted_at + distance / speed_of_sound_m_s(),
            source: self.origin,
            direction: delta.normalize_or_zero(),
            strength: self.strength,
        })
    }
    /// Every point lies on the same sphere; tangent distance bends the cap back toward its source.
    pub fn cap_point(
        &self,
        radius: f32,
        tangent: Vec3,
        bitangent: Vec3,
        u: f32,
        v: f32,
    ) -> Option<Vec3> {
        let axial_sq = radius * radius - u * u - v * v;
        if axial_sq < 0.0 {
            return None;
        }
        let axial = axial_sq.sqrt();
        if radius > 0.0 && axial / radius < self.cap_cosine {
            return None;
        }
        Some(self.origin + tangent * u + bitangent * v + self.direction * axial)
    }
    fn valid(&self) -> bool {
        self.origin.is_finite()
            && self.direction.is_finite()
            && self.emitted_at.is_finite()
            && self.strength.is_finite()
            && self.strength > 0.0
            && self.max_radius.is_finite()
            && self.max_radius > 0.0
            && self.cap_cosine.is_finite()
            && self.direction.length_squared() > 0.5
    }
}
#[derive(Clone, Copy, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Arrival {
    pub id: u64,
    pub at: f32,
    pub source: Vec3,
    pub direction: Vec3,
    pub strength: f32,
}

#[derive(Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Fronts {
    events: VecDeque<Front>,
    recent: VecDeque<u64>,
    /// This frame's hot-object plumes (set each frame; presentation state, not saved).
    #[serde(skip)]
    plumes: Vec<HeatPlume>,
}
impl Fronts {
    /// A duplicate impact cannot schedule a second pressure sound or camera response. Evicting an
    /// optical cap does not undo its receipt; the caller keeps already scheduled arrival payloads.
    pub fn emit(&mut self, mut front: Front, listener: Vec3) -> Option<Arrival> {
        if !front.valid() || self.recent.contains(&front.id) {
            return None;
        }
        front.direction = front.direction.normalize();
        front.cap_cosine = front.cap_cosine.clamp(-1.0, 0.95);
        front.strength = front.strength.clamp(0.0, 1.0);
        let arrival = front.arrival_at(listener);
        self.recent.push_back(front.id);
        while self.recent.len() > RECENT_IDS {
            self.recent.pop_front();
        }
        self.events.push_back(front);
        while self.events.len() > MAX_FRONTS {
            self.events.pop_front();
        }
        arrival
    }
    pub fn clear(&mut self) {
        self.events.clear();
        self.recent.clear();
        self.plumes.clear();
    }
    /// This frame's hot objects (the hottest `MAX_PLUMES` by heat release are drawn).
    pub fn set_plumes(&mut self, mut plumes: Vec<HeatPlume>) {
        plumes.retain(|p| p.base.is_finite() && p.heat_w > 0.0 && p.excess_k > 0.5);
        plumes.sort_by(|a, b| b.heat_w.total_cmp(&a.heat_w));
        plumes.truncate(MAX_PLUMES);
        self.plumes = plumes;
    }
    fn plume_samples(&self) -> ([PlumeGpu; MAX_PLUMES], usize) {
        let mut out = [PlumeGpu::default(); MAX_PLUMES];
        if !physical_refraction() {
            return (out, 0);
        }
        for (o, p) in out.iter_mut().zip(&self.plumes) {
            *o = PlumeGpu { base_heat: p.base.extend(p.heat_w).to_array(), shape: [p.excess_k, p.radius_m, p.reach_m(), p.seed] };
        }
        (out, self.plumes.len().min(MAX_PLUMES))
    }
    /// The fronts still travelling at `now` (within their reach, a tenth of a second's grace):
    /// what the men, the trees and the grass are swept against each update.
    pub fn active(&self, now: f32) -> Vec<Front> {
        self.events.iter().copied()
            .filter(|f| f.tnt_kg > 0.0 && now - f.emitted_at <= f.max_radius / speed_of_sound_m_s() + 0.1)
            .collect()
    }
    fn samples(&self, now: f32, shutter_s: f32, listener: Vec3) -> [FrontGpu; MAX_FRONTS] {
        if physical_refraction() {
            return self.physical_samples(now);
        }
        let mut samples = [FrontGpu::default(); MAX_FRONTS];
        for (output, front) in samples.iter_mut().zip(self.events.iter().rev()) {
            let age1 = now - front.emitted_at;
            if age1 <= 0.0 {
                continue;
            }
            let r0 = ((age1 - shutter_s.max(0.000_1)).max(0.0) * speed_of_sound_m_s()).max(0.015);
            let r1 = (age1 * speed_of_sound_m_s()).min(front.max_radius);
            if r0 >= r1 {
                continue;
            }
            // One representative radius INSIDE this actual exposure interval. Bias toward the
            // front approaching the observer when its millisecond crossing falls between frames.
            // This preserves a readable bow; it is presentation sampling, not a slower wave.
            let radius = (front.origin.distance(listener) * 0.82).clamp(r0, r1);
            let opacity = MAX_OPACITY * front.strength / (1.0 + radius * 0.04);
            *output = FrontGpu {
                origin_radius: front.origin.extend(radius).to_array(),
                direction_cap: front.direction.extend(front.cap_cosine).to_array(),
                optical: [opacity, 0.10, 1.5, 0.0],
            };
        }
        samples
    }
}
/// The physical refraction (2026-11-03, the owner: the air visibly bent by a blast's pressure, as
/// strongly as the physics gives and no more). Behind the setting `KESSEL_REFRACTION=physical`
/// until the owner approves it from stills: the look is locked. Without it the pass draws the
/// representative authored cap above, unchanged.
pub fn physical_refraction() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KESSEL_REFRACTION").is_ok_and(|v| v.trim().eq_ignore_ascii_case("physical")))
}

impl Fronts {
    /// Each front's state at `now` from the blast model (`fea_damage::blast`): its shock radius
    /// (the Rankine-Hugoniot arrival, not the acoustic speed), the length of its positive phase
    /// behind the front, and the air's refractivity jump at the front (Gladstone-Dale on the
    /// Rankine-Hugoniot density). A front from a surface (cap_cosine >= 0) is a surface burst.
    /// optical = [1 (physical), profile length m, refractivity jump, 0].
    fn physical_samples(&self, now: f32) -> [FrontGpu; MAX_FRONTS] {
        use crate::fea_damage::blast;
        let mut samples = [FrontGpu::default(); MAX_FRONTS];
        for (output, front) in samples.iter_mut().zip(self.events.iter().rev()) {
            let age = now - front.emitted_at;
            if age <= 0.0 || front.tnt_kg <= 0.0 {
                continue;
            }
            let charge = blast::Charge { at: front.origin, tnt_kg: front.tnt_kg, t0_s: 0.0, surface: front.cap_cosine >= 0.0 };
            let radius = front_radius(charge.effective_kg(), age).min(front.max_radius);
            if radius >= front.max_radius {
                continue;
            }
            let wave = charge.wave(radius);
            let jump = blast::refractivity(blast::shock_density_ratio(wave.peak_pa)) - blast::refractivity(1.0);
            // The positive phase's length in space: the air behind the front moves with it.
            let length = (blast::C0 * wave.duration_s).clamp(0.01, radius);
            *output = FrontGpu {
                origin_radius: front.origin.extend(radius).to_array(),
                direction_cap: front.direction.extend(front.cap_cosine).to_array(),
                optical: [1.0, length, jump, 0.0],
            };
        }
        samples
    }
}

/// The radius a free-air burst of `w` kg's front has reached `age` s after it (the arrival law
/// `fea_damage::blast::arrival_after_s` inverted by bisection).
pub fn front_radius(w: f32, age: f32) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 2_000.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if crate::fea_damage::blast::arrival_after_s(w, mid) < age { lo = mid; } else { hi = mid; }
    }
    0.5 * (lo + hi)
}

/// A hot object's plume of warm air (2026-11-03, the owner: the air should refract around really hot
/// objects - glowing metal, shells, the barrel, fires). Heskestad's point-source plume (SFPE
/// Handbook, "Fire Plumes"): the centreline temperature excess 9.1 (T/(g c_p² ρ²))^1/3 Q^2/3 z^-5/3,
/// its Gaussian half-width 0.12 z, the centreline rise 3.4 (g Q/(c_p ρ T))^1/3 z^-1/3, z from the
/// source (the virtual origin taken at the source: named). The excess is capped at the source's own
/// (no plume is hotter than what heats it). Its turbulence is a statistical field: temperature
/// fluctuating at 0.38 of the local excess (George, Alpert & Tamanini 1977, on the axis of a
/// buoyant plume), in hashed gradient noise of half the plume's width, carried up at the rise speed
/// (named: the noise stands in for the turbulent field, with its measured intensity and scale).
#[derive(Clone, Copy, Debug)]
pub struct HeatPlume {
    pub base: Vec3,
    /// Convective heat release into the air, W.
    pub heat_w: f32,
    /// The hot surface's temperature excess over the air, K.
    pub excess_k: f32,
    /// The source's own size, m.
    pub radius_m: f32,
    pub seed: f32,
}

pub const MAX_PLUMES: usize = 16;
/// Ambient air for the plumes: the winter acts' -20 C (`AIR_DENSITY_KG_M3`).
pub const PLUME_AIR_K: f32 = 253.0;
const CP_AIR: f32 = 1005.0;
const G: f32 = 9.81;
/// Turbulent temperature intensity on a plume's axis (George, Alpert & Tamanini 1977).
pub const PLUME_FLUCTUATION: f32 = 0.38;
/// Natural-convection coefficient for a hot body's heat into still air, W/m²K (named: the order of
/// laminar and turbulent free convection, 5-25).
pub const FREE_CONVECTION_W_M2K: f32 = 10.0;

impl HeatPlume {
    /// The plume of a hot body of surface `area_m2` at `excess_k` over the air.
    pub fn of_body(base: Vec3, area_m2: f32, excess_k: f32, radius_m: f32, seed: f32) -> Self {
        Self { base, heat_w: FREE_CONVECTION_W_M2K * area_m2 * excess_k.max(0.0), excess_k, radius_m, seed }
    }
    /// Centreline temperature excess (K) `z` m above the source.
    pub fn centre_excess_k(&self, z: f32) -> f32 {
        let rho = AIR_DENSITY_KG_M3;
        let k = 9.1 * (PLUME_AIR_K / (G * CP_AIR * CP_AIR * rho * rho)).cbrt();
        let z = z.max(self.radius_m.max(0.005));
        (k * self.heat_w.max(0.0).powf(2.0 / 3.0) * z.powf(-5.0 / 3.0)).min(self.excess_k)
    }
    pub fn half_width_m(&self, z: f32) -> f32 {
        (0.12 * z).max(self.radius_m.max(0.005))
    }
    /// Centreline rise speed (m/s) at `z`.
    pub fn rise_m_s(&self, z: f32) -> f32 {
        3.4 * (G * self.heat_w.max(0.0) / (CP_AIR * AIR_DENSITY_KG_M3 * PLUME_AIR_K)).cbrt() * z.max(0.05).powf(-1.0 / 3.0)
    }
    /// How high the plume is followed: until its centreline excess falls under 0.5 K.
    pub fn reach_m(&self) -> f32 {
        let (mut lo, mut hi) = (0.0f32, 30.0f32);
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if self.centre_excess_k(mid) > 0.5 { lo = mid; } else { hi = mid; }
        }
        0.5 * (lo + hi)
    }
}

/// Refractivity of air at `kelvin`, from its value at the plume air's temperature (density 1/T).
pub fn air_refractivity_at(kelvin: f32) -> f32 {
    crate::fea_damage::blast::GLADSTONE_DALE * AIR_DENSITY_KG_M3 * PLUME_AIR_K / kelvin.max(1.0)
}

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PlumeGpu {
    /// base xyz, heat W
    base_heat: [f32; 4],
    /// source excess K, source radius m, reach m, seed
    shape: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct FrontGpu {
    origin_radius: [f32; 4],
    direction_cap: [f32; 4],
    optical: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    inverse_view_projection: [[f32; 4]; 4],
    view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    /// width, height, plume count, time s
    viewport: [f32; 4],
    fronts: [FrontGpu; MAX_FRONTS],
    plumes: [PlumeGpu; MAX_PLUMES],
}

/// Dedicated transient pass, deliberately outside surface/volume temporal histories and bloom.
/// High uses the live TLAS to stop each source-to-front path at geometry. Portable omits this
/// optical effect rather than pretend screen depth alone proves transmission through armor.
pub struct Gpu {
    snapshot: wgpu::Texture,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: Option<wgpu::RenderPipeline>,
    views: std::cell::RefCell<Vec<(f32, f32)>>,
    width: u32,
    height: u32,
}
impl Gpu {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        depth: &wgpu::TextureView,
        scene_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Self {
        let snapshot = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pressure-hdr-snapshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::renderer::hdr_format(),
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let snapshot_view = snapshot.create_view(&Default::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pressure-uniform"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pressure-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pressure-bgl"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    3,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pressure-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&snapshot_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        let pipeline = scene_layout.map(|scene| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("pressure-front"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/pressure_front.wgsl").into(),
                ),
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pressure-pl"),
                bind_group_layouts: &[&layout, scene],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("pressure-front"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::renderer::hdr_format(),
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            })
        });
        Self {
            snapshot,
            uniform,
            bind,
            pipeline,
            views: Default::default(),
            width,
            height,
        }
    }
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        hdr: &wgpu::Texture,
        target: &wgpu::TextureView,
        scene: Option<&wgpu::BindGroup>,
        fronts: &Fronts,
        now: f32,
        view: usize,
        camera: Vec3,
        relative_view_projection: Mat4,
    ) -> bool {
        let (Some(pipeline), Some(scene)) = (&self.pipeline, scene) else {
            return false;
        };
        let shutter = {
            let mut views = self.views.borrow_mut();
            while views.len() <= view {
                views.push((now - 1.0 / 60.0, 1.0 / 60.0));
            }
            let (previous, last_dt) = views[view];
            let dt = if now > previous {
                (now - previous).min(0.1)
            } else {
                last_dt
            };
            views[view] = (now, dt);
            dt
        };
        let samples = fronts.samples(now, shutter, camera);
        let (plumes, plume_count) = fronts.plume_samples();
        if samples.iter().all(|sample| sample.optical[0] <= 0.0) && plume_count == 0 {
            return false;
        }
        let u = Uniform {
            inverse_view_projection: relative_view_projection.inverse().to_cols_array_2d(),
            view_projection: relative_view_projection.to_cols_array_2d(),
            camera: camera.extend(0.0).to_array(),
            viewport: [self.width as f32, self.height as f32, plume_count as f32, now],
            fronts: samples,
            plumes,
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        encoder.copy_texture_to_texture(
            hdr.as_image_copy(),
            self.snapshot.as_image_copy(),
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pressure-front"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_bind_group(1, scene, &[]);
        pass.draw(0..3, 0..1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mean bending (rad) of a horizontal ray passing `height` m above a plume's source through
    /// its axis: the transverse gradient of its mean refractivity integrated along the ray.
    fn plume_mean_bend(p: &HeatPlume, height: f32) -> f32 {
        let n = |x: f32, z: f32| {
            let h = p.centre_excess_k(z);
            let w = p.half_width_m(z);
            let t = h * (-(x * x) / (w * w)).exp();
            air_refractivity_at(PLUME_AIR_K + t) - air_refractivity_at(PLUME_AIR_K)
        };
        // Vertical gradient (the ray is bent up or down), across the plume.
        let (dz, ds) = (1.0e-3, 1.0e-3);
        let w = p.half_width_m(height) * 4.0;
        let mut bend = 0.0;
        let mut x = -w;
        while x < w {
            bend += (n(x, height + dz) - n(x, height - dz)) / (2.0 * dz) * ds;
            x += ds;
        }
        bend.abs()
    }

    #[test]
    fn a_plume_cools_and_widens_as_it_rises_and_bends_light_by_its_heat() {
        // A fire patch: about 20 kW into the air.
        let fire = HeatPlume { base: Vec3::ZERO, heat_w: 20_000.0, excess_k: 850.0, radius_m: 0.15, seed: 1.0 };
        assert!(fire.centre_excess_k(0.5) > fire.centre_excess_k(1.0) && fire.centre_excess_k(2.0) > 0.0);
        assert!(fire.half_width_m(2.0) > fire.half_width_m(1.0));
        assert!(fire.rise_m_s(1.0) > 1.0 && fire.rise_m_s(1.0) < 10.0, "{}", fire.rise_m_s(1.0));
        // Nothing is hotter than its source.
        let chip = HeatPlume::of_body(Vec3::ZERO, 6.0 * 0.01 * 0.01, 600.0, 0.005, 2.0);
        assert!(chip.centre_excess_k(0.001) <= 600.0);
        assert!(plume_mean_bend(&fire, 1.0) > plume_mean_bend(&chip, 0.05));
        // The air is less dense where it is hot.
        assert!(air_refractivity_at(500.0) < air_refractivity_at(PLUME_AIR_K));
    }

    #[test]
    #[ignore]
    fn print_plume_bending() {
        let px = 60f32.to_radians() / 1024.0;
        for (name, p, heights) in [
            ("fire patch 20 kW", HeatPlume { base: Vec3::ZERO, heat_w: 20_000.0, excess_k: 850.0, radius_m: 0.15, seed: 1.0 }, [0.3, 1.0, 2.0]),
            ("glowing rim 0.03 m² at 900 K", HeatPlume::of_body(Vec3::ZERO, 0.03, 650.0, 0.1, 1.0), [0.05, 0.2, 0.5]),
            ("1 cm chip at 900 K", HeatPlume::of_body(Vec3::ZERO, 6.0e-4, 650.0, 0.005, 1.0), [0.01, 0.05, 0.2]),
            ("spent case 0.2 m² at 423 K", HeatPlume::of_body(Vec3::ZERO, 0.2, 170.0, 0.06, 1.0), [0.1, 0.3, 1.0]),
            ("barrel 1.5 m² at 400 K", HeatPlume::of_body(Vec3::ZERO, 1.5, 147.0, 0.1, 1.0), [0.1, 0.3, 1.0]),
        ] {
            for h in heights {
                let b = plume_mean_bend(&p, h);
                println!("{name}: {:.0} W, at {h} m excess {:.1} K, width {:.3} m, reach {:.2} m; mean bend {:.2e} rad = {:.3} px (turbulence about 0.38 of it in fluctuation)",
                    p.heat_w, p.centre_excess_k(h), p.half_width_m(h), p.reach_m(), b, b / px);
            }
        }
    }

    fn front(id: u64) -> Front {
        Front {
            id,
            origin: Vec3::ZERO,
            direction: Vec3::Z,
            emitted_at: 2.0,
            strength: 1.0,
            max_radius: 80.0,
            cap_cosine: 0.0,
            tnt_kg: 0.71,
        }
    }
    #[test]
    fn the_charge_sets_overpressure_impulse_and_reach() {
        // 3.67 kg (a turret thrown): grass flattened at 15 m, a whip at 40 m, a tremor at 100 m.
        assert!(blade_tip_velocity_m_s(3.67, 15.0) > 5.0);
        let forty = blade_tip_velocity_m_s(3.67, 40.0);
        assert!(forty > 1.0 && forty < 5.0, "{forty}");
        assert!(blade_tip_velocity_m_s(3.67, 100.0) < 1.0);
        // A 10 m pine 15 m from it sways centimetres, not metres.
        let crown = crown_tip_velocity_m_s(3.67, 15.0, 10.0, 0.15);
        assert!(crown > 0.1 && crown < 3.0, "{crown}");
        // The reach follows the charge: a ground strike's grams stop within forty metres.
        assert!(reach_m(0.002) < 40.0);
        let kill = reach_m(0.71);
        assert!(kill > 150.0 && kill <= REACH_MAX_M, "{kill}");
        assert_eq!(reach_m(3.67), REACH_MAX_M);
        // Men duck at a near burst and ignore a distant small one.
        assert!(startle_s(3.67, 30.0) > 1.0);
        assert_eq!(startle_s(0.002, 100.0), 0.0);
        assert!(eye_kick_m(20.0) == 0.25 && eye_kick_m(5.0) < 0.13 && eye_kick_m(0.0) == 0.0);
    }
    #[test]
    fn sweeps_take_only_the_window_of_arrival() {
        let f = Front { origin: Vec3::ZERO, direction: Vec3::Y, cap_cosine: -1.0, max_radius: 240.0, tnt_kg: 3.67, ..front(3) };
        let c = speed_of_sound_m_s();
        let point = Vec3::new(30.0, 0.0, 0.0);
        let at = 2.0 + 30.0 / c;
        assert_eq!(blade_gust(&[f], point, at + 0.001, at + 0.02), glam::Vec2::ZERO);
        let gust = blade_gust(&[f], point, at - 0.02, at + 0.001);
        assert!(gust.x > 1.0 && gust.y.abs() < 1e-4, "{gust}");
        let mut fs = Fronts::default();
        fs.emit(f, Vec3::Z * 500.0);
        assert_eq!(fs.active(2.5).len(), 1);
        assert!(fs.active(2.0 + 240.0 / c + 0.2).is_empty());
    }
    #[test]
    fn arrival_uses_real_distance_and_rejects_backside_or_out_of_range() {
        let f = front(1);
        let arrival = f.arrival_at(Vec3::Z * 2.0).unwrap();
        assert!((arrival.at - (2.0 + 2.0 / speed_of_sound_m_s())).abs() < 1e-6);
        assert!(f.arrival_at(-Vec3::Z).is_none());
        assert!(f.arrival_at(Vec3::Z * 81.0).is_none());
        assert_eq!(arrival.id, 1);
    }
    #[test]
    fn cap_has_constant_radius_and_bows_toward_source_at_edges() {
        let f = front(1);
        let middle = f.cap_point(2.0, Vec3::X, Vec3::Y, 0.0, 0.0).unwrap();
        for u in [-1.6, -0.8, 0.8, 1.6] {
            let p = f.cap_point(2.0, Vec3::X, Vec3::Y, u, 0.0).unwrap();
            assert!((p.length() - 2.0).abs() < 1e-6);
            assert!(p.z < middle.z);
        }
        assert!(f.cap_point(2.0, Vec3::X, Vec3::Y, 2.1, 0.0).is_none());
    }
    #[test]
    fn subframe_sampling_is_bounded_to_exposure_and_does_not_slow_arrival() {
        let mut fs = Fronts::default();
        let arrival = fs.emit(front(1), Vec3::Z * 2.0).unwrap();
        for hz in [24.0, 60.0, 120.0, 240.0] {
            let now = 2.0 + 1.0 / hz;
            let samples = fs.samples(now, 1.0 / hz, Vec3::Z * 2.0);
            let s = samples[0];
            assert!(s.optical[0] > 0.0 && s.optical[0] <= MAX_OPACITY);
            assert!(s.origin_radius[3] <= speed_of_sound_m_s() / hz + 0.0001);
            assert!((arrival.at - (2.0 + 2.0 / speed_of_sound_m_s())).abs() < 1e-6);
        }
        assert!(fs
            .samples(2.4, 1.0 / 24.0, Vec3::Z * 2.0)
            .iter()
            .all(|s| s.optical[0] == 0.0));
    }
    #[test]
    fn one_event_yields_one_arrival_even_after_optical_eviction() {
        let mut fs = Fronts::default();
        let mut arrivals = Vec::new();
        for id in 0..16 {
            if let Some(a) = fs.emit(front(id), Vec3::Z) {
                arrivals.push(a);
            }
        }
        assert_eq!(fs.events.len(), MAX_FRONTS);
        assert_eq!(arrivals.len(), 16);
        for id in 0..16 {
            assert!(fs.emit(front(id), Vec3::Z).is_none());
        }
        let invalid = Front {
            origin: Vec3::splat(f32::NAN),
            ..front(99)
        };
        assert!(fs.emit(invalid, Vec3::Z).is_none());
    }
    #[test]
    fn shader_validates_and_uses_geometry_visibility_without_added_white_energy() {
        let source = include_str!("../shaders/pressure_front.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("pressure WGSL");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("valid pressure WGSL");
        let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains("source_blocked(front.origin_radius.xyz,radial,radius)"));
        assert!(compact.contains("min(front.optical.x,0.08)"));
        assert!(compact.contains("mix(original.rgb,refracted,weight)"));
    }
}
