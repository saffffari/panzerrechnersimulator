# Kessel — CREW & ATTRITION SYSTEM

**Team:** Crew & Attrition
**Owns:** the crew roster + mortality model, the capability/control-degradation layer, crew
characterization (silent, no-dialogue), the roster/status UI, the crew-care (cigarette/health)
ration loop, the field-mod progression, and the C++ that hangs all of this off the pawn
(`UKesselCrewComponent` + `FCrewMember`).
**Does NOT own:** the ballistics math (we consume `FCrewModule` + `FHitResolution` from the
ballistics arm), the cigarette item/economy itself (we consume it — the cigarette arm owns supply),
the atmosphere/night-light model (we feed it nothing; crew never EVA), the EVA/on-foot system (only
the player EVAs — we just lock crew inside), input *binding* (the pawn owns binds; we own the
*gating/capability mask* that those binds query).

Pillar anchors: **P1** deep-sim/abstracted (crew are a real degradation state machine; presented as
silhouettes, animation, and absence — never a chat box or floating health bars); **P2** the
workload-and-isolation pillar — *re-framed* below; **P5** always-inside-the-steel (crew are part of
the steel you live in; only YOU ever leave); **P6** historical grounding (a real Tiger crew of five,
real stations, real field expedients).

---

## 0. SUPERSEDES the "crew of 1 from the start" framing (read first)

`DESIGN_PILLARS.md` P2 currently reads *"You are the crew of one… commander, gunner, driver, loader,
radio — all you."* **This document supersedes that as the premise.** The locked vision is now:

> You **start with a full Tiger crew of five** (you = commander, + driver, gunner, loader,
> radio-operator/hull-gunner). The crew are **mortal, irreplaceable, permadeath within the run**.
> Across a 1.5–2.5 hr run you **decay from a functioning crew to a lone operator**. *"Crew of one"*
> is the **worst-case END STATE**, not the starting condition.

