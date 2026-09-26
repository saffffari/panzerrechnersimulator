# Winter courtyard, live authoring and combat pass — 25 September 2026

The ruined Pizza house, furnished interior, courtyard and gatehouse now sit in a private working copy of Act 1. White snow, overcast light, sparse bare trees, warm practical lamps and scattered battle remains establish the winter direction before the Gaea terrain arrives.

## Open and edit

Run `tools/launch_winter_courtyard_editor.sh` from this worktree. The ordinary launcher prevents a second visible game window. Close the existing window before relaunching. Add `--build` when source has changed.

- Working document: `assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest.v1.json` and its `manifest_edits.json`.
- Compound anchor: **1880, 5.4, 165**. It is north of the existing spawn at 1850, 250.
- **F3** changes internal pixel resolution; **F4** changes ordered dithering; **F5** saves. The Lighting panel controls the environment and practical sources. **F** enters Walk; **J** toggles authoring guides. Ordinary selection and G/R/T transforms remain available.
- Saved treatment: **960×540**, 31 palette levels, 35% ordered dithering, 85% retro treatment, 65% final LDR dithering. These are document settings, also loaded by Play.
- Saved lighting: exposure 1.25, fog density 0.0018, overcast 0.96, storm 0.66, sun elevation 22°. Six practical sources provide local warmth.

The seasonal terrain shader now runs in the world editor. Snow is neutral white with gentle broad variation; source terrain imagery no longer introduces red and brown checker patches into winter ground. The style remains deliberately pixelated.

## Reusable construction

`assets/worldgen/prefabs/ruined-house-courtyard-v1.json` contains the original **351-piece** compound with four lights. `assets/worldgen/prefabs/winter-ruined-courtyard-v1.json` contains the **792-piece** dressed winter site with six lights. Both appear in the asset browser's prefab library and retain individual editable pieces.

The house includes a command desk and map, domestic kitchen, aid room, stairs, an exposed upper bedroom and damaged roof. The site adds two disabled armor models, three fallen soldiers, rubble, shell scars, snow drifts, broken fences, eighteen bare trees and five additional stumps. This is an authored atmosphere study, not a reconstruction of a specific battle.

The source winter revision and its existing placements/edits were preserved. Only 182 height samples changed in the private copy to support a flat foundation and a smooth ten-metre apron. `winter-courtyard.receipt.json` records the source and height hashes. On a future Gaea map, place the rigid building on a prepared foundation; re-ground the outer foliage, wrecks and snow to that terrain. Terrain generation/import is not part of this pass.

## Density and resource use

The current editor contains **13,864 placements**, **120 shared prototypes/instance buffers**, 673 spatial pages and 389 spatial cells. Rendering already uses shared geometry, paged instances and frustum queries. Small details remain independently selectable.

Derived wreck geometry drops from 453,848 to 37,935 triangles combined (**91.64% reduction**). Original GLBs remain intact. Dark material variants, snow drifts, scorch patches and wall chips live under the shared asset root's `assets/authoring/battlefield/`, with provenance receipts. The open-window derivative likewise preserves the source Pizza GLB.

The optional live-authoring mailbox now writes its large scene snapshot only when the scene changes or a command requests it. A small heartbeat still reports current state. Sixteen unchanged-scene samples measured a median **0.319 ms** and p95 **0.343 ms** per 400 ms authoring poll. This is poll CPU time, **not frame time or FPS**. Geometry and counter evidence is in `artifacts/authoring/winter-geometry-and-authoring-20260925.json`; no comparative hardware FPS claim is made.

## Combat behavior

Short projectile sweeps use a conservative 32-metre cover grid instead of scanning the full map at each fixed tick. A 10,001-box test checks every true contact against the unfiltered reference and admits fewer than 100 candidates for each tested short segment. Large bounds/queries retain a full fallback, and destroyed structures are checked against their live state. This is a work-reduction/correctness check, not an FPS benchmark.

Runtime P08, player coax/hull MG, infantry rifles and enemy MGs now launch swept projectiles on the host's 240 Hz clock. Gravity, forward velocity decay and world crosswind affect the real flight and contact, and visible trails sample that state. Damage and sparks occur on arrival. A target can move out of the path before the bullet arrives. Ordinary small arms stop on tank armor without HP chip, penetration, spall or cabin fire. Both hull and turret envelopes block bullets, including roofs.

Small-arms speeds retain the previous game's 350/760 m/s values; their drag constants remain provisional gameplay tuning. The point-mass wind response models crosswind, not a full aerodynamic solution. Offline infantry policy training retains its hitscan approximation. Live cannon impacts retain the existing calibrated penetration/obliquity, material quality, normalization, overmatch, localized armor damage and interior fragment machinery. Cannon collision uses articulated hull/turret envelopes with front/side/rear plate data; it is not per-triangle armor and roof/belly cannon penetration remains unsupported.

Exterior explosions now have a short flash, up to six seconds of rising, wind-driven soot, and faceted smoke grains. Glancing armor sparks use the contact normal. Eight shared particle pages and fixed burst/grain budgets bound presentation; projectile scratch storage is reused between fixed ticks. Interior hot fragments retain their 768-body authority pool. The penetration review harness now checks the actual pool capacity instead of its obsolete 256-body limit.

## Validation and review

The release build passes. Focused CPU checks cover ballistics, armor damage, live flight, particle budgets, infantry, mixed combat, editor, prefabs, display profiles and shader compilation. The GPU small-arms test verifies delayed damage, a 1 cm cover slab, a moving target, range retirement and inert roof contact against the real Tiger scene. The GPU VFX review invokes actual discharge hooks; explosion and armor-spark examples are explicitly staged presentation diagnostics. The interior review passes all four seats, its four-beat fragment sequence and pool retirement at 6.575 seconds.

An additional GPU test loads the actual working winter document through Play, advances three game updates and asserts that resolution, display treatment, exposure, fog, overcast and storm still match the saved profile. Its inspected game render is `captures/astra/winter_courtyard_gameplay_20260925_v1.png`.

New captures stay flat under `captures/astra/`. Useful files include `winter_courtyard_20260925_07_seasonal_snow.png`, `winter_courtyard_20260925_05_command_interior.png`, the `weapon_vfx_20260925_v2_*` series and `winter_penfx_20260925_v2_penetration_storyboard.png`. Stills validate appearance; they do not establish frame-rate performance, historical armor accuracy or an audio audition.
