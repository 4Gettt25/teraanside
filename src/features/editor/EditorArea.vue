<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from "vue";
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import { reportFrontendError } from "../../lib/client";
import { useEditorStore } from "../../stores/editor";
import { useProjectStore } from "../../stores/project";
import PlainEditor from "./PlainEditor.vue";

// Monaco is several MB: load it only when the first file is opened.
const MonacoEditor = defineAsyncComponent(() => import("./MonacoEditor.vue"));

const { t } = useI18n();
const editor = useEditorStore();
const project = useProjectStore();
const { state, activeTab, error } = storeToRefs(editor);
const { info } = storeToRefs(project);
const monacoFailed = ref(false);
const tabRevisions = computed(() =>
  state.value.tabs.map((tab) => ({ path: tab.path, revision: tab.revision })),
);

function onMonacoFailed(message: string): void {
  monacoFailed.value = true;
  reportFrontendError(`monaco failed to load: ${message}`);
}

function fileName(path: string): string {
  return path.split("/").pop() ?? path;
}
</script>

<template>
  <section class="editor-area" :aria-label="t('editor.title')">
    <div v-if="state.tabs.length > 0" role="tablist" class="tabs" :aria-label="t('editor.tabs')">
      <div
        v-for="tab in state.tabs"
        :key="tab.path"
        class="tab"
        :class="{ active: tab.path === state.active }"
      >
        <button
          type="button"
          role="tab"
          :aria-selected="tab.path === state.active"
          :title="tab.path"
          @click="editor.dispatch({ type: 'activated', path: tab.path })"
        >
          {{ fileName(tab.path) }}
          <span v-if="tab.dirty" class="dirty">{{ t("editor.unsaved") }}</span>
        </button>
        <button
          type="button"
          class="close"
          :aria-label="t('editor.closeTab', { name: fileName(tab.path) })"
          @click="editor.close(tab.path)"
        ></button>
      </div>
    </div>

    <p v-if="activeTab?.readOnly" class="notice" role="status">
      {{ t("editor.notUtf8") }}
    </p>
    <div v-if="activeTab?.external === 'changed'" class="notice" role="alert">
      {{ t("editor.externalChanged") }}
      <button type="button" @click="editor.reload(activeTab.path)">{{ t("editor.reload") }}</button>
      <button v-if="!activeTab.readOnly" type="button" @click="editor.save(activeTab.path, true)">
        {{ t("editor.overwrite") }}
      </button>
    </div>
    <div v-else-if="activeTab?.external === 'deleted'" class="notice" role="alert">
      {{ t("editor.externalDeleted") }}
    </div>
    <p v-if="error" class="notice error" role="alert">
      {{ t("common.error", { code: error.code, message: error.message }) }}
    </p>

    <p v-if="monacoFailed" class="notice" role="status">{{ t("editor.fallback") }}</p>
    <div class="editor-host">
      <template v-if="activeTab && info">
        <MonacoEditor
          v-if="!monacoFailed"
          :project-root="info.root"
          :path="activeTab.path"
          :tabs="tabRevisions"
          :read-only="activeTab.readOnly"
          :label="t('editor.contentOf', { path: activeTab.path })"
          @change="editor.edited"
          @failed="onMonacoFailed"
        />
        <PlainEditor
          v-else
          :key="activeTab.path"
          :path="activeTab.path"
          :revision="activeTab.revision"
          :read-only="activeTab.readOnly"
          :label="t('editor.contentOf', { path: activeTab.path })"
          @change="editor.edited"
        />
      </template>
      <p v-else class="empty">{{ t("editor.empty") }}</p>
    </div>
  </section>
</template>

<style scoped>
.editor-area {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  height: 100%;
}
.tabs {
  display: flex;
  overflow-x: auto;
  border-bottom: 1px solid #ccc;
}
.tab {
  display: flex;
  align-items: center;
  border-right: 1px solid #ddd;
}
.tab.active {
  background: #fff;
  box-shadow: inset 0 -2px 0 #1d4ed8;
}
.tab button {
  border: none;
  background: transparent;
  padding: 0.35rem 0.5rem;
  cursor: pointer;
}
.close::before {
  content: "×";
}
.dirty {
  font-size: 0.75rem;
  color: #92400e;
}
.notice {
  margin: 0;
  padding: 0.4rem 0.75rem;
  background: #fff7e6;
  border-bottom: 1px solid #e5c07b;
}
.notice.error {
  background: #fdecec;
}
.editor-host {
  flex: 1;
  min-height: 0;
}
.empty {
  color: #555;
  padding: 1rem;
}
</style>
