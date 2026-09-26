# Winter sound and atmosphere foundation — 25 September 2026

The shader worktree now connects a continuous procedural wind voice to Act One's snow, fog and wind field. The saved winter courtyard remains the authored world. Start `tools/launch_winter_session.sh` or the installed **Panzer — Winter Map** application entry; its title menu has Singleplayer and Co-op buttons. The editor is available from the same application entry's actions.

## Sound direction and implementation

Three AI agents contributed: Athena implemented contact and armor layers; Hermes implemented the continuous weather voice, spatial handling controls and audio routing corrections; Hephaestus researched primary accounts and created ordinary-audio-graph listening fixtures. Root integrated gameplay, weather, lighting, KIA and local co-op, and ran the final build/render checks.

Shell impacts layer a short contact/grit transient, plate flex and inharmonic steel resonance. The existing parameter path carries striking velocity, caliber, projectile mass, physical/effective thickness, angle, panel, penetration outcome and listener geometry. Machine-gun contacts use lighter individual plate taps and granular chatter. Own-hull contacts inside arrive through the structure; exterior contacts use propagation delay and the current listener's spatialization. Rifle/MG contacts in snow or soil have a separate 95 ms dry noise response, rather than a cannon impact. These remain art-directed models; they are not acoustic measurements of a Tiger or historical validation of the armor simulation.

The new wind source uses continuously varied noise, filtered body/air and seal/hull resonances. Sample-rate ramps prevent stepping, and parameters update without rebuilding a loop each frame. Shelter changes the response, including partial protection at an open hatch. Existing engine RPM, throttle/load, gear and traverse voices remain in use. Map and compass handling now have short quiet edge-triggered contacts. Existing verified feedback recordings remain active where selected; fallback cues remain explicitly procedural.

Act One fronts retain the saved conditions for ten seconds, then ease between lulls and heavier weather over 80–120-second legs. One storm value drives snow density and visibility; the resulting wind scale feeds the wind sound, smoke and live ballistic wind. This is a designed dramatic cycle, not a weather forecast. Other acts retain their existing weather flow. No score or musical drone was added; the later composed score can occupy its own emotional space.

Cabin indirect sky fill is reduced, base haze increased, and bloom widened modestly around existing bright apertures and emissive practicals. The hemisphere-light and bloom changes also affect other shader-rendered scenes; the weather-front logic is restricted to Act One. The white snow and saved pixel/dither profile remain authored in the winter document.

## KIA and co-op

Fatal play enters black immediately, with all audio sources sample-gated. The first two seconds cannot be dismissed. Live play has no attacker/exterior replay. A single co-op casualty blacks out that person's half and cuts the shared audio mix for two seconds; the survivor then resumes hearing the current world. Expired tails are not replayed. Already queued physical-device samples cannot be recalled. See [KIA and co-op details](KIA_COOP_20260925.md).

## Review and evidence

[Open the listening review](../captures/astra/winter_sound_review_20260925.html). It contains six staged ordinary-audio-graph scenes (weather, drivetrain, mechanisms, armor, MG contacts and KIA), plus handling and the dry ground-contact preview. These are sample-clock fixtures, not microphone recordings or captured live combat. No subjective speaker/headphone audition is claimed. The six main WAVs share export gain 1.0, without per-cue normalization.

- Audio tests: 52 passed, one separately invoked audition fixture ignored by default. Acoustics: 35 passed, followed by two targeted tests after the low-speed ground-cue refinement. Shader validation: 11 passed.
- The KIA WAV has exactly 88,200 stereo frames at zero, from 3.000 through 5.000 seconds. A GPU KIA image has all RGB values zero.
- The final GPU `koop` regression passed. The live small-arms GPU fixture passed delayed damage, thin cover, moving target, range retirement and inert roof impact checks.
- `winter_fronts_20260925_v2.json` records actual saved-world updates and 960×540 internal renders on RTX 4090. Identical camera-local vegetation counts are present in both views. Staged lull: storm 0.1201, fog 0.00035; staged blizzard: storm 0.9444, fog 0.003853. Snow changes with the same storm value.
- Release/offline build, shell syntax, desktop entry validation and diff whitespace checks passed. The repository has existing compiler warnings; this is not a warning-free build.

The native split layout is reviewed separately from the diagnostic GPU fixture: `winter_coop_native_20260925_v1.png` captures the actual running winter world with Commander above and the second-device connection prompt below. `winter_menu_native_20260925_v1.png` records the title menu buttons. A physical two-controller playtest and subjective mix review remain to be done with the user.

## Online sources and provenance

[Primary accounts and recording candidates](SOUND_REFERENCES_20260925.md) separates testimony from design inference and lists creator-hosted CC0 candidates. The accounts describe different tanks/theaters and are qualitative references, not an Eastern Front Tiger sound calibration. No newly sourced online recordings were downloaded or admitted in this pass. Existing verified owner-private assets were preserved; future imported recordings need their original bytes, license record and source hash alongside the selection manifest.
