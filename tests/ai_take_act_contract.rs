//! Shipping-source contract for the mixed interior/exterior `ai-take` recorder.
//!
//! A film must show the same act the player receives. In particular, the camera director may move
//! the lens and mark it sheltered inside the armor, but it may not replace the act's atmosphere,
//! sun, precipitation, eye adaptation, retro response, or manifest-resolved spawn pose.

const SHOWREEL: &str = include_str!("../src/showreel.rs");

fn body_after(signature: &str, next_signature: &str) -> &'static str {
    SHOWREEL
        .split(signature)
        .nth(1)
        .and_then(|tail| tail.split(next_signature).next())
        .unwrap_or_else(|| panic!("could not isolate {signature}"))
}

#[test]
fn ai_take_uses_manifest_spawn_pose_and_preserves_shipping_atmosphere() {
    let center = body_after("fn take_staging_center(", "fn stage_take");
    assert!(center.contains("game.act.spawn"));
    assert!(center.contains("game.act.id == 0"));

    let stage = body_after("fn stage_take(", "fn stage_campaign_wave");
    assert!(stage.contains("take_staging_center(game)"));
    assert!(stage.contains("stage_showreel_t34_gauntlet_at"));
    assert!(stage.contains("game.act.goal - game.act.spawn"));
    assert!(stage.contains("game.player.heading"));

    let director = body_after("impl CameraDirector", "enum CampaignShot");
    assert!(director.contains("renderer.weather_shelter = 1.0"));
    assert!(director.contains("renderer.weather_shelter = 0.0"));
    for forbidden in [
        "renderer.fog_density =",
        "renderer.precipitation =",
        "renderer.sun_dir =",
        "renderer.autoexp =",
        "renderer.exposure_override =",
        "renderer.master_retro =",
    ] {
        assert!(
            !director.contains(forbidden),
            "ai-take camera must preserve shipping renderer state: {forbidden}"
        );
    }

    let recorder = body_after("fn record_take(", "fn record_campaign_wave_take");
    for forbidden in [
        "renderer.fog_density =",
        "renderer.precipitation =",
        "renderer.sun_dir =",
        "renderer.autoexp =",
        "renderer.exposure_override =",
        "renderer.master_retro =",
    ] {
        assert!(
            !recorder.contains(forbidden),
            "ai-take setup must preserve shipping renderer state: {forbidden}"
        );
    }
    assert!(recorder.contains("TakeAtmosphere::capture(renderer)"));
    assert!(recorder.contains("atmosphere.matches(renderer)"));
    assert!(recorder.contains("resolved manifest Spawn marker"));
    assert!(recorder.contains("shipping day phase"));
}
