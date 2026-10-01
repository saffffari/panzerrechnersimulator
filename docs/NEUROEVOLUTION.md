# Kessel — Neuroevolution Tank AI (plan)

Current runner and October 1 recovery evidence: [AI_RECOVERY_20261001.md](AI_RECOVERY_20261001.md).
The July throughput figures and implementation notes below are historical, not current acceptance results.

Status (2026-07-02): **pipeline smoke test PASSED** — `cargo run --release -- train [gens] [pop]
[secs]` runs the full loop (obs → shared-executor MLP → match → fitness → deterministic ES →
`assets/ai/weights_test.bin` → reload → bit-identical re-eval). `src/ai.rs` + the `enemy_policy`
seam in `game.rs` are live; scripted AI untouched when no policy is installed. Measured ~40
matches/2 s wall sequential (>1000× real-time) — M0 is about *parallel* eval, not raw speed.
Smoke-test simplifications still in place: fixed σ, K=1 match/genome, GPU device required.

**Sequencing decision (2026-07-02):** combat is still being refined/finalized — evolved weights are
throwaway until combat locks, so the FINAL training runs happen after combat lock. Before that, one
**preliminary T-34 run** (§11) validates the methodology end-to-end on real tactics (close + flank).

The original plan follows. This designs how
we evolve neural-net weights for coordinated enemy-tank AI — a **chain of command** and **combined
arms between T-34s and SU-100s** — train them offline in the headless sim, and bake the winning
weights into the shipping game. No implementation here; this is the buildable design + roadmap.

Grounding: the enemy AI today is hand-coded in `src/game.rs` (`enemy_ai` / `enemy_maneuver` /
`enemy_fire`, `Tank::ai_aim`), gated by `src/stealth.rs` (crew-vision cones, platoons of 3, the
5-second spot rule, day/night radii) and the facing-based armor model. The headless
`cargo run -- playtest <secs> <out>` already runs deterministic Tiger-vs-squad matches with combat
telemetry — that harness is the seed of the trainer. Determinism is sacred: the shipping game has
**no `rand` crate** (all randomness is counter-seeded `hash01`), and no heavy ML/autodiff deps.

## 0. Behavior target

What "good" looks like, and what fitness must reward:

- **Combined arms.** SU-100s (100mm, penetrate the Tiger's front at range) hold **overwatch at
  standoff** and pin/snipe the Tiger; T-34s exploit the fixation to **flank to the soft sides/rear**.
  Neither archetype alone should beat a competent Tiger — the win comes from the pairing.
- **Chain of command.** A squad **commander** reads the tactical picture and assigns **roles/sectors**
  (which tank overwatches, which platoon flanks left/right, when to press or fall back). Subordinates
  execute their role. Orders flow down; kills credit the squad.
- **Range discipline.** Tanks respect the range asymmetry ([[kessel-combat-range]]): fight the Tiger
  where they're strong (SU-100 far, T-34 in the flanks), avoid trading frontally at mid-range.

## 1. Architecture overview

Three pieces, one shared forward-pass:

1. **Offline trainer** — a separate cargo target (`--bin train` or a `train` subcommand) that runs the
   sim **headless with NO wgpu** (physics + AI + ballistics + stealth, no rendering), evolves weights
   over many parallel deterministic matches, and writes a weight artifact.
2. **Weight artifact** — a compact, **versioned** file in `assets/ai/` (raw `f32` LE bytes + a small
   header: magic, version, layer dims, genome id, fitness). Gitignored like other assets; regenerated
   by re-training.
3. **Runtime policy** — a **pure-Rust forward pass** (`src/ai/net.rs`) loaded once at startup and
   called from `enemy_ai`, driving the enemies. Falls back to the current hand-coded AI if the file
   is missing or `feature = "scripted-ai"`. Same code path is used by the trainer, so train==ship.

The sim must be **decoupled from the render loop**: factor `Game::update`'s world-stepping (physics,
AI, ballistics, stealth, damage) from the renderer pushes, so the trainer steps `Game` at a fixed
`dt` with no GPU. This refactor is Milestone 0 and is independently useful.

## 2. The policy network

