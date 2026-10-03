# Turret sidearm and hatch audit — 2026-10-02

The hotfix extends the existing shared P08 to the standing Loader and Commander posts.
V draws/holsters, LMB fires on a fresh press, RMB aims, R uses the existing magazine/chamber
reload, and Q holsters before leaving the post. With the pistol holstered, RMB retains the
existing physical optic focus. The head-out Commander's held-RMB shortcut remains available.
Occupied hands, climbing, switching stations, and death prevent drawing. The pistol consumes
only the local operator's overlapping weapon/service inputs; other crew channels remain live.

The launch guard sweeps the camera-to-muzzle segment against the retained, articulated Tiger
source triangles. A muzzle visually pushed through a shut hatch cannot spawn a round outside
it. A blocked muzzle spends no round. After launch the existing finite-flight P08 bullet still
queries the Tiger's own geometry; it has no mounted-cannon self-hull exemption. This is a source
triangle segment guard, not a swept solid handgun collision or external-world weapon-pose solver.
The existing P08 source model, magazine, reload, casing, flight, impact and sound are reused.

CPU gates are `game::cupola_pistol::tests` (input edges, local crew ownership, real moving/sloped
escape-hatch obstruction) and `game::small_arms_runtime::tests` (source cupola/crew contact).
Compilation/results are recorded separately; native interaction remains a playtest gate.

## Partial reload and clear aiming view

R already accepts a partially used P08 magazine, including while RMB is held, in
both the mounted sidearm and on-foot paths. The shared chamber/magazine model
retains a chambered round and tops up to eight magazine rounds plus that chambered
round, limited by reserve ammunition; it never creates or discards rounds. New
CPU regression coverage checks 250 magazine/reserve/timestep combinations, transfer
at every stage, held-R restart prevention and the firing lock during a reload.
No unnecessary replacement of the existing reload state machine was made.

The shared aimed hold is lowered rigidly by 12 mm at the existing 40 cm extension.
Both foot and turret poses use it; the complete model and launch muzzle move
together, with the camera-to-muzzle armor guard unchanged. Source-triangle tests
check central aiming clearance at 360/450/540 native pixel heights and retain the
original notch/post geometry. The pose adds no screen-space crosshair, geometry
cutout or independently moved projectile origin. Native visual acceptance is still
required after the coordinated build.

On foot, a nearby map case/searchable shelf consumes R for its displayed search
interaction; binoculars, compass or carried repair kit also prevent pistol reload.
Inside the turret, draw with V first (or use the existing head-out RMB shortcut),
then R reloads in either low-ready or aimed posture. These context rules are
unchanged. CPU filters: `game::p08::tests` and `game::cupola_pistol::tests`.

## Existing aperture protection and limits

- Incoming small arms compare live source Tiger armor/lid/glass contacts against the posed crew
  body volumes. A real open aperture can admit a shot; a closed lid stops it first. The existing
  regression directly exercises the closed/open cupola under hull/turret movement.
- Exterior HE currently applies its gameplay casualty law to infantry and the dismounted player.
  It does not propagate external blast damage through a roof/escape aperture into mounted crew.
- The exterior pressure-response path additionally requires on-foot or head-out Commander state,
  plus a clear source-to-listener segment. A below-rim listener is excluded even when an opening
  provides line of sight. This is not an aperture-resolved pressure solver.
- Interior penetration fragments have ballistic motion and crew contacts, but their cabin bounce
  enclosure still uses compartment proxies. Nearby explosion debris does not become a general
  damaging fragment population that can enter any open hatch.
- Audio switches from Buttoned to HatchOpen once any real hatch phase exceeds 0.2. Its enclosure
  losses are broad estimates, not a per-aperture diffraction/pressure computation.

These limits explain why a hatch may have a clear bullet effect while nearby explosions appear
less sensitive to it. They are not changed by this control hotfix.

## Deferred asset and posture work

Panzerfaust stock is deliberately gated by the scenario's `panzerfaust_in_service()` selection.
The current winter scenario starts without it; the later homeland scenario still has its authored
rack and interaction. No missing asset was restored or chronology changed. The rack's spare-round
representation remains an authored gameplay choice, not a verified historical stowage claim.

An MP40 FPS GLB was found in the documented canonical source warehouse:
`tank_simulator/source_assets/legacy-imports/asset-warehouse-20260831/incoming/2026-08-31-new-assets/mp-40_fps_animation.glb`.
Its embedded metadata credits FOXYSCA and CC-BY-4.0, with a Sketchfab source link. It includes
one 51-joint skin and a Reload animation. The sibling source ZIP includes a Blender file and hand
textures, but no sound. It is not in the admitted runtime closure and has not been visually
validated or integrated. It is intentionally outside this relaunch hotfix.

Loader, Driver and Radio roof lids can already open, but their controls do not yet provide the
Commander's intermediate raised head/body posture. Adding this requires consistent eye, body,
listener and traversal state. Merely permitting the pistol at those seats would not implement it.
