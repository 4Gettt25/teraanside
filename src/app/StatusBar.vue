<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import { useEditorStore } from "../stores/editor";
import { useProjectStore } from "../stores/project";

const { t } = useI18n();
const project = useProjectStore();
const editor = useEditorStore();
const { info, tree } = storeToRefs(project);
const { activeTab } = storeToRefs(editor);
</script>

<template>
  <footer class="status-bar">
    <template v-if="info">
      <span v-if="info.trusted" class="trust trusted">{{ t("status.trusted") }}</span>
      <span v-else class="trust restricted">
        {{ t("status.restricted") }}
        <button type="button" @click="project.setTrusted(true)">{{ t("trust.trust") }}</button>
      </span>
      <span v-if="tree?.detected.terraform">{{ t("status.terraform") }}</span>
      <span v-if="tree?.detected.ansible">{{ t("status.ansible") }}</span>
      <span v-if="tree?.truncated" class="warn">{{ t("status.truncated") }}</span>
    </template>
    <span class="spacer"></span>
    <span v-if="activeTab">{{ activeTab.path }}</span>
  </footer>
</template>

<style scoped>
.status-bar {
  display: flex;
  gap: 1rem;
  align-items: center;
  padding: 0.2rem 0.75rem;
  border-top: 1px solid #ccc;
  font-size: 0.8rem;
  background: #f4f4f5;
}
.spacer {
  flex: 1;
}
/* Trust state is conveyed by symbol + text, not colour alone. */
.trusted::before {
  content: "✓ ";
}
.restricted::before {
  content: "🔒 ";
}
.restricted button {
  font-size: 0.75rem;
  margin-left: 0.4rem;
}
.warn::before {
  content: "⚠ ";
}
</style>
