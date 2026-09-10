# Astra overnight development — September 5, 2026

Owner request: use the overnight workstation for competitive infantry training, tank MG/HE
anti-infantry training, runtime integration, a two-choice driver-viewport menu (Kampagne and
Playtest), a compact complete Playtest world, regenerated Pizza-based terrain/buildings/interiors,
scene-mode historical reconstructions using the editor/MCP, and first-person videos with actual
synchronized game audio covering every implemented mechanic. Preserve the earlier art direction:
sharp internal pixels, continuous aperture shafts, controlled dither, restrained fire, one-pixel
white snow and translucent dust. New review media belongs flat in captures/astra.

Work began about 03:34 America/Los_Angeles. Bound unattended heavy jobs to the morning of September
5 (09:00 local); checkpoint progress instead of starting another long batch after that time.
Never claim a task finished from process exit alone. Keep current weights until held-out evaluation
supports promotion; keep prior worlds and source assets. No unrelated project links or content.

## Work allocation

- Root: menu/Playtest integration, shared combat runtime integration, build/test scheduling,
  checkpoint promotion, video execution and visible review.
- Hubble: shared multi-team infantry combat, German/Soviet competitive evolution, independent
  tank anti-infantry policy, held-out evaluation and checkpoints. Existing networks are CPU models;
  reserve GPU for visual validation rather than claiming nonexistent GPU training kernels.
- Planck: editor scene/act pipeline, non-destructive versioned terrain/building/interior cook,
  local editor MCP, authored sound/light data flow, diagnostic Playtest world.
- Poincare: exact live-audio offline recording seam, first-person mechanic capture harness/matrix,
  evidence-linked historical scene content and preview sockets.

## Acceptance and work still required

1. Training executes real competitive combat and saves recoverable checkpoints/metrics. German
   and Soviet squads contain multiple actors. Evaluation uses separate seeds/opponents. Tanks can
   select/fire MG and HE against infantry through gameplay damage/cover logic.
2. The main menu has exactly Kampagne and Playtest as mode choices and depicts the driver's real
   viewport slit. Playtest bypasses campaign sequencing and contains a dense, traversable world
   running ordinary gameplay systems. Developer utilities remain accessible outside the main menu.
3. Editor scene mode preserves local terrain, placement, light, sound and evidence metadata. New
   world versions preserve old manifests/overlays. Asset variety serves coherent rooms/ecology;
   unsuitable modern assets do not enter historical scenes to inflate a usage count.
4. Historical scene inventory is derived from the existing 36-entry research catalog. Separate
   testimony-backed facts from inferred staging; do not invent testimony or imply every proposed
   event is already built. Campaign chronology and diagnostic composites are distinct.
5. Review matrix ties each mechanic to actual implementation, first-person scenario, trigger and
   observed result. Videos must have synchronized actual game audio, not a reconstructed impact-
   only soundtrack labelled as the full mixer. Record missing/broken mechanics explicitly.

## Coordination and continuation

Read this document, current agent messages and the latest receipts before continuing. Inspect
running kessel/cargo/ffmpeg/training processes before launching duplicate jobs. Parallel CPU
training is appropriate; run GPU captures sequentially. Keep checkpoints and logs under a dated
game-only artifacts directory. Remain quiet on unchanged background status; report meaningful
completion, failures or required user decisions. At the morning boundary, finish/checkpoint a
running bounded unit and summarize verified deliverables and remaining work.

Status at 04:40 local: implementation and first-person review remain in progress. Training
completed 10,000 generations in 1,073.96 seconds on the CPU. Sealed tests used 128 episodes per
candidate; German/Soviet/tank candidate scores were 295.12/361.95/651.35 versus baseline
-209.55/-205.50/597.95. These are training-arena scores, not proof of transfer to actual terrain,
armor or tank controls. The three candidates remain unpromoted until live-game evaluation.

Concrete receipts:

- Training: `artifacts/astra-combat-training-20260905-overnight/evaluation.json`; checkpoint files
  in its `candidate/` subdirectory. Use `KESSEL_COMBAT_WEIGHTS_DIR` explicitly for candidate films.
- Last integrated game unit suite: `artifacts/astra-audit-20260905/overnight-game-tests-v3.log`,
  630 passed, 1 ignored. Subsequent world/fuel/fixture/optic changes require a fresh build/test.
- World/editor delivery: `docs/ASTRA_WORLD_AUTHORING_20260905.md` and
  `artifacts/astra-audit-20260905/world-authoring-cook-receipt.json`. New worlds are preserved
  revisions; traversal/render/audio QA is still needed. Do not silently replace canonical acts.
