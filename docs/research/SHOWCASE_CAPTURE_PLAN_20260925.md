# Current-build showcase capture plan — 25 September 2026

Athena, AI collaborator. Read-only engine/tool audit; these commands were inspected, not executed for this plan. No native window input, process restart or capture-adapter installation occurred. There is no `.game-dev/adapter.json` in this worktree, so use the existing game-owned headless recorder directly.

## Film path

`mechanic-review <prefix> <scenario.json>` records the ordinary `Game::update` player camera at 24 fps, simulation at 120 Hz, and ordinary device-free Audio sinks. It writes `<prefix>.mp4`, `.png`, `_soundtrack.wav`, `_video_only.mp4` and `_manifest.json`. Encoded video is 1920×1080; native raster must be 576×324 or 640×360. It is scripted gameplay input, not a native keyboard/mouse recording. `mechanic-validate` is CPU preflight; `mechanic-evaluate` constructs the GPU-backed game and runs simulation/audio checks without media.

Run from `/home/alex/panzer/shader-graphics`, using the newly built binary:

```bash
export KESSEL_RENDERER=shader
export KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator
export KESSEL_INTERNAL_RES=576x324
export KESSEL_GRAPHICS_PRESET=high
export KESSEL_ENABLE_PIZZA_AUDIO=1
export KESSEL_CAPTURE_FRAME_PAUSE_MS=250
export KESSEL_CAPTURE_IDLE_FACTOR=3
./target/release/panzerrechnersystem mechanic-validate catalog/shader_reviews/act1_solo_crew_physical_map.v1.json
./target/release/panzerrechnersystem mechanic-review captures/astra/showcase_20260925_map_v1 catalog/shader_reviews/act1_solo_crew_physical_map.v1.json
```

Use fresh prefixes and run GPU captures sequentially while the user plays. The recorder’s pauses intentionally reduce desktop GPU contention; capture wall time is longer than film time. Do not use the native launcher for these commands. Source inspection is not proof that the release binary includes current changes.

## Proposed 60–90 second edit

| Segment | Existing path / preparation | What can honestly be shown |
|---|---|---|
| Three new historical scenes, 8–12 seconds each | Create one `kessel.first-person-mechanic.v1` JSON per finished preview manifest. Set `act_id:1`, explicit absolute `world_manifest`, `setup.playtest:true`, `enemy_policy:"hold_fire"`, `on_foot:true`, `foot_xz`, `look_at_xz`. Film with timeline `drive`, `steer`, `look_dx`, `look_dy`. | Actual first-person approach, threshold, interior and residue. Staged start and compressed invented layouts disclosed in receipt. No claim of an exact historic site or continuous Tiger route. |
| Physical reload, 10–12 seconds | Start with `catalog/shader_reviews/act1_held_mouse_reload.v1.json`; copy to a fresh fixture and add the missing real flick after `look_tray` while `mg_coax` remains held. | Case extraction/drop, rack pickup, alignment, flick, closed breech, second shot. Existing checks already require two shots and one rack round spent. |
| Exposed cupola P08, 5–7 seconds | New Commander fixture with `hatch_open:true`, action `head_out`, then `aim:true`, fresh `fire2:true` pulses, and `reload:true`. | Actual raised pistol, real ammunition use and recoil. Use `fire2` for the weapon path and disclose that OS LMB dispatch itself is not exercised. |
| Physical map, 6–8 seconds | Existing `act1_solo_crew_physical_map.v1.json` above. | Current sheet, actual pencil/pin input and ordinary campaign survey visibility. The current recorder source now exposes `map_zoom` steps and a zoom metric; verify the final release includes it. |
| Cabin fire and suppression, 6–10 seconds | Use fresh `catalog/shader_reviews/showcase_cabin_amber_fire_20260925.v1.json` for an eight-second growth take with explicit `setup.source_fire`. It makes no suppression or survival claim; validate current-release outcomes. | Staged ignition, ordinary growth/foam/suppression. Do not promise survival using old fixture thresholds. |
| Wreck salvage, 8–12 seconds | New on-foot fixture against the current winter-wreck manifest; locate real spilled round through its authored wreck transform. Hold `mg_coax` for pickup/carry; navigate the ordinary hatch; aim into a genuinely emptied compatible rack slot to stow. | Full route only after a continuous take and inventory receipt succeed. Presently no dedicated salvage film JSON found. |

