# Kessel — permanent Linux / RTX 4090 handoff

**Handoff date:** 2026-09-02

**Authoritative branch:** `codex/linux-4090-handoff`

**Authoritative platform:** CachyOS Linux, NVIDIA Vulkan, 4090/High

**Historical fallback only:** macOS/Metal and Laptop preset

This handoff makes the Linux workstation the permanent home of Kessel. A Mac frame, package or
timing result may remain useful evidence, but it no longer decides visual scope, simulation scope,
performance budgets or release readiness. The intentionally low internal pixel grid remains the
art direction: it is the reason the game can afford unusually rich light transport, atmosphere,
fragments, destruction, anatomy and persistence per visible pixel.

This document separates three kinds of truth:

- **implemented** means present in the integrated executable;
- **accepted** means reproduced on the restored Linux checkout;
- **locked design** means approved direction that still requires implementation.

Do not turn a locked design statement into a status claim until code and a receipt exist.

## 1. Workstation contract

The sole reference machine is:

| Part | Target |
|---|---|
| OS | CachyOS Linux, kept current as one tested system rather than a broad distro matrix |
| CPU | Intel Core i9-12900K, user-tuned to 5.4 GHz, push/pull 360 mm cooling |
| Memory | 128 GB RAM |
| Storage | NVMe Gen5 project volume; use a native Linux filesystem for the working checkout |
| GPU | NVIDIA GeForce RTX 4090 |
| Graphics API | Vulkan through `wgpu`; hardware ray queries are required for authoritative High results |
| Default quality authority | `KESSEL_GRAPHICS_PRESET=high` |

Thermal or overclock instability is still a failed build. Before blaming the engine for a sporadic
shader compile, test failure or corrupted archive, reproduce once at a known-stable CPU/RAM profile
and record clocks, temperatures and driver version with the receipt.

## 2. What was preserved

The handoff branch integrates the latest editor-authoring line, preserved HAL gameplay/rendering
work, preserved Mac visual/tooling work, and the older first-person/Panzerfaust line where its work
was not already superseded. Safety references created before integration are:

| Reference | Purpose |
|---|---|
| `codex/snapshot-mac-editor-20260902` | exact tracked Mac editor/transfer work in progress |
| `codex/snapshot-hal-wip-20260902` | exact tracked HAL gameplay/rendering work in progress |
| `codex/snapshot-pre-git-mac-stash-20260810` | named reference for the old pre-Git Mac stash |
| `codex/linux-4090-handoff` | integrated branch to continue on Linux |

### Project-task audit

The other Codex tasks attached to this checkout were inspected before freezing the handoff. No
separate task is still running. Their durable work is represented as follows:

- the Mac/Linux workflow and embodied-machine task landed in Git history and
  `docs/EMBODIED_WAR_MACHINE_ROADMAP.md`; its interrupted body-in-seat request remains explicitly
  planned there and in the rescue design below;
- editor authoring, Mac/4090 rendering, asset catalogs, Pizza audio gates, Gaea adapter, crew assets,
  Enigma and Blender-yard planning are merged or documented on this branch;
- the media task's flat 723-file `captures/` collection and the interrupted poster task's source and
  derived concepts are owner-private archive content rather than Git payload;
- the firsthand crew report PDF is preserved under `output/pdf/`, and its interactive testimony
  atlas is copied into `docs/research/crew-testimony-atlas.html`;
- the Soviet-interior/FPS task handed its work onward: the P08/FPS line is preserved in Git history,
  while the final Soviet source/rig remains an explicit asset gap in §9.

This audit records completion and remaining intent; it does not imply that every brainstormed
feature is implemented.

The transfer package should preserve the repository history/bundle, source, private runtime and
authoring assets, licences, documentation, research artifacts, audio, and the flat `captures/`
collection. Build caches such as `target/`, Python caches and platform metadata are intentionally
regenerable and should not be used to judge completeness. Check the package manifest rather than
assuming that a downloaded source archive or ignored asset is present.

`preservation/original-mac-git/` is forensic evidence only. The verified repository bundle is the
supported recovery path; never install the preserved Mac metadata as the active Linux `.git`.

`packaging/linux-handoff-source-files.txt` records the 39 owner-private downloads selected for this
handoff, including both distinct Tiger 3MF files, purchased/source packs, crew research, four project
videos and two visual references. Duplicate downloads and unrelated files are intentionally absent.

