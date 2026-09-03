#include "kessel_game.h"

#include <math.h>

namespace kessel {
namespace {

constexpr float kPi = 3.14159265358979323846f;
constexpr float kTau = 2.0f * kPi;
constexpr float kTopSpeed = 12.0f;
constexpr float kAcceleration = 6.0f;
constexpr float kTurnRate = 0.70f;
constexpr float kTraverseRate = 0.10472f;  // Tiger hydraulic baseline: 6 deg/s.
constexpr float kElevationRate = 0.06981f; // 4 deg/s.
constexpr float kEnemyTraverse = 0.80f;
constexpr float kGunMin = -0.13963f;
constexpr float kGunMax = 0.26180f;
constexpr float kReloadSeconds = 3.8f;

struct Solid {
  float x;
  float z;
  float radius;
};

// The same small authored ruin lane used by the renderer.  Circles keep the
// collision pass deterministic and cheap on the Cortex-M7.
constexpr Solid kSolids[] = {
    {39.0f, -15.0f, 6.0f}, {37.0f, 15.0f, 7.0f}, {14.0f, -20.0f, 5.5f},
    {9.0f, 18.0f, 6.0f},   {-16.0f, -15.0f, 6.5f}, {-21.0f, 17.0f, 5.0f},
    {-43.0f, -18.0f, 5.5f}, {-45.0f, 16.0f, 6.0f},
};

float clampf(float value, float lo, float hi) {
  return value < lo ? lo : (value > hi ? hi : value);
}

float approach(float value, float target, float step) {
  const float delta = target - value;
  if (delta > step) return value + step;
  if (delta < -step) return value - step;
  return target;
}

}  // namespace

Game::Game() { reset(); }

void Game::reset() {
  state_ = GameState{};
  state_.mode = RunMode::Title;
  state_.station = Station::Commander;
  state_.breech = Breech::Loaded;
  state_.reserve_rounds = 12;
  state_.armor_integrity = 3;
  state_.player.x = 56.0f;
  state_.player.z = 0.0f;
  state_.player.heading = -kPi * 0.5f;  // West.  That is the only order.

  const EnemyState spawn[kEnemyCount] = {
      {27.0f, -7.0f, kPi * 0.52f, 0.0f, 2.0f, 0.0f, true, false},
      {5.0f, 13.0f, -kPi * 0.45f, 0.0f, 4.5f, 0.0f, true, false},
      {-19.0f, -9.0f, kPi * 0.55f, 0.0f, 6.0f, 0.0f, true, false},
      {-42.0f, 5.0f, -kPi * 0.5f, 0.0f, 8.0f, 0.0f, true, false},
  };
  for (int i = 0; i < kEnemyCount; ++i) state_.enemies[i] = spawn[i];
}

float Game::wrap_angle(float angle) {
  while (angle > kPi) angle -= kTau;
  while (angle < -kPi) angle += kTau;
  return angle;
}

float Game::angle_delta(float target, float current) {
  return wrap_angle(target - current);
}

uint32_t Game::hash(uint32_t value) {
  uint32_t x = value * 2654435761u;
  x ^= x >> 15;
  x *= 2246822519u;
  x ^= x >> 13;
  return x;
}

float Game::hash01(uint32_t value) {
  return static_cast<float>(hash(value) & 0x00ffffffu) / 16777216.0f;
}

bool Game::collides(float x, float z) const {
  if (x < -63.0f || x > 63.0f || z < -31.0f || z > 31.0f) return true;
  for (const Solid& solid : kSolids) {
    const float dx = x - solid.x;
    const float dz = z - solid.z;
    const float r = solid.radius + 2.1f;
    if (dx * dx + dz * dz < r * r) return true;
  }
  return false;
}

void Game::update(const InputFrame& input, float dt) {
  dt = clampf(dt, 0.0f, 0.05f);
  ++state_.tick;

  if (state_.mode == RunMode::Title) {
    if (input.work_pressed) {
      const uint32_t next_tick = state_.tick;
      reset();
      state_.tick = next_tick;
      state_.mode = RunMode::Playing;
    }
    return;
  }
  if (state_.mode == RunMode::Victory || state_.mode == RunMode::Defeat) {
    if (input.work_pressed) reset();
    return;
  }

  state_.mission_time += dt;
  state_.player_muzzle_flash = approach(state_.player_muzzle_flash, 0.0f, dt);
  state_.impact_flash = approach(state_.impact_flash, 0.0f, dt);
  for (int i = 0; i < kEnemyCount; ++i) {
    state_.enemies[i].muzzle_flash = approach(state_.enemies[i].muzzle_flash, 0.0f, dt);
  }

  if (input.map_pressed) state_.map_open = !state_.map_open;
  if (input.station_pressed && !state_.map_open) {
    if (state_.station == Station::Commander) state_.station = Station::Gunner;
    else if (state_.station == Station::Gunner) state_.station = Station::Loader;
    else state_.station = Station::Commander;
  }

  if (!state_.map_open) update_player(input, dt);
  update_enemies(dt);

  if (state_.player.x <= -58.0f) state_.mode = RunMode::Victory;
  if (state_.armor_integrity <= 0) state_.mode = RunMode::Defeat;
}

void Game::update_player(const InputFrame& input, float dt) {
  TankState& tank = state_.player;

  if (state_.station == Station::Commander) {
    const float target = -input.y * kTopSpeed;
    tank.speed = approach(tank.speed, target, kAcceleration * dt);
    tank.heading = wrap_angle(tank.heading + input.x * kTurnRate * dt);
  } else {
    // Nobody is at the controls.  The heavy tank coasts down instead of stopping
    // magically, and the travel lock makes the player wait before firing.
    tank.speed = approach(tank.speed, 0.0f, kAcceleration * 0.42f * dt);
  }

  const float old_x = tank.x;
  const float old_z = tank.z;
  tank.x += sinf(tank.heading) * tank.speed * dt;
  tank.z += cosf(tank.heading) * tank.speed * dt;
  if (collides(tank.x, tank.z)) {
    tank.x = old_x;
    tank.z = old_z;
    tank.speed *= -0.15f;
  }

  if (state_.station == Station::Gunner) {
    tank.turret_yaw = wrap_angle(tank.turret_yaw + input.x * kTraverseRate * dt);
    tank.gun_pitch = clampf(tank.gun_pitch - input.y * kElevationRate * dt, kGunMin, kGunMax);
    if (input.work_pressed) fire_player();
  } else if (state_.station == Station::Loader) {
    if (state_.breech == Breech::Spent && state_.reserve_rounds > 0 && input.work_held) {
      state_.reload_progress += dt / kReloadSeconds;
      if (state_.reload_progress >= 1.0f) {
        --state_.reserve_rounds;
        state_.reload_progress = 0.0f;
        state_.breech = Breech::Loaded;
      }
    } else if (!input.work_held) {
      // Losing the round is deliberately costly: progress bleeds away if the
      // loader lets go, preserving Kessel's physical-workload rhythm.
      state_.reload_progress = clampf(state_.reload_progress - dt * 0.18f, 0.0f, 1.0f);
    }
  }
}

void Game::fire_player() {
  TankState& tank = state_.player;
  state_.last_shot_bounced = false;
  if (state_.breech != Breech::Loaded || fabsf(tank.speed) >= 0.6f) return;

  state_.breech = Breech::Spent;
  state_.player_muzzle_flash = 0.16f;
  state_.reload_progress = 0.0f;

  const float gun_heading = wrap_angle(tank.heading + tank.turret_yaw);
  int target = -1;
  float target_range = 100000.0f;
  float target_error = 10.0f;
  for (int i = 0; i < kEnemyCount; ++i) {
    EnemyState& enemy = state_.enemies[i];
    if (!enemy.alive) continue;
    const float dx = enemy.x - tank.x;
    const float dz = enemy.z - tank.z;
    const float range = sqrtf(dx * dx + dz * dz);
    const float bearing = atan2f(dx, dz);
    const float error = fabsf(angle_delta(bearing, gun_heading));
    const float angular_half_width = atanf(2.5f / (range > 1.0f ? range : 1.0f));
    if (error < angular_half_width && range < target_range) {
      target = i;
      target_range = range;
      target_error = error;
    }
  }

  if (target < 0) return;
  EnemyState& enemy = state_.enemies[target];
  enemy.alerted = true;

  // Compact form of the desktop PzGr.39 model: velocity decay and the
  // DeMarre 1.43 exponent are retained.  The T-34 glacis receives an
  // overmatch-aware effective thickness; a marginal result is deterministic.
  const float striking_v = 773.0f * expf(-1.30e-4f * target_range);
  const float reference_v = 773.0f * expf(-1.30e-4f * 100.0f);
  const float penetration = 138.0f * powf(striking_v / reference_v, 1.43f);
  const float approach_angle = fabsf(angle_delta(atan2f(tank.x - enemy.x, tank.z - enemy.z), enemy.heading));
  const bool side_hit = approach_angle > 0.78f;
  const float effective_armor = side_hit ? 54.0f : 88.0f;
  const float margin = penetration / effective_armor;
  const float chance = clampf(0.5f + (margin - 1.0f) * 2.0f, 0.08f, 0.98f);
  const bool penetrated = hash01(state_.tick ^ static_cast<uint32_t>(target * 977)) < chance;
  (void)target_error;

  if (penetrated) {
    enemy.alive = false;
    ++state_.kills;
    state_.impact_flash = 0.22f;
  } else {
    state_.last_shot_bounced = true;
    state_.impact_flash = 0.10f;
  }
}

void Game::update_enemies(float dt) {
  for (int i = 0; i < kEnemyCount; ++i) {
    EnemyState& enemy = state_.enemies[i];
    if (!enemy.alive) continue;
    if (enemy.reload > 0.0f) enemy.reload -= dt;

    const float dx = state_.player.x - enemy.x;
    const float dz = state_.player.z - enemy.z;
    const float range = sqrtf(dx * dx + dz * dz);
    if (range < 74.0f) enemy.alerted = true;
    if (!enemy.alerted) continue;

    const float bearing = atan2f(dx, dz);
    const float desired = angle_delta(bearing, enemy.heading);
    const float error = angle_delta(desired, enemy.turret_yaw);
    const float step = kEnemyTraverse * dt;
    enemy.turret_yaw = wrap_angle(enemy.turret_yaw + clampf(error, -step, step));
    if (range < 82.0f && fabsf(error) < 0.045f && enemy.reload <= 0.0f) {
      fire_enemy(i, range);
    }
  }
}

void Game::fire_enemy(int index, float range) {
  EnemyState& enemy = state_.enemies[index];
  enemy.reload = 5.0f + hash01(state_.tick + static_cast<uint32_t>(index * 31)) * 3.0f;
  enemy.muzzle_flash = 0.12f;

  // The 76 mm is usually defeated by the Tiger's front.  Exposure, range and
  // deterministic dispersion still make a flank lethal over time.
  const float to_enemy = atan2f(enemy.x - state_.player.x, enemy.z - state_.player.z);
  const float player_aspect = fabsf(angle_delta(to_enemy, state_.player.heading));
  const bool side = player_aspect > 0.72f;
  float hit_chance = side ? 0.44f : 0.22f;
  hit_chance *= clampf(1.15f - range / 115.0f, 0.25f, 1.0f);
  const bool hit = hash01(state_.tick ^ static_cast<uint32_t>(index * 4099 + 17)) < hit_chance;
  const bool penetrated = side || hash01(state_.tick ^ static_cast<uint32_t>(index * 8191 + 91)) < 0.10f;
  if (hit && penetrated) {
    --state_.armor_integrity;
    state_.impact_flash = 0.35f;
  } else if (hit) {
    state_.last_shot_bounced = true;
    state_.impact_flash = 0.14f;
  }
}

}  // namespace kessel

