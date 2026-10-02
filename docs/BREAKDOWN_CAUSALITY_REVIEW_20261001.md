# Breakdown causality review — 1 October 2026

Exterior damage now confirms the articulated source triangles along the actual incoming direction within a bounded slab (0.10 m before the coarse contact to 1.25 m behind it). No source confirmation means no exterior component fault. The original cannon armor-envelope and penetration decision are unchanged; this is not a claim of complete triangle-based armor ballistics.

Source frame 8 identifies the retained telescope, and frames 5/9/10/11 the visor holder and moving shields. Other direct-component envelopes now use the confirmed point: the turret ring is a narrow 40 mm height band and 100 mm radial band around its 1.45 m radius, glass needs a point within 110 mm of an actual block center, and gun supports are local to the trunnion. These remaining geometric envelopes are explicit approximations.

The old unconditional radio/sight shock probabilities are removed. A general mantlet hit no longer also damages the coax or sight, a cupola hit does not automatically damage a neighbouring block, and a bow-plate hit no longer removes spare links without identified hung-link geometry. Penetrating fragments still damage their intersected internal targets; nonpenetrating frontal hits do not invoke that path or disable the rear engine.

Energy eligibility uses deposited normal kinetic energy: 0.5 × mass × max(striking_speed² − residual_speed², 0) × incidence_cosine². Residual energy is removed for penetration. Invalid/zero-energy or tangential strikes cannot cause exterior faults. Eligibility thresholds (glass 50 J, visor/MG 500 J, running gear 2 kJ, ring 10 kJ, gun support 80 kJ) and the ramp to twice that energy are engineering estimates, not measured failure data. Existing seeded local outcome rates remain tuning only after those gates. Historical anecdotes establish possible failure modes, not these probabilities. Explosive pressure and remote shock need an actual structural propagation model and are not guessed here.

`exterior_damage_receipt().last_source_strike` records source confirmation, actual articulated frame/contact, normal deposited energy, local zone, penetration flag, seed and applied candidate effects. A source miss is explicitly recorded. New-match exterior reset clears this evidence.

Broken-track shaft telemetry is separate from vehicle displacement. One broken side reports gear/RPM-derived powered shaft attempt while the intact side supplies actual traction. Both broken sides can report powered sprocket attempts while chassis force, speed, displacement and yaw remain zero. The latter uses an explicit 2% throttle estimate for drivetrain/loose-running-gear drag; it is not measured loose-link travel. Brake, engine/transmission failure, open clutch, neutral, opposing braking input or idle suppress the clatter drive signal.

CPU validation is recorded separately in artifacts/runtime. Source saved does not imply the previously running game or rendered review clips include these changes.
