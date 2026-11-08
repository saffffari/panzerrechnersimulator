//! The casualties' bodies as voxels (2026-11-02, the owner's design). The relaxed crew figure and
//! the owner's fitted PSX skeleton (`crew_skeleton`) share one grid of `CELL_M` cells: every cell
//! inside the figure is skin within one cell of its surface and muscle deeper, and every cell a
//! skeleton triangle passes through is bone (the skeleton is a surface, so its quads are what is
//! voxelised). Only organic matter is cells: his steel helmet is a rigid body (`crew_helmet_body`).
//! A dead man is drawn as his cells, never as the figure's triangles.
//!
//! A strike is solved on those cells as a bonded lattice: each cell is a mass bonded to its 18
//! nearest neighbours; a bond is a spring that lets go past its critical stretch. A fragment is a
//! steel cube of its own mass moving through the cells along its path; it loses speed to the
//! tissue's resistance and Newtonian drag (Poncelet's form, R·A + ρ·Cd·A·v²/2) and that momentum
//! goes to the cells about it, mostly forward and partly outward. A burst's pressure work becomes motion of the cells facing it, away
//! from it. The lattice is then run for `SOLVE_S`: cells pushed hard enough tear (or crush) their bonds, and
//! every piece no longer bonded to its part's main body comes away as a chunk with its own motion.
//! Strengths are authored game-level values in the owner's order (skin gives first, then muscle,
//! bone last; a bone tears from the muscle before it breaks), scaled so tearing all of a part's soft
//! cells costs about its existing pulverizing work (`crew_damage`). This is a bond-based lattice
//! (peridynamic in form), not a finite-element model, and not a measured tissue model.
use super::*;
use crate::gltf_load::CpuMesh;

/// Cell size of the procedural build (m); an anatomical volume brings its own (`TissueModel::cell`).
pub(super) const CELL_M: f32 = 0.02;
const PARTS: usize = crew_physics::CREW_BODY_PART_COUNT;
/// Below this share of its soft cells a struck part leaves the articulated simulation (what is
/// left of it then rides the nearest retained segment).
pub(super) const KEEP_SOFT_SHARE: f32 = 0.2;
/// Energy to tear one cell free of all its neighbours per cell volume (J/m³), skin, muscle, bone,
/// fluid: authored, in the owner's order, scaled so a part's soft cells tear for about its
/// pulverizing work (82–1,250 J over 233–3,400 cell-units of skin 1 : muscle 2.5; at the
/// procedural 2 cm cell 0.4, 1.0 and 3.2 J). A fluid (blood, CSF, gut contents) is held by a
/// quarter of skin's, so it stays until struck and then spills (its flow is the blood system's;
/// named).
const TEAR_J_M3: [f32; 4] = [50_000.0, 125_000.0, 400_000.0, 12_500.0];
/// The stretch at which a bond lets go, in tension or crushed in compression (authored): skin
/// stretches far, bone hardly, a fluid as far as it is pulled.
const CRITICAL_STRETCH: [f32; 4] = [0.6, 0.35, 0.05, 1.0];
/// A uniform's woollen cloth breaks at about a quarter's stretch (wool fabric's elongation at break,
/// roughly 25-35 %; the lower end taken: named). A bond to a cloth cell holds no further than this,
/// whatever the review tune does to the flesh's stretch, and its energy is the weaker cell's: cloth
/// never outlasts the flesh under it.
pub(super) const CLOTH_BREAKING_STRAIN: f32 = 0.25;
/// Densities of the procedural build (kg/m³): soft tissue about water's, bone about cortical
/// bone's (an anatomical volume gives each cell its tissue's own, `crew_anatomy`).
const DENSITY: [f32; 4] = [1_060.0, 1_060.0, 1_900.0, 1_050.0];
/// A fragment's steel (kg/m³).
const STEEL_DENSITY: f32 = 7_850.0;
/// Drag coefficient of a tumbling fragment in tissue (authored, a blunt body's).
const DRAG_CD: f32 = 1.0;
/// Of the momentum a fragment loses, the share pushed along its path and outward from it
/// (authored: the channel opens forward, and a little to the sides).
const FORWARD: f32 = 0.95;
const OUTWARD: f32 = 0.3;
/// Simulated time after the strikes, and the per-step velocity damping (named numerical damping).
const SOLVE_S: f32 = 0.008;
/// Worker threads for a body's lattice solve (its result is the same for any count).
pub(super) const SOLVE_THREADS: usize = 8;
/// Cells this far from a fragment's path move in the solve; farther ones hold them (anchored).
const ACTIVE_REACH_M: f32 = 0.12;
const DAMPING: f32 = 0.001;
/// A fragment slower than this has stopped in the tissue (lodged).
const STOPPED_MPS: f32 = 2.0;

/// The lattice's authored constants as one set, and a review-only override of it (`fea-range`, the
/// owner's hand tuning on the range). No shipping path sets the override: `tissue_tuning()` is
/// then exactly the constants above. Bonds are baked at allocation; a solve scales each bond's
/// stiffness and critical stretch by tuned/authored for its strength class (the law
/// `stiffness = 2·energy / (critical·rest)²` kept), so a change acts on the next strike without
/// rebuilding the model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TissueTuning {
    /// Tearing energy per volume (J/m³): skin, muscle, bone, fluid.
    pub tear_j_m3: [f32; 4],
    /// Critical stretch: skin, muscle, bone, fluid.
    pub critical_stretch: [f32; 4],
    /// Shares of a fragment's lost momentum pushed along its path and outward from it.
    pub forward: f32,
    pub outward: f32,
    /// Cells this far from a fragment's path move in the solve (m).
    pub active_reach_m: f32,
    /// A landing piece breaks into grains when ½·m·vn² exceeds this times its bonds' cohesion.
    pub landing_breakup: f32,
    /// The radius a fragment's lost momentum is shared over, as a factor (with `Tune::spread`).
    pub spread: f32,
}

impl TissueTuning {
    pub(crate) const AUTHORED: Self = Self { tear_j_m3: TEAR_J_M3, critical_stretch: CRITICAL_STRETCH, forward: FORWARD,
        outward: OUTWARD, active_reach_m: ACTIVE_REACH_M, landing_breakup: 1.0, spread: 1.0 };
}

static REVIEW_TUNING: std::sync::RwLock<Option<TissueTuning>> = std::sync::RwLock::new(None);

/// Review-only (`fea-range`): replace the authored lattice constants, or `None` to restore them.
pub(crate) fn set_review_tuning(tuning: Option<TissueTuning>) {
    *REVIEW_TUNING.write().unwrap_or_else(|e| e.into_inner()) = tuning;
}

/// Whether the range's review override is set.
fn review_tuning_active() -> bool {
    REVIEW_TUNING.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

/// The lattice constants in force: the authored ones unless a review override is set.
pub(crate) fn tissue_tuning() -> TissueTuning {
    REVIEW_TUNING.read().unwrap_or_else(|e| e.into_inner()).unwrap_or(TissueTuning::AUTHORED)
}

/// Per strength class, the (stiffness, critical stretch) factors of the tuning against the
/// authored constants the bonds were built with.
fn bond_law_scale(t: &TissueTuning) -> [(f32, f32); 4] {
    std::array::from_fn(|k| {
        let energy = t.tear_j_m3[k] / TEAR_J_M3[k];
        let critical = t.critical_stretch[k] / CRITICAL_STRETCH[k];
        (energy / (critical * critical), critical)
    })
}

pub(crate) fn reference_solve() -> bool {
    std::env::var("KESSEL_TISSUE_SOLVE").as_deref() == Ok("reference")
}

/// Bound interactive integration work independently of review slider stiffness. The
/// offline reference path preserves its original timestep and constitutive law.
fn interactive_schedule(stable_dt: f32, reference: bool) -> (f32, usize, f32) {
    let requested = (SOLVE_S / stable_dt).ceil().max(1.0) as usize;
    if reference || requested <= 512 { return (stable_dt, requested, 1.0); }
    let dt = SOLVE_S / 512.0;
    (dt, 512, (stable_dt / dt).powi(2))
}

#[cfg(test)]
mod interactive_schedule_tests {
    use super::*;
    #[test]
    fn extreme_tuning_has_bounded_steps_and_stable_stiffness() {
        let stable = SOLVE_S / 111_536.0;
        let (dt, steps, scale) = interactive_schedule(stable, false);
        assert_eq!(steps, 512);
        assert!((dt * steps as f32 - SOLVE_S).abs() < 1e-8);
        assert!(dt <= stable / scale.sqrt() * 1.00001);
        assert!(scale > 0.0 && scale < 1.0);
        let (dt, steps, scale) = interactive_schedule(stable, true);
        assert_eq!(dt, stable);
        assert!(steps >= 111_535);
        assert_eq!(scale, 1.0);
    }
}

/// Labels for drawing: 1..=SKIN_LABELS are the figure's own colours (the corpse renderer's
/// palette), then two shades of muscle and the bone.
pub(super) const SKIN_LABELS: u8 = 12;
pub(super) const MUSCLE_LABELS: [u8; 2] = [SKIN_LABELS + 1, SKIN_LABELS + 2];
pub(super) const BONE_LABEL: u8 = SKIN_LABELS + 3;
/// An anatomical volume's other tissues draw as `BONE_LABEL + id` (`crew_anatomy` table ids up to
/// `MAX_ANATOMY_ID`), each in its table colour.
pub(super) const MAX_ANATOMY_ID: u8 = 48;
pub(super) const LABELS: usize = BONE_LABEL as usize + MAX_ANATOMY_ID as usize;

/// Review-only tuning (`KESSEL_TISSUE_TUNE="tear=2,stretch=1.5,outward=0.3,forward=0.95,grain=1"`):
/// scales on the bonds' tearing energy and critical stretch, the momentum shares a fragment gives
/// the cells about it, and the energy a landing piece needs to break into grains. Read only after
/// `allow_review_tune` (the `fea-film` and `fea-range` review tools call it); play and both shipping presets
/// never do, so every scale is 1 and the shares their constants there (test
/// `the_tune_is_the_constants_unless_the_review_allows_it`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Tune {
    pub tear: f32,
    pub stretch: f32,
    pub forward: f32,
    pub outward: f32,
    pub grain: f32,
    /// The radius the lost momentum is shared over, in multiples of the fragment's half-size plus
    /// half a cell.
    pub spread: f32,
}

