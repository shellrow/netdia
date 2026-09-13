import type { EventCallback, EventName, UnlistenFn } from "@tauri-apps/api/event";

type Register = <T>(event: EventName, handler: EventCallback<T>) => Promise<UnlistenFn>;

/** Owns asynchronous registrations, including those that resolve after disposal. */
export function createListenerScope(register: Register) {
  let disposed = false;
  const listeners = new Set<UnlistenFn>();

  function dispose() {
    disposed = true;
    for (const unlisten of listeners) {
      try {
        void Promise.resolve(unlisten()).catch((error) => {
          console.error("Failed to remove diagnostic listener", error);
        });
      } catch (error) {
        console.error("Failed to remove diagnostic listener", error);
      }
    }
    listeners.clear();
  }

  async function listen<T>(event: EventName, handler: EventCallback<T>): Promise<void> {
    if (disposed) throw new Error("Diagnostic listeners are unavailable");
    try {
      const unlisten = await register<T>(event, (payload) => {
        if (!disposed) handler(payload);
      });
      if (disposed) {
        await unlisten();
        throw new Error("Diagnostic listeners were disposed during registration");
      }
      listeners.add(unlisten);
    } catch (error) {
      dispose();
      throw error;
    }
  }

  return { listen, dispose };
}
