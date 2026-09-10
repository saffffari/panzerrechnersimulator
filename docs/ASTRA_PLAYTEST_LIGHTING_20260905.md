# Playtest daylight source mismatch

The v5 garage, grove and refueling films passed their mechanical checks but failed
visual coverage: the ground and objects were nearly black against a white sky.
Their rejected originals remain in the first-person library. Source investigation
found a concrete difference between the sky seen by the camera and the environment
sample used to illuminate surfaces.

`world_medium_environment_source` previously projected its atmospheric query to
the upper boundary of the independent ground aerosol layer, world Y=256 m. The
atmospheric planet datum is world Y=-256 m, so the projected point has altitude
about 512 m. Act II's low cloud begins at altitude 430 m, or world Y=174 m.

The actual grove camera started at (200, 2.5024347, 150), well below that cloud.
The projected surface-lighting query started **inside** it. The existing
`physical_cloud_diffuse_boundary` correctly restricts its lower-boundary flux
closure to below-cloud receivers, so the projection disabled that term only for
surface lighting. The real camera still saw the bright multiple-scattered cloud
boundary. The arena's cloud starts at altitude 900 m and did not cross this branch.

Both captures use the ordinary daytime phase and the same solar direction; this
was not an accidental night flag. Terrain heights also remain safely above the
numerical planet surface. Authored terrain triangles have upward winding/normals.

The bounded correction evaluates the physical atmosphere at the actual source
connection and applies the independent suspension's directional extinction and
scattered-flux transfer once there. It retains one inlined atmosphere evaluation,
the original geometry-ray offsets/visibility, refracted exits, source spectrum,
cloud and aerosol geometry, source albedo, and exposure. No cloud/overlap branch or
brightness multiplier was added.

This is a composition approximation for independently represented overlapping
media. It does not solve two-way flux exchange between coupled cloud and mist
layers, and it does not change the atmosphere solver's existing first-order
inside-cloud treatment. A partial-cloud multiple-scattering solver is a separate
task; shrinking the cloud column to a convenient receiver height would omit
coupling from the lower cloud.

Focused validation in `tests/physical_environment.rs` covers the actual Act II
camera/projected-origin domain mismatch, below/inside/above-cloud source domains,
continuity as the separate aerosol support crosses cloud base, the exact
absorber-once limit, and composed WGSL parsing. The root agent runs these checks
and the same-camera GPU proof; a source change is not a visual approval.

The source materials are independently dark. Exact PNG samples in
`artifacts/astra-audit-20260905/playtest-v5-ground-material-audit.json` show median
linear luminance 0.0264 for the new Pizza ground drape. Its sRGB mean (46, 49, 27)
closely follows the original grass tile; the authoring pipeline did not replace
its hue or apply an extra gamma decode. Those material choices may need art
calibration later, but they cannot justify the missing cloud source. This fix
does not alter them.
