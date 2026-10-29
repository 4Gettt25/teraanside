import { describe, expect, it } from "vitest";
import type { SetupEvent } from "../lib/generated/SetupEvent";
import {
  MAX_LOG_LINES,
  downloadFraction,
  initialSetupState,
  reduceSetup,
  reduceSetupEvents,
} from "./setupReducer";

const planned: SetupEvent = {
  type: "planned",
  steps: [
    { kind: "tool", tool: "opentofu", version: "1.13.1" },
    { kind: "ansibleEnv", python: "3.13.16", ansibleCore: "2.21.5", ansibleLint: "26.10.1" },
  ],
};

describe("reduceSetup", () => {
  it("tracks a successful run", () => {
    const s = reduceSetupEvents(initialSetupState, [
      planned,
      { type: "stepStarted", index: 0 },
      { type: "progress", index: 0, progress: { phase: "downloading", received: 50, total: 100 } },
      { type: "stepFinished", index: 0 },
      { type: "stepStarted", index: 1 },
      { type: "log", index: 1, line: "Using CPython 3.13.16" },
      { type: "stepFinished", index: 1 },
      { type: "finished", ok: true, cancelled: false },
    ]);
    expect(s.running).toBe(false);
    expect(s.finished).toEqual({ ok: true, cancelled: false });
    expect(s.steps.map((x) => x.state)).toEqual(["done", "done"]);
    expect(s.steps[1]?.log).toEqual(["Using CPython 3.13.16"]);
  });

  it("records failures and leaves later steps pending", () => {
    const s = reduceSetupEvents(initialSetupState, [
      planned,
      { type: "stepStarted", index: 0 },
      { type: "stepFinished", index: 0, error: "signature verification failed" },
      { type: "finished", ok: false, cancelled: false },
    ]);
    expect(s.steps[0]).toMatchObject({ state: "failed", error: "signature verification failed" });
    expect(s.steps[1]?.state).toBe("pending");
  });

  it("bounds log lines and ignores unknown indices", () => {
    let s = reduceSetup(initialSetupState, planned);
    const lines: SetupEvent[] = Array.from({ length: MAX_LOG_LINES + 5 }, (_, i) => ({
      type: "log",
      index: 1,
      line: `l${String(i)}`,
    }));
    s = reduceSetupEvents(s, lines);
    expect(s.steps[1]?.log).toHaveLength(MAX_LOG_LINES);
    expect(s.steps[1]?.log[0]).toBe("l5");
    expect(reduceSetup(s, { type: "stepStarted", index: 9 })).toBe(s);
  });

  it("computes download fractions", () => {
    expect(downloadFraction({ phase: "downloading", received: 25, total: 100 })).toBe(0.25);
    expect(downloadFraction({ phase: "downloading", received: 25 })).toBeNull();
    expect(downloadFraction({ phase: "verifying" })).toBeNull();
    expect(downloadFraction(null)).toBeNull();
  });
});
