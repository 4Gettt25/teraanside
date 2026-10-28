<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import { useEditorStore } from "../../stores/editor";
import PlainEditor from "./PlainEditor.vue";

const { t } = useI18n();
const editor = useEditorStore();
const { state, activeTab, error } = storeToRefs(editor);

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

    <div v-if="activeTab?.external === 'changed'" class="notice" role="alert">
      {{ t("editor.externalChanged") }}
      <button type="button" @click="editor.reload(activeTab.path)">{{ t("editor.reload") }}</button>
      <button type="button" @click="editor.save(activeTab.path, true)">
        {{ t("editor.overwrite") }}
      </button>
    </div>
    <div v-else-if="activeTab?.external === 'deleted'" class="notice" role="alert">
      {{ t("editor.externalDeleted") }}
    </div>
    <p v-if="error" class="notice error" role="alert">
      {{ t("common.error", { code: error.code, message: error.message }) }}
    </p>

    <div class="editor-host">
      <PlainEditor
        v-if="activeTab"
        :key="activeTab.path"
        :path="activeTab.path"
        :revision="activeTab.revision"
        :label="t('editor.contentOf', { path: activeTab.path })"
        @change="(text) => editor.edited(activeTab!.path, text)"
      />
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
