import { Vector3 as V } from "three";
import { penetration, slope, type Round } from "./preset";
export type Cell = { p: V; origin: V; size: V; kind: number; alive: boolean };
export type Particle = { p: V; v: V; life: number; size: number; kind: number };
export class Simulation {
  cells: Cell[] = [];
  particles: Particle[] = [];
  splats: { p: V; n: V; size: number }[] = [];
  shots = 0;
  removed = 0;
  seed = 41;
  time = 0;
  random() {
    this.seed = (Math.imul(this.seed, 1664525) + 1013904223) >>> 0;
    return this.seed / 4294967296;
  }
  constructor() {
    this.reset();
  }
  reset() {
    this.cells = [];
    this.particles = [];
    this.splats = [];
    this.shots = 0;
    this.removed = 0;
    this.time = 0;
    const add = (
      x: number,
      y: number,
      z: number,
      kind: number,
      size = new V(0.195, 0.195, 0.195),
    ) =>
      this.cells.push({
        p: new V(x, y, z),
        origin: new V(x, y, z),
        size,
        kind,
        alive: true,
      });
    // Metre units. Closed hull and turret, with independently breakable coarse steel cells.
    for (let x = -8; x <= 8; x++)
      for (let y = 4; y <= 10; y++)
        for (let z = -14; z <= 14; z++)
          if (Math.abs(x) === 8 || y === 4 || y === 10 || Math.abs(z) === 14)
            add(x * 0.2, y * 0.2, z * 0.2, 0);
    for (let x = -6; x <= 6; x++)
      for (let y = 11; y <= 16; y++)
        for (let z = -6; z <= 6; z++)
          if (Math.abs(x) === 6 || y === 16 || Math.abs(z) === 6)
            add(x * 0.2, y * 0.2, z * 0.2, 1);
    // Crew are small voxel assemblies, not skinned meshes; every part can break off.
    for (const [cx, cy, cz] of [
      [-0.65, 1.25, 0.2],
      [0.65, 1.25, 0.2],
      [-0.55, 1.25, -1.45],
      [0.55, 1.25, -1.45],
    ])
      for (let x = -1; x <= 1; x++)
        for (let y = 0; y < 5; y++)
          for (let z = -1; z <= 1; z++)
            add(
              cx + x * 0.12,
              cy + y * 0.12,
              cz + z * 0.12,
              y === 4 ? 3 : 2,
              new V(0.115, 0.115, 0.115),
            );
  }
  emit(p: V, v: V, kind: number, size: number, life: number) {
    if (this.particles.length < 1200)
      this.particles.push({ p: p.clone(), v, kind, size, life });
  }
  fire(
    point: V,
    incoming: V,
    normal: V,
    r: Round,
    range: number,
    reach = 0.55,
    spread = 1,
  ) {
    this.shots++;
    const dir = incoming.clone().normalize();
    const angle =
      (Math.acos(Math.min(1, Math.abs(dir.dot(normal)))) * 180) / Math.PI;
    const plate =
      Math.abs(normal.y) > 0.5 ? 25 : Math.abs(normal.z) > 0.5 ? 100 : 80;
    const pen = penetration(r, range);
    const effective = plate * slope(r, angle, plate);
    const passed = pen > effective && angle < 85;
    const experimental = r.caliber_mm >= 800;
    const radius = Math.min(
      experimental ? 2.4 : 0.6,
      Math.max(0.16, r.caliber_mm / 700),
    );
    let lost = 0,
      crew = 0;
    for (const c of this.cells) {
      if (!c.alive) continue;
      const delta = c.p.clone().sub(point),
        along = delta.dot(dir),
        radial = delta.clone().addScaledVector(dir, -along).length();
      const tunnel =
        passed &&
        along > -0.22 &&
        along < 7 &&
        radial < radius + (c.kind >= 2 ? Math.min(1, reach) : 0);
      const crater = delta.length() < radius * (passed ? 1.8 : 1.2);
      if (tunnel || crater) {
        c.alive = false;
        lost++;
        if (c.kind >= 2) crew++;
        const v = dir
          .clone()
          .multiplyScalar(2 + this.random() * 7)
          .add(
            new V(
              (this.random() - 0.5) * 4,
              this.random() * 4,
              (this.random() - 0.5) * 4,
            ),
          );
        this.emit(c.p, v, c.kind >= 2 ? 2 : 0, c.kind >= 2 ? 0.09 : 0.13, 3.5);
      } else if (delta.length() < radius * 3 && c.kind < 2)
        c.p.addScaledVector(
          dir,
          Math.max(0, 0.18 * (1 - delta.length() / (radius * 3))),
        );
    }
    this.removed += lost;
    for (let i = 0; i < 110; i++) {
      const v = new V(
        (this.random() - 0.5) * 9,
        this.random() * 8,
        (this.random() - 0.5) * 9,
      ).addScaledVector(dir, passed ? 4 : -2);
      this.emit(point, v, 1, 0.035, 0.4 + this.random() * 0.6);
    }
    for (let i = 0; i < 28; i++)
      this.emit(
        point,
        new V(
          (this.random() - 0.5) * 0.8,
          0.5 + this.random(),
          (this.random() - 0.5) * 0.8,
        ),
        3,
        0.13 + this.random() * 0.2,
        2 + this.random() * 2,
      );
    if (crew)
      for (let i = 0; i < Math.min(240, crew * 8); i++)
        this.emit(
          point.clone().addScaledVector(dir, 0.8 + this.random() * 2),
          dir
            .clone()
            .multiplyScalar(2 + this.random() * 5)
            .add(
              new V(
                (this.random() - 0.5) * 5 * spread,
                this.random() * 3,
                (this.random() - 0.5) * 5 * spread,
              ),
            ),
          2,
          0.035 + this.random() * 0.045,
          2,
        );
    return { passed, pen, effective, lost, crew, angle };
  }
  step(dt: number) {
    this.time += dt;
    for (let i = this.particles.length - 1; i >= 0; i--) {
      const p = this.particles[i];
      p.life -= dt;
      if (p.life <= 0) {
        this.particles.splice(i, 1);
        continue;
      }
      const old = p.p.clone();
      if (p.kind === 3) p.v.y += 0.1 * dt;
      else p.v.y -= 9.81 * dt;
      p.p.addScaledVector(p.v, dt);
      if (p.kind === 2) {
        let n: V | undefined;
        if (p.p.y < 0.12) {
          p.p.y = 0.125;
          n = new V(0, 1, 0);
        }
        // First crossing of cabin wall deposits a square; the particle dies, so no repeated contacts.
        for (const axis of ["x", "z"] as const) {
          const limit = axis === "x" ? 1.5 : 2.7;
          if (
            Math.abs(old[axis]) < limit &&
            Math.abs(p.p[axis]) >= limit &&
            p.p.y > 0.8 &&
            p.p.y < 2.1
          ) {
            n = new V();
            n[axis] = -Math.sign(p.p[axis]);
            p.p[axis] = Math.sign(p.p[axis]) * (limit - 0.015);
          }
        }
        if (n) {
          if (this.splats.length >= 400) this.splats.shift();
          this.splats.push({
            p: p.p.clone(),
            n,
            size: 0.05 + this.random() * 0.15,
          });
          this.particles.splice(i, 1);
        }
      } else if (p.p.y < 0.12 && p.kind !== 3) {
        p.p.y = 0.12;
        p.v.multiplyScalar(0.25);
        p.v.y = Math.abs(p.v.y);
      }
    }
  }
}
