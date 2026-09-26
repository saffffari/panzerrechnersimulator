# Winter playtest controls and verification — 25 September 2026

This is the isolated shader worktree at `/home/alex/panzer/shader-graphics`.
It uses the canonical asset library without replacing the canonical game executable.

## Launch

```sh
/home/alex/panzer/shader-graphics/tools/launch_winter_session.sh --playtest
```

This starts the singleplayer winter combat encounter with three live T-34s and return
fire. The initial enemy reloads are staggered at 25/29/33 seconds. Their shots use the
existing observation, weapon-laying, projectile and armor paths. This is a staged
encounter: enemies initially know the Tiger's last position. Campaign saves retain
their separate route (`--singleplayer`); `--coop` starts the two-player mode.
Omit the mode flag for the menu. Close the active game before launching another.

## Controls changed or clarified

- **Shift:** hold to sprint on foot. **Ctrl:** crouch; crouching overrides sprint.
- **Space:** contextual climb to chassis and turret, enter an open hatch, descend from
  the inside cupola into the turret, or climb out while head-out. **LMB on the hatch:**
  open/close it. Occupied crew stations still block entry.
- **Reload:** hold LMB to carry and align the cartridge, then flick the mouse or right
  stick in any direction to ram. Holding still cannot seat it. The open breech lamp
  stays amber; green requires a closed, ready breech. Main-gun fire belongs to Gunner.
- **Exposed Commander:** hold RMB to raise the Luger, LMB to fire, R to reload,
  release RMB to holster. B remains binoculars. Drawing and firing simultaneously
  only draws the pistol; firing then needs a fresh LMB press.
- **Held map:** mouse wheel or +/− to zoom; controller D-pad up/down while held.
  The physical sheet is 30 × 18 cm, with 1–2.25× reading zoom and landmark labels.
- **Salvage:** aim at a loose 88 mm cartridge beside a Tiger wreck and hold LMB to
  pick it up. Keep holding through climbing and boarding. At Loader, aim at an empty
  rack cradle of the same ammunition type and release LMB to stow it. Releasing
  elsewhere sets/drops the cartridge down.

## Implemented world and cabin changes

- Winter grass uses sparse coherent clumps with clean snow gaps; no moving circle
  around the camera. Summer placement is unchanged.
- Authored tank wrecks use cached collision derived from occupied source-mesh cells.
  Runtime spare Tigers also block walking; their collider follows vehicle exchange.
- The authored winter aftermath includes scorched Tiger finishes, three detached-turret
  scenes and six slumped static crew figures in open hulls. All five original hull
  positions and 795 existing authoring operations were preserved.
- Wreck salvage is finite physical ammunition: nine AP and three HE rounds per selected
  Tiger hull, up to eight nearby hulls. These rounds are spilled beside wrecks, not an
  abstract inventory cache. The same body is picked up and transferred into a real rack.
- The cupola spindle retains its source metal material. Cabin paint/floor are darker,
  while the instrument and aperture lighting remains visible.
- Cabin fire has bounded connected spread and source-fed rising wall/ceiling flames.
  Haze and light follow the active fire. Extinguishing the source removes its dependent
  flames. This models a burning source and hot gas wash, not steel itself as fuel.

## Verification and limits

Source-Tiger GPU regressions exercised the actual hatch opening and traversal, carrying
ammunition through that route, all four ram directions, breech lever closure, remote
crew controls with the keyboard player outside, finite salvage pickup/stow, cupola
pistol flight, held-map rendering, and fire spread/suppression. Targeted input, gait,
grass, collision, fire and inventory tests passed. Physical gamepads were not hand-tested.

A 60-second winter simulation observed all three enemies moving and firing real rounds.
The existing individual-tank AI can approach very closely; this is not a new platoon
doctrine system. Historical penetration tables from the supplied notes have not been
imported as authoritative constants. See
[the reference review](ARMOR_DOCTRINE_REFERENCE_REVIEW_20260925.md).

New world ammunition is session inventory; cross-restart persistence and complete
network synchronization of salvage have not been added in this pass. Static wreck
collision is a movement/cover proxy, not a replacement for the playable tank's armor
damage mesh. Runtime spare collision covers the hull and turret for walking; this does
not add traversal onto every abandoned vehicle.

## Evidence

- `captures/astra/tiger_furnace_20260925_v4_1000ms.png` and `_overhead.png`: inspected
  actual source-Tiger fire lighting; the matching JSON records spread and suppression.
- `captures/astra/winter_grass_20260925_after_act1_bare_grove_ground_day.png`: matched
  winter grass review. At this selected clear-snow location, nearby stalks fell from
  1,836 to zero; this is not a claim that every location has no grass.
- `captures/astra/athena_cupola_p08_20260925_v1_aimed.png`: actual raised sidearm.
- `artifacts/runtime/salvage-scene-tests-20260925-v2.log`: pickup, full-rack rejection,
  compatible empty-rack stow and no duplicate round.
- `artifacts/runtime/hermes-physical-ram-gpu-20260925.log`: physical pickup, stationary hold,
  four flick directions, open/amber, then source-lever click to closed/green.

- `captures/astra/winter_wreck_20260925_exterior_v1.png` and
  `winter_wreck_20260925_interior_oblique_v1.png`: final-binary workshop renders, inspected.
  Their `winter_wreck_20260925_review_receipt.json` records authoring and visual checks.
- `captures/astra/athena_held_map_20260925_v3_goal_zoom.png`: final labelled map zoom.
- `captures/astra/winter_feedback_native_20260925_paused.png`: actual current native
  window, verified at its pause menu with Resume selected. The encounter log confirms
  three live T-34s, 96 physical salvage rounds and 14,726 world instances sharing 155
  GPU prototypes. This is not a measured frame-rate benchmark.
- `artifacts/runtime/winter-feedback-native-receipt-20260925.json`: exact binary hash,
  PID, launch command and observed-state limits.

The old interactive process was closed and the new singleplayer encounter launched.
The last verified native state is paused; select Resume in Panzerrechnersystem.
Controller hardware and remote LAN sidearm/salvage have not been playtested.
