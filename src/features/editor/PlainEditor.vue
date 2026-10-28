<script setup lang="ts">
// Fallback editor (no syntax highlighting) used until/unless Monaco is available.
import { ref, watch } from "vue";
import { getBuffer } from "./buffers";

const props = defineProps<{ path: string; revision: number; label: string }>();
const emit = defineEmits<{ change: [text: string] }>();
const text = ref(getBuffer(props.path) ?? "");

watch(
  () => [props.path, props.revision] as const,
  () => {
    text.value = getBuffer(props.path) ?? "";
  },
);

function onInput(event: Event): void {
  const value = (event.target as HTMLTextAreaElement).value;
  text.value = value;
  emit("change", value);
}
</script>

<template>
  <textarea
    class="plain-editor"
    :value="text"
    :aria-label="label"
    spellcheck="false"
    @input="onInput"
  ></textarea>
</template>

<style scoped>
.plain-editor {
  width: 100%;
  height: 100%;
  box-sizing: border-box;
  border: none;
  resize: none;
  font-family: ui-monospace, monospace;
  font-size: 0.9rem;
  padding: 0.5rem;
}
</style>
