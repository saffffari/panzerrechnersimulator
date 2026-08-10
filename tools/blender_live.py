# Live collaboration script — Claude edits this, you type run() in the
# Blender Python console to execute the latest version.
#
# Current job: empty casing with real wall thickness.
#   Duplicates 'case_src' -> 'shell_case', adds a Solidify weighted by a
#   vertex group that fades to ZERO toward the base — the rim/extractor
#   geometry at the bottom is never touched (that's what got mangled last
#   time). Exports shell_case.glb.
import bpy
import os

OUT = '/home/alex/tank_simulator/assets/88mm/baked'
os.makedirs(OUT, exist_ok=True)

src = bpy.data.objects.get('case_src') or bpy.data.objects.get('defaultMaterial')
if src is None:
    print('MISSING: could not find the empty case object')
else:
    old = bpy.data.objects.get('shell_case')
    if old:
        bpy.data.objects.remove(old)
    ob = src.copy()
    ob.data = src.data.copy()
    ob.name = 'shell_case'
    mw = src.matrix_world.copy()
    ob.parent = None
    ob.matrix_world = mw
    ob.location.x += 9.0
    bpy.context.scene.collection.objects.link(ob)

    # --- vertex group: weight 1 near the mouth, 0 near the base ---
    me = ob.data
    # find the long axis in local space
    xs = [v.co for v in me.vertices]
    lo = [min(c[i] for c in xs) for i in range(3)]
    hi = [max(c[i] for c in xs) for i in range(3)]
    spans = [hi[i] - lo[i] for i in range(3)]
    axis = spans.index(max(spans))
    # which end of that axis is the mouth? -> the end higher in world space
    from mathutils import Vector
    p_lo = ob.matrix_world @ Vector([(lo[i] if i == axis else (lo[i] + hi[i]) / 2) for i in range(3)])
    p_hi = ob.matrix_world @ Vector([(hi[i] if i == axis else (lo[i] + hi[i]) / 2) for i in range(3)])
    mouth_at_hi = p_hi.z >= p_lo.z

    vg = ob.vertex_groups.new(name='mouth')
    span = max(spans[axis], 1e-6)
    for v in me.vertices:
        t = (v.co[axis] - lo[axis]) / span          # 0 at lo end, 1 at hi end
        if not mouth_at_hi:
            t = 1.0 - t                              # 0 at base, 1 at mouth
        w = min(max((t - 0.15) / 0.20, 0.0), 1.0)    # 0 below 15%, full above 35%
        if w > 0.0:
            vg.add([v.index], w, 'REPLACE')

    # --- solidify, driven by that group ---
    sol = ob.modifiers.new('Solidify', 'SOLIDIFY')
    sol.thickness = 0.02 * min(spans[(axis + 1) % 3], spans[(axis + 2) % 3])  # ~2% of diameter
    sol.offset = -1.0            # inward only; outer surface unchanged
    sol.use_even_offset = True
    sol.vertex_group = 'mouth'
    sol.thickness_vertex_group = 0.0   # weight 0 -> zero thickness (fade out, not clamp)

    # --- export ---
    bpy.ops.object.select_all(action='DESELECT')
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob
    saved = ob.matrix_world.copy()
    ob.location = (0.0, 0.0, 0.0)
    path = os.path.join(OUT, 'shell_case.glb')
    bpy.ops.export_scene.gltf(
        filepath=path,
        use_selection=True,
        export_format='GLB',
        export_yup=True,
        export_apply=True,       # bake the solidify into the GLB
    )
    ob.matrix_world = saved
    print('exported', path)
    print('DONE')
