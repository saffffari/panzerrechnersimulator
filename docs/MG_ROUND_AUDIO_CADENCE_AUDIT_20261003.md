# Mounted MG round/audio cadence audit

The live MG34 is 800 rpm: one real round every 75 ms. `mounted_mg::shot_offsets` preserves every interval across low render rates/catch-up. Each `fire_mg_round` launches one authoritative ballistic projectile, schedules one external muzzle report at that source timestamp, and plays one structure-borne receiver cycle for enclosed listeners. External arrival retains physical propagation. The listener receives two physical source components of the same round, not two ballistic rounds.

The old continuous MG loop is explicitly called with `false` in live `Game::update`. Its legacy APIs remain for isolated review/tests but do not supply runtime firing. Trigger feedback only emits a failed-trigger click on a deliberate pull. No independent live shot synthesizer/cadence was found.

The native visible belt mix selects every fifth round: 2.667 luminous tracers/second, versus 13.333 reports/second. This intentional authored mix alone produces a fivefold apparent visual/audio difference. Ball rounds remain unlit. The unused legacy `MG_TRACER_EVERY=2` constant was misleading and is removed; it did not drive the authoritative path.

Another visual limitation is the existing 1/120-second shutter history. At low presented frame rates, close impacts can finish between frames and never appear as submitted tracer geometry. Root/visual coordinator has this finding; this audit does not change tracer ammunition or manufacture audio for missed visual frames.

Added a cross-path timeline test: 134 authoritative rounds in the half-open ten-second fixture (first round at t=0), 26 selected tracer rounds, 375 ms between selected tracer launches, invariant at 60, 4 and 2 updates/second. Existing actual-mixer timestamp test independently verifies 75 ms audio spacing at 44100 Hz.

No subjective sound realism or native pixel visibility acceptance is inferred from these counts.