P2's spirit — **workload and isolation under pressure, the friction of one person doing too much** —
is *preserved and made dynamic*: instead of being maxed-out from frame one, the workload **ramps as
men die**. Each loss hands you another job. The full-crew opening is the "before," so the player
*feels* every subtraction. This is strictly a stronger version of P2: the burden is now earned,
escalating, and personal. (Recommend P2's text be edited to: *"You decay toward the crew of one."*)

This single change does not touch any other arm's locked work — ballistics already models 5 crew
slots in `FCrewModule`, the pillars' isolation/workload intent is intact, and EVA still has only the
player ever leaving the hull.

---

## 1. The roster — five men, five jobs, five costs

You command a standard Tiger I crew. Four men sit at stations you can physically look at from the
interior; you are the fifth. Every man has a **normal contribution** (what the game does *for* you
while he lives) and a **loss-cost** (the exact capability that is subtracted when he dies). Loss-cost
is the design payload — losing a crewman must change *how you play*, not just a number.

| Role | Station (where you see him) | Normal contribution | Loss-cost (what you must now do yourself / lose) |
|---|---|---|---|
| **Commander — YOU** | Turret roof / cupola; the camera | Command, spot, designate, the only one who EVAs | You can't die-and-continue; commander death = run over (the only instant-fatal crew loss) |
| **Driver** | Front-left hull, visible through the interior forward | Handles WASD locomotion *concurrently* with your turret work — you drive AND lay the gun at once | **Lose concurrency.** You can no longer drive and aim simultaneously; you must switch modes — *Drive mode* (WASD) **or** *Fight mode* (turret/gun), never both live. See §4. |
| **Gunner** | Turret front-right, at the optic, hand on elevation wheel | Lays the gun fast and steady; tight reticle, quick settle, assisted lead | **You lay the gun yourself.** Slower traverse/elevation, heavier sway/settle, no lead assist, longer time-to-first-accurate-shot. See §4. |
| **Loader** | Turret left, at the breech & ready-rack | Auto-reload on a timer (the reload you already have) | **Manual loading.** Reload becomes a deliberate, interruptible, vulnerable player action (a hold/sequence), much slower, can't be done while also doing X. See §4. |
| **Radio-op / Hull-gunner** | Front-right hull, at the set & ball-MG | Operates hull MG (close defense) + the radio = the **map / intel / contact bearings** | **Lose the hull MG and lose the map/intel.** No close-in MG answer to infantry; the nav/threat map goes dark or degrades to dead-reckoning. See §4 + §6. |

**Why this roster maps cleanly onto the existing sim:** the ballistics arm's `FCrewModule` list for
the Tiger already enumerates *Cmdr, Gunner, Loader, Driver, Radio* (research/04 §12 "Crew×5"). We are
not inventing geometry — we are giving each of those five interior nodes a **gameplay role + loss
behaviour + a body you can see**.

---

## 2. Wounded vs Killed — the survival state model

A penetrating hit does not simply delete a man. Each crewman is a small state machine so that *most*
hits degrade rather than instantly erase — this makes attrition a slope, not a cliff, and creates the
"keep a wounded man working" tension.

### 2.1 States

```
HEALTHY ──hit──► WOUNDED ──hit / bleed-out / untended──► DEAD
   ▲                 │
   └──── tended ─────┘   (cigarette-care + time can stabilise a WOUNDED man; never revives DEAD)
```

- **HEALTHY** — full contribution.
- **WOUNDED** — alive but **degraded**: the role still functions but worse (slower, shakier, lower
  effective skill), AND he is **bleeding** on a timer. A wounded man left untended drifts toward DEAD.
  This is the lever for the crew-care loop (§5): you spend a cigarette / a lull to stabilise him.
  - Wounds carry a *severity* (`Light / Serious`). Light = small penalty, slow bleed, self-stabilises.
    Serious = heavy penalty, fast bleed, will die without care.
- **DEAD** — contribution gone forever (permadeath within the run). His station goes empty; his
  effects stay (§3). No replacement, ever.

### 2.2 How a hit chooses a man — straight off the ballistics crew-module model

This is the **systemic integration the brief demands**: *where* a penetrating shell lands decides
*which* crewman is hit. We reuse the existing pipeline end to end:

1. A round penetrates a face → ballistics produces an `FHitResolution` with `entryPoint` + `dir` +
   `spallPower` (already in `KesselBallisticsTypes.h`).
2. The post-pen **fragment cone** (research/04 §9) is cast from `entryPoint` along `dir`. It tests
   the interior `FCrewModule[]` nodes — which include the five crew positions at their `LocalPos`.
3. Each crew node the cone catches takes `spallPower * falloff(distance)`. **That is the man who is
   hit.** A shell through the driver's plate sprays the driver; a turret-side pen at loader height
   sprays the loader; an ammo-rack cookoff sprays everyone.
4. Damage is applied to the crewman's wound pool, not a generic HP bar:
   `spallPower` → wound severity → state transition (HEALTHY→WOUNDED→DEAD) per thresholds.

**Consequence the player learns: armor-angling becomes protecting specific men.** Turning your hull
to bounce a shot off the glacis is now also *putting the driver behind the thickest steel*. Presenting
the turret cheek protects the gunner and loader. A side-on hull-down exposes the radio-op/hull-gunner
and the sponson ammo. This makes the existing armor model *characterful* for free — you angle to save
a person, and you feel it when you guess wrong.

**Commander exception:** the commander is YOU and is the camera. A confirmed solid commander-node pen
is the **run-ending** hit (replaces the old "crew-of-1 dies" abort in research/04 §9.4 and
MILESTONE_1_PLAN step 10). Every other crewman can be wounded or killed and the run continues —
degraded. (Optional later nicety: a brief commander "serious wound" bleed-out window for drama; M1
keeps it a clean cut to the death/abort flow.)

### 2.3 Required data extension (small, additive, ballistics-compatible)

The committed `FCrewModule` has only `{ Name, Health, Criticality }`. It is correctly minimal for the
ballistics arm and we do **not** mutate it. Crew *runtime* state lives in our own `FCrewMember`
(§7), keyed to the module by `Name`. The only thing we ask the ballistics/armor data to carry is the
interior **position** of each crew node so the fragment cone can hit it — that is a `LocalPos` on the
module (research/04 §10's schema already specified `FVector LocalPos`; the committed struct dropped
it). **Ask to ballistics arm:** restore `FVector LocalPos` (and ideally `FName Kind`) to
`FCrewModule`, or expose crew node positions on `FVehicleArmorLayout`. Until then, M1 can place crew
nodes as fixed offsets in `UKesselCrewComponent` (§9.4). This is the one cross-arm dependency.

---

## 3. No dialogue, real characters — making silent men land

Constraint (locked): **no dialogue, ever**. Individuality is conveyed **only** through presence,
animation, personal effects, mechanical role, and absence. Here is the grammar.

### 3.1 The five channels of characterization

1. **Presence at station.** When you look around the interior (commander periscope sweep, or just
   turning your head buttoned-up) the four men are *there*, at their posts, bodies occupying space.
   The interior is never empty while they live. Seeing them is the bond — they are your companions in
   the steel even though they never speak.
2. **Animation — idle / work / reaction.** Three animation registers per man, no words needed:
   - *Idle* (driving lull): the loader leans back, the gunner rubs his hands for warmth, the radio-op
     hunches at the set. Small, looping, human. (See §3.3 on the M1 stand-in for this.)
   - *Work*: the loader rams a shell on reload; the gunner spins the elevation wheel as the gun lays;
     the driver's shoulders work as you turn. **The work-anim is also a readability cue** (§8.2) —
     you *see* the reload happening.
   - *Reaction*: a near-miss makes them flinch and duck; a penetration throws them; a wounded man
     clutches and slumps; a dead man is still. Reactions are how the violence reads on the *people*,
     not just the hull.
3. **Personal effects.** Each man owns a tiny kit authored as placed props at his station: a **photo**
   tucked by his sight, a **name stencil** on his locker, his **cigarette brand** (ties to the
   cigarette arm — each man smokes a different period brand), a scarf, a mascot. These are how you
   learn *who* he is without a word — you read his corner of the tank.
4. **Mechanical role.** He *is* what he does. The loader is "the hands that keep you firing"; the
   driver is "the man who lets you move and shoot at once." You bond with the *capability*, and the
   loss-cost (§1) is the grief — you miss him because the gun is suddenly slow.
5. **Absence.** When he dies: the **seat is empty**, the **work stops** (no more ram animation; the
   gun goes quiet on its own), and his **effects are left behind** — the photo still tucked by a sight
   no one looks through, the half-smoked cigarette brand now unclaimed. Absence is the loudest channel.
   Design rule: **never clean up a dead man's corner.** The clutter staying is the eulogy.

### 3.2 How loss is made to land (the emotional beat)

A death should be a *staged moment*, not a stat tick:

- **The hit** — a penetration that catches a crew node throws a **named** reaction (that man flinches
  / slumps), a sharp audio cue at his station (P3 — handoff to audio: a cry cut short, then the
  *wrong* silence where his work-sound was), and a hard, unmistakable **capability-loss banner** (§8).
- **The aftermath** — the station goes still; the roster strip (§8) flips his portrait to a state you
  can read in a glance; the control change bites on your *very next action* (you reach to fire and the
  reload is now manual). The mechanics teach you the loss before you've finished mourning it.
- **The lingering** — his effects remain in view for the rest of the run. Every time your periscope
  sweeps past the empty seat with the photo still in it, that's the game telling the story with no
  words. The longer you survive, the more empty seats you carry — the interior becomes a tomb you
  drive west.

### 3.3 Honest M1 stand-in (we have no skeletal mesh — see ARCHITECTURE)

ARCHITECTURE locked **no skeletal mesh / rig / anim** for M1. So M1 cannot ship hand-animated bodies.
The **M1 characterization slice** is therefore:
- **Static crew "presence" props** — a crude seated form (BasicShapes assembly or a single low static
  mesh) at each of the four stations, plus the **personal-effects props** (photo card, name-stencil
  decal, a cigarette pack with a distinct brand label). This already delivers presence + effects +
  absence at near-zero rigging cost.
- **State by transform/material, not skeleton** — wounded = slumped transform + a desaturated/blood
  material swap; dead = collapsed transform + still. "Reaction" = a one-shot transform nudge
  (timeline), exactly the recoil/hatch pattern ARCHITECTURE already uses for the gun.
- **Absence = hide the seated form, keep the effects props.** Trivial and devastating.
- Full idle/work/reaction *skeletal* animation is a **post-M1 art track** (lands when the real
  interior + rig arrive, same schedule as the hero interior, OD-2). Designing the channels now means
  the rig, when it comes, has a spec to hit.

---

## 4. Capability degradation — the exact control changes

This is the core of the system. Each loss subtracts a capability via a **capability mask** the input
layer queries (§9.3). The contributions are *additive while alive*; death flips a bit and the
controls change on the next frame. **Unmistakable feedback** on each (§8) is mandatory.

### 4.1 Driver down → lose drive/fight concurrency

- **Healthy:** WASD locomotion and turret/gun aim are **both live every frame** — you reposition while
  laying the gun, the classic tank duel.
- **Driver dead:** you cannot do both. A **mode toggle** splits them:
  - *Fight mode* (default after the loss): turret/gun aim live; **hull is parked** (throttle ignored).
  - *Drive mode* (hold/press a modifier): WASD drives; **turret is locked** to its current relative
    bearing, no aim input, optic stows.
  - You must *commit* to moving or to fighting. Repositioning under fire now means going blind on the
    gun for the seconds you drive — a brutal, legible cost. (You, the commander, are "climbing into the
    driver's seat" — diegetic justification for the mode switch and for the optic stow.)
- **Tuning:** small switch latency (~0.5–1.0 s "scramble" while you change seats) so the swap itself
  is a tactical commitment, not a free toggle.

### 4.2 Gunner down → you lay the gun

- **Healthy:** gunner provides fast, steady lay — high traverse/elevation rate, tight settle, minimal
  sway, optional lead/range assist on the reticle.
- **Gunner dead:** **player lays the gun directly** at the gunner's optic, but degraded:
  - Traverse/elevation rate cut (e.g. ×0.5–0.6); **settle/sway increased** (the reticle breathes, takes
    longer to steady); **no lead assist** (bare reticle).
  - Time-to-first-accurate-shot goes up sharply. Long shots become genuinely hard; you want to close
    range or wait for a still target. This is a direct hit on your offensive power, felt every shot.

### 4.3 Loader down → manual loading

- **Healthy:** reload is the existing automatic timer in `UKesselGunComponent` — press fire, the gun
  comes back on its own after the reload interval.
- **Loader dead:** reload becomes a **deliberate manual action** the player performs:
  - A **hold-or-sequence** reload (hold R through a progress action; later a two-beat
    "pull-shell / ram-and-seat"), **much slower** than the auto reload (e.g. ×1.8–2.5 time), and
    **interruptible/vulnerable** — you are heads-down doing it, can't drive (if driver also gone) and
    your aim is occupied. A round in the breech is no longer free.
  - Deliberate, tense, period-correct (the commander hauling rounds himself). Reads instantly as
    "everything is slower and you are exposed while you do it."

### 4.4 Radio-op / hull-gunner down → lose MG + map/intel

- **Healthy:** the hull MG answers close infantry (a secondary close-defense weapon), and the radio
  feeds the **map/intel** — contact bearings, the nav/threat overlay, knowing roughly where things are.
- **Radio-op dead:** **hull MG offline** (no close-in answer — infantry/ATR teams that get close are
  now a real threat with no cheap solution) **and** the **map/intel degrades**: the threat overlay goes
  dark or drops to stale dead-reckoning; bearings stop updating. You drive west more blind, more deaf —
  which dovetails directly with the atmosphere arm's night/whiteout (you lose your electronic sense
  exactly when the world is least readable). The map UI visibly "dies" (§8) rather than just vanishing.

### 4.5 Stacking — the worst case is the premise's end-state

Losses compound. Driver + loader gone = you park, manually load, then re-take the gun: a duel becomes a
slow, vulnerable solo ritual. Driver + gunner + loader gone = you are doing four jobs in sequence under
fire — **the "crew of one" the old pillar described, now reached by attrition.** The game's difficulty
curve *is* the casualty curve. The system needs no separate difficulty scaling; the crew is the
difficulty.

---

## 5. Crew-care: cigarettes as the rationed health/morale resource

The cigarette arm owns the cigarette as a supply item; **we consume it as the crew-care currency.** The
brief makes cigarettes "extend to crew = a crew-care/health/morale resource you ration among your men."

### 5.1 What a cigarette does for a crewman

- Spending a cigarette on a crewman in a **lull** (no live combat) **tends** him:
  - **Stabilises a WOUNDED man** — halts bleed, nudges Light wounds back toward HEALTHY over time,
    holds Serious wounds from progressing to DEAD. (It never revives DEAD — permadeath is permadeath.)
  - **Restores morale/steadiness** — a frayed crewman's penalties (extra sway, slower work from
    stress/cold) ease. Morale is a soft per-man modifier on his contribution; cold + fear + watching a
    mate die degrade it; a smoke (and survival) restore it.
