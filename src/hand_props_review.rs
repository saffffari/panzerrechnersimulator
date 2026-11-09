//! `hand-props-review <prefix>`: the dismounted player's own hands with the P08 (hip, turning,
//! aimed, fired, reloaded) and with the cigarette kit (packet, a cigarette taken, the lighter,
//! lit, a toke, the exhale, in the mouth, the butt thrown down), then the kit at the loader's post.
//! Review harness only (headless). Every still is the recorder's ordinary gameplay frame at the
//! game's own pixels, drawn after each 1/60 s simulation step, so the picture carries the same
//! frame-to-frame history as play. Staging that play does not do is named where it happens: the
//! view pitch is set directly, a cigarette is taken from slot 3 if the look does not rest on one,
//! the burn is run down without drawing its minute, and the kit is put back in the pocket between
//! the two posts. `KESSEL_CANYON_MANIFEST` selects the Canyon course; without it, Act One.
//!
//! With each still: the first-person arm page's stretch audit (`FirstPerson::audit`) and, for
//! every drawn part of the pistol and the kit, where its bounds sit from the eye (right, up,
//! ahead, in metres) and how large they are. `KESSEL_HAND_PROPS_HIDE=arms|gun|kit` leaves that
//! group out of the drawn frames (to tell which meshes make a defect); the receipt names it.
//! `KESSEL_HAND_PROPS_ONLY=loader|luger|foot_cig` draws that one section's stills.
use super::*;

struct Shots<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: String,
    scale: u32,
    hide: Option<String>,
    receipt: Vec<serde_json::Value>,
}

/// Bounds of a drawn mesh from the eye: centre as (right, up, ahead) and the diagonal, metres.
fn part(r: &Renderer, name: &str, slot: usize) -> Option<serde_json::Value> {
    let (visible, _) = r.debug_mesh_state(slot)?;
    if !visible { return None; }
    let (lo, hi) = r.debug_mesh_world_bounds(slot)?;
    let forward = r.camera.forward().normalize_or_zero();
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(forward);
    let to = (lo + hi) * 0.5 - r.camera.pos;
    Some(serde_json::json!({"part": name, "slot": slot,
        "from_eye_m": [to.dot(right), to.dot(up), to.dot(forward)], "diagonal_m": (hi - lo).length()}))
}

impl Shots<'_> {
    fn hidden(&self, group: &str) -> bool { self.hide.as_deref() == Some(group) }
    /// `frames` ordinary simulation steps, each drawn as the gameplay frame; the last picture.
    fn run(&self, g: &mut Game, r: &mut Renderer, input: &TankInput, frames: usize) -> anyhow::Result<Vec<u8>> {
        let mut last = Vec::new();
        for _ in 0..frames.max(1) {
            g.update(input, 1.0 / 60.0, r);
            if self.hidden("arms") { g.hide_first_person_body(r); }
            if self.hidden("gun") {
                for slot in [g.slots.luger, g.slots.luger_mag].into_iter().chain(g.slots.luger_action) { r.set_visible(slot, false); }
            }
            if self.hidden("kit") {
                if let Some(props) = &g.hand_props { for slot in props.slots() { r.set_visible(slot, false); } }
            }
            last = crate::mechanic_capture::gameplay_native_frame(g, r, self.device, self.queue)?;
        }
        Ok(last)
    }
    fn save(&mut self, g: &Game, r: &Renderer, name: &str, rgba: Vec<u8>) -> anyhow::Result<()> {
        let native = image::RgbaImage::from_raw(r.iw, r.ih, rgba)
            .ok_or_else(|| anyhow::anyhow!("gameplay frame is not {}x{}", r.iw, r.ih))?;
        let path = format!("{}-{name}.png", self.prefix);
        image::imageops::resize(&native, r.iw * self.scale, r.ih * self.scale, image::imageops::FilterType::Nearest)
            .save(&path)?;
        let mut parts = Vec::new();
        let gun = [("p08_frame", g.slots.luger), ("p08_magazine", g.slots.luger_mag), ("p08_bolt", g.slots.luger_action[0]),
            ("p08_front_link", g.slots.luger_action[1]), ("p08_rear_link", g.slots.luger_action[2]),
            ("p08_barrel", g.slots.luger_action[3]), ("torch_body", g.slots.hand_torch[0]), ("torch_lens", g.slots.hand_torch[1])];
        parts.extend(gun.into_iter().filter_map(|(name, slot)| part(r, name, slot)));
        if let Some(props) = &g.hand_props {
            parts.extend(props.debug_named_slots().into_iter().filter_map(|(name, slot)| part(r, &name, slot)));
        }
        if let Some(fp) = &g.first_person {
            let (local, witness) = fp.debug_slots();
            for (i, slot) in local.into_iter().enumerate() {
                parts.extend(part(r, ["seated_arms", "seated_body", "standing_arms", "standing_body"][i], slot));
            }
            parts.extend(part(r, "witness_body", witness));
        }
        let audit = g.first_person.as_ref().map(|f| f.audit(g, r)).unwrap_or(serde_json::Value::Null);
        println!("[hand-props] {name}: {} parts drawn, arms {audit}", parts.len());
        self.receipt.push(serde_json::json!({"still": name, "camera": r.camera.pos.to_array(),
            "forward": r.camera.forward().to_array(), "mode": if g.mode == Mode::Foot {"foot"} else {"tank"},
            "cigarette": g.cigarette_snapshot(), "parts": parts, "first_person": audit}));
        Ok(())
    }
    fn shot(&mut self, g: &mut Game, r: &mut Renderer, input: &TankInput, frames: usize, name: &str) -> anyhow::Result<()> {
        let rgba = self.run(g, r, input, frames)?;
        self.save(g, r, name, rgba)
    }
    /// The same moment from 0.6 m to the man's right (after the frame's update; nothing moves).
    fn from_the_side(&mut self, g: &mut Game, r: &mut Renderer, input: &TankInput, name: &str) -> anyhow::Result<()> {
        self.run(g, r, input, 1)?;
        let forward = r.camera.forward().normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let eye = r.camera.pos;
        let target = eye + forward * 0.30 - Vec3::Y * 0.14;
        r.camera.pos = eye + right * 0.62 + forward * 0.22 + Vec3::Y * 0.04;
        r.camera.set_forward((target - r.camera.pos).normalize());
        let rgba = crate::mechanic_capture::gameplay_native_frame(g, r, self.device, self.queue)?;
        self.save(g, r, name, rgba)
    }
}

