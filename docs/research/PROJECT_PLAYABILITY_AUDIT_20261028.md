# Project playability and recent work audit

Panzerrechnersystem already has enough systems to support a distinctive tank game. The next priority is to assemble them into a short experience with understandable goals, meaningful command decisions, recoverable trouble and a satisfying conclusion. Adding further simulation detail before testing that experience is unlikely to resolve the owner's central complaint that the game is not fun.

The strongest design hypothesis is that meaningful decisions are distributed unevenly. A healthy solo crew can automate much of the combat, while severe damage can remove the machine and crew before the player can respond. The complicated procedures between those extremes need both clearer feedback and evidence that ordinary encounters actually produce opportunities to use them. Alternative explanations, including control friction, technical instability and a mismatch between the tested mode and the intended game, remain plausible.

The owner's clarification during this review identifies **solo combat and crew control** as the weakest area. Prioritise the commander's information, orders and control of a fight, then the transition into a crew crisis. Campaign structure, platoon play and co-op are secondary to proving that solo loop.

## Scope and baseline

This audit covers the local checkout at `C:/shop/panzerrechnersystem`, branch `raymarching`, HEAD `e5eeb1e`, on 28 October 2026. Findings come from current source, the locally present Canyon mission, recent commits, integration records and existing run telemetry. There was no new human playtest or GPU run in this audit. Source observations identify behaviour and plausible causes; they do not establish how a player feels.

The checkout already contained modifications to `shaders/weapon_particles.wgsl`, `src/drive_bench.rs`, `src/graphics.rs`, `src/main.rs`, `src/raster_optics.rs`, `src/renderer_fine_vfx.rs`, `src/renderer_weapon_particles.rs`, `src/weapon_particle_math.rs` and `src/weapon_vfx.rs`, plus untracked `artifacts/authoring/`. Those files were left untouched. References below identify the inspected working files; they should not be treated as a claim that the release executable was built from every current modification. The gameplay findings principally concern source files outside that dirty set.

The Windows launcher's default play route is Canyon v2, a fresh winter run. This is distinct from the larger three-act campaign. Campaign Acts I and II restore entrance checkpoints after ordinary failure; Act III has a terminal ordinary loss. The implemented campaign still contains Halbe, while the Reichstag finale is a design target. These modes should not be conflated when evaluating the experience. See [launch_windows.ps1:11](C:/shop/panzerrechnersystem/tools/launch_windows.ps1:11), [campaign.rs:1481](C:/shop/panzerrechnersystem/src/campaign.rs:1481) and [PLAYTEST_READY_20260914.md:31](C:/shop/panzerrechnersystem/docs/PLAYTEST_READY_20260914.md:31).

## Recent work that changes the diagnosis

The October work has already addressed several obvious omissions. A proposal to add mission guidance, crew delegation or flanking enemies from scratch would miss what is now implemented.

| Date | Work and commit | Present consequence |
| --- | --- | --- |
| 3 October | Owner playtest and release brainstorm | The owner explicitly said the game felt slow and that adding more Soviets would not solve it. The proposed response was meaningful places, situations and crew stakes. See [RELEASE_VISION_20261003.md:10](C:/shop/panzerrechnersystem/docs/RELEASE_VISION_20261003.md:10). |
| 27 October | Run loop, `52a1806` | Opening brief, result card, one-key restart and seeded clock/weather variation give Canyon a beginning and ending. The old complaint about silently returning to the title is no longer an accurate description of the intended current flow. |
| 27 October | Missions, `0b67330` and `718133a` | Six Canyon beats, written orders, map marks, compass bearings, flares, radio traffic and consequences for delay. The same mission framework also wraps Act One objectives. |
| 27 October | Living solo crew, `be8c7de`; medical procedures, `75475d6` | Five men aboard, AI work at occupied stations, injuries, finite treatment supplies and manual takeover after a death. This supersedes the one-man solo design. |
| 27 October | Integration and pilot fixes, including `83946a7`, `ae6ad8c`, `7ea14f7`, `7804cf8` and `da1789e` | The automated pilot learned to negotiate its own knocked-out enemies, abandon blocked reload attempts and escape hull deadlocks. These changes improve the harness and traversal; they are not evidence of human enjoyment. |
| 28 October | Mission interaction, `96b8114` | Deliberate keys or a click fold the order sheet; M rereads it; Enter clears message lines. |
| 28 October | Crew feedback and routines, `2518830`, `1b3d823`, `2ca91bf`, `8190cf8` and `60cf65b` | Visible casualties, empties rack and hatch disposal, commander target designation, gunner pre-slew and a second turret indicator pointer. |
| 28 October | Enemy tactics, `4efa7fd`; vehicle dynamics, `3ac26f7` and `f554cd8` | T-34s flank, relocate and react; AI orders act through the force-based drivetrain. The tactics and changed acceleration still require combined play assessment. |
| 28 October | Sound, `2eaa76f`; LAN, `3d922dc`, `b4af36a`, `5a73db4` | Room acoustics and contact foley expanded; LAN gained discovery, diagnosis, packaging work and AI in unoccupied host seats. |
| 28 October | Graphics presets, `5e94bb7`; driver repair, `b14b7c5` | A reported 113 ms cabin visibility rebuild was repaired. Integration smoke then exposed and fixed a driver reaction bug affecting continuously changing analog orders. |
| 28 October | Current rendering tip, `aab7529` and `e5eeb1e` | Static/per-frame ray-query scene structures and visibility-atlas redraw changes continue the rendering work. These are technical improvements, not measurements of gameplay quality. |

