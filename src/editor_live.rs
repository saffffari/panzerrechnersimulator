//! Opt-in local authoring mailbox for the visible world editor. Adds to in-memory state;
//! never replaces the manifest, user's placements, selection, or active transform.
use super::*;
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use std::hash::{Hash, Hasher};

pub(super) struct LiveAuthoring {
    root: PathBuf,
    last_poll: Instant,
    last_snapshot_hash: Option<u64>,
}

impl LiveAuthoring {
    pub(super) fn from_env() -> Option<Self> {
        let root = PathBuf::from(std::env::var_os("PANZER_EDITOR_LIVE_DIR")?);
        if !root.is_absolute() || !root.join("inbox").is_dir() || !root.join("outbox").is_dir() {
            eprintln!("[editor-live] requires an absolute session directory with inbox/outbox");
            return None;
        }
        Some(Self { root, last_poll: Instant::now(), last_snapshot_hash: None })
    }
}

fn vector(value: &Value, field: &str) -> anyhow::Result<Vec3> {
    let a = value.as_array().context(format!("{field}: expected three numbers"))?;
    ensure!(a.len() == 3, "{field}: expected three numbers");
    let mut v = [0.0_f32; 3];
    for i in 0..3 {
        v[i] = a[i].as_f64().context(format!("{field}: not a number"))? as f32;
        ensure!(v[i].is_finite() && v[i].abs() <= 10000.0, "{field}: invalid coordinate");
    }
    Ok(Vec3::from_array(v))
}

fn checked_tilt_edit(current: [f32; 2], expected: &Value, change: Option<&Value>) -> anyhow::Result<[f32; 2]> {
    if let Some(expected) = expected.get("tilt_deg") {
        let inspected = crate::assetlib::parse_tilt_deg(Some(expected), "expected.tilt_deg")?;
        ensure!((current[0]-inspected[0]).abs() < 0.0001 && (current[1]-inspected[1]).abs() < 0.0001,
            "placement tilt changed since inspection; preserving the user's edit");
    } else {
        ensure!(change.is_none(), "changing tilt requires expected.tilt_deg from a fresh snapshot");
    }
    match change {
        Some(value) => crate::assetlib::parse_tilt_deg(Some(value), "tilt_deg"),
        None => Ok(current),
    }
}

