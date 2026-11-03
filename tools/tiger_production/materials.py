"""The Tiger's physical material library and the rules that assign it (Blender side only).

Subcommands (Blender 5.1, headless; the engine's shaders, crush and look are not touched):

    blender -b --factory-startup --python tools/tiger_production/materials.py -- library OUT.blend
        Write the library: one Principled BSDF per physical surface class, each carrying the custom
        properties phys_material, source, engine_texture (and albedo_srgb, albedo_linear, note).

    blender -b --factory-startup KIT.blend --python tools/tiger_production/materials.py -- kithints OUT.npz
        Read the RM-5010 kit CAD's SolidWorks display colours (mat_0..mat_25) per face, for the parts
        whose colour split carries meaning (ammunition, seats, glass, tools, road wheels, radiators...),
        and write them in the game-master frame (game = s * kit + t, docs/TIGER_REBUILD_PLAN_20261102.md).
        Also measures the split parameters the geometric rules use (tyre inner radius, case length).

    blender -b --factory-startup IN.blend --python tools/tiger_production/materials.py -- assign LIB.blend OUT.blend
            [--hints kit_hints.npz] [--scheme grey|yellow] [--he-share 0.0] [--dry-run] [--report r.json]
            [--air-out air.npz | --air-in air.npz]
        Apply the library to any Tiger file: the production master (phys_material/fea_group/kit_number
        per object), the July game master (looked up by game node in catalog/tiger_parts.v1.json) or the
        kit CAD itself (by kit number, with its own face colours as the hints). Saves a NEW file; the
        input and the three source files are refused as outputs. The file's frame is fitted to the game
        master's (one similarity: scale, rotation, offset) from the catalog's part centroids, and refused
        if parts are not where the catalog puts them. The inside-air grid (see PAINT) can be written with
        --air-out from a file whose shell seals (the production master: 14.9 m3) and reused with --air-in
        for one whose shell leaks (the kit-geometry gen2 master: 2-4 m3 on its own; the leak, an
        opening wider than the closing, is not yet located).

    blender -b --factory-startup ASSIGNED.blend --python tools/tiger_production/materials.py -- render PREFIX
        Small EEVEE validation stills: exterior three-quarter, fighting compartment, engine bay,
        running gear, ammunition rack (section views by camera clipping, nothing hidden or moved).

    python3 -I tools/tiger_production/materials.py csv
        Join the visual classes onto docs/fea/tiger_materials.csv and write docs/fea/tiger_visual_materials.csv.

How a part's faces are split (every rule is geometry or the kit's own colour split, never by hand):
  PAINT       Painted steel (armour, plate, structural fittings, the non-glass parts of sights and
              periscopes, ammunition racks). The armour shell (armour, plate, structural fittings, glass and
              gun: SHELL_PHYS) is voxelised at VOXEL_M, closed by DILATE_STEPS voxels (vision slits, seams,
              the turret-ring gap) and the outside air flooded in from the grid border; the air the flood
              cannot reach is the vehicle's inside. A face looks 1.5-6 voxels along its normal: landing in
              inside air makes it interior, in outside air exterior; a face buried in clutter
              takes the label of the nearest air. Interior faces behind the firewall
              (engine bay; the firewall plane is the front of the engine, fuel tanks, radiators and fans
              less FIREWALL_MARGIN_M) take the red-oxide primer, the rest Elfenbein.
  ROADWHEEL   Disc-shaped parts only: the wheel axis is the smallest principal axis; faces whose radius
              from the axis is beyond the tyre's inner radius are rubber, the rest the painted steel disc.
              The inner-radius ratio is measured from the kit's colour split (black tyre faces).
  SPROCKET    Sprocket and idler: the outer band (teeth, rim, r >= CONTACT_BAND * R) is bare worn cast
              steel where the track wears the paint off; the rest is exterior paint.
  AMMO        The kit's colour split, transferred face by face (nearest kit face in the game frame): case
              colour -> brass, projectile colour -> projectile, rack colour -> PAINT. Fallback: along the
              round's long axis, the base end (larger radius) to CASE_FRACTION of the length is case.
              The top PZGR_CAP_MARK_M of the projectile is the white ballistic-cap marking (Pzgr. 39).
  GLASS       The game master's own glass slots ('periscope glass', 'Glass'), else the kit's glass
              colours (magenta mat_6, green mat_23); the rest of the part is PAINT.
  GUN         Gun steel facing out (barrel, mantlet side) is painted; facing in (breech, cradle,
              recuperator) is blued; the three MG 34s are blued all over.
  TOOL        Kit colour split (brown haft -> wood); single-colour tools: bins along the long axis whose
              cross-section exceeds HEAD_WIDTH_RATIO x the haft's are the steel head.
  LAMP        Faces facing forward in the front LENS_DEPTH of the lamp are the lens glass.
  RADIATOR    Faces normal to the core's thin axis inside the central CORE_SPAN are the copper fin core,
              the rest the brass header tanks and frame.

Engine mapping (notes only; src/ is unchanged): `engine_texture` names the slot each class maps to.
Existing slots are tank.rs `tiger_paint_finish` swatches 0 (exterior paint, Material.001 painted nodes),
1 (Interior Paint), 2 (Turret Floor), 3 (track steel), 4 (cable/exhaust steel), 5 (instrument face);
the worn edges and grime come from rm_gbuffer.wgsl `vehicle_finish` on top of swatch 0. Classes that
have no swatch today are given a proposed swatch number (20+) so a later bake can carry them.
"""
import sys, os, json, math, csv, re

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, '..', '..'))

# Kit CAD -> game master frame (fit.json of 2 Nov; docs/TIGER_REBUILD_PLAN_20261102.md), no rotation.
KIT_S = 38.90497872272133
KIT_T = (-0.004448562457774472, -2.636855910439308, 0.4666502927059094)

INPUTS = ['/windows/shop/ts_assets/cad_tiger_i_rm5010_complete.blend',
          '/windows/shop/panzerrechnersystem/assets/tanks/tiger/Panzerkampfwagen_VI_Tiger_Ausf.E.blend',
          '/windows/shop/panzerrechnersystem/assets/tanks/tiger/tiger_production.blend']

# Split parameters (each documented in the module docstring).
VOXEL_M = 0.025            # air-volume grid
DILATE_STEPS = (1, 2, 3, 4)   # voxels of closing tried in turn until the inside holds MIN_INSIDE_AIR_M3
MIN_INSIDE_AIR_M3 = 4.0
MISPLACED_M = 0.05  # an exact-match part's volume centroid this far from the catalog's: not in the master pose
MAX_FRAME_RESIDUAL_M = 0.02  # the file must place its parts where the catalog does, up to one similarity
# the shell: everything that bounds the crew and engine spaces (not running gear, track, tools, cables)
SHELL_PHYS = ('armour_rolled_homogeneous', 'steel_structural_plate', 'steel_structural', 'glass_laminated', 'steel_gun')
FIREWALL_MARGIN_M = 0.05
CONTACT_BAND = 0.88
CASE_FRACTION = 0.57      # measured: kit 23M03 case colour spans 0.00..0.57 of the round's length
TYRE_INNER_RATIO = 0.86   # replaced by the kit measurement when hints are given
PZGR_CAP_MARK_M = 0.040   # bocn.co.uk: Pzgr. 39(-1) "upper 40 mm of the ballistic cap painted white"
HEAD_WIDTH_RATIO = 1.8
LENS_DEPTH = 0.20
CORE_SPAN = 0.80

