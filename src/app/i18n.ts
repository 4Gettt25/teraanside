import { createI18n } from "vue-i18n";
import de from "../locales/de.json";
import en from "../locales/en.json";

export type MessageSchema = typeof en;

const fallbackLocale = "en";

function detectLocale(): "de" | "en" {
  return navigator.language.toLowerCase().startsWith("de") ? "de" : fallbackLocale;
}

export const i18n = createI18n<[MessageSchema], "de" | "en">({
  // Composition API mode; `legacy` still defaults to true in v11 and is only removed in v12.
  // eslint-disable-next-line @typescript-eslint/no-deprecated
  legacy: false,
  locale: detectLocale(),
  fallbackLocale,
  messages: { de, en },
});
