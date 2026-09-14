# Tank garage editor source inventory

Verified 2026-09-13 in `/home/alex/panzer/shader-graphics`. This records asset availability and the source import boundary; it does not certify every model as a playable tank. The garage editor uses `src/tank_editor_scene.rs` as its catalog and save/load seam. No wreck-library donor or procedural replacement is used for the full-interior inspection entries.

## Open a garage and edit it

From the title screen choose **Editor**, then **Tiger I garage**, **Tiger II garage**, **IS-2 garage**, **T-34/76 garage** or **Panzer IV garage**. The same chooser includes **Act One world**. A separate editor window opens while the title screen stays available. The running game tracks one editor child at a time.

Direct commands work from any directory and use the shader checkout:

```sh
/home/alex/panzer/shader-graphics/tools/launch_shader_playtest.sh --garage tiger_i
/home/alex/panzer/shader-graphics/tools/launch_shader_playtest.sh --garage tiger_ii
/home/alex/panzer/shader-graphics/tools/launch_shader_playtest.sh --garage is2
/home/alex/panzer/shader-graphics/tools/launch_shader_playtest.sh --garage t34_76
/home/alex/panzer/shader-graphics/tools/launch_shader_playtest.sh --garage panzer_iv
```

Use `--menu` for the chooser, `--editor` for the selected Act One world, and add `--build` after source changes. Only run one launcher at a time; its existing lock protects the current game/editor session.

