//! The voxel lattice plate solver: a local patch of the struck plate, one particle per cubic cell,
//! each with a family of its 18 nearest lattice neighbours. An ordinary state-based peridynamic
//! solid (the linear peridynamic solid of Silling et al. 2007): elastic bulk response from the
//! particle's dilatation, deviatoric response with von Mises plasticity by the return mapping of
//! Mitchell (2011), linear hardening, logarithmic rate dependence (lagged one step), linear thermal
//! softening, bond failure at a scattered critical plastic strain or a tensile spall dilatation,
//! and compressive contact kept between broken neighbours. It is a meshfree discretisation of
//! continuum mechanics, not a finite-element method. docs/FEA_DAMAGE_MODEL_20261102.md, "Stage 2".
//!
//! The projectile's motion is prescribed from the calibrated ballistic outcome (kinetic energy
//! removed uniformly along its path through the plate); the lattice computes how the plate yields,
//! tears, plugs, petals and heats around that motion and how much work it absorbed. It never
//! decides a perforation. Named approximations, each also in the ledger:
//! * Yield and the deviatoric modulus are calibrated on an interior particle's full family
//!   (exact for coordinate-plane shear and plane deviatoric strain); surface particles keep the
//!   same yield strain by scaling with their weighted volume.
//! * Plastic flow is volume-preserving; the bulk response is elastic up to the spall dilatation.
//! * Outer cells are clamped and a damping ramp absorbs outgoing waves: the rest of the plate.
//! * Broken neighbours resist compression only between original neighbours; distant material that
//!   folds onto itself does not collide.
//! * The projectile is rigid; its nose is an ellipsoid of the kind's nose length.
//! * Material finer than one cell is not resolved.

use glam::Vec3;
use serde::Serialize;

use super::materials::{self, Material, MaterialId, AMBIENT_K, TAYLOR_QUINNEY};

/// Particle cap, the same in every preset (presets never change a damage outcome).
pub const MAX_PARTICLES: usize = 12_000;
pub const MIN_CELLS_THROUGH: usize = 4;
pub const TARGET_CELLS_THROUGH: f32 = 8.0;
pub const TARGET_CELLS_ACROSS_CALIBRE: f32 = 10.0;
/// Lateral half-extent of the patch around the strike, calibres (before the particle cap).
pub const LATERAL_CALIBRES: f32 = 2.5;
/// Lateral half-extent the cap may shrink the patch to, calibres.
pub const MIN_LATERAL_CALIBRES: f32 = 1.6;
/// Failure strain scatter (±, fraction): plate heterogeneity, hashed per bond.
pub const FAILURE_SCATTER: f32 = 0.08;
/// Yield scatter (±, fraction), hashed per particle.
pub const YIELD_SCATTER: f32 = 0.04;
/// Spall: cumulative tensile damage of the Tuler–Butcher form, D = ∫ ((σ − σ0)/σ0)² dt / τ, a bond
/// failing when either end reaches 1 (scattered with the bond's failure strain). Tension must last
/// to open a spall: a short pulse from a small projectile does not, a long one does. σ0 and τ are
/// named estimates within the range reported for armour steels (threshold ~2 GPa, microsecond
/// scale).
pub const SPALL_THRESHOLD_PA: f32 = 2.0e9;
pub const SPALL_DAMAGE_TIME_S: f32 = 1.0e-6;
/// A particle whose intact family's weighted volume falls below this (over h⁵) carries no
/// state-based stress, only compressive contact: it is all but detached (named).
pub const MIN_FAMILY: f32 = 3.0;
/// Poisson's ratio of steel; the state-based solid takes it directly.
pub const POISSON: f32 = 0.29;
/// Projectile contact stiffness in units of E·h.
pub const CONTACT_STIFFNESS: f32 = 2.0;
/// Kinetic friction between the projectile and the bore (named estimate for steel sliding at
/// high speed).
pub const FRICTION_KINETIC: f32 = 0.1;
/// Cells of damping ramp inside the clamped boundary.
pub const DAMPING_CELLS: usize = 3;
/// Time step as a fraction of a cell's P-wave transit.
pub const DT_CFL: f32 = 0.3;
/// Relaxation after the projectile's motion ends, in lateral wave transits.
pub const RELAX_TRANSITS: f32 = 2.0;

