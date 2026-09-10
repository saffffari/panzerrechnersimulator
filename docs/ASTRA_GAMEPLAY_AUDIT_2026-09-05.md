# Astra gameplay and embodiment audit — 2026-09-05

This is a source-and-local-assets audit of the integrated Linux checkout. It does **not** certify a human playthrough, animation quality, audio-device playback, or performance. No GPU job or Cargo build was run for this audit. Source line references describe the September 5 working tree; function names are included because other audit work is happening concurrently.

The mechanical prototype is considerably further along than a design document: tank stations, a manual reload, on-foot movement, infantry, supply recovery, weapons, a three-act campaign, and a usable environment editor all have runtime implementations. The biggest embodiment gap is equally concrete: **ordinary solo play hides the living crew, and the player's equipment moves without visible first-person arms.** The replacement Soviet character has a completed integration *interface*, but its actual source/rig/17 runtime poses are absent here.

## Findings that change the next milestone

| Area | Verified implementation | Gap or consequence |
| --- | --- | --- |
| First-person hands | Rounds/cases, hatch/breech levers, extinguisher, repair hammer, P08 magazine/brass, binoculars and Panzerfaust have object transforms and interaction state. | No dedicated first-person arm/hand renderer slots or operation animations were found. `binocular_prop_model` explicitly disallows an unregistered arm stand-in (`src/game.rs:408`). A floating-shell reload demonstration is not a hand animation demonstration. |
| Living Tiger crew | Five authored identities, each with seated, standing and relaxed static GLBs; actual local authoring master contains one skin and 39 joints. | Authoring master has **zero animation clips**. The inspected seated runtime GLB has zero skins and zero clips. Live figures use fixed post transforms and seated/standing selection; they do not reach for controls or reload. |
| Solo crew | `Game::update` passes `input.coop` to `drive_lan_crew_rigs` (`src/game.rs:12874`, `12960`). | `drive_crew_rigs_for_view` hides both living-crew slot arrays and returns for `coop=None` (`src/game.rs:14691`). Normal solo does not synthesize a five-man NPC crew. This falls short of the current persistent-five-man design even though LAN/couch/showcase figures exist. |
| Soviet replacement | Validated 17-pose atomic loader, shared moving instance pools, and deterministic idle/walk/run/aim/fire/death timing (`src/game.rs:1539`, `10215`; `src/soviet_animation.rs:9`, `53`, `281`). | `assets/flavor/Characters/soviet-ww2-soldier-cc-by-4` does **not exist** locally. Runtime therefore selects `soviet_infantry_mesh`, a ten-box body, plus a separate box rifle (`src/game.rs:1508`, `9717`). Animation tests can pass with no replacement character installed. |
| Soviet NPCs | 24 actor slots: 12 escort tanks and 12 seeded guards/patrols. Vision, hearing, alarm spread, windup, firing, suppression, lethal hits, corpses and one-shot loot exist. | Mounted infantry run a passive pass with no human target; a head-out mounted commander is not given the dismounted infantry target. Actor movement uses terrain height without structure collision/pathfinding. Escort corpse slots are reused when later campaign contacts spawn. |
| Panzerfaust | Six visible rack tubes; dismount issues at most one ready tube plus one spare; remount returns unfired tubes; weapon selection, ADS, firing, gravity, 2.7 s tube replacement, backblast and contact effects exist. | Anti-tank damage is a gameplay shortcut: a 3.6 m-radius enemy sphere receives the bomb and any armed hit destroys that enemy directly. No HEAT jet/armor/module/crew solution. Blast visuals do not imply explosive injury simulation. |
| Map | Live, hands-busy map state, route sheets, scout marks, append-only pencil strokes and heading compass exist. | The map, compass and pencil are 2D screen-space rectangles/text (`src/ui.rs:1959`, `2305`), not authored 3D held props receiving tank illumination. Calling them physical in comments does not change that. |
| Editor | Placement, transforms, multi-selection, prefabs, scene blocks, terrain tools, sound emitters, lights, save/reload and direct play-hop are implemented. | Existing tools are worth preserving. First-class building floor/room/portal/door/loot authoring is still unfinished; the current playable modular building is a vertical slice. |
| Campaign | Persistent in-memory Tiger/resources/discoveries, generated route/map retrieval, encounters, transitions and finales are implemented. | No disk save/resume system; the running finale remains the older Halbe/town/breakout logic rather than the locked Reichstag ending (`src/campaign.rs:411`, `1769`; `docs/STATUS.md:11`, `113`). |

