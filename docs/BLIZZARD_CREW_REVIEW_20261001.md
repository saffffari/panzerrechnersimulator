# Blizzard firefight: crew-view review, 2026-10-01

The final v6 review includes an exposed commander/cupola view, a closed-hatch loader view
beside the Tiger's breech, and an exterior comparison. The two crew recordings capture the
game's actual audio graph, including the selected station's enclosure, headset, propagation,
hearing response and death fade. They do not use the exterior observer soundtrack.

| View | Video | Length | Frames |
| --- | --- | ---: | ---: |
| Cupola, hatch open | `captures/blizzard_cupola_v6_20261001.mp4` | 12 s | 360 |
| Inside, loader beside breech | `captures/blizzard_inside_v6_20261001.mp4` | 12 s | 360 |
| Exterior comparison | `captures/blizzard_external_v6_20261001.mp4` | 14 s | 420 |

Each is H.264 at 1280×720/30 fps, upscaled from the native 640×360 shader render, with stereo
44.1 kHz AAC. Matching `.json` receipts and `_soundtrack.wav` files remain beside the videos.
`catalog/combat_crew_revision_20261001.json` records their hashes, decode checks, events,
motion measurements, focused test results and the release executable's hash.

These are independent deterministic encounters with an automated crew. Initial placement,
gun lay and station controls are staged; shells, armor contact, damage, enemy shots and weather
advance through the ordinary game simulation. They are not a human playthrough, synchronized
replay, or real-time performance benchmark. Each run contains one Tiger shot, three enemy
shots, three incoming hits and one outgoing hit. Both crew listeners survive the Tiger's shot
at approximately 4.02 s and the earlier incoming hits; the Tiger's ammunition detonates at
9.93 s. The last part of each crew recording therefore includes the actual terminal hearing
fade while retaining the review camera.

The pass includes shell-flight whistles varied by shell characteristics, separate firing and
impact sources, the catastrophic blast/tearing pair, finer exterior sparks, source-derived
detached-turret motion, and fire distributed from damage onto hull and turret surfaces.
The sound, contact and combustion approximations are recorded in `PHYSICAL_TRUTH_AUDIT.md`
and the accompanying feature notes; this is not measured historical explosion physics.

## Verification

- Release build passed. The final focused suites passed 63 tests, with three existing GPU
  review tests ignored. Test filters and logs are recorded in the catalog.
- All three simulation receipts passed, all videos decoded completely with FFmpeg, frame
  counts and durations matched, and the exact-length stereo waveforms contained finite
  samples within the master limiter's ceiling. All three final soundtracks are bitwise
  identical to the validated v4 soundtracks; the last changes affect cosmetic geometry only.
- The native T-34 turret's maximum COM rise was 0.2951 m and largest contact correction
  0.0072 m. It settled on the wreck without the earlier large positional jump.
- Whistle analysis found a 1712 Hz dominant component in the isolated pre-gun flight window;
  98.15% of that window's power was in 1100–2800 Hz. This verifies the intended high-frequency
  source survived propagation and the master mix. It is not a subjective listening verdict.
- Catastrophe-versus-impact tests cover 10, 22, 50 and 100 m exterior winter paths after the
  shared master. The 22 m test measured a 12.81 dB(A) short-term difference and 6.92 dB
  broadband RMS difference. The dying crew's hearing response can muffle its own fatal blast.
- Frame sequences were inspected for the crew viewpoints, fine exterior sparks, smoke,
  fire, and the turret's flight and settlement. The retained contact sheets are named
  `blizzard_{cupola,inside,external}_v6_final_contact_20261001.jpg`.
- The low-frame-rate cupola and interior probes isolate the fatal hit at 9.93 s. Exterior
  armor ejecta stay around the outside impact; the former large white streaks through the
  cabin are absent. Close exterior sparks still have the ordinary bloom and concussion
  response. The full-rate exports are checked separately after those probes.

The superseded v3 exterior recording is **not acceptance evidence**: native review exposed a
hull-local/world contact-frame error and a moving-whistle filter transient. Both were corrected,
regression-tested and recaptured in v4. The v4/v5 crew review also exposed cosmetic exterior
ejecta crossing the cabin: the cabin ignition duplicated the exterior spray, and the
armor-impact spray ignored the struck plate normal. The final correction separates interior
ignition from exterior ejecta and confines armor-impact grains to the exterior contact plane.
Authoritative cabin fragments and damage remain unchanged. Earlier files remain for provenance.

## Reproduce

Use fresh output prefixes to preserve these reviewed artifacts:

```sh
KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=640x360 \
  target/release/panzerrechnersystem blizzard-battle captures/blizzard_cupola_NEW 12 cupola
KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=640x360 \
  target/release/panzerrechnersystem blizzard-battle captures/blizzard_inside_NEW 12 inside
KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=640x360 \
  target/release/panzerrechnersystem blizzard-battle captures/blizzard_external_NEW 14
```
