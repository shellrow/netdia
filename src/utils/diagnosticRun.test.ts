import { describe, expect, it, vi } from "vitest";
import { createDiagnosticRun } from "./diagnosticRun";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("diagnostic run ownership", () => {
  it("waits for reservation before canceling and never admits canceled events", async () => {
    const prepare = deferred();
    const calls: string[] = [];
    const invoke = vi.fn(async <T>(command: string) => {
      calls.push(command);
      if (command === "prepare_operation") await prepare.promise;
      return undefined as T;
    });
    const identity = { value: null as string | null };
    const run = createDiagnosticRun("ping", identity, invoke, () => "first");
    const begin = run.begin();
    const rejected = expect(begin).rejects.toThrow("cancelled");
    const cancel = run.cancel();
    expect(calls).toEqual(["prepare_operation"]);
    expect(run.accepts("first")).toBe(false);
    prepare.resolve();
    await Promise.all([rejected, cancel]);
    expect(calls.slice(1).every((command) => command === "cancel_operation")).toBe(true);
    expect(identity.value).toBeNull();
  });

  it("rejects unowned, missing, completed, and predecessor events", async () => {
    const identity = { value: null as string | null };
    const ids = ["first", "second"];
    const invoke = async <T>() => undefined as T;
    const run = createDiagnosticRun("ping", identity, invoke, () => ids.shift()!);
    expect(run.accepts("foreign")).toBe(false);
    expect(run.accepts(undefined)).toBe(false);
    await run.begin();
    expect(run.accepts("first")).toBe(true);
    run.finish();
    expect(run.accepts("first")).toBe(false);
    await run.begin();
    expect(run.accepts("first")).toBe(false);
    expect(run.accepts("second")).toBe(true);
    await run.cancel("first");
    expect(run.accepts("second")).toBe(true);
    expect(run.isLatest("first")).toBe(false);
    await run.dispose();
    expect(run.accepts("second")).toBe(false);
    await expect(run.begin()).rejects.toThrow("closed");
  });

  it("cleans a reservation completed after page disposal", async () => {
    const prepare = deferred();
    const invoke = vi.fn(async <T>(command: string) => {
      if (command === "prepare_operation") await prepare.promise;
      return undefined as T;
    });
    const run = createDiagnosticRun("latency", { value: null }, invoke, () => "first");
    const begin = run.begin();
    const rejected = expect(begin).rejects.toThrow("cancelled");
    const disposal = run.dispose();
    prepare.resolve();
    await Promise.all([rejected, disposal]);
    expect(invoke).toHaveBeenCalledWith("cancel_operation", { kind: "latency", runId: "first" });
  });

  it("keeps cancellation retryable after an IPC failure", async () => {
    let attempts = 0;
    const invoke = async <T>(command: string) => {
      if (command === "cancel_operation" && ++attempts === 1) throw new Error("IPC unavailable");
      return undefined as T;
    };
    const run = createDiagnosticRun("ping", { value: null }, invoke, () => "first");
    await run.begin();
    await expect(run.cancel()).rejects.toThrow("IPC unavailable");
    expect(run.accepts("first")).toBe(false);
    await run.cancel();
    expect(attempts).toBe(2);
  });
});
