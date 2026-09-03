# Panzerrechnersimulator 0.1.0

`v0.1.0` is the first curated GitHub source release of **Panzerrechnersimulator**, whose current
game/runtime name is **Kessel**. The Rust crate and executable remain named `kessel`; changing those
identifiers is not part of this release.

## Source authority

This release reconciles the newest gameplay/editor work from the active Apple Silicon Mac checkout
with the RTX 4090/Vulkan compiler and portable-volume fixes already validated in Git commit
`8228ae4`. A checksummed comparison against Hal found no newer or unique release-source code there.
The Mac is therefore the active development/source authority; Hal is the Linux/Vulkan render,
training, and batch-simulation mirror.

## Implemented game surface

- A playable three-act campaign carries one persistent Tiger, damage, stores, discoveries, and
  route state through generated objectives and inset exits.
- Solo play, a four-seat local split-screen battle, and host-authoritative LAN crew play share the
  Commander, Driver, Gunner, Loader, and optional Radio/Bow-MG station simulation. The four-player
  layout omits the Radio/Bow-MG seat. A surviving crewmate can take over a KIA station while the
  casualty remains physically present.
- Armor, penetration, spall, crew injury, ragdolls, blood grains, fire, repair, crushing,
  destructible structures, manual loading, optics, interior atmosphere, and directional procedural
  audio are live systems, although their production tuning remains in progress.
- Dismounted movement, physical jerry cans, rear refuelling, mass-dependent item throwing, the P08,
  Panzerfaust combat, Soviet riflemen, foot-role AI checkpoints, and campaign foot-transition data
  capture are implemented. The split-screen battle is still tank-only; independent couch-player
  body ownership for campaign dismounts is not complete.
- The world/editor share act manifests, terrain, paged instancing, scene-block contracts,
  destructible-building foundations, loot/state contracts, and procedural route/site generation.
- High/Vulkan uses a raster primary G-buffer plus exact triangle/material ray-query lighting,
  one-bounce diffuse/reflection estimators, physical sky/weather transport, source-visible media,
  and temporal convergence. Laptop/Metal uses the lower-cost forward path without substitute proxy
  geometry.

The selected CC BY 4.0 Soviet soldier has an attribution-preserving rig/bake/verification pipeline,
but its authenticated Sketchfab source GLB is not in this release. Until an operator supplies that
download and runs the bake, Soviet infantry use the existing procedural presentation.

## What GitHub contains

GitHub contains the engine/game source, WGSL shaders, tests, tooling, documentation, small textual
validation receipts, campaign and scene-block JSON contracts, fonts and notices, and the reviewed
CC0 German crew source/bakes. It does **not** contain the complete local runtime asset warehouse,
build caches, media captures, training checkpoints/datasets, owner-development apps/DMGs, recorded
sounds with unresolved rights, the restricted raw
Pizza Doggy libraries, or tank/terrain data whose redistribution provenance is still unresolved.

A source checkout is therefore not the same thing as the full visually populated development game.
For owner development, point `KESSEL_ASSET_ROOT` at the retained local asset warehouse (or use the
documented repository layout). Public/friend binary packaging remains closed until the gates in
`packaging/README.md` are cleared. This boundary prevents a source-control push from becoming an
unlicensed asset redistribution.

The repository currently has no project-wide open-source licence file; publication makes the
source viewable but does not grant reuse rights beyond the licences attached to individual
third-party files.

## Known production gaps

- Most campaign landmarks still need full authored interiors, scene dressing, and final encounter
  distribution; one enterable/destructible building is the proved vertical slice.
- Four-player split-screen campaign dismount/body ownership, full controller matrix testing, and
  clean-machine LAN/package QA remain release work.
- High is not yet a full multi-bounce path tracer; full 3D atmosphere/cloud transport, complete RT
  membership for every dynamic effect, glass roughness/internal bounces, and snow/ground coupling
  remain explicit renderer debt.
- A fresh public, signed, notarized, self-contained game build is not supplied by this source tag.

For the detailed implementation ledger, see `docs/STATUS.md`. For asset and redistribution gates,
see `docs/THIRD_PARTY_ASSETS.md` and `packaging/README.md`.
