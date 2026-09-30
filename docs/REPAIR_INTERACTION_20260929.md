# Repair interaction — 29 September 2026

The reported "damaged traverse" has not been identified as a particular surviving fault in the
user's live session. A turret that will not turn while the toolbox is carried is not proof of
damage: carrying tools deliberately disables gun laying, even on a healthy Tiger. Put the tools
down and operate the gunner controls before diagnosing the traverse. With the engine off/dead or
hydraulics faulty, manual traverse remains available at one quarter of the powered rate (about
1.5 degrees/s versus 6 degrees/s). A jammed or wrecked ring blocks both drives.

## Input and physical requirements

- Pick up the source toolbox/hammer with LMB while its mesh is within the pickup ray's 1.85 m
  range, with free hands and a clear path. The pickup press is consumed: release before using it.
- A fresh LMB use now starts a visible carried-hammer swing immediately, including empty air.
  Holding repeats the approximately 0.48 s cycle. A quick click completes its animation.
- Repair work requires the relevant damaged part in front of the eye, at most 1.65 m away,
  within 0.22 m of the look ray, and an unobstructed path against the retained source triangles.
  The existing contact tolerance is 0.28 m. None of these gates was widened for the input fix.
- Hold LMB on that eligible part for the job duration. Releasing, losing the target or changing
  jobs resets the ordinary repair clock; work in empty air is never banked toward a later target.
- A contact clang is allowed only when an eligible repair target is present at the swing's strike
  phase. Air, obstructed and non-repairable targets have animation but no repair-contact sound or
  repair credit. This is still the existing look/reach repair approximation, not a swept hammer
  head collision solver.
- In tank mode, the existing **wordless dot** now appears only for an eligible repair target while
  carrying the toolkit. It no longer appears everywhere merely because the kit is held. The
  internal `job_prompt` strings are diagnostic labels; ordinary play does not render those words.
- Q/E put the tools down through the existing equipment interaction. Releasing a toolkit now
  runs before the Commander station's pickup restriction, so visiting that post cannot trap the
  tool in the hands. New pickup restrictions remain; E retains earlier world-interaction priorities.

The old `repair`/G (and older R comments) are not the current carried-tool work path:
`update_activities` calls the LMB repair path. There is no universal key that repairs the tank from
anywhere. Switches, climbing and terminal death retain their existing interaction gates.

## Current fault jobs

Times are authored gameplay durations. Anchors are the current implementation, not a claim that
each named maintenance procedure is anatomically animated or mechanically solved.

| Fault/job | Current anchor/access | Time and result |
|---|---|---|
| Traverse hydraulic fault | Interior traverse control, `interior_rig::WHEEL2_POS` | 3 s; clears fault and restores powered traverse when the engine works |
| Fragment jam in turret ring | Actual recorded `exterior.ring_jam` point, normally reached from the outside hull deck | 12 s; removes the jam; steel blocks work from inside |
| Legacy/spall traverse failure | Same interior traverse-control anchor | 3 s; restores module |
| Recoil brake leak | Gun-local cradle top on loader side, `(-0.12, 0.22, -0.55)` | 20 s; restores firing |
| Sight out of true | Sight body just ahead of gunner eyepiece | 25 s; clears sight error |
| Bent MG barrel | Coax gun-local anchor or hull-MG module station | 6 s; restores MG |
| Radio shock | Hull radio module station | 2 s; restores radio |
| Legacy transmission failure | Hull transmission module station | 7 s; restores module |
| Legacy coax/hull-MG failure | Corresponding MG anchor | 1.6 s; restores module |
| Opaque cupola vision block | Inner end of affected block, toward commander's eye | 8 s; consumes one of four initial spare blocks |
| Jammed driver visor | Driver's actual visor control, held LMB; no toolkit required | 10 s; frees shutter |

`HydraulicFault` has modeled behavior, a repair route and a debug regression, but the current
source audit finds no ordinary combat producer assigning that particular enum. Normal strikes
can produce a ring jam/wreck or generic spall traverse damage. It is not evidence for the user's
unknown fault.

