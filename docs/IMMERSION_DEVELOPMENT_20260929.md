# Player-view immersion development — 29 September 2026

Continuation of `HANDOFF_20260929.md`, starting at `d7947c0` on the unpushed
`claude/sound-overhaul-20260928` line. This is a working direction and acceptance plan;
it does not promote planned systems into implemented features.

The owner's direction: use HAL's RTX 4090, i9-12900K and 128 GiB RAM for a more
emotionally forceful modern PSX image and sound. Preserve the visible pixel structure,
material character and wordless anti-war storytelling. Evaluate changes in distinct
combat encounters from the player's position with synchronized game sound. Expand
surface fire and suppression, the blood and gore caused by actual penetrating contacts,
and armor penetration through individual fragments, trajectories and deformation.

## Order and review loop

1. Real-window play and front-end flow. Record a failure before adjusting balance around it.
2. Act 1 human survivability and the audiovisual consequences of its encounters.
3. Maps 2 and 3 after the first level's interaction and combat gates pass.

Audio, graphics and damage engineering can be audited in parallel. A change is accepted
through the same encounter before and after, at the same seed, viewpoint and output gain.
Keep the original binary/receipt and a normal-speed synchronized film. Simulation-only
tests, offscreen films, live input and human listening are separate evidence columns.
Slow motion and contact/trajectory views may diagnose a problem, but normal player view
determines whether it is legible and emotionally effective.

## What the starting baseline actually does

This describes the handed-off baseline, before the bounded changes below. It is not a claim
that the current working source still has every baseline defect.

- The current launcher forces the shader backend and a 1280 × 720 internal frame.
  The High preset there does not enable hardware ray queries. The separate physical
  High backend has a traced secondary bounce, not a full camera path tracer.
- Shader environment visibility samples retained scene geometry in a small local
  atlas. Distant receivers fall outside it. Better city shadowing needs a coverage and
  convergence change with frame-time evidence; another screen-space darkening effect
  does not solve the missing visibility.
- Sound has calibrated source estimates, propagation, a listener/ear model and a master
  limiter. These do not establish that the source waveforms sound like a Tiger.
  The own-engine fallback pitches a premixed synthetic loop, including its structural
  response, with RPM. The selected own-gun recordings remain owner-reviewed inputs.
- Penetrations already generate one stable-ID fragment population. Damage forecasts its
  clones immediately; rendering/contact audio later advances the live pool separately.
  Their initial distribution is authored and capped; interior collision is still a
  box/cylinder/breech proxy. This is not one online consequence authority or material fracture.
- Armor scars already weaken later hits locally. They do not displace or perforate the
  rendered plate mesh; calling them geometry deformation would be inaccurate.
- Surface fire and foam use retained source triangles. Heat, spread and suppression
  coefficients are authored; there is no calibrated fuel/oxygen solver and the
  extinguisher currently has unlimited discharge.
- Blood already has an eightfold visual grain multiplier and an 8,192-grain pool.
  More particles require collision/deposit capacity and cost measurements, not another
  damage multiplier. Existing tissue and instantaneous-lethality rules need separate work.

## Implemented in this bounded pass; acceptance still pending

The own-engine fallback is now continuous: RPM changes the combustion arrival clock while
the inherited structure filters and decay times remain fixed in Hz/seconds. Armor upper modes
use frequency-dependent estimated structural loss. Confirmed internal filler/ammunition
outcomes select a finite broadband explosion response; shell class or casualty count alone
cannot invent one. The limiter release precision defect and reduction telemetry are corrected.
Source-level estimates, hearing/reflex law and selected own-gun recordings remain unchanged.
See [the sound implementation and measurement record](SOUND_MASS_20260929.md), including the
dry penetration's measured 3.70 dB output-LAF decrease and deeper limiting that still need
normal player-view listening. Passing calibration relations is not subjective acceptance.

The recorder now suppresses gameplay callbacks after terminal state and rejects required
postmortem actions explicitly. The old 34-second duel's two-shot success was invalid: its
second staged shot ran at 27 seconds after KIA at 18.35 seconds. Original artifacts remain
preserved and [labelled with that defect](../artifacts/immersion-20260929/duel-before-recorder-defect.md).
A separate 20-second fixture keeps the shared encounter and actual fatal outcome without the
postmortem shot. The final 20-second film passes seven state checks and required actions,
with the same actual ammunition catastrophe at 18.35 s. Already-emitted acoustic arrivals
now continue through the existing death fade in both native play and capture; this also
restores the previously lost fire-cookoff voice. See the sound and fire evidence records.

These changes do not implement new graphics transport, finite extinguisher charge, online
source-triangle fragment damage, conserved fracture, plastic deformation or additional gore
simulation. Current fire-pair fixtures test disclosed source ignition, ordinary LMB pickup,
aimed foam and wrong-aim response; a tuned fixture or passing capture is not a new fire solver.

## Coupled damage target

One resolved contact should own all its consequences:

`projectile flight → plate contact → plate response → retained penetrator/fragments`

`→ swept contacts → deposited work → component/wound/fire state → image and sound`

The first prerequisite is one online contact progression: gameplay consequences must consume
the bodies advancing now rather than a frozen-scene forecast of the next 3.6 seconds. Then
replace proxy contacts so a fragment stops or deflects on the same interior surfaces the player
sees. Damage, rendered motion and contact sound must consume that path. Retain fragment IDs,
generation seed, initial position, mass, velocity and
contact time/position/normal. Expose bounded diagnostic receipts rather than manufacturing
a separate attractive shower for the camera.

