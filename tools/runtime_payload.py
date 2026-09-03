#!/usr/bin/env python3
"""Lock, validate, and copy Kessel's non-GLB runtime payload.

The GLB model closure remains owned by ``runtime_asset_cook.py``. This companion tool follows the
three canonical campaign manifests, adds the small set of directly loaded runtime data, and copies
only reviewed files to their existing logical paths below ``Contents/Resources/assets``. It never
scans or copies whole asset directories.
"""

from __future__ import annotations

import argparse
import ast
from dataclasses import dataclass
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import sys
from typing import Any, Iterable


CATALOG_SCHEMA = "kessel.runtime-payload-catalog"
CATALOG_VERSION = 1
RUNTIME_SCHEMA = "kessel.runtime-payload"
RUNTIME_VERSION = 1
DEFAULT_CATALOG = Path("catalog/runtime_payload.v1.json")
MODEL_CATALOG = Path("catalog/runtime_assets.v1.json")
ACT_IDS = (1, 2, 3)

PROJECT_LICENSE = "kessel-project-owned"
UNRESOLVED_LICENSE = "kessel-provenance-unrecorded"
MAPZEN_TERRAIN_LICENSE = "mapzen-terrain-tiles-provenance-unverified"
SENTINEL_LICENSE = "copernicus-sentinel-data-rev1"
COP_DEM_LICENSE = "copernicus-worlddem-30-free-open"
SIL_OFL_LICENSE = "sil-open-font-license-1.1"
CC_BY_4_LICENSE = "cc-by-4.0"

OWNER_DEV_SCOPE = "owner-dev"
PUBLIC_SCOPE = "public"
RELEASE_SCOPES = (OWNER_DEV_SCOPE, PUBLIC_SCOPE)
CARGO_INVENTORY_CANONICALIZATION = "cargo-lock-dependency-inventory-v1"
CARGO_LICENSE_REVIEW_STATUS = "inventory-only-public-blocker"
PIZZA_TERMS_REVIEW_STATUS = "recheck-before-first-public-or-commercial-release"

# These records are deliberately explicit about what is and is not known. Local filenames and
# filesystem dates are not acquisition provenance. Acts 1-3 have exact Sentinel records, and Acts
# 2-3 have exact GLO-30 tile records; the legacy Stalingrad/Garage elevation source remains open.
LICENSES: dict[str, dict[str, Any]] = {
    CC_BY_4_LICENSE: {
        "data_id": None,
        "license_name": "Creative Commons Attribution 4.0 International",
        "license_url": "https://creativecommons.org/licenses/by/4.0/",
        "legal_code_url": "https://creativecommons.org/licenses/by/4.0/legalcode",
        "bundled_credit_path": "THIRD_PARTY_NOTICES/CC-BY-4.0-ASSETS.txt",
    },
    COP_DEM_LICENSE: {
        "data_id": "COP-DEM_GLO-30",
        "license_name": "Copernicus WorldDEM-30 Full, Free & Open licence",
        "license_url": (
            "https://dataspace.copernicus.eu/sites/default/files/media/files/2025-06/"
            "copernicus_contributing_mission_data_access_v2_cop_dem_licenses.pdf"
        ),
        "product_url": (
            "https://dataspace.copernicus.eu/explore-data/data-collections/"
            "copernicus-contributing-missions/collections-description/COP-DEM"
        ),
        "required_modified_data_notice": (
            "produced using Copernicus WorldDEM-30 © DLR e.V. 2010-2014 and © Airbus "
            "Defence and Space GmbH 2014-2018 provided under COPERNICUS by the European "
            "Union and ESA; all rights reserved"
        ),
        "required_liability_notice": (
            "The organisations in charge of the Copernicus programme by law or by delegation "
            "do not incur any liability for any use of the Copernicus WorldDEM-30."
        ),
    },
    PROJECT_LICENSE: {
        "data_id": None,
        "license_name": "Kessel project-authored runtime data",
        "license_url": None,
    },
    MAPZEN_TERRAIN_LICENSE: {
        "data_id": "Mapzen Terrain Tiles / elevation-tiles-prod Terrarium",
        "license_name": (
            "Mapzen Terrain Tiles attribution record; redistribution grant not verified"
        ),
        "license_url": "https://github.com/tilezen/joerd/blob/master/docs/attribution.md",
        "public_redistribution_status": "blocked-provenance-and-license-unverified",
        "registry_url": "https://registry.opendata.aws/terrain-tiles/",
        "required_attribution": [
            "Mapzen",
            (
                "ArcticDEM terrain data DEM(s) were created from DigitalGlobe, Inc., imagery "
                "and funded under National Science Foundation awards 1043681, 1559691, and "
                "1542736;"
            ),
            "Australia terrain data © Commonwealth of Australia (Geoscience Australia) 2017;",
            (
                "Austria terrain data © offene Daten Österreichs – Digitales Geländemodell "
                "(DGM) Österreich;"
            ),
            (
                "Canada terrain data contains information licensed under the Open Government "
                "Licence – Canada;"
            ),
            (
                "Europe terrain data produced using Copernicus data and information funded by "
                "the European Union - EU-DEM layers;"
            ),
            "Global ETOPO1 terrain data U.S. National Oceanic and Atmospheric Administration",
            "Mexico terrain data source: INEGI, Continental relief, 2016;",
            (
                "New Zealand terrain data Copyright 2011 Crown copyright (c) Land Information "
                "New Zealand and the New Zealand Government (All rights reserved);"
            ),
            "Norway terrain data © Kartverket;",
            (
                "United Kingdom terrain data © Environment Agency copyright and/or database "
                "right 2015. All rights reserved;"
            ),
            (
                "United States 3DEP (formerly NED) and global GMTED2010 and SRTM terrain data "
                "courtesy of the U.S. Geological Survey."
            ),
        ],
        "service_url": "https://www.mapzen.com/blog/terrain-tile-service/",
    },
    SENTINEL_LICENSE: {
        "data_id": "Copernicus Sentinel-2 TCI",
        "license_name": "Copernicus Sentinel Data Legal Notice, revision 1",
        "license_url": "https://ads.atmosphere.copernicus.eu/licences/ec-sentinel",
        "required_modified_data_notice_template": (
            "Contains modified Copernicus Sentinel data [Year]"
        ),
    },
    SIL_OFL_LICENSE: {
        "data_id": None,
        "license_name": "SIL Open Font License 1.1",
        "license_url": "https://openfontlicense.org/open-font-license-official-text/",
        "bundled_license_path": "fonts/OFL-1.1.txt",
        "bundled_copyright_path": "fonts/LICENSES.md",
    },
}

