//! Game side of the layered damage model (`crate::fea_damage`, docs/FEA_DAMAGE_MODEL_20261102.md):
//! the player's Tiger's shot line in its hull frame, the chain's consequences for its modules, the
//! residual penetrator's own path through crew and live rounds, and rounds lodged in the plate or
//! inside the vehicle.
//!
//! The struck plate's outcome is the existing calibrated `ImpactEvent`; nothing here changes
//! whether a plate is perforated. A perforated casing knocks its system out; a casing struck but
//! not perforated leaves the system working. The penetrator reaches crew and live rounds through
//! the same anatomical volumes and cartridge boxes as the fragments, with its real mass and speed;
//! injury outcomes stay with the existing crew model.
use super::*;
use crate::fea_damage::shot;
use crate::fea_damage::components::{self, Side, System};
use crate::fea_damage::{chain, compartment, fragments, lattice, lodged};

/// Contact identity of the residual penetrator in fragment contact records. Fragments use
/// `penetration_id << 10 | index`; the penetrator sets the top bit.
pub(super) fn penetrator_contact_id(penetration_id: u64) -> u64 {
    (1u64 << 63) | (penetration_id & ((1u64 << 62) - 1))
}

/// A round held by the Tiger's steel: in the struck plate (nose in, base outside) or inside a
/// component it stopped in.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct LodgedRound {
    pub(super) id: u64,
    #[serde(with = "crate::ballistics::shell_spec_ref")]
    pub(super) shell: &'static crate::ballistics::ShellSpec,
    pub(super) frame: BloodFrame,
    /// Nose tip and travel direction (nose forward) in `frame`.
    pub(super) tip_local: Vec3,
    pub(super) dir_local: Vec3,
    /// How far it went into the layer that holds it, m.
    pub(super) depth_m: f32,
    pub(super) in_struck_plate: bool,
    /// The component it stopped in, if any (catalog id).
    pub(super) component: Option<String>,
    pub(super) extraction: lodged::Extraction,
    pub(super) struck_at_s: f32,
    /// Held by a part that elevates with the gun (breech, cradle, sight): `tip_local` and
    /// `dir_local` are then in the gun's frame (`Tank::gun_model`), not `frame`'s.
    pub(super) on_gun: bool,
}

/// One chain run on the player's Tiger, kept for diagnostics, receipts and cross sections.
#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct FeaChainReceipt {
    pub(super) shell: &'static str,
    pub(super) terminal: crate::ballistics::Terminal,
    pub(super) entry_hull: Vec3,
    pub(super) dir_hull: Vec3,
    pub(super) far_wall_mm: Option<f32>,
    pub(super) result: chain::ChainResult,
    pub(super) knocked_out: Vec<ModuleKind>,
    pub(super) struck_not_breached: Vec<String>,
    /// The crew compartment's air after a burst inside it.
    pub(super) compartment: Option<CompartmentReceipt>,
}

/// What a burst inside the crew compartments did to their air and hatches.
#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct CompartmentReceipt {
    pub(super) peak_gauge_pa: f32,
    pub(super) energy_into_air_j: f32,
    pub(super) fragment_kinetic_j: f32,
    pub(super) vent_area_m2: f32,
    pub(super) hatches_blown: Vec<(&'static str, f32)>,
}

/// The voxel lattice's solve of one strike on the player's Tiger, in the hull frame.
#[derive(Clone, Debug)]
pub(super) struct LatticeOutcome {
    pub(super) measured: crate::armor_damage::MeasuredScar,
    pub(super) geometry: lattice::MeasuredGeometry,
    pub(super) clusters: Vec<lattice::Cluster>,
    /// The plate frame: origin at the strike on the struck face, x along the shot's in-plane
    /// direction, y across it, z into the plate (all hull-local).
    pub(super) origin_hull: Vec3,
    pub(super) axes_hull: [Vec3; 3],
    pub(super) thickness_m: f32,
    pub(super) particles: usize,
    pub(super) steps: u64,
    pub(super) cell_mm: f32,
    pub(super) wall_ms: f32,
    pub(super) ledger: lattice::Ledger,
    pub(super) grip_normal_n: f32,
    /// The rim's solved cooling (`armor_damage::RIM_CURVE_TIMES`).
    pub(super) rim_curve: crate::armor_damage::RimCurve,
}

impl LatticeOutcome {
    pub(super) fn plate_to_hull(&self, p: Vec3) -> Vec3 {
        self.origin_hull + self.axes_hull[0] * p.x + self.axes_hull[1] * p.y + self.axes_hull[2] * p.z
    }
    pub(super) fn plate_dir_to_hull(&self, v: Vec3) -> Vec3 {
        self.axes_hull[0] * v.x + self.axes_hull[1] * v.y + self.axes_hull[2] * v.z
    }
}

/// Everything solved for the strike being resolved now; consumed by the scar, the penetration's
/// consequences and its fragments, then cleared.
#[derive(Clone, Debug)]
pub(super) struct ImpactSolve {
    pub(super) projectile_id: u64,
    pub(super) line: chain::ShotLine,
    pub(super) chain: chain::ChainResult,
    pub(super) lattice: Option<LatticeOutcome>,
    /// The round's own solve, for a perforation (`fea_damage::shot`).
    pub(super) shot: Option<ShotStrike>,
}

/// Cells across the calibre for the round's solve in play (the review film draws 12; the same in
/// every preset).
pub(super) const SHOT_CELLS_ACROSS: f32 = 8.0;

/// What the round's own solve gave a perforation: the share of its mass that flies on, and the
/// pieces it shed through the plate (plate frame).
#[derive(Clone, Debug)]
pub(super) struct ShotStrike {
    pub(super) residual_kg: f32,
    pub(super) mass_kg: f32,
    pub(super) pieces: usize,
    pub(super) shrapnel: Vec<crate::fea_damage::shot::Shrapnel>,
    pub(super) wall_ms: f32,
}

impl ShotStrike {
    /// Solve the round of a perforation as a deformable body (`None` for other outcomes and for
    /// rounds without a kinetic body).
    pub(super) fn solve(event: &crate::ballistics::ImpactEvent, seed: u32) -> Option<Self> {
        if event.terminal != crate::ballistics::Terminal::Penetration {
            return None;
        }
        let impact = crate::fea_damage::shot::impact_of(event, seed)?;
        let started = std::time::Instant::now();
        let mut shot = crate::fea_damage::shot::Shot::with_cells(impact, if super::crew_tissue::reference_solve() { SHOT_CELLS_ACROSS } else { 3.0 });
        let out = shot.solve_for(crate::fea_damage::shot::GAME_HORIZON);
        Some(Self { residual_kg: out.residual_kg, mass_kg: out.mass_kg, pieces: out.pieces.len(),
            shrapnel: out.shrapnel(impact.thickness_m), wall_ms: started.elapsed().as_secs_f32() * 1000.0 })
    }

    pub(super) fn residual_fraction(&self) -> f32 {
        (self.residual_kg / self.mass_kg.max(1e-6)).clamp(0.0, 1.0)
    }
}

/// One plate problem for the lattice and the hull-frame placement of its answer.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct PlateJob {
    pub(super) impact: lattice::PlateImpact,
    pub(super) origin: Vec3,
    pub(super) axes: [Vec3; 3],
}

/// Offline-only recorder of the actual intermediate lattice states. No interpolation of a
/// final scar, and no change to the terminal/damage solver or its input event.
pub(super) struct PlateFilmFrame {pub t:f32,pub positions:Vec<Vec3>,pub temperatures:Vec<f32>,pub tip:Vec3}
pub(super) struct PlateFilm {pub origin:Vec3,pub axes:[Vec3;3],pub h:f32,pub half:[f32;3],pub frames:Vec<PlateFilmFrame>,pub attached:Vec<bool>}
pub(super) static RECORD_PLATE_FILM:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
pub(super) static PLATE_FILM:std::sync::Mutex<Option<PlateFilm>>=std::sync::Mutex::new(None);
impl PlateJob {
    /// Solve on a worker thread. The solve is bit-exact whatever the thread count or slicing
    /// (`lattice` tests), so a job re-run after a load gives the same answer.
    pub(super) fn spawn(self) -> std::thread::JoinHandle<LatticeOutcome> {
        std::thread::spawn(move || self.solve_with(lattice::background_threads()))
    }

    /// Solve now with every worker (the tick waits for it).
    pub(super) fn solve(self) -> LatticeOutcome {
        self.solve_with(lattice::default_threads())
    }

    fn solve_with(self, threads: usize) -> LatticeOutcome {
        let started = std::time::Instant::now();
        let reference = super::crew_tissue::reference_solve();
        let mut solver = lattice::Lattice::with_particle_budget(self.impact,
            if reference { lattice::MAX_PARTICLES } else { 1500 });
        solver.set_threads(if reference { threads } else { 1 });
        if RECORD_PLATE_FILM.load(std::sync::atomic::Ordering::Relaxed) {
            let mut film=PlateFilm{origin:self.origin,axes:self.axes,h:solver.h,
                half:[solver.dims[0]as f32*solver.h*0.5,solver.dims[1]as f32*solver.h*0.5,self.impact.thickness_m],frames:Vec::new(),attached:Vec::new()};
            loop {
                film.frames.push(PlateFilmFrame{t:solver.t,positions:(0..solver.n).map(|i|solver.position(i)).collect(),temperatures:solver.temp.clone(),tip:solver.dir*solver.tip_s});
                if solver.done{break;}
                solver.advance((1.0/(60.0*4096.0*solver.dt)).ceil().max(1.0)as u64);
            }
            let (components,plate)=solver.connectivity();
            film.attached=components.iter().map(|&c|c==plate).collect();
            *PLATE_FILM.lock().unwrap()=Some(film);
        }else{solver.solve();}
        let wall_ms = started.elapsed().as_secs_f32() * 1000.0;
        lattice_outcome(&mut solver, self.origin, self.axes, wall_ms)
    }
}

/// A plate solve of a round the plate stopped, running on a worker thread. It commits at a fixed
/// game time after the strike (`FEA_STOP_COMMIT_S`), deterministically; if it is not finished by
/// then, that update waits for it (logged).
#[derive(Debug)]
pub(super) struct PendingPlate {
    pub(super) commit_at_s: f32,
    pub(super) struck_at_s: f32,
    pub(super) facing: crate::ballistics::Facing,
    pub(super) local_mm: [i16; 3],
    pub(super) lodged_id: Option<u64>,
    pub(super) job: PlateJob,
    pub(super) handle: std::thread::JoinHandle<LatticeOutcome>,
}

/// A deferred plate solve as the world save keeps it: its problem, re-solved on load.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct PendingPlateSaved {
    commit_at_s: f32,
    struck_at_s: f32,
    facing: crate::ballistics::Facing,
    local_mm: [i16; 3],
    lodged_id: Option<u64>,
    job: PlateJob,
}

/// The damage model's simulation state in the world save (owner `vehicles.tanks`): rounds held by
/// the steel, deferred plate solves, the chip in a crewman's hand. Chain receipts are a diagnostic
/// log, not simulation state, and are not kept.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct FeaSaved {
    #[serde(default)] live_contexts: std::collections::BTreeMap<u64, LiveImpactContext>,
    #[serde(default)] contact_states: std::collections::BTreeMap<u64, FragmentDamageState>,
    #[serde(default)] live_births: std::collections::BTreeMap<u64,f32>,
    #[serde(default)] fragment_shapes: std::collections::BTreeMap<u64,(f32,Vec<Vec3>)>,
    #[serde(default)] temporal: Vec<temporal::TemporalSaved>,
    #[serde(default)] bursts: Vec<temporal::LiveBurst>,
    lodged: Vec<LodgedRound>,
    pending: Vec<PendingPlateSaved>,
    carried_fragment: Option<usize>,
    next_id: u64,
    #[serde(default)]
    interior: InteriorDamage,
}

/// What the interior mechanisms took (2026-11-03, the owner: every object inside stops or slows
/// fragments and is damaged by what it took, its function with it): the strike energy each
/// component met, summed over strikes, and what that does to the gearing. Thresholds are authored
/// (named): no measured damage-to-function data for these gears was found.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct InteriorDamage {
    /// (`components::COMPONENT_SPECS` index, joules met).
    pub(super) energy_j: Vec<(usize, f32)>,
}

/// A gearbox jams when its strikes have brought this much energy against it; below, it turns stiff
/// in proportion (authored).
pub(super) const TRAVERSE_GEAR_JAM_J: f32 = 3_000.0;
pub(super) const ELEVATION_GEAR_JAM_J: f32 = 1_500.0;
/// The stiffest a damaged gear train turns, short of jamming (share of its free rate; authored).
pub(super) const STIFF_FLOOR: f32 = 0.25;
/// A handwheel's strike counts this share against its gearbox: a bent shaft binds, a cracked rim
/// does not (authored).
pub(super) const HANDWHEEL_SHARE: f32 = 0.5;

impl InteriorDamage {
    fn spec(id: &str) -> Option<usize> {
        components::COMPONENT_SPECS.iter().position(|s| s.id == id)
    }

    pub(super) fn energy(&self, id: &str) -> f32 {
        Self::spec(id).map_or(0.0, |spec| self.energy_j.iter().filter(|(s, _)| *s == spec).map(|(_, e)| e).sum())
    }

    pub(super) fn record(&mut self, spec: usize, energy_j: f32) {
        if !(energy_j.is_finite() && energy_j > 0.0) {
            return;
        }
        match self.energy_j.iter_mut().find(|(s, _)| *s == spec) {
            Some(entry) => entry.1 += energy_j,
            None => self.energy_j.push((spec, energy_j)),
        }
    }

    fn train(&self, gearbox: &str, wheel: &str) -> f32 {
        self.energy(gearbox) + HANDWHEEL_SHARE * self.energy(wheel)
    }

    pub(super) fn traverse_jammed(&self) -> bool {
        self.train("traverse_gearbox", "traverse_flywheel") >= TRAVERSE_GEAR_JAM_J
    }

    /// The traverse's rate against its free rate (1 undamaged).
    pub(super) fn traverse_factor(&self) -> f32 {
        (1.0 - self.train("traverse_gearbox", "traverse_flywheel") / TRAVERSE_GEAR_JAM_J).clamp(STIFF_FLOOR, 1.0)
    }

    pub(super) fn elevation_jammed(&self) -> bool {
        self.train("elevation_gearbox", "elevation_handwheel") >= ELEVATION_GEAR_JAM_J
    }

    pub(super) fn elevation_factor(&self) -> f32 {
        (1.0 - self.train("elevation_gearbox", "elevation_handwheel") / ELEVATION_GEAR_JAM_J).clamp(STIFF_FLOOR, 1.0)
    }

    /// Whether a strike on `spec` counts (the mechanisms with a function or a floor to dent).
    pub(super) fn tracks(spec: usize) -> bool {
        matches!(components::COMPONENT_SPECS[spec].id, "traverse_gearbox" | "traverse_flywheel" | "traverse_pedal"
            | "elevation_gearbox" | "elevation_handwheel" | "basket_floor")
    }
}

