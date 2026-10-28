/** Editor tab state (metadata only; buffer text lives outside reactivity). */
export type ExternalState = "none" | "changed" | "deleted";

export interface EditorTab {
  path: string;
  /** SHA256 of the content on disk as last loaded or saved. */
  sha256: string;
  dirty: boolean;
  external: ExternalState;
  /** Increments whenever the buffer was replaced from disk (editor must reload). */
  revision: number;
}

export interface EditorState {
  tabs: EditorTab[];
  active: string | null;
}

export const initialEditorState: EditorState = { tabs: [], active: null };

export type EditorAction =
  | { type: "opened"; path: string; sha256: string }
  | { type: "activated"; path: string }
  | { type: "closed"; path: string }
  | { type: "edited"; path: string }
  | { type: "saved"; path: string; sha256: string }
  | { type: "reloaded"; path: string; sha256: string }
  | { type: "externalChanged"; path: string; sha256: string | null };

function update(state: EditorState, path: string, f: (t: EditorTab) => EditorTab): EditorState {
  return { ...state, tabs: state.tabs.map((t) => (t.path === path ? f(t) : t)) };
}

export function reduceEditor(state: EditorState, action: EditorAction): EditorState {
  switch (action.type) {
    case "opened": {
      if (state.tabs.some((t) => t.path === action.path)) {
        return { ...state, active: action.path };
      }
      const tab: EditorTab = {
        path: action.path,
        sha256: action.sha256,
        dirty: false,
        external: "none",
        revision: 0,
      };
      return { tabs: [...state.tabs, tab], active: action.path };
    }
    case "activated":
      return state.tabs.some((t) => t.path === action.path)
        ? { ...state, active: action.path }
        : state;
    case "closed": {
      const index = state.tabs.findIndex((t) => t.path === action.path);
      if (index < 0) return state;
      const tabs = state.tabs.filter((t) => t.path !== action.path);
      const neighbour = tabs[Math.min(index, tabs.length - 1)];
      const active = state.active === action.path ? (neighbour?.path ?? null) : state.active;
      return { tabs, active };
    }
    case "edited":
      return update(state, action.path, (t) => (t.dirty ? t : { ...t, dirty: true }));
    case "saved":
      return update(state, action.path, (t) => ({
        ...t,
        sha256: action.sha256,
        dirty: false,
        external: "none",
      }));
    case "reloaded":
      return update(state, action.path, (t) => ({
        ...t,
        sha256: action.sha256,
        dirty: false,
        external: "none",
        revision: t.revision + 1,
      }));
    case "externalChanged":
      return update(state, action.path, (t) => {
        if (action.sha256 === t.sha256) return { ...t, external: "none" };
        return { ...t, external: action.sha256 === null ? "deleted" : "changed" };
      });
  }
}

/** Tabs whose disk content changed and that can be reloaded without losing edits. */
export function silentlyReloadable(tab: EditorTab): boolean {
  return tab.external === "changed" && !tab.dirty;
}
