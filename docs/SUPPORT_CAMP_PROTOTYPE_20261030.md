# Support camp prototype — 30 October 2026

`Game::build_expedition_act` explicitly adds the camp. The existing game, Playtest, legacy
campaign and tank-editor constructors do not load its private assets or alter their scenes.

The source layout has a medical tent with two stretcher beds, two quarters tents containing
sixteen empty cots, an open workshop awning, ammunition awning, separate fuel drums, orders
table/board, resting area and an unlit cooking stove. The reviewed halftrack transport source
is parked beside the load point. Canvas and frames are authored approximate field-camp props,
not a reconstructed historical tent pattern. Existing private stretcher, table, bench, crate
and drum models retain their source scale and textures; paths are recorded in `support_camp.rs`.

Furniture is not inventory. These meshes create no people, ammunition, fuel, parts, medical
stock or recovery capacity. The expedition's finite roster and stock remain authoritative.
The halftrack is currently static source geometry; this does not implement driving or towing.

## Installation and contacts

`SupportCamp::load()` returns inspectable camp-local meshes and anchors. Optional
`include_static_halftrack()` adds the reviewed transport. `append_to_scene(scene, root)` appends
the actual visible meshes and returns their slot range, named source parts, exact world-space
source triangles and transformed anchors. Root is a rigid level translation/yaw.

The live opt-in adapter searches actual source terrain around the Tiger's spawn. It checks
the entire source footprint on a one-metre lattice, admits at most twelve centimetres of
relief, checks map boundaries, source manifest bounds, existing obstacles and initial enemy
areas, then validates late world cover. Frame feet on a natural site sit at the lowest sampled
ground, with at most twelve centimetres of soil embedment. If no natural site exists, the
explicit expedition build can grade an unobstructed site with at most 1.6 m of initial relief.
The camp core becomes level, joined to its surroundings by an eight-metre smooth earth apron.
Source-grid spacing adds a clearance margin so interpolation cannot tilt the camp floor.
Both the terrain mesh and authoritative heightfield change. Native detail uses its existing
authored held-surface flag inside the cleared pad; old snow depth and road ruts are removed
there. Cellars/trenches and neighboring source placements are excluded from the whole apron.
No leveling collision box is inserted. The installed camp receipt records the altered node
count, bounds, elevation and maximum cut/fill; the source asset files remain unchanged.
If no unobstructed moderate site exists, map authoring is still needed rather than placing
tents through buildings or flattening a steep hillside.

Camp is appended before the legacy effect tail and before renderer construction. Its
collision query retains those very meshes in the existing source-triangle accelerator.
On-foot body/standing clearance and world sight checks use finite segment intersections.
Fabric blocks sight and walking through a wall; upward fabric contact does not become a
walkable roof. The existing foot controller samples body sweeps; it is not a continuous
capsule/cloth deformation solver. There is no invisible tent-sized solid box.

Static camp contact is not yet integrated into projectile material penetration, fire,
destruction, tank crushing or navigation for all NPCs. Canvas must not be treated as armor
when those systems are connected. The source triangles preserve Canvas/Solid classification
for that work; the latter is a contact category, not an armor material or strength claim.

## Medical and personnel contract

`Game::installed_camp_anchors()` exposes world anchors. `expedition_initial_camp_position()`
returns that installed origin when present. The medical beds have IDs 1 and 2, patient poses
on the actual stretcher canvas, and separate attendant points. Both beds and the medic's
station lie within ten metres of camp origin, matching coordinator admission radius.
The sixteen quarters anchors are furniture slots; actor instancing must map finite PersonIds
to those slots rather than create decorative anonymous personnel.

`camp_people.rs` now allocates sixteen hidden source draw slots only in an Expedition build.
The live roster makes only living, capable people at the current camp visible. The controlled
person, five bound field identities, and medical patients are excluded, so retained field and
stretcher bodies are not duplicated. Every visible actor has the existing rank, surname and
trained role. Current camp rendering supports the sixteen initial identities; extending the
roster requires an explicit dynamic slot/identity extension.

The actors use the original source skin through `camp_people_rig.rs`. Local duty, rest, salute,
and walk-to-load tasks do not generate cargo or perform a strategic dispatch. Navigation uses
bounded local A* with exact camp source-triangle body sweeps and finite actor separation.
The named medic follows the current triage patient to the selected bed's actual attendant
point. Live treatment waits for that arrival; casualties continue evolving during the walk.
Walking animation advances with actual distance, and a zero-time capture preserves its pose.

Public interaction hooks are `camp_person_hover(eye, dir)` and
`camp_person_interact(eye, dir)`. Both require actual source triangle hits and clear sight;
the interaction makes an available person salute and walk to a distinct loading position.
The root input/HUD caller must route these before ordinary on-foot firing. The method does
not board anyone or allocate ammunition. `camp_people_receipt()` records actual identities,
positions, goals and pending route waypoints. Field projectile damage to these reserve actors,
ragdoll conversion, general on-foot body blocking, and identity appearance continuity when
changing from a camp slot to an existing field slot remain unintegrated. These limitations
must be resolved before presenting camp reserves as combat-complete NPCs.

`CampPlacement::relocate(root)` transforms anchors/source triangles and returns the matching
mesh model transforms. The live adapter does not expose relocation yet: it must first move
existing patients, people, inventory and parked equipment consistently, without resetting
the run or replenishing its stock.

## Review

`support_camp_capture::run(prefix)` writes six standard-renderer source views and a JSON
receipt without a game or audio device. Register the `camp-review` CLI at the root. This is
an asset/layout inspection on a neutral ground fixture, separate from a live map walk test.
Outputs remain flat under `captures/`; no source textures are collected or copied.

CPU regressions in `support_camp.rs` cover source-triangle correspondence, relocation,
doorway gaps, finite geometry and medical station radius. An ignored private-asset test
loads the real props. `camp_terrain_pad.rs` checks drawn-grid/collision correspondence,
preserved outside nodes and agreement of native detail inside the authored pad.
Compilation, GPU review and a live camp traversal remain required
before calling the prototype visually or interactively verified.

Additional CPU tests cover finite actor admission, exact triangle picking, source skin/bind
consistency, and grounded gait. The ignored `medic_and_reserve_routes_use_actual_openings_and_props`
test loads the private camp assets and checks all initial reserve routes and both medical
beds at two camp orientations. No CPU or GPU execution of these latest actor changes was
performed by the camp agent; the root retains serial build/capture ownership.
