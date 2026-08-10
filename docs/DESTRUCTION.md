# DESTRUCTION.md — engine-native destruction spec (locked 2026-07-06)

**This document is the current authority for destruction on the Rust/wgpu engine.** It supersedes
the engine half of `DESTRUCTION_ARCHITECTURE.md` (UE 5.8 Chaos design, abandoned attempt); that
doc remains useful only for its vision language. Destruction v1 (shelled walls collapse to rubble,
commit `f124d2b`) shipped and is the foundation this builds on — same `Destructible` registry,
same `damage_structure` → `collapse_structure` pipeline, same pooled FX and occluder downgrade.

## 0. Thesis

Reality first: WWII tank guns treated buildings as **concealment, not cover**. An 88 PzGr.39
(~100mm penetration at 1,000 m) passes through a timber izba — both walls — losing maybe 20mm of
equivalent penetration, doesn't even fuze on the soft material, and kills the 45mm-side T-34
behind it. Brick degrades a round (cap stripping, yaw); stone and concrete stop it. Tanks drove
through wooden buildings routinely; brick walls yielded to a heavy tank at a grind; bunkers did not.

The system that encodes this: **penetration budgets + material classes + chunk removal +
ballistic-freeze debris + momentum crush.** NO external physics crate — hand-rolled chunk
ballistics against the heightfield, deterministic via the counter-hash pattern. Chunk-vs-chunk
collision buys nothing visible through the PS1 crush filter.

## 1. Shell model — two ammo natures

Player carries both (historical Tiger loadout); switching shells is a real decision, and later
becomes the loader-station interaction in the embodied interior.

| | PzGr (AP) | SprGr (HE) |
|---|---|---|
| vs walls | punches a hole (cylinder chunk removal), **keeps going** | detonates on **first** surface hit |
| structural damage | light (hole only, ~15 HP) | heavy (radial, ~75 HP + chunk fling in radius) |
| vs armor | remaining pen budget vs facing armor | negligible (later: track/external damage in blast) |
| side effects | +hashed deflection per wall passed | sympathetic dust off structures in 2× radius |
| fuzing | does not fuze on soft walls (historical) | impact fuze, always |

**Penetration budgets (game-tuned, reality-shaped):**

| gun | budget @ 0 m | falloff |
|---|---|---|
| 8.8 cm KwK 36 PzGr | 120 mm | −8 mm / 500 m |
| 7.62 cm F-34 PzGr | 85 mm | −10 mm / 500 m |

**Effective armor (simplified per facing):**

| tank | front | side | rear |
|---|---|---|---|
| Tiger | 105 | 80 | 80 |
| T-34 | 85 | 50 | 40 |

Consequences (all emergent, none scripted): 88 kills a T-34 through a hut at any range and
through one brick wall at short-to-mid range. 76 through the same hut bounces off a Tiger's front
at any range but kills the flank inside ~250 m. Huts hide T-34s from the player's *eyes*, not the
player's *gun*; huts barely protect anyone from an 88 and protect the Tiger from 76s almost
entirely. This derives the combat-range asymmetry pillar instead of scripting it.

## 2. Material classes

Every destructible carries a `MaterialClass` that answers all three questions — shell response,
crush response, debris look/sound:

| class | examples | HP | ap_cost_mm | deflect | crush tier | debris |
|---|---|---|---|---|---|---|
| Scatter | fence, telegraph pole, cart, scatter props | 5 | 2 | 0 | T0 | splinters |
| Timber | izba wall, barn, shed, plank barricade | 40 | 12 | 5 mrad | T1 | planks + splinters |
| SmallTree | fir/sapling < ~0.25 m trunk | 10 | 3 | 0 | T1 | needles puff, hinge-fall |
| Brick | city wall, house facade (the current 44) | 50–155 (existing size scaling) | 35 | 15 mrad | T2 | brick chunks |
| Stone | heavy masonry, house corner, church wall | 200 | 70 | 25 mrad | T3 | slab chunks |
| BigTree | mature trunk ≥ ~0.4 m | 60 | 15 | 8 mrad | T3 | stump + hinge-fall top |
| Bunker | poured concrete, massive rubble mounds | ∞ (never collapses) | stops all | — | T4 | chip/spall FX only |

