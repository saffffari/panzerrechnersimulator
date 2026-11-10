# Owner requests backlog, 9 November 2026

An audit of what Alex asked for in past chat sessions and what the main build
(`/home/alex/panzer/demo-20261101`, branch `claude/windows-wip-20261101`, checked up to `32cc63c`,
9 November 21:57; the live session is still committing) actually contains. Nothing was run: no game, no build, no GPU work. Status comes
from commit subjects, code and docs only, so "DONE" means "the code and a commit say so", not
"seen working in play". Where the request is about how something looks, sounds or feels, that
limit matters and is stated.

Dates are local (Pacific). "Times raised" counts separate owner messages before tonight's session
(`7908f9f3`); "+N on 9 Nov" marks requests he raised again tonight. Counts are from a manual read
backed by keyword tallies, so treat them as close, not exact.

Statuses: **NOT DONE**, **PARTLY** (what is missing is stated), **DONE**, **SUPERSEDED**,
**UNKNOWN** (what could not be determined is stated).

## The two findings that explain most of the frustration

1. **A week of Codex work never reached the main build.** On 8 November he asked to "merge all of
   the codex commits into main". Five Codex branches are still ahead of main:
   `codex/engine-overhaul-20261107` (22 commits), `codex/integration-20261107` (20),
   `codex/hermes-web-playground` (17), `codex/shot-rings-presets` (8), `codex/armored-calculator`
   (3). They hold what he asked Codex for on 3 to 7 November: the live range playground, popup
   selectors and named presets, frame and scene caching, the two front amber cabin lamps
   (`fc3572f`), removal of the four-colour lamp stack (`d2c8b01`), bounded blood sprays, the web
   port, and the start of the engine overhaul. Every Claude branch checked is fully merged.
2. **Several fixes exist only behind a switch that the launcher does not set.** The gentle eye
   (`KESSEL_EYE=gentle`), the gen2 Tiger (`--gen2`), physical heat refraction
   (`KESSEL_REFRACTION=physical`), the Canyon road debris re-placement (a review gate, live field
   unchanged). He plays the default and does not see them.

## 1. Not done or partly done, most repeated first

