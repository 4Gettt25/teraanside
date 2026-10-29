<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import type { SetupStep } from "../../lib/generated/SetupStep";
import type { TerraformFlavor } from "../../lib/generated/TerraformFlavor";
import { downloadFraction } from "../../stores/setupReducer";
import { useToolchainStore } from "../../stores/toolchain";

const props = defineProps<{ projectId: number }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const toolchain = useToolchainStore();
const { status, setup, error } = storeToRefs(toolchain);
const dialog = ref<HTMLDialogElement | null>(null);
const flavor = ref<TerraformFlavor>("opentofu");

const hasWork = computed(
  () =>
    !!status.value &&
    !status.value.ready &&
    !status.value.needsFlavorChoice &&
    (status.value.tools.some((tool) => !tool.installed && tool.error === undefined) ||
      (status.value.ansible !== undefined && !status.value.ansible.ready)),
);

onMounted(() => {
  dialog.value?.showModal();
});

function close(): void {
  if (setup.value.running) return;
  dialog.value?.close();
  emit("close");
}

function stepLabel(step: SetupStep): string {
  return step.kind === "tool"
    ? t("toolchain.installTool", { tool: t(`tool.${step.tool}`), version: step.version })
    : t("toolchain.createEnv", {
        core: step.ansibleCore,
        lint: step.ansibleLint,
        python: step.python,
      });
}

function percent(fraction: number | null): string {
  return fraction === null ? "" : `${String(Math.round(fraction * 100))} %`;
}
</script>

<template>
  <dialog ref="dialog" class="wizard" aria-labelledby="wizard-title" @cancel.prevent="close">
    <h2 id="wizard-title">{{ t("toolchain.title") }}</h2>
    <p>{{ t("toolchain.intro") }}</p>

    <fieldset v-if="status?.needsFlavorChoice" class="flavor">
      <legend>{{ t("toolchain.flavorQuestion") }}</legend>
      <label>
        <input v-model="flavor" type="radio" value="opentofu" name="flavor" />
        <strong>{{ t("tool.opentofu") }}</strong> – {{ t("toolchain.flavorOpentofu") }}
      </label>
      <label>
        <input v-model="flavor" type="radio" value="terraform" name="flavor" />
        <strong>{{ t("tool.terraform") }}</strong> – {{ t("toolchain.flavorTerraform") }}
      </label>
      <p class="note">{{ t("toolchain.flavorNote") }}</p>
      <button
        type="button"
        class="primary"
        @click="toolchain.chooseFlavor(props.projectId, flavor)"
      >
        {{ t("toolchain.flavorConfirm") }}
      </button>
    </fieldset>

    <section v-if="status && setup.steps.length === 0" aria-labelledby="needs-title">
      <h3 id="needs-title">{{ t("toolchain.required") }}</h3>
      <ul class="tools">
        <li
          v-for="tool in status.tools"
          :key="tool.tool"
          :class="tool.installed ? 'ok' : tool.error ? 'error' : 'missing'"
        >
          <strong>{{ t(`tool.${tool.tool}`) }}</strong>
          <span v-if="tool.version">{{ tool.version }}</span>
          <span v-if="tool.source" class="source">{{ t(`pinSource.${tool.source}`) }}</span>
          <span class="state">{{
            tool.installed
              ? t("toolchain.installed")
              : tool.error
                ? tool.error
                : t("toolchain.missing")
          }}</span>
        </li>
        <li
          v-if="status.ansible"
          :class="status.ansible.ready ? 'ok' : status.ansible.error ? 'error' : 'missing'"
        >
          <strong>{{ t("toolchain.ansibleEnv") }}</strong>
          <span>{{
            t("toolchain.envVersions", {
              core: status.ansible.ansibleCore,
              lint: status.ansible.ansibleLint,
              python: status.ansible.python,
            })
          }}</span>
          <span class="state">{{
            status.ansible.ready
              ? t("toolchain.installed")
              : (status.ansible.error ?? t("toolchain.missing"))
          }}</span>
        </li>
      </ul>
      <p v-if="status.ready" class="status ok" role="status">{{ t("toolchain.ready") }}</p>
    </section>

    <ol v-if="setup.steps.length > 0" class="steps" aria-live="polite">
      <li v-for="(s, i) in setup.steps" :key="i" :class="s.state">
        <span class="label">{{ stepLabel(s.step) }}</span>
        <span class="state">{{ t(`toolchain.step.${s.state}`) }}</span>
        <template v-if="s.state === 'running' && s.progress">
          <span class="phase"
            >{{ t(`toolchain.phase.${s.progress.phase}`) }}
            {{ percent(downloadFraction(s.progress)) }}</span
          >
          <progress
            v-if="downloadFraction(s.progress) !== null"
            :value="downloadFraction(s.progress) ?? 0"
            max="1"
            :aria-label="stepLabel(s.step)"
          />
        </template>
        <p v-if="s.error" class="status error" role="alert">{{ s.error }}</p>
        <details v-if="s.log.length > 0">
          <summary>{{ t("toolchain.log") }}</summary>
          <pre>{{ s.log.join("\n") }}</pre>
        </details>
      </li>
    </ol>
    <p
      v-if="setup.finished"
      class="status"
      :class="setup.finished.ok ? 'ok' : 'error'"
      role="status"
    >
      {{
        setup.finished.ok
          ? t("toolchain.done")
          : setup.finished.cancelled
            ? t("toolchain.cancelled")
            : t("toolchain.failed")
      }}
    </p>
    <p v-if="error" class="status error" role="alert">
      {{ t("common.error", { code: error.code, message: error.message }) }}
      <template v-if="error.hint"><br />{{ t("common.hint", { hint: error.hint }) }}</template>
    </p>

    <div class="actions">
      <button v-if="setup.running" type="button" @click="toolchain.cancel()">
        {{ t("toolchain.cancel") }}
      </button>
      <button v-else type="button" @click="close">{{ t("toolchain.close") }}</button>
      <button
        v-if="hasWork && !setup.running"
        type="button"
        class="primary"
        @click="toolchain.runSetup(props.projectId)"
      >
        {{ setup.finished && !setup.finished.ok ? t("toolchain.retry") : t("toolchain.start") }}
      </button>
    </div>
  </dialog>
</template>

<style scoped>
.wizard {
  width: min(44rem, 90vw);
  border: 1px solid #888;
  border-radius: 8px;
  padding: 1.25rem;
}
.flavor label {
  display: block;
  margin-block: 0.35rem;
}
.note {
  font-size: 0.85rem;
  color: #444;
}
.tools,
.steps {
  padding-left: 1.2rem;
}
.tools li,
.steps li {
  margin-block: 0.4rem;
}
.tools li > span,
.steps li > span {
  margin-left: 0.5rem;
}
.source {
  font-size: 0.8rem;
  color: #555;
}
/* State via symbol + text, not colour alone. */
.ok > .state::before,
.done > .state::before {
  content: "✓ ";
}
.missing > .state::before,
.pending > .state::before {
  content: "○ ";
}
.running > .state::before {
  content: "⟳ ";
}
.error > .state::before,
.failed > .state::before {
  content: "✗ ";
}
progress {
  display: block;
  width: 100%;
}
pre {
  max-height: 12rem;
  overflow: auto;
  font-size: 0.75rem;
  background: #f4f4f5;
  padding: 0.5rem;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
}
.primary {
  font-weight: 600;
}
</style>
