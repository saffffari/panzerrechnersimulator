//! The casualties' bodies as voxels (2026-11-02, the owner's design). The relaxed crew figure and
//! the owner's fitted PSX skeleton (`crew_skeleton`) share one grid of `CELL_M` cells: every cell
//! inside the figure is skin within one cell of its surface and muscle deeper, and every cell a
//! skeleton triangle passes through is bone (the skeleton is a surface, so its quads are what is
//! voxelised), and every cell outside him that his steel helmet's triangles pass through is steel.
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

/// Cell size (m).
pub(super) const CELL_M: f32 = 0.02;
const PARTS: usize = crew_physics::CREW_BODY_PART_COUNT;
/// Below this share of its soft cells a struck part leaves the articulated simulation (what is
/// left of it then rides the nearest retained segment).
pub(super) const KEEP_SOFT_SHARE: f32 = 0.2;
/// Energy to tear one cell free of all its neighbours (J), skin, muscle, bone, helmet steel:
/// authored, in the owner's order, scaled so a part's soft cells tear for about its pulverizing
/// work (82–1,250 J over 233–3,400 cell-units of skin 1 : muscle 2.5); the helmet's far stronger.
const TEAR_J: [f32; 4] = [0.4, 1.0, 3.2, 40.0];
/// The stretch at which a bond lets go, in tension or crushed in compression (authored): skin
/// stretches far, bone hardly.
const CRITICAL_STRETCH: [f32; 4] = [0.6, 0.35, 0.05, 0.2];
/// Densities (kg/m³): soft tissue about water's, bone about cortical bone's. A helmet cell's mass
/// is the helmet's over its cells (a 1 mm shell is not a solid cell).
const DENSITY: [f32; 4] = [1_060.0, 1_060.0, 1_900.0, 7_850.0];
/// The steel helmet's mass (kg; authored, about a German M35's).
const HELMET_MASS_KG: f32 = 1.0;
const STEEL_DENSITY: f32 = 7_850.0;
/// Drag coefficient of a tumbling fragment in tissue (authored, a blunt body's).
const DRAG_CD: f32 = 1.0;
/// Of the momentum a fragment loses, the share pushed along its path and outward from it
/// (authored: the channel opens forward, and a little to the sides).
const FORWARD: f32 = 0.95;
const OUTWARD: f32 = 0.3;
/// Simulated time after the strikes, and the per-step velocity damping (named numerical damping).
const SOLVE_S: f32 = 0.008;
/// Cells this far from a fragment's path move in the solve; farther ones hold them (anchored).
const ACTIVE_REACH_M: f32 = 0.12;
const DAMPING: f32 = 0.001;
/// A fragment slower than this has stopped in the tissue (lodged).
const STOPPED_MPS: f32 = 2.0;

/// Labels for drawing: 1..=SKIN_LABELS are the figure's own colours (the corpse renderer's
/// palette), then two shades of muscle, the bone and the helmet's steel.
pub(super) const SKIN_LABELS: u8 = 12;
pub(super) const MUSCLE_LABELS: [u8; 2] = [SKIN_LABELS + 1, SKIN_LABELS + 2];
pub(super) const BONE_LABEL: u8 = SKIN_LABELS + 3;
pub(super) const STEEL_LABEL: u8 = SKIN_LABELS + 4;
pub(super) const LABELS: usize = STEEL_LABEL as usize;

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
    /// The steel helmet's shell.
    Steel,
}

