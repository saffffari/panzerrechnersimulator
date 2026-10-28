//! Versioned, atomic ACT-ENTRANCE saves. This is not a snapshot of the running Game.
//! Loading regenerates the saved attempt's route and encounter plan, restoring entrance stores,
//! Tiger condition and building snapshots, while retaining discovered story knowledge.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, ensure, Context, Result};
use serde_json::{json, Value};

use crate::armor_damage::{self, ArmorField, ArmorScar, ScarKind, MAX_ARMOR_SCARS};
use crate::campaign::{
    ActEntryCheckpoint, CampaignInventory, CampaignRun, PersistentBuildingSnapshot,
    PersistentSnapshot, StorySiteId, TigerSnapshot,
};
use crate::run_route::SectorEdge;

const SCHEMA: &str = "panzerrechnersystem.campaign-checkpoint";
/// 2: armor scars are one packed hex string (`armor_damage::PACKED_SCAR_BYTES` per scar) with
/// plate normal, shot direction and terminal geometry. Version 1 files still load.
const VERSION: u64 = 2;
const VERSION_V1_SCARS: u64 = 1;
const SEMANTICS: &str = "act-entrance-same-attempt";
const MAX_SAVE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BUILDINGS: usize = 256;
const MAX_MODULES: usize = 8192;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct LoadedCheckpoint {
    pub run: CampaignRun,
    pub night: bool,
}

