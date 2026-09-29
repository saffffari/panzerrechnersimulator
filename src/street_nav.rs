//! The T-34 platoons' map of a street district (urban intro, 29 Sep 2026): which cells of a 4 m grid
//! a T-34 on a route has room in, measured on the built district by the caller
//! (`Game::street_route_cell_clear` on the district's ways: no story cover, standing destruction
//! record or sight occluder within 3 m of the cell centre up to the hull's roof, and under 1.2 m of
//! relief across that box), and the shortest route along those cells between two points.
//!
//! This is route planning, not collision. Enemy hulls do not collide with building walls (the crush
//! and wall contact are the player's only; `docs/PHYSICAL_TRUTH_AUDIT.md`), so in the streets this map
//! and the manoeuvre veto (`Game::winter_safe_maneuver`: the hull's width clear 4 and 8 m ahead) are
//! what keep a T-34 out of the buildings. The grid is a discretisation of that measurement: a cell is
//! clear when it passes at the cell's centre; a route runs through clear cell centres (diagonal steps
//! only past two clear neighbours) and is shortened only where every point sampled every half cell
//! along the shortcut is clear.
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use glam::Vec2;

/// Grid pitch: a third of an ordinary street's 18 m, so its clear band is two or three cells wide.
pub(super) const NAV_CELL_M: f32 = 4.0;
/// Rooms beyond this many cells from a blocked cell cost the same (a street's middle is preferred
/// over its edges, open ground over neither).
const ROOM_CAP: u8 = 3;
/// A route is re-planned when the hull is this far off it (pushed by another tank, or by the veto).
pub(super) const REPLAN_OFF_M: f32 = 12.0;
/// Pure pursuit: the hull steers for the point this far ahead along its route.
pub(super) const CARROT_M: f32 = 10.0;

#[derive(Clone, Debug)]
pub(super) struct StreetNav {
    /// World XZ of the grid's minimum corner.
    origin: Vec2,
    cell: f32,
    w: usize,
    h: usize,
    clear: Vec<bool>,
    /// Chebyshev distance in cells to the nearest blocked cell, capped at `ROOM_CAP`.
    room: Vec<u8>,
}

impl StreetNav {
    /// Measure every cell centre of the rectangle `min..max` with `clear`.
    pub fn build(min: Vec2, max: Vec2, cell: f32, mut clear: impl FnMut(Vec2) -> bool) -> Self {
        let w = ((max.x - min.x) / cell).ceil().max(1.0) as usize;
        let h = ((max.y - min.y) / cell).ceil().max(1.0) as usize;
        let mut mask = Vec::with_capacity(w * h);
        for j in 0..h {
            for i in 0..w {
                mask.push(clear(min + Vec2::new((i as f32 + 0.5) * cell, (j as f32 + 0.5) * cell)));
            }
        }
        Self::from_mask(min, cell, w, h, mask)
    }

    pub fn from_mask(origin: Vec2, cell: f32, w: usize, h: usize, clear: Vec<bool>) -> Self {
        assert_eq!(clear.len(), w * h);
        let mut room = vec![ROOM_CAP; w * h];
        let mut frontier: Vec<usize> = (0..w * h).filter(|&k| !clear[k]).collect();
        for &k in &frontier { room[k] = 0; }
        let mut level = 0u8;
        while !frontier.is_empty() && level < ROOM_CAP {
            level += 1;
            let mut next = Vec::new();
            for k in frontier {
                let (i, j) = ((k % w) as i64, (k / w) as i64);
                for (di, dj) in NEIGHBOURS {
                    let (ni, nj) = (i + di, j + dj);
                    if ni < 0 || nj < 0 || ni >= w as i64 || nj >= h as i64 { continue; }
                    let n = nj as usize * w + ni as usize;
                    if room[n] > level {
                        room[n] = level;
                        next.push(n);
                    }
                }
            }
            frontier = next;
        }
        Self { origin, cell, w, h, clear, room }
    }

    pub fn clear_cells(&self) -> usize { self.clear.iter().filter(|&&c| c).count() }
    pub fn size(&self) -> (usize, usize) { (self.w, self.h) }
    pub fn bounds(&self) -> (Vec2, Vec2) {
        (self.origin, self.origin + Vec2::new(self.w as f32, self.h as f32) * self.cell)
    }

    fn centre(&self, k: usize) -> Vec2 {
        self.origin + Vec2::new((k % self.w) as f32 + 0.5, (k / self.w) as f32 + 0.5) * self.cell
    }

    fn cell_of(&self, p: Vec2) -> Option<usize> {
        let q = (p - self.origin) / self.cell;
        (q.x >= 0.0 && q.y >= 0.0 && (q.x as usize) < self.w && (q.y as usize) < self.h)
            .then(|| q.y as usize * self.w + q.x as usize)
    }