| # | Item | What he asked | First asked | Times raised | Quote (date) | Status and evidence | Where the work goes |
|---|---|---|---|---|---|---|---|
| 1 | Frame rate in combat | 60 fps with vsync, never under 30, at 1024x576 on the 4090; no collapse when shooting starts or wrecks burn | 2026-09-08 | about 35, +6 on 9 Nov | "fps tanks when i hit first t34 and never recovers" (2 Nov) | PARTLY. Audit written (`docs/PERFORMANCE_AUDIT_20261109.md`), hit solves moved to workers (`f54838b`, `606b1d0`), smoke clearance split (`6435376`, "partial"), effects rebuilt (`81fc630`). He still reported "lot of 15fps still" after those. No commit states a measured 60. | `src/infantry_swarm.rs`, `src/fx_plumes.rs`, `src/explosion_volumes.rs`, `src/renderer_raymarch.rs`, `src/game_fea*.rs` |
| 2 | Range editor that makes sense | Run, reset, play and fire that behave logically; presets; popup pickers; settings that stay set; real-time | 2026-11-03 | about 40 (3 to 5 Nov, Claude and Codex) | "its been all day and i have no idea what the run reset play fire buttons do" (3 Nov) | PARTLY. Claude's range is merged (`f54838b`) and frozen at `~/panzer/editor-stable` (`c12483d`). Codex's redesign (live playground, presets, popups, caching) is unmerged, see finding 1. | `src/game_fea_range.rs`, the Codex branches |
| 3 | Game is boring: missions and a reason to play | Orders, missions, a campaign structure, characters or stakes | 2026-10-01 | about 20 | "the only problem is that i think this game might be boring" (4 Oct) | PARTLY. Expedition and mission systems, the Bridge take-and-hold, the order teleprinter exist (`src/expedition*.rs`, `src/mission*.rs`, `src/order_printer.rs`). The story is an outline document only (`6e4f11f`). Direction is undecided (campaign with a persistent crew, or one reconstructed battle; his note of 8 Nov). | Design decision first |
| 4 | Daylight lights the cabin through the ports | Vision ports, periscopes and the sight as real light sources; day inside must differ from night; light shafts | 2026-09-04 | about 16, +1 on 9 Nov | "light still doesn't seem to make it into the interior from the side vision port" (2 Nov) | PARTLY. Apertures are measured light sources (`c6a97ca`), shafts are traced (`shaders/rm_medium.wgsl` `rm_cabin_beam`, stills set in `src/sky_review.rs:255`). On 8 Nov he still asked to "lean more on the exterior light sources during the day", and on 9 Nov whether inside and outside can both be exposed. Not verified in play. | `shaders/rm_aperture.wgsl`, `shaders/rm_medium.wgsl`, `src/daylight_sampling.rs`, `src/cabin_medium.rs` |
| 5 | Eye adaptation and exposure | Stop the picture swinging to black or white; see the cabin and the ports at once; slower | 2026-09-08 | about 13, +4 on 9 Nov | "this eye adjustment thing has caused a ton of pain is it really worth it?" (3 Nov) | PARTLY. Local adaptation switched off (`017da8b`). The gentle eye exists but is opt-in: `7080e6c`, `865d808` ("Back to the wide eye by default"). On 9 Nov the gun sight still goes black on a flash. | `src/observer.rs`, `shaders/exposure.wgsl`, `src/eye_local.rs` |
| 6 | Main gun sound | A boom with a high ring; use his reference recordings; not a deafening blast | 2026-09-05 | about 14 | "main gun firing sound needs to change its still that deafening blast" (28 Oct) | PARTLY. His recordings and picks are wired (`512a715`, `b81950d`, `f44096d`, `bdee3c9`). His last ask, 1 Nov ("find one online for both inside and outside"), has no commit. Whether the current sound satisfies him is unknown. | `src/audio_main_gun.rs`, `src/main_gun_candidates.rs` |
| 7 | Night too dark, gun sight black | A deep-blue moonlit night you can still play in; the sight usable at night | 2026-09-05 | about 11, +1 on 9 Nov | "at night its shouldn't be like pitch black in the gun sights" (28 Oct) | PARTLY. Moon phases, more and brighter stars (`4e443c5`), a hand torch (F). The sky is a measured model with no brightness floor (`src/celestial_sky.rs` header), so a moonless night is as dark as measured. No commit makes the night bluer or the sight brighter. | `src/celestial_sky.rs`, `src/observer.rs`, `src/tiger_gunner_sight.rs` |
| 8 | World feels empty | Rubble, wrecks, bodies and debris everywhere, placed where they would plausibly be | 2026-09-25 | about 10 | "exponentially more rubble/debris/trash/destruction" (2 Nov) | PARTLY. The bridge map got its war debris (`690ceea`, `6cedf4c`). His 8 Nov ask to move the Canyon road debris beside plausible sources is a review gate only: `6802b02` says "live field unchanged". | `src/scatter_field.rs`, `src/canyon_map.rs` |
| 9 | Repair as gameplay | Visible damage, clear cues, track link and pin work, every part field-repairable, animated repairs | 2026-07-23 | about 10 | "everything should be field repairable" (2 Nov) | PARTLY. Track link and pin phases (`src/track_repair.rs`), the flat repair manual (`src/repair_manual*.rs`), workshop parts. The 42 animated repair sequences live in the Blender rig; the game reads none. | `src/track_repair.rs`, `src/tiger_gen2.rs`, `src/workshop_runtime.rs` |
| 10 | Dusk and dawn bluer, blue haze | Less pink and purple at the ends of the day; blue haze in the air, not white scatter | 2026-09-05 | about 9 | "less pink PLEASE less pink and purple" (2 Nov) | PARTLY. A blue-hour band in the day's pacing (`07861bb`). The haze commit (`89e5712`) whitens the horizon. No commit states a bluer dusk or haze. Needs a look. | `src/physical_environment.rs`, `src/celestial_sky.rs` |
| 11 | Interior sparkle and splotches | No low-sample fireflies, banding or sampling squares on the cabin walls and in haze | 2026-09-05 | about 8 | "it \"sparkles\" all of those fireflies change frame to frame" (28 Oct) | UNKNOWN. Related fixes: range TV static (`3813b3c`), fire and smoke fizz (`2e7f1df`, 9 Nov). Whether the cabin walls are clean now can only be seen in play. | `shaders/rm_integrate.wgsl`, `src/probe_temporal.rs` |
| 12 | Audio popping and dropouts | No clicks or stutter when the frame rate drops or keys are pressed | 2026-10-01 | 6 | "audio is like freaking out there is this popping sound" (1 Nov) | NOT DONE. Only measured: starved audio blocks are counted and logged (`6b37f7b`, `src/audio_output_buffer.rs:47`). No fix since. | `src/audio_output_buffer.rs`, `src/audio_engine.rs`, `src/audio_master.rs` |
| 13 | Bailing enemy crews | Survivors climb out through opened hatches, some burning, panic and run; can be shot | 2026-10-03 | 5, +1 on 9 Nov | "they should also run away not chill by the tank" (8 Nov) | PARTLY. They can be shot, they run, they climb in procedural poses (`307f160`). Hatches opening and men on fire are not in any commit. | `src/enemy_crew.rs`, `src/soviet_animation.rs` |
| 14 | Music and ambience pacing | Much quieter than gunfire; states hold for minutes and fade slowly | 2026-10-03 | 5 | "we are switching way too quickly between songs or acoustic environments" (3 Oct) | UNKNOWN. `src/soundscape_director.rs` exists; dwell times were not checked against his numbers (2 to 3 minute loops, a minute of quiet before calm returns). | `src/soundscape_director.rs`, `src/soundscape_program.rs` |
| 15 | Friendly AI Tigers you talk to | Two AI Tigers under command that take typed orders and answer | 2026-10-03 | 5 | "two ai tigers who you can chat via llm order and comms with" (27 Oct) | PARTLY. `--platoon` fields two AI Tigers (`src/platoon.rs`); the teleprinter sends typed lines to a radio model (`src/radio_llm.rs`). Typed orders steering the platoon were not verified. | `src/platoon.rs`, `src/radio_llm.rs` |
| 16 | Lag when the Tiger is hit | No freeze on a hit or a penetration | 2026-11-03 | 4, +1 on 9 Nov | "non penetrating shells freeze up the game" (3 Nov) | PARTLY. Solves moved to workers (`f54838b`), live play no longer waits for them (`606b1d0`, 9 Nov, not yet confirmed by him). | `src/game_fea.rs`, `src/fea_damage/` |
| 17 | Gen2 Tiger in the game | The true-scale rig as the player's tank, with its materials, crew and equipment | 2026-11-08 | 3, +2 on 9 Nov | "lets wrap up this scene and bring it into the game" (9 Nov, 01:49) | PARTLY. Drawn only with `--gen2`, described as a first rough fit (`9a32e14`, `src/tiger_gen2.rs`). The default is still the old Tiger. | `src/tiger_gen2.rs`, `docs/` import plan (`d585fae`) |
| 18 | Web version of the range | A browser version with simple controls for laptops | 2026-11-03 | 4 (Codex) | "adapt this to web based version that can run on laptops" (3 Nov) | NOT DONE in main: no wasm target in `Cargo.toml`. A version exists on `codex/hermes-web-playground`, unmerged. | Codex branch |
| 19 | Cupola and periscopes that turn | Look around from the cupola or the loader's periscope without turning the hull | 2026-09-09 | 3 | "could the cupola rotate independently of the tank?" (13 Sep) | NOT DONE. No yaw control found in `src/tiger_cupola.rs` or `src/tiger_periscopes.rs`. | those two files |
| 20 | Mop | A tool to clean blood and extinguisher foam off the interior | 2026-09-08 | 3 | "we need a mop somewhere" (8 Sep) | NOT DONE. No code. | `src/equipment.rs`, `src/surface_blood.rs` |
| 21 | Star-shaped particles and star glints | One centred square per particle, never a star; no unexplained star flashes | 2026-11-02 | 6 across both, +1 on 9 Nov | "never use them for anything now everything is using them" (3 Nov) | PARTLY. Small chips fill their square (`858472a`), scripted flares retired (`f6f3c41`). On 9 Nov he sees "star things" on the optics; cause not identified. | `shaders/rm_sparks.wgsl`, `shaders/rm_lens.wgsl`, `src/raster_optics.rs` |
| 22 | Broken hand props on foot | Luger, cigarette, lighter and case draw correctly; no exploded or see-through meshes | 2026-11-07 | 3 | "pulled apart in a million directions" (8 Nov) | NOT DONE. No commit after 8 Nov touches these models. | `src/hand_prop_models.rs`, `src/p08.rs`, `src/cigarette.rs`, `src/rigid_triangle_pages.rs` |
| 23 | Fanatec wheel and pedals | Wheel base, pedals, paddles and force feedback from hits, firing and ground | 2026-11-08 | 4 (one thread) | "how hard would it be to wire in the fanatec wheel base" (8 Nov) | NOT DONE. No code. It was put as a question; no answer was acted on. | `src/gamepad.rs`, `src/haptics.rs` |
| 24 | World editor | Place assets, set home views, build levels in the same editor | 2026-09-13 | 3 | "lets finish the world editor functionality previously planned" (5 Nov) | NOT DONE. The range's WORLD tab is greyed "until built" (`ada1e5d`). The older editor (`src/editor.rs`) is the one he called unusable. | `src/game_fea_range.rs`, `src/editor.rs` |
| 25 | Hot metal stays glowing | Glow for 10 to 20 seconds, decaying slowly | 2026-11-02 | 2 | "it should glow for a good 10-20 sec slowly decaying" (2 Nov) | PARTLY. Cooling follows a physical conduction time (`657881b`), not the linear 20 seconds he named. Actual visible duration not measured. | `src/hot_surfaces.rs`, `src/incandescence.rs` |
| 26 | Shell hits shove the target | A big round rocks a T-34; momentum goes into the hull | 2026-11-02 | 2 | "doesn't have any momentum transferred to it at all" (2 Nov) | UNKNOWN, probably not done. `src/tank_push.rs` covers ramming only; no impulse from a shell hit was found. | `src/tank_push.rs`, `src/vehicle_ground.rs` |
| 27 | Night light sources on the maps | A street lamp on the bridge, a lit window, a distant fire | 2026-10-01 | 2 | "so at night there are some spots illuminated" (2 Nov) | UNKNOWN. Lantern meshes and the eye's handling of them exist (`src/urban_tableau_meshes.rs`, `091ff58`); placement on the bridge map was not confirmed. | course manifests, `src/river_crossings.rs` |
| 28 | Garage on the main menu | A night-time workshop sandbox to try repairs and inspect the tank | 2026-09-13 | 2 | "start a garage item in the main menu" (8 Nov) | NOT DONE. `garage` is only a headless still scene (`src/main.rs:2671`, `:3733`). | `src/main.rs` title menu, `src/workshop_runtime.rs` |
| 29 | Tow cables you can handle | Carry the hook, rope follows, hook both ends, pull with the half-track | 2026-11-02 | 2 | "you should be able to grab the ends of the tow cable in game" (8 Nov) | NOT DONE in the game. Cables exist in the Blender rig (`6e86a63`); the game has a separate winch line. | `src/recovery_runtime.rs`, `src/tiger_gen2.rs` |
| 30 | Sub-machine gun and rifle in the tank | An MP40 and a Kar98k on a rack, with their own ammunition | 2026-10-02 | 2 | "kar90k and mp40 model lets add those to a rack" (3 Oct) | NOT DONE. Only sound-level table entries mention them. | `src/equipment.rs`, `src/small_arms_runtime.rs` |
| 31 | Sounds fall off with distance | Distant impacts and gunfire much quieter | 2026-11-01 | 2 | "sounds need to decay more" (1 Nov) | UNKNOWN. The launcher's default playback curve is described as physical falloff; no commit after his message changes it. | `src/sound_propagation.rs`, `src/reproduction.rs` |
| 32 | Start beside the tank, crank start | Begin on foot next to the Tiger; a hand-crank engine start from the rear | 2026-11-08 | 1 | "lets start the game beside the tank" (8 Nov) | NOT DONE. Only a sound note for the inertia starter (`src/acoustics_foley.rs:1129`). | `src/canyon_game.rs`, `src/cold_recommission.rs` |
| 33 | Text popups bottom-left | Move on-screen text popups out of the way | 2026-11-01 | 1 | "move these text popups to the bottom left" (1 Nov) | NOT DONE. The interaction hint is still drawn bottom centre (`src/main.rs:14552`). Which popups he meant was not pinned down at the time. | `src/main.rs`, `src/ui.rs` |
| 34 | Stuck after leaving the gunner's seat | Do not trap the player in front of the gunner | 2026-11-01 | 1 | "i keep getting stuck after exiting gunner seat" (1 Nov) | UNKNOWN. No commit names it. | `src/hatch_traversal.rs`, `src/crew_personal_space.rs` |
| 35 | Merge the Codex work | Check and merge the Codex commits into main | 2026-11-08 | 1 (the "merge everything" instruction itself: about 25) | "merge all of the codex commits into main" (8 Nov) | NOT DONE. See finding 1. | `git merge` of the five branches, with review |