# ---------------------------------------------------------------------------------------------
# The library. Colours: 'lab' = CIELAB (D65) from the RAL Classic register, converted below;
# 'lin' = linear sRGB reflectance (metals: F0 at normal incidence). Every row has its source.
RAL_SRC = 'RAL Classic CIELAB as tabulated in en.wikipedia.org/wiki/List_of_RAL_colours (RAL CLASSIC register)'
F0_SRC = 'F0 linear sRGB, Real-Time Rendering 4th ed. Table 9.2 (Hoffman, measured spectral reflectance)'
LIB = [
    dict(name='TG_paint_dunkelgrau_ral7021', phys='paint_exterior_dunkelgrau', lab=(20.639, -0.806, -1.325), metallic=0.0, roughness=0.75, ior=1.5,
         source=RAL_SRC + ' (RAL 7021, period name Dunkelgrau Nr. 46, the Tiger factory finish to Feb 1943). Roughness 0.75 = the engine\'s flat paint (tank.rs tiger_paint_finish); IOR 1.5 = oil/alkyd binder',
         engine='swatch 0 (Material.001 painted nodes) + rm_gbuffer vehicle_finish wear',
         note='The engine swatch 0 is sRGB 72/74/78 (L*~31), lighter than the RAL chip (L* 20.6): the engine value reads as field-weathered grey; this is the chip.'),
    dict(name='TG_paint_dunkelgelb_ral7028', phys='paint_exterior_dunkelgelb', lab=(58.0, 0.0, 22.0), metallic=0.0, roughness=0.78, ior=1.5,
         source='ESTIMATE. RAL 7028 is not in the current RAL Classic register and no instrumental measurement was found (Weald Foundation SEM match paywalled). Lab chosen as a mid tan between RAL 7008 (41/4/23) and RAL 1001 (74/6/27), per descriptions of "dark yellow ... tan with a greenish tinge"',
         engine='swatch 0 alternative (spring 1943 scheme; not in the engine)',
         note='Commonly cited as ordered 18 Feb 1943 (HM 1943 Nr. 181) as the new base colour; a factory change, so a spring-1943 vehicle may carry either.'),
    dict(name='TG_paint_elfenbein_ral1001', phys='paint_interior_elfenbein', lab=(73.595, 5.518, 26.95), metallic=0.0, roughness=0.70, ior=1.5,
         source=RAL_SRC + ' (RAL 1001, period name Elfenbein). Roughness 0.70 = engine Interior Paint',
         engine='swatch 1 (Interior Paint)',
         note='Engine uses sRGB 205/186/136, a common conversion; the RAL Lab gives ~208/176/132.'),
    dict(name='TG_primer_red_oxide_ral8012', phys='primer_red_oxide', lab=(27.74, 22.005, 15.201), metallic=0.0, roughness=0.82, ior=1.5,
         source=RAL_SRC + ' (RAL 8012 Rotbraun, the usual citation for German factory primer). Iron-oxide primer is matte: roughness 0.82',
         engine='proposed swatch 20 (engine bay interior)',
         note='Whether the Tiger E engine bay was left in primer or painted is an open question (see report).'),
    dict(name='TG_armour_steel_bare', phys='armour_steel_bare', lin=(0.562, 0.565, 0.578), metallic=1.0, roughness=0.40, ior=2.9,
         source=F0_SRC + ' (iron). Bright gouges, shot strikes and cut edges; roughness 0.40 for a ground or fresh-fractured surface',
         engine='proposed swatch 21 (damage/penetration faces); rm_gbuffer worn chips',
         note=''),
    dict(name='TG_armour_steel_worn', phys='armour_steel_worn', lin=(0.36, 0.35, 0.33), metallic=0.6, roughness=0.50, ior=2.9,
         source='rm_gbuffer.wgsl vehicle_finish chip colour (linear 0.36/0.35/0.33, metallic 0.6 for unrusted) so the library and the engine wear agree; physically mill scale (magnetite) and light rust over RHA',
         engine='rm_gbuffer vehicle_finish worn chips (no swatch)',
         note=''),
    dict(name='TG_cast_steel', phys='steel_cast_bare', lin=(0.52, 0.52, 0.52), metallic=1.0, roughness=0.50, ior=2.9,
         source=F0_SRC + ' (iron 0.56, lowered ~8% for the oxide film of as-cast/worn steel). Sprocket teeth, idler rim, machined rollers',
         engine='proposed swatch 22', note=''),
    dict(name='TG_track_manganese_cast', phys='steel_track_cast', srgb=(58, 58, 56), metallic=0.48, roughness=0.90, ior=2.9,
         source='The engine\'s reviewed track steel (tank.rs TRACK_STEEL_SRGB 58/58/56, metallic 0.48, roughness 0.90): cast manganese-steel links under rust bloom and road dust',
         engine='swatch 3 (TRACK_STEEL)', note=''),
    dict(name='TG_track_pin', phys='steel_track_pin', lin=(0.45, 0.45, 0.46), metallic=0.85, roughness=0.45, ior=2.9,
         source='Forged steel pin; ' + F0_SRC + ' iron darkened for oxide, metallic 0.85 as the pin heads carry rust spots',
         engine='swatch 3 (TRACK_STEEL) or proposed 23', note=''),
    dict(name='TG_roadwheel_rubber', phys='rubber_roadwheel_tyre', lin=(0.030, 0.030, 0.031), metallic=0.0, roughness=0.85, ior=1.52,
         source='Carbon-black-filled rubber: diffuse reflectance ~3% (carbon-black compounds measure 2-5%); n=1.52 (natural rubber); roughness 0.85 for a worn, dusty tread. Buna synthetic in 1943',
         engine='proposed swatch 24', note=''),
    dict(name='TG_roadwheel_disc_ral7021', phys='steel_roadwheel_disc', lab=(20.639, -0.806, -1.325), metallic=0.0, roughness=0.80, ior=1.5,
         source=RAL_SRC + ' (RAL 7021): pressed-steel discs painted with the hull; roughness 0.80 (dustier than the hull)',
         engine='swatch 0 (painted) or proposed 25', note='In the yellow scheme the discs take TG_paint_dunkelgelb_ral7028.'),
    dict(name='TG_gun_steel_blued', phys='steel_gun_blued', lin=(0.12, 0.13, 0.15), metallic=1.0, roughness=0.35, ior=2.9,
         source='Black-oxide (blued) gun steel: a thin magnetite film over steel; reflectance ~12-15%, slightly blue; roughness 0.35 machined and oiled (reasoned, no measurement)',
         engine='proposed swatch 26 (breech, MG 34)', note=''),
    dict(name='TG_gun_steel_painted', phys='steel_gun_painted', lab=(20.639, -0.806, -1.325), metallic=0.0, roughness=0.72, ior=1.5,
         source=RAL_SRC + ' (RAL 7021): barrel and mantlet painted with the vehicle', engine='swatch 0', note=''),
    dict(name='TG_brass_cartridge', phys='brass_cartridge_case', lin=(0.910, 0.778, 0.423), metallic=1.0, roughness=0.30, ior=1.1,
         source='Cartridge brass (70/30) F0 linear sRGB 0.910/0.778/0.423 (physicallybased.info, from measured n,k); roughness 0.30 for handled, lightly tarnished cases',
         engine='proposed swatch 27', note='Many 1943 8.8 cm KwK 36 cases were lacquered steel (6347 St), see TG_case_steel_lacquered.'),
    dict(name='TG_case_steel_lacquered', phys='steel_cartridge_case', lab=(49.694, -1.244, 7.793), metallic=0.0, roughness=0.40, ior=1.5,
         source='ESTIMATE. Lacquered steel case (6347 St), grey-green lacquer; RAL 7003 Moss grey Lab used as a stand-in (' + RAL_SRC + ')',
         engine='proposed swatch 27 alternative', note='Not assigned by default.'),
    dict(name='TG_projectile_pzgr39_black', phys='projectile_ap_pzgr39', lab=(4.041, 0.091, -0.873), metallic=0.0, roughness=0.45, ior=1.5,
         source='Pzgr. 39 body painted black (bocn.co.uk 8,8cm Pzgr/Pzgr 39 thread; Lone Sentry, Tactical and Technical Trends: German 88 AP "black with a white tip"); RAL 9005 Lab (' + RAL_SRC + ')',
         engine='proposed swatch 28', note=''),
    dict(name='TG_projectile_cap_white', phys='projectile_marking_white', lab=(93.613, -0.425, 6.008), metallic=0.0, roughness=0.50, ior=1.5,
         source='White top of the ballistic cap, upper 40 mm (bocn.co.uk, Pzgr. 39(-1)); RAL 9010 Lab (' + RAL_SRC + ')',
         engine='proposed swatch 29', note=''),
    dict(name='TG_projectile_sprgr_olive', phys='projectile_he_sprgr', lab=(24.036, -1.163, 7.105), metallic=0.0, roughness=0.55, ior=1.5,
         source='ESTIMATE. Sprgr. HE body in olive/field grey as briefed; no source found for the KwK 36 Sprgr. colour; RAL 6006 Grauoliv Lab as stand-in (' + RAL_SRC + ')',
         engine='proposed swatch 30', note='Not assigned by default: the kit models every round with the Pzgr. ballistic-cap shape.'),
    dict(name='TG_projectile_band_yellow', phys='projectile_marking_yellow', lab=(77.828, 10.664, 94.6), metallic=0.0, roughness=0.50, ior=1.5,
         source='ESTIMATE. Yellow band as briefed, unsourced; RAL 1021 Lab as stand-in (' + RAL_SRC + ')',
         engine='proposed swatch 31', note='Not assigned: no source for its position.'),
    dict(name='TG_glass_vision_block', phys='glass_laminated', lin=(0.61, 0.78, 0.67), metallic=0.0, roughness=0.0, ior=1.52, transmission=1.0,
         source='Soda-lime laminated block, n=1.52. Tint = exp(-k*9 cm) with ESTIMATED iron-oxide absorption k=0.055/0.028/0.045 per cm (green-edged plate glass); a surface tint standing in for volume absorption',
         engine='proposed swatch 32 (engine: periscope glass)', note=''),
    dict(name='TG_glass_lamp', phys='glass_lamp', lin=(0.95, 0.95, 0.95), metallic=0.0, roughness=0.05, ior=1.52, transmission=1.0,
         source='Pressed clear lens glass, n=1.52, thin (negligible absorption); roughness 0.05 for the moulded lens', engine='proposed swatch 33', note=''),
    dict(name='TG_copper_conductor', phys='copper_conductor', lin=(0.955, 0.638, 0.538), metallic=1.0, roughness=0.40, ior=1.1,
         source=F0_SRC + ' (copper); roughness 0.40 tarnished', engine='proposed swatch 34', note='The kit carries no electrical harness; used where geometry exists.'),
    dict(name='TG_cable_insulation', phys='cable_insulation', lin=(0.035, 0.035, 0.035), metallic=0.0, roughness=0.80, ior=1.5,
         source='Rubber/braided-cloth insulation, carbon-black dyed: ~3.5% diffuse (as tyre rubber), roughness 0.80', engine='swatch 4 (cable) or proposed 35', note=''),
    dict(name='TG_wire_rope_steel', phys='steel_wire_rope', lin=(0.18, 0.17, 0.16), metallic=0.6, roughness=0.75, ior=2.9,
         source='Tow cables: ungalvanised steel wire rope, greased and rusted; iron F0 darkened and partly dielectric (rust), reasoned',
         engine='swatch 4 (CABLE_EXHAUST_STEEL)', note='match.py classes the two tow cables as copper_cable; they are steel rope.'),
    dict(name='TG_seat_leather', phys='leather_seat', lin=(0.045, 0.030, 0.022), metallic=0.0, roughness=0.55, ior=1.5,
         source='ESTIMATE. Dark brown/black vehicle leather; diffuse ~3-4%, roughness 0.55 (worn, waxed)', engine='proposed swatch 36 (engine: seats_black)', note=''),
    dict(name='TG_seat_steel', phys='steel_seat_frame', lab=(73.595, 5.518, 26.95), metallic=0.0, roughness=0.60, ior=1.5,
         source='Seat frames and pans painted with the interior, RAL 1001 (' + RAL_SRC + '); roughness 0.60 for rubbed paint',
         engine='swatch 1 (Interior Paint)', note=''),
    dict(name='TG_wood_haft', phys='wood_ash_haft', lin=(0.30, 0.19, 0.10), metallic=0.0, roughness=0.60, ior=1.5,
         source='ESTIMATE. Ash tool hafts, oiled and weathered: linear albedo 0.30/0.19/0.10 (sRGB ~149/121/89)', engine='proposed swatch 37', note=''),
    dict(name='TG_tool_steel_head', phys='steel_tool_head', lin=(0.30, 0.30, 0.31), metallic=0.9, roughness=0.55, ior=2.9,
         source='Forged tool steel with black oxide and rust; iron F0 darkened (reasoned)', engine='proposed swatch 38', note=''),
    dict(name='TG_radiator_copper_core', phys='copper_radiator_core', lin=(0.55, 0.36, 0.28), metallic=1.0, roughness=0.55, ior=1.1,
         source=F0_SRC + ' (copper 0.955/0.638/0.538) darkened ~40% for a Cu2O tarnish film over fin stock (reasoned)', engine='proposed swatch 39', note=''),
    dict(name='TG_radiator_brass', phys='brass_radiator_tank', lin=(0.70, 0.58, 0.32), metallic=1.0, roughness=0.50, ior=1.1,
         source='Brass F0 (physicallybased.info 0.910/0.778/0.423) darkened ~25% for tarnish; header tanks and side plates', engine='proposed swatch 40', note=''),
    dict(name='TG_fuel_tank_sheet', phys='steel_sheet_tank', lin=(0.55, 0.55, 0.56), metallic=1.0, roughness=0.55, ior=2.9,
         source='Terne-coated sheet steel (lead-tin), dull: F0 ~0.55 (between lead ~0.63 and oxidised iron), roughness 0.55 (reasoned)', engine='proposed swatch 41', note='Possibly painted in service; open question.'),
    dict(name='TG_exhaust_tempered', phys='steel_exhaust_heat', srgb=(68, 52, 44), metallic=0.3, roughness=0.85, ior=2.9,
         source='Heat-tempered mild steel run above 400 C: black oxide scale with red-brown rust (temper colours only at 220-320 C); partly dielectric oxide, metallic 0.3',
         engine='swatch 4 (CABLE_EXHAUST_STEEL, engine 73/72/68 metallic 0.60)', note=''),
    dict(name='TG_engine_cast_aluminium', phys='aluminium_engine_block', lin=(0.913, 0.922, 0.924), metallic=1.0, roughness=0.65, ior=1.4,
         source=F0_SRC + ' (aluminium); as-cast, oily: roughness 0.65. The early Tiger E HL 210 P45 had an aluminium crankcase (HL 230 cast iron from mid-1943)',
         engine='proposed swatch 42', note='Engine paint finish unknown; open question.'),
    dict(name='TG_machinery_cast_iron', phys='iron_cast_machinery', lin=(0.45, 0.45, 0.45), metallic=1.0, roughness=0.60, ior=2.9,
         source='Grey cast iron housings (gearbox, steering, motors): iron F0 lowered for graphite/oxide, roughness 0.60 (reasoned)', engine='proposed swatch 43', note=''),
    dict(name='TG_hydraulic_steel', phys='hydraulic_steel_oil', lin=(0.55, 0.55, 0.56), metallic=1.0, roughness=0.30, ior=2.9,
         source='Machined, oil-filmed steel cylinders and rods: iron F0, roughness 0.30', engine='proposed swatch 44', note=''),
    dict(name='TG_spring_steel', phys='steel_spring', lin=(0.35, 0.35, 0.36), metallic=1.0, roughness=0.45, ior=2.9,
         source='Oil-quenched spring steel, oxide-black: iron F0 darkened, roughness 0.45 (reasoned)', engine='proposed swatch 45', note=''),
    dict(name='TG_instrument_face', phys='instrument_face', srgb=(40, 40, 40), metallic=0.0, roughness=0.45, ior=1.5,
         source='The engine\'s black enamel dial (tank.rs DIAL_FACE_SRGB 40/40/40, roughness 0.45); surviving Bovington TPZ in black (tiger1.info)', engine='swatch 5 (Illuminated Dial)', note=''),
    dict(name='TG_turret_floor', phys='turret_floor_plate', srgb=(58, 70, 64), metallic=0.0, roughness=0.84, ior=1.5,
         source='The engine\'s reviewed turret floor (tank.rs tiger_paint_finish swatch 2, sRGB 58/70/64, roughness 0.84)', engine='swatch 2 (Turret Floor)', note=''),
]
BY_NAME = {m['name']: m for m in LIB}

