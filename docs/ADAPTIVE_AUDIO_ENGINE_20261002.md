# Adaptive audio engine and owned library

The runtime now consumes the user's `panzer_sounds` folders through a content-addressed
catalog and imported playback files. This checkout is isolated from the closed
`playtest-hotfix-20261002` game. Originals are read-only; imports do not bake in gains or effects.

## Runtime behavior

- One unique night ambience is chosen on the first nighttime update for each local-noon-to-noon
  night ID. It stays selected through midnight, movement, combat and hatch changes. Seeded shuffle
  bags avoid consecutive repeats when at least two eligible files exist. Source-night
  recordings with silent tails use overlap seams without editing originals.
- An independent cabin bed remains selected across exploration, driving, threat, combat and
  aftermath. Entry and exit change its level. Exterior beds become quieter and low-pass filtered
  through the listener's existing tank/building shelter value. Blizzard suppresses the quiet night
  bed while the existing physical weather engine continues.
- Speed, nearby live hostile tanks/AT guns/infantry and hostile contact drive the director.
  Proximity alone never unlocks the high-intensity combat pool. Contact retains twelve seconds of
  quiet grace, then passes through fifteen seconds of aftermath. Driving/threat use hysteresis.
  The score is an authored dramatic response, not a physical sound emitted by enemies.
  An independent programme varies delays, overlapping calm/suspense choices, cue dwell, gain and
  silence, including uneasy textures without actual danger. It never invents physical gunfire.
- There is one primary score lane; explicit driving/aftermath pools can be added, with calm
  material as fallback. Changes from BPM-labelled cues wait for a source-sample bar boundary.
  Rhythmic loops retain the exact imported period instead of shortening every loop with an
  ambience-style overlap. Filename BPM/key remain inferred metadata, not verified music analysis.
- Folder subpaths become context requirements. The most specific eligible set wins; general
  category files are fallback. Live tags include winter/spring, day/night, inside/exterior,
  clear/blizzard, current mode and authored location. Unknown subfolder tags require an explicit
  game/scene condition before they can play. Night identity deliberately outranks mid-night
  context reselection; other lanes can reselect when the eligible pool changes.
- Scene recordings are not global battlefield ambience. Author explicit location zones in
  `assets/audio/library/soundscape.json`: center, radius, approach fade, priority and room rack.
  The actual listener position controls audibility; debouncing does not keep a departed room loud.
  The six current scene files remain catalogued and disabled until locations are authored.

## Effects racks and event library

`catalog/soundscape.example.json` is an inactive authoring example. A per-cue rack can select
dry/open_air/small_room/large_room/tank_cabin and override wet mix, decay, predelay, damping,
low-pass and high-pass. A scene zone supplies a default rack unless its cue overrides it.
Optional `tail_only_s` preserves a dry attack/body and blends reverb at the finite sample ending.
Its tail is rendered to completion even after the source has stopped. The tank enclosure also changes exterior-bed tone as the listener moves. These are bounded
algorithmic effects, not acoustic ray tracing. Defaults remain dry to avoid doubling room sound
already present in a field recording. Existing gun/impact propagation is unchanged by these racks.

The event registry binds authored names to unique imported variants with cooldowns. Events have
distinct voice identities and retain a chosen variant while waiting for decode. Automatic factual
mode-entry stingers are deliberately absent. `_loop` is explicit repeat intent, including on
effect cues; unlabeled atmospheres use category policy. Death cues always play once. This is for authored stingers and local scene effects. Positional gunfire,
ballistics, impact timing and other physical emitters continue through the existing propagation
engine; a stereo field recording is not separated into individually located people by this system.

All new voices enter the existing floating-point Audio mixer and common final limiter. Physical
beds follow the listener's hearing stage. Nondiegetic music has its own bus, still obeying master
volume, listener fade, pause and sample-exact KIA gating. The explicit death cue is the sole
exception: it enters the same final limiter through a dedicated terminal mixer, continuing over
black until its natural end. At its audio midpoint, a bold white DU BIST GEFALLEN card displays
a deterministic fictional German name and plausible birth/death years. Ordinary battlefield
audio is removed sample-exactly, including buffered lookahead. Pausing freezes cue phase. Independent
master smoothing affects events and outgoing fades as well as steady loops.

## Assets, seeds, performance and saves

Run `python3 tools/catalog_panzer_sounds.py --source /home/alex/panzer/panzer_sounds
--output assets/audio/library/manifest.json --import-enabled` after organizing files. Cataloguing
deduplicates by content and preserves aliases; moving a source does not change its identity.
The runtime reads the resulting catalog at startup. It does not watch or rewrite the source tree.

Playback imports are stereo 44.1 kHz PCM16. A single bounded worker decodes requested cues;
there is no disk access or decoding on the device callback. At most four load requests, two
decode results and twelve live/fading voices are admitted. Cache residency is limited to256 MiB,
each decoded cue to64 MiB. Offline reviews decode only selected cues on the caller thread; they
do not preload the whole library. Fatal cues are warmed ahead of use, with a bounded synchronous
caller-thread fallback if the selected cue was evicted. PCM held by active/fading voices and in-flight decodes is additional;
256 MiB is not a total process-memory claim. Long recordings should be imported as authored
loop excerpts; arbitrary-hour streaming is not implemented. Source WAV originals are retained.

`PANZER_SOUND_SEED=<integer>` supports reproducible standalone audio fixtures. Fresh standalone
runs otherwise choose fresh seeds; Canyon and campaign use their existing saved run/encounter
seeds. Campaign saves restart a sector entrance, so audio restarts with the same deterministic
choices. Director snapshots round-trip exact selection/timer state; runtime snapshots are debug
receipts, not save/resume of every decoder/reverb buffer mid-sample. Canyon has no checkpoints.

