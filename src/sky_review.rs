//! `sky-review <prefix>`: the sky as the crew sees it through the day and the night, measured.
//! Exterior stills from a camera over the player's Tiger in act 1 (Stalingrad, the solstice of
//! 1942) at named Sun altitudes through dawn and dusk, at noon, on the Christmas Eve Moon and on
//! a moonless January night, plus the stars overhead; interior stills from the loader's seat with
//! the cabin lamps on at noon and at night (the interior must not change with the sky). Every
//! still carries the scene photometry, the eye's state and the displayed picture's statistics
//! (mean luminance, blue over red, the share of black pixels), in `<prefix>_sky.json`.
use crate::{
    celestial_sky::{ClockPhase, SkyClock},
    ephemeris::{self, Site, UtcInstant, STALINGRAD},
    game::{Game, Station},
    renderer::Renderer,
    tank_editor_scene::{TankAssetId, TankEditorScene},
    world, TankInput,
};
use anyhow::{ensure, Context};
use glam::Vec3;
use serde_json::json;
use std::path::Path;

/// The Sun altitude's instant on a date, searched over the rising or the setting half-day.
fn instant_at_sun_altitude(midnight: UtcInstant, site: Site, altitude_deg: f64, rising: bool) -> UtcInstant {
    let altitude = |minutes: f64| {
        let instant = midnight.plus_seconds(minutes * 60.0);
        let sun = ephemeris::sun_position(instant.jde());
        ephemeris::horizontal_position(&sun, instant, site).altitude_deg
    };
    let noon = (0..24 * 60).map(|m| m as f64)
        .max_by(|a, b| altitude(*a).total_cmp(&altitude(*b))).unwrap_or(720.0);
    // Bisection between the dark end and noon (rising) or noon and the dark end (setting): the
    // bracket keeps `lo` on the dark side and `hi` on the bright side.
    let (mut dark, mut bright) = if rising { (noon - 12.0 * 60.0, noon) } else { (noon + 12.0 * 60.0, noon) };
    for _ in 0..48 {
        let mid = 0.5 * (dark + bright);
        if altitude(mid) > altitude_deg { bright = mid } else { dark = mid }
    }
    let (lo, hi) = (dark, bright);
    midnight.plus_seconds(0.5 * (lo + hi) * 60.0)
}

/// What the displayed picture is: mean linear luminance, blue over red, the share of pixels
/// that are black on the display, and the mean of the top third (the sky).
fn display_stats(image: &image::RgbaImage) -> serde_json::Value {
    let decode = |c: u8| {
        let v = c as f64 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let (w, h) = image.dimensions();
    let (mut r, mut g, mut b, mut black, mut top) = (0.0f64, 0.0f64, 0.0f64, 0u64, 0.0f64);
    for (_, y, p) in image.enumerate_pixels() {
        let (lr, lg, lb) = (decode(p[0]), decode(p[1]), decode(p[2]));
        r += lr; g += lg; b += lb;
        if p[0].max(p[1]).max(p[2]) < 6 { black += 1; }
        if y < h / 3 { top += 0.2126 * lr + 0.7152 * lg + 0.0722 * lb; }
    }
    let n = (w as f64 * h as f64).max(1.0);
    json!({
        "mean_linear": (0.2126 * r + 0.7152 * g + 0.0722 * b) / n,
        "mean_rgb_linear": [r / n, g / n, b / n],
        "blue_over_red": if r > 0.0 { b / r } else { 0.0 },
        "black_share": black as f64 / n,
        "top_third_mean_linear": top / (w as f64 * (h / 3) as f64).max(1.0),
    })
}

struct Still<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: &'a str,
    receipts: Vec<serde_json::Value>,
}

