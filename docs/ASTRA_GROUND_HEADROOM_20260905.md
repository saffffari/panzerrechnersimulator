# Ground detail and standing clearance, 5 September 2026

This is source work plus bounded CPU geometry evidence. No new GPU capture or frame-time result is claimed.

Use the immutable review world `assets/worldgen/revisions/playtest-ground-detail/20260905T151851623908Z_be1c0a5f/manifest.v1.json`. The previous functional-house revision and every source model remain intact.

## Ground and grass

The former 1024-square Playtest drape covered 768 metres: 0.75 metre colour texels. The terrain shader additionally invented 25 cm colour and normal blocks. Grass used a legacy campaign mask at an unrelated map extent, then thinned that already sparse field with large periodic gaps.

The new drape contains smooth road/pasture/woodland material blends from the preserved Pizza source tiles. A separate, correctly registered class mask excludes roads, building slabs and the compiled house footprint. The runtime reads that mask from the selected manifest and uses its actual terrain extent.

Four 256-square source Pizza texture layers repeat every 2.56 metres, giving 1 cm source texels before projection. They provide bounded linear-reflectance detail, while the world drape remains the colour authority. CPU area-averaged mips retain a neutral mean. Raster uses the major derivative footprint and linear mip/texel filtering on terrain only. Terrain ray hits use a conservative derivative-free ray-cone footprint and the actual triangle UV gradients; both macro drape and fine detail are filtered. Grazing footprints converge to the source material average. This is not a full propagated ray-differential implementation and has not yet been visually evaluated for horizon stability. The artificial random colour and normal blocks are gone.

Grass remains actual closed, thin geometry with shared physical wind and persistent tread compression. The original corrected mask still yielded only about 58 foreground survivors per square metre. The next source revision keeps the 32,768-stalk cap and spends it near the feet: 512 candidate roots/m² inside 2.5 m, tapering to 64 at 5 m, four at 10 m, and zero at 26 m. Pasture acceptance is 0.82–0.98, woodland 0.24, and roads/slabs zero. Far cells iterate only their local budget. Stable world keys preserve surviving roots; zero mask probability now rejects even a zero hash. Blades are closed 16-triangle geometry, 6.8–8.8 mm across before bending and roughly 13–32 cm tall, with roots 5 mm into the actual ground. This remains primary-raster-only grass, without secondary-ray visibility or self shadow. Density appearance, popping and runtime cost still need the next capture.

## Boulder support

The cook preserves all 619 placement IDs and transforms except the Y translation of 85 rock instances. Actual source triangles are sampled uniformly over projected rock area, and the lower envelope is seated until approximately 45% intersects the actual local terrain surface. Drops range from 0.006 to 0.432 metres. This removes single-corner contact by embedding the lower rock, without changing source orientation, shape or terrain. This is a visual seating rule, not solved rigid-body equilibrium.

## Standing scale and architecture

The player eye was already 1.70 metres. Full standing stature is now explicitly 1.8288 metres (six feet), leaving 0.1288 metre above the eyes. The functional-building collision height previously used 1.68 metres and now uses the standing stature.

Exact source-triangle intersections show that `floor_ceiling_hr_1` is 0.400001 metre thick. The old `doorway_hr_1` lintel is 2.10 metres above its module base, leaving only **1.699999 metres above the finished floor**. Nineteen diagnostic-world door modules now use the matching open `doorway_hr_2_wide` frame: its 2.50 metre lintel gives **2.099999 metres of finished clearance**. This changes the actual mesh, without shrinking the player or scaling the whole building.

The functional house already used the taller frame; its 3 metre storeys have 2.60 metres between slabs. Sampled garage, warehouse and shed roof clearances are 3.43, 3.92 and 3.30 metres respectively. Those sampled ceiling positions were not low.

The cellar draft has no intact ceiling. Its earlier first-person film explicitly held crouch for six of nine seconds. The new CPU gate checks 340 approach/descent segments using conservative 56 cm-wide upper-body boxes and full six-foot head height. Another 88 segments each pass through the garage and warehouse openings. These 516 segments use actual finished-floor support where present and hit no source triangles in the standing body volume. The bottom 35 cm is excluded as the foot/step zone; this is not a full runtime capsule or moving-leaf proof.

Source-hinged, physically ray-visible door leaves are now implemented for manifest and featured-house frames. Their source geometry, collision, reversible opening and LOS tests passed in the coordinated v13/v14 gates; first-person appearance/interaction proof remains pending. See `ASTRA_WORLD_DOORS_20260905.md`.

CPU receipts:
- `artifacts/astra-audit-20260905/playtest-ground-detail-openings-cook.json`
- `artifacts/astra-audit-20260905/playtest-metric-clearance.json`

