# Exact rigid paging candidate

Candidate is disabled unless `PANZER_EXACT_RIGID_PAGES=1` at mesh upload. Compare the same capture, camera, source assets, seed, and settings with it absent versus enabled. Coordinate GPU runs with the main agent; do not run alongside the live game.

Large meshes (at least 32,768 triangles and more than 4,096 vertices) receive a separate spatially ordered index buffer. Source vertices, per-triangle winding, multiplicity, material, UVs, primary index buffer and hardware ray primitive identities are retained. Each triangle belongs to one centroid page; the page bounds contain all three complete source vertices, including triangles crossing page boundaries. Projection-invalid input falls back to full source draw. Posed/dynamic geometry bypasses pages and successful pose uploads invalidate the candidate buffer.

Opaque main and probe views reject pages only outside their frustum, with no shadow-sample rejection applied to MSAA main views. Source environment shadows use the existing conservative frustum/sample coverage test. Full-frustum pages coalesce into one range. Current subdivision is local X/Z with full local Y bounds, so vertical separation may limit efficiency but cannot omit in-frustum triangles.

Validation required before default enablement: CPU coverage tests (`renderer::rigid_triangle_pages::tests`), same-source production shadow depth byte comparison, textured main-view color comparison at fixed exposures, ports/ADS and moving hull views, and GPU pass timings. Original source triangle ordering changes in the candidate raster index stream: exactly coplanar overlapping textured faces could produce different depth-tie color winners. This is a reason to keep the candidate disabled until color proof, not a claim of pixel equality.

No LOD, vertex approximation, caster dropping, shadow throttling, material substitution, or measured performance claim is part of this change. Additional index upload and page draw overhead may outweigh culling for broad views. Parent static rigid-depth caching is independent and likely offers a larger stationary hull improvement.