/// Game time between a stopped round's strike and the commit of its plate solve (named: long
/// enough for the solve to finish on a worker on this machine; the scar meanwhile shows the
/// empirical geometry, and both consumers switch together at the commit).
pub(super) const FEA_STOP_COMMIT_S: f32 = 0.25;
/// Game time after a perforation at which its solved consequences (scar, fragments, crew, modules)
/// commit (`PendingPenetration`): above the plate and round solves' measured 230-330 ms on the
/// owner's machine, so the update does not wait for them there (named latency).
pub(super) const FEA_PENETRATION_COMMIT_S: f32 = 0.35;

/// Offline saved-camera films wait for the solve at the physical hit. The interactive grace
/// period is a scheduling allowance, not travel time inside the armour.
fn impact_commit_delay(interactive: f32) -> f32 {
    if super::crew_tissue::reference_solve() && std::env::var_os("KESSEL_FEA_RANGE_FILM").is_some() { 0.0 }
    else { interactive }
}


#[path = "game_fea_temporal.rs"]
mod temporal;
pub(super) use temporal::TemporalImpact as PendingPenetration;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct LiveImpactContext {
    pub arc: Arc, pub shooter: Vec3, pub entry_w: Vec3, pub shell_dir_w: Vec3,
    pub armor_part: ArmorPart, pub armor_local_point: Vec3,
    pub penetration_id: u64, pub seed: u32, pub event: crate::ballistics::ImpactEvent,
}

#[derive(Debug, Default)]
pub(super) struct FeaRuntime {
    pub bursts: Vec<temporal::LiveBurst>,
    pub fragment_shapes: std::collections::BTreeMap<u64, (f32, Vec<Vec3>)>,
    pub temporal_view: Option<temporal::TemporalView>,
    pub live_contexts: std::collections::BTreeMap<u64, LiveImpactContext>,
    pub contact_states: std::collections::BTreeMap<u64, FragmentDamageState>,
    pub live_births: std::collections::BTreeMap<u64, f32>,

    pub(super) lodged: Vec<LodgedRound>,
    pub(super) receipts: Vec<FeaChainReceipt>,
    pub(super) current: Option<ImpactSolve>,
    /// A stopped round's solve started this strike, waiting to be queued with its scar.
    pub(super) started: Option<(PlateJob, std::thread::JoinHandle<LatticeOutcome>)>,
    pub(super) pending: Vec<PendingPlate>,
    /// Perforations whose plate and round solves run on workers (`PendingPenetration`).
    pub(super) penetrations: Vec<PendingPenetration>,
    /// The hot-fragment pool slot of a steel chip in the crewman's hand.
    pub(super) carried_fragment: Option<usize>,
    next_id: u64,
    /// One fixed physical instance pool per `LODGED_SPECS` entry (allocated at build; empty when
    /// the game has no renderer pools, e.g. CPU tools).
    lodged_draw: Vec<usize>,
    /// Lodged rounds over a pool's capacity, already reported.
    lodged_overflow_logged: bool,
    /// The own solves of rounds that ricocheted (any vehicle's armour), by projectile id; each is
    /// joined at that round's next contact, so its result never depends on when the worker ends.
    pub(super) ricochet_solves: std::collections::BTreeMap<u64, std::thread::JoinHandle<Option<RicochetShot>>>,
    /// What the interior mechanisms took (`InteriorDamage`).
    pub(super) interior: InteriorDamage,
}

/// What a ricochet's own solve left of the round.
#[derive(Clone, Copy, Debug)]
pub(super) struct RicochetShot {
    pub(super) residual_fraction: f32,
    pub(super) tumble_rad_s: f32,
    pub(super) pieces: usize,
    pub(super) peak_temp_k: f32,
    pub(super) wall_ms: f32,
}

impl RicochetShot {
    /// Start a ricocheting round's solve on a worker (`None` for rounds without a kinetic body).
    pub(super) fn spawn(event: &crate::ballistics::ImpactEvent, seed: u32) -> Option<std::thread::JoinHandle<Option<Self>>> {
        let impact = crate::fea_damage::shot::impact_of(event, seed)?;
        Some(std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let out = crate::fea_damage::shot::Shot::with_cells(impact, if super::crew_tissue::reference_solve() { SHOT_CELLS_ACROSS } else { 3.0 })
                .solve_for(crate::fea_damage::shot::GAME_HORIZON);
            Some(Self { residual_fraction: (out.residual_kg / out.mass_kg.max(1e-6)).clamp(0.05, 1.0),
                tumble_rad_s: out.tumble_rad_s, pieces: out.pieces.len(), peak_temp_k: out.peak_temp_k,
                wall_ms: started.elapsed().as_secs_f32() * 1000.0 })
        }))
    }
}

impl FeaRuntime {
    pub(super) fn with_draw_pools(lodged_draw: Vec<usize>) -> Self {
        Self { lodged_draw, ..Self::default() }
    }

    /// A fresh Tiger: no rounds in its steel, no plate solves pending, no chip in hand. The draw
    /// pools stay allocated.
    pub(super) fn reset(&mut self) {
        let lodged_draw = std::mem::take(&mut self.lodged_draw);
        *self = Self::with_draw_pools(lodged_draw);
    }

    pub(super) fn export_world_state(&self) -> FeaSaved {
        FeaSaved {
            live_contexts:self.live_contexts.clone(),contact_states:self.contact_states.clone(),live_births:self.live_births.clone(),
            fragment_shapes:self.fragment_shapes.clone(),temporal:self.penetrations.iter().map(|p|p.checkpoint()).collect(),bursts:self.bursts.clone(),
            lodged: self.lodged.clone(),
            pending: self.pending.iter().map(|p| PendingPlateSaved { commit_at_s: p.commit_at_s,
                struck_at_s: p.struck_at_s, facing: p.facing, local_mm: p.local_mm, lodged_id: p.lodged_id,
                job: p.job }).collect(),
            carried_fragment: self.carried_fragment,
            next_id: self.next_id,
            interior: self.interior.clone(),
        }
    }

    /// Restore a saved state into a candidate game. Deferred solves start again from their saved
    /// problems and commit at their saved times.
    pub(super) fn import_world_state(&mut self, saved: FeaSaved) -> anyhow::Result<()> {
        anyhow::ensure!(saved.carried_fragment.is_none_or(|i| i < N_HOT_FRAGMENTS), "carried fragment slot out of range");
        anyhow::ensure!(saved.lodged.iter().all(|r| r.id < saved.next_id && r.tip_local.is_finite()
            && r.dir_local.is_finite() && r.depth_m.is_finite()), "invalid lodged round");
        anyhow::ensure!(saved.pending.iter().all(|p| p.commit_at_s.is_finite() && p.job.origin.is_finite()
            && p.lodged_id.is_none_or(|id| saved.lodged.iter().any(|r| r.id == id))), "invalid deferred plate solve");
        self.live_contexts=saved.live_contexts; self.contact_states=saved.contact_states;self.live_births=saved.live_births;
        self.fragment_shapes=saved.fragment_shapes;self.bursts=saved.bursts;
        self.penetrations=saved.temporal.into_iter().map(temporal::TemporalImpact::restore).collect::<anyhow::Result<_>>()?;
        self.temporal_view=self.penetrations.last().map(|p|p.view());
        self.lodged = saved.lodged;
        self.pending = saved.pending.into_iter().map(|p| PendingPlate { commit_at_s: p.commit_at_s,
            struck_at_s: p.struck_at_s, facing: p.facing, local_mm: p.local_mm, lodged_id: p.lodged_id,
            job: p.job, handle: p.job.spawn() }).collect();
        self.carried_fragment = saved.carried_fragment;
        self.next_id = saved.next_id;
        anyhow::ensure!(saved.interior.energy_j.iter().all(|(s, e)| *s < components::COMPONENT_SPECS.len() && e.is_finite()),
            "invalid interior damage");
        self.interior = saved.interior;
        self.receipts.clear();
        self.current = None;
        self.started = None;
        Ok(())
    }

    /// Receipts kept for diagnostics (oldest dropped first; lodged rounds are never dropped).
    pub(super) const MAX_RECEIPTS: usize = 32;
    fn push_receipt(&mut self, receipt: FeaChainReceipt) {
        if self.receipts.len() >= Self::MAX_RECEIPTS {
            self.receipts.remove(0);
        }
        self.receipts.push(receipt);
    }
}

/// A breach's fire is sought from this far before the contact (the CAD envelope's face can sit
/// just inside the drawn part) to this far past it along the contact's own path.
const BREACH_SEED_BACK_M: f32 = 0.05;
const BREACH_SEED_REACH_M: f32 = 0.75;

/// The module a component system belongs to, if the game has one for it.
pub(super) fn module_for_system(system: System) -> Option<ModuleKind> {
    match system {
        System::Engine => Some(ModuleKind::Engine),
        System::Fuel(_) => Some(ModuleKind::Fuel),
        System::Gearbox | System::Steering => Some(ModuleKind::Transmission),
        System::FinalDrive(Side::Left) => Some(ModuleKind::LeftTrack),
        System::FinalDrive(Side::Right) => Some(ModuleKind::RightTrack),
        System::Radio => Some(ModuleKind::Radio),
        System::HullMg => Some(ModuleKind::HullMg),
        System::MainGun => Some(ModuleKind::MainGun),
        System::CoaxMg => Some(ModuleKind::CoaxMg),
        System::GunnerSight => Some(ModuleKind::GunnerSight),
        System::TurretTraverse => Some(ModuleKind::TurretTraverse),
        // No live cooling circuit, air-filter state or seat state yet: recorded in the receipt.
        // No live cooling circuit, air-filter state or seat state yet: recorded in the receipt. The
        // elevation gearing and the basket floor act through `InteriorDamage` instead.
        System::AirFilters | System::Cooling(_) | System::Ammunition | System::Seat
            | System::Elevation | System::Basket => None,
    }
}


/// The shot line through a Tiger, hull frame, for a strike at `impact_world` travelling along
/// `dir_world`, with the far wall found by coming back along the line from outside to the first
/// source surface (classified by the same plate classes as the entry).
pub(super) fn shot_line(source: &surface_fire::CabinFire, models: &[Mat4; surface_fire::FRAMES], tank: &Tank,
    impact_world: Vec3, dir_world: Vec3, event: &crate::ballistics::ImpactEvent) -> chain::ShotLine {
    let hull = tank.hull_model();
    let inv = hull.inverse();
    let dir_world = dir_world.normalize_or_zero();
    let entry = inv.transform_point3(impact_world);
    let dir = inv.transform_vector3(dir_world).normalize_or_zero();
    let cos = event.impact_angle_deg.to_radians().cos().clamp(0.05, 1.0);
    let plate_path = event.plate.thickness_mm * 0.001 / cos;
    const REACH_M: f32 = 12.0;
    let far = impact_world + dir_world * REACH_M;
    let near = impact_world + dir_world * (plate_path + 0.02);
    let far_wall = projectile_armor_source::contact(source, models, tank,
        ProjectileVehicle::Player, crate::ballistics::ArmorScheme::TigerI, far, near)
        .and_then(|hit| {
            let ProjectileContactKind::Vehicle(armor) = hit.kind else { return None };
            let plate = armor.surface_plate?.plate;
            let s_outer = (hit.pos - impact_world).dot(dir_world);
            let normal_hull = inv.transform_vector3(armor.world_normal).normalize_or_zero();
            let cos_far = dir.dot(normal_hull).abs().clamp(0.05, 1.0);
            let s_inner = s_outer - plate.thickness_mm * 0.001 / cos_far;
            (s_inner > plate_path + 0.01).then_some(chain::FarWall {
                s_inner,
                plate_mm: plate.thickness_mm,
                material: crate::fea_damage::materials::MaterialId::RolledArmour,
                cos_obliquity: cos_far,
                normal: normal_hull,
            })
        });
    chain::ShotLine { entry, dir, plate_path_m: plate_path, far_wall, max_s: REACH_M }
}

/// The voxel lattice's problem for a strike: the plate frame (origin at the strike, x along the
/// shot's in-plane direction, y across, z into the plate; hull-local axes) and the prescribed
/// motion from the chain's first crossing. `None` for ricochets and shatters (named: their path out
/// of the plate is not prescribed to the lattice) and for non-kinetic rounds.
pub(super) fn lattice_problem(event: &crate::ballistics::ImpactEvent, line: &chain::ShotLine,
    chain_result: &chain::ChainResult, outward_normal_hull: Vec3, seed: u32) -> Option<(lattice::PlateImpact, [Vec3; 3])> {
    use crate::fea_damage::chain::Fate;
    let motion = chain_result.crossings.first().and_then(|c| match c.fate {
        Fate::Perforated | Fate::Burst => Some(lattice::Motion::Perforate { v_out: c.v_out }),
        Fate::Stopped { depth_m } => Some(lattice::Motion::Stop { depth_m }),
        _ => None,
    })?;
    let mut e3 = -outward_normal_hull.normalize_or_zero();
    if e3.dot(line.dir) < 0.0 {
        e3 = -e3;
    }
    let tangential = line.dir - e3 * line.dir.dot(e3);
    let e1 = tangential.try_normalize().unwrap_or_else(|| e3.any_orthonormal_vector());
    let e2 = e3.cross(e1);
    let obliquity = line.dir.dot(e3).clamp(-1.0, 1.0).acos().min(1.4);
    let shell = event.shell;
    let calibre = shell.caliber_mm * 0.001;
    // A sub-calibre core goes into the plate; the carrier is not modelled (named).
    let body_d = if shell.kind == crate::ballistics::ShellKind::Apcr { APCR_CORE_FRACTION * calibre } else { calibre };
    let material = match event.plate.material {
        crate::ballistics::ArmorMaterial::Cast => crate::fea_damage::materials::MaterialId::CastArmour,
        _ => crate::fea_damage::materials::MaterialId::RolledArmour,
    };
    Some((lattice::PlateImpact {
        material,
        thickness_m: event.plate.thickness_mm * 0.001,
        obliquity_rad: obliquity,
        projectile: lattice::ProjectileShape {
            mass_kg: shell.mass_kg,
            radius_m: 0.5 * body_d,
            nose_m: lodged::nose_length_calibres(shell.kind) * body_d,
        },
        v_in: event.striking_v,
        motion,
        seed,
    }, [e1, e2, e3]))
}

