# Kessel — NAVIGATION SYSTEM (paper map + compass + pencil, fully diegetic)

**Team:** Navigation & Intel (the "navigation arm").
**Owns:** the diegetic manual navigation loop — the physical **paper map**, **compass**, and
**pencil** as raised interactive items; the **dead-reckoning + landmark-fix** model; the
**believed-position** belief state (NOT a true-position readout); the **pencil-mark persistence**;
the **landmark registry** the world arm populates; the **radio-operator navigation assist** and its
loss; and the C++/UMG architecture that hangs all of it off the pawn.
**Does NOT own:** the handcrafted world geometry or where landmarks physically sit (the **world-map
arm** authors those; we consume a registry of them — §6/§9.4); ballistics, movement, atmosphere
state (we *read* `MPC_Atmosphere` — §5); the crew mortality model (we *consume* the radio-op's
`FCrewCapabilities.bMapIntel` flag — §7); the EVA possession swap (we ride on top of it — §8.4);
input *binding* (the `AKesselPlayerController` owns IMCs; we own a gated "navigation" mapping
context it adds/removes — §9.3).

**Status:** DESIGN — **post-M1 system, architecture authored now** (VS/MSVC gate per
`MILESTONE_1_PLAN §0`; no C++ compiled this phase). Header sketches match the on-disk scaffold
conventions (`SCAFFOLD_REPORT.md`) and the patterns the **cigarette** (`teams/cigarette-system.md`)
and **EVA** (`teams/eva-looting.md`) arms already locked for *raised interactive items* and
*input-gated ritual state machines*. Nothing here blocks the M1 critical path; navigation depends on
the world map existing, which is itself a post-M1 track (`world-map-blockout §1.3`).

Grounding (do not contradict): `DESIGN_PILLARS.md`, `GAME_DESIGN.md`, `ART_BIBLE.md`,
`ARCHITECTURE.md`, `teams/world-map-blockout.md`, `teams/atmosphere-system.md`,
`teams/crew-system.md`, `teams/cigarette-system.md`, `teams/eva-looting.md`.

---

## 0. How this answers to the pillars

| Pillar | How manual navigation honors it |
|---|---|
| **1 — Deep sim, abstracted** | Navigation is a *modeled belief system*: heading, speed, elapsed time → an accumulating dead-reckoning **error**, corrected only by landmark fixes. Presented with three stylized props (a creased paper map, a brass compass, a wax pencil), never a satellite view. The *map never shows where you are* — your position is a thing you **infer and can be wrong about**. That is the sim. |
| **2 — Crew of one (decaying)** | Reading the map **occupies your hands and stops the tank** (exactly the cigarette cost). You cannot drive, lay the gun, or fire while the map is up. And the **radio-op is your navigator** — while he lives he fixes your position for you; when he dies (`crew-system §4.4`) you must dead-reckon **alone**. Navigation difficulty *ramps with attrition*, like everything else. |
| **3 — Sound is the primary sensor** | In the whiteout you often **can't see** the landmark you need — but you can hear a rail yard, a river, wind through a treeline (audio handoff). Sound becomes a *navigation* sensor, not just a threat sensor. The radio-op's relayed bearings are an *audio* aid (P3). |
| **4 — The cold, the dark, the few warm lights** | At night the landmarks you steer by **become emissive beacons** (`atmosphere §5`, `world-map §3.1`): a burning wreck, a surviving lamp, the perimeter watch-fires. Reading the paper map in the dark needs the cabin light — a warm pool on the paper that is *also* the beacon that betrays you (the §4 light-liability, scaled to a map table). |
| **5 — Always inside the steel** | The map is read **inside the cabin**, spread on your knees under the cupola — a held item in the lower frame raised to fill the view, exactly the cigarette's gloved-hand layer. No external map screen, no overhead camera, ever. EVA navigation (the deliberate P5 exception) reuses the same item raised on foot. |
| **6 — Historical grounding** | A real Wehrmacht tank commander navigated with a **printed 1:100,000 Heereskarte**, a **Marschkompass**, a wax pencil, and the radio for fixes — by terrain association and dead reckoning. No GPS existed. The whole system *is* the period method, dramatized. |

**The headline: there is NO minimap, NO GPS, NO objective marker, NO auto-position.** The map is a
piece of paper. Where you are on it is your problem. This is the most literal possible expression of
the no-HUD diegetic philosophy — navigation is a **skill the player builds**, and getting lost is a
**modeled failure** that can end a permadeath run.

---

## 1. The core fantasy

You have driven west for ninety seconds through nothing — a flat blue-white void, the fog wall at 60
metres, the compass needle you can't quite trust swinging in its housing. You think you have been
holding 270°. You think you have come maybe two kilometres since the burnt water tower. You are not
sure. You park, button up, and pull the folded map onto your knees. The cabin light throws a small
amber pool across it. There — the tower, pre-printed, a tiny symbol; there — the rail line you crossed;
there — a pencil tick where you *believe* you are, drawn from memory of heading and time. You raise the
compass, sight it through the open vision slit at a dark vertical shape just bleeding through the
whiteout to your front-left — a chimney? You find the factory chimney symbol on the paper, lay the
bearing against it, and the two lines almost cross where your pencil tick sits. Almost. You move the
tick a finger's width north, draw your next leg west-by-southwest to thread the river ford, fold the
map away, and drive on — into the fog, on faith and a pencil line, hoping the man who used to do this
for you were still alive to check your arithmetic.

That is the loop: **you build a belief about where you are, the world erodes it, and you spend
vulnerable seconds with a map and compass to rebuild it — and you can be wrong, and being wrong in a
permadeath run can kill you.**

---

## 2. The three items (raised like the cigarette)

All three are **physical interactive items** the player *raises into the lower frame* on a
gloved-hand layer (`ART_BIBLE §1` "hands appear only during interactions"; the exact precedent is
`UKesselSmokingComponent`, `cigarette-system §3`). Raising any of them **occupies your hands and gates
drive/fire/aim input** (§9.3) — navigation, like smoking, costs tempo and exposure. They are authored
in the abstracted/period style: creased paper, scratched brass, a stub of wax pencil; the hero is the
*reading*, not high-poly models.

### 2.1 The PAPER MAP — a period Heereskarte sheet

- **What it is:** a single folded printed map sheet (period Wehrmacht style — a `1:100,000`
  topographic *Heereskarte* look: hachured/contoured terrain, rivers in blue, rail in black tick-line,
  roads, settlement blocks, a printed kilometre grid, marginal scale + magnetic-north arrow). Rendered
  as a flat textured quad (the "paper") with the printed cartography baked into a texture, **crunched
  by the `MasterRetro` post like everything else** — at full retro it reads as a posterised, coffee-
  stained, period sheet; toward modern, finer linework resolves.
