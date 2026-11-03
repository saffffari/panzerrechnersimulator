# The sky through the day — 3 November 2026

The owner's direction (2 November 2026, carried into this change): "work on the sky and
environment lighting and day night cycle and sunrise/sunset transition effects. i also want to
add more stars, make the stars brighter, but keep stars to a single in game pixel size. moon and
sun discs can also be a bit bigger. and moon should have phases with 10 days moon phase cycle".

`docs/SKY_NIGHT_20261102.md` landed the ten-day month, the 1.6x discs, the 8,800-star field, the
pigment rate and AgX. This note is what came after, branch `claude/sky-daynight-20261103`; the
ledger entries are in `docs/PHYSICAL_TRUTH_AUDIT.md`, "The sky through the day".

## What was wrong

1. **Twilight lost its higher orders the moment the Sun set.** The multiple-scattering field
   (`multiple_scattering_radiance` and the cloud deck's diffuse source) used the plain cosine of
   the body's zenith angle and returned zero once the body stood below a point's horizon. Every
   point near the ground had its second-order light cut at sunset, so the antisolar sky after
   sunset, the Earth's shadow, was single scattering alone: black at the horizon on the spring
   bridge at 16:45Z (the Sun at -2.8 deg there on 12 May 1943). Under the canyon's 0.98 deck the
   cloud base lost the sunlit air above it at the same instant.
2. **The Moon had no phase on its disc.** `rm_sky_discs` drew a flat circle whose brightness alone
   followed the phase; a crescent was a dim full disc. While the Sun keyed the beam (above -1 deg)
   the Moon was not drawn at all, so it popped into the sky at sunset.
3. **The clock lurched between the phases.** The rate blended linearly over one degree; on the
   bridge the blue hour runs some seventy times slower than the night beside it, and the eye's
   pigment rate follows the clock.
4. **Far low-cloud banks were never shaded** by the terrain or the town: beyond the froxel grid
   they took the key beam through the cloud deck and their own depth only, white over a field in
   shadow at a low Sun.

## What changed

- **Chapman's effective cosine for the higher orders** (physical, a named approximation): the
  two-stream solve takes 1/Ch(x, chi), with the planet's screening past the horizon. Clear sky,
  new Moon, Stalingrad: 920 / 134.5 / 5.16 / 0.093 / 0.0024 lx at 0 / -3 / -6 / -9 / -12 deg
  against USNO 171's 984 / 66.4 / 2.99 / 0.116 / 0.0044. The Earth's shadow a degree up at -3 deg:
  28 cd/m^2, blue over red 2.0, against the zenith's 23.
- **The Moon's phase on its disc**: terminator from the Sun's direction, the sunlit part at one
  radiance carrying the ephemeris' whole irradiance, the night side in earthshine (the Earth's
  geometric albedo 0.367, its solid angle and Lambert phase from the Moon: about 1e-4 of the full
  Moon beside a crescent). Drawn by day as well.
- **Discs 2.1x** (from 1.6x). The Moon is about twelve internal pixels across at 1024 x 576.
- **Stars to magnitude 7.5**: 170 lattice cells a face, about 27,700 stars (the Bortle 1 sky).
- **Brighter stars**: each star's flux times 4 in its one pixel (the display's smaller field of
  view: a presentation choice that lights nothing), the pixel's ceiling 5x display white (was 3x).
- **The clock's pace** blends in its logarithm over 2 deg either side of each phase edge.
- **Far banks** take the depth cascades' shadow as far as the far cascade reaches (about 700 m).

## What is on and what waits for the owner

On by default (he asked for them by name, so his request is the yes): the stars to 7.5 with their
brighter single pixels, the 2.1x discs, the Moon's phase and terminator, the clock's smooth pace,
the far banks' shadow. Held behind `KESSEL_SKY_LOOK=full` until he approves the stills: the
twilight's higher orders (the Earth's shadow), the earthshine, the Moon by day.

## Stills

`tools/sky_daynight_stills.py TAG` renders, in the locked look (deltap-real, 15,15,7, 1024x576),
the eye converged per still and the clock held: a day sequence on the canyon and the bridge from
a fixed standing view (noon, +20, +8, +3, 0, -2, -4, -6, -9, -12, -18 deg), sunset and sunrise
facing the Sun, the antisolar sky (the Earth's shadow), the bridge's night sky north and south,
and the Moon across May 1943 (the same drawing as the ten-day month's). Before stills come from
the base commit with only the camera-aim option added. Files:
`/home/alex/panzer/captures/sky-daynight-sky-daynight-{before,after}_*`.

## Results

(see the status section below)

## Status
