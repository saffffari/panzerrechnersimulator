# The deltaP look (`KESSEL_LOOK=deltap`), 1 November 2026

The owner asked for "a version of the panzer renderer that is based on" deltaP, their raymarched
cockpit sim at `/home/alex/deltaP/.claude/worktrees/v1-steps` (the stills of 2026-10-09: warm
amber practicals against cold blue ports, a fine ordered dither over a 1024x576 frame, soft
pyramid glow round every lamp). `docs/DELTAP_RENDERER_AUDIT_20261031.md` already settled what
*not* to take: deltaP's world is a signed-distance field with art-directed light shortcuts
(compartment-gated lamp shadows, gain 3 haze, an ambient floor), which the second principle
forbids here. What *can* be taken, exactly, is deltaP's display stage: light transport and display
encoding are separate jobs in both engines (deltaP `march.wgsl` -> `bloom.wgsl` -> `crush.wgsl`;
here the raymarch frame -> `stylized_bloom.wgsl` -> `crush.wgsl`), so the look moves across as a
display-stage variant on the raymarch backend without touching an emitter, a material or the
medium.

## What the look is

Selected by `KESSEL_LOOK` (read once at renderer construction, `renderer.rs`
`configured_look`); the demo launcher's `--look` flag sets it and defaults the grid to 1024x576,
deltaP's own frame, which is exactly 5x on the owner's 5120x2880 panel (deltaP's `INTERNAL_W/H`
comment: "1024x576 x 5 = 5120x2880").

