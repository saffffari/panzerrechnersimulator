# Armor, fragments and deformation: implementation plan

29 September 2026. Source audit of the playable `claude/sound-overhaul-20260928` worktree,
based on handoff `d7947c0`. This document is a proposed migration, not an implemented damage
solver or a claim of calibrated terminal ballistics. The concurrent sound pass changes sound
generation and limiter diagnostics; it does not implement the work below.

The owner's direction is to make penetrating hits, fragmentation and the Tiger's material
response more convincing while preserving the game's PSX presentation and physical authority.
Spend the available CPU/GPU budget on the state that produces the image and sound. Additional
visible fragments must not silently multiply available mass, kinetic energy, injury or heat.
`CLAUDE.md` and `PHYSICAL_TRUTH_AUDIT.md` remain the governing rules.

## Current implementation and the ledger corrections it needs

The current code is further along than two rows of the physical-truth ledger imply, but it
still contains substantial geometric and temporal approximations. Line numbers below refer
to this audit; symbols are the durable references as the large `game.rs` changes.

| Area | What the current source actually does | Remaining limit |
|---|---|---|
| Fragment identity and origin | `game.rs`: `HotFragment` (~1227), `generate_penetration_fragments` (~3480), `PenetrationSolution` (~6100), `resolve_player_penetration` (~25134). A penetration generates one deterministic body set with stable IDs. The upstream armor contact coordinate is preserved; the former `penetration_entry_point` projection is behind `#[cfg(any())]`. Damage forecasting and live display begin with copies of that set. | Preserving the coordinate does not make the upstream vehicle armor contact source-triangle geometry. Do not describe it as an exact plate fracture location. |
| Population and speed | `fragment_shower_profile` (~3410): ordinarily 220–768 cubes, 1.4–9 mm, speeds clamped within 7–220 m/s. Caliber, residual velocity, filler and `behind_armor` select the distribution. | This is an authored distribution, not a mass/energy-conserving fracture calculation. There is no separately continued residual penetrator body. Nonpenetrating events return no fragment population. |
| Fragment progression | `integrate_hot_fragment_traced` (~4014) and `drive_penetration_fx` (~27684): 480 Hz fixed steps with up to 18 mm translational subdivisions, bounded to 1,024 substeps per call. The physical cube and its measured path supply rendering and contact audio. | Contacts use a turret cylinder, hull box and expanded breech box. The center is clamped back to proxy surfaces. Collision is skipped before `entered_compartment` becomes true. The rotating rendered cube is not currently tested as that exact finite cube. |
| Damage timing | `resolve_fragment_damage` (~6326) forwards clones for up to 3.6 seconds immediately, records swept segments and intersects crew and modules. The live pool then advances its initial states separately. | One initial population is better than resampling a damage cone, but forecasting followed by later integration is not one online contact authority. Future hatch, recoil, turret and crew motion are not known to that forecast. |
| Crew and components | The forecast tests anatomical OBBs for occupied physical crew, a sphere in the older solo-body path, component spheres, and oriented boxes for actual live ammunition. Per-fragment body-part masks prevent repeated charging of one anatomical part across adjacent steps. | Crew hits at or above the existing 0.24 J threshold can kill; component resistance is simplified, soft-tissue transit does not remove energy, and module contacts change binary state. These are not calibrated injury or component-failure models. |
| Blood | `drive_cubic_blood` (~28050), `surface_blood.rs`, `surface_fire.rs`: emitted grains and supported contact film already query retained source triangles through `SourceContacts`. | The ledger's blanket claim that blood still uses only cabin proxies is stale. Grain transport is not a fluid solver. Current rendering also enlarges small grains to a pixel minimum; render size and contact size must be reported separately. |
| Armor damage | `armor_damage.rs`: `ArmorField::record`, `plate_at`, `patch`; `game.rs::drive_armor_scars` (~17359). Up to 24 quantized local scar records alter repeat-hit resistance, drive visible marks and patches, and enter snapshots. | A “hole” is a resistance reduction with a 4 mm minimum plus a drawn mark. It does not cut the hull mesh, contact triangles or RT scene. Evicting the oldest record at capacity also removes its resistance change. There is no plastic mesh deformation. |
| Fragment persistence and heat | The live cube set supplies the colored heat pages, an aggregate light source, measured trails and a bounded sample of impact sounds. | Active lifetime currently retires bodies; cold steel does not persist as a separate residue. Heat pages and emitter powers are authored, not temperature/emissivity solutions. Some dynamic fragments are still omitted from the RT scene, as the renderer ledger already records. |

