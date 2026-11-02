//! Exact mutable physical terrain owner. Source grids are fingerprinted, not copied.
//! Import validates a detached record before publishing under the event writer lock.
use super::*;
use anyhow::{ensure, Result};
use crate::expedition_world_save::OwnerBlock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const OWNER_BYTES: usize = 128 * 1024 * 1024;
// The byte codec remains authoritative. The smallest encoded page is its coordinate pair,
// variant tag and vector length (8+4+8 bytes); do not impose a 512-page travel cutoff.
const MIN_PAGE_BYTES: usize = 8 + 4 + 8;
const MAX_PAGES: usize = OWNER_BYTES / MIN_PAGE_BYTES;
const MAX_EVENTS: usize = 1_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    res: usize,
    spacing: f32,
    half: f32,
    lattice: f32,
    lattice_res: usize,
    wind: Vec2,
    source: String,
    base_hash: u64,
    crater_revision: u64,
    rut_revision: u64,
    ruts: Vec<((i32, i32), RutPage)>,
    dirty: Vec<(i32, i32)>,
    craters: Vec<(Vec2, f32, f32)>,
    crater_grid: Vec<((i32, i32), Vec<u32>)>,
    walkers: Vec<(u64, Vec2, bool)>,
    footprints: Vec<([u32; 2], [(Vec2, Vec2); 2])>,
    openings: (Vec<[f32; 4]>, bool),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SnapshotV1 {
    res: usize,
    spacing: f32,
    half: f32,
    lattice: f32,
    lattice_res: usize,
    wind: Vec2,
    source: String,
    base_hash: u64,
    crater_revision: u64,
    rut_revision: u64,
    ruts: Vec<((i32, i32), Vec<f32>)>,
    dirty: Vec<(i32, i32)>,
    craters: Vec<(Vec2, f32, f32)>,
    crater_grid: Vec<((i32, i32), Vec<u32>)>,
    walkers: Vec<(u64, Vec2, bool)>,
    footprints: Vec<([u32; 2], [(Vec2, Vec2); 2])>,
    openings: (Vec<[f32; 4]>, bool),
}
/// Float bits are authoritative: positive zero can be omitted; negative zero cannot.
/// u16 addresses every cell of the existing 256x256 page. No quantization/compression loss.
#[derive(Clone, Debug, Serialize, Deserialize)]
enum RutPage { Dense(Vec<u32>), Sparse(Vec<(u16,u32)>) }
impl RutPage {
    fn capture(values:&[f32])->Self {
        let nonzero=values.iter().filter(|v|v.to_bits()!=0).count();
        if nonzero*6 < values.len()*4 { Self::Sparse(values.iter().enumerate().filter_map(|(i,v)|(v.to_bits()!=0).then_some((i as u16,v.to_bits()))).collect()) }
        else { Self::Dense(values.iter().map(|v|v.to_bits()).collect()) }
    }
    fn validate(&self)->Result<()> {
        match self {
            Self::Dense(bits)=>ensure!(bits.len()==RUT_PAGE*RUT_PAGE&&bits.iter().all(|b|f32::from_bits(*b).is_finite()),"invalid dense physical rut page"),
            Self::Sparse(cells)=>ensure!(cells.len()<=RUT_PAGE*RUT_PAGE&&sorted_unique(cells.iter().map(|p|p.0))
                &&cells.iter().all(|(_,b)|*b!=0&&f32::from_bits(*b).is_finite()),"invalid sparse physical rut page"),
        }Ok(())
    }
    fn restore(self)->Box<[f32]> {
        match self {
            Self::Dense(bits)=>bits.into_iter().map(f32::from_bits).collect(),
            Self::Sparse(cells)=>{let mut values=vec![0.;RUT_PAGE*RUT_PAGE];for(index,bits)in cells {values[index as usize]=f32::from_bits(bits);}values.into_boxed_slice()}
        }
    }
}
impl From<SnapshotV1> for Snapshot {
    fn from(old:SnapshotV1)->Self {Self{res:old.res,spacing:old.spacing,half:old.half,lattice:old.lattice,lattice_res:old.lattice_res,wind:old.wind,source:old.source,base_hash:old.base_hash,
        crater_revision:old.crater_revision,rut_revision:old.rut_revision,ruts:old.ruts.into_iter().map(|(k,v)|(k,RutPage::Dense(v.into_iter().map(f32::to_bits).collect()))).collect(),
        dirty:old.dirty,craters:old.craters,crater_grid:old.crater_grid,walkers:old.walkers,footprints:old.footprints,openings:old.openings}}
}

