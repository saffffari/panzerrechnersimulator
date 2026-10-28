# Solo play with a living crew (branch `crew-solo`, 2026-10-27)

Singleplayer is no longer a one-man Tiger. Five men are aboard: the player is the commander,
and the gunner, driver, loader and radio operator are living crew who work their stations. The
player keeps them alive, and when one of them is killed his job is not automated away: the
player has to go to that station and do it by hand, with the body in the way until he moves it.

Owner's direction (2026-10-27): "instead of single person crew when playing singleplayer you
should still have full crew but then be responsible for patching up your crew if they get hurt.
each KIA crew member's role has to be performed manually from their station."

## What is in the build

Modules: `src/first_aid.rs` (wound time course and the first-aid box; pure, unit-tested),
`src/crew_solo.rs` (the crew's state and decisions, a child module of `game.rs`),
`src/crew_solo_capture.rs` (the headless proof, `crew-solo PREFIX`). `src/singleplayer_crew.rs`
now musters all five men instead of one. The hooks in `game.rs` are read-only on the crew's
decisions: the drive/steer authority, the gun lay and trigger, the breech service, the interior
hover, station entry, and the penetration path's wound record.

### 1. Solo = full crew

- `enable_singleplayer_crew()` (every solo launch, including the default Canyon v2 winter run)
  fills the five-man roster and switches the crew module on. The four AI men are visible from
  the player's seat at their posts through the existing crew figures (seated and standing
  static GLBs; there are still no animation clips, so a man at work is a posed figure).
- The driver follows the commander's orders: WASD from the cupola, the sights or the radio
  seat is an order, carried out after a reaction time and ramped at the driver's rate. From the
  interior walk WASD moves the player's body, so no order is given and the driver halts.
- The gunner lays on the nearest contact he can see from the eyepiece (`optical_los_clear`,
  within 1800 m) through the ordinary rate-limited turret and elevation drives, settles, and
  fires under the standing order. **K** toggles the standing order (fire at will / hold fire).
  Buttoned up at the cupola with nothing under the crosshair, **LMB** is the fire command: one
  shot when he is laid. He calls contacts and shots on the intercom.
- **Designation.** From the cupola, buttoned or head out, **MMB** (pad **RB**) designates what
  the view centre is on: a ray from the commander's eye to the first living contact (a 2.4 m
  sphere round its centre) or the terrain, out to 2.5 km. The hit distance is the range
  estimate; the bearing is the turret yaw that lays the gun on the point. A living gunner
  acknowledges ("Gunner: 3 o'clock, 250. Traversing.") and slews the turret there through the
  ordinary drive at the real powered or hand traverse rate, elevating for the range, holding
  fire, and reports "Gunner: on." A designated contact he can see becomes his target, under the
  standing order (hold fire means he lays and holds). Dropping into the sights mid-slew, the
  player finds the turret part-way there and takes over the lay. A second MMB on about the same
  bearing (within 3 degrees) refines the range; any other replaces it; it expires after 90 s.
  With the gunner dead the designation stands on the dial and the turret stays where it is.
  The gunner's turret position indicator shows it: a second, shorter pointer at the commander's
  bearing beside the turret's own pointer, the two lying together being the "on" cue. The
  session log carries the designation (bearing, range, contact, refinements, seconds to on).
- The loader serves the breech with the existing auto-crew stroke state machine at a human
  cadence (`LOADER_STROKE_S = 1.5 s` per stroke, three strokes of `RAM_PER_PRESS_OK` seat a
  round: about six seconds from the shot to the next round seated at full performance). While
  he works the round in he is holding it, so the off-station ram decay does not apply to him; a
  hurt or hampered loader is slower, never stalled. (Until 2026-10-28 the decay applied and a
  loader below about 0.55 performance, or with cases underfoot, could never seat a round.)
