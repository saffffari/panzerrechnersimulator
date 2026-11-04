import { readFileSync } from "node:fs";
export const nativePresetPath =
  "/home/alex/panzer/editor-stable/checkout/target/playground_preset.json";
export function readCurrentPreset(
  nativePath = nativePresetPath,
  baselinePath = new URL("../public/native-preset.json", import.meta.url),
) {
  for (const [source, path] of [
    ["native", nativePath],
    ["baseline", baselinePath],
  ] as const) {
    try {
      const text = readFileSync(path, "utf8");
      const raw = JSON.parse(text);
      const list = Array.isArray(raw)
        ? raw
        : Array.isArray(raw?.presets)
          ? raw.presets
          : [raw];
      if (
        !list.length ||
        list.some(
          (p: any) =>
            !p ||
            typeof p.name !== "string" ||
            !p.settings ||
            typeof p.settings !== "object",
        )
      )
        continue;
      return { text, source };
    } catch {
      /* Native save may not exist yet; never write or mutate it. */
    }
  }
  throw Error("No valid native preset or bundled baseline");
}
