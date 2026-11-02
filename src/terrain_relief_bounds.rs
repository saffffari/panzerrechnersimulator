//! Exact lattice-node maximum, including current craters and ruts. Cache only exact nodes;
//! the queried inclusive range and final floating-point padding remain unchanged.
use super::*;
use std::collections::VecDeque;
const TILE: u32 = 8;
const TILE_NODES: usize = (TILE * TILE) as usize;
const MAX_TILES: usize = 4096;

struct Tile {
    values: [f32; TILE_NODES],
    valid: u64,
    high: f32,
    magnitude: f32,
}
impl Default for Tile {
    fn default() -> Self {
        Self { values: [0.; TILE_NODES], valid: 0, high: f32::NEG_INFINITY, magnitude: 1. }
    }
}
#[derive(Default)]
pub(super) struct Cache {
    revision: Option<[u64; 2]>,
    tiles: HashMap<(u32, u32), Tile>,
    order: VecDeque<(u32, u32)>,
    #[cfg(test)] computed_nodes: u64,
}
impl Cache {
    pub(super) fn invalidate(&mut self) { self.tiles.clear(); self.order.clear(); self.revision = None; }
    fn generation(&mut self, revision: [u64; 2]) {
        if self.revision != Some(revision) {
            self.tiles.clear(); self.order.clear(); self.revision = Some(revision);
        }
    }
    fn range(&mut self, key: (u32, u32), lo: (u32, u32), hi: (u32, u32), mut node: impl FnMut(u32, u32) -> f32) -> (f32, f32) {
        if !self.tiles.contains_key(&key) {
            if self.tiles.len() == MAX_TILES {
                if let Some(old) = self.order.pop_front() { self.tiles.remove(&old); }
            }
            self.tiles.insert(key, Tile::default()); self.order.push_back(key);
        }
        let tile = self.tiles.get_mut(&key).unwrap();
        let whole = lo == (0, 0) && hi == (TILE - 1, TILE - 1);
        if whole && tile.valid == u64::MAX { return (tile.high, tile.magnitude); }
        let mut high = f32::NEG_INFINITY; let mut magnitude = 1.0f32;
        for z in lo.1..=hi.1 { for x in lo.0..=hi.0 {
            let index = (z * TILE + x) as usize; let bit = 1u64 << index;
            if tile.valid & bit == 0 {
                let h = node(key.0 * TILE + x, key.1 * TILE + z);
                tile.values[index] = h; tile.valid |= bit;
                tile.high = if h.is_finite() { tile.high.max(h) } else { f32::INFINITY };
                tile.magnitude = tile.magnitude.max(h.abs());
                #[cfg(test)] { self.computed_nodes += 1; }
            }
            let h = tile.values[index];
            if !h.is_finite() { return (f32::INFINITY, 1.); }
            high = high.max(h); magnitude = magnitude.max(h.abs());
        }}
        (high, magnitude)
    }
}
impl Detail {
    pub(crate) fn height_upper_bound(&self, min: Vec2, max: Vec2) -> f32 {
        if !min.is_finite() || !max.is_finite() || min.cmpgt(max).any() { return f32::INFINITY; }
        let last = (self.lattice_res - 2) as f32;
        let lo = ((min + Vec2::splat(self.half)) / self.lattice).floor().clamp(Vec2::ZERO, Vec2::splat(last));
        let hi = ((max + Vec2::splat(self.half)) / self.lattice).floor().clamp(Vec2::ZERO, Vec2::splat(last));
        let (x0, z0, x1, z1) = (lo.x as u32, lo.y as u32, hi.x as u32 + 1, hi.y as u32 + 1);
        // One reader guard covers every node and its full revision pair. All production
        // crater/rut mutations publish revisions while holding this same writer lock.
        let events = self.events.read().unwrap();
        let revision = [self.crater_revision(), self.rut_revision()];
        let mut cache = self.bounds_cache.lock().unwrap(); cache.generation(revision);
        let mut high = f32::NEG_INFINITY; let mut magnitude = 1.0f32;
        for tz in z0 / TILE..=z1 / TILE { for tx in x0 / TILE..=x1 / TILE {
            let lo = (x0.saturating_sub(tx * TILE), z0.saturating_sub(tz * TILE));
            let hi = ((x1 - tx * TILE).min(TILE - 1), (z1 - tz * TILE).min(TILE - 1));
            let (h, m) = cache.range((tx, tz), lo, hi, |i, j| self.lattice_node_height_locked(i, j, &events, revision[0]));
            if !h.is_finite() { return f32::INFINITY; }
            high = high.max(h); magnitude = magnitude.max(m);
        }}
        high + 16.0 * f32::EPSILON * magnitude
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(res: usize) -> Detail {
        Detail::new(res, 1., (0..res*res).map(|i| ((i*17%31) as f32 - 15.) * 0.031).collect(),
            vec![[SNOW, 40, 255, 128]; res*res], Vec2::X, "exact bound fixture".into()).unwrap()
    }
    fn brute(d: &Detail, min: Vec2, max: Vec2) -> f32 {
        let last = (d.lattice_res - 2) as f32;
        let lo = ((min + Vec2::splat(d.half)) / d.lattice).floor().clamp(Vec2::ZERO, Vec2::splat(last));
        let hi = ((max + Vec2::splat(d.half)) / d.lattice).floor().clamp(Vec2::ZERO, Vec2::splat(last));
        let mut high = f32::NEG_INFINITY; let mut magnitude = 1.0f32;
        for z in lo.y as u32..=hi.y as u32 + 1 { for x in lo.x as u32..=hi.x as u32 + 1 {
            // Deliberately bypass both node caches for an independent exact oracle.
            let px = x as f32*d.lattice-d.half; let pz = z as f32*d.lattice-d.half;
            let h = d.static_height(px,pz,0,0.) - d.rut_at(px,pz);
            high = high.max(h); magnitude = magnitude.max(h.abs());
        }}
        high + 16.0*f32::EPSILON*magnitude
    }
    fn equal(d: &Detail, min: Vec2, max: Vec2) {
        assert_eq!(d.height_upper_bound(min,max).to_bits(),brute(d,min,max).to_bits(),"bounds {min:?}..{max:?}");
    }
    #[test]
    fn rectangle_detail_bound_contains_relief_crater_and_rut_surface() {
        let d=fixture(4); d.crater(Vec3::ZERO,2.);
        let min=Vec2::new(-0.7,-0.4); let max=Vec2::new(0.8,0.6);
        equal(&d,min,max); let upper=d.height_upper_bound(min,max);
        for j in 0..=50 { for i in 0..=50 { let p=min+(max-min)*Vec2::new(i as f32/50.,j as f32/50.); assert!(d.height_at(p.x,p.y)<=upper); }}
        { let mut e=d.events.write().unwrap(); e.ruts.insert((0,0),vec![-0.2;RUT_PAGE*RUT_PAGE].into_boxed_slice()); d.rut_revision.fetch_add(1,Ordering::Relaxed); }
        equal(&d,min,max);
    }
    #[test]
    fn tile_bounds_match_brute_force_ranges_edges_and_warm_reuse() {
        let d=fixture(17);
        for i in 0..48 { let min=Vec2::new((i*31%151)as f32*0.1-8.,(i*47%139)as f32*0.1-8.); let max=min+Vec2::new((i%7)as f32*0.17,(i%11)as f32*0.13); equal(&d,min,max); }
        for p in [Vec2::splat(-1e6),Vec2::splat(1e6),Vec2::new(-1e6,1e6)] {equal(&d,p,p+Vec2::ONE);}
        let min=Vec2::splat(-1.5);let max=Vec2::splat(1.5); equal(&d,min,max);
        let computed=d.bounds_cache.lock().unwrap().computed_nodes; let before=d.height_upper_bound(min,max);
        assert_eq!(d.height_upper_bound(min,max).to_bits(),before.to_bits());
        assert_eq!(d.bounds_cache.lock().unwrap().computed_nodes,computed,"warm repeated bounds must not resample nodes or lock rut pages per node");
    }
    #[test]
    fn crater_boot_rut_clear_and_cold_reconstruction_invalidate_exact_bounds() {
        let d=fixture(9);let min=Vec2::splat(-0.6);let max=Vec2::splat(0.8);
        equal(&d,min,max);d.crater(Vec3::new(0.1,0.,0.2),0.698);equal(&d,min,max);
        d.stamp_walker(8,Vec3::new(-0.8,0.,0.));d.stamp_walker(8,Vec3::new(0.1,0.,0.));assert!(d.rut_revision()>0);equal(&d,min,max);
        // Detail currently has no production event import/clear API. This fixture reset follows
        // the writer+revision contract; cold reconstruction uses a fresh empty-cache Detail.
        {let mut events=d.events.write().unwrap();events.ruts.clear();d.rut_revision.fetch_add(1,Ordering::Relaxed);}
        equal(&d,min,max);
        let cold=fixture(9); cold.crater(Vec3::new(0.1,0.,0.2),0.698);
        assert_eq!(d.height_upper_bound(min,max).to_bits(),cold.height_upper_bound(min,max).to_bits());
    }
    #[test]
    fn compact_static_revision_wrap_cannot_resurrect_old_heights() {
        for before_wrap in [0x3fffu64,u64::MAX] {
            let d=fixture(9);let min=Vec2::splat(-0.1);let max=Vec2::splat(0.1);equal(&d,min,max);
            d.crater_revision.store(before_wrap,Ordering::Relaxed);d.crater(Vec3::ZERO,2.0);
            assert_eq!(d.crater_revision(),before_wrap.wrapping_add(1));equal(&d,min,max);
        }
    }
    #[test]
    fn rut_revision_wrap_and_deep_negative_magnitude_keep_exact_padding() {
        let d=fixture(9); let min=Vec2::splat(-0.2);let max=Vec2::splat(0.2);
        equal(&d,min,max); d.rut_revision.store(u64::MAX,Ordering::Relaxed);
        d.stamp_walker(9,Vec3::new(-0.8,0.,0.));d.stamp_walker(9,Vec3::new(0.1,0.,0.));
        assert_eq!(d.rut_revision(),0);equal(&d,min,max);
        d.crater(Vec3::ZERO,200.);assert!(d.height_at(0.,0.)< -1.);equal(&d,min,max);
    }
    #[test]
    fn lazy_tile_capacity_is_bounded_and_eviction_preserves_exact_values() {
        let mut c=Cache::default();c.generation([0,0]);
        for i in 0..MAX_TILES as u32+17 {let (h,_)=c.range((i,0),(0,0),(0,0),|x,z|(x+z)as f32);assert_eq!(h,(i*TILE)as f32);assert!(c.tiles.len()<=MAX_TILES);}
        assert_eq!(c.range((0,0),(0,0),(0,0),|x,z|(x+z)as f32).0,0.);
        c.generation([1,1]);assert!(c.tiles.is_empty());assert!(c.order.is_empty());
    }
}
