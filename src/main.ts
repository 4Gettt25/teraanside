import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./app/App.vue";
import { i18n } from "./app/i18n";
import { reportFrontendError } from "./lib/client";

window.addEventListener("error", (e) => {
  reportFrontendError(`${e.message} @ ${e.filename}:${String(e.lineno)}`);
});
window.addEventListener("unhandledrejection", (e: PromiseRejectionEvent) => {
  reportFrontendError(`unhandled rejection: ${String(e.reason)}`);
});

const app = createApp(App);
app.config.errorHandler = (err) => {
  reportFrontendError(`vue: ${String(err)}`);
};
app.use(createPinia()).use(i18n).mount("#app");
