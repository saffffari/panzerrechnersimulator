#!/usr/bin/env python3
"""Prepare private source-derived Tiger fan service meshes; never edit the source GLB.

Run with ordinary Python. Blender's CPU exact solid booleans make the authored radial
clearance and close the cuts. Existing surface positions, normals, UVs and material are
carried through; newly exposed radial cut faces are explicitly documented derivative work.
No renders, GPU calls, remeshing, decimation, or guessed gearbox internals are used.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
SOURCE = ROOT / "assets/tanks/tiger/baked/tiger_hull.glb"
SOURCE_SHA = "51938f43eb90ba321ff452c2be953d06673f7aab670465835144cf8ce88c9a84"
OUTPUT = ROOT / "assets/tanks/tiger/workshop_cooling"
CATALOG = ROOT / "catalog/tiger_workshop_cooling.v1.json"
STAGING = ROOT / "artifacts/workshop-cooling-source-20261030"
SIDES = 256
ROTOR_TRIM_M = 0.001
STATOR_TRIM_M = 0.0005

def source_data():
    from fit_crew_steel_helmet import read_glb, accessor
    from validate_tiger_hatches import matrix
    doc, binary, raw = read_glb(str(SOURCE))
    assert hashlib.sha256(raw).hexdigest() == SOURCE_SHA, "source changed; re-audit its geometry"
    assert not doc.get("images") and not doc.get("textures"), "new textured source needs texture-copy audit"
    roots = doc["scenes"][doc.get("scene", 0)]["nodes"]
    result = {}
    for i in roots:
        node = doc["nodes"][i]
        if node.get("name") not in {'Hull_1102','Hull_1103','Hull_1115','Hull_1116','Hull_1125','Hull_1126'}:
            continue
        primitives = doc["meshes"][node["mesh"]]["primitives"]
        assert len(primitives) == 1
        p = primitives[0]
        m = matrix(node)
        v = accessor(doc, binary, p["attributes"]["POSITION"]).astype(float)
        v = v @ m[:3, :3].T + m[:3, 3]
        n = accessor(doc, binary, p["attributes"]["NORMAL"]).astype(float)
        n = n @ np.linalg.inv(m[:3, :3])
        n /= np.linalg.norm(n, axis=1)[:, None]
        uv = accessor(doc, binary, p["attributes"]["TEXCOORD_0"]).astype(float)
        result[node["name"]] = dict(vertices=np.c_[v, n, uv],
            triangles=accessor(doc, binary, p["indices"]).reshape(-1, 3).astype(int),
            material=copy.deepcopy(doc["materials"][p["material"]]), node=i)
    return result

def metrics(data):
    v = data["vertices"][:, :3]
    t = data["triangles"]
    a = v[t]
    cross = np.cross(a[:, 1]-a[:, 0], a[:, 2]-a[:, 0])
    area = np.linalg.norm(cross, axis=1)*0.5
    _, welded = np.unique(np.round(v, 6), axis=0, return_inverse=True)
    wt = welded[t]
    edges = np.concatenate([wt[:, [0, 1]], wt[:, [1, 2]], wt[:, [2, 0]]])
    direction = np.where(edges[:, 0] < edges[:, 1], 1, -1)
    unique, inverse, count = np.unique(np.sort(edges, axis=1), axis=0,
                                     return_counts=True, return_inverse=True)
    balance = np.bincount(inverse, weights=direction)
    origin = v.mean(0)
    signed_volume = np.einsum("ij,ij->i", a[:, 0]-origin,
                            np.cross(a[:, 1]-origin, a[:, 2]-origin)).sum()/6
    return dict(vertices=len(v), triangles=len(t), min=v.min(0).tolist(), max=v.max(0).tolist(),
        area_m2=float(area.sum()), volume_m3=float(signed_volume),
        boundary_edges=int((count == 1).sum()), nonmanifold_edges=int((count > 2).sum()),
        inconsistent_winding_edges=int(((count == 2) & (balance != 0)).sum()),
        collapsed_edges=int((unique[:, 0] == unique[:, 1]).sum()),
        degenerate_triangles=int((area <= 1e-13).sum()),
        minimum_triangle_area_m2=float(area.min()),
        finite_attributes=bool(np.isfinite(data["vertices"]).all()))

def fit_axes(data):
    v = data["vertices"][:, :3]
    a = v[data["triangles"]]
    cross = np.cross(a[:, 1]-a[:, 0], a[:, 2]-a[:, 0])
    normals = cross / np.maximum(np.linalg.norm(cross, axis=1)[:, None], 1e-20)
    seed = np.array([0.0, 0.87518, -0.48379])
    seed /= np.linalg.norm(seed)
    planar = normals @ seed > 0.99999
    levels = a.mean(1) @ seed
    rounded, labels = np.unique(np.round(levels[planar], 4), return_inverse=True)
    areas = np.linalg.norm(cross[planar], axis=1)
    plane = rounded[np.argmax(np.bincount(labels, weights=areas))]
    pts = np.unique(a[planar & (abs(levels-plane) < 0.00006)].reshape(-1, 3), axis=0)
    _, _, basis = np.linalg.svd(pts-pts.mean(0), full_matrices=False)
    axis = basis[-1] * np.sign(basis[-1] @ seed)
    # X is an exact source symmetry direction, made orthogonal to the measured plane.
    right = np.array([1., 0., 0.]); right -= axis*(right@axis); right /= np.linalg.norm(right)
    up = np.cross(axis, right)
    q = np.c_[v @ right, v @ up]
    half = q[:, 0].mean()
    # The two hub lines are found from the repeated 229.6 mm circular aperture rims,
    # starting at the measured equal-offset halves of the source assembly bounds.
    rough = [(q[:, 0].min()+0.257866, np.median(q[:, 1])),
             (q[:, 0].max()-0.257866, np.median(q[:, 1]))]
    centers = []
    for side in (-1, 1):
        near = q[q[:, 0] < half] if side < 0 else q[q[:, 0] > half]
        center = np.array([rough[0 if side < 0 else 1][0],
                           (q[:, 1].min()+q[:, 1].max())*0.5])
        # Hub front face and aperture share the same line; infer its in-plane Y from
        # the middle of each circular outer rim, independent of a world-space label.
        center[1] = (near[:, 1].min()+near[:, 1].max())*0.5
        for _ in range(4):
            radius = np.linalg.norm(q-center, axis=1)
            selected = q[abs(radius-0.229627) < (0.004 if _ == 0 else 0.00008)]
            assert len(selected) > 100, "could not isolate source aperture rim"
            selected = np.unique(selected, axis=0)
            solution = np.linalg.lstsq(np.c_[2*selected, np.ones(len(selected))],
                                      (selected*selected).sum(1), rcond=None)[0]
            center = solution[:2]
        radius = np.linalg.norm(selected-center, axis=1)
        pivot = right*center[0]+up*center[1]+axis*float(pts.mean(0)@axis)
        centers.append(dict(center_hull_m=pivot.tolist(), axis_hull=axis.tolist(),
            radius_m=float(np.median(radius)), rim_fit_max_m=float(abs(radius-np.median(radius)).max()),
            measured_rim_points=len(selected), plane_fit_max_m=float(abs((pts-pts.mean(0))@axis).max())))
    assert all(c["rim_fit_max_m"] < 0.0001 for c in centers)
    return centers

def make_blender_source(name, data):
    import bpy
    attr = data["vertices"]
    _, first, inverse = np.unique(np.round(attr[:, :3], 6), axis=0,
                                  return_index=True, return_inverse=True)
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(attr[first, :3].tolist(), [], inverse[data["triangles"]].tolist())
    mesh.update()
    uv = mesh.uv_layers.new(name="TEXCOORD_0")
    flat = data["triangles"].reshape(-1)
    for i, index in enumerate(flat): uv.data[i].uv = attr[index, 6:8]
    mesh.normals_split_custom_set(attr[flat, 3:6].tolist())
    obj = bpy.data.objects.new(name, mesh); bpy.context.collection.objects.link(obj)
    return obj

def cylinder(name, center, axis, radius, data):
    import bpy
    axis = np.array(axis); center = np.array(center)
    e = np.array([1., 0., 0.]); e -= axis*(axis@e); e /= np.linalg.norm(e)
    f = np.cross(axis, e)
    h = (data["vertices"][:, :3]-center) @ axis
    ends = [float(h.min()-0.10), float(h.max()+0.10)]
    # Avoid coplanar coincidences with the original radially tessellated kit mesh.
    angles = np.arange(SIDES)*2*math.pi/SIDES + 0.371
    ring = radius*(np.cos(angles)[:, None]*e + np.sin(angles)[:, None]*f)
    vertices = np.concatenate([center+ring+axis*end for end in ends])
    faces = [(i, (i+1)%SIDES, (i+1)%SIDES+SIDES, i+SIDES) for i in range(SIDES)]
    faces += [tuple(range(SIDES-1, -1, -1)), tuple(range(SIDES, SIDES*2))]
    mesh = bpy.data.meshes.new(name); mesh.from_pydata(vertices.tolist(), [], faces); mesh.update()
    uv = mesh.uv_layers.new(name="TEXCOORD_0")
    for loop in mesh.loops:
        p = vertices[loop.vertex_index]-center
        uv.data[loop.index].uv = (float(p@e), float(p@axis))
    obj = bpy.data.objects.new(name, mesh); bpy.context.collection.objects.link(obj)
    return obj

def boolean(obj, cutter, operation):
    import bpy
    bpy.context.view_layer.objects.active = obj; obj.select_set(True)
    mod = obj.modifiers.new("Documented cylindrical clearance", 'BOOLEAN')
    mod.operation = operation; mod.solver = 'EXACT'; mod.object = cutter
    bpy.ops.object.modifier_apply(modifier=mod.name)
    obj.select_set(False)

def blender_data(obj, material):
    mesh = obj.data; mesh.calc_loop_triangles()
    normals = mesh.corner_normals
    uv = mesh.uv_layers.active
    rows = []
    for tri in mesh.loop_triangles:
        for loop in tri.loops:
            p = mesh.vertices[mesh.loops[loop].vertex_index].co
            rows.append([*p, *normals[loop].vector, *uv.data[loop].uv])
    vertices, inverse = np.unique(np.array(rows, dtype=np.float32), axis=0, return_inverse=True)
    return dict(vertices=vertices.astype(float), triangles=inverse.reshape(-1, 3), material=material)

def glb_bytes(name, data):
    payload = bytearray(); views = []; accessors = []
    def put(array, kind, component):
        payload.extend(b'\0'*(-len(payload)%4)); raw = array.tobytes()
        views.append(dict(buffer=0, byteOffset=len(payload), byteLength=len(raw))); payload.extend(raw)
        a = dict(bufferView=len(views)-1, componentType=component, count=len(array), type=kind)
        if kind == 'VEC3': a.update(min=array.min(0).tolist(), max=array.max(0).tolist())
        accessors.append(a); return len(accessors)-1
    a = data["vertices"]
    attrs = {key:put(a[:, lo:hi].astype('<f4').copy(), kind, 5126)
             for key,lo,hi,kind in [('POSITION',0,3,'VEC3'),('NORMAL',3,6,'VEC3'),('TEXCOORD_0',6,8,'VEC2')]}
    index = put(data["triangles"].astype('<u4').reshape(-1), 'SCALAR', 5125)
    doc = dict(asset=dict(version='2.0', generator='prepare_tiger_cooling.py'), scene=0,
        scenes=[dict(nodes=[0])], nodes=[dict(name=name, mesh=0)],
        meshes=[dict(primitives=[dict(attributes=attrs,indices=index,material=0)])],
        materials=[data["material"]], bufferViews=views, accessors=accessors,
        buffers=[dict(byteLength=len(payload))])
    extensions = sorted(data["material"].get('extensions', {}))
    if extensions: doc['extensionsUsed'] = extensions
    doc_bytes = json.dumps(doc,separators=(',',':')).encode(); doc_bytes += b' '*(-len(doc_bytes)%4)
    payload.extend(b'\0'*(-len(payload)%4))
    return (struct.pack('<4sII',b'glTF',2,28+len(doc_bytes)+len(payload))
        +struct.pack('<II',len(doc_bytes),0x4e4f534a)+doc_bytes
        +struct.pack('<II',len(payload),0x004e4942)+payload)

def read_output(path):
    from fit_crew_steel_helmet import read_glb, accessor
    doc, binary, _ = read_glb(str(path))
    node = doc['nodes'][0]
    assert set(node) == {'name','mesh'}, 'derivatives must remain in hull coordinates'
    p = doc['meshes'][node['mesh']]['primitives'][0]
    return dict(vertices=np.c_[*[accessor(doc,binary,p['attributes'][key])
        for key in ('POSITION','NORMAL','TEXCOORD_0')]].astype(float),
        triangles=accessor(doc,binary,p['indices']).reshape(-1,3).astype(int),
        material=doc['materials'][p['material']])

def hull_access_triangles():
    """Actual source triangles in the bounded rear-deck volume, with root identity."""
    from fit_crew_steel_helmet import read_glb, accessor
    from validate_tiger_hatches import matrix
    doc,binary,_=read_glb(str(SOURCE)); result=[]; names=[]; triangle_ids=[]
    lo=np.array([-1.8,1.4,-3.2]); hi=np.array([1.8,3.0,-2.2])
    def walk(index,parent,root_name):
        node=doc['nodes'][index]; m=parent@matrix(node)
        if 'mesh' in node:
            offset=0
            for p in doc['meshes'][node['mesh']]['primitives']:
                v=accessor(doc,binary,p['attributes']['POSITION'])@m[:3,:3].T+m[:3,3]
                t=accessor(doc,binary,p['indices']).reshape(-1,3)
                a=v[t]; selected=np.all(a.min(1)<=hi,axis=1)&np.all(a.max(1)>=lo,axis=1)
                result.extend(a[selected]); names.extend([root_name]*int(selected.sum()))
                triangle_ids.extend((np.where(selected)[0]+offset).tolist()); offset+=len(t)
        for child in node.get('children',[]):walk(child,m,root_name)
    for index in doc['scenes'][doc.get('scene',0)]['nodes']:
        walk(index,np.eye(4),doc['nodes'][index].get('name','unnamed'))
    return np.array(result),np.array(names),np.array(triangle_ids)

def vertical_intersections(triangles,x,z,bottom,top):
    a=triangles[:,0]; b=triangles[:,1]-a; c=triangles[:,2]-a
    det=b[:,0]*c[:,2]-b[:,2]*c[:,0]; good=abs(det)>1e-12
    idx=np.where(good)[0]; a=a[good]; b=b[good]; c=c[good]; det=det[good]
    dx=x-a[:,0]; dz=z-a[:,2]
    u=(dx*c[:,2]-dz*c[:,0])/det; v=(b[:,0]*dz-b[:,2]*dx)/det
    y=a[:,1]+u*b[:,1]+v*c[:,1]
    valid=(u>=-1e-7)&(v>=-1e-7)&(u+v<=1+1e-7)&(y>=bottom)&(y<=top)
    return idx[valid],y[valid]

def augment_report(report):
    source=source_data(); hull,names,triangle_ids=hull_access_triangles()
    def contact(root,excluded):
        data=source[root]; a=data['vertices'][data['triangles'],:3]
        cross=np.cross(a[:,1]-a[:,0],a[:,2]-a[:,0]); size=np.linalg.norm(cross,axis=1)
        norm=cross/np.maximum(size[:,None],1e-20); middle=a.mean(1)
        center=(a.reshape(-1,3).min(0)+a.reshape(-1,3).max(0))*0.5
        candidates=np.where((norm[:,1]>.7)&(size>0.00003))[0]
        candidates=sorted(candidates,key=lambda i:np.linalg.norm((middle[i]-center)[[0,2]]))
        allowed=~np.isin(names,excluded); scene=hull[allowed]; scene_names=names[allowed]
        for i in candidates:
            point=middle[i]; hit,y=vertical_intersections(scene,point[0],point[2],point[1]-.00001,point[1]+.65)
            if len(hit) and scene_names[hit[np.argmax(y)]]==root and abs(y.max()-point[1])<.00002:
                return dict(retainer_contact_hull_m=point.tolist(),source_triangle=int(i),
                    source_normal_hull=norm[i].tolist(),test_eye_hull_m=(point+[0,.65,0]).tolist(),
                    removed_for_access=excluded,
                    semantics='Exposed source service-contact candidate; source label does not certify a particular historical fastener.')
        raise AssertionError(('no exposed source service contact',root))
    report['cassettes']=[];report['grilles']=[]
    by_name={Path(f['file']).stem:f for f in report['files']}
    for assembly in report['assemblies']:
        side=assembly['side']; root=assembly['source_root']; upper,lower=assembly['grille_roots']
        c=contact(root,[upper,lower]); rotors=[]; total_caps=0.; all_positions=[]
        for rotor in assembly['rotors']:
            rotors.append(dict(glb=by_name[rotor['name']]['file'],**rotor))
        for name in [assembly['stator'],*[r['name'] for r in assembly['rotors']]]:
            data=read_output(ROOT/by_name[name]['file']); a=data['vertices'][data['triangles'],:3]
            all_positions.append(data['vertices'][:,:3])
            cross=np.cross(a[:,1]-a[:,0],a[:,2]-a[:,0]); ar=np.linalg.norm(cross,axis=1)*.5
            norm=cross/np.maximum((2*ar)[:,None],1e-20); caps=np.zeros(len(a),dtype=bool)
            for fit in assembly['rotors']:
                axis=np.array(fit['axis_hull']); p=a-np.array(fit['center_hull_m'])
                radial=np.linalg.norm(p-(p@axis)[:,:,None]*axis,axis=2)
                radius=fit['radius_m']+(STATOR_TRIM_M if name==assembly['stator'] else -ROTOR_TRIM_M)
                caps|=(np.all(radial>=radius*math.cos(math.pi/SIDES)-1e-6,axis=1)
                    &np.all(radial<=radius+1e-6,axis=1)&(abs(norm@axis)<.001))
            cap_area=float(ar[caps].sum()); by_name[name]['new_cut_cap_area_m2']=cap_area;total_caps+=cap_area
            assert data['material']==source[root]['material'],'material changed'
        points=np.concatenate(all_positions); original=assembly['source_metrics']
        assembly['new_cut_cap_area_m2']=total_caps
        assembly['original_surface_area_removed_m2']=original['area_m2']-(assembly['total_derivative_area_m2']-total_caps)
        assembly['assembled_bounds_max_difference_m']=float(max(
            abs(points.min(0)-original['min']).max(),abs(points.max(0)-original['max']).max()))
        assert assembly['assembled_bounds_max_difference_m']<1e-6
        assert assembly['original_surface_area_removed_m2']>=-1e-5
        # Vertex trajectories are useful rejection witnesses, not a complete swept-solid proof.
        # A blocker is retained in the receipt; an empty result cannot authorize a lift.
        allowed=~np.isin(names,[root,upper,lower]); scene=hull[allowed]; scene_names=names[allowed]
        scene_ids=triangle_ids[allowed]; v=np.unique(np.round(source[root]['vertices'][:,:3],7),axis=0)
        blockers=[]; count=0
        for point in v:
            hit,y=vertical_intersections(scene,point[0],point[2],point[1]+.002,point[1]+.65)
            if len(hit):
                count+=1
                if len(blockers)<8:
                    j=int(hit[np.argmin(y)]);blockers.append(dict(source_root=str(scene_names[j]),
                        source_triangle=int(scene_ids[j]),moving_vertex_hull_m=point.tolist(),
                        first_lift_m=float(y.min()-point[1])))
        assembly['extraction']['vertical_vertex_sweep']=dict(lift_m=.65,initial_contact_skip_m=.002,
            source_vertices_tested=len(v),blocked_vertices=count,blockers=blockers,
            complete_swept_solid_certificate=False,scope='Current rigid hull source only; no terrain, turret pose, crew or world fixtures.')
        report['cassettes'].append(dict(side=side,source_root=root,
            stator_glb=by_name[assembly['stator']]['file'],rotors=rotors,
            original_glb=by_name[side+'_cassette_original']['file'],**c))
        for position,r,excluded in [('upper',upper,[]),('lower',lower,[upper])]:
            report['grilles'].append(dict(side=side,position=position,source_root=r,
                glb=by_name[side+'_grille_'+position]['file'],**contact(r,excluded)))
    return report

def worker():
    import bpy
    bpy.ops.object.select_all(action='SELECT'); bpy.ops.object.delete(use_global=False)
    source = source_data(); STAGING.mkdir(parents=True, exist_ok=True)
    files = []; assemblies = []
    def emit(name, data, provenance):
        stats = metrics(data)
        assert stats['finite_attributes'] and stats['volume_m3'] > 0, (name, stats)
        assert all(stats[k] == 0 for k in ('boundary_edges','nonmanifold_edges',
            'inconsistent_winding_edges','collapsed_edges','degenerate_triangles')), (name,stats)
        raw = glb_bytes(name, data); (STAGING/(name+'.glb')).write_bytes(raw)
        files.append(dict(file='assets/tanks/tiger/workshop_cooling/'+name+'.glb',
            sha256=hashlib.sha256(raw).hexdigest(),metrics=stats,**provenance))
        return stats
    for side, root, upper, lower in [('left','Hull_1102','Hull_1115','Hull_1125'),
                                     ('right','Hull_1103','Hull_1116','Hull_1126')]:
        original = source[root]; axes = fit_axes(original)
        stator = make_blender_source(side+'_stator', original)
        rotor_stats = []; rotors = []
        for index, fit in enumerate(axes):
            center = fit['center_hull_m']; axis = fit['axis_hull']; radius = fit['radius_m']
            position = 'inboard' if abs(center[0]) < 1.15 else 'outboard'
            name = f'{side}_rotor_{position}'
            rotor = make_blender_source(name, original)
            inner = cylinder(name+'_cut',center,axis,radius-ROTOR_TRIM_M,original)
            outer = cylinder(name+'_socket',center,axis,radius+STATOR_TRIM_M,original)
            boolean(rotor, inner, 'INTERSECT'); boolean(stator, outer, 'DIFFERENCE')
            data = blender_data(rotor, original['material'])
            stat = emit(name,data,dict(source_root=root,derivative='capped cylindrical intersection'))
            rotor_stats.append(stat)
            p = data['vertices'][:,:3]-np.array(center); n = np.array(axis)
            radial = np.linalg.norm(p-(p@n)[:,None]*n,axis=1)
            bound = (radius+STATOR_TRIM_M)*math.cos(math.pi/SIDES)-float(radial.max())
            assert bound > 0.001, (name,bound)
            rotors.append(dict(name=name,**fit,rotor_max_radius_m=float(radial.max()),
                stator_cut_vertex_radius_m=radius+STATOR_TRIM_M,
                radial_gap_lower_bound_m=bound,
                rotation='hull * translation(center) * rotation(axis,angle) * translation(-center)'))
        static_stats = emit(side+'_stator',blender_data(stator,original['material']),
                            dict(source_root=root,derivative='two capped cylindrical differences'))
        emit(side+'_cassette_original',original,dict(source_root=root,derivative='none; source node transform baked'))
        for name, source_root in [('grille_upper',upper),('grille_lower',lower)]:
            emit(side+'_'+name,source[source_root],dict(source_root=source_root,derivative='none; source node transform baked'))
        before = metrics(original)
        removed = before['volume_m3']-static_stats['volume_m3']-sum(r['volume_m3'] for r in rotor_stats)
        assert 0 < removed < before['volume_m3']*.10, ('excessive removed volume',removed,before)
        assemblies.append(dict(side=side,source_root=root,source_node=original['node'],
            source_triangles=before['triangles'],source_metrics=before,
            stator=side+'_stator',rotors=rotors,grille_roots=[upper,lower],
            removed_volume_m3=removed,removed_volume_fraction=removed/before['volume_m3'],
            total_derivative_area_m2=static_stats['area_m2']+sum(r['area_m2'] for r in rotor_stats),
            extraction=dict(proposed_direction_hull=[0,1,0],grilles_must_be_removed=[upper,lower],
                clearance='Not certified: static source sweep audit is recorded separately; live source collision must veto extraction.')))
    report = dict(schema='panzer.tiger-workshop-cooling.v1',source='assets/tanks/tiger/baked/tiger_hull.glb',
        source_sha256=SOURCE_SHA,coordinate_frame='current hull metres; all GLB nodes identity',
        authored_change=dict(cylinder_sides=SIDES,rotor_radial_trim_m=ROTOR_TRIM_M,
            stator_radial_trim_m=STATOR_TRIM_M,nominal_radial_gap_m=ROTOR_TRIM_M+STATOR_TRIM_M,
            caps='CPU exact solid boolean; newly exposed cylindrical facets are authored cut faces',
            normalization='Source coincident position weld at 1 micrometre; surface UVs and corner normals retained by Blender Boolean interpolation'),
        material='Exact source material object preserved. This source has no texture images; source UVs retained.',
        original_source_modified=False,blender_version=bpy.app.version_string,assemblies=assemblies,files=files,
        limitations=['No gearbox teeth, gear ratios, bearing internals or historically certified fan speed are reconstructed.',
            'Separate left/right geometry and identity; no negative-scale substitution.',
            'Static CPU source derivative validation only; no GPU import or installed gameplay proof.'])
    (STAGING/'catalog.json').write_text(json.dumps(report,indent=2)+'\n')
    print('COOLING_WORKER_COMPLETE',len(files))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--blender',default=os.environ.get('BLENDER_EXE',r'C:\Program Files\Blender Foundation\Blender 5.1\blender.exe'))
    parser.add_argument('--worker',action='store_true')
    parser.add_argument('--update-audit',action='store_true',help='verify existing generated bytes, update only their measured catalog evidence')
    parser.add_argument('--verify',action='store_true',help='verify existing generated hashes and solids without writing')
    args = parser.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else None)
    if args.worker: worker(); return
    if args.update_audit or args.verify:
        report=json.loads(CATALOG.read_text())
        assert report['source_sha256']==SOURCE_SHA
        assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()==SOURCE_SHA, 'original source changed'
        for f in report['files']:
            raw=(ROOT/f['file']).read_bytes();assert hashlib.sha256(raw).hexdigest()==f['sha256']
            stats=metrics(read_output(ROOT/f['file']))
            assert stats['finite_attributes'] and stats['volume_m3']>0
            assert all(stats[k]==0 for k in ('boundary_edges','nonmanifold_edges','inconsistent_winding_edges','collapsed_edges','degenerate_triangles'))
        if args.update_audit:
            report=augment_report(report);CATALOG.write_text(json.dumps(report,indent=2)+'\n')
        print('VERIFIED',len(report['files']),'source-derived closed meshes');return
    assert not OUTPUT.exists() and not CATALOG.exists(), 'refusing to overwrite delivered derivatives/catalog'
    subprocess.run([args.blender,'--background','--factory-startup','--threads','1','--python-exit-code','1','--python',str(Path(__file__).resolve()),
                    '--','--worker'],check=True,cwd=ROOT)
    report = json.loads((STAGING/'catalog.json').read_text())
    # A failed worker or invalid solid never reaches the private runtime asset directory.
    assert len(report['files']) == 12
    OUTPUT.mkdir(parents=True)
    for entry in report['files']:
        name = Path(entry['file']).name; raw=(STAGING/name).read_bytes()
        assert hashlib.sha256(raw).hexdigest()==entry['sha256']
        (OUTPUT/name).write_bytes(raw)
    report=augment_report(report)
    CATALOG.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(dict(catalog=str(CATALOG),files=len(report['files']),
        minimum_radial_gap_m=min(r['radial_gap_lower_bound_m'] for a in report['assemblies'] for r in a['rotors'])),indent=2))

if __name__=='__main__': main()
