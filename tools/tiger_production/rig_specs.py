"""The gen2 Tiger's rig controls as specs for rig_build (see its header for the fields). Numbers
are world metres in the kit pose of `tiger_gen2_live.blend`, read off the meshes on the date in
each block; `source` says how. The rear plate has its own script (rear_plate_rig.py) because it
also cuts parts.
"""

# ---------------------------------------------------------------------------------------------
# Engine deck, read 2026-11-08 (destructibles row B7).
# What the kit shows: the engine hatch 34Z03 carries the round intake cover 34B24, a raised plate
# 34B18 and two gooseneck arms 34D28 whose front feet sit on the hatch's front edge (the kit calls
# hinge arms ARM elsewhere); two blocks 34F06 with a bolt 34F08 each stand by the arms; a folding
# bar 34D11 stands at the rear edge. The hinge is taken at the front edge, as on the real vehicle.
# Rear lattice grilles 34F13/34F14 lie over plates 34F12/34F11, each with a folding bar 34D11 at
# its front edge and two wing nuts 35D15 on the strip before it. Four round caps 34D42 sit on the
# louvred radiator grilles, which are part of the roof 33B01 and are not cut out yet.
# Sides: the tank's left is +X in this file, so _L names are the +X parts.
# Not verified against a drawing: which way the hatch opens, what the bolts 34F08 engage, what
# the folding bars are for, which fluid each cap covers.
DECK = [
    dict(name='Deck_Hatch', kind='hinge', pivot=(0.005, 2.2435, 1.796), axis=(1, 0, 0), parent='0B26_Cover Unit',
         parts=['34Z03_COVER', '34B24_COVER', '34B18_COVER', '34D28_ARM', '34D28_ARM.001', '34F06_BLOCK', '34F06_BLOCK.001'],
         limits=(0.0, 100.0, 'zero = the kit pose, shut. Open limit 100 deg is chosen, just past upright: the kit has no stay and nothing stops the lid before 165 deg with the turret out of the way (named approximation). With the turret at 12 o\'clock the lid meets the turret bin 15A44 at 36 deg (1 deg sweep)'),
         props={'max_turret_forward': 35.0},
         source='axis across the hull through the front feet of the gooseneck arms 34D28 (y 2.2435 m) at the hatch\'s top face; front hinge as on the real vehicle, not checked on a drawing',
         locked_by='Deck_Hatch_Lock_R, Deck_Hatch_Lock_L',
         interaction='engine hatch: draw both side bolts, lift the rear edge by the folding bar, swing it up toward the turret',
         damage='hinge arms bent or the lid jammed by a hit on the deck; a burning engine bay behind it',
         repair='lever it up with the crowbar; knock the bolts back with the hammer'),
    dict(name='Deck_Hatch_Lock_R', kind='slide', pivot=(-0.478, 2.5925, 1.855), axis=(1, 0, 0), parent='CTRL.Deck_Hatch', parts=['34F08_LCOK'],
         limits=(0.0, 0.04, 'zero = the kit pose, bolt end 37 mm past the hatch edge; 0.04 m draws it inside the edge. The kit shows no keeper (named approximation)'),
         source='bolt 34F08 in block 34F06, slides across the hull', interaction='draw the bolt inward', damage='bent by a hit', repair='hammer it back'),
    dict(name='Deck_Hatch_Lock_L', kind='slide', pivot=(0.488, 2.5925, 1.855), axis=(-1, 0, 0), parent='CTRL.Deck_Hatch', parts=['34F08_LCOK.001'],
         limits=(0.0, 0.04, 'zero = the kit pose, bolt end 37 mm past the hatch edge; 0.04 m draws it inside the edge. The kit shows no keeper (named approximation)'),
         source='bolt 34F08 in block 34F06, slides across the hull', interaction='draw the bolt inward', damage='bent by a hit', repair='hammer it back'),
    dict(name='Deck_Hatch_Bar', kind='hinge', pivot=(-0.2035, 3.388, 1.818), axis=(-1, 0, 0), parent='CTRL.Deck_Hatch', parts=['34D11_BUCKLE.002'],
         sweep=(-90.0, 90.0, 2.0), radius=0.3,
         source='folding bar 34D11 at the hatch\'s rear edge; pivot = middle of its knuckle end', interaction='fold the bar up and lift by it'),
    dict(name='Fan_Grille_R', kind='stow', pivot=(-1.032, 3.4835, 1.8255), parent='0B26_Cover Unit', parts=['34F13_FAN COVER'],
         released_by='fastener set fan_grille_R (two wing nuts)', source='seat = the lattice grille 34F13 in its kit pose',
         interaction='undo the two wing nuts, lift the lattice grille off the fan bay by its folding bar',
         damage='lattice holed or driven in onto the fan', repair='lever it off with the crowbar; run without it'),
    dict(name='Fan_Grille_L', kind='stow', pivot=(1.040, 3.4835, 1.8255), parent='0B26_Cover Unit', parts=['34F14_FAN COVER'],
         released_by='fastener set fan_grille_L (two wing nuts)', source='seat = the lattice grille 34F14 in its kit pose',
         interaction='undo the two wing nuts, lift the lattice grille off the fan bay by its folding bar',
         damage='lattice holed or driven in onto the fan', repair='lever it off with the crowbar; run without it'),
    dict(name='Fan_Grille_Bar_R', kind='hinge', pivot=(-1.2365, 3.352, 1.848), axis=(-1, 0, 0), parent='CTRL.Fan_Grille_R', parts=['34D11_BUCKLE'],
         sweep=(-90.0, 90.0, 2.0), radius=0.3, source='folding bar 34D11 on the grille\'s front edge; pivot = middle of its knuckle end',
         interaction='fold the bar up and lift by it'),
    dict(name='Fan_Grille_Bar_L', kind='hinge', pivot=(1.2445, 3.352, 1.848), axis=(-1, 0, 0), parent='CTRL.Fan_Grille_L', parts=['34D11_BUCKLE.001'],
         sweep=(-90.0, 90.0, 2.0), radius=0.3, source='folding bar 34D11 on the grille\'s front edge; pivot = middle of its knuckle end',
         interaction='fold the bar up and lift by it'),
    dict(name='Fan_Cover_R', kind='stow', pivot=(-1.032, 3.5205, 1.759), parent='0B26_Cover Unit', parts=['34F12_FAN COVER'],
         released_by='Fan_Grille_R off', source='seat = the plate 34F12 under the lattice in its kit pose',
         interaction='lift the plate out to reach the fan', damage='bent onto the fan', repair='lever it out'),
    dict(name='Fan_Cover_L', kind='stow', pivot=(1.040, 3.5205, 1.759), parent='0B26_Cover Unit', parts=['34F11_FAN COVER'],
         released_by='Fan_Grille_L off', source='seat = the plate 34F11 under the lattice in its kit pose',
         interaction='lift the plate out to reach the fan', damage='bent onto the fan', repair='lever it out'),
] + [
    dict(name='Deck_Cap_' + tag, kind='stow', pivot=pivot, parent='0B26_Cover Unit', parts=[part],
         released_by='unscrewed by hand (turns not known)', source='seat = the cap 34D42 in its kit pose; which filler it covers is not identified',
         props={'unscrew': True}, interaction='unscrew the cap and lift it off', damage='cap shot away or jammed', repair='free it with the hammer; plug the filler with a rag')
    for tag, part, pivot in (('RR', '34D42_COVER', (-1.0265, 2.9935, 1.826)), ('FR', '34D42_COVER.001', (-1.0265, 2.2405, 1.810)),
                             ('FL', '34D42_COVER.002', (1.0345, 2.241, 1.810)), ('RL', '34D42_COVER.003', (1.0345, 2.9935, 1.826)))
]

DECK_FASTENERS = [
    # kit part, kind, set, removable, tool
    ('35D15_LOCK', 'wingnut', 'fan_grille_L', True, 'hand'),
    ('35D15_LOCK.001', 'wingnut', 'fan_grille_L', True, 'hand'),
    ('35D15_LOCK.002', 'wingnut', 'fan_grille_R', True, 'hand'),
    ('35D15_LOCK.003', 'wingnut', 'fan_grille_R', True, 'hand'),
]
