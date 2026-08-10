# Kessel — ART BIBLE (Milestone 1)

**Project:** Kessel — first-person, single-crew (crew of 1) atmospheric tank-escape sim.
A lone Tiger operator breaking out of a winter encirclement ("the Kessel" / General Winter).
**This doc locks the visual direction for M1** (drivable tank + combat + volumetric fog +
emissive scatter + multi-sight first-person view).

**North star = the FEELING of the reference images, not a fixed pixel spec.** The exact
graphics style is deliberately FLEXIBLE and TUNABLE: one master dial (`MasterRetro`, see
`docs/research/05-ps1-aesthetic.md`) slides the whole image from *whiteout PS1 crunch* to
*modern-but-grimy*. Everything below names a default at the retro end and a modern end so the
look can move without re-authoring assets.

Reference images live in `C:/shop/tank_simulator/references/`. Every direction below cites the
exact filename(s) that anchor it.

---

## 0. One-paragraph vision

You are sealed inside cold steel. Through scratched periscope glass and a green optical gunsight,
a blue-grey whiteout swallows the world 40–80 m out. Enemy tanks loom out of nothing as black
silhouettes. The only warmth in the frame is emissive and isolated — the amber pulse of your own
muzzle, a burning wreck bleeding red-orange into the snow-fog, a sodium streetlamp cone, the
CRT-green wash of your own instruments on the inside of the glass. Cold base, punctured by warm
light that *scatters* into the volume. Claustrophobic, lonely, quietly terrifying.

Anchor stills for the whole vision: `8d26dd56017b2423418422927d65583f.jpg` (pure fog-silhouette
whiteout), `1afb2910b33e0c55d9745d7edb769728.jpg` (warm red light scattering into cold snow night),
`good_cockpit_refernce.jpg` (CRT-green multi-sight cockpit framing), `2f69ff9d4fefc426059fbb885f39ba8a.jpg`
(blizzard whiteout with a single warm accent).

---

## 1. Framing — ALWAYS inside the steel

**Hard rule: first-person ONLY, always rendered from inside the tank interior. Never a
third-person view, never an exterior chase cam, ever.** The steel interior is a permanent
foreground frame around every shot. The player never sees allies; the only friendly thing in
the world is the cabin around them.

- The interior is the **constant hero asset** — it is on screen 100% of the time, so it carries
  the fidelity budget (§6). Looming machined steel, rivets, weld seams, cabling, painted armor,
  optics housings. Detail and physical believability sell the claustrophobia.
  Anchors: `241d64fcd88d44a51af7f07e616006ee.jpg`, `d467af81c1b44291d9601ef03e7ef2ed.jpg`
  (white-painted AFV interior: commander optic eyepiece, blue-bezel MFDs, indicator LEDs,
  control grips, periscope vision-block ring across the top).
- **Negative space is steel.** Frame the exterior view through hard geometric apertures —
  periscope slits, the gunsight tube, the open hatch ring. The eye reads "I am looking *out of*
  something," like the strut-framed canopy in `good_cockpit_refernce.jpg` and the truss-framed
  corridor of `be6c458a0a7e16c36093d91f869859a7.jpg`.
- **First-person body presence** where it helps immersion: gloved hands on controls / the gun,
  as in `68427a0dfd0eac6cec95a0554adf2f29.jpg` (gloved hand holding a device). Hands appear only
  during interactions (gun, hatch, hand-crank); no full-body avatar.
- The interior never fully lights up. It is **dark steel keyed by isolated emissive sources** —
  one CRT, one indicator, one shaft of exterior light — exactly like the single-monitor-lit room
  in `174e5c0f2a6b622974c8077813b99d7a.jpg`. Ambient is low; the warm/green pools do the work.

---

## 2. Render target — PS1-but-TUNABLE

The graphics style is a **composite of dial-able layers**, all gated by `MasterRetro` (0..1) in
`MPC_Retro`. See `docs/research/05-ps1-aesthetic.md` for the verified engine mechanism; this
section is the *art intent* for where to set the dials.