- Historical authoring: `artifacts/astra-audit-20260905/historical-scene-authoring.json`: 22
  located drafts and 14 unlocated plans derived from the 36-entry catalog. Not 36 completed
  reconstructions; missing hero assets and inferred staging stay explicit.
- Frozen first-person queue: `artifacts/astra-first-person-20260905_controls_v4/queue.json`.
  Commander hatch/binoculars and foot/P08 clips are recorded with actual stereo mixer audio;
  fire extinguishes but its v4 camera misses the fire. Gunner/manual-reload inputs were found
  to use the wrong trigger and require corrected recordings. These are review findings.
- Driver v1/v2/v3 films failed visual review. CPU rays identify an opaque internal holder
  backing behind the glass; the outer armor already has a narrow slit. Poincare is repairing
  only the holder aperture and aligning the real eye. Existing RTT methods are no-ops, so
  camera-only/RTT-refresh edits did not repair visibility. The v2 menu screenshot was invalid
  because unposed tank geometry falsely exposed the world; do not present it as successful.

Continue with the current agents, actual player-view capture QA, runtime candidate evaluation,
then the full mechanic catalog in `docs/ASTRA_FIRST_PERSON_MECHANIC_COVERAGE_2026-09-05.md`.

## 05:25 local checkpoint

The frozen v6 executable is in `artifacts/astra-builds/20260905_v6/`. A new
`mechanic-evaluate` command executes the same Game, inputs, simulation and audio clocks without
rendering or exporting media; its `kessel.mechanic-simulation-check.v1` receipts cannot enter the
video gallery as footage. Thirty-one non-AI scenarios now pass state checks after correcting the
manual main-gun trigger, four track-hammer LMB presses and roof ammunition metric. Read the
`artifacts/astra-state-checks-20260905-v6*` receipts; driver-lean metrics require the next build.

The 45-second candidate encounter found poor transfer: 56 rifle shots per side but 34/0 damage.
Shared rifle aiming incorrectly aimed above crouched hitboxes and used dy/3D-distance as a slope.
The fix remembers only visible stance and corrects elevation without replacing learned yaw.
Tank rapid trigger release was resetting its bolt cycle and exceeding the MG firing rate; the
shared runtime/training timer is corrected. Old training scores therefore do not justify promotion.
An actual KV-2 45-second check still makes zero auxiliary HE launches; its one reported HE contact
was a primary-policy shot misattributed by the counter. Hubble is correcting that attribution and
the verified turret-pivot versus gun-trunnion elevation mismatch before retraining.

The v5 world videos pass movement/fuel state checks but fail visual QA: terrain and props are
nearly black under a bright sky. Shader audit found surface sky queries lifted from below the low
Act-II cloud into its interior, incorrectly losing the cloud lower-boundary illumination. The
minimal actual-receiver correction passes all 29 physical_environment tests. Visual proof pending.

The real driver visor now has a repaired internal holder aperture and two physical glass
interfaces; GPU rays reach external geometry. The roof pane and 0.4-second seated RMB lean are
added. The v7 full test gate currently has 655 passes, two new driver-test failures, one ignored;
Poincare is correcting source-based mesh expectations and exact fixed eye height. No v7 release
has been produced yet. Roof opening never grants driver exposure or bypasses repair requirements.

Master media library: `captures/astra/first_person_library_20260905.html`; separate hash-bound QA
sidecar keeps failed/older clips and all audio-quality reviews pending. Actual stereo mixer and
stream checks are verified, but no claim of subjective acoustic quality is made.


## Post-restart checkpoint, 07:10 local

The user reported the workstation had restarted, then confirmed drives are good and requested
return to Panzer and a status update. Do not investigate or modify drives. Prior exec sessions
and agents were lost. The original v9 test log stops during compilation; no v9 release existed.
Root restarted the full kessel test + release build for kessel/train_combat using two workers at
nice10, logs `overnight-game-tests-v9-restart.log` and `overnight-build-v9-restart.log`. Last fully
verified game suite remains frozen v8: 666 passed, 1 ignored. Recheck current processes before jobs.