impl Tissue {
    fn index(self) -> usize {
        self as usize
    }
    fn soft(self) -> bool {
        matches!(self, Tissue::Skin | Tissue::Muscle)
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
}

#[derive(Clone, Copy, Debug)]
struct Bond {
    a: u32,
    b: u32,
    rest_m: f32,
    stiffness: f32,
    critical: f32,
}

pub(super) struct TissueModel {
    pub origin: Vec3,
    pub dims: [usize; 3],
    /// Cell index + 1 per grid cell; 0 outside the figure.
    grid: Vec<u32>,
    pub cells: Vec<Cell>,
    /// Each cell's mass (kg): its tissue's density over the cell, or a share of the helmet's.
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
        let g = ((p - self.origin) / CELL_M).floor();
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

/// Fill the relaxed `figure` with cells and voxelise the fitted skeleton `bones` and the steel
/// `helmet` (in the figure's frame) onto the same grid, then bond every cell to its neighbours.
pub(super) fn build_tissue(figure: &CpuMesh, bones: &[Option<CpuMesh>; PARTS], helmet: Option<&CpuMesh>) -> TissueModel {
    let triangles = figure.indices.len() / 3;
    let corner = |t: usize, i: usize| figure.vertices[figure.indices[3 * t + i] as usize];
    let points = |t: usize| [0, 1, 2].map(|i| Vec3::from(corner(t, i).pos));
    let (lo, hi) = helmet.map_or((figure.min, figure.max), |h| (figure.min.min(h.min), figure.max.max(h.max)));
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
                                part: crew_physics::classify_relaxed_mesh_point(at), uv });
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
    // The helmet's triangles, voxelised outside him: its shell rests on his head.
    if let Some(helmet) = helmet {
        for t in helmet.indices.chunks_exact(3) {
            let tri = [0, 1, 2].map(|i| Vec3::from(helmet.vertices[t[i] as usize].pos));
            let uv = [0, 1, 2].iter().map(|&i| Vec2::from(helmet.vertices[t[i] as usize].uv)).sum::<Vec2>() / 3.0;
            let lo = ((tri[0].min(tri[1]).min(tri[2]) - origin) / CELL_M).floor().as_ivec3().max(glam::IVec3::ZERO);
            let hi = ((tri[0].max(tri[1]).max(tri[2]) - origin) / CELL_M).floor().as_ivec3()
                .min(glam::IVec3::new(dims[0] as i32 - 1, dims[1] as i32 - 1, dims[2] as i32 - 1));
            for j in lo.y..=hi.y {
                for k in lo.z..=hi.z {
                    for i in lo.x..=hi.x {
                        let slot = (j as usize * dims[2] + k as usize) * dims[0] + i as usize;
                        let at = origin + (Vec3::new(i as f32, j as f32, k as f32) + 0.5) * CELL_M;
                        if grid[slot] == 0 && triangle_box_overlap(at, half, tri) {
                            cells.push(Cell { at, ijk: [i as u16, j as u16, k as u16], tissue: Tissue::Steel,
                                part: crew_physics::CrewBodyPart::Head, uv });
                            grid[slot] = cells.len() as u32;
                        }
                    }
                }
            }
        }
    }
    let steel = cells.iter().filter(|c| c.tissue == Tissue::Steel).count().max(1);
    let mass_kg: Vec<f32> = cells.iter().map(|c| match c.tissue {
        Tissue::Steel => HELMET_MASS_KG / steel as f32,
        tissue => DENSITY[tissue.index()] * CELL_M.powi(3),
    }).collect();
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
    let mut model = TissueModel { origin, dims, grid, cells, mass_kg, part_cells, bonds: Vec::new(), bond_start: Vec::new(),
        bond_list: Vec::new(), soft_cells, blend: Vec::new() };
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
            let weaker = cell.tissue.index().min(model.cells[b].tissue.index());
            let rest_m = CELL_M * ((o[0] * o[0] + o[1] * o[1] + o[2] * o[2]) as f32).sqrt();
            let energy = TEAR_J[weaker] / 18.0;
            let critical = CRITICAL_STRETCH[weaker];
            let stiffness = 2.0 * energy / (critical * rest_m).powi(2);
            bonds.push(Bond { a: a as u32, b: b as u32, rest_m, stiffness, critical });
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
                    || (along > -ACTIVE_REACH_M && (d - dir * along).length() < ACTIVE_REACH_M) {
                    active[c] = true;
                }
            }
        }
        let moving: Vec<usize> = (0..n).filter(|&c| active[c]).collect();
        let mass = &model.mass_kg;
        let mut vel = vec![Vec3::ZERO; n];
        let mut out = Solve::default();
        for strike in strikes.iter().filter(|s| s.work_j.is_finite() && s.work_j > 0.0) {
            let dir = strike.dir.normalize_or_zero();
            if dir == Vec3::ZERO || strike.speed_mps <= STOPPED_MPS {
                self.push_pressure(model, strike, &active, mass, &mut vel);
            } else if self.push_fragment(model, strike, dir, &active, mass, &mut vel) {
                out.lodged += 1;
            }
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
            stiffness[bond.a as usize] += bond.stiffness;
            stiffness[bond.b as usize] += bond.stiffness;
        }
        let period = (0..n).filter(|&c| active[c] && stiffness[c] > 0.0)
            .map(|c| (mass[c] / stiffness[c]).sqrt()).fold(f32::INFINITY, f32::min);
        if !period.is_finite() {
            return out;
        }
        let dt = 0.6 * period;
        let steps = (SOLVE_S / dt).ceil() as usize;
        let mut disp = vec![Vec3::ZERO; n];
        let mut force = vec![Vec3::ZERO; n];
        for _ in 0..steps {
            for &c in &moving {
                force[c] = Vec3::ZERO;
            }
            for &b in &live {
                if self.broken[b as usize] {
                    continue;
                }
                let bond = model.bonds[b as usize];
                let (a, c) = (bond.a as usize, bond.b as usize);
                let d = (model.cells[c].at + disp[c]) - (model.cells[a].at + disp[a]);
                let length = d.length();
                let stretch = (length - bond.rest_m) / bond.rest_m;
                if stretch.abs() > bond.critical {
                    self.broken[b as usize] = true;
                    out.bonds_broken += 1;
                    continue;
                }
                let f = d / length.max(1.0e-9) * (bond.stiffness * (length - bond.rest_m));
                force[a] += f;
                force[c] -= f;
            }
            for &c in &moving {
                vel[c] = (vel[c] + force[c] / mass[c] * dt) * (1.0 - DAMPING);
                disp[c] += vel[c] * dt;
            }
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
                    inertia += mass[c as usize] * (r.length_squared() + CELL_M * CELL_M / 6.0);
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
                        let bond = model.bonds[b as usize];
                        0.5 * bond.stiffness * (bond.critical * bond.rest_m).powi(2)
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
    fn push_fragment(&self, model: &TissueModel, strike: &Strike, dir: Vec3, active: &[bool], mass: &[f32], vel: &mut [Vec3]) -> bool {
        let body = 2.0 * strike.work_j / (strike.speed_mps * strike.speed_mps);
        let side = (body / STEEL_DENSITY).cbrt();
        let area = side * side;
        let reach = 0.5 * side + 0.5 * CELL_M;
        let step = 0.25 * CELL_M;
        let mut speed = strike.speed_mps;
        let mut s = 0.0;
        // Along its path through the part (1.2 m covers any part from its box's face).
        while s < 1.2 && speed > STOPPED_MPS {
            let p = strike.at + dir * s;
            s += step;
            let Some(here) = model.cell_at(p) else { continue };
            let cell = &model.cells[here];
            if cell.part != strike.part || !self.present[here] {
                continue;
            }
            // Poncelet's form: the tissue's own resistance (its tearing energy per volume, authored)
            // and Newtonian drag.
            let tearing = TEAR_J[cell.tissue.index()] / CELL_M.powi(3) * area * step;
            let kinetic = 0.5 * body * speed * speed - tearing;
            let resisted = if kinetic > 0.0 { (2.0 * kinetic / body).sqrt() } else { 0.0 };
            let density = mass[here] / CELL_M.powi(3);
            let next = resisted * (-density * DRAG_CD * area * step / (2.0 * body)).exp();
            let lost = body * (speed - next);
            speed = next;
            // The cells within the body's reach of this point share the momentum it lost.
            let g = ((p - model.origin) / CELL_M).floor();
            let r = (reach / CELL_M).ceil() as i32;
            let mut near: Vec<usize> = Vec::new();
            for dz in -r..=r {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if let Some(c) = model.lookup([g.x as i32 + dx, g.y as i32 + dy, g.z as i32 + dz]) {
                            let q = model.cells[c].at;
                            let off = (q - p) - dir * (q - p).dot(dir);
                            if active[c] && model.cells[c].part == strike.part && off.length() <= reach
                                && (q - p).dot(dir).abs() <= 0.5 * CELL_M + 1.0e-4 {
                                near.push(c);
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
                vel[c] += (dir * FORWARD + outward * OUTWARD) * (share / mass[c]);
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

/// The drawing label of a cell (`skin_label` gives a skin cell its colour, 1..=SKIN_LABELS).
pub(super) fn label(cell: &Cell, skin_label: u8) -> u8 {
    match cell.tissue {
        Tissue::Skin => skin_label.clamp(1, SKIN_LABELS),
        Tissue::Muscle => {
            let [a, b, c] = cell.ijk.map(u32::from);
            let hash = a.wrapping_mul(73_856_093) ^ b.wrapping_mul(19_349_663) ^ c.wrapping_mul(83_492_791);
            MUSCLE_LABELS[(hash >> 7) as usize & 1]
        }
        Tissue::Bone => BONE_LABEL,
        Tissue::Steel => STEEL_LABEL,
    }
}

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
            * ragdoll.scale + 0.5 * CELL_M * 3f32.sqrt() * ragdoll.scale;
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
    ragdoll: &CrewRagdoll, strikes: &[Strike]) -> [bool; PARTS] {
    let shares = corpses.borrow_mut().as_mut().and_then(|corpses| corpses.strike_tissue(ragdoll, strikes));
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

    /// A box "figure" (12 x 20 x 12 cm) with a bar of "bone" (3 x 16 x 3 cm) through it.
    fn toy() -> TissueModel {
        let figure = crate::prims::box_mesh(Vec3::new(-0.06, 1.0, -0.06), Vec3::new(0.06, 1.2, 0.06), [1, 1, 1]);
        let bone = crate::prims::box_mesh(Vec3::new(-0.015, 1.02, -0.015), Vec3::new(0.015, 1.18, 0.015), [1, 1, 1]);
        let bones = std::array::from_fn(|i| (i == crew_physics::CrewBodyPart::Torso.index()).then(|| bone.clone()));
        build_tissue(&figure, &bones, None)
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
    fn a_fast_fragment_tears_a_channel_and_a_slow_one_does_not() {
        let model = toy();
        let part = crew_physics::CrewBodyPart::Torso;
        let entry = Vec3::new(0.07, 1.1, 0.03);
        // 10 g at 600 m/s across the box.
        let mut tissue = CasualtyTissue::new(&model);
        let fast = tissue.solve(&model, &[Strike { part, at: entry, dir: Vec3::NEG_X, work_j: 0.5 * 0.01 * 600.0 * 600.0, speed_mps: 600.0 }]);
        let torn: usize = fast.chunks.iter().map(|c| c.cells.len()).sum();
        assert!(torn > 0 && fast.bonds_broken > 0, "{fast:?}");
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
    fn bone_holds_where_muscle_tears_until_the_force_is_enough() {
        let model = toy();
        let part = crew_physics::CrewBodyPart::Torso;
        let at = Vec3::new(0.07, 1.1, 0.0);
        let bone_torn = |work_j: f32, speed: f32| {
            let mut tissue = CasualtyTissue::new(&model);
            let solve = tissue.solve(&model, &[Strike { part, at, dir: Vec3::NEG_X, work_j, speed_mps: speed }]);
            let cells: Vec<u32> = solve.chunks.iter().flat_map(|c| c.cells.iter().copied()).collect();
            (cells.iter().filter(|&&c| model.cells[c as usize].tissue == Tissue::Bone).count(),
             cells.iter().filter(|&&c| model.cells[c as usize].tissue != Tissue::Bone).count())
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
        assert!(!solve.chunks.is_empty());
        let after = (0..model.cells.len()).filter(|&c| tissue.present[c] && model.exposed(&tissue.present, c)).count();
        assert!(after > shell - solve.chunks.iter().map(|c| c.cells.len()).sum::<usize>(), "the wound's walls are drawn");
        assert!(solve.chunks.iter().all(|c| c.cohesion_j >= 0.0));
        assert!(model.blend.iter().all(|b| b.is_none()), "one part: no joint");
    }

    /// Measures the real crew figure: cells by tissue, bonds, boxes for a whole body, build time.
    #[test]
    #[ignore]
    fn print_real_figure_tissue() {
        let figure = load_full_crew_mesh(CREW_RELAXED_GLBS[0], "tissue probe").expect("relaxed figure");
        let (bones, _) = crew_skeleton::fitted_bone_pages(&figure).expect("PSX skeleton");
        let start = std::time::Instant::now();
        let helmet = load_crew_steel_helmet().ok();
        let model = build_tissue(&figure, &bones, helmet.as_ref());
        println!("built {} cells, {} bonds in {:.0} ms, grid {:?}", model.cell_count(), model.bonds.len(),
            start.elapsed().as_secs_f32() * 1000.0, model.dims);
        let present = vec![true; model.cells.len()];
        let mut drawn = 0;
        for part in crew_physics::CrewBodyPart::ALL {
            let part_drawn = model.part_cells[part.index()].iter().filter(|&&c| model.exposed(&present, c as usize)).count();
            drawn += part_drawn;
            let t = |kind: Tissue| model.part_cells[part.index()].iter().filter(|&&c| model.cells[c as usize].tissue == kind).count();
            println!("{part:?}: skin {} muscle {} bone {} steel {}, {part_drawn} cubes drawn", t(Tissue::Skin), t(Tissue::Muscle), t(Tissue::Bone), t(Tissue::Steel));
        }
        println!("whole body: {drawn} cubes drawn; {} cells blended at joints", model.blend.iter().filter(|b| b.is_some()).count());
        // A 57 mm shot's 3.1 kg at 377 m/s through the torso, and a 20 g fragment at 700 m/s.
        let part = crew_physics::CrewBodyPart::Torso;
        let centre = part.rest_center();
        for (label, mass, speed) in [("57 mm shot", 3.14f32, 377.0f32), ("20 g fragment", 0.02, 700.0), ("2 g fragment", 0.002, 400.0)] {
            let mut tissue = CasualtyTissue::new(&model);
            let start = std::time::Instant::now();
            let solve = tissue.solve(&model, &[Strike { part, at: centre + Vec3::new(0.3, 0.05, 0.0), dir: Vec3::NEG_X,
                work_j: 0.5 * mass * speed * speed, speed_mps: speed }]);
            let torn: usize = solve.chunks.iter().map(|c| c.cells.len()).sum();
            println!("{label}: {} pieces, {torn} cells torn, {} bonds broken, lodged {}, soft share left {:.2}, {} steps in {:.1} ms",
                solve.chunks.len(), solve.bonds_broken, solve.lodged, tissue.soft_share(&model, part), solve.steps,
                start.elapsed().as_secs_f32() * 1000.0);
        }
    }
}