/// The 18-neighbourhood: slots 0..9 are the "owned" half (each bond is stored once, on the
/// particle it starts from), slots 9..18 the opposite directions.
const OFFSETS: [[i32; 3]; 9] = [
    [1, 0, 0], [0, 1, 0], [0, 0, 1],
    [1, 1, 0], [1, -1, 0], [1, 0, 1], [1, 0, -1], [0, 1, 1], [0, 1, -1],
];
const BONDS: usize = 9;
const SLOTS: usize = 18;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ProjectileShape {
    pub mass_kg: f32,
    /// Radius of the body that goes into the plate (a sub-calibre core's for APCR), m.
    pub radius_m: f32,
    /// Ellipsoidal nose length, m.
    pub nose_m: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Motion {
    /// Through the plate, leaving at `v_out`.
    Perforate { v_out: f32 },
    /// Stopped with its tip this far along its path into the plate.
    Stop { depth_m: f32 },
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PlateImpact {
    pub material: MaterialId,
    pub thickness_m: f32,
    /// Angle between the shot and the plate normal, radians.
    pub obliquity_rad: f32,
    pub projectile: ProjectileShape,
    pub v_in: f32,
    pub motion: Motion,
    /// Deterministic hash of the impact state: the heterogeneity field.
    pub seed: u32,
}

/// Energy bookkeeping, J. `projectile_work` is the work the prescribed projectile did on the plate.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Ledger {
    pub projectile_work: f64,
    pub kinetic: f64,
    pub elastic: f64,
    pub contact: f64,
    pub plastic: f64,
    pub friction: f64,
    pub damping: f64,
    /// Elastic energy released by broken bonds (the bonds' own stored energy, and any bulk energy
    /// a damaged particle loses).
    pub fracture: f64,
}

impl Ledger {
    pub fn accounted(&self) -> f64 {
        self.kinetic + self.elastic + self.contact + self.plastic + self.friction + self.damping + self.fracture
    }
    /// Relative closure error of the energy balance.
    pub fn closure(&self) -> f64 {
        (self.projectile_work - self.accounted()).abs() / self.projectile_work.abs().max(1e-9)
    }
}

#[inline]
fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}
#[inline]
fn hash_signed(seed: u32, a: u32, b: u32) -> f32 {
    let h = hash32(seed ^ hash32(a.wrapping_mul(0x9e37_79b9) ^ hash32(b.wrapping_add(0x632b_e5ab))));
    (h >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
}

/// The plate patch and its state.
pub struct Lattice {
    pub impact: PlateImpact,
    pub h: f32,
    pub dims: [usize; 3],
    /// Plate-frame position of cell (0,0,0)'s centre. Plate frame: x along the shot's in-plane
    /// direction, y across it, z into the plate from the struck face (z = 0).
    pub origin: Vec3,
    pub n: usize,
    /// Displacements from the rest lattice (better precision than absolute positions for small
    /// strains).
    pub ux: Vec<f32>,
    pub uy: Vec<f32>,
    pub uz: Vec<f32>,
    pub vx: Vec<f32>,
    pub vy: Vec<f32>,
    pub vz: Vec<f32>,
    fx: Vec<f32>,
    fy: Vec<f32>,
    fz: Vec<f32>,
    /// Contact force of the projectile on each particle at the previous step (trapezoid work).
    contact_prev: Vec<[f32; 3]>,
    pub temp: Vec<f32>,
    /// Equivalent plastic strain and its rate (lagged one step).
    pub eps_p: Vec<f32>,
    eps_rate: Vec<f32>,
    pub theta: Vec<f32>,
    /// Volumetric strain taken up by cracks (damage dilatation): the bulk response is elastic in
    /// θ − θ_d, so breaking a bond never raises a particle's stored bulk energy.
    theta_d: Vec<f32>,
    /// Dilatation at the end of the previous step.
    theta_prev: Vec<f32>,
    /// Accumulated spall damage (Tuler–Butcher form, failure at 1).
    pub spall_damage: Vec<f32>,
    /// Per particle slot: 0 no neighbour, 1 intact, 2 broken (both ends of a bond agree).
    slot_st: Vec<u8>,
    /// Per particle slot: unit direction to the neighbour now.
    mdir: Vec<[f32; 3]>,
    threads: usize,
    yield_scale: Vec<f32>,
    /// Weighted volume of each particle's reference family over h⁵ (30 for a full interior family).
    m_ref: Vec<f32>,
    /// The same over the intact family now (bonds break; the state stays an energy gradient).
    m_cur: Vec<f32>,
    /// Bit s set when slot s has a neighbour in the grid.
    valid: Vec<u32>,
    pub fixed: Vec<bool>,
    damping: Vec<f32>,
    /// Per owned half-bond: 0 no neighbour, 1 intact, 2 broken.
    pub bond_state: Vec<u8>,
    bond_fail: Vec<f32>,
    /// Per particle slot: current extension over h, deviatoric plastic extension over h, and the
    /// scalar force state in cell-normalised units (pair force = h² × (t_ij + t_ji), Pa·m²).
    ext: Vec<f32>,
    e_dp: Vec<f32>,
    t_state: Vec<f32>,
    rest: [f32; SLOTS],
    /// Rest bond vectors over h, and their lengths over h.
    xi: [Vec3; SLOTS],
    xi_len: [f32; SLOTS],
    delta: [isize; SLOTS],
    bulk: f32,
    shear: f32,
    /// Pair stiffness of the compressive contact between broken neighbours, N per unit stretch.
    k_pair: f32,
    k_contact: f32,
    pub mass: f32,
    vol: f32,
    pub dt: f32,
    pub t: f32,
    pub step: u64,
    pub t_end: f32,
    /// Projectile tip distance along its path from the struck face, and its speed.
    pub tip_s: f32,
    pub v: f32,
    /// Unit path direction in the plate frame.
    pub dir: Vec3,
    pub ledger: Ledger,
    pub done: bool,
    material: &'static Material,
    /// Kinetic energy the calibrated law removed in this plate.
    pub calibrated_loss_j: f64,
    path_resist_m: f32,
    full_m: f32,
    /// Test-only switches for isolating ledger terms.
    #[cfg(test)]
    pub(crate) debug_elastic: bool,
    #[cfg(test)]
    pub(crate) debug_no_fracture: bool,
    #[cfg(test)]
    pub(crate) debug_rate_free: bool,
}

#[inline]
fn slot_offset(s: usize) -> [i32; 3] {
    let o = OFFSETS[s % BONDS];
    if s < BONDS { o } else { [-o[0], -o[1], -o[2]] }
}

impl Lattice {
    pub fn new(impact: PlateImpact) -> Self {
        let material = impact.material.get();
        let t = impact.thickness_m.max(0.002);
        let a = impact.projectile.radius_m.max(0.0005);
        let d = 2.0 * a;
        let cos = impact.obliquity_rad.cos().clamp(0.1, 1.0);
        let tan = impact.obliquity_rad.tan().clamp(0.0, 4.0);
        // Depth simulated: the whole plate, or the stop depth plus two calibres over a rigid
        // backing when the round cannot reach the back face.
        let depth = match impact.motion {
            Motion::Perforate { .. } => t,
            Motion::Stop { depth_m } => (depth_m * cos + 2.0 * d).min(t),
        };
        let h0 = (t / TARGET_CELLS_THROUGH).min(d / TARGET_CELLS_ACROSS_CALIBRE).max(0.0008);
        let mut nz = ((depth / h0).round() as usize).max(MIN_CELLS_THROUGH);
        let mut h = depth / nz as f32;
        let mut lateral = (LATERAL_CALIBRES * d).max(a + 4.0 * h);
        let exit_shift = depth * tan;
        let count = |h: f32, lateral: f32, nz: usize| {
            let nx = ((2.0 * lateral + exit_shift) / h).ceil() as usize;
            let ny = ((2.0 * lateral) / h).ceil() as usize;
            (nx, ny, nx * ny * nz)
        };
        // Coarsen to the cap: first the lateral extent, then the cells.
        while count(h, lateral, nz).2 > MAX_PARTICLES {
            if lateral > MIN_LATERAL_CALIBRES * d + h {
                lateral = (lateral * 0.93).max(MIN_LATERAL_CALIBRES * d);
            } else if nz > MIN_CELLS_THROUGH {
                nz -= 1;
                h = depth / nz as f32;
            } else {
                h *= 1.06;
            }
        }
        let (nx, ny, _) = count(h, lateral, nz);
        let n = nx * ny * nz;
        let origin = Vec3::new(-lateral + 0.5 * h, -lateral + 0.5 * h, 0.5 * h);
        let e = material.youngs;
        let bulk = e / (3.0 * (1.0 - 2.0 * POISSON));
        let shear = e / (2.0 * (1.0 + POISSON));
        let wave_p = ((bulk + 4.0 * shear / 3.0) / material.density).sqrt();
        let mut fixed = vec![false; n];
        let mut damping = vec![0.0; n];
        let truncated = matches!(impact.motion, Motion::Stop { .. }) && depth < t - 0.5 * h;
        let rest: [f32; SLOTS] = std::array::from_fn(|s| {
            let o = slot_offset(s);
            ((o[0] * o[0] + o[1] * o[1] + o[2] * o[2]) as f32).sqrt() * h
        });
        let delta: [isize; SLOTS] = std::array::from_fn(|s| {
            let o = slot_offset(s);
            o[0] as isize + o[1] as isize * nx as isize + o[2] as isize * (nx * ny) as isize
        });
        let vol = h * h * h;
        let mut valid = vec![0u32; n];
        let mut m_ref = vec![0.0f32; n];
        let mut yield_scale = vec![1.0f32; n];
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    let idx = (k * ny + j) * nx + i;
                    let edge = i.min(nx - 1 - i).min(j).min(ny - 1 - j);
                    if edge == 0 || (truncated && k == nz - 1) {
                        fixed[idx] = true;
                    } else if edge <= DAMPING_CELLS {
                        let ramp = 1.0 - (edge - 1) as f32 / DAMPING_CELLS as f32;
                        damping[idx] = ramp * ramp * 0.5 * wave_p / h;
                    }
                    let mut mask = 0u32;
                    let mut m = 0.0f32;
                    for s in 0..SLOTS {
                        let o = slot_offset(s);
                        let (ii, jj, kk) = (i as i32 + o[0], j as i32 + o[1], k as i32 + o[2]);
                        if ii >= 0 && jj >= 0 && kk >= 0 && ii < nx as i32 && jj < ny as i32 && kk < nz as i32 {
                            mask |= 1 << s;
                            let l = rest[s] / h;
                            m += l * l;
                        }
                    }
                    valid[idx] = mask;
                    m_ref[idx] = m;
                    yield_scale[idx] = 1.0 + YIELD_SCATTER * hash_signed(impact.seed, idx as u32, 2);
                }
            }
        }
        let full_m = (0..SLOTS).map(|s| (rest[s] / h).powi(2)).sum::<f32>();
        let xi: [Vec3; SLOTS] = std::array::from_fn(|s| {
            let o = slot_offset(s);
            Vec3::new(o[0] as f32, o[1] as f32, o[2] as f32)
        });
        let xi_len: [f32; SLOTS] = std::array::from_fn(|s| xi[s].length());
        let mut bond_state = vec![0u8; n * BONDS];
        let mut bond_fail = vec![0.0f32; n * BONDS];
        for p in 0..n {
            for b in 0..BONDS {
                if valid[p] >> b & 1 == 1 {
                    bond_state[p * BONDS + b] = 1;
                    bond_fail[p * BONDS + b] = material.failure_strain
                        * (1.0 + FAILURE_SCATTER * hash_signed(impact.seed, (p * BONDS + b) as u32, 1));
                }
            }
        }
        let mut slot_st = vec![0u8; n * SLOTS];
        for p in 0..n {
            for sl in 0..SLOTS {
                if valid[p] >> sl & 1 == 1 {
                    slot_st[p * SLOTS + sl] = 1;
                }
            }
        }
        // Bond-based contact stiffness between broken neighbours (pair force per unit stretch, N):
        // c V² with c = 18K / Σ|ξ|V.
        let sum_len: f32 = (0..SLOTS).map(|s| rest[s]).sum();
        let k_pair = 18.0 * bulk * vol / sum_len;
        let k_contact = CONTACT_STIFFNESS * e * h;
        let mass = material.density * vol;
        let dt = (DT_CFL * h / wave_p).min(0.5 * (mass / k_contact).sqrt());
        let dir = Vec3::new(impact.obliquity_rad.sin(), 0.0, impact.obliquity_rad.cos()).normalize();
        let m = impact.projectile.mass_kg as f64;
        let v_in = impact.v_in.max(0.0);
        let (path_resist, calibrated_loss, path_end) = match impact.motion {
            Motion::Perforate { v_out } => {
                let p = t / cos + impact.projectile.nose_m;
                (p, 0.5 * m * (v_in as f64 * v_in as f64 - v_out.min(v_in) as f64 * v_out.min(v_in) as f64),
                    p + 0.5 * d)
            }
            Motion::Stop { depth_m } => (depth_m.max(1e-4), 0.5 * m * v_in as f64 * v_in as f64, depth_m.max(1e-4)),
        };
        let mut t_motion = 0.0f32;
        {
            let steps = 2000;
            let ds = path_end / steps as f32;
            for k in 0..steps {
                let v = prescribed_speed(&impact, path_resist, (k as f32 + 0.5) * ds).max(1.0);
                t_motion += ds / v;
            }
        }
        // At oblique incidence the side of the nose meets the plate before its tip: start the tip
        // back along the path until no part of the projectile (with the particles' contact radius)
        // overlaps the plate, so the solve never begins with a compressed contact.
        let (sin_o, cos_o) = (impact.obliquity_rad.sin(), impact.obliquity_rad.cos());
        let mut overlap = 0.0f32;
        for k in 0..=200 {
            let b = (impact.projectile.nose_m + 2.0 * h) * k as f32 / 200.0;
            let nose = impact.projectile.nose_m.max(1e-4);
            let r = if b < nose {
                let f = 1.0 - (nose - b) / nose;
                impact.projectile.radius_m * (1.0 - (1.0 - f) * (1.0 - f)).max(0.0).sqrt()
            } else {
                impact.projectile.radius_m
            };
            overlap = overlap.max(-b * cos_o + (r + 0.5 * h) * sin_o);
        }
        let start_back = if cos_o > 0.05 { overlap.max(0.0) / cos_o } else { 0.0 };
        t_motion += start_back / v_in.max(1.0);
        let relax = (RELAX_TRANSITS * 2.0 * lateral / wave_p).max(30e-6);
        let t_end = t_motion + relax;
        Self {
            impact, h, dims: [nx, ny, nz], origin, n,
            ux: vec![0.0; n], uy: vec![0.0; n], uz: vec![0.0; n], vx: vec![0.0; n], vy: vec![0.0; n], vz: vec![0.0; n],
            fx: vec![0.0; n], fy: vec![0.0; n], fz: vec![0.0; n], contact_prev: vec![[0.0; 3]; n],
            temp: vec![AMBIENT_K; n], eps_p: vec![0.0; n], eps_rate: vec![0.0; n], theta: vec![0.0; n],
            theta_d: vec![0.0; n], theta_prev: vec![0.0; n], spall_damage: vec![0.0; n], slot_st, mdir: vec![[0.0; 3]; n * SLOTS],
            threads: default_threads(),
            yield_scale, m_cur: m_ref.clone(), m_ref, valid, fixed, damping, bond_state, bond_fail,
            ext: vec![0.0; n * SLOTS], e_dp: vec![0.0; n * SLOTS], t_state: vec![0.0; n * SLOTS],
            rest, xi, xi_len, delta, bulk, shear, k_pair, k_contact, mass, vol, dt, t: 0.0, step: 0, t_end,
            tip_s: -start_back, v: v_in, dir, ledger: Ledger::default(), done: false, material,
            calibrated_loss_j: calibrated_loss, path_resist_m: path_resist,
            full_m,
            #[cfg(test)]
            debug_elastic: false,
            #[cfg(test)]
            debug_no_fracture: false,
            #[cfg(test)]
            debug_rate_free: false,
        }
    }

    /// Particle index of cell (i, j, k).
    pub fn index(&self, i: usize, j: usize, k: usize) -> usize {
        (k * self.dims[1] + j) * self.dims[0] + i
    }

    /// Rest position of particle `p` in the plate frame.
    pub fn rest_position(&self, p: usize) -> Vec3 {
        let i = p % self.dims[0];
        let j = (p / self.dims[0]) % self.dims[1];
        let k = p / (self.dims[0] * self.dims[1]);
        self.origin + Vec3::new(i as f32, j as f32, k as f32) * self.h
    }

    pub fn position(&self, p: usize) -> Vec3 {
        self.rest_position(p) + Vec3::new(self.ux[p], self.uy[p], self.uz[p])
    }

    /// Extension over h of the bond from `p` along slot `s` to `q`, in a cancellation-free form:
    /// e = (2 ξ·Δu + |Δu|²) / (|ξ + Δu| + |ξ|), all over h.
    #[inline]
    fn extension(&self, p: usize, q: usize, s: usize) -> (f32, Vec3) {
        let inv_h = 1.0 / self.h;
        let du = Vec3::new(self.ux[q] - self.ux[p], self.uy[q] - self.uy[p], self.uz[q] - self.uz[p]) * inv_h;
        let xi = self.xi[s];
        let y = xi + du;
        let len = y.length().max(1e-9);
        let e = (2.0 * xi.dot(du) + du.length_squared()) / (len + self.xi_len[s]);
        (e, y / len)
    }

    pub fn velocity(&self, p: usize) -> Vec3 {
        Vec3::new(self.vx[p], self.vy[p], self.vz[p])
    }

    /// Planned number of steps (the motion plus relaxation).
    pub fn planned_steps(&self) -> u64 {
        (self.t_end / self.dt).ceil() as u64
    }

    /// State of the bond in particle `p`'s slot `s`: 0 none, 1 intact, 2 broken.
    #[inline]
    fn slot_state(&self, p: usize, s: usize) -> u8 {
        self.slot_st[p * SLOTS + s]
    }

    #[cfg(test)]
    fn debug_elastic_flag(&self) -> bool { self.debug_elastic }
    #[cfg(not(test))]
    fn debug_elastic_flag(&self) -> bool { false }
    #[cfg(test)]
    fn debug_no_fracture_flag(&self) -> bool { self.debug_no_fracture }
    #[cfg(not(test))]
    fn debug_no_fracture_flag(&self) -> bool { false }
    #[cfg(test)]
    fn debug_rate_free_flag(&self) -> bool { self.debug_rate_free }
    #[cfg(not(test))]
    fn debug_rate_free_flag(&self) -> bool { false }

    /// Advance at most `max_steps`. Returns true when the solve is complete. A budget-sliced solve
    /// reproduces an unsliced one bit for bit, and so does any thread count: every particle's
    /// arithmetic runs in a fixed order and the passes only exchange data across barriers.
    pub fn advance(&mut self, max_steps: u64) -> bool {
        if self.done || max_steps == 0 {
            return self.done;
        }
        let threads = self.threads.clamp(1, (self.n / 1024).max(1));
        let view = View::of(self);
        let consts = Consts::of(self);
        let barrier = SpinBarrier::new(threads);
        let broken: Vec<std::sync::Mutex<Vec<(u32, u8)>>> = (0..threads).map(|_| std::sync::Mutex::new(Vec::new())).collect();
        let partial: Vec<std::sync::Mutex<[f64; 4]>> = (0..threads).map(|_| std::sync::Mutex::new([0.0; 4])).collect();
        let chunk = self.n.div_ceil(threads);
        let start_step = self.step;
        let t0 = self.t;
        let tip0 = self.tip_s;
        let need_initial = self.step == 0;
        let mut out = (self.step, self.t, self.tip_s, self.v, 0.0f64, [0.0f64; 4], self.done);
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for w in 0..threads {
                let lo = (w * chunk).min(self.n);
                let hi = ((w + 1) * chunk).min(self.n);
                let (view, consts, barrier, broken, partial) = (&view, &consts, &barrier, &broken, &partial);
                handles.push(scope.spawn(move || {
                    // SAFETY: every pass writes only particles in [lo, hi) (or, for the serial pass,
                    // runs on worker 0 alone between barriers) and reads other particles' data only
                    // after the barrier that ends the pass writing it.
                    unsafe {
                        worker(view, consts, barrier, broken, partial, w, threads, lo, hi,
                            start_step, t0, tip0, max_steps, need_initial)
                    }
                }));
            }
            for (w, h) in handles.into_iter().enumerate() {
                let r = h.join().expect("lattice worker");
                if w == 0 {
                    out = r;
                }
            }
        });
        let (step, t, tip_s, v, fracture, sums, done) = out;
        self.step = step;
        self.t = t;
        self.tip_s = tip_s;
        self.v = v;
        self.ledger.fracture += fracture;
        self.ledger.plastic += sums[0];
        self.ledger.projectile_work += sums[1];
        self.ledger.friction += sums[2];
        self.ledger.damping += sums[3];
        if done && !self.done {
            self.done = true;
            self.finish_ledger();
        }
        self.done
    }

    pub fn solve(&mut self) {
        self.advance(u64::MAX);
    }

    /// Evaluate forces at the present state without moving anything (tests and diagnostics).
    pub fn compute_forces(&mut self) {
        let view = View::of(self);
        let consts = Consts::of(self);
        let mut broken = Vec::new();
        // SAFETY: single thread, whole range.
        unsafe {
            pass_extensions(&view, &consts, 0, self.n, &mut broken, self.tip_s);
            let fracture = pass_breaks(&view, &consts, &broken);
            self.ledger.fracture += fracture;
            let _ = pass_states(&view, &consts, 0, self.n);
            let _ = pass_gather(&view, &consts, 0, self.n, self.tip_s, self.v, false);
        }
    }

    /// Threads the solve uses (the result does not depend on it).
    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads.max(1);
    }

    fn finish_ledger(&mut self) {
        let mut kinetic = 0.0f64;
        for p in 0..self.n {
            let v2 = self.vx[p] * self.vx[p] + self.vy[p] * self.vy[p] + self.vz[p] * self.vz[p];
            kinetic += 0.5 * self.mass as f64 * v2 as f64;
        }
        // Elastic energy: bulk plus deviatoric of every particle (energy density in Pa times the
        // cell volume), plus broken-neighbour compression.
        let mut elastic = 0.0f64;
        for p in 0..self.n {
            let m_p = self.m_ref[p].max(1e-6);
            let a = 15.0 * self.shear / m_p;
            let mut dev = 0.0f32;
            let mut contact = 0.0f32;
            for s in 0..SLOTS {
                let si = p * SLOTS + s;
                match self.slot_state(p, s) {
                    1 => {
                        let ed = self.ext[si] - self.theta[p] * self.xi_len[s] / 3.0 - self.e_dp[si];
                        dev += ed * ed;
                    }
                    2 => {
                        let stretch = self.ext[si] / self.xi_len[s];
                        if stretch < 0.0 {
                            // Each pair is visited from both ends: half each.
                            contact += 0.25 * self.k_pair * self.rest[s] * stretch * stretch;
                        }
                    }
                    _ => {}
                }
            }
            let isolated = (0..SLOTS).all(|s| self.slot_state(p, s) != 1);
            let th = if isolated { 0.0 } else { self.theta[p] - self.theta_d[p] };
            let w = 0.5 * self.bulk * th * th + 0.5 * a * dev;
            elastic += (w * self.vol) as f64 + contact as f64;
        }
        let mut contact_energy = 0.0f64;
        for p in 0..self.n {
            let fc = Vec3::from_array(self.contact_prev[p]).length();
            if fc > 0.0 {
                contact_energy += (0.5 * fc * fc / self.k_contact) as f64;
            }
        }
        self.ledger.kinetic = kinetic;
        self.ledger.elastic = elastic;
        self.ledger.contact = contact_energy;
    }

    /// The current energy ledger (kinetic and elastic evaluated now).
    pub fn ledger_now(&mut self) -> Ledger {
        self.finish_ledger();
        self.ledger
    }

    /// Total mass of the patch (exact: particles are never created or destroyed).
    pub fn total_mass(&self) -> f32 {
        self.mass * self.n as f32
    }
}