| Dial end | Feeling | Anchor still |
|---|---|---|
| **Full-retro** (`MasterRetro≈1`) | PS1 survival-horror: low internal res, nearest upscale, vertex swim, posterized + Bayer-dithered, limited palette, flat fog | `51bbce01...jpg`, `8d26dd56...jpg`, `2f69ff9d...jpg`, `eed370b9...jpg` |
| **Mid / PS2-grimy** (`MasterRetro≈0.4`) | blocky foliage, wet desaturated surfaces, light quantization | `6c4358845...jpg`, `2ff8d92e...jpg` |
| **Modern-grimy** (`MasterRetro≈0.1`) | clean-ish modern render with only a hint of crunch; full interior fidelity reads | `241d64fc...jpg`, `d467af81...jpg` |

**Default shipping look for M1 = ~0.7** (strongly retro but the hero interior stays readable).
The interior is authored at modern fidelity and *quantized down* by post — so turning the dial
toward modern reveals more detail rather than requiring new art.

### What stays ON at every dial position (the non-negotiable north star)
- **Volumetric fog + emissive scatter** (§4). The fog glow is the soul of the game and is
  orthogonal to pixel crunch — never disable it for "retro."
- **CRT-green instrument glow** (§5).
- **Cold-base / warm-emissive color philosophy** (§3).

### What the dial moves (per `docs/research/05`, §6 MPC table)
- Internal resolution + nearest upscale: `r.ScreenPercentage` 16.667→100, `r.Upscale.Quality`
  0(nearest)→3, `r.AntiAliasingMethod` 0(None)→TSR. Full-retro internal res target **320×180 or
  480×270**, integer-multiple upscaled so pixels stay square (no shimmer).
- Vertex snap (PS1 "swim"): `SnapStrength`/`SnapGridDensity` — geometry visibly snaps to the
  pixel grid as the turret/vehicle moves. The swimming low-poly look of `51bbce01...jpg`.
- Posterize + ordered dither + limited palette: `ColorLevels` 5→32, `DitherStrength` 1→0.1,
  Bayer 4×4, `PaletteStrength` 0.6→0. Dither at the **internal** res so dots are one render-pixel
  (chunky), like the CRT dither of `d62c2bb057...jpg`.
- Texture crunch: point sampling + tiny textures at retro; filtered at modern.

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

### 3c. CRT instrument greens + cyans (UI only — see §5)
Anchors: `good_cockpit_refernce.jpg`, `174e5c0f...jpg`, `68427a0d...jpg`, `d62c2bb0...jpg`,
`fc7b5c99...jpg`.

| Swatch | Hex | Use |
|---|---|---|
| CRT phosphor green | `#33FF66` | gunsight reticle, primary instrument readouts |
| CRT mid green | `#2BC04A` | gauge bars (WEAP/SHIELD/DRIVE-style), thermal sight wash |
| CRT dim green | `#1F7A33` | inactive UI, grid lines, scanline floor |
| Readout amber | `#FFB020` | secondary numeric readouts (ammo count style, `good_cockpit_refernce.jpg`) |
| CRT cyan | `#7FD8FF` | terminal/hex text, map screen (`d62c2bb0...`, `fc7b5c99...`) |
| Indicator red LED | `#FF2A2A` | warning LEDs / status (`241d64fc...`, `d467af81...`) |

**Palette discipline:** at full-retro the post-process `PaletteSnap` (doc 05 §5.2) forces colors
toward a curated set built from 3a + 3b + the CRT greens. Keep warm emissive un-posterized
(`EmissiveAccent` vector in `MPC_Retro`) so glows stay smooth even when the world is crushed to
5 levels/channel.

---

## 4. Lighting philosophy — emissive scatter into the fog volume

This is the single most important visual system. Implementation is verified in
`docs/research/06-volumetric-fog-scatter.md`; this section is the *art intent*.

### 4a. The General Winter fog (the medium)
A cold blue-grey **whiteout** with short view distance — enemies loom out of nothing. The fog is
not a grey filter; it is a lit volume.
- Color: cool blue-grey albedo (`VolumetricFogAlbedo` ~ (150,170,200)), very dim cold-blue
  emissive floor so deep fog reads **luminous milky white, not black** — the flat whiteout of
  `8d26dd56...jpg` and `51bbce01...jpg`.
