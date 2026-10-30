// Language clients (monaco-languageclient) talking to servers in the core via
// Tauri channels (DESIGN.md §4.6, §14.2). Loaded with the Monaco chunk.
import { MonacoLanguageClient } from "monaco-languageclient";
import { CloseAction, ErrorAction } from "vscode-languageclient/browser";
import {
  AbstractMessageReader,
  AbstractMessageWriter,
  type DataCallback,
  type Disposable,
  type Message,
  type MessageReader,
  type MessageWriter,
} from "vscode-languageserver-protocol/browser";
import * as vscode from "vscode";
import { lspSend, lspStart, lspStop, type LspEvent, type ServerKind } from "../../../lib/client";
import { toCommandError } from "../../../lib/errors";
import { allowStart } from "../../../lib/lsp/restartPolicy";
import type { LspState } from "../../../stores/lsp";

const LANGUAGE: Record<ServerKind, string> = { terraform: "terraform", ansible: "ansible" };
const NAME: Record<ServerKind, string> = {
  terraform: "terraform-ls",
  ansible: "ansible-language-server",
};

class ChannelReader extends AbstractMessageReader implements MessageReader {
  private callback: DataCallback | null = null;
  private readonly early: Message[] = [];

  push(msg: Message): void {
    if (this.callback) this.callback(msg);
    else this.early.push(msg);
  }

  closed(error: string | null): void {
    if (error !== null) this.fireError(new Error(error));
    this.fireClose();
  }

  listen(callback: DataCallback): Disposable {
    this.callback = callback;
    for (const m of this.early.splice(0)) callback(m);
    return {
      dispose: () => {
        this.callback = null;
      },
    };
  }
}

class InvokeWriter extends AbstractMessageWriter implements MessageWriter {
  private count = 0;

  constructor(private readonly session: number) {
    super();
  }

  async write(msg: Message): Promise<void> {
    this.count++;
    try {
      await lspSend(this.session, msg);
    } catch (err: unknown) {
      this.fireError(err, msg, this.count);
      throw err;
    }
  }

  end(): void {}
}

interface Running {
  session: number;
  client: MonacoLanguageClient;
  stopping: boolean;
}

const running = new Map<ServerKind, Running>();
const starts = new Map<ServerKind, number[]>();

type StateSink = (kind: ServerKind, state: LspState) => void;

/** Starts the client+server for `kind` unless already running. */
export async function ensureClient(
  kind: ServerKind,
  projectId: number,
  root: string,
  report: StateSink,
): Promise<void> {
  if (running.has(kind)) return;
  const allowed = allowStart(starts.get(kind) ?? [], Date.now());
  if (allowed === null) {
    report(kind, { state: "failed", message: "restart limit reached" });
    return;
  }
  starts.set(kind, allowed);
  report(kind, { state: "starting" });

  const reader = new ChannelReader();
  let session: number;
  try {
    session = await lspStart(projectId, kind, (event: LspEvent) => {
      if (event.type === "message") {
        reader.push(event.payload as Message);
        return;
      }
      const entry = running.get(kind);
      const crashed = event.crashed && !(entry?.stopping ?? false);
      const detail =
        event.error ?? event.stderrTail.at(-1) ?? `exit ${String(event.code ?? event.signal)}`;
      reader.closed(crashed ? detail : null);
      running.delete(kind);
      if (crashed) {
        report(kind, { state: "failed", message: detail });
        // client-driven restart; core and allowStart() cap the attempts
        setTimeout(() => void ensureClient(kind, projectId, root, report), 1000);
      } else {
        report(kind, { state: "off" });
      }
    });
  } catch (err: unknown) {
    const e = toCommandError(err);
    report(
      kind,
      e.code === "lsp_unavailable"
        ? { state: "unavailable", message: e.message }
        : e.code === "not_trusted"
          ? { state: "restricted" }
          : { state: "failed", message: e.message },
    );
    return;
  }

  const client = new MonacoLanguageClient({
    name: NAME[kind],
    clientOptions: {
      documentSelector: [{ language: LANGUAGE[kind] }],
      workspaceFolder: { index: 0, name: "project", uri: vscode.Uri.file(root) },
      // restarts are handled above (with limits), not by the client library
      errorHandler: {
        error: () => ({ action: ErrorAction.Continue }),
        closed: () => ({ action: CloseAction.DoNotRestart }),
      },
    },
    messageTransports: { reader, writer: new InvokeWriter(session) },
  });
  running.set(kind, { session, client, stopping: false });
  try {
    await client.start();
    report(kind, { state: "running" });
  } catch (err: unknown) {
    report(kind, { state: "failed", message: err instanceof Error ? err.message : String(err) });
  }
}

/** Stops all clients (shutdown/exit to the servers), then the processes in the core. */
export async function stopAll(): Promise<void> {
  const all = [...running.entries()];
  running.clear();
  starts.clear();
  await Promise.all(
    all.map(async ([, r]) => {
      r.stopping = true;
      try {
        await r.client.stop(2000);
      } catch {
        // server may already be gone
      }
      await lspStop(r.session).catch(() => undefined);
    }),
  );
}
