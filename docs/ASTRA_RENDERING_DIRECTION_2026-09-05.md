# Color, dither and real-time lighting

The owner's target is Obra Dinn's readable 3D pixel treatment extended to rich full color,
continuous atmosphere and the localized warm/cool lighting of the approved Cultic reference.
The target is the image and its behavior in motion. A particular path-tracing implementation is
not an acceptance criterion by itself.

## Evidence from this build

The v4 actual GPU readback separates ten lighting/material targets without changing the renderer.
See `artifacts/astra-audit-20260905/light-layer-attribution-v4.json` and the matching
`captures/astra/light_layers_20260905_v4_*` buffers.

In the marked left region, 2,602 blue pixels derive about 79.1% of their luminance from accumulated
diffuse, 19.6% from specular and 1.4% from volume. The volume itself is warm there. The continuous
shaft lives in the separate haze layer. These blue patches are predominantly surface-light noise,
not authored dust.

At frame 46 their median diffuse history is only four frames. Most nearby specular history remains
46 frames old. An all-missed neighborhood of stochastic aperture rays was being interpreted as a
confident lighting change, repeatedly discarding useful diffuse history. The correction requires
evidence of a real source change before allowing this reset. It does not raise sample count or
paint additional blue fill. This finding alone cannot establish the final real-time budget.

The existing renderer already rasterizes primary geometry and traces secondary lighting. Calling
the next renderer "hybrid" therefore does not identify an optimization. Individual expensive
estimators, scene updates and reuse policies must change, with measured cost and image error.

In particular, current accumulation reuses finished radiance, but `combine.wgsl` still retraces
diffuse and GGX paths at every visible pixel each frame. Their secondary hits evaluate sun, local
lights and sky again. `smoke.wgsl` also reevaluates source visibility at many medium samples (22
cabin taps, with separate conditional world/plume integration). Near panes it reevaluates the
environment at each tap. More history can improve noise without removing this repeated work;
lighting caches and better sampling are separate optimizations.

The corrected v5 pre-impact proof (`captures/astra/pixel_light_20260905_v5_pre_impact_manifest.json`)
records one settled 640x360 camera frame on the 4090: smoke-estimate 59.038 ms, surface combine
15.864 ms, primary G-buffer 0.368 ms, explicit particles 0.059 ms and crush 0.005 ms. This paced,
single-frame measurement makes the cabin volume/source-evaluation work the first optimization
target. It is not a gameplay FPS estimate, nor evidence of the entire campaign's cost.

The paired v4/v5 readbacks have byte-identical current diffuse/specular samples, albedo, normals,
accumulated specular and immediate emission. In all 32,007 pixels of the marked left region,
diffuse history now reaches frame 46. In the previously identified blue pixels its median was
four frames before the fix. The total number of disconnected blue components falls from 243 to
116; pixels belonging to components of at most four pixels fall from 292 to 128.
Relative neighboring blue-channel variation on matched material/normal surfaces decreases 46.5%.
These are spatial measurements of one paired frame, not a temporal stability benchmark.
The approved shaft's volume-layer luminance changes less than 0.01% in its marked region.
See `artifacts/astra-audit-20260905/light-layer-comparison-v4-v5.json`.

The 192-frame, fixed-camera impact review averages 60.214 ms in smoke estimation and 13.938 ms
in surface combine. Particle drawing averages 0.067 ms and final dither 0.005 ms. Deliberate
capture pacing, CPU work and frame readback make video-production wall time unsuitable as FPS.

The v6 correction also makes the incandescent-fragment aggregate agree with its visible material:
sRGB-decoded stage radiance times cube face area divided by four, replacing the former multiplier
of 42. Its finite source radius now comes from actual emitting area instead of the empty space
enclosed by the entire shower. This prevents that envelope from shortening cabin shadow rays by
metres. It remains one power-weighted centroid source per compartment; individual-fragment
visibility is still approximated. The eight `hot_fragment` tests pass, including two new checks
against the actual mesh texels/triangle area and the source's occlusion segment.

