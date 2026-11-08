# Editor and web update — 2026-11-04

The web editor in `/home/alex/panzer/web-playground/web` now uses reduced copies of the authored Tiger and five crew models. Native materials are approximated with Three.js PBR, three cabin lights and shadow maps. A 640-pixel-wide internal viewport is the default. This is a visual port with bounded hit/damage approximations, not a port of the native tissue, layered FEA, raymarch transport or full canyon simulation.

Native range changes:

- Reset explicitly uploads the closed cupola lid, spindle and grip matrices before the next frame, including while paused.
- Clicking the impact plus reveals azimuth/elevation rings. Red azimuth, white elevation, full-disc picking and faint active fill replace the sphere handle. Guides rasterize on the internal game-pixel grid with Bresenham lines.
- The section center translates along the projected plane normal. Rotation remains on its arrows. An end-on normal uses vertical screen movement to control depth.
- Live slow motion reduces the time step as well as the simulation budget. The range temporarily limits the nested projectile and hot-fragment fixed clocks to the actual editor step, avoiding repeated poses at very low time speeds. Campaign clocks retain their existing constants.
- The bottom bar places an orthographic cutaway and authored crew figures beside the brand; other controls group against the right. The orange viewport instruction overlay is removed.
- HUD images are rendered from the same Tiger/crew assets as the web view, with no ground and transparent alpha. Separate masked orange/red layers use native module functionality/faults and crew wounds/death. Existing module grouping is approximate at render-mesh level. The current embedded plan is 135 × 240, nearest sampled; the 16-image study runs from 540 × 960 to 1 × 2.
- A closed 128-segment annular lip overlaps the hull/bearing joint. Source `Turret_0113` is approximately radius 1.222 m and spans hull y=1.935..2.122 m; source `Hull_0708` spans y=1.908..2.102 m. The added collar spans radius 1.17..1.285 m and y=1.91..2.055 m. It is an asset-fit correction, not a measured historical reconstruction. It participates in the normal rendered hull geometry; the original GLBs remain untouched.

Checks: six targeted native unit tests cover section translation, axial section orientation, slow-time cadence, review lamp behavior, eye effects and radial ring closure/open compartment. The GPU `editor-controls` probe additionally checks actual live projectile positions at 0.1×, full-disc picking and renders the UI. `cupola-reset` checks rendered matrices without a simulation tick. Web checks and asset provenance are recorded in the web README and QA JSON.

Voxel work is separate and not implemented in this change. The current anatomy source is 10 mm; the interactive path coarsens it by factor 3 to 30 mm (2,946 cells in the inspected build). Uniform-covered outer cells still double as skin. Previous cloth tearing work did not create independent uniform and skin layers. The requested half-size target needs to distinguish 15 mm interactive cells from a 5 mm anatomy source.

Artifacts remain local. Screenshot/PNG exports are in `/windows/shop/panzerrechnersystem/captures/screenshots/`; native stable binaries and settings are backed up before installation. No published release or performance guarantee is implied.
