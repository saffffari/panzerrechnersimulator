import { chromium } from "playwright";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
const busy = () =>
  execFileSync("ps", ["-eo", "comm,args"], { encoding: "utf8" })
    .split("\n")
    .some(
      (l) =>
        /^panzer\S*\s+.*fea-range/.test(l) ||
        /^panzer\S*\s+.*editor-stable\/target/.test(l),
    );
if (busy()) throw Error("Native editor open: GPU QA deferred");
const browser = await chromium.launch({
  headless: process.env.HEADED !== "1",
  executablePath: "/usr/bin/chromium",
  args: process.env.HEADED === "1" ? ["--use-gl=angle", "--use-angle=gl"] : [],
});
const monitor = setInterval(() => {
  if (busy()) {
    console.error("Native editor opened; closing own QA browser");
    browser.close();
    process.exitCode = 2;
  }
}, 250);
try {
  const page = await browser.newPage({
      viewport: { width: 1366, height: 768 },
    }),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const actual = await page.request.get(
    "http://127.0.0.1:5178/native-preset.json",
  );
  if (actual.headers()["cache-control"] !== "no-store")
    throw Error("Native endpoint cached");
  const fixture = {
    name: "Authored QA fixture",
    settings: {
      selected_shot: 1,
      shot: { shell: 4 },
      shots: [
        {
          name: "Gustav angle",
          shot: {
            shell: 16,
            range_m: 333,
            from_az: -70,
            from_el: 20,
            seed: 33,
            target: { point: [-1.6, 1.45, 0] },
          },
        },
        {
          name: "T-34 Mach 1",
          shot: {
            shell: 17,
            range_m: 0,
            from_az: -90,
            from_el: 0,
            seed: 41,
            target: { point: [-1.234, 1.567, 0.89] },
          },
        },
      ],
    },
  };
  await page.route("**/native-preset.json", (route) =>
    route.fulfill({ json: fixture }),
  );
  await page.goto("http://127.0.0.1:5178/");
  await page.waitForFunction(() => window.panzerQA?.snapshot().cells > 0);
  await page.waitForTimeout(500);
  const options = await page.locator("#shot option").allTextContents();
  if (
    JSON.stringify(options) !== JSON.stringify(["Gustav angle", "T-34 Mach 1"])
  )
    throw Error("Unexpected authored choices");
  const initial = await page.evaluate(() => window.panzerQA.snapshot());
  if (
    initial.roundIndex !== 17 ||
    JSON.stringify(initial.impact) !== JSON.stringify([-1.234, 1.567, 0.89])
  )
    throw Error("Selected shot/position changed");
  await page.selectOption("#shot", "p:0");
  const first = await page.evaluate(() => window.panzerQA.snapshot());
  if (
    first.roundIndex !== 16 ||
    Math.abs(first.from[1] - Math.sin((20 * Math.PI) / 180)) > 1e-8
  )
    throw Error("Authored round/direction lost");
  const stamp = new Date().toISOString().replaceAll(":", "-"),
    dir = "/windows/shop/panzerrechnersystem/captures/screenshots";
  await page.screenshot({
    path: `${dir}/web-playground-authored-${stamp}.png`,
  });
  await page.selectOption("#mode", "play");
  await page.selectOption("#enemy-count", "10");
  await page.waitForTimeout(250);
  if (
    (await page.locator("#shot-control").isVisible()) ||
    (await page.locator("#step-control").isVisible())
  )
    throw Error("Sandbox knobs leaked into Play");
  const start = await page.evaluate(() => window.panzerQA.snapshot());
  if (
    start.mode !== "play" ||
    start.battle.enemies.length !== 10 ||
    start.battle.enemies[0].s < 980
  )
    throw Error("Play setup failed");
  await page.keyboard.down("KeyW");
  await page.waitForTimeout(2200);
  await page.keyboard.up("KeyW");
  const moving = await page.evaluate(() => window.panzerQA.snapshot());
  if (Math.hypot(moving.battle.player.x, moving.battle.player.z) < 6)
    throw Error("Player did not move");
  if (
    moving.battle.memory.source !== "initial" ||
    moving.battle.memory.point.x !== 0 ||
    moving.battle.memory.point.z !== 0
  )
    throw Error("Omniscient memory leak");
  await page.click("#fire");
  const fired = await page.evaluate(() => window.panzerQA.snapshot());
  if (fired.battle.shots !== 1) throw Error("Play fire failed");
  await page.waitForTimeout(250);
  await page.screenshot({ path: `${dir}/web-playground-play-10-${stamp}.png` });
  await page.keyboard.press("Backspace");
  const reset = await page.evaluate(() => window.panzerQA.snapshot());
  if (
    reset.battle.player.x !== 0 ||
    reset.battle.player.z !== 0 ||
    reset.battle.shots !== 0
  )
    throw Error("Play restart failed");
  await page.selectOption("#mode", "sandbox");
  if (
    (await page.locator("#enemy-control").isVisible()) ||
    !(await page.locator("#shot-control").isVisible())
  )
    throw Error("Sandbox not restored");
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: `${dir}/web-playground-mode-mobile-${stamp}.png`,
  });
  if (
    await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)
  )
    throw Error("Mobile overflow");
  const renderer = await page.evaluate(() => {
    const gl = document.querySelector("canvas").getContext("webgl2");
    const e = gl.getExtension("WEBGL_debug_renderer_info");
    return e
      ? gl.getParameter(e.UNMASKED_RENDERER_WEBGL)
      : gl.getParameter(gl.RENDERER);
  });
  if (errors.length) throw Error(errors.join("\n"));
  fs.writeFileSync(
    new URL("../qa-play-mode.json", import.meta.url),
    JSON.stringify(
      {
        date: stamp,
        renderer,
        nativeEndpointSource: actual.headers()["x-panzer-preset-source"],
        options,
        initial,
        first,
        start,
        moving,
        fired,
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
      options,
      playerMoved: Math.hypot(moving.battle.player.x, moving.battle.player.z),
      memory: moving.battle.memory,
      enemies: moving.battle.enemies.length,
      playDraws: moving.drawCalls,
      errors,
    }),
  );
} finally {
  clearInterval(monitor);
  await browser.close();
}
