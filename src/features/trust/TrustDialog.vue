<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

defineProps<{ projectName: string; root: string }>();
const emit = defineEmits<{ trust: []; restricted: [] }>();
const { t } = useI18n();
const dialog = ref<HTMLDialogElement | null>(null);

onMounted(() => {
  dialog.value?.showModal();
});

function choose(trust: boolean): void {
  dialog.value?.close();
  if (trust) emit("trust");
  else emit("restricted");
}
</script>

<template>
  <dialog
    ref="dialog"
    class="trust-dialog"
    aria-labelledby="trust-title"
    aria-describedby="trust-body"
    @cancel.prevent="choose(false)"
  >
    <h2 id="trust-title">{{ t("trust.title", { name: projectName }) }}</h2>
    <p class="root">{{ root }}</p>
    <div id="trust-body">
      <p>{{ t("trust.explanation") }}</p>
      <ul>
        <li>{{ t("trust.risk.terraform") }}</li>
        <li>{{ t("trust.risk.ansible") }}</li>
        <li>{{ t("trust.risk.lsp") }}</li>
      </ul>
      <p>{{ t("trust.restrictedInfo") }}</p>
    </div>
    <div class="actions">
      <button type="button" @click="choose(false)">{{ t("trust.openRestricted") }}</button>
      <button type="button" class="primary" @click="choose(true)">{{ t("trust.trust") }}</button>
    </div>
  </dialog>
</template>

<style scoped>
.trust-dialog {
  max-width: 36rem;
  border: 1px solid #888;
  border-radius: 8px;
  padding: 1.25rem;
}
.root {
  font-family: monospace;
  color: #444;
  word-break: break-all;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  flex-wrap: wrap;
}
.primary {
  font-weight: 600;
}
</style>
