//! Lossless owner-block envelope for complete live expedition snapshots. This is a NEW format;
//! coordinator-only expedition.v2.json is never accepted as a suspended battlefield.
//! The Game adapter must supply every required owner, including explicit None for absent modes.
//! Immutable source/GPU caches are rebuilt only after source and build fingerprints match.
use anyhow::{ensure,Context,Result};
use bincode::Options;
use serde::{de::DeserializeOwned,Deserialize,Serialize};
use std::{collections::BTreeSet,fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf},sync::atomic::{AtomicU64,Ordering}};

/// Typed field packets let an owner retain its private fields without making renderer caches
/// serializable or maintaining a second simulation model. Unknown/missing fields are errors.
#[derive(Clone,Default,Serialize,Deserialize)]
pub struct StateFields { fields:std::collections::BTreeMap<String,Vec<u8>> }
impl StateFields {
    pub fn put<T:Serialize>(&mut self,key:&str,value:&T)->Result<()> {
        ensure!(!key.is_empty()&&key.len()<=96,"invalid state field key");
        ensure!(!self.fields.contains_key(key),"duplicate state field {key}");
        let bytes=codec(MAX_OWNER).serialize(value)
            .with_context(||format!("encode world state field {key}"))?;
        self.fields.insert(key.into(),bytes);Ok(())
    }
    pub fn take<T:DeserializeOwned>(&mut self,key:&str)->Result<T> {
        let bytes=self.fields.remove(key).with_context(||format!("required state field {key} missing"))?;
        codec(MAX_OWNER).deserialize(&bytes).with_context(||format!("decode state field {key}"))
    }
    pub fn finish(self)->Result<()> {ensure!(self.fields.is_empty(),"unconsumed state fields: {:?}",self.fields.keys());Ok(())}
    pub fn keys(&self)->impl Iterator<Item=&str> {self.fields.keys().map(String::as_str)}
    /// The encoded fields by key, and back (the FEA range's in-memory checkpoints keep them and
    /// share a field unchanged since the previous checkpoint).
    pub fn into_raw(self)->std::collections::BTreeMap<String,Vec<u8>> {self.fields}
    pub fn from_raw(fields:std::collections::BTreeMap<String,Vec<u8>>)->Self {Self{fields}}
    pub fn fingerprints(&self)->std::collections::BTreeMap<String,(usize,u64)> {
        self.fields.iter().map(|(key,bytes)|(key.clone(),(bytes.len(),checksum(bytes)))).collect()
    }
}

/// Owner-local helpers. Import targets must be disposable candidates, never the active Game.
#[macro_export]
macro_rules! capture_world_fields {
    ($owner:expr; $($field:ident),* $(,)?) => {{
        let mut fields=$crate::expedition_world_save::StateFields::default();
        $(fields.put(stringify!($field), &$owner.$field)?;)* fields
    }};
}
#[macro_export]
macro_rules! restore_world_fields {
    ($owner:expr,$fields:expr; $($field:ident),* $(,)?) => {{
        let mut fields=$fields;
        $($owner.$field=fields.take(stringify!($field))?;)* fields.finish()?
    }};
}

const MAGIC:[u8;8]=*b"PZEXW001";
pub const VERSION:u32=1;
const MAX_FILE:u64=512*1024*1024;
const MAX_OWNER:u64=128*1024*1024;
const MAX_SOURCES:usize=4096;
static SERIAL:AtomicU64=AtomicU64::new(1);

/// Changing an owner's layout requires changing its version. Missing blocks never mean fresh
/// default simulation state. An unsupported inactive feature is represented explicitly as None.
pub const REQUIRED_OWNERS:&[(&str,u32)]=&[
    ("game.physical",3),("vehicles.tanks",2),("crew.runtime",4),
    ("transport.recovery",6),("workshop.runtime",4),("world.progression",3),
    ("world.environment",2),("combat.projectiles",2),("combat.infantry",1),
    ("expedition.runtime",2),("personal.runtime",1),
];

