# Panzerrechnersystem

A lone Tiger on the wrong side of a collapsing encirclement, retracing its own trail of ruin
westward across the frozen Stalingrad *Kessel* to reach friendly lines, carrying its damage and
dwindling supplies through the campaign.
First-person, solo, four-person local split-screen, or five-seat LAN crew co-op, deep-simulation,
**ray-traced-PS1**.

> **The pivot.** The first attempt was built in Unreal and died of engine friction — every blocker
> was incidental complexity, never the game. Panzerrechnersystem is now a small engine we fully own: Rust +
> `wgpu`, developed and shipped first on Linux/Vulkan for one RTX 4090-class workstation. The Metal
> path is retained as historical compatibility, not as a design or performance constraint. See
> `docs/ENGINE.md` for the rationale.

## The look: ray-traced PS1

The target is to render the 3D scene at a tiny internal resolution in full HDR with real-time
path-traced light, then crush it to a PS1 frame (color quantization, Bayer dither, nearest upscale).
A 1999 silhouette lit by 2026 light transport. The aesthetic and the tech reinforce each other:

- **low internal res** → fewer primary samples leave more room for expensive transport,
- **low-poly PSX assets** → a compact visual vocabulary and an opportunity for efficient geometry,
- **dither + quantize** → an intentional final image treatment; temporal noise still needs validation.

Current implementation truth: High on Vulkan uses raster primary visibility, then reconstructs
secondary triangle hits with source materials/textures for one diffuse/GGX bounce and bounded glass
transport. Volumetric lighting traces scene visibility; explicit snow grains also evaluate physical
lighting. This is a bounded hybrid renderer, with substantial accuracy and convergence work remaining.
See `docs/ENGINE.md` for the current architecture.

The September 2026 audit found severe GPU costs, especially per-grain snow lighting, plus a
release-only texture-atlas packing bug. Low resolution does **not** guarantee a fast frame. See
[`docs/ASTRA_NEXT_STAGE_2026-09-05.md`](docs/ASTRA_NEXT_STAGE_2026-09-05.md) for measured workloads,
repairs, the acoustic/art/gameplay audits, and the next short playable milestone.

## Build & run

The game and default executable are named **Panzerrechnersystem** (`panzerrechnersystem`).
Compatibility configuration and archive names are described in [`docs/PRODUCT_NAME.md`](docs/PRODUCT_NAME.md).

CachyOS Linux on the RTX 4090 workstation is the sole authoritative development and performance
platform. The repository uses one integration branch and one normal checkout; platform forks are
not part of the workflow. Install stable Rust plus the Linux/Vulkan dependencies listed in
[`docs/LINUX_4090_HANDOFF.md`](docs/LINUX_4090_HANDOFF.md), then run from the repository root:

```sh
cargo run                          # windowed
cargo run -- screenshot shot.png   # headless render → captures/shot.png
cargo run -- campaign-verify       # GPU self-play through all three acts → captures/
cargo run --release -- train-foot-ai 128         # CPU-only deterministic Tiger/Soviet KSL2 checkpoints
cargo run --release -- campaign-ai-run artifacts/campaign-ai/act1.jsonl 1 0xc0de194245a10001 1200
                                                # physical Act-I AI + balance JSONL
cargo run -- soundbank /tmp/panzerrechnersystem-soundbank    # render the procedural WAV library
cargo run -- lan-verify                         # real two-socket LAN protocol proof
cargo run -- koop                                # four-seat split-screen mission proof → captures/
cargo run --release -- ai-showreel captures      # evolved AI battle film
cargo run --release                # the real thing
```

`captures/` is the canonical, flat home for simulator-made screenshots and videos. Bare output
names and prefixes are routed there automatically; an explicit absolute path or path containing a
directory remains an override. Archived filenames use `__` to encode their former directory path
without creating subfolders or overwriting captures that originally shared a basename.

Set `KESSEL_GRAPHICS_PRESET=high` for authoritative development and validation. The low internal
pixel grid is intentionally retained while ray transport, volumetrics, particles, destruction,
simulation density and persistence consume the 4090's headroom. `laptop` and `KESSEL_NO_RT=1`
remain useful only for render-free automation, fault isolation and historical compatibility; they
must not set visual scope or shipping defaults. Split-screen currently fixes each local viewport at
320×180 while retaining the selected quality tier.

`train-foot-ai [epochs] [output-directory]` writes two role-tagged candidates under
`assets/ai/foot_candidates/epochs_<epochs>` by default and refuses existing outputs.
The active runtime files remain `assets/ai/weights_foot_tiger.bin` and
`assets/ai/weights_foot_soviet.bin`; promotion is a separate evaluated decision. See
[`docs/AI_RECOVERY_20261001.md`](docs/AI_RECOVERY_20261001.md) for the comparison commands,
memory fix, and reviewed policies. Runtime loading rejects a
checkpoint with the wrong role or schema and keeps the scripted mission/doctrine expert active when
either artifact is absent or invalid. To collect the opt-in `kessel.foot_transition.v1` DAgger
stream during a physical campaign rollout, append a separate output path after the Tiger vehicle
checkpoint:

```sh
cargo run --release -- campaign-ai-run artifacts/campaign-ai/act1.jsonl 1 \
  0xc0de194245a10001 1200 assets/ai/weights_academy.bin assets/ai/weights_tiger.bin \
  artifacts/campaign-ai/act1-foot-dagger.jsonl
```

