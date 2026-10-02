# Audio and track snow — 2026-10-01

## Audio

Rifle and MG reports now use separate dry families with twelve deterministic variants each. The owned rifle input remains the hash-verified `rifle_shot` entry in `catalog/audio_feedback_20260905.json` (SHA256 `8f537a47c60ea37b5a8c8d726cc4e0520b7d48424173049e9b39a5da81958d9d`). Its source file is unchanged. The new preparation removes leading silence and retains at most 110 ms of early rifle sound or 60 ms for the MG family, mixed with a short pressure/modal source. Dry outputs are 145 ms and 90 ms. A crop can retain early reflections; it is not de-reverberation. Procedural fallbacks remain available without private assets.

Generic cannon-report, ground/armor impact, penetration, kill, turret separation and cook-off voices cycle four synthesis seeds. Variant zero retains the previous dry waveform. Source onset is not time-stretched. Per-variant checksums key the calibration cache and appear in capture metadata. These changes do not raise the physical source levels or add a second room tail.

Each actual own-MG launch now owns its exterior report; the old fixed-rate sustained loop is disabled in gameplay. The exterior report traverses the ordinary obstruction, hull and headset path. A separate 78 ms bolt/feed/return cycle runs once per actual round for a listener inside the cabin. Its level converts the existing `MG34_INSIDE` 120 dBA burst reference at 13 rounds/s into a per-round LAF, then applies the headset once. This interior reference is an authored combined mount/receiver estimate with roughly ±10 dB uncertainty, not an isolated receiver measurement. No fictional exact receiver node is claimed.

Small arms retain the real 240 Hz projectile sweep. Passing supersonic bullets produce a single Mach-cone N-wave from that path; they do not use the cannon shell whistle. Actual steel, soil and soft-body contacts have distinct sources. The 65 ms soft-body contact has an explicitly unsourced 109 dB peak / 83 dBA at 1 m reference. Root's scheduling changes retain actual source positions and test expanding acoustic wavefronts against the current receiver; the former universal 1.2 m source displacement/cover escape is removed.

The current coax muzzle locator remains an authored mount/emitter offset in `Tank::coax_muzzle`, rather than a measured source-mesh port. Its exact source-locator audit remains open. The new receiver sound does not pretend to resolve that limitation or alter the already-tested projectile dispersion.

## Track snow

`track_snow.rs` loads geometry only from `assets/tanks/tiger/baked/tiger_hull.glb`. It selects the 386 exact `Hull_*` running-track nodes from the reviewed `TRACK_STEEL` identities, excluding ten turret spare shoes. It samples actual source triangles; it does not coat the hull top, wheels or a proxy bounding box.

Actual left/right track contact motion controls snow pickup and shedding. Left is local +X and right is local −X. Broken belts receive zero belt speed. Ground contact uses the real terrain sample; airborne or non-contacting tracks do not acquire ground snow. Weather contacts test a bounded number of upwind-to-source rays against scene/source geometry, so covered faces receive no precipitation mark. Successful contacts retain a small snow mark and emit a short rebound fleck.

Retained patches follow the source hull geometry; loose flecks use age-based drag, wind and gravity. The simulation consumes elapsed time at 120 Hz and resets between matches while retaining immutable source samples. The pools are bounded to 4,096 retained patches and 512 loose flecks. Neutral matte, non-emissive snow receives ordinary scene lighting. Display size is approximately one native pixel nearby, capped to centimetre-scale clumps at distance. Receipt fields include selected source counts, side snow fractions, retained weather grains and emitted/live flecks.

This is cosmetic material transport on a rigid authored belt, not animated belt circulation, deforming links or a granular snow solver. Terrain compression is a separate root-owned change. Track-only weather contacts do not claim full-vehicle snow coating.

## Validation boundary

Before the final integrated checkpoint, dry-source/variation tests passed 4/4; calibration, routing and single-reverb checks passed 6/6; physical-level checks passed 9/9; propagation passed 9/9; listener/Mach-cone checks passed 5/5. The soft-body high-band correction, receiver cycle and three track-snow tests are included in the final integrated build and require its results. Source preparation and CPU checks are not subjective sound or visual acceptance. No new capture or interactive relaunch is claimed by this document.