- It is a **ration decision**: you have few cigarettes, several men, ongoing cold and wounds. Who gets
  the smoke? The wounded loader you need firing, or the steady gunner whose nerves are going? This is
  the crew-management heart — scarce care across irreplaceable men.

### 5.2 Integration with the health system + the player's own smoking

- The atmosphere arm already notes the **player's** smoking heal (a lit cigarette during EVA = stand
  still, regen, glow as a beacon). Crew-care is the **inside-the-tank** sibling: same item, spent on
  your men instead of yourself, with the same scarcity pressure.
- **Cold pressure (ties health to atmosphere):** General Winter slowly degrades crew steadiness/morale
  over a long run (a passive "cold" drain). Cigarettes (and being buttoned-up, cabin light/heat) push
  back. This gives the multi-hour drive a quiet attritional pulse even between fights — you are keeping
  men *alive and functional*, not just un-shot.
- **Handoff — cigarette arm:** they own supply (how many, where scavenged, brands). We define the
  *sinks* (stabilise/morale) and the per-man brand-flavor (each crewman's preferred brand among his
  effects, §3). **Handoff — health arm (if separate):** wound severity + bleed timers + the cold drain
  are the shared health model; we apply them per-crewman.

---

## 6. Field-mod progression — grounded, mostly-downward attrition

