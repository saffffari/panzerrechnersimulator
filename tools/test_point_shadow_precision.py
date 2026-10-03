#!/usr/bin/env python3
"""CPU f32 receiver/caster arithmetic witness; never opens a device or renders a frame.

Compare the source-relative shader expressions against independent double-precision
ray/plane intersections. The 1 mm blocker is separate geometry; shadow bias stays 50 um.
This does not establish GPU interpolation accuracy or visual acceptance of the cabin.
"""
import argparse
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
F32 = np.float32


def unit(v):
    return v / np.linalg.norm(v)


def cube_ray(face, uv):
    x, y = np.asarray(uv) * 2 - 1
    return np.array([(1, -y, -x), (-1, -y, x), (x, 1, y),
                     (x, -1, -y), (x, -y, 1), (-x, -y, -1)][face])


def run():
    mesh = (ROOT / 'shaders/stylized_mesh.wgsl').read_text()
    caster = (ROOT / 'shaders/point_shadow.wgsl').read_text()
    host = (ROOT / 'src/cabin_shadows.rs').read_text()
    # Bind the numeric witness to the live expressions, including the unchanged bias.
    assert 'linear*position+(model[3].xyz-cam.cam_pos)' in caster
    assert 'face_view_proj_range(glam::Vec3::ZERO, face, (NEAR,far))' in host
    assert 'cam_pos: lamp.pos.to_array()' in host
    assert 'cross(dpdx(in.relative_world), dpdy(in.relative_world))' in mesh
    assert 'in.relative_world + (cam.cam_pos - source.xyz)' in mesh
    assert 'max(0.00005,major*0.00000024)' in mesh
    cases = 0
    samples = 0
    maximum_error = 0.0
    legacy_false_shadow_taps = 0
    source_local = np.array([.375, 1.625, -.8125], dtype=F32)
    eye_local = np.array([.125, 2.25, -.25], dtype=F32)
    for translation in [(0, 0, 0), (0, 0, 178), (985, 5.5, 1205), (5000, 4, 5000)]:
        origin = np.asarray(translation, dtype=F32)
        source = origin + source_local
        eye = origin + eye_local
        for face in range(6):
            for distance in [.25, 1.5, 5.5]:
                for tilt in [.5, 2, 10]:
                    for uv in [(.5013, .5021), (.1, .4), (.9, .6)]:
                        ray = cube_ray(face, uv)
                        forward = unit(ray)
                        tangent = unit(np.cross(forward, [0, 1, 0] if abs(forward[1]) < .8 else [1, 0, 0]))
                        normal = unit(-forward + tangent * tilt)
                        bitangent = unit(np.cross(normal, tangent))
                        tangent = unit(np.cross(bitangent, normal))
                        center = source_local.astype(float) + ray * distance
                        # Sub-mm screen-plane steps stress subtraction at distant map origins.
                        plane = np.array([center, center + tangent * .0008,
                                          center + bitangent * .0008], dtype=F32)
                        exact_normal = unit(np.cross(plane[1].astype(float) - plane[0],
                                                     plane[2].astype(float) - plane[0]))
                        offset64 = plane[0].astype(float) + origin.astype(float) - source
                        exact_plane = np.dot(exact_normal, offset64)
                        relative = plane + (origin - eye)
                        receiver = relative[0] + (eye - source)
                        receiver_normal = unit(np.cross(relative[1] - relative[0], relative[2] - relative[0]))
                        old_world = plane + origin
                        old_cross = np.cross(old_world[1] - old_world[0], old_world[2] - old_world[0])
                        old_normal = unit(old_cross) if np.linalg.norm(old_cross) else exact_normal
                        first = np.floor(np.array(uv) * 512 - .5)
                        for y in range(2):
                            for x in range(2):
                                tap_ray = cube_ray(face, (first + [x, y] + .5) / 512)
                                exact_major = exact_plane / np.dot(exact_normal, tap_ray)
                                tap_f32 = tap_ray.astype(F32)
                                major = F32(np.dot(receiver_normal, receiver) / np.dot(receiver_normal, tap_f32))
                                # The caster evaluates the same source local plane in f64 here;
                                # its actual f32 vertex arithmetic is tested independently below.
                                stored = F32(exact_major / 32768.)
                                bias = F32(max(.00005, np.max(np.abs(receiver)) * .00000024))
                                reference = F32((major - bias) / F32(32768.))
                                error = abs(float(major) - exact_major)
                                maximum_error = max(maximum_error, error)
                                assert reference <= stored, ('self-shadow', translation, face, distance, tilt, uv, error)
                                # Parallel source geometry 1 mm closer along this cube face axis.
                                blocker = F32((exact_major - .001) / 32768.)
                                assert reference > blocker, ('thin blocker erased', translation, face, error)
                                old_major = np.dot(old_normal, old_world[0] - source) / np.dot(old_normal, tap_ray)
                                legacy_false_shadow_taps += int(old_major - bias > exact_major)
                                samples += 1
                        # Explicit source-relative raster vertex distances remain accurate.
                        relative_caster = plane + (origin - source)
                        expected_caster = plane.astype(float) + origin.astype(float) - source.astype(float)
                        assert np.max(np.abs(relative_caster - expected_caster)) < 1e-6
                        cases += 1
    assert legacy_false_shadow_taps > 0, 'fixture must expose the old absolute-position failure'
    return {'status': 'PASS', 'scope': 'CPU f32 arithmetic only; GPU and cabin image acceptance pending',
            'cases': cases, 'pcf_taps': samples, 'maximum_plane_depth_error_m': maximum_error,
            'legacy_false_shadow_taps': legacy_false_shadow_taps,
            'retained_blocker_separation_m': .001, 'minimum_bias_m': .00005,
            'translated_origins_m': [[0, 0, 0], [0, 0, 178], [985, 5.5, 1205], [5000, 4, 5000]]}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = run()
    text = json.dumps(result, indent=2) + '\n'
    if args.output:
        with args.output.open('x') as output:
            output.write(text)
    print(text, end='')