Log barricades = Timber with ap_cost 25. Knee-high collapse rubble sits below the shot line and
is ignored by the pen ray.

## 3. The penetration ray (replaces nearest-hit for AP)

```
fn resolve_ap_shot(origin, dir, budget_mm):
    hits = all destructible AABB intersections along ray, sorted by t   // not just nearest
    interleave: enemy tank spheres, terrain, sorted into the same t-order
    for hit in hits:
        Terrain        -> impact FX, done
        Tank           -> if budget_mm > armor(facing): kill/damage; else ricochet FX; done
        Destructible d ->
            apply hole damage (cylinder chunk removal at hit point, ~15 HP)
            budget_mm -= d.class.ap_cost_mm
            if budget_mm <= 0: shell dies in wall (bigger bite, dust), done
            dir = deflect(dir, hash(d.seed, shot_counter) * d.class.deflect)
            continue
```

### 3.1 Wall-transit ballistics (piecewise ray)

Shots already resolve deterministically at fire time, so barrier transit needs no projectile
sim — the trajectory becomes a **piecewise ray**: each wall transit mutates the shell state
(velocity, budget, direction, disruption), then drop and time-of-flight are recomputed for the
next segment. Real terminal-ballistics effects, one small table:

| class | vel_loss | deflect | disrupt | fuze on transit |
|---|---|---|---|---|
| Timber | 3% | 5 mrad | 0.0 (clean pass, historical) | never |
| Brick | 12% | 15 mrad | 0.3 | rare |
| Stone | 25% | 25 mrad | 0.6 | sometimes — bursts inside the wall |
| Earth/rubble berm | 30% | 20 mrad | 0.5 | never |

- `vel_loss` feeds both remaining penetration and the next segment's drop/ToF.
- `deflect` is a hashed angular kick — miss distance at the target grows with the wall-to-target
  gap *for free*, so a T-34 hugging the far side of a hut dies reliably while one 50 m behind it
  gets a wandering, degraded round. Depth behind concealment becomes a real positioning decision.
- `disrupt` models yaw + cap stripping (green logs and packed earth yawed rounds badly; APCBC
  caps strip on masonry): effective pen at the armor hit is `remaining_budget × (1 − disrupt)`,
  and ricochet/shatter chance scales up with it. Hits the budget-poor 76mm much harder than the
  88 — deepens the range-asymmetry pillar in the historically correct direction.
- **Tracer feedback**: the tracer visibly kinks and slows at the wall — the player watches the
  round punch through, deflect, and connect (or veer). Teaches the mechanic with no UI.
- Later, Absitzen: masonry transit sprays spall on the far side — walls being serviced by tank
  guns are lethal to stand behind on foot.

HE keeps nearest-hit semantics: detonate at first surface, then radial `apply_damage` to every
destructible whose AABB intersects the blast sphere (structural damage scaled by distance),
chunk fling impulses radial from the burst point, sympathetic dust in 2× radius.

One funnel for everything: `apply_damage(point, dir, DamageKind::{Ap{budget}, He{radius, power},
Crush{power}})`. Track crush is just the third kind.

## 4. Crush — momentum vs class tier

Each tank has a `crush_power` (Tiger 57 t → 3, T-34 26 t → 2). Each class tier gates on power
and speed:

- **T0** — anything crushes at any speed. A bump, a crunch, no slowdown.
- **T1** — instant collapse on contact above ~8 km/h; below that a 0.5 s push-grind. Slowdown to ~60%.
- **T2** — **the grind**: speed clamps to ~30%, crush damage/s flows into the existing HP pool,
  wall gives way after ~1–2 s, tank surges through. This pause-then-give is the signature moment.
- **T3** — crush_power 3 only (Tiger): crawl-speed grind ~3 s. Power 2 (T-34): hard stop.
- **T4** — hard stop for everyone. Chip FX + clang; ramming above ~12 km/h will feed minor track
  damage once the vehicle damage model exists.

A tank one power class lighter treats every tier as one step harder. Enemy T-34s use the same
funnel — an AI T-34 bursting through a barn wall at close range costs nothing extra to support
mechanically (see §7).

