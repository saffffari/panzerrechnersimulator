import { test } from "node:test";
import assert from "node:assert/strict";
import { Vector3 as V } from "three";
import { SphereAimDrag, markerWorldDiameter } from "./sphere-aim";
const origin = new V(0, 0, 5),
  center = new V(),
  r = 1;
function onRay(point: V, d: V) {
  assert.ok(
    point.clone().sub(origin).normalize().distanceTo(d.clone().normalize()) <
      1e-8,
  );
}
test("cursor ray intersects exact sphere on existing near/far hemisphere", () => {
  const d = new V(0.12, 0.08, -1);
  for (const z of [1, -1]) {
    const aim = new SphereAimDrag(origin, center, r, new V(0, 0, z));
    const p = aim.project(origin, d, center, r);
    assert.ok(Math.abs(p.length() - 1) < 1e-8);
    onRay(p, d);
    assert.equal(Math.sign(p.z), z);
  }
});
test("off-sphere cursor clamps to true silhouette and returns through opposite hemisphere", () => {
  const aim = new SphereAimDrag(origin, center, r, new V(0, 0, 1)),
    p = aim.project(origin, new V(2, 1, -1), center, r);
  assert.ok(Math.abs(p.length() - 1) < 1e-8);
  assert.ok(Math.abs(p.dot(p.clone().sub(origin))) < 1e-8);
  assert.equal(aim.hemisphere, "far");
  const again = aim.project(origin, new V(3, 1, -1), center, r);
  assert.ok(Math.abs(again.length() - 1) < 1e-8);
  assert.equal(aim.hemisphere, "far");
  const back = aim.project(origin, new V(0, 0, -1), center, r);
  assert.ok(back.distanceTo(new V(0, 0, -1)) < 1e-8);
});
test("projection is independent of intermediate XY movements and stays finite at poles", () => {
  for (const d of [
    new V(0, 0.1, -1),
    new V(0, -0.1, -1),
    new V(0.1, 0, -1),
    new V(-0.1, 0, -1),
  ]) {
    const aim = new SphereAimDrag(origin, center, r, new V(0, 0, 1));
    const p = aim.project(origin, d, center, r);
    onRay(p, d);
    assert.ok(Math.abs(p.length() - 1) < 1e-8);
  }
  const inside = new SphereAimDrag(new V(), center, r, new V(0, 0, 1)).project(
    new V(),
    new V(0, 1, 0),
    center,
    r,
  );
  assert.deepEqual(inside.toArray(), [0, 1, 0]);
});

test("equal marker pixel sizes survive unequal perspective depths", () => {
  for (const depth of [2, 7, 20]) {
    const world = markerWorldDiameter(depth, 43, 768);
    const pixels = (world / (2 * depth * Math.tan((43 * Math.PI) / 360))) * 768;
    assert.ok(Math.abs(pixels - 12) < 1e-8);
  }
});
test("world-offset sphere and arbitrary camera ray preserve the chosen handle hemisphere", () => {
  const c = new V(3, 1, -2),
    o = new V(9, 7, 10);
  for (const axis of [
    new V(1, 0, 0),
    new V(-1, 0, 0),
    new V(0, 1, 0),
    new V(0, -1, 0),
    new V(0, 0, 1),
    new V(0, 0, -1),
  ]) {
    const handle = c.clone().addScaledVector(axis, 0.85),
      aim = new SphereAimDrag(o, c, 0.85, handle),
      p = aim.project(o, handle.clone().sub(o), c, 0.85);
    assert.ok(p.distanceTo(handle) < 1e-7);
  }
});
