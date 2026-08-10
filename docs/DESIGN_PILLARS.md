# Kessel — DESIGN PILLARS

The fixed stars. Every system, asset, and tuning decision answers to these. When a tradeoff is
unclear, the pillar wins.

---

## 1. Deep simulation, abstracted presentation
**Take full advantage of what Unreal can do — for realism in the SYSTEMS, not the pixels.**
Simulate hard: armor penetration & deflection, ballistics with real drag and ranging, acoustic
propagation, internal damage / module knockout, mobility and track damage, optics behavior, the
single-crew workload. Then present all of it through a deliberately **abstracted 3D** style — low-fi,
impressionistic, stylized — that **never aims at photoreal**.

- Realism lives in **what is modeled and how it behaves**, not in surface fidelity.
- The `MasterRetro` dial moves *fidelity* (PS1-crunch ↔ modern-grimy); **abstraction is permanent** —
  neither end is photoreal.
- The blizzard whiteout is the ally of abstraction: detail is *denied by the world*, so stylization
  reads as atmosphere, not as a budget limit. Lean into it — fog, grain, silhouette, light.
- Litmus test for any feature: *"Does this deepen the simulation or the feeling?"* If it only chases
  graphical realism, it's the wrong axis — abstract it and spend the budget on a system instead.

## 2. You are the crew of one
Commander, gunner, driver, loader, radio — all you. The game is about **workload and isolation under
pressure**: you cannot drive, lay the gun, spot, and reload at once, and that friction *is* the
tension. Systems should make the one-person burden legible and dramatic, never busywork.

## 3. Sound is the primary sensor
In a whiteout you hear the war before you see it. Audio is **simulated, spatial, and adaptive** — snow
muffles, the hull occludes, threats have direction, buttoned-up vs. head-out completely changes the
mix. Sound carries information the eyes can't. (See `AUDIO_ARCHITECTURE.md`.)

## 4. The cold, the dark, and the few warm lights
Cold blue-grey base; the only warmth is **emissive and isolated** — your muzzle flash, a burning
wreck, a distant lamp — and it **scatters into the fog volume**. Atmosphere is a first-class system
(volumetric fog + emissive in-scatter + blizzard), never a backdrop. (See `ART_BIBLE.md`.)

## 5. Always inside the steel
First-person only. **No third-person, no exterior, ever.** Every frame is from inside the vehicle —
gunner's optic, commander's periscope, or head-out-of-the-hatch. The machine is always around you;
vulnerability and visibility trade off by which sight you're in.

## 6. Historical grounding, not historical pedantry
A Tiger breaking out of the Stalingrad *Kessel* under General Winter. Systems and numbers are grounded
in real WW2 data (historical-accurate ballistics baseline); the experience is dramatized survival
horror, not a museum. Plausible over encyclopedic.

---

### How the pillars constrain the build
- **Visual budget** goes to lighting, fog, silhouette, and grain — not polycount or PBR detail.
- **Simulation budget** goes wide and deep — model the system even if its visual is a few stylized
  cues. A penetrated round can ruin your day without a single high-poly spall particle.
- **The whiteout is load-bearing** — it justifies short draw distance (perf + dread) and lets crude
  geometry disappear into atmosphere.
- New systems get prioritized in `SIMULATION_ROADMAP.md` by *simulation/feeling payoff per unit
  effort*, under these pillars.
