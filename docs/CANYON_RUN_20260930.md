# Canyon Run — 30 September 2026

The title menu's **Canyon Run** uses Alex's existing Gaea canyon as the winter placeholder.
The future winter campaign map is not claimed to exist. This build is isolated at
`/home/alex/panzer/canyon-run-20260930`, based on `b7e4cd4` with the dark cabin lighting intact.

## Play

`tools/launch_current_build.sh --menu` opens the menu. Select **Canyon Run**; or use
`tools/launch_current_build.sh --singleplayer --canyon` to start directly.
The application menu has **Panzerrechnersystem — current build** and a direct
**Panzerrechnersystem Canyon Run (current build)** shortcut.
One human crewman operates the Tiger. The objective is to reach the far end alive in the tank.
Death/retry starts a fresh defence; completion fades back to the menu and records the time.
`KESSEL_CANYON_SEED=<integer>` pins a deployment for debugging; ordinary launches draw a new seed.

Inside, WASD walks the loader around the breech and the mouse looks freely. At a vehicle station,
WASD drives. Q returns from the gunner, driver or radio to the interior; at the cupola it first
lowers an exposed head. Physical station targets and hatch grips remain interactive. Held Space
climbs through an open access hatch. Reloading remains the ordinary physical sequence: open the
breech, extract and release the case, hold a fresh round at the tray, flick the mouse to ram it,
then release and close the breech. X raises the commander's head through the cupola.

## Course

The stable route is 4.93 km. Its readable sequence is a sheltered start, first gun approach,
ruined settlement, saddle crossing, factory patrol, blocking gun and final armour position.
The final third concentrates the defence: two T-34s patrol the factory, three hold the last basin.
Three manned 45 mm guns cover separate approaches; six additional riflemen hold building corners,
with the existing tank escorts and twelve gun crewmen. These are a challenge-course force, not a
claim about a recorded historical order of battle.

There are 53 standing buildings, four razed plots, 13 wrecks, 146 wall sections, 113 trees and
route-side logs, snow drifts and debris (696 placed assets). Buildings and ruins use the existing
urban grammar and sourced kit. The urban course's historical basis is documented in
`docs/research/STALINGRAD_URBAN_FABRIC_20260928.md`; the Gaea geology itself is an authored placeholder.
Landmarks remain stable so a new player can learn the route. Each run changes gun sites, rifle
posts, tank patrol starting points and patrol direction. A seed can be reproduced exactly.
Only the starting point is interpolated: normal patrol observation pauses remain at the
authored corners, instead of stopping the tanks every few metres.

Gun choices pass the existing ground, crew approach, line-of-fire, spawn safety and patrol
clearance probes, and must score at least 60% of the best site in their sector. Patrol knots are
projected onto the built scene's navigation grid, checked for support, then driven by the existing
perception/navigation AI. Enemies still have to see or hear the Tiger. Nothing fires at the start.

## Terrain and editing

`python3 tools/worldgen/gaea_canyon_run.py` generates
`assets/worldgen/workshops/gaea-canyon-run/`. It reads only
`/citadel/gaea_maps/panzer/canyon/Export.png` and preserves the original.
The 16-bit source retains its 5 km horizontal and 2.5 km encoded vertical scales, with a
centre-relative runtime datum. The runtime grid is 1025², spaced 4.883 m apart.
Local building pads and shell craters modify the working copy. The central travel strip preserves
the Gaea surface. The generator checks both terrain triangles and tank-width clearance, avoiding
the narrow ridges missed by coarse endpoint-only pathfinding.

The generator refuses to overwrite hand-edited outputs, including building/weather edits without
an `ops` entry. Use a separate `--dest` for a new version. The receipt records source/output hashes,
terrain dimensions, route grades, dressing counts and gun candidates. The route and seven design
stages live in `manifest_edits.json` beside the editable building instances.

## Fire, tools and movement

- Extinguisher grains are 12 mm instead of 8 mm; emission is 4,800 instead of 2,400 grains/s,
  with 8,192 airborne slots. Deposits still adhere to real contacted surfaces and obstruct spread.
- Fire spread probes run every 40 ms instead of 65 ms, with 16 attempts instead of 12 and faster
  local growth. Burning sources, ammunition ignition and foam coverage retain their authority.
- Rounded flame parcels rise, taper and curl continuously. Their actual mesh surface area drives
  the existing finite aggregate emission. This is a procedural visual representation of heat,
  not a fluid or thermochemical solver; see the physical-truth ledger.
- The carried hammer has a continuous windup, strike, recoil and return. The strike is aligned with
  the existing 0.24 s contact clock. Free swings do not invent repair credit or impact sounds.
- The loader starts inside the walkable floor, removing the first-step camera snap. Q also returns
  from the driver/radio. Winter enemy motion is checked after inertia and contact pushes, and
  riflemen use the existing static-cover/building/door collision queries.

## Verification

Evidence is under the flat `captures/` directory. The seeded GPU test verifies eight complete
deployments, safe starts, variation, replay, living-Tiger completion and reset. The end-to-end
drive test uses ordinary drive/steer with return fire disabled and no invulnerability or warps;
it is route acceptance, not proof that a person can beat the armed course. A separate staged
first-gun probe uses normal enemy fire and damage. Final results and live-window checks are
recorded in `captures/canyon_run_acceptance_20260930.json`.

The release build and 1,410 unit tests passed, as did the three generator checks and the two
explicit GPU course tests. The full drive finished in 803.38 simulated seconds; the separately
staged first gun fired five real rounds in 40 seconds. Reload-with-flick, foam suppression,
hammer swing, and cupola exit/entry passed their declared interaction checks. Recorded clips
use scripted gameplay inputs; the native Wayland check separately exercised the actual title
menu, X head-out, mouse look, loader WASD and Pause → Restart. Restart produced a new seed and
different gun positions. The checked start view ran about 58 fps at 1280×720 on HAL, which is
not a whole-course performance measurement. Living-Tiger completion, time storage, fade maths
and reset gates passed simulation/unit checks; complete native death/retry and finish transitions
after an armed human run remain unverified.

Terrain, cover and first-person interactions can be checked automatically; the final difficulty
still needs human play. The physics ledger retains the existing T-34 support/proxy limitations,
procedural fire coefficients and incomplete optical/light-transport work. This change does not
complete the broader engine roadmap in the previous handoff.
