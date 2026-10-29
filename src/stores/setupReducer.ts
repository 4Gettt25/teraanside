import type { InstallProgress } from "../lib/generated/InstallProgress";
import type { SetupEvent } from "../lib/generated/SetupEvent";
import type { SetupStep } from "../lib/generated/SetupStep";

export type StepState = "pending" | "running" | "done" | "failed";

export interface StepView {
  step: SetupStep;
  state: StepState;
  progress: InstallProgress | null;
  error: string | null;
  /** Last log lines (bounded). */
  log: string[];
}

export interface SetupState {
  running: boolean;
  steps: StepView[];
  finished: { ok: boolean; cancelled: boolean } | null;
}

export const MAX_LOG_LINES = 200;

export const initialSetupState: SetupState = { running: false, steps: [], finished: null };

function updateStep(state: SetupState, index: number, f: (s: StepView) => StepView): SetupState {
  if (index < 0 || index >= state.steps.length) return state;
  return { ...state, steps: state.steps.map((s, i) => (i === index ? f(s) : s)) };
}

/** Pure reducer: folds one setup event into the wizard state. */
export function reduceSetup(state: SetupState, event: SetupEvent): SetupState {
  switch (event.type) {
    case "planned":
      return {
        running: true,
        finished: null,
        steps: event.steps.map((step) => ({
          step,
          state: "pending",
          progress: null,
          error: null,
          log: [],
        })),
      };
    case "stepStarted":
      return updateStep(state, event.index, (s) => ({ ...s, state: "running" }));
    case "progress":
      return updateStep(state, event.index, (s) => ({ ...s, progress: event.progress }));
    case "log":
      return updateStep(state, event.index, (s) => ({
        ...s,
        log: [...s.log, event.line].slice(-MAX_LOG_LINES),
      }));
    case "stepFinished":
      return updateStep(state, event.index, (s) => ({
        ...s,
        state: event.error === undefined ? "done" : "failed",
        error: event.error ?? null,
      }));
    case "finished":
      return { ...state, running: false, finished: { ok: event.ok, cancelled: event.cancelled } };
  }
}

export function reduceSetupEvents(state: SetupState, events: SetupEvent[]): SetupState {
  return events.reduce(reduceSetup, state);
}

/** 0..1 for a download in progress, `null` if unknown. */
export function downloadFraction(progress: InstallProgress | null): number | null {
  if (progress?.phase !== "downloading" || progress.total === undefined || progress.total === 0) {
    return null;
  }
  return Math.min(1, progress.received / progress.total);
}
