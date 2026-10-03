# Playtest checkpoint — 3 October 2026

This is a checkpoint of the integrated work, not a claim that the complete requested damage, sound, or performance audit is finished.

## Last playable build

Release SHA256: `57746019c3458a2f2b6b478faf2e701dcff6fa96d31b822f4bdb92a08308e3de`.

Launcher: `tools/launch_fixed_playtest.sh --play`; fixed RTX 4090/Vulkan profile, 800×450 internal resolution, Studio Display fullscreen. The rigid sky-depth retention experiment is explicitly disabled by this launcher because repeated transform invalidation made it slower. Exact rigid triangle paging remains opt-in.

## Last verified camera correction

The local source body now stays at the physical station eye rather than following the remote optic camera. The gunner exterior snapshot excludes local observer geometry and restores its exact visibility for the interior surround. Hand IK preserves the original approximately 0.00879023 bone scale; the previous unit-scale replacement enlarged hands about 114 times and blocked the camera.

Two original source-arm CPU tests passed. The actual GPU P08 fixture passed ammunition, firing, reload and own-roof collision assertions, and its aimed capture visibly shows the exterior, pistol sights and original hands. The gunner capture also shows the exterior through the optic. Evidence: `captures/p08_character_clearance_20261003_v4_aimed.png`, `captures/winter_bench_gunner_shipping_physical_body_sight_fix_20261003.png`; local live launch captured as `captures/camera_hotfix_live_launch_20261003.png`.

## Validation boundary and remaining work

- Final release compilation passed. Earlier all-target compilation passed before the last camera corrections; the final camera test binary was compiled separately.
- Earlier broad main-unit sweep: 1861 passed, 9 failed, 61 ignored. Several failures have subsequent targeted fixes, but the full suite has not been rerun on this checkpoint. Do not call it fully green.
- Actual audio-output delivery fixture passed with zero starvation in 30 seconds after queue sizing matched the hardware buffer. This does not prove subjective reference fidelity or every live gameplay sound route.
- Catastrophic sound crest correction is included but its severity regression guard still needs a fresh run.
- Performance remains insufficient. Benchmark timings must disclose whether the same singleplayer body is enabled; the benchmark now enables it, so earlier no-body timings are not directly comparable.
- Exact armor source coverage remains partial. Read `PROJECTILE_ARMOR_SOURCE_AUDIT_20261003.md`; do not claim every plate/material/manufacturing property is modeled or that every crew/body aftermath is complete.
- Native hover outline acceptance, broader varied-pitch/player body clearance, full combat audio reference comparison, complete regression sweep and presented-window frame-time validation remain outstanding.

Source, supporting original arm binding metadata, tests and audit documents are committed. Large runtime assets, binaries, logs and captures remain local under the existing repository policy. Nothing was deleted as part of this checkpoint.
