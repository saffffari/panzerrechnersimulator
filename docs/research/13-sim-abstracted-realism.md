# 13 — Reconciling Deep Simulation with an Abstracted 3D Style

**Project:** Kessel (first-person, single-crew Tiger escaping the Stalingrad *Kessel*, UE 5.8)
**Scope:** The HOW of Pillar 1 — *"Deep simulation, abstracted presentation."* Where does the
realism budget go (systems) vs. where does the visual budget go (fog, light, silhouette, grain)?
What do we **simulate-but-barely-render** vs. **render-with-stylized-cues**? How does the blizzard
hide crude geometry and justify a short draw distance? How do we let the player *read* complex state
(damage, threat direction, mobility) through abstract cues — sound, instruments, silhouette, light —
instead of detailed visuals? And what concrete guardrails keep the project from drifting toward
photoreal?

This doc is the connective tissue between `DESIGN_PILLARS.md §1`, `ART_BIBLE.md`, and the system
docs (`04` ballistics, `05` PS1 retro, `06` fog, `07` view modes). It is design + research only.
External lessons are cited; on-disk engine facts are carried from the sibling docs (which already
cite `path:line` against the 5.8 source).

---

## 0. The governing idea in one sentence

> **The player never sees the simulation. The player sees a small set of cues the simulation
> drives — and reconstructs the simulation in their head from those cues.** Our job is to simulate
> hard, then choose the *cheapest legible cue* for each simulated fact, and let the fog, the dark,
> and the player's own pattern-recognition do the rest.

This is not a compromise; it is the central design lever. It lets us spend a 4090's worth of
simulation on physics/ballistics/acoustics while authoring almost no high-fidelity surface art.

---

## 1. The theory: Game Model vs. Player Model

The cleanest articulation of Pillar 1 comes from Tynan Sylvester's *The Simulation Dream*
(RimWorld). He splits any simulation game into two things:

- the **Game Model** — what the code actually simulates;
- the **Player Model** — the mental picture the player builds of that simulation.

His thesis: *"The whole value of a game is in the mental model of itself it projects into the
player's mind."* A sophisticated Game Model that never reaches the Player Model is wasted — his
canonical failures are the *Ultima Online* ecology and the *BioShock* ecosystem, both of which the
player killed through too fast to ever perceive. (Source: tynansylvester.com, *The Simulation
Dream*.)

**Two consequences that govern Kessel:**

