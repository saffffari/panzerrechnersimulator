# Source armor contact seam (partial coverage)

Current live cannon contacts now query retained source-triangle BVHs for the player Tiger
hull/turret and articulated armor hatches/shutters, and the active T34 prototype hull/turret.
The cache is constructed once during scene creation. The moving parts use the same current
frame transforms as the rendered/source fire geometry. Cabin racks, ammunition, glass,
crew and basket-floor machinery are excluded from this dedicated armor cache. There is no
per-shot/per-frame BVH build. Small-arms and other vehicle types retain their prior routes.

A source normal supplies full3D incidence. The table's design slope is zeroed for a verified
source triangle to avoid applying the modeled surface angle twice. Shell range, striking
velocity, caliber, penetration-budget loss and damage resolution retain their common route.
A roof/belly source triangle therefore no longer disappears at the legacy OBB y-normal
rejection. A real hatch aperture relies on actual mesh cutouts and live articulated lids;
no armor envelope is added across the opening. This needs native aperture acceptance.

Thickness is NOT inferred from mesh wall spacing. `SurfacePlate` retains a nominal plate,
manufacture description, mapping coverage and provenance. Most plates still have class-level
legacy values rather than a measured per-triangle map. The Tiger1943 horizontal class is
nominal25mm; the1943 US report lists1.02in (~25.9mm). Lower hull side nominal60mm follows
its2.44in (~62mm) table, but the local height cutoff1.05m remains an authored map boundary.
T34 horizontal20mm is explicitly unverified authored fallback: the British original report
was located but could not be downloaded/read to establish its15/20mm distribution. Other
vehicles retain old contacts and cannot be claimed exact. Mantlet/barrel-specific contact,
cast-thickness variation, welds/overlaps and detailed vehicle variant manufacture coverage
remain unresolved. The cache covers hull/turret/selected moving armor, not every surface.

Primary historical report (archived reproduction):
https://lonesentry.com/articles/ttt34/german-tiger-pzkw-6.html
US War Department, Tactical and Technical Trends No34,23September1943; armor table.
Its documented wartime uncertainty must be retained; this is not a modern CAD survey.
Located original British STT T34 report (read blocked by remote connectivity):
https://electronicsandbooks.com/edt/manual/Military/%21%20NEW/Preliminary_Report_No_2-0_Russian_T-34_Medium_Tank.pdf

Implementation is staged source work. GPU/native tests and complete thickness-map acceptance
are not claimed. Source changes were paused to prioritize the failing live audio queue test.
