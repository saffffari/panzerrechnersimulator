#include <assert.h>
#include <math.h>

#include "kessel_config.h"
#include "kessel_game.h"
#include "kessel_renderer.h"

int main() {
  using namespace kessel;
  Game game;
  uint8_t pixels[kRenderWidth * kRenderHeight];
  Renderer renderer(pixels);
  auto verify_frame = [&]() {
    renderer.render(game.state());
    for (int i = 0; i < kRenderWidth * kRenderHeight; ++i) {
      assert(pixels[i] < kPaletteSize);
    }
  };
  assert(game.state().mode == RunMode::Title);
  verify_frame();

  InputFrame start;
  start.work_pressed = true;
  game.update(start, 1.0f / 60.0f);
  assert(game.state().mode == RunMode::Playing);
  assert(game.state().station == Station::Commander);
  verify_frame();

  InputFrame station;
  station.station_pressed = true;
  game.update(station, 1.0f / 60.0f);
  assert(game.state().station == Station::Gunner);
  verify_frame();
  station.station_pressed = false;
  game.update(station, 1.0f / 60.0f);

  // Fire only works while posted at the gun with a stopped tank.
  InputFrame fire;
  fire.work_pressed = true;
  game.update(fire, 1.0f / 60.0f);
  assert(game.state().breech == Breech::Spent);

  station.station_pressed = true;
  game.update(station, 1.0f / 60.0f);
  assert(game.state().station == Station::Loader);
  InputFrame load;
  load.work_held = true;
  for (int i = 0; i < 240; ++i) game.update(load, 1.0f / 60.0f);
  assert(game.state().breech == Breech::Loaded);
  assert(game.state().reserve_rounds == 11);
  verify_frame();

  InputFrame map;
  map.map_pressed = true;
  game.update(map, 1.0f / 60.0f);
  assert(game.state().map_open);
  verify_frame();

  game.debug_state().map_open = false;
  game.debug_state().player.x = -58.1f;
  game.update(InputFrame{}, 1.0f / 60.0f);
  assert(game.state().mode == RunMode::Victory);
  verify_frame();

  assert(fabsf(Game::angle_delta(-3.13f, 3.13f)) < 0.03f);
  return 0;
}
