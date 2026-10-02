# Active playtest fixes


## Playtest fixes and grouped infantry checkpoint, 2026-10-01

Status: source changes in both review trees, integrated CPU checks underway. The user closed the
800x450 game before capture work. No interactive relaunch has occurred. No new GPU,
sound-listening, gameplay, or performance acceptance is claimed at this checkpoint. Its original executable is preserved in the OLD tree at
artifacts/runtime/panzer_interactive_800_before_playtest_fixes_20261001, SHA256
80556eb9144d2f758f8d8de6c3760f13d8397a4569e785a08cdc8f03bff1b41e.

- Window-size choices now include 3840x2160 and the Apple Studio Display's 5120x2880.
  The internal render-size setting remains independent; the chosen 800x450 is preserved.
  The user confirmed menu buttons already work. Small Controls/LAN pointer gaps were found
  during inspection and corrected; the menus were not redesigned.
- Repairs now have small native-pixel contact cues and actual-component selection outlines.
  A ring jam has a nonemissive lodged steel fragment at the exact turret-local repair point.
  The displayed work progress follows the contacted repair job. The old invisible jam and
  missing track outline explain why a player could swing the hammer without finding contact.
  Existing generic repair anchors retain their 28 cm end-surface tolerance; registering every
  approximate anchor to an exposed source face is a separate remaining geometry task.
- Independent track traction and source-derived replacement shoes are in both trees. One
  broken track permits limited movement/pivot about that side; it does not disable the engine.
  The original baked track skin is still rigid, so this is not a fully deformable track rig.
- Stowed rack cartridges are attached to the hull in their correct reference frame. A long
  support fitting formerly misidentified as ammunition is loaded separately, leaving 64 real
  cartridges. Source-containment tests found zero exposed rack vertices at five turret angles.
- Actual player gearbox/engine dynamics replace the speed-only engine-audio approximation.
  Automatic is the default; Driver PgUp/PgDn selects manually and End returns to automatic.
  The same gear/RPM/load drives motion and engine audio. Torque-curve details, clutch timing
  and adhesion remain explicit tuning estimates; this is not a validated Tiger drivetrain model.
- Continuous audio controls use sample-rate gain/rate ramps and a repaired traverse-loop seam.
  ALSA underruns observed in the live session are a distinct remaining host/runtime issue.
- Moving vehicles now fit pitch/roll to terrain supports. A separate, non-destructive map review
  revision seats 77 wrecks and 260 walls on source-derived supports:
  assets/worldgen/workshops/gaea-canyon-grounded-review-20261001/grounding.receipt.json
  in the MAIN tree. It is not the running/canonical map. Rigid wrecks can bridge uneven ground;
  the worst remaining support gap in this revision is 0.593 m and needs visual review.
- Canyon allocation rises from 24 to 60 Soviet actors, with up to six geometrically sited AT
  guns, their crews, tank escorts, and the remaining riflemen in loose groups of at most six.
  Added opening-hamlet/saddle post candidates address the previous kilometre-long gap.
  A whole group must fit nearby clear terrain or the site is rejected. Corpses are retained.
  This is encounter tuning, not a claim of exact historical squad establishment.
- Group cohesion uses the living group's medoid to bring separated members back. Nearby
  visible squadmates can point out a fresh contact as a look direction; no enemy memory or
  firing permission is shared. Each soldier still needs ordinary personal FOV and LOS.
  Actual map-start placement and multi-minute group behavior await the held gameplay test.
- Mounted-player perception and small-arms contacts now use the player's retained articulated
  source armor and existing crew anatomy. Exposed crew can be hit through a clear aperture;
  a nearer physical armor face blocks the bullet. Other vehicles and cannon contacts retain
  their existing separate collision paths.
- Direct AP contact with AT guns uses rendered component geometry and a residual shell
  velocity/penetration budget. The remaining shell can strike crew on its actual subsequent
  path; AP is not converted into an invented radial blast. Unspecified steel is an RHA
  surrogate, and tyre-only material penetration remains unsupported/terminal.
- Coax spread is bore-relative rather than world-axis-dependent, with 1.2 mrad per-axis sigma
  as a tuning baseline (roughly 0.59 m 95-percent circle diameter at 100 m). This is not a
  verified historical MG34 grouping specification. Gun/sight offsets and projectile drop remain.

Standalone CPU checks already passed: grouped infantry 4, coax spread 2, static seating 3,
powertrain 6. Integrated test results are recorded separately in OLD artifacts/runtime; a
successful compile alone is not visual or audible acceptance.

## Final review-build additions

- The reload lever is back in its authored loader-side position, opposite the gunner sight.
  Both sides of the breech carry the same physical state lamp (red/amber/green). Each active
  lamp enters the ordinary finite-source shadow path; neither is a camera fill.
- Open-hatch falls are reconciled through actual clear source geometry, including the
  below-cupola-rim case. Space performs an ordinary jump outside unless the player aims at
  a reachable open hatch. See OPEN_HATCH_JUMP_REPAIR_20261001.md for the geometry checks.
- Light prop contacts use explicit estimated prop mass and momentum transfer instead of a
  fixed percentage speed loss. Camera response follows the actual speed loss. Trees and
  breakable structures use that same response; their existing resistance remains. Props
  still push/topple according to their existing material motion, not a new fracture solver.
- The review recorder can encode the native800x450 pixels directly using native_output=true.
  Normal captures retain their prior1920x1080 default. Repair cues use the live HUD path.
- Source damage causality, loose-track drive audio, smoke continuity and native gameplay
  captures are being checked separately. See their receipts before declaring acceptance.
