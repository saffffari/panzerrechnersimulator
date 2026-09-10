# First-person mechanic review — 2026-09-05

This is the source-to-film coverage ledger. No complete-game coverage claim is made. The scenario files are scripts awaiting runtime validation; the first driver film had stereo audio but failed visual review, and the actual driver camera has since been corrected in source. Each new film needs a fresh manifest and visible review.

The recorder consumes the same rodio Sink queues used by live gameplay at 120 simulation steps/s, exports exact 44.1 kHz stereo beside 24 fps player views, and retains a fixed 0.5 export gain plus a documented linear emergency peak guard. Rendering/rest time never advances audio. Ordinary source quantization and approximately 5 ms Sink control polling remain. Sound quality and physical calibration are separate questions.

| Mechanic | Implementation / scenario | First-person view | Remaining evidence |
|---|---|---|---|
| Ignition, forward/reverse, steering and engine load | implemented / fp_driver_controls.json | Driver visor | v1 was audio-present but visually failed: driver visor pointed into compartment. Shared gameplay camera fix awaits v2. |
| Traverse, elevation, 88 mm firing, recoil and spent case | implemented / fp_gunner_controls.json | Gunner TZF plus actual interior surround | Record with source snapshot; verify actions, outcomes, audio and actual view. |
| Coaxial MG burst, cycling and heat | implemented / fp_gunner_controls.json | Gunner optic | Add sustained overheat/cooldown subcase; current short burst does not establish heat behavior. |
| Cupola lid, head-out exposure and binoculars | implemented / fp_commander_observation.json | Commander actual eye inside/outside | Geometry and audio aperture transfer must be evaluated separately; open-lid transfer is not experimentally calibrated. |
| Physical station transitions among five seats | implemented / fp_station_transitions.json | Actual transitioning player | Input requests alone do not prove final seat or readable view; inspect per-seat frames. |
| Foot walk/run/crouch/jump, P08 aim/fire/reload | implemented / fp_foot_movement_p08.json | On-foot actual eye and weapon mesh | Current open-ground case does not establish impact hit detection, cover or casualty reactions. |
| Disposable Panzerfaust aim, fire and fresh-tube replacement | implemented / fp_panzerfaust_handling.json | On-foot physical sights | Tube firing and inventory only: add target armor/jet and backblast witnesses; never claim reusable reload. |
| Lever, casing extraction, rack round, ram and breech closure | implemented / fp_manual_reload.json | Loader real eye and hands | Fixed interaction schedule requires actual validation; automatic debug_reload is excluded. |
| Interior fire and aimed extinguisher/foam | implemented / fp_extinguisher.json | Loader inside compartment | Initial fire is a disclosed staged fixture, not proof that an incoming projectile starts this exact fire. |
| Held map and compass presentation | implemented / fp_map_compass.json | Player held map | Pencil, annotations, objective route updates and local overlay input are not covered. |
| Cupola exit and multi-perch return | implemented / fp_dismount_mount.json | Player actual dismount/mount camera | Solo climb_out currently exits immediately; mounting requires distinct Space edges. This is the existing gameplay boundary. |
| Broken track, grab/pull/link/hammer and restoration | implemented / fp_track_repair.json | On-foot at actual left track | Broken module and kit are staged. Separate toolbox pickup remains unreviewed. |
| 45 mm nonpenetration candidate | implemented / fp_incoming_45mm.json | Commander ordinary seated view | Single fixed seed, actual recorded outcome. Isolated turret OBB bench reports the competing unfiltered whole-vehicle contact; not a complete external trajectory or validated historical damage reconstruction. |
| 85 mm penetration candidate | implemented / fp_incoming_85mm.json | Commander ordinary seated view | Single fixed seed, actual recorded outcome. Isolated turret OBB bench reports the competing unfiltered whole-vehicle contact; not a complete external trajectory or validated historical damage reconstruction. |
| 122 mm internal damage candidate | implemented / fp_incoming_122mm.json | Commander ordinary seated view | Single fixed seed, actual recorded outcome. Isolated turret OBB bench reports the competing unfiltered whole-vehicle contact; not a complete external trajectory or validated historical damage reconstruction. |
| 152 mm external HE blast/spall candidate | implemented / fp_incoming_152mm_he.json | Commander ordinary seated view | Single fixed seed, actual recorded outcome. Isolated turret OBB bench reports the competing unfiltered whole-vehicle contact; not a complete external trajectory or validated historical damage reconstruction. |
| 57/76/100/152 AP, APCR, both impact sides and every crew station | implemented / needs scenarios | Actual Commander/Driver/Loader/Gunner/Radio seat | Public action supports these profiles; 4 current cases do not cover complete ammo × side × seat × hatch matrix. |
| Perforation, spall/fragments, blood deposition, anatomy and secondary ammunition | implemented / needs outcome review | Surviving actual player interior | Incoming scenarios record outcomes but do not guarantee each rare effect occurs. Never search seeds or force KIA for an unlabeled result. |
| Trauma bus, tinnitus and recovery of engine/world audio | implemented / fp_incoming_85mm.json | Actual struck crew perspective | Actual damaging outcome and sustained recovery must be checked from recorded mix; stream presence alone is insufficient. |
| Fire growth, ammunition cook-off, bail and final tank loss | implemented / needs scenario | Commander interior then actual on-foot exit | Extinguisher case aims to prevent cook-off. Separate uncontrolled-fire story needs honest outcome and successful escape inputs. |
| Main gun, sight, traverse, MG, engine, transmission, fuel and radio repair gates | partial / needs scenarios | Affected actual station | Modules exist; do not imply each has a bespoke hand minigame. Audit accepted repairs/parts/time for each module. |
| Crew KIA, persistent body and surviving player takeover | implemented / needs couch driver | Survivor actual viewport | Existing koop stills are not first-person synchronized video. Needs real cooperative authority/frame input in dedicated recorder. |
| Radioman bow MG and Commander fallback | implemented / needs scenario | Radioman actual optic or Commander | Gunner coax film does not establish bow mount behavior. |
| Driver and loader roof lids | implemented / fp_driver_roof_hatch + fp_loader_roof_hatch, pending film | Respective physical crew eyes | H / controller LS-click+A selects the occupied post; ownership, independent edges and LAN v4 lid state pass CPU checks. Driver fixed visor does not show the roof; no exposure or venting shortcut. |
| Select AP versus HE by physical round, including chamber extraction | implemented / needs rack selection helper | Loader eye; Gunner proof after reload | Player T only explains physical selection. Do not use debug toggle_nature as an interaction proof. |
| Drop, recover and juggle loose rounds/casings | implemented / needs scenario | Loader hands/basket floor | Manual reload covers one route; loose-round trajectory, regrasp and obstruction need an explicit case. |
| Physical can pickup, placement, mass-dependent throw and persistence | implemented / needs fixture position | On-foot looking at actual prop | Actions exist in recorder but no inspected reachable fixture is specified yet. |
| Carry can to rear service zone and transfer campaign fuel | implemented / blocked campaign fixture | On-foot Tiger rear service point | Current recorder uses build_act without campaign runtime. Need prepared campaign start, partly empty tank, reachable can; full tank must preserve can contents. |
| P08 casualty and one-shot corpse ammunition loot | implemented / needs targeted scenario | On-foot physical corpse | Stage a living actor, obtain real hit outcome, then approach/search; do not pre-mark corpse looted or fabricate ammo gain. |
| Search shelves, recover supplies and persist emptied container | implemented / needs authored building fixture | On-foot shelf at arm reach | Requires the actual shelf collision/semantic anchor, not arbitrary coordinates or input R at spawn. |
| Find map case, route sheets, supply caches and stow supplies on return | implemented / blocked campaign fixture | On-foot search and physical return to vehicle | Needs campaign start/persistence and reachable authored cache; ordinary act0 does not establish campaign logic. |
| Map pencil, pins and controller map input | implemented / blocked window input adapter | Actual held map | Current dedicated recorder passes empty strokes/pins and no window input. Map presentation case is not annotation coverage. |
| Panzerfaust shaped-charge hit and armor outcome | implemented / needs targeted scenario | On-foot shooter with actual target impact | Current launcher handling fires without an authored armor target. Need a fixed shot and its actual tank damage telemetry. |
| Panzerfaust rear danger to nearby infantry | implemented / needs targeted scenario | On-foot shooter, then ordinary turn to rear witness | Requires explicit living rear actor and exposed geometry; no forced casualty. |
| Soviet infantry detection, suppression, rifle fire, wound/death and cover | implemented / needs scenario | On-foot exposed player versus protected tank view | Public staging exists; no new full first-person audiovisual case has passed. |
| German/Soviet infantry contact and tank anti-infantry MG/HE decisions | integration pending / blocked pending game hooks | Actual player side-on to staged contact line | Hubble is integrating adapter. Record learned-policy/fallback flags and real per-team shots; not every run loads trained weights. |
| Neuroevolved Soviet tank acquisition, flanking, swarming and return fire | implemented / needs live enemy scenario | Commander/driver/gunner player views | Current benign cases banish enemies. Existing third-person tactics clips do not fulfill this camera/audio requirement. |
| Track crush, AP wall passage, HE collapse and changing cover | implemented / needs destructible fixture | Driver approaching wall / gunner engaging / on-foot aftermath | Need separate interaction paths and surviving actual POV; old stills do not count. |
| Tree impact, bending/fall and blast felling | implemented / needs tree fixture | Driver or gunner, then on-foot inspection | Select an actual existing tree; do not spawn a disconnected visual-only falling prop. |
| Doors, openings, stairs/floors, destroyed support and physical cover | implemented / needs authored building fixture | On-foot walk-through | Visual shells and walkable collision must be checked together; collect actual contacts/floor heights. |
| Windblown one-pixel 3D snow, geometry occlusion and sheltered view | implemented / needs first person audio scenario | On-foot outdoors → genuine shelter or tank | Existing 4-second snow proof was silent and a custom camera. It is useful visual evidence but does not count as this review. |
| Dust, haze, blue aperture shaft, lamps and smoke response | implemented / needs first person scenario | Commander/loader eye with real hatch/fixture | Light-layer tests and old custom witness films do not establish all ordinary player seats. |
| 3D authored environmental emitters and local lights | integration pending / blocked scene runtime fixture | On-foot entry/walk sockets with actual player camera | Need real owned cue files and placed sound-producing objects. Distance gain alone is not full wave propagation/occlusion. |
| Full 3D posed skeleton and tank-component X-ray view | partial / blocked requested presentation | Player camera with complete interior retained | Retained region/module snapshots exist. Current schematic/circle overlay is rejected and must not be presented as finished 3D anatomy or organ simulation. |
| Fatal shot reconstruction / armor-hole inspection | implemented / needs first person design | Actual player inspection where possible | Existing cinematic external replay is not first person. Keep diagnostic replay distinct from player mechanic videos. |
| Act transition, route, inventory/fuel persistence and checkpoint recovery | implemented / blocked campaign runner | Player view before/after gates plus genuine menu | Ordinary build_act capture does not exercise prepare/commit/resume. Needs isolated review save slot; never overwrite user campaign. |
| Stützpunkt capture zone, waves and operation result | implemented / needs operation runner | Commander/player seat | Needs operation launch and actual mission progression, not debug-completing all waves. |
| Abandon burning vehicle, search and remount replacement | implemented / needs campaign scenario | On-foot searching, then actual new tank seat | Needs real reachable replacement and persistent inventory/ownership transfer; not current generic mount case. |
| LAN authority, local split-screen, station ownership and disconnect/takeover | implemented / blocked multiplayer runner | Each real player viewpoint separately | Single-client recorder cannot establish network synchronization, controller hotplug or simultaneous physical seats. |
| Launch Playtest/campaign, controls, pause and settings | implemented / blocked window ui capture | Actual player/menu app view | Root separately validates menu/playtest path; this renderer-only harness bypasses the title menu. |
| Scene mode placement, terrain, emitter/light authoring and save/reload | implemented / separate editor review | Actual editor UI, explicitly separate from player view | Authoring tools are not first-person gameplay. Needs their own honest UI workflow proof without calling it a game mechanic clip. |
| Individual organ injury / physiologically validated pressure model | not implemented / blocked model absent | No invented health display | Organ art is anatomical reference only, labeled ORGAN INJURY NOT MODELED; damage and acoustics are computational approximations. |