/// The cigarette kit through its ordinary inputs (C, the left button on the packet, the right
/// button held to light and to draw), drawn at each stage. `tag` prefixes the still names.
fn cigarette_kit(sh: &mut Shots, g: &mut Game, r: &mut Renderer, tag: &str, on_foot: bool) -> anyhow::Result<()> {
    let idle = TankInput::default();
    let rmb = TankInput { aim: true, ..Default::default() };
    let lmb = TankInput { fire2: true, trigger: true, mg_coax: !on_foot, ..Default::default() };
    let turn = TankInput { look_dx: 10.0, ..Default::default() };
    let set_pitch = |g: &mut Game, pitch: f32| if on_foot { g.foot.pitch = pitch } else { g.head_pitch = pitch };
    anyhow::ensure!(g.toggle_cigarette(), "{tag}: the cigarette packet could not be brought out");
    sh.shot(g, r, &idle, 30, &format!("{tag}_pack_open"))?;
    // The packet is held under the line of sight; the eye drops to its opening to choose.
    set_pitch(g, -0.25);
    sh.shot(g, r, &idle, 20, &format!("{tag}_pack_looked_at"))?;
    sh.run(g, r, &lmb, 2)?;
    let picked_by_look = g.cigarettes.view().cigarette_present;
    if !picked_by_look { g.cigarettes.take(3); }
    println!("[hand-props] {tag}: cigarette taken by the look and left button: {picked_by_look}");
    set_pitch(g, 0.0);
    sh.shot(g, r, &idle, 30, &format!("{tag}_in_hand_unlit"))?;
    sh.shot(g, r, &rmb, 8, &format!("{tag}_lighter_rising"))?;
    sh.shot(g, r, &rmb, 12, &format!("{tag}_lighter_flame"))?;
    sh.run(g, r, &rmb, 30)?;
    anyhow::ensure!(g.cigarettes.view().lit, "{tag}: the cigarette did not light");
    sh.shot(g, r, &idle, 40, &format!("{tag}_lit"))?;
    sh.run(g, r, &turn, 6)?;
    sh.shot(g, r, &turn, 1, &format!("{tag}_lit_turning_a"))?;
    sh.shot(g, r, &turn, 1, &format!("{tag}_lit_turning_b"))?;
    sh.shot(g, r, &idle, 1, &format!("{tag}_lit_turn_stopped"))?;
    sh.shot(g, r, &rmb, 60, &format!("{tag}_toke"))?;
    sh.shot(g, r, &idle, 14, &format!("{tag}_exhale_early"))?;
    sh.shot(g, r, &idle, 30, &format!("{tag}_exhale_late"))?;
    set_pitch(g, -0.6);
    sh.shot(g, r, &idle, 20, &format!("{tag}_lit_looking_down"))?;
    set_pitch(g, 0.0);
    if on_foot {
        let walk = TankInput { drive: 1.0, ..Default::default() };
        sh.run(g, r, &walk, 30)?;
        sh.shot(g, r, &walk, 1, &format!("{tag}_lit_running"))?;
        sh.from_the_side(g, r, &idle, &format!("{tag}_lit_from_the_side"))?;
    }
    // C with one in the hand shows the packet again; C once more puts it away.
    g.toggle_cigarette();
    sh.shot(g, r, &idle, 30, &format!("{tag}_pack_shown_with_one_out"))?;
    g.toggle_cigarette();
    g.stow_personal_hand_item();
    sh.shot(g, r, &idle, 30, &format!("{tag}_in_mouth"))?;
    sh.run(g, r, &turn, 6)?;
    sh.shot(g, r, &turn, 1, &format!("{tag}_in_mouth_turning"))?;
    Ok(())
}

