# Kessel — Art Bible

**Project:** first-person, physically embodied Tiger escape/survival sim in a deliberately
compressed alternate-history campaign.

**North star:** deep simulation seen through an abstract, low-resolution image. The art is not a
fixed imitation of one console and it does not chase photorealism. Internal pixel resolution and
retro-filter strength are independent, bounded controls; world composition, atmosphere, material
readability and physical embodiment must survive every supported combination.

The current reference set is the 16K research board at
`artifacts/visual-moodboard-2026-08-11/output/kessel_visual_direction_moodboard_16k.png`; its
official source pages, hashes and per-frame lessons are recorded in
`artifacts/visual-moodboard-2026-08-11/SOURCE_INDEX.md`. Older filenames below are retained only as
historical composition notes. Modern cockpit/MFD imagery is a mood and framing reference, never a
license to put modern digital hardware inside a Tiger.

---

## 0. One-paragraph vision

You are sealed inside cold steel or exposed beside it. Through scratched periscope glass, an etched
optical sight or a blowing hatch, weather denies information: Act I's blue-grey blizzard gives way
to spring rain, haze, smoke and darkness. Enemy tanks arrive first as sound and silhouette. The only
warmth is isolated—the amber pulse of the 8.8 cm, a wreck bleeding red-orange into haze, a shaded
lamp or a dim physical instrument. Cold base, punctured by warm light that scatters through a real
aperture and atmosphere. Claustrophobic, lonely, quietly terrifying.

Anchor families on the current board: Kessel's own Act I storm and turret interiors; Signalis for
disciplined dark/warm grouping; The Long Dark for readable weather; Crow Country/HROT/Cultic for
stable low-resolution form; and Darkwood/This War of Mine for environmental evidence. Historical
Tiger photographs and manuals—not mood art—decide controls and optics.

---

## 1. Framing — ALWAYS embodied; the steel is home

**Hard rule: first-person ONLY in the shipped game. Never a detached third-person chase camera,
free tactical view, or omniscient spectator.** While mounted, the steel interior is a permanent
foreground frame around every shot. On foot, the camera remains attached to a vulnerable body and
the Tiger becomes the nearby visual anchor, shelter and objective. Allies and co-op crew appear only
as physical bodies in the world—not as free markers or invisible information.

- While mounted, the interior is the **constant hero asset**, so it carries the fidelity budget
  (§6). Looming machined steel, rivets, weld seams, cabling, painted armor and optics housings sell
  the claustrophobia. On foot, that same standard transfers to hands, carried objects, complete rooms
  and the Tiger's exterior rather than becoming a low-detail intermission.
  Older modern-AFV images remain useful only for aperture rhythm, value grouping and optic framing;
  their MFDs, thermal screens and digital indicators are not Tiger hardware references.
- **Negative space is steel.** Frame the exterior view through hard geometric apertures —
  periscope slits, the gunsight tube, the open hatch ring. The eye reads "I am looking *out of*
  something," like the strut-framed canopy in `good_cockpit_refernce.jpg` and the truss-framed
  corridor of `be6c458a0a7e16c36093d91f869859a7.jpg`.
- **First-person body presence** where it helps immersion: gloved hands on controls / the gun,
  as in `68427a0dfd0eac6cec95a0554adf2f29.jpg` (gloved hand holding a device). Hands appear only
  during interactions (gun, hatch, hand-crank). The local camera does not render an obstructive
  full-body avatar; remote/co-op crew require complete physical bodies, silhouettes and collision.
- The interior never fully lights up. It is **dark steel keyed by isolated physical sources**—a
  shaded bulb, gauge illumination, muzzle flash through an aperture or a narrow exterior shaft.
  Ambient is low; small warm/red pools and reflected weather light do the work.

---

## 2. Render target — two independent knobs

The live Rust/wgpu renderer separates **pixel grid** from **retro treatment**.

### Internal resolution

`KESSEL_INTERNAL_RES` accepts exactly 640×360, 800×450, 960×540 or 1280×720. The default remains
640×360. World lighting, weather and post-processing operate at that internal size, then the final
image is enlarged with exact nearest sampling. Vertex snapping/swim is intentionally absent; object
motion and silhouettes stay geometrically stable.

800×450 is the preferred visual candidate after side-by-side review, not yet the shipping default.
A recorded Mac frame-time baseline and optimization gate must precede any default change. Effects
whose authored width is one to three internal pixels—snow, grain and dither—must scale against a
640×360 reference so higher resolution does not silently weaken the storm.

### Retro treatment

