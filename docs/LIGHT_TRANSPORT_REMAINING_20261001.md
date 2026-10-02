# Remaining transport limits in the active shader renderer

> Historical baseline audit, retained for provenance. Portable source-triangle visibility and
> cabin-medium integration landed after this inspection. The atlas/grass/direct-visibility rows
> below are not a current-status claim. See [CABIN_ATMOSPHERE_VISIBILITY_20261001.md](CABIN_ATMOSPHERE_VISIBILITY_20261001.md)
> for the integration checkpoint and [SCENE_VISIBILITY_20261001.md](SCENE_VISIBILITY_20261001.md)
> for the source geometry, deformation, glass and GPU verification boundaries. Native visual
> acceptance and unresolved transport still require their own current receipts.

This is a read-only source audit of the Canyon shader backend after the v6 review, during the
2026-10-01 corrections. It is not a claim of visual acceptance or complete physical transport.
`src/graphics.rs:129` disables hardware ray queries for this backend; `src/renderer.rs:3521`
uses `stylized_mesh.wgsl`, and its weather particles use `stylized_particles.wgsl` at line 5249.
The separate High ray-query shaders do not establish what these captures render. Line numbers
identify the inspected functions and can move as the concurrent corrections land.

The corrected point-source shadow admission and cabin-medium partition do not establish that
every remaining light path has scene-complete visibility or medium transport. The following
limits still require explicit implementation and validation; replacing an unknown visibility
with black would suppress light but would not solve its transport.

| Active path | Observed limitation and consequence | Work required |
| --- | --- | --- |
| Environment visibility | `shaders/sky_visibility_sampling.wgsl:11` returns 1 outside the tile or depth range, and line 17 blends to 1 across its outer eighth. `shaders/stylized_mesh.wgsl:172` also returns 1 for inactive directions. `src/sky_visibility.rs:25` bounds the focus footprint to 4.5 m inside / 12 m outside, with 10 m extra occluder reach. Distant roofs, hills and walls can therefore fail to block environment light. | A larger or hierarchical visibility representation, or actual geometry queries, with explicit coverage/error measurement. Keep a distinction between genuinely open directions and directions whose geometry was never evaluated. |
| Environment occluder completeness | `src/sky_visibility.rs:292` excludes glass, terrain (`track_recv`) and raster-only meshes. Its batch gate at line 297 excludes grass and dynamic raster FX. Terrain being a ground-radiance boundary does not make hills unable to occlude the sky. | Add source-geometry terrain/vegetation and view-local geometry participation, with alpha coverage and transparent transmission handled by material rather than treating all excluded geometry as empty. Verify against hill/roof/hull test scenes. |
| Direct Sun/Moon | `shaders/stylized_mesh.wgsl:108` treats samples outside a cascade as fully lit; `sun_shadow` at line 135 falls back from near to far, but has no visibility beyond the far map. `src/renderer.rs:420` sets the far half-width to 240 m (480 m footprint), and its light-space half-depth is 700 m. Glass is skipped by the active sun-shadow draw; grass/tiny raster FX are also omitted. | Scene-covering directional visibility or a declared hierarchical shadow solution, plus material-aware transparent coverage. Merely extending a depth range sacrifices precision and does not cover arbitrary terrain. |
| Raster glazing | `shaders/stylized_mesh.wgsl:473` returns a premultiplied environment sheen; `src/raster_glass.rs:6` leaves opaque depth unchanged. The raster path does not trace thickness, Snell refraction, absorption or internal reflection. The same panes are omitted from sun/sky occlusion, so they do not provide source-side dielectric transmission there. | A shared dielectric connection/material solver or a measured, explicitly bounded transmission model for both camera and source paths. The existing sheen is a presentation approximation. |
| Optical relays | `src/raster_optics.rs:1` declares per-objective exterior raster cameras and virtual-pupil limits. `shaders/raster_optics.wgsl:37` samples the HDR exterior image onto the real entrance face. Depth testing and source-verified cupola direction mapping prevent a simple through-wall overlay, but finite internal aperture rejection, glass/prism transport, aberration and exit parallax remain absent. `encode_raster_scene` omits optical surfaces to prevent feedback and shares the main visibility maps. | Model and trace the optical path through the actual aperture/lens/mirror surfaces, with shared source/medium transport and tested transmission. A depth-tested relay texture is not that solve. |
| World fog illumination | The current `shaders/stylized_smoke.wgsl` fog-source expression uses `phase_weighted_environment(..., 1.0, 1.0)` plus the key beam without per-medium-point geometry visibility. The new cabin partition removes exterior fog inside the source-derived compartment; it does not give exterior fog shadowed incident illumination. | Evaluate source visibility and source-to-sample extinction throughout the exterior volume, with the same geometry and materials used by surfaces. |
| Local smoke illumination | `medium_visibility` uses 32 atlas directions and an approximate cos^8 sun lobe instead of the exact beam direction. Local plume extinction is integrated, but `smoke_light` is evaluated once at the combined optical-depth centroid. Several separated or partially shadowed clouds can thus share one lighting sample. | Per-sample incident radiance and source extinction, with sufficient quadrature/convergence tests. Multiple scattering and soot self-shadowing remain separate work. |
| Surface illumination through media | The inspected `stylized_mesh.wgsl` surface terms combine environment, shadowed Sun/Moon and inverse-square point-source irradiance. Geometry visibility is not Beer–Lambert extinction through the world/local medium along each source connection. A final camera-ray fog composite cannot substitute for those source-to-surface paths. | Share the density field and segment integration across surface and volume source connections; keep source attenuation distinct from camera extinction. |

## Particle path under concurrent correction

At inspection, `shaders/stylized_particles.wgsl:115` lights snow from the physical probe's mean
sky/ground and key beam, without a per-flake visibility query or positioned-source lighting;
its dust result at line 124 is a fixed display color. Roof coverage and a cabin box suppress
some particles but are not illumination transport. The main view does receive the physical
probe via GPU copies in `src/renderer.rs:7553`; it should not be described as always using the
shader's fallback snow color.

The optic path had a separate concrete mismatch: `src/raster_optics.rs:410` initialized its
particle source radiances to zero, while `copy_eye_mapping` copied only eye response. Without
the main view's probe copy, the shared particle function chose the fixed snow-color fallback.
`raster_optic_particles.wgsl` then inverted the display curve/exposure to put that result into HDR.
This finding was sent to the agent implementing particle visibility and shared environment
bindings. Reinspect those functions after that patch; do not report this row as resolved merely
because surface point sources now cast shadows.

## Validation boundary

This audit does not replace GPU validation of the current cabin-light, source-shadow or medium
patches. Useful remaining acceptance scenes include: a receiver beyond both sun cascades behind
a canyon wall; a hill blocking the low sky; a room outside the environment footprint; point and
sun light passing through a known-thickness pane; an optic at off-axis pupil positions; fog
straddling a shadow boundary; and the same night snow viewed directly and through an optic.
These should compare shared source state and transport, not camera-dependent brightness fixes.
