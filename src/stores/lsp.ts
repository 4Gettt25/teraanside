import { defineStore } from "pinia";
import { ref } from "vue";
import type { ServerKind } from "../lib/generated/ServerKind";
import { useProjectStore } from "./project";

/** Language server state per kind, shown in the status bar (symbol + text). */
export type LspState =
  | { state: "off" }
  | { state: "starting" }
  | { state: "running" }
  | { state: "restricted" }
  | { state: "unavailable"; message: string }
  | { state: "failed"; message: string };

export const useLspStore = defineStore("lsp", () => {
  const servers = ref<Record<ServerKind, LspState>>({
    terraform: { state: "off" },
    ansible: { state: "off" },
  });

  function set(kind: ServerKind, state: LspState): void {
    servers.value = { ...servers.value, [kind]: state };
  }

  /** Kinds a file asked for (restarted after trust is granted). */
  const requested = new Set<ServerKind>();

  /** Called when a file of a server's language is shown (Monaco already loaded). */
  async function ensure(kind: ServerKind): Promise<void> {
    requested.add(kind);
    const project = useProjectStore();
    const info = project.info;
    if (!info) return;
    if (!info.trusted) {
      set(kind, { state: "restricted" });
      return;
    }
    const clients = await import("../features/editor/monaco/languageClients");
    await clients.ensureClient(kind, info.id, info.root, set);
  }

  /** Re-requests servers (after trust was granted or tools were set up). */
  async function retry(): Promise<void> {
    for (const kind of requested) await ensure(kind);
  }

  async function stopAll(): Promise<void> {
    requested.clear();
    const clients = await import("../features/editor/monaco/languageClients");
    await clients.stopAll();
    reset();
  }

  function reset(): void {
    servers.value = { terraform: { state: "off" }, ansible: { state: "off" } };
  }

  return { servers, set, ensure, retry, stopAll, reset };
});
