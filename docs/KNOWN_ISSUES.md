# Kessel — Known Issues

Live state of things that compile + run but need follow-up. Most are tuning/visual-iteration
items that the blind MCP build→PIE→log loop can't close efficiently — they want a human (or a
visual pass) at the editor.

---

## 1. Enemy AI rounds don't reliably connect on the player (combat staging)

**Severity:** high (it's the "shoot tanks" core), but isolated to enemy AI + the test level.

**What's verified working** (via diagnostic logging, since removed):
- Enemies *fire* on cadence (all of them, every `FireInterval`).
- Projectiles *travel*, *impact*, and run the full armor + ballistics resolve pipeline.
- `resolved=0` on landscape is correct (terrain has no `UKesselArmorComponent`).

**The bug:** enemy rounds hit a `LandscapeStreamingProxy` at ~721 m instead of the player.
Diagnosed root causes:
1. **`L_KesselTest` has a real Landscape (terrain hills), not the flat arena the combat was
   tuned for.** `GameMode::StartPlay` spawns enemies at `z=70` assuming flat ground; they
   land *inside* terrain (z≈270–470 there) and `AdjustIfPossibleButAlwaysSpawn` shoves them
   up, so they **float high** above the lower player.
2. **Aim pitch was computed from the hull origin, not the gun's mounting point** (~2.5 m up).
   A high-mounted gun then aimed *over* a lower target. **FIXED** in
   `KesselTurretAimComponent::SetWorldAimTarget` (yaw from turret pivot, pitch from gun pivot)
   — but the floating-spawn + terrain-LOS parts remain.
3. **World Partition streaming**: distant terrain cells aren't loaded when `StartPlay` runs,
   so a spawn-time ground trace at the far enemy positions would miss. Naive ground-snap
   on spawn is therefore unreliable.

**Fix direction (do during combat-staging / map work, with visual iteration):**
- Author enemy encounters *on the terrain* in the level (placed, ground-correct), or spawn
  them deferred (after the player's WP cell streams in), or via nav-mesh-valid points.
- Add an AI line-of-sight check before firing (don't shoot through hills).
- Stage fights in flat-ish engagement bowls (the design already calls for chokepoints).

**Important:** the **player's** aim is mouse-driven (`AddAimInput`), NOT the AI `SetWorldAimTarget`
path — so **you shooting enemies should work**. Playtest player→enemy first; that's the
"shoot a tank, watch the turret pop" loop, and it's independent of this AI bug.

---

## 2. Atmosphere mood is a dark first pass

`AKesselAtmosphereController` drives the cold-dark winter look (measured PIE brightness ~28/255,
down from a blown-white ~201). It may be *too* dark for comfortable first play. Tunables are
`EditAnywhere` on the controller (spawned by the GameMode): bump `SunIntensity` (0.7) and
`SkyLightIntensity` (0.35) if you can't see enough. The full volumetric/RT look (tasks #6/#18)
is the real treatment — best dialed on the 4090 with `ScreenPercentage` low + Lumen/MegaLights on.

---

## 3. MCP viewport capture shows EDITOR exposure, not the game

`CaptureViewport` (with a transform) renders an editor scene-capture that ignores the level
PostProcessVolume + uses the editor's own auto-exposure — so it can't show the real in-game
image (always blew white, or went dark from a high angle). The reliable signals are: PIE
`StartPIE` success, `Saved/Logs/Kessel.log` greps, and a PowerShell crop+mean-brightness of the
`CaptureEditorImage` PIE viewport. "Feel" is a human press-Play check. (See `tank-sim-mcp-workflow` memory.)

---

## 4. `L_KesselTest` now mixes the combat arena + the 13 km map scaffold

The Phase-0 map skeleton (ground slab + region/landmark stakes) was stamped into `L_KesselTest`
and saved there for immediacy. For production, make a dedicated **Open World / World-Partition**
level `L_KesselWorld` and re-stamp the skeleton (all coordinates are in `world-map-blockout.md`
§4.1 + the placement is re-runnable). Keep the combat arena separate from the world map.

---

## 5. Audio not wired yet

40 source sounds exist (zipped in `ts_assets`) but are MP3/OGG and need WAV conversion +
editor import before `UKesselAudioComponent` seams can play them. Plan in `docs/AUDIO_INVENTORY.md`.