`KESSEL_RETRO_PRESET` accepts clean/off, light, standard and strong. These presets control the live
posterization, 4×4 Bayer ordered dither and restrained grain without changing internal resolution.
The current default is standard (the existing 0.7 treatment). They do not change geometry, texture
assets or lighting transport.

Required comparison grid before final art lock:

- 640×360 clean;
- 640×360 standard;
- 800×450 light;
- 800×450 strong.

The non-negotiable north star stays active in every combination: weather and haze, isolated light,
cold/warm discipline, readable historical instruments, physical apertures and exact-nearest pixels.
The project is not using live texel splatting; `src/texelsplat.rs` remains an isolated research
prototype with unresolved world-scale, motion and disocclusion costs.

---

## 3. Palette — cold blue-grey base, punctured by warm emissive

The entire game is a **two-temperature system**: a cold desaturated environment, and a small set
of warm/green *emissive* accents that are the only saturated color in frame. Restraint is the
rule — if everything glows, nothing glows.

### 3a. Cold base (environment, fog, steel) — ~90% of every frame
Anchors: `8d26dd56...jpg`, `51bbce01...jpg`, `2f69ff9d...jpg`, `b8368a63...jpg`, `eed370b9...jpg`.

| Swatch | Hex | Use |
|---|---|---|
| Whiteout / deep fog | `#E6EAEC` | far fog wall, snow-sky, the "nothing" enemies emerge from |
| Fog mid blue-grey | `#AEBCC6` | mid-distance haze, snow in flat light |
| Cold steel light | `#7E8C97` | lit painted armor, periscope frame highlights |
| Cold steel mid | `#54626E` | hull steel, girders (`be6c458a...` rust-girder substrate) |
| Cold shadow | `#2E3A46` | interior shadow, deep fog occlusion |
| Night cold-blue | `#1B2430` | night exterior base (`1afb2910...`, `57bdc31c...`) |
| Cold purple-blue | `#2A2740` | dusk/night sky tint (`b8368a63...`) |
| Near-black blue | `#0A0E14` | interior darks, silhouettes, void |

### 3b. Warm emissive accents — the punctures (fire / lamps / muzzle)
Anchors: `1afb2910...jpg` (red lamp into snow), `68427a0d...jpg` (amber lamp into blue fog),
`be6c458a...jpg` (rust warmth), `57bdc31c...jpg` (distant red taillights in haze).

| Swatch | Hex | Use |
|---|---|---|
| Muzzle / flare hot core | `#FFE8B4` | muzzle flash + magnesium flare core (near-white amber) |
| Sodium amber lamp | `#FF9B3D` | streetlamp/pole light, interior tungsten warm key |
| Burning-wreck orange | `#D9531C` | wreck fire body |
| Deep ember red-orange | `#B5300E` | wreck fire base, low embers |
| Hot signal red | `#E83A3A` | red lamp/flare, taillights (`1afb2910...`) |
| Red scatter halo | `#7A1A22` | the cold-edge falloff of a red light bleeding into fog/snow |
| Rust accent | `#8A4A32` | structural rust on girders/wrecks (`be6c458a...`) |

### 3c. Physical instrument and signal accents

| Swatch | Hex | Use |
|---|---|---|
| Radium/dim dial green | `#86B879` | restrained painted numerals/needles under period illumination |
| Aged ivory | `#D6CEAE` | unlit dial markings, map/paper, sight scales |
| Tungsten amber | `#FFB060` | shaded service lamp and warm gauge spill |
| Signal red | `#D83A32` | warning lens, abandon-tank light/lever emphasis |
| Enigma/message green | `#76B889` | limited lampboard/status use on the physical message station |

**Palette discipline:** the live crush pass posterizes and dithers the lit image; it does not snap
materials toward a curated palette texture. Keep saturated accents rare and spatially justified. A
modern CRT/cyan look is appropriate only for menus, debug tools or an explicitly non-historical
reference—not for the Tiger's native controls.

---

## 4. Lighting philosophy — emissive scatter into the fog volume

This is the single most important visual target. The High renderer now visibility-tests direct sun
and compact emitters against scene triangles while integrating extinction through modeled local
media. Multiple scattering and sky-environment in-scatter remain unfinished; neither is replaced by
an authored glow.

### 4a. Weather is the medium

Act I is a cold blue-grey active blizzard: snow, gusts and spindrift reduce but do not erase usable
visibility. Acts II and III keep the information denial through wet low haze, smoke, ash, overcast
and darkness rather than copying the whiteout. Silhouettes should merge gradually into the local
fog value; the atmosphere must describe distance, not behave like a flat screen overlay.

Shafts are observations of in-scatter along a camera ray, not authored objects. They emerge where a
real lamp, fire, muzzle, or exterior environment path is visible through a modeled medium. An
aperture may admit radiance but is never itself a light source. Do not exaggerate a shaft by adding
energy outside that shared transport.