The user **rejected RPG gun/engine upgrade trees** ("you're one guy"). Progression = **crew survival +
small scavenged historically-plausible field-mods + components.** This is *not* a tech tree; it's a
handful of bolt-on expedients a real crew would scrounge, weighed against the steady downward pull of
attrition.

### 6.1 The shape: a slow downhill with small handholds

Over a run your power **mostly falls** (men die, ammo depletes, components wear). Against that you can
make **minor upward patches** — never enough to reverse the slope, just enough to *choose where to
spend a lull* and feel briefly competent. Tension stays weighted toward loss (FTL-style).

### 6.2 Field-mod catalogue (period-plausible, all scavenged, all trade-offs)

| Field-mod | Scavenged from | Effect | Cost / trade-off |
|---|---|---|---|
| **Applique armor / spare track links on the hull** | wrecks, your own column | +effective thickness on the bolted face (a few mm LOS) → shifts some marginal pens to non-pens / protects the man behind it | Adds mass (slower accel/pivot — handoff to movement); install takes a lull |
| **Sandbags (glacis/turret)** | ruins, positions | Small frontal pen-margin + morale ("doing something") | Mostly placebo vs AP historically — we model a *small* real benefit + a morale bump; weight/heat cost |
| **Captured optic** | knocked-out enemy | Tighter gunner reticle / better zoom step — **partially offsets a dead gunner** | One-time find; install time; doesn't bring the gunner back |
| **Captured MG** | wreck/dead | Restores a close-defense MG — **partially offsets a dead radio-op/hull-gunner** | Ammo-hungry; you man it yourself (occupies you) |
| **Spare track links (as spares)** | wrecks | Repair a thrown/damaged track (mobility patch) | Consumed on use; install is a vulnerable lull |
| **Components** (existing resource) | wrecks, depots, the dead | Repair/replace knocked modules (optics, breech, transmission) within limits | The core scavenge economy; finite |

