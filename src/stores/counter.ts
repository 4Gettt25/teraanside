import { defineStore } from "pinia";
import { ref } from "vue";
import { startCounter, type CounterEvent } from "../lib/client";
import { createBatcher } from "../lib/batch";
import { toCommandError, type CommandError } from "../lib/errors";
import { initialCounterState, reduceCounterEvents, type CounterState } from "./counterReducer";

export const useCounterStore = defineStore("counter", () => {
  const state = ref<CounterState>({ ...initialCounterState });
  const error = ref<CommandError | null>(null);

  async function start(count: number, intervalMs: number): Promise<void> {
    error.value = null;
    state.value = { ...initialCounterState };
    const batcher = createBatcher<CounterEvent>((events) => {
      state.value = reduceCounterEvents(state.value, events);
    });
    try {
      await startCounter({ count, intervalMs }, (event) => {
        batcher.push(event);
      });
      batcher.flush();
    } catch (err: unknown) {
      batcher.flush();
      error.value = toCommandError(err);
      state.value = { ...state.value, status: "error" };
    }
  }

  return { state, error, start };
});
