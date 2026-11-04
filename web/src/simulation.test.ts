import { test } from "node:test";
import assert from "node:assert/strict";
import { Vector3 as V } from "three";
import { Simulation } from "./simulation";
import { rounds, parsePresets, penetration } from "./preset";
test("native preset wrappers and catalogue indices", () => {
  assert.equal(rounds[12].id, "PZGR_39");
  assert.equal(rounds[4].mass_kg, 6.5);
  assert.equal(
    parsePresets({
      presets: [{ name: "Saved", settings: { shot: { shell: 4 } } }],
    }).length,
    1,
  );
  assert.equal(parsePresets({ name: "bad" }).length, 0);
  assert.ok(penetration(rounds[12], 100) > 120);
});
test("localized AP tunnel, deformation and crew breakup", () => {
  const s = new Simulation();
  const before = s.cells.length;
  const r = s.fire(
    new V(-1.7, 1.5, 0.2),
    new V(1, 0, 0),
    new V(-1, 0, 0),
    rounds[12],
    100,
  );
  assert.ok(r.passed);
  assert.ok(r.lost > 0 && r.lost < before / 3);
  assert.ok(r.crew > 0);
  assert.ok(s.cells.some((c) => c.alive && c.p.distanceTo(c.origin) > 0.001));
  assert.ok(s.particles.some((p) => p.kind === 2));
});
test("grazing hit stops and reset restores damage", () => {
  const s = new Simulation();
  const r = s.fire(
    new V(-1.7, 1.5, 0),
    new V(0.01, 0, 1).normalize(),
    new V(-1, 0, 0),
    rounds[4],
    400,
  );
  assert.equal(r.passed, false);
  s.reset();
  assert.equal(s.removed, 0);
  assert.ok(s.cells.every((c) => c.alive));
});
test("blood has a single wall contact; pools remain bounded under repeated extreme shots", () => {
  const s = new Simulation();
  s.emit(new V(1.49, 1.4, 0), new V(10, 0, 0), 2, 0.05, 2);
  s.step(1 / 60);
  assert.equal(s.particles.length, 0);
  assert.equal(s.splats.length, 1);
  s.step(1 / 60);
  assert.equal(s.splats.length, 1);
  for (let i = 0; i < 25; i++)
    s.fire(
      new V(-1.6, 1.5, 0),
      new V(1, 0, 0),
      new V(-1, 0, 0),
      rounds[17],
      100,
    );
  assert.ok(s.particles.length <= 1200);
  for (let i = 0; i < 600; i++) s.step(1 / 120);
  assert.ok(s.splats.length <= 400);
  assert.ok(s.particles.every((p) => Number.isFinite(p.p.y)));
});
