# Bounded infantry and gun response

This changes actual decisions, not health, dispersion, ammunition, projectile contact, target
visibility, or the tank reinforcement schedule. Enemy main-gun cadence and Canyon admission
budgets are unchanged. Values below are authored tactical tuning, not measured human performance.

The AT crew formerly discarded its laying state every time LOS was absent for one update. It
now retains its last observed carriage/traverse for up to1.5s of occlusion. It cannot follow a
hidden target or discharge while occluded. Longer loss requires the original3s laying delay;
reacquisition cannot shorten an outstanding8.5s reload. Dead/grounded crews and limited traverse
keep their existing gates.

Successful player weapon discharges also queue infantry attention reports with the actual
emission clock and atmospheric sound speed. Current listener distance determines wave arrival.
Each report can be admitted once per listener; the queue holds128 reports and counts overflow.
Range remains the existing authored hearing-radius model, not the player's calibrated audio
mix. The initial route conservatively rejects blocked paths: terrain, live cover/trees/doors and
source Tiger triangles participate; no diffraction or muffled transmission is claimed. This may
miss audible obstructed reports. A passing blocked wave cannot be replayed merely by opening
cover afterward. Only living Soviet garrison-squad members consume these cues.

Hearing supplies an8m-grid source area with a small stable listener-specific uncertainty. It is
not the current player position, does not set `last_seen`, and expires4s after arrival. Within60m
a squad member may investigate toward that area, stopping12m short; beyond that it only turns.
Ordinary movement/body/terrain gates resolve each step. Cohesion takes priority. A cue alone
cannot aim a rifle, consume a cartridge or create a projectile.

A group with at least four ready, unsuppressed riflemen holding their own fresh observations of
the same area can assign two to a bounded lateral maneuver. Goals lie8m to either side and2m
forward of the group center, pass ordinary ground/cover checks, last5s and are reconsidered at6s
intervals. At least two covering rifles remain; movers do not fire. Covering soldiers use the
existing shared expert's current visibility, aiming-error, magazine/reload and rifle-cycle gates.
Lost contact, suppression, death or reload cancels a maneuver. This is a finite tactical overlay,
not a learned squad commander or a global navigation solver. Existing route collision and terrain
limits remain. No hit or awareness outcome is forced.

Mixed-combat telemetry reports hearing admissions/omitted reports and maneuver plans. CPU tests
cover wavefront timing, moving listeners, occlusion/replay, queue bounds, two-move/four-cover
roles, lost-contact cancellation, blocked goals, cartridge conservation while moving, and the
requirement for each covering soldier's own current sight. Native encounter review remains the
acceptance gate for perceived difficulty and visual behavior.