The integration ledger is the best account of what was actually combined and tested: [INTEGRATION_LOG.md:87](C:/shop/panzerrechnersystem/docs/INTEGRATION_LOG.md:87). Older documents are useful history but sometimes retain superseded behaviour. For example, [DESIGN_PILLARS.md:35](C:/shop/panzerrechnersystem/docs/DESIGN_PILLARS.md:35) still describes one man alone, while [singleplayer_crew.rs:135](C:/shop/panzerrechnersystem/src/singleplayer_crew.rs:135) enables the full crew.

## The current player loop

The normal Canyon session presents a brief and an order, then asks the player to reach the ravine head. The commander can scan, drive through orders to the AI driver, designate a contact, control the gunner's fire order, consult the physical map and compass, or take another station personally. The gunner acquires targets and fires under the standing order; the loader services the breech and handles spent cases. Contact with enemy tanks and guns interrupts the journey. Damage can impair equipment or wound crew, prompting treatment, movement inside the tank or manual station takeover. Reaching the goal or dying leads to the run card and a new attempt.

This is a coherent foundation. The weaknesses lie in how its choices are introduced, connected and rewarded.

### The opening teaches the previous crew model

The current opening brief still says, “You work every post yourself,” at [canyon.rs:356](C:/shop/panzerrechnersystem/src/canyon.rs:356). The mission actually present beside the Canyon workshop says “Crew: you alone” at [mission.v1.json:11](C:/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json:11). Current solo play instead starts with the commander and four AI crew.

The Home controls screen also omits K for fire discipline, Y/N for crew requests and MMB designation from the commander's position. It includes manual loading and extensive cigarette instructions. The new commands are documented in [CONTROLS.md:194](C:/shop/panzerrechnersystem/docs/CONTROLS.md:194), but absent from the actual controls drawing in [ui.rs:2461](C:/shop/panzerrechnersystem/src/ui.rs:2461).

A player can reasonably conclude that the game is behaving unpredictably when crew act on their own after the brief said every post is theirs. Correct the brief, order and help together. Teach three early actions in context: stop behind cover, designate a contact, and release or withhold fire. These are more useful first lessons than a complete inventory of controls.

### The gunner can reduce the value of command

The AI gunner begins with `fire_at_will: true` at [crew_solo.rs:371](C:/shop/panzerrechnersystem/src/crew_solo.rs:371). Its target search considers every active living enemy within 1,800 metres, keeps the previous visible target or chooses the nearest, lays the gun and fires when settled. The implementation is at [crew_solo.rs:840](C:/shop/panzerrechnersystem/src/crew_solo.rs:840).

Visibility does include weather recognition and physical cover, through [game.rs:27315](C:/shop/panzerrechnersystem/src/game.rs:27315). It does not include a gunner optic cone or acquisition dwell in the target-selection predicate. The search therefore does not make observing a contact through the current sight a prerequisite to choosing it.

The resulting design risk is that the player mostly drives while the AI discovers and executes engagements. Taking the sight can become a voluntary replacement for effective automation. The new MMB designation mechanic is valuable, but the surrounding automatic targeting can diminish the reason to use it.

Preserve capable crew. Experiment with a commander who supplies direction and priorities while the gunner reports what he can actually observe, follows designated sectors and waits for a clear standing order. A short encounter with two threats would expose whether designation and fire timing create satisfying decisions. Do not globally weaken the crew merely to generate work for the player.

### Guidance stops during connecting legs

The mission system allows only one active beat. Factory contact waits until 44 percent of route progress and the basin waits until 76 percent. When a preceding beat ends before those thresholds, the next one remains pending. Current map guidance and the compass bearing depend on an active beat, so the connecting leg loses both. See [mission.v1.json:80](C:/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json:80), [mission.rs:679](C:/shop/panzerrechnersystem/src/mission.rs:679), [mission.rs:759](C:/shop/panzerrechnersystem/src/mission.rs:759) and [mission.rs:851](C:/shop/panzerrechnersystem/src/mission.rs:851).

