//! First-person, simulation-clock films of ordinary gameplay inputs and ordinary Audio sinks.
//! Scenario setup is disclosed in the receipt. The explicit crew-arm witness uses a render-only
//! compartment camera and is labelled non-first-person; ordinary clips retain the player camera.
//! Audio always comes from the ordinary gameplay sinks, without a substitute soundtrack.
use super::*;
use serde_json::{json, Value};
use std::io::Write;

const FPS: u32 = 24;
const SIM_HZ: u32 = 120;
const INPUTS: &[&str] = &["drive", "steer", "traverse", "elevate", "look_dx", "look_dy",
    "fire", "mg_coax", "coax", "binoculars", "extinguish", "repair", "run", "aim", "fire2",
    "crouch", "reload", "jump", "ignition", "hatch_toggle"];
const ACTIONS: &[&str] = &["hatch", "dismount", "mount", "head_out", "head_in", "station",
    "round", "portable", "throw", "equip", "look_lever", "look_breech", "look_tray", "look_rack",
    "weapon_luger", "weapon_panzerfaust", "sight_zoom", "map_open", "map_close", "incoming_impact", "look_fire", "map_pin",
    "look_sentry", "look_left_track", "door", "look_loaded_round",
    "compass",
    "look_equipment",
    "look_hatch",
    "look_cupola",
    "look_driver_control",
    "map_zoom",
    "look_at_gun",
    "staged_discharge",
    "lay_on_at_gun",
];

/// Explicit render-only crew witness for checking source hands. Ordinary input still uses the
/// live player camera; the observer cannot aim, move a body, or alter a mechanical state.
#[derive(Clone, Copy)]
struct CrewWitness { eye: glam::Vec3, target: glam::Vec3 }
impl CrewWitness {
    fn from_spec(spec: &Value) -> anyhow::Result<Option<Self>> {
        let Some(view)=spec.get("crew_witness") else {return Ok(None);};
        anyhow::ensure!(spec["setup"]["singleplayer_crew"]==true
            && spec["setup"].get("showcase_crew").is_none()
            && spec["setup"].get("diagnostic_crew").is_none()
            && spec["setup"].get("act_one_playtest").is_none()
            && spec.get("diagnostic_view").is_none(),
            "crew witness requires explicit solo crew, separate from live Act-One/first-person/LAN diagnostics");
        let vector=|key:&str|->anyhow::Result<glam::Vec3> {
            let a=view[key].as_array().ok_or_else(||anyhow::anyhow!("crew witness requires {key}"))?;
            anyhow::ensure!(a.len()==3 && a.iter().all(|n|n.as_f64().is_some_and(|v|v.is_finite()&&v.abs()<8.0)),
                "crew witness coordinates must be finite compartment-local metres");
            Ok(glam::Vec3::new(a[0].as_f64().unwrap() as f32,a[1].as_f64().unwrap() as f32,a[2].as_f64().unwrap() as f32))
        };
        let eye=vector("eye_turret_m")?;let target=vector("target_turret_m")?;
        anyhow::ensure!(eye.distance(target)>0.1,"crew witness view direction is degenerate");
        Ok(Some(Self{eye,target}))
    }
}

fn loader_preview_requested(spec: &Value) -> anyhow::Result<bool> {
    let requested = spec["setup"].get("loader_arm_preview");
    if let Some(value) = requested {
        anyhow::ensure!(value == true && spec["setup"]["singleplayer_crew"] == true,
            "loader_arm_preview must be explicit true with singleplayer_crew");
    }
    let tests_loader_contacts = spec["expect"].as_array().into_iter().flatten().any(|check|
        check["metric"].as_str().is_some_and(|metric| metric.starts_with("loader_hand_")));
    anyhow::ensure!(!tests_loader_contacts || requested == Some(&Value::Bool(true)),
        "Loader contact fixtures must explicitly declare setup.loader_arm_preview=true");
    Ok(requested == Some(&Value::Bool(true)))
}

fn require_loader_preview(spec: &Value, enabled: bool) -> anyhow::Result<()> {
    anyhow::ensure!(!loader_preview_requested(spec)? || enabled,
        "Loader arm prototype is disabled: set KESSEL_LOADER_ARM_PREVIEW=1 explicitly; its baseline contact checks currently fail");
    Ok(())
}

struct ReviewMap {
    open: bool,
    cursor: glam::Vec2,
    marks: Vec<(u8, glam::Vec2)>,
    pins: Vec<(u8, glam::Vec2)>,
    primary: bool,
}
impl Default for ReviewMap {
    fn default() -> Self {
        Self { open:false, cursor:glam::Vec2::new(0.50,0.48), marks:Vec::new(),
            pins:Vec::new(), primary:false }
    }
}
impl ReviewMap {
    fn add_to(&self, state: &mut Value) {
        state["map_open"]=json!(self.open);
        state["map_pencil_points"]=json!(self.marks.len());
        state["map_pins"]=json!(self.pins.len());
    }
    fn controls(&mut self, controls: TankInput, act: u8, zoom: f32) -> TankInput {
        if !self.open {return controls;}
        self.primary=controls.mg_coax;
        ui::update_map_pencil(&mut self.cursor,&mut self.marks,act,
            glam::Vec2::new(controls.look_dx,controls.look_dy)/zoom.max(1.0),self.primary);
        // Matches the real window: holding paper occupies both hands while the world runs.
        TankInput::default()
    }
}

fn incoming_shell(name: &str) -> anyhow::Result<&'static ballistics::ShellSpec> {
    Ok(match name {
        "BR_240_M42" => &ballistics::BR_240_M42, "BR_271_ZIS2" => &ballistics::BR_271_ZIS2,
        "BR_350B" => &ballistics::BR_350B, "BR_354P" => &ballistics::BR_354P,
        "BR_365" => &ballistics::BR_365, "BR_412" => &ballistics::BR_412,
        "BR_471" => &ballistics::BR_471, "BR_540" => &ballistics::BR_540,
        "OF_530" => &ballistics::OF_530,
        _ => anyhow::bail!("unsupported incoming projectile {name}"),
    })
}

fn module_kind(name: &str) -> anyhow::Result<game::ModuleKind> {
    use game::ModuleKind::*;
    Ok(match name {
        "main_gun"=>MainGun, "traverse"=>TurretTraverse, "sight"=>GunnerSight,
        "coax"=>CoaxMg, "engine"=>Engine, "transmission"=>Transmission,
        "left_track"=>LeftTrack, "right_track"=>RightTrack, "fuel"=>Fuel,
        "ammo"=>AmmoRack, "hull_mg"=>HullMg, "radio"=>Radio,
        _=>anyhow::bail!("unknown module {name}"),
    })
}

/// The disclosed stationary diagnostic fixture installs actual mortal roster entries via the
/// ordinary couch input path. It never creates injury records or substitutes decorative bodies.
fn diagnostic_crew_controls(mut input: TankInput, enabled: bool) -> TankInput {
    if !enabled { return input; }
    let mut crew=lan::CrewFrame::default();
    let identities=std::array::from_fn(|i| {
        let identity=game::COUCH_CREW_ROLES[i];let peer_id=i as u32+1;
        crew.roster[identity as usize]=peer_id;
        crew.inputs[identity as usize].post=identity;
        Some(game::CouchCrewIdentity{peer_id,identity})
    });
    input.coop=Some(crew);input.coop_host=true;input.couch_crew=true;
    input.couch_identities=identities;input
}

/// An explicitly staged five-person LAN roster, viewed from one fixed physical post. These are
/// occupied input channels and the shipping crew rigs, not an AI crew or extra decorative meshes.
/// Four peers remain idle; the viewer's controls use the same mapper as a native LAN session.
#[derive(Clone, Copy)]
struct ShowcaseCrew {
    role: lan::LanRole,
    post: game::Station,
}

impl ShowcaseCrew {
    fn from_spec(spec: &Value) -> anyhow::Result<Option<Self>> {
        let Some(enabled) = spec["setup"].get("showcase_crew") else { return Ok(None); };
        anyhow::ensure!(enabled == true, "showcase_crew must be explicit true");
        anyhow::ensure!(spec.get("diagnostic_view").is_none()
            && spec["setup"].get("diagnostic_crew").is_none()
            && spec["setup"]["on_foot"] != true,
            "showcase_crew requires an occupied first-person view, not a retained-model diagnostic");
        let post = station(spec["station"].as_str()
            .ok_or_else(|| anyhow::anyhow!("showcase_crew requires an explicit fixed station"))?)?;
        let role = match post {
            game::Station::Commander => lan::LanRole::Commander,
            game::Station::Driver => lan::LanRole::Driver,
            game::Station::Gunner => lan::LanRole::Gunner,
            game::Station::Loader => lan::LanRole::Loader,
            game::Station::Radioman => lan::LanRole::Radio,
        };
        let steps = spec["timeline"].as_array()
            .ok_or_else(|| anyhow::anyhow!("showcase_crew requires a timeline"))?;
        for step in steps {
            anyhow::ensure!(!matches!(step["action"].as_str(), Some("station" | "mount" | "dismount"))
                && step["input"]["jump"] != true,
                "showcase_crew keeps a fixed occupied LAN post; use a solo fixture for hatch traversal or station changes");
        }
        Ok(Some(Self { role, post }))
    }

    fn controls(self, mut input: TankInput, post: game::Station, equipped: bool,
        hovered: bool, repair_hovered: bool) -> TankInput {
        let mut frame = lan::CrewFrame::default();
        for identity in lan::LanRole::ALL {
            frame.roster[identity as usize] = identity as u32 + 1;
            frame.inputs[identity as usize].post = identity;
        }
        frame.inputs[self.role as usize] =
            lan_role_input(input, self.role, post, equipped, hovered, repair_hovered);
        input.coop = Some(frame);
        input.coop_host = true;
        // This is the five-role LAN path. Do not imply four-seat couch casualty/takeover rules.
        input.couch_crew = false;
        input.couch_identities = Default::default();
        input
    }

    fn receipt(self) -> Value {
        json!({"mode":"fixed_post_lan_input_fixture", "viewer_role":self.role.label(),
            "occupied_roles":lan::LanRole::ALL.map(|role|role.label()),
            "staged_peer_ids":[1,2,3,4,5],"idle_other_peers":4,
            "expected_other_body_count":4,
            "input_source":"Only the viewer timeline, routed through native lan_role_input and host Game::update",
            "visibility":"Shipping LAN crew rigs; viewer identity and occupied viewpoint body hidden by Game",
            "scope":"Five staged occupied identities; no network sockets, remote humans, AI decisions, automatic crew actions, or forced damage outcomes. Other bodies are naturally occluded by compartment geometry.",
            "limitations":"Fixed post; solo hatch traversal is not bypassed. This does not claim the four-player couch casualty/takeover system supports five mortal crew."})
    }
}

fn capture_crew_controls(input: TankInput, g: &Game, diagnostic: bool,
    showcase: Option<ShowcaseCrew>) -> TankInput {
    if let Some(crew) = showcase {
        crew.controls(input, g.station, g.ext_equipped, g.hovering_extinguisher(),
            g.hovering_repair_kit())
    } else {
        diagnostic_crew_controls(input, diagnostic)
    }
}

/// The terminal latch owns every gameplay callback in the recorder, not only Game::update.
/// Audio sampling and the death fade deliberately remain outside this gate.
fn capture_gameplay_if_alive<T>(kia_at: Option<f32>, operation: impl FnOnce() -> T) -> Option<T> {
    if kia_at.is_some() { None } else { Some(operation()) }
}

fn terminal_input_rejection(kia_at: f32) -> Value {
    json!({"reason":"terminal_kia", "kia_started_s":kia_at,
        "scope":"Skipped after terminal state; no gameplay action, input or interaction callback invoked"})
}

fn required_capture_actions_passed(events: &[Value]) -> bool {
    events.iter().all(|event| event["request"].get("action").is_none()
        || event["accepted"] == true || event["request"]["allow_rejected"] == true)
}

/// The one-T-34 Arena bench: the enemy-report review or the sound-calibration duel.
fn duel_setup(spec: &Value) -> Option<&Value> {
    spec["setup"].get("enemy_discharge_review").or_else(|| spec["setup"].get("calibration_duel"))
}

fn scalar(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(default)
}
fn station(name: &str) -> anyhow::Result<game::Station> {
    Ok(match name {
        "commander" => game::Station::Commander, "gunner" => game::Station::Gunner,
        "loader" => game::Station::Loader, "driver" => game::Station::Driver,
        "radio" => game::Station::Radioman,
        _ => anyhow::bail!("unknown physical station {name}"),
    })
}
fn input(value: &Value) -> TankInput {
    let f = |key| scalar(value, key, 0.0) as f32;
    let b = |key| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    TankInput { drive:f("drive"), steer:f("steer"), traverse:f("traverse"), elevate:f("elevate"),
        look_dx:f("look_dx"), look_dy:f("look_dy"), fire:b("fire"), mg_coax:b("mg_coax"),
        coax:b("coax"), binoculars:b("binoculars"), extinguish:b("extinguish"), repair:b("repair"),
        run:b("run"), aim:b("aim"), fire2:b("fire2"), crouch:b("crouch"), reload:b("reload"),
        jump:b("jump"), ignition:b("ignition"), hatch_toggle:b("hatch_toggle"), ..Default::default() }
}

/// Pure CPU preflight; callers may use this before allocating any renderer or audio bank.
fn explicit_manifest_path(path: &str) -> anyhow::Result<std::path::PathBuf> {
    let path = std::path::Path::new(path);
    if path.is_file() && (path.is_absolute() || path.starts_with("assets/worldgen/revisions")) {
        // A fixture's checked-in revision wins over KESSEL_ASSET_ROOT. Canonicalize before
        // ActManifest sees it; leave ordinary packaged-resource fallback semantics untouched.
        Ok(std::fs::canonicalize(path)?)
    } else {
        crate::resource::resolve(path)
    }
}

