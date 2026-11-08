#!/usr/bin/env python3
"""Original, deterministic voxel concept studies. No network, provider or runtime hooks.

Usage: python tools/concepts/hephaestus_primary_accounts.py
Produces editable MagicaVoxel VOX, sparse JSON, surface-only GLB, validation receipts.
Cell coordinates are Z-up; GLB is converted to metres, right-handed Y-up.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import math
import pathlib
import random
import struct

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "assets/concepts/primary-accounts-20261107/hephaestus"

PALETTE = [
    ("cut_earth", (31, 36, 43)), ("cut_earth_var", (39, 43, 49)),
    ("concrete", (85, 91, 94)), ("concrete_light", (105, 108, 109)),
    ("concrete_dark", (62, 67, 72)), ("mortar", (66, 63, 60)),
    ("brick", (117, 80, 65)), ("brick_pale", (138, 95, 75)),
    ("brick_dark", (87, 63, 57)), ("brick_ash", (95, 86, 79)),
    ("charcoal", (27, 29, 30)), ("soot", (39, 37, 35)),
    ("wood", (97, 77, 54)), ("wood_light", (133, 110, 77)),
    ("wood_dark", (64, 54, 43)), ("steel", (66, 77, 84)),
    ("steel_edge", (114, 125, 129)), ("rust", (123, 70, 43)),
    ("snow", (163, 189, 206)), ("snow_light", (196, 213, 219)),
    ("snow_shadow", (117, 151, 175)), ("ice", (77, 119, 146)),
    ("mud", (56, 54, 49)), ("mud_light", (73, 68, 57)),
    ("blanket", (93, 100, 89)), ("blanket_light", (118, 123, 104)),
    ("blanket_dark", (63, 74, 70)), ("uniform", (65, 73, 70)),
    ("uniform_light", (82, 87, 77)), ("coat_dark", (44, 54, 55)),
    ("skin", (170, 133, 103)), ("skin_shadow", (137, 101, 83)),
    ("cloth", (158, 157, 137)), ("cloth_dark", (120, 122, 109)),
    ("leather", (60, 49, 43)), ("paper", (177, 165, 133)),
    ("ash", (100, 98, 90)), ("ember", (208, 69, 19)),
    ("flame", (255, 146, 37)), ("flame_core", (255, 212, 100)),
    ("lamp_glass", (227, 172, 82)), ("dark_void", (12, 18, 23)),
]
PID = {name: i + 1 for i, (name, _) in enumerate(PALETTE)}


def material_family(name):
    if name.startswith('cut_earth') or name.startswith('mud'): return 'soil'
    if name.startswith('concrete') or name=='mortar': return 'cementitious'
    if name.startswith('brick'): return 'fired_clay'
    if name.startswith('wood') or name in ('charcoal','soot','ash'): return 'wood_or_combustion_residue'
    if name.startswith('steel') or name=='rust': return 'ferrous_metal'
    if name.startswith('snow') or name=='ice': return 'frozen_water'
    if name.startswith(('blanket','uniform','cloth')) or name=='coat_dark': return 'woven_cloth'
    if name.startswith('skin'): return 'anonymous_figure_proxy'
    if name=='leather': return 'leather'
    if name=='paper': return 'paper'
    if name in ('ember','flame','flame_core','lamp_glass'): return 'emissive_visual_only'
    return 'nonstructural_visual'


class Scene:
    def __init__(self, name, dims, cell_m, seed, title, source):
        self.name, self.dims, self.cell_m = name, dims, cell_m
        self.rng = random.Random(seed)
        self.seed, self.title, self.source = seed, title, source
        self.v = {}  # (x,y,z) -> (one-based palette index, semantic part name)
        self.preview = {}

    def put(self, x, y, z, color, part):
        key = int(x), int(y), int(z)
        if all(0 <= key[i] < self.dims[i] for i in range(3)):
            self.v[key] = (PID[color] if isinstance(color, str) else color, part)

    def box(self, a, b, color, part):
        colors = color if isinstance(color, list) else [color]
        for z in range(int(a[2]), int(b[2])):
            for y in range(int(a[1]), int(b[1])):
                for x in range(int(a[0]), int(b[0])):
                    self.put(x, y, z, self.rng.choice(colors), part)

    def erase(self, a, b):
        for z in range(int(a[2]), int(b[2])):
            for y in range(int(a[1]), int(b[1])):
                for x in range(int(a[0]), int(b[0])):
                    self.v.pop((x, y, z), None)

    def ellipsoid(self, c, r, color, part, shell=False):
        colors = color if isinstance(color, list) else [color]
        for z in range(math.floor(c[2]-r[2]), math.ceil(c[2]+r[2])+1):
            for y in range(math.floor(c[1]-r[1]), math.ceil(c[1]+r[1])+1):
                for x in range(math.floor(c[0]-r[0]), math.ceil(c[0]+r[0])+1):
                    d = sum(((a-b)/max(.01, rad))**2 for a,b,rad in zip((x,y,z),c,r))
                    if d <= 1 and (not shell or d >= .65):
                        self.put(x,y,z,self.rng.choice(colors),part)

    def line(self, a, b, width, color, part):
        n = max(1, math.ceil(math.dist(a,b)*2))
        for k in range(n+1):
            p = tuple(round(a[i]+(b[i]-a[i])*k/n) for i in range(3))
            if width <= 1:
                self.put(*p,color,part)
            else:
                self.ellipsoid(p, (width/2,)*3,color,part)

    def cylinder(self, c, r, h, color, part, hollow=False):
        for y in range(math.floor(c[1]-r),math.ceil(c[1]+r)+1):
            for x in range(math.floor(c[0]-r),math.ceil(c[0]+r)+1):
                d = math.hypot(x-c[0],y-c[1])
                if d <= r and (not hollow or d >= r-1.2):
                    self.box((x,y,c[2]),(x+1,y+1,c[2]+h),color,part)

    def masonry(self,a,b,part):
        # Real brick courses, intentionally staggered; not a noise texture on a cube.
        self.box(a,b,"mortar",part)
        for z in range(a[2],b[2],3):
            offset = -3 if ((z-a[2])//3)%2 else 0
            for y in range(a[1],b[1]):
                for x in range(a[0]+offset,b[0],6):
                    self.box((max(x,a[0]),y,z),(min(x+5,b[0]),y+1,min(z+2,b[2])),
                             self.rng.choice(["brick","brick_pale","brick_dark","brick_ash"]),part)

    def timber(self,a,b,width=2,part="fallen_timbers"):
        self.line(a,b,width,"wood_dark",part)
        self.line((a[0],a[1],a[2]+1),(b[0],b[1],b[2]+1),1,"wood",part)

    def stretcher(self,x,y,z,part,covered=True,scale=1):
        # A human-sized object on the 0.10 m cellar lattice.
        def b(a,c,col):
            self.box((x+a[0]*scale,y+a[1]*scale,z+a[2]*scale),
                     (x+c[0]*scale,y+c[1]*scale,z+c[2]*scale),col,part)
        b((-5,-13,0),(-4,13,1),"wood_light")
        b((4,-13,0),(5,13,1),"wood_light")
        b((-4,-10,0),(4,10,1),"cloth_dark")
        if covered:
            b((-3,-7,1),(3,7,4),"blanket")
            b((-2,-7,4),(2,5,5),"blanket_light")
            b((-3,-5,2),(-2,5,4),"blanket_dark")
            b((-2,7,1),(2,10,4),"skin")
            b((-3,9,2),(3,11,4),"cloth_dark")
            b((-2,-10,1),(2,-7,3),"leather")
            # One visible hand above the blanket gives the sleeping shape a scale.
            b((2,2,2),(4,5,3),"skin_shadow")

    def seated(self,x,y,z,part,scale=1):
        # Coordinates are individual occupied cells, pose remains editable in VOX.
        def b(a,c,col):
            self.box((x+a[0]*scale,y+a[1]*scale,z+a[2]*scale),
                     (x+c[0]*scale,y+c[1]*scale,z+c[2]*scale),col,part)
        b((-3,0,3),(3,4,10),"coat_dark")
        b((-3,-4,2),(0,2,5),"uniform")
        b((1,-4,2),(4,2,5),"uniform")
        b((-3,-6,0),(0,-2,3),"leather")
        b((1,-6,0),(4,-2,3),"leather")
        b((-2,-1,10),(2,3,14),"skin_shadow")
        b((-2,-2,13),(3,3,15),"uniform")
        b((-4,-2,6),(-2,2,10),"uniform")
        b((2,-2,6),(4,2,10),"uniform")
        b((-3,-3,7),(-1,0,9),"skin")
        b((1,-3,7),(3,0,9),"skin")


def cellar():
    s=Scene("factory-cellar-ward",(164,140,76),.10,305578,
            "Once in twenty-four hours — factory cellar ward",{
        "dossier_anchor":"A1-08", "record":"Stalingrad Museum-Reserve KP 25224",
        "author":"Unnamed corporal, 10./Infanterie-Regiment 578, 305th Infantry Division",
        "dates":"Diary 22 November–18 December 1942; captured translation received 1 January 1943",
        "url":"https://stalingrad1942-1943.ru/node/2724",
        "attested":["Dark cellar in a factory; wounded arriving (16 December)",
                    "Small fire directly on the floor (16 December)",
                    "One meal in twenty-four hours (16 and 18 December)",
                    "Doctor found in the cellar of a different ruined house after a long search (18 December)"],
        "interpretation":["All architecture, scale, arrangement, props and anonymous figures are original concept design, not a reconstruction.",
                          "Blankets, stretchers, ration vessel, snow and broken factory structure are visual interpretation of conditions.",
                          "The distant doorway is a visual route cue, not a claim that the doctor was within sight.",
                          "Figure count has no historical meaning; no specific victim is represented."]})
    # An irregular section of frozen ground and broken reinforced slab.
    for y in range(5,135):
        for x in range(5,159):
            edge=min(x-5,158-x,y-5,134-y)
            if edge < s.rng.randrange(0,4): continue
            s.box((x,y,0),(x+1,y+1,4),["cut_earth","cut_earth_var"],"sectioned_ground")
            s.put(x,y,4,s.rng.choice(["snow","snow_shadow","snow_light"]),"frozen_exterior")
    s.box((12,18,4),(124,121,7),["concrete","concrete_dark","concrete_light"],"factory_floor")
    s.box((20,27,7),(118,111,8),["concrete","concrete","concrete_dark"],"factory_floor")
    s.masonry((12,18,7),(18,122,49),"left_cellar_wall")
    s.masonry((18,114,7),(125,121,49),"rear_cellar_wall")
    s.masonry((119,75,7),(125,115,38),"right_wall_remnant")
    # Breaks at the front section edge and across the upper ruin silhouette.
    for y in range(18,56):
        top=22+(y-18)//2+s.rng.randrange(-2,3)
        s.erase((11,y,top),(19,y+1,50))
    for x in range(18,125):
        top=49+int(7*math.sin(x*.09))+s.rng.randrange(-2,3)
        s.masonry((x,114,49),(x+1,121,min(top+12,75)),"upper_factory_ruin")
    # Ruined windows frame a dark, cold exterior above the inhabited basement.
    for x0 in (29,72):
        s.erase((x0,113,53),(x0+25,122,76))
        s.box((x0,113,51),(x0+25,122,53),"concrete_dark","window_sills")
        s.line((x0+12,117,52),(x0+12,117,66),1,"steel","window_frames")
    # Only the back portion of roof remains; the open face is a deliberate cutaway.
    s.box((18,99,47),(120,114,50),["concrete","concrete_dark"],"detachable_roof_remnant")
    for x in range(20,120):
        s.box((x,94+s.rng.randrange(0,6),47),(x+1,100,50),"concrete_dark","detachable_roof_remnant")
    for x in (44,91):
        s.box((x,88,8),(x+3,92,48),"steel","factory_columns")
        s.box((x-1,87,8),(x+4,93,10),"steel_edge","factory_columns")
        s.box((x-1,87,45),(x+4,93,48),"steel_edge","factory_columns")
        s.box((x-1,70,47),(x+4,115,50),"steel","roof_girders")
        s.box((x,70,45),(x+3,115,47),"steel_edge","roof_girders")
    # Cold exit stair and a second, distant cellar mouth beyond the room boundary.
    for k in range(12):
        s.box((125,48+k*5,4),(141,53+k*5,5+k*2),"concrete_dark","exit_stair")
        s.box((125,48+k*5,5+k*2),(141,53+k*5,6+k*2),"snow_shadow","exit_stair")
    s.masonry((141,94,5),(146,128,31),"outside_ruins")
    s.masonry((128,124,5),(155,130,31),"distant_cellar_threshold")
    s.erase((135,123,5),(145,131,20))
    s.box((133,123,20),(147,131,23),"wood_dark","distant_cellar_threshold")
    s.box((129,124,31),(154,130,32),"snow_light","outside_snow")
    s.line((141,58,10),(141,109,30),1,"steel","stair_handrail")
    for y,z in ((59,9),(84,20),(108,29)):
        s.line((141,y,5),(141,y,z),1,"steel","stair_handrail")
    # Ward: cramped clusters, not an implied literal body count from the testimony.
    for i,(x,y) in enumerate(((30,83),(51,84),(73,82),(104,86),(31,43),(99,43))):
        p=f"anonymous_patient_{i+1:02d}"
        s.box((x-5,y-10,8),(x+5,y+10,9),"wood_dark",p+"_bedding")
        s.stretcher(x,y,9,p)
    s.seated(25,64,8,"exhausted_figure_by_wall")
    s.seated(83,51,8,"figure_near_fire")
    s.seated(105,65,8,"waiting_figure")
    # Small floor fire is the compositional focus, not a theatrical bonfire.
    s.ellipsoid((62,45,8),(9,8,1),["soot","ash","charcoal"],"floor_fire_ash")
    for k in range(12):
        a=k*math.tau/12
        x,y=round(62+7*math.cos(a)),round(45+6*math.sin(a))
        s.box((x-1,y-1,8),(x+2,y+2,10),"brick_dark","floor_fire_ring")
    s.timber((58,42,9),(66,48,10),2,"floor_fire_fuel")
    s.timber((58,48,9),(66,42,10),2,"floor_fire_fuel")
    for x,y,h in ((61,44,5),(64,46,4),(60,47,3),(65,43,3)):
        s.box((x,y,9),(x+2,y+2,10+h),"ember","floor_fire_embers")
        s.box((x,y,10),(x+1,y+1,10+h),"flame","floor_fire_flame")
        s.put(x,y,10,"flame_core","floor_fire_core")
    # The single ration vessel is apart from the fire; no overstated supply abundance.
    s.cylinder((73,37,8),3,4,"steel","ration_kettle",True)
    s.cylinder((73,37,8),3,1,"steel","ration_kettle")
    s.line((69,37,12),(69,37,16),1,"steel_edge","ration_kettle")
    s.line((77,37,12),(77,37,16),1,"steel_edge","ration_kettle")
    s.line((69,37,16),(77,37,16),1,"steel_edge","ration_kettle")
    for x,y in ((78,33),(80,36),(74,30)):
        s.cylinder((x,y,8),1.6,1,"steel_edge","empty_mess_tins",True)
    s.box((25,23,8),(38,28,12),"wood","ration_crate")
    s.box((26,23,12),(37,28,13),"wood_light","ration_crate")
    s.stretcher(53,20,8,"empty_arrival_stretcher",False)
    # Collapsed girders and piles are held outside occupied paths for clear storytelling.
    s.timber((20,112,54),(53,126,33),3,"upper_fallen_beam")
    s.timber((81,117,52),(121,131,37),3,"upper_fallen_beam")
    for i in range(190):
        x,y=s.rng.randrange(7,157),s.rng.randrange(7,135)
        if 21<x<116 and 24<y<104: continue
        floor=1+max((z for z in range(9) if (x,y,z) in s.v),default=3)
        s.box((x,y,floor),(x+s.rng.randrange(1,4),y+s.rng.randrange(1,4),floor+s.rng.randrange(1,4)),
              s.rng.choice(["brick_dark","brick_ash","concrete_dark","snow_shadow"]),"scattered_rubble")
    for start in (20,52,104):
        for x in range(start,start+7):
            for y in range(115,121):
                top=max((z for z in range(48,75) if (x,y,z) in s.v),default=0)
                if top>=53: s.put(x,y,top+1,"snow","upper_snow")
    s.preview={"target":[8.1,6.8,2.25],"camera":[23,-24,21],"ortho":20.0,
               "world":[.075,.105,.17],"key":[-7,-4,17],"key_color":[.47,.67,1.0],
               "key_power":1500,"warm_lights":[{"location":[6.2,4.5,1.4],"power":180,"size":.18}]}
    return s


def airfield():
    s=Scene("no-departure-airfield",(190,166,58),.15,211943,
            "No departure — the evacuation system after Pitomnik",{
        "dossier_anchor":"A1-07", "record":"NARA T311 R271 001189–001190",
        "author":"Chief medical officer, Army Group Don; reports from Schulze and von Bassewitz",
        "dates":"21 January 1943; observations before evacuation on 18 January",
        "url":"https://stalingrad1942-1943.ru/index.php/node/2654",
        "attested":["Temperature reported as minus 35 degrees C on departure night",
                    "Wounded remained in unit bunkers and aid posts; centralized collection was impossible",
                    "Loss of Pitomnik disrupted evacuation; only one small airfield remained under artillery fire",
                    "Insufficient food, heating fuel, medical supplies and transport"],
        "interpretation":["A fictional composite of evacuation failure, not a reconstruction of Pitomnik or the remaining field.",
                          "All terrain, shelters, crater positions, paths, props and anonymous figures are original concept composition.",
                          "Patients stay in two separated shelters. There is no historically unsupported mass patient collection on a runway.",
                          "No aircraft is included: the dossier photo attribution is weaker than the medical report.",
                          "The cratered strip, wind marker and handcart are artistic staging, not specific reported objects."]})
    # Blue snow on a visibly sliced, uneven earth base.
    for y in range(5,161):
        for x in range(5,185):
            e=min(x-5,184-x,y-5,160-y)
            if e<s.rng.randrange(0,4): continue
            h=4+int(1.1*math.sin(x*.085)+.9*math.cos(y*.065))
            s.box((x,y,0),(x+1,y+1,h),["cut_earth","cut_earth_var"],"frozen_ground_section")
            top=s.rng.choice(["snow","snow","snow_light","snow_shadow"])
            s.put(x,y,h,top,"snow_surface")
    # Long, empty runway, with no arrival and a crater interrupting its far end.
    for y in range(7,157):
        for x in range(78,112):
            z=4+int(1.1*math.sin(x*.085)+.9*math.cos(y*.065))
            s.put(x,y,z,s.rng.choice(["mud","mud_light","ice","snow_shadow"]),"abandoned_strip")
            if x in (83,84,105,106): s.put(x,y,z,"charcoal","rutted_wheel_tracks")
    # Wide crater bowl is actual empty volume, not a flat dark decal.
    cx,cy=98,123
    for y in range(99,148):
        for x in range(73,124):
            r=math.hypot((x-cx)/1.08,(y-cy)/.85)
            if r<20:
                s.erase((x,y,0),(x+1,y+1,9))
                h=max(0,int((r/20)**2*4))
                s.box((x,y,0),(x+1,y+1,h+1),["mud","cut_earth","charcoal"],"shell_crater_bowl")
            elif r<24 and s.rng.random()<.8:
                s.box((x,y,4),(x+1,y+1,6+s.rng.randrange(0,3)),["mud_light","snow_shadow"],"crater_ejecta")
    # Two physically separate aid shelters, each its own small pocket of human scale.
    for idx,(x,y,w,d) in enumerate(((17,79,46,51),(129,40,42,43)),1):
        p=f"dispersed_aid_shelter_{idx}"
        s.box((x,y,4),(x+w,y+d,7),["wood","wood_dark"],p+"_floor")
        s.box((x,y,7),(x+3,y+d,21),"wood_dark",p+"_walls")
        s.box((x+w-3,y,7),(x+w,y+d,21),"wood_dark",p+"_walls")
        s.box((x+3,y+d-3,7),(x+w-3,y+d,21),"wood_dark",p+"_walls")
        for zz in range(8,21,3):
            s.box((x,y,zz),(x+3,y+d,zz+1),"wood",p+"_log_courses")
            s.box((x+w-3,y,zz),(x+w,y+d,zz+1),"wood",p+"_log_courses")
            s.box((x+3,y+d-3,zz),(x+w-3,y+d,zz+1),"wood",p+"_log_courses")
        # Cutaway roof on the rear third preserves access to the voxel patients.
        s.box((x-1,y+d-16,21),(x+w+1,y+d+1,24),"wood_dark",p+"_detachable_roof")
        s.box((x-2,y+d-16,24),(x+w+2,y+d+2,27),["mud","mud_light"],p+"_earth_roof")
        s.box((x-2,y+d-17,27),(x+w+2,y+d+2,28),["snow","snow_light"],p+"_roof_snow")
        for k in range(3):
            s.box((x+10,y-7+k*3,4),(x+w-9,y-4+k*3,5+k),"wood_dark",p+"_entrance_steps")
        for j in range(2):
            s.stretcher(x+12+j*18,y+19,7,p+f"_patient_{j+1}",True,2/3)
        s.seated(x+8,y+d-10,7,p+"_attendant",2/3)
        # A cold stove: no fuel abundance invented from a report of fuel scarcity.
        s.cylinder((x+w-9,y+d-8,7),2,4,"steel",p+"_cold_stove")
        s.line((x+w-9,y+d-8,11),(x+w-9,y+d-8,31),1,"steel",p+"_stove_pipe")
        s.box((x+w-18,y+d-8,7),(x+w-13,y+d-3,10),"cloth_dark",p+"_small_medical_bundle")
        # Banking of snow around independent shelters.
        s.ellipsoid((x-3,y+d*.5,10),(5,d*.58,7),["snow","snow_shadow"],p+"_snow_bank")
        s.ellipsoid((x+w+3,y+d*.5,10),(5,d*.58,7),["snow","snow_shadow"],p+"_snow_bank")
    # Churned paths terminate at the empty field. No central patient crowd.
    for a,b in (((52,77,5),(84,52,5)),((132,42,5),(107,35,5)),((48,72,5),(22,16,5))):
        n=int(math.dist(a,b)*2)
        for k in range(n):
            x=round(a[0]+(b[0]-a[0])*k/n); y=round(a[1]+(b[1]-a[1])*k/n)
            for dx in range(-2,3):
                top=max((z for z in range(8) if (x+dx,y,z) in s.v),default=3)
                s.put(x+dx,y,top,s.rng.choice(["mud","mud_light","snow_shadow"]),"interrupted_footpaths")
    # An empty carried stretcher abandoned short of the strip, and the carrying party.
    s.stretcher(67,49,4,"empty_stretcher_at_strip",False,2/3)
    s.seated(60,38,4,"waiting_orderly",2/3)
    s.box((60,44,4),(65,47,6),"cloth_dark","orderly_bag")
    # Broken handcart and a few cases define logistical collapse without adding a plane.
    s.box((24,37,6),(39,53,8),"wood_dark","broken_handcart")
    s.box((24,37,8),(26,53,12),"wood","broken_handcart")
    s.box((37,43,8),(39,53,12),"wood","broken_handcart")
    s.line((25,38,7),(22,21,6),1,"wood_light","broken_handcart")
    s.line((38,38,7),(35,21,6),1,"wood_light","broken_handcart")
    for xx in (23,40):
        for t in range(24):
            a=t*math.tau/24
            s.put(xx,round(46+4*math.cos(a)),round(6+4*math.sin(a)),"steel","handcart_wheels")
        s.line((xx,42,6),(xx,50,6),1,"wood","handcart_wheels")
        s.line((xx,46,2),(xx,46,10),1,"wood","handcart_wheels")
    for x,y in ((39,26),(49,29),(151,22)):
        s.box((x,y,5),(x+6,y+8,10),"wood","supply_cases")
        s.box((x,y,9),(x+6,y+8,10),"snow_shadow","supply_cases")
        s.box((x+1,y,5),(x+2,y+8,9),"steel","supply_case_bands")
    # Broken wind marker gives the empty landscape a readable vertical silhouette.
    s.line((124,131,4),(123,131,43),2,"wood_dark","wind_marker")
    s.line((123,131,43),(139,131,40),1,"steel","wind_marker")
    for k in range(15):
        s.box((124+k,131,39-k//5),(125+k,132,43-k//5),
              "cloth_dark" if k<8 else "cloth","torn_wind_marker_cloth")
    # The abandoned communication line connects shelters but not a rescue aircraft.
    for x,y in ((11,63),(11,20),(170,104)):
        s.line((x,y,5),(x,y,31),1,"wood_dark","telephone_posts")
        s.line((x-3,y,29),(x+3,y,29),1,"wood","telephone_posts")
    for k in range(44):
        s.put(11,20+k,round(29-5*math.sin(math.pi*k/43)),"charcoal","sagging_telephone_wire")
    for y in (19,45,71,92,154):
        for x in (74,116):
            s.box((x,y,4),(x+1,y+1,9),"wood_dark","strip_edge_markers")
            s.put(x,y,9,"cloth","strip_edge_markers")
    # Scarce snow-dusted debris, never a dense invented graveyard.
    for i in range(100):
        x,y=s.rng.randrange(8,181),s.rng.randrange(8,157)
        if 15<x<70 and 76<y<134 or 124<x<176 and 35<y<89: continue
        if 78<x<112 and 10<y<100: continue
        top=max((z for z in range(8) if (x,y,z) in s.v),default=2)
        s.put(x,y,top+1,s.rng.choice(["mud","wood_dark","snow_light"]),"scattered_winter_debris")
    s.preview={"target":[14.1,12.4,1.0],"camera":[36,-37,35],"ortho":36,
               "world":[.13,.18,.28],"key":[-8,3,24],"key_color":[.60,.76,1.0],
               "key_power":2600,"warm_lights":[]}
    return s


# Face vertices have outward winding in Z-up. Rotation into glTF is determinant +1.
FACES=[((1,0,0),[(1,0,0),(1,1,0),(1,1,1),(1,0,1)]),
       ((-1,0,0),[(0,0,0),(0,0,1),(0,1,1),(0,1,0)]),
       ((0,1,0),[(0,1,0),(0,1,1),(1,1,1),(1,1,0)]),
       ((0,-1,0),[(0,0,0),(1,0,0),(1,0,1),(0,0,1)]),
       ((0,0,1),[(0,0,1),(1,0,1),(1,1,1),(0,1,1)]),
       ((0,0,-1),[(0,0,0),(0,1,0),(1,1,0),(1,0,0)])]


def surface_groups(s):
    groups=collections.defaultdict(list)
    for (x,y,z),(col,part) in sorted(s.v.items()):
        for n,corners in FACES:
            if (x+n[0],y+n[1],z+n[2]) not in s.v:
                groups[(part,col)].append(((x,y,z),n,corners))
    return groups


def export_glb(s,path):
    data=bytearray(); views=[]; accessors=[]; meshes=[]; nodes=[]
    def append(raw,target):
        while len(data)%4:data.append(0)
        off=len(data); data.extend(raw)
        views.append({"buffer":0,"byteOffset":off,"byteLength":len(raw),"target":target})
        return len(views)-1
    mats=[]
    for name,rgb in PALETTE:
        # Palette bytes are sRGB. glTF material baseColorFactor is linear.
        linear=[(v/255/12.92 if v/255<=.04045 else ((v/255+.055)/1.055)**2.4) for v in rgb]
        mat={"name":name,"pbrMetallicRoughness":{"baseColorFactor":linear+[1],"metallicFactor":.22 if "steel" in name else 0,"roughnessFactor":.94}}
        if name in ("ember","flame","flame_core"):
            mat["emissiveFactor"]=linear
            mat["extensions"]={"KHR_materials_emissive_strength":{"emissiveStrength":3 if name=="flame_core" else 1.7}}
        mats.append(mat)
    groups=surface_groups(s); by_part=collections.defaultdict(list)
    faces=0
    for (part,col),quads in sorted(groups.items()):
        pos=[]; norm=[]; indices=[]
        for origin,n,corners in quads:
            base=len(pos)//3
            for c in corners:
                x,y,z=((origin[i]+c[i])*s.cell_m for i in range(3))
                pos.extend((x,z,-y))
                norm.extend((n[0],n[2],-n[1]))
            indices.extend((base,base+1,base+2,base,base+2,base+3))
        vpos=append(struct.pack('<%sf'%len(pos),*pos),34962)
        vnorm=append(struct.pack('<%sf'%len(norm),*norm),34962)
        vidx=append(struct.pack('<%sI'%len(indices),*indices),34963)
        ai=len(accessors)
        accessors.extend([
            {"bufferView":vpos,"componentType":5126,"count":len(pos)//3,"type":"VEC3","min":[min(pos[i::3]) for i in range(3)],"max":[max(pos[i::3]) for i in range(3)]},
            {"bufferView":vnorm,"componentType":5126,"count":len(norm)//3,"type":"VEC3"},
            {"bufferView":vidx,"componentType":5125,"count":len(indices),"type":"SCALAR"},
        ])
        by_part[part].append({"attributes":{"POSITION":ai,"NORMAL":ai+1},"indices":ai+2,"material":col-1,"mode":4})
        faces+=len(quads)
    for part,prims in sorted(by_part.items()):
        meshes.append({"name":part,"primitives":prims})
        nodes.append({"name":part,"mesh":len(meshes)-1,"extras":{"semantic_part":part,"concept_asset":True}})
    doc={"asset":{"version":"2.0","generator":"Hephaestus deterministic voxel concepts / Panzer, 2026-11-07"},
         "scene":0,"scenes":[{"name":s.title,"nodes":list(range(len(nodes)))}],"nodes":nodes,"meshes":meshes,
         "materials":mats,"accessors":accessors,"bufferViews":views,"buffers":[{"byteLength":len(data)}],
         "extensionsUsed":["KHR_materials_emissive_strength"],
         "extras":{"voxel_cell_m":s.cell_m,"editable_source":path.with_suffix('.voxels.json').name,
                   "source":s.source,"status":"concept only; no runtime integration or performance claim"}}
    j=json.dumps(doc,separators=(',',':'),ensure_ascii=False).encode()
    j+=b' '*((-len(j))%4); data+=b'\0'*((-len(data))%4)
    blob=struct.pack('<4sII',b'glTF',2,28+len(j)+len(data))+struct.pack('<I4s',len(j),b'JSON')+j+struct.pack('<I4s',len(data),b'BIN\0')+data
    path.write_bytes(blob)
    return {"surface_quads":faces,"triangles":faces*2,"vertices":faces*4,"semantic_nodes":len(nodes),"primitives":len(groups)}


def export_vox(s,path):
    def chunk(name,b): return name+struct.pack('<II',len(b),0)+b
    voxels=bytes(v for p,(col,_) in sorted(s.v.items()) for v in (*p,col))
    rgba=b''.join(bytes((*rgb,255)) for _,rgb in PALETTE)+bytes(4*(256-len(PALETTE)))
    content=chunk(b'SIZE',struct.pack('<III',*s.dims))+chunk(b'XYZI',struct.pack('<I',len(s.v))+voxels)+chunk(b'RGBA',rgba)
    path.write_bytes(b'VOX '+struct.pack('<I',150)+b'MAIN'+struct.pack('<II',0,len(content))+content)


def validate(s,glb,vox,stats):
    raw=glb.read_bytes(); magic,ver,length=struct.unpack_from('<4sII',raw)
    assert magic==b'glTF' and ver==2 and length==len(raw)
    n,kind=struct.unpack_from('<I4s',raw,12); assert kind==b'JSON'
    doc=json.loads(raw[20:20+n]); bn,bkind=struct.unpack_from('<I4s',raw,20+n)
    assert bkind==b'BIN\0'
    binary=raw[28+n:]; assert len(binary)==bn
    for v in doc['bufferViews']: assert v['byteOffset']+v['byteLength']<=len(binary)
    for mesh in doc['meshes']:
        for p in mesh['primitives']:
            pa=doc['accessors'][p['attributes']['POSITION']]
            ia=doc['accessors'][p['indices']]; iv=doc['bufferViews'][ia['bufferView']]
            ids=struct.unpack_from('<%sI'%ia['count'],binary,iv['byteOffset'])
            assert max(ids)<pa['count'] and len(ids)%3==0
            pv=doc['bufferViews'][pa['bufferView']]
            positions=struct.unpack_from('<%sf'%(pa['count']*3),binary,pv['byteOffset'])
            assert all(math.isfinite(v) for v in positions)
    vraw=vox.read_bytes(); assert vraw[:4]==b'VOX ' and struct.unpack_from('<I',vraw,4)[0]==150
    cursor=20; parsed={}
    while cursor<len(vraw):
        kind,content_size,child_size=struct.unpack_from('<4sII',vraw,cursor)
        content=vraw[cursor+12:cursor+12+content_size]
        assert len(content)==content_size and child_size==0
        if kind==b'SIZE': assert struct.unpack('<III',content)==s.dims
        if kind==b'XYZI':
            count=struct.unpack_from('<I',content)[0];assert len(content)==4+count*4
            parsed={(x,y,z):c for x,y,z,c in struct.iter_unpack('<4B',content[4:])}
        if kind==b'RGBA':
            assert all(content[i*4:i*4+4]==bytes((*rgb,255)) for i,(_,rgb) in enumerate(PALETTE))
        cursor+=12+content_size
    assert parsed=={p:col for p,(col,_) in s.v.items()}
    assert all(0<=p[i]<s.dims[i]<=256 for p in s.v for i in range(3))
    assert all(1<=v[0]<=len(PALETTE) for v in s.v.values())
    counts=collections.Counter(p for _,p in s.v.values())
    chunks=collections.Counter((x//16,y//16,z//16) for x,y,z in s.v)
    bounds=[[min(k[i] for k in s.v)*s.cell_m,(max(k[i] for k in s.v)+1)*s.cell_m] for i in range(3)]
    return {"schema":"panzer.concept-validation.v1","asset":s.name,"checks":{
        "glb_header_and_length":"pass","glb_buffer_bounds":"pass","finite_positions":"pass",
        "triangle_index_bounds":"pass","vox_header":"pass","vox_round_trip_palette_and_cells":"pass","voxel_palette_and_grid_bounds":"pass",
        "hidden_faces_culled":"pass","files_under_20_mib":"pass" if max(glb.stat().st_size,vox.stat().st_size)<20*1024*1024 else "fail"},
        "occupied_voxels":len(s.v),"voxel_cell_m":s.cell_m,"grid_dimensions":s.dims,
        "occupied_bounds_m_z_up":bounds,"occupied_16_cube_chunks":len(chunks),"semantic_parts":dict(sorted(counts.items())),
        **stats,"not_validated":["Gameplay integration","runtime frame budget","collision setup","destruction connectivity",
                                 "historical reconstruction accuracy","canonical game-dev package unavailable"]}


def write_scene(s):
    OUT.mkdir(parents=True,exist_ok=True)
    parts=sorted(set(p for _,p in s.v.values())); part_index={p:i for i,p in enumerate(parts)}
    data={"schema":"panzer.sparse-voxel-concept.v1","name":s.name,"title":s.title,"seed":s.seed,
          "dimensions":s.dims,"cell_size_m":s.cell_m,"axis":"right-handed Z-up; X east, Y depth, Z height",
          "origin":"grid corner at [0,0,0]; base begins at Z=0; cells occupy [x,x+1] lattice intervals",
          "glb_coordinates":"metres; [X,Y,Z]_glb = cell_size_m * [x,z,-y]_voxel; node transforms are identity",
          "dimensions_m":[v*s.cell_m for v in s.dims],
          "material_policy":"Qualitative appearance families only. No calibrated density, yield, thermal or failure values are supplied.",
          "palette":[{"id":i+1,"name":n,"srgb":rgb,"material_family":material_family(n)} for i,(n,rgb) in enumerate(PALETTE)],
          "parts":parts,"voxel_layout":"[x,y,z,palette_id,part_index]","source":s.source,
          "preview":s.preview,"voxels":[[*p,col,part_index[part]] for p,(col,part) in sorted(s.v.items())]}
    base=OUT/s.name
    base.with_suffix('.voxels.json').write_text(json.dumps(data,separators=(',',':'),ensure_ascii=False)+'\n')
    export_vox(s,base.with_suffix('.vox'))
    stats=export_glb(s,base.with_suffix('.glb'))
    receipt=validate(s,base.with_suffix('.glb'),base.with_suffix('.vox'),stats)
    receipt['artifacts']={p.name:{"bytes":p.stat().st_size,"sha256":hashlib.sha256(p.read_bytes()).hexdigest()}
                          for p in (base.with_suffix('.voxels.json'),base.with_suffix('.vox'),base.with_suffix('.glb'))}
    base.with_suffix('.validation.json').write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps({"name":s.name,"voxels":len(s.v),**stats,"files":receipt['artifacts']},indent=2))


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--scene',choices=['cellar','airfield','all'],default='all')
    args=parser.parse_args()
    if args.scene in ('all','cellar'):write_scene(cellar())
    if args.scene in ('all','airfield'):write_scene(airfield())
