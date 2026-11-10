# One exposure for every view (design, 10 November 2026)

Status: **design; built behind `KESSEL_EXPOSURE=shared`, off by default until the owner picks it.**
The present eye (`shaders/exposure.wgsl`, `src/observer.rs`) is kept whole as the default.

## What the owner sees, and why

| Complaint | Cause in the code today |
|---|---|
| The outside looks different through every sight, block and with the head out | Every view meters its own frame. The commander's view (slot 0) is an *inside* eye when buttoned up (it meters the cabin, damped to a quarter about a 20 s reference), an *outside* eye with the head out. The gunner's sight and the binoculars have their own eye (slot 3), monocular, metering only their narrow field. Four adaptations of one landscape. |
| At night the gun sight is pitch black while the blocks are not | The sight's eye meters a 6-25 deg field on the gun line. A fire or the muzzle flash fills much of it, and the optic eye has no small-bright-source treatment (only the outdoor eye has it), so it adapts to the fire and the land goes black. |
| The gun sight turns black when bright lights flash | The flash afterimage (`flash_afterimage`, lanes 10-11) is added to the cone and rod adaptation themselves, so after a flash at night the whole mapping falls for seconds (energy memory 180 s, decay t^-2.5). Leaving the sight for half a second "cures" it (the optic re-snap clears the flash). |
| The interior is too bright at night | The cabin lamps burn at one power day and night; the cabin eye adapts to the cabin, so the ports' world is far below it. |
| Star things on the optics | Lamp reflections on the scratched vision-block glass (`glass_scratches.wgsl` `scratch_fan`: each groove's fan of reflection, grooves at several angles) give a star round every lamp glint; the TZF lens's ghost and field-edge softening are four-tap axial crosses (`rm_lens.wgsl`). |

## The scheme

1. **One outside adaptation.** The crew's eyes are adapted to the day outside, not to whatever one
   view's frame happens to contain. Its level is the outside world's own: the luminance of the
   ground under this frame's sky (the sun's and sky's light on the snow or earth, its albedo, the
   cloud), computed with the sky each frame (`renderer_raymarch.rs` prepare). Every outside view
   (head out, the gun sight at each magnification, the binoculars, the blocks and periscopes, the
   driver's visor) is exposed from this one level, adapting to it at the eye's own rates. A fire
   or a flash in one view does not move it.
2. **Each optic only takes its own light loss and field.** Through the TZF (transmission about
   0.5) the land is a little darker than with the head out, never re-exposed. The vision blocks'
   glass and the periscopes keep their measured transmission (raster relays, already).
3. **A flash dims, it never blinds for seconds.** A flash's afterimage may lift the adaptation by
   at most 0.3 log units (half a stop over a stop) and is gone within about a second; nothing
   blacks out an optic.
4. **The cabin.** Buttoned up, the eye adapts to the cabin, but no further than 1.2 log units
   (four stops) below the outside level: the blocks' world stays readable, the cabin is seen at
   the darkest the eye can take with a bright port in view.
5. **The lamps follow the day (the owner's proposal).** The cabin lamps' power follows the outside
   level slowly (one minute), as the square root of it against a dull overcast day, between 15 %
   and 120 % of the fixture's power: dimmer at night, brighter by day, too slowly to notice.
   It is a real emitter's power, so the image's light stays physical; it is named in the ledger.
6. **Stars.** The glass's groove glints are spread over the stroke's angle instead of its few
   directions, and the lens's ghost and field-edge taps become a disc: no star or cross round a
   bright point anywhere.

## What stays the same

The default eye (`KESSEL_EXPOSURE` unset), the look's colour steps and grid, bloom, the crush. The
switch changes only what each view's eye adapts to, the flash's hold on it, the lamps' power and
the two star sources.

## Tests

The same moment from every view (commander head out, the cupola blocks, the gun sight at each
magnification, the driver's visor, the loader's periscope), by day, at dusk and at night with a
burning wreck in view, default against `KESSEL_EXPOSURE=shared`; a flash in the sight at night,
frame by frame for three seconds.
