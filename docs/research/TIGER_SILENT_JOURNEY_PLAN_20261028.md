# The tank as home: open design questions

**Later planning:** the [Tiger game development map](C:/shop/panzerrechnersystem/docs/TIGER_NEXT_STAGE_DEVELOPMENT_MAP_20261028.md) consolidates the subsequent mission and tank-as-base discussion. It reopens this note's fixed westward-journey and finale assumptions; those are no longer requirements for the next stage.

28 October 2026. **Discussion notes, not an approved design or implementation plan.** The owner explicitly wants to continue thinking before deciding what the game should become. No gameplay code was changed for this investigation.

The most promising phrase from the discussion is the owner's: **“The tank is your home. And you are its keeper.”** It connects an already detailed first-person tank interior to a wider experience: a journey, a familiar crew, combat that leaves specific problems, physical diagnosis and repair, and a machine that records the player's work. Whether that combination is enjoyable remains untested. Its novelty and the quality of the model do not establish that it will be fun.

## What changed during the discussion

Before the latest suggestions, the audit emphasized combat agency: clarify what the crew is doing, keep the player responsible for consequential decisions, allow useful work to continue when the player changes stations, and make defeat understandable. It also proposed preparation, excursions and recovery, drawing on Pacific Drive. It had not developed a physical diagnostic manual into the game's central learning mechanism.

The owner's suggestions add a more substantial possibility. The player learns this particular tank by keeping it functioning. Physical improvements and growing knowledge supply progression; the crew and the journey supply reasons to care. Better combat alone would still leave the project short of that experience.

The owner's current ideas, preserved without turning them into decisions:

- No spoken dialogue. Characters, context and a continuing narrative are needed; starting outside the tank is attractive.
- Crew extend access to their stations. A living driver lets the player drive from the cupola. A gunner can receive a rough bearing, while the player takes the sight and shoots.
- Substantial components can be removed, repaired or replaced. Every screw need not become an individual interaction.
- Covers and panels provide physical access and can themselves be damaged or repaired.
- Workshops could provide memorable upgrade opportunities, but later discussion favors allowing repairs wherever the available equipment and circumstances permit.
- Actual projectile damage should determine what needs attention. The player can be inside, beside or some distance from the tank.
- A physical repair manual uses illustrated symptom-based yes/no decision trees. Knowledge should eventually reduce dependence on the book.
- Found diagnostic equipment, spares and selected indicators could become physical additions to the interior.
- FTL is a reference for approximate scope, pacing and the proportion of story to gameplay. This does **not** settle run duration, procedural structure, permadeath or campaign format.

## What the model and current game already provide

The owner verified that the CAD components are separate objects. The conversion preserves the RM5010 complete assembly's 1,481 mesh objects and hierarchy. Its EngineSystem has fourteen named child meshes, including the principal engine forms, filters, pipes and external assemblies. Cooling banks, covers and the front drivetrain housing are distinct. This is a substantial foundation for access and replacement interactions.

That inventory does not yet establish every hidden internal mechanism. No separately named piston or crankshaft was identified in the converted EngineSystem; complete transmission gearing was not confirmed. This qualifies the inventory, not the owner's confirmation that the modeled parts are separate. Source geometry, historically correct assembly relationships, functional simulation and satisfying manipulation are different kinds of readiness.

The renders the owner supplied now live beneath `C:/shop/ts_assets/_archive/_extracted/_Reference_CAD_Tigers/`, following the separately authorized archive operation. The conversion report is [here](C:/shop/ts_assets/_archive/_conversion_report_20261028/README.md). The game source and runtime assets were not replaced by those conversions.

The game also already contains localized component faults, field/workshop distinctions, physical track repair, armor-damage records and hand-stowed salvaged ammunition. For example, `src/tiger_damage.rs` distinguishes a jammed turret ring from a wrecked one, a disturbed sight from a smashed sight, and a leaking recoil system from a sheared mount. `src/track_repair.rs` uses actual repair-link geometry. This proposal would extend and connect existing work, rather than introduce component damage for the first time. The current fault probabilities and repair durations are partly tuning; their existence does not prove the desired recovery experience occurs naturally in ordinary play.

## The possible learning loop

One candidate sequence is:

