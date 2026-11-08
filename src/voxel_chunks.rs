//! Experimental local-destruction storage. No renderer, global scene or FEA dependency.
//!
//! Air owns no allocation. A changed cell invalidates its chunk and only a face-adjacent
//! chunk when it lies on that boundary. Mesh publication consumes an explicit chunk budget;
//! authoritative collision reads the edited cells immediately, never a stale render mesh.
use std::collections::{BTreeSet, HashMap, VecDeque};

pub const EDGE: i32 = 16;
const CELLS: usize = (EDGE * EDGE * EDGE) as usize;
pub type Cell = [i32; 3];
pub type Material = u16; // 0 = air

struct Chunk {
    cells: Box<[Material; CELLS]>,
    occupied: usize,
}

#[derive(Default)]
pub struct World {
    chunks: HashMap<Cell, Chunk>,
    dirty: BTreeSet<Cell>,
    pending: VecDeque<Cell>,
    occupied: usize,
}

/// An outward-facing rectangle in integer voxel coordinates. Axis determines its normal;
/// u=(axis+1)%3 and v=(axis+2)%3. Extents cover equal-material exposed cells only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quad {
    pub origin: Cell,
    pub axis: usize,
    pub positive: bool,
    pub width: u8,
    pub height: u8,
    pub material: Material,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub cell: Cell,
    pub fraction: f64,
    pub material: Material,
}

fn address(p: Cell) -> (Cell, usize) {
    let chunk = p.map(|v| v.div_euclid(EDGE));
    let local = p.map(|v| v.rem_euclid(EDGE));
    (chunk, (local[0] + EDGE * (local[1] + EDGE * local[2])) as usize)
}

impl World {
    pub fn occupied(&self) -> usize { self.occupied }
    pub fn chunks(&self) -> usize { self.chunks.len() }
    /// Cell storage only; excludes map/queue/mesh allocations, not total process memory.
    pub fn cell_bytes(&self) -> usize { self.chunks.len() * CELLS * size_of::<Material>() }
    pub fn pending(&self) -> usize { self.dirty.len() }

    pub fn get(&self, p: Cell) -> Material {
        let (key, index) = address(p);
        self.chunks.get(&key).map_or(0, |chunk| chunk.cells[index])
    }

    fn invalidate(&mut self, key: Cell) {
        if self.dirty.insert(key) { self.pending.push_back(key); }
    }

    pub fn set(&mut self, p: Cell, material: Material) -> bool {
        let (key, index) = address(p);
        if self.chunks.get(&key).map_or(0, |chunk| chunk.cells[index]) == material {
            return false;
        }
        let chunk = self.chunks.entry(key).or_insert_with(|| Chunk {
            cells: Box::new([0; CELLS]), occupied: 0,
        });
        let old = chunk.cells[index];
        chunk.cells[index] = material;
        if old == 0 { chunk.occupied += 1; self.occupied += 1; }
        if material == 0 { chunk.occupied -= 1; self.occupied -= 1; }
        if chunk.occupied == 0 { self.chunks.remove(&key); }
        // An empty mesh is still published when a chunk disappears, retiring its old GPU page.
        self.invalidate(key);
        for axis in 0..3 {
            let local = p[axis].rem_euclid(EDGE);
            let step = if local == 0 { -1 } else if local == EDGE - 1 { 1 } else { 0 };
            if step != 0 {
                let mut neighbor = key; neighbor[axis] += step;
                if self.chunks.contains_key(&neighbor) { self.invalidate(neighbor); }
            }
        }
        true
    }