- Depth: visibility crushed to ~40–80 m. Silhouettes fade to fog value with distance; far objects
  are pure value-on-fog with no detail (`8d26dd56...`, `eed370b9...`).
- Phase / directionality: slight forward scatter (G≈0.3) so lights wear a *shaped* halo and
  spot/point sources throw visible cones/shafts you can see from the side.

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
| Interior key (tungsten/CRT) | `#FF9B3D` / `#33FF66` | single warm/green pool lighting dark steel (`174e5c0f...`) |

### 4c. Light-shafts & blizzard
- True in-fog cones from spot/point lights (low phase G + high per-light scatter) — primary,
  physically-consistent shafts. Subtle overcast directional light-shaft bloom around wreck
  silhouettes.
- Driving blizzard snow (camera-anchored Niagara), streaking foreground flakes responding to
  vehicle speed, ground spindrift. The snowfall of `2f69ff9d...jpg`. Optional frozen-lens
  accumulation on periscope glass.

### 4d. Interior lighting
Dark steel, low ambient, keyed by isolated emissive: instrument glow, indicator LEDs, and
exterior light spilling through the apertures. The cabin should feel like
`174e5c0f...jpg` — one glowing source carving the geometry out of near-black. Exterior emissive
events (muzzle, nearby wreck, flare) must **spill onto the interior** through the glass/slits so
inside and outside read as one lit space.

---

## 5. Instrument UI — CRT-green, diegetic

All M1 instrumentation is **CRT-green/amber, rendered as if on physical glass inside the cabin**
(diegetic), not a flat modern HUD. Anchor: `good_cockpit_refernce.jpg` — green gunsight reticle
floating in the aperture, green MFD gauge bars (WEAP/SHIELD/DRIVE), amber numeric readouts, a
green radar scope; plus `d467af81...`/`241d64fc...` (green thermal sight screen, blue-bezel MFDs,
red/green LEDs).

- **Typography:** monospace, phosphor-green or cyan, with scanline + slight bloom and optional
  barrel curvature. Anchors: `d62c2bb0...jpg` (hex-dump CRT, cyan-on-near-black, scanlines),
  `fc7b5c99...jpg` (terminal menu, cyan monospace, CRT curve + vignette).
- **Gunsight reticle:** CRT green `#33FF66`, thin, with range ticks — the floating reticle of
  `good_cockpit_refernce.jpg`. Lives in the sight view, not a screen overlay.
- **Gauges / readouts:** green bar gauges + amber `#FFB020` numerics (the "LtLaser 50 / IBPack 17"
  amber count style). Thermal/optic screen content is green-on-black (`d467af81...`).
- **Status LEDs:** small hard red `#FF2A2A` / green `#33FF66` indicator dots on panels
  (`241d64fc...`, `d467af81...`).
- **Corner micro-HUD** (compass/heading) acceptable if it reads as an instrument, like the
  bottom-right compass in `6c4358845...jpg`. Keep minimal — the game is diegetic-first.
- CRT glow is a **warm/green emissive source** in the dark cabin (§4d): it lightly washes nearby
  steel and the inside of the glass.

---

## 6. Concrete art targets (tunable budgets)

RTX 4090 / 128 GB means we are **not perf-constrained**; low-poly is a *stylistic* choice, not a
budget cap. Author at modern fidelity; the post-process crushes it for the retro dial. Two
columns = the same asset read at each dial end.

### 6a. Poly budgets (triangles, LOD0)
| Asset class | Retro intent | Modern ceiling | Notes |
|---|---|---|---|
| **Player interior set** (constant hero) | 80k–150k total | up to ~400k | carries all fidelity; on screen 100% (`241d64fc...`, `d467af81...`) |
| Hero interior prop (optic, MFD, grip) | 1k–8k each | 20k | crisp readable controls |
| **Enemy tank** (LOD0, seen through fog) | 6k–12k | 30k | usually fog-obscured → aggressive LOD1/2 at 2k/500 |
| Environment hero (cabin, wreck, tower) | 300–2.5k | 15k | low-poly silhouette reads in fog (`51bbce01...`, `8d26dd56...`) |
| Scatter prop (tree, pole, fence) | 60–600 | 3k | blocky foliage is on-style (`2ff8d92e...`, `2f69ff9d...`) |
| Terrain | heightfield, ~1–2 cm collision | — | snow; most detail hidden by fog |

