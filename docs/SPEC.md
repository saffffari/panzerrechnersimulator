# der Kessel — Game Specification

> **Current campaign authority (2026-09-02):** `docs/STATUS.md` and
> `docs/LINUX_4090_HANDOFF.md` supersede this document wherever they differ. The canonical
> destination is Berlin and the unwinnable Reichstag last stand. A hidden escape by surviving crew
> to an intact cottage is only an optional, near-impossible, deliberately ambiguous hope coda.
> Earlier Halbe, hometown/pasture-as-main-ending, conventional final-tank-battle and
> one-man-protagonist concepts below are retained as historical design or implemented-prototype
> context, not the current target.

*Written 2026-07-07, the night the whole game arrived at once. This document is the design
authority for the full arc. `docs/ENGINE.md` remains the engine authority. When this doc
conflicts with `src/`, this doc is the destination and `src/` is the current mile marker.*

---

## The one-line thesis

**A stealth sniper game where the rifle weighs 57 tonnes** — one man walks home from Stalingrad
inside a Tiger, and every shot he fires is a plan, because the first one he misses brings all of
Mother Russia down on him.

3D pixel art (texel splatting), path-traced impressionism, sound as 85% of the fear.

---

## Pillars (revised)

1. **One shot, planned.** Combat is not a DPS exchange. It is: scout → mark → triangulate →
   pre-solve the firing solution → position → kill → vanish. A missed shot is not lost damage,
   it is a *strategic event* — your position burns, the swarm converges, the silent map becomes
   a manhunt. The 88 is the most powerful gun in this war and it changes nothing if they know
   where you are.
2. **The dismount supports the mount.** On foot you are soft, slow, and honest. Everything the
   Tiger does well is bought with information only the man on foot can gather. Mounted gameplay
   is the payoff; dismounted gameplay is the setup. Neither is complete alone.
3. **The land fights back.** Cratered, pocketed, undulating terrain is cover, route, drainage,
   and gunnery problem all at once. Nothing is flat. Some ground is only crossable at night.
4. **Everything is scarce.** Fuel, ammunition, spare parts, tungsten rounds, cigarettes, crew,
   luck. The economy is the campaign.
5. **Permadeath, softened only by knowledge.** The run ends when the man dies. What persists is
   what you learned — routes, depots, habits of the enemy. (Per-act retry rules per
   `kessel-3act-narrative`; the finale is one attempt, forever.)
6. **No dialogue, no waypoints, no HUD that lies.** Navigation by paper map, compass, landmark.
   History by scene, not text. (The Enigma remains the one language surface — and Act 3's command channel.)

---

## Part I — Gunnery: how a Tiger actually aims (and how we game it)

### The historical facts (grounding, verified against the standard references)

The Tiger I had **no rangefinder**. Not stereoscopic, not coincidence — nothing. What the crew
actually had:

- **TZF 9b** binocular gunsight (early production, 2.5×), later **TZF 9c** monocular
  (2.5×/5× selectable). The reticle: a central aiming triangle (*Zieldreieck*) flanked by
  smaller triangles at 4-mil spacing — used for lead, and for **stadiametric ranging**: a
  target of known size subtending a known fraction of the triangle gap gives range by
  arithmetic. A T-34 is ~3 m wide. That's your rangefinder: geometry and a trained eye.
- **The range drum (*Entfernungstrommel*)**: the gunner dials estimated range (graduated to
  4,000 m for PzGr. 39, further for HE) and the sight mechanically offsets elevation. Dial
  right = first-round hit. Dial wrong = a splash, and a bracket correction under pressure
  (*Gabelgruppe*: over, under, halve).
- **The commander's instruments**: S.F.14Z *Scherenfernrohr* scissor periscope (10×, used from
  turret-down cover — the periscope binoculars of artillery observers) and 6×30/10×50
  *Dienstglas* field binoculars. The commander estimates and calls range; the gunner dials it.
