import { chromium } from "playwright";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
// Never open a browser QA context while any native range editor is running.
const busy = () =>
  execFileSync("ps", ["-eo", "comm,args"], { encoding: "utf8" })
    .split("\n")
    .some(
      (l) =>
        /^panzer\S*\s+.*fea-range/.test(l) ||
        /^panzer\S*\s+.*editor-stable\/target/.test(l),
    );
if (busy()) throw Error("Native range editor open: browser GPU QA deferred");
const browser = await chromium.launch({
  headless: process.env.HEADED !== "1",
  executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
  args: [
    "--no-sandbox",
    ...(process.env.HEADED === "1" ? ["--use-gl=angle", "--use-angle=gl"] : []),
  ],
});
const monitor = setInterval(() => {
  if (busy()) {
    browser.close();
    console.error("Native editor opened; QA browser closed");
    process.exitCode = 2;
  }
}, 500);
try {
  const page = await browser.newPage({
      viewport: { width: 1366, height: 768 },
    }),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("http://127.0.0.1:5178");
  await page.waitForFunction(() => window.panzerQA?.snapshot().cells > 0);
  await page.waitForTimeout(1200);
  const gpuMode = process.env.HEADED === "1" ? "headed" : "headless";
  const capture = "/windows/shop/panzerrechnersystem/captures/screenshots";
  fs.mkdirSync(capture, { recursive: true });
  await page.screenshot({
    path: `${capture}/web-playground-${gpuMode}-baseline.png`,
  });
  const baseline = await page.evaluate(() => window.panzerQA.snapshot());
  const point = await page.evaluate(() =>
    window.panzerQA.project([-1.7, 1.5, 0.2]),
  );
  await page.mouse.click(point.x, point.y);
  const clicked = await page.evaluate(() => window.panzerQA.snapshot());
  if (clicked.shots !== 0) throw Error("Click fired");
  await page.mouse.move(point.x, point.y);
  await page.mouse.down();
  await page.waitForTimeout(230);
  await page.mouse.move(point.x + 80, point.y - 65, { steps: 10 });
  await page.mouse.up();
  const aimed = await page.evaluate(() => window.panzerQA.snapshot());
  if (
    aimed.shots !== 0 ||
    JSON.stringify(aimed.from) === JSON.stringify(clicked.from)
  )
    throw Error("Hold sphere aiming failed");
  // Drag around both hemispheres, including through a pole, without firing.
  await page.mouse.down();
  await page.waitForTimeout(200);
  await page.mouse.move(point.x + 380, point.y + 160, { steps: 20 });
  await page.mouse.up();
  const around = await page.evaluate(() => window.panzerQA.snapshot());
  if (Math.abs(Math.hypot(...around.from) - 1) > 0.00001 || around.shots !== 0)
    throw Error("Aim escaped sphere");
  const cameraBefore = around.camera;
  await page.mouse.move(1000, 500);
  await page.mouse.down({ button: "right" });
  await page.mouse.move(1100, 550, { steps: 10 });
  await page.mouse.up({ button: "right" });
  await page.waitForTimeout(250);
  const navigated = await page.evaluate(() => window.panzerQA.snapshot());
  if (
    JSON.stringify(navigated.camera) === JSON.stringify(cameraBefore) ||
    navigated.shots !== 0
  )
    throw Error("Camera control conflict");
  await page.selectOption("#shot", "p:0");
  await page.click("#fire");
  await page.waitForTimeout(200);
  await page.screenshot({
    path: `${capture}/web-playground-${gpuMode}-ap-impact.png`,
  });
  const damaged = await page.evaluate(() => window.panzerQA.snapshot());
  if (damaged.shots !== 1 || damaged.removed <= 0) throw Error("No AP damage");
  await page.selectOption("#step", "0.004166667");
  await page.keyboard.press("F");
  await page.waitForTimeout(1500);
  await page.selectOption("#shot", "r:16");
  await page.click("#fire");
  await page.waitForTimeout(250);
  await page.screenshot({
    path: `${capture}/web-playground-${gpuMode}-gustav.png`,
  });
  await page.selectOption("#shot", "r:17");
  for (let i = 0; i < 12; i++) await page.keyboard.press("F");
  await page.waitForTimeout(2000);
  const stress = await page.evaluate(() => window.panzerQA.snapshot());
  if (stress.shots !== 15)
    throw Error(`Expected 15 explicit shots, got ${stress.shots}`);
  if (stress.particles > 1200 || stress.splats > 400)
    throw Error("Unbounded FX");
  await page.keyboard.press("Backspace");
  await page.waitForTimeout(100);
  const reset = await page.evaluate(() => window.panzerQA.snapshot());
  if (reset.removed !== 0 || reset.shots !== 0) throw Error("Reset failed");
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: `${capture}/web-playground-${gpuMode}-mobile.png`,
  });
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth > innerWidth,
  );
  if (overflow) throw Error("Mobile overflow");
  const renderer = await page.evaluate(() => {
    const gl = document.querySelector("canvas").getContext("webgl2");
    const ext = gl.getExtension("WEBGL_debug_renderer_info");
    return ext
      ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)
      : gl.getParameter(gl.RENDERER);
  });
  if (errors.length) throw Error(errors.join("\n"));
  fs.writeFileSync(
    new URL(`../qa-results-${gpuMode}.json`, import.meta.url),
    JSON.stringify(
      {
        date: new Date().toISOString(),
        renderer,
        viewport: "1366x768; responsive 390x844",
        baseline,
        clicked,
        aimed,
        around,
        navigated,
        damaged,
        stress,
        reset,
        errors,
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      renderer,
      baseline: { cells: baseline.cells, draws: baseline.drawCalls },
      damaged: { removed: damaged.removed, particles: damaged.particles },
      stress: { particles: stress.particles, splats: stress.splats },
      errors,
    }),
  );
} finally {
  clearInterval(monitor);
  await browser.close();
}
