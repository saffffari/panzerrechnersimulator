// Pure simulation: no Three.js, DOM, camera, or renderer dependencies.
import data from "./canyon-road.json";
import { penetration, type Round } from "./preset";
export type Point = { x: number; z: number };
export type Obstacle = Point & { radius: number; s: number; lane: number };
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
const distance = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.z - b.z);
export class Road {
  readonly min = -40;
  readonly max = 1120;
  readonly halfWidth = 28;
  readonly nativeStart = data.native_start;
  readonly points = data.points;
  sample(s: number, lane = 0) {
    s = clamp(s, this.min, this.max);
    let i = 0;
    while (i < this.points.length - 2 && this.points[i + 1][0] < s) i++;
    const a = this.points[i],
      b = this.points[i + 1],
      t = (s - a[0]) / (b[0] - a[0]);
    const dx = b[1] - a[1],
      dz = b[2] - a[2],
      length = Math.hypot(dx, dz);
    return {
      x: a[1] + dx * t + (dz / length) * lane,
      z: a[2] + dz * t - (dx / length) * lane,
      heading: Math.atan2(dx, dz),
    };
  }
  project(p: Point) {
    let best = { s: 0, lane: 0, distance: Infinity };
    for (let i = 0; i < this.points.length - 1; i++) {
      const a = this.points[i],
        b = this.points[i + 1],
        dx = b[1] - a[1],
        dz = b[2] - a[2],
        len = Math.hypot(dx, dz);
      const t = clamp(
        ((p.x - a[1]) * dx + (p.z - a[2]) * dz) / (len * len),
        0,
        1,
      );
      const ex = p.x - (a[1] + dx * t),
        ez = p.z - (a[2] + dz * t),
        d = Math.hypot(ex, ez);
      if (d < best.distance)
        best = {
          s: a[0] + (b[0] - a[0]) * t,
          lane: (ex * dz - ez * dx) / len,
          distance: d,
        };
    }
    return best;
  }
  clear(p: Point, radius = 2.6) {
    const q = this.project(p);
    return (
      q.distance < this.halfWidth - radius &&
      q.s > this.min + radius &&
      q.s < this.max - radius
    );
  }
}
export const road = new Road();
export const obstacles: Obstacle[] = [
  { s: 285, lane: 7, radius: 3 },
  { s: 560, lane: -6, radius: 3.5 },
  { s: 825, lane: 7, radius: 3 },
].map((o) => ({ ...o, ...road.sample(o.s, o.lane) }));
export function lineOfSight(a: Point, b: Point) {
  const d = distance(a, b);
  if (d > 240) return false;
  for (const o of obstacles) {
    const dx = b.x - a.x,
      dz = b.z - a.z,
      t = clamp(((o.x - a.x) * dx + (o.z - a.z) * dz) / (d * d || 1), 0, 1);
    if (Math.hypot(a.x + dx * t - o.x, a.z + dz * t - o.z) < o.radius)
      return false;
  }
  for (let i = 1; i <= 16; i++) {
    const t = i / 17;
    if (!road.clear({ x: a.x + (b.x - a.x) * t, z: a.z + (b.z - a.z) * t }, 0))
      return false;
  }
  return true;
}
export type Observation = {
  point: Point;
  time: number;
  source: "visual" | "hearing" | "contact";
  observer: number;
};
export type Enemy = Point & {
  id: number;
  s: number;
  lane: number;
  heading: number;
  hp: number;
  role: "pressure" | "left flank" | "right flank";
  cooldown: number;
  lastVisual: number;
};
export type Memory = {
  point: Point;
  time: number;
  source: "initial" | "visual" | "hearing" | "contact";
  observer: number;
};
export class Squad {
  enemies: Enemy[] = [];
  memory: Memory;
  decisions = 0;
  private thinkClock = 0;
  private goalS = 0;
  private searching = false;
  constructor(count: number, initialKnown: Point) {
    this.memory = {
      point: { ...initialKnown },
      time: 0,
      source: "initial",
      observer: -1,
    };
    this.goalS = road.project(initialKnown).s;
    for (let i = 0; i < clamp(Math.floor(count), 1, 10); i++) {
      const s = 1000 + i * 9,
        lane = [0, -5.5, 5.5][i % 3],
        p = road.sample(s, lane);
      this.enemies.push({
        ...p,
        id: i,
        s,
        lane,
        hp: 100,
        heading: p.heading + Math.PI,
        role: (["pressure", "left flank", "right flank"] as const)[i % 3],
        cooldown: 3 + i * 0.45,
        lastVisual: -Infinity,
      });
    }
  }
  observe(observation: Observation) {
    if (
      !this.enemies.some((e) => e.id === observation.observer && e.hp > 0) ||
      !Number.isFinite(observation.point.x) ||
      !Number.isFinite(observation.point.z) ||
      observation.time < this.memory.time
    )
      return;
    this.memory = { ...observation, point: { ...observation.point } };
    this.goalS = road.project(observation.point).s;
    if (observation.source === "visual" || observation.source === "contact")
      this.enemies[observation.observer].lastVisual = observation.time;
  }
  // Deliberately no player argument. Steering only has shared, legitimately sourced memory.
  step(dt: number, time: number) {
    this.thinkClock -= dt;
    if (this.thinkClock <= 0) {
      this.thinkClock = 0.2;
      this.decisions++;
      this.searching = this.enemies.some(
        (e) => e.hp > 0 && Math.abs(e.s - this.goalS) < 18,
      );
    }
    for (const e of this.enemies) {
      if (e.hp <= 0) continue;
      e.cooldown = Math.max(0, e.cooldown - dt);
      let goal = this.goalS;
      // Search a bounded sector around the last known road point; no live coordinates leak in.
      if (this.searching && time - this.memory.time > 3)
        goal = clamp(
          this.goalS + Math.sin(time * 0.12 + e.id * 1.8) * 32,
          road.min + 8,
          road.max - 8,
        );
      const sign = Math.sign(goal - e.s);
      let speed = Math.min(8, Math.abs(goal - e.s) * 0.8);
      let laneGoal =
        e.role === "left flank" ? -9 : e.role === "right flank" ? 9 : 0;
      for (const other of this.enemies)
        if (
          other !== e &&
          other.hp > 0 &&
          sign * (other.s - e.s) > 0 &&
          sign * (other.s - e.s) < 10 &&
          Math.abs(other.lane - e.lane) < 4
        )
          speed = Math.min(speed, Math.max(0, Math.abs(other.s - e.s) - 5));
      for (const o of obstacles)
        if (
          Math.abs(o.s - e.s) < 22 &&
          Math.abs(e.lane - o.lane) < o.radius + 4
        )
          laneGoal = o.lane > 0 ? -3 : 3;
      const nextLane = e.lane + clamp(laneGoal - e.lane, -dt * 3, dt * 3),
        nextS = clamp(e.s + sign * speed * dt, road.min + 5, road.max - 5);
      const p = road.sample(nextS, nextLane);
      if (!obstacles.some((o) => distance(p, o) < o.radius + 2.8)) {
        e.s = nextS;
        e.lane = nextLane;
        e.x = p.x;
        e.z = p.z;
        if (speed > 0.1) e.heading = p.heading + (sign < 0 ? Math.PI : 0);
      }
    }
  }
}
export type Controls = { forward: number; turn: number };
export type CombatEvent = {
  from: Point;
  to: Point;
  hit: boolean;
  enemy: number;
  player: boolean;
};
export class Battle {
  squad: Squad;
  player = { x: 0, z: 0, heading: Math.PI, hp: 100, speed: 0 };
  time = 0;
  private sensorClock = 0;
  events: CombatEvent[] = [];
  shots = 0;
  playerCooldown = 0;
  constructor(count = 5) {
    this.squad = new Squad(count, { x: 0, z: 0 });
  }
  restart(count: number) {
    this.squad = new Squad(count, { x: 0, z: 0 });
    this.player = { x: 0, z: 0, heading: Math.PI, hp: 100, speed: 0 };
    this.time = 0;
    this.events = [];
    this.shots = 0;
    this.playerCooldown = 0;
    this.sensorClock = 0;
  }
  // Sensing is the only gateway allowed to compare true player coordinates with enemies.
  private sense() {
    for (const e of this.squad.enemies) {
      if (e.hp <= 0) continue;
      const d = distance(e, this.player),
        bearing = Math.atan2(this.player.x - e.x, this.player.z - e.z),
        angle = Math.atan2(
          Math.sin(bearing - e.heading),
          Math.cos(bearing - e.heading),
        );
      if (d < 7)
        this.squad.observe({
          point: { x: this.player.x, z: this.player.z },
          time: this.time,
          source: "contact",
          observer: e.id,
        });
      else if (
        d < 180 &&
        Math.abs(angle) < Math.PI / 3 &&
        lineOfSight(e, this.player)
      )
        this.squad.observe({
          point: { x: this.player.x, z: this.player.z },
          time: this.time,
          source: "visual",
          observer: e.id,
        });
    }
  }
  step(dt: number, input: Controls) {
    dt = clamp(dt, 0, 1 / 30);
    this.time += dt;
    this.playerCooldown = Math.max(0, this.playerCooldown - dt);
    if (this.player.hp > 0) {
      this.player.heading += input.turn * dt * 0.65;
      this.player.speed +=
        (input.forward * 8 - this.player.speed) * Math.min(1, dt * 3);
      const p = {
        x:
          this.player.x +
          Math.sin(this.player.heading) * this.player.speed * dt,
        z:
          this.player.z +
          Math.cos(this.player.heading) * this.player.speed * dt,
      };
      if (
        road.clear(p) &&
        !obstacles.some((o) => distance(p, o) < o.radius + 2.8) &&
        !this.squad.enemies.some((e) => e.hp > 0 && distance(p, e) < 4)
      ) {
        this.player.x = p.x;
        this.player.z = p.z;
      } else this.player.speed = 0;
    }
    this.sensorClock -= dt;
    if (this.sensorClock <= 0) {
      this.sensorClock = 0.2;
      this.sense();
    }
    const previous = this.squad.enemies.map((e) => ({
      x: e.x,
      z: e.z,
      s: e.s,
      lane: e.lane,
    }));
    this.squad.step(dt, this.time);
    // World contact resolution may compare true positions, but steering never receives them.
    for (const e of this.squad.enemies)
      if (e.hp > 0 && distance(e, this.player) < 4) {
        this.squad.observe({
          point: { x: this.player.x, z: this.player.z },
          time: this.time,
          source: "contact",
          observer: e.id,
        });
        Object.assign(e, previous[e.id]);
      }
    for (const e of this.squad.enemies)
      if (
        e.hp > 0 &&
        e.cooldown <= 0 &&
        this.time - e.lastVisual < 0.3 &&
        this.player.hp > 0
      ) {
        // Aim at observed memory; never calculate fire direction from live coordinates.
        const target = this.squad.memory.point;
        const hit =
          distance(target, this.player) < 3.5 && lineOfSight(e, target);
        e.cooldown = 5 + e.id * 0.13;
        if (hit) this.player.hp = Math.max(0, this.player.hp - 12);
        this.event({
          from: { x: e.x, z: e.z },
          to: { ...target },
          hit,
          enemy: e.id,
          player: false,
        });
      }
  }
  event(e: CombatEvent) {
    if (this.events.length >= 32) this.events.shift();
    this.events.push(e);
  }
  fire(target: Point, r: Round) {
    if (this.player.hp <= 0 || this.playerCooldown > 0) return undefined;
    this.shots++;
    this.playerCooldown = 1.2;
    // A gunshot can be heard within 350m; it shares only that actual firing location.
    for (const e of this.squad.enemies)
      if (e.hp > 0 && distance(e, this.player) <= 350)
        this.squad.observe({
          point: { x: this.player.x, z: this.player.z },
          time: this.time,
          source: "hearing",
          observer: e.id,
        });
    const dx = target.x - this.player.x,
      dz = target.z - this.player.z,
      length = Math.hypot(dx, dz);
    let nearest: Enemy | undefined,
      near = Infinity;
    for (const e of this.squad.enemies) {
      if (e.hp <= 0) continue;
      const t =
        ((e.x - this.player.x) * dx + (e.z - this.player.z) * dz) /
        (length * length || 1);
      const d = Math.hypot(
        this.player.x + dx * t - e.x,
        this.player.z + dz * t - e.z,
      );
      if (t > 0 && d < 3 && t * length < near && lineOfSight(this.player, e)) {
        near = t * length;
        nearest = e;
      }
    }
    if (nearest) {
      const p = penetration(r, near);
      nearest.hp = Math.max(0, nearest.hp - (p >= 60 ? 60 : 15));
    }
    const end = nearest ? { x: nearest.x, z: nearest.z } : target;
    const event = {
      from: { x: this.player.x, z: this.player.z },
      to: { ...end },
      hit: !!nearest,
      enemy: nearest?.id ?? -1,
      player: true,
    };
    this.event(event);
    return event;
  }
  get outcome() {
    return this.player.hp <= 0
      ? "Player disabled"
      : this.squad.enemies.every((e) => e.hp <= 0)
        ? "All T-34s disabled"
        : "Hunt in progress";
  }
}
