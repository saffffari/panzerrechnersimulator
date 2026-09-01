#pragma once

#include <stdint.h>

namespace kessel {

// The TFT is 480x320, but the software renderer works on a deliberately chunky
// 240x160 palette buffer.  The display backend expands every source pixel to a
// 2x2 RGB565 block while it streams the frame to the panel.
constexpr int kRenderWidth = 240;
constexpr int kRenderHeight = 160;
constexpr int kDisplayWidth = 480;
constexpr int kDisplayHeight = 320;
constexpr int kScale = 2;
constexpr int kPaletteSize = 32;

constexpr float kSimulationHz = 60.0f;
constexpr float kSimulationStep = 1.0f / kSimulationHz;
constexpr float kRenderHz = 30.0f;

static_assert(kRenderWidth * kScale == kDisplayWidth, "horizontal scale must fill TFT");
static_assert(kRenderHeight * kScale == kDisplayHeight, "vertical scale must fill TFT");

}  // namespace kessel

// Confirmed display: the write-only MAR3513-family 16-bit 8080 shield on the
// Apollo loom. The documented module is ILI9486; use 9488 only if the physical
// color-wash test proves this particular shield is the alternate revision.
#ifndef KESSEL_TFT_ENABLED
#define KESSEL_TFT_ENABLED 1
#endif
#ifndef KESSEL_TFT_CONTROLLER
#define KESSEL_TFT_CONTROLLER 9486
#endif

// Conservative GPIO timings for the first short-loom bring-up. Lower these
// only after the RGB/white/black wash and sustained-transfer tests pass.
#ifndef KESSEL_TFT_BUS_SETTLE_NS
#define KESSEL_TFT_BUS_SETTLE_NS 40u
#endif
#ifndef KESSEL_TFT_WR_PULSE_NS
#define KESSEL_TFT_WR_PULSE_NS 40u
#endif

// The dedicated PlatformIO `teensy41_wash` environment overrides this and
// boots into a panel diagnostic instead of the game.
#ifndef KESSEL_TFT_COLOR_WASH
#define KESSEL_TFT_COLOR_WASH 0
#endif
#if KESSEL_TFT_COLOR_WASH && !KESSEL_TFT_ENABLED
#error "KESSEL_TFT_COLOR_WASH requires KESSEL_TFT_ENABLED"
#endif

// Pins 24/25 are either direct bench ADC inputs or the final controller's
// Wire2 bus; they must never be configured as both. Only the bench profile is
// implemented until the ADS1015/MCP23017 control board exists.
#define KESSEL_INPUT_DIRECT_BENCH 1
#define KESSEL_INPUT_CONTROLLER_BOARD 2
#ifndef KESSEL_INPUT_PROFILE
#define KESSEL_INPUT_PROFILE KESSEL_INPUT_DIRECT_BENCH
#endif

#if KESSEL_INPUT_PROFILE != KESSEL_INPUT_DIRECT_BENCH
#error "The ADS1015/MCP23017 controller-board input backend is not implemented yet"
#endif

// One two-axis stick and three active-low buttons are sufficient for the first
// playable.  Controls are contextual: drive at Commander, lay at Gunner, work
// the breech at Loader.
#ifndef KESSEL_STICK_X
#define KESSEL_STICK_X A10
#endif
#ifndef KESSEL_STICK_Y
#define KESSEL_STICK_Y A11
#endif
#ifndef KESSEL_BUTTON_WORK
#define KESSEL_BUTTON_WORK 5
#endif
#ifndef KESSEL_BUTTON_STATION
#define KESSEL_BUTTON_STATION 6
#endif
#ifndef KESSEL_BUTTON_MAP
#define KESSEL_BUTTON_MAP 7
#endif