The complete 31-case frozen-v7 video queue survived in
`artifacts/astra-first-person-20260905_mechanics_v7/queue.json`. All final files have matching
video/stereo stream checks; this is not blanket visual or acoustic acceptance. The refreshed master
library has49 recordings across36 scenarios, including older failures. Root is sole QA sidecar
writer. Eleven mechanic IDs have picture-only review; all subjective listening/art reviews pending.
The actual driver still has a white slit and unusably dark cabin: FAIL navigation readability.
Its source aperture/lean are functional, but actual exposure/HDR diagnosis remains next. The
opt-in `KESSEL_CAPTURE_LIGHT_LAYERS` hook in mechanic_capture will dump the completed HDR/G0 and
exposure before recording without changing lighting. Old Driver mask2 is redundant over armor;
Poincare's metering diagnosis is an inference until this actual dump is inspected.

Corrected native-gun training v8 finished10000 generations in895.82s; preserved in
`artifacts/astra-combat-training-20260905-parity-v8`. Sealed128seed paired scores German
-206.26→351.61, Soviet-200.31→193.41, tank586.585→586.628 (effectively tied). Rifle candidates beat
initial weights but won ZERO encounters against hand-coded doctrine. No promotion. New read-only
`train_combat evaluate OUTPUT --candidate DIR --baseline DIR --run RUN_JSON --seed-set transfer`
measures new stance/movement transfer; exact input hashes/reports are required. Shared runtime now
updates escort routes without the extra3.4m/s legacy catch-up movement. Candidate battle rendering
must use explicit KESSEL_COMBAT_WEIGHTS_DIR, not silently replace shipping models.

Pending v9 source includes ten owned-German GLB crouch stages with planted feet/forward-bending
knees and helmet/uniform, staged pose/hitbox/eye agreement, and Soviet fallback standing-only stance
because the requested Soviet source and17bakedposes remain absent. Official CC-BY4 model download
was blocked by the normal browser; no source recovered or installed. See
`docs/ASTRA_INFANTRY_STANCE_2026-09-05.md`. CPU geometry proof is not gameplay proof.

Default Playtest now points to immutable functional-house revision
`assets/worldgen/revisions/playtest-functional-house/20260905T130318379870Z_39d90b32/manifest.v1.json`.
It preserves619 original placements and adds the existing compiled Pizza house at an explicit
movable editor marker, with real shelving/shelter/lights/destruction. Full editor building geometry
preview remains pending; the semantic marker is visible. Three default-world scenarios plus
original winter fixtures and tree/cook-off/can/audio-source scripts await v9 state/video execution.
Root wired destruction_review/tree_review children and strict fixture schema/readouts. The sentry
fixture now clears its former escort assignment; the P08 hit/loot scenario must be re-evaluated.
Incoming-round scripts now assert one real armor contact/caliber; v7 films predate those new checks.

Remaining major presentation issues include washed-out Driver view, very dark interiors, coarse
terrain patchwork/needle grass, incomplete interaction animation, missing requested Soviet asset,
unimplemented full model xray/organs and radio/manual-crank controls. Do not call the whole game,
all54mechanics, acoustic realism, or36 historical reconstructions finished.

## Morning review steering, 2026-09-05 (active user review)

The user is reviewing first_person_library_20260905.html and these are active requirements, not
completed claims. Preserve old media and show new SHA-bound first-person videos with real audio.

- Gun sight exit: root separated remote gunsight entrance from physical cabin station transition
  endpoints. Renderer rigid-object previous transforms now live per ViewCtx, matching each camera's
  previous frame, rather than being consumed by the first optic pass. Included in frozen v9.
- v9 release sealed under artifacts/astra-builds/20260905_v9. Initial suite676pass/1fail/1ignored:
  the sole failure was Python catalog closure after deliberately restoring the original extinguisher.
  Herschel fixed catalog/test closure and all20 offline asset cooker tests pass. Full suite must rerun.
- Original hose/trigger extinguisher restored; equipped fuel can turned180deg. Both in v9, visual
  proof still pending. Correct user Soviet source with17poses and1.75m height in v9, needs new film.
- Loader authored pivot/hierarchy is correct. Its old film looked at stationary roof; only review
  look timeline changed. Commander/loader/driver periscopes are genuinely broken optical paths.
  Cicero owns physical optical path and close eyepiece viewing, full cupola ring. Gun sight circle
  should show exterior from cabin within its real exit pupil; do not fabricate omnidirectional discs.
- Driver raw v9 probe: exposure355.5, actual aperture~0.8%screen. HDR varied but appears nearwhite.
  Cicero owns gaze/glass meter and strong usable lean; original armor opening must remain evidence-led.
