# On-foot FPS / Panzerfaust 60

This branch treats the dismounted layer as a small survival minigame, not a second full shooter:

- `1` selects the P08, `2` selects the Panzerfaust, and `Q` cycles. Controller D-pad left/right
  selects them. RMB aims down the selected weapon; LMB fires; R changes the P08 magazine or draws a
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
