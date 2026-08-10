# Kessel — WORLD / MAP BLOCKOUT

Status: **DESIGN (method + spec only)**. No actors are placed by this document.
This team designs a **blockout METHODOLOGY that Claude (main) executes** against the single live
editor; it never touches the editor itself (concurrent mutation would collide). The human ("the
user") curates after each automated pass.

Grounding (do not contradict): `DESIGN_PILLARS.md`, `GAME_DESIGN.md`, `ART_BIBLE.md`,
`MILESTONE_1_PLAN.md`, `ARCHITECTURE.md`, `research/02-unreal-mcp-bridge.md`,
`research/06-volumetric-fog-scatter.md`. Scale convention is engine-locked: **`1 uu = 1 cm`**
(MILESTONE_1_PLAN §"Scale reference"). Player hull `630×300×250 uu`, mass ~57 t.

The single most important enabling fact: **the whiteout is load-bearing.** Visibility crushes to
**40–80 m** (`ART_BIBLE §4a`). On a 13-kilometre map the player can only ever *see* a 40–80 m bubble
plus a few tall silhouettes bleeding through the fog. That is what makes a handcrafted map this large
*possible* — 99% of it is never rendered at detail, crude massing reads as atmosphere, and World
Partition only ever streams the bubble you are standing in. Every decision below leans on this.

---

## 1. World size — how big, and why

### 1.1 The run is sized by PATH LENGTH, not by clock time

A handcrafted map has a **fixed geometry**. The 1.5–2.5 hr spread is produced by *player behaviour*
(how fast they push, how much they loot/fight), not by changing the map. So we size the world to a
target **critical-path length** and let the time-band fall out.

**Run-time budget (median target 2.0 hr = 120 min):**

| Activity | Share | Time | Notes |
|---|---|---|---|
| Careful driving | ~60% | ~72 min | the baseline dread loop (`GAME_DESIGN`: slow-burn driving) |
| Rare lethal combat | ~18% | ~22 min | ~5 gated fights × ~4.5 min (`GAME_DESIGN`: rare, decisive) |
| EVA looting on foot | ~17% | ~20 min | ~5–7 dismounts × 3–6 min (Luger-only ruin scavenging) |
| Idle / nav / planning | ~5% | ~6 min | reading the fog, listening, route choice |

**Driving distance from the budget.** Moving speed is region-dependent (a heavy Tiger in winter):

| Terrain | Moving speed | Where |
|---|---|---|
| Graded steppe road (the Rollbahn) | 28–38 km/h | open steppe regions |
| Off-road snow / churned ground | 15–22 km/h | steppe shoulders, detours |
| Town / industrial ruins / forest | 6–12 km/h | urban + woods crawl |
| River bank / ice / chokepoint | 4–8 km/h | the crossing, defiles |

Weighted average over a full run ≈ **~18 km/h moving**. With ~72 min (1.2 hr) actually driving:
**path driven ≈ 18 × 1.2 ≈ 22 km.**

**West displacement from path.** The route is deliberately non-linear — it weaves around rubble, takes
a forced town detour, fords up/downstream of a blown bridge, threads a forest defile. **Winding factor
≈ 1.7** (path length ÷ straight-line west displacement). So net westward displacement:

> **~22 km path  ÷  1.7  ≈  13 km straight-line, centre-start → western edge.**

**Time-band check:** a cautious looter at ~14 km/h effective with 7 EVA stops and 6 fights lands near
**2.5 hr**; an aggressive driver who skips loot, fights only what blocks the road, and averages ~24 km/h
covers the same fixed 22 km path in **~1.5 hr**. The map does not change; behaviour spans the band. ✔

### 1.2 Recommended world bounds (concrete)

| Dimension | Metres | Engine units (uu) | Rationale |
|---|---|---|---|
| **West displacement, start→win** | **13 km** | 1.3 × 10⁶ uu | from §1.1 |
| **East buffer behind start** | ~2 km | 0.2 × 10⁶ uu | the collapsing pocket at your back; soft no-go (you *came* from there, it is overrun) |
| **Playable E–W extent** | **~15 km** | 1.5 × 10⁶ uu | 13 + 2 |
| **Playable N–S width** | **~8 km** | 0.8 × 10⁶ uu | room for winding, 2–3 branch routes, region spread |
| **Authored WP world bounds** | **16 × 12 km** | 1.6 × 10⁶ × 1.2 × 10⁶ uu | adds an **impassable frame** (the Kessel ring, deep forest, marsh, blizzard void) around the ~100 km² playable corridor |

**Total authored ≈ 192 km²; truly traversable corridor ≈ 100 km².** A 16 km axis = 1.6 × 10⁶ uu sits
comfortably inside UE5 **Large World Coordinates** (double-precision world space; World Partition is
validated to tens of km). This is *not* near any engine limit — it is bounded by **content authoring
effort**, which is exactly why the methodology in §4 is automation-first.

### 1.3 Scope honesty (M1 vs later)

A 100 km² handcrafted world is a **multi-month production track, not Milestone 1.** M1 ships the
`L_KesselTest` arena (`MILESTONE_1_PLAN` step 2) plus, at most, **one vertical-slice region** (a single
town block + an adjoining steppe stretch with one chokepoint) whose only job is to *prove the blockout
pipeline and World Partition streaming at the real look*. The full west-running map is built in the
phased plan (§5) as a post-M1 track. See `m1Slice` / `laterSlice` in the return payload.

---

## 2. Regions, the spine, and the route

### 2.1 The signature spine — "retrace the ruins you made coming in"

The **primary west road is the *Rollbahn* — the very axis your battlegroup advanced *east* along.**
Driving west in retreat, you pass, **in reverse chronological order**, the wreckage of your own
offensive: the freshest kills (yours, last week) near the centre, older and more weathered destruction
the further west you go, until at the perimeter you reach the start line of the whole doomed advance.
The map *is* the story (`GAME_DESIGN`: "told by the world, not by words", no dialogue).

Signature reverse-order beats along the spine (placed west-decreasing in "age"):
- **Centre:** your battlegroup's last leaguer — fresh-burnt German wrecks, *your* unit markings, a
  still-smouldering supply truck (warm emissive at night, `ART_BIBLE §4b`).
- **A knocked-out sister Tiger** mid-route — same silhouette as the player's tank, turret blown off; a
  mute mirror of your possible fate (permadeath made visible).
- **Older Russian positions you overran** — dug-in AT guns facing *east* (toward where you came from),
  now snow-filled and abandoned.
- **The perimeter breach** — the shell-cratered gap in the original front line where the offensive
  began; cross it and friendly troops are there. The advance ends where it started.

This spine is one continuous environmental-narrative gradient; everything else hangs off it.

### 2.2 Regions, centre → west (escalating, oscillating tension)

Eight regions, ordered for **dread → rare lethal spike → release → dread**, with a hard climb in the
back third. Lengths are *west displacement* (sum ≈ 13 km).

| # | Region (codename) | Biome | West len | Tension / role | Signature chokepoint | Loot / EVA |
|---|---|---|---|---|---|---|
| 0 | **The Leaguer** (centre start) | frozen steppe | 1.5 km | Quiet tutorialisation. Your dead battlegroup. No combat. | — (open) | tutorial cache; learn EVA on a safe truck |
| 1 | **Krasny** (ruined town) | ruined town | 1.5 km | First urban dread; first *possible* lethal fight. | Main-street rubble defile | houses (EVA), cigarettes, ammo |
| 2 | **The Long White** | frozen steppe | 2.5 km | Pure isolation drive; the dread floor. One derelict supply dump. | — (exposure, not geometry) | depot (big loot, exposed tank) |
| 3 | **The Tractor Works** (industrial) | industrial / rail yard | 2.0 km | Densest ruin; vertical; the KIT showcase. Major fight. | Factory gate + rail underpass | components, best loot, ambush risk |
| 4 | **The Crossing** | frozen river | 1.5 km | Hard chokepoint; blown bridges; ice risk; ambush. | The river itself (ford/ice) | sappers' cache on far bank |
| 5 | **The Black Belt** | forest / taiga | 2.0 km | Claustrophobic close-range dread; lethal close fight. | Forest defile / felled-tree barricade | hidden partisan stashes |
| 6 | **The Approach** | open steppe | 1.5 km | Opening out; the lines in sight; climactic gauntlet. | Last ridge / minefield gap | last-chance ammo |
| 7 | **The Perimeter** (friendly lines) | Kessel perimeter | 0.5 km | Resolution. Cross the old front line = **WIN**. | The breach | — (the goal) |

Region transitions are read in the fog by **landmark silhouettes** (§3.2), not signage — you steer
toward the factory chimney, then toward the bridge, then toward the treeline.

### 2.3 Road / rail network + route choice

- **Primary: the Rollbahn** (the spine, §2.1) — a graded steppe road running broadly west, fast but
  **exposed** (long sightlines = you can be seen/heard first). One continuous drivable ribbon end to end.
- **Secondary branches** — at 3–4 points the road forks into a *slower-but-concealed* alternative
  (off-road through a gully, a back-street through ruins, a logging track through the Black Belt). The
  trade is always **speed-and-exposure vs. time-and-concealment** — the core moment-to-moment decision,
  and it directly serves the 1.5–2.5 hr spread.
- **Rail line** — a single-track railway runs roughly parallel and *converges* on the Tractor Works
  (Region 3), where it becomes a **rail yard** (sidings, wagons, a water-crane landmark). Embankments
  are linear cover and linear *obstacles*; a **blown rail bridge** near the river forces you off it.
- **Chokepoints gate the rare lethal fights.** Combat is authored *only* where geometry funnels you:
  town defile (R1), factory gate + underpass (R3), the river crossing (R4), the forest defile (R5),
  the perimeter breach (R6→7). Open regions (R0, R2, R6-early) carry **dread and exposure**, not fights.
  This is the pacing contract: **you cannot be ambushed in the open; you can only be ambushed where the
  map forces you to slow down** — which makes the slow-downs frightening.

---

## 3. Landmarks — navigation in a whiteout

### 3.1 Why landmarks are a survival system, not decoration

Permadeath + no minimap-realism + 40–80 m fog = **getting lost is a death sentence** (you burn fuel,
time, and stumble into chokepoints unready). Navigation is therefore carried by **tall silhouettes that
punch through the fog ceiling** and by **emissive beacons at night** (`ART_BIBLE §3a` "silhouette-on-fog",
§4b warm accents). Each region gets 1–2 unmistakable, fog-readable hero silhouettes the player steers by.
This is the crew-of-1 navigation burden made legible (`DESIGN_PILLARS §2`) — there is no co-driver
calling the route.

### 3.2 Landmark set (silhouette-first, doubles as story)

| Landmark | Region | Nav role | Story / emissive |
|---|---|---|---|
| **Burning supply truck** | 0 | "start here" | your battlegroup's fresh death; amber fire at night |
| **Grain elevator / church silhouette** | 1 | town heading | the town you stormed; bell-tower stump |
| **Lone water tower** | 2 | the only vertical in the white void | a single distant lamp = "depot near" |
| **Factory chimney + smokestacks** | 3 | tallest thing on the map; visible 2+ regions out | the Tractor Works; smoulder glow |
| **Rail water-crane + wagons** | 3 | yard orientation | derailed train, frozen |
| **Blown bridge spans** | 4 | "the river is here; do not drive onto it blind" | sappers' demolition; your engineers did this coming in |
| **Crashed aircraft** | 5 | forest waypoint | a downed transport, scavenged |
| **Knocked-out sister Tiger** | 3–4 mid | mid-route fix | the permadeath mirror (§2.1) |
| **Perimeter watch-fires / friendly flare** | 7 | the win beacon | the only *welcoming* light in the game |

Landmarks must be authored as **HLOD-friendly tall geometry** so World Partition can show their
silhouette from far away cheaply (a posterised black shape in fog needs almost no detail — §4.3).

---

## 4. BLOCKOUT METHODOLOGY (the part Claude executes via MCP)

This is the runnable recipe. It is **automation-first**: Claude mass-produces structure with batch
tools; the human curates hero detail. The division of labour is explicit at every phase so the single
live editor connection is never contended — **Claude runs the batch passes; the user hand-edits between
passes.**

### 4.0 Toolset prerequisites (verified, `research/02`)

Enable, in the in-editor MCP plugin (`ModelContextProtocol` + `ToolsetRegistry`), the minimal set for
blockout work:
- **`EditorToolset`** — spawn/transform/inspect actors (this is the "ProgrammaticToolset batch-place"
  capability: scripted actor spawning + transforms). VERIFIED on-disk (`research/02 §A3`).
- **`PCGToolset`** — Procedural Content Generation graphs/volumes for scatter. VERIFIED (`research/02 §A3`).
- **`PhysicsToolsets`** (collision sanity on massed geometry), **`MaterialInstanceTools`** (region tint
  MIDs so greybox regions are colour-coded).
- **Fallback for anything the toolsets don't expose: `runreal/unreal-mcp` → PythonScriptPlugin Remote
  Execution** (the full `unreal` Python API), for grid/spline batch loops and World Partition setup
  that the fixed toolset verbs can't express (`research/02 §A5/Part B`). Node.js is the only prereq
  (`MILESTONE_1_PLAN §4`).

> **R-M2 caveat (carried):** material-graph / BP-graph read-back is weak this release — treat all
> automated placement as **fire-then-human-verify**, never trust agent self-report on graph edits.

### 4.1 World Partition setup for a 16×12 km map

Claude executes (Python remote-exec where toolset verbs are insufficient):

1. **Create the world as a World Partition level** (`L_KesselWorld`), partitioned, with **Large World
   Coordinates** on (default in UE5). Streaming source = the player pawn.
2. **Runtime grid / cell size:** because the *visible* bubble is 40–80 m, streaming can be aggressive.
   Set a **runtime hash grid cell ≈ 256 m (25,600 uu)** with a **loading range ≈ 512–768 m** — only
   ~2–3 km² is ever resident. This is what makes 100 km² affordable.
3. **HLOD layers:** generate **HLOD0 silhouette proxies** for all tall landmarks and ruin masses so
   their fog-silhouette renders past the streaming range (§3.1). Crude proxies — they only ever read as
   posterised shapes in fog.
4. **Data Layers** (orthogonal toggles, authored once):
   - `DL_Region_0…7` — per-region content, so a region can be worked on / streamed in isolation.
   - `DL_TimeOfDay` variants and `DL_Weather` hooks for the **dynamic atmosphere system** (day/night +
     weather; the new directive) — e.g. night-only emissive beacons, blizzard-state props.
   - `DL_Damage` — pristine vs. wrecked states for the retrace spine (not needed M1).
5. **Origin & stakes:** place `PlayerStart` at the **centre** (`0,0`), a `TargetPoint` "WIN" marker at
   `(-1,300,000 uu, 0)` (13 km west), and 8 region-origin `TargetPoint`s along −X. West = **−X** by
   convention; all region offsets are negative-X.

### 4.2 Macro mass-out with BasicShapes (the same kit as the tank)

Reuse the engine `BasicShapes` already in the project kit (`Cube/Cylinder/Cone/Sphere`,
`MILESTONE_1_PLAN` step 0). **No DCC art for the blockout.** A region-tinted MID per region
(`MaterialInstanceTools`) colour-codes the greybox so the user can read regions at a glance.

Placeholder vocabulary (so massing is consistent and legible):

| World element | BasicShape | Typical scale (uu) |
|---|---|---|
| Building mass | Cube | 1500–4000 × 1000–2500 × 800–3000 |
| Chimney / tower / silhouette landmark | Cylinder | Ø600–1500, height 4000–12000 |
| Rubble pile / spoil | Cone + Sphere cluster | 300–1500 |
| Road / rail ribbon | Cube (thin) or spline mesh | width 600–1200, thickness 20 |
| Wreck (tank/truck) | Cube + Cylinder (turret) | hull box ≈ player hull |
| Tree (forest greybox) | Cone on thin Cylinder | Ø400–1200, height 1500–4000 |
| River channel | scaled Plane (cut into terrain) | width 4000–9000 |
| Region boundary volume | translucent Box (tinted) | region-sized |

The **batch-place primitive** (the user's "ProgrammaticToolset batch-place"): a Python remote-exec
loop that, given (mesh, list-of-transforms), spawns `AStaticMeshActor`s in one pass — e.g. lay the
road ribbon along the route spline by sampling the spline every N metres and dropping segments; stamp
a town's building blocks from a grid; line an embankment with rubble cones. One script, many actors,
human-reviewed after.

### 4.3 Assembling a city/ruin from an imported building KIT

The town (R1) and especially the Tractor Works (R3) are built from a **modular ruin KIT** imported once
(post-greybox): `Wall_*`, `Corner_*`, `Floor_*`, `Roof_*`, `Window_*`, `Rubble_*`, `Pillar_*` modules
on a consistent grid (e.g. 400 uu). Authored at modern fidelity, crushed by the retro post
(`ART_BIBLE §6`), and aggressively LODed because **fog hides the interior detail** (`ART_BIBLE §6a`:
enemy/env "6k–12k, seen through fog → aggressive LOD").

Two-tool assembly:
1. **ProgrammaticToolset / EditorToolset batch-place — the buildings.** A Python remote-exec generator
   takes a **block footprint** (a rectangle or a hand-drawn spline loop) and walks the KIT grid: places
   `Wall`/`Corner`/`Window` modules around the perimeter, `Floor`/`Roof` per storey, swaps a tunable
   fraction to `Rubble_*`/blown variants to author the *ruined* state, and randomly omits modules
   (blast gaps). Inputs: footprint, storeys, `ruinFraction`, seed. Output: one assembled ruined block
   per call. Run it per block to mass the whole district from a handful of footprints. (Deterministic
   seed so the user can re-run with the same result after tweaking a footprint.)
2. **PCGToolset scatter — everything loose.** A **PCG graph** per region, driven by a **PCG volume** the
   user paints over the district, scatters the non-hero mass: rubble debris, brick spill, snow drifts
   against walls, dead trees, telegraph poles, fences, burnt vehicle hulks, shell craters, frozen
   corpses-of-equipment. PCG reads spline exclusion zones (keep the road/route clear) and density maps
   (denser rubble near blast centres). This is how a believable ruin-field appears without hand-placing
   thousands of props — and it regenerates when the user moves a building.

**Forest (R5)** is pure PCG: a `PCG graph` scatters greybox cone-trees by density across the region
volume, with a spline-driven clearing for the defile/route. **Steppe drifts & spindrift** are PCG scatter
of low snow-mound meshes plus the camera-anchored blizzard Niagara (`ART_BIBLE §6d`, owned by atmosphere).

### 4.4 How the user "chips away" (curation hand-off)

After each automated pass Claude **stops and hands the editor to the user.** The contract:

- **Claude owns:** World Partition skeleton, region data layers, the route/road/rail ribbons, BasicShapes
  macro massing, KIT batch-assembly of blocks, PCG scatter graphs, HLOD generation, landmark silhouette
  stakes — i.e. **everything mass-produced and re-runnable.**
- **The user owns:** hero curation that should not be automated — carving the *exact* drivable route line,
  hand-placing the **story beats in reverse order** (§2.1), the **EVA loot interiors** (a tank can't enter,
  so ruins need authored on-foot spaces + a tank-parking apron outside), **combat staging** at each
  chokepoint (cover, enemy loom positions, sightlines tuned to the 40–80 m reveal so fights are *heard
  then seen*, `DESIGN_PILLARS §3`), and per-region **dynamic lighting / time-of-day** mood.
- **Never simultaneously:** Claude's batch passes and the user's hand-editing are **sequential**, never
  concurrent on the same level — this is the hard constraint that protects the single live editor
  connection.

---

## 5. Phased blockout plan (macro → regions → landmarks → detail)

Every phase ends at a **drivable gate** — the massive map is *always* traversable end-to-end, so route
length and pacing can be playtested from day one.

**Phase 0 — Skeleton & scale stakes (Claude).**
World Partition `L_KesselWorld`, LWC on, 256 m grid, region data layers, origin/PlayerStart/WIN stakes,
8 region-origin markers, one giant snow ground proxy (or low-res Landscape). The route spine drawn as a
single **spline** centre→west. *Gate:* you can fly the spline 13 km and WP streams cells correctly.

**Phase 1 — Macro layout (Claude batch + user route curation).**
Lay the **road ribbon** along the spline (batch-place segments); stamp **region boundary volumes**
(tinted boxes); plant **landmark silhouette stakes** (tall tinted cylinders/cones) for every §3.2
landmark so navigation is testable in fog; rough the **river channel** (cut Plane) and the **rail line**.
User then hand-curves the *exact* route and the 3–4 branch forks. *Gate:* **drive the box-tank
centre→west through empty massing, steering only by silhouettes in the fog**, end to end.

**Phase 2 — Regions (Claude batch per region, user between).**
Per region, in `DL_Region_n`: BasicShapes mass-out (§4.2) of the town/industrial/forest/crossing
geometry; KIT batch-assembly for R1 + R3 blocks (§4.3 step 1); PCG scatter graphs for rubble/forest/
drifts (§4.3 step 2); chokepoint geometry that funnels the route. User refines each region's route
threading and parks the chokepoints. *Gate:* every region is individually drivable and the chokepoints
actually funnel; PCG regenerates cleanly.

**Phase 3 — Landmarks (Claude builds, user places hero beats).**
Replace silhouette stakes with built-up hero landmark massing (water tower, chimney, blown-bridge spans,
sister-Tiger wreck, perimeter watch-fires); generate **HLOD0 proxies** so they read through fog from
range; wire night-only emissive beacons into `DL_TimeOfDay`. *Gate:* you can navigate the whole map by
landmark silhouette in an 80 m whiteout, day and night.

**Phase 4 — Detail & curation (user-led, Claude assists on request).**
The user chips away: reverse-order story beats (§2.1), EVA loot interiors + tank aprons, combat staging
tuned to the audio-reveal contract, per-region dynamic weather/lighting, the retrace `DL_Damage` states.
Claude assists only with re-runnable batch/scatter tweaks. *Gate:* a full playtest lands inside the
**1.5–2.5 hr** band with the intended dread→spike→release pacing.

---

## 6. Pillar & system ties (honesty checklist)

- **Deep-sim / abstracted (P1):** the map is *huge in simulated traversal*, *crude in pixels* — massing
  reads through fog; the whiteout justifies short draw distance for free perf on 100 km².
- **Crew of one (P2):** no co-driver/navigator → landmarks + the spine *are* the navigation system; getting
  lost is a modelled failure, not a UI gap.
- **Sound as primary sensor (P3):** chokepoints are authored so fights are **heard before seen** at the
  40–80 m reveal; open regions carry exposure dread (you can be *heard* first on the fast road).
- **Cold/dark/few warm lights (P4):** landmark beacons and the retrace's burning wrecks are the warm
  emissive nav system at night; the new **dynamic day/night + weather** rides `DL_TimeOfDay`/`DL_Weather`.
- **Always inside the steel — except EVA (P5):** EVA loot sites are a deliberate, authored exception — ruin
  interiors the tank can't enter, with an outside **tank-parking apron** that visibly exposes the tank
  while you're on foot (Luger only). Designed in at Phase 4, not bolted on.
- **Historical grounding, not pedantry (P6):** Stalingrad-pocket geography (steppe, tractor works, frozen
  river, taiga, the encirclement ring) is plausible, not a survey.

---

## 7. Risks & open questions

**Risks:** authoring effort (100 km² is the project's largest content lift — automation-first is
mandatory, not optional); World Partition cooking/HLOD time on a map this size; PCG iteration cost if
region volumes are re-painted late; the retrace `DL_Damage` reverse-order beats are hand-work that
doesn't automate; fog-nav must be *tuned* so the map is legible enough to not cause cheap permadeaths.

**Open questions (for the user):** (a) confirm **−X = west** axis convention; (b) median run target —
hold **2.0 hr** or shift? (c) **branch-route count** (more replay vs. more authoring); (d) is the
**sister-Tiger wreck** mirror acceptable tonally; (e) M1 vertical-slice region choice — **Krasny town
(R1)** is the cheapest KIT proof; confirm; (f) Node.js install to unlock the Python remote-exec batch
path (gates §4.2/§4.3 automation).