**Feel (where the satisfaction lives — do not cut):** pitch impulse on contact; hull heave riding
over the fresh rubble (briefly raise effective ground height); camera shake scaled to event
magnitude; masonry dust washing over the vision ports; audio chain = contact crunch → grind loop
while pushing → collapse boom → debris patter tail. Parametric by material class and chunk count,
per the sound pillar.

## 5. Trees

A falling tree is **one hinge rotation about a base pivot** in the impulse direction, slight
overshoot, then it stays down forever as a low obstacle. No fracture, no solver.

- **SmallTree**: snaps from track contact (T1) or any shell hit or HE within radius. Fixes
  "blocked by a small fir."
- **BigTree**: AP passes through (cost 15, splinter puff). HE snaps the trunk → stump stays,
  top hinge-falls. Tiger can shove one over at a T3 crawl; T-34s must go around.
- Audio: crack → groan → whoosh → thud. Highest satisfaction-per-line-of-code in this doc.

## 6. Chunks & debris

- **Act-1 procedural walls are generated AS chunk grids** — brick-course rows of jittered boxes.
  Rendered as one merged mesh while pristine; swapped to per-chunk on first damage. No mesh
  fracturing ever happens at runtime.
- **Generic chunk pool**: ~8 shared chunk shapes (brick, half-brick, slab, plank, splinter),
  reused with varied scale/tint per material. Pure PSX asset-reuse; no unique fracture geometry.
- **Support heuristic, not stress sim**: each chunk stores one bit — is the chunk below me alive?
  Kill enough of the bottom course and the wall above cascades. Shoot-the-base becomes a tactic.
- **Ballistic debris**: a dead chunk gets a hashed impulse (radial for HE, ploughed
  forward-and-low for crush, along-ray for AP exit), integrates gravity, tumbles, bounces once
  off `height_at()` with damping, then **freezes forever** as persistent rubble. Frozen chunks
  are static meshes; overlap in a heap is correct-looking, not a bug.
- **Budgets**: airborne chunk cap ~64 round-robin (extends the existing 12-debris pool); only
  damaged buildings hold live per-chunk state; pre-allocate chunk slots at `Game::build` (the
  `Slots` index scheme requires it — same reason rubble heaps are pre-allocated today).
- **Blender-act buildings**: whole-collapse + a generic chunk cloud matched to footprint and
  palette at collapse. Extending the bake to export real chunk sets is deferred until it visibly
  needs it (through the crush filter it likely never does).
- Determinism throughout: fracture layout from the structure `seed`; impulses from
  `hash(seed, chunk_index, event_counter)`. No `rand`.

## 7. AI and the destructible world (deferred — hooks locked now)

- **Perception: already done.** Stealth/LOS occluders downgrade on collapse (v1), so enemies
  correctly gain sight when cover falls.
- **Concealment ≠ cover**: v1 behavior — T-34s treat huts as safety — stays; it's the player's
  knowledge edge. Later: veteran enemies blind-fire AP through soft walls at last-known player
  position (historical, terrifying, cheap — reuses the pen ray).
- **AI crush pathing**: crushable classes become a movement *cost*, not a wall, in enemy
  steering — a flanking T-34 bursts through timber instead of pathing around it.
- **Trainer arenas MUST include destructibles** (walls, huts, trees) once slices 1–3 land, or
  evolved policies are blind to all of it. The trainer is the asset; arena gen is where this
  knowledge enters the weights.

## 8. Verification (headless-first, per house style)

- `cargo run -- fire` variant: AP through a hut into a T-34 → `overpen_{muzzle,wallhole,kill}.png`.
- `cargo run -- drive` variant: Tiger through a timber hut → before/grind/after PNGs; assert
  position crossed the wall plane and wall collapsed.
- Tree snap demo: drive over a fir, screenshot the hinge-fall mid-frame.
- Unit tests: pen-budget arithmetic (hut pass/kill, brick marginal, bunker stop, 76-vs-Tiger-front
  bounce), sorted multi-hit ray ordering, transit state mutation (vel/deflect/disrupt applied per
  segment; scatter grows with wall-to-target gap), crush tier gating by power/speed,
  support-bit cascade.

## 9. Slice order

