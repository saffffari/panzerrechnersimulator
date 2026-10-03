//! Matched-purpose native combat films in distinct maps/locations, using the live audio graph.
//! Initial placements, clock/weather and automated crew are disclosed. No hit, kill, source fire,
//! invulnerability or damage outcome is injected. A scene which fails its observations stays failed.
use super::*;
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
};

#[derive(Clone, Copy)]
struct Scene {
    key: &'static str,
    purpose: &'static str,
    act: u8,
    position: Option<[f32; 2]>,
    offset: [f32; 2],
    phase: DayPhase,
    storm: f32,
    enemies: usize,
    range: f32,
    moving: bool,
    inside: bool,
}
const SCENES:[Scene;9]=[
    Scene{key:"winter_canyon_course",purpose:"Full seeded winter course: actual45mm AT crew, armor contacts and grouped defenders retained",act:1,
        position:None,offset:[0.0,0.0],phase:DayPhase::Day,storm:0.35,enemies:5,range:155.0,moving:false,inside:false},
    Scene{key:"winter_canyon_infantry",purpose:"Full seeded winter course: actual grouped riflemen, ordinary perception/coax fire and source armor",act:1,
        position:None,offset:[0.0,0.0],phase:DayPhase::Day,storm:0.35,enemies:5,range:34.0,moving:false,inside:false},
    Scene{key:"winter_canyon",purpose:"Canonical Canyon V2 winter map: ordered native pixels, source snow terrain, six live distant T-34s and a blizzard",act:1,
        position:None,offset:[0.0,0.0],phase:DayPhase::Day,storm:2.5,enemies:6,range:250.0,moving:false,inside:false},
    Scene{key:"drive_day",purpose:"Drive, stop and fire: drivetrain, terrain response and hull pitch/roll",act:0,
        position:Some([-170.0,-145.0]),offset:[0.0,0.0],phase:DayPhase::Day,storm:0.15,enemies:2,range:220.0,moving:true,inside:false},
    Scene{key:"urban_shadows",purpose:"Combat at the authored city spawn: building occlusion, local shadows and colored impacts",act:1,
        position:None,offset:[0.0,0.0],phase:DayPhase::Day,storm:0.10,enemies:2,range:90.0,moving:false,inside:false},
    Scene{key:"distant_blizzard",purpose:"Six live T-34s at distance: visibility, snowfall, fog, shell travel and delayed reports",act:0,
        position:Some([60.0,-280.0]),offset:[0.0,0.0],phase:DayPhase::Day,storm:2.5,enemies:6,range:250.0,moving:false,inside:false},
    Scene{key:"night_firelight",purpose:"Clear dark night at a second city location: ordinary combat flash and resulting wreck/fire light",act:1,
        position:None,offset:[-180.0,-100.0],phase:DayPhase::Night,storm:0.0,enemies:2,range:100.0,moving:false,inside:false},
    Scene{key:"dusk_country",purpose:"Oderbruch dusk combat: actual ephemeris horizon, long shadows and sky/cloud response",act:2,
        position:None,offset:[0.0,0.0],phase:DayPhase::Dusk,storm:0.12,enemies:2,range:150.0,moving:false,inside:false},
    Scene{key:"crew_machinery",purpose:"Loader in the Halbe map: live breech/reload, machinery, cabin/outdoor atmosphere separation and hearing",act:3,
        position:None,offset:[0.0,0.0],phase:DayPhase::Dawn,storm:0.45,enemies:2,range:140.0,moving:false,inside:true},
];

