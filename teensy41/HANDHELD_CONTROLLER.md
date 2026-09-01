# Kessel Pocket handheld controller

This is the first-pass control and enclosure design for the single-player Teensy 4.1 edition. The
owned display is now identified as the Mega-style MAR3513-family 480x320 shield and uses a
write-only 16-bit parallel loom. The exact LCD wiring and its occupied pins are recorded in
[DISPLAY_WIRING.md](DISPLAY_WIRING.md). A replaceable adapter is still useful mechanically and for
signal conditioning, but the display bus is no longer an unknown.

## Handheld envelope

- Closed size: **180 x 112 x 36 mm** (about 7.1 x 4.4 x 1.4 inches), excluding small rear grip
  bulges.
- Display opening: about **76 x 52 mm**, centered high in the shell. Allow a **98 x 62 mm** pocket
  behind it until the exact TFT PCB is measured.
- Front controls are low in the shell so the screen is not covered by either hand.
- The panel-mounted gun knobs are 25 mm diameter and vertically aligned under the right thumb and
  index finger. They are close enough to reach without releasing the right grip, but far enough
  apart to distinguish by touch.
- The right shoulder FIRE trigger has a 1.5-2 mm guard lip. A shot is accepted only after the
  trigger has been released once after boot.

Suggested coordinates are in millimetres from the upper-left corner of the front shell:

```text
                           180
       +-----------------------------------------+
       |                                  FIRE   |
       |          +------ 76 x 52 ------+        |
       |          |      480 x 320      |    (E) |  ELEVATION
       |          |         TFT         |        |  press = optic zoom
       |          +---------------------+    (T) |  TRAVERSE
       |  ( D )      [CMD][GUN][LOAD]            |  press = fine/coarse
       |  DRIVE       [MAP] [AP|HE] [LOAD]       |
       +-----------------------------------------+
                           112

Display opening: x=51, y=9       Drive stick: x=25, y=82
Elevation knob:  x=154, y=54     Traverse knob: x=154, y=86
```

The two gun controls are **incremental encoders**, not potentiometers. They have no end stops, so
traverse can rotate indefinitely and elevation never develops a false mechanical centre. Firmware
maps slow clicks to very small aim corrections and fast rotation to coarse slewing.

## Rev A control parts

