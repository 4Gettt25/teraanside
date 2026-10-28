/**
 * Typed client for the Rust core. This is the only place that calls `invoke`;
 * components and stores must go through these functions.
 */
import { invoke } from "@tauri-apps/api/core";
import { toCommandError } from "./errors";
import type { Pong } from "./generated/Pong";

export type { Pong };

async function call<T>(command: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err: unknown) {
    // Rejections are normalized so callers always receive a `CommandError`.
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    throw toCommandError(err);
  }
}

export function ping(message: string): Promise<Pong> {
  return call<Pong>("ping", { message });
}
