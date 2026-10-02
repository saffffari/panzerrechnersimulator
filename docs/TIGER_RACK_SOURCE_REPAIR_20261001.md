# Tiger rack source repair — 2026-10-01

The source `.blend` contains 64 actual stowed cartridges, each approximately 0.981–0.983 m long,
resting in 16 hull-fixed trays. `Hull_0559..0566` and `Hull_0735..0742` are in the existing hull
GLB. The extraction inherited the wrong parent for 48 of those cartridges, so their runtime
models rotated with the turret and swept through stationary side armor.

The old extraction also admitted `Turret_0125`: a 1.986574 m fitting, not a cartridge. The false
source slot 32 was above the regular tray grid and alone exposed 13 procedural-round vertices
through the hull at zero turret traverse. Its real 2176-triangle geometry had also been removed
from the static bake by that same long/thin filter.

The registry now contains 64 hull-frame round anchors. Zero-traverse world poses of every real
round are exactly unchanged. Runtime slot indices are compact (0–63), while source identities
remain 0–31 and 33–64. Source ID 32 is invalid. `shell_source_id` and its inverse preserve the
original 12 HE source assignments and cartridge identity used by damage, primer state, thermal
fire, manual/automatic loading and salvage. The chamber and loose-round ID ranges are unchanged.
Campaign saves store typed ammunition counts; LAN snapshots store chamber/discharge state, not
rack indices. No serialized format changed. The rack capacity is now 64 plus one chambered round,
so a count previously relying on the fictitious extra rack position no longer fits.

`tools/restore_tiger_rack_fitting.py` exports the omitted fitting from the original `.blend` into
`tiger_rack_fitting.glb` (71 KiB), preserving source triangles, normals and material. Its frame is
registered to an actual existing hull-tray root. The loader adds this source root to the hull's
remainder and material accumulation, so render, source collision and shadow geometry see it.
A future corrected bake that already contains the root is not duplicated. The large original
GLB and `.blend` are unchanged. The accompanying `tiger_rack_source.json` records source-object
names, dimensions, byte fingerprints and identity mapping. Both future bake filters exclude only
cartridge-sized objects; the fitting is assigned to the fixed hull.

CPU validation in OLD: `ammunition_stowage` **4/4 passed**. It preserves all 64 zero-traverse
world transforms, checks identity/nature mapping, verifies the exact restored source geometry
and idempotent loading, and casts rays against actual hull triangles from every distinct rendered
round vertex. Exposed vertices are **0** at 0°, 28.6°, 45°, 90° and 180°. The original registration
exposed 37 vertices at 45°. These side-to-vertex probes establish the reported side-armor bug is
removed; they do not certify all possible contacts or every historical rack dimension.

Log: `artifacts/runtime/rack_source_fix_cpu_tests_20261001.log`.
The source patch is mirrored to MAIN. No running game process, release executable, live settings,
original source asset or saved campaign has been changed. Visual validation remains pending the
end of the user's live playtest.
