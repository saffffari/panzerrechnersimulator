# Cabin lighting and distant blizzard firefight revision

This follow-up responds to the visible interior flash wash, outdoor blizzard inside the tank,
excessive viewport bloom, weak audible bass in some destruction events, and the request for a
larger, more distant fight. The earlier v6 films remain intact for comparison.

## Changes under verification

- The closed breech no longer invents a lamp after the main gun fires. The actual muzzle stays
  at its existing source power. Armor visibility determines which surfaces it can illuminate.
- Every admitted point source receives source-geometry shadow maps. Registration identifies a
  fixture's emitting mesh and torch direction; it no longer grants permission to cast shadows.
  This includes main guns, impacts, fire, incandescent fragments, hand weapons, and authored
  scene/building/editor sources. Smoke uses the same point-light visibility.
- Aperture bloom retains the source RGB but uses the ordinary nine-tap camera kernel in both
  views, replacing the interior-only 33-tap kernel. Aperture response is 0.14 instead of 0.40.
  This changes optical reproduction, not source intensity or surface lighting.
- The distant capture profile stages six ordinary T-34 actors at 140, 155, 170, 185, 200 and
  220 m. Each opening lane must pass real terrain/cover LOS and gun-depression validation.
  The platoon starts with 6.0–18.5 s remaining on its staggered opening reloads; after that
  normal AI/gun timing applies. The former two-tank profile remains available for a controlled
  before/after comparison.
- The catastrophe source and same-compartment acoustic routing are documented separately in
  CATASTROPHIC_AUDIO_20261001.md. Source spectra remain authored sound design, not measured
  historical pressure waveforms. Waveform measurements do not establish listening approval.

## Reproduction

Use the normal release binary with `KESSEL_RENDERER=shader KESSEL_NO_RT=1
KESSEL_INTERNAL_RES=640x360` and `blizzard-battle captures/<unique-prefix> 24
inside platoon` (or `cupola` / no view token for external). Add `preview` for five video
frames per second; physics and the live crew audio still run at their normal rates. Omit
`platoon` for the original comparison geometry. Captures refuse to overwrite existing files.

## Acceptance

Pending this revision's composed-shader checks, focused geometry/audio tests, native capture
receipts, video decode checks and visible frame review. A scenario setup or a successful build
alone is not a delivered firefight. No result is claimed from these instructions.
