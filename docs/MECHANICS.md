# Kessel — MECHANICS BACKLOG (judged ideation, 2026-07-01)

> 48 candidates from a 4-lens ideation panel (simulation / historical / emotional / systemic),
> scored by a design judge against the locked pillars (sim-first, diegetic, no-HUD-clutter,
> tanks-only, sound-forward). Source data: `docs/research/data/mechanics.json`.
> These are PROPOSALS for the user to accept/reject — nothing here is locked.

## Ranked survivors

### 1. Counter-battery in pencil (getting shot is a sensor)  — 9.5/10  (simulation lens, cost M)
A concealed gun's hull CLANG is instant but its report arrives range/343s later from a bearing; count the gap, take the bearing, pencil a range ray on the paper map — two hits and the rays cross on the ambusher. Being shot at becomes reconnaissance.
*Judge:* The purest single expression of every pillar at once: sound is the sensor, the paper map is the fire-control instrument, ballistics is the truth, and there is zero new HUD. ImpactEvent.flight_time/range_m are already carried through game.rs's ScheduledSound queue, so the only real cost is the pencil layer the locked design owes anyway — ship it alongside Klang-Bibliothek so the delayed report is identifiable as well as rangeable.

### 2. Klang-Bibliothek (the ear as threat library)  — 9.4/10  (simulation lens, cost S)
Every incoming round synthesizes its CLANG from caliber, mass, velocity and panel, so a 76mm bounce, an 85mm crack and a 100mm slam are physically different sounds — the player learns to identify what is shooting at them blind and re-orients the tank off one ring tone.
*Judge:* synth_impact is parametric and unit-tested today, so wiring real enemy ShellSpecs through it converts the sound-carries-85% pillar into learnable threat literacy no tank game has ever shipped — the cheapest deep-knowledge system on the list. No tweak needed; just never let a canned SFX leak back into the impact path.

### 3. Reading the kill (no hit markers, ever)  — 9.2/10  (simulation lens, cost S)
A marginal pen gives a choked clang and a wisp of smoke; an overwhelming pen gives the muffled interior crump, hatches, then the delayed cook-off you learn to wait three seconds for. Firing a confirming second round costs a reload you may need for his platoon-mate.
*Judge:* Deletes the hit marker by grading enemy death presentation off behind_armor/residual_v through wreck/cook-off FX that already exist — kill confirmation becomes an ear skill that costs a reload to shortcut, which is the no-HUD and manual-reload pillars reinforcing each other at S cost. The three-second cook-off wait is the signature beat; tune its delay to be learnable, not random.

### 4. Silence Gradient  — 9.1/10  (emotional lens, cost S)
Act 1 trains the player that silence means being hunted; approaching the home village, the war ambience layers fade one by one until there is only wind and a loose shutter banging. Arriving home is mechanically indistinguishable from an ambush being set — and the ambush never comes.
*Judge:* Weaponizes ten hours of stealth conditioning using nothing but the tested LayerState/Ambience crossfades on distance booleans — the Act-2 gut-punch delivered entirely by the audio system, exactly what the 85%-sound pillar promises, at near-zero cost. One guard: scripting must guarantee no patrol can wander into the silence radius, or the non-ambush reads as luck instead of grief.

### 5. The loader's dilemma (shell sommelier)  — 9/10  (simulation lens, cost M)
PzGr.40 out-pens APCBC point-blank, loses past ~1.5km, and hates sloped glacis — and it's loot-scarce. Since you ARE the loader, every reload is a bet: APCBC for the flankers, the hoarded APCR for the heavy, HE for the wall he hides behind — and swapping a chambered round costs a full cycle while tracks squeak closer.
*Judge:* Crew-of-one plus the already-tested APCR/APCBC/HE crossover physics makes every manual reload a wager on the next eight seconds and shell knowledge the game's only progression system — sim-first as a habit loop, zero UI. Absorbs the cut Tungsten-ration idea for free: make APCR near-absent from loot tables and each round becomes a story.

### 6. Angling doctrine (driving is armor)  — 9/10  (simulation lens, cost S)
Hold a 35-40° angle to the threat and the Tiger's vertical 100mm slab becomes ~140mm+ effective; a round that would have holed you flat-on screams off as a Bounce because you cheated your bow 30 degrees. The knowledge IS the mechanic.
*Judge:* resolve()/effective_thickness already compute compound obliquity — feeding true shot-direction-vs-hull-facing geometry for hits on the player makes the driver's seat a defensive stat with no indicator, and the reward channel (the Bounce ring) already exists in acoustics. Teach it diegetically once — a looted Tigerfibel page, never a tutorial popup.

