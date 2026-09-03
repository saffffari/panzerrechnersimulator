# Codex campaign overhaul

> **Current campaign authority (2026-09-02):** `docs/STATUS.md` and
> `docs/LINUX_4090_HANDOFF.md` supersede this document wherever they differ. The canonical
> destination is Berlin and the unwinnable Reichstag last stand. A hidden escape by surviving crew
> to an intact cottage is only an optional, near-impossible, deliberately ambiguous hope coda.
> Earlier Halbe, hometown/pasture-as-main-ending, conventional final-tank-battle and
> one-man-protagonist concepts below are retained as historical design or implemented-prototype
> context, not the current target.

This branch turns the design corpus into a start-to-finish campaign while preserving the project's
north star: **environmental storytelling**. The player is never a floating camera or an abstract
roster. It is one crew, in one increasingly damaged Tiger, moving through places whose objects tell
the story before the HUD does.

## Playable campaign

- Act 1, **The City**: winter breakout through patrols, ambushes, overwatch and a KV-2 boss.
- Act 2, **Die Heimkehr**: the same Tiger and stores cross the thawing homeland, ending against an
  ISU-152. Acts 1 and 2 restore their entrance checkpoint after loss while retaining discovered
  knowledge.
- Act 3, **Kameraden**: link with two AI Tiger wingmen, prepare Halbe, protect the civilian column
  and town, survive escalating T-34/SU-100/IS-2 waves, and reach one of three campaign endings. The
  finale is one attempt.

Encounters are deterministic for a run seed but spatially triggered. Optional routes can be
bypassed; mandatory cordons cannot. Noise, discoveries, objective state and finale phase wake
different contacts. Skill affects detection, dispersion and reload cadence, while concealed armour
holds its prepared position until alerted.

## Environmental storytelling

Every act receives an additive deterministic story layer even when a worldbuilder manifest exists.
It draws from the shipped PSX asset library and adds 500–650 placed traces per act: trash, grass,
tools, luggage, beds, furniture, bodies, crates, debris, supplies, barricades and wrecks.

The authored tableaus include the abandoned station hospital, the absurd supply drop, the final
butcher site, a buried T-34 line, the wall-less old house after breakfast, an abandoned canal repair,
the boys' trench, the jammed forest road, the dry Tiger, a prepared Autobahn blocking line, the
forest cemetery and the civilian/military rail column. Substantial wrecks and barricades are real
cover: they block sight, shells and the Tiger rather than existing only in the renderer.

## Combat and pressure

- Six Soviet vehicle identities with distinct rigs, silhouettes, armour and ammunition: T-34,
  SU-100, KV-2, ISU-152, IS-2 and IS-3.
- Streamed battles retain up to 24 exact-class wrecks, emissive embers, smoke and honest LOS cover,
  leaving a visible history down the route.
- Two allied Tigers form on the player in Act 3, acquire targets and fire through the normal combat
  path. Enemy pressure can kill wingmen, destroy the town and cut through the civilian column.
- Expanded volumetric blast/wreck smoke, pressure dust, dynamic blast lights, wind-bent columns,
  persistent fire, heavier muzzle/impact effects and twelve simultaneous ray-lit tank proxies.
- Seasonal precipitation and ground response: hard winter in Act 1, thaw in Act 2, wet spring at
  Halbe.

## Procedural acoustics

All required audio has a redistributable deterministic fallback. The runtime precomputes dry source
identities and applies live distance delay, head-relative stereo, LOS obstruction, village/open-snow
or steel-interior Schroeder reverb, head shadow and closed-hull low-pass transmission when the wave
reaches the listener. Dismounted sounds follow the crew rather than the parked Tiger. Concussion
ducks already-ringing tails as well as new sounds.

Non-commercial reference recordings are disabled by default. They can only be auditioned through
the explicit `KESSEL_ENABLE_NONCOMMERCIAL_AUDIO=1` opt-in. Generate the complete offline library:

```sh
cargo run -- soundbank /tmp/kessel-soundbank
```

## Verification receipt

```sh
cargo test --bin kessel
# 97 passed, 0 failed, 1 ignored (the ignored test requires a real host audio device)

cargo run -- campaign-verify /tmp/kessel-proof
# PASS — TownHeld, 38 kills, same Tiger 398c2a1d404b9d54
```

The GPU verifier builds and renders all three full worlds on Vulkan, resolves every authored combat
stream through the real director/death path, carries the same Tiger snapshot between acts, reaches
the finale and writes completion/ending frames. Close-range visual-playtest frames are under
`artifacts/codex-overhaul/`.
