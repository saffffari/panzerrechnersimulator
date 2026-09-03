# Kessel — The Embodied Interior (design, 2026-07-02)

> **Current crew authority (2026-09-02):** `docs/DESIGN_PILLARS.md`, `docs/STATUS.md`, and
> `docs/LINUX_4090_HANDOFF.md` supersede this document wherever crew count or casualty behavior
> differs. The campaign begins with one persistent five-man Tiger crew; four is only the local-couch
> human-post limit, and no replacement crew exists. The embodied stations and interactions below
> remain useful, while the historical one-body prototype section is explicitly retired.

Status: **locked design, staged build.** From the user's first real playtest session: *"the
switching between inside and the sights and having to perform operations inside the tank IS the
entire game — that's what's different between this and War Thunder."* This doc turns the current
hotkey view-switcher into a physical place.

## The thesis

The tank interior is a **walkable first-person space**, exactly like the on-foot mode outside but
bounded by whatever room the turret gives you. Stations are not views you hotkey between — they
are **places you walk to and instruments you physically touch**. Every transition the player makes
(ground → hull → turret → hatch → fighting compartment → optic) is embodied. Intensity comes from
the body: when a shell is coming and you're at the breech, the sight is *across the compartment*.

## Interaction model

- **Crosshair:** a single white pixel, center screen, in the walking modes (inside + outside).
- **Stations are clickable 3D elements.** Hover the crosshair on the gunner's optic → the reticle
  changes to an interaction cue → **click to enter** the sight. Walk under the cupola →
  click the cupola → you're up in it. No more Shift/C/Q view hotkeys — ALL station keys removed
  once this lands.
- **No hover lighting:** interaction feedback is UI only. It may not make the object emissive, add a
  proxy marker, or inject light into either surfaces or atmosphere.

## Enter / exit — the hatch is the only door

- **Exit:** ONLY via the top hatch, **Spacebar** from the cupola/hatch position.
- **Enter (the climb ritual):** on foot near the tank → text prompt ("CLIMB UP [SPACE]") →
  Space climbs onto the **hull deck** → you can stand/walk on the hull → Space with the crosshair
  on the **turret** climbs onto the turret roof → Space with the crosshair on the **hatch** drops
  you inside. Three physical stages, no teleport-to-seat.
- Climb stages are instant pose changes (PS1 honesty — no canned animation), but each stage is a
  real position ON the vehicle; you can stop halfway and stand on the engine deck.

## Build stages (sequenced)

**Stage 0 — user's Blender pass (GATES everything):** the Tiger interior gets cleared/roomier
geometry so a body fits and can move: fighting-compartment floor, station clearances, the
under-cupola well. Interior collision comes from this mesh.

**Stage 1 — climb ritual + hatch door (no interior walk yet):** ✅ BUILT (2026-07-02). A timed,
camera-scripted climb ladder, symmetric both ways — each Space is a committed ~0.55 s move (the
vulnerability window). MOUNT (ground→in): hull deck → turret roof → drop into the hatch. DISMOUNT
(in→out): out of the cupola → grab the rim → onto the turret roof → down to the ground (the last
descent also triggers on a walk-off). E begins the ritual; Space advances it; free mouse-look
throughout; prompts read the next action. On-foot **jump** (Space when free) with real gravity
(`foot.vel_y`/`airborne`); running is the default gait, crouch is the stalk. Perch cameras are
derived from the live hull/turret transforms (visually verified on the turret roof). The Last
Tiger remount + bail now route through the ritual. Verifier: `cargo run -- climb`. Still lands you
in today's station system on entry — hotkeys work underneath until Stage 2 removes them.

**Stage 2 — clickable stations:** crosshair raycast against annotated triangles on the actual optic
head, cupola well, breech, and driver's visor; reticle cue; click-to-enter; remove the view hotkeys.

**Stage 3 — the interior walk:** on-foot movement inside the compartment volume (reuse
`FootSoldier` with an interior collision bound from the Stage-0 mesh), stations entered from
standing positions, eye-height at scale with the real interior. The Absitzen controller is the
implementation base — inside is just outside with walls.

**Stage 4 — the operations follow the body:** the manual reload, extinguisher, repairs all happen
at their physical spots (the hotspot system already models this — it becomes load-bearing instead
of decorative).

## Blender interior rig manifest (the bake-extractor contract)

