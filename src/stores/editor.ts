import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { fileRead, fileWrite } from "../lib/client";
import { toCommandError, type CommandError } from "../lib/errors";
import { dropBuffer, getBuffer, setBuffer } from "../features/editor/buffers";
import { useProjectStore } from "./project";
import {
  initialEditorState,
  reduceEditor,
  silentlyReloadable,
  type EditorAction,
  type EditorState,
} from "./editorReducer";

export const useEditorStore = defineStore("editor", () => {
  const state = ref<EditorState>(initialEditorState);
  const error = ref<CommandError | null>(null);
  const project = useProjectStore();

  const activeTab = computed(() => state.value.tabs.find((t) => t.path === state.value.active));

  function dispatch(action: EditorAction): void {
    state.value = reduceEditor(state.value, action);
  }

  function projectId(): number | null {
    return project.info?.id ?? null;
  }

  async function open(path: string): Promise<void> {
    const id = projectId();
    if (id === null) return;
    if (state.value.tabs.some((t) => t.path === path)) {
      dispatch({ type: "activated", path });
      return;
    }
    error.value = null;
    try {
      const content = await fileRead(id, path);
      setBuffer(path, content.text);
      dispatch({ type: "opened", path, sha256: content.sha256 });
    } catch (err: unknown) {
      error.value = toCommandError(err);
    }
  }

  function edited(path: string, text: string): void {
    setBuffer(path, text);
    dispatch({ type: "edited", path });
  }

  /** Saves with conflict detection; `force` overwrites external changes. */
  async function save(path: string, force = false): Promise<boolean> {
    const id = projectId();
    const tab = state.value.tabs.find((t) => t.path === path);
    const text = getBuffer(path);
    if (id === null || !tab || text === undefined) return false;
    error.value = null;
    try {
      const sha256 = await fileWrite(id, path, text, force ? null : tab.sha256);
      dispatch({ type: "saved", path, sha256 });
      return true;
    } catch (err: unknown) {
      error.value = toCommandError(err);
      return false;
    }
  }

  async function reload(path: string): Promise<void> {
    const id = projectId();
    if (id === null) return;
    try {
      const content = await fileRead(id, path);
      setBuffer(path, content.text);
      dispatch({ type: "reloaded", path, sha256: content.sha256 });
    } catch (err: unknown) {
      error.value = toCommandError(err);
    }
  }

  function close(path: string): void {
    dispatch({ type: "closed", path });
    dropBuffer(path);
  }

  function closeAll(): void {
    for (const tab of state.value.tabs) dropBuffer(tab.path);
    state.value = initialEditorState;
  }

  async function handleExternalChange(paths: string[]): Promise<void> {
    const id = projectId();
    if (id === null) return;
    for (const tab of state.value.tabs.filter((t) => paths.includes(t.path))) {
      let sha256: string | null = null;
      try {
        sha256 = (await fileRead(id, tab.path)).sha256;
      } catch {
        sha256 = null; // deleted or no longer readable
      }
      dispatch({ type: "externalChanged", path: tab.path, sha256 });
      const updated = state.value.tabs.find((t) => t.path === tab.path);
      if (updated && silentlyReloadable(updated)) await reload(tab.path);
    }
  }

  project.onExternalChange((paths) => void handleExternalChange(paths));

  return {
    state,
    error,
    activeTab,
    open,
    edited,
    save,
    reload,
    close,
    closeAll,
    dispatch,
  };
});
