import { chromium } from "playwright";
import fs from "node:fs";
import assert from "node:assert/strict";
const dir = process.env.PANZER_CAPTURE_DIR ?? "/windows/shop/panzerrechnersystem/captures/screenshots",
  stamp = process.env.PANZER_CAPTURE_STAMP ?? new Date().toISOString().replace(/[^0-9]/g, "");
fs.mkdirSync(dir, { recursive: true });
const browser = await chromium.launch({
  executablePath: process.env.PANZER_CHROMIUM ?? "/usr/bin/chromium",
  headless: true,
  args: ["--no-sandbox", "--use-gl=angle", "--use-angle=gl"],
});
const report = { errors: [], checks: [], files: [] };
try {
  const page = await browser.newPage({
    viewport: { width: 1600, height: 1000 },
  });
  page.on("pageerror", (e) => report.errors.push(e.message));
  await page.goto(process.env.PANZER_WEB_URL ?? "http://127.0.0.1:5178/");
  await page.waitForFunction(
    () => window.panzerEditor?.snapshot().ready,
    {},
    { timeout: 60000 },
  );
  await page.waitForTimeout(1200);
  const check = (name, test) => {
    assert.ok(test, name);
    report.checks.push(name);
  };
  let s = await page.evaluate(() => window.panzerEditor.snapshot());
  check(
    "native model present and render draws real triangles",
    s.meshes >= 60 && s.triangles > 10000,
  );
  check("rings hidden initially", !s.ringsVisible);
  check(
    "native saved shots loaded",
    (await page.locator("#shot option").count()) >= 9,
  );
  await page.screenshot({ path: `${dir}/web-editor-final-${stamp}.png` });
  const center = await page.evaluate(() => window.panzerEditor.screenPoint());
  await page.mouse.click(...center);
  check(
    "impact plus reveals the two rings",
    (await page.evaluate(() => window.panzerEditor.snapshot())).ringsVisible,
  );
  await page.screenshot({ path: `${dir}/web-editor-rings-${stamp}.png` });
  const ringPoint = await page.evaluate(() => {
    let p = window.panzerEditor.snapshot().impact;
    p[0] += 0.65;
    p[2] += 0.2;
    return window.panzerEditor.screenPoint(p);
  });
  await page.mouse.move(...ringPoint);
  await page.mouse.down();
  s = await page.evaluate(() => window.panzerEditor.snapshot());
  check("full azimuth disc is clickable", s.activeRing === "azimuth");
  await page.screenshot({ path: `${dir}/web-editor-ring-drag-${stamp}.png` });
  await page.mouse.move(ringPoint[0] - 25, ringPoint[1] + 20, { steps: 5 });
  await page.mouse.up();
  check(
    "azimuth drag changes incoming direction",
    JSON.stringify(
      (await page.evaluate(() => window.panzerEditor.snapshot())).from,
    ) !== JSON.stringify(s.from),
  );
  await page.mouse.click(1500, 100);
  check(
    "click outside hides rings",
    !(await page.evaluate(() => window.panzerEditor.snapshot())).ringsVisible,
  );
  await page.evaluate(() => window.panzerEditor.setSection([1, 0, 0], 0));
  const section = await page.evaluate(
    () => window.panzerEditor.snapshot().section,
  );
  const sc = await page.evaluate(() =>
    window.panzerEditor.screenPoint(
      window.panzerEditor.snapshot().section.center,
    ),
  );
  const sa = await page.evaluate(() => {
    const p = window.panzerEditor.snapshot().section.center;
    p[0] += 1;
    return window.panzerEditor.screenPoint(p);
  });
  await page.mouse.move(...sc);
  await page.mouse.down();
  await page.mouse.move(
    sc[0] + (sa[0] - sc[0]) * 0.5,
    sc[1] + (sa[1] - sc[1]) * 0.5,
    { steps: 5 },
  );
  await page.mouse.up();
  const movedSection = await page.evaluate(
    () => window.panzerEditor.snapshot().section,
  );
  check(
    "section center translates along normal without rotating",
    Math.abs(movedSection.offset - 0.5) < 0.025 &&
      JSON.stringify(movedSection.normal) === JSON.stringify(section.normal),
  );
  await page.screenshot({
    path: `${dir}/web-editor-section-normal-${stamp}.png`,
  });
  await page.click('[data-panel="section"]');
  await page
    .getByRole("button", { name: "CLEAR SECTION", exact: true })
    .click();
  await page.click('[data-panel="section"]');
  await page.evaluate(() => {
    window.panzerEditor.aim([-1.45, 1.4, 0], [-1, 0, 0]);
  });
  await page.selectOption("#shell", "15");
  await page.click("#fire");
  await page.waitForTimeout(500);
  s = await page.evaluate(() => window.panzerEditor.snapshot());
  check("fire resolves on actual model", Object.keys(s.statuses).length > 0);
  await page.click("#reset");
  s = await page.evaluate(() => window.panzerEditor.snapshot());
  check(
    "reset restores paused intact model",
    s.paused && Object.keys(s.statuses).length === 0,
  );
  await page.evaluate(() => {
    window.panzerEditor.setTimeScale(0.1);
    window.panzerEditor.fire();
  });
  const motion = await page.evaluate(async () => {
    const samples = [];
    for (let i = 0; i < 30; i++) {
      await new Promise(requestAnimationFrame);
      const s = window.panzerEditor.snapshot();
      samples.push({ time: s.time, p: s.particle });
    }
    return samples;
  });
  const changes = motion
    .slice(1)
    .filter((v, i) => v.time > motion[i].time).length;
  check(
    "slow motion produces new physical states across display frames",
    changes >= 20 && motion.at(-1).time - motion[0].time < 0.25,
  );
  report.slowMotion = {
    samples: motion.length,
    distinctAdvances: changes,
    elapsedGameTime: motion.at(-1).time - motion[0].time,
  };
  await page.evaluate(() => {
    window.panzerEditor.reset();
    window.panzerEditor.setTimeScale(1);
  });
  const files = await page.evaluate(() =>
    window.panzerEditor.resolutionStudy(),
  );
  for (const f of files) {
    const name = `web-editor-${stamp}-${f.name}`;
    fs.writeFileSync(
      dir + "/" + name,
      Buffer.from(f.data.split(",")[1], "base64"),
    );
    report.files.push(name);
  }
  check(
    "resolution ladder reaches width one",
    files.length === 16 && files.at(-1).name.includes("1x2"),
  );
  await page.evaluate(() => {
    window.panzerEditor.setStatus("Engine", 1);
    window.panzerEditor.setStatus("Gunner", 2);
  });
  const damaged = await page.evaluate(() =>
    window.panzerEditor.capture(540, 960),
  );
  fs.writeFileSync(
    `${dir}/web-editor-damage-colors-${stamp}.png`,
    Buffer.from(damaged.split(",")[1], "base64"),
  );
  await page.click("#reset");
  await page.click("#battle-mode");
  check(
    "battle mode retained",
    (await page.evaluate(() => window.panzerEditor.snapshot())).battleMode,
  );
  await page.keyboard.down("w");
  await page.waitForTimeout(500);
  await page.keyboard.up("w");
  await page.screenshot({ path: `${dir}/web-editor-battle-${stamp}.png` });
  await page.click("#range-mode");
  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${dir}/web-editor-mobile-${stamp}.png` });
  check(
    "mobile horizontal overflow absent",
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  );
  check("no page exceptions", report.errors.length === 0);
  report.final = await page.evaluate(() => window.panzerEditor.snapshot());
  console.log(JSON.stringify(report, null, 2));
  fs.writeFileSync(process.env.PANZER_QA_REPORT ?? "qa-editor-rebuild.json", JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
