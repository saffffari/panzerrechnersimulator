#!/usr/bin/env python3
"""Read-only checks of the branch's explicitly selected Playtest inputs; never creates a device."""
import argparse
import hashlib
import json
import os
from pathlib import Path

BRANCH = Path(__file__).resolve().parents[1]
PLAYTEST = Path("assets/worldgen/revisions/playtest-ground-detail/20260905T151851623908Z_be1c0a5f/manifest.v1.json")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--asset-root", type=Path, default=Path("/home/alex/panzer/tank_simulator"))
    args = parser.parse_args()
    root = args.asset_root.resolve(strict=True)
    errors, warnings, files = [], [], []

    def check(path, *, required=True, digest=False):
        if not path.is_file():
            (errors if required else warnings).append(f"Missing {'required' if required else 'optional'} file: {path}")
            return None
        try:
            with path.open("rb") as stream:
                header = stream.read(64)
            if not header:
                errors.append(f"Empty runtime file: {path}")
                return None
            record = {"path": str(path), "bytes": path.stat().st_size}
            if digest:
                record["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
            files.append(record)
            return header
        except OSError as error:
            errors.append(f"Cannot read {path}: {error}")
            return None

    manifest_path = root / PLAYTEST
    if check(manifest_path, digest=True):
        manifest = json.loads(manifest_path.read_text())
        for logical in manifest["assets"]:
            check(root / logical)
        for key in ("heights", "texture", "ground_class"):
            logical = Path(manifest["terrain"][key])
            check(root / logical if logical.parts[0] == "assets" else manifest_path.parent / logical)
    for name in ("hull", "turret", "gun", "port_turret", "part_gunlever", "part_breechcover"):
        check(root / f"assets/tanks/tiger/baked/tiger_{name}.glb")
    check(root / "assets/props/hand_compass_artie31/hand_compass.glb", digest=True)
    for logical in ("assets/flavor/luger/baked/luger_runtime.glb",
                    "assets/fonts/PressStart2P-Regular.ttf", "assets/fonts/VT323-Regular.ttf"):
        check(root / logical)
    terrain_source = root / "source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps"
    for name in ("grass_7", "dirt_5", "dirt_2", "snow_1"):
        check(terrain_source / f"{name}.png")
    for role in ("tiger", "soviet", "german"):
        path = root / f"assets/ai/weights_foot_{role}.bin"
        header = check(path, required=role != "german", digest=True)
        if header and header[:4] != b"KSL2":
            errors.append(f"Unexpected infantry policy header: {path}")
    if not (root / "assets/ai/weights_foot_german.bin").is_file():
        warnings.append("German infantry uses the same missing-checkpoint doctrine fallback as canonical Playtest.")
    policy_dir = os.environ.get("KESSEL_COMBAT_WEIGHTS_DIR") or os.environ.get("KESSEL_COMBAT_POLICY_DIR")
    if policy_dir:
        folder = Path(policy_dir)
        if not folder.is_absolute():
            errors.append("An explicit combat policy directory must be absolute for this branch launcher.")
        else:
            for name in ("german_rifle.ksl2", "soviet_rifle.ksl2", "tank_soft_targets.json"):
                check(folder / name)
    # This loader previously bypassed KESSEL_ASSET_ROOT and silently changed policy behavior.
    loader = (BRANCH / "src/foot_ai.rs").read_text().split("pub fn load(path: &str", 1)[1].split("Self::from_artifact_bytes", 1)[0]
    if "crate::resource::open(path)?" not in loader:
        errors.append("FootPolicy::load does not use the runtime resource resolver.")
    audio_selection = BRANCH / "catalog/audio_feedback_20260905.json"
    if os.environ.get("KESSEL_ENABLE_PIZZA_AUDIO", "1") != "0":
        selection = json.loads(audio_selection.read_text())
        receipt = root / "assets/sounds/pizza_private/feedback_20260905/BANK-RECEIPT.json"
        if check(receipt) and receipt.read_bytes() != audio_selection.read_bytes():
            errors.append("Gameplay feedback bank receipt differs from the branch's compiled selection.")
        for entry in selection["entries"]:
            path = root / entry["path"]
            if check(path, digest=True):
                if files[-1]["sha256"] != entry["sha256"] or files[-1]["bytes"] != entry["bytes"]:
                    errors.append(f"Gameplay feedback recording hash/size mismatch: {path}")
    print(json.dumps({"schema": "panzer.shader-runtime-preflight.v1", "passed": not errors,
        "branch_root": str(BRANCH), "asset_root": str(root),
        "binary": str(BRANCH / "target/release/panzerrechnersystem"),
        "renderer": "shader", "internal_resolution": [640, 360],
        "checked_files": len(files), "files": files, "warnings": warnings, "errors": errors,
        "scope": "File availability and selected receipt checks only; no renderer, visual, or gameplay acceptance."}, indent=2))
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
