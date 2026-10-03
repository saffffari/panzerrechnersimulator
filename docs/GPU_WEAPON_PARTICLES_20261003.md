# Requested weapon presentation scale

The user's requested 100× exterior explosion volume is represented by a 4.641589× linear
flash/smoke support scale. Only semantic exterior `Blast` records select it. Interior
catastrophe flashes, muzzle flashes, armor-impact origins, ammunition, damage, physical
fragments, turret motion and point-light source powers are unchanged. This is an authored
presentation adjustment, not a claim of 100× physical explosive energy. Exterior cosmetic
spark velocities use the same linear scale; gravity remains 9.81m/s².

The raster backend requests 1000× the existing cosmetic spark samples:384,000 per blast or
heavy ricochet,192,000 per hot impact, and the existing energy/incidence-derived bullet
count multiplied by 1000. A GPU vertex shader evaluates their stable emitter/index hashes,
elapsed-time trajectories and individual lifetimes. One emitter is 80 bytes;48 emitter
records are 3,840 bytes. It does not build 384,000 CPU matrices. Existing flash parcels and
approved sustained fire/smoke continue through their established paths.

The per-view cap is 1,536,000 sparks. Whole bursts that do not fit are omitted and explicitly
counted; surviving bursts never receive compensation brightness. Camera-local conservative
frustum bounds cull only wholly absent burst envelopes. Maximum 48 bursts and 4096 actual swept
tracer segments bound upload storage. Per-view GPU buffers persist until renderer resize.
Counts are submitted samples; individual lifetime, source-plane and depth rejection can
produce fewer visible pixels. A capture with omitted bursts is not evidence of the full
requested density. `Renderer::debug_weapon_particle_receipt()` reports all these distinctions.

Every spark rasterizes to one native-view pixel. Its approximate projected source-support
area is divided by 1000 to avoid multiplying total cosmetic emission by the count multiplier.
This is emitting-area quadrature, not a calibrated thermal-radiation model. The area estimate
uses a conservative width/tail footprint; it is not a formal orientation-exact radiometric
bound. Source-plane rejection retains the complete original millimetre-width streak support,
including its orientation, so pixels originate only from allowed exterior trajectories.
Secondary collision with other hull surfaces is still not simulated for cosmetic sparks.
Shared atmosphere/smoke extinction and actual per-view scene depth remain active.

Actual small-arms tracers use only authoritative swept substep endpoints. Their strips have
one pixel of geometric screen-space width using each camera's homogeneous projection, not
Euclidean eye distance. Near-plane clipping retains the real endpoints. No separate white
core is added. Main tracers are added after bloom and optical distortion but before tone
mapping. Vision-port tracers use a separate transparent HDR layer, then the actual authored
entrance-face projection after bloom. The relay's existing fitted texture-to-pane resampling
can make one source-view pixel subpixel in final output; exact one-output-pixel width through
that projective resampling remains an explicit limitation. This must not be hidden by bloom.

The shutter retains actual bullet history for max(1/120s, render interval), capped at 1/30s.
It does not synthesize missing rounds or extend beyond impact. Below 30 presentedFPS that cap
can still leave gaps. Renderer-independent projectile simulation and selected tracer-round
policy remain unchanged. Legacy physical-backend fallback retains its old CPU presentation;
this GPU path is the practical raster backend only.

CPU reference checks:6/6 pass for native width at 640×360,800×450,960×540 and 64×36 across
depth/FOV/off-axis positions; exact near-plane clip; volume/count/upload bounds; complete
source-plane support on vertical/sloped armor, cookoff apex/energy caps and terrain energy/incidence. Both fully composed WGSL modules pass Naga
validation. Integrated Cargo tests, native GPU validation and performance/visual acceptance
are pending the parent's serialized build/capture. No game process was stopped or relaunched.

A separate full-cookoff stage now requests 768,000 cosmetic ejecta samples. Its maximum
vertical target is 304.8m (1,000ft), from a 77.3316m/s vertical launch and gravity. An explicitly
authored 5kg presentation reservoir receives at most 0.2% of the existing modeled ammunition
TNT energy. Each sample's velocity is capped by that finite mass/energy budget. This is not a
measurement of the real ejected mass, actual enemy ammunition inventory, secondary collision
or damaging fragment count. The physical turret's independent source mass/energy is unchanged.
A connected, optically mass-normalized smoke support follows the tall envelope; no giant opaque
smoke box is introduced.

