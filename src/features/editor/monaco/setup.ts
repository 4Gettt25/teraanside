// Loaded lazily (dynamic import) when the first file is opened: Monaco and the
// VS Code services are several MB.
import "@codingame/monaco-vscode-yaml-default-extension";
import { LogLevel } from "@codingame/monaco-vscode-api";
import * as monaco from "@codingame/monaco-vscode-editor-api";
import {
  RegisteredFileSystemProvider,
  RegisteredMemoryFile,
  registerFileSystemOverlay,
} from "@codingame/monaco-vscode-files-service-override";
import { MonacoVscodeApiWrapper } from "monaco-languageclient/vscodeApiWrapper";
import { configureDefaultWorkerFactory } from "monaco-languageclient/workerFactory";
import * as vscode from "vscode";
import { hclExtension } from "./languages";

let started: Promise<void> | null = null;
const fileSystem = new RegisteredFileSystemProvider(false);

/** Initializes the VS Code services once per app lifetime. */
export function ensureMonaco(): Promise<void> {
  started ??= (async () => {
    // The provider's enum types come from a second declaration path of the same
    // @codingame package (structurally identical, vue-tsc accepts it).
    // eslint-disable-next-line @typescript-eslint/no-unsafe-enum-assignment
    registerFileSystemOverlay(1, fileSystem);
    const wrapper = new MonacoVscodeApiWrapper({
      $type: "extended",
      viewsConfig: { $type: "EditorService" },
      logLevel: LogLevel.Warning,
      userConfiguration: {
        json: JSON.stringify({
          "workbench.colorTheme": "Default Light Modern",
          "editor.wordBasedSuggestions": "off",
          "editor.minimap.enabled": false,
          "editor.renderWhitespace": "boundary",
        }),
      },
      extensions: [hclExtension],
      monacoWorkerFactory: configureDefaultWorkerFactory,
      // Language clients run in the window; no extension host worker needed.
      advanced: { enableExtHostWorker: false },
    });
    await wrapper.start();
  })();
  return started;
}

export interface OpenModel {
  model: monaco.editor.ITextModel;
  dispose(): void;
}

/** Creates (or re-uses) the text model for a project file. */
export async function openModel(absolutePath: string, text: string): Promise<OpenModel> {
  const uri = vscode.Uri.file(absolutePath);
  const fileRegistration = fileSystem.registerFile(new RegisteredMemoryFile(uri, text));
  const ref = await monaco.editor.createModelReference(uri);
  return {
    model: ref.object.textEditorModel,
    dispose() {
      ref.dispose();
      fileRegistration.dispose();
    },
  };
}

export function setLanguage(model: monaco.editor.ITextModel, languageId: string): void {
  monaco.editor.setModelLanguage(model, languageId);
}

export function createEditor(
  container: HTMLElement,
  ariaLabel: string,
): monaco.editor.IStandaloneCodeEditor {
  return monaco.editor.create(container, {
    automaticLayout: true,
    ariaLabel,
    fontSize: 13,
    scrollBeyondLastLine: false,
  });
}
