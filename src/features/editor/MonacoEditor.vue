<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import type * as monacoApi from "@codingame/monaco-vscode-editor-api";
import { getBuffer, setBufferSource } from "./buffers";
import type { OpenModel } from "./monaco/setup";

const props = defineProps<{
  projectRoot: string;
  /** Active file. */
  path: string;
  /** All open tabs with their reload revision. */
  tabs: { path: string; revision: number }[];
  /** The active file must not be edited (non-UTF-8). */
  readOnly: boolean;
  label: string;
}>();
const emit = defineEmits<{ change: [path: string]; failed: [error: string] }>();

const container = ref<HTMLElement | null>(null);
const editor = shallowRef<monacoApi.editor.IStandaloneCodeEditor | null>(null);
const models = new Map<string, OpenModel>();
const viewStates = new Map<string, monacoApi.editor.ICodeEditorViewState | null>();
let setup: typeof import("./monaco/setup") | null = null;
let applyingFromDisk = false;

async function showPath(path: string): Promise<void> {
  const ed = editor.value;
  if (!ed || !setup) return;
  const previous = ed.getModel();
  const previousPath = [...models.entries()].find(([, m]) => m.model === previous)?.[0];
  if (previousPath) viewStates.set(previousPath, ed.saveViewState());

  let open = models.get(path);
  if (!open) {
    open = await setup.openModel(`${props.projectRoot}/${path}`, getBuffer(path) ?? "");
    models.set(path, open);
    const model = open.model;
    model.onDidChangeContent(() => {
      if (!applyingFromDisk) emit("change", path);
    });
    setBufferSource(path, () => model.getValue());
  }
  ed.setModel(open.model);
  ed.updateOptions({ readOnly: props.readOnly });
  const state = viewStates.get(path);
  if (state) ed.restoreViewState(state);
  ed.focus();
}

function reloadFromDisk(path: string): void {
  const open = models.get(path);
  const text = getBuffer(path);
  if (!open || text === undefined) return;
  applyingFromDisk = true;
  try {
    open.model.setValue(text);
  } finally {
    applyingFromDisk = false;
  }
  const model = open.model;
  setBufferSource(path, () => model.getValue());
}

onMounted(async () => {
  try {
    setup = await import("./monaco/setup");
    await setup.ensureMonaco();
    if (!container.value) return;
    editor.value = setup.createEditor(container.value, props.label);
    await showPath(props.path);
  } catch (err: unknown) {
    emit("failed", err instanceof Error ? err.message : String(err));
  }
});

watch(
  () => props.path,
  (path) => void showPath(path),
);
watch(
  () => props.readOnly,
  (readOnly) => editor.value?.updateOptions({ readOnly }),
);
const seenRevisions = new Map<string, number>();
watch(
  () => props.tabs,
  (tabs) => {
    // Reload models whose file was re-read from disk (also background tabs).
    for (const tab of tabs) {
      const seen = seenRevisions.get(tab.path);
      if (seen !== undefined && seen !== tab.revision) reloadFromDisk(tab.path);
      seenRevisions.set(tab.path, tab.revision);
    }
    // Dispose models of closed tabs.
    const open = new Set(tabs.map((t) => t.path));
    for (const [path, model] of models) {
      if (!open.has(path)) {
        model.dispose();
        models.delete(path);
        viewStates.delete(path);
        seenRevisions.delete(path);
      }
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  editor.value?.dispose();
  for (const model of models.values()) model.dispose();
  models.clear();
});
</script>

<template>
  <div ref="container" class="monaco-host"></div>
</template>

<style scoped>
.monaco-host {
  width: 100%;
  height: 100%;
}
</style>
