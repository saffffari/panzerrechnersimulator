# Kessel — HAL shoot sheet, Pass A (2026-09-03)

This is the executable, dated version of the master plan in `docs/RENDER_NEW_GRAPHICS.md`, cut
for the first shoot on the permanent Linux/RTX 4090 workstation. The master plan stays the
authority for IDs, naming, receipts and POV rules; this sheet only orders the work, folds in the
2026-08-30/31 sound and asset work, and records what is actually reachable in commit `4ac9dc4`.

Anything marked **planned** below is a reservation, not a claim.

## 0. State of the build this sheet shoots

| Item | Value |
|---|---|
| Commit | `4ac9dc4` on `codex/linux-4090-handoff` (worktree branch for this sheet) |
| Transfer | archive sha256 OK, zstd OK, 9,005/9,005 manifest entries OK, bundle complete, fsck clean |
| Linux receipt | 2026-09-03 00:32: `cargo fmt` clean, locked release build 36 s, all tests pass, 155 GLB / 22 payload validators pass |
| Graphics | rustc 1.96.0, driver 610.57.04, RTX 4090 24 GB. Presets in code are `laptop` and `high` (label "4090"). There is no separate "ultra" preset; **High is the max path** |
| Still open | Vulkan/ray-query gate (section 7 of the handoff) has **not** been run post-transfer. No `hal4090_*` or `linux4090*` frame exists in `captures/` yet |
| Audio | 79-cue deterministic audition (2026-08-31), Tiger impact rebuild (2026-08-30), procedural runtime bank only; Pizza bank uncooked |
| New assets since last shoot | jerry can GLB, Pizza asset yard (120 models), anatomical crew ragdoll v2, koop blood aftermath, Soviet infantry volley, modular buildings, Act I six-shell blizzard |

Missing masters (Tiger `.blend`, T-34 FBX, Rust & Blood SFX tree) do not block Pass A; they block
rebakes and the Pizza audio cook.

## 1. Shoot order

Run in this order. Do not start block B until block A's receipt exists.

### Block A — hardware gate (today, ~1 h)

Make sure nothing but the desktop is on the GPU (no Ollama or llama-server). Then:

```sh
KESSEL_GRAPHICS_PRESET=high cargo run --release -- rtprobe hal4090_gate_g01_rtprobe_20260903.png
KESSEL_GRAPHICS_PRESET=high cargo run --release -- screenshot hal4090_gate_g07_whiteout-wide_20260903.png
KESSEL_GRAPHICS_PRESET=high cargo run --release -- rt-bvhview hal4090_gate_g02_bvh-day_20260903
KESSEL_GRAPHICS_PRESET=high cargo run --release -- rt-bvhview hal4090_gate_g03_bvh-night_20260903 nacht
KESSEL_GRAPHICS_PRESET=high cargo run --release -- rt-parity hal4090_gate_g04_parity_20260903
KESSEL_GRAPHICS_PRESET=high cargo run --release -- rt-accum hal4090_gate_g05_accum_20260903
KESSEL_GRAPHICS_PRESET=high cargo run --release -- rt-sun hal4090_gate_g06_rtsun_20260903
KESSEL_GRAPHICS_PRESET=high cargo run --release -- atmosphere-grid /tmp/kessel-hal4090-atmosphere-20260903
KESSEL_GRAPHICS_PRESET=high cargo run --release -- gfxbench tag 600 hal4090_gate_g13_gfxbench-day_20260903.png high
KESSEL_GRAPHICS_PRESET=high cargo run --release -- gfxbench nacht 600 hal4090_gate_g13_gfxbench-night_20260903.png high
KESSEL_GRAPHICS_PRESET=high cargo run --release -- gfxbench koop 600 hal4090_gate_g12_koop-bench_20260903.png
```

Save every terminal log to `artifacts/linux-4090-acceptance/rt-gate-20260903/`. The gate passes
only if the log shows all of: adapter **NVIDIA GeForce RTX 4090**, backend **Vulkan**,
`ray_query=true`, `ray_vertex_return=true`, and `ray-query pipeline enabled; deferred lighting
available`. Any fallback message fails the gate and stops the shoot.

Gate stills: G01–G07, G12, G13, plus V12 timing from the three gfxbench runs.

### Block B — deterministic harness re-render (same day, ~2 h)

Every harness in the master plan's section 7, with today's date token. All 28 subcommands are
confirmed present in `src/main.rs` at this commit. The loops below are bash, not fish.

```sh
for h in action-shots penfx overpen damage-reel fire firefight terror-video; do
  KESSEL_GRAPHICS_PRESET=high cargo run --release -- $h hal4090_fx_${h}_20260903
done
for h in replay reload cupola drive mg; do
  KESSEL_GRAPHICS_PRESET=high cargo run --release -- $h hal4090_gameplay_${h}_20260903
done
for h in crush treefall; do
  KESSEL_GRAPHICS_PRESET=high cargo run --release -- $h hal4090_world_${h}_20260903
done
KESSEL_GRAPHICS_PRESET=high cargo run --release -- koop hal4090_coop_koop_20260903
KESSEL_GRAPHICS_PRESET=high cargo run --release -- soviet-shot hal4090_ai_soviet-shot_20260903.png
KESSEL_GRAPHICS_PRESET=high cargo run --release -- ai-showreel captures
KESSEL_GRAPHICS_PRESET=high cargo run --release -- campaign-verify captures
```

