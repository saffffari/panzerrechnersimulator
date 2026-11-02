# Expedition state agent handoff — 30 October 2026

The parent coordinator stopped on a remote compaction/connection error. This note preserves the
state-agent's latest work so the next turn can continue without recreating it. All source is in
the shared checkout; this agent has performed no Git operations or GPU launches. The parent
owns serial builds, tests and visual verification.

## Source delivered

- `src/expedition.rs`: finite 16-person named roster, one persistent Tiger, recovery equipment,
  eight preparation dispatches, idempotent dispatch/death/transfer/repair, persistent IDs.
- `src/expedition_save.rs`: version 2 coordinator save, checksum, validated atomic replacement
  and backup. Explicitly rejects version 1 aggregate health. The envelope remains an incomplete
  physical snapshot (`live_game_adapter_complete=false`). Never advertise full combat resume.
- `src/expedition_medical.rs`: actual `first_aid::Casualty`, two finite beds, named medic,
  deterministic triage/procedures, finite supplies, no instant healing/resurrection. Explicit
  exact-bed admission, field/camp authority handoff and live medic-arrival gate are now added.
- `src/first_aid.rs`: approved serde derives and validated exact `ProcedureRunSnapshot`, including
  its private cadence state. Medical equations were not replaced.
- `src/expedition_proof.rs`: CPU recovery/persistence proof plus separate medical proof fixture.
- `src/expedition_runtime.rs`: opt-in Game projection, real actor/person bindings, physical
  casualties and positions, fuel/fire/component faults, explicit finite reserve succession and
  fallback takeover of existing survivors. No Tiger/world/projectile reset on handover.
- `src/expedition_medical_runtime.rs`: physical stretcher targeting/admission, one physiology
  owner per patient, physical removal/discharge, exact named actor retained.
- `src/halftrack.rs`, `src/halftrack_capture.rs`, `tools/import_halftrack_prototype.py`: optional
  private licensed transport model, preserved articulated pivots, CPU inspection and GPU review
  harness. No driving/towing/capacity claims.

Narrow Game hooks register the expedition children, store optional runtime, tick it at the end
of update, and keep a living expedition soldier playable after the Tiger is disabled. The
parent owns main/frontend and the opt-in recovery menu. Crew-control and camp agents own their
related children and narrow Game hooks; do not overwrite their changes.

## Latest interfaces

`Game::enable_expedition(seed, camp_position)` binds the actual original five actors. The camp
position helper returns the installed camp's real root when available, otherwise searches
actual near-start terrain and fails clearly if no locally clear site is found.

`expedition_state`, `expedition_status`, `sync_expedition_now`,
`expedition_succession_pending`, `expedition_successor_candidates`, and
`expedition_take_successor(PersonId, &mut Renderer)` support the frontend. Candidates prioritize
capable camp reserves; if none exist, capable existing field crew are offered. Existing actors
retain their wounds and identity and are taken over in place. A bound discharged patient is
reembodied, not allocated as a new person. Each initial unbound camp reserve gets one finite
8-round P08 magazine once; this is a disclosed prototype allocation. Selecting again does not
refill it. Historical personal state and ammunition remain retained.

Medical input API:

- `expedition_medical_target(eye, direction) -> Option<(bed_id, static_label)>`
- `expedition_interact_medical(eye, direction) -> Result<bool>` before generic body pickup
- `expedition_admit_held_patient(bed_id) -> Result<PersonId>` for explicit proof/runtime use
- `expedition_medical_status()` for actual patient/procedure/supply receipts

The parent already wired the medical LMB handler in main. **The medical hover label still needs
verification/wiring:** the last source inspection found no call to `expedition_medical_target`
outside its input handler. Show its returned label during the ordinary hover pass.

Admission requires a living helper, an actually held living wounded world body, extraction from
the tank, arrival within 1.5 m of the selected physical stretcher, and helper distance <=2.8 m.
The bed placement API rigidly preserves articulated joints/wounds, rests the body above canvas,
and freezes only the admitted body. Taking a body by hand calls
`expedition_release_medical_actor`: critical patients resume field physiology, stabilized ones
become camp reserves, dead occupants free their bed only by explicit removal.

