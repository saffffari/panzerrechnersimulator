# Kessel — Control Scheme (rebuilt from scratch 2026-07-06, explicit user request)

The from-scratch rework after the destruction build: the old scheme grew binding-by-binding and
collapsed under overloads (LMB meant five things; C meant two; the Driver station was unreachable
in windowed play). This scheme is built on **three rules** plus a station row, and it is what
`src/main.rs` implements and the in-game Home overlay shows. When this doc and `src/` disagree,
trust `src/`. (The pre-2026-07-06 content described the abandoned Unreal build; its verified
historical values are preserved at the bottom.)

## LAN crew mode

`--host` / `--join` splits the same controls across five physical jobs. **F6** requests the next
open seat. Driver owns WASD/Z; Gunner owns mouse/LMB/C/RMB; Loader performs the physical lever,
E-equip, and fast-forward ram ritual; Radio owns LMB/C bow MG. With two crew the Commander covers
Driver and the Gunner physically alternates between sight and breech. At three crew only the
dedicated Driver post can move the tank. Remote crew bodies show the occupied physical posts. Roles
set authority but do not trap players at a camera: Q drops from the cupola into the compartment.
The held map and controls reference are local and do not pause the shared LAN simulation. Full
launch and protocol notes: [`LAN_COOP.md`](LAN_COOP.md).

## Split-screen couch co-op (Stützpunkt, 1–4 players)

The title menu's **Stützpunkt (co-op)** entry is the same crew model as LAN, on one machine and one
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
   Commander: bow MG (hold). Loader: lever, hot case, and the final ram. On foot: selected weapon.
2. **RMB = look closer.**
   Gunner: TZF magnification toggle. Commander & Loader: binoculars toggle (B is an alias).
   On foot: aim down the selected weapon's physical sights (hold).
3. **E = interact with the tank's body.** Equip the exact looked-at round, enter a station anchor,
   work the hatch LEVER, or mount from the ground. (LMB-clicking non-round interactables also works.)
   hand.) At the cupola the gaze priority is fixed: the **LEVER** (the diegetic hatch
   open/close) wins first, then the **station anchors** (CUPOLA/GUNNER/BREECH entry —
   available hatch open OR closed), then the **hatch disc** itself (commander only; a click
   pushes the lid open/closed — it never exits the tank). **Exiting** is deliberate:
   hold **X for 1.5 seconds** at the open cupola to climb out with the Luger.

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
| **B (Circle)** | back out one embodied layer; with a portable item equipped, throw it; in Stützpunkt, take over an open KIA post |
| **Y (Triangle)** | tap: head out; hold 1.5 s at the open cupola: dismount |
| **X (Square)** | P08 magazine change / draw a fresh Panzerfaust tube |
| **RB (R1)** | coax MG34 |
| **D-pad** | fine gun lay / menu navigation; left/right selects P08/Panzerfaust on foot |
| **View / Share** | raise/stow map |
| **Menu / Options** | controls/pause sheet |
| **stick clicks** | left: crouch; right: ignition |

USB is the zero-friction connection. On macOS, put the controller in pairing mode and use **System
Settings → Bluetooth**; on Linux use the desktop Bluetooth panel or `bluetoothctl`. The game can be
running before the pad connects. Solo/menu control follows the most recently used pad, while couch
crew seats retain their stable physical-pad identities.

## Physical stations

| Post | View |
|---|---|
| Driver | visor slit |
| Gunner | TZF sights |
| Commander | periscopes / cupola — *home* |
| Loader | interior walk + breech |

There are no number-key teleports. Looking at and clicking/E-ing the physical anchors (gunsight,
breech, cupola) is the path. **Q or Space** stands you
back up to the commander from the sights or the breech — Q is the universal EXIT: it pops
binoculars, racks an equipped tool, stands you up, or ducks your head back in (one layer per press).

## Everything else