fn source_hash(d: &Detail) -> u64 {
    let mut hash=0xcbf29ce484222325u64;
    for byte in d.heights.iter().flat_map(|h|h.to_bits().to_le_bytes()).chain(d.surface.iter().flatten().copied()) {
        hash=(hash^u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    hash
}
fn sorted_unique<T:Ord>(keys:impl IntoIterator<Item=T>) -> bool {
    let mut previous=None;
    for key in keys {if previous.as_ref().is_some_and(|p|p>=&key){return false;}previous=Some(key);}
    true
}
impl Snapshot {
    fn validate(&self,d:&Detail)->Result<()> {
        ensure!(self.res==d.res&&self.spacing.to_bits()==d.spacing.to_bits()&&self.half.to_bits()==d.half.to_bits()
            &&self.lattice.to_bits()==d.lattice.to_bits()&&self.lattice_res==d.lattice_res
            &&self.wind.to_array().map(f32::to_bits)==d.wind.to_array().map(f32::to_bits)
            &&self.source==d.source&&self.base_hash==source_hash(d),"physical terrain source/layout differs from saved detail");
        ensure!(self.ruts.len()<=MAX_PAGES&&self.craters.len()<=MAX_EVENTS&&self.walkers.len()<=MAX_EVENTS
            &&self.crater_grid.len()<=MAX_EVENTS&&self.openings.0.len()<=MAX_EVENTS&&self.footprints.len()<=4096,"physical terrain event capacity exceeded");
        ensure!(sorted_unique(self.ruts.iter().map(|p|p.0))&&sorted_unique(self.dirty.iter().copied())
            &&sorted_unique(self.crater_grid.iter().map(|p|p.0))&&sorted_unique(self.walkers.iter().map(|p|p.0))
            &&sorted_unique(self.footprints.iter().map(|p|p.0)),"duplicate or unordered physical terrain records");
        for (_,page) in &self.ruts {page.validate()?;}
        ensure!(self.dirty.iter().all(|key|self.ruts.binary_search_by_key(key,|p|p.0).is_ok()),"dirty physical rut page is absent");
        ensure!(self.walkers.iter().all(|(_,p,_)|p.is_finite()),"invalid physical footprint cadence");
        ensure!(self.openings.0.iter().all(|r|r.iter().all(|v|v.is_finite())&&r[0]<=r[2]&&r[1]<=r[3]),"invalid physical terrain opening");
        ensure!(self.footprints.iter().all(|(key,bands)|key.iter().all(|&v|f32::from_bits(v).is_finite()&&f32::from_bits(v)>0.)
            &&bands.iter().all(|(lo,hi)|lo.is_finite()&&hi.is_finite()&&lo.cmple(*hi).all())),"invalid source tread footprint");
        let mut expected=BTreeMap::<(i32,i32),Vec<u32>>::new();let mut references=0u64;
        for (i,&(center,radius,depth)) in self.craters.iter().enumerate() {
            ensure!(center.is_finite()&&radius.is_finite()&&radius>0.&&depth.is_finite()&&depth>=0.,"invalid physical crater");
            let reach=radius*CRATER_REACH_SHARE;
            let lo=((center-reach+d.half)/CRATER_CELL_M).floor();let hi=((center+reach+d.half)/CRATER_CELL_M).floor();
            ensure!(lo.is_finite()&&hi.is_finite(),"physical crater grid overflow");
            let (x0,z0,x1,z1)=(lo.x as i32,lo.y as i32,hi.x as i32,hi.y as i32);
            let width=i64::from(x1)-i64::from(x0)+1;let height=i64::from(z1)-i64::from(z0)+1;
            ensure!(width>0&&height>0&&width<=MAX_EVENTS as i64&&height<=MAX_EVENTS as i64,"physical crater lookup extent invalid");
            references=references.saturating_add(width as u64*height as u64);ensure!(references<=MAX_EVENTS as u64,"physical crater lookup capacity exceeded");
            for z in z0..=z1 {for x in x0..=x1 {expected.entry((x,z)).or_default().push(i as u32);}}
        }
        ensure!(self.crater_grid==expected.into_iter().collect::<Vec<_>>(),"physical crater lookup does not match retained crater order");
        Ok(())
    }
}
impl Detail {
    pub(crate) fn export_world_state(&self)->Result<OwnerBlock> {
        let events=self.events.read().unwrap();
        let mut snapshot=Snapshot{res:self.res,spacing:self.spacing,half:self.half,lattice:self.lattice,lattice_res:self.lattice_res,
            wind:self.wind,source:self.source.clone(),base_hash:source_hash(self),crater_revision:self.crater_revision(),rut_revision:self.rut_revision(),
            ruts:events.ruts.iter().map(|(k,v)|(*k,RutPage::capture(v))).collect(),dirty:events.dirty.iter().copied().collect(),
            craters:events.craters.iter().map(|c|(c.center,c.radius,c.depth)).collect(),crater_grid:events.crater_grid.iter().map(|(k,v)|(*k,v.clone())).collect(),
            walkers:events.walkers.iter().map(|(id,(p,left))|(*id,*p,*left)).collect(),
            footprints:self.footprints.read().unwrap().iter().map(|(k,v)|(*k,v.bands)).collect(),openings:self.openings.read().unwrap().clone()};
        drop(events);
        snapshot.ruts.sort_unstable_by_key(|p|p.0);snapshot.dirty.sort_unstable();snapshot.crater_grid.sort_unstable_by_key(|p|p.0);
        snapshot.walkers.sort_unstable_by_key(|p|p.0);snapshot.footprints.sort_unstable_by_key(|p|p.0);
        snapshot.validate(self)?;OwnerBlock::encode("terrain.detail",2,&snapshot)
    }
    pub(crate) fn import_world_state(&self,block:&OwnerBlock)->Result<()> {
        let snapshot:Snapshot=if block.version==1 {Snapshot::from(block.decode::<SnapshotV1>("terrain.detail",1)?)}else{block.decode("terrain.detail",2)?};snapshot.validate(self)?;
        let events=Events{ruts:snapshot.ruts.into_iter().map(|(k,v)|(k,v.restore())).collect(),dirty:snapshot.dirty.into_iter().collect(),
            craters:snapshot.craters.into_iter().map(|(center,radius,depth)|Crater{center,radius,depth}).collect(),
            crater_grid:snapshot.crater_grid.into_iter().collect(),walkers:snapshot.walkers.into_iter().map(|(id,p,left)|(id,(p,left))).collect()};
        let footprints=snapshot.footprints.into_iter().map(|(k,bands)|(k,crate::renderer::track_ruts::Footprint{bands})).collect();
        // Keep the writer throughout publication and cache clearing. Bounds and node readers
        // cannot observe restored events paired with previous revision/cached values.
        let mut target=self.events.write().unwrap();let mut target_footprints=self.footprints.write().unwrap();let mut target_openings=self.openings.write().unwrap();
        *target=events;*target_footprints=footprints;*target_openings=snapshot.openings;
        self.crater_revision.store(snapshot.crater_revision,Ordering::Relaxed);self.rut_revision.store(snapshot.rut_revision,Ordering::Relaxed);
        for slot in self.cache.slots.iter(){slot.store(u64::MAX,Ordering::Relaxed);}
        self.bounds_cache.lock().unwrap().invalidate();Ok(())
    }
}
#[cfg(test)]mod tests {
    use super::*;
    fn detail()->Detail{Detail::new(65,1.,vec![0.;65*65],vec![[SNOW,40,255,128];65*65],Vec2::X,"terrain state fixture".into()).unwrap()}
    fn setup(d:&Detail){
        d.crater(Vec3::new(1.,0.,2.),0.698);d.crater(Vec3::new(-4.,0.,-2.),2.);
        let key=crate::tank::TIGER_PARTS.hull_half_extents.to_array().map(f32::to_bits);
        d.footprints.write().unwrap().insert(key,crate::renderer::track_ruts::Footprint{bands:[(Vec2::new(-1.7,-2.),Vec2::new(-1.1,2.)),(Vec2::new(1.1,-2.),Vec2::new(1.7,2.))]});
        d.stamp_tracks(Mat4::IDENTITY,crate::tank::TIGER_PARTS);d.stamp_walker(17,Vec3::new(0.,0.,8.));d.stamp_walker(17,Vec3::new(0.8,0.,8.));
        d.set_openings(vec![[-2.,-2.,-1.,-1.]]);
    }
    #[test]fn exact_cold_terrain_restore_preserves_next_steps_berms_and_dirty_uploads(){
        let live=detail();setup(&live);let saved=live.export_world_state().unwrap();
        let cold=detail();cold.crater(Vec3::ZERO,10.);let _=cold.height_upper_bound(Vec2::splat(-1.),Vec2::ONE);
        cold.import_world_state(&saved).unwrap();assert_eq!(saved.payload,cold.export_world_state().unwrap().payload);
        for i in 0..50 {let p=Vec2::new(i as f32*0.113-2.,i as f32*0.073-1.);assert_eq!(live.height_at(p.x,p.y).to_bits(),cold.height_at(p.x,p.y).to_bits());}
        assert_eq!(live.height_upper_bound(Vec2::splat(-3.),Vec2::splat(3.)).to_bits(),cold.height_upper_bound(Vec2::splat(-3.),Vec2::splat(3.)).to_bits());
        assert!(live.events.read().unwrap().ruts.values().any(|p|p.iter().any(|v|*v<0.)),"actual track berms must survive as signed samples");
        assert_eq!(live.take_dirty_rut_pages(),cold.take_dirty_rut_pages());assert_eq!(live.take_openings(),cold.take_openings());
        for x in [1.0,1.6,2.4]{live.stamp_walker(17,Vec3::new(x,0.,8.));cold.stamp_walker(17,Vec3::new(x,0.,8.));}
        live.stamp_tracks(Mat4::from_translation(Vec3::Z*0.4),crate::tank::TIGER_PARTS);cold.stamp_tracks(Mat4::from_translation(Vec3::Z*0.4),crate::tank::TIGER_PARTS);
        assert_eq!(live.export_world_state().unwrap().payload,cold.export_world_state().unwrap().payload,"next foot side/cadence and tracks continue exactly");
        assert_eq!(live.take_dirty_rut_pages(),cold.take_dirty_rut_pages());
    }
    #[test]fn invalid_terrain_import_never_partially_changes_live_events(){
        let d=detail();setup(&d);let before=d.export_world_state().unwrap();let snapshot:Snapshot=before.decode("terrain.detail",2).unwrap();
        let mut bad=snapshot.clone();bad.ruts[0].1=RutPage::Sparse(vec![(0,f32::NAN.to_bits())]);
        assert!(d.import_world_state(&OwnerBlock::encode("terrain.detail",2,&bad).unwrap()).is_err());
        let mut bad=snapshot.clone();bad.crater_grid[0].1.reverse();bad.crater_grid[0].1.push(u32::MAX);
        assert!(d.import_world_state(&OwnerBlock::encode("terrain.detail",2,&bad).unwrap()).is_err());
        let mut bad=snapshot.clone();bad.walkers.push(bad.walkers[0]);
        assert!(d.import_world_state(&OwnerBlock::encode("terrain.detail",2,&bad).unwrap()).is_err());
        let mut bad=snapshot;bad.base_hash^=1;
        assert!(d.import_world_state(&OwnerBlock::encode("terrain.detail",2,&bad).unwrap()).is_err());
        assert_eq!(before.payload,d.export_world_state().unwrap().payload);
    }
    #[test]fn restoring_empty_events_clears_existing_craters_ruts_and_both_caches(){
        let empty=detail().export_world_state().unwrap();let d=detail();setup(&d);d.crater_revision.store(0,Ordering::Relaxed);d.rut_revision.store(0,Ordering::Relaxed);let old=d.height_at(1.,2.);let _=d.height_upper_bound(Vec2::ZERO,Vec2::ONE);
        d.import_world_state(&empty).unwrap();assert_eq!(d.rut_page_count(),0);assert!(d.craters().is_empty());
        assert_ne!(old.to_bits(),d.height_at(1.,2.).to_bits());assert_eq!(empty.payload,d.export_world_state().unwrap().payload);
    }
    #[test]fn sparse_and_dense_rut_pages_preserve_exact_bits_and_choose_smaller_payload(){
        let mut sparse=vec![0f32;RUT_PAGE*RUT_PAGE];sparse[0]=-0.;sparse[17]=-0.1125;sparse[65535]=0.30;
        let page=RutPage::capture(&sparse);assert!(matches!(page,RutPage::Sparse(_)));page.validate().unwrap();
        let encoded=OwnerBlock::encode("rut.page",1,&page).unwrap();assert!(encoded.payload.len()<64,"sparse page must not carry 256KiB of zeroes");
        let restored:RutPage=encoded.decode("rut.page",1).unwrap();assert_eq!(restored.restore().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),sparse.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
        let dense=(0..RUT_PAGE*RUT_PAGE).map(|i|((i%47)as f32+1.)*-0.001).collect::<Vec<_>>();let page=RutPage::capture(&dense);assert!(matches!(page,RutPage::Dense(_)));page.validate().unwrap();
        let encoded=OwnerBlock::encode("rut.page",1,&page).unwrap();assert_eq!(encoded.payload.len(),12+RUT_PAGE*RUT_PAGE*4);
        let restored:RutPage=encoded.decode("rut.page",1).unwrap();assert_eq!(restored.restore().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),dense.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
        assert!(RutPage::Sparse(vec![(3,1f32.to_bits()),(3,2f32.to_bits())]).validate().is_err());
        assert!(RutPage::Sparse(vec![(0,0)]).validate().is_err());assert!(RutPage::Sparse(vec![(0,f32::NAN.to_bits())]).validate().is_err());
    }
    #[test]fn mixed_page_world_roundtrip_and_dense_v1_migration_are_lossless(){
        let d=detail();setup(&d);{let mut events=d.events.write().unwrap();events.ruts.insert((0,0),vec![-0.01;RUT_PAGE*RUT_PAGE].into_boxed_slice());events.dirty.insert((0,0));d.advance_rut_revision();}
        let saved=d.export_world_state().unwrap();let snapshot:Snapshot=saved.decode("terrain.detail",2).unwrap();
        assert!(snapshot.ruts.iter().any(|(_,p)|matches!(p,RutPage::Sparse(_))));assert!(snapshot.ruts.iter().any(|(_,p)|matches!(p,RutPage::Dense(_))));
        let original_dense_bytes=snapshot.ruts.len()*RUT_PAGE*RUT_PAGE*4;assert!(saved.payload.len()<original_dense_bytes);
        let old=SnapshotV1{res:snapshot.res,spacing:snapshot.spacing,half:snapshot.half,lattice:snapshot.lattice,lattice_res:snapshot.lattice_res,wind:snapshot.wind,source:snapshot.source,base_hash:snapshot.base_hash,
            crater_revision:snapshot.crater_revision,rut_revision:snapshot.rut_revision,ruts:snapshot.ruts.into_iter().map(|(k,p)|(k,p.restore().into_vec())).collect(),dirty:snapshot.dirty,craters:snapshot.craters,
            crater_grid:snapshot.crater_grid,walkers:snapshot.walkers,footprints:snapshot.footprints,openings:snapshot.openings};
        let cold=detail();cold.import_world_state(&OwnerBlock::encode("terrain.detail",1,&old).unwrap()).unwrap();assert_eq!(saved.payload,cold.export_world_state().unwrap().payload);
        assert!(MAX_PAGES>512);
    }

    #[test]fn sparse_page_count_is_limited_by_encoded_budget_not_old_512_page_cutoff(){
        let d=detail();let mut snapshot:Snapshot=d.export_world_state().unwrap().decode("terrain.detail",2).unwrap();
        snapshot.ruts=(0..513).map(|i|((i,0),RutPage::Sparse(vec![(0,0.1f32.to_bits())]))).collect();
        snapshot.validate(&d).unwrap();let encoded=OwnerBlock::encode("terrain.detail",2,&snapshot).unwrap();
        assert!(encoded.payload.len()<32*1024);assert!(snapshot.ruts.len()>512);
    }

}
