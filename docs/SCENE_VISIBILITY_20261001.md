# Portable exact-source visibility

`src/scene_visibility.rs` builds an immutable object-space triangle BLAS for each distinct source mesh and a refittable TLAS for the current instances. `src/scene_visibility_gpu.rs` owns two read-only storage buffers; `shaders/scene_visibility.wgsl` traverses their packed data without hardware ray-query features. The renderer integration is separate from this module.

## Geometry and update contract

Register source `CpuMesh` values with `register_mesh`. Positions, triangle indices and UVs are preserved. Content dedup hashes those fields and source texture alpha, then compares the complete actual values against the packed candidate before reuse. A hash collision cannot alias different geometry. Color/normal changes alone do not change visibility. Each shared placement uses the same `GeometryId`.

Call `update_instances` with every relevant instance, including terrain, its current invertible affine transform, stable ID, geometry ID, material class and material ID. Stable topology refits bounds. Changing topology rebuilds the TLAS. An unchanged snapshot produces no new instance upload revision. Transforming rays into object space without normalizing their transformed direction preserves the world-ray parameter under nonuniform scale. Both faces of every opaque triangle block; AABBs only accelerate traversal.

This API does **not** make camera-frustum culling a valid occluder selection rule: a mesh behind the camera can still block a sky/light ray. The renderer must submit all potential occluders for its claimed domain. Rigid hull/turret/limb motion only changes the instance transform. A deforming actor gets a private ID with `register_deformable_mesh` before first deformation; `refit_mesh` then updates its positions and BLAS bounds in bounded storage. Index topology, UV and alpha changes are rejected. Cloning an actor that already has a private ID requires another private ID before either actor deforms. A position refit invalidates the next TLAS update even if instance transforms are unchanged.

Solid-grass placements provide `bend_world`, the same actual world X/Z tip displacement packed by the rendering shader. Visibility converts it through the affine inverse, applies the same `clamp(source_y / 0.48, 0, 1)^2` displacement to original triangle vertices, and expands each BLAS node and instance bound conservatively over that displacement. Grass wind changes instance data without duplicating the shared BLAS. This covers the current solid-grass vertex bend; unrelated future shader deformation is not automatically covered.

Bounds expand only by floating-point rounding/error bounds. Balanced BVHs use forward escape links, so no finite traversal stack can overflow. Invalid transforms, coordinates, indices, payload links, and storage-binding size limits fail explicitly. No source triangles are silently simplified, culled by facing, truncated to a buffer, or replaced by boxes. Normal finite-precision intersection error remains a numerical limitation.

## GPU contract

`shader_source(group, geometry_binding, instance_binding)` supplies exactly two `array<u32>` read-only storage declarations. All payload addresses are **word offsets**, not byte offsets. The source query is `scene_visibility_trace(origin, direction, lower, upper)`. Its result includes geometric world normal/front face, barycentrics, source primitive ID, instance ID, material ID and a mandatory status:

| Status | Meaning |
| --- | --- |
| 0 | Clear finite ray segment |
| 1 | Physical opaque or alpha-covered triangle |
| 2 | Dielectric interface; transmission must be evaluated |
| 3 | Unresolved material/connection; neither physical clear nor physical black |
| 4 | Invalid input/payload/numerical result; fail and diagnose |

`SceneVisibilityGpu::prepare` validates both required buffer sizes against the device limits before either upload. It uploads changed revisions only. A bounded journal merges changed geometry word ranges; position refits upload only the affected BLAS nodes and vertices, not static world geometry, source indices, or alpha textures. If a consumer falls behind the journal or a buffer reallocates, it receives a complete geometry upload. `bindings_changed` requires the renderer to rebuild bind groups because backing buffer allocations changed. `binding_generation` also exposes this lifecycle. Call `invalidate` if replacing the CPU scene registry behind an existing GPU owner. The helper is not an automatic renderer integration.

## Source alpha

Alpha-test triangles sample packed source alpha at interpolated source UV, nearest/repeat, level zero, cutoff 0.5. Transparent texels continue to geometry behind them. This represents the actual level-zero coverage; it does not claim derivative-based raster mip coverage, multisample alpha coverage, or deformation other than the explicit solid-grass bend described above. Non-alpha material classification must be provided explicitly by the renderer.

## Glass connection boundary

`transmission` / `scene_visibility_transmission` accept caller-provided source material ID, refractive index, and RGB absorption coefficients per metre. For paired parallel panes, actual first/second source-triangle hits supply the straight connection's inside distance. Each interface receives unpolarized dielectric Fresnel transmission and the measured inside distance receives Beer attenuation. Multiple nonnested panes multiply transmission. Normal-incidence source protective-glass values can be supplied by the existing protective-glass material; this module does not invent tint or opacity.

This is a **thin/parallel-window straight-connection approximation**. It omits the bent ray's internal/lateral displacement, internal-reflection transport, rough transmission, and dispersion. Fresnel uses the Snell-related inside angle, while Beer uses the measured straight connection distance. It is consequently not an exact refracted-path solution at oblique angles. Different source material IDs, open meshes, a start/end inside glass, nested media, nonparallel surfaces, or more than 16 traversed panes return explicit unresolved status. `transmission_to_emitter` / `scene_visibility_transmission_to_emitter` can end at a nominated exact emitting instance surface. Armor or other geometry encountered first still blocks; no arbitrary emitter-radius sphere removes occluders. An emitter reached during an unpaired glass interval remains unresolved.

An unsupported connection must not be presented as physically opaque or clear. The renderer must handle/report that boundary. Advancing past a boundary uses only the next representable ray parameter, not a fixed geometry-skipping epsilon.

## Verification

The focused tests cover triangle-vs-AABB distinction; open/terrain-style surfaces blocking both sides; negative and nonuniform live transforms; 10,000 instances sharing one source triangle; unchanged/refitted/removed instances; level-zero alpha holes and repeating UV; invalid input and corrupted traversal links; deliberate hash collisions; 1,000 deterministic rays checked against direct transformed source triangles in f64; paired-glass Fresnel/Beer and unresolved boundaries; buffer-limit errors; and complete Naga validation with exactly two storage bindings.

An explicitly ignored GPU test, `scene_visibility_gpu::gpu_tests::portable_gpu_payload_matches_cpu_hits_alpha_glass_and_refit`, creates a device without hardware ray-query features and compares 512 actual compute queries against the CPU packed-payload interpreter, including alpha, glass, solid-grass bending, a moved instance, and a private vertex refit with partial upload. Run it explicitly with `--ignored`; a normal focused test run does not execute that GPU test. Test success alone does not prove the shipping renderer submits all geometry or calls the query on every light path.

Initial compute execution passed on llvmpipe OpenGL, a software adapter, without hardware ray-query features. This establishes portable compute/upload agreement on that adapter; it is not an RTX performance claim. The native renderer still requires its own full-scene execution and visual/receipt validation.
