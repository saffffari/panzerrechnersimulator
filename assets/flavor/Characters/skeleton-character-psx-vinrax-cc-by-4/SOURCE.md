# Skeleton Character PSX — Kessel anatomy derivative

- Original title: **Skeleton Character PSX**
- Creator: **Vinrax**
- Source: <https://sketchfab.com/3d-models/skeleton-character-psx-ece576bbed4b4364911c7596d828a558>
- Sketchfab UID: `ece576bbed4b4364911c7596d828a558`
- Retrieved: 2026-08-30
- License: **Creative Commons Attribution 4.0 International** (`CC-BY-4.0`)
- License: <https://creativecommons.org/licenses/by/4.0/>
- Original archive SHA-256:
  `65825015009305fcc65db9eb524720306497c03ffadfdb2f1d0f733cb116e40d`

The raw ZIP and FBX stay in the private local/Hal asset warehouse and are not committed. The archive
contains a nested FBX package, so the baker gates the exact outer archive, nested archive, FBX, and
texture hashes before importing anything. The reviewed source has one 855-vertex / 1,596-triangle
weighted mesh, one 27-bone native armature, a 256×256 base-color texture, and no animation clips.

## Kessel modifications

`tools/bake_skeleton_character.py` preserves the unretargeted native skin as a self-contained rigged
authoring GLB. It then rotates only the source's existing upper-arm, forearm, and hand bones into a
relaxed/down pose, evaluates the native weights, and exports one static runtime GLB for Kessel's
current non-skeletal loader. Both outputs are 1.53 m tall, use glTF Y-up, face +Z, stand on Y=0, and
embed the supplied texture in one material. The source is already a PSX-scale 1,596-triangle mesh,
so the optimization pass deliberately performs no destructive decimation.

`baked/anatomy_alignment.v1.json` records all 27 source bone names, an explicit crosswalk to Kessel's
eleven casualty-body partitions, the triangle count assigned to each partition, and measured native
bone-landmark error against the 1.53 m crew physics centers. This is an approximate compatibility
measurement—not a claim that the skeleton was authored inside or exactly registered to the separate
AdrielCZ clothed crew model. The centroid partitions are deterministic render pages, not medical
segmentation.

Rebuild from the repository root after acquiring the asset through the creator's normal Sketchfab
download flow:

```sh
/Applications/Blender.app/Contents/MacOS/Blender \
  --background --factory-startup --python-exit-code 1 \
  --python tools/bake_skeleton_character.py -- \
  /path/to/skeleton-character-psx.zip \
  assets/flavor/Characters/skeleton-character-psx-vinrax-cc-by-4

python3 tools/verify_skeleton_character_bake.py \
  assets/flavor/Characters/skeleton-character-psx-vinrax-cc-by-4 \
  --archive /path/to/skeleton-character-psx.zip
```

The generated `bake.report.json` is the authority for Blender version, topology, bounds, material
closure, source/output hashes, and the non-exact registration statement. The visual QA preview is
written to `/private/tmp/skeleton-character-psx-relaxed-preview.png` and is intentionally untracked.
