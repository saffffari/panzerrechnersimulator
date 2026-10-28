# Armor scars: permanent dents and holes on every vehicle

Branch `armor-scars` (off `raymarching` 8cf48cf), 2026-10-27. Every strike on armor leaves a
permanent mark that the physics, the save data and the picture read from one record per wound.
The approximations are named in `docs/PHYSICAL_TRUTH_AUDIT.md` ("Armor scars, 2026-10-27").

## One record per wound (`src/armor_damage.rs`)

`ArmorScar` (32 bytes packed) holds, in the hull or turret frame selected by its facing:

| field | meaning |
|---|---|
| `local_mm` | strike point on the original surface, millimetres |
| `normal`, `dir` | the struck surface's outward normal and the shell's path (1/127 quantized) |
| `radius_mm` | wound radius: crater plus displaced metal and stripped paint |
| `depth_dmm` | crater depth (tenths of a mm) |
| `loss_mm` | resistance lost at the centre (`plate_at`); for a crater this *is* the cavity |
| `plate_mm` | plate thickness at the strike (bore length) |
| `kind` | Dent, Hole, Patched, Pock (bullet/fragment; no resistance change) |
| `class` | AP, APCR, HE, HEAT, bullet |
| `aperture_mm`, `back_radius_mm`, `petals`, `petal_mm`, `plug` | terminal geometry |

The terminal geometry is filled today by `terminal_geometry`, an empirical rule from the ballistic
state (calibre, plate, obliquity, residual speed). It is plain data so the proposed voxel or
particle plate solver can write measured aperture, scab, petal and plug values into the same
record; renderer, ballistics and saves follow without change.

Crater depth of a non-perforating kinetic strike: a share of the round's penetration capability
along the normal, scaled by how close it came (pen / effective): 25 % for a bounce or ricochet,
15 % for a shatter, capped at 45 % of the plate. The same number is the resistance loss, so the
crater the eye sees is the plate the next round meets. (The previous energy-only rule gave a
76 mm bounce off the Tiger's front a 2 mm crater; this gives 14 mm.)

Capacity: `MAX_ARMOR_SCARS = 512` per vehicle (was 24). Eviction drops pocks first, then the
smallest dents, never an open hole or a patch while a lesser mark remains. A crater swallows the
pocks under it; repeat hits in one wound merge into it.

Scope: the player's Tiger keeps its field on `Game::armor_scars` (campaign-saved); every other
`Tank` carries `Tank::scars` (enemies, wingmen); `CampaignHulk` copies the scars of the tank it
was, so a wreck carried to the next stage keeps its wounds. Cannon rounds now strike wrecks whose
turret is still seated (before, every shell flew through a dead tank); a popped turret's contact
frame is its old seat, so those are still excluded. Bullets mark every vehicle (`record_pock`).
Spall fragments mark the Tiger's interior (`mark_fragment_strikes`, below).

Persistence: campaign checkpoint version 2 stores the field as one packed hex string; version-1
files (object list, centimetre positions) still load (`ArmorScar::from_v1`). A 512-scar field
is under 64 KB of checkpoint. LAN protocol 8 streams a rotating window of 16 packed scars per
world packet (newest window every other packet, the rest walking the field), and the client
rebuilds the field with `ArmorField::apply_window`; the worst-case world packet stays under the
2048-byte limit. Strike times and rim temperatures are runtime only: a loaded scar is cold and
treated as a day old (rusted, grimed, snow in its cavity in winter).

## Drawing (raymarch backend: `src/renderer_scars.rs`, `shaders/rm_scars.wgsl`)

The game hands each vehicle mesh its wounds in the mesh's own frame (`Renderer::set_mesh_scars`;
hull scars to the hull mesh, turret scars to the turret mesh, which follows a popped turret's
flight). Each frame the backend packs them into world space (`GpuScar`, 144 bytes, up to 4096
per frame) and gives each wounded mesh its range in the push constant.

**Vertex density found.** The baked meshes are dense overall and coarse where it matters. Rays
cast onto the outer plates (`density2.py` probe, 45 rays per mesh):

| mesh | vertices | median outer-plate triangle edge | plate vertices within 10 cm (median) |
|---|---|---|---|
| Tiger hull | 3,352,932 | 44.6 cm (max 5.4 m) | 36 (min 0) |
| Tiger turret | 366,481 | 27.4 cm (max 1.9 m) | 18 (min 0) |
| T-34 hull | 179,943 | 2.23 m (max 6.7 m) | 0 |
| T-34 turret | 14,982 | 74.7 cm | 3 |