For all scene routes, obtain coordinates from each builder’s preview manifest and traversal receipt. The recorder accepts no arbitrary elevated `foot_y` setup: start outside at a valid walkable entrance and walk down the real stairs/ramp. This also exposes inaccessible rooms rather than hiding them with a free camera. Accept `foot_displacement_m`/path checks only with visual review of the threshold and camera, since distance alone does not prove the intended route was followed.

The reload gesture currently requires 28 cumulative pixel-equivalent movement at a minimum 280 pixels/second. At the recorder’s 120 Hz, a proposed initial pulse is `look_dy:-4.0` for 0.067 seconds after the round has settled at the tray; this is a tuning starting point, not a passed take. `look_tray` alone rotates the head and does not perform that gesture. Do not use the legacy `reload` still generator as proof of current continuous grip: it uses helper pickups and releases during its setup.

## Existing isolated visual/behavior fixtures

These are GPU regression fixtures, not finished movies. They are useful supporting evidence or explicitly labeled still inserts:

```bash
KESSEL_MAP_REVIEW_PREFIX=captures/astra/showcase_20260925_map_still_v1 cargo test --bin panzerrechnersystem physical_map_overview_and_zoom_preserve_ink_and_depth -- --ignored --nocapture
KESSEL_CUPOLA_PISTOL_REVIEW_PREFIX=captures/astra/showcase_20260925_p08_still_v1 cargo test --bin panzerrechnersystem exposed_p08_uses_shared_ammo_real_flight_and_no_vehicle_trigger -- --ignored --nocapture
KESSEL_FIRE_REVIEW_PREFIX=captures/astra/showcase_20260925_fire_still_v1 cargo test --bin panzerrechnersystem tiger_source_fire_reaches_enclosure_and_actual_foam_sweeps_suppress_it -- --ignored --nocapture
cargo test --bin panzerrechnersystem salvaged_round_uses_actual_hands_and_empty_rack_without_duplication -- --ignored --nocapture
```

Map stills use an isolated Tiger and staged map marks; the zoom API is exercised directly. Fire stills isolate an ammunition-empty Tiger and use distributed foam probes, not one player nozzle. Salvage’s current fixture explicitly advances from exterior pickup to a cabin pose to isolate rack inventory; it does **not** film the intervening hatch traversal. Keep those distinctions in the companion receipt.

## Capture gaps and final checks

- Updated after implementation: `src/mechanic_capture.rs` now accepts bounded `map_zoom` steps. Runtime and visual review still establish whether a particular take shows readable navigation.
- Updated after implementation: primary-click dispatch now checks the exposed Commander sidearm guard. Scripted game input remains distinct from recording actual native OS LMB events.
- Updated after implementation: the recorder source now latches terminal black without replay and calls the hard Audio gate on the terminal 120 Hz tick. Verify final-encode PCM zeros, first black-frame timing and at least two seconds of retained black; source inspection alone does not validate a finished take.
- Inspect `_manifest.json` for all outcome checks and rejected interactions before editing. Watch each clip and audition its captured audio. Keep source filenames, fixture path, build identity, staging disclosure and cut in/out times in the final edit receipt.
- Do not overlay a substitute soundtrack and call it game audio. Use modest cuts between the separate scene/date envelopes. The new scene layouts are interpretations, as detailed in `HISTORICAL_SCENE_SOURCE_AUDIT_20260925.md`.

For a finished MP4, verify duration, codec and audio stream with `ffprobe`; decode representative frames from the actual final encode and inspect them. A successful ffmpeg exit, test assertion or encoder header alone is not visual acceptance.