Small fixed-topology MLPs — cheap, deterministic, evolvable. Two nets (see §3): a per-tank
**executor** and a squad **commander**. Both are `[in]→tanh(H)→[out]` with `H≈16–24`.

**Executor observation (egocentric, per tank, ~24–28 dims, all normalized to ~[-1,1]):**
- To-Tiger: `Δx, Δz` (range-normalized ~/300 m), range, `sin/cos` of bearing-to-Tiger vs own heading.
- Own state: speed/`TOP_SPEED`, hull-vs-gun angle, reload fraction, hp/crew/ammo, `EnemyKind` (T34/SU100 one-hot), gun-can-pen-front flag.
- Awareness (from `stealth.rs`): own detection meter, platoon alert one-hot (Unaware/Searching/Engaged), time-since-spot, **LOS-to-Tiger** and **in-cover** flags (from the occluder model), Tiger's estimated facing (front/side/rear exposed to me).
- **Assigned role** (one-hot from the commander): Patrol / FlankL / FlankR / Overwatch / Press / Retreat.
- Nearest-K (K≈2) allies: relative pos + alive/role — **fixed slots by role**, not raw index, so the net is permutation-robust.

**Executor action (continuous, mapped onto existing `Tank` controls):**
`throttle`, `steer`, `gun_traverse` (SU-100: hull-turn to lay, since no turret), `gun_elevate`,
`fire` (threshold), and a scalar `target_confidence`. Outputs are `tanh`; fire is `>0` gated by the
existing `stealth`/reload/LOS checks (the net proposes, the sim's rules dispose — keeps it legal).

**Partial observability:** a tank only observes what its crew would plausibly know — Tiger position is
only fed when detected/alerted (else a stale last-known + a "blind" flag). This is what makes stalking
work against evolved AI, and it's already modeled in `stealth.rs`.

Weight count: executor ≈ `28*20 + 20*7 ≈ 700` weights; commander ≈ `40*24 + 24*12 ≈ 1300`. Tiny —
a genome is a few KB of `f32`.

## 3. Chain of command & combined arms

**Two-tier hierarchy, one shared executor policy.**

- **Commander net** (one per squad): input = pooled squad state (each tank's role/type/hp/alert +
  Tiger's detected position/facing + the range picture). Output = a **role assignment per tank** +
  a coarse **sector/target**. Runs at a low tick (e.g. 2 Hz), so orders are stable, not jittery.
- **Executor net** (shared across all tanks): every tank runs the *same* evolved executor weights,
  conditioned on its **role + type** input. One policy thus plays SU-100 overwatch *and* T-34 flank
  depending on the role byte — this is what makes combined arms emergent rather than scripted, and
  keeps the genome small (shared weights).
- **Chain:** commander → role per tank → executor conditions on role → sim executes. A genome =
  `{commander_weights, executor_weights}` evolved **together** (co-adapted).

**Credit assignment = squad-level fitness** (§4): the whole genome is scored on the squad outcome, so
a selfish tank that hogs a kill while the squad dies scores badly. This is the single most important
lever for getting cooperation instead of six lone wolves.

Why not per-agent RL / comms channels: evolving a shared role-conditioned executor + a small
commander is far simpler to make deterministic and debug than learned message-passing, and it gives
an explicit, inspectable chain of command (we can log the orders). Evolved comms is a later option if
role-conditioning plateaus.

## 4. Evolutionary algorithm

**(μ, λ) Evolution Strategy** with self-adaptive per-weight step size — best fit for optimizing
continuous weight vectors with small populations, no gradients, and deterministic operators.

- Population: **μ = 16** parents, **λ = 80** offspring. Start **(μ,λ)** (only offspring survive) for
  constant pressure; keep **4 elites** carried unchanged to guarantee monotonic best-fitness.
- **Deterministic mutation** (no `rand`): per offspring, `seed = gen*10⁶ + parent*10³ + child`;
  per weight `i`, draw two `hash01(seed ^ f(i))` values → **Box–Muller** `z ~ N(0,1)`; step
  `σ_i·z`. Each weight carries its own `σ_i` mutated multiplicatively (`σ_i·exp(τ·N(0,1))`), so the
  ES adapts step sizes without a covariance matrix. Clamp weights to a sane range.
- Selection: rank the 80 by mean fitness, take top 16, breed. CMA-ES is a stretch goal if plain ES
  stalls; a generational GA (tournament + blend crossover) is the fallback.
- Fully reproducible: `(base_seed, generation)` → identical population. A run is defined by its seed.

## 5. Fitness, opponent, curriculum

**Fitness (per match, squad genome):**
```
f = w_kill · killed_the_Tiger            // primary
  + w_dmg  · Tiger_damage_dealt          // dense shaping (crew/modules KO'd, pens landed)
  + w_flank· flank/rear_pens_landed      // reward the combined-arms payoff
  + w_combo· (SU-100 pinned while a T-34 flank-hit landed within Δt)
  + w_coord· fraction_of_time_platoon_Engaged_with_LOS
  - w_suicide· reckless_losses (tanks lost with no shot landed / frontal rushes)
  - w_camp   · time_spent_out_of_contact (anti-camping)
  - w_survive· Tiger_HP_remaining        // if the Tiger lived
```
Evaluate each genome over **K = 3–5 matches** with different spawn seeds/scenarios; genome fitness =
mean (variance reduction against lucky/unlucky spawns).

**Opponent (the Tiger):** start with the existing scripted `playtest` auto-player (auto-lay, fire when
ready, fight fires). Add a **difficulty scale** (reaction time, aim jitter, aggression, whether it
repositions). Optional later: **co-evolve** a small pool of Tiger policies to avoid overfitting one
opponent — but a scaling scripted Tiger is enough for M1–M4 and far simpler.

**Curriculum (staged, auto-advance on a fitness threshold):**
1. Passive Tiger, 3 T-34s, open ground → learn to approach + land any hit.
2. Add flanking value (front bounces enforced) → learn to reach the sides.
3. Aggressive repositioning Tiger → learn range discipline.
4. Mixed squad (2 SU-100 + 4 T-34), the far-start geometry, cover → learn combined arms + the chain
   of command.

## 6. Headless eval harness

- Reuse and generalize the `playtest` harness: a `run_match(genome, scenario, seed) -> MatchResult`
  that builds a **render-free `Game`**, installs the net policy, steps a fixed `dt` for ≤120 s (early
  terminate on Tiger death or squad wipe), and returns the fitness telemetry.
- **Parallelism:** evaluate the 80 offspring × K matches across `cpu-2` threads (matches are pure
  CPU, independent, deterministic). ~240–400 matches/generation.
- **Metrics to log each generation** (to watch behavior emerge, not just fitness): best/mean fitness,
  Tiger-kill rate, mean engagement range, flank-pen rate, combo-events/match, role distribution over
  time, and a couple of **replay seeds** re-runnable in the real renderer to eyeball behavior.

## 7. Rust architecture

```
src/ai/
  mod.rs      // EnemyPolicy trait; Scripted (current AI) vs Neural
  net.rs      // Mlp { load(bytes), forward(&[f32]) -> Vec<f32> }, pure, no-alloc hot path
  obs.rs      // build_executor_obs(&Game, tank_i), build_commander_obs(&Game, squad)
  roles.rs    // Role enum, order plumbing (commander -> executors)
  evolve.rs   // (mu,lambda)-ES: population, deterministic mutate, select  [train-only]
  fitness.rs  // MatchResult -> f32                                        [train-only]
src/bin/train.rs  // headless trainer: curriculum loop, parallel eval, writes assets/ai/weights_vN.bin
assets/ai/weights_vN.bin  // shipped artifact (gitignored)
```
- **Seam:** `enemy_ai` calls `policy.decide(&game, i)`; `EnemyPolicy` is `Scripted` (today's code,
  the fallback) or `Neural`. The stealth/armor/reload rules still gate everything the net proposes.
- **Determinism + rand-free:** the forward pass is plain `f32` matmul + `tanh`; no RNG at runtime.
  Training's only randomness is `hash01`-Box–Muller. Fix `tanh`/`exp` to a single impl so train==ship
  bit-for-bit-enough (watch cross-platform float drift — pin the math, avoid fast-math).
- The trainer depends on `rayon` (or std threads) — a **dev-only** dep behind the `train` bin, never
  in the shipping game.

## 8. Phased roadmap (each milestone verifiable headlessly)

- **M0 — Sim/render split.** Factor world-stepping out of the render loop so `Game` steps headless
  with no wgpu. Verify: `playtest` still deterministic; a bare step-loop runs at >1000× real-time.
- **M1 — Net drives a tank.** Wire `EnemyPolicy::Neural` with random weights; one net-driven T-34
  fights alongside scripted ones. Verify: no panic, legal actions, deterministic.
- **M2 — Evolve a solo tank.** ES a single T-34 to beat a passive Tiger (curriculum stage 1). Verify:
  best fitness climbs across generations; the evolved tank reliably approaches + lands hits.
- **M3 — Squad coordination.** Shared role-conditioned executor + squad fitness; evolve 3 T-34s to
  flank. Verify: flank-pen rate rises; kills credit the squad; no lone-wolf collapse.
- **M4 — Chain of command + combined arms.** Add the commander net + SU-100 overwatch role; evolve
  the mixed squad on the far-start map. Verify: combo-events/match rise; SU-100s hold range while
  T-34s flank; the pairing beats either archetype alone.
- **M5 — Ship weights.** Freeze a genome, write `weights_v1.bin`, load at runtime, play against a
  human Tiger. Verify: enemies visibly coordinate in-game; scripted fallback still works.

## 9. Risks, failure modes & detection

| Risk | Detection signal (in telemetry) | Mitigation |
|---|---|---|
| Premature convergence | mean≈best fitness, σ collapses early | self-adaptive σ, restart from perturbed elites, keep diversity |
| Sim-exploit / reward hacking | fitness high but replays look degenerate | watch replays each gen; cap dense-shaping weights; add anti-camp/anti-suicide penalties |
| Overfit to one Tiger | wins vs training Tiger, loses vs a held-out one | difficulty scale + a held-out eval Tiger; later co-evolution |
| Lone-wolf collapse (no coordination) | combo-events≈0, one tank does all damage | squad-level credit only; role entropy bonus |
| Non-transitive strategy cycles | fitness oscillates across gens | (μ+λ) elitism, Hall-of-Fame opponents |
| Float non-determinism train↔ship | replays diverge between trainer and game | pin math impls, no fast-math, integration-test a genome in both paths |

## 10. Compute

Tiny nets, CPU-only, one dev box. A match is a few thousand cheap steps; ~300 matches/gen across
~8 threads is seconds-to-low-minutes per generation. M2 (solo) converges in tens of generations
(minutes–hours); M4 (coordinated mixed squad) is the expensive one — plan for overnight runs, but
still comfortably within a single machine. No GPU needed.

## 11. Preliminary T-34 run (pre-combat-lock methodology validation)

Goal: evolve a **T-34-only** executor that reliably **closes and flanks** a live-firing Tiger —
proving the *methodology* (fitness shaping, K-match eval, replay eyeballing) on real tactics before
combat locks. The resulting brain is **explicitly throwaway** (combat is still moving); what we keep
is the hardened trainer and the confidence that the loop produces tactics, not exploits.

**Gate 1 — close the sim exploits first** (evolution finds these faster than any playtester;
all three also improve the shipped game):
1. **Shells test cover.** Occluder AABBs must stop shells in both directions (today they only block
   sight). Without this the run learns to shoot through walls and cover-use can't evolve.
2. **Lose-contact.** `Engaged` currently keeps a perfect permanent fix (`last_known` updates every
   tick, no exit). Add contact decay → stale last-known, or flanking is trivially easy to *counter*
   and re-hiding is untrainable.
3. **Movement collision.** Circle-vs-occluder-AABB pushout + tank-tank separation, or evolved
   flankers path straight through ruins (exploit in training, visibly broken in game).

**Gate 2 — trainer upgrades from smoke-test to prelim grade:**
4. **K = 3 matches/genome, mean fitness** (same seed set within a generation) — kills the seed-luck
   wobble the smoke test showed (per-gen seeds made cross-gen fitness incomparable).
5. **Self-adaptive per-weight σ** (genome carries `w ‖ σ`, σ mutated multiplicatively per §4) —
   replaces the fixed σ=0.15.
6. **Scenario control:** `reset_match` takes a squad spec — this run uses **3× T-34, no SU-100**,
   muster ring 260–320 m.
7. **Live opponent:** the scripted playtest Tiger (auto-lay + fire when ready, stationary) instead
   of the passive hull — a frontal rush now dies, which is what *forces* flanking to evolve.
8. **Flank telemetry in fitness:** count pens by `Arc` (`stat_pens_by_arc: [u32; 3]`) and weight
   `Side`/`Rear` pens ~3× a front pen. Add the doc's anti-suicide term (tanks lost with no shot
   landed) so the answer isn't "trade three hulls for one lucky pen".
9. **Replay eyeball loop:** `train replay <weights> <outdir>` — re-run the best genome's seed in
   the real renderer and write a filmstrip (the playtest machinery already does this). Watch every
   few generations; fitness-says-good + replay-looks-degenerate = reward hack, fix fitness.

**Maps:** the TRAINING ARENA is `world::act(0)` — a 1 km duel bowl on act 1's DEM window: the
hand-tuned ruin dressing + one parametric city block to the north, six occluding hull-down cover
walls on the southern arc, a 22-pine ring. `train`'s `acts` arg takes a comma list ("0,1,2"); a
genome's fitness is the MEAN of one match per map — the obs are egocentric (no absolute
coordinates), so ONE shared policy trains across terrain types and multi-map eval doubles as the
anti-overfit lever. Duels film there by default (`duel out/ [weights] [act]`).

