# Panzerrechnersystem — BUILD ORDER (the master sequence)

> The full spec is designed (see WORLD_AND_NAVIGATION, ACOUSTICS, PLATOON_COMMAND, NEUROEVOLUTION,
> and the kessel-* memories). This doc sequences the *build*, dependency-first. Locked 2026-07-01.

## The two load-bearing ideas that set the order

1. **The countryside is the first map = the playtest sandbox.** Open terrain + sparse landmarks is the
   simplest world to stand up, it fits the *current* engine (one bounded scene, no big refactor), and
   once it exists every other system is tested inside it. (It's Act 2 narratively, but build order ≠
   story order.)
2. **The shared `ImpactEvent` / ballistics is the linchpin.** One event (shell mass/velocity/type +
   hull panel + penetrate/bounce + bearing + station distance) drives the visual FX, the parametric
   sound, AND the AI's combat fitness. So once the sandbox exists, ballistics is the highest-leverage
   next step — it unblocks the sound pillar and the AI at the same time.

## Phases (each builds on the last; each testable in the sandbox)

### Phase 0 — the countryside sandbox *(START HERE)*
Wire `world::ActMap` into `Game::build`: terrain window from the act (`west_offset_m`/`half_extent_m`),
sparse landmark hero-meshes + light cover scatter, spawn at `spawn`, red-X goal gate + arrival
detection. A drivable, bounded open map. **Deliverable: a place to drive.**
- First increment: generalise `terrain::load` / `TerrainConfig` to take the act's west offset +
  half-extent (`TerrainConfig::for_act`). *(done)*
- Then: `Game::build` takes an `ActMap`; place landmarks; spawn; goal-gate arrival → act-complete stub.

Countryside is SPARSER than the city (fewer buildings) but NOT empty: it has **fully explorable
military structures / factories / bunkers** as scavenge sites. Those come in Phase 1.5.

### Phase 1 — navigation feel
Compass ribbon HUD (reads hull/commander heading). Then the paper-map overlay (`M`) — terrain sketch +
landmark glyphs + spawn cross + red X, drawn outside the crush. Then pencil drawing + night flashlight.
**Deliverable: you navigate the sandbox by hand.** (Independent of Phase 2 — could interleave.)

### Phase 1.5 — explorable structures + on-foot scavenging
Fully explorable **factories / military structures / bunkers** (bunkers may be loading-screen
sub-levels, not seamless) with **ammo + fuel + cigarette loot** — the Absitzen on-foot loop (dismount →
sneak → scavenge → exfil; Luger is a useless-but-cathartic game piece; no infantry, only tank threats;
prior work on the `Absitzen` branch — foot.rs/pickups). **Fuel is a managed resource** (attrition) that
makes route choice matter and forces the winding, non-beeline navigation. Consumes loot = {cigarettes,
shells, MG ammo, pistol ammo, fuel}.

### Phase 2 — the combat core: ballistics + the shared ImpactEvent *(the linchpin)*
The state-of-the-art ballistics engine (parked design): projectile travel-time / arc / drop, per-tank
armour plates, DeMarre penetration + normalisation + shatter + material, feeding the existing
simulation-grade spall/crew/module model. It emits the **`ImpactEvent`**. **Deliverable: real shots
resolve physically in the sandbox.**

### Phase 3 — sound goes live
Route `ImpactEvent` → the parametric impact synth (`acoustics.rs`, built) for in-world CLANGs + MG
gravel; the flash→arc→CLANG→delayed-report timeline (speed-of-sound queue already exists). Then
**interior acoustics** (buttoned = muffled + hull ring; open hatch = world floods in) + **concussion /
tinnitus** on big hits/penetrations. **Deliverable: the sandbox sounds spectacular.**
- *Quick win available first:* route the EXISTING hitscan hits to the synth for immediate in-game
  audio, then upgrade to the Phase-2 `ImpactEvent`.
