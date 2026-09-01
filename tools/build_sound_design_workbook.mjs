#!/usr/bin/env node

import fs from "node:fs/promises";
import path from "node:path";

// Codex supplies this package through its document runtime. Other maintainers can install the
// package normally or point this script at its ESM entry without baking one machine's home path
// into source control.
const artifactToolSpecifier = process.env.KESSEL_ARTIFACT_TOOL_MJS ?? "@oai/artifact-tool";
const { SpreadsheetFile, Workbook } = await import(artifactToolSpecifier);

const root = path.resolve(process.argv[2] ?? "artifacts/audio/game-audition-2026-08-31");
const outputPath = path.resolve(process.argv[3] ?? path.join(root, "Kessel_sound_design_cue_workbook.xlsx"));
const previewDir = path.join(root, "workbook-previews");
const csvPath = path.join(root, "AUDITION_CUES.csv");
const reportPath = path.join(root, "research", "report-source.md");

const DARK = "#0B0E12";
const CHARCOAL = "#141A21";
const STEEL = "#24303B";
const ORANGE = "#E06D2F";
const AMBER = "#F2A65A";
const ICE = "#8CB9D6";
const IVORY = "#F1EBDD";
const MUTED = "#A7ADB5";
const GRID = "#38434E";
const RED = "#B9473F";
const GREEN = "#4E8666";
const YELLOW = "#B89540";

function rowsFromText(text) {
  return text
    .trim()
    .split("\n")
    .map((line) => line.split("|").map((cell) => cell.trim()));
}

function cueStatus(name) {
  if (name === "live_mg_gravel_right" || name === "fragment_contact_brass_case") {
    return "DSP ONLY / NOT WIRED";
  }
  if (name.startsWith("live_")) return "OFFLINE DEMO COMPOSITE";
  if (name.startsWith("runtime_body_")) return "LIVE PARAMETER VARIANT";
  if (name.startsWith("runtime_")) return "LIVE IDENTITY";
  if (name.startsWith("destruction_dry_")) return "LIVE DRY SOURCE";
  return "LIVE PARAMETER VARIANT";
}

function cueReferences(name, category) {
  if (/near_miss|shell_shriek/.test(name)) return "A01–A03";
  if (/impact|clang|penetration|fragment|spall|body_impact|concussion/.test(name)) return "A21–A28";
  if (/gun|rifle|luger|mg_/.test(name)) return "A04–A10";
  if (/kill|turret_pop|turret_rip|cook_off|catastrophic/.test(name)) return "A11–A12";
  if (/engine|traverse/.test(name) || category === "engine" || category === "turret") return "A13–A20";
  if (/breech|casing|shell_|brass|hammer|pickup/.test(name)) return "A13, A21–A28";
  return "Evidence gap — controlled recording recommended";
}

