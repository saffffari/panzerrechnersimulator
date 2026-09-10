# Panzerrechnersystem — Stalingrad Map & Campaign Course

> **Current campaign authority (2026-09-02):** `docs/STATUS.md` and
> `docs/LINUX_4090_HANDOFF.md` supersede this document wherever they differ. The canonical
> destination is Berlin and the unwinnable Reichstag last stand. A hidden escape by surviving crew
> to an intact cottage is only an optional, near-impossible, deliberately ambiguous hope coda.
> Earlier Halbe, hometown/pasture-as-main-ending, conventional final-tank-battle and
> one-man-protagonist concepts below are retained as historical design or implemented-prototype
> context, not the current target.

**Map:** real Volgograd/Stalingrad DEM, 100×100 km, centered **48.708 N, 44.513 E** —
which is essentially the **heart of the city (Mamayev Kurgan / center)**. So real
geography maps straight onto the terrain. Tiger ≈ 25 km/h → ~4 h edge-to-edge.

**Coordinate convention used below:** kilometres from map center.
`E+` = east, `N+` = north. So `(-18, +2)` = 18 km west, 2 km north of center.
The 100 km box runs roughly **−50..+50** on each axis.

**Terrain logic (matches the DEM):**
- **West bank = ravine-cut upland** — the city site. Sits ~150–214 m, cut by deep
  **balkas (gullies)** running down to the river: historically *Krutoy, Banny, Dolgiy,
  Tsaritsa*. These ravines are the natural approaches/cover and should read in the 25 m DEM.
- **The Volga** splits the map ~near center (slightly E), flowing N→S.
- **East bank = flat floodplain** (Volga–Akhtuba meadows), ~−14 m. Soviet rear; massed
  "Trans-Volga" artillery fired over the river into the city.
- **The underground** (home for the subway/metro-horror assets): Stalingrad's command
  posts and aid stations were dug into the **riverbank cliffs and the Tsaritsa storm-drain
  tunnels** (the "Tsaritsyn bunker"). That tunnel network = the transit-horror layer.

---

## City gazetteer — the ribbon (north → south, all on the west bank)

The 1942 city was a long, narrow strip: ~40 km N–S, only a few km wide, pinned between
the steppe and the river.

| Location | Approx (E,N) km | History | Game role |
|---|---|---|---|
| **Rynok / Latashanka / Spartanovka** | (−1, +15) | Northern tip; 16th Panzer reached the Volga here 23 Aug 1942 (same day as the great air raid that burned the city). | North gate; "how it began" vista — wrecked first-day armor. |
| **Tractor Factory (Dzerzhinsky / STZ)** | (−2, +12) | Built T-34s; brutal Oct 1942 fighting. | Industrial maze, northern anchor. |
| **Barrikady (Gun Factory) + "Lyudnikov's Island"** | (−2, +9) | Artillery works; 138th Rifle Div was cut off in a tiny riverbank pocket here Nov 1942. | Vertical fighting; the cut-off-pocket set-piece. |
| **Krasny Oktyabr (Red October Steel)** | (−2, +7) | Steel mill; floors and furnaces fought over. | Worst of the *Rattenkrieg* (rat war), point-blank. |
| **"The Tennis Racket" (Lazur chemical plant)** | (−3, +6) | Rail loop shaped like a racket beside Red October. | Distinct landmark / chokepoint. |
| **Mamayev Kurgan (Height 102.0)** | (−2.5, +2) | The dominant hill, a Tatar burial mound; changed hands repeatedly; commands the center and the crossings. **The** landmark. | Central boss-hill / commanding-fire chokepoint. |
| **City Center** — Central Station, Univermag, Red Square (Pl. Pavshikh Bortsov), Pavlov's House, 9 January Sq. | (−2, 0) | Station changed hands ~13×; **Univermag basement = Paulus's HQ & surrender 31 Jan 1943**; Pavlov's House held ~58 days. | **START.** Claustrophobic rubble, night, fires, ash-snow. |
| **Tsaritsa Gorge** | (−2, −1) → river | Ravine slicing the center; NKVD HQ & the Tsaritsyn underground bunker. | Tunnel/underground horror gateway. |
| **Grain Elevator** | (−2, −5) | Iconic siege in the southern city. | Lone-tower set-piece. |
| **Yelshanka / Dar Gora / Minina / Kuporosnoye** | (−3, −7) | Southern districts; 62nd Army's southern flank. | South-city sprawl. |
| **Beketovka (+ GRES power station)** | (−3, −12) | Soviet-held suburb; southern Uranus pincer linked through here. | Soviet strongpoint. |
| **Krasnoarmeysk** | (−4, −20) | Far-southern city. | Southern map anchor. |

---

## Western steppe — the Kessel (cauldron) interior & ring

This is the dead zone the player escapes *through* — west of the city, the frozen pocket.

