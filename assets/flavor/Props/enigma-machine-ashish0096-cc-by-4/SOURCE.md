# Enigma Machine

- Original title: **Enigma Machine**
- Creator: **Ashish0096**
- Source: <https://sketchfab.com/3d-models/enigma-machine-30b85c037a164174a109ad6f002b9c47>
- Sketchfab UID: `30b85c037a164174a109ad6f002b9c47`
- Retrieved: 2026-08-30
- License: **Creative Commons Attribution 4.0 International** (`CC-BY-4.0`)
- License: <https://creativecommons.org/licenses/by/4.0/>
- Reviewed source FBX SHA-256:
  `bf1a983d7cc73ff21be771c2d2101bd575cab403a4b2497c44b743b467c2c5f0`

The unmodified FBX and 4K source texture set remain in the private local/Hal asset warehouse. They
are not committed. The source is already physically scaled in metres and depicts the machine in
its open wooden transit case, with a roughly 0.43 m × 0.29 m footprint and a 0.54 m open height.
Its front faces Blender `-Y`, which exports as Kessel/glTF `+Z`.

## Kessel conversion

`baked/enigma_machine_runtime.glb` is an adapted, self-contained runtime derivative. The reviewed
source has 147,636 triangles in many disconnected control, key, case, and mechanism objects. The
bake triangulates those objects, applies a deterministic per-part collapse budget, retains every
source mesh object, and joins the result into one 10k–25k-triangle mesh. This keeps small parts from
vanishing while making the prop suitable for a tank interior.

The source's box and mechanism PBR texture families are resized into three embedded 1024² atlases:
base color, glTF metallic-roughness, and tangent-space normal. DirectX normal-map green channels are
inverted for glTF. Source ambient-occlusion maps are deliberately not baked into base color; scene
occlusion belongs to the renderer's physical lighting. The six-triangle glass panel is represented
as a dark smooth opaque dielectric because the current one-material runtime path cannot preserve a
second transparent shader. The generated report names this limitation rather than claiming
refraction that the runtime does not compute.

The output is glTF Y-up, faces `+Z`, stands on local Y=0, and uses one material so Kessel's current
first-material-wins loader preserves the whole prop consistently.

Rebuild from the repository root with the exact reviewed inputs:

```sh
blender --background --factory-startup --python-exit-code 1 \
  --python tools/bake_enigma_machine.py -- \
  /path/to/low.fbx /path/to/textures \
  assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked

python3 tools/verify_reviewed_static_glb.py enigma \
  assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked/enigma_machine_runtime.glb \
  assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked/enigma_machine_runtime.report.json
```

The generated report is the authority for exact input/output hashes, Blender version, topology,
bounds, atlas construction, material assignment, and per-part decimation.
