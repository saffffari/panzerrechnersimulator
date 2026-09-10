# Driver visor source-geometry repair

The blank orange driver view was an actual gameplay failure. Raising the camera toward the pane
and refreshing the old RTT hooks could not restore an exterior view: the renderer intentionally
retired those hooks, and the source assembly had no complete direct dielectric path.

The read-only triangle audit starts at the former hull-local eye `(0.535, 1.68, 1.82)` and aims at
the declared pane location. It encounters:

| Geometry | Distance from eye | Meaning |
|---|---:|---|
| `Hull_0189` in `tiger_rtt_hull.glb` | 0.5678 m | Inner glass face, exported as one opaque rectangle |
| `Hull_0189_housing` | 0.6653 m | Opaque rear backing of the glass holder |
| `Hull_0164` | 0.7152 m | Upper portion of the external armor opening |

A horizontal sweep behind the holder establishes that the supplied external armor already has a
narrow open lane at approximately `y=1.725..1.740`. The old low eye looked upward into its upper
edge. No new opening is cut in `Hull_0164` or in the tank's armor.

`src/tiger_driver_visor.rs` derives the repair at load time. The named holder is partitioned from
the hull separately from the roof hatch. Its existing pane bounds define a six-plane glass volume;
only holder triangles inside that volume are removed. The remaining frame keeps its own original
material page, UVs and bounds. It stays a separate mesh so its material atlas cannot be confused
with the hull's atlas.

The old one-sided rectangle becomes a closed six-face glass block. The registered normal distance
to the source rear plane is 97.3 mm; horizontal depth is about 98.54 mm. The block extends along the
hull's forward axis, retaining the shared lower edge measured on the source front and rear faces.
Extruding along the sloped normal would incorrectly raise the rear lower edge by 15 mm and narrow
the usable field. These dimensions describe registration to the supplied model, not an independent
historical measurement of a Tiger laminate.

The ordinary High renderer traces both glass interfaces with its existing IOR, Fresnel and
absorption model. The new block uses the original pane's material. It is not emissive and does not
sample a second camera or substitute image. Other unimplemented periscopes remain unchanged.
The shared solo/couch/title driver eye is now `(0.535,1.733,1.82)`, looking horizontally through the
existing slit. Opening and closing the external armor cover is still separate work; no movement
or mechanism has been invented here. Armor/penetration collision semantics are unchanged.

All original GLBs remain untouched. Runtime registration guards reject changed pane/holder bounds
instead of cutting a moved asset. Two focused Rust tests check retained frame/material, two-sided
glass thickness, and registration to the actual source pane. CPU geometry evidence is saved in:

- `artifacts/astra-audit-20260905/driver-visor-source-ray.json`
- `artifacts/astra-audit-20260905/driver-visor-refracted-source-rays.json`

The CPU derivative keeps the holder bounds and changes its surface area from 0.170395 to 0.157291 m²,
removing only the registered aperture backing. Nine refraction probes use the existing shader's
IOR 1.52: horizontal and 1° downward rays at yaw −6°, 0° and +6° clear the source exterior; 1° upward
rays still hit the original upper steel. This is a bounded geometry check, not a rendered success
claim or a physically calibrated historical optic. A new actual driver video remains the final
visible verification, and the earlier failed driver films must be retained as failures.

The v5 GPU probe subsequently traversed both glass interfaces and reached exterior scene mesh 1
at 48.2169 m. This establishes the direct path through the repaired assembly. Its narrow bright
slit still needs motion/readability review; a successful ray is not a complete playable-view claim.

Held ordinary Driver aim/RMB now advances a 0.4 s seated lean toward hull-local
`(0.535,1.733,2.10)`. Release returns along the same path. Eye height, forward alignment and 48°
field of view remain fixed. The normal game, local crew view and capture all read this same eye
helper. This is local camera input, not a new replicated remote-crew command. Aim is inactive while
changing stations, climbing, performing an activity, dead, or already marked roof-exposed. It does
not set exposure, open a hatch or grant maintenance access. The source-based pose approximation
has at least 152.66 mm eye-to-triangle clearance along the 280 mm path, sampled every 10 mm or less
against nearby hull, turret and gun geometry at neutral turret yaw. A 120 mm eye-centered diagnostic
sphere therefore retains at least 27.66 mm clearance after a conservative 5 mm sampling allowance.
This does not validate shoulders, the whole body or historical seat ergonomics. Exterior usable
field remains limited by the farther outer armor opening, so no doubled-field claim is made.

The driver's roof lid also carried a separate eyepiece, `Hull_0300`, in `tiger_rtt_hull.glb`. Its
housing moved with the lid while that pane was left floating below the roof. The pane is now
partitioned into its own material-preserving mesh and driven by the exact same hinge transform as
the complete roof assembly. It is removed from the static screen page; `Hull_0241` remains there.
The roof periscope's mirror/optical transport remains unimplemented.

An independent roof-exit audit found that the eye must move sideways before rising: seated eye →
`(0.98,1.733,1.89)` → `(0.98,2.25,1.89)`. With the complete lid and eyepiece fully open, that two-leg
path retains at least 221.28 mm sampled source-triangle clearance; a direct diagonal crosses the
fixed roof. This is a measured proposal for later exposed movement, not an implemented lean-out
control. Opening the lid alone remains separate from exposing the driver.

The CPU-only repeatable audit is `tools/audit_driver_head_path.py`; it transforms the actual source
lid and eyepiece before bounding and exact point-to-triangle queries, without decoding textures.
Receipts are `driver-roof-opening-rays.json`, `driver-head-path-clearance.json` (the stale eyepiece)
and `driver-head-path-clearance-attached-pane.json` under `artifacts/astra-audit-20260905/`. Focused
Rust regressions cover source pane/material conservation, hatch motion, bounded reversible lean,
and retaining both visor interfaces throughout the seated path. Visual validation of the lean is
still pending a fresh first-person film.
