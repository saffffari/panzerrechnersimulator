# Driver controls and P08 reload acceptance — 13 September 2026

The Tiger's actual `Hull_0238` steering yoke and `Ignition Button` source meshes are now
operable in the first-person driver view. The existing source-weighted crew arms follow
the yoke; LMB on the red ignition button reaches and presses it. Engine fuel, damage,
cranking and start/stop rules remain authoritative.

The display above the button uses the existing `Hull_0240` illuminated face. It prints
`FUEL` and the floor of the real remaining litres, with `---` for unknown or invalid
state. This digital presentation is a gameplay readout, not a historical instrument
claim. Its 36-vertex print changes only when the displayed whole-litre value changes.

The P08 rises to a rigid reload hold before magazine extraction, then returns to its
existing hip or aimed pose. Original gun dimensions, magazine travel, action linkage,
hold-open/release timing and magazine exchange timing are preserved.

Validation completed:

- CPU: driver fuel tests **2/2**, P08 source/kinematics/projection tests **4/4**.
  Projection checks use all actual source triangles at 45° and 68° vertical FOV,
  three aspect ratios, and three look directions.
- Driver GPU fixture: **9/9 checks passed**. Steering was `+0.5226` rad at 1 s and
  `-0.5362` rad at 2 s; both physical hand targets were retained. Actual engine
  stop, crank and restart were observed. The initial v1 capture exposed an update-order
  bug that held the cosmetic yoke at zero; the accepted v2 records the correction.
- Both P08 GPU fixtures: **4/4 checks passed each**. Eight ordinary LMB shots empty
  the weapon, ordinary R reloads it, and a ninth shot proves the reload completed.
- Visual inspection: opposite steering poses at 1.0/2.15 s show hands following the
  yoke without gross separation or cabin penetration. Ignition contact at 3.58 s is
  visible and `FUEL 419L` is readable on the source face. P08 extraction at 3.45 s,
  insertion at 3.85 s and toggle pose at 4.36 s retain the complete weapon/magazine
  in frame in both hip and aimed views.

Accepted captures and receipts are under `/home/alex/panzer/shader-graphics/`:

| Review | Capture | Runtime receipt |
| --- | --- | --- |
| Driver, ignition, fuel | `captures/astra/driver_hands_20260913_v2.mp4` | `captures/astra/driver_hands_20260913_v2_manifest.json` |
| Hip P08 reload | `captures/astra/p08_reload_20260913_v1.mp4` | `captures/astra/p08_reload_20260913_v1_manifest.json` |
| Aimed P08 reload | `captures/astra/p08_aimed_reload_20260913_v1.mp4` | `captures/astra/p08_aimed_reload_20260913_v1_manifest.json` |

Corresponding `_visual_review.json` files record reviewed frame paths and limits.
Fixtures are `catalog/shader_reviews/driver_hands_ignition_fuel.v1.json`,
`p08_reload_framing.v1.json` and `p08_aimed_reload_framing.v1.json`.

These are captures of shipping gameplay input fixtures with disclosed staged positions
and enemies disabled, not native OS-event replay. Audio was not auditioned in this
review. The steering inspection aims at the low hub, so its upper hand extends partly
beyond the frame at extreme steering; ordinary head look can include it. Existing
low-poly hand geometry moves with the weighted arm rig; independent finger curling
and new first-person P08 hands were not added.
