# MAR3513 16-bit TFT wiring

The display loom is taken from the existing Apollo handheld's
[`wiring.svg`](/Users/alexandersaffari/apollo/apollo_11_retro/docs/wiring.svg). Only the LCD and
power nets are reused here; Apollo's fader and five-way-navigation wiring are not part of Kessel.

The shield is the Mega-style, write-only **MAR3513-family 480x320 TFT**: a 16-bit 8080 parallel
interface with an ILI9486 on the documented version. It is not the provisional SPI ILI9488 or the
Waveshare SPI shield. The [manufacturer manual](https://www.lcdwiki.com/res/MAR3513/3.5inch_Arduino_16BIT_Module_MAR3513_User_Manual_EN.pdf)
documents RGB565, an ILI9486, and onboard 5 V / 3.3 V level conversion.

## Display loom

| Shield signal | Teensy 4.1 | Shield signal | Teensy 4.1 |
|---|---:|---|---:|
| DB0 | 19 | DB8 | 22 |
| DB1 | 18 | DB9 | 23 |
| DB2 | 14 | DB10 | 20 |
| DB3 | 15 | DB11 | 21 |
| DB4 | 40 | DB12 | 38 |
| DB5 | 41 | DB13 | 39 |
| DB6 | 17 | DB14 | 26 |
| DB7 | 16 | DB15 | 27 |
| WR | 36 | RS / DC | 3 |
| CS | 2 | RST | 4 |

This order maps DB0 through DB15 to GPIO6 bits 16 through 31, respectively. The renderer can
therefore replace the whole data bus with one masked register write per RGB565 pixel rather than
scattering sixteen `digitalWrite()` calls.

Power and unused connections:

- One shield 5V hole goes to Teensy VIN for the first USB-powered prototype; the two 5V holes are
  tied together on the module.
- Connect **both** shield GND holes to Teensy ground; they carry the parallel bus return current.
- The shield has no RD header. Pin 37 may remain reserved as a dummy RD only if a future library
  API requires one; the Kessel write-only backend does not need it.
- Leave F_CS, MISO, MOSI, CLK, and SD_CS disconnected. Use the Teensy 4.1 native SDIO socket.
- This shield exposes neither touch signals nor backlight control. The backlight is always on while
  the shield has 5 V.
- Keep the data/control loom under 15 cm and run ground beside it. Start at a deliberately slow
  write strobe and increase it only after the color-wash test is clean.

The documented MAR3513 includes level conversion, but visually identical Mega clones sometimes use
passive networks that attenuate a Teensy's 3.3 V high level. Put optional AHCT output-buffer
footprints on the adapter and verify the shield-side logic voltage during bring-up; never solve a
marginal high by exposing a Teensy pin to 5 V.

Do not feed external 5 V into VIN while the Teensy is also USB-powered unless the VUSB-to-VIN link
has been separated and the final power-path circuit is installed. Teensy GPIO remains 3.3 V only.

## Control allocation around the display

The display consumes the old Kessel prototype controls on pins 2, 3, 4, 14, and 15. The minimal
vertical-slice input mapping is therefore:

| Input | Teensy 4.1 |
|---|---:|
| Joystick X | A10 / pin 24 |
| Joystick Y | A11 / pin 25 |
| WORK | pin 5 to GND |
| STATION | pin 6 to GND |
| MAP | pin 7 to GND |

That direct joystick mapping is only for the first bench build. The complete handheld uses Wire2
on SCL 24 / SDA 25, an ADS1015 for the joystick, and an MCP23017 for slow buttons. Pins 28–33 are
reserved from the start for four encoder channels, FIRE, and the input-expander interrupt.

## Bring-up gate

The Apollo **pin loom** is the source of truth, but its checked-in `ILI948x_t41_p` use is not a
copy-safe display backend: that dependency is configured for an 8-bit bus and its current
constructor does not match the three-argument call in Apollo. Kessel uses a small project-local
16-bit write-only backend instead.

Before relying on game output:

1. Power from USB only and verify 5 V and ground before connecting data.
2. Flash `pio run -e teensy41_wash --target upload`; it shows red, green, blue, white, and black.
3. Confirm landscape orientation, RGB/BGR order, and that no half-screen or byte-swap artifacts
   appear.
4. Measure full-frame transfer time while stepping through conservative strobe delays.
5. Run the wash continuously for ten minutes before adding the controller loom or audio.

Because the shield is write-only, firmware cannot read its controller ID. Keep the controller init
selectable if the physical board proves to be a different ILI9486/ILI9488 revision.