| `KESSEL_LOOK` | crush quantizer (after the eye, the grade and ACES) | bloom |
| --- | --- | --- |
| unset, `standard` | the existing 31-level posterize with the chosen dither pattern | compact halo + CIE veil (unchanged) |
| `deltap` | **dithered colour**: each channel to 8 steps on a square-root curve over display gamma (`sqrt(encode(lin))`, `floor(c*8 + th)/8`, squared back), the full 64-step Bayer-8 threshold indexed by the internal texel; no far-field fade | deltaP pyramid |
| `deltap-pico8` | **PICO-8 32** (the sixteen plus the console's secret sixteen): exhaustive best-pair search over all 528 pairs, pair chosen in OKLab, mixing ratio solved in linear light, pair-distance penalty 0.12, ratio snapped to eighths with 9 % dead zones | deltaP pyramid |
| `deltap-raw` | none: the tonemapped frame (deltaP's `--palette 0` A/B, "how much of the look is the palette") | deltaP pyramid |

The crush functions are ports of deltaP `crush.wgsl` (`crush_px` mode 3, `dither_best_pair`,
`linear_to_oklab`, `bayer8`); their comments there explain each colour-space choice and the two
bugs that shaped them (luma-weighted RGB distance sends a blue sky to grey; greedy pairing cannot
reach navy + bright blue). The PICO-8 entries are stored display-encoded and written verbatim, as
deltaP does, so a `deltap-pico8` frame is a genuine 32-colour frame.

**The deltaP pyramid bloom** (`stylized_bloom.wgsl` `fs_dp_prefilter`, `fs_dp_down`,
`fs_dp_up`, `fs_dp_composite`; a port of deltaP `bloom.wgsl`): a 13-tap Jimenez downsample with a
soft-knee bright pass in exposed units and a Karis average in the first level, five halvings of
the half-resolution bright pass (the existing `bloom_levels` pyramid, 1/4 .. 1/64 of the grid),
then 3x3 tent upsamples added back level by level, and the sum added to the HDR frame before the
tonemap. deltaP's own values (knee 0.55 +- 0.35, cap 1.0, 0.9 per octave, strength 0.8) were too
much for this renderer's exposed range: the owner's first expedition playtest had every lamp a
wide blur and the vision ports flooding the cabin, and a burning T-34 was a soft ball in a mottled
plateau. Since the same evening the knee is 1.0 +- 0.5 exposed, the cap 0.5, the octaves fall at
0.65 and the sum enters at 0.4 (`DP_THRESH`, `DP_KNEE`, `DP_CAP`, `DP_TAIL`, `DELTAP_BLOOM`). The halo
therefore goes through the same quantizer and Bayer threshold as everything else and comes out as
dithered rings round a lamp rather than a smooth gradient over the pixel art. Under the look the
compact halo and the CIE veil do not run; the pyramid's falling tail stands in for both.

What stays Panzer's: the crewman's eye (cone exposure, rods, the cabin draw-in), the ambient
white balance, the display grade, the peak-channel ACES shoulder, the optic masks, the reticle
and the nearest blit. deltaP's night-vision model is not ported: this renderer has its own rod
channel (`observer.rs`), and two observers in one display would be wrong.

## Added 2 November 2026: the tonemap and the pastel palette

- `KESSEL_TONEMAP=agx|aces` (default AgX; `crush.wgsl` `tonemap`, the look lane's z): the owner
  found the sunset "oversaturated, deep fried" under the ACES fit, whose shoulder skews orange to
  yellow; AgX desaturates bright colours toward white and keeps hue.
- `KESSEL_LOOK=deltap-pastel` (mode 4, `DP_PASTEL`): a thirty-two colour palette authored for this
  game (green-blues and teals first: "i don't really like these pinks and purples. we want more
  green blue"), dithered by the same OKLab best-pair search as the PICO-8 mode. The demo
  launcher's default look since the same night ("agx is too realistic, we need the pastel and
  dither"); the palette's sky ramp rings round a high Sun, which the owner half likes.
- `KESSEL_LOOK=deltap-tinted` (mode 5, `dp_tinted`, the launcher's default since the owner saw the
  two: "i like the stylized color look of the pastel version but the agx realism also is nice,
  these pastel tones are too stylized, somewhere in the middle, less pink and purple"): the
  dithered-colour mode with each pixel's OKLab hue and chroma pulled toward the nearest pastel
  entry's by `KESSEL_PALETTE_PULL` (0.7 unless set, after the 0.5 stills still read mauve at dusk), lightness kept: continuous tones in the
  palette's colour family, magentas and purples sliding to its teals, greys and creams. Outdoors
  only: inside the hull the pull is zero (the buttoned-up view's eye_range_strength), so the
  cabin keeps the plain dithered colour, as it keeps its own rod hue and gain. (It went inside for
  a few hours on 2 November 2026 and muted the annunciators' green; back outside.)
- `KESSEL_CABIN_LOOK=pastel` (the launcher's `--cabin-look pastel`; the crush uniform's
  `cabin_look` lane): in the tinted look the cabin quantizes to the pastel palette itself (mode 4's
  quantizer) while the outside keeps the pulled continuous tones. Offered after the owner called a
  cabin still under the palette "a fantastic shot" (2 November 2026); `plain` is the default.
- `KESSEL_PALETTE_SMOOTH=0..1` (the look lane's w in the palette modes): 0 keeps deltaP's flat bands between palette
  entries (the pair penalty, the ratio's dead zones and eighths); 1 dithers across them (a tenth
  of the penalty, narrow dead zones, sixteenths), the smoother variant to compare.

## Physical truth

- The look changes display encoding and the eye's glare only. No emitter, material, medium,
  preset budget or simulation value reads `KESSEL_LOOK`. Gate: `tests/display_quantization.rs`
  and `tests/dither_motion.rs` keep validating the complete crush shader; the Crush uniform grew
  by one appended `look` lane (offset 128, span 144) with every earlier lane at its offset.
- Named approximations under the look (ledger entries in `docs/PHYSICAL_TRUTH_AUDIT.md`):
  the bloom's bright-pass knee and per-texel cap are deltaP's authored values, not a measured
  point-spread function; the CIE veil is omitted while the look is on.
- The 8-step dithered colour and the 32-colour palette are recording-medium choices in the sense
  of the existing retro quantizer: they discard tonal precision, never add light.

## Cost

At 1024x576 the per-pixel passes cost about 2.9x their 640x360 figure
(`docs/RAYMARCHING_ENGINE.md`, "Internal resolution"); the pyramid bloom is eleven small passes
(one prefilter, five downs, five ups) in place of the four halo passes and the seven glare passes.
`deltap-pico8` evaluates 528 pairs per pixel with an OKLab conversion each: about 311 M pair
evaluations a frame at 1024x576, which deltaP's author measured as negligible at 160x240 on this
GPU and which is the one part of the look worth watching in `KESSEL_GPU_PROFILE=1` ("crush").
Drop to `--res 640x360` (8x on the panel) if the frame does not hold.

## Run

```sh
tools/launch_demo_20261101.sh --expedition --look deltap          # 1024x576, dithered colour
tools/launch_demo_20261101.sh --canyon --look deltap-pico8        # the 32-colour frame
tools/launch_demo_20261101.sh --canyon --look deltap --res 640x360
KESSEL_LOOK=deltap KESSEL_RENDERER=raymarch KESSEL_INTERNAL_RES=1024x576 \
  KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem target/release/panzerrechnersystem \
  gfxbench deltap 60 captures/deltap_look_20261101.png high        # headless still
```

Stills of the first pass are `captures/deltap_look_20261101_*.png` (standard / deltap /
deltap-pico8 / deltap-raw at the same frame), with the receipt in this document's history.

## Added 2 November 2026 (branch `realism_render_test`): the realism look

- `KESSEL_LOOK=deltap-real` (mode 6): the owner asked for "more contrast, less stylized colour,
  realism, and rely on the dithering", and preferred the standard display's colour to the deltaP
  and tinted looks on the same clip. The physical frame's own colour (no palette pull), through
  AgX with a steeper slope through the log-encoded mid grey (`KESSEL_REAL_CONTRAST`, 1.35 unless
  set; 1 is AgX as shipped) and a little less saturation after the outset
  (`KESSEL_REAL_SATURATION`, 0.85), then deltaP's dithered colour with
  `KESSEL_REAL_STEPS` steps per channel (8 unless set): 3 gives 4 levels per channel, 64 colours;
  8 gives 9 levels, 729 colours (deltaP's); 31 gives 32 levels, 32,768 (the standard posterize's
  count, on deltaP's square-root curve and full Bayer amplitude). The crush uniform's appended
  `real` lane (offset 160; span 176). The demo launcher defaults to it on this branch only.
- Colour counts of the other looks, for reference: standard 32 levels per channel in display
  gamma (32,768, the far field fading back toward continuous), deltaP and tinted 9 levels on a
  square-root curve (729), pastel and PICO-8 a fixed 32.
- Comparison at 1280x720 on the fea-film turret_ring clip: `ring_realism_compare_1280_20261102.png`
  and its 3x crop in the owner's captures folder.