/// The penetration's fragment population from a solved lattice (`Game::fea_penetration_fragments`
/// and the `fea-fire-odds` count): every cluster that broke free on the inside of the plate (its
/// mass, centre, velocity and spin as solved), and, for a filled round that burst, its body's
/// steel (mass minus filler) partitioned into the existing authored size shape and speeds (named:
/// no fragmentation law). `parent` is the penetration frame's model.
pub(super) fn lattice_fragments(line: &chain::ShotLine, burst: bool, lat: &LatticeOutcome, hull: Mat4, parent: Mat4,
    frame: BloodFrame, penetration_id: u64, event: &crate::ballistics::ImpactEvent, seed: u32,
    shot: Option<&ShotStrike>) -> Vec<HotFragment> {
    let parent_inv = parent.inverse();
    let to_frame = |p_plate: Vec3| parent_inv.transform_point3(hull.transform_point3(lat.plate_to_hull(p_plate)));
    let dir_to_frame = |v: Vec3| parent_inv.transform_vector3(hull.transform_vector3(lat.plate_dir_to_hull(v)));
    let profile = fragment_shower_profile(true, event.behind_armor, event.shell.caliber_mm, event.residual_v,
        if event.shell.kind == crate::ballistics::ShellKind::Heat { 0.0 } else { event.shell.filler_g });
    let penetration_seed = seed ^ penetration_id as u32 ^ (penetration_id >> 32) as u32 ^ 0x5A11_5A11;
    let mut out: Vec<HotFragment> = Vec::new();
    let make = |index: usize, pos: Vec3, vel: Vec3, size: f32, spin: Vec3, temp_k: f32, solved: bool| {
        let s = penetration_seed.wrapping_add((index as u32).wrapping_mul(2_246_822_519));
        HotFragment {
            id: (penetration_id << 10) | index as u64,
            penetration_id,
            active: true,
            settled: false,
            origin: pos,
            prev_pos: pos,
            entered_compartment: fragment_inside_compartment(frame, pos, size * 0.52),
            audio_whizzed: false,
            frame,
            pos,
            vel,
            size,
            ang: Vec3::new(hash01(s ^ 0x11), hash01(s ^ 0x22), hash01(s ^ 0x33)) * std::f32::consts::TAU,
            spin,
            age: 0.0,
            // A body the lattice solved is steel that stays (it cools to the cold page); the
            // burst body keeps the authored life of its heat pages.
            life: if solved { f32::MAX } else { (profile.fragment_life * (0.82 + 0.30 * hash01(s ^ 0x19))).max(AUTHORED_FRAGMENT_LIFE_MIN_S) },
            bounces: 0,
            seed: s,
            temp_k,
            lodged: false,
            embed_m: 0.0, mass_kg: 0.0,cross_section_m2:0.0, tracer_share: 0.0, material_remaining_m:0.0,material_force_n:0.0,
        }
    };
    {
        let temps: Vec<f32> = lat.clusters.iter().map(|c| c.temperature_k).collect();
        if let (Some(lo), Some(hi)) = (temps.iter().cloned().reduce(f32::min), temps.iter().cloned().reduce(f32::max)) {
            println!("[fea] {} solved clusters at {lo:.0}-{hi:.0} K", temps.len());
        }
    }
    // Plate clusters on the inside, moving inward. Exterior ejecta leave as the cosmetic impact
    // sparks only (named): this pool is the interior's.
    for cluster in &lat.clusters {
        if out.len() >= N_HOT_FRAGMENTS {
            break;
        }
        if !cluster.leaves_inside(lat.thickness_m, lat.cell_mm * 0.001, lat.measured.perforated) {
            continue;
        }
        let index = out.len();
        out.push(make(index, to_frame(cluster.centre), dir_to_frame(cluster.velocity), cluster.size_m,
            dir_to_frame(cluster.angular_velocity), cluster.temperature_k.max(crate::armor_damage::AMBIENT_K + 1.0), true));
    }
    // Pieces of the round itself that came through the plate (its own solve), as they left the
    // back face, at their solved motion and heat.
    if let Some(shot) = shot {
        for piece in &shot.shrapnel {
            if out.len() >= N_HOT_FRAGMENTS {
                break;
            }
            let index = out.len();
            out.push(make(index, to_frame(piece.pos), dir_to_frame(piece.vel), piece.size_m, dir_to_frame(piece.spin),
                piece.temperature_k.max(crate::armor_damage::AMBIENT_K + 1.0), true));
        }
    }
    if burst {
        const STEEL: f32 = 7850.0;
        // The burst breaks the steel the round still has (its own solve's residual, when solved).
        let carried = shot.map_or(event.shell.mass_kg, |s| s.residual_kg);
        let body_mass = (carried - event.shell.filler_g * 0.001).max(0.0);
        let count = (N_HOT_FRAGMENTS - out.len()).min(512);
        if body_mass > 0.0 && count > 0 {
            let exit = lat.plate_to_hull(Vec3::ZERO) + line.dir * line.plate_path_m;
            let burst = parent_inv.transform_point3(hull.transform_point3(exit));
            let axis = parent_inv.transform_vector3(hull.transform_vector3(line.dir)).normalize_or_zero();
            let mut side = axis.cross(Vec3::Y).normalize_or_zero();
            if side.length_squared() < 0.5 {
                side = axis.cross(Vec3::X).normalize_or_zero();
            }
            let up = side.cross(axis).normalize_or_zero();
            let shape: Vec<f32> = (0..count).map(|i| {
                let s = penetration_seed ^ 0xB0D7_0000 ^ (i as u32).wrapping_mul(2_654_435_761);
                profile.min_size_m + (profile.max_size_m - profile.min_size_m) * hash01(s ^ 0xE1).powf(2.4)
            }).collect();
            let raw_mass: f32 = shape.iter().map(|s| STEEL * s * s * s).sum();
            let k = (body_mass / raw_mass.max(1e-12)).cbrt();
            for (i, base) in shape.iter().enumerate() {
                let s = penetration_seed ^ 0xB0D7_0000 ^ (i as u32).wrapping_mul(2_654_435_761);
                let radial = hash01(s ^ 0x51).sqrt();
                let angle = hash01(s ^ 0xA7) * std::f32::consts::TAU;
                let fan = (side * angle.cos() + up * angle.sin()) * radial * profile.cone_half_tan;
                let dir = (axis + fan).normalize_or_zero();
                let speed = profile.speed_min + (profile.speed_max - profile.speed_min) * hash01(s ^ 0xD3).sqrt();
                let spin = Vec3::new(hash01(s ^ 0x44) - 0.5, hash01(s ^ 0x55) - 0.5, hash01(s ^ 0x66) - 0.5) * 40.0;
                let index = out.len();
                // The burst body's temperature is not solved: it leaves at the authored
                // chip temperature and cools as a lumped body like the rest.
                out.push(make(index, burst, dir * speed, base * k, spin, AUTHORED_FRAGMENT_KELVIN, false));
            }
        }
    }
    out
}

/// Measure a solved lattice into the outcome the game consumes.
pub(super) fn lattice_outcome(solver: &mut lattice::Lattice, origin_hull: Vec3, axes_hull: [Vec3; 3], wall_ms: f32) -> LatticeOutcome {
    let geometry = solver.measure();
    let ledger = solver.ledger_now();
    let measured = crate::armor_damage::MeasuredScar {
        perforated: matches!(solver.impact.motion, lattice::Motion::Perforate { .. }),
        aperture_mm: geometry.aperture_mm,
        back_radius_mm: geometry.back_radius_mm,
        petals: geometry.petals,
        petal_mm: geometry.petal_mm,
        plug: geometry.plug,
        cavity_mm: geometry.cavity_mm,
        worked_radius_mm: geometry.worked_radius_mm,
        rim_kelvin: geometry.rim_kelvin,
    };
    let (samples, surface) = solver.rim_cooling_and_surface(&crate::armor_damage::RIM_CURVE_TIMES,
        crate::armor_damage::RIM_SURFACE_SAMPLE);
    let mut kelvin = [crate::armor_damage::AMBIENT_K; 10];
    for (k, v) in kelvin.iter_mut().zip(samples) {
        *k = v;
    }
    LatticeOutcome {
        measured,
        geometry,
        clusters: solver.clusters(),
        origin_hull,
        axes_hull,
        thickness_m: solver.impact.thickness_m,
        particles: solver.n,
        steps: solver.step,
        cell_mm: solver.h * 1000.0,
        wall_ms,
        ledger,
        grip_normal_n: solver.body_grip_normal_n(),
        rim_curve: crate::armor_damage::RimCurve { kelvin, ring_m: surface.ring_m, face_k: surface.face_k,
            bore_k: surface.bore_k },
    }
}

impl Game {
    /// Turret yaw and gun elevation as the component envelopes need them.
    pub(super) fn fea_articulation(&self) -> components::Articulation {
        let p = &self.player;
        components::Articulation {
            hull_to_turret: (Mat4::from_translation(p.parts.turret_pivot) * Mat4::from_rotation_y(p.turret_yaw)).inverse(),
            gun_elevation: p.gun_pitch,
        }
    }

    /// The shot line through the player's Tiger (`shot_line`) against the same armour-only source
    /// cache its shell contacts use.
    pub(super) fn fea_shot_line(&self, impact_world: Vec3, dir_world: Vec3,
        event: &crate::ballistics::ImpactEvent) -> chain::ShotLine {
        shot_line(&self.projectile_player_armor, &self.fire_surface_models(), &self.player, impact_world, dir_world, event)
    }

    /// Run the chain for a kinetic strike on the player's Tiger. `None` for HE and HEAT, whose
    /// terminals keep their existing rules.
    pub(super) fn fea_run_chain(&self, impact_world: Vec3, dir_world: Vec3,
        event: &crate::ballistics::ImpactEvent) -> Option<(chain::ShotLine, chain::ChainResult)> {
        self.fea_run_chain_with(impact_world, dir_world, event, 1.0)
    }

    /// `fea_run_chain` with the round keeping `residual_fraction` of its mass past the plate.
    pub(super) fn fea_run_chain_with(&self, impact_world: Vec3, dir_world: Vec3,
        event: &crate::ballistics::ImpactEvent, residual_fraction: f32) -> Option<(chain::ShotLine, chain::ChainResult)> {
        use crate::ballistics::ShellKind;
        if !matches!(event.shell.kind, ShellKind::Ap | ShellKind::Apcbc | ShellKind::Apcr) {
            return None;
        }
        let line = self.fea_shot_line(impact_world, dir_world, event);
        let hull_inv = self.player.hull_model().inverse();
        let rounds: Vec<chain::RoundBox> = self.live_ammunition_in_frame(BloodFrame::Hull).iter().map(|r| {
            chain::RoundBox { location_id: r.stable_id, center: r.center,
                axes: [r.rotation * Vec3::X, r.rotation * Vec3::Y, r.rotation * Vec3::Z], half: r.half_extents }
        }).collect();
        let fuel = self.tank_fuel_liters().unwrap_or(TIGER_FUEL_CAPACITY_L);
        let state = chain::VehicleState {
            articulation: self.fea_articulation(),
            fuel_fill: (fuel / TIGER_FUEL_CAPACITY_L).clamp(0.0, 1.0),
            gravity_hull: hull_inv.transform_vector3(Vec3::NEG_Y).normalize_or_zero(),
            live_rounds: &rounds,
        };
        let layers = chain::build_layers(&line, components::tiger_components(), &state);
        let result = chain::run_with_residual(event, &layers, crate::fea_damage::materials::AIR.density, residual_fraction);
        Some((line, result))
    }

    /// Bring a strike's interior strikes into the mechanisms' damage and report what it did to them.
    pub(super) fn fea_apply_part_strikes(&mut self, strikes: &[(usize, f32)]) {
        if strikes.is_empty() {
            return;
        }
        let (traverse_was, elevation_was) = (self.fea.interior.traverse_jammed(), self.fea.interior.elevation_jammed());
        for &(spec, energy) in strikes {
            self.fea.interior.record(spec, energy);
            println!("[fea] {} struck: {:.0} J ({:.0} J in all)", components::COMPONENT_SPECS[spec].name, energy,
                self.fea.interior.energy(components::COMPONENT_SPECS[spec].id));
        }
        let interior = &self.fea.interior;
        println!("[fea] interior gearing: traverse {:.0} %{}, elevation {:.0} %{}", interior.traverse_factor() * 100.0,
            if interior.traverse_jammed() { " (jammed)" } else { "" }, interior.elevation_factor() * 100.0,
            if interior.elevation_jammed() { " (jammed)" } else { "" });
        if interior.traverse_jammed() && !traverse_was {
            println!("[panzerrechnersystem] the traverse gearbox has jammed: the turret will not turn");
        }
        if interior.elevation_jammed() && !elevation_was {
            println!("[panzerrechnersystem] the elevation gearbox has jammed: the gun will not elevate");
        }
    }

    /// Record a round the chain left lodged. `part` is the struck armour part (the frame of a round
    /// held in the plate); a round stopped inside goes to the frame of the component holding it.
    fn fea_record_lodged(&mut self, line: &chain::ShotLine, result: &chain::ChainResult,
        event: &crate::ballistics::ImpactEvent, part: ArmorPart, now_s: f32, grip_normal_n: Option<f32>) -> Option<u64> {
        let lodging = result.lodged?;
        let crossing = result.crossings.get(lodging.crossing)?;
        let shell = event.shell;
        let d = shell.caliber_mm * 0.001;
        let nose = lodged::nose_length_calibres(shell.kind) * d;
        let tip_hull = line.entry + line.dir * lodging.tip_s;
        let component = crossing.layer.kind.component();
        let set = components::tiger_components();
        let component_frame = component.and_then(|c| set.envelopes.iter().find(|e| e.spec == c)).map(|e| e.frame);
        let frame = match (lodging.in_struck_plate, part, component_frame) {
            (true, ArmorPart::Turret, _) => BloodFrame::Turret,
            (true, ArmorPart::Hull, _) => BloodFrame::Hull,
            (false, _, Some(components::Frame::Turret | components::Frame::Gun)) => BloodFrame::Turret,
            _ => BloodFrame::Hull,
        };
        let on_gun = !lodging.in_struck_plate && component_frame == Some(components::Frame::Gun);
        let hull = self.player.hull_model();
        let parent = if on_gun { self.player.gun_model() } else { blood_parent_model(&self.player, frame) };
        let parent_inv = parent.inverse();
        let tip_local = parent_inv.transform_point3(hull.transform_point3(tip_hull));
        let dir_local = parent_inv.transform_vector3(hull.transform_vector3(line.dir)).normalize_or_zero();
        let yield_pa = match crossing.layer.kind {
            chain::LayerKind::Casing { material, .. } => material.get().yield_stress,
            chain::LayerKind::Interior { matrix, .. } => matrix.get().yield_stress,
            _ => crate::fea_damage::materials::ROLLED_ARMOUR.yield_stress,
        };
        // Held over the body length past its nose; inside machinery it is not reachable by hand.
        let held = (lodging.depth_m - nose).max(0.0);
        let reachable = lodging.in_struck_plate
            || matches!(crossing.layer.kind, chain::LayerKind::Casing { .. });
        // The lattice's measured grip on the body when it solved this plate; otherwise the named
        // stage-1 estimate.
        let extraction = match (lodging.in_struck_plate, grip_normal_n) {
            (true, Some(grip)) => lodged::extraction_from_grip(grip, held, reachable),
            _ => lodged::extraction(lodged::estimated_grip_pa(yield_pa), d, held, reachable),
        };
        let id = self.fea.next_id;
        self.fea.next_id += 1;
        let name = component.map(|c| components::COMPONENT_SPECS[c].id.to_string());
        println!("[fea] {} lodged {} ({:.0} mm in{}): {:.1} kN to start, {:?}", shell.name,
            if lodging.in_struck_plate { "in the struck plate".to_string() } else {
                format!("inside, in the {}", name.as_deref().unwrap_or("structure")) },
            lodging.depth_m * 1000.0, if reachable { "" } else { ", not reachable" },
            extraction.force_n / 1000.0, extraction.removal);
        self.fea.lodged.push(LodgedRound { id, shell, frame, tip_local, dir_local, depth_m: lodging.depth_m,
            in_struck_plate: lodging.in_struck_plate, component: name, extraction, struck_at_s: now_s, on_gun });
        Some(id)
    }

