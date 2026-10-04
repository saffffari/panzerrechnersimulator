#!/usr/bin/env bash
# Validated hotfix, one fixed 12900K / RTX4090 profile. No build or asset generation.
set -euo pipefail
case "${1:---play}" in
  --play) PANZER_MODE=play ;;
  --menu) PANZER_MODE=menu ;;
  --check) PANZER_MODE=check ;;
  *) printf 'Usage: %s [--play|--menu|--check]\n' "$0" >&2; exit 2 ;;
esac
readonly PANZER_ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
readonly PANZER_BINARY="$PANZER_ROOT/target/release/panzerrechnersystem"
readonly PANZER_MANIFEST="$PANZER_ROOT/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json"
readonly PANZER_RUNTIME="$PANZER_ROOT/artifacts/runtime"
cd "$PANZER_ROOT"
python3 - "$PANZER_ROOT" <<'PY'
import json, re, hashlib, sys
from pathlib import Path
root=Path(sys.argv[1]); binary=root/'target/release/panzerrechnersystem'
if binary.is_symlink() or binary.read_bytes()[:4]!=b'\x7fELF':
    raise SystemExit('Expected a local native release executable')
manifest=root/'assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json'
j=json.loads(manifest.read_text())
assert j['act_id']==1 and j['terrain']['grass_season']=='winter'
required=[manifest.parent/j['terrain']['heights']]
required.extend(root/p for p in j['assets'])
required.extend(root/j['terrain'][k] for k in ('texture','rock_map','snow_map','ground_class'))
required.extend(root/f'assets/tanks/tiger/baked/tiger_{p}.glb' for p in ('hull','turret','gun'))
required.extend(root/f'catalog/maps/canyon_v2_survey_20260930.{s}' for s in ('json','png'))
required.extend(root/p for p in re.findall(r'"(source_assets/[^\"]+)"',(root/'src/terrain_detail.rs').read_text()))
library=root/'assets/audio/library'
manifest_audio=json.loads((library/'manifest.json').read_text())
config_audio=json.loads((library/'soundscape.json').read_text())
bindings={c['asset']:c for c in config_audio['cues']}
audio_count=0
for cue in manifest_audio['assets']:
    if not bindings.get(cue['id'],{}).get('enabled',cue.get('default_enabled',False)): continue
    path=(library/cue['playback_path']).resolve()
    if not path.is_relative_to(library.resolve()) or not path.is_file():
        raise SystemExit(f'Missing or invalid library cue: {cue["id"]}')
    with path.open('rb') as stream: actual=hashlib.file_digest(stream,'sha256').hexdigest()
    if actual!=cue['playback_sha256']: raise SystemExit(f'Changed library PCM: {cue["id"]}')
    required.append(path); audio_count+=1
print(f'Verified {audio_count} enabled sound-library imports.')
missing=[str(p) for p in required if not p.is_file() or p.stat().st_size==0]
if missing: raise SystemExit('Required assets missing:\n'+'\n'.join(missing))
print(f'Validated {len(required)} required files; fixed internal grid800x450, Studio Display fullscreen.')
with binary.open('rb') as f: print('Release SHA256:',hashlib.file_digest(f,'sha256').hexdigest())
# The launcher never builds. Say so loudly when the source is newer than what is about to run.
built=binary.stat().st_mtime
newer=[p for pattern in ('src/**/*.rs','shaders/*') for p in root.glob(pattern) if p.is_file() and p.stat().st_mtime>built]
if newer:
    print('\n*** WARNING: STALE BINARY ***\n'
          f'{len(newer)} source file(s) are newer than target/release/panzerrechnersystem '
          f'(e.g. {newer[0].relative_to(root)}).\n'
          'This session runs the OLD build. Rebuild with: cargo build --release --offline --locked\n',file=sys.stderr)
PY
[[ "$PANZER_MODE" != check ]] || exit 0
mkdir -p "$PANZER_RUNTIME"
exec 9>"$PANZER_RUNTIME/fixed-playtest.lock"
flock -n 9 || { printf 'This playtest is already running.\n' >&2; exit 1; }
# Keep old saved settings intact; fixed mode ignores them. Remove inherited experiment flags.
while IFS= read -r PANZER_ENV_NAME; do unset "$PANZER_ENV_NAME"; done < <(compgen -A variable KESSEL_)
export KESSEL_ASSET_ROOT="$PANZER_ROOT" KESSEL_PLAYTEST_MANIFEST="$PANZER_MANIFEST"
export KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_GRAPHICS_PRESET=high
export KESSEL_FIXED_PLAYTEST_GRAPHICS=1 KESSEL_INTERNAL_RES=800x450 KESSEL_FULLSCREEN=1
export KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4
export KESSEL_SOURCE_VISIBILITY=maps KESSEL_TRANSPORT_QUALITY=full
# Retaining whole rigid casters in the terrain cache is still experimental: small
# vehicle transform changes currently invalidate the large static atlas.
export KESSEL_RETAIN_RIGID_SKY_DEPTH=0
export KESSEL_ENABLE_PIZZA_AUDIO=1 KESSEL_CANYON_V2=1
# Wide playback retains quiet/loud contrast instead of the balanced profile's quiet-source lift.
export KESSEL_AUDIO_RANGE=reference
if [[ "$PANZER_MODE" == play ]]; then export KESSEL_CANYON=1; fi
PANZER_LOG="$PANZER_RUNTIME/fixed-playtest-$(date +%Y%m%d-%H%M%S)-$$.log"
{
  printf '[launcher] root=%s mode=%s fixed_profile=12900K-4090 internal=800x450 fullscreen=1\n' "$PANZER_ROOT" "$PANZER_MODE"
  sha256sum -- "$PANZER_BINARY"
} > "$PANZER_LOG"
ln -sfn "$(basename -- "$PANZER_LOG")" "$PANZER_RUNTIME/fixed-playtest-latest.log"
"$PANZER_BINARY" >> "$PANZER_LOG" 2>&1 &
PANZER_GAME_PID=$!
printf '[launcher] game_pid=%s\n' "$PANZER_GAME_PID" >> "$PANZER_LOG"
printf 'Playtest PID %s; log: %s\n' "$PANZER_GAME_PID" "$PANZER_LOG"
trap 'kill -TERM "$PANZER_GAME_PID" 2>/dev/null || true' TERM INT
PANZER_STATUS=0
wait "$PANZER_GAME_PID" || PANZER_STATUS=$?
printf '[launcher] exit_status=%s\n' "$PANZER_STATUS" >> "$PANZER_LOG"
exit "$PANZER_STATUS"
