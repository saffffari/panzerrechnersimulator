import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readCurrentPreset } from "../tools/preset-source";
test("explicit native preset refreshes from disk, fallback on missing or invalid file", () => {
  const dir = mkdtempSync(join(tmpdir(), "panzer-preset-test-"));
  try {
    const native = join(dir, "native.json"),
      baseline = join(dir, "baseline.json");
    const settings = {
      shots: [
        { name: "Shot 1", shot: { shell: 17, target: { point: [1, 2, 3] } } },
      ],
      selected_shot: 0,
    };
    writeFileSync(baseline, JSON.stringify({ name: "Baseline", settings: {} }));
    assert.equal(
      readCurrentPreset(native, new URL(`file://${baseline}`)).source,
      "baseline",
    );
    writeFileSync(native, JSON.stringify({ name: "Saved", settings }));
    assert.deepEqual(
      JSON.parse(readCurrentPreset(native, new URL(`file://${baseline}`)).text)
        .settings,
      settings,
    );
    writeFileSync(native, JSON.stringify({ name: "Saved again", settings }));
    assert.equal(
      JSON.parse(readCurrentPreset(native, new URL(`file://${baseline}`)).text)
        .name,
      "Saved again",
    );
    writeFileSync(native, "bad json");
    assert.equal(
      readCurrentPreset(native, new URL(`file://${baseline}`)).source,
      "baseline",
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