Default loader eye `(-0.85, 2.033, 0.15)` is about **1.844 m** from traverse anchor
`WHEEL2_POS = (0.912, 1.949, -0.386)`, outside the 1.65 m reach. Walking inside must bring the
crewman closer; the initial clamped walk position is approximately 1.643 m away, near the threshold.
That distance alone does not establish a clear source-triangle path. Reach/access for the exact
current interior pose still requires a player-view check. The ignored GPU equipment integration
test exercises several jobs from supplied eye positions; it is not proof that a player can walk
to every anchor in the current build.

## Separate exterior work

**Thrown track:** carry the kit outside to the loose track anchor, click to grab its end, hold LMB
and pull the mouse down repeatedly to bring it over the wheels, click to fit a spare link, then
land four discrete hammer clicks on the pin. One of six initial spare links is consumed when the
link is fitted. Progress survives leaving the flank. The original track timer and pin-hit count
remain separate from the new free-swing clock: ordinary hammer motion cannot advance track hits.
Destroyed/used-up spare links prevent fitting a new link.

**Armor-hole cover:** an existing separate outside interaction finds a reachable, unobstructed
hole. Holding LMB for 6 s applies a spare-track patch that restores only partial armor resistance.
It consumes one campaign `spare_parts` unit. Outside campaign state the existing fallback supplies
an effectively unlimited unit. This is distinct from the six physical track-link spares. The new
trigger motion also runs while patching; this leaves that separate work, resource consumption and
completion-sound path unchanged.

There is no generic full-health restoration. Wrecked turret bearings/ring, sheared gun mount,
smashed sight, wrecked MG and destroyed engine/fuel/ammunition modules have no field repair here.
A smashed cupola block mounting cannot be replaced like an opaque block. Road-wheel/structural
running-gear damage has no crew repair in this table. Fire suppression uses the separate physical
extinguisher and surface-coating system; hammering is not fire suppression.

Only vision-block swaps, track fitting and the armor-hole patch consume the implemented resources
above. Recoil-oil top-up, boresighting, barrel change and plug/fuse reseating are named timed jobs;
separate oil, barrel and fuse inventories, procedure-specific hand motions and tool contacts are
not implemented. The generic hammer currently represents their work animation.

## Persistence and evidence limits

Campaign restoration and LAN module snapshots preserve a working/broken mask and can reconstruct
a failed module as generic `Fault::Spall`, losing the finer original diagnosis. Exterior fault
state and consumable persistence must not be assumed from that mask. This is pre-existing; the
hammer-input change does not migrate save/network schemas.

Implementation: `src/equipment.rs`, the independent `repair_hammer_t` field/init/reset in
`src/game.rs`, and read-only recorder telemetry. CPU tests cover immediate use after pickup release,
empty-air and blocked-target motion with zero work/contact, repeat cadence, current-target contact
checks, no banked air work, and separation from the outside track input owner. Three focused hammer
tests pass on the final repair source tree. These are CPU input/contact-gating checks; they do not
exercise the rendered dot, armor-patch hand motion or Commander put-down in a player session.
Full integrated validation is reported separately in the immersion handoff.
`catalog/shader_reviews/toolkit_free_hammer_20260929.v1.json` stages real pickup, release, quick
clicks and a held swing on a healthy Tiger. Its final first-person film now proves that bounded
case below. Real-window interaction and access to a damaged traverse remain separate acceptance
work. No claim is made that the user's exact traverse fault was repaired.

The first healthy-toolkit film **failed** its acceptance checks: the fixture looked toward the
toolbox centre, but ordinary LMB picked up the extinguisher. Preserve
`captures/astra/toolkit_free_hammer_20260929_review_manifest.json` and
`artifacts/immersion-20260929/hammer.log` as evidence of that failure. The retained basket triangle
`Turret_0117` blocks the default loader's ray to the toolbox centre. The old extinguisher hover used
a broad cone and could select the off-centre bottle despite that rejected toolbox ray.