- *Future (whenever):* the full interior spatial-acoustic sim ("what it actually sounded like") — an
  upgrade to the parametric interior pass once interior geometry exists. Nobody will notice; we want it.

### Phase 4 — the AI substrate (enemies feel alive)
Headless sim/render split + self-play harness (needs Phase 2's combat resolution to score fitness) →
neuroevolution loop (CPU, 24 threads) → a T-34 that flanks → **co-evolving Tiger** → drop evolved
enemies into the sandbox. **Deliverable: living enemies to fight.**

### Phase 5 — scale to the full world
NOW the scene-architecture refactor (stable handles replacing the fixed `Slots`/FX-tail, frustum
culling, GPU instancing) — needed for the dense city + full 3-act structure + loading screens + the
per-act-restart / one-shot-finale retry model. Build **Act 1** (city: procedural buildings + interiors
+ bunkers) and the **Act 3** small-town arena. Plus the **M-D compute path tracer** (GI, RT shadows/
reflections, volumetric in-scatter) — slots here or earlier as a parallel visual track.

### Phase 6 — the finale systems
Platoon command (LLM intent parser → order schema → friendly executors + radio replies), the
**co-evolved armada** as the final boss, the per-duel evolved bosses (KV-2 / ISU-152), the King-Tiger
narrative handoff. Depends on Phase 4 (AI) + Phase 5 (world).

## Procedural vs. hand-authored (the map division of labor)

Directive (revised 2026-07-01): **maximize procedural — generate as much of the full, detailed map as
possible; the user FINE-TUNES from there** (curate + add hero detail in Blender, not build from
scratch). The split below is now about what procedural *reaches for* vs. what still benefits from a
hand pass — not a hard wall.

**Engine / procedural (automatic, deterministic, seed-driven):**
- Terrain: the per-act DEM window + season tint (done: `terrain::for_act`); grounding anything on the
  heightfield.
- Layout / blockout: street grids, building footprints & lots, block placement (`city.rs`), the
  starting point, spawn + red-X goal gate positions.
- **Placeholder landmarks:** stand-in geometry at each `ActMap` landmark position (you replace with a
  hand-built GLB later).
- Scatter (high volume, by rule): rubble, debris, craters, wrecks, vegetation (PSX Nature trees/grass),
  fences, poles, scattered cover, snow dressing — instanced.
- Roads / rail as tiles/splines. Loot-cache + enemy-spawn/patrol-zone placement (gameplay layout).
- Density / ruination variation by seed; LOD + instancing of repeats.

**You / hand-authored in Blender (bespoke, intent-driven):**
- **Hero landmarks:** the actual church, factory, grain elevator, water tower, bunker exteriors — the
  distinct silhouettes you navigate by.
- **Fully explorable interiors + bunkers:** rooms, corridors, factory floors, bunker sub-levels — laid
  by hand so they're navigable, interesting, and scavengeable (procedural interiors read generic).
- **Set-pieces & readable combat space:** the KV-2 duel arena, the empty rendezvous, the final-battle
  town — sightlines, cover, flanking routes, choke points authored for drama and fairness.
- **Emotional beats & narrative props:** your leveled home, the personal-anchor object, staged scenes.
- **Kitbashing / asset fixes:** combining pizza modular pieces into custom buildings; adapting non-pizza
  assets (brutalist/Soviet buildings, trains, sewers) where they add.

**Middle ground:** procedural *kitbash* (assemble pizza modules into buildings by rule) can carry the
background/distance city further than bare greybox; you then hand-refine the foreground/explorable/
set-piece structures and leave the far ones procedural. Pizza assets are the primary visual language;
use the others where they earn their place.

## Parallelable tracks (not strictly sequential)
- **Visuals (M-D path tracer)** can advance alongside Phases 1–4 — it doesn't block gameplay.
- **Navigation (Phase 1)** and **ballistics (Phase 2)** are independent; interleave to taste.
- **Sound dialing** (tuning the synth from listening) is continuous once Phase 3 lands.
