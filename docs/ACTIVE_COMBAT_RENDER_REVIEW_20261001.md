# Active combat/render review — 2026-10-01

This tracks the user's ongoing requests, not completion claims. Previous verified baseline is
commit c6bdb33 and the preserved blizzard v6 films. Current changes are uncommitted.

## Required outcomes

1. Correct interior light leaks, excessive bloom and separate cabin/outdoor atmospheres.
2. Audit and replace geometry-bypassing lighting across the active renderer, including sky/sun,
   local sources, surfaces, smoke, snow, glass and optics. Document numerical limits honestly.
3. Improve the combat soundscape: firing and distinct impacts, engine idle/load, nearby enemy
   engines, turret traverse, loading and other mechanisms, terrain/track/suspension reactions.
   Preserve useful force communication from the camera. No pre-hit artificial ducking.
4. Longer, farther blizzard fight with six T-34s; real first-person cupola/interior videos.
5. Matched render look tests: ordered dithering strengths, palette/grade/exposure treatments;
   daylight, twilight, dark desaturated night, firelight, blizzard and cabin. Do not silently
   replace approved defaults before visual review.
6. Isolated Panzer raymarching experiment inspired by deltaP. Clearly identify analytic test
   geometry versus the original tank meshes; lighting and palette remain separate stages.
7. Multiple new combat videos in different locations/environments, each labeled with one test
   purpose. Candidate suite: moving daytime drivetrain/suspension, urban shadow/acoustic paths,
   distant blizzard visibility/tracers, night firelight, dawn/dusk, and crew machinery/reload.

## Current evidence and work

- All32 admitted point sources now obtain shared source-geometry shadow maps (linear-depth512m,
  512px faces); named fixture registrations only identify emitter meshes/torch direction.
- Artificial closed-breech lamp removed. Portal bloom reduced and interior broadkernel removed.
- Catastrophe bass source revoiced; own-compartment explosion routing corrected while preserving
  headset/hearing. Focused source/hierarchy/routing tests pass.
- Composed shader checks16/16 pass at native checkpoint (before upcoming portableBVH integration).
- `captures/blizzard_inside_shadow_probe_v7_20261001` is a passing6s/5fps diagnostic with actual
  combat/audio. Not a final deliverable: current convex cabin medium has a measured40cm exterior
  overreach and is being replaced with a source-projected interval grid.
- Apollo AI: isolated two-level source-triangle softwarevisibility module+WGSL, then dielectric
  transmission. Root will wire source geometry, current instances, buffers and shader consumers.
- Athena AI: point shadows/particle lighting complete at checkpoint; sourceconnection extinction
  and new medium-grid renderer integration. Coordinates with Hephaestus.
- Hephaestus AI: source-derived cavity interval grid replacing invalid convex medium.
- Root: integration, actual GPU capture, soundscape inspection, raymarch experiment, look/combat
  matrix, end-state validation. All generated media flat in captures/.

## Live observations

- User liked deltaP's image; source inspection confirms shared SDF visibility, one reflection,
  approximate AO/softshadows and authored lighting. Its earlier128x128 frame is upscaled3x with
  PICO-8 pair matching/ordered dither. It is not a complete physicallybased renderer.
- Current live crew audio quiet bed0.5–1.5s measures about−49.8dBFS RMS; own shot4.0–4.2s about
  −12.9dBFS RMS. Engine and enemy loops are wired; audibility/reproduction and missing terrain
  mechanism sources need further work. Stationary capture fails to demonstrate moving machinery.

## Later user steering and integration checkpoint (16:35 local)

- User likes isolated raymarch appearance, requests **native960x540**, then specifies blizzard with
  full first-renderer feature scope. Asked for several dither algorithms, then chose **unordered** as
  lead direction. Root explained fixed blue-noise avoids a Bayer grid without re-randomizing each
  frame; geometry/snow/edge shimmer still requires a motion comparison. No claim of total stability.
- Raymarch experiment in `experiments/panzer-raymarch` builds with separate target/raymarch. Day
  native960 plain/ordered stills and6s24fps orbit are rendered; original320x180 upscaled comparisons
  also preserved. These use analytic Tiger-like geometry, not native source assets or live combat.
- Added dynamic world-space fog/cloud/smoke density and cellular snow in weather.wgsl, shared source
  attenuation and geometry visibility, bounded volume scattering quadrature. Added Apollo's
  scene_detail.wgsl:2analyticT34-like tanks,11trees,2ruins,5snowmounds. Prototype still lacks native
  crew/optics and combat/VFX/audio parity; full-set request remains active, not completed.