static REVIEW_TUNE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The review harness's gate on `KESSEL_TISSUE_TUNE` (call before the first body is built).
pub(super) fn allow_review_tune() {
    REVIEW_TUNE.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn parse_tune(spec: Option<&str>) -> Tune {
    let mut t = Tune { tear: 1.0, stretch: 1.0, forward: FORWARD, outward: OUTWARD, grain: 1.0, spread: 1.0 };
    for (key, value) in spec.unwrap_or("").split(',').filter_map(|kv| kv.split_once('=')) {
        let Ok(v) = value.trim().parse::<f32>() else { continue };
        match key.trim() {
            "tear" => t.tear = v,
            "stretch" => t.stretch = v,
            "forward" => t.forward = v,
            "outward" => t.outward = v,
            "grain" => t.grain = v,
            "spread" => t.spread = v,
            _ => {}
        }
    }
    t
}

pub(super) fn tune() -> Tune {
    static TUNE: std::sync::OnceLock<Tune> = std::sync::OnceLock::new();
    *TUNE.get_or_init(|| {
        let spec = REVIEW_TUNE.load(std::sync::atomic::Ordering::Relaxed).then(|| std::env::var("KESSEL_TISSUE_TUNE").ok()).flatten();
        let t = parse_tune(spec.as_deref());
        if spec.is_some() {
            println!("[crew] tissue tuning (review only): {t:?}");
        }
        t
    })
}

/// The 18-neighbourhood, owned half (each bond stored once).
const OFFSETS: [[i32; 3]; 9] = [
    [1, 0, 0], [0, 1, 0], [0, 0, 1],
    [1, 1, 0], [1, -1, 0], [1, 0, 1], [1, 0, -1], [0, 1, 1], [0, 1, -1],
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tissue {
    Skin,
    Muscle,
    Bone,
    Fluid,
}

impl Tissue {
    fn index(self) -> usize {
        self as usize
    }
    fn soft(self) -> bool {
        !matches!(self, Tissue::Bone)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Cell {
    /// Centre in the relaxed figure's frame, and grid coordinates.
    pub at: Vec3,
    pub ijk: [u16; 3],
    pub tissue: Tissue,
    pub part: crew_physics::CrewBodyPart,
    /// The figure's atlas coordinate at the nearest point of its surface (a skin cell's colour).
    pub uv: Vec2,
    /// Its row in the anatomical tissue table (`crew_anatomy`); 0 in the procedural build.
    pub label: u8,
}

impl Cell {
    /// The body's outer shell, drawn in the figure's own colours (its uniform, hands and face).
    pub(super) fn outer(&self) -> bool {
        if self.label == 0 { self.tissue == Tissue::Skin } else { self.label == 1 }
    }

    /// The uniform: the outer shell everywhere but the head (2026-11-03, the owner: the uniform
    /// stood as an intact shell round a destroyed body).
    pub(super) fn cloth(&self) -> bool {
        self.outer() && self.part != crew_physics::CrewBodyPart::Head
    }
}

#[derive(Clone, Copy, Debug)]
struct Bond {
    a: u32,
    b: u32,
    rest_m: f32,
    stiffness: f32,
    critical: f32,
    /// The weaker cell's strength class (`Tissue::index`), whose law the bond follows.
    class: u8,
}

pub(super) struct TissueModel {
    /// Cell size (m).
    pub cell: f32,
    /// The anatomical volume it was built from, if any (receipts).
    pub anatomy: Option<String>,
    /// The anatomical table's colours for the tissues present (id, sRGB); empty when procedural.
    pub tissue_colours: Vec<(u8, [u8; 3])>,
    pub origin: Vec3,
    pub dims: [usize; 3],
    /// Cell index + 1 per grid cell; 0 outside the figure.
    grid: Vec<u32>,
    pub cells: Vec<Cell>,
    /// Each cell's mass (kg): its tissue's density over the cell.
    mass_kg: Vec<f32>,
    pub part_cells: [Vec<u32>; PARTS],
    bonds: Vec<Bond>,
    /// Bonds of each cell (both ends), CSR.
    bond_start: Vec<u32>,
    bond_list: Vec<u32>,
    pub soft_cells: [usize; PARTS],
    /// Cells near a joint: the segment across it and how far toward it the cell is drawn (linear
    /// blend skinning of the drawn cell only, so a bent joint stays closed; named).
    pub blend: Vec<Option<(crew_physics::CrewBodyPart, f32)>>,
}

impl TissueModel {
    pub(super) fn mass(&self, cell: usize) -> f32 {
        self.mass_kg[cell]
    }

    /// Whether a cell can be seen: one of its six faces is open to air or to a cell gone.
    pub(super) fn exposed(&self, present: &[bool], cell: usize) -> bool {
        let [x, y, z] = self.cells[cell].ijk.map(i32::from);
        [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]].iter()
            .any(|o| self.lookup([x + o[0], y + o[1], z + o[2]]).is_none_or(|n| !present[n]))
    }

    fn lookup(&self, ijk: [i32; 3]) -> Option<usize> {
        if ijk.iter().zip(self.dims).any(|(&v, d)| v < 0 || v as usize >= d) {
            return None;
        }
        let [i, j, k] = ijk.map(|v| v as usize);
        let packed = self.grid[(j * self.dims[2] + k) * self.dims[0] + i];
        (packed != 0).then(|| packed as usize - 1)
    }

    fn cell_at(&self, p: Vec3) -> Option<usize> {
        let g = ((p - self.origin) / self.cell).floor();
        self.lookup([g.x as i32, g.y as i32, g.z as i32])
    }

    fn bonds_of(&self, cell: usize) -> &[u32] {
        &self.bond_list[self.bond_start[cell] as usize..self.bond_start[cell + 1] as usize]
    }

    pub(super) fn cell_count(&self) -> usize {
        self.cells.len()
    }
}

fn barycentric(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let (d00, d01, d11) = (v0.dot(v0), v0.dot(v1), v1.dot(v1));
    let (d20, d21) = (v2.dot(v0), v2.dot(v1));
    let denominator = d00 * d11 - d01 * d01;
    if denominator.abs() < 1.0e-12 {
        return Vec3::new(1.0, 0.0, 0.0);
    }
    let v = (d11 * d20 - d01 * d21) / denominator;
    let w = (d00 * d21 - d01 * d20) / denominator;
    Vec3::new(1.0 - v - w, v, w)
}

/// Triangle–box overlap by separating axes (Akenine-Möller 2001): the box's three axes, the
/// triangle's normal and the nine edge cross products.
pub(super) fn triangle_box_overlap(centre: Vec3, half: Vec3, tri: [Vec3; 3]) -> bool {
    let v = tri.map(|p| p - centre);
    let edges = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
    let separated = |axis: Vec3| {
        if axis.length_squared() < 1.0e-12 {
            return false;
        }
        let p = v.map(|q| q.dot(axis));
        let r = half.x * axis.x.abs() + half.y * axis.y.abs() + half.z * axis.z.abs();
        p.iter().cloned().fold(f32::INFINITY, f32::min) > r || p.iter().cloned().fold(f32::NEG_INFINITY, f32::max) < -r
    };
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        if separated(axis) {
            return false;
        }
    }
    if separated(edges[0].cross(edges[1])) {
        return false;
    }
    for e in edges {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            if separated(axis.cross(e)) {
                return false;
            }
        }
    }
    true
}

/// Fill the relaxed `figure` with cells and voxelise the fitted skeleton `bones` (in the figure's
/// frame) onto the same grid, then bond every cell to its neighbours.
pub(super) fn build_tissue(figure: &CpuMesh, bones: &[Option<CpuMesh>; PARTS]) -> TissueModel {
    let model = match crate::crew_anatomy::load() {
        Ok(Some(volume)) => build_anatomical(figure, &volume),
        Ok(None) => build_procedural(figure, bones),
        Err(error) => {
            eprintln!("[crew] the anatomical volume could not be read ({error:#}); the figure and skeleton build the tissue");
            build_procedural(figure, bones)
        }
    };
    if reference_solve() { model } else { coarsen_tissue(model, 3) }
}

/// The figure's atlas coordinate at its surface point nearest `at`.
fn nearest_uv(figure: &CpuMesh, at: Vec3) -> Vec2 {
    let corner = |t: usize, i: usize| figure.vertices[figure.indices[3 * t + i] as usize];
    let points = |t: usize| [0, 1, 2].map(|i| Vec3::from(corner(t, i).pos));
    let mut nearest = (f32::INFINITY, 0usize, at);
    for t in 0..figure.indices.len() / 3 {
        let [a, b, c] = points(t);
        let q = crew_skeleton::closest_point_on_triangle(at, a, b, c);
        let d = q.distance_squared(at);
        if d < nearest.0 {
            nearest = (d, t, q);
        }
    }
    let [a, b, c] = points(nearest.1);
    let w = barycentric(nearest.2, a, b, c);
    [0, 1, 2].iter().zip([w.x, w.y, w.z]).map(|(&n, weight)| Vec2::from(corner(nearest.1, n).uv) * weight).sum::<Vec2>()
}

/// The tissue from an anatomical volume (`crew_anatomy`): every voxel of a tissue that is not a
/// gas is a cell of its label, its strength class's bonds and its table density's mass; the outer
/// shell takes the figure's colours at its nearest surface point.
/// Aggregate filled anatomical cells, preserving their mass and contents. This reduces both
/// render cubes and spring bonds; it does not turn the body into a hollow surface shell.
fn coarsen_tissue(model: TissueModel, factor: usize) -> TissueModel {
    let dims = model.dims.map(|n| n.div_ceil(factor));
    let h = model.cell * factor as f32;
    let mut bins: std::collections::BTreeMap<[usize; 3], Vec<usize>> = std::collections::BTreeMap::new();
    for (i,c) in model.cells.iter().enumerate() {
        bins.entry(c.ijk.map(|v| v as usize / factor)).or_default().push(i);
    }
    let mut cells = Vec::with_capacity(bins.len());
    let mut masses = Vec::with_capacity(bins.len());
    let mut grid = vec![0u32; dims.iter().product()];
    for (ijk, members) in bins {
        let mass: f32 = members.iter().map(|&i| model.mass_kg[i]).sum();
        let at = model.origin + (Vec3::from_array(ijk.map(|v| v as f32)) + 0.5) * h;
        let source = *members.iter().min_by(|&&a,&&b| model.cells[a].at.distance_squared(at)
            .total_cmp(&model.cells[b].at.distance_squared(at))).unwrap();
        let mut c = model.cells[source];
        c.at = at; c.ijk = ijk.map(|v| v as u16);
        grid[(ijk[1] * dims[2] + ijk[2]) * dims[0] + ijk[0]] = cells.len() as u32 + 1;
        cells.push(c); masses.push(mass);
    }
    println!("[crew] interactive tissue: {} -> {} filled cells, {:.0} mm; mass preserved", model.cells.len(), cells.len(), h*1000.0);
    let mut coarse = finish_model(h, model.anatomy, model.origin, dims, grid, cells, masses);
    coarse.tissue_colours = model.tissue_colours;
    coarse
}

fn build_anatomical(figure: &CpuMesh, volume: &crate::crew_anatomy::Volume) -> TissueModel {
    use crate::crew_anatomy::Strength;
    let started = std::time::Instant::now();
    let dims = volume.dims;
    let cell_m = volume.cell_m;
    let mut cells: Vec<Cell> = Vec::new();
    let mut mass_kg: Vec<f32> = Vec::new();
    for j in 0..dims[1] {
        for k in 0..dims[2] {
            for i in 0..dims[0] {
                let label = volume.label(i, j, k);
                let Some(row) = volume.tissue(label) else { continue };
                let tissue = match row.strength {
                    Strength::Skin => Tissue::Skin,
                    Strength::Muscle => Tissue::Muscle,
                    Strength::Bone => Tissue::Bone,
                    Strength::Fluid => Tissue::Fluid,
                    Strength::Gas => continue,
                };
                let at = volume.origin + (Vec3::new(i as f32, j as f32, k as f32) + 0.5) * cell_m;
                cells.push(Cell { at, ijk: [i as u16, j as u16, k as u16], tissue,
                    part: crew_physics::classify_relaxed_mesh_point(at), uv: Vec2::ZERO, label });
                mass_kg.push(row.density_kg_m3 * cell_m.powi(3));
            }
        }
    }
    // The outer shell's colours, from the figure's atlas (threads over the shell's cells).
    let shell: Vec<usize> = (0..cells.len()).filter(|&n| cells[n].outer()).collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 12);
    let uvs: Vec<Vec<(usize, Vec2)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads).map(|thread| {
            let (shell, cells) = (&shell, &cells);
            scope.spawn(move || shell.iter().skip(thread).step_by(threads).map(|&n| (n, nearest_uv(figure, cells[n].at))).collect())
        }).collect();
        handles.into_iter().map(|h| h.join().expect("anatomy shell colours")).collect()
    });
    for (n, uv) in uvs.into_iter().flatten() {
        cells[n].uv = uv;
    }
    let mut grid = vec![0u32; dims[0] * dims[1] * dims[2]];
    for (n, cell) in cells.iter().enumerate() {
        let [i, j, k] = cell.ijk.map(|v| v as usize);
        grid[(j * dims[2] + k) * dims[0] + i] = n as u32 + 1;
    }
    let mut model = finish_model(cell_m, Some(volume.source.clone()), volume.origin, dims, grid, cells, mass_kg);
    let present: std::collections::BTreeSet<u8> = model.cells.iter().map(|c| c.label).collect();
    model.tissue_colours = volume.tissues.iter().filter(|t| present.contains(&t.id)).map(|t| (t.id, t.colour)).collect();
    println!("[crew] anatomical tissue from {}: {} cells of {:.0} mm in {} tissues, {:.1} kg, built in {:.0} ms",
        volume.source, model.cells.len(), cell_m * 1000.0,
        model.cells.iter().map(|c| c.label).collect::<std::collections::BTreeSet<u8>>().len(),
        model.mass_kg.iter().sum::<f32>(), started.elapsed().as_secs_f32() * 1000.0);
    model
}

