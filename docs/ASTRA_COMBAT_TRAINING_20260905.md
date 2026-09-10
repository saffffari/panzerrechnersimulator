# Competitive infantry and tank soft-target training — 2026-09-05

The new `train_combat` target runs genuine combat episodes without constructing a renderer or GPU
adapter. Small CPU MLPs do not benefit from allocating a 4090 merely to run individual forward
passes. Eight deterministic CPU rollout workers leave capacity for the game build and captures.

Existing tank KSL1 maneuver policies and TigerCrew/Soviet KSL2 checkpoints remain untouched.
`GermanRifle` has KSL2 role byte 3, with the existing 51→32→10 dimensions/schema preserved. The
existing two faction lanes encode German-side vs Soviet-side; the header distinguishes German
riflemen from the mission-oriented TigerCrew policy. The initial German policy warm-starts from
the Soviet policy with the two faction columns exchanged, explicitly recorded as initialization.
A separate 24→24→5 tank auxiliary chooses drive, steer, traverse, MG and HE intents. It never
replaces the old tank-versus-tank policy file.

## Shared combat and limits

`infantry_combat.rs` supplies team/actor state, observed cues, movement, legal firing, ammo/reload,
nearest cover/body hitscan and exterior HE casualty resolution. Episodes contain four German and
four Soviet riflemen. Rifle hits cause 34 HP; direct P08/MG hits preserve the game's 100 HP soft-target
law. Small arms have no armor damage. Friendly bodies can intercept a shot. HE projectiles fly
with the existing shell velocity/drag/gravity, sweep to physical cover/body/ground contact, then
apply the same finite-radius cover-gated casualty law intended for runtime. MG heat/cooling,
finite ammunition and shell reload constrain firing. No reward is granted for requesting fire.

Only targets passing range, field-of-view and cover checks enter observations. Last-known positions
expire after five seconds. The policy gets no hidden target HP or exact unseen position. Assigned
objectives are map waypoints. Both teams decide from the same starting snapshot before damage is
resolved. The maps use flat ground and randomized 3D cover: this is a bounded combat curriculum,
not proof of campaign navigation or terrain generalization. The exterior HE radius/damage law is
an explicit game abstraction, not a validated biological injury or fragment-transport model.
The tank curriculum alternates native F-34 OF-350 and M-10T OF-530 profiles by seed. Soviet
training tanks fight German infantry. Gun pivots, barrel/coax offsets, bounded elevation slew,
3D fire tolerance, MG dispersion, 5.5-second game reload and analytic HE flight law are shared
with runtime. The OF-350 velocity and drag remain explicitly provisional; the retained OF-530
profile has not been primary-revalidated. No gun receives an 88 mm fallback.

## Training and evaluation

Each generation mutates one track: German infantry, Soviet infantry, or tank soft-target control.
It evaluates real damage, kills, survival, friendly damage and ammunition use against the opposing
champion and an opponent archive. There is no action-imitation loss in this evolutionary loop.
The loaded micro-curriculum artifacts are warm starts only.

Selection uses a fixed validation bank against frozen initial and scripted opponents. A different
sealed seed bank is evaluated at run end for candidate and initial baseline. The run manifest
contains source hashes and exact seeds; per-generation JSONL includes actual shots, casualties,
damage and weapon attribution. Candidate file writes are atomic and reloaded to verify their
role/schema/weights. Initial files, improving checkpoints and final evaluation remain in the run
directory. Shipping promotion is always false; runtime activation requires an explicit candidate
path after inspection of held-out results and live behavior.

Example bounded smoke:

```sh
CARGO_BUILD_JOBS=4 cargo build --locked --release --bin train_combat
target/release/train_combat artifacts/astra-combat-training-20260905-smoke-new \
  --minutes 1 --generations 6 --population 8 --episodes 2 --seconds 10 --workers 8
```

A 60-generation, 32-member population run with 8 × 45 s training matches completed 15,360 training episodes in
about 5.3 seconds in the initial HAL timing witness. This is renderer-free synthetic-cover throughput,
not live Game update speed or rendered game FPS. Root subsequently expanded validation and sealed
test banks for the overnight run; use that run's manifest for its exact evaluation counts.

The second timing witness used real HE ground-contact aiming and clustered/spread infantry. Its
32-seed tank exam fired 1,015 MG rounds for 6,058.8 HP damage and 28 HE shells for 4,684.3 HP blast damage.
Candidate/baseline scores were German −80.04/−196.11, Soviet −88.79/−188.52 and tank 609.08/605.29.
Those small comparisons establish a working loop and distinct weapon consequences, not release
acceptance or historical tactical accuracy. The initial smoke actually regressed Soviet/tank
sealed scores, demonstrating why validation success alone must not promote a policy.

## Runtime handoff

- `KESSEL_COMBAT_WEIGHTS_DIR=/absolute/run/candidate` selects `german_rifle.ksl2`,
  `soviet_rifle.ksl2`, and `tank_soft_targets.json` with strict role/schema validation.
- No candidate path means existing foot weights and the explicitly labelled combat doctrine.
- `KESSEL_MIXED_COMBAT=0` retains the former single-player-target infantry path for regression.
- `Game::debug_stage_mixed_combat(center)` stages a live 4v4 encounter on actual terrain.
- `Game::mixed_combat_telemetry()` reports actual per-team rifle shots/damage and tank MG/HE events.
- The four German NPCs use the existing CC0 German body asset and never enter Soviet animation
  batches. A static German body pose is a known visual limit until proper locomotion/fire clips land.

Do not call runtime integration visually accepted until the normal first-person camera and real
sound output show both teams exchanging fire, cover preventing hits, and the correct weapon's
projectile/impact sequence. Root owns those captures and the decision to activate candidate files.

