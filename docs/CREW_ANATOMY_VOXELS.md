# Crew anatomy voxels (one crewman, 2026-11-02)

A full interior anatomical voxel model of one Tiger crewman, with a physical-property table per
tissue and an offline slice editor. It is an offline asset and a set of tools. This change adds
no runtime behaviour, so `docs/PHYSICAL_TRUTH_AUDIT.md` is not touched here. (The FEA branch,
`claude/fea-damage-20261102`, reads the volume in `src/crew_anatomy.rs` as a preliminary
integration. Its ledger entry belongs to that change.)

## What it is

| | |
|---|---|
| Figure | AdrielCZ Panzertruppe, face01, relaxed pose: the figure `crew_tissue` voxelises (`CREW_RELAXED_GLBS[0]`, CC-BY-4.0). All five identities share its body geometry. |
| Frame | The relaxed figure frame: metres, +Y up, floor at Y = 0, +Z is the face, **+X is the figure's anatomical left**. This is the frame `CrewBodyPart::rest_center` uses. |
| Grid | 70 × 156 × 32 voxels of 10 mm, origin (−0.35, −0.01, −0.14) m. 55,851 voxels are inside the body (55.85 L). |
| Channels | `label` (u8 tissue id, 30 labels) and `bone` (u8 index of the dominant rig joint; 255 outside the body) |
| Volume | `assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/anatomy/german_panzertruppe_face01_relaxed_anatomy.kvox` under the asset root (724,096 bytes, untracked) |
| Receipt | `catalog/crew_anatomy_volume.v1.json`: file and channel SHA-256s, masses, checks and tuning (tracked) |
| Table | `catalog/crew_anatomy_tissues.v1.json`: per label density + source, strength class, blood fraction + derivation, colour, ICRP reference |

Labels (ids are permanent; the game reads 1 = outer shell, 3 = skeletal muscle, 4 = cortical bone):
0 exterior, 1 skin, 2 subcutaneous fat, 3 muscle, 4 cortical bone, 5 spongiosa, 6 yellow
marrow, 7 cartilage, 8 brain, 9 CSF, 10 spinal cord, 11 eye, 12 lung, 13 airway (air), 14 heart
wall, 15 heart blood, 16 vessel blood, 17 liver, 18 kidney, 19 spleen, 20 pancreas, 21 stomach
wall, 22 stomach contents, 23 small intestine, 24 colon wall, 25 colon contents, 26 bladder wall,
27 urine, 28 oesophagus, 29 visceral fat.

## Tools

| File | Purpose |
|---|---|
| `tools/anatomy/build_anatomy.py` | Deterministic generator. `--verify` rebuilds and compares with the receipt. `--capture-edits FILE` stores hand edits as an edit layer. `--report FILE` gives masses and checks for any volume. |
| `tools/anatomy/kvox.py` | KVOX reader/writer (format below) |
| `tools/anatomy/anatomy_glb.py` | numpy-only glTF reader (data only; safe on unreviewed files with `python3 -I`) |
| `tools/anatomy/voxel_editor.html` | Offline slice editor (open the file in a browser) |
| `tools/anatomy/render_anatomy_previews.py` | Slice montages and voxel cutaways (PNG) |
| `tools/anatomy/test_crew_anatomy.py` | Format, editor round trip (Node), edit layer, table and built-volume tests |

```sh
export KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem      # the directory holding assets/
python3 -I tools/anatomy/build_anatomy.py                        # build (about a minute)
python3 -I tools/anatomy/build_anatomy.py --verify               # rebuild, compare with the receipt
python3 -I tools/anatomy/test_crew_anatomy.py                    # tests
python3 -I tools/anatomy/render_anatomy_previews.py VOLUME.kvox /home/alex/panzer/captures/crew_anatomy_<date>_<what>
```

### Editing by hand

Open `tools/anatomy/voxel_editor.html` in a browser, then load the `.kvox` and the tissue table
with the buttons or by dropping them on the page.

