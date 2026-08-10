# 05 — Tunable PS1 / Retro Aesthetic (UE 5.8)

**Project:** Kessel (first-person single-crew tank-escape sim)
**Milestone:** M1
**Scope of this doc:** A dial-able retro pipeline that slides from *full-retro PS1*
to *modern-but-grimy*. Covers internal render-resolution control, defeating
TAA/TSR for crunchier output, vertex snapping via material World Position Offset,
affine texture-warp feasibility, a posterize + ordered-dither + limited-palette
post-process material, and exposing every strength via a Material Parameter
Collection (MPC).

All engine APIs below were verified against the on-disk UE 5.8 source
(`C:/Program Files/Epic Games/UE_5.8/Engine/Source`). Anchors are `file:line`.
Anything not confirmed on disk is tagged **ASSUMPTION**.

---

## 0. Design principle: ONE master dial + per-feature dials

Every retro effect is gated by a `MasterRetro` scalar (0..1) *and* its own
per-feature scalar, both living in a single MPC (`MPC_Retro`). Materials read
the MPC via `CollectionParameter` nodes; render-resolution / AA live in CVars
driven from a Blueprint/console at runtime. This means the *exact* graphics
style stays flexible and tunable as the brief demands — `MasterRetro = 1`
gives whiteout-fog PS1 crunch; `MasterRetro = 0.25` gives a grimy modern image
with just a hint of quantization.

The retro look is the composite of four independently dial-able layers:

| Layer | Mechanism | Where tuned |
|---|---|---|
| A. Low internal resolution + nearest upscale | CVars | `r.ScreenPercentage`, `r.Upscale.Quality`, `r.AntiAliasingMethod` |
| B. Vertex snapping ("jitter") | Surface material WPO | MPC scalars, applied in every world material (via Material Function) |
| C. Crunchy texels | per-texture Nearest filter + mip bias | asset settings + `r.Streaming.MipBias` |
| D. Posterize + ordered dither + limited palette | Post-Process material | MPC scalars/vectors |

Affine (perspective-incorrect) texture warp is treated separately in §4 — it is
**NOT achievable in the material graph alone** and is descoped for M1.

---

## 1. Layer A — Internal render resolution + nearest-neighbour upscale

### 1.1 The mechanism (confirmed)

UE renders the 3D scene at the *primary screen percentage*, then spatially
upscales to the display. The primary screen percentage CVar:

- `r.ScreenPercentage` — float, `>0 && <=100` (supersampling `>100` allowed,
  `<=0` means "use defaults"). "To render in lower resolution and upscale for
  better performance."
  Confirmed: `Runtime/Engine/Private/LegacyScreenPercentageDriver.cpp:37-43`.

The spatial upscaler's filter kernel:

- `r.Upscale.Quality` — int. **`0 = Nearest filtering`**, `1 = Simple Bilinear`,
  `3 = 5-tap Catmull-Rom (default)`, … `5 = Gaussian`.
  Confirmed: `Runtime/Renderer/Private/PostProcess/PostProcessUpscale.cpp:21-31`;
  the `EUpscaleMethod::Nearest` branch is real
  (`PostProcessUpscale.cpp:110,162`).

**Setting `r.Upscale.Quality 0` gives the nearest-neighbour, blocky upscale we
want — but ONLY when the spatial upscaler actually runs (see §1.2).**

A second, downstream upscale stage exists (renders viewport → back buffer):

- `r.SecondaryScreenPercentage.GameViewport` — float.
  Confirmed referenced at
  `Developer/FunctionalTesting/Private/AutomationBlueprintFunctionLibrary.cpp:250`
  and `Runtime/ApplicationCore/Private/Android/AndroidWindow.cpp:620`.
  Use the *primary* `r.ScreenPercentage` for the main effect; the secondary stage
  is a fallback/second crunch and is **ASSUMPTION** as to whether it honors
  `r.Upscale.Quality` for the secondary pass — do not rely on it for M1.

### 1.2 CRITICAL interaction: TAA/TSR steal the upscale

When the anti-aliasing method is **temporal** (TAA or TSR), upscaling is done by
the *temporal upsampler*, not the spatial `r.Upscale` path — so
`r.Upscale.Quality 0` is ignored and you get a smooth, ghosted, decidedly
non-retro image. The engine itself classifies these methods as temporal:

- `IsTemporalAccumulationBasedMethod()` returns true for `AAM_TemporalAA` and
  `AAM_TSR`. Confirmed: `Runtime/Engine/Public/SceneUtils.h:56-60`.

So for crunch you MUST select a non-temporal AA. The enum:

```
AAM_None        = 0
AAM_FXAA        = 1
AAM_TemporalAA  = 2
AAM_MSAA        = 3   // forward shading only; sample count via r.MSAACount
AAM_TSR         = 4
AAM_SMAA        = 5
```
Confirmed: `Runtime/Engine/Public/SceneUtils.h:44-54` (note SMAA exists in 5.8).
Runtime CVar that selects it: `r.AntiAliasingMethod`
(confirmed used at `Runtime/Renderer/Private/RendererScene.cpp:890`).

### 1.3 Full-retro resolution recipe (CVars)

```
r.AntiAliasingMethod 0      ; None  -> spatial upscaler runs, no temporal smear
r.ScreenPercentage  25      ; 1920x1080 -> 480x270 internal (integer 4x)
r.Upscale.Quality   0       ; nearest-neighbour upscale = blocky pixels
r.TemporalAA.Upsampling 0   ; belt-and-suspenders; defeat temporal upsample (ASSUMPTION: CVar name)
r.SecondaryScreenPercentage.GameViewport 100
```

Pick `r.ScreenPercentage` so the internal resolution is an **integer divisor**
of the output, otherwise nearest-neighbour produces uneven (shimmering) pixel
blocks. Reference table for a 1920×1080 output:

| ScreenPercentage | Internal res | Integer multiple? |
|---|---|---|
| 50 | 960×540 | yes (2×) |
| 33.333 | 640×360 | yes (3×) |
| 25 | 480×270 | yes (4×) |
| 16.6667 | 320×180 | yes (6×) — most "PS1" |

"Modern-but-grimy" end of the dial: `r.AntiAliasingMethod 1` (FXAA) or `2`/`4`
(TAA/TSR) + `r.ScreenPercentage 100` + `r.Upscale.Quality 3`. The Blueprint that
owns `MasterRetro` should `lerp` `r.ScreenPercentage` between e.g. 16.667 and 100
and switch AA method at a threshold.

### 1.4 Supporting "flatten the image" CVars (turn off modern eye-candy)

These are standard scalability/feature CVars; set for the full-retro end:
```
r.MotionBlurQuality 0
r.DepthOfFieldQuality 0
r.DefaultFeature.Bloom 0          ; (or keep a tiny bloom for the emissive-in-fog glow)
r.DefaultFeature.AutoExposure 0   ; fixed exposure reads more "fixed-function"
r.Tonemapper.Quality 0           ; ASSUMPTION exact name; reduces filmic grain/sharpen
r.Tonemapper.Sharpen 0
r.Streaming.MipBias 0..2          ; positive = blurrier textures (fake low-res); confirmed family exists
```
Keep volumetric fog + emissive bloom ON regardless of `MasterRetro` — that fog
glow is the aesthetic north star (see doc 04 fog spec) and is *orthogonal* to
the pixel crunch.

---

## 2. Layer B — Vertex snapping (PS1 "vertex jitter") via WPO

PS1 hardware transformed vertices to integer screen coordinates with no
sub-pixel precision, so vertices visibly snap/swim as the camera moves. We
reproduce this with **World Position Offset (WPO)** on every world-surface
material.

Confirmed the material output pin exists: `MP_WorldPositionOffset`
(`Runtime/Engine/Public/SceneTypes.h:161`, DisplayName "World Position Offset").
Material domain for surfaces: `MD_Surface`
(`Runtime/Engine/Public/MaterialDomain.h:15`).

Implement once as a **Material Function** (`MF_VertexSnap`) and plug its output
into the WPO of every opaque/masked material, so the whole world snaps
coherently and the dial is global.

### 2.1 Recipe — screen/view-space snap (faithful, recommended)

The authentic look snaps the *projected* position to a low-res grid, not a world
grid. Compute in view space, snap the perspective-divided XY, transform back, and
output the delta as WPO:

```
Nodes (MF_VertexSnap), output = float3 WPO:

1.  P_world   = AbsoluteWorldPosition (CFG: "Absolute World Position (Excluding
                Material Shader Offsets)")
2.  P_view    = TransformPosition(P_world, World Space -> View Space)   // float3, cm
3.  invZ      = 1 / P_view.z                                            // (z = forward)
4.  screenXY  = P_view.xy * invZ                                        // perspective divide
5.  GridDensity = CollectionParameter(MPC_Retro, "SnapGridDensity")     // e.g. 120..240
6.  snapXY    = round(screenXY * GridDensity) / GridDensity             // quantize to grid
7.  P_view2   = MakeFloat3( snapXY * P_view.z , P_view.z )              // un-divide
8.  P_world2  = TransformPosition(P_view2, View Space -> World Space)
9.  delta     = P_world2 - P_world
10. Strength  = CollectionParameter(MPC_Retro,"SnapStrength")
                * CollectionParameter(MPC_Retro,"MasterRetro")
11. WPO       = delta * Strength
```

- `TransformPosition` node supports World↔View↔Local; "round" is the `Round`
  material node. All stock nodes — no Custom HLSL required.
- Drive `SnapGridDensity` from the *internal* render resolution so the snap grid
  tracks the pixel grid (tie it to `r.ScreenPercentage` in the owning BP).
- `SnapStrength`/`MasterRetro` at 0 → WPO delta is zero → vanilla smooth
  geometry. This is the dial.

### 2.2 Cheaper alternative — world-grid snap

