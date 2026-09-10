# Spring terrain color correction

The Act-II and Act-III source ground drapes contain dark olive/green vegetation and brown soil.
Both raster shaders previously discarded source RGB, retained only luminance, then substituted a
narrow grey-brown seasonal palette. Exact secondary ray hits sampled the unmodified source instead,
so primary and secondary surfaces also used different materials.

`shaders/terrain_material.wgsl` now supplies the same seasonal material to portable forward rendering,
the High G-buffer, and exact High terrain ray hits. In spring, microdetail and bank darkening multiply
the actual linear source RGB by scalars, preserving its channel ratios. The winter snow treatment
and existing seasonal roughness remain. This changes surface reflectance, not ambient light, exposure,
emission, or the atmosphere.

The existing source compositor (`tools/worldgen/make_ground_tex.py`) already blends Pizza dirt,
grass and forest litter using land-class masks, and stamps actual generated roads/craters into the
drape. The shader keeps those material distinctions instead of adding another uniform soil palette.
These colors are source artwork, not measured reflectance or a newly calibrated soil-wetness model.

The terrain RT tag follows the renderer's existing `track_recv` designation using material flag bit 1;
it does not assume terrain is scene slot zero. High ray hits use the same normal microdetail and
roughness functions. Secondary source maps currently use mip zero, so their detail is unfiltered;
raster still filters microdetail by pixel footprint. The existing tank-track map is raster-only and
remains a separate known secondary-material discrepancy pending its own binding change.

Validation so far: the actual source PNGs were inspected through 32 × 32 crops, decoding only the first
32 scanlines (583 KB and 451 KB read, respectively), and source/manifest wiring was checked. Evidence is
in `artifacts/astra-audit-20260905/terrain-source-color-proof.json`. The new renderer Naga test
`campaign_terrain_material_compiles_in_forward_and_deferred_entries` covers both raster entries;
`live_physical_environment_entries_parse_and_validate_together` covers the composed High shader.
The author of this patch has not run Cargo, GPU jobs, or a new visual capture; root coordinates them.

# Separate foliage issue, not changed here

`artifacts/astra-audit-20260905/tree-material-alpha-proof.json` records exact source GLB SHA256s,
material metadata, triangle counts, actual default-scene traversal, and current code evidence.
The loader combines primitives into one material; both `tree_5` and `pine_tree_n_1` visit their opaque
trunk first, then their differently textured BLEND leaf/branch root. As a result, later foliage gets
the bark texture. Node-array ordering is the opposite and must not be mistaken for scene traversal.
Normal tree batches also bypass the only raster alpha discard (grass mode), and current ray queries
force opaque hits. A later foliage correction must retain per-material geometry and apply its source
alpha consistently; simply turning on alpha rejection for the existing merged bark texture cannot
recover the missing leaf material.

Act-III worldgen currently contains a Halbe prototype. The canonical campaign finale remains
Berlin/Reichstag; this terrain correction does not change that campaign decision.