    /// Sweeps only the supplied elapsed-motion segment; it cannot damage a later state.
    /// Positions use voxel units. Cells are half-open, including negative world coordinates.
    pub fn first_solid(&self, from: [f64; 3], to: [f64; 3]) -> Option<Hit> {
        if !from.into_iter().chain(to).all(|v| v.is_finite() && v.abs() < i32::MAX as f64 / 2.0) {
            return None;
        }
        let delta = std::array::from_fn::<_, 3, _>(|i| to[i] - from[i]);
        let mut cell = from.map(|v| v.floor() as i32);
        let step = delta.map(|v| if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 });
        let stride = delta.map(|v| if v == 0.0 { f64::INFINITY } else { 1.0 / v.abs() });
        let mut crossing = std::array::from_fn::<_, 3, _>(|i| {
            if step[i] == 0 { f64::INFINITY }
            else { (cell[i] as f64 + if step[i] > 0 { 1.0 } else { 0.0 } - from[i]) / delta[i] }
        });
        let mut fraction = 0.0;
        loop {
            let material = self.get(cell);
            if material != 0 { return Some(Hit { cell, fraction, material }); }
            let next = crossing.into_iter().fold(f64::INFINITY, f64::min);
            if !next.is_finite() || next >= 1.0 {
                // A segment ending exactly on a negative boundary has not entered the cell
                // beyond that boundary. Classify the endpoint itself by the half-open rule.
                let endpoint = to.map(|v| v.floor() as i32);
                let material = self.get(endpoint);
                return (material != 0).then_some(Hit { cell: endpoint, fraction: 1.0, material });
            }
            // Step all simultaneous crossings. A zero-area corner touch is not a cell entry.
            for i in 0..3 {
                if crossing[i] == next { cell[i] += step[i]; crossing[i] += stride[i]; }
            }
            fraction = next;
        }
    }

    /// A deliberately authored local erasure tool, not a ballistics/material solver.
    pub fn erase_sphere(&mut self, centre: [f64; 3], radius: f64) -> usize {
        if !radius.is_finite() || !(0.0..=32.0).contains(&radius)
            || !centre.into_iter().all(|v| v.is_finite() && v.abs() < i32::MAX as f64 / 2.0) {
            return 0;
        }
        let lo = centre.map(|v| (v - radius).floor() as i32);
        let hi = centre.map(|v| (v + radius).floor() as i32);
        let mut removed = 0;
        for z in lo[2]..=hi[2] { for y in lo[1]..=hi[1] { for x in lo[0]..=hi[0] {
            let p = [x, y, z];
            let distance: f64 = (0..3).map(|i| (p[i] as f64 + 0.5 - centre[i]).powi(2)).sum();
            if distance <= radius * radius && self.set(p, 0) { removed += 1; }
        }}}
        removed
    }

    /// Deterministic FIFO publication. A bounded number of bounded-size chunks is meshed.
    /// Count budget is not a wall-clock guarantee; caller measures/deadlines independently.
    pub fn publish(&mut self, max_chunks: usize) -> Vec<(Cell, Vec<Quad>)> {
        let mut out = Vec::new();
        while out.len() < max_chunks {
            let Some(key) = self.pending.pop_front() else { break; };
            self.dirty.remove(&key);
            out.push((key, self.mesh(key)));
        }
        out
    }

    fn mesh(&self, key: Cell) -> Vec<Quad> {
        let Some(chunk) = self.chunks.get(&key) else { return Vec::new(); };
        let base = key.map(|v| v * EDGE);
        let mut quads = Vec::new();
        for axis in 0..3 { for positive in [false, true] { for layer in 0..EDGE {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            let mut mask = [0u16; (EDGE * EDGE) as usize];
            for j in 0..EDGE { for i in 0..EDGE {
                let mut p = [0; 3]; p[axis] = layer; p[u] = i; p[v] = j;
                let material = chunk.cells[(p[0] + EDGE * (p[1] + EDGE * p[2])) as usize];
                if material == 0 { continue; }
                let mut neighbor = std::array::from_fn(|a| base[a] + p[a]);
                neighbor[axis] += if positive { 1 } else { -1 };
                if self.get(neighbor) == 0 { mask[(j * EDGE + i) as usize] = material; }
            }}
            for j in 0..EDGE { for i in 0..EDGE {
                let material = mask[(j * EDGE + i) as usize];
                if material == 0 { continue; }
                let mut width = 1;
                while i + width < EDGE && mask[(j * EDGE + i + width) as usize] == material { width += 1; }
                let mut height = 1;
                while j + height < EDGE && (0..width).all(|x| mask[((j + height) * EDGE + i + x) as usize] == material) { height += 1; }
                for y in 0..height { for x in 0..width { mask[((j + y) * EDGE + i + x) as usize] = 0; }}
                let mut origin = base; origin[axis] += layer + i32::from(positive); origin[u] += i; origin[v] += j;
                quads.push(Quad { origin, axis, positive, width: width as u8, height: height as u8, material });
            }}
        }}}
        quads
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settle(world: &mut World) { while world.pending() > 0 { world.publish(8); } }

    #[test]
    fn negative_coordinates_empty_space_and_chunk_retirement() {
        let mut w = World::default();
        assert!(!w.set([-17, -1, 0], 0));
        assert_eq!(w.chunks(), 0);
        w.set([-17, -1, 0], 5);
        assert_eq!(w.get([-17, -1, 0]), 5);
        let mesh = w.publish(1); assert_eq!(mesh[0].0, [-2, -1, 0]);
        assert_eq!(mesh[0].1.len(), 6);
        w.set([-17, -1, 0], 0);
        assert_eq!((w.chunks(), w.occupied()), (0, 0));
        assert!(w.publish(1)[0].1.is_empty());
    }

    #[test]
    fn local_damage_invalidates_only_neighbors_sharing_changed_boundary() {
        let mut w = World::default();
        for p in [[15,5,5], [16,5,5], [32,5,5], [0,16,5]] { w.set(p, 1); }
        settle(&mut w);
        w.set([15,5,5], 0);
        assert_eq!(w.pending(), 2);
        assert_eq!(w.publish(1).len(), 1);
        assert_eq!(w.pending(), 1);
        let mesh = w.publish(1); assert_eq!(mesh[0].0, [1,0,0]);
        assert_eq!(mesh[0].1.len(), 6);
    }

    #[test]
    fn solid_cube_greedily_emits_only_six_rectangles() {
        let mut w = World::default();
        for z in 0..16 { for y in 0..16 { for x in 0..16 { w.set([x,y,z], 2); }}}
        let mesh = w.publish(1); assert_eq!(mesh[0].1.len(), 6);
        assert_eq!(mesh[0].1.iter().map(|q| q.width as u32 * q.height as u32).sum::<u32>(), 6 * 256);
        w.set([16, 5, 5], 3);
        let meshes = w.publish(8);
        assert_eq!(meshes.iter().flat_map(|(_,m)|m).map(|q| q.width as u32 * q.height as u32).sum::<u32>(), 6 * 256 + 4);
    }

    #[test]
    fn collision_uses_only_elapsed_segment_and_immediately_observes_damage() {
        let mut w = World::default(); w.set([10,0,0], 3); w.set([11,0,0], 4);
        assert!(w.first_solid([0.5,0.5,0.5], [9.9,0.5,0.5]).is_none());
        let hit = w.first_solid([9.9,0.5,0.5], [10.1,0.5,0.5]).unwrap();
        assert_eq!(hit.cell, [10,0,0]); assert!((hit.fraction - 0.5).abs() < 1e-10);
        w.set(hit.cell, 0); // no mesh publication is necessary for collision correctness
        assert_eq!(w.first_solid([9.9,0.5,0.5], [12.0,0.5,0.5]).unwrap().cell, [11,0,0]);
        assert!(w.first_solid([f64::NAN;3], [0.0;3]).is_none());
    }

    #[test]
    fn corner_crossing_negative_direction_and_zero_length_are_finite() {
        let mut w = World::default(); w.set([-2,-2,-2], 1);
        let h = w.first_solid([0.5;3], [-3.0;3]).unwrap(); assert_eq!(h.cell, [-2;3]);
        assert_eq!(w.first_solid([-1.5;3], [-1.5;3]).unwrap().fraction, 0.0);
        assert!(w.first_solid([0.5;3], [0.5;3]).is_none());
    }

    #[test]
    fn queue_coalesces_edits_and_zero_budget_does_not_publish() {
        let mut w = World::default();
        for x in 1..10 { w.set([x,1,1], 1); }
        assert_eq!(w.pending(), 1); assert!(w.publish(0).is_empty());
        assert_eq!(w.erase_sphere([5.5,1.5,1.5], 1.01), 3);
        assert_eq!(w.occupied(), 6); assert_eq!(w.pending(), 1);
        assert_eq!(w.publish(1).len(), 1); assert_eq!(w.pending(), 0);
    }

    #[test]
    fn half_open_segment_endpoint_does_not_enter_cell_beyond_negative_boundary() {
        let mut w = World::default();
        w.set([-1,0,0],1);
        assert!(w.first_solid([0.5,0.5,0.5],[0.,0.5,0.5]).is_none());
        assert_eq!(w.first_solid([0.5,0.5,0.5],[-0.01,0.5,0.5]).unwrap().cell,[-1,0,0]);
        w.set([0,0,0],2);
        assert_eq!(w.first_solid([1.5,0.5,0.5],[1.,0.5,0.5]),None);
        assert_eq!(w.first_solid([1.5,0.5,0.5],[0.,0.5,0.5]).unwrap().material,2);
        let mut corner = World::default(); corner.set([-1,-1,-1],3);
        assert!(corner.first_solid([0.5;3],[0.;3]).is_none());
        corner.set([-1,-1,-1],0);
        corner.set([0,0,0],4);
        assert_eq!(corner.first_solid([-0.5;3],[0.;3]).unwrap().cell,[0,0,0]);
    }

    #[test]
    fn nonfinite_queries_and_invalid_erasers_leave_world_unchanged() {
        let mut w = World::default(); w.set([0,0,0],1);
        for invalid in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY, f64::MAX] {
            assert!(w.first_solid([invalid,0.,0.],[0.;3]).is_none());
            assert!(w.first_solid([0.;3],[0.,invalid,0.]).is_none());
            assert_eq!(w.erase_sphere([invalid,0.,0.],1.),0);
            assert_eq!(w.erase_sphere([0.;3],invalid),0);
        }
        assert_eq!(w.erase_sphere([0.5;3],-1.),0);
        assert_eq!(w.erase_sphere([0.5;3],33.),0);
        assert_eq!(w.occupied(),1);
    }

    // Expand greedy rectangles into unit faces, preserving material and orientation. Equality
    // with a direct cell reference catches holes, overlaps, wrong winding planes and seams.
    fn unit_faces(meshes:&std::collections::BTreeMap<Cell,Vec<Quad>>)
        ->BTreeSet<(Cell,usize,bool,Material)> {
        let mut faces=BTreeSet::new();
        for quads in meshes.values() {for q in quads {
            assert!(q.width>0 && q.height>0);
            for j in 0..q.height {for i in 0..q.width {
                let mut origin=q.origin;
                origin[(q.axis+1)%3]+=i as i32;origin[(q.axis+2)%3]+=j as i32;
                assert!(faces.insert((origin,q.axis,q.positive,q.material)),"overlapping greedy faces");
            }}
        }}
        faces
    }

    fn reference_faces(cells:&std::collections::BTreeMap<Cell,Material>)
        ->BTreeSet<(Cell,usize,bool,Material)> {
        let mut faces=BTreeSet::new();
        for (&p,&material) in cells {for axis in 0..3 {for positive in [false,true] {
            let mut neighbor=p;neighbor[axis]+=if positive {1}else{-1};
            if !cells.contains_key(&neighbor) {
                let mut origin=p;origin[axis]+=i32::from(positive);
                faces.insert((origin,axis,positive,material));
            }
        }}}
        faces
    }

    #[test]
    fn random_sparse_greedy_faces_match_reference_and_insertion_order() {
        let mut seed=0x512c_a031u64;
        for case in 0..8 {
            let mut cells=std::collections::BTreeMap::new();
            for _ in 0..220 {
                let mut next=|| {seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1);(seed>>32) as u32};
                let p=[next() as i32%21-10,next() as i32%17-8,next() as i32%19-9];
                cells.insert(p,1+(next()%3) as Material);
            }
            // Dense mixed-material strips cross negative/positive chunk boundaries too.
            for x in -18..18 {cells.insert([x,case,0],1+(x.rem_euclid(2)) as Material);}
            let mut a=World::default();let mut b=World::default();
            for (&p,&m) in &cells {a.set(p,m);}
            for (&p,&m) in cells.iter().rev() {b.set(p,m);}
            let am=a.publish(usize::MAX).into_iter().collect::<std::collections::BTreeMap<_,_>>();
            let bm=b.publish(usize::MAX).into_iter().collect::<std::collections::BTreeMap<_,_>>();
            assert_eq!(am,bm,"mesh depends on insertion order");
            assert_eq!(unit_faces(&am),reference_faces(&cells));
            let mut updated=am;
            for p in cells.keys().step_by(5).copied().collect::<Vec<_>>() {a.set(p,0);cells.remove(&p);}
            while a.pending()>0 {for (key,mesh) in a.publish(2) {updated.insert(key,mesh);}}
            assert_eq!(a.occupied(),cells.len());
            assert_eq!(unit_faces(&updated),reference_faces(&cells));
        }
    }

    #[test]
    fn axis_sweeps_match_dense_half_open_reference() {
        let mut w=World::default();
        for x in [-19,-16,-1,0,15,16,31] {w.set([x,2,-3],(x+21) as u16);}
        // Every endpoint is an exact quarter-cell, making the reference independent of DDA
        // rounding while including zero length and exact positive/negative cell boundaries.
        for a in -84_i32..132 {for b in [-84,-76,-64,-4,0,1,60,64,124,132] {
            let from=[a as f64*0.25,2.5,-2.5];let to=[b as f64*0.25,2.5,-2.5];
            let mut expected=None;
            let steps=(b-a).abs().max(1)*2;
            for i in 0..=steps {
                let x=from[0]+(to[0]-from[0])*i as f64/steps as f64;
                let p=[x.floor() as i32,2,-3];
                if w.get(p)!=0 {expected=Some(p);break;}
            }
            assert_eq!(w.first_solid(from,to).map(|h|h.cell),expected,"{from:?} -> {to:?}");
        }}
    }
}