Machine-readable source pointers, sound paths and observation status: `artifacts/astra-audit-20260905/first-person-mechanic-coverage.json`.

Incoming impact cases are controlled isolated-turret contacts using the common terminal resolver and the catalog range law. They report both selected and unfiltered contacts. Expected nonpenetration/penetration wording identifies comparison intent, not a forced outcome. Full external flight, every station/side/hatch combination and historical experimental validation remain separate work.

No proper full-interior 3D X-ray has been approved. The retained eleven-region anatomy and module state is useful telemetry; it does not establish organ injury, and the rejected circle overlay remains excluded from first-person films.

## Additional input audit and scenarios

The latest machine-readable ledger is authoritative for scenario status. Source checks found that
solo main-gun fire is a fresh `mg_coax` (LMB) press; `fire` means Space. The gunner and manual-reload
JSONs now use the actual trigger. Earlier films made with Space remain failed evidence. Held
`coax` is correct at the Gunner. Solo bow MG is available from Commander with head out; the
Radioman firing route currently exists only in LAN input.

New scripts use ordinary inputs and read-only `Game::mechanic_status()` values. Phase assertions
sample the existing one-second telemetry: a repair must be blocked before the repair interval and
functional afterward, and sustained MG fire must actually lock from overheating and cool again.
None of these additions is a visual approval or proof that the scripted interaction succeeded.

