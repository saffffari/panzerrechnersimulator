#!/usr/bin/env bash
# Launch the shader Playtest on HAL's landscape HDMI AMOLED without moving other windows.
set -euo pipefail
PANZER_TOOLS_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
command -v qdbus6 >/dev/null
kscreen-doctor -j | python3 -c 'import json,sys; outputs=json.load(sys.stdin)["outputs"]; sys.exit(0 if any(o.get("name")=="HDMI-A-1" and o.get("enabled") and o.get("connected") for o in outputs) else "AMOLED HDMI-A-1 is not connected and enabled")'
PANZER_DISPLAY_SCRIPT=$(mktemp /tmp/panzer-amoled-XXXXXXXX.js)
PANZER_DISPLAY_PLUGIN="panzer-amoled-$$"
cleanup() {
    qdbus6 org.kde.KWin /Scripting unloadScript "$PANZER_DISPLAY_PLUGIN" >/dev/null 2>&1 || true
    rm -f -- "$PANZER_DISPLAY_SCRIPT"
}
trap cleanup EXIT
cat > "$PANZER_DISPLAY_SCRIPT" <<'JS'
const output = workspace.screens.find(screen => screen.name === "HDMI-A-1");
function placePanzer(window) {
    if (!output || window.caption !== "Panzerrechnersystem") return;
    workspace.sendClientToScreen(window, output);
    window.fullScreen = true;
    window.minimized = false;
    workspace.activeWindow = window;
    console.info("PANZER-AMOLED " + JSON.stringify({
        pid: window.pid, id: String(window.internalId), output: window.output.name,
        geometry: window.frameGeometry, fullscreen: window.fullScreen
    }));
}
workspace.windowAdded.connect(placePanzer);
JS
PANZER_DISPLAY_ID=$(qdbus6 org.kde.KWin /Scripting loadScript "$PANZER_DISPLAY_SCRIPT" "$PANZER_DISPLAY_PLUGIN")
qdbus6 org.kde.KWin "/Scripting/Script$PANZER_DISPLAY_ID" org.kde.kwin.Script.run
export KESSEL_FULLSCREEN=1
"$PANZER_TOOLS_DIR/launch_shader_playtest.sh" "$@"
