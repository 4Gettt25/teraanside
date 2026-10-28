import { describe, expect, it } from "vitest";
import {
  initialEditorState,
  reduceEditor,
  silentlyReloadable,
  type EditorAction,
} from "./editorReducer";

const run = (actions: EditorAction[]) => actions.reduce(reduceEditor, initialEditorState);

describe("reduceEditor", () => {
  it("opens tabs once and activates them", () => {
    const s = run([
      { type: "opened", path: "a.tf", sha256: "1" },
      { type: "opened", path: "b.yml", sha256: "2" },
      { type: "opened", path: "a.tf", sha256: "1" },
    ]);
    expect(s.tabs.map((t) => t.path)).toEqual(["a.tf", "b.yml"]);
    expect(s.active).toBe("a.tf");
  });

  it("closing the active tab activates the neighbour", () => {
    const s = run([
      { type: "opened", path: "a", sha256: "1" },
      { type: "opened", path: "b", sha256: "2" },
      { type: "opened", path: "c", sha256: "3" },
      { type: "activated", path: "b" },
      { type: "closed", path: "b" },
    ]);
    expect(s.active).toBe("c");
    expect(
      run([
        { type: "opened", path: "a", sha256: "1" },
        { type: "closed", path: "a" },
      ]).active,
    ).toBeNull();
  });

  it("tracks dirty state through edit and save", () => {
    const s = run([
      { type: "opened", path: "a", sha256: "1" },
      { type: "edited", path: "a" },
    ]);
    expect(s.tabs[0]?.dirty).toBe(true);
    const saved = reduceEditor(s, { type: "saved", path: "a", sha256: "2" });
    expect(saved.tabs[0]).toMatchObject({ dirty: false, sha256: "2" });
  });

  it("classifies external changes", () => {
    const base = run([{ type: "opened", path: "a", sha256: "1" }]);
    const same = reduceEditor(base, { type: "externalChanged", path: "a", sha256: "1" });
    expect(same.tabs[0]?.external).toBe("none");
    const changed = reduceEditor(base, { type: "externalChanged", path: "a", sha256: "9" });
    expect(changed.tabs[0]?.external).toBe("changed");
    expect(changed.tabs[0] && silentlyReloadable(changed.tabs[0])).toBe(true);
    const dirtyChanged = reduceEditor(reduceEditor(base, { type: "edited", path: "a" }), {
      type: "externalChanged",
      path: "a",
      sha256: "9",
    });
    expect(dirtyChanged.tabs[0] && silentlyReloadable(dirtyChanged.tabs[0])).toBe(false);
    const deleted = reduceEditor(base, { type: "externalChanged", path: "a", sha256: null });
    expect(deleted.tabs[0]?.external).toBe("deleted");
  });

  it("reload bumps the revision and clears flags", () => {
    const s = run([
      { type: "opened", path: "a", sha256: "1" },
      { type: "edited", path: "a" },
      { type: "externalChanged", path: "a", sha256: "2" },
      { type: "reloaded", path: "a", sha256: "2" },
    ]);
    expect(s.tabs[0]).toMatchObject({ dirty: false, external: "none", revision: 1, sha256: "2" });
  });
});