/// The figure's cells, skin within one cell of its surface and muscle deeper, and bone where the
/// fitted skeleton's triangles pass (see the module doc).
pub(super) fn build_procedural(figure: &CpuMesh, bones: &[Option<CpuMesh>; PARTS]) -> TissueModel {
    let triangles = figure.indices.len() / 3;
    let corner = |t: usize, i: usize| figure.vertices[figure.indices[3 * t + i] as usize];
    let points = |t: usize| [0, 1, 2].map(|i| Vec3::from(corner(t, i).pos));
    let (lo, hi) = (figure.min, figure.max);
    let origin = lo;
    let span = ((hi - lo) / CELL_M).ceil();
    let dims = [span.x as usize, span.y as usize, span.z as usize];
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 12);
    let rows: Vec<Vec<Cell>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads).map(|thread| {
            let points = &points;
            scope.spawn(move || {
                let mut out = Vec::new();
                for j in (thread..dims[1]).step_by(threads) {
                    for k in 0..dims[2] {
                        for i in 0..dims[0] {
                            let at = origin + (Vec3::new(i as f32, j as f32, k as f32) + 0.5) * CELL_M;
                            if !crew_skeleton::inside(at, figure) {
                                continue;
                            }
                            let mut nearest = (f32::INFINITY, 0usize, at);
                            for t in 0..triangles {
                                let [a, b, c] = points(t);
                                let q = crew_skeleton::closest_point_on_triangle(at, a, b, c);
                                let d = q.distance_squared(at);
                                if d < nearest.0 {
                                    nearest = (d, t, q);
                                }
                            }
                            let [a, b, c] = points(nearest.1);
                            let w = barycentric(nearest.2, a, b, c);
                            let uv = [0, 1, 2].iter().zip([w.x, w.y, w.z])
                                .map(|(&n, weight)| Vec2::from(corner(nearest.1, n).uv) * weight)
                                .sum::<Vec2>();
                            let tissue = if nearest.0.sqrt() < CELL_M { Tissue::Skin } else { Tissue::Muscle };
                            out.push(Cell { at, ijk: [i as u16, j as u16, k as u16], tissue,
                                part: crew_physics::classify_relaxed_mesh_point(at), uv, label: 0 });
                        }
                    }
                }
                out
            })
        }).collect();
        handles.into_iter().map(|h| h.join().expect("tissue builder")).collect()
    });
    let mut cells: Vec<Cell> = rows.into_iter().flatten().collect();
    // A stable order (the threads interleave rows).
    cells.sort_by_key(|c| (c.ijk[1], c.ijk[2], c.ijk[0]));
    let mut grid = vec![0u32; dims[0] * dims[1] * dims[2]];
    for (n, cell) in cells.iter().enumerate() {
        let [i, j, k] = cell.ijk.map(|v| v as usize);
        grid[(j * dims[2] + k) * dims[0] + i] = n as u32 + 1;
    }
    // The skeleton's triangles, voxelised: every figure cell a triangle passes through is bone.
    let half = Vec3::splat(0.5 * CELL_M);
    for page in bones.iter().flatten() {
        for t in page.indices.chunks_exact(3) {
            let tri = [0, 1, 2].map(|i| Vec3::from(page.vertices[t[i] as usize].pos));
            let lo = ((tri[0].min(tri[1]).min(tri[2]) - origin) / CELL_M).floor().as_ivec3().max(glam::IVec3::ZERO);
            let hi = ((tri[0].max(tri[1]).max(tri[2]) - origin) / CELL_M).floor().as_ivec3()
                .min(glam::IVec3::new(dims[0] as i32 - 1, dims[1] as i32 - 1, dims[2] as i32 - 1));
            for j in lo.y..=hi.y {
                for k in lo.z..=hi.z {
                    for i in lo.x..=hi.x {
                        let packed = grid[(j as usize * dims[2] + k as usize) * dims[0] + i as usize];
                        if packed == 0 {
                            continue;
                        }
                        let cell = &mut cells[packed as usize - 1];
                        if cell.tissue != Tissue::Bone && triangle_box_overlap(cell.at, half, tri) {
                            cell.tissue = Tissue::Bone;
                        }
                    }
                }
            }
        }
    }
    let mass_kg: Vec<f32> = cells.iter().map(|c| DENSITY[c.tissue.index()] * CELL_M.powi(3)).collect();
    finish_model(CELL_M, None, origin, dims, grid, cells, mass_kg)
}

/// The parts' cells, the joints' blend and the bonds, for either build.
/// The part classifier works point by point, so a few cells of one part can lie apart from the
/// rest of it, bonded only to its neighbours (the relaxed figure's left hand on the thigh: 121
/// forearm cells and 103 thigh cells). Such an island would come away whole, unhurt, whenever its
/// part was struck; each one joins the part it touches most instead.
fn rehome_part_islands(dims: [usize; 3], grid: &[u32], cells: &mut [Cell]) -> usize {
    let lookup = |ijk: [i32; 3]| -> Option<usize> {
        if ijk.iter().zip(dims).any(|(&v, d)| v < 0 || v as usize >= d) {
            return None;
        }
        let [i, j, k] = ijk.map(|v| v as usize);
        let packed = grid[(j * dims[2] + k) * dims[0] + i];
        (packed != 0).then(|| packed as usize - 1)
    };
    let neighbours = |ijk: [u16; 3]| {
        let [x, y, z] = ijk.map(i32::from);
        OFFSETS.iter().flat_map(move |o| [[x + o[0], y + o[1], z + o[2]], [x - o[0], y - o[1], z - o[2]]])
    };
    let mut moved = 0;
    for _ in 0..4 {
        let n = cells.len();
        let mut parent: Vec<u32> = (0..n as u32).collect();
        for a in 0..n {
            for ijk in neighbours(cells[a].ijk) {
                if let Some(b) = lookup(ijk).filter(|&b| cells[b].part == cells[a].part) {
                    let (ra, rb) = (find(&mut parent, a as u32), find(&mut parent, b as u32));
                    if ra != rb {
                        parent[ra.max(rb) as usize] = ra.min(rb);
                    }
                }
            }
        }
        let mut pieces: std::collections::BTreeMap<u32, Vec<usize>> = std::collections::BTreeMap::new();
        for c in 0..n {
            pieces.entry(find(&mut parent, c as u32)).or_default().push(c);
        }
        let mut main = [None::<(usize, u32)>; PARTS];
        for (&root, piece) in &pieces {
            let slot = &mut main[cells[piece[0]].part.index()];
            if slot.is_none_or(|(len, _)| piece.len() > len) {
                *slot = Some((piece.len(), root));
            }
        }
        let mut changed = false;
        for (root, piece) in pieces {
            if main[cells[piece[0]].part.index()].is_some_and(|(_, m)| m == root) {
                continue;
            }
            let own = cells[piece[0]].part;
            let mut touch = [0usize; PARTS];
            for &c in &piece {
                for ijk in neighbours(cells[c].ijk) {
                    if let Some(b) = lookup(ijk).filter(|&b| cells[b].part != own) {
                        touch[cells[b].part.index()] += 1;
                    }
                }
            }
            let Some(best) = (0..PARTS).filter(|&p| touch[p] > 0).max_by_key(|&p| touch[p]) else { continue };
            let to = crew_physics::CrewBodyPart::ALL[best];
            for &c in &piece {
                cells[c].part = to;
            }
            moved += piece.len();
            changed = true;
        }
        if !changed {
            break;
        }
    }
    moved
}

