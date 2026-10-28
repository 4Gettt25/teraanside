import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { createI18n } from "vue-i18n";
import en from "../../locales/en.json";
import type { TreeNode } from "../../lib/generated/TreeNode";
import ExplorerTree from "./ExplorerTree.vue";

const i18n = createI18n({ legacy: false, locale: "en", messages: { en } });
const root: TreeNode = {
  name: "repo",
  path: "",
  kind: "directory",
  children: [
    {
      name: "roles",
      path: "roles",
      kind: "rolesDir",
      children: [{ name: "a.yml", path: "roles/a.yml", kind: "file", children: [] }],
    },
    { name: "site.yml", path: "site.yml", kind: "playbook", children: [] },
  ],
};

function mountTree() {
  return mount(ExplorerTree, {
    props: { root, activePath: null },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });
}

describe("ExplorerTree", () => {
  it("renders an accessible tree with kind labels", () => {
    const w = mountTree();
    const items = w.findAll('[role="treeitem"]');
    expect(items.map((i) => i.attributes("data-path"))).toEqual(["roles", "site.yml"]);
    expect(items[0]?.attributes("aria-expanded")).toBe("false");
    expect(items[1]?.attributes("aria-label")).toBe("site.yml – Ansible playbook");
    expect(items[0]?.attributes("tabindex")).toBe("0");
    w.unmount();
  });

  it("expands with ArrowRight and opens files with Enter", async () => {
    const w = mountTree();
    await w.find('[data-path="roles"]').trigger("keydown", { key: "ArrowRight" });
    expect(w.find('[data-path="roles/a.yml"]').exists()).toBe(true);
    await w.find('[data-path="roles/a.yml"]').trigger("keydown", { key: "Enter" });
    expect(w.emitted("open")).toEqual([["roles/a.yml"]]);
    await w.find('[data-path="roles"]').trigger("keydown", { key: "ArrowLeft" });
    expect(w.find('[data-path="roles/a.yml"]').exists()).toBe(false);
    w.unmount();
  });
});