pub fn run(requested: &str) -> anyhow::Result<()> {
    let prefix = crate::capture_target(requested);
    if let Some(parent) = std::path::Path::new(&prefix).parent() { std::fs::create_dir_all(parent)?; }
    let scale = std::env::var("KESSEL_HAND_PROPS_SCALE").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).clamp(1, 4);
    let hide = std::env::var("KESSEL_HAND_PROPS_HIDE").ok().filter(|v| !v.is_empty());
    anyhow::ensure!(hide.as_deref().is_none_or(|h| ["arms", "gun", "kit"].contains(&h)),
        "KESSEL_HAND_PROPS_HIDE is arms, gun or kit");
    let only = std::env::var("KESSEL_HAND_PROPS_ONLY").ok().filter(|v| !v.is_empty());
    anyhow::ensure!(only.as_deref().is_none_or(|o| ["loader", "luger", "foot_cig"].contains(&o)),
        "KESSEL_HAND_PROPS_ONLY is loader, luger or foot_cig");
    let section = |name: &str| only.as_deref().is_none_or(|o| o == name);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let canyon = std::env::var_os(crate::canyon::MANIFEST_ENV).map(std::path::PathBuf::from);
    let (mut r, mut g) = if let Some(path) = &canyon {
        let act = crate::act_manifest::select_runtime_manifest(Some(path.clone()))?;
        anyhow::ensure!(act == Some(1), "the canyon course {} is not an Act One map", path.display());
        let (r, mut g) = Game::build_playtest_act(&device, &queue, false, &crate::world::act(1))?;
        g.playtest_setup();
        g.enable_canyon_course()?;
        (r, g)
    } else {
        Game::build_act(&device, &queue, false, &crate::world::act(1))?
    };
    g.enable_singleplayer_crew();
    g.mute_audio();
    g.debug_hold_enemy_fire();
    g.set_defer_rtt_to_render(true);
    r.camera.aspect = 16.0 / 9.0;
    g.chase_cam = false;
    let mut sh = Shots { device: &device, queue: &queue, prefix: prefix.clone(), scale, hide, receipt: Vec::new() };
    let idle = TankInput::default();
    let (g, r) = (&mut g, &mut r);

    // Inside first: the loader's post under the cabin lamps.
    g.set_cabin_lights(true);
    g.jump_station(Station::Loader);
    sh.run(g, r, &idle, 90)?;
    anyhow::ensure!(g.station == Station::Loader, "could not reach the loader post");
    g.head_yaw = 0.0;
    g.head_pitch = 0.0;
    if section("loader") {
        sh.shot(g, r, &idle, 20, "loader_empty_hands")?;
        cigarette_kit(&mut sh, g, r, "loader_cig", false)?;
    }
    // Review staging: the kit goes back to a full packet in the pocket before the walk.
    g.cigarettes = crate::cigarette::State::default();
    sh.run(g, r, &idle, 10)?;

    // Outside: the ordinary hatch and dismount, then open ground beside the Tiger.
    g.jump_station(Station::Commander);
    sh.run(g, r, &idle, 90)?;
    sh.run(g, r, &TankInput { hatch_toggle: true, ..Default::default() }, 1)?;
    for _ in 0..120 { g.update(&idle, 1.0 / 60.0, r); }
    anyhow::ensure!(g.can_climb_out(), "the cupola hatch did not open for the dismount");
    g.climb_out();
    anyhow::ensure!(g.dbg_is_foot(), "the ordinary dismount did not complete");
    let side = g.player.pos + Quat::from_rotation_y(g.player.heading) * Vec3::new(4.5, 0.0, 1.0);
    g.dbg_place_foot(side.x, side.z);
    g.foot.yaw = std::f32::consts::PI - g.player.heading;
    g.foot.pitch = 0.0;
    g.select_foot_weapon(crate::foot::FootWeapon::Luger);
    if section("luger") { luger(&mut sh, g, r)?; }
    if section("foot_cig") { foot_kit(&mut sh, g, r)?; }
    let path = format!("{prefix}.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.hand-props-review.v1", "course": canyon, "hidden_group": sh.hide, "only_section": only,
        "frame": "recorder gameplay frame at game pixels, one per 1/60 s step", "stills": sh.receipt}))?)?;
    println!("[hand-props] wrote {path}");
    Ok(())
}

