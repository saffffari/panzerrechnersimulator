# Loader hatch camera correction — September 5, 2026

The v7 roof-hatch video looked at the stationary panel `Turret_0105` ahead of the loader, while the actual lid is above and behind the original eye. Its phase counters passed, but that composition did not demonstrate the hatch opening.

The original Blender file was opened read-only in a separate background process. `Gunner Hatch Pivot` is an authored EMPTY parented to `Turret_Pivot`; its `gunner_hatch` child contains the five other moving pieces. Its engine-space hinge agrees with the existing constant to less than 0.04 mm. The empty's negative local Z is the existing engine +X opening axis. No pivot, geometry, export, parent, or runtime articulation was changed.

Source-triangle picking from the real loader eye `(-0.85, 2.033, 0.15)` establishes:

- Original forward/up film ray hits static `Turret_0105` at 0.3766 m, whether the hatch is closed or open.
- Looking toward the actual aperture at `(-0.70, 2.43, -0.10)` hits moving `Turret_0107` and `gunner_hatch` when closed.
- The same aperture ray clears every turret triangle after the existing 95 degree lid rotation.

The source scenario now turns using ordinary mouse-look inputs toward that aperture before the unchanged H press sequence. The camera remains at the actual loader eye; no debug camera placement or artificial hole was added. The CPU check reproduces the scripted input direction and verifies the closed/open triangle results.

Evidence: `artifacts/astra-audit-20260905/loader-hatch-source-inspection.json`, `loader-hatch-camera-geometry.json`, and their inspection scripts. `tools/validate_tiger_hatches.py` also still passes source partition/conservation and aperture checks. A fresh actual-game video is required before calling the user-visible hatch operation accepted.
