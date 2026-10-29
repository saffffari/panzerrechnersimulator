# Tiger game development map

28 October 2026. **Stage: design consolidation and playtest planning.** This is the working direction to investigate next, with unresolved choices kept explicit. It does not authorize gameplay implementation or commit the project to every feature below.

**The tank is your home, and you are its keeper.** You operate a powerful but vulnerable Tiger, depend on a small familiar crew, carry your equipment with you, receive assignments, and live with the mechanical and human consequences. Combat, travel, maintenance and quiet preparation should give one another purpose.

The strongest current diagnosis concerns the structure of play. Starting at one end of a predictable route, shooting the expected opposition and reaching the other end offers too few different decisions. The detailed tank can support a much richer game, but making every component interactive will not, by itself, solve that problem. The next stage must establish that different assignments, understandable choices and persistent consequences are enjoyable before multiplying the mechanical workload.

The proposed development order is **human baseline → contrasting missions → one complete repair → a short persistent chain of assignments → selective expansion**. Selected model inventory and historical checks can run alongside the early tests. Full preparation of the model waits for evidence that the mechanical loop is worth expanding.

## Direction for this stage

The discussion supports the following working direction:

- Keep the embodied first-person Tiger and its physical character. The player should continue to drive, observe, aim, fire, move inside, dismount and handle equipment.
- Treat the tank as the persistent home. Its supplies, crew, modifications, tools and scars travel with it. A permanent external base is not the organizing requirement.
- Give the player varied assignments in connected geography. An assignment needs a purpose beyond clearing the next group of tanks.
- Keep the player locally consequential. Most situations should involve a small force for which this particular Tiger matters, rather than fifty friendly Tigers with the player lost among them.
- Use text, objects, behavior and the environment to communicate. **No spoken dialogue.** A physical repair manual and written orders are compatible with this direction.
- Develop physical diagnosis, repair and replacement as a major candidate for the game's identity. Useful knowledge and ownership should accumulate alongside equipment.
- Keep direct player involvement in combat. The leading crew proposal lets living specialists extend the player's control while preserving personal aiming and firing.
- Seek outside playtest feedback early. Familiarity with the project and successful automated runs cannot tell us whether a new player enjoys it.

The single obligatory retreat from the Eastern Front to Germany is no longer an assumed campaign structure. Neither a fixed Reichstag finale nor a permanent workshop hub is required by this stage. Earlier documents remain records of earlier designs; this map carries the later discussion forward. Theatre, date, campaign ending, run length and death rules remain open.

## The experience to connect

The candidate loop is:

**Receive an order → assess the tank and prepare → choose a route and approach → act in a changing situation → recover and diagnose → repair or improve → see the consequence and receive the next assignment.**

These activities need different rhythms. A difficult engagement can be followed by a reliable stretch of driving, a quiet inspection, shared work with the crew or a satisfying improvement. Damage should sometimes create a new problem; it should not turn every encounter into the same compulsory repair stop. Preparation should sometimes work so well that the tank simply does its job.

An illustrative sequence, not an approved mission script:

1. Start outside the tank beside the crew. A manageable mechanical problem, familiar belongings and a written assignment establish who is travelling together and why they must leave.
2. Support a small infantry party by opening one blocked road. Success means the party passes through, not that every enemy on the map dies.
3. Receive an order to hold a crossing while a recovery vehicle approaches. The player chooses a position and preserves an escape route.
4. The recovery vehicle is delayed. A report creates a choice: remain, investigate or help open another route. Any urgency has an understandable cause and visible consequences.
5. Return from the assignment with a fault. Diagnose it, open the relevant access cover, replace or repair a physical assembly, and test the result. A workshop reached through the mission can supply a missing tool or compatible part.
6. The saved vehicle or familiar crew appears again. The tank still carries the replacement, the remaining damage and the supplies actually left aboard.

The point is the connection between activities. A mission creates a reason for the repair; the repair changes the next mission; recurring people give the outcome meaning.

## Orders and mission structure

### Dispatch through the radio station

The radio operator's station should become a place where the player receives and reviews written orders. Every order must make **purpose, location, urgency and completion condition** immediately understandable. Supporting information can describe reported opposition, available help, distance, route conditions and known equipment at the destination.

