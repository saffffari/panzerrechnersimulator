# Panzerrechnersystem — Control Scheme

## Solo expedition controls — 30 October 2026

The current solo bindings below supersede the older keyboard descriptions in historical sections.
LAN/controller bindings remain separate. The gunner starts on **hold fire**; **K** deliberately
switches the autonomous-gunner experiment on or off. The bottom-of-view status shows that mode
and the current driver cruise setting.

| Input | Current solo action |
|---|---|
| LMB | Interact, pick up, enter, equip or use; fires the main gun only while in its sights (a click from any other post never fires) |
| X | Put away the topmost held item or exit the current station/view; one layer per press. At the commander's periscope with the lid open, X puts the head out through the cupola and X again brings it back in; with the lid shut it drops into the compartment |
| Shift / Space (cupola) | Shift: head out and in through the open cupola lid. Space tap at the buttoned cupola: down into the compartment; Space held with the head out: climb out on foot |
| Q / E | Decrease/increase driver cruise: reverse 3/2/1, stop, forward 1/2/3 |
| W/S, A/D | Manual drive/steer at control stations; walk in the compartment or on foot |
| Arrow keys | Coarse traverse/elevation through a capable gunner; direct control at gun sights |
| Mouse | Fine gun aim only at sights; head/body look elsewhere |
| V | Indicate an approximate clock-bearing sector; no enemy lock or automatic range measurement |
| K | Toggle the autonomous-gunner experiment; no commander LMB fire order |
| M / I | Physical map / repair manual (B stays the binoculars) |
| PgUp / PgDn, Enter (map held) | Expedition orders on the theatre map: previous / next offer, accept. Accepting stows the map; the written order prints on the teleprinter strip (the head at the commander's left under the turret roof, or the detail's machine on the camp's orders table when you are on foot there). M rereads it on the map with its marks and bearing |
| E / LMB on the strip | Tear the printed order off the machine and read it in hand (free hands, like the map) |
| X (strip in hand) | Fold the strip into the pocket |
| O | Hold the pocketed strip up again |
| E / LMB on the head's keys, then type, Enter / Esc | Key a line to the support detail (Stab) at the teleprinter: the line and the detail's answer print on the strip and hang there until torn off. The answer comes from the radio model (`KESSEL_RADIO_MODEL`, `ANTHROPIC_API_KEY`) or, with no key or network, from the offline responder that reads the same context back |
| 1 / 2 | P08 / Panzerfaust on foot; 1 toggles the mounted P08 while aboard |
| R | Reload personal weapon on foot; throw a carried spent case while aboard |
| F / T | Handheld light / request ammunition type |
| Z | Stop the engine or operate its starter from an eligible control station |
| Escape / Home | Pause / controls reference |

Cruise is a standing speed request, not a throttle percentage. Initial targets are 2/5/8 m/s
forward and 1/2/3 m/s backward; actual motion still depends on the drivetrain, ground and damage.
The driver brakes before reversing. W/S at a command station cancels cruise. Walking inside and
moving between stations preserve the standing drive order; walking sideways cannot steer the hull.
Losing or incapacitating the driver cancels cruise. The player can still drive from the driver's
station. A dead gunner disables remote arrows, but the player can take the sights after clearing
that station. A held map/manual consumes new commands rather than cancelling standing orders.

On foot with empty hands, hold **LMB** on a body to move it; **X** or releasing LMB lets go.
An incapacitated living crewman can be extracted through a fully open hatch using the same
interaction. This does not kill or heal him: field bleeding continues. Bring the held patient
beside a camp stretcher and target that bed with **LMB** to admit him to medical care. Taking
the patient off the bed interrupts that care. A stabilized living person can later be selected
for control with their existing injuries and identity intact.

The transport in **Recovery Playtest** and **Expedition** uses **LMB** to board the stopped halftrack
with empty hands, **W/S** to drive, **A/D** to steer, **Space** to brake and **X** to leave once
stopped. Its current source model has no modeled cab, so this prototype uses an exterior follow
camera. Open the rear ramp before loading. Carry a compatible spare to the rear contact and use
**LMB** to load it; aim at loaded cargo and click to unload. Payload and space are finite. A held
casualty beside the open ramp loads after a short held **LMB** interaction; their injuries and
identity persist throughout transport. Do not put patients in an occupied cargo aisle.

Equip the physical field first-aid bag with **LMB**, then click a wounded person. Its prompt asks
for holding, wrapping with alternating mouse movement, turning, or watching. **X** interrupts and
puts the bag down. The bag and camp medical tent use separate finite supplies. This uses the
game's existing treatment model; first aid does not revive a dead crew member.

The portable winch is separate equipment, not an unaided halftrack towing rating. Clear the bed
and lower it down the rear ramp, push it to a level anchor site, then take its cable to the Tiger's
actual lower tow mounting. Hold **LMB** at the winch to pull. Fuel, cable length, slope, load and
obstructions constrain movement. Release the pull before **X** detaches the line for re-rigging.

For physical workshop parts, hold **LMB** on an exposed retainer to release it, release the button,
then hold again to withdraw the component. Carry a compatible replacement to the empty mounting;
seat, secure and inspect it as separate steps. **X** puts a held part down on a supported surface.
Fitting a damaged component does not make it sound. The work lamp has a finite battery.

Use the pause menu's **Save expedition** and **Load expedition** entries for the complete retained
battlefield. **F5** also saves from the succession/expedition status screens. A failed load preserves
the current world; a save requires its matching game build and source assets. New Expedition starts
at camp, while Resume preserves the saved position, people, damage, projectiles and work in progress.

These paragraphs describe registered controls. The review page distinguishes the passed transport,
winch, optics and save scenarios from boarding, medical transport and heavy overhaul still under
combined gameplay verification. The older sections below are historical references.


Solo Playtest input and physical equipment notes updated 2026-09-09. The shader branch still
requires combined runtime review; the bindings below describe the implemented public input paths.
Bounded review scenarios are listed in [SHADER_MECHANIC_REVIEW_FIXTURES.md](SHADER_MECHANIC_REVIEW_FIXTURES.md).

The from-scratch rework after the destruction build: the old scheme grew binding-by-binding and
collapsed under overloads (LMB meant five things; C meant two; the Driver station was unreachable
in windowed play). This scheme is built on **three rules** plus a station row, and it is what
`src/main.rs` implements and the in-game Home overlay shows. When this doc and `src/` disagree,
trust `src/`. (The pre-2026-07-06 content described the abandoned Unreal build; its verified
historical values are preserved at the bottom.)

The live HUD, interaction prompts, menus, map instructions, and controls are in English.
**Kampagne** remains the campaign mode name; vehicle names and ammunition codes keep their
historical designations. Original lettering painted into model textures is unchanged.

## LAN crew mode

`--host` / `--join` splits the same controls across five physical jobs. **F6** requests the next
open seat. Driver owns WASD/Z; Gunner owns mouse/LMB/MMB/RMB; Loader performs the physical lever and held-LMB
cartridge handling; Radio owns the bow MG. With two crew the Commander covers
Driver and the Gunner physically alternates between sight and breech. At three crew only the
dedicated Driver post can move the tank. Remote crew bodies show the occupied physical posts. Roles
set authority but do not trap players at a camera: Q drops from the cupola into the compartment.
The held map and controls reference are local and do not pause the shared LAN simulation. Full
launch and protocol notes: [`LAN_COOP.md`](LAN_COOP.md).

## Split-screen couch co-op (Strongpoint, 1–4 players)

Press **F5** at the title menu to open **Crew co-op**. Its **Strongpoint (co-op)** mode is the same
crew model as LAN, on one machine and one
screen: it launches the TAKE & HOLD op on the act-0 bowl (clear the picket out of the ruin compound,
stand in the yard to raise the capture, break counterattack waves of four, five, and six T-34s).
Three dug-in defenders and short regroup beats keep the contact dense. Seats are POSITIONAL, not
touch-to-claim. A full couch crew has four essential Tiger posts; the fifth historical post,
Radio/Bow-MG operator, is deliberately omitted from this mode. On the **Crew co-op** setup page,
move to **Couch devices** and choose one of two explicit arrangements before starting split-screen:

- **Keyboard + controllers:** keyboard/mouse is **COMMANDER**; pads 1–3 are **GUNNER**, **DRIVER**,
  and **LOADER**. This is the original arrangement and its controls are unchanged. Commander owns
  head-look, binoculars, hatch/head-out, held map, and WASD fallback driving until Driver joins.
- **Controllers only:** pads 1–4 are **COMMANDER**, **GUNNER**, **DRIVER**, and **LOADER**. No
  keyboard player is inserted into the roster and keyboard/mouse cannot drive or interact with the
  tank. A zero-pad run stays unmanned behind a connect-controller prompt instead of becoming solo.

The station controls are:

- **Commander pad.** Left stick drives/steers, right stick observes, **LT (L2)** raises binoculars,
  **A (Cross)** works the hatch, **RB (R1)** covers the bow MG, and **R3** controls ignition. When
  this is the only pad, its right stick and **RT (R2)** also lay and fire the main gun through the
  empty-Gunner fallback.
- **Gunner pad** (own viewport: the TZF). Right stick lays the turret at the real hydraulic
  rates, **RT (R2)** fires the 88 (edge-triggered), **RB (R1)** coax, **LT (L2)** steps the sight
  magnification, and **X (Square) or A (Cross)** works the breech between shots until a dedicated
  Loader joins.
- **Driver pad** (own viewport: the visor port). Left stick drives and **R3** controls
  ignition.
- **Loader pad** (own viewport: the breech). Hold **X (Square) or A (Cross)** to eject the
  hot case and drive the next round home; the Gunner is then free to stay on the sight.

Layout: 2 players = left/right halves (each a center crop of a full frame — square pixels at any
16:9 window). 3 players = commander keeps the tall left half; gunner and driver stack the right.
4 players = a 2×2 Commander/Gunner/Driver/Loader grid. Every viewport is a complete PS1-crushed
frame with its own temporal history. Solo pad shortcuts are disabled while seats are dealt because
those buttons belong to the crew. The pads hot-plug: a pad connected mid-run takes the next seat on
the spot; a disconnected pad keeps a black placeholder so the other views never jump cells. The
mode determines the live pad limit: three behind the keyboard or all four in Controllers-only.
Every cell stays 320×180 internally and the selected Laptop or 4090/High quality tier controls the
remaining terrain, shadow, lighting, and effects budget.

Crew loss opens the dead crewman's post instead of ending the mission immediately. A surviving pad
player presses **B (Circle)** to take over the next open KIA station; the keyboard Commander uses
**F6**. Taking over vacates the survivor's previous post, so the reduced crew must deliberately
choose which job to cover.

A controller survivor inherits the controls of the post they take. In Commander that means right
stick free-look, left-stick driving, held **LT (L2)** binoculars, **A (Cross)** hatch, **RB (R1)**
bow MG, and **R3** ignition. Behind a keyboard Commander, a controller survivor who takes that post
retains the original emergency **RT or RB** bow-MG binding; RT is the main-gun fallback only in
Controllers-only. In Controllers-only, the living Commander also owns the shared UI:
**View/Share** raises the map, right stick moves the pencil, **RT/R2** draws, **Menu/Options** toggles
the controls sheet, and **B/Circle** closes an open overlay before requesting a KIA-post takeover.

Headless verify: `cargo run -- koop prefix` walks the mission state machine with a synthesized
four-person couch crew (PASS/FAIL asserts) and writes per-seat Commander, Gunner, Driver, and Loader
captures plus the 2×2 split-screen frame, a KIA split, and a post-takeover Loader view. Living crew
are visible from the other seats; a fallen body and blood remain in its struck post after takeover.

## The three rules

1. **LMB = your station's weapon, or your hands.**
   Gunner: fire the 88 (edge-triggered; the click that *entered* the sights never fires).
   Commander: bow MG (hold). Loader: click the lever; hold a case or round while carrying it,
   release to drop it, and keep holding an aligned round at the empty open breech to seat it.
   Carried tools own LMB; otherwise on foot it fires the selected weapon.
2. **RMB = look closer.**
   Gunner: TZF magnification toggle. Buttoned Commander / Loader: look at a sight block or
   periscope and **hold RMB** to bring your eye to it; release to return to the cabin view.
   Driver: **hold RMB** to lean toward the visor; look down-left and click its handwheel to open
   or close the physical shutters. Head-out Commander: hold RMB to raise/aim the P08;
   LMB fires it while aimed. **V** draws/holsters the P08 inside the turret or at the cupola;
   with it drawn, LMB fires, RMB aims and R reloads. **Q** holsters before leaving the post.
   Holster it to use a sight block, hatch handle or other hand equipment. A carried cartridge
   takes priority over the pistol. Closed armor blocks the bullet; an obstructed muzzle cannot
   launch a round through it. The side escape opening can be fired through while standing inside.
   **B** explicitly toggles binoculars at eligible stations; held interior RMB stows them first.
   On foot: aim down the selected weapon's physical sights (hold).
3. **E = interact with physical equipment and the tank's body.** Pick up the looked-at toolbox,
   extinguisher or available Panzerfaust tube, enter a station anchor, work a hatch or mount.
   LMB also operates physical interactables; cartridges specifically use held LMB. Carry the
   toolbox visibly with the hammer, then press X to put it down nearby. It returns to its
   wall mount only when that mount is within reach and under your gaze. A carried Panzerfaust
   returns to its own empty rack slot when you face that slot within reach.
   Open a hatch with **LMB**, then press **Space** at its opening to climb through. This works
   both ways for the driver, radio, loader, side escape and cupola hatches. Roof entry requires
   climbing onto the tank first. Space remains jump away from an opening; holding it cannot
   reverse a completed climb. **Hold Space for 1.5 seconds** at the open cupola is the exit.

## Xbox / PlayStation controller

Controllers are hot-plugged through `gilrs`' native macOS/Linux gamepad backends. Xbox and
PlayStation pads use the same semantic layout; the names below use Xbox labels, with PlayStation
equivalents in parentheses.

| Input | Action |
|---|---|
| **left stick** | drive / walk |
| **right stick** | head-look / gun lay / loader hand gesture |
| **RT (R2)** | station weapon, hand, Luger, or map pencil |
| **LT (L2)** | aim / cycle sight magnification / binoculars |
| **A (Cross)** | interact with the physical object under the crosshair; on foot, pick up/place a portable item or refuel at the Tiger's rear |
| **B (Circle)** | back out one embodied layer; with a portable item equipped, throw it; in Strongpoint, take over an open KIA post |
| **Y (Triangle)** | tap: head out; hold 1.5 s at the open cupola: dismount |
| **X (Square)** | P08 magazine change; Panzerfaust tubes come from the physical rack |
| **RB (R1)** | coax MG34 |
| **D-pad** | fine gun lay / menu navigation; left/right selects P08/Panzerfaust on foot |
| **View / Share** | raise/stow map |
| **Menu / Options** | solo pause menu; local controls sheet in LAN (couch bindings above) |
| **stick clicks** | left: crouch; right: ignition |

USB is the zero-friction connection. On macOS, put the controller in pairing mode and use **System
Settings → Bluetooth**; on Linux use the desktop Bluetooth panel or `bluetoothctl`. The game can be
running before the pad connects. Solo/menu control follows the most recently used pad, while couch
crew seats retain their stable physical-pad identities.

Right-stick look starts at 2.5× the previous sensitivity in solo and couch play (1600 mouse-equivalent
pixels per second at full deflection). D-pad fine gun lay is unchanged; a sensitivity slider is not
available yet. Esc → Settings → Haptics toggles native controller rumble and optional Keys mouse
feedback together. Feedback follows actual gun discharge, own-tank impact, primer misfire, and round
seating in local solo play. Pause or focus loss stops controller rumble and further mouse requests;
the mouse service may finish a finite pulse it already accepted. LAN and couch haptics are not enabled.

## Physical stations

| Post | View |
|---|---|
| Driver | visor slit |
| Gunner | TZF sights |
| Commander | periscopes / cupola — *home* |
| Loader | interior walk + breech |

There are no number-key station teleports. Look at and click/E the actual anchor. **Q** backs
out one layer: binoculars or compass, carried equipment, sights, then the cupola. Leaving the
Gunner sight or buttoned Commander view returns to the Loader compartment. **Space** leaves
the Gunner sight, enters/exits through a nearby open hatch, or jumps while on foot away from
an opening. In the walking compartment, look at the loader or side escape opening to select it.
Hold **Ctrl** to crouch inside or outside; standing resumes only where there is headroom.

## Everything else

| Input | Action |
|---|---|
| **W S A D** | drive, from every station (A/D combine with W/S); walks at the Loader / on foot |
| **mouse** | lay the gun (gunner) / head-look (commander, loader, foot) |
| **arrows** | direct gun-lay at sights / coarse orders through a living gunner elsewhere |
| **MMB** (hold) | coax MG34 |
| **C** | take out / put away the cigarette pack and cigarette |
| **Q / E** | lower / raise the living driver's standing cruise speed; seven notches including stop |
| **LMB** | interact, pick up, equip, enter, or use the held tool; main-gun fire only at the sights |
| **F** or **L** | take out / put away the hand flashlight |
| **PageUp / PageDown** | driver: shift up / down by hand |
| **End** | driver: return the gearbox to automatic selection |
| **F3** | performance statistics overlay (also works over the map and controls sheet) |
| **Ctrl** (hold) | crouch inside or outside; release stands only with headroom |
| **Shift** | head out of the cupola ↔ duck back in (commander; only through a hatch already opened at its lever) |
| **Space** (hold 1.5 s) | climb OUT on foot, head in or head out, only with the cupola hatch open; the HUD ring fills while you hold. Nothing opens the hatch for you |
| **T** | ammunition reminder; actual AP/HE choice is the grey/yellow physical rack round you equip |
| **LMB with a tool equipped** | use the extinguisher or repair the actual broken component within reach |
| **X with toolbox** | put the visible toolbox and hammer down nearby; face its reachable mount to stow it there |
| **M** | raise/stow the held paper map; world keeps moving; the compass comes out with the map |
| **LMB on map** | draw permanent graphite with the pencil; there is no eraser |
| **Home** | controls overlay (pauses solo; local overlay in LAN / couch crew play). Reachable at any moment of play, including with the map held |
| **K** | solo crew: the gunner's standing order, fire at will / hold fire |
| **Y / N** | solo crew: yes / no to a crewman's request (the loader asking to dump the empties out of the side hatch) |
| **V**, or **MMB** (pad **RB**) at the cupola | indicate a coarse clock-bearing sector for the gunner; no precise target lock |
| **X** | stow held item or leave the current station/view; empty-space LMB never commands gunner fire |
| **E** (hold) on a wounded crewman, a body, or the first-aid box | solo crew: treat him / drag the body off the seat / treat yourself |
| **LMB** on the driver's seat from the front of the basket floor | solo crew: take the driver's seat (the clamber takes a beat) |
| **F6** | LAN: request the next open station; Strongpoint: take over an open KIA post |
| **[ ]** / **F12** | retro dial / screenshot |
| **Esc** | pause local play, including while holding the map; Esc again resumes from the main pause page |
| **Alt + Enter** | toggle fullscreen / windowed from any screen |

### Canyon run cards (2026-10-27)

The Canyon run opens, once per session, on a brief (the course in a few words, the keys, the
attempt's seed, start clock and weather); any key drops into the waiting world. When the run ends,
by death or by reaching the head of the ravine, the after-action card follows the death
presentation or the finish fade: what ended it, time, distance reached, rounds, kills, hits taken,
modules lost, the attempt's seed. **Enter / Space / R** starts a fresh seeded attempt without the
title; **Esc** returns to the title with the Canyon entry selected. Controller: **A / Cross**
restarts, **B / Circle** or **Menu** leaves. The cards ignore input for the first 0.6 s so a key
mashed through the death skip cannot dismiss them unread.

For deterministic visual comparisons, `KESSEL_RETRO_PRESET` accepts `clean`, `light`, `standard`,
or `strong`. This is independent of `KESSEL_INTERNAL_RES` (`640x360`, `800x450`, `960x540`, or
`1280x720`), so pixel density and post-process strength can be evaluated separately. `clean` keeps
the internal-resolution/nearest-neighbour presentation but fully bypasses posterization, Bayer
dither, and film grain.

In LAN play, **Esc** keeps opening the local controls sheet instead of leaving the shared
simulation paused. **Home** is always the direct controls-reference key.

## Pause, save, and load

In solo play, **Esc** opens **Resume / Save checkpoint / Load checkpoint / Settings / Exit game**
and pauses the simulation and game audio. Use the mouse, **Up / Down + Enter**, or controller
**D-pad + A / Cross**. **Esc / B / Circle** returns from a submenu; on the main pause page it
resumes play. Settings includes windowed/fullscreen display, the retro filter, and the performance
display. Use **Left / Right** to change a setting. Exit game opens a confirmation before quitting.

**Playtest** loads the actual **Act 1 / The City** winter map at its authored spawn, with six
**T-34-76s** placed nearby on clear terrain. It starts in **practice**: the targets stay stationary and can be
destroyed, while enemy tanks and NPC infantry hold their fire. In
**Esc → Settings**, set **Enemy return fire** to **ON** for live combat, or **OFF** to practice
again. Enabling restores enemy movement and gives the tank guns their staggered opening reload;
turning it off brakes tanks to a stop and holds NPC movement and new shots without removing
projectiles already in flight. Player weapons and damage remain active.
This setting is exclusive to Playtest and does not change Kampagne.

Switching to another application or minimizing pauses local play automatically and suspends its
rendering while inactive. Return to the game and choose **Resume**. LAN sessions keep running.

**Save checkpoint** is available during an active **Kampagne** sector. There is one manual save
slot; saving replaces it. Wait for **CHECKPOINT SAVED** before exiting. There is no autosave.
**Load checkpoint**, from the title or pause menu, restores that saved sector entrance and leaves
the game paused until you choose **Resume**.

This is a **sector-entrance checkpoint**, not a save at your current position. It restores the same
attempt and generated route, entrance Tiger condition, fuel, ammunition, inventory, and saved
building state, while retaining story discoveries recorded when you saved and the session's
day/night choice. Actions since entering that sector must be replayed: current position, combat,
objective progress, and map pencil marks/pins are not restored. Explicitly loading an Act III
checkpoint can restart its saved entrance; the finale's ordinary defeat path still has no automatic
retry. Playtest does not create campaign checkpoints.

On Linux, the save is `~/.local/state/panzerrechnersystem/campaign-checkpoint.v1.json`, or
`$XDG_STATE_HOME/panzerrechnersystem/campaign-checkpoint.v1.json` when that variable is an absolute
path. Missing, corrupt, or incompatible saves show an error and keep the current game. Campaign
rules or authored-world changes can make an older checkpoint incompatible; shader, UI, and audio
edits do not by themselves invalidate it.

Loading ignores incomplete temporary save files left by an interrupted write. If the installed
checkpoint itself is truncated or corrupt, loading reports an error; it does not guess missing
state or start a new campaign. There is no separate backup slot, so keep playing and save again
while the current campaign is still available.

## On foot (Absitzen)

| Input | Action |
|---|---|
| **W S A D** | move (running is the default gait) |
| **Ctrl** | crouch (slower, quieter, smaller) |
| **Space** | enter/exit a nearby open hatch with one press; jump away from hatches |
| **LMB / RMB** | fire / aim down the selected weapon's physical sights |
| **1 / 2** | select P08 / Panzerfaust |
| **R** | search a cleared campaign map case within reach; P08 magazine change |
| **B** | binoculars |
| **E** | pick up the looked-at portable item; E again places it gently with no throw impulse; a fuel-bearing jerry can refuels only at the Tiger's rear service area |
| **Q** | stow compass or put down toolbox first; otherwise cycle weapons with empty hands or throw a carried portable item |
| **E** or **LMB** on a cupola | mount |
| **LMB at a broken track, with kit** | grab loose end → hold and pull mouse down → place replacement link → hammer pin |

Equipping a portable item occupies both hands and suppresses the selected weapon until the item is
placed or thrown. Refueling transfers only what the tank can accept; any remainder stays in the
same physical can. An emptied can remains in the world and can still be carried, placed, or thrown.

In Kampagne, **M** opens the held map together with the compass. The world keeps moving. Its **?** marks the next
search area. Clear the guards, dismount, approach the map case and press **R** when its search hint
appears. Recovering all three sheets reveals the exit pin. **Home** opens the controls reference
and pauses solo play.

## WORLDBUILDER, Tiger dressing, and asset yard

Launch with `cargo run -- edit <act|manifest.v1.json>`. The scene-scale terrain workflow edits the
same heightfield used by play rather than a preview-only overlay.

Launch `cargo run -- edit yard` for a generated flat walk-around gallery containing every GLB in
the Pizza library, grouped into category blocks. The yard is read-only and regenerates from the
live library index; Tiger dressing authored there still saves normally.

| Input | Worldbuilder action |
|---|---|
| **F** | toggle grounded first-person walk and Unreal-style orbit editing |
| **F8** | enter/leave the live Tiger; WASD drives and arrow keys lay the turret/gun |
| **1 / 2 / 3 / 4 / 5** (Tiger) | Commander / Gunner / Loader / Driver / Radioman seat cameras |
| **RMB / Shift+RMB** | look/orbit / pan the orbit pivot; wheel zooms in orbit |
| **Tab** | open/close the left-side asset grid; every Pizza pack plus Materials, Sounds, Brushes, Lights, Scenes, Story Details, Prefabs, and Scene Blocks has its own tab |
| **/** / **[ ]** (browser open) | focus asset search / cycle pack tabs |
| **P** (browser open) | start a nonblocking preview bake for the current tab; the editor reports when it finishes and reloads the cache |
| **Shift+click model cards** | build one cross-pack multiasset palette; Shift+click again removes a card |
| **Shift+click texture cards** | build an equal-weight material mix of up to four image layers; drag one selected card or press V to place it |
| **B** | paint the selected multiasset palette, a selected placement mix, or a Brushes-tab recipe; B again exits |
| **LMB / Shift+LMB** (asset brush) | add field-driven dressing / erase existing members of this brush |
| **wheel / Shift+wheel** (asset brush) | brush radius / density |
| **I** (asset brush) | initialize this deterministic recipe across the current act once, with an editor-safe instance cap |
| **drag thumbnail / V** | place on the map; from a Tiger seat, an ordinary model attaches only when the cursor actually hits the Tiger interior/exterior, otherwise it places in the world; Story Details and editor tools always target the world |
| **G / R / T** | move / rotate / scale a map object or selected Tiger prop |
| **G / T / [ ]** (sound selected) | move the sound source / resize its audible radius |
| **- / =** (sound selected) | lower / raise source volume |
| **, / .** (sound selected) | shorten / lengthen duration; zero uses the source clip's duration |
| **L** (sound selected) | cycle loop-while-inside / once-per-playthrough / replay-on-re-entry |
| **drag / V** (Materials) | place a terrain material-mix zone, mud, scorch, or frozen-red-snow stain under the cursor |
| **G / R / T** (surface selected) | move / rotate / resize the outlined material or stain zone |
| **- = / , . / K / C / X** (surface selected) | opacity / wetness / snow cover / duplicate / delete |
| **drag / V** (Scenes) | stamp a Lit Utility Road, Furnished Forward Base, Destroyed Convoy, or Overrun Artillery Position; all generated pieces remain individually editable |
| **G / R** (local light selected) | move the light / rotate its authored direction |
| **[ ] / - = / , .** (local light selected) | range / intensity / color temperature |
| **L / K / C / X** (local light selected) | enable / flicker / duplicate / delete |
| **[ ] / , .** (Sun/Sky selected) | sun azimuth / elevation |
| **- = / ; ' / K / X** (Sun/Sky selected) | exposure / fog / storm / restore campaign defaults |
| **H** | enter terrain sculpt; H again accepts, Esc restores the session |
| **1 / 2 / 3 / 4 / 5** | raise/lower / smooth / flatten / deterministic noise / terrace |
| **LMB** | sculpt under the green ring; Shift+LMB lowers with raise/lower |
| **wheel / Shift+wheel** | terrain brush radius / strength |
| **cyan square / amber square** | exact captured terrain core / outer stitch-apron perimeter |
| **F7** | capture the uncaptured sculpt plus selected props as a versioned scene block and save |
| **F6** | save selected props only as the older terrain-draped prefab format |
| **F5** | save ordinary edits; refuses while a transform is unconfirmed or terrain still needs F7 capture |
| **F9** | reload from disk; with unsaved work, press twice within five seconds to confirm discard |
| **Esc / window close** | Esc first closes search/browser or cancels a tool; exit with unsaved work requires repeating the same action within five seconds |

Missing model previews show as pending instead of freezing the live editor. The offline thumbnail
pass writes JPEGs through the game renderer; sound tiles use lightweight waveform cards and never
enter the 3D thumbnail renderer. Run `cargo run --release -- thumbs` for the full preview pass, or
add a pack/category/name after `thumbs` to bake only that subset. Run the full pass as an offline
Linux/RTX 4090 batch rather than blocking an interactive editor session.

The built-in forest, forest-floor, battlefield-debris, and aftermath recipes score the same terrain
against height, slope, sun exposure, soil moisture/fertility, wind exposure, and destruction. `I`
materializes stable ordinary placements and stores the recipe/seed as provenance; afterward every
object can be moved, replaced, or deleted normally and will not reshuffle itself. Story Details
contains the license-safe fallen-soldier and burned-wreck silhouettes for manual narrative tuning.
The **Scenes** tab adds correlated compositions rather than uniform scatter: a furnished
ground-floor compound, a connected lit utility run, and two battle-aftermath layouts with wrecks,
debris, casualties, scorch, and frozen-red-snow authoring zones. Utility runs currently stamp only
on nearly level ground because ordinary placements carry yaw but not wire pitch; the saved
connection graph is retained for the runtime catenary handoff. The base deliberately uses the
structural modules that already receive gameplay collision, so its ground-floor rooms are
enterable. Generic upper floors and stairs are not yet runtime-playable.
Scene recipes stamp in place and are not auto-selected as one transform group: select and tune
individual pieces afterward. This avoids an accidental first drag moving only model placements
while silently leaving their lights, stains, and connection graph behind.
See [`PROCEDURAL_ENVIRONMENT_AUTHORING.md`](PROCEDURAL_ENVIRONMENT_AUTHORING.md) for the field model,
performance limits, and causal-destruction roadmap.

F7 captures a stable, sample-aligned ground datum and rescans the result into the TAB browser's
**Scene Blocks** category. See [`SCENE_BLOCKS.md`](SCENE_BLOCKS.md) for Rhino handoff, provenance,
size, and stitch-apron rules.

## Physical equipment and remaining review

- Reload from the Loader view: click the breech lever open, hold LMB to extract the spent case,
  and release to drop it. Hold LMB on the exact grey/yellow rack round to carry it. Present it to
  the empty open breech and maintain the grip while it seats, then click the lever closed.
  An unfired chambered round can also be extracted with held LMB. Cases and live rounds retain
  their physical size and carry momentum. Held rounds and cases use a lightweight hand spring
  and can pass through nearby surfaces; detailed rigid-body contacts while held were removed
  after severe pickup stalls. Loading uses the open/empty breech gates and guided alignment.
  Dropped ammunition uses bounded collision queries against the tank geometry, with bounce and
  friction. Resting rounds and cases sleep until the tank geometry moves or you pick them up.
  A long frame or extreme release momentum cannot expand the collision-work budget; excess
  movement is deferred. Impact sounds have a cooldown instead of repeating every contact frame.
- One Panzerfaust and two compact spare bombs sit horizontally on the upper port turret wall,
  level with and clear of the side escape hatch. Click any item to carry the complete kit;
  R readies the next reserve shot after firing. Face the reachable empty mount and use E/Q
  inside to return the unfired ammunition. This compact reserve display is a game abstraction;
  dismounting does not grant extra ammunition.
- Pick up the physical repair toolbox. The box stays in the left hand and hammer in the right,
  inside or outside. Approach and aim at the actual broken component, then hold LMB to repair.
  At a broken track: grab the loose end, hold LMB and pull down, fit the link, then hammer the pin.
  Completed work leaves the tools in your hands. X puts them down for later pickup; repair
  cannot be performed remotely from the kit mount. Specialized repair minigames beyond the
  existing track sequence remain future work.
- These mechanics have CPU coverage; the combined shader branch still needs visual and live
  input review for hand placement, clipping, readability and usable reach.
- Binoculars at the Gunner are deliberately absent (the TZF is that post's optic).
  Commander/Loader/on-foot binoculars use one stable 10-degree true field; raising them at an
  interior periscope magnifies its existing picture without changing the periscope projection.
- Rebinding UI is not implemented yet; keyboard/mouse and controller bindings coexist.

---

## Historically-accurate values (Tiger I, 8.8 cm KwK 36 L/56) — carried over, still canonical

- **Turret traverse:** hydraulic Boehringer-Sturm drive, engine-RPM dependent. Modeled baseline
  **6°/s** (360° in ~60 s at moderate RPM). Historical max at high RPM ≈ 19°/s; RPM-linked
  traverse is a future enhancement.
- **Gun elevation:** **−8° to +15°**, manual handwheel, modeled ~4°/s (playability estimate).
- **Gunsight:** two magnification stops — 2.5× (25° true FOV) and 5× (12.5° true FOV); RMB steps
  between them. (The dual-power TZF 9c is modeled; the earlier 9b was fixed 2.5×.)
- **Reticle (2026-07-25):** the authentic TZF Strichbild — BLACK etched glass, not a glowing
  overlay: a central Hauptdreieck (4 Strich wide, apex = aim point) flanked by three 2-Strich
  triangles per side at 4-Strich clear gaps (lead + stadia ranging), outer reference bars. One
  Strich = 1/6400 circle stays TRUE through every magnification (the crush uniform carries the
  live FOV). The gunner's ADS is an EYEPIECE COMPOSITE: the optic circle sits seated in the
  live turret interior (two-pass render, crush mask 4) instead of floating in black.
- The mouse at the gunner writes the *desired* lay; the turret **chases it at the real rates
  above** — flick to the target and wait for the hydraulics. That lag is the game.

## Changes of 3 October 2026

- **Left mouse is the one action button.** With something usable under the crosshair (the dot
  shows) it interacts; otherwise it fires or works the tool in hand. A carried load is a toggle:
  click to take a round or case, click again to ram a lined-up round or let go.
- **Space enters and exits** hatches, the turret cabin, a spare tank and each move of the climb,
  as it did before. (E was tried for this and kept bailing the player out in combat.) On open
  ground Space jumps; with clothes alight it beats out the flames.
- **E is a second interact key**: it does whatever the action button would do under the crosshair.
- **At the cupola, Shift puts the head out and back in, and held Space climbs out.** Both need
  the hatch already open; neither opens it. A tap of Space there does nothing, so a stray press
  cannot leave the tank. X drops back into the turret. R throws carried cases; X leaves the station.
- **The hatch lever's target is the grip and the pivot cylinder it turns**, outlined together.
- **R throws the spent case in your hands** where you are looking. Through an open hatch it
  leaves the tank and lies on the ground. LMB still just lets go of it.
- **Interaction hint.** While something is outlined, a small line at the bottom centre of the
  screen, in the outline's colour, names what it is or does and the button for it in parentheses.
- **The driver brakes when no drive is asked for.** Let go of W or S and the tank slows to rest
  in a few seconds and then holds itself on a slope. It no longer rolls away when stopped.
- **Cigarettes.** C brings out the whole packet. Look at a cigarette (it is outlined) and press
  the left button to take it. Hold the right button to light it, then hold the right button to
  toke; the left button and every interaction stay free. C with one in the mouth takes it back in
  the hand. A lit cigarette goes to the mouth by itself when the hands are needed.
- **Leaving by the cupola needs the cupola view first.** From below, click the cupola to climb
  into its seat; only there does held Space climb out. The hatch lever itself can be worked from
  below as well as from the seat. X drops back down.
- **Day and night.** Every mode now runs a fifteen minute day: five minutes of full day,
  two and a half each of sunrise and sunset, and five of night. The Sun and Moon keep their true positions for the map's date and place; only the
  clock runs fast. `KESSEL_DAY_NIGHT=0` restores the real-time clock.
- **The hatch key H is removed.** Open a hatch with the action button on its lever or lid, then
  Space to go through.
- **Reload:** take a rack round, carry it to the open breech and line it up, press again to ram
  it, then work the lever to close the breech.
- **C** is the cigarette: hold the action button to draw on it, release to exhale. The compass
  comes out with the map on **M**.
- **Death:** the view fades, two seconds of black, five seconds of the fallen card, then the
  run ends. No music plays over it.