    /// A kinetic round the struck plate stopped: it may stay lodged in it.
    pub(super) fn fea_nonpenetration(&mut self, armor: &VehicleArmorContact,
        event: &crate::ballistics::ImpactEvent, impact_world: Vec3, dir_world: Vec3, now_s: f32) {
        let solved = self.fea.current.as_ref().map(|s| (s.line, s.chain.clone(), s.lattice.as_ref().map(|l| l.grip_normal_n)));
        let Some((line, result, grip)) = solved.or_else(|| self.fea_run_chain(impact_world, dir_world, event)
            .map(|(l, r)| (l, r, None))) else { return };
        let lodged_id = self.fea_record_lodged(&line, &result, event, armor.part, now_s, grip);
        // Queue this strike's deferred plate solve with the scar it will refine.
        if let Some((job, handle)) = self.fea.started.take() {
            self.fea.pending.push(PendingPlate {
                commit_at_s: now_s + impact_commit_delay(FEA_STOP_COMMIT_S),
                struck_at_s: now_s,
                facing: armor.facing,
                local_mm: crate::armor_damage::quantize_mm(armor.local_point),
                lodged_id,
                job,
                handle,
            });
        }
        self.fea.push_receipt(FeaChainReceipt { shell: event.shell.name, terminal: event.terminal,
            entry_hull: line.entry, dir_hull: line.dir, far_wall_mm: line.far_wall.map(|w| w.plate_mm),
            result, knocked_out: Vec::new(), struck_not_breached: Vec::new(), compartment: None });
    }

    /// The layered consequences of a perforation: the systems the residual round breached, and its
    /// own contacts with crew volumes and live cartridges in the penetration frame.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fea_penetration_consequences(&mut self, impact_world: Vec3, dir_world: Vec3,
        event: &crate::ballistics::ImpactEvent, part: ArmorPart, penetration_id: u64, frame: BloodFrame,
        solo_body: Option<Vec3>, couch_bodies: &[CouchCrewBody], live_ammunition: &[LiveAmmoRound],
        burst_fragment_kinetic_j: Option<f32>,
        result: &mut SpallResult, contacts: &mut Vec<FragmentDamageContact>) {
        let hole_area_m2 = self.fea.current.as_ref().filter(|s| s.projectile_id == penetration_id)
            .and_then(|s| s.lattice.as_ref()).map(|l| hole_area_m2(&l.geometry));
        let current = self.fea.current.as_ref().filter(|s| s.projectile_id == penetration_id)
            .map(|s| (s.line, s.chain.clone(), s.lattice.as_ref().map(|l| l.grip_normal_n)));
        let Some((line, chain_result, grip)) = current.or_else(|| self.fea_run_chain(impact_world, dir_world, event)
            .map(|(l, r)| (l, r, None))) else { return };
        let hull = self.player.hull_model();
        let parent_inv = blood_parent_model(&self.player, frame).inverse();
        let to_frame = |p_hull: Vec3| parent_inv.transform_point3(hull.transform_point3(p_hull));
        let dir_frame = parent_inv.transform_vector3(hull.transform_vector3(line.dir)).normalize_or_zero();
        let shell = event.shell;
        // The round flies on at the mass its own solve left it (the whole round when unsolved).
        let mass = self.fea.current.as_ref().filter(|s| s.projectile_id == penetration_id)
            .and_then(|s| s.shot.as_ref()).map_or(shell.mass_kg, |s| s.residual_kg);
        let radius = shell.caliber_mm * 0.0005;
        let contact_id = penetrator_contact_id(penetration_id);
        let mut knocked_out = Vec::new();
        let mut struck_not_breached = Vec::new();

        // Systems breached by the round itself.
        for hit in &chain_result.hits {
            if InteriorDamage::tracks(hit.component) {
                if let Some(first) = chain_result.crossings.iter().find(|c| c.layer.kind.component() == Some(hit.component)) {
                    result.part_strikes.push((hit.component, 0.5 * mass * first.v_in * first.v_in));
                }
            }
            let spec = &components::COMPONENT_SPECS[hit.component];
            if !hit.breached {
                struck_not_breached.push(spec.id.to_string());
                continue;
            }
            // Where it entered this component, at the speed it had there.
            let Some(first) = chain_result.crossings.iter().find(|c| c.layer.kind.component() == Some(hit.component)) else { continue };
            // What burns in it (`fea_damage::ignition`): the round's entry. Its own heat is not
            // solved, so it lights by its sparks alone (named); the strike's trials follow
            // (`SpallResult::ignite`).
            if let Some(what) = crate::fea_damage::ignition::flammable(spec) {
                result.enter_flammable(StrikeIgnition { local_point: to_frame(line.entry + line.dir * first.layer.s0),
                    local_velocity: dir_frame * first.v_in, component: hit.component, what, by_round: true, body_k: 0.0 });
            }
            let Some(kind) = module_for_system(spec.system) else { continue };
            let Some(module) = self.modules.iter_mut().find(|m| m.kind == kind) else { continue };
            if module.functional {
                module.functional = false;
                module.fault = tiger_damage::Fault::Spall;
            }
            if !result.hit_modules.contains(&kind) {
                result.hit_modules.push(kind);
            }
            knocked_out.push(kind);
            let p = to_frame(line.entry + line.dir * first.layer.s0);
            contacts.push(FragmentDamageContact { fragment_id: contact_id, target: FragmentDamageTarget::Module(kind),
                body_part: None, local_point: p, local_velocity: dir_frame * first.v_in, speed_mps: first.v_in,
                energy_j: 0.5 * mass * first.v_in * first.v_in, internal_blast_origin_local: None });
        }

        // The penetrator's straight path, crossing by crossing, against crew and live rounds.
        let mut contacted = vec![0u16; couch_bodies.len()];
        for crossing in chain_result.crossings.iter().skip(1) {
            if crossing.v_in <= 0.0 {
                break;
            }
            let end_s = match crossing.fate {
                chain::Fate::Stopped { depth_m } => crossing.layer.s0 + depth_m,
                _ => crossing.layer.s1,
            };
            let from = to_frame(line.entry + line.dir * crossing.layer.s0);
            let to = to_frame(line.entry + line.dir * end_s);
            let speed = crossing.v_in;
            let energy = 0.5 * mass * speed * speed;
            if let chain::LayerKind::LiveRound { location_id } = crossing.layer.kind {
                if !result.ammo {
                    if let Some(round) = live_ammunition.iter().find(|r| r.stable_id == location_id) {
                        if let Some(module) = self.modules.iter_mut().find(|m| m.kind == ModuleKind::AmmoRack) {
                            module.functional = false;
                            module.fault = tiger_damage::Fault::Spall;
                        }
                        if !result.hit_modules.contains(&ModuleKind::AmmoRack) {
                            result.hit_modules.push(ModuleKind::AmmoRack);
                        }
                        result.ammo = true;
                        contacts.push(FragmentDamageContact { fragment_id: contact_id,
                            target: FragmentDamageTarget::Module(ModuleKind::AmmoRack), body_part: None,
                            local_point: from, local_velocity: dir_frame * speed, speed_mps: speed, energy_j: energy,
                            internal_blast_origin_local: Some(round.center) });
                    }
                }
            }
            if let Some(center) = solo_body {
                if !result.hit_body {
                    if let Some(f) = swept_segment_sphere_fraction(from, to, center, 0.35 + radius) {
                        result.hit_body = true;
                        contacts.push(FragmentDamageContact { fragment_id: contact_id, target: FragmentDamageTarget::SoloBody,
                            body_part: None, local_point: from.lerp(to, f), local_velocity: dir_frame * speed,
                            speed_mps: speed, energy_j: energy, internal_blast_origin_local: None });
                    }
                }
            }
            for (index, body) in couch_bodies.iter().enumerate() {
                if swept_segment_sphere_fraction(from, to, body.pos, 1.05 + radius).is_none() {
                    continue;
                }
                let mut hits: Vec<(f32, crew_physics::CrewBodyPart)> = body.hit_volumes.iter().filter_map(|volume| {
                    let bit = 1u16 << volume.part.index();
                    if contacted[index] & bit != 0 { return None; }
                    crew_damage::swept_sphere_obb_fraction(from, to, radius, *volume).map(|f| (f, volume.part))
                }).collect();
                hits.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.index().cmp(&b.1.index())));
                for (f, part) in hits {
                    contacted[index] |= 1u16 << part.index();
                    contacts.push(FragmentDamageContact { fragment_id: contact_id,
                        target: FragmentDamageTarget::CouchCrew(body.casualty), body_part: Some(part),
                        local_point: from.lerp(to, f), local_velocity: dir_frame * speed, speed_mps: speed,
                        energy_j: energy, internal_blast_origin_local: None });
                }
            }
        }
        let now = self.armor_clock_s;
        self.fea_record_lodged(&line, &chain_result, event, part, now, grip);
        // Without a lattice solve the hole is the calibre's (named).
        let hole = hole_area_m2.unwrap_or(std::f32::consts::PI * radius * radius);
        // A hole into the crew compartments vents them in a cook-off (`game_cookoff`).
        if matches!(event.terminal, crate::ballistics::Terminal::Penetration) && in_crew_air(part, line.entry) {
            self.cookoff.holes_m2 += hole;
        }
        self.fea_breach_effects(&line, &chain_result, hole);
        let compartment = if chain_result.burst {
            self.fea_compartment_burst(&line, &chain_result, event, part, hole, burst_fragment_kinetic_j)
        } else { None };
        println!("[fea] {}: {}", shell.name, chain_result.summary());
        if !knocked_out.is_empty() || !struck_not_breached.is_empty() {
            println!("[fea]   breached {:?}; struck, not breached {:?}", knocked_out, struck_not_breached);
        }
        self.fea.push_receipt(FeaChainReceipt { shell: shell.name, terminal: event.terminal, entry_hull: line.entry,
            dir_hull: line.dir, far_wall_mm: line.far_wall.map(|w| w.plate_mm), result: chain_result,
            knocked_out, struck_not_breached, compartment });
    }

    pub(super) fn fea_crew_openings(&self, hole_area_m2: f32) -> Vec<compartment::Opening> {
        crew_openings(self.roof_hatches.phase, self.hatch_t, hole_area_m2)
    }

    /// Light the surface fire where the strike lit what a component holds (`fea_damage::ignition`):
    /// the lighting contact (the round's own crossing or a fragment's, at `local_point` moving with
    /// `local_velocity` in the frame `parent` maps to world), swept on along its velocity to the
    /// first source surface it reaches. Returns that point, hull frame. None leaves the fire to
    /// `update_fire`'s standing first spot.
    pub(super) fn fea_seed_fire_at(&mut self, parent: Mat4, local_point: Vec3, local_velocity: Vec3,
        strength: f32) -> Option<Vec3> {
        let dir = local_velocity.normalize_or_zero();
        if dir == Vec3::ZERO {
            return None;
        }
        let models = self.fire_surface_models();
        let from = parent.transform_point3(local_point - dir * BREACH_SEED_BACK_M);
        let to = parent.transform_point3(local_point + dir * BREACH_SEED_REACH_M);
        let hit = self.cabin_fire.sweep(from, to, &models)?;
        if !self.cabin_fire.seed_from_segment(from, to, &models, strength) {
            return None;
        }
        Some(self.player.hull_model().inverse().transform_point3(models[hit.frame].transform_point3(hit.point)))
    }

    /// A filled round burst at the struck plate's back face. If that point is in the crew
    /// compartments' air (`in_crew_air`), the burst's energy less what its own fragments carry
    /// away raises their pressure (`compartment::burst`), the openings vent it, and a closed lid
    /// whose latch the pressure beats is thrown open. A burst in the engine bay is given no
    /// pressure history (named). The lid then swings at its ordinary rate (named).
    fn fea_compartment_burst(&mut self, line: &chain::ShotLine, result: &chain::ChainResult,
        event: &crate::ballistics::ImpactEvent, part: ArmorPart, hole_area_m2: f32,
        burst_fragment_kinetic_j: Option<f32>) -> Option<CompartmentReceipt> {
        let plate = result.crossings.first()?;
        let burst_hull = line.entry + line.dir * plate.layer.s1;
        if !in_crew_air(part, burst_hull) {
            println!("[fea] the {} burst in the engine bay: no crew compartment pressure (named)", event.shell.name);
            return None;
        }
        // The pressure solve alone does not draw gas. Emit the existing interior-only blast
        // at the same event and position; its fragments still come solely from the solver.
        let gas_radius = (0.45 * (event.shell.filler_g.max(1.0) / 90.0).cbrt()).clamp(0.18, 1.15);
        self.weapon_vfx.interior_blast(self.player.hull_model().transform_point3(burst_hull), gas_radius);
        let openings = self.fea_crew_openings(hole_area_m2);
        // The authored shower (no lattice) is not a partition of the round's body: nothing is
        // taken from the filler for it (named).
        let fragment_kinetic_j = burst_fragment_kinetic_j.unwrap_or(0.0);
        let history = compartment::burst(event.shell.filler_g * 0.001, compartment::TNT_J_PER_KG, fragment_kinetic_j,
            compartment::TIGER_CREW_AIR_M3, crate::fea_damage::materials::AMBIENT_K, &openings,
            compartment::GAME_DT_S, compartment::GAME_DURATION_S);
        for (name, t) in &history.hatches_blown {
            match openings.iter().position(|o| o.name == *name) {
                Some(i) if i < 4 => self.roof_hatches.open[i] = true,
                Some(CUPOLA_OPENING) => self.hatch_open = true,
                _ => {}
            }
            println!("[fea] the burst threw the {name} open at {:.1} ms", t * 1000.0);
        }
        let vent_area_m2 = openings.iter().filter(|o| o.open).map(|o| o.area_m2).sum();
        println!("[fea] {} burst in the crew compartments: {:.0} kJ into {:.0} m3 of air, peak {:.1} kPa, venting through {:.3} m2",
            event.shell.name, history.energy_into_air_j / 1000.0, compartment::TIGER_CREW_AIR_M3,
            history.peak_gauge_pa / 1000.0, vent_area_m2);
        Some(CompartmentReceipt { peak_gauge_pa: history.peak_gauge_pa, energy_into_air_j: history.energy_into_air_j,
            fragment_kinetic_j, vent_area_m2, hatches_blown: history.hatches_blown })
    }

    /// Solve one kinetic strike on the player's Tiger: the chain, then the voxel lattice of the
    /// struck plate. The lattice runs to completion inside this tick: a long solve holds the tick
    /// (logged) rather than letting any consumer see a half-solved plate or skip the contact.
    pub(super) fn fea_solve_impact(&mut self, projectile_id: u64, seed: u32, armor: &VehicleArmorContact,
        event: &crate::ballistics::ImpactEvent, impact_world: Vec3, dir_world: Vec3) {
        self.fea.current = None;
        // A perforating round's own solve runs beside the plate's; the chain takes its residual
        // mass (the plate's problem depends only on the struck plate's calibrated crossing).
        let shot_event = *event;
        let shot_seed = seed ^ 0x5407_5407;
        let shot_handle = (event.terminal == crate::ballistics::Terminal::Penetration)
            .then(|| std::thread::spawn(move || ShotStrike::solve(&shot_event, shot_seed)));
        let Some((line, chain_result)) = self.fea_run_chain(impact_world, dir_world, event) else { return };
        let hull_inv = self.player.hull_model().inverse();
        let normal_hull = hull_inv.transform_vector3(armor.world_normal).normalize_or_zero();
        let local_seed = crate::armor_damage::position_seed(armor.local_point) ^ seed.rotate_left(13);
        let problem = lattice_problem(event, &line, &chain_result, normal_hull, local_seed);
        // A round the plate stopped only changes its scar and grip: solve it on a worker and commit
        // it a fixed game time later. A perforation's fragments are needed now: solve it here.
        let job = problem.map(|(impact, axes)| PlateJob { impact, origin: line.entry, axes });
        if let Some(job) = job.filter(|j| matches!(j.impact.motion, lattice::Motion::Stop { .. })) {
            self.fea.started = Some((job, job.spawn()));
            self.fea.current = Some(ImpactSolve { projectile_id, line, chain: chain_result, lattice: None, shot: None });
            return;
        }
        let lattice = job.map(|job| {
            let outcome = job.solve();
            let g = outcome.geometry;
            println!("[fea] lattice {} particles, {:.1} mm cells, {} steps, {:.0} ms (the tick waited for it): aperture {:.0} mm, petals {}, plug {}, lip {:.0} mm, bulge {:.0} mm, rim {:.0} K, energy closure {:.2} %",
                outcome.particles, outcome.cell_mm, outcome.steps, outcome.wall_ms, g.aperture_mm, g.petals, g.plug,
                g.lip_mm, g.bulge_mm, g.rim_kelvin, outcome.ledger.closure() * 100.0);
            outcome
        });
        let shot = shot_handle.and_then(|h| h.join().ok().flatten());
        let chain_result = match &shot {
            Some(s) => {
                println!("[fea] the round's own solve: {:.2} of {:.2} kg flies on, {} pieces, {} through the plate as shrapnel, {:.0} ms",
                    s.residual_kg, s.mass_kg, s.pieces, s.shrapnel.len(), s.wall_ms);
                self.fea_run_chain_with(impact_world, dir_world, event, s.residual_fraction()).map_or(chain_result, |(_, r)| r)
            }
            None => chain_result,
        };
        self.fea.current = Some(ImpactSolve { projectile_id, line, chain: chain_result, lattice, shot });
    }

    /// The struck part's model (hull or turret) as it stands now.
    pub(super) fn part_model(&self, part: ArmorPart) -> Mat4 {
        match part { ArmorPart::Hull => self.player.hull_model(), ArmorPart::Turret => self.player.turret_model() }
    }

    /// The measured terminal geometry and solved rim cooling for the strike being resolved, if
    /// the lattice solved it.
    pub(super) fn fea_measured_scar(&self) -> Option<(crate::armor_damage::MeasuredScar, Option<crate::armor_damage::RimCurve>)> {
        self.fea.current.as_ref()?.lattice.as_ref().map(|l| (l.measured, Some(l.rim_curve)))
    }

    /// The penetration's fragment population from the lattice: every cluster that broke free on
    /// the inside of the plate (its mass, centre, velocity and spin as solved), and, for a filled
    /// round that burst, its body's steel (mass minus filler) partitioned into the existing authored
    /// size shape and speeds (named: no fragmentation law). `None` when no lattice solved it.
    pub(super) fn fea_penetration_fragments(&self, penetration_id: u64, frame: BloodFrame,
        event: &crate::ballistics::ImpactEvent, seed: u32) -> Option<Vec<HotFragment>> {
        let solve = self.fea.current.as_ref().filter(|s| s.projectile_id == penetration_id)?;
        let lat = solve.lattice.as_ref()?;
        Some(lattice_fragments(&solve.line, solve.chain.burst, lat, self.player.hull_model(),
            blood_parent_model(&self.player, frame), frame, penetration_id, event, seed, solve.shot.as_ref()))
    }

    /// Commit every deferred plate solve whose time has come (called at the start of each update).
    /// A solve still running at its commit time is waited for: the update holds rather than letting
    /// the scar and the grip switch at a machine-dependent moment.
    pub(super) fn fea_commit_pending(&mut self) {
        if self.fea.pending.is_empty() {
            return;
        }
        let now = self.armor_clock_s;
        let mut index = 0;
        while index < self.fea.pending.len() {
            if self.fea.pending[index].commit_at_s > now
                || (!super::crew_tissue::reference_solve() && !self.fea.pending[index].handle.is_finished()) {
                index += 1;
                continue;
            }
            let pending = self.fea.pending.remove(index);
            let waited = std::time::Instant::now();
            let finished = pending.handle.is_finished();
            let Ok(outcome) = pending.handle.join() else {
                eprintln!("[fea] a deferred plate solve failed; the empirical scar stays");
                continue;
            };
            if !finished {
                println!("[fea] the update waited {:.0} ms for a deferred plate solve (pending rule)",
                    waited.elapsed().as_secs_f32() * 1000.0);
            }
            let updated = self.armor_scars.apply_measured(pending.facing, pending.local_mm, &outcome.measured, Some(outcome.rim_curve));
            if let Some(id) = pending.lodged_id {
                if let Some(round) = self.fea.lodged.iter_mut().find(|r| r.id == id) {
                    let d = round.shell.caliber_mm * 0.001;
                    let held = (round.depth_m - lodged::nose_length_calibres(round.shell.kind) * d).max(0.0);
                    round.extraction = lodged::extraction_from_grip(outcome.grip_normal_n, held, true);
                }
            }
            println!("[fea] committed the plate solve of a stopped round struck at {:.2} s ({} particles, {:.0} ms on a worker): cavity {:.0} mm, rim {:.0} K{}",
                pending.struck_at_s, outcome.particles, outcome.wall_ms, outcome.geometry.cavity_mm, outcome.geometry.rim_kelvin,
                if updated { "" } else { " (the strike had merged into an earlier wound: that record stays)" });
        }
    }

    /// The resting steel fragment under the crosshair within reach, with the prompt it earns.
    pub(super) fn fea_fragment_hover(&self, eye: Vec3, look: Vec3) -> Option<(usize, f32, &'static str)> {
        let look = look.normalize_or_zero();
        let mut best: Option<(usize, f32, &'static str)> = None;
        for (i, f) in self.hot_fragments.iter().enumerate() {
            if !f.active || !f.settled || Some(i) == self.fea.carried_fragment {
                continue;
            }
            let centre = blood_parent_model(&self.player, f.frame).transform_point3(f.pos);
            let radius = (f.size * 0.8).max(FRAGMENT_PICK_RADIUS_M);
            let Some(distance) = ray_sphere(eye, look, centre, radius) else { continue };
            if distance > FRAGMENT_REACH_M || best.is_some_and(|b| b.1 <= distance) {
                continue;
            }
            // The hand reaches only what the eye sees: no steel between them (the toolbox rule).
            if !self.equipment_path_clear(eye, eye + look * distance, radius + 0.03) {
                continue;
            }
            let label = if !fragment_holdable(f) {
                "STEEL FRAGMENT - TOO HOT TO HOLD"
            } else if !fragment_pullable(f) {
                "STEEL FRAGMENT - STUCK FAST: PRY IT OUT WITH THE TOOLBOX"
            } else {
                "PICK UP STEEL FRAGMENT [LMB]"
            };
            best = Some((i, distance, label));
        }
        best
    }

    /// Take a resting fragment in hand, if it is cool enough to hold and not held fast.
    pub(super) fn fea_grab_fragment(&mut self, index: usize) -> bool {
        let Some(f) = self.hot_fragments.get(index).copied() else { return false };
        if !f.active || !f.settled || !fragment_holdable(&f) || !fragment_pullable(&f) {
            return false;
        }
        let world = blood_parent_model(&self.player, f.frame).transform_point3(f.pos);
        self.set_carry_pose(world, Quat::IDENTITY, Vec3::ZERO, Vec3::ZERO);
        self.fea.carried_fragment = Some(index);
        println!("[fea] a {:.0} g steel fragment taken in hand{}", fragments::cube_mass(f.size) * 1000.0,
            if f.lodged { " (pulled from the steel)" } else { "" });
        true
    }

    /// The fragment in hand follows the hand's spring (it is not integrated while held).
    pub(super) fn fea_follow_carried_fragment(&mut self) {
        let Some(index) = self.fea.carried_fragment else { return };
        if !self.hot_fragments.get(index).is_some_and(|f| f.active) {
            self.fea.carried_fragment = None;
            return;
        }
        let parent_inv = blood_parent_model(&self.player, self.hot_fragments[index].frame).inverse();
        let local = parent_inv.transform_point3(self.carry_pos);
        let f = &mut self.hot_fragments[index];
        f.pos = local;
        f.prev_pos = local;
        f.vel = Vec3::ZERO;
        f.settled = true;
        f.lodged = false;
        f.embed_m = 0.0;
    }

    /// Let go of the fragment in hand (`throw` = the look direction for R). A throw whose line from
    /// the hand leaves the vehicle without meeting a source surface goes out through an open
    /// aperture and leaves the simulation (named: steel outside is not kept); otherwise the chip
    /// flies inside and comes to rest under the ordinary fragment collision.
    pub(super) fn fea_release_fragment(&mut self, throw: Option<Vec3>) -> bool {
        let Some(index) = self.fea.carried_fragment.take() else { return false };
        if !self.hot_fragments.get(index).is_some_and(|f| f.active) {
            return false;
        }
        let frame = self.hot_fragments[index].frame;
        let parent_inv = blood_parent_model(&self.player, frame).inverse();
        let mut vel_world = self.carry_vel;
        if let Some(forward) = throw {
            vel_world += forward.normalize_or_zero() * FRAGMENT_THROW_SPEED + Vec3::Y * 0.8;
            let from = self.carry_pos;
            let to = from + vel_world.normalize_or_zero() * 3.0;
            let models = self.fire_surface_models();
            if self.cabin_fire.sweep(from, to, &models).is_none() {
                self.hot_fragments[index].active = false;
                println!("[fea] a steel fragment thrown out of the tank");
                return true;
            }
        }
        let f = &mut self.hot_fragments[index];
        f.settled = false;
        f.vel = parent_inv.transform_vector3(vel_world);
        f.entered_compartment = true;
        f.bounces = 0;
        true
    }
}