pub fn default_path() -> Result<PathBuf> {
    state_path(
        std::env::var_os("XDG_STATE_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

fn state_path(xdg: Option<PathBuf>, home: Option<PathBuf>) -> Result<PathBuf> {
    // XDG requires an absolute directory; an empty or relative value is ignored.
    let base = if let Some(path) = xdg.filter(|path| path.is_absolute()) {
        path
    } else {
        home.filter(|path| path.is_absolute())
            .context("HOME is unavailable for campaign saves")?
            .join(".local/state")
    };
    Ok(base.join("panzerrechnersystem/campaign-checkpoint.v1.json"))
}

pub fn save(path: &Path, run: &CampaignRun, night: bool) -> Result<()> {
    let checkpoint = run.export_act_entry_checkpoint()?;
    let compatibility = current_compatibility()?;
    let bytes = encode(&checkpoint, night, &compatibility)?;
    write_atomic(path, &bytes)
}

pub fn load(path: &Path) -> Result<LoadedCheckpoint> {
    let bytes = read_bounded(path, MAX_SAVE_BYTES).context("read campaign checkpoint")?;
    decode(&bytes, &current_compatibility()?)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "file is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "file exceeds read limit");
    Ok(bytes)
}

fn create_save_directory(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) => ensure!(metadata.is_dir(), "save directory is not a directory"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            create_save_directory(parent)?;
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    ensure!(fs::metadata(path)?.is_dir(), "save directory became a file");
                }
                Err(error) => return Err(error).context("create campaign save directory"),
            }
        }
        Err(error) => return Err(error).context("inspect campaign save directory"),
    }
    // A file fsync plus the final directory fsync does not persist a newly created directory's
    // own entry in its parent. Sync each level before creating the next. Sync an existing entry
    // too: it may be the directory left by a process interrupted just after mkdir on its last try.
    if let Some(parent) = path.parent() {
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        File::open(parent)
            .context("open save directory parent")?
            .sync_all()
            .context("sync save directory entry")?;
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= MAX_SAVE_BYTES,
        "checkpoint exceeds save size limit"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .context("checkpoint needs a file name")?
        .to_string_lossy();
    create_save_directory(parent)?;
    let directory = File::open(parent).context("open campaign save directory")?;
    let (temp, mut file) = loop {
        let serial = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(".{name}.{}.{serial}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp) {
            Ok(file) => break (temp, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("create temporary checkpoint"),
        }
    };
    let result = (|| -> Result<()> {
        file.write_all(bytes)
            .context("write temporary checkpoint")?;
        file.sync_all().context("sync temporary checkpoint")?;
        fs::rename(&temp, path).context("atomically replace checkpoint")?;
        directory
            .sync_all()
            .context("checkpoint installed but directory sync failed")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn hash_into(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
}
fn checksum(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325;
    hash_into(&mut hash, bytes);
    format!("fnv1a64:{hash:016x}")
}

/// Conservative compatibility gate: compiled campaign/route/world rules plus the selected authored
/// level designs, manifests, edits and terrain bytes. Building imports also validate layout hashes.
/// FNV detects accidental corruption/drift; this local format is not an authentication mechanism.
fn current_compatibility() -> Result<String> {
    let mut hash = 0xcbf29ce484222325;
    for bytes in [
        include_bytes!("campaign.rs").as_slice(),
        include_bytes!("run_route.rs").as_slice(),
        include_bytes!("world.rs").as_slice(),
        include_bytes!("../assets/worldgen/act1/level_design.v1.json").as_slice(),
        include_bytes!("../assets/worldgen/act2/level_design.v1.json").as_slice(),
        include_bytes!("../assets/worldgen/act3/level_design.v1.json").as_slice(),
    ] {
        hash_into(&mut hash, &(bytes.len() as u64).to_le_bytes());
        hash_into(&mut hash, bytes);
    }
    for act in 1..=3 {
        let logical = crate::act_manifest::path_for_act(act);
        let path = crate::resource::resolve(&logical)?;
        let manifest_bytes = read_bounded(&path, 16 * 1024 * 1024)?;
        hash_into(&mut hash, logical.to_string_lossy().as_bytes());
        hash_into(&mut hash, &manifest_bytes);
        let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
        let source = manifest["terrain"]["heights"]
            .as_str()
            .context("manifest terrain source missing")?;
        let relative = Path::new(source);
        ensure!(
            !relative.as_os_str().is_empty()
                && relative
                    .components()
                    .all(|c| matches!(c, Component::Normal(_))),
            "terrain source must stay inside its authored directory"
        );
        let terrain_path = path
            .parent()
            .context("manifest has no parent")?
            .join(relative);
        let terrain = read_bounded(&terrain_path, 128 * 1024 * 1024)?;
        hash_into(&mut hash, &(terrain.len() as u64).to_le_bytes());
        hash_into(&mut hash, &terrain);
        let edits = path.with_file_name("manifest_edits.json");
        match fs::metadata(&edits) {
            Ok(_) => {
                hash_into(&mut hash, b"edits-present");
                hash_into(&mut hash, &read_bounded(&edits, 16 * 1024 * 1024)?);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                hash_into(&mut hash, b"edits-absent")
            }
            Err(error) => return Err(error).context("inspect authored edits"),
        }
    }
    Ok(format!("fnv1a64:{hash:016x}"))
}

const SITES: [StorySiteId; 14] = [
    StorySiteId::Act1BarmaleyFountain,
    StorySiteId::Act1UnivermagCellar,
    StorySiteId::Act1GrainElevator,
    StorySiteId::Act1PitomnikAirfield,
    StorySiteId::Act1FrozenHorses,
    StorySiteId::Act2KienitzIceTracks,
    StorySiteId::Act2SearchlightRoad,
    StorySiteId::Act2DrownedOderbruch,
    StorySiteId::Act2OldHouse,
    StorySiteId::Act2ReitweinBunker,
    StorySiteId::Act3JammedColumn,
    StorySiteId::Act3DryTiger,
    StorySiteId::Act3AutobahnBlockingLine,
    StorySiteId::Act3ForestCemetery,
];

fn edge_json(edge: Option<SectorEdge>) -> Value {
    match edge {
        None => Value::Null,
        Some(SectorEdge::North) => json!("north"),
        Some(SectorEdge::East) => json!("east"),
        Some(SectorEdge::South) => json!("south"),
        Some(SectorEdge::West) => json!("west"),
    }
}

fn encode(checkpoint: &ActEntryCheckpoint, night: bool, compatibility: &str) -> Result<Vec<u8>> {
    let tiger = &checkpoint.entrance.tiger;
    let inventory = &checkpoint.entrance.inventory;
    let scars = hex_encode(&ArmorField::pack_all(&tiger.armor_scars));
    let buildings: Vec<_> = checkpoint.entrance.buildings.iter().map(|b| json!({
        "version":b.version,"act_id":b.act_id,"instance_id":b.instance_id,"blueprint_id":b.blueprint_id,
        "blueprint_revision":b.blueprint_revision,"layout_hash":b.layout_hash,
        "module_integrity":b.module_integrity,"fractional_damage":b.fractional_damage,
        "container_states":b.container_states,"door_states":b.door_states})).collect();
    let sites: Vec<_> = checkpoint
        .discovered_sites
        .iter()
        .map(|site| {
            SITES
                .iter()
                .position(|candidate| candidate == site)
                .expect("all campaign sites have disk codes")
        })
        .collect();
    let payload = json!({"run_seed":checkpoint.run_seed,"act_id":checkpoint.act_id,"night":night,
        "attempts_before_start":checkpoint.attempts_before_start,"discovered_sites":sites,
        "sector_entry_edge":edge_json(checkpoint.sector_entry_edge),
        "sector_orientation_quarters":checkpoint.sector_orientation_quarters,
        "entrance":{"tiger":{"vehicle_id":tiger.vehicle_id,"system_condition":tiger.system_condition,
            "hull_condition":tiger.hull_condition,"crew_fitness":tiger.crew_fitness,
            "interior_fire":tiger.interior_fire,"visible_scars":tiger.visible_scars,
            "armor_scars":scars,"field_track_repairs":tiger.field_track_repairs},
            "inventory":{"ap_rounds":inventory.ap_rounds,"he_rounds":inventory.he_rounds,
                "apcr_rounds":inventory.apcr_rounds,"mg_rounds":inventory.mg_rounds,
                "luger_rounds":inventory.luger_rounds,"panzerfaust_tubes":inventory.panzerfaust_tubes,
                "fuel_liters":inventory.fuel_liters,"spare_parts":inventory.spare_parts,
                "extinguishers":inventory.extinguishers,"cigarettes":inventory.cigarettes,
                "medical_supplies":inventory.medical_supplies},"buildings":buildings}});
    let bytes = serde_json::to_vec(&payload)?;
    let envelope = json!({"schema":SCHEMA,"version":VERSION,"semantics":SEMANTICS,
        "compatibility":compatibility,"payload_checksum":checksum(&bytes),"payload":payload});
    let encoded = serde_json::to_vec_pretty(&envelope)?;
    // Never install a file that our current reader would reject.
    decode(&encoded, compatibility)?;
    Ok(encoded)
}

/// Version-1 checkpoints stored each scar as an object with a centimetre position.
fn v1_scars(value: &Value) -> Result<Vec<ArmorScar>> {
    let mut scars = Vec::new();
    for scar in list(value, MAX_ARMOR_SCARS)? {
        let coords = list(&scar["local_cm"], 3)?;
        ensure!(coords.len() == 3, "armor scar needs three coordinates");
        let mut local_cm = [0i16; 3];
        for (i, v) in coords.iter().enumerate() {
            local_cm[i] = v
                .as_i64()
                .and_then(|n| i16::try_from(n).ok())
                .context("invalid scar coordinate")?;
        }
        let radius_mm: u16 = number(&scar["radius_mm"], "scar radius")?;
        let loss_mm: u16 = number(&scar["loss_mm"], "scar loss")?;
        ensure!(radius_mm >= 20 && loss_mm > 0, "invalid physical scar");
        scars.push(ArmorScar::from_v1(
            armor_damage::facing_from_code(number(&scar["facing"], "scar facing")?)
                .context("unknown armor facing")?,
            local_cm,
            radius_mm,
            loss_mm,
            ScarKind::from_u8(number(&scar["kind"], "scar kind")?)
                .filter(|kind| *kind != ScarKind::Pock)
                .context("unknown scar kind")?,
        ));
    }
    Ok(scars)
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let bytes = text.as_bytes();
    (bytes.len() % 2 == 0).then_some(())?;
    bytes.chunks_exact(2).map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?)).collect()
}

fn number<T: TryFrom<u64>>(value: &Value, name: &str) -> Result<T> {
    value
        .as_u64()
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| anyhow!("invalid {name}"))
}
fn list(value: &Value, max: usize) -> Result<&Vec<Value>> {
    let array = value.as_array().context("expected checkpoint array")?;
    ensure!(array.len() <= max, "checkpoint array exceeds its limit");
    Ok(array)
}
fn numbers<T: TryFrom<u64>>(value: &Value, max: usize) -> Result<Vec<T>> {
    list(value, max)?
        .iter()
        .map(|v| number(v, "array integer"))
        .collect()
}
fn name(value: &Value) -> Result<String> {
    let text = value.as_str().context("expected identifier")?;
    ensure!(
        !text.is_empty() && text.len() <= 256 && !text.chars().any(char::is_control),
        "invalid identifier"
    );
    Ok(text.to_owned())
}

fn decode(bytes: &[u8], compatibility: &str) -> Result<LoadedCheckpoint> {
    ensure!(
        bytes.len() as u64 <= MAX_SAVE_BYTES,
        "checkpoint exceeds read limit"
    );
    let envelope: Value = serde_json::from_slice(bytes).context("parse campaign checkpoint")?;
    ensure!(
        envelope["schema"] == SCHEMA
            && matches!(envelope["version"].as_u64(), Some(VERSION | VERSION_V1_SCARS))
            && envelope["semantics"] == SEMANTICS,
        "unsupported campaign checkpoint format"
    );
    ensure!(
        envelope["compatibility"] == compatibility,
        "checkpoint belongs to different campaign rules or authored maps"
    );
    let p = &envelope["payload"];
    ensure!(
        envelope["payload_checksum"] == checksum(&serde_json::to_vec(p)?),
        "checkpoint checksum mismatch"
    );
    let night = p["night"]
        .as_bool()
        .context("checkpoint time of day missing")?;
    let t = &p["entrance"]["tiger"];
    let scars = if envelope["version"].as_u64() == Some(VERSION) {
        let text = t["armor_scars"].as_str().context("armor scars must be packed text")?;
        let bytes = hex_decode(text).context("armor scars are not hex")?;
        ArmorField::unpack_all(&bytes).context("invalid packed armor scar")?
    } else {
        v1_scars(&t["armor_scars"])?
    };

    let system_condition = numbers::<u8>(&t["system_condition"], 12)?
        .try_into()
        .map_err(|_| anyhow!("checkpoint needs twelve Tiger systems"))?;
    let tiger = TigerSnapshot {
        vehicle_id: number(&t["vehicle_id"], "vehicle ID")?,
        system_condition,
        hull_condition: number(&t["hull_condition"], "hull condition")?,
        crew_fitness: number(&t["crew_fitness"], "crew fitness")?,
        interior_fire: number(&t["interior_fire"], "interior fire")?,
        visible_scars: number(&t["visible_scars"], "visible scars")?,
        armor_scars: scars,
        field_track_repairs: number(&t["field_track_repairs"], "track repairs")?,
    };
    let i = &p["entrance"]["inventory"];
    let inventory = CampaignInventory {
        ap_rounds: number(&i["ap_rounds"], "AP rounds")?,
        he_rounds: number(&i["he_rounds"], "HE rounds")?,
        apcr_rounds: number(&i["apcr_rounds"], "APCR rounds")?,
        mg_rounds: number(&i["mg_rounds"], "MG rounds")?,
        luger_rounds: number(&i["luger_rounds"], "P08 rounds")?,
        panzerfaust_tubes: number(&i["panzerfaust_tubes"], "Panzerfaust tubes")?,
        fuel_liters: number(&i["fuel_liters"], "fuel")?,
        spare_parts: number(&i["spare_parts"], "spare parts")?,
        extinguishers: number(&i["extinguishers"], "extinguishers")?,
        cigarettes: number(&i["cigarettes"], "cigarettes")?,
        medical_supplies: number(&i["medical_supplies"], "medical supplies")?,
    };
    let act_id: u8 = number(&p["act_id"], "act ID")?;
    let mut buildings = Vec::<PersistentBuildingSnapshot>::new();
    for b in list(&p["entrance"]["buildings"], MAX_BUILDINGS)? {
        let building = PersistentBuildingSnapshot {
            version: number(&b["version"], "building version")?,
            act_id: number(&b["act_id"], "building act")?,
            instance_id: name(&b["instance_id"])?,
            blueprint_id: name(&b["blueprint_id"])?,
            blueprint_revision: number(&b["blueprint_revision"], "blueprint revision")?,
            layout_hash: number(&b["layout_hash"], "building layout hash")?,
            module_integrity: numbers(&b["module_integrity"], MAX_MODULES)?,
            fractional_damage: numbers(&b["fractional_damage"], MAX_MODULES)?,
            container_states: numbers(&b["container_states"], MAX_MODULES)?,
            door_states: numbers(&b["door_states"], MAX_MODULES)?,
        };
        ensure!(
            (building.version == 1 || building.version == 2)
                && (1..=act_id).contains(&building.act_id)
                && !building.module_integrity.is_empty()
                && building.module_integrity.len() == building.fractional_damage.len()
                && building.container_states.iter().all(|n| *n <= 2)
                && building.door_states.iter().all(|n| *n <= 3)
                && (building.version != 1 || building.door_states.is_empty()),
            "invalid building checkpoint"
        );
        ensure!(!buildings.iter().any(|old| old.act_id == building.act_id && old.instance_id == building.instance_id),
            "duplicate building checkpoint");
        buildings.push(building);
    }
    let discovered_sites = numbers::<usize>(&p["discovered_sites"], SITES.len())?
        .into_iter()
        .map(|i| SITES.get(i).copied().context("unknown story site"))
        .collect::<Result<Vec<_>>>()?;
    let attempts_before_start = numbers::<u16>(&p["attempts_before_start"], 3)?
        .try_into()
        .map_err(|_| anyhow!("checkpoint needs three attempt counters"))?;
    let edge = match p.get("sector_entry_edge") {
        Some(Value::Null) => None,
        Some(Value::String(edge)) => Some(match edge.as_str() {
            "north" => SectorEdge::North,
            "east" => SectorEdge::East,
            "south" => SectorEdge::South,
            "west" => SectorEdge::West,
            _ => anyhow::bail!("unknown sector edge"),
        }),
        _ => anyhow::bail!("invalid sector edge"),
    };
    let run = CampaignRun::from_act_entry_checkpoint(ActEntryCheckpoint {
        run_seed: number(&p["run_seed"], "run seed")?,
        act_id,
        entrance: PersistentSnapshot {
            tiger,
            inventory,
            buildings,
        },
        discovered_sites,
        attempts_before_start,
        sector_entry_edge: edge,
        sector_orientation_quarters: number(
            &p["sector_orientation_quarters"],
            "sector orientation",
        )?,
    })?;
    Ok(LoadedCheckpoint { run, night })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::{FinalDefeatReason, ObjectiveId};

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let serial = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "panzer-checkpoint-test-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn roundtrip(run: &CampaignRun) -> LoadedCheckpoint {
        decode(
            &encode(
                &run.export_act_entry_checkpoint().unwrap(),
                true,
                "test-world",
            )
            .unwrap(),
            "test-world",
        )
        .unwrap()
    }

    fn complete_objectives(run: &mut CampaignRun) {
        let objectives: Vec<_> = run
            .progress()
            .objectives()
            .iter()
            .map(|objective| (objective.id, objective.target))
            .collect();
        for (id, target) in objectives {
            run.record_objective_progress(id, target).unwrap();
        }
    }

    fn runtime_map(act: u8, seed: u64) -> crate::world::ActMap {
        let mut map = crate::world::act(act);
        map.seed ^= seed as u32 ^ (seed >> 32) as u32;
        map
    }

    #[test]
    fn disk_roundtrip_restores_entrance_not_live_damage_and_replays_same_retry() {
        let mut initial = CampaignRun::new(42).persistent().clone();
        initial.inventory.ap_rounds = 17;
        initial.tiger.armor_scars.push(ArmorScar::from_v1(
            crate::ballistics::Facing::HullSide,
            [-30, 120, -90],
            80,
            35,
            ScarKind::Hole,
        ));
        initial.buildings.push(PersistentBuildingSnapshot {
            version: 2,
            act_id: 1,
            instance_id: "house".into(),
            blueprint_id: "test-house".into(),
            blueprint_revision: 4,
            layout_hash: 123,
            module_integrity: vec![90, 0],
            fractional_damage: vec![0, 12],
            container_states: vec![2],
            door_states: vec![1],
        });
        let mut run = CampaignRun::with_snapshot(42, initial.clone());
        let map = crate::world::act(1);
        run.begin_act_attempt(&map).unwrap();
        run.resolve_tiger_loss(FinalDefeatReason::TigerDestroyed)
            .unwrap();
        let retry = run.begin_act_attempt(&map).unwrap();
        run.record_discovery(StorySiteId::Act1BarmaleyFountain)
            .unwrap();
        run.record_objective_progress(ObjectiveId::Act1ChooseBreakoutRoute, 1)
            .unwrap();
        run.persistent_mut().inventory.ap_rounds = 1;
        run.persistent_mut().tiger.hull_condition = 3;
        run.persistent_mut().buildings.clear();
        let temp = Temp::new();
        let path = temp.0.join("save.json");
        save(&path, &run, true).unwrap();
        let mut loaded = load(&path).unwrap();
        assert!(loaded.night);
        assert!(!loaded.run.attempt_active());
        assert_eq!(loaded.run.persistent(), &initial);
        assert!(loaded
            .run
            .knowledge()
            .has_seen(StorySiteId::Act1BarmaleyFountain));
        assert!(!loaded
            .run
            .progress()
            .complete(ObjectiveId::Act1ChooseBreakoutRoute));
        assert_eq!(loaded.run.prepare_act_attempt(&map).unwrap(), retry);
        loaded.run.commit_prepared_attempt(&retry).unwrap();
        assert_eq!(loaded.run.attempts_for(1).unwrap(), 2);
    }

    #[test]
    fn later_sector_orientation_and_explicit_finale_restart_survive_roundtrip() {
        let mut run = CampaignRun::new(810);
        for act in 1..=2 {
            run.begin_act_attempt(&crate::world::act(act)).unwrap();
            let pending: Vec<_> = run
                .progress()
                .objectives()
                .iter()
                .map(|o| (o.id, o.target))
                .collect();
            for (id, target) in pending {
                run.record_objective_progress(id, target).unwrap();
            }
            run.persistent_mut().inventory.fuel_liters = 200 + u16::from(act);
            run.try_complete_act().unwrap();
            let loaded = roundtrip(&run);
            assert_eq!(
                loaded
                    .run
                    .prepare_act_attempt(&crate::world::act(act + 1))
                    .unwrap(),
                run.prepare_act_attempt(&crate::world::act(act + 1))
                    .unwrap()
            );
            assert_eq!(
                loaded.run.export_act_entry_checkpoint().unwrap(),
                run.export_act_entry_checkpoint().unwrap()
            );
        }
        let finale = run.begin_act_attempt(&crate::world::act(3)).unwrap();
        let mut loaded = roundtrip(&run).run;
        assert_eq!(
            loaded.prepare_act_attempt(&crate::world::act(3)).unwrap(),
            finale
        );
        loaded.commit_prepared_attempt(&finale).unwrap();
        loaded
            .resolve_tiger_loss(FinalDefeatReason::TigerDestroyed)
            .unwrap();
        assert!(loaded.export_act_entry_checkpoint().is_err());
    }

    #[test]
    fn active_second_act_retry_loads_and_can_advance_through_a_saved_finale() {
        use crate::campaign::{ActResult, CampaignEnding, FinaleReport};
        let seed = 0xC0DE_1942_45A1_0001;
        let mut run = CampaignRun::new(seed);
        run.begin_act_attempt(&runtime_map(1, seed)).unwrap();
        run.record_discovery(StorySiteId::Act1BarmaleyFountain)
            .unwrap();
        complete_objectives(&mut run);
        let mut act2_entrance = run.persistent().clone();
        act2_entrance.tiger.hull_condition = 74;
        act2_entrance.inventory.ap_rounds = 19;
        act2_entrance.inventory.fuel_liters = 230;
        assert_eq!(
            run.try_complete_act_with_snapshot(act2_entrance.clone())
                .unwrap(),
            ActResult::Advance {
                completed_act: 1,
                next_act: 2
            }
        );
        // Reaching 1945 issues the Tiger's Panzerfausts on top of the accepted snapshot.
        act2_entrance.inventory.panzerfaust_tubes = crate::campaign::PANZERFAUST_ISSUE_1945;
        let map2 = runtime_map(2, seed);
        run.begin_act_attempt(&map2).unwrap();
        run.resolve_tiger_loss(FinalDefeatReason::TigerDestroyed)
            .unwrap();
        let retry = run.begin_act_attempt(&map2).unwrap();
        assert_eq!(retry.attempt_number, 2);
        run.record_discovery(StorySiteId::Act2OldHouse).unwrap();
        let objective = run.progress().objectives()[0].id;
        run.record_objective_progress(objective, 1).unwrap();
        run.persistent_mut().inventory.fuel_liters = 7;

        let temp = Temp::new();
        let path = temp.0.join("checkpoint.json");
        save(&path, &run, false).unwrap();
        let mut restored = load(&path).unwrap();
        assert!(!restored.night);
        assert_eq!(restored.run.persistent(), &act2_entrance);
        assert!(restored
            .run
            .knowledge()
            .has_seen(StorySiteId::Act1BarmaleyFountain));
        assert!(restored.run.knowledge().has_seen(StorySiteId::Act2OldHouse));
        assert_eq!(restored.run.prepare_act_attempt(&map2).unwrap(), retry);
        assert_eq!(
            restored.run.progress(),
            &crate::campaign::ActProgress::new(2).unwrap()
        );
        restored.run.commit_prepared_attempt(&retry).unwrap();
        complete_objectives(&mut restored.run);
        let mut act3_entrance = restored.run.persistent().clone();
        act3_entrance.tiger.hull_condition = 55;
        act3_entrance.inventory.fuel_liters = 111;
        assert_eq!(
            restored
                .run
                .try_complete_act_with_snapshot(act3_entrance.clone())
                .unwrap(),
            ActResult::Advance {
                completed_act: 2,
                next_act: 3
            }
        );
        let map3 = runtime_map(3, seed);
        let finale = restored.run.begin_act_attempt(&map3).unwrap();
        restored
            .run
            .record_discovery(StorySiteId::Act3JammedColumn)
            .unwrap();
        save(&path, &restored.run, true).unwrap();

        let mut final_load = load(&path).unwrap();
        assert!(final_load.night);
        assert_eq!(final_load.run.persistent(), &act3_entrance);
        assert_eq!(final_load.run.attempts_for(1).unwrap(), 1);
        assert_eq!(final_load.run.attempts_for(2).unwrap(), 2);
        assert_eq!(final_load.run.prepare_act_attempt(&map3).unwrap(), finale);
        assert!(final_load
            .run
            .knowledge()
            .has_seen(StorySiteId::Act3JammedColumn));
        final_load.run.commit_prepared_attempt(&finale).unwrap();
        complete_objectives(&mut final_load.run);
        assert_eq!(
            final_load
                .run
                .finish_finale(FinaleReport {
                    player_alive: true,
                    town_integrity_percent: 70,
                    friendly_tigers_surviving: 1,
                    civilian_column_survived: true,
                })
                .unwrap(),
            ActResult::CampaignVictory {
                ending: CampaignEnding::TownHeld
            }
        );
        assert!(save(&path, &final_load.run, true).is_err());
        assert_eq!(
            load(&path).unwrap().run.current_act,
            3,
            "terminal save cannot replace the last entrance"
        );
    }

    #[test]
    fn interrupted_temporary_save_is_ignored_and_truncated_primary_fails_closed() {
        let temp = Temp::new();
        let path = temp.0.join("checkpoint.json");
        let mut run = CampaignRun::new(88);
        run.begin_act_attempt(&runtime_map(1, 88)).unwrap();
        save(&path, &run, false).unwrap();
        let committed = fs::read(&path).unwrap();
        // Model the on-disk remnant of termination before the atomic rename. It must never be
        // chosen over the installed checkpoint, even if it happens to be the newest directory entry.
        let orphan = temp.0.join(".checkpoint.json.12345.1.tmp");
        fs::write(&orphan, &committed[..committed.len() / 2]).unwrap();
        assert_eq!(load(&path).unwrap().run.run_seed, 88);
        assert_eq!(fs::read(&path).unwrap(), committed);
        fs::write(&path, &committed[..committed.len() / 2]).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), &committed[..committed.len() / 2]);
        fs::remove_file(&path).unwrap();
        assert!(load(&path).is_err(), "an orphan is not an installed save");
        save(&path, &run, true).unwrap();
        assert!(load(&path).unwrap().night);
        assert!(
            orphan.is_file(),
            "saving must not remove another interrupted writer's file"
        );
    }

    #[test]
    fn first_save_creates_nested_directory_tree_and_rejects_file_ancestors() {
        let temp = Temp::new();
        let path = temp.0.join("new-state/panzerrechnersystem/checkpoint.json");
        write_atomic(&path, b"complete").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"complete");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let blocked = temp.0.join("not-a-directory");
        fs::write(&blocked, b"keep").unwrap();
        assert!(write_atomic(&blocked.join("nested/checkpoint.json"), b"new").is_err());
        assert_eq!(fs::read(&blocked).unwrap(), b"keep");
    }

    #[test]
    fn hundreds_of_armor_scars_round_trip_and_version_one_files_still_load() {
        use crate::ballistics::Facing;
        let mut field = ArmorField::default();
        for i in 0..MAX_ARMOR_SCARS {
            let local = glam::Vec3::new(1.8, 0.4 + (i / 32) as f32 * 0.06, -2.5 + (i % 32) as f32 * 0.16);
            field.record_pock(Facing::HullSide, local, glam::Vec3::X, -glam::Vec3::X, 7.92, 3600.0, 80.0, 0.0);
        }
        assert_eq!(field.scars().len(), MAX_ARMOR_SCARS);
        let mut snapshot = CampaignRun::new(11).persistent().clone();
        snapshot.tiger.armor_scars = field.scars().to_vec();
        let run = CampaignRun::with_snapshot(11, snapshot.clone());
        let bytes = encode(&run.export_act_entry_checkpoint().unwrap(), false, "world").unwrap();
        assert!(bytes.len() < 64 * 1024, "{} bytes for {} scars", bytes.len(), MAX_ARMOR_SCARS);
        let loaded = decode(&bytes, "world").unwrap();
        assert_eq!(loaded.run.persistent().tiger.armor_scars, snapshot.tiger.armor_scars);

        // A version-1 file: the old object list with centimetre positions.
        let mut old: Value = serde_json::from_slice(&bytes).unwrap();
        old["version"] = json!(VERSION_V1_SCARS);
        old["payload"]["entrance"]["tiger"]["armor_scars"] = json!([
            {"facing":1,"local_cm":[-30,120,-90],"radius_mm":80,"loss_mm":35,"kind":1}]);
        old["payload_checksum"] = json!(checksum(&serde_json::to_vec(&old["payload"]).unwrap()));
        let loaded = decode(&serde_json::to_vec(&old).unwrap(), "world").unwrap();
        assert_eq!(loaded.run.persistent().tiger.armor_scars,
            vec![ArmorScar::from_v1(Facing::HullSide, [-30, 120, -90], 80, 35, ScarKind::Hole)]);

        // Damaged packed text is rejected, not partly applied.
        let mut bad: Value = serde_json::from_slice(&bytes).unwrap();
        bad["payload"]["entrance"]["tiger"]["armor_scars"] = json!("zz");
        bad["payload_checksum"] = json!(checksum(&serde_json::to_vec(&bad["payload"]).unwrap()));
        assert!(decode(&serde_json::to_vec(&bad).unwrap(), "world").is_err());
    }

    #[test]
    fn malformed_or_incompatible_saves_are_rejected_without_mutating_run() {
        let run = CampaignRun::new(7);
        let bytes = encode(&run.export_act_entry_checkpoint().unwrap(), false, "world").unwrap();
        assert!(decode(&bytes, "changed-world").is_err());
        assert!(decode(&bytes[..bytes.len() / 2], "world").is_err());
        let original: Value = serde_json::from_slice(&bytes).unwrap();
        for (pointer, value) in [
            ("/payload/act_id", json!(0)),
            ("/payload/entrance/tiger/hull_condition", json!(101)),
            ("/payload/attempts_before_start", json!([0, 1, 0])),
            ("/payload/discovered_sites", json!([99])),
            ("/payload/night", json!("night")),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            changed["payload_checksum"] =
                json!(checksum(&serde_json::to_vec(&changed["payload"]).unwrap()));
            assert!(
                decode(&serde_json::to_vec(&changed).unwrap(), "world").is_err(),
                "accepted {pointer}"
            );
        }
        let mut corrupt = original.clone();
        corrupt["payload"]["run_seed"] = json!(88);
        assert!(decode(&serde_json::to_vec(&corrupt).unwrap(), "world").is_err());
        corrupt = original;
        corrupt["version"] = json!(VERSION + 1);
        assert!(decode(&serde_json::to_vec(&corrupt).unwrap(), "world").is_err());
        assert_eq!(run.run_seed, 7);
    }

    #[test]
    fn atomic_replace_missing_size_and_failed_write_paths() {
        let temp = Temp::new();
        let path = temp.0.join("save.json");
        assert!(read_bounded(&path, MAX_SAVE_BYTES).is_err());
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(read_bounded(&path, MAX_SAVE_BYTES).unwrap(), b"new");
        assert!(write_atomic(&path, &vec![0; MAX_SAVE_BYTES as usize + 1]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(read_bounded(&path, 2).is_err());
        let destination_directory = temp.0.join("directory");
        fs::create_dir(&destination_directory).unwrap();
        assert!(write_atomic(&destination_directory, b"not-a-file").is_err());
        assert!(destination_directory.is_dir());
        assert!(!fs::read_dir(&temp.0).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }

    #[test]
    fn state_path_obeys_xdg_without_relative_directory_writes() {
        assert_eq!(
            state_path(Some("/state".into()), None).unwrap(),
            Path::new("/state/panzerrechnersystem/campaign-checkpoint.v1.json")
        );
        assert_eq!(
            state_path(Some("relative".into()), Some("/owner".into())).unwrap(),
            Path::new("/owner/.local/state/panzerrechnersystem/campaign-checkpoint.v1.json")
        );
        assert!(state_path(None, None).is_err());
    }
}
