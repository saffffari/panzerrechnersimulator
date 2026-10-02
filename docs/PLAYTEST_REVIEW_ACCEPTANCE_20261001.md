# Winter playtest review — 2026-10-01

Review page: `captures/playtest_review_native800_20261001.html` (served on localhost8871).
The interactive game remains closed. These are actual game/map captures with explicit fixture
setup and ordinary simulation/audio; they are not recordings of native keyboard play.

## Accepted captures

- Smoke comparison: `smoke_candidate_wreck800_20261001.mp4`;12s,360frames,800x450.
  Same camera/weather/sphere Bayer mode as preserved executable baseline. The previous
 1920x1080 export was nearest enlargement from800x450. Staged wreck, intentionally silent.
- Blizzard smoke: `smoke_candidate_blizzard800_20261001.mp4`;12s,360frames,800x450,
  screen Bayer8.4/4 gates pass in both smoke fixtures, no missing particle IDs or sample
  saturation in any captured frame. Near/deck frames inspected; all three comparison videos
  reached12s/end without decoder errors in the app browser. Connected grainy plumes replace
  the detached soft billow; warm fire preserved. This is a prescribed density support, not CFD.
- Reload: `canyon_reload_native800_v1_20261001.mp4`;14s,336frames,800x450,17/17 checks.
  Actual extraction, rack pickup, deliberate ram, closure and two shots; original stereo audio.
  Decoded contact sheet inspected. Native lever is back on the loader side; two lamps follow
  the actual breech state. Original stored ammunition containment has separate source tests.
- Traversal: `canyon_jump_fall_native800_v1_20261001.mp4`;8s,192frames,800x450,9/9 checks.
  Three ordinary jumps then walking into the open cupola enters Commander mode. No scripted
  climb. Aperture/interior frames inspected. Closed roof coverage is CPU-tested, not filmed here.
- Uphill: `winter_uphill_drive800_20261001.mp4`;8s,240frames,800x450,4/4 checks.
  Actual10.29degree source-terrain ascent,16.26m travel, automatic6->5->4->3 under throttle.
- Infantry: `winter800_course_infantry_v1_20261001.mp4`;20s,600frames,800x450,11/11 checks.
 146 player MG rounds,27 Soviet rifle discharges, one infantry casualty; listener survived.
  Actual seeded Canyon groups/guns retained. Zero missing particle IDs or sample saturation.
  Full ffmpeg decode and representative frame inspection passed.

All gameplay clips carry the live game mixer audio. This bypasses the OS output device, so
recorded waveforms do not establish that the previously observed ALSA underruns are solved.

- Track cue: `canyon_track_cue_native800_v1_20261001.mp4`;4s,96frames,800x450,7/7 checks.
  Native ready frame visually shows real track geometry, carried hammer, pin contact cue and
  loose-run outline. Ordinary click advances grab to pull. This does not demonstrate completion
  of the full repair. The ring-jam fragment still lacks a dedicated captured fixture.

## Explicit diagnostic, not a passed firefight

`winter800_course_at_v1_20261001.mp4` passes10/11 observations. Three Tiger shots destroy gun0
with an ordinary88mm contact. It returns no shots: shifting its trails takes10s and laying3s;
the second Tiger hit destroys it at10.0167s. This demonstrates direct AP gun damage, not a
two-sided exchange. No hit or kill was injected. The failed return-fire gate remains failed.

## Checks and boundaries

- OLD integrated mechanics checkpoint:100 passing test executions, zero failures, one existing
  GPU diagnostic ignored; exact filters recorded in `artifacts/runtime/final_cpu_checkpoint_20261001.json`.
- OLD smoke/fine renderer:9 composed fine-renderer,2 support-normalization,2 soot,
 3 loose-track and1 driving tests pass;15 raster integration tests pass. A zero-match shorthand
  remains in the raw log and was corrected to the actual9-test filter; it is not counted.
- MAIN builds;26 display/raster integration test executions pass. MAIN rendering is not
  runtime-validated by the OLD captures. The fine-particle raster pass remains an OLD experiment.
- Source-causal external damage and independent tracks are implemented. Ring/mount energy
  thresholds, drive torque/clutch details, prop masses and clatter source level are explicit
  estimates, not a validated materials/vehicle model.
- Small prop camera feedback now follows momentum loss, replacing fixed kicks. Props use
  existing push/topple motion; this is not a new fracture simulator. Static wreck/wall seating
  remains a separate337-object map review revision, not a silent replacement of the canonical map.
- Source repair cues, visible ring fragment and exact track shoes are implemented. Generic
  repair anchors retain the existing28cm end-surface tolerance. Repair-cue visual evidence is
  recorded separately where available.

