# Kessel — PLATOON COMMAND & ENEMY AI (design, deferred build)

> Status: design pinned 2026-07-01; **not built** — this is the Act-3 milestone and the enemy-AI
> throughline. Extends `docs/NEUROEVOLUTION.md`. See memories kessel-platoon-command,
> kessel-neuroevolution, kessel-3act-narrative.

The same technology drives both the **friendly platoon you command** (Act 3) and the **enemy tanks
you fight** (all acts) — it's one AI stack pointed in two directions. This doc is the plan for both.

## 1. Two AI tiers (this is the key architectural split)

- **Tier A — the combat executor (many, small, evolved).** A tiny policy net (hundreds–few-thousand
  params) that drives ONE tank on the same control surface as the player (`CONTROLS.md`): throttle,
  steer, turret, gun, fire, use-cover. Input is **fair, sensed** state — what that tank could
  actually see/hear (ray-cast LOS, the acoustic/spotting cues), never god-mode truth. Thousands of
  these run in headless self-play. **Enemies use these. Friendly platoon tanks execute your orders
  with these.**
- **Tier B — the commander brain (few, large, language-capable).** Understands intent and issues
  orders to Tier-A executors. For the **friendly platoon**, this is where the player's natural-
  language radio orders land. Kept to **2 friendly commander brains → a team of 3** (you + 2)
  deliberately, so each can be **bigger** (memory headroom). Enemy formations get a Tier-B too (the
  "chain of command" from NEUROEVOLUTION.md) so the armada maneuvers as a force, not a mob.

## 2. The radio command interface (friendly, Act 3)

Inside the tank, a **chat interface**. You type what you'd actually say on the net — free text:

- `"Attack the building at the church, grid two-four."` → target-and-engage on a map position.
- `"Follow me."` / `"Stop here."` / `"Hold this treeline."` → formation / posture.
- `"TANK APPROACHING ON THE RIGHT!"` → threat call: bias attention/facing to that bearing.
- `"Fall back to the ridge, cover my flank."` → retreat + overwatch role.

Pipeline: **free text → an LLM (or a functionally-equivalent intent parser) → a small structured
order schema** (verb + target + modifiers, e.g. `{engage, pos, priority}`, `{move, formation,
speed}`, `{threat, bearing}`). The Tier-A executors carry the order out **to the best of their
ability** given what they can sense, and **radio back** ("Wilco", "Negative, no shot", "Taking fire,
breaking left"). Orders are advisory targets, not scripted paths — the executor still fights.

Why an LLM only at Tier B: NL understanding is the one place a big model earns its cost; the
moment-to-moment driving/gunnery is a reflex policy (Tier A) that must run cheap and many.

## 3. Training: neuroevolution over the headless sim — my recommendation

The engine is an **ideal RL/neuroevolution substrate** and we should lean into it:
- **Headless self-play is just running the sim struct with no renderer** (ENGINE.md §4): the 24-thread
  box does millions of steps/sec; each genome = one headless match. Embarrassingly parallel.
- **Neuroevolution (ES/GA over policy weights) beats gradient RL as the starting point here:** no
  differentiable reward, no backprop-through-sim, no PPO plumbing; robust to the **sparse, delayed**
  reward of tank combat (you win or lose a fight); trivially parallel; small nets suffice. Start
  simple (mutate-select / CMA-ES), add NEAT-style structure only if needed. Keep PPO fine-tuning as a
  later option, not a dependency.
- **Competitive CO-EVOLUTION — evolve the Tiger too.** Both sides evolve *against each other*: the
  player-side Tiger runs its own evolving policy so the Soviet population always trains against a
  **real, improving Tiger**, never a static punching bag, and the Tiger evolves right back as the
  Soviets get harder. This mutual arms race is what produces genuinely sharp tactics on both sides
  (and the evolved Tiger doubles as a demo/autopilot and as the yardstick opponent). Guard against
  the classic coev failure modes (cycling, forgetting) with a **hall-of-fame / league**: each
  generation fights a growing pool of past champions from *both* sides so strategies don't collapse
  into one gimmick.
- **Fitness = shaped combat outcome:** damage dealt, survival, objective (reach goal / kill the boss /
  keep the platoon alive), penalize friendly fire and reckless exposure. Crucially, the **ballistics
  model teaches good behavior for free** — because flank/rear penetration is modeled, "flank the
  Tiger, don't slug it frontally" *emerges* from fitness rather than being hand-coded.
- **Determinism caveat:** the sim is counter-hash deterministic (no `rand`) — great for reproducible
  eval, but training must **vary the scenario seed across the population/generations** or policies
  overfit one map.

## 4. Bosses are BESPOKE evolved models — one per duel

The Act-1 **KV-2** and Act-2 **ISU-152** bosses (`world::HeavyTank`) are their **own specially-evolved
policies, trained purely for their specific duel** — the KV-2 evolved for the city fight vs the
player-Tiger, the ISU-152 evolved for the rendezvous fight — not a generic combat net and not a
behavior tree. Training each boss in its exact scenario (that map, that matchup, that objective)
against the co-evolving Tiger produces a genuinely hard, setting-specific opponent that *feels*
purpose-built. Each ships as a frozen champion genome for that encounter. (Readability/drama — the
telegraphed 152 mm reload window — comes from tuning the boss's gun-handling stats and the arena, not
from dumbing the policy down.) Evolved Tier-A also drives the masses: swarming T-34s, the Act-3
armada, and the friendly platoon executors.

**Greenlit:** neuroevolution is a committed direction (not a maybe). The co-evolving Tiger (§3) and
the per-duel evolved bosses (§4) are part of the spec. The one hard dependency is a stable headless
self-play harness (the sim looped with no renderer) — it comes after the world/acts + the ballistics
impact-event model exist, because the fitness function needs real combat resolution to score.

## 5. Build order (all deferred until the world/acts exist)

1. Headless self-play harness: sim loop, no renderer, scenario seeding, match → fitness.
2. Tier-A policy net + neuroevolution loop; evolve a basic T-34 that flanks. (Validates the substrate.)
3. League self-play + curriculum → the Act-3 armada behavior.
4. Tier-B enemy chain-of-command (formation maneuver).
5. Friendly Tier-A executors + the order schema (structured orders → executor).
6. The NL front-end (LLM/parser → order schema) + the radio chat UI + wilco/negative replies.
7. Authored KV-2 / ISU-152 boss behaviors.
