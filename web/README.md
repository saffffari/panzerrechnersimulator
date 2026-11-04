# Panzer browser live range

Hermes (delegated AI) prototype, based on native commit `3e09f47fd54f21f22c6d66fed19305c7e1fe5988`.
All changes live in this worktree's **web/** directory. Native Rust, shaders, stable editor, and parent checkout are untouched.

## Run locally

```bash
cd /home/alex/panzer/web-playground/web
npm ci
npm run dev
```

Open **http://127.0.0.1:5178/**. Server is loopback only; nothing is published.
`npm run build` type-checks and creates `web/dist` (about 0.53 MB, about 0.14 MB compressed JS/CSS).

Visible controls are deliberately limited to **SHOT**, **FIRE**, **TIME STEP**.
Single left click on tank places the impact and never fires. Hold left for 180 ms, then drag to rotate the single amber handle on a radius-0.85 m sphere. Continuous horizontal/vertical dragging reaches all sides of the sphere. Incoming direction is handle → impact. Right drag orbits, middle drag pans, wheel zooms. F fires. Space pauses and Backspace restores the tank (keyboard conveniences only, no extra panels). Camera navigation is independent of aiming. No cache, rewind, recording, automatic screenshots, or localStorage preset authoring exists in the app.

## Native saved preset handoff

The shipped `public/native-preset.json` is explicitly a placeholder baseline, **not Alex's final settings**. Parent will provide the chosen native export after saving it. Read the native JSON without modifying it:

```bash
cd /home/alex/panzer/web-playground/web
node tools/import-preset.mjs /home/alex/panzer/editor-stable/checkout/target/fea_range_presets.json 'shot1'
```

Replace `shot1` with the saved name, or omit the name to import all saved shots. The tool accepts `{name,settings}`, a list, or the native `{schema:"fea-range-presets/v1",presets:[...]}` envelope. It writes only `web/public/native-preset.json`. Reload the browser afterward; rebuild if serving `dist`.

Native shape:

```json
{"name":"shot1","settings":{"shot":{"shell":12,"range_m":400,"from_az":-90,"from_el":0,"target":{"point":[-1.6,1.45,0]},"seed":41},"scene":{"racks_loaded":true,"cabin_lamps":true},"tissue":{"active_reach_m":0.55,"spread":1},"camera":{"eye":[-9,6,10]},"look":{},"lamp_gain":1}}
```

Native shell indices 0–15 match `src/game_fea_range.rs::shells()` at the base commit. Shell, range, azimuth/elevation (degrees), point (hull-local metres), seed, tissue reach/spread, camera eye and sky exposure are mapped. Unknown fields are retained in fixed settings but are **not equivalent native implementations**. Current coarse geometry does not reproduce native tissue material constants, racks, lamps, grading, camera yaw/pitch, named crew targets, or native scene transforms. These require explicit future mappings if the final preset depends on them. No fine tuning is exposed in the web UI.

## Implementation and provenance

- `src/catalogue.json`: extracted from this checkout's `src/ballistics.rs`, native base commit above. `python tools/catalogue.py` refreshes it. Mass, velocity, drag, calibre, filler, reference penetration and reference range are copied for all 16 native rounds. Velocity decay, DeMarre exponent 1.43, normalization and slope exponents follow native formulas.
- All rendered assets are original procedural boxes, cylinders, planes and spheres created in `src/main.ts` and `src/simulation.ts`. No third-party models, textures, fonts, remote assets, paid generation, source asset symlinks, or large asset packages are shipped. The native catalogue is the only extracted game data.
- Engine dependency: Three.js (MIT); package versions and integrity are locked in `package-lock.json`. [InstancedMesh documentation](https://threejs.org/docs/#api/en/objects/InstancedMesh).
- Gustav and sonic T-34 are **experimental, illustrative coefficients**, not historically validated catalogue entries. Gustav: 800 mm / 7,100 kg / 720 m/s; T-34 payload: 26,500 kg / 343 m/s. Their 900/1600 mm penetration anchors are synthetic knobs fixed in code, and damage radii are deliberately capped. T-34 is a coarse energy payload, not a whole vehicle flying through a structural solver.

## Model limits and budgets

This is a simplified live JS model, **not the native exact solver**. About 2,015 metre-scaled coarse hull/crew cells; side/front/roof proxies are 80/100/25 mm. A shot immediately evaluates a line corridor, localized cell loss and nearby displacement; there is no high-speed continuous projectile solver, fracture mechanics, residual-velocity layered solve, armour ricochet trajectory, APHE fuze simulation, or native fidelity guarantee. Round HE/filler chemistry is not simulated separately; values are preserved as catalogue metadata. Tracks, wheels and barrel are static merged geometry and remain after damage.

Crew consists of breakable voxel assemblies. Blood cubes have cheap ballistic flight, a single cabin-wall/ground crossing, then become persistent square splats; the airborne particle is removed on contact. Wall contacts use fixed cabin planes, even where armour has been removed. Debris has a cheap damped ground bounce, not rigid-body physics. Smoke is translucent instanced cubes. No rigid-body dependency is needed for these intentionally coarse visual particles.

Caps: 1,200 active particles total, 100 rendered smoke cubes, 400 square splats, 8 fixed integration steps per render frame, 100 ms wall-time clamp, 1.5 device-pixel-ratio cap, 1024² shadow map. Missed integration work is discarded and counted in diagnostics. One fire action scans at most the fixed cell count. Timing diagnostics hold at most 600 scalar intervals; there is no simulation or rendered-frame cache.

## Validation

```bash
npm test
npm run build
node tools/playtest.mjs
# Optional hardware-backed pass, with native editor CLOSED:
HEADED=1 XAUTHORITY=/run/user/1000/xauth_IAxOII node tools/playtest.mjs
```

The Xauthority path is session-specific; use the current desktop's authorized path. QA refuses to start when a native range editor process is detected and closes **its own browser only** if one appears during testing. It never kills user/editor processes. Do not run GPU QA after the parent signals native launch. The production app has no process monitor; close its tab before opening the native editor.

4/4 simulation tests cover catalogue/preset compatibility, AP cell loss/deformation and crew breakup, grazing stops/reset, and blood first-contact/pool caps. Browser QA checks boot, click without firing, hold-drag aiming, FIRE/F, time-step selection, accumulated damage, extreme shots, reset and 390×844 layout; screenshots are mandatory and were inspected. Final evidence is `qa-results-headed.json` and `qa-results-headless.json`.

2026-11-03 hardware pass: Chromium / ANGLE OpenGL / NVIDIA RTX 4090, 1366×768, baseline 2,015 cells and 8 draws; first AP hit removed 99 cells; stress remained ≤1,200 particles and ≤400 splats; zero page exceptions. Timing is measured `requestAnimationFrame` interval, **not GPU execution time**. Baseline averaged 16.98 ms (p95 16.7 ms); cumulative stress-window average 16.73 ms (p95 16.8 ms), no integration budget drops. These short HAL desktop samples do not establish laptop performance. Headless SwiftShader is functional QA only.

Screenshots are in the required flat owner folder `/windows/shop/panzerrechnersystem/captures/screenshots/`, named `web-playground-headed-{baseline,ap-impact,gustav,mobile}.png` and headless equivalents. They are not committed or published.