Design rules: every mod is a **bolt-on you can see on the hull** (environmental storytelling — your
tank visibly accretes a desperate, improvised skin as the run grinds on), every mod is a **trade-off**
(mass, heat, ammo, install-time), and **no mod restores a dead man** — the captured optic/MG only
*soften* a loss, never undo it. Progression is "patch the wound," never "level up."

### 6.3 Why this satisfies the pillars

Grounded (P6 — real crews did exactly this), abstracted (a few mm of LOS + a morale float, not a stat
screen), and it keeps the spine intact: you are **one man holding a dying machine together**, scavenging
the corpses of your own offensive (GAME_DESIGN's "cross the ruins you made coming in") for the scraps
that keep you moving one more hour.

---

## 7. Emotional & pacing design over a 1.5–2.5 hr run

The run is an **attrition arc**. Crew loss is the instrument that escalates *both* difficulty and
feeling in lockstep.

### 7.1 The arc

- **Act I — Full crew (the "before").** Everything is concurrent and smooth: you drive and shoot at
  once, the gun reloads itself, the map shows contacts, men move at their posts. This is deliberately
  *comfortable* — it establishes the baseline you will lose. The player learns the crew as companions
  (presence, effects, idle anims) during the long quiet drive *before* the first real fight.
- **Act II — The first loss.** The first death is the tutorial for grief and for degradation: one
  capability is subtracted, the control change bites, a seat goes empty and stays empty. The player
  learns the system's grammar in a single landed moment.
- **Act III — Compounding.** Each subsequent loss stacks (§4.5). The tank gets quieter, the interior
  emptier, your job-list longer, the fights harder because you are slower and blinder. Difficulty rises
  *because* the crew falls — no artificial scaling. Cigarette rationing (§5) gets crueller as wounds
  pile up and supply thins.
- **End-state — the lone operator.** If you reach the western edge with one or two men dead, that's a
  hard-won win carrying ghosts. If you're down to just yourself, you are living the worst-case premise:
  a solitary commander doing five jobs in a tomb, every empty seat with its photo still tucked in,
  grinding the last kilometres to friendly lines. **Reaching the west alone is the most powerful
  possible ending** — and dying after losing four men you couldn't save is the most devastating.

### 7.2 Pacing levers we own

- **Tension/release coupling** — losses land in the rare, intense combat spikes (GAME_DESIGN's loop);
  the *consequences* (empty seat, slower reload, dead map) play out across the long quiet drives that
  follow, where you have nothing to do but feel them and ration smokes. Combat creates the wound; the
  drive makes you live with it.
- **No respawns, no replacements** — every loss is final, so every fight is genuinely high-stakes (one
  shell can take a man and a capability). This is the permadeath pressure the FTL framing wants, made
  personal.
- **Escalating quiet** — the soundscape (P3 handoff) literally empties as men die: the loader's
  ram-clack, the hull-MG chatter, the radio's static all go silent one by one. The tank gets lonelier
  to *hear*, not just to see.

---

## 8. The two cautions the user raised — with solutions

### 8.1 Caution A — legibility of silent characters ("will the player even register these men / their loss?")

**Risk:** with no dialogue and (M1) no animation, the crew could read as set-dressing; a death could
pass unnoticed.

**Solutions:**
1. **Teach the crew before you lose them.** The long Act-I drive *introduces* each man through forced,
   low-key encounters: the cigarette-care ritual makes you look at each crewman and his effects; a
   "look around the interior" affordance (periscope sweep / head turn buttoned-up) shows them at their
   posts. You can't ration smokes among them without learning who they are.
2. **Multi-channel redundancy on identity.** Each man is over-specified — station + body + photo +
   name stencil + brand + a distinct silhouette/role — so even a player who ignores three channels
   catches one. Legibility doesn't depend on subtle animation.
3. **Make the loss a staged beat, not a quiet deletion** (§3.2): named reaction + station-audio cue +
   the empty seat + the lingering effects. The *mechanical* consequence (next-action capability change)
   guarantees the player registers it even if the fiction slips past.
4. **The roster strip (§8.3) is the safety net** — a death is always legible there in a glance, even at
   the worst moment.

### 8.2 Caution B — unmistakable feedback on capability loss ("did the player understand WHAT they just lost?")

**Risk:** a subtracted capability (driver concurrency, auto-reload) could be discovered confusingly
mid-fight ("why won't it drive?!") instead of understood.

**Solutions:**
1. **A loss banner / takeover card.** The instant a crewman dies, a brief, unmissable diegetic-styled
   card states the consequence in plain mechanical terms: *"DRIVER DOWN — you cannot drive and fight at
   once"*, *"LOADER DOWN — manual reload"*, *"GUNNER DOWN — you are on the gun"*, *"RADIO OUT — hull MG
   & map lost."* One sentence, the verb that changed. Shown once, on the death.
2. **The control teaches itself on the next action.** The very next time you try the affected action,
   the new behaviour is front-and-center: the reload prompt now reads a manual hold; the move/fight
   mode indicator appears; the optic stows when you drive. The mechanic *demonstrates* the loss.
3. **Persistent status, not just a one-time alert.** The roster strip (§8.3) shows each man's
   state and each *active degradation* (a "MANUAL LOAD" / "NO DRIVE-LAY" / "MG OFFLINE" / "MAP DEAD"
   tag) so the current capability set is always glanceable — the player never has to *remember* what
   they've lost.
4. **Diegetic reinforcement** — the empty seat you can see, the dead map panel, the silent hull-MG
   port: the fiction and the UI say the same thing.

### 8.3 Roster / status UI — must read fast

A compact **crew strip**, always available (a peek key) and auto-surfacing on any state change. Design
goals: *parse in under a second, no text-reading required in the heat, unmistakable on loss.*

- **Five slots**, fixed order (Cmdr / Driver / Gunner / Loader / Radio), each a small portrait/icon
  with a hard-readable state:
  - **HEALTHY** — lit, full color, faint idle motion.
  - **WOUNDED** — amber/red wash + a bleed pip; Serious = pulsing. (Color grammar borrowed from the
    atmosphere arm's "red = danger" light language, §5 there, so the whole game speaks one color.)
  - **DEAD** — greyed/struck-through, portrait darkened. The strip visibly *empties* as the run grinds.
- **Capability tags** under each slot only when relevant (`MANUAL LOAD`, `NO DRIVE-LAY`, `MG OFFLINE`,
  `MAP DEAD`) so the active degradation set is glanceable (§8.2.3).
- **Crew-care affordance** — in a lull the strip is where you spend a cigarette on a man (select →
  smoke icon). The ration UI *is* the roster.
- **Mode indicator** (when driver is down) — a clear *DRIVE / FIGHT* state pip near the reticle so the
  split-control state is never ambiguous.
- **Style:** abstracted, cold, low-fi — stencilled icons, not photographic portraits (fits P1 + the
  PS1-tunable look; survives every `MasterRetro` position). Reads like a tank's own crew board, not a
  game HUD.

---

## 9. C++ / architecture plan

Fits the existing scaffold: a new owned component on `AKesselTankPawn`, a runtime roster struct, a
capability mask the input layer queries, and a thin integration seam into the ballistics fragment-cone
result. No new module needed (lives in `Kessel`). Mirrors the component pattern already used for
`Gun/Armor/View/Aiming`.

### 9.1 New types

| Type | Base | Role |
|---|---|---|
| `UKesselCrewComponent` | `UActorComponent` | Owns the `FCrewMember` roster; receives crew-hit events from the armor/ballistics path; runs wound/bleed/morale ticks; computes the **capability mask**; drives the roster UI; spends cigarettes. One on the player pawn (and one on `AKesselEnemyTank`, where it only counts crew toward knockout — enemy needs no capability layer). |
| `FCrewMember` | `USTRUCT` | Runtime per-man state (below). Distinct from ballistics' `FCrewModule` (which stays the *armor-geometry* node); linked by `FName`. |
| `ECrewRole` | `UENUM` | `Commander, Driver, Gunner, Loader, RadioOp`. |
| `ECrewState` | `UENUM` | `Healthy, Wounded, Dead`. |
| `EWoundSeverity` | `UENUM` | `None, Light, Serious`. |
| `FCrewCapabilities` | `USTRUCT` (bitmask-ish flags + scalars) | The derived capability set the input/gun/movement layers read: `bCanDriveAndFight`, `bAutoReload`, `bGunnerAssist`, `bHullMG`, `bMapIntel`, plus scalars `GunLayRateMul`, `GunSwayMul`, `ReloadTimeMul`. Recomputed whenever any crewman changes state. |
| `UCrewRoster` (data) | `UPrimaryDataAsset` | Authoring: the five roles, each man's name/effects/brand, default station `LocalPos`, wound/bleed/morale tuning. Keeps crew data-driven like everything else. |

```cpp
USTRUCT(BlueprintType)
struct FCrewMember
{
    GENERATED_BODY()
    UPROPERTY() ECrewRole     Role     = ECrewRole::Gunner;
    UPROPERTY() FName         ModuleName;          // links to FCrewModule on the armor layout
    UPROPERTY() ECrewState    State    = ECrewState::Healthy;
    UPROPERTY() EWoundSeverity Wound   = EWoundSeverity::None;
    UPROPERTY() float         Bleed    = 0.f;      // 0..1, rises while untended-serious
    UPROPERTY() float         Morale   = 1.f;      // 0..1, soft contribution modifier (cold/fear/care)
    // characterization refs (effects props, brand) authored on UCrewRoster, not here
};
```

### 9.2 Integration with the ballistics `FCrewModule` path

- The existing hit path (research/04 §9 / ARCHITECTURE §7) already ends: *on PEN → fragment cone vs
  `FCrewModule[]` → damage/knockout*. We **insert one call**: when the cone damages a crew-kind
  `FCrewModule`, `UKesselArmorComponent` (or the subsystem) raises
  `UKesselCrewComponent::ApplyCrewHit(FName ModuleName, float SpallPower, FVector HitPos)`.
- `ApplyCrewHit` maps `ModuleName → FCrewMember`, converts `SpallPower` to wound severity, advances the
  state machine, recomputes `FCrewCapabilities`, fires the loss banner + roster update + the named
  reaction on the body prop. **The ballistics math is untouched** — we are a consumer of its output.
- **Commander node** → routes to the existing death/abort flow (MILESTONE_1_PLAN step 10) instead of a
  capability change.

### 9.3 Capability gating of input (the control-degradation mechanism)

- `UKesselCrewComponent::GetCapabilities()` returns the current `FCrewCapabilities`. The pawn's input
  handlers and the relevant components **query it before acting**:
  - Throttle/steer handlers: if `!bCanDriveAndFight`, locomotion only applies in *Drive mode* (a small
    mode state on the pawn the crew comp owns); turret aim handler no-ops in Drive mode.
  - `UKesselTurretAimComponent`: multiplies its traverse/elevation rate by `GunLayRateMul` and its
    sway/settle by `GunSwayMul` (already rate-limited chase — we just scale the rates); disables lead
    assist when `!bGunnerAssist`.
  - `UKesselGunComponent`: if `bAutoReload` use the timer; else enter the manual-reload action
    (progress-driven, `ReloadTimeMul`). The component already owns the reload timer — we branch it.
  - Hull MG / map: gated on `bHullMG` / `bMapIntel` (M1: MG may be stubbed; map degrade is a UI state).
- This keeps the degradation **centralized and data-clean**: input bindings don't change, the
  *capability mask* changes, and every system reads the same mask. Easy to test, easy to tune.

### 9.4 Crew node positions (the one open dependency)

The fragment cone needs each crewman's interior position. Options, in preference order:
1. **Ballistics restores `FVector LocalPos` (+`FName Kind`) on `FCrewModule`** (research/04 §10 had it;
   the committed struct dropped it). Cleanest — crew geometry lives with the armor layout. *(Ask
   submitted to the ballistics arm.)*
2. **Interim:** `UKesselCrewComponent`/`UCrewRoster` carries the five `LocalPos` offsets and registers
   them so the cone test can reach them, until (1) lands. Unblocks M1 without touching ballistics.

### 9.5 UI

`WBP_CrewRoster` (UMG) bound to `UKesselCrewComponent` broadcasts (`OnCrewStateChanged`,
`OnCapabilitiesChanged`). Stencilled icons + state colors (§8.3), survives `MasterRetro`. The loss
banner is a transient `WBP_CrewLossBanner`. Both are UMG-primitive (consistent with the M1 reticle
approach — no bespoke textures needed first-pass).

---

## 10. M1 prototype slice vs later

### M1 (ship in Milestone 1 — extends the step-10 crew-pen closure)

The current M1 plan (step 10) only does *player crew-pen → disable/fade/reload* for a crew-of-1. The M1
crew slice upgrades that to the **degradation skeleton**, kept honest against the no-skeletal-mesh
constraint:

- `UKesselCrewComponent` + `FCrewMember` roster of 5 on the player pawn; `FCrewCapabilities` mask.
- `ApplyCrewHit` wired into the existing fragment-cone path (the one new call, §9.2) using interim crew
  `LocalPos` offsets (§9.4 option 2).
- **Wounded vs Killed** state machine with bleed (simple Light/Serious thresholds) — at least
  Healthy→Wounded→Dead working off `spallPower`.
- **Two capability degradations fully wired** (highest gameplay payoff, lowest art cost):
  **Loader-down → manual reload** (branch the existing reload timer) and **Gunner-down → degraded lay**
  (scale the existing aim-component rates). These prove the whole control-degradation mechanism with
  zero new art. Driver-down mode-split and radio-down MG/map are **stubbed/flagged** for M1 if movement
  & map aren't ready, full in later.
- **Commander-pen → existing death/abort flow** (reuse step-10).
- **Roster strip UI** (`WBP_CrewRoster`, stencil icons + states) + **loss banner** — UMG primitives.
  This is the legibility safety net and is cheap.
- **Static crew presence + effects props** at the four stations (seated form + photo/name/brand props),
  with transform/material wound/dead states and hide-on-death absence (§3.3). No rig.
- **Cigarette crew-care: minimal** — spend-a-cig-to-stabilise-a-wounded-man in a lull, if the cigarette
  arm's supply hook exists in M1; else stub the sink and flag.

### Later (post-M1, honest deferrals)

- **Full skeletal crew** — idle/work/reaction animation per man (lands with the hero interior + rig,
  OD-2). The single biggest deferral; everything above is designed to *upgrade into* it.
- **Driver-down mode-split** and **radio-down MG + map-degrade** fully wired (need the Phase-2 movement
  swap and the world-map/intel arm respectively).
- **Morale/cold drain** as a continuous Act-by-Act pressure; full cigarette ration economy across all
  five men with brand flavor.
- **Field-mod catalogue** (§6) — applique/sandbags/captured optic+MG/track-link repair as installable
  bolt-ons with visible hull accretion + mass/heat/ammo trade-offs (depends on movement + scavenge
  economy maturing).
- **Staged death cinematics** (the full reaction beat with audio P3 cues — the cry-cut-short, the
  emptying soundscape).
- **Serious-commander bleed-out window** drama option (OD).

---

## 11. Concise structured summary

### Key decisions
- **Supersedes "crew of one from the start."** You start with a **full crew of 5** and **decay** to a
  lone operator; "crew of one" is the **worst-case end-state**, not the premise. P2's workload/isolation
  intent is preserved and made *escalating* (recommend editing the pillar text).
- **Wounded vs Killed** state machine (Healthy→Wounded[Light/Serious, bleeding]→Dead, permadeath);
  most hits **degrade** rather than delete, creating the keep-a-wounded-man-working tension.
- **Hits choose the man via the existing ballistics fragment-cone** vs interior crew `FCrewModule`
  nodes — so **armor-angling = protecting specific men**, for free, off the committed combat model.
- **Capability degradation via a central `FCrewCapabilities` mask** the input/gun/aim/movement layers
  query: Driver→lose drive/fight concurrency (mode split); Gunner→you lay the gun (slow/shaky/no
  assist); Loader→manual reload (slow/vulnerable); Radio→lose hull MG + map/intel.
- **No-dialogue characterization** via 5 channels — presence, animation, personal effects (photo/name
  stencil/cigarette brand), mechanical role, and **absence** (empty seat + effects left behind, never
  cleaned up).
- **Cigarettes = rationed crew-care** (stabilise wounded, restore morale; never revive dead) — the
  inside-the-tank sibling of the player's smoking heal; ties to cigarette + health + cold(atmosphere).
- **Progression = crew survival + small scavenged field-mods (applique, track links, sandbags,
  captured optic/MG) + components** — visible hull accretion, all trade-offs, **none restores a dead
  man.** Explicitly **not** a tech tree.
- **`UKesselCrewComponent` + `FCrewMember`** on the pawn; one new `ApplyCrewHit` call into the existing
  cone path; UMG stencil **roster strip + loss banner** for fast legibility.

### M1 slice
Crew component + 5-man roster + capability mask; `ApplyCrewHit` wired to the fragment cone; Wounded/
Killed state machine; **two degradations fully wired with zero new art — Loader→manual reload and
Gunner→degraded lay**; commander-pen → existing death/abort; **roster strip + loss banner** (UMG
primitives); **static crew presence + effects props** with transform/material wound/dead states and
hide-on-death (no rig per the locked no-skeletal-mesh constraint); minimal cigarette stabilise-wounded
sink if supply exists. Driver-split, radio MG/map, morale/cold, field-mods, and full animation deferred.

### Dependencies on other arms
- **Ballistics (blocking-ish):** restore `FVector LocalPos` (+`FName Kind`) on `FCrewModule` so the
  cone can hit crew nodes; raise an `ApplyCrewHit` callback on crew-kind cone damage. Interim: crew
  comp holds the offsets (unblocks M1).
- **Cigarette arm:** owns supply/brands; we own the crew-care sinks + per-man brand flavor.
- **Health arm (if separate):** shared wound severity / bleed / cold-drain model.
- **Movement arm:** driver-down mode-split (optic stow / hull-park) + field-mod mass costs.
- **World-map/intel arm:** radio-down map-degrade state.
- **Audio (P3):** per-station death cues + the emptying soundscape.
- **Atmosphere arm:** cold drain pressure (no betrayal-light tie — crew never EVA, only the player).

### Open questions for the user
1. **Driver-down mode-split feel:** hard mode toggle (drive OR fight) vs a softer "degraded
   concurrency" (both live but penalised)? Recommend the hard split — clearer, more dramatic.
2. **Manual reload input:** single hold-to-reload (M1-simple) vs a two-beat pull/ram sequence
   (more tactile, more cost). M1 hold, later sequence?
3. **Cold/morale drain in M1 or later?** Adds attritional pulse to the quiet drives but needs the
   health model; defer or thin-pass?
4. **Wound revive policy confirm:** cigarettes stabilise/hold but **never** revive a Serious→Dead — keep
   permadeath absolute (recommended), or allow a rare "pull a man back from Serious" for hope?
5. **Commander bleed-out window** for drama, or keep commander-pen a clean instant run-end (M1 default)?
6. **Pillar edit:** OK to rewrite P2's "you are the crew of one" to "you decay toward the crew of one"?

### Risks
- **Legibility of silent crew (the user's caution A)** — mitigated by teach-before-loss, multi-channel
  identity redundancy, staged-death beats, and the always-glanceable roster strip (§8.1). Residual risk
  if M1's static-prop crew reads as set-dressing; the roster strip + capability banner are the safety net.
- **Unmistakable capability-loss feedback (caution B)** — mitigated by loss banner + next-action
  self-demonstration + persistent capability tags + diegetic reinforcement (§8.2).
- **Art scope (skeletal crew)** — full idle/work/reaction animation is the biggest deferral and gates
  the *full* emotional payoff; M1 ships the mechanical + static-prop version and is explicitly designed
  to upgrade into the rig. Honest: the *feeling* is only ~70% there until the animated crew lands.
- **Cross-arm coupling** — crew depends on ballistics geometry, cigarette supply, health model, movement,
  and the map arm; the capability-mask design isolates us (everyone reads one mask) but the M1 wiring
  order needs coordination so we don't block on arms that aren't ready (hence the two zero-art
  degradations chosen for M1).
- **Difficulty self-balancing** — letting the casualty curve *be* the difficulty curve is elegant but
  could spike too hard if early losses cascade; the wounded-not-killed model + crew-care ration are the
  pressure valves, but this needs playtest tuning once compilable.
```