**Run shape:** ~60–150 generations, λ=24, μ=6, K=3, 60 s matches — sequential eval is minutes,
no M0 needed for this. **Acceptance:** fitness climbs then plateaus; side/rear-pen share of kills
rises across the run; the evolved genome beats a random-weights baseline and the *scripted* T-34s
under the same fitness; the filmstrip shows visible arcing out of the Tiger's frontal arc.

**After combat lock:** re-run from scratch (weights are cheap; the trainer is the asset), then
proceed M3 (squad fitness) → M4 (commander + SU-100 combined arms) per §8. Keep a 1-generation
`train 1 4 20` smoke in the verification loop while combat changes land, so the pipeline never
silently rots.

## 11b. The calibrated trainer (every run's lessons, distilled — 2026-07-02)

What five runs taught, now hard-coded in `run_train`:
- **K=5 seeds in 8-generation windows** (fixed seeds bred choreography; per-gen rotation bred
  amnesia; windows compound without memorizing).
- **The exam is the only lifetime metric**: every generation's winner sits a held-out, ALWAYS
  full-difficulty exam; only exam records ship; classroom scores are window-local.
- **The staged opponent** (the academy-run lesson — freshmen vs the live Tiger froze at gen 8):
  stage 1 MARKSMANSHIP (passive hull, graduate at classroom form 420) → stage 2 UNDER FIRE
  (11 s crew, graduate 380) → stage 3 THE REAL THING (6.5 s crew, runs to convergence).
  Graduation = mean classroom best over one full seed window ≥ threshold.
