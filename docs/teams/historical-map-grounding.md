# Kessel — HISTORICAL MAP GROUNDING

Status: **RESEARCH (history + geography + scale reconciliation)**. No actors, no engine work.
This document is the **historical-geography arm**. It supplies the *real-world skeleton* —
true place names, terrain, distances, and sourced maps — that the parallel **world-map-blockout**
arm (`docs/teams/world-map-blockout.md`) turns into engine geometry. The two are designed to lock:
the blockout arm sized the playable corridor at **~13 km west displacement / 16×12 km world**
from path-length budgeting; this arm confirms that footprint is reconcilable with real geography
and tells you *which* real geography to drape over it.

Grounding (do not contradict): `DESIGN_PILLARS.md` (esp. **P6 "historical grounding, not pedantry —
plausible over encyclopedic"**), `GAME_DESIGN.md` (the retrace-your-own-ruins spine; permadeath;
lonely driving punctuated by lethal combat), and `docs/teams/world-map-blockout.md` (the sizing math
and the 8-region spine — **this doc maps real names onto that spine, it does not re-derive it**).

---

## 0. TL;DR (the scale verdict up front)

- **The real corridor is enormous; the game corridor is not — and that is fine.** The historically
  honest escape axis (the German southern-wing withdrawal from the Stalingrad relief high-water mark
  back to **Rostov-on-Don**) is **~230–250 km** straight-line. The blockout arm's **13 km** playable
  west axis can never be 1:1 with that and should not try.
- **Recommendation: do NOT compress the whole 230 km corridor.** Pick **one representative ~55–65 km
  operational segment**, keep its real *relative* layout (river crossings, rail junction, kolkhoz
  towns, the open steppe runs), and compress it **~4–5×** onto the 13 km axis. Compressing the full
  corridor would be ~18× and would dissolve every real landmark into mush.
- **A 1.5–2.5 hr run reconciles cleanly.** Real Tiger winter march pace (~10–20 km/h moving, constant
  stops) over a compressed ~55–65 km segment, driven at the blockout arm's weighted ~18 km/h with
  combat/EVA/nav stops, lands inside the band on a 13 km / 22 km-path map. **No conflict.** (§3)
- **The Tiger detail needs one honest correction** (§1): Tigers (**s.Pz.Abt 503**) were **not** in
  Operation Winter Storm itself. They arrived **~1 Jan 1943**, *after* Little Saturn had already
  forced the retreat, and spent **January–February 1943 as the rearguard of the fighting withdrawal
  west toward Rostov.** That later phase is an **even better** literal fit for "a cut-off Tiger
  fighting west to friendly lines" than Winter Storm is. Use it. (§1, §5)
- **One terrain correction for the blockout arm:** the real theatre is **treeless open steppe cut by
  balkas (steep ravines)**. The blockout's **R5 "Black Belt taiga forest"** is the single least
  grounded region — recommend recasting it as a **deep balka/ravine system or Don-floodplain riparian
  scrub**, not boreal forest. (§4, §6)

---

## 1. HISTORICAL REALITY — what actually happened, and where the Tiger fits

### 1.1 The three-act collapse (Nov 1942 – Feb 1943)

| Act | Operation | Date | What | Direction |
|---|---|---|---|---|
| **The trap** | Soviet **Uranus** | 19–23 Nov 1942 | Pincers close behind 6th Army; **~290–300k** Axis troops encircled at Stalingrad (the *Kessel*). | pincers from N & S |
| **The relief** | German **Winter Storm** (*Wintergewitter*) | **12–23 Dec 1942** | 4th Panzer Army drives **NE from Kotelnikovo** toward the pocket; gets to within **~48 km** (Myshkova R.) — close enough to *see the pocket's flares* — then stalls. | **SW → NE** (toward Stalingrad) |
| **The unhinging + retreat** | Soviet **Little Saturn** | **16–30 Dec 1942** | Smashes the Italian 8th Army on the upper Don/Chir; Badanov's tank corps raids **Tatsinskaya** airfield (240 km deep). Relief force's rear is exposed; Manstein **calls it off 23 Dec**, withdraws to Kotelnikovo by Christmas Eve, then keeps falling back. | **NE → SW → W** (back, then toward Rostov) |

