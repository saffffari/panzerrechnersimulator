# Singleplayer crew arm animation

The shader Playtest's five singleplayer crew identities use the existing reviewed source character meshes and atlas. One Gunner hand follows the source elevation-wheel grasp under the same rotation used by the mechanical wheel. The Loader's mechanism-following arm prototype is **disabled by default** because its hands miss the actual carried round/case and ram positions. Ordinary player inputs continue to control these mechanisms; this implementation adds no autonomous loading, firing or crew navigation.

`src/crew_arm_animation.rs` skins the original seated vertices with the original source weights. `tools/bake_crew_arm_bindings.py` exports the 39-bone seated bindings from the verified FBX extraction and matches them to the existing GLB's 914 split vertices. Runtime skinning uploads only the bounded crew vertex buffers, at most once per 1/30 second, without changing topology, materials or draw count. Pose-matched arm segments also feed the existing hit volumes and the initial casualty pose.

The embedded derivative is `assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/animation/seated_arm_bindings.v1.json`. Keep it with the Rust source: clean builds require this file. Source licensing and extraction provenance are recorded in that asset directory's `SOURCE.md`.

## Validation and limits

- An independent CPU/source audit verified the neutral GLB hash, all five variants' identical vertex/UV layout, normalized source weights, acyclic bone chains, rigid arm transforms and joint endpoint mathematics. This does not verify rendered palm or finger contact.
- Two-link IK preserves limb lengths and clamps unreachable targets. A clamped hand may miss the object; the receipt reports that error rather than claiming contact. There is no authored finger curl or palm orientation solver, shoulder/torso reach correction, or full-body loading animation.
- The current Gunner animation covers the elevation wheel only. The separately located Commander traverse flywheel is not treated as a reachable Gunner control.
- `catalog/shader_reviews/crew_arm_mechanics_witness.v1.json` replays ordinary extraction, pickup, ram, second shot and elevation inputs. Its disclosed compartment witness is a render-only camera, not first-person gameplay footage. The real player camera still owns interaction and no mechanical state is injected.
- Witness checks require two Loader targets during case holding, round holding and ramming, and one Gunner target during elevation. Missing or invalid target errors are `null`, never zero. Passing numerical checks is not a substitute for viewing the rendered arms and contacts. GPU/visual acceptance must be recorded against that fixture's own receipt; the referenced older receipt validates only its baseline input timeline.

Singleplayer mechanic receipts list all five permanent identities, including Radio/Bow MG. The existing diagnostic anatomy UI retains its separate four-human couch roster; its layout has not been expanded by this change.

## GPU baseline reviewed 2026-09-13

The baseline [receipt](../captures/astra/crew_arm_witness_20260913_v1_manifest.json) and [video](../captures/astra/crew_arm_witness_20260913_v1.mp4) passed the ordinary reload checks, including two shots and five living crew. Loader target errors failed unchanged acceptance limits: maximum error 0.39048m at 4s (case), 0.171999m at 5s (round), and 0.53827m at 6s (ram). Frames at 3.917, 5, 6 and 12 seconds were visually inspected. The Loader visibly reaches into empty space while the round moves into the chamber, and its pickup pose dips into the lap/rack. This is an incomplete pose prototype, not a finished loading animation.

Gunner elevation contact had one target with 0m reported joint-to-grasp error at 12s and a coherent visible arm. The fingers are partly occluded; exact finger/palm contact was not established. Gunner animation remains enabled. Proper Loader support needs credible body positioning and regripping; it must not be solved by stretching limbs or relaxing acceptance limits.

For deliberate prototype inspection only, start a new process with:

```bash
KESSEL_LOADER_ARM_PREVIEW=1 /home/alex/panzer/shader-graphics/tools/launch_shader_playtest.sh --playtest
```

The same flag is required before renderer creation for the explicit Loader witness:

```bash
cd /home/alex/panzer/shader-graphics
KESSEL_RENDERER=shader KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator KESSEL_INTERNAL_RES=640x360 KESSEL_LOADER_ARM_PREVIEW=1 target/release/panzerrechnersystem mechanic-review captures/astra/crew_arm_preview_review catalog/shader_reviews/crew_arm_mechanics_witness.v1.json
```

Use a fresh output prefix for each run. The preview fixture deliberately retains its failed contact assertions. Without the environment flag, normal gameplay leaves the Loader in its reviewed neutral pose; no reload state or interaction is disabled. The flag is captured once per Game construction and never enabled by ordinary input or silently by the fixture.
