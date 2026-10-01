# Fine exterior sparks, 2026-10-01

The user's blizzard firefight review requested more sparks with smaller individual particles.
This changes presentation only; shell flight, terminal time, armor contact position, damage,
interior authoritative fragments, exterior spark velocities and gravity are unchanged.

| Emitter | Previous count | New count | Previous initial width | New initial width |
| --- | ---: | ---: | ---: | ---: |
| Cannon armor ricochet | 192 | 384 | 25–60 mm | 8–18 mm |
| Small-arm armor ricochet | 40 | 72 | 8–20 mm | 3–8 mm |
| Blast ejecta | 192 | 384 | 20–60 mm | 6–18 mm |
| Impact ejecta | 96 | 192 | 20–60 mm | 6–18 mm |

Individual speed streaks now represent an authored 8 ms visual exposure, previously
20–22 ms, and never exceed 0.32 m. They keep their existing trajectory, duration, and
white-hot/yellow/orange cooling palette. There is no camera-distance minimum width for sparks.
The global 24,576-grain frame budget remains unchanged; the larger emitter populations use
that same bounded budget.

The separate legacy exterior debris pool was also producing large incandescent chunks:
its 0.22–0.62 m base edge could render as a 0.93 m glowing cube near birth. Hot debris now uses
0.010–0.024 m base edges (at most 0.036 m when rendered), with three times as many grains.
The bounded pool increased from 48 to 144 entries to accommodate that density. Ordinary
non-emissive masonry retains its previous dimensions and count. These exterior legacy grains
are cosmetic and do not participate in damage; no collision diameter or energy was altered.

Armor emitters still receive the authoritative terminal contact and plate normal. This work
retains the existing outward offsets of 8 mm for ricochets and 30 mm for the brief impact flash.
The analytic exterior spark paths remain an acknowledged presentation approximation, separate
from the authoritative cabin-fragment solver.

Validation: the existing weapon-effects suite passed 8 tests (1 GPU review ignored), including
armor-contact timing, streak endpoint clipping, palette/finiteness, smoke separation, and the
fixed memory/draw-work budget. Log: `artifacts/runtime/spark_refinement_tests_20261001.log`.
The integrated firefight capture remains the visual acceptance check for subpixel spark density.

## Cabin catastrophe presentation correction

The final v4 cupola and interior review exposed bright, near-camera bars immediately after the
player's ammunition detonation. These were the generic exterior blast's 384 radial cosmetic
ejecta crossing the compartment, in addition to the separately simulated cabin fragments.
They were not enlarged authoritative spall or a smoke cube.

Player ammunition detonation and delayed fire cook-off now explicitly select `InteriorBlast`.
This retains the same expanding flash, six-second shared smoke emitter, pooled light and dust
timing, but emits no generic radial exterior ejecta. The choice is a presentation context, never
a radius or camera-distance heuristic. Ordinary exterior blasts and impacts retain the refined
spark population above; authoritative cabin fragment positions, trajectories and damage are
unchanged. The existing separate legacy hot-debris presentation is also unchanged by this fix.

The focused regression compares identical interior and exterior blast radii (0.4225 m and 3 m):
the flash matrices must match exactly, only the exterior adds 384 streaks, the interior retains
its shared smoke emitter after the flash, and exterior ejecta keep their existing lifetime.
Final v5 game captures remain the visual check for the close-camera result.

Validation: `cargo test --bin panzerrechnersystem weapon_vfx::tests -- --nocapture`
passed 9 tests, with the isolated GPU review ignored. Log:
`artifacts/runtime/interior_blast_spark_tests_20261001.log`. `git diff --check` also passed
for the two changed Rust files. This verifies the presentation separation; it is not a
substitute for the final v5 in-game camera review.

## V5 source-plane failure and correction

The v5 game review still showed cream-white bars at the fatal AP hit. The first correction
removed a duplicate cabin blast emitter, but did not remove this second cause. The live log
identifies a 76 mm BR-350B penetration of the Tiger's side at hull-local (2.06, 1.69, -0.65) m.
`resolve_authoritative_terminal` invokes `armor_sparks` before the ammunition catastrophe.
That function also created a generic `Impact`: 192 radial grains whose trajectories ignored
the authoritative plate normal. They could pass through the hull toward the crew cameras.
Their white period lasts 120 ms, matching the remaining white bars around 83 ms after impact;
the accompanying ricochet grains have already cooled to yellow after 55 ms.

Armor contacts now explicitly create `ArmorImpact(normal)`. Its spark hemisphere and flash
expansion rotate onto the actual outward plate normal. Every armor flash grain and spark,
including the ricochet streaks, is checked using its complete oriented cuboid support against
the source plane. The old ricochet positional clamp is removed; a grain returning across the
plane is no longer drawn, without moving it onto the plate. Counts, dimensions,
palette, speed magnitudes and gravity remain as above. Generic non-armor impacts are unchanged,
and no camera-distance rule or authoritative interior-fragment change is involved.

This is an explicit cosmetic source-plane approximation, not collision against the complete
hull. Nearby separate parts can still intercept a grain without being represented by this
analytic emitter. Regression coverage uses the translated live-hit area with all six axial
normals and two sloped normals, checking complete flash/streak support over 1.3 seconds,
initial spray density, draw bounds and unchanged generic impact distribution. The v6 probe
and subsequent final game captures are required to verify the visible close-camera result.

The updated weapon-effects suite passed 10 tests, 0 failures, with 1 isolated GPU test ignored.
Final-source log: `artifacts/runtime/armor_spark_plane_final_tests_20261001.log`.
`git diff --check` passed for
`src/weapon_vfx.rs`.

The final v6 cupola, loader-interior and exterior recordings passed their native combat and
motion receipts, complete media decoding and audio checks. Frame review confirms that the
large exterior streaks no longer cross the cabin; close exterior spray remains localized
around the armor hit. Full-rate video paths and provenance are recorded in
`docs/BLIZZARD_CREW_REVIEW_20261001.md` and `catalog/combat_crew_revision_20261001.json`.
