#!/usr/bin/env bash
# Compatibility entry point for existing development scripts.
set -euo pipefail
SOURCE_PATH=$(readlink -f -- "${BASH_SOURCE[0]}")
exec "$(dirname -- "$SOURCE_PATH")/launch_panzerrechnersystem.sh" "$@"
