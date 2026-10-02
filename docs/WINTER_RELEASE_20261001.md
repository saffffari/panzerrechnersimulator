# Winter playtest release — 2026-10-01

Canonical checkout: `/home/alex/panzer/canyon-v2-20260930`. The launcher is
`tools/launch_winter_playtest.sh`; `--check` verifies the release and required
assets without launching, and `--menu` opens the title/menu instead of a fresh
Canyon V2 run. Initial display is fullscreen at native 800×450 with screen Bayer8
and four geometry coverage samples. Later graphics-menu choices persist in a
separate settings file under `artifacts/runtime/winter-playtest-state/`.

## Consolidated changes

- Opaque meshes and optical exterior geometry resolve four coverage samples in
  linear HDR before the native pixel dither. Normal-map filtering accounts for
  unresolved normal variance. Winter grass is shorter, sparser and partly buried.
  There is no temporal image accumulation or moving dither pattern.
- Fire has eight times the fine sampling density; smoke and fire use the same
  native-pixel grain path and real elapsed simulation time. Grain count changes
  preserve the source contribution instead of multiplying brightness.
- Small arms use weapon-specific swept projectiles, actual contact sparks and
  delayed exterior reports. Rifles use ball ammunition; mounted MGs select a tracer
  every fifth round per emitter. Reports and impacts vary by source seed. Own-hull
  structure-borne armor sounds and the cabin MG mechanism have distinct routes.
- Infantry align their physical rifle direction with a currently visible target
  at a limited turn rate before firing. Squads retain covering shooters while
  bounded flankers move; gun reports propagate to AI listeners. AT crews retain
  their observed gun lay through brief occlusion. Damage, spread, reload times and
  target visibility remain authoritative.
- Track snow attaches only to authored track faces, with pickup, exposed snowfall
  and loose shedding. The hull sits slightly into deformable winter terrain.
  The authored belt itself remains rigid; this does not add circulating track links.
- Capture warmup advances only the observer while the scene stays frozen. The title
  caches its backdrop only after measured eye convergence; live title input remains
  on its ordinary event loop. See `HEADLESS_EYE_WARMUP_20261001.md`.
- Earlier repair cues, independent broken tracks, drivetrain/gears, hatch collision,
  reload-lever placement, ammunition rack containment, terrain seating and prop
  feedback work are included from the active review checkout.

## Rendering boundary

The playable path uses source-geometry shadow maps for point sources and sampled
environment directions. The triangle-traversal experiment remains available with
`KESSEL_SOURCE_VISIBILITY=portable`; it is not the launch default after full-scene
GPU timeouts and unsupported optical diagnostics. See
`PLAYABLE_SHADOW_MAP_BACKEND_20261001.md` for measured evidence and approximations.

Four coverage samples reduce geometry aliasing but do not promise perfectly stable
subpixel branches. Transparent surfaces and media remain single-sampled, with
conservative resolved depth at mixed edges. Finite shadow maps can miss small
occluders, and this renderer does not implement full refracted or indirect reflected
light transport.

## Validation and review

The release review is `captures/winter_release_review_20261001.html`, with explicit
receipts beside each clip. It includes a matched winter geometry-AA comparison,
cupola and interior combat, incoming rifles, coax fire, driving mechanisms, track
snow, and a staged fire/smoke fixture. Recordings use ordinary simulation and the
live mixer with disclosed deterministic inputs. The silent staged fixture and the
eight-second motion comparisons (which end before shell contact) are labeled;
they are not relabeled as combat wins.

Verification checkpoints include 69 passing focused unit checks, 81 passing renderer
integration checks, and 10 later title/eye regression checks. One additional GPU
test in each of the first two groups is ignored by the CPU runner; separate native GPU captures exercise the
shipping path. These are focused checkpoints, not a claim that the complete
repository test suite or every historical feature was retested.

Incoming-rifle capture v4 passes 16/16 checks, with 20 real armor contacts matched to
20 spark identities and 20 structure-borne sound calls. The buttoned crew survives;
a separate exposed-crew fixture reaches KIA at 0.70 s. Reports follow measured
wavefront timing. Audio decode/sample checks establish complete streams and no
clipped PCM samples; they do not establish subjective realism or solve every
possible OS/audio-device pop. Strong cannon events engage the live master limiter.

The frozen-scene renderer benchmark improved from 143.04 ms to 33.31 ms mean per frame
(about 7.0 to 30.0 benchmark FPS) through caster-cache invalidation, unchanged-visible
geometry draw ordering, and shared shadow fingerprints. The last fingerprint
optimization produced a byte-identical final PNG. This timer excludes Game::update;
interactive FPS is not established, and occasional static-map rebuilds still spike
up to 117 ms in that 60-frame sample. See `RENDERER_PERFORMANCE_20261001.md`.

Encoded 30 fps is not a live performance result. A dead exposed listener's audio
fade is retained. Remaining limits include finite source-map resolution, possible
subpixel shimmer, single-sampled media/glass, and unanimated authored track belts.
The manual-driving torch closeup remains very high contrast; its motion/audio
checks do not establish that its lighting presentation is finished. The final
cabin clip removes the severe white opening, but modest adaptation continues
during its first two seconds.

Final native v6 validation passes all 30 declared checks across mechanisms, cabin
and coax recordings, plus the ordinary title capture. All nine review videos load
in the in-app browser at 800×450, with no media errors; the native/2× control was
verified at 800×450 and 1600×900. The final binary and source hashes are recorded in
`artifacts/runtime/winter_release_final_provenance_20261001.json`.

The active source consolidation preserves the older renderer's unique review
documents and assets. Storage cleanup is restricted to independently audited,
regenerable Cargo caches; dirty source worktrees, private assets, unique evidence,
historical archives and the current release remain preserved.
