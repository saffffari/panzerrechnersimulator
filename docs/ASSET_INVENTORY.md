# Kessel — Asset Inventory (pizzadoggy bundle)

**Source:** Humble "Game Dev Like It's 2005" / pizzadoggy PSX bundle.
**Location on disk:** `pizza_assets/` (git-ignored). Zips + `_extracted/` + `_previews/`.
**Status:** all 13 archives extracted Jun 30. Nothing imported into `Content/` yet — this is
the greenfield visual foundation. Outside finds get retro-fied to match via **PolyRazor**.

Model format: every mesh ships as **GLB + FBX + OBJ** (pick one — GLB is the vendor-recommended
and imports cleanly into UE 5.8). Counts below are unique meshes (not ×3 for the formats).

## Models — ~1,243 unique meshes

| Pack | Meshes | What it is | Kessel role |
|---|---|---|---|
| **PSX Mega Pack** | 489 | Furniture 123 · Items&Weapons 116 · Modular Structures 96 · Small Props 44 · Electronics 38 · Lighting 36 · Large Props 32 · Decals 4 | bulk interiors + lootable clutter (leaguer, buildings) |
| **PSX Mega Pack II** | 549 | Modular Structures 210 · Props 137 · Large Props & Machinery 52 · Debris 34 · Weapons&Tools 31 · Modular Props 27 · Buildings 14 · Doors&Gates 6 · Light Sources 10 · Decals 12 | town/industrial blockout + machinery + debris |
| **PSX Nature** | 98 | trees, pines, stones, grass, fern, wheat, logs, stumps, mushrooms | the 13 km winter map flora/terrain dressing |
| **PSX Tech** | 63 | gauges, levers, fuse boxes, capacitors, control panels, buttons, screens, signal receivers | **cockpit instruments + EVA gadgets** |
| **Modular Retro FPS Kit** | 44 | walls, floors, columns, doorways, stairs, tunnels, vents, crates, barrels | snap-together bunker/corridor shell |

## Textures — ~2,400 images (the surface language)

| Pack | Images | Role |
|---|---|---|
| **PSX Textures II** | 932 | primary tiling/surface library |
| **PSX Textures** | 439 | surface library |
| **Experimental Textures** | 18 | bonus mini-pack |
| (per-model textures live inside each model pack too) | ~1,000 | baked albedo for the meshes |

## Audio — ~914 sounds (foundation for AUDIO_INVENTORY.md)

| Pack | Sounds | Role |
|---|---|---|
| **Echoes Audio Super Kit** | 471 | general SFX + ambience super-kit |
| **Rust & Blood SFX** | 255 | combat/gore/impact |
| **ROT Horror Audio** | 188 | dread/atmosphere tones |

## Tools

- **PolyRazor v1.4.0** (`PolyRazor.exe`, Windows) — mesh decimation / PSX-ifier. The lever for
  pulling *outside* assets (modern FBX, scans) into the bundle's low-poly look so everything coheres.

---

# Secondary tier — `ts_assets` (C:\shop\ts_assets)

Specific hero/flavor pieces the bundle lacks. Bolted on *around* the spine, not replacing it.
**~169 unique meshes** across many small downloads (Sketchfab/GrabCAD/itch). All extracted to
`ts_assets/_extracted/` Jun 30. A capitalized category scaffold (`Characters/ Vehicles/ Items/
Props/ Weapons/ Infrastructure/ Roads/`) was started by the previous session but left nearly empty.

### IMPORT CUTOFF — nothing is in Unreal yet
`Content/` has **zero** imported meshes from either library (only `L_KesselTest` + code proxies).
Everything below is on-disk raw. No half-imported state.

