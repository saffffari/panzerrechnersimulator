# Effects source review — 2026-10-30

This review covers the existing native raymarch renderer and its effects sources. It does not
claim a completed GPU comparison or a calibrated fluid/combustion solver. There is no local
`.game-dev/adapter.json`; use the project's native, windowless evidence path below.

## Changes prepared

- A heavy `BLAST` record remains a detonation regardless of distance from the player's muzzle.
  Previously the light, media, and retained-event paths changed its meaning within 2.5 m of that
  point. The normal muzzle source is separate: `gpu_burst_record` does not emit `BLAST` for a
  muzzle discharge. Source classification now shares one CPU function.
- The same detonation uses the existing atlas path's 60 ms exponential cooling envelope in the
  point-light and fallback-parcel paths too. Below one percent it is cooled material, not a hot
  fireball; the fallback switches to ordinary smoke so it does not continue adding heat distortion.
  This is an authored presentation envelope, not a measured radiometric energy budget.
- Removed the 48 stationary glowing points planted around every ground explosion for up to 24 s.
  That event had neither a fuel source nor a support/contact budget. Real surface-supported fire
  and its own residue remain independent. Projectile, fragment, injury, armor, and ground-ejecta
  calculations are unchanged; no particle-count increase was made.
- Ground blast dust carries the colocated ground-ejecta record's snow fraction into its retained
  event. Soil now remains soil-colored in the pressure ring and fallback dust column; snow retains
  its pale scattering tint. This changes neither the dust quantities nor trajectories. Source
  association uses the nearest active ground-ejecta record within the existing 2.5 m bound.

The short high-intensity core, debris, cooling ejecta, and drifting dust still have separate
lifetimes. These changes do not make every impact into a uniform large fireball.

## Existing strengths and remaining limits

- Ground ejecta receives projectile mass/velocity and a bounded kinetic-energy share.
- Heated armor rims receive only a bounded share of actual lost kinetic energy.
- Persistent cabin fire and blood film use retained source-triangle contact/support queries.
- Blood film uses 5 mm cells and merges contiguous cells into render rows. Limits are 32,768
  airborne grains and 262,144 film cells. Film updates are bounded and render rows are cached
  until deposition changes; however, changing deposition sorts all retained cells again. Profile
  `drive_cubic_blood` before attempting a structural optimization.
- Airborne blood's rendered size is currently camera-dependent, enlarged from 4 mm to at most
  18 mm for readability. Collision and deposition retain the 4 mm source grain. This is a visible
  approximation to revisit separately, not a reason to increase injury or fluid quantities.
- The retained explosion sequel still lacks a shell-specific filler/radiant-energy budget and
  uses an authored 88 mm-class family. Legacy flash-mesh and muzzle-light inference also remain
  proximity-based. New source identities should replace those heuristics before treating the
  entire effects pipeline as physically calibrated.
- Muzzle dust still assumes snow color and a fixed vertical offset. A future warm biome needs
  actual terrain material/height supplied at discharge; changing the cloud's tint alone would not
  solve airborne discharges or raised ground.
- Explicitly sourced detached parts, fragments, source-supported residue, and persistent damage
  provide more useful detail than globally increasing spark or blood counts.

## CPU verification targets

Parent owns the shared Cargo build/test lock. Run the `event_source_tests` tests under
`renderer::raymarch`, plus the ordinary raymarch shader validation target. Tests cover detonation
identity on both sides of the old proximity threshold, non-explosive records retaining their
identity, finite/monotonic cooling, no sustained flash during the dust sequel, and soil/snow tint.

## Deterministic baseline/candidate capture recipe

Preserve a verified baseline executable **before** rebuilding the candidate. Use identical assets,
working directory, graphics settings, environment, and shot timing; one owned GPU process at a
time. Keep all outputs flat under `captures/`, with distinct `_baseline_` and `_candidate_` prefixes.

Set the following for both executions:

```powershell
$env:KESSEL_RENDERER = 'raymarch'
$env:KESSEL_AUDIO_CAPTURE = '1'
$env:KESSEL_ACT = '1'
$env:KESSEL_BENCH_IMPACT = '5.85,0,100000'
$env:KESSEL_BENCH_RECEIPTS = '1'
```

Use the same optional manifest and display environment for both; leave `KESSEL_BENCH_STATION`
unset for the normal fixed-radius observer orbit. The authored neutral Tiger gun reaches about
5.85 m forward of its hull origin and about 2.37 m above it. This places the staged ground strike
under its neutral muzzle, within the former proximity threshold on flat ground. Check the actual
scene pose/ground before claiming that threshold was exercised.

For a chosen baseline or candidate executable, run separate fresh processes:

```powershell
& $reviewExe gfxbench tag 6   captures/effects_20261030_candidate_010s.png high
& $reviewExe gfxbench tag 18  captures/effects_20261030_candidate_030s.png high
& $reviewExe gfxbench tag 60  captures/effects_20261030_candidate_100s.png high
& $reviewExe gfxbench tag 360 captures/effects_20261030_candidate_600s.png high
```

Replace the label for the baseline; retain stdout/stderr and executable hash alongside each
capture. Compare each equal-frame-count pair, not the camera positions between different ages.
Repeat the 18-frame pair with `KESSEL_BENCH_IMPACT='12,0,100000'` to separate the cooling-law
change from the old muzzle-proximity misclassification.

This is a **staged effects fixture**, not a live incoming shot: it submits standard blast and
ground-ejecta sources with the 76.2 mm HE fixture's mass and velocity, but creates no projectile,
armor/injury resolution, impact sound, or gameplay outcome. Do not present it as end-to-end
ballistics evidence. The final PNG is native renderer output without grading. Native graphics
timings compare renderer cost only; six timed frames are too few for a performance verdict.

Inspect: intact flash localization at 0.1 s; cooled dust rather than a broad glowing fireball at
0.3/1 s; no unsupported stationary ember ring at 6 s; unchanged source-supported sparks/ejecta.
The test should not expect pixels from an installed VDB atlas to match an atlas-free run. To test
fallback, use a separate documented resource bundle without that optional atlas—do not move or
delete the user's runtime assets to force the case.

## Status

Source review and bounded regression tests are prepared. GPU comparison and performance
acceptance are pending parent execution; neither is inferred from source inspection.