impl Still<'_> {
    fn take(&mut self, r: &mut Renderer, name: &str, note: &str) -> anyhow::Result<()> {
        // KESSEL_SKY_REVIEW_STILLS=a,b,c takes only the named stills.
        if let Ok(list) = std::env::var("KESSEL_SKY_REVIEW_STILLS") {
            if !list.split(',').any(|n| n.trim() == name) { return Ok(()); }
        }
        r.reset_history();
        let warmup = crate::converge_headless_screenshot(r, self.device, self.queue, 24)?;
        ensure!(warmup.stable, "{name}: the eye did not converge");
        let path = format!("{}_sky_{name}.png", self.prefix);
        r.render_to_png(self.device, self.queue, &path, r.iw * 3, r.ih * 3)?;
        let photometry = r.debug_frame_photometry(self.device, self.queue)?;
        // The renderer routes the still (a 3840-wide JPEG under KESSEL_CAPTURE_4K_JPG=1).
        let written = {
            let routed = crate::capture_target(&path);
            if Path::new(&routed).exists() { routed } else { routed.replace(".png", ".jpg") }
        };
        let shown = image::open(&written).with_context(|| format!("the still {written} was not written"))?.to_rgba8();
        let state = r.celestial().state;
        let receipt = json!({
            "still": name, "note": note, "file": written,
            "instant": state.instant.to_iso(),
            "sun_altitude_deg": state.sun.altitude_deg, "moon_altitude_deg": state.moon.altitude_deg,
            "moon_illuminated_fraction": state.moon_phase.illuminated_fraction, "moon_waxing": state.moon_phase.waxing,
            "key_body": format!("{:?}", r.celestial().key.body),
            "overcast": r.overcast, "fog_density": r.fog_density,
            "camera": {"pos": r.camera.pos.to_array(), "yaw_deg": r.camera.yaw.to_degrees(), "pitch_deg": r.camera.pitch.to_degrees(), "fov_y_deg": r.camera.fov_y.to_degrees()},
            "photometry": photometry.to_json(),
            "display": display_stats(&shown),
            "exposure_frames": warmup.exposure_frames,
        });
        println!("[sky-review] {name}: sun {:+.1} moon {:+.1} ({:.0} % lit) display mean {:.4} black {:.0} % blue/red {:.2}",
            state.sun.altitude_deg, state.moon.altitude_deg, state.moon_phase.illuminated_fraction * 100.0,
            receipt["display"]["mean_linear"].as_f64().unwrap_or(0.0),
            receipt["display"]["black_share"].as_f64().unwrap_or(0.0) * 100.0,
            receipt["display"]["blue_over_red"].as_f64().unwrap_or(0.0));
        self.receipts.push(receipt);
        Ok(())
    }
}

/// Clear winter air over the steppe, as `screenshot`'s `KESSEL_SHOT_WEATHER=clear` sets it.
fn clear_weather(r: &mut Renderer) {
    r.set_weather_override(None);
    r.overcast = 0.08;
    r.storm = 0.0;
    r.fog_density = 0.00035;
    r.precipitation = 0.0;
}

