import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [vue()],
  // monaco-languageclient docs: exactly one `vscode` API instance in the bundle
  resolve: { dedupe: ["vscode"] },
  worker: { format: "es" as const },

  // Options tailored for `tauri dev` / `tauri build`:
  // 1. prevent Vite from obscuring Rust errors
  clearScreen: false,
  // 2. Tauri expects a fixed port, fail if it is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. do not watch the Rust side
      ignored: ["**/src-tauri/**"],
    },
  },
}));
