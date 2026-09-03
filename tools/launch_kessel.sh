#!/usr/bin/env bash
# Authoritative Linux/RTX 4090 launcher. Builds the checked-out source, then starts the game from
# the repository root so development resources resolve consistently.
set -euo pipefail

SOURCE_PATH=${BASH_SOURCE[0]}
while [[ -L "$SOURCE_PATH" ]]; do
    SOURCE_DIR=$(CDPATH= cd -P -- "$(dirname -- "$SOURCE_PATH")" && pwd)
    LINK_TARGET=$(readlink "$SOURCE_PATH")
    if [[ "$LINK_TARGET" = /* ]]; then
        SOURCE_PATH=$LINK_TARGET
    else
        SOURCE_PATH=$SOURCE_DIR/$LINK_TARGET
    fi
done
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "$SOURCE_PATH")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

export KESSEL_GRAPHICS_PRESET="${KESSEL_GRAPHICS_PRESET:-high}"

if [[ "${KESSEL_SKIP_BUILD:-0}" != "1" ]]; then
    cargo build --release --locked
fi

if [[ ! -x "$REPO_ROOT/target/release/kessel" ]]; then
    echo "missing release binary; run cargo build --release --locked" >&2
    exit 1
fi

exec "$REPO_ROOT/target/release/kessel" "$@"