## Primary technical references

- [Pope's June 2014 rendering breakdown](https://dukope.com/devlogs/obra-dinn/tig-01/) separates
  surface edges, lighting and texture before the final stylization. This supports independent
  control of lighting readability and material detail; its grayscale channel packing is not our
  color representation.
- [Pope's November 2017 dither work](https://dukope.com/devlogs/obra-dinn/tig-32/) addresses visible
  pattern swimming. His sphere mapping follows camera rotation, with a supersampled display pass
  and explicit compromises. This is a presentation technique, not a Monte Carlo denoiser.
- [Pope's October 2015 lightmapper](https://dukope.com/devlogs/obra-dinn/tig-16/) bakes direct lights
  and shadows and supports layered/state-dependent lighting. It demonstrates that the aesthetic
  is independent of real-time path tracing. Our movable tank, weather and destruction need more
  dynamic treatment than simply reusing that approach unchanged.
- [Rune Skovbo Johansen's Dither3D](https://github.com/runevision/Dither3D) is a later, separate
  implementation of surface-stable fractal patterns, including RGB examples. It is a candidate for
  a controlled motion comparison, not code already integrated here.
- [ReSTIR GI research](https://research.nvidia.com/publication/2021-06_restir-gi-path-resampling-real-time-path-tracing)
  reuses useful indirect paths across pixels and frames. Its reported low-sample results show a
  route beyond brute-force convergence; they are not a performance prediction for this game.
- [NVIDIA's NRD description](https://developer.nvidia.com/blog/nvidia-real-time-denoiser-delivers-best-in-class-denoising-in-watch-dogs-legion/)
  describes reconstruction designed for sparse ray signals. Integration into this Rust/WGSL
  renderer would require engineering and validation, not a toggle.

## Proposed real-time direction

1. Keep primary surfaces and source textures crisp. Preserve full HDR color until the display
   stage; use deliberate dither to shape selected tonal transitions without covering every surface
   in independent RGB noise. Test rotation and translation, not only a stationary image.
2. Evaluate direct sun and cabin lights explicitly with real geometry visibility. Guide sky-light
   samples toward actual openings. Apertures are sampling proposals, never extra emitters, and
   every ray must still meet the correct glass, armor, crew and hatch geometry.
3. Reuse indirect illumination through a radiance cache, probe field or validated reservoir
   resampling. Reserve expensive secondary rays for changes and materials where they matter.
   Moving occluders and destructive events need explicit invalidation; surface blur is insufficient.
4. Keep the approved volumetric shaft continuous. Evaluate/cache incident light in a bounded 3D
   grid or sparse ray-marched medium, respecting source occlusion and extinction. Dust and snow
   are separate one-pixel 3D particles and do not construct the shaft out of dots.
5. Apply stable presentation dither after lighting reconstruction, then nearest-scale once. Compare
   a pinned pattern against the current fixed screen-grid Bayer treatment before selecting it.

These are proposed algorithmic changes, not completed features. First compare the corrected current
method at the same camera and sample budget. GPU pass timestamps in Astra captures attribute cost
without including deliberate capture pauses; they exclude some scene preparation/TLAS work and
are not gameplay FPS. A live representative campaign benchmark with AI and moving cameras is
still required before claiming a 60 Hz budget or choosing the replacement transport method.

The first performance prototype should cache cabin incident lighting in a bounded 3D field while
keeping the density/extinction integral and real aperture visibility. Measure it against the same
closed/open hatch, moving camera and impact cases. The next sampling prototype should add actual
open hatch apertures to the existing glass-based sky proposals; proposals guide rays and do not
emit light. Neither prototype is implemented in this pass.

Current limitations: mature diffuse history responds slowly to a moving indirect occluder when
the light sources do not change (roughly 17 frames for half the history weight). The corrected
source-step detection is not a complete production denoiser. Many folded viewport optics still
need explicit transport geometry; no invisible blue lamps should stand in for those light paths.
