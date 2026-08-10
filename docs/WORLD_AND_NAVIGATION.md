# Kessel — WORLD & NAVIGATION

> Status: design locked with the user 2026-07-01; `src/world.rs` is the first implemented slice
> (act/landmark/goal data model + navigation math). This doc is the authority for how the world is
> structured and how the player finds their way through it. See also [[STALINGRAD_CAMPAIGN.md]] for
> the fiction and `docs/ENGINE.md` for how things are drawn.

## 1. The shape of a run

A full playthrough is a **~4-hour run** in **three open "acts"** (`world::ACT_COUNT`). Each act is a
**bounded, high-detail open-world map** you load into across a **loading screen**, roam freely, and
leave by reaching its **goal gate**. The arc is an escape that becomes a homecoming that is also hell
— "the war comes home":

| Act | Title | Setting / season | The beat |
|----|-------|------------------|----------|
| 1 | **Die Stadt** | Stalingrad, **winter**, urban | Claw out of the ruined city. Dense rubble, close-range knife-fights, hard nav. |
| 2 | **Die Heimkehr** | Germany, **full-bloom spring**, open country | Push to the rendezvous X where relief was promised — and find it **empty, abandoned; your home leveled, an active warzone.** No one came. The gut-punch. |
| 3 | **Kameraden** | Germany, spring, a small town | Link up with **two additional Tigers** — you KEEP your own Tiger (the same hull, the same interior, all the way from Stalingrad; one vehicle for the whole game, no King Tiger handover), **command the platoon of three by radio**, face new heavy Soviet tanks, and fight the **final battle** vs a massive armada before it reaches Berlin. |

The Stalingrad→Germany leap (Act 1→2) is a deliberate thematic/surreal transition across the loading
screen, not literal geography.

**Retry model** (revised from pure permadeath): death in Acts 1–2, or during the Act-3 approach,
**restarts you at the beginning of that act** (per-act checkpoints; the difficulty is re-running the
whole act clean). The **final battle is ONE shot** — win or lose, the game **ends either way**;
playing again means starting over from Stalingrad (Act 1). `ActMap::final_battle` flags the terminal
act.

Why acts instead of one continuous streamed world:
- **Detail.** "Much higher detail" is only affordable when each map is bounded (no LOD/streaming
  compromise). Three dense ~5 km maps beat one thin 18 km corridor.
- **Engineering.** Each act loads as its own bounded scene — the current architecture nearly does
  this already (`Game::build` builds one scene; `start_play` rebuilds per session). No infinite
  terrain streaming, no rewrite of the flat scene list, needed to ship the world.
- **Pacing.** Three beats give a clean escalation and a natural place to turn the season and raise
  the stakes.

The two homeland acts are dressed in **spring** (`Season::Spring`) — a green reskin (terrain tint,
foliage, sky) of a heightfield window, not the Stalingrad snow DEM; `game.rs` owns that terrain-source
choice. Act 1 is the winter DEM window (`west_offset_m`).

## 2. Navigation: no waypoints, a paper map + a compass

The player is given **no waypoint marker, no minimap breadcrumb, no objective arrow.** The world is
**fully open** — you may explore any of it — but you *want* to beeline the goal. You find your way
**by hand**, using two diegetic tools:

- **The paper map (toggle: `M`).** Held in the lower part of the view with a **pocket compass + pencil**
  while the world keeps moving. It
  shows the act's terrain in a hand-drawn style, its **landmarks** (silhouette-distinct: churches,
  factory chimneys, water towers, grain elevators, bridges, monuments, wreck fields — see
  `world::LandmarkKind`), and exactly **two marks**: a printed **spawn cross** at your start and a
  hand-inked **red X** at the friendly lines you must reach. Nothing tracks your live position. At
  night the flashlight throws a warm cone over the paper. You can **draw on it with the pencil**
  (append-only persisted strokes) — mark a route, a threat, a stash. There is no eraser.
- **The compass.** A physical pocket compass rests on the raised map and exists nowhere else. It
  tells you which way you're pointed — not where the goal is. There is no permanent heading ribbon.
  You combine compass + map + the landmarks you can *see* to triangulate and steer.

The landmarks are the whole trick: they are placed to be **visible across the act** (tall
silhouettes, `height_hint`) so hand-navigation actually works by eye. "The chimney is on my right,
the church behind me, the red X is past the chimney" — that's the intended loop.

## 3. Gates

- **Spawn gate** — where the act drops you (`ActMap::spawn`), near the east edge; also the printed
  map cross.
- **End gate** — the friendly lines (`ActMap::goal`), near the west edge; the red X. Driving/walking
  within `goal_radius_m` completes the act (`ActMap::reached_goal`) → loading screen → next act.

`ActMap::bearing_to_goal` exists only so the map can ink the red X in the correct place; it is
**never** surfaced as a live waypoint.

## 4. Module map (implementation)

- **`src/world.rs`** *(done)* — `ActMap` (bounded DEM window, spawn/goal gates, seed, ruination,
  landmark list), `LandmarkKind` (silhouette kind → hero asset + map glyph + place label),
  `Landmark`, and pure nav math (`bearing`, `heading_of`, `compass_label`, `turn_delta`,
  `reached_goal`). Unit-tested. Consumes nothing else; safe backbone.
- **`src/game.rs`** *(next)* — `Game::build` takes an `ActMap`: feed `west_offset_m`/`half_extent_m`
  to `terrain::load`, `seed`/`ruination` to `city::generate`, place landmark hero meshes at their
  plan positions (grounded on the heightfield), spawn the Tiger at `spawn`, and each frame test
  `reached_goal(player_plan)` → act-complete. The escape axis stays −X/west.
- **`src/ui.rs`** *(done)* — `draw_held_map`: partial-view held paper, pocket compass, scout marks,
  and append-only pencil strokes. `M` is a hands-busy live state, not a pause overlay.
- **`src/terrain.rs`** *(small change)* — generalise `load` to accept a westward offset + half-extent
  from the `ActMap` instead of the hard-coded 2 km/4 km defaults.

## 5. Build order

1. `world.rs` data model + nav math + tests. ✅
2. Landmark hero-mesh placement in `Game::build` + terrain window from the act (visible world).
3. Physical pocket compass, visible only with the raised map. ✅
4. Held paper map (`M`): terrain sketch + landmark glyphs + spawn cross + red X. ✅
5. Pencil drawing on the map (append-only mouse strokes; no eraser). ✅
6. End-gate arrival → loading screen → next act; wire the three acts + the per-act-restart / one-shot
   finale retry model into `start_play`.

Deferred to the Act-3 milestone (pinned, not built now): the **spring green reskin** of the terrain;
the **Tiger platoon** + **radio command** (see [[PLATOON_COMMAND.md]] / kessel-platoon-command —
two AI-crewed wingman Tigers, reusing the player's Tiger rig; the playable King Tiger was CUT
2026-07-02: one vehicle, one interior, the whole game), **heavy Soviet tanks**, and the
**final-battle** endgame.

## 6. Open questions

- **Map fidelity:** hand-drawn sketch baked per act vs. a stylised top-down render of the actual
  scene. Leaning baked sketch (cheaper, more diegetic, forgiving of procedural variation).
- **Whether Act 1 doubles as the current test scene** or acts are gated behind Kampagne only.
- **On-foot (Absitzen) segments** (see [[kessel-fps-mode]]) — which acts force a dismount (Act 2's
  river crossing and the Act-3 town fight are natural candidates).
- **Spring terrain source:** reskin a DEM window green vs. a purpose-built pastoral heightfield.