- Native960 blizzard v1 rendering stopped after>3minnooutput; thin ellipsoid distance bounds were
  improved conservatively using the enclosing-box lower bound and quadrature counts reduced.
  160x90 detail probe v4 now renders; native960 blizzard plain v2 currently running. Timing logged.
- Hephaestus AI owns `src/bin/dither_review.rs` CPU frozen-frame comparison (Bayer4/8, true toroidal
  void-and-cluster blue-noise, serpentineFloydSteinberg). Blue-noise now lead; also export a tile/LUT
  for root to wire into live shader. No random-per-frame noise. Root owns shader/host integration.
- Main renderer softwareBVH +2cmcavitygrid integrated. CPU source16/16, cavity3/3, shaderNaga16/16
  pass; latestreleasebuilt. **Actual NVIDIA Vulkan FAILS with NVVM compilation failed:3**, both
  native blizzard preview and isolated BVH parity compute. llvmpipe parity512rays passes but is not
  hardware validation. Athena/Apollo own NVIDIA compiler isolation/fix. No fallback/light bypass
  authorized. Native combat suite is blocked until this is solved, not accepted.
- Hephaestus added native `combat-scene-suite PREFIX SCENE 16 preview [looks]`,6separate cases plus
  distant_blizzard_inside. Moving case drives0.5–5s then stops/fires. Six fixed-HDR display variants
  at3snapshot times use renderer_look_review.rs, without rerendering transport/exposure. Not yet run.
- Root audio_running_gear adds travel/terrain-driven contact kernels; authored acoustic approximation,
  not suspension dynamics. Hephaestus fixed pause/master/death/invalidinput lifecycle;4/4pass.
- Balanced playback is explicit global reproduction, sourcelevels untouched. Initial .6slopebelow110
  failed100m catastrophe separation (6.914dB<8dB). Root adjusted: .75slope80–110, .55slope70–80,
  max12dB liftbelow70; inverse/testupdated. Reproduction8tests must rerun. Capturemetadata namesmode;
  KESSEL_AUDIO_RANGE=reference retainsoldcurve. No assertion weakened or source boosted.
- No newcommit yet. Preserve all existing work/evidence; nativev8 failed beforemedia, not a pass.

## Final steering before native comparison

- User then requested videos of **both** Bayer and unordered/fixed-blue-noise **before deciding**.
  Preserve default; matched native960x540 blizzard A/B video now primary immediate deliverable.
- User explicitly rejected analytic scene as not the game: **use game assets and map**. Do not
  present the analytic benchmark as satisfying the gameplay/renderer comparison. No more work on
  analytic scene for the requested deliverable. It remains a separately labeled experiment.
- Root asked optional async preference: exact meshes + raymarched atmosphere/VFX (recommended) vs
  raymarch geometry too (source distance fields). Pending; continue native compile and paired capture.
- Root production crush change: fixed rank texture binding7 R16Uint, selector reusesbyte56
  (`CrushUniform._grade_pad[0]`, shadername dither_pattern), ABI80unchanged;0Bayer4/1blue64.
  assets/render/blue_noise64.bin generated byHeph,8192bytes u16LEranks0..4095, FNV1a64
  e2a6d4e8b8fc7dc9. No time/frame noise, no default change. Sourceutility dither_review owns generator.
- Root added Renderer::debug_capture_frozen_dither_pair(device,queue)->Result<[Vec<u8>;2]> in
  renderer_look_review.rs: exactnativeRGBA/sRGB Bayer4/blue64, oneHDR/currentparticles/eye state,
  no simulation/lighttransport/frame/audioadvance; restoresoriginalGPUuniform andLDR. Hephaestus
  adds `dither-video` flag to native suite with paired encoders and identical liveaudio.
- NVIDIA failure narrowed further: exact triangle + BLASnestedleafloopcompiles, addingalpha-test
  blockfailsNVVM3. Apollo fixes alpha exactequivalent; Athena verifies productionfragment after.
  No geometry omission or light bypass permitted as workaround.
- Native960 analyticblizzard v2 did finish afterdistance/quadraturefix: pipelines.168s, frame1.087s.
  It is NOT the requested gameplaycomparison and should not be promoted as such.

## Real-game integration and parallel old-renderer review

