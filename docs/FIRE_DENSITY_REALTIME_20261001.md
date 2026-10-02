# Denser native fire and elapsed-time source updates

OLD's sustained flame registrations request eight times as many native-pixel samples inside each
existing cosmetic parcel. This is quadrature refinement, not eight times the fuel, emitted light
or plume size. The same stable parcel ID and sample-index prefix define positions. Every grain
still covers exactly one native pixel; depth, medium attenuation and HDR bloom remain shared.

The shader weight remains projected area divided by actual sample count. Eight times the samples
therefore gives one eighth the per-sample weight. Aggregate pre-occlusion radiance quadrature and
modeled optical depth stay normalized. Actual pixel distribution can change, and better sampling
of visibility is expected. This does not promise bit-identical images. MAIN does not yet have this
native-grain renderer; the density change is confined to OLD rather than silently changing MAIN
mesh energy or adding a second renderer.

Necessary coverage retains the existing weight <= 0.5 requirement. Extra density is optional
when the 8,388,608-grain per-view budget cannot fit it. `density_reduced_parents` exposes that
reduction; insufficient necessary coverage continues to mark saturation and omitted energy.
Accepted captures must have no saturation. Receipts now separate `fire_grains` and `soot_grains`
and report the maximum selected density multiplier. Weapons and smoke keep their existing sample
density unless their registration explicitly requests otherwise.

Both checkouts' `CabinFire::update_fire` and `advance_foam` formerly clamped dt to 0.05 seconds,
losing elapsed simulation time during long frames. They now retain a 120 Hz accumulator and
consume all elapsed ticks, leaving less than 8.34 ms as residual. Spread and ammunition-scan
intervals retain their remainder. Pausing with zero dt does not age foam. Current source poses
are sampled for the update, as before; this is not continuous reconstruction of moving surfaces
between host frames. The authored coverage/heat system is not a calibrated combustion solver.

CPU regressions compare 30, 60 and 144 Hz updates, plus a single 0.5-second hitch, using the same
source geometry, emitted foam and source fire. Density tests verify the exact eightfold request,
normalized area/radiance and optical-depth sums, unchanged one-native-pixel shader extent, and
the existing hard-budget behavior. Native appearance/performance review is required after build.
