/**
 * Typed client for the Rust core. This is the only place that calls `invoke`;
 * components and stores must go through these functions.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import { toCommandError } from "./errors";
import type { CounterEvent } from "./generated/CounterEvent";
import type { CounterRequest } from "./generated/CounterRequest";
import type { Pong } from "./generated/Pong";

export type { CounterEvent, CounterRequest, Pong };

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

/**
 * Starts the demo counter stream. `onEvent` receives every event in order;
 * the promise resolves once the core has finished the stream.
 */
export function startCounter(
  request: CounterRequest,
  onEvent: (event: CounterEvent) => void,
): Promise<void> {
  const channel = new Channel<CounterEvent>(onEvent);
  return call<undefined>("start_counter", { request, onEvent: channel });
}