- The NVIDIA compiler failure was isolated to unsigned max-value division in the source-alpha
  overflow guard. An equivalent division-free guard now compiles. Native hardware parity now
  covers 512 general rays, eight strict edge neighbours, and two explicitly bounded grazing
  centres; both the optical solver and opaque early-exit branch are exercised.
- Revised balanced audio passes all eight reproduction tests. The 100m catastrophe/impact LAF
  separation is 9.099dB, above the unchanged 8dB gate. Display/transport contracts pass48/48.
- The first real-game BVH preview produced only one frame before being stopped for excessive
  cost. `game_blizzard_dither_probe_v9_first_20261001.png` has conspicuous unresolved-optics
  diagnostics in its periscope views. This is a failed diagnostic, NOT a playable/accepted film.
  The requested480x270 was unsupported and the logged actual diagnostic size was640x360.
- Root added projected-aperture sampling for periscope targets: two source samples per native
  aperture pixel, common integer resolution divisors, unchanged camera/FOV/aspect/source geometry,
  and correctly scaled panel UV, sky, glass, smoke and particle viewports. A distant test pane now
  needs64x36 while an eye held near it retains640x360. Ten optics tests pass, including composed
  WGSL. Actual GPU speed and appearance still require a new run.
- Athena AI added source-opaque early exit only on segments whose TLAS bounds exclude optical
  candidates, retaining ordered glass/endpoint handling otherwise. Mathematically zero source
  radiance connections are skipped without changing quadrature weights. Native parity passes.
- Hephaestus AI added a debug-only CPU report of actual optic camera/medium source connections.
  The v10 first-update report queries396 paths (348 clear,48 blocked,0 unresolved) through one
  prepared64x36 port. It does not query the main-view glass pixels that were pink in v9; this
  limited sample does not establish GPU correctness. Athena is extending the same-pose report
  to main-view receivers and an explicit one-frame render before changing optical semantics.
- User explicitly requested parallel progress and videos with the old renderer. Hermes AI created
  `/home/alex/panzer/old-renderer-review-20261001` atc6bdb335be8fc471bf6232198f67202ae4b904fe.
  That checkout preserves the old renderer and has a native960x540/30fps scene-suite recorder.
  Assets are project-local Btrfs reflinks, not shared writable symlinks. The first16s distant
  blizzard capture completed: native960x540/30fps,480decodedframes,H264/AAC,8/8 observation
  checks. Actual1Tiger/2T34shots; CommanderKIA7.6667s. Root inspected its contact and full decode,
  then delivered the video inline. Prefix `old_renderer_blizzard_cupola_v1_20261001` retains the
  baseline mix, old bloom and160m initial staging. It is not proof of the later audio fixes.
- Hephaestus AI ported bounded audio improvements and small approved muzzle/bloom fixes into the
  old-renderer checkout, keeping unvalidated BVH/cabin ABI changes separate.108audio tests pass;
  3existing diagnostic tests remain ignored. The source-pressure model is unchanged; balanced
  reproduction and track-contact kernels are explicitly documented approximations. Actual
  updated capture analysis remains necessary.
- Hermes AI ported only the dither display selector/helper into that checkout. Its first matched
  native960x540 pair uses six T34s at250m with staggered readiness6+k*2.5s. This is disclosed
  initial staging, not invulnerability or forced outcomes. Current recording already observes
  two Tiger discharges, three enemy discharges and ordinary catastrophic damage; final receipt
  and playback checks are pending. Interior, moving day, city and dusk recordings follow.
- Root added `tools/build_combat_review.py`: explicit receipt inputs, complete video decode,
  dimensions/frame-count checks and hashes, then a local review page with shared-timeline dither
  switching and separate scene videos. It cannot approve visual quality or select outcomes.
- Bayer versus fixed blue-noise remains undecided. Do not promote the experimental scene or a
  diagnostic frame as the requested actual-game, native960x540 motion comparison.

## Winter-only steering and selected display look

- The actual-game old-renderer matched pair completed at960x540/30fps/480frames, with2Tiger
  shots/3T34shots andCommanderKIA11.25s. Root decoded all outputs, inspected native PNGs and
  exercised the local review page's playback/pattern controls. Same live audio across both
  patterns; independent same-HDR passes differ in all480frames. Prefix in old checkout:
  `old_renderer_blizzard_pair_v2_20261001`. Its review page is `old_renderer_dither_review_v1_20261001.html`.
- User rejected the shimmering grass and green ground, explicitly narrowed work to the winter
  map, then selected **ordered dithering and fewer pixels**. Root chose the next step at native
  **640x360**. Stop launching other map/location captures; preserve already-recorded diagnostic
  clips. The previous blue-noise lead/undecided language above is historical, superseded here.
