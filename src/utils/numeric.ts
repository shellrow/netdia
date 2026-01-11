export function clampInt(n: number, min: number, max: number) {
  return Math.min(max, Math.max(min, n));
}
