//! Source-level boundary for deterministic screenshot metering. The behavioral tracker also has
//! unit coverage beside its implementation in `main.rs`; this contract stays independently
//! runnable when unrelated game-only unit fixtures are temporarily under construction.

const MAIN: &str = include_str!("../src/main.rs");
const RENDERER: &str = include_str!("../src/renderer.rs");

#[test]
fn screenshot_prerolls_transport_before_rearming_the_measured_eye() {
    let start = MAIN
        .find("fn converge_headless_screenshot(")
        .expect("headless convergence driver");
    let end = MAIN[start..]
        .find("\n#[cfg(test)]")
        .map(|offset| start + offset)
        .expect("end of headless convergence driver");
    let driver = &MAIN[start..end];

    let pause = driver
        .find("renderer.autoexp = false;")
        .expect("transport-only meter pause");
    let transport = driver[pause..]
        .find("renderer.render_offscreen(device, queue);")
        .map(|offset| pause + offset)
        .expect("transport pre-roll");
    let reset = driver
        .find("renderer.reset_active_exposure_for_headless_capture(queue);")
        .expect("fresh exposure sentinel");
    let resume = driver
        .find("renderer.autoexp = true;")
        .expect("meter resume");
    let measured = driver
        .find("renderer.debug_read_active_exposure(device, queue)?")
        .expect("measured GPU convergence");

    assert!(pause < transport && transport < reset && reset < resume && resume < measured);
    assert!(driver.contains("1..=HEADLESS_EXPOSURE_MAX_FRAMES"));
    assert!(driver.contains("convergence.observe(exposure)"));
    assert!(!driver.contains("exposure_override = Some"));
    for forbidden in ["campaign_exposure", "winter_exposure", "whiteout_exposure"] {
        assert!(!driver.contains(forbidden));
    }
}

#[test]
fn reset_hook_only_rearms_view_local_camera_state() {
    let start = RENDERER
        .find("pub(crate) fn reset_active_exposure_for_headless_capture")
        .expect("headless exposure reset hook");
    let end = RENDERER[start..]
        .find("\n    /// Read back the raster depth")
        .map(|offset| start + offset)
        .expect("end of reset hook");
    let hook = &RENDERER[start..end];

    assert!(hook.contains("&self.exp_bufs[self.active_view]"));
    assert!(hook.contains("uninitialized_exposure_state()"));
    for forbidden in [
        "fog_density",
        "overcast",
        "physical_environment_buf",
        "crush_buf",
        "exposure_override",
    ] {
        assert!(!hook.contains(forbidden), "reset hook mutated {forbidden}");
    }
}