The pickup correction replaces that cone with the original bottle's exact source mesh, at the same
mounted pose used for drawing. Bottle, toolbox and Panzerfaust rack selection compare actual ray-hit
distances; the closest eligible surface wins, with 1.85 m reach and the existing source-steel LOS
gate. Review gaze for the toolbox and extinguisher now chooses an exposed triangle surface, returning
failure when none is reachable and clear. It only rotates the head: it does not move the crewman,
equip a tool, modify geometry or grant a pickup. At the default loader eye the CPU source check finds
an exposed toolbox surface 1.6708 m away and a bottle surface 1.4362 m away. That focused test also
checks source-ray hits, nearest-prop ordering, obstruction refusal and out-of-reach refusal; it passes.
The unchanged hammer fixture and both physical extinguisher pickup fixtures were then evaluated
on the fresh release binary. No forced equipment state, damage setup or relaxed assertion was used.

## Final pickup and free-swing evidence

The 5.5 s first-person film
[`toolkit_free_hammer_20260929_review_pickupfixed.mp4`](../captures/astra/toolkit_free_hammer_20260929_review_pickupfixed.mp4)
passes **19/19 state checks** and all required timeline actions. Its
[receipt](../captures/astra/toolkit_free_hammer_20260929_review_pickupfixed_manifest.json) records
the toolkit equipped after ordinary LMB pickup, the extinguisher remaining unequipped, hammer
motion at 2 s and 4 s, rest at 1 s, 3 s and the end, zero repair work and track-pin credit, no repair
target, and a living Tiger with healthy traverse throughout. The earlier mechanics-only evaluation
also passes 19/19; it omitted the optional v1 audio flag and is not the final audio receipt.

The final film uses the canonical asset root, shader High at native 640×360, 24 fps, 11 verified
optional audio layers and 24 feedback recordings. The encoded 1920×1080 video and stereo 44.1 kHz
audio both last 5.5 s; audio is sampled from ordinary gameplay sinks on the simulation clock.
Inspection of frames at 0.25, 1.0, 1.9583, 2.0833, 2.25 and 4.0417 s confirms the exposed toolbox
highlight before pickup and distinct rendered hammer rest, lowered and raised poses. See the
[contact sheet](../captures/astra/toolkit_free_hammer_20260929_review_pickupfixed_contact_sheet.png)
and the six adjacent `toolkit_free_hammer_20260929_review_pickupfixed_frame_01.png` through
`_06.png` files. These are sampled visual checks, not subjective sound approval or proof of
damaged-part repair, Commander put-down or armor-patch contact.

Both fire fixtures were re-evaluated with the same final asset/audio environment to check the
shared bottle pickup path. Their actual extinguisher pickup checks pass. The aimed fixture remains
an honest **failure** of its survival objective: surface fire is zero by 3 s, with 5,317 deposits
and 0.708770 coverage, increasing to 7,442 deposits and 0.827606 coverage at 4 s. An already ignited
cartridge still detonates at **4.2916665 s**. Nine of fourteen state checks pass; the terminal latch
also rejects the later required gaze action. The wrong-aim fixture passes **11/11 causal checks**:
at 3 s it has 4,870 deposits but zero burning-surface coverage and fire 1.4; at 4 s deposits persist
at 5,459. Detonation occurs at **4.2750001 s**. These fire/coating/deposit values and death times
match the prior receipts; the pickup correction did not turn suppression failure into success.
Each fatal-audio queue submits exactly one `CookOff` and ends empty. These last two runs are
evaluations, not replacement fire movies:

- [Aimed fire receipt](../captures/astra/fire_aimed_lmb_20260929_eval_pickupfixed_manifest.json).
- [Wrong-aim fire receipt](../captures/astra/fire_wrong_aim_lmb_20260929_eval_pickupfixed_manifest.json).
- [Combined raw outcome summary](../artifacts/immersion-20260929/pickup-final-outcomes.json), including
  all check results, sampled states, audio-bank provenance and rejection events.

The original failed hammer movie and receipt remain intact. The successful replacement proves
physical pickup and free hammer motion on a healthy tank; it does not diagnose the user's live
traverse fault or prove that the current loader can reach and repair its specific broken part.