**Notice a changed behavior → reach a place where investigation is possible → consult or recall the manual → inspect → choose an intervention → perform the work → test the result → continue with a changed machine.**

The pleasure could come from several sources: recognizing a symptom, understanding the mechanism, manipulating a substantial part, hearing the engine run correctly, or knowing an improvised repair carried the crew through the next stretch. Not every hand movement has to be a strategic decision. A well-presented physical ritual can be rewarding in itself. The unresolved issue is how often and for how long the game asks for it.

Knowledge-based progression requires stable causes. If a familiar symptom corresponds to entirely unrelated hidden failures each time, the player cannot become competent. Conversely, multiple simultaneous faults mean a successful repair need not resolve every symptom. The manual should explain a follow-up check, and the game should reveal enough evidence to support it.

The most important distinction is between **uncertainty that can be investigated** and missing feedback. A player wondering which of two plausible components failed has a problem to solve. A player unable to find the interaction point or determine whether a control registered has a usability problem.

## The physical manual

The proposed book has pages, tabs, foldouts, line drawings and concise instructions. It starts from observable problems: engine will not start; engine runs but vehicle will not move; vehicle pulls to one side; turret will not traverse; view through an optic has changed. Each yes/no question identifies the observation or test and illustrates where to perform it.

A conceptual turret decision tree might distinguish whether manual movement remains possible before directing the player to inspect drive components or mechanical obstruction. This is a design example, not an approved Tiger troubleshooting procedure. Every actual branch would need checking against the game's selected configuration and the modeled faults.

The manual supplies knowledge, not hidden-state detection. It should not announce an unseen broken part or silently jump to the right answer. A conclusion can be “inspect this assembly,” rather than a guaranteed diagnosis. Replacement parts can come with supplement pages identifying their differences and compatible fittings. Player bookmarks and notes could accumulate without the printed book changing supernaturally.

Steel Crate's Keep Talking and Nobody Explodes separates the visible device from the information in a printable manual. Its cooperative communication problem differs from solo Tiger play; the useful borrowing is systematic observation connected to a reference. The player here must be able to read and act without a second person. [Developer description](https://keeptalkinggame.com/), [official manual](https://www.bombmanual.com/).

Open usability questions include readable page size within the deliberately abstract visual style, keeping a diagnostic place while looking at the machine, cross-references, and assistance for unfamiliar terminology. A physical book still needs comfortable reading. An optional enlarged page, a bookmark and a concise glossary would preserve the object while improving access.

The first learning situation could present one visible recoverable fault, a relevant bookmark and no immediate lethal pressure. That is teaching through the scene. A book alone cannot teach someone how to find, hold or use the book. Later, the player may solve familiar faults from memory and return to it for an unfamiliar assembly or combination of symptoms.

## Damage that creates different situations

The proposed invariant is: **the component the player sees, the component affected by the hit and the component repaired are one persistent thing.** A removed cover should change access; an installed replacement should remain visible; its condition should affect the associated function. Testing equipment should report the physical quantity it can measure, not reveal all damage through the hull.

There are several kinds of damage worth distinguishing: cosmetic scars, obstruction, leakage, loss of adjustment, deformation, broken components and complete destruction. They support different responses. A lodged fragment can be harmless; removing one can leave the mechanism damaged. The [historical modifications report](TIGER_I_HISTORICAL_MODIFICATIONS_20261028.md) supplies museum observations and wartime maintenance evidence for these distinctions.

Uniqueness need not mean inventing a new repair puzzle for every shot. A finite set of credible failure modes can occur at different locations, with different damaged neighbors, available parts, terrain and enemy pressure. Those combinations can create variety while keeping the machine learnable. A later detailed design would need to establish which visible damage is simulated and which is representational.

Allowing repair anywhere can mean anywhere the necessary access, support, equipment and parts exist. A workshop supplies capabilities, not a magical repair permission. Major assemblies still need convincing handling; portable or found lifting equipment could change what is possible outside a garage. Time compression and one player's control of a team's work would remain deliberate abstractions.

A disabled turret can create a dismounted episode, but should not automatically require clearing every enemy. Withdrawal using remaining mobility, concealment, a temporary intervention, seeking equipment or a later return are alternative game responses. Panzerfaust use belongs to appropriate later-war sections; the current early setting already excludes it. The vulnerability of repair needs actual room for recovery. Unrelenting enemy fire would turn a breakdown into delayed death.