DATA_SOURCES: dict[str, dict[str, Any]] = {
    "cop-dem-glo30-act2": {
        "acquisition_period": "2011-2015",
        "data_id": "COP-DEM_GLO-30",
        "derivative": "Act 2 terrain height grid",
        "evidence": "docs/RESEARCH_ACTS.md:39 and docs/research/data/geo.json:7",
        "license": COP_DEM_LICENSE,
        "provider": "Copernicus Data Space Ecosystem / AWS Open Data",
        "source_product_ids": ["Copernicus_DSM_COG_10_N52_00_E014_00_DEM"],
        "source_urls": [
            "https://copernicus-dem-30m.s3.amazonaws.com/"
            "Copernicus_DSM_COG_10_N52_00_E014_00_DEM/"
            "Copernicus_DSM_COG_10_N52_00_E014_00_DEM.tif"
        ],
    },
    "cop-dem-glo30-act3": {
        "acquisition_period": "2011-2015",
        "data_id": "COP-DEM_GLO-30",
        "derivative": "Act 3 terrain height grid",
        "evidence": "docs/RESEARCH_ACTS.md:60 and docs/research/data/geo.json:52",
        "license": COP_DEM_LICENSE,
        "provider": "Copernicus Data Space Ecosystem / AWS Open Data",
        "source_product_ids": ["Copernicus_DSM_COG_10_N52_00_E013_00_DEM"],
        "source_urls": [
            "https://copernicus-dem-30m.s3.amazonaws.com/"
            "Copernicus_DSM_COG_10_N52_00_E013_00_DEM/"
            "Copernicus_DSM_COG_10_N52_00_E013_00_DEM.tif"
        ],
    },
    "mapzen-terrain-tiles-stalingrad": {
        "acquisition_not_applicable": (
            "composite terrain service; original fetch date and exact z12 tile keys were not "
            "retained"
        ),
        "data_id": "Mapzen Terrain Tiles / elevation-tiles-prod Terrarium",
        "derivative": "Garage heightmap and Act 1 terrain height grid",
        "evidence": (
            "provider inference from an untracked legacy metadata file and a recoverable internal "
            "development note; neither is sufficient public-release provenance"
        ),
        "license": MAPZEN_TERRAIN_LICENSE,
        "provider": "Mapzen / Tilezen elevation-tiles-prod service (inferred, unverified)",
        "provenance_status": "unverified-exact-tiles-fetch-date-and-upstream-layer-not-retained",
        "source_product_ids": ["arn:aws:s3:::elevation-tiles-prod/terrarium (z12 tile set)"],
        "source_urls": [
            "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png"
        ],
    },
    "sentinel-2-tci-act1": {
        "acquisition_date": "2024-01-17",
        "acquisition_year": 2024,
        "data_id": "Copernicus Sentinel-2 TCI",
        "derivative": "Act 1 terrain drape",
        "evidence": "docs/RESEARCH_ACTS.md:27 and docs/research/data/stalingrad.json",
        "license": SENTINEL_LICENSE,
        "provider": "Copernicus Sentinel-2 / Element 84 AWS Open Data",
        "source_product_ids": [
            "S2B_38UMU_20240117_0_L2A",
            "S2B_38UMV_20240117_0_L2A",
        ],
        "source_urls": [
            "https://sentinel-cogs.s3.us-west-2.amazonaws.com/sentinel-s2-l2a-cogs/"
            "38/U/MU/2024/1/S2B_38UMU_20240117_0_L2A/TCI.tif",
            "https://sentinel-cogs.s3.us-west-2.amazonaws.com/sentinel-s2-l2a-cogs/"
            "38/U/MV/2024/1/S2B_38UMV_20240117_0_L2A/TCI.tif",
        ],
    },
    "sentinel-2-tci-act2": {
        "acquisition_date": "2025-04-02",
        "acquisition_year": 2025,
        "data_id": "Copernicus Sentinel-2 TCI",
        "derivative": "Act 2 terrain drape and class mask",
        "evidence": "docs/RESEARCH_ACTS.md:41 and docs/research/data/geo.json:8",
        "license": SENTINEL_LICENSE,
        "provider": "Copernicus Sentinel-2 / Element 84 AWS Open Data",
        "source_product_ids": ["S2C_33UVU_20250402_0_L2A"],
        "source_urls": [
            "https://sentinel-cogs.s3.us-west-2.amazonaws.com/sentinel-s2-l2a-cogs/"
            "33/U/VU/2025/4/S2C_33UVU_20250402_0_L2A/TCI.tif"
        ],
    },
    "sentinel-2-tci-act3": {
        "acquisition_date": "2025-04-04",
        "acquisition_year": 2025,
        "data_id": "Copernicus Sentinel-2 TCI",
        "derivative": "Act 3 terrain drape and class mask",
        "evidence": "docs/RESEARCH_ACTS.md:62 and docs/research/data/geo.json:53",
        "license": SENTINEL_LICENSE,
        "provider": "Copernicus Sentinel-2 / Element 84 AWS Open Data",
        "source_product_ids": ["S2A_33UVT_20250404_1_L2A"],
        "source_urls": [
            "https://sentinel-cogs.s3.us-west-2.amazonaws.com/sentinel-s2-l2a-cogs/"
            "33/U/VT/2025/4/S2A_33UVT_20250404_1_L2A/TCI.tif"
        ],
    },
}

FORBIDDEN_SUFFIXES = {
    ".7z",
    ".blend",
    ".blend1",
    ".dae",
    ".fbx",
    ".gltf",
    ".mtl",
    ".obj",
    ".rar",
    ".tif",
    ".tiff",
    ".wav",
    ".zip",
}
FORBIDDEN_PARTS = {
    ".thumbs",
    "archive",
    "archives",
    "generator",
    "previews",
    "raw",
    "source",
    "thumb",
    "thumbs",
}
ALLOWED_SUFFIXES = {".bin", ".json", ".md", ".npy", ".png", ".ttf", ".txt"}

# Authored bake metadata is tracked here because the large ignored Tiger source tree is not a
# reproducible package input. The runtime path stays exactly what game.rs loads.
FIXED_RUNTIME_DATA = (
    (
        "catalog/runtime_data/CC-BY-4.0-ASSET-CREDITS.txt",
        "THIRD_PARTY_NOTICES/CC-BY-4.0-ASSETS.txt",
        "asset-credit-and-license",
        ("package.redistribution-notices",),
        CC_BY_4_LICENSE,
        None,
        "tracked-verbatim",
    ),
    (
        "assets/fonts/VT323-Regular.ttf",
        "fonts/VT323-Regular.ttf",
        "ui-font",
        ("game.ui",),
        SIL_OFL_LICENSE,
        None,
        None,
    ),
    (
        "assets/fonts/PressStart2P-Regular.ttf",
        "fonts/PressStart2P-Regular.ttf",
        "ui-font",
        ("game.ui",),
        SIL_OFL_LICENSE,
        None,
        None,
    ),
    (
        "assets/fonts/LICENSES.md",
        "fonts/LICENSES.md",
        "font-copyright-notice",
        ("package.redistribution-notices",),
        SIL_OFL_LICENSE,
        None,
        "tracked-verbatim",
    ),
    (
        "assets/fonts/OFL-1.1.txt",
        "fonts/OFL-1.1.txt",
        "font-license",
        ("package.redistribution-notices",),
        SIL_OFL_LICENSE,
        None,
        "tracked-verbatim",
    ),
    (
        "Cargo.lock",
        "THIRD_PARTY_NOTICES/RUST-DEPENDENCIES.txt",
        "rust-dependency-inventory",
        ("package.redistribution-notices",),
        PROJECT_LICENSE,
        None,
        CARGO_INVENTORY_CANONICALIZATION,
    ),
    (
        "catalog/runtime_data/PIZZA-DOGGY-CREDITS.txt",
        "THIRD_PARTY_NOTICES/PIZZA-DOGGY.txt",
        "asset-credit-and-policy",
        ("package.redistribution-notices",),
        PROJECT_LICENSE,
        None,
        "tracked-verbatim",
    ),
    (
        "catalog/runtime_data/tiger_ports.json",
        "tanks/tiger/baked/tiger_ports.json",
        "tiger-apertures",
        ("game.tiger-interior",),
        PROJECT_LICENSE,
        None,
        "tracked-canonical-json-terminal-lf",
    ),
    (
        "catalog/runtime_data/tiger_rtt_screens.json",
        "tanks/tiger/baked/tiger_rtt_screens.json",
        "tiger-periscopes",
        ("game.tiger-interior",),
        PROJECT_LICENSE,
        None,
        "tracked-canonical-json-terminal-lf",
    ),
    (
        "catalog/runtime_data/THIRD-PARTY-DATA-NOTICES.txt",
        "THIRD_PARTY_DATA/CREDITS.txt",
        "legal-notices",
        ("package.redistribution-notices",),
        PROJECT_LICENSE,
        None,
        "tracked-verbatim",
    ),
    (
        "assets/terrain/stalingrad_100km_height16.png",
        "terrain/stalingrad_100km_height16.png",
        "terrain-heightmap",
        ("garage.terrain",),
        MAPZEN_TERRAIN_LICENSE,
        "mapzen-terrain-tiles-stalingrad",
        None,
    ),
)

GROUND_CLASS_MASKS = {
    2: "assets/terrain/act2/ground_class.png",
    3: "assets/terrain/act3/ground_class.png",
}

DIRECT_ASSET_LITERAL_RE = re.compile(
    r'"(assets/[^"\n]+?\.(?:bin|json|npy|ogg|png|ttf|txt))"'
)

