//! Headless proof of the living crew in empty LAN seats (`lan-crew PREFIX`).
//!
//! The act-0 arena with one parked T-34 in view, driven through the LAN host path: the crew frame
//! a `LanSession` host builds (`lan::host_crew_frame`), `TankInput::coop_ai_crew`, the real
//! update. Only simulation steps run; nothing is rendered, so the proof is short on any GPU.
//! Checks: the AI men hold every seat but the host's; the AI driver follows the commander
//! channel's WASD; the AI gunner lays and fires; the AI loader reloads; a human gunner takes the
//! seat and the AI gunner stands down (no shot); the human leaves and the AI gunner returns.
//! JSON receipt at `PREFIX_receipt.json`; the command fails on any FAIL.
use crate::game::{Game, TankInput};
use crate::lan::{self, LanRole, RoleInput};
use glam::Vec3;
use serde_json::{json, Value};

const DT: f32 = 1.0 / 60.0;

fn bit(role: LanRole) -> u8 {
    1 << role as u8
}

fn input(humans: &[(LanRole, RoleInput)]) -> TankInput {
    TankInput {
        coop: Some(lan::host_crew_frame(humans)),
        coop_host: true,
        coop_ai_crew: true,
        ..TankInput::default()
    }
}

/// Keep one live range target near where the first one stood (beside its wreck, in view): a kill
/// by the AI gunner would otherwise leave the later checks without a contact.
fn rearm_target(g: &mut Game, r: &mut crate::renderer::Renderer, spot: Vec3, heading: f32) -> Option<usize> {
    // The gunner's own test: a burning wreck's smoke hides what stands behind it.
    let in_view = |g: &Game, i: usize| g.debug_crew_gunner_sees(i);
    if let Some(live) = (0..g.enemies.len()).find(|&i| in_view(g, i) && g.enemies[i].pos.distance(spot) < 40.0) {
        return Some(live);
    }
    let next = (0..g.enemies.len()).find(|&i| g.enemies[i].alive)?;
    let side = Vec3::new(heading.cos(), 0.0, -heading.sin());
    for offset in [30.0f32, -30.0, 60.0, -60.0, 90.0, -90.0, 130.0, -130.0] {
        let at = spot + side * offset;
        g.debug_place_enemy(next, at.x, at.z, heading);
        g.debug_crew_solo_set_enemy_active(next, true);
        g.update(&TankInput::default(), DT, r);
        if in_view(g, next) {
            g.debug_engage_all();
            return Some(next);
        }
    }
    None
}

