import { Vector3 as V } from "three";
export type Hemisphere = "near" | "far";
// CPU geometry only. The cursor supplies a world ray, never an incremental screen rotation.
export class SphereAimDrag {
  hemisphere: Hemisphere;
  outside = false;
  constructor(origin: V, center: V, radius: number, handle: V) {
    this.hemisphere =
      handle.clone().sub(center).dot(origin.clone().sub(center)) >=
      radius * radius
        ? "near"
        : "far";
  }
  project(origin: V, direction: V, center: V, radius: number) {
    const d = direction.clone().normalize(),
      oc = origin.clone().sub(center),
      b = oc.dot(d),
      c = oc.lengthSq() - radius * radius,
      discriminant = b * b - c;
    if (discriminant >= 0) {
      const root = Math.sqrt(discriminant),
        near = -b - root,
        far = -b + root;
      if (far >= 0) {
        this.outside = false;
        const t = this.hemisphere === "near" && near >= 0 ? near : far;
        return origin.clone().addScaledVector(d, t);
      }
    }
    const distance = oc.length();
    if (distance <= radius) return center.clone().addScaledVector(d, radius);
    // Clamp a missed cursor ray to the perspective silhouette (the true tangent circle).
    const toward = oc.clone().negate().divideScalar(distance),
      side = d.clone().addScaledVector(toward, -d.dot(toward));
    if (side.lengthSq() < 1e-12)
      side.set(1, 0, 0).addScaledVector(toward, -toward.x);
    if (side.lengthSq() < 1e-12) side.set(0, 1, 0);
    side.normalize();
    const sin = radius / distance,
      cos = Math.sqrt(1 - sin * sin),
      tangent = toward.multiplyScalar(cos).addScaledVector(side, sin);
    if (!this.outside)
      this.hemisphere = this.hemisphere === "near" ? "far" : "near";
    this.outside = true;
    return origin
      .clone()
      .addScaledVector(
        tangent,
        Math.sqrt(distance * distance - radius * radius),
      );
  }
}

export function markerWorldDiameter(
  depth: number,
  fovDegrees: number,
  viewportHeight: number,
  pixels = 12,
) {
  return (
    (2 *
      Math.max(0.01, depth) *
      Math.tan((fovDegrees * Math.PI) / 360) *
      pixels) /
    Math.max(1, viewportHeight)
  );
}