fn finish_model(cell_m: f32, anatomy: Option<String>, origin: Vec3, dims: [usize; 3], grid: Vec<u32>, mut cells: Vec<Cell>,
    mass_kg: Vec<f32>) -> TissueModel {
    rehome_part_islands(dims, &grid, &mut cells);
    let mut part_cells: [Vec<u32>; PARTS] = std::array::from_fn(|_| Vec::new());
    let mut soft_cells = [0usize; PARTS];
    for (n, cell) in cells.iter().enumerate() {
        part_cells[cell.part.index()].push(n as u32);
        if cell.tissue.soft() {
            soft_cells[cell.part.index()] += 1;
        }
    }
    // Bonds: each pair's energy to break is its weaker cell's tearing energy over a full
    // neighbourhood; its stiffness follows from that energy at that cell's critical stretch.
    let mut model = TissueModel { cell: cell_m, anatomy, tissue_colours: Vec::new(), origin, dims, grid, cells, mass_kg, part_cells, bonds: Vec::new(),
        bond_start: Vec::new(), bond_list: Vec::new(), soft_cells, blend: Vec::new() };
    // Joints: a cell within 3.5 cells of the segment across a joint (parent or child) is drawn
    // pulled toward it, half way at the boundary, nothing at 3.5 cells.
    const BLEND_CELLS: f32 = 3.5;
    let reach = BLEND_CELLS.ceil() as i32;
    let joint = |a: crew_physics::CrewBodyPart, b: crew_physics::CrewBodyPart|
        crew_anatomical_parent(a) == Some(b) || crew_anatomical_parent(b) == Some(a);
    model.blend = model.cells.iter().map(|cell| {
        let [x, y, z] = cell.ijk.map(i32::from);
        let mut best: Option<(crew_physics::CrewBodyPart, f32)> = None;
        for dz in -reach..=reach {
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let d = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
                    if d > BLEND_CELLS || best.is_some_and(|(_, b)| b <= d) {
                        continue;
                    }
                    if let Some(n) = model.lookup([x + dx, y + dy, z + dz]) {
                        let other = model.cells[n].part;
                        if other != cell.part && joint(cell.part, other) {
                            best = Some((other, d));
                        }
                    }
                }
            }
        }
        best.map(|(other, d)| (other, 0.5 * (1.0 - (d - 1.0).max(0.0) / (BLEND_CELLS - 1.0)).clamp(0.0, 1.0)))
    }).collect();
    let mut bonds = Vec::new();
    for (a, cell) in model.cells.iter().enumerate() {
        let [x, y, z] = cell.ijk.map(i32::from);
        for o in OFFSETS {
            let Some(b) = model.lookup([x + o[0], y + o[1], z + o[2]]) else { continue };
            // The weaker cell's hold (the lower tearing energy), and its stretch.
            let weaker = if TEAR_J_M3[cell.tissue.index()] <= TEAR_J_M3[model.cells[b].tissue.index()] { cell.tissue }
                else { model.cells[b].tissue }.index();
            let rest_m = cell_m * ((o[0] * o[0] + o[1] * o[1] + o[2] * o[2]) as f32).sqrt();
            let energy = TEAR_J_M3[weaker] * tune().tear * cell_m.powi(3) / 18.0;
            let mut critical = CRITICAL_STRETCH[weaker] * tune().stretch;
            if cell.cloth() || model.cells[b].cloth() {
                critical = critical.min(CLOTH_BREAKING_STRAIN);
            }
            let stiffness = 2.0 * energy / (critical * rest_m).powi(2);
            bonds.push(Bond { a: a as u32, b: b as u32, rest_m, stiffness, critical, class: weaker as u8 });
        }
    }
    let mut counts = vec![0u32; model.cells.len() + 1];
    for bond in &bonds {
        counts[bond.a as usize + 1] += 1;
        counts[bond.b as usize + 1] += 1;
    }
    for n in 1..counts.len() {
        counts[n] += counts[n - 1];
    }
    let mut fill = counts.clone();
    let mut list = vec![0u32; bonds.len() * 2];
    for (n, bond) in bonds.iter().enumerate() {
        for end in [bond.a, bond.b] {
            list[fill[end as usize] as usize] = n as u32;
            fill[end as usize] += 1;
        }
    }
    model.bonds = bonds;
    model.bond_start = counts;
    model.bond_list = list;
    model
}

/// One strike on one part, in the relaxed figure's frame: where it meets the part and the
/// direction it travels, with the fragment's energy and speed; or, with no direction, a burst's
/// pressure work on the part from the burst's own point `at`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Strike {
    pub part: crew_physics::CrewBodyPart,
    pub at: Vec3,
    pub dir: Vec3,
    pub work_j: f32,
    pub speed_mps: f32,
}

/// A piece that came away from a body: its cells, their centre (figure frame), and its motion.
#[derive(Clone, Debug)]
pub(super) struct Chunk {
    pub part: crew_physics::CrewBodyPart,
    pub cells: Vec<u32>,
    pub centre: Vec3,
    pub velocity: Vec3,
    pub spin: Vec3,
    /// The energy its own intact bonds hold it together with (J).
    pub cohesion_j: f32,
}

/// What one solve did: the pieces that came away, fragments that stopped in him, bonds broken.
#[derive(Clone, Debug, Default)]
pub(super) struct Solve {
    pub chunks: Vec<Chunk>,
    pub lodged: usize,
    pub bonds_broken: usize,
    /// Cells crushed along a fragment's track (its permanent cavity), drawn as gone.
    pub crushed: usize,
    /// Cloth cells left with nothing of the body under them, blown away as single cells.
    pub cloth_shredded: usize,
    pub steps: usize,
}

/// One dead man's body: which cells are still part of it, which bonds still hold.
#[derive(Clone, Debug)]
pub(super) struct CasualtyTissue {
    pub present: Vec<bool>,
    pub broken: Vec<bool>,
    /// Bumped whenever a cell comes away; drawing caches compare it.
    pub revision: u32,
}

/// Disjoint sets for the connectivity pass.
fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

impl CasualtyTissue {
    pub(super) fn new(model: &TissueModel) -> Self {
        Self { present: vec![true; model.cells.len()], broken: vec![false; model.bonds.len()], revision: 0 }
    }

    pub(super) fn joint_connections(&self,model:&TissueModel)->[bool;PARTS]{
        let mut total=[0usize;PARTS];let mut intact=[0usize;PARTS];
        for (i,bond) in model.bonds.iter().enumerate(){
            let a=model.cells[bond.a as usize].part;let b=model.cells[bond.b as usize].part;
            let child=if crew_anatomical_parent(a)==Some(b){Some(a)}else if crew_anatomical_parent(b)==Some(a){Some(b)}else{None};
            if let Some(child)=child {let j=child.index();total[j]+=1;
                if !self.broken[i]&&self.present[bond.a as usize]&&self.present[bond.b as usize]{intact[j]+=1;}}
        }
        std::array::from_fn(|i|total[i]==0||intact[i]>0)
    }
    /// Whether any cell of `part` has come away.
    #[cfg(test)]
    pub(super) fn struck(&self, model: &TissueModel, part: crew_physics::CrewBodyPart) -> bool {
        model.part_cells[part.index()].iter().any(|&c| !self.present[c as usize])
    }

    /// The share of the part's soft cells still on the body.
    pub(super) fn soft_share(&self, model: &TissueModel, part: crew_physics::CrewBodyPart) -> f32 {
        let total = model.soft_cells[part.index()];
        if total == 0 {
            return 1.0;
        }
        let left = model.part_cells[part.index()].iter()
            .filter(|&&c| self.present[c as usize] && model.cells[c as usize].tissue.soft()).count();
        left as f32 / total as f32
    }

    /// Solve one killing event's strikes on the lattice (see the module doc).
    pub(super) fn solve(&mut self, model: &TissueModel, strikes: &[Strike]) -> Solve {
        // Interactive hits must not create thousands of OS threads. Keep the unrestricted
        // material solve available to the offline calculator explicitly.
        let reference = std::env::var("KESSEL_TISSUE_SOLVE").as_deref() == Ok("reference");
        let threads = if reference { std::thread::available_parallelism().map_or(1, |t| t.get()).clamp(1, SOLVE_THREADS) } else { 1 };
        self.solve_with_threads(model, strikes, threads)
    }

