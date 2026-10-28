import { describe, expect, it } from "vitest";
import { isCommandError, toCommandError } from "./errors";

describe("toCommandError", () => {
  it("passes structured errors through", () => {
    const err = { code: "invalid_input", message: "too long", hint: null };
    expect(isCommandError(err)).toBe(true);
    expect(toCommandError(err)).toBe(err);
  });

  it("wraps plain strings (e.g. unknown command)", () => {
    expect(toCommandError("command foo not found")).toEqual({
      code: "ipc",
      message: "command foo not found",
      hint: null,
    });
  });

  it("wraps Error instances and unknown values", () => {
    expect(toCommandError(new Error("boom")).message).toBe("boom");
    expect(toCommandError(42).message).toBe("unknown error");
  });

  it("rejects objects with wrong field types", () => {
    expect(isCommandError({ code: 1, message: "x", hint: null })).toBe(false);
    expect(isCommandError({ code: "c", message: "x" })).toBe(false);
  });
});
