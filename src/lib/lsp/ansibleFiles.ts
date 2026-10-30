import type { NodeKind } from "../generated/NodeKind";
import type { TreeNode } from "../generated/TreeNode";

const ANSIBLE_CONTAINERS = new Set<NodeKind>([
  "rolesDir",
  "role",
  "roleSection",
  "groupVars",
  "hostVars",
  "inventory",
]);

/**
 * Whether a YAML file belongs to Ansible (and should use the `ansible` language
 * so that ansible-language-server handles it): playbooks, everything below
 * roles/, group_vars/, host_vars/ and inventories of a detected Ansible project.
 */
export function isAnsibleFile(root: TreeNode, path: string, ansibleProject: boolean): boolean {
  if (!ansibleProject || !/\.ya?ml$/.test(path)) return false;
  const parts = path.split("/");
  let node: TreeNode = root;
  for (let i = 0; i < parts.length; i++) {
    const childPath = parts.slice(0, i + 1).join("/");
    const child = node.children.find((c) => c.path === childPath);
    if (!child) return false;
    if (child.kind === "playbook" || ANSIBLE_CONTAINERS.has(child.kind)) return true;
    node = child;
  }
  return false;
}