## 2. Done, and asked for more than once (do not redo)

"Tonight" means committed on 9 November during the live session and not yet confirmed by him.

| Item | Times raised | Evidence |
|---|---|---|
| Space enters, leaves and exits hatches and stations; LMB takes a station | about 12 | `12e3266` (8 Nov), `docs/CONTROLS.md` |
| Hammer no longer blocks interaction | 2 | `12e3266` |
| LMB fires the gun only in the sights | 2 | `docs/CONTROLS.md` solo table |
| Arrow keys lay the gun from outside the sights | 3 | `77652ad`, `docs/CONTROLS.md` |
| Coax MG on MMB in the sights | 2 | `src/main.rs:10296` |
| Commander marks a bearing, gunner slews | 3 | `8190cf8` (V key in the current table) |
| Driver cruise on Q and E | 1 | `src/crew_solo.rs:722` |
| Casings clinking and rattling without end | 5 | `bc93785`, `0f3ae80` |
| Smoke hiding every impact | about 12 | `edfecd7`, `938f596`, `77d96e7` |
| Blood: more, smaller droplets, pooling, darker | about 12 | `eee2233`, `dfc9a90`, `f846d39` |
| Bigger explosions, turret throws, whole-load cook-off, flame vents | about 14 | `b961453`, `f2e62b4` |
| Tracers at bullet size; tracer no longer hides the shell | about 7 | `55d8d35`, `7418d97` |
| Smaller, sharper sparks | about 10 | `4579b17`, `858472a` (look not verified) |
| Dense grass with wind, hull push, sparse flowers, fireflies, butterflies | about 12 | `10f67ce`, `229cf77`, `src/meadow_fauna.rs` |
| Day and night lengths and slower transitions | about 8 | `src/renderer.rs` `FULL_DAY_S`, `TWILIGHT_S`, `NIGHT_S`; launcher `--day-cycle 6`; 07:00 start `0719687` |
| White one-pixel stars, bigger discs, Moon phases | 4 | `4e443c5`, `83ba8a0` |
| Low volumetric cloud inside the map | 5 | `965f31d`, `4ee2b75` |
| Tiger exterior neutral grey, warm white interior, grime | about 8 | `bc146f5`, `723ead1`, `630a368` |
| Armour thickness, angle and fragments on enemy tanks | about 12 (he said 20) | `src/armor_damage.rs`, `src/fea_damage/`, `b961453`, mantlet as armour `eb5ec8a` |
| Tracks sink into the snow | 2 | `src/vehicle_ground.rs:57` (0.30 m) |
| Glass on periscopes, side ports and the sight | about 8 | `c793f74`, `e796b78` |
| Hover outline and bottom-centre hint | about 9 | `4545181`, `179704a`, `a82c7b2` |
| Breech state lamps on both sides, sight lamps | about 6 | `src/game.rs:9604` (green, amber, red) |
| Crew status lights with pictograms | 2 | `src/crew_status_panel.rs`, `src/indicator_pictograms.rs` |
| Driver's dials (revs, coolant, fuel drums), Braun-style panels | 3 | `c250b78`, `src/cabin_panels.rs:18` |
| One flat captures folder, 4K JPEG stills, gallery page | 5 | `CLAUDE.md` lines 169 to 189, `07c07ca`, `586ec1a`, `tools/capture_gallery.py` |
| No agent renders while the game or editor is open | 6 | `add9efa`, `ca695a4`, `da7b2e8` |
| Evolved T-34 AI as the default everywhere | 2 | `7e41e1f` |
| Solo crew on the playtest menu, with a crewed switch | 4 | `7e0e2e3`, `e0a2803` |
| Cigarette: ash tap, 1.5x speed on foot, night-vision gain | 4 | `83ba8a0`, `d0499be` |
| Scripted flares and on-screen radio lines removed | 3 | `f6f3c41` |
| Blast front moves grass, trees and men | 3 | `fdd49ae` |
| Crew helmets, 1.25x | 4 | `b7e2028` |
| Gun fumes only after the breech opens | 2 | `651679a` |
| Camp just over the first hill on the bridge map | 2 | `d0499be` |
| TPZ dial dimmer | 4 | `6ffab31`, `eb292f3`, `c6a97ca`, `f6f3c41` |
| TPZ dial deep orange | see section 4 | `22f2337` (tonight, not rendered) |
| Less camera motion in the sight | 4 | `f0a6758` (tonight) |
| Loader stops loading | 2 | `50ee0bf` (tonight) |
| Shell needs a shove to ram home | 2 | `666ce12` (tonight) |
| Tiger too easy to break | 4 | `a3785f3` (tonight) |
| Infantry shoot at the Tiger | 4 | `d64d1aa`, `9fc293a` (tonight) |
| Death sequence shows where the round came from | 3 | `14c9d37` (tonight) |
| Panzerfaust stowed in the tank | 5 | `6e4f11f` (tonight, playtest courses only) |

