# Canyon Run: adaptive defence and field map

The Gaea canyon is the winter placeholder, not a surveyed historical engagement or the eventual
winter campaign map. The intended level lasts **60–120 minutes**, with a roughly 90-minute pacing
reference. Long runs and terminal failure support the owner's FTL-like replay structure. This is a
balance target; no timer prevents a skilled crew reaching the exit sooner.

The initial five T-34s are opening patrols, **not the total force**. `canyon_director.rs` admits more
armour throughout play according to furthest route progress, pace relative to the reference,
remaining living armour, close/engaged enemies, recent player discharges, kills, damaged modules,
crew casualties and fire. The target grows from three to twelve living tanks; a stalled or badly
damaged Tiger receives less replacement pressure. Damage opens a 120-second reinforcement respite,
and kills a 65-second respite. This only postpones new arrivals; tanks already present keep fighting.
Credits are capped at four so a long pause or prolonged fight cannot produce an enormous queued wave.
Pause is outside simulation time. There is no fixed total, kill quota, forced minimum play time,
health boost, accuracy change, free ammo or automatic repair.

Every request is subject to map admission. Candidates are stable knots ahead on the real route,
650–1550 m from the Tiger, and at least 650 m from the actual player's eye (including on foot).
The built terrain/nav grid must support every tank and its patrol legs.
Spacing is checked against living tanks and wrecks. Roof and silhouette probes must be hidden from
both the actual player's eye and the Tiger's elevated position through terrain/cover; view direction
and fog never excuse a visible arrival. If no legal entry exists the request waits. Crews start on
a route fixed by that entry, with no player target fix, and drive normally. They learn about the
Tiger through existing sight, hearing and radio rules. The approach near the exit can therefore
naturally run out of safe new entries; no spawn is forced to reach a number.

Reinforcements receive new actor, platoon, perception and renderer identities. Existing units and
casualties are never recycled, moved off-map, revived or despawned. Render geometry, materials and
built acceleration structures are shared; history buffers grow before upload. Original winter and
urban encounters retain their five-tank starting organisation. Canyon additions are under-strength
or full platoons of one to three, without magically respawning infantry escorts.

The physical renderer updates the sparse actor lookup while retaining its source geometry and
texture buffers. In the staged RTX 4090 test those updates took 1.76–1.86 ms of CPU submission work;
the former full repack took about 2.6 seconds. This is a local measurement, not a whole-frame or
two-hour performance benchmark. Capacity exhaustion still triggers a full exact repack.

Canyon death stops its run and returns to the title after the death presentation. Restart builds
an entirely new attempt and seed. No Canyon checkpoint can be saved/loaded; the menu's developer
invincibility choice does not apply to Canyon. Existing non-Canyon campaign save behaviour is outside
this change. Successful times remain local records, not a restorable run.

The editable drawing is `catalog/maps/canyon_survey_20260930.svg` and its exact runtime PNG. The
reproducible generator `tools/worldgen/canyon_paper_map.py` reads actual 20 m contours, 100 m index
contours, building footprints, placed trees, the route and real start/exit. North is -Z. Red pencil
marks the planned route; the map has no enemy positions or automatic player marker. The JSON records
source height hash and route; runtime checks route registration before applying the drawing. The
image becomes albedo on the existing folded, lit, non-emissive paper; pencil and pins share its UVs.
No AI terrain image, fabricated road network, brightened cabin lighting or screen overlay is used.

Validation and captures are recorded in `captures/canyon_adaptive_acceptance_20260930.json`.
The final host suite passed 1,414 tests (54 GPU/manual fixtures ignored), both shader and physical
reinforcement GPU tests passed, and the eight-seed/permadeath GPU regression passed. The staged
reinforcement checks fielded 14 tanks in total and preserved the same motion and wreck results in
both renderers. The release binary compiled. `git diff --check` and formatting of the new Rust
modules passed; repository-wide `cargo fmt --check` still fails on existing formatting, independently
reproduced from the unmodified baseline. No bulk reformat accompanies this gameplay change.
Director model tests are not player-completion evidence. GPU tests disclose staged positions,
casualties and clock advances. A fully armed 1–2-hour human playthrough, ammunition/salvage balance,
and whole-course sustained performance remain to be measured.
