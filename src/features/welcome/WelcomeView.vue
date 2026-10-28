<script setup lang="ts">
import { onMounted } from "vue";
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import { useProjectStore } from "../../stores/project";
import CounterPanel from "../diagnostics/CounterPanel.vue";
import PingPanel from "../diagnostics/PingPanel.vue";

const { t } = useI18n();
const project = useProjectStore();
const { recent } = storeToRefs(project);

onMounted(() => void project.loadRecent());
</script>

<template>
  <main class="welcome">
    <h1>{{ t("app.title") }}</h1>
    <p>{{ t("welcome.intro") }}</p>
    <button type="button" class="primary" @click="project.openDialog(t('welcome.dialogTitle'))">
      {{ t("welcome.open") }}
    </button>
    <p class="shortcut">{{ t("welcome.shortcut") }}</p>

    <section v-if="recent.length > 0" aria-labelledby="recent-title">
      <h2 id="recent-title">{{ t("welcome.recent") }}</h2>
      <ul class="recent">
        <li v-for="r in recent" :key="r.path">
          <button type="button" :disabled="!r.exists" @click="project.openRecent(r.path)">
            <strong>{{ r.name }}</strong>
            <span class="path">{{ r.path }}</span>
            <span v-if="!r.exists" class="missing">{{ t("welcome.missing") }}</span>
          </button>
        </li>
      </ul>
    </section>

    <details class="diagnostics">
      <summary>{{ t("welcome.diagnostics") }}</summary>
      <PingPanel />
      <CounterPanel />
    </details>
  </main>
</template>

<style scoped>
.welcome {
  max-width: 48rem;
  margin: 0 auto;
  padding: 2rem 1rem;
}
.primary {
  font-size: 1rem;
  padding: 0.5rem 1rem;
}
.shortcut {
  color: #555;
  font-size: 0.85rem;
}
.recent {
  list-style: none;
  padding: 0;
}
.recent button {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  width: 100%;
  text-align: left;
  padding: 0.4rem 0.6rem;
  margin-block: 0.25rem;
  border: 1px solid #ccc;
  border-radius: 6px;
  background: #fff;
  cursor: pointer;
}
.path {
  font-family: monospace;
  font-size: 0.8rem;
  color: #444;
}
.missing {
  color: #a11;
  font-size: 0.8rem;
}
.diagnostics {
  margin-top: 2rem;
}
</style>
