# Complete reference-driven game checklist

Consolidated October 2, 2026. This is the implementation and review list drawn
from the supplied footage, sound, firing GIFs, interior photo, green-map color
photo and earlier impact reference. It carries forward the agreed pixel-rendering constraints. It does
not reopen unrelated menu, repair or storage-cleanup work.

An unchecked item means **not yet accepted against these references**, not that
the game has none of that functionality. Existing propagation, individual
bullet contacts, fire simulation, powertrain and rendering work must be retained.
The preceding source audit used commit `e11ec0e`.

Evidence labels: **U** user request or user-heard observation; **O** inspected
visual evidence; **G** confirmed implementation gap; **M** signal measurement;
**P** existing behavior/style to preserve and verify; **Q** unresolved reference
identity, historical detail, timing or subjective acceptance. Sound descriptions
from the user are not represented as independent assistant listening judgments.

## 1. Gun discharge and smoke — 8 items

- [ ] **GUN-01 — Weapon-specific discharge.** Give Tiger, T-34 and anti-tank guns appropriately distinct source position, direction, scale and smoke behavior. Use the actual gun/caliber rather than a shared 76 mm assumption. **U/G**
- [ ] **GUN-02 — Brief, conditional flash.** Tune flash visibility for weapon, shot and lighting. References support smoke-dominant shots and some bright bursts; neither permanent orange flame nor an absolute no-flash rule is justified. **U/O/G/Q**
- [ ] **GUN-03 — One emission event.** Drive visible muzzle emission and its scene light from the same discharge envelope and spectrum; remove the independent legacy grain/light timing. **G/P**
- [ ] **GUN-04 — Fast, directional cloud expansion.** Form a compact initial discharge that rapidly expands forward and sideways, including the backward/lateral spread visible around the gun. Avoid an evenly growing spherical puff. **O**
- [ ] **GUN-05 — Uneven billowing and dissipation.** Preserve dense lobes, thin edges, drift and gradual breakup after the initial expansion. Smoke must obscure and reveal the background over time. Give propellant, exhaust and fire soot their own source locations and lifecycles. **O/P**
- [ ] **GUN-06 — Lighting-dependent smoke tone.** Let scattering, absorption, density, lighting and shadow produce pale or dark-looking regions. The monochrome GIFs do not establish one universal white smoke color. **O/Q**
- [ ] **GUN-07 — Ground disturbance.** Distinguish airborne propellant from low dust, snow or loose material disturbed near the vehicle. Select the response from the actual surface and snow conditions. **O/U**
- [ ] **GUN-08 — Smoke affects sightlines.** Check that the visible cloud's obstruction is respected consistently by optics, player visibility and AI perception; it should have a gameplay consequence where optically dense. **O/U/P**

## 2. Shell flight and impact — 6 items

- [ ] **HIT-01 — Three readable stages.** Keep the projectile/tracer, terminal impact flash/cloud and flying fragments visually distinct, as requested with the earlier M2 reference. **U**
- [ ] **HIT-02 — Moving rounds and tracers.** Preserve actual projectile flight, readable direction and appropriate tracer cadence/length; effects should follow the projectile rather than form persistent beams. **U/P**
- [ ] **HIT-03 — Armor-surface origin.** Start impact sparks, debris, smoke and contact sound at the resolved contact surface and orient them using the strike direction/surface normal. **U/P**
- [ ] **HIT-04 — Fine, numerous sparks.** Use many small sparks with varied trajectories, velocity, cooling and lifetime. Preserve their considerable travel relative to vehicle size without enlarging each spark into a block. **U/P**
- [ ] **HIT-05 — Outcome and material differences.** Distinguish small-arms strikes, ricochets, AP penetration, explosive-shell bursts, earth, snow, wood and masonry contacts. A penetration or a kill must not automatically look like the largest explosion. **U/G/P**
- [ ] **HIT-06 — Impact color and scale.** Tune hot yellow/orange emission and the brief bright core against the earlier reference; separate it from non-emissive dust and soot. Scale the overall shower/cloud independently from individual particle size. **U/P**

## 3. Fire, ammunition burnout and destruction — 8 items