pub fn validate_file(path: &str) -> anyhow::Result<Value> {
    let spec: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    anyhow::ensure!(spec["schema"] == "kessel.first-person-mechanic.v1", "wrong mechanic schema");
    let name = spec["id"].as_str().ok_or_else(|| anyhow::anyhow!("missing scenario id"))?;
    anyhow::ensure!(!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "scenario id must be filename-safe");
    let seconds = scalar(&spec, "seconds", 0.0);
    anyhow::ensure!((2.0..=60.0).contains(&seconds), "duration must be 2..60 seconds");
    validate_act_one_playtest(&spec)?;
    ShowcaseCrew::from_spec(&spec)?;
    CrewWitness::from_spec(&spec)?;
    loader_preview_requested(&spec)?;
    if let Some(enabled)=spec["setup"].get("singleplayer_crew") {
        anyhow::ensure!(enabled==true && spec["setup"].get("showcase_crew").is_none()
            && spec["setup"].get("diagnostic_crew").is_none(), "singleplayer_crew must be explicit true without network fixtures");
    }
    if let Some(caption)=spec.get("caption") {
        anyhow::ensure!(caption.as_str().is_some_and(|s|s.is_ascii() && s.len()<=100
            && !s.contains('\n') && !s.contains('\r')), "caption must be one short ASCII pixel-font line");
    }
    if let Some(view) = spec.get("diagnostic_view") {
        anyhow::ensure!(view == "retained_model", "unsupported diagnostic view");
        anyhow::ensure!(spec["setup"]["on_foot"] != true,
            "retained tank diagnostic requires an occupied tank fixture");
    }
    if spec["setup"].get("diagnostic_crew").is_some() {
        anyhow::ensure!(spec["setup"]["diagnostic_crew"] == true && spec["diagnostic_view"] == "retained_model",
            "diagnostic_crew must be explicit true in a retained-model diagnostic");
        anyhow::ensure!(spec["timeline"].as_array().is_some_and(|steps|steps.iter().all(|step|
            matches!(step["action"].as_str(),Some("incoming_impact"|"hatch")))),
            "stationary diagnostic crew fixture accepts only disclosed contact/hatch actions");
    }
    if let Some(seconds) = spec.get("diagnostic_anatomy_last_s") {
        anyhow::ensure!(spec["diagnostic_view"] == "retained_model" && seconds.as_f64()
            .is_some_and(|n|n.is_finite() && n>=0.0 && n<scalar(&spec,"seconds",0.0)),
            "anatomy appendix must fit the explicit diagnostic timeline");
    }
    let act = spec["act_id"].as_u64().ok_or_else(|| anyhow::anyhow!("missing act_id"))?;
    anyhow::ensure!(act <= 3, "act_id must be 0..3");
    if let Some(value) = spec["setup"].get("winter_session") {
        anyhow::ensure!(value == true && act == 1 && spec["setup"]["playtest"] == true,
            "winter_session must be explicit true on an Act One Playtest world");
    }
    if let Some(playtest) = spec["setup"].get("playtest") {
        anyhow::ensure!(playtest.is_boolean(), "playtest must be an explicit boolean");
    }
    if let Some(value) = spec["setup"].get("canyon_session") {
        anyhow::ensure!(value == true && act == 1 && spec["setup"]["playtest"] == true
            && spec["setup"].get("winter_session").is_none(),
            "canyon_session must be explicit true on an Act One Playtest world, without winter_session");
    }
    if spec["setup"].get("canyon_stage").is_some() {
        anyhow::ensure!(spec["setup"]["canyon_session"] == true, "canyon_stage needs canyon_session");
    }
    if let Some(fault) = spec["setup"].get("primer_fault") {
        anyhow::ensure!(fault == "chamber", "primer_fault must name the existing chamber cartridge");
        anyhow::ensure!(spec["setup"]["on_foot"] != true,
            "primer-fault review requires an occupied tank");
        for incompatible in ["broken_module", "fire", "source_fire"] {
            anyhow::ensure!(spec["setup"].get(incompatible).is_none(),
                "isolated primer-fault review cannot combine with {incompatible}");
        }
    }
    if let Some(path) = spec["world_manifest"].as_str() {
        let selected = act_manifest::ActManifest::load(&explicit_manifest_path(path)?)?;
        anyhow::ensure!(selected.act_id == act as u8, "scenario act_id differs from selected manifest");
    }
    if let Some(value) = spec["setup"].get("playtest_refuel") {
        anyhow::ensure!(value.is_boolean(), "playtest_refuel must be an explicit boolean");
        if value == true {
            anyhow::ensure!(spec["setup"]["playtest"] == true && spec["setup"]["on_foot"] == true,
                "playtest_refuel requires disclosed Playtest and ordinary on-foot setup");
            anyhow::ensure!(spec["world_manifest"].is_string(), "playtest_refuel requires a selected world");
            for incompatible in ["mixed_combat", "soft_target_tank", "track_repair_post", "fire", "broken_module"] {
                anyhow::ensure!(spec["setup"].get(incompatible).is_none(),
                    "playtest_refuel cannot combine with {incompatible}");
            }
        }
    }
    if let Some(fixture)=spec["setup"].get("feature_building") {
        anyhow::ensure!(matches!(fixture.as_str(),Some("entry"|"shelf")),
            "feature_building must be entry or shelf");
        anyhow::ensure!(spec["setup"]["playtest"]==true && spec["setup"]["on_foot"]==true,
            "feature-building fixture requires disclosed standalone Playtest and ordinary on-foot setup");
    }
    if let Some(fixture)=spec["setup"].get("destruction_review") {
        anyhow::ensure!(matches!(fixture.as_str(),Some("feature_wall"|"small_tree")),
            "destruction_review must be feature_wall or small_tree");
        anyhow::ensure!(spec["setup"]["playtest"]==true && spec["setup"]["on_foot"]!=true,
            "destruction fixture requires standalone Playtest and an occupied tank");
    }
    for key in ["player_xz", "foot_xz", "look_at_xz"] {
        if let Some(value) = spec["setup"].get(key) {
            anyhow::ensure!(value.as_array().is_some_and(|a| a.len() == 2
                && a.iter().all(|x| x.as_f64().is_some_and(f64::is_finite))),
                "{key} must contain two finite coordinates");
        }
    }
    if let Some(setup) = spec["setup"].get("source_fire") {
        anyhow::ensure!(matches!(setup["frame"].as_str(), Some("hull" | "turret")),
            "source_fire.frame must be hull or turret");
        for key in ["ray_start_m", "ray_end_m"] {
            anyhow::ensure!(setup[key].as_array().is_some_and(|a| a.len() == 3 &&
                a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite))),
                "source_fire.{key} requires three finite source-frame coordinates");
        }
        anyhow::ensure!((0.1..=1.4).contains(&scalar(setup, "strength", 0.5)),
            "source_fire.strength must be within 0.1..1.4");
        anyhow::ensure!(spec["setup"]["fire"] != true && spec["setup"]["on_foot"] != true,
            "source_fire requires one initial fire in an occupied tank");
    }
    for (block, keys) in [("mixed_combat", &["center_xz"][..]),
        ("soft_target_tank", &["pos_xz", "target_xz"][..])] {
        if let Some(setup) = spec["setup"].get(block) {
            anyhow::ensure!(setup.is_object(), "{block} must be an explicit setup object");
            for key in keys {
                anyhow::ensure!(setup[*key].as_array().is_some_and(|a| a.len() == 2
                    && a.iter().all(|x| x.as_f64().is_some_and(f64::is_finite))),
                    "{block}.{key} requires two finite world coordinates");
            }
            anyhow::ensure!(spec["setup"]["on_foot"] == true
                || spec["station"].as_str().unwrap_or("commander") == "commander",
                "mixed combat review requires an ordinary on-foot or commander observer");
        }
    }
    if let Some(setup) = spec["setup"].get("soft_target_tank") {
        anyhow::ensure!(matches!(setup["kind"].as_str(), Some("kv2" | "t34")),
            "soft-target bench currently supports kv2 or t34");
        anyhow::ensure!(setup["index"].as_u64().is_some_and(|i|i<6), "tank index must be 0..5");
        if let Some(he) = setup.get("he_ammo") {
            anyhow::ensure!(he.as_u64().is_some_and(|n|n<=8), "soft-target HE ammunition must be 0..8 whole rounds");
        }
    }
    if let Some(setup) = spec["setup"].get("calibration_duel") {
        // The enemy-report bench's geometry (one T-34, ordinary AI) for a sound-level film: the
        // commander may close and open his hatch, ride head-out, drive, and use the disclosed
        // staged discharge. No impact, target placement or outcome is injected.
        anyhow::ensure!(spec["setup"].get("enemy_discharge_review").is_none(),
            "calibration duel and enemy report bench are separate fixtures");
        anyhow::ensure!(setup.is_object() && act == 0 && spec["night"] == false
            && spec["station"] == "commander" && spec["setup"]["on_foot"] != true
            && spec["setup"]["enemy_policy"] == "live" && spec["setup"]["hatch_open"] == true,
            "calibration duel needs a daylight Arena Commander, live AI and an open hatch at setup");
        for incompatible in ["mixed_combat", "soft_target_tank", "diagnostic_crew", "playtest",
            "fire", "source_fire", "broken_module", "destruction_review", "aim_enemy", "showcase_crew"] {
            anyhow::ensure!(spec["setup"].get(incompatible).is_none(),
                "calibration duel cannot combine with {incompatible}");
        }
        anyhow::ensure!(setup["range_m"].as_f64().is_some_and(|n|(40.0..=300.0).contains(&n))
            && setup.get("bearing_deg").is_none_or(|v|v.as_f64().is_some_and(|n|(-180.0..=180.0).contains(&n)))
            && setup.get("readiness_s").is_none_or(|v|v.as_f64().is_some_and(|n|(0.5..=3.0).contains(&n))),
            "calibration duel range/readiness/bearing invalid");
        anyhow::ensure!(spec["timeline"].as_array().is_some_and(|steps| steps.iter().all(|step|
            matches!(step["action"].as_str(), Some("hatch" | "head_out" | "head_in" | "staged_discharge"))
            || step["input"].as_object().is_some_and(|input| input.keys()
                .all(|key| matches!(key.as_str(), "drive" | "steer"))))),
            "calibration duel accepts hatch, head-out/in, staged discharge and driving only");
    }
    if let Some(setup) = spec["setup"].get("enemy_discharge_review") {
        anyhow::ensure!(setup.is_object() && act == 0 && spec["night"] == false
            && spec["station"] == "commander" && spec["setup"]["on_foot"] != true,
            "enemy discharge review needs a daylight Arena Commander observer");
        anyhow::ensure!(spec["setup"]["enemy_policy"] == "live" && spec["setup"]["hatch_open"] == true,
            "enemy discharge review needs explicit live AI and an open observer hatch");
        for incompatible in ["mixed_combat", "soft_target_tank", "diagnostic_crew", "playtest",
            "fire", "source_fire", "broken_module", "destruction_review", "aim_enemy"] {
            anyhow::ensure!(spec["setup"].get(incompatible).is_none(),
                "normal enemy discharge fixture cannot combine with {incompatible}");
        }
        anyhow::ensure!(setup["range_m"].as_f64().is_some_and(|n|(40.0..=300.0).contains(&n))
            && setup.get("bearing_deg").is_none_or(|v|v.as_f64().is_some_and(|n|(-180.0..=180.0).contains(&n)))
            && setup.get("readiness_s").is_none_or(|v|v.as_f64().is_some_and(|n|(0.5..=3.0).contains(&n))),
            "enemy discharge range/readiness/bearing invalid");
        anyhow::ensure!(spec["timeline"].as_array().is_some_and(|steps| !steps.is_empty()
            && steps.iter().all(|step| step["action"] == "head_out")),
            "stationary enemy report review accepts only the ordinary head-out action; no scripted impacts or weapons");
    }
    if let Some(setup) = spec["setup"].get("sentry_target") {
        anyhow::ensure!(spec["setup"]["on_foot"] == true, "sentry review needs the ordinary on-foot player");
        for key in ["pos_xz", "facing_xz"] {
            anyhow::ensure!(setup[key].as_array().is_some_and(|a| a.len()==2
                && a.iter().all(|v|v.as_f64().is_some_and(f64::is_finite))),
                "sentry_target.{key} requires two finite coordinates");
        }
    }
    if spec["setup"]["require_combat_candidates"] == true {
        let dir = std::env::var("KESSEL_COMBAT_WEIGHTS_DIR")
            .or_else(|_| std::env::var("KESSEL_COMBAT_POLICY_DIR"))
            .map_err(|_| anyhow::anyhow!("candidate review requires KESSEL_COMBAT_WEIGHTS_DIR"))?;
        for (name,role) in [("german_rifle.ksl2",foot_ai::PolicyRole::GermanRifle),
            ("soviet_rifle.ksl2",foot_ai::PolicyRole::SovietRifle)] {
            let path=std::path::Path::new(&dir).join(name);
            foot_ai::FootPolicy::load(path.to_string_lossy().as_ref(),role)?;
        }
        anti_infantry_ai::Policy::load(&std::path::Path::new(&dir).join("tank_soft_targets.json"))?;
    }
    station(spec["station"].as_str().unwrap_or("commander"))?;
    if let Some(name)=spec["setup"]["broken_module"].as_str() {module_kind(name)?;}
    let steps = spec["timeline"].as_array().ok_or_else(|| anyhow::anyhow!("missing timeline"))?;
    for step in steps {
        let start = scalar(step, "at_s", scalar(step, "from_s", 0.0));
        anyhow::ensure!((0.0..seconds).contains(&start), "step lies outside film");
        if let Some(action) = step["action"].as_str() {
            anyhow::ensure!(ACTIONS.contains(&action), "unsupported action {action}");
            if action == "station" { station(step["value"].as_str().unwrap_or(""))?; }
            if action == "map_zoom" {
                anyhow::ensure!(step["steps"].as_f64().is_some_and(|n| n.is_finite() && (-8.0..=8.0).contains(&n)),
                    "map_zoom requires finite steps within -8..8");
            }
            if action == "look_equipment" {
                anyhow::ensure!(
                    matches!(step["value"].as_str(), Some("toolbox" | "panzerfaust_rack" | "extinguisher")),
                    "look_equipment requires toolbox, panzerfaust_rack or extinguisher"
                );
            }
            if action == "look_driver_control" {
                anyhow::ensure!(matches!(step["value"].as_str(), Some("visor" | "ignition" | "steering" | "forward")),
                    "look_driver_control requires visor, ignition, steering or forward");
            }
            if action == "look_cupola" {
                anyhow::ensure!(matches!(step["value"].as_str(), Some("entry" | "grip" | "spindle"
                    | "block0" | "block1" | "block2" | "block3" | "block4" | "block5" | "block6")),
                    "look_cupola requires entry, grip, spindle or block0..block6");
            }
            if action == "look_hatch" {
                anyhow::ensure!(
                    matches!(
                        step["value"].as_str(),
                        Some(
                            "driver_lid" | "radio_lid" | "side_escape_handle"
                                | "side_escape_open_handle"
                                | "side_escape_aperture"
                                | "driver_aperture"
                                | "loader_aperture"
                                | "radio_aperture"
                                | "cupola_aperture"
                        )
                    ),
                    "look_hatch requires a named source hatch handle or aperture"
                );
            }
            if action == "incoming_impact" {
                incoming_shell(step["shell"].as_str().unwrap_or(""))?;
                anyhow::ensure!((1.0..=5000.0).contains(&scalar(step,"range_m",0.0)),
                    "incoming bench range must be 1..5000 m");
                anyhow::ensure!(step["physical_left"].is_boolean() && step["seed"].as_u64()
                    .is_some_and(|seed|seed<=u32::MAX as u64), "incoming side/seed must be explicit");
            }
        } else {
            let until = scalar(step, "until_s", -1.0);
            anyhow::ensure!(until > start && until <= seconds, "invalid input interval");
            let fields = step["input"].as_object().ok_or_else(|| anyhow::anyhow!("missing input"))?;
            for (key, value) in fields {
                anyhow::ensure!(INPUTS.contains(&key.as_str()), "unknown input {key}");
                let analog = INPUTS[..6].contains(&key.as_str());
                anyhow::ensure!(if analog { value.as_f64().is_some_and(|n| n.is_finite() && n.abs() <= 200.0) }
                    else { value.is_boolean() }, "invalid input value {key}");
            }
        }
    }
    if let Some(checks) = spec.get("expect") {
        let checks = checks.as_array().ok_or_else(|| anyhow::anyhow!("expect must be an array"))?;
        for check in checks {
            anyhow::ensure!(check["metric"].as_str().is_some_and(|key| !key.is_empty()), "outcome check requires metric");
            if let Some(at) = check.get("at_s") {
                anyhow::ensure!(at.as_u64().is_some_and(|t| t as f64 <= seconds),
                    "outcome checkpoint must be a whole second within the film");
            }
            for key in ["min_value", "max_value", "min_delta", "max_delta"] {
                if let Some(value) = check.get(key) {
                    anyhow::ensure!(value.as_f64().is_some_and(f64::is_finite), "{key} must be finite");
                }
            }
            anyhow::ensure!(check.get("equals").is_some() || ["min_value", "max_value", "min_delta", "max_delta"]
                .iter().any(|key|check.get(key).is_some()), "outcome check needs an explicit comparison");
        }
    }
    Ok(spec)
}

/// A campaign-backed Act-One recording cannot silently inherit the old six-target practice bench.
/// Only the deterministic run seed and explicit live policy are configurable; fixture teleports,
/// supplied damage, held enemy fire and pretend LAN/diagnostic crews are rejected before GPU work.
fn validate_act_one_playtest(spec: &Value) -> anyhow::Result<bool> {
    let Some(enabled) = spec["setup"].get("act_one_playtest") else { return Ok(false); };
    anyhow::ensure!(enabled == true && spec["act_id"] == 1,
        "act_one_playtest must be explicit true for Act One");
    anyhow::ensure!(spec.get("diagnostic_view").is_none(),
        "Act-One Playtest uses the actual first-person camera, not a retained diagnostic");
    anyhow::ensure!(spec["world_manifest"].is_string(),
        "Act-One Playtest capture must name its actual selected manifest");
    for key in spec["setup"].as_object().unwrap().keys() {
        anyhow::ensure!(["act_one_playtest", "run_seed", "enemy_policy"].contains(&key.as_str()),
            "Act-One Playtest cannot combine with fixture setup {key}");
    }
    if let Some(seed) = spec["setup"].get("run_seed") {
        anyhow::ensure!(seed.as_u64().is_some(), "Act-One run_seed must be a u64");
    }
    anyhow::ensure!(spec["setup"].get("enemy_policy").is_none()
        || spec["setup"]["enemy_policy"] == "live", "Act-One Playtest enemies must remain live");
    anyhow::ensure!(!spec["timeline"].as_array().into_iter().flatten()
        .any(|step|step["action"] == "incoming_impact" || step["action"] == "staged_discharge"),
        "Act-One Playtest cannot inject diagnostic damage or staged discharges");
    Ok(true)
}

fn ingest_capture_campaign(g: &mut Game, run: &mut Option<campaign::CampaignRun>) -> anyhow::Result<()> {
    let Some(run) = run.as_mut() else { return Ok(()); };
    for signal in g.drain_mission_signals() {
        match signal {
            game::MissionSignal::Objective(id, amount) => { run.record_objective_progress(id,amount)?; }
            game::MissionSignal::Discovery(site) => { run.record_discovery(site)?; }
        }
    }
    Ok(())
}

// Same requested gait priority as Foot::update; collision/headroom can still constrain motion.
fn requested_foot_speed(input: &TankInput) -> f32 {
    if input.crouch {foot::CROUCH_WALK} else if input.run {foot::RUN} else {foot::WALK}
}

#[derive(Default)]
struct FootMotion {
    path_m: f32,
    requested_m: f32,
    clipped_m: f32,
    clipped_ticks: u32,
}

impl FootMotion {
    fn record(&mut self, before: Vec3, after: Vec3, requested_m: f32) {
        let distance = glam::Vec2::new(after.x-before.x,after.z-before.z).length();
        self.path_m += distance;
        self.requested_m += requested_m;
        let shortfall = (requested_m - distance).max(0.0);
        self.clipped_m += shortfall;
        if shortfall > 0.001 {self.clipped_ticks += 1;}
    }
    fn add_to(&self, state: &mut Value, position: Vec3, initial: Vec3) {
        state["foot_displacement_m"] = json!(glam::Vec2::new(position.x-initial.x,position.z-initial.z).length());
        state["foot_path_m"] = json!(self.path_m);
        state["foot_requested_path_m"] = json!(self.requested_m);
        state["foot_motion_clipped_m"] = json!(self.clipped_m);
        state["foot_motion_clipped_ticks"] = json!(self.clipped_ticks);
    }
}

