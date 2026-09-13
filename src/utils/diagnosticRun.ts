type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
type Identity = { value: string | null };

/** Reserve before work; cancellation waits for the reservation acknowledgment. */
export function createDiagnosticRun(
  kind: string,
  identity: Identity,
  invoke: Invoke,
  makeId: () => string = newRunId,
) {
  let current: { id: string; prepared: Promise<unknown>; canceled: boolean } | null = null;
  let disposed = false;
  let latestId: string | null = null;

  async function begin(): Promise<string> {
    if (disposed) throw new Error("Diagnostic page was closed");
    const id = makeId();
    identity.value = id;
    latestId = id;
    const record = { id, prepared: invoke("prepare_operation", { kind, runId: id }), canceled: false };
    current = record;
    try {
      await record.prepared;
    } catch (error) {
      if (current === record) {
        current = null;
        identity.value = null;
      }
      throw error;
    }
    if (disposed || record.canceled || current !== record) {
      // A cancellation may have arrived while the reservation was in flight.
      await invoke("cancel_operation", { kind, runId: id });
      throw new Error("cancelled");
    }
    return id;
  }

  async function cancel(expectedId?: string): Promise<void> {
    const record = current;
    if (!record || (expectedId && record.id !== expectedId)) return;
    record.canceled = true;
    if (current === record) identity.value = null;
    try {
      await record.prepared;
    } catch {
      return;
    }
    await invoke("cancel_operation", { kind, runId: record.id });
    if (current === record) current = null;
  }

  function accepts(runId: unknown): boolean {
    return !disposed && !!current && !current.canceled
      && typeof runId === "string" && identity.value === runId && current.id === runId;
  }

  async function dispose() {
    disposed = true;
    await cancel();
  }

  function finish() { identity.value = null; }
  function isLatest(id: string) { return latestId === id; }
  return { begin, cancel, accepts, dispose, finish, isLatest };
}

/** Generate UUID v4 using Web Crypto, including WebViews without randomUUID. */
function newRunId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return [hex.slice(0, 8), hex.slice(8, 12), hex.slice(12, 16), hex.slice(16, 20), hex.slice(20)].join("-");
}