const implementedRows = rowsFromText(`
player_gun_ext_1..4 | Live identity | One-shot family | Own 88 mm fired with hatch open; allied Tiger variants | Procedural exterior gun punch, four deterministic identities | Own shot centered; allied fire is delayed/world-spatial | Useful transient but not a measured KwK 36; four timbral variants need one physical family | P0 | 1–4 | A04–A07
player_gun_int_1..2 | Live identity | One-shot family | Own 88 mm fired buttoned | Interior gun punch plus steel-cabin reverb | Local/centered | Needs authentic pressure/mechanism separation and controlled cabin transfer | P0 | 5–6 | A04
enemy_gun | Live identity / placeholder reuse | One-shot | Enemy close report; ambient boom; Panzerfaust launch placeholder | Fixed 88 mm/280 m procedural report | World-spatial except Panzerfaust placeholder | Panzerfaust must not reuse a tank gun | P0 | 7 | A04–A10
impact_ground | Live identity / overused | One-shot | Shell, MG, tree, timber, wall and Panzerfaust environment impacts | One procedural ground impact | World-spatial | Many materials collapse into one identity | P0 | 8 | A08–A10
impact_tank | Live identity | One-shot | External vehicle penetration/contact and steel collision | Fixed 76 mm parametric hull strike | World-spatial | Must be split by terminal result, panel and energy | P0 | 9 | A21–A28
armor_clang_1..2 | Live identity / wrong reuse | One-shot family | Ricochet; every fourth MG armor hit; empty Panzerfaust | Fixed 76.2 mm structural impacts | World-spatial | MG incorrectly triggers cannon-scale clang; modal character is too bell-like | P0 | 10–11 | A21–A28
penetration_hit | Live identity | One-shot | External penetration; Panzerfaust penetration; debug fatal cue | Fixed 85 mm APHE penetration | World-spatial | Onboard Tiger uses the better parametric body; generic identity remains broad | P0 | 12 | A21–A28
near_miss | Live identity | One-shot | Enemy cannon closest approach | Filtered noise plus explicit 2.9 kHz to 230 Hz sine chirp | At closest point; world-spatial | Long whistle conflicts with measured brief ballistic-wave evidence | P0 | 13 | A01–A03
kill_explosion | Live identity / placeholder reuse | Destruction | Ordinary tank destruction, HE blast, collapse, ambient boom | Layered procedural kill arrangement | World-spatial | Building collapse reuse is wrong; needs physical fuel/ammo/steel layers | P0 | 14 | A11–A12
turret_pop | Live identity | Destruction | Catastrophic ammunition detonation | Layered destruction arrangement | World-spatial | Needs energy/mass-driven air and structure components | P1 | 15 | A11–A12
turret_rip | Live identity | Destruction | Turret-ring failure | Mechanical destruction arrangement | World-spatial | Needs physically timed airborne movement/landing companion cues | P1 | 16 | A11–A12
cook_off | Live identity | Destruction | Delayed wreck ammo detonation and allied Tiger loss | Procedural cook-off arrangement | World-spatial | Sustained fire and ammunition progression are still silent | P0 | 17 | A11–A12
shell_shriek | Live identity | Moving one-shot | Outgoing player shell | Same flyby synth as near_miss, playback swept 1.85 to 0.55 | Dedicated moving sink | Not based on listener closest approach; only one can exist | P0 | 18 | A01–A03
engine_death | Live identity / wrong reuse | One-shot | Module/tank death and ordinary ignition off | Procedural engine failure | Local/centered | Healthy shutdown must be a different sequence | P0 | 19 | A13–A20
breech_clack | Live identity / placeholder | Foley | Hatch lever and fresh Panzerfaust tube | Five decaying sine modes at 330 Hz base | Local | Melodic legacy synth; main breech correctly uses the mechanism family instead | P0 | 20 | A13
brass_clatter | Live identity / placeholder | Foley | Loose 88 mm shell, track link, loose metal props | Five decaying sine modes at 510 Hz base | Mixed local/world | Tonal and generic; must be mass/material/impulse driven | P0 | 21 | A13, A21–A28
hammer_strike | Live identity / placeholder | Foley | Armor patch and track-pin hammer | Five decaying sine modes at 178 Hz base | Local | Tuned modal rack reads as an instrument | P1 | 22 | A13
pickup | Live identity / placeholder | Interaction | Refuel, cans, shelf, reload, supplies, corpse ammo | Five decaying sine modes at 690 Hz base | Local | One tonal cue represents unrelated objects and gestures | P0 | 23 | Evidence gap
luger_shot | Live identity | Weapon | Player P08 fires | Procedural small weapon | Local/centered | Needs full mechanical family and owned firearm reference | P1 | 24 | A04–A10
alarm_shout | Live identity | Crew | Soviet reaches full detection | Non-verbal procedural two-part shout | Delayed/world-spatial | Needs restrained human variation without spoken exposition | P1 | 25 | Evidence gap
rifle_shot | Live identity / placeholder reuse | Weapon | Soviet rifle and enemy-tank coax placeholder | Procedural small weapon | Delayed/world-spatial | Tank MG cannot reuse one rifle report | P0 | 26 | A04–A10
engine_loop | Live identity | Continuous | Player Maybach running | Procedural V12 combustion/exhaust/block/gear/lash bed | One centered voice; RPM pitch and load gain | Missing independent tracks, fan, transmission and interior/exterior perspectives | P0 | 27 | A13–A20
ambience_interior | Live identity | Continuous | Buttoned Tiger | Procedural sealed air/fan turbulence | Local loop with crossfade | Generic and not tied to fans, fixtures, switches or damage | P1 | 28 | A04, A24
ambience_exterior | Live identity | Continuous | Hatch open or on foot | Procedural wind | Local loop with crossfade | Not act, shelter, or weather-zone specific | P1 | 29 | Evidence gap
mg_loop_interior | Live identity | Continuous weapon | Coax/hull MG buttoned | Procedural 13 rps loop | Shared centered voice | Coax and bow MG share one voice; impacts trigger wrong cannon clang | P0 | 30 | A04–A10
mg_loop_exterior | Live identity | Continuous weapon | Coax/hull MG exposed | Procedural 13 rps loop | Local | Needs receiver, feed, muzzle and environment layers | P0 | 31 | A04–A10
turret_traverse_loop | Live identity | Continuous machinery | Player turret rotates with engine on | Procedural PTO hydraulic pump/oil/gears | Rate, hydraulic load and engine state control playback/gain | Needs elevation, stops, brake and source-position perspectives | P0 | 32 | A13
enemy_gun_report_dynamic | Live parameter family | Parametric one-shot | Every enemy main-gun discharge | Actual caliber and current range | Flash-to-bang delay, absorption, bearing, LOS and reverb | Close shots stack the fixed enemy_gun layer | P0 | 33–36 | A01–A07
damage_impact_body | Live parameter family | Parametric one-shot | Player armor hit while onboard | Shell class/mass/velocity, plate, angle, residual, behind-armor energy, panel and listener data | Local structural body plus cabin returns | Strongest current subsystem; still needs approved physical source language | P0 | 37–46 | A21–A28
concussion_trauma | Live parameter family | Trauma bus | Medium+ hit or close large-caliber bounce | Decorrelated low-passed pressure/fatigue | World ducking recovers over 3.1–8.48 s | Needs user intensity/accessibility control and measured safe monitoring | P0 | 47–48 | A21–A28
fragment_whizz | Live parameter family | Moving one-shot | Hot cube passes within 0.62 m | Filtered moving source from exact swept path | ITD/head shadow; one per penetration | Physically driven but only the shared primary listener hears its own field | P0 | 49–50 | A21–A28
fragment_contacts | Live family / one unwired variant | Collision family | Hot cube hits wall, breech or floor | Short energy/material contacts | Actual bearing/distance/speed; max two per penetration | Brass-case DSP variant is not wired; contact budget may be too sparse | P0 | 51–54 | A21–A28
main_gun_recoil | Live parameter family | Mechanism | Own main-gun discharge | Authored recoil mechanical cue | Local bearing 4 degrees | Needs pressure-independent hydraulic/metal timing | P1 | 55 | A04, A13
breech_open / breech_lock | Live parameter family | Mechanism | Manual detents and couch reload completion | Moving-stereo mechanical foley | Local | Needs isolated KwK 36 reference or controlled surrogate recording | P1 | 56–57 | A13
casing_extract / casing_drop | Live parameter family | Mechanism | Case extraction and first physical floor bounce | Moving-stereo mechanical foley | Local | Only first bounce sounds; basket/wall roll absent | P1 | 58–59 | A13
shell_lift / shell_ram / shell_drop | Live parameter family | Mechanism | Manual ammunition handling | Moving-stereo foley | Local | Needs mass/effort/rack/tray interaction layers | P1 | 60–62 | A13
body_impact | Live parameter family | Casualty foley | Casualty count after armor hit | Up to four delayed cloth-damped masses | Local | Predicted from count, not actual later ragdoll contacts | P0 | 63–66 | A21–A28
engine_state_variants | Live parameter family | Continuous/one-shot | Idle, gears, loads, shutdown/death demo | Engine model parameter sweeps | Local | Audition variants expose synthesis but do not add missing physical layers | P0 | 67–72 | A13–A20
turret_state_variants | Live parameter family | Continuous | Slow/fast interior/exterior traverse | PTO hydraulic parameter sweeps | Local | No source-position or per-station mix | P1 | 73–76 | A13
destruction_dry_sources | Live dry source | Destruction components | Dry source layers used inside live arrangements | Pressure, steel tear, flame/fuel components | Dry, not final live mix | Useful for redesign isolation | P1 | 77–79 | A11–A12
`);

