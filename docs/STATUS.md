# Kessel — current status (2026-07-23)

Kessel is past first-playable. `main` contains a start-to-finish three-act campaign, the embodied
Tiger interior and dismounted loop, LAN crew mode, deterministic verification tools, evolved tank
AI, and a cinematic battle recorder.

## Playable now

- A title menu launches Campaign, Skirmish, LAN host/join, controls, and quit.
- Solo Esc opens the in-game title menu and resumes the live run; Home opens the controls sheet.
  LAN Esc remains a local controls overlay and never pauses the shared simulation.
- Campaign Acts 1–3 carry the same Tiger, damage, stores, and route state through the breakout.
  Encounters include T-34, SU-100, KV-2, ISU-152, IS-2, IS-3, allied Tigers, multiple endings,
  retries in Acts 1–2, and a one-attempt Halbe finale.
- Four embodied posts, manual breech/reload handling, physical optics, hatch/climb/dismount,
  stealth reconnaissance, Luger/infantry, paper map and permanent pencil marks.
- Armor faces, penetration/spall, crew and module wounds, fire/cook-off, wrecks, destruction,
  terrain-following tracked movement, sound propagation, procedural audio, weather, and campaign
  persistence.
- Interior fire suppression is a carried extinguisher with a physical foam stream.
- Track repair is a four-phase outside ritual: grab the loose end, pull the articulated shoes over
  the wheels, place a replacement link, and hammer its pin.
- Normal desktop launch is a decorated maximized window, with an explicit borderless-fullscreen
  opt-in. The `der Kessel` Desktop/taskbar launcher always builds the current `main` release.

## Visual and filming stack

- Low-resolution HDR scene, PS1 vertex/texture treatment, crushed palette/dither, nearest upscale,
  bloom, volumetric smoke/fog, dynamic combat lights, seasonal atmosphere, and real ray-traced
  contact-shadow/sky-occlusion slices.
- Direct RGBA frame capture streams into ffmpeg without thousands of intermediate PNGs.
- The AI camera director has establishing, Tiger-shoulder, T-34 chase, crossfire, ground orbit,
  tactical, and event-close shots. Default output is 1080p/24 H.264 CRF 14; 4K is available.

## AI battle lab

- The existing evolved T-34 policy and a separately evolved Tiger policy fight through the normal
  crew, reload, aiming, armor, and damage rules.
- Tiger training has deterministic classroom seeds, held-out exams, resumable checkpoints, and a
  permanent exam champion so classroom luck cannot overwrite a proved policy.
- A nine-seed 1–6 gauntlet currently finds an empirical majority threshold of **five T-34s**.
- Fixed-seed takes support honest before/after learning films. See `AI_SHOWREEL.md`.

## Verified commands

```sh
cargo test --bin kessel
cargo run --release -- campaign-verify /tmp/kessel-proof
cargo run --release -- firefight /tmp/kessel-firefight
cargo run --release -- ai-gauntlet artifacts/ai-gauntlet \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin 9 90 0
```

## What remains

1. **Scale beyond six full-detail tanks.** Dozens/100 tanks require pooled instanced rigs, spatial
   queries, batched low-rate policy inference, off-camera simulation LOD, and replay logs that can
   be re-filmed. This is the blocker for the armada showreel.
2. **Finish the subsystem-repair language.** Replace transmission, traverse, radio, and MG hold
   actions with distinct wrench/electrical/alignment interactions; add carried spare parts, hands,
   interruption/failure states, and an optional field-workshop welder. A welder should not become a
   magical fix for armor or a thrown track.
3. **Broaden AI generalization.** Co-evolution, more held-out maps/weather/ranges, navigation around
   destruction, squad doctrine, and integration of trained policies into authored campaign bosses
   rather than only the lab.
4. **Showreel finishing.** Replay-first multi-camera coverage, spline/crane authoring, shot handles,
   automatic edit beats, final audio mix, and higher-detail hero track/link assets.
5. **Content and visual closure.** Replace remaining hero placeholders/missing vehicle rigs, finish
   bespoke Act 2/3 geography and landmarks, improve particles/explosions, and continue material/
   exposure grading.
6. **Production closure.** Save/options/rebinding/accessibility, scalable quality presets, broad
   performance profiling, campaign balance passes, soak tests, packaging, and release QA.