### 4b. Warm sources puncture it
Because the fog albedo is cool, a **warm light scatters as a warm core melting to a cool halo
edge** — exactly the reference smear. Restraint: few sources, each meaningful.

| Source | Color | Look anchor |
|---|---|---|
| Muzzle flash (pulsed, ~0.06 s) | hot amber-white `#FFE8B4` | huge transient fog bloom on every shot |
| Burning wreck (flicker) | `#D9531C`→`#B5300E` | red-orange bleeding into snow-fog + smoke plume |
| Streetlamp / pole | sodium amber `#FF9B3D` | cone-of-light shaft in fog (`68427a0d...`, `be6c458a...`) |
| Red signal lamp / flare | `#E83A3A`→halo `#7A1A22` | the hero red-into-snow scatter of `1afb2910...` |
| Distant warm points | amber/red | strung lights & taillights receding into haze (`57bdc31c...`, `b8368a63...`) |
| Interior key (shaded lamp/gauge) | `#FF9B3D` / `#86B879` | small justified pools carving dark steel |

### 4c. Light shafts & blizzard
- Hero emitters create low-resolution shadowed volumetric structure through the same medium
  integral as every other source. Analytic cones and glow approximations are forbidden even as a
  fallback; reduce samples or resolution when necessary.
- The live blizzard is a deterministic depth-aware fullscreen weather field with world-reconstructed
  shells and spindrift—not Niagara or CPU particles. Flake silhouettes remain hard-edged and must be
  resolution-normalized. Optional frost/condensation belongs to periscope glass, not the whole screen.

### 4d. Interior lighting
Dark steel, low ambient, keyed by shaded bulbs, subtle gauge light and exterior energy through real
apertures. Exterior events—muzzle, nearby wreck, flare, headlight—must affect the interior only
where geometry permits, so inside and outside read as one space rather than two color grades.

---

## 5. Instruments and optics — physical, period-grounded, diegetic

The Tiger does not receive fictional MFDs, radar bars, a thermal channel or a floating corner HUD.
Information is read from physical controls and historically researched optics:

- **Driver:** mechanical speed, RPM, fuel and gear indication with glass, needles, printed markings
  and restrained period illumination. Values move the actual needle/indicator geometry.
- **Gunner:** production-period TZF reticle and range scales, verified before art lock. Markings live
  in the optical path with edge refraction, dirt and damaged/replacement glass; they are not a generic
  neon-green screen overlay.
- **Commander/loader/radio:** real switches, lamps, labels, intercom/radio hardware and mechanical
  state. Signal colors remain small and purposeful.
- **Map/compass/watch:** physical carried objects. If the player leaves the map or compass behind,
  the information is unavailable.
- **Enigma/message station:** lampboard and typework may carry a limited distinctive green/amber
  language, but the machine is a physical prop and not a tactical radio display.

Menus and developer tools may use terminal typography as an out-of-world presentation layer. The
playfield remains diegetic-first.

---

## 6. Concrete art and performance targets

Apple Silicon macOS is the primary development target. Treat both CPU and GPU budgets as constrained
until a self-describing baseline identifies the limiting passes; no document may assume unlimited
RTX-class compute or memory. Low-poly form is both style and a scaling tool.

### 6a. Asset rules

- The Tiger interior, working controls, held objects and enterable rooms receive the highest local
  fidelity because they are viewed at arm's length.
- Enemy vehicles and world props need explicit near/mid/far meshes or impostors. Fog is not a license
  to submit one expensive LOD and draw it everywhere.
- Reuse one GPU prototype across spatial pages; instance transforms and state are the variable data.
- Author PBR information where it changes the read—paint, steel, brass, glass, snow, soot and wet
  masonry—but keep material count, alpha and texture residency bounded.
- Pixel-art texture treatment comes from intentional source art and pixel-safe mips, not from making
  metre-scale satellite pixels visible at foot height.

### 6b. Terrain and effects

- Terrain combines macro color/control with small tiled albedo/normal/roughness, slope rules and
  authored roads, tracks, craters, mud, snow, ash and rubble.
- Weather, smoke, fire, decals and particles have bounded pools and distance tiers.
- A one- to three-pixel effect is specified relative to the 640×360 reference grid and rescaled for
  higher internal resolutions.
- Exact-nearest screenshots are an acceptance gate; live window presentation must preserve aspect
  and avoid uneven pixel stretching.

### 6c. Required receipts

Every proposed default change records commit, Mac model, build profile, scene, warmup/timed frame
count, average/p50/p99 frame time, internal resolution and effect preset. A folder of PNGs is visual
evidence, not a performance receipt.

