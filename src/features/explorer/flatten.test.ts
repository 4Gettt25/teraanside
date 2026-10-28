import { describe, expect, it } from "vitest";
import type { TreeNode } from "../../lib/generated/TreeNode";
import { flattenTree, navigate } from "./flatten";

const leaf = (path: string): TreeNode => ({
  name: path.split("/").pop() ?? path,
  path,
  kind: "file",
  children: [],
});
const root: TreeNode = {
  name: "repo",
  path: "",
  kind: "directory",
  children: [
    {
      name: "roles",
      path: "roles",
      kind: "rolesDir",
      children: [
        { name: "web", path: "roles/web", kind: "role", children: [leaf("roles/web/x.yml")] },
      ],
    },
    leaf("site.yml"),
  ],
};

describe("flattenTree", () => {
  it("shows only expanded branches", () => {
    expect(flattenTree(root, new Set()).map((r) => r.node.path)).toEqual(["roles", "site.yml"]);
    const rows = flattenTree(root, new Set(["roles", "roles/web"]));
    expect(rows.map((r) => [r.node.path, r.depth])).toEqual([
      ["roles", 0],
      ["roles/web", 1],
      ["roles/web/x.yml", 2],
      ["site.yml", 0],
    ]);
    expect(rows[1]?.parentPath).toBe("roles");
  });
});

describe("navigate", () => {
  const collapsed = flattenTree(root, new Set());
  const open = flattenTree(root, new Set(["roles"]));

  it("moves focus up and down within bounds", () => {
    expect(navigate(collapsed, "roles", "ArrowDown")).toEqual({ type: "focus", path: "site.yml" });
    expect(navigate(collapsed, "site.yml", "ArrowDown")).toEqual({
      type: "focus",
      path: "site.yml",
    });
    expect(navigate(collapsed, "roles", "ArrowUp")).toEqual({ type: "focus", path: "roles" });
    expect(navigate(collapsed, "roles", "End")).toEqual({ type: "focus", path: "site.yml" });
  });

  it("expands, enters and collapses with left/right", () => {
    expect(navigate(collapsed, "roles", "ArrowRight")).toEqual({ type: "expand", path: "roles" });
    expect(navigate(open, "roles", "ArrowRight")).toEqual({ type: "focus", path: "roles/web" });
    expect(navigate(open, "roles/web", "ArrowLeft")).toEqual({ type: "focus", path: "roles" });
    expect(navigate(open, "roles", "ArrowLeft")).toEqual({ type: "collapse", path: "roles" });
    expect(navigate(open, "site.yml", "ArrowRight")).toEqual({ type: "none" });
  });

  it("focuses the first row when nothing is focused", () => {
    expect(navigate(collapsed, null, "ArrowDown")).toEqual({ type: "focus", path: "roles" });
    expect(navigate([], null, "ArrowDown")).toEqual({ type: "none" });
  });
});