- **PATIENCE=150 early stop, final stage only**; 2 immigrants/gen; elite carry; multi-map
  (arena + city) classroom AND exam.
Verified in sanity: stage-1 form 425 → graduation at gen 7 → stage-2 classroom drops to ~180
(fresh gradient) and climbs again, exam line monotone throughout.

## 12. THE ACADEMY — the full curriculum (supersedes §5's four stages)

The staged curriculum, taken seriously as a syllabus (2026-07-02). Rationale: ES needs **fitness
variance inside every population** — when the whole class scores zero on "kill the Tiger",
selection is a coin flip. Fine early rungs grade every random-weights cadet on *something*, so
there is always a gradient. The deeper payoff: the early courses train the **order-execution
primitives the chain of command needs** — a cadet that reliably converts an objective input into
motion/fire is exactly the subordinate the commander net (M4) must be able to command. The
executor's "assigned role/sector" obs slot doubles as the course-objective input from day one, so
Academy graduates already speak the command interface.

One shared executor genome attends all courses; only the scenario + fitness change per stage.
Stages auto-advance on a graduation threshold (§5), and **exams are cumulative**: once a stage is
passed, later generations keep a small sampled mix of earlier-stage matches in their fitness mean —
evolution forgets skills it is no longer graded on (catastrophic forgetting is real in ES too).