/// A sub-calibre round's core diameter as a fraction of its calibre (named, shared by the plate
/// solve and the drawn core: no source in this repository gives the cores' dimensions).
pub(super) const APCR_CORE_FRACTION: f32 = 0.37;
/// Drawn length of an APCR core in core diameters (named, unsourced authored shape). The length
/// of a body never enters the chain or the plate solve; for full-calibre shot it follows from the
/// round's mass, but an APCR round's mass is mostly its carrier, which is not modelled.
const APCR_CORE_LENGTH_DIAMETERS: f32 = 3.0;

/// The body the damage model sends into the steel, as drawn when it lodges: the plate solve's
/// diameter and ellipsoidal nose. A full-calibre round's cylinder holds its mass as solid steel
/// (named: no cavity, filler, cap, windshield or driving band); an APCR round is its core alone,
/// `APCR_CORE_LENGTH_DIAMETERS` long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PenetratorBody {
    pub(super) radius_m: f32,
    pub(super) nose_m: f32,
    pub(super) length_m: f32,
}

/// The same rest lattice used at contact, with the nose face at the flight point.
/// APCR includes its full-calibre carrier. It is not a bare APFSDS-style dart.
fn flight_lattice(shell:&crate::ballistics::ShellSpec)->Option<shot::Shot> {
    if shell.kind==crate::ballistics::ShellKind::Heat {return None;}
    let design=shot::design(shell)?;
    Some(shot::Shot::with_cells(shot::ShotImpact{design,v_in:shell.muzzle_velocity,
        obliquity_rad:0.,thickness_m:0.1,resistance_pa:1.0e9,seed:0},
        if super::crew_tissue::reference_solve(){SHOT_CELLS_ACROSS}else{3.}))
}

pub(super) fn penetrator_body(shell: &crate::ballistics::ShellSpec) -> PenetratorBody {
    static CACHE:std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String,PenetratorBody>>>=std::sync::OnceLock::new();
    let mut cache=CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(body)=cache.get(shell.name){return *body;}
    let body=if let Some(shot)=flight_lattice(shell) {
        PenetratorBody{radius_m:shot.impact.design.calibre_m*0.5,
            nose_m:shot.impact.design.nose_calibres*shot.impact.design.calibre_m,
            length_m:shot.length_m}
    }else{
        let radius=shell.caliber_mm*0.0005;
        PenetratorBody{radius_m:radius,nose_m:radius*2.,length_m:shell.mass_kg/(7850.*std::f32::consts::PI*radius*radius)+radius}
    };
    cache.insert(shell.name.to_owned(),body);body
}

fn flight_body_mesh(shell:&crate::ballistics::ShellSpec)->CpuMesh {
    let Some(shot)=flight_lattice(shell) else{return penetrator_mesh(penetrator_body(shell));};
    let key=|p:Vec3|(p/shot.h).round().as_ivec3().to_array();
    let occupied:std::collections::HashSet<_>=shot.pos.iter().map(|&p|key(p)).collect();
    let axes=[Vec3::X,Vec3::NEG_X,Vec3::Y,Vec3::NEG_Y,Vec3::Z,Vec3::NEG_Z];
    let mut vertices=Vec::new();let mut indices=Vec::new();
    let mut lo=Vec3::splat(f32::INFINITY);let mut hi=Vec3::splat(f32::NEG_INFINITY);
    for &p in &shot.pos {
        let half=Vec3::splat(shot.h*0.5);lo=lo.min(p-half);hi=hi.max(p+half);
        let cube=crate::prims::box_mesh(p-half,p+half,[102,106,110]);
        for (face,axis) in axes.iter().enumerate() {
            if occupied.contains(&key(p+*axis*shot.h)){continue;}
            let base=vertices.len()as u32;vertices.extend_from_slice(&cube.vertices[face*4..face*4+4]);
            indices.extend([base,base+1,base+2,base,base+2,base+3]);
        }
    }
    let mut mesh=CpuMesh::basic(vertices,indices,Some(crate::prims::solid_color([102,106,110])),lo,hi);
    mesh.metallic_factor=0.85;mesh.roughness_factor=0.55;mesh
}

