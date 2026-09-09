import { describe, expect, it, vi } from "vitest";
import type { EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import { createListenerScope } from "./listenerScope";

describe("diagnostic listener ownership", () => {
  it("removes registrations that finish after disposal", async () => {
    let resolve!: (unlisten: UnlistenFn) => void;
    const register = () => new Promise<UnlistenFn>((done) => { resolve = done; });
    const scope = createListenerScope(register);
    const pending = scope.listen("ping:done", () => {});
    const rejected = expect(pending).rejects.toThrow("disposed");
    scope.dispose();
    const unlisten = vi.fn();
    resolve(unlisten);
    await rejected;
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("rolls back partial setup and blocks callbacks after failure", async () => {
    let callback!: EventCallback<unknown>;
    const unlisten = vi.fn();
    let calls = 0;
    const scope = createListenerScope(async <T>(_event: string, handler: EventCallback<T>) => {
      calls++;
      if (calls === 2) throw new Error("IPC unavailable");
      callback = handler as EventCallback<unknown>;
      return unlisten;
    });
    const handler = vi.fn();
    await scope.listen("ping:progress", handler);
    await expect(scope.listen("ping:done", handler)).rejects.toThrow("IPC unavailable");
    callback({ event: "ping:progress", id: 1, payload: {} });
    expect(handler).not.toHaveBeenCalled();
    expect(unlisten).toHaveBeenCalledTimes(1);
    scope.dispose();
    expect(unlisten).toHaveBeenCalledTimes(1);
    await expect(scope.listen("ping:error", handler)).rejects.toThrow("unavailable");
    expect(calls).toBe(2);
  });
});