### Armor roster (ts_assets is the ONLY source of tanks)
| Asset | File | Plan |
|---|---|---|
| **Tiger H1 (exterior, game-res)** | `tank-pzkpfwvi-ausfh1-tiger/source/tiger-h1.fbx` (4.4 MB) | **PLAYER / HERO — placeholder now** |
| Tiger hi-res raw | `uploads_files_2486207_tigerTank-v001.fbx` (131 MB) | candidate exterior, manual PS1 pass |
| Tiger SolidWorks CAD (full interior+engine) | `_Reference_CAD_Tigers/…STEP` | **user builds the custom in-game Tiger from this later** |
| **T-34/76 F-34 1942** | `tank-t-3476-f-34-1942/source/T-34 1941.fbx` | **classic enemy target dummy — import one now** |
| T-34 variants (L-11 '40, +add-armor, +side-skirts, D) | `tank-t-34*/source/*.fbx` | variant library, catalog only |
| KV-1 (ZIS-5 1941) · Pz III Ausf.A 1937 | `tank-kv-1*`, `tank-pz-kpfw-iii*` | variant library, catalog only |
| SU-100 (RU Winter, Unreal-embedded) | `uploads_files_5010853_RU_Winter_SU100_…fbx` (748 MB) | hi-res, **manual PS1-ify by user** |

### Other secondary content
- **Brutalist/Soviet buildings** — brutalist ×3, panelka, slum apartment, Soviet housing,
  abandoned house/building, lowpoly buildings, water tower, windmill ("brutalist beauties").
- **Infrastructure** — R46 subway station, Soviet metro props, ER-9 electric train + interior
  train, `Train and Railway/trainpack.fbx`, sewers, roads (`psx-road`), **`Roads.rar` (NOT
  extracted — no rar tool on box)**, bus station, jersey/street barriers, urban street props.
- **Weapons & kit** — Luger/P08 ×2, AK-47, Colt 1911, Desert Eagle, rocket launcher, Heavy
  Weapons Pack, ammo boxes, rigged FPS hands, WW2 German greatcoat, PSX skeleton, deadman.
- **Set dressing / mechanic props** — trash & debris, damaged/PSX containers, jerry can, fire
  extinguisher, garage/industrial props, morgue cabinet, vans/Corolla/NPC truck, stove, lamps,
  washing machine, bunk bed, bottles pack.
- **Cigarette mechanic** — `retro-marlboro-reds/…psx_marlboro_reds.glb` + `psx-style-smoking-pack`.

### ts_assets gotchas
- Many downloads are duplicates (` (1)`, ` (2)` suffixes) and demo/sample versions of bundle packs
  already owned in full under `pizza_assets` (PSX Mega Pack [Demo], Tech [Sample], etc.) — **skip
  those demos; use the full versions.**
- `Roads.rar` needs a rar extractor (not installed). `_Reference_CAD_Tigers` is CAD reference, not
  game-ready mesh.

---

# Decisions (locked this session)

- **Hierarchy:** `pizza_assets` = primary spine · `ts_assets` = secondary flavor · hi-res Tiger/SU-100
  = manual hero conversions.
- **Tiger 1 = player/hero vehicle.** Low-res `tiger-h1.fbx` is the placeholder; full custom interior
  Tiger comes later from the GrabCAD SolidWorks CAD (user-built).
- **One classic T-34** imported now as enemy target for playtesting the Tiger's gun + armor.
- **No browse-grid for browsing.** The UE **Content Drawer is the library** — import once into
  `/Content/Kessel/Library/<Pack>/`, auto-thumbnails + search + drag-drop = the "compile file."
- **Blender** is reserved for (a) the manual hi-res **Tiger + SU-100** PS1 conversions (user-done),
  and optionally (b) triaging/deduping the messy `ts_assets` pile before UE import. Not for the
  spine packs — they're already game-ready GLB/FBX; a Blender round-trip would only degrade them.
- **Optional one-time scale-reference sublevel** (not a library): hero Tiger + a human-height ref +
  one building, just to lock world scale. A ruler, not a browser.
- **First import pipeline test:** PSX Tech (63 meshes) into the Content Drawer before fanning out.

## Notes

- Vendor preview/screenshot lighting is **dark interior / night** — that is NOT Kessel's target
  mood. Geometry + texture language is what we adopt; lighting is driven by our day-night system.
- `_previews/` holds extracted preview sheets per pack for quick browsing without importing.