# phys_material (catalog class, FEA) -> the visual materials it resolves to, and the split rule.
PHYS_VISUAL = {
    'armour_rolled_homogeneous': ('PAINT', 'TG_paint_dunkelgrau_ral7021;TG_paint_dunkelgelb_ral7028;TG_paint_elfenbein_ral1001;TG_primer_red_oxide_ral8012;TG_armour_steel_worn;TG_armour_steel_bare'),
    'steel_structural_plate': ('PAINT', 'TG_paint_dunkelgrau_ral7021;TG_paint_elfenbein_ral1001;TG_primer_red_oxide_ral8012'),
    'steel_structural': ('PAINT', 'TG_paint_dunkelgrau_ral7021;TG_paint_elfenbein_ral1001;TG_primer_red_oxide_ral8012'),
    'steel_cast': ('SPROCKET', 'TG_cast_steel;TG_paint_dunkelgrau_ral7021'),
    'steel_track_cast': ('TRACK', 'TG_track_manganese_cast;TG_track_pin'),
    'steel_roadwheel_rubber_tyred': ('ROADWHEEL', 'TG_roadwheel_rubber;TG_roadwheel_disc_ral7021'),
    'steel_gun': ('GUN', 'TG_gun_steel_painted;TG_gun_steel_blued'),
    'iron_cast_machinery': ('WHOLE', 'TG_engine_cast_aluminium;TG_machinery_cast_iron'),
    'radiator_copper_brass': ('RADIATOR', 'TG_radiator_copper_core;TG_radiator_brass'),
    'steel_sheet_tank': ('WHOLE', 'TG_fuel_tank_sheet'),
    'steel_exhaust': ('WHOLE', 'TG_exhaust_tempered'),
    'ammunition_88mm': ('AMMO', 'TG_brass_cartridge;TG_projectile_pzgr39_black;TG_projectile_cap_white;TG_projectile_sprgr_olive;TG_case_steel_lacquered'),
    'glass_laminated': ('GLASS', 'TG_glass_vision_block;PAINT'),
    'copper_cable': ('CABLE', 'TG_wire_rope_steel;TG_cable_insulation;TG_copper_conductor;PAINT'),
    'seat_steel_leather': ('SEAT', 'TG_seat_leather;TG_seat_steel'),
    'tool_steel_wood': ('TOOL', 'TG_wood_haft;TG_tool_steel_head'),
    'hydraulic_steel_oil': ('WHOLE', 'TG_hydraulic_steel'),
    'lamp_steel_glass': ('LAMP', 'TG_glass_lamp;PAINT'),
    'steel_spring': ('WHOLE', 'TG_spring_steel'),
    'instrument': ('WHOLE', 'TG_instrument_face'),
}

# ---------------------------------------------------------------------------------------------
def lab_to_linear(L, a, b):
    """CIELAB (D65 white) -> linear sRGB (IEC 61966-2-1 matrix)."""
    fy = (L + 16) / 116; fx = fy + a / 500; fz = fy - b / 200
    finv = lambda f: f ** 3 if f ** 3 > 216 / 24389 else (116 * f - 16) / (24389 / 27)
    X, Y, Z = 0.95047 * finv(fx), 1.0 * finv(fy), 1.08883 * finv(fz)
    r = 3.2404542 * X - 1.5371385 * Y - 0.4985314 * Z
    g = -0.9692660 * X + 1.8760108 * Y + 0.0415560 * Z
    bl = 0.0556434 * X - 0.2040259 * Y + 1.0572252 * Z
    return tuple(max(0.0, min(1.0, v)) for v in (r, g, bl))
