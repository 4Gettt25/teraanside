import { describe, expect, it } from "vitest";
import type { TreeNode } from "../generated/TreeNode";
import { isAnsibleFile } from "./ansibleFiles";

const n = (path: string, kind: TreeNode["kind"], children: TreeNode[] = []): TreeNode => ({
  name: path.split("/").pop() ?? path,
  path,
  kind,
  children,
});

const root = n("", "directory", [
  n("site.yml", "playbook"),
  n("vars.yml", "file"),
  n("roles", "rolesDir", [
    n("roles/web", "role", [
      n("roles/web/tasks", "roleSection", [n("roles/web/tasks/main.yml", "file")]),
    ]),
  ]),
  n("group_vars", "groupVars", [n("group_vars/all.yml", "file")]),
  n("inventory", "inventory", [n("inventory/hosts.yml", "file")]),
  n(".github", "directory", [n(".github/ci.yml", "file")]),
]);

describe("isAnsibleFile", () => {
  it("recognises playbooks, role files, vars and inventories", () => {
    for (const p of [
      "site.yml",
      "roles/web/tasks/main.yml",
      "group_vars/all.yml",
      "inventory/hosts.yml",
    ]) {
      expect(isAnsibleFile(root, p, true), p).toBe(true);
    }
  });

  it("leaves other YAML and non-Ansible projects alone", () => {
    expect(isAnsibleFile(root, "vars.yml", true)).toBe(false);
    expect(isAnsibleFile(root, ".github/ci.yml", true)).toBe(false);
    expect(isAnsibleFile(root, "site.yml", false)).toBe(false);
    expect(isAnsibleFile(root, "roles/web/tasks/main.tf", true)).toBe(false);
    expect(isAnsibleFile(root, "missing/x.yml", true)).toBe(false);
  });
});