const missingRows = rowsFromText(`
Independent left/right track-link cadence | P0 | Vehicle locomotion | Actual left/right track speed, slip and steering | Link strikes, return-run slap, side-specific hull coupling | Granulator III owned track loops + Operator impulses + Corpus Beam | Planned | Weight, motion, threat | A20
Sprocket and final-drive tooth rhythm | P0 | Vehicle locomotion | RPM, gear, track speed and torque | Tooth cadence, lash, final-drive strain | Operator fixed inharmonic pulse + Corpus | Planned | Mechanical inevitability | A20
Roadwheel bearing growl and idler impacts | P0 | Vehicle locomotion | Speed and terrain impulse | Broad bearing texture, intermittent idler knocks | Granulator III + Corpus Beam | Planned | Mass and fatigue | A20
Suspension compression and rebound | P0 | Vehicle locomotion | Per-wheel suspension impulse | Spring/damper groan, stop impact, hull response | Owned metal/rubber sources + Corpus | Planned | 57-ton inertia | A20
Hull fixtures and ammunition-rack rattle | P0 | Vehicle locomotion | Hull acceleration and impulse spectrum | Sparse loose-object contacts by location | Simpler/Granulator owned foley + Corpus | Planned | Claustrophobia, vulnerability | A04, A20, A24
Terrain contact layers | P0 | Vehicle locomotion | Material, speed, sink and slip | Snow, frozen earth, cobble, mud, rubble, timber | Owned field recordings + Granulator III | Planned | Place, traction, weather | A20
Pivot-steer scrub and braking load | P0 | Vehicle locomotion | Opposed track speeds and brake torque | Scrub, squeal, drivetrain groan | Granulator III + Corpus + restrained Roar | Planned | Strain and scale | A20
Starter motor and cold crank sequence | P0 | Engine | Temperature, battery, engine health | Starter, individual coughs, catch/bark | Operator cadence + owned mechanical sources | Planned | Anticipation, uncertainty | A13–A20
Warm restart and healthy shutdown | P0 | Engine | Temperature and operator input | Short restart; combustion coast-down and metal settle | Operator/Granulator envelopes | Planned | Machine state clarity | A13–A20
Gear engagement and shift transients | P0 | Drivetrain | Gear, clutch/load and RPM delta | Engagement knock, rev drop, downshift and lash | Operator transient + Corpus Plate/Beam | Planned | Effort and momentum | A20
Separate interior and exterior engine perspectives | P0 | Engine | Hatch/station and listener position | Block/transmission/fan inside; exhaust/mechanics outside | Parallel Rack chains, owned sources | Planned | Embodiment | A04, A13–A20
Cooling-fan layer | P0 | Engine | RPM, temperature and damage | Broad mechanical air plus bearing modes | Granulator III + filtered Operator noise | Planned | Constant oppressive bed | A24
Physical interior/wreck fire loop | P0 | Fire/destruction | Fuel, oxygen, heat, ammunition and enclosure | Base flame, fuel roar, crackles, steel ticks, smoke pressure | Granulator III owned fire + Operator/Roar | Planned | Horror, urgency | A11–A12
Fire-extinguisher family | P0 | Fire control | Valve, nozzle aim, pressure and remaining charge | Valve, agent jet, nozzle, knockdown, empty sputter | Granulator III/owned jet + noise shaping | Planned | Agency under panic | Evidence gap
Mach/CPA-driven incoming tank-round crack | P0 | Ballistics | Projectile Mach, path and listener closest approach | 5–10 ms N-wave/crack; separate report and terminal event | Operator impulse or native DSP, not tonal chirp | Planned | Adrenaline and direction | A01–A03
Material-specific terminal impacts | P0 | Ballistics/destruction | Material, energy, angle and terminal result | Snow, frozen soil, masonry, timber, steel, water | Owned impulses + Corpus/material chains | Planned | Readable combat geometry | A08–A10
Exit-hole and overpenetration blast | P0 | Ballistics | Residual energy and exit geometry | Rear-face rupture, air pulse, expelled fragments | Operator pressure + owned steel + Corpus | Planned | Penetration consequence | A21–A28
APHE internal filler burst | P0 | Ballistics | Fuse, filler, delay and compartment | Compact detonation, panel loading, fragments, short furnace tail | Operator/Roar/Corpus + owned blast | Planned | Terror and lethality | A21–A28
Panzerfaust full family | P0 | Infantry weapon | Launch, backblast, rocket flight, contact/dud | Tube mechanics, propellant, flight, jet, armor response | Operator + Granulator + owned recordings | Planned | Weapon identity | A08–A10
Shared master bus and export stems | P0 | Mix/runtime | Total bus energy and trauma priority | Headroom, safety limiting, diegetic/score/trauma stems | Ableton Rack + game-side final limiter | Planned | Intensity without pain | L06, L10–L11
Per-seat headphone/listener mixes | P0 | Spatial audio | Four crew seats/cameras and device routing | Seat-relative bearing, occlusion and local mechanisms | Game runtime; not a Live export | Planned | Multicrew embodiment | A04
Station-switch boots, cloth and leather | P1 | Crew | Locomotion, posture and station geometry | Foot contact, uniform/equipment movement | Owned foley + Simpler/Granulator | Planned | Humanity, cramped labor | Evidence gap
Binocular physical-object family | P1 | Crew interaction | Case, hinge, grip and animation phase | Case, hinge, hand, raise, eye contact, lower | Owned foley Rack | Planned | Tactility, observation | Evidence gap
Hatch/cupola/visor mechanisms | P1 | Crew interaction | Latch state, angle and velocity | Latch, hinge weight, seal, cupola/periscope controls | Owned metal foley + Corpus | Planned | Armor boundary | A13
Gun elevation handwheel and gearing | P1 | Weapon mechanism | Elevation rate/load | Handwheel, gear train, stops and brake | Operator impulses + Corpus | Planned | Manual labor | A13
Turret brake, lock and limit contacts | P1 | Weapon mechanism | Traverse state and end stop | Brake drag, lock detent, structural stop | Owned foley + Corpus | Planned | Machine constraint | A13
Trigger, recoil cylinder and recuperator settle | P1 | Weapon mechanism | Fire and recoil animation phases | Trigger train, hydraulic return, settling metal | Owned foley + Corpus/Roar | Planned | Fire sequence credibility | A04, A13
Rack latch, shell scrape and tray contact | P1 | Ammunition handling | Exact reload animation phase | Latch, shell mass scrape, tray and rammer | Owned foley + Corpus | Planned | Labor and danger | A13
Casing roll and later basket/wall bounces | P1 | Ammunition handling | Physical casing collision telemetry | Energy/material-specific later contacts | Runtime collision events + owned one-shots | Planned | Persistent cabin chaos | A13
MG charge/feed/jam/recovery | P1 | Weapon mechanism | Weapon state and temperature | Charging handle, feed, receiver, stoppage | Operator cadence + owned foley | Planned | Panic and failure | A04–A10
Ragdoll limb/head/torso collisions | P1 | Crew casualty | Real collision impulse, body part and surface | Cloth-damped masses and occasional metal contact | Runtime collision telemetry + owned foley | Planned | Human consequence | A21–A28
Crew breathing, effort and flinch | P1 | Crew | Exertion, fear, injury and pressure events | Restrained nonverbal breath/effort/pain/shock | Owned performances, no dialogue | Planned | Humanity and panic | A24–A28
Fuel cap, can slosh, pour and vent | P1 | Logistics | Fill mass, pose, flow and cap state | Cap, slosh, glug, vent, empty/full throws | Owned foley + granular variations | Planned | Physical survival loop | Evidence gap
Radio, Morse and Enigma machinery | P1 | Crew/electrical | Device power, key/rotor/lamps | Receiver bed, key, rotors, lamp relays | Operator/noise + owned mechanisms | Planned | Mystery and late-game communication | Evidence gap
Material footsteps and posture | P1 | On foot | Surface, gait, stance and side | Snow/rubble/wood/concrete, jump, land, stumble | Owned foley + randomized Rack | Planned | Vulnerability outside armor | Evidence gap
Cold breathing and exertion recovery | P1 | On foot | Temperature, pace and injury | Breath loop, gasps, recovery | Owned performances | Planned | Exposure and fear | Evidence gap
Full P08 mechanical family | P1 | On foot weapon | Fire, slide, casing, magazine, round and dry state | Shot layers, action, reload and empty click | Operator transient + owned foley | Planned | Weapon tactility | A04–A10
Full Kar98 mechanical family | P1 | On foot weapon | Bolt, clip, rounds, fire and dry state | Bolt, stripper clip, handling and report | Operator transient + owned foley | Planned | Enemy/ally readability | A04–A10
Soviet infantry movement and suppression | P1 | Enemy infantry | AI locomotion, aim, fire, reload and suppression | Footsteps, cloth, rifle, cracks and panic | Owned foley/performances | Planned | Threat and humanity | A01–A03, A04–A10
Physical item pickup/set/throw/bounce | P1 | Interaction | Mass, material, grip and collision | Object-specific contact and movement | Runtime physics + owned foley | Planned | World tactility | Evidence gap
Map, pencil and physical pins | P1 | Navigation | Map overlay, mark and pin state | Paper, pencil, pin placement/removal | Owned close foley | Planned | Being lost without UI abstraction | Evidence gap
Doors, bunkers, crates and rubble search | P1 | World interaction | Hinge/latch/object state | Weight/material-specific mechanisms | Owned location foley + Corpus | Planned | Exploration tension | Evidence gap
Telephone-pole and small-tree destruction | P1 | Destruction | Contact impulse and fracture state | Bend, fiber crack, branches, cable whip, trunk thud | Owned field foley + physics events | Planned | Tiger mass | Evidence gap
Progressive masonry failure | P1 | Destruction | Stress, cracks and collapse | Chips, groan, collapse, rubble grinding | Owned demolition foley + Corpus/Roar | Planned | Destructive scale | A11–A12
Loose-object material collisions | P1 | Interior/world | Mass, material, velocity and enclosure | Steel, wood, glass, ceramic, toolbox, furniture | Owned foley libraries | Planned | Detail and chaos | Evidence gap
Tank-on-tank scrape and wreck traversal | P1 | Vehicle collision | Hull/track contact and load | Plate scrape, track climb, debris crush | Granulator III + Corpus | Planned | 57-ton contact | A20
Turret airborne motion and landing | P1 | Catastrophic destruction | Ballistic body trajectory and impact | Air movement, ring tear, ground landing | Runtime timing + owned sources | Planned | Catastrophic scale | A11–A12
Blizzard and shelter transitions | P1 | Environment | Wind field, snowfall, hull orientation and shelter | Broad wind, gusts, snow hiss on armor/viewports | Owned field recordings + Granulator III | Planned | Isolation and cold | Evidence gap
Act-specific exterior beds | P1 | Environment | Act, biome, time, battle density | City, forest, river, thaw, distant battle layers | Owned ambiences + Granulator III | Planned | Place and narrative progression | Evidence gap
Interior fan/radio/electrical beds | P1 | Interior ambience | Switches, RPM, damage and load | Fan bearings, transformers, receiver/electrical texture | Operator + Granulator III | Planned | Living machine | A24
Sparse diegetic UI feedback | P2 | UI | Join, takeover, map, inventory and failure | Mechanical clicks and material gestures, no arcade chimes | Operator/owned foley | Planned | Clarity without breaking tone | Evidence gap
`);

