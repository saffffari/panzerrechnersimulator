# The vehicle surface model: every material on the Tiger, from its causes (design, 2 November 2026)

The owner, after seeing the FEA carnage against blank cabin walls and the whitewash mottling outside:
the vehicle is most of the screen, inside especially; its materials must go from "default metal" to a
war machine; wear, rust, grime and damage must not be mixed in at random but placed exactly where
they would really be; drop the white camouflage for the factory German grey with real 3D snow on top;
the look must be the same in every environment. This document says how.

## The guiding rule: realistic in essence, carried by the low resolution

What has worked best in this project (the owner, 2 November 2026): build more realism than the pixel
grid strictly demands, and let the low resolution and the dither carry it, so the frame is realistic
in essence rather than in pixel-art shorthand. So every layer here is authored and simulated at true
scale (1 cm placement, sub-centimetre detail, measured reflectances), never simplified to suit
1024x576; the grid and the dither do the reduction, and what survives them reads as a real worn
machine because its causes are real.

## What is wrong today

Every wear and dirt layer on the vehicle is a noise pattern switched on by a coarse rule:

| Layer | Today | Why it reads false |
|---|---|---|
| Interior paint | one ivory colour on every source-metal face (`paint_source_metal`) | no primer, no steel, no variation, no history |
| Interior grime | fbm blotches by height and facing (`cabin_grime`) | dirt where no dirt would go, none in the corners where it would |
| Scratches | two noise directions in patches (`apply_wear`) | scratches on walls nobody touches |
| Worn edges | screen-space normal crease, one pixel wide (`rm_shade.wgsl`) | every edge identical, no chips, no primer halo, flickers with the view |
| Corner grime | screen-space occlusion | changes as the camera moves; it is the gather's AO, not the surface's |
| Exterior | whitewash brush noise, dark blotches, slush noise on the lower hull | the owner dislikes the coat; the blotches are random |
| Other vehicles | the same noise at per-tank loads (`vehicle_finish`) | identical placement logic |
| Edges | the source meshes' edges are perfectly sharp and painted to the edge | "super crisp": real armour edges are rounded by paint build-up and wear |

None of it is placed by a cause. That is the whole problem, and the fix is to place every layer by its
cause.

## The idea: a surface that remembers what happened to it

Each vehicle carries a **surface model**: a sparse 3D field fixed to its hull, turret and gun frames,
1 cm cells on the surfaces only, holding *measured* properties of the geometry and the *accumulated*
results of the processes that act on a tank. Materials are then evaluated per pixel as a physically
ordered stack of real layers, each layer's amount read from the field. Nothing is random at the scale
of placement; noise only shapes the sub-centimetre detail inside a region the causes chose.

### Why a field in the vehicle's frame, not textures on a UV atlas

| | Sparse 3D field (recommended) | Unique UV atlas |
|---|---|---|
| Meshes | used as they are | every vehicle mesh unwrapped (the Tiger hull alone is 1,215 nodes), seams |
| Engine | one more bind group | a second UV in every vertex layout and pipeline (depth, G-buffer, shadow, ray tracing) |
| Separate props, crew kit, FEA-deformed plate | sample it by position, for free | each needs its own unwrap |
| Dynamic damage (hits, scorch, blood, soot) | splat into cells by world position | render into the atlas |
| Thin sheet (fenders, 2 mm) | each side's cells sit 1 cm apart along the normal: separate | separate texels |
| Memory | about 10-30 M cells on the surfaces (bricks of 8x8x8 where any surface passes) | 2-4 8K maps per vehicle |

The field is the better fit for this engine: no vertex format change, every mesh and prop included,
damage written by position from the same events the FEA model produces. It is also exactly the seed
already written for the cabin (`src/cabin_material_field.rs`, uncommitted: openness, exposure,
overhang, crew traffic, baked by ray casting the source triangles), which this design generalises to
the whole vehicle at 1 cm with bricks.

### Why material and normals for the small scale, geometry for the large

- **Below about 2 cm** (chips, scratches, rust, weld texture, streaks, edge rounding): material layers
  plus a shading normal from the field. At 1024x576 a pixel is 5 mm a metre from the eye and a
  centimetre at five metres; real geometry at this scale would cost millions of triangles and buy
  nothing a correct normal does not.
- **Edge rounding** is a shading normal bent toward the edge's bisector over the real radius (paint
  build-up and wear give 2-6 mm, more where the edge is used), from a distance-to-edge channel. It is
  the single largest fix for "super crisp", named as a normal approximation of a geometric round.
