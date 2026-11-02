# Expedition medical persistence and camp stabilization

30 October 2026. This is a coordinator and CPU acceptance slice. It does not establish a live
medical tent, carrying animation, ambulance transport, treatment interaction or campaign UI.

## One physiological authority

Every persistent `PersonId` now stores the existing `first_aid::Casualty`, replacing the first
development draft's aggregate `Health` field. Blood volume, every wound and care flag,
unconsciousness, concussion, morphine, airway state, tourniquet time, cause and record clock all
survive saving. Ability and displayed state are derived from that record. The finite roster has
16 people; its initial medic is the existing Gefreiter Schneider, PersonId 16, not a seventeenth
person. He can die or be reassigned, and can become the last viable successor.

All rates and thresholds remain the existing game's deterministic first-aid approximations.
The coordinator does not add an independent healing countdown. In particular, existing first
aid models clotting, treatment, concussion recovery and drug decay, but not blood regeneration,
surgery, bone union or burn healing. Stabilized discharge retains blood loss and lasting injury.

## Persistence boundary

Expedition saves are version **2**, normally `expedition.v2.json`. Version 1 is explicitly refused
with an explanation: its aggregate fields cannot reconstruct individual wounds or treatment
cadence. The old development save is preserved; silently inventing wounds is not a migration.

JSON encodes the default positive-infinite morphine interval as `never_dosed`. Other timer
values must be finite. Death causes use the three existing named values (`killed`, `bled out`,
`airway`), restored from a fixed whitelist without leaking strings to manufacture static lifetimes.
Unsupported causes fail validation until the schema is deliberately extended.

`first_aid::ProcedureRunSnapshot` saves and validates all procedure state, including the private
last wrap direction and time since the last turn. Completed steps have already consumed their
items. Restoring a half-finished procedure does not replenish supplies or charge completed steps
again. The normal save transaction installs changed state only after durable saving succeeds.

The save envelope still states that the complete live Game adapter is unfinished. It must not be
advertised as a complete mid-combat save of every projectile and simulation timer.

## Camp workflow

- Two beds, a finite existing `AidBox::FULL`, and one assigned, capable named camp medic.
- Admission needs a recorded on-foot arrival within 10 metres of camp. A person still assigned
  to a tank seat cannot be remotely admitted. The live caller must actually move/unload him.
- Patients occupy beds and cannot dispatch or take over a seat until removed or discharged.
  A dead occupant retains his bed until explicitly removed. Camp relocation requires those
  dead bed occupants to be cleared; it cannot silently transport or discard their bodies.
- The existing first-aid triage order chooses urgency; persistent identity breaks equal ties.
  The medic performs the original hands-on steps at deterministic cadence. Interrupted work
  retains its actual cursor/quality; shortages pause unavailable work. Camp uses issued supplies,
  so the field model's improvised belt/cloth assumptions do not become unlimited camp restocking.
- Discharge requires existing `Casualty.able()` and no outstanding indicated procedure. It does
  not replace the casualty with a healthy default. Slow residual clotting and injury can persist.

## Integration API

`import_casualty(person, &actual_casualty)` imports a live record, refuses clock reversal and
resurrection, and records newly dead people in the existing expedition event ledger.

`admit_to_camp(person, arrival)` owns a bed after physical arrival;
`camp_triage()` exposes the actual treatment order; `can_discharge`, `discharge_from_camp` and
`leave_medical_bed` support reassignment, withdrawal and body removal. `assign_camp_medic`
changes the finite roster assignment rather than creating a service NPC.

`advance_medical_to(absolute_seconds, scope)` uses 100 ms steps and an absolute saved clock.
Repeating an already-applied time does not spend another dressing or advance a wound again.
`AllPersonnel` is the standalone authority used by the CPU proof. `CampPatientsOnly` is intended
for the live adapter, where Game continues advancing field casualties and stops advancing a
person after admission. The adapter must transfer authority and the complete casualty record at
admission/discharge; running both clocks on one person would double his blood loss.

Use the existing save transaction for material medical changes. The advance API is bounded to
one hour per call; it does not advance the unrelated world clock or simulate offscreen combat.

## Acceptance evidence to run

`cargo test expedition` covers the coordinator, serialization and medical rules. The existing
`expedition-proof <new-receipt.json>` also writes a separate `.medical-state.json` fixture. It
compares two identical severe limb bleeds: early arrival receives a real tourniquet procedure,
whereas delayed evacuation dies under the unchanged first-aid time course. It saves during the
windlass step, reloads the exact cadence, checks finite supply consumption and permanent death,
and shows that the survivor's blood volume is still reduced. These are staged CPU positions,
not proof of live casualty transport or medical-tent gameplay.