The historical 510 MiB macOS DMG and stale Mac package tree under `dist/` are intentionally omitted
from this Linux-authority handoff. They remain only in the retained Mac checkout until Linux is
accepted; neither is source material or a dependency of the Linux build.

Direct SSH transfer was not completed during assembly because the available `alex@hal` connection
did not authenticate. That is not evidence of a bad archive. Use USB or a working local SSH key and
verify the same SHA-256 at both ends.

## 3. Migration safety — do not skip

### Before leaving the Mac

1. Keep the original checkout, its `.git` directory and all source-asset downloads unchanged.
2. Keep the final `.tar.zst`, its external `.sha256`, internal manifest and repository bundle
   together. Do not rename only one of a sidecar pair.
3. Run the archive integrity test and compare its SHA-256 with the written sidecar.
4. A single archive is expected to exceed FAT32's 4 GiB file limit. Use exFAT, NTFS or `rsync`; do
   not split or recompress it casually.
5. Eject the USB volume cleanly. Retain the Mac copy until Linux passes every gate in §6–§8.

For a USB copy on the Mac, replace both absolute paths with the generated archive location and the
mounted exFAT/NTFS volume:

```sh
kessel_transfer_dir=/absolute/path/to/transfer
kessel_archive="$kessel_transfer_dir/kessel-linux-4090-handoff.tar.zst"
kessel_sidecar="$kessel_archive.sha256"
kessel_usb_root=/Volumes/KESSEL_TRANSFER
cd "$kessel_transfer_dir"
shasum -a 256 -c kessel-linux-4090-handoff.tar.zst.sha256
cp -p "$kessel_archive" "$kessel_sidecar" "$kessel_usb_root/"
cd "$kessel_usb_root"
shasum -a 256 -c kessel-linux-4090-handoff.tar.zst.sha256
sync
```

For a resumable direct copy after SSH authentication is repaired, replace the neutral destination:

```sh
kessel_archive=/absolute/path/to/kessel-linux-4090-handoff.tar.zst
kessel_sidecar="$kessel_archive.sha256"
kessel_remote=LINUX_USER@LINUX_HOST:/absolute/path/to/incoming/
rsync -ah --partial --progress \
  "$kessel_archive" \
  "$kessel_sidecar" \
  "$kessel_remote"
```

### On the Linux workstation

1. Copy the archive onto the NVMe before extracting. Do not build from the USB stick.
2. Enter the incoming directory and verify integrity after the copy:

   ```sh
   cd /absolute/path/to/incoming
   sha256sum -c kessel-linux-4090-handoff.tar.zst.sha256
   zstd -t kessel-linux-4090-handoff.tar.zst
   ```

   If the actual generated filename differs, use the matching filename recorded in its sidecar.

3. Extract onto a case-sensitive native Linux filesystem and preserve permissions and hard links:

   ```sh
   kessel_restore_root=/absolute/path/to/dev
   mkdir -p "$kessel_restore_root"
   tar --use-compress-program=unzstd -xpf kessel-linux-4090-handoff.tar.zst -C "$kessel_restore_root"
   ```

   Replace `/absolute/path/to/dev` with the explicit permanent directory on the NVMe before running
   either command.

4. Enter the extracted project, then confirm the branch and safety refs before modifying anything:

   ```sh
   cd "$kessel_restore_root/tank_simulator"
   sha256sum -c TRANSFER_MANIFEST.sha256
   git status --short --branch
   git show-ref --heads --tags
   git log --oneline --decorate -12
   git fsck --full
   git bundle list-heads repository-all.bundle
   ```

