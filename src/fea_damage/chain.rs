//! The layered residual-velocity chain: the struck plate's calibrated result, then every layer the
//! shot line crosses behind it — air, casings, homogenised interiors, liquids, live rounds and the
//! far wall — each with its own named law. docs/FEA_DAMAGE_MODEL_20261102.md, "Stage 1".
//!
//! Laws (each crossing records the one it used):
//! * `struck plate`: the authoritative `ImpactEvent`, untouched. A stopped round reaches the depth
//!   where its calibrated capability is spent, spent uniformly along its path through the plate
//!   (depth = pen / effective × path); a named reduced rule.
//! * `armour (DeMarre/Lambert-Jonas)`: further armour at the current speed, the project's own law
//!   including its 0.92 residual factor.
//! * `solid layer (DeMarre/Lambert-Jonas, Recht-Ipson plug share)`: casings and solid parts as
//!   RHA-equivalent thickness (areal density × material resistance, `materials`), the same slope
//!   law, and the momentum share of the plug of the layer's thickness at the shell's diameter,
//!   `a = m / (m + m_plug)` (Recht & Ipson 1963), in place of the fixed 0.92.
//! * `homogenised interior`: the same capability against the path's RHA-equivalent, no plug.
//! * `hydrodynamic drag`: liquids and gas, `v = v0 exp(-rho Cd A L / 2m)`, Cd 0.5 (named estimate
//!   for an ogive nose in a liquid).
//!
//! The projectile is rigid and keeps its mass (named: no erosion, no APCR carrier stripping, no
//! yaw). A round with filler that perforated bursts at the struck plate's back face: the existing
//! game behaviour (no fuze-delay model).

use glam::Vec3;
use serde::Serialize;

use super::components::{self, ComponentSet, Interior, VoxelEnvelope, COMPONENT_SPECS};
use super::materials::{self, MaterialId};
use crate::ballistics::{ImpactEvent, ShellKind, ShellSpec, Terminal};

/// Drag coefficient of a shell's nose in a liquid or gas layer (named estimate).
pub const LIQUID_DRAG_CD: f32 = 0.5;
/// Below this speed a round in a liquid is taken as stopped (it sinks; named).
pub const LIQUID_STOP_MPS: f32 = 20.0;
/// Filler mass above which a perforating round bursts at the plate's back face (the existing game
/// rule in `resolve_player_penetration`).
pub const BURST_FILLER_G: f32 = 10.0;
/// Homogenised density of stowed 8.8 cm rounds, kg/m³ (a complete round ~14.7 kg in a
/// 88 mm × 0.93 m envelope; estimate).
pub const LIVE_ROUND_DENSITY: f32 = 2580.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum LayerKind {
    StruckPlate,
    Armour { plate_mm: f32, material: MaterialId },
    Casing { component: usize, material: MaterialId, entering: bool },
    Interior { component: usize, matrix: MaterialId, density: f32 },
    Liquid { component: usize, material: MaterialId },
    Vapour { component: usize },
    LiveRound { location_id: u32 },
    Air,
}

impl LayerKind {
    pub fn component(self) -> Option<usize> {
        match self {
            LayerKind::Casing { component, .. }
            | LayerKind::Interior { component, .. }
            | LayerKind::Liquid { component, .. }
            | LayerKind::Vapour { component } => Some(component),
            _ => None,
        }
    }
    pub fn solid(self) -> bool {
        matches!(self, LayerKind::StruckPlate | LayerKind::Armour { .. } | LayerKind::Casing { .. }
            | LayerKind::Interior { .. } | LayerKind::LiveRound { .. })
    }
}

