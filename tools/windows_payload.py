#!/usr/bin/env python3
"""Compute and copy the runtime asset closure for the portable Windows folder.

The owner's asset library (about 3.4 GB of purchased kits, git-ignored) never goes to GitHub and is
never copied whole. This tool selects the files the game loads and copies them, at their logical
repository paths, into a package root that the game opens with ``KESSEL_ASSET_ROOT``:

1. every source GLB locked by ``catalog/runtime_assets.v1.json`` and every file locked by
   ``catalog/runtime_payload.v1.json`` (the reviewed macOS closure);
2. every world manifest and runtime catalog named by a string literal in the shipping source
   (``src/``, minus captures, proofs, reviews, benches and tests), each walked recursively: any
   JSON string that names an existing file, relative to the asset root or to the manifest, is in,
   and JSON files reached that way are walked in turn; a manifest's own directory (heights, layer
   files, edits overlays and scatter layers live beside it) is copied whole;
3. every exact ``assets/...`` or ``catalog/...`` file literal in the shipping source;
4. directories the source composes names inside at run time (fonts, sounds, AI weights, Tiger
   baked meshes) — listed in ``DYNAMIC_DIRS``;
5. every ``present`` path of one or more ``KESSEL_ASSET_TRACE`` files from real play sessions.

Read-only on the asset root. ``--output`` must not exist yet (or be empty).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
from pathlib import Path

SKIP_SOURCE = re.compile(
    r"(capture|proof|review|bench|_tests?\.rs$|tests\.rs$|editor_smoke|gallery|showreel|astra_|"
    r"filmstrip|mechanic_capture|combat_scene_suite|scene_suite)"
)
LITERAL = re.compile(r'"((?:assets|catalog|source_assets)/[^"{}\n]+?)"')
DYNAMIC_DIRS = [
    "assets/fonts",
    "assets/sounds",
    "assets/ai",
    "assets/tanks/tiger/baked",
    "assets/tanks/T34-76",
    "assets/88mm",
    "assets/ui",
    "assets/pixelated",
    "assets/render",
]
# Never shipped even when a manifest directory is copied whole: authoring and review by-products.
EXCLUDE_PARTS = {"_review", "review", "__pycache__", ".git"}
EXCLUDE_SUFFIXES = {".blend", ".blend1", ".psd", ".kra", ".xcf", ".mp4", ".mov", ".zip", ".7z"}


def shipping_sources(repo: Path) -> list[Path]:
    return sorted(p for p in (repo / "src").rglob("*.rs") if not SKIP_SOURCE.search(p.name))


# Authoring receipts and review records name every input of a bake, not what the game loads.
EXCLUDE_NAME = re.compile(r"(\.receipt\.|receipt\.json$|review|gallery|contact[-_]sheet)", re.IGNORECASE)


def is_shippable(rel: str) -> bool:
    path = Path(rel)
    return (
        not (set(path.parts) & EXCLUDE_PARTS)
        and path.suffix.lower() not in EXCLUDE_SUFFIXES
        and not EXCLUDE_NAME.search(path.name)
    )


class Closure:
    def __init__(self, asset_root: Path, repo: Path):
        self.asset_root = asset_root
        self.repo = repo
        self.files: dict[str, str] = {}  # logical path -> reason
        self.walked: set[str] = set()
        self.missing: dict[str, str] = {}

    def logical(self, path: Path) -> str | None:
        try:
            return path.resolve().relative_to(self.asset_root.resolve()).as_posix()
        except ValueError:
            return None

    def add(self, rel: str, reason: str) -> bool:
        rel = rel.replace("\\", "/").lstrip("/")
        if not rel.startswith(("assets/", "catalog/", "source_assets/")):
            return False
        if not self.source_path(rel).is_file():
            self.missing.setdefault(rel, reason)
            return False
        if not is_shippable(rel):
            return False
        self.files.setdefault(rel, reason)
        if rel.endswith(".json") and rel not in self.walked:
            self.walk_json(rel)
        return True

    def add_dir(self, rel: str, reason: str) -> None:
        root = self.asset_root / rel
        if not root.is_dir():
            self.missing.setdefault(rel + "/", reason)
            return
        for path in sorted(root.rglob("*")):
            if path.is_file():
                logical = self.logical(path)
                if logical and is_shippable(logical):
                    self.files.setdefault(logical, reason)

    def source_path(self, rel: str) -> Path:
        # Files present in the building checkout (tracked: catalogs, mission sheets, small runtime
        # data) ship in that checkout's version, which matches the exe; the git-ignored library
        # comes from the asset root. A fresh worktree holds nothing untracked under assets/.
        path = self.repo / rel
        return path if path.is_file() else self.asset_root / rel

    def walk_json(self, rel: str) -> None:
        self.walked.add(rel)
        try:
            doc = json.loads(self.source_path(rel).read_text(encoding="utf-8"))
        except (OSError, ValueError, UnicodeDecodeError):
            return
        base = Path(rel).parent
        strings: list[str] = []

        def collect(value):
            if isinstance(value, str):
                strings.append(value)
            elif isinstance(value, list):
                for item in value:
                    collect(item)
            elif isinstance(value, dict):
                for item in value.values():
                    collect(item)

        collect(doc)
        for value in strings:
            if len(value) > 300 or "\n" in value or not re.search(r"\.[A-Za-z0-9]{2,5}$", value):
                continue
            for candidate in (Path(value), base / value):
                text = candidate.as_posix()
                if text.startswith("../") or os.path.isabs(text):
                    try:
                        text = (self.asset_root / candidate).resolve().relative_to(self.asset_root.resolve()).as_posix()
                    except (ValueError, OSError):
                        continue
                if (self.asset_root / text).is_file() or (self.repo / text).is_file():
                    self.add(text, f"named by {rel}")
                    break


def audio_library(closure: Closure) -> None:
    """The soundscape plays clips by id: ship the library manifest, the cue sheet, and only the clips
    of the cues the sheet enables (`src/audio_soundscape.rs`)."""
    root = "assets/audio/library"
    manifest_path = closure.asset_root / root / "manifest.json"
    cues_path = closure.asset_root / root / "soundscape.json"
    if not manifest_path.is_file() or not cues_path.is_file():
        return
    closure.walked.update({f"{root}/manifest.json", f"{root}/soundscape.json"})
    closure.files[f"{root}/manifest.json"] = "soundscape library"
    closure.files[f"{root}/soundscape.json"] = "soundscape cue sheet"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    cues = json.loads(cues_path.read_text(encoding="utf-8")).get("cues", [])
    wanted = {cue.get("asset") for cue in cues if cue.get("enabled", True)}
    for asset in manifest.get("assets", []):
        if asset.get("id") not in wanted:
            continue
        playback = asset.get("playback_path") or f"clips/{asset['id']}.wav"
        if not closure.add(f"{root}/{playback}", "enabled soundscape cue"):
            for candidate in (closure.asset_root / root / "clips").glob(f"{asset['id']}.*"):
                closure.add(f"{root}/clips/{candidate.name}", "enabled soundscape cue")


def gather(asset_root: Path, repo: Path, traces: list[Path]) -> Closure:
    closure = Closure(asset_root, repo)
    assets_catalog = json.loads((repo / "catalog/runtime_assets.v1.json").read_text(encoding="utf-8"))
    for asset in assets_catalog["assets"]:
        source = asset.get("source")
        if isinstance(source, str):
            closure.add(source, "runtime_assets.v1.json")
    payload = json.loads((repo / "catalog/runtime_payload.v1.json").read_text(encoding="utf-8"))
    for entry in payload["files"]:
        source = entry.get("source")
        if isinstance(source, str) and source.startswith(("assets/", "catalog/", "source_assets/")):
            closure.add(source, "runtime_payload.v1.json")

    manifests: set[str] = set()
    for source in shipping_sources(repo):
        text = source.read_text(encoding="utf-8", errors="replace")
        for literal in LITERAL.findall(text):
            name = f"src/{source.name}"
            if literal.endswith("manifest.v1.json") and literal.startswith("assets/worldgen/"):
                manifests.add(literal)
            elif (asset_root / literal).is_file() or (repo / literal).is_file():
                # Directory literals are browse roots and path prefixes (the editor library, the
                # wreck-collision filter), not loads; what is loaded inside them is named elsewhere.
                closure.add(literal, name)
    for act in ("assets/worldgen/act1", "assets/worldgen/act2", "assets/worldgen/act3"):
        manifests.add(act + "/manifest.v1.json")
    for manifest in sorted(manifests):
        if (asset_root / manifest).is_file() or (repo / manifest).is_file():
            # The manifest and the JSON beside it (edits overlay, scatter layers, features, mission
            # sheets), then whatever those name.
            beside = {p.name for root in (asset_root, repo) for p in (root / manifest).parent.glob("*.json")}
            for name in sorted(beside):
                closure.add(Path(manifest).parent.as_posix() + "/" + name, f"beside {manifest}")
            closure.add(manifest, "shipping source")
    audio_library(closure)
    for directory in DYNAMIC_DIRS:
        closure.add_dir(directory, "names composed at run time")
    # Committed load traces (tools/windows_payload_traces/*.txt, file names only) and any given.
    traces = sorted((repo / "tools" / "windows_payload_traces").glob("*.txt")) + list(traces)
    for trace in traces:
        for line in trace.read_text(encoding="utf-8", errors="replace").splitlines():
            state, _, raw = line.partition("\t")
            rel = logical_trace_path(raw)
            # Directory resolves (the sound library root) are not loads.
            if rel and ((asset_root / rel).is_dir() or (repo / rel).is_dir()):
                continue
            if state == "present" and rel:
                closure.add(rel, f"trace {trace.name}")
    # Walking already-added JSON (catalogs added by directory copies) for what they name.
    for rel in sorted(closure.files):
        if rel.endswith(".json") and rel not in closure.walked:
            closure.walk_json(rel)
    return closure


def logical_trace_path(raw: str) -> str | None:
    """A traced path as `assets/...` or `catalog/...`: relative ones as they are; absolute ones (a
    canonicalized manifest path) from their last `/assets/` or `/catalog/` segment."""
    path = raw.strip().replace("\\", "/")
    if path.startswith(("assets/", "catalog/", "source_assets/")):
        return path
    for marker in ("/source_assets/", "/assets/", "/catalog/"):
        at = path.rfind(marker)
        if at >= 0:
            return path[at + 1:]
    return None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--asset-root", required=True, type=Path, help="checkout holding the git-ignored assets/")
    parser.add_argument("--repo-root", required=True, type=Path, help="checkout whose src/ and catalog/ define the build")
    parser.add_argument("--output", type=Path, help="package root to copy into (assets/, catalog/ at logical paths)")
    parser.add_argument("--trace", type=Path, action="append", default=[], help="KESSEL_ASSET_TRACE file(s) to include")
    parser.add_argument("--dry-run", action="store_true", help="report the closure and size; copy nothing")
    parser.add_argument("--list", action="store_true", help="print every selected file with its reason")
    args = parser.parse_args()

    asset_root = args.asset_root.resolve()
    repo = args.repo_root.resolve()
    if not (asset_root / "assets").is_dir():
        print(f"windows_payload: no assets/ under {asset_root}", file=sys.stderr)
        return 2
    closure = gather(asset_root, repo, args.trace)
    total = sum(closure.source_path(rel).stat().st_size for rel in closure.files)
    if args.list:
        for rel, reason in sorted(closure.files.items()):
            print(f"{rel}\t{reason}")
    by_top: dict[str, int] = {}
    for rel in closure.files:
        top = "/".join(rel.split("/")[:2])
        by_top[top] = by_top.get(top, 0) + closure.source_path(rel).stat().st_size
    print(f"windows_payload: {len(closure.files)} files, {total / 1e6:.1f} MB")
    for top, size in sorted(by_top.items(), key=lambda item: -item[1])[:14]:
        print(f"  {size / 1e6:8.1f} MB  {top}")
    if closure.missing:
        print(f"windows_payload: {len(closure.missing)} named path(s) absent from the asset root (not copied):")
        for rel, reason in sorted(closure.missing.items())[:40]:
            print(f"  absent  {rel}  ({reason})")
    if args.dry_run or not args.output:
        return 0

    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        print(f"windows_payload: refusing to write into non-empty {output}", file=sys.stderr)
        return 2
    records = []
    for rel in sorted(closure.files):
        source = closure.source_path(rel)
        target = output / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
        records.append({"path": rel, "bytes": target.stat().st_size, "sha256": sha256(target), "reason": closure.files[rel]})
    manifest = {
        "schema": "kessel.windows-payload",
        "version": 1,
        "file_count": len(records),
        "total_bytes": sum(record["bytes"] for record in records),
        "policy": "owner-private: purchased kits; copy only to the owner's own machines, never upload",
        "files": records,
    }
    (output / "payload.windows.v1.json").write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    print(f"windows_payload: copied {len(records)} files into {output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