Subsequent armor work needs an explicit shared plate representation. Local dents,
perforations and detached pieces must change the collision surface and visible surface
together, survive repeated impacts, and preserve a declared material/energy budget.
A bounded deformable patch is a possible implementation step; it is not validated
continuum fracture or FEM. Material laws, mesh resolution, time step, conservation error
and comparison evidence must be stated before accuracy claims. Presentation presets
may change sampling and rendering cost, not the outcome of the hit.

The detailed [armor and fragment development plan](ARMOR_FRAGMENT_DEVELOPMENT_20260929.md)
records current implementation versus stale ledger descriptions, finite-cube SAT/BVH reuse,
inner-face release and entry-perforation hazards, shared collision/visible/RT geometry revisions,
performance bounds and phased acceptance. It is a proposed migration, not implemented damage
work from this sound pass. Preserve its old reference fixtures and failed receipts as each
phase gains new evidence.

## Encounter review set

| Encounter | What it should reveal | Existing starting fixture |
|---|---|---|
| Buttoned cupola, T-34 duel | Hull mass, incoming strike direction, own gun, actual fatal outcome and fade | `pilot_cupola_t34_duel_terminal_20260929.v1.json`; original 34-second baseline preserved with recorder defect |
| Street ambush | Threat through windows/slits, distance and occlusion, readable cover | `pilot_cupola_street_ambush_20260929.v1.json` |
| Head-out → buttoned firing | Distinct exterior/interior spectra and ear consequences | `sound_calibration_20260929.v1.json` |
| Canyon contact | Long-range reports, echo/occlusion limitations, track damage | `canyon_fight_20260929.v1.json` |
| Loader fights an actual surface fire | Physical pickup, aimed foam, wrong-aim failure, persistent coating | `fire_suppression_aimed_lmb_20260929.v1.json` and `fire_suppression_wrong_aim_lmb_20260929.v1.json`; wrong-aim checks pass, aimed fire is extinguished but prior cartridge ignition still kills the crew |
| Matched plate strikes | Bounce/through-hit, retained fragment path, occluded crew/components, repeated-hit scar | Author after collision gate |

The named fixtures are under `catalog/shader_reviews/`. Their staged setup and
`staged_discharge` actions are disclosed in their metadata; they are evaluation
fixtures, not proof that a human completed the input loop.

## Acceptance details

- **Sound:** compare source spectrum and attack/body/tail, enclosure perspective,
  meaningful limiter-reduction duration and output peak. Preserve physical source-level
  estimates and prior hearing decisions unless a specific implementation defect is found.
  No pre-impact duck or nonphysical warning of a supersonic shell.
- **Fragments:** earliest retained-surface contact wins; a protected target cannot be
  damaged through it. High-speed sweeps must not tunnel, and identical initial states
  must give identical live and diagnostic paths. State finite-radius/mesh omissions.
- **Deformation:** repeated impacts consult the changed plate; a hole visible in one
  renderer is also a hole to collision and the other renderer. No decal-only claim.
- **Fire:** spray acts through deposited coverage, wrong aim fails, coating remains,
  and an already ignited cartridge cannot be undone by extinguishing a neighboring flame.
  Finite charge must be simulation state and survive stow/re-equip and persistence.
- **Blood:** request/admit/recycle/deposit counters reveal saturation; visual density
  changes cannot cause additional injuries. Retain source-surface contact and normal speed.
- **Performance:** measure actual frame time in the dense district and contact peaks.
  Spend the hardware on modeled phenomena, with explicit budgets and no silent changes
  to physical outcomes between presets.

## Initial live observation

The baseline opened on HAL. The owner reported it frozen, then confirmed it came up.
The same interval had the main thread building the district/renderer, low GPU utilization,
and no process failure. Startup needs a responsive loading presentation; this observation
does not certify the campaign/death/retry or manual combat loop.

The retained native-input log records actual station changes, two gun discharges before one
ammunition catastrophe, subsequent retry play, climbing and on-foot movement. It does not prove
a completed manual reload or save/load cycle. The user later reported apparently damaged traverse
and an unresponsive hammer. The input investigation and bounded fix are recorded in
`REPAIR_INTERACTION_20260929.md`; the particular surviving fault remains unconfirmed.

Visual inspection of the live courtyard and recorded cupola/fire views found bright, broadly
lit snow and clipped daylight ports, strong flame-driven cabin contrast, and clear persistent
surface coating. The shader near-field visibility atlas and only two shadowed cabin emitters
remain important coverage limits. These observations are not a new atmosphere implementation.
The next lighting work should resolve emitter visibility and scene coverage at matched inputs
before changing exposure or adding more emitters. Film rendering time is not an interactive
frame-time measurement, especially while other GPU work is running.

## Final integration evidence

The release builds and the final binary unit suite passes 1,403 tests, with 51 ignored.
Calibration passes 24 events and 54 relations. The terminal duel, hammer input and fire outcomes
have separate receipts and synchronized films in the local
[review page](../captures/astra/immersion_review_20260929.html). Source, measurement and known
integration-test limitations are recorded in [the sound report](SOUND_MASS_20260929.md).

Physical High and Laptop captures both complete on the RTX 4090 from the same initial scene,
time and viewpoint: `captures/astra/immersion_20260929_physical_high.png` and
`captures/astra/immersion_20260929_physical_laptop.png`. Internal sampling is 640×360 and 320×180
respectively. High constructed the ray-query geometry; both images were visually inspected.
Their different lighting/noise are observations, not physical parity or atmosphere approval.
These are frame-generation gates, not measured interactive performance.

Browser playback and comparison controls work. Subjective sound acceptance and the owner's
exact surviving traverse repair remain unverified. Act 1 balance, maps 2/3, online source-mesh
fragment contacts, deformation and further atmosphere work remain the subsequent sequence.
