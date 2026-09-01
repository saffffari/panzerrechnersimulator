#ifdef ARDUINO

#include <Arduino.h>

#include "kessel_config.h"
#include "kessel_game.h"
#include "kessel_parallel_tft.h"
#include "kessel_renderer.h"

namespace {

DMAMEM uint8_t frame_buffer[kessel::kRenderWidth * kessel::kRenderHeight];
DMAMEM uint16_t scanline_pair[kessel::kDisplayWidth * 2];

kessel::Game game;
kessel::Renderer renderer(frame_buffer);

#if KESSEL_TFT_ENABLED
kessel::ParallelTft tft;
#endif

struct ButtonState {
  bool previous = false;
  bool held = false;
  bool pressed = false;

  void sample(int pin) {
    held = digitalRead(pin) == LOW;
    pressed = held && !previous;
    previous = held;
  }
};

ButtonState work_button;
ButtonState station_button;
ButtonState map_button;
elapsedMicros since_step;
elapsedMicros since_present;
bool pending_work_press = false;
bool pending_station_press = false;
bool pending_map_press = false;
uint32_t presented_frames = 0;

#if KESSEL_TFT_ENABLED && KESSEL_TFT_COLOR_WASH
void write_solid_marker(int16_t x, int16_t y, int16_t size, uint16_t color) {
  static uint16_t pixels[16 * 16];
  const int count = size * size;
  for (int i = 0; i < count; ++i) pixels[i] = color;
  tft.writeRect(x, y, size, size, pixels);
}

void run_color_wash() {
  struct Wash {
    const char* name;
    uint16_t color;
  };
  constexpr Wash washes[] = {
      {"red", 0xF800}, {"green", 0x07E0}, {"blue", 0x001F},
      {"white", 0xFFFF}, {"black", 0x0000},
  };

  const uint32_t serial_wait_started = millis();
  while (!Serial && millis() - serial_wait_started < 1500u) {
  }
  Serial.println("[kessel] MAR3513 color-wash diagnostic");
  for (const Wash& wash : washes) {
    const uint32_t started = micros();
    tft.fillScreen(wash.color);
    const uint32_t elapsed = micros() - started;
    Serial.printf("[kessel] %-5s full frame: %lu us (%.1f fps)\n", wash.name,
                  static_cast<unsigned long>(elapsed),
                  elapsed ? 1000000.0f / static_cast<float>(elapsed) : 0.0f);
    delay(800);
  }

  // Different sizes make rotation and every corner unambiguous even if a
  // color channel is swapped: TL red 16, TR green 12, BL blue 8, BR white 4.
  write_solid_marker(0, 0, 16, 0xF800);
  write_solid_marker(tft.width() - 12, 0, 12, 0x07E0);
  write_solid_marker(0, tft.height() - 8, 8, 0x001F);
  write_solid_marker(tft.width() - 4, tft.height() - 4, 4, 0xFFFF);
  Serial.println("[kessel] wash complete; inspect four asymmetric corner markers");
}
#endif

float read_axis(int pin) {
  const int raw = analogRead(pin) - 512;
  if (raw > -58 && raw < 58) return 0.0f;
  float value = static_cast<float>(raw) / 511.0f;
  if (value < -1.0f) value = -1.0f;
  if (value > 1.0f) value = 1.0f;
  return value;
}

kessel::InputFrame read_input() {
  work_button.sample(KESSEL_BUTTON_WORK);
  station_button.sample(KESSEL_BUTTON_STATION);
  map_button.sample(KESSEL_BUTTON_MAP);
  kessel::InputFrame input;
  input.x = read_axis(KESSEL_STICK_X);
  input.y = read_axis(KESSEL_STICK_Y);
  input.work_held = work_button.held;
  input.work_pressed = work_button.pressed;
  input.station_pressed = station_button.pressed;
  input.map_pressed = map_button.pressed;
  return input;
}

void present() {
#if KESSEL_TFT_ENABLED
  const uint8_t* source = renderer.pixels();
  const uint16_t* palette = kessel::Renderer::palette565();
  for (int y = 0; y < kessel::kRenderHeight; ++y) {
    uint16_t* first = scanline_pair;
    uint16_t* second = scanline_pair + kessel::kDisplayWidth;
    for (int x = 0; x < kessel::kRenderWidth; ++x) {
      const uint16_t color = palette[source[y * kessel::kRenderWidth + x]];
      first[x * 2] = color;
      first[x * 2 + 1] = color;
      second[x * 2] = color;
      second[x * 2 + 1] = color;
    }
    tft.writeRect2Rows(y * 2, scanline_pair);
  }
#endif
}

}  // namespace

