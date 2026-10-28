/**
 * Current editor text per path, deliberately outside Vue reactivity: buffers can be
 * large and change on every keystroke; the store only tracks metadata (dirty, hash).
 */
const buffers = new Map<string, string>();

export function getBuffer(path: string): string | undefined {
  return buffers.get(path);
}

export function setBuffer(path: string, text: string): void {
  buffers.set(path, text);
}

export function dropBuffer(path: string): void {
  buffers.delete(path);
}

export function clearBuffers(): void {
  buffers.clear();
}