Runtime implementation receipt: `cargo check --locked --bin kessel` passed on September 5.
The explicit override is loaded per Game, bypassing the old checkpoint `OnceLock`; the older
`KESSEL_COMBAT_POLICY_DIR` spelling remains an alias. Telemetry includes actual actor positions,
health/ammo/last-seen state and tank target coordinates, aim point, laid gate, requested weapons,
native HE availability, remaining ammo, heat, HE terminal events, and weapon-specific damage.

Transfer limits still requiring live evaluation: runtime runs the infantry policy at 5 Hz but
resolves movement on the Game timestep. Training now uses turret yaw relative to the hull, so hull
turning moves the gun as it does in runtime. Training still uses simplified flat-ground hull/cover
collision and 50 ms swept projectile chords; runtime uses the scene and finer projectile steps. The runtime auxiliary currently supports MG on turreted
Soviet tanks, a native 76 mm OF-350 review candidate on the T-34, and the existing 152 mm
OF-530 model on the KV-2. OF-350 projectile mass/TNT filling are primary-verified; velocity remains
provisional and drag uncalibrated. See `research/SOVIET_HE_PROVENANCE_20260905.md`. Casemate
auxiliary control and missing 85/100/122 mm native HE definitions remain unimplemented. The gun physically elevates and shells
use the authoritative projectile loop; training scores do not prove that transferred policy aims
well in this domain. Existing tank-versus-tank KSL1 controls retain priority against a spotted,
living mounted Tiger. The new mixed-team perception path supplies shared range/FOV/cover sensing;
it does not yet reproduce every old hearing/alarm-cascade/DAgger feature. The regression switch
retains that legacy path and omits the added German NPC presentation.


September 5 weapon-parity correction: AI elevation now uses the actual trunnion, not the lower
turret ring. The 3D firing gate is shared with the trainer and cannot be satisfied by horizontal
traverse alone. Training no longer assigns pitch instantaneously. Actual auxiliary HE launch IDs
are registered and consumed at contact; `tank_he_impacts` and `tank_he_damage` now count those
auxiliary shots only. `primary_tank_he_impacts` / `primary_tank_he_damage` preserve the separate
primary-policy result. All enemy HE still applies the ordinary physical contact and shared casualty
law. Actor telemetry includes effective stance, observed stance and remembered aim point.

Prior candidate scores predate these dynamics and must be re-evaluated or retrained. These changes
do not promote or overwrite a policy checkpoint, and CPU tests are not first-person acceptance.

## Frozen v8 parity run and transfer evaluation

`artifacts/astra-combat-training-20260905-parity-v8` completed 10,000 generations in 895.82 s with
32 candidates, 8 training episodes, 45 s episode limits and 8 CPU workers. Its immutable executable
SHA-256 is `bcecc77e9a57e5f68d3dbade092002358f31c4ec0119402833325025144dbb4f`.
`run.json` records compile-time source hashes; the later live checkout is not its source receipt.
The sealed test uses 128 separate seeds, alternating hand-coded combat doctrine and fixed initial
checkpoint opponents. `sealed-summary.json` preserves metrics and candidate hashes.

| Policy | Initial score | Candidate score | Interpretation |
|---|---:|---:|---|
| German rifle | -206.26 | 351.61 | Improved against initial policies; no wins against doctrine |
| Soviet rifle | -200.31 | 193.41 | Improved against initial policies; no wins against doctrine |
| Tank MG/HE | 586.585 | 586.628 | Effectively tied; no demonstrated general improvement |

German wins 61/64 encounters against the initial policy and 0/64 against doctrine (3 draws).
Soviet wins 60/64 against the initial policy and 0/64 against doctrine. Soviet friendly fire remains
material: 2,866 HP across its candidate exam. These results do not establish robust combat tactics.
The tank candidate inflicts MG damage in 107/128 episodes and HE damage in 94/128. Its native F34
subset is 60 episodes (56 with MG damage, 51 with HE damage); M10T is 68 episodes (51 MG, 43 HE).
The gun assignment comes from the declared seed-parity curriculum, not live scene telemetry.
No checkpoint is promoted. Earlier overnight scores used the pre-correction MG cycling and aiming
rules and are not like-for-like performance evidence.

The following read-only command evaluates those exact policies under the newly compiled rules,
without running evolution, saving weights, or changing the original run. Its default `transfer`
seed set is a new fixed namespace, distinct from original validation and sealed-test seeds.
Use `--seed-set sealed-replay` only for an explicitly labelled replay of the old test.

```sh
target/release/train_combat evaluate artifacts/astra-combat-transfer-v9 \
  --candidate artifacts/astra-combat-training-20260905-parity-v8/candidate \
  --baseline artifacts/astra-combat-training-20260905-parity-v8/initial \
  --run artifacts/astra-combat-training-20260905-parity-v8/run.json
```

The evaluator strictly loads both factions and the auxiliary tank policy, rechecks input hashes,
rejects existing output directories, records current compile-time hashes and native-gun/opponent
metadata, and retains each episode result. It is single-threaded and renderer-free. A wrapper should
record the evaluator executable's SHA-256 when retaining a frozen build receipt.

Post-v8 transfer changes include German five-stage articulated crouch and a standing-only Soviet
fallback while the selected source model is unavailable. The runtime escort update now moves only
its route/objective; shared combat applies the sole physical movement step. Previously the legacy
escort catch-up could add 3.4 m/s before that step, contradicting the trainer's speed limit. The
legacy passive path retains its original catch-up behavior. CPU transfer evaluation covers shared
stance/gun rules; only an actual Game evaluation checks removal of the runtime escort movement.
