# Preferred Tiger audiovisual reference — 2026-10-02

User priority: “outstanding reference.” Original preserved at
`/home/alex/Downloads/tiger_visual_and_sound_reference_outstanding-reference.mp4`.
It is **byte-identical** to the earlier `/home/alex/Downloads/tiger1 clip2.mp4`:
SHA-256 `c71a28f774edd3386a7e00b8c33586745598a5551b5e9ad06d2be876dfcf11bb`.
This designation raises its priority; it is not a second independent recording.

Inspected timestamped decoded frames, adjacent frames around the gun flash, a
spectrogram and digital signal statistics. **No direct listening was available.**
The file is an edited montage. Authenticity of vehicles, historical footage,
original synchronous sound, microphone placement, later foley and mastering cannot
be established from it. Use it as a presentation target, not an acoustic or
explosive-yield calibration. Do not copy its soundtrack into shipping assets.

## Observations and useful comparison windows

Times below are original media timestamps, not an inferred continuous action.

| Window | Actually visible / measured | Concrete game comparison |
|---|---|---|
| 0.00–1.43 s | Frontal moving Tiger-shaped vehicle, winter camouflage, open crew positions and ground-relative hull motion. | Hull/crew movement over terrain; keep vehicle inertia distinct from camera motion. The shot does not provide measured suspension travel. |
| 1.43–3.93 s | Tight moving track view with articulated links wrapping the running gear and dirty bright/dark link surfaces. Audio interval has 73.76% of analyzed energy below 250 Hz plus changing mid/high content. | Side-view actual belt motion, contact-driven track texture and attached dirt/snow. Do not infer a measured link cadence or isolated track recording from the mixed soundtrack. |
| 3.93–7.40 s | Convoy, then view from a moving deck with crew in foreground. Tracks/ground and camera move independently; later view shows airborne dust. | Vehicle group movement, ground-contact motion and localized dust/snow transport rather than a camera-attached particle cloud. |
| 7.40–9.40 s | Tight, dark crew/interior framing against a bright opening. Audio is strongly weighted to low/mid bands; only 0.29% above 2 kHz in this interval. | Cabin machinery perspective and dark interior exposure. Spectral difference is useful for an A/B, but this edit does not measure the hull transfer function. |
| 9.40–10.50 s | Exterior tank framed with broad irregular smoke/dust around the gun; the edit starts after the initial flash. | Smoke persistence and irregular dispersal. Do not derive onset or projectile travel time from this shot. |
| 10.50–11.30 s | Narrow armored slit view with close dark occluders and a bright distant exterior. | True small-aperture view, silhouette/readability and confined camera perspective without adding interior fog. |
| **11.30–12.067 s** | Cleanest firing window. At **11.400 s**, a bright yellow-orange flash occupies one decoded frame; it is absent at 11.367 and 11.433 s. At 11.433–11.633 s grey/brown muzzle smoke spreads across/beside the vehicle and continues afterward. The audio interval is 60.62% mid-band and 18.40% above 2 kHz. | Separate short flash, sharp report and longer expanding smoke. Compare near yellow/cream core, orange edge, immediate visibility loss and recovery. These frames do not establish a precise physical flash duration or calibrated recoil stroke. |
| **12.067–13.10 s** | Hard edit to a separate low ground explosion: multiple orange/yellow lobes followed by grey/brown rolling smoke spreading laterally along terrain. | Ground impact plume, lateral dust and irregular smoke boundaries. **Do not treat the 0.667 s gap from the preceding muzzle flash as shell flight or sound propagation.** |
| 13.10–14.067 s | Close vehicle flank and a broad tan dust sheet thrown outward near ground level. | Track/contact emission location, ground-hugging transport and occlusion of the lower hull. |
| **14.067–17.78 s** | A separate distant shot begins with existing smoke; bright detonation develops around 14.6–14.9 s, rises into a tall lobed orange plume and persists through the end. A dark fragment is visibly separated above the plume around 15.5–16.3 s. | Catastrophic event distinct from a small impact: sustained evolving plume, independent debris trajectory and lingering base smoke. Fragment identity, exact scale, fuel type and yield are unverified; do not label it a measured turret toss. |

Best compact A/Bs: **1.43–3.93 s** for running gear; **7.40–9.40 s** for
interior perspective; **11.30–12.033 s** for muzzle flash/smoke; **12.067–13.10 s**
for a separate ground impact; **14.40–17.78 s** for catastrophic plume evolution.
Keep the intervening edits visible or label these as separate shots. This file
does not clearly resolve individual armor-hit sparks, tracers, a reload/case
contact sequence or an authenticated MG shot; retain the other references for
those requirements.

## Signal measurements and limits

Video: H.264, 1280×680, 30 fps, 535 frames, 17.833 s. Audio: AAC stereo, 48 kHz;
decoded analysis contains 853,632 sample frames (17.784 s). Full audio/video decode
completed without reported errors. AAC padding/container duration differs from
decoded duration; no missing final gameplay event is inferred.

Decoded sample peak **−5.72 dBFS**, RMS **−16.18 dBFS**, crest factor **10.46 dB**;
zero non-finite samples and zero samples with absolute amplitude ≥0.99. This is
not an intersample true-peak or analog clipping test. Stereo correlation is 0.8794.
Hann-4096 STFT energy, mono mean and 512-sample hop, normalized only over
20 Hz–12 kHz: **62.14% / 32.40% / 5.46%** in 20–250 / 250–2,000 / 2,000–12,000 Hz.
These percentages depend on the stated analysis and differ slightly from the
earlier full-file frequency analysis; they are not SPL measurements.

The soundtrack changes spectrum strongly between machinery/interior and the
visible gun window, supporting separate source families and perspective rather
than one generic bass/noise voice. It does **not** justify blindly matching these
percentages with EQ, global gain, or per-camera muffling. The final giant plume is
not the digital loudest segment; editing/microphone/dynamics are unknown, so its
RMS cannot set the game's catastrophe/impact level hierarchy. Peaks in a window
are not acoustic arrival onsets. Echo identity, authenticity and subjective
realism still require direct listening and provenance, while the game must retain
its actual wavefront timing and environment-dependent reflections.

## Review artifacts

All are small local derivatives in the flat `captures/` directory; no runtime
source, asset or soundbank was changed:

- `tiger_outstanding_contact_20261002.png` — overview, original 1 s timestamps.
- `tiger_outstanding_firing_detail_20261002.png` — 8.8–12.3 s at 0.1 s spacing.
- `tiger_outstanding_muzzle_frames_20261002.png` — every frame, 11.267–11.633 s.
- `tiger_outstanding_detonation_detail_20261002.png` — later plume sequence.
- `tiger_outstanding_audio_structure_20261002.png` — RMS, spectrum and band balance.
- `tiger_outstanding_signal_20261002.json` — reproducible measurement definitions,
  interval statistics, source hash and explicit limitations.
