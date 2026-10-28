<!-- SPIKE (M1, DESIGN.md §14 #4) – throwaway code, not for main. -->
<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import * as vscode from "vscode";
import { LogLevel } from "@codingame/monaco-vscode-api";
import {
  RegisteredFileSystemProvider,
  RegisteredMemoryFile,
  registerFileSystemOverlay,
} from "@codingame/monaco-vscode-files-service-override";
import { MonacoLanguageClient } from "monaco-languageclient";
import { EditorApp } from "monaco-languageclient/editorApp";
import { MonacoVscodeApiWrapper } from "monaco-languageclient/vscodeApiWrapper";
import { configureDefaultWorkerFactory } from "monaco-languageclient/workerFactory";
import { startTerraformLs, stats, stopTerraformLs } from "./tauriTransport";
import mainTf from "./main.tf?raw";

const root = import.meta.env.VITE_SPIKE_TF_ROOT as string;
const container = ref<HTMLElement | null>(null);
const log = ref<string[]>([]);
let client: MonacoLanguageClient | null = null;
let sessionId: number | null = null;
let editorApp: EditorApp | null = null;

function note(line: string): void {
  log.value.push(`${Math.round(performance.now())} ms: ${line}`);
  void invoke("spike_log", { line });
}
(window as unknown as { spikeLog: string[] }).spikeLog = log.value;

document.addEventListener("securitypolicyviolation", (e) =>
  note(`CSP violation: ${e.violatedDirective} blocked ${e.blockedURI}`),
);
window.addEventListener("error", (e) =>
  note(`window error: ${e.message} @ ${e.filename}:${String(e.lineno)}`),
);
window.addEventListener("unhandledrejection", (e) =>
  note(
    `unhandled rejection: ${e.reason instanceof ErrorEvent ? `${e.reason.message} @ ${e.reason.filename}` : e.reason instanceof Event ? `Event type=${e.reason.type} target=${String((e.reason.target as { constructor?: { name?: string } } | null)?.constructor?.name)}` : String(e.reason)}`,
  ),
);

onMounted(async () => {
  const t0 = performance.now();
  const fileUri = vscode.Uri.file(`${root}/main.tf`);
  const fsProvider = new RegisteredFileSystemProvider(false);
  fsProvider.registerFile(new RegisteredMemoryFile(fileUri, mainTf));
  registerFileSystemOverlay(1, fsProvider);

  const apiWrapper = new MonacoVscodeApiWrapper({
    $type: "extended",
    viewsConfig: { $type: "EditorService" },
    logLevel: LogLevel.Warning,
    userConfiguration: {
      json: JSON.stringify({
        "workbench.colorTheme": "Default Light Modern",
        "editor.wordBasedSuggestions": "off",
        "editor.semanticHighlighting.enabled": true,
      }),
    },
    extensions: [
      {
        config: {
          name: "terraform-spike",
          publisher: "iac-studio",
          version: "0.0.1",
          engines: { vscode: "*" },
          contributes: {
            languages: [{ id: "terraform", extensions: [".tf"], aliases: ["Terraform"] }],
          },
        },
      },
    ],
    monacoWorkerFactory: configureDefaultWorkerFactory,
    advanced: { enableExtHostWorker: false },
  });
  await apiWrapper.start();
  note(`vscode api started after ${Math.round(performance.now() - t0)} ms`);

  editorApp = new EditorApp({
    codeResources: {
      modified: { text: mainTf, uri: fileUri.path, enforceLanguageId: "terraform" },
    },
  });
  if (container.value) await editorApp.start(container.value);
  note(`editor started after ${Math.round(performance.now() - t0)} ms`);

  const transport = await startTerraformLs(root);
  sessionId = transport.sessionId;
  client = new MonacoLanguageClient({
    name: "terraform-ls (spike)",
    clientOptions: {
      documentSelector: [{ language: "terraform" }],
      workspaceFolder: { index: 0, name: "ws", uri: vscode.Uri.file(root) },
    },
    messageTransports: { reader: transport.reader, writer: transport.writer },
  });
  const tInit = performance.now();
  note("client.start() called");
  await client.start();
  note(
    `client started (initialize round trip incl. spawn) in ${Math.round(performance.now() - tInit)} ms; first server message after ${Math.round(stats.firstMessageMs)} ms`,
  );
  note(
    `server capabilities: ${Object.keys(client.initializeResult?.capabilities ?? {}).join(", ")}`,
  );

  vscode.languages.onDidChangeDiagnostics(() => {
    const diags = vscode.languages.getDiagnostics(fileUri);
    note(
      `diagnostics (${diags.length}): ${diags.map((d) => `L${d.range.start.line + 1} ${d.message}`).join(" | ")}`,
    );
  });

  // Programmatic completion probe inside the resource block (line 19, col 3)
  const tc = performance.now();
  const list = await vscode.commands.executeCommand<vscode.CompletionList>(
    "vscode.executeCompletionItemProvider",
    fileUri,
    new vscode.Position(18, 2),
  );
  note(
    `completion: ${list.items.length} items in ${Math.round(performance.now() - tc)} ms: ${list.items
      .slice(0, 6)
      .map((i) => (typeof i.label === "string" ? i.label : i.label.label))
      .join(", ")}`,
  );
  note(`messages received=${stats.received} sent=${stats.sent}`);
  setTimeout(() => void invoke("spike_close_window"), 1500);
});

onBeforeUnmount(async () => {
  const t = performance.now();
  await client?.stop();
  if (sessionId !== null) {
    const code = await stopTerraformLs(sessionId);
    note(
      `server stopped with exit code ${String(code)} in ${Math.round(performance.now() - t)} ms`,
    );
  }
  await editorApp?.dispose();
});
</script>

<template>
  <section>
    <!-- eslint-disable-next-line vue/no-bare-strings-in-template -->
    <h2>LSP spike (terraform-ls)</h2>
    <div ref="container" style="height: 420px; border: 1px solid #ccc"></div>
    <pre style="white-space: pre-wrap; font-size: 12px">{{ log.join("\n") }}</pre>
  </section>
</template>
