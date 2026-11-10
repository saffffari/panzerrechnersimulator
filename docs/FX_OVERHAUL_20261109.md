# Fire, smoke, embers and explosions rebuilt (9 November 2026)

Branch `claude/fx-overhaul-20261109`. Code: `src/fx_plumes.rs`, `shaders/rm_plumes.wgsl`, the
`fx` branches in `src/game.rs` (`drive_fire_presentation`, the blast light), `src/renderer_raymarch.rs`
(`collect_plumes`, the `rm-plumes` pass, the ember budget), `src/weapon_vfx.rs`. Tests:
`cargo run --release -- fx-lab PREFIX SCENE` (`src/fx_lab.rs`), `tools/fx_lab_compare.py`.
`KESSEL_FX_LEGACY=1` draws the old effects for comparison; nothing else changes with it.

## What was wrong

Measured on the Canyon kill bench (below) and read from the code:

1. **The far fights showed no fire at all.** The raymarch backend draws flame and soot grains,
   smoke supports and explosion volumes only inside its froxel grid, which ends at 320 m
   (`VOL_FAR_M`), and embers only within 90 m. A T-34 killed 950 m away burned on the CPU every
   update (its surface patches, 768 flame grains, three smoke supports) and drew nothing; in fog
   what little was within reach was a few grey froxels, "flattened by the fog".
2. **The cost came from the near fire, mostly the player's own.** In the audit's bench the
   infantry swarm's petrol bombs set the Tiger's engine bay alight; its flame grains, the cabin
   fire's grains (up to 49,152 flame and 24,576 soot) and the renderer's 20 embers per flame grain
   filled the 262,144-ember ceiling and gave the 72,000 merged grains. With the swarm off and the
   turrets at rest, three far wrecks cost 1.6 ms of simulation and nothing on the GPU.
3. **Every burning thing was generated per grain, every update, at full detail at any distance,**
   and each wreck kept its own patches, cells, supports and light: the cost grew with kills.
4. **A cook-off's blast light** (about 50,000 renderer units for the 29 m fireball) was held at
   18-100 % for 0.85 s: the fog glowed white for most of a second and the eye's adaptation then
   left the scene near black for two to three seconds.

## What replaced it

- **One volume per burning thing.** A burning wreck, hulk, dead ally, the player's burning
  Tiger and its engine-deck and petrol-pool fires are each one `Plume` descriptor: up to three
  flame roots on the wreck's strongest burning surfaces (re-read once a second), a buoyant smoke
  column 35-120 m tall leaning downwind, its soot diluting as it widens. A detonation is a
  fireball volume (a cook-off's with its stem), an HE shell on the ground a burst volume (the
  flash, a fountain of thrown earth and snow over a base surge). The descriptors come from the
  game's own fire state and the renderer's own detonation events; nothing is stored per grain.
- **Ray-marched per native pixel** (`rm-plumes`, after the reflections and glass, before the
  embers and sparks), inside each volume's bounding box, 8-22 steps by projected size, stopped by
  the scene depth, composited with the surfaces' own `fog_to` (the froxel medium, the far banks,
  the weather layer and the clear air), so a fire 450 m or 1,000 m off is drawn and fades with the
  same air as the ground under it.