The wreck pressure sequence is one-shot: a short jet leaves the recorded armor breach before
scheduled cookoff, or at catastrophic separation. Once a turret's source transform has moved
more than 0.12m, the large plume can use its original ring transform. Delayed cookoff retains the
actual breach. The 0.12m separation condition is a presentation gate, not a complete finite
aperture-clearance solver. Existing damage authority decides whether those events occur.
Root owns the pressure-jet sound call at the marked update_wrecks hook.

Enemy baked GLBs expose only one merged node per hull/turret/gun and currently have no retained
hatch aperture/open-state contract. Approximate crew roof escape points are not used as vents.
Closed-hatch pressure-driven opening mechanics remain unimplemented. Authoritative damaging
fragments and turret motion are not strengthened to match the requested cosmetic scale.

Terrain-only contacts now feed actual residual velocity, mass and caliber to a separate matte
native-pixel ejecta record. Its upward height is capped at 6.096m (20ft) and kinetic energy at
10% of residual projectile KE; weak rounds cannot be forced to that height. Ground normal is
sampled from the actual heightfield gradient. Forward bias follows incidence, with gravity and
complete source-plane rejection. Estimated displaced support is a cylinder of radius 6 calibers
and depth 12 calibers, using 220kg/m³ snow or 1600kg/m³ soil. These are declared material/support
estimates, not validated crater constitutive physics. No energy is invented from projectile
muzzle velocity after it has slowed. Hard cover, structures, bodies and range retirement do not
select this soil route. Former generic Terrain puffs are suppressed in raster mode to avoid
counting the presentation twice; physical-backend fallback retains them.

Ground samples partition the estimated mass; their physical equivalent-sphere diameters set
opacity area. They use shared source lighting and atmospheric attenuation, not additive glowing
smoke. Bursts are sorted back-to-front for matte compositing, with unsorted individual grains
inside each burst. Tiny cosmetic grains do not cast individual shadow maps or affect gameplay.
The shared 1,536,000-particle per-view cap includes ground, pressure jets and full cookoff; the
receipt distinguishes these categories from sparks. Native GPU cost/appearance is still pending.

Vehicle terminal presentation now consumes the resolved `ImpactEvent`. Penetration creates a
small exterior metal-dust spray, a source-plane-confined nonpyro puff and an annular cooling rim;
it never selects a ricochet or generic fireball merely because the projectile contains filler.
Shatter gets compact dust/sparks and bounce gets reflected cosmetic spray. An exterior detonation
requires positive actual filler and `HeBlast`, or the solver's armed HE/HEAT thin-plate
`Penetration` response; that chemical breach is distinct from ordinary kinetic perforation.
The separate inherited physical HE
damage branch still needs its own armed-terminal audit; this change does not alter it.
The retained-speed AT-gun perforation route likewise uses actual lost KE and source steel contact
for a nonpyro spray, replacing its former generic ricochet/flash. Rubber has no metal spark route.

The rim has512 analytic annulus samples, included in the existing shared cap and48-record pool.
Its hole radius is half the actual caliber. Heated width is0.1caliber,2–25mm; depth is3% of the
struck plate thickness,1–6mm. Steel density7850kg/m³ and heat capacity500J/kg/K estimate the thin
rim mass/reservoir. At most1.5% of actual lost projectile KE heats it, capped at2000K. Weak strikes
are not guaranteed a molten glow. A2.5s exponential cooling estimate plus Planck-band color ratios
changes its emission with elapsed time; the display radiance scale remains authored. This is not
a validated melting/conduction solver or calibrated steel emissivity. Its geometry follows the
actual hull/turret contact frame and outward normal, including a physically detached turret.
The ordinary persistent armor scar/damage record remains unchanged. The rim emits no invented
point light and does not enlarge the hole or affect penetration resistance.

The ballistic vehicle `Bounce` route currently retires its projectile. Reflected cosmetic metal
grains are therefore not evidence of an outgoing shell; no reflected shell path was fabricated.
Real ricochet continuation requires a separately authorized residual-energy/direction response
inside the authoritative projectile contact loop, not an effect emitter.
