# Enemy crew aftermath

Source: `src/enemy_crew.rs`, called from authoritative resolved enemy penetration after the wreck receives its actual impact event. Player Tiger anatomy remains unchanged.

- Enemy tanks now retain identity-separated crew state on actual penetration. Nonpenetrating strikes and snapshot-only deaths do not invent crew or damage.
- Four crew stations for T-34/SU-100/IS-2/IS-3; five for ISU-152/KV-2. This is the currently modeled roster, not a historical staffing validation (KV-2 historical crew completeness remains outstanding).
- The same deterministic penetration fragment generator and tissue work thresholds as Tiger supply injury, dismemberment and pulverization. Every fragment's finite 3 m compartment sweep is assigned to its first person/anatomical contact; its kinetic work is not repeated across the roster. Internal filler pressure follows spherical distance falloff; Heat uses no conventional filler.
- Distal limb loss can leave a survivor. Destructive central/head work kills; catastrophic ammo/turret events admit no fabricated survivors. Later cookoff terminates anyone still inside or climbing. No presentation explosion multiplier changes physics energy.
- Survivors wait briefly, climb individual approximate roof exits, traverse the roof/track support footprint, drop under gravity, and move away. Leg loss slows evacuation; missing segments retain gravity and terrain support independently. Actual burning wreck state supplies a coarse proximity-based heat dose and subsequent attached pixel flames/death.
- Eleven shared pools reuse the current textured Soviet actor split into anatomical source triangle pages. The source is normalized to the shared 1.53 m anatomical coordinates. Severed/pulverized parts follow authority; damaged bodies retain dark blood evidence. Shared restrained pool geometry is reused without red emission. Independent flame grains are sized by internal render pixels.
- Maximum 128 retained persons; no per-kill mesh creation. Pools start empty. Slot reactivation and encounter resets clear the relevant old crew. CPU/GPU/native validation is coordinated by root; no live game interruption by this change.

## Explicit limits

Enemy rigs do not provide the Tiger's authored seated interior and hatch collision contracts. Crew stations, roof exits and planar interior/roof supports are **approximate**; this pass does not validate animation through a mechanically animated source hatch. The four/five-person roster does not yet model every historical enemy station. Survivors use a shared static source pose rather than a new skeletal climb/run clip. External obstacle checks use existing cover AABBs and terrain, not the full infantry navigation solver. Heat dose is a radius/time approximation from a real burning source, not measured garment thermochemistry. Anatomical triangle classification is an approximate existing classifier, not cut-plane surgery; missing parts can expose source seams. Blood ground evidence is a bounded reusable pool rather than source-triangle wet-film on enemy armor. No new facial expressions, screams, anatomical inner wound surfaces or limb-cap assets were added. Host casualty state is not yet replicated as a dedicated LAN roster.

Tests cover undamaged body, isolated surviving forearm loss, and lethal internal filler. Root must run and fix them before calling this tested; native visual inspection remains necessary.

## Survival authority correction

A solved penetration is no longer automatically a vehicle kill. Crew damage resolves before the wreck classification. A path that misses crew and coarse modules leaves the vehicle live; later penetrations reuse persistent prior wounds. Crew inside a still live/moving hull follows the actual hull transform. An immobilized/disabled wreck retains survivors, while actual modeled ammo contact causes interior fatal catastrophe; neither a random WreckKind nor visual particle amplification kills crew. Generic destroy routes no longer roll ammo/fuel fires or secondary cookoff. The explicit staged fire capture still sets actual fixture burning state.

The enemy engine/ammo/fuel boxes are coarse hull-local compartment proxies, with a provisional 20 J solved fragment contact threshold; they are not authored interior meshes or calibrated ammunition ignition physics. No historical damage-validation claim is warranted. Nonpenetrating contacts leave this pipeline unchanged. Native survival, repeated penetration, bailout clearance and complete crew rosters need coordinated root tests.