- **Raise:** holding the map bind lifts the folded sheet into the lower-centre frame and the view
  settles to a **map-read framing** (a near-top-down read of the sheet on your knees — this is the
  *Interior / instrument-focus* view mode, `ART_BIBLE §7d`, the darkest mode, keyed by the cabin
  light's warm pool on the paper). You are buttoned and stationary.
- **Pan / zoom the paper:** mouse **pans** the sheet under a fixed reading "loupe" frame (the paper is
  larger than the view; you drag it around); mouse-wheel / zoom bind **zooms** between *folded
  overview* (the whole leg of the journey) and *detail* (a single grid square, where you read terrain
  and place precise marks). Zoom is a smooth `FInterpTo` paper scale + the loupe, not a UI scroll bar.
- **Crucially: the map shows NO "you are here."** No blip, no arrow, no auto-centred dot. It shows the
  *world's* printed features and *your own pencil marks* (§2.3) — nothing else. (See §3 for why.)

### 2.2 The COMPASS — a Marschkompass you sight with

- **What it is:** a period marching compass (brass, lidded, with a sighting line and a graduated bezel
  — Bézard/Marschkompass family). A small held item raised into frame.
- **Raise + read:** holding the compass bind lifts it; the **needle settles** (a short damped swing) and
  you read your **current heading** off the card, and/or **sight a bearing** to a landmark: with the
  compass up you can line the sighting notch on a silhouette in the fog and the bezel reads the bearing
  *to that object* (the input you need for a landmark fix, §4.2). The needle is the only heading
  instrument — there is **no permanent compass HUD strip** by default (see §2.4 on the optional corner
  micro-compass).
- **The steel-hull deviation (the period tension, P6):** a magnetic compass **inside a 57-tonne steel
  box, next to an electrical system, is unreliable** — real crews knew this. So:
  - **Buttoned-up read:** the needle carries a **deviation error** (a believable few-to-many degrees,
    worse near the engine/turret mass, worse with the turret traversed off-centre). The reading is
    *usable but biased* — a cheap, fast read you slightly distrust.
  - **Head-out read (P5 escalation):** popping the hatch and reading the compass with your head clear of
    the steel gives a **clean** bearing — but exposes you (the `bExposed` cost, `view-component`). The
    accurate read is the dangerous read. This is the same risk grammar as the cigarette/headlamp.
  - Deviation is a modeled scalar (§9.2), not flavour text: it feeds the dead-reckoning error, so a lazy
    buttoned navigator drifts faster than one who pops the hatch to check.

### 2.3 The PENCIL — your only record

- **What it is:** a stub of wax/grease pencil (period chinagraph). Raised *with the map up* — the
  pencil is the map's tool, not a standalone view.
- **Place a mark:** with the map raised, the pencil bind enters a **mark mode**; mouse positions a
  cursor on the paper and a press **commits a pencil mark** at that point. Mark *types* (cycle):
  - **Position belief tick** — "I think I am *here*." The single most important mark; there is **one
    active** at a time (placing a new one supersedes the old, leaving a faint dotted trail of where you
    *thought* you were — your own breadcrumb of guesses).
  - **Route line** — drag to draw your intended path / the leg you mean to drive next (a planning aid;
    purely yours).
  - **Waypoint / X** — mark a thing to remember: a loot site you'll come back to, a chokepoint to avoid,
    a landmark you've confirmed. The player **invents their own objectives** here — there are no game-
    given waypoints, only the ones you pencil in.
  - **Bearing line** — drop a line from a landmark symbol along a sighted bearing (the construction line
    for a fix, §4.2).
- **Erase:** an erase bind + drag rubs out marks (the wax smears — a stylized smudge, not a clean undo).
  Erasing is deliberate and a little lossy, like a real grease pencil.
- **Persistence:** every committed mark is saved into the run (§9.5) — your map *accretes your whole
  journey* as a permadeath artifact. If you die, the last thing the run was is a creased map covered in
  your own anxious pencil, ending at a tick that may or may not have been where you actually were.

### 2.4 The optional corner micro-compass (kept minimal, diegetic)

`ART_BIBLE §5` allows "a corner micro-HUD (compass/heading) … acceptable if it reads as an instrument,
like the bottom-right compass in `6c4358845…`." Navigation's stance: **the raised Marschkompass is the
real tool; the corner micro-compass is an optional, toggleable, *deliberately degraded* convenience.**
If shipped, it is a tiny etched-glass heading tape on the periscope frame that shows **only the biased
buttoned heading** (carries the steel deviation, §2.2) — never a true bearing, never a position. It
exists so the player isn't *constantly* raising the compass for a rough "am I still going west," while
the accurate work still requires the raised item head-out. Default: **off / opt-in**, because the game
is diegetic-first and getting-lost is a feature. (OQ-1.)

---

## 3. Why the map never shows your position (the central rule)

This is the design's spine and it is worth stating as a hard rule, because every downstream system
exists to serve it:

> **The world knows where you are. The map does not. The gap between the two is the gameplay.**

- A "you are here" dot would collapse navigation into "drive toward the marker" and delete the entire
  pillar (`world-map §6` P2: "getting lost is a modelled failure, not a UI gap"). So we **never expose
  true position to the map UI.**
- Instead the player maintains a **believed position** (their pencil tick, §2.3) and the system
  maintains a hidden **true position** (the pawn transform). They start coincident (you know you begin
  at the centre leaguer) and **drift apart** as dead-reckoning error accumulates (§4.1). The player
  *closes* the gap with landmark fixes (§4.2) and the radio-op's help (§7).
- **The drama is the uncertainty.** Am I where my pencil says? The whiteout, the night, the steel-
  compass bias, and a dead radio-op all widen the gap; clear weather, a good landmark, and a living
  radio-op close it. The player *feels* their own navigational confidence rise and fall — and the game
  never tells them their true error (only the world's consequences do: you arrive at the wrong place).

Everything in §4–§7 is the machinery of that gap.

---

## 4. The dead-reckoning + landmark-fix loop

Two complementary methods, exactly as a real crew used them. Dead reckoning is *continuous and always
degrading*; landmark fixes are *discrete and corrective*.

### 4.1 Dead reckoning — heading × speed × time, with error

While you drive, the system runs a hidden **dead-reckoning estimator** that the player is *meant to be
doing in their head* — and the player's pencil tick is their manual version of it.

- **Inputs:** current **heading** (from the movement/turret-independent hull yaw), current **speed**
  (movement component), and **elapsed time**. Integrated, these give a *displacement estimate*.
- **Error accumulation:** the estimate is deliberately **noisy and biased**, and the noise is what makes
  the map hard. Per-tick error grows with:
  - **Compass deviation** (§2.2) — a heading bias if you navigate buttoned and never re-check head-out.
  - **Speed/terrain uncertainty** — churned snow, slip, pivoting, and detours inject distance error
    (you *think* you went 2 km; the tracks slipped, you went 1.6).
  - **Time without a fix** — error is unbounded; the longer since your last landmark/radio fix, the
    wider your true-vs-believed gap can be.
  - **Weather/night multipliers** (§5) — you can't course-correct against anything you can't see, so
    error grows *faster* in whiteout/blizzard/night.
- **What the player sees:** nothing automatic. The estimator is the *invisible truth of how lost you
  are getting*. The player counter-acts it by **manually moving their pencil tick** as they reason
  ("held west ~3 minutes at crawl ≈ ~1 km, move the tick a centimetre west"). A skilled player who
  tracks heading and time keeps their tick close to true even with no landmarks; a careless one drifts.
- **Optional confidence cue (diegetic, abstract):** the system *may* surface a soft **confidence**
  read — not a position, but a *fuzziness*: as time-since-fix grows, the area around your believed tick
  could be drawn as a faint widening pencil-smudge "I'm somewhere in here" halo on the paper (your own
  uncertainty, sketched). This communicates "you should re-fix" without ever giving true position.
  Keep it subtle and toggleable (OQ-2).

### 4.2 The landmark fix — orient, identify, correct

The corrective half. A **fix** is the act of recognising a real-world landmark, finding it on the
printed map, and using it to reset your believed position. This is the skill loop.

**The fix procedure (what the player physically does):**
1. **Spot a landmark** punching through the fog (a tall silhouette by day, an emissive beacon by night —
   the world arm places these to be fog-readable, `world-map §3`). Or *hear* one (rail yard, river,
   treeline — audio, P3).
2. **Identify it** against the paper map — the landmark is **pre-printed** as a period map symbol
   (church, elevator, water tower, factory, bridge, rail crane — §6.2). Matching the silhouette/sound to
   the symbol is the player's read.
3. **Take a bearing** to it with the raised compass (§2.2), head-out for accuracy.
4. **Construct the fix** with the pencil: drop a **bearing line** (§2.3) from the landmark's symbol
   along the *reciprocal* of the sighted bearing. Where it crosses your route — or where **two** landmark
   bearing lines cross (a proper resection/triangulation) — is your corrected position.
5. **Move your belief tick** to the crossing. **Fix complete:** dead-reckoning error resets toward zero
   from this point; your confidence is restored.

**Tiers of fix (how good a correction you get):**

| Fix quality | Requires | Correction |
|---|---|---|
| **Resection (best)** | Two+ identified landmarks, clean head-out bearings | Near-exact; error reset to ~0 |
| **Single bearing + route** | One landmark + your drawn route line | Good along-track; some cross-track slop |
| **Recognition only** | "That's the water tower" but no compass bearing | Coarse — snaps belief near the landmark, not precise |
| **Radio fix (§7)** | Living radio-op | He hands you a position; quality depends on his state |
| **Dead reckoning only** | No landmark visible (whiteout/night) | **No correction** — error keeps growing |

The whole tension: **fixes require visibility, and the game's signature condition is a whiteout.** When
you can see, you can stay found cheaply; when the blizzard closes in, you're navigating on pencil and
faith until it lifts.

### 4.3 The loop, compressed

```
DRIVE  ──(heading×speed×time)──►  dead-reckoning error GROWS (faster in whiteout/night)
  │                                         │
  │ player manually nudges belief tick      │ error is the hidden true-vs-believed gap (§3)
  ▼                                         ▼
SPOT/HEAR a landmark ──► identify on paper ──► compass bearing (head-out) ──► pencil bearing line
  ──► crossing = corrected position ──► move belief tick ──► error RESET
  │
  └─(or)─► radio-op alive? he hands you a fix (§7).  Radio-op dead? you do all of the above alone.
```

---

## 5. How weather, night, and whiteout degrade navigation

Navigation reads the **atmosphere arm's** published state (`atmosphere §2c`/`§4`, via
`MPC_Atmosphere`) — we consume, we don't own it. The weather *directly* sets how navigable the world
is. This is the lever that turns navigation into tension.

| Atmosphere state (read) | Source | Effect on navigation |
|---|---|---|
| `FogConcealment` / `ViewDistance` | weather preset (`atmosphere §8b`) | Sets **landmark spotting range**. Clear ≈ 9 km view → distant silhouettes readable, fixes easy. Blizzard ≈ 6 km *froxel* but **40–80 m effective** silhouette reveal → almost nothing to fix on; dead-reckoning error multiplier spikes. |
| `IsNight` / `AmbientDarkness` | time-of-day curve (`atmosphere §3`) | Daytime silhouettes vanish; navigation shifts to **emissive beacons only** (burning wrecks, lamps, watch-fires — `atmosphere §5`, `world-map §3.2`). A landmark with no night-light becomes **un-fixable at night**. Reading the paper needs the **cabin light** (a beacon liability, §0/P4). |
| `SnowfallRate` / `WindStrength` (blizzard 0..1) | weather | **Whiteout blackout:** at full blizzard, spotting range collapses, the compass is your *only* instrument, and dead-reckoning error grows fastest. The blizzard is the **navigation boss fight**. |
| `AudioMuffle` | weather → audio (P3) | High muffle (deep snow) **also dampens the audio landmarks** (you can't hear the rail yard through the storm) — weather closes the ears as well as the eyes. |

**The degradation curve (concrete intent):**
- **Clear day:** trivial. Landmarks visible far; fixes cheap; error stays small. (Used sparingly — the
  game is mostly *not* this.)
- **Overcast / ground fog (the baseline dread):** landmarks loom at the 40–80 m reveal; you fix off them
  as you pass; error stays bounded but you must *work* for it. The dread floor of `The Long White`
  (`world-map R2`).
- **Snow / dusk:** silhouettes intermittent; you lean on the compass and time; error creeps; you start
  *guessing*. First real navigational stress.
- **Blizzard / night (the deadly state — `atmosphere §0`):** near-total nav blackout. Compass only
  (and it's biased buttoned). Dead-reckoning error compounds toward "I have no idea where I am." This is
  where wrong-heading deaths happen (§8). The **director-pinned blizzard at the western edge**
  (`atmosphere §6`) means the *climax* of the run is a whiteout navigation gauntlet to friendly lines —
  by design.

**Why this is fair, not cheap:** the world arm places landmarks to be *legible at the reveal distance*
(`world-map §3`, "fog-nav must be tuned so the map is legible enough to not cause cheap permadeaths").
The blizzard makes you *lose the thread temporarily*, not *instantly die* — you slow down, you crawl,
you wait for it to thin, you use the compass to hold a heading, and you accept growing error as the
*price of pushing on in a storm*. Patience (park, wait out the worst, navigate when it lifts) is a
valid, taught counter-play — which couples navigation to the time/fuel/exposure economy.

---

## 6. What the in-world paper map actually depicts

The map sheet is a **handcrafted, hand-authored period artifact** that the world arm and navigation arm
co-specify. It is **not** an auto-generated render of the level — it is a *drawn map* of the world, with
all the selective omission and error of a real one.

### 6.1 Style & detail level

- **Look:** period `1:100,000` Heereskarte — contour/hachure terrain, blue watercourses, black rail
  tick-lines, road classes, settlement blocks, a printed kilometre grid (the **grid is the player's
  coordinate language** — "I'm in square Kr-14"), marginal scale bar + **magnetic-north arrow with the
  declination note** (so the compass bearings make sense). Coffee-stained, creased along the folds,
  hand-annotated in places by a previous (German) hand. Crunched by `MasterRetro` like all art.
- **Detail level — deliberately coarse and incomplete:** it shows **terrain and landmarks, not
  threats and not you.** Towns are blocks, not street plans; the forest is a green wash, not individual
  trees; the river is a line, not the exact ford. The map tells you *the shape of the land and the
  things that stick up*; it does **not** tell you where the enemy is, where the loot is, or where you
  are. Reading the *real* terrain against this coarse sheet is the skill.
- **Honest period errors (optional flavour, P6):** the map can be *slightly wrong* in places — a track
  that no longer exists, a village now rubble, a bridge the map shows intact that you'll find blown
  (`world-map R4`). The map is a pre-war survey; the war changed the ground. This is texture, used
  sparingly so it adds dread, not unfairness. (OQ-3.)

### 6.2 Pre-marked vs blank (what's printed, what you add)

| On the sheet from the start (printed) | The player adds (pencil) |
|---|---|
| Terrain: contours, rivers, forest, marsh, the steppe | **Belief position tick** (§2.3) |
| The **landmark symbols** (church, grain elevator, water tower, factory chimney + smokestacks, rail water-crane, the bridges, the perimeter) — the world arm's `world-map §3.2` set, drawn as period map symbols | **Route lines** (planned legs) |
| Roads + the **Rollbahn** spine and the rail line | **Waypoints / X marks** (self-set objectives, loot, threats to avoid) |
| The kilometre grid + marginal scale + magnetic north | **Bearing construction lines** (for fixes) |
| The **start point** (your centre leaguer — the one thing you *know*) circled by a previous hand | **Confidence smudges** (§4.1, optional) |
| Faint German annotations from the advance (the unit that drew its route *east* — now your route *west* in reverse, `world-map §2.1`) | **Erasures / smears** (your corrections) |

**Two hard rules on pre-marking:**
1. **The destination is NOT a marked objective.** "West to friendly lines" is the *only* thing you know
   (`GAME_DESIGN`); the map shows the perimeter as *terrain* (the front-line trace, the breach as a
   gap), not as a glowing goal. You navigate toward the **western edge of the sheet**, reading the
   landmarks in reverse-advance order (`world-map §2.1`) — the map's own geography is the guidance, with
   no hand-holding marker.
2. **Your position is NEVER pre-marked or auto-marked** (§3). Only your own pencil puts you on the map.

### 6.3 The map *is* the environmental narrative (ties to the spine)

Because the centre→west route **retraces the ruins you made coming in** (`GAME_DESIGN`, `world-map
§2.1`), the faint German annotations on the sheet are *your own battlegroup's advance markings, read in
reverse*. The map you navigate your escape with is the map of your offensive — the pencil route east
that someone in your unit drew is the route west you now retrace. Finding your own old marks on the
paper is a wordless story beat (no dialogue, P-story). The map is not just a tool; it's a document of
the doomed advance.

---

## 7. Radio-operator navigation assist — and its loss

This is the **crew integration** the brief demands, and it is already reserved on the other side: the
crew arm's radio-op loss-cost is *"lose the hull MG and lose the map/intel … the nav/threat map goes
dark or degrades to dead-reckoning"* (`crew-system §1`/§4.4). **Navigation is the "map/intel arm" that
the crew doc hands off to.** We define exactly what the radio-op *does* for navigation and what
breaks when he dies.

### 7.1 While the radio-op lives (`FCrewCapabilities.bMapIntel == true`)

The radio-op is your **navigator and intel relay** — he does the arithmetic you'd otherwise do alone:

- **Position fixes on request:** in a lull you can ask the radio-op for a fix (a "where are we?" bind).
  He works the radio (net traffic, known station bearings, his own reckoning) and **hands you a position
  estimate** — the system *places a fix-quality belief tick for you*, or tightens your existing one. This
  is the single biggest assist: he keeps your believed position honest so you don't have to perfectly
  track heading and time yourself.
- **Landmark relays / confirmation:** when you spot an ambiguous shape, he can **confirm the
  identification** ("that's the elevator, not the church") — turning a coarse recognition-only fix
  (§4.2) into a confident one. Couples to audio (P3): his relay is a *heard* aid, not a text box.
- **Periodic drift correction:** while he lives, your dead-reckoning error is **passively capped lower**
  (he's quietly keeping track) — error still grows in a blizzard but never runs away as badly as it does
  without him.
- **Threat bearings (handoff, not owned here):** he also relays *contact* bearings (the crew doc's
  "threat overlay") — that's a stealth/AI-facing channel; navigation only owns the *position/landmark*
  half. We note the seam so the same radio-op death kills both.

### 7.2 When the radio-op dies (`bMapIntel == false`)

The capability flag flips (crew arm raises `ApplyCrewHit` → recomputes `FCrewCapabilities`,
`crew-system §9.2`); navigation reads the flag and **degrades materially**:

- **No fixes on request.** The "where are we?" bind goes dead — *you* must dead-reckon entirely alone,
  tracking heading and time in your head and your pencil, and constructing every landmark fix yourself
  (§4.2). The skill burden lands fully on the player.
- **No identification help.** Ambiguous silhouettes stay ambiguous; you can misidentify a landmark and
  build a *wrong* fix (a confident pencil tick in the wrong place — the most dangerous kind of lost).
- **Error cap removed.** Dead-reckoning error can now run away in sustained whiteout — the radio-op was
  the thing quietly holding it down.
- **Diegetic death of the aid (the crew doc's "map visibly dies"):** the optional corner micro-compass
  (§2.4), if shipped, **goes dark/static** on his death (it was radio-fed); the radio set at his empty
  station goes silent (P3 — the emptying soundscape, `crew-system §7.2`). You feel the assist *die*, you
  don't just notice a number changed.

**The arc this creates:** early run, with a full crew, navigation is *assisted* — comfortable, you trust
your tick. As attrition bites and the radio-op is killed (often in the very fights that gate the route),
navigation **gets materially harder for the rest of the run** — and it gets harder *exactly as* you push
into the back-third blizzard climax (`world-map §2.2`, `atmosphere §6`). Losing the radio-op turns the
final whiteout run to friendly lines into a genuinely lonely, fully-manual navigation ordeal. The
difficulty curve is, again, the casualty curve (`crew-system §4.5`).

---

## 8. Failure / lost states and recovery

Getting lost must be a **real, modeled danger with consequences** — but recoverable by skill, not a
random insta-death (P5/`world-map §7` "tuned to not cause cheap permadeaths").

### 8.1 The states of being lost

| State | What it is | How it feels |
|---|---|---|
| **Oriented** | belief tick near true; recent fix | confident; drive on |
| **Drifting** | error growing, no recent fix; you *think* you know | quiet unease; "should I check?" |
| **Lost** | large true-vs-believed gap; no landmark in sight | dread; you crawl, hunt for anything recognisable |
| **Misled (worst)** | confident pencil tick in the *wrong* place (a bad fix / misidentified landmark / no radio-op to correct you) | false confidence — you drive hard in the wrong direction |

"Misled" is deliberately the most dangerous: a *confident wrong belief* (often from a dead radio-op,
§7.2) sends you driving *fast* the wrong way, into the consequences below.

### 8.2 The consequences of a wrong heading (why it matters in permadeath)

A wrong heading is not a slap on the wrist; it spends the run's scarce resources and can end it
(the brief: *"wrong heading = into enemy, off a balka, out of fuel"*):

- **Into the enemy:** drift off the planned concealed branch onto the exposed Rollbahn, or wander into a
  chokepoint *unready* (`world-map §2.3`: combat is gated at chokepoints; arriving lost = arriving
  ambushed). A lost navigator stumbles into the lethal fights from the wrong angle.
- **Off a balka / into the river:** the steppe is cut by **balkas** (ravines) and the frozen river
  (`world-map R4`); a wrong heading in a whiteout can drive a 57-tonne tank over an edge or onto thin ice
  — a mobility-kill or a drowning, run-ending. (Couples to movement/terrain.)
- **Out of fuel / time:** every wrong kilometre is fuel and daylight burned (couples to the attrition
  economy + the day/night clock, `atmosphere §3`). Get lost enough and you run the tank dry far from the
  lines, or the climactic blizzard catches you mid-steppe — a slow permadeath by exposure.

### 8.3 Recovery (always a skill path back)

Being lost is **never a dead end you can't climb out of** — there is always a route back to oriented:

1. **Stop and wait** — park, button up, wait for the blizzard to thin (the taught counter-play, §5).
   When `FogConcealment` drops, landmarks reappear and you can fix. Costs time/fuel, not the run.
2. **Find any known landmark** — drive a cautious search pattern until *something* recognisable looms or
   is heard; one good landmark + a compass bearing re-anchors you (§4.2).
3. **Backtrack your pencil trail** — your superseded belief ticks (§2.3) and route lines are a
   breadcrumb; retrace toward the last point you were confident, then re-fix.
4. **Ask the radio-op** — if he's alive, a requested fix (§7.1) is the fast recovery. If he's dead, this
   path is gone (which is exactly why his loss raises the stakes).
5. **Hold a true heading head-out** — pop the hatch for a clean compass bearing (§2.2), accept the
   exposure, and just *hold west* by the accurate needle until you cross the Rollbahn or the rail line
   (linear features you can't miss — a "catching feature" you steer to intersect, classic land-nav).

**Catching features as a safety net (world-arm coordination):** the world is laid out so a lost player
who simply **holds west** will eventually intersect a **linear feature that runs across their path** —
the rail line, the river, the Rollbahn, the treeline edge (`world-map §2.3`). Hitting a long line you
*can't* miss and then following it re-orients you. This is the geometric guarantee that holding a rough
heading always eventually gets you found — so the blizzard is *frightening*, not *unfair*.

---

## 9. C++ / UMG architecture

Fits the existing scaffold: new owned components on `AKesselTankPawn` (and reused on the EVA
`AKesselFootCharacter`), data assets for the map + landmarks, a UMG map widget, a landmark registry the
world arm populates, and pencil-mark persistence into the run save. Mirrors the component pattern of
`Gun/Armor/View/Aiming` and the *raised-item state machine* of `UKesselSmokingComponent`. No new module
(lives in `Kessel`; re-enables `UMG`/`Niagara` deps when it lands, as the cigarette arm notes).

### 9.1 New types

| Type | Base | Role |
|---|---|---|
| `UKesselNavigationComponent` | `UActorComponent` | The brain. Owns the **belief-position** state + the hidden **dead-reckoning estimator**; the **raised-item state machine** (None/Map/Compass/Pencil) that gates input like smoking; runs the error model (reads `MPC_Atmosphere`); resolves landmark **fixes**; queries the crew's `bMapIntel`; owns the pencil-mark list and its save/load. One on the player pawn. |
| `UKesselMapWidget` (`WBP_NavMap`) | `UUserWidget` | The raised paper sheet UMG: renders the map texture, pans/zooms the paper, draws the pencil-mark overlay (lines/ticks/X), hosts the mark/erase cursor. **Never draws true position.** Bound to the nav component's broadcasts. |
| `UNavMapData` | `UPrimaryDataAsset` | The **map sheet**: the printed cartography texture(s) at zoom tiers, the world-uu↔paper-uv transform (grid origin + scale + magnetic declination), and the list of **printed landmark symbols** (id + paper position + symbol). One per region/sheet. |
| `UNavLandmarkRegistry` | `UPrimaryDataAsset` *or* `UWorldSubsystem`-backed list | The **landmark registry the world arm populates**: each entry = a world-placed landmark's id, world transform, fog-readable silhouette/beacon class, audio signature, and its matching map symbol id. Navigation *consumes* it for the fix loop; the world arm *authors* it (§9.4). |
| `FNavLandmark` | `USTRUCT` | One registry entry: `Id`, `WorldLocation`, `ENavLandmarkKind Kind`, `MapSymbolId`, `bHasNightBeacon`, `AudioTag`, `SpotRangeClear`. |
| `FNavPencilMark` | `USTRUCT` | One saved mark: `ENavMarkType Type`, paper-space points (`TArray<FVector2D>` for lines, one point for ticks), timestamp, optional `LandmarkId` (for confirmed-fix marks). Serialized into the run save. |
| `FNavBelief` | `USTRUCT` | The player's believed state: `FVector2D BelievedPaperPos`, `float Confidence` (0..1, inverse of time-since-fix scaled), `float BelievedHeading`. **Distinct from the pawn's true transform.** |
| `ENavItem` | `UENUM` | `None, Map, Compass, Pencil` (the raised-item state). |
| `ENavMarkType` | `UENUM` | `BeliefTick, Route, Waypoint, BearingLine, Smudge`. |
| `ENavLandmarkKind` | `UENUM` | `Church, GrainElevator, WaterTower, FactoryChimney, RailCrane, Bridge, WreckBeacon, Perimeter, RiverBend, TreelineEdge, …`. |

```cpp
USTRUCT(BlueprintType)
struct FNavBelief
{
    GENERATED_BODY()
    // The player's BELIEVED position on the paper — NOT the true pawn position.
    UPROPERTY() FVector2D BelievedPaperPos = FVector2D::ZeroVector;
    UPROPERTY() float     BelievedHeading  = 270.f;   // degrees, player's pencil heading
    UPROPERTY() float     Confidence       = 1.f;     // 0..1, decays with time-since-fix + weather
    UPROPERTY() float     TimeSinceFix     = 0.f;     // seconds; drives error growth (§4.1)
};
```

### 9.2 The dead-reckoning estimator + error model

- `UKesselNavigationComponent::TickComponent` integrates the **hidden true displacement** from the
  movement component (it already has the real transform) and grows a **DR error** scalar:
  `error += dt * (BaseRate + CompassDeviationBias + WeatherMul·(1 - FogConcealmentInverse) +
  NightMul·IsNight) · (bMapIntel ? RadioCap : 1.0)`. Reads `FogConcealment`, `IsNight`,
  `WindStrength` from `MPC_Atmosphere` (atmosphere arm's published scalars, `atmosphere §2a`).
- **Compass deviation** is a modeled scalar: `Deviation = BaseSteelBias · (bExposed ? 0 : 1) +
  TurretOffsetTerm` — buttoned reads carry the bias, head-out (`bExposed` from `UKesselViewComponent`)
  reads clean (§2.2).
- The estimator **never writes the belief tick** — only the player's pencil (or a radio fix) moves the
  belief. The estimator's job is to (a) drive the optional confidence smudge (§4.1) and (b) decide how
  *wrong* a coarse/recognition fix is allowed to be. True position is *only* ever read internally for
  fix resolution and consequence checks; it is **never** surfaced to `UKesselMapWidget`.

### 9.3 The raised-item state machine + input gating (the cigarette pattern)

- A small FSM (`ENavItem`) exactly like `UKesselSmokingComponent` (`cigarette-system §3.1`/§9.4):
  raising the Map/Compass/Pencil **suppresses the drive/fire/aim mapping contexts** and adds a gated
  **`IMC_Navigation`** context — and the `AKesselPlayerController` (the *sole* `AddMappingContext`
  owner, `SCAFFOLD §1`, `cigarette §3.3`) performs the swap on the component's request. While any nav
  item is raised you are **buttoned, stationary, defenceless** — the same exposure cost as smoking.
- `IMC_Navigation` binds: pan (mouse), zoom (wheel), cycle mark type, place mark, erase, take compass
  bearing, request radio fix, lower item. A hit / loud threat **panic-lowers** the item (drop the map,
  hands back to the gun) — reusing the smoking panic-stub idea.
- View: raising the map drives `UKesselViewComponent` to the **instrument-focus framing**
  (`ARCHITECTURE §5` / `ART_BIBLE §7d`); the compass is a lighter raise that can be done in any
  buttoned/head-out view (head-out for the clean bearing).

### 9.4 The landmark registry seam (world-arm coordination)

This is the **hard cross-arm dependency**, and it is symmetrical with how the atmosphere arm hands
light-archetype placement to the world arm:

- **The world arm authors the landmarks** (`world-map §3.2` is already the canonical set) — it places
  the physical silhouettes/beacons and, **at placement, registers each one** into the
  `UNavLandmarkRegistry` (id, world transform, kind, the map-symbol id it corresponds to, night-beacon
  flag, audio tag). The world arm's blockout method (`world-map §4`) gains one step: "stamp the nav
  registry entry when you place a landmark stake."
- **The navigation arm consumes the registry** read-only for the fix loop (§4.2): which landmarks are
  *spottable now* (range vs `ViewDistance`/`IsNight`/beacon), and the world↔paper transform to draw the
  player's bearing line against the printed symbol.
- **The map sheet (`UNavMapData`) and the registry must agree:** every registry landmark has a printed
  `MapSymbolId` on the sheet, and every printed symbol the player can fix on has a registry entry. The
  two arms co-own a **single id list** (the `world-map §3.2` landmark table *is* that list) so the paper
  map and the world never disagree about what's out there. **This is the coordination contract:** the
  paper map depicts exactly the world arm's authored landmarks, placed to be fog-readable.

### 9.5 Pencil-mark persistence

- `FNavPencilMark[]` lives on `UKesselNavigationComponent`, serialized into the **run save**
  (`UKesselGameInstance` holds the run seed, `ARCHITECTURE §2`; the save payload carries the marks +
  `FNavBelief`). Marks survive park/EVA/reload-within-the-run; they are wiped on a new permadeath run.
- The widget renders marks from this list each time the map is raised; placing/erasing mutates the list
  and broadcasts `OnMarksChanged`. Because it's plain serialized data, the **dead player's annotated
  map** can be shown on the death screen (a powerful, free permadeath beat — the run's whole journey in
  pencil, ending at a tick).

### 9.6 Wiring summary

```
DRIVE ──► UKesselNavigationComponent.Tick(): integrate true displacement, grow DR error
            │  reads MPC_Atmosphere {FogConcealment, IsNight, WindStrength}
            │  reads UKesselViewComponent.bExposed (compass deviation gate)
            │  reads UKesselCrewComponent.GetCapabilities().bMapIntel (radio assist)
            ▼
RAISE MAP/COMPASS/PENCIL bind ──► ENavItem FSM ──► PlayerController suppress drive/fire IMC,
            │                                        add IMC_Navigation (cigarette pattern)
            ▼
UKesselMapWidget (WBP_NavMap): pan/zoom paper (UNavMapData), draw FNavPencilMark overlay
            │   place/erase marks ──► FNavPencilMark[] (saved, §9.5) ──► OnMarksChanged
            ▼
COMPASS bearing (head-out=clean) + landmark spotted (UNavLandmarkRegistry, range-gated by weather)
            │   ──► pencil BearingLine ──► crossing = fix ──► move BeliefTick ──► error RESET
            ▼
RADIO FIX bind ──► if bMapIntel: nav comp places/tightens belief tick ; else: dead (do it yourself)
```

---

## 10. M1 prototype slice vs later

### Not in M1 (honest)

M1's done-when is "drivable + shooting tank in volumetric fog" (`MILESTONE_1_PLAN`). Navigation is
**post-M1**, like the cigarette and EVA systems, and it has a **hard prerequisite that is itself
post-M1**: a handcrafted world with placed, fog-readable landmarks (`world-map §1.3`/§3). You cannot
navigate a world that doesn't exist yet. Nothing here blocks the M1 critical path and nothing here
should be built before the world-map vertical slice lands.

### First vertical slice (post-M1, smallest honest navigation)

Buildable as soon as the world arm's **one vertical-slice region** (`world-map §1.3`, the Krasny town +
adjoining steppe) exists with 2–3 registered landmarks:

1. **`UKesselNavigationComponent` + the raised-item FSM** (`ENavItem`) reusing the cigarette input-gate:
   raise the **paper map** → instrument-focus framing, drive/fire suppressed. *Done-when:* hold a key →
   the map fills the lower frame, the tank is stationary, you can lower it.
2. **`UNavMapData` + `UKesselMapWidget`:** one hand-authored map sheet for the slice region; pan + zoom
   the paper; **no position shown** (prove the central rule, §3).
3. **Pencil marks:** place a **belief tick** + a **route line**, erase, and **persist** them
   (`FNavPencilMark[]` into the run save). *Done-when:* marks survive a save/reload and render on raise.
4. **Compass raise + heading read** (`ENavItem::Compass`) with the **buttoned-deviation vs head-out-clean**
   split wired to `UKesselViewComponent.bExposed`.
5. **`UNavLandmarkRegistry` + the manual fix loop (recognition + single-bearing tier, §4.2):** register
   the slice's 2–3 landmarks; spot one, take a bearing, drop a bearing line, move the belief tick. Prove
   spotting is **range-gated by `MPC_Atmosphere.FogConcealment`** (a landmark you can't see, you can't
   fix on). *Done-when:* a clean-weather fix corrects the tick; a blizzard hides the landmark and the
   fix is unavailable.
6. **Radio-op assist stub:** read `UKesselCrewComponent.GetCapabilities().bMapIntel` — if alive, a
   "request fix" bind places a belief tick; if the crew arm has killed the radio-op, the bind is dead.
   (Couples to whatever the crew arm has wired; stub if `bMapIntel` isn't exposed yet.)

This slice proves the entire spine — **raise items, no auto-position, pencil belief, weather-gated
landmark fix, radio-op dependency** — with one map sheet and a handful of landmarks.

### Later (post-slice, honest deferrals)

- **Full dead-reckoning error model + the confidence smudge** (§4.1) tuned against real run telemetry.
- **Resection / two-landmark triangulation** (the best fix tier, §4.2) and **audio landmarks** (fix by
  *hearing* a rail yard/river — waits on the audio arm's spatial landmark cues, P3).
- **Full radio-op behaviour:** identification confirmation, passive error-cap, the diegetic death of the
  aid (corner-compass goes static, radio falls silent — waits on the crew arm's full radio-op wiring +
  the audio emptying-soundscape, `crew-system §7.2`).
- **The optional corner micro-compass** (§2.4) — opt-in, radio-fed, dies with the radio-op.
- **Multi-sheet map** across all 8 regions (`world-map §2.2`) with the **reverse-advance German
  annotations** as story (§6.3) and the **honest map-vs-reality errors** (blown bridge shown intact,
  §6.2 — waits on the `DL_Damage` retrace states, `world-map §4.1`).
- **EVA navigation:** the same items raised on foot (the P5 exception, `eva-looting`), reusing the
  possession seam — navigate a ruin or relocate your parked tank on the map while dismounted.
- **The dead-player annotated-map death screen** (§9.5) — a polish beat.
- **Cabin-light-on-paper beacon coupling** (reading the map at night lights you up — the §0/P4 liability,
  waits on the atmosphere night-detection model maturing, `atmosphere §4`).
- **Per-brand/period map art passes** and **affine paper crunch** under `MasterRetro`.

---

## 11. Concise structured summary

### Key decisions
- **Fully diegetic manual navigation: no minimap, no GPS, no objective marker, no auto-position.** Three
  raised interactive items — **paper map (period Heereskarte), compass (Marschkompass), pencil
  (chinagraph)** — in the abstracted/period style, raised on a gloved-hand layer exactly like the
  cigarette, **gating drive/fire/aim input** (navigation costs tempo + exposure).
- **The map never shows your position (the central rule, §3).** The system holds a hidden **true
  position**; the player holds a **believed position** (their pencil tick). The *gap between them is the
  gameplay.* Dead reckoning grows the gap; landmark fixes close it.
- **Dead-reckoning + landmark-fix loop (§4):** heading×speed×time → accumulating error; correct it by
  spotting a pre-printed landmark, taking a compass bearing, and constructing a pencil **fix**
  (recognition → single-bearing → two-landmark resection tiers).
- **Weather/night/whiteout directly degrade navigation (§5)** by reading the atmosphere arm's
  `MPC_Atmosphere` (`FogConcealment`/`IsNight`/blizzard): landmarks become un-spottable, error runs away,
  the **blizzard is the navigation boss fight** and the director-pinned western-edge blizzard makes the
  climax a whiteout nav gauntlet.
- **The compass carries a modeled steel-hull deviation:** biased buttoned, clean head-out — the accurate
  read is the exposed read (period-true, P6; same risk grammar as the headlamp/cherry).
- **The paper map depicts the world arm's authored landmarks (§6/§9.4)** as coarse period symbols + a km
  grid; **pre-marked = terrain + landmarks + start point + faint reverse-advance German annotations**;
  **player-added = belief tick + route + self-set waypoints + bearing lines.** Destination and position
  are *never* marked.
- **Radio-op = your navigator (§7).** Alive: fixes on request, identification confirmation, a passive
  error cap. Dead (`crew-system §4.4`, `bMapIntel=false`): **no assist, fully manual dead reckoning, error
  uncapped, the aid visibly dies** — navigation gets materially harder for the rest of the run, in lock-
  step with attrition.
- **Lost is modeled and recoverable (§8):** Oriented→Drifting→Lost→Misled; wrong heading spends fuel/time
  and drives you into enemies, off balkas, onto thin ice (real permadeath risk) — but **catching features**
  (hold west → hit the rail/river/Rollbahn) guarantee a skill path back, so the whiteout is frightening,
  not unfair.
- **Architecture:** `UKesselNavigationComponent` (belief + DR estimator + raised-item FSM + fixes),
  `UKesselMapWidget`/`WBP_NavMap` (paper, never draws true position), `UNavMapData` (the sheet),
  `UNavLandmarkRegistry` (world-arm-populated), `FNavPencilMark` persisted into the run save. Reuses the
  cigarette input-gate pattern and the `Gun/Armor/View` component pattern; no new module.

### M1 slice
**Navigation is post-M1** (needs the post-M1 handcrafted world + landmarks; M1 is just "drivable shooting
tank in fog"). First vertical slice once the world arm's Krasny vertical-slice region + 2–3 registered
landmarks exist: nav component + raised-map FSM (cigarette input-gate); one `UNavMapData` sheet +
`UKesselMapWidget` with **no position shown**; pencil belief-tick + route + erase + **persistence**;
compass raise with buttoned-deviation/head-out-clean split; `UNavLandmarkRegistry` + the
recognition/single-bearing fix loop **range-gated by `FogConcealment`**; radio-op `bMapIntel` assist stub.

### Dependencies on other arms
- **World-map arm (hard, blocking):** authors + **registers the landmarks** into `UNavLandmarkRegistry`
  (one new step in its blockout method, §9.4); co-owns the single landmark id list so the paper map and
  the world agree; lays out **catching features** (rail/river/Rollbahn across the path) for the lost-
  recovery guarantee (§8.3). Navigation cannot exist before the world does.
- **Crew arm:** we are the "map/intel arm" its radio-op loss-cost hands off to; we consume
  `FCrewCapabilities.bMapIntel` (§7). Coordinate the death seam (radio falls silent / corner-compass dies).
- **Atmosphere arm:** we read `MPC_Atmosphere` `FogConcealment`/`IsNight`/`WindStrength`/`ViewDistance`
  for spotting range + error multipliers (§5); the night-beacon landmarks are its light archetypes.
- **Audio arm (P3):** audio landmarks (hear a rail yard/river/treeline) + the radio-op's *heard* relay +
  the emptying soundscape on his death.
- **EVA arm:** the same items raised on foot (the P5 exception), reusing the possession seam (later).
- **Movement arm:** true heading/speed for the DR estimator; balka/ice consequences of a wrong heading.

### Open questions
1. **OQ-1 — corner micro-compass:** ship the optional radio-fed degraded corner compass (§2.4) at all, or
   keep navigation *purely* item-raised? (Recommend opt-in, default off — diegetic-first.)
2. **OQ-2 — confidence smudge:** surface the soft "I'm somewhere in here" uncertainty halo (§4.1), or keep
   error entirely invisible and let only the world's consequences teach it? (Recommend a subtle, toggleable
   smudge — communicates "re-fix" without giving position.)
3. **OQ-3 — map-vs-reality errors:** how far to lean on the period "map is slightly wrong" flavour (§6.2,
   blown bridge shown intact) — dread texture vs unfairness? (Recommend sparing, tied to the `DL_Damage`
   retrace beats only.)
4. **OQ-4 — fix-construction depth:** does the player manually draw bearing-line crossings (tactile, harder)
   or does taking a bearing auto-snap a fix (simpler)? (Recommend manual for the resection tier, auto-assist
   while the radio-op lives.)
5. **OQ-5 — buttoned vs head-out map read:** can the map be read at all head-out (wind would take the paper),
   or is map-reading strictly buttoned and only the *compass* gets the head-out clean read? (Recommend map =
   buttoned only; compass = both, with the deviation split.)

### Risks
- **Legibility / frustration (the cardinal risk):** a no-auto-position map is unforgiving; if landmarks
  aren't legible at the fog reveal distance, the player gets *cheaply* lost and the system feels broken
  rather than tense. Mitigation: the world arm's fog-legible landmark placement (`world-map §3`/§7), the
  catching-feature safety net (§8.3), the radio-op early-run assist (§7.1), and hard playtest tuning of the
  error model once compilable. This is the make-or-break and needs the most iteration.
- **Cross-arm coupling:** navigation is the most *dependent* system in the project (world geometry + crew +
  atmosphere + audio + movement). The registry seam (§9.4) and reading published `MPC_Atmosphere`/
  `FCrewCapabilities` isolate us, but the build order must let the world vertical slice land first.
- **Teaching the method without a tutorial/HUD:** dead reckoning + resection is a real skill; with no
  dialogue and no markers, the *early* assisted run (full crew, clear-ish weather, radio-op fixing for you)
  is the teacher — the comfortable Act-I (`crew-system §7.1`) must double as the navigation tutorial before
  the radio-op dies and the blizzard closes in. If that on-ramp is too thin, players bounce.
- **The believed/true split is invisible — hard to debug + tune:** because true position is never shown,
  balancing "fair amount of drift" is subtle. Need a dev-only true-position overlay (editor/PIE only, never
  shipped) to tune the error model.
- **Scope:** the full multi-sheet map, audio landmarks, resection, and the radio-op's full behaviour are a
  real content + systems lift; the first slice is honest, but the *complete* feeling (lonely manual nav in a
  blizzard with a dead radio-op) only lands once the world, crew, and audio arms are all mature.

---

## 12. Cross-references
- Pillars: `DESIGN_PILLARS.md` (1 sim/abstract, 2 crew-of-one/decaying, 3 sound-as-sensor, 4 light, 5 inside-the-steel, 6 grounding).
- Vision: `GAME_DESIGN.md` ("head west", no dialogue, the retrace spine, permadeath).
- Art intent (paper/compass style, instrument-focus framing, corner-compass allowance, fog reveal): `ART_BIBLE.md` §1, §5, §7d.
- World geometry + the landmark set + catching features: `teams/world-map-blockout.md` §2–§3.
- Weather/night state we read: `teams/atmosphere-system.md` §2c, §4, §5, §8b (`MPC_Atmosphere`).
- Radio-op loss-cost (the assist we own the map half of): `teams/crew-system.md` §1, §4.4, §7, §9.2 (`FCrewCapabilities.bMapIntel`).
- Raised-item / input-gate / state-machine precedent: `teams/cigarette-system.md` §3, §9.4 (`UKesselSmokingComponent`).
- EVA possession seam (later, on-foot nav): `teams/eva-looting.md` §2.
- Component/data-asset conventions + IMC ownership + run save: `ARCHITECTURE.md` §2, §5, §6; `SCAFFOLD_REPORT.md`.
</content>