### 7. Horchhalt — engine-off listening and ember discipline  — 8.9/10  (historical lens, cost S)
Kill the engine and crack the cupola hatch: the muffle lifts and T-34 track squeal and diesel clatter become rangeable by ear long before any vision cone connects — but lighting a cigarette in the open hatch makes the ember a night-detection beacon.
*Judge:* Turns the existing Interior/Exterior muffle crossfade into the Tiger's true long-range sensor and makes shutting down a core verb — sound-forward, stealth-coupled, and mostly gain curves. The cigarette-ember night beacon ties the locked loot list into light discipline; only bolt on the cut Cold Soak restart-risk if playtests show players never turning engines off.

### 8. Walk the splash (bracketing as the only rangefinder)  — 8.8/10  (simulation lens, cost S)
Every miss lands deterministically short or long as a snow/masonry fountain at its true fall-of-shot point; the player brackets like a real gunner — over, short, halve, fire for effect — and knows the 88's holdover at 800 vs 1500m by feel. No range readout ever.
*Judge:* drop_at/time_of_flight are real, so misses become information and bracketing becomes the game's only rangefinder — pure skill expression straight from the combat-range-asymmetry pillar, at the cost of resolving misses to a world point and spawning existing snow-puff FX. Later, fold the cut Tracer Tax in as the cheap-but-loud MG ranging alternative with its stealth bearing-ping.

### 9. The Photograph Alignment  — 8.8/10  (emotional lens, cost S)
A photo tucked beside the gunner's optic all game: a house, trees, figures on a porch. At the leveled rendezvous you raise it in first person and line it up against the ruin yourself; the match is exact and the game says absolutely nothing.
*Judge:* The player performs the Act-2 realization with their own hands and the game does nothing when it clicks — the gut-punch pillar with no cutscene, and the headless screenshot pipeline literally authors the photo from the real house asset for free. Seed the photo beside the optic from minute one of Act 1 so the payoff is earned across a whole act.

### 10. Empty Intercom Roll Call  — 8.7/10  (emotional lens, cost S)
Engine start is a ritual: out of habit the commander keys the intercom and calls his dead crew into static. Act 1 all four names; Act 2 only two; Act 3 he keys the mic, holds it, and says nothing. The whole character arc in three engine starts.
*Judge:* The crew-of-one premise given a voice and a decay arc in under ten seconds of audio per act, hung on an engine-start hook — CrewRole names already exist in game.rs and the interior audio path is built, making this the highest emotion-per-line-of-code on the list. The Act-3 held-then-silent mic press must stay unexplained.

### 11. Concussion blindness (damage to your best sensor)  — 8.6/10  (simulation lens, cost M)
A heavy non-penetrating HE hit leaves you concussed: tinnitus ring, highs ducked, directional pan collapsed to mono for 20 seconds. You're unhurt on paper but can no longer hear which side the engines close from — the correct play is to relocate blind.
*Judge:* If sound is 85% of the game, deafness is the scariest damage state — this attacks the player's actual sensor using behind_armor data every impact already carries, and it IS the planned tinnitus pillar with a trigger. Cap frequency and duration hard and make recovery audible (the world fading back in); repeated mono-fold gets punishing-then-nauseating fast.

### 12. Track surgery (immobilize, then choose)  — 8.5/10  (simulation lens, cost M)
Enemy tracks are external and hittable; a thrown track turns a flanking T-34 into a fixed gun with a live turret. Break the two flankers and deal with them as stationary problems — or leave them screaming into the radio as bait.
*Judge:* Porting the existing ModuleKind/spall model onto enemies converts the ballistics engine's precision into tactical vocabulary against the locked swarm design — mission-kill, withdraw, or bait — without one pixel of new UI. Coordinate with the neuroevolution plan early: immobilized-but-alive champions need a trained overwatch behavior or thrown tracks become free kills.

