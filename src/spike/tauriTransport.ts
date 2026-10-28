// SPIKE (M1, DESIGN.md §14 #4) – throwaway code, not for main.
// MessageReader/MessageWriter for vscode-languageclient on top of a Tauri channel.
import { Channel, invoke } from "@tauri-apps/api/core";
import {
  AbstractMessageReader,
  AbstractMessageWriter,
  type DataCallback,
  type Disposable,
  type Message,
  type MessageReader,
  type MessageWriter,
} from "vscode-languageserver-protocol/browser";

type LspEvent = { type: "message"; payload: Message } | { type: "exited"; code: number | null };

export const stats = { received: 0, sent: 0, firstMessageMs: 0, startedAt: 0 };

class ChannelReader extends AbstractMessageReader implements MessageReader {
  private callback: DataCallback | null = null;
  private buffer: Message[] = [];

  push(msg: Message): void {
    stats.received++;
    if (this.callback) this.callback(msg);
    else this.buffer.push(msg);
  }

  close(): void {
    this.fireClose();
  }

  listen(callback: DataCallback): Disposable {
    this.callback = callback;
    for (const m of this.buffer.splice(0)) callback(m);
    return {
      dispose: () => {
        this.callback = null;
      },
    };
  }
}

class InvokeWriter extends AbstractMessageWriter implements MessageWriter {
  constructor(private readonly sessionId: number) {
    super();
  }

  async write(msg: Message): Promise<void> {
    stats.sent++;
    try {
      await invoke("spike_lsp_send", { id: this.sessionId, message: msg });
    } catch (err: unknown) {
      this.fireError(err, msg, stats.sent);
      throw err;
    }
  }

  end(): void {}
}

export interface TauriTransport {
  sessionId: number;
  reader: MessageReader;
  writer: MessageWriter;
}

export async function startTerraformLs(root: string): Promise<TauriTransport> {
  const reader = new ChannelReader();
  const channel = new Channel<LspEvent>((ev) => {
    if (ev.type === "message") {
      if (stats.received === 0) stats.firstMessageMs = performance.now() - stats.startedAt;
      reader.push(ev.payload);
    } else {
      reader.close();
    }
  });
  stats.startedAt = performance.now();
  const sessionId = await invoke<number>("spike_lsp_start", { root, onEvent: channel });
  return { sessionId, reader, writer: new InvokeWriter(sessionId) };
}

export function stopTerraformLs(sessionId: number): Promise<number | null> {
  return invoke<number | null>("spike_lsp_stop", { id: sessionId });
}