# Every quoted, non-GLB asset literal currently present in src/*.rs is reviewed here. This makes a
# newly introduced direct resource literal fail catalog validation instead of silently escaping the
# package closure. Dynamic manifest dependencies remain validated by discover_closure below.
REVIEWED_DIRECT_REFERENCES: dict[tuple[str, str], tuple[str, str]] = {
    ("src/act_manifest.rs", "assets/worldgen/act1/manifest.v1.json"): (
        "required-runtime",
        "versioned act manifests are locked by manifest discovery",
    ),
    ("src/act_manifest.rs", "assets/worldgen/act{act_id}/manifest.json"): (
        "excluded-diagnostic",
        "legacy manifest is checked only to produce a migration error",
    ),
    ("src/audio.rs", "assets/sounds/engine_loop.ogg"): (
        "excluded-optional",
        "procedural engine fallback is the packaged baseline; recordings remain unverified",
    ),
    ("src/audio.rs", "assets/sounds/mg_ext_loop.ogg"): (
        "excluded-optional",
        "procedural machine-gun fallback is the packaged baseline; recordings remain unverified",
    ),
    ("src/audio.rs", "assets/sounds/mg_int_loop.ogg"): (
        "excluded-optional",
        "procedural machine-gun fallback is the packaged baseline; recordings remain unverified",
    ),
    ("src/editor.rs", "assets/worldgen/act{n}/manifest.v1.json"): (
        "required-runtime",
        "versioned act manifests are locked by manifest discovery",
    ),
    ("src/game.rs", "assets/tanks/tiger/baked/tiger_ports.json"): (
        "required-runtime",
        "locked canonical Tiger aperture metadata",
    ),
    ("src/game.rs", "assets/tanks/tiger/baked/tiger_rtt_screens.json"): (
        "required-runtime",
        "locked canonical Tiger periscope metadata",
    ),
    ("src/game.rs", "assets/terrain/act2/ground_class.png"): (
        "required-runtime",
        "locked Act 2 terrain class mask",
    ),
    ("src/game.rs", "assets/terrain/act3/ground_class.png"): (
        "required-runtime",
        "locked Act 3 terrain class mask",
    ),
    ("src/game.rs", "assets/terrain/stalingrad_100km_height16.png"): (
        "required-runtime",
        "locked Garage terrain heightmap",
    ),
    ("src/main.rs", "assets/ai/weights_academy.bin"): (
        "excluded-developer-tooling",
        "training/showreel CLI input; campaign has scripted AI fallback",
    ),
    ("src/main.rs", "assets/ai/weights_test.bin"): (
        "excluded-developer-tooling",
        "training/showreel CLI input; campaign has scripted AI fallback",
    ),
    ("src/main.rs", "assets/ai/weights_tiger.bin"): (
        "excluded-developer-tooling",
        "training/showreel CLI input; campaign has scripted AI fallback",
    ),
    ("src/main.rs", "assets/ai/weights_{tag}.bin"): (
        "excluded-developer-tooling",
        "generated training artifact, not campaign runtime data",
    ),
    ("src/resource.rs", "assets/worldgen/act1/heights.npy"): (
        "required-runtime",
        "locked Act 1 height grid and resource resolver test fixture",
    ),
    ("src/showreel.rs", "assets/ai/weights_{tag}.bin"): (
        "excluded-developer-tooling",
        "generated training/showreel artifact, not campaign runtime data",
    ),
    ("src/terrain.rs", "assets/terrain/act2/act2_ground_hires.png"): (
        "required-runtime",
        "locked Act 2 terrain drape",
    ),
    ("src/terrain.rs", "assets/terrain/act3/act3_ground_hires.png"): (
        "required-runtime",
        "locked Act 3 terrain drape",
    ),
    ("src/terrain.rs", "assets/terrain/act{}/heightmap.png"): (
        "excluded-optional-authoring",
        "alternate authored heightmap is absent from the reviewed campaign manifests",
    ),
    ("src/terrain.rs", "assets/worldgen/act{act_id}/heights.npy"): (
        "required-runtime",
        "all three height grids are locked by manifest discovery",
    ),
    ("src/tank_dressing.rs", "assets/tanks/tiger/dressing.v1.json"): (
        "excluded-optional-authoring",
        "local editor-authored Tiger prop state; an absent file intentionally loads as an empty dressing",
    ),
}


class PayloadError(RuntimeError):
    pass


@dataclass(frozen=True)
class Candidate:
    source: str
    runtime_path: str
    kind: str
    required_by: tuple[str, ...]
    license_id: str = PROJECT_LICENSE
    data_source: str | None = None
    canonicalization: str | None = None


def sha256_bytes(blob: bytes) -> str:
    return hashlib.sha256(blob).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def cargo_lock_packages(lock_path: Path) -> list[dict[str, str]]:
    """Parse the locked package identity fields without requiring Python 3.11/tomllib."""
    try:
        lines = lock_path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        raise PayloadError(f"cannot read Cargo.lock dependency inventory: {error}") from error

    packages: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    field_re = re.compile(r'^(name|version|source|checksum) = ("(?:[^"\\]|\\.)*")$')

    def finish() -> None:
        nonlocal current
        if current is None:
            return
        if "name" not in current or "version" not in current:
            raise PayloadError("Cargo.lock package lacks name or version")
        packages.append(current)
        current = None

    for line in lines:
        if line == "[[package]]":
            finish()
            current = {}
            continue
        if current is None:
            continue
        match = field_re.match(line)
        if match and match.group(1) not in current:
            try:
                value = json.loads(match.group(2))
            except json.JSONDecodeError as error:
                raise PayloadError(f"Cargo.lock contains an invalid quoted field: {line}") from error
            if not isinstance(value, str) or not value:
                raise PayloadError(f"Cargo.lock contains an empty package field: {line}")
            current[match.group(1)] = value
    finish()
    if not packages:
        raise PayloadError("Cargo.lock contains no packages")
    return packages


def external_cargo_packages(repo_root: Path) -> list[dict[str, str]]:
    packages = cargo_lock_packages(repo_file(repo_root, "Cargo.lock", "Cargo dependency lock"))
    external = []
    for package in packages:
        source = package.get("source")
        checksum = package.get("checksum")
        if source is None:
            if package.get("name") != "kessel":
                raise PayloadError(
                    f"Cargo.lock has an unclassified local dependency: {package.get('name')}"
                )
            continue
        if not checksum:
            raise PayloadError(
                f"Cargo.lock dependency lacks a checksum: {package['name']} {package['version']}"
            )
        external.append(package)
    external.sort(key=lambda item: (item["name"], item["version"], item["source"]))
    identities = [
        (item["name"], item["version"], item["source"], item["checksum"])
        for item in external
    ]
    if len(identities) != len(set(identities)):
        raise PayloadError("Cargo.lock dependency identity inventory contains duplicates")
    return external


def render_cargo_dependency_inventory(repo_root: Path) -> bytes:
    lock_path = repo_file(repo_root, "Cargo.lock", "Cargo dependency lock")
    packages = external_cargo_packages(repo_root)
    lines = [
        "KESSEL RUST DEPENDENCY INVENTORY",
        "================================",
        "",
        "This file is generated deterministically from Cargo.lock. It identifies the complete",
        "locked dependency universe, including target-specific packages. It is an engineering",
        "inventory, not a completed licence/notice audit. Public redistribution remains blocked",
        "until applicable licence texts, copyright notices, and obligations are reviewed and",
        "bundled for the dependencies actually linked into the release binary.",
        "",
        f"Cargo.lock SHA-256: {sha256_file(lock_path)}",
        f"External locked packages: {len(packages)}",
        f"Licence review status: {CARGO_LICENSE_REVIEW_STATUS}",
        "",
    ]
    for package in packages:
        lines.extend(
            [
                f"{package['name']} {package['version']}",
                f"  Source: {package['source']}",
                f"  Checksum: {package['checksum']}",
                "",
            ]
        )
    return ("\n".join(lines).rstrip() + "\n").encode("utf-8")


def generated_payload_bytes(repo_root: Path, candidate: Candidate) -> bytes | None:
    if candidate.canonicalization == CARGO_INVENTORY_CANONICALIZATION:
        if candidate.source != "Cargo.lock":
            raise PayloadError("Cargo inventory generator must use Cargo.lock")
        return render_cargo_dependency_inventory(repo_root)
    return None


def discover_direct_reference_inventory(repo_root: Path) -> list[dict[str, str]]:
    found: set[tuple[str, str]] = set()
    source_root = repo_root / "src"
    for code_path in sorted(source_root.glob("*.rs")):
        if code_path.is_symlink() or not code_path.is_file():
            raise PayloadError(f"runtime loader source is missing/not regular: {code_path}")
        relative = code_path.relative_to(repo_root).as_posix()
        try:
            text = code_path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            raise PayloadError(f"cannot inspect runtime loader {relative}: {error}") from error
        for literal in DIRECT_ASSET_LITERAL_RE.findall(text):
            found.add((relative, literal))

    reviewed = set(REVIEWED_DIRECT_REFERENCES)
    unreviewed = sorted(found - reviewed)
    stale = sorted(reviewed - found)
    if unreviewed:
        raise PayloadError(
            "unreviewed direct non-GLB resource literals: "
            + ", ".join(f"{source}:{literal}" for source, literal in unreviewed)
        )
    if stale:
        raise PayloadError(
            "stale direct non-GLB resource inventory: "
            + ", ".join(f"{source}:{literal}" for source, literal in stale)
        )
    return [
        {
            "disposition": REVIEWED_DIRECT_REFERENCES[key][0],
            "literal": key[1],
            "reason": REVIEWED_DIRECT_REFERENCES[key][1],
            "source": key[0],
        }
        for key in sorted(reviewed)
    ]


def safe_relative(raw: str, field: str) -> PurePosixPath:
    if not isinstance(raw, str) or not raw:
        raise PayloadError(f"{field} must be a nonempty path string")
    path = PurePosixPath(raw)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        raise PayloadError(f"{field} must be a normalized relative path: {raw!r}")
    return path