In `PHYSICAL_TRUTH_AUDIT.md`, the **Armor / terminal ballistics** row still says entry geometry
is overwritten and damage sampled separately from visible fragments. Replace that description
with the narrower, current forecast/live split and proxy-contact debt when updating the ledger.
The **Crew / gore** row should distinguish source-contact blood from proxy-contact hot fragments.
`docs/research/SIMULATION_GAPS_20260928.md` §3.6 and its ledger note already flag much of this,
but its old line numbers and generalized wording need current-source verification.

## First prerequisite: damage from the bodies that are advancing now

Do this before presenting a retained-triangle conversion as complete. Merely passing the current
`SourceContacts` query to both the forecast and live integrator does not solve their temporal
disagreement. The forecast sees one hatch angle; a later live tick can see an open hatch. Freezing
the collision geometry to force agreement would make collision contradict the visible hatch.

Introduce a fixed-tick fragment update owned by the simulation, independent of rendering. The
existing pool can remain the initial storage. Each active body advances once, emits ordered
motion/contact records, and passes those same records to injury, module damage, ammunition
initiation, physical aftermath and audio. Rendering reads completed state; a second viewport
never advances it again.

The event record should retain at least:

- Penetration ID, fragment ID, simulation tick and within-tick contact time.
- Source frame and stable primitive/material identity, including the geometry revision.
- Incoming and outgoing pose/velocity, normal, contact point and the actual traveled segment.
- Target identity/body part where applicable, work transferred and residual fragment state.
- The declared geometric tolerance and any exhausted numerical-work budget.

Keep per-fragment/per-target deduplication for prolonged overlap, but do not discard a later hit
on a different anatomical region. Sort candidate contacts by physical time and stable IDs.
Commit a nearer steel contact before examining a target behind it. A ricochet splits the recorded
path at the contact; no damage ray or visible trail may connect its incoming and outgoing ends
through the obstacle.

`resolve_player_penetration` should stage the initial fracture event and fragment set, then
ordinary ticks should commit consequences when contact occurs. The initial strike sound can start
at impact; subsequent fragment contacts, ammunition initiation, fire and crew consequences must
follow their own events. Do not require a future casualty result to determine the earlier source.
God-mode diagnostics may observe the same events while explicitly suppressing target mutation;
their receipts must retain that distinction.

Keep the existing forecast as a read-only comparison/research tool during migration. On a frozen
fixture its event stream should agree with the online engine within declared discretization
bounds. It must cease to commit gameplay damage once online authority is enabled. In a moving
fixture, compare online events with the actual time-indexed scene, not with a frozen forecast.

## Retained finite-cube contact

Useful infrastructure already exists:

- `surface_fire.rs::CabinFire::new` (~614) copies triangles from the meshes submitted to the
  renderer. `game.rs` builds `fire_surfaces` around ~11113: hull, turret, gun, articulated hatches,
  breech block/lever, sight, shutters and other retained rig surfaces in 16 frames.
- `CabinFire::source_contacts` (~698) caches those frame transforms and inverses per query.
  `SourceContacts::world_triangles_into` (~399) performs bounded BVH overlap queries into reusable
  scratch storage. Failure clears the partial result and must not be interpreted as open space.
- `SourceContacts::sweep_batch` (~449) returns exact segment/triangle crossings, but those are
  center rays. They are not sufficient for a rotating 1.4–9 mm cube whose edge reaches a face
  before its center.
- `shell_collision.rs::sweep_triangle` (~87) already implements continuous translational
  OBB/triangle SAT, including edges and both sides of thin steel. `Body::advance` (~320) also
  contains rotational subdivision. This is a cartridge solver; extract a shared geometric
  narrow phase instead of importing its cartridge mass, 1 mm skin or response settings blindly.

The first source-contact implementation should use each fragment's actual cube dimensions and
rotation (`hot_fragment_render_pose`, ~2390). Query a conservative swept bound, including the
rotational envelope, then resolve the nearest finite-cube contact. Retain source frame/triangle
identity alongside vertices; the current world-triangle list alone loses that metadata. This
lets audio select the contacted material instead of guessing “breech/floor/armor” from proximity
to the old boxes (`hot_fragment_contact_material`, ~2640).

The triangle query must use a consistent tick snapshot for every body and target. For moving
rig pieces, static endpoint queries alone can miss a hatch sweeping into a chip; either solve
relative motion or subdivide the shared articulation with a documented surface-motion bound.
Transform normals with the appropriate inverse transpose, and keep event coordinates associated
with the contacted frame rather than assuming every collision is hull-local or turret-local.

Proposed initial numerical acceptance target: translational time of impact agrees with a double
precision reference to 0.1 mm over the tested scale/range, and maximum untested rotational
surface travel is at most the smaller of 0.1 mm and one tenth of the fragment's minimum dimension.
These are engineering targets to measure, not existing guarantees. The cartridge solver's 1 mm
skin is too large to reuse unquestioned for the smallest 1.4 mm fragment.