5. If the working checkout's Git metadata is ever damaged, first verify its internal transfer
   manifest, then clone the included bundle into a new sibling and overlay the already-verified
   non-Git payload:

   ```sh
   cd "$kessel_restore_root/tank_simulator"
   sha256sum -c TRANSFER_MANIFEST.sha256
   cd "$kessel_restore_root"
   git bundle verify tank_simulator/repository-all.bundle
   git clone tank_simulator/repository-all.bundle tank_simulator-restored
   git -C tank_simulator-restored switch codex/linux-4090-handoff
   rsync -a --exclude='.git/' tank_simulator/ tank_simulator-restored/
   cd tank_simulator-restored
   git status --short --branch
   python3 tools/runtime_asset_cook.py validate
   python3 tools/runtime_payload.py validate --release-scope owner-dev
   ```

   The asset payload is not necessarily stored in Git. Never assume a bundle-only clone is the
   complete game. The full manifest applies to the original extracted transfer tree, including its
   `.git`; the recovered clone instead inherits trust from that prior manifest pass plus these Git
   and asset checks.

   The transfer checkout has no configured `origin` by design because its temporary Mac filesystem
   remote would be invalid on Linux. Add the real network upstream only after choosing it.

6. Search the restored tree for case-colliding names, external symlinks and hard-coded old machine
   roots before the first bake. The repository launcher and bake tools must resolve from their own
   location or an explicit argument, not `/Users/...` or `/home/alex/...`.
7. Do not delete the archive, Mac checkout or snapshot refs after compilation alone. The migration
   is complete only after the Vulkan, campaign, controls, audio, asset and LAN gates all pass.
8. After those gates pass and the checkout is in its permanent location, run
   `tools/install_linux_desktop.sh`. It creates a relocatable per-user launcher backed by
   `tools/launch_kessel.sh`; re-run it whenever the repository moves.

## 4. Linux prerequisites

Kessel does not currently pin a `rust-toolchain.toml`. `wgpu` 27 requires Rust 1.88 or newer; the
integrated Mac toolchain was Rust 1.94. Start with current stable Rust, record the exact version and
pin it only after the first clean Linux receipt.

A representative CachyOS/Arch dependency set is:

```sh
sudo pacman -Syu --needed \
  base-devel git rsync tar zstd rustup clang cmake pkgconf \
  alsa-lib systemd-libs \
  libx11 libxrandr libxi libxcursor libxkbcommon wayland \
  vulkan-icd-loader vulkan-tools \
  python python-numpy python-pillow gdal \
  blender ffmpeg imagemagick

rustup toolchain install stable
rustup default stable
rustc --version
cargo --version
```

Package names can move in a rolling distribution. Resolve renamed packages through the current
CachyOS/Arch repository rather than installing an unrelated substitute. Install the NVIDIA kernel
module/driver recommended for the exact CachyOS kernel; do not blindly mix `nvidia`, `nvidia-open`,
DKMS and kernel-specific packages. The acceptance facts are more important than the package name:

```sh
nvidia-smi
vulkaninfo --summary
```

Both must identify the RTX 4090 without `llvmpipe`, Nouveau or another software/fallback adapter.
For authoring, existing bake reports were produced with Blender 4.5.9, while the latest Mac
authoring installation was Blender 5.2.1 LTS. Preserve the recorded bake version for reproducibility;
before rebaking production assets with a newer Blender, compare geometry, transforms, materials and
hash receipts. Python uses the distribution's current Python 3 plus NumPy, Pillow and GDAL. `ffmpeg`
supports video/audio proof, ImageMagick is optional tooling, and PlatformIO is needed only for the
separate Teensy build.

## 5. First boot: give the game the whole GPU

The earlier HAL session had Ollama/llama-server reserving roughly 18 GB of VRAM. Do not validate the
renderer in that state. Stop whichever service actually exists, confirm no inference process
remains, and record free VRAM:

```sh
systemctl --user stop ollama.service
sudo systemctl stop ollama.service
pgrep -af 'ollama|llama-server'
nvidia-smi
```

It is normal for one or both service-stop commands to report that no such unit exists. Do not remove
models. Once the game passes in isolation, coexistence with a local model can be measured as a
separate budget; it may never redefine the clean renderer baseline.

## 6. Build and deterministic gates

Run these from the repository root. Preserve complete terminal output under
`artifacts/linux-4090-acceptance/` with the commit, Rust version, NVIDIA driver and date.

```sh
cargo fmt --all -- --check
cargo test --locked --no-fail-fast
cargo build --release --locked
python3 tools/runtime_asset_cook.py validate
python3 tools/runtime_payload.py validate --release-scope owner-dev
```

Do not rewrite either asset lock merely to make migration errors disappear. A failure can mean an
asset was omitted, a private file changed, or newly integrated source introduced a runtime literal.
Investigate and review the delta first.