### 6b. Texture resolution & filtering
| | Retro end | Modern end |
|---|---|---|
| Hero interior maps | 256–512, **point** filter | 2k, trilinear |
| Enemy tank | 128–256, point | 1k–2k |
| Environment / props | 64–256, point | 512–1k |
| Decals / grime | 256 | 1k |
| Authoring | **always author high**; mip-bias/point-filter to crunch | — |

PBR is fine to author; the posterize/palette pass flattens it for retro. Snow/steel benefit from
subtle roughness variation even when crushed.

### 6c. Color depth, dither, palette (drives `MPC_Retro`, doc 05 §6)
| Param | Retro default | Modern | Meaning |
|---|---|---|---|
| `ColorLevels` (per channel) | **5** | 32 | posterize steps → cube palette |
| `DitherStrength` | **1.0** | 0.1 | Bayer ordered-dither intensity |
| `BayerSize` | 4 (4×4) | 8 | dither matrix |
| `PaletteStrength` | **0.6** | 0.0 | blend toward curated palette (§3) |
| `DitherGridRes` | internal res (e.g. 480×270) | display res | one dither dot = one render pixel |
| `SnapStrength` | **1.0** | 0.0 | vertex swim |
| Internal render res | **320×180 / 480×270** | native | nearest upscale, integer multiple |

Effective output bit-depth at retro ≈ 5 levels/ch ≈ **~7-bit color** dithered up — the chunky
banded look of `d0f5855999...jpg` (pixel-art limited palette) and the CRT dither of `d62c2bb0...`.

### 6d. Snow & motion
- Persistent driving blizzard particles (camera-anchored), streaking foreground flakes scaled by
  vehicle speed, ground spindrift. Mood: `2f69ff9d...jpg`.
- Optional periscope-glass frost/condensation accumulation driven by the same wind parameter.

---

## 7. Per-view-mode look notes

Multi-sight first-person system (see `docs/research/07-view-modes-controls.md`). Each mode is a
different aperture in the same steel cabin — the framing rule (§1) never breaks.

### 7a. Driver / hull periscope view
- Narrow horizontal slit aperture; heavy steel frame top & bottom, scratched/iced glass overlay.
- Wide-ish FOV but vertically letterboxed by the slit. Snow streaks across the glass.
- Minimal UI: small green heading/speed readout in corner (`6c4358845...` compass style).
- Anchor: vision-block ring of `d467af81...`; flat foggy road ahead like `eed370b9...`,
  `be6c458a...`.

### 7b. Gunner optical sight (primary combat view)
- Zoomed circular/rectangular optic with **CRT-green reticle + range ticks** floating in a black
  vignette tube; etched stadia lines. Magnification readout ("3X") corner.
- This is where muzzle flash blooms hugest into the fog (§4b). Recoil shakes the reticle.
- Green thermal/optic wash acceptable as an alternate sight channel (`d467af81...`,
  `241d64fc...` green sight screen). Reticle color `#33FF66`, amber range numerics `#FFB020`.
- Anchor: floating green reticle of `good_cockpit_refernce.jpg`.

### 7c. Commander / open-hatch view (situational)
- Wider FOV, head above the ring; full blizzard exposure, periscope-ring steel framing the
  bottom edge (`241d64fc...`, `d467af81...` vision-block ring).
- Most exposed/vulnerable; the whiteout is most total here (`8d26dd56...`, `2f69ff9d...`).
- Light spill from exterior emissive events strongest in this mode.

### 7d. Interior / instrument focus (MFD, controls)
- Camera settles on the panel: blue-bezel MFD, green map/thermal screen, gauge bars, LEDs, grips.
- Darkest mode — keyed almost entirely by the screens' own emissive (§4d, `174e5c0f...`).
- Strongest CRT typography read (`d62c2bb0...`, `fc7b5c99...`).
- Anchors: `241d64fc...`, `d467af81...`.

**Transitions** between modes are diegetic (lean to the optic, pop the hatch) — no abrupt cuts
that would break the "inside the steel" continuity.

---

## 8. Do / Don't (quick reference)