## Soviet asset recovery and NPC behavior

The exact source is identified in `tools/soviet_character_recipe.py:22–31`: Michael Jones, “WW2 Soviet Soldier,” Sketchfab identifier `331f36489ce440549dc1a6acb82fd931`, CC BY 4.0. The recipe preserves downloaded source bytes and generates an editable rig plus static deformation frames; see its provenance writer at line 346 and authoring/runtime distinction at lines 394–402. This is an identified acquisition/bake task, not a need to select a different character again.

Local inspection found only these character directories: `deadman-psx`, `german-panzertruppe-adrielcz-cc-by-4`, `german-ww2-soldier-cc0`, and `skeleton-character-psx-vinrax-cc-by-4`. The selected Soviet source was not found in the runtime character root. Recover/download the exact source, preserve attribution, bake all 17 poses, verify the receipt, add the completed asset to the reviewed runtime cook, then inspect its actual raster/shadow/ray-query playback. Do not count an animation contract or procedural fallback as completion.

The gameplay interpretation requested by Alex is broadly present: rifles cannot harm the armored, mounted Tiger; on foot the player is exposed and each connected rifle shot inflicts 34 HP against 100 HP (`src/game.rs:335`; `src/foot.rs:23`). `update_infantry` at `src/game.rs:17532` performs perception and resolves shots; `update_infantry_passive` at line 17624 deliberately supplies no target. A 0.5 s aim windup and 1.6 s shot interval are configured in `src/infantry.rs:31–32`. Sound events drive both audible reports and AI alarm behavior, although their simulation models are separate.

Specific limitations to test before claiming credible infantry:

- `Soldier::approach` and `patrol` (`src/infantry.rs:746–777`), `reanchor_escort` (`:247`), and the learned motor override (`:500–526`) advance positions and ground them on the heightfield. They do not collide against building walls or plan through doors. Cover is used for perception and shot resolution, not locomotion. An intact wall may stop a rifle shot while the soldier walks through that same wall.
- `resolve_infantry_shot` (`src/game.rs:17760`) uses a sphere centered at the player's eye plus live terrain/structural occlusion, rather than anatomical small-arms injury. P08/MG infantry hits are gameplay lethality, not the detailed tank crew damage solver.
- `activate_escort` (`src/infantry.rs:261–283`) moves an actor to the new route, sets it alive, resets loot and revives its animator. The campaign remuster calls it (`src/game.rs:12041`). Bodies survive tank/foot mode switches, but dead escort slots do not accumulate permanently across later contacts.
- Mounted passive behavior means infantry cannot target the exposed head-out commander. Decide whether that exposure should matter before tuning the on-foot threat.
- A corpse yields one or two P08-compatible loose rounds regardless of the actor's rifle (`src/infantry.rs:220`); this is explicitly a survival-game economy shortcut requiring an art/design decision.

## Panzerfaust: playable weapon, incomplete physical model

The chain is real code: geometry is generated in `src/panzerfaust.rs`; `issue_panzerfaust_loadout` / `return_panzerfaust_loadout` (`src/game.rs:15976`, `15988`) conserve unfired tubes; `fire_panzerfaust` (`:17177`) launches a gravity-affected bomb at 45 m/s; `update_panzerfaust_shot` (`:17342`) sweeps its frame segment; `drive_panzerfaust_rack` (`:17488`) renders the remaining rack inventory. Constants retain an 80 m range cap, 8.45° ADS elevation, 2.7 s spare-tube swap, and a 5 m arming assumption (`src/foot.rs:297–302`; `src/game.rs:599`). These are existing tuning values, not historical values independently revalidated by this audit.

The incomplete parts are substantial:

1. The enemy sphere is the final tank collision, so a near miss of the visible armor can still count. An armed tank contact calls `destroy_enemy` without the normal armor solver, incidence, penetration, residual jet, internal component or crew consequences.
2. Contact variants include enemy tanks, structures, hard cover, trees, terrain and range, but not the player's Tiger or direct infantry contact. This is not a complete projectile/world collision contract.
3. Backblast checks Soviet actors within a direction cone, kills inside 3 m and flinches inside 10 m. It has no wall occlusion, enclosure reflection, shooter injury, or crew injury path in this function. It is not a pressure simulation.
4. A terrain/cover detonation calls `spawn_blast`, which only claims a visual fireball slot and dust (`src/game.rs:23242`). The normal HE damage entry point is separate (`apply_authoritative_he_blast`, `:20523`) and is not invoked by these Panzerfaust impacts. Visible nearby infantry are not automatically injured by that blast effect.
5. Launch audio reuses `Sfx::EnemyGun`; tank contacts play `ImpactTank` and `PenetrationHit`. This is not a distinct computed recoilless-launch/HEAT event.

The existing `panzerfaust_sweep_stops_at_eighty_metres_and_arms_at_exact_contact` test checks helpers, not a complete rendered firing sequence. No dedicated Panzerfaust showcase command was found. Add a reproducible near miss, armor hit, unarmed contact, enclosed backblast and inventory-return scenario before describing the weapon as fully tested.

## Tiger operation animation

Useful existing work should become the authority for body animation:

- The cupola hatch lifts 0.15 m and swings 110° with reversible timing (`src/game.rs`, hatch transform block near line 13905).
- Manual breech lever, extracted case, selected rack round, carry inertia, drop/throw, ram and loaded state are represented in the reload interaction path (near `src/game.rs:13251–13448`).
- `drive_interior_parts` (`src/game.rs:15125`) rotates the TPZ needle, elevation wheel and commander traverse flywheel, and drives chamber/rack visibility. Elevation wheel rotation is currently `gun_pitch * 40`; traverse uses the declared 595-turn ratio from `src/interior_rig.rs:84`.
- Living crew mesh selection is static seated/standing; `crew_live_model` (`src/game.rs:2820`) supplies fixed post transforms. Ragdoll anatomy is a separate casualty system and does not supply working arm animation.
- Loader movement remains a floor-bound compartment controller with a breech obstacle (`src/interior_rig.rs:58–65`), not general collision against every visible interior triangle.

First-person arms should follow the same object state, contact targets and timing as these operations. The minimum credible implementation needs two-handed shell support, release/drop continuity, hand-to-lever contact, wheel grasp during lay, hatch reach, and visible interruption when the player changes tasks. Then extend the same rig to other living crew. Avoid baking a cinematic sequence that looks correct only in a showcase while gameplay still moves disconnected props.

## Existing shortcuts and honest showcase coverage

Run from `/home/alex/panzer/tank_simulator`. These are **source-confirmed commands, not executions performed by this audit**. Build once through the normal launcher; the direct binary examples deliberately avoid starting concurrent compilers. All new capture output belongs in the flat repository-root `captures/` directory per `AGENTS.md`.

```bash
# Normal title/menu and campaign; launcher builds the checkout unless explicitly skipped.
./tools/launch_kessel.sh

# Jump directly to the live Act-I world; this is an editor/test hop, not a complete campaign run.
KESSEL_HOP_ACT=1 ./target/release/kessel

# Actual environment editor and complete Pizza gallery.
./target/release/kessel edit 1
./target/release/kessel edit yard

# Existing focused mechanics proofs.
./target/release/kessel reload captures/astra_reload_20260905
./target/release/kessel soviet-shot captures/astra_soviet_20260905.png
./target/release/kessel ui heldmap captures/astra_heldmap_20260905.png
./target/release/kessel terror-video captures/astra_terror_20260905

# Authored source/pose validation once the missing asset has been restored.
python3 tools/verify_soviet_character_bake.py assets/flavor/Characters/soviet-ww2-soldier-cc-by-4
```

`reload` provides a still series of shell/case operations (`src/main.rs`, `run_reload_demo`). `soviet-shot` drives a real dismount/infantry shot sequence but can still display fallback bodies. `terror-video` includes timed reload/fire/hit choreography and stereo audio, but installs a staged crew and writes a separately authored soundtrack; it is not evidence of normal solo NPC behavior or live audio capture. The retained `captures/hal4090_fx_terror_video_20260903.mp4` is an existing artifact, not a fresh result from this audit. `absitzen`, `hatch` and `climb` also exist, but some of their fixed output names need collision-safe modernization before repeated audit captures.