## Crew and control remain open

The leading owner proposal preserves direct shooting. A bearing cue slews the gunner toward a sector; the player then enters the sight and makes the precise shot. The cue should not provide hidden identification, range or perfect tracking. Predictable coarse information is more useful than fresh random error on every key press. `V` is currently the mounted-sidearm binding in `src/main.rs`; the proposed use would require reassignment or another key.

Clicking a living gunner to use his sight presents a useful unresolved embodiment choice. It could mean controlling the crew as a collective, changing perspective to his station. If it literally means the commander moves into that occupied seat, a physical handover would need explanation. Neither interpretation should be assumed merely from the click.

Loss should remove that person's contribution. Without the driver, remote driving is unavailable; without the loader, someone has to perform the loading work; without the gunner, bearing handoff is lost. The gun's mechanical traverse limits should not arbitrarily halve because a crewman died. Physical injury, missing knowledge or workload can affect operation where they are actually represented.

Characters need more than the convenience they provide. Small repeated acts can establish recognition: one man anticipates a tool handoff, another carefully stows the recovered part, another always returns the manual to its familiar place. The act should recur and respond to circumstances. Personal belongings can reinforce that relationship, but cannot create it alone. Care for a person should occasionally be worthwhile without a mechanical reward.

No spoken callouts are proposed. Work animation, glance, gesture, mechanical sound and optional accessibility indicators would carry feedback. A living crew should still leave room for voluntary direct operation and repair; it should not require casualties before the player can enjoy the machine's detail.

## Progression without erasing its own rewards

Three different improvements could coexist: restoration of damaged capability, genuine hardware revisions, and the player's growing knowledge. A complete late Tiger already has several historical improvements; its starting configuration limits which upgrades remain meaningful.

Real changes include the HL210-to-HL230 conversion, steel-tyred wheel sets, optics and mantlet changes, and numerous reliability corrections. Two muzzle-brake forms are documented, but measured performance advantages between them were not established in this research. A sequence of stronger Tiger I barrels, aftermarket performance pistons or superior transmissions is not verified. Fictional variants can be explored honestly without presenting them as historical equipment. [Historical inventory and evidence limits](TIGER_I_HISTORICAL_MODIFICATIONS_20261028.md).

An upgrade should have a perceivable consequence connected to its function. A cooling improvement can change sustained operation; an optic changes observation; stowage changes what can be carried. A muzzle-brake variant should not grant arbitrary projectile damage. A plate patch should not silently restore pristine armor resistance.

Enemies becoming stronger because the player installed a component risks cancelling the reward. An alternative is threat progression tied to geography, scenario and historical period. Improved equipment can make familiar problems easier while opening harder optional routes. Neither approach is selected yet.

The physical manual and diagnostic equipment offer a different progression from raw power. Better access to information can let the player distinguish faults sooner. Indicators should represent real circuits or explicitly fictional instrumentation; a universal red damage light would weaken diagnosis if it simply answers the book's questions.

## What narrative could make the work worthwhile

The existing campaign already has a westward journey through the consequences of the failed advance. Its intended Reichstag last stand and more hopeful optional coda remain documented targets, not decisions reopened or replaced by this memo. The tension is that a long journey of accumulating competence can feel futile if the player's care cannot affect any meaningful outcome. Local victories and the fate of individual people must retain value even in a tragic setting.

Three possible backbones deserve comparison, not immediate selection:

| Possibility | Concrete continuing goal | What repairs serve | Main risk |
|---|---|---|---|
| Escape across a closing route | Reach a particular crossing or remaining corridor with the crew | Keep enough capability to make the next leg and choose detours | A hard clock can punish players for learning the manual |
| Reach a promised rendezvous | Follow an agreed route through several identifiable stops | Carry a familiar machine and people toward a place with personal meaning | Repeated empty destinations can make every hope feel false |
| A commitment to someone else | Bring a specific person, group or essential item through the withdrawal | Preserve capabilities needed for more than the tank's own survival | Escort behavior or an arbitrary passenger can become an obligation without attachment |

These are fictional scenario directions. Their dates, locations and participants would need historical grounding after a direction is chosen. The surrounding world should retain the consequences of the invasion and the experiences of people outside the German crew. Caring about these men need not erase what their army did or reduce opposing crews to anonymous repair-puzzle generators.

