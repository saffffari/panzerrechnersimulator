#include "kessel_renderer.h"

#include <math.h>
#include <stddef.h>
#include <string.h>

namespace kessel {
namespace {

constexpr uint8_t BLACK = 0;
constexpr uint8_t SKY_DARK = 1;
constexpr uint8_t SKY_MID = 2;
constexpr uint8_t SKY_LIGHT = 3;
constexpr uint8_t SNOW_DARK = 4;
constexpr uint8_t SNOW_MID = 5;
constexpr uint8_t SNOW_LIGHT = 6;
constexpr uint8_t STEEL_DARK = 7;
constexpr uint8_t STEEL = 8;
constexpr uint8_t STEEL_LIGHT = 9;
constexpr uint8_t RUIN_DARK = 10;
constexpr uint8_t RUIN = 11;
constexpr uint8_t SMOKE_DARK = 12;
constexpr uint8_t ORANGE_DARK = 14;
constexpr uint8_t ORANGE = 15;
constexpr uint8_t FIRE = 16;
constexpr uint8_t WHITE = 17;
constexpr uint8_t RED = 18;
constexpr uint8_t GREEN = 19;
constexpr uint8_t GRAPHITE = 20;
constexpr uint8_t BROWN = 21;
constexpr uint8_t TANK_DARK = 22;
constexpr uint8_t TANK = 23;

// RGB565 palette: cold blue-grey world, isolated warm emissives.
constexpr uint16_t kPalette[kPaletteSize] = {
    0x0000, 0x18c6, 0x2969, 0x422c, 0x632c, 0x8c51, 0xc618, 0x18e3,
    0x31a6, 0x528a, 0x2945, 0x4228, 0x2104, 0x4a49, 0x7800, 0xf280,
    0xff60, 0xffff, 0xd124, 0x5dc9, 0x3186, 0x61e3, 0x18c2, 0x3184,
    0x73ae, 0xa514, 0x4208, 0x7bef, 0x0010, 0x0390, 0x780f, 0xffdf,
};

enum class ObjectKind : uint8_t { Ruin, Tree, Wreck };

struct WorldObject {
  float x;
  float z;
  float width;
  float height;
  ObjectKind kind;
};

constexpr WorldObject kObjects[] = {
    {39.0f, -15.0f, 10.0f, 9.0f, ObjectKind::Ruin},
    {37.0f, 15.0f, 12.0f, 12.0f, ObjectKind::Ruin},
    {14.0f, -20.0f, 9.0f, 8.0f, ObjectKind::Ruin},
    {9.0f, 18.0f, 11.0f, 10.0f, ObjectKind::Ruin},
    {-16.0f, -15.0f, 12.0f, 8.0f, ObjectKind::Ruin},
    {-21.0f, 17.0f, 8.0f, 11.0f, ObjectKind::Ruin},
    {-43.0f, -18.0f, 9.0f, 7.0f, ObjectKind::Ruin},
    {-45.0f, 16.0f, 11.0f, 10.0f, ObjectKind::Ruin},
    {47.0f, -7.0f, 2.5f, 8.0f, ObjectKind::Tree},
    {30.0f, 23.0f, 3.0f, 10.0f, ObjectKind::Tree},
    {19.0f, 5.0f, 2.5f, 7.0f, ObjectKind::Tree},
    {-5.0f, -22.0f, 3.0f, 9.0f, ObjectKind::Tree},
    {-31.0f, 22.0f, 3.0f, 8.0f, ObjectKind::Tree},
    {-53.0f, -3.0f, 4.0f, 2.5f, ObjectKind::Wreck},
};

struct Projection {
  float distance;
  float lateral;
  int x;
  int ground_y;
  int width;
  int height;
  uint8_t source;
  uint8_t index;
};

float clampf(float value, float lo, float hi) {
  return value < lo ? lo : (value > hi ? hi : value);
}

int clampi(int value, int lo, int hi) {
  return value < lo ? lo : (value > hi ? hi : value);
}

uint32_t noise32(uint32_t value) {
  uint32_t x = value * 747796405u + 2891336453u;
  x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
  return (x >> 22u) ^ x;
}

// Five-wide, seven-high glyphs packed one row per byte.  Only ASCII used by
// the embedded UI is retained; unsupported characters render as a small box.
const uint8_t* glyph(char c) {
  static const uint8_t blank[7] = {0, 0, 0, 0, 0, 0, 0};
  static const uint8_t box[7] = {31, 17, 17, 17, 17, 17, 31};
  static const uint8_t digits[10][7] = {
      {14,17,19,21,25,17,14}, {4,12,4,4,4,4,14}, {14,17,1,2,4,8,31},
      {30,1,1,14,1,1,30}, {2,6,10,18,31,2,2}, {31,16,16,30,1,1,30},
      {14,16,16,30,17,17,14}, {31,1,2,4,8,8,8}, {14,17,17,14,17,17,14},
      {14,17,17,15,1,1,14},
  };
  static const uint8_t letters[26][7] = {
      {14,17,17,31,17,17,17}, {30,17,17,30,17,17,30}, {14,17,16,16,16,17,14},
      {30,17,17,17,17,17,30}, {31,16,16,30,16,16,31}, {31,16,16,30,16,16,16},
      {14,17,16,23,17,17,15}, {17,17,17,31,17,17,17}, {14,4,4,4,4,4,14},
      {7,2,2,2,18,18,12}, {17,18,20,24,20,18,17}, {16,16,16,16,16,16,31},
      {17,27,21,21,17,17,17}, {17,25,21,19,17,17,17}, {14,17,17,17,17,17,14},
      {30,17,17,30,16,16,16}, {14,17,17,17,21,18,13}, {30,17,17,30,20,18,17},
      {15,16,16,14,1,1,30}, {31,4,4,4,4,4,4}, {17,17,17,17,17,17,14},
      {17,17,17,17,17,10,4}, {17,17,17,21,21,21,10}, {17,17,10,4,10,17,17},
      {17,17,10,4,4,4,4}, {31,1,2,4,8,16,31},
  };
  static const uint8_t dash[7] = {0,0,0,31,0,0,0};
  static const uint8_t dot[7] = {0,0,0,0,0,12,12};
  static const uint8_t colon[7] = {0,12,12,0,12,12,0};
  static const uint8_t slash[7] = {1,2,2,4,8,8,16};
  if (c == ' ') return blank;
  if (c >= '0' && c <= '9') return digits[c - '0'];
  if (c >= 'a' && c <= 'z') c = static_cast<char>(c - 'a' + 'A');
  if (c >= 'A' && c <= 'Z') return letters[c - 'A'];
  if (c == '-') return dash;
  if (c == '.') return dot;
  if (c == ':') return colon;
  if (c == '/') return slash;
  return box;
}

}  // namespace

const uint16_t* Renderer::palette565() { return kPalette; }

void Renderer::clear(uint8_t color) {
  memset(pixels_, color, static_cast<size_t>(kRenderWidth * kRenderHeight));
}

void Renderer::put(int x, int y, uint8_t color) {
  if (static_cast<unsigned>(x) >= static_cast<unsigned>(kRenderWidth) ||
      static_cast<unsigned>(y) >= static_cast<unsigned>(kRenderHeight)) return;
  pixels_[y * kRenderWidth + x] = color;
}

void Renderer::hline(int x0, int x1, int y, uint8_t color) {
  if (y < 0 || y >= kRenderHeight) return;
  if (x0 > x1) { const int t = x0; x0 = x1; x1 = t; }
  if (x1 < 0 || x0 >= kRenderWidth) return;
  x0 = clampi(x0, 0, kRenderWidth - 1);
  x1 = clampi(x1, 0, kRenderWidth - 1);
  if (x1 < x0) return;
  memset(pixels_ + y * kRenderWidth + x0, color, static_cast<size_t>(x1 - x0 + 1));
}

void Renderer::vline(int x, int y0, int y1, uint8_t color) {
  if (x < 0 || x >= kRenderWidth) return;
  if (y0 > y1) { const int t = y0; y0 = y1; y1 = t; }
  if (y1 < 0 || y0 >= kRenderHeight) return;
  y0 = clampi(y0, 0, kRenderHeight - 1);
  y1 = clampi(y1, 0, kRenderHeight - 1);
  for (int y = y0; y <= y1; ++y) pixels_[y * kRenderWidth + x] = color;
}

void Renderer::rect(int x, int y, int w, int h, uint8_t color) {
  if (w <= 0 || h <= 0) return;
  const int y0 = clampi(y, 0, kRenderHeight);
  const int y1 = clampi(y + h, 0, kRenderHeight);
  for (int py = y0; py < y1; ++py) hline(x, x + w - 1, py, color);
}

void Renderer::line(int x0, int y0, int x1, int y1, uint8_t color) {
  int dx = x1 > x0 ? x1 - x0 : x0 - x1;
  const int sx = x0 < x1 ? 1 : -1;
  int dy = y1 > y0 ? y0 - y1 : y1 - y0;
  const int sy = y0 < y1 ? 1 : -1;
  int error = dx + dy;
  for (;;) {
    put(x0, y0, color);
    if (x0 == x1 && y0 == y1) break;
    const int twice = error * 2;
    if (twice >= dy) { error += dy; x0 += sx; }
    if (twice <= dx) { error += dx; y0 += sy; }
  }
}

void Renderer::circle(int cx, int cy, int radius, uint8_t color) {
  int x = radius;
  int y = 0;
  int error = 1 - x;
  while (x >= y) {
    put(cx + x, cy + y, color); put(cx + y, cy + x, color);
    put(cx - y, cy + x, color); put(cx - x, cy + y, color);
    put(cx - x, cy - y, color); put(cx - y, cy - x, color);
    put(cx + y, cy - x, color); put(cx + x, cy - y, color);
    ++y;
    if (error < 0) error += 2 * y + 1;
    else { --x; error += 2 * (y - x + 1); }
  }
}

void Renderer::text(const char* value, int x, int y, uint8_t color, int scale) {
  while (*value) {
    const uint8_t* rows = glyph(*value++);
    for (int gy = 0; gy < 7; ++gy) {
      for (int gx = 0; gx < 5; ++gx) {
        if (rows[gy] & (1u << (4 - gx))) rect(x + gx * scale, y + gy * scale, scale, scale, color);
      }
    }
    x += 6 * scale;
  }
}

void Renderer::number(int value, int x, int y, uint8_t color, int scale) {
  char out[12];
  int cursor = 10;
  out[11] = '\0';
  bool negative = value < 0;
  unsigned magnitude = negative ? static_cast<unsigned>(-value) : static_cast<unsigned>(value);
  do {
    out[cursor--] = static_cast<char>('0' + magnitude % 10u);
    magnitude /= 10u;
  } while (magnitude && cursor >= 0);
  if (negative && cursor >= 0) out[cursor--] = '-';
  text(out + cursor + 1, x, y, color, scale);
}

void Renderer::render(const GameState& game) {
  if (game.mode == RunMode::Title) render_title(game);
  else if (game.mode == RunMode::Victory) render_end(game, true);
  else if (game.mode == RunMode::Defeat) render_end(game, false);
  else if (game.map_open) render_map(game);
  else if (game.station == Station::Loader) render_loader(game);
  else render_world(game);
}

void Renderer::render_title(const GameState& game) {
  (void)game;
  clear(SKY_DARK);
  for (int y = 34; y < 95; ++y) hline(0, kRenderWidth - 1, y, y < 62 ? SKY_MID : SKY_LIGHT);
  for (int y = 95; y < kRenderHeight; ++y) hline(0, kRenderWidth - 1, y, (y & 2) ? SNOW_DARK : SNOW_MID);

  // Tiger silhouette, facing west into the whiteout.
  rect(50, 92, 104, 24, TANK_DARK);
  rect(72, 80, 55, 18, TANK_DARK);
  rect(122, 85, 60, 5, TANK_DARK);
  for (int x = 56; x < 150; x += 12) circle(x, 117, 6, BLACK);
  for (int x = 0; x < kRenderWidth; x += 2) put(x, 94 + ((x * 13) & 3), WHITE);

  text("DER KESSEL", 38, 20, WHITE, 3);
  text("TEENSY 4.1", 84, 49, SKY_LIGHT, 1);
  text("HEAD WEST", 87, 132, WHITE, 1);
  text("PRESS WORK", 81, 145, ORANGE, 1);
}

void Renderer::render_world(const GameState& game) {
  const float camera_heading = game.station == Station::Gunner
      ? Game::wrap_angle(game.player.heading + game.player.turret_yaw)
      : game.player.heading;
  const int horizon = game.station == Station::Gunner
      ? 76 + static_cast<int>(game.player.gun_pitch * 110.0f)
      : 67;

  clear(SKY_DARK);
  for (int y = 0; y < horizon; ++y) {
    const int band = (y * 3) / (horizon > 0 ? horizon : 1);
    hline(0, kRenderWidth - 1, y, static_cast<uint8_t>(SKY_DARK + clampi(band, 0, 2)));
  }
  render_ground(game, camera_heading, horizon);
  render_objects(game, camera_heading, horizon);
  snow(game.tick, horizon);
  if (game.station == Station::Gunner) render_gunner(game);
  else render_commander(game);

  if (game.impact_flash > 0.0f && ((game.tick >> 1) & 1u)) {
    for (int x = 0; x < kRenderWidth; x += 3) {
      put(x, 0, game.last_shot_bounced ? WHITE : RED);
      put(x, kRenderHeight - 1, game.last_shot_bounced ? WHITE : RED);
    }
  }
}

void Renderer::render_ground(const GameState& game, float camera_heading, int horizon) {
  const float fov = game.station == Station::Gunner ? 0.44f : 1.05f;
  const float focal = (kRenderWidth * 0.5f) / tanf(fov * 0.5f);
  const float cs = cosf(camera_heading);
  const float sn = sinf(camera_heading);
  constexpr float eye_height = 2.35f;
  for (int y = horizon; y < kRenderHeight; ++y) {
    const float row = static_cast<float>(y - horizon) + 1.0f;
    const float distance = clampf(eye_height * focal / row, 1.0f, 120.0f);
    const uint8_t fogged = distance > 78.0f ? SNOW_DARK : (distance > 38.0f ? SNOW_MID : SNOW_LIGHT);
    for (int x = 0; x < kRenderWidth; ++x) {
      const float lateral = (static_cast<float>(x) - kRenderWidth * 0.5f) * distance / focal;
      const float wx = game.player.x + sn * distance + cs * lateral;
      const float wz = game.player.z + cs * distance - sn * lateral;
      const int ix = static_cast<int>(floorf(wx * 0.22f));
      const int iz = static_cast<int>(floorf(wz * 0.22f));
      const bool road = fabsf(wz) < 4.5f || fabsf(wz - 13.0f) < 2.0f;
      uint8_t color = road ? SNOW_DARK : fogged;
      if (((ix ^ iz) & 3) == 0 && distance < 65.0f) color = road ? STEEL_DARK : SNOW_MID;
      if (((x + y) & 3) == 0 && color == SNOW_LIGHT) color = SNOW_MID;  // stable Bayer-like crush.
      put(x, y, color);
    }
  }
}

void Renderer::render_objects(const GameState& game, float camera_heading, int horizon) {
  constexpr int object_count = static_cast<int>(sizeof(kObjects) / sizeof(kObjects[0]));
  constexpr int total = object_count + kEnemyCount;
  Projection projected[total];
  int count = 0;
  const float fov = game.station == Station::Gunner ? 0.44f : 1.05f;
  const float focal = (kRenderWidth * 0.5f) / tanf(fov * 0.5f);
  const float cs = cosf(camera_heading);
  const float sn = sinf(camera_heading);

  auto project = [&](float wx, float wz, float width, float height, uint8_t source, uint8_t index) {
    const float dx = wx - game.player.x;
    const float dz = wz - game.player.z;
    const float forward = dx * sn + dz * cs;
    const float lateral = dx * cs - dz * sn;
    if (forward <= 1.0f || forward > 125.0f) return;
    const int sx = static_cast<int>(kRenderWidth * 0.5f + lateral * focal / forward);
    const int sw = clampi(static_cast<int>(width * focal / forward), 1, 180);
    if (sx + sw < 0 || sx - sw >= kRenderWidth) return;
    const int gy = horizon + static_cast<int>(2.35f * focal / forward);
    const int sh = clampi(static_cast<int>(height * focal / forward), 1, 145);
    projected[count++] = {forward, lateral, sx, gy, sw, sh, source, index};
  };

  for (int i = 0; i < object_count; ++i) {
    project(kObjects[i].x, kObjects[i].z, kObjects[i].width, kObjects[i].height, 0, static_cast<uint8_t>(i));
  }
  for (int i = 0; i < kEnemyCount; ++i) {
    const EnemyState& enemy = game.enemies[i];
    project(enemy.x, enemy.z, 3.2f, enemy.alive ? 2.7f : 1.8f, 1, static_cast<uint8_t>(i));
  }

  // Painter's order is enough for the sparse vertical slice and costs no depth buffer.
  for (int i = 1; i < count; ++i) {
    Projection key = projected[i];
    int j = i - 1;
    while (j >= 0 && projected[j].distance < key.distance) {
      projected[j + 1] = projected[j];
      --j;
    }
    projected[j + 1] = key;
  }

  for (int i = 0; i < count; ++i) {
    const Projection& p = projected[i];
    const int left = p.x - p.width / 2;
    const int top = p.ground_y - p.height;
    const bool distant = p.distance > 70.0f;
    if (p.source == 0) {
      const WorldObject& object = kObjects[p.index];
      if (object.kind == ObjectKind::Ruin) {
        const uint8_t shade = distant ? SMOKE_DARK : RUIN;
        rect(left, top, p.width, p.height, shade);
        // Broken roofline and empty windows carry more silhouette than texture.
        rect(left + p.width / 5, top, p.width / 5, clampi(p.height / 4, 1, 30), SKY_MID);
        rect(left + p.width * 3 / 5, top, p.width / 6, clampi(p.height / 6, 1, 20), SKY_MID);
        if (p.width > 10) {
          rect(left + p.width / 5, top + p.height / 3, clampi(p.width / 7, 1, 8), clampi(p.height / 5, 1, 12), BLACK);
          rect(left + p.width * 3 / 5, top + p.height / 2, clampi(p.width / 7, 1, 8), clampi(p.height / 5, 1, 12), BLACK);
        }
      } else if (object.kind == ObjectKind::Tree) {
        rect(p.x - clampi(p.width / 10, 1, 3), top + p.height / 2, clampi(p.width / 5, 1, 5), p.height / 2, BROWN);
        for (int row = 0; row < p.height * 3 / 4; ++row) {
          const int half = (row * p.width / (p.height > 0 ? p.height : 1)) / 2;
          hline(p.x - half, p.x + half, top + row, distant ? SMOKE_DARK : TANK_DARK);
        }
      } else {
        rect(left, top + p.height / 2, p.width, p.height / 2, TANK_DARK);
        rect(p.x - p.width / 5, top + p.height / 3, p.width / 2, p.height / 3, RUIN_DARK);
      }
    } else {
      const EnemyState& enemy = game.enemies[p.index];
      if (enemy.alive) {
        const uint8_t shade = distant ? SMOKE_DARK : TANK;
        rect(left, top + p.height * 2 / 5, p.width, p.height * 3 / 5, shade);
        rect(p.x - p.width / 4, top + p.height / 5, p.width / 2, p.height / 3, shade);
        const float world_gun = enemy.heading + enemy.turret_yaw;
        const float apparent = Game::angle_delta(world_gun, camera_heading);
        const int barrel = clampi(p.width, 3, 34);
        line(p.x, top + p.height / 3, p.x + static_cast<int>(sinf(apparent) * barrel),
             top + p.height / 3 - static_cast<int>(cosf(apparent) * barrel * 0.18f), TANK_DARK);
        if (enemy.muzzle_flash > 0.0f) circle(p.x, top + p.height / 3, clampi(p.width / 5, 1, 7), FIRE);
      } else {
        rect(left, p.ground_y - p.height / 3, p.width, p.height / 3, TANK_DARK);
        const int plume = clampi(static_cast<int>(32.0f * (1.0f - p.distance / 130.0f)), 2, 28);
        for (int s = 0; s < plume; s += 3) circle(p.x + ((s * 7) % 5) - 2, p.ground_y - p.height / 3 - s, 2 + s / 9, s < 8 ? ORANGE_DARK : SMOKE_DARK);
      }
    }
  }

  if (game.player_muzzle_flash > 0.0f) {
    circle(kRenderWidth / 2, horizon + 5, 9, FIRE);
    circle(kRenderWidth / 2, horizon + 5, 5, WHITE);
  }
}

void Renderer::render_commander(const GameState& game) {
  // Cupola steel and periscope framing: every frame remains physically inside the Tiger.
  rect(0, 0, 18, kRenderHeight, STEEL_DARK);
  rect(kRenderWidth - 18, 0, 18, kRenderHeight, STEEL_DARK);
  rect(0, 0, kRenderWidth, 12, STEEL_DARK);
  rect(0, kRenderHeight - 25, kRenderWidth, 25, STEEL_DARK);
  for (int x = 20; x < kRenderWidth - 20; x += 44) rect(x, 7, 24, 5, BLACK);
  text("CMD", 22, 143, STEEL_LIGHT, 1);
  text("WEST", 94, 143, WHITE, 1);
  line(119, 139, 119, 151, ORANGE);
  line(113, 145, 119, 139, ORANGE);
  line(125, 145, 119, 139, ORANGE);
  text("ARM", 164, 143, STEEL_LIGHT, 1);
  number(game.armor_integrity, 188, 143, game.armor_integrity > 1 ? GREEN : RED, 1);
}

void Renderer::render_gunner(const GameState& game) {
  const int cx = kRenderWidth / 2;
  const int cy = kRenderHeight / 2;
  const int radius = 70;
  // Mask outside the optic.  Integer circle spans avoid a second framebuffer.
  for (int y = 0; y < kRenderHeight; ++y) {
    const int dy = y - cy;
    const int inside_sq = radius * radius - dy * dy;
    if (inside_sq <= 0) hline(0, kRenderWidth - 1, y, BLACK);
    else {
      const int span = static_cast<int>(sqrtf(static_cast<float>(inside_sq)));
      hline(0, cx - span - 1, y, BLACK);
      hline(cx + span + 1, kRenderWidth - 1, y, BLACK);
    }
  }
  circle(cx, cy, radius, STEEL);
  circle(cx, cy, radius - 1, STEEL_DARK);

  // Compact TZF-style etched reticle: central triangle and flanking stadia.
  line(cx, cy, cx - 5, cy + 10, BLACK);
  line(cx, cy, cx + 5, cy + 10, BLACK);
  line(cx - 5, cy + 10, cx + 5, cy + 10, BLACK);
  for (int side = -1; side <= 1; side += 2) {
    for (int mark = 1; mark <= 3; ++mark) {
      const int x = cx + side * mark * 16;
      line(x, cy + 8, x - side * 3, cy + 13, BLACK);
      line(x, cy + 8, x + side * 3, cy + 13, BLACK);
      line(x - side * 3, cy + 13, x + side * 3, cy + 13, BLACK);
    }
  }
  text("GNR", 8, 145, STEEL_LIGHT, 1);
  text(game.breech == Breech::Loaded ? "READY" : "SPENT", 92, 145,
       game.breech == Breech::Loaded ? GREEN : ORANGE, 1);
  text("AP", 202, 145, STEEL_LIGHT, 1);
  number(game.reserve_rounds + (game.breech == Breech::Loaded ? 1 : 0), 220, 145, WHITE, 1);
  if (fabsf(game.player.speed) >= 0.6f) text("TRAVEL LOCK", 84, 13, ORANGE, 1);
}

void Renderer::render_loader(const GameState& game) {
  clear(STEEL_DARK);
  // Fighting compartment: breech, open tray and ready rack as readable blocks.
  rect(13, 11, 214, 128, STEEL);
  rect(21, 19, 198, 112, RUIN_DARK);
  rect(85, 33, 89, 48, TANK_DARK);
  rect(105, 76, 50, 14, BLACK);
  rect(28, 36, 31, 73, STEEL_DARK);
  for (int i = 0; i < 5; ++i) {
    rect(33, 42 + i * 13, 21, 7, i < game.reserve_rounds ? ORANGE_DARK : BLACK);
    put(52, 45 + i * 13, FIRE);
  }
  circle(185, 55, 9, game.breech == Breech::Loaded ? GREEN : RED);
  text("BREECH", 94, 23, STEEL_LIGHT, 1);
  text(game.breech == Breech::Loaded ? "LOADED" : "SPENT", 99, 96,
       game.breech == Breech::Loaded ? GREEN : ORANGE, 1);

  rect(45, 113, 150, 8, BLACK);
  const int filled = clampi(static_cast<int>(game.reload_progress * 146.0f), 0, 146);
  rect(47, 115, filled, 4, ORANGE);
  text("LDR", 12, 146, STEEL_LIGHT, 1);
  if (game.breech == Breech::Spent && game.reserve_rounds > 0) text("HOLD WORK TO RAM", 75, 132, WHITE, 1);
  else if (game.reserve_rounds == 0 && game.breech == Breech::Spent) text("RACK EMPTY", 90, 132, RED, 1);
  else text("GUN READY", 93, 132, GREEN, 1);
}

void Renderer::render_map(const GameState& game) {
  clear(GRAPHITE);
  rect(12, 8, 216, 144, 24);
  rect(16, 12, 208, 136, SNOW_MID);
  for (int x = 20; x < 220; x += 16) vline(x, 15, 144, SNOW_DARK);
  for (int y = 16; y < 145; y += 16) hline(18, 221, y, SNOW_DARK);

  auto map_x = [](float wx) { return 20 + static_cast<int>((wx + 64.0f) * 1.55f); };
  auto map_y = [](float wz) { return 80 + static_cast<int>(wz * 1.7f); };
  for (const WorldObject& object : kObjects) {
    rect(map_x(object.x) - 2, map_y(object.z) - 2, 5, 5,
         object.kind == ObjectKind::Tree ? TANK_DARK : RUIN_DARK);
  }
  for (int i = 0; i < kEnemyCount; ++i) {
    if (game.enemies[i].alive && game.enemies[i].alerted)
      rect(map_x(game.enemies[i].x) - 1, map_y(game.enemies[i].z) - 1, 3, 3, RED);
  }
  circle(map_x(game.player.x), map_y(game.player.z), 3, BLACK);
  line(map_x(game.player.x), map_y(game.player.z),
       map_x(game.player.x + sinf(game.player.heading) * 6.0f),
       map_y(game.player.z + cosf(game.player.heading) * 6.0f), BLACK);
  vline(map_x(-58.0f), 18, 142, ORANGE);
  text("WEST", 20, 18, BLACK, 1);
  text("FIELD MAP", 92, 139, BLACK, 1);
}

void Renderer::render_end(const GameState& game, bool victory) {
  clear(victory ? SKY_DARK : BLACK);
  for (int y = 92; y < kRenderHeight; ++y) hline(0, kRenderWidth - 1, y, victory ? SNOW_DARK : RUIN_DARK);
  text(victory ? "FRIENDLY LINES" : "TIGER LOST", victory ? 39 : 48, 39,
       victory ? WHITE : RED, 2);
  text(victory ? "THE CREW SURVIVES" : "THE KESSEL CLOSES", victory ? 63 : 58, 72, STEEL_LIGHT, 1);
  text("KILLS", 81, 99, STEEL_LIGHT, 1);
  number(game.kills, 119, 99, WHITE, 1);
  text("PRESS WORK", 81, 136, ORANGE, 1);
}

void Renderer::snow(uint32_t tick, int horizon) {
  for (int i = 0; i < 54; ++i) {
    const uint32_t n = noise32(static_cast<uint32_t>(i) * 991u + (tick >> 2));
    const int x = static_cast<int>((n + tick * (1u + (i & 3))) % kRenderWidth);
    const int y = static_cast<int>(((n >> 12) + tick * (2u + (i & 1))) % kRenderHeight);
    put(x, y, y < horizon ? WHITE : SKY_LIGHT);
    if ((i & 7) == 0) put(x + 1, y + 1, WHITE);
  }
}

}  // namespace kessel
