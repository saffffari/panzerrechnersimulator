# Kessel — Rendering Vision: "Ray-Traced PS1"

> ⚠️ **Engine superseded — see `ENGINE.md`.** The *vision* in this doc (ray-traced PS1: render
> expensive in HDR at low internal res, crush last) is canonical and fully alive. The *mechanism*
> is not: there is no Unreal, no Lumen/Niagara/MegaLights/Substrate, no `r.*` CVars. The look is
> now produced by our own Rust + wgpu renderer (raster G-buffer → compute path tracer → WGSL crush).
> Read every "UE feature" below as the *effect* we reproduce ourselves; `ENGINE.md` §8 maps each one.

The pitch in one line: **PS1 is the art-direction floor, UE5.8's entire modern rendering
stack is the ceiling, and the low internal resolution is what lets us afford the ceiling.**
We are not simulating a PlayStation. We are building a 1999 silhouette lit by 2026 light
transport. This doc is the idea-space to explore; pair it with `tank-sim-raytraced-ps1`
(memory) and tasks #6 (PS1 post) + #18 (RT pipeline).

## The core principle (why this isn't a contradiction)

Render the 3D scene **internally at ~320×240–480×270**, in **full HDR**, with everything
expensive turned on. Then, as the *final* post step, crush that HDR frame to a PS1 frame:
quantize color, dither, nearest-neighbor upscale to the monitor. Two consequences:

1. **The expensive stuff is cheap.** Path/ray-tracing cost scales with pixel count. At
   ~115k internal pixels (vs 8.3M at 4K), a 4090 has ~70× headroom. We spend it all on light.
2. **Do the fancy lighting in HDR, crush LAST.** A muzzle flash blooms in HDR, *then* gets
   quantized — so the bloom survives into the chunky frame as blown-out clusters of lit
   pixels. GI, volumetrics, and reflections all read THROUGH the PS1 filter instead of being
   replaced by it. The order is the whole trick.

---

## 1. Light transport & GI — the headroom dump

- **Lumen (Hardware Ray Tracing) GI + Reflections.** The flagship. In a near-black world,
  *every* emissive source — muzzle flash, burning wreck, tracer, flare, instrument dial —
  throws real bounced light onto snow and steel. The cold dark isn't lit by a global ambient;
  it's lit by isolated fires that actually illuminate their surroundings. This is already
  half-built: the tracer lights, wreck-glow, and impact flashes I wrote become GI emitters.
- **MegaLights (UE5.5+).** *This is the answer to "tons of light sources."* MegaLights makes
  hundreds–thousands of **shadow-casting** dynamic lights affordable via stochastic sampling.
  So every ember, every tracer, every spark can be its own shadow-casting point light, and the
  blizzard can be lit by dozens of moving sources at once. Without MegaLights you'd budget ~8
  dynamic shadowed lights; with it, hundreds. Turn this on early — it's transformative here.
- **Ray-traced shadows** — sharp, contact-accurate shadows from the weak sun and every fire;
  a tracer streaking past casts a moving shadow across a wall.
- **Ray-traced reflections** — wet metal, ice sheets, puddles, the tank's glacis, and the
  **gunner's periscope glass**. A burning sister-Tiger mirrored in river ice.
- **Path Tracer** — too slow for runtime, but perfect for a **Photo Mode** and marketing
  shots: ground-truth GI on the PS1 silhouette is a striking, ownable image.

## 2. Volumetrics — the fog IS the world

The whiteout is load-bearing (40–80 m visibility, `world-map-blockout`). Make it volumetric:

- **Volumetric fog + light shafts.** Every light scatters god-rays through the blizzard. A
  tracer leaves a glowing *tunnel* of lit fog behind it. A muzzle flash momentarily lights the
  whole 80 m bubble like sheet lightning. The wreck-glow becomes a soft dome in the murk.
- **Heterogeneous Volumes / Sparse Volume Textures (UE5.4+)** — thick, self-shadowing smoke
  columns boiling off burning tanks; a shell's cook-off smoke lit from within by its own fire.
- **Local Fog Volumes (UE5.5+)** — denser pockets in craters, behind wrecks, drifting banks
  across the steppe; the balka ravines (R5) full of pooled mist.