- **Views:** axial, coronal and sagittal, as three views or one. The mouse wheel or the arrow
  keys move the slice.
- **Paint:** left drag paints, right drag (or Shift) erases to a chosen label, and Alt-click
  picks a label. The brush is a disc in the slice plane or a ball in 3-D. "Only over" limits a
  stroke to one label, or to the body.
- **Legend:** per-label visibility, voxel counts and volumes. Colour by tissue or by rig bone.
- **Undo:** per stroke.
- **Save:** writes the same format and appends an `edits` record to the header. Chrome and Edge
  save back to the opened file; other browsers download it.

To keep hand edits through a rebuild, read them back into the build:

```sh
python3 -I tools/anatomy/build_anatomy.py --capture-edits edited.kvox   # writes catalog/crew_anatomy_edits.v1.json
python3 -I tools/anatomy/build_anatomy.py                               # applies the layer on top of the procedural base
```

The edit layer records the procedural base's label SHA-256. If the generator changes the base,
the receipt flags the layer as stale, but it is still applied voxel by voxel.

## Format (KVOX v1)

| Bytes | Content |
|---|---|
| 0 to 7 | `KVOXANAT` |
| 8 to 11 | u32 version 1 |
| 12 to 15 | u32 header length N |
| 16 to 16+N | UTF-8 JSON header, space-padded so the payloads start 8-byte aligned |
| then | one u8 payload per header channel, in order, of nx·ny·nz bytes |

Voxel (x, y, z) sits at index `(y·nz + z)·nx + x`, the same order as `crew_tissue`'s grid. Its
centre is `origin_m + (i + 0.5, j + 0.5, k + 0.5)·voxel_size_m`. Integers are little-endian.

The header carries:

- `dims`, `voxel_size_m`, `origin_m`, `channels` (each with `payload_sha256`), `frame`.
- `labels`, a compact copy of the table's ids, keys and colours.
- `rig`: 39 joints with parent, relaxed head position, relaxed world matrix and the game part
  its bone midpoint classifies to; plus the skinning check.
- `figure`, `landmarks`, `provenance`, `edits`.

## How it is built

1. **Inputs.** The figure GLB, the authoring rig and the bake report are verified against the
   bake receipt's SHA-256s. The rig is posed with the bake's recorded relaxed rotations and
   re-centred the way the bake did. Linear-blend skinning of the rig then reproduces the baked
   figure to 6e-7 m. All four pieces must be skinned with every influence set; the hands carry
   more than four.
2. **Outer body.** The figure is four open, overlapping shells: body, head, two arms, with 8, 6,
   4 and 4 boundary edges. A voxel is inside if any piece's generalized winding number exceeds
   ½. This is the test `crew_skeleton::inside` uses. Openings are closed implicitly by the ½
   threshold; nothing was re-meshed.
3. **Outer surface for depth.** Triangles whose two sides are both inside the union are dropped:
   buried arm ends, nested shells. Partly buried triangles are split at their midpoints up to
   five times, and only their exterior parts are kept: 1,887 pieces. Depth is the exact distance
   to these.
4. **Layers.**
   - **Skin:** the closed one-voxel shell of the inside mask.
   - **Subcutaneous fat:** voxels nearer the surface than a per-region authored depth. Almost none
     on the front of the chest; one voxel on the abdomen, flanks, back, buttocks, thighs and upper
     arms; none on the head, hands or feet.
   - **Muscle:** fills the rest.