def repo_file(repo_root: Path, raw: str, field: str) -> Path:
    relative = safe_relative(raw, field)
    path = repo_root.joinpath(*relative.parts)
    try:
        path.resolve(strict=False).relative_to(repo_root.resolve())
    except ValueError as error:
        raise PayloadError(f"{field} escapes the repository: {raw!r}") from error
    if not path.is_file():
        raise PayloadError(f"required payload file is missing: {raw}")
    if path.is_symlink():
        raise PayloadError(f"payload input may not be a symlink: {raw}")
    return path


def runtime_path_for_asset(source: str) -> str:
    path = safe_relative(source, "source")
    if not path.parts or path.parts[0] != "assets":
        raise PayloadError(f"logical runtime asset must begin with assets/: {source}")
    runtime = PurePosixPath(*path.parts[1:])
    if not runtime.parts:
        raise PayloadError(f"logical runtime asset has no path below assets/: {source}")
    return runtime.as_posix()


def reject_authoring_path(candidate: Candidate) -> None:
    source = safe_relative(candidate.source, "source")
    runtime = safe_relative(candidate.runtime_path, "runtime_path")
    if candidate.canonicalization == CARGO_INVENTORY_CANONICALIZATION:
        if source.as_posix() != "Cargo.lock" or runtime.suffix.casefold() != ".txt":
            raise PayloadError("Cargo dependency inventory mapping is invalid")
        return
    suffix = source.suffix.casefold()
    lower_parts = {part.casefold() for part in source.parts}
    if suffix not in ALLOWED_SUFFIXES or suffix in FORBIDDEN_SUFFIXES:
        raise PayloadError(f"unsupported/authoring payload format: {candidate.source}")
    if lower_parts & FORBIDDEN_PARTS:
        raise PayloadError(f"preview/raw/generator path is forbidden: {candidate.source}")
    if "pizza" in lower_parts:
        raise PayloadError(f"non-GLB payload must never copy Pizza trees: {candidate.source}")
    if any(token in source.name.casefold() for token in ("preview", "thumb")):
        raise PayloadError(f"preview/thumbnail is forbidden: {candidate.source}")
    if runtime.suffix.casefold() != suffix:
        raise PayloadError(
            f"runtime extension differs from source for {candidate.source}: {candidate.runtime_path}"
        )


def load_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PayloadError(f"cannot parse {label} {path}: {error}") from error


def discover_closure(repo_root: Path) -> list[Candidate]:
    by_runtime: dict[str, Candidate] = {}

    discover_direct_reference_inventory(repo_root)

    def add(candidate: Candidate) -> None:
        reject_authoring_path(candidate)
        previous = by_runtime.get(candidate.runtime_path)
        if previous is None:
            by_runtime[candidate.runtime_path] = candidate
            return
        if (
            previous.source != candidate.source
            or previous.kind != candidate.kind
            or previous.license_id != candidate.license_id
            or previous.data_source != candidate.data_source
            or previous.canonicalization != candidate.canonicalization
        ):
            raise PayloadError(
                f"runtime path collision at {candidate.runtime_path}: "
                f"{previous.source} vs {candidate.source}"
            )
        by_runtime[candidate.runtime_path] = Candidate(
            source=previous.source,
            runtime_path=previous.runtime_path,
            kind=previous.kind,
            required_by=tuple(sorted(set(previous.required_by + candidate.required_by))),
            license_id=previous.license_id,
            data_source=previous.data_source,
            canonicalization=previous.canonicalization,
        )

    for act_id in ACT_IDS:
        consumer = f"act{act_id}.campaign"
        manifest_source = f"assets/worldgen/act{act_id}/manifest.v1.json"
        manifest_path = repo_file(repo_root, manifest_source, "act manifest")
        manifest = load_json(manifest_path, "act manifest")
        if (
            not isinstance(manifest, dict)
            or manifest.get("schema") != "kessel.act"
            or manifest.get("version") != 1
            or manifest.get("act_id") != act_id
        ):
            raise PayloadError(f"{manifest_source}: unsupported schema/version/act_id")
        add(
            Candidate(
                manifest_source,
                runtime_path_for_asset(manifest_source),
                "campaign-manifest",
                (consumer,),
            )
        )

        terrain = manifest.get("terrain")
        if not isinstance(terrain, dict):
            raise PayloadError(f"{manifest_source}: terrain must be an object")
        heights_raw = terrain.get("heights")
        heights_rel = safe_relative(heights_raw, f"{manifest_source}.terrain.heights")
        if len(heights_rel.parts) != 1:
            raise PayloadError(f"{manifest_source}: terrain.heights must stay beside the manifest")
        heights_source = (
            PurePosixPath(manifest_source).parent / heights_rel
        ).as_posix()
        add(
            Candidate(
                heights_source,
                runtime_path_for_asset(heights_source),
                "terrain-height-grid",
                (consumer,),
                MAPZEN_TERRAIN_LICENSE if act_id == 1 else COP_DEM_LICENSE,
                (
                    "mapzen-terrain-tiles-stalingrad"
                    if act_id == 1
                    else f"cop-dem-glo30-act{act_id}"
                ),
            )
        )

        texture_source = safe_relative(
            terrain.get("texture"), f"{manifest_source}.terrain.texture"
        ).as_posix()
        add(
            Candidate(
                texture_source,
                runtime_path_for_asset(texture_source),
                "terrain-drape",
                (consumer,),
                SENTINEL_LICENSE,
                f"sentinel-2-tci-act{act_id}",
            )
        )

        assets = manifest.get("assets")
        if not isinstance(assets, list):
            raise PayloadError(f"{manifest_source}: assets must be an array")
        for index, raw in enumerate(assets):
            path = safe_relative(raw, f"{manifest_source}.assets[{index}]")
            if path.suffix.casefold() != ".glb":
                source = path.as_posix()
                add(
                    Candidate(
                        source,
                        runtime_path_for_asset(source),
                        "manifest-runtime-data",
                        (consumer,),
                    )
                )

        edits_source = f"assets/worldgen/act{act_id}/manifest_edits.json"
        edits_path = repo_root / edits_source
        if edits_path.exists():
            repo_file(repo_root, edits_source, "manifest edits")
            edits = load_json(edits_path, "manifest edits")
            if not isinstance(edits, dict) or not isinstance(edits.get("ops"), list):
                raise PayloadError(f"{edits_source}: ops must be an array")
            add(
                Candidate(
                    edits_source,
                    runtime_path_for_asset(edits_source),
                    "campaign-overlay",
                    (consumer,),
                )
            )
            for index, operation in enumerate(edits["ops"]):
                if not isinstance(operation, dict) or operation.get("op") != "add":
                    continue
                raw = operation.get("asset")
                path = safe_relative(raw, f"{edits_source}.ops[{index}].asset")
                if path.suffix.casefold() != ".glb":
                    source = path.as_posix()
                    add(
                        Candidate(
                            source,
                            runtime_path_for_asset(source),
                            "overlay-runtime-data",
                            (consumer,),
                        )
                    )

        if act_id in GROUND_CLASS_MASKS:
            mask_source = GROUND_CLASS_MASKS[act_id]
            add(
                Candidate(
                    mask_source,
                    runtime_path_for_asset(mask_source),
                    "terrain-class-mask",
                    (f"act{act_id}.grass",),
                    SENTINEL_LICENSE,
                    f"sentinel-2-tci-act{act_id}",
                )
            )

    for (
        source,
        runtime_path,
        kind,
        required_by,
        license_id,
        data_source,
        canonicalization,
    ) in FIXED_RUNTIME_DATA:
        add(
            Candidate(
                source,
                runtime_path,
                kind,
                required_by,
                license_id,
                data_source,
                canonicalization,
            )
        )

    candidates = sorted(by_runtime.values(), key=lambda value: value.runtime_path)
    for candidate in candidates:
        repo_file(repo_root, candidate.source, "payload source")
    return candidates


def validate_png(path: Path, label: str) -> None:
    with path.open("rb") as source:
        header = source.read(24)
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        raise PayloadError(f"{label}: invalid PNG header")
    width, height = struct.unpack(">II", header[16:24])
    if width < 1 or height < 1:
        raise PayloadError(f"{label}: PNG dimensions must be positive")


