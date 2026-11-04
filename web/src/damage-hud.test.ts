import { test } from "node:test";
import assert from "node:assert/strict";
import { Simulation, crewSeats } from "./simulation";
import { summarizeDamage, damageHudMarkup } from "./damage-hud";
test("five unlabeled silhouettes start intact; actual regional voxel loss drives red and KIA X", () => {
  const s = new Simulation();
  let state = summarizeDamage(s.cells);
  assert.equal(state.crew.length, 5);
  assert.ok(
    state.crew.every((c) => !c.dead && !Object.values(c.regions).some(Boolean)),
  );
  const id = 2;
  for (const c of s.cells)
    if (c.crew === id && c.origin.y - crewSeats[id].at[1] > 0.42)
      c.alive = false;
  state = summarizeDamage(s.cells);
  assert.equal(state.crew[id].regions.head, true);
  assert.equal(state.crew[id].dead, true);
  assert.equal(state.crew[0].dead, false);
  const markup = damageHudMarkup(state);
  assert.ok(markup.includes("#e35847"));
  assert.ok(markup.includes('stroke="#050505"'));
  assert.ok(!markup.includes("<text"));
});
test("hull/turret status follows only real cell damage; static tracks stay explicitly unknown", () => {
  const s = new Simulation();
  const cell = s.cells.find((c) => c.kind === 1)!;
  cell.alive = false;
  const state = summarizeDamage(s.cells);
  assert.equal(state.modules.turret, true);
  assert.equal(state.modules.front, false);
  assert.ok(
    damageHudMarkup(state).includes("Gray tracks are static, unmodeled"),
  );
  s.reset();
  assert.ok(Object.values(summarizeDamage(s.cells).modules).every((x) => !x));
});