/// One stretch of the shot line, `s0..s1` metres along it from the struck plate's outer face.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Layer {
    pub kind: LayerKind,
    pub s0: f32,
    pub s1: f32,
    /// Cosine of the angle between the path and the layer's surface normal where it is entered.
    pub cos_obliquity: f32,
    /// Hull-local outward normal of the entered surface (zero for interiors and air).
    pub normal: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Fate {
    Perforated,
    /// Stopped inside the layer, this far into it along the path.
    Stopped { depth_m: f32 },
    /// Crossed a liquid, gas or air layer.
    Passed,
    /// A round with filler burst behind the struck plate.
    Burst,
    /// The struck plate deflected the round outward.
    Ricochet,
    /// The struck plate broke the round up.
    Shattered,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Crossing {
    pub layer: Layer,
    pub v_in: f32,
    pub v_out: f32,
    pub energy_j: f32,
    /// Capability and effective thickness for solid layers (mm RHA), zero otherwise.
    pub pen_mm: f32,
    pub effective_mm: f32,
    pub fate: Fate,
    pub law: &'static str,
}

/// A system the chain reached.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ComponentHit {
    pub component: usize,
    /// The casing was perforated (the round entered the interior).
    pub breached: bool,
    /// The round stopped in this component.
    pub stopped: bool,
    pub energy_j: f32,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Lodging {
    /// Index of the crossing the round stopped in.
    pub crossing: usize,
    /// Path distance of the nose tip from the struck plate's outer face (m).
    pub tip_s: f32,
    /// How far the nose went into that layer (m).
    pub depth_m: f32,
    /// The struck plate itself (pulled from outside), or a layer inside the vehicle.
    pub in_struck_plate: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChainResult {
    pub crossings: Vec<Crossing>,
    pub hits: Vec<ComponentHit>,
    pub lodged: Option<Lodging>,
    /// The round left the vehicle through the far wall at this speed.
    pub exit_speed: Option<f32>,
    /// It burst behind the struck plate (filler).
    pub burst: bool,
    /// Kinetic energy at the struck face, and the sum the crossings removed (closure check).
    pub energy_in_j: f32,
    pub energy_removed_j: f32,
}

impl ChainResult {
    /// Where along the line the round ended: its stop, burst, or the far wall's outer face.
    pub fn end_s(&self) -> f32 {
        self.crossings.last().map_or(0.0, |c| match c.fate {
            Fate::Stopped { depth_m } => c.layer.s0 + depth_m,
            _ => c.layer.s1,
        })
    }
    pub fn summary(&self) -> String {
        let mut out = String::new();
        for c in &self.crossings {
            let what = match c.layer.kind {
                LayerKind::StruckPlate => "struck plate".to_string(),
                LayerKind::Armour { plate_mm, .. } => format!("far wall {plate_mm:.0} mm"),
                LayerKind::Casing { component, entering, .. } => format!("{} casing {}",
                    COMPONENT_SPECS[component].name, if entering { "in" } else { "out" }),
                LayerKind::Interior { component, .. } => format!("{} interior", COMPONENT_SPECS[component].name),
                LayerKind::Liquid { component, .. } => format!("{} liquid", COMPONENT_SPECS[component].name),
                LayerKind::Vapour { component } => format!("{} vapour", COMPONENT_SPECS[component].name),
                LayerKind::LiveRound { location_id } => format!("live round {location_id:#x}"),
                LayerKind::Air => "air".to_string(),
            };
            out.push_str(&format!("{what}: {:.0}->{:.0} m/s {:?}; ", c.v_in, c.v_out, c.fate));
        }
        out
    }
}

/// Speed after a solid layer whose RHA-equivalent effective thickness is `eff` (mm), by the
/// calibrated capability at `v`; `None` when the round stops in it.
fn through_solid(shell: &ShellSpec, v: f32, eff: f32, momentum_share: f32) -> (Option<f32>, f32) {
    let pen = shell.pen_flat_at_v(v);
    if eff <= 0.0 {
        return (Some(v), pen);
    }
    if pen <= eff {
        return (None, pen);
    }
    let v_bl = v * (eff / pen).powf(1.0 / crate::ballistics::DEMARRE_EXPONENT);
    (Some(momentum_share * (v * v - v_bl * v_bl).max(0.0).sqrt()), pen)
}

fn drag_speed(v: f32, density: f32, length_m: f32, mass: f32, diameter_m: f32) -> f32 {
    let area = std::f32::consts::FRAC_PI_4 * diameter_m * diameter_m;
    v * (-(density * LIQUID_DRAG_CD * area * length_m) / (2.0 * mass)).exp()
}

/// A homogenised interior (or a live round) of mean `density` over `path` metres.
fn interior_crossing(crossing: &mut Crossing, shell: &ShellSpec, v: f32, mat: &materials::Material,
    density: f32, path: f32) {
    let rha = materials::rha_equivalent_mm(mat, path, density);
    let eff = crate::ballistics::layer_effective_mm(shell, rha, 0.0);
    let (after, pen) = through_solid(shell, v, eff, 1.0);
    crossing.pen_mm = pen;
    crossing.effective_mm = eff;
    crossing.law = "homogenised interior: DeMarre capability against areal density, no plug";
    match after {
        Some(vo) => { crossing.v_out = vo; crossing.fate = Fate::Perforated; }
        None => {
            crossing.v_out = 0.0;
            crossing.fate = Fate::Stopped { depth_m: (pen / eff).clamp(0.0, 0.999) * path };
        }
    }
}

/// Follow the round through `layers` (sorted, non-overlapping, the struck plate first).
pub fn run(event: &ImpactEvent, layers: &[Layer], air_density: f32) -> ChainResult {
    let shell = event.shell;
    let m = shell.mass_kg;
    let d = shell.caliber_mm * 0.001;
    let ke = |v: f32| 0.5 * m * v * v;
    let mut out = ChainResult {
        crossings: Vec::new(),
        hits: Vec::new(),
        lodged: None,
        exit_speed: None,
        burst: false,
        energy_in_j: ke(event.striking_v),
        energy_removed_j: 0.0,
    };
    let mut v = event.striking_v;
    for (index, layer) in layers.iter().enumerate() {
        let path = (layer.s1 - layer.s0).max(0.0);
        let mut crossing = Crossing { layer: *layer, v_in: v, v_out: v, energy_j: 0.0, pen_mm: 0.0,
            effective_mm: 0.0, fate: Fate::Passed, law: "" };
        match layer.kind {
            LayerKind::StruckPlate => {
                crossing.pen_mm = event.pen_mm;
                crossing.effective_mm = event.effective_mm;
                crossing.law = "struck plate: calibrated ImpactEvent";
                match event.terminal {
                    Terminal::Penetration => {
                        crossing.v_out = event.residual_v;
                        let bursts = shell.filler_g > BURST_FILLER_G && shell.kind != ShellKind::Heat;
                        crossing.fate = if bursts { Fate::Burst } else { Fate::Perforated };
                    }
                    Terminal::Ricochet => {
                        crossing.v_out = 0.0;
                        crossing.fate = Fate::Ricochet;
                    }
                    Terminal::Shatter => {
                        crossing.v_out = 0.0;
                        crossing.fate = Fate::Shattered;
                    }
                    Terminal::Bounce | Terminal::HeBlast => {
                        crossing.v_out = 0.0;
                        let spent = if event.effective_mm > 0.0 {
                            (event.pen_mm / event.effective_mm).clamp(0.0, 0.999)
                        } else { 0.0 };
                        crossing.fate = Fate::Stopped { depth_m: spent * path };
                    }
                }
            }
            LayerKind::Armour { plate_mm, material } => {
                let mat = material.get();
                let angle = layer.cos_obliquity.clamp(0.05, 1.0).acos().to_degrees();
                let rha = plate_mm * mat.ke_resistance;
                let eff = crate::ballistics::layer_effective_mm(shell, rha, angle);
                let (after, pen) = through_solid(shell, v, eff, 0.92);
                crossing.pen_mm = pen;
                crossing.effective_mm = eff;
                crossing.law = "armour: DeMarre capability, Lambert-Jonas residual (project law, 0.92)";
                match after {
                    Some(vo) => { crossing.v_out = vo; crossing.fate = Fate::Perforated; }
                    None => {
                        crossing.v_out = 0.0;
                        crossing.fate = Fate::Stopped { depth_m: (pen / eff).clamp(0.0, 0.999) * path };
                    }
                }
            }
            LayerKind::Casing { material, .. } => {
                let mat = material.get();
                let cos = layer.cos_obliquity.clamp(0.05, 1.0);
                let normal_thickness = path * cos;
                let rha = materials::rha_equivalent_mm(mat, normal_thickness, mat.density);
                let eff = crate::ballistics::layer_effective_mm(shell, rha, cos.acos().to_degrees());
                let plug = mat.density * std::f32::consts::FRAC_PI_4 * d * d * normal_thickness;
                let (after, pen) = through_solid(shell, v, eff, m / (m + plug));
                crossing.pen_mm = pen;
                crossing.effective_mm = eff;
                crossing.law = "solid layer: DeMarre capability, Lambert-Jonas residual, Recht-Ipson plug share";
                match after {
                    Some(vo) => { crossing.v_out = vo; crossing.fate = Fate::Perforated; }
                    None => {
                        crossing.v_out = 0.0;
                        crossing.fate = Fate::Stopped { depth_m: (pen / eff).clamp(0.0, 0.999) * path };
                    }
                }
            }
            LayerKind::Interior { matrix, density, .. } => {
                interior_crossing(&mut crossing, shell, v, matrix.get(), density, path);
            }
            LayerKind::LiveRound { .. } => {
                interior_crossing(&mut crossing, shell, v, MaterialId::Copper.get(), LIVE_ROUND_DENSITY, path);
            }
            LayerKind::Liquid { material, .. } => {
                crossing.law = "hydrodynamic drag";
                let vo = drag_speed(v, material.get().density, path, m, d);
                if vo < LIQUID_STOP_MPS {
                    // Distance to slow to the stop speed under the same exponential law.
                    let area = std::f32::consts::FRAC_PI_4 * d * d;
                    let k = material.get().density * LIQUID_DRAG_CD * area / (2.0 * m);
                    let depth = ((v / LIQUID_STOP_MPS).ln() / k).clamp(0.0, path);
                    crossing.v_out = 0.0;
                    crossing.fate = Fate::Stopped { depth_m: depth };
                } else {
                    crossing.v_out = vo;
                }
            }
            LayerKind::Vapour { .. } | LayerKind::Air => {
                crossing.law = "aerodynamic drag";
                crossing.v_out = drag_speed(v, air_density, path, m, d);
            }
        }
        crossing.energy_j = (ke(crossing.v_in) - ke(crossing.v_out)).max(0.0);
        out.energy_removed_j += crossing.energy_j;
        v = crossing.v_out;
        let fate = crossing.fate;
        out.crossings.push(crossing);
        if let Some(component) = layer.kind.component() {
            let breached = matches!(layer.kind, LayerKind::Casing { entering: true, .. })
                && matches!(fate, Fate::Perforated)
                || matches!(layer.kind, LayerKind::Interior { .. } | LayerKind::Liquid { .. } | LayerKind::Vapour { .. });
            let stopped = matches!(fate, Fate::Stopped { .. });
            match out.hits.iter_mut().find(|h| h.component == component) {
                Some(hit) => {
                    hit.breached |= breached;
                    hit.stopped |= stopped;
                    hit.energy_j += crossing.energy_j;
                }
                None => out.hits.push(ComponentHit { component, breached, stopped, energy_j: crossing.energy_j }),
            }
        }
        match fate {
            Fate::Stopped { depth_m } => {
                let in_struck_plate = matches!(layer.kind, LayerKind::StruckPlate);
                // In the struck plate only a body pushed past its nose taper is held; inside the
                // vehicle the round stays where it stopped (`lodged`).
                let nose = super::lodged::nose_length_calibres(shell.kind) * d;
                let deep_enough = !in_struck_plate || super::lodged::retained(depth_m, nose);
                let kinetic = matches!(shell.kind, ShellKind::Ap | ShellKind::Apcbc | ShellKind::Apcr);
                if deep_enough && kinetic && event.terminal != Terminal::HeBlast {
                    out.lodged = Some(Lodging { crossing: index, tip_s: layer.s0 + depth_m, depth_m, in_struck_plate });
                }
                return out;
            }
            Fate::Burst => {
                out.burst = true;
                return out;
            }
            Fate::Ricochet | Fate::Shattered => return out,
            Fate::Perforated | Fate::Passed => {}
        }
    }
    if matches!(layers.last().map(|l| l.kind), Some(LayerKind::Armour { .. })) {
        out.exit_speed = Some(v);
    }
    out
}

/// The struck plate's far face and the opposite wall along the shot line.
#[derive(Clone, Copy, Debug)]
pub struct FarWall {
    /// Path distance from the struck face to the far wall's inner face (m).
    pub s_inner: f32,
    pub plate_mm: f32,
    pub material: MaterialId,
    pub cos_obliquity: f32,
    pub normal: Vec3,
}

/// A live cartridge's oriented box in the hull frame (the game's live-ammunition record).
#[derive(Clone, Copy, Debug)]
pub struct RoundBox {
    pub location_id: u32,
    pub center: Vec3,
    pub axes: [Vec3; 3],
    pub half: Vec3,
}

/// Everything the layer builder needs to know about the vehicle's present state.
#[derive(Clone, Copy, Debug)]
pub struct VehicleState<'a> {
    pub articulation: components::Articulation,
    /// Fraction of the fuel system's capacity in the tanks.
    pub fuel_fill: f32,
    /// Gravity direction in the hull frame (unit, pointing down).
    pub gravity_hull: Vec3,
    pub live_rounds: &'a [RoundBox],
}

/// The shot line in the hull frame.
#[derive(Clone, Copy, Debug)]
pub struct ShotLine {
    /// The strike point on the struck plate's outer face.
    pub entry: Vec3,
    /// Unit travel direction.
    pub dir: Vec3,
    /// Path length through the struck plate (thickness / cos obliquity), m.
    pub plate_path_m: f32,
    /// The far wall, if the line meets one.
    pub far_wall: Option<FarWall>,
    /// Where the line stops being followed when there is no far wall (m).
    pub max_s: f32,
}

fn slab(line: &ShotLine, b: &RoundBox, s_end: f32) -> Option<(f32, f32)> {
    let mut t0 = 0.0f32;
    let mut t1 = s_end;
    let rel = line.entry - b.center;
    for k in 0..3 {
        let axis = b.axes[k];
        let e = rel.dot(axis);
        let f = line.dir.dot(axis);
        if f.abs() < 1e-9 {
            if e.abs() > b.half[k] {
                return None;
            }
        } else {
            let mut ta = (-b.half[k] - e) / f;
            let mut tb = (b.half[k] - e) / f;
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 >= t1 {
                return None;
            }
        }
    }
    Some((t0, t1))
}

/// Fuel surface height (along -gravity) in a tank's frame that leaves `fill` of its voxel volume
/// below it. Exact for the voxelised volume.
pub fn liquid_level(env: &VoxelEnvelope, up: Vec3, fill: f32) -> f32 {
    let mut heights = Vec::with_capacity(env.occupied_cells);
    for ix in 0..env.dims[0] as i64 {
        for iz in 0..env.dims[2] as i64 {
            for iy in 0..env.dims[1] as i64 {
                if env.occupied(ix, iy, iz) {
                    let c = env.origin + (Vec3::new(ix as f32, iy as f32, iz as f32) + 0.5) * env.cell;
                    heights.push(c.dot(up));
                }
            }
        }
    }
    if heights.is_empty() {
        return f32::NEG_INFINITY;
    }
    heights.sort_by(f32::total_cmp);
    let fill = fill.clamp(0.0, 1.0);
    if fill <= 0.0 {
        return heights[0] - env.cell;
    }
    let k = ((fill * heights.len() as f32).ceil() as usize).clamp(1, heights.len());
    heights[k - 1] + 0.5 * env.cell
}

/// Build the ordered layers along `line` through `set`'s components in `state`.
pub fn build_layers(line: &ShotLine, set: &ComponentSet, state: &VehicleState) -> Vec<Layer> {
    let dir = line.dir.normalize_or_zero();
    let s_plate = line.plate_path_m.max(0.0);
    let s_end = line.far_wall.map_or(line.max_s, |w| w.s_inner).max(s_plate);
    let mut solids: Vec<Layer> = Vec::new();
    let a = line.entry + dir * s_plate;
    let b = line.entry + dir * s_end;
    let span = s_end - s_plate;
    if span > 1e-4 {
        for env in &set.envelopes {
            let spec = &COMPONENT_SPECS[env.spec];
            let ac = state.articulation.to_component(env, a);
            let bc = state.articulation.to_component(env, b);
            for (t_in, t_out) in env.segment_runs(ac, bc) {
                let s_in = s_plate + t_in * span;
                let s_out = s_plate + t_out * span;
                let probe_in = ac + (bc - ac) * (t_in + 0.25 * env.cell / span.max(1e-4)).min(1.0);
                let probe_out = ac + (bc - ac) * (t_out - 0.25 * env.cell / span.max(1e-4)).max(0.0);
                let n_in = state.articulation.dir_to_hull(env, env.normal_at(probe_in)).normalize_or_zero();
                let n_out = state.articulation.dir_to_hull(env, env.normal_at(probe_out)).normalize_or_zero();
                let cos_in = (-dir).dot(n_in).abs().clamp(0.05, 1.0);
                let cos_out = dir.dot(n_out).abs().clamp(0.05, 1.0);
                let run = s_out - s_in;
                let casing = spec.casing_mm * 0.001;
                let c_in = (casing / cos_in).min(run * 0.5);
                let c_out = (casing / cos_out).min(run * 0.5);
                let component = env.spec;
                solids.push(Layer { kind: LayerKind::Casing { component, material: spec.casing, entering: true },
                    s0: s_in, s1: s_in + c_in, cos_obliquity: cos_in, normal: n_in });
                let (i0, i1) = (s_in + c_in, s_out - c_out);
                if i1 > i0 + 1e-5 {
                    match spec.interior {
                        Interior::Homogenised { matrix, real_mass_kg } => solids.push(Layer {
                            kind: LayerKind::Interior { component, matrix, density: components::path_density(env, real_mass_kg) },
                            s0: i0, s1: i1, cos_obliquity: 1.0, normal: Vec3::ZERO }),
                        Interior::Tank { liquid } => {
                            let up = -state.articulation.to_component_dir(env, state.gravity_hull).normalize_or_zero();
                            let level = liquid_level(env, up, state.fuel_fill);
                            let p0 = ac + (bc - ac) * ((i0 - s_plate) / span);
                            let p1 = ac + (bc - ac) * ((i1 - s_plate) / span);
                            let (h0, h1) = (p0.dot(up), p1.dot(up));
                            // The path is straight: the liquid part is where height < level.
                            let cut = if (h1 - h0).abs() > 1e-9 { ((level - h0) / (h1 - h0)).clamp(0.0, 1.0) } else if h0 < level { 1.0 } else { 0.0 };
                            let s_cut = i0 + (i1 - i0) * cut;
                            let (first_liquid, second_liquid) = (h0 < level, h1 < level);
                            let mut push = |s0: f32, s1: f32, liquid_here: bool| {
                                if s1 > s0 + 1e-6 {
                                    solids.push(Layer { kind: if liquid_here { LayerKind::Liquid { component, material: liquid } }
                                        else { LayerKind::Vapour { component } }, s0, s1, cos_obliquity: 1.0, normal: Vec3::ZERO });
                                }
                            };
                            push(i0, s_cut, first_liquid);
                            push(s_cut, i1, second_liquid);
                        }
                        Interior::Empty => {}
                    }
                }
                solids.push(Layer { kind: LayerKind::Casing { component, material: spec.casing, entering: false },
                    s0: s_out - c_out, s1: s_out, cos_obliquity: cos_out, normal: n_out });
            }
        }
        for round in state.live_rounds {
            if let Some((t0, t1)) = slab(line, round, s_end) {
                let (s0, s1) = (t0.max(s_plate), t1.min(s_end));
                if s1 > s0 + 1e-5 {
                    solids.push(Layer { kind: LayerKind::LiveRound { location_id: round.location_id }, s0, s1,
                        cos_obliquity: 1.0, normal: Vec3::ZERO });
                }
            }
        }
    }
    // Sort by start; the earlier layer keeps any overlap (voxel staircases overlap by under a cell).
    solids.sort_by(|x, y| x.s0.total_cmp(&y.s0).then(x.s1.total_cmp(&y.s1)));
    let mut layers = vec![Layer { kind: LayerKind::StruckPlate, s0: 0.0, s1: s_plate, cos_obliquity: 1.0, normal: Vec3::ZERO }];
    let mut cursor = s_plate;
    for mut l in solids {
        if l.s1 <= cursor + 1e-6 {
            continue;
        }
        if l.s0 < cursor {
            l.s0 = cursor;
        }
        if l.s0 > cursor + 1e-6 {
            layers.push(Layer { kind: LayerKind::Air, s0: cursor, s1: l.s0, cos_obliquity: 1.0, normal: Vec3::ZERO });
        }
        cursor = l.s1;
        layers.push(l);
    }
    if cursor < s_end - 1e-6 {
        layers.push(Layer { kind: LayerKind::Air, s0: cursor, s1: s_end, cos_obliquity: 1.0, normal: Vec3::ZERO });
        cursor = s_end;
    }
    if let Some(w) = line.far_wall {
        let path = w.plate_mm * 0.001 / w.cos_obliquity.clamp(0.05, 1.0);
        layers.push(Layer { kind: LayerKind::Armour { plate_mm: w.plate_mm, material: w.material },
            s0: cursor, s1: cursor + path, cos_obliquity: w.cos_obliquity, normal: w.normal });
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ballistics::{self, ArmorMaterial, Plate};

    fn tiger_plate(name: &'static str, mm: f32) -> Plate {
        Plate { name, thickness_mm: mm, slope_deg: 0.0, material: ArmorMaterial::Rha }
    }

    fn state<'a>() -> VehicleState<'a> {
        VehicleState {
            articulation: components::Articulation::rest(Vec3::new(0.0, 0.524, 0.019)),
            fuel_fill: 0.8,
            gravity_hull: Vec3::NEG_Y,
            live_rounds: &[],
        }
    }

    /// A shot into the front lower plate along the hull's axis, toward the gearbox.
    fn front_line(plate_mm: f32) -> ShotLine {
        ShotLine {
            entry: Vec3::new(-0.04, 1.06, 3.55),
            dir: Vec3::new(0.0, 0.0, -1.0),
            plate_path_m: plate_mm * 0.001,
            far_wall: Some(FarWall { s_inner: 6.9, plate_mm: 80.0, material: MaterialId::RolledArmour,
                cos_obliquity: 1.0, normal: Vec3::NEG_Z }),
            max_s: 7.2,
        }
    }

    #[test]
    fn layers_are_ordered_contiguous_and_start_with_the_struck_plate() {
        let set = components::tiger_components();
        let layers = build_layers(&front_line(100.0), set, &state());
        assert_eq!(layers[0].kind, LayerKind::StruckPlate);
        for w in layers.windows(2) {
            assert!((w[1].s0 - w[0].s1).abs() < 1e-4, "gap between {:?} and {:?}", w[0], w[1]);
            assert!(w[1].s1 >= w[1].s0);
        }
        let names: Vec<_> = layers.iter().filter_map(|l| l.kind.component()).map(|c| COMPONENT_SPECS[c].id).collect();
        assert!(names.contains(&"steering_unit") || names.contains(&"gearbox"), "{names:?}");
        assert!(names.contains(&"engine_block"), "a line down the hull axis reaches the engine: {names:?}");
        assert!(matches!(layers.last().unwrap().kind, LayerKind::Armour { .. }));
    }

    #[test]
    fn speed_never_rises_and_energy_closes() {
        let set = components::tiger_components();
        let line = front_line(100.0);
        let layers = build_layers(&line, set, &state());
        for shell in [&ballistics::PZGR_40, &ballistics::BR_354P, &ballistics::BR_240, &ballistics::BR_412] {
            let ev = ballistics::resolve(shell, 50.0, tiger_plate("front lower", 100.0), 0.0, 7);
            let r = run(&ev, &layers, 1.2);
            let mut v = ev.striking_v;
            for c in &r.crossings {
                assert!(c.v_out <= c.v_in + 1e-3 && c.v_in <= v + 1e-3, "{}: {}", shell.name, r.summary());
                v = c.v_out;
            }
            let end = 0.5 * shell.mass_kg * r.crossings.last().unwrap().v_out.powi(2);
            assert!((r.energy_in_j - r.energy_removed_j - end).abs() <= 1e-3 * r.energy_in_j.max(1.0));
        }
    }

    #[test]
    fn front_lower_plate_then_the_drive_train_is_layered_not_yes_no() {
        let set = components::tiger_components();
        let line = front_line(60.0);
        let layers = build_layers(&line, set, &state());
        // A strong solid shot goes through the 60 mm plate and stops somewhere in the drive train
        // or engine; a weaker one at long range stops earlier along the same line.
        let strong = run(&ballistics::resolve(&ballistics::PZGR_40, 100.0, tiger_plate("front lower", 60.0), 0.0, 3), &layers, 1.2);
        let weak = run(&ballistics::resolve(&ballistics::BR_240, 400.0, tiger_plate("front lower", 60.0), 0.0, 3), &layers, 1.2);
        assert!(strong.crossings.len() > 2, "{}", strong.summary());
        assert!(strong.end_s() > weak.end_s(), "strong {} vs weak {}", strong.summary(), weak.summary());
        // The first component reached by the strong shot is struck; if it stopped in a casing the
        // component is struck but not breached.
        for hit in &strong.hits {
            if hit.stopped && !hit.breached {
                assert!(strong.lodged.is_some());
            }
        }
    }

    #[test]
    fn filled_rounds_burst_behind_the_plate_and_solid_shot_does_not() {
        let set = components::tiger_components();
        let layers = build_layers(&front_line(60.0), set, &state());
        let aphe = run(&ballistics::resolve(&ballistics::BR_365, 100.0, tiger_plate("front lower", 60.0), 0.0, 1), &layers, 1.2);
        assert!(aphe.burst, "{}", aphe.summary());
        assert_eq!(aphe.crossings.len(), 1);
        let solid = run(&ballistics::resolve(&ballistics::PZGR_40, 100.0, tiger_plate("front lower", 60.0), 0.0, 1), &layers, 1.2);
        assert!(!solid.burst);
    }

    #[test]
    fn a_stopped_round_lodges_only_with_its_nose_buried() {
        let layers = [Layer { kind: LayerKind::StruckPlate, s0: 0.0, s1: 0.1, cos_obliquity: 1.0, normal: Vec3::Z }];
        // 76 mm AP against the 100 mm front at 500 m: stops; depth = spent capability.
        let ev = ballistics::resolve(&ballistics::BR_350B, 500.0, tiger_plate("front", 100.0), 0.0, 1);
        assert_eq!(ev.terminal, Terminal::Bounce);
        let r = run(&ev, &layers, 1.2);
        let Fate::Stopped { depth_m } = r.crossings[0].fate else { panic!() };
        assert!((depth_m - 0.1 * ev.pen_mm / ev.effective_mm).abs() < 1e-5);
        let nose = super::super::lodged::nose_length_calibres(ShellKind::Ap) * 0.0762;
        assert_eq!(r.lodged.is_some(), depth_m > nose);
        // A 45 mm round at long range barely scratches 100 mm: no lodging.
        let far = run(&ballistics::resolve(&ballistics::BR_240, 1500.0, tiger_plate("front", 100.0), 0.0, 1), &layers, 1.2);
        assert!(far.lodged.is_none() || far.crossings[0].pen_mm / far.crossings[0].effective_mm > 0.25);
    }

    #[test]
    fn fuel_level_splits_a_tank_into_liquid_and_vapour() {
        let set = components::tiger_components();
        let tank = set.envelopes.iter().find(|e| COMPONENT_SPECS[e.spec].id == "fuel_tank_left").unwrap();
        let empty = liquid_level(tank, Vec3::Y, 0.0);
        let half = liquid_level(tank, Vec3::Y, 0.5);
        let full = liquid_level(tank, Vec3::Y, 1.0);
        assert!(empty < half && half < full);
        // A horizontal line through the tank above the half level meets vapour, one below meets fuel.
        let (lo, hi) = tank.bounds();
        let mid = (lo + hi) * 0.5;
        let mk = |y: f32| ShotLine { entry: Vec3::new(hi.x + 0.3, y, mid.z), dir: Vec3::NEG_X, plate_path_m: 0.08,
            far_wall: None, max_s: (hi.x - lo.x) + 0.6 };
        let mut st = state();
        st.fuel_fill = 0.5;
        let kinds = |y: f32| build_layers(&mk(y), set, &st).into_iter().map(|l| l.kind).collect::<Vec<_>>();
        let low = kinds(half - 0.08);
        let high = kinds(half + 0.08);
        assert!(low.iter().any(|k| matches!(k, LayerKind::Liquid { .. })), "{low:?}");
        assert!(high.iter().all(|k| !matches!(k, LayerKind::Liquid { .. })), "{high:?}");
    }
}

#[cfg(test)]
mod print_cases {
    use super::*;
    use crate::ballistics::{self, ArmorMaterial, Plate};
    #[test]
    #[ignore]
    fn print_front_line_chains() {
        let set = components::tiger_components();
        let st = VehicleState { articulation: components::Articulation::rest(Vec3::new(0.0, 0.524, 0.019)),
            fuel_fill: 0.8, gravity_hull: Vec3::NEG_Y, live_rounds: &[] };
        for (mm, y) in [(60.0, 1.06), (100.0, 1.6)] {
            let line = ShotLine { entry: Vec3::new(-0.04, y, 3.55), dir: Vec3::NEG_Z, plate_path_m: mm * 0.001,
                far_wall: Some(FarWall { s_inner: 6.9, plate_mm: 80.0, material: MaterialId::RolledArmour, cos_obliquity: 1.0, normal: Vec3::NEG_Z }),
                max_s: 7.2 };
            let layers = build_layers(&line, set, &st);
            println!("--- plate {mm} mm at y {y}: {} layers", layers.len());
            for l in &layers { println!("   {:?} {:.3}..{:.3} cos {:.2}", l.kind, l.s0, l.s1, l.cos_obliquity); }
            for (shell, range) in [(&ballistics::BR_240, 100.0), (&ballistics::BR_240, 500.0), (&ballistics::BR_354P, 100.0),
                (&ballistics::PZGR_40, 100.0), (&ballistics::BR_412, 500.0), (&ballistics::BR_471, 500.0)] {
                let plate = Plate { name: "test", thickness_mm: mm, slope_deg: 0.0, material: ArmorMaterial::Rha };
                let ev = ballistics::resolve(shell, range, plate, 0.0, 3);
                let r = run(&ev, &layers, 1.2);
                println!("{} @{range} m: {:?} pen {:.0} eff {:.0} -> {}", shell.name, ev.terminal, ev.pen_mm, ev.effective_mm, r.summary());
                println!("    hits: {:?} lodged {:?} end {:.2}", r.hits.iter().map(|h| (COMPONENT_SPECS[h.component].id, h.breached, h.stopped)).collect::<Vec<_>>(), r.lodged, r.end_s());
            }
        }
    }
}
