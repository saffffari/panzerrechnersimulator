# Native pixel fire and soot — old renderer checkpoint

Implementation is saved in the isolated `old-renderer-review-20261001` checkout. CPU checks pass; GPU rendering, perceptual quality and performance have not yet been validated for this new pass. The parent task owns the final runtime receipt and physical-truth ledger. Main checkout has not received this pixel pass.

## Presentation

The raster path now omits registered flame, weapon-burst and soot parcels from opaque mesh draws. A dedicated HDR pass draws camera-facing quads with edges at consecutive **native target pixel boundaries**. Each accepted quad covers one native pixel. It uses that view's projection and extent, including the actual port target. The same formula works at640×360,800×450 and960×540; nearest window upscale changes the displayed pixel size, not the native quad width. This is a particle reconstruction rule, not a physical grain-size claim or subpixel antialiasing.

Parent transforms, particle trajectories, armor-plane constraints, whole parcel support, color, emission and aggregate fire-light powers remain unchanged. Continuous shell tracer beams have their own original mesh pages. Emissive weapon and exterior parcels start at64 procedural samples each; the existing6–8mm cabin flame parcels and all soot parcels start at1. Tiny soot therefore avoids63 redundant samples while preserving its area-weighted optical depth. Their required counts then grow with projected source silhouette area, rounded up to a power of two. No individual count is driven by the window's upscale extent.

Only parent records are uploaded. Shader-generated sample positions use one fixed low-discrepancy local prefix, attached to the moving parent transform. There is no per-frame shuffle. Persistent integer IDs from the simulation now decorrelate those prefixes. All four identity words (domain, emitter, parcel, generation) enter the stable hash. Color-page/instance indices do not enter the hash because they change when particles cool, expire or migrate pages. Missing identities are reported and disallowed by the capture gate.

The source box or source faceted soot lobe determines sample support and projected area. The soot samples lie within tetrahedra formed from the actual12 source vertices and20 source triangles. The prototype also retains its deterministic64-way geometric refinement for the unconverted fallback path; it is not drawn in addition to the native-pixel path.

## Energy, coverage and visibility

The quad carries projected parent silhouette area divided by sample count. Unsaturated density gives a per-sample weight of at most0.5. Fire radiance is accumulated additively; matte soot uses premultiplied coverage `alpha=1-exp(-weight)`. These conserve the chosen finite quadrature's projected emission and additive optical depth as count changes. They do not increase aggregate physical source-light flux to make up for smaller presentation grains.

The pass runs after raster participating media and before eye metering, bloom and crush. Fragment depth is compared with the actual current scene depth. Flame attenuation and soot illumination reuse the live raster volume shader's weather/local-medium functions, sky visibility atlas and registered source-geometry cubemap lighting. Soot receives lit single-scattering radiance; it does not reuse an unlit fire color. Foreground participating-medium radiance is included in the soot over-composite so it is not incorrectly darkened by a particle behind it.

Parents are globally sorted back to front across all colors and sample counts. Only adjacent equal-count parents share a draw. **Individual samples within one parent are not depth sorted**, so overlapping translucent grains still use a finite ordering approximation. The preexisting raster medium's source/moment approximation, sky-direction discretization and finite cubemap visibility limits remain; this change does not make that renderer exact transport.

## Budget and receipts

- Maximum65,536 procedural samples per draw; larger parents split into contiguous sample-prefix chunks without restarting their sequence or changing their total-area weight.
- Hard maximum8,388,608 submitted samples per view. Optional minimum-count and power-of-two padding is reduced first. If necessary coverage itself exceeds that budget, whole parents are omitted and the missing projected area/emission proxy is reported. There is no overbright single-grain replacement and no conservation claim for omitted parents. Accepted captures require zero saturation, zero missing identities and maximum weight≤0.5.
- Receipts include actual view extent, admitted parent count, required/requested/submitted grain count, saturation and density-reduction counts, omitted projected area/emission proxy, missing identities, near-plane intersections, maximum weight, draw calls, upload bytes, instance capacity and buffer allocations. These are submitted procedural sample counts, **not measured surviving fragments or visible pixels**.
- Near-plane intersections are clipped in homogeneous coordinates. The projected convex support is reconstructed from the clipped vertices, including the actual cut face. Projection is camera-relative to avoid large-world cancellation.
- Each main/port view retains its uniform and geometrically grown instance buffer; unchanged capacity does not allocate a new GPU buffer. Changing render resolution preserves registrations/identities but recreates view-dependent bindings.
- Many adjacent sample-count runs can raise draw-call cost. Runtime GPU timings are required before asserting playability.

## Checks completed

`artifacts/runtime/native_pixel_fine_vfx_refinement_tests_20261001.log`:11/11 pass. Includes exact crossing-triangle/cut-face clipping, large-world rebasing, tiny-soot work reduction, hard-budget omission behavior, contiguous chunking, reusable-buffer capacity and stable identity hashing. Earlier `artifacts/runtime/native_pixel_fine_vfx_tests_20261001.log`:5/5 pass. Covers640/960/port extent, FOV/depth scaling; projected energy/optical-depth arithmetic; composed shader validation; static prototype area/support preservation and deterministic grain extent.

`artifacts/runtime/native_pixel_shared_regressions_20261001.log`:23/23 pass,1 preexisting ignored GPU diagnostic:14 weapon,2 persistent surface identities,4 graphics preferences and3 source-geometry repair tests. Earlier `artifacts/runtime/native_pixel_weapon_vfx_tests_20261001.log`:10/10 pass,1 preexisting ignored native diagnostic. Continuous tracer endpoints, armor-plane support, catastrophic cabin presentation and bounded effect lifecycle remain covered.

The earlier interior fire-shadow registration remains included:25 finite cubemap source slots (8 practical+8 world+8 interior+torch),12/12 focused registration checks and14/14 raster shader/ABI checks passed. No new source-power change was made by that registration fix.
