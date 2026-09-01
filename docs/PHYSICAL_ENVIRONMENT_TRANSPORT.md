# Physical atmosphere and snow transport foundation

Status: **atmosphere/environment transport, delta-Eddington liquid-cloud multiple scattering, a
bounded bulk blowing-snow medium with High/Vulkan orders 2–5, and High's persistent resolved-grain
transport are live and tested. Their joint event ordering is not yet one unified march.**

The renderer composes the binding-free transport library directly into the visible-sky, High
surface, volume, and resolved-snow shaders. The old authored sky and legacy white particle/shell/
ground-ribbon presentation are gone.

## What exists

- `src/physical_environment.rs` is the f64 CPU reference, physical parameter authority, and exact
  GPU uniform ABI.
- `shaders/physical_environment.wgsl` is a binding-free WGSL transport library. It parses and
  validates through the same Naga version re-exported by wgpu.
- `tests/physical_environment.rs` validates the CPU/GPU reference, the uniform ABI, and every live
  composed WGSL entry, including the High smoke ray-query variant.

There is one atmosphere query:

```text
physical_environment_transport(environment, origin, direction, maximum_distance)
    -> { inscattered_radiance, direct_solar_radiance, transmittance, integrated_distance }
```

`maximum_distance = PHYSICAL_ENV_INFINITY_M` exposes the physical space boundary and finite solar
disk. A finite distance integrates only the participating medium up to that endpoint. Therefore:

- a camera miss calls the function to infinity;
- a surface path calls the same function when that path misses geometry;
- a volume integrator calls the same function with its finite segment length;
- a camera-to-surface segment composes `L = L_medium + T_medium * L_surface`.

No camera mode, interior flag, campaign mood, or graphics preset is an input to the laws of this
function.

## Declared physical model

The local game datum is embedded in a spherical Earth with radius 6,360 km and a 100 km atmosphere.
The physical planet surface is declared at local `y = -256 m`, below all current campaign terrain
(Act I reaches approximately `-44 m`), so a valley camera cannot be misclassified as inside Earth.
The renderer otherwise works in local metres and no simulation system needs giant coordinates.

| Quantity | Declared value |
| --- | ---: |
| Rayleigh scale height | 8,000 m |
| Aerosol scale height | 1,200 m |
| Rayleigh scattering, RGB sample bands | 5.802, 13.558, 33.100 × 10^-6 m^-1 |
| Aerosol scattering / extinction | 3.996 / 4.440 × 10^-6 m^-1 |
| Aerosol HG asymmetry | 0.80 |
| Ozone center / half-width | 25 km / 15 km |
| Ozone absorption, RGB sample bands | 0.650, 1.881, 0.085 × 10^-6 m^-1 |
| Solar apparent radius | 0.266 degrees |
| Solar irradiance, RGB sample bands | 1.474, 1.8504, 1.91198 |

Rayleigh and Henyey-Greenstein phase functions are normalized over 4π and numerically regression
tested. View and source extinction are Beer-Lambert integrals through the same spherical densities.
The Sun is a hard, finite angular emitter. There is no circumsolar power curve; any halo must emerge
from forward scattering in the atmosphere or clouds.

### Overcast

Cloud is liquid water in a spherical altitude layer, not a color branch. Its extinction is derived
from liquid-water content and effective droplet radius:

```text
beta_ext = 3 * Q_ext * LWC / (4 * rho_water * r_effective)
```

Default droplet declarations are 0.25 g m^-3 liquid water, 10 µm effective radius, `Q_ext = 2`,
single-scattering albedo 0.999, and HG asymmetry 0.85. Coverage changes world-space density
occupancy. It never mixes the result toward gray or white. Every cloud source query includes
transmittance from the finite solar disk to the scattering point.

For a receiver below the cloud, the solver measures the structured vertical liquid-water optical
depth at the actual cloud-base crossing and applies the Joseph–Wiscombe–Weinman delta-Eddington
`f=g²` transform. A direct-beam two-stream solve returns total transmitted flux; true uncollided
Beer-Lambert solar flux stays separate, so their difference is cloud-scattered diffuse flux. A
second diffuse-input solve transports the spectrally resolved Rayleigh/Mie/ozone skylight arriving
at cloud top. The lower-boundary radiance uses the hemispheric closure `L = F_down / pi`; it is
attenuated from the physical boundary to the receiver. On that handled ray, directional cloud
single scattering is omitted to prevent counting the same cloud event twice. Rayleigh and aerosol
single scattering remain.

