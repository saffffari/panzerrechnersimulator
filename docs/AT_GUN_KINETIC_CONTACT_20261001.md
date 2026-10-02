# AT-gun kinetic contact correction — 2026-10-01

The previous gun target was one large box around the shield/carriage. Any cannon contact wrecked
that gun and retired the projectile. An AP shell therefore never reached a crewman behind the
shield. The box also counted empty space as a hit, missed the protruding barrel and disappeared
from collision when the gun was wrecked.

Contacts now use the actual eight rendered component meshes, with the current lower/upper
transforms. The same source-part constructors build the visible meshes. A contacted steel part
provides its entry normal and ray chord to the existing RHA penetration solver. The gun loses
service, and a penetrating kinetic shell retains its identity, reduced forward speed and reduced
penetration budget. Its position stays at the entry contact: the next ordinary swept query still
compares infantry, terrain, walls, vehicles and other gun components by distance. The passed part
is excluded only while the projectile remains inside that same closed source component. Wreck
collision follows the same existing tilted upper-carriage pose used for rendering.

No AP area kill, automatic crew death or invented APHE burst is added. Crew on the continued
trajectory use the existing shared casualty path, which sets both body and combat-actor state
dead and starts the death animation. Exposed crew before the gun can still be the nearest hit.
A bystander off the trajectory is not killed merely because its gun was hit.

Limits are explicit: this gun is still a coarse procedural model. The steel boxes have no
authored steel grade; RHA is a declared ballistic surrogate. The visible shield is 40 mm thick,
which is source-model geometry, not a claim about historical 53-K shield thickness. The wheels
are solid boxes labelled tyre with no separate steel rim or measured rubber constitutive law;
tyre-only hits retain terminal behavior. APHE filler exists in ShellSpec, but an actual arming,
base-fuze delay and survival model is absent, so no APHE blast is fabricated here. The existing
instant cosmetic wreck-pose change also remains. These limitations need better assets/terminal
models before claiming a physically complete AT-gun damage simulation.

CPU regressions exercise exact shield/barrel contacts, rejection of the old empty hit box,
retained velocity after penetration, the actual downstream body and a nearer wall, a bystander
miss, exposed crew ordering, and persistent wreck geometry. Visual/live validation remains
pending the end of the user's running playtest. Source changes are mirrored in OLD and MAIN.