## 3. Superseded and unknown

### Superseded (he reversed or replaced it)

- **Living AI crew as the singleplayer default** (3 Oct, 27 Oct). Later: "the solo one man crew
  gameplay is just vastly more fun. multicrew should be reserved for splitscreen" (2 Nov), and
  again on 9 Nov.
- **X as the leave-station key** (29 Oct). Later: "instead of x to exit gun sights or cupola we
  just use spacebar" (8 Nov).
- **E as the interact key** (3 Oct). Later the same night: "im remembering why we stopped using
  e"; then "lmb should be the de facto like interact/equip/enter/use button" (29 Oct).
- **Scene and frame caching in the range** ("i asked claude 10 time", 3 Nov). Two hours later:
  "no frame caching no sim caching the whole thing needs to run in real time" (3 Nov).
- **The 100-run tissue tuning batch.** "100 rounds no longer needed we have the FEA range sim"
  (3 Nov).
- **A physical 3D repair-manual book** (28 Oct). Later: "i dont think this in game physical book
  is gonna work at this pixel res" (2 Nov); the flat manual replaced it.
- **Wide two-eyed gun sight** (28 Oct). Same day: "lets also return to monocular sight".
- **Winter white camouflage and snow lying on the tank.** "lets focus on the german grey for now"
  and "we can keep the tank grey ... not worried about now" (2 Nov).
