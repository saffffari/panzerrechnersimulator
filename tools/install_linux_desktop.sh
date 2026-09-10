#!/usr/bin/env bash
# Install the local release in the app menu and on the desktop; rerun if the checkout moves.
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APPLICATION_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/256x256/apps"
DESKTOP_DIR=$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")
DESKTOP_TARGET="$APPLICATION_DIR/panzerrechnersystem.desktop"
install -d "$BIN_DIR" "$APPLICATION_DIR" "$ICON_DIR" "$DESKTOP_DIR"
ln -sfn "$REPO_ROOT/tools/launch_panzerrechnersystem.sh" "$BIN_DIR/panzerrechnersystem"
install -m 0644 "$REPO_ROOT/assets/ui/kessel_icon_tiger.png" "$ICON_DIR/panzerrechnersystem.png"
python3 - "$REPO_ROOT/tools/panzerrechnersystem.desktop" "$DESKTOP_TARGET" "$BIN_DIR/panzerrechnersystem" <<'PY'
from pathlib import Path
import sys
source, target, executable = sys.argv[1:]
def quote_exec(value):
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$') + '"'
lines = []
for line in Path(source).read_text().splitlines():
    if line.startswith('Exec=panzerrechnersystem'):
        line = 'Exec=' + quote_exec(executable) + line[len('Exec=panzerrechnersystem'):]
    elif line.startswith('TryExec='):
        line = 'TryExec=' + executable
    lines.append(line)
Path(target).write_text('\n'.join(lines) + '\n')
PY
chmod 0644 "$DESKTOP_TARGET"
desktop-file-validate "$DESKTOP_TARGET"
install -m 0755 "$DESKTOP_TARGET" "$DESKTOP_DIR/Panzerrechnersystem.desktop"
# Migrate only an old entry installed by this repository, if present.
if [[ -f "$APPLICATION_DIR/kessel.desktop" ]] && rg -q '^Name=der Kessel$' "$APPLICATION_DIR/kessel.desktop"; then
    mv "$APPLICATION_DIR/kessel.desktop" "$APPLICATION_DIR/kessel.desktop.pre-panzerrechnersystem"
fi
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$APPLICATION_DIR" >/dev/null 2>&1 || true; fi
printf 'Installed %s\n' "$DESKTOP_TARGET" "$DESKTOP_DIR/Panzerrechnersystem.desktop"