    fn solve_with_threads(&mut self, model: &TissueModel, strikes: &[Strike], threads: usize) -> Solve {
        let n = model.cells.len();
        let mut struck = [false; PARTS];
        for strike in strikes.iter().filter(|s| s.work_j.is_finite() && s.work_j > 0.0) {
            struck[strike.part.index()] = true;
        }
        if !struck.iter().any(|s| *s) {
            return Solve::default();
        }
        // Cells near a fragment's path, and every cell of a part a burst works on, move; the rest of
        // him holds them (anchored).
        let tuning = tissue_tuning();
        let law = bond_law_scale(&tuning);
        // A bond's (stiffness, critical stretch) under the tuning in force.
        let bond_law = |bond: &Bond| {
            let (k, c) = law[bond.class as usize];
            (bond.stiffness * k, bond.critical * c)
        };
        let mut active = vec![false; n];
        for strike in strikes.iter().filter(|s| s.work_j.is_finite() && s.work_j > 0.0) {
            let dir = strike.dir.normalize_or_zero();
            for &c in &model.part_cells[strike.part.index()] {
                let c = c as usize;
                if !self.present[c] {
                    continue;
                }
                let d = model.cells[c].at - strike.at;
                let along = d.dot(dir);
                if dir == Vec3::ZERO || strike.speed_mps <= STOPPED_MPS
                    || (along > -tuning.active_reach_m && (d - dir * along).length() < tuning.active_reach_m) {
                    active[c] = true;
                }
            }
        }
        let moving: Vec<usize> = (0..n).filter(|&c| active[c]).collect();
        let mass = &model.mass_kg;
        let mut vel = vec![Vec3::ZERO; n];
        let mut out = Solve::default();
        let mut crushed: Vec<usize> = Vec::new();
        for strike in strikes.iter().filter(|s| s.work_j.is_finite() && s.work_j > 0.0) {
            let dir = strike.dir.normalize_or_zero();
            if dir == Vec3::ZERO || strike.speed_mps <= STOPPED_MPS {
                self.push_pressure(model, strike, &active, mass, &mut vel);
            } else if self.push_fragment(model, strike, dir, &active, mass, &mut vel, &tuning, &mut crushed) {
                out.lodged += 1;
            }
        }
        // The track itself: what the fragment passed through is crushed (its permanent cavity) and
        // drawn as gone; its pulp and blood are the blood system's (named).
        for c in crushed {
            if self.present[c] {
                self.present[c] = false;
                out.crushed += 1;
            }
        }
        if out.crushed > 0 {
            self.revision = self.revision.wrapping_add(1);
        }
        // The lattice: explicit symplectic Euler at a tenth of the stiffest moving cell's period.
        let live: Vec<u32> = (0..model.bonds.len() as u32).filter(|&b| {
            let bond = model.bonds[b as usize];
            !self.broken[b as usize] && (active[bond.a as usize] || active[bond.b as usize])
                && self.present[bond.a as usize] && self.present[bond.b as usize]
        }).collect();
        let mut stiffness = vec![0.0f32; n];
        for &b in &live {
            let bond = model.bonds[b as usize];
            let (k, _) = bond_law(&bond);
            stiffness[bond.a as usize] += k;
            stiffness[bond.b as usize] += k;
        }
        let period = (0..n).filter(|&c| active[c] && stiffness[c] > 0.0)
            .map(|c| (mass[c] / stiffness[c]).sqrt()).fold(f32::INFINITY, f32::min);
        if !period.is_finite() {
            return out;
        }
        let stable_dt = 0.6 * period;
        let reference = std::env::var("KESSEL_TISSUE_SOLVE").as_deref() == Ok("reference");
        let (dt, steps, stiffness_scale) = interactive_schedule(stable_dt, reference);
        // Explicit real-time approximation: soften excessively stiff springs, rather than
        // using an unstable enlarged timestep or stopping before the physical interval ends.
        // Mass, initial momentum, fracture strain and the 8 ms interval remain unchanged.
        let bond_law = |bond: &Bond| {
            let (k, c) = law[bond.class as usize];
            (bond.stiffness * k * stiffness_scale, bond.critical * c)
        };
        // In parallel and the same for any thread count: each live bond's force from both ends'
        // displacements, then each moving cell gathers its bonds' forces in a fixed order.
        let threads = threads.max(1);
        let mut slot = vec![u32::MAX; n];
        for (i, &c) in moving.iter().enumerate() {
            slot[c] = i as u32;
        }
        let mut gather_start = vec![0u32; moving.len() + 1];
        for &b in &live {
            let bond = model.bonds[b as usize];
            for end in [bond.a, bond.b] {
                if slot[end as usize] != u32::MAX {
                    gather_start[slot[end as usize] as usize + 1] += 1;
                }
            }
        }
        for i in 1..gather_start.len() {
            gather_start[i] += gather_start[i - 1];
        }
        let mut fill = gather_start.clone();
        let mut gather = vec![(0u32, 0.0f32); *gather_start.last().unwrap_or(&0) as usize];
        for (li, &b) in live.iter().enumerate() {
            let bond = model.bonds[b as usize];
            for (end, sign) in [(bond.a, 1.0f32), (bond.b, -1.0)] {
                let m = slot[end as usize];
                if m != u32::MAX {
                    gather[fill[m as usize] as usize] = (li as u32, sign);
                    fill[m as usize] += 1;
                }
            }
        }
        let mut intact: Vec<bool> = live.iter().map(|&b| !self.broken[b as usize]).collect();
        let mut bond_force = vec![Vec3::ZERO; live.len()];
        let mut mdisp = vec![Vec3::ZERO; moving.len()];
        let mut mvel: Vec<Vec3> = moving.iter().map(|&c| vel[c]).collect();
        let bond_chunk = live.len().div_ceil(threads).max(1);
        let cell_chunk = moving.len().div_ceil(threads).max(1);
        for _ in 0..steps {
            let (slot, mdisp_ref) = (&slot, &mdisp);
            std::thread::scope(|scope| {
                for ((bonds, alive), forces) in live.chunks(bond_chunk).zip(intact.chunks_mut(bond_chunk)).zip(bond_force.chunks_mut(bond_chunk)) {
                    let mut work = move || {
                        for i in 0..bonds.len() {
                            if !alive[i] {
                                forces[i] = Vec3::ZERO;
                                continue;
                            }
                            let bond = model.bonds[bonds[i] as usize];
                            let (bond_stiffness, bond_critical) = bond_law(&bond);
                            let at = |cell: u32| {
                                let m = slot[cell as usize];
                                model.cells[cell as usize].at + if m == u32::MAX { Vec3::ZERO } else { mdisp_ref[m as usize] }
                            };
                            let d = at(bond.b) - at(bond.a);
                            let length = d.length();
                            if ((length - bond.rest_m) / bond.rest_m).abs() > bond_critical {
                                alive[i] = false;
                                forces[i] = Vec3::ZERO;
                                continue;
                            }
                            forces[i] = d / length.max(1.0e-9) * (bond_stiffness * (length - bond.rest_m));
                        }
                    };
                    if threads == 1 { work(); } else { scope.spawn(work); }
                }
            });
            let (gather, gather_start, bond_force, moving) = (&gather, &gather_start, &bond_force, &moving);
            std::thread::scope(|scope| {
                for (k, (vels, disps)) in mvel.chunks_mut(cell_chunk).zip(mdisp.chunks_mut(cell_chunk)).enumerate() {
                    let mut work = move || {
                        for j in 0..vels.len() {
                            let m = k * cell_chunk + j;
                            let mut f = Vec3::ZERO;
                            for &(li, sign) in &gather[gather_start[m] as usize..gather_start[m + 1] as usize] {
                                f += bond_force[li as usize] * sign;
                            }
                            let c = moving[m];
                            vels[j] = (vels[j] + f / mass[c] * dt) * (1.0 - DAMPING);
                            disps[j] += vels[j] * dt;
                        }
                    };
                    if threads == 1 { work(); } else { scope.spawn(work); }
                }
            });
        }
        for (li, &b) in live.iter().enumerate() {
            if !intact[li] && !self.broken[b as usize] {
                self.broken[b as usize] = true;
                out.bonds_broken += 1;
            }
        }
        let mut disp = vec![Vec3::ZERO; n];
        for (m, &c) in moving.iter().enumerate() {
            disp[c] = mdisp[m];
            vel[c] = mvel[m];
        }
        out.steps = steps;
        if std::env::var("KESSEL_TISSUE_DEBUG").as_deref() == Ok("cells") {
            for c in 0..n {
                if active[c] && disp[c].length() > 0.004 {
                    let intact: Vec<[u16; 3]> = model.bonds_of(c).iter().filter(|&&b| !self.broken[b as usize]).map(|&b| {
                        let bond = model.bonds[b as usize];
                        model.cells[if bond.a as usize == c { bond.b } else { bond.a } as usize].ijk
                    }).collect();
                    println!("[tissue] cell {:?} {:?} moved {:.3?} at {:.1?} m/s; intact to {intact:?}", model.cells[c].ijk,
                        model.cells[c].tissue, disp[c], vel[c]);
                }
            }
        }
        // Cloth with nothing of the body left under it does not stand as a shell: a cloth cell of
        // a struck part is held when it keeps an intact bond to a present cell of flesh, or one to
        // cloth that is (a shell's edges and corners lie over cloth, not flesh). Every other one
        // comes away on its own, with its own motion (shredded and blown off; named: a cell, not a
        // hanging rag).
        let flesh_held = |tissue: &Self, c: usize| model.bonds_of(c).iter().any(|&b| {
            let bond = model.bonds[b as usize];
            let other = if bond.a as usize == c { bond.b } else { bond.a } as usize;
            !tissue.broken[b as usize] && tissue.present[other] && !model.cells[other].cloth()
        });
        let cloth: Vec<usize> = crew_physics::CrewBodyPart::ALL.into_iter().filter(|p| struck[p.index()])
            .flat_map(|part| model.part_cells[part.index()].iter().map(|&c| c as usize))
            .filter(|&c| self.present[c] && model.cells[c].cloth()).collect();
        let held: std::collections::HashSet<usize> = cloth.iter().copied().filter(|&c| flesh_held(self, c)).collect();
        let unsupported: Vec<usize> = cloth.iter().copied().filter(|c| !held.contains(c)).filter(|&c| !model.bonds_of(c).iter().any(|&b| {
            let bond = model.bonds[b as usize];
            let other = if bond.a as usize == c { bond.b } else { bond.a } as usize;
            !self.broken[b as usize] && held.contains(&other)
        })).collect();
        for &c in &unsupported {
            self.present[c] = false;
            out.cloth_shredded += 1;
            out.chunks.push(Chunk { part: model.cells[c].part, cells: vec![c as u32], centre: model.cells[c].at,
                velocity: vel[c], spin: Vec3::ZERO, cohesion_j: 0.0 });
        }
        if !unsupported.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        // Each struck part keeps its largest piece still bonded together; every other piece of it
        // comes away (bonds across parts do not hold a piece on: the parts are separate bodies).
        let mut parent: Vec<u32> = (0..n as u32).collect();
        for (b, bond) in model.bonds.iter().enumerate() {
            let (a, c) = (bond.a as usize, bond.b as usize);
            if !self.broken[b] && self.present[a] && self.present[c] && model.cells[a].part == model.cells[c].part {
                let (ra, rc) = (find(&mut parent, bond.a), find(&mut parent, bond.b));
                if ra != rc {
                    parent[ra.max(rc) as usize] = ra.min(rc);
                }
            }
        }
        for part in crew_physics::CrewBodyPart::ALL.into_iter().filter(|p| struck[p.index()]) {
            let mut pieces: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
            for &c in &model.part_cells[part.index()] {
                if self.present[c as usize] {
                    pieces.entry(find(&mut parent, c)).or_default().push(c);
                }
            }
            let Some(main) = pieces.iter().max_by_key(|(root, cells)| (cells.len(), std::cmp::Reverse(**root))).map(|(root, _)| *root) else {
                continue;
            };
            for (root, cells) in pieces {
                if root == main {
                    continue;
                }
                let total: f32 = cells.iter().map(|&c| mass[c as usize]).sum();
                let centre = cells.iter().map(|&c| model.cells[c as usize].at * mass[c as usize]).sum::<Vec3>() / total;
                let velocity = cells.iter().map(|&c| vel[c as usize] * mass[c as usize]).sum::<Vec3>() / total;
                let (mut momentum, mut inertia) = (Vec3::ZERO, 0.0f32);
                for &c in &cells {
                    let r = model.cells[c as usize].at + disp[c as usize] - centre;
                    momentum += r.cross(vel[c as usize] - velocity) * mass[c as usize];
                    inertia += mass[c as usize] * (r.length_squared() + model.cell * model.cell / 6.0);
                }
                for &c in &cells {
                    self.present[c as usize] = false;
                }
                let inside: std::collections::HashSet<u32> = cells.iter().copied().collect();
                // Each intact bond inside it once (from the cell that owns it), at its breaking energy.
                let cohesion_j: f32 = cells.iter().flat_map(|&c| model.bonds_of(c as usize).iter().map(move |&b| (c, b)))
                    .filter(|&(c, b)| {
                        let bond = model.bonds[b as usize];
                        !self.broken[b as usize] && bond.a == c && inside.contains(&bond.b)
                    }).map(|(_, b)| {
                        let (k, c) = bond_law(&model.bonds[b as usize]);
                        0.5 * k * (c * model.bonds[b as usize].rest_m).powi(2)
                    }).sum();
                out.chunks.push(Chunk { part, cells, centre, velocity, spin: momentum / inertia.max(1.0e-9), cohesion_j });
            }
        }
        if !out.chunks.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        out
    }

