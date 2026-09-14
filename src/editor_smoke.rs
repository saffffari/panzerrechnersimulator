//! Opt-in method-level editor acceptance. Uses the real EditorState and renderer, with a temporary
//! world or garage document. This is deliberately not a claim that native mouse/window input was replayed.

use super::*;
use anyhow::{bail, ensure, Context};
use serde_json::{json, Value};

const REVIEW_PROP: &str =
    "assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked/enigma_machine_runtime.glb";

fn checked(checks: &mut Vec<Value>, name: &str, pass: bool, details: Value) -> anyhow::Result<()> {
    checks.push(json!({"name":name,"pass":pass,"details":details}));
    ensure!(pass, "editor smoke assertion failed: {name}");
    Ok(())
}

fn optional_bytes(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("read protected document {}", path.display()))
        }
    }
}

fn output_base(prefix: &str, document: &str) -> anyhow::Result<PathBuf> {
    let folder = std::env::current_dir()?.join("captures/astra");
    std::fs::create_dir_all(&folder)?;
    let folder = folder.canonicalize()?;
    let requested = Path::new(prefix);
    let leaf = requested
        .file_name()
        .and_then(|name| name.to_str())
        .context("smoke prefix has no file name")?;
    ensure!(
        !leaf.is_empty() && leaf != "." && leaf != "..",
        "invalid smoke prefix"
    );
    if let Some(parent) = requested.parent().filter(|p| !p.as_os_str().is_empty()) {
        ensure!(
            parent.canonicalize()? == folder,
            "editor smoke output must be flat in captures/astra"
        );
    }
    Ok(folder.join(format!("{leaf}_{document}")))
}

fn output_path(base: &Path, suffix: &str) -> PathBuf {
    base.with_file_name(format!(
        "{}{suffix}",
        base.file_name().unwrap().to_string_lossy()
    ))
}

/// Copy only document-owned files. Model, texture and scene-block asset references deliberately
/// retain their normal resolver paths, while the relative heightfield lives beside the copy.
fn copy_world_document(source: &Path, temporary: &Path) -> anyhow::Result<PathBuf> {
    let bytes = crate::resource::read(source)?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    let heights = Path::new(
        manifest["terrain"]["heights"]
            .as_str()
            .context("terrain.heights missing")?,
    );
    ensure!(
        !heights.as_os_str().is_empty()
            && heights
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
        "unsafe relative terrain path"
    );
    let destination = temporary.join("manifest.v1.json");
    let terrain_destination = temporary.join(heights);
    std::fs::create_dir_all(
        terrain_destination
            .parent()
            .context("heightfield parent missing")?,
    )?;
    let terrain_source = source
        .parent()
        .context("manifest parent missing")?
        .join(heights);
    std::fs::write(
        &terrain_destination,
        crate::resource::read(&terrain_source)?,
    )?;
    std::fs::write(&destination, bytes)?;
    let edits_source = source.with_file_name("manifest_edits.json");
    if crate::resource::is_file(&edits_source) {
        std::fs::write(
            temporary.join("manifest_edits.json"),
            crate::resource::read(&edits_source)?,
        )?;
    }
    Ok(destination)
}

