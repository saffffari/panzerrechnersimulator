import test from "node:test";
import assert from "node:assert/strict";
import { playbackStep, sectionSlide } from "./editor-time";
test("slow motion advances every display frame while preserving elapsed game time", () => {
  for (const speed of [1, 0.5, 0.1, 0.01]) {
    const step = playbackStep(1 / 60, speed);
    let acc = 0,
      time = 0,
      moving = 0;
    for (let f = 0; f < 120; f++) {
      acc += speed / 60;
      let moved = false;
      while (acc + step * 1e-5 >= step) {
        time += step;
        acc = Math.max(0, acc - step);
        moved = true;
      }
      if (moved) moving++;
    }
    assert.equal(moving, 120);
    assert.ok(Math.abs(time - speed * 2) < 1e-10);
  }
});
test("section center projects translation onto its normal with an end-on fallback", () => {
  assert.equal(sectionSlide(0.05, 0, 0.1, 0), 0.5);
  assert.equal(sectionSlide(0.05, 0, -0.1, 0), -0.5);
  assert.equal(sectionSlide(0, 0.05, 0.1, 0), 0);
  assert.ok(Math.abs(sectionSlide(0, -0.1, 0, 0) - 0.6) < 1e-10);
});