This is a plane-parallel two-stream flux approximation applied to a measured 3D density column,
not exact 3D multistream cloud radiance. It preserves energy and the physical RGB source spectrum,
but cannot reproduce side leakage or anisotropic detail below broken cloud. Sources: Joseph,
Wiscombe & Weinman (1976), DOI
`10.1175/1520-0469(1976)033<2452:TDEAFR>2.0.CO;2`; Meador & Weaver (1980), DOI
`10.1175/1520-0469(1980)037<0630:TSATRT>2.0.CO;2`.

### Act-I blowing-snow suspension (live)

The Act-I whiteout keeps the campaign's `sigma_t = 0.0068 m^-1`, so horizontal transmission is
still Beer–Lambert: about 25% at 205 m and 13% at 300 m. The change for a committed winter storm is
vertical support and optical material, not exposure or a painted horizon:

| Effective bulk quantity | Generic aerosol | Blowing-snow endpoint |
| --- | ---: | ---: |
| Layer support | -128..256 m | -128..128 m |
| Single-scattering albedo | 0.88 | 0.995 |
| HG asymmetry | 0.42 | 0.76 |

`precipitation * storm` is the effective snow fraction used to interpolate albedo and phase. Its
smooth support transition reaches the 128 m ceiling for the Act-I value of 0.98. That ceiling is
above the highest current Act-I terrain (~84 m), but removes 128 m of dense medium above the
battlefield. Real finite-disk sunlight and environment radiance therefore reach scattering points
with substantially greater source transmittance while horizontal low visibility remains unchanged.

The snow albedo is a neutral scalar. No blue RGB tint is authored. Its faint cold cast comes from
the shared physical environment's blue-sky radiance; fire, lamps, muzzle flashes and the Sun retain
their own spectra through the same source/visibility/transmittance calculation. There is no white
sky replacement, ambient floor, fixed exposure, camera veil or post-process brightness lift.

Surface endpoints now solve the missing source-side snow transport with the same delta-Eddington
flux closure used for the measured cloud column. The receiver's actual vertical optical depth is
`tau_v = sigma_t * (top_y - max(receiver_y, bottom_y))`. At the Act-I validation camera height
`y = -10.3 m`, `tau_v = 0.94044`. The exact uncollided fraction of an isotropic diffuse flux is

```text
T0,diffuse = 2 integral_0^1 mu exp(-tau_v / mu) dmu = 2 E3(tau_v) = 0.23763
```

while the live neutral-snow delta-Eddington response transmits about 0.849. The renderer keeps the
directional boundary radiance times its exact Beer transmittance, then adds only the approximately
0.611 scattered-flux remainder as hemispherically reconstructed radiance. The `2 E3` integral uses
an unrolled four-point Gauss-Legendre rule shared with the f64 CPU reference; it adds no shader loop.

The sampled finite solar disk remains a separate directional source. Its uncollided fraction is
the true `exp(-tau_v / mu0)` beam from the delta-Eddington result; only
`T_total - T_direct` is converted from horizontal transmitted flux to diffuse radiance with
`L = F_down / pi`. `environment_incident` excludes the solar disk, so this split cannot count the
same solar energy in both estimators. Vacuum and purely absorbing limits add zero scattered flux.
All closure coefficients are scalar: the RGB spectrum still comes exclusively from the physical
atmosphere/cloud boundary or the attenuated solar source.

This is an effective homogeneous particulate model, not the resolved-flake population. High/Vulkan
adds one temporally accumulated sourceward path per pixel: exact exponential camera/free-flight
sampling, normalized HG continuation, next-event estimation, TLAS segment termination and Russian
roulette resolve scattering orders 2–5 without changing analytic Beer–Lambert opacity. The source
estimator reads environment `inscattered_radiance` while estimating the finite solar disk
separately, so the physical cloud diffuse boundary and direct Sun are not counted twice. See
`docs/BOUNDED_SNOW_MULTISCATTER.md`.

