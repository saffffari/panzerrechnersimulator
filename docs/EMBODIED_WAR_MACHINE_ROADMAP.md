# Panzerrechnersystem — Embodied War Machine Roadmap

> **Current platform/campaign authority:** Use this ledger with `docs/STATUS.md` and
> `docs/LINUX_4090_HANDOFF.md`. Linux/Vulkan on the RTX 4090 is the sole performance target, and the
> Reichstag last stand plus optional hidden cottage escape supersede older finale language.

This is the implementation ledger for the August 2026 design expansion. It keeps the work ordered
by dependency so a mechanic is simulated once and reused by audio, visuals, AI, campaign and co-op.

Status vocabulary: **live**, **partial**, **next**, **planned**, **deferred**, **research gate**.

## 1. Combat event spine

- **Live:** one resolved ballistic strike drives armor outcome, crew/module damage, directional
  acoustics, concussion, spall, visible scars and replay.
- **Live:** localized dents/open holes weaken only the struck region; 24 compact scars persist in
  campaign and LAN. Toolbox repair consumes one spare part and restores partial protection.
- **Live:** repair visuals use three procedural spare track shoes across the breach. Welding is a
  workshop/recovery capability, not an onboard magic tool.
- **Live:** directional camera impulse follows the incoming round's world-space travel vector, so the
  eye is thrown away from its source and scales with caliber, penetration and station distance.
  Side-dependent roll remains a polish item after the translational/yaw/pitch response is playtested.
- **Planned:** flat capped silver chips for rifle/MG strikes. They are visual-only unless an optic,
  exposed mechanism or other vulnerable component is actually hit.
- **Planned:** destructible gun tube, mantlet mechanisms, traverse, sights/periscopes, ventilation
  fan, radio, engine accessories and ammo-bin lids. Damage must alter function, not just HP.
- **Planned:** anti-tank mines primarily throw tracks/damage suspension; exceptional large/stacked
  charges can breach the belly. Historical mine types and fuzes require a source gate. No red LEDs:
  discoverability should come from disturbed soil, marker stakes/wire, exposed edges or prior blast
  traces unless a deliberately non-historical accessibility assist is enabled.

## 2. Main-gun embodiment and reload

- **Foundation only:** `src/reload.rs` now provides a bounded renderer-independent identity/location
  model for rounds and cases, transactional rack/hand/loose/sack moves, breech interlocks,
  semi-automatic ejection and deterministic cached misfire/hangfire plans. The live game still uses
  its older fragmented reload fields; integration, persistence, animation and researched tuning are
  the next work.
- **Next vertical slice:** physically violent 8.8 cm firing: stronger impulse/shake, correct recoil
  direction, visible breech travel, short pressure/smoke discharge when opened, heat accumulation,
  hot-breech material response, and failed-fan hot-box penalties.
- **Next vertical slice:** realism/fun reload sequence:
  1. select correctly colored AP/HE from organized racks;
  2. bring the nose into the breech alignment volume;
  3. orange outline while alignable, short green confirmation when seated;
  4. deliberate forward ram gesture;
  5. compact button sequence for breech/ready operations instead of lever busywork.
- **Planned:** spent case can be ghost-positioned, returned to a free rack space, placed on the floor,
  or pushed through any sufficiently open hatch with the same forward gesture. Act I can award a
  five-round shell trolley found in an abandoned German encampment.
- **Planned:** deterministic, condition-based misfire/hangfire model. Opening/extracting a suspected
  hangfire carries a delayed ignition risk based on ammunition condition and elapsed wait—not a
  casino roll detached from physical state.
- **Research gate:** verify current breech cover/elevation parenting, actual Tiger firing controls,
  historic sight reticles and smoke-launcher fit by production period.

## 3. Crew embodiment, injury and aid

- **Planned anatomy core:** limbs, hands, major vessels and organs; penetration path and deposited
  energy produce bleeding, pneumothorax, pain, shock, consciousness, mobility and task capability.
  Survival uses evidence-bounded ranges rather than false second-perfect fatal timers.
- **Planned:** self-aid only while conscious and mechanically capable. Tourniquets help appropriate
  limb hemorrhage but cannot treat torso bleeding; dressings, chest seals and medkits have distinct
  roles. Aid to a crew mate requires access to the body and working hands.
- **Planned:** a dead/incapacitated crew body physically occupies its seat. The player or another
  crew member must release/drag it clear before taking the position; bodies and helpers have collision.
- **Planned:** blood and debris inside the tank are event-driven, directional and persistent within
  a bounded pool—not uniform gore decals.
- **Planned:** silent player-perspective catastrophic death: immediate black on unsurvivable cookoff
  or massive trauma. Exterior observers still receive the world event.
- **Deferred by design:** surviving enemy tankers bailing out, fleeing, carrying wounded, abandoning
  casualties under pressure and potentially finding another tank. When built, surrendering or
  incapacitated people remain noncombatants; escape/rearm state is explicit.

## 4. Enemy doctrine and training

- **Foundation only:** a renderer-independent deterministic doctrine module defines six- and ten-tank
  roles, `Form → Fix → Mask → Assault → Exploit` phases and causal sacrifice credit. It is not yet
  wired into campaign enemies, movement, smoke or the learned controller.
- **Next:** integrate and train the 6-v-1 curriculum: contact fixing, sacrifice/pressure, crossfire,
  flank rotation, disabled-tank cover, bounded aggression and coordinated withdrawal.
- **Blocked on scaling:** 10-v-1 uses the same doctrine vocabulary, but the live game still has a
  hard six-enemy scene/simulation pool. Do not claim a ten-tank battlefield drill until that limit,
  separation, collision and performance are addressed.