const abletonChains = rowsFromText(`
1 | ENGINE PRESSURE | Operator | Mono fixed/inharmonic combustion cadence and sub-pressure body; avoid keyboard pitch | RPM, Load, Gear, Seed | Dry pressure and body stem
2 | ENGINE MATERIAL | Granulator III Loop | Owned Maybach/exhaust/track recordings crossfaded by RPM and load | RPM, Load, Speed, Interior/Exterior | RPM × load × perspective loop grid
3 | DRIVETRAIN / PTO | Operator + Granulator III + Corpus | Friction, tooth cadence, hydraulic pump and hull coupling | Speed, Gear, Traverse Rate, Hydraulic Load | Drivetrain and turret stems
4 | IMPACT CONTACT | Operator impulse + owned one-shots | Very short non-tonal strike excitation; velocity follows energy | Impact Energy, Panel, Bearing, Seed | Dry contact transients
5 | ARMOR BODY | Corpus Plate/Beam + restrained Roar | Short panel modes and energy-dependent iron pressure | Panel Size/Tune, Impact Energy, Interior/Exterior | Panel/body stems
6 | PENETRATION / SPALL | Granulator III Cloud + sparse one-shots | Fragment whizzes and independent material contacts; no white-noise wash | Residual Energy, Bearing, Station Distance, Seed | Dry fragment/contact assets
7 | CATASTROPHIC | Operator + owned blast + Roar | Low pressure, steel failure, flame/fuel and long tails in separate chains | Damage Severity, Impact Energy, Seed | Independent pressure/steel/fire/tail stems
8 | SCORE RESONANCE | Meld + Granulator III Cloud | Inharmonic score grows from the same engine and plate source corpus | Score Tension, RPM, Damage Severity, Environment | Calm/tension/collapse score stems
9 | PERSPECTIVE / SPACE | Parallel interior/exterior effect chains | EQ/dynamics/IR for authoring preview; keep shipping assets dry enough for runtime acoustics | Interior/Exterior, Occlusion, Station Distance | Near-field dry and perspective references
`);