- Anscombe owns authored grass mask/extent correction, fine repeated Pizza terrain detail with
  grazing-angle minification, denser cover, versioned substantial rock support, doorway clearances.
  Player stature should1.8288m with eyes1.70m. Narrow old doorway has only1.70mclearance above0.4mfloor;
  wide doorway2.10m. Functional house actual storeys2.60mclear. Cellar clip was crouching and lacksroof;
  inspect actualstandinggeometry beforeclaimingallceilingsshort.
- Root owns campaignweather: replace static91%/96%laterovercast and forcedAct3dusk with progressive
  blizzard→clearing→brightblue sunny spring. Retain nightasexplicitvariant. Not implemented yet.
- Herschel owns Panzerfaust: original asset1:1 +closer underarm hold to increase apparent size;
  faction-neutral infantry/crew collision and proper armor terminal routing, not unconditionaltankkill.
- Fire/foam requested: smaller/manymorecubes, permanent surface deposits that stack and coat; fire
  rooted on fuel-bearing surfaces, suppression based on actualcoverage, spread/heating→liveammocookoff.
  Currentfoam1.6cm1152ring/360s,6.5sdeposittime, erroneous1.45mfloor; suppressioncurrentlytrigger-based.
  This needs substantial mechanicwork, not only cosmeticdensity. No owner yet.
- Root added liveAppState unskippable3secondblack atALLlocalKIA beforedamage replay or memorial.
  This edit is afterv9 and notyetbuilt. Capture presentation must match, including cookoff.
- Traversejam must make strain/rattle and bounded back-and-forthattemptwithoutactualturretadvance.
  Root owns. Main gun report too highpitched; assetmatching audit underwaybeforechangingDSP.
- User corrected blanketall-audiorejection: PRESERVE45mmM42incoming +thatclipcabinambience; KV2MG
  acceptable.85mmD5Tgoodcharacter needsmorelevel.152mmML20OF530decent,morebass/lesstopend.
  The152mmclip's actualreceipt is nonpenetratingHE. OthermatchingPizza recordings shouldREPLACE
  proceduralbases, notquietlylayerunder them. Sourcebundle path documented in ART_STORY audit.
- User wants texelsplatting and identifiedhorizonshimmer. VerifiedoriginalIndividualKex isDylanEbert:
  https://github.com/dylanebert/texel-splatting andhttps://dylanebert.com/texel-splatting/ .
  Fixedcubeprobes/worldtexelquads stabilizeperspectivepixelcreep; doesnotbyitselfdenoiseMonteCarlo
  lighting. Eye-probedisocclusionstillshimmers. Need boundedactualcabin+movinghorizoncomparisonbefore
  adoptingfullrenderer; noimplementationyet. Do notsubstituteAIimages/screenspacepixelationproof.
- Userlikesapparentpressurefrontbutwantsbowedlowopacityair,muchfaster,exactaudioarrivalatcamera.
  Rootmustidentifyflatshape in v7incoming152HE: itisnotyetconfirmedwavegeometry.

Source edits and tests do not establish art/audioacceptance. User is awake: one boundedGPUjob
atatime,640x360native,nearest1920x1080,default250mspause/3xidle. Newmediaflatcaptures/astra.


## Current checkpoint: v10, September 5 morning review

This section supersedes unfinished source status in the preceding morning ledger. The overnight
heartbeat is PAUSED at its morning boundary. The active user review continues with one bounded
first-person proof; there is no new training batch or unattended render queue.

Frozen executable: `artifacts/astra-builds/20260905_v10/kessel`, SHA-256
`b6e06e7d431054a2f80d92bca56e08aa3f1afb20469610847807f614c1fba42a`. See the exact
`artifacts/astra-builds/20260905_v10/SHA256SUMS` receipt.

Implemented in v10:
- Actual driver glass/gaze exposure metering; 500 mm seated lean with explicit close inspection.
- Seven commander cupola optical paths through source window/mirror geometry, and close viewing.
  Loader/driver roof reconstruction and accurate gunner objective registration remain separate.
- Original extinguisher, corrected Soviet model, binocular eyecup rotation, fuel-can orientation,
  Panzerfaust hold and faction-neutral contacts with a distinct HEAT terminal model.
- Fine mipmapped terrain detail, denser grass, grounded rocks and taller functional door frames in
  an immutable Playtest revision. See ASTRA_GROUND_HEADROOM_20260905.md. The driver sound bench
  deliberately still uses its old isolated terrain setup; it does not prove this world revision.