struct Encoder(Child);
impl Drop for Encoder {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn dither_keys(alternate_mode: u32) -> [&'static str; 2] {
    if alternate_mode==4 {["screen_bayer8","continuous"]}
    else {["bayer4",match alternate_mode {3=>"screen_bayer8",2=>"sphere_bayer8",_=>"blue_noise64"}]}
}
struct DitherVideos {
    keys: [&'static str; 2],
    alternate_mode: u32,
    encoders: Vec<Encoder>,
    pipes: Vec<std::process::ChildStdin>,
    raw: [String; 2],
    native: [u32; 2],
    frames: u32,
    different_pixels: u64,
    frames_with_differences: u32,
    snapshots: Vec<Value>,
}
impl DitherVideos {
    fn new(prefix: &str, native: [u32; 2], fps: u32, alternate_mode: u32) -> anyhow::Result<Self> {
        ensure!((1..=4).contains(&alternate_mode), "unknown comparison mode");
        let keys = dither_keys(alternate_mode);
        let raw = keys.map(|key| format!("{prefix}_{key}_picture.mp4"));
        for key in keys {
            for suffix in ["_picture.mp4", ".mp4", "_contact.jpg"] {
                ensure!(
                    !Path::new(&format!("{prefix}_{key}{suffix}")).exists(),
                    "dither output exists: {prefix}_{key}{suffix}"
                );
            }
            for index in 0..3 {
                ensure!(
                    !Path::new(&format!("{prefix}_{key}_native_{index}.png")).exists(),
                    "dither still exists; choose a fresh prefix"
                );
            }
        }
        let mut encoders = Vec::new();
        let mut pipes = Vec::new();
        for path in &raw {
            let mut child = Encoder(
                Command::new("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-nostdin",
                        "-n",
                        "-f",
                        "rawvideo",
                        "-pixel_format",
                        "rgba",
                        "-video_size",
                        &format!("{}x{}", native[0], native[1]),
                        "-framerate",
                        &fps.to_string(),
                        "-i",
                        "pipe:0",
                        "-an",
                        // Match the native sRGB attachment, with an explicit RGB-to-YUV matrix.
                        "-vf",
                        "scale=iw:ih:in_range=full:out_range=tv:out_color_matrix=bt709,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709",
                        "-c:v",
                        "libx264",
                        "-threads",
                        "2",
                        "-preset",
                        "fast",
                        "-crf",
                        "12",
                        "-pix_fmt",
                        "yuv420p",
                        "-color_range",
                        "tv",
                        "-colorspace",
                        "bt709",
                        "-color_primaries",
                        "bt709",
                        "-color_trc",
                        "iec61966-2-1",
                        "-movflags",
                        "+faststart",
                        path,
                    ])
                    .stdin(Stdio::piped())
                    .spawn()?,
            );
            pipes.push(child.0.stdin.take().context("native dither video pipe")?);
            encoders.push(child);
        }
        Ok(Self {
            keys,
            alternate_mode,
            encoders,
            pipes,
            raw,
            native,
            frames: 0,
            different_pixels: 0,
            frames_with_differences: 0,
            snapshots: Vec::new(),
        })
    }
    fn frame(
        &mut self,
        prefix: &str,
        images: [Vec<u8>; 2],
        tick: u32,
        at: f32,
        snapshot: Option<usize>,
    ) -> anyhow::Result<()> {
        let expected = self.native[0] as usize * self.native[1] as usize * 4;
        ensure!(
            images.iter().all(|v| v.len() == expected),
            "native dither pair dimensions changed"
        );
        let different = images[0]
            .chunks_exact(4)
            .zip(images[1].chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count() as u64;
        self.different_pixels += different;
        self.frames_with_differences += u32::from(different > 0);
        for (pipe, image) in self.pipes.iter_mut().zip(&images) {
            pipe.write_all(image)?;
        }
        if let Some(index) = snapshot {
            let mut paths = Vec::new();
            for (key, image) in self.keys.into_iter().zip(images.iter()) {
                let path = format!("{prefix}_{key}_native_{index}.png");
                ensure!(!Path::new(&path).exists(), "native dither still exists");
                image::save_buffer(
                    &path,
                    image,
                    self.native[0],
                    self.native[1],
                    image::ColorType::Rgba8,
                )?;
                paths.push(path);
            }
            self.snapshots.push(json!({"simulation_tick":tick,"at_s":at,"paths":paths,"different_native_pixels":different}));
        }
        self.frames += 1;
        Ok(())
    }
    fn finish(mut self, prefix: &str, wav: &str, fps: u32) -> anyhow::Result<Value> {
        self.pipes.clear();
        for encoder in &mut self.encoders {
            ensure!(
                encoder.0.wait()?.success(),
                "native dither video encoder failed"
            );
        }
        let mut outputs = Vec::new();
        let duration = self.frames as f32 / fps as f32;
        for (key, raw) in self.keys.into_iter().zip(&self.raw) {
            let video = format!("{prefix}_{key}.mp4");
            mux_duel_soundtrack(raw, wav, &video)?;
            let contact = format!("{prefix}_{key}_contact.jpg");
            let contact_ok = Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-n",
                    "-i",
                    &video,
                    "-vf",
                    &format!(
                        "fps={:.8},scale=640:360:flags=neighbor,tile=3x2",
                        6.0 / duration
                    ),
                    "-frames:v",
                    "1",
                    "-q:v",
                    "2",
                    &contact,
                ])
                .status()?
                .success();
            ensure!(contact_ok, "native dither contact sheet failed");
            outputs.push(
                json!({"pattern":key,"video":video,"picture_only":raw,"contact_sheet":contact,
                "soundtrack":wav,"native_resolution":self.native,"fps":fps,"frames":self.frames}),
            );
        }
        Ok(
            json!({"requested":true,"outputs":outputs,"snapshots":self.snapshots,
            "frames":self.frames,"different_native_pixels_total":self.different_pixels,"frames_with_differences":self.frames_with_differences,
            "same_simulation_and_audio":true,"same_hdr_eye_camera_and_particles":true,"scene_rendered_once_per_pair":true,
            "palette":if self.alternate_mode==4 {"Native Bayer8 current treatment versus continuous tone; palette and LDR dither disabled only in continuous output"}else{"current native game crush settings, identical between paired frames; no PICO8 conversion"},
            "alternate_mode":self.alternate_mode,"patterns":self.keys,
            "thresholds":if self.alternate_mode==4 {json!(["Native Bayer8 centered ranks","none"])}else{json!(["Bayer4 centered ranks",match self.alternate_mode {3=>"native screen Bayer8 centered ranks",2=>"camera-centered world-oriented sphere Bayer8 with2×2 threshold samples",_=>"fixed64x64 toroidal void-and-cluster centered ranks"}])},
            "scope":"Two display-only passes immediately after each ordinary gameplay render. No extra simulation, transport, exposure metering or audio advancement; saved defaults restored. Output-resolution UI omitted equally.",
            "codec_limit":"Playable videos use H264 CRF12 yuv420p, explicit BT709 matrix/primaries and sRGB transfer; exact native RGBA PNGs at the three predetermined times preserve the uncompressed pixel pattern."}),
        )
    }
}
fn save_json(path: &str, value: &Value) -> anyhow::Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn course_defender_shots(receipt:&Value)->u64 {
    receipt["at_guns"].as_array().map_or(0,|guns|guns.iter().map(|g|g["shots"].as_u64().unwrap_or(0)).sum::<u64>())
        + receipt["mixed"]["rifle_shots"].as_array().map_or(0,|teams|teams.iter().filter_map(Value::as_u64).sum::<u64>())
}
/// Observe incoming live fire without adding a masking own gun, turret motion or reload loop.
/// These are the same occupied, mortal station identities used by the other crew films.
fn incoming_small_arms_controls() -> game::TankInput {
    let mut crew=lan::CrewFrame::default();
    for (index,role) in [lan::LanRole::Commander,lan::LanRole::Gunner,
        lan::LanRole::Driver,lan::LanRole::Loader].into_iter().enumerate() {
        crew.roster[role as usize]=index as u32+1;
        crew.inputs[role as usize].post=role;
    }
    game::TankInput {coop:Some(crew),coop_host:true,couch_crew:true,..Default::default()}
}
/// Submission is quantized to the ordinary60Hz update. Unknown/non-wave events remain
/// disclosed and are not falsely joined to a projectile by invented IDs.
fn small_arms_arrival_review(receipt:&Value,origin:f32)->Value {
    let mut checked=0_u64;let mut violations=0_u64;let mut small_arm_arrivals=0_u64;
    let mut unstamped=0_u64;let mut maximum_lateness=0.0_f64;
    for event in receipt["arrivals"].as_array().into_iter().flatten() {
        let Some(arrival)=event["arrival_s"].as_f64().filter(|v|*v>=f64::from(origin)) else {continue;};
        let sfx=event["sfx"].as_str().unwrap_or("");
        if matches!(sfx,"RifleShot"|"TankMgShot"|"RifleArmorTick"|"RifleGroundImpact") {small_arm_arrivals+=1;}
        let Some((emitted,speed))=event["emitted_at_s"].as_f64().zip(event["speed_of_sound_m_s"].as_f64()) else {unstamped+=1;continue;};
        let distance=event["distance_m"].as_f64().unwrap_or(f64::NAN);
        let late=arrival-emitted-distance/speed;
        checked+=1; maximum_lateness=maximum_lateness.max(late);
        if !late.is_finite() || speed<=0.0 || distance<0.0 || late < -0.0005 || late > 1.0/60.0+0.0005 {violations+=1;}
    }
    json!({"checked":checked,"violations":violations,"unstamped_arrivals":unstamped,
        "small_arm_arrivals":small_arm_arrivals,"maximum_tick_lateness_s":maximum_lateness,
        "tolerance_s":1.0/60.0+0.0005,"identity":"source/kind/time only; no exact projectile audio join",
        "scope":"arrival submission versus emitted time plus current source-to-listener range/sound speed; excludes unstamped structural sounds and does not prove output-device timing or perceptual audibility"})
}
fn is_incoming_small_arms(scene:Scene)->bool {matches!(scene.key,"winter_canyon_incoming_small_arms"|"winter_canyon_incoming_small_arms_inside")}
/// These are bounded receipt observations of the player's actual selected bullets. A
/// submitted segment is geometry evidence, never proof that a tracer was visible in the image.
fn coax_tracer_observations(receipt:&Value)->Value {
    let events=receipt["events"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let selected:Vec<u64>=events.iter().filter(|e|e["event"]=="discharge"
        && e["shooter"].as_u64()==Some(u32::MAX as u64)&&e["tracer_selected"]==true)
        .filter_map(|e|e["projectile_id"].as_u64()).collect();
    let submitted=events.iter().filter(|e|e["event"]=="tracer_segment_submitted"
        && e["projectile_id"].as_u64().is_some_and(|id|selected.contains(&id))).count();
    json!({"selected_player_tracer_ids":selected,"matching_segments_submitted":submitted,
        "scope":"Player projectile-ID matches retained in bounded receipt; not total lifetime counts or visible pixels"})
}
fn capture_score_seed(key:&str)->u64 {
    key.bytes().fold(0x53434f52455f3239,|hash,b|(hash^u64::from(b)).wrapping_mul(0x100000001b3))
}
fn initial_enemy_reload(scene:Scene,index:usize)->f32 {
    if scene.enemies>=6 {6.0+index as f32*2.5}
    else if scene.moving {7.0+index as f32*2.0}
    else {4.5+index as f32*1.5}
}
fn description(scene: Scene) -> Value {
    json!({"key":scene.key,"purpose":scene.purpose,"act_id":scene.act,
    "explicit_position_xz":scene.position,"offset_from_authored_spawn":scene.offset,"day_phase":scene.phase.label(),
    "storm_override":scene.storm,"requested_t34_count":scene.enemies,"base_range_m":scene.range,
    "gun_control":if scene.key=="winter_canyon_night_drive" {"no own trigger or aim commands; ordinary live course defenders"} else if is_incoming_small_arms(scene) {"no own trigger, lay or breech-service inputs; ordinary incoming AI only"}
        else if scene.key.starts_with("winter_canyon") {"desired lay through ordinary physical drives; armor review retains first live actor with 0.012rad firing tolerance"} else {"legacy direct scripted gun lay"},
    "initial_enemy_reload_s":if matches!(scene.key,"winter_canyon_course"|"winter_canyon_infantry"|"winter_canyon_infantry_inside"|"winter_canyon_night_drive") || is_incoming_small_arms(scene) {None}else{Some((0..scene.enemies).map(|index|initial_enemy_reload(scene,index)).collect::<Vec<_>>())},
    "camera_motion":if is_incoming_small_arms(scene) {if scene.inside {"stationary occupied Loader, commander hatch closed; ordinary impact feedback retained"} else {"stationary occupied commander head-out; ordinary impact feedback retained"}}
        else if scene.key.starts_with("winter_canyon") && !scene.inside {"12deg commander head-yaw arc1–3s via ordinary input, returns before first shot"} else {"ordinary station camera"},
    "camera":if scene.inside {"occupied Loader station"}else{"occupied Commander head-out"},
    "driver_input":if scene.key=="winter_canyon_night_drive" {"0.30 throttle from0.5s to23s through ordinary co-op Driver, then coast; actual terrain/obstacle constraints"} else if scene.moving {"0.38 throttle from0.5s to5s; modest steering after4s; then stop before firing"}else{"stationary"}})
}

pub fn run(
    prefix: &str,
    key: &str,
    seconds: f32,
    preview: bool,
    look_review: bool,
    dither_video: Option<u32>,
) -> anyhow::Result<()> {
    ensure!(
        seconds.is_finite() && (8.0..=60.0).contains(&seconds),
        "duration must be8..60seconds"
    );
    let prefix = crate::capture_target(prefix);
    if key == "all" {
        ensure!(
            !Path::new(&format!("{prefix}_suite.json")).exists(),
            "suite receipt exists; choose a fresh prefix"
        );
        if let Some(parent) = Path::new(&prefix).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut cases = Vec::new();
        // Each case gets a bounded GPU lifetime; stale resources from six authored worlds cannot
        // accumulate. A failed case does not suppress the other observations or rewrite evidence.
        for scene in SCENES {
            let child_prefix = format!("{prefix}_{}", scene.key);
            let mut cmd = Command::new(std::env::current_exe()?);
            cmd.args([
                "combat-scene-suite",
                &child_prefix,
                scene.key,
                &seconds.to_string(),
            ]);
            if preview {
                cmd.arg("preview");
            }
            if look_review {
                cmd.arg("looks");
            }
            if let Some(mode) = dither_video {
                cmd.arg(match mode { 4 => "no-dither-video", 2 => "obra-dinn-video", _ => "dither-video" });
            }
            let result = cmd.status();
            let receipt_path = format!("{child_prefix}.json");
            let receipt = std::fs::read(&receipt_path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
            cases.push(json!({"scene":description(scene),"process_success":result.as_ref().is_ok_and(|s|s.success()),
                "process_error":result.err().map(|e|e.to_string()),"receipt":receipt_path,"evidence":receipt}));
        }
        let passed = cases
            .iter()
            .all(|c| c["process_success"] == true && c["evidence"]["passed"] == true);
        save_json(
            &format!("{prefix}_suite.json"),
            &json!({"schema":"panzer.combat-scene-suite.v1","passed":passed,
            "preview":preview,"dither_video_requested":dither_video.is_some(),"cases":cases,"scope":"Independent live simulated battles. No inter-case outcome matching or seed search. Visual appearance still requires review of the native captures/contact sheets."}),
        )?;
        ensure!(passed,"one or more suite cases failed; retained individual receipts identify missing observations");
        return Ok(());
    }
    let scene = if key == "winter_canyon_night_drive" {
        Scene{key:"winter_canyon_night_drive",purpose:"Authored Canyon winter start at night: ordinary low-throttle driving, full live seeded defenders, layered night/engine/music without own trigger commands",phase:DayPhase::Night,storm:0.10,moving:true,
            ..*SCENES.iter().find(|s|s.key=="winter_canyon_course").unwrap()}
    } else if key == "winter_canyon_blizzard_close" {
        Scene{key:"winter_canyon_blizzard_close",purpose:"Three ordinary T34s on measured-clear nearby Canyon lanes in heavy snow: shared local optical visibility, real AI fire and propagated audio",storm:1.8,enemies:3,range:50.0,
            ..*SCENES.iter().find(|s|s.key=="winter_canyon").unwrap()}
    } else if key == "winter_canyon_incoming_small_arms" {
        Scene{key:"winter_canyon_incoming_small_arms",purpose:"Incoming seeded Soviet rifle group without own gun masking: real ballistic flight, armor contacts and delayed sound; other live course defenders retained",..*SCENES.iter().find(|s|s.key=="winter_canyon_infantry").unwrap()}
    } else if key == "winter_canyon_incoming_small_arms_inside" {
        Scene{key:"winter_canyon_incoming_small_arms_inside",purpose:"Same seeded incoming rifle group with the occupied Loader inside and commander hatch closed: ordinary armor protection, structural contact audio and delayed reports; no own guns or invulnerability",inside:true,..*SCENES.iter().find(|s|s.key=="winter_canyon_infantry").unwrap()}
    } else if key == "winter_canyon_infantry_inside" {
        Scene{key:"winter_canyon_infantry_inside",purpose:"Same seeded infantry group and ordinary coax engagement from the occupied Loader with commander hatch closed: source armor protection, live small-arms sound and selected MG tracer geometry; mortal crew and ordinary damage",inside:true,..*SCENES.iter().find(|s|s.key=="winter_canyon_infantry").unwrap()}
    } else if key == "distant_blizzard_inside" {
        Scene{key:"distant_blizzard_inside",purpose:"Same six-T34 distant blizzard setup from the occupied Loader: cabin atmosphere, reload and live sound",inside:true,..*SCENES.iter().find(|s|s.key=="distant_blizzard").unwrap()}
    } else if key == "winter_canyon_clear_air" {
        Scene{key:"winter_canyon_clear_air",purpose:"Canonical winter encounter in light snowfall: fixed 12 degree head arc exposes thin branches, terrain and vehicle edges for native AA comparison",storm:0.15,..*SCENES.iter().find(|s|s.key=="winter_canyon").unwrap()}
    } else if key == "winter_canyon_inside" {
        Scene{key:"winter_canyon_inside",purpose:"Same canonical winter Canyon V2 blizzard encounter from the occupied Loader",inside:true,..*SCENES.iter().find(|s|s.key=="winter_canyon").unwrap()}
    } else {
        SCENES
            .iter()
            .copied()
            .find(|s| s.key == key)
            .with_context(|| {
                format!(
                    "unknown scene {key}; use all, winter_canyon_night_drive, winter_canyon_blizzard_close, distant_blizzard_inside, winter_canyon_inside, winter_canyon_incoming_small_arms, winter_canyon_incoming_small_arms_inside, winter_canyon_infantry_inside or {}",
                    SCENES.map(|s| s.key).join(", ")
                )
            })?
    };
    run_case(&prefix, scene, seconds, preview, look_review, dither_video)
}

fn choose_lanes(g: &Game, scene: Scene, weather:&crate::weather_visibility::WeatherView) -> anyhow::Result<(Vec<glam::Vec2>, Vec<Value>)> {
    let origin = glam::Vec2::new(g.player.pos.x, g.player.pos.z);
    let mut positions: Vec<glam::Vec2> = Vec::new();
    let mut evidence = Vec::new();
    // A fixed geometry-only candidate list, independent of any subsequent combat outcome.
    let canyon = scene.key.starts_with("winter_canyon");
    let bearings: &[f32] = if canyon {
        &[0.0,-5.0,5.0,-10.0,10.0,-15.0,15.0,-20.0,20.0,-30.0,30.0,-45.0,45.0,-65.0,65.0,90.0,-90.0,135.0,-135.0,180.0]
    } else {
        &[-20.0,20.0,0.0,40.0,-40.0,65.0,-65.0,90.0,-90.0,135.0,-135.0,180.0]
    };
    for &bearing in bearings {
        for increment in [0.0, 35.0, 70.0] {
            let range = (scene.range + increment).min(300.0);
            let angle = g.player.heading + bearing.to_radians();
            let p = origin + glam::Vec2::new(angle.sin(), angle.cos()) * range;
            let mut report = g.capture_opening_lane(p);
            let source=Vec3::new(p.x,g.hf.vehicle_height_at(p.x,p.y)+tank::T34_PARTS.center_h,p.y);
            let contrast=weather.transmittance(source,g.player.center());
            report["opening_optical_contrast"]=json!(contrast);
            report["same_weather_recognition_threshold"]=json!(crate::weather_visibility::RECOGNITION_CONTRAST);
            let optical=scene.key!="winter_canyon_blizzard_close" || contrast>=crate::weather_visibility::RECOGNITION_CONTRAST;
            report["optical_staging_required"]=json!(scene.key=="winter_canyon_blizzard_close");
            let separated = positions.iter().all(|q| q.distance(p) > 24.0);
            report["separated_from_selected"] = json!(separated);
            let accept = report["accepted"] == true && separated && optical;
            evidence.push(report);
            if accept {
                positions.push(p);
                if positions.len() == scene.enemies {
                    return Ok((positions, evidence));
                }
                if !canyon { break; }
            }
        }
    }
    anyhow::bail!(
        "{} has only{}/{} clear source terrain/cover lanes; no obstacles removed; evidence={}",
        scene.key,
        positions.len(),
        scene.enemies,
        json!(evidence)
    )
}

/// Match the recorder's source-shadow focus while warming the same occupied view. No Game
/// update or sound frame occurs here; only the captured observer may adapt at this frozen pose.
fn warmup_recording_view(g: &Game, r: &mut Renderer, device: &wgpu::Device,
    queue: &wgpu::Queue) -> anyhow::Result<HeadlessWarmupReport> {
    r.begin_coop_shadow_frame(g.player.center());
    let report = converge_headless_screenshot(r, device, queue, 12);
    r.end_coop_shadow_frame();
    report
}

fn run_case(
    prefix: &str,
    scene: Scene,
    seconds: f32,
    preview: bool,
    look_review: bool,
    dither_video: Option<u32>,
) -> anyhow::Result<()> {
    for suffix in [
        ".json",
        ".mp4",
        "_picture.mp4",
        "_soundtrack.wav",
        "_contact.jpg",
        "_first.png",
    ] {
        ensure!(
            !Path::new(&format!("{prefix}{suffix}")).exists(),
            "output exists: {prefix}{suffix}"
        );
    }
    if let Some(parent) = Path::new(prefix).parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Select device-free ordinary Audio before Game construction; no loopback or replacement mix.
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let canyon = scene.key.starts_with("winter_canyon");
    let incoming_small_arms=is_incoming_small_arms(scene);
    let night_drive=scene.key=="winter_canyon_night_drive";
    let course_review=night_drive || matches!(scene.key,"winter_canyon_course"|"winter_canyon_infantry"|"winter_canyon_infantry_inside") || incoming_small_arms;
    let infantry_review=matches!(scene.key,"winter_canyon_infantry"|"winter_canyon_infantry_inside") || incoming_small_arms;
    let mut source_heading = 0.0;
    let selected_manifest = if canyon {
        let path = crate::resource::resolve(crate::canyon::V2_MANIFEST)?;
        let manifest = crate::act_manifest::ActManifest::load(&path)?;
        ensure!(manifest.act_id == 1, "winter canyon must be Act 1");
        let course = crate::canyon::from_edits(manifest.edits())?
            .context("winter canyon manifest has no authored course")?;
        source_heading = course.heading;
        let evidence = json!({"path":path,"act_id":manifest.act_id,"authoring":manifest.value()["authoring"],
            "terrain":manifest.value()["terrain"],"source_start_xz":course.start.to_array(),
            "source_heading_deg":course.heading.to_degrees(),
            "build_path":"Game::build_playtest_act; selected exact Canyon V2 manifest",
            "scope":if course_review {"Authored map with full live seeded course defenders; review control/staging is disclosed separately"}else{"Authored map and ordinary simulation with disclosed review actors; not a full seeded Canyon course run"}});
        crate::act_manifest::select_runtime_manifest(Some(path))?;
        Some(evidence)
    } else { None };
    let (mut r, mut g) = if canyon {
        Game::build_playtest_act(&device, &queue, matches!(scene.phase, DayPhase::Night), &world::act(1))?
    } else {
        Game::build_act(&device, &queue, matches!(scene.phase, DayPhase::Night), &world::act(scene.act))?
    };
    let winter = matches!(g.act.season, world::Season::Winter);
    r.apply_campaign_atmosphere(scene.act, winter, scene.phase);
    r.set_weather_seed(g.act.seed);
    r.set_weather_override(Some(scene.storm));
    r.advance_sky_clock(0.0); // realize the selected medium before geometry-only opening-lane audit
    let source_spawn = g.act.spawn;
    let course_setup=if night_drive {
        g.enable_canyon_course_seeded(29)?;
        source_heading=g.player.heading;
        Some(json!({"seed":29,"identity":{"kind":"authored_course_start"},
            "player_world_m":g.player.pos.to_array(),"source_heading_rad":g.player.heading,
            "active_t34_indices":g.enemies.iter().enumerate().filter(|(i,e)|e.alive&&g.capture_enemy_active(*i)).map(|(i,_)|i).collect::<Vec<_>>(),
            "initial_course":g.canyon_snapshot(),"initial_defenders":g.capture_course_receipt(),
            "scope":"Unmoved authored course spawn with full live seeded defenders; ordinary Driver input only, no forced awareness or outcome"}))
    } else if course_review {
        match g.capture_stage_course(infantry_review) {
            Ok(setup)=>Some(setup),Err(error)=>{
                save_json(&format!("{prefix}.json"),&json!({"schema":"panzer.combat-scene-case.v1","passed":false,
                    "scene":description(scene),"failure_stage":"seeded_course_geometry","reason":format!("{error:#}"),"media_recorded":false}))?;
                return Err(error);
            }
        }
    }else{None};
    let (active,lane_evidence)=if let Some(setup)=course_setup.as_ref() {
        source_heading=g.player.heading;
        let active=setup["active_t34_indices"].as_array().context("course active actors")?.iter()
            .filter_map(|v|v.as_u64().map(|v|v as usize)).collect();
        (active,vec![setup.clone()])
    } else {
        g.reset_match(0x5C3E_1001u32.wrapping_add(scene.act as u32), scene.range);
        let position=scene.position.map(glam::Vec2::from).unwrap_or(source_spawn+glam::Vec2::from(scene.offset));
        g.player.pos=Vec3::new(position.x,g.hf.height_at(position.x,position.y),position.y);
        g.player.heading=source_heading;
        let (positions,evidence)=match choose_lanes(&g,scene,&r.weather_visibility()) {
            Ok(v)=>v,Err(error)=>{
                save_json(&format!("{prefix}.json"),&json!({"schema":"panzer.combat-scene-case.v1","passed":false,
                    "scene":description(scene),"failure_stage":"initial_source_geometry","reason":format!("{error:#}"),"media_recorded":false}))?;
                return Err(error);
            }
        };
        let active=g.capture_stage_t34s(&positions);
        ensure!(active.len()==scene.enemies,"requested source actor slots unavailable");
        for (k,&i) in active.iter().enumerate() {g.enemies[i].reload=initial_enemy_reload(scene,k);}
        (active,evidence)
    };
    g.set_defer_rtt_to_render(true);
    if canyon {g.set_playtest_return_fire(true);}
    let policy = crate::resource::resolve("assets/ai/weights_academy.bin")?;
    if !course_review {g.enemy_policy = Some(ai::Mlp::load(policy.to_str().context("policy path")?)?);}
    ensure!(!g.god, "scene suite refuses invulnerability");
    let view = if scene.inside {
        crate::crew_blizzard_capture::View::Inside
    } else {
        crate::crew_blizzard_capture::View::Cupola
    };
    view.stage(&mut g);
    let score_seed=capture_score_seed(scene.key);
    g.prepare_capture_soundscape(score_seed)?;
    let initial = g.player.pos;
    let initial_clock = r.sky_clock.now().to_iso();
    let small_arms_initial=Some(g.capture_small_arms_receipt());
    let small_arms_audio_initial=Some(g.capture_small_arms_audio_receipt());
    let small_arms_recording_origin=r.time;
    if std::env::var("KESSEL_COMBAT_STILL_ONLY").as_deref() == Ok("1") {
        let input = if incoming_small_arms || night_drive {incoming_small_arms_controls()}
            else if canyon { view.controls_with_slew(&mut g, 0, false) } else { view.controls(&mut g, 0, false) };
        g.update(&input, 1.0 / 60.0, &mut r);
        simulation_uploads::drain(&device, &queue)?;
        let warmup = warmup_recording_view(&g, &mut r, &device, &queue)?;
        let rgba = mechanic_capture::gameplay_native_frame(&mut g, &mut r, &device, &queue)?;
        let image = format!("{prefix}_first.png");
        image::save_buffer(&image, &rgba, r.iw, r.ih, image::ColorType::Rgba8)?;
        save_json(&format!("{prefix}.json"), &json!({"schema":"panzer.combat-scene-opening-still.v1",
            "scene":description(scene),"world_manifest":selected_manifest,"image":image,
            "native_resolution":[r.iw,r.ih],"output_resolution":[r.iw,r.ih],"dither":std::env::var("KESSEL_DITHER_PATTERN").unwrap_or_else(|_|"bayer4".into()),
            "source_spawn_xz":source_spawn.to_array(),"initial_player_world_m":initial.to_array(),
            "source_heading_deg":source_heading.to_degrees(),"initial_clock":initial_clock,
            "weather_seed":g.act.seed,"simulation_seed":0x5C3E_1001u32.wrapping_add(scene.act as u32),
            "opening_lanes":lane_evidence,"active_t34_indices":active,"ordinary_damage_enabled":!g.god,
            "return_fire":g.playtest_return_fire(),"camera_world_m":r.camera.pos.to_array(),
            "soundscape":g.capture_soundscape_receipt(),"soundscape_seed":score_seed,
            "weather":{"snow":r.precipitation,"fog":r.fog_density,"wind":r.weather_wind_world.to_array()},
            "fine_vfx":r.debug_fine_vfx_receipt(),
            "warmup":{"transport_frames":warmup.transport_frames,"exposure_frames":warmup.exposure_frames,"stable":warmup.stable},
            "scope":"Native first frame for map and composition inspection; no claim of completed combat or motion/audio review"}))?;
        return Ok(());
    }
    let fps = if preview { 5 } else { 30 };
    let frame_stride = 60 / fps;
    let raw = format!("{prefix}_picture.mp4");
    let mut encoder = Encoder(
        Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &format!("{}x{}", r.iw, r.ih),
                "-framerate",
                &fps.to_string(),
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-threads",
                "2",
                "-preset",
                "fast",
                "-crf",
                "17",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                &raw,
            ])
            .stdin(Stdio::piped())
            .spawn()?,
    );
    let mut pipe = encoder.0.stdin.take().context("video pipe")?;
    let mut dither_videos = if let Some(mode) = dither_video {
        Some(DitherVideos::new(prefix, [r.iw, r.ih], fps, mode)?)
    } else {
        None
    };
    let steps = (seconds * 60.0).round() as u32;
    let mut frames = 0;
    let mut sound = Vec::new();
    let mut events = Vec::new();
    let mut samples = Vec::new();
    let mut tiger_cursor = game::ShotSequenceCursor::at(g.main_gun_shot_sequence());
    let mut enemy_cursor: Vec<_> = active
        .iter()
        .map(|&i| game::ShotSequenceCursor::at(g.enemy_main_gun_shot_sequence(i)))
        .collect();
    let mut tiger_shots = 0u32;
    let mut enemy_shots = 0u32;
    let mut cooldown = 3.5f32;
    let mut previous_hits = (0u32, 0u64);
    let mut listener_kia = None;
    let mut max_distance = 0.0f32;
    let mut max_pitch = 0.0f32;
    let mut max_roll = 0.0f32;
    let mut breech_states = std::collections::BTreeSet::new();
    let mut max_fire = 0.0f32;
    let mut elevation_motion_ticks = 0u32;
    let mut maximum_elevation_rate = 0.0f32;
    let mut elevation_travel_rad = 0.0f32;
    let mut previous_gun_pitch = g.player.gun_pitch;
    let mut previous_camera_arc = 0.0f32;
    let initial_mg_rounds=g.mg_round_count();
    let mut previous_course_shots=0u64;
    let course_focus=course_setup.as_ref().and_then(|s|s["focus_world_m"].as_array())
        .map(|p|Vec3::new(p[0].as_f64().unwrap() as f32,p[1].as_f64().unwrap() as f32,p[2].as_f64().unwrap() as f32));
    let mut fine_vfx_saturation_observations = 0u64;
    let mut fine_vfx_missing_identity_observations = 0u64;
    let mut fine_vfx_maximum_weight = 0.0f64;
    let mut fine_vfx_effect_frames = 0u32;
    let snapshot_ticks =
        [steps / 8, steps / 2, steps * 7 / 8].map(|tick| tick / frame_stride * frame_stride);
    let mut look_snapshots = Vec::new();
    for tick in 0..steps {
        let at = (tick + 1) as f32 / 60.0;
        cooldown = (cooldown - 1.0 / 60.0).max(0.0);
        let mut input = if incoming_small_arms || night_drive {incoming_small_arms_controls()}
            else if canyon { view.controls_with_slew(&mut g, tick, cooldown <= 0.0) }
            else { view.controls(&mut g, tick, cooldown <= 0.0) };
        if let Some(target)=course_focus.filter(|_|!incoming_small_arms) {
            // Automated crew sees the disclosed initial source target; gun mechanics still slew.
            let laid=g.campaign_ai_order_lay(target,0.002);
            if let Some(crew)=input.coop.as_mut() {
                crew.inputs[lan::LanRole::Gunner as usize].flags=if laid && cooldown<=0.0 {
                    if infantry_review {lan::F_FIRE_COAX}else if g.auto_can_fire(){lan::F_FIRE_MAIN}else{0}
                }else{0};
            }
        }
        if canyon && !scene.inside && !incoming_small_arms {
            // A slow ordinary commander head-look, equally present in both frozen displays.
            // Integrate exact desired arc increments through the same COUCH_LOOK_SENS as input.
            let arc = if (1.0..3.0).contains(&at) {
                12.0_f32.to_radians() * ((at-1.0)*std::f32::consts::FRAC_PI_2).sin()
            } else { 0.0 };
            if let Some(crew) = input.coop.as_mut() {
                crew.inputs[lan::LanRole::Commander as usize].look_dx += (arc-previous_camera_arc)/0.0022;
            }
            previous_camera_arc = arc;
        }
        if scene.moving && (0.5..if night_drive {23.0}else{5.0}).contains(&at) {
            if let Some(crew) = input.coop.as_mut() {
                let driver = &mut crew.inputs[lan::LanRole::Driver as usize];
                driver.drive = if night_drive {0.30}else{0.38};
                driver.steer = if !night_drive && at > 4.0 {
                    0.10 * ((at - 4.0) * 0.7).sin()
                } else {
                    0.0
                };
            }
        }
        g.update(&input, 1.0 / 60.0, &mut r);
        let mechanism = g.capture_gun_mechanism();
        let elevation_rate = mechanism["elevation_normalized_rate"].as_f64().unwrap_or(0.0) as f32;
        elevation_motion_ticks += u32::from(elevation_rate > 0.0);
        maximum_elevation_rate = maximum_elevation_rate.max(elevation_rate);
        elevation_travel_rad += (g.player.gun_pitch - previous_gun_pitch).abs();
        previous_gun_pitch = g.player.gun_pitch;
        if tick % 8 == 0 {
            simulation_uploads::drain(&device, &queue)?;
        }
        if let Some(advance) = tiger_cursor.observe(g.main_gun_shot_sequence()) {
            for sequence in advance.iter() {
                tiger_shots += 1;
                events.push(json!({"at_s":at,"kind":"tiger_discharge","sequence":sequence,"muzzle":g.player.muzzle().0.to_array()}));
            }
            cooldown = 6.5;
        }
        for (k, &i) in active.iter().enumerate() {
            if let Some(advance) = enemy_cursor[k].observe(g.enemy_main_gun_shot_sequence(i)) {
                for sequence in advance.iter() {
                    enemy_shots += 1;
                    events.push(json!({"at_s":at,"kind":"t34_discharge","actor":i,"sequence":sequence,"muzzle":g.enemies[i].muzzle().0.to_array()}));
                }
            }
        }
        let combat = g.capture_combat_receipt();
        let hits = (g.stat_hits_on_player, combat["hits"].as_u64().unwrap_or(0));
        if hits != previous_hits {
            let damage_sound = (hits.0 > previous_hits.0).then(|| g.debug_last_player_damage_sound())
                .flatten().map(|(params, context)| {
                    let profile = acoustics::damage_sound_profile(params, context);
                    json!({"profile_kind":format!("{:?}",profile.kind),
                        "internal_detonation":context.internal_detonation,
                        "catastrophic":context.catastrophic,"caliber_mm":params.caliber_mm,
                        "penetrated":params.penetrated,
                        "scope":"Most recent ordinary incoming-hit audio bridge; read-only, not injected sound"})
                });
            events.push(json!({"at_s":at,"kind":"resolved_hits","incoming":hits.0,"outgoing":hits.1,"combat":combat,
                "incoming_damage_sound":damage_sound,"exterior":g.exterior_damage_receipt()}));
            previous_hits = hits;
        }
        if course_review {
            let defenders=g.capture_course_receipt();let count=course_defender_shots(&defenders);
            if count!=previous_course_shots {
                events.push(json!({"at_s":at,"kind":"course_defender_discharge","total":count,
                    "at_guns":defenders["at_guns"],"rifle_shots":defenders["mixed"]["rifle_shots"]}));
                previous_course_shots=count;
            }
        }
        if !view.listener_alive(&g) && listener_kia.is_none() {
            listener_kia = Some(at);
            events.push(json!({"at_s":at,"kind":"listener_kia"}));
        }
        if let Some(kia) = listener_kia {
            g.set_audio_level(1.0 - death_fade(at - kia));
            if at - kia >= DEATH_FADE {
                g.set_audio_silenced(true);
            }
        }
        sound.extend(g.render_audio_capture_frames(acoustics::SAMPLE_RATE as usize / 60)?);
        max_distance = max_distance.max(g.player.pos.distance(initial));
        let ground = g.tank_ground_state().clone();
        max_pitch = max_pitch.max(ground.pitch_deg.abs());
        max_roll = max_roll.max(ground.roll_deg.abs());
        max_fire = max_fire.max(g.fire_level());
        breech_states.insert(g.breech_label().to_string());
        if tick % frame_stride == 0 {
            if frames == 0 {
                let warmup = warmup_recording_view(&g, &mut r, &device, &queue)?;
                events.push(json!({"at_s":at,"kind":"opening_sensor_warmup","transport_frames":warmup.transport_frames,
                    "exposure_frames":warmup.exposure_frames,"stable":warmup.stable,"exposure":warmup.measured_exposure}));
            }
            let rgba=mechanic_capture::gameplay_native_frame(&mut g,&mut r,&device,&queue)?;
            if frames==0 { image::save_buffer(format!("{prefix}_first.png"),&rgba,r.iw,r.ih,image::ColorType::Rgba8)?; }
            pipe.write_all(&rgba)?;
            let fine = r.debug_fine_vfx_receipt();
            let mut has_effects = false;
            if let Some(views) = fine["views"].as_array() {
                for view in views {
                    let counts = &view["counts"];
                    has_effects |= counts["parents"].as_u64().unwrap_or(0) > 0;
                    fine_vfx_saturation_observations += counts["saturated_parents"].as_u64().unwrap_or(0);
                    fine_vfx_missing_identity_observations += counts["missing_particle_ids"].as_u64().unwrap_or(0);
                    fine_vfx_maximum_weight = fine_vfx_maximum_weight.max(counts["maximum_weight"].as_f64().unwrap_or(0.0));
                }
            }
            fine_vfx_effect_frames += u32::from(has_effects);
            frames += 1;
            if let Some(videos) = dither_videos.as_mut() {
                let pair = r.debug_capture_frozen_dither_comparison(&device, &queue, videos.alternate_mode)?;
                videos.frame(
                    prefix,
                    pair,
                    tick,
                    at,
                    snapshot_ticks
                        .iter()
                        .position(|candidate| *candidate == tick),
                )?;
            }
            if look_review && snapshot_ticks.contains(&tick) {
                let look_prefix = format!("{prefix}_look_{:05}", tick);
                let mut receipt =
                    r.debug_capture_frozen_display_looks(&device, &queue, &look_prefix)?;
                receipt["simulation_tick"] = json!(tick);
                receipt["at_s"] = json!(at);
                look_snapshots.push(receipt);
            }
        }
        if tick % 60 == 0 {
            samples.push(json!({"at_s":at,"player_world_m":g.player.pos.to_array(),"camera_world_m":r.camera.pos.to_array(),
                "camera_forward":r.camera.forward().to_array(),"tiger_alive":g.player.alive,"listener_alive":view.listener_alive(&g),
                "enemy_alive":active.iter().map(|&i|g.enemies[i].alive).collect::<Vec<_>>(),"speed_m_s":g.player.speed,
                "pitch_deg":ground.pitch_deg,"roll_deg":ground.roll_deg,"running_gear":g.capture_running_gear(),"gun_mechanism":g.capture_gun_mechanism(),"breech":g.breech_label(),"engine_on":g.engine_on,
                "weather":{"snow":r.precipitation,"fog":r.fog_density,"wind":r.weather_wind_world.to_array(),"night":r.night},
                "soundscape":g.capture_soundscape_receipt(),"course":course_review.then(||g.capture_course_receipt()),"fine_vfx":r.debug_fine_vfx_receipt(),"clock":r.sky_clock.now().to_iso(),"incoming_hits":hits.0,"outgoing_hits":hits.1}));
            println!(
                "[combat-scene] {} {at:.1}s own={tiger_shots} enemy={enemy_shots} hits={hits:?}",
                scene.key
            );
        }
    }
    drop(pipe);
    ensure!(encoder.0.wait()?.success(), "video encoder failed");
    let latency = crate::reproduction::MASTER_LATENCY_FRAMES;
    sound.extend(g.render_audio_capture_frames(latency)?);
    sound.drain(..latency * 2);
    let peak = sound.iter().fold(0.0f32, |a, &s| a.max(s.abs()));
    let expected_audio = steps as usize * (acoustics::SAMPLE_RATE as usize / 60) * 2;
    let wav = format!("{prefix}_soundtrack.wav");
    acoustics::write_wav_stereo(&wav, &sound, acoustics::SAMPLE_RATE)?;
    let dither_receipt = if let Some(videos) = dither_videos {
        ensure!(
            videos.frames == frames,
            "dither pair frame count differs from ordinary gameplay capture"
        );
        videos.finish(prefix, &wav, fps)?
    } else {
        json!({"requested":false})
    };
    let video = format!("{prefix}.mp4");
    mux_duel_soundtrack(&raw, &wav, &video)?;
    let duration = frames as f32 / fps as f32;
    let contact = format!("{prefix}_contact.jpg");
    let contact_ok = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-i",
            &video,
            "-vf",
            &format!(
                "fps={:.8},scale=640:360:flags=neighbor,tile=3x2",
                6.0 / duration
            ),
            "-frames:v",
            "1",
            "-q:v",
            "2",
            &contact,
        ])
        .status()?
        .success();
    let own_heard = events
        .iter()
        .find(|e| e["kind"] == "tiger_discharge")
        .and_then(|e| e["at_s"].as_f64())
        .is_some_and(|shot| listener_kia.is_none_or(|kia| f64::from(kia) > shot + 0.1));
    let final_course=course_review.then(||g.capture_course_receipt());
    let small_arms_final=Some(g.capture_small_arms_receipt());
    let small_arms_audio_final=Some(g.capture_small_arms_audio_receipt());
    let small_arms_timing=Some(small_arms_arrival_review(
        small_arms_audio_final.as_ref().unwrap(),small_arms_recording_origin));
    let mg_rounds=g.mg_round_count().wrapping_sub(initial_mg_rounds);
    let coax_tracers=coax_tracer_observations(small_arms_final.as_ref().unwrap());
    let course_contact=final_course.as_ref().is_some_and(|c| c["at_guns"].as_array().is_some_and(|guns|guns.iter().any(|g|g["state"]=="Wrecked"))
        || c["mixed"]["soviet_alive"].as_u64().unwrap_or(60)<course_setup.as_ref().unwrap()["initial_defenders"]["mixed"]["soviet_alive"].as_u64().unwrap_or(60));
    let mut checks = json!({"both_sides_fired":tiger_shots>0 && enemy_shots>0,"resolved_hit_observed":previous_hits.0>0 || previous_hits.1>0,
        "audio_live_frames_complete":sound.len()==expected_audio && peak>1e-6 && peak<=1.0001,"listener_heard_own_shot":own_heard,
        "moving_case_travelled":!scene.moving || max_distance>=3.0,"loader_case_cycle_observed":!scene.inside || (tiger_shots>0 && breech_states.len()>=2),
        "contact_sheet_written":contact_ok,"ordinary_damage_enabled":!g.god,
        "native_vfx_sample_budget_respected":fine_vfx_effect_frames>0 && fine_vfx_saturation_observations==0 && fine_vfx_maximum_weight<=0.5,
        "native_vfx_stable_identities_present":fine_vfx_effect_frames>0 && fine_vfx_missing_identity_observations==0});
    if course_review {
        checks["both_sides_fired"]=json!(tiger_shots+mg_rounds>0 && previous_course_shots+enemy_shots as u64>0);
        checks["resolved_hit_observed"]=json!(previous_hits.0>0 || previous_hits.1>0 || course_contact);
        checks["listener_heard_own_shot"]=json!(own_heard || infantry_review && mg_rounds>0);
        checks["seeded_groups_retained"]=json!(final_course.as_ref().is_some_and(|c|c["groups"].as_array().is_some_and(|g|!g.is_empty())));
    }
    if night_drive {
        let fields=checks.as_object_mut().unwrap();
        for key in ["both_sides_fired","resolved_hit_observed","listener_heard_own_shot","loader_case_cycle_observed"] {fields.remove(key);}
        fields.insert("own_guns_remained_quiet".into(),json!(tiger_shots==0&&mg_rounds==0));
        fields.insert("listener_survived_night_drive".into(),json!(listener_kia.is_none()&&view.listener_alive(&g)));
        fields.insert("night_bed_advanced".into(),json!(samples.iter().any(|s|s["soundscape"]["voices"].as_array().is_some_and(|v|v.iter().any(|v|v["lane"]=="night"&&v["frames"].as_u64().unwrap_or(0)>0)))));
        // No ballistic particles are promised by a quiet drive; any submitted grains retain strict gates.
        fields.insert("native_vfx_sample_budget_respected".into(),json!(fine_vfx_saturation_observations==0&&fine_vfx_maximum_weight<=0.5));
        fields.insert("native_vfx_stable_identities_present".into(),json!(fine_vfx_missing_identity_observations==0));
    }
    let score_final=g.capture_soundscape_receipt();
    checks["sound_library_loaded"]=json!(score_final["enabled_cues"].as_u64().unwrap_or(0)>0);
    checks["sound_library_no_decode_failure"]=json!(score_final["failed"].as_array().is_some_and(|v|v.is_empty()));
    checks["layered_sound_sources_advanced"]=json!(samples.iter().any(|s|s["soundscape"]["voices"].as_array().is_some_and(|v|v.iter().filter(|v|v["frames"].as_u64().unwrap_or(0)>0).count()>=2)));
    if infantry_review && !incoming_small_arms {
        let fields=checks.as_object_mut().unwrap();
        fields.remove("loader_case_cycle_observed"); // coax does not operate the main-gun breech
        fields.remove("listener_heard_own_shot"); // no isolated audibility claim from a counter
        fields.insert("actual_coax_rounds_fired".into(),json!(mg_rounds>0));
        fields.insert("main_gun_remained_quiet".into(),json!(tiger_shots==0));
        fields.insert("player_tracer_round_selected".into(),json!(
            coax_tracers["selected_player_tracer_ids"].as_array().is_some_and(|ids|!ids.is_empty())));
        fields.insert("player_tracer_geometry_submitted".into(),json!(
            coax_tracers["matching_segments_submitted"].as_u64().is_some_and(|n|n>0)));
        fields.insert("listener_survived_coax_review".into(),json!(listener_kia.is_none()&&view.listener_alive(&g)));
        if scene.inside {
            let listener=&small_arms_final.as_ref().unwrap()["listener_state"];
            fields.insert("commander_hatch_closed".into(),json!(listener["hatch_open"]==false
                && listener["hatch_fraction"].as_f64().is_some_and(|v|v<=0.001)));
        }
    }
    if incoming_small_arms {
        let initial=small_arms_initial.as_ref().expect("incoming receipt snapshot");
        let final_state=small_arms_final.as_ref().expect("incoming receipt snapshot");
        let delta=|key:&str| final_state[key].as_u64().unwrap_or(0)
            .saturating_sub(initial[key].as_u64().unwrap_or(0));
        let fields=checks.as_object_mut().unwrap();
        // The quiet incoming review deliberately never commands an own discharge.
        fields.remove("both_sides_fired");
        fields.remove("listener_heard_own_shot");
        fields.remove("resolved_hit_observed");
        fields.insert("own_guns_remained_quiet".into(),json!(tiger_shots==0 && mg_rounds==0));
        fields.insert("incoming_small_arm_discharges".into(),json!(delta("discharges")>0
            && final_course.as_ref().is_some_and(|c|c["mixed"]["rifle_shots"][1].as_u64().unwrap_or(0)
                > course_setup.as_ref().unwrap()["initial_defenders"]["mixed"]["rifle_shots"][1].as_u64().unwrap_or(0))));
        fields.insert("actual_player_armor_contacts".into(),json!(delta("player_armor_contacts")>0));
        fields.insert("actual_contact_spark_bursts".into(),json!(delta("armor_spark_bursts")>0));
        fields.remove("loader_case_cycle_observed"); // quiet incoming never requests a reload cycle
        fields.insert("each_player_armor_contact_called_structural_audio".into(),json!(
            delta("player_armor_contacts")>0 && delta("structural_armor_sound_calls")==delta("player_armor_contacts")
                && delta("structural_armor_mixer_calls")==delta("player_armor_contacts")));
        if scene.inside {
            fields.insert("buttoned_listener_survived".into(),json!(listener_kia.is_none()&&view.listener_alive(&g)));
            fields.insert("commander_hatch_closed".into(),json!(
                final_state["listener_state"]["hatch_open"]==false
                    && final_state["listener_state"]["hatch_fraction"].as_f64().is_some_and(|v|v<=0.001)));
        }

        fields.insert("small_arms_wave_arrivals_observed".into(),json!(small_arms_timing.as_ref().unwrap()["small_arm_arrivals"].as_u64().unwrap_or(0)>0));
        fields.insert("propagated_arrivals_match_wavefront".into(),json!(small_arms_timing.as_ref().unwrap()["checked"] .as_u64().unwrap_or(0)>0
            && small_arms_timing.as_ref().unwrap()["violations"].as_u64().unwrap_or(1)==0));
        // Rifle ball has no tracer composition. Selected MG tracer counts are evidence, never
        // a requirement manufactured by adding a masking own MG to this incoming-only scene.
    }
    let passed = checks.as_object().unwrap().values().all(|v| v == true);
    let mut receipt = json!({"schema":"panzer.combat-scene-case.v1","passed":passed,"checks":checks,"scene":description(scene),"world_manifest":selected_manifest,
        "geometry_aa":r.geometry_aa_receipt(),"lighting_visibility":r.lighting_visibility_receipt(),"renderer_baseline":"c6bdb33","renderer_changes":"consolidated MAIN: native geometry AA, native fine VFX, source-derived cabin medium, selectable source geometry map / experimental portable visibility, normal-map variance filtering, seasonal terrain, native dither selector; see geometry_aa and source audit for approximation limits","look_review_requested":look_review,"look_snapshots":look_snapshots,
        "soundscape_capture":{"seed":score_seed,"asset_loading":"synchronous on-demand; ordinary director/transport/mixer","final":score_final,"scope":"Per-second voice sample cursors prove submitted layered sources, not perceptual audibility or a human playthrough"},
        "dither_pattern":std::env::var("KESSEL_DITHER_PATTERN").unwrap_or_else(|_|"bayer4".into()),
        "dither_video":dither_receipt,"course_setup":course_setup,"course_final":final_course,
        "course_defender_discharges":previous_course_shots,"player_mg_rounds":mg_rounds,
        "contact_sheet_sampling":"six evenly spaced ffmpeg fps samples,3columns×2rows; each cell keeps the full gameplay frame",
        "media_recorded":true,"video":video,"soundtrack":wav,"contact_sheet":contact,"duration_s":duration,"fps":fps,
        "fine_vfx_final":r.debug_fine_vfx_receipt(),
        "fine_vfx_all_captured_frames":{"effect_frames":fine_vfx_effect_frames,
            "missing_identity_observations":fine_vfx_missing_identity_observations,
            "saturated_parent_observations":fine_vfx_saturation_observations,"maximum_per_grain_weight":fine_vfx_maximum_weight,
            "scope":"Every captured ordinary frame and prepared optical view, before matched display passes; summed observations are not unique parents"},
        "native_resolution":[r.iw,r.ih],"output_resolution":[r.iw,r.ih],"adapter":adapter.get_info().name});
    let staging = json!({"source_spawn_xz":source_spawn.to_array(),"initial_player_world_m":initial.to_array(),"initial_clock":initial_clock,
        "source_heading_deg":source_heading.to_degrees(),"weather_seed":g.act.seed,"simulation_seed":0x5C3E_1001u32.wrapping_add(scene.act as u32),
        "opening_lanes":lane_evidence,"active_t34_indices":active,"enemy_policy":policy,"events":events,"samples":samples});
    let observations = json!({"tiger_shots":tiger_shots,"enemy_shots":enemy_shots,"incoming_hits":previous_hits.0,"outgoing_hits":previous_hits.1,
        "maximum_travel_m":max_distance,"maximum_pitch_deg":max_pitch,"maximum_roll_deg":max_roll,"breech_states":breech_states,
        "running_gear":g.capture_running_gear(),"maximum_player_fire":max_fire,
        "elevation_motion_s":elevation_motion_ticks as f32/60.0,"maximum_elevation_normalized_rate":maximum_elevation_rate,"elevation_angular_travel_deg":elevation_travel_rad.to_degrees(),"resulting_enemy_wrecks":active.iter().filter(|&&i|!g.enemies[i].alive).count(),"listener_kia_at_s":listener_kia});
    let mut provenance = json!({"audio":{"kind":"ordinary_live_gameplay_sinks","frames":sound.len()/2,"peak":peak,"master_latency_removed_frames":latency,
            "metadata":g.audio_capture_metadata(),"offline_gain_added":false,"offline_replacement":false,
            "authored_source_references":{"burning_fire_laf_dba_1m":86.0,"previous_burning_fire_laf_dba_1m":74.0,
                "fuel_ignition_lpk_db_1m":106.0,"fuel_ignition_laf_dba_1m":92.0,
                "manual_elevation_laf_dba_1m_at_full_speed":75.0,
                "provenance":"Authored source estimates, not historical measurements. No offline boost or replacement of captured audio; incoming detonation timbre preserves calibrated event levels."}},
        "forced_hits":false,"forced_kills":false,"forced_fire":false,"invulnerability":g.god,
        "scope":"Independent fixed-start combat review. Four occupied mortal co-op identities, automated gun lay (winter uses desired lay through ordinary physical drives; legacy scenes use direct scripted lay) and ordinary breech service,6.5s minimum Tiger firing cycle. Enemy AI, swept shells, armor, damage and collisions remain live. Native occupied camera and live audio listener. Not human gameplay or real-time performance. Camera stays for review after KIA with ordinary terminal audio fade.",
        "visual_review_required":"Physics/audio observation checks do not establish attractive grading, visible firelight, good framing, accurate shadows or ideal storm density. Review six contact cells and native video. If combat produces no wreck, do not call this a wreck-fire test."});
    if night_drive {provenance["scope"]=json!("Full live seeded Canyon course at unmoved authored winter spawn, night/weather selected for this review. Occupied mortal co-op Driver uses fixed ordinary throttle; all AI/perception/damage remain live. No own shots requested, no combat guaranteed or forced. Simulation-clock native capture, not human gameplay or real-time performance.");}
    if incoming_small_arms {
        provenance["scope"]=json!("Independent fixed-start incoming-fire review. Four occupied mortal co-op identities, stationary requested crew viewpoint, no own trigger/lay/reload inputs. Actual seeded course defenders, perception, swept bullets, source armor, damage and collisions remain live. Native occupied camera and ordinary audio listener. Other course sound sources remain. Not human gameplay or real-time performance. Camera remains for review after KIA with ordinary terminal audio fade.");
        receipt["small_arms_review"]=json!({"recording_origin_renderer_time_s":small_arms_recording_origin,
            "initial":small_arms_initial,"final":small_arms_final,
            "audio_initial":small_arms_audio_initial,"audio_final":small_arms_audio_final,"arrival_review":small_arms_timing,
            "own_fire_inputs":false,"own_lay_inputs":false,"own_reload_inputs":false,
            "listener_view":view.label(),"commander_hatch_initially_closed":scene.inside,
            "structural_audio_scope":"Same-hull armor method calls are in presentation events, separate from propagated world wave arrivals; measured casualty/silenced flags disclose terminal fade",
            "other_course_sources_retained":true,
            "scope":"Incoming ordinary AI at the existing fixed seeded course group. No projectile, impact, awareness, target or sound is injected. Rifle ball is unlit; MG tracer selection and render submission are observations, not proof of visible pixels. Other live course sources may mask small arms and remain in the recording.",
            "visual_and_listening_review_required":true});
    }
    if infantry_review && !incoming_small_arms {
        receipt["coax_review"]=coax_tracers;
        provenance["scope"]=json!("Fixed-start engagement against the existing seeded infantry group. Four occupied mortal co-op identities; disclosed automated target lay through ordinary gun drives and coax trigger. Loader inside alias closes the commander hatch via the existing crew setup. No main-gun trigger, forced awareness, hit, kill or invulnerability. Other course defenders and sound sources remain live. Counts and submitted geometry do not prove perceptual audibility or visible tracer pixels.");
    }
    receipt["small_arms_presentation"]=json!({"recording_origin_renderer_time_s":small_arms_recording_origin,
        "initial":small_arms_initial,"final":small_arms_final,
        "scope":"Actual bullet discharges, source contacts, spark bursts and tracer geometry submissions. Does not assert visible pixels or force a tracer/hit outcome."});
    receipt["world_sound_arrivals"]=json!({"initial":small_arms_audio_initial,"final":small_arms_audio_final,
        "arrival_review":small_arms_timing,
        "scope":"Bounded ordinary mixer submissions; source/kind/time identity only, no fabricated projectile audio join."});
    for fields in [staging, observations, provenance] {
        receipt
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
    }
    save_json(&format!("{prefix}.json"), &receipt)?;
    ensure!(passed,"{} missed one or more observations; retained receipt/video is diagnostic, not accepted showcase",scene.key);
    Ok(())
}