| Input | Action |
|---|---|
| **W S A D** | drive, from every station (A/D combine with W/S); walks at the Loader / on foot |
| **mouse** | lay the gun (gunner) / head-look (commander, loader, foot) |
| **arrows** | fine gun-lay nudge (gunner) / fast scan (commander) |
| **C** (hold) | coax MG34, at the sights |
| **X** (tap) | head out of the cupola ↔ duck back in (commander; opens the hatch if needed) |
| **X** (hold 1.5 s) | climb OUT on foot — only in the cupola view with the hatch open; the HUD ring fills while you hold |
| **T** | ammunition reminder; actual AP/HE choice is the grey/yellow physical rack round you equip |
| **F** / **G** (hold) | extinguisher / field repair |
| **M** | raise/stow the held paper map and pocket compass; world keeps moving |
| **LMB on map** | draw permanent graphite with the pencil; there is no eraser |
| **Home** | controls overlay (pauses solo; local overlay in LAN) |
| **F6** | LAN: request the next open station; Stützpunkt: take over an open KIA post |
| **[ ]** / **F12** | retro dial / screenshot |
| **Esc** | close overlay → title; from that menu, Esc resumes the same solo run |

For deterministic visual comparisons, `KESSEL_RETRO_PRESET` accepts `clean`, `light`, `standard`,
or `strong`. This is independent of `KESSEL_INTERNAL_RES` (`640x360`, `800x450`, `960x540`, or
`1280x720`), so pixel density and post-process strength can be evaluated separately. `clean` keeps
the internal-resolution/nearest-neighbour presentation but fully bypasses posterization, Bayer
dither, and film grain.

In LAN play, **Esc** keeps opening the local controls sheet instead of leaving the shared
simulation paused. **Home** is always the direct controls-reference key.

## On foot (Absitzen)

| Input | Action |
|---|---|
| **W S A D** | move (running is the default gait) |
| **Ctrl** | crouch (slower, quieter, smaller) |
| **Space** | jump; advances the climb ritual at a perch |
| **LMB / RMB** | fire / aim down the selected weapon's physical sights |
| **1 / 2** | select P08 / Panzerfaust |
| **R** | P08 magazine change / draw a complete fresh Panzerfaust tube |
| **B** | binoculars |
| **E** | pick up the looked-at portable item; E again places it gently with no throw impulse; a fuel-bearing jerry can refuels only at the Tiger's rear service area |
| **Q** | with empty hands, cycle P08/Panzerfaust; with a portable item equipped, throw it (mass and throw speed follow the remaining litres) |
| **E** or **LMB** on a cupola | mount |
| **LMB at a broken track, with kit** | grab loose end → hold and pull mouse down → place replacement link → hammer pin |

Equipping a portable item occupies both hands and suppresses the selected weapon until the item is
placed or thrown. Refueling transfers only what the tank can accept; any remainder stays in the
same physical can. An emptied can remains in the world and can still be carried, placed, or thrown.

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

## Known gaps / next

- Ctrl in the tank is unbound (on foot it remains crouch).
- The reload, by hand (Loader view): hold LMB on the **breech lever** and move the mouse down to
  open it; pull the hot case; an unfired loaded round can also be pulled back out with **E**; look at
  the desired rack round and press **E** to equip it; carry the
  round to the mouth, align the crosshair, hold LMB and shove the mouse quickly forward to ram it;
  then hold the lever and move the mouse up to close. Lever grip and committed ram both lock view.
- Binoculars at the gunner are deliberately absent (the TZF is the gunner's optic).
- Commander/loader/on-foot binoculars use one stable **10° true field** (about a 7× screen-space
  enlargement from the open commander view). Raising them over an interior periscope magnifies its
  existing picture; it no longer changes that periscope's own projection.
- Rebinding UI is not implemented yet; keyboard/mouse and the semantic controller layout coexist.
- Track repair has its full outside interaction; the transmission, traverse, radio, and MG repairs
  still use their station hold actions and need their own wrench/electrical/optics minigames.

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