| # | Course | Scenario | Fitness core | Needs |
|---|--------|----------|--------------|-------|
| A | **Movement** | drive spawn → point B, per-map | -time to arrive, arrival bonus | objective obs slot |
| B | **Cones** | waypoint chain threaded between obstacles | A + proximity/contact penalty per cone | movement collision (the Gate-1 fix pulls double duty) |
| C | **Gunnery, stationary** | parked hull targets, no return fire | hits + pens per minute | target = god-mode player hull; stat counters exist |
| D | **Gunnery, moving targets** | the auto-Tiger drives a scripted circuit, god mode | C, lead-solving emerges | scripted Tiger driver |
| E | **Fire on the move** | D, but shots only score while own speed > threshold | C + the speed gate | fitness term only |
| F | **Sparring: 1v1 / 2v2 / 3v3, same make** | T-34 vs T-34 melees | win + damage-efficiency | **enemy-vs-enemy ballistics** (needed for act 3 anyway) |
| G | **Platoons vs the Tiger** | game-realistic platoon sizes (2-3, then the mixed 6) vs one Tiger I | §5/§11 combat fitness (flank-weighted pens, anti-suicide) | the paced auto-Tiger |

Course-F caveat (the negative-transfer warning, kept honest): symmetric duels reward frontal
brawling, which stage G punishes. F stays in the syllabus for what DOES transfer — aim under fire,
cover use, target selection, mutual support in 2v2/3v3 — but G must dominate total selection
pressure (more matches, higher weight), and F only enters once enemy-vs-enemy ballistics lands.

