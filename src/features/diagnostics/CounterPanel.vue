<script setup lang="ts">
import { computed, ref } from "vue";
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import { useCounterStore } from "../../stores/counter";

const { t } = useI18n();
const store = useCounterStore();
const { state, error } = storeToRefs(store);

const count = ref(100);
const intervalMs = ref(20);
const running = computed(() => state.value.status === "running");

function start(): void {
  void store.start(count.value, intervalMs.value);
}
</script>

<template>
  <section class="panel" aria-labelledby="counter-title">
    <h2 id="counter-title">{{ t("counter.title") }}</h2>
    <form class="row" @submit.prevent="start">
      <label for="counter-count">{{ t("counter.count") }}</label>
      <input id="counter-count" v-model.number="count" type="number" min="1" max="10000" />
      <label for="counter-interval">{{ t("counter.interval") }}</label>
      <input id="counter-interval" v-model.number="intervalMs" type="number" min="0" max="10000" />
      <button type="submit" :disabled="running">{{ t("counter.start") }}</button>
    </form>
    <progress
      :max="Math.max(state.total, 1)"
      :value="state.value"
      aria-labelledby="counter-title"
    />
    <p class="status" :class="state.status" role="status">
      {{ t(`counter.status.${state.status}`) }} –
      {{ t("counter.progress", { value: state.value, total: state.total, ticks: state.ticks }) }}
    </p>
    <p v-if="error" class="status error" role="alert">
      {{ t("common.error", { code: error.code, message: error.message }) }}
    </p>
  </section>
</template>
