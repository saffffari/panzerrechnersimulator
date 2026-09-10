# Panzerrechnersystem — ML Tank AI (SU-100 vs Tiger self-play) — Plan

Status: **R&D plan, not started.** Ambitious, "maybe works maybe doesn't" — but the sim is already a
strong RL substrate (real ballistics, armor, kill/death, and the **AI can use the same control inputs
as the player**). Recorded 2026-06-30.

## Goal
An **SU-100 enemy** driven by a learned policy trained to **destroy Tigers**. Simultaneously train a
**Tiger policy** (NOT shipped) purely as a learning sparring partner, so the SU-100 faces a real,
improving opponent instead of a dumb script. This is **competitive self-play / adversarial
co-evolution**.

> Historical note: the SU-100 (100 mm, 1944) post-dates the 42–43 setting and shreds a Tiger
> frontally. Fine as an ML antagonist (a tough opponent is good for training); for period accuracy
> the closest turretless TD is the **SU-85** (Aug 1943). See enemy-roster notes in TIGER_I_SPEC.md.

## Approach — use self-play RL (PPO), not pure neuroevolution
- **Recommended: PPO + competitive self-play** (the AlphaStar / OpenAI-Five recipe). Far more
  sample-efficient than evolution for continuous control, and self-play is the right frame for the
  "train both sides" idea.
- **Neuroevolution (GA / ES / NEAT)** — the literal "evolutionary" path — also works and parallelizes
  trivially (evaluate a population across many matches, select + mutate). But it's sample-hungry.
  Start with PPO; reach for ES only for the evolutionary flavor or if PPO stalls.
- **Opponent pool / league:** keep snapshots of past Tiger & SU-100 policies and sample opponents
  from the pool — prevents strategy cycling / catastrophic forgetting.

## UE 5.8 tooling — gets ~70–80% of the way there
- **Learning Agents** (Epic plugin, matured by 5.8) — THE spine. Provides the observation/action
  interfaces, the **PPO trainer** (bridges to an external PyTorch process), critic, imitation
  learning, and parallel data collection. Enable it + its dependencies.
- **NNE (Neural Network Engine)** — runtime inference of the trained model (ONNX) in the shipped game
  (cheap, no Python at runtime).
- **Headless + accelerated/fixed-timestep PIE** — run many fast matches for sample throughput.
- **NOT built-in (ours to build):** reward design, curriculum, the opponent-pool/league, any GA loop.

## Action space (= the player's control surface — elegant + fair)
The agent drives with the SAME inputs a human has (the controls we already built):
- **Tiger (turreted):** throttle [-1,1], steer [-1,1], turret-traverse rate [-1,1], gun-elevation
  rate [-1,1] (continuous) + fire (discrete) [+ optional shell-select].
- **SU-100 (casemate TD — NO turret):** drops turret-traverse; it **aims by pointing the hull**
  (+ ~±8° gun traverse, elevation). Learns a different doctrine: ambush, hull-aim, hull-down,
  point-and-shoot. A genuinely distinct agent from the Tiger.

## Observation space — keep it EGO-CENTRIC (relative bearings/ranges generalize; world coords don't)
- **Own state:** speed, heading, turret yaw (rel hull), gun pitch, reload/can-fire, ammo type,
  hull/track/module status (which capabilities alive), is-moving (can't-fire-on-move).
- **Enemy state (partial obs — fog matters):** relative range, bearing (own frame), **aspect angle**
  (so it learns to seek the flank/rear), enemy turret facing (is its gun on me?), **LOS/visibility
  through fog**, last-known position when occluded.
- **Terrain/cover:** a ring of raycast "whiskers" for obstacles/cover; optionally a small local
  heightmap patch.
- **Tactical:** own armor-facing vs enemy gun (am I showing my weak side?), enemy weak-side exposure.

## Reward design (the hard part)
- **Sparse terminal:** +1 kill enemy, −1 die.
- **Dense shaping (to bootstrap):** reward penetrating hits, achieving flank/rear aspect, holding LOS
  while unseen, keeping strong armor toward the enemy gun; penalize exposing the weak side, firing on
  the move, and taking hits.
- **SU-100 specifically:** "penetrate the Tiger" is only achievable by flanking → the reward
  **teaches Soviet doctrine for free** (don't duel the front; flank, immobilize, swarm).
- **Curriculum:** start vs a stationary/scripted Tiger to validate the loop → ramp to co-evolving
  self-play with the opponent pool.

## Build sequence (milestone, multi-session)
1. **Prereq:** Tiger wired as the drivable BP pawn (so agents drive the real vehicle, same inputs).
2. Enable **Learning Agents** + **NNE** plugins.
3. Wrap the tank as a Learning Agents agent: define observation struct + action struct (map to the
   existing `UInputAction`s / component drivers), reward function, episode reset.
4. **Training arena** level: small bounded map, spawn SU-100 + Tiger agents, randomized starts,
   episodic resets, accelerated/headless.
5. Validate loop: SU-100 (PPO) vs scripted/curriculum Tiger.
6. Switch to **self-play** (both learning) + opponent pool.
7. Train headless/accelerated → checkpoint → evaluate → iterate reward.
8. Bake winning policy to **ONNX → NNE** inference → drop into the in-game SU-100 enemy.

## Honest risk
Competitive self-play with continuous control + partial observability is genuinely hard and
compute-hungry; it may need a lot of tuning and might not converge to "good tankers." But the
foundation (shared control inputs, real ballistics/armor, win/lose conditions) is the hard 80%
already in place, and Learning Agents removes most of the plumbing. Worth the experiment.

See: [[tank-sim-crew]] (damage model the reward reads), CONTROLS.md (the action surface),
TIGER_I_SPEC.md (vehicle params + enemy roster).
