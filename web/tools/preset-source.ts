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
      if (source === "baseline" && nativePath === nativePresetPath) {
        try {
          const settings = JSON.parse(
            readFileSync(
              "/home/alex/panzer/editor-stable/checkout/target/fea_range_state.json",
              "utf8",
            ),
          );
          if (settings.shot && Array.isArray(settings.shots))
            return {
              text: JSON.stringify({ name: "Native editor state", settings }),
              source: "native",
            };
        } catch {
          /* No saved native state yet. */
        }
      }
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
