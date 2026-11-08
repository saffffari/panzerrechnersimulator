import { test } from "node:test";
import assert from "node:assert/strict";
import { performance } from "node:perf_hooks";
import { Battle, Squad, road, obstacles, lineOfSight } from "./battle";
import { rounds } from "./preset";
const idle = { forward: 0, turn: 0 };
test("clipped native road starts at Canyon player start; exactly 1km along-road spawn", () => {
  assert.deepEqual(road.nativeStart, {
    heading_deg: 180,
    x: -791.016,
    z: 745.783,
  });
  const b = new Battle(10);
  assert.equal(b.squad.enemies.length, 10);
  const first = b.squad.enemies[0];
  assert.equal(first.s, 1000);
  assert.deepEqual(
    { x: first.x, z: first.z },
    { x: road.sample(1000).x, z: road.sample(1000).z },
  );
  assert.ok(Math.hypot(first.x, first.z) < 1000 - 5);
  assert.equal(b.player.heading, Math.PI);
  assert.ok(road.points.every((p) => p[0] >= -40 && p[0] <= 1120));
  assert.equal(new Battle(999).squad.enemies.length, 10);
  assert.equal(new Battle(0).squad.enemies.length, 1);
});
test("initial knowledge copied once; unseen silent relocation never updates memory or steering", () => {
  const a = new Battle(10),
    b = new Battle(10);
  b.player.x = road.sample(80, 15).x;
  b.player.z = road.sample(80, 15).z;
  for (let i = 0; i < 600; i++) {
    a.step(1 / 60, idle);
    b.step(1 / 60, idle);
  }
  assert.deepEqual(a.squad.memory, {
    point: { x: 0, z: 0 },
    time: 0,
    source: "initial",
    observer: -1,
  });
  assert.deepEqual(b.squad.memory, a.squad.memory);
  assert.deepEqual(b.squad.enemies, a.squad.enemies);
  const point = { x: 0, z: 0 },
    squad = new Squad(2, point);
  point.x = 90;
  assert.equal(squad.memory.point.x, 0);
});
test("only real near visual/hearing observations update squad memory, shared by all members", () => {
  const b = new Battle(3),
    far = road.sample(120, 5);
  b.player.x = far.x;
  b.player.z = far.z;
  b.fire(road.sample(130), rounds[12]);
  assert.equal(b.squad.memory.source, "initial");
  b.restart(3);
  const near = road.sample(900);
  b.player.x = near.x;
  b.player.z = near.z;
  b.step(1 / 60, idle);
  assert.equal(b.squad.memory.source, "visual");
  assert.deepEqual(b.squad.memory.point, { x: near.x, z: near.z });
  b.restart(3);
  const audible = road.sample(760);
  b.player.x = audible.x;
  b.player.z = audible.z;
  b.fire(road.sample(770), rounds[12]);
  assert.equal(b.squad.memory.source, "hearing");
  assert.deepEqual(b.squad.memory.point, { x: audible.x, z: audible.z });
  const before = structuredClone(b.squad.memory);
  b.squad.observe({
    point: { x: 100, z: 200 },
    time: -1,
    source: "visual",
    observer: 0,
  });
  assert.deepEqual(b.squad.memory, before);
});
test("canyon bend and rock block sight; no vision behind enemy", () => {
  assert.equal(lineOfSight(road.sample(600), road.sample(1100)), false);
  const o = obstacles[0];
  assert.equal(
    lineOfSight(road.sample(o.s - 15, o.lane), road.sample(o.s + 15, o.lane)),
    false,
  );
  const b = new Battle(1),
    p = road.sample(1100);
  b.player.x = p.x;
  b.player.z = p.z;
  b.step(1 / 60, idle);
  assert.equal(b.squad.memory.source, "initial");
});
test("ten-enemy hunt stays in corridor, avoids obstacles and searches old memory", () => {
  const s = new Squad(10, { x: 0, z: 0 });
  const started = performance.now();
  for (let i = 0; i < 6000; i++) s.step(1 / 30, i / 30);
  assert.ok(s.enemies.some((e) => e.s < 100));
  assert.ok(
    s.enemies.every((e) => road.clear(e) && Number.isFinite(e.heading)),
  );
  assert.ok(
    s.enemies.every(
      (e) =>
        !obstacles.some(
          (o) => Math.hypot(e.x - o.x, e.z - o.z) < o.radius + 2.7,
        ),
    ),
  );
  assert.equal(s.memory.source, "initial");
  assert.deepEqual(s.memory.point, { x: 0, z: 0 });
  assert.ok(s.decisions <= 1001);
  console.log(
    `Squad-only 200 simulated seconds / 10 enemies: ${(performance.now() - started).toFixed(1)}ms CPU, ${s.decisions} decisions`,
  );
});
test("combat damage, disabled enemies, fire cooldown and restart are real state transitions", () => {
  const b = new Battle(1),
    p = road.sample(900);
  b.player.x = p.x;
  b.player.z = p.z;
  const target = { x: b.squad.enemies[0].x, z: b.squad.enemies[0].z };
  const event = b.fire(target, rounds[12]);
  assert.ok(event?.hit);
  assert.equal(b.squad.enemies[0].hp, 40);
  assert.equal(b.fire(target, rounds[12]), undefined);
  for (let i = 0; i < 80; i++) b.step(1 / 60, idle);
  assert.ok(b.fire(target, rounds[12])?.hit);
  assert.equal(b.squad.enemies[0].hp, 0);
  assert.equal(b.outcome, "All T-34s disabled");
  b.restart(7);
  assert.equal(b.squad.enemies.length, 7);
  assert.equal(b.player.hp, 100);
  assert.deepEqual(b.squad.memory.point, { x: 0, z: 0 });
  assert.equal(b.events.length, 0);
});

test("after a real sighting, unseen silent relocation cannot refresh or redirect shared memory", () => {
  const a = new Battle(4),
    b = new Battle(4),
    seen = road.sample(900);
  for (const game of [a, b]) {
    game.player.x = seen.x;
    game.player.z = seen.z;
    game.step(1 / 60, idle);
    assert.equal(game.squad.memory.source, "visual");
  }
  const memory = structuredClone(a.squad.memory);
  Object.assign(a.player, { x: 0, z: 0 });
  const elsewhere = road.sample(60, 15);
  Object.assign(b.player, { x: elsewhere.x, z: elsewhere.z });
  for (let i = 0; i < 600; i++) {
    a.step(1 / 60, idle);
    b.step(1 / 60, idle);
  }
  assert.deepEqual(a.squad.memory, memory);
  assert.deepEqual(b.squad.memory, memory);
  assert.deepEqual(a.squad.enemies, b.squad.enemies);
});

test("bounded full Battle simulation including sensing and combat runs to contact", () => {
  const b = new Battle(10),
    started = performance.now();
  let peakEvents = 0;
  for (let i = 0; i < 7200; i++) {
    b.step(1 / 60, idle);
    peakEvents = Math.max(peakEvents, b.events.length);
  }
  assert.ok(b.squad.enemies.some((e) => e.s < 180));
  assert.notEqual(b.squad.memory.source, "initial");
  assert.ok(peakEvents <= 32);
  assert.ok(b.player.hp < 100);
  console.log(
    `Full Battle 120 simulated seconds / 10 enemies / sensors+combat: ${(performance.now() - started).toFixed(1)}ms CPU; ${b.squad.decisions} squad decisions; peak ${peakEvents}/32 events`,
  );
});
