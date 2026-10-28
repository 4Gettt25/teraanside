import type { CommandError } from "./generated/CommandError";

export type { CommandError };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    isRecord(value) &&
    typeof value.code === "string" &&
    typeof value.message === "string" &&
    (value.hint === null || typeof value.hint === "string")
  );
}

/** Normalizes anything an `invoke` call may reject with into a `CommandError`. */
export function toCommandError(value: unknown): CommandError {
  if (isCommandError(value)) {
    return value;
  }
  const message =
    typeof value === "string" ? value : value instanceof Error ? value.message : "unknown error";
  return { code: "ipc", message, hint: null };
}
