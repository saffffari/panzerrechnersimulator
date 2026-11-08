import { Plane, Ray, Vector3 } from "three";
export type RingAxis = "azimuth" | "elevation";
export function ringNormal(axis: RingAxis, from: Vector3) {
  return axis === "azimuth"
    ? new Vector3(0, 1, 0)
    : new Vector3(from.z, 0, -from.x).normalize();
}
export function discHit(
  ray: Ray,
  center: Vector3,
  normal: Vector3,
  radius: number,
) {
  const p = ray.intersectPlane(
    new Plane().setFromNormalAndCoplanarPoint(normal, center),
    new Vector3(),
  );
  return p && p.distanceToSquared(center) <= radius * radius ? p : null;
}
export function rotateOnRing(
  axis: RingAxis,
  point: Vector3,
  center: Vector3,
  initial: Vector3,
) {
  const d = point.clone().sub(center);
  if (d.lengthSq() < 1e-8) return initial.clone();
  if (axis === "azimuth") {
    const az = Math.atan2(d.x, d.z),
      h = Math.sqrt(Math.max(0, 1 - initial.y * initial.y));
    return new Vector3(Math.sin(az) * h, initial.y, Math.cos(az) * h);
  }
  const horizontal = new Vector3(initial.x, 0, initial.z).normalize();
  const el = Math.atan2(d.y, d.dot(horizontal));
  return horizontal
    .multiplyScalar(Math.cos(el))
    .add(new Vector3(0, Math.sin(el), 0));
}