/// The prescribed speed at tip path distance `s`: kinetic energy removed uniformly along the
/// resisting path (`path_resist` m), to the calibrated exit speed or to rest.
pub fn prescribed_speed(impact: &PlateImpact, path_resist: f32, s: f32) -> f32 {
    let v_in = impact.v_in.max(0.0);
    match impact.motion {
        Motion::Perforate { v_out } => {
            let v_out = v_out.clamp(0.0, v_in);
            let f = (s / path_resist).clamp(0.0, 1.0);
            (v_in * v_in - (v_in * v_in - v_out * v_out) * f).max(0.0).sqrt()
        }
        Motion::Stop { depth_m } => {
            let f = (s / depth_m.max(1e-6)).clamp(0.0, 1.0);
            (v_in * v_in * (1.0 - f)).max(0.0).sqrt()
        }
    }
}


// ── The parallel step ──────────────────────────────────────────────────────────────────────────────

/// Raw views of the lattice's arrays for the worker threads. Writes are partitioned by particle
/// range within a pass; reads of other ranges happen only after a barrier.
#[derive(Clone, Copy)]
struct View {
    n: usize,
    ux: *mut f32, uy: *mut f32, uz: *mut f32,
    vx: *mut f32, vy: *mut f32, vz: *mut f32,
    fx: *mut f32, fy: *mut f32, fz: *mut f32,
    contact_prev: *mut [f32; 3],
    temp: *mut f32,
    eps_p: *mut f32,
    eps_rate: *mut f32,
    theta: *mut f32,
    theta_prev: *mut f32,
    spall_damage: *mut f32,
    theta_d: *mut f32,
    m_cur: *mut f32,
    m_ref: *const f32,
    yield_scale: *const f32,
    valid: *const u32,
    fixed: *const bool,
    damping: *const f32,
    bond_state: *mut u8,
    bond_fail: *const f32,
    slot_st: *mut u8,
    ext: *mut f32,
    mdir: *mut [f32; 3],
    e_dp: *mut f32,
    t_state: *mut f32,
}
unsafe impl Send for View {}
unsafe impl Sync for View {}

impl View {
    fn of(l: &mut Lattice) -> Self {
        View {
            n: l.n,
            ux: l.ux.as_mut_ptr(), uy: l.uy.as_mut_ptr(), uz: l.uz.as_mut_ptr(),
            vx: l.vx.as_mut_ptr(), vy: l.vy.as_mut_ptr(), vz: l.vz.as_mut_ptr(),
            fx: l.fx.as_mut_ptr(), fy: l.fy.as_mut_ptr(), fz: l.fz.as_mut_ptr(),
            contact_prev: l.contact_prev.as_mut_ptr(),
            temp: l.temp.as_mut_ptr(),
            eps_p: l.eps_p.as_mut_ptr(),
            eps_rate: l.eps_rate.as_mut_ptr(),
            theta: l.theta.as_mut_ptr(),
            theta_prev: l.theta_prev.as_mut_ptr(),
            spall_damage: l.spall_damage.as_mut_ptr(),
            theta_d: l.theta_d.as_mut_ptr(),
            m_cur: l.m_cur.as_mut_ptr(),
            m_ref: l.m_ref.as_ptr(),
            yield_scale: l.yield_scale.as_ptr(),
            valid: l.valid.as_ptr(),
            fixed: l.fixed.as_ptr(),
            damping: l.damping.as_ptr(),
            bond_state: l.bond_state.as_mut_ptr(),
            bond_fail: l.bond_fail.as_ptr(),
            slot_st: l.slot_st.as_mut_ptr(),
            ext: l.ext.as_mut_ptr(),
            mdir: l.mdir.as_mut_ptr(),
            e_dp: l.e_dp.as_mut_ptr(),
            t_state: l.t_state.as_mut_ptr(),
        }
    }
}