1. **Depth is justified only when it changes a cue the player can perceive.** A penetration model
   that distinguishes a deflected APCBC from a clean pen is worth it *because the player hears the
   spang vs. the wet bang, sees the spall, and loses a module* — not because of a high-poly spall
   particle. The litmus test from `DESIGN_PILLARS.md §1` ("does this deepen the simulation OR the
   feeling?") is the same test stated from the player's side.
2. **Apophenia does free narrative work.** Humans project intent, threat, and story onto sparse
   patterns (Sylvester's bouncing-balls/Heider-Simmel point). A black silhouette that grows in the
   fog while a diesel note deepens in your right ear *is* an enemy flanking you — the player's brain
   supplies the dread. We don't need to render the enemy's face; we need to render the *growth* and
   place the *sound*. **Abstraction is not information loss if the cue preserves the load-bearing
   variable** (here: bearing, range-rate, and class).

So the design pattern for every simulated system is: **simulate the variable deeply → identify the
one or two variables the player must feel → bind each to the cheapest unambiguous cue → discard the
rest of the visual.**

---

## 2. Lessons from games that ran deep systems under low-fi visuals

| Game / sim | Deep system underneath | Low-fi / stylized surface | Lesson for Kessel |
|---|---|---|---|
| **Silent Hill (PS1, 1999)** | full 3D town, free exploration, monster AI | distance fog + darkness to mask a tiny render radius and kill pop-in | Fog as a *feature, not a patch*: the technical mask **became** the dread. We design the whiteout *intentionally* for the same double win — perf + atmosphere. (retro-replay.com; neogaf; eliasdaler PS1-fog talk) |
| **Silent Hunter series** | real hydrophone bearings, ship acoustic signatures, hull stress | almost no exterior visual relevance; you play the **sound station** | Threat *direction and class* can live entirely in audio. "Fast chopping = fast screws; bearing drift = it's turning." You win by listening, not looking. This is Pillar 3 made literal. (subsim.com) |
| **Dwarf Fortress** | the deepest world-sim in games (geology, history, body parts) | ASCII / tilemap; near-zero rendering | Proves simulation depth is **orthogonal** to render fidelity. DF's abstraction *invites* the imagination to paint the scene. (gamedeveloper.com; Wikipedia) |
| **RimWorld** | colonist needs, relationships, incident "storyteller" | flat 2D top-down, simple sprites | The discipline of *legibility*: simulate only what serves the stories you want, and represent it so the player can read it at a glance. (gamedeveloper.com; The Simulation Dream) |
| **LIMBO / INSIDE (Playdead)** | physics puzzles, atmospheric AI | silhouettes, fog, backlight, minimal geometry & texture | Readability from **light, form, and composition** instead of detail. Atmospheric fog gives depth and scale; a backlight defines an edge. Simple geometry "finds its beauty in minimalist simplicity." (pixelfondue.com; girlsongames.ca) |
| **King Kong (P. Jackson, "no-HUD")** | ammo, stamina, health | **no HUD at all** — state read from breathing, shot count, speed, ambient sound | Diegetic-only is viable when the *world* carries the signal. A hardcore sim "can afford more diegetic and less explanatory interfaces." (tvtropes; nastyrodent.com) |

**Synthesis:** every one of these decoupled *system depth* from *pixel count*, and several turned the
visual *poverty itself* into the mood. Kessel sits at the intersection of all six: Silent Hill's fog,
Silent Hunter's acoustic threat-reading, DF/RimWorld's sim-depth-without-fidelity, Playdead's
silhouette-and-light readability, and King Kong's diegetic restraint.

---

## 3. The moodboard intent: PS1 survival-horror + fog as a *design license*

The reference set (`ART_BIBLE §9`) is dominated by two registers: **PS1/PS2 survival-horror fog
whiteouts** (`8d26dd56…`, `51bbce01…`, `2f69ff9d…`, `eed370b9…`) and **CRT-green diegetic cockpit
instrumentation** (`good_cockpit_refernce.jpg`, `241d64fc…`, `d467af81…`, `d62c2bb0…`). That is not a
coincidence of taste — it is the whole thesis pre-figured in images:

- The **fog whiteouts** say: *the world is allowed to be value-on-fog with no detail.* The reference
  enemies in `8d26dd56…` are pure silhouettes. That is the license to ship 6k-tri enemy tanks
  (`ART_BIBLE §6a`) and let LOD1/2 drop them to 2k/500 because fog eats the difference.
- The **CRT cockpit** images say: *complex state is allowed to arrive as glowing instrument text and
  reticles on glass*, not as a photoreal damage visualization. The amber "LtLaser 50 / IBPack 17"
  readout style is exactly how we surface ammo, reload, and module health.

The PS1 register is doing **three** jobs at once, and we should treat each as deliberate:

1. **Aesthetic** — survival-horror dread, nostalgia, the uncanny.
2. **Concealment** — vertex swim, posterize, dither and low internal res (`05`) hide that the
   geometry is crude and the textures tiny; crunch is *camouflage for low-fi assets*.
3. **Honesty about the budget** — the `MasterRetro` dial (`05 §0`) means the look never *pretends*
   to be photoreal, so it can never *fail* to be photoreal. A stylized image has no uncanny valley
   to fall into.

> **Key reframing:** in a photoreal game, low-poly geometry and small textures are *bugs* the player
> notices. In Kessel they are *the style* — and the fog + crunch make them unreadable as flaws.
> Abstraction converts our biggest art savings into our strongest mood.

---

## 4. The taxonomy: simulate-but-barely-render vs. render-with-stylized-cues

This is the core deliverable. For every system, decide which **bucket** it lives in.

### 4a. The three buckets

- **Bucket A — SIMULATE DEEP / RENDER NOTHING (or near-nothing).** The system runs at full fidelity
  in code; its visual footprint is a sound, a number, a light pulse, or a single particle. Most of
  the simulation lives here.
- **Bucket B — SIMULATE DEEP / RENDER A STYLIZED CUE.** The system runs deep *and* gets a
  deliberately abstracted visual: a silhouette, a glow, a reticle, a posterized muzzle bloom. The
  cue is impressionistic, never literal.
- **Bucket C — RENDER WITH FIDELITY (the hero exception).** The one place we spend real surface
  budget: the **interior cabin**, on screen 100% of the time (`ART_BIBLE §1, §6a`). Even here the
  posterize/palette pass crushes it for the retro dial — it is authored high, *displayed* abstracted.

### 4b. System-by-system assignment

| System (depth lives in code) | Bucket | What we actually render | Cheapest legible cue(s) |
|---|---|---|---|
| **Armor penetration / deflection** (`04`) | A | nothing of the *calculation* | spang-vs-bang **audio**; brief spark/spall sprite; module-loss readout |
| **Internal damage / module knockout** (`04 §9`) | A | no interior gore, no x-ray | CRT **status LED** flips red; gauge bar drops; capability *changes* (turret won't traverse, gun won't load) |
| **Ballistic flight / drag / drop** (`04 §3`) | A→B | a faint tracer/arc only | tracer streak; impact flash + dirt/snow puff at the strike point |
| **Acoustic propagation / occlusion / snow muffle** (Pillar 3) | A | nothing visual | **spatialized, filtered audio** — the primary sensor |
| **Mobility: tracks, suspension, terrain, throw-a-track** (`03`) | A→B | crude hull motion, no track-link detail | engine-note pitch by load; **camera shake**/lurch; a track squeal; heading/speed readout |
| **Enemy presence & approach** | B | low-poly hull as **value-on-fog silhouette** | growing silhouette; engine note panned by bearing; muzzle flash through fog |
| **Muzzle flash / firing event** (`06 §3`) | B | pulsed point light + sprite, posterized | huge transient **warm fog bloom**; recoil shake; gun report |
| **Burning wreck (enemy death)** (`06 §3-4`) | B | flickering light + Local Fog plume | red-orange **glow bleeding into snow-fog**; crackle audio |
| **Weather / blizzard** (`06 §5`) | B | camera-anchored Niagara snow | streaking flakes; wind roar; reduced visibility *as a mechanic* |
| **Optics / sight behavior** (`07`) | B | vignette tube, CRT reticle, FOV zoom | green reticle + range ticks on glass; recoil shakes reticle |
| **Crew-of-one workload** (Pillar 2) | A→B | which sight you're in, hands on interaction | you *cannot* drive + lay + load at once; the friction is felt, not drawn |
| **The interior cabin** (`ART_BIBLE §1`) | **C** | the one high-fidelity hero asset (then crushed) | rivets/weld/cabling lit by isolated emissive |

**Rule of thumb for new features (write this into `SIMULATION_ROADMAP` prioritization):**
> Default every new system to **Bucket A**. Promote to **B** only if a *stylized* cue measurably
> improves legibility or feeling. Promote to **C** only if it is the interior. If a feature *only*
> makes sense as fidelity (a photoreal track-link, a high-poly spall fan, PBR mud), it is on the
> wrong axis — cut the render, keep the system.

---

## 5. How the blizzard/fog hides crude geometry and earns the short draw distance

The whiteout is **load-bearing infrastructure**, not set dressing (`DESIGN_PILLARS §1`, "the
whiteout is load-bearing"). It does four jobs simultaneously:

### 5a. Concealment of low-fi assets
The volumetric fog (`06`) crushes visibility to **40–80 m** (`VolumetricFogExtinctionScale 2–4`,
`VolumetricFogDistance 6000–9000`, per `06 §2c`). Beyond ~30 m, an object is *value-on-fog with no
detail* — its texture resolution, normal map, and polycount are irrelevant because the medium has
already dissolved them. This is exactly why `ART_BIBLE §6a` can budget enemy tanks at 6–12k tris
with aggressive LOD1/2 at 2k/500: **the fog is a free, physically-motivated LOD system.** The PS1
posterize/dither pass (`05 §5`) finishes the job on whatever detail survives the fog.

### 5b. Justified short draw distance (perf + dread, the Silent Hill double-win)
Because the fog *narratively* explains why you can't see past 80 m, we can hard-cull the world at
that range with **zero immersion cost** — the player attributes the limit to General Winter, not to
the engine. This is the explicit lesson of Silent Hill's fog: a render-radius mask that became the
defining mood (retro-replay.com; neogaf). For us it means: small streamed world, cheap far culling,
and the dread of "they're out there, I just can't see them yet."

### 5c. Silhouette-first readability (the Playdead lesson)
With detail denied, the **silhouette and the light** carry all meaning (pixelfondue.com;
girlsongames.ca). Design implications:
- **Enemy hulls must read as silhouettes**, not by their textures. Author distinct *outlines*
  (turret shape, gun length, hull proportion) so a black shape at 60 m is identifiable by class.
- **Backlight and emissive define edges.** A wreck fire behind an approaching tank rims it; your
  muzzle bloom briefly prints the scene. INSIDE's "volumetric rays around dark outlines" is our
  wreck-glow-through-fog.
- **Scale and distance come from haze depth** — fog gradient *is* the depth cue, so we needn't model
  fine mid-ground detail.

### 5d. Motion-masking
Driving blizzard particles (`06 §5`) and the foreground streaking flakes add high-frequency screen
motion that **masks vertex swim, dither crawl, and LOD pops**. The snow is camouflage for the
crunch's own artifacts. Tie flake density/speed to vehicle velocity so fast motion (when pops are
most likely) gets the most cover.

> **Net:** fog + dark + snow + crunch form one coherent *concealment stack*. Each layer hides a
> different class of cheapness (geometry, draw distance, texture, temporal artifacts) while all four
> read as a single intentional atmosphere. Never treat any of them as a removable "graphics option"
> — they are the abstraction engine.

---

## 6. Reading complex state through abstract cues (not detailed visuals)

The simulation produces a lot of state. Below is the **cue-binding table**: each simulated fact and
the abstract channel(s) that surface it. Channels are ordered by Pillar priority — **sound first**
(Pillar 3), then instrument, then silhouette/light.

### 6a. Threat direction & class → SOUND (primary), silhouette (secondary)
- **Bearing:** spatialized engine/track/firing audio panned to the threat's direction — *"a shell
  impact left of the tank bangs in the left ear"* (user audio vision; Pillar 3). This is the Silent
  Hunter model: bearing and range-rate from sound alone (subsim.com).
- **Range / closing:** loudness + low-pass (snow muffles distance, `06`/Pillar 3) + the silhouette
  *growing* in the fog. Range-rate is the Doppler-ish change in the engine note.
- **Class:** diesel note + silhouette outline (gun length, turret shape). The player learns "that
  cadence + that shape = T-34."
- **Buttoned vs. head-out** completely changes the mix (open-air vs. muffled interior) — so *which
  sight you choose* trades visibility for acoustic information. (Detailed in the planned
  `AUDIO_ARCHITECTURE.md`, referenced by `DESIGN_PILLARS §3`.)

### 6b. Damage / internal module state → CRT INSTRUMENTS + capability change
We never render interior gore or an x-ray cutaway. A penetration's *consequences* surface as:
- **CRT status LEDs / gauge bars** (`ART_BIBLE §5`): green→red LED on the affected module; a bar
  drops. The diegetic "WEAP/SHIELD/DRIVE"-style gauges of `good_cockpit_refernce.jpg`.
- **Capability change as the loudest cue:** the truest signal that the gun loader is hit is *the
  reload timer stops*; that the traverse drive is knocked is *the turret won't slew*. Per Sylvester,
  the player models the damage from what they can no longer **do** — far stronger than any visual.
- **Audio:** the spang of a deflection vs. the deep bang of a clean pen tells the player the outcome
  before any gauge updates.
- **Screen-space feedback for a hit on the player:** brief red flash + heavy camera shake (`MILESTONE_1_PLAN` step 10), not a blood overlay.

### 6c. Mobility / vehicle state → MOTION + ENGINE AUDIO + minimal readout
- **Engine load/RPM:** pitch and grind of the diesel loop (drive the audio from the `03 §3`
  `FVehicleEngineConfig` RPM/torque curve). The player hears the hill before the speedo moves.
- **Track/terrain events:** lurch + camera shake on rough ground; a squeal on hard pivot; a distinct
  *snap* if a track is thrown (Bucket A: simulated, surfaced as audio + a heading drift you must
  correct).
- **Heading / speed:** a small CRT corner readout (`ART_BIBLE §7a`, the `6c4358845…` compass style) —
  minimal, diegetic, never a modern HUD.

### 6d. Combat events → LIGHT scattering into the fog
Per `06 §3`, the warm emissive events are both the drama and the information:
- **Your muzzle flash** prints the immediate scene for ~0.06 s — a free, momentary "flash photograph"
  of what's in front of you.
- **Enemy muzzle flash** in the fog is a *bearing fix* on an unseen shooter.
- **Burning wreck** = confirmed kill + a persistent warm landmark that rims approaching threats.

### 6e. Design rules for cues
1. **One fact, one primary channel.** Bearing is *audio-primary*; damage is *instrument-primary*.
   Redundancy is fine (silhouette also implies bearing) but each fact has a designated lead channel
   so the player learns where to look/listen.
2. **Cues must be learnable and consistent.** The spang-vs-bang, the diesel cadences, the LED colors
   form a *vocabulary*; never reuse a cue for two unrelated facts.
3. **Prefer capability-change over annotation.** When possible, communicate damage by changing what
   the machine does, not by drawing a damage texture.
4. **Keep the warm/green accents scarce** (`ART_BIBLE §3b`): if every event glows, no glow carries
   information. Scarcity is what makes a single muzzle flash a usable bearing fix.

---

## 7. Concrete guardrails against photoreal drift

These are the project's *fixed stars* for the abstraction axis. They belong in review checklists and
in `SIMULATION_ROADMAP` gating. Each is a binary you can hold a build up against.

### 7a. Process guardrails
- **G1 — The simulation/feeling litmus on every feature.** Before any work: *"does this deepen the
  simulation or the feeling?"* If the only answer is "it looks more real," reject or re-scope to a
  system. (`DESIGN_PILLARS §1`.)
- **G2 — Default-to-Bucket-A.** New systems render *nothing* until a stylized cue is justified (§4).
  Reviewers must ask "what is the cheapest cue?" before approving any new mesh/material.
- **G3 — Author high, display crushed.** Surface art may be authored at modern fidelity, but it
  **must** pass through `MasterRetro` post (`05`); nothing ships that only looks right with the dial
  at 0. The dial's *retro end is the design target* (default ship ~0.7, `ART_BIBLE §2`).
- **G4 — The whiteout is non-removable.** Volumetric fog + emissive scatter + CRT-green stay **ON at
  every dial position** (`ART_BIBLE §2`, `MILESTONE_1_PLAN §2`). They are not a graphics option.
- **G5 — No third-person, no exterior, no allies, ever** (`DESIGN_PILLARS §5`). This *structurally*
  caps the fidelity demand: we never have to render a beauty-shot of the tank's exterior, so we never
  need exterior PBR detail. First-person-only is itself an abstraction guardrail.

### 7b. Render-budget guardrails (the anti-photoreal "do-nots")
- **G6 — Visual budget goes to lighting, fog, silhouette, grain — not polycount or PBR detail**
  (`DESIGN_PILLARS`, "How the pillars constrain the build"). Poly/texture budgets are *style choices*
  (`ART_BIBLE §6`), and the 4090 headroom is spent on **simulation and the volumetric grid**
  (`06 §7`), not on surface fidelity.
- **G7 — Detail only where the eye lives.** Fidelity is allowed exactly one home: the interior hero
  asset (Bucket C). Everything in the fog gets *less* detail, not more, the further out it sits.
- **G8 — No feature whose payoff is purely cosmetic realism.** Examples to *reject* on sight:
  photoreal track-link physics rendering, high-poly spall debris fans, PBR mud/wetness on the
  exterior, sub-surface skin, lens-accurate film grain chasing realism rather than CRT grain.
  Simulate the *effect* (a thrown track, spall damage) in Bucket A; skip the photoreal render.
- **G9 — Anti-uncanny-valley clause.** If a render starts to read as "almost photoreal," that is a
  defect, not progress — push it back toward stylization (more posterize/palette, lower internal res,
  more fog). The style has *no* valley to fall into only as long as it stays committedly abstract.

### 7c. The single test that catches drift
> **Screenshot test:** take any in-game still. If it could be mistaken for a photo or a modern
> milsim screenshot, the abstraction has failed — regardless of how good the simulation is. A correct
> Kessel still looks like a PS1 survival-horror frame (`8d26dd56…`, `51bbce01…`): fog-swallowed
> silhouettes, posterized cold values, isolated warm glow, CRT-green instruments. The *systems*
> behind that frame can be the deepest tank sim ever built.

---

## 8. Worked example — a single enemy round penetrating the hull

To show the buckets cooperating end-to-end (and that "abstracted presentation" loses *no* simulation
depth):

1. **Simulated deep (Bucket A, invisible):** enemy fires; `04` ballistics computes muzzle velocity,
   drag, drop, impact angle on a real `FArmorSection`; ricochet check; LOS thickness; the round
   *penetrates*; a fragment cone resolves against `FCrewModule`s and knocks out the loader node.
2. **Rendered as stylized cues (Bucket B):**
   - **Sound (lead):** a deep bang panned to the impact bearing, then the metallic interior
     after-ring; the snow-muffled report of the distant shooter arrives spatialized.
   - **Light:** the enemy muzzle flash was a brief warm bloom in the fog — a bearing fix you got a
     half-second earlier.
   - **Screen:** short red flash + a hard camera lurch (`MILESTONE_1_PLAN` step 10).
3. **Rendered as instrument state (Bucket C surface, abstracted display):** the loader/ammo CRT LED
   flips red; the **reload timer stops** — the truest, most legible signal that you've lost the
   loader. Your next shot simply won't chamber until you address it.

Nowhere did we render interior gore, a spall fan, or a photoreal armor scar. The player's Player
Model — *"I took a clean pen on the left, lost my loader, someone's reloading to finish me, bearing
nine o'clock"* — is complete, vivid, and dread-soaked, built entirely from sound, one light, one
shake, and one stopped timer over a deep simulation.

---

## 9. Recommendations (what to lock)

1. **Adopt the three-bucket taxonomy (§4) as the standard triage** for every system in
   `SIMULATION_ROADMAP`. Default Bucket A; promote deliberately.
2. **Treat fog + dark + snow + PS1 crunch as one non-removable "concealment stack" (§5)** — document
   it as infrastructure, not graphics settings. They are why low-fi assets are affordable.
3. **Make audio the primary state-channel for threat direction and combat outcome (§6)** — this is
   Pillar 3 and the Silent Hunter lesson; it carries the most information for the least render cost.
   Fast-track the planned `AUDIO_ARCHITECTURE.md`.
4. **Surface damage as capability-change + CRT instrument, never as photoreal visualization (§6b).**
5. **Put the §7 guardrails (especially G2 default-to-Bucket-A and the §7c screenshot test) into the
   review checklist** so drift is caught per-build, not per-milestone.
6. **Keep fidelity to exactly one home — the interior (Bucket C)** — and even there, ship it through
   the `MasterRetro` crush.

---

## 10. Open questions (for the user / future docs)

- **OQ-1 — Cue vocabulary spec.** The spang-vs-bang, diesel cadences, and LED color language (§6)
  need a single authoritative table once real SFX exist. Belongs in `AUDIO_ARCHITECTURE.md`.
- **OQ-2 — Silhouette-identifiability target.** How distinct must enemy outlines be at 60 m in fog
  for class recognition? Needs a playtest once placeholder hulls exist (`MILESTONE_1_PLAN` step 0).
- **OQ-3 — Minimum legible instrument set for M1.** Which CRT readouts are load-bearing for the
  worked-example loop (§8) vs. polish? (Ties to `07` and the reticle/UMG plan.)
- **OQ-4 — `MasterRetro` default vs. abstraction floor.** `ART_BIBLE` proposes ~0.7 ship default
  (`OD-6`); confirm, and define the *lowest* dial position still considered non-photoreal (the G9
  floor).
- **OQ-5 — Does any system ever earn Bucket C besides the interior?** e.g. a hero wreck the player
  passes within touching distance. Default: no.

---

## 11. Sources

External lessons (web, accessed June 2026):
- Tynan Sylvester, *The Simulation Dream* — Game Model vs. Player Model, apophenia, simulate-only-
  what-serves-the-story. tynansylvester.com/2013/06/the-simulation-dream/
- Silent Hill fog as draw-distance mask turned signature mood — retro-replay.com; neogaf.com
  "technical limitations as an advantage"; Elias Daler PS1-fog talk (eliasdaler.itch.io/ps1-fog).
- Silent Hunter acoustic threat-reading (bearing/class from sound) — subsim.com radio room & reviews.
- Dwarf Fortress / RimWorld — simulation depth orthogonal to render fidelity; legibility discipline —
  gamedeveloper.com; en.wikipedia.org/wiki/Dwarf_Fortress.
- LIMBO / INSIDE — silhouette, fog, backlight, minimalist geometry for readability — pixelfondue.com;
  girlsongames.ca; en.wikipedia.org/wiki/Inside_(video_game).
- Diegetic / no-HUD interfaces (King Kong; "hardcore sim affords more diegetic interface") —
  tvtropes.org Diegetic Interface; nastyrodent.com diegetic-vs-non-diegetic UI.

On-disk / sibling-doc grounding (engine facts cited `path:line` in those docs):
- `DESIGN_PILLARS.md §1, §3, §5` — Pillar 1 abstraction, sound-as-sensor, first-person-only.
- `ART_BIBLE.md §1–9` — interior hero asset, PS1-tunable render target, palette, fog scatter, CRT UI,
  poly/texture budgets, reference index.
- `docs/research/04-ballistics-armor.md` — deep penetration/spall/module model (Bucket A).
- `docs/research/05-ps1-aesthetic.md` — the `MasterRetro` crunch stack (concealment + honesty).
- `docs/research/06-volumetric-fog-scatter.md` — whiteout, extinction/draw-distance, emissive scatter.
- `docs/research/07-view-modes-controls.md` — sights, reticles, FOV zoom (Bucket B optics cues).
- `MILESTONE_1_PLAN.md` — build steps for fog-first, cue events (muzzle bloom, red-flash hit, wreck).
- `AUDIO_ARCHITECTURE.md` — *planned sibling*, referenced by `DESIGN_PILLARS §3`; owns the cue
  vocabulary (OQ-1).
