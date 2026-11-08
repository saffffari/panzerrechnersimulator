//! Large-population recording adapter. Shared infantry rules, Tank drivetrain and ballistic
//! tables; deliberately coarse vehicle hit boxes/knockouts, no close-up anatomical FEA.
//! All casualties arise from simulated weapon contacts. The starting roster is finite and
//! every wreck/body persists. Four fixed reserves enter at the field edges; never respawn casualties. The accelerated astronomical clock is presentation only.
use super::*;
use crate::infantry_combat::{self as combat, Combatant, Cover, CoverSet, Team};
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
const DT: f32 = 0.05;
const FPS: u32 = 30;
const TANKS: usize = 96;
const PEOPLE: usize = 1536;
const CENTRE: Vec2 = Vec2::new(-300.0, -600.0);
fn noise(n: u32) -> f32 {
    let mut n = n.wrapping_mul(747796405).wrapping_add(2891336453);
    n = ((n >> ((n >> 28) + 4)) ^ n).wrapping_mul(277803737);
    ((n >> 22) ^ n) as f64 as f32 / u32::MAX as f32
}
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
struct Vehicle {
    tank: Tank,
    team: Team,
    model: usize,
    ammo: u32,
    arrival: f32,
    died: Option<f32>,
}
struct Round {
    owner: usize,
    pos: Vec3,
    vel: Vec3,
    start: Vec3,
    shell: &'static crate::ballistics::ShellSpec,
}
struct Army {
    vehicles: Vec<Vehicle>,
    people: Vec<Combatant>,
    rounds: Vec<Round>,
    time: f32,
    tick: u64,
    shots: u64,
    hits: u64,
    rifle_shots: u64,
    events: Vec<Value>,
    deaths: Vec<Option<f32>>,
    arrivals: Vec<f32>,
    seed: u32,
    fx: Vec<(Vec3, Vec3, bool)>,
}
impl Army {
    fn new(g: &Game, seed: u32) -> Self {
        let mut vehicles = Vec::new();
        let mut people = Vec::new();
        let mut arrivals = Vec::new();
        for side in 0..2 {
            let sign = if side == 0 { 1.0 } else { -1.0 };
            let team = if side == 0 {
                Team::German
            } else {
                Team::Soviet
            };
            for n in 0..48 {
                let x = CENTRE.x - sign * (170.0 + (n / 12) as f32 * 1.0);
                let z = CENTRE.y + (n % 12) as f32 * 20.0 - 110.0;
                let model = if side == 0 {
                    0
                } else if n % 3 == 0 {
                    2
                } else {
                    1
                };
                let parts = match model {
                    0 => tank::TIGER_PARTS,
                    2 => tank::IS2_PARTS,
                    _ => tank::T34_PARTS,
                };
                let mut t = Tank::new(x, z, sign * std::f32::consts::FRAC_PI_2, &g.hf, parts);
                t.kind = if model == 2 {
                    EnemyKind::Is2
                } else {
                    EnemyKind::T34
                };
                t.reload = 2.0 + noise(seed + n as u32 + side * 48) * 8.0;
                vehicles.push(Vehicle {
                    tank: t,
                    team,
                    model,
                    ammo: 40,
                    arrival: (n / 12) as f32 * 180.0,
                    died: None,
                });
            }
            for n in 0..768 {
                let id = people.len() as u32;
                let x = CENTRE.x - sign * (150.0 + ((n % 192) / 32) as f32 * 5.0);
                let z = CENTRE.y - 116.0 + (n % 32) as f32 * 7.4 + (noise(id + seed) - 0.5) * 2.0;
                let mut a = Combatant::new(
                    id,
                    team,
                    Vec3::new(x, g.hf.height_at(x, z), z),
                    sign * std::f32::consts::FRAC_PI_2,
                );
                a.cooldown = noise(id + seed) * 1.6;
                a.reserve = 60;
                a.pose_support = combat::PoseSupport::StandingOnly;
                people.push(a);
                arrivals.push((n / 192) as f32 * 180.0);
            }
        }
        Self {
            vehicles,
            people,
            rounds: Vec::new(),
            time: 0.,
            tick: 0,
            shots: 0,
            hits: 0,
            rifle_shots: 0,
            events: Vec::new(),
            deaths: vec![None; PEOPLE],
            arrivals,
            seed,
            fx: Vec::new(),
        }
    }
    fn counts(&self) -> Value {
        json!({"tank_wrecks":self.vehicles.iter().filter(|v|!v.tank.alive).count(),"infantry_down":self.people.iter().filter(|a|!a.alive()).count(),"tank_rounds":self.shots,"tank_contacts":self.hits,"rifle_rounds":self.rifle_shots})
    }
    fn cover(&self) -> Vec<Cover> {
        self.vehicles
            .iter()
            .filter(|v| v.arrival <= self.time)
            .map(|v| {
                let m = v.tank.hull_model();
                let mut min = Vec3::splat(f32::INFINITY);
                let mut max = Vec3::splat(f32::NEG_INFINITY);
                for x in [-1.8, 1.8] {
                    for y in [0.2, 2.5] {
                        for z in [-3.5, 3.5] {
                            let p = m.transform_point3(Vec3::new(x, y, z));
                            min = min.min(p);
                            max = max.max(p);
                        }
                    }
                }
                Cover { min, max }
            })
            .collect()
    }
    fn step(&mut self, g: &Game) {
        self.time += DT;
        self.tick += 1;
        let deployed: Vec<_> = self
            .vehicles
            .iter()
            .map(|v| v.arrival <= self.time)
            .collect();
        let poses: Vec<_> = self
            .vehicles
            .iter()
            .map(|v| (v.tank.pos, v.tank.alive && v.arrival <= self.time, v.team))
            .collect();
        let static_cover = g.world_cover(Vec::new());
        for (i, v) in self.vehicles.iter_mut().enumerate() {
            if !v.tank.alive || v.arrival > self.time {
                continue;
            }
            let target = poses
                .iter()
                .enumerate()
                .filter(|(_, (_, alive, team))| *alive && *team != v.team)
                .filter(|(_, (p, _, _))| {
                    combat::clear_segment(
                        v.tank.pos + Vec3::Y * 1.8,
                        *p + Vec3::Y * 1.5,
                        &static_cover,
                    )
                })
                .min_by(|a, b| {
                    a.1 .0
                        .distance_squared(v.tank.pos)
                        .total_cmp(&b.1 .0.distance_squared(v.tank.pos))
                });
            let Some((target_id, (target, _, _))) = target else {
                continue;
            };
            let delta = *target - v.tank.pos;
            let yaw = delta.x.atan2(delta.z);
            let dist = delta.length();
            let direction = delta.normalize();
            let lane_clear = !poses.iter().enumerate().any(|(j, (p, _, _))| {
                j != i && j != target_id && deployed[j] && {
                    let along = (*p - v.tank.pos).dot(direction);
                    along > 0.
                        && along < dist - 4.
                        && (*p - (v.tank.pos + direction * along)).length() < 3.5
                }
            });
            let blocked = poses.iter().enumerate().any(|(j, (p, _, _))| {
                j != i
                    && deployed[j]
                    && p.distance(v.tank.pos) < 9.0
                    && (p - v.tank.pos).dot(v.tank.forward()) > 0.
            });
            let drive = if !blocked {
                if !lane_clear {
                    0.3
                } else if dist > 110.0 {
                    0.48
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let desired_heading = yaw
                + if lane_clear {
                    0.0
                } else {
                    if i % 2 == 0 {
                        0.8
                    } else {
                        -0.8
                    }
                };
            let steer = (wrap(desired_heading - v.tank.heading) * 1.2).clamp(-1., 1.);
            let traverse = (wrap(yaw - v.tank.heading - v.tank.turret_yaw) * 2.).clamp(-1., 1.);
            v.tank.update(drive, steer, traverse, 0., DT, &g.hf);
            let aim = *target + Vec3::Y * 1.4;
            let muzzle = v.tank.muzzle().0;
            let d = (aim - muzzle).normalize_or_zero();
            let own_lane_clear = !poses.iter().enumerate().any(|(j, (p, _, team))| {
                j != i && deployed[j] && *team == v.team && {
                    let along = (*p + Vec3::Y * 1.4 - muzzle).dot(d);
                    along > 0.0
                        && along < dist
                        && (*p + Vec3::Y * 1.4 - (muzzle + d * along)).length() < 3.5
                }
            });
            if lane_clear
                && own_lane_clear
                && v.ammo > 0
                && v.tank.reload <= 0.
                && wrap(yaw - v.tank.heading - v.tank.turret_yaw).abs() < 0.025
                && dist < 330.
            {
                let shell = match v.model {
                    0 => &crate::ballistics::PZGR_39,
                    2 => &crate::ballistics::BR_471,
                    _ => &crate::ballistics::BR_350B,
                };
                let j = noise(self.seed.wrapping_add(self.shots as u32));
                let d = (d + Vec3::new(
                    0.,
                    (j - 0.5) * 0.004,
                    (noise(i as u32 + self.tick as u32) - 0.5) * 0.004,
                ))
                .normalize();
                let tof = dist / shell.muzzle_velocity;
                let vel = d * shell.muzzle_velocity + Vec3::Y * (4.905 * tof);
                self.rounds.push(Round {
                    owner: i,
                    pos: muzzle,
                    vel,
                    start: muzzle,
                    shell,
                });
                self.fx.push((muzzle, d, false));
                self.shots += 1;
                v.ammo -= 1;
                v.tank.reload = if v.model == 2 { 18.0 } else { 7.0 };
                v.tank.shot_age = 0.;
            }
        }
        let mut remaining = Vec::new();
        for mut shot in self.rounds.drain(..) {
            let next = shot.pos + shot.vel * DT - Vec3::Y * (4.905 * DT * DT);
            let delta = next - shot.pos;
            let length = delta.length();
            let d = delta / length;
            let terrain_distance = (0..=(length / 1.0).ceil() as usize).find_map(|n| {
                let t = (n as f32).min(length);
                let p = shot.pos + d * t;
                (p.y <= g.hf.height_at(p.x, p.z)).then_some(t)
            });
            let mut nearest = static_cover
                .first_hit(shot.pos, d, length)
                .into_iter()
                .chain(terrain_distance)
                .fold(length, f32::min);
            let mut victim = None;
            for (i, v) in self.vehicles.iter().enumerate() {
                if i == shot.owner || v.arrival > self.time {
                    continue;
                }
                let inv = v.tank.hull_model().inverse();
                let p = inv.transform_point3(shot.pos);
                let q = inv.transform_point3(next);
                let dd = q - p;
                let ll = dd.length();
                if let Some(t) = combat::ray_box(
                    p,
                    dd / ll,
                    Cover {
                        min: Vec3::new(-1.6, 0.25, -3.),
                        max: Vec3::new(1.6, 2.3, 3.),
                    },
                    ll,
                ) {
                    let w = t / ll * length;
                    if w < nearest {
                        nearest = w;
                        victim = Some((i, inv.transform_vector3(d)));
                    }
                }
            }
            if let Some((i, local)) = victim {
                self.hits += 1;
                let v = &mut self.vehicles[i];
                let side = local.x.abs() > local.z.abs();
                let facing = if side {
                    crate::ballistics::Facing::HullSide
                } else if local.z < 0. {
                    crate::ballistics::Facing::HullFront
                } else {
                    crate::ballistics::Facing::HullRear
                };
                let scheme = match v.model {
                    0 => crate::ballistics::ArmorScheme::TigerI,
                    2 => crate::ballistics::ArmorScheme::Is2,
                    _ => crate::ballistics::ArmorScheme::T34_76,
                };
                let normal_cos = if side { local.x.abs() } else { local.z.abs() };
                let ob = normal_cos.clamp(0., 1.).acos().to_degrees();
                let point = shot.pos + d * nearest;
                self.fx.push((point, -d, true));
                let event = crate::ballistics::resolve(
                    shot.shell,
                    shot.start.distance(point),
                    scheme.plate(facing),
                    ob,
                    self.seed.wrapping_add(self.hits as u32),
                );
                if event.terminal == crate::ballistics::Terminal::Penetration && v.tank.alive {
                    v.tank.alive = false;
                    v.tank.speed = 0.;
                    v.died = Some(self.time);
                    self.events.push(json!({"time":self.time,"kind":"tank_knockout","id":i,"at":point.to_array(),"shell":shot.shell.name,"terminal":"penetration"}));
                    let mut exposed = self.people.clone();
                    for (i, a) in exposed.iter_mut().enumerate() {
                        if self.arrivals[i] > self.time {
                            a.hp = 0.;
                        }
                    }
                    combat::blast(
                        point,
                        shot.shell.filler_g * 0.001,
                        v.team,
                        &mut exposed,
                        &static_cover,
                    );
                    for (i, a) in self.people.iter_mut().enumerate() {
                        if self.arrivals[i] <= self.time {
                            a.hp = exposed[i].hp;
                            a.suppression = exposed[i].suppression;
                        }
                    }
                }
            } else if nearest >= length && shot.start.distance(next) < 600. {
                shot.pos = next;
                shot.vel -= Vec3::Y * 9.81 * DT;
                remaining.push(shot);
            }
        }
        self.rounds = remaining;
        // 10 Hz decisions and movement; exact shared ray/body rifle contacts. Spatial sensor bins
        // change candidate enumeration, not the shared 45 m visibility/range rule.
        if self.tick % 2 == 0 {
            let cover = g.world_cover(self.cover());
            let mut snapshot = self.people.clone();
            for (i, a) in snapshot.iter_mut().enumerate() {
                if self.arrivals[i] > self.time {
                    a.hp = 0.;
                }
            }
            let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
            for (i, a) in snapshot.iter().enumerate().filter(|(_, a)| a.alive()) {
                cells
                    .entry((
                        (a.pos.x / 45.).floor() as i32,
                        (a.pos.z / 45.).floor() as i32,
                    ))
                    .or_default()
                    .push(i);
            }
            let mut fire = Vec::new();
            for a in self
                .people
                .iter_mut()
                .filter(|a| a.alive() && self.arrivals[a.id as usize] <= self.time)
            {
                let key = (
                    (a.pos.x / 45.).floor() as i32,
                    (a.pos.z / 45.).floor() as i32,
                );
                let mut near = Vec::new();
                for x in -1..=1 {
                    for z in -1..=1 {
                        if let Some(ids) = cells.get(&(key.0 + x, key.1 + z)) {
                            near.extend(ids.iter().map(|i| snapshot[*i].clone()));
                        }
                    }
                }
                let objective = Vec3::new(
                    CENTRE.x + if a.team == Team::German { 120. } else { -120. },
                    a.pos.y,
                    a.pos.z,
                );
                let obs = combat::observe(a, &near, &cover, objective, DT * 2.);
                let action = crate::foot_ai::micro_expert(&obs);
                if let Some(s) = combat::apply_action(a, &obs, action, &cover, DT * 2., self.tick) {
                    fire.push(s);
                }
                a.pos.y = g.hf.height_at(a.pos.x, a.pos.z);
            }
            let mut live = self.people.clone();
            for (i, a) in live.iter_mut().enumerate() {
                if self.arrivals[i] > self.time {
                    a.hp = 0.;
                }
            }
            for shot in fire {
                self.rifle_shots += 1;
                combat::resolve_hitscan(shot, &mut live, &cover);
            }
            for (i, a) in self.people.iter_mut().enumerate() {
                if self.arrivals[i] <= self.time {
                    a.hp = live[i].hp;
                    a.suppression = live[i].suppression;
                }
            }
        }
        for (i, a) in self.people.iter().enumerate() {
            if !a.alive() && self.deaths[i].is_none() {
                self.deaths[i] = Some(self.time);
                self.events.push(
                    json!({"time":self.time,"kind":"infantry_down","id":i,"at":a.pos.to_array()}),
                );
            }
        }
    }
}
struct MeshPool {
    id: usize,
    mats: Vec<Mat4>,
}
fn pool(
    r: &mut Renderer,
    d: &wgpu::Device,
    q: &wgpu::Queue,
    m: &CpuMesh,
    n: usize,
) -> anyhow::Result<MeshPool> {
    println!("[army-mesh] vertices={} indices={} bounds={:?}..{:?}", m.vertices.len(), m.indices.len(), m.min, m.max);
    let prototype = r.register_instanced_prototype(d, q, m);
    let id = r
        .add_moving_instanced_pool(d, prototype, n)
        .context("army instance pool capacity")?;
    Ok(MeshPool {
        id,
        mats: Vec::new(),
    })
}
fn charred(mut m: CpuMesh) -> CpuMesh {
    if let Some(t) = m.texture.as_mut() {
        for px in t.rgba_mut().chunks_exact_mut(4) {
            for c in &mut px[..3] {
                *c = ((*c as f32 * 0.27) + 9.).min(255.) as u8;
            }
        }
    }
    m.roughness_factor = 1.;
    m
}
/// army-timelapse PREFIX SECONDS [scan|render], fixed finite roster; no casualty quota.
pub fn run(prefix: &str, seconds: f32, scan: bool) -> anyhow::Result<()> {
    ensure!(
        (5.0..=300.).contains(&seconds),
        "duration must be 5.0.300 seconds"
    );
    let movie = format!("{prefix}.mp4");
    let receipt = format!("{prefix}.json");
    ensure!(
        !Path::new(&receipt).exists() && !Path::new(&movie).exists(),
        "choose new output prefix"
    );
    crate::act_manifest::select_runtime_manifest(Some("/windows/shop/panzerrechnersystem/assets/worldgen/workshops/spring-bridge-haze-test-aerosol/manifest.v1.json".into()))?;
    std::env::set_var("KESSEL_INTERNAL_RES", std::env::var("KESSEL_ARMY_RES").unwrap_or_else(|_|"1920x1080".into()));
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &crate::world::act(1))?;
    g.mute_audio();
    // This harness does not call Game::update, which normally installs the map atmosphere.
    // Apply its authored lighting explicitly instead of retaining the renderer's generic fog.
    r.apply_authored_lighting(g.scene_environment.lighting, false);
    r.low_clouds = g.scene_environment.low_clouds(&g.hf);
    if std::env::var_os("KESSEL_ARMY_CLEAR_REVIEW").is_some() {r.fog_density=0.;r.low_clouds=None;}
    println!("[army] map atmosphere fog={} aerosol={:?} sky={:?}",r.fog_density,r.aerosol,r.sky_clock);
    r.real = [1.35, 0.85, 15., 7.];
    r.display_dither_pattern = 1.;
    let mut display = crate::renderer::display_tuning::DisplayTuning::at_resolution(r.iw, r.ih);
    display.ordered_dither = 0.55;
    display.ldr_dither = 0.65;
    r.display_tuning = Some(display);
    r.optic_mask = 0.;
    r.interior_light = 0.;
    r.interior_haze = 0.;
    r.rt_mix = 1.;
    r.set_interior_atmosphere(0., 0., Vec3::ZERO);
    let mut army = Army::new(&g, 0xA411_1104);
    let sim_seconds = 900.;
    let frames = (seconds * FPS as f32).round() as usize;
    let steps = (sim_seconds / DT) as usize;
    let mut models = Vec::new();
    let mut bodies = Vec::new();
    if !scan {
        for build in [
            tank::build_tiger as fn() -> [CpuMesh; 3],
            tank::build_t34,
            tank::build_is2,
        ] {
            let source = build();
            for wreck in [false, true] {
                for m in &source {
                    models.push(pool(
                        &mut r,
                        &device,
                        &queue,
                        &if wreck { charred(m.clone()) } else { m.clone() },
                        TANKS,
                    )?);
                }
            }
        }
        // Use authored source body silhouettes. Grounded prone bodies remain in these same pools.
        bodies.push(pool(
            &mut r,
            &device,
            &queue,
            &mixed_combat_runtime::german_infantry_mesh(),
            PEOPLE,
        )?);
        bodies.push(pool(
            &mut r,
            &device,
            &queue,
            &load_exact_soviet_pose_meshes()
                .map(|m| m[0].clone())
                .unwrap_or_else(|_| soviet_infantry_mesh([78, 82, 70])),
            PEOPLE,
        )?);
    }
    let mut encoder = if scan {
        None
    } else {
        Some(
            Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "rgba",
                    "-video_size",
                    &format!("{}x{}",r.iw,r.ih),
                    "-framerate",
                    &FPS.to_string(),
                    "-i",
                    "pipe:0",
                    "-vf",
                    "scale=1920:1080:flags=neighbor",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "fast",
                    "-crf",
                    "15",
                    "-pix_fmt",
                    "yuv444p",
                    "-movflags",
                    "+faststart",
                    &movie,
                ])
                .stdin(Stdio::piped())
                .spawn()?,
        )
    };
    let clock = r.sky_clock;
    let base = r.time;
    let mut timeline = Vec::new();
    let started = std::time::Instant::now();
    let mut done = 0usize;
    let mut daylight_exposure = None;
    let night_ev=std::env::var("KESSEL_ARMY_NIGHT_EV").ok().and_then(|s|s.parse::<f32>().ok()).unwrap_or(6.).clamp(0.,14.);
    let close_review = std::env::var_os("KESSEL_ARMY_CLOSE_REVIEW").is_some();
    for frame in 0..frames {
        let target = ((frame + 1) * steps) / frames;
        while done < target {
            army.step(&g);
            done += 1;
        }
        if frame % FPS as usize == 0 {
            let c = army.counts();
            println!("[army] frame={frame}/{frames} sim={:0.1} {c}", army.time);
            timeline.push(json!({"frame":frame,"sim_seconds":army.time,"counts":c}));
        }
        if scan {
            continue;
        }
        let progress = frame as f32 / (frames - 1) as f32;
        r.time = base + army.time;
        r.look = 6.;
        r.display_grade = [1., 1.15, 0., 1.];
        r.day_night_cycle = false;
        let mut c = clock;
        if !close_review { c.advance(progress as f64 * 3. * 86400.); }
        r.set_sky_clock(c);
        if let Some(daylight)=daylight_exposure {
            // A photographic night bracket, tied only to solar elevation, avoids the metered
            // eye chasing every muzzle flash while keeping the nocturnal battlefield readable.
            let night=((-(c.celestial_state().sun.altitude_deg as f32)-2.)/10.).clamp(0.,1.);
            let smooth=night*night*(3.-2.*night);
            r.exposure_override=Some(daylight*2f32.powf(night_ev*smooth));
        }
        let eye = Vec3::new(CENTRE.x, 140., CENTRE.y - 300. + progress * 35.);
        let at = Vec3::new(CENTRE.x, -4., CENTRE.y - 10.);
        let forward = (at - eye).normalize();
        r.camera.pos = eye;
        r.camera.set_forward(forward);
        r.camera.aspect = 16. / 9.;
        r.camera.fov_y = 65f32.to_radians();
        if close_review {
            let actor = army.vehicles[0].tank.pos;
            r.camera.pos = actor + Vec3::new(0., 18., -30.);
            r.camera.set_forward((actor + Vec3::Y - r.camera.pos).normalize());
        }
        r.reticle = false;
        if let Some(grass) = g.grass.as_mut() {
            for v in army.vehicles.iter().filter(|v|v.arrival<=army.time) {
                grass.press_hull(v.tank.pos, v.tank.heading, Vec2::new(1.8,3.5));
                grass.trample(v.tank.pos, v.tank.heading);
            }
            for (_,a) in army.people.iter().enumerate().filter(|(i,_)|army.arrivals[*i]<=army.time) {
                if a.alive() { grass.press_walker(a.pos); }
                else {
                    let along=combat::prone_rotation(a.yaw)*Vec3::Y;
                    let center=a.pos+along*0.85;
                    let heading=along.x.atan2(along.z);
                    grass.press_hull(center,heading,Vec2::new(0.45,0.95));
                    grass.trample_body(center,heading);
                }
            }
        }
        g.prepare_world_survey(&mut r, sim_seconds / frames as f32);
        for p in models.iter_mut().chain(bodies.iter_mut()) {
            p.mats.clear();
        }
        g.drive_weapon_vfx(sim_seconds / frames as f32, &mut r);
        r.clear_smoke();
        let mut lights = Vec::new();
        for (position, direction, hit) in army.fx.drain(..) {
            if hit {
                g.weapon_vfx.armor_sparks(
                    position,
                    direction,
                    direction,
                    weapon_vfx::Weapon::Cannon(88),
                );
            } else {
                g.weapon_vfx
                    .muzzle(position, direction, weapon_vfx::Weapon::Cannon(88));
            }
            lights.push(PointLight {
                pos: position,
                source_radius: 0.2,
                color: Vec3::new(1., 0.4, 0.08),
                radiant_intensity: 3000.,
            });
        }
        // Integrate each shutter interval's real discharges/contacts into this time-lapse frame.
        // Damage is already resolved; these are only the ordinary visible weapon effects.
        g.drive_weapon_vfx(0., &mut r);
        for (i, v) in army.vehicles.iter().enumerate() {
            if v.arrival > army.time {
                continue;
            }
            let offset = v.model * 6 + if v.tank.alive { 0 } else { 3 };
            for (k, m) in [
                v.tank.hull_model(),
                v.tank.turret_model(),
                v.tank.visual_gun_model(),
            ]
            .into_iter()
            .enumerate()
            {
                models[offset + k].mats.push(m);
            }
            if let Some(t) = v.died {
                let age = army.time - t;
                if age < 500. {
                    let p = v.tank.pos + Vec3::Y * 2.;
                    let h = (age * 0.13).min(22.);
                    r.add_smoke_sweep(
                        p,
                        p + Vec3::new(age.min(35.) * 0.12, h, 0.),
                        2. + h * 0.07,
                        0.3 * (1. - age / 500.),
                        i as f32 + army.time * 0.03,
                        0.08,
                    );
                    if age < 200. {
                        lights.push(PointLight {
                            pos: p,
                            source_radius: 0.7,
                            color: Vec3::new(1., 0.17, 0.025),
                            radiant_intensity: 900. * (1. - age / 200.),
                        });
                    }
                }
            }
        }
        for (i, a) in army.people.iter().enumerate() {
            if army.arrivals[i] > army.time {
                continue;
            }
            let team = usize::from(a.team == Team::Soviet);
            let rot = if a.alive() {
                Quat::from_rotation_y(std::f32::consts::PI - a.yaw)
            } else {
                combat::prone_rotation(a.yaw)
            };
            let y = if a.alive() {
                if a.velocity.length() > 0.1 {
                    (army.time * 13. + i as f32).sin().abs() * 0.035
                } else {
                    0.
                }
            } else {
                0.18
            };
            bodies[team]
                .mats
                .push(Mat4::from_rotation_translation(rot, a.pos + Vec3::Y * y));
        }
        for p in models.iter().chain(bodies.iter()) {
            ensure!(
                r.set_moving_instanced_mats(p.id, &p.mats),
                "army instance update failed"
            );
        }
        if frame==0 {println!("[army] visibility {}", r.debug_instance_visibility(&models.iter().chain(bodies.iter()).map(|p|p.id).collect::<Vec<_>>()));}
        r.set_lights(&lights);
        if frame == 0 {
            let eye = crate::converge_headless_screenshot(&mut r, &device, &queue, 24)?;
            println!("[army] exposure {eye:?}");
            // A photographic time lapse keeps one exposure across the compressed sky cycle.
            // This is local to this capture; ordinary gameplay retains its adapting eye.
            r.exposure_override = eye.measured_exposure.or(r.exposure_override);
            daylight_exposure = r.exposure_override;
            println!("[army] first actor {:?}, camera {:?}",army.vehicles[0].tank.pos,r.camera.pos);
        }
        if frame == frames / 2 && std::env::var_os("KESSEL_ARMY_COLOR_DIAGNOSTICS").is_some() {
            let photo = r.debug_frame_photometry(&device, &queue)?;
            r.debug_dump_crush_input(&device,&queue,"/home/alex/panzer/voxel-runtime/target/temporal-impact-20261104/army/midbattle.hdr",&photo.to_json())?;
            let review=r.debug_capture_frozen_display_looks(&device,&queue,"/windows/shop/panzerrechnersystem/captures/screenshots/voxel-runtime-army-midbattle-look-20261104")?;
            std::fs::write("/home/alex/panzer/voxel-runtime/target/temporal-impact-20261104/army/midbattle-looks.json",serde_json::to_vec_pretty(&review)?)?;
        }
        let rgba = r.render_to_rgba(&device, &queue, r.iw, r.ih)?;
        encoder
            .as_mut()
            .unwrap()
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&rgba)?;
        if frame == 0 || frame == frames / 2 || frame + 1 == frames {
            let label = if frame == 0 {
                "opening"
            } else if frame + 1 == frames {
                "aftermath"
            } else {
                "midbattle"
            };
            let name = Path::new(prefix).file_name().unwrap().to_string_lossy();
            let path = format!(
                "/windows/shop/panzerrechnersystem/captures/screenshots/{name}-{label}.png"
            );
            image::save_buffer(path, &rgba, r.iw, r.ih, image::ColorType::Rgba8)?;
        }
    }
    if let Some(mut e) = encoder {
        drop(e.stdin.take());
        ensure!(e.wait()?.success(), "army movie encoding failed");
    }
    let report = json!({"schema":"panzer-army-timelapse-v1","complete":true,"scan":scan,"seed":army.seed,"roster":{"tanks":TANKS,"infantry":PEOPLE},"simulation_seconds":army.time,"simulation_step_s":DT,"infantry_step_s":DT*2.,"video_seconds":seconds,"fps":FPS,"astronomical_days":3,"dolly_m":35.0,"native_grid":[r.iw,r.ih],"daylight_exposure":daylight_exposure,"night_bracket_ev":night_ev,"fog_density":r.fog_density,"display_grade":r.display_grade,"dither_pattern":r.display_dither_pattern,"camera_from":[CENTRE.x,140.,CENTRE.y-300.],"camera_to":[CENTRE.x,140.,CENTRE.y-265.],"counts":army.counts(),"timeline":timeline,"events":army.events,"wall_seconds":started.elapsed().as_secs_f64(),"video":if scan{None}else{Some(movie)},"limits":["Large-battle adapter uses shared native Tank drivetrain, infantry micro-expert/ray-body combat, and ballistic plate tables.","Tank hit boxes and penetration knockouts are coarse distant-battle rules, not per-crew FEA. Infantry use shared gameplay wounds, not anatomy.","Finite roster in four opposing advance waves at 0/180/360/540 seconds. No recycled units, fabricated casualties, or guaranteed casualty quota; bodies and wrecks persist.","Astronomical sky advances three days during fifteen minutes of combat. Infantry use standing source silhouettes; walking bob is a distant presentation approximation.","Silent timelapse; does not time-compress gun reports into an audio wall."]});
    std::fs::write(&receipt, serde_json::to_vec_pretty(&report)?)?;
    println!("[army] complete {receipt}");
    Ok(())
}