fn outcome_check(check: &Value, initial: &Value, observed_state: &Value) -> Value {
    let key=check["metric"].as_str().unwrap_or("");
    let observed=observed_state.get(key).cloned().unwrap_or(Value::Null);
    let delta=observed.as_f64().zip(initial[key].as_f64()).map(|(a,b)|a-b);
    let passed=if observed_state.get(key).is_none() {false}
        else if let Some(expected)=check.get("equals") {observed==*expected}
        else if check.get("min_value").is_some() || check.get("max_value").is_some() {
            observed.as_f64().is_some_and(|value| {
            value>=scalar(check,"min_value",f64::NEG_INFINITY)
                && value<=scalar(check,"max_value",f64::INFINITY)
        })
        } else {delta.is_some_and(|d| {
            d>=scalar(check,"min_delta",f64::NEG_INFINITY)
            && d<=scalar(check,"max_delta",f64::INFINITY)
        })};
    json!({"requirement":check,"initial":initial[key],"observed":observed,"delta":delta,"passed":passed})
}

/// A look target only: read the actual source handle/body and authored hinge, then turn the
/// existing head. The open-handle target is explicitly its fully open pose; it never opens a lid.
fn side_hatch_look_target(name: &str) -> anyhow::Result<Vec3> {
    let root = if name == "side_escape_aperture" {
        "escape hatch"
    } else {
        "Turret_0215"
    };
    let mut parts = crate::gltf_load::load_glb_partitioned_subtrees(
        "assets/tanks/tiger/baked/tiger_turret.glb",
        &[root],
    )?;
    let point = parts
        .pop()
        .ok_or_else(|| anyhow::anyhow!("source side hatch target missing"))?
        .center();
    if name != "side_escape_open_handle" {
        return Ok(point);
    }
    let source: Value = serde_json::from_slice(&crate::resource::read(
        "assets/tanks/tiger/baked/tiger_interior.json",
    )?)?;
    let hatch = &source["hatches"]["turret_escape"];
    let p = hatch["pos"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("authored side hatch hinge missing"))?;
    anyhow::ensure!(p.len() == 3, "invalid source hinge");
    let hinge = Vec3::new(
        p[0].as_f64().unwrap() as f32,
        p[1].as_f64().unwrap() as f32,
        p[2].as_f64().unwrap() as f32,
    );
    let yaw = hatch["rot_deg"][2]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("source hinge basis missing"))? as f32;
    let axis = glam::Quat::from_rotation_y(yaw.to_radians()) * Vec3::NEG_X;
    Ok(hinge + glam::Quat::from_axis_angle(axis, 95_f32.to_radians()) * (point - hinge))
}

fn action(g: &mut Game, r: &mut Renderer, step: &Value, map: &mut ReviewMap) -> anyhow::Result<(bool, Option<Value>)> {
    let name = step["action"].as_str().unwrap_or("");
    let mut detail=None;
    let accepted = match name {
        "hatch" => { g.toggle_hatch(); true },
        "dismount" => { let allowed=g.can_climb_out(); if allowed {g.climb_out();} allowed },
        "mount" => { let allowed=g.can_mount(); if allowed {g.toggle_dismount();} allowed },
        "head_out" => { let allowed=g.hatch_is_open(); if allowed {g.commander_outside=true;} allowed },
        "head_in" => {g.commander_outside=false;true},
        "station" => {g.request_station(station(step["value"].as_str().unwrap_or(""))?);true},
        "round" => g.interact_round(r.camera.forward()),
        "door" => g.interact_world_door(r.camera.forward(), r.camera.pos),
        "portable" => g.interact_portable_item(r.camera.forward(), r.camera.pos),
        "throw" => g.throw_portable_item(r.camera.forward()),
        "equip" => g.interact_equip(),
        "compass" => {
            let accepted = g.toggle_compass();
            g.consume_primary_press();
            accepted
        }
        "look_equipment" => g.debug_loader_face_equipment(step["value"].as_str().unwrap_or("")),
        "look_driver_control" => {
            detail = Some(json!({"scope":"Head rotation only toward source hull control; no position, station or mechanical state mutation"}));
            g.debug_face_driver_control(step["value"].as_str().unwrap_or(""))
        },
        "lay_on_at_gun" => {
            detail = Some(json!({"scope":"Disclosed lay fixture: the turret and gun are laid on the named anti-tank gun's shield (ballistic aim for the Pzgr 39) as a gunner would lay them; nothing is fired"}));
            g.debug_lay_on_at_gun(step["value"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0))
        },
        "look_at_gun" => {
            detail = Some(json!({"scope":"Head rotation only toward the named anti-tank gun"}));
            g.debug_face_at_gun(step["value"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0))
        },
        "look_cupola" => {
            detail = Some(json!({"scope":"Current operator head rotation only; no station, position or hatch-state change"}));
            g.debug_face_cupola(step["value"].as_str().unwrap_or(""))
        },
        "look_hatch" => {
            let name = step["value"].as_str().unwrap_or("");
            if matches!(name, "driver_lid" | "radio_lid") {
                let index = if name == "driver_lid" { 0 } else { 2 };
                let accepted = g.debug_face_front_hatch_lid(index);
                return Ok((accepted, Some(json!({"source_lid_index":index,
                    "scope":"Head rotation only toward current articulated source lid; no hatch state, phase or body movement"}))));
            }
            let aperture = match name {
                "driver_aperture" => Some(0),
                "loader_aperture" => Some(1),
                "radio_aperture" => Some(2),
                "side_escape_aperture" => Some(3),
                "cupola_aperture" => Some(4),
                _ => None,
            };
            if let Some(index) = aperture {
                detail = Some(json!({"source_aperture_index":index,
                    "scope":"Current operator head rotation only; authored aperture, no movement, station or hatch-state mutation"}));
                g.debug_face_hatch_aperture(index)
            } else {
                let target = side_hatch_look_target(name)?;
                detail = Some(json!({"source_target_turret_m":target.to_array(),
                    "scope":"Head rotation only; source handle target, no movement or hatch-state mutation"}));
                if g.dbg_is_foot() {
                    g.dbg_look_foot_at(g.player.turret_model().transform_point3(target));
                    true
                } else if g.mechanic_status()["current_station"] == "loader" {
                    g.debug_loader_face_local(target);
                    true
                } else {
                    false
                }
            }
        }
        "look_lever" => {g.debug_loader_face_lever();true},
        "look_breech" => {g.debug_loader_face_breech();true},
        "look_loaded_round" => {g.debug_loader_face_loaded_round();true},
        "look_tray" => {g.debug_loader_face_tray();true},
        "look_rack" => {g.debug_loader_face_rack();true},
        "look_fire" => {g.debug_loader_face_fire();true},
        "look_sentry" => {
            if g.dbg_is_foot() && g.n_soldiers()>0 {g.dbg_look_foot_at(g.soldier_pos(0)+Vec3::Y*1.2);true} else {false}
        },
        "look_left_track" => {
            if g.dbg_is_foot() {g.dbg_look_foot_at(g.player_track_anchor(game::ModuleKind::LeftTrack)+Vec3::Y*0.1);true} else {false}
        },
        "weapon_luger" => {g.select_foot_weapon(foot::FootWeapon::Luger);true},
        "weapon_panzerfaust" => {g.select_foot_weapon(foot::FootWeapon::Panzerfaust);true},
        "sight_zoom" => {g.sights_mag=(g.sights_mag+1)%3;true},
        "map_open" => {map.open=g.set_map_held(true);map.open},
        "map_close" => {g.set_map_held(false);map.open=false;map.primary=false;true},
        "map_zoom" => {
            if map.open {
                let before=g.held_map_zoom();
                g.zoom_held_map(scalar(step,"steps",0.0) as f32);
                detail=Some(json!({"before":before,"after":g.held_map_zoom(),
                    "scope":"Same zoom_held_map API as native wheel or +/-; no camera FOV or cartography mutation"}));
                true
            } else {false}
        },
        "map_pin" => {
            if map.open {ui::toggle_map_pin(&mut map.pins,g.act.id,map.cursor);true} else {false}
        },
        "staged_discharge" => {
            let before=g.main_gun_shot_sequence();
            let post=g.mechanic_status()["current_station"].clone();
            g.debug_staged_discharge(r);
            detail=Some(json!({"station":post,"shots_before":before,"shots_after":g.main_gun_shot_sequence(),
                "scope":"Disclosed discharge fixture: a serviceable round is seated and the gun fires along its current lay while the viewer stays at the current post (in a crew the gunner fires while the commander rides head-out; the solo player fires only from the sight). Round, report at the listener's ear, recoil and hearing follow the ordinary path."}));
            g.main_gun_shot_sequence()==before.wrapping_add(1)
        },
        "incoming_impact" => {
            let shell=incoming_shell(step["shell"].as_str().unwrap_or(""))?;
            let range=scalar(step,"range_m",0.0) as f32;
            let contact=g.debug_review_turret_side_impact(r,shell,range,
                step["physical_left"].as_bool().unwrap(),step["seed"].as_u64().unwrap() as u32)?;
            let sound=g.debug_last_player_damage_sound().map(|(p,c)|json!({
                "impact_params":{"shell":format!("{:?}",p.shell),"caliber_mm":p.caliber_mm,
                    "mass_kg":p.mass_kg,"velocity":p.velocity,"plate_thickness_mm":p.plate_thickness_mm,
                    "effective_mm":p.effective_mm,"impact_angle_deg":p.impact_angle_deg,
                    "residual_v":p.residual_v,"behind_armor":p.behind_armor,"panel":format!("{:?}",p.panel),
                    "penetrated":p.penetrated,"bearing_deg":p.bearing_deg,"station_dist_m":p.station_dist_m,"seed":p.seed},
                "damage_context":{"casualty_count":c.casualty_count,"module_hits":c.module_hits,
                    "internal_detonation":c.internal_detonation,"catastrophic":c.catastrophic}}));
            detail=Some(json!({"terminal_contact":contact.to_json(),"shell":shell.name,
                "range_m":range,"striking_velocity_m_s":shell.velocity_at(range),"audio_bridge":sound,
                "scope":"Disclosed isolated turret contact bench; common terminal resolver, no outcome filtering, no external flight replay."}));
            true
        },
        _ => anyhow::bail!("unsupported action {name}"),
    };
    Ok((accepted,detail))
}

/// Reproduce the live player's optic/interior composition, retaining Game::update's camera.
/// The ordinary periscope RTT and actual gunner-eye surround are both present.
/// `death_fade` 0 is an ordinary frame; above 0 the frame darkens toward black as the window's
/// death overlay does (see `render_death` in main.rs).
/// One 1920x1080 first-person gameplay frame exactly as the recorder draws it (optic composite,
/// crew view, pixel HUD; no caption), for other headless films such as `campaign-ai-run`'s.
pub(crate) fn gameplay_frame(g: &mut Game, r: &mut Renderer, device: &wgpu::Device,
    queue: &wgpu::Queue) -> anyhow::Result<Vec<u8>> {
    frame(g, r, device, queue, None, None, None, 0.0)
}

pub(crate) fn gameplay_native_frame(g: &mut Game, r: &mut Renderer, device: &wgpu::Device,
    queue: &wgpu::Queue) -> anyhow::Result<Vec<u8>> {
    let native=[r.iw,r.ih];
    frame_at_size(g,r,device,queue,None,None,None,0.0,native)
}

fn frame(g: &mut Game, r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue,
    map: Option<&ReviewMap>, caption: Option<&str>, witness: Option<CrewWitness>,
    death_fade: f32) -> anyhow::Result<Vec<u8>> {
    frame_at_size(g,r,device,queue,map,caption,witness,death_fade,[1920,1080])
}

fn frame_at_size(g: &mut Game, r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue,
    map: Option<&ReviewMap>, caption: Option<&str>, witness: Option<CrewWitness>,
    death_fade: f32, size:[u32;2]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(!g.chase_cam, "first-person recorder refuses chase/photo camera");
    r.begin_coop_shadow_frame(g.player.center());
    let shadow_ready=g.refresh_rtt_periscopes(r);
    r.set_coop_shadow_reuse(shadow_ready);
    let map = map.filter(|m|m.open);
    if let Some(map)=map {g.focus_held_map(map.cursor);}
    let map_visible = g.prepare_held_map(r, map.is_some());
    if let Some(map) = map.filter(|_| map_visible) {
        let heading=world::heading_of(glam::Vec2::new(r.camera.forward().x,r.camera.forward().z));
        let (route,sheets)=g.campaign_map_state().map_or((None,0),|(route,sheets)|(Some(route),sheets));
        r.ui_reset();
        // As in the window: the winter session carries its own sketch, otherwise the survey print.
        if !g.draw_canyon_map(r,&map.marks,&map.pins,map.cursor) && !g.draw_winter_map(r,&map.marks,&map.pins,map.cursor) {
            ui::draw_held_map_print(r,&g.act,&g.marks,&map.marks,&map.pins,route,sheets,map.cursor,heading,map.primary,g.map_world_position());
        }
        r.render_held_map_texture(device,queue,g.held_map_slot());
    }
    // Play carries no on-screen words (2026-09-28): no prompts, loading guide or map key hints.
    r.ui_reset();
    if let Some(caption)=caption {
        // Explicitly requested review annotation, using the game's pixel font. It is separate
        // from ordinary HUD controls and never feeds back into simulation or camera selection.
        r.fill_rect(0.015,0.922,0.55,0.052,[0.0,0.0,0.0,0.62]);
        r.draw_text(caption,0.026,0.948,0.023,[0.88,0.86,0.80,1.0],ui::Align::Left);
    }
    if death_fade > 0.0 { r.fill_rect(0.0,0.0,1.0,1.0,[0.0,0.0,0.0,death_fade.min(1.0)]); }
    let saved=(r.camera,r.reticle,r.optic_mask,r.precipitation,r.weather_shelter,r.interior_light,
        r.interior_haze,r.rt_mix,r.interior_atmosphere_enabled(),r.rt_skip);
    if g.protected_main_interior_view() {r.camera.zfar=PROTECTED_INTERIOR_FAR_M;}
    if let Some(witness)=witness {
        let parent=g.player.turret_model();
        r.camera.pos=parent.transform_point3(witness.eye);
        r.camera.set_forward(parent.transform_point3(witness.target)-r.camera.pos);
        r.camera.fov_y=72_f32.to_radians();r.camera.zfar=PROTECTED_INTERIOR_FAR_M;
        r.reticle=false;r.optic_mask=0.0;r.precipitation=0.0;r.weather_shelter=1.0;
        g.configure_crew_render_view(lan::LanRole::Loader,false,r);
        g.drive_singleplayer_crew_rigs_for_view(None,r);
    } else if g.optic_composite_active() {
        r.weather_shelter=1.0;
        g.configure_crew_render_view(lan::LanRole::Gunner,true,r);
        r.set_active_view(0);
        r.render_optic_snapshot(device,queue);
        let (eye,forward,fov)=g.gunner_interior_camera();
        r.camera.pos=eye;r.camera.set_forward(forward);r.camera.fov_y=fov;
        r.camera.zfar=PROTECTED_INTERIOR_FAR_M;
        r.precipitation=0.0;r.reticle=false;r.optic_mask=4.0;
        g.configure_crew_render_view(lan::LanRole::Gunner,false,r);
        g.apply_cinematic_camera_impulses(r);
        r.set_active_view(3);r.rt_skip=true;r.set_coop_shadow_reuse(true);
    }
    if witness.is_none() {g.draw_repair_cue(r);}
    let rgba=r.render_to_rgba_with_hud(device,queue,size[0],size[1]);
    r.camera=saved.0;r.reticle=saved.1;r.optic_mask=saved.2;r.precipitation=saved.3;
    r.weather_shelter=saved.4;r.interior_light=saved.5;r.interior_haze=saved.6;r.rt_mix=saved.7;
    r.set_interior_atmosphere_enabled(saved.8);r.rt_skip=saved.9;r.set_active_view(0);
    if witness.is_some() {g.drive_singleplayer_crew_rigs_for_view(g.singleplayer_embodied_role(),r);}
    r.end_coop_shadow_frame();
    rgba
}

fn crew_arm_contact_metrics(arms: &Value) -> Value {
    let contacts = |post: Option<&str>| {
        let errors: Vec<Option<f64>> = arms["actors"].as_array().into_iter().flatten()
            .filter(|actor| post.is_none_or(|post| actor["post"].as_str() == Some(post)))
            .flat_map(|actor| actor["hand_error_m"].as_array().into_iter().flatten())
            .map(|error| error.as_f64().filter(|v| v.is_finite() && *v >= 0.0)).collect();
        let maximum = (!errors.is_empty() && errors.iter().all(Option::is_some))
            .then(|| errors.iter().flatten().copied().fold(0.0_f64, f64::max));
        (errors.len(), maximum)
    };
    let (all_count, all_error) = contacts(None);
    let (loader_count, loader_error) = contacts(Some("LOADER"));
    let (gunner_count, gunner_error) = contacts(Some("GUNNER"));
    let (driver_count, driver_error) = contacts(Some("DRIVER"));
    json!({"crew_arm_animation_available":arms["available"],
        "loader_arm_preview":arms["loader_preview"],
        "crew_hand_targets":all_count,"crew_hand_target_error_m":all_error,
        "loader_hand_targets":loader_count,"loader_hand_target_error_m":loader_error,
        "gunner_hand_targets":gunner_count,"gunner_hand_target_error_m":gunner_error,
        "driver_hand_targets":driver_count,"driver_hand_target_error_m":driver_error})
}