- **Pastel palette, then AgX.** Replaced by the locked look of 2 Nov (deltap-real, 2,048 colours,
  1024x576). On 9 Nov he added: "anything that is \"locked\" is unlocked".
- **No spoken dialogue.** "the original no speech rule is not a hard rule" (8 Nov); crew voices
  are on trial, undecided.
- **LAN co-op.** "lan co-op is tabled for now" (29 Oct).
- **In-game graphics settings page** (1 Oct), then "we just have a single graphics setting"
  (2 Oct), then "a low graphics setting a high graphics and an ultra" (28 Oct). The menu exists
  (`src/graphics_menu.rs`); the last word favours presets.
- **Local eye adaptation.** Built 3 Nov, rejected the same day, switched off (`017da8b`).
- **Oblique section cuts.** "no more oblique sections only ortho" (3 Nov).
- **Many background agents.** One Opus 5.5 at most, queue the rest (8 Nov).
- **Dial colour, twice.** "glowing dial could more yellow just a tad" (3 Oct) and "the dial can be
  less orange but dimmer" (2 Nov) are both overtaken by his orange requests. See section 4.

### Unknown (could not be determined without running the game)

- Whether the cabin walls still sparkle (item 11), whether sparks and fire now read the way he
  described, whether the outline he reported lost on 1 Nov is back in every station.