def srgb8_to_linear(v): v = v / 255.0; return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4
def linear_to_srgb8(v):
    v = max(0.0, min(1.0, v)); s = 12.92 * v if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055
    return int(round(s * 255))
def albedo(m):
    if 'lab' in m: lin = lab_to_linear(*m['lab'])
    elif 'srgb' in m: lin = tuple(srgb8_to_linear(v) for v in m['srgb'])
    else: lin = m['lin']
    return lin, tuple(linear_to_srgb8(v) for v in lin)

def lowbias32(seed, salt):
    x = (seed * 0x9E3779B9 ^ salt) & 0xFFFFFFFF
    x ^= x >> 16; x = (x * 0x7feb352d) & 0xFFFFFFFF; x ^= x >> 15; x = (x * 0x846ca68b) & 0xFFFFFFFF; x ^= x >> 16
    return (x >> 8) / 16777216.0

def args_after_dashes():
    return sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else sys.argv[1:]

def refuse_input(out):
    o = os.path.realpath(out)
    for p in INPUTS:
        if os.path.exists(p) and os.path.realpath(p) == o: raise SystemExit(f'refusing to overwrite an input: {out}')
    try:
        import bpy
        if bpy.data.filepath and os.path.realpath(bpy.data.filepath) == o: raise SystemExit(f'refusing to overwrite the open input: {out}')
    except ImportError:
        pass

