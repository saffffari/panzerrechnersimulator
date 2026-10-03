# Existing hand props and finite cigarette geometry

`src/hand_prop_models.rs` loads the existing licensed Pizza GLBs through the normal
resource/glTF importer. No original GLB, image or texture is edited. The source
root is `assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/`.

- `Small Props/cig_1.glb`: straight eight-sided153mm×13.5mm source, uniformly
  normalized to84mm×7.412mm. The texture identifies +X as the filter cap and -X as
  the tobacco end. The held copy is clipped at the burning end, with side UVs
  interpolated along the original surface; the mouth/filter position and diameter
  never shrink. The authored22mm filter leaves62mm consumable tobacco.
- `Items & Weapons/cigs_packet_1.glb`: source box faces are retained; its actual
  +X filter-end quad becomes a hinged lid. Twenty copies of the source cigarette occupy7/6/7
  rows inside the uniformly scaled source packet. Visible count comes only from
  `cigarette::View.pack_count`, with the top layer removed first. Packet opening,
  hinge and hand pose are new project authoring; the source was originally closed.
  The end opening faces the eye. CPU source-triangle rays reach all20 filter-cap
  centers without crossing the packet body or its open lid; this is geometric
  visibility, not proof that all20 caps are legible under every lighting condition.
- `Items & Weapons/lighter_mp_1.glb`: source disposable-style lighter, with an
  authored wick anchor on the existing burner rather than on the flint-wheel tip.
  RMB pose and flame availability come from the state owner. The8mm-high,2mm-wide
  flame is ordinary depth-tested emissive geometry. Its small1mm-radius point source uses
  the exact nozzle anchor plus3.5mm world height and enters the shared geometry-
  shadowed light bank. Output0.00025 is an authored game-source value, not a
  measured lighter specification. Its dedicated registration exempts only the
  emitting flame mesh from its own light's shadow; the lighter body, other props
  and armor still cast shadows. There is no camera fill or distance cutoff.
- `Items & Weapons/flashlight_mp_1.glb`: all282 source triangles are retained.
  Source emissive-atlas centroid samples partition the lens from the housing;
  the two groups are rotated together to the existing local+Z beam and uniformly
  normalized to the existing15mm lens radius at z=.035m. The existing flashlight
  transport and power stay unchanged. This triangle-level lens-material partition
  is an approximation of the source texture mask, not a new illumination bypass.

The unlit cigarette uses its original textured tobacco cap. Lighting swaps that
surface for one warm emissive cap at the same cut boundary, avoiding overlapping
coplanar faces. Demeter's shared surface parameters drive char/ash sleeves and
ember intensity. The cap is emissive-only: negligible remote ember illumination
and indirect light are not solved or claimed. Lighter flame and point light are
separate from this deliberate tiny-ember limit. Smoke/ash consume the same world
`Anchors` as the meshes, including the actual last uploaded cigarette length.

There are29 fixed ordinary mesh slots, normally hidden, plus the existing two
flashlight slots. The pack uses at most20 small source meshes; no extra screen
pass, texture upload or lighting model is introduced. All props register in the
existing dynamic caster set to avoid invalidating static world shadows on every
hand movement. `mark_raster_only` follows the existing personal-item path: source
shadow maps render them, but the experimental hardware-TLAS path excludes these
split-view personal props. This is not full hardware-ray visibility parity.

The red/white commercial-style packet and disposable-style lighter are existing
stylized art, **not verified period equipment**. No brand or historical provenance
is inferred from their filenames or texture. Replacement period artwork remains
an art task if that fidelity is required.

The initial six actual-source CPU geometry regressions passed through an isolated rustc harness:
fixed filter/diameter during shortening;20 nonoverlapping cigarettes within the
packet; complete body/lens partition and registered lens dimensions; bounded slot
allocation; separate paper/tobacco cap surfaces; light gating at the visible wick.
The renderer methods are stubbed in that harness, so it proves source geometry and
API syntax, not native rendering or input integration. No Cargo or GPU job ran in
this lane. Log: `artifacts/runtime/hand_props_source_cpu_tests_20261002.log`.
The CPU source contact sheet is `captures/hand_props_source_geometry_20261002.png`.

The integrated test executable now passes8/8 hand-prop tests, including two later
regressions: all20 filter-cap centers clear the real packet/lid triangles, and
the source cigarette across its puff poses plus the complete raised lighter fit
the ordinary60-degree vertical,16:9 native camera field. The flame dimensions and
point-source height are checked in that same geometry test. Receipt:
`artifacts/runtime/sorted_final_hand_prop_models_tests_20261002.log`.

## Native cabin review: mechanics pass, readability remains open

`captures/cigarette_ritual_v2_20261002_manifest.json` passes15/15 outcome checks
and all required actions. The video demonstrates finite stock, lighter ignition,
three puffs, burn consumption and stowing without firing a gun. Its stills at1s,
3s and17.5s were inspected: the revised opening and full lighter are in frame, but
the camera-facing packet, filter caps and cigarette remain very dark. This is
**not accepted cabin material readability** and must not be called visually finished.

The source audit finds nonblack sRGB textures, explicit metallicFactor0, outward
unit normals agreeing with triangle winding, and retained UV/material channels.
The same images show a lit packet lid and a thin illuminated cigarette edge.
The active forward shader has direct geometry-shadowed practical lights and an
exterior environment probe, but no reflected practical-light bounce between cabin
surfaces. Its current light/face arrangement leaves much of these visible surfaces
without direct illumination. That is a concrete lighting-model limitation; the
audit does not prove exact shadow-map parity at every small-prop pixel. A matched
daylight exterior capture is the next check of authentic material appearance.
No emission, exposure, fill light or shadow bypass was added to conceal the issue.

Remaining visual acceptance includes cabin indirect-light refinement, exterior
material readability, the held flashlight, and nearby-wall depth/shadow checks.
The bounded smoke/ash particles retain their birth air domain and have no armor
collision sweep; they use ordinary depth and lighting, but are cosmetic transport,
not a sealed-compartment smoke solver.

Daylight native follow-up completed: `captures/cigarette_daylight_v1_20261003.mp4`,15/15interaction checks. Frames at1,3,6and17.5seconds visibly retain white paper, tan filters, packet stock and the red/brown source lighter. This confirms the cabin darkness is not lost source artwork; cabin reflected lighting remains a separate unresolved visual limit. No rendering-source changes were made for the daylight comparison.
