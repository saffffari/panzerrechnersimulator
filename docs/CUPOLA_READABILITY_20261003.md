# Cupola readability correction — 3 October 2026

The prior correction preserved a known obstruction instead of making the healthy port useful.
`Turret_0116`, the fixed cap ahead of cupola port 1, reached 2.53562 m: just above the
2.53060 m objective. The ordinary commander eye was also 23 mm above the eyepiece,
so its centre ray looked down into the fitting. RMB changed the eye position and cleared
the centre, but left much of the lower field blocked.

The new player assembly makes two physical changes:

- The seated eye moves down 23 mm, from 2.415 to 2.392244 m. All seven eyepiece centre
  rays now look level. The existing held viewing posture remains unchanged. The ideal
  optical prescription retains its original calibration datum and does not change.
- The fitting pedestal `Turret_0115` is shortened by 55 mm above its unchanged foot
  at 2.41108 m. The complete 31 mm cap translates downward by the same amount, retaining
  its original joint and shape. The cap top is now 2.48062 m, still above the actual
  local roof mounting surface at 2.407–2.427 m. Roof opening, armor and all other nodes
  remain unchanged.

This is a bounded game-asset authoring correction, not historical validation of the fitting
or optical prescription. Nothing becomes invisible to the periscope, light or collision rays.
The import applies the same transform to positions, normals and tangents before material
assembly. Runtime collision/shadow geometry uses that corrected assembly; cabin air-mask
construction applies the same source transform. Source GLB files and all material texels
are unchanged.

The first 35 mm fit cleared only 20 of 25 held source-field samples: the bottom row still
struck the far side of the complete cap, 0.39–0.40 m beyond its optical exit. The 45 mm
correction accounts for that far side as well as the nearer cap edge. This is why testing
only a centre ray or the raster approximation would have missed the remaining obstruction.

Regression coverage reproduces the original blocked ordinary centre against the original cap,
checks the corrected ordinary centres across all seven ports, traces the source optical
field and reciprocal rays through the retained windows/lenses/mirrors, and checks cap shape,
pedestal foot, joint, materials and untouched roof geometry. Root-run test/capture results
should be recorded separately; source tests alone do not establish visual acceptance.

The edge-artifact audit also found the shadow receiver plane used derivatives of centroid
interpolation and rejected planes below a fixed world-area threshold. At 25 mm eye relief,
14 degrees and 450 native rows, one pixel spans about 14 micrometres: a legitimate plane
failed that threshold and fell back to the smooth material normal. The shader now retains
centroid positions for actual coverage/shading, uses separate pixel-centre camera-relative
positions for derivatives, and normalizes its two derivative directions before checking
angular degeneracy. Shadow bias and blocker membership are unchanged. An analytic perspective
plane regression covers ordinary and held-eye footprints and explicitly witnesses the old
cutoff failure; native image comparison remains necessary to assess all reported edge artifacts.

Use `catalog/shader_reviews/cupola_seven_fields_20260929.v1.json` for the same ordinary,
held and released seven-port comparison as the baseline. Its older caption/acceptance text
describes the previous retained obstruction; current acceptance requires clear held fields,
readable exterior contrast and no geometry hidden or removed to manufacture visibility.

Final tilted-view validation: the 45 mm candidate cleared the level source field but one lower corner still intersected the retained cap when the tank was pitched. The final 55 mm fitting passes the raster field test across all seven ports and transformed poses. The pedestal foot, cap shape and opaque occlusion remain intact.