If view-space transforms are too costly on dense meshes, snap local position to a
coarse world grid instead (less faithful — grid is world-locked, not
screen-locked, so vertices don't "swim" with camera the same way):

```
P_local  = LocalPosition
Gw       = CollectionParameter(MPC_Retro,"WorldSnapSize")   // cm, e.g. 1..4
snap     = round(P_local / Gw) * Gw
delta    = TransformVector(snap - P_local, Local -> World)
WPO      = delta * SnapStrength * MasterRetro
```

Recommend the §2.1 screen-space version for the hero look; expose
`SnapMode` (0=off,1=world,2=screen) as an MPC scalar if both are built.

### 2.3 Tunable knobs for Layer B
`SnapStrength`, `SnapGridDensity` (screen mode), `WorldSnapSize` (world mode),
all multiplied by `MasterRetro`.

---

## 3. Layer C — Crunchy texels

PS1 textures were unfiltered and low-res. Two parts:

1. **Nearest texture sampling.** Set each world texture's `Filter = Nearest` in
   the texture asset, OR in-material use a `TextureObject` + a `Sampler` whose
   Sampler Source / filter is set to nearest. There is **no single reliable CVar
   to force all samplers to point filtering** (do it per-asset / per-material) —
   **ASSUMPTION** that no global override exists; treat as asset-side policy.
2. **Fake low resolution / mip bias.** Raise `r.Streaming.MipBias` (positive →
   coarser mip = blurrier) for a soft retro read, or author intentionally tiny
   (64–256 px) textures. For the *crunchy* read, prefer small textures + Nearest
   filter rather than mip bias (which blurs).

Dial: tie a `TexCrunch` MPC scalar to a material `Lerp` between a filtered and a
point-sampled tap if you want a runtime slide; otherwise this layer is mostly
asset policy and can be a fixed choice.

---

## 4. Affine / perspective-incorrect texture warp — feasibility

**Verdict: NOT feasible in the UE material graph alone. Descope for M1.**

Reasoning: PS1 lacked perspective-correct texture mapping, so UVs were
interpolated *affinely* (linearly in screen space), producing the signature
texture "wobble" on large near-camera polygons. Modern GPUs interpolate all
interpolators **perspective-correct** by default; defeating it requires the HLSL
`noperspective` interpolator qualifier on the UV TEXCOORD.

- You cannot cancel it from inside the material by pre-multiplying by W: passing
  `UV*W` and `W` as separate interpolators then dividing in the pixel shader
  yields `(perspective-correct UV*W)/(perspective-correct W) = perspective-correct
  UV` again — the divide cancels exactly. The only fix is `noperspective`, which
  the Material Editor does **not** expose (the `Vertex Interpolator` node emits
  default perspective-correct interpolators).
- A genuine implementation needs an **engine-source change**: add a
  `noperspective`-qualified TEXCOORD to a custom vertex factory (clone of
  `LocalVertexFactory.ush`) and route material UVs through it — out of scope for
  a no-compile design phase, and high-maintenance.
- Practical approximation if ever desired: heavily tessellate large near-camera
  surfaces so per-triangle affine error is small — but that *reduces* the
  wobble, i.e. the opposite of the goal, so it is not a real substitute.

**Recommendation:** rely on Layers A + B + D for the PS1 read; they deliver the
overwhelming majority of the look. Log affine warp as a stretch item requiring a
custom vertex factory.

---

## 5. Layer D — Post-process material: posterize + ordered dither + limited palette

A single Post-Process material does color quantization, ordered dithering, and
optional palette mapping on the final tonemapped image.

### 5.1 Material settings (confirmed enums)

- **Material Domain = Post Process** → `MD_PostProcess`
  (`Runtime/Engine/Public/MaterialDomain.h:23`).
- **Blendable Location = "Scene Color After Tonemapping"** → `BL_SceneColorAfterTonemapping`
  (`Runtime/Engine/Classes/Engine/BlendableInterface.h:75`).
  **Note the UE 5.4 rename:** the old name was `BL_AfterTonemapping`; in 5.8 use
  `BL_SceneColorAfterTonemapping` (the deprecated alias is at
  `BlendableInterface.h:82`). Operating on the LDR post-tonemap image keeps the
  quantization in a perceptually flat 0..1 space, which is what we want for a
  fixed palette.
- **Output** the result to **Emissive Color** (the only active output for a PP
  material).
- Read the scene via a `SceneTexture` node, input
  **`PostProcessInput0`** → `PPI_PostProcessInput0`
  (`Runtime/Engine/Public/MaterialSceneTextureId.h:43`; `PPI_SceneColor` at
  line 15 is the alternative but `PostProcessInput0` is the correct input for a
  chained PP material).
- Attach via a Post Process Volume (Unbound) → Post Process Materials array, or a
  Blendable on the camera. Blend Weight can itself be driven by `MasterRetro`.

### 5.2 Node recipe

```
INPUT
  SceneCol = SceneTexture:PostProcessInput0 .Color (rgb)          // tonemapped LDR
  UV       = SceneTexture:PostProcessInput0 .UV    (or ViewportUV)

--- master/strength reads from MPC ---
  Master   = CollectionParameter(MPC_Retro,"MasterRetro")
  Levels   = CollectionParameter(MPC_Retro,"ColorLevels")      // e.g. 4..32 per channel
  DitherK  = CollectionParameter(MPC_Retro,"DitherStrength")   // 0..1
  PalK     = CollectionParameter(MPC_Retro,"PaletteStrength")  // 0..1
  LowRes   = CollectionParameter(MPC_Retro,"DitherGridRes")    // vec2, the internal res

--- 1) integer pixel coordinate on the LOW-RES grid (so dither matches pixels) ---
  pixCoord = floor(UV * LowRes)                                  // float2

--- 2) ordered dither threshold from a Bayer matrix (Custom HLSL node) ---
  // Custom node "Bayer4" inputs: float2 pixCoord ; output float (0..1)
  // (Bayer8 variant for finer dither — pick via BayerSize scalar / two nodes + lerp)
  bayer    = Custom(Bayer4, pixCoord)                            // 0..1
  // center the threshold and scale to one quantization step:
  ditherOffset = (bayer - 0.5) * (1.0 / Levels) * DitherK

--- 3) posterize / quantize each channel ---
  c0       = SceneCol + ditherOffset
  quant    = floor(c0 * (Levels - 1) + 0.5) / (Levels - 1)       // rounds to nearest level

--- 4) optional limited palette (nearest-match) ---
  // Custom node "PaletteSnap": loops over a small const palette array (or samples
  // a 1xN palette texture) and returns nearest color to 'quant'.
  pal      = Custom(PaletteSnap, quant)                          // rgb
  graded   = lerp(quant, pal, PalK)

--- 5) master blend back to original so the whole effect dials 0..1 ---
  outRGB   = lerp(SceneCol, graded, Master)

OUTPUT -> Emissive Color = outRGB
```

`Bayer4` Custom HLSL (4×4 ordered dither, returns 0..1):
```hlsl
const float M[16] = {
   0, 8, 2,10,
  12, 4,14, 6,
   3,11, 1, 9,
  15, 7,13, 5 };
int2 p = int2(pixCoord) & 3;          // mod 4
return (M[p.y*4 + p.x] + 0.5) / 16.0;
```
`Bayer8` is the same idea with a 64-entry matrix and `& 7`.

`PaletteSnap` Custom HLSL (nearest of N const colors — example uses a cold
blue-grey set; swap for the real palette, or replace the const array with a
`Texture2D` palette sample loop):
```hlsl
// in: float3 c ; returns nearest palette color
const float3 PAL[8] = {
  float3(0.04,0.05,0.07), float3(0.10,0.13,0.17),
  float3(0.18,0.24,0.30), float3(0.30,0.38,0.45),
  float3(0.55,0.30,0.12), float3(0.85,0.45,0.15),   // warm emissive accents
  float3(0.20,0.55,0.25), float3(0.75,0.85,0.90) }; // CRT green + cold highlight
float bestD = 1e9; float3 best = c;
[unroll] for (int i=0;i<8;i++){ float3 d=c-PAL[i]; float dd=dot(d,d);
  if (dd<bestD){bestD=dd;best=PAL[i];} }
return best;
```

Notes:
- Quantizing per-channel at `Levels` already yields a fixed RGB-cube palette
  (`Levels^3` colors) without `PaletteSnap`; the palette node is only for a
  *curated* limited palette (e.g. forcing the cold-blue / warm-amber / CRT-green
  triad of the reference images). Keep `PaletteStrength` low for grimy-modern.
- Do the dither at the **internal** resolution grid (`DitherGridRes` = internal
  render res), not the display res, so dither dots are one render-pixel each and
  survive the nearest upscale as chunky pixels.

### 5.3 Tunable knobs for Layer D
`MasterRetro`, `ColorLevels`, `DitherStrength`, `BayerSize` (4 vs 8),
`PaletteStrength`, `DitherGridRes` (vec2), palette colors (vector params or a
palette texture).

---

## 6. Material Parameter Collection — `MPC_Retro`

Confirmed class: `UMaterialParameterCollection`
(`Runtime/Engine/Public/Materials/MaterialParameterCollection.h:78`) with
`ScalarParameters` (line 91) and `VectorParameters` (line 94) arrays. Materials
reference values with the `CollectionParameter` node. A single global collection
asset is read by both the surface WPO material function and the post-process
material, so every dial is centralized and can be set at runtime from Blueprint
(`SetScalarParameterValue` / `SetVectorParameterValue` on the collection
instance). Exact max parameter count is **ASSUMPTION** (engine packs scalars into
float4s; we need ~14, far under any cap).

### Scalar parameters
| Name | Range | Default (full-retro) | Drives |
|---|---|---|---|
| `MasterRetro` | 0..1 | 1.0 | global master blend (A switch logic + B + D) |
| `SnapStrength` | 0..1 | 1.0 | WPO vertex snap amount |
| `SnapMode` | 0/1/2 | 2 | off / world-grid / screen-space snap |
| `SnapGridDensity` | 60..480 | 240 | screen-space snap grid (tie to internal res) |
| `WorldSnapSize` | 0.5..8 cm | 2.0 | world-grid snap cell |
| `ColorLevels` | 2..64 | 5 | posterize levels per channel |
| `DitherStrength` | 0..1 | 1.0 | ordered-dither intensity |
| `BayerSize` | 4/8 | 4 | dither matrix size selector |
| `PaletteStrength` | 0..1 | 0.6 | blend toward curated palette |
| `PaletteSize` | 2..32 | 8 | palette entry count (if texture-driven) |
| `TexCrunch` | 0..1 | 1.0 | point-vs-filtered texture blend (Layer C) |

### Vector parameters
| Name | Meaning |
|---|---|
| `DitherGridRes` | internal render resolution (xy) for dither/snap alignment |
| `FogTint` | cold blue-grey tint pushed into quantization (north-star palette) |
| `EmissiveAccent` | warm amber/red accent kept un-posterized for fog glow |
| `PaletteA..PaletteH` | (optional) explicit palette slots if not using a palette texture |

### Runtime-CVar dials (not in MPC — driven from the same controller BP/console)
`r.ScreenPercentage`, `r.AntiAliasingMethod`, `r.Upscale.Quality`,
`r.Streaming.MipBias`, plus the "flatten" set in §1.4. The controller Blueprint
maps `MasterRetro` → these CVars (e.g. via `Execute Console Command`) and → the
MPC scalars together, so one slider moves the whole look.

---

## 7. Confirmed CVar / API reference (anchors)

| API / CVar | Value(s) | On-disk anchor |
|---|---|---|
| `r.ScreenPercentage` | float, >0..100, >100 SS | `Runtime/Engine/Private/LegacyScreenPercentageDriver.cpp:37-43` |
| `r.Upscale.Quality` | 0=Nearest,1=Bilinear,3=default | `Runtime/Renderer/Private/PostProcess/PostProcessUpscale.cpp:21-31` |
| `EUpscaleMethod::Nearest` | spatial nearest path exists | `.../PostProcessUpscale.cpp:110,162` |
| `r.SecondaryScreenPercentage.GameViewport` | float | `Developer/FunctionalTesting/Private/AutomationBlueprintFunctionLibrary.cpp:250`; `Runtime/ApplicationCore/Private/Android/AndroidWindow.cpp:620` |
| `r.AntiAliasingMethod` | selects EAntiAliasingMethod | `Runtime/Renderer/Private/RendererScene.cpp:890` |
| `EAntiAliasingMethod` | None=0,FXAA=1,TAA=2,MSAA=3,TSR=4,SMAA=5 | `Runtime/Engine/Public/SceneUtils.h:44-54` |
| `IsTemporalAccumulationBasedMethod` | TAA & TSR are temporal | `Runtime/Engine/Public/SceneUtils.h:56-60` |
| `MD_PostProcess` | Material Domain = Post Process | `Runtime/Engine/Public/MaterialDomain.h:23` |
| `MD_Surface` | Material Domain = Surface | `Runtime/Engine/Public/MaterialDomain.h:15` |
| `BL_SceneColorAfterTonemapping` | =0 (renamed from BL_AfterTonemapping in 5.4) | `Runtime/Engine/Classes/Engine/BlendableInterface.h:75,82` |
| `EBlendableLocation` (full enum) | DOF/Bloom/SSR/Tonemapper locations | `.../BlendableInterface.h:14-82` |
| `PPI_PostProcessInput0` / `PPI_SceneColor` | SceneTexture inputs | `Runtime/Engine/Public/MaterialSceneTextureId.h:43,15` |
| `MP_WorldPositionOffset` | WPO material output | `Runtime/Engine/Public/SceneTypes.h:161` |
| `UMaterialParameterCollection` | Scalar/Vector params | `Runtime/Engine/Public/Materials/MaterialParameterCollection.h:78,91,94` |

**ASSUMPTION** (not confirmed on disk, verify when VS is installed):
`r.TemporalAA.Upsampling`, `r.Tonemapper.Quality` exact names; absence of a
global force-point-sampling CVar; exact MPC parameter cap; whether the secondary
upscale stage honors `r.Upscale.Quality`.

---

## 8. Build order for M1

1. Create `MPC_Retro` with the params in §6.
2. Build `MF_VertexSnap` (§2.1) and add to every world material's WPO.
3. Build `PP_Retro` post-process material (§5) + Bayer/Palette Custom nodes;
   add to an Unbound Post Process Volume.
4. Build `BP_RetroController` that owns `MasterRetro` and writes both the MPC
   scalars and the §1/§1.4 CVars from one slider (full-retro ↔ modern-grimy).
5. Per-asset: set hero textures to `Filter = Nearest`.
6. Validate the integer-divisor resolution table (§1.3) on the 1920×1080 and
   the user's native res; lock to the nearest integer-multiple screen percentage.

Affine texture warp (§4) is explicitly **out of scope** for M1 (needs a custom
vertex factory).
