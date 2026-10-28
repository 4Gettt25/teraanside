import { afterEach, describe, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ping } from "./client";

afterEach(() => {
  clearMocks();
});

describe("client.ping", () => {
  it("invokes the ping command with the message", async () => {
    const calls: { cmd: string; payload: unknown }[] = [];
    mockIPC((cmd, payload) => {
      calls.push({ cmd, payload });
      return { message: "hi", coreVersion: "0.1.0", os: "linux" };
    });

    await expect(ping("hi")).resolves.toEqual({
      message: "hi",
      coreVersion: "0.1.0",
      os: "linux",
    });
    expect(calls).toEqual([{ cmd: "ping", payload: { message: "hi" } }]);
  });

  it("rejects with a structured CommandError", async () => {
    mockIPC(() => {
      // Tauri rejects with the serialized error object, not an Error instance.
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      throw { code: "invalid_input", message: "too long", hint: null };
    });
    await expect(ping("x")).rejects.toEqual({
      code: "invalid_input",
      message: "too long",
      hint: null,
    });
  });
});
