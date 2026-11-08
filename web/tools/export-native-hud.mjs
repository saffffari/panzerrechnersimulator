import { chromium } from "playwright";
import fs from "node:fs";
import { execFileSync } from "node:child_process";
const destination =
  "/home/alex/panzer/voxel-runtime/src/game_fea_range/hud_assets";
fs.mkdirSync(destination, { recursive: true });
const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  headless: true,
  args: ["--no-sandbox", "--use-gl=angle", "--use-angle=gl"],
});
try {
  const page = await browser.newPage();
  await page.goto("http://127.0.0.1:5178/");
  await page.waitForFunction(
    () => window.panzerEditor?.snapshot().ready,
    {},
    { timeout: 60000 },
  );
  const images = await page.evaluate(() => window.panzerEditor.hudAtlas());
  for (const im of images)
    fs.writeFileSync(
      `${destination}/${im.name}.png`,
      Buffer.from(im.data.split(",")[1], "base64"),
    );
  execFileSync("python3", [
    "-c",
    `from pathlib import Path
from PIL import Image
p=Path(${JSON.stringify(destination)})
base=list(Image.open(p/'tank.png').convert('RGBA').getdata())
for path in p.glob('tank-*.png'):
 im=Image.open(path).convert('RGBA')
 im.putdata([v if v!=base[i] else (0,0,0,0) for i,v in enumerate(im.getdata())])
 im.save(path)
`,
  ]);
  console.log(
    `Exported ${images.length} native HUD source renders to ${destination}`,
  );
} finally {
  await browser.close();
}
