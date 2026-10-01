# Detached turret motion, 2026-10-01

The earlier wreck path assigned every catastrophic turret a random 14–20 m/s vertical launch,
2–6 m/s lateral launch and 6–14 rad/s spin, then froze its ring at ground + 0.4 m. That produced
roughly 10–20 m of ascent above the ring and repeated full rotations regardless of its source
mesh, gun, damage, or landing orientation.

`src/wreck_turret.rs` now builds a detached turret and attached gun body from the loaded source
triangles. The exact articulated turret/gun pose is retained at detachment, including the hull
slope and laid gun pitch. Area times thickness times steel density supplies a thin-shell mass
estimate. The mass centroid and triangular-lamina second moments supply its inertia tensor.
Wall thickness uses the existing armor scheme; roof/floor use 30% of its side thickness and
the attached gun uses a 25 mm shell estimate. These are explicit mass approximations, not
measured historical assembly weights.

The existing modeled catastrophic ammunition equivalent (3.67 kg TNT) supplies the energy
scale. Only 0.12–0.28% is coupled into turret kinetic energy, according to the actual killing
round's retained ignition severity. The outward normal at the actual struck plate supplies
the lateral direction. Pressure eccentricity uses 3.5% of the measured impact-to-COM lever,
capped at 3 m before that weighting; it produces a small directional rotation. Impulse size
is solved against the combined translational and rotational kinetic-energy budget. These
coupling coefficients are authored pressure approximations; there is no simulated pressure
vessel rupture, detachment fracture, or reconstructed enemy ammunition inventory.

Gravity and angular momentum advance in substeps no longer than 1/120 s. Contact samples
are generated over the same turret and gun triangles, with a reported maximum triangular
lattice spacing (target 0.18 m; subdivision count is bounded). Terrain contact uses the actual
heightfield. Descending samples sweep the original wreck hull's source triangles so a turret
can hit its own deck. Near-coplanar supports within 1 mm of the deepest contact share a
manifold centroid, avoiding arbitrary corner torque when contact depths tie. Inelastic
impulses, contact damping and a quiet-period threshold with rocking hysteresis let
it settle at source-supported geometry instead of a constant ring altitude.

The surface sampling is a bounded approximation, not exact triangle/triangle contact:
features between support samples and other world objects are not covered. The ground normal
uses a 0.10 m finite difference. The source hull is queried on descent; separation during
ascent is prescribed by the existing catastrophic detachment decision. Contact damping is
an authored energy-loss model. The pre-existing 35% catastrophic classification and generic
enemy penetration/kill policy remain unchanged and should not be described as an ammunition
damage simulation. The source-derived body improves the motion of that outcome only.

Focused regressions verify source mass changes launch speed while keeping the energy bound,
the initial rendered pose does not jump, supported landing settles without perpetual spin,
and the actual loaded T-34 has a short detachment and settles within 12 s. The main change's
physical-truth ledger must retain these limits alongside the visual capture receipt.

Validation: all three focused tests passed in
`artifacts/runtime/wreck_turret_final_tests_20261001.log`. The loaded T-34 fixture measured
an estimated 7,794.414 kg assembly, 3.218 m/s initial speed, 0.305 rad/s initial angular speed,
and 0.327 m maximum COM rise. Its 21,376 source support samples had a maximum lattice gap
of 0.1793 m; the body settled with zero final linear and angular velocity. These numbers
describe the explicit source-mesh fixture/model, not a reconstructed historical detonation.

The later native `blizzard_external_v3_20261001` capture exposed a frame error that the
origin-only fixtures missed: `SourceContacts` returns hull-local points and normals, which
were mistakenly used as world-space contacts. At the real wreck location this created a
roughly 100 m positional correction. That capture is invalid visual acceptance evidence.
The contact point and normal now pass through the contacted source model before depth and
impulse calculation. A translated, rotated, sloped, gun-pitched T-34 regression near the
failed capture coordinates checks individual step size, maximum rise, equivalent motion
under translation and settlement. Runtime receipts now expose initial COM, peak COM rise,
and maximum contact correction so native validation can reject recurrence.

The corrected suite passes 4/4 in `artifacts/runtime/wreck_turret_worldframe_final_20261001.log`.
The translated fixture measured a 0.024924 m maximum step, 0.294449 m maximum COM rise and
0.019226 m maximum trajectory difference after removing the world translation; both bodies
settled. The corrected native v4 exterior, cupola and interior captures also passed: maximum
COM rise was 0.295107 m and maximum contact correction was 0.007184 m. The turret settled
on the wreck by approximately 7.02 s and remained at rest. See
`docs/BLIZZARD_CREW_REVIEW_20261001.md` and `catalog/combat_crew_revision_20261001.json`.
