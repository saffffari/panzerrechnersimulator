#pragma once

#include <stdint.h>

namespace kessel {

constexpr int kEnemyCount = 4;

// The embedded edition deliberately owns exactly one local crew state and one
// input frame. Multiplayer/replication is outside this target's design, not a
// deferred subsystem.

enum class RunMode : uint8_t {
  Title,
  Playing,
  Victory,
  Defeat,
};

enum class Station : uint8_t {
  Commander,
  Gunner,
  Loader,
};

enum class Breech : uint8_t {
  Loaded,
  Spent,
};

struct InputFrame {
  float x = 0.0f;
  float y = 0.0f;
  bool work_held = false;
  bool work_pressed = false;
  bool station_pressed = false;
  bool map_pressed = false;
};

struct TankState {
  float x = 0.0f;
  float z = 0.0f;
  float heading = 0.0f;
  float speed = 0.0f;
  float turret_yaw = 0.0f;
  float gun_pitch = 0.0f;
};

struct EnemyState {
  float x = 0.0f;
  float z = 0.0f;
  float heading = 0.0f;
  float turret_yaw = 0.0f;
  float reload = 0.0f;
  float muzzle_flash = 0.0f;
  bool alive = true;
  bool alerted = false;
};

struct GameState {
  RunMode mode = RunMode::Title;
  Station station = Station::Commander;
  Breech breech = Breech::Loaded;
  TankState player{};
  EnemyState enemies[kEnemyCount]{};
  uint32_t tick = 0;
  int reserve_rounds = 12;
  int armor_integrity = 3;
  int kills = 0;
  float reload_progress = 0.0f;
  float player_muzzle_flash = 0.0f;
  float impact_flash = 0.0f;
  float mission_time = 0.0f;
  bool map_open = false;
  bool last_shot_bounced = false;
};

class Game {
 public:
  Game();

  void reset();
  void update(const InputFrame& input, float dt);

  const GameState& state() const { return state_; }
  GameState& debug_state() { return state_; }

  static float wrap_angle(float angle);
  static float angle_delta(float target, float current);

 private:
  void update_player(const InputFrame& input, float dt);
  void update_enemies(float dt);
  void fire_player();
  void fire_enemy(int index, float range);
  bool collides(float x, float z) const;
  static uint32_t hash(uint32_t value);
  static float hash01(uint32_t value);

  GameState state_{};
};

}  // namespace kessel
