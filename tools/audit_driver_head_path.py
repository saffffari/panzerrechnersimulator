#!/usr/bin/env python3
"""Bounded CPU source-geometry clearance audit; no textures, rendering or source mutation."""
import json
import mmap
import struct
from pathlib import Path

import numpy as np

from audit_driver_optic import trs

ROOT = Path(__file__).resolve().parents[1]


def nearby_triangles(attach_roof_pane=True):
    hinge = np.array([1.382032, 1.9149045, 1.8893634])
    angle = np.radians(-95)
    rot = np.array([[np.cos(angle), -np.sin(angle), 0],
                    [np.sin(angle), np.cos(angle), 0], [0, 0, 1]])
    motion = np.eye(4)
    motion[:3, :3] = rot
    motion[:3, 3] = hinge - rot @ hinge
    lower, upper = np.array([.3, 1.45, 1.4]), np.array([1.6, 2.8, 2.6])
    collected, labels = [], []
    for filename in ('tiger_hull.glb', 'tiger_turret.glb', 'tiger_gun.glb', 'tiger_rtt_hull.glb'):
        with (ROOT / 'assets/tanks/tiger/baked' / filename).open('rb') as file:
            file.read(12)
            size, _ = struct.unpack('<II', file.read(8))
            doc = json.loads(file.read(size))
            file.read(8)
            start = file.tell()
            with mmap.mmap(file.fileno(), 0, access=mmap.ACCESS_READ) as data:
                def accessor(index):
                    spec = doc['accessors'][index]
                    view = doc['bufferViews'][spec['bufferView']]
                    dtype = {5121: 'u1', 5123: '<u2', 5125: '<u4', 5126: '<f4'}[spec['componentType']]
                    width = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4}[spec['type']]
                    size = np.dtype(dtype).itemsize
                    return np.ndarray((spec['count'], width), dtype=dtype, buffer=data,
                                      offset=start + view.get('byteOffset', 0) + spec.get('byteOffset', 0),
                                      strides=(view.get('byteStride', size * width), size)).copy()

                def visit(index, parent, moving=False):
                    node = doc['nodes'][index]
                    model = parent @ trs(node)
                    moving = moving or node.get('name') == 'Drivers Hatch Pivot'
                    pane = attach_roof_pane and filename == 'tiger_rtt_hull.glb' and node.get('name') == 'Hull_0300'
                    posed = motion @ model if moving or pane else model
                    if 'mesh' in node:
                        for primitive in doc['meshes'][node['mesh']]['primitives']:
                            spec = doc['accessors'][primitive['attributes']['POSITION']]
                            corners = np.array([[x, y, z, 1] for x in (spec['min'][0], spec['max'][0])
                                                for y in (spec['min'][1], spec['max'][1])
                                                for z in (spec['min'][2], spec['max'][2])]) @ posed.T
                            if np.any(corners[:, :3].max(0) < lower) or np.any(corners[:, :3].min(0) > upper):
                                continue
                            vertices = accessor(primitive['attributes']['POSITION'])
                            vertices = vertices @ posed[:3, :3].T + posed[:3, 3]
                            indices = accessor(primitive['indices']).ravel() if 'indices' in primitive else np.arange(len(vertices))
                            triangles = vertices[indices.reshape(-1, 3)]
                            keep = np.all(triangles.max(1) >= lower, axis=1) & np.all(triangles.min(1) <= upper, axis=1)
                            triangles = triangles[keep]
                            collected.append(triangles)
                            labels.extend([filename + ':' + node.get('name', str(index))] * len(triangles))
                    for child in node.get('children', []):
                        visit(child, model, moving)

                for node in doc['scenes'][doc.get('scene', 0)]['nodes']:
                    visit(node, np.eye(4))
    return np.concatenate(collected), np.array(labels)


def closest_triangle(points, triangles, labels):
    a = triangles[:, 0]
    ab, ac = triangles[:, 1] - a, triangles[:, 2] - a
    normal = np.cross(ab, ac)
    normal /= np.maximum(np.linalg.norm(normal, axis=1)[:, None], 1e-12)
    d00, d01, d11 = np.sum(ab * ab, axis=1), np.sum(ab * ac, axis=1), np.sum(ac * ac, axis=1)
    denominator = d00 * d11 - d01 * d01
    results = []
    for point in points:
        ap = point - a
        plane_dist = np.sum(ap * normal, axis=1)
        projected = ap - plane_dist[:, None] * normal
        d20, d21 = np.sum(projected * ab, axis=1), np.sum(projected * ac, axis=1)
        v = np.divide(d11 * d20 - d01 * d21, denominator, out=np.full_like(denominator, -1), where=denominator > 1e-15)
        w = np.divide(d00 * d21 - d01 * d20, denominator, out=np.full_like(denominator, -1), where=denominator > 1e-15)
        distance = np.where((v >= 0) & (w >= 0) & (v + w <= 1), plane_dist ** 2, np.inf)
        for j, k in ((0, 1), (1, 2), (2, 0)):
            edge = triangles[:, k] - triangles[:, j]
            fraction = np.sum((point - triangles[:, j]) * edge, axis=1) / np.maximum(np.sum(edge * edge, axis=1), 1e-20)
            nearest = triangles[:, j] + np.clip(fraction, 0, 1)[:, None] * edge
            distance = np.minimum(distance, np.sum((point - nearest) ** 2, axis=1))
        index = int(distance.argmin())
        results.append(dict(clearance_m=float(np.sqrt(distance[index])), node=str(labels[index]),
                            point_m=point.tolist(), triangle=triangles[index].tolist()))
    return results


def audit_path(vertices, triangles, labels):
    points = []
    for start, end in zip(vertices[:-1], vertices[1:]):
        start, end = np.array(start), np.array(end)
        points.extend(np.linspace(start, end, int(np.ceil(np.linalg.norm(end - start) / .01)) + 1))
    samples = closest_triangle(points, triangles, labels)
    return dict(path_m=vertices, min_clearance=min(samples, key=lambda sample: sample['clearance_m']), samples=samples)


def main():
    triangles, labels = nearby_triangles(attach_roof_pane=True)
    cases = {
        'roof_lateral_then_up': [[.535, 1.733, 1.82], [.98, 1.733, 1.89], [.98, 2.25, 1.89]],
        'roof_direct_diagonal': [[.535, 1.733, 1.82], [.98, 2.25, 1.89]],
        'visor_forward_to_2_10': [[.535, 1.733, 1.82], [.535, 1.733, 2.1]],
        'visor_forward_to_2_20': [[.535, 1.733, 1.82], [.535, 1.733, 2.2]],
    }
    report = dict(method='Actual source hierarchy; complete driver lid plus Hull_0300 pane rotated by existing -Z 95deg hinge transform before bounding. Exact closest-triangle distance to eye-centered sample points at <=10mm spacing. Not a full body capsule or historical ergonomic validation. Default turret yaw only; original holder retained because forward path stays behind it. No textures or GPU.',
                  hatch_open_angle_deg=95, roof_pane_attached=True, nearby_triangles=len(triangles), cases={})
    for name, vertices in cases.items():
        report['cases'][name] = audit_path(vertices, triangles, labels)
        print(name, report['cases'][name]['min_clearance'])
    output = ROOT / 'artifacts/astra-audit-20260905/driver-head-path-clearance-attached-pane.json'
    output.write_text(json.dumps(report, indent=2) + '\n')
    print(output)


if __name__ == '__main__':
    main()