    /// A fragment through the part's cells: Newtonian drag in the tissue it crosses, its lost
    /// momentum given to the cells about it. True when it stopped inside.
    fn push_fragment(&self, model: &TissueModel, strike: &Strike, dir: Vec3, active: &[bool], mass: &[f32], vel: &mut [Vec3],
        tuning: &TissueTuning, crushed: &mut Vec<usize>) -> bool {
        let body = 2.0 * strike.work_j / (strike.speed_mps * strike.speed_mps);
        let side = (body / STEEL_DENSITY).cbrt();
        let area = side * side;
        // The range's live dials when set, else the review tune (constants outside the reviews).
        let (forward, outward_share) = if review_tuning_active() { (tuning.forward, tuning.outward) } else { (tune().forward, tune().outward) };
        let cell_m = model.cell;
        let reach = (0.5 * side + 0.5 * cell_m) * tune().spread * tuning.spread;
        // The track: cells whose centre the fragment's own section passes within a quarter cell of.
        let crush = 0.5 * side + 0.25 * cell_m;
        let step = 0.25 * cell_m;
        let mut speed = strike.speed_mps;
        let mut s = 0.0;
        // Along its path through the part (1.2 m covers any part from its box's face).
        while s < 1.2 && speed > STOPPED_MPS {
            let p = strike.at + dir * s;
            s += step;
            let g=((p-model.origin)/cell_m).floor().as_ivec3();
            let r=(crush/cell_m).ceil()as i32;
            let mut here=None;let mut nearest=f32::INFINITY;
            for z in -r..=r {for y in -r..=r {for x in -r..=r {
                let Some(c)=model.lookup([g.x+x,g.y+y,g.z+z])else{continue;};
                let cell=&model.cells[c];if cell.part!=strike.part||!self.present[c]{continue;}
                let offset=cell.at-p;let along=offset.dot(dir);let radial=(offset-dir*along).length_squared();
                let gap=(offset.abs()-Vec3::splat(cell_m*0.5)).max(Vec3::ZERO);
                if along.abs()<=cell_m*0.5&&gap.length_squared()<=0.25*side*side&&radial<nearest{nearest=radial;here=Some(c);}
            }}}
            let Some(here)=here else{continue;};let cell=&model.cells[here];
            // Poncelet's form: the tissue's own resistance (its tearing energy per volume, authored)
            // and Newtonian drag.
            let tearing = tuning.tear_j_m3[cell.tissue.index()] * area * step;
            let kinetic = 0.5 * body * speed * speed - tearing;
            let resisted = if kinetic > 0.0 { (2.0 * kinetic / body).sqrt() } else { 0.0 };
            let density = mass[here] / cell_m.powi(3);
            let next = resisted * (-density * DRAG_CD * area * step / (2.0 * body)).exp();
            let lost = body * (speed - next);
            speed = next;
            // The cells within the body's reach of this point share the momentum it lost.
            let g = ((p - model.origin) / cell_m).floor();
            let r = (reach.max(crush) / cell_m).ceil() as i32;
            let mut near: Vec<usize> = Vec::new();
            for dz in -r..=r {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if let Some(c) = model.lookup([g.x as i32 + dx, g.y as i32 + dy, g.z as i32 + dz]) {
                            let q = model.cells[c].at;
                            let off = (q - p) - dir * (q - p).dot(dir);
                            let slab = (q - p).dot(dir).abs() <= 0.5 * cell_m + 1.0e-4;
                            if active[c] && model.cells[c].part == strike.part && slab {
                                if off.length() <= reach {
                                    near.push(c);
                                }
                                if off.length() <= crush {
                                    crushed.push(c);
                                }
                            }
                        }
                    }
                }
            }
            if near.is_empty() {
                near.push(here);
            }
            let share = lost / near.len() as f32;
            for c in near {
                let q = model.cells[c].at;
                let outward = ((q - p) - dir * (q - p).dot(dir)).normalize_or_zero();
                vel[c] += (dir * forward + outward * outward_share) * (share / mass[c]);
            }
        }
        speed <= STOPPED_MPS
    }

    /// A burst's pressure work on a part: kinetic energy of the part's surface cells facing the
    /// burst, shared by how squarely they face it, each pushed away from it (authored mapping:
    /// all of the part's blast work).
    fn push_pressure(&self, model: &TissueModel, strike: &Strike, active: &[bool], mass: &[f32], vel: &mut [Vec3]) {
        let mut facing: Vec<(usize, f32, Vec3)> = Vec::new();
        for &c in &model.part_cells[strike.part.index()] {
            let c = c as usize;
            if !active[c] {
                continue;
            }
            let [x, y, z] = model.cells[c].ijk.map(i32::from);
            let mut inward = Vec3::ZERO;
            let mut open = false;
            for (o, v) in [([1, 0, 0], Vec3::X), ([-1, 0, 0], Vec3::NEG_X), ([0, 1, 0], Vec3::Y), ([0, -1, 0], Vec3::NEG_Y),
                ([0, 0, 1], Vec3::Z), ([0, 0, -1], Vec3::NEG_Z)] {
                match model.lookup([x + o[0], y + o[1], z + o[2]]) {
                    Some(nb) if self.present[nb] => inward += v,
                    _ => open = true,
                }
            }
            if !open {
                continue;
            }
            let away = (model.cells[c].at - strike.at).normalize_or_zero();
            let outward = (-inward).normalize_or_zero();
            let weight = (-away).dot(outward).max(0.0);
            if weight > 0.0 {
                facing.push((c, weight, away));
            }
        }
        let total: f32 = facing.iter().map(|f| f.1).sum();
        if total <= 0.0 {
            return;
        }
        for (c, weight, away) in facing {
            let energy = strike.work_j * weight / total;
            vel[c] += away * (2.0 * energy / mass[c]).sqrt();
        }
    }
}

/// The drawing label of a cell (`skin_label` gives an outer cell its colour, 1..=SKIN_LABELS).
/// In an anatomical volume the outer shell takes the figure's colours, muscle its two shades,
/// cortical bone the skeleton's, and every other tissue its table colour (`BONE_LABEL + id`).
pub(super) fn label(cell: &Cell, skin_label: u8) -> u8 {
    let muscle = || {
        let [a, b, c] = cell.ijk.map(u32::from);
        let hash = a.wrapping_mul(73_856_093) ^ b.wrapping_mul(19_349_663) ^ c.wrapping_mul(83_492_791);
        MUSCLE_LABELS[(hash >> 7) as usize & 1]
    };
    if cell.label != 0 {
        return match cell.label {
            1 => skin_label.clamp(1, SKIN_LABELS),
            ANATOMY_MUSCLE => muscle(),
            ANATOMY_CORTICAL_BONE => BONE_LABEL,
            other => BONE_LABEL + other.min(MAX_ANATOMY_ID),
        };
    }
    match cell.tissue {
        Tissue::Skin => skin_label.clamp(1, SKIN_LABELS),
        Tissue::Muscle | Tissue::Fluid => muscle(),
        Tissue::Bone => BONE_LABEL,
    }
}

/// The tissue table's skeletal muscle and cortical bone (`catalog/crew_anatomy_tissues.v1.json`).
pub(super) const ANATOMY_MUSCLE: u8 = 3;
pub(super) const ANATOMY_CORTICAL_BONE: u8 = 4;

/// The strikes one killing event delivers to a casualty, in the relaxed figure's frame: every
/// exact fragment contact that crossed him (its own energy, speed, point and direction) and the
/// filler's pressure work on each part from the burst's own point. `source_to_ragdoll` maps the
/// event's frame into the body's; the body is still in the pose the damage was resolved against
/// (its first ragdoll frame).
pub(super) fn casualty_strikes(ragdoll: &CrewRagdoll, source_to_ragdoll: Mat4, contacts: &[FragmentDamageContact],
    pressure: &[(crew_physics::CrewBodyPart, f32)], burst_source: Vec3) -> Vec<Strike> {
    let figure = |part: crew_physics::CrewBodyPart, point: Vec3| {
        let segment = ragdoll.segments[part.index()];
        part.rest_center() + segment.rot.conjugate() * (point - segment.pos) / ragdoll.scale
    };
    let mut strikes = Vec::new();
    for contact in contacts.iter().filter(|c| c.target == FragmentDamageTarget::CouchCrew(ragdoll.casualty)) {
        let Some(part) = contact.body_part else { continue };
        let point = source_to_ragdoll.transform_point3(contact.local_point);
        let dir = ragdoll.segments[part.index()].rot.conjugate() * source_to_ragdoll.transform_vector3(contact.local_velocity);
        strikes.push(Strike { part, at: figure(part, point), dir: dir.normalize_or_zero(), work_j: contact.energy_j,
            speed_mps: contact.speed_mps });
    }
    let burst = source_to_ragdoll.transform_point3(burst_source);
    for &(part, work_j) in pressure {
        if work_j > 0.0 {
            strikes.push(Strike { part, at: figure(part, burst), dir: Vec3::ZERO, work_j, speed_mps: 0.0 });
        }
    }
    strikes
}

/// A piece of a body in flight or at rest: its cells (figure frame, about `centre`), its pose in
/// its frame (`frame`'s local coordinates) and its motion there. It meets the vehicle's surfaces as
/// a sphere of its cells' bounding radius (named).
#[derive(Clone, Debug)]
pub(super) struct ChunkBody {
    pub actor: u32,
    pub frame: BloodFrame,
    pub cells: Vec<u32>,
    pub centre: Vec3,
    pub scale: f32,
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub spin: Vec3,
    pub radius: f32,
    /// What holds its cells together (J): a hit harder than this breaks it into single grains.
    pub cohesion_j: f32,
    pub bounces: u8,
    pub settled: bool,
}

impl ChunkBody {
    /// Its cells' frame from the figure's: frame-local = pos + rot·scale·(p − centre).
    pub(super) fn local_model(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(Vec3::splat(self.scale), self.rot, self.pos) * Mat4::from_translation(-self.centre)
    }
}

/// The pieces of a solve, each placed where its part is now (the ragdoll's first frame) with its
/// own velocity, the part's, and its spin.
pub(super) fn chunk_bodies(model: &TissueModel, ragdoll: &CrewRagdoll, chunks: Vec<Chunk>) -> Vec<ChunkBody> {
    chunks.into_iter().map(|chunk| {
        let segment = ragdoll.segments[chunk.part.index()];
        let figure_to_local = crew_segment_local_pose(segment) * Mat4::from_scale(Vec3::splat(ragdoll.scale))
            * Mat4::from_translation(-chunk.part.rest_center());
        let radius = chunk.cells.iter().map(|&c| model.cells[c as usize].at.distance(chunk.centre)).fold(0.0f32, f32::max)
            * ragdoll.scale + 0.5 * model.cell * 3f32.sqrt() * ragdoll.scale;
        ChunkBody {
            actor: ragdoll.casualty.peer_id,
            frame: ragdoll.frame,
            centre: chunk.centre,
            scale: ragdoll.scale,
            pos: figure_to_local.transform_point3(chunk.centre),
            rot: segment.rot,
            vel: segment.vel + segment.rot * chunk.velocity,
            spin: segment.angular_vel + segment.rot * chunk.spin,
            radius,
            cohesion_j: chunk.cohesion_j,
            cells: chunk.cells,
            bounces: 0,
            settled: false,
        }
    }).collect()
}

/// Solve a casualty's strikes in the solo corpse renderer, which keeps his cells and the pieces
/// that come away. Returns the parts whose remaining soft cells keep them in the articulated
/// simulation: in solo play (where the voxel body is drawn) a pulverizing outcome only removes a
/// part that kept less than `KEEP_SOFT_SHARE` of its soft cells.
pub(super) fn carve_casualty_tissue(corpses: &std::cell::RefCell<Option<crew_corpse_render::Corpses>>, solo: bool,
    ragdoll: &mut CrewRagdoll, strikes: &[Strike]) -> [bool; PARTS] {
    let shares = corpses.borrow_mut().as_mut().and_then(|corpses| corpses.strike_tissue(ragdoll, strikes));
    sync_voxel_joints(corpses,ragdoll);
    std::array::from_fn(|i| solo && shares.is_some_and(|shares| shares[i] >= KEEP_SOFT_SHARE))
}

fn sync_voxel_joints(corpses:&std::cell::RefCell<Option<crew_corpse_render::Corpses>>,ragdoll:&mut CrewRagdoll){
    if let Some(joined)=corpses.borrow().as_ref().and_then(|c|c.tissue_joint_connections(ragdoll.casualty.peer_id)){
        for (segment,joined) in ragdoll.segments.iter_mut().zip(joined){segment.detached=!joined;}
    }
}

/// A casualty's solve deferred behind the death's fade and black (`Game::settle_casualty_tissue`):
/// its worker, and what the carve's consequences need once it is back.
pub(super) struct DeferredCarve {
    pub(super) casualty: CouchCrewCasualty,
    pub(super) job: crew_corpse_tissue::StrikeJob,
    pub(super) solo: bool,
    pub(super) fragment_impulses: [Vec3; PARTS],
    /// Parts the strike would hide (its pulverizing outcome), before the cells decide.
    pub(super) hide: [bool; PARTS],
}

/// Start a casualty's solve on a worker (`None` without the voxel corpse renderer).
pub(super) fn begin_casualty_carve(corpses: &std::cell::RefCell<Option<crew_corpse_render::Corpses>>,
    ragdoll: &CrewRagdoll, strikes: &[Strike]) -> Option<crew_corpse_tissue::StrikeJob> {
    corpses.borrow_mut().as_mut().and_then(|corpses| corpses.begin_tissue_strike(ragdoll, strikes))
}