| Location | Approx (E,N) km | History | Game role |
|---|---|---|---|
| **Orlovka salient** | (−6, +8) | NW finger held by the "northern group." | NW flank. |
| **Gorodishche** | (−8, +6) | Village behind the factory district. | Steppe transition. |
| **Gumrak airfield + rail jct** | (−12, +3) | Secondary airfield; Paulus's HQ before the end. | First airfield hub — wrecks, frozen equipment. |
| **Pitomnik airfield** | (−18, +2) | **The** main airlift field — the lifeline that died; Ju-52 graveyard. | Largest dead-zone set-piece; the false hope of escape-by-air. |
| **Rossoshka** | (−28, +5) | Western defensive line; today the German cemetery. | Open tank country; **SU-100 ambush ground**. |
| **Karpovka** | (−33, −5) | SW pocket. | Penultimate line. |
| **Marinovka ("the nose")** | (−40, −3) | Western *tip* of the pocket — the breakout point. | Last enemy line of the escape; you punch through here. |

---

## Encircling jaws & relief axis (mid/far map)

| Location | Approx (E,N) km | History |
|---|---|---|
| **Kotluban corridor / Samofalovka / Kuzmichi** | (−20, +35) | The northern "land bridge" between Don & Volga; repeated bloody Soviet offensives against the German flank. |
| **Sarpa Lakes / Tundutovo** | (−5, −30) | Southern Uranus pincer (51st Army) jump-off. |
| **Kalach-on-Don** | (−70, 0) | Where the Uranus pincers met **23 Nov 1942**, closing the cauldron. At/just beyond the **western edge** — use as a horizon objective. |
| **Don River bend** | (−60..−70, —) | Western boundary. |
| **Kotelnikovo / Myshkova R. (Op. Winter Storm)** | far SW, off-map | Manstein's Dec 1942 relief drive; stalled at the Myshkova ~50 km short. Off-map — the direction of "rescue/home." |

---

## East bank (floodplain)

| Location | Approx (E,N) km | Role |
|---|---|---|
| **Volga islands** (Zaitsevsky, Golodny, Sarpinsky) | (+1, 0..−3) | Crossing stones / artillery spits. |
| **Krasnaya Sloboda** | (+3, 0) | Soviet rear directly opposite center; main crossing. |
| **Trans-Volga artillery line** | (+5..+10, —) | Massed Soviet guns firing over the river. |
| **Akhtuba** | (+20, +5) | East-bank river town; open floodplain beyond. |

---

## The player's course — Escape the Cauldron

A single **west-bound axis**: from the river-side ruins, across the dying pocket, out
into the living world. The world *changes biome and tone* as you go — that progression
is the story.

| # | Beat | Where (E,N) | Biome / tone | Gameplay |
|---|---|---|---|---|
| 1 | **The Start — city ruins on the Volga** | (−2, 0) | Night, fire-glow, ash-snow, grey rubble. Claustrophobic. | Lone-crew Tiger; *Rattenkrieg*, point-blank rubble fighting. Goal: break west. |
| 2 | **Mamayev Kurgan** | (−2.5, +2) | Bare scarred hill, smoke. | First real gun duel; commanding fire from the heights. |
| 3 | **Factory maze / Tsaritsa exit** | (−2..−4, +6) | Twisted steel, vertical ruin. | Tight industrial fighting; optional Lyudnikov's-Island / Tennis-Racket detours. |
| 4 | **City edge → balkas** | (−5, +2) | Ruins thin; broken snowy gullies. | Transition; ambushes from the ravines. |
| 5 | **Gumrak airfield** | (−12, +3) | Frozen wreck-field, abandoned kit, corpses. | Open ground begins; first **SU-100** contacts. |
| 6 | **Pitomnik airfield** | (−18, +2) | Ju-52 graveyard; the airlift's dead. | Big dead-zone set-piece; emotional low. |
| 7 | **Rossoshka steppe** | (−28, +5) | Open white steppe, mud beneath. | The **SU-100 gauntlet** (the ML hunters) — open tank country. |
| 8 | **Marinovka "the nose"** | (−40, −3) | The last grey line. | Hardest escape fight; you punch through the western tip. |
| 9 | **OUT — green grass** | (−45..−50) | **Snow gives way to green. Firing stops. Silence.** | **The catharsis / first ending beat.** You're out of the cauldron. You made it. |

### Coda — The Home Return
| # | Beat | Biome / tone | Gameplay |
|---|---|---|---|
| 10 | **Pasture drive** | Quiet green pasture, warm light, exhale. Heading home/to friendly lines. | Calm transit. No threat — until you crest a rise. |
| 11 | **The Ambush (reveal)** | Green pasture torn by smoke; a Tiger brewing up. | **First time you ever see friendly Tigers** — and they're being ambushed. The **Tiger AI are now on YOUR side.** |
| 12 | **Final Tank Battle** | Open pasture arena. | You + allied Tiger AI vs the enemy armor (the SU-100 hunters). **Destroy all other tanks, or die.** Win = survive & clear the field. Lose = death. End. |

**The shape:** death (frozen grey cauldron) → struggle west → **life (green)** → false peace
→ the war follows you home → last stand. The green grass is both a literal biome line
(out of the encirclement's dead zone) and the emotional summit; the coda earns its
gut-punch precisely because you thought it was over.

**Why it fits the fiction:** lone-crew Tiger escaping the *Kessel*; the SU-100 learned-policy
hunters (the ML thread) are the recurring predator and the final-battle enemy; friendly
Tiger AI use the same player control inputs; looting/EVA happens in the dead set-pieces
(Pitomnik, the ruins); the underground tunnel network carries the transit-horror layer.