- [ ] **FIRE-01 — Damage-attached surface fire.** Sustain fire across relevant horizontal and vertical surfaces, with movement and changing intensity tied to burning material rather than a single central flame. Attached effects must follow detached moving parts and their light must cease when extinguished. **U/P**
- [ ] **FIRE-02 — Fire-to-smoke transition.** Make flame, embers and smoke evolve continuously, sharing motion, grain scale and update cadence so smoke does not read as a separate low-frame-rate object. **U/P**
- [ ] **FIRE-03 — Distinct event stages.** Separate initial ignition, rapid blast expansion, rising flame, rolling smoke cap and long-lived aftermath. Preserve the differences between an impact and sustained burning. **O/U**
- [ ] **FIRE-04 — Ammunition burnout.** Add sustained vented burning where ammunition is involved, with direction/escape paths grounded in actual openings and the vehicle's damage state. **U/G**
- [ ] **FIRE-05 — Finite fuel and ammunition.** Drive duration, intensity and secondary events from remaining fuel/ammunition and heat exposure; retain burnout/extinguishing and avoid repeatedly consuming or detonating the same inventory. **U/G/P**
- [ ] **FIRE-06 — Varied loss outcomes.** Allow disabled/knocked-out, burning, delayed cook-off and catastrophic outcomes to differ. Replace the generic blast on every normal enemy kill and chance-only catastrophe selection with causal state where practical. **G/U**
- [ ] **FIRE-07 — Credible turret detachment.** Model detachment, mass/inertia, impulse, rotation, collisions with terrain/vehicles/world objects and persistent settling, including any delayed catastrophe. Review player Tiger and enemy paths separately. Do not adopt the estimated 200-foot toss as a measurement or set arbitrary launch speed. **U/G/Q**
- [ ] **FIRE-08 — Burning environment.** Let damaged buildings and other appropriate sources contribute separate fires and smoke columns, so the settlement and background evolve along with the vehicles. **O/U**

## 4. Combat and vehicle audio — 12 items

- [ ] **AUD-01 — Tiger/T-34/gun identity and variation.** Establish distinct convincing dry reports with a strong onset, body and decay; fix Tiger variants resolving to the same generic recording. Retain charge/directivity and interior/exterior treatment. **U/G**
- [ ] **AUD-02 — Distant firing and impacts.** Hear both the gun event and the actual terminal event with their respective travel times. Preserve shell flight plus sound propagation; cover every vehicle/gun route and avoid duplicate explosions for one contact. **U/G/P**
- [ ] **AUD-03 — MG and rifle character.** Give rapid gunfire appropriate attack, body and mechanism detail. Retain per-round events and distinct rifle/MG identities; avoid weak pellet-like ticks or a continuous noise loop. **U/P**
- [ ] **AUD-04 — Small arms hitting armor.** Make exterior strikes/ricochets and interior structure-borne knocks perceptually clear and location-aware, synchronized with actual contacts/sparks. Preserve immediate own-hull vibration versus delayed airborne arrival. **U/P**
- [ ] **AUD-05 — Round passage.** Distinguish firing report, projectile passage/crack or whistle, and impact; vary passage with projectile characteristics and trajectory. Preserve the existing flight model rather than attach the same whistle to every bullet. **U/P**
- [ ] **AUD-06 — Extraction, ram and breech audio.** Start scraping/grinding while the part moves, including stops/reversals; use separate release, seating and locking contacts. Replace mismatched handling proxies. **U/G**
- [ ] **AUD-07 — Case and cartridge contact.** Give empty cases their own ring, clank, rebounds and settling/rolling; keep heavy live rounds distinct. Replace hatch-stop samples/fixed first-bounce sound with bounded location/energy-driven contacts. **U/G**
- [ ] **AUD-08 — Track clatter, grind and squeal.** Tie link cadence to actual belt travel and intermittent friction to contact/load/turning. Separate enemy track noise from engine RPM and silence travel clatter at rest; preserve powered broken-track behavior. **U/G/P**
- [ ] **AUD-09 — Engine and drivetrain.** Make own/enemy idle, throttle, load, hill climbing, gear changes and coasting readable. Retain authoritative powertrain input and a strong low-frequency body with changing upper detail; do not derive everything from vehicle speed. **U/G/M/P**
- [ ] **AUD-10 — Fire and aftermath.** Distinguish ignition, sustained roar/crackle, actual ammunition events and fading fire. Evolve sound with the finite source and distance without restarting ignition or inventing explosions. **U/G/P**
- [ ] **AUD-11 — Listening position and atmosphere.** Review exterior, exposed cupola, open hatch and closed cabin, including other tanks and background battle. Preserve barrier/headset filtering and propagation; balance wind/blizzard against machinery and avoid stacking recorded reverberation tails. **U/P**
- [ ] **AUD-12 — Dynamic range and continuity.** Let catastrophes outweigh minor impacts while machinery remains audible between shots. Check limiter behavior, missing events, voice stealing, start/stop clicks and input-key pops in actual gameplay. **U/P/Q**