- Surface-attached fire, permanent 8 mm foam deposits and coverage-driven suppression. Source
  collision grid replaces the invented floor; live ammunition still governs cook-off.
  See ASTRA_SURFACE_FIRE_FOAM_20260905.md for model limits and required runtime proofs.
- Campaign cloud/aerosol/storm profiles clear progressively with gameplay progress, smoothed over
  time; later acts use daylight. Winter snowfall now follows storm intensity.
- Deliberate trigger click with blocked cannon/spent chamber; interior coax top-end attenuation;
  hatch grind during measured movement and metal stop only at the actual endpoint; starter-motor
  approximation during the existing crank interval; bounded traverse-jam backlash and strain.
- Exact Pizza recordings replace selected pistol/rifle/hammer/cannon source voices. New heavy-door
  and trigger foley use the same bank. Recipes in catalog/audio_feedback_20260905.json and cooker
  tools/cook_audio_feedback.py. Main-gun blast is an authored explosion adaptation, not a verified
  Tiger recording. No subjective audition is claimed. Engine/ambience and 45 mm retained;
  85 mm gain, 122 mm low-end weight and 152 mm top-end/body balance adjusted separately.
- Player KIA cuts to black for three seconds before replay; recording path now follows that state.
  Multiplayer individual-KIA presentation still needs a separate visible audit.
- Incoming labels carry platform/gun/round/range and CONTACT TEST disclosure. The old OF-530 clip
  was incorrectly labelled ML-20: its actual source profile is KV-2 / M-10T. Old receipts preserved.

Validation: initial v10 unit run 689 passed, zero failed, one ignored. Exposure/physical environment,
optical and surface contracts passed after replacing an obsolete source assertion (`let material`
became `var material` for terrain detail). Full final test-gate log is
artifacts/astra-audit-20260905/v10-complete-test-gate.log; inspect its final results before claiming
all tests passed. Frozen release compiled successfully. The first v10 driver recording was stopped
after the surface shader alone needed 268934 ms to compile and the volume shader began compiling.
No v10 video was delivered. Log:
artifacts/astra-audit-20260905/driver-feedback-v10-capture.log.

Still open: texel-splatting prototype; actual moving-camera sampling/horizon comparison; physical
loader/driver roof and gunner optical routes; pressure-front identification and synchronized curved
wave; first-person proof for new terrain, fire/foam, combat, models and all other revised mechanics;
training transfer evaluation/promotion. Preserve the sealed v8 policies; no success claim or promotion
for the failed infantry evaluation. Follow optics-followup-after-v10.md and the source geometry audit.


### v10.1 recovery checkpoint

Frozen executable: `artifacts/astra-builds/20260905_v10_1/kessel`, SHA-256
`952e9766a33d9aba2660b4e08eb29527af4eb393cca050872758bb16c519e1da`.
The source-receipt.json beside it hashes the exact source, shader and catalog inputs.

- Optic path preparation shares one downstream world/atmosphere evaluator again. Static inlining
  copies drop from four to two. The unchanged 12-step optical bound is carried in the shared
  source-scene header to avoid driver unrolling; path geometry and sampling weights are retained.
- The initial v10 sound replacement incorrectly affected tank MG because it shared RifleShot.
  TankMgShot now has an independent dispatch identity with exact old procedural PCM and legacy
  optional layer/variation mapping. Both real enemy tank-MG dispatches use it. Infantry remains
  on the new recorded rifle source. Two regression tests verify the separation.
- Capture telemetry now exposes persistent surface deposits, first-patch covered area and live
  ammunition thermal exposure. It records real state without changing the simulation.
- User sound feedback was appended against exact old video and receipt hashes. 45 mm and its
  ambience have explicit audio approval; other partial approvals and requested changes remain
  described as pending. Existing picture failures were preserved.

Validation: 691 unit tests passed, one ignored. The three previously failing external targets now
pass all ten tests. Release compiled in 37.72 seconds. New driver proof is running one bounded
job; actual video, compilation performance, art and audio still require review.
Logs: v10_1-unit-tests.log, v10_1-contract-rerun.log, v10_1-release-build.log, and
driver-feedback-v10_1-capture.log under artifacts/astra-audit-20260905.


### Morning visual evidence and v10.2 glow pass

The completed v10.1 Driver recording passed all ten control checks and failed its picture review.
A dedicated runtime probe then identified the large dark rectangle as Arena cover wall k=0, scene
slot 1, 48.216904 m past the glass. Source construction predicts 48.2174 m. The 97.319 mm glass
path transmits correctly. Do not remove tank armor or glass to fix this real external obstacle.
A clear-road scenario in the actual immutable Playtest revision is now prepared. The separately
dim cabin remains a real lighting/readability limitation.