An opening outside the tank can communicate four essentials through action: who is travelling together, why the vehicle matters, why remaining is untenable, and which visible route offers a next step. A damaged assembly and a shared task can introduce the crew before combat. A map with a marked destination or a brief written order can state the immediate goal without spoken exposition. The owner's physical manual already establishes that written information is compatible with the concept.

Carry the narrative through recurring people, places and objects. A promised bridge appears on the map, is glimpsed at a distance, and later has a condition that changes the plan. A rescued part has a recognizable donor and remains inside the tank. A crewman's repeated action changes after an injury. The player's own repairs become some of the evidence of the journey; authored events give that evidence context.

## FTL scale and the unresolved pacing problem

FTL explicitly organizes encounters around multiple responses, crew/resource decisions, upgrades, events and possible defeat. Those are useful reference points. Its exact average playtime was not measured here, and the owner's analogy should not be converted into an asserted ninety-minute requirement. [Developer description](https://store.steampowered.com/app/212680/FTL_Faster_Than_Light/).

The useful target may be **a comprehensible journey containing a manageable number of consequential situations**. One stop might provide shelter and a choice of parts; another a blocked route; another an opportunity to help someone; another a fight with damage that matters afterward. Narrative can be compact but recurrent. Every event need not be combat or a breakdown.

Physical work takes more player time than FTL's interface actions. A short repeatable run, detailed repairs, lengthy driving and deep attachment cannot all be maximized at once. We need to understand which experience the owner values most before deciding duration, saving or repetition. Losing a mechanically elaborate project at the end of a run has a different emotional cost from losing a quickly assembled configuration.

The risk is a loop of being hit, doing mandatory work, and being hit again before enjoying the repair. Counterweights could include stretches of dependable operation, optional improvements, successful avoidance, useful maintenance choices and rare memorable major failures. The player should sometimes finish a journey leg and feel the tank worked because they understood it.

## Comparative references added during this discussion

- **My Summer Car:** component-level assembly, engine work and a working vehicle as an earned result. Its developer explicitly targets players who enjoy meticulous building. That supports a potential audience; it does not prove the same workload fits this game. [Developer description](https://store.steampowered.com/app/516750/My_Summer_Car/).
- **My Garage:** separate body panels, suspension parts, engine condition, tools and donor vehicles connect repair to physical behavior. Useful reference for coherent parts and inspection. [Developer description](https://store.steampowered.com/app/1578390/My_Garage/).
- **Jalopy:** caring for a persistent vehicle across a journey, with condition and load affecting travel. Useful for connecting the next route to the current machine. [Developer description](https://store.steampowered.com/app/446020/Jalopy/).
- **The Last of Us Part II:** the publisher's preview documents detailed upgrade animations at workbenches. The proposed borrowing is the satisfying physical presentation and the anticipation of a stop, not a claim that its upgrades are mechanically simulated. [Publisher preview](https://blog.playstation.com/2019/09/26/new-the-last-of-us-part-ii-gameplay-details-revealed/).
- **FAR: Lone Sails:** learning a vehicle while travelling through a story conveyed largely through the environment. Creative lead Don Schmocker describes substantial early testing and gradually expanded functionality to manage complexity. Its camera and authored side-scrolling composition differ from this first-person project. [Developer interview](https://www.gamedeveloper.com/design/telling-a-great-road-trip-tale-without-words-in-i-far-lone-sails-i-), [developer description](https://store.steampowered.com/app/609320/FAR_Lone_Sails/).

## Questions to keep open

The immediate discussion need not produce a feature list. It should clarify what kind of satisfaction we want: diagnosing an unfamiliar failure, becoming fast at familiar work, making a personalized machine, protecting a crew, or completing a difficult journey. Several can coexist, but their relative importance determines the game's rhythm.

Useful next thought exercises would describe one ordinary stop, one memorable failure and one quiet character moment from arrival through departure. We can compare how each feels with and without detailed mechanical work. We should also imagine the tenth repair, not only the first impressive one. A compelling paper scenario would justify a later bounded test; it would not establish enjoyment by itself.

No new model preparation, manual production, simulation expansion or gameplay build is authorized by these notes. The current output is research and a record of possibilities. The owner has explicitly kept the decisions open.