| Scenario | Actual operation and scope |
|---|---|
| `fp_bow_mg_heat` | Head-out Commander fires, overheats, cools and refires the bow MG. |
| `fp_coax_stoppage` | Gunner trigger is blocked, held repair clears the stoppage, firing resumes. |
| `fp_traverse_jam` | Powered lay is blocked, held repair clears the jam, traverse resumes. |
| `fp_radio_repair` | Inspect Radio seat, then the current Commander-owned held repair restores its module flag. |
| `fp_engine_failure` | Engine damage blocks motion; repair input cannot restore a destroyed engine. |
| `fp_main_gun_failure` | Destroyed cannon rejects both real trigger edges and repair. |
| `fp_transmission_blocked` | Broken transmission immobilizes; missing driver-exposure input keeps repair unavailable. |
| `fp_round_drop` | Fire, move to Loader, equip a real rack round and release it to physical basket-floor motion. |
| `fp_interior_walk` | Ordinary Loader walking/head-look keeps the tank stationary and exposes actual compartment constraints. |
| `fp_mixed_combat_candidates` | Four German/four Soviet actors must exchange damaging shots using explicitly loaded candidates. |
| `fp_soft_target_tank_candidates` | Correct KV-2 rig must produce real MG damage and native OF-530 launch/contact/damage. |

The driver/loader roof lids now use the ordinary `TankInput.hatch_toggle` request (H or controller
left-stick-click + A), with separate per-lid edges and cooperative post ownership. The two new
roof-hatch scripts hold and release that real control, check the other two lids stay shut, and verify
opening a roof does not imply an exposed driver. Ten focused hatch tests and 23 LAN tests pass;
the new films remain pending. The seated Driver camera remains fixed to its visor, so that clip
cannot yet visually establish roof motion. A measured lean route and the roof-periscope attachment
are being checked separately; no transmission-repair bypass was introduced. Radio tuning/transmitting/Enigma operation
are not implemented. The static physical props and radio module repair flag do not establish those
mechanics. Powered gun lay animates handwheels, but independent manual hand-crank control and
hand contact are also absent. Loose-round recovery and physical AP/HE chamber exchange still need
accepted interaction films. The main-script pencil/pin adapter is being integrated by root; its
shared input test and fresh video receipt must replace the earlier window-input blocker.