The toolbar provides **Orbit**, **Walk**, **Inside**, **Assets**, **Lighting**, **Save**, **Play** and **Home view**. Start with **Inside** (or **F8**) to inspect the vehicle, use **Assets**/**Tab** to choose a prop, close the browser, then aim and press **V** to place it. The upper-right **High** or **Low** label reports graphics quality, followed by the render resolution.

| Action | Control |
| --- | --- |
| Orbit around the tank | Hold MMB and move the mouse; RMB also works |
| Pan the orbit pivot | Shift+MMB |
| Zoom orbit | Mouse wheel |
| Switch Walk / Orbit | F or toolbar |
| Enter interior inspection / Tiger seat | Inside or F8 |
| Move in garage Walk | WASD; Q/E down/up; Ctrl crouch; Shift faster |
| Look in Walk or Inside | Mouse; Esc releases the pointer |
| Select at the first-person crosshair | LMB |
| Place selected library asset at the aimed surface | V |
| Asset browser | Assets or Tab; `/` searches |
| Move / rotate / scale selected prop | G / R / T; LMB confirms, Esc cancels |
| Choose tank-prop rotation axis | R, then X, Y or Z; mouse left/right rotates; default axis is Y |
| Duplicate / delete selected prop | C / X or Delete |
| Lighting controls | Lighting toolbar for environment; select a lamp for its local controls |
| Selected lamp range / intensity / temperature | T or [/] / − and = / comma and period |
| Selected lamp on/off / flicker | L / K |
| Save the vehicle scene | Save or F5 |
| Save and test | Play or F10 |
| Return to initial view | Home view |

First-person selection/placement uses the central crosshair; the same workflow works in the world editor. Orbit keeps a free cursor for picking. The browser temporarily releases mouse capture. On the Tiger I garage, Inside starts at the loader camera: 1–5 select commander, gunner, loader, driver and radio positions; WASD drives the tank and the arrows move the gun. Switch to Walk for free inspection and prop placement around it. Other full-interior sources use a movable inspection camera, not simulated crew positions. T-34/76 has exterior inspection because a crew interior has not been verified. While rotating a tank prop, X selects the X axis instead of deleting it; LMB commits the rotation and Esc restores its previous transform.

**Tiger I Play** opens actual isolated Tiger gameplay with the saved scene. It does not start the Act One mission. Close that test-game window to resume editing. **Other vehicles' Play** saves and enters a source walkthrough in the editor, with editing disabled; **Stop** or **F10** returns to editing. It does not fabricate gun, hatch, driving or crew mechanisms for an unconfigured source model.

## Available vehicles

| Catalog ID | Actual source used | Asset capability |
| --- | --- | --- |
| `tiger_i` | `assets/tanks/tiger/baked/tiger_{hull,turret,gun}.glb` from the canonical game warehouse | Existing articulated Tiger I, retained interior and current full crew gameplay. Authored hatch/optic mechanisms have separate runtime rig handling. |
| `t34_76` | `assets/tanks/T34-76/baked/t34_{hull,turret,gun}.glb` from the canonical game warehouse | Real baked articulated exterior. No verified playable crew interior. |
| `tiger_ii` | `assets/tanks/editor/tiger_ii/source_full.glb` in this shader checkout | Complete supplied geometry advertised as full interior; static inspection model, no configured gameplay or hatch rig. |
| `is2` | `assets/tanks/editor/is2/source_full.glb` in this shader checkout | Complete supplied geometry advertised as full interior; static inspection model, no configured gameplay or hatch rig. |
| `panzer_iv` | `assets/tanks/editor/panzer_iv/source_full.glb` in this shader checkout | Panzer IV Ausf. G complete supplied geometry advertised as full interior; static inspection model, no configured gameplay or hatch rig. |

Source discovery stayed inside the game warehouse. Other existing Soviet runtime builders do not establish more ready full-interior assets: `build_kv2`, `build_is2`, `build_is3` and `build_isu152` use procedural rigs; `build_su100` references baked paths that were absent in the inspected warehouse. Older complete flattened vehicles under `assets/tanks/wrecks/` are not substitutes for the full-interior sources listed above.

## Full-interior archive provenance

Archive root:

`/home/alex/panzer/tank_simulator/source_assets/legacy-imports/asset-warehouse-20260831/incoming/2026-08-31-new-assets/`

| Vehicle | Archive → nested archive → source | Objects | Preserved triangles | GLB size |
| --- | --- | ---: | ---: | ---: |
| Tiger II | `pzkpfw-vi-tiger-2-with-full-interior.zip` → `source/Tiger2int.zip` → `Tiger2int.obj` | 1,264 | 1,136,835 | 80,913,304 bytes |
| IS-2 | `is-2-with-full-interior.zip` → `source/IS-2INT.zip` → `IS-2INT.obj` | 1,710 | 804,467 | 67,256,276 bytes |
| Panzer IV | `pzkpfw-iv-ausf-g-with-full-interior.zip` → `source/pz4int.zip` → `pz4int.obj` | 425 | 616,690 | 65,674,012 bytes |

The nested archives each contain only one OBJ. The source header says ImageToStl.com. The files contain anonymous `obj1`… object groups, triangle faces, normals and UV coordinates. They supply no MTL, texture files, material assignments, semantic component names, parent hierarchy, hinges, original pivots, author or license record. The archive title establishes which vehicle was supplied, not the completeness or accuracy of its interactive systems. Redistribution status remains unverified from these local files.

Every triangle and source object is retained by `tools/bake_tank_editor_sources.py`. This uses direct OBJ index-triplet conversion, without mesh validation, decimation, welding, recentering individual components or an inferred rig. An initial Blender OBJ import dropped source faces, so that export was rejected by a triangle-count assertion. The shipped editor GLBs come from the direct conversion.

The single **provisional neutral matte metal** material is linear RGB `(0.18, 0.20, 0.22)`, metallic `0.25`, roughness `0.82`. It is an editor display material, not recovered source paint. Source normals are transformed and normalized; UVs use `(u, 1-v)` for the OBJ-to-glTF convention. Missing paint and textures are not fabricated.

Each vehicle directory includes `SOURCE.md` and `source_manifest.v1.json` with the archive/nested-archive/OBJ/GLB hashes, original bounds, exact normalization matrix, full object list and object bounds.

## Coordinate frames and provisional scale

All new source GLBs are baked into the same game convention: **+X tank-left, +Y up, +Z forward; ground at Y=0**. Their whole source belongs to an identity hull frame. The unused turret and gun slots stay hidden. They must not receive Tiger I turret/trunnion offsets. Only the original Tiger I and T-34/76 use their existing authored hull/turret/gun `Parts` offsets.

| Vehicle | Source-to-game formula | Game bounds min → max, metres |
| --- | --- | --- |
| Tiger II | `(-z + .04, -y - .19, -x + .39)` | `(-1.900, 0, -3.720)` → `(1.910, 3.940, 6.170)` |
| IS-2 | `(-.5x - .06706825, .5y - .096057, -.5z - .64426175)` | `(-1.718, 0, -3.381)` → `(1.735, 3.382, 6.372)` |
| Panzer IV | `(-z - .1269725, -y + .208103, -x + 1.5002635)` | `(-1.495, 0, -2.993)` → `(1.461, 2.820, 3.870)` |

CPU vertex projections identify Tiger II/Panzer IV up as source −Y and barrel direction as source −X; IS-2 uses source +Y up and −Z forward. X/Z centering uses the support vertices within the lowest 0.8 m. The IS-2 uniform 0.5 factor changes its raw 19.51 m overall extent into 9.75 m. That is a provisional physically sensible normalization, **not a source unit declaration or calibrated historical blueprint**. Heights include antenna/roof projections; lengths include barrels.

The diagnostic `captures/astra/tank_editor_source_projection_20260913.png` shows raw vertex projections for orientation checking. It is not a gameplay screenshot or a beauty render.

## Authoring persistence and installed objects

Vehicle-specific documents live at `assets/tanks/editor/<catalog-id>/scene.v1.json`. These are versioned authoring sources for props, attached lights and lighting values. The asset loader prefers an actual current-checkout model path, then falls back to the configured game asset warehouse. Saves use explicit local authoring paths, so `KESSEL_ASSET_ROOT` cannot redirect an edit into another checkout.

`src/tank_dressing.rs` supplies existing Hull/Turret/Gun frames and local translation, XYZ rotation and scale. World-to-local placement uses `parent.inverse() * world`; presentation-only gun recoil is applied by the visual parent. Static inspection tanks support only Hull attachments until a real articulation map is authored. Their lack of a hinge rig must not be hidden by applying Tiger pivots.

The real Enigma model is already available at:

`assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked/enigma_machine_runtime.glb`

Its runtime catalog ID is `prop.enigma_machine.runtime`; source/license documentation is adjacent. The real asset browser includes **Enigma machine** under **Tank equipment** in both world and tank editors; search `enigma` to select it. A new Tiger I garage seeds its existing Enigma at hull-local `(-1.20, 0.74, 1.22)` with a +90° Y rotation. Select that machine and use G/R/T to edit its placement. Gameplay recognizes this authored entry as an override of the existing installed Enigma, rather than drawing a duplicate; multiple Enigma overrides are rejected. Removing the entry hides it in isolated garage Play. Ordinary campaign gameplay still has its original fallback when no override exists.

## Inspection candidates and verification

`inspection_candidates.v1.json` in each imported vehicle directory records candidate camera positions and six axial ray-to-triangle distances, in order +X, −X, +Y, −Y, +Z, −Z. These measurements identify usable starting free space without declaring a crew station, collision capsule, or gameplay spawn. They are not a complete clearance proof or visual acceptance.

Useful initial candidates face +Z:

- Tiger II: `(0.60, 1.80, -0.40)`; nearest forward geometry 1.154 m, upward geometry 0.260 m, downward geometry 0.700 m.
- IS-2: `(0.50, 1.80, 0.50)`; nearest forward geometry 1.346 m, upward geometry 0.936 m, downward geometry 1.281 m.
- Panzer IV: `(0.40, 1.85, -0.20)`; nearest forward geometry 0.745 m, upward geometry 0.643 m, downward geometry 0.933 m. All six tested axes clear at least 0.339 m.

CPU GLB validation checks the file/chunk lengths, all accessors/indices, finite vertex/normal/UV values, exact triangle counts, source object counts and output bounds. All three exports passed. The compiled Rust test `tank_editor_scene::tests::imported_editor_sources_load_exact_geometry` also passed on 2026-09-13 using the central release test binary and canonical `KESSEL_ASSET_ROOT`: all three sources loaded through `tank_editor_scene::load_preview`, with exact index counts, normalized bounds and hull-only visibility.

The subsequent real-renderer GPU review passed for all five garages. Final receipts are `captures/astra/editor_acceptance_20260913_v2_<catalog-id>_receipt.json`: Tiger I passed 12 checks; Tiger II, IS-2, T-34/76 and Panzer IV each passed 14. This version selects Enigma from the production asset browser without a test-only fallback entry. `src/editor_smoke.rs` drives actual `EditorState` methods and frame updates with the centered editing ray, renders the editor HUD, and edits temporary scene documents. Checks cover Inside navigation, picking source geometry, placing and selecting the imported Enigma, moving its attachment, editing an attached lamp, save/reload persistence, and preservation of live authoring documents. Static-model checks also exercise walkthrough Play/Stop and preservation of the chosen camera pose.

This is **method-level GPU acceptance, not native mouse/keyboard event replay**. It does not verify window focus, native pointer capture or the title-screen click sequence, and the garage smoke harness does not launch full Tiger gameplay. Those remain separate runtime checks.

The nine IS-2, Panzer IV and T-34/76 `orbit`, `inside` and `reloaded` images in the final v2 capture series, plus Tiger I's v2 `inside` image, were visually inspected. All three exterior models are upright and coherent. IS-2 and Panzer IV show retained interior geometry from usable inspection positions; their initial cabins are dark, and the saved test lamp makes the equipment more readable after reload. Tiger I now starts at the loader with cabin geometry and a vision port visible; its initial unlit garage remains dark. T-34/76 correctly retains an exterior inspection view with an explicit no-verified-interior message. The Enigma and lamp smoke placements demonstrate persistence, not finished stowage: the Panzer IV test prop is close to surrounding hardware and needs deliberate placement before treating that layout as playable. No collision-free or ergonomic placement claim follows from these checks.

The large derived GLBs remain local and ignored. The small source manifests, authoring scenes, receipts, documentation and rebuild tool are allowlisted in Git. There is no existing `.gitattributes` or Git LFS configuration in this checkout; this work does not introduce a new LFS dependency.

Rebuild each local GLB from the retained source archives:

```sh
OPENBLAS_NUM_THREADS=1 python3 tools/bake_tank_editor_sources.py --vehicle tiger_ii
OPENBLAS_NUM_THREADS=1 python3 tools/bake_tank_editor_sources.py --vehicle is2
OPENBLAS_NUM_THREADS=1 python3 tools/bake_tank_editor_sources.py --vehicle panzer_iv
```

## Gameplay and world acceptance

`captures/astra/tiger_workshop_play_20260913_v2.json` verifies the actual isolated `Game::build_tank_editor` factory with an edited scene containing an Enigma attachment, toolbox and lamp. After 22 seconds of real simulation it still has no active enemies, soldiers, pickups, grass, campaign route or ambient battlefield explosions, and the saved attachment content is retained. The associated exterior and Loader images use the real renderer. This is a factory/runtime check; native window input and a complete manual reload cycle were not replayed by this check.

`captures/astra/editor_acceptance_20260913_v2_world_receipt.json` passes 11 checks against a temporary copy of the actual Act One winter manifest: grounded walking, crosshair placement, selection, movement, world save/reload and unchanged live files. The orbit, edited and reloaded images were visually inspected. The bright snow made the original HUD difficult to read, so the final build adds dark backing behind editor text.

The final release built successfully. The focused editor/navigation/scene/dressing/workshop suite passed 46 CPU tests, and a separate real-source import test verified every triangle in all three full-interior imports.
