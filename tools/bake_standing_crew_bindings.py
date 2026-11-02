#!/usr/bin/env python3
"""Recover standing bindings from the reviewed native rigged master, without editing meshes.

Usage: python tools/bake_standing_crew_bindings.py
Requires NumPy. The original FBX need not be present: the bake receipt pins the rigged GLB,
its original weights and the authored pose recipe. Reproducing the existing FBX-derived
seated binding export is an independent coordinate/weight check before standing is written.
"""
import json
import math
from pathlib import Path
import struct

import numpy as np
import tiger_crew_recipe as recipe

ROOT = Path(__file__).resolve().parents[1]
ASSET = ROOT / 'assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4'
TOLERANCE = 1e-5


class Glb:
    def __init__(self, path):
        self.path = path
        data = path.read_bytes()
        assert struct.unpack_from('<4sII', data) == (b'glTF', 2, len(data))
        size, kind = struct.unpack_from('<II', data, 12)
        assert kind == 0x4E4F534A
        self.doc = json.loads(data[20:20 + size])
        binary_size, binary_kind = struct.unpack_from('<II', data, 20 + size)
        assert binary_kind == 0x004E4942
        self.binary = data[28 + size:28 + size + binary_size]

    def read(self, index):
        accessor = self.doc['accessors'][index]
        assert 'sparse' not in accessor and not accessor.get('normalized', False)
        view = self.doc['bufferViews'][accessor['bufferView']]
        assert view.get('buffer', 0) == 0
        dtype = {5121: '<u1', 5123: '<u2', 5125: '<u4', 5126: '<f4'}[accessor['componentType']]
        width = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}[accessor['type']]
        size = np.dtype(dtype).itemsize
        offset = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
        return np.ndarray((accessor['count'], width), dtype=dtype, buffer=self.binary,
                          offset=offset, strides=(view.get('byteStride', width * size), size)).copy()

    def meshes(self):
        def walk(index):
            node = self.doc['nodes'][index]
            if 'mesh' in node:
                for primitive in self.doc['meshes'][node['mesh']]['primitives']:
                    assert primitive.get('mode', 4) == 4
                    yield node, primitive
            for child in node.get('children', []):
                yield from walk(child)
        for index in self.doc['scenes'][self.doc.get('scene', 0)]['nodes']:
            yield from walk(index)


def local_matrix(node):
    if 'matrix' in node:
        return np.array(node['matrix']).reshape(4, 4).T
    x, y, z, w = node.get('rotation', [0, 0, 0, 1])
    norm = math.sqrt(x*x + y*y + z*z + w*w)
    x, y, z, w = (v / norm for v in (x, y, z, w))
    matrix = np.eye(4)
    matrix[:3, :3] = np.array([
        [1-2*(y*y+z*z), 2*(x*y-z*w), 2*(x*z+y*w)],
        [2*(x*y+z*w), 1-2*(x*x+z*z), 2*(y*z-x*w)],
        [2*(x*z-y*w), 2*(y*z+x*w), 1-2*(x*x+y*y)],
    ]) @ np.diag(node.get('scale', [1, 1, 1]))
    matrix[:3, 3] = node.get('translation', [0, 0, 0])
    return matrix