/// Everything constant during a solve.
struct Consts {
    impact: PlateImpact,
    material: &'static Material,
    dims: [usize; 3],
    origin: Vec3,
    h: f32,
    vol: f32,
    dt: f32,
    mass: f32,
    bulk: f32,
    shear: f32,
    k_pair: f32,
    k_contact: f32,
    full_m: f32,
    xi: [Vec3; SLOTS],
    xi_len: [f32; SLOTS],
    rest: [f32; SLOTS],
    delta: [isize; SLOTS],
    dir: Vec3,
    path_resist: f32,
    t_end: f32,
    elastic_only: bool,
    no_fracture: bool,
    rate_free: bool,
}

impl Consts {
    fn of(l: &Lattice) -> Self {
        Consts {
            impact: l.impact, material: l.material, dims: l.dims, origin: l.origin, h: l.h, vol: l.vol,
            dt: l.dt, mass: l.mass, bulk: l.bulk, shear: l.shear, k_pair: l.k_pair, k_contact: l.k_contact,
            full_m: l.full_m, xi: l.xi, xi_len: l.xi_len, rest: l.rest,
            delta: l.delta, dir: l.dir, path_resist: l.path_resist_m, t_end: l.t_end,
            elastic_only: l.debug_elastic_flag(), no_fracture: l.debug_no_fracture_flag(),
            rate_free: l.debug_rate_free_flag(),
        }
    }
    #[inline]
    fn rest_position(&self, p: usize) -> Vec3 {
        let i = p % self.dims[0];
        let j = (p / self.dims[0]) % self.dims[1];
        let k = p / (self.dims[0] * self.dims[1]);
        self.origin + Vec3::new(i as f32, j as f32, k as f32) * self.h
    }
}

/// A sense-reversing spin barrier (the passes are microseconds long; a parking barrier would
/// cost more than the work). Spins briefly, then yields.
struct SpinBarrier {
    count: std::sync::atomic::AtomicUsize,
    generation: std::sync::atomic::AtomicUsize,
    n: usize,
}
impl SpinBarrier {
    fn new(n: usize) -> Self {
        Self { count: 0.into(), generation: 0.into(), n }
    }
    fn wait(&self) {
        use std::sync::atomic::Ordering::*;
        if self.n == 1 {
            return;
        }
        let gen = self.generation.load(Acquire);
        if self.count.fetch_add(1, AcqRel) + 1 == self.n {
            self.count.store(0, Relaxed);
            self.generation.fetch_add(1, Release);
        } else {
            let mut spins = 0u32;
            while self.generation.load(Acquire) == gen {
                spins += 1;
                if spins < 2000 { std::hint::spin_loop() } else { std::thread::yield_now() }
            }
        }
    }
}

#[inline]
unsafe fn position(v: &View, c: &Consts, p: usize) -> Vec3 {
    c.rest_position(p) + Vec3::new(*v.ux.add(p), *v.uy.add(p), *v.uz.add(p))
}

/// The prescribed projectile one step on, from the same law every worker evaluates identically.
#[inline]
fn advance_projectile(c: &Consts, tip_s: f32) -> (f32, f32) {
    let v_here = prescribed_speed(&c.impact, c.path_resist, tip_s);
    let mut tip = tip_s + v_here * c.dt;
    let mut v = v_here;
    if let Motion::Stop { depth_m } = c.impact.motion {
        if tip >= depth_m {
            tip = depth_m;
            v = 0.0;
        }
    }
    (tip, v)
}

#[allow(clippy::too_many_arguments)]
unsafe fn worker(v: &View, c: &Consts, barrier: &SpinBarrier, broken: &[std::sync::Mutex<Vec<(u32, u8)>>],
    partial: &[std::sync::Mutex<[f64; 4]>], w: usize, threads: usize, lo: usize, hi: usize,
    start_step: u64, t0: f32, tip0: f32, max_steps: u64, need_initial: bool)
    -> (u64, f32, f32, f32, f64, [f64; 4], bool) {
    let mut step = start_step;
    let mut t = t0;
    let mut tip = tip0;
    let mut speed = prescribed_speed(&c.impact, c.path_resist, tip0);
    let mut fracture = 0.0f64;
    let mut sums = [0.0f64; 4];
    let mut local_broken: Vec<(u32, u8)> = Vec::new();
    let mut taken = 0u64;
    let inv_m = 1.0 / c.mass;
    let dt = c.dt;
    let mut forces = |tip: f32, speed: f32, kick: bool, local_broken: &mut Vec<(u32, u8)>| {
        local_broken.clear();
        pass_extensions(v, c, lo, hi, local_broken, tip);
        *broken[w].lock().unwrap() = std::mem::take(local_broken);
        barrier.wait();
        if w == 0 {
            let mut all = Vec::new();
            for list in broken.iter().take(threads) {
                all.extend(list.lock().unwrap().iter().copied());
            }
            fracture += pass_breaks(v, c, &all);
        }
        barrier.wait();
        let plastic = pass_states(v, c, lo, hi);
        barrier.wait();
        let [work, fric, damp] = pass_gather(v, c, lo, hi, tip, speed, kick);
        barrier.wait();
        [plastic, work, fric, damp]
    };
    if need_initial {
        let s = forces(tip, speed, false, &mut local_broken);
        for k in 0..4 { sums[k] += s[k]; }
    }
    while taken < max_steps && t < c.t_end {
        // A: half kick and drift of this range; the projectile moves (identically in every worker).
        for p in lo..hi {
            *v.theta_prev.add(p) = *v.theta.add(p);
            if *v.fixed.add(p) {
                continue;
            }
            *v.vx.add(p) += 0.5 * dt * *v.fx.add(p) * inv_m;
            *v.vy.add(p) += 0.5 * dt * *v.fy.add(p) * inv_m;
            *v.vz.add(p) += 0.5 * dt * *v.fz.add(p) * inv_m;
            *v.ux.add(p) += dt * *v.vx.add(p);
            *v.uy.add(p) += dt * *v.vy.add(p);
            *v.uz.add(p) += dt * *v.vz.add(p);
        }
        let (ntip, nspeed) = advance_projectile(c, tip);
        tip = ntip;
        speed = nspeed;
        barrier.wait();
        let s = forces(tip, speed, true, &mut local_broken);
        for k in 0..4 { sums[k] += s[k]; }
        t += dt;
        step += 1;
        taken += 1;
    }
    // Every worker's partial sums, combined by worker 0 in worker order.
    *partial[w].lock().unwrap() = sums;
    barrier.wait();
    let mut total = [0.0f64; 4];
    if w == 0 {
        for slot in partial.iter().take(threads) {
            let s = *slot.lock().unwrap();
            for k in 0..4 { total[k] += s[k]; }
        }
    }
    (step, t, tip, speed, fracture, total, t >= c.t_end)
}

/// Pass 1+2: extensions and directions of every slot of this range, failure of owned bonds
/// (scattered critical plastic strain under tension, or the previous step's spall dilatation at
/// either end), and each particle's dilatation over its family as it stood (bonds broken now are
/// removed in `pass_breaks`).
unsafe fn pass_extensions(v: &View, c: &Consts, lo: usize, hi: usize, broken: &mut Vec<(u32, u8)>, _tip: f32) {
    let inv_h = 1.0 / c.h;
    for p in lo..hi {
        let valid = *v.valid.add(p);
        let mut acc = 0.0f32;
        let mut m = 0.0f32;
        let (pux, puy, puz) = (*v.ux.add(p), *v.uy.add(p), *v.uz.add(p));
        for s in 0..SLOTS {
            let si = p * SLOTS + s;
            if valid >> s & 1 == 0 {
                *v.ext.add(si) = 0.0;
                continue;
            }
            let q = (p as isize + c.delta[s]) as usize;
            let du = Vec3::new(*v.ux.add(q) - pux, *v.uy.add(q) - puy, *v.uz.add(q) - puz) * inv_h;
            let xi = c.xi[s];
            let y = xi + du;
            let len = y.length().max(1e-9);
            let e = (2.0 * xi.dot(du) + du.length_squared()) / (len + c.xi_len[s]);
            *v.ext.add(si) = e;
            *v.mdir.add(si) = (y / len).to_array();
            let st = *v.slot_st.add(si);
            if st == 1 {
                if s < BONDS && e > 0.0 && !c.no_fracture {
                    let bi = p * BONDS + s;
                    let fail = *v.bond_fail.add(bi);
                    let strained = 0.5 * (*v.eps_p.add(p) + *v.eps_p.add(q)) >= fail;
                    let damage = (*v.spall_damage.add(p)).max(*v.spall_damage.add(q));
                    let spalled = damage >= fail / c.material.failure_strain;
                    if strained || spalled {
                        *v.bond_state.add(bi) = 2;
                        broken.push((p as u32, s as u8));
                    }
                }
                acc += c.xi_len[s] * e;
                m += c.xi_len[s] * c.xi_len[s];
            }
        }
        *v.m_cur.add(p) = m;
        *v.theta.add(p) = 3.0 * acc / (*v.m_ref.add(p)).max(1e-6);
    }
}

