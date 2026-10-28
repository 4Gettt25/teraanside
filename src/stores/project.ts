import { defineStore } from "pinia";
import { ref, shallowRef } from "vue";
import {
  projectClose,
  projectOpenDialog,
  projectOpenRecent,
  projectRecent,
  projectSetTrusted,
  projectTree,
  projectWatch,
  type ProjectInfo,
  type ProjectTree,
  type RecentProject,
} from "../lib/client";
import { toCommandError, type CommandError } from "../lib/errors";

export const useProjectStore = defineStore("project", () => {
  const info = ref<ProjectInfo | null>(null);
  // The tree can be large; replaced as a whole, never mutated deeply.
  const tree = shallowRef<ProjectTree | null>(null);
  const recent = ref<RecentProject[]>([]);
  const error = ref<CommandError | null>(null);
  /** True until the user answered the trust question for the current project. */
  const trustPending = ref(false);
  const changeListeners = new Set<(paths: string[]) => void>();

  async function guard<T>(f: () => Promise<T>): Promise<T | null> {
    error.value = null;
    try {
      return await f();
    } catch (err: unknown) {
      error.value = toCommandError(err);
      return null;
    }
  }

  async function loadRecent(): Promise<void> {
    recent.value = (await guard(projectRecent)) ?? recent.value;
  }

  async function activate(opened: ProjectInfo | null): Promise<void> {
    if (!opened) return;
    if (info.value && info.value.id !== opened.id) await guard(() => close());
    info.value = opened;
    trustPending.value = !opened.trusted;
    await refreshTree();
    await guard(() =>
      projectWatch(opened.id, (event) => {
        void refreshTree();
        for (const listener of changeListeners) listener(event.paths);
      }),
    );
    await loadRecent();
  }

  async function openDialog(title: string): Promise<void> {
    await activate(await guard(() => projectOpenDialog(title)));
  }

  async function openRecent(path: string): Promise<void> {
    await activate(await guard(() => projectOpenRecent(path)));
  }

  async function refreshTree(): Promise<void> {
    const current = info.value;
    if (!current) return;
    const next = await guard(() => projectTree(current.id));
    if (next) tree.value = next;
  }

  async function setTrusted(trusted: boolean): Promise<void> {
    const current = info.value;
    if (!current) return;
    const updated = await guard(() => projectSetTrusted(current.id, trusted));
    if (updated) info.value = updated;
    trustPending.value = false;
  }

  function dismissTrust(): void {
    trustPending.value = false;
  }

  async function close(): Promise<void> {
    const current = info.value;
    info.value = null;
    tree.value = null;
    trustPending.value = false;
    if (current) await projectClose(current.id);
  }

  /** Subscribe to external file changes of the open project; returns an unsubscribe. */
  function onExternalChange(listener: (paths: string[]) => void): () => void {
    changeListeners.add(listener);
    return () => changeListeners.delete(listener);
  }

  return {
    info,
    tree,
    recent,
    error,
    trustPending,
    loadRecent,
    openDialog,
    openRecent,
    refreshTree,
    setTrusted,
    dismissTrust,
    close,
    onExternalChange,
  };
});