The deterministic surface closure is plane-parallel and hemispheric; it does not recover the full
3D angular field, side leakage, ground return, or obstacle coupling inside the snow column. The
atmosphere and bounded snow are also factored rather than solved as one coupled multistream medium.
Volume orders above five and ground/surface-reflected incident radiance remain absent, so the result
can still be too directional or too dark in cases dominated by those paths. The volume TLAS binding
is binary visibility only: an opaque continuation hit terminates instead of evaluating its
material. Those omissions must be solved in transport; they are not permission to tint or lift the
image.

## Persistent individual-crystal snow field

Snow particles are Lagrangian world objects:

```text
identity = hash(world_cell_xyz, lane, world_seed)
reference_position = (world_cell + hashed_offset) * cell_size
position(time) = reference_position + physical_velocity * simulation_time
```

Camera position and orientation are absent from generation. The camera only selects rays through
the field, in exactly the same way it selects scene triangles. Revisiting a cell at the same
simulation time returns the same particle identity and position.

The CPU reference default remains four crystal lanes per 0.25 m cell. High's live large-aggregate
population uses four lanes per 0.5 m cell (32 aggregates m^-3), a 2.5 mm effective radius and
`Q_ext = 2`, giving 0.00126 m^-1 declared extinction (about 796 m mean free path) before the
separate fine-grain suspension. Density is therefore traceable to particle geometry instead of a
whiteout opacity slider.

Each ray query uses:

- a physical ray cone derived from the path/pixel footprint;
- the crystal cross-section `Q_ext πr²`;
- a finite shutter interval, represented as a swept capsule perpendicular to the ray;
- energy-preserving division by the swept filter area;
- Beer-Lambert extinction;
- ice single-scattering albedo and normalized HG response applied to actual incident environment
  radiance.

There is no constant-white flake color, eye-relative shell, depth-reconstructed spindrift ribbon,
or whiteout veil. The WGSL helper returns HDR radiance and spectral transmittance; the entry derives
its premultiplied compositing alpha only as `1 - T`.

The reference DDA checks a 3×3×3 neighborhood around each traversed Lagrangian cell. It reports
`complete = 0` if the ray-cone/shutter support exceeds one cell or if its declared step budget ends
before the ray. Shipping code must treat that as a failed query; it may not display the incomplete
result.

## Renderer integration seams

The live renderer now:

1. Owns one shared 160-byte `GpuPhysicalEnvironment` uniform and composes
   `physical_environment.wgsl` ahead of every consuming shader. WGSL has no native include, so no
   transport implementation is copied into an entry shader.
2. Uses `physical_environment_transport(..., INFINITY)` for the visible sky and all High
   surface-ray misses. Its explicit direct-light sample uses the declared finite solar disk and
   `physical_solar_transmittance`.
3. Uses the same environment as incident radiance in the world/cabin/plume volume march. The
   medium pass is premultiplied front-to-back, so final composition is
   `L = L_medium + T_medium * L_surface`; the Sun is evaluated separately and not double-counted in
   the diffuse environment estimator.
4. Keeps the world suspension as a disjoint bounded near-ground medium rather than duplicating the
   atmosphere coefficients. A winter storm changes its declared bulk albedo, phase and vertical
   support while preserving authored `sigma_t`; it receives neutral scattering from the real
   physical environment and positioned emitters instead of replacing the sky with a fog color.
   High surface bounces now apply the same bounded medium to finite-disk solar, directional
   environment and positioned-source rays before shading, and to the uncollided radiance carried
   between primary and secondary surface events. At solar/environment surface endpoints, a
   constant-cost delta-Eddington slab solve transports the missing scattered flux using live
   `sigma_t`, slab boundaries, albedo and asymmetry. Exact directional Beer transmission is retained
   and subtracted before the hemispheric remainder is added, while the finite solar disk remains a
   separate shadow ray. This closes the previous asymmetric path where the camera ray crossed
   blowing snow but illumination arriving at the visible snow surface carried only uncollided
   energy. Positioned emitters and radiance carried between two material hits still have only their
   exact uncollided snow segment; their scattered coupling remains declared debt.
5. Resolves High's large aggregates to 32 m with a neighboring-primary-ray cone, 1/48 s finite
   shutter, scene-depth endpoint, immutable world identities and exact triangle visibility for the
   ballistic upstream path, finite Sun and two environment directions. Their neutral 0.995 albedo
   can acquire a faint blue hue only by scattering the shared environment. The result enters HDR
   before bloom and measured exposure. A failed DDA/support bound contributes nothing.