/// Kinetic rounds that can lodge in the Tiger (every kinetic specification the game fires), each
/// drawn from its own exact prototype.
pub(super) static LODGED_SPECS: [&crate::ballistics::ShellSpec; 16] = [
    &crate::ballistics::PZGR_39, &crate::ballistics::PZGR_40, &crate::ballistics::PZGR_39_43,
    &crate::ballistics::BR_240_M42, &crate::ballistics::BR_240, &crate::ballistics::BR_240P,
    &crate::ballistics::BR_271_ZIS2, &crate::ballistics::BR_350B, &crate::ballistics::BR_354P,
    &crate::ballistics::BR_365, &crate::ballistics::BR_412, &crate::ballistics::BR_471,
    &crate::ballistics::BR_540,&crate::ballistics::HEAT_REVIEW,&crate::ballistics::HESH_REVIEW,&crate::ballistics::M829A2_REVIEW,
];
/// Lodged rounds of one specification drawn at once (named bound: further ones stay in the
/// state and stay removable, but are not drawn; logged once).
pub(super) const LODGED_DRAW_CAPACITY: usize = 6;

fn lodged_spec_index(shell: &crate::ballistics::ShellSpec) -> Option<usize> {
    LODGED_SPECS.iter().position(|s| s.name == shell.name)
}

/// The drawn round: a 12-sided lathe, nose tip at the origin pointing +Z, base at -length.
/// Bare steel (the shells' paint is not modelled).
pub(super) fn penetrator_mesh(body: PenetratorBody) -> CpuMesh {
    const SIDES: usize = 12;
    const NOSE_RINGS: usize = 5;
    use crate::gltf_load::Vertex;
    let (r, n, l) = (body.radius_m, body.nose_m.max(1e-4), body.length_m.max(body.nose_m));
    // Profile rings from the tip: (radius, z, radial normal weight, axial normal weight).
    let mut rings: Vec<(f32, f32, f32, f32)> = Vec::new();
    for k in 1..=NOSE_RINGS {
        let phi = std::f32::consts::FRAC_PI_2 * k as f32 / NOSE_RINGS as f32;
        let rho = r * phi.sin();
        let z = -n * (1.0 - phi.cos());
        // Ellipse (rho/r)^2 + ((z+n)/n)^2 = 1: its gradient is the outward normal.
        let (nr, nz) = (rho / (r * r), (z + n) / (n * n));
        let len = (nr * nr + nz * nz).sqrt().max(1e-9);
        rings.push((rho, z, nr / len, nz / len));
    }
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let dir = |side: usize| {
        let a = std::f32::consts::TAU * side as f32 / SIDES as f32;
        (a.cos(), a.sin())
    };
    let push = |p: Vec3, nrm: Vec3, vertices: &mut Vec<Vertex>| {
        let t = nrm.cross(if nrm.x.abs() < 0.9 { Vec3::X } else { Vec3::Y }).normalize_or_zero();
        vertices.push(Vertex { pos: p.to_array(), normal: nrm.to_array(), uv: [0.5, 0.5], tangent: [t.x, t.y, t.z, 1.0] });
        (vertices.len() - 1) as u32
    };
    // Tip fan.
    let tip = push(Vec3::ZERO, Vec3::Z, &mut vertices);
    let mut previous: Vec<u32> = (0..SIDES).map(|side| {
        let (c, s) = dir(side);
        let (rho, z, nr, nz) = rings[0];
        push(Vec3::new(c * rho, s * rho, z), Vec3::new(c * nr, s * nr, nz), &mut vertices)
    }).collect();
    for side in 0..SIDES {
        indices.extend_from_slice(&[tip, previous[side], previous[(side + 1) % SIDES]]);
    }
    let band = |from: &[u32], to: &[u32], indices: &mut Vec<u32>| {
        for side in 0..SIDES {
            let next = (side + 1) % SIDES;
            indices.extend_from_slice(&[from[side], to[side], to[next], from[side], to[next], from[next]]);
        }
    };
    for &(rho, z, nr, nz) in &rings[1..] {
        let ring: Vec<u32> = (0..SIDES).map(|side| {
            let (c, s) = dir(side);
            push(Vec3::new(c * rho, s * rho, z), Vec3::new(c * nr, s * nr, nz), &mut vertices)
        }).collect();
        band(&previous, &ring, &mut indices);
        previous = ring;
    }
    // Cylinder (its own radial-normal rows), then the flat base.
    let top: Vec<u32> = (0..SIDES).map(|side| { let (c, s) = dir(side);
        push(Vec3::new(c * r, s * r, -n), Vec3::new(c, s, 0.0), &mut vertices) }).collect();
    let bottom: Vec<u32> = (0..SIDES).map(|side| { let (c, s) = dir(side);
        push(Vec3::new(c * r, s * r, -l), Vec3::new(c, s, 0.0), &mut vertices) }).collect();
    band(&top, &bottom, &mut indices);
    let centre = push(Vec3::new(0.0, 0.0, -l), Vec3::NEG_Z, &mut vertices);
    let rim: Vec<u32> = (0..SIDES).map(|side| { let (c, s) = dir(side);
        push(Vec3::new(c * r, s * r, -l), Vec3::NEG_Z, &mut vertices) }).collect();
    for side in 0..SIDES {
        indices.extend_from_slice(&[centre, rim[(side + 1) % SIDES], rim[side]]);
    }
    let mut mesh = CpuMesh::basic(vertices, indices, Some(crate::prims::solid_color([92, 90, 86])),
        Vec3::new(-r, -r, -l), Vec3::new(r, r, 0.0));
    mesh.metallic_factor = 0.85;
    mesh.roughness_factor = 0.55;
    mesh
}

/// Register the lodged-round prototypes and their fixed physical pools (casting, in the TLAS).
pub(super) fn allocate_lodged_round_pools(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer) -> Vec<usize> {
    LODGED_SPECS.iter().map(|spec| {
        let prototype = r.register_instanced_prototype(device, queue, &flight_body_mesh(spec));
        r.add_moving_instanced_pool(device, prototype, LODGED_DRAW_CAPACITY).expect("lodged round pool")
    }).collect()
}

/// Field removal times that are not counted blows (named tuning, like the other field repairs).
const HAND_PULL_S: f32 = 4.0;
const BAR_PRY_S: f32 = 15.0;
/// One sledge blow per swing of the carried hammer (`equipment`'s cycle).
const SLEDGE_SWING_S: f32 = 0.48;

impl Game {
    fn lodged_round_model(&self, round: &LodgedRound) -> Mat4 {
        let rotation = Quat::from_rotation_arc(Vec3::Z, round.dir_local.normalize_or_zero());
        let parent = if round.on_gun { self.player.gun_model() } else { blood_parent_model(&self.player, round.frame) };
        parent * Mat4::from_rotation_translation(rotation, round.tip_local)
    }

    /// Pose every lodged round with its hull or turret, and every cannon round in flight.
    pub(super) fn drive_lodged_rounds(&mut self, r: &mut Renderer) {
        if self.fea.lodged_draw.is_empty() {
            return;
        }
        let mut per_spec: Vec<Vec<Mat4>> = vec![Vec::new(); LODGED_SPECS.len()];
        let mut overflow = false;
        for round in &self.fea.lodged {
            let Some(index) = lodged_spec_index(round.shell) else { continue };
            if per_spec[index].len() >= LODGED_DRAW_CAPACITY {
                overflow = true;
                continue;
            }
            per_spec[index].push(self.lodged_round_model(round));
        }
        // Rounds in flight are drawn as the same body (nose at the round's point, along its
        // velocity): a cannon round is a real object in the air, not only its tracer's glow.
        for shot in self.projectiles.iter().filter(|p| p.small_arm.is_none()) {
            let Some(index) = lodged_spec_index(shot.shell) else { continue };
            let dir = shot.velocity.normalize_or_zero();
            if dir == Vec3::ZERO || per_spec[index].len() >= LODGED_DRAW_CAPACITY {
                continue;
            }
            per_spec[index].push(Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Z, dir), shot.pos));
        }
        if overflow && !self.fea.lodged_overflow_logged {
            println!("[fea] more than {LODGED_DRAW_CAPACITY} rounds of one kind are lodged: the extra ones are not drawn (named bound)");
            self.fea.lodged_overflow_logged = true;
        }
        for (pool, mats) in self.fea.lodged_draw.iter().zip(&per_spec) {
            r.set_moving_instanced_mats(*pool, mats);
        }
    }

    /// Lodged rounds a crewman can work out in the field (not those needing a workshop).
    pub(super) fn fea_lodged_jobs(&self) -> Vec<super::equipment::RepairJob> {
        self.fea.lodged.iter().filter(|r| !matches!(r.extraction.removal, lodged::Removal::Workshop))
            .map(|r| super::equipment::RepairJob::LodgedRound(r.id)).collect()
    }

    /// Where the tools work on a lodged round: the middle of its exposed length (outside the
    /// plate for a round held by the struck plate).
    pub(super) fn fea_lodged_point(&self, id: u64) -> Option<Vec3> {
        let round = self.fea.lodged.iter().find(|r| r.id == id)?;
        let body = penetrator_body(round.shell);
        let exposed_from = if round.in_struck_plate { round.depth_m.min(body.length_m) } else { 0.0 };
        let along = 0.5 * (exposed_from + body.length_m);
        Some(self.lodged_round_model(round).transform_point3(Vec3::new(0.0, 0.0, -along)))
    }

    pub(super) fn fea_lodged_job_seconds(&self, id: u64) -> f32 {
        match self.fea.lodged.iter().find(|r| r.id == id).map(|r| r.extraction.removal) {
            Some(lodged::Removal::Hand) => HAND_PULL_S,
            Some(lodged::Removal::Bar) => BAR_PRY_S,
            Some(lodged::Removal::Blows(n)) => n as f32 * SLEDGE_SWING_S,
            _ => f32::INFINITY,
        }
    }

    pub(super) fn fea_lodged_job_prompt(&self, id: u64) -> &'static str {
        match self.fea.lodged.iter().find(|r| r.id == id).map(|r| r.extraction.removal) {
            Some(lodged::Removal::Hand) => "PULL THE LODGED ROUND OUT [HOLD LMB]",
            Some(lodged::Removal::Bar) => "PRY THE LODGED ROUND OUT WITH THE BAR [HOLD LMB]",
            Some(lodged::Removal::Blows(_)) => "KNOCK THE LODGED ROUND LOOSE WITH THE SLEDGE [HOLD LMB]",
            _ => "THE LODGED ROUND NEEDS A WORKSHOP",
        }
    }

    /// Embedded steel fragments the toolbox can free (those one man's pull cannot, short of a
    /// workshop): the bar or the sledge, by the lodged-round extraction rule on the chip.
    pub(super) fn fea_fragment_jobs(&self) -> Vec<super::equipment::RepairJob> {
        self.hot_fragments.iter().enumerate()
            .filter(|(_, f)| f.active && f.settled && f.lodged && !fragment_pullable(f)
                && !matches!(fragment_extraction(f).removal, lodged::Removal::Workshop))
            .map(|(i, _)| super::equipment::RepairJob::Fragment(i)).collect()
    }

    pub(super) fn fea_fragment_point(&self, slot: usize) -> Option<Vec3> {
        let f = self.hot_fragments.get(slot).filter(|f| f.active && f.lodged)?;
        Some(blood_parent_model(&self.player, f.frame).transform_point3(f.pos))
    }

    pub(super) fn fea_fragment_job_seconds(&self, slot: usize) -> f32 {
        match self.hot_fragments.get(slot).map(fragment_extraction).map(|e| e.removal) {
            Some(lodged::Removal::Bar) => FRAGMENT_PRY_S,
            Some(lodged::Removal::Blows(n)) => n as f32 * SLEDGE_SWING_S,
            Some(lodged::Removal::Hand) => 0.0,
            _ => f32::INFINITY,
        }
    }

    pub(super) fn fea_fragment_job_prompt(&self, slot: usize) -> &'static str {
        match self.hot_fragments.get(slot).map(fragment_extraction).map(|e| e.removal) {
            Some(lodged::Removal::Blows(_)) => "KNOCK THE STEEL FRAGMENT LOOSE WITH THE SLEDGE [HOLD LMB]",
            _ => "PRY THE STEEL FRAGMENT OUT WITH THE BAR [HOLD LMB]",
        }
    }

    /// The chip is free of the steel: it rests loose where it was, for the hand to take.
    pub(super) fn fea_free_fragment(&mut self, slot: usize) {
        let Some(frame) = self.hot_fragments.get(slot).map(|f| f.frame) else { return };
        let eye = blood_parent_model(&self.player, frame).inverse().transform_point3(self.station_eye(self.station));
        if let Some(f) = self.hot_fragments.get_mut(slot) {
            // The bar draws it out toward the man working it, clear of the face it was in (named:
            // the extraction direction), and it falls from there under the ordinary collision.
            let out = (eye - f.pos).normalize_or_zero();
            f.pos += out * (f.embed_m + f.size);
            f.prev_pos = f.pos;
            f.lodged = false;
            f.embed_m = 0.0;
            f.settled = false;
            f.vel = Vec3::ZERO;
            f.spin = Vec3::ZERO;
            println!("[fea] a {:.0} g steel fragment pried out of the steel", fragments::cube_mass(f.size) * 1000.0);
        }
    }

    /// The round is out: it leaves the simulation (named: a removed round is not kept as a loose
    /// object).
    pub(super) fn fea_remove_lodged(&mut self, id: u64) {
        if let Some(index) = self.fea.lodged.iter().position(|r| r.id == id) {
            let round = self.fea.lodged.remove(index);
            println!("[fea] the lodged {} is out ({:?}, {:.1} kN grip)", round.shell.name,
                round.extraction.removal, round.extraction.force_n / 1000.0);
        }
    }
}

/// The cupola's aperture radius (the source aperture the descent test uses, `foot`).
pub(super) const CUPOLA_APERTURE_RADIUS_M: f32 = 0.25;
const CUPOLA_OPENING: usize = 4;

/// Hull-frame z of the engine bay's firewall (game scale): the CAD envelopes put the fuel
/// tanks' forward ends at -1.14 m and the sponson ammunition bins' aft ends at -1.16 m (named,
/// within one 20 mm voxel).
pub(super) const FIREWALL_Z_M: f32 = -1.15;