The integrated pre-transfer receipt has 703 passing tests across all targets and one real-host-audio
test ignored. A restricted sandbox initially denied three loopback-socket tests; all 22 LAN tests
passed when socket access was available. Treat this as comparison evidence, not a promised Linux
count: the accepted Linux receipt is whatever the restored branch produces with zero unexpected
failures.

## 7. Vulkan / ray-tracing acceptance gate

Run the probe first, then real High scenes. `rtprobe` alone is insufficient because it does not
compile and exercise the complete surface and volume pipeline.

```sh
KESSEL_GRAPHICS_PRESET=high cargo run --release -- rtprobe linux4090-rtprobe-20260902.png
KESSEL_GRAPHICS_PRESET=high cargo run --release -- screenshot linux4090-high-scene-20260902.png
KESSEL_GRAPHICS_PRESET=high cargo run --release -- atmosphere-grid /tmp/kessel-linux4090-atmosphere-20260902
KESSEL_GRAPHICS_PRESET=high cargo run --release -- gfxbench koop 240 linux4090-koop-20260902.png
```

Bare screenshot/prefix names are routed to the flat repository `captures/` directory. The explicit
`/tmp` atmosphere directory is a deliberate scratch override; do not create subdirectories inside
`captures/`.

The terminal receipt must show all of the following:

- adapter name identifies **NVIDIA GeForce RTX 4090**;
- backend is **Vulkan**;
- `ray_query=true`;
- `ray_vertex_return=true`;
- the renderer prints `ray-query pipeline enabled; deferred lighting available` (the **RT ON**
  state), not the forward/fallback message;
- the full High surface, smoke/volume and post pipelines compile without validation errors;
- the real scene renders, the 2×2 crew benchmark completes, and output pixels are not black,
  magenta, NaN-corrupted or stretched with uneven nearest-neighbor pixels;
- benchmark output records scene, resolution, frame count and average/p50/p99, rather than quoting
  an uncaptured FPS impression.

Reject the migration if the selected adapter is integrated graphics/software, either ray feature is
false, the High path silently falls back, or only the probe succeeds. Fix the environment or code;
do not weaken the required features to turn the log green.

## 8. Full playable smoke matrix

### Campaign and persistence

```sh
cargo run --release -- campaign-verify /tmp/kessel-linux4090-campaign-20260902
cargo run --release -- campaign-ai-run artifacts/campaign-ai/linux4090-act1.jsonl \
  1 0xc0de194245a10001 1200
```

Then manually start **KAMPAGNE** and verify all three current acts load, the same Tiger carries fuel,
ammunition, damage, discoveries and crew state forward, dismount/remount works, failure/retry rules
match the current build, and no asset resolves through an old Mac path. This checks the implemented
Halbe prototype only; it does not accept the future Reichstag rewrite.

### Controls and embodiment

- Keyboard/mouse: driving, steering, gun lay/fire, manual loading, optics, physical map/compass,
  interact/carry/place/throw, climb/dismount/remount, cupola hatch and station movement.
- One through four controllers: both couch-device modes, join/leave, Commander fallback, each
  assigned post, map/control overlays, vibration and casualty takeover.
- Verify a KIA leaves a persistent body, the post opens and a survivor must take it over. Record the
  current behavior before adding medical state.
- The driver front hatch, radio front hatch and turret-side escape hatch are **remaining work**; do
  not mark the hatch gate complete merely because the cupola operates.

The existing deterministic couch proof is:

```sh
KESSEL_GRAPHICS_PRESET=high cargo run --release -- koop linux4090-koop-smoke-20260902
```

### Audio

```sh
cargo run --release -- soundbank /tmp/kessel-linux4090-soundbank-20260902
cargo run --release -- sfx linux4090-sfx-20260902
```

On a real output device, check engine start/idle/load/exhaust, tracks and gear changes, main gun and
coax, directional armor impacts, interior sympathetic rattle, concussion/tinnitus recovery, fire,
weather and the buttoned/open-hatch transition. Test stereo headphones and the intended speaker
path. A deterministic WAV receipt does not replace the ALSA/device test that was ignored on Mac.

### LAN crew co-op

First run the two-socket local proof:

```sh
cargo run --release -- lan-verify
```

