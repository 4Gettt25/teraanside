<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { NodeKind } from "../../lib/generated/NodeKind";
import type { TreeNode } from "../../lib/generated/TreeNode";
import { flattenTree, navigate, type TreeKey, type TreeRow } from "./flatten";

const props = defineProps<{ root: TreeNode; activePath: string | null }>();
const emit = defineEmits<{ open: [path: string] }>();
const { t } = useI18n();

const expanded = ref(new Set<string>());
const focused = ref<string | null>(null);
const listEl = ref<HTMLElement | null>(null);
const rows = computed(() => flattenTree(props.root, expanded.value));

// Short, language-neutral badges; the full kind name is the accessible label.
const BADGES: Partial<Record<NodeKind, string>> = {
  terraformRoot: "TF",
  terraformModule: "MOD",
  terraformFile: "tf",
  terraformVars: "vars",
  terraformLock: "lock",
  ansibleConfig: "cfg",
  playbook: "PB",
  rolesDir: "RL",
  role: "R",
  roleSection: "§",
  inventory: "INV",
  groupVars: "GV",
  hostVars: "HV",
  collectionRequirements: "req",
  projectConfig: "IaC",
  symlink: "↪",
};

function isLeaf(row: TreeRow): boolean {
  return !row.expandable;
}

function toggle(path: string): void {
  const next = new Set(expanded.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  expanded.value = next;
}

function activate(row: TreeRow): void {
  focused.value = row.node.path;
  if (row.expandable) toggle(row.node.path);
  else if (row.node.kind !== "symlink") emit("open", row.node.path);
}

async function focusRow(path: string): Promise<void> {
  focused.value = path;
  await nextTick();
  listEl.value?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)?.focus();
}

const NAV_KEYS = new Set<string>([
  "ArrowDown",
  "ArrowUp",
  "ArrowLeft",
  "ArrowRight",
  "Home",
  "End",
]);

function onKeydown(event: KeyboardEvent, row: TreeRow): void {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    activate(row);
    return;
  }
  if (!NAV_KEYS.has(event.key)) return;
  event.preventDefault();
  const result = navigate(rows.value, row.node.path, event.key as TreeKey);
  if (result.type === "focus") void focusRow(result.path);
  else if (result.type === "expand" || result.type === "collapse") toggle(result.path);
}

watch(
  () => props.root,
  () => {
    if (focused.value === null && rows.value[0]) focused.value = rows.value[0].node.path;
  },
  { immediate: true },
);
</script>

<template>
  <nav class="explorer" :aria-label="t('explorer.title')">
    <ul ref="listEl" role="tree" :aria-label="root.name">
      <li
        v-for="row in rows"
        :key="row.node.path"
        role="treeitem"
        :data-path="row.node.path"
        :aria-level="row.depth + 1"
        :aria-expanded="row.expandable ? row.expanded : undefined"
        :aria-selected="row.node.path === activePath"
        :aria-label="`${row.node.name} – ${t(`kind.${row.node.kind}`)}`"
        :tabindex="row.node.path === focused ? 0 : -1"
        :class="{ active: row.node.path === activePath, leaf: isLeaf(row) }"
        :style="{ paddingLeft: `${String(0.25 + row.depth * 0.9)}rem` }"
        @click="activate(row)"
        @keydown="onKeydown($event, row)"
        @focus="focused = row.node.path"
      >
        <span class="twisty" aria-hidden="true">{{
          row.expandable ? (row.expanded ? "▾" : "▸") : ""
        }}</span>
        <span v-if="BADGES[row.node.kind]" class="badge" aria-hidden="true">{{
          BADGES[row.node.kind]
        }}</span>
        <span class="name">{{ row.node.name }}</span>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
.explorer ul {
  list-style: none;
  margin: 0;
  padding: 0;
}
[role="treeitem"] {
  display: flex;
  gap: 0.35rem;
  align-items: center;
  padding-block: 0.1rem;
  padding-right: 0.5rem;
  cursor: pointer;
  white-space: nowrap;
  font-size: 0.9rem;
}
[role="treeitem"]:focus-visible {
  outline: 2px solid #1d4ed8;
  outline-offset: -2px;
}
.active {
  background: #e5edff;
}
.twisty {
  width: 0.8rem;
  color: #555;
}
.badge {
  font-size: 0.65rem;
  border: 1px solid #999;
  border-radius: 3px;
  padding: 0 0.2rem;
  color: #333;
}
</style>
