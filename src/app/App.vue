<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import EditorArea from "../features/editor/EditorArea.vue";
import ExplorerTree from "../features/explorer/ExplorerTree.vue";
import TrustDialog from "../features/trust/TrustDialog.vue";
import WelcomeView from "../features/welcome/WelcomeView.vue";
import { useEditorStore } from "../stores/editor";
import { useProjectStore } from "../stores/project";
import StatusBar from "./StatusBar.vue";

const { t } = useI18n();
const project = useProjectStore();
const editor = useEditorStore();
const { info, tree, trustPending, error: projectError } = storeToRefs(project);
const { state: editorState } = storeToRefs(editor);

async function closeProject(): Promise<void> {
  editor.closeAll();
  await project.close();
}

function onKeydown(event: KeyboardEvent): void {
  if (!(event.ctrlKey || event.metaKey)) return;
  const key = event.key.toLowerCase();
  if (key === "o") {
    event.preventDefault();
    void project.openDialog(t("welcome.dialogTitle"));
  } else if (key === "s" && editorState.value.active) {
    event.preventDefault();
    void editor.save(editorState.value.active);
  } else if (key === "w" && editorState.value.active) {
    event.preventDefault();
    editor.close(editorState.value.active);
  }
}

onMounted(async () => {
  window.addEventListener("keydown", onKeydown);
  const file = await project.openInitial();
  if (file) await editor.open(file);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <div class="shell">
    <WelcomeView v-if="!info" />
    <template v-else>
      <header class="toolbar">
        <strong>{{ info.name }}</strong>
        <span class="root">{{ info.root }}</span>
        <span class="spacer"></span>
        <button type="button" @click="project.openDialog(t('welcome.dialogTitle'))">
          {{ t("toolbar.open") }}
        </button>
        <button type="button" @click="closeProject">{{ t("toolbar.close") }}</button>
      </header>
      <div class="workbench">
        <aside class="sidebar">
          <ExplorerTree
            v-if="tree"
            :root="tree.root"
            :active-path="editorState.active"
            @open="editor.open"
          />
        </aside>
        <EditorArea />
      </div>
      <TrustDialog
        v-if="trustPending"
        :project-name="info.name"
        :root="info.root"
        @trust="project.setTrusted(true)"
        @restricted="project.dismissTrust()"
      />
    </template>
    <p v-if="projectError" class="global-error" role="alert">
      {{ t("common.error", { code: projectError.code, message: projectError.message }) }}
    </p>
    <StatusBar />
  </div>
</template>

<style>
:root {
  font-family: system-ui, sans-serif;
  color: #111;
  background: #fff;
}
body {
  margin: 0;
}
.panel {
  border: 1px solid #ccc;
  border-radius: 6px;
  padding: 0.75rem 1rem;
  margin-block: 1rem;
}
.row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  flex-wrap: wrap;
}
/* Status is conveyed by symbol + text, not by colour alone. */
.status.ok::before,
.status.finished::before {
  content: "✓ ";
}
.status.error::before {
  content: "✗ ";
}
.status.running::before {
  content: "⟳ ";
}
.status.idle::before {
  content: "○ ";
}
.status.ok,
.status.finished {
  color: #1a6b2a;
}
.status.error {
  color: #a11;
}
progress {
  width: 100%;
}
</style>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
}
.toolbar {
  display: flex;
  gap: 0.75rem;
  align-items: center;
  padding: 0.35rem 0.75rem;
  border-bottom: 1px solid #ccc;
}
.root {
  font-family: monospace;
  font-size: 0.8rem;
  color: #444;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.spacer {
  flex: 1;
}
.workbench {
  flex: 1;
  display: grid;
  grid-template-columns: minmax(12rem, 18rem) 1fr;
  min-height: 0;
}
.sidebar {
  border-right: 1px solid #ccc;
  overflow: auto;
}
.global-error {
  margin: 0;
  padding: 0.4rem 0.75rem;
  background: #fdecec;
}
</style>
