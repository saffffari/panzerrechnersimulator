"""Build a Blender asset library from the pizza-pack GLBs.

Run:
    blender --background --python tools/worldgen/build_asset_library.py -- \
        assets/pizza/_extracted assets/pizza/_asset_library [pack_filter]

One .blend per (pack, category) directory of GLBs. Each GLB becomes a
collection named after the file, marked as an asset and assigned to a
catalog "<pack>/<category>". Previews are generated headless where the
GPU allows; regenerate any blanks from the Asset Browser if needed.
"""

import sys
import time
import uuid
from pathlib import Path

import bpy

NAMESPACE = uuid.UUID("b5c7a6a0-0000-4000-8000-6b657373656c")  # stable catalog ids

# Directories (relative to _extracted) whose GLBs are duplicates or unwanted.
SKIP_SUBSTRINGS = [
    "Tree branches separated",  # PSX Nature ships every tree twice
]


def short_pack(name: str) -> str:
    # "PSX Mega Pack II v1.8" -> "PSX Mega Pack II"
    parts = name.split(" v")
    return parts[0].replace(" 3.1.3", "").strip()


def find_categories(extracted: Path, pack_filter: str):
    """Yield (pack, category, [glb paths]) for every dir holding GLBs."""
    dirs = {}
    for glb in sorted(extracted.rglob("*.glb")):
        rel = glb.relative_to(extracted)
        if any(s in str(rel) for s in SKIP_SUBSTRINGS):
            continue
        pack = short_pack(rel.parts[0])
        if pack_filter and pack_filter.lower() not in pack.lower():
            continue
        # category = last dir unless it's a generic GLB folder
        cat = glb.parent.name
        if cat.upper().startswith("GLB") or cat == "other-formats":
            cat = "Models"
        dirs.setdefault((pack, cat), []).append(glb)
    return sorted(dirs.items())


def slug(s: str) -> str:
    return "".join(c if c.isalnum() else "_" for c in s).strip("_").lower()


def build_blend(glbs, out_path: Path, catalog_id: str):
    bpy.ops.wm.read_homefile(use_empty=True)
    scene_col = bpy.context.scene.collection
    for glb in glbs:
        before = set(bpy.data.objects)
        try:
            bpy.ops.import_scene.gltf(filepath=str(glb))
        except Exception as e:
            print(f"  !! import failed {glb.name}: {e}")
            continue
        new_objs = [o for o in bpy.data.objects if o not in before]
        col = bpy.data.collections.new(glb.stem)
        scene_col.children.link(col)
        for o in new_objs:
            for uc in o.users_collection:
                uc.objects.unlink(o)
            col.objects.link(o)
        col.asset_mark()
        col.asset_data.catalog_id = catalog_id
        try:
            with bpy.context.temp_override(id=col):
                bpy.ops.ed.lib_id_generate_preview()
        except Exception as e:
            print(f"  preview failed for {glb.stem}: {e}")
    # The last queued preview job only flushes on save; give it a bounded
    # head start, then save (which completes it).
    deadline = time.time() + 20
    while bpy.app.is_job_running("RENDER_PREVIEW") and time.time() < deadline:
        time.sleep(0.1)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(out_path), compress=True)
    print(f"  saved {out_path} ({len(glbs)} assets)")


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    if len(argv) < 2:
        sys.exit("args: <extracted_dir> <library_dir> [pack_filter]")
    extracted, library = Path(argv[0]).resolve(), Path(argv[1]).resolve()
    pack_filter = argv[2] if len(argv) > 2 else ""

    cats = find_categories(extracted, pack_filter)
    print(f"{len(cats)} categories, {sum(len(g) for _, g in cats)} models")

    # blender_assets.cats.txt — pack/category tree with stable uuids
    lines = ["VERSION 1", ""]
    seen_packs = set()
    entries = []
    for (pack, cat), glbs in cats:
        if pack not in seen_packs:
            seen_packs.add(pack)
            lines.append(f"{uuid.uuid5(NAMESPACE, pack)}:{pack}:{pack}")
        cid = str(uuid.uuid5(NAMESPACE, f"{pack}/{cat}"))
        lines.append(f"{cid}:{pack}/{cat}:{cat}")
        entries.append((pack, cat, glbs, cid))
    library.mkdir(parents=True, exist_ok=True)
    (library / "blender_assets.cats.txt").write_text("\n".join(lines) + "\n")

    for pack, cat, glbs, cid in entries:
        print(f"== {pack} / {cat} ({len(glbs)}) ==")
        out = library / slug(pack) / f"{slug(cat)}.blend"
        build_blend(glbs, out, cid)


main()