- **Above about 5 cm** (dents, bent and missing fenders, torn track guards, lost tools, holes): real
  geometry. The FEA lattice already solves each strike's deformed plate (lip, bulge, petals) and the
  renderer does not draw it yet; that is part of this pass. Historically attested wear-and-tear
  geometry (front fender sections lost, bent mudguards, missing tools) is authored on the source in
  Blender as variants.

This keeps the physical-truth rules: the layers change albedo, roughness, metalness and normal only;
nothing here adds or bends light. The "occlusion" the field measures is a cause (where dirt pools),
never used to darken lighting.

## The measured channels (bake, offline, cached per source mesh)

Ray cast against the vehicle's own source triangles (a CPU triangle BVH, deterministic, cached by the
meshes' hash):

1. **Openness at 0.5, 2 and 6 cm**: the share of short rays that escape. Flat 0.5, convex edges and
   corners above, creases and weld joints below. Multi-scale separates a sharp plate edge from a broad
   convex curve.
2. **Distance to the nearest convex edge, and its direction**: drives edge rounding and the chip band.
3. **Sky exposure**: the hemisphere visible to open sky. Weathering (UV chalking, rain, snow fall)
   reaches only what the sky sees.
4. **Enclosure at 25 cm**: where dirt, dust and soot pool because neither air flow nor hands reach.
5. **Up-facing and gravity terms**: what holds water, dust and snow; what sheds them.
6. **Material class**: from a reviewed catalogue mapping each source node to what it is (below).

## The processes (simulated, offline or at campaign time, each with its cause)

Each process writes into its own channel of the field. Deterministic, seeded by the vehicle's identity,
amounts scaled by the existing campaign loads (`surface_wear`) and history.

- **Water and gravity flow.** Drops released where rain and meltwater gather (ledges, bolt heads, hatch
  rims, weld beads, the tops of fittings) are traced down the actual surfaces under gravity until they
  drip off. Their paths are the streaks: dust and grime carried down, and rust bleeding down only from
  where steel is already exposed. Rust is causal: exposed steel x wetness x time, never painted on.
- **Crew contact.** Authored routes and contact points: the climb onto the hull (front plate step,
  grab handles), walking lines on the deck and turret roof between hatches, hatch rims and handles,
  the tool rack, the loader's path from the racks to the breech, each station's hands and boots
  inside. Contact wears topcoat to primer, primer to steel, polishes steel, and leaves hands' grime.
- **Abrasion and impacts at the extremities.** The most exposed convex edges facing the direction of
  travel and the vehicle's outer envelope take chips from brush, debris and the crew's own handling.
- **Track throw.** Particles launched from the tracks' top run, sprocket and idler at their speeds,
  traced ballistically to where they strike the hull, fenders, final drives and lower plates: mud,
  slush and snow spatter where a moving Tiger really collects it.
- **Exhaust.** Heat along the exhaust stacks and their guards (paint burnt brown then black, then
  scaled steel, by temperature from the engine model), and soot carried by the plume with the wind
  onto the rear plate.
- **The gun.** Barrel paint burnt toward the muzzle by firing heat (the round count), propellant soot
  on the muzzle brake and mantlet, and inside the breech's fumes and leaking gas on the roof, the
  loader's side and the ventilator.
- **Leaks.** Oil and grease from the final drive seals, road wheel hubs, the engine deck's hatches and
  the traverse gearbox; fuel at the filler caps; recoil oil under the cradle. Each traced under
  gravity like water, staining dark and glossy where it runs and pools.