# ---------------------------------------------------------------------------------------------
def cmd_library(out):
    import bpy
    refuse_input(out)
    for o in list(bpy.data.objects): bpy.data.objects.remove(o)
    for m in list(bpy.data.materials): bpy.data.materials.remove(m)
    for m in LIB:
        lin, s8 = albedo(m)
        mat = bpy.data.materials.new(m['name'])
        mat.use_nodes = True
        nt = mat.node_tree
        b = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
        b.inputs['Base Color'].default_value = (*lin, 1.0)
        b.inputs['Metallic'].default_value = m['metallic']
        b.inputs['Roughness'].default_value = m['roughness']
        b.inputs['IOR'].default_value = m['ior']
        if m.get('transmission'):
            b.inputs['Transmission Weight'].default_value = m['transmission']
            try: mat.use_raytrace_refraction = True
            except AttributeError: pass
            try: mat.surface_render_method = 'DITHERED'
            except AttributeError: pass
        if m['phys'] == 'leather_seat':
            b.inputs['Sheen Weight'].default_value = 0.15
        mat.diffuse_color = (*lin, 1.0)
        mat.metallic = m['metallic']; mat.roughness = m['roughness']
        mat['phys_material'] = m['phys']
        mat['source'] = m['source']
        mat['engine_texture'] = m['engine']
        mat['albedo_linear'] = [round(v, 4) for v in lin]
        mat['albedo_srgb'] = list(s8)
        if m.get('note'): mat['note'] = m['note']
        mat.use_fake_user = True
    # A swatch board so the library opens to something visible.
    col = bpy.context.scene.collection
    for i, m in enumerate(LIB):
        bpy.ops.mesh.primitive_uv_sphere_add(radius=0.4, location=((i % 8) * 1.0, -(i // 8) * 1.0, 0.4), segments=48, ring_count=24)
        o = bpy.context.active_object; o.name = 'swatch.' + m['name']
        bpy.ops.object.shade_smooth()
        o.data.materials.append(bpy.data.materials[m['name']])
    bpy.ops.wm.save_as_mainfile(filepath=out)
    print('LIBRARY', len(LIB), 'materials ->', out)

# ---------------------------------------------------------------------------------------------
HINT_WORDS = ('AMMO', 'ROLLOR', 'CHAIR', 'SEAT', 'GLASS', 'VIEWER', 'VIEW', 'SCAN', 'AXE', 'SHOVEL', 'HAMMER', 'TOOL',
              'PLIERS', 'LIGHT', 'COOLBOX', 'WIRE', 'Hydraulic', 'BOX')
def clean_kit(n): return re.sub(r'(_îAÔO.*?)?(\.\d+)?$', '', n).strip()
def kit_ident(n):
    m = re.match(r'TG_RM5010-([\w-]+?)_(.+)$', clean_kit(n))
    return (m.group(1), m.group(2)) if m else ('', clean_kit(n))

def cmd_kithints(out):
    import bpy, numpy as np
    rng = np.random.default_rng(7)
    names, cens, offs, fc, fm = [], [], [0], [], []
    matnames = sorted(m.name for m in bpy.data.materials)
    midx = {n: i for i, n in enumerate(matnames)}
    S, T = KIT_S, np.array(KIT_T)
    tyre = {}; case = []
    for o in bpy.data.objects:
        if o.type != 'MESH': continue
        num, word = kit_ident(o.name)
        if not (word in HINT_WORDS or len(o.material_slots) > 1): continue
        me = o.data; P = len(me.polygons)
        if P == 0: continue
        c = np.empty(P * 3); me.polygons.foreach_get('center', c); c = c.reshape(-1, 3)
        a = np.empty(P); me.polygons.foreach_get('area', a)
        mi = np.empty(P, np.int32); me.polygons.foreach_get('material_index', mi)
        M = np.array(o.matrix_world); c = (c @ M[:3, :3].T + M[:3, 3]) * S + T
        a = a * S * S * abs(np.linalg.det(M[:3, :3])) ** (2 / 3)
        slot = np.array([midx.get(s.material.name, -1) if s.material else -1 for s in o.material_slots] or [-1])
        fmat = slot[np.clip(mi, 0, len(slot) - 1)]
        cen = (c * a[:, None]).sum(0) / max(a.sum(), 1e-12)
        # measured split parameters
        if word == 'ROLLOR' and len(o.material_slots) > 1:
            q = c - cen; _, _, vt = np.linalg.svd(q[::max(1, P // 20000)], full_matrices=False)
            ax = vt[2]; r = np.linalg.norm(q - np.outer(q @ ax, ax), axis=1); R = r.max()
            black = fmat == midx.get('mat_2', -9)
            outer = black & (r > 0.5 * R)
            if outer.any(): tyre.setdefault(num, []).append(float(np.percentile(r[outer], 2) / R))
        if word == 'AMMO' and num.startswith('23M03'):
            q = c - cen; _, _, vt = np.linalg.svd(q[::max(1, P // 20000)], full_matrices=False)
            ax = vt[0]; t = q @ ax; L = t.max() - t.min(); r = np.linalg.norm(q - np.outer(t, ax), axis=1)
            lo = t < t.min() + 0.05 * L; base_low = r[lo].max() > r[t > t.max() - 0.05 * L].max()
            u = (t - t.min()) / L if base_low else (t.max() - t) / L
            cm = fmat == midx.get('mat_8', -9)
            if cm.any(): case.append(float(np.percentile(u[cm], 99.5)))
        if P > 60000:
            k = rng.choice(P, 60000, replace=False); c, fmat = c[k], fmat[k]
        names.append(f'{num}|{word}'); cens.append(cen); fc.append(c.astype(np.float32)); fm.append(fmat.astype(np.int16))
        offs.append(offs[-1] + len(c))
    rgb = []
    for n in matnames:
        m = bpy.data.materials[n]; b = next((x for x in m.node_tree.nodes if x.type == 'BSDF_PRINCIPLED'), None) if m.node_tree else None
        rgb.append(list(b.inputs['Base Color'].default_value)[:3] if b else list(m.diffuse_color)[:3])
    params = dict(tyre_inner_ratio={k: round(float(np.median(v)), 4) for k, v in tyre.items()},
                  case_fraction=round(float(np.median(case)), 4) if case else None)
    np.savez_compressed(out, names=np.array(names), cens=np.array(cens), offs=np.array(offs), fc=np.concatenate(fc), fm=np.concatenate(fm),
                        matnames=np.array(matnames), rgb=np.array(rgb), params=json.dumps(params))
    print('HINTS', len(names), 'parts', offs[-1], 'faces ->', out); print('PARAMS', json.dumps(params))

# ---------------------------------------------------------------------------------------------
class Hints:
    def __init__(self, path):
        import numpy as np
        z = np.load(path, allow_pickle=False)
        self.names = [str(x) for x in z['names']]; self.cens = z['cens']; self.offs = z['offs']; self.fc = z['fc']; self.fm = z['fm']
        self.matnames = [str(x) for x in z['matnames']]; self.params = json.loads(str(z['params']))
        self.by_num = {}
        for i, n in enumerate(self.names): self.by_num.setdefault(n.split('|')[0], []).append(i)
        self.trees = {}
    def part_for(self, num, cen, tol=0.03):
        import numpy as np
        best = None
        for i in self.by_num.get(num, []):
            d = float(np.linalg.norm(self.cens[i] - cen))
            if d < tol and (best is None or d < best[0]): best = (d, i)
        return best[1] if best else None
    def colours(self, i, pts):
        """Kit colour name of the nearest kit face for each point (game frame)."""
        from mathutils.kdtree import KDTree
        if i not in self.trees:
            a, b = self.offs[i], self.offs[i + 1]
            t = KDTree(b - a)
            for k, p in enumerate(self.fc[a:b]): t.insert(p.tolist(), k)
            t.balance(); self.trees[i] = (t, self.fm[a:b])
        t, fm = self.trees[i]
        out = []
        for p in pts:
            _, k, _ = t.find(p.tolist())
            out.append(self.matnames[fm[k]] if fm[k] >= 0 else '-')
        return out
    def single_colour(self, i):
        import numpy as np
        a, b = self.offs[i], self.offs[i + 1]
        vals, cnt = np.unique(self.fm[a:b], return_counts=True)
        return [self.matnames[v] for v in vals[np.argsort(-cnt)]]

def load_catalog():
    p = os.path.join(REPO, 'catalog', 'tiger_parts.v1.json')
    return json.load(open(p))['parts'] if os.path.exists(p) else {}

def object_info(o, cat, kit_mode, kit_by_num):
    """phys_material, fea_group, kit_number, kit_name, game_node for one object, whatever file it came from."""
    if 'phys_material' in o.keys():
        return dict(phys=o['phys_material'], fea=o.get('fea_group', 'fitting'), num=o.get('kit_number', ''), word=o.get('kit_name', ''),
                    asm=o.get('kit_assembly', ''), node=o.get('game_node', o.name))
    node = o.get('game_node', o.name)
    if node in cat:
        e = cat[node]
        return dict(phys=e['phys_material'], fea=e['fea_group'], num=e.get('kit_number', ''), word=e.get('kit_name', ''), asm=e.get('kit_assembly', ''), node=node)
    if kit_mode:
        num, word = kit_ident(o.name)
        if num in kit_by_num:
            phys, fea, asm = kit_by_num[num]
            return dict(phys=phys, fea=fea, num=num, word=word, asm=asm, node=o.name)
        chain = []
        p = o.parent
        while p is not None: chain.append(clean_kit(p.name)); p = p.parent
        asm = chain[0].replace('TG_RM5010-', '') if chain else ''
        return dict(phys='steel_structural', fea='fitting', num=num, word=word, asm=asm, node=o.name, note='kit part absent from the catalog')
    return None

def volume_centroid(o, to_master=None):
    import numpy as np
    me = o.data; me.calc_loop_triangles(); nt = len(me.loop_triangles)
    if nt < 4: return None
    co = np.empty(len(me.vertices) * 3); me.vertices.foreach_get('co', co); co = co.reshape(-1, 3)
    W = np.array(o.matrix_world); w = co @ W[:3, :3].T + W[:3, 3]
    if to_master: w = to_master(w)
    tri = np.empty(nt * 3, np.int64); me.loop_triangles.foreach_get('vertices', tri); tri = tri.reshape(-1, 3)
    a, b, c = w[tri[:, 0]], w[tri[:, 1]], w[tri[:, 2]]
    v6 = np.einsum('ij,ij->i', a, np.cross(b, c))
    if abs(v6.sum()) < 1e-15: return None
    return ((a + b + c) * v6[:, None]).sum(0) / (4.0 * v6.sum())

def fit_frame(objs, info, cat, kit_mode):
    """Scale and offset from this file's world to the game master's metres (no rotation in any Tiger
    file): least squares between part volume centroids and the catalog's game-master centroids."""
    import numpy as np
    A, B = [], []
    for o in objs:
        i = info.get(o.name); e = cat.get(i['node']) if i else None
        if not e or e.get('kit_match') != 'exact' or len(o.data.polygons) > 20000: continue
        vc = volume_centroid(o)
        if vc is None: continue
        A.append(vc); B.append([float(x) for x in e['centroid_m'].split()])
        if len(A) >= 400: break
    if len(A) < 10:
        return ((KIT_S, np.eye(3), np.array(KIT_T)) if kit_mode else (1.0, np.eye(3), np.zeros(3))), None
    A, B = np.array(A), np.array(B)
    def umeyama(A, B):  # similarity B ~ s R A + t
        am, bm = A.mean(0), B.mean(0); X, Y = A - am, B - bm
        U, D, Vt = np.linalg.svd(Y.T @ X / len(A)); E = np.eye(3)
        if np.linalg.det(U) * np.linalg.det(Vt) < 0: E[2, 2] = -1
        Rm = U @ E @ Vt; s = float((D * np.diag(E)).sum() / (X ** 2).sum() * len(A))
        return s, Rm, bm - s * Rm @ am
    s, Rm, t = umeyama(A, B)
    r = np.linalg.norm(s * A @ Rm.T + t - B, axis=1)
    keep = r < max(0.02, np.percentile(r, 80))
    s, Rm, t = umeyama(A[keep], B[keep])
    return (s, Rm, t), round(float(np.median(np.linalg.norm(s * A[keep] @ Rm.T + t - B[keep], axis=1))), 4)

def cmd_assign(lib, out, hints_path=None, scheme='grey', he_share=0.0, dry=False, report=None, air_in_path=None, air_out_path=None):
    import bpy, bmesh, numpy as np, collections, time
    refuse_input(out)
    t0 = time.time()
    kit_mode = any(o.name.startswith('TG_RM5010-') for o in bpy.data.objects if o.type == 'MESH')
    cat = load_catalog()
    kit_by_num = {}
    for e in cat.values():
        if e.get('kit_number'): kit_by_num.setdefault(e['kit_number'], (e['phys_material'], e['fea_group'], e.get('kit_assembly', '')))
    hints = Hints(hints_path) if hints_path and os.path.exists(hints_path) else None
    params = hints.params if hints else {}
    case_fraction = params.get('case_fraction') or CASE_FRACTION
    tyre_ratio = params.get('tyre_inner_ratio') or {}
    # library
    with bpy.data.libraries.load(lib, link=False) as (src, dst):
        dst.materials = [n for n in src.materials if n.startswith('TG_')]
    M = {m.name: m for m in bpy.data.materials if m.name.startswith('TG_')}
    ext_paint = 'TG_paint_dunkelgelb_ral7028' if scheme == 'yellow' else 'TG_paint_dunkelgrau_ral7021'
    disc_paint = 'TG_paint_dunkelgelb_ral7028' if scheme == 'yellow' else 'TG_roadwheel_disc_ral7021'
    gun_paint = 'TG_paint_dunkelgelb_ral7028' if scheme == 'yellow' else 'TG_gun_steel_painted'

    objs = [o for o in bpy.data.objects if o.type == 'MESH' and len(o.data.polygons) > 0]
    info = {}
    for o in objs:
        i = object_info(o, cat, kit_mode, kit_by_num)
        if i: info[o.name] = i
    (S, RF, T), fit_res = fit_frame(objs, info, cat, kit_mode)
    print('FRAME s', S, 'R', RF.round(4).tolist(), 't', T.tolist(), 'median residual m', fit_res)
    if fit_res is not None and fit_res > MAX_FRAME_RESIDUAL_M:
        raise SystemExit(f'this file is not one rigid, uniformly scaled copy of the game master (median part residual '
                         f'{fit_res} m > {MAX_FRAME_RESIDUAL_M} m): its parts are not where the catalog puts them; nothing written')
    to_master = lambda w: S * (w @ RF.T) + T

    def world_faces(o):
        me = o.data; P = len(me.polygons)
        c = np.empty(P * 3); me.polygons.foreach_get('center', c); c = c.reshape(-1, 3)
        n = np.empty(P * 3); me.polygons.foreach_get('normal', n); n = n.reshape(-1, 3)
        a = np.empty(P); me.polygons.foreach_get('area', a)
        W = np.array(o.matrix_world); R = W[:3, :3]
        c = to_master(c @ R.T + W[:3, 3])
        n = n @ np.linalg.inv(R) @ RF.T; n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-12)
        a = a * abs(np.linalg.det(R)) ** (2 / 3) * S * S
        return c, n, a

    # the armour shell for the enclosure test
    shell_v, shell_f = [], []
    firewall = []
    for o in objs:
        i = info.get(o.name)
        if not i: continue
        fea, asm = i['fea'], str(i['asm']).upper()
        if fea.startswith('component:') and fea.split(':')[1].startswith(('engine', 'fuel_tank', 'radiator', 'cooling_fans')):
            W = np.array(o.matrix_world)
            co = np.empty(len(o.data.vertices) * 3); o.data.vertices.foreach_get('co', co); co = co.reshape(-1, 3)
            w = to_master(co @ W[:3, :3].T + W[:3, 3])
            firewall.append(w[:, 1].min())
        if fea.startswith('armour:') or i['phys'] in SHELL_PHYS:
            W = np.array(o.matrix_world)
            co = np.empty(len(o.data.vertices) * 3); o.data.vertices.foreach_get('co', co); co = co.reshape(-1, 3)
            w = to_master(co @ W[:3, :3].T + W[:3, 3])
            base = sum(len(v) for v in shell_v)
            shell_v.append(w)
            for p in o.data.polygons: shell_f.append([base + k for k in p.vertices])
    sv = np.concatenate(shell_v) if shell_v else np.zeros((0, 3))
    # firewall: the front of the engine bay contents; robust against stray small parts with a low percentile
    fire_y = float(np.percentile(firewall, 5)) - FIREWALL_MARGIN_M if firewall else 1e9
    # The vehicle's air: voxelise the armour shell, close slits and seams by dilation, flood the
    # outside air in from the grid border; what the flood cannot reach is the inside.
    stats = collections.Counter()
    vox = VOXEL_M
    inside = None
    if air_in_path:
        z = np.load(air_in_path)
        inside = (z['air'], z['closed'], z['lo'], z['dims'], z['lab'])
        stats['inside_air_m3'] = round(float(z['air'].sum()) * float(z['vox']) ** 3, 2); stats['air_from'] = 1
        vox = float(z['vox'])
        print('AIR from', air_in_path, stats['inside_air_m3'], 'm3')
    elif len(sv):
        lo = sv.min(0) - 6 * vox; dims = np.ceil((sv.max(0) + 6 * vox - lo) / vox).astype(int)
        solid = np.zeros(dims, bool)
        tri = []
        for f in shell_f:
            for j in range(1, len(f) - 1): tri.append((f[0], f[j], f[j + 1]))
        tri = np.array(tri); A, B, C = sv[tri[:, 0]], sv[tri[:, 1]], sv[tri[:, 2]]
        edge = np.maximum(np.maximum(np.linalg.norm(B - A, axis=1), np.linalg.norm(C - B, axis=1)), np.linalg.norm(A - C, axis=1))
        sub = np.clip(np.ceil(edge / (0.5 * vox)).astype(int), 1, 400)
        for m_ in np.unique(sub):
            sel = sub == m_
            ii, jj = np.meshgrid(np.arange(m_ + 1), np.arange(m_ + 1), indexing='ij'); keep = ii + jj <= m_
            u_, v_ = (ii[keep] / m_)[None, :, None], (jj[keep] / m_)[None, :, None]
            pts = (A[sel][:, None] * (1 - u_ - v_) + B[sel][:, None] * u_ + C[sel][:, None] * v_).reshape(-1, 3)
            idx = np.floor((pts - lo) / vox).astype(int)
            solid[idx[:, 0], idx[:, 1], idx[:, 2]] = True
        def grow(x):
            y = x.copy()
            y[1:] |= x[:-1]; y[:-1] |= x[1:]; y[:, 1:] |= x[:, :-1]; y[:, :-1] |= x[:, 1:]; y[:, :, 1:] |= x[:, :, :-1]; y[:, :, :-1] |= x[:, :, 1:]
            return y
        for dil in DILATE_STEPS:
            closed = solid
            for _ in range(dil): closed = grow(closed)
            outside = np.zeros(dims, bool); outside[0] = outside[-1] = True; outside[:, 0] = outside[:, -1] = True; outside[:, :, 0] = outside[:, :, -1] = True
            outside &= ~closed
            while True:
                nxt = grow(outside) & ~closed
                for _ in range(15): nxt = grow(nxt) & ~closed
                if nxt.sum() == outside.sum(): break
                outside = nxt
            air_in = ~outside & ~closed
            vol = float(air_in.sum()) * vox ** 3
            stats['dilation_voxels'] = dil; stats['inside_air_m3'] = round(vol, 2); print('AIR dilation', dil, 'inside m3', round(vol, 2))
            if vol >= MIN_INSIDE_AIR_M3: break
        # closed voxels take the label of the nearest air (1 inside, 2 outside) for buried faces
        lab = np.where(air_in, 1, np.where(outside, 2, 0)).astype(np.int8)
        for _ in range(60):
            z = lab == 0
            if not z.any(): break
            for ax_ in range(3):
                for sh in (1, -1):
                    nb = np.roll(lab, sh, axis=ax_); fill = (lab == 0) & (nb > 0); lab[fill] = nb[fill]
        inside = (air_in, closed, lo, dims, lab)
        if air_out_path:
            np.savez_compressed(air_out_path, air=air_in, closed=closed, lo=lo, dims=dims, lab=lab, vox=vox)
    def enclosed_many(c, n):
        """Per face: does it face the vehicle's inside air? (vectorised over the faces)"""
        res = np.zeros(len(c), bool)
        if inside is None: return res
        air, closed, lo_, dims_, lab_ = inside
        todo = np.ones(len(c), bool)
        for d in (1.5, 2.5, 3.5, 4.5, 6.0, 8.0):
            idx = np.where(todo)[0]
            if not len(idx): break
            q = np.floor((c[idx] + n[idx] * d * vox - lo_) / vox).astype(int)
            outb = np.any(q < 0, axis=1) | np.any(q >= dims_, axis=1)
            todo[idx[outb]] = False
            idx, q = idx[~outb], q[~outb]
            free = ~closed[q[:, 0], q[:, 1], q[:, 2]]
            res[idx[free]] = air[q[free, 0], q[free, 1], q[free, 2]]
            todo[idx[free]] = False
        idx = np.where(todo)[0]
        stats['faces_buried'] += len(idx)
        if len(idx):
            q = np.clip(np.floor((c[idx] + n[idx] * 1.5 * vox - lo_) / vox).astype(int), 0, dims_ - 1)
            res[idx] = lab_[q[:, 0], q[:, 1], q[:, 2]] == 1
        return res
    def pca(c, a):
        cen = (c * a[:, None]).sum(0) / max(a.sum(), 1e-12); q = c - cen
        _, s, vt = np.linalg.svd(q[::max(1, len(q) // 20000)], full_matrices=False)
        return cen, q, vt
    def in_turret(o, i):
        if str(i['fea']).startswith('armour:turret') or 'mantlet' in str(i['fea']): return True
        if any(cl.name.startswith('TURRET') for cl in o.users_collection) or str(o.get('fea_id', '')).startswith('TURRET'): return True
        p = o.parent
        while p is not None:
            if p.name in ('Turret_Pivot',) or 'Turret' in p.name: return True
            p = p.parent
        return False

    splits = collections.defaultdict(collections.Counter)
    misplaced = {}
    agree = collections.Counter()
    per_obj = {}
    for o in objs:
        i = info.get(o.name)
        if not i: continue
        c, n, a = world_faces(o)
        P = len(c)
        src = [s.material.name if s.material else '-' for s in o.material_slots] or ['-']
        mi = np.empty(P, np.int32); o.data.polygons.foreach_get('material_index', mi)
        srcf = [src[min(x, len(src) - 1)] for x in mi]
        phys, fea, word, num = i['phys'], str(i['fea']), str(i['word']), str(i['num'])
        rule = PHYS_VISUAL.get(phys, ('PAINT',))[0]
        turret = in_turret(o, i)
        ce = cat.get(i['node'])
        if ce and ce.get('kit_match') == 'exact':
            vc = volume_centroid(o, to_master)
            if vc is not None:
                off = float(np.linalg.norm(vc - np.array([float(x) for x in ce['centroid_m'].split()])))
                if off > MISPLACED_M: misplaced[o.name] = round(off, 3)
        enc_ = [None]
        def enc(f):
            if enc_[0] is None: enc_[0] = enclosed_many(c, n)
            return enc_[0][f]
        def paint_f(f):
            if enc(f):
                if not turret and c[f][1] > fire_y: return 'TG_primer_red_oxide_ral8012'
                return 'TG_paint_elfenbein_ral1001'
            return ext_paint
        hp = None; kc = None
        if hints is not None and num:
            cen = (c * a[:, None]).sum(0) / max(a.sum(), 1e-12)
            hp = hints.part_for(num, cen)
            if hp is not None and rule in ('AMMO', 'GLASS', 'SEAT', 'TOOL', 'CABLE'):
                kc = hints.colours(hp, c)
        if any(re.fullmatch(r'mat_\d+', x) for x in src):  # the kit's own SolidWorks colours are on the faces
            kc = srcf
        res = [None] * P
        how = rule
        # source-slot overrides that carry real identity in the game master
        for f in range(P):
            s = srcf[f]
            if s == 'Turret Floor': res[f] = 'TG_turret_floor'
            elif s == 'Illuminated Dial': res[f] = 'TG_instrument_face'
            elif s == 'dial red': res[f] = 'KEEP'
        if rule == 'TRACK':
            for f in range(P): res[f] = res[f] or ('TG_track_pin' if word.upper() == 'PIN' else 'TG_track_manganese_cast')
        elif rule == 'WHOLE':
            m = {'iron_cast_machinery': 'TG_engine_cast_aluminium' if fea == 'component:engine_block' else 'TG_machinery_cast_iron',
                 'steel_sheet_tank': 'TG_fuel_tank_sheet', 'steel_exhaust': 'TG_exhaust_tempered', 'hydraulic_steel_oil': 'TG_hydraulic_steel',
                 'steel_spring': 'TG_spring_steel', 'instrument': 'TG_instrument_face'}[phys]
            for f in range(P): res[f] = res[f] or m
        elif rule == 'ROADWHEEL' or rule == 'SPROCKET':
            cen, q, vt = pca(c, a)
            ext = np.ptp(q @ vt.T, axis=0)
            disc = ext[1] > 0.3 and ext[2] < 0.6 * ext[1] and abs(ext[0] - ext[1]) < 0.08 * ext[0]
            if disc and (rule == 'ROADWHEEL' or fea == 'running_gear'):
                ax = vt[2]; r = np.linalg.norm(q - np.outer(q @ ax, ax), axis=1); R = r.max()
                if rule == 'ROADWHEEL':
                    ratio = tyre_ratio.get(num) or (float(np.median(list(tyre_ratio.values()))) if tyre_ratio else TYRE_INNER_RATIO)
                    for f in range(P): res[f] = res[f] or ('TG_roadwheel_rubber' if r[f] >= ratio * R else disc_paint)
                    how = f'ROADWHEEL r>={ratio:.3f}R'
                else:
                    for f in range(P): res[f] = res[f] or ('TG_cast_steel' if r[f] >= CONTACT_BAND * R else ext_paint)
                    how = f'SPROCKET r>={CONTACT_BAND}R'
            elif rule == 'SPROCKET':
                for f in range(P): res[f] = res[f] or 'TG_cast_steel'
                how = 'WHOLE cast steel (not running gear)'
            else:
                for f in range(P): res[f] = res[f] or paint_f(f)
                how = 'PAINT (not a disc)'
        elif rule == 'GUN':
            mg = 'MG34' in word.upper() or fea in ('component:hull_mg', 'component:coax_mg')
            for f in range(P):
                if res[f]: continue
                res[f] = 'TG_gun_steel_blued' if mg or enc(f) else gun_paint
            how = 'GUN blued (MG 34)' if mg else 'GUN enclosure: out painted / in blued'
        elif rule == 'AMMO':
            cen, q, vt = pca(c, a)
            ax = vt[0]; t = q @ ax; L = np.ptp(t); rad = np.linalg.norm(q - np.outer(t, ax), axis=1)
            round_like = L > 0.6
            base_low = rad[t < t.min() + 0.05 * L].max() >= rad[t > t.max() - 0.05 * L].max()
            u = (t - t.min()) / L if base_low else (t.max() - t) / L
            seed = 0
            for ch in str(i['node']): seed = (seed * 131 + ord(ch)) & 0xFFFFF
            he = he_share > 0 and lowbias32(seed, 0x88) < he_share
            proj = 'TG_projectile_sprgr_olive' if he else 'TG_projectile_pzgr39_black'
            colour_map = {'mat_8': 'TG_brass_cartridge', 'mat_5': 'TG_brass_cartridge', 'mat_1': 'TG_brass_cartridge',
                          'mat_15': 'TG_brass_cartridge', 'mat_2': proj, 'mat_3': 'PAINT', 'brass': 'TG_brass_cartridge', 'bullet': proj}
            for f in range(P):
                if res[f]: continue
                s = srcf[f]
                if s in ('brass', 'bullet'): res[f] = colour_map[s]
                elif kc is not None and kc[f] in colour_map: res[f] = colour_map[kc[f]]
                elif round_like: res[f] = 'TG_brass_cartridge' if u[f] <= case_fraction else proj
                else: res[f] = 'PAINT'
                if res[f] == proj and round_like and not he and (1.0 - u[f]) * L <= PZGR_CAP_MARK_M: res[f] = 'TG_projectile_cap_white'
            how = 'AMMO kit colours' + (' + axis' if round_like else '') if kc is not None or any(s in ('brass', 'bullet') for s in srcf) else f'AMMO axis case<= {case_fraction:.2f}L'
            if he: how += ' (HE)'
        elif rule == 'GLASS':
            game_glass = any(s in ('periscope glass', 'Glass') for s in srcf)
            for f in range(P):
                if res[f]: continue
                if game_glass: res[f] = 'TG_glass_vision_block' if srcf[f] in ('periscope glass', 'Glass') else 'PAINT'
                elif kc is not None: res[f] = 'TG_glass_vision_block' if kc[f] in ('mat_6', 'mat_23') else 'PAINT'
                else: res[f] = 'TG_glass_vision_block'
            how = 'GLASS game slots' if game_glass else ('GLASS kit colours' if kc is not None else 'GLASS whole')
        elif rule == 'SEAT':
            single = hints.single_colour(hp) if (hints is not None and hp is not None) else []
            for f in range(P):
                if res[f]: continue
                if srcf[f] == 'seats_black': res[f] = 'TG_seat_leather'
                elif kc is not None: res[f] = 'TG_seat_leather' if kc[f] == 'mat_2' else 'TG_seat_steel'
                else: res[f] = 'TG_seat_steel'
            how = 'SEAT game slot' if 'seats_black' in srcf else ('SEAT kit colours (black = leather)' if kc is not None else 'SEAT steel')
        elif rule == 'TOOL':
            multi = kc is not None and len(set(kc)) > 1
            if word.upper() == 'PLIERS':
                for f in range(P): res[f] = res[f] or 'TG_tool_steel_head'
                how = 'TOOL all steel (pliers)'
            elif multi:
                for f in range(P): res[f] = res[f] or ('TG_wood_haft' if kc[f] == 'mat_13' else 'TG_tool_steel_head')
                how = 'TOOL kit colours (brown = wood)'
            else:
                cen, q, vt = pca(c, a); ax = vt[0]; t = q @ ax; rad = np.linalg.norm(q - np.outer(t, ax), axis=1)
                bins = np.linspace(t.min(), t.max() + 1e-9, 41); bi = np.clip(np.digitize(t, bins) - 1, 0, 39)
                width = np.array([rad[bi == b].max() if (bi == b).any() else 0 for b in range(40)])
                haft = np.percentile(width[width > 0], 30)
                for f in range(P): res[f] = res[f] or ('TG_tool_steel_head' if width[bi[f]] > HEAD_WIDTH_RATIO * haft else 'TG_wood_haft')
                how = f'TOOL cross-section > {HEAD_WIDTH_RATIO}x haft = head'
        elif rule == 'LAMP':
            y = c[:, 1]; front = y.min() + LENS_DEPTH * np.ptp(y)
            for f in range(P): res[f] = res[f] or ('TG_glass_lamp' if (y[f] <= front and n[f][1] < -0.8) else 'PAINT')
            how = 'LAMP forward faces in front 20% = lens'
        elif rule == 'RADIATOR':
            cen, q, vt = pca(c, a); thin = vt[2]; pr = q @ vt.T
            span = [(pr[:, k].min(), pr[:, k].max()) for k in (0, 1)]
            inner = np.ones(P, bool)
            for kk, (lo, hi) in enumerate(span):
                m = (lo + hi) / 2; h = (hi - lo) / 2 * CORE_SPAN
                inner &= np.abs(pr[:, kk] - m) <= h
            for f in range(P): res[f] = res[f] or ('TG_radiator_copper_core' if inner[f] and abs(float(n[f] @ thin)) > 0.9 else 'TG_radiator_brass')
            how = 'RADIATOR core faces / tanks'
        elif rule == 'CABLE':
            L = float(np.ptp(c, axis=0).max())
            if word.upper() == 'WIRE' and L > 2.0:
                for f in range(P): res[f] = res[f] or 'TG_wire_rope_steel'
                how = 'CABLE tow rope (steel, > 2 m)'
            elif word.upper() == 'ANTENNA':
                for f in range(P): res[f] = res[f] or 'TG_spring_steel'
                how = 'CABLE antenna rod (steel)'
            elif 'HOOK' in word.upper():
                for f in range(P): res[f] = res[f] or 'PAINT'
                how = 'CABLE hook = painted steel'
            else:
                for f in range(P): res[f] = res[f] or 'TG_cable_insulation'
                how = 'CABLE insulation'
        else:  # PAINT
            if fea.startswith('component:fuel_tank'):
                for f in range(P): res[f] = res[f] or 'TG_fuel_tank_sheet'
                how = 'WHOLE fuel tank'
        for f in range(P):
            if res[f] is None or res[f] == 'PAINT':
                res[f] = paint_f(f)
        if rule == 'PAINT' and how == 'PAINT':
            how = 'PAINT enclosure'
        # agreement with the game master's own interior/exterior slots (a check, never an input)
        objagree = collections.Counter()
        if 'Interior Paint' in src:
            for f in range(P):
                s = srcf[f]
                if s in ('Interior Paint', 'Material.001'):
                    mine = 'in' if res[f] in ('TG_paint_elfenbein_ral1001', 'TG_primer_red_oxide_ral8012', 'TG_seat_steel') else 'out'
                    agree[(s, mine)] += float(a[f]); objagree[f'{s}->{mine}'] += round(float(a[f]), 4)
        cnt = collections.Counter(res)
        splits[how].update({k: 1 for k in cnt})
        splits[how]['_objects'] += 1
        per_obj[o.name] = dict(node=i['node'], phys=phys, rule=how, faces={k: v for k, v in cnt.items()}, game_slot_check=dict(objagree))
        if dry: continue
        # write slots and indices
        if o.data.users > 1: o.data = o.data.copy()
        keep = [o.material_slots[mi[f]].material if mi[f] < len(o.material_slots) else None for f in range(P)]
        order = []
        for f in range(P):
            key = ('KEEP', keep[f].name if keep[f] else '-') if res[f] == 'KEEP' else ('LIB', res[f])
            if key not in order: order.append(key)
        idx = {k: j for j, k in enumerate(order)}
        for s in o.material_slots: s.link = 'DATA'
        o.data.materials.clear()
        for kind, nm in order: o.data.materials.append(bpy.data.materials.get(nm) if kind == 'KEEP' else M[nm])
        new = np.array([idx[('KEEP', keep[f].name if keep[f] else '-') if res[f] == 'KEEP' else ('LIB', res[f])] for f in range(P)], np.int32)
        o.data.polygons.foreach_set('material_index', new)
        o['visual_rule'] = how
        o['visual_materials'] = ';'.join(nm for _, nm in order)
        o.data.update()
    rep = dict(file=bpy.data.filepath, air_grid_from=air_in_path or 'this file', kit_mode=kit_mode, frame_scale=S, frame_rotation=RF.round(5).tolist(), frame_offset=[float(x) for x in T], frame_residual_m=fit_res, scheme=scheme, he_share=he_share, firewall_y=fire_y,
               case_fraction=case_fraction, tyre_inner_ratio=tyre_ratio, inside_air_m3=stats['inside_air_m3'], dilation_voxels=stats['dilation_voxels'], faces_buried=stats['faces_buried'],
               seconds=round(time.time() - t0, 1),
               splits={k: dict(v) for k, v in splits.items()},
               interior_check_area_m2={f'{a_}->{b_}': round(v, 3) for (a_, b_), v in agree.items()},
               objects_unassigned=[o.name for o in objs if o.name not in info],
               objects_off_catalog_pose=len(misplaced), objects_off_catalog_pose_examples=dict(list(sorted(misplaced.items(), key=lambda x: -x[1]))[:15]))
    if report:
        json.dump(dict(rep, objects=per_obj), open(report, 'w'), indent=1)
    print('ASSIGN', json.dumps(rep, indent=1))
    if not dry:
        for m in bpy.data.materials:
            if not m.users and not m.use_fake_user: pass
        bpy.context.scene['tiger_frame'] = dict(s=S, R=RF.reshape(-1).tolist(), t=[float(x) for x in T])
        bpy.ops.wm.save_as_mainfile(filepath=out, copy=True)
        print('SAVED', out)

# ---------------------------------------------------------------------------------------------
def cmd_render(prefix, width=960, height=540):
    import bpy, numpy as np
    from mathutils import Vector
    sc = bpy.context.scene
    sc.render.engine = 'BLENDER_EEVEE'
    sc.render.resolution_x, sc.render.resolution_y, sc.render.resolution_percentage = width, height, 100
    sc.render.image_settings.file_format = 'PNG'
    try: sc.view_settings.view_transform = 'AgX'
    except TypeError: pass
    sc.view_settings.exposure = 0.0
    ee = sc.eevee
    for attr, val in (('taa_render_samples', 64), ('use_raytracing', True), ('use_shadows', True), ('shadow_ray_count', 2)):
        try: setattr(ee, attr, val)
        except (AttributeError, TypeError): pass
    # Physical-ish daylight: a sun and a uniform sky (no fill lights). Interior practicals stay as authored.
    w = bpy.data.worlds.new('validation_sky'); w.use_nodes = True
    bg = w.node_tree.nodes['Background']; bg.inputs['Color'].default_value = (0.36, 0.48, 0.75, 1); bg.inputs['Strength'].default_value = 1.2
    sc.world = w
    for o in [o for o in bpy.data.objects if o.type == 'LIGHT' and o.data.type == 'SUN']: o.hide_render = True
    sun = bpy.data.objects.new('validation_sun', bpy.data.lights.new('validation_sun', 'SUN'))
    sun.data.energy = 4.0; sun.data.angle = math.radians(0.53); sc.collection.objects.link(sun)
    cam = bpy.data.objects.new('validation_cam', bpy.data.cameras.new('validation_cam')); sc.collection.objects.link(cam); sc.camera = cam
    # shots are authored in game-master metres; the assign step stored this file's frame
    fr = sc.get('tiger_frame')
    fs, fR, ft = (float(fr['s']), np.array(fr['R']).reshape(3, 3), np.array(fr['t'])) if fr else (1.0, np.eye(3), np.zeros(3))
    to_world = lambda m: (fR.T @ (np.array(m) - ft)) / fs
    def look(eye, target, lens=40, clip=0.05, ortho=None, sun_dir=(-0.4, 0.6, -0.7)):
        e, tg = to_world(eye), to_world(target)
        cam.location = Vector(e.tolist()); d = Vector((tg - e).tolist())
        cam.rotation_euler = d.to_track_quat('-Z', 'Y').to_euler()
        cam.data.type = 'ORTHO' if ortho else 'PERSP'
        if ortho: cam.data.ortho_scale = ortho / fs
        cam.data.lens = lens; cam.data.clip_start = clip / fs; cam.data.clip_end = 200 / fs
        sun.rotation_euler = Vector((fR.T @ np.array(sun_dir)).tolist()).to_track_quat('-Z', 'Y').to_euler()
    # aim the ammo shot at the left sponson racks
    ammo = [o for o in bpy.data.objects if o.type == 'MESH' and o.get('phys_material') == 'ammunition_88mm']
    cat = load_catalog()
    pts = np.array([[float(x) for x in cat[o['game_node']]['centroid_m'].split()] for o in ammo if o.get('game_node') in cat]) if ammo else []
    if not len(pts): pts = np.array([[1.2, -0.5, 1.4]])
    left = pts[pts[:, 0] > 0.3] if (pts[:, 0] > 0.3).any() else pts
    ac = left.mean(0)
    shots = [
        ('exterior', dict(eye=(7.5, -10.0, 4.2), target=(0, -0.3, 1.2), lens=45, sun_dir=(-0.5, 0.5, -0.65))),
        ('fighting_compartment', dict(eye=(-8.0, -0.6, 1.9), target=(0, -0.6, 1.5), lens=32, clip=8.0 - 1.05, sun_dir=(0.6, 0.2, -0.75))),
        ('engine_bay', dict(eye=(0.0, 2.0, 12.0), target=(0.0, 2.0, 0.0), lens=50, clip=12.0 - 1.75, ortho=4.4, sun_dir=(-0.3, 0.2, -0.93))),
        ('running_gear', dict(eye=(5.2, -2.6, 0.9), target=(1.6, -0.9, 0.6), lens=40, sun_dir=(-0.6, 0.4, -0.6))),
        ('ammo_rack', dict(eye=(8.0, float(ac[1]), float(ac[2]) + 0.4), target=(0.0, float(ac[1]), float(ac[2])), lens=40, clip=8.0 - 1.60, sun_dir=(-0.7, 0.1, -0.7))),
    ]
    for name, kw in shots:
        look(**kw)
        sc.render.filepath = f'{prefix}{name}.png'
        bpy.ops.render.render(write_still=True)
        print('RENDERED', sc.render.filepath)

# ---------------------------------------------------------------------------------------------
def join_csv(repo=REPO):
    """docs/fea/tiger_materials.csv gains the visual columns; docs/fea/tiger_visual_materials.csv is the library."""
    p = os.path.join(repo, 'docs', 'fea', 'tiger_materials.csv')
    rows = list(csv.DictReader(open(p)))
    base = [k for k in rows[0].keys() if k not in ('visual_rule', 'visual_materials', 'engine_texture')]
    with open(p, 'w', newline='') as f:
        w = csv.DictWriter(f, fieldnames=base + ['visual_rule', 'visual_materials', 'engine_texture'], lineterminator='\n'); w.writeheader()
        for r in rows:
            rule, mats = PHYS_VISUAL.get(r['phys_material'], ('PAINT', ''))
            eng = ';'.join(sorted({BY_NAME[m]['engine'] for m in mats.split(';') if m in BY_NAME}))
            w.writerow({**{k: r[k] for k in base}, 'visual_rule': rule, 'visual_materials': mats, 'engine_texture': eng})
    q = os.path.join(repo, 'docs', 'fea', 'tiger_visual_materials.csv')
    users = {}
    for phys, (_, mats) in PHYS_VISUAL.items():
        for m in mats.split(';'): users.setdefault(m, []).append(phys)
    with open(q, 'w', newline='') as f:
        w = csv.writer(f, lineterminator='\n')
        w.writerow(['material', 'phys_material', 'fea_classes', 'albedo_linear', 'albedo_srgb', 'metallic', 'roughness', 'ior', 'transmission', 'engine_texture', 'source', 'note'])
        for m in LIB:
            lin, s8 = albedo(m)
            w.writerow([m['name'], m['phys'], ';'.join(users.get(m['name'], [])), ' '.join(f'{v:.4f}' for v in lin), ' '.join(str(v) for v in s8),
                        m['metallic'], m['roughness'], m['ior'], m.get('transmission', 0.0), m['engine'], m['source'], m.get('note', '')])
    print('CSV', p, q)

if __name__ == '__main__':
    a = args_after_dashes()
    if not a: raise SystemExit(__doc__)
    cmd = a[0]
    opt = lambda k, d=None: a[a.index(k) + 1] if k in a else d
    if cmd == 'library': cmd_library(a[1])
    elif cmd == 'kithints': cmd_kithints(a[1])
    elif cmd == 'assign':
        cmd_assign(a[1], a[2], hints_path=opt('--hints'), scheme=opt('--scheme', 'grey'), he_share=float(opt('--he-share', 0.0)),
                   dry='--dry-run' in a, report=opt('--report'), air_in_path=opt('--air-in'), air_out_path=opt('--air-out'))
    elif cmd == 'render': cmd_render(a[1])
    elif cmd == 'csv': join_csv()
    else: raise SystemExit(__doc__)
