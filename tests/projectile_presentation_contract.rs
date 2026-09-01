use std::{fs, path::PathBuf};

fn game_source() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join("src/game.rs")).expect("read cannon presentation source")
}

#[test]
fn retired_authoritative_shells_clear_player_and_external_tracer_draws() {
    let source = game_source();
    assert!(source.contains(
        "fn authoritative_tracer_has_live_owner(projectile_id: Option<u64>, tracer_active: bool)"
    ));
    assert!(source.contains("tracer_active && projectile_id.is_some()"));

    let update = source
        .split("fn update_fx(&mut self, dt: f32, r: &mut Renderer)")
        .nth(1)
        .and_then(|tail| tail.split("fn drive_foam").next())
        .expect("inspect live tracer presentation pass");
    assert!(
        update.contains("r.set_visible(self.slots.tracer, false)"),
        "player tracer must be hidden whenever its shell owner is gone"
    );
    assert!(
        update.contains("r.set_visible(tr, false)"),
        "every external tracer slot must be hidden while unowned"
    );
}
