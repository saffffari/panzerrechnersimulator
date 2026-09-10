# Panzerrechnersystem — DESIGN PILLARS

> **Current authority (2026-09-02):** These systemic pillars remain active, while campaign and crew
> specifics are governed by `docs/STATUS.md` and `docs/LINUX_4090_HANDOFF.md`. The player is the
> fixed five-man crew as one interdependent body, with no replacements—not an isolated protagonist
> or a set of detached NPC stories. Four is only the local-couch human-post limit (the radioman is
> delegated); LAN may seat all five. The canonical finale is the unwinnable Reichstag last stand,
> with only an optional, near-impossible cottage-escape coda. Older Halbe and one-man wording below
> is retained as historical design or implemented-prototype context, not the current target.

The fixed stars. Every system, asset, and tuning decision answers to these. When a tradeoff is
unclear, the pillar wins.

---

## 1. Deep simulation, abstracted presentation
**Take full advantage of the custom Rust/wgpu engine — for realism in the SYSTEMS, not the
pixels.**
Simulate hard: armor penetration & deflection, ballistics with real drag and ranging, acoustic
propagation, internal damage / module knockout, mobility and track damage, optics behavior, the
interdependent five-station crew workload. Then present all of it through a deliberately
**abstracted 3D** style — low-fi,
impressionistic, stylized — that **never aims at photoreal**.

- Realism lives in **what is modeled and how it behaves**, not in surface fidelity.
- Internal resolution and retro-filter strength are independent controls. Resolution changes the
  geometric pixel grid; clean/light/standard/strong presets change posterization, ordered dither and
  grain. **Abstraction is permanent**—neither end aims at photoreal.
- Weather-denied information is the ally of abstraction: Act I blizzard, spring haze, smoke, rain and
  darkness let silhouette and light carry the image without making every act the same whiteout.
- Litmus test for any feature: *"Does this deepen the simulation or the feeling?"* If it only chases
  graphical realism, it's the wrong axis — abstract it and spend the budget on a system instead.

## 2. Five crew, five jobs, one machine
Commander, gunner, driver, loader and radio operator are five persistent crewmen with five
irreducible jobs. In solo, one player embodies the crew and must move between and prioritize all
five. In local couch play, up to four human posts divide that work while the radioman is delegated;
LAN may seat all five. The game is about **workload, communication and isolation under pressure**:
you cannot drive, lay the gun, spot, reload and communicate effectively at once, and that friction
*is* the tension. Systems should make the burden legible and dramatic, never busywork.

## 3. Sound is the primary sensor
When weather, smoke, darkness or steel denies sight, you hear the war first. Audio is **simulated,
spatial, and adaptive**—snow muffles, the hull occludes, threats have direction, and buttoned-up vs.
head-out completely changes the mix. Sound carries information the eyes cannot. (See
`AUDIO_ARCHITECTURE.md`.)

## 4. The cold, the dark, and the few warm lights
Cold blue-grey base; the only warmth is **emissive and isolated** — your muzzle flash, a burning
wreck, a distant lamp. Atmosphere is a first-class system, never a backdrop: the current act haze,
blizzard and source-bound glow establish the look; true local volumetric extinction, self-shadowing
and emissive in-scatter are the target. (See `ART_BIBLE.md`.)

## 5. Always embodied; the steel is home
First-person only in the shipped game. **No detached chase camera, tactical drone, or omniscient
view.** While mounted, every frame is physically located at a crew station, optic, periscope or hatch.
On foot, the player remains a vulnerable body carrying real objects, and the Tiger remains the nearby
home, store and source of protection. Visibility and hearing always trade against exposure. Co-op
crew and allies may be seen only where their bodies actually are—never as free information.

## 6. Historical grounding, not historical pedantry
Panzerrechnersystem is explicit alternate history: one Tiger and crew pass through a compressed continuity from
the fictional Stalingrad breakout to Oderbruch and Halbe in 1945. A Tiger inside the Stalingrad
pocket is already the premise's foundational fiction. Systems, equipment and numbers are grounded in
real WW2 evidence where possible; chronology serves dramatized survival horror, not a documentary.
Plausible and internally coherent over encyclopedic.

---

### How the pillars constrain the build
- **Visual budget** goes to lighting, fog, silhouette, and grain — not polycount or PBR detail.
- **Simulation budget** goes wide and deep — model the system even if its visual is a few stylized
  cues. A penetrated round can ruin your day without a single high-poly spall particle.
- **Weather-denied information is load-bearing**—Act I whiteout, later haze/smoke/darkness and short
  clear intervals all support performance, dread and tactical uncertainty.
- New systems get prioritized in `SIMULATION_ROADMAP.md` by *simulation/feeling payoff per unit
  effort*, under these pillars.
