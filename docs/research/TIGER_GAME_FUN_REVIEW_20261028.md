# Making solo Tiger combat and crew control engaging

28 October 2026. Priority confirmed by the owner: **solo combat and crew control**.

**Planning update after the initial audit:** the owner has also identified the lack of narrative, character attachment and physical tank progression. Spoken dialogue is excluded. The companion [silent journey and workshop proposal](TIGER_SILENT_JOURNEY_PLAN_20261028.md) develops that wider direction and records which earlier recommendations change. This report preserves the initial combat diagnosis; a better fight alone is not sufficient to make the proposed game. All recommendations are plans, with no gameplay implementation authorized in this pass.

The strongest working diagnosis is that the simulation offers more physical detail than the player can consistently turn into meaningful decisions. A healthy AI crew can discover, aim at and shoot targets automatically. A serious hit can abruptly destroy both men and the equipment needed to continue. Between those extremes, the player faces long movement, uncertain control states and demanding procedures. The compelling middle—forming a plan, seeing it work, adapting to a setback and recovering—is insufficiently established.

**The next milestone should be one short, repeatable solo engagement that makes the player feel responsible for the fight.** Use the existing Tiger, living crew, optics, ballistics, map, terrain and damage. First make commands and consequences understandable; then test which decisions belong to the player. Keep substantial new simulation and campaign expansion behind this experiment.

This is a code- and evidence-based diagnosis, reinforced by the owner's dissatisfaction and comparative research. It is not a fresh human playtest of the current build. The recommendations below are testable hypotheses, not a claim that a design document can prove enjoyment.

## Evidence and supporting research

The frozen source baseline is branch `raymarching`, commit `e5eeb1eaccf3ad5f8fe7ecb17c533f7e7d5436ca`, inspected with the existing working changes present. Those changes were left untouched. At that snapshot the history contained 230 commits dated since 1 October; that is a history count, not a measure of feature completion. Other work advanced the checkout during this review, so this is not a claim about the final HEAD.

Four companion reports preserve the detailed evidence:

- [Project and recent work audit](PROJECT_PLAYABILITY_AUDIT_20261028.md): current implementation, source locations, superseded claims and validation limits.
- [Mechanics from fifteen relevant games](TANK_GAME_MECHANICS_20261028.md): primary developer and publisher sources, adaptations, tradeoffs and prototypes.
- [Tiger experience and eighteen historical sources](TIGER_EXPERIENCE_SOURCES_20261028.md): contemporary reports, retrospective testimony and historical boundaries.
- [Photographs, films and the surviving local references](TIGER_MEDIA_REVIEW_20261028.md): six visually inspected wartime photographs, sampled frames from two archival films, and the local media inventory.

## What the recent work has actually achieved

The project has moved well beyond a basic tank simulator. Recent work supplies many of the ingredients that a good short game needs.

| Work | Present value | Remaining design question |
| --- | --- | --- |
| Canyon brief, ending cards, restart flow and seeded variation | Attempts now have a beginning and an end | Does the player understand the current crew model and learn enough to want another attempt? |
| Six authored mission beats, orders, map marks, bearings, flares and radio events | The route can communicate purposes and consequences | Are those purposes continuous, varied and accurately acknowledged? |
| Five living solo crew, target designation, standing fire orders and automatic loading | The player can command an interdependent team | Which valuable decisions remain with the human, and can the human understand what the team is doing? |
| Wounds, treatment, finite supplies and manual station takeover | Loss can change how the machine is operated | Do ordinary encounters produce recoverable problems often enough for these systems to matter? |
| Enemy flanking, movement between shots and reactions to hits | Opponents can create changing situations | Can a player perceive their intentions and respond before the outcome is decided? |
| Force-based vehicle dynamics, suspension, sound propagation and interior lighting | Driving and inhabiting the Tiger have a stronger physical identity | Do handling, sound and visibility provide useful feedback at the moment of a decision? |
| Platoon work, couch co-op and LAN | The machine can support shared responsibility | Does each human role remain interesting rather than merely occupied? |

The October 28 integration log also records an analog steering freeze that survived many unit tests and was discovered by a later run. It was fixed in `b14b7c5`; it is not a current defect. That incident shows why a short session through the normal launcher must accompany technical verification. Passing thousands of assertions does not establish that controls are understandable or a session is enjoyable.

The default Windows experience is Canyon v2. The larger campaign, its implemented Halbe finale and intended Reichstag replacement are different scopes. Changes to the campaign's eventual ending will not repair the immediate solo interaction loop.

## Why solo play may feel unsatisfying

### The player is taught an obsolete job