Original interactive executable is preserved. Initial review binary SHA256:
`19c9866299be4d27b6fff6ddf14d65ea6834c4e4f7447d51d91667ae60af48e1`.
The v2 build corrects driving-fixture edge timing/framing. The stricter shutdown check then
exposed a separate gameplay bug: keyboard/crew ignition was immediately handled, then the
cosmetic hand animation dispatched ignition again at contact. V3 separates physical-button
requests from already-handled animation; three actual Controls tests pass. Final driving
capture owns the integration gate. Accepted smoke/fire and gameplay sound synthesis are unchanged.


## Final driving and ignition verification

- Broken track: `winter_broken_track800_v3_20261001.mp4`;8s,240frames,800x450,30fps.
  Runtime5/5 and independent media/stop checks6/6 pass. The intact side produces2.821m
  translation and pivoting.247 active broken-shaft/clatter ticks and36 release-silent ticks.
  After6.6s engine stays off, crank remains0 and both attempted shaft demands remain0.
  One MOTOR AUS and no ANLASSER follows. Mixed world audio and engine wind-down continue;
  source demand stopping does not mean the entire soundtrack is silent.
- Manual controls: `winter_manual_drive800_v2_20261001.mp4`;8s,240frames,800x450,30fps.
  Mechanical5/5 pass,10.45m travel, two manual shifts,0.390rad traverse and0.104rad elevation.
  The real torch-lit driver cabin is visibly milky; this is explicitly a lighting diagnostic,
  not acceptance of the cabin atmosphere. No isolated-source audibility or OS audio claim.
- Both driving WAVs have352800 stereo44.1kHz sample frames, no nonfinite or clipped samples.

V3 OLD binary SHA256 (ignition integration):
`7a946fa2693aafc9f139441cbff7c36b02907a3f3bcedce308206c8a182868de`.
V3 source inventory: `artifacts/runtime/playtest_review_build_v3_20261001.json`.
MAIN final offline binary check passes after the same ignition fix. The interactive game remains
closed and its original executable is preserved.


## Cabin weather follow-up — source correction

The manual v2 view revealed an actual OLD medium/illumination leak: exterior weather occupied
interior ray segments, and the fog source used unoccluded sky/sun radiance. Relative depth
reconstruction was correct. The port reuses MAIN's source-derived cabin medium domain at2cm
column resolution, with articulated hull/turret frames, rather than a camera-role density mask.
Exterior optical depth excludes only cabin intervals; rays leaving a port retain outdoor fog.
The existing OLD sky-visibility backend now gates fog illumination at the exterior optical
centroid. Camera, optical and fine-VFX paths share the same medium. The approved OLD flame
synthesis, smoke continuity and baseline cabin haze coefficient are preserved.

This is a sampled enclosure, not exact Boolean geometry, ventilation or a full transport solver.
Native acceptance of the matched driver and exterior smoke captures is recorded below.


## Cabin correction accepted in native v4 captures

`winter_manual_drive800_v4_20261001.mp4` passes5/5 at800x450,30fps,240frames/8s.
The setup, initial state, all480 simulation samples and audio WAV are identical to manual v2.
Root and the capture agent inspected the matched4.5s view: the broad gray cabin veil is gone,
with local torch illumination and clear geometry/shadows. Torch-lit nearby surfaces remain
very bright; this confirms the weather leak correction without settling aesthetic exposure.
The review page includes matched before/after images.

`smoke_candidate_blizzard800_v4_20261001.mp4` passes4/4 at800x450,30fps,360frames/12s,
with the same2.5 storm fixture as the prior smoke capture. Root and the capture agent inspected
near/deck views: connected fine smoke and warm fire are preserved. Zero missing identities or
saturation over360frames; maximum grain weight0.0797966. Flame synthesis is unchanged, but
geometry-visible medium lighting changes some pixels (not a pixel-identical image claim).
Both complete videos decode successfully; adjacent verification JSON records details.

Final reviewed OLD binary SHA256:
`2501feb81e4e6300ff1334e6f80e684be7406136a880c56ea8964241e2c7a7b0`.
Release build68s;302 Rust/WGSL source hashes in `artifacts/runtime/playtest_review_build_v4_20261001.json`.
Cabin port: cargo check and43/43 relevant domain/shader/particle/optic/ABI tests pass.
Three unchanged legacy literal assertions in broader suites remain documented in
`docs/OLD_CABIN_WEATHER_DOMAIN_20261001.md`; this is not an all-tests-green claim.

Final review: all32 linked artifacts exist;11 video files have readable metadata. V4 smoke and
driver clips played through to12s and8s in the app browser with no media error. Fixed-width
control was checked at actual800x450 CSS content size. Source inventory remains unchanged.
