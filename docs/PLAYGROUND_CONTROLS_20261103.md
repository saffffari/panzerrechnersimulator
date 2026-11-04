# Native playground controls and browser handoff

- Click a tank surface to add an authored shot. Earlier shots remain available in SHOT.
- Hold left and drag, or drag the single ring handle on the small wireframe sphere, to
  change the incoming direction around the impact point. No separate azimuth/elevation handles.
- F / Enter / FIRE fires into the current damaged tank. Space pauses/resumes.
  Backspace / RESET restores a fresh tank, paused. Simulation output is not cached.
- SHOT, SHELL, saved camera, saved preset, target, section direction, display look and
  dither pattern use direct popup lists. Scroll longer lists; arrows/Enter/Escape also work.
- SAVE PRESET opens a menu to create a named preset or overwrite an existing one.
  The preset retains fine settings plus all authored shots.

Named saves update target/fea_range_presets.json and export the current preset to
 target/playground_preset.json (relative to the stable checkout). The latter is the
input for the browser playground. settings.shots contains {name,shot} records;
shot fields retain the native numeric catalogue index, range, bearing/elevation,
seed and target. settings.shot remains the currently selected shot for compatibility.

Catalogue entries 0-15 are unchanged. Entry16 is an experimental 800mm Gustav
railway-gun AP surrogate (7100kg,720m/s); entry17 is a 28.5t T34 payload at343m/s
(approximately Mach1 at20C). Both have deliberately approximate penetration anchors,
zero explosive filler and no calibrated drag. These are playground objects, not
historical game ammunition. The T34 is visibly drawn using the game's T34 meshes;
its collision/penetration/deformation is a coarse projectile surrogate, not a
complete vehicle-on-vehicle solid-body solve.

Native headless controls probe checks direct round selection, independent authored
shots, sphere aiming without moving the impact, named preset save/overwrite/export.
The payload probe fires both experimental objects and checks that live hits resolve.