- Whether distant sounds are quiet enough (item 31) and whether music states hold long enough
  (item 14).
- "Orange pointy things" in the player's own body when looking down (2 Nov). Solid hands were
  merged (`2f7f34e`); the body mesh was not named in any commit.
- Tank feel: weight, grip, power through branches (6 messages from 3 Oct to 28 Oct). Slope hold
  (`78dcf1e`) and woody crush (`6a9cad2`) landed; the feel is his to judge. Turning overshoot was
  changed tonight (`d308515`).
- Infantry thrown back and dismembered by hits (3 Oct, three messages). `src/infantry_casualties.rs`
  exists; behaviour not checked.
- Canyon run with procedural contents and a start near combat (3 Oct, 1 Nov). Not checked.
- The Berlin map's fidelity (2 Nov: "berlin needs to be recognizable"). A map exists (`b011207`,
  `--berlin`); accuracy not assessed.
- Elbows, wrists, feet and knees in the Blender character rig (8 Nov). Blender files were not
  opened.
- The "engine overhaul" he asked Codex for on 7 Nov. It was started on
  `codex/engine-overhaul-20261107` (unmerged); how far it got was not assessed.
- Two older requests with no trace either way: an opening line of text before the campaign
  (4 Sep), and spent cases thrown by the player through any hatch (R throws a carried case per
  `docs/CONTROLS.md`; hatch targeting not checked).