impl EditorState {
    pub(super) fn run_editor_smoke(&mut self, prefix: &str) -> anyhow::Result<()> {
        if self.world.garage.is_none() {
            return self.run_world_editor_smoke(prefix);
        }
        let tank = self
            .world
            .garage
            .as_ref()
            .context("editor smoke requires an isolated tank garage")?
            .tank;
        let base = output_base(prefix, tank.as_str())?;
        let receipt_path = output_path(&base, "_receipt.json");
        ensure!(
            !receipt_path.exists(),
            "smoke receipt already exists: {}",
            receipt_path.display()
        );
        let live_scene = crate::tank_editor_scene::default_scene_path(tank);
        let live_dressing = PathBuf::from("assets/tanks/tiger/dressing.v1.json");
        let scene_before = optional_bytes(&live_scene)?;
        let dressing_before = optional_bytes(&live_dressing)?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let temporary = std::env::temp_dir().join(format!(
            "panzer-editor-smoke-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&temporary)?;
        let temporary_scene = temporary.join("scene.v1.json");
        crate::tank_editor_scene::save(&temporary_scene, self.world.garage.as_ref().unwrap())?;
        self.world.manifest_path = temporary_scene.clone();
        self.manifest_mtime = std::fs::metadata(&temporary_scene)?.modified().ok();
        self.manifest_conflict = false;
        self.browser.open = false;
        // A hidden window does not own native mouse capture. The method-level test uses the
        // center frustum ray explicitly and records that distinction in its receipt.
        self.release_editor_pointer();
        self.cursor = [0.5, 0.5];
        let mut checks = Vec::new();
        let mut captures = Vec::new();
        let result = self.smoke_sequence(&base, &mut checks, &mut captures);
        let protected_unchanged = optional_bytes(&live_scene)? == scene_before
            && optional_bytes(&live_dressing)? == dressing_before;
        checks.push(
            json!({"name":"live_documents_unchanged","pass":protected_unchanged,
            "details":{"garage":live_scene,"tiger_dressing":live_dressing}}),
        );
        let pass = result.is_ok() && protected_unchanged;
        let receipt = json!({
            "schema":"kessel.editor-method-smoke","version":1,"tank":tank.as_str(),"pass":pass,
            "input_route":"Actual EditorState methods and frame update with centered edit ray; not native mouse/keyboard replay",
            "temporary_scene":temporary_scene,"checks":checks,"captures":captures,
            "error":result.as_ref().err().map(|error|format!("{error:#}")),
            "limits":["Does not validate native pointer capture/focus on a visible window",
                "Does not spawn full Tiger gameplay; the root Game factory review owns that check",
                "Imported static tanks remain geometry walkthroughs, without invented crew controls"]
        });
        std::fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
        println!(
            "[editor-smoke] {} {}",
            if pass { "PASS" } else { "FAIL" },
            receipt_path.display()
        );
        ensure!(
            protected_unchanged,
            "smoke changed a protected live document"
        );
        result
    }

    fn run_world_editor_smoke(&mut self, prefix: &str) -> anyhow::Result<()> {
        ensure!(
            !self.world.is_asset_yard,
            "world smoke requires an Act document"
        );
        let base = output_base(prefix, "world")?;
        let receipt_path = output_path(&base, "_receipt.json");
        ensure!(!receipt_path.exists(), "smoke receipt already exists");
        let live_manifest = self.world.manifest_path.clone();
        let protected = [
            live_manifest.clone(),
            self.world.edits_path(),
            PathBuf::from("assets/tanks/tiger/dressing.v1.json"),
        ];
        let original = protected
            .iter()
            .map(|path| optional_bytes(path))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let temporary = std::env::temp_dir().join(format!(
            "panzer-world-editor-smoke-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&temporary)?;
        let temporary_manifest = copy_world_document(&live_manifest, &temporary)?;
        self.world.manifest_path = temporary_manifest.clone();
        self.manifest_mtime = std::fs::metadata(&temporary_manifest)?.modified().ok();
        self.manifest_conflict = false;
        self.browser.open = false;
        self.release_editor_pointer();
        self.cursor = [0.5, 0.5];
        let mut checks = Vec::new();
        let mut captures = Vec::new();
        let result = self.smoke_world_sequence(&base, &mut checks, &mut captures);
        let protected_unchanged = protected
            .iter()
            .zip(original)
            .map(|(path, before)| optional_bytes(path).map(|after| after == before))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .all(|unchanged| unchanged);
        checks.push(json!({"name":"live_documents_unchanged","pass":protected_unchanged,"details":{"paths":protected}}));
        let pass = result.is_ok() && protected_unchanged;
        std::fs::write(
            &receipt_path,
            serde_json::to_vec_pretty(&json!({
                "schema":"kessel.editor-method-smoke","version":1,"document":"world","pass":pass,
                "input_route":"Actual EditorState navigation, placement, pick, transform and reload methods; not native input replay",
                "save_route":"EditorWorld::save_edits (actual world save component); full save_document deliberately excluded because it also saves live Tiger dressing",
                "source_manifest":live_manifest,"temporary_scene":temporary_manifest,"checks":checks,"captures":captures,
                "error":result.as_ref().err().map(|error|format!("{error:#}")),
                "limits":["No native pointer/focus acceptance", "No gameplay child is spawned", "Only the world document save component is exercised"]
            }))?,
        )?;
        println!(
            "[editor-smoke] {} {}",
            if pass { "PASS" } else { "FAIL" },
            receipt_path.display()
        );
        ensure!(
            protected_unchanged,
            "smoke changed a protected live document"
        );
        result
    }

    fn smoke_world_sequence(
        &mut self,
        base: &Path,
        checks: &mut Vec<Value>,
        captures: &mut Vec<Value>,
    ) -> anyhow::Result<()> {
        use crate::editor_navigation::{EditorViewMode, WALK_EYE_HEIGHT};
        self.smoke_frame(&mut EKeys::default());
        self.smoke_capture(output_path(base, "_orbit.png"), captures)?;
        // This fixture seeds a clear nearby inspection position, then uses the actual F and W
        // routes. Picking remains the ordinary centered ray, and no source object is hidden.
        let mut seed = None;
        for radius in [12.0, 20.0, 32.0] {
            for spoke in 0..12 {
                let angle = spoke as f32 * std::f32::consts::TAU / 12.0;
                let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
                let feet = self.tiger.pos + outward * radius;
                let feet = Vec3::new(feet.x, self.world.ground(feet.x, feet.z), feet.z);
                let target = feet + outward * 2.5;
                let target = Vec3::new(target.x, self.world.ground(target.x, target.z), target.z);
                self.renderer.camera.pos = feet + Vec3::Y * WALK_EYE_HEIGHT;
                self.renderer
                    .camera
                    .set_forward((target - self.renderer.camera.pos).normalize());
                if self.pick(self.cursor_ray()).is_none()
                    && self.ray_tiger_surface(self.cursor_ray()).is_none()
                    && self
                        .world
                        .ray_ground(self.renderer.camera.pos, self.cursor_ray())
                        .is_some()
                {
                    seed = Some(feet);
                    break;
                }
            }
            if seed.is_some() {
                break;
            }
        }
        let seed = seed.context("no unobstructed nearby world placement seed")?;
        self.toggle_walk_orbit();
        self.smoke_frame(&mut EKeys::default());
        let before_walk = self.walk_camera.feet;
        for _ in 0..6 {
            self.smoke_frame(&mut EKeys {
                w: true,
                ..EKeys::default()
            });
        }
        checked(
            checks,
            "actual_grounded_walk",
            self.view_mode == EditorViewMode::Walk
                && self.walk_camera.feet.distance(before_walk) > 0.05
                && (self.walk_camera.feet.y
                    - self
                        .world
                        .ground(self.walk_camera.feet.x, self.walk_camera.feet.z))
                .abs()
                    < 0.0001
                && (self.renderer.camera.pos.y - self.walk_camera.feet.y - WALK_EYE_HEIGHT).abs()
                    < 0.0005,
            json!({"seed_feet":seed.to_array(),"walked_m":self.walk_camera.feet.distance(before_walk),"eye":self.renderer.camera.pos.to_array()}),
        )?;
        let ground_hit = self
            .world
            .ray_ground(self.renderer.camera.pos, self.cursor_ray())
            .context("walk crosshair misses terrain")?;
        checked(
            checks,
            "world_crosshair_targets_ground",
            self.ray_tiger_surface(self.cursor_ray()).is_none(),
            json!({"ground_hit":ground_hit.to_array()}),
        )?;
        let before_props = self.dressing.to_value()?;
        let count = self.world.placements.len();
        let lib_idx = self.smoke_prop_library_index()?;
        self.place_library_asset_for_view(lib_idx);
        checked(
            checks,
            "actual_world_model_placement",
            self.world.placements.len() == count + 1
                && self.dressing.to_value()? == before_props
                && self.sel_tank_prop.is_none(),
            json!({"source":REVIEW_PROP,"status":self.status}),
        )?;
        let index = *self.sel.last().context("world placement not selected")?;
        let before = self.world.placements[index].clone();
        checked(
            checks,
            "placement_lands_at_crosshair",
            before.pos.distance(ground_hit) < 0.005,
            json!({"error_m":before.pos.distance(ground_hit)}),
        )?;
        let mesh = self.asset_meshes[before.asset]
            .as_ref()
            .context("world prop source mesh missing")?;
        let center = before.model().transform_point3((mesh.min + mesh.max) * 0.5);
        self.smoke_face(center);
        self.smoke_frame(&mut EKeys::default());
        let picked = self.pick(self.cursor_ray());
        checked(
            checks,
            "world_prop_is_pickable",
            picked == Some(Picked::Placement(index)),
            json!({"picked":format!("{picked:?}"),"expected_index":index}),
        )?;
        self.sel = vec![index];
        self.enter_mode(Mode::Grab);
        self.smoke_frame(&mut EKeys {
            wheel: 0.5,
            ..EKeys::default()
        });
        self.smoke_confirm_transform();
        let after = &self.world.placements[index];
        checked(
            checks,
            "grab_moves_world_prop",
            after.pos.distance(before.pos) > 0.1,
            json!({"before":before.pos.to_array(),"after":after.pos.to_array()}),
        )?;
        let expected: Vec<_> = self
            .world
            .placements
            .iter()
            .filter(|p| !p.deleted)
            .map(|p| (self.world.assets[p.asset].clone(), p.clone()))
            .collect();
        // The real world save implementation is isolated here. Full save_document has an
        // additional live Tiger-dressing destination and must never be called from this seam.
        let operations = self.world.save_edits()?;
        self.dirty = false;
        checked(
            checks,
            "temporary_world_edit_overlay_saved",
            self.world.edits_path().is_file() && operations > 0,
            json!({"path":self.world.edits_path(),"operations":operations}),
        )?;
        self.smoke_capture(output_path(base, "_edited.png"), captures)?;
        let temporary_path = self.world.manifest_path.clone();
        self.reload();
        checked(
            checks,
            "reload_completed",
            self.status == "reloaded",
            json!({"status":self.status}),
        )?;
        self.smoke_frame(&mut EKeys::default());
        let actual: Vec<_> = self
            .world
            .placements
            .iter()
            .filter(|p| !p.deleted)
            .collect();
        let equal = actual.len() == expected.len()
            && actual.iter().zip(&expected).all(|(a, (path, b))| {
                self.world.assets[a.asset] == *path
                    && a.pos.distance(b.pos) < 0.0001
                    && a.scale.distance(b.scale) < 0.0001
                    && (a.rot_deg - b.rot_deg).abs() < 0.0001
            });
        checked(
            checks,
            "world_placements_survive_reload",
            equal && self.world.manifest_path == temporary_path,
            json!({"expected_count":expected.len(),"actual_count":actual.len(),"manifest":self.world.manifest_path}),
        )?;
        checked(
            checks,
            "world_reload_preserves_walk_and_tank_dressing",
            self.view_mode == EditorViewMode::Walk
                && self.dressing.to_value()? == before_props
                && self.play_child.is_none(),
            json!({"child_spawned":self.play_child.is_some()}),
        )?;
        self.smoke_capture(output_path(base, "_reloaded.png"), captures)?;
        Ok(())
    }

    fn smoke_prop_library_index(&self) -> anyhow::Result<usize> {
        self
            .browser
            .lib
            .iter()
            .position(|entry| entry.path == REVIEW_PROP)
            .context("Enigma is missing from the actual user-facing asset library")
    }

    fn smoke_frame(&mut self, keys: &mut EKeys) {
        self.release_editor_pointer();
        self.frame(keys, 1.0 / 60.0);
    }

    fn smoke_capture(&mut self, path: PathBuf, captures: &mut Vec<Value>) -> anyhow::Result<()> {
        ensure!(
            !path.exists(),
            "smoke capture already exists: {}",
            path.display()
        );
        self.smoke_capture_error = None;
        self.smoke_capture_path = Some(path.clone());
        self.render();
        ensure!(
            self.smoke_capture_path.is_none(),
            "hidden editor surface was unavailable for capture"
        );
        if let Some(error) = self.smoke_capture_error.take() {
            bail!("editor HUD capture failed: {error}");
        }
        ensure!(path.is_file(), "editor HUD capture was not written");
        captures.push(json!({"path":path,"includes_editor_hud":true}));
        Ok(())
    }

    fn smoke_face(&mut self, target: Vec3) {
        let direction = (target - self.renderer.camera.pos).normalize_or_zero();
        self.renderer.camera.set_forward(direction);
        match self.view_mode {
            crate::editor_navigation::EditorViewMode::Tiger => {
                let local = self
                    .tiger_seat
                    .parent_model(&self.tiger)
                    .inverse()
                    .transform_vector3(direction)
                    .normalize_or_zero();
                self.tiger_look =
                    Vec2::new((-local.x).atan2(local.z), local.y.clamp(-1.0, 1.0).asin());
            }
            crate::editor_navigation::EditorViewMode::Walk => {
                self.walk_camera.yaw = self.renderer.camera.yaw;
                self.walk_camera.pitch = self.renderer.camera.pitch;
            }
            crate::editor_navigation::EditorViewMode::Orbit => {}
        }
    }

    fn smoke_confirm_transform(&mut self) {
        // The same state transition as ordinary LMB confirmation in window_event.
        self.mode = Mode::Fly;
        self.mode_undo = ModeUndo::default();
    }

    fn smoke_sequence(
        &mut self,
        base: &Path,
        checks: &mut Vec<Value>,
        captures: &mut Vec<Value>,
    ) -> anyhow::Result<()> {
        use crate::editor_navigation::EditorViewMode;
        let tank = self.world.garage.as_ref().unwrap().tank;
        self.smoke_frame(&mut EKeys::default());
        self.smoke_capture(output_path(base, "_orbit.png"), captures)?;
        self.toggle_tiger_view();
        self.smoke_frame(&mut EKeys::default());
        checked(
            checks,
            "inside_navigation",
            self.view_mode
                == if tank == crate::tank_editor_scene::TankAssetId::TigerI {
                    EditorViewMode::Tiger
                } else {
                    EditorViewMode::Walk
                },
            json!({"view":format!("{:?}",self.view_mode),"eye":self.renderer.camera.pos.to_array(),"status":self.status}),
        )?;
        self.smoke_capture(output_path(base, "_inside.png"), captures)?;
        let hit = self.ray_tiger_surface(self.cursor_ray());
        checked(
            checks,
            "center_ray_hits_source_geometry",
            hit.is_some(),
            json!({"distance_m":hit.map(|hit|hit.distance),"frame":hit.map(|hit|hit.frame.as_str())}),
        )?;
        let source_hit = hit.unwrap();
        let lib_idx = self.smoke_prop_library_index()?;
        let before_count = self.dressing.props.len();
        self.place_library_asset_for_view(lib_idx);
        checked(
            checks,
            "actual_model_placement",
            self.dressing.props.len() == before_count + 1,
            json!({"source":REVIEW_PROP,"status":self.status}),
        )?;
        let index = self.sel_tank_prop.context("placed prop was not selected")?;
        let prop = self.dressing.props[index].clone();
        checked(
            checks,
            "placement_inherits_hit_parent",
            prop.frame == source_hit.frame,
            json!({"expected":source_hit.frame.as_str(),"actual":prop.frame.as_str()}),
        )?;
        let asset = self.dressing_assets[index].context("placed prop has no loaded asset")?;
        let mesh = self.asset_meshes[asset]
            .as_ref()
            .context("placed prop has no source mesh")?;
        let center = prop
            .visual_world_model(&self.tiger)
            .transform_point3((mesh.min + mesh.max) * 0.5);
        self.smoke_face(center);
        self.smoke_frame(&mut EKeys::default());
        let picked = self.pick(self.cursor_ray());
        checked(
            checks,
            "placed_prop_is_pickable",
            picked == Some(Picked::TankProp(index)),
            json!({"picked":format!("{picked:?}"),"expected_index":index}),
        )?;
        self.sel_tank_prop = Some(index);
        self.enter_mode(Mode::Grab);
        let mut movement = EKeys::default();
        movement.wheel = 2.0;
        self.smoke_frame(&mut movement);
        self.smoke_confirm_transform();
        checked(
            checks,
            "grab_moves_attachment",
            self.dressing.props[index]
                .local_translation_m
                .distance(prop.local_translation_m)
                > 0.001,
            json!({"before":prop.local_translation_m.to_array(),"after":self.dressing.props[index].local_translation_m.to_array()}),
        )?;
        let lamp_lib = self
            .browser
            .lib
            .iter()
            .position(|entry| {
                entry.kind == crate::assetlib::AssetKind::Light && entry.path.ends_with("/work")
            })
            .context("work lamp missing from actual editor library")?;
        self.place_library_asset_for_view(lamp_lib);
        let light_index = self.sel_light.context("lamp was not created/selected")?;
        self.world.authored_lights[light_index].temperature_k = 4100.0;
        self.dirty = true;
        self.enter_mode(Mode::Grab);
        let mut movement = EKeys::default();
        movement.wheel = 1.0;
        self.smoke_frame(&mut movement);
        self.smoke_confirm_transform();
        self.save_document()?;
        let expected = self.world.garage.as_ref().unwrap().clone();
        let expected_light = expected
            .lights
            .iter()
            .find(|source| source.light.id == self.world.authored_lights[light_index].id)
            .context("lamp not serialized as an attachment")?
            .clone();
        checked(
            checks,
            "lamp_edit_and_parent_saved",
            expected_light.light.temperature_k == 4100.0,
            json!({"id":expected_light.light.id,"parent":expected_light.frame.as_str(),"temperature_k":expected_light.light.temperature_k}),
        )?;
        self.smoke_capture(output_path(base, "_edited.png"), captures)?;
        let temporary_path = self.world.manifest_path.clone();
        self.reload();
        checked(
            checks,
            "reload_completed",
            self.status == "reloaded",
            json!({"status":self.status}),
        )?;
        self.smoke_frame(&mut EKeys::default());
        checked(
            checks,
            "reload_uses_temporary_document",
            self.world.manifest_path == temporary_path,
            json!({"expected":temporary_path,"actual":self.world.manifest_path,"status":self.status}),
        )?;
        checked(
            checks,
            "prop_transforms_survive_reload",
            self.dressing.to_value()? == expected.dressing.to_value()?,
            json!({"prop_count":self.dressing.props.len()}),
        )?;
        let loaded_light = self
            .world
            .garage
            .as_ref()
            .unwrap()
            .lights
            .iter()
            .find(|source| source.light.id == expected_light.light.id)
            .context("saved lamp missing after reload")?;
        checked(
            checks,
            "lamp_parent_and_transform_survive_reload",
            loaded_light.frame == expected_light.frame
                && loaded_light
                    .light
                    .position
                    .distance(expected_light.light.position)
                    < 0.0001
                && loaded_light.light.temperature_k == 4100.0,
            json!({"parent":loaded_light.frame.as_str(),"local_error_m":loaded_light.light.position.distance(expected_light.light.position)}),
        )?;
        self.smoke_capture(output_path(base, "_reloaded.png"), captures)?;
        if !tank.descriptor().full_game_playable() {
            let pose = self.renderer.camera.pos;
            self.play_document()?;
            checked(
                checks,
                "walkthrough_play_preserves_chosen_pose",
                self.walkthrough
                    && self.play_child.is_none()
                    && self.walk_camera.eye_position().distance(pose) < 0.0001,
                json!({"child_spawned":self.play_child.is_some()}),
            )?;
            self.play_document()?;
            checked(
                checks,
                "walkthrough_stop_returns_to_editor",
                !self.walkthrough && self.play_child.is_none(),
                json!({}),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "panzer-editor-smoke-test-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn world_review_copy_preserves_relative_terrain_and_never_writes_source_overlay() {
        let root = scratch("copy");
        let live = root.join("live");
        let review = root.join("review");
        std::fs::create_dir_all(live.join("terrain")).unwrap();
        std::fs::create_dir(&review).unwrap();
        let source = live.join("manifest.v1.json");
        let manifest = br#"{"terrain":{"heights":"terrain/heights.npy","texture":"assets/terrain/ground.png"},"assets":["assets/props/source.glb"]}"#;
        let overlay = br#"{"ops":[],"keep_unknown_metadata":"retained"}"#;
        std::fs::write(&source, manifest).unwrap();
        std::fs::write(
            live.join("terrain/heights.npy"),
            b"exact source terrain bytes",
        )
        .unwrap();
        std::fs::write(live.join("manifest_edits.json"), overlay).unwrap();
        let copied = copy_world_document(&source, &review).unwrap();
        assert_eq!(std::fs::read(copied).unwrap(), manifest);
        assert_eq!(
            std::fs::read(review.join("terrain/heights.npy")).unwrap(),
            b"exact source terrain bytes"
        );
        assert_eq!(
            std::fs::read(review.join("manifest_edits.json")).unwrap(),
            overlay
        );
        std::fs::write(review.join("manifest_edits.json"), b"review-only edits").unwrap();
        assert_eq!(
            std::fs::read(live.join("manifest_edits.json")).unwrap(),
            overlay
        );
        assert_eq!(std::fs::read(source).unwrap(), manifest);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn world_review_copy_rejects_heightfield_escape_before_writing() {
        let root = scratch("escape");
        let source = root.join("manifest.v1.json");
        let review = root.join("review");
        std::fs::create_dir(&review).unwrap();
        for reference in ["../heights.npy", "/tmp/heights.npy", ""] {
            std::fs::write(
                &source,
                serde_json::to_vec(&json!({"terrain":{"heights":reference}})).unwrap(),
            )
            .unwrap();
            assert!(copy_world_document(&source, &review).is_err());
            assert!(std::fs::read_dir(&review).unwrap().next().is_none());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