The October 27 automated runs recorded beat 3 ending around 107–123 seconds and beat 4 opening around 680 seconds. A further road leg lasted roughly 6–11 minutes. These are earlier autopilot measurements, preceding the full crew integration, rather than current human timings. The source structure that permits the guidance gap remains present. See [MILESTONE_1_PERF_20261027.md:772](C:/shop/panzerrechnersystem/docs/MILESTONE_1_PERF_20261027.md:772).

Open a transit goal immediately after the preceding objective, while retaining the later trigger for combat escalation. A destination and a reason to approach it should persist even during quiet travel. Quiet can support listening, observation and relief; it needs a purpose and a visible horizon.

### The game sometimes misreports a successful choice

Going around a strongpoint completes a beat through `bypass_fraction`, using the same boolean result as destroying its defenders. The completion response still says “Gun silenced,” “Yard clear,” or “Basin clear.” See [mission.rs:689](C:/shop/panzerrechnersystem/src/mission.rs:689) and the local mission at lines [60](C:/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json:60), [82](C:/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json:82) and [104](C:/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json:104).

Allowing bypass is a strength. Record bypass, destruction and withdrawal separately so the radio can acknowledge what happened and later encounters can respond to surviving defenders. This makes an existing tactical option feel intentional without adding another combat system.

### Recoverable crises need to occur in ordinary play

First aid and manual takeover are substantial implemented systems. Seven treatment procedures, quality, improvisation, triage and finite supplies exist. A dead crewman's post stops working until the player clears the body and performs the job. That is potentially the game's most distinctive source of personal stakes.

The proof demonstrates the procedures under carefully controlled conditions. The Tiger's ammunition was mostly fired off because every tested live lane into a seated man with full stowage set off cartridges. A solid APCR round produced the survivable wound. Gunner and driver deaths were staged because the tested live hits also destroyed the gun/sight/traverse or transmission needed for the following manual takeover. See [CREW_SOLO.md:219](C:/shop/panzerrechnersystem/docs/CREW_SOLO.md:219).

This does not show that all penetrations are inevitably fatal. It does show that a passing procedure proof cannot establish how frequently a normal encounter offers a recoverable injury. Earlier mortal autopilot runs likewise recorded mostly bouncing hits interrupted by terminal events, including a single penetration leading through fuel fire to ammunition cook-off. See [MILESTONE_1_PERF_20261027.md:778](C:/shop/panzerrechnersystem/docs/MILESTONE_1_PERF_20261027.md:778).

Measure the distribution of combat outcomes before expanding the treatment system: wounded survivors with working stations, broken tracks with an intact gun, extinguished fires, successful withdrawals and resumed fighting. If the middle states are rare, investigate ammunition geometry, fragment transport, armour failure, damage propagation and encounter exposure. Any deliberate gameplay departure should be named separately from a physical-model correction.

### Treatment demands more precision than its current presentation supports

The medical procedures include timed wrapping, distinct actions and interruption rules. Abandoning work can spend supplies. However, first-person treatment hands and props, crew breathing and voice audio remain unbuilt; working crew are posed figures. See [CREW_SOLO.md:239](C:/shop/panzerrechnersystem/docs/CREW_SOLO.md:239).

The risk is an input puzzle communicated through text when the intended reward is bodily presence and concern for a person. Compare a simple treatment interaction against the detailed version. Preserve triage, exposure, supply use and time as decisions. Retain the detailed hand procedure only if its feedback and mastery improve the experience enough to justify its interruption of command.

### The result card measures the run better than it teaches the fight

The card now reports time, progress, rounds, kills, hits, penetrations and lost capabilities. The code is at [canyon.rs:291](C:/shop/panzerrechnersystem/src/canyon.rs:291). This is a useful improvement over an unexplained return to the menu.

The next useful feedback is causal: what threat acquired the tank, what exposure permitted the fatal hit, and what capability was lost first. Such explanation can remain on an optional after-action view rather than competing with the first-person scene. It should explain recorded events rather than invent a single correct tactic.

## Enemy behaviour and encounter pacing

The October 28 T-34 work corrected a real design mismatch. Canyon tanks previously used courtyard stealth behaviour written against a person on foot: holding a standoff, stopping wingmen in columns, slow searches and little response to armour or incoming fire. Commit `4efa7fd` added flanking positions, covering partners, relocations, short firing halts and reactions to hits. See [ENEMY_TANK_AI.md:7](C:/shop/panzerrechnersystem/docs/ENEMY_TANK_AI.md:7).

