# The Windows work-in-progress: what it built, planned and left open (1 November 2026)

The Linux port of the Windows checkout's uncommitted work (commit cb585b8 on
`claude/windows-wip-20261101`, over origin/main 63269ea) is the starting point for the overhaul the
owner asked for: "much of the ideas were good but the execution sloppy". This map is the inventory
that overhaul starts from. Status words are the Windows agents' own evidence (their build numbers
b20..b58, `docs/EXPEDITION_DEVELOPMENT_LOG_20261030.md`, `docs/FOUNDATION_REVIEW_20261031.md`,
`docs/CREW_FOUNDATION_AUDIT_20261031.md`) plus the owner's Linux playtest of 2026-11-01. Verdicts
are proposals for the owner to confirm, not decisions.

## 1. Built (runs, has harness evidence)

| Feature | Where | Evidence | Verdict |
| --- | --- | --- | --- |
| Expedition coordinator: 16 named people, one persistent Tiger, eight preparation dispatches, death, succession, camp relocation, finale | `src/expedition.rs`, `expedition_proof.rs`, `expedition_runtime.rs` | 21 CPU tests; `expedition-proof`; live relief review 3 successions | **keep**; the state model is sound |
| Versioned world save/resume (v2 coordinator save + complete-owner world adapter, build/source fingerprint gate) | `expedition_save*.rs`, `expedition_world_save.rs`, `game_world_state.rs`, `renderer_world_state.rs` | b58 cold resume exact for 11 owners, active loader carry/ram | **keep**, but the fingerprint gate (`env!("KESSEL_SIMULATION_FINGERPRINT")`) ties saves to a build: decide whether that is acceptable for players |
| The theatre "The March": 12 km, 12 sites, river, two crossings, roads | `catalog/expedition_theatre.v1.json`, `tools/worldgen/build_expedition_theatre.py`, `expedition_theatre*.rs` | 13 mission-rule checks (staged, not played) | **rework**: the terrain is three sine ridges and one gaussian hill, visibly synthetic; the sites are points with guards. Needs authored geography (Gaea or hand-placed) and real settlements |
| Support camp: tents, two beds, 16 cots, awnings, orders table, cooking stove, people on duty/rest loops, named medic | `support_camp*.rs`, `camp_people*.rs`, `camp_dressing.rs`, `camp_terrain_pad.rs` | camp-review stills; medical review b55 15/15 | **rework**: boxy canvas props, no interior lighting pass, actors slide; keep the roster and anchors, redo the dressing and animation |
| Solo living crew: Q/E cruise notches, WASD orders from command stations, V designation, K gunner experiment, empties rack, side hatch | `crew_solo.rs`, `crew_status_panel*.rs` | 9 crew tests; cruise tests | **keep the Oct 27 behaviour** (the owner: "working well"); the Oct 30 additions need a pass: the camp intro parks the Tiger cold with no cue (fixed 2026-11-01: the driver cranks it on the first order), the crew panel pictograms are placeholder |
| Physical repair manual: 16 pages, symptom-led branches, hinged page turns, zoom, bookmark | `repair_manual*.rs` | 6 CPU tests; v12 stills | **keep the idea, rework the content**: pages are schematic placeholders; the first chapter (tracks) must connect to the real track-repair interaction; key moved from B to I (B is the binoculars) |
| Field aid, stretchers, casualty extraction, carrier transport, medical tent care, succession to a stabilised patient | `field_aid.rs`, `crew_body_recovery.rs`, `crew_drag_*.rs`, `expedition_medical*.rs`, `patient_loading_sweep.rs` | b55 continuous journey 15/15, 343 s | **rework the presentation**: three-minute drag across camp, rigid bed pose with raised knees, no animation; the state machine itself can stay |
| Halftrack transport prototype (Polyfork APC): boarding, drive, ramp, cargo, winch, reverse winch loading | `halftrack*.rs`, `carrier_*.rs`, `recovery_*.rs`, `tools/import_halftrack_prototype.py` | recovery review 28/29, b31 26 checks | **replace the model with the OT-810** (section 4); keep the carrier/winch/cargo rules and re-fit them to the new interior |
| Tiger fuel service (cans, caps, finite transfer), cold recommission through the starter, engine operability | `tiger_refuel*.rs`, `standalone_fuel.rs`, `cold_recommission*.rs`, `engine_operability.rs`, `engine_cooling.rs` | 4 fuel paths; recommission 15 checks | **keep** |
| Workshop source registration: optic replacement, battery work lamp, track mounts, vision mounts, engine coupling/cassette/cover latches | `workshop_*.rs`, `catalog/tiger_*service*.json`, `tools/prepare_engine_*.py` | 69 checks for optic + lamp; engine cover motion still collides with the turret track carrier | **keep the part/connection/job model; finish one chain end to end** (tracks first, per the Codex handoff) before more parts |
| Static wreck grounding, vehicle rollover, trees' root/soil support, loose rocks, scatter support | `static_wreck_*.rs`, `vehicle_rollover.rs`, `tree_ground_support.rs`, `scatter_support.rs` | b48/b56 focused tests pass; real map: 146 admitted / 118 unresolved placements | **keep**; the 118 unresolved canyon placements are an open defect |
| Crew personal space, corpse retention, retained KIA bodies, hatch posture, loader hand service, cartridge in the hull frame | `crew_personal_space.rs`, `crew_corpse_render.rs`, `crew_hatch_posture.rs`, `crew_loader_work.rs`, `loader_hand_service.rs` | b57 2,390 tests pass; moving-loader clip 11/11 | **keep**, animation quality is the overhaul target (no finger curl, no planted-foot turns, abrupt reach) |
| Renderer work in the same days: exposure meter scheduling (bit-identical), cached optics invalidation, warm cabin calibration, weather density shader | `exposure_meter_review.rs`, `raster_optics.rs`, `shaders/weather_density.wgsl` | 144 GPU equivalence cases | **keep** |

