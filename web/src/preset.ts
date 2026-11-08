import catalogue from "./catalogue.json";
export const rounds = [
  ...catalogue,
  {
    id: "GUSTAV",
    name: "800 mm Gustav AP · experimental",
    kind: "Ap",
    caliber_mm: 800,
    mass_kg: 7100,
    muzzle_velocity: 720,
    filler_g: 0,
    drag_k: 0.00002,
    pen_ref_mm: 900,
    ref_range_m: 100,
  },
  {
    id: "T34_SONIC",
    name: "T-34 @ 343 m/s · experimental",
    kind: "Ap",
    caliber_mm: 3000,
    mass_kg: 28500,
    muzzle_velocity: 343,
    filler_g: 0,
    drag_k: 0,
    pen_ref_mm: 1600,
    ref_range_m: 100,
  },
];
export type Round = (typeof rounds)[number];
export type Preset = { name: string; settings: Record<string, any> };
export const finite = (x: unknown, d: number, lo = -1e5, hi = 1e5) =>
  typeof x === "number" && Number.isFinite(x)
    ? Math.min(hi, Math.max(lo, x))
    : d;
export function parsePresets(raw: any): Preset[] {
  const list = Array.isArray(raw)
    ? raw
    : Array.isArray(raw?.presets)
      ? raw.presets
      : [raw];
  return list
    .filter(
      (p: any) =>
        p &&
        typeof p.name === "string" &&
        p.settings &&
        typeof p.settings === "object",
    )
    .map((p: any) => ({ name: p.name, settings: p.settings }));
}
// Authored lists replace all catalogue/preset fallback options, preserving each saved shot.
export function shotOptions(presets: Preset[]) {
  const authored = presets.some((p) => Array.isArray(p.settings.shots));
  const choices: Preset[] = [];
  let selected = 0;
  for (const p of presets) {
    if (!authored) {
      choices.push(p);
      continue;
    }
    const shots = p.settings.shots;
    if (!Array.isArray(shots)) continue;
    const current = Math.floor(
      finite(p.settings.selected_shot, 0, 0, Math.max(0, shots.length - 1)),
    );
    for (const [i, entry] of shots.entries()) {
      if (
        !entry ||
        typeof entry.name !== "string" ||
        !entry.shot ||
        typeof entry.shot !== "object"
      )
        continue;
      if (i === current && p === presets[0]) selected = choices.length;
      choices.push({
        name: entry.name,
        settings: { ...p.settings, shot: entry.shot },
      });
    }
  }
  return { authored, choices, selected };
}
export function slope(r: Round, angle: number, plate: number) {
  const norm =
    r.kind === "Apcbc" ? 5 : r.kind === "Ap" ? 3 : r.kind === "Apcr" ? 1.5 : 0;
  const exp =
    r.kind === "Apcbc"
      ? 1.3
      : r.kind === "Ap"
        ? 1.4
        : r.kind === "Apcr"
          ? 1.75
          : 1;
  return (
    (1 /
      Math.max(
        0.17,
        Math.cos((Math.min(80, Math.max(0, angle - norm)) * Math.PI) / 180),
      )) **
    Math.max(
      0.85,
      exp * Math.max(0.2, Math.min(3, plate / r.caliber_mm)) ** 0.4,
    )
  );
}
export function penetration(r: Round, range: number) {
  return (
    r.pen_ref_mm *
    slope(r, 30, r.pen_ref_mm) *
    Math.exp(-r.drag_k * (range - r.ref_range_m) * 1.43)
  );
}