def validate_npy(path: Path, declared_res: int | None, label: str) -> None:
    blob = path.read_bytes()
    if not blob.startswith(b"\x93NUMPY") or len(blob) < 10:
        raise PayloadError(f"{label}: invalid NPY header")
    major = blob[6]
    if major == 1:
        header_size = struct.unpack_from("<H", blob, 8)[0]
        header_start = 10
    elif major in (2, 3):
        if len(blob) < 12:
            raise PayloadError(f"{label}: truncated NPY header")
        header_size = struct.unpack_from("<I", blob, 8)[0]
        header_start = 12
    else:
        raise PayloadError(f"{label}: unsupported NPY version {major}")
    header_end = header_start + header_size
    if header_end > len(blob):
        raise PayloadError(f"{label}: NPY header exceeds file")
    try:
        header = ast.literal_eval(blob[header_start:header_end].decode("latin1").strip())
    except (SyntaxError, ValueError, UnicodeDecodeError) as error:
        raise PayloadError(f"{label}: invalid NPY dictionary") from error
    shape = header.get("shape") if isinstance(header, dict) else None
    if header.get("descr") not in ("<f4", "=f4", "|f4") or not isinstance(shape, tuple):
        raise PayloadError(f"{label}: expected an f32 NPY array")
    if len(shape) != 2 or shape[0] != shape[1] or shape[0] < 2:
        raise PayloadError(f"{label}: expected a square two-dimensional height grid")
    if declared_res is not None and shape != (declared_res, declared_res):
        raise PayloadError(
            f"{label}: NPY shape {shape} differs from manifest {declared_res}x{declared_res}"
        )
    expected_bytes = header_end + shape[0] * shape[1] * 4
    if expected_bytes != len(blob):
        raise PayloadError(
            f"{label}: NPY payload length is {len(blob)}, expected {expected_bytes}"
        )


def finite_vector(value: Any, length: int, label: str) -> None:
    if (
        not isinstance(value, list)
        or len(value) != length
        or any(not isinstance(item, (int, float)) or not math.isfinite(item) for item in value)
    ):
        raise PayloadError(f"{label} must be {length} finite numbers")


def validate_tiger_json(path: Path, kind: str, label: str) -> None:
    value = load_json(path, label)
    if not isinstance(value, list) or not value:
        raise PayloadError(f"{label}: expected a nonempty array")
    names: set[str] = set()
    tiles: set[int] = set()
    for index, entry in enumerate(value):
        item = f"{label}[{index}]"
        if not isinstance(entry, dict):
            raise PayloadError(f"{item}: expected object")
        name = entry.get("name")
        frame = entry.get("frame")
        if not isinstance(name, str) or not name or name in names:
            raise PayloadError(f"{item}.name must be nonempty and unique")
        if frame not in ("hull", "gun", "turret"):
            raise PayloadError(f"{item}.frame is invalid")
        names.add(name)
        finite_vector(entry.get("centroid"), 3, f"{item}.centroid")
        finite_vector(entry.get("bearing"), 3, f"{item}.bearing")
        if kind == "tiger-periscopes":
            tile = entry.get("tile")
            if not isinstance(tile, int) or tile < 0 or tile in tiles:
                raise PayloadError(f"{item}.tile must be a unique nonnegative integer")
            tiles.add(tile)


def validate_font_notices(repo_root: Path) -> None:
    expected_fonts = {
        "VT323-Regular.ttf": {
            "copyright": "Copyright 2011, The VT323 Project Authors (peter.hull@oikoi.com)",
            "sha256": "cf4de751ada78ceac033dbe16a687742939995b77bc2a052ae17a4957958594d",
        },
        "PressStart2P-Regular.ttf": {
            "copyright": "Copyright 2012 The Press Start 2P Project Authors (cody@zone38.net)",
            "sha256": "034c77f1f05ec89421e4a63f0e3a4ca1ecf852cc6d2bf611f126f275728e017d",
        },
    }
    copyrights_path = repo_file(repo_root, "assets/fonts/LICENSES.md", "font copyrights")
    ofl_path = repo_file(repo_root, "assets/fonts/OFL-1.1.txt", "font licence")
    try:
        copyrights = copyrights_path.read_text(encoding="utf-8")
        ofl = ofl_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise PayloadError(f"cannot read bundled font notices: {error}") from error
    for filename, expected in expected_fonts.items():
        font_path = repo_file(repo_root, f"assets/fonts/{filename}", "bundled UI font")
        digest = sha256_file(font_path)
        if digest != expected["sha256"]:
            raise PayloadError(f"bundled UI font hash is unreviewed: {filename} ({digest})")
        for fragment in (filename, expected["copyright"], expected["sha256"], "OFL-1.1.txt"):
            if fragment not in copyrights:
                raise PayloadError(f"font copyright notice lacks {fragment!r}")
    for fragment in (
        "SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007",
        "provided that each copy",
        "contains the above copyright notice and this license",
        'Reserved Font Name',
    ):
        if fragment not in ofl:
            raise PayloadError(f"bundled OFL text lacks {fragment!r}")


def validate_pizza_notice(repo_root: Path) -> None:
    notice_path = repo_file(
        repo_root,
        "catalog/runtime_data/PIZZA-DOGGY-CREDITS.txt",
        "Pizza Doggy credit and policy",
    )
    try:
        notice = notice_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise PayloadError(f"cannot read bundled Pizza Doggy credit: {error}") from error
    required = (
        "Credit: Pizza Doggy",
        "not a substitute for the retained",
        "not a grant to extract or reuse the models",
        "must not distribute the source archives",
        "pizzadoggy-local-purchase",
        "re-reviewed before Kessel's first public or commercial release",
        "https://pizzadoggy.itch.io/psx-mega-pack-2/devlog/961920/updated-the-license-agreement",
        "https://pizzadoggy.itch.io/psx-tech",
    )
    missing = [fragment for fragment in required if fragment not in notice]
    if missing:
        raise PayloadError(f"Pizza Doggy credit/policy notice is incomplete: {missing}")


def validate_cc_by_asset_notice(repo_root: Path) -> None:
    notice_path = repo_file(
        repo_root,
        "catalog/runtime_data/CC-BY-4.0-ASSET-CREDITS.txt",
        "CC BY 4.0 asset credits",
    )
    try:
        notice = notice_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise PayloadError(f"cannot read bundled CC BY 4.0 asset credits: {error}") from error
    required = (
        "Creative Commons Attribution 4.0 International",
        "https://creativecommons.org/licenses/by/4.0/",
        "https://creativecommons.org/licenses/by/4.0/legalcode",
        '"Free Tiger And Crew Kit" by AdrielCZ',
        "free-tiger-and-crew-kit-6290669ab2cb49599f1606507626af39",
        '"Enigma Machine" by Ashish0096',
        "enigma-machine-30b85c037a164174a109ad6f002b9c47",
        '"Skeleton Character PSX" by Vinrax',
        "skeleton-character-psx-ece576bbed4b4364911c7596d828a558",
        "source-native 27-bone skin",
        "not exact anatomical registration",
        "Kessel modifications:",
        "no endorsement by any creator is implied",
    )
    missing = [fragment for fragment in required if fragment not in notice]
    if missing:
        raise PayloadError(f"CC BY 4.0 asset credit notice is incomplete: {missing}")


def validate_semantics(repo_root: Path, candidates: Iterable[Candidate]) -> None:
    candidates = list(candidates)
    validate_font_notices(repo_root)
    validate_pizza_notice(repo_root)
    validate_cc_by_asset_notice(repo_root)
    render_cargo_dependency_inventory(repo_root)
    referenced_data_sources: set[str] = set()
    for candidate in candidates:
        if candidate.license_id not in LICENSES:
            raise PayloadError(
                f"{candidate.source}: unknown payload license ID {candidate.license_id!r}"
            )
        if candidate.data_source is not None:
            provenance = DATA_SOURCES.get(candidate.data_source)
            if provenance is None:
                raise PayloadError(
                    f"{candidate.source}: unknown data source ID {candidate.data_source!r}"
                )
            if provenance.get("license") != candidate.license_id:
                raise PayloadError(
                    f"{candidate.source}: data source/license mismatch for {candidate.data_source}"
                )
            referenced_data_sources.add(candidate.data_source)
        elif candidate.license_id in {
            SENTINEL_LICENSE,
            COP_DEM_LICENSE,
            MAPZEN_TERRAIN_LICENSE,
        }:
            raise PayloadError(f"{candidate.source}: third-party terrain lacks a data source ID")

    unused_data_sources = sorted(set(DATA_SOURCES) - referenced_data_sources)
    if unused_data_sources:
        raise PayloadError(f"unreferenced terrain data-source records: {unused_data_sources}")

    manifest_res: dict[int, int] = {}
    for act_id in ACT_IDS:
        source = repo_root / f"assets/worldgen/act{act_id}/manifest.v1.json"
        manifest = load_json(source, "act manifest")
        res = manifest.get("terrain", {}).get("res")
        spacing = manifest.get("terrain", {}).get("spacing")
        if not isinstance(res, int) or res < 2:
            raise PayloadError(f"{source}: terrain.res must be an integer >= 2")
        if not isinstance(spacing, (int, float)) or not math.isfinite(spacing) or spacing <= 0:
            raise PayloadError(f"{source}: terrain.spacing must be positive and finite")
        manifest_res[act_id] = res

    for candidate in candidates:
        path = repo_root / candidate.source
        if candidate.kind == "terrain-height-grid":
            act_id = int(PurePosixPath(candidate.source).parts[2].removeprefix("act"))
            validate_npy(path, manifest_res[act_id], candidate.source)
        elif candidate.kind in {"terrain-drape", "terrain-class-mask", "terrain-heightmap"}:
            validate_png(path, candidate.source)
        elif candidate.kind in {"tiger-apertures", "tiger-periscopes"}:
            validate_tiger_json(path, candidate.kind, candidate.source)
        elif path.suffix.casefold() == ".json":
            load_json(path, "runtime JSON")

    notices_path = repo_root / "catalog/runtime_data/THIRD-PARTY-DATA-NOTICES.txt"
    try:
        notices = notices_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise PayloadError(f"cannot read bundled third-party data notices: {error}") from error
    required_notice_fragments = (
        LICENSES[SENTINEL_LICENSE]["license_url"],
        LICENSES[SENTINEL_LICENSE]["required_modified_data_notice_template"],
        LICENSES[COP_DEM_LICENSE]["license_url"],
        LICENSES[COP_DEM_LICENSE]["product_url"],
        LICENSES[COP_DEM_LICENSE]["required_modified_data_notice"],
        LICENSES[COP_DEM_LICENSE]["required_liability_notice"],
        LICENSES[MAPZEN_TERRAIN_LICENSE]["license_url"],
        LICENSES[MAPZEN_TERRAIN_LICENSE]["registry_url"],
        LICENSES[MAPZEN_TERRAIN_LICENSE]["service_url"],
    ) + tuple(LICENSES[MAPZEN_TERRAIN_LICENSE]["required_attribution"])
    missing_notice_fragments = [
        fragment for fragment in required_notice_fragments if fragment not in notices
    ]
    if missing_notice_fragments:
        raise PayloadError(
            "bundled third-party data notice is incomplete: "
            + ", ".join(repr(item) for item in missing_notice_fragments)
        )