5. **Skeleton** (rig-relative, authored proportions; bones are painted before organs, which
   avoid them):
   - **Spine.** Vertebral levels are set from the figure's own landmarks: L5/S1 0.090·H above
     the crotch, C7/T1 0.053·H above the shoulder joints, C1 3 cm below the eyes. Thoracic and
     lumbar lengths are in a 61 : 39 ratio. Each vertebra has a body (spongiosa), a disc
     (cartilage), an arch with spinous and transverse processes (cortical) and a canal.
   - **Sacrum and pelvis.** A tapering sacrum; iliac wings as curved plates from the sacroiliac
     joints to the anterior superior spines; acetabula round the femoral heads; pubic and
     ischial rami.
   - **Rib cage.** Twelve ribs per side run from their vertebra in the chest wall's rib layer,
     descending forward. Costal cartilages join a sternum; the spaces between ribs are
     intercostal muscle. Clavicles and scapulae complete the girdle.
   - **Limbs.** Long bones run along the relaxed rig's segments, with cortical shafts, a
     yellow-marrow core line and spongiosa ends. Each femoral head sits 6.5 cm (0.0425·H) above
     the crotch, because the rig's hip joints are at crotch height. Patellae, radius and ulna,
     fibula, carpals, metacarpals and phalanges along the rig's finger chains, and tarsals and
     metatarsals lifted above the boot soles complete them.
   - **Skull** (the owner's method). The scalp is the skin shell, the next voxel shell is the
     skull, and the inside is brain. A skull-base plate is left open at the foramen magnum. The
     facial skeleton is the same second shell under the face, but not under the jaw. Teeth are
     one dental arch. Any brain voxel left touching non-cranial tissue becomes skull: 8 voxels.
6. **Organs.**
   - **Targets.** Each is the ICRP 89 adult-male reference mass × k, where k = figure volume ÷
     reference body volume = 55.85 / 70.11 = 0.797. At a ragdoll scale of 1.079 the figure would
     be the reference man.
   - **Shapes.**
     - Ellipsoids: kidneys, spleen, bladder, heart, eyes.
     - Tubes along authored paths: stomach, pancreas, colon, duodenum.
     - Bisection on the shape's scale reaches the target voxel count, keeping one connected piece.
     - Liver and small intestine are ranked fills of their regions, grown as one connected piece.
   - **Cavities.** The thoracic cavity lies inside the rib layer, above two diaphragm domes and
     the central tendon. Lungs fill what the mediastinum, heart and vessels leave. The abdominal
     cavity lies inside a skin, fat and muscle wall, in front of the spine, psoas and quadratus,
     above the pelvic floor; what organs leave is visceral fat.
   - **Order.** Kidneys, spleen, pancreas, stomach, colon, liver, bladder, duodenum, small
     intestine. The abdominal aorta and vena cava are reserved first.
7. **Thin structures.** A tube plus a face-connected centre line keeps each one continuous:
   - **Airway:** nasal cavity, pharynx, larynx and trachea (with a cartilage wall), main bronchi.
   - **Oesophagus.**
   - **Vessels:** aorta (rising in front of the trachea, passing to its left, behind the left
     bronchus), venae cavae, pulmonary trunk, carotids, jugulars, subclavian to forearm trunks,
     iliac to posterior tibial trunks.
   - **Spinal cord** to L1/L2, with CSF below it, and CSF ventricles in the brain.
8. **Rig binding.** Each voxel takes the nearest bone segment of the relaxed rig, among the
   bones of the figure piece(s) that hold it. At the skin it agrees with the mesh's own dominant
   skin weight for 67% of voxels; the rest are at joints.

### Checks (receipt `checks`, all automated)

- No non-skin voxel touches the exterior: the skin shell is closed.
- No organ voxel touches the skin. No lung voxel touches skin or subcutaneous fat. No brain voxel
  touches non-cranial tissue.
- 1,270 bone voxels touch skin. These are subcutaneous bone surfaces: skull vault, tibiae, hands,
  feet, iliac crests. Bones never replace skin.
- Every inside voxel has a rig bone.
- Connected pieces:
  - Brain 1, kidneys 2, eyes 2, spinal cord 1, oesophagus 1, pancreas 1, spleen 1.
  - Heart, stomach, colon and bladder: 1 each (wall + contents).
  - Lungs 2 (+1 stray voxel).
  - Airway 1 tract from the nose to the bronchi; the maxillary sinuses are separate.
  - Vessel blood: 2 pieces. The right leg trunk is cut where it passes through the skin shell
    behind the knee (2 voxels); its lower 26 voxels are separate.

## Mass against reference

ICRP 89 adult male (73 kg, 1.76 m) and the same × k = 0.797 (this figure's size). The ratio is
voxel mass ÷ scaled reference. Voxel mass is voxel count × 1 cm³ × the table's density.

| Organ / tissue | Voxel kg | ICRP 89 kg | × k kg | Ratio |
|---|---:|---:|---:|---:|
| Brain | 1.541 | 1.45 | 1.155 | 1.33 |
| Eyes (both) | 0.012 | 0.015 | 0.012 | 1.01 |
| Spinal cord | 0.051 | 0.03 | 0.024 | 2.11 |
| Lungs (with blood) | 0.852 | 1.2 | 0.956 | 0.89 |
| Heart wall | 0.379 | 0.33 | 0.263 | 1.44 |
| Heart contents (blood) | 0.193 | 0.51 | 0.406 | 0.48 |
| Heart, wall + contents | 0.573 | 0.84 | 0.669 | 0.86 |
| Liver | 1.240 | 1.8 | 1.434 | 0.86 |
| Kidneys (both) | 0.247 | 0.31 | 0.247 | 1.00 |
| Spleen | 0.120 | 0.15 | 0.119 | 1.00 |
| Pancreas | 0.112 | 0.14 | 0.112 | 1.00 |
| Stomach, wall + contents | 0.322 | 0.40 | 0.319 | 1.01 |
| Small intestine, wall + contents | 0.757 | 1.0 | 0.797 | 0.95 |
| Colon, wall + contents | 0.477 | 0.67 | 0.534 | 0.89 |
| Urinary bladder, wall + urine | 0.204 | 0.25 | 0.199 | 1.02 |
| Oesophagus | 0.045 | 0.04 | 0.032 | 1.40 |
| Skin (outer shell) | 17.672 | 3.3 | 2.629 | 6.72 |
| Adipose, subcutaneous + visceral (vs separable adipose) | 6.870 | 14.5 | 11.551 | 0.59 |
| Skeletal muscle | 21.182 | 29.0 | 23.102 | 0.92 |
| Cortical bone | 5.856 | 4.4 | 3.505 | 1.67 |
| Spongiosa (trabecular bone + red marrow) | 1.213 | 2.27 | 1.808 | 0.67 |
| Yellow marrow | 0.133 | 2.48 | 1.976 | 0.07 |
| Cartilage | 0.328 | 1.1 | 0.876 | 0.37 |
| Skeleton (bone, marrow, cartilage) | 7.530 | 10.25 | 8.165 | 0.92 |
| Whole body | 60.645 | 73.0 | 58.153 | 1.04 |

Wall and contents individually, from the receipt:

| Hollow organ | Wall (× scaled) | Contents (× scaled) |
|---|---:|---:|
| Stomach | 1.89 | 0.48 |
| Colon | 1.30 | 0.39 |
| Bladder | 3.16 | 0.49 |

The walls are one voxel thick: see the approximations.

Blood is the sum over voxels of volume × the table's blood fraction: **4.15 L**. That compares
with the ICRP 89 5.3 L × k = 4.22 L. The crew blood system authors 6 L; scale by 6 / 4.15 if the
two must agree. Explicit blood voxels (heart chambers, vessel trunks) hold 0.95 L.

**Reading the outliers.**

- The brain is 1.07× the *unscaled* reference. The figure's head is adult-sized on a 0.8-scale
  body, and the skull method (two voxel shells) decides the brain's size.
- The skin, walls, cord and oesophagus are thin structures widened to whole voxels.
- The heart and lungs fall short because the figure's chest is flat: 16 cm deep where a real
  chest scaled to this figure would be about 20 cm.
- The liver and colon fall short because the upper abdomen under the domes cannot hold both at
  full size together with the stomach.

## Integrating with `crew_tissue`

The FEA branch already does the basic mapping (`src/crew_anatomy.rs`). Each non-gas voxel
becomes a cell with its tissue's density and its strength class's `TEAR_J_M3` and
`CRITICAL_STRETCH`; fluids are held at a quarter of skin's tear energy. What it still needs or
should know:

**Cell size and authored energies.** `crew_tissue` treats tear energy as per volume, so a 1 cm
cell carries 1/8 of a 2 cm cell's energy, and bond stiffness follows from it. Whether tearing
should scale with area (fracture energy) instead is the owner's choice; the table records both
the J/m³ and the per-2-cm-cell values.

**Cost at 1 cm.**

| Quantity | Value |
|---|---|
| Cells | about 55,800 per body (the 73 air voxels are not cells), about 8× the 2 cm build |
| Bonds | 9 owned per cell, about 500,000 |
| Memory | a few MB per body for cells, about 10 MB for bonds and CSR, plus per-casualty bitsets of 56 k and 500 k |
| Exposed cells drawn on an intact body | 15,935 skin cubes. Spread over the twelve skin-colour pages that is about 1,300 per page, under the `CUBES_PER_LABEL` bound of 8,192, but one colour page holding most of the skin would overflow. |

The lattice's stable time step scales with the cell size: dt ∝ √(m/k), with m ∝ h³ and k ∝ h.
A solve over the same 12 cm active reach is therefore about 16× the work of the 2 cm build: 8×
the cells and 2× the steps. Loading the volume removes the build-time winding-number and
closest-point work, which takes about 40 s offline at 1 cm.

**Posed and ragdolled bodies.** The volume is in the relaxed pose. For the eleven-segment ragdoll,
each cell needs a segment. Today cells take the figure's part classification
(`classify_relaxed_mesh_point`); the volume's `bone` channel offers a finer binding.

- Map each rig joint to a crew part: the header gives `crew_part_by_game_classifier` per joint.
- The two schemes put 23% of voxels in different parts, all at segment boundaries:
  - Hip: the rig's hip joints sit at the crotch, while the classifier's pelvis/thigh line is
    6 cm lower.
  - Shoulder girdle and neck.
  - Only 269 torso-bound voxels are called arm by the classifier's |x| > 0.155 rule.
- Seated or standing stations need the 39-bone rig:
  - Rigid per bone: p_local = inverse(`matrix_relaxed[j]`)·p, then posed = M_pose[j]·p_local.
  - Or linear-blend skinning, with weights interpolated from the figure's nearest vertices; the
    authoring rig carries them, and the bake report's pose rotations give M_pose.
  - Either way, blend cells near joints as `TissueModel::blend` does now, so joints stay closed.
- **Sides:** the game's `Left*` parts are the −X side, which is the figure's anatomical *right*.
  The anatomy is anatomical: the heart, stomach and spleen are on +X, the liver on −X. Keep this
  in mind before naming a wound "left".

**Blood coupling.** Every label has `blood_volume_fraction`.

- Tissues: ICRP 89 regional share × 5.3 L ÷ reference tissue volume.
- Heart and vessel blood: 1.
- Contents, CSF, urine and air: 0.

The 24% of blood in large arteries and veins is present only as the explicit vessel trunks.

## Named approximations

- **One figure, one pose.** face01 relaxed. The other identities share the body geometry; other
  poses need the rig binding above.
- **Clothed surface as skin.** The figure's outer surface is the uniform and boots, so clothing
  thickness counts as body. Where an arm lies against the chest closer than a voxel, arm and
  chest are fused: there is no skin between them.
- **Implicitly closed openings.** The four open shells are closed by the winding-number threshold
  (½), not re-meshed. The exterior surface is resolved to 1/32 of a source triangle edge (five
  splits).
- **One-voxel thin layers.** Skin is the one-voxel outer shell (10 mm against 2 mm of real skin),
  hence 6.7× the skin mass. Thin flat bones (skull vault, ribs, scapulae, iliac wings, facial
  bones) are a full voxel thick, hence 1.7× the cortical mass.
- **Hollow organ walls one voxel thick.** Stomach, colon, bladder and heart: walls over and
  contents under; each organ's total is on target. The interventricular septum is one plane.
- **Lost at 1 cm.**
  - Intervertebral discs thinner than a voxel: most thoracic ones.
  - The cord is 1 voxel across.
  - Most CSF: the subarachnoid space is absent; only the ventricles and the lumbar canal remain.
  - Articular cartilage and menisci.
  - Most nerves, lymph and small vessels.
- **Thin bones and teeth as cortical bone.** Radius/ulna, fibula, phalanges and metatarsals are
  cortical lines. Teeth are cortical bone (enamel's 2.9 g/cm³ is not represented).
- **Cancellous bone all spongiosa.** Cancellous bones are labelled spongiosa throughout, with a
  red-marrow composition, even in adult limb epiphyses that hold yellow marrow. Yellow marrow is
  only the shafts' core line, hence 0.07× the reference.
- **Authored proportions.** Vertebral levels, chest-wall and abdominal-wall thicknesses, fat
  depths, diaphragm domes, organ positions, orientations, paths and seed shapes are authored.
  Organ volumes are tuned to the reference; shapes are not measured anatomy.
- **Rib layer.** The rib layer and cavities use the body piece's own surface depth; each axial
  section is treated as a prism.
- **Flat chest, capped heart.** The heart is capped at 14 cm wide and below mid-T5 for the flat
  chest; it reaches 0.86 of its target. Lungs fill the remaining pleural space (0.89).
- **Liver, colon and tube organs.**
  - The liver is a ranked fill of the right upper quadrant whose edge runs down the right flank.
  - The colon is a tube of fixed path that ends 0.89 of target after clipping.
  - Duodenum and jejunum meet below voxel resolution: the small intestine has 3 pieces, 708
    voxels in the main one.
- **Vessels.** Walls are not resolved: vessel voxels are blood. Arteries and veins of a limb share
  one trunk. Trunks stop above the wrist and the ankle, where the real vessels run shallower than
  the skin voxel. Trunks may groove thin bones (rib, clavicle, scapula, pelvic wing, forearm,
  fibula, hand, foot) but not long bones, spine or skull.
- **Bone binding.** Nearest bone segment within the voxel's figure piece(s); not heat-diffused or
  skin-weight-interpolated.
- **Reference scaling.** Reference masses are scaled by the figure's body-volume ratio k. Organ
  size does not truly scale with a clothed figure's volume.
- **Table values.**
  - Strength classes are the owner's authored `crew_tissue` values.
  - The soft tissues without their own class (fat, marrow, brain, cord, eye, lung, solid
    abdominal organs) are assigned skin's class (the weakest), as an authored placeholder.
  - Densities and blood shares were transcribed offline from ICRP 89, ICRP 110 and IT'IS V4.1.
    Verify them against the publications before citing outside the project.
  - No injury criteria are used.

## Not done

- **The citadel library was not used.** `/citadel/3D/library/models/anatomy` is a local disk and
  was readable. It holds:
  - `Complete Human Male Anatomy_Blender_42.blend`: 856 MB, with no licence or provenance found.
  - Z-Anatomy exports from Sketchfab (splanchnology, myology, angiology, neurology, arthrology):
    CC-BY-SA-4.0, and material-merged, so organs are unnamed chunks.

  The procedural build keeps the volume a derivative only of the CC-BY-4.0 figure, and needs no
  share-alike. If the owner wants organ shapes from Z-Anatomy, the voxeliser can take meshes,
  but the output would then be CC-BY-SA-4.0.
- No network access was used.
- No runtime code was changed here.