fn telemetry(g: &Game, r: &Renderer) -> Value {
    let combat=g.mixed_combat_telemetry();
    let (surface_foam_deposits, surface_fire_coverage, ammunition_heat) = g.debug_surface_fire();
    let (surface_fire_patches, surface_fire_wall_patches, burning_ammunition_id) = g.debug_fire_spread();
    let (pressure_pending, pressure_delivered, pressure_arrival_s) = g.debug_pressure_arrivals();
    let combat_fields = json!({"mixed_combat":combat,
        "german_rifle_shots":combat["rifle_shots"][0],"soviet_rifle_shots":combat["rifle_shots"][1],
        "german_combat_damage":combat["damage"][0],"soviet_combat_damage":combat["damage"][1],
        "tank_aux_mg_shots":combat["tank_mg_shots"],"tank_aux_mg_damage":combat["tank_mg_damage"],
        "tank_aux_he_shots":combat["tank_he_shots"],"tank_aux_he_impacts":combat["tank_he_impacts"],
        "tank_aux_he_damage":combat["tank_he_damage"],
        "german_combat_learned":combat["german_learned"],"soviet_combat_learned":combat["soviet_learned"],
        "tank_combat_learned":combat["tank_learned"],"combat_explicit_weights":combat["explicit_override"]});
    let mut state = json!({"world_doors":g.debug_world_doors(),"player_world_m":g.player.pos.to_array(),"speed_m_s":g.player.speed,
        "main_gun_shots":g.main_gun_shot_sequence(),"foot_world_m":g.foot_pos().to_array(),
        "turret_yaw_rad":g.player.turret_yaw,"on_foot":g.dbg_is_foot(),
        "driver_visor_lean":g.driver_visor_lean_fraction(),
        "driver_visor_open":g.driver_visor_open(),"driver_visor_openness":g.driver_visor_open_fraction(),
        "driver_visor_eye_hull_m":g.driver_visor_eye_hull_local().to_array(),
        "driver_wide_view":g.driver_wide_view_active(),
        "camera_horizontal_fov_deg":(2.0*((r.camera.fov_y*0.5).tan()*r.camera.aspect).atan()).to_degrees(),
        "camera_optic_mask":r.optic_mask,
        "camera_hull_m":g.player.hull_model().inverse().transform_point3(r.camera.pos).to_array(),
        "camera_forward_world":r.camera.forward().to_array(),
        "driver_controls":g.driver_controls_receipt(),
        "driver_steering_rad":g.driver_controls_receipt()["steering_rad"],
        "hatch_open":g.hatch_is_open(),"engine_on":g.engine_on,"fire":g.fire_level(),
        "foam_active":g.debug_foam_counts().0,"foam_airborne":g.debug_foam_counts().1,
        "breech":g.breech_label(),"breech_loaded":g.breech_loaded(),"lever":g.debug_lever(),
        "carrying_round":g.debug_carrying(),"carrying_case":g.debug_casing_carried(),
        "rack_rounds":g.debug_rack_left(),"active_casings":g.active_casings(),
        "foot_hp":g.foot_hp(),"p08_rounds":g.foot_p08_rounds(),"panzerfaust":g.debug_panzerfaust_inventory(),
        "panzerfaust_carried_tubes":g.debug_panzerfaust_inventory().1,
        "living_soldiers":g.soldiers_alive(),"supplies":g.supplies_total(),"climbing":g.is_climbing(),
        "soldier_0_looted":(g.n_soldiers()>0).then(||g.soldier_looted(0)),
        "soldier_0_world_m":(g.n_soldiers()>0).then(||g.soldier_pos(0).to_array()),
        "mg_rounds":g.mg_round_count(),"tank_alive":g.player.alive});
    state["game_over"] = json!(g.game_over().is_some());
    state["death_cause"] = json!(g.game_over().map(|cause|format!("{cause:?}")));
    // What the Tiger's tracks meet (solid city, 2026-09-29): the hull's tilt, what carries it,
    // the limit refusing its move, and the extremes so far.
    let ground = g.tank_ground_state();
    let ground_fields = json!({
        "tank_pitch_deg": ground.pitch_deg, "tank_roll_deg": ground.roll_deg,
        "tank_above_terrain_m": ground.above_terrain_m, "tank_carried_by": ground.carried_by,
        "tank_refused": ground.refused, "tank_refused_value": ground.refused_value,
        "tank_obstacle": ground.obstacle, "tank_crushing": ground.crushing.is_some(),
        "tank_floors_gave_way": ground.gave_way, "tank_falling": ground.falling,
        "tank_embedded": ground.embedded, "tank_max_climbed_step_m": ground.max_climbed_step_m,
        "tank_max_pitch_deg": ground.max_pitch_deg,
        "tank_max_rubble_above_terrain_m": ground.max_rubble_above_terrain_m,
    });
    state.as_object_mut().unwrap().extend(ground_fields.as_object().unwrap().clone());
    let diagnostic = g.damage_readout();
    state["tank_aux_inventory"] = combat["tanks"].clone();
    state["couch_living_crew"] = json!(g.couch_living_crew());
    state["singleplayer_living_crew"] = json!(g.singleplayer_living_crew());
    let arms=g.crew_arm_pose_receipt();
    state.as_object_mut().unwrap().extend(crew_arm_contact_metrics(&arms).as_object().unwrap().clone());
    state["crew_arm_pose"]=arms;
    state["map_held_3d"] = json!(g.map_held);
    state["held_map_zoom"] = json!(g.held_map_zoom());
    state["campaign_route_active"] = json!(g.campaign_map_state().is_some());
    state["practice_return_fire"] = json!(g.playtest_return_fire());
    state["live_ammunition_count"] = json!(diagnostic.ammunition.len());
    state["left_track_functional"] = json!(g.is_module_functional(game::ModuleKind::LeftTrack));
    state["track_repair_phase"] = json!(g.track_repair_phase_label());
    state["track_pull"] = json!(g.track_pull_progress());
    state["module_state"] = json!(diagnostic.components.iter().map(|m|json!({
        "name":m.label,"functional":m.functional})).collect::<Vec<_>>());
    state["crew_state"] = g.singleplayer_crew_state_receipt().unwrap_or_else(||
        json!(diagnostic.crew.iter().map(|c|json!({"identity":c.identity,
            "incapacitated":c.incapacitated,"has_anatomical_record":c.has_anatomical_record})).collect::<Vec<_>>()));
    let (trigger_pulls, primer_misfires, chamber_primer_strikes, faulty_cartridges) = g.debug_primer_state();
    state["main_trigger_pulls"] = json!(trigger_pulls);
    state["primer_misfires"] = json!(primer_misfires);
    state["chamber_primer_fault"] = json!(chamber_primer_strikes.is_some());
    state["chamber_primer_strikes"] = json!(chamber_primer_strikes.unwrap_or(0));
    state["faulty_cartridges"] = json!(faulty_cartridges);
    state["loose_rounds"] = json!(g.loose_shells_active());
    state["surface_blood"] = g.debug_surface_blood();
    state.as_object_mut().unwrap().extend(g.debug_enemy_report_telemetry().as_object().unwrap().clone());
    state["ground_cover"] = g.world_survey_grass_stats();
    let doors = g.debug_world_doors();
    for field in ["open_pairs", "moving_pairs", "blocked_apertures", "physical_moving_rt"] {
        state[format!("world_door_{field}")] = doors[field].clone();
    }
    state["grass_near_2_5_m"] = state["ground_cover"]["within_2_5_m"].clone();
    state["grass_class_loaded"] = state["ground_cover"]["ground_class_loaded"].clone();
    state["foot_eye_above_feet_m"] = json!(r.camera.pos.y - g.foot_pos().y);
    state["standing_height_m"] = json!(crate::foot::STANDING_HEIGHT);
    state.as_object_mut().unwrap().extend(combat_fields.as_object().unwrap().clone());
    let fire_fields = json!({"surface_foam_deposits":surface_foam_deposits,
        "surface_fire_coverage":surface_fire_coverage,"ammunition_heat":ammunition_heat,
        "surface_fire_patches":surface_fire_patches,"surface_fire_wall_patches":surface_fire_wall_patches,
        "ammunition_ignited":burning_ammunition_id.is_some(),"burning_ammunition_id":burning_ammunition_id,
        "pressure_pending":pressure_pending,"pressure_delivered":pressure_delivered,
        "pressure_last_arrival_s":pressure_arrival_s});
    state.as_object_mut().unwrap().extend(fire_fields.as_object().unwrap().clone());
    let world_fields = json!({"tank_fuel_liters":g.tank_fuel_liters(),
        "foot_x_m":g.foot_pos().x,"foot_y_m":g.foot_pos().y,"foot_z_m":g.foot_pos().z});
    state.as_object_mut().unwrap().extend(world_fields.as_object().unwrap().clone());
    state.as_object_mut().unwrap().extend(g.portable_fuel_telemetry().as_object().unwrap().clone());
    state.as_object_mut().unwrap().extend(g.mechanic_status().as_object().unwrap().clone());
    state["carried_hammer"] = g.carried_hammer_receipt();
    state["hammer_swing_remaining_s"] = state["carried_hammer"]["swing_remaining_s"].clone();
    state["hammer_target_job"] = state["carried_hammer"]["target_job"].clone();
    state["hammer_track_pin_hits"] = state["carried_hammer"]["track_pin_hits"].clone();
    state["cupola_focus"] = g.cupola_focus_receipt();
    state["cupola_focus_index"] = state["cupola_focus"]["index"].clone();
    state["cupola_focus_phase"] = state["cupola_focus"]["phase"].clone();
    state["cupola_blocks"] = g.exterior_damage_receipt()["cupola_blocks"].clone();
    state.as_object_mut().unwrap().extend(g.world_mechanic_status(r).as_object().unwrap().clone());
    state.as_object_mut().unwrap().extend(g.destruction_mechanic_status().as_object().unwrap().clone());
    state
}

pub fn run(prefix: &str, scenario_path: &str) -> anyhow::Result<()> {
    run_mode(prefix,scenario_path,true)
}

/// Fast scenario integration check using the same Game, inputs and simulation/audio clocks.
/// Renderer resources are constructed for gameplay meshes, but no film or audio file is exported.
/// This cannot establish visible behavior and never enters the video gallery as capture evidence.
pub fn evaluate(prefix: &str, scenario_path: &str) -> anyhow::Result<()> {
    run_mode(prefix,scenario_path,false)
}

/// Reject the old partial fixture that relocated a campaign roster without constructing the
/// actual Playtest rigs and practice policy. Check before any declared staging/banishment.
fn validate_constructed_playtest(return_fire: Option<bool>, kinds: &[tank::EnemyKind]) -> anyhow::Result<()> {
    anyhow::ensure!(return_fire == Some(false), "Playtest fixture did not construct safe practice mode");
    anyhow::ensure!(kinds.len() == 6 && kinds.iter().all(|kind| *kind == tank::EnemyKind::T34),
        "Playtest fixture did not construct its six T-34 targets");
    Ok(())
}