fn commander(drive: f32) -> (LanRole, RoleInput) {
    (LanRole::Commander, RoleInput { post: LanRole::Commander, drive, ..RoleInput::default() })
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[lan-crew] adapter={} ({:?}, {:?})", info.name, info.device_type, info.backend);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &crate::world::act(0))?;
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    g.god = true;
    g.targets_only = true;
    g.brawl = false;
    g.debug_hold_enemy_fire();
    for i in 1..g.enemies.len() {
        g.debug_crew_solo_set_enemy_active(i, false);
        g.debug_place_enemy(i, g.player.pos.x + 400.0 + 40.0 * i as f32, g.player.pos.z - 400.0, 0.0);
    }
    g.set_lan_role(LanRole::Commander);
    let alone = [commander(0.0)];
    let heading = g.player.heading;
    let mut placed = None;
    'search: for &range in &[220.0f32, 300.0, 180.0, 400.0] {
        for &bearing_deg in &[0.0f32, 20.0, -20.0, 40.0, -40.0] {
            let bearing = heading + bearing_deg.to_radians();
            let at = g.player.pos + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * range;
            g.debug_place_enemy(0, at.x, at.z, bearing + std::f32::consts::PI);
            g.update(&input(&alone), DT, &mut r);
            let (recognizable, los) = g.debug_crew_solo_target_visible(0);
            if recognizable && los {
                placed = Some((range, bearing_deg, at, bearing + std::f32::consts::PI));
                break 'search;
            }
        }
    }
    g.debug_engage_all();

    let mut checks: Vec<Value> = Vec::new();
    let mut pass = true;
    let mut check = |label: &str, ok: bool, detail: Value| {
        println!("[lan-crew] {label} — {}", if ok { "PASS" } else { "FAIL" });
        checks.push(json!({"check": label, "pass": ok, "detail": detail}));
        pass &= ok;
    };
    let run = |g: &mut Game, r: &mut crate::renderer::Renderer, humans: &[(LanRole, RoleInput)], seconds: f32, until: &dyn Fn(&Game) -> bool| -> f32 {
        let steps = (seconds / DT).round() as usize;
        for step in 0..steps {
            g.update(&input(humans), DT, r);
            if until(g) {
                return step as f32 * DT;
            }
        }
        seconds
    };

    check("target placed in the gunner's view", placed.is_some(),
        json!({"range_m_bearing_deg": placed.map(|(range, bearing, _, _)| (range, bearing))}));
    let (spot, facing) = placed.map_or((g.player.pos, 0.0), |(_, _, at, facing)| (at, facing));
    let ai = bit(LanRole::Gunner) | bit(LanRole::Driver) | bit(LanRole::Loader) | bit(LanRole::Radio);
    check("AI men hold every seat but the host's", g.lan_ai_crew_mask() == ai,
        json!({"mask": g.lan_ai_crew_mask(), "expected": ai}));

    let fired_before = g.net_snapshot().rounds_spent;
    let fired_at = run(&mut g, &mut r, &alone, 30.0, &|g| g.net_snapshot().rounds_spent > fired_before);
    let fired = g.net_snapshot().rounds_spent > fired_before;
    check("AI gunner lays and fires on the contact", fired, json!({"seconds": fired_at, "rounds": g.net_snapshot().rounds_spent}));
    let reloaded_in = run(&mut g, &mut r, &alone, 20.0, &|g| g.breech_loaded());
    check("AI loader reloads at a man's rate", g.breech_loaded() && reloaded_in > 2.0,
        json!({"seconds": reloaded_in}));

    // Hold fire while the driver is tested, so the drive check is not a recoil artefact.
    let start = g.player.pos;
    run(&mut g, &mut r, &[commander(1.0)], 5.0, &|_| false);
    let moved = g.player.pos.distance(start);
    check("AI driver follows the commander's WASD", moved > 1.0 && g.player.speed.abs() > 0.3,
        json!({"moved_m": moved, "speed_m_s": g.player.speed}));
    run(&mut g, &mut r, &alone, 4.0, &|_| false);

    let gunner = (LanRole::Gunner, RoleInput { post: LanRole::Gunner, ..RoleInput::default() });
    let crewed = [commander(0.0), gunner];
    run(&mut g, &mut r, &crewed, 0.2, &|_| false);
    check("a human gunner takes the seat: the AI gunner stands down",
        g.lan_ai_crew_mask() & bit(LanRole::Gunner) == 0,
        json!({"mask": g.lan_ai_crew_mask()}));
    run(&mut g, &mut r, &crewed, 20.0, &|g| g.breech_loaded());
    let target = rearm_target(&mut g, &mut r, spot, facing);
    let held_from = g.net_snapshot().rounds_spent;
    run(&mut g, &mut r, &crewed, 10.0, &|_| false);
    check("no shot while the human gunner holds the seat without firing",
        target.is_some() && g.net_snapshot().rounds_spent == held_from,
        json!({"live_target": target, "rounds_before": held_from, "rounds_after": g.net_snapshot().rounds_spent}));

    run(&mut g, &mut r, &alone, 0.2, &|_| false);
    check("the human leaves: the AI gunner takes his seat back",
        g.lan_ai_crew_mask() & bit(LanRole::Gunner) != 0,
        json!({"mask": g.lan_ai_crew_mask()}));
    let target = rearm_target(&mut g, &mut r, spot, facing);
    let back_before = g.net_snapshot().rounds_spent;
    let back_at = run(&mut g, &mut r, &alone, 30.0, &|g| g.net_snapshot().rounds_spent > back_before);
    check("the returned AI gunner fires again", g.net_snapshot().rounds_spent > back_before,
        json!({"seconds": back_at, "live_target": target}));

    let receipt = json!({"schema": "panzer.lan-crew-proof/v1", "adapter": info.name,
        "backend": format!("{:?}", info.backend), "pass": pass, "checks": checks,
        "scope": "LAN host path with the crew frame a LanSession host builds; one parked range target (targets_only); simulation steps only, nothing rendered; no network sockets (lan-probe covers the transport)."});
    let path = format!("{prefix}_receipt.json");
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&receipt)?)?;
    println!("[lan-crew] receipt {path}");
    anyhow::ensure!(pass, "lan-crew proof failed");
    println!("[lan-crew] PASS");
    Ok(())
}