- Root found the green-ground cause: the capture used Act0's legacy training DEM, not the
  authored Canyon winter map. Its season is Winter but neutral atmosphere mood0.35 reached the
  terrain shader, blending78.4% spring pasture. `campaign_ground_mood` now derives terrain
  season from the map in both campaign setup and weather progression, in main and old checkout;
  a snowy map cannot turn green as a weather front changes. Shader lighting is unchanged.
  The unit regression across all authored profiles/interpolations passes in the old checkout.
- Hermes is adding `winter_canyon` against the actual CanyonV2 Act1 Playtest manifest, authored
  spawn, Gaea rock/snow maps and the existing91pocket winter vegetation mask. The prior arena
  used generic fallback stalk distribution. Native capture dimensions will follow the renderer.
  First640x360still must be inspected before a longer new film. No authored obstacles are deleted.
- User requests fire/other VFX cubes approximately one native pixel and vastly more particles.
  Hephaestus measured exterior fire as768parcels/source,80–115mm; interior fire already has up
  to49152grains of6–8mm. Exterior/weapon parcel refinement is in progress: deterministic fine
  grain placement over original support, preserving total emission, with a measurable GPU
  instance budget. Do not claim that shrinking existing grains alone meets the request.
- Hephaestus also found eight actual interior-fire sources missing from the old shadow bank.
  All eight are now registered in source (25total finite-cubemap slots),12focusedtests pass;
  a fresh native render is pending. This does not solve the old cabin haze/world-medium boundary.
- Main renderer v11 one-frame diagnostic completed in about9.56s total; render/readback/PNG
  section about1.82s, a cold isolated observation notFPS. Pink source paths are now reproduced:
  actual relay lenses and nonparallel entry/exit faces of closed panes. Athena is implementing
  direction-aware environment transmission using existing source optic laws. Finite-source
  boundary-value paths and full Fresnel multipath remain explicitly separate limitations.


## Current resolution and fine-effects contract

- Latest user decision is **640x360 with ordered Bayer dithering**, superseding the earlier960
  preference. Final new review stays on the actual Canyon winter map, not the old training arena.
- The user explicitly requires individual VFX grains approximately one native rendered pixel
  regardless of resolution. A fixed world-space subdivision cannot satisfy this at different
  distances, FOVs and port-target sizes. The active implementation is a dedicated HDR grain path
  before exposure/bloom/crush, using each view's actual projection and target extent. Overall
  effect support and motion remain world-space; density/coverage must preserve the plume.
  This is stylized sampling, not an assertion that camera-facing grains are physical flame geometry.
- Hermes' first native640 winter still is inspected: authored snowy ground and sparse winter
  vegetation, with heavy blizzard visibility. Long captures wait for grain and audio integration.
  Prefix: old-renderer-review-20261001/captures/old_renderer_winter_canyon_640_bayer_still_v1_20261001.
- Root's new combustion synth has a low roar with sparse irregular crackles. Continuous fire uses
  an authored86dBA-at1m reference (+12dB over the former74), with existing source propagation,
  headset, hearing and master reproduction. These are authored estimates, not historical measurements.
- First actual newly burning cabin surface schedules a finite ignition catch at its patch position.
  Destruction already has a catch and is not duplicated. Non-catastrophic ignition uses authored
 106dBpeak/92dBAat1m; the ordinary arrival queue owns delay and compartment/headset transfer.
- Confirmed internal detonations now have a causal upper-bass compression body and reduced noise
  dominance. Dry penetrations retain steel/fragment character. A trial delayed wavelet failed the
  existing onset test and was replaced by a causal damped response; no assertion was relaxed.
  All44 acoustics tests pass: artifacts/runtime/winter_fire_impact_audio_tests_v3_20261001.log in
  the old checkout. A fresh gameplay capture is still required for perceptual acceptance.
- Hermes is implementing a separate elevation mechanism voice from actual gun-pitch motion and
  the source-model elevation-wheel location. It must be silent at rest/stops and remain available
  for manual elevation with the engine off. New winter capture controls must slew normally;
  direct debug gun-pose assignment is not evidence of mechanical motion.
- Athena's corrected shared triangle-edge evaluation passes RTX4090/Vulkan:522geometry rays
  (520strict plus2 explicitly bounded outer-edge cases) and448strict optical paths. Native v13
  still retains a small unresolved-optics patch; it is not accepted as a final renderer video.


