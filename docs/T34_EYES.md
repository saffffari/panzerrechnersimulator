# T-34 crews with eyes, memory and orders (2026-11-02, in progress)

Branch `claude/t34-eyes-20261102`. The owner preferred the evolved T-34 crews (July academy
policy) to the scripted doctrine in a Canyon 1v5, then asked for them to be "vastly improved":
more aggressive, working together, hiding from the Tiger and outflanking it, and later trained on
missions with objectives. This note records what the branch changes, what is measured so far and
what is still open. **Training is unfinished; the default T-34 weights are unchanged.**

## The July net's limits, seen in the receipts

`canyon-ai-run` seed 29, five T-34s, no 45 mm guns, the July net
(`artifacts/t34-eyes-20261102/before/`):

- One tank drove 4.7 m in 60 s (55.7 s stuck): it reverses into ruins behind its start and stays
  there. In the open-ended run (388 s, the T-34s kill the Tiger) the same tank is stuck 220 s.
- A blind crew gets all-zero inputs. It reverses and turns in circles even while the doctrine's
  perception holds a fix on the Tiger (a seen muzzle flash, a heard engine).

## What changed

- **A second policy format, `KSL2`**: 31 inputs x 27 hidden x 3 actions (`src/ai.rs`). Inputs
  0-13 are the July inputs unchanged, so a July `KSL1` net loaded as a T-34 policy
  (`Mlp::load_t34`) reads the first 14 slots and drives exactly as before. Seats that read the
  14-input observation (the AI Tiger, the platoon wingmen) load with `Mlp::load`, which refuses a
  `KSL2` net by name; a header whose dimensions do not match its magic is refused as a mismatched
  genome; a net handed a narrower observation panics with both widths. Unit tests:
  `cargo test --release --bin panzerrechnersystem -- ai::tests`.
- **The new inputs** (`Game::executor_obs`):
  - 14-17 room to drive toward the left front, ahead, the right front and astern, out to 24 m in
    3 m steps, judged with `street_way_clear`, the footprint test that clips the T-34's motion on
    the Canyon and in the streets; refreshed every 6 frames per tank (`enemy_room`);
  - 18 grade ahead; 19 how long the crew has driven without moving (`enemy_stall_s`);
  - 20-23 where it would look for the Tiger: in sight the Tiger, else the winter encounter's fix,
    sighting or heard area (`WinterEncounter::memory_of_target`), or on open ground the platoon's
    alerted last-known position;
  - 24-25 the nearest mate's angle around that Tiger relative to its own (a pincer reads cos -1);
    26 mates with the Tiger in their own sight;
  - 27-30 orders: the firing post the platoon's armour doctrine assigns it (fix and flank,
    `WinterEncounter::assigned_post`, stance +1), else the corner of its platoon's beat
    (`WinterEncounter::patrol_corner`, stance -1). Mission objectives can use the same slots.
- **Generation zero** (`Mlp::t34_reflex_seed`, `t34-seed OUT [JULY]`): the July net widened with
  zero weights, plus hand-wired units that throttle back with no room ahead, push forward with no
  room astern, steer to the freer side, drive toward whichever end has room when stalled, and
  steer and drive toward the remembered Tiger or the orders when blind. Each unit's output bias is
  offset so that on open ground with the Tiger in sight the seed answers as the July net
  (unit-tested).
- **Training on the Canyon**: `t34-canyon-eval SEEDS SECONDS` plays one policy on the opening
  deployment with the `canyon-ai-run` pilot as the Tiger and prints the episode's telemetry plus
  closing distance, pinned time, time exposed in the Tiger's frontal 60 degrees, time on its
  flanks, how far round its bow the tanks stood, and the pincer angle. `tools/t34_canyon_es.py`
  evolves from generation zero, one fresh process per episode (re-fielding inside one process
  drifts from a fresh run), small mutations on the July weights and larger ones on the new units,
  a fitness written in its header (hits and penetrations, flank and rear shots, contact, closing,
  being off the bow, minus exposure to the Tiger's front, pinned time, throttle see-sawing and
  losses, minus drift from the July net's answers on open ground). Right before every episode it
  checks that the owner's game or editor is not live and waits while it is; otherwise it runs at
  most two episodes at once.
- `KESSEL_T34_WEIGHTS=FILE` picks another T-34 policy wherever a winter encounter is fielded
  (`install_t34_brain`), logged with its width. `KESSEL_T34_OBS_DBG=SLOT` prints one tank's
  observation and action twice a second.
- Unchanged: the gun lay, dispersion, first-shot delay, reload, the fire gate (`a[2] > 0` still has
  to meet the lay, readiness and line-of-fire gates), the doctrine, the default weights.

## Measured so far

Episodes are deterministic per policy and seed (a rerun matched the trainer's cache exactly).

Generation zero against the July net, seed 29, 120 s: the stuck tank drove 134 m instead of
4.7 m; the crews made contact (141-206 tank-s against 0) and fired (1-9 shots against 0);
total stuck time 85 tank-s against 118.

Most of the remaining `stuck_s` is not walls: the receipt counts every throttle reversal through
zero, and blind crews see-saw between the July units' reverse and the reflexes' advance. The
trainer measures **pinned** time (driving without moving for over 1.5 s) separately; generation
zero on seed 3 is pinned 0 s of 70 stuck tank-s.

Evolution (run `es3`, population 8, three training seeds per generation, 90 s episodes; held-out
receipt seeds 29, 7, 211 never trained on), best fitness per generation: -53.6, -43.7, +1.5,
+28.8. The generation-3 champion hit the Tiger on all three seeds of its generation, penetrated
and killed it on one, and lost two T-34s over the three episodes. No crew has yet worked off the
Tiger's bow within 500 m; the off-the-bow measure was added at generation 4 to give evolution a
gradient there.

## How to finish

```sh
tools/t34_overnight.sh 12
```

resumes from the last finished generation, runs twelve more, then writes before/after receipts
(seeds 29, 7, 211 at 60 s and 180 s, one process at a time) and the arena gauntlet for the
champion and the July net into `artifacts/t34-eyes-20261102/`. Then: read the receipts, film or
play a round with `KESSEL_T34_WEIGHTS=<champion>`, install it as
`assets/ai/weights_t34_eyes.bin` in both asset roots, and only then make it the default.

## Open

- Flanking and the pincer are not learned yet.
- Mission scenarios (attack a held position, defend a choke, a meeting engagement) on the orders
  inputs: not built.
- The 45 mm guns and riflemen were off in every episode.
