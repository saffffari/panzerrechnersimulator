/** Reduce physical time per sample, keeping the display cadence in slow motion. */
export function playbackStep(step: number, speed: number): number {
  return Math.max(1e-6, step) * Math.max(0.001, Math.min(1, speed));
}
/** Cursor and projected axis are in the same normalized screen coordinates. */
export function sectionSlide(
  dx: number,
  dy: number,
  ax: number,
  ay: number,
): number {
  const length2 = ax * ax + ay * ay;
  return length2 > 0.0001 ? (dx * ax + dy * ay) / length2 : -dy * 6;
}