def euler_xyz(degrees):
    x, y, z = np.radians(degrees)
    cx, cy, cz = np.cos([x, y, z])
    sx, sy, sz = np.sin([x, y, z])
    result = np.eye(4)
    result[:3, :3] = np.array([[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]]) @ np.array(
        [[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]) @ np.array(
        [[1, 0, 0], [0, cx, -sx], [0, sx, cx]])
    return result


def worlds(glb, rotations):
    result, parents = {}, {}
    def walk(index, parent, frame):
        node = glb.doc['nodes'][index]
        local = local_matrix(node)
        if node['name'] in rotations:
            local = local @ euler_xyz(rotations[node['name']])
        result[index] = frame @ local
        parents[index] = parent
        for child in node.get('children', []):
            walk(child, index, result[index])
    for index in glb.doc['scenes'][glb.doc.get('scene', 0)]['nodes']:
        walk(index, None, np.eye(4))
    return result, parents


def bindings(master, target, rotations):
    rest, parents = worlds(master, {})
    posed, _ = worlds(master, rotations)
    assert len(master.doc['skins']) == 1
    skin = master.doc['skins'][0]
    assert len(skin['joints']) == 39
    inverse = master.read(skin['inverseBindMatrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
    for index, matrix in zip(skin['joints'], inverse):
        assert np.max(np.abs(rest[index] @ matrix - np.eye(4))) < TOLERANCE
    node_by_name = {master.doc['nodes'][index]['name']: index for index in skin['joints']}
    assert set(node_by_name) == set(recipe.ARMATURE_BONES)
    bone_index = {name: i for i, name in enumerate(recipe.ARMATURE_BONES)}
    skin_indices = [bone_index[master.doc['nodes'][i]['name']] for i in skin['joints']]
    transformed = {}
    for node, primitive in master.meshes():
        assert node['skin'] == 0 and node['name'] not in transformed
        attributes = primitive['attributes']
        points = master.read(attributes['POSITION'])
        uv = master.read(attributes['TEXCOORD_0'])
        joints = np.concatenate([master.read(attributes[f'JOINTS_{i}']) for i in range(2)
                                 if f'JOINTS_{i}' in attributes], axis=1)
        weights = np.concatenate([master.read(attributes[f'WEIGHTS_{i}']) for i in range(2)
                                  if f'WEIGHTS_{i}' in attributes], axis=1).astype(float)
        assert joints.shape == weights.shape and weights.shape[1] <= 8
        assert np.isfinite(weights).all() and (weights >= 0).all()
        assert np.max(np.abs(weights.sum(axis=1)-1)) < 1e-3
        weights /= weights.sum(axis=1)[:, None]
        deltas = np.array([posed[index] @ inv for index, inv in zip(skin['joints'], inverse)])
        # The reviewed master stores bind-world positions. Its meshWorld cancels in glTF
        # skinning; applying that transform again would incorrectly shrink/rotate the body.
        homogeneous = np.column_stack([points, np.ones(len(points))])
        output = np.einsum('nwij,nj,nw->ni', deltas[joints], homogeneous, weights)[:, :3]
        rows = [sorted([[skin_indices[int(j)], float(w)] for j, w in zip(js, ws) if w > 0])
                for js, ws in zip(joints, weights)]
        transformed[node['name']] = (output, uv, rows)
    all_points = np.concatenate([item[0] for item in transformed.values()])
    lo, hi = all_points.min(axis=0), all_points.max(axis=0)
    shift = np.array([-(lo[0]+hi[0])/2, -lo[1], 0])
    normalization = np.eye(4)
    normalization[:3, 3] = shift
    result, max_error, ambiguous = [], 0., 0
    for node, primitive in target.meshes():
        assert np.max(np.abs(local_matrix(node)-np.eye(4))) < 1e-8
        points = target.read(primitive['attributes']['POSITION'])
        uv = target.read(primitive['attributes']['TEXCOORD_0'])
        original, source_uv, rows = transformed[node['name'].removesuffix('_Static')]
        for point, texcoord in zip(points, uv):
            errors = np.linalg.norm(original + shift - point, axis=1)
            matches = np.where((errors < TOLERANCE) & (np.max(np.abs(source_uv-texcoord), axis=1) < 1e-6))[0]
            assert len(matches), (node['name'], float(errors.min()))
            chosen = min(matches, key=lambda i: errors[i])
            # UV/tangent seam splits may coincide, but must never conceal differing skin weights.
            assert all(rows[int(i)] == rows[int(chosen)] for i in matches)
            ambiguous += len(matches) > 1
            result.append(rows[int(chosen)])
            max_error = max(max_error, float(errors[chosen]))
    bones = []
    for name in recipe.ARMATURE_BONES:
        index = node_by_name[name]
        parent = parents[index]
        parent_name = master.doc['nodes'][parent]['name'] if parent is not None else None
        matrix = normalization @ posed[index]
        assert np.isfinite(matrix).all() and abs(np.linalg.det(matrix)) > 1e-10
        bones.append({'name': name, 'parent': bone_index.get(parent_name), 'matrix': matrix.T.ravel().tolist()})
    assert len(result) == 914
    return bones, result, max_error, ambiguous


def main():
    report_path = ASSET / 'bake.report.json'
    report = json.loads(report_path.read_text())
    assert report['schema'] == 'kessel.tiger-crew-bake' and report['version'] == 1
    master_path = recipe.verify_file_record(ASSET, report['rigged_master'])
    master = Glb(master_path)
    records = {r['path']: r for r in report['runtime_outputs']}
    pose_recipes = {r['pose']: r['bone_rotations_degrees_xyz'] for r in report['poses']}
    def target(pose, face='face01'):
        relative = f'baked/german_panzertruppe_{face}_{pose}_static.glb'
        return Glb(recipe.verify_file_record(ASSET, records[relative]))
    seated_path = ASSET / 'animation/seated_arm_bindings.v1.json'
    seated = json.loads(seated_path.read_text())
    assert seated['source_fbx_sha256'] == recipe.SOURCE_FBX_SHA256
    seated_target = target('seated')
    assert seated['neutral_mesh_sha256'] == recipe.sha256_file(seated_target.path)
    bones, weights, seated_error, _ = bindings(master, seated_target, pose_recipes['seated'])
    assert len(bones) == len(seated['bones']) == 39
    assert len(weights) == len(seated['weights']) == seated['vertex_count'] == 914
    assert all(a['name'] == b['name'] and a['parent'] == b['parent']
               for a, b in zip(bones, seated['bones']))
    matrix_error = max(float(np.max(np.abs(np.array(a['matrix'])-b['matrix'])))
                       for a, b in zip(bones, seated['bones']))
    assert matrix_error < TOLERANCE, matrix_error
    weight_error = 0.
    for actual, expected in zip(weights, seated['weights']):
        actual, expected = dict(actual), dict(expected)
        assert actual.keys() == expected.keys()
        weight_error = max(weight_error, max(abs(actual[j]-expected[j]) for j in actual))
    assert weight_error < 1e-6, weight_error
    base = target('standing')
    bones, weights, error, seam_count = bindings(master, base, pose_recipes['standing'])
    identities = []
    for identity in recipe.IDENTITIES:
        other = target('standing', identity)
        base_meshes, other_meshes = list(base.meshes()), list(other.meshes())
        assert len(base_meshes) == len(other_meshes) == 4
        assert sum(len(other.read(p['indices'])) for _, p in other_meshes) == 564 * 3
        for (n, p), (m, q) in zip(base_meshes, other_meshes):
            assert n['name'] == m['name']
            for attribute in ('POSITION', 'NORMAL', 'TEXCOORD_0'):
                assert np.array_equal(base.read(p['attributes'][attribute]), other.read(q['attributes'][attribute]))
            assert np.array_equal(base.read(p['indices']), other.read(q['indices']))
        identities.append(recipe.file_record(other.path, logical_path=other.path.relative_to(ASSET).as_posix()))
    result = {'schema': 'panzer.source-crew-arm-bindings.v1', 'pose': 'standing',
              'source_fbx_sha256': recipe.SOURCE_FBX_SHA256,
              'source_rigged_master_sha256': recipe.sha256_file(master_path),
              'neutral_mesh_sha256': recipe.sha256_file(base.path),
              'vertex_count': len(weights), 'max_neutral_match_error_m': error,
              'bones': bones, 'weights': weights,
              'provenance': {'method': 'Original rigged master skin; recorded XYZ pose recipe; position+UV seam mapping',
                             'original_fbx_present': False, 'bake_report_sha256': recipe.sha256_file(report_path),
                             'seated_bindings_sha256': recipe.sha256_file(seated_path),
                             'seated_reconstruction_max_vertex_error_m': seated_error,
                             'seated_reconstruction_max_matrix_element_error': matrix_error,
                             'seated_reconstruction_max_weight_error': weight_error,
                             'coincident_equal_weight_seam_vertices': seam_count,
                             'verified_standing_meshes': identities}}
    output = ASSET / 'animation/standing_arm_bindings.v1.json'
    output.write_text(json.dumps(result, separators=(',', ':'))+'\n', encoding='utf-8')
    print(json.dumps({'output': str(output), 'sha256': recipe.sha256_file(output),
                      'vertices': len(weights), 'bones': len(bones), 'max_error_m': error,
                      'seated_matrix_error': matrix_error, 'seated_weight_error': weight_error}))


if __name__ == '__main__':
    main()
