#pragma once

#include <stddef.h>
#include <stdint.h>

namespace kessel {

// Write-only 16-bit 8080 backend for the MAR3513-style Mega shield and the
// Apollo loom.  The shield has no RD connection, so this class deliberately
// contains no controller-ID or pixel-read API.
class ParallelTft final {
 public:
  enum class Controller : uint8_t {
    kILI9486,
    kILI9488,
  };

  static constexpr int16_t kPortraitWidth = 320;
  static constexpr int16_t kPortraitHeight = 480;
  static constexpr int16_t kLandscapeWidth = 480;
  static constexpr int16_t kLandscapeHeight = 320;

  // Control pins fixed by docs/wiring.svg in the Apollo project.
  static constexpr uint8_t kPinCs = 2;
  static constexpr uint8_t kPinDc = 3;
  static constexpr uint8_t kPinReset = 4;
  static constexpr uint8_t kPinWr = 36;

  // Initializes the GPIO bus and panel in Kessel's 480x320 landscape mode.
  // The hardware cannot report its ID, so ILI9486 is the intentional default
  // for the MAR3513 shield.
  void begin(Controller controller = Controller::kILI9486);

  // Rotation values follow the common Adafruit convention.  Rotation 1 is
  // the 480x320 landscape orientation used by Kessel.
  void setRotation(uint8_t rotation);
  uint8_t rotation() const { return rotation_; }
  int16_t width() const { return width_; }
  int16_t height() const { return height_; }
  bool initialized() const { return initialized_; }

  void fillScreen(uint16_t rgb565);

  // Blocking transfer.  The source may be reused as soon as this returns;
  // there is no DMA/interrupt lifetime hazard.  Rectangles are clipped to the
  // current rotation while preserving the source stride.
  bool writeRect(int16_t x, int16_t y, int16_t w, int16_t h,
                 const uint16_t* rgb565);

  // Fast-path-shaped convenience for Kessel's existing 480x2 scanline buffer.
  bool writeRect2Rows(int16_t y, const uint16_t* rgb565) {
    return writeRect(0, y, kLandscapeWidth, 2, rgb565);
  }

 private:
  void configurePins();
  void hardwareReset();
  void initILI9486();
  void initILI9488();
  void runInitTable(const uint8_t* table);
  void sendCommand(uint8_t command, const uint8_t* data = nullptr,
                   size_t count = 0);
  void beginWrite();
  void endWrite();
  void writeCommand(uint8_t command);
  void writeData8(uint8_t value);
  void writeBusWord(uint16_t value);
  void setAddressWindow(uint16_t x0, uint16_t y0, uint16_t x1, uint16_t y1);

  Controller controller_ = Controller::kILI9486;
  uint8_t rotation_ = 1;
  int16_t width_ = kLandscapeWidth;
  int16_t height_ = kLandscapeHeight;
  bool initialized_ = false;
};

}  // namespace kessel
