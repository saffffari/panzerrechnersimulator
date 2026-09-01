# Terrain Pipeline — Gaea 2 realism probe → Kessel

Kessel has a validated, deterministic import boundary for a Gaea-style heightfield. It does **not**
contain a Gaea-generated landscape yet. Gaea 2 officially supports Windows 10/11 only. No supported
Windows Gaea host was reachable during this task, so this repository provides the contract and cook
but does not invent or substitute a terrain bake.

The geology source remains tool-agnostic: any tool may produce the same unsigned 16-bit PNG and
versioned sidecar. Gaea 2 is the intended authoring tool for the first 4 × 4 km realism probe.

## Runtime truth

There are two terrain entry paths in current code:

- Normal campaign play and the WORLDBUILDER both require
  `assets/worldgen/act<N>/manifest.v1.json`. That manifest identifies a row-major float32 square
  `heights.npy`, its sample spacing, and a required terrain texture. This is the authoritative path
  for Acts 1–3 and preserves game/editor parity.
- `src/terrain.rs::load_heightmap` is an older direct `heightmap.png` reader reached through the
  explicit legacy/Act-0 terrain path. Merely placing `assets/terrain/act1/heightmap.png` does **not**
  override a versioned Act-1 campaign manifest.

The offline probe cook bridges these paths honestly: it validates the Gaea delivery, decodes it to
the exact row-major float32 NPY grid the campaign manifest already consumes, and writes a candidate
manifest `terrain` fragment. It never edits or activates the live manifest.

## Source-delivery contract

Deliver one directory from the Windows/Gaea authoring machine:

```text
gaea-export/
  heightmap.png       # required: square, non-interlaced, native greyscale PNG16
  heightmap.json      # required: exact v1 sidecar below
  ground.png          # optional: square georeferenced PNG drape named by `drape`
  height_source.exr   # recommended private archive; not read by the runtime cook
```

Use [the checked-in exact example](../catalog/examples/terrain-heightmap.v1.example.json) as
`heightmap.json`:

```json
{
  "schema": "kessel.terrain-heightmap",
  "version": 1,
  "orientation": "north-up",
  "meters_per_side": 4000.0,
  "elev_min": -12.5,
  "elev_max": 137.5,
  "drape": "ground.png"
}
```

The machine-readable definition is
[`catalog/schemas/terrain-heightmap.v1.schema.json`](../catalog/schemas/terrain-heightmap.v1.schema.json).
The validator additionally enforces cross-field and binary-image rules JSON Schema cannot express:

- `meters_per_side` is the endpoint-to-endpoint width on both axes and must be positive. Sample
  spacing is exactly `meters_per_side / (resolution - 1)`.
- Pixel value `0` represents `elev_min` metres; `65535` represents `elev_max` metres, and max must
  be greater than min.
- `north-up` is not a label-only promise: the top row is world north (`-Z`), the bottom row south
  (`+Z`), the left column west (`-X`), and the right column east (`+X`). Do not rotate or mirror the
  PNG between Gaea and the cook.
- The centre pixel becomes runtime `Y=0`. Absolute elevations are retained in the cook receipt,
  while the runtime grid is centre-relative like the existing campaign terrain.
- The optional drape path is portable, relative to the sidecar, unable to traverse outside that
  directory, and must resolve to a square PNG.
- The height PNG must really be greyscale 16-bit. Promoting an 8-bit image into a 16-bit buffer is
  rejected rather than silently losing terrain precision.

## Validate and stage a campaign-ready probe

Validation is read-only and uses only Python's standard library:

```sh
python3 tools/gaea/heightmap_probe.py validate /path/to/gaea-export/heightmap.json
python3 tools/gaea/heightmap_probe.py validate /path/to/gaea-export/heightmap.json --json
```

To stage a candidate for Act 1, choose a **new** child directory below the act manifest. The cook
refuses to overwrite it and does not alter `manifest.v1.json`:

```sh
python3 tools/gaea/heightmap_probe.py cook /path/to/gaea-export/heightmap.json \
  --manifest assets/worldgen/act1/manifest.v1.json \
  --output-dir assets/worldgen/act1/gaea-realism-probe
```

The result is intentionally isolated:

```text
assets/worldgen/act1/gaea-realism-probe/
  heights.npy                 # little-endian f32, C/row-major, full source resolution
  ground.png                  # copied only when sidecar.drape exists
  heightmap.source.json       # source contract retained with the cook
  terrain.fragment.json       # exact candidate manifest terrain object
  cook.receipt.json           # hashes, scale, decoded range, activation=false
```

If the sidecar intentionally omits a drape, `cook` requires an existing repository texture via
`--texture-logical assets/...png`, because the versioned campaign manifest never hides a missing
terrain texture behind a flat fallback.

Activation is a separate reviewed edit: replace only the target manifest's `terrain` object with
`terrain.fragment.json`, then run the manifest/terrain tests and inspect the act. Do not activate a
probe on top of existing towns without re-grounding or regenerating placements: changing geology
can bury/float structures, roads, story scenes, spawn points, and loot. The first visual experiment
should therefore remain an isolated terrain probe; campaign promotion follows a worldgen/site merge
pass.

## Gaea 2 authoring boundary

Gaea owns slow-changing geology: the real DEM base, restrained erosion, balkas/gullies, and broad
hull-down undulation. Deterministic Kessel worldgen and scene blocks own gameplay-readable changes:
roads, embankments, craters, site pads, trenches, and authored scene terrain. Keeping those stamps
outside the opaque terrain bake lets AI, navigation, the paper map, and the editor reason about them.

Recommended first probe:

1. Set the Gaea terrain extent to exactly 4000 m square and work north-up.
2. Import a real regional DEM crop as macro relief; do not replace it with a synthetic mountain.
3. Apply restrained erosion and low-amplitude, gameplay-scale undulation. Preserve source-scale
   elevation units.
4. Archive the high-precision source as 32-bit EXR for Blender/reference work.
5. Export the runtime delivery as a square, non-interlaced, unsigned greyscale 16-bit PNG. Record
   the exact encoded elevation endpoints in `heightmap.json`.
6. Export a square north-up ground drape and useful masks (flow, wear, deposition, slope) beside the
   delivery. Masks are retained for later worldgen wiring; the current terrain cook does not claim
   to consume them.

Gaea 2 itself requires a supported Windows machine. Its documented automation entry point is
`Gaea.Swarm.exe`; a `.terrain` project must first be authored/built in the UI before headless
rebuilds. Follow the current official documentation rather than the obsolete Gaea 1 `Gaea.Build`
command:

- [Gaea system requirements](https://docs.gaea.app/install/install-gaea/system-requirements.html)
- [Gaea automation overview](https://docs.gaea.app/developers/automation/index.html)
- [Gaea command-line automation](https://docs.gaea.app/developers/automation/cli/command-line-automation.html)
- [Gaea → Blender heightfield guidance](https://docs.gaea.app/guides/use-in/apps/blender.html)

## Verification after an explicit activation

```sh
python3 tools/gaea/heightmap_probe.py validate /path/to/gaea-export/heightmap.json
python3 tools/gaea/test_heightmap_probe.py
cargo test --bin kessel terrain::tests
cargo run -- edit 1
cargo run -- screenshot out.png
```

Inspect world-axis orientation, centre datum, tank grounding, slopes at road/site boundaries, drape
alignment, and the full game/editor terrain identity log. A screenshot alone does not verify
collision or placement grounding.
