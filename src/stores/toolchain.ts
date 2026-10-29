import { defineStore } from "pinia";
import { ref } from "vue";
import {
  toolchainCancel,
  toolchainChooseFlavor,
  toolchainSetup,
  toolchainStatus,
  type SetupEvent,
  type TerraformFlavor,
  type ToolchainStatus,
} from "../lib/client";
import { createBatcher } from "../lib/batch";
import { toCommandError, type CommandError } from "../lib/errors";
import { initialSetupState, reduceSetupEvents, type SetupState } from "./setupReducer";

export const useToolchainStore = defineStore("toolchain", () => {
  const status = ref<ToolchainStatus | null>(null);
  const setup = ref<SetupState>(initialSetupState);
  const error = ref<CommandError | null>(null);
  const wizardOpen = ref(false);

  async function guard<T>(f: () => Promise<T>): Promise<T | null> {
    error.value = null;
    try {
      return await f();
    } catch (err: unknown) {
      error.value = toCommandError(err);
      return null;
    }
  }

  /** Loads the status; opens the wizard automatically if something is missing. */
  async function load(projectId: number, autoOpen = true): Promise<void> {
    setup.value = initialSetupState;
    const s = await guard(() => toolchainStatus(projectId));
    status.value = s;
    if (s && autoOpen && !s.ready && (s.detected.terraform || s.detected.ansible)) {
      wizardOpen.value = true;
    }
  }

  async function chooseFlavor(projectId: number, flavor: TerraformFlavor): Promise<void> {
    const s = await guard(() => toolchainChooseFlavor(projectId, flavor));
    if (s) status.value = s;
  }

  async function runSetup(projectId: number): Promise<void> {
    setup.value = { ...initialSetupState, running: true };
    const batcher = createBatcher<SetupEvent>((events) => {
      setup.value = reduceSetupEvents(setup.value, events);
    });
    const s = await guard(() =>
      toolchainSetup(projectId, (e) => {
        batcher.push(e);
      }),
    );
    batcher.flush();
    if (s) status.value = s;
    if (setup.value.running) setup.value = { ...setup.value, running: false };
  }

  async function cancel(): Promise<void> {
    await guard(toolchainCancel);
  }

  function reset(): void {
    status.value = null;
    setup.value = initialSetupState;
    wizardOpen.value = false;
    error.value = null;
  }

  return { status, setup, error, wizardOpen, load, chooseFlavor, runSetup, cancel, reset };
});