## Entry skin, inner face and a real perforation

Switching on source collision at the current external entry coordinate will frequently collide
with the very armor surface the penetration was declared to have defeated. Continuing to ignore
all geometry until the old proxy cabin is reached would retain the principal error, including
possible passage through a firewall or intervening machinery.

Separate two modeled events:

1. **Back-face spall release:** locate the inner material surface belonging to the struck plate,
   retain its outward-to-compartment normal, and release the corresponding plate fragments there.
   The position must come from plate geometry/thickness and the actual strike; it cannot be a
   fixed distance moved toward the camera or the old compartment boundary.
2. **Residual penetrator/plug passage:** a surviving core and displaced plug need mass, shape,
   velocity and a perforated path. Their collision cannot simply ignore intact steel. The same
   opening must exist for rendering, contacts, repeat shots and light transport.

There is a limited inner-surface precedent at `game.rs` ~23337: the pressure-front path casts
back from 0.40 m beyond the impact to 0.10 m before it, accepts hull/turret contacts within
0.45 m and checks normal orientation. That is useful evidence to inspect, not a generally valid
exit-face resolver: these fixed extents can select another face or miss thick/oblique structure.
Do not repurpose it without plate identity, thickness and source-triangle tests.

Extend upstream armor contacts to identify the plate, source primitive, local normal, thickness
and inside/outside surfaces. Where a source model is only a thin skin or contains disconnected
decoration, report that limitation and create a source-derived collision solid with a measured
bound before claiming a volumetric fracture. Test side, oblique front, turret and edge strikes,
including near a real hatch aperture and against closed/open breech geometry. Roof/belly armor
coverage and full three-dimensional obliquity remain separate terminal-ballistics work.

## Deformation must change one shared geometry revision

A visible dent alone is not deforming armor. Define a persistent local damage state from which
all affected geometry consumers obtain the same revision:

- Outer and inner surface displacement, remaining thickness and actual perforation boundary.
- Displaced/retained material, detached plug/chips and their mass/momentum/energy accounting.
- Raster vertices/indices, CPU contact BVHs and the GPU BLAS/exact-hit data used by lighting.
- Repeat-hit intersection/resistance, aperture transmission, blood/foam surface attachments,
  save/load and host-authoritative multiplayer state.

Start with a bounded local patch on one supported plate. A dent may move vertices with unchanged
topology; a perforation requires topology that exposes the opening and its edge. Preserve material
and source IDs or provide an explicit old-to-new attachment map. Rebuild/refit only affected BVH
branches and GPU geometry, but publish their revision together at a simulation boundary. Until a
revision is ready, keep the previous complete geometry/state consistently; never draw a new hole
while collision or shadows use the old surface.

Choose the deformation constitutive/fracture approximation from documented material inputs and
validation cases. The first patch may be a named reduced model; it must not be called FEM or
validated penetration mechanics merely because it produces convincing dents. Track an energy
ledger across residual projectile, fragments, deformation work and heat. Bound closure error in
tests before increasing fragment density. Current profile multipliers are not such a ledger.

Replace the 24-scar eviction behavior with persistent damage storage whose capacity policy cannot
heal old holes. Save a versioned damage representation and geometry checksum. Migrating existing
saves may preserve legacy resistance records as explicitly legacy state; do not infer unrecorded
fracture topology and silently claim it was simulated.

## Phased migration and evidence

| Phase | Concrete deliverable | Required evidence before enabling the next phase |
|---|---|---|
| 0 — Freeze the baseline | Versioned fixture inputs, current executable/source revision, deterministic initial fragment sets and event receipts. Record known proxy/forecast discrepancies. | Existing fixed-scene tests and first-person films remain available under their original names. New artifacts use collision-safe names; films/stills belong in flat `captures/astra/`. |
| 1 — One online authority | Move consequence application into the shared fixed-tick fragment progression while temporarily retaining the same contact approximation. Keep forecast comparison read-only. | Frozen-fixture online/forecast agreement; no early crew/module mutation; frame-partition, one-view/multiple-view and save/replay invariance; moving target hit/miss tests. Update the ledger to keep the proxy approximation visible. |
| 2 — Finite source contact fixture | Extract reusable cube/triangle SAT and metadata-preserving BVH queries. Prove them in isolated finite-body fixtures and the retained Tiger cabin. Resolve inner-face origin semantics. | Thin wall, edge graze, open aperture, rotating hatch, moving breech and high-speed no-tunneling cases; measured tolerance and query cost. A debug comparison may run beside gameplay but cannot commit a second damage result. |
| 3 — Source contact in live play | Replace fragment cylinder/box/breech contact, collision-free entry travel and floor projection. Route injury, sparks, trails and contact sound from the online event stream. | Same fragment visibly strikes the same surface that produces the contact; material IDs agree; no post-bounce chord damage; source geometry revision recorded; worst-case workload meets the measured CPU budget. |
| 4 — One plate's material failure | Add persistent dent/perforation geometry and a residual penetrator/plug with shared CPU/GPU geometry. | A repeated shot, finite chip, light/shadow ray and visible camera all agree on the opening. Mass/energy closure, save/load and replication tests pass. No old damage disappears at the former 24-scar limit. |
| 5 — Extend and calibrate | Additional plates and vehicle types, source-grounded fragment distributions, calibrated injury/module response and retained cooled fragments. | Primary-source/material validation, explicitly bounded approximation error, campaign performance and distinct mortal player-view encounters. |