## Review and validation

`cargo test --release --bin panzerrechnersystem soundscape` checks state transitions, selection,
snapshot continuity, racks and transport. `soundscape-review captures/adaptive_soundscape_20261002`
exports a scripted context timeline through ordinary Audio sinks/master, night loop
auditions extending beyond one complete source cycle, a staged fatal-explosion/death-chord sequence, and a JSON receipt. It is a mixer
fixture rather than a gameplay recording. Source parsing/selection tests and the importer suite complement this export.

Weapon timbre changes and their actual per-round MG comparison are documented separately in
`docs/WEAPON_TIMBRE_CANDIDATES_20261002.md`. Numerical checks do not establish that the new sounds
match the reference or are pleasing. Listening and integrated native play remain the acceptance gate.

## Added cue and gameplay rules

The current imported snapshot includes 73 playable cues. Sunrise and dusk have actual source files;
sunset has a registered event but its folder was empty in the last targeted import. Sunrise/sunset
follow directional solar-horizon crossings, dusk follows descending -6 degrees. Loading, pausing,
horizon jitter and large debug clock jumps do not replay these events. Opening atmosphere plays one
complete finite piece per run, interrupted only by incoming combat or the player's main-gun cut.

A main-gun discharge cuts only the foreground score. A distinct eligible successor is warmed ahead
of time; a 35 ms crossfade avoids clicks and bypasses bar waiting. Environmental tracks keep their
cursors. If an unusually early first-use cue is not decoded yet, the outgoing score still cuts on
the shot and the replacement enters when the bounded worker completes it. The high-intensity pool
requires an actual hostile hit or unobstructed close passage. Own fire selects suspense until such
contact occurs. Suspense rises over 8–18 seconds and releases gradually, including after combat.

Heavy-blizzard audio is a separate physical bed driven by local outdoor optical whiteout, not a
wind-slider threshold. See `docs/LOCAL_WEATHER_VISIBILITY_20261002.md` for the shared renderer/AI
medium and its limits. The engine-hiss effect follows the actual failed Engine module and the
articulated rear engine position. It attenuates/pans as the listener moves and fades after repair.
The current 27-second cooling recording plays once per failure; an explicit loop override can
sustain another authored hiss. This is a damage-state effect, not a simulated coolant-pressure law.

Night, day, cabin, scene, heavy weather, engine effect, foreground score and finite accent tracks
are independent. They share the master limiter, but a foreground edit never restarts the other beds.

## Completed validation for this build

The final release compiled. 90 distinct targeted Rust tests and 22 metadata tests passed. The
calibration fixture passed 24 events and 55 level relations. The actual shared mixer exported
nine complete stereo WAVs: a 142-second layered context run, five night seams, sunrise, dusk and
the continuing death chord. All pause/KIA/terminal checks passed; the final card was rendered
windowlessly on the host GPU. The launcher verified all 73 library imports and 155 required files.

Open `http://127.0.0.1:8873/adaptive_soundscape_final_20261002_adaptive_review.html` while the
local review server is running. This page links the nine separate main-gun/MG comparisons and
the actual death-card PNG. The game remains closed. Technical validation does not establish
subjective sound quality, full gameplay performance or native AI/player visibility acceptance.

Release receipt: `artifacts/runtime/adaptive_audio_release_20261002.json`. New files added to
source folders after their inventory still require catalog/import; there is no filesystem watcher.


## Occasional incoming-shell risers (2026-10-02)

The licensed `suspense/riser_transition.wav` is now a finite score accent, separate from physical gunshot and impact audio. Its measured sample peak is at 5.404943 s. A normal-speed excerpt supplies 1.5–4 seconds of build, followed by the recording's natural tail. This does not time-stretch or pitch-shift the source.

A currently laid enemy tank or engaging AT crew with at most four seconds until readiness can offer a cue only when a read-only ballistic forecast intersects the player's moving tank. Camera frustum, optical/world visibility and an eight-second recent-sighting memory conservatively approximate whether the player has noticed the source. This is a camera heuristic, not proof of player attention. The source gets one 20% selection attempt per firing cycle, with a shared randomized 60–150-second cooldown. Most attacks therefore have no cue. Rare free suspense phrases use the same recording independently of enemies, without inventing firing or impact sounds.

On discharge the selection binds to the actual dispersed shell. Current geometry, wind, and constant-velocity Tiger prediction update the estimate at 5 Hz; turning, acceleration, moving cover and later collisions can invalidate it. These predictions do not change aim, firing, damage or projectile motion. Lost readiness and misses fade the accent over 0.8 s. Actual collision alone confirms a hit and independently triggers the existing physical impact path.

The audio sample clock places the authored peak at the predicted deadline. Forecast corrections use a 10 ms crossfaded source-phase adjustment, without changing playback rate. Early actual contact can finish that correction in 10 ms; a peak already played is never rewound or repeated. At completion its natural tail eases to32% gain over120ms, leaving room for the factual armor strike without changing physical source calibration; free suspense uses the same release. Native frame timing, device buffering and late predictions prevent a universal sample-exact synchronization claim. The score still obeys pause, master level, voice limits and the ordinary death cut; it cannot invoke terminal/death audio.

Review fixtures are staged through the ordinary Audio sinks/master. They demonstrate selected hit, canceled miss, uncued hit and independent suspense. They are not gameplay recordings or subjective listening acceptance.