/// Take a casualty's solve back: the parts whose soft cells keep them in the articulated
/// simulation (as `carve_casualty_tissue`).
pub(super) fn finish_casualty_carve(corpses: &std::cell::RefCell<Option<crew_corpse_render::Corpses>>, solo: bool,
    job: crew_corpse_tissue::StrikeJob, ragdoll: &mut CrewRagdoll) -> [bool; PARTS] {
    let shares = corpses.borrow_mut().as_mut().and_then(|corpses| corpses.finish_tissue_strike(job, ragdoll));
    sync_voxel_joints(corpses,ragdoll);
    std::array::from_fn(|i| solo && shares.is_some_and(|shares| shares[i] >= KEEP_SOFT_SHARE))
}

/// Restitution and tangential keep of a piece meeting a surface or another grain (authored: wet
/// tissue), the speed below which a piece resting on a face turned up (or on grains) stops, and its
/// bounce budget.
pub(super) const PIECE_RESTITUTION: f32 = 0.25;
pub(super) const PIECE_TANGENT_KEEP: f32 = 0.55;
pub(super) const PIECE_REST_MPS: f32 = 0.35;
pub(super) const PIECE_BOUNCES: u8 = 10;

/// Advance the pieces of solved bodies by `dt` (`crew_corpse_tissue::TissueDraw::integrate`).
pub(super) fn integrate_chunks(corpses: &std::cell::RefCell<Option<crew_corpse_render::Corpses>>, world: &FragmentWorld,
    player: &Tank, dt: f32) {
    if let Some(corpses) = corpses.borrow_mut().as_mut() {
        corpses.integrate_tissue(world, player, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The review override (`fea-range`'s tissue knobs) is never a shipping default: without it the
    /// lattice runs on exactly the authored constants, a bond's law is unscaled, and no source but
    /// the review range sets it.
    #[test]
    fn the_tissue_tuning_is_authored_unless_the_review_range_sets_it() {
        let t = TissueTuning::AUTHORED;
        assert_eq!((t.tear_j_m3, t.critical_stretch, t.forward, t.outward, t.active_reach_m, t.landing_breakup, t.spread),
            (TEAR_J_M3, CRITICAL_STRETCH, FORWARD, OUTWARD, ACTIVE_REACH_M, 1.0, 1.0));
        assert_eq!(*REVIEW_TUNING.read().unwrap(), None, "no override at start");
        assert!(bond_law_scale(&t).iter().all(|&(k, c)| k == 1.0 && c == 1.0));
        // Doubling skin's tearing energy at its stretch doubles its bonds' stiffness.
        let mut tuned = t;
        tuned.tear_j_m3[0] *= 2.0;
        tuned.critical_stretch[1] *= 2.0;
        let law = bond_law_scale(&tuned);
        assert!((law[0].0 - 2.0).abs() < 1e-6 && law[0].1 == 1.0);
        assert!((law[1].0 - 0.25).abs() < 1e-6 && (law[1].1 - 2.0).abs() < 1e-6);
        let needle = ["set_review_tuning", "(Some"].concat(); // not this file's own text
        let mut setters = Vec::new();
        for entry in std::fs::read_dir("src").unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") && std::fs::read_to_string(&path).unwrap().contains(&needle) {
                setters.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        assert_eq!(setters, vec!["game_fea_range.rs".to_string()], "only the review range overrides the tissue");
    }

    #[test]
    fn the_classes_hold_in_order_and_keep_their_2_cm_energies() {
        // Fluid gives first, then skin, muscle, bone; at the procedural 2 cm cell the old per-cell
        // tearing energies (0.4, 1.0, 3.2 J) are unchanged.
        let [skin, muscle, bone, fluid] = TEAR_J_M3;
        assert!(fluid < skin && skin < muscle && muscle < bone);
        let per_cell = |density: f32| density * CELL_M.powi(3);
        for (got, want) in [(per_cell(skin), 0.4), (per_cell(muscle), 1.0), (per_cell(bone), 3.2)] {
            assert!((got - want).abs() < 1e-6, "{got} {want}");
        }
        assert!(Tissue::Fluid.soft() && !Tissue::Bone.soft());
    }

    /// A box "figure" (12 x 20 x 12 cm) with a bar of "bone" (3 x 16 x 3 cm) through it.
    fn toy() -> TissueModel {
        let figure = crate::prims::box_mesh(Vec3::new(-0.06, 1.0, -0.06), Vec3::new(0.06, 1.2, 0.06), [1, 1, 1]);
        let bone = crate::prims::box_mesh(Vec3::new(-0.015, 1.02, -0.015), Vec3::new(0.015, 1.18, 0.015), [1, 1, 1]);
        let bones = std::array::from_fn(|i| (i == crew_physics::CrewBodyPart::Torso.index()).then(|| bone.clone()));
        // The procedural build, whatever anatomical volume the asset root holds.
        build_procedural(&figure, &bones)
    }

    fn count(model: &TissueModel, t: Tissue) -> usize {
        model.cells.iter().filter(|c| c.tissue == t).count()
    }

    #[test]
    fn the_figure_and_its_skeleton_share_one_grid() {
        let model = toy();
        assert_eq!(model.cells.len(), 6 * 10 * 6);
        assert!(count(&model, Tissue::Skin) > 0 && count(&model, Tissue::Muscle) > 0 && count(&model, Tissue::Bone) > 0);
        for cell in &model.cells {
            assert_eq!(cell.part, crew_physics::CrewBodyPart::Torso);
            if cell.tissue == Tissue::Bone {
                assert!(cell.at.x.abs() < 0.04 && cell.at.z.abs() < 0.04, "bone at {:?}", cell.at);
            }
        }
        // Every cell has its neighbours bonded (interior cells all 18).
        let interior = model.cells.iter().position(|c| c.ijk == [2, 5, 2]).unwrap();
        assert_eq!(model.bonds_of(interior).len(), 18);
        assert!(triangle_box_overlap(Vec3::ZERO, Vec3::splat(0.5), [Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 0.0, -2.0), Vec3::new(0.0, 0.0, 2.0)]));
        assert!(!triangle_box_overlap(Vec3::ZERO, Vec3::splat(0.5), [Vec3::new(-2.0, 0.7, -2.0), Vec3::new(2.0, 0.7, -2.0), Vec3::new(0.0, 0.7, 2.0)]));
    }

    #[test]
    fn the_solve_is_the_same_on_one_thread_or_many() {
        let model = toy();
        let part = crew_physics::CrewBodyPart::Torso;
        let strikes = [Strike { part, at: Vec3::new(0.07, 1.1, 0.03), dir: Vec3::NEG_X, work_j: 0.5 * 0.01 * 600.0 * 600.0, speed_mps: 600.0 },
            Strike { part, at: Vec3::new(0.0, 1.1, 0.6), dir: Vec3::ZERO, work_j: 2_000.0, speed_mps: 0.0 }];
        let mut one = CasualtyTissue::new(&model);
        let a = one.solve_with_threads(&model, &strikes, 1);
        let mut many = CasualtyTissue::new(&model);
        let b = many.solve_with_threads(&model, &strikes, 7);
        assert_eq!((a.bonds_broken, a.chunks.len(), a.crushed), (b.bonds_broken, b.chunks.len(), b.crushed));
        assert_eq!(one.present, many.present);
        assert_eq!(one.broken, many.broken);
        for (x, y) in a.chunks.iter().zip(&b.chunks) {
            assert_eq!(x.velocity.to_array().map(f32::to_bits), y.velocity.to_array().map(f32::to_bits));
        }
    }

    #[test]
    fn a_fast_fragment_tears_a_channel_and_a_slow_one_does_not() {
        let model = toy();
        let part = crew_physics::CrewBodyPart::Torso;
        let entry = Vec3::new(0.07, 1.1, 0.03);
        // 10 g at 600 m/s across the box.
        let mut tissue = CasualtyTissue::new(&model);
        let fast = tissue.solve(&model, &[Strike { part, at: entry, dir: Vec3::NEG_X, work_j: 0.5 * 0.01 * 600.0 * 600.0, speed_mps: 600.0 }]);
        // The channel: cells crushed in the track and pieces torn from its walls.
        let gone = (0..model.cells.len()).filter(|&c| !tissue.present[c]).count();
        assert!(fast.crushed > 0 && gone >= fast.crushed, "{fast:?}");
        for c in (0..model.cells.len()).filter(|&c| !tissue.present[c]) {
            let at = model.cells[c].at;
            assert!((at.y - 1.1).abs() < 0.05 && (at.z - 0.03).abs() < 0.05, "gone far from the path: {at:?}");
        }
        for chunk in &fast.chunks {
            for &c in &chunk.cells {
                let at = model.cells[c as usize].at;
                assert!((at.y - 1.1).abs() < 0.05 && (at.z - 0.03).abs() < 0.05, "torn far from the path: {at:?}");
            }
            assert!(chunk.velocity.x < 0.0, "pieces fly on along the path: {:?}", chunk.velocity);
        }
        assert!(tissue.struck(&model, part) && tissue.soft_share(&model, part) > 0.7);
        // 10 g at 15 m/s: nothing tears.
        let mut tissue = CasualtyTissue::new(&model);
        let slow = tissue.solve(&model, &[Strike { part, at: entry, dir: Vec3::NEG_X, work_j: 0.5 * 0.01 * 15.0 * 15.0, speed_mps: 15.0 }]);
        assert!(slow.chunks.is_empty(), "{slow:?}");
        assert_eq!(slow.lodged, 1, "it stops in him");
    }

    #[test]
    fn a_strike_that_destroys_the_flesh_takes_the_cloth_over_it() {
        let model = toy();
        assert!(model.cells.iter().any(|c| c.cloth()), "the toy's outer shell is its uniform");
        let part = crew_physics::CrewBodyPart::Torso;
        let mut tissue = CasualtyTissue::new(&model);
        // A heavy burst on one side and a fast fragment through: flesh torn and crushed.
        let solve = tissue.solve(&model, &[Strike { part, at: Vec3::new(0.0, 1.1, 0.6), dir: Vec3::ZERO, work_j: 4_000.0, speed_mps: 0.0 },
            Strike { part, at: Vec3::new(0.07, 1.1, 0.03), dir: Vec3::NEG_X, work_j: 0.5 * 0.05 * 700.0 * 700.0, speed_mps: 700.0 }]);
        assert!(solve.crushed + solve.chunks.len() > 0);
        // No cloth stands on its own: every cloth cell still there holds to flesh still there, or
        // to cloth that does.
        let on_flesh = |c: usize| model.bonds_of(c).iter().any(|&b| {
            let bond = model.bonds[b as usize];
            let other = if bond.a as usize == c { bond.b } else { bond.a } as usize;
            !tissue.broken[b as usize] && tissue.present[other] && !model.cells[other].cloth()
        });
        for c in (0..model.cells.len()).filter(|&c| tissue.present[c] && model.cells[c].cloth()) {
            let held = on_flesh(c) || model.bonds_of(c).iter().any(|&b| {
                let bond = model.bonds[b as usize];
                let other = if bond.a as usize == c { bond.b } else { bond.a } as usize;
                !tissue.broken[b as usize] && tissue.present[other] && on_flesh(other)
            });
            assert!(held, "cloth cell {:?} left as a shell", model.cells[c].ijk);
        }
        // Where the flesh under the fragment's track is gone, the cloth over it is gone too.
        let track = |c: usize| { let at = model.cells[c].at; (at.y - 1.1).abs() < 0.012 && (at.z - 0.03).abs() < 0.012 };
        assert!((0..model.cells.len()).filter(|&c| track(c) && model.cells[c].cloth()).all(|c| !tissue.present[c]));
        // A cloth bond lets go at the textile's stretch at most.
        assert!(model.bonds.iter().filter(|b| model.cells[b.a as usize].cloth() || model.cells[b.b as usize].cloth())
            .all(|b| b.critical <= CLOTH_BREAKING_STRAIN + 1e-6));
    }

    #[test]
    fn an_intact_neck_cannot_detach_from_an_energy_label(){
        let mut model=toy();
        for cell in &mut model.cells {cell.part=if cell.at.y>1.1{crew_physics::CrewBodyPart::Head}else{crew_physics::CrewBodyPart::Torso};}
        let mut tissue=CasualtyTissue::new(&model);let head=crew_physics::CrewBodyPart::Head.index();
        assert!(tissue.joint_connections(&model)[head]);
        for (i,b) in model.bonds.iter().enumerate(){
            if model.cells[b.a as usize].part!=model.cells[b.b as usize].part{tissue.broken[i]=true;}
        }
        assert!(!tissue.joint_connections(&model)[head]);
    }

    #[test]
    fn bone_holds_where_muscle_tears_until_the_force_is_enough() {
        let model = toy();
        let part = crew_physics::CrewBodyPart::Torso;
        let at = Vec3::new(0.07, 1.1, 0.0);
        let bone_torn = |work_j: f32, speed: f32| {
            let mut tissue = CasualtyTissue::new(&model);
            tissue.solve(&model, &[Strike { part, at, dir: Vec3::NEG_X, work_j, speed_mps: speed }]);
            // Crushed in the track or torn away.
            let gone: Vec<usize> = (0..model.cells.len()).filter(|&c| !tissue.present[c]).collect();
            (gone.iter().filter(|&&c| model.cells[c].tissue == Tissue::Bone).count(),
             gone.iter().filter(|&&c| model.cells[c].tissue != Tissue::Bone).count())
        };
        let (bone_small, soft_small) = bone_torn(0.5 * 0.004 * 300.0 * 300.0, 300.0);
        let (bone_big, soft_big) = bone_torn(0.5 * 3.0 * 380.0 * 380.0, 380.0);
        assert!(soft_small > 0 && bone_small < soft_small, "a small fragment tears soft cells: {soft_small} soft, {bone_small} bone");
        assert!(bone_big > 0 && soft_big > soft_small, "a 3 kg body at 380 m/s takes bone too: {bone_big} bone, {soft_big} soft");
    }

    #[test]
    fn a_burst_tears_the_side_facing_it_and_a_weak_one_does_not() {
        let model = toy();
        let part = crew_physics::CrewBodyPart::Torso;
        let burst = Vec3::new(0.0, 1.1, 0.6);
        let mut tissue = CasualtyTissue::new(&model);
        let strong = tissue.solve(&model, &[Strike { part, at: burst, dir: Vec3::ZERO, work_j: 2_000.0, speed_mps: 0.0 }]);
        let torn: Vec<u32> = strong.chunks.iter().flat_map(|c| c.cells.iter().copied()).collect();
        assert!(!torn.is_empty());
        let mean_z = torn.iter().map(|&c| model.cells[c as usize].at.z).sum::<f32>() / torn.len() as f32;
        assert!(mean_z > 0.0, "the near side goes: mean z {mean_z}");
        assert!(strong.chunks.iter().all(|c| c.velocity.z < 0.5), "pushed away from the burst");
        let mut tissue = CasualtyTissue::new(&model);
        let weak = tissue.solve(&model, &[Strike { part, at: burst, dir: Vec3::ZERO, work_j: 0.5, speed_mps: 0.0 }]);
        assert!(weak.chunks.is_empty(), "{weak:?}");
    }

    #[test]
    fn only_cells_open_to_air_or_a_wound_are_drawn() {
        let model = toy();
        let mut tissue = CasualtyTissue::new(&model);
        let shell = (0..model.cells.len()).filter(|&c| model.exposed(&tissue.present, c)).count();
        // A 6 x 10 x 6 block: everything but its 4 x 8 x 4 core.
        assert_eq!(shell, 6 * 10 * 6 - 4 * 8 * 4);
        let part = crew_physics::CrewBodyPart::Torso;
        let solve = tissue.solve(&model, &[Strike { part, at: Vec3::new(0.07, 1.1, 0.03), dir: Vec3::NEG_X,
            work_j: 0.5 * 0.01 * 600.0 * 600.0, speed_mps: 600.0 }]);
        let gone = (0..model.cells.len()).filter(|&c| !tissue.present[c]).count();
        assert!(gone > 0);
        let after = (0..model.cells.len()).filter(|&c| tissue.present[c] && model.exposed(&tissue.present, c)).count();
        assert!(after > shell - gone, "the wound's walls are drawn");
        assert!(solve.chunks.iter().all(|c| c.cohesion_j >= 0.0));
        assert!(model.blend.iter().all(|b| b.is_none()), "one part: no joint");
    }

    #[test]
    fn the_tune_is_the_constants_unless_the_review_allows_it() {
        let constants = Tune { tear: 1.0, stretch: 1.0, forward: FORWARD, outward: OUTWARD, grain: 1.0, spread: 1.0 };
        assert_eq!(parse_tune(None), constants);
        assert_eq!(parse_tune(Some("tear=6,grain=30")).tear, 6.0);
        // No test or play path opens the gate; only `fea-film` does.
        assert!(!REVIEW_TUNE.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(tune(), constants);
    }

    /// Each part of the real figure is built in one bonded piece (`rehome_part_islands`).
    #[test]
    #[ignore]
    fn the_real_figure_parts_are_each_one_piece() {
        let figure = load_full_crew_mesh(CREW_RELAXED_GLBS[0], "tissue probe").expect("relaxed figure");
        let (bones, _) = crew_skeleton::fitted_bone_pages(&figure).expect("PSX skeleton");
        let model = build_tissue(&figure, &bones);
        let n = model.cells.len();
        let mut parent: Vec<u32> = (0..n as u32).collect();
        for bond in &model.bonds {
            if model.cells[bond.a as usize].part == model.cells[bond.b as usize].part {
                let (ra, rc) = (find(&mut parent, bond.a), find(&mut parent, bond.b));
                if ra != rc {
                    parent[ra.max(rc) as usize] = ra.min(rc);
                }
            }
        }
        for part in crew_physics::CrewBodyPart::ALL {
            let mut pieces: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
            for &c in &model.part_cells[part.index()] {
                pieces.entry(find(&mut parent, c)).or_default().push(c);
            }
            let mut sizes: Vec<(usize, [u16; 3], u8)> = pieces.values().map(|cells| {
                let c = &model.cells[cells[0] as usize];
                (cells.len(), c.ijk, c.label)
            }).collect();
            sizes.sort_by(|a, b| b.0.cmp(&a.0));
            println!("{part:?}: {} pieces {:?}", sizes.len(), &sizes[..sizes.len().min(6)]);
            assert_eq!(sizes.len(), 1, "{part:?} is built in one piece");
        }
    }

    /// Measures the real crew figure: cells by tissue, bonds, boxes for a whole body, build time.
    #[test]
    #[ignore]
    fn print_real_figure_tissue() {
        let figure = load_full_crew_mesh(CREW_RELAXED_GLBS[0], "tissue probe").expect("relaxed figure");
        let (bones, _) = crew_skeleton::fitted_bone_pages(&figure).expect("PSX skeleton");
        let start = std::time::Instant::now();
        let model = build_tissue(&figure, &bones);
        println!("built {} cells, {} bonds in {:.0} ms, grid {:?}", model.cell_count(), model.bonds.len(),
            start.elapsed().as_secs_f32() * 1000.0, model.dims);
        let present = vec![true; model.cells.len()];
        let mut drawn = 0;
        for part in crew_physics::CrewBodyPart::ALL {
            let part_drawn = model.part_cells[part.index()].iter().filter(|&&c| model.exposed(&present, c as usize)).count();
            drawn += part_drawn;
            let t = |kind: Tissue| model.part_cells[part.index()].iter().filter(|&&c| model.cells[c as usize].tissue == kind).count();
            println!("{part:?}: skin {} muscle {} bone {}, {part_drawn} cubes drawn", t(Tissue::Skin), t(Tissue::Muscle), t(Tissue::Bone));
        }
        println!("whole body: {drawn} cubes drawn; {} cells blended at joints", model.blend.iter().filter(|b| b.is_some()).count());
        // A burst on every part (a cook-off's), on one thread and on the solve's workers.
        let blast: Vec<Strike> = crew_physics::CrewBodyPart::ALL.iter().map(|&part| Strike { part, at: part.rest_center() + Vec3::new(0.0, 0.0, 0.8),
            dir: Vec3::ZERO, work_j: 600.0, speed_mps: 0.0 }).collect();
        for threads in [1, SOLVE_THREADS] {
            let mut tissue = CasualtyTissue::new(&model);
            let start = std::time::Instant::now();
            let solve = tissue.solve_with_threads(&model, &blast, threads);
            println!("whole-body burst on {threads} threads: {} bonds broken, {} pieces, {} steps in {:.0} ms", solve.bonds_broken,
                solve.chunks.len(), solve.steps, start.elapsed().as_secs_f32() * 1000.0);
        }
        // A 57 mm shot's 3.1 kg at 377 m/s through the torso, and a 20 g fragment at 700 m/s.
        let part = crew_physics::CrewBodyPart::Torso;
        let centre = part.rest_center();
        for (label, mass, speed) in [("57 mm shot", 3.14f32, 377.0f32), ("20 g fragment", 0.02, 700.0), ("2 g fragment", 0.002, 400.0)] {
            let mut tissue = CasualtyTissue::new(&model);
            let start = std::time::Instant::now();
            let solve = tissue.solve(&model, &[Strike { part, at: centre + Vec3::new(0.3, 0.05, 0.0), dir: Vec3::NEG_X,
                work_j: 0.5 * mass * speed * speed, speed_mps: speed }]);
            let torn: usize = solve.chunks.iter().map(|c| c.cells.len()).sum();
            let single = solve.chunks.iter().filter(|c| c.cells.len() == 1).count();
            let largest = solve.chunks.iter().map(|c| c.cells.len()).max().unwrap_or(0);
            let fastest = solve.chunks.iter().map(|c| c.velocity.length()).fold(0.0f32, f32::max);
            println!("{label}: {} pieces ({single} single cells, largest {largest}, fastest {fastest:.0} m/s), {torn} cells torn, {} crushed, {} bonds broken, lodged {}, soft share left {:.2}, {} steps in {:.1} ms",
                solve.chunks.len(), solve.crushed, solve.bonds_broken, solve.lodged, tissue.soft_share(&model, part), solve.steps,
                start.elapsed().as_secs_f32() * 1000.0);
        }
    }
}