const macroRows = rowsFromText(`
1 | RPM | Continuous | EngineTelemetry.rpm_norm | Combustion cadence, granular position and drivetrain rates
2 | Load | Continuous | EngineTelemetry.load | Pressure, distortion and mechanical strain
3 | Speed | Continuous | EngineTelemetry.speed_norm | Track cadence and aerodynamic/terrain texture
4 | Gear | Stepped | EngineTelemetry.gear | Ratio, tooth/lash family and shift variation
5 | Traverse Rate | Continuous | Actual turret angular speed | PTO/hydraulic cadence
6 | Hydraulic Load | Continuous | Turret hydraulic load | Pump pressure and mechanical strain
7 | Impact Energy | Continuous | Shell/plate event energy | Contact velocity, panel excitation and saturation
8 | Penetration / Residual | Continuous | Residual and behind-armor energy | Exit, spall and internal burst balance
9 | Panel Size / Tune | Continuous/stepped | Hit panel and effective armor | Corpus resonator dimensions/modes
10 | Bearing | Bipolar | Listener-relative bearing | Authoring pan preview; runtime remains authoritative
11 | Station Distance | Continuous | Crew-seat distance | Direct/body/contact balance
12 | Interior / Exterior | Crossfade | Hatch/listener state | Perspective chain selector
13 | Damage Severity | Continuous/stepped | Light/medium/heavy/catastrophic | Layer density and tail duration
14 | Score Tension | Continuous | Director state | Meld/granular score emergence
15 | Environment / Occlusion | Continuous/stepped | Act/zone/LOS | Preview filtering and environment family
16 | Seed / Variation | Stepped | Deterministic event seed | Repeatable timbral variation
`);

function parseReferenceTables(markdown) {
  const references = [];
  for (const line of markdown.split("\n")) {
    if (!/^\| [AL]\d\d \|/.test(line)) continue;
    const cells = line.split("|").slice(1, -1).map((cell) => cell.trim());
    if (cells[0].startsWith("A")) {
      const [id, url, publisher, supports, rights, confidence] = cells;
      references.push([id, "Armored acoustics", url, publisher, supports, rights, confidence]);
    } else {
      const [id, url, publisher, supports, confidence] = cells;
      references.push([id, "Ableton / Max", url, publisher, supports, "Official documentation; cite and follow license terms", confidence]);
    }
  }
  return references;
}

function baseSheet(sheet) {
  sheet.showGridLines = false;
}

function titleBand(sheet, range, title, subtitle) {
  sheet.getRange(range).merge();
  const anchor = sheet.getRange(range.split(":")[0]);
  anchor.values = [[`${title}\n${subtitle}`]];
  anchor.format = {
    fill: DARK,
    font: { name: "Aptos Display", size: 18, bold: true, color: IVORY },
    wrapText: true,
    verticalAlignment: "center",
    horizontalAlignment: "left",
    borders: { bottom: { style: "thick", color: ORANGE } },
  };
  sheet.getRange(range).format.rowHeight = 54;
}

function styleHeader(range) {
  range.format = {
    fill: STEEL,
    font: { name: "Aptos", size: 10, bold: true, color: IVORY },
    wrapText: true,
    verticalAlignment: "center",
    borders: { bottom: { style: "medium", color: ORANGE } },
  };
  range.format.rowHeight = 32;
}

function styleBody(range) {
  range.format = {
    fill: "#F8FAFC",
    font: { name: "Aptos", size: 9, color: "#1A1D22" },
    wrapText: true,
    verticalAlignment: "top",
    borders: { insideHorizontal: { style: "thin", color: "#D7DCE1" } },
  };
}

function setWidths(sheet, widths) {
  widths.forEach((width, index) => {
    sheet.getRangeByIndexes(0, index, 1, 1).format.columnWidth = width;
  });
}

function addPriorityFormatting(range) {
  range.conditionalFormats.add("containsText", { text: "P0", format: { fill: "#F4D7D4", font: { bold: true, color: RED } } });
  range.conditionalFormats.add("containsText", { text: "P1", format: { fill: "#FAEBC9", font: { bold: true, color: "#876313" } } });
  range.conditionalFormats.add("containsText", { text: "P2", format: { fill: "#DDEBF4", font: { bold: true, color: "#285C7A" } } });
}

function addStatusFormatting(range) {
  range.conditionalFormats.add("containsText", { text: "NOT WIRED", format: { fill: "#F4D7D4", font: { bold: true, color: RED } } });
  range.conditionalFormats.add("containsText", { text: "PLACEHOLDER", format: { fill: "#FAEBC9", font: { bold: true, color: "#876313" } } });
  range.conditionalFormats.add("containsText", { text: "LIVE", format: { fill: "#DCEBDF", font: { color: "#315D43" } } });
}

const csvText = await fs.readFile(csvPath, "utf8");
const imported = await Workbook.fromCSV(csvText, { sheetName: "Audition Source" });
const csvValues = imported.worksheets.getItemAt(0).getUsedRange().values;
const report = await fs.readFile(reportPath, "utf8");
const referenceRows = parseReferenceTables(report);
if (csvValues.length !== 80) throw new Error(`expected 79 cues plus header, got ${csvValues.length}`);
if (referenceRows.length !== 51) throw new Error(`expected 51 research references, got ${referenceRows.length}`);

const workbook = Workbook.create();
const overview = workbook.worksheets.add("Overview");
const audition = workbook.worksheets.add("Audition Cues");
const implemented = workbook.worksheets.add("Implemented Runtime");
const missing = workbook.worksheets.add("Missing Backlog");
const refs = workbook.worksheets.add("Reference Library");
const ableton = workbook.worksheets.add("Ableton Architecture");