The foot stream records the executed action and its scripted expert correction for both roles. It
does not reinterpret older campaign JSONL as foot-policy training data.

The complete local asset library is intentionally not stored in Git. During development Panzerrechnersystem
discovers the repository asset root; a future Linux package must read a reviewed cooked payload
rather than the raw owner library. The existing `Kessel.app/Contents/Resources/assets` layout and
[`packaging/README.md`](packaging/README.md) remain the repeatable historical Mac owner-build receipt,
not the Linux distribution design.

For a couch battle, choose **Crew co-op**, set **Couch devices**, then choose **Split-screen crew**.
**Keyboard + controllers** keeps keyboard/mouse on Commander and assigns controllers 1–3 to Gunner,
Driver, and Loader. **Controllers only** assigns pads 1–4 to Commander, Gunner, Driver, and Loader,
with no hidden keyboard crewman; one Commander pad can drive, steer, lay, and fire until more pads
join. On a two-pad local setup, pad 1 is Commander/Driver and pad 2 is Gunner/Loader, so both players
have complete jobs and the Tiger remains fully operable. The Radio/Bow-MG operator is deliberately
omitted. If a crewman is KIA, a controller survivor
uses **B/Circle** to take over the open post; the keyboard Commander uses **F6**.
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

The authoritative build requires the workstation's NVIDIA Vulkan driver and exposed ray-query plus
ray-triangle-vertex-return features. A successful `rtprobe` is necessary but not sufficient: a full
High scene must compile and render before the transfer is accepted. The exact gate, expected log
fields and migration checklist are in [`docs/LINUX_4090_HANDOFF.md`](docs/LINUX_4090_HANDOFF.md).

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
| `docs/` | design corpus — `ENGINE.md` is the engine authority; the design, art and campaign documents hold the vision |

## Status

The integrated code contains a playable start-to-finish three-act Campaign: one persistent Tiger,
route-triggered armour encounters, dismounted reconnaissance, story-site objectives, retries in
Acts 1–2, and the earlier Halbe finale with no automatic retry. Select **KAMPAGNE** from the title
screen. The canonical design has now changed: clues, orders and encounters draw the crew toward an impossible
last stand at the Reichstag. Replacing the implemented Halbe endings with that finale is remaining
work, not a claim about the current executable. See [`docs/LINUX_4090_HANDOFF.md`](docs/LINUX_4090_HANDOFF.md)
for the locked decisions and [`docs/STATUS.md`](docs/STATUS.md) for the implementation ledger.

The separate AI battle lab can evolve an embodied Tiger crew, measure one through six evolved
T-34s against it, and record deterministic high-quality battle films with an event-driven camera
director. See [`docs/AI_SHOWREEL.md`](docs/AI_SHOWREEL.md) for the proved five-T-34 threshold,
reproduction commands, and the path to a 100-tank armada.

## Launching

On Linux, `tools/launch_panzerrechnersystem.sh` opens the prebuilt release executable at
`target/release/panzerrechnersystem`. Normal desktop launch does not invoke Cargo. After source
changes, run `tools/launch_panzerrechnersystem.sh --build` to rebuild before launching, then
verify the selected adapter, Vulkan backend and rendered result.

Install the per-user desktop shortcut with `tools/install_linux_desktop.sh`. Double-click
**Panzerrechnersystem** to open the menu; choose **Playtest** for the single-player test range or
**Kampagne** for the campaign. The desktop shortcut also offers **Playtest** in its right-click
actions. Re-run the installer if the checkout moves.

Playtest has six **T-34-76s** and starts in **practice**, with enemy return fire off so you can
learn the sights, loading and movement. Targets stay stationary and take normal damage. Use
**Esc → Settings → Enemy return fire** to enable live combat; turning it back off stops new NPC
shots and brakes the tanks to a stop, while projectiles already in flight remain live.
Live combat restores normal enemy movement and firing. Campaign combat is unchanged.

During solo play, press **Esc** for **Resume**, **Save checkpoint**, **Load checkpoint**,
**Settings**, or **Exit game**. The pause menu stops the simulation and game audio. Campaign saves
use one manual slot and restore the saved **sector entrance**, including its entrance Tiger,
stores, and route; they do not resume at the current position or preserve mid-sector combat.
Save before exiting, then use **Load checkpoint** at the title screen to continue from that
entrance. There is no autosave. See [`docs/CONTROLS.md`](docs/CONTROLS.md#pause-save-and-load) for
controls, checkpoint contents, and the save location.

Normal launch is a decorated, maximized window. Restore it from the title bar to resize or move it;
the pointer remains free in menus and is captured only during embodied play. **Alt + Enter** toggles
fullscreen/windowed, also available under **Esc → Settings**. Switching away pauses local play and
stops rendering until the game is active again; choose **Resume** when you return. Set
`KESSEL_FULLSCREEN=1` only when an explicit borderless-fullscreen takeover is wanted. Existing
`KESSEL_*` configuration variables remain supported for compatibility. The old
`tools/launch_kessel.sh` command forwards to the renamed launcher.

macOS owner/test packages remain historical artifacts; new scaffold builds use the current product
name. See [`docs/PRODUCT_NAME.md`](docs/PRODUCT_NAME.md) for naming and compatibility boundaries.

Graphics and weather controls (including **L** for handheld light): [October 1 guide](docs/GRAPHICS_WEATHER_20261001.md).
