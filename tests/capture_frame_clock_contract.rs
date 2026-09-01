//! Source boundary for the headless/video capture stochastic clock. Behavioral render tests need a
//! GPU, so this independently runnable contract prevents the capture path from silently freezing
//! the pixel/frame seeds again.

const RENDERER: &str = include_str!("../src/renderer.rs");

fn function_body<'a>(source: &'a str, signature: &str, next_signature: &str) -> &'a str {
    let start = source.find(signature).expect("function signature");
    let end = source[start..]
        .find(next_signature)
        .map(|offset| start + offset)
        .unwrap_or(source.len());
    &source[start..end]
}

#[test]
fn rgba_capture_advances_the_stochastic_frame_before_uniform_upload() {
    let capture = function_body(
        RENDERER,
        "    fn capture_rgba(",
        "    /// Compatibility no-op for the removed pasted-camera atlas diagnostic.",
    );
    let advance = capture
        .find("self.frame = self.frame.wrapping_add(1);")
        .expect("capture frame advance");
    let upload = capture
        .find("self.upload_uniforms(queue);")
        .expect("capture uniform upload");

    assert!(
        advance < upload,
        "frame seed must roll before uniforms are uploaded"
    );
    assert_eq!(
        capture
            .matches("self.frame = self.frame.wrapping_add(1);")
            .count(),
        1,
        "one encoded image must advance the stochastic clock exactly once"
    );
}

#[test]
fn capture_and_live_paths_share_the_same_frame_seed_rule() {
    for (signature, next) in [
        (
            "    pub fn render_to_surface(\n",
            "    /// GUNNER EYEPIECE COMPOSITE",
        ),
        (
            "    pub fn render_offscreen(",
            "    /// Read the active view",
        ),
        (
            "    fn capture_rgba(",
            "    /// Compatibility no-op for the removed pasted-camera atlas diagnostic.",
        ),
    ] {
        let body = function_body(RENDERER, signature, next);
        let advance = body
            .find("self.frame = self.frame.wrapping_add(1);")
            .unwrap_or_else(|| panic!("missing frame advance in {signature}"));
        let upload = body
            .find("self.upload_uniforms(queue);")
            .unwrap_or_else(|| panic!("missing uniform upload in {signature}"));
        assert!(
            advance < upload,
            "{signature} uploads a stale stochastic seed"
        );
    }
}