fn codec(limit:u64)->impl Options {
    bincode::DefaultOptions::new().with_fixint_encoding().with_little_endian().with_limit(limit).reject_trailing_bytes()
}
fn hash_into(hash:&mut u64,bytes:&[u8]) {for byte in bytes {*hash=(*hash^u64::from(*byte)).wrapping_mul(0x100000001b3);}}
pub fn checksum(bytes:&[u8])->u64 {let mut hash=0xcbf29ce484222325;hash_into(&mut hash,bytes);hash}

#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct OwnerBlock {pub key:String,pub version:u32,pub payload:Vec<u8>,pub checksum:u64}
impl OwnerBlock {
    pub fn encode<T:Serialize>(key:&str,version:u32,value:&T)->Result<Self> {
        ensure!(!key.is_empty()&&key.len()<=96&&version>0,"invalid owner identity");
        let payload=codec(MAX_OWNER).serialize(value).with_context(||format!("encode world owner {key}"))?;
        let block=Self{key:key.into(),version,checksum:checksum(&payload),payload};block.validate()?;Ok(block)
    }
    pub fn decode<T:DeserializeOwned>(&self,key:&str,version:u32)->Result<T> {
        self.validate()?;ensure!(self.key==key&&self.version==version,"owner layout mismatch for {key}");
        codec(MAX_OWNER).deserialize(&self.payload).with_context(||format!("decode world owner {key}"))
    }
    fn validate(&self)->Result<()> {
        ensure!(!self.key.is_empty()&&self.key.len()<=96&&self.version>0,"invalid owner identity");
        ensure!(self.payload.len() as u64<=MAX_OWNER,"world owner {} is too large",self.key);
        ensure!(checksum(&self.payload)==self.checksum,"world owner {} checksum mismatch",self.key);Ok(())
    }
}

/// Only for existing JSON-compatible records (e.g. the already validated coordinator schema or
/// optional audit logs). Bincode cannot decode serde_json::Value or internally tagged enums.
/// Authoritative physical floats belong in typed binary DTOs, not in this compatibility wrapper.
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct JsonState {bytes:Vec<u8>}
impl JsonState {
    pub fn capture<T:Serialize>(value:&T)->Result<Self> {
        let bytes=serde_json::to_vec(value)?;ensure!(bytes.len() as u64<=MAX_OWNER,"JSON compatibility record too large");Ok(Self{bytes})
    }
    pub fn restore<T:DeserializeOwned>(&self)->Result<T> {
        ensure!(self.bytes.len() as u64<=MAX_OWNER,"JSON compatibility record too large");Ok(serde_json::from_slice(&self.bytes)?)
    }
}

