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
- The loader serves the breech with the existing auto-crew stroke state machine at a human
  cadence (`LOADER_STROKE_S = 0.75 s` per stroke against `RAM_PER_PRESS_OK` and `RAM_DECAY`,
  about six seconds from the shot to the next round seated at full performance).
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
| **LMB** buttoned at the cupola, nothing hovered | fire command |
| **WASD** from cupola / sights / radio seat | orders to the driver |
| **E** held on a wounded man, a body, or the first-aid box | treat / drag aside / self-aid |
| **LMB** on the driver's seat from the basket floor | take the driver's seat |

## Verified by play (headless, `crew-solo captures/crew_solo_20261027`)

See the receipt `captures/crew_solo_20261027.json` and the eight stills beside it. The proof
runs on the act-0 arena with one parked T-34 160 m ahead that holds its fire. Every strike is
the live armour solve, fragment cloud and anatomy aimed through the side plate at the man's
seat (`debug_crew_solo_strike`), with seeds iterated until the intended class of outcome; the
receipt lists every attempt.

Checks, in order (the receipt holds each one's detail):

1. Five men aboard; gunner, driver and loader working their own posts; player at the cupola.
2. The gunner lays on the T-34 and fires under fire-at-will (he killed it with the first
   round; the proof keeps a queue of parked targets and rolls the next one up).
3. The loader serves the breech back to loaded at a human rate (3-12 s).
4. Hold-fire holds the gunner for eight seconds; the commander's fire command releases one
   shot.
5. The driver moves the tank on the commander's forward order and halts on none.
6. A live 76 mm penetration through the hull side at the driver's seat, with the player at
   the cupola, leaves at least one man wounded and alive (in the recorded run it wounded the
   gunner, loader and radio operator in the legs and killed the driver outright).
7. The crosshair on the wounded man from the interior walk names the procedure he needs.
8. The first procedure is worked end to end by hand (steps in order, items taken, quality
   kept, the bleed controlled), then every procedure the man and the next-worst man need, then
   a staged fracture's splint (the fracture is injected and named so in the receipt: a 76 mm
   cone rarely breaks a bone); three or more distinct procedures, a still of the hands at
   each.
9. A minute later the treated man is alive, at his post, working degraded.
10. The dead driver's body blocks the seat; dragged off (hold E), the seat is taken from the
    basket floor with the clamber.
11. The gunner is killed by a live solid shot; nobody lays or fires the gun for ten seconds.
12. The dead gunner in the seat refuses the sights; holding E at the body drags it off; the
    player takes the sights, lays and fires by hand with no AI shot counted.
13. With the driver dead the tank does not move on the commander's order; the player takes the
    seat and drives.

State at the time of writing (2026-10-27 evening): the run of 18:55 passed checks 1-10 in
their earlier single-hold form (the receipt and stills were since overwritten by later
attempts; the log of that run is quoted in the hand-over note). That run then lost the Tiger
to its own stowage at the gunner strike (a solid 76 mm slug through the gunner's seat set off
the eight rounds left in the rear racks); the fixture now fires the last rounds off before
every strike. The procedure medkit (check 8 in its present form) and checks 11-13 with the
new fixture are **not yet verified by play**: the GPU was held for the owner's other work
(Kerbal Space Program at 97 % and 18.8 GB of 24.5 GB; the engine then comes up without ray
queries and dies at renderer construction), and a hold on launching the game was in force.
Pending captures: the whole `crew-solo` set (`01` crew at posts, `02` after the first shot,
`03` crewman hit, `04_<procedure>_<man>` one per procedure, `05` dead gunner from the cupola,
`06` the player at the sights, `07` driver dead and the tank halted, `08` the player driving)
and the JSON receipt. Rerun with
`tools/crew_solo_proof.sh captures/crew_solo_<date> C:/shop/panzerrechnersystem` and replace
this paragraph with the result.

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