// Overview
titleBand(overview, "A1:J2", "KESSEL // SOUND DESIGN COMMAND SHEET", "One 57-ton machine • implementation truth • redesign queue • evidence • Ableton authoring map");
overview.getRange("A4:J4").values = [["AUDITION CUES", null, "LIVE IDENTITIES", null, "MISSING CUES", null, "P0 BACKLOG", null, "EVIDENCE GAPS", null]];
overview.getRange("A4:J4").format = { fill: CHARCOAL, font: { bold: true, color: MUTED, size: 9 }, horizontalAlignment: "center" };
overview.getRange("A5:B6").merge(); overview.getRange("A5").formulas = [["=COUNTA('Audition Cues'!F4:F82)"]];
overview.getRange("C5:D6").merge(); overview.getRange("C5").formulas = [["=COUNTIF('Audition Cues'!K4:K82,\"LIVE IDENTITY\")"]];
overview.getRange("E5:F6").merge(); overview.getRange("E5").formulas = [["=COUNTA('Missing Backlog'!A4:A54)"]];
overview.getRange("G5:H6").merge(); overview.getRange("G5").formulas = [["=COUNTIF('Missing Backlog'!B4:B54,\"P0\")"]];
overview.getRange("I5:J6").merge(); overview.getRange("I5").formulas = [["=COUNTIF('Missing Backlog'!I4:I54,\"Evidence gap\")"]];
overview.getRange("A5:J6").format = { fill: "#EEF1F3", font: { name: "Aptos Display", size: 24, bold: true, color: ORANGE }, horizontalAlignment: "center", verticalAlignment: "center", borders: { outside: { style: "medium", color: GRID } } };
overview.getRange("A8:J8").merge(); overview.getRange("A8").values = [["NORTH STAR // THE PLAYER SHOULD HEAR ONE 57-TON MACHINE, NOT A COLLECTION OF SYNTH PATCHES"]];
overview.getRange("A8:J8").format = { fill: ORANGE, font: { bold: true, color: DARK, size: 11 }, horizontalAlignment: "center", verticalAlignment: "center" };
overview.getRange("A10:E10").merge(); overview.getRange("A10").values = [["PHYSICAL FINDINGS"]];
overview.getRange("F10:J10").merge(); overview.getRange("F10").values = [["EMOTIONAL MIX PRINCIPLES"]];
overview.getRange("A10:J10").format = { fill: STEEL, font: { bold: true, color: IVORY }, borders: { bottom: { style: "medium", color: ORANGE } } };
overview.getRange("A11:E16").merge(); overview.getRange("A11").values = [["• Near-pass: a 5–10 ms directional ballistic crack/N-wave, then separate muzzle report or impact.\n• Armor strike: short panel clang/knock plus low hull jolt; penetration flash, blast and spall are separate layers.\n• Spall: sparse irregular whizzes and contacts driven by actual fragment paths—not claps or a noise wash.\n• Tracked motion: combustion + fan + final drive + sprocket/track + wheels + suspension + hull/fixtures.\n• Critical gap: no trustworthy occupied-target interior impact recording was found; commission a controlled armored-hull session."]];
overview.getRange("F11:J16").merge(); overview.getRange("F11").values = [["• ADRENALINE: causally precise transients, direction, pre-impact warning and short recovery windows.\n• FEAR: low-frequency structure, uncertain distant threats, failing mechanisms and constrained headroom—not painful treble.\n• HUMANITY: breathing, labor, cloth, hands, injury and bodies interacting with the cabin.\n• SADNESS: restraint, aftermath, persistent damaged-machine beds and environmental silence.\n• INTENSITY SAFETY: one shared master bus, predictable peaks, trauma-bus controls and an accessibility intensity setting."]];
overview.getRange("A11:J16").format = { fill: "#F5F2EC", font: { color: "#252A30", size: 10 }, wrapText: true, verticalAlignment: "top", borders: { outside: { style: "thin", color: GRID } } };
overview.getRange("A11:J16").format.rowHeight = 28;
overview.getRange("A18:C22").values = [["Priority", "Backlog count", "Meaning"], ["P0", null, "Changes the 57-ton-machine feel or corrects a misleading cue"], ["P1", null, "Humanity, interaction, destruction and world detail"], ["P2", null, "Polish and sparse diegetic interface"], ["TOTAL", null, "Editable backlog rows"]];
overview.getRange("B19").formulas = [["=COUNTIF('Missing Backlog'!B4:B54,A19)"]]; overview.getRange("B19:B21").fillDown();
overview.getRange("B22").formulas = [["=SUM(B19:B21)"]];
styleHeader(overview.getRange("A18:C18")); styleBody(overview.getRange("A19:C22"));
addPriorityFormatting(overview.getRange("A19:A21"));
const priorityChart = overview.charts.add("bar", overview.getRange("A18:B21"));
priorityChart.title = "Missing sound work by priority";
priorityChart.hasLegend = false;
priorityChart.xAxis = { axisType: "textAxis", textStyle: { fontSize: 9 } };
priorityChart.yAxis = { numberFormatCode: "0" };
if (priorityChart.series.items.length) priorityChart.series.items[0].fill = ORANGE;
priorityChart.setPosition("E18", "J31");
overview.getRange("A24:C31").merge(); overview.getRange("A24").values = [["ABLETON DECISION\n\nAuthor in Live 12 Suite with Operator, Granulator III, Corpus, Roar and Meld. Prototype telemetry with a frozen Max for Live UDP device. Export approved dry/near-field assets and stems; the Rust runtime remains authoritative for game state, delay, bearing, occlusion and final safety limiting. Native Live devices are not an SDK that can be embedded in the game."]];
overview.getRange("A24:C31").format = { fill: CHARCOAL, font: { color: IVORY, size: 10 }, wrapText: true, verticalAlignment: "top", borders: { left: { style: "thick", color: ICE } } };
overview.getRange("A24:C31").format.rowHeight = 24;
setWidths(overview, [14, 12, 22, 4, 14, 12, 14, 12, 14, 12]);
overview.freezePanes.freezeRows(2);