Do not multiply blood or fragment counts as a substitute for phases 1–4. More blood can be a
presentation-density decision with a stated model, but damage and fluid quantity must not increase
merely because a preset can draw more particles. Cold fragments should eventually retain collision
and persistent residue after emissive cooling, with an explicit sleeping/aggregation policy rather
than silently expiring matter when its light fades.

## Work bounds and acceptance

The current worst nominal forecast is 768 × 480 × 3.6 = **1,327,104 fragment steps per hit**,
before velocity subdivisions. At the 220 m/s cap, an initial 480 Hz step spans about 0.458 m and
requires 26 of the current 18 mm subdivisions. These are upper-work estimates, not measured
runtime; many bodies settle sooner. A naïve 16-frame triangle query at every existing subdivision
could create a severe impact hitch on the i9-12900K even though the GPU has spare capacity.

Reuse frame inverses and scratch arrays. Query each body's conservative swept neighborhood once
where valid, cache candidates with geometry revision/coverage checks, and invalidate them when
articulated motion escapes the cached bound. Continuous translation collision should avoid the
need for spatial marching along clear straight segments. Profile before adding parallelism; if
parallel contact queries are used, merge their events in a stable order before mutations.

Record per tick and per penetration: active/settled bodies, fixed ticks, BVH nodes, candidate
triangles, SAT tests, contact iterations, peak scratch allocation, actual wall time and budget
exhaustions. Report p50/p95/p99 and the maximum impact frame separately from ordinary driving.
Measure both dense district and cabin fixtures on HAL; tests with no rendered frame do not establish
player-visible frame rate. The proposed first performance target is a 60 Hz 4090 session with a
16.7 ms total frame budget, with the fragment slice allocated only after an actual baseline
profile. Do not invent a spare-millisecond allowance from the hardware specification.

Numerical-work exhaustion must never mean “no collision.” An unfinished tick must remain pending
and visibly diagnosed; all consumers must wait for its committed state. Dropping time, letting
fragments pass through steel or giving Laptop different damage is not a valid performance fix.
Test bounded recovery from an exhausted budget and show that it converges to the unbounded
reference event stream. A proposed runtime freeze/catch-up policy itself needs a user-visible
stress test before shipping.

Useful existing tests to preserve or extend (verify `#[cfg]` guards before treating old tests as
live coverage):

- `game::surface_fire::tests::segment_bvh_shared_edge_ties_keep_lowest_source_triangle_normal`.
- `game::surface_fire::tests::body_bvh_selects_one_triangle_from_dense_source_without_scanning_the_mesh`.
- `game::surface_fire::tests::body_source_query_follows_independently_articulated_faces_and_filters_world_bounds`.
- `game::surface_fire::tests::body_source_query_budgets_fail_closed_and_reuse_the_result_buffer`.
- `game::shell_collision::tests::narrow_breech_aperture_rejects_finite_width_but_accepts_clear_gap`.
- `game::tests::all_roof_hatches_block_closed_descent_and_accept_only_their_open_source_apertures`.
- `game::tests::hot_fragment_audio_uses_moving_compartment_and_live_head_frame` and
  `hot_fragment_whizz_comes_from_a_real_swept_listener_near_miss`.
- `armor_damage::tests::penetration_creates_a_local_repeat_hit_weakness`.

New acceptance fixtures must include an opening hatch after penetration, a traversing turret,
gun recoil, two penetrations within the same fixed tick, a chip hitting a thin edge before its
center reaches the plane, and a crew volume behind that edge. Include surviving wounds and
nonpenetrating strikes as future calibrated behaviors; do not manufacture those outcomes in
capture scripts to demonstrate a solver that does not yet produce them.

For the owner, finish each enabled phase with synchronized first-person combat: a nonpenetrating
street ambush, a confined close-range penetration, and an ammunition/fire consequence. Keep the
initial conditions and staged actions disclosed. Compare identical inputs where deterministic
comparison is intended, and retain failed receipts. A successful build, a larger particle count
or an attractive slow-motion diagram is not evidence that the live contact chain is correct.