**Source confirmation:**
- Winter Storm: 4th Panzer Army (Hoth), **LVII Panzer Corps = 6th + 23rd Panzer Divisions (~170 tanks),
  later reinforced by 17th Panzer** — launched from the **Kotelnikovo** bridgehead, crossing the
  **two branches of the Aksai**, taking **Verkhne-Kumsky**, reaching the **Myshkova** (Vasilevka
  bridgehead); the pocket was ~55 miles further. ([Wikipedia: Operation Winter Storm](https://en.wikipedia.org/wiki/Operation_Winter_Storm);
  [Warfare History Network](https://warfarehistorynetwork.com/article/stunning-victories-ultimate-defeat/))
- Little Saturn (16 Dec) hit the Italian 8th Army, **threatened the relief force's rear**, took the
  **Tatsinskaya** airfield (cutting the airlift), and **forced Manstein's retreat back to Kotelnikovo
  on 29 Dec** "leaving the encircled Germans to their fate." ([Wikipedia: Operation Little Saturn](https://en.wikipedia.org/wiki/Operation_Little_Saturn);
  [stalingradfront.com](https://stalingradfront.com/articles/articles-about-stalingrad-battle/operation-little-saturn/))

### 1.2 The Tiger correction (important — read before building)

The prompt's premise — *"Tigers (s.Pz.Abt 503) were with the Winter Storm relief force"* — is **off by
one phase**, and the truth is better for the game:

- **Winter Storm had NO Tigers.** Its armour was 6th/23rd/17th Panzer Divisions (Pz III/IV).
- **s.Pz.Abt 503 (the first Tiger battalion in the south) arrived ~1 January 1943** — *after* the relief
  had already failed and the front was collapsing west. It was assigned to **Army Group Don** and
  immediately thrown into **securing the Manych River bridges** and **covering the fighting withdrawal
  toward Rostov.** It first saw action **~4 Jan 1943**, and over 1–17 Jan covered huge distances —
  including a **single-day 65 km move** — as a mobile rearguard. By 20 Jan it had already lost Tigers
  (two destroyed: chassis 250046, 250050). It then fought the **withdrawal to Rostov.**
  ([Wikipedia: 503rd Heavy Panzer Battalion](https://en.wikipedia.org/wiki/503rd_Heavy_Panzer_Battalion);
  [tiger1.info — 503 at Rostov](https://tiger1.info/unit-page/503-Rostov);
  [War History — sPzAbt 503 with Army Group Don](https://warhistory.org/@msw/article/heavy-tank-battalion-503-with-army-group-don-in-southern-russia))

**Why this is a gift, not a problem.** The literal Kessel premise ("a lone Tiger on the wrong side of a
collapsing encirclement, retracing its trail of ruin westward to friendly lines") is **not** Winter
Storm (an *advance*, hopeful, NE). It **is** the **January–February 1943 southern-wing withdrawal**:
a Tiger as the cut-off-feeling rearguard of a shattered front, grinding **west/southwest** through
frozen steppe toward the one open gate — **Rostov-on-Don** — that the entire Caucasus army group had
to funnel through before the Soviets slammed it shut. The "head west, friendly lines are where you came
from" frame is *exactly* the historical 503 experience. **Recommend grounding the game in the
Jan–Feb 1943 withdrawal phase, dressed with Winter Storm's high-water-mark wreckage as the eastern
start.** (This also fixes the retrace-spine logic: you start at the furthest-east advance point and
drive back down your own offensive's axis — see §5.)

---

## 2. SOURCE REAL MAPS — the geography, with links

### 2.1 The real place-name skeleton (east → west, with relative positions)

All distances are straight-line approximations; the theatre is the open Kalmyk/Don steppe between the
lower Don and the Volga.

| Place | What it is | Relative position | Terrain character |
|---|---|---|---|
| **Stalingrad** (now Volgograd) | The pocket / *Kessel*, on the **Volga** | The far NE anchor; on the river | City ruins (but a Tiger never entered — see §7) |
| **Myshkova River** (Vasilevka) | Winter Storm **high-water mark**; ~48 km from the pocket | NE end of the relief axis | Small steppe river in a shallow valley; the furthest-east battle |
| **Verkhne-Kumsky** | Kolkhoz village; **heavy tank battles 13–19 Dec** | Between the two **Aksai** branches | A single steppe village on bare rolling ground; ravine-scarred |
| **Aksai River** (two branches: Esaulovsky / Kurmoyarsky) | River line crossing the axis **~28 miles N of Kotelnikovo** | Mid-axis | Two steppe-river crossings; the LVII Corps held an Aksai bridgehead |
| **Kotelnikovo** (now Kotelnikovsky) | **Rail junction & railhead** of the whole relief offensive; ~120 km SW of Stalingrad | The pivot — start of the advance, fallback of the retreat | Steppe town on the **Kotelnikovo–Stalingrad railway**; depots, sidings, a real **rail node** |
| **Chir River** | German flank line west of Don | West/NW of Kotelnikovo | Steppe river; Italian/Romanian sector that Little Saturn caved in |
| **Tatsinskaya** | Supply **airfield**; Badanov's 240 km raid (24 Dec) | Far NW | Steppe airbase — the airlift's lifeline, then a wreck-field |
| **Manych River** (Manych canal / lakes) | The **503's January bridge-securing line** | SW, between Don & Caucasus | Broad shallow river/lake-chain; key **crossings** the rearguard held open |
| **Proletarskaya / Salsk** | Withdrawal waypoints on the Manych–Rostov rail | SW of the Manych | Steppe rail towns on the retreat road |
| **Rostov-on-Don** + **Bataysk** | **The western gate / friendly lines** — the bottleneck the whole southern wing escaped through | Far west, on the **lower Don** | The **river-crossing chokepoint**; Bataysk is its south-bank twin |

**Terrain in one paragraph (well-sourced):** "gently rolling steppe with virtually no vegetation
except the ubiquitous steppe grass; what little bush or small trees existed were only in the many
**steep-cut ravines (balkas)** scattered throughout." Supply ran "over 100 km of snow-covered steppes,"
and **"each little village across the steppe… housed some sort of repair shop, stable, supply troop,
replacement center, or transport depot."** ([History Rise — Operation Winter Storm](https://historyrise.com/operation-winter-storm-the-attempt-to-relieve-the-encircled-6th-army-at-stalingrad/);
[littlewarstv.com campaign PDF — "Nach Stalingrad!"](https://www.littlewarstv.com/uploads/2/3/7/7/2377799/nach_stalingrad_campaign_1.26_update.pdf))
That paragraph is the **art-direction brief**: open frozen steppe, balka ravines as the only relief and
only cover, treeless, and **kolkhoz villages that are literally military-logistics depots** — i.e.
your loot/EVA sites are *historically* depots, repair shops, and supply troops. Perfect.

### 2.2 Operational maps (sourced, free / referenceable)

For the **Kessel boundary + relief axis + Little Saturn pincer**:
- **Wikipedia — Operation Winter Storm** (clean operational map of the Kotelnikovo→Myshkova axis):
  https://en.wikipedia.org/wiki/Operation_Winter_Storm
- **Wikipedia — Operation Little Saturn** (the outflanking pincer & retreat):
  https://en.wikipedia.org/wiki/Operation_Little_Saturn
- **Wikimedia Commons — Maps of the Battle of Stalingrad** (public-domain / CC; pull `Operation Winter
  Storm.png`, the `Stalingrad Encirclement.jpg` series, and the **Little Saturn** subcategory):
  https://commons.wikimedia.org/wiki/Category:Maps_of_the_Battle_of_Stalingrad
- **historyofwar.org — "The German Relief Effort, December 1942"** (Dec relief axis map):
  https://www.historyofwar.org/Maps/maps_stalingrad9.html
- **onwar.com — Winter Storm 12–18 Dec 1942** (dated operational map):
  https://onwar.com/wwii/maps/efront/13efront.html
- **Omniatlas — "Failure of Winter Storm, 29 Dec 1942"** (shows the retreat unhinging):
  https://omniatlas.com/maps/europe/19421229/
- **The Map Archive — "Operations Winter Storm & Little Saturn 12–30 Dec 1942"** (single combined map):
  https://themaparchive.com/product/operations-winter-storm-little-saturn-1230-december-192/

For the **Tiger / 503 withdrawal toward Rostov & the Manych line** (Jan–Feb 1943):
- **tiger1.info — s.Pz.Abt 503, Battle of Rostov** (unit-level operational detail & maps):
  https://tiger1.info/unit-page/503-Rostov
- **HyperWar — "Stalingrad to Berlin" (US Army "Green Book," public domain)** — chapter with the
  southern-wing withdrawal & Rostov-funnel operational maps:
  https://www.ibiblio.org/hyperwar/USA/USA-EF-Defeat/USA-EF-Defeat-3.html
- **West Point — Digital History Center / atlases** (WWII European theatre plates, free):
  https://dhc.westpoint.edu/atlases/

For **modern terrain reference** (the real steppe surface, balkas, rivers, rail you'll model):
- Use **OpenTopoMap / Google Terrain / Yandex Maps** over **Kotelnikovsky → Zimovniki → Proletarsk →
  Bataysk/Rostov-on-Don** (Volgograd & Rostov Oblasts). This shows the **real balka networks**, the
  Aksai/Sal/Manych river courses, the rail alignments, and the kolkhoz-town spacing — the geomorphology
  is essentially unchanged since 1942 and is the best free DEM-class reference for ravine shapes.

---

## 3. SCALE RECONCILIATION (the user's key question, answered)

### 3.1 Real distances vs. the game axis

| Real segment | Straight-line | Note |
|---|---|---|
| Kotelnikovo → Stalingrad pocket (full Winter Storm axis) | **~120 km** | the relief that never connected |
| Myshkova high-water mark → Kotelnikovo (the meaningful relief corridor) | **~60–65 km** | the part actually fought over |
| Kotelnikovo → Rostov-on-Don (the full withdrawal) | **~230–250 km** | the 503's Jan–Feb retreat axis |
| Manych line → Rostov approach (the rearguard's last stretch) | **~55–60 km** | the climactic funnel |
| **Game playable west axis (from blockout arm)** | **13 km** | start(center) → win(west edge); 22 km of actual *path* at winding factor 1.7 |

### 3.2 Tank pace — does the clock work?

Real heavy-tank winter march pace: **~10–20 km/h moving**, but *operationally* far slower with refuel,
recovery, navigation, and contact — the 503's "good day" was the famed **65 km in a single day** of
near-continuous movement. The blockout arm independently derived a **weighted ~18 km/h moving average**
for a Tiger over mixed winter steppe/town/ice, with the run split ~60% driving / ~18% combat /
~17% EVA / ~5% nav. Both numbers agree.

**The reconciliation:** a 1.5–2.5 hr run is *path-length* sized, not distance-to-Rostov sized. At
~18 km/h moving for ~72 min you cover ~22 km of winding path = **13 km west displacement**. That 13 km
is a **compressed stand-in for a chosen ~55–65 km real segment**, not for the whole 230 km corridor.
The empty steppe between landmarks — which in reality is *hours* of nothing — is exactly what gets cut.
**Verdict: the 1.5–2.5 hr run and the real geography reconcile, provided you compress one segment, not
the whole corridor.**

### 3.3 Recommended compression ratio

- **Compress ~4–5×** against a **single ~55–65 km representative segment** (either the Myshkova→Kotelnikovo
  relief corridor, or the Manych→Rostov withdrawal funnel). 60 km / 13 km ≈ **4.6×**. This keeps every
  real landmark's **relative spacing and order** while shrinking the dead steppe runs hardest.
- **Do NOT compress the full 230 km Kotelnikovo→Rostov corridor** — that is ~18×, which would collapse
  the Aksai, Myshkova, Manych, Kotelnikovo and Rostov onto each other and destroy the relative geography
  the user wants to preserve. If you want the *whole arc* represented, do it as **non-linear time
  compression** (a couple of "you have driven for hours" steppe stretches that stand in for the long
  empty legs) rather than uniform spatial scaling.
- **Anisotropic compression is fine and recommended:** shrink the *empty* steppe legs ~6–8×, but keep
  *landmark clusters* (a river crossing, a rail yard, a town) near **1–2×** so they read at believable
  human scale. This is exactly the blockout arm's region-length table — just apply a heavier squeeze to
  R2 "The Long White" (pure empty steppe) and a lighter one to the town/yard/crossing regions.

### 3.4 UE5 feasibility (affirming the blockout arm)

The blockout arm already established **16×12 km is trivial for World Partition + Large World
Coordinates** (LWC is double-precision world space; WP is validated to tens of km; the 40–80 m whiteout
means only ~2–3 km² is ever resident). Nothing in the historical layer changes that. For reference, a
*literal* 1:1 of the 230 km corridor would be a ~370 km world — still inside LWC's numeric range, but
absurd as **handcrafted** content and pointless given the whiteout. **The compression is a content/
fidelity decision, never an engine limit.** Recommended footprint stays the blockout arm's
**16×12 km authored / ~13 km playable west axis** — this doc just fills it with real names.

**Concrete recommended map size: keep 16 × 12 km (authored), 13 km playable west run** — large enough
to hold the real relative layout of one ~60 km segment at ~4.6× compression, small enough to handcraft,
and comfortably within WP/LWC.

---

## 4. RECOMMENDED MAP BLUEPRINT — real geography over the blockout spine

This maps **real place names onto the blockout arm's existing 8-region spine** (do not renumber it).
Orientation matches the blockout convention: **start at center, win at the west edge (−X = west).**
Framing assumed: **the Jan 1943 withdrawal**, with the eastern start dressed as the Winter Storm
high-water wreckage you advanced to and now retrace (§5 explains why this ordering is historically and
narratively correct).

| # | Blockout region | Real-geography dressing | Terrain | Landmark (fog silhouette) |
|---|---|---|---|---|
| **0** | The Leaguer (center start) | **The Myshkova / Vasilevka high-water mark** — your battlegroup's furthest-east leaguer; fresh-burnt German wrecks of the advance that died here. *You could see Stalingrad's flares from here.* | frozen steppe, shallow river valley | burning supply truck; far-off glow on the NE horizon = the dying pocket you'll never reach |
| **1** | Krasny (ruined town) | **Verkhne-Kumsky** — the kolkhoz village fought over 13–19 Dec; first urban dread, first possible fight. | bare steppe village, balka-scarred | grain elevator / church-stump silhouette |
| **2** | The Long White | **The steppe between the two Aksai branches** — pure isolation; the dread floor; the "100 km of snow" leg, heavily time-compressed. | open frozen steppe | lone water tower; a single distant lamp = depot near |
| **3** | The Tractor Works (industrial/rail) | **Kotelnikovo** — the **real rail junction & railhead** of the whole offensive. Recast the "tractor works" as the **Kotelnikovo rail yard / depot complex**: sidings, a derailed supply train, the water-crane, repair sheds. Densest ruin; major fight. | rail yard, steppe-town industry | factory/elevator chimney + smokestacks; rail water-crane + frozen wagons |
| **4** | The Crossing | **The Aksai (or, in the withdrawal framing, the Manych) river crossing** — blown bridges your own engineers dropped coming through; ice risk; ambush. | frozen river, ice, demolition | blown bridge spans |
| **5** | The Black Belt | **HISTORICAL CORRECTION — recast as a deep balka / ravine system** (or Don-floodplain riparian scrub), **not taiga.** The steppe is treeless; its only close-terrain is the steep-cut ravines where the brush grows. Same claustrophobic close-range dread, but grounded. | steep balka network; scrub in the gully floor | crashed aircraft on the ravine lip; defile of fallen scrub/wrecks |
| **6** | The Approach | **The open approach to the Don / Rostov** — opening out; the lines in sight; climactic gauntlet across the last exposed steppe. | open steppe rising to the river | last ridge; the Don's far bank silhouette |
| **7** | The Perimeter (friendly lines = WIN) | **Rostov-on-Don / Bataysk bridgehead** — the one gate the whole southern wing escaped through; cross the old front line / the Don crossing = **WIN**. | river-crossing perimeter | perimeter watch-fires / a friendly flare — the only welcoming light in the game |

**Route / network grounded in real features (complements blockout §2.3):**
- **The "Rollbahn" spine = the real Kotelnikovo–Stalingrad road/rail axis** you advanced east along and
  now retrace west. The **single-track railway converging on the rail yard (R3) = the literal
  Kotelnikovo line.** Both are real, not invented.
- **Branch routes = balka detours.** The "slower-but-concealed alternative" forks are historically the
  **ravines** — the only concealment on open steppe — trading the exposed graded road for a slow,
  hidden gully. This is both the real terrain logic and the blockout arm's speed-vs-exposure decision.
- **Chokepoints are real:** the **Aksai/Manych crossings**, the **Kotelnikovo rail underpass/yard**, the
  **balka defiles**, and the **Rostov/Don bridgehead.** Combat gates land exactly where the real terrain
  funnels armour — which is also where the blockout arm authors its fights.
- **EVA loot sites are real depots.** Per §2.1, every steppe kolkhoz was a repair shop / supply troop /
  replacement center. Your on-foot scavenging of village ruins is **historically accurate looting of
  logistics nodes** — lean into it (frozen repair sheds, abandoned ammo dumps, a stable, a transport
  depot).

---

## 5. The retrace-spine, made historically correct

The blockout arm's signature spine — *"driving west you cross, in reverse chronological order, the
wreckage of your own offensive"* — is **historically exact** under the Jan-1943 framing:

- You **start (center) at the furthest-east point your side reached** — the Myshkova/Vasilevka
  high-water mark of Winter Storm. The freshest wrecks (last week's) are here.
- Driving **west = retreating down your own advance axis** (the Kotelnikovo Rollbahn). The destruction
  gets **older and more weathered** the further west you go — the dug-in **Soviet AT guns face east**
  (toward where you came from), now snow-filled; you pass the start line of the whole doomed push.
- The **perimeter breach you cross to win** = the gate at **Rostov/Bataysk** the army group funneled
  through — historically the literal "friendly lines to the west."

The **knocked-out sister Tiger** mid-route (the permadeath mirror) is also historically supportable: by
20 Jan 1943 the 503 had Tigers destroyed and shipped back — a blown-turret Tiger of *your own battalion*
on the retreat road is real, not just thematic.

---

## 6. ALTERNATIVE FRAMING — the choice for the user

Two ways to ground the same 16×12 km map. Both honor the pillars; they differ on what "Stalingrad" the
map *evokes*.

### Option A — **Steppe relief/withdrawal corridor** (RECOMMENDED)
The Kotelnikovo–Aksai–Myshkova–Manych–Rostov axis above. **Historically clean for a Tiger** (it is
literally where s.Pz.Abt 503 fought), and a perfect fit for the lonely-driving / open-exposure /
retrace-your-ruins spine.
- **Pros:** historically *correct* (P6 satisfied at the encyclopedic level, not just plausible); the
  open frozen steppe + whiteout + balka close-terrain is the ideal canvas for the dread-drive loop and
  the "heard before seen" combat contract; loot sites are real depots; the west-to-friendly-lines frame
  is the actual 503 experience.
- **Cons:** the iconography is **less instantly recognizable** than "Stalingrad." It's steppe, villages,
  a rail town, and a river — evocative but not the postcard. Mitigate with **one or two iconic distant
  silhouettes** (the dying pocket's glow on the NE horizon from R0; a grain elevator) so the player
  *feels* the famous battle at the edge of the frame without the map being the city.

### Option B — **Stalingrad CITY ruins** (artistic license)
Use the Stalingrad urban iconography — Red October / Barrikady / the Tractor Factory, the Volga bluffs,
the Pavlov's-House rubblescape.
- **Pros:** **instant recognizability**; the "Tractor Works" region becomes the *real* Stalingrad
  Tractor Factory; rich vertical ruin for KIT showcase and close combat; strongest marketing image.
- **Cons:** **historically wrong for a Tiger.** No Tigers fought *inside* the Stalingrad pocket; the
  pocket's armour was immobilized/destroyed and a heavy tank threading factory rubble is a fantasy.
  It also **breaks the escape geometry** — the city is *on the Volga at the NE edge*; "drive west to
  friendly lines" from inside the city is the whole 290k-man tragedy that historically had **no exit.**
  Choosing B means accepting that the map's premise is iconographic, not factual — a direct tension with
  **P6** as the user has written the brief ("grounded in REAL geography rather than invented").

**Recommended hybrid:** **Option A as the map**, with **Option B's iconography quoted at a distance** —
the burning city glow on the NE horizon at the R0 start, maybe a single recognizable ruined-elevator or
factory-chimney silhouette in the rail town (R3). You get the famous *feeling* of Stalingrad while the
Tiger fights the geography it actually fought. This keeps P6 honest and still markets.

---

## 7. Open questions for the user

1. **Framing (the big one):** confirm **Option A (steppe withdrawal corridor)** vs **B (city)** vs the
   recommended **A-with-distant-B-iconography hybrid**. Everything in §4–§5 assumes A/hybrid.
2. **Which real ~60 km segment** anchors the compression: the **Myshkova→Kotelnikovo relief corridor**
   (advance-then-retreat, tightest to the "Winter Storm" name) or the **Manych→Rostov withdrawal funnel**
   (cleanest "cut-off Tiger escaping west," matches the 503's actual Jan-43 axis)? Both fit 4–5×; pick
   one as the spine and borrow landmarks from the other.
3. **Tiger-timeline honesty:** accept the §1.2 correction (game is the **Jan–Feb 1943 withdrawal**, with
   Winter Storm wreckage as the eastern start), or insist on the literal "Tiger in Winter Storm" framing
   (which would require knowingly placing Tigers in a battle they weren't in — a P6 violation)?
4. **R5 recast:** approve replacing **"Black Belt taiga"** with a **balka/ravine** (or Don-floodplain
   scrub) close-terrain region? Strongly recommended for grounding; needs a nod to the blockout arm.
5. **Compression style:** uniform ~4.6× vs the recommended **anisotropic** squeeze (empty steppe 6–8×,
   landmark clusters 1–2×)? Affects how "honest" the relative layout feels.
6. **How loudly to quote Stalingrad:** how much distant city iconography is acceptable before it reads as
   "this is the city" (and re-opens the P6 problem)?

---

### Sources
- Operation Winter Storm — Wikipedia: https://en.wikipedia.org/wiki/Operation_Winter_Storm
- Operation Little Saturn — Wikipedia: https://en.wikipedia.org/wiki/Operation_Little_Saturn
- 503rd Heavy Panzer Battalion — Wikipedia: https://en.wikipedia.org/wiki/503rd_Heavy_Panzer_Battalion
- s.Pz.Abt 503 at Rostov — tiger1.info: https://tiger1.info/unit-page/503-Rostov
- Heavy Tank Battalion 503 with Army Group Don — WarHistory.org: https://warhistory.org/@msw/article/heavy-tank-battalion-503-with-army-group-don-in-southern-russia
- Operation Winter Storm: Manstein's Attempted Relief — Warfare History Network: https://warfarehistorynetwork.com/article/stunning-victories-ultimate-defeat/
- Operation Winter Storm (terrain detail) — History Rise: https://historyrise.com/operation-winter-storm-the-attempt-to-relieve-the-encircled-6th-army-at-stalingrad/
- Operation Little Saturn / Tatsinskaya raid — stalingradfront.com: https://stalingradfront.com/articles/articles-about-stalingrad-battle/operation-little-saturn/
- Maps of the Battle of Stalingrad — Wikimedia Commons: https://commons.wikimedia.org/wiki/Category:Maps_of_the_Battle_of_Stalingrad
- The German Relief Effort, Dec 1942 (map) — historyofwar.org: https://www.historyofwar.org/Maps/maps_stalingrad9.html
- Winter Storm 12–18 Dec 1942 (map) — onwar.com: https://onwar.com/wwii/maps/efront/13efront.html
- Failure of Winter Storm, 29 Dec 1942 (map) — Omniatlas: https://omniatlas.com/maps/europe/19421229/
- Winter Storm & Little Saturn combined map — The Map Archive: https://themaparchive.com/product/operations-winter-storm-little-saturn-1230-december-192/
- "Stalingrad to Berlin" (US Army, public domain operational maps) — HyperWar: https://www.ibiblio.org/hyperwar/USA/USA-EF-Defeat/USA-EF-Defeat-3.html
- Digital History Center atlases — West Point: https://dhc.westpoint.edu/atlases/
- "Nach Stalingrad!" Winter Storm campaign study (terrain/villages) — littlewarstv.com: https://www.littlewarstv.com/uploads/2/3/7/7/2377799/nach_stalingrad_campaign_1.26_update.pdf