def build_catalog(repo_root: Path) -> dict[str, Any]:
    candidates = discover_closure(repo_root)
    validate_semantics(repo_root, candidates)
    cargo_packages = external_cargo_packages(repo_root)
    direct_inventory = discover_direct_reference_inventory(repo_root)
    files = []
    for candidate in candidates:
        path = repo_root / candidate.source
        generated = generated_payload_bytes(repo_root, candidate)
        if generated is None:
            expected_bytes = path.stat().st_size
            expected_sha256 = sha256_file(path)
        else:
            expected_bytes = len(generated)
            expected_sha256 = sha256_bytes(generated)
        entry = {
            "expected": {
                "bytes": expected_bytes,
                "sha256": expected_sha256,
            },
            "kind": candidate.kind,
            "license": candidate.license_id,
            "required_by": list(candidate.required_by),
            "runtime_path": candidate.runtime_path,
            "source": candidate.source,
        }
        if candidate.canonicalization is not None:
            entry["canonicalization"] = candidate.canonicalization
            if generated is not None:
                entry["generator_input"] = {
                    "bytes": path.stat().st_size,
                    "sha256": sha256_file(path),
                }
        if candidate.data_source is not None:
            entry["data_source"] = candidate.data_source
        files.append(entry)
    return {
        "catalog_id": "kessel.macos-non-glb-runtime",
        "data_sources": DATA_SOURCES,
        "direct_resource_inventory": {
            "policy": (
                "every quoted non-GLB assets/ literal in src/*.rs must be classified; required "
                "manifest dependencies are additionally resolved and locked"
            ),
            "references": direct_inventory,
            "scanner": "quoted-assets-literals-v1",
        },
        "excludes": [
            "Pizza archives and extracted non-GLB authoring trees",
            "raw/alternate 3D formats and Blender/generator intermediates",
            "previews, thumbnails, screenshots, and user images",
            "optional unverified/non-commercial recordings",
            "legacy manifests, terrain source rasters, and AI training checkpoints",
        ],
        "file_count": len(files),
        "files": files,
        "licenses": LICENSES,
        "release_policy": {
            "owner_dev_allowed_with_blockers": True,
            "public_release_requires": [
                (
                    "documented date/period or not-applicable rationale, source IDs, provider, "
                    "licence, and required notices for every terrain source"
                ),
                "resolved redistribution provenance for every non-Pizza runtime model",
                "verified Mapzen source provenance and applicable redistribution terms",
                "complete applicable Rust dependency licence texts and copyright notices",
                "re-review of current Pizza Doggy terms and retained purchase evidence",
            ],
        },
        "schema": CATALOG_SCHEMA,
        "software_dependencies": {
            "cargo_lock_sha256": sha256_file(repo_root / "Cargo.lock"),
            "external_locked_package_count": len(cargo_packages),
            "license_review_status": CARGO_LICENSE_REVIEW_STATUS,
            "notice_runtime_path": "THIRD_PARTY_NOTICES/RUST-DEPENDENCIES.txt",
        },
        "third_party_asset_policy": {
            "cc_by_credit_runtime_path": "THIRD_PARTY_NOTICES/CC-BY-4.0-ASSETS.txt",
            "pizza_credit_runtime_path": "THIRD_PARTY_NOTICES/PIZZA-DOGGY.txt",
            "pizza_terms_review_status": PIZZA_TERMS_REVIEW_STATUS,
        },
        "total_bytes": sum(entry["expected"]["bytes"] for entry in files),
        "version": CATALOG_VERSION,
    }


def load_catalog(path: Path) -> dict[str, Any]:
    value = load_json(path, "runtime payload catalog")
    if not isinstance(value, dict):
        raise PayloadError("runtime payload catalog must be an object")
    return value


def validate_catalog(repo_root: Path, catalog_path: Path) -> dict[str, Any]:
    actual = load_catalog(catalog_path)
    if actual.get("schema") != CATALOG_SCHEMA or actual.get("version") != CATALOG_VERSION:
        raise PayloadError(
            f"catalog must declare schema={CATALOG_SCHEMA!r}, version={CATALOG_VERSION}"
        )
    expected = build_catalog(repo_root)
    try:
        actual_bytes = catalog_path.read_bytes()
    except OSError as error:
        raise PayloadError(f"cannot read runtime payload catalog bytes: {error}") from error
    if actual_bytes != canonical_json(actual):
        raise PayloadError("runtime payload catalog JSON is not canonical")
    if actual != expected:
        expected_files = {entry["runtime_path"]: entry for entry in expected["files"]}
        actual_files = {
            entry.get("runtime_path"): entry
            for entry in actual.get("files", [])
            if isinstance(entry, dict)
        }
        missing = sorted(set(expected_files) - set(actual_files))
        extra = sorted(set(actual_files) - set(expected_files))
        drifted = sorted(
            path
            for path in set(expected_files) & set(actual_files)
            if expected_files[path] != actual_files[path]
        )
        detail = []
        if missing:
            detail.append(f"missing={missing}")
        if extra:
            detail.append(f"extra={extra}")
        if drifted:
            detail.append(f"drifted={drifted}")
        if not detail:
            detail.append("catalog metadata/counts differ")
        raise PayloadError(
            "runtime payload catalog is stale (" + ", ".join(detail) + "); review and relock"
        )
    return actual


