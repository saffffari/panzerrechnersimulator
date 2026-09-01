# Scene blocks

A **scene block** is a relocatable piece of authored evidence: one room, a building and yard, or a
multi-building outdoor compound whose props, interiors, sockets, connectors, and sculpted ground
share one datum. It is not a fixed map coordinate and it is not itself a historical claim.

Every finished block must obey the first principle in `CLAUDE.md`: communicate one sourced human
event without player-facing words, and retain the primary/authoritative source locator in
authoring-only `metadata`. Do not invent a scene to fill a slot.

Provenance also controls **where** the evidence may truthfully appear:

- `exact_landmark` is locked to its sourced `act_id`, landmark ID, coordinate, and tolerance.
- `district_event` may move only among matching district/feature tags.
- `attested_condition` is a recurring sourced condition and may move among compatible tags.

Record source IDs, HTTP(S) source URLs, and lowercase `placement_constraints` such as
`urban_rail`, `airfield`, `bridge_approach`, or `pine_forest`. The browser labels these blocks
`[LOCKED]`, `[DISTRICT]`, and `[PORTABLE]`; a locked block is rejected away from its exact anchor.

## Contract

- Copy `assets/worldgen/scene_blocks/starter.scene-block.v1.json`; validate against
  `scene-block.schema.v1.json`. Runtime accepts only `schema: kessel.scene-block`, `version: 1`.
- One **ground anchor** is `[0,0,0]`; engine +Y is up and +Z is forward. Every item transform,
  socket, connector, and height sample is relative to it. World coordinates are forbidden.
- `items[].dy` is height above the single anchor datum. Unlike a quick F6 prefab, the pieces are
  never re-draped individually, so floors, stairs, furniture, bodies, and exterior relationships
  cannot shear on a slope.
- `terrain.heights_m` is a square row-major `res*res` grid in metres above the datum. Its first
  sample is local `(-X,-Z)`. The importer rigidly rotates this grid with the scene and blends the
  edge into the full-map heightfield through a smoothstep `apron_m` (minimum two samples). The
  apron reaches exactly zero at its outer edge: one continuous grid, no crack or hole geometry.
- Sockets are semantic landing points inside the block (map case, spawn, interaction, light,
  damage evidence). Connectors face outward and describe roads, paths, trenches, rails, doors,
  utilities, or water edges that procedural layout may join later. They do not spawn gameplay yet.
- GLB paths must be canonical repository-relative paths below `assets/`; `..`, absolute paths,
  backslashes, non-GLB payloads, and missing files make the entire scene-block load fail before a
  partial stamp. Put textures inside the GLB.
- Keep the authored footprint within 10 km of the origin, use positive finite scale, do not bake
  sun/fog/world terrain into a prop, and do not reference another scene block recursively.

## Rhino 8 handoff

Make a Rhino block whose base point is the scene anchor. Model in metres, Z-up, with a named
`SCENE_ANCHOR` at the origin and named points/axes for every socket and connector. Export meshes as
binary glTF with embedded materials, then convert coordinates to engine space:

`engine (x, y, z) = Rhino (x, z, -y)`

Export the local terrain surface as a regular square height grid at a documented spacing; heights
are relative to the anchor plane, never sea level. Keep the stitch apron free of irreplaceable
story props because that band is intentionally reshaped to meet the destination map.

There is currently **no direct `.3dm` importer or Rhino-to-scene-block exporter**. GLB export and
the JSON transform/height list are still a manual handoff. That exporter is the next useful tool;
the runtime contract is now stable enough to target.

## In-engine workflow

F6 remains the fast, terrain-draped **Prefab** workflow. A valid JSON under
`assets/worldgen/scene_blocks/` appears separately under **Scene Blocks** in the TAB browser.
Drag it into the map or select it and press V. The editor validates every asset, stamps all props
as a selected group, applies the terrain grid and stitch apron, rebuilds render/collision/contact
shadow terrain together, and prints touched/core sample counts plus maximum height delta.

Press **H** for the dedicated local terrain mode. The numbered tools are **1** raise/lower,
**2** smooth, **3** flatten, **4** deterministic noise, and **5** terrace. Hold LMB to sculpt;
Shift+LMB lowers with tool 1. The wheel changes radius and Shift+wheel changes strength. H accepts
the terrain session. Esc restores every height sample to its state when the mode was entered.

The live green ring is the brush. Once a sample changes, a cyan square shows the exact square core
that F7 will carry and an amber square shows the outer stitch-apron perimeter. Terrain edits update
the CPU surface immediately and rebuild the visible mesh, collision heightfield, ray picking, and
contact-shadow terrain together when the stroke ends.

Select every prop belonging to the site, then press **F7**. WORLDBUILDER writes an odd-resolution,
sample-aligned `kessel.scene-block` v1 JSON with one stable ground datum, the sculpted height grid,
selected rigid prop transforms, and a three-sample/minimum-12 m stitch apron. It registers the
terrain instance in `manifest_edits.json` and saves the overlay immediately. F5 deliberately
refuses an uncaptured terrain sculpt, so reload cannot quietly discard hand work. A captured block
is rescanned into the **Scene Blocks** browser category immediately.

F7 output is marked `draft_requires_primary_source_provenance`; replace its placeholder source ID,
URL, and placement constraint before approving it as a historical scene. Props remain ordinary
placement ops in the source map while the `scene_blocks` instance replays the terrain, preserving
legacy manifest and F6 prefab compatibility. When the block is stamped elsewhere, both props and
terrain come from the versioned JSON.

The math foundation supports arbitrary yaw, but today a browser drop starts at 0 degrees and later
G/R/T transforms affect the selected props only; they do not yet rewrite the persisted terrain
instance. Reposition a placed block by changing/removing its `scene_blocks` entry and re-stamping,
or wait for linked block-instance handles. F7 captures one square up to 513×513 samples; split a
larger landscape into neighboring authored sites instead of silently cropping it.
