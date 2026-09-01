#pragma once

#include <stdint.h>

#include "kessel_config.h"
#include "kessel_game.h"

namespace kessel {

class Renderer {
 public:
  explicit Renderer(uint8_t* pixels) : pixels_(pixels) {}

  void render(const GameState& game);
  const uint8_t* pixels() const { return pixels_; }
  static const uint16_t* palette565();

 private:
  void clear(uint8_t color);
  void put(int x, int y, uint8_t color);
  void hline(int x0, int x1, int y, uint8_t color);
  void vline(int x, int y0, int y1, uint8_t color);
  void rect(int x, int y, int w, int h, uint8_t color);
  void line(int x0, int y0, int x1, int y1, uint8_t color);
  void circle(int cx, int cy, int radius, uint8_t color);
  void text(const char* value, int x, int y, uint8_t color, int scale = 1);
  void number(int value, int x, int y, uint8_t color, int scale = 1);

  void render_title(const GameState& game);
  void render_world(const GameState& game);
  void render_ground(const GameState& game, float camera_heading, int horizon);
  void render_objects(const GameState& game, float camera_heading, int horizon);
  void render_commander(const GameState& game);
  void render_gunner(const GameState& game);
  void render_loader(const GameState& game);
  void render_map(const GameState& game);
  void render_end(const GameState& game, bool victory);
  void snow(uint32_t tick, int horizon);

  uint8_t* pixels_;
};

}  // namespace kessel