- **Physically shaped light.** The flame is soot-laden gas: absorption about 1.4 /m with Planck
  emission at a noisy temperature field (about 1,000 K at the ragged edges, 1,450-1,570 K in the
  body), its colour and brightness from the Planck spectrum's photopic weight, 1,500 K at
  7e4 cd/m^2. The smoke absorbs and scatters (albedo 0.12 fresh, 0.32 settled, the burst's
  earth 0.07 at the core to the snow's 0.65 at the edges), lit by the key beam through its own
  optical depth and the cloud shadow, by the sky above and the snow below, and by the flame.
- **Fixed budgets, nearest first.** At most 8 world fires (as before), 48 volumes, 4,096 fire
  embers and 12,288 grain embers in all; a far fire throws no embers. The cost cannot grow with
  kills, only with what the nearest volumes cover on the screen.
- **Long fires settle.** Over the first 90-420 s of a burn the flames die back to 35 % and the
  smoke thins and greys: an old wreck is a smouldering grey column, cheaper than a fresh one.
- **Embers** are burning flakes lofted by the plume, cooling from 1,300 to 800 K in 2-5 s,
  swirling and carried downwind: up to 360 per fire at the fire, none beyond 140 m. The cabin
  fire keeps its grains, six times fewer and each sqrt(6) wider (same covered area, same light),
  and two embers per grain instead of twenty.
- **The blast light** of a kill or cook-off now falls as f^5 over its life (half of it in the
  first eighth) instead of holding.

## Numbers, before and after

All on this machine's RTX 4090, 1024x576, the launcher's locked look, Canyon v2 manifest at 07:00,
fixed 1/60 s steps. Another session was benching the same GPU at the time, so absolute times
drift by about 1 ms between runs; every pair below ran back to back with the same binary
(`KESSEL_FX_LEGACY=1` for the old effects).

### The kill bench (`gfxbench high 2400`, `KESSEL_BENCH_LIVE_FRAME=1`)

Kills at timed frames 60, 120 and 180 (`KESSEL_BENCH_KILL=60,120,180`, the nearest T-34 each
time, 934-956 m from the Tiger), measured from frame 1200 to 2400 (`KESSEL_BENCH_SETTLE=1200`),
when the thrown turrets are at rest. Mean per frame:

| Run | GPU frame | Simulation | Effects phases (`update_fx`) | Embers | Merged grains |
|---|---|---|---|---|---|
| Infantry swarm off, no kill | 15.32 ms | 11.18 ms | 0.24 ms | 0 | 0 |
| swarm off, 1 kill, new | 15.55 ms | 11.08 ms | | 0 | 0 |
| swarm off, 3 kills, old | 15.21 ms | 13.09 ms | 1.90 ms | 0 | 0 |
| swarm off, 3 kills, new | 15.18 ms | 11.67 ms | 0.41 ms | 0 | 0 |
| swarm on, 3 kills, the Tiger's engine bay alight (petrol bombs), old | 31.81 ms | 37.08 ms | 9.29 ms | 262,140 | 69,769 |
| same, new | 24.97 ms | 30.77 ms | 3.09 ms | 12,288 | 11,632 |

With three burning wrecks (swarm off) the new effects add **-0.1 ms GPU and +0.5 ms simulation
over the empty scene** (old: -0.1 ms GPU, because nothing past 320 m was drawn, and +1.9 ms
simulation). The old effects' far wrecks were cheap only because they were invisible.

In the real fight (swarm on) the effects that were the problem fell: `update_fx` 9.29 -> 3.09 ms,
`render_collect_parcels` 5.60 -> 1.57 ms, `render_upload_uniforms` 7.80 -> 3.51 ms, the GPU frame
31.8 -> 25.0 ms. **Gap still open:** the player's own burning Tiger's cabin and engine-bay fire
keeps its grain model (thinned six-fold): `fire_smoke_clearance` is still 2.1 ms a update (nine
calls, now dominated by their per-call setup, not by the grain count) and the remaining 11,632
grains cost about 1.6 ms to merge. The infantry swarm (`mixed_observe_act` 18.4 ms) and the thrown
turrets' flight (`update_wrecks` up to 31 ms in a frame while they fly) are not effects and are
unchanged.

### The effect scenes (`fx-lab`, observer cameras; effect cost = frame minus the same view empty)

| Scene | Old: GPU / simulation | New: GPU / simulation |
|---|---|---|
| Burning T-34 at 17 m (`wreck`, near camera) | +1.4 / +0.6 ms | +2.1 / -0.0 ms |
| Three burning wrecks at 60, 250, 700 m (`three`) | +1.8 / +2.0 ms | +0.8 / +0.3 ms |
| Burning wreck 450 m off through the Canyon air (`fog`) | -0.0 / +0.9 ms (draws nothing) | -0.3 / +0.0 ms |
| Cook-off at 130 m, 24 s later (`cookoff`) | -2.6 / -0.6 ms (noise) | +0.4 / -0.0 ms |
| Cook-off at 130 m, the 24 s from the kill, with readback | 23.2 / 18.1 ms per frame | 22.9 / 16.1 ms per frame |

The near wreck is the dearest case: its column fills most of the frame. It costs 2.1 ms at
17 m, under 1 ms at 60 m and beyond, nothing measurable past 300 m.

## Second round (9 November, the owner's notes)

- **Flames stand on the burning surface itself.** Every burning surface patch of the wreck (about
  0.22 m apart, up to 40 per fire) is a flame root with its own surface normal: the flames rise off
  a sloped plate, the deck, a hull side or a corner as the surface lies, not from a flat disc. A
  root is kept by its patch's identity once it has caught and its strength eases toward the
  surface's own over about a second, so the fire no longer jumps between refreshes. The flame's
  box gets its own fine march (16 steps), which removed the speckle.
- **Colour.** A sooty flame's gradient: yellow (1,650-1,750 K) only in the hot core low in the
  body, orange through it (1,250-1,450 K), red-orange at the ragged edges and tips. The colour is an
  authored display ramp (dull red, red-orange, orange, deep orange-yellow, yellow); the three-band
  Planck chroma it replaces put so little in green that the display's tone curve made the flame
  pinkish. Brightness follows the Planck photopic weight compressed to its square root (the full
  weight spans 200x and washed the core white). 1,500 K is 0.45 renderer units (about 2.4e4 cd/m^2).
  Explosions: a fireball peaks near 1,550 K at the core and shows half its emission (soot rolling
  over it); a shell burst's flash peaks near 1,650 K for an instant.
- **Embers** no longer form a funnel: most die low and soon (life skewed short), each wanders on its
  own random-walk path, at most 140 per fire, none past 80 m, dimmer. The pale specks across the sky
  in the darker frames are falling snow made visible by the exposure (`KESSEL_RM_SKIP=4` removes
  them), not embers.
- **The ammunition flash.** The game's blast light for a whole-load cook-off was 20,000-57,000
  renderer units (about 1-3e9 cd) held for 0.85 s. With the new effects it is capped at 1,500 units
  (about 8e7 cd: a burning-propellant fireball of some 400 m^2 at about 2e5 cd/m^2) and falls as
  f^5 over its life.
- **A gentler eye, the default since the owner's yes on 9 November 2026 (`KESSEL_EYE=wide` is the
  eye as it was; it was first shipped as the switch `KESSEL_EYE=gentle`).** Outdoors the eye follows a slow (8 s) log
  average of the field and only a third of each change about it, within 0.5 log units (about 1.7
  stops) either way; a flash leaves a quarter of its afterimage. The default eye is unchanged.