---

## 7. Per-view-mode look notes

Multi-sight first-person system (see `docs/research/07-view-modes-controls.md`). Each mode is a
different aperture in the same steel cabin — the framing rule (§1) never breaks.

### 7a. Driver / hull periscope view
- Narrow horizontal slit aperture; heavy steel frame top & bottom, scratched/iced glass overlay.
- Wide-ish FOV but vertically letterboxed by the slit. Snow streaks across the glass.
- No floating speed/heading HUD. The player must look to the physical driver gauges and carried
  compass; an accessibility overlay, if added, is explicitly separate from canonical presentation.
- Anchor: vision-block ring of `d467af81...`; flat foggy road ahead like `eed370b9...`,
  `be6c458a...`.

### 7b. Gunner optical sight (primary combat view)
- Zoomed circular/rectangular optical path with a production-period TZF reticle, etched stadia/range
  markings and restrained glass-edge refraction inside a black tube vignette.
- This is where muzzle flash blooms hugest into the fog (§4b). Recoil shakes the reticle.
- No fictional thermal mode or generic neon reticle. Illumination/color must follow the researched
  sight and time-of-day hardware; damage can crack or destroy the replaceable glass.

### 7c. Commander / open-hatch view (situational)
- Wider FOV, head above the ring; full blizzard exposure, periscope-ring steel framing the
  bottom edge (`241d64fc...`, `d467af81...` vision-block ring).
- Most exposed/vulnerable; Act I's whiteout is most total here, while later acts expose rain, haze,
  smoke and distant silhouettes.
- Light spill from exterior emissive events strongest in this mode.

### 7d. Interior / instrument focus
- Camera settles on a real gauge, switch, sight, radio, map, breech or service control.
- This is the darkest mode, keyed by one shaded lamp, dim gauge illumination or exterior spill.
- Labels, needles, switch positions, moving linkages and sound communicate state; no modern MFD
  substitutes for the mechanism.

**Transitions** between modes are diegetic (lean to the optic, pop the hatch) — no abrupt cuts
that would break the "inside the steel" continuity.

---

## 8. Do / Don't (quick reference)

**DO**
- Keep every view physically embodied; frame mounted views through the steel interior and foot views
  through body/held objects and nearby world geometry.
- Keep the world cold and desaturated; let only emissive sources carry warm/green saturation.
- Make every warm light *scatter into the fog* (core→cool-halo).
- Author high-fidelity, crunch via post so the dial is honest.
- Keep instruments diegetic, physical and period-grounded.

**DON'T**
- No detached third-person / exterior chase cam in the shipped game.
- No omniscient ally markers or information; visible allies must occupy real space.
- Do not weaken atmosphere to create the retro look. True local volumetric extinction/scatter is a
  target; the current haze/glow approximations and pixel crunch remain independent controls.
- Don't let warm accents proliferate (kills the punctured-darkness contrast).
- Don't over-blue the fog albedo (warm lights would desaturate — doc 06 §3 caveat).

---

## 9. Current reference hierarchy

Use references for one property at a time; never import a whole game's look or anachronistic hardware.

1. **Current Kessel screenshots** define the machine, palette and embodiment already achieved.
2. **Official game screenshots on the 16K board** define composition lessons: Signalis for controlled
   value and red accents; The Long Dark for weather readability; Crow Country/HROT/Cultic for stable
   low-resolution 3D; Darkwood/Project Zomboid/This War of Mine for dense environmental evidence;
   Brigador/Foxhole/Teardown for readable destruction and battlefield mass.
3. **Historical Tiger evidence** controls hardware, optics, reticles, labels and mechanisms. Modern
   AFV/cockpit images may influence framing or light grouping only.
4. **The Pizza library** controls the environment's modular visual vocabulary and shipping license
   boundary; it does not override historical Tiger truth.

The full source ledger is
`artifacts/visual-moodboard-2026-08-11/SOURCE_INDEX.md`. Third-party screenshots are research-only
and are not redistribution assets.

## 10. Current implementation references

- `src/renderer.rs` and `shaders/crush.wgsl` — live internal-resolution and retro preset behavior.
- `shaders/snow.wgsl` — deterministic depth-aware blizzard/spindrift implementation.
- `docs/DAY0_TO_NOW_AUDIT.md` — current visual/gameplay gaps and production order.
- `docs/AUDIO_ARCHITECTURE.md` and `docs/research/audio/` — current acoustics and source research.
- `docs/research/05-ps1-aesthetic.md`, `06-volumetric-fog-scatter.md` and
  `07-view-modes-controls.md` are early Unreal-era research, not current engine recipes.
