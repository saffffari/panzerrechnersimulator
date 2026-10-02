# Native pixel-grid dither at 800×450

The new `screen_bayer8` mode uses one 8×8 Bayer threshold indexed by the native fragment pixel.
That single threshold is reused by palette and final LDR quantization. It does not use the sphere
mode's four-subpixel average. The internal rendering resolution, HDR lighting, eye response,
geometry and final nearest-neighbor presentation are unchanged.

Select **Graphics → Retro → Dither pattern → Bayer 8 / pixel grid**, or launch with:

```
KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8
```

Saved preferences include the selector and survive restart. Older preference files without that
field preserve an explicit launch choice. Existing explicit saved selector values remain user
preferences. `bayer4`, `blue_noise64` and `sphere_bayer8` remain available. The frozen-frame review
helper also accepts comparison mode 3. No running process, live configuration or play-session
state was changed by this source update.

The prior sphere mode maps a world-oriented angular grid onto the screen. Its cell scale only
matches one native pixel near the image center and its perspective projection changes the cell
shape toward the edges. It averages four thresholded subpixel values in each native pixel, first
for the palette and then for 8-bit LDR. That creates intermediate values and weakens the crisp
screen-grid pattern. This is display reproduction, not an HDR geometry blur.

The presentation sampler already uses nearest minification and magnification. At a 5120×2880
output, 800×450 is a 6.4× scale, so native pixels occupy alternating 6/7 physical display pixels.
That produces uneven block widths, not bilinear blur. This patch adds no letterboxing and changes
neither the requested 800×450 resolution nor fullscreen behavior. An integer-scale presentation
option would be a separate user choice.

CPU checks validate the complete shader and 128-byte eye/display ABI in both checkouts. Graphics
preference tests cover selector cycling, serialization, invalid-value rejection and existing
menu row hit testing. GPU visual verification is pending while the user plays; a compiled shader
is not evidence of the final look.
