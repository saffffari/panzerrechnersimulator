# Tank game mechanics and experiments for Panzerrechnersystem

Research and design assessment, 28 October 2026.

**Later owner direction:** no spoken dialogue; crew should extend the player's control, with a rough-bearing handoff and direct player shooting as the leading candidate. A silent narrative and hands-on vehicle progression also need a place in the core loop. See [the follow-up plan](TIGER_SILENT_JOURNEY_PLAN_20261028.md). The original command-first diagnosis below remains a comparison hypothesis, not a settled design decision.

The strongest direction is a commander game inside a vulnerable machine: observe, choose, order, see a consequence, recover, and choose again. The owner identified **solo combat and crew control** as the weakest area, so command authority, crew feedback and recoverable combat take priority over campaign expansion or multiplayer features. Panzerrechnersystem already has much of the simulation needed. The design risk is that it alternates between automatic combat, lengthy routine handling, and catastrophic loss without enough understandable decisions between them. This is a hypothesis to test with people, not a conclusion established by automated completion or screenshots.

The comparisons below use developer publications, designer presentations, manuals and publisher descriptions. **Documented mechanics** describe those games; **adaptation** and the explanation of its likely value are design judgments for this project. The objective is to choose a few experiments, rather than accumulate every attractive feature.

## Current project constraints

The October 27–28 solo crew work already provides five living crewmen, autonomous loading, target designation, fire orders, wounds, and manual station takeover. These are foundations to improve, not missing features to propose again. The gunner defaults to fire at will; designation offers the commander more control. The important question is whether the player feels responsible for choosing an engagement. See [CREW_SOLO.md](../CREW_SOLO.md), especially the opening implementation description, and `src/crew_solo.rs`, `CrewSolo::default`.

Leaving the command position to walk inside currently halts the driver. Attending a casualty can therefore remove the player's ability to influence the vehicle's movement at precisely the moment it matters. A persistent, bounded driver order is a promising extension of delegation. See [CREW_SOLO.md](../CREW_SOLO.md), particularly its remaining-work discussion.

The [October 3 release proposal](../RELEASE_VISION_20261003.md) already identified missing reasons to stop, choose, or care along the canyon. Its exact enemy timing is historical evidence, not a statement of current balance. The [October 27 run-loop audit](../RUN_LOOP_20261027.md) verified important pieces but lacked a human native-window run and full-course armed completion in that pass. Its one-person-crew description is superseded by later crew work; the later crew document also records a reload-stall fix.

The largest design opportunity is consequently to organize the existing systems into a readable, satisfying sequence. More procedures, vehicles, or visual effects cannot by themselves establish that sequence.

## Tank and combined arms references

### Gunner HEAT PC

