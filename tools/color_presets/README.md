# Reference chroma presets

`reference-chroma-115.json` and `reference-chroma-125.json` set the native display saturation to
1.15 and 1.25 respectively. The 1.15 option is now the native default; 1.25 remains a stronger review option. They preserve the
physical sky, atmosphere, lights and materials, as well as the saved exposure, warmth, gamma,
dither, resolution and simulation settings.

Create an alternate saved range state without modifying the original:

```sh
python tools/apply_color_preset.py tools/color_presets/reference-chroma-115.json path/to/fea_range_state.json path/to/fea_range_state-chroma115.json
```

The output uses the existing FEA range state schema. It can be loaded by the range editor or
placed in a film plan's `settings` object. The helper changes only `look.grade[1]` and refuses to
overwrite an existing file. It does not change any running editor or install a default.

The legacy `KESSEL_REAL_SATURATION=0.85` is a different, earlier AgX stage and remains unchanged.
The exported video copies used FFmpeg `eq=saturation=1.25`; that post-export operation is not
pixel-identical to the native display grade. Native comparisons are captured with
`KESSEL_SHOT_LOOK_REVIEW=/absolute/flat/output/prefix` on the screenshot command, which renders
every treatment from one frozen HDR frame and the same GPU eye state.

Reference atmosphere: `spring-bridge-haze-test-aerosol`, 1943-05-12, AOD550 0.2, Angstrom 1.6,
aerosol scale height 1500 m, fog 0.00045/m, overcast 0.06, storm 0. The screenshot command's last
positional argument is a fog override: pass `0.00045` to preserve this map's authored fog.

## Native dither correction

The deltaP modes, including `look=6` (`deltap-real`), now honor an explicit blue-noise selection:
`KESSEL_DITHER_PATTERN=blue_noise64`, `r.display_dither_pattern=1.0`, or `look.dither=1` in a saved
range state. Previously `dp_threshold` ignored that selection and always used Bayer8. Other
pattern selections retain the historical deltaP Bayer8 behavior. The saturation-only helper
above deliberately preserves the state's existing dither choice.

For the outdoor reference look, use `real=[1.35,0.85,15,7]`, display grade `[1,1.15,0,1]`, and
blue-noise mode 1. This is deterministic spatial grain at native pixels, without blurring,
temporal randomization, a new palette, or relighting the scene. Coarser `7,7,4` quantization
remains available but has larger neighboring color differences in smooth gradients.

Two frozen native comparisons (daylight and dawn) passed repeated-capture RGBA byte identity.
Mean RGB difference from ordered dither was below 0.011 of one 8-bit code per channel. The
largest coherent high-frequency spectral peak's share in the sampled sky fell from 66.3% to
0.41% in daylight and 51.0% to 0.22% at dawn. Production WGSL passed Naga semantic validation.
Receipts: `target/temporal-impact-20261104/dither-fix/`. Owner gallery:
`/windows/shop/panzerrechnersystem/captures/screenshots/voxel-runtime-daylight-openfield-20261104-gallery.html`.

`KESSEL_SHOT_DITHER_REVIEW=/absolute/flat/output/prefix` on the screenshot command captures the
ordered and blue-noise treatments from the exact same HDR frame and eye state, and repeats the
pair to assert determinism. No world time, lighting or simulation step advances between them.
