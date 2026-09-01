# chatPANZER next phase — playtest direction (2026-07-17)

> Historical checkpoint only. Any lighting language below is superseded by `CLAUDE.md`'s physical
> truth rule: apertures and optics are geometry, not emitters; shafts emerge from scattering along
> visible light paths and are never separately authored.

Source: the annotated screenshot playtest set in
`Pictures/Screenshots/next phase deliverables for a massive development effort`, plus Camillo's
first-session reaction. The north star remains **environmental storytelling through an embodied
machine**. This file separates what this checkpoint fixed from the next coherent pushes.

## Evidence worth trusting

- A non-tank-player found the interior/exterior visibility problem genuinely fun and tense.
- His first two asks were co-op and PvP. Co-op is already the stronger fit: communication, divided
  attention, and physically seeing occupied posts all amplify the thing that is working.
- The strongest screenshots are not wide battlefield vistas. They are the moments where a bright
  slit, a dark compartment, a shell, a lever, smoke, and an uncertain outside contact share a frame.

## Fixed in the controller/interior checkpoint

- Xbox and PS4 hot-plug controls, menus, on-foot play, loader gestures, map pencil, and LAN inputs.
- Cupola exit now actually changes authoritative mode to on-foot.
- Physical live-round unloading; AP/HE comes from the chosen rack object, not a hotkey inventory swap.
- Authored tray-to-chamber ram path, crosshair alignment gate, view lock, higher closed breech cover.
- One bore-smoke vent per fired case; repeat lever pulls create no fake smoke.
- Smaller matte-white foam; no orange emissive landing cubes.
- Procedural building collision and tank-on-tank collision.
- 2.5 cm interior near plane, half-width interaction outlines, irregular deferred glass stipple.
- Matte charred wreck steel and a faceted expanding blast instead of the initial explosion cube.
- The old aperture/periscope scattering shortcut was later rejected; real apertures, emitters,
  occlusion, extinction, and scattering now define the target.

## Next production stack

### P0 — navigation becomes an object, not an overlay

- Render map, compass, and pencil as held 3D meshes with depth, hand movement, shadows, and world
  still visible around them.
- Put authored silhouettes, roads, rail lines, frozen river edges, settlements, objective bearings,
  and discovered battlefield evidence on the paper. No GPS dot. Position is triangulated from the
  environment and pencil bearings.
- Make the major landmarks in code stop being placeholder towers.

### P0 — battle damage tells the story afterward

- Persistent entry holes, ricochet furrows, spall exits, soot cones, scorched paint, shed track links,
  and thrown turrets. Decals must survive for the act so the route becomes an after-action record.
- Wreck library: Pz III/Pz IV/T-34/SU-100 plus overturned, side-resting, buried, abandoned, stripped,
  and burned variants. Assemble old battle sites as readable events, not random prop scatters.
- Replace every enemy placeholder rig, especially the SU-100, before adding more vehicle classes.

### P0 — weather is an adversary

- Winter: gust-driven blizzard cells, pauses, whiteouts, track filling, windward snow accumulation,
  muffled reports, iced optics, and visibility openings that briefly expose both sides.
- Spring: rain sheets, mud adhesion, fog banks, dripping interiors, wet reflection breakup.
- Weather changes encounter geometry and information; it is not a color-grade preset.

### P1 — optics and interior material finish

- Rebuild the TZF reticle from the actual stadia/triangle layout and make the sight read as reflective
  optical glass, not a transparent window.
- Eye-box movement over terrain and a larger binocular vignette. Keep each physical periscope image
  projection-stable when binoculars inspect it; do not replace the pane with a second zoom tier.
- TPZ dial face/needle material pass, proper metal lever response, smaller back-plated buttons,
  historically plausible dim task lamps, physically transported glass/reflections, and scattering
  that emerges from those real sources and apertures.
- Ground-contact AO and close-structure material stability; remove television-static surface noise.

### P1 — encounters have military shape

- Seeded spawn alternatives and real platoon compositions instead of isolated random targets.
- Formation movement, overwatch, bounding, regrouping, withdrawal, casualty response, and crews
  bailing from disabled tanks.
- Close combat contact physics, track entanglement, towing/pushing, and wrecks that block streets.

## Product direction

1. **Build co-op campaign depth before competitive PvP.** The tank itself is the co-op map and the
   best source of comedy, panic, and dependence. Finish reconnect/rejoin, lobby discovery, latency
   abuse tests, station handoff, and full-campaign LAN persistence first.
2. **When PvP arrives, do not make a symmetric arena.** Start with an asymmetric hunt: one Tiger crew
   with terrible awareness versus a larger Soviet platoon with thinner armor and distributed eyes.
   Victory can be escape, delay, identification, rescue, or holding a crossing—not a kill counter.
3. **Keep long silence.** Constant explosions will flatten the fear. Intensity comes from quiet work,
   incomplete information, then a short irreversible catastrophe. Remove anything that supplies
   free certainty: permanent compass, omniscient markers, glowing wreck hulls, and magic ammo swaps.

## Original mechanics to prototype

- **Pass the weight:** in co-op, a loader can physically hand a shell, extinguisher, map, or wounded
  crewman to another visible rig. Dropping it during a hit leaves it rolling on the basket floor.
- **Sound-board triangulation:** the commander hears delayed reports, marks bearing lines on the map,
  and derives an ambusher's position from two contacts. The existing speed-of-sound system is already
  the foundation.
- **Compartment memory:** every fired round, casing, pencil mark, blood/soot smear, broken lamp, and
  repair remains. The interior becomes the campaign's save file without a stats screen.
- **False contact:** ice cracking, distant artillery, echoing ruins, friendly engines, and ricochet
  reports occasionally resemble a tank. A crew must agree what it heard before turning the machine.
- **Crew voice as bandwidth:** optional proximity/interior voice is filtered by station, engine RPM,
  hatch state, concussion, and radio condition. A knocked intercom forces shouting or moving bodies.
- **No-respawn PvP rounds with aftermath:** dead players can remain as dismounted survivors trying to
  escape, scavenge, guide another crew, or deny intelligence. The battle does not end when their tank
  stops moving.