/// Serial pass 2b: a bond broken this evaluation releases its own stored energy (the fracture
/// sink) and must not raise the energy of what remains: the dilatation change it causes is taken
/// into the remaining bonds' plastic extensions (their elastic deviatoric extensions stay as they
/// were) and, where it would compress further, into the damage dilatation.
unsafe fn pass_breaks(v: &View, c: &Consts, broken: &[(u32, u8)]) -> f64 {
    if broken.is_empty() {
        return 0.0;
    }
    let mut fracture = 0.0f64;
    let mut touched: Vec<(u32, f32)> = Vec::with_capacity(broken.len() * 2);
    for &(p, b) in broken {
        let p = p as usize;
        let b = b as usize;
        let q = (p as isize + c.delta[b]) as usize;
        for (x, slot) in [(p, b), (q, (b + BONDS) % SLOTS)] {
            let e = *v.ext.add(x * SLOTS + slot);
            touched.push((x as u32, -3.0 * c.xi_len[slot] * e / (*v.m_ref.add(x)).max(1e-6)));
        }
    }
    // The broken bonds' own stored deviatoric energy, from each end's family as it stood.
    let mut dtheta: std::collections::BTreeMap<u32, f32> = std::collections::BTreeMap::new();
    for &(x, d) in &touched {
        *dtheta.entry(x).or_insert(0.0) += d;
    }
    for &(p, b) in broken {
        let p = p as usize;
        let b = b as usize;
        let q = (p as isize + c.delta[b]) as usize;
        for (x, slot) in [(p, b), (q, (b + BONDS) % SLOTS)] {
            let a = 15.0 * c.shear / (*v.m_ref.add(x)).max(1e-6);
            let theta_old = *v.theta.add(x);
            let eps = *v.ext.add(x * SLOTS + slot) - theta_old * c.xi_len[slot] / 3.0 - *v.e_dp.add(x * SLOTS + slot);
            fracture += (0.5 * a * eps * eps * c.vol) as f64;
            *v.slot_st.add(x * SLOTS + slot) = 2;
        }
    }
    for (&x, &d) in &dtheta {
        let x = x as usize;
        // A particle that just lost its last bond holds no stress: its whole bulk energy is
        // released with it.
        let left = (0..SLOTS).filter(|&s| *v.slot_st.add(x * SLOTS + s) == 1).count();
        if left == 0 {
            let el = *v.theta.add(x) - *v.theta_d.add(x);
            fracture += (0.5 * c.bulk * el * el * c.vol) as f64;
            *v.theta.add(x) = 0.0;
            *v.theta_d.add(x) = 0.0;
            continue;
        }
        if d == 0.0 {
            continue;
        }
        for s in 0..SLOTS {
            if *v.slot_st.add(x * SLOTS + s) == 1 {
                *v.e_dp.add(x * SLOTS + s) -= d * c.xi_len[s] / 3.0;
            }
        }
        let theta_old = *v.theta.add(x);
        let theta_new = theta_old + d;
        *v.theta.add(x) = theta_new;
        let td = *v.theta_d.add(x);
        let (el_old, el_new) = (theta_old - td, theta_new - td);
        if el_new.abs() > el_old.abs() {
            *v.theta_d.add(x) = td + d;
        } else {
            fracture += (0.5 * c.bulk * (el_old * el_old - el_new * el_new) * c.vol) as f64;
        }
    }
    fracture
}

/// Pass 3: force states, the exact gradient of each particle's stored energy
///   W = ½ K (θ − θ_d)² + ½ a Σ ε_j²,  ε_j = e_j − θ ξ_j / 3 − e_dp_j,  a = 15 μ / m_ref,
/// over its intact family, with the deviatoric radial return of Mitchell (2011): when Σ (a ε)²
/// exceeds (k²/2)(m_full/m_ref) the elastic deviatoric extensions are scaled back to the yield
/// surface and the difference becomes plastic extension. Normalising by the reference family keeps
/// a damaged particle's stiffness bounded; the gradient's correction term keeps the force
/// conservative after bonds break. Returns the plastic work of the range (J).
unsafe fn pass_states(v: &View, c: &Consts, lo: usize, hi: usize) -> f64 {
    let mut plastic = 0.0f64;
    let mut eps = [0.0f32; SLOTS];
    let material = c.material;
    for p in lo..hi {
        let m_p = (*v.m_ref.add(p)).max(1e-6);
        let a = 15.0 * c.shear / m_p;
        let theta = *v.theta.add(p);
        let tension = c.bulk * (theta - *v.theta_d.add(p));
        if tension > SPALL_THRESHOLD_PA {
            let over = (tension - SPALL_THRESHOLD_PA) / SPALL_THRESHOLD_PA;
            *v.spall_damage.add(p) += over * over * c.dt / SPALL_DAMAGE_TIME_S;
        }
        let bulk_term = 3.0 * c.bulk * (theta - *v.theta_d.add(p)) / m_p;
        let rate = if c.rate_free { 0.0 } else { *v.eps_rate.add(p) };
        let sigma = materials::flow_stress(material, *v.eps_p.add(p), rate, *v.temp.add(p)) * *v.yield_scale.add(p);
        let k_shear = sigma / 3.0f32.sqrt();
        let limit2 = 0.5 * k_shear * k_shear * (c.full_m / m_p);
        let mut norm2 = 0.0f32;
        let mut any = false;
        for s in 0..SLOTS {
            let si = p * SLOTS + s;
            if *v.slot_st.add(si) != 1 {
                eps[s] = 0.0;
                continue;
            }
            any = true;
            let e = *v.ext.add(si) - theta * c.xi_len[s] / 3.0 - *v.e_dp.add(si);
            eps[s] = e;
            norm2 += (a * e) * (a * e);
        }
        if !any {
            for s in 0..SLOTS {
                *v.t_state.add(p * SLOTS + s) = 0.0;
            }
            *v.eps_rate.add(p) = 0.0;
            continue;
        }
        let yielding = !c.elastic_only && norm2 > limit2 && limit2 > 0.0;
        if yielding {
            let scale = (limit2 / norm2).sqrt();
            let mut w0 = 0.0f32;
            for s in 0..SLOTS {
                let si = p * SLOTS + s;
                if *v.slot_st.add(si) != 1 {
                    continue;
                }
                w0 += eps[s] * eps[s];
                *v.e_dp.add(si) += (1.0 - scale) * eps[s];
                eps[s] *= scale;
            }
            // Plastic work density: the returned deviatoric stress times the plastic increment
            // (backward Euler, Pa). The predictor's overshoot ½ a (1 − scale)² Σ ε² is an O(dt)
            // discretisation error the time integrator does not pay for, so it is not counted.
            let dw = a * w0 * scale * (1.0 - scale);
            if sigma > 0.0 {
                let dep = dw / sigma;
                *v.eps_p.add(p) += dep;
                *v.eps_rate.add(p) = dep / c.dt;
            }
            plastic += (dw * c.vol) as f64;
            *v.temp.add(p) += TAYLOR_QUINNEY * dw / (material.density * material.specific_heat);
        } else {
            *v.eps_rate.add(p) = 0.0;
        }
        let mut sum = 0.0f32;
        for s in 0..SLOTS {
            sum += c.xi_len[s] * eps[s];
        }
        let corr = sum / m_p;
        for s in 0..SLOTS {
            let si = p * SLOTS + s;
            *v.t_state.add(si) = if *v.slot_st.add(si) == 1 {
                bulk_term * c.xi_len[s] + a * (eps[s] - c.xi_len[s] * corr)
            } else {
                0.0
            };
        }
    }
    plastic
}

/// Pass 4: gather the pair forces, broken-neighbour compression and the projectile's contact, and
/// (with `kick`) the second half kick and the boundary damping. Returns the range's projectile
/// work, friction heat and damped energy (J).
unsafe fn pass_gather(v: &View, c: &Consts, lo: usize, hi: usize, tip_s: f32, speed: f32, kick: bool) -> [f64; 3] {
    let h = c.h;
    let h2 = h * h;
    let tip = c.dir * tip_s;
    let dir = c.dir;
    let vproj = c.dir * speed;
    let shape = c.impact.projectile;
    let material = c.material;
    let heat_capacity = c.mass * material.specific_heat;
    let inv_m = 1.0 / c.mass;
    let dt = c.dt;
    let mut proj_work = 0.0f64;
    let mut friction = 0.0f64;
    let mut damped = 0.0f64;
    for p in lo..hi {
        if *v.fixed.add(p) {
            *v.fx.add(p) = 0.0;
            *v.fy.add(p) = 0.0;
            *v.fz.add(p) = 0.0;
            continue;
        }
        let mut f = Vec3::ZERO;
        let valid = *v.valid.add(p);
        for s in 0..SLOTS {
            if valid >> s & 1 == 0 {
                continue;
            }
            let si = p * SLOTS + s;
            let st = *v.slot_st.add(si);
            let m_dir = Vec3::from_array(*v.mdir.add(si));
            if st == 1 {
                let q = (p as isize + c.delta[s]) as usize;
                let opp = (s + BONDS) % SLOTS;
                let t_sum = *v.t_state.add(si) + *v.t_state.add(q * SLOTS + opp);
                f += m_dir * (t_sum * h2);
            } else {
                let stretch = *v.ext.add(si) / c.xi_len[s];
                if stretch < 0.0 {
                    f += m_dir * (c.k_pair * stretch);
                }
            }
        }
        // Projectile contact (ellipsoidal nose, cylindrical body extending back out of the patch).
        let mut fc = Vec3::ZERO;
        let pos = position(v, c, p);
        let rel = pos - tip;
        let behind = -rel.dot(dir);
        if behind > -0.5 * h {
            let radial_v = rel + dir * behind;
            let r2 = radial_v.length_squared();
            let ar = shape.radius_m + 0.5 * h;
            if r2 < ar * ar {
                let r = r2.sqrt();
                let nose = shape.nose_m.max(1e-4);
                let (inside, normal, depth) = if behind < nose {
                    let ln = nose + 0.5 * h;
                    let ax = (nose - behind) / ln;
                    let rho = ((r / ar).powi(2) + ax * ax).sqrt();
                    let nrm = (radial_v / (ar * ar) + dir * (nose - behind) / (ln * ln)).normalize_or_zero();
                    (rho < 1.0, nrm, (1.0 - rho) * ar.min(ln))
                } else {
                    (true, radial_v.normalize_or_zero(), ar - r)
                };
                if inside {
                    let normal = if normal.length_squared() > 0.5 { normal } else { dir };
                    let fn_mag = c.k_contact * depth.max(0.0);
                    fc = normal * fn_mag;
                    let vp = Vec3::new(*v.vx.add(p), *v.vy.add(p), *v.vz.add(p));
                    let slip = vproj - vp;
                    let slip_t = slip - normal * slip.dot(normal);
                    let st = slip_t.length();
                    // A bore surface near its melting point shears in a molten film: friction falls
                    // with the same thermal softening as the flow stress (named).
                    let temp = *v.temp.add(p);
                    let soft = (1.0 - ((temp - AMBIENT_K) / (material.melt_k - AMBIENT_K)).clamp(0.0, 1.0)).max(0.0);
                    if st > 1e-6 && soft > 0.0 {
                        let ft = slip_t / st * (FRICTION_KINETIC * soft * fn_mag);
                        fc += ft;
                        let q = (ft.dot(slip_t) * dt) as f64;
                        friction += q;
                        *v.temp.add(p) += q as f32 / heat_capacity;
                    }
                }
            }
        }
        let prev = Vec3::from_array(*v.contact_prev.add(p));
        proj_work += (0.5 * (fc + prev).dot(vproj) * dt) as f64;
        *v.contact_prev.add(p) = fc.to_array();
        f += fc;
        *v.fx.add(p) = f.x;
        *v.fy.add(p) = f.y;
        *v.fz.add(p) = f.z;
        if kick {
            *v.vx.add(p) += 0.5 * dt * f.x * inv_m;
            *v.vy.add(p) += 0.5 * dt * f.y * inv_m;
            *v.vz.add(p) += 0.5 * dt * f.z * inv_m;
            let eta = *v.damping.add(p);
            if eta > 0.0 {
                let keep = (-eta * dt).exp();
                let (a, b, cc) = (*v.vx.add(p), *v.vy.add(p), *v.vz.add(p));
                let v2 = a * a + b * b + cc * cc;
                damped += 0.5 * c.mass as f64 * v2 as f64 * (1.0 - (keep * keep) as f64);
                *v.vx.add(p) = a * keep;
                *v.vy.add(p) = b * keep;
                *v.vz.add(p) = cc * keep;
            }
        }
    }
    [proj_work, friction, damped]
}

pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn small_impact(perforate: bool, seed: u32) -> PlateImpact {
        PlateImpact {
            material: MaterialId::RolledArmour,
            thickness_m: 0.02,
            obliquity_rad: 0.2,
            projectile: ProjectileShape { mass_kg: 1.43, radius_m: 0.0225, nose_m: 0.054 },
            v_in: 700.0,
            motion: if perforate { Motion::Perforate { v_out: 500.0 } } else { Motion::Stop { depth_m: 0.012 } },
            seed,
        }
    }

    fn coarse(impact: PlateImpact) -> Lattice {
        let mut l = Lattice::new(impact);
        // Keep the debug-build test fast: a bounded run.
        l.t_end = l.t_end.min(l.dt * 400.0);
        l
    }

    #[test]
    fn construction_sizes_the_grid_and_the_time_step() {
        let l = Lattice::new(small_impact(true, 1));
        assert!(l.n <= MAX_PARTICLES && l.dims[2] >= MIN_CELLS_THROUGH);
        assert!((l.dims[2] as f32 * l.h - 0.02).abs() < 1e-6, "plate thickness is exact");
        assert!(l.dt > 0.0 && l.dt < 1e-5);
    }

    #[test]
    fn affine_expansion_gives_exact_dilatation_and_no_interior_force() {
        let mut l = Lattice::new(small_impact(true, 1));
        let c = l.rest_position(l.index(l.dims[0] / 2, l.dims[1] / 2, l.dims[2] / 2));
        let s = 1.0e-4;
        for p in 0..l.n {
            let r = l.rest_position(p) - c;
            l.ux[p] = r.x * s;
            l.uy[p] = r.y * s;
            l.uz[p] = r.z * s;
        }
        l.tip_s = -1.0; // projectile far away
        l.compute_forces();
        let mid = l.index(l.dims[0] / 2, l.dims[1] / 2, l.dims[2] / 2);
        assert!((l.theta[mid] - 3.0 * s).abs() < 1e-3 * 3.0 * s, "theta {}", l.theta[mid]);
        let f = Vec3::new(l.fx[mid], l.fy[mid], l.fz[mid]).length();
        let scale = l.bulk * 3.0 * s * l.h * l.h;
        assert!(f < 1e-3 * scale, "interior force {f} vs scale {scale}");
    }

    #[test]
    fn prescribed_speed_reaches_the_calibrated_exit_and_rest() {
        let imp = small_impact(true, 1);
        assert!((prescribed_speed(&imp, 0.08, 0.0) - 700.0).abs() < 1e-3);
        assert!((prescribed_speed(&imp, 0.08, 0.08) - 500.0).abs() < 1e-2);
        let stop = small_impact(false, 1);
        assert!(prescribed_speed(&stop, 0.012, 0.012) < 1e-3);
    }

    #[test]
    fn energy_closes_and_mass_is_exact() {
        let mut l = coarse(small_impact(true, 7));
        let mass0 = l.total_mass();
        l.solve();
        let ledger = l.ledger_now();
        assert!(ledger.projectile_work > 0.0);
        assert!(ledger.closure() < 0.05, "energy closure {:.4} {:?}", ledger.closure(), ledger);
        assert_eq!(l.total_mass(), mass0);
    }

    #[test]
    fn sliced_solve_matches_the_unsliced_reference_bit_for_bit() {
        let mut a = coarse(small_impact(true, 11));
        let mut b = coarse(small_impact(true, 11));
        a.solve();
        while !b.advance(37) {}
        assert_eq!(a.step, b.step);
        assert!(a.ux.iter().zip(&b.ux).all(|(x, y)| x.to_bits() == y.to_bits()));
        assert!(a.temp.iter().zip(&b.temp).all(|(x, y)| x.to_bits() == y.to_bits()));
        assert_eq!(a.bond_state, b.bond_state);
    }

    #[test]
    fn oblique_thin_plate_closes_its_energy_ledger() {
        // The case whose initial nose overlap once created energy: 76 mm AP, 25 mm roof, 34 deg.
        let imp = PlateImpact { material: MaterialId::RolledArmour, thickness_m: 0.025, obliquity_rad: 0.6,
            projectile: ProjectileShape { mass_kg: 6.5, radius_m: 0.0381, nose_m: 0.091 }, v_in: 600.0,
            motion: Motion::Perforate { v_out: 520.0 }, seed: 99 };
        let mut l = Lattice::new(imp);
        l.t_end = l.t_end.min(l.dt * 500.0);
        l.solve();
        let led = l.ledger_now();
        assert!(led.closure() < 0.02, "closure {:.4} {:?}", led.closure(), led);
        let vmax = (0..l.n).map(|p| l.velocity(p).length()).fold(0.0f32, f32::max);
        assert!(vmax < 3.0 * 600.0, "no particle may be flung faster than an elastic bounce off the nose: {vmax}");
    }

    #[test]
    fn each_impact_state_is_unique_and_repeatable() {
        let run = |seed: u32| {
            let mut l = coarse(small_impact(true, seed));
            l.solve();
            (l.bond_state.clone(), l.ux.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
        };
        let (a1, ua1) = run(21);
        let (a2, ua2) = run(21);
        let (b, ub) = run(22);
        assert_eq!(a1, a2);
        assert_eq!(ua1, ua2);
        assert!(a1 != b || ua1 != ub, "a different impact state must tear differently");
    }

    #[test]
    fn the_rim_cools_monotonically_from_its_solved_temperature() {
        let mut l = coarse(small_impact(true, 9));
        l.solve();
        let times = crate::armor_damage::RIM_CURVE_TIMES;
        let curve = l.rim_cooling(&times);
        assert_eq!(curve.len(), times.len());
        assert!(curve[0] > AMBIENT_K + 50.0, "fresh rim {}", curve[0]);
        for w in curve.windows(2) {
            assert!(w[1] <= w[0] + 1e-3, "{curve:?}");
        }
        assert!(curve[curve.len() - 1] < curve[0]);
    }

    #[test]
    fn thread_count_does_not_change_the_result() {
        let mut a = coarse(small_impact(true, 5));
        let mut b = coarse(small_impact(true, 5));
        a.set_threads(1);
        b.set_threads(4);
        a.solve();
        b.solve();
        assert_eq!(a.step, b.step);
        assert!(a.ux.iter().zip(&b.ux).all(|(x, y)| x.to_bits() == y.to_bits()));
        assert!(a.temp.iter().zip(&b.temp).all(|(x, y)| x.to_bits() == y.to_bits()));
        assert_eq!(a.bond_state, b.bond_state);
        let (la, lb) = (a.ledger_now(), b.ledger_now());
        assert!((la.plastic - lb.plastic).abs() <= 1e-9 * la.plastic.abs().max(1.0));
    }

    #[test]
    fn a_perforation_breaks_bonds_and_heats_the_bore_more_than_a_stop() {
        let mut through = coarse(small_impact(true, 3));
        let mut stop = coarse(small_impact(false, 3));
        through.solve();
        stop.solve();
        let broken = |l: &Lattice| l.bond_state.iter().filter(|&&s| s == 2).count();
        assert!(broken(&through) > broken(&stop), "{} vs {}", broken(&through), broken(&stop));
        let tmax = |l: &Lattice| l.temp.iter().cloned().fold(0.0f32, f32::max);
        assert!(tmax(&through) > AMBIENT_K + 50.0, "bore heated to {}", tmax(&through));
    }
}
/// One body that broke free of the plate: a connected set of particles.
#[derive(Clone, Debug, Serialize)]
pub struct Cluster {
    pub particles: Vec<u32>,
    pub mass_kg: f32,
    /// Plate frame.
    pub centre: Vec3,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    /// Kinetic energy of the particles' motion about the cluster's own mean (internal), J.
    pub internal_kinetic_j: f32,
    pub temperature_k: f32,
    /// Edge of a cube of the cluster's volume, m.
    pub size_m: f32,
    /// The detached cluster ahead of the nose on the path (the punched plug).
    pub plug: bool,
}

impl Cluster {
    /// Whether this cluster leaves the plate on the inside: moving inward and, when the round
    /// perforated, past the plate's mid-thickness (the bore is open); when it stopped, already past
    /// the original back face (a scab), since material crushed ahead of the nose is still enclosed
    /// by intact plate.
    pub fn leaves_inside(&self, thickness_m: f32, cell_m: f32, perforated: bool) -> bool {
        self.velocity.z > 0.0 && if perforated {
            self.centre.z >= 0.5 * thickness_m
        } else {
            self.centre.z >= thickness_m - 0.5 * cell_m
        }
    }
}

/// Terminal geometry measured from the solved lattice, in the plate frame (millimetres where the
/// name says so). Written into `ArmorScar` in place of the empirical rule.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct MeasuredGeometry {
    /// Open bore radius across the shot (0 when not perforated), mm.
    pub aperture_mm: f32,
    /// Bore radius along the shot's in-plane direction, mm.
    pub aperture_along_mm: f32,
    /// Radius of the back face's missing or torn material around the exit, mm.
    pub back_radius_mm: f32,
    pub petals: u8,
    pub petal_mm: f32,
    pub plug: bool,
    pub plug_mass_kg: f32,
    /// Raised lip on the struck face, mm.
    pub lip_mm: f32,
    /// Back-face bulge, mm.
    pub bulge_mm: f32,
    /// Cavity depth along the normal for a stopped round, mm.
    pub cavity_mm: f32,
    /// Radius of the plastically worked zone on the struck face, mm.
    pub worked_radius_mm: f32,
    /// Hottest particle on the bore wall or crater floor, K.
    pub rim_kelvin: f32,
    /// Work the lattice absorbed over the kinetic energy the calibrated law removed.
    pub work_ratio: f32,
}

