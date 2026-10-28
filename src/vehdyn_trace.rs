//! `vehdyn-trace PREFIX [SEED] [FROM_M] [SECONDS]`: the vehicle-dynamics receipt on Canyon v2
//! (2026-10-28). The Tiger is staged (disclosed: `debug_place_tiger`, invincible: a ride trace,
//! not a survivability result) `FROM_M` metres along the authored route, short of the hamlet, and
//! the rule-based crew (`CanyonPilot`) drives on through the hamlet street and the rough ground
//! for `SECONDS`. Every frame logs the sprung hull (`suspension`) beside the rigid rest it used to
//! snap to (`tank_support::settle`, still computed for the refusal rules), the speed, gear and what
//! carries it. Then a T-34 (`vehicle_spec::T34_76` on the AI drivetrain) is staged at the same
//! point on the same heightfield and steered along the same route at 60 % of its road speed,
//! logging its sprung pose beside the rigid six-contact fit (`vehicle_ground::solve`) the AI
//! vehicles used to take. Writes `PREFIX_tiger.jsonl`, `PREFIX_t34.jsonl` and `PREFIX_summary.json`.

use crate::game::{CanyonPilot, TankInput};
use glam::Vec2;
use std::{fs, io::Write};

#[derive(Default)]
struct Roughness {
    frames: usize,
    pitch_rate_sq: f64,
    roll_rate_sq: f64,
    heave_accel_sq: f64,
    max_pitch_rate: f32,
    max_roll_rate: f32,
    max_heave_step: f32,
    last: Option<(f32, f32, f32)>,
    last_vy: Option<f32>,
}

impl Roughness {
    fn add(&mut self, y: f32, pitch: f32, roll: f32, dt: f32) {
        if let Some((y0, p0, r0)) = self.last {
            let (pr, rr, vy) = ((pitch - p0) / dt, (roll - r0) / dt, (y - y0) / dt);
            self.pitch_rate_sq += (pr as f64).powi(2);
            self.roll_rate_sq += (rr as f64).powi(2);
            self.max_pitch_rate = self.max_pitch_rate.max(pr.abs());
            self.max_roll_rate = self.max_roll_rate.max(rr.abs());
            self.max_heave_step = self.max_heave_step.max((y - y0).abs());
            if let Some(v0) = self.last_vy {
                self.heave_accel_sq += (((vy - v0) / dt) as f64).powi(2);
                self.frames += 1;
            }
            self.last_vy = Some(vy);
        }
        self.last = Some((y, pitch, roll));
    }
    fn json(&self) -> serde_json::Value {
        let n = self.frames.max(1) as f64;
        serde_json::json!({
            "rms_pitch_rate_deg_s": (self.pitch_rate_sq / n).sqrt().to_degrees(),
            "rms_roll_rate_deg_s": (self.roll_rate_sq / n).sqrt().to_degrees(),
            "rms_heave_accel_m_s2": (self.heave_accel_sq / n).sqrt(),
            "max_pitch_rate_deg_s": self.max_pitch_rate.to_degrees(),
            "max_roll_rate_deg_s": self.max_roll_rate.to_degrees(),
            "max_heave_step_m_per_frame": self.max_heave_step,
        })
    }
}

fn route_point_from_start(route: &[Vec2], along_m: f32) -> (Vec2, f32, usize) {
    let mut remaining = along_m;
    for i in 0..route.len().saturating_sub(1) {
        let d = route[i + 1] - route[i];
        let len = d.length();
        if remaining <= len || i + 2 == route.len() {
            let t = (remaining / len.max(1e-3)).clamp(0.0, 1.0);
            return (route[i] + d * t, d.x.atan2(d.y), i + 1);
        }
        remaining -= len;
    }
    (route[0], 0.0, 1)
}

