import { chromium } from "playwright";
import fs from "node:fs";
const dir = "/windows/shop/panzerrechnersystem/captures/screenshots";
fs.mkdirSync(dir, { recursive: true });
const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  headless: true,
  args: ["--no-sandbox", "--use-gl=angle", "--use-angle=gl"],
});
try {
  const page = await browser.newPage({
    viewport: { width: 1440, height: 900 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("http://127.0.0.1:5178/");
  await page.waitForFunction(
    () => window.panzerEditor?.snapshot().ready,
    {},
    { timeout: 90000 },
  );
  await page.waitForTimeout(1500);
  console.log(await page.evaluate(() => window.panzerEditor.snapshot()));
  await page.screenshot({ path: dir + "/web-editor-rebuild-exterior.png" });
  await page.evaluate(() => window.panzerEditor.setRoof(true));
  await page.selectOption("#camera", "top");
  await page.waitForTimeout(500);
  await page.screenshot({ path: dir + "/web-editor-rebuild-top.png" });
  const png = await page.evaluate(() => window.panzerEditor.capture(540, 960));
  fs.writeFileSync(
    dir + "/web-editor-cutaway-540x960.png",
    Buffer.from(png.split(",")[1], "base64"),
  );
  console.log({ errors });
} finally {
  await browser.close();
}