#[cfg(test)]
mod small_arms_review_tests {
    use super::*;
    #[test]
    fn quiet_capture_controls_do_not_command_fire_or_force_aim() {
        let input=incoming_small_arms_controls();
        assert!(!input.fire && !input.fire2 && !input.coax && !input.mg_coax);
        for post in input.coop.unwrap().inputs {
            assert_eq!(post.flags,0);
            assert_eq!(post.traverse,0.0);assert_eq!(post.elevate,0.0);
        }
    }
    #[test]
    fn capture_music_seeds_are_repeatable_and_scene_specific() {
        let names=["winter_canyon_night_drive","winter_canyon_infantry","winter_canyon_blizzard_close"];
        let seeds=names.map(capture_score_seed);
        assert!(seeds.iter().all(|v|*v!=0));
        for (i,a) in seeds.iter().enumerate() {for b in &seeds[i+1..] {assert_ne!(a,b);}}
        assert_eq!(capture_score_seed(names[0]),seeds[0]);
    }
    #[test]
    fn coax_receipt_requires_selected_player_ids_not_enemy_tracers_or_unselected_ball() {
        let discharge=|id:u64,shooter:u32,tracer:bool|json!({"event":"discharge",
            "projectile_id":id,"shooter":shooter,"tracer_selected":tracer});
        let segment=|id:u64|json!({"event":"tracer_segment_submitted","projectile_id":id});
        let receipt=json!({"events":[discharge(1,u32::MAX,false),discharge(2,10000,true),
            discharge(3,u32::MAX,true),segment(1),segment(2),segment(3),segment(3)]});
        let actual=coax_tracer_observations(&receipt);
        assert_eq!(actual["selected_player_tracer_ids"],json!([3]));
        assert_eq!(actual["matching_segments_submitted"],2);
        let absent=coax_tracer_observations(&json!({"events":[discharge(2,10000,true),segment(2)]}));
        assert_eq!(absent["matching_segments_submitted"],0);
        assert_eq!(absent["selected_player_tracer_ids"],json!([]));
    }
    #[test]
    fn incoming_wave_review_rejects_early_or_dropped_timestamps() {
        let event=|arrival:f64|json!({"sfx":"RifleShot","arrival_s":arrival,"emitted_at_s":2.0,
            "speed_of_sound_m_s":340.0,"distance_m":34.0});
        let okay=small_arms_arrival_review(&json!({"arrivals":[event(2.11)]}),1.0);
        assert_eq!(okay["checked"],1);assert_eq!(okay["violations"],0);
        let early=small_arms_arrival_review(&json!({"arrivals":[event(2.08)]}),1.0);
        assert_eq!(early["violations"],1);
        let late=small_arms_arrival_review(&json!({"arrivals":[event(2.14)]}),1.0);
        assert_eq!(late["violations"],1);
        let unknown=small_arms_arrival_review(&json!({"arrivals":[{"sfx":"RifleShot","arrival_s":2.1}]}),1.0);
        assert_eq!(unknown["checked"],0);assert_eq!(unknown["unstamped_arrivals"],1);
    }
}
