import type { CounterEvent } from "../lib/generated/CounterEvent";

export type CounterStatus = "idle" | "running" | "finished" | "error";

export interface CounterState {
  status: CounterStatus;
  total: number;
  value: number;
  /** Number of tick events received (detects gaps/duplicates). */
  ticks: number;
}

export const initialCounterState: CounterState = {
  status: "idle",
  total: 0,
  value: 0,
  ticks: 0,
};

/** Pure reducer: folds one stream event into the counter state. */
export function reduceCounter(state: CounterState, event: CounterEvent): CounterState {
  switch (event.type) {
    case "started":
      return { status: "running", total: event.total, value: 0, ticks: 0 };
    case "tick":
      return { ...state, value: event.value, ticks: state.ticks + 1 };
    case "finished":
      return { ...state, status: "finished", total: event.total };
  }
}

export function reduceCounterEvents(state: CounterState, events: CounterEvent[]): CounterState {
  return events.reduce(reduceCounter, state);
}