`expedition_camp_owns_casualty(role)` skips field first-aid only for admitted identities.
`expedition_camp_owns_actor(actor)` freezes their bed pose. The coordinator copies the treated
Casualty back into its bound live slot after advancing camp time; no parallel aggregate health.

The newest live care call is `advance_medical_to_with_bed_access`. It continues wound clocks and
chooses the triage target while the medic walks, but pauses treatment/supply consumption until
the camp agent's `Game::camp_medic_at_bed(id)` confirms actual arrival. Reachability is sampled
**before** sync temporarily takes the runtime out of Game, because the actor helper reads the
named medic via `expedition_state`. Preserve that ordering.

## Cross-agent dependencies already on disk

Crew agent APIs in `crew_body_recovery.rs`:
`crew_body_held_actor`, `place_detached_crew_body(actor, body_pose)`,
`reembody_singleplayer_person(PersonId)`, plus actual living extraction and retained exterior
corpses. `singleplayer_crew.rs` supplies new-person binding and existing-person control switching.
Living evacuation does not set `crew.ok=false`; only actual death does. Slot replacement still
requires the previous person to be dead: a living patient keeps his original slot.

Camp agent supplies `installed_camp_anchors()` and is finishing `camp_people.rs` Game hooks.
The named medic follows active_patient or public `Expedition::camp_triage()` to the real bed.
`camp_medic_at_bed` is on disk. Confirm its Game module/field/update hooks before compiling.
Camp actor tasks use existing PersonIds; they must not create anonymous additional reserves.

## Verification evidence and next pass

Parent previously reported 21 expedition/core/medical tests passing, 9 legacy Windows campaign
save tests passing, and all 4 halftrack tests passing. Workspace TEMP/TMP must be used for save
tests because Windows system-temp rename is blocked in this environment; do not weaken atomic
saving to work around that environmental failure.

Read-only inspection confirms `artifacts/expedition-20261030/compile-tests-09.log` ended in a
successful test compilation and `release-build-08.log` ended in successful release compilation.
These do **not** cover the newest medical bed-access/camp actor edits added afterwards.

`succession-review-v1.log` reports three distinct controlled deaths, idempotent losses, retained
Tiger, finite successor ammunition, and refusal to refill through reselecting. The parent made
flat captures `captures/expedition_succession_20261030_v1_*.png`. The camp review wrote six source
views and a JSON receipt. These files were generated by the parent, not this agent.

Next: wait for camp hooks, run the serial compile, then `cargo test expedition` with workspace
TEMP/TMP. New focused tests cover exact bed ID/nonmutation, field→camp→field wound ownership,
and no remote care/supply use while the medic travels. Run the crew-body and camp tests too.
Then stage a **living incapacitated** casualty for the live evacuation/medical proof; merely
wounded but still able crew are intentionally not pickup targets. Do not claim the live medical
loop is verified until that actual path and its visible results pass.

## Remaining design/runtime boundaries

No automatic complete Game save, full enemy/projectile resume, drivable recovery halftrack,
towing or finished strategic campaign is implemented by these modules. The existing Game is
preserved in memory during succession. Camp physiology currently uses a 100 ms fixed cadence.

If everyone alive is temporarily unconscious/in treatment, the death choice screen can have no
capable candidate without total personnel loss. The parent was notified: add a deliberate way
to wait for medical progress rather than declaring defeat or leaving time frozen indefinitely.

Source docs: `docs/research/EXPEDITION_LIVE_ADAPTER_20261030.md`,
`docs/research/EXPEDITION_MEDICAL_MODEL_20261030.md`, and
`docs/research/HALFTRACK_ASSET_ADMISSION_20261030.md`.

Private model root: `assets/vehicles/polyfork_halftrack_20261030/`. The admitted corrected GLB
SHA256 is `9a8ba1ed19f1e025c2203c2b31436fd15145e66ce1e024a7c08958948435557f`.
Do not use the preserved rejected linear-palette export. Do not commit assets or packages.