Reproduce with `tools/worldgen/cook_playtest_ground_detail.py` and `tools/worldgen/check_playtest_metric_clearance.py`. Root must run the terrain-source WGSL validation, texture neutral-mean test, standing building collision tests, and a current first-person GPU review before calling this delivered.

## Next source revision: support, density and ordinary gameplay recipes

The source floor/ceiling meshes were visible in ordinary manifest buildings but only the featured building re-grounded the soldier onto its floors. This put ordinary feet roughly 10 cm below the visible garage/warehouse slab tops. `src/walk_surfaces.rs` now indexes source floor and roof triangles in two-metre XZ bins. Authored transformed normals distinguish upward floor tops from downward slab undersides. A floor can be reached only within a 30 cm standing step or a real downward crossing; distant upper floors cannot attract the player. Exact triangle coverage retains holes. Clipped source polygons over the body footprint stop low-headroom steps and upward head motion. Leaving a raised slab preserves the departure height and starts ordinary gravity. The camera stays at the existing 1.70 m eye height; the body remains 1.8288 m.

This is ordinary static manifest support. Featured-building destruction/stair support remains in its existing damage-aware system. Vertical structural faces still use the shared world cover path; the new layer does not add collision to arbitrary props or promise universal triangle-level collision.

The reported 168/191 stalk logs came from the act-0 legacy Stalingrad heightfield with no selected Playtest class mask. That is the deliberately sparse dry/winter fallback. New telemetry exposes class-mask load/request status, the fallback flag, roots inside 2.5/5 metres, and the actual 32,768 cap. The near counts are measured around the quantized two-metre population focus, not the exact eye position. The new recipes select the same immutable corrected world as the current main-menu Playtest path.

Source-ground texel projection: at 640×360, 68° vertical FOV and a 1.70 m eye, a one-centimetre cross-ground source texel projects to 0.785 native pixels at a 30° downward glance, 1.110 at 45°, and 1.516 at 75°. Along-view foreshortening differs; distant/grazing detail must remain footprint-filtered. No ground shader or gain changed in this follow-up. This is a size calculation, not visual sampling proof.

Seven meaningful new unit tests are authored: three grass population/mask tests and four floor-support/source-clearance tests. They cover dense occupied foreground, deterministic placement and cap, exact bare-mask exclusion, disclosed sparse winter fallback, the actual selected class mask, real Pizza top/underside normals, ceiling stops, bounded steps, holes, slab departure and downward terrain travel. Rust syntax parsing passed. Execution of these new tests is pending the next coordinated root gate; the earlier v14/v15 binary gate does not include this later world source work.

New source receipts:

- `artifacts/astra-audit-20260905/standing-world-routes-v15-source.json`: all 516 conservative upper-body segments clear, including actual finished-floor height.
- `artifacts/astra-audit-20260905/ground-texel-projection-v15-source.json`: current shader repeat and geometric projection.
- The earlier `standing-world-routes-v14-source.json` is retained as the preceding terrain-height-only upper-body receipt.

The new first-person JSON recipes below are source recipes named v15, **not completed v15 videos**. Each has actual runtime outcome assertions, live game audio, a standing camera, and only ordinary input/action after disclosed initial placement. Main render output remains 640×360 enlarged by integer three to 1920×1080.

| Recipe under `assets/review/first_person/` | Required visible action and causal assertions |
| --- | --- |
| `fp_garage_source_doors_v15.json` | Open with ordinary aim, enter onto the finished slab, reverse out, turn to the open leaf, close, then meet the closed leaf. Aperture LOS must change with the same leaf and motion must be clipped. |
| `fp_warehouse_source_doors_v15.json` | Same open/traverse/close/block sequence at the actual warehouse frame. |
| `fp_featured_source_door_stairs_v15.json` | Close the initially open authored entrance, turn to its center, reopen and walk up the live house stairs without forced height or crouch. |
| `fp_sourced_cellar_standing_v15.json` | Descend the retained sourced cellar approach while standing, with eye-minus-feet held at 1.70 m. No intact ceiling is claimed. |
| `fp_pasture_ground_grass_v15.json` | Look down, walk eleven metres through correctly masked pasture, then raise the view. Require at least 8,000 near roots and verify visible blade grounding. |
| `fp_grove_supported_rock_grass_v15.json` | Approach actual retained stone placement `p_1ebbc3e1520ca4f46509_0`, then inspect its lower envelope, roots and soil. Woodland density is intentionally below pasture. |

Door actions may not be rejected. Timed mouse turns are derived from the actual source hinge position; there is no runtime autoaim. Recipe preflight has been added to the existing CPU validator test. Plain JSON/paths/timing inspection passed; game execution, final framing, audio and art approval remain pending. If a route or action fails, correct the route or source bug and retain the failed receipt rather than weakening its outcome threshold.
