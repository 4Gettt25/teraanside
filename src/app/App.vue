<script setup lang="ts">
import { useI18n } from "vue-i18n";
import CounterPanel from "../features/diagnostics/CounterPanel.vue";
import PingPanel from "../features/diagnostics/PingPanel.vue";
import LspSpike from "../spike/LspSpike.vue";

const { t } = useI18n();
</script>

<template>
  <main>
    <header>
      <h1>{{ t("app.title") }}</h1>
      <p>{{ t("app.subtitle") }}</p>
    </header>
    <PingPanel />
    <CounterPanel />
    <LspSpike />
  </main>
</template>

<style>
:root {
  font-family: system-ui, sans-serif;
  color: #111;
  background: #fff;
}
main {
  max-width: 64rem;
  margin: 0 auto;
  padding: 1rem;
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
.status.ok::before {
  content: "✓ ";
}
.status.error::before {
  content: "✗ ";
}
.status.running::before {
  content: "⟳ ";
}
.status.finished::before {
  content: "✓ ";
}
.status.idle::before {
  content: "○ ";
}
.status.finished {
  color: #1a6b2a;
}
progress {
  width: 100%;
}
.status.ok {
  color: #1a6b2a;
}
.status.error {
  color: #a11;
}
</style>