Then use two physical machines on the same LAN:

```sh
# host
cargo run --release -- --host --mission campaign --role commander --name OTTO --seed 1942

# client; replace the address with the workstation's LAN address
cargo run --release -- --join 192.168.1.50 --role gunner --name KARL
```

Exercise 2-, 3- and 5-seat sessions; disconnect/reconnect; role rejection; late join; campaign state,
building damage and crew/body replication; manual loading; takeover after a casualty; and firewall
behavior on UDP 28460. Single-player remains the design authority, so co-op must express the same
physical labor and losses rather than becoming a separate arcade rule set.

## 9. Known asset and integration gaps

- Runtime Tiger/T-34 bakes exist, but the hand-authored Tiger master
  `assets/tanks/tiger/Panzerkampfwagen_VI_Tiger_Ausf.E.blend` was not found during the transfer audit.
- The T-34 source expected by the current bake script,
  `assets/new_tanks/tank-t-3476-1942/source/T-34-76.fbx`, was also not found. Search old HAL storage,
  backup disks and the retained source-download set before attempting a substitute rebake.
- The exact CC-BY Soviet tank/interior source and rig remain unresolved; the existing procedural
  fallback is not the final interior art.
- Exact raw authoring inputs for the Panzertruppe crew, skeleton, Enigma machine and city metal
  bridge are not in the transfer tree, although their attributed runtime derivatives, licences and
  hash receipts are. Recover the original crew archive, skeleton ZIP/FBX, Enigma FBX/textures and
  bridge FBX/textures from retained storage or their documented sources before attempting a rebake.
- The generated Pizza asset-yard library/manifest is owner-private and regenerated outside the
  runtime closure. Confirm its expected generated manifest after the full private asset overlay.
- The selected Pizza audio bank is not cooked: 11 reviewed cue entries still have null byte/hash
  pins, `assets/sounds/pizza_private/` is absent, and runtime correctly remains procedural-only.
  Its selected **Rust & Blood - SFX Library v1.1d** source tree is also absent and must be recovered.
  The retained Echoes, ROT and Pizza ZIPs are only future audition candidates, not substitutes.
- The regenerated authoritative GLB catalog contains **155 files / 241,657,292 bytes**: 120 Pizza
  models plus 35 non-Pizza models. The latter include 16 Tiger/T-34 models, all 15 static
  Panzertruppe identity/pose meshes, the Panzertruppe binoculars, Enigma machine, P08 and skeleton.
  Reproduce that exact closure with the model validator after extraction.
- The regenerated non-GLB payload catalog contains **22 files / 399,651,573 bytes**, making the two
  current runtime catalogs **177 files / 641,308,865 bytes** together. Newly integrated literals can
  make this payload catalog stale independently of the model catalog. Regenerate/review it when
  required, validate it, and take `file_count` and `total_bytes` directly from that accepted catalog
  for future receipts rather than copying this prose total forward.
- No Gemma/Ollama `.gguf`, safetensors, ONNX or PyTorch model is part of the repository. The future
  Enigma language interface must choose and version a local model separately; gameplay cannot
  depend on a model until deterministic fallback, memory/VRAM coexistence and packaging are proved.
- Public/friend distribution remains blocked on tank/P08 provenance, selected terrain provenance,
  Pizza purchase/terms review and dependency notices. Owner-private migration is not permission to
  redistribute the asset warehouse.

## 10. Locked crew and medical design

Single-player is the primary mode. The target is one continuous four-to-six-hour ironman run across
three level loads with a rolling irreversible save. One fixed five-person roster begins it—commander,
gunner, loader, driver and radio operator/bow machine gunner—and no replacements exist. Four local
couch views are only an input/display cap; LAN may seat all five posts. Losing a crewman is not a
temporary debuff: his body remains, his station stops performing labor, and the survivors must
physically cover the missing role for the remainder of the campaign. Multiple losses can leave the
Tiger functionally doomed. Commander death ends the run.

The crew must not be partitioned into collectible biographies or isolated dramatic side quests.
The player **is the crew**. Attachment comes from the loader catching the next round, the driver
holding a line under fire, the gunner keeping the sight, the radio post noticing something, and the
painful extra movement after any one of those functions goes silent. Avoid relationship meters,
portrait popups and omniscient casualty banners. Terse situated reports and physical absence carry
the emotion.

