# Loose-track audio and winter mechanical review — 2026-10-01

A broken-track flag alone does not start a loop. `Powertrain::broken_track_drive` exposes each
side's actual gear/clutch/RPM-derived **attempted** belt motion and delivered/drag load. One broken
side retains the intact side's constrained mobility. Both broken sides retain shaft attempts but
produce zero ground traction/distance/yaw; the no-ground-contact engine load includes an explicit
2% throttle loose-running-gear drag estimate. Brake, engine/transmission failure, neutral/shift,
no throttle, or no delivered effort suppress the source.

The loose-link contact loop is positioned at the modeled side-specific repair/link point,
propagated through the normal distance, occlusion, enclosure/headset and reproduction path.
Modal ringing at183/421/997/2311Hz supplies distinct short metal contacts rather than a continuous
noise bed. Playback cadence follows attempted belt speed.103dBA at1m/reference effort, modal
weights and drag are authored estimates, not Tiger measurements. Existing calibrated machinery
voice smoothing and short gain release prevent abrupt loop edges. Source energy is not replaced
by a camera-dependent gain. The normal balanced reproduction curve remains separate.

Hermes's18:59:59PDT CPU executable passed the two source/contact tests, positioned loop lifecycle
test and mechanical controller test. Actual live-graph WAVs remain required for perceptual review.

`winter-drive-review PREFIX uphill|manual|broken [preview]` produces three separate eight-second
fixtures on the canonical winter terrain at native800×450 screen Bayer8, with5fps preview or30fps
video and the ordinary audio graph. It records60Hz powertrain/track/gun telemetry, fixed setup,
manual shift events, stills/contact sheet, and explicit observation checks. Mechanical fixtures
disable incoming fire and request no shots; they do not use invulnerability. The uphill case
starts rolling at5m/s in sixth gear to inspect automatic downshift/load on a source-terrain
ascent. Its terrain selection is geometry-only, never outcome selection. The broken case
contains drive/release/restart and an ordinary ignition shutoff. These are mechanical diagnostics,
not combat evidence, historical performance validation, or a solved loose-chain simulation.

Read-only device diagnosis: the closed live game's log contained273 ALSA underruns. Rodio0.20.1
pulls voices directly from CPAL's output callback; the app has no pre-rendered ring and selects
the default output stream. CPAL0.15.3's ALSA default requests25ms periods/100ms buffer, but the
actual negotiated values/callback timings were not recorded. The log has no event timestamps
that establish a causal link to any key edge or compiler activity. Existing gain/rate smoothing
addresses signal discontinuities; it does not prove starvation fixed. No driver configuration,
buffering policy, priority or device settings were changed. Offline WAV capture cannot validate
ALSA scheduling. A future bounded diagnosis should measure callback duration, active voice count
and negotiated period before selecting a buffering change.

Recorded native mechanical checkpoint before the final capture-control retry:
`captures/winter_uphill_drive800_20261001.mp4` passed4/4 observations. It travelled16.26m on a
source-terrain ascent initially10.29deg, with6→5→4→3 while driving and further2→1 downshifts
while released/rolling back. The slope setup is disclosed; this is not a historical performance
measurement. Its WAV contains352,800 stereo sample frames at44.1kHz, peak0.20956, no clipped
PCM values/nonfinite samples. Four-frame contact sheet inspected.

The first broken and manual clips are retained diagnostics, even though their initial numeric
receipts passed: broken input spanned two60Hz ticks, so the fixture was tightened to one tick; manual view faced
an unhelpfully bright slit/dark dashboard. The second recording established that the real repeat
was a delayed button-animation event (the gameplay edge gate already coalesced the held input). The final controller
uses exactly tick390, verifies engine off AND not cranking, and the manual fixture looks toward
the actual source steering control with the ordinary handheld torch on. Those setup changes are
disclosed, and the focused controller regression passed. No source audio/fire/lighting power
was retuned for these retries.

The strict v2 broken-track receipt then FAILED the end-off-and-not-cranking check and exposed
an actual gameplay double dispatch: key/crew ignition executed immediately, then
`driver_controls.request()` armed the cosmetic reach to emit a second ignition action0.28s later.
The fix separates `request()` (physical button action at contact) from `animate()` (already handled
command, visual reach only). Three actual Controls unit tests passed in a source-extracted fast
Rust harness using real glam; the unused source-mesh declaration alone was stubbed. Full release
integration and native broken-trackv3 are the final checks. This is a gameplay fix in both trees,
not suppression of a failed capture check. Existing v1/v2 failed-review outputs are preserved.

Final integrated shutdown checkpoint: `captures/winter_broken_track800_v3_20261001.mp4` uses
release SHA256`7a946fa2693aafc9f139441cbff7c36b02907a3f3bcedce308206c8a182868de`.
Runtime5/5 and independent verification6/6 pass. Actual native800×450,30fps,240frames/8s;
44.1kHz stereo352,800 PCM frames, peak0.08551, zero clipped/nonfinite samples. The vehicle
translated2.821m with intact-side pivot,247 driven broken-shaft ticks and36 release-silent ticks.
From6.6s onward engine_on=false, crank remaining0, and both attempted shafts/load remain0.
The live log records one shutdown and no new crank. The mixed soundtrack retains ordinary
wind-down/world sound; it is not an isolated clatter stem or a claim of fixed ALSA scheduling.
Contact sheet inspected. The v1/v2 controller/gameplay diagnostics remain preserved separately.

`captures/winter_manual_drive800_v2_20261001.mp4` passed its mechanical5/5 checks with live
stereo audio and actual manual shifts, but the native contact sheet exposes severe milky cabin
lighting when the real torch is on. It remains an explicitly unresolved cabin-lighting diagnostic,
not a clean visual showcase. No extra light, exposure or medium override was used to hide this.
