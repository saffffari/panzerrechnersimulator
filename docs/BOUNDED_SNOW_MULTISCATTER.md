# Bounded blowing-snow higher-order transport

Status: **live on the hardware ray-query High/Vulkan volume path**. Laptop retains the identical
Beer–Lambert extinction and first-order source integral, but contributes zero higher orders until
its exact software-triangle visibility backend exists.

## Estimator

`shaders/smoke.wgsl::bounded_world_higher_order_scatter` launches one temporally scrambled path per
active volume pixel. It is separate from the existing order-one camera quadrature:

1. Sample the first camera collision from the homogeneous free-flight density
   `p(t) = sigma_t exp(-sigma_t t)` inside the exact horizontal-slab interval.
2. Sample a sourceward direction from the normalized HG phase function. The phase value cancels its
   sampling PDF; the neutral effective-medium albedo remains in throughput.
3. Sample each subsequent free flight from the same extinction coefficient and terminate at the
   exact slab boundary or an opaque TLAS blocker.
4. At collisions two through five, estimate direct incident radiance with next-event samples of the
   finite solar disk, the physical environment, and the shared positioned-emitter table.
5. Apply Russian roulette after the early orders. Survival weighting makes roulette unbiased with
   respect to the declared five-order cap.

The finite cap is a numerical approximation. It is not a two-stream brightness lookup, ambient
term, tint, or view-dependent fill. Four kilometres bounds a nearly horizontal continuation; at
Act I's `sigma_t = 0.0068 m^-1`, the probability of travelling that far without another collision
is below `1.6e-12`. Vertical paths use their nearer physical slab exit.

The uncollided camera term is not estimated by this stochastic path. Volume alpha is always the
analytic Beer–Lambert result

```text
alpha = 1 - exp(-sigma_t * slab_segment_length)
```

and the spatiotemporal volume filter continues to filter effective source radiance only.

## Source separation

The continuation environment NEE reads only
`physical_environment_transport(...).inscattered_radiance`. The finite solar disk has its own
single NEE term, so direct solar radiance is not duplicated. The environment field may include the
physical below-cloud delta-Eddington diffuse boundary; this is transported incident radiance from
the shared atmosphere solution, not an authored snow colour. Snow albedo is scalar and neutral, so
any blue comes from that real incident spectrum while fire and tracers retain their emitted spectra.

## Deliberate boundary

The smoke pipeline's TLAS binding returns binary segment visibility, not committed triangle
material data. A continuation that reaches opaque geometry therefore terminates. The estimator does
not yet add reflected or emitted surface radiance from that hit. This underestimates ground/steel
coupling but cannot double-count it or manufacture an unshadowed boundary. Completing that term
requires the exact-hit material payload used by the High surface integrator to be exposed to the
volume pipeline.

The orders-two-plus path is also still composed separately from compact cabin/plume media and the
resolved large snow aggregates. Unifying event ordering remains transport debt.

## Gates

`tests/snow_multiscatter_contract.rs` checks:

- HG inverse sampling has the declared mean cosine;
- sampled first-collision probability matches exact slab opacity;
- the path contains HG continuation, NEE, opaque TLAS termination, albedo throughput, roulette and
  survival reweighting;
- higher orders are added once after order one;
- environment NEE excludes direct solar radiance;
- world opacity is analytic Beer–Lambert; and
- no snow tint, whiteout colour, brightness floor, camera fill or ambient floor enters the shader.

`tests/physical_environment.rs::live_environment_shader_compositions_parse_and_validate` composes
and validates both portable and hardware-ray-query versions with the shared environment library.
