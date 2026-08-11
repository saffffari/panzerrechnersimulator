# Kessel

A lone Tiger on the wrong side of a collapsing encirclement, retracing its own trail of ruin
westward across the frozen Stalingrad *Kessel* to reach friendly lines — one long, permadeath run.
First-person, solo or five-seat LAN crew co-op, deep-simulation, **ray-traced-PS1**.

> **The pivot.** The first attempt was built in Unreal and died of engine friction — every blocker
> was incidental complexity, never the game. Kessel is now a small cross-platform engine we fully
> own: Rust + `wgpu`, Metal on macOS and Vulkan on Linux. See `docs/ENGINE.md` for the rationale.

## The look: ray-traced PS1

Render the 3D scene at a tiny internal resolution (~480×270) in full HDR with **real-time
path-traced light**, then crush it to a PS1 frame (color-quantize, Bayer-dither, nearest upscale,
vertex-snap). A 1999 silhouette lit by 2026 light transport. The aesthetic and the tech reinforce
each other:

- **low internal res** → path tracing is cheap (~115k pixels; the RTX 4090 has ~70× headroom),
- **low-poly PSX assets** → the ray-tracing BVH is fast,
- **dither + quantize** → hides the path tracer's noise (the crush *is* the denoiser).

## Build & run

Apple Silicon macOS is the primary development platform. The repository uses one `main` branch and
one normal checkout; no platform branches or worktrees are required. Install the stable Rust
toolchain and Xcode Command Line Tools, then run from the repository root:

```sh
cargo run                          # windowed
cargo run -- screenshot shot.png   # headless render → PNG (deterministic verification)
cargo run -- campaign-verify /tmp/kessel-proof  # GPU self-play through all three acts
cargo run -- soundbank /tmp/kessel-soundbank    # render the procedural WAV library
cargo run -- lan-verify                         # real two-socket LAN protocol proof
cargo run --release -- ai-showreel artifacts/ai-showreel  # evolved AI battle film
cargo run --release                # the real thing
```

The complete local asset library is intentionally not stored in Git. During development Kessel
discovers the repository asset root; a packaged app instead reads its cooked assets from
`Kessel.app/Contents/Resources/assets`. See [`packaging/README.md`](packaging/README.md) for the
repeatable owner/developer app build and the stricter public-release checks.

LAN crew co-op is host-authoritative and needs no service or account:

```sh
# host (UDP 28460)
cargo run --release -- --host --mission campaign --role commander --name OTTO --seed 1942

# another machine on the same LAN
cargo run --release -- --join 192.168.1.50 --role gunner --name KARL
```

Up to five people can claim Commander, Driver, Gunner, Loader, and Radio/Bow-MG. With two, the
Commander also drives while the Gunner moves between sight and breech. Three adds a dedicated
Driver, and only that occupied post can move the tank. Visible tanker rigs show who is at each post.
See [`docs/LAN_COOP.md`](docs/LAN_COOP.md).

Navigation is diegetic: M puts a folded paper map, pocket compass, and pencil in your hands while
the battle continues. There is no permanent compass HUD and no eraser. Interior optics use a
high-resolution periscope atlas, binocular inspection, slow eye adaptation, and volumetric daylight
shafts through the glass; gun smoke accumulates in the sealed fighting compartment.

Requires macOS 13 or newer on Apple Silicon for the current Mac build. The Metal forward renderer
is the supported Mac path; Linux retains the Vulkan ray-query renderer when the GPU exposes it.

## Layout

| Path | What |
|---|---|
| `src/` | the engine + game (Rust) |
| `shaders/` | WGSL: raster, compute path tracer, PS1 crush post |
| `assets/pizza/` | PSX Mega Packs — the primary visual language |
| `assets/terrain/` | the real Stalingrad 100 km DEM heightfield |
| `assets/tanks/` | the Tiger (hero), T-34 / KV-1 sources |
| `assets/flavor/` | secondary set-dressing |
| `tools/` | Blender headless asset-bake pipeline |
| `docs/` | design corpus — `ENGINE.md` is the engine authority; `DESIGN_PILLARS.md` / `GAME_DESIGN.md` / `ART_BIBLE.md` / `STALINGRAD_CAMPAIGN.md` the vision |

## Status

The `main` branch contains a playable start-to-finish three-act Campaign: one persistent Tiger,
route-triggered armour encounters, dismounted reconnaissance, story-site objectives, retries in
Acts 1–2, the one-attempt Halbe finale, and multiple victory/defeat codas. Select **KAMPAGNE** from
the title screen. See `docs/CODEX_OVERHAUL.md` for the implementation and verification receipt.
The concise current build/remaining-work ledger is [`docs/STATUS.md`](docs/STATUS.md).

The separate AI battle lab can evolve an embodied Tiger crew, measure one through six evolved
T-34s against it, and record deterministic high-quality battle films with an event-driven camera
director. See [`docs/AI_SHOWREEL.md`](docs/AI_SHOWREEL.md) for the proved five-T-34 threshold,
reproduction commands, and the path to a 100-tank armada.

## Launching

On macOS, development uses `cargo run --release`; packaged owner/test builds launch as a normal
`Kessel.app` and keep resources inside the application bundle. The legacy KDE launcher remains in
`tools/` for Linux testing but is no longer the authoritative development entry point.

Normal launch is a decorated, maximized window. Restore it from the title bar to resize or move it;
the pointer remains free in menus and is captured only during embodied play. Set
`KESSEL_FULLSCREEN=1` only when an explicit borderless-fullscreen takeover is wanted.