/// Compatibility for already versioned, validated JSON coordinator records and audit values.
/// Do not apply to live physical state containing IEEE infinity sentinels.
pub mod json_compat {
    use serde::{de::DeserializeOwned,Deserialize,Deserializer,Serialize,Serializer};
    pub fn serialize<T:Serialize,S:Serializer>(value:&T,s:S)->Result<S::Ok,S::Error> {
        serde_json::to_vec(value).map_err(serde::ser::Error::custom)?.serialize(s)
    }
    pub fn deserialize<'de,T:DeserializeOwned,D:Deserializer<'de>>(d:D)->Result<T,D::Error> {
        let bytes=Vec::<u8>::deserialize(d)?;
        serde_json::from_slice(&bytes).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub struct SourceFingerprint {pub path:String,pub bytes:u64,pub checksum:u64}
impl SourceFingerprint {
    /// The caller supplies a game resource path; no source bytes are copied into the save.
    pub fn inspect(path:&str)->Result<Self> {
        let resolved=crate::resource::resolve(path)?;let mut file=File::open(&resolved)?;
        let metadata=file.metadata()?;ensure!(metadata.is_file(),"save source is not a regular file: {path}");
        let mut hash=0xcbf29ce484222325;let mut actual=0_u64;let mut buffer=[0_u8;64*1024];
        loop {let count=file.read(&mut buffer)?;if count==0 {break;}hash_into(&mut hash,&buffer[..count]);actual+=count as u64;}
        ensure!(actual==metadata.len(),"source changed while fingerprinting: {path}");
        Ok(Self{path:path.into(),bytes:actual,checksum:hash})
    }
}

#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct WorldRecipe {
    pub manifest_path:String,
    pub run_seed:u64,
    pub act_id:u8,
    /// Exact existing support-camp root, not a new search for a nearby free pad on load.
    pub camp_root:[f32;16],
    pub sources:Vec<SourceFingerprint>,
}
impl WorldRecipe {
    pub fn capture(manifest_path:String,run_seed:u64,act_id:u8,camp_root:glam::Mat4)->Result<Self> {
        let manifest_path=manifest_path.replace('\\',"/");
        crate::resource::register_world_source(&manifest_path)?;
        let sources=crate::resource::resolved_world_sources()?.into_iter().map(|s|SourceFingerprint{path:s.path,bytes:s.bytes,checksum:s.checksum}).collect();
        let recipe=Self{manifest_path,run_seed,act_id,camp_root:camp_root.to_cols_array(),sources};recipe.validate()?;Ok(recipe)
    }
    pub fn verify_sources(&self)->Result<()> {
        self.validate()?;
        for source in &self.sources {
            ensure!(SourceFingerprint::inspect(&source.path)?==*source,"world source changed: {}",source.path);
        }Ok(())
    }
    fn validate(&self)->Result<()> {
        ensure!(!self.manifest_path.is_empty()&&self.manifest_path.len()<=4096,"missing world manifest path");
        ensure!(!self.sources.is_empty()&&self.sources.len()<=MAX_SOURCES,"missing or excessive source fingerprints");
        let unique=self.sources.iter().map(|s|&s.path).collect::<BTreeSet<_>>();
        ensure!(unique.len()==self.sources.len(),"duplicate world source fingerprint");
        ensure!(self.sources.iter().any(|s|s.path==self.manifest_path),"manifest fingerprint is missing");
        ensure!(self.sources.iter().all(|s|!s.path.is_empty()&&s.path.len()<=4096),"invalid source path");
        let root=glam::Mat4::from_cols_array(&self.camp_root);
        ensure!(root.is_finite()&&root.determinant().abs()>1e-7,"invalid support camp root");Ok(())
    }
}

/// Paper annotations belong to the window frontend, but are part of the player's retained world.
/// Headless owner proofs have no frontend; a window resume requires this record explicitly.
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct FrontendState {
    pub map_cursor:glam::Vec2,
    pub pencil_marks:Vec<(u8,glam::Vec2)>,
    pub map_pins:Vec<(u8,glam::Vec2)>,
    pub show_map:bool,
    pub relief_selected:usize,
}
impl FrontendState {
    pub fn validate(&self)->Result<()> {
        ensure!(self.map_cursor.is_finite()&&self.pencil_marks.len()<=1_000_000&&self.map_pins.len()<=100_000,"invalid saved map annotations");
        ensure!(self.pencil_marks.iter().chain(&self.map_pins).all(|(_,p)|p.is_finite()),"nonfinite saved map annotation");
        ensure!(self.relief_selected<16,"invalid saved relief selection");Ok(())
    }
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct WorldSave {
    pub version:u32,
    pub build_fingerprint:u64,
    pub world:WorldRecipe,
    pub simulation_seconds:f64,
    pub owners:Vec<OwnerBlock>,
    pub frontend:Option<FrontendState>,
}
impl WorldSave {
    pub fn new(world:WorldRecipe,simulation_seconds:f64,owners:Vec<OwnerBlock>)->Result<Self> {
        let save=Self{version:VERSION,build_fingerprint:current_build_fingerprint(),world,simulation_seconds,owners,frontend:None};
        save.validate()?;Ok(save)
    }
    pub fn owner(&self,key:&str)->Result<&OwnerBlock> {
        self.owners.iter().find(|owner|owner.key==key).with_context(||format!("required world owner {key} is absent"))
    }
    pub fn validate(&self)->Result<()> {
        ensure!(self.version==VERSION,"unsupported live expedition save version");
        ensure!(self.build_fingerprint==current_build_fingerprint(),"save belongs to different simulation code; no reset/migration fallback");
        ensure!(self.simulation_seconds.is_finite()&&self.simulation_seconds>=0.0,"invalid world clock");
        self.world.validate()?;
        if let Some(frontend)=&self.frontend {frontend.validate()?;}
        ensure!(self.owners.len()==REQUIRED_OWNERS.len(),"live world snapshot does not cover every registered owner");
        let keys=self.owners.iter().map(|o|o.key.as_str()).collect::<BTreeSet<_>>();
        ensure!(keys.len()==self.owners.len(),"duplicate live world owner");
        for &(key,version) in REQUIRED_OWNERS {let owner=self.owner(key)?;owner.validate()?;ensure!(owner.version==version||(key=="crew.runtime"&&owner.version==3)||(key=="expedition.runtime"&&owner.version==1)||(key=="transport.recovery"&&owner.version==5)||(key=="world.environment"&&owner.version==1),"unsupported {key} snapshot version");}
        Ok(())
    }
    /// Must precede candidate Game construction/import; no currently running Game is mutated.
    pub fn verify_for_resume(&self)->Result<()> {self.validate()?;self.world.verify_sources()}
}

/// Conservative same-code gate. Owner schema versions are necessary but not sufficient: a
/// change to physics or source placement can reinterpret the same bytes differently.
pub fn current_build_fingerprint()->u64 {
    u64::from_str_radix(env!("KESSEL_SIMULATION_FINGERPRINT"),16)
        .expect("build.rs emitted a validated complete source fingerprint")
}

pub fn encode(save:&WorldSave)->Result<Vec<u8>> {
    save.validate()?;let payload=codec(MAX_FILE).serialize(save)?;
    let mut bytes=Vec::with_capacity(payload.len()+24);bytes.extend(MAGIC);
    bytes.extend((payload.len() as u64).to_le_bytes());bytes.extend(checksum(&payload).to_le_bytes());bytes.extend(payload);
    ensure!(bytes.len() as u64<=MAX_FILE,"world save exceeds file limit");Ok(bytes)
}
pub fn decode(bytes:&[u8])->Result<WorldSave> {
    ensure!(bytes.len()>=24&&bytes.len() as u64<=MAX_FILE,"invalid world save size");
    ensure!(bytes[..8]==MAGIC,"not a complete live expedition save (coordinator checkpoints cannot resume a battlefield)");
    let length=u64::from_le_bytes(bytes[8..16].try_into()?);let hash=u64::from_le_bytes(bytes[16..24].try_into()?);
    ensure!(length==bytes.len() as u64-24,"world save length mismatch");
    ensure!(checksum(&bytes[24..])==hash,"world save checksum mismatch");
    let save:WorldSave=codec(MAX_FILE).deserialize(&bytes[24..])?;save.validate()?;Ok(save)
}
pub fn default_path()->Result<PathBuf> {Ok(crate::expedition_save::default_path()?.with_file_name("expedition-world.v1.bin"))}
pub fn autosave_path()->Result<PathBuf> {Ok(default_path()?.with_file_name("expedition-world-auto.v1.bin"))}
pub fn saved_file_available()->bool { [default_path(),autosave_path()].into_iter().filter_map(Result::ok)
    .any(|path|path.is_file()||backup_path(&path).is_ok_and(|p|p.is_file())) }
/// Most recently written primary/backup first. A bad generation is reported by the caller;
/// files are never deleted because a new build cannot understand them.
pub fn resume_paths()->Result<Vec<PathBuf>> {
    let mut paths=Vec::new();for primary in [default_path()?,autosave_path()?] {
        for path in [primary.clone(),backup_path(&primary)?] {if let Ok(meta)=fs::metadata(&path) {
            if meta.is_file(){paths.push((meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),path));}
        }}
    }
    paths.sort_by(|a,b|b.0.cmp(&a.0));Ok(paths.into_iter().map(|(_,p)|p).collect())
}
pub fn load(path:&Path)->Result<WorldSave> {decode(&read_bounded(path)?)}
pub struct Loaded {pub save:WorldSave,pub recovered_from_backup:bool}
pub fn load_with_recovery(path:&Path)->Result<Loaded> {
    match load(path) {
        Ok(save)=>Ok(Loaded{save,recovered_from_backup:false}),
        Err(primary)=>Ok(Loaded{save:load(&backup_path(path)?).with_context(||format!("primary world save failed ({primary:#}); backup failed"))?,recovered_from_backup:true}),
    }
}
pub fn save(path:&Path,snapshot:&WorldSave)->Result<()> {
    let bytes=encode(snapshot)?;
    if path.exists() {let old=read_bounded(path)?;if decode(&old).is_ok() {write_atomic(&backup_path(path)?,&old)?;}}
    write_atomic(path,&bytes)
}
fn read_bounded(path:&Path)->Result<Vec<u8>> {
    let file=File::open(path).with_context(||format!("read {}",path.display()))?;let metadata=file.metadata()?;
    ensure!(metadata.is_file()&&metadata.len()<=MAX_FILE,"world save is not a bounded regular file");
    let mut bytes=Vec::new();file.take(MAX_FILE+1).read_to_end(&mut bytes)?;ensure!(bytes.len() as u64<=MAX_FILE,"world save too large");Ok(bytes)
}
fn backup_path(path:&Path)->Result<PathBuf> {
    let mut name=path.file_name().context("world save requires filename")?.to_os_string();name.push(".bak");Ok(path.with_file_name(name))
}
fn write_atomic(path:&Path,bytes:&[u8])->Result<()> {
    let parent=path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or_else(||Path::new("."));fs::create_dir_all(parent)?;
    let name=path.file_name().context("world save requires filename")?.to_string_lossy();
    let (temp,mut file)=loop {
        let temp=parent.join(format!(".{name}.{}.{}.tmp",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file)=>break(temp,file),Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(e.into()),
        }
    };
    let result=(||->Result<()> {file.write_all(bytes)?;file.sync_all()?;drop(file);fs::rename(&temp,path)?;
        #[cfg(unix)]File::open(parent)?.sync_all()?;Ok(())})();
    if result.is_err(){let _=fs::remove_file(temp);}result
}

/// Static specification pointers are restored by stable model identity after the build gate,
/// never deserialized as addresses or replaced with a default Tiger specification.
pub mod vehicle_spec_ref {
    use serde::{Deserialize,Deserializer,Serializer};
    use crate::vehicle_spec as v;
    pub fn serialize<S:Serializer>(spec:&&'static v::VehicleSpec,s:S)->Result<S::Ok,S::Error> {s.serialize_str(spec.name)}
    pub fn deserialize<'de,D:Deserializer<'de>>(d:D)->Result<&'static v::VehicleSpec,D::Error> {
        let name=String::deserialize(d)?;
        [&v::TIGER_I,&v::T34_76,&v::SU100,&v::KV2,&v::IS2,&v::ISU152,&v::IS3].into_iter()
            .find(|spec|spec.name==name).ok_or_else(||serde::de::Error::custom(format!("unknown vehicle specification {name}")))
    }
}

pub use crate::ballistics::{shell_spec_ref,plate_name};

pub mod telemetry_label {
    use serde::{Deserialize,Deserializer,Serialize,Serializer};
    pub fn serialize<S:Serializer>(label:&Option<&'static str>,s:S)->Result<S::Ok,S::Error> {label.serialize(s)}
    pub fn deserialize<'de,D:Deserializer<'de>>(d:D)->Result<Option<&'static str>,D::Error> {
        Option::<String>::deserialize(d)?.map(|text| match text.as_str() {
            "infantry_rifle"=>Ok("infantry_rifle"),"tank_coax_mg"=>Ok("tank_coax_mg"),
            _=>Err(serde::de::Error::custom("unknown small-arms telemetry source")),
        }).transpose()
    }
}

#[cfg(test)]mod tests {
    use super::*;
    #[derive(Debug,Serialize,Deserialize)]struct Physical {health:f32,clock:f64,negative_zero:f32,pose:glam::Mat4,ammo:u32}
    #[test]fn live_owner_preserves_float_bits_and_rejects_trailing_or_corrupt_bytes() {
        let a=Physical{health:0.375,clock:f64::INFINITY,negative_zero:-0.0,pose:glam::Mat4::from_translation(glam::Vec3::new(1.25,2.5,-4.0)),ammo:7};
        let block=OwnerBlock::encode("test",1,&a).unwrap();let b:Physical=block.decode("test",1).unwrap();
        assert_eq!(a.health.to_bits(),b.health.to_bits());assert_eq!(a.clock.to_bits(),b.clock.to_bits());
        assert_eq!(a.negative_zero.to_bits(),b.negative_zero.to_bits());assert_eq!(a.pose,b.pose);assert_eq!(a.ammo,b.ammo);
        let mut trailing=block.clone();trailing.payload.push(0);trailing.checksum=checksum(&trailing.payload);assert!(trailing.decode::<Physical>("test",1).is_err());
        let mut damaged=block.clone();damaged.payload[0]^=1;assert!(damaged.decode::<Physical>("test",1).is_err());
        assert!(block.decode::<Physical>("test",2).is_err());
    }
    #[test]fn restored_drivetrain_continues_shift_and_broken_track_load_exactly() {
        use crate::powertrain::{Controls,Powertrain};
        let controls=Controls {drive:0.8,steer:-0.35,engine_running:true,transmission_ok:true,
            left_track:true,right_track:false,running_gear_factor:0.7,parking_brake:false,driver_brake:Some(0.0)};
        let mut original=Powertrain::default();let mut speed=0.0;
        for _ in 0..87 {speed=original.advance(1./60.,speed,0.035,controls).speed;}
        original.shift_manual(1);
        let block=OwnerBlock::encode("drivetrain",1,&original).unwrap();
        let mut restored:Powertrain=block.decode("drivetrain",1).unwrap();
        let mut restored_speed=speed;
        for _ in 0..180 {
            let a=original.advance(1./60.,speed,0.035,controls);
            let b=restored.advance(1./60.,restored_speed,0.035,controls);
            assert_eq!((a.speed.to_bits(),a.distance.to_bits(),a.yaw.to_bits()),(b.speed.to_bits(),b.distance.to_bits(),b.yaw.to_bits()));
            speed=a.speed;restored_speed=b.speed;
        }
        assert_eq!(OwnerBlock::encode("drivetrain",1,&original).unwrap().payload,
            OwnerBlock::encode("drivetrain",1,&restored).unwrap().payload);
    }
    #[test]fn hash_container_restore_keeps_canonical_owner_bytes() {
        let a=std::collections::HashMap::from([((11_i32,-8_i32),3_u64),((-2,9),21)]);
        let block=OwnerBlock::encode("map",1,&ordered_map::Ref(&a)).unwrap();
        let b:std::collections::HashMap<(i32,i32),u64>=block.decode("map",1).unwrap();
        assert_eq!(block.payload,OwnerBlock::encode("map",1,&ordered_map::Ref(&b)).unwrap().payload);
    }
    #[test]fn atomic_live_save_recovers_previous_valid_generation_after_truncation() {
        let dir=std::env::temp_dir().join(format!("kessel-world-{}-{}",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
        fs::create_dir(&dir).unwrap();let path=dir.join("world.bin");
        let world=WorldRecipe{manifest_path:"fixture.json".into(),run_seed:19,act_id:1,camp_root:glam::Mat4::IDENTITY.to_cols_array(),
            sources:vec![SourceFingerprint{path:"fixture.json".into(),bytes:10,checksum:3}]};
        let owners=REQUIRED_OWNERS.iter().map(|&(key,version)|OwnerBlock::encode(key,version,&Option::<u32>::None).unwrap()).collect();
        let mut state=WorldSave::new(world,12.5,owners).unwrap();save(&path,&state).unwrap();
        state.simulation_seconds=25.;save(&path,&state).unwrap();assert_eq!(load(&path).unwrap().simulation_seconds,25.);
        fs::write(&path,b"PZEXW001 truncated").unwrap();let loaded=load_with_recovery(&path).unwrap();
        assert!(loaded.recovered_from_backup);assert_eq!(loaded.save.simulation_seconds,12.5);
        fs::remove_file(&path).unwrap();fs::remove_file(backup_path(&path).unwrap()).unwrap();fs::remove_dir(dir).unwrap();
    }
    #[test]fn complete_world_contract_refuses_omitted_or_duplicate_owners() {
        let world=WorldRecipe{manifest_path:"fixture.json".into(),run_seed:19,act_id:1,camp_root:glam::Mat4::IDENTITY.to_cols_array(),
            sources:vec![SourceFingerprint{path:"fixture.json".into(),bytes:10,checksum:3}]};
        let owners=REQUIRED_OWNERS.iter().map(|&(key,version)|OwnerBlock::encode(key,version,&Option::<u32>::None).unwrap()).collect::<Vec<_>>();
        let save=WorldSave::new(world,12.5,owners).unwrap();let bytes=encode(&save).unwrap();assert_eq!(decode(&bytes).unwrap().world.run_seed,19);
        let mut missing=save.clone();missing.owners.pop();assert!(encode(&missing).is_err());
        let mut duplicate=save.clone();duplicate.owners[1]=duplicate.owners[0].clone();assert!(encode(&duplicate).is_err());
        let mut appended=bytes.clone();appended.push(0);assert!(decode(&appended).is_err());
        let mut wrong=save;wrong.build_fingerprint^=1;assert!(encode(&wrong).is_err());
    }
}