/// The P08 on foot: at the hip, turning, looking down, running, aimed, fired, the magazine change.
fn luger(sh: &mut Shots, g: &mut Game, r: &mut Renderer) -> anyhow::Result<()> {
    let idle = TankInput::default();
    let turn = TankInput { look_dx: 10.0, ..Default::default() };
    let walk = TankInput { drive: 1.0, ..Default::default() };
    let aim = TankInput { aim: true, ..Default::default() };
    sh.shot(g, r, &idle, 60, "luger_hip")?;
    sh.from_the_side(g, r, &idle, "luger_hip_from_the_side")?;
    sh.run(g, r, &turn, 6)?;
    sh.shot(g, r, &turn, 1, "luger_turning_a")?;
    sh.shot(g, r, &turn, 1, "luger_turning_b")?;
    sh.shot(g, r, &idle, 1, "luger_turn_stopped")?;
    g.foot.pitch = -0.6;
    sh.shot(g, r, &idle, 20, "luger_looking_down")?;
    g.foot.pitch = 0.0;
    sh.run(g, r, &walk, 30)?;
    sh.shot(g, r, &walk, 1, "luger_running_a")?;
    sh.shot(g, r, &walk, 8, "luger_running_b")?;
    sh.shot(g, r, &aim, 30, "luger_aimed")?;
    sh.from_the_side(g, r, &aim, "luger_aimed_from_the_side")?;
    sh.shot(g, r, &TankInput { aim: true, fire2: true, trigger: true, ..Default::default() }, 1, "luger_fired_frame_0")?;
    sh.shot(g, r, &aim, 1, "luger_fired_frame_1")?;
    sh.shot(g, r, &aim, 2, "luger_fired_frame_3")?;
    sh.shot(g, r, &aim, 6, "luger_fired_frame_9")?;
    let aim_turn = TankInput { aim: true, look_dx: 6.0, ..Default::default() };
    sh.run(g, r, &aim_turn, 6)?;
    sh.shot(g, r, &aim_turn, 1, "luger_aimed_turning")?;
    sh.run(g, r, &idle, 30)?;
    // The magazine change (1.6 s): mag out, the hand away, the fresh one in, home.
    sh.run(g, r, &TankInput { reload: true, ..Default::default() }, 1)?;
    anyhow::ensure!(g.foot.reload_t > 0.0, "the P08 reload did not start");
    for (frames, name) in [(13, "luger_reload_15"), (19, "luger_reload_35"), (19, "luger_reload_55"),
        (24, "luger_reload_80"), (15, "luger_reload_95")] {
        sh.shot(g, r, &idle, frames, name)?;
    }
    sh.shot(g, r, &idle, 30, "luger_after_reload")?;
    Ok(())
}

/// The cigarette kit on foot, then the pistol with one in the mouth, the butt and the torch.
fn foot_kit(sh: &mut Shots, g: &mut Game, r: &mut Renderer) -> anyhow::Result<()> {
    let idle = TankInput::default();
    cigarette_kit(sh, g, r, "foot_cig", true)?;
    // The pistol comes back to the hand with the lit one in the mouth.
    sh.shot(g, r, &idle, 20, "luger_with_one_in_mouth")?;
    // Review staging: the rest of the minute's burn is simulated without drawing it, then the
    // butt is thrown down at the filter.
    let mut steps = 0;
    while g.cigarettes.view().cigarette_present && steps < 4200 {
        g.update(&idle, 1.0 / 60.0, r);
        steps += 1;
    }
    sh.shot(g, r, &idle, 2, "butt_thrown_frame_2")?;
    sh.shot(g, r, &idle, 20, "butt_thrown_frame_22")?;
    g.foot.pitch = -0.9;
    sh.shot(g, r, &idle, 60, "butt_on_the_ground")?;
    g.foot.pitch = 0.0;
    if g.toggle_hand_flashlight() {
        sh.shot(g, r, &idle, 30, "torch_in_hand")?;
        g.stow_personal_hand_item();
    }
    Ok(())
}