**The Tiger track.** The opponent trains separately, "to match the game": the paced auto-crew is
the difficulty dial (crew cycle, acquisition delay, dispersion, repositioning on/off) and climbs
its own ladder against the cadet population — passive → slow crew (~10 s) → the 6.5 s crew →
repositioning/hull-down. Later options, in order of ambition: a held-out eval Tiger (never trained
against — the overfit detector), Hall-of-Fame frozen champions as sparring partners once F exists,
and full co-evolution of a Tiger policy only if the scripted dial plateaus (§9's cycling risk
applies — elitism + Hall-of-Fame are the guard rails).

## 13. The shipped brains — final-run spec (post-combat-lock)

The environment sets the intelligence ceiling (§12 rationale), so the FINAL runs are an
environment-design problem: train on the FULL act maps (arena = marksmanship; full maps =
maneuvers — patrol, search, picket, cover-use at operational scale, the stealth counterplay the
player actually experiences), across scenario variety, against the full opponent roster — single
Tigers, TIGER GROUPS (act 3 ships platoon-vs-armada; untested = uncovered), and the other German
armor as ballistics stat blocks (Panther, Pz III, Tiger II — meshes irrelevant to training).

**Difficulty tiers = training checkpoints (free):** save the champion every N generations; ship
early checkpoints as green conscript platoons, the final champion as GUARDS units late in the run
— the veterans literally are more evolved. Seed variation across runs yields doctrine
personalities (aggressive vs cautious platoons) at no cost.

**Generalization guarantee:** egocentric obs + no memory ⇒ brains cannot memorize layouts — they
learn "how to fight among buildings", never "where the buildings are". Training on shipped maps
does not overfit to them; hand-tuned Blender maps arrive covered.

---

**Bottom line:** a `(μ,λ)`-ES over small shared-executor + commander MLPs, scored on **squad**
fitness in the existing headless harness, is the simplest path to *emergent* combined-arms tank AI
that stays fully deterministic and `rand`-free in the shipped game. Build M0 (the sim/render split)
first — it's the enabler and useful on its own.
