/** Client-side restart policy after crashes; the core enforces the same budget. */
export const MAX_STARTS = 3;
export const WINDOW_MS = 60_000;

/** Returns the start timestamps to keep, or `null` if another start is not allowed. */
export function allowStart(previous: readonly number[], now: number): number[] | null {
  const recent = previous.filter((t) => now - t < WINDOW_MS);
  return recent.length >= MAX_STARTS ? null : [...recent, now];
}
