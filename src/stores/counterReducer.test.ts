import { describe, expect, it } from "vitest";
import type { CounterEvent } from "../lib/generated/CounterEvent";
import { initialCounterState, reduceCounter, reduceCounterEvents } from "./counterReducer";

const fullRun: CounterEvent[] = [
  { type: "started", total: 3 },
  { type: "tick", value: 1 },
  { type: "tick", value: 2 },
  { type: "tick", value: 3 },
  { type: "finished", total: 3 },
];

describe("reduceCounter", () => {
  it("reduces a complete run", () => {
    expect(reduceCounterEvents(initialCounterState, fullRun)).toEqual({
      status: "finished",
      total: 3,
      value: 3,
      ticks: 3,
    });
  });

  it("is running after start and partial ticks", () => {
    expect(reduceCounterEvents(initialCounterState, fullRun.slice(0, 3))).toEqual({
      status: "running",
      total: 3,
      value: 2,
      ticks: 2,
    });
  });

  it("resets progress on a new start", () => {
    const finished = reduceCounterEvents(initialCounterState, fullRun);
    expect(reduceCounter(finished, { type: "started", total: 5 })).toEqual({
      status: "running",
      total: 5,
      value: 0,
      ticks: 0,
    });
  });

  it("does not mutate the input state", () => {
    const state = { ...initialCounterState };
    reduceCounter(state, { type: "started", total: 1 });
    expect(state).toEqual(initialCounterState);
  });

  it("batched reduction equals event-by-event reduction", () => {
    const stepwise = fullRun.reduce(reduceCounter, initialCounterState);
    const batched = reduceCounterEvents(
      reduceCounterEvents(initialCounterState, fullRun.slice(0, 2)),
      fullRun.slice(2),
    );
    expect(batched).toEqual(stepwise);
  });
});
