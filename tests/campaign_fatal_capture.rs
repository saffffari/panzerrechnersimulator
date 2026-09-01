#[path = "../src/campaign_capture.rs"]
mod campaign_capture;

use campaign_capture::FatalCaptureLatch;

const OUTPUT_DT: f32 = 1.0 / 24.0;
const REQUIRED_HOLD: f32 = 2.0;

#[test]
fn death_between_output_samples_latches_until_two_seconds_are_encoded() {
    let mut latch = FatalCaptureLatch::default();

    assert!(!latch.observe_authoritative_state(10.0, true, true));
    assert!(latch.observe_authoritative_state(10.0 + 1.0 / 60.0, true, false));
    assert_eq!(latch.death_at(), Some(10.0 + 1.0 / 60.0));
    assert!(latch.requires_full_turret_camera(REQUIRED_HOLD));

    // The death occurred between output samples. Only frames actually sent to the encoder count.
    for frame in 0..47 {
        latch.note_rendered_camera_frame(10.0 + (frame + 1) as f32 * OUTPUT_DT, true, OUTPUT_DT);
    }
    assert!(latch.requires_full_turret_camera(REQUIRED_HOLD));
    assert!(!latch.has_required_hold(REQUIRED_HOLD, OUTPUT_DT * 0.5));

    latch.note_rendered_camera_frame(12.0 + OUTPUT_DT, true, OUTPUT_DT);
    assert!(!latch.requires_full_turret_camera(REQUIRED_HOLD));
    assert!(latch.has_required_hold(REQUIRED_HOLD, OUTPUT_DT * 0.5));
    assert!(latch.first_interior_frame_at().is_some());
    assert!((latch.maximum_interior_hold() - REQUIRED_HOLD).abs() < 1.0e-4);
}

#[test]
fn exterior_frame_breaks_continuity_and_persistent_death_does_not_retrigger() {
    let mut latch = FatalCaptureLatch::default();
    assert!(latch.observe_authoritative_state(3.0, true, false));

    for frame in 0..30 {
        latch.note_rendered_camera_frame(3.0 + frame as f32 * OUTPUT_DT, true, OUTPUT_DT);
    }
    latch.note_rendered_camera_frame(4.3, false, OUTPUT_DT);
    for frame in 0..30 {
        latch.note_rendered_camera_frame(4.4 + frame as f32 * OUTPUT_DT, true, OUTPUT_DT);
    }
    assert!(latch.requires_full_turret_camera(REQUIRED_HOLD));
    assert!(!latch.has_required_hold(REQUIRED_HOLD, OUTPUT_DT * 0.5));

    assert!(!latch.observe_authoritative_state(6.0, false, false));
    assert_eq!(latch.death_at(), Some(3.0));
}

#[test]
fn campaign_take_connects_authoritative_edge_before_camera_and_audits_encoded_frames() {
    let source = std::fs::read_to_string("src/showreel.rs").expect("read showreel source");
    let edge = source
        .find("audit.note_authoritative_tiger_state(now, previous_tiger_alive, game.player.alive)")
        .expect("authoritative death observer");
    let camera = source[edge..]
        .find("director.note_fatal_transition(tiger_destroyed_this_tick)")
        .map(|offset| edge + offset)
        .expect("fatal camera latch");
    let render = source[camera..]
        .find("renderer.render_to_rgba")
        .map(|offset| camera + offset)
        .expect("render sample");
    let encoded_audit = source[render..]
        .find("audit.note_rendered_camera_frame")
        .map(|offset| render + offset)
        .expect("encoded frame audit");

    assert!(edge < camera && camera < render && render < encoded_audit);
    assert!(source.contains("CampaignShot::TurretWide"));
}