## 4. The "light dial"

**Which one.** The TPZ: the turret position indicator, the lit clock-face dial inside the turret
that shows where the gun points relative to the hull. He calls it "the turret position dial",
"the tpz dial", "the dial". It is not the dome lamp and not the indicator lamps beside it, which
he praised separately ("i LOVE these indicator lights left of it", 30 Oct).

**What colour.** A deep, glowing orange, a little dimmer than it was. Not yellow, not white, not
unlit.

**His messages about it, in order.**

| Date | His words | What it asked |
|---|---|---|
| 9 Sep | "i love that glowing effect on the turret position dial" | keep the glow |
| 2 Oct | "really like the look of the dials glowing in this frame. also we should add a light glass cover on it" | glass cover |
| 3 Oct | "glowing dial could more yellow just a tad" | slightly more yellow (the one early message that points the other way) |
| 28 Oct | "the tpz needle is too wide. also missing a reflective glass cover" | needle, glass |
| 28 Oct | "this dial is fucking too bright its like the sun inside the tank" | dimmer |
| 30 Oct | "this dial isn't illuminated" then "i prefer the old dial that was glowing orange" | lit, orange |
| 31 Oct | "make the orange from this dial a deeper orange. and just a tad less bright" | deeper orange, slightly dimmer |
| 2 Nov | "the dial can be less orange but dimmer" | read literally: less orange, dimmer |
| 2 Nov | "dial back for tpz is now metal and should be white" | white dial back |
| 8 Nov | "tpz dial is too bright in the canyon run as well. lets dim it a bit" | dimmer by day |
| 9 Nov | "that light dial ive asked 1000000 times to make more orange" | more orange |

**What happened.** The 31 October request was carried out on the Windows side (deeper orange, ten
percent dimmer; the comment is still in `src/interior_rig.rs`). On 2 November his message "less
orange but dimmer" was taken at its word and the dial was made paler (`eb292f3`, linear colour
1.0, 0.50, 0.14). That is the pale dial he has been looking at for a week. It was dimmed again on
8 November with a day setting (`f6f3c41`). Tonight at 21:46 the live session put it back to deep
orange (`22f2337`, linear 1.0, 0.24, 0.02, about sRGB 255, 134, 38), brightness unchanged, not
yet rendered or seen by him.

So the count is two explicit orange requests for the dial before tonight (30 and 31 Oct), four
dimmer requests (28 Oct, 31 Oct, 2 Nov, 8 Nov), and one message of his own that sent it the wrong
way. Two related requests for the cabin as a whole: "these yellows should be more orange inside"
(3 Oct) and "lets bring back the orange interior cabin lighting" (30 Oct).

Constant to check after any look change: `TPZ_ILLUMINATOR_LINEAR_COLOR` and
`TPZ_ILLUMINATOR_INTENSITY` in `src/interior_rig.rs`.

## 5. Method

**Read, line by line with a streaming script (never whole):**

- Claude Code, Linux: every `.jsonl` under `~/.claude/projects/-home-alex-panzer`,
  `-home-alex-panzer-fea-damage`, `-home-alex-panzer-fx-fire-sparks`,
  `-home-alex-panzer-fx-overhaul-20261109` (22 session files; subagent transcripts scanned and
  excluded as not his typing). `-home-alex-panzer-tank-simulator` holds memory notes only.
- Claude Code, Windows drive: `/windows/Users/saffari/.claude/projects/C--shop*` (21 files). These
  hold the October work and were not in the brief; they turned out to be where much of the
  repetition happened.
- Codex, Linux: the 22 top-level threads whose working directory was `~/panzer` or
  `~/tank_simulator`, found through `~/.codex/state_5.sqlite` (read-only) and read from their
  rollout files (some zstd-compressed). 226 Codex subagent threads were skipped.
