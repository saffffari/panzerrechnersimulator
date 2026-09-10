# Tiger driver and loader roof lids

The player Tiger now partitions two complete authored assemblies from the existing baked GLBs
before renderer upload. The original files, triangle positions, hierarchy transforms, UVs and
material references remain unchanged. The hull/turret remainder cannot retain a second static
copy of either selected subtree. Other Tiger instances still use the original closed meshes.

| Part | Source scene root and descendants | Moving triangles | Hinge in parent frame | Open motion |
| --- | --- | ---: | --- | --- |
| Driver roof lid | `tiger_hull.glb`: `Drivers Hatch Pivot`, nodes 6, 0–5 | 21,140 | (1.382032, 1.9149045, 1.8893634) | 95° about -Z |
| Loader roof lid | `tiger_turret.glb`: `gunner_hatch`, nodes 9, 4–8 | 18,334 | (-0.7036, 2.4564, 0.1235) | 95° about +X |

The source calls the loader-side lid `gunner_hatch`. Its GLB object origin is at the lid center;
the rotation instead uses the authored hinge in `tiger_interior.json` at `hatches.gunner.pos`.
The driver assembly is already rooted at its hinge. The 95° resting angle and easing are runtime
animation choices. There is no booleanned opening, replacement lid or interactive Blender edit.

`gltf_load::load_glb_partitioned_roots` splits named scene roots in one import and retains the
ordinary loader's material behavior. This does not repair that loader's existing first-material
limitation on the static multi-material hull/turret. Each moving assembly uses one source material;
both receive the same existing German-grey runtime repaint as the rest of the Tiger. The resource
catalog already contains the original GLBs, so no new asset payload is required.

`Game::debug_set_roof_hatches(driver, loader, instant)` selects open/closed states for captures.
With `instant = false`, the ordinary update loop eases the lids; with `true`, it sets the animation
phase immediately. They follow hull/turret transforms and the frozen replay pose as normal opaque
meshes. Opening the driver lid does not automatically move the driver's head outside.

## Checks

Run `python tools/validate_tiger_hatches.py`. This CPU check reads the actual source accessors and
hierarchy, reports SHA-256 hashes, preserves the triangle sum (3,234,818 hull and 372,655 turret),
and tests a vertical line through each aperture. The closed moving mesh obstructs each line;
the opened mesh and the static remainder leave it clear. This proves those sampled authored
openings exist, not that every lid edge or possible collision has been exhaustively certified.

The checked source hashes are:

- Hull: `51938f43eb90ba321ff452c2be953d06673f7aab670465835144cf8ce88c9a84`.
- Turret: `20f86e9bacd1487869d42b59e40f86e0548af0a3626e57c250272b21f546e195`.

Rust tests in `tiger_hatches.rs` verify closed-pose identity, fixed hinges under rotated parent
frames, upward opening and reversible motion. Integrated renderer captures remain necessary to
judge the animation and lighting visually.

## Current limits

These are real rendered/shadow geometry changes. They do not yet extend the coarse tank ballistic
collider, pressure model, hatch ventilation multipliers, network snapshots or ordinary interaction
bindings. The existing commander's cupola hatch and exposure controls remain separate. The turret
escape hatch remains static. An open-roof capture must not imply those missing couplings are
already simulated or historically validated.
