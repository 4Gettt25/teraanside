import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createBatcher } from "./batch";

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("createBatcher", () => {
  it("delivers queued items once per interval", () => {
    const batches: number[][] = [];
    const b = createBatcher<number>((items) => batches.push(items), 50);
    b.push(1);
    b.push(2);
    vi.advanceTimersByTime(49);
    expect(batches).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(batches).toEqual([[1, 2]]);
    b.push(3);
    vi.advanceTimersByTime(50);
    expect(batches).toEqual([[1, 2], [3]]);
  });

  it("flush delivers immediately and cancels the timer", () => {
    const batches: number[][] = [];
    const b = createBatcher<number>((items) => batches.push(items), 50);
    b.push(1);
    b.flush();
    vi.advanceTimersByTime(100);
    expect(batches).toEqual([[1]]);
  });

  it("does not deliver empty batches", () => {
    const deliver = vi.fn();
    const b = createBatcher<number>(deliver, 50);
    b.flush();
    expect(deliver).not.toHaveBeenCalled();
  });

  it("dispose drops queued items", () => {
    const deliver = vi.fn();
    const b = createBatcher<number>(deliver, 50);
    b.push(1);
    b.dispose();
    vi.advanceTimersByTime(100);
    expect(deliver).not.toHaveBeenCalled();
  });
});