- Codex, Windows drive: the one main thread in `C:\shop\panzerrechnersystem`.
- Memory notes in `~/.claude/projects/-home-alex-panzer/memory/` as a cross-check.

**Kept:** entries typed by him (`origin.kind == "human"`, queued prompts from `queue-operation`
enqueue records, and Codex `role: user` messages). **Dropped:** tool results, system reminders,
task notifications, cross-session and coordinator messages, hook output, slash-command echoes,
compaction summaries, prompts written by one session for another, and Codex context blocks.
Exact repeats were removed (the two forked sessions `a73a07a8` and `a8cbe039` share a prefix and
are counted once).

**His messages found: 1,694.**

| Source | Session | Messages | Dates |
|---|---|---|---|
| Claude, Windows | `C--shop` (3 files), `4c12ad1b` | 44 | 30 Jun |
| Codex | `019f924c`, `01a05b17`, `01a06735` | 12 | 23 Jul to 3 Sep |
| Codex | `01a07009` | 54 | 4 to 5 Sep |
| Codex | `01a08420` | 195 | 8 to 16 Sep |
| Codex | `01a08a07`, `01a09f2d`, `01a0a893`, `01a0abb4` | 12 | 9 to 16 Sep |
| Claude, Windows | `92d12888` | 3 | 19 Sep |
| Codex | `01a0d9da` | 42 | 25 Sep |
| Codex | `01a0eead`, `01a0eeb2`, `01a0efba` | 22 | 29 Sep |
| Claude | `f38a0190` | 18 | 1 Oct |
| Codex | `01a0f8ef` | 208 | 1 to 3 Oct |
| Claude | `393792f6` | 106 | 3 to 4 Oct |
| Claude, Windows | `e3f2646d` | 123 | 21 to 29 Oct |
| Claude, Windows | `6a731d3d`, `0dfc0144`, `79b540ec`, `51df7894` | 28 | 27 to 28 Oct |
| Codex, Windows | `01a1859e` | 84 | 28 to 31 Oct |
| Claude | `a73a07a8` (with fork `a8cbe039`) | 286 | 1 to 9 Nov |
| Claude | `a559a6bc`, `d5b9cd9f`, `9c15b12c`, `39f5cbcb` | 20 | 1 to 7 Nov |
| Claude, fx-fire-sparks | `148750c4`, `09c25a96` | 97 | 1 to 2 Nov |
| Claude, fea-damage | `3f06b69b`, `5b6108e5`, `a927b027`, `8b6764e1`, `17196d5a`, `fab86b7b`, `1b01b29e` | 133 | 2 to 3 Nov |
| Codex | `01a1a3a3`, `01a1a454`, `01a1a496`, `01a1a940`, `01a1aa2f` | 154 | 3 to 7 Nov |
| Claude (tonight) | `7908f9f3`, fx-overhaul `b8fbe602`, `1136035a` | 53 | 9 Nov |

There are no chats between 5 and 20 October in any store that was found.

**Status checks:** `git log` on the main branch (996 commits), `git rev-list` of every other
branch against it, `grep` over `src/`, `shaders/`, `tools/`, `docs/`, and `docs/CONTROLS.md`,
`CLAUDE.md` and the launcher `tools/launch_demo_20261101.sh`.

**Not read or not possible:**

- Images he attached. Many requests point at a screenshot ("this", "these ones"); the text alone
  was used, which is why the 2 Nov dial message can only be read literally.
- One Codex rollout is missing from disk (`01a089ad`, 9 Sep, zero tokens used).
- Codex subagent threads, ChatGPT app conversations, and anything said by voice outside these
  tools.
- Roughly 40 Codex messages are his pastes of another agent's output; they were left in the count
  but not treated as requests.
- Requests about the hardware console, the traffic-light timer and the surgery prototype were
  left out as other projects.
- Long design monologues (campaign structure, upgrades, crew rules of 28 to 30 Oct) are
  summarised as item 3 and not itemised.

**One thing to act on outside this list:** two API keys were pasted into chat in plain text and
sit in the transcript files (an Anthropic key on 27 Oct in the Windows store, an ElevenLabs key on
9 Nov in `a73a07a8`). They are not reproduced here. Rotating them is worth a minute.
