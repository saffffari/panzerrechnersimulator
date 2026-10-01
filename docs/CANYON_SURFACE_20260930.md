# Canyon v2 surface review, 30 September 2026

Generator version 5 retains the native Gaea heights, 4.93 km route, 249 buildings and encounter
layout. V1 is unchanged. The canonical v2 workshop has been regenerated; the previous v4 terrain
materials, masks and blood meshes are preserved in `gaea-canyon-v4-preserved` for comparisons.

The derived material map now breaks up snow coverage along traffic bands, yards and exposed
banks. The shader resolves mixed snow and soil using filtered source detail textures. There are
no new geometric ruts or fabricated terrain normals. At 4096 texels over 5 km, the authored map
spacing is about 1.22 m; narrow features use fractional coverage.

Winter grass occupies 91 isolated stands, with clear roads and yards. The authored mask is the
clearing authority, so a second noise field no longer erases entire allowed patches. Just 0.1735%
of the full source map exceeds a quarter cover value. This is an authoring statistic, not a
historical measurement. Separate stalk props were reduced from 1,423 to 10; runtime grass supplies
the stands. Near stems are about 6 mm wide, with 13/24 mm representatives at coarser LODs. Straw
albedo replaces olive. The grass review camera resolves 1,170 roots and 7,946 visible stems.
Explicit survey cameras now refresh ordinary local grass after moving away from the Tiger.

The previous large, nested pink/red deposits have been replaced by 49,983 small closed cuboids
around the existing 753 anonymous casualty placements. Their widths are 6–32 mm and heights
3–10 mm. They are irregular static authored aftermath, not simulated wound output. Summed
cuboid footprint is about 12.46 square metres across the whole map, including overlaps.

Reviewed native evidence in `captures/`:

- `canyon_surface02_player_20260930.png`: broken road edges, sparse vegetation distribution.
- `canyon_surface03_splatter_20260930.png`: exposed small red drops beside a casualty.
- `canyon_surface_final_grass_20260930.png`: finer dry stems in a surviving patch.
- `canyon_resolution_720_player_20260930.png`: unchanged 1280×720 sensor comparison baseline.

The first three images and capture receipts were sent to the Pi and their remote hashes checked.
The low-resolution style remains; no display-resolution change was promoted. Weather experiments
are separate workshops; see `CANYON_STORM_20260930.md` for the combat review.

Visual references and their limitations are recorded in
`research/CANYON_FOOTAGE_DIRECTION_20260930.md`. Black-and-white winter photographs support patchy
stubble and disturbed snow, not a measured color palette or a claim that no grass existed.
