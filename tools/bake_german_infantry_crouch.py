#!/usr/bin/env python3
"""CPU-only articulated crouch derivatives of the preserved CC0 German FBX; no renders.

Run Blender with --background --factory-startup --threads 2 --python-exit-code 1 --python this_file.
The original FBX, existing rig/crew poses and interactive Blender document remain unchanged.
"""
import json
import math
from pathlib import Path
import sys

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bake_crew_soldier_variants as variants
import bake_soviet_infantry as skin_quality

ROOT = Path(__file__).resolve().parents[1]
ASSET = ROOT / 'assets/flavor/Characters/german-ww2-soldier-cc0'
SOURCE = ASSET / 'source/LowpolySoldier.fbx'
OUTPUT = ASSET / 'baked/infantry_crouch_v1'


def bake(phase, aimed):
    arm = variants.import_character(SOURCE)
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    standing_points = [p for page in skin_quality.world_vertex_points(meshes, evaluated=True).values() for p in page]
    minimum = [min(p[axis] for p in standing_points) for axis in range(3)]
    maximum = [max(p[axis] for p in standing_points) for axis in range(3)]
    scale = 1.75 / (maximum[2] - minimum[2])
    source_floor = minimum[2]
    source_center_x = (minimum[0] + maximum[0]) * 0.5
    root_transform = Matrix.Scale(scale, 4) @ Matrix.Translation((-source_center_x, 0.0, -source_floor))
    original = skin_quality.evaluated_deformation_snapshot(meshes)
    # Preserve volume through original FBX joint rolls; linear blended twists can collapse a
    # narrow source knee/ankle edge even in an otherwise valid IK pose.
    for mesh in meshes:
        for modifier in mesh.modifiers:
            if modifier.type == 'ARMATURE':
                modifier.use_deform_preserve_volume = True
    # This FBX carries rest compensation in pose channels: Bone.tail_local is not the imported
    # standing ankle. Pin the actual evaluated standing ankle rather than an incorrect data rest.
    feet = {side: arm.matrix_world @ arm.pose.bones[side + 'Leg'].tail
            for side in ('Left', 'Right')}
    foot_rotations = {side: (arm.matrix_world @ arm.pose.bones[side + 'Foot'].matrix).to_quaternion()
                      for side in ('Left', 'Right')}
    drop = (0.65 / scale) * phase
    retreat = (0.22 / scale) * phase
    # Pelvis descends while ankle targets stay on the same ground: thigh/shin IK bends the knees.
    # This is armature deformation, not runtime full-body scaling or sinking through the floor.
    arm.location.z -= drop
    arm.location.y += retreat
    for bone in arm.pose.bones:
        bone.ik_stretch = 0.0
        bone.lock_ik_x = bone.lock_ik_y = bone.lock_ik_z = False
        bone.ik_stiffness_x = bone.ik_stiffness_y = bone.ik_stiffness_z = 0.0
    for side, sign in (('Left', 1.0), ('Right', -1.0)):
        foot = feet[side]
        pole = (float(foot.x), float(foot.y - 0.65), 0.35)
        variants.add_limb_ik(arm, side + 'Leg', tuple(foot), pole, 90.0 * sign)
        arm.pose.bones[side + 'Leg'].constraints[-1].use_stretch = False
        fixed = variants.target(side + '_planted_rotation', tuple(foot))
        fixed.rotation_mode = 'QUATERNION'
        fixed.rotation_quaternion = foot_rotations[side]
        constraint = arm.pose.bones[side + 'Foot'].constraints.new('COPY_ROTATION')
        constraint.target = fixed
        constraint.owner_space = 'WORLD'
        constraint.target_space = 'WORLD'
    if aimed:
        hands = ((0.14, -0.54 + retreat, 1.39 - drop), (-0.12, -0.19 + retreat, 1.39 - drop))
    else:
        hands = ((0.24, -0.05 + retreat, 1.02 - drop), (-0.22, 0.01 + retreat, 0.97 - drop))
    for (side, sign), hand in zip((('Left', 1.0), ('Right', -1.0)), hands):
        variants.add_limb_ik(arm, side + 'ForeArm', hand, (0.50 * sign, -0.17 + retreat, 1.22 - drop), 90.0 * sign)
    # FBX bone rolls differ left/right. Solve the pole offset from evaluated limb geometry so
    # both knees bend in their sagittal plane toward model forward (-Y), never sideways.
    def knee_plane(side):
        bpy.context.view_layer.update()
        evaluated = arm.evaluated_get(bpy.context.evaluated_depsgraph_get())
        hip, knee, ankle = [evaluated.matrix_world @ evaluated.pose.bones[side + name].head
                            for name in ('UpLeg', 'Leg', 'Foot')]
        axis = (ankle - hip).normalized()
        actual = knee - hip
        actual = (actual - axis * actual.dot(axis)).normalized()
        forward = Vector((0, -1, 0))
        desired = (forward - axis * forward.dot(axis)).normalized()
        return actual.dot(desired), math.atan2(axis.dot(actual.cross(desired)), actual.dot(desired))
    for side in ('Left', 'Right'):
        constraint = arm.pose.bones[side + 'Leg'].constraints[-1]
        for _ in range(3):
            score, angle = knee_plane(side)
            if score > 0.99999:
                break
            original_angle = constraint.pole_angle
            candidates = []
            for sign in (-1, 1):
                constraint.pole_angle = original_angle + sign * angle
                candidates.append((knee_plane(side)[0], constraint.pole_angle))
            constraint.pole_angle = max(candidates)[1]
    bpy.context.view_layer.update()
    quality = skin_quality.deformation_quality_between(
        original, skin_quality.evaluated_deformation_snapshot(meshes), height_m=1.75 / scale)
    evaluated = arm.evaluated_get(bpy.context.evaluated_depsgraph_get())
    head_bone = evaluated.pose.bones['Head']
    # The midpoint of the original head bone is an explicit eye proxy, not a forensic eye landmark.
    # Record it after the same fixed root transform as the visible body so sensing follows the pose.
    head_points = [root_transform @ evaluated.matrix_world @ point
                   for point in (head_bone.head, head_bone.tail)]
    eye_proxy = (head_points[0] + head_points[1]) * 0.5
    def yup(point):
        return [point.x, point.z, -point.y]
    joints = {}
    for side in ('Left', 'Right'):
        points = [root_transform @ evaluated.matrix_world @ evaluated.pose.bones[side + bone].head
                  for bone in ('UpLeg', 'Leg', 'Foot')]
        knee_angle = math.degrees((points[0] - points[1]).angle(points[2] - points[1]))
        joints[side] = dict(hip=list(points[0]), knee=list(points[1]), ankle=list(points[2]),
                            knee_angle_deg=knee_angle, forward_plane_alignment=knee_plane(side)[0])
    static = variants.bake_skin_to_static()
    # One scale established from the original standing body applies to every pose. Normalizing
    # each crouched mesh to standing height would undo the articulated motion and distort limbs.
    variants.transform_vertices(static, root_transform)
    minimum, maximum = variants.exact_vertex_bounds(static)
    label = f'german_infantry_{"aim" if aimed else "relaxed"}_crouch_{round(phase * 4):02d}'
    out = OUTPUT / (label + '.glb')
    result = bpy.ops.export_scene.gltf(filepath=str(out), export_format='GLB', use_selection=False,
        export_apply=True, export_texcoords=True, export_normals=True, export_materials='EXPORT',
        export_image_format='AUTO', export_skins=False, export_animations=False, export_yup=True)
    if 'FINISHED' not in result:
        raise RuntimeError(f'export failed: {result}')
    record = dict(name=label, path=str(out.relative_to(ROOT)), phase=phase, aimed=aimed,
        scale_from_standing_source=scale, height_m=maximum[2] - minimum[2],
        fixed_ankle_targets_blender_z_up={side:list(root_transform @ point) for side,point in feet.items()},
        bounds_blender_z_up=[minimum, maximum], joints_blender_z_up=joints,
        head_bone_endpoints_y_up=[yup(point) for point in head_points],
        eye_proxy_y_up=yup(eye_proxy),
        eye_proxy_definition='Midpoint of the evaluated source Head bone; an authored eye proxy, not anatomical calibration.',
        deformation_quality=quality, source_sha256=variants.source_bake.sha256(SOURCE),
        output_sha256=variants.source_bake.sha256(out), bytes=out.stat().st_size)
    print('CROUCH_RECEIPT', json.dumps(record), flush=True)
    return record


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    records = [bake(phase / 4.0, aimed) for aimed in (False, True) for phase in range(5)]
    for record in records:
        if not record['deformation_quality']['passed']:
            raise RuntimeError(f"skin validation failed: {record['name']}")
        if abs(record['height_m'] - (1.75 - 0.65 * record['phase'])) > 0.012:
            raise RuntimeError(f"crouch did not lower the articulated body correctly: {record['name']}")
        if abs(record['bounds_blender_z_up'][0][2]) > 0.012:
            raise RuntimeError(f"feet left the original ground datum: {record['name']}")
        for side, joint in record['joints_blender_z_up'].items():
            if record['phase'] == 1.0 and joint['knee_angle_deg'] >= 90.0:
                raise RuntimeError(f"full crouch failed to bend {side} knee")
            if joint['forward_plane_alignment'] < 0.999:
                raise RuntimeError(f"{side} knee bent outside the forward sagittal plane")
            expected = Vector(record['fixed_ankle_targets_blender_z_up'][side])
            if (Vector(joint['ankle']) - expected).length > 0.012:
                raise RuntimeError(f"{side} ankle left its planted target")
    manifest = dict(schema='kessel.german-infantry-articulated-crouch.v1',
        source='assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier.fbx',
        author='nisu', license='CC0-1.0', source_page='https://opengameart.org/content/rigged-lowpoly-ww2-soldier',
        method='Original weighted49-bone source; fixed ankle IK, actual knee bend and planted foot rotations; fixed standing-derived scale. Runtime static-pose playback, not runtime skeletal animation.',
        runtime_integrated=False, visual_review='pending', poses=records)
    (OUTPUT / 'manifest.v1.json').write_text(json.dumps(manifest, indent=2) + '\n')


if __name__ == '__main__':
    main()