- **Dust and debris inside.** Settling on what faces up and is enclosed, swept clean where boots go.
- **Battle damage (at play time, from the FEA model and the fire).** Each strike writes its
  heat-affected ring (paint burnt off round the wound), the back face's flaked paint even when not
  perforated (interior scabbing from the lattice's back-face strain), spall gouges and embedded chips'
  marks, bullet splash (paint chipped and lead-smeared in rings), scorch from fire graded by the fire
  model's surface temperature (ivory to brown to black to scaled steel), soot from smoke, and blood
  that darkens from red to brown as it dries.

## The layer stack (per pixel, physically ordered)

1. **Substrate** by material class: rolled armour (mill-scale texture, faint rolling direction), cast
   steel (pitted, rougher; the cupola, the gun's castings), sheet steel (fenders, bins), manganese
   track steel, rubber (road wheel tyres), glass, wood (tool handles), canvas and leather (interior),
   brass (cartridges), wiring.
2. **Primer**: red-oxide (Rotbraun) over all painted steel.
3. **Topcoat**: Dunkelgrau RAL 7021 outside, Elfenbein RAL 1001 in the fighting compartment, with
   factory variation (spray thickness, overspray at masking lines, runs on vertical faces). The
   whitewash is retired from the default; it can return as a simulated coat later if wanted.
4. **Wear** through the coats in steps: topcoat chipped with a primer halo, primer worn to steel,
   steel polished where contact continues; chip size from the cause's energy, shape from sub-cell
   detail.
5. **Corrosion**: on exposed steel by wetness and time, flash rust to scale, its streaks from the
   flow.
6. **Deposits**: dust, grime, soot, oil, mud, each from its process.
7. **Weather**: UV chalking on sky-facing paint, water marks, snow (3D, below).
8. **Battle**: the dynamic damage layers above.

Each layer has measured reference values: paint reflectances from colour standards (RAL 7021,
RAL 1001, RAL 8012), steel and oxide reflectances from the literature, roughness by state (wet, oiled,
polished, chalked). The ledger names every estimate.

## Snow, physically, and the same in every environment

The base coat is the same everywhere; only physical layers change with the act and the weather. Snow
on the vehicle extends the lying-snow relief already in place: fall weighted by sky exposure and wind
(windward faces of fittings catch it), packed in recesses and on the track guards, sliding off steep
faces, melted on the engine deck and around the exhausts by their heat, compressed and cleared along
the crew's routes, and thrown by the tracks. The ground's snow and the tank's come from the same
weather.

## Tools and verification

- `KESSEL_VSM_VIEW=<channel>`: any measured or simulated channel drawn as false colour on the vehicle,
  for checking placement before materials.
- `KESSEL_VSM_LAYERS=...`: layers on and off for A/B stills.
- An `fx-review` part `materials`: the exterior from eight bearings and three distances, every crew
  station's view, close-ups of the reference spots (front plate edge, hatch rims, exhausts, final
  drives, the breech, the loader's floor), at noon, 1024x576, 5x to 5120x2880.
- Reference set: the owner's chosen photographs of Tigers in service in 1942-43 and the surviving
  vehicles, used for comparison only, never shipped.
- Tests: the bake's determinism and cache key, the flow tracer's mass conservation, openness on
  known shapes (flat, edge, crease), layer ordering, and that no layer writes emission.

## Phases

1. **Foundation**: the brick field and its bake (all channels above), the material-class catalogue,
   the debug views. Proves placement before any colour changes.
2. **Paint and steel**: substrate, primer, topcoat, edge rounding, wear through the coats by contact
   and abrasion, corrosion. Interior and exterior together. The noise layers retire.
3. **Processes**: water flow and streaks, track throw, exhaust heat and soot, the gun, leaks, dust.
4. **Snow and season**: whitewash off by default, physical snow on the vehicle, mud and dust by act.
5. **Battle**: the dynamic layer driven by the FEA strikes, the fire and the crew, and drawing the
   lattice's solved deformation.
6. **Every other vehicle**: the T-34s and the rest through the same model with their own paint (4BO)
   and classes.

## Status (2 November 2026)

What the live code computes, with its limits, is in `docs/PHYSICAL_TRUTH_AUDIT.md` ("The vehicle
surface model"); this section only says where the phases stand.

- Phase 1, the field: implemented (`src/vehicle_surface.rs`, `shaders/rm_vsm.wgsl`). Openness at
  0.6, 2 and 6 cm, sky, enclosure, wall thickness, the nearest sharp edge as an exact line, and
  the material class, at 1 cm on the hull, turret and gun.
- Phase 2, paint and steel: implemented as layers 1-9 (substrate, primer, topcoat, rounding,
  chips, contact, corrosion, deposits, weather) on the hull, turret, gun, roof hatches, visor holder
  and cupola grip; the noise layers and screen-space worn edges stand down there.
- Phase 3, processes: implemented as layers 10-14 (water and its streaks, track throw, exhaust heat
  and soot, the gun's heat and soot, final-drive oil), simulated at bake time.
- Phase 4, snow and season: implemented for the player's vehicle. The whitewash no longer reaches
  it; the lying snow keeps to what the sky sees, melts off the exhaust and the warm engine deck,
  and is trodden off the hatches and the deck lane, identically in the depth and G-buffer passes.
- Phases 5 and 6 (battle damage written into the field, the solved lattice drawn; other vehicles):
  not started.

Review: `fx-review` part `materials` and `tools/vsm_contact_sheet.py` (each layer's stills, lit and
as unlit albedo, beside the former finish).
