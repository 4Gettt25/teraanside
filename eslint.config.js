// @ts-check
import pluginVue from "eslint-plugin-vue";
import { defineConfigWithVueTs, vueTsConfigs } from "@vue/eslint-config-typescript";
import prettier from "eslint-config-prettier";

export default defineConfigWithVueTs(
  {
    ignores: ["dist/**", "src-tauri/**", "node_modules/**", "src/lib/generated/**"],
  },
  pluginVue.configs["flat/recommended"],
  vueTsConfigs.strictTypeChecked,
  {
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
      "vue/no-bare-strings-in-template": "error",
    },
  },
  {
    files: ["*.config.ts", "*.config.js"],
    extends: [vueTsConfigs.disableTypeChecked],
    // Tooling configs are type-checked via tsconfig.node.json, not the app project service.
    languageOptions: { parserOptions: { projectService: false } },
  },
  prettier,
);
