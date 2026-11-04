import { crewSeats, type Cell } from "./simulation";
export type Region =
  | "head"
  | "torso"
  | "leftArm"
  | "rightArm"
  | "leftLeg"
  | "rightLeg";
export type CrewDamage = {
  role: string;
  dead: boolean;
  regions: Record<Region, boolean>;
};
export type DamageState = {
  crew: CrewDamage[];
  modules: { front: boolean; rear: boolean; hull: boolean; turret: boolean };
};
export function summarizeDamage(cells: Cell[]): DamageState {
  const crew = crewSeats.map((seat, id) => {
    const owned = cells.filter((c) => c.crew === id),
      regions = {
        head: false,
        torso: false,
        leftArm: false,
        rightArm: false,
        leftLeg: false,
        rightLeg: false,
      };
    let lost = 0,
      headLost = 0,
      headTotal = 0;
    for (const c of owned) {
      const y = c.origin.y - seat.at[1],
        x = c.origin.x - seat.at[0];
      const region: Region =
        y > 0.42
          ? "head"
          : y < 0.18
            ? x < 0
              ? "leftLeg"
              : "rightLeg"
            : x < -0.06
              ? "leftArm"
              : x > 0.06
                ? "rightArm"
                : "torso";
      const damaged = !c.alive || c.p.distanceToSquared(c.origin) > 0.000025;
      if (damaged) regions[region] = true;
      if (!c.alive) lost++;
      if (region === "head") {
        headTotal++;
        if (!c.alive) headLost++;
      }
    }
    return {
      role: seat.role,
      regions,
      dead:
        lost > owned.length * 0.45 ||
        (headTotal > 0 && headLost >= headTotal * 0.5),
    };
  });
  const modules = { front: false, rear: false, hull: false, turret: false };
  for (const c of cells)
    if (
      c.kind < 2 &&
      (!c.alive || c.p.distanceToSquared(c.origin) > 0.000025)
    ) {
      if (c.kind === 1) modules.turret = true;
      else if (c.origin.z > 1.6) modules.front = true;
      else if (c.origin.z < -1.6) modules.rear = true;
      else modules.hull = true;
    }
  return { crew, modules };
}
const neutral = "#dddcd0",
  red = "#e35847",
  unknown = "#64716b";
export function damageHudMarkup(state: DamageState) {
  const people = state.crew
    .map((c) => {
      const fill = (region: Region) => (c.regions[region] ? red : neutral);
      return `<svg viewBox="0 0 24 56" role="img" aria-label="${c.role}: ${c.dead ? "KIA (coarse estimate)" : "region damage"}"><title>${c.role} — coarse voxel status</title><circle cx="12" cy="6" r="5" fill="${fill("head")}"/><path d="M8 13H16L17 31H7Z" fill="${fill("torso")}"/><path d="M4 14H8L6 33H2Z" fill="${fill("leftArm")}"/><path d="M16 14H20L22 33H18Z" fill="${fill("rightArm")}"/><path d="M7 32H12L10 54H5Z" fill="${fill("leftLeg")}"/><path d="M12 32H17L19 54H14Z" fill="${fill("rightLeg")}"/>${c.dead ? '<path d="M1 5L23 53M23 5L1 53" fill="none" stroke="#050505" stroke-width="4"/>' : ""}</svg>`;
    })
    .join("");
  const m = state.modules,
    col = (damaged: boolean) => (damaged ? red : neutral);
  return `<div class="crew-silhouettes">${people}</div><svg class="tank-status" viewBox="0 0 40 64" role="img" aria-label="Top-down coarse tank modules"><title>Damaged hull regions red. Gray tracks are static, unmodeled. Not native module telemetry.</title><path d="M4 10H9V60H4ZM31 10H36V60H31Z" fill="${unknown}"/><path d="M9 8H31V23H9Z" fill="${col(m.front)}"/><path d="M9 23H31V45H9Z" fill="${col(m.hull)}"/><path d="M9 45H31V59H9Z" fill="${col(m.rear)}"/><path d="M12 23H28V42H12Z" fill="${col(m.turret)}" stroke="#26342e" stroke-width="1"/><path d="M18 1H22V23H18Z" fill="${col(m.turret)}"/><path d="M10 24H30M10 45H30" stroke="#26342e" stroke-width="1" fill="none"/></svg>`;
}
export class DamageHud {
  private element: HTMLElement;
  constructor() {
    this.element = document.createElement("aside");
    this.element.id = "damage-hud";
    this.element.setAttribute("aria-label", "Coarse crew and tank damage");
    document.body.append(this.element);
  }
  update(cells: Cell[]) {
    this.element.innerHTML = damageHudMarkup(summarizeDamage(cells));
  }
  setVisible(visible: boolean) {
    this.element.hidden = !visible;
  }
}
