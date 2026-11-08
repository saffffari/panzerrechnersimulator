import fs from "node:fs";
import path from "node:path";
const input =
    process.argv[2] ||
    "/home/alex/panzer/editor-stable/checkout/target/playground_preset.json",
  name = process.argv[3];
const raw = JSON.parse(fs.readFileSync(path.resolve(input), "utf8"));
const list = Array.isArray(raw)
  ? raw
  : Array.isArray(raw.presets)
    ? raw.presets
    : [raw];
const chosen = name ? list.filter((p) => p.name === name) : list;
if (
  !chosen.length ||
  chosen.some(
    (p) =>
      typeof p.name !== "string" ||
      !p.settings ||
      typeof p.settings !== "object",
  )
)
  throw Error(
    "Expected {name,settings} preset or list; requested name must exist",
  );
fs.writeFileSync(
  new URL("../public/native-preset.json", import.meta.url),
  JSON.stringify(chosen.length === 1 ? chosen[0] : chosen, null, 2) + "\n",
);
console.log(
  `Imported ${chosen.length} preset(s); reload the local web page. Native file was read only.`,
);
