# Own-gun headroom correction, 2 October 2026

The loaded-clip f32 correction removed pre-master i16 clipping. The subsequent actual device-free Audio review proved live/offline LAF parity improved from +5.41 dB to 0.00 dB, but exposed excessive source crest and master limiting. Its 49/53 passing checks are preserved in `captures/hotfix_audio_float_20261002_receipt.json`; the four failed relations were not relaxed.

The existing own-gun source concentrated most unweighted pressure into a 49 Hz lobe and a roughly 25 ms onset. Its active interior waveform had 26.64 dB peak-minus-LAF crest; at the unchanged calibrated target it demanded +9.45 dBFS before the shared master and caused 10.45 dB gain reduction. The old 15.69 dB historical reference is a shared exposure anchor, not this current waveform.

Only `synth_gun_punch` now adds an authored near-source response: a causal 45 Hz high-pass removes excessive deep displacement, followed by five irregular, briefly decaying pressure/structure modes, with a 4 ms rise and a finite 150–230 ms fade. The interior body is darker than the exterior body. Seeded detuning and phase variation remain deterministic. This is an authored timbre, not a measured Tiger pressure or resonance reconstruction. No new shot event or recoil event is added. Propagation, physical SPL references, headset/enclosure loss, hearing model, shared exposure and master settings are unchanged. A final small shared-source correction increases only the 118 Hz pressure-lobe coefficient from 0.44 to 0.49: earlier charge-duration variation had reduced the remote 88 mm 100–350 Hz body fraction below its existing 10% requirement. The correction restores all six 76/88 mm seed cases without relaxing those assertions.

CPU prototypes extracted the exact saved source helpers, A-weighted meter and master into a standalone Rust executable. They are not the full Audio/hearing graph and do not establish subjective authenticity:

| Current variant | Source crest dB | Post-master LAF dBFS | Deepest master reduction dB |
|---|---:|---:|---:|
| Interior 1 | 16.10 | -17.19 | 0.00 |
| Interior 2 | 17.20 | -17.62 | -1.01 |
| Exterior 1 | 14.93 | -14.98 | -5.24 |
| Exterior 2 | 14.13 | -14.40 | -4.44 |
| Exterior 3 | 14.15 | -14.31 | -4.46 |
| Exterior 4 | 14.16 | -14.37 | -4.47 |

The unchanged remote-source regression retains its original low-pressure spectral expectations. A separate own-gun regression now verifies the intended audible body over excessive deep displacement, minimal high-frequency hiss, deterministic finite decay, all active variant crests, calibrated master headroom and hatch/head-out ordering. It replaces the old own-gun-only assertion that all 160–3000 Hz energy must be less than one third of <160 Hz energy, which encoded the failed subbass-heavy design. The 53 mix acceptance ranges are untouched.

Validation to run after rebuilding:

- `acoustics::tests::own_gun_pressure_body_preserves_transient_and_calibrated_headroom`
- `acoustics::tests::cannon_sources_keep_pressure_body_ahead_of_noise_and_finish_promptly`
- `own_gun_active_variants_match_the_calibrated_float_chain_and_master`
- `impact-mix-review` with a fresh prefix, asset root set to this checkout and `KESSEL_ENABLE_PIZZA_AUDIO=1`.

The user's original WAV references remain read-only. `main_gun_and_reload_perfect.wav` has 17 full-scale samples and is a mixed full-cycle recording, so its whole-file crest is not an isolated gun-source calibration. No reference audio was imported into the runtime. Audition of the full live gun/recoil/reload path remains necessary; the isolated mix review includes only the report, not actual recoil and loader contacts.

## Integrated review checkpoint

The rebuilt own-gun response passed its headroom test and active-source float/master parity test. `captures/hotfix_audio_body_20261002_receipt.json` passed 54/54 mix checks: all 53 prior ranges unchanged, plus the conditional curve-placement check now applicable because the buttoned gun no longer limits. Gun/bed difference 16.36 dB, head-out/buttoned 2.21 dB, hatch/buttoned 0.55 dB, fresh-ear reduction 2.78 dB, live/offline parity 0.01 dB. All 24 WAV payloads are complete, with no hard-clipped PCM16 samples. No subjective audition was performed.

This integrated review preceded the final 0.44→0.49 remote-body correction. Its unchanged six-case spectral regression passed against exact extracted saved-source functions; full rebuilt review of that final coefficient remains pending. Final source-only meter results are in `artifacts/runtime/hotfix_gun_envelope_final_source_20261002.log`; buttoned variant 1 still has 0 dB master reduction and variant 2 about 1 dB.
