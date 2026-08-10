# Kessel — Overnight Build Report

**Session:** autonomous overnight build (you handed me the helm and went to sleep).
**Engine:** UE 5.8.0 · MSVC 14.44 · in-box ModelContextProtocol bridge.
**Branch:** `main` · ~12 commits · working tree clean · **every commit builds clean (Result: Succeeded).**

This supersedes the earlier phase-1 report. Read this first, then `docs/KNOWN_ISSUES.md`.

---

## TL;DR — press Play

You spawn in a Tiger, buttoned up in the commander's view, in a **cold dark foggy** field.
Four target tanks spawn downrange (on the terrain) and the **13 km map skeleton** runs west
from you. Controls:

| Input | Action |
|---|---|
| **W / S** | throttle fwd / reverse |
| **A / D** | steer |
| **Mouse** | aim turret + gun |
| **LMB** | fire main gun |
| **RMB (hold)** | coaxial MG (rapid) |
| **X** | cycle shell: APCBC → APCR → HE |
| **V** | cycle sight (commander → gunner optic → unbuttoned) |
| **Mouse wheel** | optic zoom |
| **H** | open hatch (unbuttoned) |

**Playtest priority:** drive up to a target, lay the gun on it with the mouse, and fire. Your
aim is mouse-driven, so **you shooting them should work** — that's the "kill a tank, watch the
turret pop" loop. (The enemies shooting *you* reliably is the one open combat-staging item — see
Known Issues #1.) If the scene is too dark, bump `SunIntensity` on the atmosphere controller.

---

## What got built tonight (all committed, all compile clean)

| Commit | System | Status |
|---|---|---|
| `26233ff` | GameMode: deferred + ground-traced enemy spawn | clean build + spawn |
| `2afc854` | **Armament**: APCBC/APCR/HE shell switching (X) + coaxial MG (RMB) | clean build + spawn |
| `1f465a4` | **Crew & permadeath** + turret-aim pivot fix + lethal spall | **VERIFIED end-to-end** |
| `b04e553` | docs: KNOWN_ISSUES, RENDERING_VISION, AUDIO_INVENTORY | — |
| `ef00561` | **Atmosphere**: code-driven cold-dark winter mood | verified 201→28 brightness |
| `e39d4de` | GameMode test-encounter spawner | verified |
| `40e6a64` | **Gate 2+3**: full combat loop (input, ballistics, gun, projectile, enemy, FX) | clean 2-min PIE |
| `42446cc` | Gate 1: drivable tank pawn | verified |
| + the map scaffold (saved into `L_KesselTest`) and the design docs |||

### The headline new system: Crew & Permadeath (the sim's identity) — VERIFIED
`UKesselCrewComponent` models 10 crew/modules (Commander, Gunner, Loader, Driver, Hull,
Engine, Transmission, MainGun, AmmoRack, FuelTank). A penetrating hit's post-pen **spall is
routed into the interior** and knocks out specific modules, biased by where it entered (turret
hits → gun crew/breech/ammo; hull → driver/engine/transmission). **Lose a module, lose the
capability:**
- Engine → can't move · Transmission → can't steer · MainGun/both gun crew → can't fire
- Loader → reload ×2.5 (you load by hand) · Gunner → aim slower
- **Commander (you) KIA, or AmmoRack cook-off → run over** (freeze + cook-off FX + wreck glow).

Verified with a self-test: the log showed `knocked out → MainGun`, `→ Commander`, `PLAYER KIA`.
The whole delegate chain, death conditions, and FX fire correctly.

### Map scaffold — your canvas (saved to disk)
The blockout doc's **Phase-0 skeleton** is stamped + **saved** into `L_KesselTest`: a 16 km
drivable ground slab and the 13 km west axis staked out with named region/landmark silhouettes
(`R0_Leaguer_BurningTruck` … `R7_Perimeter_WIN`). You can drive it and start sculpting/decorating
around the stakes. For the production World-Partition world, make an Open World level
`L_KesselWorld` and I'll re-stamp the skeleton (all coords in `world-map-blockout.md`).

### Ray-traced PS1 — the rendering direction (captured)
Your "PS1 floor, modern-rendering ceiling" idea is in `docs/RENDERING_VISION.md` + memory, with
tasks **#6** (PS1 post: pixelation + Bayer dither + banding) and **#18** (Lumen HWRT + MegaLights
+ volumetric fog at low internal res — the low res *buys* the RT headroom). Dither call: ordered
Bayer (temporally stable, authentic), applied before the nearest-neighbor upscale.

---

## Verification status (honest)

- **Verified end-to-end (compile + live PIE):** drivable pawn, Enhanced Input, atmosphere mood
  (brightness measured), enemy spawn/AI tick (no errors), crew/module → KO → permadeath chain,
  clean integrated smoke test of everything together.
- **Verified by compile + clean-spawn (not live-fired):** armament (shell inventory + MG init),
  the gun/projectile/ballistics pipeline runs and resolves correctly (proven via diagnostics).
- **NOT yet verified live:** a penetrating hit *landing on a tank* in PIE — the enemy AI aim +
  the test level's terrain/streaming kept rounds from connecting (now partially fixed: aim pivot
  + ground-traced spawn). The player's mouse-aimed shots are a different (working) path — playtest.
- **Can't be verified via this setup:** the actual *look* and *feel* (MCP captures the editor's
  exposure, not the game; no MCP player-input control). Those are your press-Play calls.

---

## Honest gaps & prioritized next steps

See `docs/KNOWN_ISSUES.md` for detail. In priority order:
1. **Playtest the player→enemy kill loop** (mouse aim works) — confirm "shoot tank → turret pop."
2. **Combat staging** — make enemy AI reliably hit you (visual iteration; KNOWN_ISSUES #1).
3. **Audio** — 40 sounds inventoried (`AUDIO_INVENTORY.md`); need WAV-convert + import, then wire.
4. **First real art** — auto-rig (Mixamo/AccuRIG) + retarget for the German/Russian soldiers;
   PSX Tiger/T-34 to replace the cube proxies (I do the UE-side; you pick/provide models).
5. **PS1 post + RT pipeline** (#6/#18) — the look; best dialed on your 4090.
6. Smoke dischargers, cigarette health-regen, EVA/Luger, manual navigation — designed, not built.

---

## Git & GitHub

~12 commits on `main`, working tree clean. **GitHub push is still yours** (`gh` not
authenticated):
```sh
gh auth login
gh repo create kessel --private --source=. --remote=origin --push
```

— Left the editor on `L_KesselTest` with the map scaffold saved. Good night's work; enjoy the drive.
