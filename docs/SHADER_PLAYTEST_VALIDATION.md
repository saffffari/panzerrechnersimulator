# Shader Playtest validation

This worktree is `/home/alex/panzer/shader-graphics`, branch
`codex/shader-graphics-rebuild`, starting from gameplay checkpoint `c9e7178`.
The current desktop launcher and binary remain in `/home/alex/panzer/tank_simulator`.

## Branch launcher

```bash
tools/launch_shader_playtest.sh --check
tools/launch_shader_playtest.sh --build --playtest
tools/launch_shader_playtest.sh --menu
```

The default mode is Playtest. `--check` only reads paths and selected bank receipts;
it never builds, launches, creates a GPU device, or plays audio. Build and launch must
wait for the shared GPU/build gate while another agent is validating the engine.

The launcher pins `KESSEL_RENDERER=shader`, `KESSEL_INTERNAL_RES=640x360`, and the
branch-local `CARGO_TARGET_DIR=…/shader-graphics/target`. It runs with the worktree as
its working directory and `KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator`.
No asset library is copied or linked. Shader run logs use `artifacts/runtime/` here;
branch checkpoint state uses `artifacts/shader-state/`. Default media belongs in the
flat `captures/astra/` directory here. The launcher holds its own lock and the existing
canonical launcher's lock, refusing a second game. It does not install desktop entries.

## Confirmed gameplay input receipts

Both original scenarios were evaluated on the canonical release at `c9e7178` using
`mechanic-evaluate` on 2026-09-09. The evaluator constructed renderer resources with
`KESSEL_NO_RT=1`, but rendered no frames and played no audio. It ran the ordinary
Game input, simulation, and device-free audio clocks at 120 Hz.

| Scenario | Canonical state acceptance | Shader state / visual acceptance |
| --- | --- | --- |
| Reload recovery | 9/9 checks, 20/20 actions; two shots, case extraction and reload, three floor rounds, rack 65→61, empty hands/chamber at end | Pending / pending |
| Cupola exit, roof descent and P08 | 6/6 checks, 2/2 actions; feet on roof at 2 s, terrain by 4 s, eye 1.70 m above feet, P08 ammunition 16→15, HP 100 | Pending / pending |

Canonical receipts remain at:

- `../tank_simulator/artifacts/play-readiness-20260909/reload_recovery_cpu_20260909_1915_manifest.json`
- `../tank_simulator/artifacts/play-readiness-20260909/dismount_roof_p08_cpu_20260909_1915_manifest.json`

Only the two small scenario JSONs were copied byte-for-byte into this worktree:

| File in `captures/astra/` | SHA-256 |
| --- | --- |
| `shader_c9e7178_reload_recovery_20260908_scenario.json` | `28d920b787c33d5e98f16e9672e557326a7d3c95c6f40aeac6eb6e1cddbfe0b6` |
| `shader_c9e7178_dismount_roof_p08_readiness_20260909_scenario.json` | `d4e636e0f637d9fca070c174755eceac09c9d2fa69aea2f6f94d0b2594939cbd` |

The scaffold gallery is `captures/astra/shader_playtest_validation_20260909.html`.
It deliberately lists GPU acceptance as pending and contains no placeholder videos.

## Repeat on the shader branch

After the branch build is available and the GPU gate is clear, run each evaluation
sequentially from this worktree. The timestamp/PID prefix avoids replacing evidence:

```bash
export KESSEL_RENDERER=shader
export KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator
export KESSEL_INTERNAL_RES=640x360
export XDG_STATE_HOME="$PWD/artifacts/shader-state"
run_id=$(date +%Y%m%d-%H%M%S)-$$
target/release/panzerrechnersystem mechanic-evaluate \
  "$PWD/captures/astra/shader_reload_${run_id}" \
  "$PWD/captures/astra/shader_c9e7178_reload_recovery_20260908_scenario.json"
target/release/panzerrechnersystem mechanic-evaluate \
  "$PWD/captures/astra/shader_dismount_${run_id}" \
  "$PWD/captures/astra/shader_c9e7178_dismount_roof_p08_readiness_20260909_scenario.json"
```

For a separately scheduled visual pass, use `mechanic-review` with a new prefix.
Review the extraction axis, full-size carried case, held-mouse camera lock, three
independent floor rounds, standing roof support, continuous jump, upright P08 through
walking yaw, and flash at its posed muzzle. Final counters alone do not validate these.

The current scenario harness calls `build_act` then `playtest_setup` for a Playtest
fixture; that is sufficient for these isolated input paths but does not establish the
dedicated live Playtest roster/return-fire behavior. Check the actual branch window for
six T-34 targets, stationary safe practice, and the explicit return-fire toggle.

## Runtime path and interface boundaries

`FootPolicy::load` now uses `resource::open`, matching `Mlp::load`, so the Tiger and
Soviet policy files resolve under the explicit asset root instead of silently falling
back because `assets/ai` is absent locally. Its regression test uses a child process
with an empty working directory, an external asset root, an absolute artifact path,
and wrong-role rejection. Run it under the root-controlled Cargo gate:

```bash
CARGO_TARGET_DIR="$PWD/target" cargo test --locked --bin panzerrechnersystem \
  runtime_policy_load_resolves_external_asset_root_and_keeps_absolute_artifacts
```

The canonical German infantry checkpoint is already absent; doctrine fallback remains
the baseline. Explicit `KESSEL_COMBAT_WEIGHTS_DIR` / `KESSEL_COMBAT_POLICY_DIR` overrides
must be absolute here because auxiliary policy loading accepts literal user paths.
Terrain, models, dressing, and feedback audio already resolve runtime paths. Source
tests that directly read untracked assets may need absolute/resource-resolved fixture
paths; that is distinct from a runtime asset failure. Do not blanket-link `assets/`.

The shader backend still must preserve stable mesh/prototype/pool handles, model and
visibility updates, source material channels, all station cameras and optics, gunner
snapshot composition, output-resolution UI, pause/focus handling, scene-depth effects,
and explicit no-RT device startup. CPU receipts above prove none of those graphics
conditions. Actual shader build, visual acceptance, and interactive frame timings remain
pending until fresh results are recorded.