**DO**
- Keep the steel interior framing every shot; first-person only.
- Keep the world cold and desaturated; let only emissive sources carry warm/green saturation.
- Make every warm light *scatter into the fog* (core→cool-halo).
- Author high-fidelity, crunch via post so the dial is honest.
- Keep instruments diegetic CRT-green/amber on glass.

**DON'T**
- No third-person / exterior chase cam, ever.
- No allies on screen.
- Don't disable volumetric fog or emissive scatter for "retro" — crunch is orthogonal to fog.
- Don't let warm accents proliferate (kills the punctured-darkness contrast).
- Don't over-blue the fog albedo (warm lights would desaturate — doc 06 §3 caveat).

---

## 9. Reference image index (filename → what it anchors)

| File | Anchors |
|---|---|
| `good_cockpit_refernce.jpg` | CRT-green multi-sight cockpit framing; floating reticle; green MFD bars; amber readouts; steel canopy struts |
| `241d64fcd88d44a51af7f07e616006ee.jpg` | Hero AFV interior fidelity; optic eyepiece; blue-bezel MFD + green thermal screen; LEDs; periscope ring |
| `d467af81c1b44291d9601ef03e7ef2ed.jpg` | Wider AFV interior; vision-block ring; green "3X" sight screen; white painted armor |
| `51bbce01d236bfd05e01de5cead2567c.jpg` | General Winter whiteout exterior; low-poly bare trees + cabin; flat fog sky |
| `8d26dd56017b2423418422927d65583f.jpg` | Pure fog-occlusion silhouette whiteout (Silent Hill cast); blue-grey monochrome |
| `2f69ff9d4fefc426059fbb885f39ba8a.jpg` | Blizzard snowfall + whiteout + single warm(red) accent; low-poly |
| `1afb2910b33e0c55d9745d7edb769728.jpg` | HERO: warm red lamp scattering into cold snow night; warm-core/cool-edge |
| `68427a0dfd0eac6cec95a0554adf2f29.jpg` | Amber lamp scatter into blue fog; green CRT monitors; gloved-hand FPS |
| `be6c458a0a7e16c36093d91f869859a7.jpg` | Steel truss looming in fog; rust-red girder warmth; lamps fading to grey |
| `174e5c0f2a6b622974c8077813b99d7a.jpg` | Single emissive (green CRT) keying a dark cluttered interior |
| `57bdc31c4c8370519cd8fda2494ae30b.jpg` | Headlight cone + distant warm point-lights strung through blue haze |
| `b8368a63d52784e6008512024f06feaf.jpg` | Cold purple-blue night; warm window/taillight accents through haze; pixel |
| `d62c2bb057133f2db6d0b8504011b8d6.jpg` | CRT typography: cyan/white monospace hex on near-black; scanlines/dither |
| `fc7b5c99ac505a251af6797a9fa69a0c.jpg` | CRT terminal UI; cyan monospace menu; barrel curvature + vignette |
| `6c4358845a33d48c59dcab46dd7aae5f.jpg` | PS2-grimy wet overcast street; corner compass micro-HUD; mid-dial look |
| `2ff8d92e60a293b7dd7fa71e70b93910.jpg` | Modern-grimy / PS2 end of dial; blocky foliage; daytime |
| `d0f5855999387d133a1f9f1b71137aa6.jpg` | Limited-palette pixel-art dither/banding reference |
| `eed370b9b563204df0db6c594b6de015.jpg` | Low-res compressed grimy fog street; lone amber dot in grey |
| `9c852b17a846cc0f878f815fec293611.gif` | (animated mood board — motion/atmosphere; not stills-analyzed) |
| `f78b3ab7bfb8d37723e5591e30f7ead8.gif` | (animated mood board — motion/atmosphere; not stills-analyzed) |

---

## 10. Cross-references
- PS1-but-tunable render mechanism + `MPC_Retro` dials: `docs/research/05-ps1-aesthetic.md`
- Volumetric fog / emissive scatter / blizzard engine recipe: `docs/research/06-volumetric-fog-scatter.md`
- Multi-sight view & controls: `docs/research/07-view-modes-controls.md`
- Ballistics/armor (combat that drives muzzle-flash & wreck-fire emissive events): `docs/research/04-ballistics-armor.md`
