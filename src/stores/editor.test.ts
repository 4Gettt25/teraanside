import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const fileWrite = vi.fn<(...args: unknown[]) => Promise<string>>();
vi.mock("../lib/client", () => ({
  fileRead: () =>
    Promise.resolve({
      path: "legacy.tf",
      text: 'name = "Gr��e"\n',
      sha256: "abc",
      notUtf8: true,
    }),
  fileWrite: (...args: unknown[]) => fileWrite(...args),
  projectClose: vi.fn(),
  projectInitial: vi.fn(),
  projectOpenDialog: vi.fn(),
  projectOpenRecent: vi.fn(),
  projectRecent: vi.fn(),
  projectSetTrusted: vi.fn(),
  projectTree: vi.fn(),
  projectWatch: vi.fn(),
}));

import { useEditorStore } from "./editor";
import { useProjectStore } from "./project";

beforeEach(() => {
  setActivePinia(createPinia());
  fileWrite.mockReset();
});

describe("editor store with a Latin-1 file", () => {
  it("opens it read-only and never writes it, not even forced", async () => {
    useProjectStore().info = {
      id: 1,
      name: "p",
      root: "/p",
      trusted: false,
      config: { schema: 1, terraform: {}, ansible: {} },
    };
    const editor = useEditorStore();
    await editor.open("legacy.tf");
    expect(editor.activeTab).toMatchObject({ path: "legacy.tf", readOnly: true });
    editor.edited("legacy.tf");
    expect(editor.activeTab?.dirty).toBe(false);
    expect(await editor.save("legacy.tf")).toBe(false);
    expect(await editor.save("legacy.tf", true)).toBe(false);
    expect(fileWrite).not.toHaveBeenCalled();
  });
});
