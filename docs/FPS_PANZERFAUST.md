# On-foot FPS / Panzerfaust 60

This branch treats the dismounted layer as a small survival minigame, not a second full shooter:

- `1` selects the P08 and `2` selects the Panzerfaust. With empty hands `Q` cycles; while carrying a
  physical item, Q preserves the existing throw action. Controller D-pad left/right selects the
  weapons. RMB aims down the selected weapon; LMB fires; R changes the P08 magazine or draws a
  complete fresh Panzerfaust tube.
- The P08 starts 8+8 and kills an infantryman with one clean hit. A corpse yields exactly one or two
  loose rounds once, so a miss is a permanent loss until another body/cache is reached.
- One crew carrier may claim at most one ready Panzerfaust plus one complete spare. The Tiger begins
  with six complete tubes in its visible turret rack/mount. Unfired tubes return to the rack on
  remount; fired tubes are spent.
- Twenty-four Soviet riflemen persist in both play modes. Twelve re-anchor around moving Soviet
  tanks and twelve guard patrols/doorways/site interiors and exteriors. Rifles do not damage the
  mounted Tiger; the same men become dangerous when the player dismounts. P08 and MG34 hits leave
  persistent prone bodies.

## Historical tuning

The implemented weapon is a **Panzerfaust 60**, not an RPG-7:

- 45 m/s launch speed, practical 60 m range, last sight setting 80 m.
- Full-size PzF 60 geometry: 1.045 m assembled, 800 x 50 mm launcher tube, and a 495 mm long,
  150 mm diameter bomb.
- Three modeled sight apertures: 30/60/80 m. The current ADS alignment uses the central 60 m zero,
  elevating the tube about 8.45 degrees while the line of sight stays on target.
- The projectile is a recoilless-launched, fin-stabilized coasting bomb. There is no sustained rocket
  motor or smoke trail.
- Documented penetration is about 200 mm at 30 degrees from normal. A valid armed contact therefore
  defeats a T-34 frontally in this first gameplay pass. The T-34's 45 mm glacis at 60 degrees has
  roughly 90 mm line-of-sight thickness, although very oblique/edge/poor-fuze contacts are a future
  failure mode.
- The rear 3 m backblast cone is lethal to infantry and the cone suppresses out to 10 m.
- A fired tube is never field-reloaded. R discards/lowers the spent tube, draws the complete spare,
  unfolds its sight and arms it over 2.7 seconds.
- The game uses a conservative 5 m minimum arming distance. Primary manuals describe the fuze and
  safety but do not give a numeric arming distance, so this parameter should be treated as a tuning
  assumption rather than settled historical fact.

Primary/technical references:

- German D 560/3 Panzerfaust 60 wartime manual: <https://djvu.online/file/xVyEqdA2dCtZi>
- British Ministry of Supply, ADD Technical Report 2/46 Part N:
  <https://www.bulletpicker.com/pdf/ARD-Report-German-Grenades.pdf>
- Slovak Military Historical Institute, Panzerfaust 60 collection record:
  <https://www.vhu.sk/nemecka-rucna-protitankova-zbran-panzerfaust-60m/>
- British School of Tank Technology preliminary T-34 report:
  <https://electronicsandbooks.com/edt/manual/Military/%21%20NEW/Preliminary_Report_No_2-0_Russian_T-34_Medium_Tank.pdf>

## Art and assets

Hal and the local asset tree contained no RPG or Panzerfaust model. The branch therefore ships an
original sub-800-vertex faceted Panzerfaust body, separate folding aperture sight/front bead and
separate bomb in `src/panzerfaust.rs`. No marketplace model or third-party geometry is incorporated.

The best exact external reference found was “Panzerfaust 60” by Superplunger (2.9k triangles, CC BY
4.0), but Sketchfab requires an authenticated account to download it:
<https://sketchfab.com/3d-models/panzerfaust-60-7af8a3f967f647fab1877ceb8f714d41>.

The P08 reload uses the retained Pizza PSX pack's authored loaded magazine, loose pistol round, and
spent casing GLBs. They receive separate runtime transforms so the magazine drop/fresh-mag rise,
visible cartridge crown, seating beat and ejected brass all remain legible in first person.

## Multiplayer boundary

`panzerfaust_claimed` is the single-carrier authority seam and the rack arithmetic is authoritative
inside `Game`. The current engine still has one global dismounted avatar, so independent simultaneous
crew avatars firing Lugers from several open hatches require the later per-crew foot-state/network
replication pass. Infantry persistence and the turret mount are already shared with tank mode.


## September 5 first-person and collision correction

The physical 1.045 m original prop now sits under the arm at 1:1 scale. Its former 0.62 scale and
forward placement made the warhead read about 3.3 times smaller in hip fire. The sight leaf stays
upright, the 60 m aperture aligns with the camera, and rendering, launch clearance and flash share
one held transform. Original asset geometry is unchanged.

The moving bomb now competes against both factions of infantry, occupied Tiger crew anatomy,
live structures/trees/terrain, and vehicle armor. A launch clearance sweep prevents a long held
weapon from spawning the bomb beyond a person immediately ahead. The retained 5 m fuze assumption
controls explosion only; unarmed bomb contact still delivers kinetic injury. Tiger contacts query
the actual source triangles and current roof/cupola lid transforms, allowing a shot in empty cabin
space to reach a crewman rather than instantly striking the enclosing vehicle bounds. Enemy/ally
armor still uses the game's source-derived articulated OBBs. Unsupported roofs/bellies stop the
bomb but do not silently inherit side-plate penetration properties.

A distinct HEAT terminal profile replaces the unconditional 3.6 m sphere kill. Penetration is
calibrated to the previously recorded 200 mm / 30 degree anchor; flight speed does not act as a
kinetic penetration multiplier. The 2.9 kg bomb, 1.6 kg filler and 1600 m/s residual-jet ceiling are
explicit coarse simulation inputs, not a newly validated historical measurement set. The existing
shared vehicle consequence path owns destruction after penetration. Its current enemy model still
treats a penetration as a vehicle kill, while the Tiger resolves crew/components/secondary ammo
contacts. HEAT filler is exterior: it cannot become an invented full-charge cabin explosion.
Armed exterior blasts reuse the existing faction-neutral infantry blast/cover law. Backblast now
respects intervening geometry for exterior infantry.

This remains a gameplay approximation: no hydrodynamic jet, spaced armor, material-per-triangle
armor table, or complete enclosed backblast-pressure model is claimed. The standalone Panzerfaust
still retains the existing 80 m flight envelope. Added CPU regression cases cover sight placement,
apparent width, empty-cabin versus surface collision, near unarmed crew contact, nearest-cover
ordering, HEAT fuze/obliquity/velocity behavior, and external filler separation. Real first-person
video and the integrated Cargo gate remain separate validation steps.