The Canyon brief says the player works every station alone (`src/canyon.rs:356`), and the loaded mission order still says the crew is the player alone. Current solo code instead supplies five living crewmen. The Home controls overlay omits the new commander designation, fire-discipline and loader-request controls while teaching extensive manual operation.

This is a confirmed usability defect, not a speculative preference. Repair the briefing and in-game reference before interpreting a new player's confusion as dislike of realistic tanks. Teach an action in the situation where it helps: observe a contact, designate a bearing, perceive acknowledgment through action, understand readiness, choose when to fire. The owner's no-dialogue constraint requires visual and mechanical feedback rather than spoken callouts.

### Delegation may remove the decisions as well as the labor

The gunner starts with fire-at-will enabled. His acquisition searches active enemy vehicles around the Tiger, retaining a visible target or selecting the nearest. It respects cover and weather, but the acquisition predicate does not require an optic field of view or a spotting dwell (`src/crew_solo.rs:840`). Once laid and ready, he can fire without the commander.

That may be convenient and enjoyable for a player who wants to drive. It may also make discovering a target, choosing a priority and authorizing a shot feel unnecessary. The right repair is not automatically a worse gunner. Establish whether the player is confused about competent assistance or understands it perfectly and feels redundant.

**Initial experiment proposed before the latest owner feedback:** expose the crew's current order and status while retaining existing competence, then compare command-led target priorities and first-shot authorization. The owner's subsequent preference is more active: the gunner receives a rough bearing, while the player uses the sight and fires. That is now the leading candidate in the companion plan. Autonomous shooting remains a comparison or optional mode to consider, not the recommended default.

### Solo needs both a satisfying commander and a satisfying gunner

Do not assume that everyone wants to spend the whole battle giving orders. The tank's gun is an important source of tactile satisfaction.

Test two coherent ways to own the action within the same crew:

- **At the cupola:** the player discovers and prioritizes contacts, chooses position and exposure, sets fire discipline, and decides when to move or break contact. Crew competence carries out that plan.
- **At the sight:** the player makes the shot—identification, laying, correction and trigger timing—while the other crew continue useful work. A bounded driver order and concise contact reports preserve the rest of the vehicle's activity.

Changing stations should be an intentional change of responsibility. It should not silently invalidate an order, take away the player's target, or make an AI fight the player's inputs. Keep full manual handling available for specialists and emergencies; do not make the player perform it merely to recover agency that automation removed elsewhere.

### Leaving the cupola can cancel the plan

Current documentation states that walking inside halts the tank: WASD becomes bodily movement. On foot, the solo AI stations do not continue fighting through the same authority path. These are concrete limits, not proof that the crew has become incapable. The inspected source paths are `src/crew_solo.rs:953–965` and `src/game.rs:16651–16680`.

The design consequence is substantial. Attending a casualty can mean surrendering the movement decision precisely when it matters most. Prototype one bounded persistent driver order, such as reaching a selected stopping point or reversing to a previously established position. Give it an acknowledgment, visible cancellation and a reason for refusal. Begin with an existing traversable route; open-ended autonomous navigation would enlarge this experiment unnecessarily.

### The recovery systems may live in a narrow gap between safety and death

The existing treatment proof deliberately spent most tank ammunition before introducing wounds, because its tested live shot paths through occupied stations ignited full stowage. Some gunner and driver deaths were staged because live hits also wrecked the station machinery that the proof needed to demonstrate. This validates procedure execution, not the prevalence of a playable recovery situation (`docs/CREW_SOLO.md:219`).

Before expanding medicine, measure the natural outcome distribution: stopped hits, damaged but usable components, wounded crew with working equipment, controllable fires, successful withdrawals and immediate terminal events. A disclosed training scenario can separately test whether recovery is enjoyable when it is possible.

Preserve severe vulnerability while finding a useful range of consequences. Prefer clear information, terrain that permits withdrawal, shorter exposure and different encounter composition before modifying physical constants. If a deliberately more forgiving damage option proves necessary, label it as a game setting rather than a historical correction.

### Complex procedures need perceptible state and a reason to interrupt them

The treatment system has distinct sequences, timing, quality, supply consumption and interruption costs. Its first-person hands and props, and the crew's breathing and voices, remain incomplete. This can produce a procedural input puzzle where the intended physical and emotional feedback is missing.

The decision worth preserving is whom to help, where to stop, what resource to spend and what capability the player is willing to lose while helping. Compare the detailed procedure with a shorter assisted interaction after that decision. If the detailed version makes players more absorbed, keep it. If it mostly delays return to meaningful play, compress it.

### Travel and failure can prevent learning

