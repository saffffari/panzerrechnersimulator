# Look keep list — 2 November 2026

Every worktree is now merged into `claude/windows-wip-20261101` or archived beside it. This page
says which render, look and visual-style pieces the game keeps, which it drops, and which wait on
the owner's decision. It is built from two read-only inventories (the main line's looks and gates;
the uncommitted edits in the older worktrees) and the owner's recorded direction:

- **deep blues against orange-reds** (cabin reload frame, 1 Oct): warm practicals and hot metal
  against cold daylight, never a uniformly grey or uniformly warm frame;
- **deltaP is "the best one so far"** (1 Nov), and the tinted variant is the launcher default;
- **the "amazing green"** annunciator, the yellow/blue round lamp, a dark daytime cabin lit mostly
  from outside, no star flares, the pastel interior still called "fantastic" (2 Nov).

## Where every worktree stands

| Worktree / branch | State | What it held |
|---|---|---|
| fx-fire-sparks | merged (4579b17) | FEA breach fx outside the hull, Planck chips, thinner interior smoke, hot metal cooling on its own thermal time |
| fea-damage | merged (2a95166) | voxel armour and component damage, fires at the breach, fea-film clips |
| total-war-overhaul (Sep 15) | merged in part (57f42ae) | warscape kept; crew seat-block, 57 t ride and barrage not ported (rebuilt since; see the commit) |
| shader-graphics | uncommitted, archived `archive/wip-shader-graphics-20261102` | almost all already in the main line in a later form |
| reference-pass | uncommitted, archived `archive/wip-reference-pass-20261102` | display work already merged; coating atlas and seasonal ground cover only here |
| old-renderer-review | uncommitted, archived `archive/wip-old-renderer-review-20261102` | all ported since |
| wf_1f9b1365 | uncommitted, archived `archive/wip-raster-fog-emitters-20261102` | raster fog lit by fires and lamps, SSAO debug-only, particles on the laptop raster path |
| tank_simulator | uncommitted, archived `archive/wip-map-stills-20261102` | `map-stills` review tool, render library, reground tool |
| player-showcase | uncommitted, archived `archive/wip-player-showcase-20261102` | capture timeline (gaze, walk, camera trace) |
| wf_f112fe25 | abandoned mid-merge, not archived | rubble clear of open cellars; main line buries rubble instead |
| bluewinter, canyon-v2, canyon-run, atmosphere, urban-*, sound-overhaul, adaptive-audio, bug-squash, playtest-hotfix | committed work already in the main line | nothing outstanding |
| t34-eyes | the enemy-tank session's new worktree at the main line | in progress elsewhere |
| `/home/alex/panzer/codex-last-light` | separate repository, not a worktree | independent gameplay fork, left alone |

The archive branches are snapshots made through a temporary index; the worktrees themselves were
not touched. None of them were built.

## Keep — the shipped look

**Frame.** Raymarch backend. **Superseded the same evening:** after comparing the FEA film's
turret-ring clip (standard display, 1280x720) with deltaP refilms of it, the owner chose the
standard display's colour ("better than our stylized effects on the color as of now"). The launcher
default is now the standard look at 1280x720 (4x on the panel) with the Canyon document's dither
profile; deltaP stays selectable with `--look`. AgX tonemap. Eye adaptation with the
rod channel capped at the display key. Eyepiece lens, optic masks, the etched TZF reticle,
concussion defocus, the one-pixel hover outline.

**Sky.** Ephemeris sun and moon over the spectral atmosphere. Clock starts 07:00, cycle x6.
Ten-day moon month, discs drawn 1.6x with irradiance unchanged. Single-pixel stars capped at 3x
display white. Cloud deck and its shadows with the water path kept. Froxel fog and media. City
fire glow on the cloud base.

**Light sources.** One Planck law for fire pages, embers, vents, fireballs, sparks and rims. Hot
metal cooling on its own thermal time (10 to 36 s). Heat pages drawn at 1.7x their band. VDB
explosions, exhaust plume, sparks and tracers. Penetration smoke at a third of its old density.

**Interior.** Practicals at 0.0012, dial at 0.018. The green annunciator. The round lamp in
yellow, blue and red. Daylight through the panes as measured apertures. Cabin haze and the cabin
medium.

**Surfaces.** Lying snow with relief, whitewash, surface wear, the T-34's 4BO finish, detail
terrain with ruts and craters, hex-tiled ground breakup, grass, the strewn field, and now the
warscape (bodies, wrecks, obstacles, stained snow, drag marks).

## Decide — the owner's call

1. **Pastel cabin as the default.** `--cabin-look pastel` now renders as the liked still did
   (fixed in 4958eba; it was dithering between palette entries). Default today: plain.
2. **The bluewinter white balance.** The 1 Oct blue came from `ambient_white_balance` at 0.5 on
   the old backend. It is off in the demo config. Worth one A/B under deltaP before deciding.
3. **Armour-scar glow.** Linear 20 s (your request, the default) or the solved cooling
   (`KESSEL_SCAR_GLOW=solved`). Two answers to the same question now coexist.
4. **Windows plays a different look.** `tools/launch_windows.ps1` sets no `KESSEL_LOOK`, so
   Windows gets the standard crush while Linux gets tinted. Align it, or keep Windows as the
   reference look.
5. **Seasonal ground cover and flowers** (reference-pass archive). Real but unfinished; it matters
   for the spring and summer acts, not the winter ones. Port when those acts are next.

## Drop or retire

- **Looks nobody selects:** `deltap`, `deltap-pico8`, `deltap-raw`. Keep `pastel`, because the
  cabin uses its palette.
- **Knobs inert under the look:** `KESSEL_DITHER_PATTERN`, palette levels, ordered dither, retro
  strength and LDR dither change nothing under deltaP. They only reach the standard look.
  `KESSEL_RETRO_PRESET` only reaches the physical backend.
- **Dead lanes:** the crush `reticle` lane (the reticle moved to the blit) and
  `cabin_practical_gain`, which returns 1 on both branches.
- **ACES** stays as an env fallback only; three docs still call it the tone curve (CLAUDE.md, the
  ledger's renderer section, DELTAP_LOOK).
- **The binary's default backend** is still the physical one when `KESSEL_RENDERER` is unset.
  Every launcher overrides it; the default itself should become raymarch.
- **Archived pieces not worth porting:** the reference-pass coating atlas (the raymarch
  `vehicle_finish.rs` does this), faceted smoke grains and the sparse winter grass from
  shader-graphics (raster-only experiments), the 0.62 sky-fill multiplier (its function is gone).

## Port — archived pieces worth bringing in

- **Fog lit by fires and lamps on the raster path** (`world_emitter_inscatter`, raster-fog
  archive). The periscope and sight relays are raster captures, and you said the ports "don't
  have the same fog and lighting as outside". This closes part of that gap until the relays go
  raymarch.
- **Raster SSAO behind a debug gate** (same archive). CLAUDE.md forbids presenting screen-space
  occlusion as physical; the main line still runs it by default on raster, which the relays use.
- **Particles on the laptop raster path** and the archive's GPU-eye parity and moonless-radiance
  tests.
- **The barrage** from the total-war branch, rebuilt on the FEA damage solver and the physical
  sound emissions: the main line removed its horizon booms "pending simulated off-map indirect
  fire", and this is that.
