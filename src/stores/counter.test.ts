import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import type { CounterEvent, CounterRequest } from "../lib/client";
import { useCounterStore } from "./counter";

const startCounter =
  vi.fn<(req: CounterRequest, onEvent: (e: CounterEvent) => void) => Promise<void>>();

vi.mock("../lib/client", () => ({
  startCounter: (req: CounterRequest, onEvent: (e: CounterEvent) => void) =>
    startCounter(req, onEvent),
}));

beforeEach(() => {
  setActivePinia(createPinia());
  startCounter.mockReset();
});

describe("counter store", () => {
  it("reduces streamed events into state", async () => {
    startCounter.mockImplementation((req, onEvent) => {
      onEvent({ type: "started", total: req.count });
      for (let value = 1; value <= req.count; value++) onEvent({ type: "tick", value });
      onEvent({ type: "finished", total: req.count });
      return Promise.resolve();
    });
    const store = useCounterStore();
    await store.start(4, 0);
    expect(startCounter).toHaveBeenCalledWith({ count: 4, intervalMs: 0 }, expect.any(Function));
    expect(store.state).toEqual({ status: "finished", total: 4, value: 4, ticks: 4 });
    expect(store.error).toBeNull();
  });

  it("records structured errors", async () => {
    startCounter.mockRejectedValue({ code: "invalid_input", message: "bad", hint: null });
    const store = useCounterStore();
    await store.start(0, 0);
    expect(store.state.status).toBe("error");
    expect(store.error).toEqual({ code: "invalid_input", message: "bad", hint: null });
  });
});