pub fn run(prefix: &str, seed: u64, from_m: f32, seconds: f32) -> anyhow::Result<()> {
    anyhow::ensure!(seconds.is_finite() && seconds > 0.0, "positive duration required");
    let (device, queue, mut r, mut g, manifest) = crate::canyon_attempt::build_course(seed)?;
    let route = g.canyon_route().ok_or_else(|| anyhow::anyhow!("no course route"))?;
    let (at, heading, first_waypoint) = route_point_from_start(&route, from_m);
    g.god = true;
    g.debug_place_tiger(at, heading);
    g.update(&TankInput::default(), 0.0, &mut r);
    let mut pilot = CanyonPilot::new(&g);
    let dt = 1.0 / 60.0;
    let mut log = std::io::BufWriter::new(fs::File::create(format!("{prefix}_tiger.jsonl"))?);
    let (mut sprung, mut rigid) = (Roughness::default(), Roughness::default());
    let start = g.player.pos;
    let (mut max_speed, mut carried) = (0.0_f32, std::collections::BTreeMap::<String, usize>::new());
    for step in 0..(seconds / dt).ceil() as usize {
        let t = step as f32 * dt;
        let input = pilot.step(&mut g, &r, t, dt);
        g.update(&input, dt, &mut r);
        if step % 8 == 0 {
            crate::simulation_uploads::drain(&device, &queue)?;
        }
        let p = &g.player;
        let rest = g.player_rigid_rest();
        sprung.add(p.pos.y, p.pitch, p.roll, dt);
        if let Some((y, pitch, roll)) = rest {
            rigid.add(y, pitch, roll, dt);
        }
        max_speed = max_speed.max(p.speed.abs());
        let ground = g.tank_ground_carrier().to_owned();
        *carried.entry(ground.clone()).or_default() += 1;
        writeln!(log, "{}", serde_json::json!({
            "t": t, "x": p.pos.x, "z": p.pos.z, "speed": p.speed, "gear": g.player_powertrain.gear,
            "y": p.pos.y, "pitch_deg": p.pitch.to_degrees(), "roll_deg": p.roll.to_degrees(),
            "rigid_y": rest.map(|r| r.0), "rigid_pitch_deg": rest.map(|r| r.1.to_degrees()),
            "rigid_roll_deg": rest.map(|r| r.2.to_degrees()),
            "travel": p.ride.travel, "along_accel": p.ride.along_accel, "lateral_accel": p.ride.lateral_accel,
            "carried_by": ground,
        }))?;
        if step % 60 == 0 {
            println!("[vehdyn] t {t:5.1} speed {:5.2} gear {} pitch {:6.2} (rigid {:6.2}) roll {:6.2} y {:.3}",
                p.speed, g.player_powertrain.gear, p.pitch.to_degrees(),
                rest.map_or(f32::NAN, |r| r.1.to_degrees()), p.roll.to_degrees(), p.pos.y);
        }
    }
    log.flush()?;
    let tiger_distance = Vec2::new(g.player.pos.x - start.x, g.player.pos.z - start.z).length();

    // The T-34 on the same course: route following on the drivetrain and the sprung hull.
    let mut t34 = crate::tank::Tank::new(at.x, at.y, heading, &g.hf, crate::tank::T34_PARTS);
    t34.kind = crate::tank::EnemyKind::T34;
    let mut waypoint = first_waypoint;
    let mut log = std::io::BufWriter::new(fs::File::create(format!("{prefix}_t34.jsonl"))?);
    let (mut t34_sprung, mut t34_rigid) = (Roughness::default(), Roughness::default());
    let mut t34_max_speed = 0.0_f32;
    let mut reached_speed_at: Option<f32> = None;
    for step in 0..(seconds / dt).ceil() as usize {
        let t = step as f32 * dt;
        let here = Vec2::new(t34.pos.x, t34.pos.z);
        while waypoint + 1 < route.len() && here.distance(route[waypoint]) < 12.0 {
            waypoint += 1;
        }
        let to = route[waypoint] - here;
        let mut err = to.x.atan2(to.y) - t34.heading;
        while err > std::f32::consts::PI { err -= std::f32::consts::TAU; }
        while err < -std::f32::consts::PI { err += std::f32::consts::TAU; }
        let steer = (err * 1.8).clamp(-1.0, 1.0);
        let drive = if waypoint + 1 >= route.len() && to.length() < 12.0 { 0.0 } else { 0.6 * (1.0 - err.abs().min(1.0) * 0.6) };
        t34.update(drive, steer, 0.0, 0.0, dt, &g.hf);
        let fit = crate::vehicle_ground::solve(Vec2::new(t34.pos.x, t34.pos.z), t34.heading,
            t34.parts.hull_half_extents, |x, z| g.hf.vehicle_height_at(x, z));
        t34_sprung.add(t34.pos.y, t34.pitch, t34.roll, dt);
        if let Some(f) = fit {
            t34_rigid.add(f.height, f.pitch, f.roll, dt);
        }
        t34_max_speed = t34_max_speed.max(t34.speed.abs());
        if reached_speed_at.is_none() && t34.speed >= 0.9 * 0.6 * crate::vehicle_spec::T34_76.road_speed {
            reached_speed_at = Some(t);
        }
        writeln!(log, "{}", serde_json::json!({
            "t": t, "x": t34.pos.x, "z": t34.pos.z, "speed": t34.speed, "gear": t34.powertrain.gear,
            "y": t34.pos.y, "pitch_deg": t34.pitch.to_degrees(), "roll_deg": t34.roll.to_degrees(),
            "rigid_y": fit.map(|f| f.height), "rigid_pitch_deg": fit.map(|f| f.pitch.to_degrees()),
            "rigid_roll_deg": fit.map(|f| f.roll.to_degrees()), "travel": t34.ride.travel,
        }))?;
        if step % 60 == 0 {
            println!("[vehdyn-t34] t {t:5.1} speed {:5.2} gear {} pitch {:6.2} (rigid {:6.2}) roll {:6.2}",
                t34.speed, t34.powertrain.gear, t34.pitch.to_degrees(),
                fit.map_or(f32::NAN, |f| f.pitch.to_degrees()), t34.roll.to_degrees());
        }
    }
    log.flush()?;
    let t34_distance = Vec2::new(t34.pos.x - at.x, t34.pos.y - at.y).length();
    let summary = serde_json::json!({
        "schema": "panzer.vehdyn-trace.v1", "seed": seed, "from_m": from_m, "seconds": seconds,
        "manifest": manifest, "staged_at": [at.x, at.y], "heading": heading,
        "staging": "debug_place_tiger on the authored route; god mode (ride trace, not survivability); CanyonPilot drives",
        "tiger": {"distance_m": tiger_distance, "max_speed_m_s": max_speed, "carried_by_frames": carried,
            "sprung": sprung.json(), "rigid_rest": rigid.json()},
        "t34": {"distance_m": t34_distance, "max_speed_m_s": t34_max_speed,
            "reached_60pct_road_speed_s": reached_speed_at,
            "old_kinematic_controller_time_to_same_speed_s": 0.9 * 0.6 * crate::vehicle_spec::T34_76.road_speed / 6.0,
            "sprung": t34_sprung.json(), "rigid_fit": t34_rigid.json()},
    });
    fs::write(format!("{prefix}_summary.json"), serde_json::to_vec_pretty(&summary)?)?;
    println!("[vehdyn-summary] {summary}");
    Ok(())
}