- **Doctrine**: below ~1,200 m the 8.8's flat trajectory forgives estimation error — crews
  trained for first-round hits under 1,000 m. Beyond that, ranging skill IS the weapon.

### The game mechanics

- **Stadia ranging (already half-present)**: the TZF reticle triangles become functional. Lay
  the triangle over a marked or visible target; the game does NOT print a range — you read the
  subtension like a real gunner, or you…
- **Map triangulation (the scout loop's payoff)**: a scouted mark is a *position on the paper
  map*. Your own position you fix by compass + landmarks (the existing navigation pillar). Map
  distance between the two = range. Measured on the map sheet with the printed scale — a
  finger-widths mechanic, diegetic, no magic numbers.
- **The range drum is a real control**: dial it at the sights (scroll/keys). The reticle aim
  point physically offsets. A pre-solved shot (scouted mark + triangulated range + drum dialed
  before you crest the rise) is a first-round kill. An improvised shot is an estimate.
- **Miss consequences**: a fired 88 is a thunderclap with a smoke signature. Every enemy
  within acoustic range gets a bearing on the shot (not your position — a bearing, feeding
  the platoon alert machine). Two shots from one position = a fix. The doctrine writes itself:
  *one shot, one kill, one move.*
- **PzGr. 40 (tungsten APCR)** is the scarce "sniper round" — historical, rare (tungsten
  shortage), flat-shooting, for the shot that MUST kill through the front plate. Loot-only.

## Part II — Stealth, engine-off, and the night

- **Silent watch (SHIPPED 2026-07-07)**: Z kills the Maybach. Detection envelope collapses
  (×0.42 radius, halved accrual). Restart is a 1.8 s crank and a 5-second acoustic *bark* that
  widens the envelope ×1.7 — starting the engine is a tactical decision. Dismounting under
  direct enemy LOS is the one hole in the silence ("seen exiting").
- **Scout marking (SHIPPED 2026-07-07)**: glasses up on foot, hold on an enemy 0.8 s → inked
  on the paper map as a red pencil x. A position *snapshot* — the ink doesn't follow a tank
  that moves. Stale recon is the risk you manage.
- **The Luger lie (SHIPPED 2026-07-07)**: the pistol hits, staggers, and has never killed
  anyone. No tooltip will ever say so. Firing it is loud. It exists to be believed in.
- **Night as terrain**: detection at night is already reduced (NIGHT_FACTOR). Extend: certain
  map regions — open steppe, bridge approaches, the railway embankment gap — are *daylight
  death zones*: uncrossable against the alert envelopes of their garrisons except at night.
- **Headlights**: the Tiger carried Bosch blackout equipment (*Tarnscheinwerfer* — the slit
  hood driving lamp; late production deleted even the second headlamp). Three-state control:
  OFF (blind), TARN (a dim slit pool ~10 m, mild signature), FULL (the road is yours and so is
  every gun within 4 km — certain death by design, present precisely so the player understands
  what the blackout drive is protecting them from).
- **Blind night driving**: the supported way across the death zones — *pre-planned routes*.
  Plan on the paper map by day (bearings between landmarks, counted in compass legs), then
  execute at night at TARN or darker, navigating by silhouette, sound, and the physical compass
  that appears only when the paper map is raised.
  The scout loop again: walk the route by day if you dare, learn it, drive it blind at night.

## Part III — The land (world overhaul)

- **Craters everywhere.** The entire operational area is shell-worked: continuous small-crater
  texture (displacement + normal, feeds the terrain splat), frequent vehicle-scale craters
  (hull-down positions — a crater IS a firing position), and rare **giant** craters
  (aerial-bomb / ammunition-train scars, tens of meters, whole micro-arenas with interior
  shadow, meltwater ice, wrecks in the bottom). Craters are cover, concealment, route
  obstacles, and gunnery backstops. Worldgen: crater field masks by bombardment-history zones
  (approaches to objectives heaviest), age variation (fresh black / old snow-softened).
- **Undulating, pocketed terrain**: the DEM base gets a medium-frequency relief pass — swales,
  balkas (the steppe gully systems, historically THE tank-country feature of the Don steppe),
  railway embankments, drainage cuts. Hull-down is found geometry, not built geometry.
- **Foliage, vastly denser**: real bushes (multi-mesh clumps, not billboards), dense forest
  blocks with genuine interior darkness (light shafts between trunks — the aperture-light
  system outdoors), treelines as concealment corridors. Winter Act 1: bare + snow-loaded firs;
  spring acts: full green density. Bushes are also a RESOURCE (see finale: camouflage).
- All of it must serve the sniper loop: the map READS as firing positions, approach routes,
  and dead ground. Map design v2 memory (cover/compartments, clusters ≥3) stands, multiplied.

## Part IV — Economy

- **Fuel.** The Tiger drinks ~500 L/100 km cross-country; this becomes the campaign's tempo
  governor. Fuel gauge diegetic (instrument), consumption simulated from throttle/terrain.
  **Oil/fuel depots** scattered per act (marked landmarks-of-rumor on the map margin, never
  X'd): captured Soviet stock, abandoned German columns, a downed Ju 52's drop canisters.
  Hand-pumping fuel is a timed, loud, vulnerable ritual (jerrycan relay — engine off,
  dismounted, exposed). Route planning = fuel arithmetic on the paper map.
- **Loot split (the rule)**:
  - **Enemy vehicles/positions** → *consumables*: cigarettes (the soft-currency/morale token),
    medical, food, intel scraps (pre-inked enemy marks on YOUR map — captured recon).
  - **German wrecks** → *hardware*: 8.8 ammunition, PzGr. 40 if blessed, spare parts
    (road wheels, track links, filters), repair materials, and Tiger-specific components.
    Doctrine-true: nothing Soviet fits your gun.
- **Salvage from operational Tigers**: finding a live spare Tiger when you don't need the hull
  (the Last Tiger rule already spawns them) lets you strip ONE major component as an
  **upgrade** — welded on at a long, loud, stationary field ritual, then *factored into the
  simulation* (mass, mobility, armor model, sound):

  | Upgrade (salvage) | Historical anchor | Simulation effect |
  |---|---|---|
  | HL 230 engine swap | HL 210 P45 → HL 230 P45 (production upgrade, +~50 hp) | top speed, acceleration, fuel burn, engine note |
  | Add-on armor: track links + spare wheels | universal crew practice on turret/hull | +effective armor on covered arcs, +mass, silhouette change |
  | Cast cupola + AA ring | drum → cast cupola with periscopes (mid-43) | commander survivability, head-out MG |
  | Steel-rim roadwheels | late-production resilient wheels | track HP, mine resistance, distinct running sound |
  | S-mine dischargers | hull-corner anti-infantry launchers (early 43) | the anti-swarm panic button, very limited charges |
  | Nahverteidigungswaffe | roof close-defense launcher (late 43): S-mine, **smoke**, flares | 360° infantry denial + self-screening smoke |
  | Turret bin + stowage | Rommelkiste etc. | carry capacity for the loot economy |
  | TZF 9c + drum refinement | 9b → 9c sight evolution | 5× zoom stage, finer range drum |
  | *Fiction, flagged:* L/71 barrel | never fielded on Tiger I (that's the II) | if we ever want it: end-game, loud about its ahistoricity |

  (PzGr. 40 stays ammunition-loot, not an upgrade — scarcity is its identity.)

## Part V — Damage & field repair

Module damage stops being a percentage and becomes **work**:

- A hit that wounds the engine *degrades* it (speed cap, smoke plume signature, louder — a
  wounded Tiger is easier to find). It does not heal by menu.
- Repair requires: engine OFF, dismount, the right **parts** (loot economy), the repair kit,
  and a **physical puzzle ritual** at the damaged system. The shipped track template is now:
  grab the loose run, pull it back over the wheels with repeated mouse strokes, place the
  replacement shoe, and hammer its pin home. It is diegetic, interruptible, LOUD at the wrong
  moments, and leaves the player exposed. Different modules still need distinct rituals (engine
  access-hatch sequence, fuel-line patch, optics realignment against a landmark).
- Triage IS the gameplay: limp home on a wounded engine at night, or repair in enemy country
  with your pants down.

## Part VI — Command, not crew (multicrew CUT 2026-07-07)

The crew-recruitment arc was considered and **cut** the same night it was proposed (user):
it would redesign the entire gameplay setup, exist for only a fraction of the run, and sink a
solo indie project. **You are alone in the hull for the whole game.** The solo embodiment IS
the game — every station worked with your own hands, forever.

What survives (and was always the plan): **the platoon**. In Act 3 you link up with two other
Tigers and command them **by radio, through the Enigma machine** — typed orders, a low-level
LLM/parser, two AI tank-commander brains ([[kessel-platoon-command]] design). This is also the
clean resolution of the no-dialogue pillar: the Enigma was already the ONE sanctioned language
surface in the world. You never gain a loader; you gain an army of two, addressed the way a
commander actually addressed one — in five-letter groups, blind, trusting.

## Part VII — The campaign arc (revised finale)

Acts 1–2 stand as designed (Stalingrad breakout; the leveled spring homeland and the empty
rendezvous). The finale is rewritten:

**Act 3 is your hometown.** Not "a small town in the eastern German countryside" — *your* town.
(The locked Halbe map stays: same real terrain, same forest approaches — the revision is
narrative, not geographic. The Kessel von Halbe becomes the Kessel around your own street.)
You make it back — the whole game was the walk home — and the war has followed you. Word
arrives over the platoon net: the assault comes in a day, maybe two.

The finale is a **defense you author**:

- **Preparation phase (the day)**: site your positions (the terrain rules from Part III now
  run in reverse — you know this ground because it's *yours*), gather cut bushes and weld
  spare track for camouflage and armor, dig the Tiger hull-down (a crater or a dug scrape),
  cache fuel and ammunition at fallback positions, plan the night-move routes between them,
  brief your platoon (place the other two Tigers).
- **The battle (one attempt, forever)**: everything the run taught and everything it gave —
  crew, upgrades, tungsten rounds, marked approaches — spent in one night and one dawn,
  defending your wife, your son, your daughter. By any means necessary. At all costs.
- Permadeath rule inversion for the final beat: the run can end with the Tiger dead and the
  man alive — *whether the town stands is the ending*.

**Boom. That's the game.**

## Sequencing (proposed order of build)

1. **Now**: texel-splatting prototype branch (visual identity) ∥ gunnery model (range drum +
   stadia reticle + map triangulation — small, ships the sniper thesis onto systems that exist).
2. Terrain/crater/foliage overhaul (worldgen — feeds everything downstream).
3. Fuel + loot split + repair rituals (economy layer over existing modules/pickups).
4. Night systems (death zones, headlight states, route planning tools on the map).
5. Salvage upgrades (component model + welding ritual + sim hooks).
6. Platoon command via Enigma (act-3 systems; [[kessel-platoon-command]]).
7. Act 3 hometown: prep phase systems + the one-attempt battle.

## Open questions

1. ~~Finale read-check~~ **RESOLVED (2026-07-07)**: the two Tigers DELIVER the warning; they
   are not the threat. (Betrayal twist considered and rejected — "a twist for twist's sake;
   the history is far more devastating.")
2. ~~Crew vs no-dialogue~~ **MOOT**: multicrew cut; the platoon speaks only through the Enigma,
   which was always the sanctioned language surface.
3. **Cigarettes as currency**: barter with whom? Proposal: they're morale/steadiness (hands
   shake less on the stadia read after a smoke — a quiet ritual at engine-off), not trade.
4. How much fuel arithmetic is fun before it's spreadsheet? Prototype the gauge + one depot
   loop in Act 1 and playtest before spreading depots across all acts.
