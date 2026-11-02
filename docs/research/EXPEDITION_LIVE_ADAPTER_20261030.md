# Experimental expedition live adapter

This is the first opt-in bridge between the finite expedition roster and the existing physical
Game. The normal sandbox and authored campaign remain separate. It is a recovery playtest,
not the completed campaign or a complete suspend/resume implementation.

## Ownership

Game remains authoritative for the Tiger, crew bodies, wounds, combat, projectiles, terrain and
other world objects. At the end of each update, `expedition_runtime.rs` projects actual named
crew casualties, physical positions, Tiger pose, observed fuel, fire and existing component
faults into the coordinator. The adapter does not run a second field wound simulation.
`CampPatientsOnly` advances only admitted camp patients. A still-bound Game actor keeps the
same identity during evacuation. Admission transfers his physiology clock to camp and copies
the resulting casualty back to that actor, while the field time course skips him.

Person IDs persist independently of appearance slots and seat assignments. Interior standby
or walking people use an `Aboard` location, with their actual world position, rather than
claiming the same occupied seat twice. Historical ragdolls use their own parent transform;
extracted world-space bodies remain outside when the Tiger moves.

## Explicit succession

The front end offers a named camp reserve only after the controlled person's actual death.
Unconsciousness is not a free respawn, and a disabled Tiger does not kill a surviving person.
The caller pauses the world while presenting that choice.

Selecting a reserve binds a new actor to the controlled appearance slot, retains the old actor's
corpse and wounds, and places only the newcomer at camp. No Tiger, enemy, projectile or world
reset occurs. Personal foot, hearing and burn state is retained per retired person in memory.
The new person's actual saved casualty is imported, not replaced by a healthy default.

For this prototype each initial camp reserve receives one finite eight-round P08 magazine.
The initial commander retains the actual current ammunition. Other aboard crew do not receive
an invented reserve. Succession reads the selected person's stock; it never refills it. This
allocation is a gameplay fixture, not a historical equipment claim. Physical corpse-loot pickup
and complete personal loadout modeling remain outstanding.

## Public API

- `expedition_initial_camp_position() -> Result<Vec3>` searches actual terrain near the start
  for a locally level, unobstructed site outside the Tiger's footprint. It does not establish
  tactical safety, a travel route or a fully furnished camp. Failure requires an authored site.
- `enable_expedition(seed, camp_position)` binds the five original named crew, seeds finite
  personal ammunition once and starts the opt-in projection. It refuses a legacy/LAN overlay.
- `expedition_state()` and `expedition_status()` expose the coordinator and explicit live limits.
- `sync_expedition_now()` synchronizes without advancing time before a succession UI/proof.
- `expedition_succession_pending()` distinguishes confirmed death from unconsciousness.
- `expedition_successor_candidates()` lists capable camp reserves, with the medic last. When
  none remain, it offers capable already-deployed survivors. Existing actors are taken over at
  their actual post, with their wounds and actor IDs intact; they are not returned through camp.
- `expedition_take_successor(PersonId, &mut Renderer)` performs the explicit personal handover.

## Deliberate incomplete boundaries

Automatic disk saving is disabled. The durable coordinator format still explicitly declares
`live_game_adapter_complete=false`: complete enemy, projectile, armor and simulation-timer
snapshots are not yet adapted. In-memory succession preserves the actual running Game instead.
Boolean component functionality is copied truthfully as functional/disabled; it is not presented
as a new graded wear simulation.

The registered recovery halftrack is not yet a spawned, driving or towing vehicle. Exhausting camp reserves while crew
still survive is not reported as total personnel loss. The crew body's death adapter retains
the actual exterior ragdoll for a person killed on foot, rather than leaving a second corpse in
his former seat. These paths require live capture verification in addition to coordinator tests.

Compilation, CPU tests and a staged live succession capture are separate evidence. A successful
coordinator proof alone does not establish that recovery, towing or the complete campaign plays.

## Physical medical handoff

`expedition_medical_runtime.rs` identifies the actual installed stretcher under the view ray.
Admission requires a living dismounted helper, a held world-space wounded actor, that body's
arrival within 1.5 metres of the selected bed, and the helper within 2.8 metres of its attending
position. The broad interaction target follows the stretcher transform; existing source cover
blocks the hand ray. It is not collision geometry. Neither a tank seat nor a guessed position
can be remotely admitted.

The chosen bed owns exactly that actor. A rigid placement preserves the articulated joints and
wounds while laying the body above the canvas; the camp-owned body is held there during care.
The finite named medic performs the existing deterministic first-aid procedures with finite
supplies. The live coordinator chooses a patient while the medic walks, but no treatment step
or supply consumption occurs until his actual camp actor reaches that bed. Wounds keep advancing
during the approach. This is a gameplay treatment simulation, not a clinical model or full recovery system.

Removing the body by hand explicitly frees the bed. A stabilized person becomes a camp reserve
with lingering wounds; a critical patient resumes the field time course and can still die.
A corpse leaves the bed only through the same physical removal path. An already-bound discharged
patient uses same-actor reembodiment when selected, rather than the new-reserve allocation path.

Input API: `expedition_medical_target(eye, direction)` supplies the bed and contextual label;
`expedition_interact_medical(eye, direction)` attempts held-patient admission. Occupied beds
remain available to normal physical body pickup. `expedition_medical_status()` exposes actual
patient/bed assignments, procedure cursor and supplies for verification.