The interior `.blend` (user-authored, 2026-07-03/04) exposes named objects the extractor pulls out
with their pivots. **Convention (same as the tank bake): each moving part's ORIGIN sits on its
rotation axis** so the engine spins it in place via a local transform, never orbits it. Fixed
detail rides its parent mesh (turret parts rotate with the turret; hull parts stay put).

| Object | What it is | Driven by / pivot |
|---|---|---|
| hull / turret split | turret sub-tree rotates with traverse; hull sub-tree is fixed | `Tank::turret_model()` vs hull model |
| `cupola_pivot` / `Kommandantenkuppel` | commander's cupola | rotates with turret; parent of the 8 periscopes |
| 8 periscopes (under `Kommandantenkuppel`) | vision blocks with explicit glass/prism geometry and physical optical material; never emitters | material name keys wavelength-dependent transmission/reflection |
| cupola hatch hinge | two-step open: **lift 0.145 m up the post, then swing 90° about Z** | `hatch_t`; committed |
| `gunner_hatch_pivot` | rarely used; opens **up to 95° about X**, closed default | optional open state |
| `turret escape hatch pivot` | side turret escape hatch | open/close for bail |
| `Drivers Hatch Pivot` | driver's overhead hatch | opens 90°; enter/exit for the Easter-egg driver station |
| `gun_pivot` (carries the coax MG + **breech**) | gun trunnion / elevation | `Tank::gun_model()` |
| breech (labelled, lives under `gun_pivot`) | the ONLY interior obstacle | one box collider from its world AABB, snapped to the resting gun pose (traverse+elevation locked while on foot inside) |
| `Round in the chamber` (mats `brass`+`bullet`) | THE 8.8 cm round — the ONLY textured shell mesh; also its own transform = the chambered **seat pose** (ram END). Instanced everywhere: rack slots + the in-transit reload round + the ejected casing | baked once; long axis local Y (~1 m); rack slot = ram START (1.1 m back), lerp to seat |
| **`Shells` collection** (existing rack shells — **shells ONLY**) | the ammo stowage LOAD; model already has shells racked at correct positions, just untextured | extractor pulls `bpy.data.collections["Shells"].all_objects`, reads each transform, HIDES the original, instances the textured `Round in the chamber` mesh there (axis-corrected). Slots deplete as you fire; this rack is what the fire cooks off (`FIRE_TO_AMMO`). ⚠️ collection must be shells only — a round is instanced on EVERY member (keep mechanicals like `Gun wheel1` out); exclude `Round in the chamber` if present |
| `Gun wheel1`, `Gun wheel2` (handwheels) | manual elevation + traverse cranks — the gunner's pair plus (likely) the loader's traverse-assist wheel. NOT a commander wheel: the Tiger had hydraulic power traverse (engine-driven) + hand backup | spin with gun pitch / turret yaw (traverse wheels turn in tandem); grab-to-aim controls later; origin at hub, axis = spin axis. **Design hook:** engine dead (bailed/dying Tiger) → no power traverse → slow hand-crank only (hundreds of turns for 360°) = Last-Tiger tension. NOT in `Shells` |
| monocular gun sight (scaled up) | the gunner-optic station | Station::Gunner camera head |
| `TURRET FLOOR` | walkable floor — **its outer bounds ARE the walk boundary** (turret-only) | stand-plane height + XZ footprint (point-in-outline) |
| `TPZ dial indicator` (needle, renamed from `Cube`, mat `dial red`) + `turret position indicator backing` (mat `dial back`) | **TPZ turret-position indicator** — azimuth dial (the LIVE one) | needle rotates by turret-yaw-relative-to-hull; origin at dial centre, axis = face normal; grouped with `Turret_Pivot` so it rides the turret. A SECOND dial baked into `Turret_0008` stays **static** (merged geometry — rotation ditched, that's fine) |

Walls come from the existing armor shell (cull-none renders the interior faces; a `front_facing`
normal flip shades them right). Shipping interior collision must be generated from the same visible
triangle geometry: floor/walls use the mesh triangles and the articulated breech uses its live
transform. The current floor-outline + one-box implementation is recorded debt, not exact physics.

**Ground truth from the saved `.blend`** (`assets/tanks/tiger/Panzerkampfwagen_VI_Tiger_Ausf.E.blend`,
inspected headless 2026-07-04, 1552 objects). Hierarchy: root `Panzerkampfwagen VI Tiger Ausf. E`
→ `Body_Pivot` (HULL) → `Turret_Pivot` (TURRET, nested under Body_Pivot). **Turret group = anything
under `Turret_Pivot`; hull = the rest.** Gun elevates/recoils via `Gun_pivot` → `Gun_Recoil`, and
`Breech` rides `Gun_Recoil`. Round = ONE object `Round in the chamber` (mats `brass`+`bullet`, ~1 m
along local Y) — instanced for rack/ram/ejected; its own transform = the seat pose. TPZ dial = needle
`Cube` (mat `dial red`, origin already ≈ dial centre) + `turret position indicator backing` (mat
`dial back`); both currently **unparented roots** → to be parented under `Turret_Pivot` and the needle
renamed `dial_needle`. `TURRET FLOOR` = 1.6×1.6 m, parented to `Turret_Pivot` (rotates with turret —
fine while traverse locks on foot). **User pre-authored a full interior point-light rig** (~18 `L_*`
POINT lights — but several were Fable's earlier gamey pass and get CUT under the "only real light"
directive ([[kessel-sim-authenticity-light]]). **Extractor whitelist (real sources only):**
- **Daylight apertures are geometry, never lights.** Exterior environment radiance enters only along
  unoccluded transport paths through the modeled slit/glass/prism assembly, then bounces off the real
  interior materials. `L_Cupola_Daylight` and `L_Slit_Driver` are forbidden stand-ins.
- **Instrument backlights** — `TPZ light` (turret-position dial, read from the cupola/gunner). Faint,
  kept. *(Driver gauges `L_Dial_*` and radio `L_Radio_*` deleted — see cupola-driving note.)*
- **Dome lamp** — `L_Turret_Dome` 0.4 W (Innenbeleuchtung), mostly a NIGHT source.
- **Gunner's sight reticle lamp** — `L_Sight_Gunner` (dimmed): the TZF sight had an illuminated
  graticule for low-light aiming. Real, kept (as a dim warm point, not a fill light).
- **Turret indicator lamps** — small colored bulbs for firing-circuit / traverse warnings:
  `L_Turret_Indic_Grn/Red`. Real, kept as tiny colored points.
- **Ignition flavor light** — one small **red** point at the driver's ignition/starter (`L_Ignition`),
  kept as an indicator-lamp glow (user flavor, reads as a real panel lamp).
- **Muzzle flash + fire** — dynamic, already in the point-light system.

**Light energy uses one radiometric contract.** Import source dimensions, color temperature and
measured/declared output into the engine's physical units; source radius regularizes the emitter and
does not define an authored reach. Camera exposure and the PS1 crush change the recorded image, not
transport. Brightness is calibrated by changing a real fixture or exposure—not by view-state gain.

- **CUT:** crew-position fill `L_Commander/Driver/Gunner/Loader/RadioOp` (0 W, invented), the stray
  root 1000 W scene `Light`, and the front-hull **driver + radio/MG dedicated lamps** (`L_Dial_*`,
  `L_Slit_Driver`, `L_Radio_*`) — deleted **for now**. The **`periscope glass` apertures are KEPT
  everywhere, including the driver + MG ports**; environment light may reach the compartments only
  when the shared scene geometry and optical material permit it.

**Driving + Easter-egg stations.** Primary driving is from the **commander's cupola** (you command
the tank from up top, not by physically moving to the driver's seat). BUT the **driver's seat** and
the **MG-operator seat** are being added as optional **Easter-egg stations** you can enter and try —
click-to-enter (walkable area stays turret-only; the hull seats are reached by station transition,
not by walking there). The MG seat is the point of that helplessness beat: blind but for the MG's
own sight, the commander's orders in your ears, nothing you can do. Their dedicated lamps stay
deferred; for now they receive transported environment light plus the red ignition fixture. The
`Drivers Hatch Pivot` (90°) gives the driver's seat its own enter/exit.

## Interior material palette (art direction, 2026-07-04)

Authenticity + the light-bounce logic ([[kessel-sim-authenticity-light]]). **Author every albedo at
its TRUE value — never pre-darken to "match the gloom," or light stops bouncing and it goes to mud
through the crush; let the lighting do the darkening** ([[kessel-dark-psx-albedos]]).
- **Upper walls + ceiling** — ivory / *Elfenbein* (warm off-white ~`#D2C6A8`), historically painted
  pale to BOUNCE the dim daylight around the compartment. Grimed toward `#9A9078` in corners, worn to
  bare metal at hand-contact points.
- **`TURRET FLOOR`** — raw worn STEEL, heavily weathered (the filthiest surface, NOT ivory). Metallic
  ~0.9, dark neutral-grey base ~`(0.20,0.20,0.21)`, roughness varied: oil-black glossy (~0.2) at
  breech/traverse gear, boot-polished (~0.35) down the walk path, grimy matte (~0.85, faint brown) in
  the corners; optional red-brown primer at the edges. Collects ejected brass through a fight.
- **Mechanicals** (breech, cradle, traverse gear, handwheels) — oil-darkened steel grey / blued metal
  (≈ the current look; leave them).
- Light above, dark below = the authentic German scheme, and it reads great through the crush.

## Live vision ports (design locked 2026-07-04)

Two tiers, both governed by the same transport geometry:
- **Direct slits** (driver visor, pistol/MG ports): the user cuts REAL HOLES in the mesh; the slit
  glass material renders see-through. The camera inside genuinely sees the exterior through the
  opening (cull-none makes this near-free). The letterbox-of-daylight dread.
- **Periscopes + gun optics** model their redirected optical path: prism/mirror normals, glass IOR,
  Fresnel reflection, absorption, and the objective/eyepiece stops determine which exterior rays reach
  the eye. A render-to-texture camera pasted onto emissive glass is a known current proxy and is not a
  shippable solution. The TZF sight uses the same optical transport plus its physical graticule lamp.
- **Station views may fill the screen** only as the camera moves into the actual eye box; they do not
  bypass or replace the optic.
- Optics and daylight share geometry, not invented energy: an optic can transmit exterior radiance
  into the eye/interior and reflect it, but the glass itself is not a daytime emitter.

## Vision-slit glass blocks (Schutzglas — spec'd 2026-07-04, awaiting the model rebuild)

The real construction (user observed it in the model): a slit **narrow outside, funneling wider
inward** through the armor, backed inside by a thick **laminated bulletproof glass block** (~90 mm,
green-tinted, in a steel holder, replaceable — MG fire on slits crazed them; the glass is what kept
bullet splash/lead spray out of the crewman's eyes).

Replication: **user models** the slit as a wedge/frustum (narrow outer face → wide inner mouth) and
keeps each glass block a separate object with the `periscope glass` material. **Engine**: the bake
exports the blocks as part GLBs (not dropped like the open ports); the glass BSDF models Fresnel
reflection, refraction/transmission, green laminate absorption, thickness, and roughness. Pixel
quantization happens only after HDR transport; checker discard and sun-driven glow are forbidden.
Later hook: blocks DEGRADE under MG fire on the slit
(craze → whiten → spent), the enemy shooting at your eyes.

## The physical reload (Stage 4 mechanic — user design, 2026-07-04)

Reloading is a manual act at the breech, not a timer. **Numbers (from the model):** the round is
~1 m long → the ram stroke is ~1 m forward; the ready/grab pose is **1.1 m back** from `Round in
the chamber` along the bore. Intent: *"physicsy"* — you must line the round up and seat it
correctly — but **bounded**, so a calm loader always makes it and only a panicked fumble under the
cook-off clock costs a life.

No rigid-body sim (determinism is sacred, no `rand`). It's a guided manipulation that mirrors how a
real tapered chamber loads:
- **Hold** — round at the ready pose, base near your hands; a small **deterministic sway**
  (sum-of-sines, the `fire_glow` idiom) means you can't hold it dead still — that's the physical feel.
- **Steer (mouse)** — moves the round's TIP in the breech-facing plane (pivots about the base); you
  aim the nose at the breech mouth.
- **Ram (hold key)** — pushes ~1 m forward along the round's own axis.
- **Seat gate = the chamber funnel** — catch the nose inside the breech-mouth radius; once caught,
  the tapered chamber self-centres and an assist guides it home. Tolerance is generous at the mouth,
  tightening with depth — **the skill is the initial catch**, not wrestling it the whole way.
- **Miss** — nose hits the breech ring off-centre → clunk + small bounce-back, reposition. The cost
  is TIME, never damage.
- **Difficulty knob** — mouth-catch radius + sway amplitude, tuned against `FIRE_TO_AMMO` (5 s):
  calm ≈ 1.5–2 s, fumble 3–4 s, a bad fumble under fire can kill. (Optionally: the funnel assist
  widens as the cook-off timer runs low, so tension stays on the clock, not on fiddly controls.)
- Seating fires the existing breech-open haze puff + a solid seat/close **CLACK** (sound pillar).
- **WEIGHT (user, 2026-07-04)** — the round is ~15 kg. While one is **in your hands** (LMB-hold pickup
  → carry → ram), your **look/turn sensitivity drops below the normal free-look speed** — heavy,
  deliberate, can't-whip-it-around. One `look_sens *= carry_factor` (~0.5) gate while carrying; it
  reinforces the funnel-catch difficulty (you can't flick the nose in, you have to guide it) without
  any extra rule. Same slow-down for the pickup itself (LMB-hold to hoist a round off the rack).
- **SPENT BRASS is pickupable (user, 2026-07-04)** — fired casings eject onto the floor and
  accumulate (they already litter the `TURRET FLOOR`); LMB-pick one up to **clear it out from
  underfoot**. Housekeeping under fire — historically a real chore (brass piled up, rolled around,
  fouled footing and the traverse). A small immersive verb, and a reason the floor gets messier the
  longer a fight runs. (Where you toss them is cosmetic; the point is clearing the walk space.)

## The handheld kit (same pillar, 2026-07-02 playtest)

**No full-page overlays, no HUD compass.** M raises a **physical map in your hands** — a paper
view-model like the Luger's slot, opening up in front of you; the world stays visible around it.
Your right hand holds the **pencil** instead of the pistol (strokes draw on the paper as before).
On the left, a **physical compass** in hand — a real needle that swings with your facing; a
compass rose printed on the map itself. The screen-top compass strip is REMOVED. Implementation:
render the map content (terrain sketch + landmarks + strokes) to a texture once + on stroke
change, mapped onto an in-hand quad; compass = small procedural body + needle box driven by yaw.

## Historical solo-body prototype (retired 2026-09-02)

The following records the superseded 2026-07-02 one-body implementation. It is retained only for
the useful position-dependent spall and acoustic observations; it is not the current crew design:

- The spall cone checks your body at the seat you're posted at (later: wherever you're standing).
  **Position is the defence**: a hull penetration while you're up in the sights is a near-miss you
  *hear*; the same round while you're at the driver's visor is death. The cone was tightened
  (vertical band 0.25–0.95, off-axis floor 0.12) so a compartment away genuinely protects.
- The parametric hull CLANG already scales with your seat's distance to the entry; its stereo pan
  is now relative to YOUR HEAD (camera), not the hull — the plate you face rings ahead.
- Tank `hp` is now the MACHINE (modules + fire). The man is binary: alive, or the kill-cam.
- Death: spall catches your body / ammunition detonation / the fire takes the tank.

## Interior haze / smoke (2026-07-02, "absolutely critical")

The buttoned-up fighting compartment fills with propellant smoke as you fire — an accumulator +
fan model (user's design, refined): `interior_haze` 0..1.
- **A shot** adds a little (+0.06, muzzle backdraft); **OPENING THE BREECH** to reload adds the
  big puff (+0.16 — hot gas venting off the ejected casing). So it couples to the reload ritual.
- **Drain**: the bore-evacuator FAN pulls a steady 0.055/s; an **OPEN HATCH** vents 0.40/s more
  (natural draft) — pop the hatch to clear the smoke fast, at the cost of exposure. Ties the two
  systems together.
- Sustained rapid fire without venting → you're **firing blind in your own smoke** (the veil
  saturates). The Tiger's slow reload usually lets the fan keep pace; a panicked fast string
  doesn't.
- **Render**: a veil in `crush.wgsl`, ONLY when buttoned up inside (gated by `interior_light` — it
  never appears on foot or head-out, so it is strictly an interior effect, not world fog).
  It is a VOLUME, not a flat wash: (1) DEPTH-CONFINED — the crush pass samples scene depth and the
  smoke pools thick on NEAR interior geometry (breech/barrel) and thins toward the far world seen
  through the optic; (2) DYNAMIC — two drifting value-noise octaves churn the density on `cr.frame`
  so it curls and moves, living smoke not static milk; (3) roof-heavy (smoke rises) + bright
  sources bleed their colour into it (`lit_smoke`, the outline glow). `haze = interior_haze ·
  interior_light`. Inert (skipped) when haze≈0. Verifier: `cargo run -- haze`.

**Hover highlight** (2026-07-02): ✅ BUILT. A STEADY emissive silhouette OUTLINE (no pulse) — an
inverted-hull rim: `outline_pipe` (front-face-cull, no depth write), the mesh shader inflates
along the local normal (params.w=6) and emits a warm-amber HDR rim, drawn as a sub-pass after the
mesh pass. Because it's bright HDR the BLOOM haloes it, and the interior-haze veil's `lit_smoke`
term makes the smoke around it GLOW (it "glows into the volume" — user). `renderer.set_outline(idx,
intensity)` is the per-mesh hook — reused by every Stage-2 clickable station. Zero cost when
nothing is hovered (drawn only for meshes with outline>0).

## Interior richness — mechanics backlog (ideation 2026-07-02)

All in the embodiment language (objects, not menus), roughly ordered by leverage:

1. **Shell types at the rack** — AP vs HE picked BY HAND from different rack positions (SPEC C1's
   loot-by-type feeds this). Wrong round up the spout when armor appears = a decision under fire.
2. **Ready-rack restock** — the 6-8 ready rounds by the breech deplete; restocking from hull
   stowage is a two-trip carry you do BETWEEN fights (or desperately during one).
3. **The wound** — you can be HIT without dying (MG splinters, grazing spall): bleeding tunnels
   vision + shakes the lay until you bandage (scavenged medical supplies finally matter inside).
4. **Cigarettes as nerves** — light one (physical: match flare in the dark interior) to steady the
   gun-lay tremor after a near-death beat. The scavenged treasure gets a use. Night: the ember is
   VISIBLE through your open hatch.
5. **Manual traverse crank** — hydraulic traverse module dead → the hand crank at the gunner spot
   still works: LMB circles to wind the turret around, slow and loud.
6. **Periscope/vision-block spares** — a hit crazes your optic (cracked-glass overlay); swap a
   spare head in by hand from the spares bin.
7. **Cold starts** — winter engine: a stalled/cold engine needs the primer + starter ritual at the
   driver's spot; stall it during a knife fight and the silence is the loudest thing in the game.
8. **Hatch state as a system** — ✅ BUILT (2026-07-02, cupola hatch): H opens/closes the cupola
   lid (also a click target when the crosshair hovers it — it pulses). The lid swings on a SIDE
   HINGE about engine-Y (Blender-Z, the vertical), pivoting on the cylinder off to the side (not
   like the flat hatches). Open VENTS the fire (climbs ~0.6× slower — the reason to pop it) but
   EXPOSES you: +18% detection signature, and a penetrating round's spall/overpressure can catch
   the commander through the open cupola (forgetting to button up is its own death). You MUST open
   it to BAIL. Head-out (C) requires it open; closing it pulls your head in. Verifier: `cargo run
   -- hatch`. **The lid mesh is a placeholder box** — the baked Tiger already molds a cupola+hatch,
   so the real animated hatch is a separate mesh from the Blender re-bake, dropped into
   `slots.hatch` with the existing swing transform (turret_model · T(hinge) · Ry(swing)).
9. **The radio, physically** — tune the dial across static to find German traffic (Enigma-flavour
   diegetic story delivery per kessel-envstory; no readable text — voice and Morse).
10. **Frost + breath** — optics and vision slits frost over with time buttoned-up; wipe with a
    sleeve (LMB hold). Your breath fogs in the cold interior — heavier when wounded/afraid.
11. **Fuel transfer pump** — hand-pump between internal tanks when one is holed; a slow hold-to-
    operate beat with the fire risk hanging over it.
12. **Grease-pencil kill rings** — YOU paint the barrel rings on the gun after confirmed kills
    (physical pencil, same tool as the map!).

## Notes

- The climb ritual doubles as a **vulnerability beat**: you're exposed on the deck, and tank crews
  now hunt the dismounted man (coax bursts) — mounting under fire is a decision, not a menu.
- The kill-cam X-ray, crew/module stations, and spall model already agree on interior positions
  (`CrewRole::station()` / `ModuleKind::station()`) — the walkable interior should use the SAME
  coordinates so the damage model, the X-ray, and the player's body all live in one interior.
- T-34 (and act-3 wingman Tigers) keep the simple exterior rig — the embodied interior is the
  player's Tiger only.
