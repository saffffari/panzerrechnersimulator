# Playtest and campaign readiness — 14 September 2026

Worktree: `/home/alex/panzer/shader-graphics` (shader renderer). Build with
`tools/launch_shader_playtest.sh --build --playtest`; the canonical renderer installation is separate.

## KIA replay

Tank KIA now plays three seconds of black, four seconds following the incoming shot to the armour,
and four seconds inside the compartments. The interior timeline advances 1.2 seconds of the
recorded terminal aftermath in slow motion. Crew identities, anatomical damage, emitted droplets,
fragment bodies and existing blast envelopes survive the camera cut. The camera first witnesses
the struck compartment; when recorded turret casualties exist, it also shows the turret aftermath.

The interior pass applies no new damage, crew deaths, ammo expenditure or mission progress.
Fuel-fire cookoff goes directly to the internal cause after black; it does not invent a new incoming
shot. Deaths without a recorded fatal tank hit retain the black-to-memorial fallback. A new keypress
can skip the replay after the mandatory black interval.

`replay-aftermath` records a real resolved BR-412 ammunition detonation in the garage with all five
solo crew enabled. The verified fixture retained five anatomical records, 10 destroyed and 24
detached body parts, and 8,160 emitted blood grains. Body positions change across the recorded
phases while casualty outcomes and emission counts remain fixed. This is a controlled render
fixture, not a claim of a human-played fatal encounter or an audio listening review.

Evidence: `artifacts/readiness-20260914/replay-verified.log`,
`captures/astra/kia_replay_verified_20260914_{incoming,entry,interior0,interior1,interior2,interior3}.png`,
and `captures/astra/kia_replay_verified_20260914_motion.json`.

## Campaign

The current three-act campaign built and its structural verifier completed Acts I, II and III,
reaching the TownHeld ending with the same Tiger. The verifier deliberately forces objective
completion: it proves progression plumbing, not a complete human playthrough. Campaign-focused
CPU tests passed: 72. Replay-focused tests passed: 13. Act II bridge/landmark tests passed: 5. The first CPU invocation lacked the shared
asset root; the corrected invocation uses `KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator`.

There is no discovered mandatory editor fix for launching the current prototype. Before treating
it as a finished campaign, review Act II's bridge approaches and Acts II/III landmark dressing.
Several landmark hero models are absent and use blockouts; the SU-100 also reports a placeholder.
The later sectors are visibly sparse in the current captures. General route tests cannot prove every
on-foot path through authored buildings is comfortable to use.

Campaign uses `assets/worldgen/act1`, `act2`, `act3` and any adjacent `manifest_edits.json`.
The winter-approach revision selected by Playtest is isolated and does not automatically replace
Campaign's Act I. Existing campaign saves retain sector-entrance semantics. The integrated ending
is still the three-act Halbe prototype; the planned Reichstag campaign is not implemented.

Evidence: `artifacts/readiness-20260914/campaign-verify.log`, `campaign-tests-assets.log`,
`act2-tests.log`; rendered sectors and ending under `captures/astra/campaign_check_20260914/`.

## Display presets

See [DISPLAY_PREVIEWS.md](DISPLAY_PREVIEWS.md). Esc → Settings → Display / Adafruit previews.
The 3.5, 4.3 and 5 inch options use native 320×240, 480×272 and 800×480 rendering and size the
borderless window from the monitor's reported physical dimensions. Alt+Enter restores the desktop.

Window-control QA wrote the open test run's Playtest sector-entrance checkpoint to
`artifacts/shader-state/panzerrechnersystem/playtest-act1-checkpoint.v1.json`.
The running Playtest remains on the earlier binary until its restart is approved.
