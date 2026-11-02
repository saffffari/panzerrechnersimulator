# Geometry cost and visual LOD audit, 31 October 2026

Read-only source and GLB-header inspection after build 39. No assets were generated or changed, and no Cargo or GPU work was run by this subagent. The parent reports build 39 and its 16 focused tests passed. Its matched instance-range benchmark was still pending when this audit was written.

## The missing capability

There is no implemented near/mid/far visual LOD selector for the live Tiger, live T-34, ordinary instanced wrecks, or their optical captures. Their loader selects one source mesh set and continues to use it. A directory named `half` is a fixed derivative, not a runtime distance- or pixel-error-driven LOD system.

The engine has useful separate pieces: terrain CDLOD for detail-enabled maps, scatter-only geometry tiers and GPU indirect submission, exact rigid/terrain triangle paging, and a reduced environment-shadow occluder. None of those currently supplies a complete general vehicle/world visual LOD path. `ART_BIBLE.md` asks for explicit near/mid/far enemy vehicles and props; that target remains incompletely implemented.

## Source geometry counts

Counts below come from triangle-mode GLB accessor index counts and node references, read directly from the existing JSON chunks. They are source/resident totals, **not measured triangles submitted in a frame or visible pixels**. Runtime partitions, source additions, hidden parts and clipping can change the actual draw total.

| Existing source | Triangles |
| --- | ---: |
| `assets/tanks/tiger/baked/tiger_hull.glb` | 3,234,818 |
| `assets/tanks/tiger/baked/tiger_turret.glb` | 372,655 |
| `assets/tanks/tiger/baked/tiger_gun.glb` | 43,122 |
| Full Tiger three-part total | **3,650,595** |
| `assets/tanks/tiger/baked/half/tiger_hull.glb` | 1,674,131 |
| `assets/tanks/tiger/baked/half/tiger_turret.glb` | 388,333 |
| `assets/tanks/tiger/baked/half/tiger_gun.glb` | 29,571 |
| Fixed `half` Tiger three-part total | **2,092,035** |
| T-34 hull / turret / gun | 122,851 / 10,067 / 846 |
| Live T-34 three-part total | **133,764** |
| Source KV-1 wreck / Panzer III wreck | 270,639 / 183,209 |
| Source Tiger II wreck / Panther wreck / T-34-85 wreck | 251,159 / 203,354 / 149,827 |

The full Tiger hull contains 1,215 named glTF meshes, but the runtime loader merges the remainder into a large rigid body after splitting selected moving components. The turret contains 172 meshes before partitioning. The source's fine part granularity is therefore not automatically a draw-culling granularity. The biggest individual hull source nodes include `Hull_0619` at 66,798 triangles, engine core `Hull_0971` at 64,684, and `Hull_0708` at 46,960; the cost is distributed through many components rather than residing only in one extravagant engine mesh.

### The actual Act-I benchmark map

`assets/worldgen/act1/manifest.v1.json` references 13,451,145 placed source triangles before visibility. Five fixed `half` Tiger sets contribute 10,460,175 of those triangles, approximately **77.8%**. The next large contributions include 490 broken-wall modules at 1,662 triangles each (814,380 total), three 183,209-triangle Panzer III wrecks (549,627 total), and one 270,639-triangle KV-1 wreck.

The log also reports three additional abandoned live-source Tigers allocated by `Game::build`, beyond the manifest's dressing, and the player's source Tiger. These are not necessarily all visible simultaneously. The b37 frame contains 18,433 TLAS instances, 1,761 instance batches and 23,816 instances; neither this count nor a BLAS count establishes submitted triangle cost.

The authored ground is a single 1,024-square grid: **2,093,058 triangles**. Its manifest has no `terrain.detail`, so the existing terrain CDLOD is inactive. A view intersecting the large terrain mesh AABB can submit the full terrain index buffer.

### The new theatre is a different workload

`assets/worldgen/expedition/the_march_v1/manifest.v1.json` has a 1,025-square, 12 km ground grid: **2,097,152 triangles** before openings or runtime modifications. It also has no `terrain.detail` block.

Its manifest dressing totals 12,679,121 placed source triangles. Two 1,250-triangle pine variants account for 9,834 placements and **12,292,500 triangles**, approximately 97% of that manifest total. This excludes separately installed building assemblies, camp, live vehicle rigs and actors. Forests need measured per-view instance culling and LOD; the Act-I Tiger-wreck percentages must not be applied to the theatre.

## What the existing mechanisms actually do

| Mechanism | Implemented coverage | Important limit |
| --- | --- | --- |
| `tank.rs::build_tiger`, `build_t34`; `tiger_hatches::load_player_tiger` | Full source, authored moving partitions and material overrides | No visual tier selection or error metadata |
| `terrain_lod.rs`, `renderer_raymarch.rs::make_terrain` | CDLOD for an attached `terrain_relief::Detail`, crack-free morphing and relief function | Optional manifest block; inactive in both inspected maps; its 16 view slots cover primary and shadow views, not raster optical captures |
| `renderer_scatter.rs` | GPU cell/instance culling, indirect draw buckets, source/mid/pixel tiers for the separately loaded scatter field | Not ordinary manifest instances or live vehicles; reduced tier is coarse clustering, pixel tier uses mean albedo/up normal, no complete PBR transfer, previous-frame Hi-Z has a disclosed disocclusion delay |
| `sky_visibility.rs::clustered_occluder` | Source-derived 6 mm clustering, maximum vertex displacement 10.4 mm, for the dynamic environment-visibility atlas | Not camera geometry, ordinary sun/lamp shadows, or vehicle LOD |
| `terrain_shadow_pages.rs` | Exact full-source terrain triangle pages, hierarchy/frustum/sample rejection | Used for shadow submission, not primary or optical loose geometry |
| `rigid_triangle_pages.rs` | Exact full-source rigid pages, primary-frustum or shadow-sample selection | Disabled unless `PANZER_EXACT_RIGID_PAGES=1`; local X/Z partition only; reordered triangle indices require depth-tie image proof; posed/deforming meshes bypass it |
| Existing wreck preparation script | Fixed derivative copies with preserved originals/materials | No runtime tier selection, screen-space error metadata, or measured geometric-error admission |

