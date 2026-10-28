export interface Batcher<T> {
  /** Queues an item; it is delivered with the next flush. */
  push(item: T): void;
  /** Delivers all queued items immediately. */
  flush(): void;
  /** Drops queued items and cancels the pending flush. */
  dispose(): void;
}

/**
 * Collects high-frequency stream events and delivers them in batches at most
 * every `intervalMs`, so the UI re-renders once per batch instead of per event.
 */
export function createBatcher<T>(deliver: (items: T[]) => void, intervalMs = 50): Batcher<T> {
  let queue: T[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  function cancelTimer(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function flush(): void {
    cancelTimer();
    if (queue.length === 0) {
      return;
    }
    const items = queue;
    queue = [];
    deliver(items);
  }

  return {
    push(item: T): void {
      queue.push(item);
      timer ??= setTimeout(flush, intervalMs);
    },
    flush,
    dispose(): void {
      cancelTimer();
      queue = [];
    },
  };
}