## 2. Planned (roadmap stages, all boxes unticked at the cutoff)

`docs/EXPEDITION_IMPLEMENTATION_ROADMAP_20261030.md` stages 0-11. Nothing below has acceptance
evidence; several have partial code (named in column three).

| Stage | Intent | Partial code |
| --- | --- | --- |
| 0 | baseline reconciliation, amended written-word principle, keep the old campaign selectable, fix cleared/bypassed reports and the interior-walking order reset | `CLAUDE.md` amended; order reset still open |
| 1 | persistent expedition state and reliable saving | largely built (section 1) |
| 2 | responsive controls and embodied crew: input table, cruise, remote arrows, crew diagram, bodies block seats, crew commands (hold, follow, board, carry, assist, take cover) | cruise, arrows, panel built; commands absent |
| 3 | physical manual and the first complete repair (tracks) | book built; the track chain is not connected |
| 4 | first recovery operation as the playable milestone (camp, two approaches, a disabled Tiger, recovery transport, repeated failure) | the pieces exist separately; no composed scenario |
| 5 | wounded survivors and the medical tent | built, presentation weak |
| 6 | source models, physical assembly, tools, advanced repairs (1,481 Tiger CAD parts) | optic, lamp, engine cover started |
| 7 | halftrack, heavy recovery, asset admission | Polyfork prototype; OT-810 is the owner's chosen base |
| 8 | camp, bodies, locomotion, animation | the weakest area: no locomotion set, no joint limits, no handholds |
| 9 | fuel, ammunition, stowage, supply missions | fuel cans, ammunition carts started |
| 10 | 32.768 km streamed theatre, 48 sites, monochrome atlas | 12 km fixed theatre, monochrome map linework started (`map_linework.rs`) |
| 11 | mission director, frontline, narrative; orders at the radio station | mission sheet (`mission.rs`) exists for the Canyon run; no director |

## 3. In progress at the cutoff (b58, 31 October 18:13)

- Engine cover full motion collides with the rear turret track-carrier hardware; the full engine
  exchange is blocked on it.
- Real-map placement: 118 unresolved originals on the canyon (93 unstable, 25 search budget).
- Held-cartridge picking is not watertight at tessellation seams.
- Medical bed placement is one rigid transform; the final care camera frames the wrong height.
- The 1,800-second armed travel run never passed (commander killed at 97 s; track damage at 754 m).
- `KSP` was open on the Windows GPU during every measurement: no frame-rate claim exists for any of
  this work. The Linux Canyon run at 1024x576 with the deltaP look is the first clean environment.

## 4. The owner's 2026-11-01 playtest and the fixes on this branch

- Dismounted look-down showed the open neck of the hollow source body (fixed: body set back 0.20 m).
- "Driving doesn't work": the camp introduction parks the Tiger cold (fixed: the driver cranks the
  engine on the first drive order). The owner wants the Oct 27 cupola/sight/head-out WASD orders
  kept as the reference behaviour.
- The repair manual took the binoculars' B key (fixed: manual on I, B is the binoculars).
- The owner prefers a playtest that starts near combat, not at the camp; the Canyon v2 run with the
  deltaP look is the current playtest (`tools/launch_demo_20261101.sh --canyon --look deltap`).
- Standing crew bindings were read relative to the checkout, so every worktree lost them (fixed).

## 5. The OT-810 as the halftrack

`/home/alex/Downloads/ot-810.zip`: `source/ot810.zip` with `ot810.FBX` (hull 21,767 vertices /
17,061 triangles, `track` 940, `uk_vz59` machine gun 2,514), diffuse/specular/bump DDS and PNG
textures. Inspection (Blender 5.1, headless): the hull mesh is **2,746 disconnected shells**, so
road wheels (70-unit squares, four per side), the front wheels, roof panels, doors and fittings can
be separated by loose parts without cutting; about 31 % of faces point inward, so a partial
interior shell exists but there is no cab, seat or floor detail. The FBX unit scale is unknown
(hull 1,300 units long for a 5.92 m vehicle); fix the scale from a known dimension. No licence
file is in the archive: record provenance in an `admission.json` before it enters the asset library.
The OT-810 is the Czechoslovak post-war Sd.Kfz. 251 derivative; document how it stands in for the
1943 Hanomag (vz. 59 MG to be swapped or hidden). The owner wants: the two roof panel doors as
working hatches that cargo can be hoisted through, hatches the crew and player operate, and an
interior to start from, as the fully armoured version (no open-top variant).

## 6. Proposed overhaul order

1. Controls and embodiment on the Tiger (stage 2): the Oct 27 driving as reference, the first-person
   body (done), the crew panel, and a composed combat-adjacent start for playtests.
2. The OT-810: admission, metric bake, loose-part separation, hatches, interior, then re-fit the
   carrier/winch/cargo rules to it.
3. One complete repair chain (tracks) through the manual, with the book's pages drawn from the real
   geometry.
4. Camp and medical presentation: locomotion set, bed poses, dressing.
5. The theatre: authored geography before more sites; the mission director after that.