**Documented mechanics.** The developer emphasizes unified controls, contextual access to minor functions, realistic vehicle characteristics, and crew speech tied to actions. The current guide describes commander-to-gunner aiming, ammunition transfer with operational costs, and an after-action review that explains individual shots. Its older design article also contains future intentions; those should not all be treated as shipped features. [Design principles](https://gunnerheatpc.com/news/articles/dev-log-number-2-why-we-fight), [current guide](https://gunnerheatpc.com/news/articles/getting-started-in-ghpc).

**Adaptation.** Complete the information loop around the existing target designation: order acknowledged, turret moving, target lost, loaded, ready, fired, observed result. Translate GHPC's useful information into visible crew work, mechanical sounds, physical indicators and optional accessibility cues; the owner's game excludes spoken dialogue. Prototype one encounter in which the player can perceive these stages without conversational subtitles. Keep enemy damage uncertain when nobody can observe it. An optional post-run reconstruction can reveal more without changing live combat.

### Steel Fury Kharkov 1942

**Documented mechanics.** Publisher material describes large combined-arms missions, route freedom, command of participating units, and several playable crew positions. [Publisher description](https://store.steampowered.com/app/1351450/Steel_Fury_Kharkov_1942/).

**Adaptation.** Let the Tiger participate in an event with actors and purposes beyond itself. A nearby friendly group can make success concrete: they cross, withdraw, or remain trapped because of what the player does. Prototype one short scene with a visible before-and-after consequence. The useful borrowing is a shared objective, not the scope of an entire commandable army. The Tiger should remain dependent on information and activity outside its compartment.

### Steel Armor Blaze of War

**Documented mechanics.** Graviteam lists reconnaissance, holding locations, infantry support, ambush and breakout among its objectives. The game also exposes damage and shot vectors in after-battle statistics. [Developer description](https://graviteam.com/games/SABOW.html).

**Adaptation.** Vary the required action and the victory condition. Passage, delay, supply recovery, and preserving a capability can matter without destroying every enemy. Reuse one terrain area for three short mission variants before building more geography. Each needs an observable completion condition. A damaged Tiger reaching shelter can constitute a satisfying result even when several enemies remain active.

### IL-2 Tank Crew

**Documented mechanics.** Developer blog 360 announces Ground Forces Advanced Quick Missions for the then-upcoming 5.203 update, with strongpoint defense, attack, breakthrough and rear-area attack. The announced design allows initial enemy distance to be selected explicitly because ground vehicles move slowly. This source establishes developer intent, not independent verification of the released feature. [Developer blog dated 5 April 2024](https://il2sturmovik.com/news/846/dev-blog-360/).

**Adaptation.** Separate approach duration from the tank's physical speed. Start a short scenario one meaningful terrain decision away from contact. Preserve long approaches where navigation, anticipation or preparation makes them worthwhile. Prototype configurable starting distance on an existing encounter; do not alter the Tiger's movement model to compensate for excessive empty space. This also provides a repeatable learning exercise for new players.

### War Thunder

**Documented mechanics.** Gaijin's fatal-shot X-ray camera explains where a projectile entered, which components were damaged, and how the crew was affected. The developer explicitly frames it as a way to understand the damage model. [Developer explanation](https://warthunder.com/en/news/3880--en).

**Adaptation.** Make a loss teach something. Extend the existing run card with incoming direction, struck plate, failed capability, and the last relevant player action. Begin with a simple annotated sketch using impact data; a cinematic replay is unnecessary. Place omniscient reconstruction outside the live battle, and make it optional. Showing the physical cause is preferable to a generic hint that guesses what the player should have done.

### World of Tanks

**Documented mechanics.** Wargaming describes distinct vehicle roles, a pace intended to leave room for tactical thought, and staged beginner learning through Bootcamp and Tank Academy. [Publisher description](https://store.steampowered.com/app/1407200/World_of_Tanks/).

**Adaptation.** Give enemies recognizable jobs and teach one concept at a time. Behavior and position can create variety before adding models. Prototype three brief exercises: notice a threat before exposure, change position after a shot, and finish an objective without eliminating everyone. The attractive element is readable specialization. Its vehicle grind, tier progression and broad collection of machines do not fit a journey with one persistent Tiger.

### Squad 44

**Documented mechanics.** Crews divide driver, gunner and commander work. Infantry, logistics, fortifications, and local or squad communication support the wider battle. [Publisher description](https://store.steampowered.com/app/736220/Squad_44/).

**Adaptation.** Give roles different information and decisions, not merely different controls. In a two-person test, the commander can locate an obscured threat while the driver negotiates a consequential route. Both need useful work during quiet periods. One friendly report tied to a landmark can connect the tank to the surrounding battle. Solo play needs meaningful simulated information exchange; multiplayer's social energy will not emerge simply by assigning the same jobs to silent bots.

## References for delegation recovery and pacing

### UBOAT

**Documented mechanics.** The player controls the boat through its crew, manages damage and supplies, delegates work, and can personally operate stations. The publisher distinguishes learning complex matters from the ability to begin playing effectively. [Developer and publisher description](https://store.steampowered.com/app/494840/UBOAT/).

**Adaptation.** Delegate routine competence and reserve personal intervention for a meaningful exception, preference, or crisis. The new living crew already supports this direction. Prototype one persistent driver order that remains active while the commander moves inside, with explicit cancellation and failure feedback. Avoid expanding the crew into five separate maintenance bars. A healthy specialist should normally be an asset the player can trust.

### FTL

**Documented mechanics.** Crew orders, power priorities, targeted attacks, limited resources, and optional combat pause offer competing responses to crises. Its developer description presents escape, additional firepower, and boarding as different responses to disabled defenses. [Developer description](https://store.steampowered.com/app/212680/FTL_Faster_Than_Light/).

**Adaptation.** A damage event should change the decision landscape. Saving a crewman, keeping the gun active, and escaping should sometimes compete, with more than one viable response. Prototype a disclosed test scenario with two recoverable problems. Test an optional order-time slowdown separately if comprehension remains poor. FTL's omniscient view and replaceable personnel conflict with embodied observation and a fixed crew; borrow competing priorities rather than its whole interface.

### Barotrauma

**Documented mechanics.** Interacting onboard systems, specialized roles, bots, medical work, and external missions support cooperative survival. [Publisher description](https://store.steampowered.com/app/602960/Barotrauma/).

**Adaptation.** Let failures propagate through a small set of understandable dependencies. A damaged station should create coordination and alternatives, rather than a queue of unrelated minigames. Prototype a temporary workaround versus a protected repair while another crewman remains useful. Its extensive crafting, talent and medical systems are poor scope targets here. The Tiger already has enough ways to fail; those failures need legibility and room for response.

### Into the Breach

**Documented mechanics.** Telegraphed attacks and deterministic player turns support short defensive battles. Matthew Davis's postmortem describes readability, limited menus, interesting choices and minimizing wasted time as constraints. It also documents removing complex but shallow strategic systems. [Designer postmortem, especially slides 7, 13–21 and 38–44](https://media.gdcvault.com/gdc2019/presentations/Into%20the%20Breach%20Postmortem%20Final.pdf).

**Adaptation.** Telegraph credible intention rather than exact outcomes: a turret searching, infantry taking cover, a vehicle reversing, or a reported contact. Prototype one dangerous transition that the player can perceive and influence, plus an objective completed by delay or passage. Literal attack arrows would undermine uncertain first-person observation. The deeper lesson is to simplify rules until a player can make an informed choice, then build encounters around that choice.

### SnowRunner

**Documented mechanics.** Terrain responds to vehicle weight, speed and momentum. Environments include mud, water and snow, with equipment suited to different conditions. [Publisher description](https://store.steampowered.com/app/1465360/SnowRunner/).

**Adaptation.** Driving becomes engaging when terrain presents a decision and a legible consequence. Prototype a fork between an exposed firm route and a sheltered route with a visible mobility cost. At the game's display resolution, the player must be able to read those surfaces before committing. Slow movement is compatible with engagement when it contains judgment. Constant immobilization would turn that judgment into a repetitive punishment.

### Pacific Drive

**Documented mechanics.** Ironwood describes a cycle of repairing, preparing, selecting a route, undertaking a dangerous excursion, and returning. The route planner supplies conditions and resource information that can influence preparation. [Ironwood's gameplay-loop explanation](https://blog.playstation.com/?p=386881).

**Adaptation.** Give scavenging a destination and damage a recovery horizon. Prototype one sheltered halt between encounters where the player inspects losses, chooses one worthwhile preparation, then selects the next route. A permanent home garage conflicts with the westward journey; temporary shelter fits. Recovery is valuable because it changes the next plan, not because every halt contains a long checklist of repairs.

### Receiver 2

**Documented mechanics.** Detailed firearm operation and malfunction handling form the central skill. Enemies use physical damage and can be defeated with a single well-placed shot. [Developer description](https://store.steampowered.com/app/1129310/Receiver_2/).

**Adaptation.** Manual handling can be compelling when mastery is central and mechanism state is visible. Preserve the breech as a learnable specialist experience and casualty fallback. Compare manual and competent AI loading inside the same encounter. Avoid requiring equally elaborate manipulation for every peripheral object. The test is whether handling produces anticipation and satisfying improvement, or merely delays the decision the player wants to make.

### Left 4 Dead

**Documented mechanics.** Valve's director estimates intensity and creates peaks and recovery intervals. Active combat must subside before a recovery interval begins. The paper explicitly distinguishes changing pacing from changing difficulty. [Valve's AI paper, slides 77–91](https://steamcdn-a.akamaihd.net/apps/valve/2009/ai_systems_of_l4d_mike_booth.pdf).

**Adaptation.** Govern new pressure and recovery opportunities without secretly altering shell or armor performance. Extend and tune the existing `canyon_director.rs`, which already gates reinforcements during contact and provides recovery intervals; consider recent hits and incapacitated-crew workload where the current pacing does not account for them. Delay a reserve's arrival rather than remove a visible enemy. The project needs both pressure and relief; endlessly introducing enemies because the player is surviving defeats that purpose. Quiet becomes meaningful when the player has a reason to use it.

## Ranked prototype matrix

These are bounded experiments. Test them separately before combining them, so an improvement can be attributed to a specific change. Durations below are proposed scenario targets, not measured implementation estimates.

| Rank | Experiment | Minimum change | What it tests | Main risk |
| --- | --- | --- | --- | --- |
| 1 | Complete command feedback | Acknowledgment, progress, obstruction and result for designation and firing in one repeatable engagement | Whether solo players understand and own an engagement | Excess chatter or knowledge the crew could not possess |
| 2 | Delegation during crisis | One bounded movement order remains active while the commander attends the interior | Whether casualty care creates prioritization while the vehicle remains controllable | Autonomous movement surprising the player |
| 3 | One complete short mission | An eight-to-twelve-minute solo slice using existing terrain and systems, with a route choice, optional useful stop, and a final objective | Whether the improved command loop sustains interest beyond one fight | More scenery without actual choices |
| 4 | Learnable defeat | Causal run-card explanation, same-seed retry, encounter checkpoint for development tests | Whether losses produce understanding and another attempt | Reducing stakes before knowing whether recovery is the actual problem |
| 5 | Consequential recovery | One sheltered halt with limited preparation and a visible next-leg tradeoff | Whether damage and supplies create anticipation rather than chores | Turning relief into maintenance overhead |

### The first combat experiment

Use a repeatable engagement with two visible priorities and a protected withdrawal option. The commander should decide which contact matters, designate it, understand what the gunner is doing, authorize or withhold fire, and change the movement plan after seeing a result. The healthy driver and loader should execute routine work without constant correction. The player can take a station personally when precision or preference warrants it.

Compare the baseline with a version that adds only complete order feedback. Count mistaken reissued commands, unexplained non-responses and involuntary station switching. Ask what the player believed each crewman was doing at the critical moment. If the player cannot tell, additional simulation is unlikely to solve the immediate control problem. If the player understands everything but feels passive, test fire authorization and target priority next. These are different failure modes and need different changes.

### The first mission

Start one meaningful terrain decision from a visible problem. Within the first minute, the player should understand what they are attempting and have an alternative to consider. Give the middle of the mission an optional stop whose reward answers a concrete shortage. Finish with passage, protection or withdrawal rather than compulsory extermination. Arriving at shelter should visibly change the state of the crew or the surrounding group.

Use existing units, fuel, damage and crew systems. Avoid adding a major new subsystem for this experiment. A smaller slice also reduces the cost of trying different information, travel and enemy arrangements. If it remains unengaging, the diagnosis is much easier than it would be across an entire campaign.

### Command and treatment variants

Compare hold-fire and fire-at-will defaults on the same encounter. Authorizing the first shot may create ownership, but it may also be a redundant button. Preserve autonomous fire as a deliberate standing order rather than assuming one default suits every situation.

Test casualty treatment after persistent driving works. Otherwise the treatment trial also tests an unintended loss of movement authority. A shorter assisted treatment option is worth a separate comparison with the detailed procedure. The relevant outcome is whether treatment produces tense prioritization and attachment, not how many steps it contains. Preserve the owner's established manual-care direction unless a tested alternative is subsequently chosen.

## Human playtest observations

Use the same small seed set with several new players and the owner. Alternate version order where practical. Automated traces can reveal state changes and stalls; voluntary replay and a player's explanation of a decision require people.

Record time until the player can explain the immediate goal and available choice; time spent waiting or searching for an interaction; misunderstood orders; whether damage led to a new plan; whether the player can explain the loss; and whether they voluntarily start another attempt. Ask which moment they would describe to someone else. Distinguish intentional watchful waiting from confusion and inactivity.

Candidate acceptance targets are a meaningful choice in the first minute, most players explaining a loss, no repeated unexplained station failure, and a majority voluntarily retrying the short mission. These are proposed local targets, not industry benchmarks. More clicks, explosions, or successful completions are not substitutes for enjoyment.

## Design boundaries

Keep the Tiger's characteristic limitations, destructive vulnerability, crew interdependence, interior and sensory confinement. Exercise creative freedom first in encounter spacing, mission structure, information presentation, control assistance, recovery opportunities and retry design. These can improve the experience without inventing armor performance.

Prioritize GHPC for communication and learning, UBOAT for delegation, and Into the Breach for disciplined decision design. Use Left 4 Dead's pacing distinction to protect both tension and recovery. Once solo combat is satisfying, Pacific Drive can inform journey rhythm while Steel Armor and Tank Crew provide mission variety compatible with serious tank simulation. The next commitment should be a tested solo engagement followed by a short mission, not a larger feature list.