// Audition cues
titleBand(audition, "A1:P2", "LABELED AUDITION // 79 CUES", "Each cue is independently normalized to −6 dBFS for review; this is not the in-game relative mix");
const auditionHeaders = [...csvValues[0], "status", "primary_reference_ids", "review_decision", "review_priority", "review_notes", "redo_flag"];
const auditionData = csvValues.slice(1).map((row) => {
  const name = String(row[5]);
  return [...row, cueStatus(name), cueReferences(name, String(row[4])), "Unreviewed", "", "", null];
});
audition.getRangeByIndexes(2, 0, 1, auditionHeaders.length).values = [auditionHeaders];
audition.getRangeByIndexes(3, 0, auditionData.length, auditionHeaders.length).values = auditionData;
audition.getRange("P4").formulas = [["=IF(OR(M4=\"Revise\",M4=\"Replace\",M4=\"Remove\"),\"REDO\",\"\")"]];
audition.getRange("P4:P82").fillDown();
styleHeader(audition.getRange("A3:P3")); styleBody(audition.getRange("A4:P82"));
audition.getRange("B4:E82").format.numberFormat = "0.000";
audition.getRange("H4:J82").format.numberFormat = "0.000000";
audition.getRange("M4:M82").dataValidation = { rule: { type: "list", values: ["Unreviewed", "Keep", "Revise", "Replace", "Remove", "Needs Context"] } };
audition.getRange("N4:N82").dataValidation = { rule: { type: "list", values: ["", "P0", "P1", "P2"] } };
addStatusFormatting(audition.getRange("K4:K82")); addPriorityFormatting(audition.getRange("N4:N82"));
audition.getRange("P4:P82").conditionalFormats.add("containsText", { text: "REDO", format: { fill: "#F4D7D4", font: { bold: true, color: RED } } });
audition.tables.add("A3:P82", true, "AuditionCueTable");
audition.getRange("A4:P82").format.fill = "#F8FAFC";
audition.getRange("A4:P82").conditionalFormats.addCustom("=MOD(ROW(),2)=0", { fill: "#E7F0F5" });
audition.freezePanes.freezeRows(3); audition.freezePanes.freezeColumns(2);
setWidths(audition, [7, 11, 11, 10, 15, 34, 38, 12, 12, 12, 26, 18, 17, 12, 40, 10]);

// Implemented runtime
titleBand(implemented, "A1:J2", "IMPLEMENTED RUNTIME SOUND", "What the game actually triggers today—not planned systems or documentation claims");
const implementedHeaders = ["cue_or_family", "runtime_status", "type", "live_trigger", "current_construction", "routing_or_spatialization", "known_issue", "redesign_priority", "audition_cues", "reference_ids"];
implemented.getRange("A3:J3").values = [implementedHeaders];
implemented.getRangeByIndexes(3, 0, implementedRows.length, implementedHeaders.length).values = implementedRows;
styleHeader(implemented.getRange("A3:J3")); styleBody(implemented.getRangeByIndexes(3, 0, implementedRows.length, 10));
implemented.tables.add(`A3:J${implementedRows.length + 3}`, true, "ImplementedRuntimeTable");
implemented.getRange(`A4:J${implementedRows.length + 3}`).format.fill = "#F8FAFC";
implemented.getRange(`A4:J${implementedRows.length + 3}`).conditionalFormats.addCustom("=MOD(ROW(),2)=0", { fill: "#E7F0F5" });
implemented.getRange(`H4:H${implementedRows.length + 3}`).dataValidation = { rule: { type: "list", values: ["P0", "P1", "P2"] } };
addPriorityFormatting(implemented.getRange(`H4:H${implementedRows.length + 3}`)); addStatusFormatting(implemented.getRange(`B4:B${implementedRows.length + 3}`));
implemented.freezePanes.freezeRows(3); implemented.freezePanes.freezeColumns(2);
setWidths(implemented, [28, 24, 18, 42, 48, 40, 48, 12, 13, 18]);

// Missing backlog
titleBand(missing, "A1:I2", "MISSING SOUND BACKLOG", "Editable production queue: real trigger inputs, desired layers, Ableton construction and emotional function");
const missingHeaders = ["missing_cue_or_system", "priority", "domain", "runtime_trigger_inputs", "physical_layers", "ableton_starting_point", "status", "emotional_function", "reference_ids"];
missing.getRange("A3:I3").values = [missingHeaders];
missing.getRangeByIndexes(3, 0, missingRows.length, missingHeaders.length).values = missingRows;
styleHeader(missing.getRange("A3:I3")); styleBody(missing.getRangeByIndexes(3, 0, missingRows.length, 9));
missing.tables.add(`A3:I${missingRows.length + 3}`, true, "MissingSoundBacklogTable");
missing.getRange(`A4:I${missingRows.length + 3}`).format.fill = "#F8FAFC";
missing.getRange(`A4:I${missingRows.length + 3}`).conditionalFormats.addCustom("=MOD(ROW(),2)=0", { fill: "#E7F0F5" });
missing.getRange(`B4:B${missingRows.length + 3}`).dataValidation = { rule: { type: "list", values: ["P0", "P1", "P2"] } };
missing.getRange(`G4:G${missingRows.length + 3}`).dataValidation = { rule: { type: "list", values: ["Planned", "Designing", "Auditioning", "Approved", "Implemented", "Blocked"] } };
addPriorityFormatting(missing.getRange(`B4:B${missingRows.length + 3}`)); addStatusFormatting(missing.getRange(`G4:G${missingRows.length + 3}`));
missing.freezePanes.freezeRows(3); missing.freezePanes.freezeColumns(2);
setWidths(missing, [38, 10, 22, 42, 46, 44, 14, 32, 18]);

// Reference library
titleBand(refs, "A1:G2", "REAL-WORLD REFERENCE LIBRARY", "Official measurements and recordings first; museum and firsthand testimony are clearly labeled; no copyrighted media was copied");
const refHeaders = ["id", "domain", "url", "publisher_or_type", "supports", "rights_or_use", "confidence"];
refs.getRange("A3:G3").values = [refHeaders];
refs.getRangeByIndexes(3, 0, referenceRows.length, refHeaders.length).values = referenceRows;
styleHeader(refs.getRange("A3:G3")); styleBody(refs.getRangeByIndexes(3, 0, referenceRows.length, 7));
refs.tables.add(`A3:G${referenceRows.length + 3}`, true, "ReferenceLibraryTable");
refs.getRange(`A4:G${referenceRows.length + 3}`).format.fill = "#F8FAFC";
refs.getRange(`A4:G${referenceRows.length + 3}`).conditionalFormats.addCustom("=MOD(ROW(),2)=0", { fill: "#E7F0F5" });
refs.getRange(`G4:G${referenceRows.length + 3}`).dataValidation = { rule: { type: "list", values: ["High", "Medium/High", "Medium", "Low/Medium", "Low", "None"] } };
refs.getRange(`G4:G${referenceRows.length + 3}`).conditionalFormats.add("containsText", { text: "High", format: { fill: "#DCEBDF", font: { color: GREEN } } });
refs.getRange(`G4:G${referenceRows.length + 3}`).conditionalFormats.add("containsText", { text: "Low", format: { fill: "#F4D7D4", font: { color: RED } } });
refs.freezePanes.freezeRows(3); refs.freezePanes.freezeColumns(2);
setWidths(refs, [9, 20, 58, 32, 52, 40, 14]);

