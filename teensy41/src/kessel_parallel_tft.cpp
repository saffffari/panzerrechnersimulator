#ifdef ARDUINO

#include "kessel_parallel_tft.h"

#include <Arduino.h>

#include "kessel_config.h"

#if !defined(ARDUINO_TEENSY41)
#error "kessel::ParallelTft requires a Teensy 4.1"
#endif

namespace kessel {
namespace {

// DB0..DB15 are intentionally reordered at the connector so they land on the
// contiguous high half of GPIO6.  This is the exact Apollo wiring.svg loom:
//   DB0  19  DB1  18  DB2  14  DB3  15
//   DB4  40  DB5  41  DB6  17  DB7  16
//   DB8  22  DB9  23  DB10 20  DB11 21
//   DB12 38  DB13 39  DB14 26  DB15 27
constexpr uint8_t kDataPins[16] = {
    19, 18, 14, 15, 40, 41, 17, 16,
    22, 23, 20, 21, 38, 39, 26, 27,
};

constexpr uint32_t kDataMask = 0xFFFF0000u;

// Deliberately conservative for a first bring-up over short jumper wires.
// ILI9486/ILI9488 write-cycle minima are much shorter; these pauses also give
// the GPIO SET/CLEAR writes time to reach the pins before each WR edge.
constexpr uint32_t kBusSettleNs = KESSEL_TFT_BUS_SETTLE_NS;
constexpr uint32_t kWrPulseNs = KESSEL_TFT_WR_PULSE_NS;

constexpr uint8_t kSoftwareReset = 0x01;
constexpr uint8_t kSleepOut = 0x11;
constexpr uint8_t kDisplayOff = 0x28;
constexpr uint8_t kDisplayOn = 0x29;
constexpr uint8_t kColumnAddressSet = 0x2A;
constexpr uint8_t kPageAddressSet = 0x2B;
constexpr uint8_t kMemoryWrite = 0x2C;
constexpr uint8_t kMemoryAccessControl = 0x36;

constexpr uint8_t kILI9486MadctlByRotation[4] = {
    0x48,  // MX | BGR
    0x38,  // MV | ML | BGR (LCDWiki runtime rotation 1)
    0x88,  // MY | BGR
    0xE8,  // MX | MY | MV | BGR
};

constexpr uint8_t kILI9488MadctlByRotation[4] = {
    0xC8,  // MX | MY | BGR
    0xA8,  // MY | MV | BGR
    0x18,  // ML | BGR
    0x78,  // MX | ML | MV | BGR
};

// Compact command tables: command, argument count, arguments..., terminated
// by command 0.  Delays around reset, sleep-out and display-on are kept in the
// controller-specific routines where their ordering is explicit.
constexpr uint8_t kILI9486Init[] = {
    0xF1, 6, 0x36, 0x04, 0x00, 0x3C, 0x0F, 0x8F,
    0xF2, 9, 0x18, 0xA3, 0x12, 0x02, 0xB2, 0x12, 0xFF, 0x10, 0x00,
    0xF8, 2, 0x21, 0x04,
    0xF9, 2, 0x00, 0x08,
    0x36, 1, 0x08,
    0xB4, 1, 0x00,
    0xC1, 1, 0x41,
    0xC5, 4, 0x00, 0x91, 0x80, 0x00,
    0xE0, 15, 0x0F, 0x1F, 0x1C, 0x0C, 0x0F, 0x08, 0x48, 0x98,
              0x37, 0x0A, 0x13, 0x04, 0x11, 0x0D, 0x00,
    0xE1, 15, 0x0F, 0x32, 0x2E, 0x0B, 0x0D, 0x05, 0x47, 0x75,
              0x37, 0x06, 0x10, 0x03, 0x24, 0x20, 0x00,
    0x3A, 1, 0x55,  // 16-bit RGB565 MCU interface
    0,
};

constexpr uint8_t kILI9488Init[] = {
    0xF7, 4, 0xA9, 0x51, 0x2C, 0x82,
    0xC0, 2, 0x11, 0x09,
    0xC1, 1, 0x41,
    0xC5, 3, 0x00, 0x0A, 0x80,
    0xB1, 2, 0xB0, 0x11,
    0xB4, 1, 0x02,
    0xB6, 2, 0x02, 0x22,
    0xB7, 1, 0xC6,
    0xBE, 2, 0x00, 0x04,
    0xE9, 1, 0x00,
    0x36, 1, 0x08,
    0x3A, 1, 0x55,  // 16-bit RGB565 (not SPI's usual 18-bit mode)
    0xE0, 15, 0x00, 0x07, 0x10, 0x09, 0x17, 0x0B, 0x41, 0x89,
              0x4B, 0x0A, 0x0C, 0x0E, 0x18, 0x1B, 0x0F,
    0xE1, 15, 0x00, 0x17, 0x1A, 0x04, 0x0E, 0x06, 0x2F, 0x45,
              0x43, 0x02, 0x0A, 0x09, 0x32, 0x36, 0x0F,
    0,
};

// Compile-time proof of the non-obvious pin permutation.  A future core pin
// map change should fail loudly rather than scramble colors at the connector.
static_assert(CORE_PIN19_BIT == 16, "DB0 must be GPIO6 bit 16");
static_assert(CORE_PIN18_BIT == 17, "DB1 must be GPIO6 bit 17");
static_assert(CORE_PIN14_BIT == 18, "DB2 must be GPIO6 bit 18");
static_assert(CORE_PIN15_BIT == 19, "DB3 must be GPIO6 bit 19");
static_assert(CORE_PIN40_BIT == 20, "DB4 must be GPIO6 bit 20");
static_assert(CORE_PIN41_BIT == 21, "DB5 must be GPIO6 bit 21");
static_assert(CORE_PIN17_BIT == 22, "DB6 must be GPIO6 bit 22");
static_assert(CORE_PIN16_BIT == 23, "DB7 must be GPIO6 bit 23");
static_assert(CORE_PIN22_BIT == 24, "DB8 must be GPIO6 bit 24");
static_assert(CORE_PIN23_BIT == 25, "DB9 must be GPIO6 bit 25");
static_assert(CORE_PIN20_BIT == 26, "DB10 must be GPIO6 bit 26");
static_assert(CORE_PIN21_BIT == 27, "DB11 must be GPIO6 bit 27");
static_assert(CORE_PIN38_BIT == 28, "DB12 must be GPIO6 bit 28");
static_assert(CORE_PIN39_BIT == 29, "DB13 must be GPIO6 bit 29");
static_assert(CORE_PIN26_BIT == 30, "DB14 must be GPIO6 bit 30");
static_assert(CORE_PIN27_BIT == 31, "DB15 must be GPIO6 bit 31");

}  // namespace

void ParallelTft::begin(Controller controller) {
  initialized_ = false;
  controller_ = controller;
  rotation_ = 1;
  width_ = kLandscapeWidth;
  height_ = kLandscapeHeight;

  configurePins();
  hardwareReset();

  // A software reset after the pin/bus test state makes repeated begin() calls
  // deterministic even if the panel retained state across a Teensy reset.
  sendCommand(kSoftwareReset);
  delay(150);
  sendCommand(kDisplayOff);

  if (controller_ == Controller::kILI9488) {
    initILI9488();
  } else {
    initILI9486();
  }

  initialized_ = true;
}

void ParallelTft::configurePins() {
  // Load inactive control levels before enabling outputs to avoid accidental
  // write clocks or chip-select pulses during startup.
  digitalWriteFast(kPinCs, HIGH);
  digitalWriteFast(kPinDc, HIGH);
  digitalWriteFast(kPinReset, HIGH);
  digitalWriteFast(kPinWr, HIGH);
  pinMode(kPinCs, OUTPUT);
  pinMode(kPinDc, OUTPUT);
  pinMode(kPinReset, OUTPUT);
  pinMode(kPinWr, OUTPUT);

  for (uint8_t pin : kDataPins) {
    pinMode(pin, OUTPUT);
  }
  GPIO6_DR_CLEAR = kDataMask;
}

void ParallelTft::hardwareReset() {
  digitalWriteFast(kPinReset, HIGH);
  delay(5);
  digitalWriteFast(kPinReset, LOW);
  delay(20);
  digitalWriteFast(kPinReset, HIGH);
  delay(150);
}

void ParallelTft::initILI9486() {
  // Exact active register table/order from LCDWiki's MAR3513 ILI9486 driver.
  runInitTable(kILI9486Init);
  sendCommand(kSleepOut);
  const uint8_t landscape = kILI9486MadctlByRotation[1];
  sendCommand(kMemoryAccessControl, &landscape, 1);
  delay(120);
  sendCommand(kDisplayOn);
}

void ParallelTft::initILI9488() {
  runInitTable(kILI9488Init);
  sendCommand(kSleepOut);
  const uint8_t landscape = kILI9488MadctlByRotation[1];
  sendCommand(kMemoryAccessControl, &landscape, 1);
  delay(120);
  sendCommand(kDisplayOn);
}

void ParallelTft::runInitTable(const uint8_t* table) {
  while (*table != 0) {
    const uint8_t command = *table++;
    const uint8_t count = *table++;
    sendCommand(command, table, count);
    table += count;
  }
}

void ParallelTft::setRotation(uint8_t rotation) {
  rotation_ = rotation & 3u;
  if ((rotation_ & 1u) != 0u) {
    width_ = kPortraitHeight;
    height_ = kPortraitWidth;
  } else {
    width_ = kPortraitWidth;
    height_ = kPortraitHeight;
  }

  if (initialized_) {
    const uint8_t madctl =
        controller_ == Controller::kILI9488
            ? kILI9488MadctlByRotation[rotation_]
            : kILI9486MadctlByRotation[rotation_];
    sendCommand(kMemoryAccessControl, &madctl, 1);
  }
}

void ParallelTft::fillScreen(uint16_t rgb565) {
  if (!initialized_) return;

  beginWrite();
  setAddressWindow(0, 0, static_cast<uint16_t>(width_ - 1),
                   static_cast<uint16_t>(height_ - 1));
  const uint32_t count = static_cast<uint32_t>(width_) *
                         static_cast<uint32_t>(height_);
  for (uint32_t i = 0; i < count; ++i) {
    writeBusWord(rgb565);
  }
  endWrite();
}

bool ParallelTft::writeRect(int16_t x, int16_t y, int16_t w, int16_t h,
                            const uint16_t* rgb565) {
  if (!initialized_ || rgb565 == nullptr || w <= 0 || h <= 0) return false;

  const int32_t sourceStride = w;
  int32_t sourceX = 0;
  int32_t sourceY = 0;
  int32_t clippedX = x;
  int32_t clippedY = y;
  int32_t clippedW = w;
  int32_t clippedH = h;

  if (clippedX < 0) {
    sourceX = -clippedX;
    clippedW += clippedX;
    clippedX = 0;
  }
  if (clippedY < 0) {
    sourceY = -clippedY;
    clippedH += clippedY;
    clippedY = 0;
  }
  if (clippedX + clippedW > width_) clippedW = width_ - clippedX;
  if (clippedY + clippedH > height_) clippedH = height_ - clippedY;
  if (clippedW <= 0 || clippedH <= 0 || clippedX >= width_ || clippedY >= height_) {
    return false;
  }

  beginWrite();
  setAddressWindow(static_cast<uint16_t>(clippedX),
                   static_cast<uint16_t>(clippedY),
                   static_cast<uint16_t>(clippedX + clippedW - 1),
                   static_cast<uint16_t>(clippedY + clippedH - 1));

  const uint16_t* row = rgb565 + sourceY * sourceStride + sourceX;
  for (int32_t rowIndex = 0; rowIndex < clippedH; ++rowIndex) {
    const uint16_t* pixel = row;
    for (int32_t column = 0; column < clippedW; ++column) {
      writeBusWord(*pixel++);
    }
    row += sourceStride;
  }

  endWrite();
  return true;
}

void ParallelTft::sendCommand(uint8_t command, const uint8_t* data,
                              size_t count) {
  beginWrite();
  writeCommand(command);
  for (size_t i = 0; i < count; ++i) {
    writeData8(data[i]);
  }
  endWrite();
}

inline void ParallelTft::beginWrite() {
  digitalWriteFast(kPinCs, LOW);
}

inline void ParallelTft::endWrite() {
  digitalWriteFast(kPinWr, HIGH);
  digitalWriteFast(kPinDc, HIGH);
  digitalWriteFast(kPinCs, HIGH);
}

inline void ParallelTft::writeCommand(uint8_t command) {
  digitalWriteFast(kPinDc, LOW);
  writeBusWord(command);
}

inline void ParallelTft::writeData8(uint8_t value) {
  digitalWriteFast(kPinDc, HIGH);
  writeBusWord(value);
}

inline void ParallelTft::writeBusWord(uint16_t value) {
  // Atomic SET/CLEAR aliases avoid a read-modify-write on GPIO6 and therefore
  // cannot clobber unrelated low-half GPIO6 pins used by an interrupt.
  GPIO6_DR_CLEAR = kDataMask;
  GPIO6_DR_SET = static_cast<uint32_t>(value) << 16;
  delayNanoseconds(kBusSettleNs);
  digitalWriteFast(kPinWr, LOW);
  delayNanoseconds(kWrPulseNs);
  digitalWriteFast(kPinWr, HIGH);
  delayNanoseconds(kBusSettleNs);
}

void ParallelTft::setAddressWindow(uint16_t x0, uint16_t y0, uint16_t x1,
                                   uint16_t y1) {
  // Called with CS asserted.  Each address byte is one 16-bit bus beat with
  // its value on DB0..DB7; pixel beats after RAMWR use all sixteen lines.
  writeCommand(kColumnAddressSet);
  writeData8(static_cast<uint8_t>(x0 >> 8));
  writeData8(static_cast<uint8_t>(x0));
  writeData8(static_cast<uint8_t>(x1 >> 8));
  writeData8(static_cast<uint8_t>(x1));

  writeCommand(kPageAddressSet);
  writeData8(static_cast<uint8_t>(y0 >> 8));
  writeData8(static_cast<uint8_t>(y0));
  writeData8(static_cast<uint8_t>(y1 >> 8));
  writeData8(static_cast<uint8_t>(y1));

  writeCommand(kMemoryWrite);
  digitalWriteFast(kPinDc, HIGH);
}

}  // namespace kessel

#endif  // ARDUINO