## Obra Dinn-inspired motion comparison

- User confirmed Return of the Obra Dinn as the reference and explicitly requested trying its
  dither approach. Primary account: https://dukope.com/devlogs/obra-dinn/tig-32/ . It describes
  Bayer8/blue128, camera-centred/world-oriented sphere mapping,2x thresholding and downsampling,
  and separate digital/analog modes. It does not require a raymarched geometry renderer.
- Root implemented optional display mode2: camera-centred world-oriented latitude rings with
  circumference-adjusted whole Bayer8 tiles. Four thresholded subpixel samples resolve to each
  native pixel. This is an independent inspired implementation, not Pope's unpublished shader.
  Scene colours, palette levels, eye response and HDR remain matched; this is not a1-bit conversion.
- Camera rotation is represented, but camera translation and moving objects are not stabilized
  by the angular mapping. Ring/pole and screen-edge distortion remain things to inspect in motion.
- Crush appends camera basis/FOV to128bytes; original80byte prefix and GPU eye offsets remain.
  Current Bayer4 remains default. A generic frozen-frame helper compares Bayer4 with mode1blue
  or mode2sphere without advancing simulation, transport, exposure, particles or audio.
- New old-renderer suite flag obra-dinn-video records Bayer4/sphere_bayer8 from the same HDR frames.
  Winter cupola includes an ordinary12degree head-turn arc1–3seconds; source tanks and gun drives
  still update normally. Reference/output target remains640x360ordered; review choice pending.
- Renderer recommendation is hybrid: original source meshes plus coherent geometry visibility
  and ray-integrated atmosphere/VFX. The new path is not performance-accepted;1.67s diagnostic
  frame/readback/PNG and cold compilation time are not gameplay FPS. Raymarching itself is not
  inherently unrealistic. The separate analytic SDF study is not the game's performance evidence.


## Live 800x450 playtest and next particle checkpoint

- Live game was launched fullscreen with the shader renderer, CanyonV2 winter assets, high
  preset and sphere_bayer8 at800x450. User confirmed this resolution is perfect. Preserve it.
  The5120x2880 output uses nearest scaling at6.4x (six/seven physical-pixel runs, no bilinear blur).
  Current executable SHA256:80556eb9144d2f758f8d8de6c3760f13d8397a4569e785a08cdc8f03bff1b41e.
- User requested a major particle push while playing. No GPU captures or game restarts during
  their session. Source updates are for the next build, not modifications to the running process.
- Next particle checkpoint: persistent per-view GPU buffers, adaptive tiny-soot density,
  near-plane clipping, a strict8M-grain budget with explicit omitted-coverage/energy receipts,
  stable emitter/parcel/generation IDs, stable per-surface identities, proper lifetime elapsed
  time and impact-age credit. Root fixed full-width seed-to-float phase loss; three CPU phase
  regressions pass. Weather wind is shared by weapon flash and its volumetric plume.
- User observed dither softness. Mode2 averages four threshold samples and has angular cell
  distortion. Optional mode3 screen_bayer8 now gives one threshold per native pixel and a
  Graphics/Retro pattern selector; sphere mode remains for comparison. CPU tests pass; no live
  settings change or post-change visual acceptance yet.
- Active playtest log reports right-track failure and successful engine restarts. User reports
  no repair highlight, placeholder blocks, stowed rounds protruding through armor, excessive
  cabin haze and popping on A/D/other controls. A live screenshot was inspected at old checkout
  captures/live_track_repair_bug_20261001_01.png, showing carried hammer and external ammo tips.
- Root replaced repair placeholder boxes with actual5426-triangle Hull_0628 source geometry,
  extracted losslessly at its authored scale, and added shared pin targeting for input/prompts/
  depth-tested selection outlines. Link spacing now follows arc distance. The original rigid
  hull still includes its baked track skin; this is not a fully deforming/destructible track rig.
  CPU integration tests and a subsequent real repair playthrough remain required.
- Root reduced cabin baseline haze0.12→0.025 and dust0.12→0.035; actual shot/fire accumulator
  contribution0.78 is preserved. This is a medium-density tuning change, not a light bypass.
- Hermes is repairing the both-tracks movement gate to allow independent traction. Athena is
  auditing stowed round attachment frames. Hephaestus found stepped continuous-loop controls
  in addition to confirmed ALSA underruns and is adding sample-rate control ramps/regressions.
  None of those pending fixes is yet claimed as live or audio/visually verified.


