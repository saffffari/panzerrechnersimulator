//! Saves for the expedition coordinator and physical records explicitly supplied to it.
//! The format intentionally does not claim live-Game/mid-combat suspension: that adapter,
//! including in-flight projectiles and all solver timers, is still a separate implementation.

use crate::expedition::Expedition;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const SCHEMA: &str = "panzerrechnersystem.expedition";
const VERSION: u32 = 2;
const SCOPE: &str = "coordinator-and-explicit-physical-records";
const MAX_BYTES: u64 = 64 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: String,
    version: u32,
    snapshot_scope: String,
    live_game_adapter_complete: bool,
    payload_checksum: String,
    payload: serde_json::Value,
}

pub struct LoadedExpedition {
    pub expedition: Expedition,
    /// A recovery must be reported to the player; it may lose the latest transaction.
    pub recovered_from_backup: bool,
}

pub fn default_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".local/state"))
        })
        .context("no absolute user state directory")?;
    Ok(base.join("panzerrechnersystem/expedition.v2.json"))
}

fn checksum(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |h, byte| {
        (h ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64:{hash:016x}")
}

pub fn encode(expedition: &Expedition) -> Result<Vec<u8>> {
    expedition.validate()?;
    let payload = serde_json::to_value(expedition)?;
    let envelope = Envelope {
        schema: SCHEMA.into(),
        version: VERSION,
        snapshot_scope: SCOPE.into(),
        live_game_adapter_complete: false,
        payload_checksum: checksum(&serde_json::to_vec(&payload)?),
        payload,
    };
    let bytes = serde_json::to_vec(&envelope)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "expedition exceeds save size limit"
    );
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<Expedition> {
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "expedition exceeds read size limit"
    );
    let envelope: Envelope = serde_json::from_slice(bytes).context("parse expedition envelope")?;
    ensure!(envelope.version != 1, "expedition v1 cannot be migrated: its aggregate health lacks wounds and treatment cadence; preserve the old development proof and start a v2 expedition");
    ensure!(
        envelope.schema == SCHEMA
            && envelope.version == VERSION
            && envelope.snapshot_scope == SCOPE
            && !envelope.live_game_adapter_complete,
        "unsupported expedition format or snapshot scope"
    );
    ensure!(
        envelope.payload_checksum == checksum(&serde_json::to_vec(&envelope.payload)?),
        "expedition checksum mismatch"
    );
    let expedition: Expedition =
        serde_json::from_value(envelope.payload).context("parse expedition records")?;
    expedition
        .validate()
        .context("validate expedition records")?;
    Ok(expedition)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_BYTES,
        "save is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "expedition exceeds read size limit"
    );
    Ok(bytes)
}

fn backup_path(path: &Path) -> Result<PathBuf> {
    let mut name = path
        .file_name()
        .context("expedition save requires a filename")?
        .to_os_string();
    name.push(".bak");
    Ok(path.with_file_name(name))
}

/// Reads exactly this file. Call load_with_recovery when an explicit backup fallback is wanted.
pub fn load(path: &Path) -> Result<Expedition> {
    decode(&read_bounded(path)?)
}

pub fn load_with_recovery(path: &Path) -> Result<LoadedExpedition> {
    match load(path) {
        Ok(expedition) => Ok(LoadedExpedition {
            expedition,
            recovered_from_backup: false,
        }),
        Err(primary_error) => {
            let expedition = load(&backup_path(path)?).with_context(|| {
                format!("primary save failed ({primary_error:#}); backup also unavailable")
            })?;
            Ok(LoadedExpedition {
                expedition,
                recovered_from_backup: true,
            })
        }
    }
}

/// Installs a validated new save. A valid prior primary becomes the corruption-recovery backup.
/// Invalid state never touches either file, and a damaged primary cannot overwrite a good backup.
pub fn save(path: &Path, expedition: &Expedition) -> Result<()> {
    let bytes = encode(expedition)?;
    let backup = backup_path(path)?;
    match read_bounded(path) {
        Ok(previous) if decode(&previous).is_ok() => write_atomic(&backup, &previous)?,
        Ok(_) => {} // Keep the last known valid backup if the primary has been corrupted.
        Err(error) if path.exists() => {
            return Err(error).context("inspect previous expedition save")
        }
        Err(_) => {}
    }
    write_atomic(path, &bytes)
}