struct UnionFind {
    parent: Vec<u32>,
}
impl UnionFind {
    fn new(n: usize) -> Self {
        Self { parent: (0..n as u32).collect() }
    }
    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let p = self.parent[x as usize];
            self.parent[x as usize] = self.parent[p as usize];
            x = p;
        }
        x
    }
    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Deterministic: the smaller root wins.
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi as usize] = lo;
        }
    }
}

impl Lattice {
    /// Particles grouped by intact bonds. Returns (cluster id per particle, the plate's id).
    pub fn connectivity(&self) -> (Vec<u32>, u32) {
        let mut uf = UnionFind::new(self.n);
        for p in 0..self.n {
            for b in 0..BONDS {
                if self.bond_state[p * BONDS + b] == 1 {
                    let q = (p as isize + self.delta[b]) as usize;
                    uf.union(p as u32, q as u32);
                }
            }
        }
        let ids: Vec<u32> = (0..self.n as u32).map(|p| uf.find(p)).collect();
        // The plate is whatever holds the clamped boundary (the lowest-index fixed particle's root).
        let plate = (0..self.n).find(|&p| self.fixed[p]).map_or(0, |p| ids[p]);
        (ids, plate)
    }

    /// Detached clusters with their mass, motion and temperature. Deterministic order: by the
    /// lowest particle index in each cluster.
    pub fn clusters(&self) -> Vec<Cluster> {
        let (ids, plate) = self.connectivity();
        let mut map: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
        for (p, &id) in ids.iter().enumerate() {
            if id != plate {
                map.entry(id).or_default().push(p as u32);
            }
        }
        let vol = self.h.powi(3);
        let mut out: Vec<Cluster> = map.into_values().map(|particles| {
            let m = self.mass;
            let total = m * particles.len() as f32;
            let mut c = Vec3::ZERO;
            let mut v = Vec3::ZERO;
            let mut t = 0.0f32;
            for &p in &particles {
                c += self.position(p as usize);
                v += self.velocity(p as usize);
                t += self.temp[p as usize];
            }
            let k = particles.len() as f32;
            c /= k;
            v /= k;
            t /= k;
            // Angular momentum about the centre and a point-mass inertia tensor.
            let mut l = Vec3::ZERO;
            let mut inertia = glam::Mat3::ZERO;
            let mut internal = 0.0f32;
            for &p in &particles {
                let r = self.position(p as usize) - c;
                let dv = self.velocity(p as usize) - v;
                l += r.cross(dv) * m;
                internal += 0.5 * m * dv.length_squared();
                let rr = r.length_squared() + self.h * self.h / 6.0; // a cube's own inertia
                inertia += (glam::Mat3::from_diagonal(Vec3::splat(rr)) - outer(r)) * m;
            }
            let omega = if inertia.determinant().abs() > 1e-30 { inertia.inverse() * l } else { Vec3::ZERO };
            Cluster { particles, mass_kg: total, centre: c, velocity: v, angular_velocity: omega,
                internal_kinetic_j: internal, temperature_k: t, size_m: (k * vol).cbrt(), plug: false }
        }).collect();
        // The plug: the heaviest cluster whose centre lies on the path beyond the struck face.
        let a = self.impact.projectile.radius_m;
        let mut best: Option<(usize, f32)> = None;
        for (i, cl) in out.iter().enumerate() {
            let along = cl.centre.dot(self.dir);
            let radial = (cl.centre - self.dir * along).length();
            if along > 0.0 && radial < a * 1.2 && best.is_none_or(|(_, m)| cl.mass_kg > m) {
                best = Some((i, cl.mass_kg));
            }
        }
        if let Some((i, m)) = best {
            let plug_volume_mass = std::f32::consts::PI * a * a * self.impact.thickness_m * self.material.density;
            if m > 0.2 * plug_volume_mass {
                out[i].plug = true;
            }
        }
        out
    }

    /// Measure the terminal geometry from the solved state.
    pub fn measure(&self) -> MeasuredGeometry {
        let (ids, plate) = self.connectivity();
        let t = self.impact.thickness_m;
        let h = self.h;
        let nz = self.dims[2];
        let a = self.impact.projectile.radius_m;
        let perforated = matches!(self.impact.motion, Motion::Perforate { .. });
        let exit = self.dir * (t / self.dir.z.max(0.1));
        let mut g = MeasuredGeometry::default();
        // Bore: the innermost plate particles around the path at mid-thickness, by sector.
        const SECTORS: usize = 32;
        let mut inner = [f32::INFINITY; SECTORS];
        let side = Vec3::Y;
        let up = self.dir.cross(side).normalize_or_zero();
        let mut rim_k = AMBIENT_K;
        for p in 0..self.n {
            if ids[p] != plate {
                continue;
            }
            let rest = self.rest_position(p);
            if rest.z < 0.25 * t || rest.z > 0.75 * t {
                continue;
            }
            let pos = self.position(p);
            let along = pos.dot(self.dir);
            let radial_v = pos - self.dir * along;
            let r = radial_v.length();
            let angle = radial_v.dot(up).atan2(radial_v.dot(side));
            let sector = (((angle + std::f32::consts::PI) / std::f32::consts::TAU * SECTORS as f32) as usize).min(SECTORS - 1);
            inner[sector] = inner[sector].min(r);
            if r < a + 2.0 * h {
                rim_k = rim_k.max(self.temp[p]);
            }
        }
        if perforated {
            let mut across: Vec<f32> = Vec::new();
            let mut along: Vec<f32> = Vec::new();
            for (s, &r) in inner.iter().enumerate() {
                if !r.is_finite() {
                    continue;
                }
                let angle = (s as f32 + 0.5) / SECTORS as f32 * std::f32::consts::TAU - std::f32::consts::PI;
                // Sectors near ±90° (along "up", which lies in the shot plane) measure along.
                if angle.sin().abs() > 0.707 { along.push(r) } else { across.push(r) }
            }
            let median = |v: &mut Vec<f32>| {
                if v.is_empty() { return 0.0; }
                v.sort_by(f32::total_cmp);
                v[v.len() / 2]
            };
            g.aperture_mm = ((median(&mut across) - 0.5 * h).max(0.0)) * 1000.0;
            g.aperture_along_mm = ((median(&mut along) - 0.5 * h).max(0.0)) * 1000.0;
        }
        // Back face: missing or torn material around the exit, petals bent out past it.
        let mut back_r = 0.0f32;
        let mut petal_sector = [0.0f32; 16];
        let mut bulge = 0.0f32;
        let mut lip = 0.0f32;
        let mut worked = 0.0f32;
        for p in 0..self.n {
            let rest = self.rest_position(p);
            let k = ((rest.z - self.origin.z) / h).round() as usize;
            let pos = self.position(p);
            let lateral_rest = Vec3::new(rest.x - exit.x, rest.y, 0.0);
            if k == nz - 1 {
                let detached = ids[p] != plate;
                let torn = self.broken_fraction(p) > 0.3;
                if detached || torn {
                    back_r = back_r.max(lateral_rest.length());
                }
                if !detached {
                    let out = pos.z - rest.z;
                    bulge = bulge.max(out);
                    if perforated && out > 0.5 * h && lateral_rest.length() < 3.0 * a {
                        let angle = lateral_rest.y.atan2(lateral_rest.x);
                        let s = (((angle + std::f32::consts::PI) / std::f32::consts::TAU * 16.0) as usize).min(15);
                        petal_sector[s] = petal_sector[s].max(out);
                    }
                }
            }
            if k == 0 && ids[p] == plate {
                lip = lip.max(rest.z - pos.z);
                if self.max_plastic(p) > 0.01 {
                    worked = worked.max(Vec3::new(rest.x, rest.y, 0.0).length());
                }
            }
        }
        // Contiguous runs of bent-out sectors are petals.
        let bent: Vec<bool> = petal_sector.iter().map(|&o| o > 0.5 * h).collect();
        if bent.iter().any(|&b| b) && !bent.iter().all(|&b| b) {
            let start = bent.iter().position(|&b| !b).unwrap();
            let mut runs = 0u8;
            let mut longest = 0.0f32;
            let mut inside = false;
            for i in 0..16 {
                let s = (start + i) % 16;
                if bent[s] && !inside { runs += 1; }
                inside = bent[s];
                if bent[s] { longest = longest.max(petal_sector[s]); }
            }
            g.petals = runs;
            g.petal_mm = longest * 1000.0;
        } else if bent.iter().all(|&b| b) {
            g.petals = 0;
        }
        let clusters = self.clusters();
        if let Some(plug) = clusters.iter().find(|c| c.plug) {
            g.plug = true;
            g.plug_mass_kg = plug.mass_kg;
        }
        g.back_radius_mm = back_r * 1000.0;
        g.bulge_mm = bulge * 1000.0;
        g.lip_mm = lip * 1000.0;
        g.worked_radius_mm = worked * 1000.0;
        if let Motion::Stop { depth_m } = self.impact.motion {
            g.cavity_mm = depth_m * self.dir.z * 1000.0;
        }
        g.rim_kelvin = rim_k;
        g.work_ratio = (self.ledger.projectile_work / self.calibrated_loss_j.max(1e-9)) as f32;
        g
    }

    /// Normal force the plate presses onto the projectile's cylindrical body behind its nose at
    /// the end of the solve (N): the grip that friction acts on when a stopped round is pulled.
    pub fn body_grip_normal_n(&self) -> f32 {
        let tip = self.dir * self.tip_s;
        let nose = self.impact.projectile.nose_m;
        let mut total = 0.0f32;
        for p in 0..self.n {
            let f = Vec3::from_array(self.contact_prev[p]);
            if f == Vec3::ZERO {
                continue;
            }
            let rel = self.position(p) - tip;
            let behind = -rel.dot(self.dir);
            if behind < nose {
                continue;
            }
            let radial = (rel + self.dir * behind).normalize_or_zero();
            total += f.dot(radial).abs();
        }
        total
    }