/// The exterior camera: over the Tiger, looking toward an azimuth with a pitch.
fn aim_exterior(r: &mut Renderer, g: &mut Game, toward: glam::DVec3, pitch_deg: f32) {
    let eye = g.player.pos + Vec3::new(0.0, 3.2, 0.0);
    let flat = Vec3::new(toward.x as f32, 0.0, toward.z as f32).normalize_or_zero();
    let flat = if flat.length_squared() < 0.5 { Vec3::Z } else { flat };
    r.camera.pos = eye;
    r.camera.set_forward(flat);
    r.camera.pitch = pitch_deg.to_radians();
    r.camera.fov_y = 55f32.to_radians();
    r.reticle = false;
    r.interior_light = 0.0;
    r.interior_haze = 0.0;
    r.rt_mix = 1.0;
    r.weather_shelter = g.building_weather_shelter_at(r.camera.pos);
    r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    g.prepare_world_survey(r, 1.0 / 60.0);
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    let prefix = crate::capture_target(prefix);
    ensure!(!Path::new(&format!("{prefix}_sky.json")).exists(), "sky review prefix already exists; choose a fresh prefix");
    if let Some(parent) = Path::new(&prefix).parent() { std::fs::create_dir_all(parent)?; }
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    std::env::set_var("KESSEL_DAY_NIGHT", "0");
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let site = STALINGRAD;
    let solstice = UtcInstant::from_calendar(1942, 12, 21, 0, 0, 0.0);
    let dawn = |alt: f64| instant_at_sun_altitude(solstice, site, alt, true);
    let dusk = |alt: f64| instant_at_sun_altitude(solstice, site, alt, false);
    let moon_night = UtcInstant::from_calendar(1942, 12, 24, 23, 0, 0.0);
    let moonless_night = UtcInstant::from_calendar(1943, 1, 6, 20, 0, 0.0);
    let mut cap = Still { device: &device, queue: &queue, prefix: &prefix, receipts: Vec::new() };

    // The exterior: act 1 with the player's Tiger, the camera over it.
    {
        let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
        g.mute_audio();
        g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
        clear_weather(&mut r);
        let series: [(&str, UtcInstant, f32, &str); 11] = [
            ("dawn_m8", dawn(-8.0), 6.0, "the Sun 8 deg under the horizon, nautical twilight"),
            ("dawn_m4", dawn(-4.0), 6.0, "civil twilight"),
            ("sunrise", dawn(0.0), 6.0, "the Sun on the horizon"),
            ("dawn_p3", dawn(3.0), 6.0, "the Dawn phase: the Sun 3 deg up"),
            ("noon", crate::celestial_sky::phase_instant(1, ClockPhase::Day), 6.0, "local noon, 17.9 deg"),
            ("dusk_p3", dusk(3.0), 6.0, "the Dusk phase"),
            ("sunset", dusk(0.0), 6.0, "the Sun on the horizon"),
            ("dusk_m4", dusk(-4.0), 6.0, "civil twilight"),
            ("dusk_m8", dusk(-8.0), 6.0, "nautical twilight"),
            ("night_moon", moon_night, 6.0, "Christmas Eve 1942, 23:00 UT: a 96 % Moon 59 deg up"),
            ("night_moonless", moonless_night, 6.0, "6 January 1943, 20:00 UT: no Moon"),
        ];
        for (name, instant, pitch, note) in series {
            r.set_sky_clock(SkyClock::new(instant, site));
            let toward = r.celestial().key.direction;
            aim_exterior(&mut r, &mut g, toward, pitch);
            cap.take(&mut r, name, note)?;
        }
        // The stars overhead on both nights, and the moonlit night under cloud.
        for (name, instant, note) in [("stars_moonless", moonless_night, "the moonless sky 50 deg up"), ("stars_moon", moon_night, "the moonlit sky 50 deg up, away from the Moon")] {
            r.set_sky_clock(SkyClock::new(instant, site));
            let toward = -r.celestial().state.moon.engine_direction();
            aim_exterior(&mut r, &mut g, toward, 50.0);
            cap.take(&mut r, name, note)?;
        }
        // The day under six tenths cloud: the clouds against the hazed sky.
        for (name, instant, note) in [("noon_clouds", crate::celestial_sky::phase_instant(1, ClockPhase::Day), "noon under six tenths cloud"),
            ("sunset_clouds", dusk(0.0), "sunset under six tenths cloud")] {
            r.set_sky_clock(SkyClock::new(instant, site));
            r.overcast = 0.6;
            let toward = r.celestial().key.direction;
            aim_exterior(&mut r, &mut g, toward, 10.0);
            cap.take(&mut r, name, note)?;
        }
        r.set_sky_clock(SkyClock::new(moon_night, site));
        r.overcast = 0.6;
        let toward = r.celestial().key.direction;
        aim_exterior(&mut r, &mut g, toward, 6.0);
        cap.take(&mut r, "night_moon_clouds", "the moonlit night under six tenths cloud")?;
        r.overcast = 0.99;
        cap.take(&mut r, "night_moon_overcast", "the moonlit night under overcast: the cauldron's glow")?;
        clear_weather(&mut r);
    }
    // The interior in the world (not the editor's hall): the loader's seat, the gunner's sight
    // and the buttoned-up commander at noon and at night, the lamps on, and the loader with
    // them off, so daylight through the ports and the sight shows against the lamps.
    {
        let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
        g.mute_audio();
        g.set_defer_rtt_to_render(true);
        g.set_cabin_lights(true);
        for _ in 0..3 { g.update(&TankInput::default(), 1.0 / 60.0, &mut r); }
        let noon = crate::celestial_sky::phase_instant(1, ClockPhase::Day);
        for (name, station, instant, lamps, note) in [
            ("interior_noon", Station::Loader, noon, true, "the loader's seat in the world, lamps on, noon"),
            ("interior_night", Station::Loader, moon_night, true, "the loader's seat in the world, lamps on, the moonlit night"),
            ("interior_noon_dark", Station::Loader, noon, false, "the loader's seat, lamps off, noon: the ports' daylight alone"),
            ("gunner_noon", Station::Gunner, noon, true, "the gunner's sight at noon"),
            ("gunner_night", Station::Gunner, moon_night, true, "the gunner's sight on the moonlit night"),
            ("commander_noon", Station::Commander, noon, true, "the commander buttoned up at noon"),
            ("commander_night", Station::Commander, moon_night, true, "the commander buttoned up on the moonlit night"),
        ] {
            g.set_cabin_lights(lamps);
            // The cupola is reached from the walkable compartment (Game::jump_station).
            if station == Station::Commander { g.jump_station(Station::Loader); }
            g.jump_station(station);
            r.set_sky_clock(SkyClock::new(instant, site));
            for _ in 0..3 { g.update(&TankInput::default(), 1.0 / 60.0, &mut r); }
            cap.take(&mut r, name, note)?;
        }
    }
    let receipt = json!({
        "schema": "kessel.sky-review.v1",
        "site": {"latitude_deg": site.latitude_deg, "longitude_deg_east": site.longitude_deg_east},
        "stills": cap.receipts,
        "scope": "Headless stills through the shipping atmosphere, ephemeris and eye, converged per still (the eye fully adapted to each field; live play adapts over time). Exterior over the act-1 Tiger in clear winter air unless noted; interior from the source Tiger loader station with the cabin lamps on.",
    });
    std::fs::write(format!("{prefix}_sky.json"), serde_json::to_string_pretty(&receipt)?).context("sky review receipt")?;
    println!("[sky-review] PASS — {} stills under {prefix}_sky_*", cap.receipts.len());
    Ok(())
}
