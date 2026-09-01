# Kessel

A lone Tiger on the wrong side of a collapsing encirclement, retracing its own trail of ruin
westward across the frozen Stalingrad *Kessel* to reach friendly lines — one long, permadeath run.
First-person, solo, four-person local split-screen, or five-seat LAN crew co-op, deep-simulation,
**ray-traced-PS1**.

> **The pivot.** The first attempt was built in Unreal and died of engine friction — every blocker
> was incidental complexity, never the game. Kessel is now a small cross-platform engine we fully
> own: Rust + `wgpu`, Metal on macOS and Vulkan on Linux. See `docs/ENGINE.md` for the rationale.

## The look: ray-traced PS1

The target is to render the 3D scene at a tiny internal resolution in full HDR with real-time
path-traced light, then crush it to a PS1 frame (color quantization, Bayer dither, nearest upscale).
A 1999 silhouette lit by 2026 light transport. The aesthetic and the tech reinforce each other:

- **low internal res** → path tracing is cheap (~115k pixels; the RTX 4090 has ~70× headroom),
- **low-poly PSX assets** → the ray-tracing BVH is fast,
- **dither + quantize** → can hide low-sample path-tracing noise.

Current implementation truth: the 4090/Vulkan path has hardware-traced direct sun shadows and one
stochastic sky-visibility ray with temporal accumulation. It does **not yet** have traced indirect
GI, ray-traced reflections, or geometry-shadowed volumetric scattering. Laptop/Metal uses forward
raster lighting. See `docs/ENGINE.md` for the target pipeline and the exact live gap.

## Build & run

Apple Silicon macOS is the primary development platform. The repository uses one `main` branch and
one normal checkout; no platform branches or worktrees are required. Install the stable Rust
toolchain and Xcode Command Line Tools, then run from the repository root:

```sh
cargo run                          # windowed
cargo run -- screenshot shot.png   # headless render → PNG (deterministic verification)
cargo run -- campaign-verify /tmp/kessel-proof  # GPU self-play through all three acts
cargo run --release -- campaign-ai-run artifacts/campaign-ai/act1.jsonl 1 0xc0de194245a10001 1200
                                                # physical Act-I AI + balance JSONL
cargo run -- soundbank /tmp/kessel-soundbank    # render the procedural WAV library
cargo run -- lan-verify                         # real two-socket LAN protocol proof
cargo run -- koop /tmp/kessel-koop              # four-seat split-screen mission proof
cargo run --release -- ai-showreel artifacts/ai-showreel  # evolved AI battle film
cargo run --release                # the real thing
```

Set `KESSEL_GRAPHICS_PRESET=laptop` or `high` to force the 320×180 integrated-GPU budget or the
640×360 4090 budget for headless checks. Split-screen selects High automatically. Render-free AI
rollouts can combine Laptop with `KESSEL_NO_RT=1` to keep GPU memory available for other work.

The complete local asset library is intentionally not stored in Git. During development Kessel
discovers the repository asset root; a packaged app instead reads its cooked assets from
`Kessel.app/Contents/Resources/assets`. See [`packaging/README.md`](packaging/README.md) for the
repeatable owner/developer app build and the stricter public-release checks.

For a four-person couch battle, choose **Crew co-op → Split-screen crew**. Keyboard and mouse own
the Commander while controllers 1–3 own Gunner, Driver, and Loader. The Radio/Bow-MG operator is
deliberately omitted, so all four players have a defined essential job. If a crewman is KIA, a
survivor can take over the open post with **B/Circle**; the keyboard player uses **F6**.
Other seats see the casualty flash immediately, and the fallen body remains in the compartment when
someone takes over the post.
Stützpunkt gets to contact quickly: three dug-in T-34s guard the objective, followed by close
counterattack waves of four, five, and six tanks with short regroup beats.

On foot, fuel is physical. Look at a jerry can and press **E** (controller **A/Cross**) to carry it;
stand behind the Tiger and press E again to refuel. Elsewhere E places it gently. **Q** (controller
**B/Circle**) throws the equipped can. Throw mass and speed follow the litres left, so an empty can
travels much farther than a full one. The current Stützpunkt split-screen battle remains tank-only;
its prepared on-foot pad routing becomes live when couch campaign/body ownership is added.

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
high-resolution periscope atlas, binocular inspection, and slow eye adaptation. Exterior daylight
reaches the compartment only through real openings; gun smoke accumulates in the sealed fighting
compartment.

Requires macOS 13 or newer on Apple Silicon for the current Mac build. The Metal forward renderer
is the supported Mac path; Linux retains the Vulkan ray-query renderer when the GPU exposes it.

## Teensy 4.1 pocket build

[`teensy41/`](teensy41/) contains a separate fixed-memory C++ adaptation for a 480×320 TFT. It is
single-player only and renders a 240×160 indexed battlefield scaled exactly 2×, preserving the
embodied commander/gunner/loader loop, manual reload, armor combat, whiteout palette, and westward escape
without attempting to cross-compile the desktop `wgpu` engine. Its README covers the host preview,
PlatformIO build, default ILI9488 wiring assumption, and the display-controller performance gate.

## Layout

| Path | What |
|---|---|
| `src/` | the engine + game (Rust) |
| `shaders/` | WGSL: raster, hardware ray-query direct lighting, volumes, PS1 crush post |
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
