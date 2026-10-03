# Regression audit — 3 October 2026

## Verified rendering cause

The Vulkan renderer uses the RTX 4090. The scene grid is 800×450; the Studio Display-sized 5120×2880 output is a final upscale. A low pixel count does not constrain geometry and shadow submission costs.

The gunner composite alternated a 6 km exterior view and a 24 m cabin view. These independently changed the source-light domain, invalidating static sky and point-light depth caches twice per frame. The environment atlas also submits 66 directional views. The large terrain was previously submitted whole to these shadow views.

The corrected renderer shares conservative source support for the complete frame, retains independent static atlas groups, and uses exact terrain triangle pages for shadow culling. It does not remove shadow casters or disable light occlusion.

Controlled real-GPU, fenced headless tests (60 timed frames, 24 warm-up frames, actual winter map, seed 20261003, high/MSAA4/Bayer8, 5K final blit):

| Fixture | Before median / p95 | Candidate median / p95 |
|---|---|---|
| Gunner aiming | 272.13 / 295.55 ms | 39.40 / 43.82 ms |
| Moving commander, forced five-tick catch-up | 178.99 / 188.30 ms | 91.23 / 100.90 ms |

Receipts: `artifacts/runtime/interior_gunner_precache_20261003.json`, `interior_gunner_finalpages_20261003.json`, `interior_commander_precache_moving_20261003.json`, `interior_commander_finalpagesdrive_moving_20261003.json`.

These are not presented-window FPS measurements. The moving fixture deliberately forces simulation catch-up. Compilation overlapped portions of this verification session, so CPU measurements warrant an uncontended repeat. Do not describe this as smooth gameplay or a complete performance fix.

## Remaining measured costs

Moving candidate GPU timestamps: static point shadows 12.67 ms; six optic exterior renders 9.84 ms; dynamic ammunition shadow composition 9.54 ms; dynamic sky visibility 8.37 ms; main scene 4.21 ms; static sky 3.02 ms. Simulation median 14.33 ms for the forced catch-up workload. GPU passes may overlap; do not sum these as total frame time.

Further candidates requiring separate validation: viewer-hidden optic captures; repeated optic MSAA resolves; dynamic sky atlas updates and moving point-shadow geometry. The old one-RTT-tile-per-frame diagnostic does not describe the current shader optic path.

## Visual corrections

See CUPOLA_READABILITY_20261003.md, OBSERVER_TRANSITIONS_20261003.md and CABIN_PRACTICAL_SPECTRUM_20261003.md. The accepted dark interior/readable port appearance is retained. Source geometry fixes clear the right port; short glances cannot trigger a false weapons-flash afterimage. Slit/aperture-guided indirect lighting of the cupola ring remains outstanding.

## Audio finding — not fixed

The normal launcher enables the verified private audio bank, but that is not equivalent to replacing the Tiger sound families. `src/audio_metadata.rs` explicitly reports generated engine/MG/mechanical sources unless their optional recorded routes are active. `src/audio_feedback.rs::replacement_key` does not replace the tank MG. Ammunition contacts remain authored procedural modes. Existing reference-review exports also isolate the main report and explicitly omit recoil mechanisms and reload.

Therefore bank load success and isolated event exports cannot establish that the complete vehicle sounds like the supplied recordings. No subjective reference match is claimed. Next audio acceptance must compare complete in-game firing/recoil/extraction/ramming and moving engine/track sequences with the user's recordings, including source identity, actual mix and interior filtering. Ambient music integration is separate from those vehicle effects.