### Trauma state

- One penetration solution owns shrapnel trajectories, detailed body/limb/organ contacts, visible
  entry wounds, damage, FX and collision audio.
- Track hidden blood volume in millilitres, vessel/wound-dependent loss, pressure, shock,
  consciousness and organ function. Expose pallor, responsiveness, breathing, pooling, soaked cloth
  and pulse checks—not a health bar or exact death timer.
- Heart, lungs, liver, head/brain and major vessels must matter, but organ detail must serve legible
  decisions. Do not promise medically exact outcomes from coarse geometry.
- Direct pressure, wound packing and bandaging are separate physical steps. Tourniquets work only on
  eligible limb hemorrhage and carry ischemia/performance consequences. Core/internal catastrophic
  trauma may be untreatable in the field.
- Treatment occurs in real time while the battle continues. Retrieving a kit, opening a hatch,
  reaching/dragging the body, exposing the wound, selecting a tool, applying it correctly, checking
  bleeding and returning to a post all cost time and attention. Often the tactically correct truth is
  that opening the tank to help means both men die.
- The starting German kit may include period bandages/gauze, tourniquet material, stimulants and
  cigarettes, but exact contents, labels, dosages and effects require historical and medical review.
  Never turn amphetamines, tobacco or transfusion into a clean magical heal.
- Give the kit a physical period-styled pictogram card so a player can learn pressure, packing and
  tourniquet placement without pausing for a modern medical HUD.
- Correct treatment improves a casualty's seeded, physiology-derived survival window but need not
  guarantee recovery. Any uncertainty must follow recorded state such as blood lost, organ injury,
  shock, delay and contamination—not an unexplained death roll after identical perfect care.
- A stabilized amputee can return in a later act after an explicit rehabilitation time jump, with a
  crude prosthesis and a lasting role-relevant loss of speed/range/strength. This is an acknowledged
  dramatic abstraction, not same-day field-surgery realism.

### Soviet aid posts

Each map may hide one fixed Soviet aid post. It is a scarce, dangerous opportunity rather than a
shop or respawn room. Armed defenders protect it; wounded Soviet patients and medical personnel are
noncombatants. The player must identify targets under pressure, and stray rounds, fire or demolition
can ruin blood, dressings, instruments or the treatment space. The design must never reward killing
incapacitated patients to “clear” a post.

The tank's kit buys stabilization. An aid post provides the limited chance at definitive supplies or
blood support. Reaching it, securing a route, physically carrying supplies back to the Tiger (or a
future casualty-transport solution), and completing treatment form one uninterrupted survival
problem. Only a living, stabilized casualty is eligible, and the opportunity is effectively
single-use. The chain should be possible, understandable and punishing enough that most rescues fail
for concrete physical reasons—not because of a hidden coin flip after perfect treatment.

In co-op, another player can maintain pressure, fetch equipment, defend the hatch or operate the
tank while treatment continues, but parallel hands do not create extra supplies or omniscient
diagnosis. The host owns wound, blood and treatment state; each crewman receives only local sensory
evidence. The exact control/spectator behavior for a human player's incapacitated body remains to be
designed before network implementation.

### Hatches and station consequences

Add physical state, collision, animation, damage and jamming for the turret-side escape hatch and
both front hatches (driver and radio). They are alternate rescue/bailout routes, ventilation paths,
light/sound apertures and ballistic vulnerabilities. Bodies and loose objects can obstruct movement.
From the cupola, a living crew can execute delegated labor; after a loss, the player must move to the
vacant post. Loader options are Assisted, Hybrid (default) and Manual, but no option manufactures a
replacement crewman.

### Enigma condition reports

The Enigma/message machine is the only sanctioned language surface, not a supernatural narrator.
A future local model may turn deterministic perceived state into short reports such as “Driver is
down, Commander; he isn't moving,” “The loader has lost his right arm,” uncertainty, or silence.
The simulation supplies facts and perception limits. The model cannot decide injury, survival,
enemy position, objectives or endings, and its output must be replayable/fallback-safe when the
model is absent.

## 11. FX and pyrotechnics direction

Primitive independent emitters are to become one causally linked effect system:

1. A shot/impact produces one event containing energy, direction, projectile/fragment trajectories,
   material contacts, pressure and heat.
