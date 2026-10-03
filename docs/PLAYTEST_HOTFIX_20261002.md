# Winter playtest hotfix — 2 October 2026

This checkout is the relaunch candidate. The previous executable and assets in
`/home/alex/panzer/canyon-v2-20260930` remain intact. The broader landscape/material
work in `reference-pass-20261002` is not part of this release. The reference checklist
is still active; this document does not mark the full reference pass complete.

## Rendering and performance

- One fixed high profile: native 800 × 450, ordered Bayer8, MSAA4 geometry and
  nearest presentation in a fullscreen desktop window. Saved graphics preferences
  are ignored by this launcher, preserved on disk, and the graphics controls are
  hidden while the fixed profile is active.
- The LDR intermediate now stores sRGB display codes. This preserves valid dark
  palette steps instead of rounding them to black or conspicuous sparse coverage.
- Source/receiver shadow calculations use local coordinates to retain precision;
  moving meshes leave the static shadow cache and remain in the dynamic caster set.
  Geometry, source powers, near range and occlusion are retained.
- Bloom is narrower and weaker. A continuous display shoulder retains bright
  highlight differences. Steady bright apertures no longer repeatedly trigger the
  flash-blindness response.
- The latter exposed an older display error: globally dark-adapted rods added a
  huge neutral signal even to lit lamps. A local sensitivity bound now covers scene
  and particle display responses consistently. It does not change transported
  lighting, eye adaptation state, shadows, exposure or the native dither grid.
- Particle subdivision follows projected coverage rather than a 512-particle
  minimum for each parent. Source-area/energy accounting and fine pixel size remain.
- Boolean source occlusion stops at the first blocking triangle. Cabin/exterior
  attenuation shares one interval traversal. Real-time catch-up retains each
  physics/audio substep while rebuilding grass and fire presentation once afterward.

Performance is improved, but a heavy burning-cabin workload is still expensive.
Offscreen fenced measurements are not native fullscreen FPS. Early manual/catch-up
comparisons had different encounter seeds; their validation receipt discloses this.
The final benchmark wrapper pins and records the deployment seed.

## Audio and controls

- Loaded effects enter the live sink as floating-point samples. Previously, gain
  above unity clipped integer samples before the common limiter, so the live and
  offline gun outputs differed by about 5.4 dB.
- The own-gun source has a finite pressure/structural body with less excessive deep
  displacement. Global exposure and physical source levels are unchanged. The MG's
  repeated long comb tail is removed from its local mechanical voice; propagation
  and outdoor reverberation remain.
- MG34 cadence is 800 rounds/minute. Event scheduling preserves 75 ms separation
  during a slow presentation frame. A late frame cannot play already elapsed audio
  in the past, but no longer collapses the batch into simultaneous shots.
- Engine pitch follows RPM/load. Empty case and live-round contacts have distinct
  modal voices driven by actual contact energy; extraction/ram friction follows
  real loader movement. These are authored sounds and require listening acceptance.
- V draws/holsters the P08 in the turret; RMB aims, LMB fires, R reloads, Q holsters.
  The actual muzzle path is blocked by closed armor. ADS is 12 mm lower. Partial
  P08 reloads already worked and now have expanded chamber/magazine regressions.
- At the gunner post, arrow keys traverse/elevate the main gun; this existing control
  is now documented in the menu. It does not remotely lay the gun from every seat.
- Both direct turret side slits can be focused through their real source opening.
  The seven cupola views are registered. The right-front view's roof-cap obstruction
  is a source geometry/eye-clearance issue, not an absent render; focused raised-eye
  clearance is covered by a regression. General hatch head-out and MP40 integration
  remain separate work.
- Two finite glass covers sit ahead of the driver and TPZ instrument faces/needles.
  This uses the existing glass sheen approximation, not new refraction or a light.

## Evidence and limits

The release build, final targeted Rust tests, shader tests, audio mix receipt and
day/night images are recorded under `artifacts/runtime/` and flat `captures/`.
The listening page pairs unchanged supplied recordings with staged current-game
audio. It is not a full firefight or a loudness-matched comparison; the isolated gun
take excludes the separate recoil mechanism and actual reload cycle.

The current cabin material palette still needs the broader art pass. Narrow
geometry-occluded shafts through actual apertures are specified in
`CABIN_SLIT_LIGHT_REFERENCE_20261002.md`, not implemented by this hotfix. The existing
peak-channel tone curve also retains a known extreme saturated-blue luminance
limitation; the local rod fix is not a complete display-model redesign.

Launch with `tools/launch_fixed_playtest.sh --play`; `--check` validates the binary
and required assets without opening the game. The launcher writes an executable
hash, process ID and runtime log. A launch or diagnostic pass is not subjective
playtest acceptance. No old worktree or original reference media was deleted.