fn slug(value: &Value) -> anyhow::Result<&str> {
    let name = value.as_str().context("name required")?;
    ensure!(!name.is_empty() && name.len() < 100 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'), "name must be a short filename slug");
    Ok(name)
}

fn asset_path(value: &Value) -> anyhow::Result<&str> {
    let path = value.as_str().context("asset required")?;
    ensure!(path.starts_with("assets/") && path.ends_with(".glb") && !path.contains('\\')
        && Path::new(path).components().all(|c| matches!(c, std::path::Component::Normal(_))), "expected a repository asset GLB");
    Ok(path)
}

fn atomic_json(path: &Path, value: &Value) -> anyhow::Result<()> {
    let pending = path.with_extension("pending");
    std::fs::write(&pending, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(pending, path)?;
    Ok(())
}

impl EditorState {
    pub(super) fn apply_display_tuning(&mut self, tuning: crate::renderer::display_tuning::DisplayTuning) -> anyhow::Result<()> {
        ensure!(!self.world.is_asset_yard, "display profiles belong to an editable world");
        self.renderer.resize_shader_resolution(&self.device, &self.queue, (tuning.width,tuning.height))?;
        self.renderer.display_tuning = Some(tuning);
        self.world.display_tuning = Some(tuning);
        self.apply_lighting_preview();
        self.dirty = true;
        self.status = format!("Display {}x{} · {} levels · dither {:.0}% · F3 pixels / F4 dither / F5 save",
            tuning.width,tuning.height,tuning.palette_levels,tuning.ordered_dither*100.0);
        Ok(())
    }

    pub(super) fn poll_live_authoring(&mut self) {
        let Some(mut live) = self.live_authoring.take() else { return; };
        if live.last_poll.elapsed().as_millis() < 400 {
            self.live_authoring = Some(live);
            return;
        }
        live.last_poll = Instant::now();
        let poll_started = Instant::now();
        let mut commanded = false;
        // Wait until the user's gesture is finished. Remote additions never steal a selection.
        let busy = !matches!(self.mode, Mode::Fly) || self.drag.is_some() || self.painting || self.walkthrough;
        if !busy && !self.world.is_asset_yard && self.world.garage.is_none() {
            let mut requests = std::fs::read_dir(live.root.join("inbox")).into_iter().flatten()
                .filter_map(Result::ok).map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json")).collect::<Vec<_>>();
            requests.sort();
            if let Some(path) = requests.first() {
                // Claim before execution: a crash leaves a .processing file for manual review,
                // never an automatically replayed batch that could duplicate hundreds of props.
                let claimed = path.with_extension("processing");
                if std::fs::rename(path, &claimed).is_ok() {
                    commanded = true;
                    let result = (|| -> anyhow::Result<Value> {
                        ensure!(std::fs::metadata(&claimed)?.len() <= 4 * 1024 * 1024, "request too large");
                        let command: Value = serde_json::from_slice(&std::fs::read(&claimed)?)?;
                        self.live_command(&command)
                    })();
                    let reply = match result {
                        Ok(value) => json!({"ok":true,"result":value}),
                        Err(error) => { self.status = format!("Live authoring: {error:#}"); json!({"ok":false,"error":format!("{error:#}")}) }
                    };
                    let output = live.root.join("outbox").join(path.file_name().unwrap());
                    if atomic_json(&output, &reply).is_ok() { let _ = std::fs::remove_file(claimed); }
                }
            }
        }
        // A large map previously reallocated/serialized every placement every 400 ms even while
        // untouched. Hash the compact authored values, and publish the full scene only on change
        // or request. A small independent heartbeat always reports current busy/camera state.
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for p in &self.world.placements {
            p.asset.hash(&mut hash); p.deleted.hash(&mut hash);
            for n in [p.pos.x,p.pos.y,p.pos.z,p.scale.x,p.scale.y,p.scale.z,p.rot_deg] {
                n.to_bits().hash(&mut hash);
            }
        }
        self.world.assets.hash(&mut hash);
        self.sel.hash(&mut hash);
        self.has_unsaved_changes().hash(&mut hash);
        json!(crate::environment_authoring::lights_to_json(&self.world.authored_lights)).to_string().hash(&mut hash);
        self.world.lighting.to_json().to_string().hash(&mut hash);
        let fingerprint = hash.finish();
        let changed = commanded || live.last_snapshot_hash != Some(fingerprint);
        if changed {
        let placements: Vec<Value> = self.world.placements.iter().enumerate().map(|(index,p)| json!({
            "index":index,"asset":self.world.assets[p.asset],"position":p.pos.to_array(),
            "scale":p.scale.to_array(),"yaw":p.rot_deg,"tilt_deg":p.tilt_deg,"deleted":p.deleted
        })).collect();
        let snapshot = json!({"schema":"panzer.editor-live.v1","manifest":self.world.manifest_path,
            "busy":busy,"dirty":self.has_unsaved_changes(),"status":self.status,
            "camera":self.renderer.camera.pos.to_array(),"selected":self.sel,"placements":placements,
            "assets":self.world.assets.iter().zip(&self.asset_meshes).filter_map(|(path,mesh)| mesh.as_ref().map(|mesh|
                json!({"path":path,"min":mesh.min.to_array(),"max":mesh.max.to_array()}))).collect::<Vec<_>>(),
            "lights":crate::environment_authoring::lights_to_json(&self.world.authored_lights)});
        if atomic_json(&live.root.join("state.json"), &snapshot).is_ok() {
            live.last_snapshot_hash = Some(fingerprint);
        }
        }
        let _ = atomic_json(&live.root.join("heartbeat.json"), &json!({
            "manifest":self.world.manifest_path,"busy":busy,"dirty":self.has_unsaved_changes(),
            "camera":self.renderer.camera.pos.to_array(),"status":self.status,
            "scene_changed":changed,"poll_cpu_ms":poll_started.elapsed().as_secs_f64()*1000.0,
            "scene_fingerprint":fingerprint,
            "display":self.renderer.display_tuning.map(|p|p.to_json())
        }));
        self.live_authoring = Some(live);
    }

    fn live_command(&mut self, command: &Value) -> anyhow::Result<Value> {
        match command["op"].as_str().context("op required")? {
            "snapshot" => Ok(json!({"path":"state.json"})),
            "display" => {
                let tuning = crate::renderer::display_tuning::DisplayTuning::from_json(&command["display_tuning"])?;
                self.apply_display_tuning(tuning)?;
                Ok(tuning.to_json())
            }
            "render_stats" => {
                let (prototypes,buffers,pages,instances) = self.renderer.instanced_paging_stats();
                let (cells,references,fallbacks) = self.renderer.instanced_spatial_stats();
                let work = self.renderer.render_work_stats();
                Ok(json!({"prototypes":prototypes,"instance_buffers":buffers,"pages":pages,
                    "instances":instances,"spatial_cells":cells,"cell_references":references,
                    "fallback_pages":fallbacks,"scene_renders":work.scene_renders,
                    "scene_instanced_draws":work.scene_instanced_draws,"scene_mesh_draws":work.scene_mesh_draws,
                    "shadow_instanced_draws":work.shadow_instanced_draws,"shadow_mesh_draws":work.shadow_mesh_draws,
                    "display":self.renderer.display_tuning.map(|p|p.to_json()),
                    "camera":self.renderer.camera.pos.to_array(),
                    "note":"Actual encoded work counters; not CPU or GPU timings"}))
            }
            "guides" => {
                self.authoring_guides_visible = command["visible"].as_bool().context("visible required")?;
                self.refresh_markers();
                Ok(json!({"visible":self.authoring_guides_visible}))
            }
            "edit" => {
                let edits = command["items"].as_array().context("items required")?;
                ensure!(!edits.is_empty() && edits.len() <= 512, "edit batch must contain 1..512 items");
                let mut pending = Vec::new();
                let mut seen = BTreeSet::new();
                for edit in edits {
                    let index = edit["index"].as_u64().context("index required")? as usize;
                    ensure!(seen.insert(index), "duplicate edited index");
                    let mut placement = self.world.placements.get(index).context("unknown index")?.clone();
                    let expected = &edit["expected"];
                    placement.tilt_deg = checked_tilt_edit(placement.tilt_deg, expected, edit.get("tilt_deg"))?;
                    ensure!(!placement.deleted && expected["asset"].as_str() == Some(self.world.assets[placement.asset].as_str())
                        && placement.pos.distance(vector(&expected["position"], "expected.position")?) < 0.0001
                        && placement.scale.distance(vector(&expected["scale"], "expected.scale")?) < 0.0001
                        && (placement.rot_deg-expected["yaw"].as_f64().context("expected.yaw required")? as f32).abs() < 0.0001,
                        "placement {index} changed since inspection; preserving the user's edit");
                    let old_asset = placement.asset;
                    if let Some(asset) = edit.get("asset") {
                        placement.asset = self.ensure_world_asset(asset_path(asset)?).context("replacement asset unavailable")?;
                    }
                    if let Some(value) = edit.get("position") { placement.pos = vector(value, "position")?; }
                    if let Some(value) = edit.get("scale") {
                        placement.scale = vector(value, "scale")?;
                        ensure!(placement.scale.min_element() >= 0.001 && placement.scale.max_element() <= 1000.0, "invalid scale");
                    }
                    if let Some(value) = edit.get("yaw") {
                        placement.rot_deg = value.as_f64().context("invalid yaw")? as f32;
                        ensure!(placement.rot_deg.is_finite(), "invalid yaw");
                    }
                    if let Some(value) = edit.get("deleted") { placement.deleted = value.as_bool().context("deleted must be boolean")?; }
                    pending.push((index,old_asset,placement));
                }
                let mut touched = Vec::new();
                for (index,old,placement) in pending {
                    touched.extend([old,placement.asset]);
                    self.world.placements[index] = placement;
                }
                self.sel.retain(|&index| !self.world.placements[index].deleted);
                self.refresh_assets(touched);
                self.dirty = true;
                Ok(json!({"updated":edits.len()}))
            }
            "lighting" => {
                let tuning = crate::environment_authoring::LightingTuning::from_edits(Some(command))?;
                ensure!(tuning.enabled, "enabled lighting tuning required");
                self.world.lighting = tuning;
                self.apply_lighting_preview();
                self.dirty = true;
                Ok(json!({"lighting":tuning.to_json()}))
            }
            "lights" => {
                let lights = crate::environment_authoring::lights_from_edits(Some(command))?;
                ensure!(lights.len() <= 32, "at most 32 practical sources per batch");
                for light in &lights {
                    ensure!(!self.world.authored_lights.iter().any(|p| p.id == light.id), "light id already exists");
                }
                let count = lights.len();
                self.world.authored_lights.extend(lights);
                self.refresh_markers();
                self.apply_lighting_preview();
                self.dirty = true;
                Ok(json!({"added":count}))
            }
            "transform" => {
                let index = command["index"].as_u64().context("index required")? as usize;
                let expected = &command["expected"];
                let position = vector(&command["position"], "position")?;
                let scale = vector(&command["scale"], "scale")?;
                ensure!(scale.min_element() >= 0.001 && scale.max_element() <= 1000.0, "invalid scale");
                let yaw = command["yaw"].as_f64().context("yaw required")? as f32;
                ensure!(yaw.is_finite(), "invalid yaw");
                let current = self.world.placements.get_mut(index).context("unknown index")?;
                let tilt = checked_tilt_edit(current.tilt_deg, expected, command.get("tilt_deg"))?;
                ensure!(!current.deleted && current.pos.distance(vector(&expected["position"], "expected.position")?) < 0.0001
                    && current.scale.distance(vector(&expected["scale"], "expected.scale")?) < 0.0001
                    && (current.rot_deg - expected["yaw"].as_f64().context("expected.yaw required")? as f32).abs() < 0.0001,
                    "placement changed since inspection; preserving the user's edit");
                current.pos = position;
                current.scale = scale;
                current.rot_deg = yaw;
                current.tilt_deg = tilt;
                let asset = current.asset;
                self.refresh_asset(asset);
                self.dirty = true;
                Ok(json!({"index":index}))
            }
            "add" => {
                let items = command["items"].as_array().context("items required")?;
                ensure!(!items.is_empty() && items.len() <= 512, "batch must contain 1..512 props");
                let mut pending = Vec::new();
                // Validate the whole batch before changing placements. Cached prototypes are safe
                // to retain on an error; they add no visible geometry to the world.
                for item in items {
                    let path = asset_path(&item["asset"])?;
                    let pos = vector(&item["position"], "position")?;
                    let scale = vector(&item["scale"], "scale")?;
                    ensure!(scale.min_element() >= 0.001 && scale.max_element() <= 1000.0, "invalid scale");
                    let yaw = item["yaw"].as_f64().context("yaw required")? as f32;
                    ensure!(yaw.is_finite(), "invalid yaw");
                    let asset = self.ensure_world_asset(path).context(format!("load asset {path}"))?;
                    let tilt_deg = crate::assetlib::parse_tilt_deg(item.get("tilt_deg"), "tilt_deg")?;
                    pending.push(Placement { asset, pos, scale, rot_deg:yaw, tilt_deg, deleted:false });
                }
                let first = self.world.placements.len();
                let touched = pending.iter().map(|p| p.asset).collect::<Vec<_>>();
                self.world.placements.extend(pending);
                self.refresh_assets(touched);
                self.dirty = true;
                self.status = format!("Live build: {} · {} pieces added · all pieces editable", command["label"].as_str().unwrap_or("assembly"), items.len());
                Ok(json!({"indices":(first..self.world.placements.len()).collect::<Vec<_>>() }))
            }
            "camera" => {
                let eye = vector(&command["eye"], "eye")?;
                let target = vector(&command["target"], "target")?;
                ensure!(eye.distance(target) > 0.1, "camera eye and target must differ");
                self.renderer.camera.pos = eye;
                self.renderer.camera.set_forward((target-eye).normalize());
                if command["view"].as_str() == Some("walk") {
                    self.walk_camera.feet = eye - Vec3::Y * self.walk_camera.eye_height;
                    self.walk_camera.yaw = self.renderer.camera.yaw;
                    self.walk_camera.pitch = self.renderer.camera.pitch;
                    self.view_mode = crate::editor_navigation::EditorViewMode::Walk;
                } else {
                    self.orbit_camera.pivot = target;
                    self.orbit_camera.set_distance(eye.distance(target));
                    self.orbit_camera.yaw = self.renderer.camera.yaw;
                    self.orbit_camera.pitch = self.renderer.camera.pitch;
                    self.view_mode = crate::editor_navigation::EditorViewMode::Orbit;
                }
                self.release_editor_pointer();
                Ok(json!({"eye":eye.to_array(),"target":target.to_array()}))
            }
            "capture" => {
                let path = PathBuf::from("captures/astra").join(format!("{}.png",slug(&command["name"])?));
                ensure!(!path.exists(), "capture already exists");
                std::fs::create_dir_all(path.parent().unwrap())?;
                self.smoke_capture_error = None;
                self.smoke_capture_path = Some(path.clone());
                self.render();
                ensure!(self.smoke_capture_path.is_none(), "capture did not complete");
                ensure!(self.smoke_capture_error.is_none(), "capture failed: {:?}", self.smoke_capture_error);
                Ok(json!({"path":path.canonicalize()?}))
            }
            "save" => {
                ensure!(!self.manifest_save_conflicted(), "manifest changed; resolve in editor first");
                ensure!(self.terrain_dirty_bounds.is_none(), "capture terrain with F7 first");
                let count = self.world.save_edits()?;
                self.dirty = false;
                self.status = "Saved live assembly and your world edits".into();
                Ok(json!({"saved":count,"manifest":self.world.manifest_path}))
            }
            "export_prefab" => {
                let name = slug(&command["name"])?;
                let anchor = vector(&command["anchor"], "anchor")?;
                let indices = command["indices"].as_array().context("indices required")?;
                ensure!(!indices.is_empty(), "empty prefab");
                let mut seen = BTreeSet::new();
                let mut items = Vec::new();
                for index in indices {
                    let index = index.as_u64().context("invalid placement index")? as usize;
                    ensure!(seen.insert(index), "duplicate index");
                    let p = self.world.placements.get(index).context("unknown placement index")?;
                    if p.deleted { continue; }
                    items.push(json!({"asset":self.world.assets[p.asset],"dx":p.pos.x-anchor.x,
                        "dz":p.pos.z-anchor.z,"hoff":p.pos.y-anchor.y,"deg":p.rot_deg,"tilt_deg":p.tilt_deg,"s":p.scale.to_array()}));
                }
                ensure!(!items.is_empty(), "all prefab pieces were deleted");
                let path = PathBuf::from(crate::assetlib::PREFAB_DIR).join(format!("{name}.json"));
                ensure!(!path.exists(), "prefab exists; use a new revision name");
                std::fs::create_dir_all(path.parent().unwrap())?;
                let light_ids = command["light_ids"].as_array().cloned().unwrap_or_default();
                let lights = self.world.authored_lights.iter().filter(|light| !light.deleted && light_ids.iter().any(|id| id.as_str() == Some(light.id.as_str())))
                    .cloned().map(|mut light| { light.position -= anchor; light }).collect::<Vec<_>>();
                atomic_json(&path, &json!({"name":name,"grounding":"rigid_anchor","items":items,
                    "environment":{"authored_lights":crate::environment_authoring::lights_to_json(&lights)},
                    "metadata":{"authoring":"editable design study","historical_reconstruction":false}}))?;
                self.browser.rescan();
                Ok(json!({"path":path.canonicalize()?,"pieces":items.len()}))
            }
            other => anyhow::bail!("unsupported live operation {other}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_tilt_live_edits_preserve_legacy_fields_and_check_concurrent_changes() {
        let current=[12.0,-7.0];
        assert_eq!(checked_tilt_edit(current,&json!({}),None).unwrap(),current);
        assert!(checked_tilt_edit(current,&json!({}),Some(&json!([1,2]))).is_err());
        assert!(checked_tilt_edit(current,&json!({"tilt_deg":[0,0]}),Some(&json!([1,2]))).is_err());
        assert_eq!(checked_tilt_edit(current,&json!({"tilt_deg":current}),Some(&json!([1,2]))).unwrap(),[1.0,2.0]);
    }
    #[test]
    fn mailbox_rejects_escaping_paths_and_bad_vectors() {
        assert!(asset_path(&json!("assets/../secret.glb")).is_err());
        assert!(asset_path(&json!("/tmp/model.glb")).is_err());
        assert!(asset_path(&json!("assets/pizza/wall.glb")).is_ok());
        assert!(slug(&json!("../capture")).is_err());
        assert!(vector(&json!([1,2]), "position").is_err());
        assert!(vector(&json!([1,2,1e100]), "position").is_err());
    }
}
