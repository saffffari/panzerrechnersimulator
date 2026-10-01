# Canyon v2: winter battlefield footage direction

User-supplied visual target, 2026-09-30. This guides material and spatial appearance in the
fictional Gaea course; period architecture, vehicles and historical provenance remain separate.

## Sources and inspection

- [BFBS Forces News: Ukraine — How US-supplied Bradleys destroyed feared Russian T-90 tank](https://www.youtube.com/watch?v=yrrso5JDR5I), published 2024-01-19, 100 seconds.
  Downloaded to `/home/alex/panzer/reference/bradley_t90_20260930/yrrso5JDR5I.mp4` for local
  reference inspection. SHA-256: `352802a23767d366cea41963c11513a06785d9d59f561eaa5b8b4b672b364a82`.
  A 10-frame contact sheet and separate 5, 25, 45, 55, 65 and 75 second frames are beside it.
- [User's second-angle Reddit post](https://www.reddit.com/r/tanks/comments/199b36w/another_angle_of_bradley_slaughtering_the_t90/).
  Post text was readable; the browser presented a human verification challenge before video
  playback. Do not claim this second clip was inspected. Commenters' damage interpretations are
  not evidence for the simulation.

The YouTube footage was visually sampled across its duration, including the ground-level view
near 45 seconds. This was not an audio review. Drone exposure, compression, white balance and
shot changes prevent inferring calibrated material reflectance, emitter power or ammunition
performance from displayed pixels. Reference media stays out of game assets and Pi deliverables.

## Visible cues to carry into the game

1. Snow forms the light background, broken by connected traffic lanes, exposed earth, dark
   structural remains and debris. Clear white sheets beneath isolated props lose this scale.
2. Even damaged plots retain a street / frontage / yard pattern. Low remnants and missing roofs
   contribute as much as tall buildings. Use the existing period building grammar.
3. Bare trees are frequent, irregular vertical silhouettes, with low broken branches and fallen
   wood connecting them to the ground. Conifers should not dominate the village frontage.
4. Quiet surfaces are subdued grey, brown and blue-grey. The requested blue belongs mainly in
   winter sky illumination and shadow; avoid a saturated blue color filter.
5. Bright yellow-orange contacts are brief against that subdued ground. Fine divergent sparks
   and lingering smoke have different lifetimes. Do not give every impact a sustained fireball.
6. Smoke locally obscures vehicles and disperses; it is not uniform distance fog or a permanent
   screen wash. Preserve the actual impact, fire and wind inputs when revising effects.

Modern autocannon / armor behavior is not a WWII penetration model, and the clip is not evidence
of a historical event witnessed by the fictional Tiger crew. Source the older campaign's actual
architecture and aftermath from the existing Stalingrad research.

## First authoring pass

Implemented in generator versions 3–4:

- Retain the established Gaea route and existing landmarks; add nearby burned / partly collapsed
  roadside plots with checked building clearance and an open travel strip.
- Orient supplies and broken fences behind their own houses. Add bare garden trees and denser
  debris at those plots, rather than distributing every object uniformly across the floor.
- Preserve native Gaea rock and snow inputs. Bake a derived substrate and coverage map for prior
  traffic, access lanes and disturbed yards. The approximately 1.22 m texels encode materials;
  these marks do **not** add geometric ruts or dynamic track physics.
- Increase physical cloud occupancy from 0.38 to 0.82 with the same ephemeris and storm setting.
- Clear winter grass across the same authored traffic/yard masks; retain winter stems beside them.
- Replace square aftermath stain tiles with irregular terrain-following pigment contours and
  place fallen trunks along their measured long axis rather than repeating the source's leaning pose.

Acceptance uses identical game camera positions before / after, a nearby yard view and aerial
layout inspection. A successful generator or compilation alone is not visual acceptance. The
remaining impact, cooling and audio work is tracked by the armor/audio handoff, not implied done
by this environment pass.
