# Kessel — current status (2026-08-10)

Kessel is a playable three-act prototype developed from one `main` checkout on Apple Silicon macOS.
The Mac is the development machine and source of truth; Hal/Linux is only a future Vulkan
compatibility target. The Mac build uses Metal's forward path and does not create or package the
Vulkan ray-query resources.

## Playable now

- Acts 1–3 carry one Tiger, its damage, stores, discoveries, and route state through the campaign.
- Solo, split-screen seats, and host-authoritative LAN crew play use the same embodied commander,
  driver, gunner, loader, and radio/bow-MG roles.
- The interior, cupola/hatches, optics, manual loading, tank movement, on-foot scouting and
  scavenging, paper map, stealth, armor/spall/module damage, fire, repair, crush, shell destruction,
  T-34 AI and encounter/platoon logic are working systems.
- The authored campaign worlds and encounter placement remain thin. Kessel has the mechanical loop,
  but not yet the dense, dangerous, explorable warzone it is meant to become.

## Mac renderer and atmosphere

- Winter has a dense, depth-aware, crisp-pixel blizzard with world-fixed flakes, gusts, spindrift,
  interior shelter masking, and usable visibility through open hatches and optics.
- Every act has deterministic dawn/day/dusk/night profiles. Overcast, haze, smoke and restrained
  visibility replace the old clear-sky, glorious-weather look.
- Game and editor share terrain and manifests. Placements compile into 128 m pages with shared GPU
  prototypes and packed instance buffers.
- A renderer-owned spatial-cell index culls page work for local camera and shadow queries instead of
  scanning the whole world. The current maps contain 12,438 / 20,474 / 36,438 placements, although
  Act I is still a small city patch in a large field and Acts II–III are still tree-heavy.

## Playable building and world foundation

- A live two-storey Pizza building is enterable on foot and contains four rooms, real doorways,
  windows and stairs, interior lights, weather shelter, shelves and loot.
- Shell and tank-crush damage can remove supported structure and produce grounded rubble. Rendering,
  collision, cover/line-of-sight, lighting and shelter respond to the surviving structure.
- The modular compiler/runtime has stable instance IDs, floor/room/portal data, support topology,
  compact damage and loot state, plus incremental render/nav/collision dirty sets.
- The one featured current-act building is keyed by act plus stable instance/layout identity in the
  in-memory campaign checkpoint. Load/retry restores it, successful completion commits it, and a
  blocked/lost completion cannot overwrite it. This is not a disk-save system. Mutable state is
  exact module integrity, 1/256-quantized sub-point damage, and atomic shelf/container state. All
  current doorways are permanently open; door-leaf gameplay and partial per-item loot are not live.
- A compact bounded copy of that state is host-authoritative over LAN and is included for late
  joiners. Gameplay protocol v3 uses a host epoch and fresh client challenge to reject stale and
  duplicate packets without reopening old sequence spaces across wrap or host restart; discovery
  remains format/version v1.
- This is one proved vertical slice, not a finished city system. Most landmarks remain sealed or
  placeholder geometry, and the editor still needs first-class floorplan, room, stair, door, loot
  and structural authoring.

## Runtime assets and Mac package

- The current reviewed model closure is exactly **134 GLBs / 231,175,716 bytes**: **118 Pizza**
  models / 79,157,108 bytes plus exactly **16 current Tiger/T-34** models / 152,018,608 bytes.
- With the 21-file non-GLB payload, the exact locked resource input is **630,266,093 bytes**. Raw
  authoring libraries, source archives, screenshots, video and retired models are not bundled.
- The current owner-development checkpoint is `dist/macos-f8496b9-final/`: its app occupies
  **627,716 KiB (about 613 MiB)** and its compressed DMG is **534,839,582 bytes (about 510 MiB)**.
  The DMG SHA-256 is
  `2d57a97bf0e686e2868b0bc146f5efd2bced11992bc5f7904cb13257911d3d72`.
- The ignored artifact presently in `dist/macos/` is an internally valid but stale owner/developer
  snapshot. It embeds the older **147-GLB / 491,705,164-byte** model closure and must not be presented
  as a build of current `main`.
- Current Mac source receipt: **214 passed, 0 failed, 1 real-audio-device test ignored** under the
  full locked suite; the locked release build also passes. The current app and DMG independently
  pass resource hashes, ad-hoc hardened-runtime signatures, architecture/minimum-OS checks and disk
  image verification. With repository asset overrides removed and its working directory outside
  the repository, the bundled executable loaded Garage and all three acts from `Contents/Resources`
  and rendered one Garage frame plus 12 Metal dawn/day/dusk/night frames. The atmosphere grid is
  exact 2x-nearest from 640x360; the Garage frame is exact 3x-nearest.

Friend/public distribution is still blocked by the 16 tank models' unrecorded provenance, unverified
Mapzen-derived Garage/Act I redistribution rights, review and bundling of applicable Rust dependency
licences, and a re-check of Pizza Doggy terms and purchase evidence. A standard friend build also
still needs Developer ID signing/notarization, local-network permission/firewall and cross-machine
session QA, controller and 2/3/5-crew testing, and a clean-account downloaded-DMG launch test.

## Immediate production order

1. **Scale the vertical slice.** Turn the proved building into a dense 500×500 m warzone block with
   varied multi-floor interiors, destruction, scavenging, debris, cover, forest and streets. Keep
   compact building state bounded and authoritative as instance count grows.
2. **Worldbuilder v2.** Add floorplan/room/door/stair/loot tools, structural tagging and collapse
   preview, density brushes, spline roads/trenches/patrols, semantic zones, encounter sockets and
   validation overlays. Store procedural rule+seed layers instead of enormous hand-authored lists.
3. **Terrain and streaming.** Replace the unusably coarse ground treatment with chunked/LOD terrain
   and layered tiled materials; add vegetation LOD, interest-managed collision/destruction and
   room/portal streaming across all local and networked players.
4. **Danger everywhere.** Author hull-down, ambush, patrol, fallback, reinforcement and artillery
   sockets; support several nearby platoons and persistent dormant/active/cleared encounter state.
   The campaign should be extremely difficult but readable, with meaningful lulls for scavenging.
5. **Co-op and release hardening.** Stress building damage/loot synchronization and late joins at
   world scale; exercise 2/3/5-seat LAN and split screen; then clear distribution gates, rebuild,
   sign, notarize, staple and verify the DMG on a clean non-developer Mac.

## Useful verification commands

```sh
cargo test --locked --no-fail-fast
cargo run --release -- atmosphere-grid /tmp/kessel-atmosphere
cargo run --release -- campaign-verify /tmp/kessel-proof
tools/package_macos.sh --dry-run --offline --release-scope owner-dev
```