The v10.1 fire film showed persistent foam but only 393 deposits and 10.75 percent first-patch
coverage. It did not extinguish. Four-layer stacking saturates a stationary nozzle footprint;
the follow-up uses ordinary mouse input to sweep five rows while retaining the outcome checks.
Old failed video and QA hashes are preserved.

Frozen v10.2 executable: artifacts/astra-builds/20260905_v10_2/kessel, SHA-256
9a17815393bf789cdb5c233c475b71382b56e6fe146bcb78bbd166d91e2eaa14. Source receipt is beside it.
It shares the GPU-adapted camera exposure between glow and final tonemapping, meters transported
HDR before bloom, and retains world-space HDR radiance in the bloom composite. This makes an
interior emitter eligible for a halo when it is bright to the adapted camera. Snow and dust remain
after glow/tonemapping, preserving their one-native-pixel coverage. The four fire stages now follow
a coherent source-space hot core/cooler shell instead of height-only stripes; the palette has a
deep red base and orange core. Cube size, motion and density remain unchanged.

Audio startup and capture metadata now describe actual loaded replacement files, checksums,
legacy status and generated families. Default review has eight verified feedback recordings,
nine recorded Sfx replacement routes, eighteen generated Sfx routes and zero enabled v1 layers.
The separate optional v1 closure conflict with the new verified child bank is under repair in
post-v10.2 source; no waveform change is authorized or needed for that compatibility repair.

Validation: 695 unit tests passed, one ignored. Bloom uniform span/offset and render ordering,
exposure ABI, and fine-cell effects contracts passed. Release compiled in 36.04 seconds. The
v10.2 fire sweep video is still being rendered; do not infer its outcome or art quality from tests.


### Morning v10.3 verification and remaining driver failure

The v10.2 glow sweep visibly extinguished fire by 5 seconds and retained 4,691 foam cells
covering the source surface. Sixteen of seventeen checks passed; the remaining failure was
a stale authored ammunition-exposure reading after extinction. The inactive-fire update now
clears that exposure and cookoff timer while preserving the coating. This is not a model of
physical ammunition temperature or cooling. Old failed film and receipt remain unchanged.

The real Playtest road film passed all six controls checks, including both lean cycles and
8.4006 m displacement. The prior Arena wall is absent. Independent decoded-frame review
still fails visual/art acceptance: cabin is nearly black, road boundaries are hard to read,
the exterior remains noisy, and the 4-degree inspection crop is narrow. Do not call the
driver lighting/readability repaired merely because the window transmits and the tank moves.

The optional original audio bank now validates alongside the strictly verified independent
feedback partition. Original files, gains, variants, receipts and waveform hashes are unchanged.
Twenty Python tests and the actual bank validation pass. v10.3 has 701 passing unit tests,
one ignored, plus all three fine-effect contract checks. Release build completed in 38.29 s.
Frozen executable: artifacts/astra-builds/20260905_v10_3/kessel; SHA-256
b23839605561dd98108f576ede154e846e9bf056cc671e5f0418ad7ac463b420.
The final v10.3 fire sweep was launched as one supervised bounded capture; a completion
receipt below will state its actual result. No overnight queue or training was restarted.


### Completed v10.3 video proof

[Actual first-person video with game audio](../captures/astra/loader_fire_glow_sweep_v10_3_20260905.mp4)
passes all **17 of 17** declared outcome checks. At 5 seconds fire and authored ammunition
exposure are both zero. Final coating has 4,691 deposits / 100 percent source coverage,
zero airborne foam after release, and the ammunition rack and tank remain intact.
Six decoded frames confirm the orange core / red base and shell, connected glow, aimed sweep,
extinction and retained surface coating. Visual mechanic coverage passes; sound quality and
final art approval remain pending. Existing sky/transport noise and dark driver lighting are
not resolved by this fire pass. Output: 10 seconds, 1920x1080 nearest upscale from 640x360,
24 fps with stereo 44.1 kHz AAC gameplay audio. All supervised GPU jobs have finished.

Exact MP4 SHA-256: 0e27e66ae9f037bb1352798832ce452a90cf87e9324eeed0e067af5378fc2764
Capture receipt SHA-256: 2535b3fddc94b5333c98194a2f79ae1b0677572b3b462f952c1e6e08ba7d7059
Frozen-binary provenance: ../artifacts/astra-first-person-20260905_feedback_v10_3/queue.json