### 13. Demolition gunnery (HE strips the world and de-fangs tanks)  — 8.4/10  (simulation lens, cost M)
SprGr. through the wall a T-34 is hull-down behind collapses his cover — and his 76mm can do the same to yours. Non-penetrating HE still kills tracks, sights and external MGs, so buildings become armor the ballistics engine already prices.
*Judge:* blast_defeat_mm (18·√filler, live in ballistics.rs:364) already prices masonry, so cover gets honest hit-points and HE becomes the mission-kill answer when AP can't beat a plate — this deepens destructible walls and the module system rather than adding anything new. Keep non-penetrating HE module damage conservative or every engagement collapses into HE spam.

### 14. Two Cigarettes  — 8.3/10  (emotional lens, cost S)
Stop, open the hatch, strike a match: the flare spikes night detection but the smoke fades tinnitus and steadies post-concussion sway. In Act 2 a second interaction appears — light two, rest one on the hatch rim for nobody.
*Judge:* Gives the locked cigarette loot category its consuming mechanic — tinnitus fade and tremor steady, priced by the ember's night-detection cost from stealth.rs — and the Act-2 light-two-rest-one beat is devastating for a single interaction. If a fatigue system ever ships, migrate the cut One Match Left's fatigue-reset scalar here rather than building a second ritual.

### 15. Ready Rack  — 8.3/10  (systemic lens, cost S)
Shells live in a fast ready rack and unusable-in-combat hull stowage; restowing demands the tank stopped and you out of the seat, so every fight is fought with what you racked in the last safe pocket — and full stowage widens the cook-off vulnerability.
*Judge:* Two-tier ammo bookkeeping layered on the existing reload flow and AmmoRack cook-off code, with the vicious inversion — greedy looting literally makes you more explosive — that ties the loot economy to the damage model. This is the kept phrasing; the historical restow duplicate adds nothing the stationary restow ritual here doesn't include.

### 16. The Ring Closes  — 8.2/10  (systemic lens, cost M)
The encirclement front advances across the ActMap on a real clock; the eastern artillery rumble grows day by day. Resting, repairing and detouring spend the same clock the ring spends — unfold the map and measure with a pencil that your planned gap was overrun while you slept.
*Judge:* The game's title as its master clock, and it's heard before it's seen — a distance-scaled artillery LayerState and pencil measurement on the paper map, never a HUD timer — which makes every kept mechanic's time cost real. The pitch's own warning is the risk: tie front speed to act script rather than player performance so it pressures detours without deleting them.

### 17. Petrol Arithmetic  — 8.1/10  (systemic lens, cost M)
The Tiger drinks by throttle, grade, snow depth and engine health; the cockpit needle lies and the dipstick doesn't. Kneeling on the engine deck at dusk, pencil in hand, you draw a range ring on the map and realize the only road you can afford is the one where they're waiting.
*Judge:* Makes fuel — a locked loot category — the route-planning resource, and keeps it diegetic with exactly the right anti-HUD move: an unreliable bouncing gauge whose only truth is an on-foot dipstick. Merge the cut Kessel-fuel-arithmetic duplicate's dig-in-as-pillbox endstate as the historically perfect out-of-fuel fail-forward.

### 18. The Snow Ledger  — 8/10  (systemic lens, cost L)
Everything you do is written into the world: tread imprints, smoke pillars over kills, craters, a collapsed wall. Patrols follow the ledger, converging on your inferred vector; snowfall erases tracks in minutes, a hard freeze preserves them for hours.
*Judge:* The biggest bet kept: patrols reading physical evidence instead of psychically knowing your position makes the whole stealth ecology legible and produces the backtrack-over-your-own-tracks-to-write-a-lie moment nothing in the genre has. Its L cost is honest, so stage it — smoke pillars and kill-site evidence first (stealth.rs last_known already consumes positions), track decals second — and let Hunt Tier and the fuel-leak-trail idea plug in as later evidence types.

### 19. The Last Letter  — 7.9/10  (emotional lens, cost S/M)
At the assembly area before the unretryable finale: paper, the map's pencil, a few minutes to write home knowing no one is there. Whatever you write, in your own handwriting, becomes the death screen if you fall — or is shown folded and unsent if you live.
*Judge:* Reuses the paper-map pencil tech verbatim to make the locked one-shot finale rule mean something — players write honestly because there is no retry — and draw_death already composites the death screen it lands on. Keep the victory-screen folded-unsent variant from the pitch so the act of writing is never wasted either way.