1. **Track crush + material classes** — tiers, the grind, plough debris, hull heave, audio chain.
   **SHIPPED 2026-07-06**: `Material` classes on every destructible, the bow-contact crush gate
   (`resolve_tank_crush`), the brick grind (speed clamp + crush DPS into the shared HP pool →
   `collapse_structure` → surge), timber burst-at-speed and bunker hard-stop paths (unit-tested;
   act-1 spawners for those classes arrive with their world classes), camera crush-shudder,
   throttled plough debris/dust + contact crunch. Verified by `cargo run -- crush <prefix>`
   (asserts the hull crosses the felled wall line). Enemy tanks still treat walls as ghosts (§7).
2. **Trees** — hinge-fall, drive-over and shot-induced.
   **SHIPPED 2026-07-06**: `src/trees.rs` — every tree-asset instance (worldgen pines, act
   manifests, the arena ring) registers as a destructible tree on the instanced draw path
   (device-clone + `set_instanced_mats`, the grass pattern). Firs snap under the tracks without
   slowing the Tiger; big pines hold it to a crawl for ~2 s of shove then give; any main-gun hit
   fells (slice 3's pen ray refines AP pass-through). One hinge rotation at the base pivot,
   composed off the authored transform; Down trees persist. Events (snap/thud) route to pooled
   dust/debris + ranged audio. Verified by `cargo run -- treefall` (3 asserted beats). Enemy
   shells still ignore trees (cheap, rare — revisit with the AI destruction pass).
3. **Ammo natures + penetration ray** — AP/HE split, overpen kills, sympathetic dust (on the
   current HP model; holes become real once chunks land).
   **SHIPPED 2026-07-06**: T swaps PzGr/SprGr (interim instant hand-swap; loader ritual later),
   sights HUD tag. AP walks the pen ray exactly as §3 specs: `ballistics::budget_mm` at range,
   per-material `ApTransit` (cost/deflect/disrupt), hashed kinks, the terminal plate fought with
   `(budget − costs) × (1 − disrupt)`; the tracer flies the piecewise route and every en-route
   effect (wall chip, tree snap) applies the moment the round passes it. HE detonates on first
   surface: radial structure damage with falloff (85 HP peak, 7 m radius), outward tree blast,
   sympathetic dust to 2×. AP is structurally light (15 HP hole / 30 HP terminal); HE
   demolishes. Verified: `cargo run -- overpen` (a T-34 killed THROUGH the demo wall, which
   survives; two SprGr then fell it) + the pen_ray_budget_asymmetry unit test (88-through-brick
   kills a T-34 flank; 76-through-brick can't hole the Tiger). Enemy shells still terminate on
   walls (no enemy pen ray yet — §7).
4. **Chunk-grid walls + ballistic debris** — act-1 walls as chunk grids, holes, base cascades,
   persistent settled rubble.
   **SHIPPED 2026-07-06**: `src/rubble.rs` — every thin axis-aligned destructible (22 of the 27
   act-1 walls, ~450 chunks) pre-allocates a staggered brick-course grid, hidden behind the
   intact mesh. First hole activates the set. AP transits knock out a cylinder along the shot
   line (a hole you can drive a gun sight through); HE removes a sphere and flings radially;
   a chunk whose under-course died lets go after a hashed beat (shoot-the-base cascades); full
   collapse (shellfire OR the crush grind) tumbles everything still standing. Dead chunks fly
   dumb ballistics, bounce once off the heightfield, and FREEZE forever — persistent rubble, no
   physics crate. Rotated GLB shells keep swap-collapse (their AABBs lie). Verified inside
   `cargo run -- overpen` (hole counts + all-settled-frozen assertions). Deferred: occluder
   holes (LOS stays coarse until full collapse), chunked Blender-act buildings.
5. Later: Blender-act chunk bake, bunker spall decals, craters, ram → track damage,
   AI blind-fire + crush pathing, cellar-collapse hazard (tanks falling into Stalingrad cellars —
   real, and a terrifying scripted set-piece candidate).

## 10. Open questions

- Ammo switch input & HUD (interim key + reticle tag now; loader-station ritual when the
  embodied interior lands).
- Whether destructible cap rises from 44 (likely 64 with chunk pooling; measure first).
- Does HE need a minimum-arm distance (muzzle-adjacent wall self-blast)? Probably yes, ~15 m.