A dent of 5-20 cm radius therefore falls on zero to a few vertices: displacing the vehicle's own
vertex buffer cannot show it. The near path is **local densification** instead: a dense patch per
wound (48 segments x 43 rings, 2,064 quads, a few millimetres between vertices) generated in the
vertex shader from the record (no vertex buffer, one instanced draw for all near wounds), while
the vehicle's coarse plate is cut away under it. That replaces the brief's compute pass over the
vehicle buffer: it is per instance (every T-34 shares one mesh), needs no per-vehicle buffer copy,
and its displacement is real geometry in the depth buffer.

Per wound and per pixel:

- **Far and mid** (projected radius under 3 internal pixels, or beyond 70 m): the wounded mesh is
  drawn with `fs_scar`. Inside the crater the view ray is marched through the height profile
  (parallax occlusion, 16 steps + 5 bisections) and the fragment writes the crater floor's own
  normal, material and depth, so contact shadows, AO and reflections see the real cavity. Around
  it the paint is stripped (ragged flake edge) and scorched (a multiplier on the vehicle's own
  albedo, so its wear and grime stay).
- **Near**: the patch draws the displaced crater face, the oblique elliptical bore down the
  shell's path, the back-face scab cone, petals bent along the residual path for thin plate, and
  the punched plug where it came to rest. The plate is cut only inside the patch's footprint, and
  that footprint is clamped per sector to where the vehicle's real surface extends (`scar_extent`
  probes the source triangles once per wound), so a wound at a plate's edge never extends into
  the air.
- **Holes are holes**: fragments of the plate inside the bore and scab are discarded in the
  G-buffer and in every depth map (sun cascades, sky cover, point-light faces), so the key beam,
  lamps and the eye pass through exactly the same opening. At mid and far range the bore wall is
  ray-traced analytically inside the cut (oblique elliptical cylinder) and written with its depth.
  Behind a hole the Tiger's modeled interior is seen; a T-34 has no modeled interior, so the inner
  faces of its own hull shell are what shows (named).

The profile: a parabolic bowl of depth D out to R with a crisp edge, a ragged lip of displaced
metal (22 % of D; 5 % for HE, which dishes rather than digs), a hole's entrance funnel down to
its aperture, HE fragment pits (hash-placed 2-6 mm craters thinning out to 2.6 R), elongation
along the shot by 1/cos(obliquity). Materials: rough, dull fresh steel (darker where the cap
smeared the floor), a faint temper-colour oxide ring, dark mill scale in the stripped band,
torn bore and crystalline scab. Weathering by strike age: rust bloom (first order, 6 h), grime in
the cavity (1 h), and snow drifting into sky-facing cavities while it snows. Fresh rims glow
with the same thin-ring heat estimate and cooling law as the incandescent rim grains
(`rim_heat`, 2.5 s), as Planck radiance on the same scale.

Light leaks: an open hole into the player's Tiger registers a small emitter just inside its exit
carrying the outside sky radiance along the plate normal times the aperture area (W/sr); the key
beam needs none (the hole is cut out of the depth maps). Cabin lamps are unshadowed clustered
lights in this backend (except the two ranked casters), so no outward emitter is added.

Interior consequences: the back face of a hole shows the scab or petals bent down the same
residual path the crew solver's spall cone uses; each fast first strike of a spall fragment on
the bounded cabin collision walls is traced back to the cabin's own surface behind it and leaves
a pock there. The field patch's spare track shoes now sit on the crater lip, square to the real
plate normal, long axis across the bore (they used to float on the facing's nominal axis).

The shader and physical backends keep the previous rigid marks on the player's Tiger only (the
24 most significant wounds) plus the track patch.

## Verify: `armor-scars PREFIX`

Run from the main checkout with the branch's binary (assets are shared), with the usual capture
environment (`KESSEL_RENDERER=raymarch`, high preset, 800x450, Canyon v2 manifest):
`panzerrechnersystem.exe armor-scars captures/armor_scars_<date>`. `KESSEL_SCAR_SCENES=a,b` renders
only stills whose names start with those prefixes; `KESSEL_SCAR_TRACE=1` prints every terminal
contact and each wound's probed surface extent; `KESSEL_RM_SHADER_DIR` reads live shaders from a
worktree while assets come from another root.
The scenario (`src/armor_scars_review.rs`) stages the Tiger and a live T-34 on the Canyon v2
workshop, fires the canonical set through the ordinary flight sweep and terminal resolver (each
round starts 9 m short with the speed, flown distance and age of its nominal range):