## Playtest fixes and grouped infantry checkpoint, 2026-10-01

Status: source changes in both review trees, integrated CPU checks underway. The running
800x450 game has not been restarted or modified. No new GPU, sound-listening, gameplay,
or performance acceptance is claimed. Its original executable is preserved in the OLD tree at
artifacts/runtime/panzer_interactive_800_before_playtest_fixes_20261001, SHA256
80556eb9144d2f758f8d8de6c3760f13d8397a4569e785a08cdc8f03bff1b41e.

- Window-size choices now include 3840x2160 and the Apple Studio Display's 5120x2880.
  The internal render-size setting remains independent; the chosen 800x450 is preserved.
  The user confirmed menu buttons already work. Small Controls/LAN pointer gaps were found
  during inspection and corrected; the menus were not redesigned.
- Repairs now have small native-pixel contact cues and actual-component selection outlines.
  A ring jam has a nonemissive lodged steel fragment at the exact turret-local repair point.
  The displayed work progress follows the contacted repair job. The old invisible jam and
  missing track outline explain why a player could swing the hammer without finding contact.
  Existing generic repair anchors retain their 28 cm end-surface tolerance; registering every
  approximate anchor to an exposed source face is a separate remaining geometry task.
- Independent track traction and source-derived replacement shoes are in both trees. One
  broken track permits limited movement/pivot about that side; it does not disable the engine.
  The original baked track skin is still rigid, so this is not a fully deformable track rig.
- Stowed rack cartridges are attached to the hull in their correct reference frame. A long
  support fitting formerly misidentified as ammunition is loaded separately, leaving 64 real
  cartridges. Source-containment tests found zero exposed rack vertices at five turret angles.
- Actual player gearbox/engine dynamics replace the speed-only engine-audio approximation.
  Automatic is the default; Driver PgUp/PgDn selects manually and End returns to automatic.
  The same gear/RPM/load drives motion and engine audio. Torque-curve details, clutch timing
  and adhesion remain explicit tuning estimates; this is not a validated Tiger drivetrain model.
- Continuous audio controls use sample-rate gain/rate ramps and a repaired traverse-loop seam.
  ALSA underruns observed in the live session are a distinct remaining host/runtime issue.
- Moving vehicles now fit pitch/roll to terrain supports. A separate, non-destructive map review
  revision seats 77 wrecks and 260 walls on source-derived supports:
  assets/worldgen/workshops/gaea-canyon-grounded-review-20261001/grounding.receipt.json
  in the MAIN tree. It is not the running/canonical map. Rigid wrecks can bridge uneven ground;
  the worst remaining support gap in this revision is 0.593 m and needs visual review.
- Canyon allocation rises from 24 to 60 Soviet actors, with up to six geometrically sited AT
  guns, their crews, tank escorts, and the remaining riflemen in loose groups of at most six.
  Added opening-hamlet/saddle post candidates address the previous kilometre-long gap.
  A whole group must fit nearby clear terrain or the site is rejected. Corpses are retained.
  This is encounter tuning, not a claim of exact historical squad establishment.
- Group cohesion uses the living group's medoid to bring separated members back. Nearby
  visible squadmates can point out a fresh contact as a look direction; no enemy memory or
  firing permission is shared. Each soldier still needs ordinary personal FOV and LOS.
  Actual map-start placement and multi-minute group behavior await the held gameplay test.
- Mounted-player perception and small-arms contacts now use the player's retained articulated
  source armor and existing crew anatomy. Exposed crew can be hit through a clear aperture;
  a nearer physical armor face blocks the bullet. Other vehicles and cannon contacts retain
  their existing separate collision paths.
- Direct AP contact with AT guns uses rendered component geometry and a residual shell
  velocity/penetration budget. The remaining shell can strike crew on its actual subsequent
  path; AP is not converted into an invented radial blast. Unspecified steel is an RHA
  surrogate, and tyre-only material penetration remains unsupported/terminal.
- Coax spread is bore-relative rather than world-axis-dependent, with 1.2 mrad per-axis sigma
  as a tuning baseline (roughly 0.59 m 95-percent circle diameter at 100 m). This is not a
  verified historical MG34 grouping specification. Gun/sight offsets and projectile drop remain.

Standalone CPU checks already passed: grouped infantry 4, coax spread 2, static seating 3,
powertrain 6. Integrated test results are recorded separately in OLD artifacts/runtime; a
successful compile alone is not visual or audible acceptance.
