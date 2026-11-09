# Source me: the locked look (tools/launch_demo_20261101.sh: deltap-real, 2,048 colours, 1024x576)
# and the shared captures folder, for headless film runs. Clears inherited KESSEL_* flags first.
for v in $(env | grep -o '^KESSEL_[A-Z0-9_]*'); do unset "$v"; done
export KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem
export KESSEL_RENDERER=raymarch KESSEL_GRAPHICS_PRESET=high KESSEL_INTERNAL_RES=1024x576
export KESSEL_LOOK=deltap-real KESSEL_REAL_STEPS=15,15,7
export KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4 KESSEL_SOURCE_VISIBILITY=maps
export KESSEL_TRANSPORT_QUALITY=full KESSEL_DAY_CYCLE_SCALE=6 KESSEL_SKY_START=07:00
export KESSEL_CAPTURES_DIR=/home/alex/panzer/captures
_cfg=/home/alex/panzer/demo-20261101/artifacts/runtime/demo-state/graphics.deltap-real.v1.json
[ -s "$_cfg" ] && export KESSEL_GRAPHICS_CONFIG=$_cfg
