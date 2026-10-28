<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { ping, type Pong } from "../../lib/client";
import { toCommandError, type CommandError } from "../../lib/errors";

const { t } = useI18n();

const message = ref("hello");
const pending = ref(false);
const result = ref<Pong | null>(null);
const error = ref<CommandError | null>(null);

async function send(): Promise<void> {
  pending.value = true;
  error.value = null;
  result.value = null;
  try {
    result.value = await ping(message.value);
  } catch (err: unknown) {
    error.value = toCommandError(err);
  } finally {
    pending.value = false;
  }
}
</script>

<template>
  <section class="panel" aria-labelledby="ping-title">
    <h2 id="ping-title">{{ t("ping.title") }}</h2>
    <form class="row" @submit.prevent="send">
      <label for="ping-message">{{ t("ping.message") }}</label>
      <input id="ping-message" v-model="message" maxlength="256" />
      <button type="submit" :disabled="pending">{{ t("ping.send") }}</button>
    </form>
    <p v-if="result" class="status ok" role="status">
      {{ t("ping.reply", { message: result.message, version: result.coreVersion, os: result.os }) }}
    </p>
    <p v-if="error" class="status error" role="alert">
      {{ t("common.error", { code: error.code, message: error.message }) }}
      <template v-if="error.hint"><br />{{ t("common.hint", { hint: error.hint }) }}</template>
    </p>
  </section>
</template>
