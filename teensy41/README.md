# Kessel for Teensy 4.1

This is a separate, fixed-memory adaptation of Kessel for a 480x320 TFT.  It does not attempt to
compile the desktop Rust/wgpu renderer on a microcontroller.  It preserves the things that make the
game Kessel: an embodied first-person Tiger, slow westward escape, station workload, travel-locked
gunnery, manual loading, sparse T-34 contacts, deterministic armor resolution, cold whiteout color,
and isolated warm flashes.

## Single-player only

The Teensy edition is permanently **single-player**. One operator moves between every embodied
station and bears the cost of leaving one job to perform another. There is one local `GameState`,
one `InputFrame`, and no LAN, split-screen, networking, replication, second controller, or remote
crew path. Future features must deepen that solo crew workload rather than add multiplayer code.

## Rendering target

- Panel output: **480x320 RGB565**.
- Software render buffer: **240x160, 8-bit indexed color**, nearest-scaled 2x.
- Render buffer: 38,400 bytes.
- Two expanded RGB565 scanlines: 1,920 bytes.
- No full-size framebuffer and no dynamic allocation in the game/render loop.
- Simulation: fixed 60 Hz. Intended render cadence: 30 Hz; actual panel cadence depends on its bus.

The first slice uses a sparse painter-sorted 3D battlefield rather than a triangle GPU pipeline:
perspective snow, authored ruins, trees, tank silhouettes, fog-distance palette bands, snow, a
commander cupola, circular TZF sight, and a loader/breech view.  That is intentionally close to the
desktop game's low-resolution silhouette language while staying within a Cortex-M7 budget.

## Current first playable

The Tiger starts on the east edge and must reach the west line.  Four T-34s wake when the player
enters their range.  The player moves between three compressed solo stations:

| Station | Stick | WORK |
|---|---|---|
| Commander | drive / steer | unused |
| Gunner | traverse / elevate | fire the 88 mm |
| Loader | unused | hold to work and ram the next round |

The tank coasts while the player leaves the commander controls, and the main gun is travel-locked.
That keeps the original game's central choice: one person cannot drive, aim, and load at once.

## Confirmed display and wiring

The owned panel is the Mega-style **MAR3513-family 480x320 shield**, documented as ILI9486 with a
write-only 16-bit 8080 bus. It is neither the provisional SPI ILI9488 nor the Waveshare SPI shield.
The complete signal-by-signal loom, power notes, unused pins, and bring-up test are in
[DISPLAY_WIRING.md](DISPLAY_WIRING.md).

| Signal group | Teensy 4.1 pins |
|---|---|
| TFT DB0–DB15 | 19, 18, 14, 15, 40, 41, 17, 16, 22, 23, 20, 21, 38, 39, 26, 27 |
| TFT WR / DC / CS / RST | 36 / 3 / 2 / 4 |
| Bench joystick X / Y | A10 / A11 (pins 24 / 25) |
| Bench WORK / STATION / MAP | 5 / 6 / 7 to GND |

DB0–DB15 deliberately land on GPIO6 bits 16–31, so the local display backend writes one contiguous
16-bit RGB565 bus. It is blocking: the renderer may safely reuse its two-row expansion buffer as
soon as each write returns. Pin 37 is not electrically connected; reserve it only as a dummy RD for
any future driver API that requires one.

The shield takes 5 V power but Teensy GPIO remains 3.3 V only. Connect both ground holes, keep the
loom under 15 cm, leave the shield's SD/flash pins disconnected, and use the Teensy's native SDIO
socket. This particular shield exposes no touch signals and no backlight-control pin.

The proposed two-handwheel handheld controller, enclosure dimensions, parts list, and final control
pin allocation are in [HANDHELD_CONTROLLER.md](HANDHELD_CONTROLLER.md).

## Build and flash

Install PlatformIO, connect the Teensy 4.1 over USB, then from this directory:

```sh
pio run
pio run --target upload
pio device monitor
```

The display code is project-local; no third-party TFT dependency is needed. The backend expands one
pair of indexed scanlines to RGB565 and writes that 480x2 rectangle over the 16-bit bus. Start with
the conservative strobe timing in `include/kessel_config.h`, run the color-wash gate, and only then
tune transfer speed on the physical loom.

Do not copy Apollo's checked-in `ILI948x_t41_p` setup as the backend. Its loom is useful, but that
dependency is currently configured for an 8-bit bus and the Apollo constructor call does not match
the checked-out API.

For the first panel power-up, build and flash the diagnostic environment instead of the game:

```sh
pio run -e teensy41_wash --target upload
pio device monitor
```

It holds red, green, blue, white, and black full-screen washes, prints each blocking frame time,
then leaves four differently sized corner markers visible. Do not switch to the normal `teensy41`
environment until every wash and corner is correct.

## Host preview and tests

No Teensy toolchain is needed to verify the portable game and renderer:

```sh
mkdir -p ../target/teensy41-preview
c++ -std=c++17 -O2 -Iinclude \
  src/kessel_game.cpp src/kessel_renderer.cpp preview/preview_main.cpp \
  -o ../target/teensy41-preview/kessel_preview
../target/teensy41-preview/kessel_preview ../target/teensy41-preview/kessel

c++ -std=c++17 -O2 -Iinclude \
  src/kessel_game.cpp src/kessel_renderer.cpp preview/test_game.cpp \
  -o ../target/teensy41-preview/test_game
../target/teensy41-preview/test_game
```

The preview writes title, commander, gunner, and loader PPM frames at the exact TFT resolution.

## Memory direction

The first slice needs no PSRAM.  Optional PSRAM is useful later for sprite banks, cached SD assets,
audio, or a second indexed frame, but time-critical render state should remain in internal RAM.  The
native microSD slot is the right home for future compressed campaign maps and ADPCM audio.

## Next milestones

1. Build the confirmed display loom and pass the conservative-speed red/green/blue/white/black wash
   plus a ten-minute sustained-transfer test.
2. Flash this vertical slice and calibrate joystick centres, landscape orientation, RGB/BGR order,
   and sustainable parallel strobe timing.
3. Add a tiny sprite/texture cooker that converts desktop art into palette-indexed flash/SD assets.
4. Add MQS or I2S audio: synthesized engine/track layers first, then ADPCM gun and impact transients.
5. Expand the authored westward route, component damage, AP/HE selection, map marks, and campaign
   persistence.  Add PSRAM only when one of those measured budgets actually needs it.