## 5. Vehicle and loading motion — 8 items

- [ ] **MECH-01 — Whole-vehicle firing response.** Feed recoil impulse into a supported hull response shared by visible geometry and collision, accounting for gun direction, mass and ground support. Couple camera motion to the vehicle/player state rather than add unrelated wobble. **U/G**
- [ ] **MECH-02 — Barrel recoil and recovery.** Coordinate fast recoil, recuperation and settling with the discharge and mechanism contacts. Preserve existing barrel geometry/animation while validating its stroke/timing against suitable evidence. **U/P/Q**
- [ ] **MECH-03 — Moving running gear.** Make track belts and road wheels move consistently with travel and steering; preserve contact with the ground and snow. The authored Tiger belt currently does not circulate. **O/G/P**
- [ ] **MECH-04 — Terrain and suspension response.** Give the hull/running gear weight and controlled settling on uneven ground; keep minor crushable obstacles from creating excessive camera motion. **U/O/P**
- [ ] **MECH-05 — Traverse and elevation mechanisms.** Keep audible and visible movement tied to actual traverse/elevation, including starting, stopping and load, and audible between shots. **U/P**
- [ ] **MECH-06 — Bounded manual ramming.** Separate the player's gesture from actual cartridge movement so a fast input cannot snap the round instantly into the chamber. Coordinate with AUD-06. **U/G**
- [ ] **MECH-07 — Extraction and hot cases.** Preserve the real bore/extraction path, then transition to carried/dropped physics. Tie trapped fired-round gas to opening the breech, with no repeated gas from working an empty block. Add appropriate localized hot-case smoke/haze without filling the cabin with exterior weather fog. **U/P**
- [ ] **MECH-08 — Coherent loading sequence.** Keep manual and assisted extraction, insertion, seating and breech-state changes visually continuous, with consistent contact points and finished/interrupted states. Preserve real empty-chamber, alignment, held-round, seating, breech-closure and firing-readiness gates. **U/G**

## 6. Landscape and seasonal composition — 12 items