- **The blast's pressure front** is now drawn on the raymarch backend (it was never drawn there): the
  snow it lifts in a band just behind it as it sweeps outward at the speed of sound, thickest where
  the overpressure was highest, and a faint condensation shell while it is above about 8 kPa. The
  front's refraction is still not drawn on this backend. At 319 m/s it crosses 5 m a frame, so it
  shows for a few frames only.
- **Strikes on armour.** Every cannon strike: a flash and puff volume at the struck point, grey off
  the plate; a perforation's darker, the compartment venting through the hole for seven seconds.
  Drawn at any range, so the gunner can read a hit at 600-900 m. A round stopped by the plate no
  longer draws as a perforation (smoke venting from an unholed plate and puffs inside the hull);
  inside the struck hull the blow shakes dust off the plate's inner face. A knocked-out T-34 that is
  not burning smoulders: a thin dark trail for five minutes.
- **Machine guns on armour.** Each hit keeps its spark spray, pock mark and recorded strike; added: a
  ricochet whine off glancing hits (HT-07, pitched up and quieter) and a burst's hail (HT-02, at
  most once per 1.6 s per vehicle), each varied per hit in pitch and level. A machine gun's
  propellant smoke no longer draws as a metres-long white stream (its millimetre grains were merged
  into 8-45 cm smoke cells).
- **Cigarette.** Its smoke and ash merge into centimetre cells with no coverage floor (each wisp
  grain had become a smoke ball across the view), the sidestream is 40 % and the exhale 45 % of
  before, and the coal glows dull red at rest and flares orange on the draw.
- **Column size options** (`KESSEL_FX_COLUMN=H,W,D`): see below.
- **Hot fragments** after a penetration: the struck part's perforation is solved once per contact
  instead of twice (each a component walk with allocations). The 3 s stall of the 9 November run
  is mostly the synchronous fragment-damage solve and the plate-solve join, not the effects.

### How big a burning T-34's smoke column really is

A burning tank releases very roughly 3-10 MW. By Briggs' plume-rise formulas (buoyancy flux about
30 m^4/s^3 for 3.5 MW convected) in the Canyon's 5.5 m/s wind the plume rises about 55 m before it
levels off and trails downwind:

