# Gunner sight geometry audit — 5 September 2026

The current exterior optic camera is a placeholder halfway down the gun. `Tank::gunsight()`
places it at gun-local `(0.55, 0.30, 2.395)` m; the source comment explicitly says this was done
to clear the mantlet and opaque turret geometry. It is not the player's physical eyepiece or a
measured telescope objective. At zero elevation it is turret-local `(0.554, 2.145, 3.435)` m,
2.900 m from the authored lens center. The separate 66-degree interior camera supplies only the
surround of the composited sight picture.

A read-only projection of all 43,122 actual gun triangles found only a small muzzle-brake sliver
at the lower-right edge of the widest sight. At the current 25-degree vertical field, the closest
triangle lies 11.104 degrees from the optical axis. Its normalized radius is 0.44266, while the
composite accepts the exterior image below 0.45 and darkens the bezel from 0.432. No barrel
triangle enters the clear central circle. The intrusion is about 1.3 radial render pixels at
180 pixels high, or 2.6 at 360 pixels high. Both 12- and 6-degree fields exclude the barrel. Full
0.320 m recoil also excludes it. These results describe the nominal, unshaken camera, not every
impact impulse or a historical telescope's field.

The preserved `tiger_port_turret.glb` contains `gunner sight lens`, a roughly 83.6 mm diameter,
3.98 mm thick disc centered at turret-local `(0.496611, 1.994036, 0.539825)` m. Its planar normal
is along ±Z. The stored ports metadata bearing `(0.677, 0, 0.7359)` disagrees with that face:
tracing that direction from the lens hits its own source housing after 6.3 cm.

The source sight housing `Turret_0003.232` spans turret-local Z 0.533307–1.468944 m. Forward +Z
rays from the lens center and offsets of ±5 mm clear all source gun/turret triangles for 5 m;
several ±15/25 mm offsets meet the tube or nearby `Gun_0008` near Z 1.43–1.46 m. A narrow axial
path therefore exists in the source. The present interior camera's off-axis ray hits the housing
at Z 0.694 m, so moving its current pose backward or removing armor would not solve registration.

The next bounded correction is to register the pupil to the lens axis, positively identify the
distal objective/aperture, and verify the aperture cone and the sight's elevation linkage before
moving the external camera. Geometry inspection alone does not establish historical visibility,
magnification, eye relief, or a complete optical model. No camera, source mesh or armor surface
was changed by this audit.

Machine-readable measurements:

- `artifacts/astra-audit-20260905/gunner-sight-barrel-projection-20260905.json`
- `artifacts/astra-audit-20260905/gunner-source-lens-path-20260905.json`

The subsequent distal-face audit identifies two annuli at turret-local Z 1.433916 m. The open
one aligned with the labeled eyepiece centers on XY `(0.498174, 1.998528)` m, with a 19.46 mm
inner radius and 25.30 mm outer radius. The adjacent annulus centers on X 0.576013 m and has an
opaque source end cap at Z 1.468944 m. These are measured model apertures, not verified historical
optical identities. The labeled rear lens is a flat disc; its geometry alone does not specify
magnification or a focusing optical prescription.

Both the source lens and housing currently belong to turret-fixed runtime assemblies, while
the barrel and exterior sight camera follow gun elevation. A future physically registered
sight must address that linkage. Merely moving the camera onto the source aperture would align
one level-gun pose while leaving the source tube and elevated gun on different transforms.
