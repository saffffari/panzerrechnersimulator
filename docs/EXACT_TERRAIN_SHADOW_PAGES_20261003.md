# Exact terrain shadow pages — 2026-10-03

The full-resolution Canyon terrain contains about 2.1 million triangles. Previously a
shadow direction that saw any part of its broad bounding box submitted the entire terrain
index buffer. Small local environment tiles and every moving practical-light cubemap paid
that vertex cost even when most triangles projected outside their depth image.

`terrain_shadow_pages.rs` builds a separate shadow-only index ordering for large, broad meshes.
The runtime uses it only for a terrain mesh marked as a track receiver. It copies every source
triangle triplet once, preserving its vertex indices, winding and multiplicity, into 32×32 XZ
spatial buckets. Bucket assignment uses the centroid; each bucket's actual bound includes all
three vertices of every assigned triangle, including triangles crossing bucket boundaries.
Clipped terrain openings require no special case or replacement geometry.

A hierarchy over contiguous Morton-ordered buckets rejects spans outside the actual shadow
frustum or without a possible shadow texel sample. It reuses the renderer's conservative
sample-coverage test and its 1/8-pixel guard. Whole hierarchy nodes entirely inside the
frustum become one index range, and neighboring selected ranges merge. A broad cascade that
contains the complete terrain therefore retains one full-buffer draw. Invalid projection
inputs fall back to submitting the full terrain.

The primary raster and hardware-ray index buffers remain unchanged. No vertex, height,
material, LOD, caster distance, source power, resolution or shadow-map sampling budget changes.
The additional GPU allocation is four bytes per terrain index: about 24 MiB for the current
2.1-million-triangle terrain, plus a small CPU bounds hierarchy. Mutable small actor meshes
cannot acquire this metadata. Point-light static/dynamic mesh loops use the new methods;
environment visibility integrates them in its separately coordinated cache patch.

CPU tests check exact triangle multiplicity/winding, bounds containing cross-cell triangles,
full-cascade range collapse, invalid-projection fallback, and retention of all potentially
sampled source triangles across four narrow orthographic views. An explicitly scheduled GPU
test renders the production `point_shadow.wgsl` with original indices versus culled page
ranges at two world origins and three cubemap directions, requiring identical Depth32 bytes
and confirming the fixture actually rejects triangles and renders terrain.

At source handoff, `git diff --check` passed. Compiled tests, the GPU equivalence run and
measured native frame-time comparisons remain coordinator-owned gates. No speedup is claimed
from source inspection alone.
