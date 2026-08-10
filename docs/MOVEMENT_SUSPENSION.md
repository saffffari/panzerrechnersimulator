# Kessel — Movement & Suspension (diagnosis + plan)

## Where we are NOW (the bug you're hitting)
Movement is still the **Gate-1 kinematic placeholder** in `AKesselTankPawn::Tick` — there is **no
suspension and no obstacle handling at all**. `UKesselTrackedMovementComponent` is an **empty stub**.

Why a tiny bump or debris stops the tank dead, exactly:
1. The hull is one big box collider (630×300×250 uu) with `SetSimulatePhysics(false)`.
2. Forward motion is `AddActorWorldOffset(Delta, /*bSweep=*/true, &MoveHit)`. **Any** geometry the box
   touches — a 5 cm rock, a curb, debris — registers as `MoveHit.bBlockingHit`.
3. On a blocking hit the code does `CurrentSpeed = 0.f;` — a **hard stop, with no step/slope test.**
4. `FollowGround` only samples **one center line-trace** and only nudges Z. It runs *after* the swept
   move already blocked, and it never pitches/rolls the hull — so the flat box rams every leading edge.

So: the tank can't climb anything because the mover treats all contact as a wall, and the ground-follow
is a single-point flat-conform that can't represent a slope or a step.

## What needs to be designed/implemented

### Tier A — "conforming kinematic" upgrade (fixes the showstopper NOW, ~1 file)
Keep the kinematic model (no Chaos yet) but make it drive over terrain/debris:
- **Multi-point ground sampling:** trace down at 4 corners (or along the two track lines) instead of
  one center point. From the 4 hit heights, compute target Z **and hull pitch/roll** so the tank
  conforms to slopes and rides over bumps.
- **Step / slope classification on the sweep:** when the forward sweep hits something, inspect the hit
  normal + height. If it's below a **max step height** (~40–50 cm) or under a **max climb angle**
  (~30–35°), DON'T stop — raise the hull onto it (step-up) and continue. Only zero speed on a true
  near-vertical wall taller than the step height.
- **Suspension feel:** spring the hull toward the sampled target Z + orientation with a critically-
  damped interp (separate, slightly softer on pitch/roll) → the torsion-bar "settle" without physics.
- **Neutral-steer pivot:** the Tiger pivots in place (regenerative steering). Allow A/D to rotate the
  hull at low/zero throttle (turning circle ~0 → matches history). Ties to CONTROLS.md A/D.

This is the right scope to make the new tank model drivable and playtestable. It is NOT physically
accurate tracks, but it removes the "stops on a pebble" blocker.

### Tier B — real tracked physics (the proper system, bigger lift)
Grow `UKesselTrackedMovementComponent` into the real model (ARCHITECTURE §3):
- **Per-road-wheel suspension line traces** (the interleaved Schachtellaufwerk) → spring/damper forces
  on the Chaos rigid body each physics sub-step. Real ride, real obstacle climbing, real cresting.
- **Per-side tractive force** (drive sprocket torque), **strong lateral track resistance** (tracks
  resist sideways slide), **neutral-steer pivot** from opposing track forces.
- Hull becomes `SimulatePhysics(true)`; movement runs in sub-steps for stability at 57 t.
- Enables: throwing a track (mobility kill), freeze-jam flavor, climbing rubble, slope stalls, ground
  pressure (wide track crosses soft ground) — all from TIGER_I_SPEC.md.

### Historical hooks to fold in (from TIGER_I_SPEC.md)
- **Cannot fire on the move** (travel lock) — gate `Gun->Fire()` on near-zero speed.
- **Engine-RPM-dependent traverse** (6 → 19 → 36°/s) — couples movement throttle/revs to turret speed;
  revving to slew fast is loud (stealth tension). Lands once we have an engine/RPM model.
- **Ponderous accel** (~25–35 s to top speed), poor reverse (~10–12 km/h), neutral-steer pivot 3.44 m.

## Scope cut — tracks are first-person-invisible (2026-06-30 decision)
The player view is inside the tank, so **you never see your own tracks/road wheels while driving.**
That means **dynamic track + interleaved-wheel VISUALS on the player tank are not needed** — a big
chunk of Tier B is deletable. BUT keep the **felt ride**: hull pitch/roll/shake (camera) on bumps,
braking, and turns is what sells "57 tonnes" from the seat, and that's Tier A, not per-wheel physics.

Where tracks/wheels are actually on screen (all covered by a cheap fake):
- **Enemy tanks** seen externally → scrolling track-texture material (UV pans with speed) + static
  wheels. Reads fine at combat range; no real suspension geometry.
- **Brief unbuttoned glance down** → same scrolling-texture fake.
- **Wrecks / cinematics** → static.
- Track **sound** (squeal/clatter) matters more than the visual → audio, not geometry.

Net: **Tier A (felt ride) = do it. Tier B per-roadwheel visual suspension = mostly deleted; only a
texture-scroll fake for external/enemy views.**

## Recommendation
Do **Tier A now** to unblock driving the real model and to deliver the felt ride from the interior.
Tier A is a contained rewrite of the pawn's drive/ground code. Skip Tier B's per-wheel visual
suspension on the player tank (never seen); reduce it to a scrolling-track-texture fake for enemy/
external views, plus optional physics only if external cinematics later demand it.