    /// Outside the measured rectangle nothing is known to stand (the district and a wide margin of
    /// open ground are measured).
    pub fn is_clear(&self, p: Vec2) -> bool {
        self.cell_of(p).is_none_or(|k| self.clear[k])
    }

    /// Every point sampled each half cell along `a → b` is clear.
    pub fn segment_clear(&self, a: Vec2, b: Vec2) -> bool {
        let n = ((a.distance(b) / (self.cell * 0.5)).ceil() as usize).max(1);
        (0..=n).all(|s| self.is_clear(a.lerp(b, s as f32 / n as f32)))
    }

    /// The clear cell nearest `p` (by centre distance) within `reach` metres, if any.
    fn nearest_clear_cell(&self, p: Vec2, reach: f32) -> Option<usize> {
        let q = ((p - self.origin) / self.cell).floor();
        let (ci, cj) = (q.x as i64, q.y as i64);
        let r = (reach / self.cell).ceil() as i64;
        let mut best: Option<(f32, usize)> = None;
        for j in (cj - r).max(0)..=(cj + r).min(self.h as i64 - 1) {
            for i in (ci - r).max(0)..=(ci + r).min(self.w as i64 - 1) {
                let k = j as usize * self.w + i as usize;
                if !self.clear[k] { continue; }
                let d = self.centre(k).distance(p);
                if d <= reach && best.is_none_or(|(bd, bk)| d < bd || (d == bd && k < bk)) {
                    best = Some((d, k));
                }
            }
        }
        best.map(|(_, k)| k)
    }

    /// The clear point nearest `p` within `reach` metres: `p` itself when it is clear.
    pub fn nearest_clear(&self, p: Vec2, reach: f32) -> Option<Vec2> {
        if self.is_clear(p) { return Some(p); }
        self.nearest_clear_cell(p, reach).map(|k| self.centre(k))
    }

    /// The route from `from` to `to` over clear cells (A*, eight neighbours, diagonal steps only
    /// past two clear neighbours; a cell near a wall costs up to 1.6 times an open one), shortened
    /// where the shortcut is clear. It starts at `from` and ends at `to`; `None` when either lies
    /// more than 16 m from a clear cell or no route connects them.
    pub fn path(&self, from: Vec2, to: Vec2) -> Option<Vec<Vec2>> {
        let start = self.nearest_clear_cell(from, 16.0)?;
        let goal = self.nearest_clear_cell(to, 16.0)?;
        let (w, h) = (self.w as i64, self.h as i64);
        let cost_at = |k: usize| 100 + 60 * (ROOM_CAP - self.room[k].min(ROOM_CAP)) as u32 / ROOM_CAP as u32;
        let (gi, gj) = ((goal % self.w) as i64, (goal / self.w) as i64);
        let heuristic = |k: usize| {
            let (dx, dy) = (((k % self.w) as i64 - gi).abs(), ((k / self.w) as i64 - gj).abs());
            (100 * (dx.max(dy) - dx.min(dy)) + 141 * dx.min(dy)) as u32
        };
        let mut g = vec![u32::MAX; self.w * self.h];
        let mut came = vec![u32::MAX; self.w * self.h];
        let mut open = BinaryHeap::new();
        g[start] = 0;
        open.push(Reverse((heuristic(start), start)));
        while let Some(Reverse((_, k))) = open.pop() {
            if k == goal { break; }
            let (i, j) = ((k % self.w) as i64, (k / self.w) as i64);
            for (di, dj) in NEIGHBOURS {
                let (ni, nj) = (i + di, j + dj);
                if ni < 0 || nj < 0 || ni >= w || nj >= h { continue; }
                let n = nj as usize * self.w + ni as usize;
                if !self.clear[n] { continue; }
                if di != 0 && dj != 0 {
                    let side_a = j as usize * self.w + ni as usize;
                    let side_b = nj as usize * self.w + i as usize;
                    if !self.clear[side_a] || !self.clear[side_b] { continue; }
                }
                let step = if di != 0 && dj != 0 { 141 } else { 100 };
                let tentative = g[k].saturating_add(step * cost_at(n) / 100);
                if tentative < g[n] {
                    g[n] = tentative;
                    came[n] = k as u32;
                    open.push(Reverse((tentative.saturating_add(heuristic(n)), n)));
                }
            }
        }
        if g[goal] == u32::MAX { return None; }
        let mut cells = vec![goal];
        while *cells.last().unwrap() != start {
            cells.push(came[*cells.last().unwrap()] as usize);
        }
        cells.reverse();
        let mut raw = Vec::with_capacity(cells.len() + 2);
        raw.push(from);
        raw.extend(cells.iter().map(|&k| self.centre(k)));
        raw.push(to);
        // String-pull: from each kept point, jump to the farthest later point in clear sight.
        let mut out = vec![raw[0]];
        let mut at = 0;
        while at < raw.len() - 1 {
            let mut next = at + 1;
            for later in (at + 2..raw.len()).rev() {
                if self.segment_clear(raw[at], raw[later]) { next = later; break; }
            }
            out.push(raw[next]);
            at = next;
        }
        Some(out)
    }
}