- **Volumetric Clouds** — a low, oppressive overcast ceiling; the grey lid on the whole map.

## 3. Particles (Niagara) — "tons of particles"

- **GPU blizzard** — millions of advected snow particles, wind-driven, **lit by the volumetric
  system** so snow glints when it crosses a fire's light.
- **Embers & sparks as MegaLights** — impact sparks and wreck embers that are each a tiny
  moving light (affordable only because of §1). Drifting orange motes in a black-blue void.
- **Tracers** — Niagara ribbon + point light + volumetric interaction (the fog tunnel).
- **Snow interaction & accumulation** — tracks carve trenches (Runtime Virtual Texture
  displacement), spray kicks up, snow settles on the hull and gun; your trail is visible behind
  you (and to enemies). Ties into the "retrace your own ruins" spine.
- **Niagara Fluids** — simulated fire/smoke on catastrophic kills; the turret-pop with a real
  fireball.

## 4. Materials & surface — modern PBR under the crush

- **Substrate (UE5.x layered materials)** — paint over primer over rusted steel; frost
  *accreting* over the paint as the run goes on. Rich material response, then posterized — the
  tension that makes the look.
- **Snow shader** — subsurface scattering (snow faintly glows), sparkle (RT or screen-space),
  RVT-driven accumulation in crevices and against walls.
- **Wet / icy metal & ice** — clearcoat + RT reflections; the frozen river as a literal mirror.
- **Emissive everything** — cooling shell-holes that glow orange→black, glowing reticles,
  cabin dials, distant horizon fires. In a dark game, emissives are the whole palette.

## 5. The PS1 crush pipeline (tasks #6/#18)

Applied at low internal res, before nearest-neighbor upscale, so it's pixel-locked:
- **Pixelation** (fixed low ScreenPercentage / render-target size → nearest upscale).
- **Color quantization** to ~15-bit (5-5-5), the PS1 framebuffer.
- **Ordered (Bayer 4×4) dithering** — authentic + temporally stable (no shimmer on chunky
  pixels); blue-noise variant for the smooth RT/volumetric gradients. (Never white-noise/
  error-diffusion — they crawl.)
- **Vertex snapping / jitter** + **affine (non-perspective-correct) texture warp** — the
  classic PS1 wobble, as optional toggles.
- **Dithered transparency** for fog edges and glass.

## 6. Camera, optics & immersion

- **Periscope / gunsight as rendered optics** — vignette, reticle, chromatic fringe, RT
  reflection on the glass; the commander's vision blocks as letterboxed slits.
- **Claustrophobic buttoned interior** — a dark cabin lit only by a couple of warm dials and
  the sight's glow, GI bouncing off the interior walls; the contrast when you unbutton into the
  white void is the emotional beat.
- **Breath / temperature fog** on the optic glass when buttoned; frost creeping the edges.

## 7. Set-pieces these unlock (the "wow")

- A muzzle flash lighting the entire foggy field for one frame via GI, then black again.
- Driving toward the factory chimney (R3) as a black silhouette in white fog, its base
  glowing from a fire you can't see yet.
- A penetrating hit: HDR spark shower → bloom → quantized into a burst of blown pixels, with
  god-rays stabbing out of the new hole in the armor.
- The perimeter flare (R7) — the only *welcoming* light in the game — as a volumetric beacon
  pulling you home through the murk.

## 8. Performance & "turn on first"

The low internal res is the budget. On a 4090, lead with the highest-impact toggles:
1. **MegaLights** (unlocks the many-lights fantasy).
2. **Lumen Hardware RT GI + Reflections** (high quality — we can afford it).
3. **Volumetric fog** at high quality + per-light volumetric scattering.
4. **The PS1 post material** (so all of the above reads correctly crushed).
5. Then layer Niagara blizzard/embers, Substrate frost, RVT snow tracks.

Tune interactively on the 4090 (the editor capture can't show it — it's a press-Play /
playtest judgment). Config levers: `r.Lumen.*`, `r.RayTracing.*`, `r.MegaLights.*`,
`r.VolumetricFog.*`, `ScreenPercentage`, plus the PS1 post-process material params.