### 20. Versorgungsbomben — chasing the supply drop  — 7.8/10  (historical lens, cost M)
An He 111's drone crosses the night sky with no marker; you take a compass bearing on the sound, pencil it on the map, and race co-evolved patrols to the wind-drifted canisters. Half the time it's shells and fuel; sometimes, faithfully, it's something uselessly absurd.
*Judge:* One repeatable beat that exercises compass, paper map, directional acoustics, loot scarcity and patrol convergence simultaneously — navigation-by-ear made a race — and the historically faithful pepper-and-summer-uniforms roll is precisely Kessel's brand of cruelty. Keep drops rare and radio-announced so they stay events, not a farming loop.

## Cut (with one-clause reasons)

- Lead or die — largely an emergent consequence of ballistics already flying real shells plus the honest intercept-aim the neuroevolution co-evolution plan requires anyway; it comes free, so it doesn't need a design slot.
- Second-life shells (ricochets keep flying) — rare-event garnish; add the deflection-ray continuation as post-core kill-cam polish.
- Blood in the snow (fuel leaks write trails) — fold the fuel-drip trail into The Snow Ledger as one more evidence type instead of a standalone system.
- Kaltstart — inertia starter and engine watch — best of the cold-chore trio, but a temperature-and-crank subsystem duplicates the shutdown tension Horchhalt already sells; salvage the hand-crank as one scripted Act-1 beat.
- Frozen Schachtellaufwerk — a second overnight-cold immobilization chore stacked on Kaltstart's; cut for chore density, though its dawn-probe AI instinct is worth keeping as enemy scheduling flavor.
- Kessel fuel arithmetic — duplicate of Petrol Arithmetic (kept phrasing there); merge its dug-in-pillbox out-of-fuel endstate.
- Mündungsschoner and bore care — maintenance busywork whose dispersion payoff most players will never perceive.
- Tungsten ration — subsumed by The loader's dilemma's hoarded-APCR beat plus a loot-table entry.
- Ready rack and restow (historical) — duplicate of the kept systemic Ready Rack, which adds the loot-greed/cook-off inversion at lower cost.
- Funkdisziplin — L-cost twin of the radio station Act 3's locked LLM net must build anyway; fold its transmit-equals-RDF-beacon risk into that station.
- Kalkanstrich — a third on-foot upkeep loop buying a stealth scalar that night and weather modifiers already provide.
- Bergung — L-cost drive-physics rework for a recovery chore; rescue the gorgeous sledgehammer-CLANG-carries-to-every-ear idea inside track repair later.
- Panzerkameraden on the engine deck — human rider characters strain the tanks-only art scope at L cost for a single act, however moving the recovered-crew doctrine is.
- Seat Relics — lovely, but it's set dressing rather than a mechanic; ship as an art pass on the interior bake.
- The Other Handwriting — a second Act-2 paper reveal that The Photograph Alignment lands harder; keep the authored under-layer as cheap content only if pencil strokes already persist.
- Undelivered Mail — carryable-and-flammable item machinery for a beat the roll call and photograph already deliver.
- Chalk Kill Rings — charming risk-ritual but pure polish nothing downstream consumes; revisit once hull decals exist.
- Dead Frequency — rides radio hardware Act 3 must build regardless; fold the dead rendezvous frequency and the one-time half-second maybe-voice in as content on that station.
- The Fuel Cache Note — act-transition staging rather than a competing mechanic; hand it to Act-2 level design exactly as written.
- Armor as Diary — resonant but display-only at real decal-projection cost; build it after The Snow Ledger creates the decal machinery it needs.
- Cold Soak — duplicate union of Horchhalt's listening half and Kaltstart's restart half, both judged separately.
- One Match Left — duplicate of Two Cigarettes; migrate its fatigue-reset scalar there if a fatigue system ships.
- Weather Fronts as Windows — a modulation layer over scalars every kept system already exposes; adopt as a tuning/content pass once stealth and navigation ship.
- Hunt Tier — its tracking half is contingent on The Snow Ledger; add it later as the ledger's escalation layer, where the radio-tank decapitation counterplay (already in the neuroevolution chain-of-command plan) belongs.
- Wreck Triage — diesel-vs-petrol wreck typing and the siphon-under-convergence-ETA beat are small extensions to the existing loot pass, not a headline slot.
- Maintenance Debt — a damage-persistence policy rather than a mechanic; adopt as the default wiring for the module system.
- Dead Reckoning Drift — poisoning the player's own pencil marks risks reading as input noise and unfairness; prototype only after the map ships, with generous resection.
- Tracer Tax — fold its MG-tracer bearing-ping cost into Walk the splash's ranging economy.
