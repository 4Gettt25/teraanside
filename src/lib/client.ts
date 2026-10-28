/**
 * Typed client for the Rust core. This is the only place that calls `invoke`;
 * components and stores must go through these functions.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import { toCommandError } from "./errors";
import type { CounterEvent } from "./generated/CounterEvent";
import type { CounterRequest } from "./generated/CounterRequest";
import type { FileContent } from "./generated/FileContent";
import type { InitialOpen } from "./generated/InitialOpen";
import type { Pong } from "./generated/Pong";
import type { ProjectEvent } from "./generated/ProjectEvent";
import type { ProjectInfo } from "./generated/ProjectInfo";
import type { ProjectTree } from "./generated/ProjectTree";
import type { RecentProject } from "./generated/RecentProject";

export type {
  CounterEvent,
  CounterRequest,
  FileContent,
  InitialOpen,
  Pong,
  ProjectEvent,
  ProjectInfo,
  ProjectTree,
  RecentProject,
};

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

/** Shows the native folder picker in the core; resolves to `null` if cancelled. */
export function projectOpenDialog(title: string): Promise<ProjectInfo | null> {
  return call<ProjectInfo | null>("project_open_dialog", { title });
}

/** Project (and file) given on the command line; returned only once. */
export function projectInitial(): Promise<InitialOpen | null> {
  return call<InitialOpen | null>("project_initial", {});
}

export function projectOpenRecent(path: string): Promise<ProjectInfo> {
  return call<ProjectInfo>("project_open_recent", { path });
}

export function projectRecent(): Promise<RecentProject[]> {
  return call<RecentProject[]>("project_recent", {});
}

export function projectClose(id: number): Promise<void> {
  return call<undefined>("project_close", { id });
}

export function projectSetTrusted(id: number, trusted: boolean): Promise<ProjectInfo> {
  return call<ProjectInfo>("project_set_trusted", { id, trusted });
}

export function projectTree(id: number): Promise<ProjectTree> {
  return call<ProjectTree>("project_tree", { id });
}

export function projectWatch(id: number, onEvent: (event: ProjectEvent) => void): Promise<void> {
  return call<undefined>("project_watch", { id, onEvent: new Channel<ProjectEvent>(onEvent) });
}

export function fileRead(id: number, path: string): Promise<FileContent> {
  return call<FileContent>("file_read", { id, path });
}

/** Writes atomically in the core; resolves to the SHA256 of the written text. */
export function fileWrite(
  id: number,
  path: string,
  text: string,
  expectedSha256: string | null,
): Promise<string> {
  return call<string>("file_write", { id, path, text, expectedSha256 });
}

/** Reports an uncaught frontend error to the core log; never throws. */
export function reportFrontendError(message: string): void {
  invoke("frontend_log", { message }).catch(() => undefined);
}