def redistribution_blockers(repo_root: Path, payload_catalog: dict[str, Any]) -> list[dict[str, Any]]:
    blockers = []
    unreviewed_payload = [
        entry["source"]
        for entry in payload_catalog["files"]
        if entry.get("license") == UNRESOLVED_LICENSE
    ]
    if unreviewed_payload:
        blockers.append(
            {
                "code": "unresolved-non-glb-provenance",
                "count": len(unreviewed_payload),
                "examples": unreviewed_payload[:8],
            }
        )

    referenced_data_sources = sorted(
        {
            entry["data_source"]
            for entry in payload_catalog["files"]
            if isinstance(entry.get("data_source"), str)
        }
    )
    incomplete_terrain_sources = []
    data_sources = payload_catalog.get("data_sources")
    if not isinstance(data_sources, dict):
        raise PayloadError("runtime payload catalog lacks data_sources metadata")
    for data_source_id in referenced_data_sources:
        record = data_sources.get(data_source_id)
        if not isinstance(record, dict):
            raise PayloadError(f"catalog data source is missing/invalid: {data_source_id}")
        missing = []
        acquisition_year = record.get("acquisition_year")
        acquisition_date = record.get("acquisition_date")
        acquisition_period = record.get("acquisition_period")
        acquisition_not_applicable = record.get("acquisition_not_applicable")
        has_acquisition = (
            isinstance(acquisition_year, int)
            and 1900 <= acquisition_year <= 2100
            or isinstance(acquisition_date, str)
            and bool(acquisition_date)
            or isinstance(acquisition_period, str)
            and bool(acquisition_period)
            or isinstance(acquisition_not_applicable, str)
            and bool(acquisition_not_applicable)
        )
        if not has_acquisition:
            missing.append("acquisition_date_or_period")
        source_product_ids = record.get("source_product_ids")
        if (
            not isinstance(source_product_ids, list)
            or not source_product_ids
            or any(not isinstance(item, str) or not item for item in source_product_ids)
        ):
            missing.append("source_product_ids")
        if not isinstance(record.get("data_id"), str) or not record["data_id"]:
            missing.append("data_id")
        if not isinstance(record.get("provider"), str) or not record["provider"]:
            missing.append("provider")
        license_id = record.get("license")
        if not isinstance(license_id, str) or license_id not in payload_catalog.get("licenses", {}):
            missing.append("reviewed_license_record")
        source_urls = record.get("source_urls")
        if (
            not isinstance(source_urls, list)
            or not source_urls
            or any(not isinstance(item, str) or not item.startswith("https://") for item in source_urls)
        ):
            missing.append("source_urls")
        if missing:
            incomplete_terrain_sources.append({"data_source": data_source_id, "missing": missing})
    if incomplete_terrain_sources:
        blockers.append(
            {
                "code": "incomplete-terrain-source-metadata",
                "count": len(incomplete_terrain_sources),
                "examples": incomplete_terrain_sources[:8],
            }
        )

    unverified_mapzen = [
        data_source_id
        for data_source_id in referenced_data_sources
        if data_sources[data_source_id].get("license") == MAPZEN_TERRAIN_LICENSE
    ]
    if unverified_mapzen:
        blockers.append(
            {
                "code": "mapzen-terrain-provenance-and-license-unverified",
                "count": len(unverified_mapzen),
                "examples": unverified_mapzen[:8],
            }
        )

    software_dependencies = payload_catalog.get("software_dependencies")
    if not isinstance(software_dependencies, dict):
        raise PayloadError("runtime payload catalog lacks software_dependencies metadata")
    cargo_count = software_dependencies.get("external_locked_package_count")
    if not isinstance(cargo_count, int) or cargo_count < 1:
        raise PayloadError("runtime payload catalog has an invalid Cargo dependency count")
    if software_dependencies.get("license_review_status") == CARGO_LICENSE_REVIEW_STATUS:
        blockers.append(
            {
                "code": "rust-dependency-license-notices-unreviewed",
                "count": cargo_count,
                "examples": [software_dependencies.get("notice_runtime_path")],
            }
        )
    elif software_dependencies.get("license_review_status") != "reviewed-complete":
        raise PayloadError("runtime payload catalog has an unknown Cargo licence-review status")

    model_path = repo_root / MODEL_CATALOG
    model_catalog = load_json(model_path, "runtime model catalog")
    model_assets = model_catalog.get("assets") if isinstance(model_catalog, dict) else None
    if not isinstance(model_assets, list):
        raise PayloadError(f"{MODEL_CATALOG}: assets must be an array")
    pizza_models = sorted(
        entry.get("source")
        for entry in model_assets
        if isinstance(entry, dict)
        and isinstance(entry.get("source"), str)
        and entry["source"].startswith("assets/pizza/")
        and entry.get("license") == "pizzadoggy-local-purchase"
    )
    asset_policy = payload_catalog.get("third_party_asset_policy")
    if not isinstance(asset_policy, dict):
        raise PayloadError("runtime payload catalog lacks third_party_asset_policy metadata")
    cc_by_models = [
        entry
        for entry in model_assets
        if isinstance(entry, dict) and entry.get("license") == CC_BY_4_LICENSE
    ]
    if cc_by_models:
        expected_runtime_credit = "THIRD_PARTY_NOTICES/CC-BY-4.0-ASSETS.txt"
        if asset_policy.get("cc_by_credit_runtime_path") != expected_runtime_credit:
            raise PayloadError("runtime payload catalog lacks the CC BY model-credit mapping")
        matching_notices = [
            entry
            for entry in payload_catalog.get("files", [])
            if isinstance(entry, dict)
            and entry.get("runtime_path") == expected_runtime_credit
            and entry.get("license") == CC_BY_4_LICENSE
        ]
        if len(matching_notices) != 1:
            raise PayloadError("runtime payload catalog must bundle exactly one CC BY credit file")
        model_license = model_catalog.get("licenses", {}).get(CC_BY_4_LICENSE)
        if (
            not isinstance(model_license, dict)
            or model_license.get("license_url")
            != "https://creativecommons.org/licenses/by/4.0/"
            or model_license.get("bundled_credit_reference")
            != "catalog/runtime_data/CC-BY-4.0-ASSET-CREDITS.txt"
        ):
            raise PayloadError("runtime model catalog has an incomplete CC BY 4.0 license record")
    if pizza_models and asset_policy.get("pizza_terms_review_status") == PIZZA_TERMS_REVIEW_STATUS:
        blockers.append(
            {
                "code": "pizza-vendor-terms-recheck-required",
                "count": 1,
                "examples": [
                    {
                        "integrated_model_count": len(pizza_models),
                        "notice": asset_policy.get("pizza_credit_runtime_path"),
                    }
                ],
            }
        )
    elif pizza_models and asset_policy.get("pizza_terms_review_status") != "reviewed-current":
        raise PayloadError("runtime payload catalog has an unknown Pizza terms-review status")
    unreviewed_models = sorted(
        entry.get("source")
        for entry in model_assets
        if isinstance(entry, dict)
        and isinstance(entry.get("source"), str)
        and not entry["source"].startswith("assets/pizza/")
        and entry.get("license") == UNRESOLVED_LICENSE
    )
    if unreviewed_models:
        blockers.append(
            {
                "code": "unresolved-non-pizza-model-provenance",
                "count": len(unreviewed_models),
                "examples": unreviewed_models[:8],
            }
        )
    return blockers


def validate_release_mode(blockers: list[dict[str, Any]], release_scope: str) -> None:
    if release_scope not in RELEASE_SCOPES:
        raise PayloadError(f"unsupported release scope: {release_scope!r}")
    if not blockers:
        return
    summary = ", ".join(f"{item['code']}={item['count']}" for item in blockers)
    if release_scope == PUBLIC_SCOPE:
        raise PayloadError(
            "PUBLIC RELEASE BLOCKED: unresolved release requirements: " + summary
        )
    print(
        "runtime_payload: WARNING: owner/developer-scope package only; public redistribution is "
        f"blocked ({summary})",
        file=sys.stderr,
    )


