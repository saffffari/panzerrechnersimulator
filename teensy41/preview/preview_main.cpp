#include <errno.h>
#include <stdio.h>
#include <string.h>

#include "kessel_config.h"
#include "kessel_game.h"
#include "kessel_renderer.h"

namespace {

bool write_ppm(const char* path, const kessel::Renderer& renderer) {
  FILE* file = fopen(path, "wb");
  if (!file) {
    fprintf(stderr, "cannot open %s: %s\n", path, strerror(errno));
    return false;
  }
  fprintf(file, "P6\n%d %d\n255\n", kessel::kDisplayWidth, kessel::kDisplayHeight);
  const uint8_t* pixels = renderer.pixels();
  const uint16_t* palette = kessel::Renderer::palette565();
  for (int y = 0; y < kessel::kRenderHeight; ++y) {
    for (int sy = 0; sy < kessel::kScale; ++sy) {
      for (int x = 0; x < kessel::kRenderWidth; ++x) {
        const uint16_t rgb565 = palette[pixels[y * kessel::kRenderWidth + x]];
        const unsigned char rgb[3] = {
            static_cast<unsigned char>(((rgb565 >> 11) & 31u) * 255u / 31u),
            static_cast<unsigned char>(((rgb565 >> 5) & 63u) * 255u / 63u),
            static_cast<unsigned char>((rgb565 & 31u) * 255u / 31u),
        };
        for (int sx = 0; sx < kessel::kScale; ++sx) fwrite(rgb, 1, 3, file);
      }
    }
  }
  fclose(file);
  return true;
}
void start_game(kessel::Game& game) {
  kessel::InputFrame input;
  input.work_pressed = true;
  input.work_held = true;
  game.update(input, kessel::kSimulationStep);
}

}  // namespace

int main(int argc, char** argv) {
  const char* prefix = argc > 1 ? argv[1] : "kessel-teensy";
  uint8_t pixels[kessel::kRenderWidth * kessel::kRenderHeight];
  kessel::Renderer renderer(pixels);
  kessel::Game game;
  char path[512];

  renderer.render(game.state());
  snprintf(path, sizeof(path), "%s-title.ppm", prefix);
  if (!write_ppm(path, renderer)) return 1;

  start_game(game);
  // Drive west through the opening lane long enough to make the battlefield
  // composition differ from the title, then stop and post at the gun.
  kessel::InputFrame drive;
  drive.y = -0.86f;
  for (int i = 0; i < 180; ++i) game.update(drive, kessel::kSimulationStep);
  for (int i = 0; i < 90; ++i) game.update(kessel::InputFrame{}, kessel::kSimulationStep);
  renderer.render(game.state());
  snprintf(path, sizeof(path), "%s-commander.ppm", prefix);
  if (!write_ppm(path, renderer)) return 1;

  kessel::InputFrame station;
  station.station_pressed = true;
  game.update(station, kessel::kSimulationStep);
  renderer.render(game.state());
  snprintf(path, sizeof(path), "%s-gunner.ppm", prefix);
  if (!write_ppm(path, renderer)) return 1;

  station = kessel::InputFrame{};
  station.station_pressed = true;
  game.update(station, kessel::kSimulationStep);
  game.debug_state().breech = kessel::Breech::Spent;
  for (int i = 0; i < 150; ++i) {
    kessel::InputFrame work;
    work.work_held = true;
    game.update(work, kessel::kSimulationStep);
  }
  renderer.render(game.state());
  snprintf(path, sizeof(path), "%s-loader.ppm", prefix);
  if (!write_ppm(path, renderer)) return 1;

  printf("wrote %s-{title,commander,gunner,loader}.ppm\n", prefix);
  printf("palette framebuffer: %d bytes; expanded TFT frame: %d pixels\n",
         kessel::kRenderWidth * kessel::kRenderHeight,
         kessel::kDisplayWidth * kessel::kDisplayHeight);
  return 0;
}