- [ ] **LAND-01 — Sparse and dense winter grass.** Mix short pale stubble with selected taller, full-density dead-grass beds and bare snow. Preserve fixed roots, roads/yards and deep-snow exclusions. **U/O/G**
- [ ] **LAND-02 — Field-scale vegetation structure.** Arrange open fields, dense scrub, copses and woodland edges as large coherent areas rather than distribute foliage uniformly. **U/O**
- [ ] **LAND-03 — Tree and plant diversity.** Improve height, silhouette, spacing and stand composition using appropriate bare trees, shrubs and evergreens. Audit arrangement before increasing the existing tree count. **U/O/P**
- [ ] **LAND-04 — Traffic and ground history.** Add convincing dark ruts, disturbed snow, worn roads and exposed earth; keep newly made track trails matched to actual track width. **U/O/P**
- [ ] **LAND-05 — Snow variation and vehicle contact.** Vary cover with local terrain/exposure and preserve shallow track seating, track snow pickup/shedding and small snow contacts. Honor the user's narrower request rather than add a blanket of snow over every tank top. **U/O/P**
- [ ] **LAND-06 — Settlement character.** Review building silhouettes, gardens/boundaries, damage, rubble and nearby vegetation against the winter/urban footage; ground buildings, walls and wrecks correctly on slopes. **U/O/P**
- [ ] **LAND-07 — Spring/thaw treatment.** Develop a distinct transitional landscape with appropriate patchy snow, exposed/damp ground and surviving vegetation, where the references support it. Verify season identity rather than assign it from montage captions. **U/Q**
- [ ] **LAND-08 — Summer treatment.** Use fuller grass/crops, leafy vegetation and different ground coverage for the non-snow maps. Preserve open sightlines and dense field margins; do not transfer summer green ground into winter. **U/O**
- [ ] **LAND-09 — Depth, weather and battle atmosphere.** Coordinate foreground detail, distant treelines, atmospheric falloff, wind, local smoke and existing snow/blizzard/fog so they read as one environment. Preserve seasonal and day/night differences. **U/O/P**
- [ ] **LAND-10 — Gold, green and violet palette.** Use the user's new colorized photo as the art target for green maps: golden sunlit/dry grass, olive and sage vegetation, restrained violet-magenta accents, warm masonry and pale creamy smoke. Treat this as an intentional aesthetic choice, not calibrated historical color. **U/O**
- [ ] **LAND-11 — Local flower and vegetation patches.** Place small purple flowering accents among mixed green and dry growth, especially around suitable edges and disturbed ground. Keep recognizable clumps and sparse accents at the native pixel resolution rather than making the entire field a purple tint. Verify regional/species choices where needed. **U/O/Q**
- [ ] **LAND-12 — Warm light and shaded depth.** Reproduce the reference's gold highlights, deep green/neutral foreground shade, weathered red-brown brick, pale plaster and bright distant haze through materials and plausible lighting. Retain texture in highlights and useful shadow detail; avoid applying a uniform yellow filter. **U/O**

## 7. Exterior and interior materials — 10 items

- [ ] **MAT-01 — Worn winter whitewash.** Add an irregular seasonal light paint layer to appropriate Tiger/T-34 exterior surfaces, with base color showing through. Keep it distinct from snow and protect interior, optics, tracks, cables and exhaust identities. **U/O/G/Q**
- [ ] **MAT-02 — Exterior wear and dirt.** Place dirt, soot, scuffing and exposed material according to contact and damage; preserve readable dark running gear and openings. **O/U**
- [ ] **MAT-03 — Warm ivory structure.** Give appropriate interior walls, structural parts and ring surfaces a warm light-painted material, avoiding a uniformly grey cabin. **U/O/Q**
- [ ] **MAT-04 — Dark green/blue-grey equipment.** Separate selected mechanisms and equipment from the light structure through component-specific paint, verified where historical fidelity matters. **U/O/Q**
- [ ] **MAT-05 — Black fittings and padding.** Distinguish painted fittings, rubber, optical housings and padded surfaces; do not bake every shadowed recess into black albedo. **U/O/Q**
- [ ] **MAT-06 — Red controls.** Include restrained red accents on the appropriate handwheel/controls, after checking component identity and historical paint evidence. **U/O/Q**
- [ ] **MAT-07 — Exposed working metal.** Distinguish steel rails, breech/contact faces and polished wear from painted parts using roughness and reflectance as well as color. **U/O**
- [ ] **MAT-08 — Cartridges and cases.** Give appropriate case/projectile components distinct metal/paint materials, with heat/handling effects where justified. Verify ammunition color conventions instead of copying model paint unquestioningly. **U/O/Q**
- [ ] **MAT-09 — Floors and lower compartments.** Review floor/grating, darker lower regions and reddish panel treatment from the cutaway with appropriate material and historical checks. **O/Q**
- [ ] **MAT-10 — Localized material wear.** Add restrained chipped paint/contact shine and variation at handled surfaces; preserve distinct paint, metal and padding response under the same light. **O/U**

## 8. Infantry and close threats — 4 items