The raymarch primary path already invokes `m.rigid_index_buffer()` and `m.primary_index_ranges(vp)`. The optical forward path instead binds `m.ibuf` and draws `0..m.index_count`; it does not consume those rigid pages even when present. This is an integration gap, not a claim that enabling the old candidate unconditionally is already safe.

The main Tiger's source-shadow exclusion is limited to the relay environment-visibility atlas. It does not exclude that hull from ordinary optical scene geometry. It is an existing documented lighting approximation, not a general culling optimization to extend.

## Recommended architecture sequence

### 1. Make complete source geometry spatially submit-able

Unify conservative source-cluster selection across the primary view and every actual objective camera. Begin with the full terrain and large rigid Tiger/wreck bodies, because they can currently be accepted as a whole despite a small visible portion. Keep vertices, complete triangles, material attributes, collision and ray-hit identities unchanged. Use source part ranges or three-dimensional clusters for the Tiger; an X/Z partition with full height leaves unnecessary cabin/exterior overlap.

Reuse existing tested bounds and triangle-page construction where appropriate, but retain original draw order or prove the particular source cannot have competing equal-depth surfaces before reordering. A single-valued terrain surface is a better first admission target than arbitrary overlapping CAD parts. Moving hatches, removed workshop components, dynamic damage and every other owner must invalidate or update their own cluster transforms without changing component identity.

Avoid replacing one expensive draw with thousands of CPU calls. The scatter path demonstrates indirect draw support already requested by `main.rs`; a general source-cluster path can build compact surviving commands while preserving deterministic order. Frustum rejection alone is exact. Any later occlusion stage needs a current-frame conservative proof or an explicit temporal error policy; do not silently reuse the scatter system's one-frame disappearance behavior for the cabin or enemies.

Measure loose and instanced submitted triangles by source family and per view, retained cluster counts, command counts and CPU preparation/encoding. The current b39 per-instance counters do not cover loose terrain or the player's monolithic hull. Resident asset counts cannot replace those measurements.

### 2. Add actual render-only, bounded-error vehicle and world tiers

Create source-preserving near geometry plus reviewed lower-detail derivatives with numerical geometric-error metadata. Select separately for the primary eye, magnified gun sight, each objective capture and shadow sample footprint. Distance alone is insufficient: magnification can make a distant vehicle large. The original simulation/contact/repair meshes and stable component identities remain authoritative.

Error should be evaluated in native projected pixels from a declared source-space bound, not by an arbitrary triangle percentage. Keep the original for arm's-length repair, interacting components, vision apertures, gun bore/muzzle, moving seams, exposed interior, and damage states lacking an admitted derivative. Retain each moving assembly's pivot and topology ownership. Preserve material seams, source texture/normal/roughness channels, and significant silhouette openings. Use hysteresis or a documented transition strategy without making components physically appear/disappear.

The existing `half` copies, scatter clustering and 6 mm occluder are not sufficient certification. A simplifier's target face count does not prove a geometric or silhouette error bound. Admission needs source/derivative measurements and rendered near/mid/far comparisons at both intended native resolutions, including scope magnification, grazing silhouettes, hatches, damage and repair. This is deliberately a numerical rendering approximation; it should never be called triangle-identical physics.

Start with distant repeated Tiger wrecks in Act I, T-34 hulls, and theatre pines. Preserve the player's near mechanical detail. This targets substantial geometry instead of reducing the visibility or brightness of nonzero optical views.

### 3. Solve the independent atmospheric and forward-overdraw costs

Even excellent geometry LOD cannot remove the measured 8.3 ms of relay sky integration, and it does not automatically eliminate the forward shader's repeated 32-direction environment solve. The parallel architecture work is a shared physically specified clear-atmosphere representation with validated numerical error, plus actual-objective cloud evaluation, and a safe opaque visibility/shading separation. The optical audit describes alpha, coplanar-winner and MSAA requirements for a depth prepass. These are substantive renderer changes with visual and physical acceptance gates, not preset switches.

## Completion criteria

The b37 baseline is 72.26 ms per live frame at Ultra 640 x 360. The parent reports changing only sample coverage to single-sample improved approximately 13.85 to 14.50 FPS, so MSAA is not the main remedy and that diagnostic is a quality tradeoff.

A successful architecture step needs an attributable pass-time reduction and full-frame improvement under matched scenes, camera motion and combat. Target 16.67 ms for 60 FPS and 33.33 ms for the requested 30 FPS floor. Validate the near interior, optics, repair closeups, forest traversal and several distant tanks. Do not claim the target from a source triangle reduction or an empty scene. No speedup from the architecture proposals above has yet been measured.