- **Next:** smoke-screen action with finite launchers/ammunition, line-of-sight consequences and
  doctrine-aware timing rather than a visual cooldown buff.
- **Next:** anti-infantry drill: detection of dismounted player, coax/hull MG at exposed targets, HE
  against occupied cover, wall material/penetration awareness and avoidance of wasting AP.
- **Deferred:** bailout/recrew loop described above.

## 5. Repair, fuel and salvage

- **Planned:** engine deck panels are removable physical assemblies. Full engine repair minigames
  remain on backburner; the first slice gates engine-kit use and exposes replaceable components.
- **Planned:** radio replacement from scavenged components; sight glass replacements in repair kits;
  turret electrical failure falls back to slow manual traverse; severe damage can jam the ring.
- **Live/partial:** fuel quantity persists through the campaign, driving consumes it and an empty tank
  shuts the engine down. **Planned:** a physical driver gauge and cans, richer RPM/load/terrain
  consumption, abandoned-Tiger/fuel-depot salvage and a historically checked siphon rule for
  incompatible fuels.
- **Planned:** wreck-reading loop: blackened/rusted Tigers, missing or displaced turrets, thrown tracks,
  bodies and removed components tell the player what may still be salvageable.
- **Research gate:** German field maintenance had truck-carried gas and arc-welding equipment plus
  generators/transformers. A portable welding scene belongs at a workshop/recovery site; reliability,
  armor-metallurgy limits and repair scope require explicit source notes.

## 6. Stations, controls and instruments

- **Next:** one seat-cycle key with physical transition timing and collision. Verify which Tiger
  stations were internally traversable; never teleport through bulkheads.
- **Planned:** driving may be delegated from the cupola at reduced effectiveness, while full speed,
  ignition and detailed handling require the driver. Radio transmission requires the radio operator.
- **Planned:** driver-only physical ignition control; illuminated gear/RPM/speed/fuel dials with glass;
  glass-covered TPZ and instruments; working rear/right turret escape hatch; red abandon-tank lever.
- **Planned:** user and AI headlights are physical, damageable, independently toggled sources. They
  improve navigation while spending battery/fuel and dramatically increasing visual detection.
- **Planned:** map board, removable physical map, compass with glass/red-grey needle, ruler/marks and
  wristwatch. Items must be carried to be available.
- **Live:** a procedural PBR turret-floor disc with asymmetric cleat/inspection hardware is parented
  to the live turret transform, making traverse readable from inside while its centre stays on the
  ring axis.

## 7. Sensory simulation

- **Live/partial:** engine and ambience loops, directional/delayed reports, hatch/interior acoustic
  states and procedural impact rack.
- **Next:** both friendly and enemy engine/track signatures derive from RPM, load, speed and terrain;
  AI hearing and player audio share the same propagation event. Unbuttoning increases useful hearing
  while exposure and concussion risk rise.
- **Planned:** material impacts (dirt/stone/steel/wood/masonry), destruction, cookoff, climb/jump,
  ignition/idle/load, traverse, every reload step, hatch mechanics, Luger shot/impact and enclosure
  transition. Later export stems/parameters into an Ableton-authored production rack.
- **Planned:** original/licensed German crew barks and radio procedure are parameterized by role,
  contact confidence, injury, intercom state, enclosure and stress. Archival chatter remains research
  reference unless its rights and historical framing are explicit.
- **Planned:** restrained gun-sight head motion follows hull angular acceleration and concussion;
  optics receive researched reticles, edge refraction and breakable/replaceable glass rather than a
  generic camera shake overlay.
- **Planned:** flashlight with finite batteries/spares; portable lantern; more bounded interior point
  lights; subtle dust; exhaust from Tiger/T-34; stronger but art-directed shafts of light.
- **Planned:** low-resolution moving volumetric clouds, real extinction/scattering volumes, improved
  explosions/fire/smoke. Fire spread follows fuel/material/ventilation and accelerates through connected
  flammables. Subtle seeded weather variation preserves each act's identity.
- **Longer-term renderer:** GI approximation, transmission/transparency and richer PBR response.
  Subsurface scattering is limited to materials that physically need it; it is not a universal toggle.

## 8. World, story and communication

- **Planned:** authored overrun/failed-assault tableaux use historically researched local casualty
  densities and tactical placement. Ordinary terrain gets sparse, story-driven remains rather than a
  uniform bodies-per-square-metre field.
- **Planned:** fuel depots, German encampments, repair/recovery sites and somber wreck fields become
  scavenging landmarks with real shadows and destruction state.
- **Planned:** Enigma-style physical message station is the campaign tutorial/command channel. Use a
  Pizza prop if a suitable licensed asset exists; otherwise build a procedural placeholder. Historical
  operation and plaintext/cipher workflow need checking—the machine was not simply a radio typewriter.
- **Planned:** lived-in Tiger dressing: photos, food, cigarettes/matches, ammunition, clothes, tools,
  bottles/cans and period personal effects. Consumables have tradeoffs, never unexplained health buffs.
- **Planned:** opening broadcast uses original/licensed material with explicit anti-Nazi historical
  framing and subtitles; no unlicensed archival speech is silently bundled.

## 9. Co-op and acceptance

- Every mutable tank/crew/item system is host authoritative and late-join safe before it is called
  co-op complete. Seat ownership, body removal, aid, inventory, reload state and damage cannot diverge.
- Required gates: solo and 2/3/5-player crew on Linux; Linux↔Linux LAN;
  disconnect/rejoin during reload/repair/injury; split-screen performance; controller parity;
  deterministic replay; source-asset/license closure. Mac interoperability is optional diagnostic
  evidence and cannot block the authoritative Linux build.
