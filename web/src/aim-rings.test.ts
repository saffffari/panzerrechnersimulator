import test from "node:test";
import assert from "node:assert/strict";
import { Ray, Vector3 as V } from "three";
import { discHit, rotateOnRing, ringNormal } from "./aim-rings";
test("ring picking accepts the filled disc, rejects outside and rays behind the eye", () => {
  const c = new V();
  assert.ok(
    discHit(
      new Ray(new V(0.2, 3, 0.1), new V(0, -1, 0)),
      c,
      new V(0, 1, 0),
      0.85,
    ),
  );
  assert.equal(
    discHit(new Ray(new V(1, 3, 0), new V(0, -1, 0)), c, new V(0, 1, 0), 0.85),
    null,
  );
  assert.equal(
    discHit(new Ray(new V(0, 3, 0), new V(0, 1, 0)), c, new V(0, 1, 0), 0.85),
    null,
  );
});
test("azimuth preserves elevation; elevation rotates only in its selected plane", () => {
  const from = new V(0.8, 0.6, 0),
    c = new V(2, 3, 4),
    az = rotateOnRing("azimuth", c.clone().add(new V(0, 0, 1)), c, from);
  assert.ok(az.distanceTo(new V(0, 0.6, 0.8)) < 1e-10);
  const el = rotateOnRing(
    "elevation",
    c.clone().add(new V(0.5, 0.5, 0)),
    c,
    from,
  );
  assert.ok(el.distanceTo(new V(Math.SQRT1_2, Math.SQRT1_2, 0)) < 1e-10);
  assert.equal(ringNormal("elevation", from).dot(el), 0);
});
