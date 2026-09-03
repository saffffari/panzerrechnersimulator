#!/usr/bin/env bash
# Install a relocatable per-user launcher. Re-run after moving the repository to a new path.
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APPLICATION_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/256x256/apps"
DESKTOP_TARGET="$APPLICATION_DIR/kessel.desktop"

install -d "$BIN_DIR" "$APPLICATION_DIR" "$ICON_DIR"
ln -sfn "$REPO_ROOT/tools/launch_kessel.sh" "$BIN_DIR/kessel"
install -m 0644 "$REPO_ROOT/assets/ui/kessel_icon_tiger.png" "$ICON_DIR/kessel.png"
sed \
    -e "s|^Exec=.*|Exec=$BIN_DIR/kessel|" \
    -e "s|^TryExec=.*|TryExec=$BIN_DIR/kessel|" \
    "$REPO_ROOT/tools/derKessel.desktop" > "$DESKTOP_TARGET"
chmod 0644 "$DESKTOP_TARGET"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPLICATION_DIR" >/dev/null 2>&1 || true
fi

echo "installed $DESKTOP_TARGET"
