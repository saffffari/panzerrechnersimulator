//! Conservative broad phase for short projectile sweeps through dense authored scenery.
//! Bounds are static for a Game; destructible alive state is checked by the caller at contact.
use super::*;
use std::collections::HashMap;
const CELL:f32=32.0;
const MAX_CELLS:i64=64;
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Key { Story(usize), Structure(usize) }
#[derive(Default)]
pub(super) struct Index {
    counts:(usize,usize),
    cells:HashMap<(i32,i32),Vec<Key>>,
    broad:Vec<Key>,
}
fn cell(p:Vec3)->(i32,i32) { ((p.x/CELL).floor() as i32,(p.z/CELL).floor() as i32) }
fn cell_count(a:(i32,i32),b:(i32,i32))->i64 {
    (b.0 as i64-a.0 as i64+1).saturating_mul(b.1 as i64-a.1 as i64+1)
}
impl Index {
    pub fn matches(&self, story:usize, structures:usize)->bool { self.counts==(story,structures) }
    pub fn build(story:&[(Vec3,Vec3)], structures:&[Destructible])->Self {
        let mut index=Self {counts:(story.len(),structures.len()),..Self::default()};
        for (i,&(lo,hi)) in story.iter().enumerate() { index.insert(Key::Story(i),lo,hi); }
        for (i,body) in structures.iter().enumerate() { index.insert(Key::Structure(i),body.min,body.max); }
        index
    }
    fn insert(&mut self,key:Key,lo:Vec3,hi:Vec3) {
        let (a,b)=(cell(lo),cell(hi));
        let count=cell_count(a,b);
        if !lo.is_finite() || !hi.is_finite() || count<=0 || count>MAX_CELLS {
            self.broad.push(key);return;
        }
        for x in a.0..=b.0 { for z in a.1..=b.1 { self.cells.entry((x,z)).or_default().push(key); } }
    }
    /// No per-step allocation or fixed candidate cap. Oversized queries conservatively fall back
    /// to every bound. A boundary-crossing sweep may visit a key more than once, which is harmless.
    pub fn visit(&self,start:Vec3,end:Vec3,mut visit:impl FnMut(Key)) {
        let (a,b)=(cell(start.min(end)-Vec3::splat(0.001)),cell(start.max(end)+Vec3::splat(0.001)));
        if !start.is_finite() || !end.is_finite() || cell_count(a,b)>MAX_CELLS {
            for i in 0..self.counts.0 { visit(Key::Story(i)); }
            for i in 0..self.counts.1 { visit(Key::Structure(i)); }
            return;
        }
        // Preserve cover-before-structure behavior where the two envelopes meet at the same t.
        for structure in [false,true] {
            let mut emit=|key| { if matches!(key,Key::Structure(_))==structure { visit(key); } };
            for &key in &self.broad { emit(key); }
            for x in a.0..=b.0 { for z in a.1..=b.1 {
                if let Some(keys)=self.cells.get(&(x,z)) { for &key in keys { emit(key); } }
            } }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dense_cover_queries_preserve_all_hits_across_cells_and_broad_bounds() {
        let mut boxes=Vec::new();
        for x in -50..50 { for z in -50..50 {
            let p=Vec3::new(x as f32*8.0,0.0,z as f32*8.0);
            boxes.push((p,p+Vec3::new(0.01,3.0,4.0)));
        } }
        boxes.push((Vec3::new(-500.0,0.0,-0.01),Vec3::new(500.0,3.0,0.01)));
        let index=Index::build(&boxes,&[]);
        let mut max_candidates=0;
        for n in 0..240 {
            let x=(n%31) as f32*8.0-120.0;
            let z=(n%29) as f32*8.0-112.0;
            let start=Vec3::new(x-2.0,1.0,z+0.01);
            let end=start+Vec3::new(4.0,0.0,0.0);
            let exact:std::collections::BTreeSet<_>=boxes.iter().enumerate().filter_map(|(i,(lo,hi))|
                segment_local_aabb_contact(start,end,*lo,*hi).map(|_|Key::Story(i))).collect();
            let mut queried=std::collections::BTreeSet::new();
            index.visit(start,end,|key| {queried.insert(key);});
            assert!(exact.is_subset(&queried),"broad phase lost a real thin-wall contact");
            max_candidates=max_candidates.max(queried.len());
        }
        assert!(max_candidates<100,"short sweeps must not scan ten thousand distant props");
        let mut broad=std::collections::BTreeSet::new();
        index.visit(Vec3::splat(-600.0),Vec3::splat(600.0),|key|{broad.insert(key);});
        assert_eq!(broad.len(),boxes.len());
    }
}
