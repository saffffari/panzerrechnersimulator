# Panzer web editor

The browser editor uses the game's authored Tiger hull, turret, gun, cabin, crew, helmets and cupola lid. It keeps the native editor's bottom-bar organization while using a smaller raster viewport and simplified simulation.

## Run and build

```sh
npm ci
npm run dev
# http://127.0.0.1:5178/
npm test
npm run build
```

The dev server is loopback-only. The generated `dist/` contains the model package and a native preset snapshot. No external deployment is performed.

## Controls

- Left click the Tiger to place an impact; click its red plus to reveal the red azimuth and white elevation rings. Drag anywhere inside either disc. A disc fills faintly during dragging; clicking elsewhere hides the rings.
- Right drag orbits, middle drag pans, wheel zooms. F fires, Space pauses/resumes, Backspace resets to an intact paused tank.
- SECTION removes roof/deck panels or enables a clipping plane. Choose its normal; drag the square center along that normal. For a normal pointing directly at the camera, vertical dragging controls depth.
- LOOK controls exposure, pixel resolution and time speed. Slow motion reduces the physical step as well as elapsed game time, creating intermediate states at the normal update cadence.
- The native-style bar includes saved shots, ammunition, integration step, cameras, preset import/export and WebM recording. Detailed controls are in the nine inspector panels.
- BATTLE retains the bounded canyon skirmish: WASD drives, mouse selects aim, F fires. The player uses the authored Tiger; enemies and terrain remain coarse proxies.

## Cutaway HUD and PNG study

The plan and crew thumbnails use the same authored model assets. Tank captures are top-down orthographic, front up, with roof/deck panels hidden, cabin lamps enabled and a transparent background. Intact parts retain their materials; damaged parts are orange and destroyed parts red. The toolbar keeps the brand and tank/crew together on the left, with controls grouped against the right edge.

LOOK → RESOLUTION STUDY renders 16 separate PNGs: widths 540, 405, 270, 180, 135, 96, 72, 54, 36, 24, 16, 12, 8, 4, 2 and 1. Portrait height follows 16:9, from 540 × 960 to 1 × 2. The HUD currently samples 135 × 240 with nearest-neighbor display. The gallery allows comparing and downloading alternatives.

`tools/export-native-hud.mjs` creates the embedded native-editor HUD atlas from the same scene. Each module's orange/red layer is masked against the intact render, and the five crew retain separate status variants. Native code uses live crew wounds/death and module faults/functionality to select these images; the browser's damage approximation is not used to decide native status.

## Assets and budget

`tools/prepare-native-assets.mjs` reads only the documented game asset root `/windows/shop/panzerrechnersystem/assets` and native material declarations in `/home/alex/panzer/voxel-runtime/src`. It preserves source files, consolidates meshes by semantic group, simplifies geometry and writes meshopt-compressed GLBs to `public/native/`. The manifest records source SHA-256, counts and the source checkout baseline.

The original Tiger assemblies contain approximately 3.65 million triangles; the browser scene uses approximately 1.11 million triangles including crew and accessories, with 78 draw calls in the default range view. The package is about 14 MB. The default viewport width is 640 game pixels, selectable from 320 to 960, independent of device pixel ratio. Edges and overlays are rendered without MSAA and scaled with nearest-neighbor sampling. Shadows update on scene changes; a paused unchanged range does not render continuously. Particle pools are bounded at 1,200 and catch-up at eight steps per display frame.

Crew source: AdrielCZ German Panzertruppe, CC BY 4.0, from the existing project package. Helmet source: the existing project's CC0 helmet. The exported assets retain the source meshes/textures and provenance manifest; this work does not grant new redistribution rights for the game's other assets.

## Native presets

GET/HEAD `/native-preset.json` reads the stable editor's `target/playground_preset.json` on each request. If that export is absent, it reads `target/fea_range_state.json`; the committed baseline is the final fallback. The endpoint is read-only and no-store. The production build packages a snapshot. Refresh the web tab after native shot changes.

Saved shot positions, directions, rounds and selection populate the web editor. Native world cameras, physical sky, ray-tracing settings and simulation state are not automatically translated into equivalent browser results. Browser presets can be saved/exported separately.

## Fidelity boundary

This is a lightweight visual port, not the Rust engine compiled to WebAssembly. Three.js PBR raster lighting approximates the native materials and cabin lights; the native raymarch transport, volumetric effects, full canyon streaming, layered FEA penetration, deformable anatomy and coupled vehicle physics are not ported.

Shots raycast against the real mesh, use the existing ballistics catalogue and slope formula, remove local hit triangles and set bounded module/crew damage states. Module grouping in the reduced mesh is approximate; crew hit detection uses bounds. The old voxel prototype remains in simulation tests but is not the displayed Tiger. Native physics remains the authority.

## Validation (2026-11-04)

`npm test`: 27 passing CPU checks. `npm run build`: TypeScript and production bundle pass. `node tools/editor-acceptance.mjs`: real model, saved shots, ring reveal/full-disc drag, section translation, firing/reset, slow-motion cadence, 16 PNG exports, Battle entry and mobile width checks pass with no page exceptions. At 0.1×, the browser probe observed 29 advancing intervals in 30 display samples and 0.0483 seconds of game time. These are functional results on HAL, not a low-end-device performance guarantee.

The original report remains in the source web worktree. A fresh integration check on **2026-11-07** passed all 27 CPU tests, the production build, and all 15 browser acceptance checks with no page exceptions. Its receipt is `target/integration-20261107/web-qa.json` in the native checkout. This proves the listed interactions, not native physics parity or a frame-rate target.

Captures are in the owner's flat folder `/windows/shop/panzerrechnersystem/captures/screenshots/`, prefixed `web-editor-`. `tools/editor-acceptance.mjs` supports `PANZER_WEB_URL`, `PANZER_CAPTURE_STAMP`, `PANZER_CAPTURE_DIR`, `PANZER_CHROMIUM` and `PANZER_QA_REPORT` so reruns do not overwrite prior evidence. QA browsers close on completion.