- [ ] **INF-01 — Visible groups and combined presence.** Build on squad behavior so infantry appears in coherent groups alongside vehicles, among buildings and across fields. Preserve more infantry/AT action between main-gun shots. **U/O/P**
- [ ] **INF-02 — Cover and concealment.** Use the actual vegetation, buildings, wrecks and smoke for movement and observation. Ensure concealment has consistent player/AI effects without giving AI knowledge through dense smoke. **U/O/P**
- [ ] **INF-03 — Limited incendiary-bottle role.** Add finite carried supply, identifiable preparation/release and a physical thrown object for suitable equipped infantry. Treat availability as a scenario/loadout decision, not universal equipment. **U/Q**
- [ ] **INF-04 — Incendiary consequences.** Resolve contact into finite localized surface fire, smoke, exposed-person risk and component damage through the shared fire/heat/occlusion model. Maintain armor/interior separation, meaningful suppression/extinguishing and ammunition-removal consequences, and avoid a guaranteed tank explosion on contact. **U/P**

## 9. Rendering and simulation constraints — 6 items

- [ ] **PRES-01 — Native pixel presentation.** Keep the agreed 800x450 look, ordered dithering aligned with the internal grid and crisp final scaling. Retain adjustable resolution. **U/P**
- [ ] **PRES-02 — Fine effect grain.** Keep fire, smoke and spark grain approximately pixel-sized across resolutions; gain volume/detail through density and distribution rather than giant cubes. **U/P**
- [ ] **PRES-03 — Real-time coherence.** Advance fire, smoke, particles, lighting, motion and audio against consistent simulation time. Avoid separate apparent update rates or lost elapsed time. Rendering quality settings must not change physical outcomes. **U/P**
- [ ] **PRES-04 — Moving-image stability.** Test dense stems, branches, buildings, tanks and fine effects in motion with existing geometry AA/dither; avoid reviving shimmer or smearing the chosen pixel look. **U/P**
- [ ] **PRES-05 — Physical lighting and emission.** Preserve armor/building shadows, source color, exposure and restrained bloom, including source-colored glow through viewports. Match light to its effect and audit occlusion rather than introduce lighting bypasses. **U/P**
- [ ] **PRES-06 — Closed-cabin atmosphere.** Maintain separate interior/exterior media and plausible local light. The roofless model photograph is a palette reference, not a target exposure for a closed tank. Preserve dark-night readability through actual lights. **U/O/P**

## 10. Acceptance and evidence — 6 items

- [ ] **VERIFY-01 — Matched firing review.** Capture Tiger, T-34 and AT firing from exterior, cupola and cabin, at near/far distances and relevant day/night/weather conditions. Include flight, actual impact and sound arrival in the same sequence. **P/Q**
- [ ] **VERIFY-02 — Matched movement/mechanism review.** Capture idle, acceleration, hill load, turns, gear changes, stopping, broken-track drive attempts, traverse/elevation and interrupted/full reloads. Verify visual motion and audio synchronization. **P/Q**
- [ ] **VERIFY-03 — Damage outcome review.** Compare non-catastrophic hits/losses, surface fire, ammunition burnout, delayed events and rare large catastrophes. Record the causal damage/ammunition state; distinguish staged fixtures from gameplay outcomes. **P/Q**
- [ ] **VERIFY-04 — Seasonal/material review.** Capture winter first, then spring/summer comparisons, with moving cameras and open/closed hatches. Check foliage composition, track seating, exterior-only whitewash and interior material separation. **P/Q**
- [ ] **VERIFY-05 — Actual listening and performance.** Audition matched game/reference events and live device output; measure frame timing, particle cost, clipping, limiting and event coverage. Signal spectra or encoded video FPS alone are not sound-quality or live-performance acceptance. **P/Q**
- [ ] **VERIFY-06 — Reference calibration and provenance.** Verify uncertain gun/vehicle identity, paint details, edit/retiming and source rights before treating footage as calibrated or copying audio into assets. Preserve originals and use owned/verified recordings or authored synthesis for shipping. **P/Q**

## Source map

The detailed observations, timestamps and code audit are in
[REFERENCE_STUDY_20261002.md](/home/alex/panzer/canyon-v2-20260930/docs/REFERENCE_STUDY_20261002.md).
The current batch contains 18 supplied files; the earlier impact video/still and
earlier renderer look decisions are carried forward separately.