/// Whether a point behind the struck plate is in the crew compartments' air: anything in the
/// turret (open to the fighting compartment) or forward of the firewall in the hull (the
/// driving and fighting compartments, sponsons included, machinery not subtracted: named).
pub(super) fn in_crew_air(part: ArmorPart, point_hull: Vec3) -> bool {
    part == ArmorPart::Turret || point_hull.z > FIREWALL_Z_M
}

/// The open area of a solved hole: the measured bore radii across and along the shot.
pub(super) fn hole_area_m2(g: &lattice::MeasuredGeometry) -> f32 {
    std::f32::consts::PI * (g.aperture_mm * 1e-3) * (g.aperture_along_mm.max(g.aperture_mm) * 1e-3)
}

/// The crew compartments' openings: the four roof and escape lids (`tiger_hatches` order) and the
/// cupola, at their source apertures brought to real size (the game's Tiger is drawn 1.112 times
/// real; a lid's area loads its latch at real size), then the shot's own entry hole (solved at
/// real size). A lid counts open past half its travel (named); the compartments' other small
/// leaks are not modelled.
pub(super) fn crew_openings(lid_phase: [f32; 4], cupola_phase: f32, hole_area_m2: f32) -> Vec<compartment::Opening> {
    const NAMES: [&str; 4] = ["driver's hatch", "loader's hatch", "radio operator's hatch", "turret escape hatch"];
    let disc = |r: f32| {
        let real = r / crate::fea_damage::components::GAME_OVER_REAL_LENGTH;
        std::f32::consts::PI * real * real
    };
    let mut openings: Vec<compartment::Opening> = (0..4).map(|i| compartment::Opening { name: NAMES[i],
        area_m2: disc(tiger_hatches::APERTURE_RADII[i]), open: lid_phase[i] > 0.5, hatch: true }).collect();
    openings.push(compartment::Opening { name: "commander's cupola", area_m2: disc(CUPOLA_APERTURE_RADIUS_M),
        open: cupola_phase > 0.5, hatch: true });
    openings.push(compartment::Opening { name: "the shot's entry hole", area_m2: hole_area_m2, open: true, hatch: false });
    openings
}

/// Kinetic energy (J) the burst body's pieces carry in a lattice-path population: the pieces with
/// no solved temperature (the plate's clusters have one).
pub(super) fn burst_body_kinetic_j(fragments: &[HotFragment]) -> f32 {
    fragments.iter().filter(|f| f.temp_k <= 0.0)
        .map(|f| 0.5 * fragments::cube_mass(f.size) * f.vel.length_squared()).sum()
}

/// The pool slot a new fragment takes. Resting steel the lattice made (cooled chips on the floor
/// or in the walls) and the chip in a crewman's hand persist: the ring passes over them, and only a
/// pool full of resting steel gives up the slot at the cursor, its oldest (a named bound of
/// `N_HOT_FRAGMENTS` bodies). The chip in hand is never overwritten.
pub(super) fn next_fragment_slot(pool: &[HotFragment], cursor: usize, carried: Option<usize>) -> usize {
    let n = pool.len();
    let start = cursor % n;
    if let Some(slot)=(0..n).map(|k|(start+k)%n).find(|&i|!pool[i].active && Some(i)!=carried){return slot;}
    for k in 0..n {
        let slot = (start + k) % n;
        let f = &pool[slot];
        let persistent = f.active && f.settled && f.life == f32::MAX;
        if !persistent && Some(slot) != carried {
            return slot;
        }
    }
    if Some(start) == carried { (start + 1) % n } else { start }
}

/// Smallest pick radius of a fragment under the crosshair, m (named: a chip smaller than this is
/// still picked where the eye puts it).
const FRAGMENT_PICK_RADIUS_M: f32 = 0.012;
const FRAGMENT_REACH_M: f32 = 2.2;
/// Speed a crewman's toss adds to a small chip, m/s (named).
const FRAGMENT_THROW_SPEED: f32 = 6.0;

fn ray_sphere(origin: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - centre;
    let b = oc.dot(dir);
    let c = oc.length_squared() - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let t = -b - root;
    if t >= 0.0 { Some(t) } else if -b + root >= 0.0 { Some(0.0) } else { None }
}

/// A fragment can be held when it is no hotter than a gloved hand tolerates (`LodgedFragment::
/// HOLDABLE_K`); an authored chip with no solved temperature while it is on a glowing page.
pub(super) fn fragment_holdable(f: &HotFragment) -> bool {
    if f.temp_k > 0.0 {
        f.temp_k <= fragments::LodgedFragment::HOLDABLE_K
    } else {
        hot_fragment_heat_stage(f) >= N_HOT_FRAGMENT_BATCHES - 2
    }
}

/// A fragment embedded in a wall comes out by hand only when one man's pull beats its grip.
pub(super) fn fragment_pullable(f: &HotFragment) -> bool {
    !f.lodged || matches!(fragment_extraction(f).removal, lodged::Removal::Hand)
}

/// The lodged-round rule on an embedded chip: Coulomb friction on the named residual grip of
/// the cabin wall's steel over its embedded depth.
pub(super) fn fragment_extraction(f: &HotFragment) -> lodged::Extraction {
    lodged::extraction(lodged::estimated_grip_pa(CABIN_WALL_FLOW_STRESS_PA), f.size, f.embed_m, true)
}

/// Time to pry a chip out with the toolbox's bar (named tuning, like the other field repairs).
const FRAGMENT_PRY_S: f32 = 3.0;

#[cfg(test)]
mod true_scale_tests {
    use super::*;

    /// The plate the lattice solves is the armour scheme's, in true millimetres: the hero mesh's
    /// 1.112 visual scale (docs/PHYSICAL_TRUTH_AUDIT.md, "Visual-size debt") never reaches it.
    #[test]
    fn the_lattice_takes_the_schemes_true_thickness_not_the_meshs() {
        use crate::ballistics::{ArmorScheme, Facing};
        let mut solved = 0;
        for (facing, shell, range) in [(Facing::HullFront, &crate::ballistics::BR_365, 300.0f32),
            (Facing::HullSide, &crate::ballistics::BR_354P, 200.0), (Facing::TurretSide, &crate::ballistics::BR_240_M42, 100.0)] {
            let plate = ArmorScheme::TigerI.plate(facing);
            let event = crate::ballistics::resolve(shell, range, plate, 0.0, 7);
            let cos = event.impact_angle_deg.to_radians().cos().clamp(0.05, 1.0);
            let path = event.plate.thickness_mm * 0.001 / cos;
            let line = chain::ShotLine { entry: Vec3::ZERO, dir: Vec3::NEG_Z, plate_path_m: path, far_wall: None, max_s: 12.0 };
            let layers = [chain::Layer { kind: chain::LayerKind::StruckPlate, s0: 0.0, s1: path, cos_obliquity: cos, normal: Vec3::Z }];
            let result = chain::run(&event, &layers, 1.2);
            let Some((impact, _)) = lattice_problem(&event, &line, &result, Vec3::Z, 1) else { continue };
            assert_eq!(impact.thickness_m, plate.thickness_mm * 0.001, "{facing:?}: {} mm", plate.thickness_mm);
            solved += 1;
        }
        assert!(solved >= 2, "the cases must reach the lattice ({solved})");
    }

    /// The strike frame's cost before and after the perforation's solves left the main thread:
    /// the chain (still on it) against the plate lattice and the round's own solve (now on
    /// workers). `cargo test --release -- --ignored strike_frame_cost --nocapture`.
    #[test]
    #[ignore]
    fn strike_frame_cost() {
        use crate::ballistics::{ArmorScheme, Facing};
        for (label, facing, shell, range) in [
            ("76 mm BR-350B on the hull front (stopped)", Facing::HullFront, &crate::ballistics::BR_350B, 500.0f32),
            ("88 mm PzGr 39 through the hull side", Facing::HullSide, &crate::ballistics::PZGR_39, 500.0),
        ] {
            let plate = ArmorScheme::TigerI.plate(facing);
            let event = crate::ballistics::resolve(shell, range, plate, 0.0, 7);
            let cos = event.impact_angle_deg.to_radians().cos().clamp(0.05, 1.0);
            let path = event.plate.thickness_mm * 0.001 / cos;
            let line = chain::ShotLine { entry: Vec3::new(0.0, 1.2, 2.0), dir: Vec3::NEG_Z, plate_path_m: path,
                far_wall: None, max_s: 12.0 };
            let t = std::time::Instant::now();
            let state = chain::VehicleState { articulation: components::Articulation::rest(Vec3::ZERO), fuel_fill: 0.5,
                gravity_hull: Vec3::NEG_Y, live_rounds: &[] };
            let layers = chain::build_layers(&line, components::tiger_components(), &state);
            let result = chain::run(&event, &layers, 1.2);
            let chain_ms = t.elapsed().as_secs_f64() * 1000.0;
            let lattice_ms = lattice_problem(&event, &line, &result, Vec3::Z, 1).map_or(0.0, |(impact, axes)| {
                let t = std::time::Instant::now();
                PlateJob { impact, origin: line.entry, axes }.solve();
                t.elapsed().as_secs_f64() * 1000.0
            });
            let t = std::time::Instant::now();
            let shot = ShotStrike::solve(&event, 9);
            let shot_ms = if shot.is_some() { t.elapsed().as_secs_f64() * 1000.0 } else { 0.0 };
            let penetrated = event.terminal == crate::ballistics::Terminal::Penetration;
            // Before: a perforation solved its lattice and the round's own solve on the main
            // thread; a stopped round already solved its lattice on a worker.
            let before = chain_ms + if penetrated { lattice_ms.max(shot_ms) } else { 0.0 };
            println!("[strike-cost] {label}: {:?}; chain {chain_ms:.2} ms (main thread), lattice {lattice_ms:.0} ms, round {shot_ms:.0} ms (workers); strike frame before {before:.0} ms, after {chain_ms:.2} ms",
                event.terminal);
        }
    }
}

#[cfg(test)]
mod compartment_tests {
    use super::*;

    fn openings(open: [bool; 5], hole: f32) -> Vec<compartment::Opening> {
        let phase = |o: bool| if o { 1.0 } else { 0.0 };
        crew_openings([phase(open[0]), phase(open[1]), phase(open[2]), phase(open[3])], phase(open[4]), hole)
    }

    fn burst(filler_kg: f32, lids: [bool; 5]) -> compartment::PressureHistory {
        compartment::burst(filler_kg, compartment::TNT_J_PER_KG, 0.0, compartment::TIGER_CREW_AIR_M3,
            crate::fea_damage::materials::AMBIENT_K, &openings(lids, 0.0046), compartment::GAME_DT_S,
            compartment::GAME_DURATION_S)
    }

    #[test]
    fn an_armour_piercing_burst_holds_the_latches_and_a_large_charge_throws_every_closed_lid() {
        // A 76 mm BR-350B's 65 g: about 11 kPa, far below a lid's latch (about 90 kPa on 0.22 m2).
        let small = burst(crate::ballistics::BR_350B.filler_g * 0.001, [false; 5]);
        assert!(small.hatches_blown.is_empty(), "{} Pa", small.peak_gauge_pa);
        assert!((small.peak_gauge_pa - 0.4 * 0.065 * 4.184e6 / 10.0).abs() < 300.0, "{}", small.peak_gauge_pa);
        // A 152 mm OF-530's 5.9 kg inside: every closed lid goes; an open one is not 'blown'.
        let large = burst(crate::ballistics::OF_530.filler_g * 0.001, [false, true, false, false, false]);
        let names: Vec<&str> = large.hatches_blown.iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), 4, "{names:?}");
        assert!(!names.contains(&"loader's hatch"));
        // Open lids vent it: the pressure falls well below the peak within the sampled 0.2 s.
        assert!(large.samples.last().unwrap().1 < 0.2 * large.peak_gauge_pa);
    }

    #[test]
    fn the_firewall_splits_crew_air_from_the_engine_bay() {
        let set = crate::fea_damage::components::tiger_components();
        let z_range = |id: &str| {
            let e = set.envelopes.iter().find(|e| components::COMPONENT_SPECS[e.spec].id == id).expect(id);
            let (min, max) = e.bounds();
            (min.z, max.z)
        };
        // The firewall lies between the engine bay's machinery and the crew's ammunition.
        for id in ["engine_block", "fuel_tank_left", "fuel_tank_right", "radiator_left", "cooling_fans_right"] {
            assert!(z_range(id).1 <= FIREWALL_Z_M + 0.02, "{id} reaches past the firewall: {:?}", z_range(id));
        }
        for id in ["ammo_sponson_left_rear", "ammo_sponson_right_rear", "gearbox", "radio", "driver_seat"] {
            assert!(z_range(id).0 >= FIREWALL_Z_M - 0.02, "{id} is behind the firewall: {:?}", z_range(id));
        }
        assert!(in_crew_air(ArmorPart::Hull, Vec3::new(1.6, 1.4, 0.3)), "a sponson in the fighting compartment");
        assert!(in_crew_air(ArmorPart::Hull, Vec3::new(-0.4, 1.6, 2.4)), "behind the front plate");
        assert!(!in_crew_air(ArmorPart::Hull, Vec3::new(0.9, 1.5, -1.8)), "the fuel tank's side");
        assert!(in_crew_air(ArmorPart::Turret, Vec3::new(0.0, 2.0, -2.5)), "the turret is open to the hull");
    }

    #[test]
    fn the_burst_bodys_energy_comes_off_the_filler() {
        let piece = |temp_k: f32, v: f32| HotFragment { active: true, size: 0.01, temp_k, vel: Vec3::X * v,
            ..HotFragment::default() };
        // Two body pieces at 100 m/s; the plate cluster (solved temperature) is not the body's.
        let pieces = [piece(0.0, 100.0), piece(0.0, 100.0), piece(900.0, 400.0)];
        let expected = 2.0 * 0.5 * fragments::cube_mass(0.01) * 100.0 * 100.0;
        assert!((burst_body_kinetic_j(&pieces) - expected).abs() < 1e-3);
    }
}

#[cfg(test)]
mod lodged_draw_tests {
    use super::*;

    #[test]
    fn drawn_body_holds_the_rounds_mass_and_the_solved_diameter() {
        for spec in LODGED_SPECS.iter() {
            let Some(shot)=flight_lattice(spec)else{continue;};
            let body=penetrator_body(spec);let mesh=flight_body_mesh(spec);
            let mass:f32=shot.mass.iter().sum();
            assert!((mass/spec.mass_kg-1.).abs()<1e-4,"{}",spec.name);
            assert!(mesh.max.z.abs()<1e-6,"nose face at contact, {}: {}",spec.name,mesh.max.z);
            assert!((mesh.min.z+body.length_m).abs()<shot.h*0.01,"{}",spec.name);
            let expected=if crate::ballistics::is_long_rod(spec){0.03}else{spec.caliber_mm*0.001};
            assert!((body.radius_m*2.-expected).abs()<1e-6,"{}",spec.name);
            // Regression: the old flying APCR mesh showed only its core, then grew at impact.
            assert!(mesh.max.x-mesh.min.x>expected*0.65,"{}",spec.name);
        }
    }

