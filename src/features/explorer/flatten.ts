import type { TreeNode } from "../../lib/generated/TreeNode";

export interface TreeRow {
  node: TreeNode;
  depth: number;
  parentPath: string | null;
  expandable: boolean;
  expanded: boolean;
}

/** Visible rows of the tree (root itself excluded) for the given expanded paths. */
export function flattenTree(root: TreeNode, expanded: ReadonlySet<string>): TreeRow[] {
  const rows: TreeRow[] = [];
  const visit = (nodes: TreeNode[], depth: number, parentPath: string | null): void => {
    for (const node of nodes) {
      const expandable = node.children.length > 0;
      const isExpanded = expandable && expanded.has(node.path);
      rows.push({ node, depth, parentPath, expandable, expanded: isExpanded });
      if (isExpanded) visit(node.children, depth + 1, node.path);
    }
  };
  visit(root.children, 0, null);
  return rows;
}

export type TreeKey = "ArrowDown" | "ArrowUp" | "ArrowLeft" | "ArrowRight" | "Home" | "End";

export type TreeNavResult =
  | { type: "focus"; path: string }
  | { type: "expand"; path: string }
  | { type: "collapse"; path: string }
  | { type: "none" };

/** WAI-ARIA tree keyboard navigation as a pure function. */
export function navigate(rows: TreeRow[], focused: string | null, key: TreeKey): TreeNavResult {
  if (rows.length === 0) return { type: "none" };
  const index = rows.findIndex((r) => r.node.path === focused);
  const row = index >= 0 ? rows[index] : undefined;
  const focusAt = (i: number): TreeNavResult => {
    const target = rows[Math.max(0, Math.min(rows.length - 1, i))];
    return target ? { type: "focus", path: target.node.path } : { type: "none" };
  };
  if (!row) return focusAt(0);
  switch (key) {
    case "ArrowDown":
      return focusAt(index + 1);
    case "ArrowUp":
      return focusAt(index - 1);
    case "Home":
      return focusAt(0);
    case "End":
      return focusAt(rows.length - 1);
    case "ArrowRight":
      if (row.expandable && !row.expanded) return { type: "expand", path: row.node.path };
      if (row.expanded) return focusAt(index + 1);
      return { type: "none" };
    case "ArrowLeft":
      if (row.expanded) return { type: "collapse", path: row.node.path };
      if (row.parentPath !== null) return { type: "focus", path: row.parentPath };
      return { type: "none" };
  }
}