Editor essentials: Tab opens the library; drag/drop or V places; G/R/T transforms; sounds and lights have separate selectable entities; F5 saves `manifest_edits.json`; F10 saves and launches the current act near the editor camera. `EditorWorld::save_edits` (`src/editor.rs:936`) and F5 handling (`:7503`) persist sounds, lights, surfaces, procedural layers, scene recipes and Tiger props. This is real implementation, although not freshly validated through the visible editor in this audit.

For a new player's first loop, current controls are E/click on station anchors, LMB for the active station's gun/hand, RMB for closer view/aim, M for map, Q to back out, and hold X for 1.5 s at an open cupola to dismount. On foot 1/2 select P08/Panzerfaust and R services the selected weapon. The top comment of `src/main.rs:5` still advertises older Space/V/C controls; `docs/CONTROLS.md` and actual event handling supersede it. This is a small onboarding fix worth making immediately.

## First playable milestone acceptance

Use one 10–15 minute Act-I slice rather than claiming the entire campaign is finished. It should start at the Tiger, teach physical operation through legible interactions, reach one entered building/story location, survive a combined tank/infantry contact, recover a map/supply object, and return to the Tiger or a clear terminal outcome.

1. **A real human can complete the slice.** Record the executable revision, seed, controls and one uninterrupted attempt. Scripted success does not substitute for discovering/reaching controls at the intended pixel resolution. Include recovery from a missed click, interrupted reload, closed hatch and dropped round.
2. **Embodiment exists in solo.** The same persistent crew identities remain visible at work; hands maintain contact with a shell, breech lever, wheel and hatch; local arms are visible without camera clipping; duties stop or transfer after a casualty. A separate in-game operation reel shows each action with sound.
3. **Replacement infantry is actually on screen.** Exact source provenance and bake pass; runtime logs the full 17-pose set; idle/walk/run/aim/fire/death visibly match actor motion; agents obey walls and doors and stop shooting through intact cover. Mounted versus dismounted exposure behavior is deliberate and demonstrated.
4. **Panzerfaust claims match evidence.** Before physics work, label it a gameplay prototype. For acceptance as physical damage, use actual armor contacts and the shared damage authority; demonstrate near misses, obliquity/arming, backblast occlusion and conserved tube inventory, with audiovisual timing tied to those events.
5. **One map sheet is readable and grounded.** Landmarks correspond to the played terrain; drawing survives raising/lowering and the relevant checkpoint; no hidden player-position marker appears. Upgrade its visual treatment with a real held prop and scene lighting if physical-map presentation remains the target.
6. **Storytelling is spatial and audible.** At least one researched scene has intentional prop arrangement, route sightlines, local sounds, occlusion and pacing; its meaning can be perceived without an explanatory objective banner. Demonstrate approaching, entering and departing in stereo video.
7. **Progress survives expected use.** Establish disk-save/continue behavior before asking someone to commit to a multi-hour campaign. Until then, explicitly treat the slice as a single-session prototype. Protect resource/crew/scene state across retry and act transitions.
8. **Evidence is reviewable.** One short audiovisual reel per mechanic plus the uninterrupted human attempt, each labeled with gameplay versus staged setup and measured solo frame time. Preserve the editor workflow; expand its building authoring only after this common slice is enjoyable to play.

The unreleased ending, crew trauma/rescue system, complete room/door/loot authoring, disk saves, global corpse persistence, and independent simultaneous networked foot avatars remain production work beyond this first slice. Existing docs themselves flag several of these as unbuilt; they should not be silently included in a “playable now” claim.

## Central verification after this audit

The parent audit completed a release build and 710 passing tests (one existing audio-device test
ignored), plus a 12.5-second High mechanics film with stereo audio. That film stages four co-op
bodies and assembles its soundtrack offline; it does not overturn the missing living solo crew,
first-person arms, Soviet model or live acoustic acceptance findings above. See
`ASTRA_NEXT_STAGE_2026-09-05.md` for the media and measured renderer results.