    #[test]
    fn drawn_round_is_a_closed_outward_lathe_of_that_body() {
        let body = penetrator_body(&crate::ballistics::BR_350B);
        let mesh = penetrator_mesh(body);
        // Divergence theorem: a closed, outward-wound surface has a positive volume equal to the
        // 12-sided lathe of the profile (inscribed polygon: area factor 12 sin(30 deg) / (2 pi)).
        let mut volume = 0.0f64;
        for tri in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from_array(mesh.vertices[tri[i] as usize].pos).as_dvec3());
            volume += a.dot(b.cross(c)) / 6.0;
        }
        let polygon = 12.0 * (std::f64::consts::TAU / 12.0).sin() / std::f64::consts::TAU;
        let area = std::f64::consts::PI * (body.radius_m as f64).powi(2) * polygon;
        let cylinder = area * (body.length_m - body.nose_m) as f64;
        assert!(volume > cylinder, "outward and closed: {volume}");
        assert!(volume < cylinder + 2.0 / 3.0 * area * body.nose_m as f64 * 1.01, "{volume}");
        assert!(mesh.max.z.abs() < 1e-6 && (mesh.min.z + body.length_m).abs() < 1e-6, "tip at the origin, base at -length");
    }

    #[test]
    fn every_kinetic_round_the_save_knows_has_a_draw_pool() {
        use crate::ballistics as b;
        for spec in [&b::PZGR_39, &b::PZGR_40, &b::PZGR_39_43, &b::BR_240_M42, &b::BR_240, &b::BR_240P,
            &b::BR_271_ZIS2, &b::BR_350B, &b::BR_354P, &b::BR_365, &b::BR_412, &b::BR_471, &b::BR_540,
            &b::SPRGR_88, &b::OF_350_F34, &b::OF_530, &b::PANZERFAUST_60] {
            let kinetic = matches!(spec.kind, b::ShellKind::Ap | b::ShellKind::Apcbc | b::ShellKind::Apcr);
            assert_eq!(lodged_spec_index(spec).is_some(), kinetic, "{}", spec.name);
        }
    }
}

#[cfg(test)]
mod save_tests {
    use super::*;
    use crate::expedition_world_save::OwnerBlock;

    fn stop_job() -> PlateJob {
        PlateJob {
            impact: lattice::PlateImpact {
                material: crate::fea_damage::materials::MaterialId::RolledArmour,
                thickness_m: 0.02,
                obliquity_rad: 0.2,
                projectile: lattice::ProjectileShape { mass_kg: 1.43, radius_m: 0.0225, nose_m: 0.054 },
                v_in: 700.0,
                motion: lattice::Motion::Stop { depth_m: 0.012 },
                seed: 5,
            },
            origin: Vec3::new(0.4, 1.2, 2.3),
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
        }
    }

    #[test]
    fn struck_gearing_turns_stiff_then_jams_and_a_handwheel_counts_half() {
        let mut damage = InteriorDamage::default();
        assert_eq!((damage.traverse_factor(), damage.elevation_factor()), (1.0, 1.0));
        let gearbox = InteriorDamage::spec("traverse_gearbox").unwrap();
        let flywheel = InteriorDamage::spec("traverse_flywheel").unwrap();
        damage.record(gearbox, 0.5 * TRAVERSE_GEAR_JAM_J);
        assert!((damage.traverse_factor() - 0.5).abs() < 1e-6 && !damage.traverse_jammed());
        damage.record(flywheel, 0.5 * TRAVERSE_GEAR_JAM_J);
        assert!((damage.traverse_factor() - 0.25).abs() < 1e-6 && !damage.traverse_jammed());
        damage.record(gearbox, 0.25 * TRAVERSE_GEAR_JAM_J);
        assert!(damage.traverse_jammed());
        assert_eq!(damage.elevation_factor(), 1.0, "the elevation train is its own");
        damage.record(InteriorDamage::spec("elevation_gearbox").unwrap(), ELEVATION_GEAR_JAM_J);
        assert!(damage.elevation_jammed());
        for id in ["traverse_gearbox", "traverse_flywheel", "traverse_pedal", "elevation_gearbox", "elevation_handwheel", "basket_floor"] {
            assert!(InteriorDamage::tracks(InteriorDamage::spec(id).unwrap()), "{id}");
        }
    }

    fn saved_state() -> FeaSaved {
        let extraction = lodged::extraction(4.0e7, 0.045, 0.02, true);
        FeaSaved {
            lodged: vec![LodgedRound { id: 3, shell: &crate::ballistics::BR_471, frame: BloodFrame::Hull,
                tip_local: Vec3::new(0.2, 1.0, 2.6), dir_local: Vec3::NEG_Z, depth_m: 0.07,
                in_struck_plate: true, component: None, extraction, struck_at_s: 12.5, on_gun: false }],
            pending: vec![PendingPlateSaved { commit_at_s: 12.75, struck_at_s: 12.5,
                facing: crate::ballistics::Facing::HullFront, local_mm: [200, 1000, 2600], lodged_id: Some(3),
                job: stop_job() }],
            carried_fragment: Some(17),
            next_id: 4,
            interior: InteriorDamage { energy_j: vec![(InteriorDamage::spec("traverse_gearbox").unwrap(), 1_200.0)] },
            ..FeaSaved::default()
        }
    }

    #[test]
    fn damage_state_survives_the_save_codec_and_a_deferred_solve_resolves_the_same() {
        let block = OwnerBlock::encode("test.fea", 1, &saved_state()).unwrap();
        let mut runtime = FeaRuntime::default();
        runtime.import_world_state(block.decode("test.fea", 1).unwrap()).unwrap();
        let again = OwnerBlock::encode("test.fea", 1, &runtime.export_world_state()).unwrap();
        assert_eq!(block.payload, again.payload, "export after import is byte-identical");
        assert_eq!(runtime.lodged.len(), 1);
        assert_eq!(runtime.carried_fragment, Some(17));
        // The re-started solve gives the answer a solve before the save would have given.
        let pending = runtime.pending.pop().unwrap();
        let resumed = pending.handle.join().unwrap();
        let direct = stop_job().solve();
        assert_eq!(resumed.steps, direct.steps);
        assert_eq!(resumed.rim_curve.kelvin, direct.rim_curve.kelvin);
        assert_eq!(resumed.grip_normal_n.to_bits(), direct.grip_normal_n.to_bits());
    }

    #[test]
    fn damage_state_import_refuses_broken_records() {
        let mut bad = saved_state();
        bad.carried_fragment = Some(N_HOT_FRAGMENTS);
        assert!(FeaRuntime::default().import_world_state(bad).is_err());
        let mut bad = saved_state();
        bad.pending[0].lodged_id = Some(99);
        assert!(FeaRuntime::default().import_world_state(bad).is_err());
        let mut bad = saved_state();
        bad.next_id = 2;
        assert!(FeaRuntime::default().import_world_state(bad).is_err());
    }
}

#[cfg(test)]
mod hand_tests {
    use super::*;

    fn chip(temp_k: f32, size: f32) -> HotFragment {
        HotFragment { active: true, settled: true, frame: BloodFrame::Hull, size, life: f32::MAX,
            temp_k, seed: 7, ..HotFragment::default() }
    }

    #[test]
    fn hand_takes_only_cool_and_loose_steel() {
        // At the glove limit it can be held; above it, it cannot.
        assert!(fragment_holdable(&chip(fragments::LodgedFragment::HOLDABLE_K, 0.01)));
        assert!(!fragment_holdable(&chip(fragments::LodgedFragment::HOLDABLE_K + 1.0, 0.01)));
        // A chip resting loose always comes away.
        assert!(fragment_pullable(&chip(300.0, 0.01)));
        // A chip barely bitten into the wall comes out by hand; one driven deep needs more.
        let mut shallow = chip(300.0, 0.008);
        shallow.lodged = true;
        shallow.embed_m = 0.0002;
        let mut deep = shallow;
        deep.embed_m = 0.008;
        let pull = |f: &HotFragment| lodged::extraction(lodged::estimated_grip_pa(CABIN_WALL_FLOW_STRESS_PA),
            f.size, f.embed_m, true);
        assert!(fragment_pullable(&shallow), "{:?}", pull(&shallow));
        assert!(!fragment_pullable(&deep), "{:?}", pull(&deep));
        assert!(matches!(pull(&deep).removal, lodged::Removal::Bar | lodged::Removal::Blows(_)));
    }

    #[test]
    fn pool_keeps_resting_solved_steel_and_the_chip_in_hand() {
        let mut pool = vec![HotFragment::default(); 6];
        for i in [0, 1, 3] {
            pool[i] = chip(320.0, 0.01);
        }
        // An authored (unsolved) chip in slot 2 is ordinary, expiring glow: it may be replaced.
        pool[2] = HotFragment { active: true, settled: true, life: 1.0, ..HotFragment::default() };
        assert_eq!(next_fragment_slot(&pool, 0, None), 2);
        assert_eq!(next_fragment_slot(&pool, 0, Some(2)), 4);
        assert_eq!(next_fragment_slot(&pool, 3, None), 4);
        // Full of resting steel: the cursor's slot (the oldest) gives way, never the chip in hand.
        let full = vec![chip(300.0, 0.01); 4];
        assert_eq!(next_fragment_slot(&full, 1, None), 1);
        assert_eq!(next_fragment_slot(&full, 1, Some(1)), 2);
        // A hot chip still flying is not persistent yet: it is not skipped.
        let mut flying = chip(1200.0, 0.01);
        flying.settled = false;
        assert_eq!(next_fragment_slot(&[flying, chip(300.0, 0.01)], 0, None), 0);
    }

    #[test]
    fn crosshair_ray_meets_the_chip_it_points_at() {
        let centre = Vec3::new(0.0, 0.0, 1.0);
        assert!((ray_sphere(Vec3::ZERO, Vec3::Z, centre, 0.02).unwrap() - 0.98).abs() < 1e-5);
        assert!(ray_sphere(Vec3::ZERO, Vec3::Y, centre, 0.02).is_none());
        assert!(ray_sphere(Vec3::ZERO, Vec3::NEG_Z, centre, 0.02).is_none());
        assert_eq!(ray_sphere(centre, Vec3::Z, centre, 0.02), Some(0.0));
    }
}

#[cfg(test)]
mod ignition_tests {
    use super::*;
    use crate::fea_damage::ignition::{Flammable, LUBRICATING_OIL_K, OIL, PETROL};

    fn entry(component: usize, what: Flammable, by_round: bool, body_k: f32, x: f32) -> StrikeIgnition {
        StrikeIgnition { local_point: Vec3::X * x, local_velocity: Vec3::Z, component, what, by_round, body_k }
    }

    #[test]
    fn a_strike_tries_each_flammable_component_once_at_its_hottest_body() {
        let oil = Flammable::Oil { hot_surface_k: LUBRICATING_OIL_K };
        let mut result = SpallResult::default();
        // Three fragments into the gearbox, one of them hot; a fragment, then the round, into a tank.
        result.enter_flammable(entry(9, oil, false, 400.0, 1.0));
        result.enter_flammable(entry(9, oil, false, 900.0, 2.0));
        result.enter_flammable(entry(9, oil, false, 300.0, 3.0));
        result.enter_flammable(entry(7, Flammable::Petrol, false, 0.0, 4.0));
        result.enter_flammable(entry(7, Flammable::Petrol, true, 0.0, 5.0));
        result.ignite(42);
        assert_eq!(result.ignition_trials.len(), 2);
        let gearbox = result.ignition_trials.iter().find(|t| t.component == 9).unwrap();
        assert_eq!((gearbox.body_k, gearbox.chance), (900.0, OIL));
        let tank = result.ignition_trials.iter().find(|t| t.component == 7).unwrap();
        assert!(tank.by_round && tank.chance == PETROL);
        // The round's entry is where a fire in the tank starts; the first fragment's in the gearbox.
        assert_eq!(result.ignition_entries.iter().find(|e| e.component == 7).unwrap().local_point, Vec3::X * 5.0);
        assert_eq!(result.ignition_entries.iter().find(|e| e.component == 9).unwrap().local_point, Vec3::X * 1.0);
        assert_eq!(result.fire, result.ignition.is_some());
        // The same strike always answers the same.
        let mut again = result.clone();
        again.ignite(42);
        assert_eq!(again.ignition.map(|i| i.component), result.ignition.map(|i| i.component));
    }

    #[test]
    fn over_many_strikes_a_holed_tank_lights_at_its_chance_and_cold_oil_never() {
        let strikes = 4000u64;
        let lit = (0..strikes).filter(|&id| {
            let mut result = SpallResult::default();
            result.enter_flammable(entry(6, Flammable::Petrol, true, 0.0, 0.0));
            result.ignite(id);
            result.fire
        }).count() as f32 / strikes as f32;
        assert!((lit - PETROL).abs() < 0.03, "{lit}");
        let cold = (0..strikes).filter(|&id| {
            let mut result = SpallResult::default();
            result.enter_flammable(entry(8, Flammable::Oil { hot_surface_k: LUBRICATING_OIL_K }, false, 500.0, 0.0));
            result.ignite(id);
            result.fire
        }).count();
        assert_eq!(cold, 0);
    }
}

#[cfg(test)]
mod round_solve_tests {
    use super::*;
    use crate::ballistics::{self, ArmorMaterial, Plate, Terminal};

    fn plate(mm: f32) -> Plate {
        Plate { name: "test plate", thickness_mm: mm, slope_deg: 0.0, material: ArmorMaterial::Rha }
    }

    #[test]
    fn a_glancing_round_is_solved_tumbling_and_a_perforating_one_sheds_into_the_vehicle() {
        let glance = ballistics::resolve(&ballistics::BR_471, 500.0, plate(100.0), 72.0, 5);
        assert_eq!(glance.terminal, Terminal::Bounce);
        let solved = RicochetShot::spawn(&glance, 9).unwrap().join().unwrap().unwrap();
        assert!(solved.tumble_rad_s > 20.0 && solved.residual_fraction > 0.5 && solved.residual_fraction <= 1.0, "{solved:?}");
        let through = ballistics::resolve(&ballistics::PZGR_39, 300.0, plate(80.0), 30.0, 5);
        assert_eq!(through.terminal, Terminal::Penetration);
        let shot = ShotStrike::solve(&through, 9).unwrap();
        assert!(shot.residual_fraction() < 1.0 && !shot.shrapnel.is_empty(), "{} kg, {} shrapnel", shot.residual_kg, shot.shrapnel.len());
        assert!(ShotStrike::solve(&glance, 9).is_none(), "only a perforation's round is solved for the chain");
    }
}