/// The caller's live coordinator changes only after the candidate has been durably installed.
/// Use for dispatch/death/transfer transactions so a failed save cannot spend a second slot.
pub fn transaction<T>(
    path: &Path,
    live: &mut Expedition,
    apply: impl FnOnce(&mut Expedition) -> Result<T>,
) -> Result<T> {
    let mut candidate = live.clone();
    let result = apply(&mut candidate)?;
    save(path, &candidate)?;
    *live = candidate;
    Ok(result)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "expedition exceeds save size limit"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).context("create expedition save directory")?;
    let name = path
        .file_name()
        .context("save requires a filename")?
        .to_string_lossy();
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
            Err(error) => return Err(error).context("create temporary expedition save"),
        }
    };
    let result = (|| -> Result<()> {
        file.write_all(bytes).context("write expedition save")?;
        file.sync_all().context("sync expedition save")?;
        drop(file); // Windows must not retain a conflicting file handle across replacement.
        fs::rename(&temp, path).context("replace expedition save")?;
        // Opening a directory as File fails on Windows. On Unix, sync the directory entry too.
        #[cfg(unix)]
        {
            File::open(parent)?
                .sync_all()
                .context("sync expedition save directory")?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expedition::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let serial = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "panzer-expedition-save-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("expedition.json")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn records_roundtrip_without_reviving_people_or_rerolling_world() {
        let mut run = Expedition::new(137);
        run.vehicles[0].position = WorldPosition {
            x: 2000.0,
            y: 10.0,
            z: -40.0,
        };
        run.vehicles[0].recovery_required = true;
        run.components[6].fault = Some("broken_link".into());
        run.components[6].repair = Some(RepairProgress {
            procedure: "track_replacement".into(),
            completed_steps: 2,
            total_steps: 5,
        });
        run.items.push(Item {
            id: ItemId(100),
            kind: "track pin".into(),
            quantity: 1,
            condition: 1.0,
            location: ItemLocation::Ground {
                position: run.vehicles[0].position,
            },
        });
        let enemy = Vehicle {
            id: VehicleId(20),
            kind: VehicleKind::Enemy,
            position: WorldPosition {
                x: 2100.0,
                ..Default::default()
            },
            orientation_radians: [0.1, 0.2, 0.3],
            velocity_metres_per_second: [0.0; 3],
            fuel_litres: 12.0,
            fire_intensity: 0.0,
            recovery_required: false,
            recoverable: true,
            scars: Vec::new(),
        };
        run.vehicles.push(enemy);
        run.enemies.push(EnemyRecord {
            vehicle: VehicleId(20),
            intent: "patrol".into(),
            destination: WorldPosition {
                x: 2200.0,
                ..Default::default()
            },
            ammunition: 3,
            timers: [("reload".into(), 2.25)].into(),
        });
        run.rng_state = 983479;
        run.simulation_seconds = 200.5;
        run.record_death(EventId(1), PersonId(1), run.vehicles[0].position)
            .unwrap();
        run.take_control(PersonId(6)).unwrap();
        let encoded = encode(&run).unwrap();
        assert_eq!(decode(&encoded).unwrap(), run);
        let json: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(json["live_game_adapter_complete"], false);
        assert_eq!(json["snapshot_scope"], SCOPE);
    }
    #[test]
    fn corruption_and_legacy_checkpoint_are_rejected() {
        let bytes = encode(&Expedition::new(1)).unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["payload"]["preparation_slots"] = serde_json::json!(0);
        assert!(decode(&serde_json::to_vec(&json).unwrap()).is_err());
        assert!(
            decode(br#"{"schema":"panzerrechnersystem.campaign-checkpoint","version":2}"#).is_err()
        );
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
        let mut v1: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        v1["version"] = serde_json::json!(1);
        assert!(decode(&serde_json::to_vec(&v1).unwrap())
            .unwrap_err()
            .to_string()
            .contains("cannot be migrated"));
    }
    #[test]
    fn disk_replacement_keeps_explicit_corruption_backup() {
        let temp = Temp::new();
        let path = temp.path();
        let first = Expedition::new(1);
        save(&path, &first).unwrap();
        let mut second = first.clone();
        second.simulation_seconds = 12.0;
        save(&path, &second).unwrap();
        assert_eq!(load(&path).unwrap(), second);
        assert_eq!(load(&backup_path(&path).unwrap()).unwrap(), first);
        fs::write(&path, b"corrupt").unwrap();
        let recovered = load_with_recovery(&path).unwrap();
        assert!(recovered.recovered_from_backup);
        assert_eq!(recovered.expedition, first);
        save(&path, &second).unwrap();
        assert_eq!(load(&backup_path(&path).unwrap()).unwrap(), first);
    }
    #[test]
    fn invalid_save_and_failed_transaction_preserve_last_state() {
        let temp = Temp::new();
        let path = temp.path();
        let mut live = Expedition::new(1);
        save(&path, &live).unwrap();
        let before = live.clone();
        let bytes = fs::read(&path).unwrap();
        assert!(transaction(&path, &mut live, |r| {
            r.preparation_slots = 2;
            Ok(())
        })
        .is_err());
        assert_eq!(live, before);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let blocker = temp.0.join("not_a_directory");
        fs::write(&blocker, b"file").unwrap();
        assert!(transaction(&blocker.join("save"), &mut live, |r| {
            r.simulation_seconds = 20.0;
            Ok(())
        })
        .is_err());
        assert_eq!(live, before);
    }
    #[test]
    fn dispatch_transaction_does_not_double_spend_on_retry() {
        let temp = Temp::new();
        let mut live = Expedition::new(1);
        live.vehicles[0].recovery_required = true;
        for _ in 0..2 {
            transaction(&temp.path(), &mut live, |r| {
                r.begin_dispatch(DispatchId(1), DispatchKind::Recovery { vehicle: TIGER_ID })
            })
            .unwrap();
        }
        assert_eq!(live.preparation_slots, 7);
        assert_eq!(load(&temp.path()).unwrap().dispatches.len(), 1);
    }
}
