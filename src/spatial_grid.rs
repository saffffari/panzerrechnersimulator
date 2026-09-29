//! Broad phase for the physical queries that meet thousands of building modules: tank crush,
//! blasts, shell and small-arms rays, infantry cover, stealth line of sight (urban intro F2,
//! 2026-09-28).
//!
//! A [`BoxIndex`] is a uniform XZ grid over static world boxes, two levels deep: every box (a
//! "member", addressed by its caller key) belongs to a group — the members of one building that
//! share a grid cell, or a lone box — and each cell lists the groups whose padded bounds overlap it.
//! A query visits only the cells it touches, tests each group's bounds, then hands the caller the
//! member keys; the caller runs its own exact test on its current box. Every query therefore
//! returns a superset of the boxes it can meet, and a caller that iterates candidates in the order
//! the linear scan used answers exactly what the scan answered. Its cost depends on the boxes near
//! the query, not on how many buildings the act holds.
//!
//! Robustness: boxes are registered with `PAD_M` of padding, so a box that only touches a cell
//! boundary, and a segment that passes a cell corner within float rounding, are still found.
//! Member bounds must be conservative for the life of the index: callers may shrink a box (a
//! collapsed wall's occluder) but never grow it beyond the bounds it was indexed with.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};

/// Registration padding around every box, metres.
pub const PAD_M: f32 = 0.01;
/// Largest dense cell table; a sparser world uses coarser cells rather than more memory.
const MAX_DENSE_CELLS: u64 = 1 << 22;
/// A group spanning more cells than this stays on a short broad list every query tests.
const MAX_CELLS_PER_GROUP: u64 = 4096;

#[derive(Clone, Copy, Debug)]
struct Group {
    min: Vec3,
    max: Vec3,
    first: u32,
    len: u32,
}

/// Counters of one query, for tests and the micro-benchmark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryWork {
    pub cells: u32,
    pub groups: u32,
    pub members: u32,
}

#[derive(Default)]
pub struct BoxIndex {
    cell: f32,
    origin: (i32, i32),
    dims: (u32, u32),
    /// CSR table: groups of dense cell `c` are `cell_groups[cell_start[c]..cell_start[c + 1]]`.
    cell_start: Vec<u32>,
    cell_groups: Vec<u32>,
    broad: Vec<u32>,
    groups: Vec<Group>,
    /// Member keys in group order (ascending within a group) and their indexed bounds.
    members: Vec<u32>,
    member_bounds: Vec<(Vec3, Vec3)>,
}

