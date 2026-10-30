import { describe, expect, it } from "vitest";
import { MAX_STARTS, WINDOW_MS, allowStart } from "./restartPolicy";

describe("allowStart", () => {
  it("allows a few starts per window and forgets old ones", () => {
    let starts: number[] = [];
    for (let i = 0; i < MAX_STARTS; i++) {
      const next = allowStart(starts, 1000 + i);
      expect(next).not.toBeNull();
      starts = next ?? [];
    }
    expect(allowStart(starts, 2000)).toBeNull();
    expect(allowStart(starts, 1000 + WINDOW_MS + 5)).toHaveLength(1);
  });
});