void setup() {
  pinMode(KESSEL_BUTTON_WORK, INPUT_PULLUP);
  pinMode(KESSEL_BUTTON_STATION, INPUT_PULLUP);
  pinMode(KESSEL_BUTTON_MAP, INPUT_PULLUP);
  analogReadResolution(10);
  Serial.begin(115200);

#if KESSEL_TFT_ENABLED
#if KESSEL_TFT_CONTROLLER == 9488
  tft.begin(kessel::ParallelTft::Controller::kILI9488);
#elif KESSEL_TFT_CONTROLLER == 9486
  tft.begin(kessel::ParallelTft::Controller::kILI9486);
#else
#error "KESSEL_TFT_CONTROLLER must be 9486 or 9488"
#endif
  tft.setRotation(1);
#endif
#if KESSEL_TFT_ENABLED && KESSEL_TFT_COLOR_WASH
  run_color_wash();
  return;
#endif
  renderer.render(game.state());
  present();
  since_step = 0;
  since_present = 0;
}

void loop() {
#if KESSEL_TFT_COLOR_WASH
  delay(1000);
  return;
#endif
  const kessel::InputFrame sampled = read_input();
  // A button edge may arrive between fixed simulation ticks.  Latch it until a
  // tick actually consumes it instead of losing very short presses.
  pending_work_press |= sampled.work_pressed;
  pending_station_press |= sampled.station_pressed;
  pending_map_press |= sampled.map_pressed;
  constexpr uint32_t step_us = static_cast<uint32_t>(1000000.0f / kessel::kSimulationHz);
  constexpr uint32_t present_us = static_cast<uint32_t>(1000000.0f / kessel::kRenderHz);

  // Bounded catch-up prevents a slow panel transfer from making simulation time run
  // away.  Edge-triggered input is consumed only on the first catch-up step.
  int steps = 0;
  while (since_step >= step_us && steps < 8) {
    kessel::InputFrame step_input = sampled;
    step_input.work_pressed = pending_work_press;
    step_input.station_pressed = pending_station_press;
    step_input.map_pressed = pending_map_press;
    game.update(step_input, kessel::kSimulationStep);
    pending_work_press = false;
    pending_station_press = false;
    pending_map_press = false;
    since_step -= step_us;
    ++steps;
  }

  if (since_present >= present_us) {
    // Consume one display period before the blocking transfer. Time spent on
    // the bus then counts toward the next frame instead of being followed by
    // an unnecessary full-period sleep.
    since_present -= present_us;
    renderer.render(game.state());
    const uint32_t transfer_started = micros();
    present();
    const uint32_t transfer_us = micros() - transfer_started;
    ++presented_frames;
    if ((presented_frames & 31u) == 0u) {
      Serial.printf("[kessel] TFT transfer %lu us (%.1f fps payload+driver)\n",
                    static_cast<unsigned long>(transfer_us),
                    transfer_us ? 1000000.0f / static_cast<float>(transfer_us) : 0.0f);
    }
    // When transfer itself is slower than the 30 Hz target, run at measured
    // bus throughput without accumulating an ever-growing render backlog.
    if (since_present > present_us) since_present = present_us;
  }
}

#endif  // ARDUINO
