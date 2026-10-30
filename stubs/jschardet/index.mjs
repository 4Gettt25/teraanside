// Stub for jschardet (see package.json). Callers (VS Code's encoding guesser)
// treat a missing result as "unknown encoding".
export function detect() {
  return null;
}

export function detectAll() {
  return [];
}

export default { detect, detectAll };