- **Empties.** After freeing the hot case the loader turns and stands it upright in the
  empties rack along the curved inner rear wall of the turret, slot by slot
  (`EMPTIES_RACK_SLOTS = 12`), and is away from the breech for 2.2 s / performance; the case
  is the real case geometry and rides the turret in its slot for the rest of the run, in view
  from the cupola and the sights. The slot stands are probed once from the turret's own
  interior triangles (a ray fan along the rear wall, the floor or shelf under each spot, the
  headroom above), so a slot exists only where a case can stand; the probe's count is printed
  and in the receipt. He lets the case fall to the floor instead when he is rushed (a contact
  under fire at will), hurt (performance under 0.7), or the rack is full ("Loader: rack's
  full."). The player as loader stows the same way: carry the case to the rack and let go
  within reach of an empty slot ("EMPTIES RACK - LET GO TO STAND THE CASE IN IT"); the floor
  and the throw stay as they were. `stowed_empties()` and `loose_empties_aboard()` are the
  counts the salvage and resupply paths read.
- **When the rack is full** the loader asks: "Loader: rack's full. Out the side hatch? (Y/N)",
  repeated every 30 s while unanswered. **Y** approves, **N** refuses; either can be changed
  later. Approved, he opens the turret's side escape hatch between reloads (breech loaded, no
  contact in the gunner's sight), heaves the empties out one by one through the aperture, rack
  first and then the loose ones off the floor (1.8 s a case / performance), and shuts the hatch
  when they are gone; a contact coming into the gunner's sight makes him shut it at once
  ("Loader: contact, hatch shut."). The open hatch is the real lid state: the cabin's sound
  becomes the open-hatch enclosure, daylight and weather come in through the real opening, and
  rifle fire resolves against the moved lid, so the aperture is a real hole for small arms.
  The cases land outside where the case physics puts them (deck, track guard or ground) and
  stay there. Refused or unanswered, the next empties fall to the floor and stay: loose cases
  round the loader's post slow his strokes (a quarter more per case, at most double), and the
  player's step over loose cases is slower (down to half) and kicks them along. As loader, the
  player has the same choice through the ordinary interactions: the side hatch lid (LMB) and
  throwing a carried case (X) through the open aperture.
- The player can take any station as before (sight anchor, breech, cupola, and now the
  **driver's seat** from the front of the basket floor). The man he displaces stands by at
  the rear of the basket floor, never under the cupola (the player is the commander; nobody
  stands in his place when he leaves it). Walking the compartment does not displace the
  loader: he keeps serving the breech until the player comes to the breech himself (within
  0.55 m of the loader's post, or with a round or case in hand). Every move in full-crew solo
  costs the real clamber (`switch_cost`: 0.55 s within the turret, 0.95 s turret to hull).

### 2. Wounds, not just death

`first_aid::Casualty` per man. Wounds come from the live penetration solver's anatomical
events (`CrewPartDamageEvent`: part, tissue outcome, fragment and blast work) through
`crew_solo_record_wounds`, and burns from the live cabin fire (`fire_active`, `fire`). The
model is deterministic.

| Wound | Source | Course | Treatment |
| --- | --- | --- | --- |
| Bleeding (head, trunk, arm, leg) | fragment injury, detachment, pulverization | litres per minute by region and severity^1.5; minor wounds clot over ~5 min; loss ≥15 % degrades, ≥33 % knocks out, ≥42 % kills | dressing (×0.15 limb, ×0.55 trunk, ×0.35 head); tourniquet stops a limb bleed |
| Fracture | high fragment work on a limb | performance ×0.45, ×0.70 splinted | splint |
| Burn | seconds in a burning compartment (hull seats ×1.0, turret ×0.6, head out ×0.15) | performance ×(1 − 0.5·severity), pain | dressing |
| Unconscious | head wound ≥0.35, blast work >150 J, blood loss | out for 20–80 s, then concussed (cap 0.7, recovering over 10 min) | treat the cause |
| Pain / morphine | untreated wounds | performance ×(1 − 0.35·pain); morphine removes pain, drowsy ×0.85, one dose per hour | morphine |

### The medkit in the hands (procedures)

Treatment is not one hold. Each wound kind has its own hands-on procedure, done at the man
from the interior walk in the game's object-interaction register: crosshair on him within
1.75 m (2.8 m down into the hull front for the driver and radio operator), the crosshair
names what is wrong and what the hands would do ("LOADER - ARTERIAL BLEED - TOURNIQUET [E]"),
**E** starts it, and from then on the mouse is the hands and the button is the button until the
job is done or the player steps away. The crosshair reads the current step. No bars.

| Procedure | Steps (button = E or LMB) | Kit |
| --- | --- | --- |
| Tourniquet (limb arterial bleed) | place above the wound [E]; turn the windlass [three presses, each given its half second]; secure and note the hour [hold 2 s] | Esmarch tourniquet; improvised: belt and wrench, five turns, capped at 0.6 |
| Pressure dressing (a bleeder) | cut the cloth away [hold 2 s]; pack [E]; wrap six times [mouse left and right in a cadence, 0.25-1.8 s a wrap]; tie off [hold 1.5 s] | Verbandpäckchen; improvised: a torn sleeve, capped at 0.6 |
| Splint (fracture) | draw the limb straight [hold 3 s]; lay the ladder splint [E]; bind four wraps; knot [hold 1 s] | ladder splint + triangular bandage; improvised: the cleaning-rod section, five wraps |
| Burn care | cut the charred cloth away [hold 2 s]; pour the canteen [E]; lay the burn dressing [E]; wrap loosely four times | burn dressing + canteen; a dry canteen is skipped and the care is poorer; improvised: a plain dressing |
| Chest seal (penetrating trunk wound) | open the tunic [hold 1.5 s]; the dressing's waxed wrapper over the hole [E]; hold it sealed as he breathes out [hold 3 s]; strap three wraps | a Verbandpäckchen's wrapper |
| Airway (unconscious man) | tilt the head back, clear the mouth [hold 1.5 s]; roll him onto his side [E]; watch him breathe [hands off 4 s] | nothing |
| Morphine | dose into the thigh [E]; mark him, M and the hour [hold 1.5 s] | morphine ampoule; one dose an hour |

Adaptive: the procedure offered is the one the wound wants (`procedure_needed`: airway first,
then tourniquet, chest seal, pressure dressing, burn care, splint, morphine). Rushed work
costs quality (a wrap outside the cadence, a windlass mashed, a hold let go early), a hit on
the tank mid-procedure jerks the hands (the step's work is lost, a wrap comes undone,
"Hit. Hands off him a moment."), and the result is as good as the work: a loose tourniquet
leaks half the bleed, a loose wrap holds less, an improvised job is capped. Stepping away
abandons the procedure with the items spent. The kit is finite and varied
(`AidBox::FULL`: 6 Verbandpäckchen, 2 Esmarch tourniquets, 3 morphine ampoules, 2 ladder
splints, 2 burn dressings, 3 triangular bandages, 2 canteens, scissors), restocks at the
same pickups as ammunition and fuel, and a dead soldier's own dressing packet is taken when
his body is looted (bounded at ten). Triage: when two or more men need care, the intercom
names the one nearest death first ("Two down. Loader first."); the order is the game, since
the slower bleed waits while the faster one is worked. An unconscious man left on his back
loses his airway after four minutes unless somebody positions him. The box (`AidBox::FULL`: 6 Verbandpäckchen, 2 Esmarch
tourniquets, 3 morphine ampoules, 2 ladder splints) is finite and restocks at the same
pickups that refill ammunition and fuel. The player treats his own wounds at the box on the
rear right turret wall. Treated men work degraded: a dressed arm still costs performance, and
a splinted man is at 70 %.

### 3. KIA means manual

- A man killed outright (the solver's fatal anatomy) or dead of his wounds is a ragdoll at his
  post, as before. His station goes dark: no lay, no trigger, no breech service, no driving.
- A dead driver with the player elsewhere means the tank stops whatever the commander orders.
- A man dead of his wounds folds where he sat (a gentle ragdoll collapse) and is logged
  "gone" on the intercom; a man killed outright is the solver's ragdoll as before.
- The sight anchor and the driver's seat refuse entry while the body is in the seat:
  "GUNNER DEAD IN THE SEAT - DRAG HIM OFF FIRST". The interior walk is never refused (it is
  the compartment, not a seat). Crosshair on the body, **hold E** 4 s drags it toward the
  rear of the basket floor (the settled articulated body is translated as one; see the
  ledger), after which the station can be entered.

### 4. Legibility

- The hit: the existing blood at the solved wound, then a bleeding man keeps bleeding where he
  sits (grains every half second in proportion to his rate, from the wound's region, through
  the surface-blood transport: a pool grows on the seat and the floor), and what is left in
  him runs out as he folds; the short red flash (`crew_ko_t`), the man
  slumping forward and sideways in his seat by his performance (`slump_pose`, a static pose),
  and silence or an intercom line in the terse register ("Loader: hit, leg — bleeding.",
  "Gunner: tank, 11 o'clock, 150.", "Loader: up.", "Driver: gone."). Lines show for 3.2 s
  above the interaction hint; `KESSEL_INTERCOM=0` silences them. No health bars.
- The commander's view of who is up is the compartment itself: look down from the cupola.

## Controls added

| Input | Action |
| --- | --- |
| **K** | standing order for the gunner: fire at will / hold fire |
| **Y / N** | answer a crewman's request (the loader asking to dump the empties out of the side hatch) |
| **MMB** (pad **RB**) at the cupola | designate the bearing under the view centre for the gunner; again on the same bearing refines the range |
| **LMB** buttoned at the cupola, nothing hovered | fire command |
| **WASD** from cupola / sights / radio seat | orders to the driver |
| **E** held on a wounded man, a body, or the first-aid box | treat / drag aside / self-aid |
| **LMB** on the driver's seat from the basket floor | take the driver's seat |

## Verified by play (headless, 2026-10-28)

Command, from the worktree, with the main checkout's asset library:

```bash
bash tools/crew_solo_proof.sh captures/crew_solo_20261028 C:/shop/panzerrechnersystem
```

Result: **all 20 checks pass** (shader backend, RTX 4090, act-0 arena). Receipt
`captures/crew_solo_20261028.json`; stills `captures/crew_solo_20261028_*.png`: `01` the crew
from the cupola, `02` after the gunner's first shot, `02b` an empty standing in the rear-wall
rack, `03` the hit crewman, `04_<procedure>_<man>` one per procedure and man (pressure
dressing on the loader and the radio operator, morphine on the loader, splint on the gunner,
airway and pressure dressing on the driver), `05` the dead gunner from the cupola, `06` the
player at the sights, `07` the driver dead and the tank halted, `08` the player driving.

Checks, in order:

1. five men aboard; gunner, driver and loader working their own posts; player at the cupola.
2. gunner lays on the T-34 and fires under fire-at-will.
3. loader serves the breech back to loaded at a human rate (3-12 s).
4. hold-fire holds the gunner; the commander's fire command releases one shot.
5. the loader stands the commanded shot's empty upright in the rear-wall rack.
6. driver moves the tank on the commander's forward order and halts on none.
7. the live penetration leaves at least one crewman wounded and alive to treat.
8. crosshair on the wounded man offers the procedure he needs, by name.
9. the first procedure is worked end to end by hand: steps in order, items taken, a good job, the bleed controlled.
10. three or more distinct procedures worked by hand, each with a still of the hands at it.
11. a minute later the treated man is alive, at his post, working degraded.
12. the driver's seat is reachable from the interior walk and the move costs the clamber.
13. gunner killed; nobody lays or fires the gun for ten seconds.
14. the dead gunner in the seat refuses the sights.
15. holding E at the body drags it off the seat.
16. the player takes the sights once the body is clear.
17. the handed-up round goes home: by the loader if he lives, by the player's own hands at the breech if he is dead.
18. the player lays and fires the gun himself (a loaded round leaves the breech); no AI shot is counted.
19. driver dead; the tank does not move on the commander's order.
20. the player drags the driver's body off, takes the seat, and drives the tank himself.

Fixture, stated in the receipt: clear weather; parked range targets that do not fire (the
88 kills a T-34 with one round, so a queue of them is rolled up, each placed where the gunner
can see it from the muzzle); the Tiger down to its last rounds, fired off before each strike,
because with a full stowage every lane tried into a seated man set off live rounds. The wound
is a live penetration: solid 76 mm APCR through the right hull side at the radio operator,
raked 35 degrees rearward, with the player up in the turret (a filled round's burst inside the
compartment killed every man near it, the player included, in earlier attempts). Its fragment
cloud reached three men; the proof treats the one with the fewest indicated wounds first, then
the next, then a staged fracture (named so) on the least-hurt man. The gunner and driver KIAs
are staged through the casualty ledger, ragdoll and blood path (`debug_crew_solo_kia`):
every live lane into those seats also wrecked the gun, sight and traverse or the transmission
the proof goes on to use. A live-penetration KIA is covered by `crew-solo-kia` (the loader).
In the recorded run the loader lived, so the handed-up round was loaded by him; an earlier run
of the same harness had the loader dead and the player loaded the breech by hand in 1.3 s.

What it does not cover: the held-mouse cartridge carry (it has its own `reload` proof), the
raymarch backend for the full set (the short `crew-solo-kia` check runs there), audio, and a
human at the controls.

## Medkit: done versus designed

Done in code and unit-tested: the seven procedures with their step tables, cadence and turn
rules, quality and interruption model, the varied finite kit with improvisation and
scavenging, triage, the airway timer, the first-person driving of a procedure from the
interior walk with the look locked to the hands, the crosshair as the step readout, the
intercom lines for the result. Designed, not built: first-person hands and props for the
procedures (there are no arm animations in the project yet), the man's breathing and voice
as audio (no assets), scavenging a wreck's own kit (wreck pickups restock the box as a whole),
and the morphine overdose beyond the one-dose-an-hour refusal.

## Verified against the play reports (2026-10-28, `crew-solo-kia`, raymarch backend)

Two reports from play on the integrated main: a figure left at the cupola when the player
moved, and a loader killed with nothing to see. The short headless check `crew-solo-kia PREFIX`
(one run, raymarch backend, `captures/crew_solo_kia_20261028*`) now passes: with the player at
the sights the gunner stands by at the rear of the basket and no figure is under the cupola;
walking the compartment leaves the loader at his post; a loader killed by a live solid shot
leaves a settled body at his post with blood emitted and retained across the cabin (the still
from the interior walk shows the body on the floor and the blood on the walls); a staged
femoral bleed keeps emitting onto the station over thirty seconds. The cause of the second
report was also a rule: a dead man's body blocked the *interior walk* as if it were a seat, so
the player could not drop into the compartment to find him; and a loader who died while the
player walked the compartment had been displaced to the cupola, so his body was not at his
station. Both rules are changed above.

## Verified: what the renderer draws (2026-10-28, `crew-solo-kia`, raymarch)

The cupola report is now checked against the renderer, not only the crew model: after a frame
is rendered, the visible crew-figure slots are read back (`debug_crew_solo_visible_figures`).
With the player at the sights the renderer draws the driver, loader and radio operator at their
posts and the displaced gunner standing by at the rear of the basket (0.57 m from the cupola
post); from the compartment floor it draws the four men at their own posts. In neither view is a
commander figure drawn, nor any figure within 0.35 m of the cupola post. The seated man seen
when looking up toward the cupola from the floor is the gunner on his own seat, below and ahead
of the commander's place, where a Tiger's gunner sits. All six `crew-solo-kia` checks pass.

## Verified: the empties (2026-10-28, `crew-solo-empties`)

`crew-solo-empties captures/crew_solo_empties_20261028` (shader backend): all six checks
pass. The rack is filled by a debug helper (named in the receipt) so the request comes up
without a dozen shots; the loader asks; refused, the commanded shot's empty falls to the floor
with the rack still full; four more cases are dropped at the loader's post (named) and the
player walks through them; approved, the loader opens the side hatch and heaves the cases out
(stills `empties_01_floor_clutter_at_the_breech`, `empties_02_side_hatch_open_dumping`: the
open aperture to daylight, a case mid-heave, upright cases in the rack beside it); a T-34
coming into the gunner's sight shuts the hatch at once; with the contact gone he resumes,
empties the rack and the floor, shuts the hatch, and the cases lie outside where they fell.
The Tiger is invulnerable for this check (it is about the empties, not the fight).

## Verified: the designation (2026-10-28, `crew-solo-designate`)

`crew-solo-designate captures/crew_solo_designate_20261028` (shader backend): all five checks
pass. Designations go through the same cast from the commander's eye with the look aimed at the
point (`debug_crew_solo_designate_at`). Ground 75 degrees off the bow: acknowledged, the second
pointer up, the turret on after 13.5 s (75 degrees at the 6 deg/s powered traverse, plus the
settle). The commander's eye ray met a rise of ground at about 230 m though the point asked was
600 m away; the range is what the eye sees. A second designation on the same bearing refined the
standing one. Designating 60 degrees the other way and dropping into the sights six seconds
later found the turret more than a quarter of the way there. A designated T-34 became the
gunner's target under hold fire with no shot. With the gunner dead (staged) the designation
stood on the dial and the turret did not move. Stills: `designate_01_tpz_two_pointers_on` (the
dial under its lamp, the two pointers together), `designate_02_sights_mid_slew`.

## Unit tests

`cargo test --release --locked --bin panzerrechnersystem -- first_aid crew_solo singleplayer_crew`
(24 tests): every procedure runs to its end with practised hands in about its nominal time; a
rushed, interrupted tourniquet leaks while a practised one holds; the wrap cadence counts only
alternating strokes and marks a loose wrap; improvised kit is used when the box is out and caps
the result; triage puts the man nearest death first; an unconscious man on his back loses his
airway unless positioned; burn care cools and dresses; the chest seal; the wound time course (untreated femoral bleed degrades, knocks out and kills within five
minutes; tourniquet stops it; dressing slows it; a trunk bleed is only partly controlled;
minor wounds clot), supply accounting (refusals cost nothing, out-of-supply, morphine limit,
restock), morphine and fracture performance, head wound unconsciousness and concussion, fire
burns, dead men refuse treatment; the slump pose; label coverage; the five-man roster and
displacement.

## Open items

- **No animation clips.** Men at work are posed static figures; wounded men slump by a
  rotation about the seat; a drag is a translation of the settled ragdoll. First-person arms
  for the treatment itself do not exist.
- **The first-aid box has no mesh**; it is a hotspot on the rear right turret wall.
- **Small arms on the head-out commander** are not resolved against the crew model: mounted
  infantry are still passive toward the tank (see `ASTRA_GAMEPLAY_AUDIT_2026-09-05.md`).
  The wound model is source-agnostic (`Wound::new`), so the hook is one call.
- **No voice.** There are no scream or breathing assets; a hit is blood, the flash, the slump
  and the intercom text.
- **Dismounted player.** While the player is on foot the crew keeps bleeding, burning and
  dying, but the AI stations do not fight (the vehicle authority is not stepped in the foot
  branch without a co-op frame).
- **Hit volumes do not follow the slump**; a slumped man's anatomical boxes stay at the seated
  pose (named in the ledger).
- **The radio operator** has no station the player can take (deliberately, as in co-op); he
  is a body that can be wounded and treated.
- Drive orders from the interior walk: WASD walks the body, so leaving the cupola halts the
  tank. A "continue last order" would need its own key.