fn capture_timing_summary(samples: &[f64]) -> Value {
    if samples.is_empty() {
        return json!({"count":0,"total_ms":0.0,"mean_ms":null,"min_ms":null,
            "median_ms":null,"p95_ms":null,"p99_ms":null,"max_ms":null});
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let quantile = |fraction: f64| {
        let position = (sorted.len() - 1) as f64 * fraction;
        let low = position.floor() as usize;
        let high = position.ceil() as usize;
        sorted[low] + (sorted[high] - sorted[low]) * (position - low as f64)
    };
    let total = sorted.iter().sum::<f64>();
    json!({"count":sorted.len(),"total_ms":total,"mean_ms":total / sorted.len() as f64,
        "min_ms":sorted[0],"median_ms":quantile(0.5),"p95_ms":quantile(0.95),
        "p99_ms":quantile(0.99),"max_ms":sorted[sorted.len()-1]})
}

fn capture_work_json(work: renderer::RenderWorkStats) -> Value {
    json!({"scene_renders":work.scene_renders,"shadow_passes":work.shadow_passes,
        "shadow_mesh_draws":work.shadow_mesh_draws,"shadow_instanced_draws":work.shadow_instanced_draws,
        "scene_mesh_draws":work.scene_mesh_draws,"scene_instanced_draws":work.scene_instanced_draws,
        "indexed_draws":work.indexed_draws(),"fullscreen_raster_passes":work.fullscreen_raster_passes,
        "compute_passes":work.compute_passes,"smoke_passes":work.smoke_passes,
        "bloom_passes":work.bloom_passes,"snow_passes":work.snow_passes})
}

fn run_mode(prefix: &str, scenario_path: &str, record: bool) -> anyhow::Result<()> {
    let spec=validate_file(scenario_path)?;
    require_loader_preview(&spec, std::env::var("KESSEL_LOADER_ARM_PREVIEW").as_deref()==Ok("1"))?;
    let act_one_playtest=validate_act_one_playtest(&spec)?;
    let retained_diagnostic=spec["diagnostic_view"]=="retained_model";
    anyhow::ensure!(spec.get("native_output").is_none_or(Value::is_boolean),
        "native_output must be a boolean");
    let native_output=spec["native_output"].as_bool().unwrap_or(false);
    anyhow::ensure!(!native_output || !retained_diagnostic,
        "native_output is supported for ordinary gameplay views only");
    let diagnostic_crew=spec["setup"]["diagnostic_crew"]==true;
    let showcase_crew=ShowcaseCrew::from_spec(&spec)?;
    let crew_witness=CrewWitness::from_spec(&spec)?;
    // This process is a dedicated recorder. Device-free Audio must be selected before Game build.
    std::env::set_var("KESSEL_AUDIO_CAPTURE","1");
    let instance=create_instance();let adapter=request_adapter(&instance,None);
    let (device,queue)=request_device(&adapter);
    if let Some(path) = spec["world_manifest"].as_str() {
        act_manifest::select_runtime_manifest(Some(explicit_manifest_path(path)?))?;
    }
    let mut act=world::act(spec["act_id"].as_u64().unwrap() as u8);
    let mut campaign_run = if act_one_playtest {
        let manifest=act_manifest::ActManifest::load_for_act(1)?;
        act=world::resolve_manifest(act,&manifest)?;
        let seed=spec["setup"]["run_seed"].as_u64().unwrap_or(20_260_913);
        act.seed ^= seed as u32 ^ (seed >> 32) as u32;
        Some(campaign::CampaignRun::new(seed))
    } else { None };
    let campaign_start=campaign_run.as_ref().map(|run|run.prepare_act_attempt(&act)).transpose()?;
    let playtest = spec["setup"]["playtest"].as_bool().unwrap_or(false);
    let night = spec["night"].as_bool().unwrap_or(false);
    let (mut r,mut g) = if let Some(start)=campaign_start.as_ref() {
        Game::build_campaign_act(&device,&queue,night,&act,start)?
    } else if playtest {
        Game::build_playtest_act(&device, &queue, night, &act)?
    } else {
        Game::build_act(&device, &queue, night, &act)?
    };
    if playtest {
        validate_constructed_playtest(g.playtest_return_fire(), &g.enemies.iter().map(|e| e.kind).collect::<Vec<_>>())?;
        g.playtest_setup();
    }
    if spec["setup"]["winter_session"]==true {
        // The same live encounter the window's winter launcher starts: Soviet platoons on their
        // patrols, the 45 mm guns silent until the fight begins, the untimed field exercise.
        g.enable_live_playtest_encounter();
        anyhow::ensure!(g.winter_mission_snapshot().is_some(),
            "winter_session needs the authored winter courtyard approach map");
    }
    if spec["setup"]["canyon_session"]==true {
        // The course as the title menu's Canyon entry fields it (src/canyon.rs).
        g.enable_canyon_course()?;
    }
    if let Some(start)=campaign_start.as_ref() {
        g.configure_campaign(start,&mut r)?;
        campaign_run.as_mut().unwrap().commit_prepared_attempt(start)?;
        g.enable_singleplayer_crew();
        anyhow::ensure!(!g.god && g.playtest_return_fire().is_none()
            && g.singleplayer_living_crew()==Some(1) && g.campaign_map_state().is_some(),
            "Act-One Playtest must have a live campaign route and its one mortal crewman");
    }
    if spec["setup"]["singleplayer_crew"]==true {g.enable_singleplayer_crew();}
    let construction_receipt = json!({
        "constructor": if act_one_playtest { "Game::build_campaign_act" }
            else if playtest { "Game::build_playtest_act" } else { "Game::build_act" },
        "act_one_playtest":act_one_playtest,
        "winter_session":g.winter_mission_snapshot(),
        "canyon_session":g.canyon_snapshot(),
        "campaign_run_seed":campaign_run.as_ref().map(|run|run.run_seed),
        "campaign_attempt":campaign_start.as_ref().map(|start|start.attempt_number),
        "campaign_route_seed":campaign_start.as_ref().map(|start|start.route.attempt_seed),
        "campaign_spawn_xz":campaign_start.as_ref().map(|start|start.spawn.to_array()),
        "singleplayer_living_crew":g.singleplayer_living_crew(),
        "god":g.god,
        "checkpoint_written":false,
        "selected_manifest":act_manifest::selected_runtime_manifest().map(|(_,path)|path),
        "playtest_return_fire": g.playtest_return_fire(),
        "enemy_roster": g.enemies.iter().map(|enemy| enemy.kind.name()).collect::<Vec<_>>(),
        "scope": "Initial construction before explicitly requested fixture staging or enemy policy changes"
    });
    if let Some(xz) = spec["setup"]["player_xz"].as_array() {
        let (x, z) = (xz[0].as_f64().unwrap() as f32, xz[1].as_f64().unwrap() as f32);
        g.player.pos = Vec3::new(x, g.hf.height_at(x, z), z);
    }
    if let Some(yaw) = spec["setup"]["player_heading_deg"].as_f64() {
        anyhow::ensure!(yaw.is_finite(), "player heading must be finite");
        g.player.heading = (yaw as f32).to_radians();
    }
    // Native output preserves one encoded pixel per scene pixel for effects/dither review.
    anyhow::ensure!(matches!((r.iw,r.ih),(640,360)|(576,324)|(800,450)),
        "first-person review requires KESSEL_INTERNAL_RES=640x360, 576x324 or 800x450");
    let output_size=if native_output {[r.iw,r.ih]} else {[1920,1080]};
    let output_dimensions=format!("{}x{}",output_size[0],output_size[1]);
    let output_pixels=output_size[0] as usize*output_size[1] as usize;
    g.chase_cam=false;g.jump_station(station(spec["station"].as_str().unwrap_or("commander"))?);
    if let Some(crew) = showcase_crew { g.set_lan_role(crew.role); }
    g.set_defer_rtt_to_render(true);
    match spec["setup"]["enemy_policy"].as_str().unwrap_or(if act_one_playtest {"live"} else {"banish"}) {
        "banish"=>g.debug_banish_enemies(),"hold_fire"=>g.debug_hold_enemy_fire(),
        "live"=>{if playtest {g.set_playtest_return_fire(true);}},
        other=>anyhow::bail!("unknown enemy policy {other}"),
    }
    if duel_setup(&spec).is_some() {
        // Discarded warmup has no live duel; one real opponent is placed only after it finishes.
        g.debug_banish_enemies();
    }
    if spec["setup"]["on_foot"].as_bool().unwrap_or(false) {
        // Open the occupied post's hatch and let its ordinary animation finish before using
        // the real dismount transition. Merely setting the cupola target left hatch_t at zero,
        // so the source-aperture exit gate correctly refused the old exterior setup.
        g.update(
            &TankInput {
                hatch_toggle: true,
                ..Default::default()
            },
            1.0 / SIM_HZ as f32,
            &mut r,
        );
        for _ in 0..SIM_HZ {
            g.update(&TankInput::default(), 1.0 / SIM_HZ as f32, &mut r);
            g.render_audio_capture_frames((acoustics::SAMPLE_RATE / SIM_HZ) as usize)?;
        }
        anyhow::ensure!(
            g.can_climb_out(),
            "on-foot review setup could not open its occupied station hatch"
        );g.climb_out();
        anyhow::ensure!(
            g.dbg_is_foot(),
            "on-foot review setup did not complete the ordinary dismount"
        );
        let p=g.player.pos;
        let foot = spec["setup"]["foot_xz"].as_array().map(|a|
            [a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32])
            .unwrap_or([p.x+3.5, p.z]);
        let target = spec["setup"]["look_at_xz"].as_array().map(|a|
            [a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32])
            .unwrap_or([p.x, p.z-15.0]);
        g.dbg_place_foot(foot[0], foot[1]);
        g.face_foot_toward(target[0], target[1]);
    }
    if spec["setup"]["hatch_open"].as_bool().unwrap_or(false) {g.dbg_open_hatch();}
    if spec["setup"]["fire"].as_bool().unwrap_or(false) {g.debug_start_fire();}
    if let Some(name)=spec["setup"]["broken_module"].as_str() {g.debug_break_module(module_kind(name)?);}
    if spec["setup"]["exterior_damage_review"].as_bool().unwrap_or(false) {
        // Disclosed fixture: staged strikes through the ordinary exterior-damage path.
        println!("[mechanic] exterior damage staged: {}", g.debug_stage_exterior_damage_review());
    }
    if spec["setup"]["track_repair_post"].as_bool().unwrap_or(false) {
        anyhow::ensure!(g.dbg_is_foot(),"track bench requires ordinary on-foot setup");
        // Fixture setup is explicit: this does not claim the toolbox pickup was demonstrated.
        g.dbg_take_track_kit();
        let anchor=g.player_track_anchor(game::ModuleKind::LeftTrack);
        let mut outboard=anchor-g.player_center();outboard.y=0.0;
        let post=anchor+outboard.normalize_or_zero()*2.0;
        g.dbg_place_foot(post.x,post.z);g.dbg_look_foot_at(anchor+glam::Vec3::Y*0.1);
    }
    if let Some(index)=spec["setup"]["aim_enemy"].as_u64() {g.debug_aim_at_enemy_turret(index as usize);}
    // Let ordinary loops and camera state settle; consume and discard those exact warmup samples.
    for i in 0..SIM_HZ {
        let controls=capture_crew_controls(TankInput::default(),&g,diagnostic_crew,showcase_crew);
        g.update(&controls,1.0/SIM_HZ as f32,&mut r);
        ingest_capture_campaign(&mut g,&mut campaign_run)?;
        let samples=((i+1) as u64*acoustics::SAMPLE_RATE as u64/SIM_HZ as u64)
            -(i as u64*acoustics::SAMPLE_RATE as u64/SIM_HZ as u64);
        g.render_audio_capture_frames(samples as usize)?;
    }
    // New combat rosters are staged after warmup so their opening shots remain in the film.
    let mut staged_combat=false;
    let mut setup_results = serde_json::Map::new();
    if let Some(crew) = showcase_crew { setup_results.insert("showcase_crew".into(),crew.receipt()); }
    if let Some(setup) = duel_setup(&spec) {
        setup_results.insert("enemy_discharge_review".into(),g.debug_stage_enemy_discharge_review(
            scalar(setup,"range_m",0.0) as f32,scalar(setup,"bearing_deg",0.0) as f32,
            scalar(setup,"readiness_s",1.0) as f32,r.time)?);
        staged_combat=true;
    }
    if spec["setup"]["primer_fault"] == "chamber" {
        setup_results.insert("primer_fault".into(), g.debug_stage_primer_fault()?);
    }
    if let Some(setup) = spec["setup"].get("source_fire") {
        let point = |key: &str| { let a = setup[key].as_array().unwrap();
            Vec3::new(a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32, a[2].as_f64().unwrap() as f32) };
        setup_results.insert("source_fire".into(), g.debug_start_surface_fire(setup["frame"].as_str().unwrap(),
            point("ray_start_m"), point("ray_end_m"), scalar(setup,"strength",0.5) as f32)?);
        if spec["station"] == "loader" { g.debug_loader_face_fire(); }
        staged_combat = true;
    }
    if let Some(setup)=spec["setup"].get("canyon_stage") {
        setup_results.insert("canyon_stage".into(),g.debug_stage_canyon_fight(scalar(setup,"gun",0.0) as usize,
            scalar(setup,"range_m",300.0) as f32, setup["lay_on_gun"].as_bool().unwrap_or(false))?);
        staged_combat=true;
    }
    if let Some(setup)=spec["setup"].get("at_gun_review") {
        setup_results.insert("at_gun_review".into(),g.debug_stage_at_gun_review(scalar(setup,"range_m",240.0) as f32,
            setup["lay_on_gun"].as_bool().unwrap_or(false))?);
        staged_combat=true;
    }
    if let Some(fixture)=spec["setup"]["destruction_review"].as_str() {
        setup_results.insert("destruction_review".into(),g.debug_stage_destruction_review(fixture)?);
        staged_combat=true;
    }
    if let Some(fixture)=spec["setup"]["feature_building"].as_str() {
        setup_results.insert("feature_building".into(),g.debug_stage_feature_building(fixture)?);
        staged_combat=true;
    }
    if let Some(setup)=spec["setup"].get("sentry_target") {
        anyhow::ensure!(g.n_soldiers()>0,"sentry_target requires an existing soldier; isolated scene has none");
        let point=|key:&str| {let a=setup[key].as_array().unwrap();
            (a[0].as_f64().unwrap() as f32,a[1].as_f64().unwrap() as f32)};
        let (sx,sz)=point("pos_xz");let (fx,fz)=point("facing_xz");
        g.dbg_stage_soldier(0,sx,sz,fx,fz);
        g.dbg_look_foot_at(g.soldier_pos(0)+Vec3::Y*1.2);
        setup_results.insert("sentry_target".into(),json!({"index":0,"alive_at_setup":true,
            "position_m":g.soldier_pos(0).to_array(),"scope":"Living sentry staged before recording; no damage, corpse or loot result forced."}));
        staged_combat=true;
    }
    if spec["setup"]["playtest_refuel"] == true {
        setup_results.insert("playtest_refuel".into(), g.debug_stage_playtest_refuel()?);
        staged_combat = true;
    }
    if let Some(setup)=spec["setup"].get("mixed_combat") {
        let xy=setup["center_xz"].as_array().unwrap();
        let center=Vec3::new(xy[0].as_f64().unwrap() as f32,0.0,xy[1].as_f64().unwrap() as f32);
        g.debug_stage_mixed_combat(center);
        staged_combat=true;
    }
    if let Some(setup)=spec["setup"].get("soft_target_tank") {
        let point=|key:&str| {let a=setup[key].as_array().unwrap();
            Vec3::new(a[0].as_f64().unwrap() as f32,0.0,a[1].as_f64().unwrap() as f32)};
        let kind=if setup["kind"]=="kv2" {tank::EnemyKind::Kv2} else {tank::EnemyKind::T34};
        let mut target=point("target_xz");target.y=g.hf.height_at(target.x,target.z)+0.9;
        anyhow::ensure!(g.debug_stage_soft_target_tank(setup["index"].as_u64().unwrap() as usize,
            kind,point("pos_xz"),target,setup["he_ammo"].as_u64().unwrap_or(8) as u32),"failed to stage real tank rig");
        setup_results.insert("soft_target_tank".into(),json!({"index":setup["index"],"kind":setup["kind"],
            "he_ammunition":setup["he_ammo"].as_u64().unwrap_or(8),"mg_ammunition":300,
            "scope":"Initial live auxiliary inventory only; ordinary candidate observation, action selection, aim, ammunition depletion and impacts."}));
        staged_combat=true;
    }
    if staged_combat {
        // One ordinary tick submits live actor/tank models and the actual player camera.
        let controls=capture_crew_controls(TankInput::default(),&g,diagnostic_crew,showcase_crew);
        g.update(&controls,1.0/SIM_HZ as f32,&mut r);
        g.render_audio_capture_frames((acoustics::SAMPLE_RATE/SIM_HZ) as usize)?;
    }
    let mut model_view=(record && retained_diagnostic).then(||
        game::damage_model_view::Gpu::new(&device,&mut r));
    r.camera.aspect=16.0/9.0;
    let pause_ms=std::env::var("KESSEL_CAPTURE_FRAME_PAUSE_MS").ok()
        .and_then(|s|s.parse::<u64>().ok()).unwrap_or(250).clamp(1,5000);
    let idle_factor=std::env::var("KESSEL_CAPTURE_IDLE_FACTOR").ok()
        .and_then(|s|s.parse::<f32>().ok()).filter(|v|v.is_finite())
        .unwrap_or(3.0).clamp(0.0,10.0);
    for _ in 0..if record && !retained_diagnostic {24} else {0} {
        let started=Instant::now();let _=frame_at_size(&mut g,&mut r,&device,&queue,None,None,crew_witness,0.0,output_size)?;
        std::thread::sleep(started.elapsed().mul_f32(idle_factor)
            .max(std::time::Duration::from_millis(pause_ms)).min(std::time::Duration::from_secs(5)));
    }
    if record && std::env::var("KESSEL_CAPTURE_RASTER_OPTICS").as_deref() == Ok("1") {
        r.debug_capture_raster_optics(&device,&queue,&format!("{prefix}_pre"))?;
    }
    let mut map=ReviewMap::default();
    let mut primary_was_held = false;
    if duel_setup(&spec).is_some() {
        g.debug_enemy_report_recording_origin(r.time)?;
    }
    let mut initial=telemetry(&g,&r);initial["player_displacement_m"]=json!(0.0);map.add_to(&mut initial);
    let initial_foot = g.foot_pos();
    let mut foot_motion = FootMotion::default();
    foot_motion.add_to(&mut initial,initial_foot,initial_foot);
    let initial_position=g.player.pos;let initial_audio=g.audio_capture_metadata();
    if record && std::env::var_os("KESSEL_CAPTURE_OPTIC_PROBE").is_some() {
        let origin = r.camera.pos;
        let forward = r.camera.forward();
        let probe = r.probe_daylight_connection(&device, &queue, origin, forward)?;
        std::fs::write(format!("{prefix}_optic_probe.json"), serde_json::to_vec_pretty(&probe)?)?;
    }
    if record && std::env::var_os("KESSEL_CAPTURE_LIGHT_LAYERS").is_some() {
        r.debug_capture_light_layers(&device,&queue,&format!("{prefix}_pre_capture"))?;
    }
    if let Some(parent)=std::path::Path::new(prefix).parent(){std::fs::create_dir_all(parent)?;}
    let raw=format!("{prefix}_video_only.mp4");let wav=format!("{prefix}_soundtrack.wav");
    let output=format!("{prefix}.mp4");let poster=format!("{prefix}.png");
    anyhow::ensure!(!std::path::Path::new(&output).exists(),"refusing to overwrite {output}");
    let mut encoder=if record {Some(std::process::Command::new("ffmpeg").args(["-v","error","-nostdin","-y",
        "-f","rawvideo","-pixel_format","rgba","-video_size",&output_dimensions,"-framerate","24",
        "-i","pipe:0","-an","-c:v","libx264","-threads","1","-preset","veryfast","-crf","12",
        "-pix_fmt","yuv420p","-movflags","+faststart",&raw]).stdin(std::process::Stdio::piped()).spawn()?)} else {None};
    let frames=(scalar(&spec,"seconds",8.0)*FPS as f64).round() as u32;
    let diagnostic_layer_frame = std::env::var("KESSEL_CAPTURE_LIGHT_LAYERS_AT_S").ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|seconds| (seconds * FPS as f32).round() as u32);
    let timeline=spec["timeline"].as_array().unwrap();let mut performed=vec![false;timeline.len()];
    let mut samples=Vec::new();let mut events=Vec::new();let mut states=Vec::new();
    let mut kia_at = g.game_over().map(|_|0.0_f32);
    let mut kia_audio_at_fatal = kia_at.map(|_|g.debug_scheduled_audio());
    let capture_profile_enabled = std::env::var("KESSEL_CAPTURE_PROFILE").as_deref() == Ok("1");
    let mut render_readback_ms = Vec::new();
    let mut simulation_batch_ms = Vec::new();
    let mut timing_frames = Vec::new();
    let work_before_capture = r.render_work_stats();
    let result=(||->anyhow::Result<()> {
        for f in 0..frames {
            let started=Instant::now();
            let mut profile_kind = "simulation_only";
            let mut profile_render_ms = None;
            let mut profile_render_work = renderer::RenderWorkStats::default();
            if let Some(encoder)=encoder.as_mut() {
                let now = f as f32 / FPS as f32;
                let render_start = capture_profile_enabled.then(|| (Instant::now(), r.render_work_stats()));
                let rgba = if let Some(phase)=kia_at.and_then(|start|crate::terminal_visual_phase(now-start))
                    .filter(|_|g.last_fatal.is_some()) {
                    profile_kind="kia_recorded_replay";
                    g.render_replay(phase,&mut r);
                    let mut pixels=r.render_replay_rgba(&device,&queue,now-kia_at.unwrap(),
                        renderer::ReplayCapturePolicy::Current)?;
                    let since = now-kia_at.unwrap();
                    if since < crate::DEATH_FADE {
                        for pixel in pixels.chunks_exact_mut(4) { for c in &mut pixel[..3] {
                            *c = (*c as f32*(1.0-crate::death_fade(since))).round() as u8;
                        }}
                    }
                    let native=image::RgbaImage::from_raw(r.iw,r.ih,pixels).unwrap();
                    image::imageops::resize(&native,output_size[0],output_size[1],image::imageops::FilterType::Nearest).into_raw()
                } else if kia_at.is_some_and(|start| now - start >= crate::DEATH_FADE) {
                    // No recorded shot, or completed replay: hold black. Never invent a hit.
                    profile_kind = "kia_black_no_gpu_render";
                    let mut black = vec![0u8; output_pixels * 4];
                    for pixel in black.chunks_exact_mut(4) { pixel[3] = 255; }
                    black
                } else if kia_at.is_some() {
                    profile_kind = "kia_black_no_fatal_record";
                    [0, 0, 0, 255].repeat(output_pixels)
                } else if let Some(view) = model_view.as_mut() {
                    profile_kind = "retained_model_diagnostic";
                    let appendix=scalar(&spec,"diagnostic_anatomy_last_s",0.0) as f32;
                    if appendix>0.0 && now>=scalar(&spec,"seconds",0.0) as f32-appendix {
                        r.ui_reset();g.damage_readout().draw_anatomy(&mut r,16.0/9.0);
                    } else { view.draw(&g,&mut r,&device,&queue,now)?; }
                    view.capture_rgba(&mut r,&device,&queue)?
                } else {
                    profile_kind = "gameplay";
                    frame_at_size(&mut g,&mut r,&device,&queue,Some(&map),spec["caption"].as_str(),crew_witness,0.0,output_size)?
                };
                if let Some((start, work_before)) = render_start {
                    // render_to_rgba waits for GPU completion and copies mapped RGBA rows. Stop
                    // before PNG writing, encoder pipe writes/backpressure, diagnostics and sleep.
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    profile_render_work = r.render_work_stats().delta_since(work_before);
                    if profile_kind != "kia_black_no_gpu_render" {
                        profile_render_ms = Some(elapsed);
                        render_readback_ms.push(elapsed);
                    }
                }
                if f==0 { image::save_buffer(&poster,&rgba,output_size[0],output_size[1],image::ColorType::Rgba8)?; }
                encoder.stdin.as_mut().unwrap().write_all(&rgba)?;
                if diagnostic_layer_frame == Some(f) {
                    r.debug_capture_light_layers(&device,&queue,
                        &format!("{prefix}_frame_{f:06}_layers"))?;
                }
            }
            if f%FPS==0 {
                let mut state = telemetry(&g,&r);foot_motion.add_to(&mut state,g.foot_pos(),initial_foot);map.add_to(&mut state);
                states.push(json!({"at_s":f as f32/FPS as f32,"state":state,
                    "camera_world_m":r.camera.pos.to_array(),"camera_forward":r.camera.forward().to_array()}));
            }
            if record {
                std::thread::sleep(started.elapsed().mul_f32(idle_factor)
                    .max(std::time::Duration::from_millis(pause_ms)).min(std::time::Duration::from_secs(5)));
            }
            let simulation_start = capture_profile_enabled.then(Instant::now);
            for sub in 0..SIM_HZ/FPS {
                let tick=f*(SIM_HZ/FPS)+sub;let t=tick as f64/SIM_HZ as f64;
                let mut fields=serde_json::Map::new();
                for (i,step) in timeline.iter().enumerate() {
                    if step.get("action").is_some() {
                        if !performed[i] && t+1e-8>=scalar(step,"at_s",0.0) {
                            let terminal_before = kia_at;
                            let (accepted,detail)=capture_gameplay_if_alive(kia_at,
                                || action(&mut g,&mut r,step,&mut map)).transpose()?
                                .unwrap_or_else(|| (false, Some(terminal_input_rejection(
                                    terminal_before.expect("only terminal actions are skipped")))));
                            performed[i]=true;
                            events.push(json!({"at_s":t,"request":step,"accepted":accepted,"detail":detail}));
                            // Preserve the complete fatal fade/receipt. Required skipped actions
                            // fail acceptance below, instead of aborting or pretending success.
                            anyhow::ensure!(accepted || step["allow_rejected"]==true || terminal_before.is_some(),
                                "scenario interaction rejected at {t}: {}",step["action"]);
                            if kia_at.is_none() && g.game_over().is_some() { kia_at=Some(t as f32); }
                        }
                    } else if t+1e-8>=scalar(step,"from_s",0.0) && t<scalar(step,"until_s",0.0) {
                        if let Some(start) = kia_at {
                            if !performed[i] {
                                events.push(json!({"at_s":t,"request":step,"accepted":false,
                                    "detail":terminal_input_rejection(start),
                                    "ignored_until_s":step["until_s"]}));
                                performed[i]=true;
                            }
                        } else {
                            for (key,value) in step["input"].as_object().unwrap() {fields.insert(key.clone(),value.clone());}
                        }
                    }
                }
                capture_gameplay_if_alive(kia_at, || -> anyhow::Result<()> {
                    let mut controls = map.controls(input(&Value::Object(fields)),g.act.id,g.held_map_zoom());
                    if map.open {g.focus_held_map(map.cursor);}
                    let primary_edge = controls.mg_coax && !primary_was_held;
                    primary_was_held = controls.mg_coax;
                    // Mirror native LMB dispatch before Game::update. Held ammunition/equipment
                    // remain update-owned; station entry and the cupola crank are click-owned.
                    if primary_edge && !map.open && !diagnostic_crew
                        && !(controls.aim && g.exposed_commander_sidearm_post()) {
                        let forward = r.camera.forward();
                        let eye = r.camera.pos;
                        let dispatch = if g.click_articulated_hatch() {
                            Some("click_articulated_hatch")
                        } else if g.primary_loader_owns_click() {
                            None
                        } else if g.interact_world_door(forward, eye) || g.interact_portable_item(forward, eye) {
                            g.consume_primary_press();
                            Some("world_or_portable_interaction")
                        } else if showcase_crew.is_some() && g.hover_hatch_lever() {
                            // Native LAN clicks send a station-owned hatch request to authority.
                            controls.hatch_toggle = true;
                            g.consume_primary_press();
                            Some("lan_hatch_pulse")
                        } else if g.interact_hatch_lever(forward, eye) {
                            Some("interact_hatch_lever")
                        } else if g.click_station() {
                            Some("click_station")
                        } else if g.hover_hatch() {
                            if showcase_crew.is_some() { controls.hatch_toggle = true; }
                            else { g.click_hatch_or_exit(); }
                            Some("click_hatch_or_exit")
                        } else if g.click_mount(forward, eye) {
                            Some("click_mount")
                        } else { None };
                        if let Some(dispatch) = dispatch {
                            events.push(json!({"at_s":t,"request":{"input":{"mg_coax":true}},
                                "accepted":true,"detail":{"dispatch":dispatch,
                                    "scope":"Ordinary source-hover LMB edge; no forced hatch or station state"}}));
                        }
                    }
                    let controls=capture_crew_controls(controls,&g,diagnostic_crew,showcase_crew);
                    let before = g.foot_pos();
                    let tracking_foot = g.dbg_is_foot() && !g.is_climbing() && g.foot_hp()>0.0;
                    let requested = if tracking_foot && (controls.drive*controls.drive+controls.steer*controls.steer)>1e-8 {
                        requested_foot_speed(&controls)
                    } else {0.0} / SIM_HZ as f32;
                    g.update(&controls,1.0/SIM_HZ as f32,&mut r);
                    ingest_capture_campaign(&mut g,&mut campaign_run)?;
                    if let Some(crew) = showcase_crew {
                        anyhow::ensure!(g.station == crew.post && !g.dbg_is_foot(),
                            "showcase_crew left its fixed LAN post; use a separate solo traversal fixture");
                    }
                    if tracking_foot && g.dbg_is_foot() {foot_motion.record(before,g.foot_pos(),requested);}
                    Ok(())
                }).transpose()?;
                if g.game_over().is_some() && kia_at.is_none() {
                    kia_at=Some((t+1.0/SIM_HZ as f64) as f32);
                }
                if let Some(start) = kia_at {
                    // As in live play, the whole mix fades with the picture, then holds silent.
                    let since = (t+1.0/SIM_HZ as f64) as f32 - start;
                    if kia_audio_at_fatal.is_none() { kia_audio_at_fatal = Some(g.debug_scheduled_audio()); }
                    // Only sound already emitted before terminal state may finish travelling.
                    // The fatal tick has since=0, so it is never advanced twice.
                    g.advance_terminal_audio(1.0/SIM_HZ as f32, &r, since);
                    g.set_audio_level(1.0-crate::death_fade(since));
                    if since >= crate::DEATH_FADE {g.set_audio_silenced(true);}
                }
                let count=((tick+1) as u64*acoustics::SAMPLE_RATE as u64/SIM_HZ as u64)
                    -(tick as u64*acoustics::SAMPLE_RATE as u64/SIM_HZ as u64);
                samples.extend(g.render_audio_capture_frames(count as usize)?);
            }
            if let Some(start) = simulation_start {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                simulation_batch_ms.push(elapsed);
                timing_frames.push(json!({"frame":f,"at_s":f as f64 / FPS as f64,
                    "kind":profile_kind,"render_readback_ms":profile_render_ms,
                    "simulation_tick_batch_ms":elapsed,"render_work":capture_work_json(profile_render_work)}));
            }
        }
        Ok(())
    })();
    if let Some(encoder)=encoder.as_mut() {
        drop(encoder.stdin.take());
        if result.is_err() {let _=encoder.kill();let _=encoder.wait();return result;}
        anyhow::ensure!(encoder.wait()?.success(),"video encoder failed");
    }
    result?;
    anyhow::ensure!(samples.iter().all(|s|s.is_finite()),"nonfinite gameplay audio");
    anyhow::ensure!(samples.iter().any(|s|s.abs()>1e-6),"gameplay capture was silent");
    let raw_peak=samples.iter().map(|s|s.abs()).fold(0.0f32,f32::max);
    // Unity export: the capture is the game's own output, after its one reproduction transform
    // and master limiter (-1 dBFS). The linear guard only acts on a sum the master let through.
    let peak_guard=acoustics::attenuate_over_peak(&mut samples,0.94);
    if record {
        acoustics::write_wav_stereo(&wav,&samples,acoustics::SAMPLE_RATE)?;
        let mux=std::process::Command::new("ffmpeg").args(["-v","error","-nostdin","-y","-i",&raw,
            "-i",&wav,"-map","0:v:0","-map","1:a:0","-c:v","copy","-c:a","aac","-b:a","192k",
            "-movflags","+faststart","-shortest",&output]).status()?;
        anyhow::ensure!(mux.success(),"audio/video mux failed");
    }
    let mut final_state=telemetry(&g,&r);map.add_to(&mut final_state);
    final_state["player_displacement_m"]=json!(g.player.pos.distance(initial_position));
    foot_motion.add_to(&mut final_state,g.foot_pos(),initial_foot);
    let checks:Vec<Value>=spec["expect"].as_array().into_iter().flatten().map(|check| {
        let checkpoint = check["at_s"].as_u64();
        let observed = match checkpoint {
            Some(0) => &initial,
            Some(at) if (at as f64-scalar(&spec,"seconds",0.0)).abs()>1e-6 => states.iter()
                .find(|sample|sample["at_s"].as_f64()==Some(at as f64))
                .and_then(|sample|sample.get("state")).unwrap_or(&Value::Null),
            _ => &final_state,
        };
        outcome_check(check,&initial,observed)
    }).collect();
    let required_actions_passed=required_capture_actions_passed(&events);
    let checks_passed=required_actions_passed && checks.iter().all(|c|c["passed"]==true);
    let mut receipt=json!({
        "schema":if record && retained_diagnostic {"kessel.retained-model-diagnostic-record.v1"}
            else if record {"kessel.first-person-mechanic-record.v1"} else {"kessel.mechanic-simulation-check.v1"},
        "scenario_path":scenario_path,"scenario":spec,"media_recorded":record,
        "video":record.then_some(&output),"poster":record.then_some(&poster),"audio":record.then_some(&wav),
        "fps":FPS,"frames":if record {frames} else {0},"simulation_hz":SIM_HZ,
        "encoded_resolution":output_size,"native_resolution":[r.iw,r.ih],"first_person":!retained_diagnostic && crew_witness.is_none(),
        "crew_witness":spec.get("crew_witness"),
        "camera":if crew_witness.is_some() {"Disclosed turret-local render-only witness; ordinary live player camera retained for input; actual mortal solo roster, no fabricated animation or authority"}
            else if retained_diagnostic {"Hull-relative diagnostic orbit; gameplay camera and simulation remain unchanged"}
            else {"Game::update player camera; ordinary two-pass gunner optic when active; no chase camera"},
        "audio_source":"same ordinary gameplay Audio sinks, sampled on simulation clock; no substitute soundtrack",
        "audio_initial":initial_audio,"audio_final":g.audio_capture_metadata(),"raw_mix_peak":raw_peak,
        "export_fixed_gain":1.0,"export_peak_guard_gain":peak_guard,"sample_frames":samples.len()/2,
        "initial_state":initial,"final_state":final_state,"events":events,"state_samples":states,
        "setup_results":setup_results,"construction":construction_receipt,
        "campaign_capture_state":campaign_run.as_ref().map(|run|json!({
            "act":run.current_act,"attempt_active":run.attempt_active(),
            "phase":format!("{:?}",run.progress().phase),"checkpoint_written":false,
            "scope":"Normal mission signals consumed; short capture stays within Act One and does not exercise completion/retry menus"})),
        "movement_evidence":"Actual foot XZ positions/path vs requested controller distance. Clipped distance reports constrained motion, not an invented collision contact or material classification.",
        "outcome_checks":checks,"outcome_checks_passed":checks_passed,
        "kia_started_s":kia_at,"kia_policy":"As in live play: terminal state latches before any further gameplay action/input/interaction; skipped timeline requests are recorded as terminal_kia, and required skipped actions fail acceptance. Already emitted sound finishes travelling during the fade without gameplay updates. The turret witness and whole ordinary mix fade together over DEATH_FADE seconds, then the interior-only recorded aftermath replays silently, followed by black; the 120 Hz audio clock continues. No new damage or gameplay inputs during replay",
        "kia_black_seconds_in_clip":kia_at.map(|t| (scalar(&spec,"seconds",0.0)-(t+crate::DEATH_FADE+if g.last_fatal.is_some() {crate::REPLAY_DUR} else {0.0}) as f64).max(0.0)),
        "capture_frame_pause_ms":pause_ms,"capture_idle_factor":idle_factor,
        "validation":"interaction acceptance, declared state outcomes and audible finite mixer checked; visual review remains separate",
        "limitations":["Setup is a disclosed deterministic bench, not a historical event reconstruction.",
            "Rendered state is sampled at 24 fps; audio event timing follows 120 Hz simulation ticks and ordinary rodio control latency.",
            "Held map uses the live folded paper mesh, GPU cartography texture, imported compass and pencil/pin input; network overlays and controls menus are not exercised here."]
    });
    receipt["kia_audio_at_fatal"] = json!(kia_audio_at_fatal);
    receipt["scheduled_audio_final"] = g.debug_scheduled_audio();
    receipt["required_timeline_actions_passed"] = json!(required_actions_passed);
    receipt["capture_profile"] = if capture_profile_enabled {
        let adapter_info = adapter.get_info();
        json!({"enabled":true,"schema":"kessel.mechanic-capture-timing.v1",
            "renderer_backend":if r.shader_backend {"shader"} else {"physical"},
            "graphics_preset":r.graphics_preset.key(),"adapter":adapter_info.name,
            "device_backend":format!("{:?}",adapter_info.backend),
            "ray_queries_enabled":r.rt_on,"deferred_enabled":r.deferred,
            "timestamp_query_feature_enabled":device.features().contains(wgpu::Features::TIMESTAMP_QUERY),
            "gpu_pass_timestamps_collected":false,
            "native_scene_resolution":[r.iw,r.ih],"rgba_readback_resolution":record.then_some(output_size),
            "rgba_bytes_per_rendered_frame":record.then_some(output_pixels as u64 * 4),
            "gpu_fenced_readback":record,"simulation_ticks_per_batch":SIM_HZ/FPS,
            "simulated_seconds_per_batch":1.0 / FPS as f64,"warmup_excluded":true,
            "render_readback_ms":capture_timing_summary(&render_readback_ms),
            "simulation_tick_batch_ms":capture_timing_summary(&simulation_batch_ms),
            "render_work_total":capture_work_json(r.render_work_stats().delta_since(work_before_capture)),
            "frames":timing_frames,
            "quantiles":"Linear interpolation at (count-1)*p; no discarded slow frames",
            "render_scope":"Existing render/capture orchestration, GPU completion wait, declared output-resolution RGBA readback and row copy. Includes active periscope/ADS views and HUD. Scene resolution is reported separately. KIA black frames are excluded from render statistics.",
            "simulation_scope":"Five ordinary 120Hz input/action/Game-update ticks and device-free audio mixing per batch. Includes normal renderer-state uploads; no extra GPU fence.",
            "excluded":"PNG writing, encoder pipe/backpressure, ffmpeg/mux time, deliberate sleeps, diagnostic exports and warmup",
            "interpretation":"Capture/readback timings, not pure GPU timings or interactive window FPS. No presentation/vsync measurement; five simulation ticks per captured frame differ from one interactive frame."})
    } else {json!({"enabled":false})};
    receipt["enemy_report_trace"] = g.debug_enemy_report_telemetry();
    receipt["diagnostic_view"]=json!(spec.get("diagnostic_view"));
    receipt["retained_source_draws"]=json!(model_view.as_ref().map(|view|view.last_draw_count));
    receipt["diagnostic_scope"]=json!(retained_diagnostic.then_some("Retained live tank meshes with source section planes; skeleton and live cartridge X-ray overlay. Separate bounded diagnostic lighting. No organ health, hit-volume circles, gameplay visibility changes or substituted audio."));
    std::fs::write(format!("{prefix}_manifest.json"),serde_json::to_vec_pretty(&receipt)?)?;
    anyhow::ensure!(checks_passed,"mechanic did not meet its state or required interaction checks; retained available evidence for diagnosis");
    println!("[mechanic-review] {}",if record {output} else {format!("{prefix}_manifest.json (simulation only; no media)")});
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn front_seat_look_reviews_use_source_targets_and_ordinary_buttons() {
        for path in ["catalog/shader_reviews/driver_seated_wide_visor.v1.json",
            "catalog/shader_reviews/radio_seated_look_hatch.v1.json"] {
            let spec=validate_file(path).unwrap();
            let steps=spec["timeline"].as_array().unwrap();
            assert!(steps.iter().filter_map(|s|s["action"].as_str())
                .all(|action|matches!(action,"look_hatch"|"look_driver_control")));
            assert!(steps.iter().any(|s|s["input"]["mg_coax"]==true));
            assert!(steps.iter().all(|s|s["input"].get("hatch_toggle").is_none()));
            assert!(spec["setup"].get("hatch_open").is_none());
        }
        let spec=validate_file("catalog/shader_reviews/driver_seated_wide_visor.v1.json").unwrap();
        let checks=spec["expect"].as_array().unwrap();
        assert!(checks.iter().any(|c|c["metric"]=="driver_wide_view" && c["equals"]==true));
        assert!(checks.iter().any(|c|c["metric"]=="driver_wide_view" && c["at_s"]==9 && c["equals"]==false));
        assert!(checks.iter().any(|c|c["metric"]=="camera_horizontal_fov_deg" && c["min_value"]==89.99));
    }

    #[test]
    fn loader_contact_fixture_requires_explicit_preview_before_renderer_creation() {
        let normal=json!({"setup":{"singleplayer_crew":true},"expect":[]});
        assert!(require_loader_preview(&normal,false).is_ok());
        let mut fixture=normal.clone();
        fixture["expect"]=json!([{"metric":"loader_hand_targets","at_s":4,"equals":2}]);
        assert!(require_loader_preview(&fixture,true).is_err());
        fixture["setup"]["loader_arm_preview"]=json!(true);
        assert!(require_loader_preview(&fixture,false).is_err());
        assert!(require_loader_preview(&fixture,true).is_ok());
        fixture["setup"]["loader_arm_preview"]=json!("true");
        assert!(require_loader_preview(&fixture,true).is_err());
    }

    #[test]
    fn arm_contact_receipt_cannot_pass_missing_loader_targets_as_zero_error() {
        let missing = crew_arm_contact_metrics(&json!({"available":true,"actors":[]}));
        assert_eq!(missing["crew_hand_targets"], 0);
        assert!(missing["crew_hand_target_error_m"].is_null());
        assert_eq!(missing["driver_hand_targets"],0);
        assert!(missing["driver_hand_target_error_m"].is_null());
        let driver=crew_arm_contact_metrics(&json!({"available":true,"actors":[
            {"post":"DRIVER","hand_error_m":[0.03,0.01]}]}));
        assert_eq!(driver["driver_hand_targets"],2);
        assert_eq!(driver["driver_hand_target_error_m"],0.03);
        let gunner_only = crew_arm_contact_metrics(&json!({"available":true,"actors":[
            {"post":"GUNNER","hand_error_m":[0.001]}]}));
        assert_eq!(gunner_only["gunner_hand_targets"], 1);
        assert_eq!(gunner_only["loader_hand_targets"], 0);
        assert!(gunner_only["loader_hand_target_error_m"].is_null());
        let real = crew_arm_contact_metrics(&json!({"available":true,"actors":[
            {"post":"GUNNER","hand_error_m":[0.001]},
            {"post":"LOADER","hand_error_m":[0.002,0.07]}]}));
        assert_eq!(real["loader_hand_targets"], 2);
        assert_eq!(real["loader_hand_target_error_m"], 0.07);
        let invalid = crew_arm_contact_metrics(&json!({"available":true,"actors":[
            {"post":"LOADER","hand_error_m":[0.001,null]}]}));
        assert_eq!(invalid["loader_hand_targets"], 2);
        assert!(invalid["loader_hand_target_error_m"].is_null());
    }

    #[test]
    fn crew_witness_requires_explicit_solo_scope_and_rejects_network_or_live_capture_mixing() {
        let fixture=json!({"setup":{"singleplayer_crew":true},"crew_witness":{
            "eye_turret_m":[0,2.2,-0.7],"target_turret_m":[0,1.4,0.3]}});
        assert!(CrewWitness::from_spec(&fixture).unwrap().is_some());
        for field in ["showcase_crew","diagnostic_crew","act_one_playtest"] {
            let mut invalid=fixture.clone();invalid["setup"][field]=json!(true);
            assert!(CrewWitness::from_spec(&invalid).is_err());
        }
        let mut invalid=fixture.clone();invalid["crew_witness"]["target_turret_m"]=invalid["crew_witness"]["eye_turret_m"].clone();
        assert!(CrewWitness::from_spec(&invalid).is_err());
    }

    #[test]
    fn showcase_crew_routes_only_the_viewer_through_five_real_lan_posts() {
        for (post, role) in [(game::Station::Commander,lan::LanRole::Commander),
            (game::Station::Driver,lan::LanRole::Driver), (game::Station::Gunner,lan::LanRole::Gunner),
            (game::Station::Loader,lan::LanRole::Loader), (game::Station::Radioman,lan::LanRole::Radio)] {
            let crew = ShowcaseCrew { role, post };
            let raw = TankInput {drive:0.4,steer:-0.3,traverse:0.2,elevate:0.1,
                look_dx:0.5,look_dy:-0.6,mg_coax:true,coax:true,reload:true,
                ..Default::default()};
            let mapped = crew.controls(raw,post,false,false,false);
            assert!(mapped.coop_host && !mapped.couch_crew);
            assert!(mapped.couch_identities.iter().all(Option::is_none));
            let frame = mapped.coop.unwrap();
            assert_eq!(frame.human_count(),5);
            assert!(!frame.gunner_is_loader());
            for identity in lan::LanRole::ALL {
                assert_eq!(frame.roster[identity as usize],identity as u32+1);
                let expected = if identity == role {
                    lan_role_input(raw,role,post,false,false,false)
                } else { lan::RoleInput {post:identity,..Default::default()} };
                assert_eq!(frame.input(identity),expected,"idle peers must never inherit viewer input");
            }
            // Local camera/hand input survives role routing, including hold/release on the loader.
            assert!(mapped.mg_coax && mapped.reload);
            assert_eq!(mapped.look_dx,raw.look_dx);
            let released = crew.controls(TankInput::default(),post,false,false,false);
            assert!(!released.mg_coax);
            assert!(released.coop.unwrap().inputs.iter().all(|input|input.flags == 0));
        }
    }

    #[test]
    fn showcase_crew_preserves_loader_manual_hands_and_gunner_tool_routing() {
        let loader = ShowcaseCrew {role:lan::LanRole::Loader,post:game::Station::Loader};
        let held = loader.controls(TankInput {mg_coax:true,drive:1.0,look_dx:2.0,
            ..Default::default()},loader.post,false,false,false);
        let frame = held.coop.unwrap();
        assert!(frame.input(lan::LanRole::Loader).flag(lan::F_SERVICE_BREECH));
        assert!(held.coop_host); // Game's existing local_manual_loader gate owns the physical hold.
        assert_eq!(frame.input(lan::LanRole::Loader).drive,0.0);
        // Directional input must survive the remote loader route for deliberate ram gestures.
        assert_eq!(frame.input(lan::LanRole::Loader).look_dx,2.0);
        assert_eq!(frame.input(lan::LanRole::Gunner).flags,0);
        assert!(held.mg_coax);
        assert_eq!(held.look_dx,2.0);
        let gunner = ShowcaseCrew {role:lan::LanRole::Gunner,post:game::Station::Gunner};
        let using_tool = gunner.controls(TankInput {mg_coax:true,..Default::default()},
            gunner.post,true,false,false).coop.unwrap();
        assert!(using_tool.input(gunner.role).flag(lan::F_EXTINGUISH));
        assert!(!using_tool.input(gunner.role).flag(lan::F_FIRE_MAIN));
        assert_eq!(using_tool.input(lan::LanRole::Loader).flags,0,
            "occupied idle loader must never start automatic service");
    }

    #[test]
    fn showcase_crew_rejects_diagnostic_mixing_and_traversal_without_bypassing_lan() {
        let base=json!({"station":"loader","setup":{"showcase_crew":true},"timeline":[]});
        assert_eq!(ShowcaseCrew::from_spec(&base).unwrap().unwrap().role,lan::LanRole::Loader);
        assert!(ShowcaseCrew::from_spec(&json!({})).unwrap().is_none());
        for value in [json!(false),json!("yes"),Value::Null] {
            let mut spec=base.clone();spec["setup"]["showcase_crew"]=value;
            assert!(ShowcaseCrew::from_spec(&spec).is_err());
        }
        for (key,value) in [("on_foot",json!(true)),("diagnostic_crew",json!(true))] {
            let mut spec=base.clone();spec["setup"][key]=value;
            assert!(ShowcaseCrew::from_spec(&spec).is_err());
        }
        let mut diagnostic=base.clone();diagnostic["diagnostic_view"]=json!("retained_model");
        assert!(ShowcaseCrew::from_spec(&diagnostic).is_err());
        let mut no_post=base.clone();no_post.as_object_mut().unwrap().remove("station");
        assert!(ShowcaseCrew::from_spec(&no_post).is_err());
        for step in [json!({"action":"station","value":"gunner"}),json!({"action":"mount"}),
            json!({"action":"dismount"}),json!({"input":{"jump":true}})] {
            let mut spec=base.clone();spec["timeline"]=json!([step]);
            assert!(ShowcaseCrew::from_spec(&spec).is_err());
        }
        let mut manual=base;
        manual["timeline"]=json!([{"action":"look_breech"},{"input":{"mg_coax":true}},
            {"input":{"mg_coax":false,"hatch_toggle":true}}]);
        assert!(ShowcaseCrew::from_spec(&manual).is_ok());
    }

    #[test]
    fn showcase_crew_compartment_recipe_discloses_idle_crew_without_fake_combat() {
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("catalog/shader_reviews/showcase_crew_compartment.v1.json");
        let spec=validate_file(&path.to_string_lossy()).unwrap();
        let crew=ShowcaseCrew::from_spec(&spec).unwrap().unwrap();
        assert_eq!(crew.role,lan::LanRole::Loader);
        assert_eq!(spec["act_id"],1);
        assert_eq!(spec["setup"]["enemy_policy"],"hold_fire");
        assert!(spec["timeline"].as_array().unwrap().iter().all(|step|
            step["action"].is_null() || step["action"] == "look_breech"));
        let receipt=crew.receipt();
        assert_eq!(receipt["idle_other_peers"],4);
        assert_eq!(receipt["occupied_roles"].as_array().unwrap().len(),5);
        assert!(receipt["scope"].as_str().unwrap().contains("no network sockets"));
    }

    #[test]
    fn act1_hatch_roundtrips_use_real_space_edges_without_traversal_injection() {
        for (hatch, station) in [("driver", "driver"), ("loader", "loader"),
            ("radio", "radio"), ("side_escape", "loader"), ("cupola", "commander")] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("catalog/shader_reviews/act1_{hatch}_space_roundtrip.v1.json"));
            let spec = validate_file(&path.to_string_lossy()).unwrap();
            assert_eq!(spec["act_id"], 1);
            assert_eq!(spec["world_manifest"], "assets/worldgen/act1/manifest.v1.json");
            assert_eq!(spec["station"], station);
            assert_eq!(spec["setup"]["playtest"], true);
            assert_eq!(spec["setup"]["enemy_policy"], "hold_fire");
            assert_eq!(spec["setup"].as_object().unwrap().len(), 2,
                "the actual map must own spawn and initial hatch state");
            let steps = spec["timeline"].as_array().unwrap();
            assert!(steps.iter().filter_map(|s| s["action"].as_str())
                .all(|action| matches!(action, "look_hatch" | "look_cupola")));
            let holds: Vec<_> = steps.iter().filter(|s| s["input"]["jump"] == true).collect();
            assert_eq!(holds.len(), 2, "exit and entry must both use ordinary Space");
            assert!(scalar(holds[0], "until_s", 0.0) < scalar(holds[1], "from_s", 0.0),
                "the two traversals need separate press edges");
            for hold in holds {
                let start = scalar(hold, "from_s", 0.0);
                let end = scalar(hold, "until_s", 0.0);
                assert!(end - start >= 3.0, "hold Space beyond the traversal finish");
                assert!(steps.iter().any(|s| s["action"] == "look_hatch"
                    && s["value"] == format!("{hatch}_aperture")
                    && scalar(s, "at_s", 0.0) > start + 1.5
                    && scalar(s, "at_s", 0.0) < end),
                    "look into the opening after traversing while the same Space press remains held");
            }
            let checks = spec["expect"].as_array().unwrap();
            assert!(checks.iter().any(|c| c["metric"] == "on_foot"
                && c["at_s"] == 4 && c["equals"] == true));
            assert!(checks.iter().any(|c| c["metric"] == "on_foot"
                && c["at_s"] == 8 && c["equals"] == false));
        }
    }

    // Authored fixtures are shared game resources. Keep validate_file's explicit filesystem
    // semantics for caller-owned capture scenarios, while these asset tests honor asset roots.
    fn validate_review_resource(path: &str) -> anyhow::Result<Value> {
        let physical = crate::resource::resolve(path)?;
        validate_file(&physical.to_string_lossy())
    }

    #[test]
    fn held_mouse_reload_fixture_uses_one_continuous_grip_without_injected_inventory() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("catalog/shader_reviews/held_mouse_reload.v1.json");
        let spec = validate_file(&path.to_string_lossy()).unwrap();
        let steps = spec["timeline"].as_array().unwrap();
        assert!(steps.iter().all(|s| !matches!(s["action"].as_str(),Some("round"|"equip"|"portable"))));
        let grip = steps.iter().find(|s| s["from_s"]==4.8).unwrap();
        assert_eq!(grip["input"]["mg_coax"],true);
        assert_eq!(grip["until_s"],8.1);
        assert!(steps.iter().any(|s|s["action"]=="look_tray"&&s["at_s"]==5.5));
        assert!(spec["expect"].as_array().unwrap().iter().any(|c|c["metric"]=="main_gun_shots"
            &&c["min_delta"]==2&&c["max_delta"]==2));
    }

    #[test]
    fn capture_profile_summaries_keep_tail_latency_and_mark_missing_renders() {
        let empty = capture_timing_summary(&[]);
        assert_eq!(empty["count"], 0);
        assert!(empty["median_ms"].is_null() && empty["p99_ms"].is_null());
        let single = capture_timing_summary(&[7.0]);
        assert_eq!(single["median_ms"], 7.0);
        assert_eq!(single["p99_ms"], 7.0);
        let all: Vec<f64> = (1..=101).rev().map(f64::from).collect();
        let summary = capture_timing_summary(&all);
        assert_eq!(summary["count"], 101);
        assert_eq!(summary["min_ms"], 1.0);
        assert_eq!(summary["median_ms"], 51.0);
        assert_eq!(summary["p95_ms"], 96.0);
        assert_eq!(summary["p99_ms"], 100.0);
        assert_eq!(summary["max_ms"], 101.0);
        assert_eq!(capture_timing_summary(&[10.0, 2.0])["median_ms"], 6.0);
    }

    #[test]
    fn playtest_fixture_construction_rejects_partial_campaign_setup() {
        let t34s = [tank::EnemyKind::T34; 6];
        assert!(validate_constructed_playtest(Some(false), &t34s).is_ok());
        assert!(validate_constructed_playtest(None, &t34s).is_err());
        assert!(validate_constructed_playtest(Some(true), &t34s).is_err());
        let mut old_campaign = t34s;
        old_campaign[0] = tank::EnemyKind::Su100;
        old_campaign[1] = tank::EnemyKind::Su100;
        assert!(validate_constructed_playtest(Some(false), &old_campaign).is_err());
        assert!(validate_constructed_playtest(Some(false), &t34s[..5]).is_err());
    }

    #[test]
    fn act_one_capture_uses_real_campaign_and_rejects_fixture_cheats() {
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("catalog/shader_reviews/act1_solo_crew_physical_map.v1.json");
        let spec=validate_file(&path.to_string_lossy()).unwrap();
        assert!(validate_act_one_playtest(&spec).unwrap());
        for key in ["playtest", "god", "invincible", "showcase_crew", "diagnostic_crew",
            "player_xz", "on_foot", "fire", "source_fire", "broken_module", "aim_enemy"] {
            let mut modified=spec.clone();modified["setup"][key]=json!(true);
            assert!(validate_act_one_playtest(&modified).is_err(),"accepted {key}");
        }
        for policy in ["banish", "hold_fire"] {
            let mut modified=spec.clone();modified["setup"]["enemy_policy"]=json!(policy);
            assert!(validate_act_one_playtest(&modified).is_err());
        }
        let mut modified=spec.clone();modified["act_id"]=json!(2);
        assert!(validate_act_one_playtest(&modified).is_err());
        let mut modified=spec;modified["timeline"]=json!([{"at_s":1,"action":"incoming_impact"}]);
        assert!(validate_act_one_playtest(&modified).is_err());
    }

    #[test]
    fn diagnostic_crew_is_explicit_mortal_roster_not_visual_fallback() {
        let ordinary=diagnostic_crew_controls(TankInput::default(),false);
        assert!(ordinary.coop.is_none());assert!(!ordinary.couch_crew);
        let diagnostic=diagnostic_crew_controls(TankInput::default(),true);
        let frame=diagnostic.coop.unwrap();assert_eq!(frame.human_count(),4);
        assert!(!frame.occupied(lan::LanRole::Radio));
        for (i,identity) in game::COUCH_CREW_ROLES.into_iter().enumerate() {
            assert_eq!(frame.roster[identity as usize],i as u32+1);
            assert_eq!(frame.input(identity).post,identity);
            assert_eq!(diagnostic.couch_identities[i].unwrap().identity,identity);
        }
        assert!(diagnostic.couch_crew && diagnostic.coop_host);
    }
    #[test]
    fn retained_model_recipe_has_diagnostic_contract_and_real_contact_assertion() {
        let spec=validate_review_resource("assets/review/first_person/diagnostic_retained_tiger_85mm_v17.json").unwrap();
        assert_eq!(spec["diagnostic_view"],"retained_model");
        assert_eq!(spec["setup"]["diagnostic_crew"],true);
        assert!(spec["expect"].as_array().unwrap().iter().any(|check|
            check["metric"]=="player_armor_contacts" && check["min_delta"]==1));
    }
    #[test]
    fn held_paper_consumes_weapon_and_motion_controls_but_keeps_annotations() {
        let mut map=ReviewMap::default();map.open=true;
        let out=map.controls(TankInput {mg_coax:true,drive:1.0,look_dx:10.0,..Default::default()},2,1.0);
        assert!(!out.mg_coax);assert_eq!(out.drive,0.0);assert_eq!(out.look_dx,0.0);
        assert_eq!(map.marks.len(),1);
        assert_eq!(map.marks[0].0,2);
        ui::toggle_map_pin(&mut map.pins,2,map.cursor);
        ui::toggle_map_pin(&mut map.pins,3,map.cursor);
        ui::toggle_map_pin(&mut map.pins,2,map.cursor);
        assert_eq!(map.pins.len(),1);assert_eq!(map.pins[0].0,3);
        map.open=false;
        assert!(map.controls(TankInput {mg_coax:true,..Default::default()},2,1.0).mg_coax);
        assert_eq!(map.marks.len(),1);
    }
    #[test]
    fn showcase_20260925_requested_distance_uses_walk_run_and_crouch_priority() {
        assert_eq!(requested_foot_speed(&TankInput::default()),foot::WALK);
        assert_eq!(requested_foot_speed(&TankInput {run:true,..Default::default()}),foot::RUN);
        assert_eq!(requested_foot_speed(&TankInput {run:true,crouch:true,..Default::default()}),foot::CROUCH_WALK);
    }
    #[test]
    fn showcase_20260925_fixtures_and_zoomed_pencil_match_native_controls() {
        for clip in ["reload", "pistol", "map"] {
            let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("catalog/shader_reviews/showcase_20260925_{clip}.json"));
            validate_file(&path.to_string_lossy()).unwrap();
        }
        let mut normal=ReviewMap::default(); normal.open=true;
        let mut zoomed=ReviewMap::default(); zoomed.open=true;
        let origin=normal.cursor;
        let input=TankInput {mg_coax:true,look_dx:8.0,look_dy:-4.0,..Default::default()};
        normal.controls(input,1,1.0);
        zoomed.controls(input,1,2.0);
        assert!((normal.cursor-origin-2.0*(zoomed.cursor-origin)).length()<1e-6,
            "zoom must reduce paper-space mouse travel exactly as the native map branch");
        assert_eq!(normal.marks.len(),1);assert_eq!(zoomed.marks.len(),1);
    }
    #[test]
    fn rational_audio_clock_has_no_24fps_rounding_drift() {
        for seconds in [4u32,8,17,60] {
            let count:u64=(0..seconds*SIM_HZ).map(|t| {
                    ((t+1) as u64*44_100/SIM_HZ as u64)-(t as u64*44_100/SIM_HZ as u64)
                }).sum();
            assert_eq!(count,seconds as u64*44_100);
        }
    }

    #[test]
    fn terminal_capture_never_invokes_gameplay_callbacks_but_keeps_audio_clock() {
        let mut kia_at = None;
        let mut shots = 0;
        let mut input_ticks = 0;
        let mut audio_frames = 0u64;
        let mut events = Vec::new();
        let terminal_tick = 2_202; // 18.35 s, matching the observed original duel.
        let mut state_at_death = None;
        for tick in 0..34 * SIM_HZ {
            if tick == 7 * SIM_HZ || tick == 27 * SIM_HZ {
                let accepted = capture_gameplay_if_alive(kia_at, || { shots += 1; true })
                    .unwrap_or(false);
                events.push(json!({"request":{"action":"staged_discharge"},
                    "accepted":accepted,"detail":kia_at.map(terminal_input_rejection)}));
            }
            capture_gameplay_if_alive(kia_at, || { input_ticks += 1; });
            if tick + 1 == terminal_tick {
                kia_at = Some((tick + 1) as f32 / SIM_HZ as f32);
                state_at_death = Some((shots, input_ticks));
            }
            // Audio continues on the same rational clock while the ordinary death fade runs.
            audio_frames += ((tick+1) as u64 * 44_100 / SIM_HZ as u64)
                - (tick as u64 * 44_100 / SIM_HZ as u64);
            if let Some(start) = kia_at {
                let since = (tick+1) as f32 / SIM_HZ as f32 - start;
                if since >= crate::DEATH_FADE { assert_eq!(crate::death_fade(since), 1.0); }
            }
        }
        assert_eq!(Some((shots, input_ticks)), state_at_death,
            "neither staged actions nor input/interaction callbacks may mutate terminal state");
        assert_eq!(shots, 1);
        assert_eq!(audio_frames, 34 * 44_100);
        assert_eq!(events[1]["accepted"], false);
        assert_eq!(events[1]["detail"]["reason"], "terminal_kia");
        assert!(!required_capture_actions_passed(&events));
        let required_shots = json!({"metric":"main_gun_shots","min_delta":2,"max_delta":2});
        assert_eq!(outcome_check(&required_shots,&json!({"main_gun_shots":0}),
            &json!({"main_gun_shots":shots}))["passed"], false);
        events[1]["request"]["allow_rejected"] = json!(true);
        assert!(required_capture_actions_passed(&events));
    }

    #[test]
    fn terminal_duel_evaluation_preserves_setup_and_has_no_postmortem_shot() {
        let original = validate_file("catalog/shader_reviews/pilot_cupola_t34_duel_20260929.v1.json").unwrap();
        let evaluation = validate_file("catalog/shader_reviews/pilot_cupola_t34_duel_terminal_20260929.v1.json").unwrap();
        assert_eq!(evaluation["setup"], original["setup"]);
        assert_eq!(evaluation["seconds"], 20);
        assert_eq!(evaluation["timeline"], json!(original["timeline"].as_array().unwrap()[..3]));
        let expects = evaluation["expect"].as_array().unwrap();
        assert!(expects.iter().any(|check| check["metric"] == "main_gun_shots"
            && check["min_delta"] == 1 && check["max_delta"] == 1));
        assert!(expects.iter().any(|check| check["metric"] == "game_over" && check["equals"] == true));
        assert!(expects.iter().any(|check| check["metric"] == "death_cause"
            && check["equals"] == "AmmoDetonation"));
    }

    #[test]
    fn healthy_hammer_fixture_uses_real_pickup_then_released_clicks_and_hold() {
        let spec = validate_file("catalog/shader_reviews/toolkit_free_hammer_20260929.v1.json").unwrap();
        assert_eq!(spec["act_id"], 0);
        assert_eq!(spec["station"], "loader");
        assert_eq!(spec["setup"], json!({"enemy_policy":"banish"}));
        let timeline = spec["timeline"].as_array().unwrap();
        assert!(timeline.iter().filter_map(|step|step["action"].as_str())
            .all(|action| matches!(action,"look_equipment" | "look_tray")));
        let presses: Vec<_> = timeline.iter().filter(|step|step["input"]["mg_coax"]==true).collect();
        assert_eq!(presses.len(), 4, "pickup, two short clicks, then a hold");
        assert!(presses.windows(2).all(|pair|
            scalar(pair[0],"until_s",0.0)<scalar(pair[1],"from_s",0.0)));
        assert!(scalar(presses[3],"until_s",0.0)-scalar(presses[3],"from_s",0.0)>1.0);
        let checks = spec["expect"].as_array().unwrap();
        assert!(checks.iter().any(|check|check["metric"]=="repair_progress_s"
            && check["max_value"]==0));
        assert!(checks.iter().any(|check|check["metric"]=="hammer_swing_remaining_s"
            && check["at_s"]==2 && scalar(check,"min_value",0.0)>0.0));
    }

    #[test]
    fn world_service_outcomes_require_observed_values_and_keep_checkpoint_deltas() {
        let initial=json!({"tank_fuel_liters":400.0,"held_fuel_can":false});
        let held=json!({"tank_fuel_liters":400.0,"held_fuel_can":true});
        let filled=json!({"tank_fuel_liters":420.0,"held_fuel_can":true});
        let delta=json!({"metric":"tank_fuel_liters","min_delta":19.99,"max_delta":20.01});
        assert_eq!(outcome_check(&delta,&initial,&held)["passed"],false);
        assert_eq!(outcome_check(&delta,&initial,&filled)["passed"],true);
        assert_eq!(outcome_check(&json!({"metric":"tank_fuel_liters","min_value":419.9,"max_value":420.1}),&initial,&filled)["passed"],true);
        assert_eq!(outcome_check(&json!({"metric":"held_fuel_can","equals":true}),&initial,&held)["passed"],true);
        assert_eq!(outcome_check(&json!({"metric":"missing","equals":null}),&initial,&filled)["passed"],false);
    }

    #[test]
    fn motion_evidence_distinguishes_free_travel_from_constrained_motion() {
        let mut motion=FootMotion::default();
        motion.record(Vec3::ZERO,Vec3::Z,1.0);
        motion.record(Vec3::Z,Vec3::Z,1.0);
        motion.record(Vec3::Z,Vec3::new(0.0,-2.0,2.0),1.0);
        assert_eq!(motion.path_m,2.0);assert_eq!(motion.requested_m,3.0);
        assert_eq!(motion.clipped_m,1.0);assert_eq!(motion.clipped_ticks,1);
        let mut state=json!({});motion.add_to(&mut state,Vec3::new(0.0,-2.0,2.0),Vec3::ZERO);
        assert_eq!(state["foot_displacement_m"],2.0);
    }

    #[test]
    fn current_fire_pair_uses_identical_source_and_released_lmb_pickup() {
        let read = |name: &str| {
            let path = format!("catalog/shader_reviews/fire_suppression_{name}_lmb_20260929.v1.json");
            validate_file(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
            serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap()
        };
        let aimed = read("aimed");
        let wrong = read("wrong_aim");
        assert_eq!(aimed["setup"], wrong["setup"]);
        for spec in [&aimed, &wrong] {
            let steps = spec["timeline"].as_array().unwrap();
            assert!(steps.iter().any(|step| step["action"] == "look_equipment"
                && step["value"] == "extinguisher"));
            assert!(steps.iter().all(|step| step["input"].get("extinguish").is_none()
                && step["action"] != "equip"));
            let presses: Vec<_> = steps.iter().filter(|step| step["input"]["mg_coax"] == true).collect();
            assert_eq!(presses.len(), 2);
            assert!(scalar(presses[0], "until_s", 0.0) < scalar(presses[1], "from_s", 0.0));
        }
    }

    #[test]
    fn authored_world_scenarios_pass_cpu_preflight() {
        for name in ["fp_playtest_refuel","fp_playtest_garage_traversal",
            "fp_playtest_warehouse_traversal","fp_playtest_grove_walk","fp_sourced_cellar_descent",
            "fp_garage_source_doors_v15","fp_warehouse_source_doors_v15",
            "fp_featured_source_door_stairs_v15","fp_sourced_cellar_standing_v15",
            "fp_pasture_ground_grass_v15","fp_grove_supported_rock_grass_v15"] {
            let path=format!("assets/review/first_person/{name}.json");
            validate_review_resource(&path).unwrap_or_else(|error|panic!("{path}: {error}"));
        }
    }

    #[test]
    fn refuel_fixture_rejects_implicit_or_wrong_mode_setup_before_renderer_creation() {
        let temp=std::env::temp_dir().join(format!("kessel-refuel-preflight-{}.json",std::process::id()));
        for value in [json!("yes"),json!(true)] {
            let spec=json!({"schema":"kessel.first-person-mechanic.v1","id":"bad_refuel",
                "act_id":0,"seconds":3,"setup":{"playtest_refuel":value},"timeline":[]});
            std::fs::write(&temp,serde_json::to_vec(&spec).unwrap()).unwrap();
            assert!(validate_file(temp.to_str().unwrap()).is_err());
        }
        std::fs::remove_file(temp).unwrap();
    }

    #[test]
    fn primer_fixture_is_explicit_and_cannot_masquerade_as_a_disabled_gun() {
        validate_review_resource("assets/review/first_person/fp_primer_fault_reload_20260905.json").unwrap();
        let temp=std::env::temp_dir().join(format!("kessel-primer-preflight-{}.json",std::process::id()));
        for setup in [json!({"primer_fault":true}), json!({"primer_fault":"random"}),
            json!({"primer_fault":"chamber","broken_module":"main_gun"}),
            json!({"primer_fault":"chamber","on_foot":true})] {
            let spec=json!({"schema":"kessel.first-person-mechanic.v1","id":"bad_primer",
                "act_id":0,"seconds":3,"setup":setup,"timeline":[]});
            std::fs::write(&temp,serde_json::to_vec(&spec).unwrap()).unwrap();
            assert!(validate_file(temp.to_str().unwrap()).is_err());
        }
        std::fs::remove_file(temp).unwrap();
    }

    #[test]
    fn combat_observer_may_use_the_ordinary_commander_station() {
        let temp=std::env::temp_dir().join(format!("kessel-combat-observer-{}.json",std::process::id()));
        for (post,on_foot,allowed) in [("commander",false,true),("driver",true,true),("gunner",false,false)] {
            let spec=json!({"schema":"kessel.first-person-mechanic.v1","id":"combat_observer",
                "act_id":0,"seconds":3,"station":post,"setup":{"on_foot":on_foot,
                "mixed_combat":{"center_xz":[80,80]}},"timeline":[]});
            std::fs::write(&temp,serde_json::to_vec(&spec).unwrap()).unwrap();
            assert_eq!(validate_file(temp.to_str().unwrap()).is_ok(),allowed,"{post}/{on_foot}");
        }
        std::fs::remove_file(temp).unwrap();
    }

    #[test]
    fn normal_enemy_report_recipes_require_discharge_and_reject_injected_impacts() {
        let temp=std::env::temp_dir().join(format!("kessel-normal-report-preflight-{}.json",std::process::id()));
        for label in ["near","far"] {
            let mut spec=validate_review_resource(&format!("assets/review/first_person/fp_enemy_report_{label}_v19_20260905.json")).unwrap();
            assert_eq!(spec["setup"]["enemy_policy"],"live");
            assert!(spec["expect"].as_array().unwrap().iter().any(|c|
                c["metric"]=="enemy_fire_discharges" && c["min_delta"]==1));
            spec["timeline"].as_array_mut().unwrap().push(json!({"at_s":2,"action":"incoming_impact",
                "shell":"BR_350B","range_m":80,"physical_left":true,"seed":1}));
            std::fs::write(&temp,serde_json::to_vec(&spec).unwrap()).unwrap();
            assert!(validate_file(temp.to_str().unwrap()).is_err());
        }
        std::fs::remove_file(temp).unwrap();
    }

    #[test]
    fn auxiliary_he_fixture_accepts_empty_inventory_and_rejects_invalid_round_counts() {
        let temp=std::env::temp_dir().join(format!("kessel-soft-he-preflight-{}.json",std::process::id()));
        for (he, allowed) in [(json!(0),true),(json!(8),true),(json!(-1),false),
            (json!(9),false),(json!(1.5),false),(json!("0"),false),(Value::Null,false)] {
            let spec=json!({"schema":"kessel.first-person-mechanic.v1","id":"soft_he_inventory",
                "act_id":0,"seconds":3,"station":"commander","setup":{"soft_target_tank":{
                    "index":0,"kind":"kv2","pos_xz":[80,25],"target_xz":[80,89],"he_ammo":he}},"timeline":[]});
            std::fs::write(&temp,serde_json::to_vec(&spec).unwrap()).unwrap();
            assert_eq!(validate_file(temp.to_str().unwrap()).is_ok(),allowed);
        }
        let legacy=json!({"schema":"kessel.first-person-mechanic.v1","id":"legacy_soft_he_inventory",
            "act_id":0,"seconds":3,"station":"commander","setup":{"soft_target_tank":{
                "index":0,"kind":"kv2","pos_xz":[80,25],"target_xz":[80,89]}},"timeline":[]});
        assert!(legacy["setup"]["soft_target_tank"].get("he_ammo").is_none());
        std::fs::write(&temp,serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(validate_file(temp.to_str().unwrap()).is_ok());
        std::fs::remove_file(temp).unwrap();
    }
}
