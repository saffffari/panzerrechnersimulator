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
Radio/Bow-MG operator, is deliberately omitted from this mode:

- **Keyboard + mouse — COMMANDER.** Commander controls head-look, binoculars, hatch/head-out, and
  the held map. Dismount is locked for this shared-tank mission so the other three posts never lose
  their host simulation. Until a dedicated Driver pad joins, the Commander also covers driving on
  WASD.
- **First pad — GUNNER** (own viewport: the TZF). Right stick lays the turret at the real hydraulic
  rates, **RT (R2)** fires the 88 (edge-triggered), **RB (R1)** coax, **LT (L2)** steps the sight
  magnification, and **X (Square) or A (Cross)** works the breech between shots until a dedicated
  Loader joins.
- **Second pad — DRIVER** (own viewport: the visor port). Left stick drives and **R3** controls
  ignition.
- **Third pad — LOADER** (own viewport: the breech). Hold **X (Square) or A (Cross)** to eject the
  hot case and drive the next round home; the Gunner is then free to stay on the sight.

Layout: 2 players = left/right halves (each a center crop of a full frame — square pixels at any
16:9 window). 3 players = commander keeps the tall left half; gunner and driver stack the right.
4 players = a 2×2 Commander/Gunner/Driver/Loader grid. Every viewport is a complete PS1-crushed
frame with its own temporal history. Solo pad shortcuts are disabled while seats are dealt because
those buttons belong to the crew. The pads hot-plug: a pad connected mid-run takes the next seat on
the spot; a disconnected pad keeps a black placeholder so the other views never jump cells. With
**no** pad ever connected the op runs as an ordinary solo mission.

Crew loss opens the dead crewman's post instead of ending the mission immediately. A surviving pad
player presses **B (Circle)** to take over the next open KIA station; the keyboard Commander uses
**F6**. Taking over vacates the survivor's previous post, so the reduced crew must deliberately
choose which job to cover.

A controller survivor inherits the controls of the post they take. In Commander that means right
stick free-look, held **LT (L2)** binoculars, **A (Cross)** hatch, **RT/RB (R2/R1)** emergency bow
MG, and **R3** ignition.

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

Controllers are hot-plugged through the Linux gamepad stack. Xbox and PS4 pads use the same
semantic layout; the names below use Xbox labels, with PlayStation equivalents in parentheses.

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

USB is the zero-friction connection. For Bluetooth, put the controller in pairing mode and pair it
through KDE Bluetooth; it may also be paired from a terminal with `bluetoothctl`. The game can be
running before the pad connects. If multiple pads are connected, the most recently used one owns
the local crewman.

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
| **Tab** | open/close the left-side asset grid; every Pizza pack, Sounds, Prefabs, and Scene Blocks has its own tab |
| **/** / **[ ]** (browser open) | focus asset search / cycle pack tabs |
| **P** (browser open) | start a nonblocking preview bake for the current tab; the editor reports when it finishes and reloads the cache |
| **drag thumbnail / V** | place on the map; from a Tiger seat, ray-place onto the interior/exterior and inherit the hit hull/turret/gun frame |
| **G / R / T** | move / rotate / scale a map object or selected Tiger prop |
| **G / T / [ ]** (sound selected) | move the sound source / resize its audible radius |
| **- / =** (sound selected) | lower / raise source volume |
| **, / .** (sound selected) | shorten / lengthen duration; zero uses the source clip's duration |
| **L** (sound selected) | cycle loop-while-inside / once-per-playthrough / replay-on-re-entry |
| **H** | enter terrain sculpt; H again accepts, Esc restores the session |
| **1 / 2 / 3 / 4 / 5** | raise/lower / smooth / flatten / deterministic noise / terrace |
| **LMB** | sculpt under the green ring; Shift+LMB lowers with raise/lower |
| **wheel / Shift+wheel** | terrain brush radius / strength |
| **cyan square / amber square** | exact captured terrain core / outer stitch-apron perimeter |
| **F7** | capture the uncaptured sculpt plus selected props as a versioned scene block and save |
| **F6** | save selected props only as the older terrain-draped prefab format |
| **F5** | save ordinary edits; refuses while a terrain sculpt still needs F7 capture |

Missing model previews show as pending instead of freezing the Mac editor. The offline thumbnail
pass writes JPEGs through the game renderer; sound tiles use lightweight waveform cards and never
enter the 3D thumbnail renderer. Run `cargo run --release -- thumbs` for the full preview pass, or
add a pack/category/name after `thumbs` to bake only that subset. The full pass is intended for the
workstation rather than the live Mac editing session.

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