def validate(
    repo_root: Path, catalog_path: Path, release_scope: str
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    catalog = validate_catalog(repo_root, catalog_path)
    blockers = redistribution_blockers(repo_root, catalog)
    validate_release_mode(blockers, release_scope)
    print(
        f"runtime payload: {catalog['file_count']} files, {catalog['total_bytes']} bytes, "
        f"catalog_sha256={sha256_file(catalog_path)}"
    )
    return catalog, blockers


def destination(root: Path, runtime_path: str) -> Path:
    relative = safe_relative(runtime_path, "runtime_path")
    result = root
    for part in relative.parts:
        result = result / part
        if result.is_symlink():
            raise PayloadError(f"runtime destination may not traverse a symlink: {runtime_path}")
    try:
        result.resolve(strict=False).relative_to(root.resolve())
    except ValueError as error:
        raise PayloadError(f"runtime path escapes output: {runtime_path}") from error
    return result


def runtime_entry_from_catalog(entry: dict[str, Any]) -> dict[str, Any]:
    expected = entry.get("expected")
    if not isinstance(expected, dict):
        raise PayloadError("payload catalog file entry lacks expected bytes/hash")
    runtime_entry = {
        "bytes": expected.get("bytes"),
        "kind": entry.get("kind"),
        "license": entry.get("license"),
        "path": entry.get("runtime_path"),
        "required_by": entry.get("required_by"),
        "sha256": expected.get("sha256"),
        "source": entry.get("source"),
    }
    if "canonicalization" in entry:
        runtime_entry["canonicalization"] = entry["canonicalization"]
    if "data_source" in entry:
        runtime_entry["data_source"] = entry["data_source"]
    return runtime_entry


def cook(
    repo_root: Path,
    catalog_path: Path,
    output: Path,
    release_scope: str,
) -> None:
    if output.is_symlink():
        raise PayloadError(f"output may not be a symlink: {output}")
    catalog, blockers = validate(repo_root, catalog_path, release_scope)
    output.mkdir(parents=True, exist_ok=True)
    if output.is_symlink() or not output.is_dir():
        raise PayloadError(f"output must be a real directory: {output}")

    runtime_entries = []
    hash_entries: list[tuple[str, str]] = []
    for entry in catalog["files"]:
        target = destination(output, entry["runtime_path"])
        if target.exists() or target.is_symlink():
            raise PayloadError(f"refusing to overwrite payload destination: {target}")
        target.parent.mkdir(parents=True, exist_ok=True)
        if entry.get("canonicalization") == CARGO_INVENTORY_CANONICALIZATION:
            target.write_bytes(render_cargo_dependency_inventory(repo_root))
        else:
            shutil.copyfile(repo_root / entry["source"], target)
        os.chmod(target, 0o644)
        runtime_entry = runtime_entry_from_catalog(entry)
        runtime_entries.append(runtime_entry)
        hash_entries.append((runtime_entry["path"], runtime_entry["sha256"]))

    bundled_catalog_path = destination(output, "catalog/runtime_payload.v1.json")
    if bundled_catalog_path.exists() or bundled_catalog_path.is_symlink():
        raise PayloadError(f"refusing to overwrite bundled catalog: {bundled_catalog_path}")
    bundled_catalog_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(catalog_path, bundled_catalog_path)
    os.chmod(bundled_catalog_path, 0o644)
    catalog_hash = sha256_file(bundled_catalog_path)
    hash_entries.append(("catalog/runtime_payload.v1.json", catalog_hash))

    manifest = {
        "blockers": blockers,
        "catalog_sha256": catalog_hash,
        "data_sources": catalog["data_sources"],
        "direct_resource_inventory": catalog["direct_resource_inventory"],
        "file_count": len(runtime_entries),
        "files": runtime_entries,
        "licenses": catalog["licenses"],
        "public_release_ready": not blockers,
        "release_scope": release_scope,
        "schema": RUNTIME_SCHEMA,
        "software_dependencies": catalog["software_dependencies"],
        "third_party_asset_policy": catalog["third_party_asset_policy"],
        "total_bytes": sum(entry["bytes"] for entry in runtime_entries),
        "version": RUNTIME_VERSION,
    }
    manifest_path = destination(output, "runtime-payload.manifest.json")
    if manifest_path.exists() or manifest_path.is_symlink():
        raise PayloadError(f"refusing to overwrite runtime manifest: {manifest_path}")
    manifest_path.write_bytes(canonical_json(manifest))
    os.chmod(manifest_path, 0o644)

    hash_path = destination(output, "RUNTIME-PAYLOAD.sha256")
    if hash_path.exists() or hash_path.is_symlink():
        raise PayloadError(f"refusing to overwrite payload hash list: {hash_path}")
    hash_path.write_text(
        "".join(f"{digest}  {path}\n" for path, digest in sorted(hash_entries)),
        encoding="utf-8",
    )
    os.chmod(hash_path, 0o644)
    print(
        f"runtime payload cooked: {len(runtime_entries)} files, "
        f"{manifest['total_bytes']} bytes -> {output}"
    )


def verify_runtime(output: Path) -> None:
    manifest_path = destination(output, "runtime-payload.manifest.json")
    catalog_path = destination(output, "catalog/runtime_payload.v1.json")
    hash_path = destination(output, "RUNTIME-PAYLOAD.sha256")
    manifest = load_json(manifest_path, "cooked runtime payload manifest")
    catalog = load_json(catalog_path, "bundled runtime payload catalog")
    if manifest_path.read_bytes() != canonical_json(manifest):
        raise PayloadError("cooked runtime payload manifest JSON is not canonical")
    if catalog_path.read_bytes() != canonical_json(catalog):
        raise PayloadError("bundled runtime payload catalog JSON is not canonical")
    if (
        not isinstance(manifest, dict)
        or manifest.get("schema") != RUNTIME_SCHEMA
        or manifest.get("version") != RUNTIME_VERSION
    ):
        raise PayloadError("invalid cooked runtime payload schema/version")
    if (
        not isinstance(catalog, dict)
        or catalog.get("schema") != CATALOG_SCHEMA
        or catalog.get("version") != CATALOG_VERSION
    ):
        raise PayloadError("invalid bundled payload catalog schema/version")
    if sha256_file(catalog_path) != manifest.get("catalog_sha256"):
        raise PayloadError("bundled payload catalog hash differs from runtime manifest")
    if manifest.get("licenses") != catalog.get("licenses"):
        raise PayloadError("runtime manifest license records differ from bundled catalog")
    if manifest.get("data_sources") != catalog.get("data_sources"):
        raise PayloadError("runtime manifest data-source records differ from bundled catalog")
    if manifest.get("direct_resource_inventory") != catalog.get("direct_resource_inventory"):
        raise PayloadError("runtime manifest direct-resource inventory differs from bundled catalog")
    if manifest.get("software_dependencies") != catalog.get("software_dependencies"):
        raise PayloadError("runtime manifest software dependency data differs from bundled catalog")
    if manifest.get("third_party_asset_policy") != catalog.get("third_party_asset_policy"):
        raise PayloadError("runtime manifest asset policy differs from bundled catalog")
    files = manifest.get("files")
    catalog_files = catalog.get("files")
    if not isinstance(catalog_files, list):
        raise PayloadError("bundled payload catalog files must be an array")
    expected_runtime_files = [runtime_entry_from_catalog(entry) for entry in catalog_files]
    if (
        not isinstance(files, list)
        or manifest.get("file_count") != len(files)
        or not files
        or files != sorted(files, key=lambda entry: entry.get("path", ""))
    ):
        raise PayloadError("cooked runtime payload file list is empty, unsorted, or inconsistent")
    if files != expected_runtime_files:
        raise PayloadError("runtime manifest files differ from the locked bundled catalog")
    blockers = manifest.get("blockers")
    if not isinstance(blockers, list) or manifest.get("public_release_ready") is not (not blockers):
        raise PayloadError("runtime manifest release-blocker state is inconsistent")
    release_scope = manifest.get("release_scope")
    if release_scope not in RELEASE_SCOPES:
        raise PayloadError("runtime manifest release scope is invalid")
    if release_scope == PUBLIC_SCOPE and blockers:
        raise PayloadError("public runtime manifest contains unresolved release blockers")
    total = 0
    expected_hash_entries: list[tuple[str, str]] = []
    seen = set()
    for index, entry in enumerate(files):
        runtime_path = entry.get("path")
        if not isinstance(runtime_path, str) or runtime_path in seen:
            raise PayloadError(f"runtime files[{index}].path is invalid or duplicated")
        seen.add(runtime_path)
        path = destination(output, runtime_path)
        if not path.is_file() or path.is_symlink():
            raise PayloadError(f"cooked payload file is missing/not regular: {runtime_path}")
        size = path.stat().st_size
        digest = sha256_file(path)
        if size != entry.get("bytes") or digest != entry.get("sha256"):
            raise PayloadError(f"cooked payload hash/size mismatch: {runtime_path}")
        total += size
        expected_hash_entries.append((runtime_path, digest))
    if total != manifest.get("total_bytes"):
        raise PayloadError("cooked runtime payload byte total is inconsistent")
    expected_hash_entries.append(
        ("catalog/runtime_payload.v1.json", sha256_file(catalog_path))
    )
    expected_hash_text = "".join(
        f"{digest}  {path}\n" for path, digest in sorted(expected_hash_entries)
    )
    try:
        actual_hash_text = hash_path.read_text(encoding="utf-8")
    except OSError as error:
        raise PayloadError(f"cannot read {hash_path}: {error}") from error
    if actual_hash_text != expected_hash_text:
        raise PayloadError("RUNTIME-PAYLOAD.sha256 is missing, drifted, or noncanonical")
    print(f"runtime payload verified: {len(files)} files, {total} bytes")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--catalog", type=Path, default=DEFAULT_CATALOG)
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate_parser = subparsers.add_parser("validate", help="validate closure and locked hashes")
    validate_parser.add_argument(
        "--release-scope", choices=RELEASE_SCOPES, default=OWNER_DEV_SCOPE
    )

    lock_parser = subparsers.add_parser("lock", help="write a reviewed catalog candidate")
    lock_parser.add_argument("--output", type=Path, required=True)

    cook_parser = subparsers.add_parser("cook", help="copy the locked payload")
    cook_parser.add_argument("--output", type=Path, required=True)
    cook_parser.add_argument(
        "--release-scope", choices=RELEASE_SCOPES, default=OWNER_DEV_SCOPE
    )

    verify_parser = subparsers.add_parser("verify", help="verify an existing cooked payload")
    verify_parser.add_argument("--assets-root", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    repo_root = args.repo_root.resolve()
    catalog_path = args.catalog
    if not catalog_path.is_absolute():
        catalog_path = repo_root / catalog_path
    try:
        if args.command == "lock":
            output = args.output
            if not output.is_absolute():
                output = repo_root / output
            if output.exists() or output.is_symlink():
                raise PayloadError(f"refusing to overwrite catalog: {output}")
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(canonical_json(build_catalog(repo_root)))
            print(f"wrote runtime payload catalog candidate: {output}")
        elif args.command == "validate":
            validate(repo_root, catalog_path, args.release_scope)
        elif args.command == "cook":
            output = args.output
            if not output.is_absolute():
                output = Path.cwd() / output
            cook(repo_root, catalog_path, output, args.release_scope)
        elif args.command == "verify":
            verify_runtime(args.assets_root.resolve())
        else:
            raise PayloadError(f"unsupported command: {args.command}")
    except PayloadError as error:
        print(f"runtime_payload: ERROR: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