| Qty | Function | Recommended part | Why |
|---:|---|---|---|
| 2 | Gun elevation and traverse | [Bourns PEC11R-4015F-S0024](https://www.digikey.com/en/products/detail/bourns-inc/PEC11R-4015F-S0024/4499668) | No detent, 24 PPR, push switch, inexpensive, compact 12 mm body |
| 2 | Handwheel | [Adafruit 1-inch machined knob #2056](https://www.adafruit.com/product/2056) | 25 mm weighted knob with rubber grip; fits the 6 mm encoder shaft |
| 1 | Driving | [Adafruit mini 2-axis joystick #2765](https://www.adafruit.com/product/2765) | Self-centering throttle/brake plus skid steering in a 17.5 mm body |
| 1 | Joystick prototype carrier | [Adafruit breakout #3246](https://www.adafruit.com/product/3246) | Convenient for the breadboard build; omit it on the custom PCB |
| 6 | Station/map/load/back keys | [Aratas/Omron B3F-1000](https://www.digikey.com/en/products/detail/aratas-formerly-omron-components/B3F-1000/33150) | Small, readily stocked 6 mm tactile switch; use printed caps/plungers |
| 1 | Shoulder FIRE | [Omron D2F-01F](https://www.digikey.com/en/products/detail/omron-electronics-inc-emc-div/D2F-01F/83266) | Compact light-force microswitch with a long mechanical life |
| 1 | AP/HE selector | [E-Switch EG1218](https://www.digikey.com/en/products/detail/e-switch/EG1218/101726) | Physical two-position SPDT slide switch |
| 1 | Digital input expansion | [Microchip MCP23017](https://www.microchip.com/en-us/product/MCP23017) | Sixteen 3.3 V I/O lines and an interrupt output on I2C |
| 1 | Joystick ADC | [TI ADS1015](https://www.ti.com/product/ADS1015) | Two joystick channels without consuming TFT-sensitive Teensy ADC pins |

At August 2026 single-unit pricing, the mechanical controls are about **$23** before shipping. Add
roughly $7-12 for the expander, ADC, connectors, protection, and passives, so a practical controls
budget is **$30-35**, excluding the Teensy, TFT, PCB, enclosure, battery, and audio.

An optional [DRV2605L haptic driver](https://www.adafruit.com/product/2305) and
[10 mm vibration disc](https://www.adafruit.com/product/1201) add a short breech/recoil cue for about
$10. Put the motor in the right grip on rubber isolation so it does not shake the TFT connector.

### Premium encoder swaps

Do not buy premium encoders until the knob geometry has passed a play test. Keep the panel cutout
and connector replaceable so either upgrade can be tried later:

- [Bourns EMS22Q31-B28-LS3](https://www.digikey.com/en/products/detail/bourns-inc/EMS22Q31-B28-LS3/3693508):
  128 PPR magnetic quadrature, smooth/no detents, 3.3 V, and a 100-million-revolution rating; about
  $59 each.
- [Grayhill 62AG11-L5-060C](https://www.digikey.com/en/products/detail/grayhill-inc/62AG11-L5-060C/3673041):
  32 PPR optical encoder with tactile detents and push, one-million-revolution rating; about $39
  each. Power it from 5 V but pull its open-collector A/B outputs up to **3.3 V only**.

## Input architecture

```text
Elevation encoder A/B ---- interrupt GPIO ----+
Traverse encoder A/B ----- interrupt GPIO ----+--> Teensy 4.1 game input
FIRE microswitch ---------- direct GPIO -------+

Joystick X/Y ------------- ADS1015 --+
Buttons / AP-HE / pushes - MCP23017 -+-- Wire2 --+
                                     +-- INT GPIO

MAR3513 TFT <---- 16-bit adapter/loom <---- GPIO6 display backend
```

- Run the final control bus on Teensy `Wire2`: **SCL pin 24, SDA pin 25**, as shown by the
  [PJRC Wire pin map](https://www.pjrc.com/teensy/td_libs_Wire.html), with 4.7 kOhm pull-ups to
  3.3 V. The display uses the normal Wire pins 18/19 as DB1/DB0, so the final controller firmware
  must construct both I2C peripherals on `Wire2`, not the libraries' default `Wire` instance.
- Connect each PEC11R encoder common to ground. Give A and B a 10 kOhm pull-up to 3.3 V, a
  220 Ohm-1 kOhm series resistor, and 10 nF to ground, following the conditioning shown in the
  [PEC11R datasheet](https://www.bourns.com/docs/Product-Datasheets/pec11R.pdf).
- Decode all four quadrature states with a valid Gray-code transition table. Invalid jumps are
  noise and must not move the gun.
- Connect FIRE directly to a Teensy GPIO with a 10 kOhm pull-up, 1 kOhm series resistor, and 10 nF
  capacitor. The other low-rate switches use MCP23017 inputs and its interrupt output.
- All control signals are **3.3 V**. [Teensy 4.1 GPIO is not 5 V tolerant](https://www.pjrc.com/store/teensy41.html).
  The TFT's 5 V supply is a
  separate power rail, not a logic voltage.
- Use the Teensy 4.1 built-in SDIO socket for game assets. Disable the TFT shield's SD card and keep
  its chip select inactive.

The display now owns pins 2, 3, 4, 14–23, 26, 27, 36, and 38–41. The Rev A controller allocation is
therefore fixed as follows:

| Function | Teensy 4.1 pin |
|---|---:|
| Wire2 SCL / SDA | 24 / 25 |
| Traverse encoder A / B | 28 / 29 |
| Elevation encoder A / B | 30 / 31 |
| FIRE | 32 |
| MCP23017 interrupt | 33 |

Encoder pushes, AP/HE, station, map, load, and back keys go to the MCP23017. Joystick X/Y go to the
ADS1015. This leaves pins 10 and 12 available for MQS audio and avoids every display signal. During
the minimal pre-controller prototype only, the joystick may connect directly to A10/A11 (24/25)
and WORK/STATION/MAP to 5/6/7. The direct joystick cannot coexist with Wire2, but the three bench
buttons do not collide with the final encoder allocation.

The checked-in firmware currently enables only that direct bench profile. It deliberately raises a
compile error if the controller-board profile is selected before the ADS1015/MCP23017 input backend
is implemented.

## Main carrier and panel boards

Use a modular stack rather than making the large Mega shield the permanent main board:

1. **Main carrier, about 104 x 66 mm:** Teensy 4.1, MCP23017, ADS1015, protection/passives, audio
   connector, power connector, and keyed control harnesses. Mount it behind the display/lower
   centre with four M2.5 holes.
2. **MAR3513 TFT adapter:** maps the confirmed 16-bit shield loom to the carrier. Put 22-33 Ohm
   source resistors beside the Teensy or buffer outputs—the signal source, not the TFT—provide both
   ground returns, and keep the complete bus under 15 cm. Leave footprints for AHCT output buffering
   because a clone's Mega-oriented passive input network may attenuate 3.3 V logic even when the
   nominal module family is described as 3.3 V compatible.
3. **Left control board, about 30 x 32 mm:** raw joystick plus its two RC-filtered ADC outputs.
4. **Right control harness/board, about 36 x 72 mm:** two panel-mounted encoders, their filters, and
   the shoulder FIRE switch. Use a keyed connector so the entire grip can be removed for service.

The TFT mounts to bezel standoffs, never by loading its Arduino headers. Six rear M2.5 screws should
open the case without removing knobs or buttons. A raw Mega shield stack may temporarily require a
42-45 mm-deep prototype shell; the custom TFT adapter is what brings the final target back to 36 mm.

## Power stages

For Rev A, use a known-good 5 V / 1 A USB source or power bank. It is faster and safer to prove the
display bandwidth and controls before adding a battery subsystem.

For the integrated handheld revision:

```text
USB-C 5 V -> fuse/ESD -> BQ24074 power-path charger -> protected 1S 2500 mAh LiPo
                                                    -> 5 V boost (EN switch)
                                                    -> TFT 5 V + Teensy VIN
Teensy 3.3 V --------------------------------------> controls only
```

Useful prototype modules are the [Adafruit BQ24074 charger #4755](https://www.adafruit.com/product/4755),
[PowerBoost 1000 Basic #2030](https://www.adafruit.com/product/2030), and
[protected 2500 mAh LiPo #328](https://www.adafruit.com/product/328). Put 220 uF low-ESR bulk
capacitance near the TFT and 10 uF near the Teensy carrier. If the battery's boosted 5 V can be
active while USB programming is connected, separate the Teensy VUSB and VIN pads to prevent
back-feeding. PowerBoost 1000 Basic produces 5.2 V; verify that against the identified TFT module or
use a regulated 5.0 V boost stage in the final PCB.

## Prototype gates

1. Build the exact loom in [DISPLAY_WIRING.md](DISPLAY_WIRING.md), then pass the RGB/white/black
   color-wash and sustained-transfer test before connecting the controls. Use PlatformIO's
   `teensy41_wash` environment for this test.
2. Breadboard both PEC11R encoders, the joystick, FIRE, MCP23017, and ADS1015 on USB power. Log raw
   counts and reject-rate while turning both knobs together.
3. Run the same input test while the TFT continuously transfers frames. No missed encoder steps or
   audio interruptions are acceptable.
4. Print a 1:1 paper template, then make a cardboard or rough 3D-printed 180 x 112 mm faceplate.
   Verify that both gun knobs can be operated simultaneously and that FIRE cannot be hit by accident.
5. Only then freeze the PCB outlines, exact holes, battery position, and display adapter.