Objectives now exist, but pending route triggers leave intervals without a current objective or bearing. Older instrumented autopilot runs recorded lengthy connecting legs. Those timings predate the latest crew integration and should not be presented as current human measurements; the mission structure still warrants a direct test.

Likewise, the existing after-action card and replay presentation are useful foundations. Extend them instead of inventing another disconnected interface. Explain the important causal event, distinguish a bypass from a destroyed strongpoint, and offer **retry the same situation** alongside a new attempt. Repeating a long drive is not necessary to preserve the consequence of a lost fight.

## What history contributes to fun

The best historical material reinforces competent cooperation. The May 1943 Tiger experience compilation describes dependence on infantry, communication and maintenance support. Lange's January 1944 report describes tanks holding weakened infantry together and the burden of continuous duties. Zabel's February 1943 account describes surviving while losing observation, communication and other capabilities. These support protection, withdrawal and recovery as meaningful mission outcomes. [May 1943 compilation](https://www.panzer-elmito.org/tanques/tiger_i/informes/tiger_i_28_05_1943_E.html), [Lange](https://www.panzer-elmito.org/tanques/tiger_i/informes/tiger_i_15_01_1944_E.html), [Zabel](https://www.panzer-elmito.org/tanques/tiger_i/informes/tiger_i_19_02_1943_E.html).

The opposing accounts are equally useful. Eroshenko recalls an enemy tank withdrawing without knowing whether his second shot penetrated. Stychinsky describes seeking a better view through smoke and fog at personal risk. The experience is partial information followed by action and interpretation. That is a richer design target than either perfect target labels or continuous helplessness. Their vehicle identifications remain retrospective claims. [Eroshenko](https://iremember.ru/memoirs/tankisti/eroshenko-vladimir-mikhaylovich/), [Stychinsky](https://iremember.ru/memoirs/samokhodchiki/stichinskiy-sergey-aleksandrovich/).

Authenticity therefore does not require a solo player to do five people's routine work. It can mean relying on capable specialists, understanding their limitations and taking responsibility for the shared plan. Let enemies protect themselves and disengage too. A retreat, a passage opened or a crew saved can be a readable success without every engagement ending in a burning wreck.

The fictional Tiger breakout from Stalingrad is already identified as alternate history in `GAME_DESIGN.md`. Keep that honest boundary. Creative freedom in scenario composition and pacing need not be disguised as a claim about real operations.

## The mechanics worth adapting first

| Reference | Adaptation for this game | Boundary |
| --- | --- | --- |
| [Gunner HEAT PC](https://gunnerheatpc.com/news/articles/getting-started-in-ghpc) | Crew acknowledgment, target handoff and understandable readiness; useful after-action information | Reports should reflect what crew can observe, not reveal every hidden enemy state |
| [UBOAT](https://store.steampowered.com/app/494840/UBOAT/) | Routine work delegated to a functioning team, with personal intervention when useful | Avoid multiplying maintenance bars or making the crew deliberately unreliable |
| [IL-2 Tank Crew](https://il2sturmovik.com/news/846/dev-blog-360/) | Different mission verbs and selectable distance to the initial action | Preserve slow driving where terrain or anticipation supplies an actual choice |
| [Into the Breach](https://media.gdcvault.com/gdc2019/presentations/Into%20the%20Breach%20Postmortem%20Final.pdf) | Small situations with readable stakes and several useful responses | Translate telegraphing into observable behavior, rather than exact future attack arrows |
| [Pacific Drive](https://blog.playstation.com/?p=386881) | Preparation, dangerous excursion and relief; damage informs the next route | Temporary shelters suit the westward journey better than an obligatory permanent garage |
| [Left 4 Dead](https://steamcdn-a.akamaihd.net/apps/valve/2009/ai_systems_of_l4d_mike_booth.pdf) | Separate the pacing of pressure and recovery from weapon difficulty | Hold reserves or compose encounters; do not secretly change a shell already in flight |

Receiver 2 supplies a useful counterexample to indiscriminate simplification: detailed manipulation can be the satisfying skill when the entire experience teaches it and rewards mastery. The question is which manipulations deserve that prominence here. The full comparison report also considers Steel Fury, Steel Armor, War Thunder, World of Tanks, Squad 44, FTL, Barotrauma and SnowRunner.

## A concrete short mission to test

Use an existing hamlet and its approach, with a nearby rally point. Target **six to ten minutes** for the prototype, as a proposed tuning range rather than a claimed optimum. Keep the world, enemy seed and weather fixed during comparisons.

| Beat | Player's decision | Crew contribution | Evidence that the beat works |
| --- | --- | --- | --- |
| Orientation | Choose cupola command or direct gunnery; identify the rally point | Visible station work establishes who does what | Player can explain the goal and their own responsibility |
| Approach | Select the exposed short route or a sheltered approach | Driver acknowledges; observers report only credible contacts | The route choice changes what the player can see or risk |
| First contact | Choose a priority and whether to engage now | Gunner confirms target/readiness; loader handles the routine cycle | Player connects an order or shot to an observable outcome |
| Complication | Continue, withdraw or address a recoverable problem | An interrupted action makes the lost capability perceptible; others keep working | More than one response remains viable |
| Resolution | Reach the rally point with the state the player chose to accept | Crew behavior reflects survival, damage and relief | Success is clear without requiring every enemy to die |
| Review | Retry the same situation or choose a fresh attempt | Brief causal report identifies one useful lesson | Player wants another attempt and knows what to change |

For the first control study, use a disclosed staged complication if needed; keep that separate from tests of natural combat damage. Once the solo loop works, adapt the same space to protecting a companion's withdrawal. That adds responsibility for others without making a full campaign the prerequisite for learning anything.

## Prototype order and decision gates

**Prerequisite for every comparison:** freeze the executable revision, seed, starting state and input device; establish a development checkpoint before contact; verify basic controls and performance. This makes the first three experiments repeatable before player-facing retry work begins.

1. **Correct the player's model of the crew.** Update the brief and Home controls; expose current orders, acknowledgments, readiness and refusal reasons in one repeatable encounter. Use crew movement, mechanical sounds, bearing/readiness cues and optional accessibility indicators. Do not add spoken dialogue or simultaneously retune enemy lethality.
2. **Test agency without sacrificing competence.** Compare the corrected baseline with the owner's rough-bearing handoff and direct player gunnery. Record who discovered, selected and fired at targets. A player who understands the crew but feels redundant needs a different fix from a player who cannot tell why it stopped. An indirect commander variant remains a secondary comparison.
3. **Let a plan survive an interior intervention.** Add one persistent movement order and test it during a recoverable crisis. Then compare assisted and detailed treatment separately. This isolates whether movement authority, procedure friction or both cause frustration.
4. **Shorten the learning cycle.** Expose same-seed retry to players; extend the current report with a causal explanation and truthful mission outcomes. Keep campaign and Ironman rules separate from the controlled test setup above.
5. **Compose pressure and relief.** Extend and tune the existing canyon director, which already gates reinforcements during contact and provides recovery intervals. Tune the short mission's approach, engagement, withdrawal opportunity and protected finish; consider casualty workload when delaying new pressure. Quiet should allow planning, observation or emotional contrast, rather than serve as empty padding.

Each gate should use the owner's play plus several unfamiliar players. Five unfamiliar participants are a practical first usability pass, not a statistically conclusive enjoyment study. Alternate the order of variants to reduce familiarity effects. Record short sessions with consent, without explaining controls during the attempt unless the player is stuck.

Capture time to first understood decision, unexplained waiting, misunderstood orders, voluntary station changes, reasons for firing, recovery attempts, the player's explanation of defeat and voluntary replay. Ask what they were trying to do at the most memorable moment. Kill counts and completion times remain useful context, but cannot stand in for involvement.

Provisional targets: a comprehensible choice within the first minute; no repeated unexplained command failure; most participants can name a decision that changed the fight; most can explain a defeat; and at least three of five voluntarily choose another short attempt. These are adjustable decision gates for this prototype, not external industry benchmarks. If feedback alone makes the baseline compelling, keep that result and avoid unnecessary restrictions on AI initiative.

## Where creative freedom has the greatest value

Use freedom first in the distance between decisions, clarity of information, persistence of orders, assistance for repetitive gestures, encounter objectives and cost of retrying. Optional command-time slowdown is worth a separate accessibility and solo-control experiment after the basic interface is understandable. It need not affect co-op or become the default.

Keep the Tiger's characteristic inertia, restricted viewpoints, ammunition identity, crew dependence and dangerous mistakes. These give the game its identity. Revisit their presentation when they obscure a decision; change physical behavior deliberately only when a measured design benefit justifies the tradeoff.

A tragic campaign can still contain earned local victories. A successful withdrawal, a rescued crewman or a well-executed engagement should matter even if the larger war and intended finale remain hopeless. Without that contrast, the player can learn that their competence changes nothing.

The initial audit recommended postponing a broad progression economy while proving the combat loop. The owner's later feedback changes the scope of that proof: it must also demonstrate attachment, a physical improvement and a reason to continue the journey. The companion proposal therefore frames one repair, one relationship and one encounter together, before multiplying procedures, vehicles or upgrades. Planning remains separate from authorization to build.