| Reference group | Files / useful content | Principal checklist areas |
| --- | --- | --- |
| Winter village and fields | `ScreenRecording_10-02-2026 17-55-14_1.MP4`, `17-38-55_1.MP4`; overlapping sequences, not independent examples | GUN, LAND, MAT-01, INF, FIRE-08 |
| Thin snow/stubble | `17-49-50_1.MP4` | LAND-01/04/05/06 |
| Tiger/weapon montage | `17-51-07_1.MP4`, `tiger1 clip2.mp4`; some repeated events | GUN, MECH, FIRE, AUD; identity/timing limits |
| Urban battle | `17-53-40_1.MP4` | LAND-06, FIRE, HIT |
| Mixed countryside/urban sequences | `17-43-35_1.MP4`, `17-42-21_1.MP4` | LAND, INF, FIRE-08 |
| Fuller non-snow vegetation / mixed countryside | `17-40-58_1.MP4`, `17-40-07_1.MP4`, `17-37-51_1.MP4` | LAND-02/03/07/08, INF, GUN |
| Museum sound | `/home/alex/Recordings/tiger1_tank_museum_sound.wav`; user highlights engine, tracks, MG and armor strikes | AUD-03/04/08/09/11/12 |
| Modern drive-by | `/home/alex/Downloads/tiger1 sound 3.mp4`; several edited views, almost-mono audio | AUD-08/09, MECH-03/04; signal-only assistant analysis |
| Three firing GIFs | `xd7171hgyrnb1.gif`, `b5p8vwbgyrnb1.gif`, `1ckdg18gyrnb1.gif`; all 125 frames inspected | GUN, MECH-02; no sound, original camera speed unknown |
| Interior cutaway | `codex-clipboard-e0e7b2fc-3480-4b1f-bae2-5d1db9550da8.png`; appears to be a painted scale model | MAT-03 through MAT-10, PRES-06 |
| Green-map palette photo | `/home/alex/Downloads/myzrv4w4n3w31.jpg`; visibly credited as colorized by Julius Jääskeläinen; user-selected gold/green/purple art direction | LAND-08/10/11/12, MAT-02 |
| Earlier impact reference | `/home/alex/Downloads/m2-res_720p.mp4` and the previously supplied `Screenshot_20261001_131949.png` | HIT, FIRE, AUD-02/05/12; user-described targets carried forward, not newly re-reviewed here |
| Earlier look decisions | Obra Dinn/dither discussion, deltaP comparison, prior game videos | PRES; established constraints rather than new footage evidence |

Full paths for the ten phone recordings and review artifacts are indexed in
`artifacts/runtime/instagram_references_20261002.json`. Additional MP4/GIF hashes
are in `artifacts/runtime/tiger_additional_references_20261002.json`, frame timing
in `tiger_firing_ref_20261002_gif_manifest.json`, and sound measurements in
`tiger_audio_reference_analysis_20261002.json`.

The cutaway image has an unchanged preserved copy at
`artifacts/reference/tiger_cutaway_palette_20261002.png`. The older M2 video still
exists; its original temporary screenshot path no longer exists locally, though
the user-supplied image remains conversation context.

The green-map image is preserved unchanged at
`artifacts/reference/green_map_gold_olive_violet_20261002.jpg`, with its visible
credit retained and a checksum/provenance record in
`artifacts/runtime/green_map_palette_reference_20261002.json`.

## Recommended implementation order

1. Fix known event/source errors: AUD-01/02/06/07/08, MECH-06/08 and GUN-03.
2. Review the core sensory loop: firing, flight, impact, recoil, engine/track
   movement, native-sized particles and inside/outside audio.
3. Improve winter composition and component materials, preserving pixel stability.
4. Extend causal recoil/burnout/destruction and infantry incendiaries with the
   necessary physical state and representative game captures.
5. Broaden to spring/summer and complete the matched acceptance set.

This list records work and acceptance targets. It does not mark the reference
pass itself as a new game build or claim the pending items are implemented.
