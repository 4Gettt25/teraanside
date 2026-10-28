import { describe, expect, it } from "vitest";
import { dropBuffer, getBuffer, setBuffer, setBufferSource } from "./buffers";

describe("buffers", () => {
  it("stores text or reads lazily from a source", () => {
    setBuffer("a", "one");
    expect(getBuffer("a")).toBe("one");
    let calls = 0;
    setBufferSource("a", () => {
      calls++;
      return "live";
    });
    expect(calls).toBe(0);
    expect(getBuffer("a")).toBe("live");
    expect(calls).toBe(1);
    dropBuffer("a");
    expect(getBuffer("a")).toBeUndefined();
  });
});