The measurements demonstrate changed behaviour in the opening encounter, not balanced full-run combat. The test Tiger stalled early, and its older pilot could select targets through weather that prevented enemy recognition. More recent integration smoke repaired the steering stall, but does not retroactively turn the earlier comparison into a human test. Do not infer that more lethal flanking alone solves the original complaint.

For humans, the important next question is whether enemy intentions are readable in time to counter them. A flank that produces a visible movement, engine sound, crew report and a decision to turn or withdraw can create tension. A flank understood only after a sudden fatal penetration can create confusion.

The director already includes recovery gates and avoids requesting new reinforcements during tracked contact. Its 90-minute reference adjusts reinforcement credit according to progress; it is not a mandatory run duration. See [canyon_director.rs:10](C:/shop/panzerrechnersystem/src/canyon_director.rs:10) and [canyon_director.rs:99](C:/shop/panzerrechnersystem/src/canyon_director.rs:99). Replacing quiet with more enemies is therefore not the only available tuning lever. Mission layout, threat readability and player decisions deserve priority.

## Verification findings

Round 10 of integration found a driver bug that had survived several merge gates: every analog input change restarted the driver's reaction timer. The Tiger could pivot in place while the log emitted thousands of driver lines. Commit `b14b7c5` corrected the distinction between a changed order and small variations within it. Fresh 300-second runs reached 13.9–17.2 percent progress, with 4–6 kills and no penetrations. This is fixed in the inspected baseline. See [INTEGRATION_LOG.md:99](C:/shop/panzerrechnersystem/docs/INTEGRATION_LOG.md:99).

The same ledger reports 1,995 passing tests, 89 failures and 63 ignored after that fix, with the failure set unchanged. Earlier entries identify missing worktree assets and pre-existing failures. It would be inaccurate to describe the suite as clean, or to treat those aggregate failures as newly introduced gameplay faults. The useful lesson is narrower: unit tests and compilation did not expose a major cross-system control regression until the integrated game ran.

The October 27 performance document explicitly states that the ten-minute human run had not occurred. Its windowed runs were controlled by the automated pilot. The medical proof likewise excludes a human at the controls. Retain those distinctions in every readiness claim. A short human session through the normal launch route should be part of the acceptance gate for changes to crew behaviour, input, mission flow and damage response.

## Three experiments to run next

These are proposed tests, not committed design changes. Their durations and success criteria are practical starting targets rather than historical measurements.

### A commander who changes the fight

Compare the present crew with a guided command variant in the same short encounter. Teach designation and fire discipline; constrain the gunner to plausible observation; present two threats that reward different priorities. Preserve his ability to carry out competent work once directed.

Record who discovered, selected and fired on each contact. Ask the player what their choice changed. The variant succeeds if command creates useful anticipation and decisions without turning every shot into a mandatory confirmation chore. A player who prefers the current automation is evidence to consider, not a failed participant.

### A complete short mission

Build an approximately eight-minute version using the existing hamlet and fork. Correct the brief and controls, keep a transit destination active, offer direct attack and bypass, and return a truthful outcome. Use the existing mission framework, terrain and run card. Avoid requiring new vehicles or a larger map.

Test it with five unfamiliar players. Observe whether they can explain the next goal, make a deliberate route choice, understand why they won or lost and choose whether to replay without coaching. Record time spent lost, time without a meaningful action, requests for help and voluntary replays. Five people is enough to expose major comprehension failures, not to estimate a market-wide retention rate.

### A crisis the player can recover from

Use a short exercise with a clearly disclosed authored nonterminal injury or mobility fault, nearby cover and a credible withdrawal route. Compare simple and detailed treatment interactions. The player should notice the problem, choose a response, regain a capability and continue.

Separately run ordinary combat to measure how often such states occur naturally. An authored injury proves the recovery sequence can be enjoyable; it does not prove the normal damage model generates it. Stop expanding treatment complexity until both questions have answers.

## Alternative explanations to challenge

The claim that automation removes the enjoyable part may be wrong for this game's audience. Tank command can be satisfying precisely because the crew handles the gun while the player reads terrain and coordinates action. Poor guidance, unclear optics or unreliable controls could prevent that command game from becoming visible. Removing automation prematurely would turn a potentially strong command simulation back into one person performing five jobs.

A second possibility is that the user's current build differs materially from the inspected source. The project has moved quickly across several integrations, and the existing executable may not include the latest driver, mission or crew fixes. Record the exact binary/source revision, launch mode, seed and input device for the next human session. Diagnose the version the user actually dislikes.

Finally, a bleak retreat and a short repeatable combat challenge may need different pacing. The larger campaign can justify quiet, uncertainty and long-term attachment that an isolated Canyon run cannot. Establish which promise the short test is trying to deliver, then judge the slice on that promise. The immediate aim is not to make every second busy; it is to make the player's attention and choices matter.