6. Does not submit a resolved-grain pass on Laptop until exact software-triangle visibility exists;
   the physically transported bulk suspension remains, and no sprite/ribbon fallback is allowed.

Remaining renderer work is:

7. Merge persistent snow events by distance into the unified fine-grain volume march. The current
   resolved pass preserves each aggregate's extinction/scattering but composes its population as a
   separate front-to-back layer after the bulk estimate; co-located event ordering is therefore
   explicit P0 transport debt, not a claim of an exact coupled solution.
8. Propagate ray cones/differentials for secondary paths and derive shutter duration from the real
   camera exposure interval instead of the current declared 1/48 s capture shutter.
9. Verify a clear day, low winter Sun, looking directly at the solar limb, interior aperture,
   moving exterior camera, stopped camera, and a repeated world route on the 4090. A stable world
   cell must show the same particle sequence on revisit. Live cloud coverage comes from the
   campaign's `overcast` state in both presets; High/Laptop differ only in measured-column
   quadrature count (12/8 samples).

### Correct volume ordering (remaining P0)

Atmospheric and snow events lie on the same physical ray. The production volume loop must sort or
march them front-to-back and update one throughput. Computing a complete atmosphere image and then
alpha-blending snow over it is not an exact coupled solution. High's live resolved pass is a
bounded transition implementation whose separate ordering is recorded above; the common loop must
eventually query medium coefficients, source transmittance and snow events without duplicating laws.

## Cost and acceleration plan

The direct atmosphere reference cost is explicit:

```text
view density evaluations
  + view_steps × solar_disk_samples × source_steps source-density evaluations
```

- Reference (32 × 4 × 16): 2,080 medium evaluations per environment ray.
- Interactive high (20 × 2 × 10): 420 medium evaluations per environment ray.
- Each cloud-density evaluation currently includes two trilinear world-noise samples.
- A below-cloud ray additionally measures one 16/12/8-sample structured cloud column for
  Reference/High/Laptop and evaluates constant-cost direct/diffuse delta-Eddington flux closures.

This direct form establishes the numerical truth and is suitable for validation and sparse
secondary queries. Production acceleration may cache clear-atmosphere optical depth in
altitude/zenith coordinates and importance-sample the finite solar disk. Such a cache must be
generated from these coefficients, validated against this f64 reference, and leave the
world-varying cloud extinction in the live path. It is numerical acceleration, not a painted sky.

The literal snow reference traversal costs roughly:

```text
(ray_length / 0.25 m) × 27 neighboring cells × 4 lanes
```

A 100 m ray therefore tests about 43,200 deterministic candidates before scattering queries. That
is intentionally honest and not a viable full-screen shipping budget. The integration path should
generate/hash visible world chunks into a GPU particle buffer, build/refit a particle BVH, and ray
query the same physical crystals. Frustum culling may decide which world chunks are resident; it
must never change their identities or move generation into camera shells. Low/high presets can
alter rays per pixel and BVH update cadence, not the particle field.

## Tests and unresolved scope

Run:

```sh
cargo test --test physical_environment
cargo test --test snow_multiscatter_contract
```

The suite currently covers finite/nonnegative radiance, [0,1] transmittance, monotone
Beer-Lambert attenuation, numerical phase normalization and forward behavior, finite solar-disk
extent, cloud extinction/scattering, deterministic snow identity, exact linear advection,
camera-independent generation, finite-shutter energy behavior, uniform ABI size/alignment, WGSL
parse/validation (including every live composed entry and High smoke ray query), a nonblack clear
external sky from the Act-I valley datum, altitude-boundary cloud capture at the minimum requested
quadrature, and forbidden-shortcut source contracts.

Still unresolved and therefore not claimed: coupled atmosphere/snow marching, a full angular/3D
solution for the surface-endpoint snow flux, source-scattered positioned emitters and inter-material
segments, Rayleigh/aerosol multiple scattering, bounded-snow volume orders above five, material
response at a snow continuation's opaque hit, exact 3D/multistream cloud transport, ground/surface
contribution to sky irradiance, spectral transport beyond three sample bands, polarization,
nonspherical ice phase data, turbulent world-space wind, particle BVH construction, temporal
convergence, and measured 4090/laptop cost.

The High renderer remains raster-primary with exact triangle ray queries for one-bounce lighting.
This integration does not turn it into, and must not be described as, a full path tracer.