const NEIGHBOURS: [(i64, i64); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];

fn project(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-6 { 1.0 } else { (p - a).dot(ab) / len2 }
}

/// One street route a tank is driving, followed by pure pursuit.
#[derive(Clone, Debug)]
pub(super) struct Leg {
    pub goal: Vec2,
    path: Vec<Vec2>,
    /// Index of the route point the hull is heading for.
    next: usize,
}

impl Leg {
    pub fn plan(nav: &StreetNav, from: Vec2, goal: Vec2) -> Option<Self> {
        nav.path(from, goal).map(|path| Self { goal, path, next: 1 })
    }

    /// Advance along the route: the point `ahead` metres on from the hull's foot on it, the route
    /// length left from that foot, and how far the hull stands off the route.
    pub fn track(&mut self, pos: Vec2, ahead: f32) -> (Vec2, f32, f32) {
        let n = self.path.len();
        if n < 2 { return (self.goal, pos.distance(self.goal), 0.0); }
        while self.next < n - 1 {
            let (a, b) = (self.path[self.next - 1], self.path[self.next]);
            if project(pos, a, b) >= 1.0 || pos.distance(b) < 3.0 { self.next += 1; } else { break; }
        }
        let (a, b) = (self.path[self.next - 1], self.path[self.next]);
        let foot = a.lerp(b, project(pos, a, b).clamp(0.0, 1.0));
        let off = pos.distance(foot);
        let remaining = foot.distance(b) + self.path[self.next..].windows(2).map(|s| s[0].distance(s[1])).sum::<f32>();
        let mut left = ahead;
        let mut at = foot;
        let mut k = self.next;
        let carrot = loop {
            let len = at.distance(self.path[k]);
            if len >= left { break at.lerp(self.path[k], left / len.max(1e-6)); }
            if k == n - 1 { break self.path[k]; }
            left -= len;
            at = self.path[k];
            k += 1;
        };
        (carrot, remaining, off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 300 m square of four 100 m blocks with 20 m streets between and round them.
    pub(crate) fn blocks() -> StreetNav {
        let blocked = |p: Vec2| {
            let inside = |x: f32| (20.0..140.0).contains(&x) || (160.0..280.0).contains(&x);
            inside(p.x) && inside(p.y)
        };
        StreetNav::build(Vec2::ZERO, Vec2::splat(300.0), NAV_CELL_M, |p| !blocked(p))
    }

    #[test]
    fn a_route_follows_the_streets_round_the_blocks() {
        let nav = blocks();
        let path = nav.path(Vec2::new(10.0, 10.0), Vec2::new(290.0, 290.0)).unwrap();
        assert_eq!(path[0], Vec2::new(10.0, 10.0));
        assert_eq!(*path.last().unwrap(), Vec2::new(290.0, 290.0));
        for s in path.windows(2) {
            assert!(nav.segment_clear(s[0], s[1]) || s[0].distance(s[1]) < 6.0, "{:?} crosses a block", s);
        }
        let length: f32 = path.windows(2).map(|s| s[0].distance(s[1])).sum();
        assert!(length < 290.0 * 2.0 + 10.0 && length > 395.0, "{length}");
        // The middle cross street is the short way to the far corner of the first block.
        let path = nav.path(Vec2::new(10.0, 150.0), Vec2::new(290.0, 150.0)).unwrap();
        assert_eq!(path.len(), 2, "straight down the cross street: {path:?}");
    }

    #[test]
    fn pure_pursuit_walks_the_route_and_counts_what_is_left() {
        let nav = blocks();
        let mut leg = Leg::plan(&nav, Vec2::new(150.0, 10.0), Vec2::new(150.0, 290.0)).unwrap();
        let (carrot, remaining, off) = leg.track(Vec2::new(150.0, 10.0), CARROT_M);
        assert!((remaining - 280.0).abs() < 1.0 && off < 1e-3, "{remaining} {off}");
        assert!((carrot - Vec2::new(150.0, 20.0)).length() < 1e-3);
        let (_, remaining, off) = leg.track(Vec2::new(153.0, 200.0), CARROT_M);
        assert!((remaining - 90.0).abs() < 1.0 && (off - 3.0).abs() < 1e-3, "{remaining} {off}");
        assert!(nav.path(Vec2::new(80.0, 80.0), Vec2::new(150.0, 150.0)).is_none(), "a block's heart is off the map");
        assert_eq!(nav.nearest_clear(Vec2::new(30.0, 80.0), 16.0).map(|p| p.x < 20.0), Some(true));
    }
}