At the Tiger: 76 mm BR-350B bounce at 500 m (front), a 24-round MG burst (front), 76 mm OF-350 HE
(left side), 88 mm PzGr.39 at 1490 m (left, engine compartment), 76 mm BR-354P APCR at 200 m
(turret rear). At the T-34: the same classes from the Tiger (glacis bounce, turret MG burst, 88 mm
HE side, PzGr.40 APCR turret, PzGr.39 side). The Tiger is staged with its racks, breech and loose
rounds emptied: with stowed rounds any of these penetrations detonates them.

Stills (near, mid, far; day and night; fresh 0.1 s / 1.1 s / 4 s; inside the Tiger; patched;
after a checkpoint save and load) and `PREFIX_receipt.json` (every shot's terminal and resulting
record, the final fields, the checkpoint round trip, per-frame cost with and without wounds).
The save/load round trip writes a real checkpoint file with the game's encoder and reads it back
through the decoder; the unit test `hundreds_of_armor_scars_round_trip_and_version_one_files_still_load`
does the same for a full 512-scar field and a version-1 file.

## Results (2026-10-27, `captures/armor_scars_20261027_*` in the main checkout)

Every canonical round left its mark through the ordinary terminal path (receipt `shots`):

| target | round | terminal | record |
|---|---|---|---|
| Tiger front (100 mm) | 76 mm BR-350B, 500 m, 33 deg | bounce | dent R 91 mm, 14.2 mm deep (= loss) |
| Tiger front | 24-round MG burst | inert | 26 pocks, ~1 mm |
| Tiger left lower side | 76 mm OF-350 | HE blast | dish R 168 mm, 5.5 mm |
| Tiger sponson chamfer | 88 mm PzGr.39, 1490 m, 37 deg | penetration | hole, aperture 46 mm, scab 106 mm, plug |
| Tiger turret rear | 76 mm BR-354P, 200 m | penetration | core hole 15 mm, scab 35 mm, plug |
| T-34 glacis | 76 mm BR-350B, 500 m | bounce | dent R 91 mm, 10.7 mm deep |
| T-34 turret | 24-round MG burst | inert | 25 pocks |
| T-34 upper side | 88 mm SprGr | HE blast | dish R 194 mm, 7.3 mm (cut off at the fender edge) |
| T-34 turret side | 88 mm PzGr.40, 200 m | penetration | core hole 18 mm, scab 49 mm, plug |
| T-34 side (wreck) | 88 mm PzGr.39, 1490 m | penetration | hole 46 mm, 45 mm plate under 0.6 calibre: petalled |

Spall from the Tiger's two penetrations left 55 pocks on its cabin surfaces. The field patch's
shoes sit on the sponson chamfer over the 88 mm hole; after a checkpoint written to disk and read
back the field is identical (all 84 Tiger records) and redrawn cold and weathered.

Measured cost (`KESSEL_GPU_PROFILE=1`, same view, 113 records, 4 near patches, 800x450; the
GPU was shared with other sessions' captures at ~97 % load, so these are pass-to-pass
comparisons only): G-buffer 8.79 ms with wounds vs 8.85 ms without, far cascade 5.43 vs 5.82,
point-light faces 0.84 vs 0.65 (the hole-cutting depth program on wounded meshes, +0.2 ms). The
wound shading is below the noise of this measurement; the point faces are the one consistent
cost. A clean solo figure is owed.

## Open items

- The dead player Tiger and popped enemy turrets still let shells pass (pre-existing contact
  rules; the player one needs a decision about re-running penetration effects on a dead tank).
- LAN streams only the player Tiger's field; enemy and wingman wounds are host-local.
- The shader and physical backends draw only the player's 24 most significant wounds as rigid
  marks; no cut or displacement there.
- The interior still from the loader's station does not show the 15 mm APCR core hole (turret
  traverse gear in the way); a clear interior framing of a hull hole into the fighting
  compartment is owed. Cabin fire and spall heat haze dominate the compartment for seconds after
  a penetration, which is truthful but hides the hole.
- Patch on curved castings is planar (sagitta error, see the ledger); the extent probe clamps it
  at plate edges but not to curvature.
- Terminal geometry and crater depth are empirical rules awaiting the voxel/particle plate
  solver, which can write straight into `ArmorScar`.
- Snow, rust and grime rates are authored; a loaded scar is treated as a day old regardless of
  the campaign clock.
- Six `campaign_save` file-system tests fail on Windows before and after this branch (directory
  fsync via `File::open`, Unix-style absolute paths); the encode/decode round trip tests pass.
- Review staging: the Tiger's racks are emptied (any of these penetrations otherwise detonates
  them) and the cabin fire is put out before the turret-rear shot; both are stated in the receipt.