A physical message log is the strongest historical candidate. The proposed typewriter or paper-feed interface remains an attractive fictional presentation option. The Tiger's Fu 5 radio equipment used voice and keyed communication; an automatic thermal-style order printer would be a deliberate departure. A radioman's transcribed message preserves the text interface without requiring the player to learn Morse. See the [radio arrangement and manual illustration](https://tiger1.info/EN/Radio-wiring.html).

The log can retain orders, corrections and outcomes. Repeated signatures or callsigns can establish characters without a conversation system. Critical updates should remain readable when the player is away from the radio seat; every new message must not require abandoning the sight during combat. The exact delivery cue, interruption rules and physical interface need a usability test.

### Orders versus choice

This remains an important design decision. The user sees appeal in both choosing assignments by risk and reward and receiving orders that must be followed.

The proposed middle ground is **orders with discretion**: a clear priority assignment, freedom over preparation and execution, optional requests that compete with it, and occasional genuine choice between assignments. Reports can expose risk and opportunity without becoming a shop menu of guaranteed loot and difficulty stars.

An urgent order must have understandable timing. Routine assignments should provide real room to inspect and repair. Avoid a world that repeatedly claims an emergency while waiting indefinitely, and avoid a clock that punishes someone for learning the book. A rendezvous marker identifies a place to meet or deploy; touching it need not magically start every surrounding event.

### Mission families

These are a catalogue to select from, not a commitment to build every family at once.

| Assignment | What makes it different | Readable success or changed outcome |
|---|---|---|
| Support an assault | Coordinate arrival, observation and fire with a small friendly group | The group reaches or occupies its objective |
| Breach a strongpoint | Identify the position preventing movement and create an opening | A specified route becomes usable; surrounding enemies can remain |
| Defend a crossing | Choose fields of view and hold for a purpose | A column crosses, engineers finish work or relief arrives |
| Delay and withdraw | Spend time or space while preserving an exit | Others disengage and the Tiger leaves before being trapped |
| Recover a vehicle or crew | Secure access, inspect the casualty and obtain help or equipment | People or a recoverable machine reach safety |
| Escort a recovery or supply party | Protect something with a useful role and a vulnerable route | The party completes its job, providing later capability |
| Reopen a road | Resolve an obstruction, threat or loss of control | Movement resumes, changing later travel or supply |
| Counterattack or reinforce | Reach a threatened place and change an unstable situation | A bounded position stabilizes, or an organized withdrawal becomes possible |
| Reconnoitre or verify a report | Observe, distinguish uncertainty and decide how far to expose the tank | Useful information is delivered; fighting may be avoidable |
| Deliver a message or essential item | Move through uncertain territory with a reason to arrive intact | The recipient receives it and acts; a short route can carry greater risk |
| Intercept a convoy or strike a command position | Choose timing and access against a specific target | The named movement or capability is interrupted |
| Remain in reserve | Prepare under incomplete information, then respond to a developing need | The player arrives ready where that preparation matters |

The assassination idea fits a bounded command-post or convoy mission more readily than a separate stealth assassination game. Reserve duty needs useful preparation and a timely development; waiting with nothing to decide is not a mission mechanic.

For each authored assignment, record the obligation, protected people or capability, information available, terrain choices, pressure, completion condition, withdrawal conditions, consequences and possible recovery from failure. Change those properties before increasing enemy counts. Reusing a place under a different obligation can be more valuable than building another interchangeable arena.

### Local scale and player agency

The surrounding war can be large through distant fighting, radio traffic, passing columns and aftermath. The playable job should usually be comprehensible at a smaller scale: the player's Tiger, some infantry or engineers, a recovery party, perhaps another tank whose crew becomes recognizable. Exact force counts remain a tuning choice.

Allies should have useful capabilities and intentions. Infantry can provide information or protection; engineers can make a route usable; recovery personnel can enable work the crew cannot do alone. They should not exist only as fragile escort objects. An occasional second friendly tank can matter precisely because it is unusual.

Historical battalion operations provide situations and constraints, not enemy totals to assign to one player. Adapt the objective and local opposition together. The player should feel powerful within a bounded task without becoming capable of personally defeating an army.

Competence must change outcomes. The wider war may deteriorate, but a well-run defense, a rescued crew or a successful repair must remain a real achievement. Some assignments can fail without ending the campaign, and some should end in a clean success. Do not automatically increase every enemy's strength to cancel a newly installed upgrade.

## The tank as home

The tank carries the resources the player expects to have available: ammunition, fuel, tools, spares, diagnostic equipment and personal supplies. Food and water can support atmosphere and attachment without committing to hunger and thirst meters.

Storage should be physical and understandable. Protected internal space competes with exposed external space. Frequently needed tools deserve accessible positions. A large spare consumes carrying capacity that could have held fuel or ammunition. New racks and brackets make that choice visible on the vehicle.

Placement must respect hatches, engine covers, observation, turret movement and service access. The user can personalize the tank without a universal ability to bolt anything anywhere. Forgiving placement assistance and clear mounting regions are candidates; repeated pixel-perfect packing should not become the main challenge. Additional external fuel arrangements need historical and physical review before becoming ordinary equipment.

Temporary depots, shelters, garages and workshops can provide people, shelter, supplies, lifting equipment or fabrication capability. They do not have to become a recurring home base. Repair is possible wherever the required access, tools, parts and assistance exist. A major engine or transmission job still needs convincing handling.

Persistence needs to cover installed components and their condition, open or removed panels, cargo quantities and locations, crew state, remaining ammunition and fuel, relevant world changes, and the order log, for the selected systems and places in the slice. This is a design requirement for the integrated test, not a claim that all of it is already saved correctly. Save, reload and recovery rules must protect the meaning of physical work.

## Geography and navigation

A continuous operations region is the current candidate. “Massive” is an aspiration to evaluate after a compact region proves interesting. More kilometres can reproduce the existing problem if travel offers no decisions, anticipation or discovery.

Use recognizable roads, crossings, shelters, settlements and landmarks with more than one useful route. Revisit places under changed circumstances: a secured road becomes a supply route, a previous fighting position becomes a recovery site, or a bridge's condition changes access. Travel should expose the condition of the war and offer reasons to pay attention.

Retain a clear live player location marker and a known assignment destination. The current held-map renderer already plots the player's position; the work is to verify and clarify that experience, including its relationship to existing dead-reckoning guidance. A tank marker when dismounted would help the player return home. Unknown targets can use approximate search areas without revealing enemies omnisciently.

One physical map with regional and closer detail is a candidate, rather than assuming a separate tactical interface. Automatic position marking is an intentional navigation aid. It does not require making the tank behave as though it had a historical GPS receiver.

## Crew and direct control

The leading proposal is a crew that extends the player's reach across stations. A living driver can execute driving input from the cupola; a living loader keeps the weapon supplied; the gunner can bring the weapon toward a designated sector. The player still makes the precise shot.

| Role | Intended contribution to test | What absence should change |
|---|---|---|
| Driver | Execute the player's movement intentions while the player observes or attends another task | Remote driving is lost; someone must occupy and operate the controls |
| Gunner | Respond to a rough bearing or pointing cue, with visible acknowledgement and traverse | Sector handoff is lost; direct operation remains possible if the mechanism works |
| Loader | Load and handle ammunition as a recognizable physical job | Loading becomes the player's or another survivor's responsibility |
| Radio operator | Receive and record orders, maintain a connection to people outside | Receipt and handling of messages require another arrangement or direct radio interaction |
| Commander or player | Observe, prioritize, prepare, choose movement and engagement, and intervene directly | The exact player embodiment and death rules still need a decision |

The proposed bearing cue should resemble pointing or giving a clock direction, not perfect target identification, range finding and tracking. `V` was suggested, but it currently draws the mounted sidearm. Select or reassign a binding deliberately. Clicking the living gunner to enter the sight, and the sight itself when he is gone, is a candidate physical interface.

That click also exposes an unresolved embodiment choice: is the player controlling the crew collectively, or literally moving one person into another man's station? Resolve it before designing transitions and casualty behavior. The current gunner defaults to fire at will; changing that is a deliberate control experiment, not a feature already delivered.

Crew effectiveness must leave useful work for the player without requiring a casualty to unlock hands-on interaction. A safe, bounded driving order that continues while the player moves inside is worth testing. Acknowledgement, effort, lost capability and completion should be conveyed through gesture, work animation, mechanical sound and readable cues. Optional assistance can support accessibility without spoken callouts.

Attachment needs repeated behavior as well as belongings. A crewman can anticipate a tool handoff, tend his usual station or help with a repair. An injury changes that routine; an empty seat leaves a recognizable absence. Avoid reducing each man to a maintenance meter or a replaceable statistical bonus.

## Damage diagnosis and repair

### The physical contract

The visible component, the component affected by damage and the component repaired should refer to the same persistent part. Opening its cover changes access. Removing it changes what the vehicle can do. Installing another leaves the replacement visible and changes the relevant behavior.

The source model gives a strong basis for this. The owner verified that modeled components are separate objects. The converted complete RM5010 assembly preserves 1,481 mesh objects. That does not establish that every hidden mechanism is modeled: the inspected EngineSystem contains fourteen named child meshes, while separate pistons, a crankshaft and complete transmission gearing were not confirmed. Preserve the existing separation and identify additional geometry only when an interaction needs it.

The long-term ambition can include every relevant panel and cover. The first implementation candidate should include one complete access-and-repair chain, not all objects at once. Group repetitive fasteners into sensible interactions while retaining the important sequence, connections, mass and access constraints.

### Failure families

| System | Observable problem candidates | Different responses worth supporting |
|---|---|---|
| Cooling and engine services | Rising temperature, leakage, abnormal sound or unreliable running | Change operation, inspect, repair a service component, replace an assembly |
| Drivetrain and steering | Engine runs but movement changes, unusual noise, loss of one function | Diagnose the affected path, preserve remaining mobility, recover or replace |
| Tracks and running gear | Loss of drive on one side, pulling, visible broken or displaced parts | Reposition if possible, perform a field repair, seek recovery assistance |
| Turret drive and ring | Powered or manual traverse fails, resistance or obstruction | Inspect the actual fault, change the fighting plan, withdraw or seek heavier work |
| Gun and recoil equipment | Disturbed operation, leakage, damaged mounts or alignment | Stop unsafe operation in the fiction, inspect, adjust or replace the affected assembly |
| Optics and observation | Changed view, obstruction, damage or loss of alignment | Clean, inspect, use another viewpoint, replace or calibrate |
| Electrical and communication equipment | An instrument, starter function or radio stops responding | Trace a supported fault using appropriate tests and equipment |
| Covers, stowage and protection | Access blocked, exposed equipment, damaged fittings or local armor damage | Clear access, secure a replacement or patch, accept a remaining limitation |

These are design categories, not a verified Tiger maintenance procedure. Each selected system needs a historically grounded access path and a finite, learnable set of faults. Hit location and damage propagation should determine which supported faults occur. Different combinations, terrain, available equipment and pressure can provide variety without a new bespoke puzzle for every shell.

A lodged projectile or fragment is a compelling inspection detail. Extracting it does not automatically restore broken gears or a deformed housing. A welded patch does not automatically restore pristine armor resistance. Cosmetic damage, obstruction, leakage, maladjustment, broken parts and irreparable destruction need different meanings.

### Recovery must leave choices

A jammed turret could lead to withdrawal, concealment, another way of aiming, assistance, a temporary intervention or a dismounted action. It should not always force “exit, kill everyone, repair, repeat.” Panzerfausts can become useful during appropriate later-war scenarios, but their availability depends on the selected period. On-foot combat should support the tank-centered experience before it grows into a second full game.

Investigation, salvage and rescue can also take the player some distance from the tank. Returning to a familiar machine should matter. The map and crew behavior need to support that separation without an arbitrary short tether or an assumption that the unattended Tiger will fight effectively on its own.

Damage needs readable symptoms and plausible opportunities to respond. Major failures can be memorable; constant multi-system breakdowns will erase contrast and make every success feel like deferred chores. Natural play must demonstrate recoverable failures. A specially staged survival fixture only proves that a recovery path can execute.

## The physical repair manual

The book is the candidate teaching and mastery system. It opens with symptom-led yes/no questions, then directs the player to observations, tests, drawings and procedures. Every question must say what to check and show where to check it.

An illustrative structure is: **turret does not turn → check whether the supported manual traverse still works → follow the relevant drive or obstruction inspection → perform a repair → test movement again.** The actual branches must match the chosen Tiger configuration and implemented faults. The book must never diagnose an invisible component merely because the game knows its state.

Use line drawings with the same orientation and recognizable features as the physical assembly, concise text, labels for unfamiliar parts, and clear return points after tests. Bookmarks, tabs, a glossary and an enlarged reading view are useful candidates. Low-resolution art direction must not make essential instructions unreadable.

The player should be able to keep a place in the book while inspecting the vehicle. Test that interaction directly. Knowledge should reduce dependence on the book over time; a familiar fault should become recognizable and quicker to solve. New parts can come with a supplement page or different diagnostic requirements.

The opening can teach through one manageable fault with room to investigate. Avoid an extensive detached tutorial, but still teach the basic act of finding, reading and using the manual. Removing tutorial screens does not remove the need for instruction.

Diagnostic equipment and local indicators can extend the player's ability to investigate. Each instrument should reveal something it could measure. A universal damage lamp that simply names the broken component would undercut the manual's purpose.

## Physical progression and upgrades

Progression has four possible sources: restoring lost function, fitting a documented improvement, making an explicitly fictional but mechanically coherent adaptation, and becoming more knowledgeable. The tank can also become more personal through practical storage and accumulated evidence of the journey.

The physical sequence is **discover or earn access → identify compatibility → transport and stow → reach the required equipment → remove and install → test → use the changed capability**. A mission can lead to a depot, a donor vehicle, a mechanic or a workshop truck. The reward need not appear as abstract points or a menu purchase.

| Project | Candidate value | Historical boundary |
|---|---|---|
| Cooling repair or revised assembly | More dependable sustained operation and a visible mechanical result | Verify the chosen fan, drive and engine configuration; avoid invented universal performance percentages |
| Engine replacement or conversion | Restore serviceability; a major physical project | HL210 to HL230 conversion is documented, with associated changes; a late Tiger cannot receive that improvement twice |
| Transmission or steering overhaul | Restore lost capability and teach the drivetrain | An interchangeable ladder of faster Tiger transmissions is not established by the research |
| Optic and compatible fittings | Change observation, operation or protection in a perceivable way | Real TZF9b and TZF9c configurations exist; compatibility and installation matter |
| Barrel, muzzle brake and mantlet work | Distinct condition, handling, protection or serviceability where justified | Two brake forms and mantlet changes are documented; arbitrary damage bonuses or a simple Tiger II gun swap are not |
| Stowage and spare-track carriers | Carry a different kit and make ownership visible | Respect existing fittings, movement and service access |
| Internal engine parts | Potentially deeper diagnosis and rebuilding | Separate source geometry and a historical performance-piston progression are not established |
| Salvaged protective plates | A possible physical improvisation with local protection and costs | A general Tiger program of welding enemy armor onto the hull is not established; treat such variants as declared fiction |

Detailed evidence, including cupolas, wheels, hatches, ventilation, roof changes and production compatibility, is in the [historical modifications inventory](C:/shop/panzerrechnersystem/docs/research/TIGER_I_HISTORICAL_MODIFICATIONS_20261028.md). A production change is not automatically a field upgrade. Choose the starting tank and campaign period before freezing the available parts.

Upgrades should alter the experience in ways connected to their function. Weight, clearance, resource use, reliability or effort can create tradeoffs. Some changes can be straightforward improvements; every reward need not conceal a penalty. Threat progression should be tested against geography and mission demands before adopting direct scaling to the player's equipment.

## Narrative and emotional progression

The campaign needs a continuing reason to accept the next assignment and care about what survives. Written orders can communicate objectives, but orders alone will not supply that attachment.

Build a small recurring cast through action: the crew, a recovery party, a mechanic, another tank crew or the people repeatedly affected by the player's work. Give them recognizable routines, belongings and purposes. A person rescued earlier can later supply help; a familiar location can show what changed after the Tiger left. These are possibilities to test, not a fixed cast list.

The tank records the same history through repairs, components, storage and damage. Quiet time lets the player notice those changes. A repair can become a shared event rather than a solitary series of clicks.

An arc from offensive confidence toward defensive and increasingly desperate work is a strong candidate. It offers changing responsibilities without requiring constant new systems. It is not yet the selected story, and it must fit a period in which Tigers actually operated. A German crew's perspective should coexist with the experiences of opposing crews and people outside the tank; neither the invasion nor its consequences should disappear from the world.

FTL is the reference for a manageable journey with dense decisions and compact story relative to play. It does not settle exact hours, permadeath, procedural generation or a sector map. Detailed physical work consumes more time than a menu action. Session length, saving and repeated repairs must be evaluated together.

## Existing foundations and gaps

The October 28 project audit and a follow-up source check at revision `d47884b` provide the baseline below. Subsequent work since the audit concerns optics, vehicle finishes and distant effects. The checkout also has concurrent rendering and optics edits; these entries describe the gameplay paths inspected, not a new end-to-end human verification.

| Foundation already present | Next-stage implication |
|---|---|
| A functioning first-person interior, driving, sight operation, loading, dismounting and hand equipment | Build the new loop around these strengths; do not start a replacement tank controller |
| Five living solo crew with wounds, assistance and station takeover | Test control authority and comprehension; the current fire-at-will default differs from the leading direct-fire proposal |
| Target designation and crew orders | Compare existing behavior with the proposed coarse bearing handoff before adding a parallel control scheme |
| Localized component faults, field/workshop distinctions, physical track fitting, vision-block replacement, armor patches and salvage | Extend these systems; most other interior repairs still use timed work rather than removing and replacing the specific component |
| Written orders, radio text, map review, bearings, flares and delayed consequences | A physical dispatch desk and persistent correspondence are the new presentation and continuity layer; verify a reliable solo access path to the represented radio station |
| Objective primitives for entering, observing, destroying, holding, surviving, recovering and escorting | Deepen existing primitives into distinct situations; current recovery and escort checks do not constitute complete recovery or escort gameplay |
| Authored canyon beats, a director with recovery/contact gates, and enemy flank and reposition behavior | Differentiate obligations and completion conditions before adding more combat density |
| A held map with a live player point, route guidance and a separate dead-reckoning estimate | Keep navigation readable and resolve conflicting guidance rather than treating a player marker as entirely missing |
| Briefing, run completion/loss cards and repeatable harnesses | Use these to support testing, while adding truthful mission outcomes and understandable failure causes |
| Flattened Blender collection and preserved CAD assemblies | Begin selected component preparation without repeating the completed conversion |

Priority baseline checks are persistent driver intent while the player moves inside, useful crew behavior while dismounted, gunner acquisition and fire authority, mission status between authored beats, and distinct reporting for bypassed versus destroyed opposition. Walking inside currently zeros the driver's orders. The gunner's default acquisition is more autonomous and precise than the proposed sector cue. Bypass still completes some destruction objectives while their messages announce that opposition was cleared. These differences matter before persistent consequences can be trustworthy.

The opening also still contains descriptions of a lone crewman, despite the five-man solo roster. Bring those instructions into agreement with the selected test behavior before asking unfamiliar players to learn it. Existing persistence can reduce detailed faults to working-or-broken state; a durable identity and condition for each selected part is foundational work, not a final polish task.

Implementation anchors: [mission outcomes and objectives](C:/shop/panzerrechnersystem/src/mission.rs:693), [crew default](C:/shop/panzerrechnersystem/src/crew_solo.rs:371), [driver order eligibility](C:/shop/panzerrechnersystem/src/crew_solo.rs:953), [held-map player marker](C:/shop/panzerrechnersystem/src/ui.rs:2403), [current V binding](C:/shop/panzerrechnersystem/src/main.rs:15104), and [repair and persistence limitations](C:/shop/panzerrechnersystem/docs/REPAIR_INTERACTION_20260929.md:90). These are candidates for bounded work after planning, not changes made by this document.

The asset conversion produced **1,467 Blender files**, with supplied maps packed where available. **387 are self-contained; 1,080 retain missing source-map references.** Ten unmatched native SolidWorks files remain unconverted in the archive. The archive contains preserved originals and must remain until the owner finishes review. See the [conversion report](C:/shop/ts_assets/_archive/_conversion_report_20261028/README.md).

For the selected Tiger subsystem, preparation must identify assembly relationships, names, units, pivots, access dependencies, interaction locations, materials, collision needs, damage representations and suitable runtime detail. Check source rights and historical configuration before promoting geometry into the shipped asset set. Separate meshes are a major advantage, not proof of complete gameplay readiness.

## Development sequence and gates

The sequence below describes future work to authorize and evaluate. There are no calendar estimates while the scope and asset preparation costs remain uncertain.

### Stage 0 Establish the human baseline

Select and record a stable playable revision, with briefing and help that accurately describe its crew and controls. Give a small group of unfamiliar players the current short scenario with minimal coaching. Include people attracted to simulation and people attracted to the vehicle-adventure premise. Observe what they believe their job is, what they think each crewman does, where they lose control, and why they stop caring.

Record concrete events: missed orders, unexplained controls, time spent without a decision, recoverable versus fatal damage, repeated handling, understanding of defeat, and whether they voluntarily want another attempt. Ask them to describe their most interesting decision and least enjoyable stretch. The working explanation is predictable mission structure; test it rather than asking people only to confirm it.

**Gate:** identify the main sources of boredom and confusion well enough to choose the next bounded experiment. A functioning automated run is supporting evidence, not this gate.

### Stage 1 Prove mission variety with the existing tank

Use a compact shared area for three contrasting jobs: support an advance, hold a passage for others, and withdraw or recover something. Supply readable orders, clear completion conditions and useful route choices. Use existing assets and mechanics. Address control or feedback defects only where they prevent a fair comparison.

Compare the leading direct-fire crew model with the existing model in the same short encounter, using the same world, seed and starting state. Then hold the selected control behavior constant across the contrasting assignments so the effect of mission variety can be assessed separately. Ensure a player can understand who is acting and why. Retain the live map marker. Separate mission outcomes such as destroyed, bypassed, held, rescued and withdrawn.

**Gate:** players change preparation or approach because the assignment changed, understand what success requires, and can name a decision that affected the result. If the three jobs still feel like the same drive-and-shoot corridor, revise their structure before expanding the map.

### Stage 2 Prove one complete mechanical learning loop

Choose a provisional Tiger configuration for this test. Prepare one bounded assembly with its actual access cover, recognizable fault symptoms, one short manual decision tree, appropriate tools, a physical repair or replacement and a meaningful post-repair test. Give the selected part a stable identity, condition and saved installation state from the outset. Candidate subjects are cooling service equipment or a supported turret-drive fault; choose after inspecting access and existing simulation.

Include one alternative response to the failure, such as withdrawing to work or continuing with a limitation. Test the same procedure again after the player has learned it. Include a short safe learning situation and a later situation with understandable pressure.

**Gate:** players diagnose through evidence, understand why the intervention helped, and find the return to operation satisfying. Familiarity should make the work easier. If the repeated procedure becomes a chore, simplify its handling before adding more faults. Do not present a scripted damage fixture as evidence that ordinary combat produces this frequency of recoverable damage.

### Stage 3 Connect orders ownership and consequences

Choose provisional save, reload and setback rules, along with a concrete personal or operational reason to continue. Build a short chain that carries the same tank and crew through the contrasting jobs. Add a persistent written log, a small physical kit selection, one visible installed improvement, a temporary service opportunity and one recurring relationship. Preserve state across departure, return and reload.

This is the first integrated slice: order, preparation, route, action, consequence, diagnosis, physical work and the next order. It can reuse the same few places. The external workshop supplies a capability; the tank remains home.

**Gate:** players can explain what changed between assignments, why they care about the next one, and how their tank has become their own. They should sometimes enjoy reliable operation after doing good work. Repeat the slice after interruption and reload to test whether persistence feels trustworthy.

### Stage 4 Expand only the parts that earn their cost

Confirm or revise the broader campaign period, starting configuration, campaign structure and failure policy using the earlier results. Extend the component catalogue and manual together, add mission families that produce different decisions, and enlarge geography only where it supports those assignments.

Broader preparation of the detailed model, major engine or transmission work, additional historical configurations, larger narrative arcs and more elaborate storage belong here. Review each addition against the same question: what new observation, choice, physical satisfaction or consequence does this provide?

**Gate:** the expanded content preserves comprehension and pacing without requiring repeated explanations or accumulating mandatory chores. Performance and asset readability must remain suitable for ordinary native play, including time spent inside and around the tank.

### Dependencies and work that can overlap

| Workstream | Can begin alongside | Must have before expansion |
|---|---|---|
| Human baseline and control review | Selected model inventory and historical configuration review | A usable short scenario and honest observations |
| Mission variety | Paper order formats and story sketches | Clear control authority, readable objectives and truthful outcomes |
| One mechanical procedure | Mission experiments, after its subsystem is chosen | Mapped part, supported fault, access sequence, feedback and test |
| Manual pages | The corresponding procedure | Verified questions that match actual behavior and readable drawings |
| Physical storage and improvement | Preparation of the integrated slice | A limited useful kit, mounting rules and persistent state |
| Larger campaign and model preparation | Only selective research before the gates pass | A compelling integrated slice and decisions on period, scope and loss |

## Playtest questions and scope controls

The most useful questions are behavioral: does the player notice a choice, act on it, understand the consequence and want to continue? Praise for the model is valuable but does not answer those questions.

| Risk | Design response to test |
|---|---|
| More impressive machinery but the same repetitive missions | Prove contrasting objectives before broad mechanical expansion |
| A crew that plays the enjoyable parts for the player | Preserve direct shooting and voluntary hands-on work; make delegation legible |
| Repair becomes a tax after every fight | Tune incidence, allow reliable stretches and provide alternatives |
| Diagnosis is actually a hidden interaction hunt | Make symptoms, drawings, access and feedback correspond clearly |
| A huge region becomes long empty travel | Validate a smaller connected area with consequential route choices |
| Loss destroys motivation to maintain the tank | Test saving and setback rules before committing to permadeath |
| Rising enemies erase every improvement | Let upgrades resolve familiar problems and harder situations arise from context |
| Historical detail turns into arbitrary incompatibility traps | Make fit, required equipment and expected consequences inspectable |
| Personalization obstructs ordinary operation | Enforce clearances and show blocked access before installation |
| The war makes competence feel meaningless | Preserve local victories, relationships and consequences the player can influence |

The first slice does not require a vast open world, a dynamic simulation of an entire front, every object becoming destructible, individual work on every screw, a full vehicle-building economy, hunger and thirst systems, a separate infantry campaign, fifty allied tanks or a generated radio conversation system. Those are not prerequisites for discovering whether this game is fun.

## Decisions still open

| Decision | Current candidate or tension | Resolve when |
|---|---|---|
| Player control and embodiment | Direct firing with crew assistance; collective station control versus one physical person | Before the crew comparison and transition design |
| Orders and discretion | Strict orders, assignment choice, or priority orders with optional requests | During mission paper design and Stage 1 |
| Radio presentation | Transcribed log versus deliberately fictional paper-feed hardware | Before the physical dispatch interface |
| First repair subsystem | Cooling service assembly or a supported traverse fault | After selected model and simulation inventory |
| Manual assistance | Physical pages plus bookmarks, glossary and optional enlargement | During Stage 2 usability tests |
| Repair duration and repeated handling | Rich first discovery versus rapid familiar work | After repeated-procedure playtests |
| Starting Tiger and historical window | Early configuration with real revisions versus later restoration and customization | Before final part compatibility and campaign content |
| Campaign backbone and continuing goal | Why this crew keeps accepting assignments or continuing the journey; local operations, possibly offensive to defensive | Establish a provisional reason for Stage 3; refine after testing its emotional purpose |
| Map scope | One compact persistent region first; larger geography later | After route and mission variety tests |
| Saving and loss | Tank loss, crew death, abandonment, recovery and retry costs | Before substantial persistent progression |
| Equipment progression | Restoration, documented revisions and bounded fictional adaptations | With the starting configuration and slice reward |
| Difficulty progression | Mission and geographic context versus direct equipment scaling | Compare after one meaningful improvement |
| Session and total duration | FTL-like density and story proportion, with physical work taking time | After measured slice pacing |
| Companion replacement | Persistent named crew, replacements, and the meaning of an empty station | With narrative and loss policy |

## Historical mission reference bank

These cases supply mission pressures and human circumstances. They do not mandate literal reenactments, accepted kill counts or the original formation sizes. Contemporary reports are claims made by participants; later memories are attributed testimony. Adapt the situation to a bounded local job and preserve the distinction between evidence and fiction.

| Account | Order situation and ending | Useful design material |
|---|---|---|
| [Zabel, Tiger I, 10–11 February 1943](https://www.panzer-elmito.org/tanques/tiger_i/informes/tiger_i_19_02_1943_E.html), report 19 February | An attack left observation, controls and other equipment damaged; the crew reported continuing movement afterward | Surviving a fight with degraded functions and an understandable recovery problem; the famous impact count is not a universal durability rule |
| [Lange, 2./502, January 1943](https://www.panzer-elmito.org/tanques/tiger_i/informes/2_PzAbt-502_29_01_1943_E.html), contemporary report | Early operations involved changing assignments, covering withdrawal and difficult recovery | Redirection, protecting others, maintenance under disrupted plans |
| [Großdeutschland, 24–26 August 1943](https://www.panzer-elmito.org/tanques/tiger_i/informes/tiger_i_31_08_1943_E.html), contemporary report | Operational Tigers recovered other vehicles, returned and received further orders; breakdown and combat damage led to more recovery | A small number of functioning vehicles becoming locally important; fatigue and preparation matter |
| [Lueder, Béja, 26 February–2 March 1943](https://www.panzer-elmito.org/tanques/tiger_i/informes/tiger_i_430311_E.html), report 11 March | Mines, poor ground and blocked movement frustrated an attack; recovery failed and vehicles were abandoned | Terrain, route access and recovery limits can decide an operation without a larger tank duel |
| [Bäke group, 24–31 January 1944](https://www.panzer-elmito.org/tanques/tiger_i/informes/Baeke_03_02_1944_E.html), report 3 February | Breakthrough operations involved a damaged crossing, holding positions, linkup and securing supply movement | Connected assignments with different obligations; adapt only a small local portion of this large operation |
| [Richard von Rosen on Goodwood, 18 July 1944](https://erenow.org/ww/goodwood-over-the-battlefield/5.php), later testimony reproduced in Ian Daglish's history | Bombing and disrupted readiness preceded renewed action and withdrawal | Begin with disorientation, damaged equipment and a need to re-establish function; exact damage explanations require caution |
| [Tiger 131 and William Ratcliffe's wartime letter](https://tankmuseum.org/article/tiger-capture-donation), 24 April 1943 | The Tiger was abandoned after disabling damage during the fighting; Ratcliffe's British account describes the exhaustion and relief of the engagement | Pair the machine's mechanical vulnerability with the experience of those facing it; the museum corrects the older capture narrative |
| [Panther driver Müller, Normandy](https://www.panzer-elmito.org/panzertruppe/frente%20invasi%C3%B3n/informes/I-Pz-Rgt-24_180944_E.html), report 18 September 1944 | Road security became withdrawal and escape from a burning tank; the account ends with a gravely wounded commander's farewell instructions | A bounded road task can become a personal crisis; the precise battle date and final outcome are not established here |
| [Panther loader Rudi Becker, Heiderscheid, 23 December 1944](https://www.panzer-elmito.org/panzertruppe/frente%20oeste/informes/Heiderscheid_FGB_E.html), recollection written 1955 | A demonstration and reconnaissance assignment proceeded despite radio coordination concerns; the vehicle burned and only part of the crew escaped | Communications, access and crew relationships can matter before the first shot |
| [Heike, assault-gun unit at Birgel, 14 December 1944](https://www.panzer-elmito.org/ca%C3%B1ones_asalto/stug_iii_iv/informes/StuG-Brig%20902_151244_E.html), next-day report | Attack became withdrawal and counterattack; friendly mines blocked access and crews cleared a route before the remaining forward vehicles returned | An objective changes while local movement becomes the central problem; the report covers StuG/StuH operations, not an identified StuG III in every incident |
| [Jagdpanther unit 563, January 1945](https://www.panzer-elmito.org/cazacarros/jagdpanther/informes/sPzJgAbt-563_02-02-45_E.html), report 2 February | Newly allocated vehicles went into action with little preparation; fuel and recovery failures led to extensive demolitions | Service readiness and support are capabilities; avoid reproducing operational catastrophe as unavoidable punishment |
| [Alfred Rubbel, Panzer IV and Tiger II episodes](https://frontstory.ru/memoirs/germany/alfred-rubbel/), later interview | He recalls a March 1942 forest attack halted by a KV, later found abandoned after becoming stuck; in May 1945 surviving Tiger IIs were abandoned when a weak bridge blocked their route | Uncertain reports and terrain can change apparent strength; the later bridge's precise location is uncertain |

The [crew experience source survey](C:/shop/panzerrechnersystem/docs/research/TIGER_EXPERIENCE_SOURCES_20261028.md) also preserves opposing accounts and the limitations of German retrospective testimony. The [media review](C:/shop/panzerrechnersystem/docs/research/TIGER_MEDIA_REVIEW_20261028.md) records wartime visual references and distinguishes them from modern restoration footage and gameplay captures.

## Game references and supporting documents

| Reference | Mechanic or quality to study | Boundary for this game |
|---|---|---|
| Pacific Drive | Ownership of a persistent vehicle, preparation, damage and recovery | The proposed home is the tank itself; a permanent garage is not required |
| [Outer Wilds](https://store.steampowered.com/app/753640/Outer_Wilds/) | Knowledge that changes what a player notices and attempts | Borrow learning and discovery, not an assumption of its narrative structure or time loop |
| FTL | Dense consequential situations, interacting failures and compact story | Physical repairs and embodied observation require different pacing and information |
| Keep Talking and Nobody Explodes | Symptom recognition, tests and a reference manual | A solo player must be able to consult and act without a second person |
| My Summer Car and My Garage | Mechanical ownership, physical parts and satisfaction when the machine works | Test the repeated workload; meticulous assembly is not automatically the right pacing |
| The Last of Us workbenches | Anticipation and visible physical presentation of an improvement | Installation here should connect to the actual part and capability |
| Gunner HEAT PC and other tank simulators | Clear crew feedback, engagement decisions and understandable results | Translate useful information into this game's silent embodied presentation |
| Steel Armor and Steel Fury | Different military objectives and useful surrounding actors | Keep the local force small enough that the player's Tiger remains consequential |
| UBOAT and Barotrauma | Crew assistance and interacting system failures | Avoid replacing direct play with management menus or adding five sets of chores |
| FAR Lone Sails and Jalopy | A vehicle carrying a journey and attachment through repeated use | Give travel meaningful changes rather than stretching empty distance |

Documented mechanics, primary developer links and the original comparison experiments are in [Tank game mechanics](C:/shop/panzerrechnersystem/docs/research/TANK_GAME_MECHANICS_20261028.md). The expanded repair and journey comparisons are in the earlier [open design discussion](C:/shop/panzerrechnersystem/docs/research/TIGER_SILENT_JOURNEY_PLAN_20261028.md); its fixed-journey assumptions are superseded here.

Further implementation evidence and historical detail remain in:

- [Project playability audit](C:/shop/panzerrechnersystem/docs/research/PROJECT_PLAYABILITY_AUDIT_20261028.md)
- [Game fun review and initial hypotheses](C:/shop/panzerrechnersystem/docs/research/TIGER_GAME_FUN_REVIEW_20261028.md)
- [Solo crew behavior and limitations](C:/shop/panzerrechnersystem/docs/CREW_SOLO.md)
- [Existing damage and repair model](C:/shop/panzerrechnersystem/docs/TIGER_DAMAGE_REPAIR_20260928.md)
- [Repair interactions](C:/shop/panzerrechnersystem/docs/REPAIR_INTERACTION_20260929.md)
- [Historical Tiger modifications](C:/shop/panzerrechnersystem/docs/research/TIGER_I_HISTORICAL_MODIFICATIONS_20261028.md)
- [Inside the Steel Box source survey](C:/shop/panzerrechnersystem/docs/research/inside-the-steel-box-report-source.md)

The next concrete step, if development is authorized, is the human baseline and the smallest contrasting mission test. Broad component preparation follows evidence that the surrounding game gives that work a purpose.