2. That event drives armor scars, sparks, hot fragments, debris, dust/snow displacement, wounds,
   blood, camera impulse, lighting and spatial sound.
3. Persistent states then evolve: smoke carries mass, temperature and soot; fire consumes fuel and
   oxygen; wind and hatch/fan state move both; surfaces accumulate wetness, soot, blood and debris.
4. The extinguisher emits a physical plume that displaces/suppresses flame and obscures the cramped
   compartment. Engine exhaust follows RPM, load, temperature, damage and weather rather than a
   constant loop. Track dust/snow follows ground material, slip, speed and moisture.
5. Every important particle/volume casts, receives or attenuates light at the tier appropriate to
   its lifetime and distance. The ray-traced scene may use bounded pools and LOD, but “raster-only so
   it cannot affect the world” is not an acceptable fate for hero fire, smoke or fragments.

The low-poly/pixel style controls silhouettes, palette, duration and one-to-three-pixel rhythm. It
does not require effects to be sparse. The RTX 4090 budget should produce dense layered events: a
brief hot core, pressure dust, material-specific fragments, rolling smoke, falling debris, embers and
persistent aftermath, all still crushed into the same severe image language.

## 12. Canonical destination and endings

The campaign's new purpose is Berlin. Orders, clues, fragmented Enigma traffic, wreck evidence and
contact with other tanks gradually reveal that the crew is moving toward a final defense of the
Reichstag. The soft front keeps moving while the player scavenges or treats casualties: time
changes viable roads, enemy density, aid-post danger and final conditions without an arbitrary
glowing countdown.

The **Reichstag last stand is canonical**. It is not a boss fight that can be won. The Tiger is
eventually knocked out; surviving crew, station coverage and accumulated damage determine how the
stand unfolds and how long it lasts. Historical research must still choose the exact Tiger variant,
unit fiction and route. The game may be explicit alternate history, but it must not claim that a
particular documented Tiger I performed this exact defense.

One exceptionally hidden hope/easter-egg ending may coexist with that canon. It begins only after
the Tiger is knocked out at the Reichstag. Every surviving mobile crewman may bail out; they must
cross collapsing Berlin without being shot or captured, locate a usable civilian car and escape.
Failure at any step ends normally. Success cuts forward in time and season to an intact cottage on
green pasture with one warm light, perhaps showing only the car's arrival. There is no wife, child,
reunion, triumphant music, scorecard or explanatory text. The image is quiet enough to remain a
possibility rather than proof. This preserves the crew-as-player, allows surviving comrades to leave
together, stays nearly impossible and never converts the Reichstag into a power fantasy.

## 13. Work that remains after migration

1. Produce and preserve a clean Linux compilation/test/asset receipt.
2. Pass the full Vulkan/RT High gate and establish a repeatable solo plus four-view frame budget.
3. Recover the missing Tiger/T-34 authoring masters before any destructive rebake.
4. Resolve runtime asset catalog drift and package-root portability without weakening validation.
5. Replace fragmented reload/round/case state and add an atomic, versioned, rolling ironman save.
6. Build a dense authored vertical slice before scaling the entire route.
7. Implement one shared impact/fragment/crew/module/FX/audio truth, then anatomy and treatment.
8. Implement the two front hatches, turret escape hatch, dragging, blocked stations and rescue flow.
9. Implement Soviet aid posts with combatant/noncombatant distinction and destructible supplies.
10. Add the deterministic Enigma report contract first; choose/test a local model only afterward.
11. Replace the earlier Halbe endings with Reichstag build-up, last stand and hidden escape route.
12. Run the full solo, controller, audio and 2/3/5-seat LAN matrix before calling the Linux move done.
13. Promote the worldbuilder and terrain workflow into a first-class authoring path rather than a
    collection of one-off tools.
14. Build the main-gun plus engine/track-signature vertical slice before widening the content set.
15. Wire and validate the existing six-tank doctrine/smoke scenario; the ten-tank version remains
    blocked until the hard six-enemy pool is deliberately expanded and re-budgeted.
16. Close provenance and dependency-notice gates, then produce and test the first Linux package.

The archive is the bridge, not the finish line. Keep it until this checklist has a dated Linux
receipt and the next known-good archive exists.