/// Stable bytes for hash containers; iteration order is not a simulation event. Keys sort by
/// their lossless fixed-width encoding, so raw owner checksums can prove a reload unchanged.
pub mod ordered_map {
    use serde::{Serialize,Serializer,Deserialize,Deserializer,ser::SerializeMap};
    use std::{collections::HashMap,hash::Hash};
    pub fn serialize<K:Serialize+Eq+Hash,V:Serialize,S:Serializer>(map:&HashMap<K,V>,s:S)->Result<S::Ok,S::Error> {
        let mut rows=map.iter().map(|(k,v)|Ok((bincode::serialize(k).map_err(serde::ser::Error::custom)?,k,v))).collect::<Result<Vec<_>,S::Error>>()?;
        rows.sort_by(|a,b|a.0.cmp(&b.0));let mut out=s.serialize_map(Some(rows.len()))?;
        for (_,k,v) in rows {out.serialize_entry(k,v)?;}out.end()
    }
    pub fn deserialize<'de,K:Deserialize<'de>+Eq+Hash,V:Deserialize<'de>,D:Deserializer<'de>>(d:D)->Result<HashMap<K,V>,D::Error> {HashMap::deserialize(d)}
    pub struct Ref<'a,K,V>(pub &'a HashMap<K,V>);
    impl<K:Serialize+Eq+Hash,V:Serialize> Serialize for Ref<'_,K,V> {
        fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error> {serialize(self.0,s)}
    }
}
pub mod ordered_set {
    use serde::{Serialize,Serializer,Deserialize,Deserializer,ser::SerializeSeq};
    use std::{collections::HashSet,hash::Hash};
    pub fn serialize<K:Serialize+Eq+Hash,S:Serializer>(set:&HashSet<K>,s:S)->Result<S::Ok,S::Error> {
        let mut rows=set.iter().map(|k|Ok((bincode::serialize(k).map_err(serde::ser::Error::custom)?,k))).collect::<Result<Vec<_>,S::Error>>()?;
        rows.sort_by(|a,b|a.0.cmp(&b.0));let mut out=s.serialize_seq(Some(rows.len()))?;
        for (_,k) in rows {out.serialize_element(k)?;}out.end()
    }
    pub fn deserialize<'de,K:Deserialize<'de>+Eq+Hash,D:Deserializer<'de>>(d:D)->Result<HashSet<K>,D::Error> {HashSet::deserialize(d)}
}