fn cell_coord(value: f32, inv_cell: f32) -> i32 {
    (value * inv_cell).floor().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

impl BoxIndex {
    /// Index `(key, min, max, group)` boxes. Boxes with the same `group` (a building instance)
    /// whose centres fall in one cell form one group; `None` boxes stand alone. Non-finite boxes
    /// go to the broad list, so they are still returned by every query.
    pub fn build(cell_m: f32, items: impl IntoIterator<Item = (u32, Vec3, Vec3, Option<u32>)>) -> Self {
        assert!(cell_m.is_finite() && cell_m > 0.0, "grid cell must be a positive length");
        let items: Vec<_> = items.into_iter().collect();
        let mut cell = cell_m;
        let inv = |cell: f32| 1.0 / cell;
        // Group members: (group id or unique, centre cell) → member list in key order.
        let mut grouped: BTreeMap<(u8, u32, i32, i32), Vec<usize>> = BTreeMap::new();
        for (index, &(key, min, max, group)) in items.iter().enumerate() {
            let finite = min.is_finite() && max.is_finite();
            let entry = match (group, finite) {
                (Some(group), true) => {
                    let c = (min + max) * 0.5;
                    (0, group, cell_coord(c.x, inv(cell)), cell_coord(c.z, inv(cell)))
                }
                _ => (1, key, index as i32, 0),
            };
            grouped.entry(entry).or_default().push(index);
        }
        let mut groups = Vec::with_capacity(grouped.len());
        let mut members = Vec::with_capacity(items.len());
        let mut member_bounds = Vec::with_capacity(items.len());
        for (_, mut list) in grouped {
            list.sort_by_key(|&index| items[index].0);
            let first = members.len() as u32;
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for &index in &list {
                let (key, lo, hi, _) = items[index];
                members.push(key);
                member_bounds.push((lo, hi));
                min = min.min(lo);
                max = max.max(hi);
            }
            groups.push(Group { min, max, first, len: list.len() as u32 });
        }
        // Dense cell rectangle over every finite group; coarsen the cell if it would be huge.
        let finite: Vec<usize> = (0..groups.len())
            .filter(|&g| groups[g].min.is_finite() && groups[g].max.is_finite())
            .collect();
        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for &g in &finite {
            lo = lo.min(Vec2::new(groups[g].min.x, groups[g].min.z) - PAD_M);
            hi = hi.max(Vec2::new(groups[g].max.x, groups[g].max.z) + PAD_M);
        }
        let mut origin = (0, 0);
        let mut dims = (0u32, 0u32);
        if !finite.is_empty() {
            loop {
                let (x0, z0) = (cell_coord(lo.x, inv(cell)), cell_coord(lo.y, inv(cell)));
                let (x1, z1) = (cell_coord(hi.x, inv(cell)), cell_coord(hi.y, inv(cell)));
                let nx = (i64::from(x1) - i64::from(x0) + 1) as u64;
                let nz = (i64::from(z1) - i64::from(z0) + 1) as u64;
                if nx.saturating_mul(nz) <= MAX_DENSE_CELLS {
                    origin = (x0, z0);
                    dims = (nx as u32, nz as u32);
                    break;
                }
                cell *= 2.0;
            }
        }
        let cells = dims.0 as usize * dims.1 as usize;
        let mut counts = vec![0u32; cells + 1];
        let mut broad = Vec::new();
        let mut spans = Vec::with_capacity(groups.len());
        for (g, group) in groups.iter().enumerate() {
            let span = (group.min.is_finite() && group.max.is_finite()).then(|| {
                let x0 = cell_coord(group.min.x - PAD_M, inv(cell)) - origin.0;
                let z0 = cell_coord(group.min.z - PAD_M, inv(cell)) - origin.1;
                let x1 = cell_coord(group.max.x + PAD_M, inv(cell)) - origin.0;
                let z1 = cell_coord(group.max.z + PAD_M, inv(cell)) - origin.1;
                (x0, z0, x1, z1)
            });
            let span = span.filter(|&(x0, z0, x1, z1)| {
                ((x1 - x0 + 1) as u64) * ((z1 - z0 + 1) as u64) <= MAX_CELLS_PER_GROUP
            });
            match span {
                Some((x0, z0, x1, z1)) => {
                    for z in z0..=z1 {
                        for x in x0..=x1 {
                            counts[z as usize * dims.0 as usize + x as usize] += 1;
                        }
                    }
                }
                None => broad.push(g as u32),
            }
            spans.push(span);
        }
        let mut cell_start = vec![0u32; cells + 1];
        for c in 0..cells {
            cell_start[c + 1] = cell_start[c] + counts[c];
        }
        let mut fill = cell_start.clone();
        let mut cell_groups = vec![0u32; cell_start[cells] as usize];
        for (g, span) in spans.iter().enumerate() {
            if let Some((x0, z0, x1, z1)) = *span {
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        let c = z as usize * dims.0 as usize + x as usize;
                        cell_groups[fill[c] as usize] = g as u32;
                        fill[c] += 1;
                    }
                }
            }
        }
        Self { cell, origin, dims, cell_start, cell_groups, broad, groups, members, member_bounds }
    }

    /// Indexed member count.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    fn cell_slice(&self, x: i32, z: i32) -> &[u32] {
        let (lx, lz) = (i64::from(x) - i64::from(self.origin.0), i64::from(z) - i64::from(self.origin.1));
        if lx < 0 || lz < 0 || lx >= i64::from(self.dims.0) || lz >= i64::from(self.dims.1) {
            return &[];
        }
        let c = lz as usize * self.dims.0 as usize + lx as usize;
        &self.cell_groups[self.cell_start[c] as usize..self.cell_start[c + 1] as usize]
    }

    fn group_members(&self, g: u32) -> std::ops::Range<usize> {
        let group = self.groups[g as usize];
        group.first as usize..(group.first + group.len) as usize
    }

    /// Ascending, unique keys whose indexed box overlaps the XZ rectangle `[min, max]` (widened
    /// by the padding). `out` is cleared first.
    pub fn keys_in_rect(&self, min: Vec2, max: Vec2, out: &mut Vec<u32>) -> QueryWork {
        out.clear();
        let mut work = QueryWork::default();
        if self.is_empty() || !(min.is_finite() && max.is_finite()) {
            // Non-finite queries are conservative: every member.
            out.extend_from_slice(&self.members);
            out.sort_unstable();
            out.dedup();
            return work;
        }
        let (min, max) = (min - PAD_M, max + PAD_M);
        let overlaps = |lo: Vec3, hi: Vec3| {
            !(lo.is_finite() && hi.is_finite())
                || (hi.x >= min.x && lo.x <= max.x && hi.z >= min.y && lo.z <= max.y)
        };
        let take_group = |g: u32, out: &mut Vec<u32>, work: &mut QueryWork| {
            work.groups += 1;
            let group = self.groups[g as usize];
            if !overlaps(group.min, group.max) {
                return;
            }
            for m in self.group_members(g) {
                work.members += 1;
                let (lo, hi) = self.member_bounds[m];
                if overlaps(lo, hi) {
                    out.push(self.members[m]);
                }
            }
        };
        for &g in &self.broad {
            take_group(g, out, &mut work);
        }
        let inv = 1.0 / self.cell;
        let x0 = cell_coord(min.x, inv).max(self.origin.0);
        let z0 = cell_coord(min.y, inv).max(self.origin.1);
        let x1 = cell_coord(max.x, inv).min(self.origin.0.saturating_add(self.dims.0 as i32 - 1));
        let z1 = cell_coord(max.y, inv).min(self.origin.1.saturating_add(self.dims.1 as i32 - 1));
        for z in z0..=z1 {
            for x in x0..=x1 {
                work.cells += 1;
                for &g in self.cell_slice(x, z) {
                    take_group(g, out, &mut work);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        work
    }

    /// Visit the cells the XZ projection of `a → b` crosses, front to back, clipped to the grid.
    /// `visit(cell groups, fraction of the segment at which the cell is left)`.
    fn walk_cells(&self, a: Vec3, b: Vec3, mut visit: impl FnMut(&[u32], f32) -> bool) {
        if self.dims.0 == 0 || self.dims.1 == 0 {
            return;
        }
        let inv = 1.0 / self.cell;
        let p0 = Vec2::new(a.x, a.z) * inv;
        let p1 = Vec2::new(b.x, b.z) * inv;
        let d = p1 - p0;
        // Clip to the dense rectangle (Liang–Barsky), widened by one cell for rounding.
        let lo = Vec2::new(self.origin.0 as f32 - 1.0, self.origin.1 as f32 - 1.0);
        let hi = lo + Vec2::new(self.dims.0 as f32 + 2.0, self.dims.1 as f32 + 2.0);
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for axis in 0..2 {
            if d[axis].abs() < 1e-12 {
                if p0[axis] < lo[axis] || p0[axis] > hi[axis] {
                    return;
                }
                continue;
            }
            let (mut ta, mut tb) = ((lo[axis] - p0[axis]) / d[axis], (hi[axis] - p0[axis]) / d[axis]);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return;
            }
        }
        let start = p0 + d * t0;
        let mut cx = start.x.floor() as i64;
        let mut cz = start.y.floor() as i64;
        let step = |v: f32| if v > 0.0 { 1i64 } else if v < 0.0 { -1 } else { 0 };
        let (sx, sz) = (step(d.x), step(d.y));
        let t_delta = |v: f32| if v != 0.0 { 1.0 / v.abs() } else { f32::INFINITY };
        let (dx, dz) = (t_delta(d.x), t_delta(d.y));
        let first_boundary = |c: i64, p: f32, v: f32| {
            if v > 0.0 {
                (c as f32 + 1.0 - p) / v
            } else if v < 0.0 {
                (p - c as f32) / -v
            } else {
                f32::INFINITY
            }
        };
        // Parameters are fractions of the whole segment.
        let mut tx = t0 + first_boundary(cx, start.x, d.x);
        let mut tz = t0 + first_boundary(cz, start.y, d.y);
        let max_steps = self.dims.0 as u64 + self.dims.1 as u64 + 8;
        for _ in 0..max_steps {
            let t_exit = tx.min(tz).min(t1);
            let slice = self.cell_slice(
                cx.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
                cz.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            );
            if !visit(slice, t_exit) {
                return;
            }
            if t_exit >= t1 {
                return;
            }
            if tx < tz {
                cx += sx;
                tx += dx;
            } else {
                cz += sz;
                tz += dz;
            }
        }
    }

    /// Segment against a group's bounds (slab test, padded).
    fn segment_meets(a: Vec3, delta: Vec3, min: Vec3, max: Vec3) -> bool {
        if !(min.is_finite() && max.is_finite()) {
            return true;
        }
        let (min, max) = (min - PAD_M, max + PAD_M);
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for axis in 0..3 {
            if delta[axis].abs() < 1e-12 {
                if a[axis] < min[axis] || a[axis] > max[axis] {
                    return false;
                }
                continue;
            }
            let (mut ta, mut tb) = ((min[axis] - a[axis]) / delta[axis], (max[axis] - a[axis]) / delta[axis]);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return false;
            }
        }
        true
    }

    /// True when `hit(key)` holds for some member whose indexed box the segment `a → b` may meet.
    /// Candidates come front to back and the walk stops at the first hit.
    pub fn any_on_segment(&self, a: Vec3, b: Vec3, mut hit: impl FnMut(u32) -> bool) -> bool {
        self.any_on_segment_counted(a, b, &mut hit).0
    }

    pub fn any_on_segment_counted(
        &self,
        a: Vec3,
        b: Vec3,
        hit: &mut dyn FnMut(u32) -> bool,
    ) -> (bool, QueryWork) {
        let mut work = QueryWork::default();
        if self.is_empty() {
            return (false, work);
        }
        let delta = b - a;
        if !(a.is_finite() && b.is_finite()) {
            let found = self.members.iter().any(|&key| hit(key));
            return (found, work);
        }
        let mut test_group = |g: u32, work: &mut QueryWork| -> bool {
            work.groups += 1;
            let group = self.groups[g as usize];
            if !Self::segment_meets(a, delta, group.min, group.max) {
                return false;
            }
            self.group_members(g).any(|m| {
                work.members += 1;
                let (lo, hi) = self.member_bounds[m];
                Self::segment_meets(a, delta, lo, hi) && hit(self.members[m])
            })
        };
        for &g in &self.broad {
            if test_group(g, &mut work) {
                return (true, work);
            }
        }
        let mut found = false;
        self.walk_cells(a, b, |groups, _| {
            work.cells += 1;
            for &g in groups {
                if test_group(g, &mut work) {
                    found = true;
                    return false;
                }
            }
            true
        });
        (found, work)
    }

    /// The nearest member along `a → b`: `hit(key)` returns the caller's exact entry distance in
    /// metres from `a` (or `None`). Ties go to the lowest key, as a linear scan with a strict `<`
    /// in key order would answer. Cells come front to back; the walk stops once no unvisited
    /// member can be nearer.
    pub fn nearest_on_segment(
        &self,
        a: Vec3,
        b: Vec3,
        mut hit: impl FnMut(u32) -> Option<f32>,
    ) -> Option<(f32, u32)> {
        self.nearest_on_segment_counted(a, b, &mut hit).0
    }

    pub fn nearest_on_segment_counted(
        &self,
        a: Vec3,
        b: Vec3,
        hit: &mut dyn FnMut(u32) -> Option<f32>,
    ) -> (Option<(f32, u32)>, QueryWork) {
        let mut work = QueryWork::default();
        let mut best: Option<(f32, u32)> = None;
        if self.is_empty() {
            return (best, work);
        }
        let mut consider = |key: u32, best: &mut Option<(f32, u32)>| {
            if let Some(t) = hit(key) {
                if best.is_none_or(|(bt, bk)| t < bt || (t == bt && key < bk)) {
                    *best = Some((t, key));
                }
            }
        };
        if !(a.is_finite() && b.is_finite()) {
            for &key in &self.members {
                consider(key, &mut best);
            }
            return (best, work);
        }
        let delta = b - a;
        let length = delta.length();
        let mut test_group = |g: u32, best: &mut Option<(f32, u32)>, work: &mut QueryWork| {
            work.groups += 1;
            let group = self.groups[g as usize];
            if !Self::segment_meets(a, delta, group.min, group.max) {
                return;
            }
            for m in self.group_members(g) {
                work.members += 1;
                let (lo, hi) = self.member_bounds[m];
                if Self::segment_meets(a, delta, lo, hi) {
                    consider(self.members[m], best);
                }
            }
        };
        for &g in &self.broad {
            test_group(g, &mut best, &mut work);
        }
        self.walk_cells(a, b, |groups, t_exit| {
            work.cells += 1;
            for &g in groups {
                test_group(g, &mut best, &mut work);
            }
            // Every member met before this cell's exit has been visited (padded registration).
            best.is_none_or(|(t, _)| t >= t_exit * length - PAD_M)
        });
        (best, work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic counter hash in [0, 1).
    fn h(n: u32) -> f32 {
        (n.wrapping_mul(2_654_435_761) >> 8) as f32 / (1u32 << 24) as f32
    }

    fn slab(a: Vec3, b: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
        let delta = b - a;
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for axis in 0..3 {
            if delta[axis].abs() < 1e-12 {
                if a[axis] < min[axis] || a[axis] > max[axis] {
                    return None;
                }
                continue;
            }
            let (mut ta, mut tb) = ((min[axis] - a[axis]) / delta[axis], (max[axis] - a[axis]) / delta[axis]);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return None;
            }
        }
        Some(t0 * delta.length())
    }

    /// A street grid of 8 m "houses", each a group of wall boxes; some boxes exactly on 16 m cell
    /// boundaries and corners, plus a few huge and non-finite boxes.
    fn city(houses_x: u32, houses_z: u32) -> Vec<(u32, Vec3, Vec3, Option<u32>)> {
        let mut boxes = Vec::new();
        let mut key = 0u32;
        for hx in 0..houses_x {
            for hz in 0..houses_z {
                let house = hx * houses_z + hz;
                let o = Vec3::new(hx as f32 * 16.0, 0.0, hz as f32 * 16.0);
                let walls = [
                    (Vec3::new(0.0, 0.0, 0.0), Vec3::new(8.0, 3.0, 0.2)),
                    (Vec3::new(0.0, 0.0, 7.8), Vec3::new(8.0, 3.0, 8.0)),
                    (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.2, 3.0, 8.0)),
                    (Vec3::new(7.8, 0.0, 0.0), Vec3::new(8.0, 3.0, 8.0)),
                    (Vec3::new(0.0, 3.0, 0.0), Vec3::new(8.0, 3.4, 8.0)),
                ];
                for (min, max) in walls {
                    boxes.push((key, o + min, o + max, Some(house)));
                    key += 1;
                }
            }
        }
        boxes.push((key, Vec3::new(-2000.0, 0.0, -0.5), Vec3::new(2000.0, 2.0, 0.5), None));
        key += 1;
        boxes.push((key, Vec3::new(f32::NAN, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0), None));
        key += 1;
        boxes.push((key, Vec3::new(31.99, 0.0, 47.99), Vec3::new(32.0, 1.0, 48.0), None));
        boxes
    }

    /// Every query answers exactly what the linear scan answers: rectangles, any-hit and nearest
    /// segments over thousands of random queries, including cell corners and grid-aligned boxes.
    #[test]
    fn queries_equal_the_linear_scan() {
        let boxes = city(12, 9);
        for cell in [16.0, 32.0, 7.0] {
            let index = BoxIndex::build(cell, boxes.iter().copied());
            assert_eq!(index.len(), boxes.len());
            let mut out = Vec::new();
            for n in 0..3000u32 {
                let a = Vec3::new(h(n * 7) * 220.0 - 20.0, h(n * 7 + 1) * 5.0, h(n * 7 + 2) * 170.0 - 20.0);
                let far = n % 3 == 0;
                let reach = if far { 180.0 } else { 9.0 };
                let mut b = a + Vec3::new(
                    (h(n * 7 + 3) - 0.5) * 2.0 * reach,
                    (h(n * 7 + 4) - 0.5) * 3.0,
                    (h(n * 7 + 5) - 0.5) * 2.0 * reach,
                );
                if n % 11 == 0 {
                    // Exactly through a cell corner and along a cell boundary.
                    b = Vec3::new(a.x + 32.0, a.y, a.z + 32.0);
                }
                if n % 13 == 0 {
                    b = Vec3::new(a.x, a.y + 1.0, a.z); // vertical
                }
                let finite = |(_, lo, hi, _): &(u32, Vec3, Vec3, Option<u32>)| lo.is_finite() && hi.is_finite();
                // Any-hit (a non-finite box is never met).
                let exact_any = boxes.iter().any(|b0| finite(b0) && slab(a, b, b0.1, b0.2).is_some());
                let grid_any = index.any_on_segment(a, b, |key| {
                    let b0 = boxes[key as usize];
                    finite(&b0) && slab(a, b, b0.1, b0.2).is_some()
                });
                assert_eq!(grid_any, exact_any, "any-hit {a:?} -> {b:?}");
                // Nearest (finite boxes; the NaN box never reports a distance).
                let exact_near = boxes
                    .iter()
                    .filter(|b0| finite(b0))
                    .filter_map(|b0| slab(a, b, b0.1, b0.2).map(|t| (t, b0.0)))
                    .fold(None::<(f32, u32)>, |best, (t, k)| match best {
                        Some((bt, _)) if bt <= t => best,
                        _ => Some((t, k)),
                    });
                let grid_near = index.nearest_on_segment(a, b, |key| {
                    let b0 = boxes[key as usize];
                    if finite(&b0) { slab(a, b, b0.1, b0.2) } else { None }
                });
                assert_eq!(grid_near, exact_near, "nearest {a:?} -> {b:?}");
                // Rectangle.
                let (lo, hi) = (Vec2::new(a.x.min(b.x), a.z.min(b.z)), Vec2::new(a.x.max(b.x), a.z.max(b.z)));
                index.keys_in_rect(lo, hi, &mut out);
                for b0 in &boxes {
                    let overlaps = !finite(b0)
                        || (b0.2.x >= lo.x && b0.1.x <= hi.x && b0.2.z >= lo.y && b0.1.z <= hi.y);
                    if overlaps {
                        assert!(out.binary_search(&b0.0).is_ok(), "rect lost box {} for {lo:?}..{hi:?}", b0.0);
                    }
                }
                assert!(out.windows(2).all(|w| w[0] < w[1]), "rect keys ascend, unique");
            }
        }
    }

    #[test]
    fn empty_and_non_finite_queries_are_conservative() {
        let empty = BoxIndex::build(16.0, std::iter::empty());
        assert!(!empty.any_on_segment(Vec3::ZERO, Vec3::X, |_| true));
        let boxes = city(2, 2);
        let index = BoxIndex::build(16.0, boxes.iter().copied());
        let mut out = Vec::new();
        index.keys_in_rect(Vec2::splat(f32::NAN), Vec2::ZERO, &mut out);
        assert_eq!(out.len(), boxes.len());
        assert!(index.any_on_segment(Vec3::splat(f32::INFINITY), Vec3::ZERO, |_| true));
    }

    /// Per-query work stays flat as the city grows: the same local queries visit the same cells,
    /// groups and members whether the district holds 16 or 4,096 houses.
    #[test]
    fn per_query_work_does_not_grow_with_the_building_count() {
        let mut previous: Option<(u32, u32)> = None;
        for side in [4u32, 16, 64] {
            let boxes = city(side, side);
            let index = BoxIndex::build(16.0, boxes.iter().copied());
            let (mut members, mut groups) = (0u32, 0u32);
            let mut out = Vec::new();
            for n in 0..200u32 {
                // Local queries inside the first 45 m of the district (the four houses every size
                // shares along each axis end at 56 m).
                let a = Vec3::new(h(n) * 40.0, 1.5, h(n + 999) * 40.0);
                let b = a + Vec3::new((h(n + 5) - 0.5) * 10.0, 0.0, (h(n + 7) - 0.5) * 10.0);
                let (_, work) = index.nearest_on_segment_counted(a, b, &mut |_| None);
                members += work.members;
                groups += work.groups;
                let work = index.keys_in_rect(Vec2::new(a.x, a.z) - 3.0, Vec2::new(a.x, a.z) + 3.0, &mut out);
                members += work.members;
                groups += work.groups;
            }
            if let Some((m, g)) = previous {
                assert_eq!((members, groups), (m, g), "work grew with {} houses", side * side);
            }
            previous = Some((members, groups));
        }
    }
}