- **At 10 s:** the starting plume's head is at about 30-40 m.
- **At 1 min:** it reaches its full rise of about 55 m, with a trail a few hundred metres downwind.
- **At 10 min:** with a third of the heat it reaches about 40 m.

In still air it would stand two to three times higher. The present column (about 100 m) is
therefore about twice a real one in that wind. The pick sheets show the same wreck three ways:
`fx-overhaul-20261109-fxcolumn-options-three-tiger-view.jpg`, `...-options-wreck-wide.jpg` and
`...-options-wreck-column.jpg` (A: current, about 100 m; B: about 55 m, width 0.8; C: about 40 m,
width 0.6, density 0.7).

## Captures (`/home/alex/panzer/captures/`)

Each scene has 5120x2880 stills and 5120x2880 30 fps clips per variant
(`fx-overhaul-20261109-fxlab-<scene>-<old|new>-<shot>`) and side by side, old left, new right,
labelled with the measured cost (`...-<scene>-old-left-new-right-<shot>.jpg/.mp4`, 4096x1152):

- `wreck`: `near`, `wide`, `column`, clip `orbit`
- `cookoff`: `t0.15s`, `t0.6s`, `t2s`, `t6s`, `t12s`, `t24s`, clip `kill`
- `impact`: `t0.05s`, `t0.3s`, `t1s`, `t3s`, clip `strikes`
- `fog`: `wreck450m`, `wreck450m-binocular`, `cookoff-t0.4s`, `cookoff-t3s`, `cookoff-t10s`, clip `far`
- `three`: `tiger-view`, clip `pan`
- `shock` (new only): `he40m-f1..f5`, `cookoff150m-f3..f25`, clips `he40m`, `cookoff150m`
- `sight` (old and new; observer camera with the TZF's field on the line of fire, real Pzgr. 39
  rounds at 300, 600 and 900 m): `<range>m-hit`, `-hit+0.5s`, `-hit+3s`, `-hit+8s`, `-hit+15s`, clips `<range>m`
- `mg` (old and new): `onto-t34-sight`, `onto-t34-near`, `onto-tiger` at frames 20 and 40, clips of each
- cook-off with the gentle eye: `fx-overhaul-20261109-fxlab-cookoff-new-gentle-eye-*`; the
  exposure set old, new and new-gentle-eye: `fx-overhaul-20261109-fxeye-*` and `-fxeye2-*`
- column sizes: `fx-overhaul-20261109-fxcolumn-options-*.jpg`
- cigarette, the loader smoking at his post: `fx-overhaul-20261109-fxcig-old-left-new-right-loader_cig_*.jpg`
  (first fix) and `fx-overhaul-20261109-fxcig-new2-*` (the flare halved, a little more wisp)

Receipts: `fx-overhaul-20261109-fxlab-<scene>-<old|new>.json`.

## What changes in the picture (for a yes or no)

1. **Fires are visible at any range** and their smoke columns stand 35-120 m over the valley: a
   fight at 900 m now leaves black columns on the skyline where it left nothing.
2. **Wreck smoke is black and tall** instead of a few metres of grey; fresh fires are oily black,
   old ones grey and thinner.
3. **A cook-off** is a sooty fireball (1,650 K, orange) that rolls into a dark mushroom with a stem,
   not a white-hot blob; its blast light now flashes and falls off within about a tenth of a second.
   The eye, adapted to the flash, then sees the scene darker for one to two seconds (the old
   held glow kept it adapted); say if that should be softened.
4. **A shell's burst** is a fountain of dark earth and snow over a low base cloud that drifts and
   settles, in place of the baked explosion volumes. It is the weakest of the new effects: at 45 m
   it reads as a pale grey funnel; it wants another pass.
5. **Embers** are a few hundred drifting sparks at a near fire instead of tens of thousands of
   points packed into the flame.
6. **A fire lights its surroundings about a third as strongly** as the old wreck light (about 7
   against 26 renderer units at full strength): the flame's own radiance times its area.
7. **The cupola's relayed vision blocks** (raster relays) show no wreck smoke column now; they drew
   the three old smoke supports. The gunner's sight and every raymarched view draw the new volumes.

## Follow-ups

- The player's own cabin and engine-bay fire: move it to the same volumes (the remaining 2-4 ms in
  the burning-Tiger case).
- The raster relay views: draw the volumes there, or keep one smoke support per near wreck for them.
- The shell burst's look.

