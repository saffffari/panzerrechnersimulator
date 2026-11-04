# Panzer browser playground

Hermes (delegated AI) prototype based on native commit `3e09f47fd54f21f22c6d66fed19305c7e1fe5988`. All changes are in **web/**. Native Rust, shaders, assets, stable editor and parent checkout remain untouched. No external publication or paid asset generation.

## Run

```bash
cd /home/alex/panzer/web-playground/web
npm ci
npm run dev
```

**http://127.0.0.1:5178/**, loopback only. `npm run build` type-checks and creates `dist/` with a preset snapshot. Current package: about 548 KiB on disk, JavaScript about 546.5 kB / 142.3 kB gzip; CSS 1.87 kB. The complete native terrain and prop packages are never loaded.

## Exactly two modes

**Sandbox**: mode shell plus **SHOT / FIRE / TIME STEP**, fixed native tuning. Left click places the impact and never fires. Hold left 100 ms and drag the single amber handle around a 0.85 m wire sphere; incoming ray is handle → impact, with continuous rotation through every axis. Right drag orbits, middle drag pans, wheel zooms. F fires; Space pauses; Backspace restores tank. No fine tuning panels, camera selector, preset authoring, localStorage saves, recording, replay, simulation cache or frame cache.

**Play**: mode shell plus **FIRE / T-34 count (1–10)**. W/S or up/down drive; A/D or left/right steer; left click enemy or road to aim; F fires; right drag orbits; wheel zooms; Space pauses; Backspace restarts. Changing enemy count starts a fresh skirmish. Weapon is the current authored Sandbox round; no added tuning controls. Simple hull health, return fire, disable states and restart make a playable single skirmish. Play's health proxies do not run the Sandbox voxel solver. No additional mission, scoring system or unstated continuation of Alex's truncated request was invented.

## Native preset: refresh without manual import

`vite.config.ts` handles only **GET/HEAD /native-preset.json**, reading this exact local file anew per request:

`/home/alex/panzer/editor-stable/checkout/target/playground_preset.json`

Native Save Preset writes it automatically. Refresh the browser afterward. Responses use `Cache-Control: no-store`; `X-Panzer-Preset-Source` says `native` or `baseline`. An absent/invalid export falls back to `public/native-preset.json`, explicitly labelled **Baseline — awaiting native export**. Nothing writes to the native source. `npm run build` reads the same source and emits **dist/native-preset.json** as a fixed snapshot. The final user export was still pending during validation; current build packaged the baseline.

The optional CLI remains for manual baseline snapshots and legacy files:

```bash
node tools/import-preset.mjs
node tools/import-preset.mjs /absolute/path/fea_range_presets.json 'saved preset name'
```

It accepts a single `{name,settings}`, a list, or `{schema:"fea-range-presets/v1",presets:[...]}`, and writes only `web/public/native-preset.json`.

**Authored lists**: `settings.shots:[{name,shot:{shell,range_m,from_az,from_el,seed,target}}]` is authoritative when present. Those names are the **sole SHOT options**, with `settings.selected_shot` initially selected. `settings.shot` is the legacy fallback. Names, positions, angles, seeds and indices remain intact. Empty authored lists disable firing. Saved point markers retain the exact point; coarse hull entry is computed separately at fire time. `{man:role}` is preserved but that shot is explicitly disabled: native crew-role resolution is unsupported. Web click placement is transient retargeting, not authored-list editing.

| Native settings | Browser mapping |
| --- | --- |
| shots, selected_shot, shot shell/range/angles/point/seed | Authored selection and Sandbox ray; native degrees and hull-local metres |
| tissue.active_reach_m, tissue.spread | Bounded coarse crew/particle approximations |
| camera.eye | Bounded local Sandbox view; not full world-space camera equivalence |
| sky.exposure | Bounded tone-mapping exposure |
| scene racks/lamps/timing/fresh behavior | Unsupported |
| tissue tear energy, critical stretch, forward/outward, landing breakup | Unsupported |
| section; look; effects; lamp_gain; sky weather/time/solar controls | Unsupported |
| camera yaw/pitch/focal length/speed; tiger world transform | Unsupported |
| time_speed, looping, engine, accumulation, wear, snow, native battle/route/open state | Unsupported |
| target.man | Retained and clearly disabled, not silently redirected |

Other fields survive import unchanged but are ignored; no native lighting equivalence is claimed. Native shell indices 0–15 match the base `src/game_fea_range.rs::shells()`. Index **16** is experimental Gustav800 AP, **7,100 kg / 720 m/s**; **17** is experimental T-34 Mach1, **28,500 kg / 343 m/s**. Their 900/1600 mm penetration anchors are synthetic fixed approximations; this is not a historically validated whole-vehicle impact solver.

## Canyon subset and asset provenance

`tools/extract-canyon.py` reads only the authorized game source:

`/windows/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/manifest_edits.json`

It extracts 149 road knots to **src/canyon-road.json (4,686 bytes)**, including source SHA-256. Native start is **x −791.016, z 745.783, heading 180°**. Browser origin is that start. Road distance is measured along the authored polyline, not Euclidean distance. Player starts at **s=0**; lead T-34 starts at **s=1,000 m**, and remaining vehicles at 9 m road intervals up to **s=1,081 m**. Data is clipped to **s ∈ [−40, 1,120] m** for rear/formation/turning margins. Floor is a 56 m strip; coarse roadside cliffs add a bounded visual wall margin. No full canyon, world streaming, original heightfield, native prop archive, garrisons or assets outside this corridor are imported or rendered. No native assets were removed or altered.

Terrain, cliffs, rocks, road markings and tank silhouettes are original procedural geometry in `play-view.ts`; they are **coarse flat-floor proxies**, not an exact terrain transplant. Three fixed rock obstacles exist only inside the corridor. Sandbox geometry is likewise original procedural boxes/cylinders/planes/spheres. No third-party models, textures, fonts, remote asset downloads, large binary assets or source symlinks are shipped.

`src/catalogue.json` copies mass, velocity, drag, calibre, filler, reference penetration and range for 16 rounds from this checkout's `src/ballistics.rs`. `python tools/catalogue.py` refreshes it. Exponential velocity decay, DeMarre exponent 1.43, normalization and slope exponents follow native formulas. Three.js is MIT; dependency integrity is locked in `package-lock.json`. [Instancing documentation](https://threejs.org/docs/#api/en/objects/InstancedMesh).

## AI knowledge boundary and work budget

`battle.ts` has no renderer, Three.js or DOM dependency. `Squad.step(dt,time)` receives **no player state**. The squad copies the player's initial position once at initialization. Every enemy steers toward a shared, timestamped **remembered** point. Road following avoids cross-terrain chords; pressure/left/right flank lanes spread the group; pair separation and three rock checks bound local avoidance. Near the remembered point they search a small sector around it rather than following an unseen player.

Only `Battle` sensing can compare true positions. Visual observations require **<180 m**, a **120° front cone**, rock occlusion clearance and a 16-sample corridor line-of-sight test. Gunshots can be heard within **350 m** (coarse distance-only acoustic model). Movement is deliberately silent. Actual contact within 7 m also yields an observation. Sight/hearing/contact reports carry observer ID, point and time and are shared squad-wide. Fire aims at observed memory; true position is used only for physical hit/contact resolution. No global live-player steering update exists. No magical reacquisition occurs when the player silently moves unseen.

Maximum ten tanks, 5 Hz sensing/squad decisions, pair checks bounded to 100, fixed 60 Hz Play steps under the common 8-step/frame cap, 32 combat events and 32 tracer lines. No reinforcements or unbounded history. Coarse health/60 mm enemy armour and 12-point enemy hit damage are gameplay proxies, not native ballistics/damage fidelity. Local steering is not A*, a full formation planner or native doctrine. Light tanks have cheap instance silhouettes; disabled hulls darken. Sandbox's blood/deformation solver is not multiplied across enemies.

## Sandbox limits

About 2,015 coarse hull/crew cells; side/front/roof proxies 80/100/25 mm. Immediate shot-ray corridor, localized cell removal, nearby displacement and independent crew voxel breakup. Blood cubes have ballistic motion, one cabin-wall/ground contact, then become square splats and leave the airborne pool. Wall planes remain even where armour is removed. Debris has a damped ground bounce. Smoke is translucent instanced cubes. Tracks, wheels and barrel are static merged geometry.

No exact solver, fracture mechanics, continuous high-speed projectile flight, layered residual velocity, fuze simulation, separate HE/filler chemistry, true rigid bodies or native fidelity guarantee. Caps: **1,200 particles, 100 rendered smoke cubes, 400 splats, 8 integration steps/frame, 100 ms wall-time clamp, 1.5 pixel ratio, 1024² Sandbox shadow map**. Discarded integration work is counted. Diagnostics retain only 600 scalar timing intervals, not frames or simulation checkpoints.

## Validation and GPU resource state

```bash
npm test
npm run build
# Browser QA ONLY after parent explicitly confirms native is closed again:
HEADED=1 XAUTHORITY=/path/to/current/authorized/xauth node tools/play-mode-test.mjs
```

**GPU QA is paused by parent request. No agent browser, animation or stress session remains running. Only the loopback Vite dev server is retained.** Do not run browser QA during native validation/editor use. The bounded QA harness checks native range processes and closes its own browser on detection; it never kills user/editor processes. Production tabs have no OS process monitor: close them before native GPU work. Xauthority paths are desktop-session-specific.

**15/15 CPU tests pass**, including native export reread/fallback, authored-only dropdown data, rounds 16/17, AP deformation and crew breakup, blood first-contact/pool caps, exact along-road spawning, bounded count/corridor/obstacle behavior, real combat/reset, sensor occlusion and stale-observation rejection. Two non-omniscience tests compare entire squad states under different unseen silent player locations, both before and after a real sighting: memory and steering stay unchanged. Full 10-enemy Battle simulation (sensors/combat included), **120 simulated seconds / 7,200 steps**, took **83.3 ms CPU** on HAL in the recorded run; 554 squad decisions and peak 9/32 events. Squad-only 200 seconds took 22.8 ms CPU. These accelerated CPU tests do not measure real-time frame cost or laptop speed.

Before the parent GPU pause, `tools/play-mode-test.mjs` passed on Chromium / ANGLE OpenGL / RTX 4090 at 1366×768: exactly two authored shot names, selected index 17 and exact saved point, index 16 direction, ten enemies at the road spawn, 15.07 m player movement with memory still initial `(0,0)`, explicit fire, restart, Sandbox restoration, 390×844 layout; **zero page exceptions**. Play rendered **15 draw calls / 6,116 triangles**; cumulative sampled frame interval averaged **17.34 ms**, p95 **16.8 ms**, including startup/mode transition. This is desktop evidence, not a laptop 60 FPS guarantee. Evidence: **qa-play-mode.json**, **qa-cpu-summary.json**.

Earlier Sandbox evidence remains `qa-results-headed.json` and `qa-results-headless.json`: baseline 8 draws / 2,015 cells; first AP shot removed 99 cells; extreme-shot pools remained capped; hardware stress-window mean 16.73 ms and p95 16.8 ms; no integration budget drops. Headless SwiftShader was functional QA only. These earlier captures predate Play.

Screenshots were inspected and are uncommitted in the flat owner folder `/windows/shop/panzerrechnersystem/captures/screenshots/`. Play capture: `web-playground-play-10-2026-11-04T02-36-06.781Z.png`; authored and mobile captures share that timestamp. Existing `web-playground-headed-*` and headless captures remain. No additional GPU tests were run after the parent's pause.