Harness outputs cover F01–F14 diagnostics, S14–S18, S44, S35–S36 and G10/G11 companions.

### Block C — new-asset and FX beauty stills (next session)

These are the frames that did not exist when the master plan was written. Each needs a beauty
take and a diagnostic twin.

| ID | Frame | Asset or system it proves |
|---|---|---|
| N01 | Act I whiteout, Tiger three-quarter, six snow shells and spindrift visible in depth | 2026-08-29 blizzard rework (G07 beauty) |
| N02 | Interior, protected, all local lights, firefly heatmap twin | 2026-08-30 interior firefly clean-up (G10) |
| N03 | Jerry can: held, placed at rear deck, thrown empty vs full, abandoned in snow | jerry can GLB + physical fuel (S28) |
| N04 | Koop split with blood flight, ricochet, pooling and aftermath in each cell | crew ragdoll v2 + koop blood work (S18, V05) |
| N05 | Soviet rifle volley, five synchronized rifles, alarm state | Soviet infantry (S35) |
| N06 | Modular building: intact, breached, collapsed, rubble route | modular building gallery (S33, F10) |
| N07 | Pizza asset yard dressing wide: at least one frame per act with catalogued props | 120 Pizza models, `assets.csv` |
| N08 | Enemy hull footprints: two T-34s pressed together without overlap | parts-derived footprints (S36) |
| N09 | Tiger armor dent/hole persistence after repeat hits, same camera, hit 1 / hit 3 | persistent local armor damage (S14, S15) |
| N10 | Panzertruppe static identity meshes, all 15, one contact sheet | non-Pizza GLB closure (S03) |

### Block D — sound proof pass (next session, headphones)

The 2026-08-31 audition proved 79 cues in isolation, peak-normalized. Pass A owes the **in-game**
relative mix of the same identities. Record V11 as one uncut clip plus the eight short takes
below, each with a spectrogram twin from the capture log. Cue names are the audition sheet's.

| ID | Take | Cues that must be audible and correctly placed |
|---|---|---|
| A01 | Buttoned vs open, engine idle to full load | `engine_idle_600rpm` → `engine_full_load_2500rpm`, `runtime_ambience_interior/exterior` |
| A02 | Eight-gear forward sweep from the driver's post | `engine_forward_8_gear_sweep`, track/terrain if live |
| A03 | Main gun from gunner post, then from outside at 50 m | `runtime_player_gun_int_*`, `runtime_player_gun_ext_*`, `main_gun_recoil` |
| A04 | Full reload cycle, uncut | `breech_open`, `casing_extract`, `casing_drop`, `shell_lift`, `shell_ram`, `breech_lock`, `runtime_brass_clatter` |
| A05 | Turret traverse interior crawl / lay / full, then exterior | `turret_interior_*`, `turret_exterior_full`, `turret_speed_load_perspective_sweep` |
| A06 | Incoming 76 mm matrix, four takes, camera fixed | `live_76mm_long_range_front_ricochet`, `live_76mm_long_range_oblique_weak`, `live_76mm_close_right_side_penetration`, `live_122mm_heavy_oblique_filled_penetration` |
| A07 | Penetration interior: fragments and hearing trauma | `fragment_contact_*`, `fragment_whizz_*`, `hearing_trauma_moderate/heavy`, `body_impact` |
| A08 | Catastrophic kill chain at 200 m | `destruction_dry_kill`, `destruction_dry_turret_pop`, `destruction_dry_turret_rip`, `destruction_dry_cook_off`, delayed `gun_report_distant_right` |

The 51 cue systems in the workbook's *Missing Backlog* (tracked-machine, crew-humanity, on-foot,
destruction, environment, mix) are **planned** and get no take until they are wired.

### Block E — player-controlled POV masters (after C and D)

From the master plan, only the reels whose beats are live now: P01, P02, P03, V04, V05, V09, V10.
3840x2160 at 60 fps, integer-nearest from the 640x360 High grid, 48 kHz stereo, uncut task chains.
One keyboard take and one controller take each for P01 and P03.

Hold P04–P08, V07 and every `spoiler_` frame; the crew-trauma, hatches, aid posts, Enigma and
Reichstag work is locked design and not in this executable.

## 2. Per-take receipt

Copy this block into the take log for every capture. The master plan's checklist is the authority;
this is the short form.

```text
take:       hal4090_<class>_<id>_<slug>_a<act>_s<seed>_t<take>_20260903
commit:     4ac9dc4  dirty: no
preset:     high  internal: 640x360  output: 3840x2160 nearest
adapter:    NVIDIA GeForce RTX 4090 / Vulkan  ray_query=true  ray_vertex_return=true  RT ON
driver:     610.57.04   rustc: 1.96.0
scene/act/seed/take:
frames/dropped:   audio clip/peak:
notes:
```

## 3. Sign-off for Pass A

- [ ] Block A gate log preserved and every required line present.
- [ ] Every harness in block B re-rendered at this commit with today's date.
- [ ] N01–N10 have a beauty still and a diagnostic twin.
- [ ] A01–A08 recorded, reviewed on headphones, mix notes written back into the audition workbook.
- [ ] P01, P02, P03, V04, V05, V09, V10 masters play start to finish with sync.
- [ ] `docs/RENDER_NEW_GRAPHICS.md` section 8 Pass A boxes ticked from this evidence, not from memory.