    /// Temperature of the hottest exposed surface particle around the bore at each of `times`
    /// (s after the strike): explicit conduction through intact face bonds, radiation (the
    /// material's emissivity) and natural convection (10 W/m² K, named) from every exposed face.
    /// The patch's own boundary is adiabatic (named: the rest of the plate is not a heat sink here,
    /// so late temperatures run slightly high).
    pub fn rim_cooling(&self, times: &[f32]) -> Vec<f32> {
        const FACE_SLOTS: [usize; 6] = [0, 1, 2, BONDS, BONDS + 1, BONDS + 2];
        const NATURAL_CONVECTION: f32 = 10.0;
        let m = self.material;
        let alpha = m.conductivity / (m.density * m.specific_heat);
        let h = self.h;
        let face = h * h;
        let capacity = m.density * m.specific_heat * h * h * h;
        let a = self.impact.projectile.radius_m + 3.0 * h;
        let tip = self.dir * self.tip_s;
        // Particles near the bore whose temperature counts as the rim, and every particle's exposed
        // face count.
        let mut exposed = vec![0u8; self.n];
        let mut rim: Vec<usize> = Vec::new();
        for p in 0..self.n {
            let mut faces = 0u8;
            for &s in &FACE_SLOTS {
                if self.slot_state(p, s) != 1 {
                    faces += 1;
                }
            }
            exposed[p] = faces;
            let rel = self.position(p) - tip;
            let along = rel.dot(self.dir);
            let radial = (rel - self.dir * along).length();
            if faces > 0 && radial < a {
                rim.push(p);
            }
        }
        let mut temp = self.temp.clone();
        let mut next = temp.clone();
        let rim_now = |temp: &[f32]| rim.iter().map(|&p| temp[p]).fold(AMBIENT_K, f32::max);
        let mut out = Vec::with_capacity(times.len());
        let mut t = 0.0f32;
        let dt_stable = 0.15 * h * h / alpha;
        for &target in times {
            while t < target - 1e-6 {
                let dt = dt_stable.min(0.05 * (t + 0.5)).min(target - t);
                for p in 0..self.n {
                    let tp = temp[p];
                    let mut q = 0.0f32;
                    for &s in &FACE_SLOTS {
                        if self.slot_state(p, s) == 1 {
                            let n = (p as isize + self.delta[s]) as usize;
                            q += m.conductivity * h * (temp[n] - tp);
                        }
                    }
                    if exposed[p] > 0 {
                        let area = face * exposed[p] as f32;
                        q -= area * (m.emissivity * materials::STEFAN_BOLTZMANN * (tp.powi(4) - AMBIENT_K.powi(4))
                            + NATURAL_CONVECTION * (tp - AMBIENT_K));
                    }
                    next[p] = (tp + q * dt / capacity).max(AMBIENT_K.min(tp));
                }
                std::mem::swap(&mut temp, &mut next);
                t += dt;
            }
            out.push(rim_now(&temp));
        }
        out
    }

    /// Fraction of particle `p`'s family bonds that are broken.
    pub fn broken_fraction(&self, p: usize) -> f32 {
        let (mut broken, mut total) = (0, 0);
        for s in 0..SLOTS {
            match self.slot_state(p, s) { 1 => total += 1, 2 => { total += 1; broken += 1 }, _ => {} }
        }
        if total == 0 { 0.0 } else { broken as f32 / total as f32 }
    }

    /// Equivalent plastic strain of particle `p`.
    pub fn max_plastic(&self, p: usize) -> f32 {
        self.eps_p[p]
    }
}

fn outer(r: Vec3) -> glam::Mat3 {
    glam::Mat3::from_cols(r * r.x, r * r.y, r * r.z)
}

#[cfg(test)]
mod look {
    use super::*;
    /// Writes lattice section images to the scratch directory named by FEA_LOOK_DIR.
    #[test]
    #[ignore]
    fn look_at_lattice_cases() {
        let dir = std::env::var("FEA_LOOK_DIR").unwrap_or_else(|_| "/tmp".into());
        let cases = [
            ("88_apcr_100mm", 0.100, 0.0f32, 0.044 * 0.37 * 2.0 / 2.0, 7.3, 0.044, 915.0, Motion::Perforate { v_out: 697.0 }),
            ("76_ap_60mm", 0.060, 0.2, 0.0381, 6.5, 0.091, 640.0, Motion::Perforate { v_out: 300.0 }),
            ("45_ap_stop_100mm", 0.100, 0.0, 0.0225, 1.43, 0.054, 740.0, Motion::Stop { depth_m: 0.052 }),
            ("76_ap_roof_25mm", 0.025, 0.6, 0.0381, 6.5, 0.091, 600.0, Motion::Perforate { v_out: 520.0 }),
        ];
        for (name, t, ob, radius, mass, nose, v, motion) in cases {
            let imp = PlateImpact { material: MaterialId::RolledArmour, thickness_m: t, obliquity_rad: ob,
                projectile: ProjectileShape { mass_kg: mass, radius_m: radius, nose_m: nose }, v_in: v, motion, seed: 99 };
            let mut l = Lattice::new(imp);
            let t0 = std::time::Instant::now();
            l.solve();
            let wall = t0.elapsed();
            let g = l.measure();
            let led = l.ledger_now();
            let clusters = l.clusters();
            println!("{name}: n {} dims {:?} h {:.2} mm dt {:.3} us steps {} wall {:.0} ms", l.n, l.dims, l.h * 1000.0,
                l.dt * 1e6, l.step, wall.as_secs_f64() * 1000.0);
            println!("   geometry {:?}", g);
            println!("   ledger closure {:.4} work {:.0} J calibrated {:.0} J kin {:.0} el {:.0} pl {:.0} fr {:.0} damp {:.0} frac {:.0}",
                led.closure(), led.projectile_work, l.calibrated_loss_j, led.kinetic, led.elastic, led.plastic, led.friction, led.damping, led.fracture);
            println!("   clusters {} (plug {:?}), largest {:.3} kg, fragment mass {:.3} kg",
                clusters.len(), clusters.iter().find(|c| c.plug).map(|c| c.mass_kg),
                clusters.iter().map(|c| c.mass_kg).fold(0.0, f32::max), clusters.iter().map(|c| c.mass_kg).sum::<f32>());
            crate::fea_damage::section::lattice_png(&l, &std::path::Path::new(&dir).join(format!("lattice_{name}.png")), 6).unwrap();
        }
    }
}

#[cfg(test)]
/// Diagnostics (ignored): energy closure by term and through time, time-step sensitivity.
mod diagnostics {
    use super::*;
    #[test]
    #[ignore]
    fn time_step_stability_scan() {
        for (name, t, ob, radius, mass, nose, v, motion) in [
            ("76_ap_60mm", 0.060f32, 0.2f32, 0.0381f32, 6.5f32, 0.091f32, 640.0f32, Motion::Perforate { v_out: 300.0 }),
            ("76_ap_roof_25mm", 0.025, 0.6, 0.0381, 6.5, 0.091, 600.0, Motion::Perforate { v_out: 520.0 }),
        ] {
            for cfl_scale in [0.5f32, 1.0, 1.5, 2.0] {
                let imp = PlateImpact { material: MaterialId::RolledArmour, thickness_m: t, obliquity_rad: ob,
                    projectile: ProjectileShape { mass_kg: mass, radius_m: radius, nose_m: nose }, v_in: v, motion, seed: 99 };
                let mut l = Lattice::new(imp);
                l.dt *= cfl_scale;
                let t0 = std::time::Instant::now();
                l.solve();
                let led = l.ledger_now();
                let g = l.measure();
                println!("{name} dt x{cfl_scale}: steps {} wall {:.0} ms closure {:.4} kin {:.0} pl {:.0} aperture {:.1} bulge {:.1} lip {:.1} petals {} rim {:.0}",
                    l.step, t0.elapsed().as_secs_f64() * 1e3, led.closure(), led.kinetic, led.plastic, g.aperture_mm, g.bulge_mm, g.lip_mm, g.petals, g.rim_kelvin);
            }
        }
    }

    #[test]
    #[ignore]
    fn closure_through_time() {
        let imp = PlateImpact { material: MaterialId::RolledArmour, thickness_m: 0.025, obliquity_rad: 0.6,
            projectile: ProjectileShape { mass_kg: 6.5, radius_m: 0.0381, nose_m: 0.091 }, v_in: 600.0,
            motion: Motion::Perforate { v_out: 520.0 }, seed: 99 };
        let mut l = Lattice::new(imp);
        while !l.done {
            l.advance(50);
            let led = l.ledger_now();
            let broken = l.bond_state.iter().filter(|&&b| b == 2).count();
            let vmax = (0..l.n).map(|p| l.velocity(p).length()).fold(0.0f32, f32::max);
            println!("step {:5} tip {:.3} work {:9.0} acc {:9.0} diff {:8.0} kin {:8.0} el {:7.0} pl {:8.0} fr {:7.0} frac {:7.0} broken {broken} vmax {vmax:.0}",
                l.step, l.tip_s, led.projectile_work, led.accounted(), led.accounted() - led.projectile_work,
                led.kinetic, led.elastic, led.plastic, led.friction, led.fracture);
        }
    }

    #[test]
    #[ignore]
    fn isolate_energy_error() {
        for (name, elastic, nofrac, dt_scale, rate_free) in [("full", false, false, 1.0f32, false), ("no fracture", false, true, 1.0, false),
            ("no frac, no rate", false, true, 1.0, true), ("elastic", true, true, 1.0, false), ("no frac half dt", false, true, 0.5, false),
            ("full half dt", false, false, 0.5, false)] {
            let roof = false;
            let mut imp = tests::small_impact(true, 7);
            imp.v_in = 300.0;
            imp.motion = Motion::Perforate { v_out: 250.0 };
            if roof {
                imp = PlateImpact { material: MaterialId::RolledArmour, thickness_m: 0.025, obliquity_rad: 0.6,
                    projectile: ProjectileShape { mass_kg: 6.5, radius_m: 0.0381, nose_m: 0.091 }, v_in: 600.0,
                    motion: Motion::Perforate { v_out: 520.0 }, seed: 99 };
            }
            let mut l = Lattice::new(imp);
            l.debug_elastic = elastic;
            l.debug_no_fracture = nofrac;
            l.debug_rate_free = rate_free;
            l.dt *= dt_scale;
            if !roof { l.t_end = l.t_end.min(l.dt * 400.0 / dt_scale); }
            l.solve();
            let led = l.ledger_now();
            println!("{name:16}: closure {:.4} work {:.0} kin {:.0} el {:.0} pl {:.0} fr {:.0} damp {:.0} contact {:.1} frac {:.0}",
                led.closure(), led.projectile_work, led.kinetic, led.elastic, led.plastic, led.friction, led.damping, led.contact, led.fracture);
        }
    }
}
