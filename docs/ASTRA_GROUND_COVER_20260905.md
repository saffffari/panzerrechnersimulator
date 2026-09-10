# Astra ground cover, 5 September 2026

The old runtime grass was a 0.8 m crossed alpha card, with about 46 clumps per 24 m pasture cell. Its local-axis sway changed wind direction with random instance yaw. The low-resolution tank rut texture darkened terrain but did not flatten grass geometry.

`src/grass.rs` now places closed, tapered stalks from `src/grass_filament.rs`: 16 triangles, three bending intervals, and an 18 mm diameter envelope before modest random scaling. There are at most 16,384 roots within 26 m of the camera. Soil class, irregular patches and distance reduce that count. Later acts use restrained early growth rather than a uniform tall summer lawn; winter has sparse dry stalks. A tiny opaque palette texture supplies color only.

The stalks share the existing world wind used by smoke and weather. Eight-metre wind cells integrate a critically damped spring; per-vertex quadratic bending fixes the root and root tangent. Both forward and G-buffer shaders apply the same deformation and its normal correction. Treads use two narrow physical footprints, not the enlarged low-resolution rut decal. Contact is remembered on 25 cm cells, with 64 heading bins, and roots hinge toward a flattened pose with strongly reduced wind response. Memory survives camera streaming and revisits within this Game instance. Compact pages bound its payload to 64 MiB and never evict old marks; no save-game persistence is claimed.

The fixed instance pool is raster-only. High's primary G-buffer sees the actual solid stalks and their depth occlusion; they receive light transported through the ordinary scene. Grass is currently absent from secondary rays, reflections, cast ray shadows and grass self-shadowing. This is a coarse filament approximation, with no fibre collision solver. A render is needed to assess the apparent pixel width and density; CPU topology and proposal counts alone are not visual validation.

## Winter placement gap and source retention

The tracked Act I manifest contains only 24 `pine_tree_n_1` and 6 `pine_tree_n_2` placements, plus 4 stumps and 4 logs. It contains no bare deciduous trees. The former always-on story overlay added no trees.

The seasonal overlay adds small, broken property/field-boundary groups along the travel corridor while retaining the protected central combat area and open views. `tree_5` and `tree_7` retain every original woody triangle and omit their separately authored leaf roots: each variant retains 385 wood triangles and removes 184 leaf triangles. Existing bare `tree_8` and `tree_9` provide additional silhouettes. All four variants enter the normal Trees collision/felling subsystem. Existing Pizza logs, stumps, stones 2/3 and restrained later-act ferns add nearby ground objects. No standalone fallen-leaf or twig asset was found in the already catalogued Nature vocabulary; no replacement texture squares were fabricated.

A CPU mirror of the current recipe yields 79 / 60 / 62 added bare trees in Acts I / II / III, respectively. Counts and exact source-data evidence are in `artifacts/astra-audit-20260905/grass-worldgen-cpu.json`. Runtime tests check bounded placement, root routing, and retained source triangles. The placements are seasonal inference, not surveyed eyewitness locations. April visual guidance comes from the Faminsky images cited on the Seelow museum's exhibition page, https://www.seelowerhoehen.de/ausstellung-detail.php?id=5 . The existing Act II/III macro manifests still contain leafy assets. The current Act III Halbe prototype is not the canonical Berlin/Reichstag finale.

## Separate pasture proof cameras

These actual red-channel pasture sites provide useful close grass views without changing the soil to suit the camera:

| Current map | Eye (m) | Target (m) | Vertical FOV | CPU proposed roots / within 5 m |
| --- | --- | --- | --- | --- |
| Act II, Canal Bridge property margin | (-82, 1.16245, 360) | (-73, 0.65143, 368) | 66° | 4,603 / 846 |
| Act III prototype, Rail Station clearing | (338, -2.58405, 430) | (349, -3.04198, 420) | 66° | 4,340 / 582 |

The first Act II survey grove is on the authored bare-ground mask and should not be described as a dense-grass proof. The whole-world survey records actual vegetation counts and camera coordinates; these optional close views are additional evidence.