// Ableton architecture
titleBand(ableton, "A1:F2", "ABLETON LIVE 12 // KESSEL 57T MACHINE", "Same inharmonic source language for diegetic sound and score; game telemetry maps to real synthesis controls");
ableton.getRange("A4:F4").values = [["chain", "name", "devices", "purpose", "primary_macros", "deliverable"]];
ableton.getRangeByIndexes(4, 0, abletonChains.length, 6).values = abletonChains;
styleHeader(ableton.getRange("A4:F4")); styleBody(ableton.getRangeByIndexes(4, 0, abletonChains.length, 6));
ableton.getRange("A16:E16").values = [["macro", "name", "kind", "game_source", "sound_control"]];
ableton.getRangeByIndexes(16, 0, macroRows.length, 5).values = macroRows;
styleHeader(ableton.getRange("A16:E16")); styleBody(ableton.getRangeByIndexes(16, 0, macroRows.length, 5));
ableton.getRange("A34:F34").merge(); ableton.getRange("A34").values = [["DELIVERY BOUNDARY"]];
ableton.getRange("A34:F34").format = { fill: ORANGE, font: { bold: true, color: DARK }, horizontalAlignment: "center" };
ableton.getRange("A35:F39").merge(); ableton.getRange("A35").values = [["Live/M4L is the authoring and telemetry-audition environment. Export aligned one-shots, loops, RPM × load × perspective grids, and separate diegetic/score/trauma stems. The Rust game remains authoritative for state, speed-of-sound delay, bearing, occlusion, cabin transmission and the shared safety limiter. A native Live device cannot be embedded as an Ableton SDK plug-in. Use RNBO or native Rust/C++ only if an approved continuous DSP core truly needs to ship."]];
ableton.getRange("A35:F39").format = { fill: CHARCOAL, font: { color: IVORY, size: 10 }, wrapText: true, verticalAlignment: "top", borders: { left: { style: "thick", color: ICE } } };
ableton.getRange("A41:F41").values = [["file", "role", "versioning_rule", "shipping_status", "license_note", "official_reference"]];
ableton.getRange("A42:F47").values = [
  ["Kessel_57t_Machine.als", "Canonical Live Project Set", "Save a Live 12.4.3 copy before 12.4.5 resaves", "Authoring only", "Collect All and Save owned/cleared media", "L01, L08–L11, L21"],
  ["Kessel_57t_Machine.adg", "Outer 16-Macro Instrument Rack", "Version with cue workbook", "Authoring only", "Native Live devices do not ship in game", "L02–L08"],
  ["Kessel_Telemetry.amxd", "Frozen UDP telemetry/event bridge", "Freeze dependencies before sharing", "Authoring only", "M4L is Live-dependent", "L12–L17"],
  ["Approved WAV stems", "One-shots, loops, grids and score stems", "Hash and catalog every export", "Ships after rights review", "Use original synthesis and owned/cleared sources", "L09–L11, L22–L23"],
  ["RNBO export", "Optional future continuous DSP", "Prototype before adoption", "Possible runtime component", "Separate RNBO/Steinberg/signing review", "L18–L20"],
  ["Factory Pack material", "Reference or transformed composition layer", "Never isolate by default", "Blocked unless cleared", "Do not assume rendered Pack samples are redistributable", "L22–L23"],
];
styleHeader(ableton.getRange("A41:F41")); styleBody(ableton.getRange("A42:F47"));
ableton.freezePanes.freezeRows(2);
setWidths(ableton, [10, 28, 42, 58, 36, 42]);

for (const sheet of [overview, audition, implemented, missing, refs, ableton]) baseSheet(sheet);

await fs.mkdir(path.dirname(outputPath), { recursive: true });
await fs.mkdir(previewDir, { recursive: true });

const checks = {
  auditionCueCount: audition.getRange("A4:A82").values.filter((row) => row[0] !== null && row[0] !== "").length,
  implementedRows: implementedRows.length,
  missingRows: missingRows.length,
  referenceRows: referenceRows.length,
  sheets: [],
  formulaErrors: [],
};

for (const sheet of [overview, audition, implemented, missing, refs, ableton]) {
  const used = sheet.getUsedRange();
  const values = used.values;
  for (let row = 0; row < values.length; row += 1) {
    for (let col = 0; col < values[row].length; col += 1) {
      const value = values[row][col];
      if (typeof value === "string" && /^#(REF|DIV\/0|VALUE|NAME|N\/A|NUM|NULL|SPILL|CALC)!?/.test(value)) {
        checks.formulaErrors.push(`${sheet.name}!R${row + 1}C${col + 1}:${value}`);
      }
    }
  }
  const preview = await workbook.render({ sheetName: sheet.name, autoCrop: "all", scale: 0.8, format: "png" });
  const previewPath = path.join(previewDir, `${sheet.name.replaceAll(" ", "-").toLowerCase()}.png`);
  await fs.writeFile(previewPath, new Uint8Array(await preview.arrayBuffer()));
  checks.sheets.push({ name: sheet.name, rows: values.length, columns: values[0]?.length ?? 0, preview: previewPath });
}

if (checks.auditionCueCount !== 79) throw new Error(`audition sheet has ${checks.auditionCueCount} cues`);
if (checks.formulaErrors.length) throw new Error(`formula errors: ${checks.formulaErrors.join(", ")}`);

const inspection = await workbook.inspect({
  kind: "workbook,sheet,table,formula",
  maxChars: 12000,
  tableMaxRows: 4,
  tableMaxCols: 8,
  tableMaxCellChars: 70,
});
checks.inspection = inspection.ndjson ?? inspection;

const xlsx = await SpreadsheetFile.exportXlsx(workbook);
await xlsx.save(outputPath);
await fs.writeFile(path.join(root, "WORKBOOK_VALIDATION.json"), `${JSON.stringify(checks, null, 2)}\n`, "utf8");
console.log(JSON.stringify({ status: "PASS", outputPath, ...checks, inspection: undefined }, null, 2));
