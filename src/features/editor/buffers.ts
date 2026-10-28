/**
 * Current editor text per path, deliberately outside Vue reactivity. Instead of
 * copying the text on every keystroke, each editor registers a source function
 * (Monaco: `model.getValue()`), which is only called when saving.
 */
type BufferSource = () => string;

const sources = new Map<string, BufferSource>();

export function getBuffer(path: string): string | undefined {
  return sources.get(path)?.();
}

export function setBuffer(path: string, text: string): void {
  sources.set(path, () => text);
}

export function setBufferSource(path: string, source: BufferSource): void {
  sources.set(path, source);
}

export function dropBuffer(path: string): void {
  sources.delete(path);
}
